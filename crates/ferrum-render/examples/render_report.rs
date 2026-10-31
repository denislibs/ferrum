//! Контрольный бенчмарк сцены и рендера:
//! `cargo run --release -p ferrum-render --example render_report`.
//! Сверяет замеры с целями этапа 2 и печатает таблицу PASS/FAIL (код выхода 1 при провале).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ferrum_doc::container::{decode, encode};
use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{Camera, CpuBackend, NoImages, Overlay, Renderer, TextEngine, LABEL_SIZE};
use ferrum_scene::Scene;
use kurbo::{Point, Size};

/// Считает живые байты в куче — для памяти кэша отрисовки.
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn heap_mb() -> f64 {
    LIVE.load(Ordering::Relaxed) as f64 / 1_048_576.0
}

const NODES: usize = 100_000;
/// Прогонов в медиане для строк `Scene::apply`.
const APPLY_RUNS: usize = 5;
/// Предел кадров прогрева: больше — кэш не сходится, это провал.
const WARMUP_CAP: usize = 2000;
const VIEWPORT: Size = Size::new(1920.0, 1080.0);
/// Фреймов с усечёнными подписями и кадров зума в замере подписей.
const LABEL_FRAMES: usize = 60;
const LABEL_ZOOM_FRAMES: usize = 60;

/// Медиана нескольких замеров: один прогон на ноутбуке шумит на десятки процентов.
fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn ms(d: Duration) -> String {
    format!("{:.3} мс", d.as_secs_f64() * 1000.0)
}

struct Row {
    metric: String,
    value: String,
    limit: &'static str,
    pass: Option<bool>,
}

struct Report(Vec<Row>);

impl Report {
    fn gate(&mut self, metric: &str, d: Duration, limit_ms: f64, limit: &'static str) {
        self.0.push(Row {
            metric: metric.to_string(),
            value: ms(d),
            limit,
            pass: Some(d.as_secs_f64() * 1000.0 <= limit_ms),
        });
    }

    fn fail(&mut self, metric: &str, value: &str) {
        self.0.push(Row {
            metric: metric.to_string(),
            value: value.to_string(),
            limit: "прогрев ≤ 2000 кадров",
            pass: Some(false),
        });
    }

    fn info(&mut self, metric: &str, value: String) {
        self.0.push(Row {
            metric: metric.to_string(),
            value,
            limit: "—",
            pass: None,
        });
    }
}

/// Применяет транзакцию и замеряет `Scene::apply` её изменений, затем отмены.
/// Медиана `APPLY_RUNS` прогонов: подготовка (транзакция) каждый раз вне замера,
/// отмена возвращает документ и сцену в исходное состояние.
fn apply_and_undo(
    doc: &mut Document,
    page: PageId,
    scene: &mut Scene,
    edit: impl Fn(&ferrum_doc::Tx) -> ferrum_doc::DocResult<()>,
) -> (Duration, Duration) {
    let mut forward = Vec::new();
    let mut back = Vec::new();
    for _ in 0..APPLY_RUNS {
        doc.transact(page, vec![], |p| edit(p).map(|_| vec![]))
            .unwrap();
        let changes = doc.take_changes(page);
        let t = Instant::now();
        scene.apply(&changes, doc.page(page).unwrap());
        forward.push(t.elapsed());
        doc.undo().unwrap();
        let changes = doc.take_changes(page);
        let t = Instant::now();
        scene.apply(&changes, doc.page(page).unwrap());
        back.push(t.elapsed());
    }
    (median(forward), median(back))
}

/// Кадры, пока растровый кэш не прогреется (`raster_builds == 0`). `None` — предел
/// кадров исчерпан, кэш не прогрелся (провал).
fn warm_up(
    renderer: &mut Renderer,
    backend: &mut CpuBackend,
    scene: &Scene,
    camera: &Camera,
) -> Option<Vec<Duration>> {
    let (w, h) = camera.device_size();
    let mut times = Vec::new();
    for _ in 0..WARMUP_CAP {
        backend.reset(w, h);
        let t = Instant::now();
        let s = renderer.draw_scene(backend, scene, camera, &NoImages);
        times.push(t.elapsed());
        if s.raster_builds == 0 {
            return Some(times);
        }
    }
    None
}

/// Кадр после прогрева кэша: медиана нескольких сборок в бэкенд без растеризации.
fn frame(
    renderer: &mut Renderer,
    backend: &mut CpuBackend,
    scene: &Scene,
    camera: &Camera,
) -> (Duration, ferrum_render::FrameStats) {
    let (w, h) = camera.device_size();
    let mut stats = Default::default();
    let runs = (0..7)
        .map(|_| {
            backend.reset(w, h);
            let t = Instant::now();
            stats = renderer.draw_scene(backend, scene, camera, &NoImages);
            t.elapsed()
        })
        .collect();
    (median(runs), stats)
}

/// Вшитый Inter из `apps/web/public/fonts` в общем реестре — как его грузит ядро.
fn inter_fonts() -> ferrum_render::SharedFonts {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/public/fonts");
    let fonts = ferrum_render::Fonts::shared();
    let mut f = fonts.borrow_mut();
    let files = [
        "inter/Inter-Variable.ttf",
        "inter/Inter-Italic-Variable.ttf",
    ];
    let mut faces = Vec::new();
    for rel in files {
        faces.extend(ferrum_text::read_faces(
            &std::fs::read(dir.join(rel)).unwrap(),
            rel,
        ));
    }
    f.add_catalog(ferrum_text::SourceKind::Bundled, ferrum_text::group(faces));
    for rel in files {
        f.load("Inter", rel, std::fs::read(dir.join(rel)).unwrap())
            .unwrap();
    }
    drop(f);
    fonts
}

/// Текст (подсистема 3, этап 1): страница из 10 000 текстов открывается без
/// раскладки (размеры — из документа), раскладка ленивая (при первом рисовании,
/// в пределах времени кадра), панорамирование по разложенным — 60 fps.
fn text_section(report: &mut Report) {
    const TEXTS: usize = 10_000;
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.transact(page, vec![], |p| {
        ferrum_doc::fixture::fill_texts(p, TEXTS).map(|_| vec![])
    })
    .unwrap();
    let bytes = encode(&doc).unwrap();
    let open = median(
        (0..3)
            .map(|_| {
                let t = Instant::now();
                let mut d = decode(&bytes).unwrap().document;
                let scene = Scene::from_page(d.page(page).unwrap());
                std::hint::black_box(scene.len());
                t.elapsed()
            })
            .collect(),
    );
    report.gate(
        "Текст: открытие страницы из 10 000 текстов (файл → сцена)",
        open,
        1000.0,
        "≤ 1000 мс (wasm ≤ 2 с)",
    );
    let mut d = decode(&bytes).unwrap().document;
    let scene = Scene::from_page(d.page(page).unwrap());
    let mut renderer = Renderer::new();
    renderer.set_fonts(inter_fonts());
    let mut camera = Camera::new(VIEWPORT, 1.0);
    camera.set_center(Point::new(720.0, 512.0));
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    // Прогрев: ленивая раскладка видимых текстов догоняет за несколько кадров.
    let mut warm = 0;
    let mut first = Duration::ZERO;
    for i in 0..WARMUP_CAP {
        backend.reset(w, h);
        let t = Instant::now();
        let s = renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
        if i == 0 {
            first = t.elapsed();
        }
        warm = i + 1;
        if !s.needs_another_frame {
            break;
        }
    }
    report.gate(
        "Текст: первый кадр (раскладка в пределах кадра)",
        first,
        16.7,
        "≤ 16.7 мс",
    );
    report.info(
        "Текст: кадров до полной раскладки видимого",
        warm.to_string(),
    );
    // Панорамирование по странице: каждый кадр — сдвиг на 40 px; новые тексты
    // раскладываются на лету.
    let mut times = Vec::new();
    for i in 0..240 {
        camera.set_center(Point::new(
            720.0 + i as f64 * 40.0,
            512.0 + (i % 60) as f64 * 10.0,
        ));
        backend.reset(w, h);
        let t = Instant::now();
        renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
        times.push(t.elapsed());
    }
    times.sort();
    let p95 = times[times.len() * 95 / 100];
    report.gate(
        "Текст: панорамирование, p95 кадра (зум 100 %)",
        p95,
        16.7,
        "≤ 16.7 мс",
    );
    // Вся страница издалека: полосы LOD и растры фреймов.
    camera.set_zoom(0.05);
    camera.set_center(Point::new(40_000.0, 1_500.0));
    let _ = warm_up(&mut renderer, &mut backend, &scene, &camera);
    let (whole, stats) = frame(&mut renderer, &mut backend, &scene, &camera);
    report.info(
        "Текст: вся страница — растров / глифами / полосами",
        format!(
            "{} / {} / {}",
            stats.rasters, stats.text_glyphs, stats.text_greeked
        ),
    );
    report.gate(
        "Текст: вся страница с LOD, сборка кадра",
        whole,
        8.0,
        "≤ 8 мс",
    );
}

/// Векторы (подсистема 3, этап 3): сеть из 100 000 точек с областью и обводкой
/// рисуется в бюджете кадра; правка одной точки — новый контур и кадр.
fn vector_section(report: &mut Report) {
    use std::sync::Arc;
    const POINTS: usize = 100_000;
    let net = ferrum_doc::fixture::wavy_network(POINTS);
    let b = ferrum_scene::vector::network_bounds(&net).unwrap();
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Vector)?;
            tx.set_props(
                id,
                &PropsPatch {
                    w: Some(b.width()),
                    h: Some(b.height()),
                    fills: Some(vec![ferrum_doc::Fill::solid(ferrum_doc::Color::SHAPE_GRAY)]),
                    vector: Some(Arc::new(net.clone())),
                    ..Default::default()
                },
            )?;
            Ok(vec![id])
        })
        .unwrap()[0];
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let mut camera = Camera::new(VIEWPORT, 1.0);
    camera.fit(kurbo::Rect::new(0.0, 0.0, b.width(), b.height()), 48.0);
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    let mut renderer = Renderer::new();
    let t = Instant::now();
    renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
    report.gate(
        "Вектор 100 000 точек: первый кадр (контуры и обводка строятся)",
        t.elapsed(),
        16.7,
        "≤ 16.7 мс",
    );
    let (steady, _) = frame(&mut renderer, &mut backend, &scene, &camera);
    report.gate(
        "Вектор 100 000 точек: кадр (контуры в кэше)",
        steady,
        16.7,
        "≤ 16.7 мс",
    );
    // Правка одной точки: документ, сцена и кадр с новым контуром.
    let mut edited = net.clone();
    edited.points[POINTS / 3].x += 15.0;
    let edited = Arc::new(edited);
    let t = Instant::now();
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                vector: Some(edited.clone()),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    let changes = doc.take_changes(page);
    scene.apply(&changes, doc.page(page).unwrap());
    let (w, h) = camera.device_size();
    backend.reset(w, h);
    renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
    report.gate(
        "Вектор 100 000 точек: правка точки — запись, сцена и кадр",
        t.elapsed(),
        16.7,
        "≤ 16.7 мс",
    );
    // Режим правки: сеть тонкой линией, выделенная точка, ручки.
    let overlay = Overlay {
        vector_edit: Some(ferrum_render::VectorEditOverlay {
            transform: scene.node(id).unwrap().world
                * ferrum_scene::vector::fit(&scene.node(id).unwrap().props),
            net: edited.clone(),
            points: vec![POINTS as u32 / 3],
            segments: vec![],
            handles: vec![],
            selected_handles: vec![],
            rubber: None,
        }),
        ..Default::default()
    };
    let runs = (0..7)
        .map(|_| {
            let t = Instant::now();
            renderer.render(&mut backend, &scene, &camera, &overlay, &NoImages, None);
            t.elapsed()
        })
        .collect();
    report.gate(
        "Вектор 100 000 точек: кадр правки (сеть и точки в оверлее)",
        median(runs),
        16.7,
        "≤ 16.7 мс",
    );
}

/// Вектор с настоящими кривыми (каждый из 100 000 сегментов выгнут): упрощение
/// под масштаб не раздувает контуры на приближении, точные контуры строятся один
/// раз и дальше не перестраиваются.
fn curvy_vector_section(report: &mut Report) {
    use std::sync::Arc;
    const POINTS: usize = 100_000;
    const RADIUS: f64 = 2000.0;
    let net = ferrum_doc::fixture::curvy_network(POINTS, RADIUS);
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut props = ferrum_doc::Props::defaults(NodeKind::Vector);
    let b = ferrum_scene::vector::network_bounds(&net).unwrap();
    (props.w, props.h) = (b.width(), b.height());
    props.fills = vec![ferrum_doc::Fill::solid(ferrum_doc::Color::SHAPE_GRAY)];
    props.strokes = vec![ferrum_doc::Stroke {
        color: ferrum_doc::Color::BLACK,
        width: 2.0,
        align: ferrum_doc::StrokeAlign::Center,
    }];
    props.vector = Some(Arc::new(net));
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_with_props(None, None, &props)?])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Точный контур обводки — для сравнения с перестройкой на приближении.
    let shape = ferrum_scene::vector::network_shape(&props);
    let t = Instant::now();
    let exact =
        ferrum_scene::vector::stroke_outline(&shape, &props.strokes[0], &props.stroke_style);
    let exact_ms = t.elapsed();
    report.info(
        "Кривой вектор 100 000 точек: точный контур обводки (для сравнения)",
        format!("{} ({} элементов)", ms(exact_ms), exact.elements().len()),
    );
    let mut camera = Camera::new(VIEWPORT, 1.0);
    camera.fit(b, 48.0);
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    let mut renderer = Renderer::new();
    let t = Instant::now();
    renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
    report.gate(
        "Кривой вектор 100 000 точек: первый кадр целиком",
        t.elapsed(),
        16.7,
        "≤ 16.7 мс",
    );
    let (steady, _) = frame(&mut renderer, &mut backend, &scene, &camera);
    report.gate(
        "Кривой вектор 100 000 точек: кадр целиком (контуры в кэше)",
        steady,
        16.7,
        "≤ 16.7 мс",
    );
    // Приближение до предела на краю контура: упрощать нечего, контуры точные.
    camera.set_center(Point::new(2.0 * RADIUS, RADIUS));
    let mut rebuild = Duration::ZERO;
    let mut zoom = camera.zoom();
    while zoom < ferrum_render::MAX_ZOOM {
        zoom = (zoom * 2.0).min(ferrum_render::MAX_ZOOM);
        camera.set_zoom(zoom);
        let t = Instant::now();
        renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
        rebuild = rebuild.max(t.elapsed());
    }
    report.gate(
        "Кривой вектор 100 000 точек: худший кадр приближения до ×256 (перестройка контуров)",
        rebuild,
        exact_ms.as_secs_f64() * 1000.0 * 1.5 + 16.7,
        "≤ 1.5 × точного контура + кадр",
    );
    let (steady, _) = frame(&mut renderer, &mut backend, &scene, &camera);
    report.gate(
        "Кривой вектор 100 000 точек: кадр при ×256 (контуры в кэше)",
        steady,
        16.7,
        "≤ 16.7 мс",
    );
    let built = ferrum_scene::vector::simplify(&shape, 0.25 / 256.0);
    report.0.push(Row {
        metric: "Кривой вектор 100 000 точек: элементов пути при ×256 / точного".into(),
        value: format!(
            "{} / {}",
            built.stroke.elements().len(),
            shape.stroke.elements().len()
        ),
        limit: "не больше точного",
        pass: Some(built.stroke.elements().len() <= shape.stroke.elements().len()),
    });
}

/// Булева группа из 1000 перекрывающихся кругов (спека подсистемы 3, раздел 10:
/// булевы операции на 1000 контуров ≤ 50 мс): каждый круг задевает до восьми
/// соседей — тысячи пересечений.
pub fn circles_boolean(doc: &mut Document, page: PageId, op: ferrum_doc::BooleanOp) -> NodeId {
    doc.transact(page, vec![], |tx| {
        let b = tx.create_node(None, None, NodeKind::Boolean)?;
        tx.set_props(
            b,
            &PropsPatch {
                boolean_op: Some(op),
                ..Default::default()
            },
        )?;
        for i in 0..1000 {
            let e = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
            tx.set_props(
                e,
                &PropsPatch {
                    x: Some((i % 40) as f64 * 15.0),
                    y: Some((i / 40) as f64 * 15.0),
                    w: Some(20.0),
                    h: Some(20.0),
                    ..Default::default()
                },
            )?;
        }
        Ok(vec![b])
    })
    .unwrap()[0]
}

fn boolean_section(report: &mut Report) {
    use ferrum_doc::BooleanOp;
    use ferrum_scene::boolean::{combine, flatten_path, normalize, BOOLEAN_TOLERANCE};
    // Операция сама по себе: формы кругов уже раздроблены.
    let circles: Vec<_> = (0..1000)
        .map(|i| {
            let c = kurbo::Circle::new(
                ((i % 40) as f64 * 15.0 + 10.0, (i / 40) as f64 * 15.0 + 10.0),
                10.0,
            );
            normalize(
                flatten_path(&kurbo::Shape::to_path(&c, 1e-3), BOOLEAN_TOLERANCE),
                ferrum_doc::FillRule::NonZero,
            )
        })
        .collect();
    for op in BooleanOp::ALL {
        let runs = (0..5)
            .map(|_| {
                let t = Instant::now();
                std::hint::black_box(combine(op, &circles));
                t.elapsed()
            })
            .collect();
        report.gate(
            &format!("Булева операция «{}»: 1000 кругов", op.name()),
            median(runs),
            50.0,
            "≤ 50 мс",
        );
    }
    // Пересечение, у которого общее у всех не пусто: 1000 кругов радиуса 10 со
    // сдвигом по 0,005 — каждый шаг режет почти весь результат.
    let stacked: Vec<_> = (0..1000)
        .map(|i| {
            let c = kurbo::Circle::new((10.0 + 0.005 * i as f64, 10.0), 10.0);
            normalize(
                flatten_path(&kurbo::Shape::to_path(&c, 1e-3), BOOLEAN_TOLERANCE),
                ferrum_doc::FillRule::NonZero,
            )
        })
        .collect();
    let runs = (0..5)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(combine(BooleanOp::Intersect, &stacked));
            t.elapsed()
        })
        .collect();
    report.gate(
        "Булева операция «Пересечение»: 1000 почти совпадающих кругов",
        median(runs),
        50.0,
        "≤ 50 мс",
    );
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let b = circles_boolean(&mut doc, page, BooleanOp::Union);
    let build = median(
        (0..5)
            .map(|_| {
                let t = Instant::now();
                std::hint::black_box(Scene::from_page(doc.page(page).unwrap()));
                t.elapsed()
            })
            .collect(),
    );
    report.gate(
        "Булева группа 1000 кругов: сцена с результатом (дробление и операция)",
        build,
        50.0,
        "≤ 50 мс",
    );
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let bounds = scene.node(b).unwrap().world_bounds.unwrap();
    let mut camera = Camera::new(VIEWPORT, 1.0);
    camera.fit(bounds, 48.0);
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    let mut renderer = Renderer::new();
    renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
    let (steady, _) = frame(&mut renderer, &mut backend, &scene, &camera);
    report.gate(
        "Булева группа 1000 кругов: кадр (результат в кэше)",
        steady,
        16.7,
        "≤ 16.7 мс",
    );
    // Правка одного ребёнка: запись, сцена (результат заново) и кадр.
    let child = doc.page(page).unwrap().children(Some(b))[500];
    let runs = (0..5)
        .map(|k| {
            let t = Instant::now();
            doc.transact(page, vec![], |tx| {
                tx.set_props(
                    child,
                    &PropsPatch {
                        x: Some(3.0 * k as f64),
                        ..Default::default()
                    },
                )
                .map(|_| vec![])
            })
            .unwrap();
            let changes = doc.take_changes(page);
            scene.apply(&changes, doc.page(page).unwrap());
            backend.reset(w, h);
            renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
            t.elapsed()
        })
        .collect();
    report.gate(
        "Булева группа 1000 кругов: правка ребёнка — запись, сцена и кадр",
        median(runs),
        50.0,
        "≤ 50 мс (кадр жеста — editor_report)",
    );
}

fn main() {
    let mut report = Report(Vec::new());

    // Документ из 100 000 узлов (фреймы по 100: фрейм + 99 фигур), сохранённый в файл.
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids: Vec<NodeId> = doc.transact(page, vec![], |p| fill(p, NODES)).unwrap();
    let bytes = encode(&doc).unwrap();
    let mut doc = decode(&bytes).unwrap().document;
    let rows = doc.cached_scene(page).unwrap().to_vec();
    assert_eq!(rows.len(), NODES);

    let build = median(
        (0..5)
            .map(|_| {
                let t = Instant::now();
                std::hint::black_box(Scene::from_rows(&rows));
                t.elapsed()
            })
            .collect(),
    );
    report.gate(
        "Построение сцены из кэша сцены (100 000)",
        build,
        100.0,
        "≤ 100 мс",
    );
    let mut scene = Scene::from_rows(&rows);
    // Фоновая загрузка Loro (этап 1) до правок: она не входит в замеры сцены.
    assert_eq!(doc.page(page).unwrap().walk().len(), NODES);

    // 1000 фигур, разбросанных по всей странице (по одной почти из каждого фрейма).
    // Фреймы — каждый сотый узел генератора; их сдвиг двигает и 99 детей.
    let spread: Vec<NodeId> = ids
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 100 != 0)
        .map(|(_, id)| *id)
        .step_by(99)
        .take(1000)
        .collect();
    assert_eq!(spread.len(), 1000);
    let frames: Vec<NodeId> = ids.iter().step_by(100).take(10).copied().collect();
    let (props, undo_props) = apply_and_undo(&mut doc, page, &mut scene, |p| {
        for (i, id) in spread.iter().enumerate() {
            p.set_props(
                *id,
                &PropsPatch {
                    x: Some(i as f64),
                    y: Some(7.0),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    });
    report.gate("Scene::apply: сдвиг 1000 узлов", props, 5.0, "≤ 5 мс");
    report.gate(
        "Scene::apply: undo сдвига 1000 узлов",
        undo_props,
        5.0,
        "≤ 5 мс",
    );

    let (frames_moved, undo_frames) = apply_and_undo(&mut doc, page, &mut scene, |p| {
        for id in &frames {
            p.set_props(
                *id,
                &PropsPatch {
                    x: Some(-5000.0),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    });
    report.gate(
        "Scene::apply: сдвиг 10 фреймов (1000 узлов с детьми)",
        frames_moved,
        5.0,
        "≤ 5 мс",
    );
    report.gate(
        "Scene::apply: undo сдвига 10 фреймов",
        undo_frames,
        5.0,
        "≤ 5 мс",
    );

    let (group, undo_group) = apply_and_undo(&mut doc, page, &mut scene, |p| {
        let frame = p.create_node(None, None, NodeKind::Frame)?;
        p.move_nodes(&spread, Some(frame), 0)
    });
    report.gate("Scene::apply: 1000 узлов во фрейм", group, 5.0, "≤ 5 мс");
    report.gate(
        "Scene::apply: undo переноса 1000 узлов",
        undo_group,
        5.0,
        "≤ 5 мс",
    );

    let (delete, undo_delete) =
        apply_and_undo(&mut doc, page, &mut scene, |p| p.delete_nodes(&spread));
    report.gate("Scene::apply: удаление 1000 узлов", delete, 5.0, "≤ 5 мс");
    report.gate(
        "Scene::apply: undo удаления 1000 узлов",
        undo_delete,
        5.0,
        "≤ 5 мс",
    );

    // Превью перетаскивания 1000 узлов: каждый кадр жеста — новое смещение.
    let preview = median(
        (0..7)
            .map(|k| {
                let patches: Vec<(NodeId, PropsPatch)> = spread
                    .iter()
                    .map(|id| {
                        (
                            *id,
                            PropsPatch {
                                x: Some(k as f64 * 3.0),
                                ..Default::default()
                            },
                        )
                    })
                    .collect();
                let t = Instant::now();
                scene.set_preview(&patches);
                t.elapsed()
            })
            .collect(),
    );
    scene.clear_preview();
    report.gate(
        "Превью перетаскивания 1000 узлов (кадр)",
        preview,
        5.0,
        "≤ 5 мс",
    );
    assert_eq!(
        ferrum_scene::testing::snapshot(&scene),
        ferrum_scene::testing::snapshot(&Scene::from_page(doc.page(page).unwrap())),
        "сцена после правок и отмен совпадает с построенной заново"
    );

    // Перемещение одного узла среди 100 000 соседей: плоская страница из корневых фигур.
    {
        let mut flat = Document::new();
        let fpage = flat.pages()[0].0;
        let fids: Vec<NodeId> = flat
            .transact(fpage, vec![], |p| {
                (0..NODES)
                    .map(|i| {
                        let id = p.create_node(None, None, NodeKind::Rect)?;
                        p.set_props(
                            id,
                            &PropsPatch {
                                x: Some((i % 400) as f64 * 30.0),
                                y: Some((i / 400) as f64 * 30.0),
                                w: Some(20.0),
                                h: Some(20.0),
                                ..Default::default()
                            },
                        )?;
                        Ok(id)
                    })
                    .collect()
            })
            .unwrap();
        // Изменения создания уже учтены сценой, построенной по странице.
        let _ = flat.take_changes(fpage);
        let mut flat_scene = Scene::from_page(flat.page(fpage).unwrap());
        let mover = fids[NODES / 2];
        let (mv, undo_mv) = apply_and_undo(&mut flat, fpage, &mut flat_scene, |p| {
            p.move_node(mover, None, 0)
        });
        report.gate(
            "Перемещение одного узла среди 100 000 соседей",
            mv,
            5.0,
            "≤ 5 мс",
        );
        report.gate(
            "Перемещение одного узла среди 100 000 соседей: undo",
            undo_mv,
            5.0,
            "≤ 5 мс",
        );
    }

    // Правка внутри группы без обрезки с 10 000 детей.
    {
        let mut gdoc = Document::new();
        let gpage = gdoc.pages()[0].0;
        let mut out = None;
        gdoc.transact(gpage, vec![], |p| {
            let g = p.create_node(None, None, NodeKind::Group)?;
            let kids = (0..10_000)
                .map(|i| {
                    let id = p.create_node(Some(g), None, NodeKind::Rect)?;
                    p.set_props(
                        id,
                        &PropsPatch {
                            x: Some((i % 100) as f64 * 30.0),
                            y: Some((i / 100) as f64 * 30.0),
                            w: Some(20.0),
                            h: Some(20.0),
                            ..Default::default()
                        },
                    )?;
                    Ok(id)
                })
                .collect::<ferrum_doc::DocResult<Vec<NodeId>>>()?;
            out = Some(kids);
            Ok(vec![])
        })
        .unwrap();
        let kids = out.unwrap();
        let _ = gdoc.take_changes(gpage);
        let mut gscene = Scene::from_page(gdoc.page(gpage).unwrap());
        let kid = kids[5000];
        let (edit, undo_edit) = apply_and_undo(&mut gdoc, gpage, &mut gscene, |p| {
            p.set_props(
                kid,
                &PropsPatch {
                    x: Some(-100.0),
                    w: Some(33.0),
                    ..Default::default()
                },
            )
        });
        report.gate(
            "Правка внутри группы со множеством детей (10 000)",
            edit,
            5.0,
            "≤ 5 мс",
        );
        report.gate(
            "Правка внутри группы со множеством детей: undo",
            undo_edit,
            5.0,
            "≤ 5 мс",
        );
    }

    // Hit-test: точки по всей странице, допуск 4 экранных пикселя при зуме 1.
    let bounds = scene.bounds().unwrap();
    let points: Vec<Point> = (0..10_000u64)
        .map(|i| {
            let fx = (i.wrapping_mul(7919) % 10_007) as f64 / 10_007.0;
            let fy = (i.wrapping_mul(104_729) % 10_009) as f64 / 10_009.0;
            Point::new(
                bounds.x0 + fx * bounds.width(),
                bounds.y0 + fy * bounds.height(),
            )
        })
        .collect();
    let t = Instant::now();
    let hits = points
        .iter()
        .filter(|p| scene.hit_test(**p, 4.0).is_some())
        .count();
    let hit = t.elapsed() / points.len() as u32;
    assert!(hits > points.len() / 2, "точки попадают в узлы: {hits}");
    report.gate("Hit-test (среднее)", hit, 0.1, "≤ 0.1 мс");

    let mut renderer = Renderer::new();

    // Типичный вьюпорт: около 2000 видимых узлов.
    let mut camera = Camera::new(VIEWPORT, 1.0);
    camera.set_zoom(0.25);
    camera.set_center(Point::new(3500.0, 2000.0));
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    let before = heap_mb();
    let raster_before = renderer.raster_cache_bytes();
    let t = Instant::now();
    renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
    let cold = t.elapsed();
    let (typical, stats) = frame(&mut renderer, &mut backend, &scene, &camera);
    let visible = scene.query_rect(camera.visible_world_rect());
    let t = Instant::now();
    let query = scene.query_rect(camera.visible_world_rect()).len();
    let query_time = t.elapsed();
    assert_eq!(query, visible.len());
    report.info(
        "Типичный вьюпорт: видимых узлов (R-tree / нарисовано)",
        format!("{} / {}", visible.len(), stats.drawn + stats.lod_rects),
    );
    report.info("Типичный вьюпорт: запрос R-tree", ms(query_time));
    report.info("Типичный вьюпорт: первый кадр (списки строятся)", ms(cold));
    report.gate(
        "Типичный вьюпорт: отсечение + списки в бэкенд",
        typical,
        4.0,
        "≤ 4 мс",
    );
    let t = Instant::now();
    std::hint::black_box(backend.render());
    report.info(
        "Типичный вьюпорт: растеризация CPU 1920×1080",
        ms(t.elapsed()),
    );

    // Вся страница при сильном отдалении: LOD и растровый кэш фреймов. Кэш
    // строится по `RASTER_BUILD_TIME_PER_FRAME` за кадр (не больше
    // `RASTER_BUILDS_PER_FRAME` растров); замер — после прогрева.
    camera.fit(scene.bounds().unwrap(), 0.0);
    {
        let mut cold_renderer = Renderer::new();
        // Пользователь видит и кадры прогрева, а не только прогретый кэш: худший из
        // них — под той же целью кадра 60 Гц, что и кадр после зума.
        match warm_up(&mut cold_renderer, &mut backend, &scene, &camera) {
            Some(t) => {
                report.info(
                    "Прогрев растрового кэша (холодный рендерер): кадров до прогрева",
                    format!("{}", t.len() - 1),
                );
                report.gate(
                    "Худший кадр прогрева растрового кэша (холодный рендерер)",
                    *t.iter().max().unwrap(),
                    16.0,
                    "≤ 16 мс",
                );
            }
            None => report.fail(
                "Прогрев растрового кэша (холодный рендерер)",
                "не прогрелся",
            ),
        }
    }
    match warm_up(&mut renderer, &mut backend, &scene, &camera) {
        Some(t) => {
            report.info(
                "Вся страница: прогрев кэша (рендерер с прогретыми списками): кадров",
                format!("{}", t.len() - 1),
            );
            report.gate(
                "Вся страница: худший кадр прогрева (рендерер с прогретыми списками)",
                *t.iter().max().unwrap(),
                16.0,
                "≤ 16 мс",
            );
        }
        None => report.fail("Вся страница: прогрев кэша", "не прогрелся"),
    }
    // Растры всей страницы не помещаются в предел памяти (четверть нужного):
    // не поместившиеся фреймы рисуются узлами, кадр полный и устойчивый.
    {
        let limit = renderer.raster_cache_bytes() / 4;
        let mut tight = Renderer::with_raster_cache_bytes(limit);
        let mut worst = Duration::ZERO;
        let mut warm_frames = 0;
        let mut last = Default::default();
        for i in 0..WARMUP_CAP + 10 {
            backend.reset(w, h);
            let t = Instant::now();
            last = tight.draw_scene(&mut backend, &scene, &camera, &NoImages);
            worst = worst.max(t.elapsed());
            if last.needs_another_frame {
                warm_frames = i + 1;
            } else if i >= warm_frames + 10 {
                break;
            }
        }
        let ferrum_render::FrameStats {
            rasters,
            drawn,
            lod_rects,
            needs_another_frame,
            ..
        } = last;
        report.info(
            "Растры не помещаются в память (предел — четверть): кадров прогрева / из растров / узлами / LOD / кадр неполный",
            format!("{warm_frames} / {rasters} / {drawn} / {lod_rects} / {needs_another_frame}"),
        );
        report.gate(
            "Растры не помещаются в память: худший кадр (прогрев и 10 устойчивых)",
            worst,
            16.0,
            "≤ 16 мс",
        );
    }
    let (whole, stats) = frame(&mut renderer, &mut backend, &scene, &camera);
    report.info(
        "Вся страница: зум / из растрового кэша / узлами / LOD-прямоугольников",
        format!(
            "{:.3} / {} / {} / {}",
            camera.zoom(),
            stats.rasters,
            stats.drawn,
            stats.lod_rects
        ),
    );
    report.gate("Вся страница с LOD: сборка кадра", whole, 8.0, "≤ 8 мс");
    let t = Instant::now();
    std::hint::black_box(backend.render());
    report.info("Вся страница: растеризация CPU 1920×1080", ms(t.elapsed()));
    // Память: прирост всей кучи (включая кадр бэкенда и векторы) и учёт самого
    // рендерера. Растровый кэш ограничен `RASTER_CACHE_BYTES` (64 МБ).
    report.info(
        "Прирост кучи после отрисовки (всё)",
        format!("{:.1} МБ", heap_mb() - before),
    );
    report.info(
        "Растровый кэш фреймов по учёту рендерера (бюджет 64 МБ)",
        format!(
            "{:.1} МБ",
            (renderer.raster_cache_bytes() - raster_before.min(renderer.raster_cache_bytes()))
                as f64
                / 1_048_576.0
        ),
    );

    // Кадр сразу после зума через степень двойки: сколько уходит на построение
    // растров, а сколько на рисование узлов. Построение = первый кадр минус
    // установившийся кадр того же зума (после прогрева).
    {
        let z = camera.zoom();
        let target = 2f64.powf(z.log2().floor() + 1.0) * 1.01;
        camera.set_zoom(target);
        backend.reset(w, h);
        let t = Instant::now();
        let first = renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
        let first_time = t.elapsed();
        report.info(
            "Кадр после зума через степень двойки: зум / построено растров / из кэша / узлами / LOD",
            format!(
                "{:.3} / {} / {} / {} / {}",
                camera.zoom(),
                first.raster_builds,
                first.rasters,
                first.drawn,
                first.lod_rects
            ),
        );
        report.gate(
            "Кадр сразу после зума через степень двойки",
            first_time,
            16.0,
            "≤ 16 мс",
        );
        // Следующие кадры: растры догоняют по `RASTER_BUILD_TIME_PER_FRAME`, остальные
        // фреймы — прежние растры в новом масштабе, поштучно узлы не рисуются.
        let mut series = Vec::new();
        for _ in 0..3 {
            backend.reset(w, h);
            let t = Instant::now();
            let s = renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
            series.push(format!(
                "{} ({} растров, {} узлов)",
                ms(t.elapsed()),
                s.raster_builds,
                s.drawn
            ));
        }
        report.info(
            "Кадры 2–4 после зума: время (построено растров, нарисовано узлов поштучно)",
            series.join("; "),
        );
        match warm_up(&mut renderer, &mut backend, &scene, &camera) {
            Some(t) => {
                let (steady, _) = frame(&mut renderer, &mut backend, &scene, &camera);
                report.info(
                    "Кадр после зума: первый кадр / установившийся кадр / кадров до прогрева",
                    format!("{} / {} / {}", ms(first_time), ms(steady), t.len() + 3),
                );
                report.gate(
                    "Кадры прогрева после зума: худший",
                    *t.iter().max().unwrap(),
                    16.0,
                    "≤ 16 мс",
                );
            }
            None => report.fail("Прогрев после зума через степень двойки", "не прогрелся"),
        }
        camera.set_zoom(z);
    }

    // Подписи фреймов при плавном зуме: 60 фреймов с длинными названиями, все
    // подписи усечены, ширина каждой меняется на каждом кадре. Фреймы без заливки и
    // детей: время кадра — почти целиком работа с подписями.
    {
        let mut ldoc = Document::new();
        let lpage = ldoc.pages()[0].0;
        ldoc.transact(lpage, vec![], |p| {
            for i in 0..LABEL_FRAMES {
                let id = p.create_node(None, None, NodeKind::Frame)?;
                p.set_props(
                    id,
                    &PropsPatch {
                        name: Some(format!(
                            "Экран оформления заказа {i}: доставка, оплата и подтверждение"
                        )),
                        x: Some((i % 10) as f64 * 260.0),
                        y: Some((i / 10) as f64 * 300.0),
                        w: Some(200.0),
                        h: Some(200.0),
                        fills: Some(vec![]),
                        ..Default::default()
                    },
                )?;
            }
            Ok(vec![])
        })
        .unwrap();
        let _ = ldoc.take_changes(lpage);
        let lscene = Scene::from_page(ldoc.page(lpage).unwrap());
        let mut lcam = Camera::new(VIEWPORT, 1.0);
        lcam.fit(lscene.bounds().unwrap(), 40.0);
        let overlay = Overlay::default();
        let mut lr = Renderer::new();
        let (lw, lh) = lcam.device_size();
        let mut lb = CpuBackend::new(lw, lh);
        // Первый кадр раскладывает полные названия — прогрев.
        lr.render(&mut lb, &lscene, &lcam, &overlay, &NoImages, None);
        let warm = lr.text_layouts();
        let mut worst = Duration::ZERO;
        for _ in 0..LABEL_ZOOM_FRAMES {
            lcam.set_zoom(lcam.zoom() * 1.002);
            let t = Instant::now();
            lr.render(&mut lb, &lscene, &lcam, &overlay, &NoImages, None);
            worst = worst.max(t.elapsed());
        }
        let layouts = (lr.text_layouts() - warm) as f64 / LABEL_ZOOM_FRAMES as f64;
        // Сама работа с подписями — то, что оверлей делает с каждым названием на
        // кадре (очистка, кэш, усечение до ширины фрейма), без заливки глифов:
        // тот же зум, те же ширины, отдельный `TextEngine`.
        let mut text = TextEngine::new();
        let mut zcam = Camera::new(VIEWPORT, 1.0);
        zcam.fit(lscene.bounds().unwrap(), 40.0);
        let names: Vec<(String, f64)> = lscene
            .roots()
            .iter()
            .map(|&s| {
                let n = lscene.at(s);
                (n.props.name.clone(), n.world_bounds.unwrap().width())
            })
            .collect();
        let mut label_work = |cam: &Camera| {
            let t = Instant::now();
            for (name, w) in &names {
                std::hint::black_box(text.label(name, LABEL_SIZE, 400, w * cam.device_scale()));
            }
            t.elapsed()
        };
        label_work(&zcam);
        let mut label_worst = Duration::ZERO;
        for _ in 0..LABEL_ZOOM_FRAMES {
            zcam.set_zoom(zcam.zoom() * 1.002);
            label_worst = label_worst.max(label_work(&zcam));
        }
        report.info(
            "Подписи при зуме: фреймов с усечённой подписью / раскладок в прогреве",
            format!("{LABEL_FRAMES} / {warm}"),
        );
        report.0.push(Row {
            metric: "Подписи при зуме: раскладок текста на кадр после прогрева".to_string(),
            value: format!("{layouts:.2}"),
            limit: "0",
            pass: Some(layouts == 0.0),
        });
        report.gate(
            "Подписи при зуме: работа с подписями за кадр (худший кадр)",
            label_worst,
            1.0,
            "≤ 1 мс",
        );
        report.info(
            "Подписи при зуме: кадр целиком с заливкой глифов (худший)",
            ms(worst),
        );
    }

    text_section(&mut report);
    vector_section(&mut report);
    curvy_vector_section(&mut report);
    boolean_section(&mut report);

    // Экспорт фрейма @3x — справочно.
    let first_frame = scene.at(scene.roots()[0]).id;
    let t = Instant::now();
    let png = renderer
        .export_png(&scene, first_frame, 3.0, &NoImages)
        .unwrap();
    report.info(
        "Экспорт фрейма 1440×1024 @3x в PNG",
        format!("{} ({} КБ)", ms(t.elapsed()), png.len() / 1024),
    );

    println!("| Метрика | Значение | Цель | Итог |");
    println!("|---|---|---|---|");
    let mut failed = false;
    for r in &report.0 {
        let verdict = match r.pass {
            Some(true) => "PASS",
            Some(false) => {
                failed = true;
                "FAIL"
            }
            None => "—",
        };
        println!("| {} | {} | {} | {} |", r.metric, r.value, r.limit, verdict);
    }
    println!();
    println!("Замер нативный, release, 100 000 узлов. WASM ожидается в 1.5–2 раза медленнее.");
    if failed {
        std::process::exit(1);
    }
}
