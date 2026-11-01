//! Таблица команд (спека подсистемы 2, раздел 3): у каждой команды — группа,
//! синонимы на двух языках и признак переключателя; команды `ui.*` ядро не
//! выполняет, а сообщает о них интерфейсу.

mod common;

use std::collections::HashSet;

use common::*;
use ferrum_editor::*;

fn cyrillic(s: &str) -> bool {
    s.chars().any(|c| ('а'..='я').contains(&c) || c == 'ё')
}

fn latin(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_lowercase())
}

#[test]
fn every_command_has_group_keywords_and_unique_key() {
    let mut keys = HashSet::new();
    let mut ids = HashSet::new();
    for c in COMMANDS {
        assert!(keys.insert(c.key), "ключ {} повторяется", c.key);
        assert!(ids.insert(c.id), "{:?} повторяется", c.id);
        assert!(!c.name.is_empty(), "{:?} без названия", c.id);
        // Синонимы: хотя бы один русский и один английский, без пустых и повторов.
        assert!(
            c.keywords.iter().any(|k| cyrillic(k)),
            "{:?}: нет русских синонимов",
            c.id
        );
        assert!(
            c.keywords.iter().any(|k| latin(k) && !cyrillic(k)),
            "{:?}: нет английских синонимов",
            c.id
        );
        let unique: HashSet<_> = c.keywords.iter().collect();
        assert_eq!(
            unique.len(),
            c.keywords.len(),
            "{:?}: повтор синонима",
            c.id
        );
        for k in c.keywords {
            assert!(
                !k.is_empty() && *k == k.trim() && k.to_lowercase() == *k,
                "{:?}: синоним «{k}» — строчными и без пробелов по краям",
                c.id
            );
        }
        // Команды интерфейса — по ключу `ui.*`; у остальных ключ начинается с группы
        // (кроме «Сохранить»: `file.save` — в группе «Файл»).
        assert_eq!(c.id.is_ui(), c.key.starts_with("ui."));
        if !c.id.is_ui() {
            assert_eq!(c.key.split('.').next(), Some(c.group.key()), "{:?}", c.id);
        }
        assert_eq!(c.id.group(), c.group);
    }
    // Каждая группа меню непуста; ключи и названия групп различны.
    let groups: HashSet<_> = COMMANDS.iter().map(|c| c.group).collect();
    for g in CommandGroup::ALL {
        assert!(groups.contains(&g), "группа {g:?} пуста");
    }
    let titles: HashSet<_> = CommandGroup::ALL.iter().map(|g| g.title()).collect();
    assert_eq!(titles.len(), CommandGroup::ALL.len());
}

#[test]
fn toggles_are_exactly_the_checkable_commands() {
    use CommandId as C;
    let toggles: Vec<CommandId> = COMMANDS.iter().filter(|c| c.toggle).map(|c| c.id).collect();
    assert_eq!(
        toggles,
        vec![
            C::BooleanUnion,
            C::BooleanSubtract,
            C::BooleanIntersect,
            C::BooleanExclude,
            C::PixelGrid,
            C::UiPixelSnap,
            C::UiThemeDark,
            C::UiThemeLight,
            C::UiThemeSystem,
        ]
    );
}

#[test]
fn ui_commands_are_reported_not_performed() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a]);
    h.ed.take_events();
    let before = h.state();
    let steps = h.ed.step_count();
    for c in COMMANDS.iter().filter(|c| c.id.is_ui()) {
        h.ed.execute(c.id);
        let events = h.ed.take_events();
        assert_eq!(
            events,
            vec![EditorEvent::UiCommand(c.id)],
            "{} — только событие",
            c.key
        );
    }
    // Документ, история и выделение не тронуты.
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(h.state(), before);
    assert_eq!(h.sel(), vec![a]);
}

#[test]
fn main_menu_shortcut_reaches_the_interface() {
    for platform in [Platform::Mac, Platform::Windows] {
        let mut h = H::with_platform(platform);
        h.ed.take_events();
        h.key("Slash", alt());
        assert_eq!(
            h.ed.take_events(),
            vec![EditorEvent::UiCommand(CommandId::UiMainMenu)],
            "{platform:?}"
        );
    }
}

#[test]
fn ui_commands_keep_vector_editing() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a]);
    assert!(h.ed.start_vector_edit(a));
    h.ed.execute(CommandId::UiSettings);
    assert!(h.ed.is_editing_vector(), "правка вектора продолжается");
}
