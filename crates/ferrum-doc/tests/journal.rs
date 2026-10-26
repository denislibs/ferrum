//! Журнал изменений (этап 6): пачки обновлений Loro после файла восстанавливают
//! документ после сбоя — страницы, узлы, имя, фон и изображения; повреждённая или
//! неполная пачка — предупреждение, а не сбой; повтор пачки, уже вошедшей в файл,
//! безвреден; случайные правки с записью пачек и полных файлов в случайные моменты
//! восстанавливаются точно.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;
use proptest::prelude::*;

type PageState = (
    PageId,
    String,
    Option<Color>,
    Vec<(NodeId, Option<NodeId>, Props)>,
);

/// Всё, что видит пользователь: имя, страницы с фоном, узлы с родителями и свойствами.
fn state(doc: &mut Document) -> (String, Vec<PageState>) {
    let mut pages = Vec::new();
    for (id, name) in doc.pages() {
        let bg = doc.page_background(id);
        let page = doc.page(id).unwrap();
        let nodes = page
            .walk()
            .into_iter()
            .map(|n| (n, page.parent(n), page.props(n).unwrap()))
            .collect();
        pages.push((id, name, bg, nodes));
    }
    (doc.name(), pages)
}

fn rect(doc: &mut Document, page: PageId, x: f64) -> NodeId {
    doc.transact(page, vec![], |tx| {
        let id = tx.create_node(None, None, NodeKind::Rect)?;
        tx.set_props(
            id,
            &PropsPatch {
                x: Some(x),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    })
    .unwrap()[0]
}

/// Файл с фреймом на первой странице и открытый из него документ.
fn opened() -> (Vec<u8>, Document) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_node(None, None, NodeKind::Frame)?])
    })
    .unwrap();
    let bytes = encode(&doc).unwrap();
    let doc = decode(&bytes).unwrap().document;
    (bytes, doc)
}

fn recover(file: &[u8], batches: &[&[u8]]) -> (Document, Replayed) {
    let mut doc = decode(file).unwrap().document;
    let replayed = doc.apply_journal(batches);
    (doc, replayed)
}

#[test]
fn no_changes_no_batch() {
    let (_, mut doc) = opened();
    assert_eq!(doc.take_journal().unwrap(), None, "открытие — не правка");
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 10.0);
    assert!(doc.take_journal().unwrap().is_some());
    assert_eq!(doc.take_journal().unwrap(), None, "всё уже отдано");
}

#[test]
fn edits_after_open_are_restored_on_top_of_the_file() {
    let (file, mut doc) = opened();
    let p1 = doc.pages()[0].0;
    let r = rect(&mut doc, p1, 10.0);
    doc.set_name("Лендинг").unwrap();
    doc.rename_page(p1, "Главная").unwrap();
    doc.set_page_background(p1, Some(Color::WHITE)).unwrap();
    let hash = doc.images.add(vec![7; 100]);
    doc.transact(p1, vec![], |tx| {
        tx.set_props(
            r,
            &PropsPatch {
                fills: Some(vec![Fill::image(hash.clone())]),
                ..Default::default()
            },
        )?;
        Ok(vec![r])
    })
    .unwrap();
    let b1 = doc.take_journal().unwrap().unwrap();
    let p2 = doc.add_page("Вторая").unwrap();
    rect(&mut doc, p2, 5.0);
    doc.transact(p1, vec![], |tx| {
        tx.delete_nodes(&[r])?;
        Ok(vec![])
    })
    .unwrap();
    doc.undo().unwrap();
    let b2 = doc.take_journal().unwrap().unwrap();
    let (mut back, replayed) = recover(&file, &[&b1, &b2]);
    assert_eq!(replayed.applied, 2);
    assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
    assert_eq!(state(&mut back), state(&mut doc));
    assert_eq!(back.images.get(&hash), Some(&[7u8; 100][..]));
    // Восстановленное — уже в журнале: следующая пачка — только новые правки.
    assert_eq!(back.take_journal().unwrap(), None);
    // И переживает полное сохранение.
    let mut again = decode(&encode(&back).unwrap()).unwrap().document;
    assert_eq!(state(&mut again), state(&mut doc));
    assert!(again.images.get(&hash).is_some());
}

#[test]
fn batch_already_in_the_file_is_harmless() {
    let (_, mut doc) = opened();
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 1.0);
    let b1 = doc.take_journal().unwrap().unwrap();
    // Полное сохранение (журнал до него стирается, но пусть пачка осталась).
    let file2 = encode(&doc).unwrap();
    rect(&mut doc, page, 2.0);
    let b2 = doc.take_journal().unwrap().unwrap();
    for batches in [vec![&b2[..]], vec![&b1[..], &b2[..]]] {
        let (mut back, replayed) = recover(&file2, &batches);
        assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
        assert_eq!(state(&mut back), state(&mut doc));
    }
}

#[test]
fn damaged_batch_is_skipped_with_one_warning() {
    let (file, mut doc) = opened();
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 1.0);
    let good = doc.take_journal().unwrap().unwrap();
    let mut flipped = good.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 0xff;
    let truncated = good[..good.len() - 5].to_vec();
    let before = state(&mut decode(&file).unwrap().document);
    for bad in [flipped, truncated, b"garbage".to_vec(), Vec::new()] {
        let (mut back, replayed) = recover(&file, &[&bad]);
        assert_eq!(replayed.applied, 0);
        assert_eq!(replayed.warnings.len(), 1, "{:?}", replayed.warnings);
        assert!(replayed.warnings[0].contains("журнал изменений повреждён"));
        assert_eq!(state(&mut back), before, "документ — как в файле");
    }
}

#[test]
fn missing_earlier_batch_is_reported() {
    let (file, mut doc) = opened();
    let page = doc.pages()[0].0;
    let r = rect(&mut doc, page, 1.0);
    doc.take_journal().unwrap().unwrap();
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            r,
            &PropsPatch {
                x: Some(50.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let b2 = doc.take_journal().unwrap().unwrap();
    let (_, replayed) = recover(&file, &[&b2]);
    assert_eq!(
        replayed.warnings.len(),
        1,
        "правка узла, которого нет в файле"
    );
}

#[test]
fn deleted_page_and_its_edits_are_dropped() {
    let (file, mut doc) = opened();
    let p2 = doc.add_page("Вторая").unwrap();
    rect(&mut doc, p2, 3.0);
    let b1 = doc.take_journal().unwrap().unwrap();
    doc.delete_page(p2).unwrap();
    let b2 = doc.take_journal().unwrap().unwrap();
    let (mut back, replayed) = recover(&file, &[&b1, &b2]);
    assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
    assert_eq!(back.pages().len(), 1);
    assert_eq!(state(&mut back), state(&mut doc));
}

#[test]
fn mark_journal_skips_what_is_already_there() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 1.0);
    doc.images.add(vec![1, 2, 3]);
    doc.mark_journal();
    assert_eq!(doc.take_journal().unwrap(), None);
}

#[derive(Debug, Clone)]
enum Op {
    Rect(i16),
    MoveFirst(i16),
    DeleteLast,
    Undo,
    Redo,
    AddPage,
    RenamePage(u8),
    Image(u8),
    /// Пачка журнала.
    Flush,
    /// Полное сохранение: пачки до него больше не нужны.
    Save,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => any::<i16>().prop_map(Op::Rect),
        2 => any::<i16>().prop_map(Op::MoveFirst),
        1 => Just(Op::DeleteLast),
        1 => Just(Op::Undo),
        1 => Just(Op::Redo),
        1 => Just(Op::AddPage),
        1 => any::<u8>().prop_map(Op::RenamePage),
        1 => any::<u8>().prop_map(Op::Image),
        2 => Just(Op::Flush),
        1 => Just(Op::Save),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 48,
        rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x6a_0e_2a_06),
        ..ProptestConfig::default()
    })]
    #[test]
    fn file_plus_journal_restores_the_document(ops in prop::collection::vec(op(), 1..40)) {
        let (mut file, mut doc) = opened();
        let mut batches: Vec<Vec<u8>> = Vec::new();
        for op in ops {
            let pages = doc.pages();
            let page = pages.last().unwrap().0;
            match op {
                Op::Rect(x) => { rect(&mut doc, page, x as f64); }
                Op::MoveFirst(x) => {
                    if let Some(&id) = doc.page(page).unwrap().children(None).first() {
                        doc.transact(page, vec![], |tx| {
                            tx.set_props(id, &PropsPatch { x: Some(x as f64), ..Default::default() })?;
                            Ok(vec![])
                        }).unwrap();
                    }
                }
                Op::DeleteLast => {
                    if let Some(&id) = doc.page(page).unwrap().children(None).last() {
                        doc.transact(page, vec![], |tx| { tx.delete_nodes(&[id])?; Ok(vec![]) }).unwrap();
                    }
                }
                Op::Undo => { let _ = doc.undo(); }
                Op::Redo => { let _ = doc.redo(); }
                Op::AddPage => { doc.add_page("Ещё").unwrap(); }
                Op::RenamePage(n) => doc.rename_page(page, &format!("Страница {n}")).unwrap(),
                Op::Image(n) => { doc.images.add(vec![n; 16]); }
                Op::Flush => {
                    if let Some(b) = doc.take_journal().unwrap() { batches.push(b); }
                }
                Op::Save => {
                    // Как автосохранение: сначала пачка, затем файл; пачки до него стираются.
                    doc.take_journal().unwrap();
                    file = encode(&doc).unwrap();
                    batches.clear();
                }
            }
        }
        // Сбой после последней пачки: восстанавливаем файл и все пачки после него.
        if let Some(b) = doc.take_journal().unwrap() { batches.push(b); }
        let refs: Vec<&[u8]> = batches.iter().map(Vec::as_slice).collect();
        let (mut back, replayed) = recover(&file, &refs);
        prop_assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
        prop_assert_eq!(state(&mut back), state(&mut doc));
    }
}
