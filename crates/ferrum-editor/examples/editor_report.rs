//! Контрольный бенчмарк редактора:
//! `cargo run --release -p ferrum-editor --example editor_report`.
//! Страница из 100 000 узлов; сверяет замеры с целями этапа 3 и печатает таблицу
//! PASS/FAIL (код выхода 1 при провале).

use std::time::{Duration, Instant};

use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, NodeId};
use ferrum_editor::{CommandId, Editor, Input, Modifiers, Platform, PointerEvent};
use ferrum_render::Camera;
use kurbo::{Point, Size};

const NODES: usize = 100_000;
const VIEWPORT: Size = Size::new(1920.0, 1080.0);
/// Движений указателя в замере одного жеста.
const MOVES: usize = 60;
/// Повторов записи и отмены перетаскивания (медиана).
const COMMIT_RUNS: usize = 5;
/// Порог привязки редактора в экранных пикселях (`snap::SNAP_THRESHOLD_PX`).
const SNAP_PX: f64 = 4.0;

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn worst(runs: &[Duration]) -> Duration {
    runs.iter().copied().max().unwrap_or_default()
}

fn ms(d: Duration) -> String {
    format!("{:.3} мс", d.as_secs_f64() * 1000.0)
}

struct Row {
    metric: String,
    value: String,
    limit: String,
    pass: Option<bool>,
}

/// Подпись цели из числового предела — единственное место, где она строится.
fn limit_label(limit_ms: f64) -> String {
    format!("≤ {limit_ms} мс")
}

struct Report(Vec<Row>);

impl Report {
    fn gate(&mut self, metric: &str, d: Duration, limit_ms: f64) {
        self.0.push(Row {
            metric: metric.to_string(),
            value: ms(d),
            limit: limit_label(limit_ms),
            pass: Some(d.as_secs_f64() * 1000.0 <= limit_ms),
        });
    }

    /// Проверка без времени: замер не считается, если сценарий ничего не сделал.
    fn check(&mut self, metric: &str, ok: bool, value: String, limit: &str) {
        self.0.push(Row {
            metric: metric.to_string(),
            value,
            limit: limit.to_string(),
            pass: Some(ok),
        });
    }

    fn info(&mut self, metric: &str, value: String) {
        self.0.push(Row {
            metric: metric.to_string(),
            value,
            limit: "—".to_string(),
            pass: None,
        });
    }
}

/// Часы событий: каждое следующее на 600 мс позже — клики не склеиваются в двойные.
struct Clock(f64);

impl Clock {
    fn at(&mut self, p: Point) -> PointerEvent {
        self.0 += 600.0;
        PointerEvent::new(p, Modifiers::NONE, self.0)
    }
}

fn timed(ed: &mut Editor, input: Input) -> Duration {
    let t = Instant::now();
    ed.handle(input);
    let d = t.elapsed();
    ed.take_events();
    d
}

/// Экранная точка мира.
fn screen(ed: &Editor, x: f64, y: f64) -> Point {
    ed.camera().world_to_screen(Point::new(x, y))
}

/// Левый верхний угол узла в мире (с обводкой) — по нему видно, сдвинулся ли узел.
fn origin(ed: &Editor, id: NodeId) -> Point {
    let b = ed.scene().node(id).unwrap().world_bounds.unwrap();
    Point::new(b.x0, b.y0)
}

/// Исход перетаскивания: времена замеренных движений и свидетельства, что жест
/// действительно что-то делал.
struct Drag {
    /// Перетаскиваемые узлы.
    sel: Vec<NodeId>,
    times: Vec<Duration>,
    /// Наибольшее число линий привязки за движения.
    max_lines: usize,
    /// Узлов выделения, сдвинутых в сцене к последнему движению.
    moved: usize,
    /// Экранная точка последнего движения (для отпускания).
    end: Point,
}

/// Перетаскивание выделенных `sel` от экранной точки `from`; кнопка остаётся
/// нажатой. Первое движение (4 px > порога 3 px) начинает жест и не входит в замер,
/// поэтому в выборке нет пустых движений до порога. Каждое из `MOVES` замеренных
/// движений сдвигает указатель на `step` px по x.
fn drag_moves(ed: &mut Editor, clock: &mut Clock, sel: &[NodeId], from: Point, step: f64) -> Drag {
    ed.handle(Input::PointerDown(clock.at(from)));
    // Пустой `sel` — выделение определяет само нажатие (что окажется под курсором).
    let sel: Vec<NodeId> = if sel.is_empty() {
        ed.selection().to_vec()
    } else {
        sel.to_vec()
    };
    let before: Vec<Point> = sel.iter().map(|id| origin(ed, *id)).collect();
    let kick = Point::new(from.x + 4.0, from.y);
    ed.handle(Input::PointerMove(clock.at(kick)));
    ed.take_events();
    let mut times = Vec::new();
    let mut max_lines = 0;
    let mut end = kick;
    for i in 1..=MOVES {
        end = Point::new(kick.x + step * i as f64, kick.y + 0.37 * i as f64);
        times.push(timed(ed, Input::PointerMove(clock.at(end))));
        max_lines = max_lines.max(ed.overlay().snap_lines.len());
    }
    let moved = sel
        .iter()
        .zip(&before)
        .filter(|(id, b)| origin(ed, **id) != **b)
        .count();
    Drag {
        sel,
        times,
        max_lines,
        moved,
        end,
    }
}

/// Проверки жеста: сдвинуто `want` узлов и привязки нашли хоть одну линию.
fn check_drag(report: &mut Report, name: &str, d: &Drag, want: usize) {
    report.check(
        &format!("{name}: узлов сдвинуто в превью сцены"),
        d.moved == want,
        format!("{}", d.moved),
        &format!("{want}"),
    );
    report.check(
        &format!("{name}: наибольшее число линий привязки за жест"),
        d.max_lines > 0,
        format!("{}", d.max_lines),
        "> 0",
    );
}

/// Наведение: `n` движений по видимой части без кнопки; возвращает времена и
/// число движений, после которых над курсором есть объект.
fn hover_moves(
    ed: &mut Editor,
    clock: &mut Clock,
    n: usize,
    a: usize,
    b: usize,
) -> (Vec<Duration>, usize) {
    let mut hits = 0;
    let times = (0..n)
        .map(|i| {
            let p = Point::new((i * a % 1900) as f64 + 7.0, (i * b % 1060) as f64 + 9.0);
            let d = timed(ed, Input::PointerMove(clock.at(p)));
            if ed.overlay().hover.is_some() {
                hits += 1;
            }
            d
        })
        .collect();
    (times, hits)
}

fn main() {
    let mut report = Report(Vec::new());

    // Документ из 100 000 узлов: фреймы по 100 (фрейм + 99 фигур 120×80 сеткой 10×10).
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids: Vec<NodeId> = doc.transact(page, vec![], |p| fill(p, NODES)).unwrap();
    let mut ed = Editor::new(doc, Some(page), VIEWPORT, 1.0, Platform::Mac).unwrap();
    assert_eq!(ed.scene().len(), NODES);
    let mut cam = Camera::new(VIEWPORT, 1.0);
    cam.set_center(Point::new(960.0, 540.0));
    ed.set_camera(cam.clone());
    let mut clock = Clock(0.0);

    // 1. Перетаскивание 1000 выделенных: дети первых фреймов верхнего ряда.
    let children: Vec<NodeId> = ids
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 100 != 0)
        .map(|(_, id)| *id)
        .take(1000)
        .collect();
    ed.select(&children);
    // Первый ребёнок первого фрейма: (0, 0) — 120×80.
    let grab = screen(&ed, 60.0, 40.0);
    let drag = drag_moves(&mut ed, &mut clock, &children, grab, 1.7);
    assert!(
        ed.is_busy() && ed.scene().has_preview(),
        "перетаскивание идёт"
    );
    report.gate(
        "Движение при перетаскивании 1000 выделенных: превью и привязки (медиана)",
        median(drag.times.clone()),
        4.0,
    );
    report.info("То же, худшее движение", ms(worst(&drag.times)));
    check_drag(
        &mut report,
        "Перетаскивание 1000 выделенных",
        &drag,
        children.len(),
    );
    let end = drag.end;
    ed.handle(Input::PointerUp(clock.at(end)));
    ed.execute(CommandId::Undo);
    ed.take_events();

    // 2. Запись перетаскивания 1000 узлов одной транзакцией и её отмена.
    let mut commits = Vec::new();
    let mut undos = Vec::new();
    let steps0 = ed.step_count();
    let sample = children[500];
    let orig = origin(&ed, sample);
    let mut dx_ok = true;
    let mut back_ok = true;
    let mut shift = Point::ORIGIN;
    for _ in 0..COMMIT_RUNS {
        ed.select(&children);
        let drag = drag_moves(&mut ed, &mut clock, &children, grab, 1.7);
        commits.push(timed(&mut ed, Input::PointerUp(clock.at(drag.end))));
        // Сдвиг образца равен сдвигу указателя в мире с точностью до привязки.
        let now = origin(&ed, sample);
        shift = Point::new(now.x - orig.x, now.y - orig.y);
        let want = (drag.end - grab) / ed.camera().zoom();
        let tol = SNAP_PX / ed.camera().zoom() + 1e-6;
        dx_ok &= (shift.x - want.x).abs() <= tol
            && (shift.y - want.y).abs() <= tol
            && (shift.x != 0.0 || shift.y != 0.0);
        let t = Instant::now();
        ed.execute(CommandId::Undo);
        undos.push(t.elapsed());
        ed.take_events();
        back_ok &= origin(&ed, sample) == orig;
    }
    assert_eq!(
        ed.step_count(),
        steps0 + COMMIT_RUNS as u64,
        "каждый жест — один шаг"
    );
    report.gate(
        "Запись перетаскивания 1000 узлов (отпускание: транзакция и сцена)",
        median(commits),
        20.0,
    );
    report.gate(
        "Отмена перетаскивания 1000 узлов (документ и сцена)",
        median(undos),
        20.0,
    );
    report.check(
        "Запись: образец сдвинулся на сдвиг указателя (допуск — привязка)",
        dx_ok,
        format!("({:.1}, {:.1})", shift.x, shift.y),
        "= сдвигу указателя",
    );
    report.check(
        "Отмена: образец вернулся точно на прежнее место",
        back_ok,
        format!("{back_ok}"),
        "true",
    );

    // 2а. Отражение 1000 выделенных (Shift+H): одна транзакция и её отмена — та же
    // запись 1000 узлов, что и у перетаскивания, с теми же целями.
    let mut flips = Vec::new();
    let mut unflips = Vec::new();
    let mut flip_ok = true;
    let mut unflip_ok = true;
    let steps0 = ed.step_count();
    let props0 = ed.page_doc().props(sample).unwrap();
    for _ in 0..COMMIT_RUNS {
        ed.select(&children);
        let t = Instant::now();
        ed.execute(CommandId::FlipHorizontal);
        flips.push(t.elapsed());
        ed.take_events();
        let p = ed.page_doc().props(sample).unwrap();
        flip_ok &= p.flip_x != props0.flip_x && p.x != props0.x;
        let t = Instant::now();
        ed.execute(CommandId::Undo);
        unflips.push(t.elapsed());
        ed.take_events();
        unflip_ok &= ed.page_doc().props(sample).unwrap() == props0;
    }
    assert_eq!(
        ed.step_count(),
        steps0 + COMMIT_RUNS as u64,
        "каждое отражение — один шаг"
    );
    report.gate(
        "Отражение 1000 выделенных (Shift+H: транзакция и сцена)",
        median(flips),
        20.0,
    );
    report.gate(
        "Отмена отражения 1000 узлов (документ и сцена)",
        median(unflips),
        20.0,
    );
    report.check(
        "Отражение: у образца переключён flip_x и сдвинуто положение",
        flip_ok,
        format!("{flip_ok}"),
        "true",
    );
    report.check(
        "Отмена отражения: образец вернулся точно",
        unflip_ok,
        format!("{unflip_ok}"),
        "true",
    );

    // 3. Один узел среди 98 соседей и фрейма: привязки к ним.
    let one = children[50];
    ed.select(&[one]);
    let c = ed.scene().node(one).unwrap().world_bounds.unwrap().center();
    let from = screen(&ed, c.x, c.y);
    let drag = drag_moves(&mut ed, &mut clock, &[one], from, 2.3);
    report.gate(
        "Движение одного узла с привязками к 100 соседям (медиана)",
        median(drag.times.clone()),
        1.0,
    );
    report.info("То же, худшее движение", ms(worst(&drag.times)));
    check_drag(&mut report, "Один узел", &drag, 1);
    ed.execute(CommandId::Escape);
    ed.handle(Input::PointerUp(clock.at(from)));
    ed.take_events();

    // 4. Наведение: hit-test и правила выделения на каждое движение без кнопки.
    ed.select(&[]);
    let (hovers, hits) = hover_moves(&mut ed, &mut clock, 400, 37, 53);
    report.gate(
        "Наведение: hit-test и выбор объекта (медиана)",
        median(hovers.clone()),
        0.2,
    );
    report.info("То же, худшее движение", ms(worst(&hovers)));
    report.check(
        "Наведение: движений, после которых объект под курсором найден",
        hits > 0,
        format!("{hits} из 400"),
        "> 0",
    );

    // 4а. Наведение при 1000 выделенных: стоимость не должна расти вместе с выделением.
    ed.select(&children);
    ed.take_events();
    let (hovers_sel, hits_sel) = hover_moves(&mut ed, &mut clock, 400, 41, 59);
    report.gate(
        "Наведение при 1000 выделенных (медиана)",
        median(hovers_sel.clone()),
        0.2,
    );
    report.info("То же, худшее движение", ms(worst(&hovers_sel)));
    report.check(
        "Наведение при 1000 выделенных: движений с найденным объектом",
        hits_sel > 0,
        format!("{hits_sel} из 400"),
        "> 0",
    );
    ed.select(&[]);

    // 4б. Перетаскивание фрейма верхнего уровня с привязками при отдалении на плотной
    // странице: соседи — тысяча фреймов, полоса привязки в мире шире в 1/зум раз, и
    // в неё попадает ряд или столбец сетки (десятки кандидатов на ось).
    for zoom in [0.1, 0.03] {
        let mut dense = Camera::new(VIEWPORT, 1.0);
        dense.set_zoom(zoom);
        dense.set_center(Point::new(9600.0, 5400.0));
        ed.set_camera(dense);
        let target = ids
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 100 == 0)
            .filter_map(|(_, id)| {
                let b = ed.scene().node(*id)?.world_bounds?;
                // Хватаем за пустой угол фрейма: на ребёнка нажатие выбрало бы его, а не фрейм.
                let c = Point::new(b.x1 - 10.0, b.y1 - 10.0);
                Some((*id, c, (c.x - 9600.0).hypot(c.y - 5400.0)))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .unwrap();
        // Выделения нет: нажатие в углу выделенного фрейма попало бы в ручку ресайза;
        // что выберет нажатие, смотрим по выделению после него.
        ed.select(&[]);
        let from = screen(&ed, target.1.x, target.1.y);
        let drag = drag_moves(&mut ed, &mut clock, &[], from, 0.8);
        let name = format!("Фрейм при отдалении (зум {zoom})");
        report.gate(
            &format!("Движение фрейма с привязками к соседям-фреймам при отдалении (зум {zoom}; медиана)"),
            median(drag.times.clone()),
            4.0,
        );
        report.info(&format!("{name}: худшее движение"), ms(worst(&drag.times)));
        check_drag(&mut report, &name, &drag, 1);
        report.check(
            &format!("{name}: нажатие выбрало фрейм верхнего уровня"),
            drag.sel.len() == 1 && ids.iter().step_by(100).any(|f| *f == drag.sel[0]),
            format!("{}", drag.sel.len()),
            "1 фрейм",
        );
        ed.execute(CommandId::Escape);
        ed.handle(Input::PointerUp(clock.at(from)));
        ed.take_events();
        ed.select(&[]);
    }

    // 4в. 1000 узлов из ~100 родителей (по 10 детей каждого видимого фрейма) при зуме 0.1.
    let mut dense = Camera::new(VIEWPORT, 1.0);
    dense.set_zoom(0.1);
    dense.set_center(Point::new(9600.0, 5400.0));
    ed.set_camera(dense);
    let view = ed.camera().visible_world_rect();
    let spread: Vec<NodeId> = ids
        .chunks(100)
        .filter(|ch| {
            ed.scene()
                .node(ch[0])
                .and_then(|n| n.world_bounds)
                .is_some_and(|b| view.contains(b.center()))
        })
        .take(100)
        .flat_map(|ch| ch[1..=10].iter().copied())
        .collect();
    ed.select(&spread);
    let c = ed
        .scene()
        .node(spread[0])
        .unwrap()
        .world_bounds
        .unwrap()
        .center();
    let from = screen(&ed, c.x, c.y);
    let drag = drag_moves(&mut ed, &mut clock, &spread, from, 0.8);
    report.check(
        "Перетаскивание из многих родителей: выделено узлов",
        spread.len() == 1000,
        format!("{}", spread.len()),
        "1000",
    );
    report.gate(
        "Движение при перетаскивании 1000 узлов из ~100 фреймов при зуме 0.1 (медиана)",
        median(drag.times.clone()),
        4.0,
    );
    report.info("То же, худшее движение", ms(worst(&drag.times)));
    check_drag(
        &mut report,
        "1000 узлов из ~100 фреймов",
        &drag,
        spread.len(),
    );
    ed.execute(CommandId::Escape);
    ed.handle(Input::PointerUp(clock.at(from)));
    ed.take_events();
    ed.select(&[]);

    // 5. Рамка выделения: с пустого холста при отдалении, ~100 фреймов в кадре.
    let mut far = Camera::new(VIEWPORT, 1.0);
    far.set_zoom(0.1);
    far.set_center(Point::new(9600.0, 5400.0));
    ed.set_camera(far);
    let start = screen(&ed, 1520.0, -60.0);
    ed.handle(Input::PointerDown(clock.at(start)));
    let marquee: Vec<Duration> = (1..=MOVES)
        .map(|i| {
            let k = i as f64 / MOVES as f64;
            let p = Point::new(start.x + 1800.0 * k, start.y + 1000.0 * k);
            timed(&mut ed, Input::PointerMove(clock.at(p)))
        })
        .collect();
    let selected = ed.selection().len();
    report.gate(
        "Обновление рамки выделения на странице 100 000 (медиана, ~100 фреймов в рамке)",
        median(marquee.clone()),
        4.0,
    );
    report.info(
        "То же: худшее обновление / выделено фреймов в конце",
        format!("{} / {selected}", ms(worst(&marquee))),
    );
    ed.handle(Input::PointerUp(clock.at(start)));

    // Рамка через всю страницу при «показать всё» — справочно.
    ed.execute(CommandId::ZoomToFit);
    let corner = screen(&ed, -100.0, -100.0);
    ed.handle(Input::PointerDown(clock.at(corner)));
    let far_corner = screen(&ed, 80_000.0, 24_000.0);
    ed.handle(Input::PointerMove(
        clock.at(Point::new(corner.x + 5.0, corner.y + 5.0)),
    ));
    let whole = timed(&mut ed, Input::PointerMove(clock.at(far_corner)));
    let frames = ed.selection().len();
    report.gate(
        "Обновление рамки через всю страницу 100 000 (выделяется 1000 фреймов)",
        whole,
        4.0,
    );
    report.check(
        "Рамка через всю страницу выделила все фреймы верхнего уровня",
        frames == NODES / 100,
        format!("{frames}"),
        "1000",
    );
    ed.handle(Input::PointerUp(clock.at(far_corner)));
    report.check(
        "Превью не осталось после жестов",
        !ed.scene().has_preview() && !ed.is_busy(),
        format!("{}", ed.scene().has_preview()),
        "false",
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
