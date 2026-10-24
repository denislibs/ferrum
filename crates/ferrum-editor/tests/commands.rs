//! Команды из таблицы (раздел 7.6 спеки): каждая правка — один шаг отмены.

mod common;

use common::*;
use ferrum_doc::NodeKind;
use ferrum_editor::*;
use kurbo::Point;

fn world_corner(h: &H, id: ferrum_doc::NodeId) -> Point {
    h.ed.scene().node(id).unwrap().world * Point::ZERO
}

#[test]
fn delete_and_duplicate() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(None, 300.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyD", ctrl());
    let roots = h.children(None);
    assert_eq!(roots.len(), 3);
    assert_eq!(roots[0], a);
    let copy = roots[1];
    assert_eq!(h.sel(), vec![copy], "копия над оригиналом и выделена");
    assert_eq!(h.props(copy).x, 100.0, "на том же месте");
    h.assert_one_step(steps, &before);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("Delete", none());
    assert_eq!(h.children(None), vec![a, b]);
    assert!(h.sel().is_empty());
    h.assert_one_step(steps, &before);
    h.click(320.0, 120.0, none());
    h.key("Backspace", none());
    assert_eq!(h.children(None), vec![a]);
}

#[test]
fn group_ungroup_and_frame_selection_keep_screen_positions() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 800.0, 800.0);
    let a = h.rect(Some(f), 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(Some(f), 300.0, 200.0, 50.0, 50.0);
    let c = h.rect(Some(f), 500.0, 500.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    h.click(320.0, 220.0, shift());
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyG", ctrl());
    let g = h.sel()[0];
    assert_eq!(h.props(g).kind, NodeKind::Group);
    assert_eq!(
        h.children(Some(f)),
        vec![g, c],
        "группа на месте верхнего выделенного"
    );
    assert_eq!(h.children(Some(g)), vec![a, b]);
    assert_eq!(world_corner(&h, a), Point::new(100.0, 100.0));
    h.assert_one_step(steps, &before);
    // Разгруппировка возвращает детей на место группы.
    let steps = h.ed.step_count();
    let before = h.state();
    h.key("KeyG", ctrl().with_shift());
    assert_eq!(h.children(Some(f)), vec![a, b, c]);
    assert_eq!(h.sel(), vec![a, b]);
    assert_eq!(world_corner(&h, b), Point::new(300.0, 200.0));
    h.assert_one_step(steps, &before);
    // Обернуть во фрейм: фрейм по общей рамке, дети в его координатах.
    h.key("KeyG", ctrl().with_alt());
    let fr = h.sel()[0];
    let p = h.props(fr);
    assert_eq!(
        (p.kind, p.x, p.y, p.w, p.h),
        (NodeKind::Frame, 100.0, 100.0, 250.0, 150.0)
    );
    assert_eq!((h.props(a).x, h.props(a).y), (0.0, 0.0));
    assert_eq!((h.props(b).x, h.props(b).y), (200.0, 100.0));
    assert_eq!(world_corner(&h, b), Point::new(300.0, 200.0));
    h.assert_scene_clean();
}

#[test]
fn z_order_commands() {
    let mut h = H::new();
    let ids: Vec<_> = (0..4)
        .map(|i| h.rect(None, 100.0 + 10.0 * i as f64, 100.0, 50.0, 50.0))
        .collect();
    let [a, b, c, d] = [ids[0], ids[1], ids[2], ids[3]];
    h.ed.select(&[a]);
    h.key("BracketRight", ctrl());
    assert_eq!(h.children(None), vec![b, a, c, d]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("BracketRight", ctrl().with_alt());
    assert_eq!(h.children(None), vec![b, c, d, a]);
    h.assert_one_step(steps, &before);
    h.key("BracketLeft", ctrl());
    assert_eq!(h.children(None), vec![b, c, a, d]);
    h.key("BracketLeft", ctrl().with_alt());
    assert_eq!(h.children(None), vec![a, b, c, d]);
    // Несколько выделенных сдвигаются вместе, верхний уже наверху не лезет дальше.
    h.ed.select(&[b, d]);
    h.key("BracketRight", ctrl());
    assert_eq!(h.children(None), vec![a, c, b, d]);
    let steps = h.ed.step_count();
    h.ed.select(&[d]);
    h.key("BracketRight", ctrl());
    assert_eq!(h.ed.step_count(), steps, "без изменений — без шага");
}

#[test]
fn nudge_hide_lock_select_all_and_escape() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 800.0, 800.0);
    let a = h.rect(Some(f), 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(Some(f), 300.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    h.key("ArrowRight", none());
    h.key("ArrowDown", shift());
    assert_eq!((h.props(a).x, h.props(a).y), (101.0, 110.0));
    h.key("ArrowLeft", shift());
    h.key("ArrowUp", none());
    assert_eq!((h.props(a).x, h.props(a).y), (91.0, 109.0));
    // Выделить всё — соседи в контексте выделения.
    h.key("KeyA", ctrl());
    assert_eq!(h.sel(), vec![a, b]);
    h.key("KeyH", ctrl().with_shift());
    assert!(!h.props(a).visible && !h.props(b).visible);
    h.key("KeyH", ctrl().with_shift());
    assert!(h.props(a).visible && h.props(b).visible);
    h.key("KeyL", ctrl().with_shift());
    assert!(h.props(a).locked && h.props(b).locked);
    h.key("KeyA", ctrl());
    assert!(
        h.sel().is_empty() || h.sel() == vec![f],
        "заблокированные не выделяются"
    );
    h.ed.select(&[a, b]);
    h.key("KeyL", ctrl().with_shift());
    assert!(!h.props(a).locked);
    // Esc: сначала инструмент → «Перемещение», затем снять выделение.
    h.key("KeyR", none());
    h.key("Escape", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert!(!h.sel().is_empty());
    h.key("Escape", none());
    assert!(h.sel().is_empty());
    // Выделить всё без выделения — объекты верхнего уровня.
    h.key("KeyA", ctrl());
    assert_eq!(h.sel(), vec![f]);
}

#[test]
fn zoom_commands_and_events() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 2000.0, 1000.0);
    let b = h.rect(None, 5000.0, 5000.0, 100.0, 100.0);
    h.key("Digit1", shift());
    let cam = h.ed.camera().clone();
    let all = cam.visible_world_rect();
    assert!(all.contains(Point::new(0.0, 0.0)) && all.contains(Point::new(5100.0, 5100.0)));
    h.ed.select(&[b]);
    h.key("Digit2", shift());
    assert!(close(h.ed.camera().center().x, 5050.0));
    assert!(h.ed.camera().zoom() > 1.0);
    h.key("Digit0", shift());
    assert_eq!(h.ed.camera().zoom(), 1.0);
    h.key("Equal", none());
    assert_eq!(h.ed.camera().zoom(), 2.0);
    h.key("Minus", none());
    assert_eq!(h.ed.camera().zoom(), 1.0);
    h.ed.take_events();
    h.key("Backslash", ctrl());
    h.key("KeyC", ctrl().with_shift());
    h.key("KeyV", ctrl());
    let events = h.ed.take_events();
    assert!(events.contains(&EditorEvent::ToggleUi));
    assert!(events.contains(&EditorEvent::ExportPng(vec![b])));
    assert!(events.contains(&EditorEvent::PasteRequested));
    let _ = a;
}

#[test]
fn undo_redo_restore_selection_and_skip_during_gesture() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    h.drag((120.0, 120.0), (220.0, 120.0), none());
    h.click(900.0, 700.0, none());
    assert!(h.sel().is_empty());
    h.key("KeyZ", ctrl());
    assert_eq!(h.props(a).x, 100.0);
    assert_eq!(h.sel(), vec![a], "отмена вернула выделение");
    h.key("KeyZ", ctrl().with_shift());
    assert_eq!(h.props(a).x, 200.0);
    h.key("KeyZ", ctrl());
    h.key("KeyY", ctrl());
    assert_eq!(h.props(a).x, 200.0, "Ctrl+Y на Windows");
    // Во время жеста команды не выполняются.
    h.drag_hold((220.0, 120.0), (400.0, 400.0), none());
    h.key("KeyZ", ctrl());
    h.key("Delete", none());
    h.up(400.0, 400.0, none());
    assert_eq!(h.children(None), vec![a]);
    assert_eq!(h.props(a).x, 380.0);
}

#[test]
fn undo_switches_back_to_the_edited_page() {
    let mut h = H::new();
    let p1 = h.ed.page();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let p2 = h.ed.add_page("Вторая").unwrap();
    h.ed.set_page(p2).unwrap();
    assert!(h.ed.take_events().contains(&EditorEvent::PageChanged(p2)));
    let b = h.rect(None, 10.0, 10.0, 20.0, 20.0);
    h.ed.set_page(p1).unwrap();
    h.ed.select(&[a]);
    h.key("Delete", none());
    assert!(h.children(None).is_empty());
    h.key("KeyZ", ctrl());
    assert_eq!(h.children(None), vec![a]);
    assert_eq!(h.ed.page(), p1);
    h.ed.take_events();
    // Следующая отмена правит вторую страницу — редактор переходит на неё.
    h.key("KeyZ", ctrl());
    assert_eq!(h.ed.page(), p2);
    assert!(h.ed.take_events().contains(&EditorEvent::PageChanged(p2)));
    assert!(h.children(None).is_empty(), "создание b отменено");
    h.assert_scene_clean();
    h.key("KeyZ", ctrl().with_shift());
    assert_eq!(h.children(None), vec![b]);
    h.assert_scene_clean();
}

#[test]
fn frame_presets_create_frames_to_the_right() {
    let mut h = H::new();
    let presets = frame_presets();
    assert_eq!(presets.len(), 11);
    let iphone = presets.iter().find(|p| p.name == "iPhone 16").unwrap();
    assert_eq!((iphone.width, iphone.height), (393.0, 852.0));
    let a4 = presets.iter().find(|p| p.name == "A4").unwrap();
    assert_eq!((a4.width, a4.height), (595.0, 842.0));
    h.ed.add_frame_preset(iphone);
    let f = h.sel()[0];
    let p = h.props(f);
    assert_eq!((p.kind, p.w, p.h), (NodeKind::Frame, 393.0, 852.0));
    assert_eq!(p.name, "iPhone 16");
    h.ed.add_frame_preset(a4);
    let g = h.sel()[0];
    assert_eq!(h.props(g).x, p.x + 393.0 + PRESET_GAP);
    assert_eq!(h.props(g).y, p.y);
    assert!(parse_presets(
        "{\"version\":1,\"presets\":[{\"group\":\"x\",\"name\":\"y\",\"width\":0,\"height\":5}]}"
    )
    .is_err());
    assert!(matches!(
        parse_presets("{\"version\":9,\"presets\":[]}"),
        Err(PresetError::NewerVersion(9))
    ));
    assert!(parse_presets("[]").is_err());
}

#[test]
fn failed_panel_transaction_rolls_back_without_step() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let steps = h.ed.step_count();
    let before = h.state();
    // Ошибка посреди транзакции (перенос узла в самого себя): откат, без шага.
    let r = h.ed.transact(|tx| {
        tx.set_props(a, &patch(1.0, 1.0, 1.0, 1.0))?;
        tx.move_node(a, Some(a), 0)?;
        Ok(vec![])
    });
    assert!(r.is_err());
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    h.assert_scene_clean();
}

#[test]
fn editor_uses_keymap_overrides() {
    let mut h = H::new();
    h.ed.keymap_mut()
        .set_bindings(CommandId::ToolRect, vec![Shortcut::parse("KeyQ").unwrap()]);
    h.key("KeyR", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    h.key("KeyQ", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
}
