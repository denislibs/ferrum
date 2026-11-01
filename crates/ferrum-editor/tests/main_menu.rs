//! Главное меню данными (спека подсистемы 2, раздел 4): подменю в порядке
//! Figma UI3, только существующие команды, без пустых подменю и лишних
//! разделителей.

use std::collections::HashSet;

use ferrum_editor::*;

fn titles(items: &[MenuNode]) -> Vec<&'static str> {
    items
        .iter()
        .filter_map(|n| match n {
            MenuNode::Submenu { title, .. } => Some(*title),
            _ => None,
        })
        .collect()
}

fn submenu<'a>(items: &'a [MenuNode], title: &str) -> &'a [MenuNode] {
    items
        .iter()
        .find_map(|n| match n {
            MenuNode::Submenu { title: t, items } if *t == title => Some(items.as_slice()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("нет подменю «{title}»"))
}

fn keys(items: &[MenuNode]) -> Vec<&'static str> {
    menu_commands(items).into_iter().map(|c| c.key()).collect()
}

/// Каждый уровень: не пуст, без разделителей в начале, в конце и подряд, без
/// повторов команд.
fn check_level(items: &[MenuNode], path: &str) {
    assert!(!items.is_empty(), "{path}: пусто");
    assert!(
        !matches!(items.first(), Some(MenuNode::Separator))
            && !matches!(items.last(), Some(MenuNode::Separator)),
        "{path}: разделитель с краю"
    );
    let mut seen = HashSet::new();
    for w in items.windows(2) {
        assert!(
            !(matches!(w[0], MenuNode::Separator) && matches!(w[1], MenuNode::Separator)),
            "{path}: разделители подряд"
        );
    }
    for n in items {
        match n {
            MenuNode::Command(c) => assert!(seen.insert(*c), "{path}: {c:?} дважды"),
            MenuNode::Submenu { title, items } => check_level(items, &format!("{path} › {title}")),
            MenuNode::Separator => {}
        }
    }
}

#[test]
fn top_level_follows_figma_ui3() {
    let menu = main_menu();
    assert_eq!(
        titles(&menu),
        [
            "Файл",
            "Правка",
            "Вид",
            "Объект",
            "Текст",
            "Расставить",
            "Вектор",
            "Настройки",
            "Справка"
        ]
    );
    assert_eq!(menu.len(), 9, "только подменю, без плагинов и виджетов");
    check_level(&menu, "меню");
}

#[test]
fn submenus_hold_the_commands_of_the_spec() {
    let menu = main_menu();
    assert_eq!(
        keys(submenu(&menu, "Файл")),
        [
            "ui.new_file",
            "ui.open",
            "ui.open_disk",
            "file.save",
            "ui.save_disk",
            "ui.save_copy",
            "ui.export",
            "ui.all_files"
        ]
    );
    let edit = submenu(&menu, "Правка");
    assert_eq!(
        keys(submenu(edit, "Копировать как")),
        ["edit.copy_png", "edit.copy_svg"]
    );
    for k in [
        "edit.undo",
        "edit.redo",
        "edit.duplicate",
        "edit.select_all",
    ] {
        assert!(keys(edit).contains(&k), "{k}");
    }
    let object = submenu(&menu, "Объект");
    assert_eq!(
        keys(submenu(object, "Булевы операции")),
        [
            "object.boolean_union",
            "object.boolean_subtract",
            "object.boolean_intersect",
            "object.boolean_exclude"
        ]
    );
    assert!(keys(submenu(&menu, "Вид")).contains(&"view.pixel_grid"));
    assert_eq!(keys(submenu(&menu, "Вектор")), ["vector.edit", "tool.pen"]);
    assert_eq!(
        keys(submenu(submenu(&menu, "Настройки"), "Тема")),
        ["ui.theme_light", "ui.theme_dark", "ui.theme_system"]
    );
    assert_eq!(
        keys(submenu(&menu, "Справка")),
        ["ui.shortcuts", "ui.about"]
    );
}

#[test]
fn every_menu_command_is_in_the_table_and_groups_are_covered() {
    let all: HashSet<CommandId> = menu_commands(&main_menu()).into_iter().collect();
    // Команды групп меню — все в дереве, кроме тех, что живут только на
    // клавишах: Esc, сдвиги, инструменты (у них тулбар), само главное меню.
    for c in COMMANDS {
        let only_keys = matches!(c.group, CommandGroup::Tool | CommandGroup::Nudge)
            || matches!(c.id, CommandId::Escape | CommandId::UiMainMenu);
        assert_eq!(
            all.contains(&c.id),
            !only_keys || c.id == CommandId::ToolPen,
            "{}",
            c.key
        );
    }
}
