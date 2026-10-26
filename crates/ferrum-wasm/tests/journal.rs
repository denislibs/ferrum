//! Журнал изменений в ядре worker'а (этап 6): пачка правок по команде и перед
//! снимком, номер пачки в снимке, открытие файла с журналом — восстановленный
//! документ изменён, повреждённый журнал — предупреждение, а не отказ открыть.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::Platform;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

const VIEW: Size = Size::new(800.0, 600.0);

fn core() -> Core {
    Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu)
}

fn run(core: &mut Core, command: &str) {
    core.command(Command::Execute {
        command: command.into(),
    });
}

/// Файл с примером из `nodes` узлов и ядро, открывшее его (как редактор файла из хранилища).
fn stored(nodes: u32) -> (Vec<u8>, Core) {
    let mut c = core();
    c.command(Command::Demo { nodes });
    c.command(Command::Save);
    let bytes = c
        .take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Saved { bytes } => Some(bytes.into_vec()),
            _ => None,
        })
        .expect("нет Saved");
    let mut c = core();
    c.command(Command::Open {
        bytes: ByteBuf::from(bytes.clone()),
    });
    c.take_events();
    (bytes, c)
}

fn journal(ev: &[CoreEvent]) -> Vec<(f64, Option<Vec<u8>>)> {
    ev.iter()
        .filter_map(|e| match e {
            CoreEvent::Journal { batch, bytes } => {
                Some((*batch, bytes.as_ref().map(|b| b.to_vec())))
            }
            _ => None,
        })
        .collect()
}

fn flush(core: &mut Core) -> (f64, Option<Vec<u8>>) {
    core.command(Command::FlushJournal);
    let mut j = journal(&core.take_events());
    assert_eq!(j.len(), 1, "один ответ на команду");
    j.remove(0)
}

fn reopen(file: &[u8], batches: &[Vec<u8>]) -> (Core, Vec<CoreEvent>) {
    let mut c = core();
    c.command(Command::OpenWithJournal {
        bytes: ByteBuf::from(file.to_vec()),
        journal: batches.iter().cloned().map(ByteBuf::from).collect(),
    });
    let ev = c.take_events();
    (c, ev)
}

#[test]
fn flush_answers_with_a_batch_only_when_there_are_edits() {
    let (_, mut c) = stored(10);
    assert_eq!(flush(&mut c), (0.0, None), "открытие — не правка");
    run(&mut c, "edit.select_all");
    assert_eq!(flush(&mut c), (0.0, None), "выделение — не правка");
    run(&mut c, "edit.delete");
    let (batch, bytes) = flush(&mut c);
    assert_eq!(batch, 1.0);
    assert!(bytes.is_some());
    assert_eq!(flush(&mut c), (1.0, None), "всё уже отдано");
}

#[test]
fn demo_is_not_journaled() {
    let mut c = core();
    c.command(Command::Demo { nodes: 100 });
    c.take_events();
    assert_eq!(flush(&mut c), (0.0, None));
}

#[test]
fn journal_restores_edits_on_top_of_the_file_and_marks_modified() {
    let (file, mut c) = stored(10);
    run(&mut c, "edit.select_all");
    run(&mut c, "edit.delete");
    let (_, b1) = flush(&mut c);
    c.command(Command::AddPage);
    let (_, b2) = flush(&mut c);
    let (back, ev) = reopen(&file, &[b1.unwrap(), b2.unwrap()]);
    let at = |pred: &dyn Fn(&CoreEvent) -> bool| ev.iter().position(pred).expect("нет события");
    let recovered = at(&|e| *e == CoreEvent::Recovered { batches: 2 });
    let opened = at(&|e| matches!(e, CoreEvent::Opened { .. }));
    assert!(recovered < opened, "Recovered — до Opened: {ev:?}");
    let CoreEvent::Opened {
        pages,
        nodes,
        warnings,
        ..
    } = &ev[opened]
    else {
        unreachable!()
    };
    assert_eq!((*pages, *nodes), (2, 0), "{warnings:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(ev.contains(&CoreEvent::Modified { modified: true }));
    assert_eq!(
        back.editor().document().pages().len(),
        c.editor().document().pages().len()
    );
}

#[test]
fn snapshot_flushes_the_journal_first_and_carries_its_number() {
    let (_, mut c) = stored(10);
    run(&mut c, "edit.select_all");
    run(&mut c, "edit.delete");
    c.command(Command::Snapshot { thumbnail: false });
    let ev = c.take_events();
    let j = ev
        .iter()
        .position(|e| matches!(e, CoreEvent::Journal { batch, bytes: Some(_) } if *batch == 1.0))
        .expect("пачка перед снимком");
    let (s, file2) = ev
        .iter()
        .enumerate()
        .find_map(|(i, e)| match e {
            CoreEvent::Snapshot { journal, bytes, .. } => {
                assert_eq!(*journal, 1.0);
                Some((i, bytes.to_vec()))
            }
            _ => None,
        })
        .expect("снимок");
    assert!(j < s);
    // Без правок снимок пачку не шлёт, номер — прежний.
    c.command(Command::Snapshot { thumbnail: false });
    let ev = c.take_events();
    assert!(journal(&ev).is_empty());
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Snapshot { journal, .. } if *journal == 1.0)));
    // Правка после снимка — следующая пачка; файл снимка и она дают документ.
    c.command(Command::AddPage);
    let (batch, b2) = flush(&mut c);
    assert_eq!(batch, 2.0);
    let (back, _) = reopen(&file2, &[b2.unwrap()]);
    assert_eq!(back.editor().document().pages().len(), 2);
}

#[test]
fn damaged_journal_opens_the_file_with_a_warning() {
    let (file, _) = stored(10);
    let (back, ev) = reopen(&file, &[b"garbage".to_vec()]);
    let warnings = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::Opened { warnings, .. } => Some(warnings.clone()),
            _ => None,
        })
        .expect("файл открыт");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("журнал изменений повреждён"));
    assert!(!ev.iter().any(|e| matches!(e, CoreEvent::Recovered { .. })));
    assert!(
        !ev.contains(&CoreEvent::Modified { modified: true }),
        "ничего не восстановлено"
    );
    assert_eq!(back.editor().scene().len(), 10);
}

#[test]
fn too_many_batches_is_an_error_and_keeps_the_document() {
    let (file, _) = stored(10);
    let mut c = core();
    c.command(Command::Demo { nodes: 5 });
    c.take_events();
    c.command(Command::OpenWithJournal {
        bytes: ByteBuf::from(file),
        journal: vec![ByteBuf::new(); MAX_JOURNAL_BATCHES + 1],
    });
    let ev = c.take_events();
    assert!(ev.iter().any(|e| matches!(e, CoreEvent::Error { message }
        if message.starts_with("Не удалось открыть файл: в журнале"))));
    assert_eq!(c.editor().scene().len(), 5);
}
