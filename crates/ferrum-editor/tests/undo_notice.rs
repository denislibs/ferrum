//! Уведомление с «Отменить» (спека подсистемы 2, раздел 10): шаг удаления, «Сплющить»,
//! «Контура обводки», разгруппировки и вставки с заменой сообщается; кнопка отменяет,
//! только если он ещё последний.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::{Color, NodeId, NodeKind, PropsPatch, Stroke, StrokeAlign};
use ferrum_editor::*;

fn notice(ev: &[EditorEvent]) -> Option<(&'static str, u64)> {
    ev.iter().find_map(|e| match e {
        EditorEvent::UndoNotice { text, step } => Some((*text, *step)),
        _ => None,
    })
}

#[test]
fn each_listed_command_reports_its_step_and_others_do_not() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(None, 200.0, 100.0, 50.0, 50.0);
    h.ed.take_events();

    h.ed.select(&[a, b]);
    h.ed.execute(CommandId::Group);
    assert_eq!(
        notice(&h.ed.take_events()),
        None,
        "группировка — без уведомления"
    );

    h.ed.execute(CommandId::Ungroup);
    let (text, step) = notice(&h.ed.take_events()).expect("разгруппировка");
    assert_eq!(text, "Разгруппировано");
    assert_eq!(Some(step), h.ed.document().last_step());

    h.ed.select(&[a, b]);
    h.ed.execute(CommandId::Flatten);
    assert_eq!(notice(&h.ed.take_events()).unwrap().0, "Сплющено");

    let c = h.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 2.0,
                align: StrokeAlign::Center,
            }]),
            ..patch(400.0, 100.0, 50.0, 50.0)
        },
    );
    h.ed.select(&[c]);
    h.ed.take_events();
    h.ed.execute(CommandId::OutlineStroke);
    assert_eq!(
        notice(&h.ed.take_events()).unwrap().0,
        "Контур обводки создан"
    );

    let sel = h.ed.selection().to_vec();
    assert!(!sel.is_empty());
    h.ed.execute(CommandId::Delete);
    assert_eq!(notice(&h.ed.take_events()).unwrap().0, "Удалено");
}

#[test]
fn nothing_to_delete_means_no_notice() {
    let mut h = H::new();
    h.ed.take_events();
    h.ed.execute(CommandId::Delete);
    assert_eq!(notice(&h.ed.take_events()), None);
}

#[test]
fn replace_paste_reports_but_plain_paste_does_not() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a]);
    h.ed.execute(CommandId::Copy);
    let bytes =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::Clipboard(b) => Some(b),
                _ => None,
            })
            .unwrap();
    h.ed.paste(&bytes).unwrap();
    assert_eq!(notice(&h.ed.take_events()), None);
    h.ed.paste_with(&bytes, PasteMode::Replace).unwrap();
    assert_eq!(notice(&h.ed.take_events()).unwrap().0, "Заменено");
    // Без выделения это обычная вставка.
    h.ed.select(&[]);
    h.ed.take_events();
    h.ed.paste_with(&bytes, PasteMode::Replace).unwrap();
    assert_eq!(notice(&h.ed.take_events()), None);
}

#[test]
fn undo_step_only_while_it_is_last() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(None, 200.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a]);
    h.ed.take_events();
    h.ed.execute(CommandId::Delete);
    let (_, step) = notice(&h.ed.take_events()).unwrap();
    assert!(!h.ed.page_doc().contains(a));

    // Другой шаг после него: кнопка ничего не делает.
    h.set(
        b,
        PropsPatch {
            x: Some(300.0),
            ..Default::default()
        },
    );
    assert!(!h.ed.undo_step(step));
    assert!(!h.ed.page_doc().contains(a));
    assert_eq!(h.props(b).x, 300.0);
    // Отменили тот шаг сами: снова верхний — удаления, кнопка его отменяет.
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.props(b).x, 200.0);
    assert!(h.ed.undo_step(step));
    assert!(h.ed.page_doc().contains(a));
    // Уже отменён: второй раз — ничего, повтора не трогает.
    assert!(!h.ed.undo_step(step));
    assert!(h.ed.document().can_redo());
    // Неизвестный номер.
    assert!(!h.ed.undo_step(step + 1000));
}

#[test]
fn delete_inside_vector_edit_shows_no_layer_notice() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[r]);
    h.ed.execute(CommandId::EditObject);
    h.ed.take_events();
    h.click(100.0, 100.0, Modifiers::NONE);
    h.ed.execute(CommandId::Delete);
    assert_eq!(notice(&h.ed.take_events()), None, "точка вектора — не слой");
}

#[test]
fn undo_step_does_not_touch_an_open_pen_path() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a]);
    h.ed.take_events();
    h.ed.execute(CommandId::Delete);
    let (_, step) = notice(&h.ed.take_events()).unwrap();
    h.ed.execute(CommandId::ToolPen);
    h.click(300.0, 300.0, Modifiers::NONE);
    h.click(400.0, 300.0, Modifiers::NONE);
    assert!(
        !h.ed.undo_step(step),
        "отмена сняла бы точку пера, а не удаление"
    );
    assert!(!h.ed.page_doc().contains(a));
}

/// Удалённый прямоугольник с уведомлением, затем правка существующего текста.
fn deleted_rect_then_text_edit() -> (H, NodeId, NodeId, u64) {
    let mut h = text_h();
    let t = add_text(&mut h, 100.0, 100.0, "Привет мир", 24.0);
    let a = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.select(&[a]);
    h.ed.take_events();
    h.ed.execute(CommandId::Delete);
    let (_, step) = notice(&h.ed.take_events()).unwrap();
    h.ed.select(&[t]);
    h.key("Enter", none());
    assert!(h.ed.is_editing_text());
    (h, t, a, step)
}

#[test]
fn undo_step_in_text_edit_works_like_ctrl_z() {
    let (mut h, t, a, step) = deleted_rect_then_text_edit();
    assert!(h.ed.undo_step(step));
    assert!(h.ed.page_doc().contains(a));
    assert!(h.ed.is_editing_text(), "правка продолжается");
    assert_eq!(
        h.sel(),
        vec![t],
        "выделение — текст в правке, а не восстановленный слой"
    );
}

#[test]
fn menu_undo_and_redo_in_text_edit_keep_the_session() {
    let (mut h, t, a, _) = deleted_rect_then_text_edit();
    h.ed.execute(CommandId::Undo);
    assert!(h.ed.page_doc().contains(a));
    assert!(h.ed.is_editing_text());
    assert_eq!(h.sel(), vec![t]);
    h.ed.execute(CommandId::Redo);
    assert!(!h.ed.page_doc().contains(a));
    assert!(h.ed.is_editing_text());
    assert_eq!(h.sel(), vec![t]);
}

#[test]
fn undo_step_with_a_text_draft_finishes_the_draft_like_ctrl_z() {
    let mut h = text_h();
    let a = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.select(&[a]);
    h.ed.take_events();
    h.ed.execute(CommandId::Delete);
    let (_, step) = notice(&h.ed.take_events()).unwrap();
    h.key("KeyT", none());
    h.click(100.0, 100.0, none());
    assert!(h.ed.is_editing_text());
    assert!(h.ed.undo_step(step));
    assert!(!h.ed.is_editing_text());
    assert!(h.ed.page_doc().contains(a));
}
