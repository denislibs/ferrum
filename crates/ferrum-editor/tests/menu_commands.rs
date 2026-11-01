//! Команды главного меню, которых не было в раскладке (спека подсистемы 2,
//! раздел 4): пиксельная сетка, «Войти в правку», форматирование текста — в
//! правке и вне её, по действующей раскладке.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn font_of(h: &mut H, id: NodeId, at: usize) -> (String, String) {
    let p = h.props(id);
    let t = p.text.unwrap();
    let f = t.content.attrs_at(at).font_name(&t.props.style);
    (f.family, f.style)
}

/// «Привет мир»: первые три символа — Roboto, остальное — Inter Regular.
fn mixed_family_text(h: &mut H) -> NodeId {
    let id = add_text(h, 100.0, 100.0, "Привет мир", 24.0);
    h.ed.transact(|tx| {
        tx.format_text(
            id,
            0..3,
            &TextAttrs {
                font: Some("Roboto".into()),
                style: Some("Regular".into()),
                ..Default::default()
            },
            &[],
        )?;
        Ok(vec![id])
    })
    .unwrap();
    id
}

#[test]
fn pixel_grid_toggles_with_shift_quote() {
    let mut h = H::new();
    assert!(h.ed.settings().pixel_grid);
    h.ed.take_events();
    h.key("Quote", shift());
    assert!(!h.ed.settings().pixel_grid);
    assert!(
        h.ed.take_events().contains(&EditorEvent::NeedsRedraw),
        "сетка — часть кадра"
    );
    h.ed.execute(CommandId::PixelGrid);
    assert!(h.ed.settings().pixel_grid);
    assert_eq!(h.ed.step_count(), 0, "вид — не правка документа");
}

#[test]
fn edit_object_enters_vector_or_text_editing() {
    let mut h = text_h();
    let r = h.rect(None, 300.0, 100.0, 50.0, 50.0);
    h.ed.select(&[r]);
    h.ed.execute(CommandId::EditObject);
    assert!(h.ed.is_editing_vector());
    // В правке — ничего: правка та же.
    h.ed.execute(CommandId::EditObject);
    assert!(h.ed.is_editing_vector());
    h.key("Escape", none());
    h.key("Escape", none());
    let t = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.ed.select(&[t]);
    h.ed.execute(CommandId::EditObject);
    assert!(h.ed.is_editing_text());
}

#[test]
fn bold_outside_editing_toggles_whole_selected_texts_in_one_step() {
    let mut h = text_h();
    let id = mixed_family_text(&mut h);
    let other = add_text(&mut h, 100.0, 300.0, "Мир", 24.0);
    h.ed.select(&[id, other]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyB", ctrl());
    assert!(!h.ed.is_editing_text());
    assert_eq!(font_of(&mut h, id, 0), ("Roboto".into(), "Bold".into()));
    assert_eq!(font_of(&mut h, id, 5), ("Inter".into(), "Bold".into()));
    assert_eq!(font_of(&mut h, other, 1).1, "Bold");
    h.assert_one_step(steps, &before);
    // Всё уже жирное — снова обычный.
    h.ed.execute(CommandId::TextBold);
    assert_eq!(font_of(&mut h, id, 0), ("Roboto".into(), "Regular".into()));
    assert_eq!(font_of(&mut h, other, 1).1, "Regular");
}

#[test]
fn underline_and_align_from_the_menu_apply_to_the_selection() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.ed.select(&[id]);
    h.ed.execute(CommandId::TextUnderline);
    let t = h.props(id).text.unwrap();
    assert_eq!(t.props.style.decoration, Decoration::Underline);
    h.ed.execute(CommandId::TextAlignRight);
    assert_eq!(h.props(id).text.unwrap().props.align_h, AlignH::Right);
    // В правке — к фрагменту, правка продолжается.
    h.key("Enter", none());
    h.ed.execute(CommandId::TextAlignCenter);
    assert_eq!(h.props(id).text.unwrap().props.align_h, AlignH::Center);
    assert!(h.ed.is_editing_text());
}

#[test]
fn text_shortcuts_in_editing_follow_the_keymap() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    // Жирный переназначен на Ctrl+Shift+B: Ctrl+B в правке больше не действует.
    h.ed.keymap_mut().set_bindings(
        CommandId::TextBold,
        vec![Shortcut::parse("Ctrl+Shift+KeyB").unwrap()],
    );
    h.ed.select(&[id]);
    h.key("Enter", none());
    h.key("KeyB", ctrl());
    assert_eq!(font_of(&mut h, id, 0).1, "Regular");
    h.key("KeyB", ctrl().with_shift());
    assert_eq!(font_of(&mut h, id, 0).1, "Bold");
    assert!(h.ed.is_editing_text());
}
