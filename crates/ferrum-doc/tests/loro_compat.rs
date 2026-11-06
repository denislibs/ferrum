//! Совместимость Loro с сервером (спека подсистемы 4, разделы 3 и 13.2): что написал `loro-crdt` на Bun, читает `loro` ядра,
//! и наоборот (`apps/server/test/loro-compat.test.ts`). Файлы данных пишут `LORO_COMPAT_WRITE=1 cargo test`
//! и однострочник в начале теста сервера; при смене версии Loro они пересобираются вместе.

use ferrum_doc::container::{decode, encode_full};
use ferrum_doc::fixture::page_with_nodes;
use ferrum_doc::*;
use loro::{ExportMode, LoroDoc, LoroValue, TreeParentId, ValueOrContainer};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/loro-compat");

#[test]
fn server_blobs_read_and_core_blobs_written() {
    let doc = LoroDoc::new();
    let tree = doc.get_tree("nodes");
    tree.enable_fractional_index(0);
    let node = tree.create(TreeParentId::Root).unwrap();
    doc.commit();
    let snapshot = doc.export(ExportMode::Snapshot).unwrap();
    let from = doc.oplog_vv();
    tree.get_meta(node).unwrap().insert("x", 42.0).unwrap();
    doc.commit();
    let update = doc.export(ExportMode::updates(&from)).unwrap();
    if std::env::var_os("LORO_COMPAT_WRITE").is_some() {
        std::fs::create_dir_all(DIR).unwrap();
        std::fs::write(format!("{DIR}/rust.snapshot"), &snapshot).unwrap();
        std::fs::write(format!("{DIR}/rust.update"), &update).unwrap();
    }

    let js = std::fs::read(format!("{DIR}/js.snapshot")).unwrap();
    let read = LoroDoc::from_snapshot(&js).unwrap();
    match read.get_map("m").get("hello") {
        Some(ValueOrContainer::Value(LoroValue::String(s))) => assert_eq!(&*s, "привет"),
        other => panic!("{other:?}"),
    }
    // Снимок страницы настоящего размера тоже читается: его заберёт сжатие на сервере.
    let (page, _) = page_with_nodes(10).unwrap();
    let meta =
        LoroDoc::decode_import_blob_meta(&page.export_full_snapshot().unwrap(), false).unwrap();
    assert!(meta.partial_end_vv.iter().count() >= 1);
}

/// Страница настоящей схемы: снимок и журнал пишет ядро, сервер (`compact`) собирает новый снимок, читает его снова ядро.
/// Текст с разметкой — главный риск: у `loro-crdt` на сервере нет настройки расширения разметки.
#[test]
fn page_compacted_by_server_reads_back_with_text_and_fills() {
    let doc = Document::new();
    let page = doc.pages()[0].0;
    let first = encode_full(&doc).unwrap();
    let mut a = decode(&first).unwrap().document;
    a.sync_start();
    let ids = a
        .transact(page, vec![], |tx| {
            let t = tx.create_node(None, None, NodeKind::Text)?;
            tx.insert_text(t, 0, "Привет, мир", None)?;
            tx.format_text(
                t,
                0..6,
                &TextAttrs {
                    size: Some(32.0),
                    ..Default::default()
                },
                &[],
            )?;
            let r = tx.create_node(None, None, NodeKind::Rect)?;
            tx.set_props(
                r,
                &PropsPatch {
                    fills: Some(vec![Fill::image("a".repeat(64))]),
                    ..Default::default()
                },
            )?;
            Ok(vec![t, r])
        })
        .unwrap();
    let snapshot = a.page(page).unwrap().export_full_snapshot().unwrap();
    if std::env::var_os("LORO_COMPAT_WRITE").is_some() {
        std::fs::write(format!("{DIR}/rust-page.snapshot"), &snapshot).unwrap();
        return;
    }
    let bytes = std::fs::read(format!("{DIR}/js-page.snapshot")).unwrap();
    let read = PageDoc::from_snapshot(&bytes).unwrap();
    let mut texts = read
        .walk()
        .into_iter()
        .filter_map(|n| read.props(n).unwrap().text.map(|t| t.content.clone()));
    let text = texts.next().expect("текст на месте");
    assert_eq!(text.text(), "Привет, мир\n");
    assert!(text.runs.len() >= 2, "разметка на месте: {:?}", text.runs);
    assert_eq!(text.runs[0].attrs.size, Some(32.0));
    assert!(read.image_refs().unwrap().contains(&"a".repeat(64)));
    assert_eq!(ids.len(), 2);
}
