//! Перемещение, копия с Alt, перенос во фрейм, ресайз, поворот, привязки (разделы
//! 7.1–7.3 спеки). Каждый жест — ровно один шаг истории.

mod common;

use common::*;
use ferrum_doc::{NodeKind, PropsPatch};
use ferrum_editor::*;
use ferrum_render::Camera;
use kurbo::Point;

#[test]
fn drag_moves_after_threshold_in_one_step() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let before = h.state();
    let steps = h.ed.step_count();
    // 2 px — это клик, не перетаскивание.
    h.down(120.0, 120.0, none());
    h.mv(122.0, 120.0, none());
    h.up(122.0, 120.0, none());
    assert_eq!(h.sel(), vec![r]);
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    h.drag_hold((120.0, 120.0), (170.0, 140.0), none());
    // Во время перетаскивания меняется только превью сцены.
    assert_eq!(h.props(r).x, 100.0);
    assert_eq!(h.ed.scene().node(r).unwrap().props.x, 150.0);
    h.up(170.0, 140.0, none());
    let p = h.props(r);
    assert_eq!((p.x, p.y), (150.0, 120.0));
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn shift_locks_axis_and_escape_cancels() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.drag((120.0, 120.0), (190.0, 135.0), shift());
    assert_eq!((h.props(r).x, h.props(r).y), (170.0, 100.0));
    h.drag((190.0, 120.0), (200.0, 60.0), shift());
    assert_eq!((h.props(r).x, h.props(r).y), (170.0, 40.0));
    // Esc посреди перетаскивания: превью убрано, истории нет.
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag_hold((190.0, 60.0), (400.0, 400.0), none());
    h.key("Escape", none());
    h.up(400.0, 400.0, none());
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    h.assert_scene_clean();
}

#[test]
fn alt_drag_duplicates_in_one_step() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 600.0, 600.0);
    let g = h.add(Some(f), NodeKind::Group, PropsPatch::default());
    let a = h.rect(Some(g), 100.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    assert_eq!(h.sel(), vec![g]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag_hold((120.0, 120.0), (220.0, 120.0), alt());
    assert_eq!(h.ed.cursor(), Cursor::Duplicate);
    // Оригинал на месте, двигается черновик-копия, он и выделен в оверлее.
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 100.0);
    let shown = h.ed.overlay().selection;
    assert!(shown.iter().all(|id| h.ed.scene().is_draft(*id)));
    h.up(220.0, 120.0, alt());
    let kids = h.children(Some(f));
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0], g, "копия над оригиналом");
    let copy = kids[1];
    assert_eq!(h.sel(), vec![copy]);
    assert_eq!(h.props(a).x, 100.0);
    let inner = h.children(Some(copy));
    assert_eq!(inner.len(), 1);
    // Сдвигается копия группы, её ребёнок — на своём месте внутри.
    assert_eq!(h.props(copy).x, 100.0);
    assert_eq!(h.props(inner[0]).x, 100.0);
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn releasing_alt_mid_drag_moves_originals() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.drag_hold((120.0, 120.0), (220.0, 120.0), alt());
    assert!(h.ed.scene().has_preview());
    h.ev(Input::KeyUp(KeyEvent::new("AltLeft", none())));
    assert_eq!(h.ed.cursor(), Cursor::Default);
    h.up(220.0, 120.0, none());
    assert_eq!(h.children(None), vec![r]);
    assert_eq!(h.props(r).x, 200.0);
    h.assert_scene_clean();
}

#[test]
fn dragging_into_and_out_of_frames_reparents() {
    let mut h = H::new();
    let f = h.frame(None, 300.0, 100.0, 400.0, 400.0);
    h.set(
        f,
        PropsPatch {
            rotation: Some(90.0),
            ..Default::default()
        },
    );
    // Фрейм повёрнут на 90°: его мир — x ∈ [300, 700], y ∈ [-300, 100].
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag((110.0, 110.0), (510.0, -90.0), none());
    assert_eq!(h.parent(r), Some(f), "во фрейм под курсором");
    // На экране на месте: левый верхний угол (500, -100).
    let n = h.ed.scene().node(r).unwrap();
    let corner = n.world * Point::ZERO;
    assert!(
        (corner - Point::new(500.0, -100.0)).hypot() < 1e-9,
        "{corner:?}"
    );
    assert_eq!(h.props(r).rotation, 270.0, "поворот компенсирует фрейм");
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
    // Наружу — снова в корень.
    h.drag((510.0, -90.0), (110.0, 610.0), none());
    assert_eq!(h.parent(r), None);
    let p = h.props(r);
    assert!(
        close(p.x, 100.0) && close(p.y, 600.0) && p.rotation == 0.0,
        "{p:?}"
    );
    // Узел внутри группы из группы не выходит.
    let g = h.add(None, NodeKind::Group, PropsPatch::default());
    let c = h.rect(Some(g), 800.0, 600.0, 20.0, 20.0);
    h.click(810.0, 610.0, ctrl());
    assert_eq!(h.sel(), vec![c]);
    h.drag((810.0, 610.0), (510.0, -90.0), none());
    assert_eq!(h.parent(c), Some(g));
}

#[test]
fn resize_handles_modifiers_and_rotated_nodes() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    h.click(150.0, 125.0, none());
    let before = h.state();
    let steps = h.ed.step_count();
    // Правый нижний угол.
    h.hover(200.0, 150.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { .. }));
    h.drag((200.0, 150.0), (260.0, 170.0), none());
    let p = h.props(r);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 160.0, 70.0));
    h.assert_one_step(steps, &before);
    // Shift — пропорции; Alt — от центра.
    h.drag((260.0, 170.0), (420.0, 180.0), shift());
    let p = h.props(r);
    assert!(close(p.w / p.h, 160.0 / 70.0), "{p:?}");
    assert_eq!((p.x, p.y), (100.0, 100.0));
    let c = (p.x + p.w / 2.0, p.y + p.h / 2.0);
    h.drag(
        (p.x + p.w, p.y + p.h / 2.0),
        (p.x + p.w + 20.0, p.y + p.h / 2.0),
        alt(),
    );
    let q = h.props(r);
    assert_eq!(q.w, p.w + 40.0, "от центра — обе стороны");
    assert_eq!((q.x + q.w / 2.0, q.y + q.h / 2.0), c);
    // Левый край: правая сторона стоит.
    let right = q.x + q.w;
    h.drag((q.x, q.y + q.h / 2.0), (q.x - 30.0, q.y + 5.0), none());
    let s = h.props(r);
    assert_eq!((s.x + s.w, s.h), (right, q.h));
    assert_eq!(s.x, q.x - 30.0);
    // Повёрнутый узел меняет размер в своих осях.
    let t = h.rect(None, 500.0, 500.0, 100.0, 40.0);
    h.set(
        t,
        PropsPatch {
            rotation: Some(90.0),
            ..Default::default()
        },
    );
    // Повёрнут на 90° против часовой: мир — x ∈ [500, 540], y ∈ [400, 500].
    h.click(520.0, 450.0, none());
    assert_eq!(h.sel(), vec![t]);
    // Ручка «справа» узла теперь сверху (500 + 20, 400).
    h.drag((520.0, 400.0), (520.0, 350.0), none());
    let p = h.props(t);
    assert_eq!((p.w, p.h, p.rotation), (150.0, 40.0, 90.0));
    assert_eq!((p.x, p.y), (500.0, 500.0), "левая сторона на месте");
    h.assert_scene_clean();
}

#[test]
fn group_resize_scales_children_in_one_transaction() {
    let mut h = H::new();
    let g = h.add(None, NodeKind::Group, PropsPatch::default());
    let a = h.rect(Some(g), 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(Some(g), 150.0, 150.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    assert_eq!(h.sel(), vec![g]);
    let before = h.state();
    let steps = h.ed.step_count();
    // Группа 100..200 тянется за правый нижний угол до 300: масштаб 2.
    h.drag((200.0, 200.0), (300.0, 300.0), none());
    let (pa, pb) = (h.props(a), h.props(b));
    assert_eq!((pa.x, pa.y, pa.w, pa.h), (100.0, 100.0, 100.0, 100.0));
    assert_eq!((pb.x, pb.y, pb.w, pb.h), (200.0, 200.0, 100.0, 100.0));
    let pg = h.props(g);
    assert_eq!((pg.x, pg.y), (0.0, 0.0), "группа сама не сдвинулась");
    h.assert_one_step(steps, &before);
}

#[test]
fn multi_selection_resizes_and_rotates_as_one_box() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 300.0, 100.0, 100.0, 100.0);
    h.click(150.0, 150.0, none());
    h.click(350.0, 150.0, shift());
    // Общая рамка 100..400 × 100..200; левый край ведём к 0 — масштаб 4/3.
    h.drag((100.0, 150.0), (0.0, 150.0), none());
    let (pa, pb) = (h.props(a), h.props(b));
    assert_eq!((pa.x, pa.w), (0.0, 133.0), "пиксельная привязка округляет");
    assert_eq!(pb.x + pb.w, 400.0);
    // Поворот общей рамки на 90° за зону у угла.
    let before = h.state();
    let steps = h.ed.step_count();
    let (pa, pb) = (h.props(a), h.props(b));
    let center = Point::new((pa.x + pb.x + pb.w) / 2.0, 150.0);
    // Зона поворота — снаружи правого нижнего угла рамки.
    let start = Point::new(pb.x + pb.w + 8.0, 208.0);
    h.hover(start.x, start.y);
    assert!(
        matches!(h.ed.cursor(), Cursor::Rotate { .. }),
        "{:?}",
        h.ed.cursor()
    );
    let r = (start - center).hypot();
    let a0 = (start - center).atan2();
    let a1 = a0 + std::f64::consts::FRAC_PI_2;
    let end = center + kurbo::Vec2::new(a1.cos(), a1.sin()) * r;
    h.drag((start.x, start.y), (end.x, end.y), shift());
    assert_eq!(h.props(a).rotation, 270.0, "по часовой на экране");
    assert_eq!(h.props(b).rotation, 270.0);
    h.assert_one_step(steps, &before);
}

#[test]
fn rotation_keeps_center_and_shift_snaps_to_15() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    h.click(150.0, 125.0, none());
    let center = Point::new(150.0, 125.0);
    let start = Point::new(208.0, 158.0);
    let a0 = (start - center).atan2();
    // Против часовой на экране на ~37° с Shift — до 30°.
    let a1 = a0 - 37f64.to_radians();
    let end = center + kurbo::Vec2::new(a1.cos(), a1.sin()) * (start - center).hypot();
    h.drag((start.x, start.y), (end.x, end.y), shift());
    let p = h.props(r);
    assert_eq!(p.rotation, 30.0);
    let n = h.ed.scene().node(r).unwrap();
    let c = n.world * Point::new(50.0, 25.0);
    assert!((c - center).hypot() < 1.0, "центр на месте: {c:?}");
    h.assert_scene_clean();
}

#[test]
fn line_endpoints_drag_with_45_degree_steps() {
    let mut h = H::new();
    let l = h.add(None, NodeKind::Line, patch(100.0, 100.0, 100.0, 0.0));
    h.click(150.0, 100.0, none());
    assert_eq!(h.sel(), vec![l]);
    h.drag((200.0, 100.0), (300.0, 100.0), none());
    let p = h.props(l);
    assert_eq!((p.x, p.y, p.w, p.rotation), (100.0, 100.0, 200.0, 0.0));
    // Начало ведём вниз от неподвижного конца (300, 100): с Shift — строго вертикально.
    h.drag((100.0, 100.0), (290.0, 290.0), shift());
    let p = h.props(l);
    let len = (10.0f64 * 10.0 + 190.0 * 190.0).sqrt();
    assert!(close(p.x, 300.0) && close(p.y, 100.0 + len), "{p:?}");
    assert!(close(p.w, len));
    assert_eq!(
        p.rotation, 90.0,
        "от начала к концу — вверх, против часовой"
    );
}

#[test]
fn snapping_to_sibling_edges_with_screen_threshold() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 300.0, 300.0, 50.0, 50.0);
    h.click(320.0, 320.0, none());
    // Левый край b (300) ведём к 203: до правого края a (200) — 3 px.
    h.drag_hold((320.0, 320.0), (223.0, 320.0), none());
    assert_eq!(
        h.ed.scene().node(b).unwrap().props.x,
        200.0,
        "привязка к краю"
    );
    let lines = h.ed.overlay().snap_lines;
    assert!(
        lines.iter().any(|l| l.p0.x == 200.0 && l.p1.x == 200.0),
        "{lines:?}"
    );
    h.up(223.0, 320.0, none());
    assert_eq!(h.props(b).x, 200.0);
    assert!(h.ed.overlay().snap_lines.is_empty());
    // 6 px — дальше порога.
    h.drag((220.0, 320.0), (226.0, 320.0), none());
    assert_eq!(h.props(b).x, 206.0);
    // Порог в экранных пикселях: при зуме 2 те же 6 px экрана — 3 px мира, привязка есть.
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_zoom(2.0);
    cam.set_center(Point::new(250.0, 250.0));
    h.ed.set_camera(cam);
    let s = |p: Point| h.ed.camera().world_to_screen(p);
    let from = s(Point::new(230.0, 320.0));
    let to = s(Point::new(226.0, 320.0));
    h.drag((from.x, from.y), (to.x, to.y), none());
    assert_eq!(h.props(b).x, 200.0);
    let _ = a;
}

#[test]
fn snapping_to_parent_frame_and_centers() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    let r = h.rect(Some(f), 50.0, 50.0, 40.0, 40.0);
    h.click(60.0, 60.0, none());
    // Центр r (70) ведём к 198: центр фрейма — 200.
    h.drag((60.0, 60.0), (188.0, 60.0), none());
    assert_eq!(h.props(r).x, 180.0);
    // Края соседей вне видимой полосы не мешают: далёкий узел другого родителя.
    let other = h.rect(None, 2.0, 900.0, 10.0, 10.0);
    h.drag((190.0, 60.0), (0.0 + 13.0, 60.0), none());
    assert_eq!(h.props(r).x, 0.0, "левый край фрейма");
    let _ = other;
}

#[test]
fn pixel_snapping_rounds_and_can_be_disabled() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_zoom(4.0);
    cam.set_center(Point::new(125.0, 125.0));
    h.ed.set_camera(cam);
    let s = |h: &H, p: Point| h.ed.camera().world_to_screen(p);
    let (a, b) = (
        s(&h, Point::new(110.0, 110.0)),
        s(&h, Point::new(130.3, 110.6)),
    );
    h.drag((a.x, a.y), (b.x, b.y), none());
    assert_eq!((h.props(r).x, h.props(r).y), (120.0, 101.0));
    let mut settings = h.ed.settings().clone();
    settings.pixel_snap = false;
    settings.snapping = false;
    h.ed.set_settings(settings);
    let (a, b) = (
        s(&h, Point::new(130.0, 110.0)),
        s(&h, Point::new(140.3, 120.6)),
    );
    h.drag((a.x, a.y), (b.x, b.y), none());
    let p = h.props(r);
    assert!(close(p.x, 130.3) && close(p.y, 111.6), "{p:?}");
}

#[test]
fn locked_nodes_do_not_move() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    h.set(
        r,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(h.sel(), vec![r], "выделение заблокированного остаётся");
    let steps = h.ed.step_count();
    h.drag((120.0, 120.0), (300.0, 300.0), none());
    assert_eq!(h.props(r).x, 100.0);
    assert_eq!(h.ed.step_count(), steps);
}

/// Ручка, доведённая ровно до противоположной стороны, раньше давала `w = 0`, и
/// узел больше не растягивался: масштаб нулевой стороны — всегда ноль. Сторона не
/// схлопывается до нуля (минимум `MIN_SIDE`, как 0.01 в Figma), и её можно
/// растянуть обратно — и по ширине, и по высоте.
#[test]
fn collapsed_side_can_be_widened_again() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    h.click(200.0, 150.0, none());
    assert_eq!(h.sel(), vec![r]);
    // Правая ручка — ровно на левую сторону.
    h.drag((300.0, 150.0), (100.0, 150.0), none());
    let p = h.props(r);
    assert!(p.w > 0.0 && p.w <= 0.01 + 1e-12, "{p:?}");
    assert_eq!((p.x, p.h), (100.0, 100.0));
    h.assert_scene_clean();
    // И обратно: правая ручка (она у левой стороны) — на 250 px вправо.
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag((100.0 + p.w, 150.0), (350.0, 150.0), none());
    let p = h.props(r);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 250.0, 100.0));
    assert!(!p.flip_x);
    h.assert_one_step(steps, &before);
    // То же по высоте: верхняя ручка на нижнюю сторону и обратно вверх.
    h.drag((225.0, 100.0), (225.0, 200.0), none());
    let p = h.props(r);
    assert!(p.h > 0.0 && p.h <= 0.01 + 1e-12, "{p:?}");
    assert_eq!(p.y, 200.0);
    h.drag((225.0, 200.0), (225.0, 120.0), none());
    let p = h.props(r);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 120.0, 250.0, 80.0));
    assert!(!p.flip_y);
    h.assert_scene_clean();
}

/// Esc отменяет жест по физической клавише (раздел 7.1), даже если команду
/// «Отмена» переназначили, а клавишу Escape отдали другой команде.
#[test]
fn escape_cancels_gesture_even_when_rebound() {
    let mut h = H::new();
    h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.keymap_mut()
        .set_bindings(CommandId::Escape, vec![Shortcut::parse("KeyQ").unwrap()]);
    h.ed.keymap_mut().set_bindings(
        CommandId::ToolRect,
        vec![Shortcut::parse("Escape").unwrap()],
    );
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag_hold((120.0, 120.0), (300.0, 300.0), none());
    assert!(h.ed.is_busy() && h.ed.scene().has_preview());
    h.ev(Input::KeyDown(KeyEvent::new("Escape", none())));
    assert!(!h.ed.is_busy(), "Esc отменил жест");
    assert_eq!(
        h.ed.tool_kind(),
        ToolKind::Move,
        "команда клавиши не выполнена"
    );
    h.up(300.0, 300.0, none());
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    h.assert_scene_clean();
}

/// Узел с уже нулевой стороной (из `transact` или из вставки: буфер обмена
/// приводит отрицательную ширину к нулю) растягивается первым же жестом: база
/// ресайза получает сторону `MIN_SIDE`, и масштаб идёт от неё сразу к цели.
/// Раньше первый жест записывал w = 0.01 лишним шагом, и тянул только второй.
#[test]
fn zero_side_node_widens_in_one_drag() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 0.0, 100.0);
    assert_eq!(h.props(a).w, 0.0);
    // Вторая — из вставки: копия, удаление оригинала, вставка на то же место.
    h.ed.select(&[a]);
    h.ed.take_events();
    h.ed.execute(CommandId::Copy);
    let bytes =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::Clipboard(b) => Some(b),
                _ => None,
            })
            .expect("байты буфера");
    let b = h.rect(None, 600.0, 100.0, 0.0, 100.0);
    h.ed.select(&[a]);
    h.ed.execute(CommandId::Delete);
    h.ed.paste(&bytes).unwrap();
    let pasted = h.sel()[0];
    assert_eq!((h.props(pasted).x, h.props(pasted).w), (100.0, 0.0));
    for (id, x0) in [(pasted, 100.0), (b, 600.0)] {
        h.ed.select(&[id]);
        let before = h.state();
        let steps = h.ed.step_count();
        h.drag((x0, 150.0), (x0 + 250.0, 150.0), none());
        let p = h.props(id);
        assert_eq!((p.x, p.y, p.w, p.h), (x0, 100.0, 250.0, 100.0), "{p:?}");
        assert!(!p.flip_x && !p.flip_y);
        h.assert_one_step(steps, &before);
    }
    // По высоте: верхняя ручка нулевой высоты вверх — одним жестом.
    let c = h.rect(None, 100.0, 400.0, 200.0, 0.0);
    h.ed.select(&[c]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag((200.0, 400.0), (200.0, 320.0), none());
    let p = h.props(c);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 320.0, 200.0, 80.0), "{p:?}");
    assert!(!p.flip_x && !p.flip_y);
    h.assert_one_step(steps, &before);
    // Линия в выделении остаётся линией: её высота не становится `MIN_SIDE`.
    let d = h.rect(None, 100.0, 600.0, 0.0, 100.0);
    let line = h.add(
        None,
        ferrum_doc::NodeKind::Line,
        patch(300.0, 650.0, 100.0, 0.0),
    );
    h.ed.select(&[d, line]);
    // Общая рамка (100, 600)–(400, 700): правая ручка на 100 px вправо.
    h.drag((400.0, 650.0), (500.0, 650.0), none());
    assert_eq!(h.props(line).h, 0.0);
    assert!(h.props(d).w > 0.0, "{:?}", h.props(d));
    h.assert_scene_clean();
}
