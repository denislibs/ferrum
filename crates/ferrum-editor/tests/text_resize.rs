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
