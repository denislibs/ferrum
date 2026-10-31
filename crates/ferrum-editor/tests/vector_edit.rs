//! Режим правки вектора (подсистема 3, этап 3): вход двойным щелчком и Enter,
//! выход Esc и щелчком по другому узлу, выделение точек щелчком, Shift и рамкой,
//! перемещение с привязками, ручки, изгиб, удаление с сохранением формы и с
//! разрывом, «Заливка области», фигура становится вектором при первой правке,
//! отмена в правке, повёрнутый и отражённый узел.

mod common;

use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;
use ferrum_scene::vector::{fit, segment_curve};
use kurbo::{ParamCurve, Point};

/// Квадрат 100 × 100 в (100, 100): четыре точки по часовой от левого верхнего.
fn square(h: &mut H, closed_region: bool) -> NodeId {
    let net = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(100.0, 100.0),
            VectorPoint::new(0.0, 100.0),
        ],
        segments: (0..4)
            .map(|i| VectorSegment::line(i, (i + 1) % 4))
            .collect(),
        regions: if closed_region {
            vec![VectorRegion {
                rule: FillRule::NonZero,
                loops: vec![vec![0, 1, 2, 3]],
                fills: None,
            }]
        } else {
            vec![]
        },
    };
    h.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(net)),
            ..patch(100.0, 100.0, 100.0, 100.0)
        },
    )
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

fn net_of(h: &mut H, id: NodeId) -> VectorNetwork {
    h.props(id).vector.as_deref().unwrap().clone()
}

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-6
}

fn points(h: &H) -> Vec<u32> {
    h.ed.vector_selection()
        .unwrap()
        .points
        .iter()
        .copied()
        .collect()
}

#[test]
fn double_click_and_enter_enter_edit_mode_esc_leaves_it() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.dbl(150.0, 100.0);
    assert!(h.ed.is_editing_vector());
    assert_eq!(h.ed.vector_edit_node(), Some(id));
    h.key("Escape", Modifiers::NONE);
    assert!(!h.ed.is_editing_vector());
    assert_eq!(h.sel(), vec![id]);
    h.key("Enter", Modifiers::NONE);
    assert!(h.ed.is_editing_vector());
    h.key("Enter", Modifiers::NONE);
    assert!(!h.ed.is_editing_vector());
    // Заблокированный — нет.
    h.set(
        id,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    h.ed.select(&[id]);
    h.key("Enter", Modifiers::NONE);
    assert!(!h.ed.is_editing_vector());
}

#[test]
fn drag_point_is_one_step_with_pixel_and_point_snapping() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    let before = h.state();
    let steps = h.ed.step_count();
    // Правая нижняя (200, 200) → (230.4, 240.6): пиксельная сетка округляет.
    h.drag((200.0, 200.0), (230.4, 240.6), Modifiers::NONE);
    assert_eq!(points(&h), vec![2]);
    let w = world_points(&mut h, id);
    assert!(near(w[2], Point::new(230.0, 241.0)), "{w:?}");
    assert!(near(w[0], Point::new(100.0, 100.0)), "остальные на месте");
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
    // К точке сети ближе 4 px по x — привязка к ней, а не к сетке.
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    h.drag((200.0, 200.0), (102.5, 250.0), Modifiers::NONE);
    let w = world_points(&mut h, id);
    assert!(near(w[2], Point::new(100.0, 250.0)), "{w:?}");
}

#[test]
fn shift_click_and_marquee_select_points() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 100.0, Modifiers::SHIFT);
    assert_eq!(points(&h), vec![0, 1]);
    h.click(100.0, 100.0, Modifiers::SHIFT);
    assert_eq!(points(&h), vec![1]);
    // Рамка с пустого места внутри квадрата захватывает нижние точки.
    h.drag((90.0, 150.0), (210.0, 210.0), Modifiers::NONE);
    assert_eq!(points(&h), vec![2, 3]);
    // Стрелка сдвигает выделенные точки на пиксель (шаг отмены).
    let steps = h.ed.step_count();
    h.key("ArrowDown", Modifiers::NONE);
    assert_eq!(h.ed.step_count(), steps + 1);
    let w = world_points(&mut h, id);
    assert!(near(w[3], Point::new(100.0, 201.0)), "{w:?}");
}

#[test]
fn handles_mirror_and_ctrl_drag_bends_a_segment() {
    let mut h = H::new();
    let e = h.add(None, NodeKind::Ellipse, patch(100.0, 100.0, 200.0, 200.0));
    h.ed.start_vector_edit(e);
    // Правая точка (300, 200); её ручки — вверх и вниз на 200·κ/2.
    h.click(300.0, 200.0, Modifiers::NONE);
    let k = 100.0 * 0.552_284_749_830_793_4;
    h.drag((300.0, 200.0 + k), (330.0, 200.0 + k), Modifiers::NONE);
    let id = h.ed.vector_edit_node().unwrap();
    assert_eq!(id, e, "эллипс стал вектором на месте (тот же узел)");
    assert_eq!(h.props(id).kind, NodeKind::Vector);
    let net = net_of(&mut h, id);
    // Ручки у точки 1: выходящая к курсору, входящая — зеркально той же длины.
    let out = net.segments[1].tangent_start;
    let inc = net.segments[0].tangent_end;
    assert!(
        (out[0] - 30.0).abs() < 1e-6 && (out[1] - k).abs() < 1e-6,
        "{out:?}"
    );
    assert!(
        (inc[0] + 30.0).abs() < 1e-6 && (inc[1] + k).abs() < 1e-6,
        "{inc:?}"
    );
    // Изгиб: точка нижнего отрезка квадрата уходит под курсор.
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    h.drag((150.0, 200.0), (150.0, 240.0), ctrl());
    let net = net_of(&mut h, id);
    let n = h.ed.scene().node(id).unwrap().clone();
    let m = n.world * fit(&n.props);
    let mid = m * segment_curve(&net, &net.segments[2]).eval(0.5);
    assert!(near(mid, Point::new(150.0, 240.0)), "{mid:?}");
}

#[test]
fn delete_keeps_shape_shift_delete_breaks_and_empty_vector_goes() {
    let mut h = H::new();
    let id = square(&mut h, true);
    h.ed.start_vector_edit(id);
    h.click(200.0, 100.0, Modifiers::NONE);
    h.key("Delete", Modifiers::NONE);
    let net = net_of(&mut h, id);
    assert_eq!((net.points.len(), net.segments.len()), (3, 3));
    assert_eq!(net.regions.len(), 1, "треугольник остался замкнутым");
    // С разрывом: сегменты точки уходят, область — тоже.
    h.click(100.0, 100.0, Modifiers::NONE);
    h.key("Delete", Modifiers::SHIFT);
    let net = net_of(&mut h, id);
    assert_eq!((net.points.len(), net.segments.len()), (2, 1));
    assert!(net.regions.is_empty());
    // Всё выделено и удалено — узла нет, правки нет.
    h.key("KeyA", ctrl());
    h.key("Delete", Modifiers::NONE);
    assert!(h.children(None).is_empty());
    assert!(!h.ed.is_editing_vector());
}

#[test]
fn paint_bucket_toggles_a_closed_area() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    h.key("KeyB", Modifiers::NONE);
    assert_eq!(h.ed.vector_tool(), Some(VectorTool::Bucket));
    let steps = h.ed.step_count();
    h.click(150.0, 150.0, Modifiers::NONE);
    let net = net_of(&mut h, id);
    assert_eq!(net.regions.len(), 1);
    assert_eq!(
        net.regions[0].fills,
        Some(vec![Fill::solid(Color::SHAPE_GRAY)])
    );
    assert_eq!(h.ed.step_count(), steps + 1);
    h.click(150.0, 150.0, Modifiers::NONE);
    assert!(net_of(&mut h, id).regions.is_empty());
    // Мимо замкнутой области — ничего.
    h.click(400.0, 400.0, Modifiers::NONE);
    assert_eq!(h.ed.step_count(), steps + 2);
    h.key("Escape", Modifiers::NONE);
    assert_eq!(h.ed.vector_tool(), Some(VectorTool::Select));
}

#[test]
fn shape_becomes_a_vector_only_when_edited() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let steps = h.ed.step_count();
    h.dbl(150.0, 150.0);
    assert!(h.ed.is_editing_vector());
    h.key("Escape", Modifiers::NONE);
    assert_eq!(
        h.props(r).kind,
        NodeKind::Rect,
        "без правки — прямоугольник"
    );
    assert_eq!(h.ed.step_count(), steps);
    let before = h.state();
    h.key("Enter", Modifiers::NONE);
    // Превью до отпускания: прямоугольник ещё в документе.
    h.drag_hold((200.0, 200.0), (220.0, 230.0), Modifiers::NONE);
    assert_eq!(h.props(r).kind, NodeKind::Rect);
    h.up(220.0, 230.0, Modifiers::NONE);
    let v = h.ed.vector_edit_node().unwrap();
    assert_eq!(h.props(v).kind, NodeKind::Vector);
    let w = world_points(&mut h, v);
    assert!(near(w[2], Point::new(220.0, 230.0)));
    h.ed.end_vector_edit();
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn click_on_another_node_leaves_edit_mode_and_selects_it() {
    let mut h = H::new();
    let id = square(&mut h, false);
    let other = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.start_vector_edit(id);
    h.click(420.0, 420.0, Modifiers::NONE);
    assert!(!h.ed.is_editing_vector());
    assert_eq!(h.sel(), vec![other]);
}

#[test]
fn undo_in_edit_mode_restores_the_network() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    h.drag((200.0, 200.0), (250.0, 250.0), Modifiers::NONE);
    h.key("KeyZ", ctrl());
    assert!(h.ed.is_editing_vector());
    let work = h.ed.vector_work().unwrap().clone();
    assert_eq!(work.points[2], VectorPoint::new(100.0, 100.0));
    h.key("KeyY", ctrl());
    assert_eq!(
        h.ed.vector_work().unwrap().points[2],
        VectorPoint::new(150.0, 150.0)
    );
}

#[test]
fn rotated_and_flipped_vector_moves_points_under_the_cursor() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.set(
        id,
        PropsPatch {
            rotation: Some(30.0),
            flip_x: Some(true),
            ..Default::default()
        },
    );
    h.ed.set_settings(Settings {
        pixel_snap: false,
        snapping: false,
        ..h.ed.settings().clone()
    });
    let w = world_points(&mut h, id);
    h.ed.start_vector_edit(id);
    let target = Point::new(w[2].x + 25.0, w[2].y - 10.0);
    h.drag((w[2].x, w[2].y), (target.x, target.y), Modifiers::NONE);
    let after = world_points(&mut h, id);
    assert!(near(after[2], target), "{after:?}");
    assert!(near(after[0], w[0]), "остальные на месте");
    let p = h.props(id);
    assert!(
        p.flip_x && (p.rotation - 30.0).abs() < 1e-9,
        "поза узла сохранена"
    );
}

#[test]
fn selection_never_points_past_the_network_after_the_pen_drops_a_point() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    assert!(h.ed.start_vector_edit(r));
    h.key("KeyP", Modifiers::NONE);
    // Одинокая точка получает номер 4 и выделяется; путь заканчивает Esc.
    h.click(400.0, 400.0, Modifiers::NONE);
    let during = h.ed.vector_selection().unwrap().points.clone();
    assert!(during.iter().all(|&i| i < 5), "{during:?}");
    h.key("Escape", Modifiers::NONE);
    assert!(h.ed.is_editing_vector());
    assert_eq!(h.ed.vector_tool(), Some(VectorTool::Select));
    let sel = h.ed.vector_selection().unwrap();
    assert!(sel.points.iter().all(|&i| i < 4), "{sel:?}");
    // Ничего не выделено: сдвиг и удаление не пишут и не падают.
    let steps = h.ed.step_count();
    h.key("ArrowDown", Modifiers::NONE);
    h.key("Delete", Modifiers::NONE);
    h.key("Delete", Modifiers::SHIFT);
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(h.props(r).kind, NodeKind::Rect);
    // Точки сети (4 у прямоугольника) выделяются и двигаются как обычно.
    h.click(200.0, 200.0, Modifiers::NONE);
    assert_eq!(points(&h), vec![2]);
    h.key("ArrowDown", Modifiers::NONE);
    assert_eq!(h.ed.step_count(), steps + 1);
}

#[test]
fn layout_commands_during_an_open_pen_path_finish_it_first() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    assert!(h.ed.start_vector_edit(r));
    h.key("KeyP", Modifiers::NONE);
    h.click(400.0, 400.0, Modifiers::NONE);
    h.click(500.0, 400.0, Modifiers::NONE);
    // Дублирование не теряет и не затирает открытый путь.
    h.key("KeyD", ctrl());
    assert!(!h.ed.is_editing_vector());
    let net = net_of(&mut h, r);
    assert_eq!((net.points.len(), net.segments.len()), (6, 5));
    assert_eq!(h.children(None).len(), 2, "оригинал и копия");
    h.assert_scene_clean();
}

#[test]
fn double_click_sets_the_edit_vector_outbox_and_enters_edit_mode() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.dbl(150.0, 100.0);
    assert_eq!(h.ed.vector_edit_node(), Some(id));
    assert_eq!(h.sel(), vec![id]);
}

#[test]
fn lost_pointer_up_does_not_leak_into_the_next_edit() {
    let mut h = H::new();
    // Прямоугольник, ещё не ставший вектором: у вектора resync и так восстанавливает сеть.
    let id = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    h.ed.start_vector_edit(id);
    // Протяжка точки без отпускания, затем щелчок по другой точке и сдвиг.
    h.drag_hold((200.0, 200.0), (250.0, 250.0), Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    assert!(!h.ed.scene().has_preview());
    assert_eq!(
        h.ed.vector_work().unwrap().points[2],
        VectorPoint::new(100.0, 100.0)
    );
    h.key("ArrowDown", Modifiers::NONE);
    let w = world_points(&mut h, id);
    assert!(near(w[2], Point::new(200.0, 200.0)), "{w:?}");
    assert!(near(w[0], Point::new(100.0, 101.0)), "{w:?}");
}

#[test]
fn delete_and_nudge_finish_an_open_pen_path_as_its_own_step() {
    // Путь — один шаг, сдвиг — свой.
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    assert!(h.ed.start_vector_edit(r));
    h.key("KeyP", Modifiers::NONE);
    let steps = h.ed.step_count();
    h.click(400.0, 400.0, Modifiers::NONE);
    h.click(500.0, 400.0, Modifiers::NONE);
    h.key("ArrowDown", Modifiers::NONE);
    assert_eq!(h.ed.step_count(), steps + 2, "путь и сдвиг");
    h.click(600.0, 400.0, Modifiers::NONE);
    h.click(700.0, 400.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    assert_eq!(h.ed.step_count(), steps + 3);
    // Пустой холст, перо с панели: узел появляется от записи пути, не от сдвига.
    let mut h = H::new();
    h.key("KeyP", Modifiers::NONE);
    let steps = h.ed.step_count();
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 100.0, Modifiers::NONE);
    h.key("ArrowDown", Modifiers::NONE);
    assert_eq!(
        h.ed.step_count(),
        steps + 2,
        "путь создал узел, сдвиг — свой шаг"
    );
    assert_eq!(h.children(None).len(), 1);
}

#[test]
fn delete_in_pen_mode_does_not_connect_to_a_renumbered_point() {
    let mut h = H::new();
    let id = h.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(VectorNetwork {
                points: (0..4)
                    .map(|i| VectorPoint::new(i as f64 * 50.0, 0.0))
                    .collect(),
                segments: (0..3).map(|i| VectorSegment::line(i, i + 1)).collect(),
                regions: vec![],
            })),
            ..patch(100.0, 100.0, 150.0, 1.0)
        },
    );
    h.ed.start_vector_edit(id);
    h.key("KeyP", Modifiers::NONE);
    // Продолжить от точки 0, удалить её, поставить две новые точки.
    h.click(100.0, 100.0, Modifiers::NONE);
    h.key("Delete", Modifiers::NONE);
    h.click(400.0, 400.0, Modifiers::NONE);
    h.click(500.0, 400.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    let net = net_of(&mut h, id);
    let n = net.points.len() as u32;
    // Путь начат заново: сегмент только между двумя новыми точками.
    assert_eq!(net.segments.len(), 3, "{:?}", net.segments);
    let last = net.segments.last().unwrap();
    assert_eq!((last.start, last.end), (n - 2, n - 1), "{:?}", net.segments);
}

#[test]
fn repeated_escape_does_nothing_in_edit_mode() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    assert!(h.ed.start_vector_edit(r));
    h.key("KeyP", Modifiers::NONE);
    let mut rep = KeyEvent::new("Escape", Modifiers::NONE);
    rep.repeat = true;
    h.ev(Input::KeyDown(rep.clone()));
    h.ev(Input::KeyDown(rep.clone()));
    assert_eq!(h.ed.vector_tool(), Some(VectorTool::Pen), "повторы пусты");
    h.key("Escape", Modifiers::NONE);
    assert_eq!(h.ed.vector_tool(), Some(VectorTool::Select));
    h.ev(Input::KeyDown(rep));
    assert!(h.ed.is_editing_vector(), "повтор не выходит из режима");
}

#[test]
fn bucket_mode_leaves_edit_on_a_press_at_another_node() {
    let mut h = H::new();
    let id = square(&mut h, false);
    let other = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.start_vector_edit(id);
    h.key("KeyB", Modifiers::NONE);
    h.click(420.0, 420.0, Modifiers::NONE);
    assert!(!h.ed.is_editing_vector());
    assert_eq!(h.sel(), vec![other]);
}
