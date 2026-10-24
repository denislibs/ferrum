use ferrum_doc::*;

fn first_page(doc: &Document) -> PageId {
    doc.pages()[0].0
}

#[test]
fn new_document_has_one_page() {
    let doc = Document::new();
    let pages = doc.pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].1, "Страница 1");
    assert_eq!(doc.schema_version(), SCHEMA_VERSION);
}

#[test]
fn pages_can_be_added_renamed_deleted() {
    let mut doc = Document::new();
    let p2 = doc.add_page("Черновики").unwrap();
    doc.rename_page(p2, "Идеи").unwrap();
    assert_eq!(doc.pages()[1], (p2, "Идеи".to_string()));
    doc.delete_page(p2).unwrap();
    assert_eq!(doc.pages().len(), 1);
    assert!(matches!(doc.page(p2), Err(DocError::PageNotFound(_))));
}

#[test]
fn transaction_is_one_undo_step_and_restores_selection() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    let mut created = None;
    let sel = doc
        .transact(page, vec![], |p| {
            let id = p.create_node(None, None, NodeKind::Rect)?;
            p.set_props(
                id,
                &PropsPatch {
                    x: Some(10.0),
                    ..Default::default()
                },
            )?;
            created = Some(id);
            Ok(vec![id])
        })
        .unwrap();
    let id = created.unwrap();
    assert_eq!(sel, vec![id]);

    let restore = doc.undo().unwrap().unwrap();
    assert_eq!(
        restore,
        Restore {
            page,
            selection: vec![]
        }
    );
    assert!(doc.page(page).unwrap().children(None).is_empty());

    let restore = doc.redo().unwrap().unwrap();
    assert_eq!(restore.selection, vec![id]);
    assert_eq!(doc.page(page).unwrap().props(id).unwrap().x, 10.0);
}

#[test]
fn undo_follows_user_order_across_pages() {
    let mut doc = Document::new();
    let p1 = first_page(&doc);
    let p2 = doc.add_page("Вторая").unwrap();
    doc.transact(p1, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    doc.transact(p2, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Ellipse)?])
    })
    .unwrap();
    doc.transact(p1, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Line)?])
    })
    .unwrap();

    assert_eq!(doc.undo().unwrap().unwrap().page, p1);
    assert_eq!(doc.page(p1).unwrap().children(None).len(), 1);
    assert_eq!(doc.undo().unwrap().unwrap().page, p2);
    assert!(doc.page(p2).unwrap().children(None).is_empty());
    assert_eq!(doc.undo().unwrap().unwrap().page, p1);
    assert!(doc.page(p1).unwrap().children(None).is_empty());
    assert!(doc.undo().unwrap().is_none());
}

#[test]
fn new_change_clears_redo() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    doc.transact(page, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    doc.undo().unwrap();
    assert!(doc.can_redo());
    doc.transact(page, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Ellipse)?])
    })
    .unwrap();
    assert!(!doc.can_redo());
}

#[test]
fn empty_transaction_adds_no_history() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    doc.transact(page, vec![], |_| Ok(vec![])).unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn failed_transaction_rolls_back() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    let err = doc.transact(page, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        p.move_node(f, Some(f), 0)?;
        Ok(vec![f])
    });
    assert!(matches!(err, Err(DocError::CyclicMove)));
    assert!(doc.page(page).unwrap().children(None).is_empty());
    assert!(!doc.can_undo());
}

#[test]
fn deleting_page_forgets_its_history() {
    let mut doc = Document::new();
    let p2 = doc.add_page("Вторая").unwrap();
    doc.transact(p2, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    doc.delete_page(p2).unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn images_are_deduplicated_by_hash() {
    let mut doc = Document::new();
    let a = doc.images.add(vec![1, 2, 3]);
    let b = doc.images.add(vec![1, 2, 3]);
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
    assert_eq!(doc.images.len(), 1);
    assert_eq!(doc.images.get(&a), Some(&[1u8, 2, 3][..]));
}

#[test]
fn node_ids_survive_undo_of_delete_with_subtree_and_fills() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    let (mut frame, mut child) = (None, None);
    doc.transact(page, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        let c = p.create_node(Some(f), None, NodeKind::Rect)?;
        p.set_props(
            c,
            &PropsPatch {
                fills: Some(vec![Fill::solid(Color::BLACK)]),
                ..Default::default()
            },
        )?;
        frame = Some(f);
        child = Some(c);
        Ok(vec![f])
    })
    .unwrap();
    let (frame, child) = (frame.unwrap(), child.unwrap());
    doc.transact(page, vec![frame], |p| {
        p.delete_nodes(&[frame])?;
        Ok(vec![])
    })
    .unwrap();

    let restore = doc.undo().unwrap().unwrap();
    assert_eq!(restore.selection, vec![frame]);
    let p = doc.page(page).unwrap();
    assert!(p.contains(frame) && p.contains(child));
    assert_eq!(p.children(Some(frame)), vec![child]);
    assert_eq!(p.parent(child), Some(frame));
    assert_eq!(
        p.props(child).unwrap().fills,
        vec![Fill::solid(Color::BLACK)]
    );
    assert_eq!(p.walk(), vec![frame, child]);
}

#[test]
fn node_ids_survive_reload_after_undo() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    let id = doc
        .transact(page, vec![], |p| {
            Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    doc.undo().unwrap();
    doc.redo().unwrap();
    let snapshot = doc.page(page).unwrap().export_snapshot().unwrap();
    let reloaded = PageDoc::from_snapshot(&snapshot).unwrap();
    assert!(reloaded.contains(id));
    assert_eq!(reloaded.walk(), vec![id]);
}
