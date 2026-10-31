//! Ресайз текста по правилам Figma (подсистема 3, этап 2): у «авто» ширина —
//! «авто по высоте» с переносом уже в превью, высота и угол — фиксированный
//! размер; поля W и H панели — так же.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn sizing(h: &mut H, id: NodeId) -> TextSizing {
    h.props(id).text.unwrap().props.sizing
}

#[test]
fn side_handle_turns_auto_width_into_auto_height_and_rewraps_in_preview() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет большой мир", 24.0);
    let p = h.props(id);
    let (right, mid) = (100.0 + p.w, 100.0 + p.h / 2.0);
    let steps = h.ed.step_count();
    // Правая сторона — вдвое уже: в превью строки уже переносятся.
    h.drag_hold((right, mid), (100.0 + p.w / 2.0, mid), none());
    let n = h.ed.scene().node(id).unwrap();
    assert_eq!(n.props.text_props().unwrap().sizing, TextSizing::AutoHeight);
    assert!(
        n.props.h > 1.5 * p.h,
        "превью перенесло строки: {}",
        n.props.h
    );
    h.up(100.0 + p.w / 2.0, mid, none());
    let after = h.props(id);
    assert_eq!(sizing(&mut h, id), TextSizing::AutoHeight);
    assert!((after.w - p.w / 2.0).abs() <= 1.0);
    assert!(after.h > 1.5 * p.h);
    assert_eq!(h.ed.step_count(), steps + 1);
    // «Авто по высоте» по ширине остаётся собой, высота — по тексту.
    let wide = h.props(id);
    h.drag(
        (100.0 + wide.w, 100.0 + wide.h / 2.0),
        (100.0 + 3.0 * wide.w, 100.0 + wide.h / 2.0),
        none(),
    );
    assert_eq!(sizing(&mut h, id), TextSizing::AutoHeight);
    assert_eq!(h.props(id).h, p.h);
}

#[test]
fn bottom_and_corner_handles_make_text_fixed() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    let p = h.props(id);
    h.drag(
        (100.0 + p.w / 2.0, 100.0 + p.h),
        (100.0 + p.w / 2.0, 200.0),
        none(),
    );
    assert_eq!(sizing(&mut h, id), TextSizing::Fixed);
    assert_eq!(h.props(id).h, 100.0);
    h.ed.execute(CommandId::Undo);
    let p = h.props(id);
    h.drag((100.0 + p.w, 100.0 + p.h), (300.0, 300.0), none());
    assert_eq!(sizing(&mut h, id), TextSizing::Fixed);
}

#[test]
fn panel_width_is_auto_height_and_height_is_fixed() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет большой мир", 24.0);
    let p = h.props(id);
    let w = |value| PanelEdit::Number {
        field: NumField::W,
        value,
        relative: false,
    };
    // Превью поля W переносит строки так же, как запись.
    h.ed.edit_selection(&w(p.w / 2.0), Phase::Preview);
    assert!(h.ed.scene().node(id).unwrap().props.h > 1.5 * p.h);
    h.ed.edit_selection(&w(p.w / 2.0), Phase::Commit);
    assert_eq!(sizing(&mut h, id), TextSizing::AutoHeight);
    h.ed.edit_selection(
        &PanelEdit::Number {
            field: NumField::H,
            value: 300.0,
            relative: false,
        },
        Phase::Commit,
    );
    assert_eq!(sizing(&mut h, id), TextSizing::Fixed);
    assert_eq!(h.props(id).h, 300.0);
}

/// Мировая точка локального угла узла.
fn corner(h: &H, id: NodeId, x: f64, y: f64) -> kurbo::Point {
    h.ed.scene().node(id).unwrap().world * kurbo::Point::new(x, y)
}

fn set_pose(h: &mut H, id: NodeId, rotation: f64, flip_y: bool) {
    h.set(
        id,
        PropsPatch {
            rotation: Some(rotation),
            flip_y: Some(flip_y),
            ..Default::default()
        },
    );
}

#[test]
fn width_resize_keeps_the_anchor_corner_of_flipped_and_rotated_text() {
    for (rotation, flip_y) in [(0.0, true), (30.0, true), (30.0, false)] {
        let mut h = text_h();
        let id = add_text(&mut h, 100.0, 100.0, "Привет большой мир", 24.0);
        set_pose(&mut h, id, rotation, flip_y);
        let p = h.props(id);
        // Локальный (0, 0) — начало текста: при переносе строк он стоит на месте
        // (у отражённого по Y рамка растёт от него в обратную сторону).
        let start = corner(&h, id, 0.0, 0.0);
        let w = PanelEdit::Number {
            field: NumField::W,
            value: p.w / 2.0,
            relative: false,
        };
        h.ed.edit_selection(&w, Phase::Preview);
        let n = h.ed.scene().node(id).unwrap().props.clone();
        assert!(n.h > 1.5 * p.h, "{rotation} {flip_y}: {}", n.h);
        let moved = corner(&h, id, 0.0, 0.0);
        assert!(
            (moved - start).hypot() < 1e-6,
            "превью {rotation} {flip_y}: {start:?} {moved:?}"
        );
        h.ed.edit_selection(&w, Phase::Commit);
        let moved = corner(&h, id, 0.0, 0.0);
        assert!(
            (moved - start).hypot() < 1e-6,
            "запись {rotation} {flip_y}: {start:?} {moved:?}"
        );
    }
}

#[test]
fn side_handle_of_rotated_text_without_pixel_snap_is_auto_height() {
    let mut h = text_h();
    let mut st = h.ed.settings().clone();
    st.pixel_snap = false;
    h.ed.set_settings(st);
    let id = add_text(&mut h, 100.0, 100.0, "Привет большой мир", 24.0);
    set_pose(&mut h, id, 30.0, false);
    let p = h.props(id);
    let from = corner(&h, id, p.w, p.h / 2.0);
    let to = corner(&h, id, p.w / 2.0 + 0.123, p.h / 2.0);
    h.drag((from.x, from.y), (to.x, to.y), none());
    assert_eq!(sizing(&mut h, id), TextSizing::AutoHeight);
}

#[test]
fn width_resize_while_fonts_load_keeps_the_node_consistent_as_fixed() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет большой мир", 24.0);
    let p = h.props(id);
    // Новый реестр: шрифты в списке, но байты ещё не пришли.
    let pending = ferrum_text::Fonts::shared();
    let rel = "inter/Inter-Variable.ttf";
    let faces = ferrum_text::read_faces(&font_file(rel), rel);
    pending
        .borrow_mut()
        .add_catalog(ferrum_text::SourceKind::Bundled, ferrum_text::group(faces));
    h.ed.set_fonts(pending);
    h.ed.edit_selection(
        &PanelEdit::Number {
            field: NumField::W,
            value: p.w / 2.0,
            relative: false,
        },
        Phase::Commit,
    );
    let after = h.props(id);
    assert_eq!(after.w, p.w / 2.0);
    assert_eq!(sizing(&mut h, id), TextSizing::Fixed);
}
