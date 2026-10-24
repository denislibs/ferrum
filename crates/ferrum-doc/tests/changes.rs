use ferrum_doc::*;

fn sorted(mut ids: Vec<NodeId>) -> Vec<String> {
    let mut s: Vec<String> = ids.drain(..).map(|id| id.to_string()).collect();
    s.sort();
    s.dedup();
    s
}

fn x(v: f64) -> PropsPatch {
    PropsPatch {
        x: Some(v),
        ..Default::default()
    }
}

/// Фрейм с двумя детьми и прямоугольник в корне; изменения создания уже забраны.
fn setup() -> (Document, PageId, [NodeId; 4]) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut ids = Vec::new();
    doc.transact(page, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        ids.push(f);
        ids.push(p.create_node(Some(f), None, NodeKind::Rect)?);
        ids.push(p.create_node(Some(f), None, NodeKind::Ellipse)?);
        ids.push(p.create_node(None, None, NodeKind::Rect)?);
        Ok(vec![])
    })
    .unwrap();
    let created = doc.take_changes(page);
    assert_eq!(sorted(created.created), sorted(ids.clone()));
    assert!(
        doc.take_changes(page).is_empty(),
        "take_changes забирает изменения"
    );
    (doc, page, [ids[0], ids[1], ids[2], ids[3]])
}

#[test]
fn each_operation_kind_is_reported() {
    let (mut doc, page, [f, a, b, r]) = setup();

    doc.transact(page, vec![], |p| p.set_props(a, &x(5.0)).map(|_| vec![]))
        .unwrap();
    let c = doc.take_changes(page);
    assert_eq!(c.props, vec![a]);
    assert!(c.created.is_empty() && c.deleted.is_empty() && c.moved.is_empty());

    doc.transact(page, vec![], |p| p.move_node(r, Some(f), 0).map(|_| vec![]))
        .unwrap();
    let c = doc.take_changes(page);
    assert_eq!(c.moved, vec![r]);
    assert_eq!(c.len(), 1);

    doc.transact(page, vec![], |p| p.delete_nodes(&[f]).map(|_| vec![]))
        .unwrap();
    let c = doc.take_changes(page);
    assert_eq!(sorted(c.deleted), sorted(vec![f, a, b, r]));
    assert!(c.created.is_empty() && c.moved.is_empty() && c.props.is_empty());
}

#[test]
fn undo_and_redo_report_their_changes() {
    let (mut doc, page, [f, a, b, r]) = setup();
    doc.transact(page, vec![], |p| p.delete_nodes(&[f]).map(|_| vec![]))
        .unwrap();
    doc.take_changes(page);

    doc.undo().unwrap();
    let c = doc.take_changes(page);
    assert_eq!(sorted(c.created), sorted(vec![f, a, b]));
    assert!(c.deleted.is_empty());

    doc.redo().unwrap();
    let c = doc.take_changes(page);
    assert_eq!(sorted(c.deleted), sorted(vec![f, a, b]));

    doc.undo().unwrap();
    doc.take_changes(page);
    doc.transact(page, vec![], |p| {
        p.set_props(r, &x(9.0))?;
        p.move_node(b, None, 0)?;
        Ok(vec![])
    })
    .unwrap();
    doc.take_changes(page);
    doc.undo().unwrap();
    let c = doc.take_changes(page);
    assert_eq!(c.props, vec![r]);
    assert_eq!(c.moved, vec![b]);
    doc.redo().unwrap();
    let c = doc.take_changes(page);
    assert_eq!(c.props, vec![r]);
    assert_eq!(c.moved, vec![b]);
    assert_eq!(doc.page(page).unwrap().props(r).unwrap().x, 9.0);
}

#[test]
fn changes_accumulate_until_taken_and_stay_per_page() {
    let (mut doc, p1, [_, a, _, r]) = setup();
    let p2 = doc.add_page("Вторая").unwrap();
    doc.transact(p1, vec![], |p| p.set_props(a, &x(1.0)).map(|_| vec![]))
        .unwrap();
    let other = doc
        .transact(p2, vec![], |p| {
            Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    doc.transact(p1, vec![], |p| p.set_props(r, &x(2.0)).map(|_| vec![]))
        .unwrap();
    assert_eq!(doc.take_changes(p1).props, vec![a, r]);
    assert_eq!(doc.take_changes(p2).created, vec![other]);
    assert!(doc.take_changes(p1).is_empty());
}

#[test]
fn failed_transaction_reports_touched_nodes_and_keeps_page() {
    let (mut doc, page, [_, a, _, _]) = setup();
    let err = doc.transact(page, vec![], |p| {
        p.set_props(a, &x(42.0))?;
        Err(DocError::CyclicMove)
    });
    assert!(err.is_err());
    assert_eq!(doc.page(page).unwrap().props(a).unwrap().x, 0.0);
    // Откат вернул узел; сцене достаточно сверить его со страницей.
    assert_eq!(doc.take_changes(page).props, vec![a]);
}

#[test]
fn placement_matches_children_order() {
    let (mut doc, page, [f, a, b, r]) = setup();
    doc.transact(page, vec![], |p| p.move_node(r, Some(f), 1).map(|_| vec![]))
        .unwrap();
    let p = doc.page(page).unwrap();
    assert_eq!(p.children(Some(f)), vec![a, r, b]);
    for (i, id) in [a, r, b].into_iter().enumerate() {
        assert_eq!(p.placement(id), Some((Some(f), Some(i))));
    }
    assert_eq!(p.placement(f), Some((None, Some(0))));
    doc.transact(page, vec![], |p| p.delete_nodes(&[a]).map(|_| vec![]))
        .unwrap();
    assert_eq!(doc.page(page).unwrap().placement(a), None);
}
