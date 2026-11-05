//! Синхронизация в ядре worker'а (спека подсистемы 4, разделы 7 и 8): подписка, своя правка уходит
//! событием, чужая применяется, «изменён» снимает только подтверждение сервера, журнал снимается
//! только после него.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_doc::SyncVersion;
use ferrum_editor::{Input, Modifiers, Platform, PointerEvent};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

fn file() -> Vec<u8> {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::Snapshot {
        thumbnail: false,
        full: Some(true),
    });
    c.take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Snapshot { bytes, .. } => Some(bytes.into_vec()),
            _ => None,
        })
        .unwrap()
}

/// Ядро открыло файл на сервере; сервер знает то же, что и файл.
fn opened(bytes: &[u8]) -> Core {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::OpenWithJournal {
        bytes: ByteBuf::from(bytes.to_vec()),
        journal: vec![],
        remote: Some(true),
    });
    c.command(Command::SyncStart);
    subscribe(&mut c);
    c.take_events();
    c
}

/// Подписки, на которые сервер отвечает версией клиента.
fn subscribe(c: &mut Core) {
    c.command(Command::SyncFlush);
    for e in c.take_events() {
        if let CoreEvent::SyncSubscribe { doc, version } = e {
            c.command(Command::SyncSubscribed { doc, version });
        }
    }
}

/// Рисует прямоугольник инструментом: своя правка, как у человека.
fn rect(c: &mut Core) {
    let at = |x: f64, y: f64, t: f64| PointerEvent::new((x, y), Modifiers::NONE, t);
    c.command(Command::SetTool { tool: Tool::Rect });
    c.handle(Input::PointerDown(at(100.0, 100.0, 1000.0)));
    c.handle(Input::PointerMove(at(200.0, 200.0, 1100.0)));
    c.handle(Input::PointerUp(at(200.0, 200.0, 1200.0)));
}

fn updates(c: &mut Core) -> Vec<(String, u32, Vec<u8>)> {
    c.command(Command::SyncFlush);
    c.take_events()
        .into_iter()
        .filter_map(|e| match e {
            CoreEvent::LocalUpdates {
                doc,
                client_seq,
                bytes,
            } => Some((doc, client_seq, bytes.into_vec())),
            _ => None,
        })
        .collect()
}

/// PNG 1×1: хэш — как у `ImageStore`.
fn tiny_png() -> Vec<u8> {
    let mut pm = ferrum_render::Pixmap::new(1, 1);
    pm.data_as_u8_slice_mut()
        .copy_from_slice(&[10, 20, 30, 255]);
    pm.into_png().unwrap()
}

#[test]
fn own_edit_goes_out_foreign_comes_in_and_modified_follows_ack() {
    let bytes = file();
    let mut a = opened(&bytes);
    let mut b = opened(&bytes);
    rect(&mut a);
    let ev = a.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::Modified { modified: true })),
        "{ev:?}"
    );

    let ups = updates(&mut a);
    assert!(!ups.is_empty());
    for (doc, _, bytes) in &ups {
        b.command(Command::ApplyRemote {
            doc: doc.clone(),
            batches: vec![ByteBuf::from(bytes.clone())],
        });
    }
    let ev = b.take_events();
    assert!(
        ev.iter()
            .all(|e| !matches!(e, CoreEvent::Modified { modified: true })),
        "чужая правка не «изменён»: {ev:?}"
    );
    assert!(
        ev.iter().any(|e| matches!(e, CoreEvent::Document { .. })),
        "панели знают: {ev:?}"
    );
    assert_eq!(b.editor().scene().len(), 1);
    assert!(!b.editor().document().can_undo());

    for (doc, seq, _) in &ups {
        a.command(Command::SyncAcked {
            doc: doc.clone(),
            client_seq: *seq,
        });
    }
    let ev = a.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::Modified { modified: false })),
        "{ev:?}"
    );
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::SyncState { unacked: false })),
        "{ev:?}"
    );
}

#[test]
fn journal_is_covered_only_after_ack_and_empty_flush_writes_nothing() {
    let bytes = file();
    let mut a = opened(&bytes);
    a.command(Command::FlushJournal);
    let ev = a.take_events();
    assert!(
        ev.iter().any(|e| matches!(
            e,
            CoreEvent::Journal {
                bytes: None,
                failed: false,
                ..
            }
        )),
        "своего нет — пачки нет: {ev:?}"
    );
    rect(&mut a);
    a.command(Command::FlushJournal);
    let ev = a.take_events();
    let batch = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::Journal {
                batch,
                bytes: Some(_),
                ..
            } => Some(*batch),
            _ => None,
        })
        .unwrap();
    let ups = updates(&mut a);
    assert!(a
        .take_events()
        .iter()
        .all(|e| !matches!(e, CoreEvent::JournalCovered { batch: b } if *b >= batch)));
    for (doc, seq, _) in &ups {
        a.command(Command::SyncAcked {
            doc: doc.clone(),
            client_seq: *seq,
        });
    }
    let ev = a.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::JournalCovered { batch: b } if *b == batch)),
        "{ev:?}"
    );
}

#[test]
fn reconnect_resends_what_was_not_acked() {
    let bytes = file();
    let mut a = opened(&bytes);
    let server: Vec<(String, Vec<u8>)> = {
        let mut x = opened(&bytes);
        x.command(Command::SyncReset);
        x.command(Command::SyncFlush);
        x.take_events()
            .into_iter()
            .filter_map(|e| match e {
                CoreEvent::SyncSubscribe { doc, version } => Some((doc, version.into_vec())),
                _ => None,
            })
            .collect()
    };
    rect(&mut a);
    let first = updates(&mut a);
    assert!(!first.is_empty());
    assert!(updates(&mut a).is_empty(), "дважды не шлём");
    a.command(Command::SyncReset);
    a.command(Command::SyncFlush);
    a.take_events();
    for (doc, version) in &server {
        assert!(SyncVersion::decode(version).is_some());
        a.command(Command::SyncSubscribed {
            doc: doc.clone(),
            version: ByteBuf::from(version.clone()),
        });
    }
    let again = updates(&mut a);
    assert_eq!(first.len(), again.len());
    assert_eq!(first[0].2, again[0].2, "те же байты: повтор безвреден");
    assert_ne!(
        first[0].1, again[0].1,
        "новый номер: ответ на старый игнорируется"
    );
}

#[test]
fn view_only_receives_and_sends_nothing() {
    let bytes = file();
    let mut a = opened(&bytes);
    let mut b = opened(&bytes);
    b.command(Command::SetViewOnly { on: true });
    b.take_events();
    rect(&mut a);
    for (doc, _, bytes) in updates(&mut a) {
        b.command(Command::ApplyRemote {
            doc,
            batches: vec![ByteBuf::from(bytes)],
        });
    }
    assert_eq!(b.editor().scene().len(), 1);
    assert!(updates(&mut b).is_empty());
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    let bytes = file();
    let mut a = opened(&bytes);
    a.command(Command::ApplyRemote {
        doc: "../x".into(),
        batches: vec![],
    });
    a.command(Command::SyncSubscribed {
        doc: "root".into(),
        version: ByteBuf::from(vec![1, 2, 3]),
    });
    let page = a.editor().page().to_string();
    a.command(Command::ApplyRemote {
        doc: page,
        batches: vec![ByteBuf::from(b"garbage".to_vec())],
    });
    a.command(Command::SyncAcked {
        doc: "root".into(),
        client_seq: 777,
    });
    let errors = a
        .take_events()
        .into_iter()
        .filter(|e| matches!(e, CoreEvent::Error { .. }))
        .count();
    assert_eq!(errors, 3);
}

#[test]
fn foreign_image_reference_is_requested_and_bytes_are_checked() {
    let bytes = file();
    let mut a = opened(&bytes);
    let mut b = opened(&bytes);
    rect(&mut a);
    for (doc, _, bytes) in updates(&mut a) {
        b.command(Command::ApplyRemote {
            doc,
            batches: vec![ByteBuf::from(bytes)],
        });
    }
    a.take_events();
    let png = tiny_png();
    a.command(Command::PlaceImage {
        bytes: ByteBuf::from(png.clone()),
    });
    a.take_events();
    let at = PointerEvent::new((300.0, 300.0), Modifiers::NONE, 5000.0);
    a.handle(Input::PointerDown(at));
    a.handle(Input::PointerUp(PointerEvent::new(
        (300.0, 300.0),
        Modifiers::NONE,
        5100.0,
    )));
    a.command(Command::SyncFlush);
    let ev = a.take_events();
    let image = ev
        .iter()
        .position(|e| matches!(e, CoreEvent::LocalImage { .. }));
    let update = ev
        .iter()
        .position(|e| matches!(e, CoreEvent::LocalUpdates { .. }));
    assert!(
        image.unwrap() < update.unwrap(),
        "картинка раньше правки: {ev:?}"
    );
    for e in ev {
        if let CoreEvent::LocalUpdates { doc, bytes, .. } = e {
            b.command(Command::ApplyRemote {
                doc,
                batches: vec![bytes],
            });
        }
    }
    let ev = b.take_events();
    let hash = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::ImagesNeeded { hashes } => hashes.first().cloned(),
            _ => None,
        })
        .expect("картинку попросили");
    b.command(Command::AddRemoteImage {
        hash: hash.clone(),
        bytes: ByteBuf::from(b"chto-to drugoe".to_vec()),
    });
    assert!(
        b.editor().document().images.get(&hash).is_none(),
        "чужой хэш не проходит"
    );
    b.command(Command::AddRemoteImage {
        hash: hash.clone(),
        bytes: ByteBuf::from(png),
    });
    assert!(b.editor().document().images.get(&hash).is_some());
}
