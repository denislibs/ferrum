//! Синхронизация в ядре worker'а (спека подсистемы 4, разделы 7 и 8): подписка, своя правка уходит
//! событием, чужая применяется, «изменён» снимает только подтверждение сервера, журнал снимается
//! только после него.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_doc::SyncVersion;
use ferrum_editor::{Input, Modifiers, Platform, PointerEvent};
use ferrum_render::CpuBackend;
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

fn drag(c: &mut Core, from: (f64, f64), to: (f64, f64), t: f64) {
    c.handle(Input::PointerDown(PointerEvent::new(
        from,
        Modifiers::NONE,
        t,
    )));
    c.handle(Input::PointerMove(PointerEvent::new(
        to,
        Modifiers::NONE,
        t + 100.0,
    )));
    c.handle(Input::PointerUp(PointerEvent::new(
        to,
        Modifiers::NONE,
        t + 200.0,
    )));
}

/// Несколько кадров: растр догоняет по бюджету. Пиксели цвета `tiny_png`.
fn image_pixels(c: &mut Core) -> usize {
    let (w, h) = c.device_size();
    let mut pixels = 0;
    for _ in 0..5 {
        let mut b = CpuBackend::new(w, h);
        c.draw(&mut b);
        pixels = b
            .render()
            .data_as_u8_slice()
            .chunks_exact(4)
            .filter(|p| p[..3] == [10, 20, 30])
            .count();
    }
    pixels
}

#[test]
fn image_downloaded_later_appears_inside_a_frame_when_zoomed_out() {
    let bytes = file();
    let mut a = opened(&bytes);
    let mut b = opened(&bytes);
    a.command(Command::SetTool { tool: Tool::Frame });
    drag(&mut a, (100.0, 100.0), (400.0, 400.0), 1000.0);
    let png = tiny_png();
    a.command(Command::PlaceImage {
        bytes: ByteBuf::from(png.clone()),
    });
    drag(&mut a, (150.0, 150.0), (350.0, 350.0), 2000.0);
    b.command(Command::SetZoom { zoom: 0.25 });
    b.take_events();
    for (doc, _, bytes) in updates(&mut a) {
        b.command(Command::ApplyRemote {
            doc,
            batches: vec![ByteBuf::from(bytes)],
        });
    }
    let hash = b
        .take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::ImagesNeeded { hashes } => hashes.first().cloned(),
            _ => None,
        })
        .expect("картинку попросили");
    assert_eq!(image_pixels(&mut b), 0, "пока заглушка");
    b.command(Command::AddRemoteImage {
        hash,
        bytes: ByteBuf::from(png),
    });
    assert!(image_pixels(&mut b) > 0, "растр фрейма перестроен");
}

#[test]
fn image_not_drawn_yet_does_not_invalidate_rasters() {
    let bytes = file();
    let mut b = opened(&bytes);
    b.command(Command::SetTool { tool: Tool::Frame });
    drag(&mut b, (100.0, 100.0), (400.0, 400.0), 1000.0);
    b.command(Command::SetTool { tool: Tool::Rect });
    drag(&mut b, (150.0, 150.0), (250.0, 250.0), 2000.0);
    b.command(Command::SetZoom { zoom: 0.25 });
    image_pixels(&mut b);
    let (w, h) = b.device_size();
    assert_eq!(
        b.draw(&mut CpuBackend::new(w, h)).raster_builds,
        0,
        "растр готов"
    );
    let png = tiny_png();
    let hash = ferrum_doc::ImageStore::default().add(png.clone());
    b.command(Command::AddRemoteImage {
        hash,
        bytes: ByteBuf::from(png),
    });
    assert_eq!(b.draw(&mut CpuBackend::new(w, h)).raster_builds, 0);
}

/// Файл на сервере и журнал браузера с картинкой и правкой, которую сервер не видел.
fn journal_with_image(bytes: &[u8]) -> Vec<u8> {
    let mut a = opened(bytes);
    a.command(Command::PlaceImage {
        bytes: ByteBuf::from(tiny_png()),
    });
    drag(&mut a, (100.0, 100.0), (200.0, 200.0), 1000.0);
    a.command(Command::FlushJournal);
    a.take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Journal { bytes: Some(b), .. } => Some(b.into_vec()),
            _ => None,
        })
        .expect("пачка журнала")
}

fn reopened(bytes: &[u8], journal: Vec<u8>) -> Core {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::OpenWithJournal {
        bytes: ByteBuf::from(bytes.to_vec()),
        journal: vec![ByteBuf::from(journal)],
        remote: Some(true),
    });
    c.command(Command::SyncStart);
    c
}

/// Ответы сервера, который знает только чистый файл: версии берутся у открывшего этот файл.
fn clean_versions(bytes: &[u8]) -> Vec<(String, ByteBuf)> {
    let mut x = opened(bytes);
    x.command(Command::SyncReset);
    x.command(Command::SyncFlush);
    x.take_events()
        .into_iter()
        .filter_map(|e| match e {
            CoreEvent::SyncSubscribe { doc, version } => Some((doc, version)),
            _ => None,
        })
        .collect()
}

#[test]
fn image_restored_from_journal_goes_to_server_before_the_edit() {
    let bytes = file();
    let journal = journal_with_image(&bytes);
    let mut c = reopened(&bytes, journal);
    c.command(Command::SyncFlush);
    c.take_events();
    for (doc, version) in clean_versions(&bytes) {
        c.command(Command::SyncSubscribed { doc, version });
    }
    c.command(Command::SyncFlush);
    let ev = c.take_events();
    let image = ev
        .iter()
        .position(|e| matches!(e, CoreEvent::LocalImage { .. }));
    let update = ev
        .iter()
        .position(|e| matches!(e, CoreEvent::LocalUpdates { .. }));
    assert!(image.is_some() && update.is_some(), "{ev:?}");
    assert!(image < update, "байты раньше ссылки: {ev:?}");
}

#[test]
fn journal_of_past_sessions_is_covered_by_mark_zero_after_ack() {
    let bytes = file();
    let journal = journal_with_image(&bytes);
    let mut c = reopened(&bytes, journal);
    c.command(Command::SyncFlush);
    c.take_events();
    for (doc, version) in clean_versions(&bytes) {
        c.command(Command::SyncSubscribed { doc, version });
    }
    let ups = updates(&mut c);
    assert!(!ups.is_empty(), "неотправленное из журнала уходит");
    assert!(c
        .take_events()
        .iter()
        .all(|e| !matches!(e, CoreEvent::JournalCovered { .. })));
    for (doc, seq, _) in &ups {
        c.command(Command::SyncAcked {
            doc: doc.clone(),
            client_seq: *seq,
        });
    }
    let ev = c.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::JournalCovered { batch } if *batch == 0.0)),
        "{ev:?}"
    );
}

#[test]
fn own_edit_before_subscription_is_journaled_and_unacked() {
    let bytes = file();
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::OpenWithJournal {
        bytes: ByteBuf::from(bytes),
        journal: vec![],
        remote: Some(true),
    });
    c.command(Command::SyncStart);
    rect(&mut c);
    assert!(c.editor().sync_unacked(), "сервера ещё не спрашивали");
    c.command(Command::FlushJournal);
    let ev = c.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::Journal { bytes: Some(_), .. })),
        "правка не теряется: {ev:?}"
    );
    assert!(
        ev.iter()
            .all(|e| !matches!(e, CoreEvent::SyncState { unacked: false })),
        "{ev:?}"
    );
}

#[test]
fn thumbnail_answers_with_png_and_keeps_nothing() {
    let bytes = file();
    let mut c = opened(&bytes);
    let thumb = |c: &mut Core| {
        c.command(Command::Thumbnail);
        c.take_events().into_iter().find_map(|e| match e {
            CoreEvent::Thumbnail { bytes, keep } => Some((bytes.is_some(), keep)),
            _ => None,
        })
    };
    assert_eq!(thumb(&mut c), Some((false, false)), "показать нечего");
    rect(&mut c);
    assert_eq!(thumb(&mut c), Some((true, false)));
}

#[test]
fn page_removed_by_other_reaches_the_person_as_notice() {
    let bytes = file();
    let mut a = opened(&bytes);
    let mut b = opened(&bytes);
    let added = {
        let before = a.editor().document().pages();
        a.command(Command::AddPage);
        let now = a.editor().document().pages();
        now.into_iter().find(|p| !before.contains(p)).unwrap().0
    };
    for (doc, _, bytes) in updates(&mut a) {
        b.command(Command::ApplyRemote {
            doc,
            batches: vec![ByteBuf::from(bytes)],
        });
    }
    b.command(Command::SetPage {
        id: added.to_string(),
    });
    a.command(Command::DeletePage {
        id: added.to_string(),
    });
    b.take_events();
    for (doc, _, bytes) in updates(&mut a) {
        b.command(Command::ApplyRemote {
            doc,
            batches: vec![ByteBuf::from(bytes)],
        });
    }
    let ev = b.take_events();
    assert!(
        ev.iter().any(|e| matches!(e, CoreEvent::Notice { message } if message == ferrum_editor::PAGE_REMOVED_BY_OTHER)),
        "{ev:?}"
    );
}

#[test]
fn ack_with_wrong_document_does_not_cancel_the_real_one() {
    let bytes = file();
    let mut a = opened(&bytes);
    rect(&mut a);
    let ups = updates(&mut a);
    for (_, seq, _) in &ups {
        a.command(Command::SyncAcked {
            doc: "root-other".into(),
            client_seq: *seq,
        });
    }
    assert!(a.editor().sync_unacked());
    for (doc, seq, _) in &ups {
        a.command(Command::SyncAcked {
            doc: doc.clone(),
            client_seq: *seq,
        });
    }
    assert!(!a.editor().sync_unacked(), "настоящее подтверждение дошло");
}

/// Следование: кадр участника обогнал правку с его новой страницей — переход на неё, как только
/// она дойдёт, а не со следующим кадром участника.
#[test]
fn following_moves_to_peer_page_as_soon_as_it_arrives() {
    let bytes = file();
    let mut a = opened(&bytes);
    let mut b = opened(&bytes);
    a.command(Command::AddPage);
    let page = a.editor().page().to_string();
    let first = b.editor().page();
    assert_ne!(first.to_string(), page);
    b.command(Command::Presence {
        items: vec![
            PresenceItemDto::Join {
                participant: 1,
                user: "u-1".into(),
                name: "Анна".into(),
                color: 0x0d74ce,
            },
            PresenceItemDto::State {
                participant: 1,
                page: page.clone(),
                view: [0.0, 0.0, 800.0, 600.0],
                cursor: Some([1.0, 1.0]),
                selection: None,
                gesture: None,
            },
        ],
    });
    b.command(Command::Follow {
        participant: Some(1),
    });
    assert_eq!(b.editor().page(), first, "страницы ещё нет");
    assert_eq!(b.editor().following(), Some(1));
    for (doc, _, bytes) in updates(&mut a) {
        b.command(Command::ApplyRemote {
            doc,
            batches: vec![ByteBuf::from(bytes)],
        });
    }
    assert_eq!(
        b.editor().page().to_string(),
        page,
        "страница дошла — переход"
    );
    assert_eq!(b.editor().following(), Some(1));
}
