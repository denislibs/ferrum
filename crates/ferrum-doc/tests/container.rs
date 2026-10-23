use ferrum_doc::container::{decode, encode, FORMAT_VERSION, MAGIC};
use ferrum_doc::*;

fn sample() -> (Document, PageId, PageId, NodeId, String) {
    let mut doc = Document::new();
    doc.set_name("Лендинг кофейни").unwrap();
    let p1 = doc.pages()[0].0;
    let p2 = doc.add_page("Черновики").unwrap();
    let hash = doc.images.add(vec![9; 1000]);
    let _orphan = doc.images.add(vec![7; 10]);
    let h = hash.clone();
    let mut node = None;
    doc.transact(p1, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        p.set_props(
            f,
            &PropsPatch {
                w: Some(393.0),
                h: Some(852.0),
                ..Default::default()
            },
        )?;
        let img = p.create_node(Some(f), None, NodeKind::Image)?;
        p.set_props(
            img,
            &PropsPatch {
                fills: Some(vec![Fill::image(h)]),
                ..Default::default()
            },
        )?;
        node = Some(f);
        Ok(vec![f])
    })
    .unwrap();
    (doc, p1, p2, node.unwrap(), hash)
}

#[test]
fn roundtrip_keeps_pages_nodes_images() {
    let (doc, p1, p2, frame, hash) = sample();
    let bytes = encode(&doc).unwrap();
    assert_eq!(&bytes[..8], MAGIC);
    let mut opened = decode(&bytes).unwrap();
    assert!(opened.warnings.is_empty(), "{:?}", opened.warnings);
    let d = &mut opened.document;
    assert_eq!(d.name(), "Лендинг кофейни");
    assert_eq!(
        d.pages().iter().map(|p| p.0).collect::<Vec<_>>(),
        vec![p1, p2]
    );
    assert!(!d.is_page_loaded(p1), "страницы грузятся лениво");
    assert_eq!(d.page(p1).unwrap().props(frame).unwrap().w, 393.0);
    assert!(d.is_page_loaded(p1));
    assert_eq!(d.images.get(&hash).unwrap().len(), 1000);
}

#[test]
fn unreferenced_images_are_not_saved() {
    let (doc, ..) = sample();
    let opened = decode(&encode(&doc).unwrap()).unwrap();
    assert_eq!(opened.document.images.len(), 1);
}

#[test]
fn unloaded_pages_survive_resave() {
    let (doc, p1, _, frame, hash) = sample();
    let once = decode(&encode(&doc).unwrap()).unwrap().document;
    let mut twice = decode(&encode(&once).unwrap()).unwrap().document;
    assert_eq!(twice.page(p1).unwrap().props(frame).unwrap().h, 852.0);
    assert!(
        twice.images.get(&hash).is_some(),
        "ссылки незагруженной страницы сохраняются"
    );
}

#[test]
fn rejects_non_ferrum_and_future_format() {
    assert!(matches!(
        decode(b"PNG garbage here...."),
        Err(DocError::NotAFerrumFile)
    ));
    let mut bytes = encode(&Document::new()).unwrap();
    bytes[8..12].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
    assert!(matches!(
        decode(&bytes),
        Err(DocError::UnsupportedFormat(_))
    ));
}

#[test]
fn corrupted_image_is_skipped_with_warning() {
    let (doc, ..) = sample();
    let mut bytes = encode(&doc).unwrap();
    let last = bytes.len() - 1; // последняя секция — изображение
    bytes[last] ^= 0xff;
    let opened = decode(&bytes).unwrap();
    assert_eq!(opened.document.images.len(), 0);
    assert!(
        opened
            .warnings
            .iter()
            .any(|w| w.contains("изображений — 1")),
        "{:?}",
        opened.warnings
    );
}

#[test]
fn truncated_file_opens_what_it_can() {
    let (doc, p1, ..) = sample();
    let bytes = encode(&doc).unwrap();
    let cut = &bytes[..bytes.len() - 10];
    let mut opened = decode(cut).unwrap();
    assert!(opened.warnings.iter().any(|w| w.contains("обрезан")));
    assert_eq!(opened.document.page(p1).unwrap().children(None).len(), 1);
}

#[test]
fn newer_schema_opens_read_only() {
    let mut doc = Document::new();
    doc.set_name("x").unwrap();
    let bytes = encode(&doc).unwrap();
    // Подменяем версию схемы через публичный API нельзя — собираем документ из будущего вручную.
    let future = ferrum_doc::container::testing::with_schema_version(&bytes, SCHEMA_VERSION + 1);
    let mut opened = decode(&future).unwrap();
    assert!(opened.document.is_read_only());
    assert!(opened
        .warnings
        .iter()
        .any(|w| w.contains("только для чтения")));
    let page = opened.document.pages()[0].0;
    let err = opened.document.transact(page, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
    });
    assert!(matches!(err, Err(DocError::ReadOnly)));
}

#[test]
fn scene_cache_is_available_before_page_loads() {
    let (doc, p1, _, frame, hash) = sample();
    let mut opened = decode(&encode(&doc).unwrap()).unwrap().document;
    let rows = opened.cached_scene(p1).expect("кэш сцены записан");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, frame);
    assert_eq!(rows[0].parent, None);
    assert_eq!(rows[0].props.w, 393.0);
    assert_eq!(rows[1].parent, Some(0));
    assert_eq!(
        rows[1].props.fills[0].image_hash.as_deref(),
        Some(hash.as_str())
    );
    // Кэш совпадает с тем, что даёт загруженный Loro.
    let expected = opened.page(p1).unwrap().scene_rows().unwrap();
    let again = decode(&encode(&doc).unwrap()).unwrap().document;
    assert_eq!(again.cached_scene(p1).unwrap(), expected.as_slice());
}

#[test]
fn broken_scene_cache_checksum_is_ignored() {
    let (doc, p1, ..) = sample();
    let mut bytes = encode(&doc).unwrap();
    // Портим первый байт данных кэша: контрольная сумма секции не сходится, секция пропускается.
    let pos = ferrum_doc::container::testing::section_data_offset(&bytes, 5).unwrap();
    bytes[pos] ^= 0xff;
    let mut opened = decode(&bytes).unwrap();
    assert!(opened.document.cached_scene(p1).is_none());
    assert!(
        opened.warnings.is_empty(),
        "кэш необязателен — без предупреждений"
    );
    assert_eq!(opened.document.page(p1).unwrap().children(None).len(), 1);
}

#[test]
fn stale_scene_cache_is_ignored() {
    let (doc, p1, ..) = sample();
    // Контрольная сумма секции верна, но префикс-хэш снапшота не совпадает со страницей.
    let bytes =
        ferrum_doc::container::testing::map_sections(&encode(&doc).unwrap(), |kind, _, data| {
            if kind == 5 {
                for b in &mut data[..32] {
                    *b ^= 0xff;
                }
            }
        });
    let mut opened = decode(&bytes).unwrap();
    assert!(opened.document.cached_scene(p1).is_none());
    assert!(
        opened.warnings.is_empty(),
        "устаревший кэш необязателен — без предупреждений"
    );
    assert_eq!(opened.document.page(p1).unwrap().children(None).len(), 1);
}

#[test]
fn image_with_mismatched_id_is_skipped_with_warning() {
    let (doc, ..) = sample();
    // Секция картинки с верной контрольной суммой, но id, который не равен хэшу содержимого.
    let bytes =
        ferrum_doc::container::testing::map_sections(&encode(&doc).unwrap(), |kind, id, _| {
            if kind == 4 {
                *id = "0".repeat(64);
            }
        });
    let opened = decode(&bytes).unwrap();
    assert_eq!(opened.document.images.len(), 0);
    assert!(
        opened
            .warnings
            .iter()
            .any(|w| w.contains("изображений — 1")),
        "{:?}",
        opened.warnings
    );
}

#[test]
fn corrupted_page_refs_are_rebuilt_from_snapshot() {
    let (doc, _p1, _p2, _frame, hash) = sample();
    let mut bytes = encode(&doc).unwrap();
    // Портим данные первой секции списка ссылок (страница 1).
    let pos = ferrum_doc::container::testing::section_data_offset(&bytes, 3).unwrap();
    bytes[pos] ^= 0xff;
    let opened = decode(&bytes).unwrap();
    assert!(
        opened
            .warnings
            .iter()
            .any(|w| w.contains("восстановлен из её содержимого")),
        "{:?}",
        opened.warnings
    );
    // Пересохранение не теряет картинку страницы.
    let again = decode(&encode(&opened.document).unwrap()).unwrap();
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    assert!(again.document.images.get(&hash).is_some());
}

#[test]
fn corrupted_root_is_an_error() {
    let (doc, ..) = sample();
    let mut bytes = encode(&doc).unwrap();
    // Первая секция файла — корневой документ.
    let pos = ferrum_doc::container::testing::section_data_offset(&bytes, 1).unwrap();
    bytes[pos] ^= 0xff;
    assert!(matches!(decode(&bytes), Err(DocError::Corrupted(_))));
}

#[test]
fn corrupted_page_opens_empty_while_others_survive() {
    let mut doc = Document::new();
    let p1 = doc.pages()[0].0;
    let p2 = doc.add_page("Вторая").unwrap();
    doc.transact(p2, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    doc.transact(p1, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    let mut bytes = encode(&doc).unwrap();
    // Первая секция страницы (тип 2) — страница 1.
    let pos = ferrum_doc::container::testing::section_data_offset(&bytes, 2).unwrap();
    bytes[pos] ^= 0xff;
    let mut opened = decode(&bytes).unwrap();
    assert!(
        opened
            .warnings
            .iter()
            .any(|w| w.contains("повреждена и открыта пустой")),
        "{:?}",
        opened.warnings
    );
    assert_eq!(opened.document.page(p1).unwrap().children(None).len(), 0);
    assert_eq!(opened.document.page(p2).unwrap().children(None).len(), 1);
}

#[test]
fn opened_file_has_clean_undo_history() {
    let (doc, p1, _, frame, _) = sample();
    let mut opened = decode(&encode(&doc).unwrap()).unwrap().document;
    assert!(!opened.can_undo(), "прогрев не попадает в историю");
    opened
        .transact(p1, vec![], |p| {
            p.set_props(
                frame,
                &PropsPatch {
                    x: Some(50.0),
                    ..Default::default()
                },
            )?;
            Ok(vec![frame])
        })
        .unwrap();
    opened.undo().unwrap();
    assert!(opened.undo().unwrap().is_none());
    assert_eq!(opened.page(p1).unwrap().props(frame).unwrap().x, 0.0);
}
