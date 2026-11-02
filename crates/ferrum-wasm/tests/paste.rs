//! Способы вставки в ядре worker'а (спека подсистемы 2, раздел 6): команды
//! «Вставить поверх» и «Вставить с заменой» просят буфер событием
//! `PasteRequested { mode }`, `Paste { mode }` вставляет этим способом.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;

const VIEW: Size = Size::new(800.0, 600.0);

fn click(core: &mut Core, x: f64, y: f64, t: f64) {
    let e = PointerEvent {
        point: (x, y).into(),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t,
    };
    core.handle(Input::PointerDown(e));
    core.handle(Input::PointerUp(e));
}

fn execute(core: &mut Core, key: &str) -> Vec<CoreEvent> {
    core.command(Command::Execute {
        command: key.into(),
    });
    core.take_events()
}

#[test]
fn paste_commands_ask_with_mode_and_replace_swaps_the_selection() {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    core.command(Command::Demo { nodes: 100 });
    // «Слой 1» в «Экран 0»: мир (200, 40), экран (140, 64).
    click(&mut core, 140.0, 64.0, 1000.0);
    let ev = execute(&mut core, "edit.copy");
    let bytes = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::Clipboard { bytes, .. } => Some(bytes.clone()),
            _ => None,
        })
        .expect("копия");
    for (key, mode) in [
        ("edit.paste", PasteModeDto::Normal),
        ("edit.paste_over", PasteModeDto::Over),
        ("edit.paste_replace", PasteModeDto::Replace),
    ] {
        let ev = execute(&mut core, key);
        assert!(
            ev.contains(&CoreEvent::PasteRequested { mode }),
            "{key}: {ev:?}"
        );
    }
    // Замена «Слоя 2» (мир (340, 40)) копией «Слоя 1»: узлов столько же.
    click(&mut core, 280.0, 64.0, 2000.0);
    let x = |core: &Core| {
        let ed = core.editor();
        let id = ed.selection()[0];
        ed.scene().node(id).unwrap().world_bounds.unwrap().x0
    };
    let (nodes, before) = (core.live().nodes, x(&core));
    let replaced = core.editor().selection()[0];
    core.command(Command::Paste {
        bytes,
        mode: Some(PasteModeDto::Replace),
    });
    let ed = core.editor();
    assert_eq!(ed.selection().len(), 1);
    assert_ne!(ed.selection()[0], replaced);
    assert!(ed.scene().node(replaced).is_none(), "заменённого нет");
    assert_eq!(core.live().nodes, nodes);
    assert_eq!(x(&core), before, "на место заменённого");
}
