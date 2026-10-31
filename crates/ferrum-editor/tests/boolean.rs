//! Булевы операции в редакторе (Alt+Shift+U/S/I/E) (подсистема 3, этап 4): команды на выделение и
//! смена операции, стиль и имя новой группы, фреймы и одиночный слой, дети из
//! разных родителей, правка ребёнка с живым результатом, выделение щелчком и
//! двойным щелчком, разгруппировка.

mod common;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;
use kurbo::{Point, Rect};

/// Alt+Shift — булевы операции, как в Figma (Option+Shift на macOS).
fn alt_shift() -> Modifiers {
    Modifiers {
        alt: true,
        shift: true,
        ..Modifiers::NONE
    }
}

fn solid(r: u8, g: u8, b: u8) -> Vec<Fill> {
    vec![Fill::solid(Color::rgba(r, g, b, 255))]
}

fn errors(h: &mut H) -> Vec<String> {
    h.ed.take_events()
        .into_iter()
        .filter_map(|e| match e {
            EditorEvent::Error(m) => Some(m),
            _ => None,
        })
        .collect()
}

/// Красный прямоугольник 100..200 и синий 150..250 по x (y 100..200).
fn two_rects(h: &mut H) -> (NodeId, NodeId) {
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 150.0, 100.0, 100.0, 100.0);
    h.set(
        a,
        PropsPatch {
            fills: Some(solid(255, 0, 0)),
            ..Default::default()
        },
    );
    h.set(
        b,
        PropsPatch {
            fills: Some(solid(0, 0, 255)),
            name: Some("Синий".into()),
            ..Default::default()
        },
    );
    (a, b)
}

fn world_box(h: &mut H, id: NodeId) -> Rect {
    let n = h.ed.scene().node(id).unwrap();
    n.world.transform_rect_bbox(n.local_bounds.unwrap())
}

fn near(a: Rect, b: Rect) -> bool {
    [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
        .iter()
        .all(|d| d.abs() < 0.01)
}

#[test]
fn union_wraps_the_selection_in_one_step_with_the_top_style() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    let before_a = world_box(&mut h, a);
    h.ed.select(&[a, b]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyU", alt_shift());
    let roots = h.children(None);
    assert_eq!(roots.len(), 1);
    let g = roots[0];
    let p = h.props(g);
    assert_eq!(p.kind, NodeKind::Boolean);
    assert_eq!(p.boolean_op, Some(BooleanOp::Union));
    assert_eq!(p.name, "Объединение");
    assert_eq!(p.fills, solid(0, 0, 255), "стиль верхнего слоя");
    assert_eq!(h.children(Some(g)), vec![a, b]);
    assert_eq!(h.sel(), vec![g]);
    assert!(near(world_box(&mut h, a), before_a), "дети на месте");
    assert!(near(
        world_box(&mut h, g),
        Rect::new(100.0, 100.0, 250.0, 200.0)
    ));
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn subtract_takes_the_bottom_style_and_draws_the_difference() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    h.ed.select(&[b, a]);
    h.key("KeyS", alt_shift());
    let g = h.children(None)[0];
    let p = h.props(g);
    assert_eq!(
        (p.boolean_op, p.name.as_str()),
        (Some(BooleanOp::Subtract), "Вычитание")
    );
    assert_eq!(p.fills, solid(255, 0, 0), "стиль нижнего слоя");
    assert!(near(
        world_box(&mut h, g),
        Rect::new(100.0, 100.0, 150.0, 200.0)
    ));
    // Щелчок по результату выделяет группу, по вычтенной части — ничего.
    h.ed.select(&[]);
    h.click(120.0, 150.0, Modifiers::NONE);
    assert_eq!(h.sel(), vec![g]);
    h.click(220.0, 150.0, Modifiers::NONE);
    assert!(h.sel().is_empty());
}

#[test]
fn selected_booleans_switch_their_op_and_default_name() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    h.ed.select(&[a, b]);
    h.key("KeyU", alt_shift());
    let g = h.children(None)[0];
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyI", alt_shift());
    let p = h.props(g);
    assert_eq!(
        (p.boolean_op, p.name.as_str()),
        (Some(BooleanOp::Intersect), "Пересечение")
    );
    assert_eq!(h.children(Some(g)), vec![a, b], "не обёрнута заново");
    assert!(near(
        world_box(&mut h, g),
        Rect::new(150.0, 100.0, 200.0, 200.0)
    ));
    h.assert_one_step(steps, &before);
    // Своё имя остаётся; та же операция — без шага.
    h.set(
        g,
        PropsPatch {
            name: Some("Мой знак".into()),
            ..Default::default()
        },
    );
    h.key("KeyE", alt_shift());
    assert_eq!(h.props(g).name, "Мой знак");
    assert_eq!(h.props(g).boolean_op, Some(BooleanOp::Exclude));
    let steps = h.ed.step_count();
    h.key("KeyE", alt_shift());
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn single_layers_and_frames_are_refused_with_a_message() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    let f = h.frame(None, 300.0, 0.0, 100.0, 100.0);
    let b = h.rect(None, 100.0, 0.0, 50.0, 50.0);
    let steps = h.ed.step_count();
    h.ed.select(&[a]);
    h.ed.take_events();
    h.key("KeyU", alt_shift());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(
        errors(&mut h),
        vec!["Для булевой операции выделите хотя бы два слоя"]
    );
    h.ed.select(&[a, f]);
    h.key("KeyU", alt_shift());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(
        errors(&mut h),
        vec!["Булевы операции не применяются к фреймам"]
    );
    // Фрейм пропускается, остальные — в группу.
    h.ed.select(&[a, f, b]);
    h.key("KeyU", alt_shift());
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(errors(&mut h), vec!["Фреймы в булеву группу не входят"]);
    let g = h.sel()[0];
    assert_eq!(h.children(Some(g)), vec![a, b]);
    assert_eq!(h.parent(f), None);
}

#[test]
fn children_from_a_rotated_group_keep_their_place() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let grp = h.add(None, NodeKind::Group, patch(400.0, 100.0, 0.0, 0.0));
    let b = h.rect(Some(grp), 0.0, 0.0, 60.0, 40.0);
    h.set(
        grp,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    let corner = h.ed.scene().node(b).unwrap().world * Point::new(60.0, 40.0);
    h.ed.select(&[a, b]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    assert_eq!(h.parent(b), Some(g));
    let now = h.ed.scene().node(b).unwrap().world * Point::new(60.0, 40.0);
    assert!((now - corner).hypot() < 1e-6, "{now:?} ≠ {corner:?}");
    h.assert_scene_clean();
}

#[test]
fn dragging_a_child_updates_the_result_live_and_is_one_step() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    h.ed.select(&[a, b]);
    h.key("KeyI", alt_shift());
    let g = h.sel()[0];
    // Двойной щелчок — внутрь группы, к ребёнку под курсором.
    h.ed.select(&[]);
    h.click(175.0, 150.0, Modifiers::NONE);
    assert_eq!(h.sel(), vec![g]);
    h.dbl(175.0, 150.0);
    assert_eq!(h.sel(), vec![b]);
    let before = h.state();
    let steps = h.ed.step_count();
    // Синий уходит влево на 30: пересечение 150..200 → 120..200 уже в превью.
    h.drag_hold((175.0, 150.0), (145.0, 150.0), Modifiers::NONE);
    assert!(
        near(world_box(&mut h, g), Rect::new(120.0, 100.0, 200.0, 200.0)),
        "{:?}",
        world_box(&mut h, g)
    );
    assert_eq!(h.props(b).x, 150.0, "документ ещё не тронут");
    h.up(145.0, 150.0, Modifiers::NONE);
    assert!(near(
        world_box(&mut h, g),
        Rect::new(120.0, 100.0, 200.0, 200.0)
    ));
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn ungroup_returns_the_children_and_resize_scales_them() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    h.ed.select(&[a, b]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    // Ресайз группы за правый край: дети растягиваются, как в группе.
    h.ed.execute(CommandId::Escape);
    h.ed.select(&[g]);
    h.drag((250.0, 150.0), (400.0, 150.0), Modifiers::NONE);
    assert!(near(
        world_box(&mut h, g),
        Rect::new(100.0, 100.0, 400.0, 200.0)
    ));
    assert!((h.props(a).w - 200.0).abs() < 1e-6, "{}", h.props(a).w);
    h.ed.select(&[g]);
    h.key(
        "KeyG",
        Modifiers {
            shift: true,
            ..ctrl()
        },
    );
    assert_eq!(h.children(None), vec![a, b]);
    assert_eq!(h.sel(), vec![a, b]);
    h.assert_scene_clean();
}

#[test]
fn mixed_selections_with_booleans_and_frames() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    h.ed.select(&[a, b]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    let f = h.frame(None, 400.0, 0.0, 100.0, 100.0);
    // [булева группа, фрейм] — операция группы меняется, фрейм не мешает.
    h.ed.select(&[g, f]);
    h.ed.take_events();
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyI", alt_shift());
    assert_eq!(h.props(g).boolean_op, Some(BooleanOp::Intersect));
    assert!(errors(&mut h).is_empty());
    h.assert_one_step(steps, &before);
    // [булева группа, прямоугольник] — оборачиваются заново в новую булеву группу.
    let r = h.rect(None, 0.0, 300.0, 50.0, 50.0);
    h.ed.select(&[g, r]);
    h.key("KeyE", alt_shift());
    let outer = h.sel()[0];
    assert_ne!(outer, g);
    assert_eq!(h.props(outer).boolean_op, Some(BooleanOp::Exclude));
    assert_eq!(h.children(Some(outer)), vec![g, r]);
    assert_eq!(h.props(g).boolean_op, Some(BooleanOp::Intersect));
}

#[test]
fn style_comes_from_the_leaf_of_a_group_or_a_nested_boolean() {
    let mut h = H::new();
    let low = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    h.set(
        low,
        PropsPatch {
            fills: Some(solid(1, 2, 3)),
            ..Default::default()
        },
    );
    let grp = h.add(None, NodeKind::Group, patch(0.0, 0.0, 0.0, 0.0));
    let in_a = h.rect(Some(grp), 100.0, 0.0, 50.0, 50.0);
    let in_b = h.rect(Some(grp), 120.0, 0.0, 50.0, 50.0);
    h.set(
        in_a,
        PropsPatch {
            fills: Some(solid(10, 0, 0)),
            ..Default::default()
        },
    );
    h.set(
        in_b,
        PropsPatch {
            fills: Some(solid(0, 10, 0)),
            ..Default::default()
        },
    );
    // Группа сверху: стиль её верхнего листа; у «Вычитания» — нижнего слоя.
    h.ed.select(&[low, grp]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    assert_eq!(h.props(g).fills, solid(0, 10, 0));
    // Группа снизу при «Вычитании»: её нижний лист.
    h.ed.execute(CommandId::Undo);
    let above = h.rect(None, 200.0, 0.0, 50.0, 50.0);
    h.ed.select(&[grp, above]);
    h.key("KeyS", alt_shift());
    let g = h.sel()[0];
    assert_eq!(h.props(g).fills, solid(10, 0, 0));
    // Вложенная булева группа берётся как есть, со своими заливками.
    let inner = g;
    let top = h.rect(None, 300.0, 0.0, 50.0, 50.0);
    h.ed.select(&[inner, top]);
    h.key("KeyS", alt_shift());
    let outer = h.sel()[0];
    assert_eq!(h.props(outer).fills, solid(10, 0, 0));
}

#[test]
fn ungrouping_a_boolean_is_one_undo_step() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    h.ed.select(&[a, b]);
    h.key("KeyU", alt_shift());
    let before = h.state();
    let steps = h.ed.step_count();
    h.key(
        "KeyG",
        Modifiers {
            shift: true,
            ..ctrl()
        },
    );
    assert_eq!(h.children(None), vec![a, b]);
    h.assert_one_step(steps, &before);
}

#[test]
fn boolean_over_other_parents_is_placed_like_a_group() {
    // Те же слои, обёрнутые Ctrl+G и булевой операцией, встают в одни координаты.
    let place = |boolean: bool| {
        let mut h = H::new();
        let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
        let grp = h.add(None, NodeKind::Group, patch(400.0, 100.0, 0.0, 0.0));
        let b = h.rect(Some(grp), 0.0, 0.0, 60.0, 40.0);
        h.set(
            grp,
            PropsPatch {
                rotation: Some(30.0),
                ..Default::default()
            },
        );
        h.ed.select(&[a, b]);
        if boolean {
            h.key("KeyU", alt_shift());
        } else {
            h.key("KeyG", ctrl());
        }
        let c = h.sel()[0];
        let par = h.parent(c);
        let sibs = h.children(par);
        let idx = sibs.iter().position(|x| *x == c).unwrap();
        let pb = h.props(b);
        (idx, pb.x, pb.y, pb.rotation, h.children(Some(c)).len())
    };
    assert_eq!(place(true), place(false));
}
