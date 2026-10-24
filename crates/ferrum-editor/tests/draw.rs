//! Инструменты рисования, рука и зум (раздел 7.1 спеки).

mod common;

use common::*;
use ferrum_doc::{FillKind, NodeKind};
use ferrum_editor::*;
use kurbo::Point;

fn only_child(h: &mut H) -> ferrum_doc::NodeId {
    let roots = h.children(None);
    assert_eq!(roots.len(), 1);
    roots[0]
}

#[test]
fn rect_tool_draws_one_step_selects_and_returns_to_move() {
    let mut h = H::new();
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyR", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
    assert_eq!(h.ed.cursor(), Cursor::Crosshair);
    h.drag_hold((100.0, 100.0), (200.0, 150.0), none());
    // Пока кнопка нажата, фигура — черновик сцены и выделена в оверлее.
    assert!(h.children(None).is_empty(), "документ не тронут");
    assert!(h.ed.scene().has_preview());
    let overlay = h.ed.overlay();
    assert_eq!(overlay.selection.len(), 1);
    assert!(h.ed.scene().is_draft(overlay.selection[0]));
    h.up(200.0, 150.0, none());
    let id = only_child(&mut h);
    let p = h.props(id);
    assert_eq!(p.kind, NodeKind::Rect);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 100.0, 50.0));
    assert_eq!(h.sel(), vec![id]);
    assert_eq!(
        h.ed.tool_kind(),
        ToolKind::Move,
        "после рисования — «Перемещение»"
    );
    h.assert_scene_clean();
    let events = h.ed.take_events();
    assert!(events.contains(&EditorEvent::ToolChanged(ToolKind::Move)));
    assert!(events.contains(&EditorEvent::SelectionChanged(vec![id])));
    assert!(events.contains(&EditorEvent::DocumentChanged));
    h.assert_one_step(steps, &before);
}

#[test]
fn modifiers_square_center_and_angle() {
    let mut h = H::new();
    // Shift — квадрат по большей стороне в сторону курсора.
    h.key("KeyR", none());
    h.drag((100.0, 100.0), (180.0, 130.0), shift());
    let a = h.sel()[0];
    let p = h.props(a);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 80.0, 80.0));
    // Alt — от центра; эллипс с Shift — круг.
    h.key("KeyO", none());
    h.drag((300.0, 300.0), (340.0, 320.0), alt());
    let p = h.props(h.sel()[0]);
    assert_eq!(p.kind, NodeKind::Ellipse);
    assert_eq!((p.x, p.y, p.w, p.h), (260.0, 280.0, 80.0, 40.0));
    h.key("KeyO", none());
    h.drag((500.0, 500.0), (440.0, 530.0), shift());
    let p = h.props(h.sel()[0]);
    assert_eq!((p.x, p.y, p.w, p.h), (440.0, 500.0, 60.0, 60.0));
    // Линия с Shift — шаг 45°.
    h.key("KeyL", none());
    h.drag((100.0, 600.0), (200.0, 590.0), shift());
    let p = h.props(h.sel()[0]);
    assert_eq!(p.kind, NodeKind::Line);
    assert_eq!((p.x, p.y, p.rotation), (100.0, 600.0, 0.0));
    assert!(close(p.w, (100.0f64 * 100.0 + 100.0).sqrt()));
    h.key("KeyL", none());
    h.drag((100.0, 700.0), (160.0, 640.0), shift());
    let p = h.props(h.sel()[0]);
    assert_eq!(p.rotation, 45.0, "вверх-вправо — против часовой");
    assert_eq!(p.h, 0.0);
}

#[test]
fn shapes_nest_into_the_frame_under_the_start_point() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 400.0, 300.0);
    h.key("KeyF", none());
    h.drag((150.0, 150.0), (250.0, 250.0), none());
    let inner = h.sel()[0];
    assert_eq!(h.parent(inner), Some(f), "фрейм во фрейме");
    let p = h.props(inner);
    assert_eq!(
        (p.x, p.y, p.w, p.h),
        (50.0, 50.0, 100.0, 100.0),
        "координаты родителя"
    );
    h.key("KeyR", none());
    h.drag((160.0, 160.0), (200.0, 200.0), none());
    let r = h.sel()[0];
    assert_eq!(h.parent(r), Some(inner), "самый глубокий фрейм");
    assert_eq!((h.props(r).x, h.props(r).y), (10.0, 10.0));
    // Вне фрейма — в корень.
    h.key("KeyR", none());
    h.drag((700.0, 100.0), (750.0, 150.0), none());
    assert_eq!(h.parent(h.sel()[0]), None);
    // A — тоже фрейм.
    h.key("KeyA", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Frame);
}

#[test]
fn click_creates_default_size() {
    let mut h = H::new();
    h.key("KeyF", none());
    h.click(300.0, 200.0, none());
    let p = h.props(h.sel()[0]);
    assert_eq!(p.kind, NodeKind::Frame);
    assert_eq!((p.x, p.y, p.w, p.h), (300.0, 200.0, 100.0, 100.0));
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    h.key("KeyL", none());
    h.click(100.0, 100.0, none());
    let p = h.props(h.sel()[0]);
    assert_eq!((p.kind, p.w, p.h), (NodeKind::Line, 100.0, 0.0));
}

#[test]
fn escape_cancels_drawing_without_history() {
    let mut h = H::new();
    let steps = h.ed.step_count();
    h.key("KeyR", none());
    h.drag_hold((100.0, 100.0), (200.0, 200.0), none());
    assert!(h.ed.scene().has_preview());
    h.key("Escape", none());
    assert!(!h.ed.is_busy());
    h.assert_scene_clean();
    assert_eq!(
        h.ed.tool_kind(),
        ToolKind::Rect,
        "первый Esc только отменяет жест"
    );
    h.up(200.0, 200.0, none());
    assert!(
        h.children(None).is_empty(),
        "отпускание после Esc ничего не создаёт"
    );
    h.key("Escape", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert_eq!(h.ed.step_count(), steps);
    assert!(!h.ed.document().can_undo());
    // Потеря указателя тоже отменяет.
    h.key("KeyR", none());
    h.drag_hold((100.0, 100.0), (200.0, 200.0), none());
    h.ev(Input::PointerCancel);
    h.assert_scene_clean();
    assert!(h.children(None).is_empty());
}

#[test]
fn image_tool_places_by_hash_with_aspect() {
    let mut h = H::new();
    let hash = "ab".repeat(32);
    h.key("KeyK", ctrl().with_shift());
    assert!(h.ed.take_events().contains(&EditorEvent::RequestImage));
    h.ed.place_image(&hash, 200.0, 100.0);
    assert_eq!(h.ed.tool_kind(), ToolKind::Image);
    h.click(50.0, 60.0, none());
    let p = h.props(h.sel()[0]);
    assert_eq!(p.kind, NodeKind::Image);
    assert_eq!((p.x, p.y, p.w, p.h), (50.0, 60.0, 200.0, 100.0));
    assert_eq!(p.fills[0].kind, FillKind::Image);
    assert_eq!(p.fills[0].image_hash.as_deref(), Some(hash.as_str()));
    // Перетаскивание сохраняет пропорции картинки.
    h.ed.place_image(&hash, 200.0, 100.0);
    h.drag((300.0, 300.0), (400.0, 380.0), none());
    let p = h.props(h.sel()[0]);
    assert_eq!((p.w, p.h), (160.0, 80.0));
}

#[test]
fn hand_tool_space_hold_and_middle_button_pan() {
    let mut h = H::new();
    let c0 = h.ed.camera().center();
    h.key("KeyH", none());
    assert_eq!(h.ed.cursor(), Cursor::Grab);
    h.drag_hold((500.0, 400.0), (450.0, 380.0), none());
    assert_eq!(h.ed.cursor(), Cursor::Grabbing);
    h.up(450.0, 380.0, none());
    assert_eq!(
        h.ed.camera().center(),
        c0 + v(50.0, 20.0),
        "содержимое едет с курсором"
    );
    // Space держит руку временно.
    h.key("KeyR", none());
    h.ev(Input::KeyDown(KeyEvent::new("Space", none())));
    assert_eq!(h.ed.tool_kind(), ToolKind::Hand);
    let mut repeat = KeyEvent::new("Space", none());
    repeat.repeat = true;
    h.ev(Input::KeyDown(repeat));
    h.drag((500.0, 400.0), (510.0, 400.0), none());
    h.ev(Input::KeyUp(KeyEvent::new("Space", none())));
    assert_eq!(
        h.ed.tool_kind(),
        ToolKind::Rect,
        "после Space — прежний инструмент"
    );
    assert!(h.children(None).is_empty(), "рука ничего не рисует");
    // Средняя кнопка панорамирует поверх любого инструмента.
    let c1 = h.ed.camera().center();
    let mut e = h.pe(100.0, 100.0, none());
    e.button = PointerButton::Middle;
    h.ev(Input::PointerDown(e));
    e.point = Point::new(130.0, 100.0);
    h.ev(Input::PointerMove(e));
    h.ev(Input::PointerUp(e));
    assert_eq!(h.ed.camera().center(), c1 - v(30.0, 0.0));
    assert!(h.children(None).is_empty());
}

#[test]
fn wheel_zooms_at_cursor_and_pans() {
    let mut h = H::new();
    let cursor = Point::new(200.0, 300.0);
    let anchor = h.ed.camera().screen_to_world(cursor);
    h.ev(Input::Wheel(WheelEvent {
        point: cursor,
        delta: v(0.0, -100.0),
        modifiers: ctrl(),
    }));
    let z = h.ed.camera().zoom();
    assert!(close(z, 1f64.exp()), "{z}");
    let after = h.ed.camera().screen_to_world(cursor);
    assert!(
        (after - anchor).hypot() < 1e-9,
        "точка под курсором на месте"
    );
    // Щипок и предел зума.
    h.ev(Input::Pinch {
        point: cursor,
        scale: 1e6,
    });
    assert_eq!(h.ed.camera().zoom(), ferrum_render::MAX_ZOOM);
    // Без Ctrl колесо панорамирует, с Shift — вбок.
    let c = h.ed.camera().center();
    h.ev(Input::Wheel(WheelEvent {
        point: cursor,
        delta: v(0.0, 256.0),
        modifiers: shift(),
    }));
    let d = h.ed.camera().center() - c;
    assert!(close(d.x, 1.0) && close(d.y, 0.0), "{d:?}");
}

#[test]
fn russian_layout_uses_physical_keys() {
    let mut h = H::new();
    // На русской раскладке R печатает «к», O — «щ»; сочетания по физической клавише.
    let mut e = KeyEvent::new("KeyR", none());
    e.key = "к".into();
    h.ev(Input::KeyDown(e));
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
    let mut e = KeyEvent::new("KeyO", none());
    e.key = "щ".into();
    h.ev(Input::KeyDown(e));
    assert_eq!(h.ed.tool_kind(), ToolKind::Ellipse);
}
