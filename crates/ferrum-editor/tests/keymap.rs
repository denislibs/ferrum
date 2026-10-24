//! Таблица команд и раскладка клавиш (раздел 7.6 спеки).

use ferrum_editor::*;

fn sc(s: &str) -> Shortcut {
    Shortcut::parse(s).unwrap()
}

fn key(code: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> KeyEvent {
    KeyEvent::new(
        code,
        Modifiers {
            shift,
            alt,
            ctrl,
            meta,
        },
    )
}

/// Все сочетания из разделов 7.1 и 7.6 спеки — как в Figma.
const EXPECTED: &[(CommandId, &[&str])] = &[
    (CommandId::ToolMove, &["KeyV"]),
    (CommandId::ToolFrame, &["KeyF", "KeyA"]),
    (CommandId::ToolRect, &["KeyR"]),
    (CommandId::ToolEllipse, &["KeyO"]),
    (CommandId::ToolLine, &["KeyL"]),
    (CommandId::ToolHand, &["KeyH"]),
    (CommandId::HandHold, &["Space"]),
    (CommandId::ToolImage, &["Mod+Shift+KeyK"]),
    (CommandId::Delete, &["Delete", "Backspace"]),
    (CommandId::Duplicate, &["Mod+KeyD"]),
    (CommandId::Group, &["Mod+KeyG"]),
    (CommandId::Ungroup, &["Mod+Shift+KeyG"]),
    (CommandId::FrameSelection, &["Mod+Alt+KeyG"]),
    (CommandId::BringForward, &["Mod+BracketRight"]),
    (CommandId::SendBackward, &["Mod+BracketLeft"]),
    (CommandId::BringToFront, &["Mod+Alt+BracketRight"]),
    (CommandId::SendToBack, &["Mod+Alt+BracketLeft"]),
    (CommandId::NudgeLeft, &["ArrowLeft"]),
    (CommandId::NudgeRight, &["ArrowRight"]),
    (CommandId::NudgeUp, &["ArrowUp"]),
    (CommandId::NudgeDown, &["ArrowDown"]),
    (CommandId::NudgeLeftBig, &["Shift+ArrowLeft"]),
    (CommandId::NudgeRightBig, &["Shift+ArrowRight"]),
    (CommandId::NudgeUpBig, &["Shift+ArrowUp"]),
    (CommandId::NudgeDownBig, &["Shift+ArrowDown"]),
    (CommandId::ZoomToFit, &["Shift+Digit1"]),
    (CommandId::ZoomToSelection, &["Shift+Digit2"]),
    (CommandId::Zoom100, &["Shift+Digit0"]),
    (CommandId::ToggleVisible, &["Mod+Shift+KeyH"]),
    (CommandId::ToggleLock, &["Mod+Shift+KeyL"]),
    (CommandId::FlipHorizontal, &["Shift+KeyH"]),
    (CommandId::FlipVertical, &["Shift+KeyV"]),
    (CommandId::ToggleUi, &["Mod+Backslash"]),
    (CommandId::Copy, &["Mod+KeyC"]),
    (CommandId::Paste, &["Mod+KeyV"]),
    (CommandId::Cut, &["Mod+KeyX"]),
    (CommandId::CopyAsPng, &["Mod+Shift+KeyC"]),
    (CommandId::Undo, &["Mod+KeyZ"]),
    (CommandId::Redo, &["Mod+Shift+KeyZ"]),
    (CommandId::SelectAll, &["Mod+KeyA"]),
    (CommandId::Escape, &["Escape"]),
];

#[test]
fn defaults_match_figma_on_both_platforms() {
    for (platform, primary) in [(Platform::Mac, "Meta"), (Platform::Windows, "Ctrl")] {
        let map = Keymap::new(platform);
        for (cmd, list) in EXPECTED {
            let mut want: Vec<Shortcut> = list
                .iter()
                .map(|s| sc(&s.replace("Mod", primary)))
                .collect();
            if *cmd == CommandId::Redo && platform != Platform::Mac {
                want.push(sc("Ctrl+KeyY"));
            }
            assert_eq!(
                map.bindings(*cmd),
                want.as_slice(),
                "{cmd:?} на {platform:?}"
            );
        }
        assert!(map.conflicts().is_empty(), "{:?}", map.conflicts());
        assert!(
            map.browser_conflicts().is_empty(),
            "умолчания не задевают сочетаний браузера"
        );
    }
    // Каждая команда таблицы — с названием, ключом и сочетанием.
    let map = Keymap::new(Platform::Linux);
    for c in COMMANDS {
        assert!(!c.name.is_empty() && CommandId::from_key(c.key) == Some(c.id));
        assert!(!map.bindings(c.id).is_empty(), "{:?}", c.id);
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 43);
}

#[test]
fn lookup_by_physical_code_with_exact_modifiers() {
    let mac = Keymap::new(Platform::Mac);
    assert_eq!(
        mac.lookup(&key("KeyD", false, false, false, true)),
        Some(CommandId::Duplicate)
    );
    assert_eq!(
        mac.lookup(&key("KeyD", true, false, false, false)),
        None,
        "Ctrl на маке — не Cmd"
    );
    let win = Keymap::new(Platform::Windows);
    // Русская раскладка: «в» на клавише KeyD.
    let mut e = key("KeyD", true, false, false, false);
    e.key = "в".into();
    assert_eq!(win.lookup(&e), Some(CommandId::Duplicate));
    assert_eq!(
        win.lookup(&key("Digit1", false, false, true, false)),
        Some(CommandId::ZoomToFit)
    );
    // Модификаторы — точным набором: Shift+V — отражение, а не «Перемещение» (V),
    // Shift+H — отражение, а не «Рука» (H) и не «Скрыть» (Ctrl+Shift+H).
    assert_eq!(
        win.lookup(&key("KeyV", false, false, true, false)),
        Some(CommandId::FlipVertical)
    );
    assert_eq!(
        win.lookup(&key("KeyV", false, false, false, false)),
        Some(CommandId::ToolMove)
    );
    assert_eq!(
        win.lookup(&key("KeyH", false, false, true, false)),
        Some(CommandId::FlipHorizontal)
    );
    assert_eq!(
        win.lookup(&key("KeyH", false, false, false, false)),
        Some(CommandId::ToolHand)
    );
    assert_eq!(
        win.lookup(&key("KeyH", true, false, true, false)),
        Some(CommandId::ToggleVisible)
    );
    assert_eq!(
        win.lookup(&key("KeyV", true, false, false, false)),
        Some(CommandId::Paste)
    );
    // Shift без своей команды не падает на команду без Shift: Shift+R — не «Прямоугольник».
    assert_eq!(win.lookup(&key("KeyR", false, false, true, false)), None);
    assert_eq!(
        Keymap::new(Platform::Mac).lookup(&key("KeyR", false, false, true, false)),
        None
    );
}

#[test]
fn bind_conflicts_replace_unbind_and_reset() {
    let mut map = Keymap::new(Platform::Windows);
    // R занята «Прямоугольником».
    let err = map
        .bind(CommandId::ToolEllipse, sc("KeyR"), false)
        .unwrap_err();
    assert_eq!(err.commands, vec![CommandId::ToolRect]);
    assert_eq!(map.bindings(CommandId::ToolEllipse), &[sc("KeyO")]);
    // С заменой сочетание переходит к эллипсу.
    map.bind(CommandId::ToolEllipse, sc("KeyR"), true).unwrap();
    assert_eq!(
        map.bindings(CommandId::ToolEllipse),
        &[sc("KeyO"), sc("KeyR")]
    );
    assert!(map.bindings(CommandId::ToolRect).is_empty());
    assert!(map.is_overridden(CommandId::ToolRect));
    assert_eq!(
        map.lookup(&key("KeyR", false, false, false, false)),
        Some(CommandId::ToolEllipse)
    );
    // Сброс одной команды возвращает умолчание; конфликт теперь виден.
    map.reset(CommandId::ToolRect);
    assert_eq!(map.bindings(CommandId::ToolRect), &[sc("KeyR")]);
    assert_eq!(map.conflicts().len(), 1);
    assert_eq!(
        map.conflicts()[0].commands,
        vec![CommandId::ToolRect, CommandId::ToolEllipse]
    );
    // Убрать лишнее — список снова равен умолчанию и перестаёт быть отличием.
    map.unbind(CommandId::ToolEllipse, &sc("KeyR"));
    assert!(!map.is_overridden(CommandId::ToolEllipse));
    map.set_bindings(CommandId::Duplicate, vec![]);
    assert_eq!(map.lookup(&key("KeyD", true, false, false, false)), None);
    map.reset_all();
    assert_eq!(map.bindings(CommandId::Duplicate), &[sc("Ctrl+KeyD")]);
    assert!(!map.is_overridden(CommandId::Duplicate));
}

#[test]
fn export_import_round_trip_stores_only_differences() {
    let mut map = Keymap::new(Platform::Mac);
    assert!(map.export_json().contains("\"overrides\": {}"));
    map.set_bindings(CommandId::ToolRect, vec![sc("KeyQ"), sc("Shift+KeyR")]);
    map.set_bindings(CommandId::Duplicate, vec![]);
    let json = map.export_json();
    let mut other = Keymap::new(Platform::Mac);
    assert!(other.import_json(&json).unwrap().is_empty());
    for c in COMMANDS {
        assert_eq!(other.bindings(c.id), map.bindings(c.id), "{:?}", c.id);
    }
    assert_eq!(other.export_json(), json);
    // Неизвестные команды и непонятные сочетания — предупреждения, остальное принято.
    let warnings = other
        .import_json(
            r#"{"format":"ferrum-keymap","version":1,"overrides":{
                "tool.rect":["KeyW","Hyper+KeyQ",5],"no.such":["KeyZ"],"tool.line":"KeyL"}}"#,
        )
        .unwrap();
    assert_eq!(warnings.len(), 4, "{warnings:?}");
    assert_eq!(other.bindings(CommandId::ToolRect), &[sc("KeyW")]);
    assert!(
        !other.is_overridden(CommandId::Duplicate),
        "импорт заменяет отличия целиком"
    );
    // Ошибки формата не меняют раскладку.
    let snapshot = other.export_json();
    for bad in [
        "not json",
        "[]",
        r#"{"format":"other","version":1,"overrides":{}}"#,
        r#"{"format":"ferrum-keymap","overrides":{}}"#,
        r#"{"format":"ferrum-keymap","version":1}"#,
    ] {
        assert!(
            matches!(other.import_json(bad), Err(KeymapError::BadFormat(_))),
            "{bad}"
        );
        assert_eq!(other.export_json(), snapshot);
    }
    assert!(matches!(
        other.import_json(r#"{"format":"ferrum-keymap","version":2,"overrides":{}}"#),
        Err(KeymapError::NewerVersion(2))
    ));
    let huge = format!(
        r#"{{"format":"ferrum-keymap","version":1,"overrides":{{"tool.rect":["{}"]}}}}"#,
        "A".repeat(70_000)
    );
    assert_eq!(other.import_json(&huge), Err(KeymapError::TooLarge));
    assert_eq!(other.export_json(), snapshot);
}

#[test]
fn browser_reserved_combos_are_reported() {
    let reserved = browser_reserved(Platform::Windows);
    assert!(reserved.contains(&sc("Ctrl+KeyW")));
    assert!(reserved.contains(&sc("Ctrl+KeyT")));
    assert!(reserved.contains(&sc("Ctrl+KeyN")));
    assert!(browser_reserved(Platform::Mac).contains(&sc("Meta+KeyW")));
    let mut map = Keymap::new(Platform::Windows);
    map.bind(CommandId::Duplicate, sc("Ctrl+KeyW"), false)
        .unwrap();
    assert_eq!(
        map.browser_conflicts(),
        vec![(CommandId::Duplicate, sc("Ctrl+KeyW"))]
    );
}

#[test]
fn shortcut_text_forms() {
    let s = sc("Shift+Ctrl+KeyZ");
    assert_eq!(s.to_string(), "Ctrl+Shift+KeyZ", "каноническая запись");
    assert_eq!(sc("Cmd+KeyK"), sc("Meta+KeyK"));
    assert_eq!(sc("Meta+Shift+KeyK").label(Platform::Mac), "⇧⌘K");
    assert_eq!(
        sc("Ctrl+Alt+BracketRight").label(Platform::Windows),
        "Ctrl+Alt+]"
    );
    for bad in [
        "",
        "Ctrl+",
        "Ctrl+Ctrl+KeyA",
        "KeyAA",
        "Shift+ShiftLeft",
        "Super+KeyA",
        "F25",
    ] {
        assert!(Shortcut::parse(bad).is_err(), "{bad}");
    }
    for good in ["F12", "Digit0", "Numpad5", "Space", "Alt+Backquote"] {
        assert!(Shortcut::parse(good).is_ok(), "{good}");
    }
}
