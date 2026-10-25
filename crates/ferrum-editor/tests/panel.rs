//! Правки из панелей (этап 5): поля панели свойств, палитра, кнопки и
//! переименование слоёв, перетаскивание слоёв. Каждая правка — один шаг отмены;
//! жест поля или палитры — превью и один шаг при отпускании.

mod common;

use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch, Stroke, StrokeAlign};
use ferrum_editor::*;
use kurbo::Point;

fn num(field: NumField, value: f64) -> PanelEdit {
    PanelEdit::Number {
        field,
        value,
        relative: false,
    }
}

fn add(field: NumField, value: f64) -> PanelEdit {
    PanelEdit::Number {
        field,
        value,
        relative: true,
    }
}

fn center(h: &H, id: ferrum_doc::NodeId) -> Point {
    let n = h.ed.scene().node(id).unwrap();
    n.world * n.local_bounds.unwrap().center()
}

#[test]
fn typed_value_is_one_step_for_every_selected() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 0.0, 50.0, 50.0);
    let b = h.rect(None, 50.0, 0.0, 50.0, 50.0);
    h.ed.select(&[a, b]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::X, 100.0), Phase::Commit);
    assert_eq!((h.props(a).x, h.props(b).x), (100.0, 100.0));
    h.assert_one_step(steps, &before);
    // «Смешанные»: прибавка к своему значению у каждого.
    h.ed.edit_selection(&num(NumField::Y, 7.0), Phase::Commit);
    h.set(
        b,
        PropsPatch {
            y: Some(20.0),
            ..Default::default()
        },
    );
    h.ed.edit_selection(&add(NumField::Y, 5.0), Phase::Commit);
    assert_eq!((h.props(a).y, h.props(b).y), (12.0, 25.0));
    assert_eq!(h.sel(), vec![a, b], "выделение остаётся");
}

#[test]
fn width_resizes_node_and_scales_group_children() {
    let mut h = H::new();
    let r = h.rect(None, 10.0, 20.0, 50.0, 40.0);
    h.ed.select(&[r]);
    h.ed.edit_selection(&num(NumField::W, 200.0), Phase::Commit);
    let p = h.props(r);
    assert_eq!(
        (p.x, p.y, p.w, p.h),
        (10.0, 20.0, 200.0, 40.0),
        "угол на месте"
    );
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 30.0, 0.0, 10.0, 10.0);
    h.ed.select(&[a, b]);
    h.key("KeyG", ctrl());
    let g = h.sel()[0];
    assert_eq!(h.props(g).kind, NodeKind::Group);
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::W, 80.0), Phase::Commit);
    assert_eq!(h.ed.step_count(), steps + 1);
    let pose = panel_pose(h.ed.scene(), h.ed.scene().slot(g).unwrap());
    assert!(close(pose.w, 80.0) && close(pose.h, 10.0), "{pose:?}");
    assert!(close(h.props(b).x, 60.0) && close(h.props(b).w, 20.0));
    let line = h.add(None, NodeKind::Line, patch(0.0, 300.0, 100.0, 0.0));
    h.ed.select(&[line]);
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::H, 50.0), Phase::Commit);
    assert_eq!(
        (h.ed.step_count(), h.props(line).h),
        (steps, 0.0),
        "у линии нет высоты"
    );
}

#[test]
fn rotation_turns_around_center() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 100.0, 50.0);
    h.ed.select(&[r]);
    let c = center(&h, r);
    h.ed.edit_selection(&num(NumField::Rotation, 90.0), Phase::Commit);
    assert!(close(h.props(r).rotation, 90.0));
    let c2 = center(&h, r);
    assert!(close(c.x, c2.x) && close(c.y, c2.y), "{c:?} → {c2:?}");
    h.ed.edit_selection(&num(NumField::Rotation, -90.0), Phase::Commit);
    assert!(close(h.props(r).rotation, 270.0));
    assert_eq!(display_rotation(270.0), -90.0, "панель показывает −90°");
}

#[test]
fn opacity_radius_and_stroke_apply_where_they_make_sense() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let e = h.add(None, NodeKind::Ellipse, patch(20.0, 0.0, 10.0, 10.0));
    h.ed.select(&[r, e]);
    h.ed.edit_selection(&num(NumField::Opacity, 1.7), Phase::Commit);
    assert_eq!((h.props(r).opacity, h.props(e).opacity), (1.0, 1.0));
    h.ed.edit_selection(&num(NumField::Opacity, 0.25), Phase::Commit);
    assert_eq!(h.props(e).opacity, 0.25);
    h.ed.edit_selection(&num(NumField::CornerRadius, 6.0), Phase::Commit);
    assert_eq!(
        (h.props(r).corner_radius, h.props(e).corner_radius),
        (6.0, 0.0)
    );
    let stroke = Stroke {
        color: Color::BLACK,
        width: 2.0,
        align: StrokeAlign::Inside,
    };
    h.ed.edit_selection(&PanelEdit::Strokes(vec![stroke]), Phase::Commit);
    h.ed.edit_selection(&add(NumField::StrokeWidth, 3.0), Phase::Commit);
    assert_eq!(h.props(r).strokes[0].width, 5.0);
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::X, f64::NAN), Phase::Commit);
    assert_eq!(h.ed.step_count(), steps, "NaN не правка");
}

#[test]
fn scrub_is_preview_then_one_step() {
    let mut h = H::new();
    let r = h.rect(None, 10.0, 0.0, 50.0, 50.0);
    h.ed.select(&[r]);
    let before = h.state();
    let steps = h.ed.step_count();
    for d in [1.0, 4.0, 9.0] {
        h.ed.edit_selection(&add(NumField::X, d), Phase::Preview);
        assert_eq!(h.props(r).x, 10.0, "документ не меняется во время жеста");
        assert_eq!(
            h.ed.scene().node(r).unwrap().props.x,
            10.0 + d,
            "превью от начала жеста"
        );
    }
    assert_eq!(h.ed.step_count(), steps);
    h.ed.edit_selection(&add(NumField::X, 9.0), Phase::Commit);
    assert_eq!(h.props(r).x, 19.0);
    h.assert_one_step(steps, &before);
}

#[test]
fn scrub_cancel_and_canvas_input_leave_no_history() {
    let mut h = H::new();
    let r = h.rect(None, 10.0, 0.0, 50.0, 50.0);
    h.ed.select(&[r]);
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Preview);
    h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Cancel);
    assert_eq!(h.ed.step_count(), steps);
    h.assert_scene_clean();
    h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Preview);
    h.down(500.0, 500.0, none());
    assert_eq!(
        h.ed.scene().node(r).unwrap().props.x,
        10.0,
        "превью снято нажатием"
    );
    h.up(500.0, 500.0, none());
    assert_eq!(
        h.ed.step_count(),
        steps,
        "ввод на холсте отменяет жест панели"
    );
    h.assert_scene_clean();
}

#[test]
fn fill_preview_and_commit_skip_groups() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 20.0, 0.0, 10.0, 10.0);
    h.ed.select(&[a, b]);
    h.key("KeyG", ctrl());
    let g = h.sel()[0];
    let c = h.rect(None, 50.0, 0.0, 10.0, 10.0);
    h.ed.select(&[g, c]);
    let red = vec![Fill::solid(Color::rgba(255, 0, 0, 255))];
    let steps = h.ed.step_count();
    h.ed.edit_selection(&PanelEdit::Fills(red.clone()), Phase::Preview);
    assert_eq!(h.ed.scene().node(c).unwrap().props.fills, red);
    assert_ne!(h.props(c).fills, red);
    h.ed.edit_selection(&PanelEdit::Fills(red.clone()), Phase::Commit);
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(h.props(c).fills, red);
    assert!(h.props(g).fills.is_empty(), "у группы заливок нет");
    h.ed.select(&[c]);
    h.ed.edit_selection(&PanelEdit::ClipContent(false), Phase::Commit);
    assert_eq!(h.ed.step_count(), steps + 1, "обрезка — только у фреймов");
}

#[test]
fn flags_and_rename_are_one_step_each() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.set_flags(&[r], Some(false), None);
    assert!(!h.props(r).visible);
    h.assert_one_step(steps, &before);
    h.ed.set_flags(&[r], None, Some(true));
    assert!(h.props(r).locked);
    let steps = h.ed.step_count();
    h.ed.rename_node(r, "  Кнопка  ");
    assert_eq!(h.props(r).name, "Кнопка");
    h.ed.rename_node(r, "");
    h.ed.rename_node(r, "Кнопка");
    assert_eq!(
        h.ed.step_count(),
        steps + 1,
        "пустое и то же имя — не правка"
    );
}

#[test]
fn move_layers_reorders_within_parent() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 20.0, 0.0, 10.0, 10.0);
    let c = h.rect(None, 40.0, 0.0, 10.0, 10.0);
    let before = h.state();
    let steps = h.ed.step_count();
    // Строки сверху вниз: c, b, a. «a» над «c» — на передний план.
    assert!(h.ed.move_layers(&[a], DropTarget::Above(c)));
    assert_eq!(h.children(None), vec![b, c, a]);
    h.assert_one_step(steps, &before);
    assert!(h.ed.move_layers(&[a], DropTarget::Below(b)));
    assert_eq!(h.children(None), vec![a, b, c]);
    assert_eq!(h.sel(), vec![a], "перенесённые выделены");
}

#[test]
fn move_into_frame_keeps_screen_position() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 300.0, 300.0);
    h.set(
        f,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    let r = h.rect(None, 150.0, 120.0, 20.0, 20.0);
    let world = h.ed.scene().node(r).unwrap().world;
    assert!(h.ed.move_layers(&[r], DropTarget::Into(f)));
    assert_eq!(h.parent(r), Some(f));
    let after = h.ed.scene().node(r).unwrap().world;
    for (x, y) in world.as_coeffs().iter().zip(after.as_coeffs()) {
        assert!(close(*x, y), "{world:?} → {after:?}");
    }
    assert!(
        !h.ed.move_layers(&[f], DropTarget::Into(r)),
        "в лист нельзя"
    );
    assert!(
        !h.ed.move_layers(&[f], DropTarget::Into(f)),
        "в себя нельзя"
    );
    let g = h.frame(Some(f), 0.0, 0.0, 50.0, 50.0);
    assert!(
        !h.ed.move_layers(&[f], DropTarget::Above(g)),
        "внутрь своего потомка нельзя"
    );
    assert_eq!(h.parent(g), Some(f));
}

#[test]
fn preset_size_is_one_step_for_frame_and_group() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 100.0, 100.0);
    h.ed.select(&[f]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.edit_selection(&PanelEdit::Size { w: 393.0, h: 852.0 }, Phase::Commit);
    let p = h.props(f);
    assert_eq!((p.w, p.h), (393.0, 852.0));
    h.assert_one_step(steps, &before);
    let a = h.rect(None, 500.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 530.0, 30.0, 10.0, 10.0);
    h.ed.select(&[a, b]);
    h.key("KeyG", ctrl());
    let g = h.sel()[0];
    h.ed.edit_selection(&PanelEdit::Size { w: 80.0, h: 20.0 }, Phase::Commit);
    let pose = panel_pose(h.ed.scene(), h.ed.scene().slot(g).unwrap());
    assert!(close(pose.w, 80.0) && close(pose.h, 20.0), "{pose:?}");
}

#[test]
fn drop_in_place_and_same_value_make_no_step() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 20.0, 0.0, 10.0, 10.0);
    let steps = h.ed.step_count();
    // «a» уже под «b»: бросить её под «b» — ничего не менять.
    assert!(!h.ed.move_layers(&[a], DropTarget::Below(b)));
    h.ed.select(&[a]);
    h.ed.edit_selection(&num(NumField::Opacity, 1.0), Phase::Commit);
    h.ed.set_flags(&[a], Some(true), Some(false));
    assert_eq!(
        h.ed.step_count(),
        steps,
        "то же место и те же значения — не шаг"
    );
}

#[test]
fn command_during_panel_gesture_cancels_it() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[a]);
    h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Preview);
    h.ed.execute(CommandId::Delete);
    h.ed.take_events();
    // Отпускание после команды — без ошибки «узел не найден» и без правки.
    h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Commit);
    assert!(!h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::Error(_))));
    assert!(h.children(None).is_empty());
    h.assert_scene_clean();
}

fn group_of(h: &mut H, ids: &[ferrum_doc::NodeId]) -> ferrum_doc::NodeId {
    h.ed.select(ids);
    h.key("KeyG", ctrl());
    h.sel()[0]
}

fn world_of(h: &H, id: ferrum_doc::NodeId) -> kurbo::Affine {
    h.ed.scene().node(id).unwrap().world
}

fn assert_same_affine(a: kurbo::Affine, b: kurbo::Affine) {
    for (x, y) in a.as_coeffs().iter().zip(b.as_coeffs()) {
        assert!(close(*x, y), "{a:?} ≠ {b:?}");
    }
}

#[test]
fn selected_ancestor_and_child_move_once() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 30.0, 0.0, 10.0, 10.0);
    let g = group_of(&mut h, &[a, b]);
    h.ed.select(&[g, a]);
    let (wa, wb) = (world_of(&h, a), world_of(&h, b));
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.edit_selection(&add(NumField::X, 10.0), Phase::Commit);
    assert_same_affine(world_of(&h, a), kurbo::Affine::translate((10.0, 0.0)) * wa);
    assert_same_affine(world_of(&h, b), kurbo::Affine::translate((10.0, 0.0)) * wb);
    h.assert_one_step(steps, &before);
}

#[test]
fn selected_ancestor_and_child_rotate_once() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 30.0, 0.0, 10.0, 10.0);
    let g = group_of(&mut h, &[a, b]);
    h.ed.select(&[g, a]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.edit_selection(&add(NumField::Rotation, 30.0), Phase::Commit);
    assert!(close(h.props(g).rotation, 30.0), "{}", h.props(g).rotation);
    assert!(
        close(h.props(a).rotation, 0.0),
        "ребёнок не поворачивается сам"
    );
    h.assert_one_step(steps, &before);
}

#[test]
fn selected_group_and_child_resize_once() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 30.0, 0.0, 10.0, 10.0);
    let g = group_of(&mut h, &[a, b]);
    h.ed.select(&[g, a]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::W, 80.0), Phase::Commit);
    let pose = panel_pose(h.ed.scene(), h.ed.scene().slot(g).unwrap());
    assert!(close(pose.w, 80.0), "{pose:?}");
    assert!(close(h.props(a).w, 20.0) && close(h.props(b).w, 20.0));
    assert!(close(h.props(b).x, 60.0));
    h.assert_one_step(steps, &before);
    h.ed.edit_selection(&PanelEdit::Size { w: 40.0, h: 20.0 }, Phase::Commit);
    let pose = panel_pose(h.ed.scene(), h.ed.scene().slot(g).unwrap());
    assert!(close(pose.w, 40.0) && close(pose.h, 20.0), "{pose:?}");
    assert!(close(h.props(a).w, 10.0), "{:?}", h.props(a));
}

#[test]
fn equivalent_rotation_is_not_a_step() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[r]);
    h.ed.edit_selection(&num(NumField::Rotation, 30.0), Phase::Commit);
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::Rotation, 390.0), Phase::Commit);
    h.ed.edit_selection(&num(NumField::Rotation, -330.0), Phase::Commit);
    h.ed.edit_selection(&add(NumField::Rotation, 360.0), Phase::Commit);
    assert_eq!(h.ed.step_count(), steps, "полный оборот не правка");
    assert!(close(h.props(r).rotation, 30.0));
}

#[test]
fn rotation_edit_is_one_step() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 100.0, 50.0);
    h.ed.select(&[r]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::Rotation, 45.0), Phase::Commit);
    h.assert_one_step(steps, &before);
}

#[test]
fn repeated_size_edits_do_not_drift_on_rotated_and_flipped() {
    let mut h = H::new();
    let r = h.rect(None, 10.0, 20.0, 50.0, 40.0);
    h.set(
        r,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    let flipped = h.rect(None, 200.0, 20.0, 50.0, 40.0);
    h.set(
        flipped,
        PropsPatch {
            flip_x: Some(true),
            rotation: Some(20.0),
            ..Default::default()
        },
    );
    for id in [r, flipped] {
        h.ed.select(&[id]);
        let (x, y) = (h.props(id).x, h.props(id).y);
        let corner = world_of(&h, id) * Point::ORIGIN;
        for i in 0..6 {
            h.ed.edit_selection(&num(NumField::W, 70.0 + f64::from(i)), Phase::Commit);
            h.ed.edit_selection(&num(NumField::H, 30.0 + f64::from(i)), Phase::Commit);
        }
        let p = h.props(id);
        assert_eq!((p.x, p.y, p.w, p.h), (x, y, 75.0, 35.0), "{p:?}");
        let c2 = world_of(&h, id) * Point::ORIGIN;
        if id == r {
            assert!(
                close(corner.x, c2.x) && close(corner.y, c2.y),
                "{corner:?} {c2:?}"
            );
        }
    }
}

#[test]
fn infinite_and_huge_inputs_are_rejected_or_clamped() {
    let limit = ferrum_scene::shape::COORD_LIMIT;
    let mut h = H::new();
    let r = h.rect(None, 10.0, 0.0, 50.0, 50.0);
    h.ed.select(&[r]);
    let steps = h.ed.step_count();
    for f in [
        NumField::X,
        NumField::W,
        NumField::Opacity,
        NumField::Rotation,
    ] {
        h.ed.edit_selection(&num(f, f64::INFINITY), Phase::Commit);
        h.ed.edit_selection(&add(f, f64::NEG_INFINITY), Phase::Commit);
    }
    h.ed.edit_selection(
        &PanelEdit::Size {
            w: f64::INFINITY,
            h: 5.0,
        },
        Phase::Commit,
    );
    assert_eq!(h.ed.step_count(), steps, "бесконечность — не правка");
    h.ed.edit_selection(&num(NumField::X, 1e300), Phase::Commit);
    h.ed.edit_selection(&num(NumField::W, 1e300), Phase::Commit);
    h.ed.edit_selection(&num(NumField::CornerRadius, 1e300), Phase::Commit);
    let p = h.props(r);
    assert_eq!((p.x, p.w, p.corner_radius), (limit, limit, limit));
}

#[test]
fn escape_select_undo_and_page_switch_cancel_panel_preview() {
    for case in 0..4 {
        let mut h = H::new();
        let a = h.rect(None, 10.0, 0.0, 50.0, 50.0);
        h.ed.select(&[a]);
        h.ed.rename_node(a, "Один");
        let steps = h.ed.step_count();
        let page2 = h.ed.add_page("Вторая").unwrap();
        let first = h.ed.page();
        h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Preview);
        assert_eq!(h.ed.scene().node(a).unwrap().props.x, 300.0);
        match case {
            0 => h.ed.execute(CommandId::Escape),
            1 => h.ed.select(&[a]),
            2 => h.ed.execute(CommandId::Undo),
            _ => h.ed.set_page(page2).unwrap(),
        }
        if case == 3 {
            h.ed.set_page(first).unwrap();
        }
        assert!(!h.ed.scene().has_preview(), "случай {case}");
        // Отпускание после отмены ничего не пишет.
        let steps_now = h.ed.step_count();
        if case != 2 {
            assert_eq!(steps_now, steps, "случай {case}");
        }
        h.ed.select(&[a]);
        h.ed.edit_selection(&num(NumField::X, 10.0), Phase::Commit);
        assert_eq!(h.ed.step_count(), steps_now, "случай {case}");
        assert_eq!(h.props(a).x, 10.0, "случай {case}");
        h.assert_scene_clean();
    }
}

#[test]
fn move_layers_block_into_group_and_undo() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 20.0, 0.0, 10.0, 10.0);
    let c = h.rect(None, 40.0, 0.0, 10.0, 10.0);
    let d = h.rect(None, 60.0, 0.0, 10.0, 10.0);
    let before = h.state();
    let steps = h.ed.step_count();
    // Блок {c, a} (в любом порядке на входе) встаёт над «d» в прежнем порядке.
    assert!(h.ed.move_layers(&[c, a], DropTarget::Above(d)));
    assert_eq!(h.children(None), vec![b, d, a, c]);
    assert_eq!(h.sel(), vec![a, c]);
    h.assert_one_step(steps, &before);
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.children(None), vec![a, b, c, d], "отмена вернула порядок");
    h.ed.execute(CommandId::Redo);
    h.ed.execute(CommandId::Undo);

    let x = h.rect(None, 100.0, 0.0, 10.0, 10.0);
    let y = h.rect(None, 130.0, 0.0, 10.0, 10.0);
    let g = group_of(&mut h, &[x, y]);
    let (wa, wb) = (world_of(&h, a), world_of(&h, b));
    let before = h.state();
    let steps = h.ed.step_count();
    assert!(h.ed.move_layers(&[a, b], DropTarget::Into(g)));
    assert_eq!(h.children(Some(g)), vec![x, y, a, b]);
    assert_eq!((h.parent(a), h.parent(b)), (Some(g), Some(g)));
    assert_same_affine(world_of(&h, a), wa);
    assert_same_affine(world_of(&h, b), wb);
    h.assert_one_step(steps, &before);
    h.ed.execute(CommandId::Undo);
    assert_eq!((h.parent(a), h.parent(b)), (None, None));
    assert_eq!(h.children(Some(g)), vec![x, y]);
}
