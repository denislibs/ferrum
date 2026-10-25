//! Сообщения worker'а: каждая команда и каждое событие переживают сериализацию,
//! имена полей — как ждёт TypeScript, неверные команды дают ошибку, а не панику.
//! (Тот же serde, что использует serde-wasm-bindgen в браузере; разбор настоящих
//! JS-объектов проверяет wasm-тест `tests/web.rs`.)
#![cfg(not(target_arch = "wasm32"))]

use ferrum_wasm::protocol::*;
use serde_bytes::ByteBuf;
use serde_json::json;

fn bytes(b: &[u8]) -> ByteBuf {
    ByteBuf::from(b.to_vec())
}

fn commands() -> Vec<Command> {
    vec![
        Command::Open {
            bytes: bytes(b"FERRUM"),
        },
        Command::Demo { nodes: 100_000 },
        Command::Execute {
            command: "edit.undo".into(),
        },
        Command::SetTool {
            tool: Tool::Ellipse,
        },
        Command::SetTheme {
            theme: Theme::Light,
        },
        Command::Paste { bytes: bytes(&[1]) },
        Command::PlaceImage {
            bytes: bytes(&[0x89, b'P']),
        },
        Command::Save,
        Command::ReadPixels { reference: true },
        Command::LayersWindow {
            start: 10,
            count: 40,
        },
        Command::ExpandLayer {
            id: "1@2".into(),
            expanded: true,
        },
        Command::FilterLayers {
            query: "кнопка".into(),
        },
        Command::PrepareSearch,
        Command::Select {
            ids: vec!["1@2".into()],
            mode: SelectMode::Range,
        },
        Command::HoverLayer { id: None },
        Command::RenameLayer {
            id: "1@2".into(),
            name: "Фон".into(),
        },
        Command::MoveLayers {
            ids: None,
            target: "3@4".into(),
            position: DropPosition::Into,
        },
        Command::SetLayerFlags {
            ids: vec!["1@2".into()],
            visible: Some(false),
            locked: None,
        },
        Command::EditSelection {
            edit: Edit::Number {
                field: Field::CornerRadius,
                value: 4.0,
                relative: false,
            },
            phase: EditPhase::Preview,
            ids: None,
        },
        Command::EditSelection {
            edit: Edit::Fills {
                fills: vec![FillDto {
                    kind: PaintKind::Solid,
                    color: "#ff0000ff".into(),
                    opacity: 1.0,
                    visible: true,
                    image_hash: None,
                    scale_mode: None,
                }],
            },
            phase: EditPhase::Commit,
            ids: None,
        },
        Command::EditSelection {
            edit: Edit::Strokes {
                strokes: vec![StrokeDto {
                    color: "#000000ff".into(),
                    width: 1.0,
                    align: Align::Inside,
                }],
            },
            phase: EditPhase::Cancel,
            ids: None,
        },
        Command::EditSelection {
            edit: Edit::ClipContent { value: false },
            phase: EditPhase::Commit,
            ids: None,
        },
        Command::EditSelection {
            edit: Edit::Size { w: 393.0, h: 852.0 },
            phase: EditPhase::Commit,
            ids: None,
        },
        Command::AddPage,
        Command::RenamePage {
            id: "5@6".into(),
            name: "Главная".into(),
        },
        Command::DeletePage { id: "5@6".into() },
        Command::SetPage { id: "5@6".into() },
        Command::SetPageBackground {
            color: Some("#ffffff".into()),
            phase: EditPhase::Commit,
        },
        Command::SetDocumentName {
            name: "Лендинг".into(),
        },
        Command::AddFramePreset { index: 3 },
        Command::SetZoom { zoom: 2.0 },
        Command::Export { scale: 3.0 },
        Command::PasteImage {
            bytes: bytes(&[0x89]),
        },
        Command::Bind {
            command: "tool.rect".into(),
            shortcut: "KeyQ".into(),
            replace: false,
        },
        Command::Unbind {
            command: "tool.rect".into(),
            shortcut: "KeyR".into(),
        },
        Command::ResetKeymap { command: None },
        Command::ImportKeymap { json: "{}".into() },
        Command::SetSettings { pixel_snap: false },
    ]
}

fn events() -> Vec<CoreEvent> {
    vec![
        CoreEvent::Ready {
            backend: Backend::Webgl,
            fallbacks: vec!["webgpu: нет адаптера".into()],
        },
        CoreEvent::Opened {
            pages: 2,
            nodes: 10,
            warnings: vec![],
            name: "Лендинг".into(),
        },
        CoreEvent::Selection {
            count: 2,
            ids: vec!["1@2".into(), "3@4".into()],
        },
        CoreEvent::Tool { tool: Tool::Hand },
        CoreEvent::Cursor {
            cursor: CursorKind::Resize,
            angle: 45.0,
        },
        CoreEvent::Document {
            can_undo: true,
            can_redo: false,
            steps: 3.0,
        },
        CoreEvent::Page { id: "p1".into() },
        CoreEvent::Clipboard { bytes: bytes(b"x") },
        CoreEvent::ClipboardPng { bytes: bytes(b"p") },
        CoreEvent::PasteRequested,
        CoreEvent::RequestImage,
        CoreEvent::ToggleUi,
        CoreEvent::Saved { bytes: bytes(b"s") },
        CoreEvent::Pixels {
            backend: Backend::Cpu,
            width: 1,
            height: 1,
            rgba: bytes(&[1, 2, 3, 4]),
        },
        CoreEvent::Error {
            message: "ой".into(),
        },
        CoreEvent::Layers {
            start: 0,
            total: 1,
            rows: vec![LayerRow {
                id: "1@2".into(),
                name: "Фон".into(),
                kind: NodeType::Rect,
                depth: 1,
                has_children: false,
                expanded: false,
                selected: true,
                parent_selected: false,
                visible: true,
                locked: false,
                ancestor_hidden: false,
                ancestor_locked: true,
            }],
            reveal: Some(0),
        },
        CoreEvent::Properties {
            summary: Summary::empty(),
        },
        CoreEvent::Pages {
            pages: vec![PageInfo {
                id: "5@6".into(),
                name: "Страница 1".into(),
            }],
            current: "5@6".into(),
            background: None,
        },
        CoreEvent::Keymap {
            commands: vec![KeymapEntry {
                key: "tool.rect".into(),
                name: "Прямоугольник".into(),
                group: "tool".into(),
                bindings: vec!["KeyR".into()],
                labels: vec!["R".into()],
                defaults: vec!["KeyR".into()],
                overridden: false,
            }],
            conflicts: vec![ConflictInfo {
                shortcut: "KeyR".into(),
                label: "R".into(),
                commands: vec![],
            }],
            reserved: vec!["Ctrl+KeyW".into()],
            overrides: "{}".into(),
        },
        CoreEvent::BindConflict {
            command: "a".into(),
            shortcut: "KeyR".into(),
            commands: vec!["b".into()],
        },
        CoreEvent::KeymapImported {
            warnings: vec!["пропущено".into()],
        },
        CoreEvent::Presets {
            presets: vec![PresetInfo {
                group: "Телефоны".into(),
                name: "iPhone 16".into(),
                width: 393.0,
                height: 852.0,
            }],
        },
        CoreEvent::Exported {
            name: "Фон@2x.png".into(),
            bytes: bytes(b"png"),
        },
        CoreEvent::DocumentName {
            name: "Лендинг".into(),
        },
    ]
}

#[test]
fn every_message_round_trips() {
    for c in commands() {
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(serde_json::from_value::<Command>(v).unwrap(), c);
    }
    for e in events() {
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(serde_json::from_value::<CoreEvent>(v).unwrap(), e);
    }
}

#[test]
fn field_names_match_typescript() {
    assert_eq!(
        serde_json::to_value(Command::SetTool { tool: Tool::Rect }).unwrap(),
        json!({"type": "setTool", "tool": "rect"})
    );
    assert_eq!(
        serde_json::to_value(Command::ReadPixels { reference: false }).unwrap(),
        json!({"type": "readPixels", "reference": false})
    );
    assert_eq!(
        serde_json::to_value(&events()[5]).unwrap(),
        json!({"type": "document", "canUndo": true, "canRedo": false, "steps": 3.0})
    );
    assert_eq!(
        serde_json::to_value(&events()[8]).unwrap()["type"],
        json!("clipboardPng")
    );
    assert_eq!(
        serde_json::to_value(Command::Save).unwrap(),
        json!({"type": "save"})
    );
    assert_eq!(
        serde_json::to_value(Command::EditSelection {
            edit: Edit::Number {
                field: Field::CornerRadius,
                value: 4.0,
                relative: true
            },
            phase: EditPhase::Preview,
            ids: None,
        })
        .unwrap(),
        json!({"type": "editSelection", "edit": {"type": "number", "field": "cornerRadius", "value": 4.0, "relative": true}, "phase": "preview", "ids": null})
    );
    assert_eq!(
        serde_json::to_value(Command::MoveLayers {
            ids: None,
            target: "1@2".into(),
            position: DropPosition::Above
        })
        .unwrap(),
        json!({"type": "moveLayers", "ids": null, "target": "1@2", "position": "above"})
    );
    let row = &serde_json::to_value(
        events()
            .iter()
            .find(|e| matches!(e, CoreEvent::Layers { .. }))
            .unwrap(),
    )
    .unwrap()["rows"][0];
    assert_eq!(row["hasChildren"], json!(false));
    assert_eq!(row["ancestorLocked"], json!(true));
    let s = serde_json::to_value(Summary::empty()).unwrap();
    assert_eq!(s["cornerRadius"], json!(null));
    assert_eq!(s["opacity"], json!({"value": 1.0, "mixed": false}));
}

#[test]
fn bad_commands_are_errors_not_panics() {
    for bad in [
        json!({"type": "format"}),
        json!({"type": "demo"}),
        json!({"type": "demo", "nodes": -1}),
        json!({"type": "demo", "nodes": 1, "extra": 1}),
        json!({"type": "setTool", "tool": "pen"}),
        json!({"tool": "rect"}),
        json!("save"),
        json!(null),
    ] {
        assert!(
            serde_json::from_value::<Command>(bad.clone()).is_err(),
            "принято: {bad}"
        );
    }
}

/// Команды приходят из основного потока и не доверенные: неверные типы полей,
/// значения вне диапазона и лишние поля — ошибка разбора, а не паника.
#[test]
fn malformed_commands_never_panic() {
    let bad = [
        // Неверные типы полей.
        json!({"type": "demo", "nodes": "100"}),
        json!({"type": "demo", "nodes": 1.5}),
        json!({"type": "demo", "nodes": null}),
        json!({"type": "demo", "nodes": [1]}),
        json!({"type": "execute", "command": 5}),
        json!({"type": "execute", "command": null}),
        // (Строка вместо байтов serde_bytes::ByteBuf принимает как UTF-8: это не паника
        // и не ошибка безопасности, поэтому здесь её нет.)
        json!({"type": "open", "bytes": 5}),
        json!({"type": "open", "bytes": [256]}),
        json!({"type": "open", "bytes": [-1]}),
        json!({"type": "open", "bytes": [1.5]}),
        json!({"type": "paste", "bytes": {"a": 1}}),
        json!({"type": "readPixels", "reference": "yes"}),
        json!({"type": "readPixels", "reference": 1}),
        json!({"type": "setTheme", "theme": 1}),
        json!({"type": "setTheme", "theme": "Dark"}),
        // Вне диапазона.
        json!({"type": "demo", "nodes": 4_294_967_296u64}),
        json!({"type": "demo", "nodes": 1e300}),
        json!({"type": "demo", "nodes": -0.5}),
        // Неизвестный или неверно написанный тег.
        json!({"type": "Save"}),
        json!({"type": "set_tool", "tool": "rect"}),
        json!({"type": ""}),
        json!({"type": 7}),
        json!({"type": null}),
        json!({"type": ["save"]}),
        // Лишние поля.
        // (`save` без полей — единственное исключение: serde не применяет
        // deny_unknown_fields к unit-вариантам внутренне тегированного enum, лишнее
        // поле там молча отбрасывается. Безвредно: команда ничего не читает.)
        json!({"type": "setTool", "tool": "rect", "x": 1}),
        // Команды этапа 5: неверные значения перечислений, вложенные поля, типы.
        json!({"type": "select", "ids": ["1@2"], "mode": "all"}),
        json!({"type": "select", "ids": "1@2", "mode": "replace"}),
        json!({"type": "layersWindow", "start": -1, "count": 10}),
        json!({"type": "moveLayers", "ids": null, "target": "1@2", "position": "inside"}),
        json!({"type": "editSelection", "edit": {"type": "number", "field": "z", "value": 1, "relative": false}, "phase": "commit"}),
        json!({"type": "editSelection", "edit": {"type": "number", "field": "x", "value": "1", "relative": false}, "phase": "commit"}),
        json!({"type": "editSelection", "edit": {"type": "number", "field": "x", "value": 1, "relative": false, "y": 2}, "phase": "commit"}),
        json!({"type": "editSelection", "edit": {"type": "fills", "fills": [{"kind": "solid", "color": "#fff"}]}, "phase": "commit"}),
        json!({"type": "editSelection", "edit": {"type": "fills", "fills": [{"kind": "gradient", "color": "#ffffff", "opacity": 1, "visible": true}]}, "phase": "commit"}),
        json!({"type": "editSelection", "edit": {"type": "clipContent", "value": true}, "phase": "later"}),
        json!({"type": "editSelection", "edit": {"type": "size", "w": 1}, "phase": "commit"}),
        json!({"type": "setLayerFlags", "ids": ["1@2"], "visible": "no", "locked": null}),
        json!({"type": "bind", "command": "tool.rect", "shortcut": "KeyQ"}),
        json!({"type": "export", "scale": "2"}),
        json!({"type": "addFramePreset", "index": -3}),
        json!({"type": "setSettings", "pixelSnap": 1}),
        json!({"type": "setSettings", "pixel_snap": true}),
        // Не объекты.
        json!([]),
        json!([{"type": "save"}]),
        json!(true),
        json!(1),
        json!({}),
    ];
    for b in bad {
        let r = std::panic::catch_unwind(|| serde_json::from_value::<Command>(b.clone()));
        let r = r.unwrap_or_else(|_| panic!("паника на {b}"));
        assert!(r.is_err(), "принято: {b}");
    }
    // Сырые строки JSON, не доходящие даже до значения.
    for raw in ["", "{", "{\"type\":", "\u{0}", "{\"type\":\"save\"} x"] {
        assert!(serde_json::from_str::<Command>(raw).is_err());
    }
}

#[test]
fn backend_codes_match_layout() {
    for b in [Backend::Webgpu, Backend::Webgl, Backend::Cpu] {
        assert_eq!(ferrum_wasm::layout::BACKENDS[b.code() as usize], b.name());
        assert_eq!(Backend::parse(b.name()), Some(b));
    }
    assert_eq!(Backend::parse("vulkan"), None);
}
