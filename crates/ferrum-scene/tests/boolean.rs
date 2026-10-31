//! Булевы операции (подсистема 3, этап 4): операция над контурами — свойства и
//! точность на касаниях и общих рёбрах; результат булевой группы в сцене —
//! границы, попадание, пересчёт при правке и превью детей, вложенность,
//! скрытые дети, пустой результат, текст через контуры глифов.

use std::rc::Rc;
use std::sync::Arc;

use ferrum_doc::{BooleanOp, Document, FillRule, NodeId, NodeKind, PageId, Props, PropsPatch};
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
