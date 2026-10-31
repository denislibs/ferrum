//! Перо (подсистема 3, этап 3): точки щелчком, ручки протяжкой (зеркально, Alt —
//! разрыв), замыкание по первой точке, соединение с точкой другого контура,
//! Shift — 45°, Esc/Enter — конец пути одним шагом, Ctrl/Cmd+Z — последняя точка,
//! поворот и отражение родителя, перо в правке фигуры.

mod common;

use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;
use ferrum_scene::vector::fit;
use kurbo::Point;

/// Точки вектора в мировых координатах.
fn world_points(h: &mut H, id: NodeId) -> Vec<Point> {
    let n = h.ed.scene().node(id).unwrap().clone();
    let net = n.props.vector.as_deref().unwrap().clone();
    let m = n.world * fit(&n.props);
    net.points
        .iter()
        .map(|p| m * Point::new(p.x, p.y))
        .collect()
}

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-6
}

fn only_vector(h: &mut H) -> NodeId {
    let all = h.children(None);
    let v: Vec<NodeId> = all
        .into_iter()
        .filter(|id| h.props(*id).kind == NodeKind::Vector)
        .collect();
    assert_eq!(v.len(), 1, "один вектор");
    v[0]
}

#[test]
fn clicks_make_an_open_path_in_one_step() {
    let mut h = H::new();
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyP", Modifiers::NONE);
    assert_eq!(h.ed.tool_kind(), ToolKind::Pen);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 100.0, Modifiers::NONE);
    h.click(200.0, 180.0, Modifiers::NONE);
    // До конца пути в документе ничего нет: путь — черновик сцены.
    assert!(h.children(None).is_empty());
    h.key("Enter", Modifiers::NONE);
    let id = only_vector(&mut h);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert_eq!(h.sel(), vec![id]);
    let p = h.props(id);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 100.0, 80.0));
    let net = p.vector.as_deref().unwrap();
    assert_eq!(
        (net.points.len(), net.segments.len(), net.regions.len()),
        (3, 2, 0)
    );
    let w = world_points(&mut h, id);
    assert!(near(w[2], Point::new(200.0, 180.0)));
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn drag_pulls_mirrored_handles_and_alt_breaks_them() {
    let mut h = H::new();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    // Протяжка из (200, 100) на (+30, +40): исходящая ручка к курсору,
    // входящая — зеркально.
    h.drag((200.0, 100.0), (230.0, 140.0), Modifiers::NONE);
    h.click(300.0, 100.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    let id = only_vector(&mut h);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!(net.points[1].handle_mode, HandleMode::MirrorAll);
    assert_eq!(net.segments[0].tangent_end, [-30.0, -40.0]);
    assert_eq!(net.segments[1].tangent_start, [30.0, 40.0]);
    // Alt посреди протяжки: входящая ручка остаётся как была.
    let mut h = H::new();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.down(200.0, 100.0, Modifiers::NONE);
    h.mv(210.0, 120.0, Modifiers::NONE);
    h.mv(240.0, 100.0, Modifiers::ALT);
    h.up(240.0, 100.0, Modifiers::ALT);
    h.click(300.0, 100.0, Modifiers::NONE);
    h.key("Enter", Modifiers::NONE);
    let id = only_vector(&mut h);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!(net.points[1].handle_mode, HandleMode::Free);
    assert_eq!(net.segments[0].tangent_end, [-10.0, -20.0]);
    assert_eq!(net.segments[1].tangent_start, [40.0, 0.0]);
}

#[test]
fn click_on_the_first_point_closes_the_path_into_a_region() {
    let mut h = H::new();
    let steps = h.ed.step_count();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 100.0, Modifiers::NONE);
    h.click(150.0, 180.0, Modifiers::NONE);
    // Попадание в точку — в радиусе 6 px.
    h.click(103.0, 102.0, Modifiers::NONE);
    let id = only_vector(&mut h);
    assert_eq!(
        h.ed.step_count(),
        steps + 1,
        "замыкание — конец пути, один шаг"
    );
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!((net.points.len(), net.segments.len()), (3, 3));
    assert_eq!(net.regions.len(), 1);
    assert_eq!(net.regions[0].loops, vec![vec![0, 1, 2]]);
    assert!(net.validate().is_ok());
}

#[test]
fn shift_snaps_to_45_degrees_and_esc_drops_a_lone_point() {
    let mut h = H::new();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 130.0, Modifiers::SHIFT);
    h.key("Enter", Modifiers::NONE);
    let id = only_vector(&mut h);
    let w = world_points(&mut h, id);
    // 16,7° — к горизонтали; длина — проекция на неё.
    assert!(near(w[1], Point::new(200.0, 100.0)), "{w:?}");
    // Одна точка без сегментов — не вектор: ни узла, ни шага.
    let mut h = H::new();
    let steps = h.ed.step_count();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    assert!(h.children(None).is_empty());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    h.assert_scene_clean();
}

#[test]
fn undo_during_a_path_removes_its_last_point() {
    let mut h = H::new();
    let steps = h.ed.step_count();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 100.0, Modifiers::NONE);
    h.click(300.0, 100.0, Modifiers::NONE);
    h.key("KeyZ", ctrl());
    assert_eq!(h.ed.step_count(), steps);
    h.key("Enter", Modifiers::NONE);
    let id = only_vector(&mut h);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!((net.points.len(), net.segments.len()), (2, 1));
    assert_eq!(h.ed.step_count(), steps + 1);
}

#[test]
fn pen_honours_parent_rotation_and_flip() {
    let mut h = H::new();
    let f = h.frame(None, 50.0, 50.0, 600.0, 500.0);
    h.set(
        f,
        PropsPatch {
            rotation: Some(30.0),
            flip_x: Some(true),
            ..Default::default()
        },
    );
    let a = h.ed.scene().node(f).unwrap().world * Point::new(100.0, 100.0);
    let b = h.ed.scene().node(f).unwrap().world * Point::new(300.0, 200.0);
    h.ed.set_settings(Settings {
        pixel_snap: false,
        ..h.ed.settings().clone()
    });
    h.key("KeyP", Modifiers::NONE);
    h.click(a.x, a.y, Modifiers::NONE);
    h.click(b.x, b.y, Modifiers::NONE);
    h.key("Enter", Modifiers::NONE);
    let id = h.children(Some(f))[0];
    assert_eq!(h.parent(id), Some(f), "вектор — в фрейме под точкой");
    let w = world_points(&mut h, id);
    assert!(near(w[0], a) && near(w[1], b), "{w:?}");
}

#[test]
fn pen_in_edit_mode_joins_contours_and_adds_points_on_segments() {
    let mut h = H::new();
    // Два отдельных отрезка в одном векторе.
    let net = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(0.0, 100.0),
            VectorPoint::new(100.0, 100.0),
        ],
        segments: vec![VectorSegment::line(0, 1), VectorSegment::line(2, 3)],
        regions: vec![],
    };
    let id = h.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(net)),
            ..patch(100.0, 100.0, 100.0, 100.0)
        },
    );
    assert!(h.ed.start_vector_edit(id));
    let steps = h.ed.step_count();
    h.key("KeyP", Modifiers::NONE);
    assert_eq!(h.ed.vector_tool(), Some(VectorTool::Pen));
    // От конца первого отрезка — к концу второго: сеть соединилась.
    h.click(200.0, 100.0, Modifiers::NONE);
    h.click(200.0, 200.0, Modifiers::NONE);
    assert_eq!(h.ed.step_count(), steps + 1);
    assert!(h.ed.is_editing_vector(), "перо в правке остаётся в ней");
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!(net.segments.len(), 3);
    assert_eq!(net.segments[2], VectorSegment::line(1, 3));
    // Щелчок пером по сегменту без пути — новая точка на нём.
    h.click(150.0, 100.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!((net.points.len(), net.segments.len()), (5, 4));
    assert_eq!(net.points[4], VectorPoint::new(50.0, 0.0));
    assert_eq!(h.ed.step_count(), steps + 2);
}

#[test]
fn pen_on_a_rectangle_turns_it_into_a_vector_in_one_step() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    h.set(
        r,
        PropsPatch {
            name: Some("Кнопка".into()),
            ..Default::default()
        },
    );
    let before = h.state();
    let steps = h.ed.step_count();
    assert!(h.ed.start_vector_edit(r));
    h.key("KeyP", Modifiers::NONE);
    // Точка на верхней стороне.
    h.click(150.0, 100.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    let id = only_vector(&mut h);
    assert_eq!(id, r, "тот же узел стал вектором");
    let p = h.props(id);
    assert_eq!(p.name, "Кнопка");
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 200.0, 100.0));
    assert_eq!(p.fills, Props::defaults(NodeKind::Rect).fills);
    let net = p.vector.as_deref().unwrap();
    assert_eq!(net.points.len(), 5);
    assert_eq!(net.regions[0].loops[0].len(), 5);
    h.ed.end_vector_edit();
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn lone_point_in_edit_mode_leaves_a_shape_untouched() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    assert!(h.ed.start_vector_edit(r));
    h.key("KeyP", Modifiers::NONE);
    let before = h.state();
    let steps = h.ed.step_count();
    // Одна точка вне фигуры и Esc: ни шага, ни превращения.
    h.click(500.0, 400.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    assert_eq!(h.props(r).kind, NodeKind::Rect);
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    h.assert_scene_clean();
    // Точка и Ctrl+Z, затем Enter: путь пуст, записывать нечего.
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    assert!(h.ed.start_vector_edit(r));
    h.key("KeyP", Modifiers::NONE);
    let before = h.state();
    let steps = h.ed.step_count();
    h.click(500.0, 400.0, Modifiers::NONE);
    h.key("KeyZ", ctrl());
    h.key("Enter", Modifiers::NONE);
    assert_eq!(h.props(r).kind, NodeKind::Rect);
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn closing_drag_mirrors_both_handles_of_the_first_point() {
    let mut h = H::new();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 100.0, Modifiers::NONE);
    h.click(150.0, 180.0, Modifiers::NONE);
    // Замыкающая протяжка с первой точки.
    h.drag((100.0, 100.0), (120.0, 70.0), Modifiers::NONE);
    let id = only_vector(&mut h);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!(net.points[0].handle_mode, HandleMode::MirrorAll);
    let incoming = net.segments[2].tangent_end;
    let outgoing = net.segments[0].tangent_start;
    assert!((incoming[0] + outgoing[0]).abs() < 1e-9 && (incoming[1] + outgoing[1]).abs() < 1e-9);
    assert!((outgoing[0] - 20.0).abs() < 1e-9 && (outgoing[1] + 30.0).abs() < 1e-9);
    // Правка ручки позже не двигает вторую: она уже зеркальна.
    let moved = ferrum_editor::vector_ops::set_handle(
        &net,
        0,
        ferrum_editor::vector_ops::HandleEnd::Start,
        kurbo::Vec2::new(outgoing[0], outgoing[1]),
        false,
    );
    let jump = moved.segments[2].tangent_end;
    assert!((jump[0] - incoming[0]).abs() < 1e-9 && (jump[1] - incoming[1]).abs() < 1e-9);
}

#[test]
fn panel_edit_during_an_open_path_survives_the_commit() {
    // Вектор: панель двигает и растягивает узел посреди пути.
    let mut h = H::new();
    let net = VectorNetwork {
        points: vec![VectorPoint::new(0.0, 0.0), VectorPoint::new(100.0, 0.0)],
        segments: vec![VectorSegment::line(0, 1)],
        regions: vec![],
    };
    let id = h.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(net)),
            ..patch(100.0, 100.0, 100.0, 100.0)
        },
    );
    assert!(h.ed.start_vector_edit(id));
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(100.0, 200.0, Modifiers::NONE);
    h.set(
        id,
        PropsPatch {
            opacity: Some(0.5),
            ..Default::default()
        },
    );
    h.key("Escape", Modifiers::NONE);
    let p = h.props(id);
    assert_eq!(p.opacity, 0.5, "правка панели не затёрта");
    assert_eq!(
        p.vector.as_deref().unwrap().segments.len(),
        2,
        "путь записан"
    );
    // Фигура: заливка из панели переходит в новый вектор.
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    assert!(h.ed.start_vector_edit(r));
    h.key("KeyP", Modifiers::NONE);
    h.click(150.0, 100.0, Modifiers::NONE);
    h.set(
        r,
        PropsPatch {
            name: Some("Из панели".into()),
            opacity: Some(0.25),
            ..Default::default()
        },
    );
    h.key("Escape", Modifiers::NONE);
    let id = only_vector(&mut h);
    let p = h.props(id);
    assert_eq!((p.name.as_str(), p.opacity), ("Из панели", 0.25));
    assert_eq!(p.vector.as_deref().unwrap().points.len(), 5);
}

#[test]
fn shape_becomes_a_vector_in_place_keeping_id_and_order() {
    let mut h = H::new();
    let below = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    let above = h.rect(None, 0.0, 300.0, 10.0, 10.0);
    let before = h.state();
    let steps = h.ed.step_count();
    assert!(h.ed.start_vector_edit(r));
    h.key("KeyP", Modifiers::NONE);
    h.click(150.0, 100.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    assert_eq!(
        h.props(r).kind,
        NodeKind::Vector,
        "тот же узел стал вектором"
    );
    assert_eq!(h.children(None), vec![below, r, above]);
    assert_eq!(h.sel(), vec![r]);
    h.ed.end_vector_edit();
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
    h.ed.execute(CommandId::Undo);
    assert_eq!(
        h.props(r).kind,
        NodeKind::Rect,
        "после отмены снова прямоугольник"
    );
}
