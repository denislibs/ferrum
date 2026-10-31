//! «Сплющить» булевой группы и текста, «Контур обводки», текст в булевой
//! группе по контурам глифов (подсистема 3, этап 4).

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::vector_ops::path_network;
use ferrum_editor::*;
use ferrum_scene::vector::fit;
use kurbo::{BezPath, Circle, Point, Rect, Shape};

/// «Контур обводки» — Ctrl/Cmd+Alt+O.
fn ctrl_alt() -> Modifiers {
    Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::NONE
    }
}

/// Булевы операции — Alt+Shift+буква, как в Figma.
fn alt_shift() -> Modifiers {
    Modifiers {
        alt: true,
        shift: true,
        ..Modifiers::NONE
    }
}

fn world_points(h: &mut H, id: NodeId) -> Vec<Point> {
    let n = h.ed.scene().node(id).unwrap().clone();
    let net = n.props.vector.as_deref().unwrap().clone();
    let m = n.world * fit(&n.props);
    net.points
        .iter()
        .map(|p| m * Point::new(p.x, p.y))
        .collect()
}

fn world_box(h: &mut H, id: NodeId) -> Rect {
    let n = h.ed.scene().node(id).unwrap();
    n.world.transform_rect_bbox(n.local_bounds.unwrap())
}

fn near(a: Rect, b: Rect, tol: f64) -> bool {
    [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
        .iter()
        .all(|d| d.abs() < tol)
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

#[test]
fn path_network_keeps_curves_and_closes_loops() {
    let mut p = BezPath::new();
    p.move_to((0.0, 0.0));
    p.line_to((10.0, 0.0));
    p.quad_to((10.0, 10.0), (0.0, 10.0));
    p.close_path();
    // Подпуть, замкнутый своей точкой: замыкающий отрезок не повторяется.
    p.move_to((20.0, 0.0));
    p.line_to((30.0, 0.0));
    p.line_to((30.0, 10.0));
    p.line_to((20.0, 0.0));
    p.close_path();
    // Открытый подпуть — без области.
    p.move_to((50.0, 0.0));
    p.curve_to((60.0, 0.0), (60.0, 10.0), (50.0, 10.0));
    let net = path_network(&p, FillRule::EvenOdd);
    assert_eq!(net.validate(), Ok(()));
    assert_eq!(
        (net.points.len(), net.segments.len()),
        (3 + 3 + 2, 3 + 3 + 1)
    );
    assert_eq!(net.regions.len(), 1);
    assert_eq!(net.regions[0].rule, FillRule::EvenOdd);
    assert_eq!(net.regions[0].loops, vec![vec![0, 1, 2], vec![3, 4, 5]]);
    assert!(!net.segments[1].is_straight(), "квадратичная — кубической");
    // Круг: четыре кривые, одна петля.
    let c = path_network(
        &Circle::new((0.0, 0.0), 5.0).to_path(1e-3),
        FillRule::NonZero,
    );
    assert_eq!(c.validate(), Ok(()));
    assert_eq!(c.points.len(), c.segments.len());
    assert!(c.segments.iter().all(|s| !s.is_straight()));
}

#[test]
fn flatten_boolean_gives_its_result_in_place_in_one_step() {
    let mut h = H::new();
    let below = h.rect(None, 0.0, 300.0, 10.0, 10.0);
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 150.0, 100.0, 100.0, 100.0);
    let above = h.rect(None, 0.0, 400.0, 10.0, 10.0);
    h.ed.select(&[a, b]);
    h.key("KeyS", alt_shift());
    let g = h.sel()[0];
    h.set(
        g,
        PropsPatch {
            name: Some("Знак".into()),
            ..Default::default()
        },
    );
    let fills = h.props(g).fills;
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyE", ctrl());
    let roots = h.children(None);
    assert_eq!(roots.len(), 3, "дети ушли вместе с группой");
    assert_eq!((roots[0], roots[2]), (below, above), "на месте группы");
    let v = roots[1];
    assert_eq!(h.sel(), vec![v]);
    let p = h.props(v);
    assert_eq!((p.kind, p.name.as_str()), (NodeKind::Vector, "Знак"));
    assert_eq!(p.fills, fills);
    assert!(p.boolean_op.is_none());
    let w = world_points(&mut h, v);
    assert_eq!(w.len(), 4, "{w:?}");
    for c in [
        (100.0, 100.0),
        (150.0, 100.0),
        (150.0, 200.0),
        (100.0, 200.0),
    ] {
        assert!(
            w.iter().any(|q| (*q - Point::from(c)).hypot() < 1e-6),
            "{c:?}"
        );
    }
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn text_flattens_and_booleans_use_glyph_outlines() {
    let mut h = text_h();
    let t = add_text(&mut h, 100.0, 100.0, "Ok", 40.0);
    let tb = world_box(&mut h, t);
    // Объединение текста и прямоугольника справа: результат — глифы, а не рамка
    // текста (между буквами пусто).
    let r = h.rect(None, tb.x1 + 50.0, tb.y0, 20.0, 20.0);
    h.ed.select(&[t, r]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    let n = h.ed.scene().node(g).unwrap();
    let area = n.boolean.as_ref().unwrap().contours.to_path().area().abs();
    assert!(area > 400.0 + 100.0, "глифы в результате: {area}");
    assert!(
        area < 400.0 + 0.8 * tb.area(),
        "не рамка текста: {area} при рамке {}",
        tb.area()
    );
    h.ed.execute(CommandId::Undo);
    // Текст сплющивается в контуры глифов на своём месте.
    h.ed.select(&[t]);
    h.ed.take_events();
    h.key("KeyE", ctrl());
    assert!(errors(&mut h).is_empty());
    let v = h.sel()[0];
    let p = h.props(v);
    assert_eq!(p.kind, NodeKind::Vector);
    assert!(p.text.is_none());
    let net = p.vector.as_deref().unwrap();
    // «O» — две петли (внешняя и внутренняя), «k» — одна.
    assert!(
        net.regions[0].loops.len() >= 3,
        "{}",
        net.regions[0].loops.len()
    );
    let vb = world_box(&mut h, v);
    assert!(tb.contains(vb.center()) && vb.width() <= tb.width() + 1.0);
}

#[test]
fn text_without_fonts_is_not_flattened() {
    let mut h = H::new();
    let t = h.add(None, NodeKind::Text, patch(0.0, 0.0, 50.0, 20.0));
    h.ed.select(&[t]);
    h.ed.take_events();
    let steps = h.ed.step_count();
    h.key("KeyE", ctrl());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(
        errors(&mut h),
        vec!["Шрифты ещё загружаются — текст не сплющен"]
    );
    assert_eq!(h.props(t).kind, NodeKind::Text);
}

#[test]
fn outline_stroke_of_a_filled_rect_adds_a_ring_vector_above() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    let red = Color::rgba(255, 0, 0, 255);
    h.set(
        r,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: red,
                width: 10.0,
                align: StrokeAlign::Inside,
            }]),
            ..Default::default()
        },
    );
    h.ed.select(&[r]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyO", ctrl_alt());
    let roots = h.children(None);
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0], r, "слой с заливкой остаётся под контуром");
    assert!(h.props(r).strokes.is_empty());
    let v = roots[1];
    assert_eq!(h.sel(), vec![v]);
    let p = h.props(v);
    assert_eq!(p.kind, NodeKind::Vector);
    assert!(p.fills.is_empty() && p.strokes.is_empty());
    assert_eq!(p.name, "Прямоугольник");
    let net = p.vector.as_deref().unwrap();
    assert_eq!(net.regions.len(), 1);
    assert_eq!(net.regions[0].fills, Some(vec![Fill::solid(red)]));
    assert_eq!(net.regions[0].rule, FillRule::EvenOdd);
    // Внутренняя обводка — внутри рамки.
    assert!(near(
        world_box(&mut h, v),
        Rect::new(100.0, 100.0, 200.0, 150.0),
        1e-6
    ));
    // Кольцо: щелчок в середину — мимо контура, по краю — в него.
    h.ed.select(&[]);
    h.click(105.0, 125.0, Modifiers::NONE);
    assert_eq!(h.sel(), vec![v]);
    h.click(150.0, 125.0, Modifiers::NONE);
    assert_eq!(h.sel(), vec![r]);
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn outline_stroke_of_an_open_path_keeps_curves_caps_and_removes_the_layer() {
    let mut h = H::new();
    let l = h.add(None, NodeKind::Line, patch(100.0, 100.0, 200.0, 0.0));
    h.set(
        l,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 4.0,
                align: StrokeAlign::Center,
            }]),
            stroke_cap_end: Some(StrokeCap::Round),
            ..Default::default()
        },
    );
    h.ed.select(&[l]);
    h.key("KeyO", ctrl_alt());
    let roots = h.children(None);
    assert_eq!(roots.len(), 1, "линия без заливок — заменена контуром");
    let v = roots[0];
    let p = h.props(v);
    let net = p.vector.as_deref().unwrap();
    assert!(
        net.segments.iter().any(|s| !s.is_straight()),
        "круглое окончание — кривые"
    );
    let b = world_box(&mut h, v);
    assert!(near(b, Rect::new(100.0, 98.0, 302.0, 102.0), 1e-3), "{b:?}");
}

#[test]
fn outline_stroke_without_strokes_is_a_message() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[r]);
    h.ed.take_events();
    let steps = h.ed.step_count();
    h.key("KeyO", ctrl_alt());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(
        errors(&mut h),
        vec!["У выделенных слоёв нет видимых обводок"]
    );
}

#[test]
fn outline_stroke_of_a_boolean_outlines_its_result() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 150.0, 150.0, 100.0, 100.0);
    h.set(
        b,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 2.0,
                align: StrokeAlign::Outside,
            }]),
            ..Default::default()
        },
    );
    h.ed.select(&[a, b]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    h.key("KeyO", ctrl_alt());
    let v = *h.children(None).last().unwrap();
    assert_ne!(v, g);
    assert_eq!(h.props(v).kind, NodeKind::Vector);
    // Снаружи результата: рамка шире на толщину, внутри объединения пусто.
    assert!(near(
        world_box(&mut h, v),
        Rect::new(98.0, 98.0, 252.0, 252.0),
        0.01
    ));
    let n = h.ed.scene().node(v).unwrap();
    let inside = n.world.inverse() * Point::new(160.0, 160.0);
    assert!(!ferrum_scene::shape::hit_shape(
        &n.props,
        n.path_shape().map(|s| &**s),
        inside,
        0.0
    ));
}

#[test]
fn text_missed_while_the_font_registry_is_busy_is_recomputed_on_the_next_frame() {
    let mut h = text_h();
    let t = add_text(&mut h, 100.0, 100.0, "Ok", 40.0);
    let r = h.rect(None, 400.0, 100.0, 20.0, 20.0);
    h.ed.select(&[t, r]);
    // Реестр занят, пока сцена считает группу (как посреди раскладки):
    // текст формы не даёт.
    let fonts = h.ed.fonts().clone();
    let busy = fonts.borrow_mut();
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    let area = |h: &H| {
        h.ed.scene()
            .node(g)
            .unwrap()
            .boolean
            .as_ref()
            .unwrap()
            .contours
            .to_path()
            .area()
            .abs()
    };
    assert!((area(&h) - 400.0).abs() < 1e-6, "пока только прямоугольник");
    drop(busy);
    // Шрифты не менялись, но следующий кадр пересчитывает пропущенное.
    h.hover(10.0, 10.0);
    assert!(area(&h) > 500.0, "глифы в результате: {}", area(&h));
}
