//! Отражение узла (`flipX`, `flipY`): нейтральное `false` не записывается, флаги
//! переживают файл, кэш сцены, отмену и повтор, а старые файлы открываются без них.

use ferrum_doc::container::{decode, encode, testing, SCENE_CACHE_VERSION};
use ferrum_doc::*;
use loro::{ExportMode, LoroDoc, LoroValue, ValueOrContainer};

fn flips(x: bool, y: bool) -> PropsPatch {
    PropsPatch {
        flip_x: Some(x),
        flip_y: Some(y),
        ..Default::default()
    }
}

/// Документ: фрейм с отражением по x и ребёнком с отражением по y, прямоугольник без
/// отражения в корне.
fn sample() -> (Document, PageId, [NodeId; 3]) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut ids = Vec::new();
    doc.transact(page, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        p.set_props(
            f,
            &PropsPatch {
                x: Some(10.0),
                w: Some(200.0),
                h: Some(100.0),
                ..flips(true, false)
            },
        )?;
        let c = p.create_node(Some(f), None, NodeKind::Rect)?;
        p.set_props(c, &flips(false, true))?;
        let r = p.create_node(None, None, NodeKind::Rect)?;
        ids.extend([f, c, r]);
        Ok(vec![])
    })
    .unwrap();
    (doc, page, [ids[0], ids[1], ids[2]])
}

fn flip_of(doc: &mut Document, page: PageId, id: NodeId) -> (bool, bool) {
    let p = doc.page(page).unwrap().props(id).unwrap();
    (p.flip_x, p.flip_y)
}

/// Ключи узлов страницы прямо из Loro: (ключ, значение) каждого узла.
fn raw_keys(snapshot: &[u8]) -> Vec<Vec<(String, LoroValue)>> {
    let doc = LoroDoc::from_snapshot(snapshot).unwrap();
    let tree = doc.get_tree("nodes");
    tree.nodes()
        .into_iter()
        .filter(|t| !tree.is_node_deleted(t).unwrap_or(true))
        .map(|t| {
            let meta = tree.get_meta(t).unwrap();
            let mut keys = Vec::new();
            meta.for_each(|k, v| {
                if let ValueOrContainer::Value(v) = v {
                    keys.push((k.to_string(), v));
                }
            });
            keys
        })
        .collect()
}

#[test]
fn defaults_are_not_mirrored() {
    for kind in [
        NodeKind::Frame,
        NodeKind::Group,
        NodeKind::Rect,
        NodeKind::Ellipse,
        NodeKind::Line,
        NodeKind::Image,
    ] {
        let d = Props::defaults(kind);
        assert!(!d.flip_x && !d.flip_y, "{kind:?}");
    }
}

#[test]
fn neutral_flips_are_not_written_and_true_is() {
    let page = PageDoc::new();
    let tx = page.tx_for_tests();
    let plain = tx.create_node(None, None, NodeKind::Rect).unwrap();
    let mirrored = tx.create_node(None, None, NodeKind::Rect).unwrap();
    tx.set_props(
        mirrored,
        &PropsPatch {
            flip_x: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    page.commit_for_tests();
    let nodes = raw_keys(&page.export_snapshot().unwrap());
    assert_eq!(nodes.len(), 2);
    let has = |keys: &[(String, LoroValue)], k: &str| keys.iter().find(|(n, _)| n == k).cloned();
    let id_of = |keys: &[(String, LoroValue)]| match has(keys, "id") {
        Some((_, LoroValue::String(s))) => s.to_string(),
        other => panic!("нет id: {other:?}"),
    };
    let plain_keys = nodes
        .iter()
        .find(|k| id_of(k) == plain.to_string())
        .unwrap();
    assert!(has(plain_keys, "flipX").is_none(), "{plain_keys:?}");
    assert!(has(plain_keys, "flipY").is_none(), "{plain_keys:?}");
    let mirrored_keys = nodes
        .iter()
        .find(|k| id_of(k) == mirrored.to_string())
        .unwrap();
    assert_eq!(
        has(mirrored_keys, "flipX").map(|(_, v)| v),
        Some(LoroValue::Bool(true))
    );
    assert!(has(mirrored_keys, "flipY").is_none());
    let p = page.props(mirrored).unwrap();
    assert!(p.flip_x && !p.flip_y);
}

#[test]
fn flips_roundtrip_through_file_cache_and_loro() {
    let (mut doc, page, [f, c, r]) = sample();
    let bytes = encode(&doc).unwrap();
    let expected = doc.page(page).unwrap().scene_rows().unwrap();

    // Через кэш сцены: строки таблицы несут флаги.
    let mut opened = decode(&bytes).unwrap();
    assert!(opened.warnings.is_empty(), "{:?}", opened.warnings);
    assert_eq!(
        opened.document.cached_scene(page),
        Some(expected.as_slice())
    );
    let d = &mut opened.document;
    assert!(d.page(page).unwrap().loaded_from_cache());
    assert_eq!(flip_of(d, page, f), (true, false));
    assert_eq!(flip_of(d, page, c), (false, true));
    assert_eq!(flip_of(d, page, r), (false, false));

    // Без кэша — из Loro: те же флаги.
    let no_cache = testing::map_sections(&bytes, |kind, _, data| {
        if kind == 5 {
            data.truncate(10);
        }
    });
    let mut opened = decode(&no_cache).unwrap();
    let d = &mut opened.document;
    assert!(d.cached_scene(page).is_none());
    assert!(!d.page(page).unwrap().loaded_from_cache());
    assert_eq!(flip_of(d, page, f), (true, false));
    assert_eq!(flip_of(d, page, c), (false, true));
    assert_eq!(flip_of(d, page, r), (false, false));
}

#[test]
fn flip_patch_is_one_step_reported_in_changes() {
    let (mut doc, page, [f, _, r]) = sample();
    doc.take_changes(page);
    doc.transact(page, vec![r], |p| {
        p.set_props(r, &flips(true, true))?;
        p.set_props(f, &flips(false, false))?;
        Ok(vec![r])
    })
    .unwrap();
    let ch = doc.take_changes(page);
    assert!(ch.props.contains(&r) && ch.props.contains(&f), "{ch:?}");
    assert_eq!(flip_of(&mut doc, page, r), (true, true));
    assert_eq!(flip_of(&mut doc, page, f), (false, false));

    doc.undo().unwrap().expect("шаг отмены");
    let ch = doc.take_changes(page);
    assert!(ch.props.contains(&r) && ch.props.contains(&f), "{ch:?}");
    assert_eq!(flip_of(&mut doc, page, r), (false, false));
    assert_eq!(flip_of(&mut doc, page, f), (true, false));

    doc.redo().unwrap().expect("шаг повтора");
    let ch = doc.take_changes(page);
    assert!(ch.props.contains(&r) && ch.props.contains(&f), "{ch:?}");
    assert_eq!(flip_of(&mut doc, page, r), (true, true));
    assert_eq!(flip_of(&mut doc, page, f), (false, false));
}

#[test]
fn undo_of_delete_restores_flips() {
    let (mut doc, page, [f, c, _]) = sample();
    doc.transact(page, vec![], |p| {
        p.delete_nodes(&[f])?;
        Ok(vec![])
    })
    .unwrap();
    doc.undo().unwrap();
    assert_eq!(flip_of(&mut doc, page, f), (true, false));
    assert_eq!(flip_of(&mut doc, page, c), (false, true));
}

/// Файл записан сборкой до отражения (efa34d1): кэш сцены версии 1, ключей
/// `flipX`/`flipY` нет.
const PRE_FLIP: &[u8] = include_bytes!("data/pre_flip.ferrum");

#[test]
fn file_from_before_flips_opens_unmirrored() {
    let mut opened = decode(PRE_FLIP).unwrap();
    assert!(opened.warnings.is_empty(), "{:?}", opened.warnings);
    let d = &mut opened.document;
    let page = d.pages()[0].0;
    // Таблица старой версии не используется: в ней нет флагов отражения, а в Loro они
    // могли появиться (старая сборка пересохранила файл новой). Страница — из Loro.
    assert!(d.cached_scene(page).is_none());
    let p = d.page(page).unwrap();
    assert!(!p.loaded_from_cache());
    let walk = p.walk();
    assert_eq!(walk.len(), 2);
    let frame = p.props(walk[0]).unwrap();
    assert_eq!(frame.kind, NodeKind::Frame);
    assert_eq!(
        (frame.x, frame.y, frame.w, frame.h, frame.rotation),
        (10.0, 20.0, 200.0, 100.0, 30.0)
    );
    assert_eq!(frame.name, "Экран");
    let rect = p.props(walk[1]).unwrap();
    assert_eq!((rect.x, rect.y, rect.w, rect.h), (5.0, 6.0, 40.0, 30.0));
    for props in [&frame, &rect] {
        assert!(!props.flip_x && !props.flip_y);
    }
    // Пересохранённый файл пишет кэш новой версии, и он принимается.
    let resaved = decode(&encode(d).unwrap()).unwrap();
    assert!(resaved.document.cached_scene(page).is_some());
}

#[test]
fn scene_cache_version_covers_flips() {
    assert_eq!(
        SCENE_CACHE_VERSION, 4,
        "таблица с флагами отражения (2), текстом (3), стилем обводки и сетью (4)"
    );
    // Кэш прежней версии с честным хэшем снапшота игнорируется без предупреждений.
    let (doc, page, [f, ..]) = sample();
    let bytes = testing::map_sections(&encode(&doc).unwrap(), |kind, _, data| {
        if kind == 5 {
            data[32] = 1;
        }
    });
    let mut opened = decode(&bytes).unwrap();
    assert!(opened.warnings.is_empty(), "{:?}", opened.warnings);
    assert!(opened.document.cached_scene(page).is_none());
    assert_eq!(flip_of(&mut opened.document, page, f), (true, false));
}

#[test]
fn unknown_keys_survive_flip_edit_and_resave() {
    // Узел с ключом из будущей версии.
    let page = PageDoc::new();
    let id = page
        .tx_for_tests()
        .create_node(None, None, NodeKind::Rect)
        .unwrap();
    page.commit_for_tests();
    let raw = LoroDoc::from_snapshot(&page.export_snapshot().unwrap()).unwrap();
    let tree = raw.get_tree("nodes");
    let tid = tree.nodes()[0];
    tree.get_meta(tid)
        .unwrap()
        .insert("blendMode", "multiply")
        .unwrap();
    raw.commit();
    let future = raw.export(ExportMode::Snapshot).unwrap();

    let page = PageDoc::from_snapshot(&future).unwrap();
    page.tx_for_tests()
        .set_props(id, &flips(true, false))
        .unwrap();
    page.commit_for_tests();
    let keys = &raw_keys(&page.export_snapshot().unwrap())[0];
    let get = |k: &str| keys.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    assert_eq!(get("blendMode"), Some(LoroValue::from("multiply")));
    assert_eq!(get("flipX"), Some(LoroValue::Bool(true)));
    let p = page.props(id).unwrap();
    assert!(p.flip_x && !p.flip_y);
}
