use ferrum_doc::*;

#[test]
fn create_and_read_node_with_defaults() {
    let page = PageDoc::new();
    let id = page.create_node(None, None, NodeKind::Rect).unwrap();
    let p = page.props(id).unwrap();
    assert_eq!(p, Props::defaults(NodeKind::Rect));
    assert_eq!(page.kind(id).unwrap(), NodeKind::Rect);
}

#[test]
fn set_props_changes_only_given_fields() {
    let page = PageDoc::new();
    let id = page.create_node(None, None, NodeKind::Rect).unwrap();
    page.set_props(
        id,
        &PropsPatch {
            x: Some(12.5),
            name: Some("Фон".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let p = page.props(id).unwrap();
    assert_eq!(p.x, 12.5);
    assert_eq!(p.name, "Фон");
    assert_eq!(p.y, 0.0);
    assert_eq!(p.w, 100.0);
}

#[test]
fn fills_and_strokes_roundtrip() {
    let page = PageDoc::new();
    let id = page.create_node(None, None, NodeKind::Rect).unwrap();
    let fills = vec![
        Fill::solid(Color::parse("#ff000080").unwrap()),
        Fill {
            scale_mode: ScaleMode::Tile,
            opacity: 0.5,
            ..Fill::image("abc123")
        },
    ];
    let strokes = vec![Stroke {
        color: Color::BLACK,
        width: 2.0,
        align: StrokeAlign::Inside,
    }];
    page.set_props(
        id,
        &PropsPatch {
            fills: Some(fills.clone()),
            strokes: Some(strokes.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let p = page.props(id).unwrap();
    assert_eq!(p.fills, fills);
    assert_eq!(p.strokes, strokes);
    assert_eq!(page.image_refs().unwrap(), vec!["abc123".to_string()]);
}

#[test]
fn children_keep_order_and_index_insert() {
    let page = PageDoc::new();
    let frame = page.create_node(None, None, NodeKind::Frame).unwrap();
    let a = page.create_node(Some(frame), None, NodeKind::Rect).unwrap();
    let b = page
        .create_node(Some(frame), None, NodeKind::Ellipse)
        .unwrap();
    let c = page
        .create_node(Some(frame), Some(0), NodeKind::Line)
        .unwrap();
    assert_eq!(page.children(Some(frame)), vec![c, a, b]);
    assert_eq!(page.parent(a), Some(frame));
    assert_eq!(page.parent(frame), None);
}

#[test]
fn move_node_reorders_and_reparents() {
    let page = PageDoc::new();
    let f1 = page.create_node(None, None, NodeKind::Frame).unwrap();
    let f2 = page.create_node(None, None, NodeKind::Frame).unwrap();
    let a = page.create_node(Some(f1), None, NodeKind::Rect).unwrap();
    let b = page.create_node(Some(f1), None, NodeKind::Rect).unwrap();
    page.move_node(b, Some(f1), 0).unwrap();
    assert_eq!(page.children(Some(f1)), vec![b, a]);
    page.move_node(a, Some(f2), 0).unwrap();
    assert_eq!(page.children(Some(f1)), vec![b]);
    assert_eq!(page.children(Some(f2)), vec![a]);
}

#[test]
fn move_into_own_descendant_is_rejected() {
    let page = PageDoc::new();
    let outer = page.create_node(None, None, NodeKind::Frame).unwrap();
    let inner = page
        .create_node(Some(outer), None, NodeKind::Frame)
        .unwrap();
    assert!(matches!(
        page.move_node(outer, Some(inner), 0),
        Err(DocError::CyclicMove)
    ));
    assert!(matches!(
        page.move_node(outer, Some(outer), 0),
        Err(DocError::CyclicMove)
    ));
}

#[test]
fn delete_removes_subtree() {
    let page = PageDoc::new();
    let frame = page.create_node(None, None, NodeKind::Frame).unwrap();
    let child = page.create_node(Some(frame), None, NodeKind::Rect).unwrap();
    page.delete_nodes(&[frame, frame]).unwrap();
    assert!(!page.contains(frame));
    assert!(!page.contains(child));
    assert!(page.children(None).is_empty());
    assert!(matches!(page.props(child), Err(DocError::NodeNotFound(_))));
}

#[test]
fn walk_is_depth_first_bottom_to_top() {
    let page = PageDoc::new();
    let f = page.create_node(None, None, NodeKind::Frame).unwrap();
    let a = page.create_node(Some(f), None, NodeKind::Rect).unwrap();
    let b = page.create_node(None, None, NodeKind::Rect).unwrap();
    assert_eq!(page.walk(), vec![f, a, b]);
}

#[test]
fn snapshot_roundtrip() {
    let page = PageDoc::new();
    let f = page.create_node(None, None, NodeKind::Frame).unwrap();
    page.set_props(
        f,
        &PropsPatch {
            w: Some(393.0),
            h: Some(852.0),
            ..Default::default()
        },
    )
    .unwrap();
    let copy = PageDoc::from_snapshot(&page.export_snapshot().unwrap()).unwrap();
    assert_eq!(copy.props(f).unwrap(), page.props(f).unwrap());
}

#[test]
fn node_id_string_roundtrip() {
    let page = PageDoc::new();
    let id = page.create_node(None, None, NodeKind::Rect).unwrap();
    let parsed: NodeId = id.to_string().parse().unwrap();
    assert_eq!(parsed, id);
    assert!("мусор".parse::<NodeId>().is_err());
}
