//! Чужие правки (спека подсистемы 4, раздел 8): зеркало обновляется по изменённым узлам и совпадает с
//! полной сборкой из Loro при любом порядке и повторах пачек; курсор
//! синхронизации шлёт только своё и считает версии независимо от журнала.

use std::collections::HashMap;

use ferrum_doc::container::{decode, encode_full};
use ferrum_doc::*;
use proptest::prelude::*;

/// Две вкладки одного файла: общий снимок, у каждой свой пир.
fn pair() -> (Document, Document, PageId) {
    let doc = Document::new();
    let bytes = encode_full(&doc).unwrap();
    let mut a = decode(&bytes).unwrap().document;
    let mut b = decode(&bytes).unwrap().document;
    let page = a.pages()[0].0;
    for d in [&mut a, &mut b] {
        d.sync_start();
        d.page(page).unwrap();
        let subs = d.take_sync_subscribes();
        for (k, v) in subs {
            d.sync_subscribed(k, v);
        }
    }
    (a, b, page)
}

fn deliver(from: &mut Document, to: &mut [&mut Document]) -> Vec<RemoteOutcome> {
    let mut out = Vec::new();
    for batch in from.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
        for d in to.iter_mut() {
            out.push(d.apply_remote(batch.doc, &[&batch.bytes]).unwrap());
        }
        from.sync_acked(batch.doc, &batch.end);
    }
    out
}

fn sync(a: &mut Document, b: &mut Document) {
    deliver(a, &mut [&mut *b]);
    deliver(b, &mut [&mut *a]);
}

fn rect(doc: &mut Document, page: PageId) -> NodeId {
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap()[0]
}

fn set_x(doc: &mut Document, page: PageId, id: NodeId, x: f64) {
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                x: Some(x),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    })
    .unwrap();
}

/// Узлы страницы: родитель, позиция и свойства — всё, что читает сцена.
fn nodes(doc: &mut Document, page: PageId) -> HashMap<NodeId, (Option<NodeId>, Props)> {
    let p = doc.page(page).unwrap();
    p.walk()
        .into_iter()
        .map(|n| (n, (p.parent(n), p.props(n).unwrap())))
        .collect()
}

/// Зеркало совпадает с тем, что строится из Loro с нуля.
fn assert_mirror(doc: &mut Document, page: PageId) {
    let snapshot = doc.page(page).unwrap().export_full_snapshot().unwrap();
    let fresh = PageDoc::from_snapshot(&snapshot).unwrap();
    let want: HashMap<_, _> = fresh
        .walk()
        .into_iter()
        .map(|n| (n, (fresh.parent(n), fresh.props(n).unwrap())))
        .collect();
    assert_eq!(
        nodes(doc, page),
        want,
        "зеркало расходится с полной сборкой"
    );
}

#[test]
fn foreign_create_edit_move_delete_reach_mirror_and_changes() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    let group = rect(&mut a, page);
    sync(&mut a, &mut b);
    let c = b.take_changes(page);
    assert!(
        c.created.contains(&id) && c.created.contains(&group),
        "{c:?}"
    );
    assert_eq!(
        b.page(page).unwrap().props(id).unwrap().kind,
        NodeKind::Rect
    );

    set_x(&mut a, page, id, 42.0);
    sync(&mut a, &mut b);
    let c = b.take_changes(page);
    assert_eq!(c.props, vec![id], "изменён только этот узел");
    assert_eq!(b.page(page).unwrap().props(id).unwrap().x, 42.0);

    a.transact(page, vec![], |tx| {
        tx.move_node(id, None, 1)?;
        Ok(vec![id])
    })
    .unwrap();
    sync(&mut a, &mut b);
    assert!(b.take_changes(page).moved.contains(&id));
    assert_eq!(b.page(page).unwrap().children(None)[1], id);

    a.transact(page, vec![], |tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
    sync(&mut a, &mut b);
    assert!(b.take_changes(page).deleted.contains(&id));
    assert!(!b.page(page).unwrap().contains(id));
    assert_mirror(&mut b, page);
}

#[test]
fn deleting_a_frame_removes_descendants_from_mirror() {
    let (mut a, mut b, page) = pair();
    let (frame, kid) = {
        let ids = a
            .transact(page, vec![], |tx| {
                let f = tx.create_node(None, None, NodeKind::Frame)?;
                Ok(vec![f, tx.create_node(Some(f), None, NodeKind::Rect)?])
            })
            .unwrap();
        (ids[0], ids[1])
    };
    sync(&mut a, &mut b);
    b.take_changes(page);
    a.transact(page, vec![], |tx| {
        tx.delete_nodes(&[frame])?;
        Ok(vec![])
    })
    .unwrap();
    sync(&mut a, &mut b);
    let c = b.take_changes(page);
    assert!(
        c.deleted.contains(&frame) && c.deleted.contains(&kid),
        "{c:?}"
    );
    assert_mirror(&mut b, page);
}

#[test]
fn text_shift_moves_caret_with_foreign_edits() {
    let (mut a, mut b, page) = pair();
    let id = a
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Text)?])
        })
        .unwrap()[0];
    a.transact(page, vec![id], |tx| {
        tx.insert_text(id, 0, "абвгд", None)?;
        Ok(vec![id])
    })
    .unwrap();
    sync(&mut a, &mut b);
    b.transact(page, vec![id], |tx| {
        tx.insert_text(id, 1, "XY", None)?;
        tx.delete_text(id, 5..6)?;
        Ok(vec![id])
    })
    .unwrap();
    let got = deliver(&mut b, &mut [&mut a]);
    let shifts: Vec<&TextShift> = got
        .iter()
        .flat_map(|o| o.text.iter().map(|(_, s)| s))
        .collect();
    assert_eq!(shifts.len(), 1);
    let s = shifts[0];
    assert_eq!(s.map(0), 0);
    assert_eq!(
        s.map(1),
        1,
        "вставка ровно в позиции курсора его не двигает"
    );
    assert_eq!(s.map(2), 4);
    assert_eq!(s.map(5), 6);
}

#[test]
fn foreign_page_add_and_remove_update_document_and_forget_steps() {
    let (mut a, mut b, _) = pair();
    let added = a.add_page("Вторая").unwrap();
    let got = deliver(&mut a, &mut [&mut b]);
    assert!(got.iter().any(|o| o.added_pages == vec![added]), "{got:?}");
    assert_eq!(b.pages().len(), 2);
    let subs = b.take_sync_subscribes();
    assert!(
        subs.iter().any(|(k, _)| *k == DocKey::Page(added)),
        "подписка на новую страницу"
    );

    let own = rect(&mut b, added);
    assert!(b.can_undo());
    let _ = own;
    a.delete_page(added).unwrap();
    let got = deliver(&mut a, &mut [&mut b]);
    assert!(got.iter().any(|o| o.removed_pages == vec![added]));
    assert_eq!(b.pages().len(), 1);
    assert!(!b.can_undo(), "шаги удалённой страницы забыты");
}

#[test]
fn viewer_applies_foreign_edits_and_sends_nothing() {
    let (mut a, mut b, page) = pair();
    b.set_view_only(true);
    let id = rect(&mut a, page);
    sync(&mut a, &mut b);
    assert!(b.page(page).unwrap().contains(id));
    assert!(b
        .take_sync_updates(MAX_SYNC_BATCH_BYTES)
        .unwrap()
        .is_empty());
    assert!(!b.sync_unacked());
}

#[test]
fn newer_schema_from_other_becomes_read_only_but_keeps_applying() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    a.force_schema_version_for_tests(SCHEMA_VERSION + 1);
    let got = deliver(&mut a, &mut [&mut b]);
    assert!(got.iter().any(|o| o.became_read_only), "{got:?}");
    assert!(b.is_read_only());
    assert!(
        b.page(page).unwrap().contains(id),
        "чужие правки применяются"
    );
}

#[test]
fn unknown_kind_from_other_makes_read_only_with_rebuild() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    sync(&mut a, &mut b);
    b.take_changes(page);
    a.page(page).unwrap().force_kind_for_tests(id, "star");
    let got = deliver(&mut a, &mut [&mut b]);
    let o = got.iter().find(|o| o.page == Some(page)).unwrap();
    assert!(o.rebuilt && o.became_read_only, "{o:?}");
    assert!(
        !b.page(page).unwrap().contains(id),
        "скрыт от модели, в Loro остался"
    );
    let later = PageDoc::from_snapshot(&b.page(page).unwrap().export_full_snapshot().unwrap());
    assert!(later.unwrap().has_unknown_nodes());
}

#[test]
fn broken_bytes_are_reported_without_harm() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    sync(&mut a, &mut b);
    let o = b
        .apply_remote(DocKey::Page(page), &[b"not a loro blob"])
        .unwrap();
    assert!(o.failed);
    assert!(b.page(page).unwrap().contains(id));
    assert_mirror(&mut b, page);
}

#[test]
fn update_without_its_dependencies_waits_and_applies_later() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    let first = a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap();
    set_x(&mut a, page, id, 7.0);
    let second = a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap();
    let doc = DocKey::Page(page);
    let f = first.iter().find(|x| x.doc == doc).unwrap();
    let s = second.iter().find(|x| x.doc == doc).unwrap();
    let o = b.apply_remote(doc, &[&s.bytes]).unwrap();
    assert!(o.pending);
    assert!(!b.page(page).unwrap().contains(id));
    b.apply_remote(doc, &[&f.bytes]).unwrap();
    assert_eq!(b.page(page).unwrap().props(id).unwrap().x, 7.0);
    assert_mirror(&mut b, page);
}

#[test]
fn cursor_sends_only_unsent_and_resends_after_reconnect() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    let batches = a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap();
    assert!(!batches.is_empty());
    assert!(
        a.take_sync_updates(MAX_SYNC_BATCH_BYTES)
            .unwrap()
            .is_empty(),
        "второй раз нечего"
    );
    assert!(a.sync_unacked());
    // Обрыв до ack: подписка заново, сервер сообщает прежнюю версию, правка уходит снова.
    a.sync_reset();
    let old = SyncVersion::default();
    for (k, _) in a.take_sync_subscribes() {
        a.sync_subscribed(
            k,
            if k == DocKey::Root {
                SyncVersion::default()
            } else {
                old.clone()
            },
        );
    }
    let again = a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap();
    assert!(again.iter().any(|x| x.doc == DocKey::Page(page)));
    for x in &again {
        // Повтор безвреден: Loro пропускает известное.
        b.apply_remote(x.doc, &[&x.bytes]).unwrap();
        b.apply_remote(x.doc, &[&x.bytes]).unwrap();
        a.sync_acked(x.doc, &x.end);
    }
    assert!(b.page(page).unwrap().contains(id));
    assert!(!a.sync_unacked());
    assert_mirror(&mut b, page);
}

#[test]
fn foreign_updates_are_not_sent_back() {
    let (mut a, mut b, page) = pair();
    rect(&mut a, page);
    sync(&mut a, &mut b);
    assert!(
        b.take_sync_updates(MAX_SYNC_BATCH_BYTES)
            .unwrap()
            .is_empty(),
        "чужое не шлём"
    );
    assert!(!b.sync_unacked());
}

#[test]
fn journal_marks_are_covered_only_after_ack() {
    let (mut a, _b, page) = pair();
    rect(&mut a, page);
    let batches = a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap();
    let marks = a.sync_versions();
    assert!(!a.sync_covers(&marks));
    for x in &batches {
        a.sync_acked(x.doc, &x.end);
    }
    assert!(a.sync_covers(&marks));
}

#[test]
fn big_edit_is_split_under_the_limit_and_converges() {
    let (mut a, mut b, page) = pair();
    a.transact(page, vec![], |tx| {
        for _ in 0..2000 {
            tx.create_node(None, None, NodeKind::Rect)?;
        }
        Ok(vec![])
    })
    .unwrap();
    let limit = 16 * 1024;
    let batches = a.take_sync_updates(limit).unwrap();
    assert!(batches.len() > 1);
    assert!(batches.iter().all(|x| x.bytes.len() <= limit));
    for x in &batches {
        b.apply_remote(x.doc, &[&x.bytes]).unwrap();
        a.sync_acked(x.doc, &x.end);
        // Снимок между кусками (сжатие на сервере): половина транзакции файл не ломает.
        assert_mirror(&mut b, page);
    }
    assert_eq!(nodes(&mut a, page), nodes(&mut b, page));
    assert_mirror(&mut b, page);
    assert!(!a.sync_unacked());
}

#[test]
fn version_wire_format_round_trips_and_rejects_garbage() {
    let (mut a, _b, page) = pair();
    rect(&mut a, page);
    let v = a.sync_versions();
    let (_, version) = v.iter().find(|(k, _)| *k == DocKey::Page(page)).unwrap();
    let bytes = version.encode();
    assert_eq!(SyncVersion::decode(&bytes).unwrap(), *version);
    assert!(SyncVersion::decode(&bytes[..bytes.len() - 1]).is_none());
    assert!(SyncVersion::decode(&u32::MAX.to_le_bytes()).is_none());
    let k = DocKey::Page(page);
    assert_eq!(DocKey::parse(&k.to_string()), Some(k));
    assert_eq!(DocKey::parse("root"), Some(DocKey::Root));
    assert_eq!(DocKey::parse("../etc"), None);
}

#[test]
fn concurrent_revive_and_restore_converge_with_rebuild() {
    // a удаляет узел и отменяет удаление (новый `TreeID` с прежним `NodeId`), а b параллельно его переносит:
    // перенос может оживить старый узел. У обоих — одно и то же, ничего не потеряно.
    for _ in 0..8 {
        let (mut a, mut b, page) = pair();
        let id = rect(&mut a, page);
        let other = rect(&mut a, page);
        sync(&mut a, &mut b);
        a.transact(page, vec![], |tx| {
            tx.delete_nodes(&[id])?;
            Ok(vec![])
        })
        .unwrap();
        a.undo().unwrap();
        b.transact(page, vec![], |tx| {
            tx.move_node(id, None, 5)?;
            Ok(vec![])
        })
        .unwrap();
        b.transact(page, vec![], |tx| {
            tx.move_node(other, None, 0)?;
            Ok(vec![])
        })
        .unwrap();
        sync(&mut a, &mut b);
        sync(&mut a, &mut b);
        assert_mirror(&mut a, page);
        assert_mirror(&mut b, page);
        assert_eq!(nodes(&mut a, page), nodes(&mut b, page));
    }
}

#[derive(Debug, Clone)]
enum Op {
    Create {
        who: usize,
        parent: usize,
    },
    Move {
        who: usize,
        node: usize,
        parent: usize,
        index: usize,
    },
    Delete {
        who: usize,
        node: usize,
    },
    SetX {
        who: usize,
        node: usize,
        x: i32,
    },
    /// Доставка: пачки в перемешанном порядке и с повторами.
    Sync {
        seed: u64,
    },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (0usize..3, 0usize..40).prop_map(|(who, parent)| Op::Create { who, parent }),
        2 => (0usize..3, 0usize..40, 0usize..40, 0usize..6)
            .prop_map(|(who, node, parent, index)| Op::Move { who, node, parent, index }),
        1 => (0usize..3, 0usize..40).prop_map(|(who, node)| Op::Delete { who, node }),
        3 => (0usize..3, 0usize..40, -500i32..500).prop_map(|(who, node, x)| Op::SetX { who, node, x }),
        3 => any::<u64>().prop_map(|seed| Op::Sync { seed }),
    ]
}

fn shuffle<T>(v: &mut [T], mut seed: u64) {
    for i in (1..v.len()).rev() {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        v.swap(i, (seed >> 33) as usize % (i + 1));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Спека, раздел 13.2: случайные правки нескольких клиентов с перестановкой и повтором пачек; все приходят к одному дереву, зеркало совпадает с полной сборкой.
    #[test]
    fn random_edits_converge_and_mirror_matches_rebuild(ops in proptest::collection::vec(op(), 1..70)) {
        let doc = Document::new();
        let bytes = encode_full(&doc).unwrap();
        let mut docs: Vec<Document> = (0..3).map(|_| decode(&bytes).unwrap().document).collect();
        let page = docs[0].pages()[0].0;
        for d in &mut docs {
            d.sync_start();
            d.page(page).unwrap();
            for (k, v) in d.take_sync_subscribes() {
                d.sync_subscribed(k, v);
            }
        }
        let mut log: Vec<(usize, DocKey, Vec<u8>)> = Vec::new();
        let pick = |d: &mut Document, n: usize| {
            let all = d.page(page).unwrap().walk();
            if all.is_empty() { None } else { Some(all[n % all.len()]) }
        };
        for o in &ops {
            match o {
                Op::Create { who, parent } => {
                    let d = &mut docs[*who];
                    let p = pick(d, *parent).filter(|n| d.page(page).unwrap().kind(*n).unwrap() == NodeKind::Frame);
                    let kind = if parent % 2 == 0 { NodeKind::Frame } else { NodeKind::Rect };
                    let _ = d.transact(page, vec![], |tx| Ok(vec![tx.create_node(p, None, kind)?]));
                }
                Op::Move { who, node, parent, index } => {
                    let d = &mut docs[*who];
                    let (Some(n), p) = (pick(d, *node), pick(d, *parent)) else { continue };
                    let p = p.filter(|x| d.page(page).unwrap().kind(*x).unwrap() == NodeKind::Frame);
                    let _ = d.transact(page, vec![], |tx| { tx.move_node(n, p, *index)?; Ok(vec![n]) });
                }
                Op::Delete { who, node } => {
                    let d = &mut docs[*who];
                    let Some(n) = pick(d, *node) else { continue };
                    let _ = d.transact(page, vec![], |tx| { tx.delete_nodes(&[n])?; Ok(vec![]) });
                }
                Op::SetX { who, node, x } => {
                    let d = &mut docs[*who];
                    let Some(n) = pick(d, *node) else { continue };
                    let _ = d.transact(page, vec![], |tx| {
                        tx.set_props(n, &PropsPatch { x: Some(*x as f64), ..Default::default() })?;
                        Ok(vec![n])
                    });
                }
                Op::Sync { seed } => {
                    for (who, d) in docs.iter_mut().enumerate() {
                        for b in d.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                            log.push((who, b.doc, b.bytes));
                        }
                    }
                    let mut part: Vec<_> = log.clone();
                    shuffle(&mut part, *seed);
                    part.truncate(part.len() / 2 + 1);
                    for (who, doc, bytes) in part {
                        let to = (who + 1 + (*seed as usize % 2)) % 3;
                        docs[to].apply_remote(doc, &[&bytes]).unwrap();
                    }
                }
            }
            for d in &mut docs {
                assert_mirror(d, page);
            }
        }
        // Финал: всё всем в обычном порядке и ещё раз с повторами.
        for (who, d) in docs.iter_mut().enumerate() {
            for b in d.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                log.push((who, b.doc, b.bytes));
            }
        }
        for round in 0..2 {
            let _ = round;
            for (_, doc, bytes) in &log {
                for d in &mut docs {
                    d.apply_remote(*doc, &[bytes]).unwrap();
                }
            }
        }
        let want = nodes(&mut docs[0], page);
        for d in &mut docs {
            assert_mirror(d, page);
            prop_assert_eq!(&nodes(d, page), &want);
        }
    }
}
