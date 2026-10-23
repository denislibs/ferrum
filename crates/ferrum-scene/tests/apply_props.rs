//! Случайные транзакции, отмены и повторы, превью жеста: сцена, обновлённая по
//! `Changes`, после каждого шага совпадает со сценой, построенной заново по странице.

use std::cell::RefCell;

use ferrum_doc::*;
use ferrum_scene::{testing::snapshot, Scene};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};

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
    Preview { nodes: Vec<usize>, x: i16 },
    ClearPreview,
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
        1 => (prop::collection::vec(any::<usize>(), 1..4), any::<i16>())
            .prop_map(|(nodes, x)| Step::Preview { nodes, x }),
        1 => Just(Step::ClearPreview),
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

/// Что сделала транзакция сверх самих правок. Учитывается только при успехе:
/// при откате все эти события отменяются вместе с правками.
#[derive(Default, Clone, Copy)]
struct TxHits {
    /// Перенос узла к другому родителю.
    reparent: usize,
    /// Пакетный перенос, который прошёл и двигает два разных узла.
    batch_pair: usize,
    /// Удаление узла, у которого были дети.
    delete_with_children: usize,
}

/// Счётчики трудных случаев за весь прогон: тест не должен проходить вхолостую.
#[derive(Default, Debug)]
struct Stats {
    structural: usize,
    props_only: usize,
    moved_steps: usize,
    reparent: usize,
    batch_pair: usize,
    delete_with_children: usize,
    history: usize,
    history_multi: usize,
    rotated_clipped_steps: usize,
    hidden_parent_steps: usize,
    preview_steps: usize,
    failed_tx: usize,
}

fn run(p: &Tx, e: &Edit, hits: &mut TxHits) -> DocResult<()> {
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
            Some(n) => {
                let target = pick(&alive, parent);
                let old = p.parent(n);
                p.move_node(n, target, index)?;
                if old != target {
                    hits.reparent += 1;
                }
                Ok(())
            }
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
                p.move_nodes(&ids, pick(&alive, parent), index)?;
                if ids.len() == 2 && ids[0] != ids[1] {
                    hits.batch_pair += 1;
                }
                Ok(())
            }
            None => Ok(()),
        },
        Edit::Delete { node } => match pick(&alive, node) {
            Some(n) => {
                let had_children = !p.children(Some(n)).is_empty();
                p.delete_nodes(&[n])?;
                if had_children {
                    hits.delete_with_children += 1;
                }
                Ok(())
            }
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

/// Есть ли на странице повёрнутый фрейм с обрезкой и детьми.
fn has_rotated_clipped_frame(p: &PageDoc) -> bool {
    p.walk().into_iter().any(|id| {
        p.props(id).is_ok_and(|pr| {
            pr.kind == NodeKind::Frame
                && pr.rotation != 0.0
                && pr.clip_content
                && !p.children(Some(id)).is_empty()
        })
    })
}

/// Есть ли скрытый узел, у которого есть видимый ребёнок.
fn has_hidden_parent_with_visible_child(p: &PageDoc) -> bool {
    p.walk().into_iter().any(|id| {
        p.props(id).is_ok_and(|pr| !pr.visible)
            && p.children(Some(id))
                .into_iter()
                .any(|c| p.props(c).is_ok_and(|pc| pc.visible))
    })
}

#[test]
fn scene_after_changes_matches_rebuild() {
    let stats = RefCell::new(Stats::default());
    let mut runner = TestRunner::new(Config {
        cases: 96,
        failure_persistence: None,
        ..Config::default()
    });
    runner
        .run(&prop::collection::vec(step(), 1..40), |steps| {
            let mut doc = Document::new();
            let page = doc.pages()[0].0;
            let mut scene = Scene::new();
            // Превью жеста так, как его задал последний `set_preview` в тесте.
            let mut preview: Vec<(NodeId, PropsPatch)> = Vec::new();
            // Зеркало истории: число правок в каждой записи стеков undo и redo.
            let mut undo_stack: Vec<usize> = Vec::new();
            let mut redo_stack: Vec<usize> = Vec::new();
            for s in &steps {
                // Число правок транзакции, если она записана в историю (проверим ниже).
                let mut recorded: Option<usize> = None;
                match s {
                    Step::Tx(edits) => {
                        let mut hits = TxHits::default();
                        let r = doc.transact(page, vec![], |p| {
                            edits
                                .iter()
                                .try_for_each(|e| run(p, e, &mut hits))
                                .map(|_| vec![])
                        });
                        match r {
                            Ok(_) => {
                                recorded = Some(edits.len());
                                let mut st = stats.borrow_mut();
                                st.reparent += hits.reparent;
                                st.batch_pair += hits.batch_pair;
                                st.delete_with_children += hits.delete_with_children;
                            }
                            // Отказ по циклу — ожидаемый откат транзакции; любая другая
                            // ошибка означает сбой в самом документе.
                            Err(DocError::CyclicMove) => stats.borrow_mut().failed_tx += 1,
                            Err(e) => {
                                return Err(TestCaseError::fail(format!(
                                    "неожиданная ошибка транзакции: {e}"
                                )))
                            }
                        }
                    }
                    Step::Undo => {
                        if doc.undo().unwrap().is_some() {
                            let n = undo_stack.pop().expect("зеркало истории совпадает");
                            redo_stack.push(n);
                            let mut st = stats.borrow_mut();
                            st.history += 1;
                            if n >= 2 {
                                st.history_multi += 1;
                            }
                        }
                    }
                    Step::Redo => {
                        if doc.redo().unwrap().is_some() {
                            let n = redo_stack.pop().expect("зеркало истории совпадает");
                            undo_stack.push(n);
                            let mut st = stats.borrow_mut();
                            st.history += 1;
                            if n >= 2 {
                                st.history_multi += 1;
                            }
                        }
                    }
                    Step::Preview { nodes, x } => {
                        let alive = doc.page(page).unwrap().walk();
                        preview.clear();
                        for &i in nodes {
                            if let Some(id) = pick(&alive, i) {
                                if !preview.iter().any(|(q, _)| *q == id) {
                                    let patch = PropsPatch {
                                        x: Some(f64::from(*x)),
                                        ..Default::default()
                                    };
                                    preview.push((id, patch));
                                }
                            }
                        }
                        scene.set_preview(&preview);
                    }
                    Step::ClearPreview => {
                        preview.clear();
                        scene.clear_preview();
                    }
                }
                let changes = doc.take_changes(page);
                if let Some(n) = recorded {
                    // Пустая транзакция в историю не попадает; непустая очищает redo.
                    if !changes.is_empty() {
                        undo_stack.push(n);
                        redo_stack.clear();
                    }
                }
                {
                    let mut st = stats.borrow_mut();
                    if !changes.moved.is_empty() {
                        st.moved_steps += 1;
                    }
                    if changes.created.len() + changes.deleted.len() + changes.moved.len() > 0 {
                        st.structural += 1;
                    } else if !changes.is_empty() {
                        st.props_only += 1;
                    }
                }
                // Сцена забывает превью удалённого поддерева, даже если отмена вернёт узел.
                preview.retain(|(id, _)| !changes.deleted.contains(id));
                let p = doc.page(page).unwrap();
                scene.apply(&changes, p);
                {
                    let mut st = stats.borrow_mut();
                    if scene.has_preview() {
                        st.preview_steps += 1;
                    }
                    if has_rotated_clipped_frame(p) {
                        st.rotated_clipped_steps += 1;
                    }
                    if has_hidden_parent_with_visible_child(p) {
                        st.hidden_parent_steps += 1;
                    }
                }
                let mut fresh = Scene::from_page(p);
                fresh.set_preview(&preview);
                prop_assert_eq!(snapshot(&scene), snapshot(&fresh), "после {:?}", s);
            }
            Ok(())
        })
        .unwrap();
    let st = stats.into_inner();
    // Тест не должен проходить вхолостую: сцена обновлялась всеми путями.
    assert!(st.structural > 300, "структурных шагов: {}", st.structural);
    assert!(
        st.props_only > 100,
        "шагов только со свойствами: {}",
        st.props_only
    );
    assert!(st.history > 100, "отмен и повторов: {}", st.history);
    assert!(st.failed_tx > 0, "откатов транзакций: {}", st.failed_tx);
    assert!(st.moved_steps > 50, "шагов с переносом: {}", st.moved_steps);
    assert!(
        st.reparent > 15,
        "переносов к другому родителю: {}",
        st.reparent
    );
    assert!(
        st.batch_pair > 5,
        "пакетных переносов двух узлов: {}",
        st.batch_pair
    );
    assert!(
        st.delete_with_children > 8,
        "удалений узла с детьми: {}",
        st.delete_with_children
    );
    assert!(
        st.history_multi > 50,
        "отмен и повторов транзакций из двух и более правок: {}",
        st.history_multi
    );
    assert!(
        st.rotated_clipped_steps > 30,
        "шагов с повёрнутым фреймом с обрезкой и детьми: {}",
        st.rotated_clipped_steps
    );
    assert!(
        st.hidden_parent_steps > 25,
        "шагов со скрытым узлом и видимым ребёнком: {}",
        st.hidden_parent_steps
    );
    assert!(
        st.preview_steps > 150,
        "шагов с активным превью: {}",
        st.preview_steps
    );
}
