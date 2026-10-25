//! Измерение расстояний с Alt (разделы 7.3 и 8.6 спеки): сценарии событий →
//! точные отрезки и значения в оверлее. Камера с зумом 1: экран совпадает с миром.
//! Каждый «пустой» исход проверяется после того, как в той же сцене измерения
//! были, — иначе тест прошёл бы и без измерений вовсе.

mod common;

use common::*;
use ferrum_doc::{NodeId, NodeKind};
use ferrum_editor::measure::{measure, round_value};
use ferrum_editor::*;
use ferrum_render::{Measure, MeasureKind};
use kurbo::{Line, Rect};

const ALT: Modifiers = Modifiers::ALT;

fn d(a: (f64, f64), b: (f64, f64), value: f64) -> Measure {
    Measure {
        line: Line::new(a, b),
        value,
        kind: MeasureKind::Distance,
    }
}

fn g(a: (f64, f64), b: (f64, f64)) -> Measure {
    Measure {
        line: Line::new(a, b),
        value: 0.0,
        kind: MeasureKind::Guide,
    }
}

fn alt_key(code: &str) -> KeyEvent {
    KeyEvent {
        key: "Alt".into(),
        ..KeyEvent::new(code, ALT)
    }
}

impl H {
    fn alt_down(&mut self) {
        self.ev(Input::KeyDown(alt_key("AltLeft")));
    }

    fn alt_up(&mut self) {
        self.ev(Input::KeyUp(KeyEvent {
            key: "Alt".into(),
            ..KeyEvent::new("AltLeft", Modifiers::NONE)
        }));
    }

    /// Движение с нажатым Alt (как его шлёт браузер: `altKey` у указателя).
    fn alt_hover(&mut self, x: f64, y: f64) {
        self.mv(x, y, ALT);
    }

    fn measures(&self) -> Vec<Measure> {
        self.ed.overlay().measurements
    }

    fn redraws(&mut self) -> usize {
        self.ed
            .take_events()
            .iter()
            .filter(|e| **e == EditorEvent::NeedsRedraw)
            .count()
    }
}

/// Выделение S — один узел; затем Alt нажат.
fn select_and_alt(h: &mut H, s: NodeId) {
    h.ed.select(&[s]);
    h.alt_down();
}

#[test]
fn target_contains_selection_four_distances_to_inner_edges() {
    let mut h = H::new();
    let t = h.rect(None, 0.0, 0.0, 400.0, 300.0);
    let s = h.rect(None, 100.0, 50.0, 80.0, 40.0);
    select_and_alt(&mut h, s);
    h.alt_hover(300.0, 250.0);
    assert_eq!(h.ed.overlay().hover, Some(t), "наведён T");
    // От краёв S до внутренних краёв T, по центру S (140, 70).
    assert_eq!(
        h.measures(),
        [
            d((100.0, 70.0), (0.0, 70.0), 100.0),
            d((180.0, 70.0), (400.0, 70.0), 220.0),
            d((140.0, 50.0), (140.0, 0.0), 50.0),
            d((140.0, 90.0), (140.0, 300.0), 210.0),
        ]
    );
}

#[test]
fn selection_contains_target_four_distances_from_target_edges() {
    let mut h = H::new();
    let s = h.rect(None, 0.0, 0.0, 400.0, 300.0);
    let t = h.rect(None, 100.0, 50.0, 80.0, 40.0);
    select_and_alt(&mut h, s);
    h.alt_hover(140.0, 70.0);
    assert_eq!(h.ed.overlay().hover, Some(t));
    // Между краями T и внутренними краями S, по центру T; отрезок — от края S.
    assert_eq!(
        h.measures(),
        [
            d((0.0, 70.0), (100.0, 70.0), 100.0),
            d((400.0, 70.0), (180.0, 70.0), 220.0),
            d((140.0, 0.0), (140.0, 50.0), 50.0),
            d((140.0, 300.0), (140.0, 90.0), 210.0),
        ]
    );
}

#[test]
fn disjoint_horizontal_gap_at_middle_of_vertical_overlap() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    h.rect(None, 300.0, 120.0, 60.0, 60.0);
    select_and_alt(&mut h, s);
    h.alt_hover(330.0, 150.0);
    // Перекрытие по y — 120..140, середина 130.
    assert_eq!(h.measures(), [d((180.0, 130.0), (300.0, 130.0), 120.0)]);
    // T слева от S: от левого края S до правого края T.
    let mut h = H::new();
    let s = h.rect(None, 300.0, 100.0, 80.0, 40.0);
    h.rect(None, 100.0, 90.0, 50.0, 30.0);
    select_and_alt(&mut h, s);
    h.alt_hover(120.0, 100.0);
    assert_eq!(h.measures(), [d((300.0, 110.0), (150.0, 110.0), 150.0)]);
}

#[test]
fn disjoint_vertical_gap_at_middle_of_horizontal_overlap() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    h.rect(None, 140.0, 250.0, 100.0, 30.0);
    select_and_alt(&mut h, s);
    h.alt_hover(200.0, 260.0);
    // Перекрытие по x — 140..180, середина 160.
    assert_eq!(h.measures(), [d((160.0, 140.0), (160.0, 250.0), 110.0)]);
    // T сверху.
    let mut h = H::new();
    let s = h.rect(None, 100.0, 300.0, 80.0, 40.0);
    h.rect(None, 60.0, 100.0, 60.0, 50.0);
    select_and_alt(&mut h, s);
    h.alt_hover(80.0, 120.0);
    assert_eq!(h.measures(), [d((110.0, 300.0), (110.0, 150.0), 150.0)]);
}

#[test]
fn diagonal_draws_both_gaps_from_selection_centre_with_guides() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    h.rect(None, 300.0, 250.0, 50.0, 50.0);
    select_and_alt(&mut h, s);
    h.alt_hover(320.0, 270.0);
    // Центр S — (140, 120). Направляющие вдоль измеренных краёв T до самого T.
    assert_eq!(
        h.measures(),
        [
            d((180.0, 120.0), (300.0, 120.0), 120.0),
            g((300.0, 120.0), (300.0, 250.0)),
            d((140.0, 140.0), (140.0, 250.0), 110.0),
            g((140.0, 250.0), (300.0, 250.0)),
        ]
    );
    // T слева сверху: края T — правый и нижний.
    let mut h = H::new();
    let s = h.rect(None, 300.0, 300.0, 80.0, 40.0);
    h.rect(None, 100.0, 100.0, 50.0, 50.0);
    select_and_alt(&mut h, s);
    h.alt_hover(120.0, 120.0);
    assert_eq!(
        h.measures(),
        [
            d((300.0, 320.0), (150.0, 320.0), 150.0),
            g((150.0, 320.0), (150.0, 150.0)),
            d((340.0, 300.0), (340.0, 150.0), 150.0),
            g((340.0, 150.0), (150.0, 150.0)),
        ]
    );
}

#[test]
fn partial_overlap_measures_differing_edges_only() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    h.rect(None, 150.0, 100.0, 100.0, 60.0);
    select_and_alt(&mut h, s);
    h.alt_hover(230.0, 130.0);
    // Перекрытие 150..200 × 100..160: середины 175 и 130. Верхние края совпали — их нет.
    assert_eq!(
        h.measures(),
        [
            d((100.0, 130.0), (150.0, 130.0), 50.0),
            d((200.0, 130.0), (250.0, 130.0), 50.0),
            d((175.0, 200.0), (175.0, 160.0), 40.0),
        ]
    );
}

#[test]
fn empty_hover_measures_to_parent_frame() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 300.0);
    let s = h.rect(Some(f), 100.0, 50.0, 80.0, 40.0);
    let want = [
        d((100.0, 70.0), (0.0, 70.0), 100.0),
        d((180.0, 70.0), (400.0, 70.0), 220.0),
        d((140.0, 50.0), (140.0, 0.0), 50.0),
        d((140.0, 90.0), (140.0, 300.0), 210.0),
    ];
    h.click(140.0, 70.0, Modifiers::NONE);
    assert_eq!(h.sel(), [s]);
    h.alt_down();
    // Пустой холст.
    h.alt_hover(700.0, 600.0);
    assert_eq!(h.ed.overlay().hover, None);
    assert_eq!(h.measures(), want);
    // Наведён сам родительский фрейм (из панели слоёв) — то же.
    h.ed.set_hover(Some(f));
    assert_eq!(h.ed.overlay().hover, Some(f));
    assert_eq!(h.measures(), want);
    // Пустое место фрейма на холсте — то же.
    h.alt_hover(350.0, 250.0);
    assert_eq!(h.measures(), want);
}

#[test]
fn parent_frame_is_found_above_groups_and_for_common_parent() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 300.0);
    let grp = h.add(Some(f), NodeKind::Group, patch(0.0, 0.0, 0.0, 0.0));
    let a = h.rect(Some(grp), 100.0, 50.0, 80.0, 40.0);
    let b = h.rect(Some(grp), 200.0, 150.0, 20.0, 50.0);
    // Один узел в группе внутри фрейма: меряем до фрейма, не до группы.
    h.ed.select(&[a]);
    h.alt_down();
    h.alt_hover(700.0, 600.0);
    assert_eq!(h.measures()[0], d((100.0, 70.0), (0.0, 70.0), 100.0));
    // Два узла: их общая рамка 100..220 × 50..200 до фрейма.
    h.ed.select(&[a, b]);
    assert_eq!(
        h.measures(),
        [
            d((100.0, 125.0), (0.0, 125.0), 100.0),
            d((220.0, 125.0), (400.0, 125.0), 180.0),
            d((160.0, 50.0), (160.0, 0.0), 50.0),
            d((160.0, 200.0), (160.0, 300.0), 100.0),
        ]
    );
    // Узлы из разных фреймов: общий родитель — корень, мерить не до чего.
    let f2 = h.frame(None, 500.0, 0.0, 200.0, 200.0);
    let c = h.rect(Some(f2), 520.0, 20.0, 10.0, 10.0);
    h.ed.select(&[a, c]);
    assert_eq!(h.measures(), []);
}

#[test]
fn root_selection_without_hover_shows_nothing() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    h.rect(None, 300.0, 120.0, 60.0, 60.0);
    select_and_alt(&mut h, s);
    h.alt_hover(330.0, 150.0);
    assert_eq!(h.measures().len(), 1, "над соседом измерение есть");
    // Пустой холст, а S в корне — родительского фрейма нет.
    h.alt_hover(700.0, 600.0);
    assert_eq!(h.ed.overlay().hover, None);
    assert_eq!(h.measures(), []);
}

#[test]
fn hovering_selected_node_shows_nothing() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    let b = h.rect(None, 300.0, 100.0, 80.0, 40.0);
    let t = h.rect(None, 100.0, 300.0, 80.0, 40.0);
    // Одиночное выделение: над соседом есть, над самим S — нет.
    select_and_alt(&mut h, a);
    h.alt_hover(140.0, 320.0);
    assert_eq!(h.ed.overlay().hover, Some(t));
    assert!(!h.measures().is_empty());
    h.alt_hover(140.0, 120.0);
    assert_eq!(h.ed.overlay().hover, Some(a));
    assert_eq!(h.measures(), []);
    // Несколько выделенных: над любым из них — ничего.
    h.ed.select(&[a, b]);
    h.alt_hover(140.0, 320.0);
    assert!(!h.measures().is_empty());
    h.alt_hover(340.0, 120.0);
    assert_eq!(h.ed.overlay().hover, Some(b));
    assert_eq!(h.measures(), []);
}

#[test]
fn nothing_over_selection_handles() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 300.0);
    let s = h.rect(Some(f), 100.0, 50.0, 80.0, 40.0);
    select_and_alt(&mut h, s);
    h.alt_hover(250.0, 200.0);
    assert_eq!(h.measures().len(), 4);
    // Правая нижняя ручка: курсор ресайза, измерений нет.
    h.alt_hover(180.0, 90.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { .. }));
    assert_eq!(h.measures(), []);
}

#[test]
fn zero_distances_are_omitted() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 300.0);
    // S касается левого и верхнего краёв фрейма.
    let s = h.rect(Some(f), 0.0, 0.0, 80.0, 40.0);
    select_and_alt(&mut h, s);
    h.alt_hover(700.0, 600.0);
    assert_eq!(
        h.measures(),
        [
            d((80.0, 20.0), (400.0, 20.0), 320.0),
            d((40.0, 40.0), (40.0, 300.0), 260.0),
        ]
    );
    // Вплотную стоящий сосед: зазор 0 — ни линии, ни направляющей.
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    h.rect(None, 180.0, 100.0, 50.0, 40.0);
    select_and_alt(&mut h, s);
    h.alt_hover(200.0, 120.0);
    assert!(h.ed.overlay().hover.is_some());
    assert_eq!(h.measures(), []);
}

#[test]
fn values_are_rounded_like_properties_panel() {
    let mut h = H::new();
    // Правый край S — 60.125, зазор до T — 39.875 → 39.88; положение линии точное.
    let s = h.rect(None, 10.125, 100.0, 50.0, 40.0);
    h.rect(None, 100.0, 100.0, 50.0, 40.0);
    select_and_alt(&mut h, s);
    h.alt_hover(120.0, 120.0);
    assert_eq!(h.measures(), [d((60.125, 120.0), (100.0, 120.0), 39.88)]);
    // Зазор 0.004 округляется до 0 — не рисуется.
    let mut h = H::new();
    let s = h.rect(None, 0.0, 100.0, 100.0, 40.0);
    h.rect(None, 100.004, 100.0, 50.0, 40.0);
    select_and_alt(&mut h, s);
    h.alt_hover(120.0, 120.0);
    assert!(h.ed.overlay().hover.is_some());
    assert_eq!(h.measures(), []);
    assert_eq!(round_value(12.345_6), 12.35);
    assert_eq!(round_value(7.0), 7.0);
}

#[test]
fn alt_up_clears_and_redraws_only_on_change() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    h.rect(None, 300.0, 120.0, 60.0, 60.0);
    h.ed.select(&[s]);
    h.hover(330.0, 150.0);
    assert_eq!(h.measures(), [], "без Alt измерений нет");
    h.redraws();
    // Alt без движения: измерения сразу и кадр.
    h.alt_down();
    assert_eq!(h.measures().len(), 1);
    assert_eq!(h.redraws(), 1, "измерения появились — нужен кадр");
    // Движение над тем же T: измерения те же — кадр не нужен.
    h.alt_hover(331.0, 151.0);
    assert_eq!(h.measures().len(), 1);
    assert_eq!(h.redraws(), 0, "те же измерения — без кадра");
    // Автоповтор клавиши Alt ничего не меняет.
    h.ev(Input::KeyDown(KeyEvent {
        repeat: true,
        ..alt_key("AltLeft")
    }));
    assert_eq!(h.redraws(), 0);
    h.alt_up();
    assert_eq!(h.measures(), []);
    assert_eq!(h.redraws(), 1, "измерения ушли — нужен кадр");
}

#[test]
fn updates_on_selection_and_scene_change() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    let t = h.rect(None, 300.0, 120.0, 60.0, 60.0);
    let u = h.rect(None, 100.0, 300.0, 80.0, 40.0);
    select_and_alt(&mut h, s);
    h.alt_hover(330.0, 150.0);
    assert_eq!(h.measures(), [d((180.0, 130.0), (300.0, 130.0), 120.0)]);
    // Правка сдвинула S: кэш рамки выделения обновился.
    h.set(s, patch(120.0, 100.0, 80.0, 40.0));
    assert_eq!(h.measures(), [d((200.0, 130.0), (300.0, 130.0), 100.0)]);
    // Другое выделение — от его рамки.
    h.ed.select(&[u]);
    assert_eq!(h.ed.overlay().hover, Some(t));
    assert_eq!(
        h.measures(),
        [
            d((180.0, 320.0), (300.0, 320.0), 120.0),
            g((300.0, 320.0), (300.0, 180.0)),
            d((140.0, 300.0), (140.0, 180.0), 120.0),
            g((140.0, 180.0), (300.0, 180.0)),
        ]
    );
}

#[test]
fn blur_clears() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    h.rect(None, 300.0, 120.0, 60.0, 60.0);
    select_and_alt(&mut h, s);
    h.alt_hover(330.0, 150.0);
    assert_eq!(h.measures().len(), 1);
    h.redraws();
    h.ev(Input::Blur);
    assert_eq!(h.measures(), []);
    assert_eq!(h.redraws(), 1);
}

#[test]
fn pointer_modifiers_fix_missed_alt_key_events() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    h.rect(None, 300.0, 120.0, 60.0, 60.0);
    h.ed.select(&[s]);
    // Нажатие Alt не дошло (фокус был в поле), но указатель несёт altKey.
    h.alt_hover(330.0, 150.0);
    assert_eq!(h.measures().len(), 1);
    // Отпускание не дошло: следующее движение без Alt убирает измерения.
    h.hover(331.0, 150.0);
    assert_eq!(h.measures(), []);
}

#[test]
fn alt_graph_does_not_measure_on_windows_and_linux() {
    for platform in [Platform::Windows, Platform::Linux] {
        let mut h = H::with_platform(platform);
        let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
        h.rect(None, 300.0, 120.0, 60.0, 60.0);
        h.ed.select(&[s]);
        // Обычный Alt в той же сцене — измерения есть.
        h.alt_down();
        h.alt_hover(330.0, 150.0);
        assert_eq!(h.measures().len(), 1, "{platform:?}: Alt");
        h.alt_up();
        h.hover(330.0, 150.0);
        // AltGr: приходит как Ctrl+Alt с настоящим AltGraph.
        let ctrl_alt = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::NONE
        };
        h.ev(Input::KeyDown(KeyEvent {
            alt_graph: true,
            ..KeyEvent::new("AltRight", ctrl_alt)
        }));
        assert_eq!(h.measures(), [], "{platform:?}: AltGr");
        // Движение с зажатым AltGr — указатель видит Ctrl+Alt и AltGr от Alt не отличает.
        h.mv(331.0, 150.0, ctrl_alt);
        assert_eq!(h.measures(), [], "{platform:?}: движение с AltGr");
        // Отпустили AltGr — нажатие обычного Alt снова меряет.
        h.ev(Input::KeyUp(KeyEvent::new("AltRight", Modifiers::NONE)));
        h.alt_down();
        assert_eq!(h.measures().len(), 1, "{platform:?}: снова Alt");
    }
    // На macOS признак AltGraph не учитывается: Option иногда приходит как AltGraph.
    let mut h = H::with_platform(Platform::Mac);
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    h.rect(None, 300.0, 120.0, 60.0, 60.0);
    h.ed.select(&[s]);
    h.hover(330.0, 150.0);
    h.ev(Input::KeyDown(KeyEvent {
        alt_graph: true,
        ..alt_key("AltLeft")
    }));
    assert_eq!(h.measures().len(), 1, "macOS: Option как AltGraph");
}

#[test]
fn no_measurements_during_gestures_and_alt_keeps_its_meanings() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 600.0, 400.0);
    let s = h.rect(Some(f), 100.0, 100.0, 80.0, 40.0);
    h.rect(Some(f), 300.0, 120.0, 60.0, 60.0);
    select_and_alt(&mut h, s);
    h.alt_hover(330.0, 150.0);
    assert_eq!(h.measures().len(), 1);
    let steps = h.ed.step_count();
    // Перетаскивание с Alt: копия, как и раньше, и никаких измерений во время жеста.
    h.drag_hold((140.0, 120.0), (140.0, 220.0), ALT);
    assert!(h.ed.is_busy());
    assert_eq!(h.ed.cursor(), Cursor::Duplicate, "Alt — копия");
    assert_eq!(h.measures(), []);
    h.up(140.0, 220.0, ALT);
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(h.children(Some(f)).len(), 3, "копия создана");
    // После жеста Alt всё ещё нажат: измерения — от новой выделенной копии.
    assert!(!h.measures().is_empty());
    // Ресайз за ручку с Alt (от центра) — без измерений.
    let sel = h.sel()[0];
    let b = h.ed.scene().node(sel).unwrap().world_bounds.unwrap();
    h.drag_hold((b.x1, b.y1), (b.x1 + 20.0, b.y1 + 20.0), ALT);
    assert!(h.ed.is_busy());
    assert_eq!(h.measures(), []);
    h.ev(Input::KeyDown(KeyEvent::new("Escape", ALT)));
    // Рамка выделения мышью — тоже.
    h.drag_hold((500.0, 350.0), (450.0, 300.0), ALT);
    assert!(h.ed.overlay().marquee.is_some());
    assert_eq!(h.measures(), []);
    h.ev(Input::KeyDown(KeyEvent::new("Escape", ALT)));
    // Рисование.
    h.ed.select(&[s]);
    h.ed.set_tool(ToolKind::Rect);
    h.alt_hover(500.0, 300.0);
    assert_eq!(h.measures(), [], "у инструмента рисования измерений нет");
    h.drag_hold((500.0, 300.0), (550.0, 350.0), ALT);
    assert_eq!(h.measures(), []);
}

#[test]
fn rotated_objects_use_world_bounds() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    let t = h.rect(None, 300.0, 100.0, 100.0, 100.0);
    h.set(
        t,
        ferrum_doc::PropsPatch {
            rotation: Some(45.0),
            ..Default::default()
        },
    );
    let wb = {
        let n = h.ed.scene().node(t).unwrap();
        n.world.transform_rect_bbox(n.local_bounds.unwrap())
    };
    select_and_alt(&mut h, s);
    let c = wb.center();
    h.alt_hover(c.x, c.y);
    assert_eq!(h.ed.overlay().hover, Some(t));
    assert_eq!(
        measure(Rect::new(100.0, 100.0, 180.0, 140.0), wb),
        h.measures()
    );
    let m = h.measures();
    assert!(!m.is_empty());
    // Ближний край T — левый край его рамки в мире, а не угол повёрнутого квадрата.
    assert_eq!(m[0].line.p1.x, wb.x0);
}

#[test]
fn no_measurements_during_panel_gesture() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 300.0);
    let s = h.rect(Some(f), 100.0, 50.0, 80.0, 40.0);
    select_and_alt(&mut h, s);
    h.alt_hover(700.0, 600.0);
    assert_eq!(h.measures().len(), 4);
    // Перетаскивание подписи поля X (Alt — шаг ×0.1): жест панели, превью в сцене.
    let edit = |value| PanelEdit::Number {
        field: NumField::X,
        value,
        relative: false,
    };
    h.ed.edit_selection(&edit(101.5), Phase::Preview);
    assert_eq!(h.measures(), [], "во время жеста панели измерений нет");
    h.ed.edit_selection(&edit(102.0), Phase::Preview);
    assert_eq!(h.measures(), []);
    h.ed.edit_selection(&edit(102.0), Phase::Commit);
    // Жест закончен, Alt всё ещё нажат — измерения от сдвинутого узла.
    assert_eq!(h.measures()[0], d((102.0, 70.0), (0.0, 70.0), 102.0));
}

#[test]
fn camera_change_retargets_hover_and_measurements() {
    let mut h = H::new();
    let s = h.rect(None, 100.0, 100.0, 80.0, 40.0);
    let b = h.rect(None, 300.0, 120.0, 60.0, 60.0);
    let c = h.rect(None, 300.0, 300.0, 60.0, 60.0);
    select_and_alt(&mut h, s);
    h.alt_hover(330.0, 150.0);
    assert_eq!(h.ed.overlay().hover, Some(b));
    let cam = h.ed.camera().clone();
    // Колесо сдвигает холст на 180 вниз по миру: под тем же курсором теперь C.
    h.ev(Input::Wheel(WheelEvent {
        point: kurbo::Point::new(330.0, 150.0),
        delta: kurbo::Vec2::new(0.0, 180.0),
        modifiers: ALT,
    }));
    assert_eq!(
        h.ed.overlay().hover,
        Some(c),
        "наведение следует за камерой"
    );
    let sr = Rect::new(100.0, 100.0, 180.0, 140.0);
    assert_eq!(
        h.measures(),
        measure(sr, Rect::new(300.0, 300.0, 360.0, 360.0))
    );
    // Камера целиком (восстановление вида) — по последней точке указателя.
    h.ed.set_camera(cam);
    assert_eq!(h.ed.overlay().hover, Some(b));
    assert_eq!(h.measures(), [d((180.0, 130.0), (300.0, 130.0), 120.0)]);
}

#[test]
fn guide_only_when_line_misses_target_edge() {
    let mut h = H::new();
    // Горизонтальная линия (рамка нулевой высоты) на уровне верхнего края T: линия
    // приходит прямо в угол T, направляющая вдоль всего края не нужна.
    let s = h.add(None, NodeKind::Line, patch(100.0, 100.0, 80.0, 0.0));
    h.rect(None, 300.0, 100.0, 60.0, 100.0);
    select_and_alt(&mut h, s);
    h.alt_hover(330.0, 150.0);
    assert_eq!(h.measures(), [d((180.0, 100.0), (300.0, 100.0), 120.0)]);
    // То же для вертикальной линии у левого края T снизу.
    assert_eq!(
        measure(
            Rect::new(300.0, 300.0, 300.0, 380.0),
            Rect::new(300.0, 100.0, 400.0, 160.0)
        ),
        [d((300.0, 300.0), (300.0, 160.0), 140.0)]
    );
}
