//! Контрольный бенчмарк сцены и рендера:
//! `cargo run --release -p ferrum-render --example render_report`.
//! Сверяет замеры с целями этапа 2 и печатает таблицу PASS/FAIL (код выхода 1 при провале).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ferrum_doc::container::{decode, encode};
use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{Camera, CpuBackend, NoImages, Renderer};
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
    // строится по `RASTER_BUILDS_PER_FRAME` фреймов за кадр; замер — после прогрева.
    camera.fit(scene.bounds().unwrap(), 0.0);
    {
        let mut cold_renderer = Renderer::new();
        match warm_up(&mut cold_renderer, &mut backend, &scene, &camera) {
            Some(t) => report.info(
                "Худший кадр прогрева растрового кэша (холодный рендерер: кадров до прогрева / максимум)",
                format!("{} / {}", t.len() - 1, ms(*t.iter().max().unwrap())),
            ),
            None => report.fail("Прогрев растрового кэша (холодный рендерер)", "не прогрелся"),
        }
    }
    match warm_up(&mut renderer, &mut backend, &scene, &camera) {
        Some(t) => report.info(
            "Вся страница: прогрев кэша (рендерер с прогретыми списками: кадров / самый долгий кадр)",
            format!("{} / {}", t.len() - 1, ms(*t.iter().max().unwrap())),
        ),
        None => report.fail("Вся страница: прогрев кэша", "не прогрелся"),
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
        // Следующие кадры: время падает вместе с числом узлов, рисуемых поштучно, —
        // по ним видно, что дороже: растры или поштучная отрисовка.
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
                    format!("{} / {} / {}", ms(first_time), ms(steady), t.len()),
                );
                report.info(
                    "Кадр после зума: лишнее время первого кадра (растры + поштучная отрисовка узлов без растров)",
                    ms(first_time.saturating_sub(steady)),
                );
            }
            None => report.fail("Прогрев после зума через степень двойки", "не прогрелся"),
        }
        camera.set_zoom(z);
    }

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
