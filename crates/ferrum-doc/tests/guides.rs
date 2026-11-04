//! Направляющие в документе и шаги отмены страниц (спека подсистемы 2, разделы 9–11).

use ferrum_doc::container::{decode, encode, testing};
use ferrum_doc::{
    DocError, Document, Guide, GuideAxis, GuideOwner, NodeId, NodeKind, PageId, PropsPatch,
    MAX_GUIDES,
};

fn gx(v: f64) -> Guide {
    Guide::new(GuideAxis::X, v)
}
fn gy(v: f64) -> Guide {
    Guide::new(GuideAxis::Y, v)
}

fn doc_with_frame() -> (Document, PageId, NodeId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |tx| {
            let f = tx.create_node(None, None, NodeKind::Frame)?;
            let r = tx.create_node(None, None, NodeKind::Rect)?;
            Ok(vec![f, r])
        })
        .unwrap();
    doc.clear_history();
    (doc, page, ids[0], ids[1])
}

fn frame_guides(doc: &mut Document, page: PageId, id: NodeId) -> Vec<Guide> {
    let p = doc.page(page).unwrap().props(id).unwrap();
    p.guides.map(|g| g.to_vec()).unwrap_or_default()
}

#[test]
fn each_edit_is_one_undo_step_for_page_and_frame() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(page, vec![], &[(GuideOwner::Page(page), vec![gx(10.0)])])
        .unwrap();
    assert_eq!(doc.page_guides(page), [gx(10.0)]);
    let first = doc.last_step().expect("шаг есть");
    doc.set_guides(page, vec![], &[(GuideOwner::Frame(frame), vec![gy(5.0)])])
        .unwrap();
    assert_eq!(frame_guides(&mut doc, page, frame), [gy(5.0)]);
    assert_ne!(doc.last_step(), Some(first));

    doc.undo().unwrap();
    assert!(frame_guides(&mut doc, page, frame).is_empty());
    assert_eq!(
        doc.page_guides(page),
        [gx(10.0)],
        "шаг страницы ещё не отменён"
    );
    doc.undo().unwrap();
    assert!(doc.page_guides(page).is_empty());
    assert!(!doc.can_undo(), "ровно два шага");
    doc.redo().unwrap();
    doc.redo().unwrap();
    assert_eq!(doc.page_guides(page), [gx(10.0)]);
    assert_eq!(frame_guides(&mut doc, page, frame), [gy(5.0)]);
}

#[test]
fn moving_a_guide_between_page_and_frame_is_one_step() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(page, vec![], &[(GuideOwner::Page(page), vec![gx(10.0)])])
        .unwrap();
    doc.clear_history();
    doc.set_guides(
        page,
        vec![],
        &[
            (GuideOwner::Page(page), vec![]),
            (GuideOwner::Frame(frame), vec![gx(3.0)]),
        ],
    )
    .unwrap();
    assert!(doc.page_guides(page).is_empty());
    assert_eq!(frame_guides(&mut doc, page, frame), [gx(3.0)]);
    doc.undo().unwrap();
    assert!(!doc.can_undo());
    assert_eq!(doc.page_guides(page), [gx(10.0)]);
    assert!(frame_guides(&mut doc, page, frame).is_empty());
}

#[test]
fn same_guides_make_no_step() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(page, vec![], &[(GuideOwner::Page(page), vec![])])
        .unwrap();
    doc.set_guides(page, vec![], &[(GuideOwner::Frame(frame), vec![])])
        .unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn failed_edit_leaves_no_step_and_no_change() {
    let (mut doc, page, frame, rect) = doc_with_frame();
    let too_many = vec![gx(1.0); MAX_GUIDES + 1];
    for (owner, bad) in [
        (GuideOwner::Page(page), too_many.clone()),
        (GuideOwner::Frame(frame), too_many),
        (GuideOwner::Page(page), vec![gx(f64::NAN)]),
        (GuideOwner::Frame(frame), vec![gy(f64::INFINITY)]),
        (GuideOwner::Frame(frame), vec![gy(1.5e9)]),
    ] {
        let err = doc.set_guides(page, vec![], &[(owner, bad)]);
        assert!(matches!(err, Err(DocError::InvalidGuides(_))), "{err:?}");
    }
    // Не фрейм — отказ; и первая часть правки не остаётся.
    let err = doc.set_guides(
        page,
        vec![],
        &[
            (GuideOwner::Page(page), vec![gx(1.0)]),
            (GuideOwner::Frame(rect), vec![gx(1.0)]),
        ],
    );
    assert!(err.is_err());
    assert!(doc.page_guides(page).is_empty());
    assert!(!doc.can_undo());
    assert!(!doc.is_read_only());
}

#[test]
fn duplicates_are_dropped_on_write() {
    let (mut doc, page, _, _) = doc_with_frame();
    doc.set_guides(
        page,
        vec![],
        &[(GuideOwner::Page(page), vec![gx(1.0), gx(1.0), gy(1.0)])],
    )
    .unwrap();
    assert_eq!(doc.page_guides(page), [gx(1.0), gy(1.0)]);
}

#[test]
fn guides_patch_is_ignored_on_other_kinds() {
    let (mut doc, page, _, rect) = doc_with_frame();
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            rect,
            &PropsPatch {
                guides: Some(std::sync::Arc::new(vec![gx(1.0)])),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    assert!(doc
        .page(page)
        .unwrap()
        .props(rect)
        .unwrap()
        .guides
        .is_none());
    assert!(!doc.can_undo(), "правки нет — шага нет");
}

#[test]
fn guides_survive_save_open_and_scene_cache() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(
        page,
        vec![],
        &[
            (GuideOwner::Page(page), vec![gx(-12.5), gy(7.0)]),
            (GuideOwner::Frame(frame), vec![gy(5.0)]),
        ],
    )
    .unwrap();
    let bytes = encode(&doc).unwrap();
    let mut opened = decode(&bytes).unwrap();
    assert!(opened.warnings.is_empty());
    let d = &mut opened.document;
    assert_eq!(
        d.page_guides(page),
        [gx(-12.5), gy(7.0)],
        "без загрузки страницы"
    );
    let rows = d.cached_scene(page).expect("кэш принят");
    let row = rows.iter().find(|r| r.id == frame).unwrap();
    assert_eq!(row.props.guides.as_deref(), Some(&vec![gy(5.0)]));
    assert_eq!(frame_guides(d, page, frame), [gy(5.0)]);
    // И прямо из Loro, без кэша.
    let stale = testing::map_sections(&bytes, |kind, _, data| {
        if kind == 5 {
            data[32] = 1;
        }
    });
    let mut d = decode(&stale).unwrap().document;
    assert!(d.cached_scene(page).is_none());
    assert_eq!(frame_guides(&mut d, page, frame), [gy(5.0)]);
}

#[test]
fn bad_guides_in_file_are_dropped_and_document_stays_writable() {
    let (mut doc, page, frame, rect) = doc_with_frame();
    let raw = [
        ("x", 1.0),
        ("q", 2.0),
        ("y", f64::NAN),
        ("y", f64::NEG_INFINITY),
        ("y", 2e9),
        ("x", 1.0),
    ];
    doc.force_page_guides_for_tests(page, &raw);
    let p = doc.page(page).unwrap();
    p.force_guides_for_tests(frame, &raw);
    p.force_guides_for_tests(rect, &[("x", 1.0)]);
    let bytes = encode(&doc).unwrap();
    let no_cache = testing::map_sections(&bytes, |kind, _, data| {
        if kind == 5 {
            data[32] = 1;
        }
    });
    let mut d = decode(&no_cache).unwrap().document;
    assert_eq!(d.page_guides(page), [gx(1.0)]);
    assert_eq!(frame_guides(&mut d, page, frame), [gx(1.0)]);
    let p = d.page(page).unwrap().props(rect).unwrap();
    assert!(
        p.guides.is_none(),
        "у прямоугольника направляющих не бывает"
    );
    assert!(!d.is_read_only());
}

#[test]
fn more_than_the_limit_in_file_is_cut() {
    let (mut doc, page, frame, _) = doc_with_frame();
    let offsets: Vec<f64> = (0..MAX_GUIDES + 10).map(|i| i as f64).collect();
    let raw: Vec<(&str, f64)> = offsets.iter().map(|o| ("x", *o)).collect();
    doc.force_page_guides_for_tests(page, &raw);
    doc.page(page).unwrap().force_guides_for_tests(frame, &raw);
    assert_eq!(doc.page_guides(page).len(), MAX_GUIDES);
    let bytes = encode(&doc).unwrap();
    let no_cache = testing::map_sections(&bytes, |kind, _, data| {
        if kind == 5 {
            data[32] = 1;
        }
    });
    let mut d = decode(&no_cache).unwrap().document;
    assert_eq!(frame_guides(&mut d, page, frame).len(), MAX_GUIDES);
}

#[test]
fn nan_in_scene_cache_rejects_the_cache_not_the_file() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(page, vec![], &[(GuideOwner::Frame(frame), vec![gy(5.0)])])
        .unwrap();
    // Фрейм создан первым: подменяем смещение 5.0 внутри строки таблицы.
    let forged = testing::map_sections(&encode(&doc).unwrap(), |kind, _, data| {
        if kind == 5 {
            let five = 5.0f64.to_le_bytes();
            let at = data
                .windows(8)
                .rposition(|w| w == five)
                .expect("смещение в таблице");
            data[at..at + 8].copy_from_slice(&f64::NAN.to_le_bytes());
        }
    });
    let mut d = decode(&forged).unwrap().document;
    assert!(d.cached_scene(page).is_none(), "таблица отброшена");
    assert_eq!(
        frame_guides(&mut d, page, frame),
        [gy(5.0)],
        "значение — из Loro"
    );
}

#[test]
fn guides_edit_on_unloaded_page_with_unknown_nodes_is_refused() {
    // Файл с повреждённым узлом на незагруженной странице.
    let mut src = Document::new();
    let other = src.add_page("Вторая").unwrap();
    let v = src
        .transact(other, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Vector)?])
        })
        .unwrap()[0];
    src.page(other)
        .unwrap()
        .force_vector_bytes_for_tests(v, b"not a network");
    src.page(other).unwrap().commit_for_tests();
    let file = encode(&src).unwrap();
    let no_cache = testing::map_sections(&file, |kind, _, data| {
        if kind == 5 {
            data[32] = 1;
        }
    });
    let mut d = decode(&no_cache).unwrap().document;
    assert!(!d.is_page_loaded(other));
    assert!(!d.is_read_only());
    let err = d.set_guides(other, vec![], &[(GuideOwner::Page(other), vec![gx(1.0)])]);
    assert!(matches!(err, Err(DocError::ReadOnly)), "{err:?}");
    assert!(d.page_guides(other).is_empty());
    assert!(!d.can_undo());
}

#[test]
fn undo_returns_to_the_page_of_the_guide_step() {
    let (mut doc, page, _, _) = doc_with_frame();
    let other = doc.add_page("Вторая").unwrap();
    doc.set_guides(other, vec![], &[(GuideOwner::Page(other), vec![gx(1.0)])])
        .unwrap();
    let r = doc.undo().unwrap().unwrap();
    assert_eq!(r.page, other);
    assert!(doc.page_guides(other).is_empty());
    let r = doc.redo().unwrap().unwrap();
    assert_eq!(r.page, other);
    assert_eq!(doc.page_guides(other), [gx(1.0)]);
    assert!(doc.page_guides(page).is_empty());
}

#[test]
fn guides_survive_the_journal() {
    // Журнал: направляющие страницы — в пачке корня и переживают восстановление.
    let (doc, page, frame, _) = doc_with_frame();
    let file = encode(&doc).unwrap();
    let mut opened = decode(&file).unwrap().document;
    opened.page(page).unwrap();
    opened.mark_journal();
    opened
        .set_guides(
            page,
            vec![],
            &[
                (GuideOwner::Page(page), vec![gx(9.0)]),
                (GuideOwner::Frame(frame), vec![gy(8.0)]),
            ],
        )
        .unwrap();
    let batch = opened.take_journal().unwrap().expect("пачка");
    let mut back = decode(&file).unwrap().document;
    let replayed = back.apply_journal(&[&batch]);
    assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
    assert_eq!(back.page_guides(page), [gx(9.0)]);
    assert_eq!(frame_guides(&mut back, page, frame), [gy(8.0)]);
}

#[test]
fn page_move_is_one_step_and_same_place_none() {
    let mut doc = Document::new();
    let a = doc.pages()[0].0;
    let b = doc.add_page("B").unwrap();
    let c = doc.add_page("C").unwrap();
    let order = |d: &Document| d.pages().iter().map(|p| p.0).collect::<Vec<_>>();
    doc.clear_history();
    doc.move_page(a, 0, a, vec![]).unwrap();
    assert!(!doc.can_undo(), "то же место — без шага");
    doc.move_page(a, 2, a, vec![]).unwrap();
    assert_eq!(order(&doc), [b, c, a]);
    doc.move_page(c, 0, a, vec![]).unwrap();
    assert_eq!(order(&doc), [c, b, a]);
    doc.undo().unwrap();
    assert_eq!(order(&doc), [b, c, a]);
    doc.undo().unwrap();
    assert_eq!(order(&doc), [a, b, c]);
    assert!(!doc.can_undo());
    doc.redo().unwrap();
    doc.redo().unwrap();
    assert_eq!(order(&doc), [c, b, a]);
    // За концом — на последнее место.
    doc.move_page(c, 99, a, vec![]).unwrap();
    assert_eq!(order(&doc), [b, a, c]);
}

#[test]
fn page_order_survives_save_and_journal() {
    let mut doc = Document::new();
    let a = doc.pages()[0].0;
    let b = doc.add_page("B").unwrap();
    let file = encode(&doc).unwrap();
    let mut opened = decode(&file).unwrap().document;
    opened.page(a).unwrap();
    opened.page(b).unwrap();
    opened.mark_journal();
    opened.move_page(b, 0, a, vec![]).unwrap();
    let batch = opened.take_journal().unwrap().expect("пачка");
    let mut back = decode(&file).unwrap().document;
    back.apply_journal(&[&batch]);
    assert_eq!(back.pages()[0].0, b);
    let resaved = decode(&encode(&opened).unwrap()).unwrap().document;
    assert_eq!(resaved.pages()[0].0, b);
}

#[test]
fn duplicate_page_undo_redo_keeps_copy_and_later_steps() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(
        page,
        vec![],
        &[
            (GuideOwner::Page(page), vec![gx(1.0)]),
            (GuideOwner::Frame(frame), vec![gy(2.0)]),
        ],
    )
    .unwrap();
    doc.clear_history();
    let copy = doc
        .duplicate_page(page, "Копия", page, vec![frame])
        .unwrap();
    assert_eq!(doc.pages().len(), 2);
    assert_eq!(doc.page_guides(copy), [gx(1.0)]);
    let made = doc
        .transact(copy, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Ellipse)?])
        })
        .unwrap()[0];

    doc.undo().unwrap();
    let r = doc.undo().unwrap().unwrap();
    assert_eq!(r.page, page, "отмена — на страницу, с которой дублировали");
    assert_eq!(r.selection, [frame]);
    assert_eq!(doc.pages().len(), 1);
    assert!(!doc.can_undo());

    let r = doc.redo().unwrap().unwrap();
    assert_eq!(doc.pages().len(), 2);
    let back = doc.pages()[1].0;
    assert_eq!(r.page, back, "повтор — на копию");
    assert_eq!(doc.pages()[1].1, "Копия");
    assert_eq!(doc.page_guides(back), [gx(1.0)]);
    // Следующий шаг повтора — про ту же копию, под её новым идентификатором.
    let r = doc.redo().unwrap().unwrap();
    assert_eq!(r.page, back);
    assert!(doc.page(back).unwrap().contains(made));
    // И снова назад: цепочка не рвётся.
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.pages().len(), 1);
}

#[test]
fn undone_copy_is_not_in_the_file_and_redone_is() {
    let (mut doc, page, _, _) = doc_with_frame();
    doc.duplicate_page(page, "Копия", page, vec![]).unwrap();
    doc.undo().unwrap();
    assert_eq!(
        decode(&encode(&doc).unwrap())
            .unwrap()
            .document
            .pages()
            .len(),
        1
    );
    doc.redo().unwrap();
    let mut d = decode(&encode(&doc).unwrap()).unwrap().document;
    let pages = d.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(d.page(pages[1].0).unwrap().walk().len(), 2, "узлы копии");
}

#[test]
fn new_step_forgets_the_parked_copy() {
    let (mut doc, page, _, _) = doc_with_frame();
    doc.duplicate_page(page, "Копия", page, vec![]).unwrap();
    doc.undo().unwrap();
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_node(None, None, NodeKind::Ellipse)?])
    })
    .unwrap();
    assert!(!doc.can_redo());
    doc.undo().unwrap();
    assert_eq!(doc.pages().len(), 1);
    assert!(!doc.can_undo());
}

#[test]
fn deleting_a_page_forgets_steps_that_rely_on_it() {
    let mut doc = Document::new();
    let a = doc.pages()[0].0;
    let b = doc.add_page("B").unwrap();
    doc.move_page(b, 0, a, vec![]).unwrap();
    doc.delete_page(b).unwrap();
    assert!(!doc.can_undo(), "шаг перестановки удалённой страницы забыт");
    let copy = doc.duplicate_page(a, "Копия", a, vec![]).unwrap();
    doc.delete_page(a).unwrap();
    assert!(!doc.can_undo(), "дубль удалённой страницы забыт");
    assert_eq!(doc.pages()[0].0, copy);
}

#[test]
fn step_number_survives_undo_redo_and_changes_with_new_step() {
    let (mut doc, page, _, _) = doc_with_frame();
    assert_eq!(doc.last_step(), None);
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    let id = doc.last_step().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.last_step(), None);
    doc.redo().unwrap();
    assert_eq!(doc.last_step(), Some(id));
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    assert_ne!(doc.last_step(), Some(id));
}

#[test]
fn duplicate_page_undo_redo_survive_the_journal() {
    let (doc, page, _, _) = doc_with_frame();
    let file = encode(&doc).unwrap();
    let mut open = decode(&file).unwrap().document;
    open.page(page).unwrap();
    open.mark_journal();
    let copy = open.duplicate_page(page, "Копия", page, vec![]).unwrap();
    let dup = open.take_journal().unwrap().expect("пачка дубля");
    open.undo().unwrap();
    let undo = open.take_journal().unwrap().expect("пачка отмены");
    open.redo().unwrap();
    let redo = open.take_journal().unwrap().expect("пачка повтора");

    let mut a = decode(&file).unwrap().document;
    a.apply_journal(&[&dup, &undo]);
    assert_eq!(a.pages().len(), 1, "отменённой копии нет");

    let mut b = decode(&file).unwrap().document;
    let replayed = b.apply_journal(&[&dup, &undo, &redo]);
    assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
    let pages = b.pages();
    assert_eq!(pages.len(), 2);
    assert_ne!(
        pages[1].0, copy,
        "повтор — новый идентификатор в дереве страниц"
    );
    assert_eq!(pages[1].1, "Копия");
    assert_eq!(b.page(pages[1].0).unwrap().walk().len(), 2, "узлы копии");
}
