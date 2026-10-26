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
    (CommandId::ZoomIn, &["Equal", "Shift+Equal", "NumpadAdd"]),
    (CommandId::ZoomOut, &["Minus", "NumpadSubtract"]),
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
            // Ctrl+Alt на Windows и Linux — это AltGr: там передний и задний план
            // на Ctrl+Shift+] / Ctrl+Shift+[, как в Figma для Windows.
            if platform != Platform::Mac {
                match cmd {
                    CommandId::BringToFront => want = vec![sc("Ctrl+Shift+BracketRight")],
                    CommandId::SendToBack => want = vec![sc("Ctrl+Shift+BracketLeft")],
                    _ => {}
                }
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
    assert_eq!(COMMANDS.len(), 44);
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
    assert_eq!(
        err.conflict().map(|c| c.commands.clone()),
        Some(vec![CommandId::ToolRect])
    );
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

/// Замена у команды с полным списком (8 сочетаний) раньше снимала сочетание с
/// другой команды, а потом обрезала его у целевой: сочетание пропадало у обеих.
/// Предел проверяется до любых изменений, и ошибка ничего не меняет.
#[test]
fn bind_at_cap_fails_without_touching_other_commands() {
    let mut map = Keymap::new(Platform::Windows);
    let full: Vec<Shortcut> = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8"]
        .iter()
        .map(|s| sc(s))
        .collect();
    assert_eq!(full.len(), keymap::MAX_SHORTCUTS_PER_COMMAND);
    map.set_bindings(CommandId::ToolEllipse, full.clone());
    let before = map.export_json();
    // R занята «Прямоугольником»: с заменой и без — ошибка, раскладка как была.
    for replace in [true, false] {
        assert_eq!(
            map.bind(CommandId::ToolEllipse, sc("KeyR"), replace),
            Err(BindError::TooMany)
        );
        assert_eq!(map.bindings(CommandId::ToolRect), &[sc("KeyR")]);
        assert_eq!(map.bindings(CommandId::ToolEllipse), full.as_slice());
        assert_eq!(map.export_json(), before);
    }
    // Свободное сочетание сверх предела тоже не теряется молча.
    assert_eq!(
        map.bind(CommandId::ToolEllipse, sc("F9"), false),
        Err(BindError::TooMany)
    );
    assert_eq!(map.bindings(CommandId::ToolEllipse), full.as_slice());
    // Уже привязанное сочетание — не новое, предел не мешает.
    assert!(map.bind(CommandId::ToolEllipse, sc("F1"), false).is_ok());
    assert_eq!(map.export_json(), before);
}

/// Импорт, где все сочетания команды непонятны, раньше записывал пустой список и
/// снимал с команды все сочетания. Такая команда пропускается (остаётся
/// умолчание), а предел в 8 считается по разобранным: 8 плохих не прячут хорошее.
#[test]
fn import_with_only_bad_combos_keeps_default() {
    let mut map = Keymap::new(Platform::Windows);
    let warnings = map
        .import_json(
            r#"{"format":"ferrum-keymap","version":1,"overrides":{
                "tool.rect":["Hyper+KeyQ","Ctrl+Nope",7],
                "tool.ellipse":["b1","b2","b3","b4","b5","b6","b7","b8","KeyQ"]}}"#,
        )
        .unwrap();
    assert_eq!(map.bindings(CommandId::ToolRect), &[sc("KeyR")]);
    assert!(!map.is_overridden(CommandId::ToolRect));
    assert_eq!(map.bindings(CommandId::ToolEllipse), &[sc("KeyQ")]);
    // По предупреждению на каждое плохое сочетание (3 + 8) и одно — что команда
    // пропущена.
    assert_eq!(warnings.len(), 3 + 8 + 1, "{warnings:?}");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("tool.rect") && w.contains("пропущена")),
        "{warnings:?}"
    );
    // Явный пустой список — намеренное снятие сочетаний, он принимается.
    map.import_json(r#"{"format":"ferrum-keymap","version":1,"overrides":{"tool.rect":[]}}"#)
        .unwrap();
    assert!(map.bindings(CommandId::ToolRect).is_empty());
    // Больше 8 разобранных — лишние отбрасываются с предупреждением.
    let w = map
        .import_json(
            r#"{"format":"ferrum-keymap","version":1,"overrides":{
                "tool.rect":["F1","F2","F3","F4","F5","F6","F7","F8","F9"]}}"#,
        )
        .unwrap();
    assert_eq!(map.bindings(CommandId::ToolRect).len(), 8);
    assert!(!map.bindings(CommandId::ToolRect).contains(&sc("F9")));
    assert_eq!(w.len(), 1, "{w:?}");
}

/// AltGr на Windows приходит как Ctrl+Alt: набор символа (например, «@» на
/// немецкой раскладке — AltGr+Q) не должен вызывать команду с Ctrl+Alt.
#[test]
fn alt_graph_never_matches_mod_alt_bindings() {
    let win = Keymap::new(Platform::Windows);
    let mut e = key("KeyG", true, true, false, false);
    assert_eq!(win.lookup(&e), Some(CommandId::FrameSelection));
    e.alt_graph = true;
    assert_eq!(win.lookup(&e), None, "AltGr+G — символ, не команда");
    let mut map = Keymap::new(Platform::Linux);
    map.set_bindings(CommandId::ToolRect, vec![sc("Ctrl+Alt+KeyQ")]);
    let mut q = key("KeyQ", true, true, false, false);
    q.alt_graph = true;
    assert_eq!(map.lookup(&q), None);
    // Без Ctrl+Alt в сочетании AltGr ничему не мешает.
    let mut esc = KeyEvent::new("Escape", Modifiers::NONE);
    esc.alt_graph = true;
    assert_eq!(win.lookup(&esc), Some(CommandId::Escape));
    assert!(
        !KeyEvent::new("KeyG", Modifiers::NONE).alt_graph,
        "по умолчанию false"
    );
    // На маке AltGr нет: Option некоторые браузеры отдают как AltGraph, и правило
    // убило бы ⌥⌘] / ⌥⌘[. Там флаг сочетаниям не мешает.
    let mac = Keymap::new(Platform::Mac);
    let mut f = key("BracketRight", false, true, false, true);
    assert_eq!(mac.lookup(&f), Some(CommandId::BringToFront));
    f.alt_graph = true;
    assert_eq!(mac.lookup(&f), Some(CommandId::BringToFront));
    let mut b = key("BracketLeft", false, true, false, true);
    b.alt_graph = true;
    assert_eq!(mac.lookup(&b), Some(CommandId::SendToBack));
}

/// На Windows и Linux Ctrl+Alt совпадает с AltGr, поэтому «На передний/задний
/// план» там — Ctrl+Shift+] / Ctrl+Shift+[, как в Figma для Windows; на маке
/// остаются ⌥⌘] / ⌥⌘[.
#[test]
fn bring_to_front_defaults_per_platform() {
    let mac = Keymap::new(Platform::Mac);
    assert_eq!(
        mac.bindings(CommandId::BringToFront),
        &[sc("Meta+Alt+BracketRight")]
    );
    assert_eq!(
        mac.bindings(CommandId::SendToBack),
        &[sc("Meta+Alt+BracketLeft")]
    );
    for p in [Platform::Windows, Platform::Linux] {
        let map = Keymap::new(p);
        assert_eq!(
            map.bindings(CommandId::BringToFront),
            &[sc("Ctrl+Shift+BracketRight")]
        );
        assert_eq!(
            map.bindings(CommandId::SendToBack),
            &[sc("Ctrl+Shift+BracketLeft")]
        );
        assert!(map.conflicts().is_empty(), "{:?}", map.conflicts());
        assert_eq!(
            map.lookup(&key("BracketRight", true, false, true, false)),
            Some(CommandId::BringToFront)
        );
        assert_eq!(
            map.lookup(&key("BracketRight", true, true, false, false)),
            None
        );
    }
}

/// Приблизить и отдалить: =, Shift+= («+»), цифровой блок; без конфликтов.
#[test]
fn zoom_in_out_defaults_include_plus_and_numpad() {
    for p in [Platform::Mac, Platform::Windows] {
        let map = Keymap::new(p);
        assert_eq!(
            map.bindings(CommandId::ZoomIn),
            &[sc("Equal"), sc("Shift+Equal"), sc("NumpadAdd")]
        );
        assert_eq!(
            map.bindings(CommandId::ZoomOut),
            &[sc("Minus"), sc("NumpadSubtract")]
        );
        assert!(map.conflicts().is_empty(), "{:?}", map.conflicts());
        assert_eq!(
            map.lookup(&key("Equal", false, false, true, false)),
            Some(CommandId::ZoomIn)
        );
        assert_eq!(
            map.lookup(&key("NumpadSubtract", false, false, false, false)),
            Some(CommandId::ZoomOut)
        );
    }
}

/// На маке Ctrl+Tab и Ctrl+Shift+Tab переключают вкладки браузера (Cmd+Tab — уже
/// система, его страница не видит).
#[test]
fn mac_reserves_ctrl_tab() {
    let mac = browser_reserved(Platform::Mac);
    assert!(mac.contains(&sc("Ctrl+Tab")));
    assert!(mac.contains(&sc("Ctrl+Shift+Tab")));
    assert!(mac.contains(&sc("Meta+Tab")));
}

/// Платформа в файле экспорта (этап 6): файл с macOS на Windows меняет Cmd на Ctrl,
/// обратно — Ctrl на Cmd; та же группа платформ и старый файл без поля — как есть.
#[test]
fn import_from_another_platform_swaps_ctrl_and_cmd() {
    let mut mac = Keymap::new(Platform::Mac);
    mac.import_json(r#"{"format":"ferrum-keymap","version":1,"overrides":{"tool.rect":["Meta+KeyK","Ctrl+KeyJ"]}}"#)
        .unwrap();
    let exported = mac.export_json();
    let v: serde_json::Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(v["platform"], "mac");

    let mut win = Keymap::new(Platform::Windows);
    let warnings = win.import_json(&exported).unwrap();
    assert_eq!(
        warnings,
        vec!["Раскладка с macOS: Cmd заменён на Ctrl".to_string()]
    );
    let back: serde_json::Value = serde_json::from_str(&win.export_json()).unwrap();
    assert_eq!(back["platform"], "windows");
    assert_eq!(
        back["overrides"]["tool.rect"],
        serde_json::json!(["Ctrl+KeyK", "Meta+KeyJ"])
    );

    // Linux — та же группа, что Windows: без замены и без предупреждения.
    let mut linux = Keymap::new(Platform::Linux);
    assert!(linux.import_json(&win.export_json()).unwrap().is_empty());
    // Обратно на macOS: Ctrl → Cmd.
    let mut mac2 = Keymap::new(Platform::Mac);
    assert_eq!(
        mac2.import_json(&win.export_json()).unwrap(),
        vec!["Раскладка с Windows/Linux: Ctrl заменён на Cmd".to_string()]
    );
    let again: serde_json::Value = serde_json::from_str(&mac2.export_json()).unwrap();
    assert_eq!(
        again["overrides"]["tool.rect"],
        serde_json::json!(["Meta+KeyK", "Ctrl+KeyJ"])
    );
}

#[test]
fn import_without_or_with_unknown_platform() {
    let old = r#"{"format":"ferrum-keymap","version":1,"overrides":{"tool.rect":["Meta+KeyK"]}}"#;
    let mut win = Keymap::new(Platform::Windows);
    assert!(
        win.import_json(old).unwrap().is_empty(),
        "старый файл — как есть"
    );
    let v: serde_json::Value = serde_json::from_str(&win.export_json()).unwrap();
    assert_eq!(
        v["overrides"]["tool.rect"],
        serde_json::json!(["Meta+KeyK"])
    );
    let odd = r#"{"format":"ferrum-keymap","version":1,"platform":"amiga","overrides":{}}"#;
    assert_eq!(
        win.import_json(odd).unwrap(),
        vec!["Неизвестная платформа раскладки — сочетания как в файле".to_string()]
    );
}
