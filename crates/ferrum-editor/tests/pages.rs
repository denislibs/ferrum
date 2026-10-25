//! Страницы из панели (этап 5): создать и переключиться, переименовать, удалить
//! текущую с переходом на соседнюю, фон страницы. В историю отмены не попадают.

mod common;

use common::*;
use ferrum_doc::{Color, DocError};
use ferrum_editor::*;

#[test]
fn create_page_switches_and_picks_free_name() {
    let mut h = H::new();
    let first = h.ed.page();
    h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.take_events();
    let p2 = h.ed.create_page().unwrap();
    assert_eq!(h.ed.page(), p2, "новая страница — текущая");
    assert!(h.ed.scene().is_empty());
    assert!(h.ed.take_events().contains(&EditorEvent::PageChanged(p2)));
    h.ed.rename_page(p2, "Страница 3").unwrap();
    let p3 = h.ed.create_page().unwrap();
    let names: Vec<String> = h.ed.document().pages().into_iter().map(|p| p.1).collect();
    assert_eq!(
        names,
        ["Страница 1", "Страница 3", "Страница 4"],
        "имя не повторяется"
    );
    assert_ne!(p3, first);
}

#[test]
fn rename_cleans_name_and_ignores_empty() {
    let mut h = H::new();
    let p = h.ed.page();
    h.ed.rename_page(p, "  Экраны\nвхода\t ").unwrap();
    assert_eq!(h.ed.document().pages()[0].1, "Экраны входа");
    h.ed.rename_page(p, "   ").unwrap();
    assert_eq!(
        h.ed.document().pages()[0].1,
        "Экраны входа",
        "пустое имя не меняет"
    );
    let long = "я".repeat(1000);
    h.ed.rename_page(p, &long).unwrap();
    assert_eq!(h.ed.document().pages()[0].1.chars().count(), MAX_NAME_CHARS);
    assert_eq!(clean_name("\u{7}"), None);
}

#[test]
fn delete_current_page_moves_to_neighbour() {
    let mut h = H::new();
    let p1 = h.ed.page();
    let p2 = h.ed.create_page().unwrap();
    let p3 = h.ed.create_page().unwrap();
    h.ed.set_page(p2).unwrap();
    h.ed.delete_page(p2).unwrap();
    assert_eq!(h.ed.page(), p3, "следующая");
    h.ed.delete_page(p3).unwrap();
    assert_eq!(h.ed.page(), p1, "у последней — предыдущая");
    assert!(matches!(h.ed.delete_page(p1), Err(DocError::LastPage)));
    assert_eq!(h.ed.document().pages().len(), 1);
}

#[test]
fn delete_other_page_keeps_current_and_selection() {
    let mut h = H::new();
    let p1 = h.ed.page();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let p2 = h.ed.create_page().unwrap();
    h.ed.set_page(p1).unwrap();
    h.ed.select(&[a]);
    h.ed.delete_page(p2).unwrap();
    assert_eq!(h.ed.page(), p1);
    assert_eq!(h.sel(), vec![a]);
}

#[test]
fn background_redraws_current_page_only() {
    let mut h = H::new();
    let p1 = h.ed.page();
    let p2 = h.ed.add_page("Фон").unwrap();
    h.ed.take_events();
    let c = Color::parse("#102030").unwrap();
    h.ed.set_page_background(p2, Some(c)).unwrap();
    assert!(!h.ed.take_events().contains(&EditorEvent::NeedsRedraw));
    assert_eq!(h.ed.page_background(), None);
    h.ed.set_page_background(p1, Some(c)).unwrap();
    assert!(h.ed.take_events().contains(&EditorEvent::NeedsRedraw));
    assert_eq!(h.ed.page_background(), Some(c));
    assert!(!h.ed.document().can_undo(), "фон не в истории");
}

#[test]
fn document_name_is_cleaned() {
    let mut h = H::new();
    assert_eq!(h.ed.set_document_name("  Лендинг\n ").unwrap(), "Лендинг");
    assert_eq!(
        h.ed.set_document_name("").unwrap(),
        "Лендинг",
        "пустое не меняет"
    );
    assert_eq!(h.ed.document().name(), "Лендинг");
}
