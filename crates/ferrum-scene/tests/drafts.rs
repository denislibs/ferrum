//! Черновики жеста: временные узлы, которых нет в документе.

use ferrum_doc::{Document, NodeId, NodeKind, PageId, Props, PropsPatch};
use ferrum_scene::{testing::snapshot, Draft, Scene};
use kurbo::{Point, Rect};

/// Идентификатор черновика: Loro не выдаёт узлам пир `u64::MAX`, поэтому такой
/// `NodeId` не совпадёт ни с одним узлом документа.
fn draft_id(n: u32) -> NodeId {
    format!("{n}@{}", u64::MAX).parse().unwrap()
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Props {
    Props {
        x,
        y,
        w,
        h,
        ..Props::defaults(NodeKind::Rect)
    }
}

/// Фрейм (0, 0, 400×300) с прямоугольником внутри и прямоугольник в корне.
fn sample() -> (Document, PageId, [NodeId; 3]) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |p| {
            let f = p.create_node(None, None, NodeKind::Frame)?;
            p.set_props(
                f,
                &PropsPatch {
                    w: Some(400.0),
                    h: Some(300.0),
                    ..Default::default()
                },
            )?;
            let a = p.create_node(Some(f), None, NodeKind::Rect)?;
            let b = p.create_node(None, None, NodeKind::Rect)?;
            p.set_props(
                b,
                &PropsPatch {
                    x: Some(600.0),
                    ..Default::default()
                },
            )?;
            Ok(vec![f, a, b])
        })
        .unwrap();
    doc.take_changes(page);
    (doc, page, [ids[0], ids[1], ids[2]])
}

fn fresh(doc: &mut Document, page: PageId) -> Scene {
    Scene::from_page(doc.page(page).unwrap())
}

#[test]
fn drafts_are_top_children_and_go_away_with_preview() {
    let (mut doc, page, [f, a, _]) = sample();
    let mut scene = fresh(&mut doc, page);
    let before = scene.change_counter();
    let d = draft_id(0);
    scene.set_drafts(&[Draft {
        id: d,
        parent: Some(f),
        props: rect(200.0, 100.0, 50.0, 50.0),
    }]);
    assert!(scene.change_counter() > before);
    assert!(scene.has_preview());
    assert!(scene.is_draft(d));
    assert!(!scene.is_draft(a));
    let fs = scene.slot(f).unwrap();
    let kids = &scene.at(fs).children;
    assert_eq!(kids.len(), 2);
    assert_eq!(
        scene.at(*kids.last().unwrap()).id,
        d,
        "черновик — верхний ребёнок"
    );
    assert_eq!(
        scene.node(d).unwrap().world_bounds,
        Some(Rect::new(200.0, 100.0, 250.0, 150.0))
    );
    assert_eq!(scene.hit_test(Point::new(220.0, 120.0), 0.0), Some(d));

    scene.clear_preview();
    assert!(!scene.has_preview());
    assert!(scene.node(d).is_none());
    assert!(!scene.is_draft(d));
    assert_eq!(snapshot(&scene), snapshot(&fresh(&mut doc, page)));
}

#[test]
fn draft_subtrees_move_by_preview_and_bad_drafts_are_skipped() {
    let (mut doc, page, [f, a, b]) = sample();
    let mut scene = fresh(&mut doc, page);
    let (g, c, orphan, dup) = (draft_id(0), draft_id(1), draft_id(2), draft_id(3));
    scene.set_drafts(&[
        Draft {
            id: g,
            parent: None,
            props: Props::defaults(NodeKind::Group),
        },
        Draft {
            id: c,
            parent: Some(g),
            props: rect(10.0, 10.0, 20.0, 20.0),
        },
        // Родителя нет ни в документе, ни среди черновиков.
        Draft {
            id: orphan,
            parent: Some(draft_id(99)),
            props: rect(0.0, 0.0, 5.0, 5.0),
        },
        // Идентификатор уже занят узлом документа.
        Draft {
            id: a,
            parent: None,
            props: rect(0.0, 0.0, 5.0, 5.0),
        },
        Draft {
            id: dup,
            parent: Some(c),
            props: rect(1.0, 1.0, 2.0, 2.0),
        },
    ]);
    assert!(scene.node(orphan).is_none());
    assert_eq!(
        scene.node(a).unwrap().parent,
        scene.slot(f),
        "узел документа не тронут"
    );
    assert_eq!(scene.node(dup).unwrap().parent, scene.slot(c));
    assert_eq!(
        scene.node(g).unwrap().subtree_bounds,
        Some(Rect::new(10.0, 10.0, 30.0, 30.0))
    );
    scene.set_preview(&[(
        g,
        PropsPatch {
            x: Some(100.0),
            ..Default::default()
        },
    )]);
    assert_eq!(
        scene.node(c).unwrap().world_bounds,
        Some(Rect::new(110.0, 10.0, 130.0, 30.0))
    );
    // Новый список заменяет старый целиком.
    scene.set_drafts(&[Draft {
        id: orphan,
        parent: Some(b),
        props: rect(0.0, 0.0, 5.0, 5.0),
    }]);
    assert!(scene.node(g).is_none() && scene.node(c).is_none());
    assert!(scene.is_draft(orphan));
    scene.clear_preview();
    assert_eq!(snapshot(&scene), snapshot(&fresh(&mut doc, page)));
}

#[test]
fn apply_keeps_drafts_on_top_with_their_preview() {
    let (mut doc, page, [f, a, _]) = sample();
    let mut scene = fresh(&mut doc, page);
    let d = draft_id(0);
    scene.set_drafts(&[Draft {
        id: d,
        parent: Some(f),
        props: rect(0.0, 0.0, 10.0, 10.0),
    }]);
    scene.set_preview(&[(
        d,
        PropsPatch {
            x: Some(50.0),
            ..Default::default()
        },
    )]);
    // Документ получает нового ребёнка фрейма поверх всех — черновик остаётся выше.
    let n = doc
        .transact(page, vec![], |p| {
            Ok(vec![p.create_node(Some(f), None, NodeKind::Ellipse)?])
        })
        .unwrap()[0];
    let changes = doc.take_changes(page);
    scene.apply(&changes, doc.page(page).unwrap());
    let kids: Vec<NodeId> = scene
        .at(scene.slot(f).unwrap())
        .children
        .iter()
        .map(|s| scene.at(*s).id)
        .collect();
    assert_eq!(kids, vec![a, n, d]);
    assert_eq!(
        scene.node(d).unwrap().props.x,
        50.0,
        "превью черновика сохранилось"
    );
    scene.clear_preview();
    assert_eq!(snapshot(&scene), snapshot(&fresh(&mut doc, page)));
}
