//! Случайные последовательности операций не ломают дерево, а полная отмена
//! возвращает страницу в исходное (пустое) состояние.

use std::collections::HashSet;

use ferrum_doc::*;
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Create {
        parent: usize,
        kind: u8,
    },
    Move {
        node: usize,
        parent: usize,
        index: usize,
    },
    Delete {
        node: usize,
    },
    SetX {
        node: usize,
        x: i32,
    },
    Fill {
        node: usize,
        rgb: u8,
    },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<usize>(), 0u8..6).prop_map(|(parent, kind)| Op::Create { parent, kind }),
        (any::<usize>(), any::<usize>(), 0usize..5).prop_map(|(node, parent, index)| Op::Move {
            node,
            parent,
            index
        }),
        any::<usize>().prop_map(|node| Op::Delete { node }),
        (any::<usize>(), -1000i32..1000).prop_map(|(node, x)| Op::SetX { node, x }),
        (any::<usize>(), any::<u8>()).prop_map(|(node, rgb)| Op::Fill { node, rgb }),
    ]
}

const KINDS: [NodeKind; 6] = [
    NodeKind::Frame,
    NodeKind::Group,
    NodeKind::Rect,
    NodeKind::Ellipse,
    NodeKind::Line,
    NodeKind::Image,
];

/// `pick(i)` — живой узел или корень (None), чтобы операции попадали в существующие узлы.
fn pick(alive: &[NodeId], i: usize) -> Option<NodeId> {
    if alive.is_empty() || i % (alive.len() + 1) == alive.len() {
        None
    } else {
        Some(alive[i % (alive.len() + 1)])
    }
}

fn check_tree(p: &PageDoc) {
    let all = p.walk();
    let unique: HashSet<_> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "узел встречается в дереве дважды");
    for id in &all {
        assert!(p.contains(*id));
        let siblings = p.children(p.parent(*id));
        assert!(siblings.contains(id), "родитель не знает о ребёнке");
        p.props(*id).expect("свойства читаются");
    }
    // Зеркало свойств совпадает с тем, что честно читается из Loro.
    let fresh = PageDoc::from_snapshot(&p.export_snapshot().unwrap()).unwrap();
    assert_eq!(fresh.walk(), all, "порядок узлов после перезагрузки");
    for id in &all {
        assert_eq!(
            fresh.props(*id).unwrap(),
            p.props(*id).unwrap(),
            "зеркало разошлось с Loro"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn random_ops_keep_tree_valid_and_undo_all_empties_page(ops in prop::collection::vec(op(), 1..40)) {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        for op in ops {
            let alive = doc.page(page).unwrap().walk();
            let _ = doc.transact(page, vec![], |p| {
                match op {
                    Op::Create { parent, kind } => {
                        p.create_node(pick(&alive, parent), None, KINDS[kind as usize])?;
                    }
                    Op::Move { node, parent, index } => {
                        if let Some(n) = pick(&alive, node) {
                            p.move_node(n, pick(&alive, parent), index)?;
                        }
                    }
                    Op::Delete { node } => {
                        if let Some(n) = pick(&alive, node) {
                            p.delete_nodes(&[n])?;
                        }
                    }
                    Op::Fill { node, rgb } => {
                        if let Some(n) = pick(&alive, node) {
                            let fills = vec![Fill::solid(Color::rgba(rgb, rgb, 0, 255))];
                            p.set_props(n, &PropsPatch { fills: Some(fills), ..Default::default() })?;
                        }
                    }
                    Op::SetX { node, x } => {
                        if let Some(n) = pick(&alive, node) {
                            p.set_props(n, &PropsPatch { x: Some(x as f64), ..Default::default() })?;
                        }
                    }
                }
                Ok(vec![])
            });
            check_tree(doc.page(page).unwrap());
        }
        while doc.undo().unwrap().is_some() {
            check_tree(doc.page(page).unwrap());
        }
        prop_assert!(doc.page(page).unwrap().walk().is_empty());
        while doc.redo().unwrap().is_some() {
            check_tree(doc.page(page).unwrap());
        }
    }
}
