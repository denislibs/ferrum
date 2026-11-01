//! Булевы операции (подсистема 3, этап 4): операция над контурами — свойства и
//! точность на касаниях и общих рёбрах; результат булевой группы в сцене —
//! границы, попадание, пересчёт при правке и превью детей, вложенность,
//! скрытые дети, пустой результат, текст через контуры глифов.

use std::rc::Rc;
use std::sync::Arc;

use ferrum_doc::{
    BooleanOp, Document, FillRule, NodeId, NodeKind, PageId, Props, PropsPatch, SceneRow,
};
use ferrum_scene::boolean::{combine, flatten_path, normalize, Contours};
use ferrum_scene::{testing::snapshot, Scene};
use kurbo::{BezPath, Circle, Point, Rect, Shape};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Contours {
    normalize(
        flatten_path(&Rect::new(x0, y0, x1, y1).to_path(0.1), 0.01),
        FillRule::NonZero,
    )
}

fn area(c: &Contours) -> f64 {
    c.to_path().area().abs()
}

#[test]
fn ops_follow_figma_semantics_on_two_squares() {
    let a = rect(0.0, 0.0, 10.0, 10.0);
    let b = rect(5.0, 5.0, 15.0, 15.0);
    let ab = [a.clone(), b.clone()];
    assert!((area(&combine(BooleanOp::Union, &ab)) - 175.0).abs() < 1e-9);
    assert!((area(&combine(BooleanOp::Subtract, &ab)) - 75.0).abs() < 1e-9);
    assert!((area(&combine(BooleanOp::Intersect, &ab)) - 25.0).abs() < 1e-9);
    assert!((area(&combine(BooleanOp::Exclude, &ab)) - 150.0).abs() < 1e-9);
    // Вычитание — из нижнего: порядок важен.
    let ba = [b, a];
    assert!((area(&combine(BooleanOp::Subtract, &ba)) - 75.0).abs() < 1e-9);
    // Одна форма — она сама.
    let one = [rect(0.0, 0.0, 3.0, 4.0)];
    for op in BooleanOp::ALL {
        assert!((area(&combine(op, &one)) - 12.0).abs() < 1e-9);
    }
    assert!(combine(BooleanOp::Union, &[]).is_empty());
}

#[test]
fn touching_and_coincident_edges_are_exact() {
    let a = rect(0.0, 0.0, 10.0, 10.0);
    let right = rect(10.0, 0.0, 20.0, 10.0);
    // Встык — один контур, без щели и без шва.
    let u = combine(BooleanOp::Union, &[a.clone(), right.clone()]);
    assert_eq!(u.0.len(), 1);
    assert!((area(&u) - 200.0).abs() < 1e-9);
    assert!(combine(BooleanOp::Intersect, &[a.clone(), right]).is_empty());
    // Совпадающие — исключение и вычитание пусты, объединение — один квадрат.
    let same = a.clone();
    assert!(combine(BooleanOp::Exclude, &[a.clone(), same.clone()]).is_empty());
    assert!(combine(BooleanOp::Subtract, &[a.clone(), same.clone()]).is_empty());
    let u = combine(BooleanOp::Union, &[a.clone(), same]);
    assert_eq!(u.0.len(), 1);
    assert!((area(&u) - 100.0).abs() < 1e-9);
    // Касание углом — два контура.
    let corner = rect(10.0, 10.0, 20.0, 20.0);
    assert_eq!(combine(BooleanOp::Union, &[a.clone(), corner]).0.len(), 2);
    // Общая сторона при вычитании — ровно половина.
    let half = rect(0.0, 5.0, 10.0, 15.0);
    assert!((area(&combine(BooleanOp::Subtract, &[a, half])) - 50.0).abs() < 1e-9);
}

#[test]
fn shapes_are_normalized_by_their_own_rule_and_orientation() {
    // Звезда из пересекающихся отрезков: ненулевое правило заливает середину,
    // чётно-нечётное — нет; форма из обращённого контура — та же.
    let mut star = BezPath::new();
    let pts = (0..5).map(|i| {
        let a = std::f64::consts::TAU * (i as f64 * 2.0) / 5.0;
        Point::new(50.0 + 40.0 * a.sin(), 50.0 - 40.0 * a.cos())
    });
    for (i, p) in pts.enumerate() {
        if i == 0 {
            star.move_to(p);
        } else {
            star.line_to(p);
        }
    }
    star.close_path();
    let nz = normalize(flatten_path(&star, 0.01), FillRule::NonZero);
    let eo = normalize(flatten_path(&star, 0.01), FillRule::EvenOdd);
    assert!(area(&nz) > area(&eo) + 100.0);
    // Обращённый квадрат объединяется с прямым без дыры на пересечении.
    let mut rev = flatten_path(&Rect::new(5.0, 5.0, 15.0, 15.0).to_path(0.1), 0.01);
    rev[0].reverse();
    let rev = normalize(rev, FillRule::NonZero);
    let u = combine(BooleanOp::Union, &[rect(0.0, 0.0, 10.0, 10.0), rev]);
    assert!((area(&u) - 175.0).abs() < 1e-9);
    // Круг раздроблен с точностью: площадь — до долей процента.
    let c = normalize(
        flatten_path(&Circle::new((0.0, 0.0), 100.0).to_path(1e-4), 0.005),
        FillRule::NonZero,
    );
    let exact = std::f64::consts::PI * 1e4;
    assert!((area(&c) - exact).abs() / exact < 1e-4);
}

fn patch(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

/// Булева группа `op` в (100, 100) с прямоугольником 0..100 и эллипсом 50..150
/// по x. Возвращает [группа, прямоугольник, эллипс].
fn sample(op: BooleanOp) -> (Document, PageId, [NodeId; 3]) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |tx| {
            let b = tx.create_with_props(
                None,
                None,
                &Props {
                    x: 100.0,
                    y: 100.0,
                    boolean_op: Some(op),
                    ..Props::defaults(NodeKind::Boolean)
                },
            )?;
            let r = tx.create_node(Some(b), None, NodeKind::Rect)?;
            tx.set_props(r, &patch(0.0, 0.0, 100.0, 100.0))?;
            let e = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
            tx.set_props(e, &patch(50.0, 0.0, 100.0, 100.0))?;
            Ok(vec![b, r, e])
        })
        .unwrap();
    doc.take_changes(page);
    (doc, page, ids.try_into().unwrap())
}

fn local_bounds(scene: &Scene, id: NodeId) -> Rect {
    scene.node(id).unwrap().local_bounds.unwrap()
}

/// Совпадение рамок с точностью дробления кривых.
fn near(a: Rect, b: Rect) -> bool {
    let ok = [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
        .iter()
        .all(|d| d.abs() < 0.01);
    if !ok {
        eprintln!("{a:?} ≠ {b:?}");
    }
    ok
}

#[test]
fn result_gives_bounds_and_hits_and_children_are_hit_only_inside_it() {
    let (mut doc, page, [b, r, e]) = sample(BooleanOp::Subtract);
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Вычитание: прямоугольник без правой половины эллипса.
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 100.0, 100.0)
    ));
    assert_eq!(scene.hit_test(Point::new(110.0, 150.0), 0.0), Some(r));
    // Внутри эллипса (дыра) — мимо, хотя прямоугольник и эллипс там есть.
    assert_eq!(scene.hit_test(Point::new(190.0, 150.0), 0.0), None);
    assert_eq!(scene.hit_test(Point::new(240.0, 150.0), 0.0), None);
    let hits = scene.hit_test_all(Point::new(110.0, 150.0), 0.0);
    assert!(hits.contains(&b) && !hits.contains(&e), "{hits:?}");
    // Объединение: и эллипс справа.
    let (mut doc, page, [b, _, e]) = sample(BooleanOp::Union);
    let scene = Scene::from_page(doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 150.0, 100.0)
    ));
    assert_eq!(scene.hit_test(Point::new(240.0, 150.0), 0.0), Some(e));
    let shape = scene.node(b).unwrap().path_shape().unwrap().clone();
    assert!(shape.closed && shape.regions.len() == 1);
}

#[test]
fn child_edit_and_preview_recompute_the_result_live() {
    let (mut doc, page, [b, r, e]) = sample(BooleanOp::Intersect);
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(50.0, 0.0, 100.0, 100.0)
    ));
    let v0 = scene.node(b).unwrap().version;
    // Превью жеста ребёнка (перетаскивание) — результат уже в сцене.
    scene.set_preview(&[(
        e,
        PropsPatch {
            x: Some(80.0),
            ..Default::default()
        },
    )]);
    // Эллипс 80..180: у x = 100 он от 10 до 90 по y.
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(80.0, 10.0, 100.0, 90.0)
    ));
    assert_ne!(
        scene.node(b).unwrap().version,
        v0,
        "рендер перестроит список"
    );
    scene.clear_preview();
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(50.0, 0.0, 100.0, 100.0)
    ));
    // Правка документа: ребёнок сжался.
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            r,
            &PropsPatch {
                w: Some(70.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    scene.apply(&doc.take_changes(page), doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(50.0, 10.0, 70.0, 90.0)
    ));
    assert_eq!(
        snapshot(&scene),
        snapshot(&Scene::from_page(doc.page(page).unwrap()))
    );
    // Сдвиг самой группы результат не пересчитывает (тот же `Arc`).
    let before = scene.node(b).unwrap().boolean.clone().unwrap();
    scene.set_preview(&[(
        b,
        PropsPatch {
            x: Some(300.0),
            ..Default::default()
        },
    )]);
    let after = scene.node(b).unwrap().boolean.clone().unwrap();
    assert!(Arc::ptr_eq(&before, &after));
    // Смена операции — новый результат.
    scene.clear_preview();
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            b,
            &PropsPatch {
                boolean_op: Some(BooleanOp::Union),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    scene.apply(&doc.take_changes(page), doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 150.0, 100.0)
    ));
}

#[test]
fn hidden_children_lines_and_empty_results() {
    let (mut doc, page, [b, r, e]) = sample(BooleanOp::Union);
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            e,
            &PropsPatch {
                visible: Some(false),
                ..Default::default()
            },
        )?;
        // Линия формы заливки не имеет — в результат не входит.
        let l = tx.create_node(Some(b), None, NodeKind::Line)?;
        tx.set_props(l, &patch(0.0, 300.0, 500.0, 0.0))?;
        Ok(vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 100.0, 100.0)
    ));
    // Пустое пересечение: ничего не рисуется и не ловит щелчок, рамка — по детям.
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            e,
            &PropsPatch {
                visible: Some(true),
                x: Some(500.0),
                ..Default::default()
            },
        )?;
        tx.set_props(
            b,
            &PropsPatch {
                boolean_op: Some(BooleanOp::Intersect),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    let n = scene.node(b).unwrap();
    assert!(n.boolean.as_ref().unwrap().contours.is_empty());
    assert!(near(
        n.local_bounds.unwrap(),
        Rect::new(0.0, 0.0, 600.0, 300.0)
    ));
    assert_eq!(scene.hit_test(Point::new(150.0, 150.0), 0.0), None);
    let _ = r;
}

#[test]
fn nested_booleans_groups_and_flipped_children() {
    let (mut doc, page, [b, _, _]) = sample(BooleanOp::Union);
    let inner = doc
        .transact(page, vec![], |tx| {
            // Вложенная «Вычитание»: квадрат 200..300 без 250..300 по x.
            let s = tx.create_with_props(
                Some(b),
                None,
                &Props {
                    x: 200.0,
                    boolean_op: Some(BooleanOp::Subtract),
                    ..Props::defaults(NodeKind::Boolean)
                },
            )?;
            let base = tx.create_node(Some(s), None, NodeKind::Rect)?;
            tx.set_props(base, &patch(0.0, 0.0, 100.0, 100.0))?;
            let cut = tx.create_node(Some(s), None, NodeKind::Rect)?;
            tx.set_props(cut, &patch(50.0, 0.0, 100.0, 100.0))?;
            // Группа с отражённым прямоугольником: объединяется без дыр.
            let g = tx.create_node(Some(b), None, NodeKind::Group)?;
            let f = tx.create_node(Some(g), None, NodeKind::Rect)?;
            tx.set_props(
                f,
                &PropsPatch {
                    flip_x: Some(true),
                    ..patch(155.0, 0.0, 30.0, 100.0)
                },
            )?;
            Ok(vec![s])
        })
        .unwrap()[0];
    let scene = Scene::from_page(doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, inner),
        Rect::new(0.0, 0.0, 50.0, 100.0)
    ));
    let outer = &scene.node(b).unwrap().boolean.as_ref().unwrap().contours;
    // 0..150 (прямоугольник и эллипс) ∪ 155..185 ∪ 200..250: три куска.
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 250.0, 100.0)
    ));
    let total = area(outer);
    let ellipse_right = std::f64::consts::PI * 50.0 * 50.0 / 2.0;
    let expected = 100.0 * 100.0 + ellipse_right + 30.0 * 100.0 + 50.0 * 100.0;
    assert!((total - expected).abs() < 5.0, "{total} ≈ {expected}");
}

#[test]
fn text_uses_glyph_outlines_from_the_editor_and_refreshes() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let [b, _t]: [NodeId; 2] = doc
        .transact(page, vec![], |tx| {
            let b = tx.create_node(None, None, NodeKind::Boolean)?;
            let t = tx.create_node(Some(b), None, NodeKind::Text)?;
            tx.set_props(t, &patch(0.0, 0.0, 40.0, 20.0))?;
            Ok(vec![b, t])
        })
        .unwrap()
        .try_into()
        .unwrap();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    // Без источника контуров текст формы не даёт.
    assert!(scene
        .node(b)
        .unwrap()
        .boolean
        .as_ref()
        .unwrap()
        .contours
        .is_empty());
    let size = Rc::new(std::cell::Cell::new(10.0));
    let s = size.clone();
    scene.set_text_outlines(Some(Rc::new(move |_p: &Props| {
        Some(Rect::new(0.0, 0.0, s.get(), 10.0).to_path(0.1))
    })));
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 10.0, 10.0)
    ));
    // Шрифт пришёл — контуры другие, группа пересчитана.
    size.set(30.0);
    let counter = scene.change_counter();
    scene.refresh_text_outlines();
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 30.0, 10.0)
    ));
    assert!(scene.change_counter() > counter, "нужен новый кадр");
}

/// Сетка 6 × 5 перекрывающихся кругов (по 15, радиус 10) в булевой группе `op`.
fn grid(op: BooleanOp) -> (Document, PageId, NodeId, Vec<NodeId>) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut ids = doc
        .transact(page, vec![], |tx| {
            let b = tx.create_with_props(
                None,
                None,
                &Props {
                    boolean_op: Some(op),
                    ..Props::defaults(NodeKind::Boolean)
                },
            )?;
            let mut ids = vec![b];
            for i in 0..30 {
                let e = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
                tx.set_props(
                    e,
                    &patch((i % 6) as f64 * 15.0, (i / 6) as f64 * 15.0, 20.0, 20.0),
                )?;
                ids.push(e);
            }
            Ok(ids)
        })
        .unwrap();
    let b = ids.remove(0);
    (doc, page, b, ids)
}

/// Объединение(Вычитание(A, B), C): дети внутренней группы попадают только
/// там, где рисуется и она, и внешняя; в дыре внутренней группы, которую
/// закрывает C, щелчок не выбирает A и B.
#[test]
fn children_of_nested_booleans_are_hit_only_inside_every_enclosing_result() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let [outer, inner, a, b, c]: [NodeId; 5] = doc
        .transact(page, vec![], |tx| {
            let outer = tx.create_with_props(
                None,
                None,
                &Props {
                    boolean_op: Some(BooleanOp::Union),
                    ..Props::defaults(NodeKind::Boolean)
                },
            )?;
            let inner = tx.create_with_props(
                Some(outer),
                None,
                &Props {
                    boolean_op: Some(BooleanOp::Subtract),
                    ..Props::defaults(NodeKind::Boolean)
                },
            )?;
            let a = tx.create_node(Some(inner), None, NodeKind::Rect)?;
            tx.set_props(a, &patch(0.0, 0.0, 100.0, 100.0))?;
            let b = tx.create_node(Some(inner), None, NodeKind::Rect)?;
            tx.set_props(b, &patch(25.0, 25.0, 50.0, 50.0))?;
            let c = tx.create_node(Some(outer), None, NodeKind::Rect)?;
            tx.set_props(c, &patch(40.0, 40.0, 20.0, 20.0))?;
            Ok(vec![outer, inner, a, b, c])
        })
        .unwrap()
        .try_into()
        .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    // В дыре «Вычитания», закрытой C: только C и внешняя группа.
    let hits = scene.hit_test_all(Point::new(50.0, 50.0), 0.0);
    assert_eq!(hits, vec![c, outer]);
    // В дыре, которую C не закрывает: ничего.
    assert!(scene.hit_test_all(Point::new(30.0, 30.0), 0.0).is_empty());
    // На основе: A, обе группы.
    let hits = scene.hit_test_all(Point::new(10.0, 10.0), 0.0);
    assert_eq!(hits, vec![a, inner, outer]);
    assert!(!hits.contains(&b));
}

#[test]
fn gesture_on_one_child_recomputes_only_near_it_and_matches_the_full_result() {
    for op in BooleanOp::ALL {
        for moved in [0usize, 14] {
            let (mut doc, page, b, kids) = grid(op);
            let mut scene = Scene::from_page(doc.page(page).unwrap());
            for dx in [3.0, 7.0, 40.0] {
                scene.set_preview(&[(
                    kids[moved],
                    PropsPatch {
                        x: Some((moved % 6) as f64 * 15.0 + dx),
                        y: Some(-2.0),
                        ..Default::default()
                    },
                )]);
                // Тот же документ, записанный, — полный пересчёт.
                doc.transact(page, vec![], |tx| {
                    tx.set_props(
                        kids[moved],
                        &PropsPatch {
                            x: Some((moved % 6) as f64 * 15.0 + dx),
                            y: Some(-2.0),
                            ..Default::default()
                        },
                    )?;
                    Ok(vec![])
                })
                .unwrap();
                let full = Scene::from_page(doc.page(page).unwrap());
                let outline = scene.node(b).unwrap().boolean.clone().unwrap();
                assert!(outline.is_partial(), "во время жеста — по частям");
                let got = area(&outline.contours);
                let want = area(&full.node(b).unwrap().boolean.as_ref().unwrap().contours);
                assert!(
                    (got - want).abs() < 1e-3 * want.max(1.0),
                    "{op:?}, ребёнок {moved}, сдвиг {dx}: {got} ≠ {want}"
                );
                assert!(
                    near(local_bounds(&scene, b), local_bounds(&full, b)),
                    "{op:?} {moved} {dx}"
                );
            }
            // Жест кончился (как у редактора: превью снято, документ записан) —
            // без полного пересчёта: те же сложенные остальные.
            scene.clear_preview();
            let changes = doc.take_changes(page);
            scene.apply(&changes, doc.page(page).unwrap());
            let fresh = Scene::from_page(doc.page(page).unwrap());
            let outline = scene.node(b).unwrap().boolean.clone().unwrap();
            assert!(outline.is_partial(), "запись — по частям, {op:?} {moved}");
            let want = area(&fresh.node(b).unwrap().boolean.as_ref().unwrap().contours);
            assert!((area(&outline.contours) - want).abs() < 1e-3 * want.max(1.0));
            assert!(near(local_bounds(&scene, b), local_bounds(&fresh, b)));
            // Правка другого ребёнка без жеста — полный пересчёт, как у открытого.
            doc.transact(page, vec![], |tx| {
                tx.set_props(
                    kids[(moved + 7) % kids.len()],
                    &PropsPatch {
                        y: Some(-5.0),
                        ..Default::default()
                    },
                )?;
                Ok(vec![])
            })
            .unwrap();
            let changes = doc.take_changes(page);
            scene.apply(&changes, doc.page(page).unwrap());
            assert!(!scene
                .node(b)
                .unwrap()
                .boolean
                .as_ref()
                .unwrap()
                .is_partial());
            assert_eq!(
                snapshot(&scene),
                snapshot(&Scene::from_page(doc.page(page).unwrap())),
                "{op:?} {moved}"
            );
        }
    }
}

/// Жест над ребёнком `kid` группы `b` (превью на `dx`), затем запись: и в
/// жесте, и после записи результат `b` и внешней группы `top` — как у полного
/// пересчёта по площади и по числу контуров.
fn gesture_matches_full(
    doc: &mut Document,
    page: PageId,
    kid: NodeId,
    b: NodeId,
    top: NodeId,
    dx: f64,
) {
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let x0 = scene.node(kid).unwrap().props.x;
    let moved = PropsPatch {
        x: Some(x0 + dx),
        ..Default::default()
    };
    scene.set_preview(&[(kid, moved.clone())]);
    doc.transact(page, vec![], |tx| tx.set_props(kid, &moved).map(|_| vec![]))
        .unwrap();
    let full = Scene::from_page(doc.page(page).unwrap());
    let same = |scene: &Scene, what: &str| {
        for id in [b, top] {
            let got = scene.node(id).unwrap().boolean.clone().unwrap();
            let want = full.node(id).unwrap().boolean.clone().unwrap();
            assert_eq!(
                got.contours.0.len(),
                want.contours.0.len(),
                "{what}: контуры"
            );
            assert!(
                (area(&got.contours) - area(&want.contours)).abs()
                    < 1e-3 * area(&want.contours).max(1.0),
                "{what}: площадь {} ≠ {}",
                area(&got.contours),
                area(&want.contours)
            );
        }
    };
    same(&scene, "жест");
    scene.clear_preview();
    let changes = doc.take_changes(page);
    scene.apply(&changes, doc.page(page).unwrap());
    same(&scene, "запись");
}

#[test]
fn one_visible_child_keeps_its_shape_in_gesture_and_after_commit() {
    for op in BooleanOp::ALL {
        for hidden in [false, true] {
            let mut doc = Document::new();
            let page = doc.pages()[0].0;
            let ids = doc
                .transact(page, vec![], |tx| {
                    let b = tx.create_with_props(
                        None,
                        None,
                        &Props {
                            boolean_op: Some(op),
                            ..Props::defaults(NodeKind::Boolean)
                        },
                    )?;
                    let e = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
                    tx.set_props(e, &patch(0.0, 0.0, 20.0, 20.0))?;
                    let mut ids = vec![b, e];
                    if hidden {
                        let h = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
                        tx.set_props(
                            h,
                            &PropsPatch {
                                visible: Some(false),
                                ..patch(5.0, 5.0, 20.0, 20.0)
                            },
                        )?;
                        ids.push(h);
                    }
                    Ok(ids)
                })
                .unwrap();
            gesture_matches_full(&mut doc, page, ids[1], ids[0], ids[0], 7.0);
            let scene = Scene::from_page(doc.page(page).unwrap());
            assert!(
                area(
                    &scene
                        .node(ids[0])
                        .unwrap()
                        .boolean
                        .as_ref()
                        .unwrap()
                        .contours
                ) > 300.0,
                "{op:?} hidden={hidden}"
            );
        }
    }
}

/// Вложенные группы: внешняя `outer` над прямоугольником и внутренней `inner`
/// из 12 кругов (сетка 4 × 3, шаг 15, диаметр 20). Три жеста подряд — над
/// детьми 0, 5 и снова 0: первый оставляет круг 0 в (40, 2), второй ставит
/// круг 5 точно туда же — два круга совпадают, и вычитание в два приёма
/// оставляло осколок там, где полный пересчёт даёт пустоту (а внешнее
/// пересечение — 0 вместо 3600). Исключение при этом расходилось числом контуров.
#[test]
fn nested_groups_match_the_full_result_when_two_circles_coincide() {
    for outer_op in BooleanOp::ALL {
        for inner_op in BooleanOp::ALL {
            let mut doc = Document::new();
            let page = doc.pages()[0].0;
            let ids = doc
                .transact(page, vec![], |tx| {
                    let mk = |op| Props {
                        boolean_op: Some(op),
                        ..Props::defaults(NodeKind::Boolean)
                    };
                    let outer = tx.create_with_props(None, None, &mk(outer_op))?;
                    let r = tx.create_node(Some(outer), None, NodeKind::Rect)?;
                    tx.set_props(r, &patch(-10.0, -10.0, 60.0, 60.0))?;
                    let inner = tx.create_with_props(Some(outer), None, &mk(inner_op))?;
                    let mut ids = vec![outer, inner];
                    for k in 0..12 {
                        let e = tx.create_node(Some(inner), None, NodeKind::Ellipse)?;
                        tx.set_props(
                            e,
                            &patch((k % 4) as f64 * 15.0, (k / 4) as f64 * 15.0, 20.0, 20.0),
                        )?;
                        ids.push(e);
                    }
                    Ok(ids)
                })
                .unwrap();
            let mut scene = Scene::from_page(doc.page(page).unwrap());
            let check = |scene: &Scene, doc: &mut Document, what: &str| {
                let full = Scene::from_page(doc.page(page).unwrap());
                for id in [ids[1], ids[0]] {
                    let got = scene.node(id).unwrap().boolean.clone().unwrap();
                    let want = full.node(id).unwrap().boolean.clone().unwrap();
                    let tag = format!("{outer_op:?}/{inner_op:?} {what}");
                    assert_eq!(
                        got.contours.0.len(),
                        want.contours.0.len(),
                        "{tag}: контуры"
                    );
                    assert!(
                        (area(&got.contours) - area(&want.contours)).abs()
                            < 1e-3 * area(&want.contours).max(1.0),
                        "{tag}: площадь {} ≠ {}",
                        area(&got.contours),
                        area(&want.contours)
                    );
                }
            };
            for kid in [ids[2], ids[7], ids[2]] {
                for dx in [3.0, 9.0, 40.0] {
                    let moved = PropsPatch {
                        x: Some(dx),
                        y: Some(2.0),
                        ..Default::default()
                    };
                    scene.set_preview(&[(kid, moved.clone())]);
                    doc.transact(page, vec![], |tx| tx.set_props(kid, &moved).map(|_| vec![]))
                        .unwrap();
                    check(&scene, &mut doc, &format!("жест dx={dx}"));
                }
                scene.clear_preview();
                let changes = doc.take_changes(page);
                scene.apply(&changes, doc.page(page).unwrap());
                check(&scene, &mut doc, "запись");
            }
        }
    }
}

#[test]
fn subtract_fully_covered_by_dragged_child_is_empty_like_the_full_result() {
    for base_moves in [false, true] {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        let ids = doc
            .transact(page, vec![], |tx| {
                let b = tx.create_with_props(
                    None,
                    None,
                    &Props {
                        boolean_op: Some(BooleanOp::Subtract),
                        ..Props::defaults(NodeKind::Boolean)
                    },
                )?;
                let base = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
                tx.set_props(base, &patch(20.0, 20.0, 20.0, 20.0))?;
                let cover = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
                tx.set_props(cover, &patch(100.0, 0.0, 60.0, 60.0))?;
                let other = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
                tx.set_props(other, &patch(200.0, 0.0, 20.0, 20.0))?;
                Ok(vec![b, base, cover, other])
            })
            .unwrap();
        // Покрывающий круг приезжает на основу (или основа — под него).
        if base_moves {
            gesture_matches_full(&mut doc, page, ids[1], ids[0], ids[0], 90.0);
        } else {
            gesture_matches_full(&mut doc, page, ids[2], ids[0], ids[0], -90.0);
        }
    }
}

/// Осколок площадью меньше `SLIVER_AREA` — не результат: ни полная операция,
/// ни пересчёт по частям его не оставляют.
#[test]
fn slivers_below_the_tolerance_are_dropped_by_full_and_partial_results() {
    use ferrum_scene::boolean::{combine_rest, combine_with, Rest};
    let big = rect(0.0, 0.0, 10.0, 10.0);
    // Почти такой же прямоугольник, короче на 1e-6: остаётся полоска площадью 1e-5.
    let almost = rect(0.0, 0.0, 10.0, 10.0 - 1e-6);
    let full = combine(BooleanOp::Subtract, &[big.clone(), almost.clone()]);
    assert!(full.is_empty(), "{} контуров", full.0.len());
    let (mode, parts) = combine_rest(
        BooleanOp::Subtract,
        &[big.clone(), almost.clone(), rect(50.0, 50.0, 60.0, 60.0)],
        1,
    );
    assert_eq!(mode, Rest::Minus);
    let part = combine_with(mode, &parts, &almost);
    assert!(part.0.len() <= 1);
    let (mode, parts) = combine_rest(
        BooleanOp::Subtract,
        &[almost.clone(), big.clone(), rect(50.0, 50.0, 60.0, 60.0)],
        0,
    );
    assert_eq!(mode, Rest::From);
    let from = combine_with(mode, &parts, &almost);
    assert!(from.is_empty(), "{} контуров", from.0.len());
}

#[test]
fn forcing_full_booleans_drops_the_history_of_a_gesture() {
    let (mut doc, page, b, kids) = grid(BooleanOp::Union);
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let moved = PropsPatch {
        x: Some(33.0),
        y: Some(-2.0),
        ..Default::default()
    };
    scene.set_preview(&[(kids[3], moved.clone())]);
    doc.transact(page, vec![], |tx| {
        tx.set_props(kids[3], &moved).map(|_| vec![])
    })
    .unwrap();
    scene.clear_preview();
    let changes = doc.take_changes(page);
    scene.apply(&changes, doc.page(page).unwrap());
    assert!(scene
        .node(b)
        .unwrap()
        .boolean
        .as_ref()
        .unwrap()
        .is_partial());
    let before = scene.node(b).unwrap().subtree_version;
    let kid_before = scene.node(kids[3]).unwrap().subtree_version;
    scene.force_full_booleans(scene.slot(b).unwrap());
    // Кэши рендера по версии поддерева не держат результат по частям.
    assert_ne!(scene.node(b).unwrap().subtree_version, before);
    assert_ne!(scene.node(kids[3]).unwrap().subtree_version, kid_before);
    assert!(!scene
        .node(b)
        .unwrap()
        .boolean
        .as_ref()
        .unwrap()
        .is_partial());
    assert_eq!(
        snapshot(&scene),
        snapshot(&Scene::from_page(doc.page(page).unwrap()))
    );
}

/// Файл с булевой группой над цепочкой из 20 000 вложенных групп открывается
/// на стеке worker'а (1 МБ — как у wasm32): форма группы считается без
/// рекурсии. Строки цепочки размножены из строк маленькой страницы (тот же
/// путь, что у кэша сцены и `Scene::from_page`, но без Loro: его чтение
/// глубокой страницы в отладочной сборке само упирается в стек и к этапу не
/// относится).
#[test]
fn deep_group_chain_in_a_boolean_does_not_overflow_the_stack() {
    const DEPTH: usize = 20_000;
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.transact(page, vec![], |tx| {
        let b = tx.create_node(None, None, NodeKind::Boolean)?;
        let g = tx.create_node(Some(b), None, NodeKind::Group)?;
        let r = tx.create_node(Some(g), None, NodeKind::Rect)?;
        tx.set_props(r, &patch(0.0, 0.0, 10.0, 10.0))?;
        Ok(vec![])
    })
    .unwrap();
    let template = doc.page(page).unwrap().scene_rows().unwrap();
    let [boolean, group, rect]: [SceneRow; 3] = template.try_into().unwrap();
    let b = boolean.id;
    let peer = b.to_string().split('@').nth(1).unwrap().to_string();
    let fresh = |i: usize| -> NodeId { format!("{}@{peer}", 1_000_000 + i).parse().unwrap() };
    let mut rows = vec![boolean];
    for i in 0..DEPTH {
        let mut g = group.clone();
        g.id = fresh(i);
        g.parent = Some(i as u32);
        rows.push(g);
    }
    let mut r = rect;
    r.id = fresh(DEPTH);
    r.parent = Some(DEPTH as u32);
    rows.push(r);
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let scene = Scene::from_rows(&rows);
            assert_eq!(scene.len(), DEPTH + 2);
            assert!(near(
                local_bounds(&scene, b),
                Rect::new(0.0, 0.0, 10.0, 10.0)
            ));
        })
        .unwrap()
        .join()
        .unwrap();
}
