//! Булевы группы в ядре worker'а (подсистема 3, этап 4): команды раскладки
//! через протокол, строка слоя с операцией, сводка выделения с операцией и
//! смена операции выделенной группы.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_wasm::core::Core;
use ferrum_wasm::layers::NodeType;
use ferrum_wasm::protocol::*;
use ferrum_wasm::summary::BooleanOpDto;
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

fn drag(c: &mut Core, from: (f64, f64), to: (f64, f64), t: f64) {
    let ev = |p: (f64, f64), dt: f64| PointerEvent {
        point: p.into(),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t + dt,
    };
    c.handle(Input::PointerDown(ev(from, 0.0)));
    c.handle(Input::PointerMove(ev(to, 10.0)));
    c.handle(Input::PointerUp(ev(to, 20.0)));
}

fn summary(ev: &[CoreEvent]) -> Option<Summary> {
    ev.iter().rev().find_map(|e| match e {
        CoreEvent::Properties { summary } => Some(summary.clone()),
        _ => None,
    })
}

fn run(c: &mut Core, command: &str) -> Vec<CoreEvent> {
    c.command(Command::Execute {
        command: command.into(),
    });
    c.take_events()
}

/// Два прямоугольника инструментом R, оба выделены. Зум демо — 0,5: в мире
/// это 200 × 200 с перекрытием 100.
fn two_rects(c: &mut Core) {
    for (i, x) in [100.0, 150.0].into_iter().enumerate() {
        c.command(Command::SetTool { tool: Tool::Rect });
        drag(c, (x, 100.0), (x + 100.0, 200.0), 1000.0 * (i + 1) as f64);
    }
    let ids: Vec<String> = c
        .editor()
        .scene()
        .iter()
        .map(|(_, n)| n.id.to_string())
        .collect();
    c.command(Command::Select {
        ids,
        mode: SelectMode::Replace,
    });
    c.take_events();
}

#[test]
fn boolean_commands_reach_the_editor_and_the_summary_shows_the_op() {
    let mut c = core();
    two_rects(&mut c);
    let ev = run(&mut c, "object.boolean_subtract");
    let s = summary(&ev).expect("сводка");
    assert_eq!(s.kind, Some(NodeType::Boolean));
    assert_eq!(s.boolean_op, Some(BooleanOpDto::Subtract));
    assert_eq!(s.name.as_deref(), Some("Вычитание"));
    assert_eq!(s.w.value, 100.0, "рамка — по результату");
    // Та же команда на выделенной группе меняет операцию.
    let ev = run(&mut c, "object.boolean_union");
    let s = summary(&ev).expect("сводка");
    assert_eq!(s.boolean_op, Some(BooleanOpDto::Union));
    assert_eq!(s.w.value, 300.0);
    // Строка слоя несёт операцию для иконки.
    c.command(Command::LayersWindow {
        start: 0,
        count: 10,
    });
    let ev = c.take_events();
    let rows = ev
        .iter()
        .rev()
        .find_map(|e| match e {
            CoreEvent::Layers { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .expect("строки слоёв");
    let row = rows.iter().find(|r| r.kind == NodeType::Boolean).unwrap();
    assert_eq!(row.boolean_op, Some(BooleanOpDto::Union));
    assert!(rows
        .iter()
        .filter(|r| r.kind == NodeType::Rect)
        .all(|r| r.boolean_op.is_none()));
}

#[test]
fn boolean_and_outline_commands_are_in_the_keymap() {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    let ev = c.take_events();
    let commands = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::Keymap { commands, .. } => Some(commands.clone()),
            _ => None,
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 53);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
        ("object.boolean_subtract", "Alt+Shift+KeyS"),
        ("object.boolean_intersect", "Alt+Shift+KeyI"),
        ("object.boolean_exclude", "Alt+Shift+KeyE"),
        ("object.outline_stroke", "Ctrl+Alt+KeyO"),
    ] {
        let e = commands.iter().find(|e| e.key == key).expect(key);
        assert_eq!(e.bindings, vec![binding.to_string()], "{key}");
    }
}
