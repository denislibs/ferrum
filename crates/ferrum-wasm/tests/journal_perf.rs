//! Цена журнала на странице в 100 000 узлов (этап 6): пачка после сдвига 1000 узлов —
//! миллисекунды, а не полсекунды полного снимка; открытие файла с журналом — в пределах
//! цели открытия (спека 3). Время меряется только в release.
#![cfg(all(not(target_arch = "wasm32"), not(debug_assertions)))]

use std::time::{Duration, Instant};

use ferrum_editor::Platform;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

fn core() -> Core {
    Core::new(
        Platform::Windows,
        Size::new(1280.0, 720.0),
        1.0,
        Backend::Cpu,
    )
}

fn run(core: &mut Core, command: &str) {
    core.command(Command::Execute {
        command: command.into(),
    });
}

#[test]
fn journal_of_100k_nodes_is_cheap() {
    let mut c = core();
    c.command(Command::Demo { nodes: 100_000 });
    c.command(Command::Save);
    let file = c
        .take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Saved { bytes } => Some(bytes.into_vec()),
            _ => None,
        })
        .expect("нет Saved");
    let mut c = core();
    c.command(Command::Open {
        bytes: ByteBuf::from(file.clone()),
    });
    c.take_events();
    // Сдвиг 1000 фреймов верхнего уровня — правка 1000 узлов, затем пачка.
    run(&mut c, "edit.select_all");
    run(&mut c, "nudge.right");
    c.take_events();
    let t = Instant::now();
    c.command(Command::FlushJournal);
    let flush = t.elapsed();
    let batch = c
        .take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Journal { bytes, .. } => bytes,
            _ => None,
        })
        .expect("пачка");
    let t = Instant::now();
    let mut back = core();
    back.command(Command::OpenWithJournal {
        bytes: ByteBuf::from(file),
        journal: vec![batch.clone()],
    });
    let open = t.elapsed();
    assert!(back
        .take_events()
        .contains(&CoreEvent::Recovered { batches: 1 }));
    eprintln!(
        "journal: flush {flush:?}, {} КБ; open with journal {open:?}",
        batch.len() / 1024
    );
    assert!(flush < Duration::from_millis(20), "пачка: {flush:?}");
    assert!(
        open < Duration::from_millis(3000),
        "открытие с журналом: {open:?}"
    );
}
