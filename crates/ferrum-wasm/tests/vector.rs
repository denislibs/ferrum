//! Векторы в ядре worker'а (подсистема 3, этап 3): инструмент «Перо» и курсор,
//! строка слоя вектора, сводка стиля обводки, правка окончаний и соединения из
//! панели с проверкой недоверенного ввода.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;

fn core() -> Core {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::Demo { nodes: 0 });
    c.take_events();
    c
}

fn click(c: &mut Core, x: f64, y: f64, t: f64) {
    let e = PointerEvent {
        point: (x, y).into(),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t,
    };
    c.handle(Input::PointerDown(e));
    c.handle(Input::PointerUp(e));
}

fn key(c: &mut Core, code: &str) {
    c.handle(Input::KeyDown(KeyEvent::new(code, Modifiers::NONE)));
    c.handle(Input::KeyUp(KeyEvent::new(code, Modifiers::NONE)));
}

fn summary(ev: &[CoreEvent]) -> Option<Summary> {
    ev.iter().rev().find_map(|e| match e {
        CoreEvent::Properties { summary } => Some(summary.clone()),
        _ => None,
    })
}

fn errors(ev: &[CoreEvent]) -> Vec<String> {
    ev.iter()
        .filter_map(|e| match e {
            CoreEvent::Error { message } => Some(message.clone()),
            _ => None,
        })
        .collect()
}

fn style(edit: Edit) -> Command {
    Command::EditSelection {
        edit,
        phase: EditPhase::Commit,
        ids: None,
        selection_version: None,
    }
}

/// Открытый путь пером из трёх точек, выделен.
fn pen_path(c: &mut Core) {
    c.command(Command::SetTool { tool: Tool::Pen });
    click(c, 100.0, 100.0, 1000.0);
    click(c, 200.0, 100.0, 2000.0);
    click(c, 200.0, 200.0, 3000.0);
    key(c, "Enter");
}

#[test]
fn pen_makes_a_vector_layer_and_its_stroke_style_summary() {
    let mut c = core();
    c.command(Command::SetTool { tool: Tool::Pen });
    let ev = c.take_events();
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Tool { tool: Tool::Pen })));
    assert!(ev.iter().any(|e| matches!(
        e,
        CoreEvent::Cursor {
            cursor: CursorKind::Pen,
            ..
        }
    )));
    pen_path(&mut c);
    let ev = c.take_events();
    let s = summary(&ev).expect("сводка");
    assert_eq!(s.kind, Some(NodeType::Vector));
    let st = s.stroke_style.expect("стиль обводки");
    assert!(st.ends && st.corners);
    assert_eq!(st.cap_start, Some(StrokeCapDto::None));
    assert_eq!(st.join, Some(StrokeJoinDto::Miter));
    assert_eq!(st.miter_limit.value, 4.0);
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Tool { tool: Tool::Move })));
}

#[test]
fn stroke_style_edit_applies_and_bad_values_are_refused() {
    let mut c = core();
    pen_path(&mut c);
    c.take_events();
    c.command(style(Edit::StrokeStyle {
        cap_start: None,
        cap_end: Some(StrokeCapDto::ArrowTriangle),
        join: Some(StrokeJoinDto::Round),
        miter_limit: None,
    }));
    let st = summary(&c.take_events()).unwrap().stroke_style.unwrap();
    assert_eq!(st.cap_end, Some(StrokeCapDto::ArrowTriangle));
    assert_eq!(st.join, Some(StrokeJoinDto::Round));
    // Соседнее поле чередуется: правка без изменения не даёт события.
    let mut flip = false;
    for bad in [
        0.5,
        101.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -4.0,
    ] {
        c.command(style(Edit::StrokeStyle {
            cap_start: None,
            cap_end: None,
            join: None,
            miter_limit: Some(bad),
        }));
        let ev = c.take_events();
        assert_eq!(
            errors(&ev),
            ["Неверная правка: Предел острого угла — от 1 до 100"],
            "{bad}"
        );
        // Отказ — без правки: допустимая правка соседнего поля покажет
        // сводку, в которой предел остался прежним.
        c.command(style(Edit::StrokeStyle {
            cap_start: None,
            cap_end: None,
            join: Some(if flip {
                StrokeJoinDto::Bevel
            } else {
                StrokeJoinDto::Miter
            }),
            miter_limit: None,
        }));
        flip = !flip;
        let st = summary(&c.take_events()).unwrap().stroke_style.unwrap();
        assert_eq!(st.miter_limit.value, 4.0, "{bad}");
    }
    // Границы диапазона допустимы.
    for ok in [1.0, 100.0] {
        c.command(style(Edit::StrokeStyle {
            cap_start: None,
            cap_end: None,
            join: None,
            miter_limit: Some(ok),
        }));
        let ev = c.take_events();
        assert!(errors(&ev).is_empty(), "{ok}");
        let st = summary(&ev).unwrap().stroke_style.unwrap();
        assert_eq!(st.miter_limit.value, ok);
    }
}

#[test]
fn empty_stroke_style_edit_is_refused_without_a_step() {
    let mut c = core();
    pen_path(&mut c);
    c.take_events();
    c.command(style(Edit::StrokeStyle {
        cap_start: None,
        cap_end: None,
        join: None,
        miter_limit: None,
    }));
    assert_eq!(
        errors(&c.take_events()),
        ["Неверная правка: В правке стиля обводки нет изменений"]
    );
}

#[test]
fn stroke_style_protocol_shape() {
    let json = serde_json::json!({
        "type": "editSelection",
        "edit": {"type": "strokeStyle", "capEnd": "arrowLines", "miterLimit": 8},
        "phase": "commit",
        "ids": null,
        "selectionVersion": null
    });
    let cmd: Command = serde_json::from_value(json).unwrap();
    assert!(matches!(
        cmd,
        Command::EditSelection {
            edit: Edit::StrokeStyle {
                cap_end: Some(StrokeCapDto::ArrowLines),
                cap_start: None,
                ..
            },
            ..
        }
    ));
    // Неизвестное окончание и лишнее поле — ошибка разбора.
    for bad in [
        serde_json::json!({"type": "strokeStyle", "capEnd": "arrow"}),
        serde_json::json!({"type": "strokeStyle", "dash": 2}),
    ] {
        assert!(
            serde_json::from_value::<Edit>(bad.clone()).is_err(),
            "{bad}"
        );
    }
    for tool in ["pen", "arrow"] {
        let t: Tool = serde_json::from_value(serde_json::json!(tool)).unwrap();
        assert_eq!(serde_json::to_value(t).unwrap(), serde_json::json!(tool));
    }
}
