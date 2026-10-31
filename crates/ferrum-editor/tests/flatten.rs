//! «Сплющить», «Стрелка» и стиль обводки из панели (подсистема 3, этап 3).

mod common;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;
use ferrum_scene::vector::fit;
use kurbo::Point;

fn world_points(h: &mut H, id: NodeId) -> Vec<Point> {
    let n = h.ed.scene().node(id).unwrap().clone();
    let net = n.props.vector.as_deref().unwrap().clone();
    let m = n.world * fit(&n.props);
    net.points
        .iter()
        .map(|p| m * Point::new(p.x, p.y))
        .collect()
}

fn has(points: &[Point], p: Point) -> bool {
    points.iter().any(|q| (*q - p).hypot() < 1e-6)
}

#[test]
fn several_shapes_become_one_vector_in_one_step() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    let b = h.rect(None, 300.0, 100.0, 50.0, 50.0);
    h.set(
        a,
        PropsPatch {
            fills: Some(vec![Fill::solid(Color::rgba(255, 0, 0, 255))]),
            ..Default::default()
        },
    );
    h.set(
        b,
        PropsPatch {
            rotation: Some(90.0),
            name: Some("Верхний".into()),
            ..Default::default()
        },
    );
    let corner_b = h.ed.scene().node(b).unwrap().world * Point::new(50.0, 50.0);
    h.ed.select(&[a, b]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyE", ctrl());
    let all = h.children(None);
    assert_eq!(all.len(), 1);
    let v = all[0];
    assert_eq!(h.sel(), vec![v]);
    let p = h.props(v);
    assert_eq!(p.kind, NodeKind::Vector);
    assert_eq!(p.name, "Верхний", "свойства верхнего узла");
    assert_eq!(p.rotation, 0.0);
    let net = p.vector.as_deref().unwrap();
    assert_eq!((net.points.len(), net.regions.len()), (8, 2));
    // Нижний сохранил свою заливку в своей области; верхний — заливки узла.
    assert_eq!(
        net.regions[0].fills,
        Some(vec![Fill::solid(Color::rgba(255, 0, 0, 255))])
    );
    assert_eq!(net.regions[1].fills, None);
    let w = world_points(&mut h, v);
    assert!(
        has(&w, Point::new(100.0, 100.0)) && has(&w, corner_b),
        "{w:?}"
    );
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn one_shape_keeps_its_pose_and_groups_flatten_whole() {
    let mut h = H::new();
    let e = h.add(None, NodeKind::Ellipse, patch(100.0, 100.0, 80.0, 40.0));
    h.set(
        e,
        PropsPatch {
            rotation: Some(30.0),
            flip_y: Some(true),
            ..Default::default()
        },
    );
    h.ed.select(&[e]);
    h.key("KeyE", ctrl());
    let v = h.sel()[0];
    assert_eq!(v, e, "тот же узел на том же месте");
    assert_eq!(h.children(None), vec![e]);
    let p = h.props(v);
    assert_eq!(
        (p.kind, p.rotation, p.flip_y),
        (NodeKind::Vector, 30.0, true)
    );
    assert_eq!((p.w, p.h), (80.0, 40.0));
    // Группа из прямоугольника и линии — вектор, группы нет.
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let l = h.add(None, NodeKind::Line, patch(20.0, 5.0, 30.0, 0.0));
    h.ed.select(&[r, l]);
    h.key("KeyG", ctrl());
    h.key("KeyE", ctrl());
    let all = h.children(None);
    assert_eq!(all.len(), 1);
    assert_eq!(h.props(all[0]).kind, NodeKind::Vector);
    let net = h.props(all[0]).vector.as_deref().unwrap().clone();
    assert_eq!((net.points.len(), net.segments.len()), (6, 5));
}

#[test]
fn nothing_to_flatten_is_an_error_without_a_step() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 100.0, 100.0);
    h.ed.select(&[f]);
    let steps = h.ed.step_count();
    h.ed.take_events();
    h.key("KeyE", ctrl());
    assert_eq!(h.ed.step_count(), steps);
    assert!(h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::Error(m) if m.starts_with("Нечего сплющить"))));
}

#[test]
fn arrow_tool_draws_a_line_with_arrow_lines_at_its_end() {
    let mut h = H::new();
    h.key("KeyL", Modifiers::SHIFT);
    assert_eq!(h.ed.tool_kind(), ToolKind::Arrow);
    h.drag((100.0, 100.0), (300.0, 100.0), Modifiers::NONE);
    let id = h.sel()[0];
    let p = h.props(id);
    assert_eq!((p.kind, p.w), (NodeKind::Line, 200.0));
    assert_eq!(p.stroke_style.cap_end, StrokeCap::ArrowLines);
    assert_eq!(p.stroke_style.cap_start, StrokeCap::None);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
}

#[test]
fn stroke_style_from_the_panel_is_one_step_and_skips_groups() {
    let mut h = H::new();
    let l = h.add(None, NodeKind::Line, patch(0.0, 0.0, 100.0, 0.0));
    let r = h.rect(None, 200.0, 0.0, 50.0, 50.0);
    h.ed.select(&[l, r]);
    let before = h.state();
    let steps = h.ed.step_count();
    let edit = PanelEdit::StrokeStyle {
        cap_start: Some(StrokeCap::Round),
        cap_end: None,
        join: Some(StrokeJoin::Bevel),
        miter_limit: Some(10.0),
    };
    h.ed.edit_selection(&edit, Phase::Preview);
    assert_eq!(
        h.props(l).stroke_style.cap_start,
        StrokeCap::None,
        "превью — только в сцене"
    );
    h.ed.edit_selection(&edit, Phase::Commit);
    let s = h.props(l).stroke_style;
    assert_eq!(
        (s.cap_start, s.cap_end, s.join, s.miter_limit),
        (StrokeCap::Round, StrokeCap::None, StrokeJoin::Bevel, 10.0)
    );
    assert_eq!(h.props(r).stroke_style.join, StrokeJoin::Bevel);
    h.assert_one_step(steps, &before);
}
