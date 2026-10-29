//! Случайные последовательности операций не ломают дерево, а полная отмена
//! возвращает страницу в исходное (пустое) состояние. Отмена и повтор идут
//! вперемешку с правками; состояние после каждого шага сверяется с моделью истории.

use std::collections::HashSet;
use std::mem::replace;

use ferrum_doc::container;
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
    MoveMany {
        nodes: (usize, usize, usize),
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
    /// Правки текста (подсистема 3): вставка буквы или перевода строки, удаление
    /// диапазона, кегль диапазона. Узел не текст — правки нет.
    TextInsert {
        node: usize,
        at: usize,
        ch: u8,
    },
    TextDelete {
        node: usize,
        a: usize,
        b: usize,
    },
    TextFormat {
        node: usize,
        a: usize,
        b: usize,
        size: u8,
    },
    Undo,
    Redo,
}

fn edit_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<usize>(), 0u8..7).prop_map(|(parent, kind)| Op::Create { parent, kind }),
        (any::<usize>(), any::<usize>(), 0usize..5).prop_map(|(node, parent, index)| Op::Move {
            node,
            parent,
            index
        }),
        (any::<(usize, usize, usize)>(), any::<usize>(), 0usize..5).prop_map(
            |(nodes, parent, index)| Op::MoveMany {
                nodes,
                parent,
                index
            }
        ),
        any::<usize>().prop_map(|node| Op::Delete { node }),
        (any::<usize>(), -1000i32..1000).prop_map(|(node, x)| Op::SetX { node, x }),
        (any::<usize>(), any::<u8>()).prop_map(|(node, rgb)| Op::Fill { node, rgb }),
        (any::<usize>(), any::<usize>(), any::<u8>()).prop_map(|(node, at, ch)| Op::TextInsert {
            node,
            at,
            ch
        }),
        (any::<usize>(), any::<usize>(), any::<usize>()).prop_map(|(node, a, b)| Op::TextDelete {
            node,
            a,
            b
        }),
        (any::<usize>(), any::<usize>(), any::<usize>(), any::<u8>())
            .prop_map(|(node, a, b, size)| Op::TextFormat { node, a, b, size }),
    ]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => edit_op(),
        1 => Just(Op::Undo),
        1 => Just(Op::Redo),
    ]
}

const KINDS: [NodeKind; 7] = [
    NodeKind::Frame,
    NodeKind::Group,
    NodeKind::Rect,
    NodeKind::Ellipse,
    NodeKind::Line,
    NodeKind::Image,
    NodeKind::Text,
];

/// Правка текста: шаг истории есть, только если текст изменился (кегль, уже
/// стоящий на диапазоне, — не правка).
fn is_text_op(op: &Op) -> bool {
    matches!(
        op,
        Op::TextInsert { .. } | Op::TextDelete { .. } | Op::TextFormat { .. }
    )
}

/// Длина текста узла (с последним переводом строки); не текст — `None`.
fn text_len(p: &Tx, n: NodeId) -> DocResult<Option<usize>> {
    Ok(p.props(n)?.text.map(|t| t.content.char_len()))
}

/// `pick(i)` — живой узел или корень (None), чтобы операции попадали в существующие узлы.
fn pick(alive: &[NodeId], i: usize) -> Option<NodeId> {
    if alive.is_empty() || i % (alive.len() + 1) == alive.len() {
        None
    } else {
        Some(alive[i % (alive.len() + 1)])
    }
}

/// Попытается ли правка изменить страницу (а не выйти по `pick` в пустую ветку).
fn attempted(op: &Op, alive: &[NodeId]) -> bool {
    match *op {
        Op::Create { .. } => true,
        Op::Move { node, .. }
        | Op::MoveMany {
            nodes: (node, _, _),
            ..
        }
        | Op::Delete { node }
        | Op::SetX { node, .. }
        | Op::Fill { node, .. } => pick(alive, node).is_some(),
        Op::TextInsert { .. } | Op::TextDelete { .. } | Op::TextFormat { .. } => false,
        Op::Undo | Op::Redo => false,
    }
}

fn apply_edit(p: &Tx, op: &Op, alive: &[NodeId]) -> DocResult<()> {
    match *op {
        Op::Create { parent, kind } => {
            p.create_node(pick(alive, parent), None, KINDS[kind as usize])?;
        }
        Op::Move {
            node,
            parent,
            index,
        } => {
            if let Some(n) = pick(alive, node) {
                p.move_node(n, pick(alive, parent), index)?;
            }
        }
        Op::MoveMany {
            nodes: (a, b, c),
            parent,
            index,
        } => {
            // Первый узел задаёт, будет ли правка; остальные — живые узлы или пропуск.
            if let Some(first) = pick(alive, a) {
                let ids: Vec<NodeId> = [Some(first), pick(alive, b), pick(alive, c)]
                    .into_iter()
                    .flatten()
                    .collect();
                p.move_nodes(&ids, pick(alive, parent), index)?;
            }
        }
        Op::Delete { node } => {
            if let Some(n) = pick(alive, node) {
                p.delete_nodes(&[n])?;
            }
        }
        Op::SetX { node, x } => {
            if let Some(n) = pick(alive, node) {
                p.set_props(
                    n,
                    &PropsPatch {
                        x: Some(x as f64),
                        ..Default::default()
                    },
                )?;
            }
        }
        Op::Fill { node, rgb } => {
            if let Some(n) = pick(alive, node) {
                // Синий канал 1: такой заливки нет ни у одного узла по умолчанию (у
                // текста — чёрная), и правка всегда что-то меняет.
                let fills = vec![Fill::solid(Color::rgba(rgb, rgb, 1, 255))];
                p.set_props(
                    n,
                    &PropsPatch {
                        fills: Some(fills),
                        ..Default::default()
                    },
                )?;
            }
        }
        Op::TextInsert { node, at, ch } => {
            if let Some(n) = pick(alive, node) {
                if let Some(len) = text_len(p, n)? {
                    let s = ["а", "б", "x", " ", "\n"][ch as usize % 5];
                    p.insert_text(n, at % len, s, None)?;
                }
            }
        }
        Op::TextDelete { node, a, b } => {
            if let Some(n) = pick(alive, node) {
                if let Some(len) = text_len(p, n)? {
                    // Последний перевод строки не удаляется.
                    let body = len - 1;
                    if body > 0 {
                        let start = a % body;
                        p.delete_text(n, start..start + 1 + b % (body - start))?;
                    }
                }
            }
        }
        Op::TextFormat { node, a, b, size } => {
            if let Some(n) = pick(alive, node) {
                if let Some(len) = text_len(p, n)? {
                    let start = a % len;
                    let attrs = TextAttrs {
                        size: Some(10.0 + (size % 3) as f64),
                        ..Default::default()
                    };
                    p.format_text(n, start..start + 1 + b % (len - start), &attrs, &[])?;
                }
            }
        }
        Op::Undo | Op::Redo => unreachable!("undo и redo не проходят через transact"),
    }
    Ok(())
}

/// Состояние страницы: порядок узлов и свойства каждого.
type Snapshot = Vec<(NodeId, Props)>;

fn snapshot(p: &PageDoc) -> Snapshot {
    p.walk()
        .into_iter()
        .map(|id| (id, p.props(id).expect("свойства читаются")))
        .collect()
}

/// Модель истории: текущее состояние и стеки состояний, которые должны открывать undo и redo.
#[derive(Default)]
struct Model {
    cur: Snapshot,
    past: Vec<Snapshot>,
    future: Vec<Snapshot>,
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

/// Сохранение и открытие по кэшу сцены — быстрый путь продакшена — дают ту же
/// страницу: тот же порядок, стабильные ID и свойства, в том числе у узлов,
/// заново созданных отменой удаления под новыми `TreeID`. Возвращает открытый документ.
fn check_reopen(doc: &mut Document, page: PageId) -> Document {
    let live = doc.page(page).unwrap();
    let expected = snapshot(live);
    let rows = live.scene_rows().unwrap();
    let opened = container::decode(&container::encode(doc).unwrap()).unwrap();
    assert!(opened.warnings.is_empty(), "{:?}", opened.warnings);
    let mut reopened = opened.document;
    assert_eq!(
        reopened.cached_scene(page),
        Some(rows.as_slice()),
        "кэш сцены расходится с живой страницей"
    );
    let p = reopened.page(page).unwrap();
    assert!(p.loaded_from_cache(), "страница открылась не по кэшу сцены");
    assert_eq!(snapshot(p), expected, "страница после открытия по кэшу");
    check_tree(p);
    reopened
}

/// На открытом по кэшу документе правка и её отмена держат дерево согласованным.
fn edit_and_undo_reopened(doc: &mut Document, page: PageId) {
    let mut reopened = check_reopen(doc, page);
    let before = snapshot(reopened.page(page).unwrap());
    let alive = reopened.page(page).unwrap().walk();
    reopened
        .transact(page, vec![], |p| {
            let new = p.create_node(alive.first().copied(), None, NodeKind::Rect)?;
            p.set_props(
                new,
                &PropsPatch {
                    x: Some(42.0),
                    ..Default::default()
                },
            )?;
            if let Some(last) = alive.last() {
                p.move_nodes(&[new, *last], None, 0)?;
            }
            if let Some(first) = alive.first() {
                p.delete_nodes(&[*first])?;
            }
            Ok(vec![new])
        })
        .unwrap();
    check_tree(reopened.page(page).unwrap());
    assert!(reopened.undo().unwrap().is_some());
    assert_eq!(snapshot(reopened.page(page).unwrap()), before);
    // Отмена удаления создала узлы заново — повторное открытие по кэшу их сохраняет.
    check_reopen(&mut reopened, page);
}

/// Дерево валидно, состояние совпадает с моделью, доступность undo/redo — тоже.
fn check_model(doc: &mut Document, page: PageId, m: &Model) {
    let p = doc.page(page).unwrap();
    check_tree(p);
    assert_eq!(
        snapshot(p),
        m.cur,
        "состояние страницы расходится с моделью истории"
    );
    assert_eq!(
        doc.can_undo(),
        !m.past.is_empty(),
        "can_undo расходится с моделью"
    );
    assert_eq!(
        doc.can_redo(),
        !m.future.is_empty(),
        "can_redo расходится с моделью"
    );
}

proptest! {
    // Детерминированное зерно: каждый прогон проверяет одни и те же 64 сценария,
    // и падение воспроизводимо, а не зависит от удачи.
    #![proptest_config(ProptestConfig {
        cases: 64,
        rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x0f_e2_2a_11),
        ..ProptestConfig::default()
    })]
    #[test]
    fn random_ops_keep_tree_valid_and_undo_all_empties_page(ops in prop::collection::vec(op(), 1..40)) {
        // Без создания узла проверять нечего: сценарий не считается.
        // Создание в корне всегда успешно, поэтому при нём страница точно станет непустой.
        prop_assume!(ops.iter().any(|o| matches!(o, Op::Create { .. })));

        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        let mut m = Model {
            cur: snapshot(doc.page(page).unwrap()),
            ..Default::default()
        };
        let mut applied = 0usize;
        let mut max_size = 0usize;

        for op in ops {
            let alive = doc.page(page).unwrap().walk();
            match op {
                Op::Undo => {
                    let restored = doc.undo().expect("undo не должен падать");
                    match m.past.pop() {
                        Some(prev) => {
                            assert!(restored.is_some(), "undo не откатил шаг, который есть в модели");
                            m.future.push(replace(&mut m.cur, prev));
                        }
                        None => assert!(restored.is_none(), "undo откатил шаг, которого нет в модели"),
                    }
                }
                Op::Redo => {
                    let restored = doc.redo().expect("redo не должен падать");
                    match m.future.pop() {
                        Some(next) => {
                            assert!(restored.is_some(), "redo не повторил шаг, который есть в модели");
                            m.past.push(replace(&mut m.cur, next));
                        }
                        None => assert!(restored.is_none(), "redo повторил шаг, которого нет в модели"),
                    }
                }
                edit => {
                    let tried = attempted(&edit, &alive);
                    let result = doc.transact(page, vec![], |p| {
                        apply_edit(p, &edit, &alive)?;
                        Ok(vec![])
                    });
                    match result {
                        Ok(_) => {
                            let after = snapshot(doc.page(page).unwrap());
                            let changed = if is_text_op(&edit) { after != m.cur } else { tried };
                            if changed {
                                m.past.push(replace(&mut m.cur, after));
                                m.future.clear();
                                applied += 1;
                            }
                        }
                        // Перемещение в собственное поддерево — ожидаемый отказ; откат не оставляет следов.
                        Err(DocError::CyclicMove) if matches!(edit, Op::Move { .. } | Op::MoveMany { .. }) => {}
                        Err(e) => panic!("неожиданная ошибка операции {edit:?}: {e}"),
                    }
                }
            }

            // Ids, которых не стало после шага, не должны оставаться в хранилище.
            let p = doc.page(page).unwrap();
            let now = p.walk();
            for id in alive.iter().filter(|id| !now.contains(id)) {
                assert!(!p.contains(*id), "удалённый узел всё ещё на странице");
            }
            check_model(&mut doc, page, &m);
            check_reopen(&mut doc, page);
            max_size = max_size.max(m.cur.len());
        }

        // Счётчики: проверка не должна проходить вхолостую.
        prop_assert!(applied > 0, "ни одна правка не применилась");
        prop_assert!(max_size > 0, "страница ни разу не была непустой");

        // Полный повтор доходит до конца линейной истории: это самое дальнее состояние
        // в стеке повтора (его дно), а если повтора нет — текущее состояние.
        let target = m
            .future
            .first()
            .cloned()
            .unwrap_or_else(|| m.cur.clone());

        // Полная отмена возвращает пустую страницу.
        while doc.undo().unwrap().is_some() {
            let prev = m.past.pop().expect("undo сработал без шага в модели");
            m.future.push(replace(&mut m.cur, prev));
            check_model(&mut doc, page, &m);
        }
        prop_assert!(doc.page(page).unwrap().walk().is_empty());

        while doc.redo().unwrap().is_some() {
            let next = m.future.pop().expect("redo сработал без шага в модели");
            m.past.push(replace(&mut m.cur, next));
            check_model(&mut doc, page, &m);
        }
        prop_assert_eq!(snapshot(doc.page(page).unwrap()), target);
        edit_and_undo_reopened(&mut doc, page);
    }
}
