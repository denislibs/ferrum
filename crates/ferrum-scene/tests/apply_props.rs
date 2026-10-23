//! Случайные транзакции, отмены и повторы: сцена, обновлённая по `Changes`,
//! после каждого шага совпадает со сценой, построенной заново по странице.

use std::cell::Cell;

use ferrum_doc::*;
use ferrum_scene::{testing::snapshot, Scene};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

#[derive(Debug, Clone)]
enum Edit {
    Create {
        parent: usize,
        kind: u8,
        index: usize,
    },
    Move {
        node: usize,
        parent: usize,
        index: usize,
    },
    MoveMany {
        nodes: (usize, usize),
        parent: usize,
        index: usize,
    },
    Delete {
        node: usize,
    },
    Geometry {
        node: usize,
        x: i16,
        w: u8,
        rotation: u8,
    },
    Toggle {
        node: usize,
        visible: bool,
        clip: bool,
    },
}

#[derive(Debug, Clone)]
enum Step {
    Tx(Vec<Edit>),
    Undo,
    Redo,
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        3 => (any::<usize>(), 0u8..6, 0usize..6)
            .prop_map(|(parent, kind, index)| Edit::Create { parent, kind, index }),
        2 => (any::<usize>(), any::<usize>(), 0usize..6)
            .prop_map(|(node, parent, index)| Edit::Move { node, parent, index }),
        1 => (any::<(usize, usize)>(), any::<usize>(), 0usize..6)
            .prop_map(|(nodes, parent, index)| Edit::MoveMany { nodes, parent, index }),
        1 => any::<usize>().prop_map(|node| Edit::Delete { node }),
        3 => (any::<usize>(), any::<i16>(), any::<u8>(), any::<u8>())
            .prop_map(|(node, x, w, rotation)| Edit::Geometry { node, x, w, rotation }),
        1 => (any::<usize>(), any::<bool>(), any::<bool>())
            .prop_map(|(node, visible, clip)| Edit::Toggle { node, visible, clip }),
    ]
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => prop::collection::vec(edit(), 1..4).prop_map(Step::Tx),
        1 => Just(Step::Undo),
        1 => Just(Step::Redo),
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

fn pick(alive: &[NodeId], i: usize) -> Option<NodeId> {
    (!alive.is_empty() && i % (alive.len() + 1) != alive.len())
        .then(|| alive[i % (alive.len() + 1)])
}

fn run(p: &Tx, e: &Edit) -> DocResult<()> {
    let alive = p.walk();
    match *e {
        Edit::Create {
            parent,
            kind,
            index,
        } => {
            let id = p.create_node(pick(&alive, parent), Some(index), KINDS[kind as usize])?;
            p.set_props(
                id,
                &PropsPatch {
                    x: Some(index as f64 * 13.0),
                    y: Some(kind as f64 * 7.0),
                    ..Default::default()
                },
            )
        }
        Edit::Move {
            node,
            parent,
            index,
        } => match pick(&alive, node) {
            Some(n) => p.move_node(n, pick(&alive, parent), index),
            None => Ok(()),
        },
        Edit::MoveMany {
            nodes: (a, b),
            parent,
            index,
        } => match pick(&alive, a) {
            Some(first) => {
                let ids: Vec<NodeId> = [Some(first), pick(&alive, b)]
                    .into_iter()
                    .flatten()
                    .collect();
                p.move_nodes(&ids, pick(&alive, parent), index)
            }
            None => Ok(()),
        },
        Edit::Delete { node } => match pick(&alive, node) {
            Some(n) => p.delete_nodes(&[n]),
            None => Ok(()),
        },
        Edit::Geometry {
            node,
            x,
            w,
            rotation,
        } => match pick(&alive, node) {
            Some(n) => p.set_props(
                n,
                &PropsPatch {
                    x: Some(x as f64),
                    w: Some(w as f64),
                    rotation: Some(rotation as f64 * 15.0),
                    ..Default::default()
                },
            ),
            None => Ok(()),
        },
        Edit::Toggle {
            node,
            visible,
            clip,
        } => match pick(&alive, node) {
            Some(n) => p.set_props(
                n,
                &PropsPatch {
                    visible: Some(visible),
                    clip_content: Some(clip),
                    ..Default::default()
                },
            ),
            None => Ok(()),
        },
    }
}

#[test]
fn scene_after_changes_matches_rebuild() {
    let structural = Cell::new(0usize);
    let props_only = Cell::new(0usize);
    let history = Cell::new(0usize);
    let failed_tx = Cell::new(0usize);
    let mut runner = TestRunner::new(Config {
        cases: 96,
        ..Config::default()
    });
    runner
        .run(&prop::collection::vec(step(), 1..40), |steps| {
            let mut doc = Document::new();
            let page = doc.pages()[0].0;
            let mut scene = Scene::new();
            for s in &steps {
                match s {
                    Step::Tx(edits) => {
                        let r = doc.transact(page, vec![], |p| {
                            edits.iter().try_for_each(|e| run(p, e)).map(|_| vec![])
                        });
                        if r.is_err() {
                            failed_tx.set(failed_tx.get() + 1);
                        }
                    }
                    Step::Undo => {
                        if doc.undo().unwrap().is_some() {
                            history.set(history.get() + 1);
                        }
                    }
                    Step::Redo => {
                        if doc.redo().unwrap().is_some() {
                            history.set(history.get() + 1);
                        }
                    }
                }
                let changes = doc.take_changes(page);
                if changes.created.len() + changes.deleted.len() + changes.moved.len() > 0 {
                    structural.set(structural.get() + 1);
                } else if !changes.is_empty() {
                    props_only.set(props_only.get() + 1);
                }
                let p = doc.page(page).unwrap();
                scene.apply(&changes, p);
                let fresh = Scene::from_page(p);
                prop_assert_eq!(snapshot(&scene), snapshot(&fresh), "после {:?}", s);
            }
            Ok(())
        })
        .unwrap();
    // Тест не должен проходить вхолостую: сцена обновлялась всеми путями.
    assert!(
        structural.get() > 300,
        "структурных шагов: {}",
        structural.get()
    );
    assert!(
        props_only.get() > 100,
        "шагов только со свойствами: {}",
        props_only.get()
    );
    assert!(history.get() > 100, "отмен и повторов: {}", history.get());
    assert!(
        failed_tx.get() > 0,
        "откатов транзакций: {}",
        failed_tx.get()
    );
}
