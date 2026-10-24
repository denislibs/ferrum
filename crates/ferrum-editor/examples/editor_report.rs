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

    fn check(&mut self, metric: &str, ok: bool, value: String, limit: &'static str) {
        self.0.push(Row {
            metric: metric.to_string(),
            value,
            limit,
            pass: Some(ok),
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

/// Перетаскивание от экранной точки `from`: время каждого движения (превью и
/// привязки) и состояние «кнопка ещё нажата».
fn drag_moves(ed: &mut Editor, clock: &mut Clock, from: Point, step: f64) -> Vec<Duration> {
    ed.handle(Input::PointerDown(clock.at(from)));
    // Первое движение начинает жест (порог 3 px); оно тоже в замере.
    (1..=MOVES)
        .map(|i| {
            let p = Point::new(from.x + step * i as f64, from.y + 0.37 * i as f64);
            timed(ed, Input::PointerMove(clock.at(p)))
        })
        .collect()
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
    let moves = drag_moves(&mut ed, &mut clock, grab, 1.7);
    assert!(
        ed.is_busy() && ed.scene().has_preview(),
        "перетаскивание идёт"
    );
    report.gate(
        "Движение при перетаскивании 1000 выделенных: превью и привязки (медиана)",
        median(moves.clone()),
        4.0,
        "≤ 4 мс",
    );
    report.info(
        "То же, худшее движение (включая начало жеста)",
        ms(worst(&moves)),
    );
    let end = Point::new(grab.x + 1.7 * MOVES as f64, grab.y + 0.37 * MOVES as f64);
    ed.handle(Input::PointerUp(clock.at(end)));
    ed.execute(CommandId::Undo);
    ed.take_events();

    // 2. Запись перетаскивания 1000 узлов одной транзакцией и её отмена.
    let mut commits = Vec::new();
    let mut undos = Vec::new();
    let steps0 = ed.step_count();
    for _ in 0..COMMIT_RUNS {
        ed.select(&children);
        drag_moves(&mut ed, &mut clock, grab, 1.7);
        commits.push(timed(&mut ed, Input::PointerUp(clock.at(end))));
        let t = Instant::now();
        ed.execute(CommandId::Undo);
        undos.push(t.elapsed());
        ed.take_events();
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
        "≤ 20 мс",
    );
    report.gate(
        "Отмена перетаскивания 1000 узлов (документ и сцена)",
        median(undos),
        20.0,
        "≤ 20 мс",
    );

    // 3. Один узел среди 98 соседей и фрейма: привязки к ним.
    let one = children[50];
    ed.select(&[one]);
    let n = ed.scene().node(one).unwrap();
    let c = n.world_bounds.unwrap().center();
    let from = screen(&ed, c.x, c.y);
    let moves = drag_moves(&mut ed, &mut clock, from, 2.3);
    let lines = ed.overlay().snap_lines.len();
    report.gate(
        "Движение одного узла с привязками к 100 соседям (медиана)",
        median(moves.clone()),
        1.0,
        "≤ 1 мс",
    );
    report.info(
        "То же: худшее движение / линий привязки в конце",
        format!("{} / {lines}", ms(worst(&moves))),
    );
    ed.execute(CommandId::Escape);
    ed.handle(Input::PointerUp(clock.at(from)));
    ed.take_events();

    // 4. Наведение: hit-test и правила выделения на каждое движение без кнопки.
    ed.select(&[]);
    let hovers: Vec<Duration> = (0..400)
        .map(|i| {
            let p = Point::new((i * 37 % 1900) as f64 + 7.0, (i * 53 % 1060) as f64 + 9.0);
            timed(&mut ed, Input::PointerMove(clock.at(p)))
        })
        .collect();
    report.gate(
        "Наведение: hit-test и выбор объекта (медиана)",
        median(hovers.clone()),
        0.2,
        "≤ 0.2 мс",
    );
    report.info("То же, худшее движение", ms(worst(&hovers)));

    // 4а. Наведение при ~1000 выделенных: наведение и события не должны зависеть от размера выделения.
    ed.select(&children);
    ed.take_events();
    let hovers_sel: Vec<Duration> = (0..400)
        .map(|i| {
            let p = Point::new((i * 41 % 1900) as f64 + 5.0, (i * 59 % 1060) as f64 + 11.0);
            timed(&mut ed, Input::PointerMove(clock.at(p)))
        })
        .collect();
    report.gate(
        "Наведение при 1000 выделенных (медиана)",
        median(hovers_sel.clone()),
        0.2,
        "≤ 0.2 мс",
    );
    report.info("То же, худшее движение", ms(worst(&hovers_sel)));
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
                let c = ed.scene().node(*id)?.world_bounds?.center();
                Some((*id, c, (c.x - 9600.0).hypot(c.y - 5400.0)))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .unwrap();
        ed.select(&[target.0]);
        let from = screen(&ed, target.1.x, target.1.y);
        let moves = drag_moves(&mut ed, &mut clock, from, 0.8);
        let lines = ed.overlay().snap_lines.len();
        let (name, limit) = if zoom == 0.1 {
            ("зум 0.1", 4.0)
        } else {
            ("зум 0.03", 4.0)
        };
        report.gate(
            &format!(
                "Движение фрейма с привязками к соседям-фреймам при отдалении ({name}; медиана)"
            ),
            median(moves.clone()),
            limit,
            "≤ 4 мс",
        );
        report.info(
            &format!("То же ({name}): худшее движение / линий привязки в конце"),
            format!("{} / {lines}", ms(worst(&moves))),
        );
        ed.execute(CommandId::Escape);
        ed.handle(Input::PointerUp(clock.at(from)));
        ed.take_events();
        ed.select(&[]);
    }

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
        "≤ 4 мс",
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
        "≤ 4 мс",
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
