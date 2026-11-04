//! Контекстные меню (спека подсистемы 2, раздел 5): выбор на правой кнопке, как в
//! Figma, состав меню холста по выделению, пустого места, строки слоя и
//! страницы; «Дублировать страницу».

mod common;

use std::collections::HashSet;

use common::text::*;
use common::*;
use ferrum_doc::{Color, NodeKind};
use ferrum_editor::*;
use kurbo::Point;

fn canvas(h: &mut H, x: f64, y: f64) -> Vec<MenuNode> {
    h.ed.context_menu(ContextTarget::Canvas(Point::new(x, y)))
}

fn keys(items: &[MenuNode]) -> Vec<&'static str> {
    items
        .iter()
        .filter_map(|n| match n {
            MenuNode::Command(c) => Some(c.key()),
            _ => None,
        })
        .collect()
}

fn submenu<'a>(items: &'a [MenuNode], title: &str) -> Option<&'a [MenuNode]> {
    items.iter().find_map(|n| match n {
        MenuNode::Submenu { title: t, items } if *t == title => Some(items.as_slice()),
        _ => None,
    })
}

fn actions(items: &[MenuNode]) -> Vec<(MenuAction, bool)> {
    items
        .iter()
        .filter_map(|n| match n {
            MenuNode::Action { action, enabled } => Some((*action, *enabled)),
            _ => None,
        })
        .collect()
}

/// Уровень меню: не пуст, без разделителей с краю и подряд, без повторов команд.
fn check_level(items: &[MenuNode]) {
    assert!(!items.is_empty());
    assert!(!matches!(items.first(), Some(MenuNode::Separator)));
    assert!(!matches!(items.last(), Some(MenuNode::Separator)));
    for w in items.windows(2) {
        assert!(!(matches!(w[0], MenuNode::Separator) && matches!(w[1], MenuNode::Separator)));
    }
    let mut seen = HashSet::new();
    for n in items {
        match n {
            MenuNode::Command(c) => assert!(seen.insert(*c), "{c:?} дважды"),
            MenuNode::Submenu { items, .. } => check_level(items),
            _ => {}
        }
    }
}

#[test]
fn right_click_selects_an_unselected_node_and_keeps_a_selected_one() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 300.0, 100.0, 100.0, 100.0);
    let menu = canvas(&mut h, 150.0, 150.0);
    assert_eq!(h.sel(), [a], "невыделенный узел под точкой выделяется");
    check_level(&menu);
    // Выделены оба: правый щелчок по одному из них сохраняет выделение.
    h.ed.select(&[a, b]);
    canvas(&mut h, 350.0, 150.0);
    assert_eq!(h.sel(), [a, b]);
    // Пустое место внутри общей рамки выделения — тоже.
    canvas(&mut h, 250.0, 150.0);
    assert_eq!(h.sel(), [a, b]);
    // Пустое место вне рамки снимает выделение: меню пустого места.
    let empty = canvas(&mut h, 600.0, 600.0);
    assert!(h.sel().is_empty());
    assert_eq!(
        keys(&empty),
        [
            "edit.paste",
            "edit.paste_over",
            "edit.select_all",
            "view.fit_all",
            "view.rulers"
        ]
    );
    check_level(&empty);
}

#[test]
fn canvas_menu_follows_figma_block_order() {
    let mut h = H::new();
    h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let menu = canvas(&mut h, 150.0, 150.0);
    assert_eq!(
        keys(&menu),
        [
            "edit.copy",
            "edit.paste",
            "edit.paste_over",
            "edit.paste_replace",
            "edit.copy_props",
            "edit.paste_props",
            "arrange.forward",
            "arrange.backward",
            "arrange.front",
            "arrange.back",
            "object.group",
            "object.ungroup",
            "object.frame_selection",
            "object.flatten",
            "object.outline_stroke",
            "object.flip_horizontal",
            "object.flip_vertical",
            "object.toggle_visible",
            "object.toggle_lock",
            "vector.edit",
            "edit.delete",
        ]
    );
    let titles: Vec<&str> = menu
        .iter()
        .filter_map(|n| match n {
            MenuNode::Submenu { title, .. } => Some(*title),
            _ => None,
        })
        .collect();
    assert_eq!(
        titles,
        ["Копировать как", "Выделить слой", "Булевы операции"]
    );
    assert_eq!(
        keys(submenu(&menu, "Копировать как").unwrap()),
        ["edit.copy_png", "edit.copy_svg"]
    );
    assert!(
        actions(&menu).is_empty(),
        "у холста нет действий строки слоя"
    );
    // Каждая команда меню — из таблицы.
    for k in keys(&menu) {
        assert!(CommandId::from_key(k).is_some(), "{k}");
    }
}

#[test]
fn select_layer_lists_every_node_under_the_point_top_first() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    let low = h.rect(Some(f), 50.0, 50.0, 200.0, 200.0);
    let high = h.rect(Some(f), 100.0, 100.0, 50.0, 50.0);
    h.set(
        high,
        ferrum_doc::PropsPatch {
            name: Some("Верхний".into()),
            ..Default::default()
        },
    );
    let menu = canvas(&mut h, 120.0, 120.0);
    let under: Vec<(ferrum_doc::NodeId, String)> = submenu(&menu, "Выделить слой")
        .unwrap()
        .iter()
        .map(|n| match n {
            MenuNode::Node { id, name } => (*id, name.clone()),
            other => panic!("{other:?}"),
        })
        .collect();
    let ids: Vec<_> = under.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        ids,
        [high, f, low],
        "сверху вниз, контейнер — после ребёнка"
    );
    assert_eq!(under[0].1, "Верхний");
    // Ребёнок фрейма верхнего уровня выбирается щелчком, как левой кнопкой.
    assert_eq!(h.sel(), [high]);
}

#[test]
fn edit_object_only_for_vectors_and_shapes() {
    let mut h = H::new();
    let f = h.frame(None, 500.0, 100.0, 100.0, 100.0);
    h.rect(None, 100.0, 100.0, 100.0, 100.0);
    assert!(keys(&canvas(&mut h, 150.0, 150.0)).contains(&"vector.edit"));
    h.ed.select(&[f]);
    let menu = h.ed.context_menu(ContextTarget::Layer(f));
    assert!(!keys(&menu).contains(&"vector.edit"));
}

#[test]
fn layer_menu_selects_the_row_and_adds_rename_and_expand() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 300.0, 100.0, 100.0, 100.0);
    h.ed.select(&[a]);
    let menu = h.ed.context_menu(ContextTarget::Layer(b));
    assert_eq!(h.sel(), [b], "невыделенная строка выделяется");
    check_level(&menu);
    assert!(submenu(&menu, "Выделить слой").is_none());
    assert_eq!(
        actions(&menu),
        [
            (MenuAction::RenameLayer, true),
            (MenuAction::ExpandAll, true),
            (MenuAction::CollapseAll, true)
        ]
    );
    assert_eq!(keys(&menu).last(), Some(&"edit.delete"));
    // Выделенная строка выделение сохраняет.
    h.ed.select(&[a, b]);
    h.ed.context_menu(ContextTarget::Layer(a));
    assert_eq!(h.sel(), [a, b]);
}

#[test]
fn page_menu_actions_and_their_state() {
    let mut h = H::new();
    let first = h.ed.page();
    let menu = h.ed.context_menu(ContextTarget::Page(first));
    check_level(&menu);
    assert_eq!(
        actions(&menu),
        [
            (MenuAction::RenamePage, true),
            (MenuAction::DuplicatePage, true),
            (MenuAction::DeletePage, false),
            (MenuAction::CopyPageLink, true)
        ],
        "единственную страницу удалить нельзя"
    );
    h.ed.create_page().unwrap();
    let menu = h.ed.context_menu(ContextTarget::Page(first));
    assert!(actions(&menu).contains(&(MenuAction::DeletePage, true)));
    // Только для чтения: правки страниц и слоя недоступны, ссылка — доступна.
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    let menu = h.ed.context_menu(ContextTarget::Page(first));
    assert_eq!(
        actions(&menu),
        [
            (MenuAction::RenamePage, false),
            (MenuAction::DuplicatePage, false),
            (MenuAction::DeletePage, false),
            (MenuAction::CopyPageLink, true)
        ]
    );
}

#[test]
fn unknown_targets_give_no_menu() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let page = h.ed.page();
    h.ed.transact(|tx| {
        tx.delete_nodes(&[a])?;
        Ok(vec![])
    })
    .unwrap();
    assert!(h.ed.context_menu(ContextTarget::Layer(a)).is_empty());
    h.ed.create_page().unwrap();
    h.ed.delete_page(page).unwrap();
    assert!(h.ed.context_menu(ContextTarget::Page(page)).is_empty());
}

#[test]
fn right_click_in_text_or_vector_edit_keeps_the_edit() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    h.ed.select(&[r]);
    h.ed.execute(CommandId::EditObject);
    assert!(h.ed.vector_edit_node().is_some());
    canvas(&mut h, 600.0, 600.0);
    assert_eq!(h.ed.vector_edit_node(), Some(r), "правка вектора идёт");
    assert_eq!(h.sel(), [r]);
}

#[test]
fn right_click_in_text_edit_keeps_the_edit() {
    let mut h = text_h();
    let r = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    let t = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.ed.select(&[t]);
    h.key("Enter", Modifiers::NONE);
    assert!(h.ed.is_editing_text());
    // Правый щелчок на пустом месте и на другом узле: правка и выделение целы.
    canvas(&mut h, 900.0, 900.0);
    assert!(h.ed.is_editing_text(), "правка текста идёт");
    assert_eq!(h.sel(), [t]);
    canvas(&mut h, 420.0, 420.0);
    assert!(h.ed.is_editing_text());
    assert_eq!(h.sel(), [t], "другой узел не выделен: {r:?}");
}

#[test]
fn duplicate_page_copies_nodes_after_the_source_and_opens_it() {
    let mut h = H::new();
    let first = h.ed.page();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    h.rect(Some(f), 10.0, 10.0, 50.0, 50.0);
    h.ed.set_page_background(first, Some(Color::rgba(10, 20, 30, 255)))
        .unwrap();
    let last = h.ed.create_page().unwrap();
    h.ed.set_page(first).unwrap();
    let steps = h.ed.step_count();
    let before = h.state();
    let copy = h.ed.duplicate_page(first).unwrap();
    let pages = h.ed.document().pages();
    assert_eq!(
        pages.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        [first, copy, last],
        "копия — сразу после страницы"
    );
    assert_eq!(pages[1].1, "Страница 1 (копия)");
    assert_eq!(h.ed.page(), copy, "копия — текущая");
    assert_eq!(h.ed.step_count(), steps + 1, "один шаг отмены");
    assert_eq!(h.ed.page_background(), Some(Color::rgba(10, 20, 30, 255)));
    let after = h.state();
    assert_eq!(after.len(), before.len());
    for ((id_a, parent_a, pa), (id_b, parent_b, pb)) in before.iter().zip(&after) {
        assert_ne!(id_a, id_b, "у копий новые идентификаторы");
        assert_eq!(parent_a.is_some(), parent_b.is_some());
        assert_eq!(
            (pa.kind, pa.x, pa.y, pa.w, pa.h),
            (pb.kind, pb.x, pb.y, pb.w, pb.h)
        );
    }
    assert_eq!(after[0].2.kind, NodeKind::Frame);
    // Узлы копии правятся отдельно от оригинала.
    let (child, _, _) = after[1].clone();
    h.set(child, patch(200.0, 200.0, 50.0, 50.0));
    h.ed.set_page(first).unwrap();
    assert_eq!(h.state(), before);
}

#[test]
fn duplicate_page_undo_returns_and_redo_opens_the_copy() {
    let mut h = H::new();
    let first = h.ed.page();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    h.ed.select(&[f]);
    let copy = h.ed.duplicate_page(first).unwrap();
    h.ed.take_events();
    assert_eq!(h.ed.page(), copy);
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.ed.page(), first, "копия была текущей: назад на исходную");
    assert_eq!(h.ed.selection(), [f], "выделение на момент дубля");
    assert_eq!(h.ed.document().pages().len(), 1);
    let ev = h.ed.take_events();
    assert!(ev.contains(&EditorEvent::PagesChanged), "{ev:?}");
    assert!(ev
        .iter()
        .any(|e| matches!(e, EditorEvent::PageChanged(p) if *p == first)));
    h.ed.execute(CommandId::Redo);
    assert_eq!(h.ed.document().pages().len(), 2);
    assert_eq!(h.ed.page(), h.ed.document().pages()[1].0);
    assert!(h.ed.take_events().contains(&EditorEvent::PagesChanged));
    assert_eq!(h.state().len(), 1, "сцена копии на месте");
}
