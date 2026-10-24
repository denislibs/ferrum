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
