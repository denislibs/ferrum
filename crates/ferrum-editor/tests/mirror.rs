//! Отражение, как в Figma: ресайз за противоположную сторону и команды «Отразить по
//! горизонтали / по вертикали» (Shift+H, Shift+V). Рамка узла остаётся
//! положительной, отражение хранится флагами `flip_x`/`flip_y`; каждый жест и
//! команда — один шаг истории.

mod common;

use common::*;
use ferrum_doc::{NodeId, NodeKind, PropsPatch};
use ferrum_editor::geometry::{rect_map, SelectionBox};
use ferrum_editor::*;
use kurbo::{Affine, Point, Rect};

fn world(h: &H, id: NodeId) -> Affine {
    h.ed.scene().node(id).unwrap().world
}

#[track_caller]
fn assert_affine(a: Affine, b: Affine) {
    let (a, b) = (a.as_coeffs(), b.as_coeffs());
    assert!(
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6),
        "{a:?} != {b:?}"
    );
}

/// Экранная (= мировая при зуме 1) точка ручки выделения по долям рамки.
fn handle(h: &H, fx: f64, fy: f64) -> Point {
    SelectionBox::of(h.ed.scene(), h.ed.selection())
        .unwrap()
        .point(fx, fy)
}

fn flips(h: &mut H, id: NodeId) -> (bool, bool) {
    let p = h.props(id);
    (p.flip_x, p.flip_y)
}

/// Прямоугольник 100×50 в (100, 100), выделенный.
fn one_rect() -> (H, NodeId) {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    h.ed.select(&[r]);
    (h, r)
}

#[test]
fn resize_past_right_side_mirrors_in_one_step() {
    let (mut h, r) = one_rect();
    let before = h.state();
    let steps = h.ed.step_count();
    // Правая ручка (200, 125) за левую сторону, в 70.
    h.drag_hold((200.0, 125.0), (70.0, 125.0), none());
    // Во время жеста — только превью: отражение уже видно в сцене.
    let shown = h.ed.scene().node(r).unwrap().props.clone();
    assert!(shown.flip_x && !shown.flip_y);
    assert_eq!((shown.x, shown.w), (70.0, 30.0));
    assert_eq!(h.state(), before, "документ не тронут до отпускания");
    h.up(70.0, 125.0, none());
    let p = h.props(r);
    assert!(p.flip_x && !p.flip_y);
    assert_eq!(
        (p.x, p.y, p.w, p.h, p.rotation),
        (70.0, 100.0, 30.0, 50.0, 0.0)
    );
    // Ручка под курсором: левая видимая сторона рамки там, где отпустили.
    assert_eq!(
        h.ed.scene().node(r).unwrap().world_bounds.unwrap(),
        Rect::new(70.0, 100.0, 100.0, 150.0)
    );
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn resize_past_top_mirrors_vertically() {
    let (mut h, r) = one_rect();
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag((150.0, 100.0), (150.0, 180.0), none());
    let p = h.props(r);
    assert!(!p.flip_x && p.flip_y);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 150.0, 100.0, 30.0));
    h.assert_one_step(steps, &before);
}

#[test]
fn crossing_and_coming_back_leaves_no_flip() {
    let (mut h, r) = one_rect();
    h.down(200.0, 125.0, none());
    h.mv(150.0, 125.0, none());
    h.mv(70.0, 125.0, none());
    assert!(h.ed.scene().node(r).unwrap().props.flip_x);
    h.mv(150.0, 125.0, none());
    h.mv(250.0, 125.0, none());
    assert!(!h.ed.scene().node(r).unwrap().props.flip_x);
    h.up(250.0, 125.0, none());
    let p = h.props(r);
    assert!(!p.flip_x && !p.flip_y);
    assert_eq!((p.x, p.w), (100.0, 150.0));

    // Второй ресайз туда и обратно уже отражённого узла снова снимает отражение.
    h.drag((250.0, 125.0), (40.0, 125.0), none());
    assert_eq!(flips(&mut h, r), (true, false));
    let p = h.props(r);
    assert_eq!((p.x, p.w), (40.0, 60.0));
    // Видимая правая сторона теперь — левая сторона узла (ручка Left).
    let right = handle(&h, 0.0, 0.5);
    assert_eq!(right, Point::new(100.0, 125.0));
    h.drag((40.0, 125.0), (160.0, 125.0), none());
    assert_eq!(flips(&mut h, r), (false, false));
    let p = h.props(r);
    assert_eq!((p.x, p.w), (100.0, 60.0));
}

#[test]
fn corner_past_both_sides_mirrors_both_axes_without_rotation() {
    let (mut h, r) = one_rect();
    h.drag((200.0, 150.0), (60.0, 80.0), none());
    let p = h.props(r);
    assert!(p.flip_x && p.flip_y);
    assert_eq!(
        (p.x, p.y, p.w, p.h, p.rotation),
        (60.0, 80.0, 40.0, 20.0, 0.0)
    );
}

#[test]
fn shift_keeps_aspect_and_alt_resizes_from_centre_while_mirroring() {
    let (mut h, r) = one_rect();
    // Shift: угол за левую сторону; пропорции 2:1 по большему изменению (×1.2).
    h.drag((200.0, 150.0), (40.0, 160.0), shift());
    let p = h.props(r);
    assert!(p.flip_x && !p.flip_y);
    assert_eq!((p.x, p.y, p.w, p.h), (-20.0, 100.0, 120.0, 60.0));

    let (mut h, r) = one_rect();
    // Alt: правая ручка за центр (150) — обе стороны симметрично меняются местами.
    h.drag((200.0, 125.0), (120.0, 125.0), alt());
    let p = h.props(r);
    assert!(p.flip_x && !p.flip_y);
    assert_eq!((p.x, p.y, p.w, p.h), (120.0, 100.0, 60.0, 50.0));
}

#[test]
fn rotated_node_mirrors_in_its_own_axes() {
    let mut h = H::new();
    let r = h.rect(None, 300.0, 300.0, 100.0, 50.0);
    h.set(
        r,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    h.ed.select(&[r]);
    let w0 = world(&h, r);
    let before = h.state();
    let steps = h.ed.step_count();
    // Правая ручка — в точку (−40, 25) в осях узла, за его левую сторону.
    let from = w0 * Point::new(100.0, 25.0);
    let to = w0 * Point::new(-40.0, 25.0);
    h.drag((from.x, from.y), (to.x, to.y), none());
    let p = h.props(r);
    assert!(p.flip_x && !p.flip_y);
    assert!(close(p.rotation, 30.0), "{p:?}");
    assert!(close(p.w, 40.0) && close(p.h, 50.0), "{p:?}");
    // Новая матрица — старая, отражённая и сжатая в осях узла: ручка под курсором.
    // Узел хранит размер, а не масштаб, поэтому точка u прежней рамки — это 0.4·u новой.
    let expected = w0
        * rect_map(
            Rect::new(0.0, 0.0, 100.0, 50.0),
            Rect::new(0.0, 0.0, -40.0, 50.0),
        );
    assert_affine(world(&h, r) * Affine::scale_non_uniform(0.4, 1.0), expected);
    // Ручка под курсором: правая сторона узла (теперь отражённая, видимо — левая)
    // в точке отпускания.
    let edge = world(&h, r) * Point::new(40.0, 25.0);
    assert!((edge - to).hypot() < 1e-6, "{edge:?} != {to:?}");
    h.assert_one_step(steps, &before);
}

#[test]
fn group_resized_past_side_mirrors_children_in_world() {
    let mut h = H::new();
    let g = h.add(
        None,
        NodeKind::Group,
        PropsPatch {
            x: Some(100.0),
            y: Some(200.0),
            ..Default::default()
        },
    );
    let a = h.rect(Some(g), 0.0, 0.0, 40.0, 20.0);
    let b = h.rect(Some(g), 60.0, 10.0, 30.0, 30.0);
    h.set(
        b,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    h.ed.select(&[g]);
    let sbox = SelectionBox::of(h.ed.scene(), &[g]).unwrap();
    let (wa, wb) = (world(&h, a), world(&h, b));
    let before = h.state();
    let steps = h.ed.step_count();
    // Правая ручка ровно на ширину рамки за левую сторону: чистое отражение.
    let from = sbox.point(1.0, 0.5);
    let to = sbox.point(-1.0, 0.5);
    h.drag((from.x, from.y), (to.x, to.y), none());
    let r = sbox.rect;
    let mirrored = Rect::new(r.x0, r.y0, r.x0 - r.width(), r.y1);
    let t = sbox.to_world * rect_map(r, mirrored) * sbox.to_world.inverse();
    assert!(t.determinant() < 0.0);
    // Мировые углы детей — отражение прежних: новая мировая матрица = t · прежняя.
    assert_affine(world(&h, a), t * wa);
    assert_affine(world(&h, b), t * wb);
    // У каждого ребёнка переключено ровно одно отражение (определитель меняет знак).
    let (fa, fb) = (flips(&mut h, a), flips(&mut h, b));
    assert_eq!(fa, (true, false));
    assert!(fb.0 ^ fb.1, "{fb:?}");
    // Группа своей рамки не имеет: отражаются её дети, сама она не отражена.
    assert_eq!(flips(&mut h, g), (false, false));
    h.assert_one_step(steps, &before);
}

#[test]
fn multi_selection_resized_past_side_mirrors_each_node() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(None, 200.0, 100.0, 50.0, 30.0);
    // Повёрнутый на четверть: правило то же, что для любого угла — отражение по x
    // мира переключает flip_x, а поворот θ становится −θ (90° → 270°).
    let c = h.rect(None, 300.0, 200.0, 60.0, 20.0);
    h.set(
        c,
        PropsPatch {
            rotation: Some(90.0),
            ..Default::default()
        },
    );
    h.ed.select(&[a, b, c]);
    let sbox = SelectionBox::of(h.ed.scene(), &[a, b, c]).unwrap();
    let olds = [world(&h, a), world(&h, b), world(&h, c)];
    let before = h.state();
    let steps = h.ed.step_count();
    let from = sbox.point(1.0, 0.5);
    let to = sbox.point(-1.0, 0.5);
    h.drag((from.x, from.y), (to.x, to.y), none());
    let r = sbox.rect;
    let t = rect_map(r, Rect::new(r.x0, r.y0, r.x0 - r.width(), r.y1));
    for (id, old) in [a, b, c].into_iter().zip(olds) {
        assert_affine(world(&h, id), t * old);
    }
    assert_eq!(flips(&mut h, a), (true, false));
    assert_eq!(flips(&mut h, b), (true, false));
    assert_eq!(flips(&mut h, c), (true, false));
    assert_eq!(h.props(c).rotation, 270.0);
    h.assert_one_step(steps, &before);
}

#[test]
fn flip_commands_mirror_selection_about_its_centre() {
    let (mut h, r) = one_rect();
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyH", shift());
    let p = h.props(r);
    assert!(p.flip_x && !p.flip_y);
    // Рамка на месте: центр выделения неподвижен.
    assert_eq!(
        (p.x, p.y, p.w, p.h, p.rotation),
        (100.0, 100.0, 100.0, 50.0, 0.0)
    );
    assert_eq!(h.sel(), vec![r]);
    h.assert_one_step(steps, &before);
    // Повторное отражение возвращает узел.
    h.key("KeyH", shift());
    assert_eq!(h.state(), before);

    let steps = h.ed.step_count();
    h.key("KeyV", shift());
    assert_eq!(flips(&mut h, r), (false, true));
    h.assert_one_step(steps, &before);
    // Инструмент не сменился: Shift+V — не «Перемещение», Shift+H — не «Рука».
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    h.key("KeyH", shift());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert_eq!(flips(&mut h, r), (true, true));
}

#[test]
fn flip_command_on_rotated_node_negates_rotation() {
    let mut h = H::new();
    let r = h.rect(None, 300.0, 300.0, 100.0, 50.0);
    h.set(
        r,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    h.ed.select(&[r]);
    let w0 = world(&h, r);
    let c = SelectionBox::of(h.ed.scene(), &[r]).unwrap().center_world();
    h.ed.execute(CommandId::FlipHorizontal);
    let p = h.props(r);
    // Отражение по x мира: поворот θ становится −θ, отражение по x узла включается.
    assert!(p.flip_x && !p.flip_y);
    assert!(close(p.rotation, 330.0), "{p:?}");
    let mx = Affine::translate(c.to_vec2())
        * Affine::scale_non_uniform(-1.0, 1.0)
        * Affine::translate(-c.to_vec2());
    assert_affine(world(&h, r), mx * w0);

    h.ed.execute(CommandId::FlipVertical);
    let p = h.props(r);
    assert!(p.flip_x && p.flip_y);
    assert!(close(p.rotation, 30.0), "{p:?}");
    let my = Affine::translate(c.to_vec2())
        * Affine::scale_non_uniform(1.0, -1.0)
        * Affine::translate(-c.to_vec2());
    assert_affine(world(&h, r), my * mx * w0);
}

#[test]
fn flip_command_on_frame_mirrors_children_without_rewriting_them() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 200.0, 100.0);
    let k = h.rect(Some(f), 10.0, 10.0, 40.0, 20.0);
    let k_props = h.props(k);
    h.ed.select(&[f]);
    let steps = h.ed.step_count();
    let before = h.state();
    h.ed.execute(CommandId::FlipHorizontal);
    assert_eq!(flips(&mut h, f), (true, false));
    assert_eq!(h.props(k), k_props, "дети фрейма не переписываются");
    assert_eq!(
        h.ed.scene().node(k).unwrap().world_bounds.unwrap(),
        Rect::new(250.0, 110.0, 290.0, 130.0)
    );
    h.assert_one_step(steps, &before);
}

#[test]
fn flip_command_skips_locked_and_empty_selection() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    let steps = h.ed.step_count();
    h.ed.execute(CommandId::FlipHorizontal);
    assert_eq!(h.ed.step_count(), steps, "без выделения шага нет");
    h.set(
        r,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    h.ed.select(&[r]);
    let steps = h.ed.step_count();
    h.ed.execute(CommandId::FlipVertical);
    assert_eq!(h.ed.step_count(), steps, "заблокированный не отражается");
    assert_eq!(flips(&mut h, r), (false, false));
}

#[test]
fn escape_during_mirroring_resize_leaves_no_history() {
    let (mut h, r) = one_rect();
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag_hold((200.0, 125.0), (40.0, 125.0), none());
    assert!(
        h.ed.scene().node(r).unwrap().props.flip_x,
        "превью отражено"
    );
    h.key("Escape", none());
    h.up(40.0, 125.0, none());
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    assert!(!h.ed.scene().node(r).unwrap().props.flip_x);
    h.assert_scene_clean();
}

#[test]
fn snapping_and_pixel_snap_work_on_mirrored_node() {
    let (mut h, r) = one_rect();
    h.rect(None, 300.0, 100.0, 50.0, 50.0);
    h.ed.select(&[r]);
    h.key("KeyH", shift());
    assert_eq!(flips(&mut h, r), (true, false));
    // Видимая правая сторона (200, 125) — ручка Left отражённого узла. До соседа 3 px:
    // край притягивается к 300.
    assert_eq!(handle(&h, 0.0, 0.5), Point::new(200.0, 125.0));
    h.drag((200.0, 125.0), (297.0, 125.0), none());
    let p = h.props(r);
    assert!(p.flip_x);
    assert_eq!((p.x, p.w), (100.0, 200.0));
    // Пиксельная привязка: дробный курсор вдали от соседей — целые размер и положение.
    h.drag((300.0, 125.0), (250.4, 125.0), none());
    let p = h.props(r);
    assert_eq!((p.x, p.w), (100.0, 150.0));
    // И при переходе за сторону: 70.4 → рамка 70..100.
    h.drag((250.0, 125.0), (70.4, 125.0), none());
    let p = h.props(r);
    assert!(!p.flip_x, "второе отражение снимает флаг");
    assert_eq!((p.x, p.w), (70.0, 30.0));
}

#[test]
fn snap_lines_show_for_mirrored_node() {
    let (mut h, r) = one_rect();
    h.rect(None, 300.0, 100.0, 50.0, 50.0);
    h.ed.select(&[r]);
    h.key("KeyH", shift());
    h.drag_hold((200.0, 125.0), (297.0, 125.0), none());
    assert!(
        !h.ed.overlay().snap_lines.is_empty(),
        "линия привязки к соседу"
    );
    h.up(297.0, 125.0, none());
    assert_eq!(h.props(r).w, 200.0);
}

#[test]
fn dragging_out_of_mirrored_frame_keeps_world_appearance() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 300.0, 200.0);
    h.set(
        f,
        PropsPatch {
            flip_x: Some(true),
            ..Default::default()
        },
    );
    let k = h.rect(Some(f), 10.0, 10.0, 40.0, 20.0);
    h.ed.select(&[k]);
    let w0 = world(&h, k);
    // Ребёнок в мире у правого края фрейма: 100 + 300 − 10 − 40 = 350..390.
    let c = h.ed.scene().node(k).unwrap().world_bounds.unwrap().center();
    assert_eq!(c, Point::new(370.0, 120.0));
    h.drag((370.0, 120.0), (570.0, 520.0), none());
    assert_eq!(h.parent(k), None, "ушёл из фрейма на холст");
    // Вид на экране тот же: отражение фрейма перешло в отражение узла.
    assert_affine(world(&h, k), Affine::translate((200.0, 400.0)) * w0);
    assert_eq!(flips(&mut h, k), (true, false));
}

#[test]
fn copy_paste_out_of_mirrored_frame_keeps_world_appearance() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 300.0, 200.0);
    h.set(
        f,
        PropsPatch {
            flip_y: Some(true),
            ..Default::default()
        },
    );
    let k = h.rect(Some(f), 10.0, 10.0, 40.0, 20.0);
    h.ed.select(&[k]);
    let w0 = world(&h, k);
    h.ed.execute(CommandId::Copy);
    let bytes =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::Clipboard(b) => Some(b),
                _ => None,
            })
            .expect("копия");
    h.ed.select(&[]);
    h.ed.paste(&bytes).unwrap();
    let pasted = h.sel();
    assert_eq!(pasted.len(), 1);
    assert_eq!(h.parent(pasted[0]), None);
    assert_affine(world(&h, pasted[0]), w0);
    assert_eq!(flips(&mut h, pasted[0]), (false, true));
}

#[test]
fn drawing_inside_mirrored_frame_appears_unmirrored() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 300.0, 200.0);
    h.set(
        f,
        PropsPatch {
            flip_x: Some(true),
            ..Default::default()
        },
    );
    h.key("KeyR", none());
    h.drag((150.0, 150.0), (210.0, 190.0), none());
    let r = h.sel()[0];
    assert_eq!(h.parent(r), Some(f));
    // Нарисованная фигура лежит там, где её рисовали, и в мире не отражена.
    assert_eq!(
        h.ed.scene().node(r).unwrap().world_bounds.unwrap(),
        Rect::new(150.0, 150.0, 210.0, 190.0)
    );
    assert!(world(&h, r).determinant() > 0.0);
    assert_eq!(flips(&mut h, r), (true, false));
}

/// Прямоугольник 100×50 в (300, 300) с поворотом `deg`, выделенный.
fn rotated(deg: f64) -> (H, NodeId) {
    let mut h = H::new();
    let r = h.rect(None, 300.0, 300.0, 100.0, 50.0);
    h.set(
        r,
        PropsPatch {
            rotation: Some(deg),
            ..Default::default()
        },
    );
    h.ed.select(&[r]);
    (h, r)
}

fn mirror_about(c: Point, sx: f64, sy: f64) -> Affine {
    Affine::translate(c.to_vec2())
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate(-c.to_vec2())
}

/// Одно правило при любом угле, как в Figma: отражение по x мира переключает
/// flip_x, по y — flip_y, поворот θ становится −θ; повтор возвращает узел.
#[test]
fn flip_commands_follow_one_rule_at_any_angle() {
    for deg in [60.0, 90.0, 135.0] {
        for (cmd, sx, sy, want) in [
            (CommandId::FlipHorizontal, -1.0, 1.0, (true, false)),
            (CommandId::FlipVertical, 1.0, -1.0, (false, true)),
        ] {
            let (mut h, r) = rotated(deg);
            let before = h.props(r);
            let w0 = world(&h, r);
            let c = SelectionBox::of(h.ed.scene(), &[r]).unwrap().center_world();
            h.ed.execute(cmd);
            let p = h.props(r);
            assert_eq!((p.flip_x, p.flip_y), want, "{deg} {cmd:?}");
            let neg = (360.0 - deg) % 360.0;
            assert!(close(p.rotation, neg), "{deg} {cmd:?}: {p:?}");
            assert_affine(world(&h, r), mirror_about(c, sx, sy) * w0);
            // Повтор возвращает прежнюю позу.
            h.ed.execute(cmd);
            let p = h.props(r);
            assert_eq!((p.flip_x, p.flip_y), (false, false), "{deg} {cmd:?}");
            assert_eq!(p.rotation, before.rotation, "{deg} {cmd:?}");
            assert!(
                (p.x - before.x).abs() < 1e-9 && (p.y - before.y).abs() < 1e-9,
                "{deg} {cmd:?}: {p:?}"
            );
            assert_eq!((p.w, p.h), (before.w, before.h));
            assert_affine(world(&h, r), w0);
        }
    }
}

/// Ресайз нескольких узлов за сторону — то же правило, что у команд.
#[test]
fn multi_selection_resize_past_side_follows_one_rule_at_any_angle() {
    for deg in [60.0, 90.0, 135.0] {
        for vertical in [false, true] {
            let mut h = H::new();
            let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
            let r = h.rect(None, 300.0, 300.0, 100.0, 50.0);
            h.set(
                r,
                PropsPatch {
                    rotation: Some(deg),
                    ..Default::default()
                },
            );
            h.ed.select(&[a, r]);
            let sbox = SelectionBox::of(h.ed.scene(), &[a, r]).unwrap();
            let w0 = world(&h, r);
            let (from, to, t) = if vertical {
                let rr = sbox.rect;
                (
                    sbox.point(0.5, 1.0),
                    sbox.point(0.5, -1.0),
                    rect_map(rr, Rect::new(rr.x0, rr.y0, rr.x1, rr.y0 - rr.height())),
                )
            } else {
                let rr = sbox.rect;
                (
                    sbox.point(1.0, 0.5),
                    sbox.point(-1.0, 0.5),
                    rect_map(rr, Rect::new(rr.x0, rr.y0, rr.x0 - rr.width(), rr.y1)),
                )
            };
            let steps = h.ed.step_count();
            h.drag((from.x, from.y), (to.x, to.y), none());
            assert_eq!(h.ed.step_count(), steps + 1);
            let p = h.props(r);
            let want = if vertical {
                (false, true)
            } else {
                (true, false)
            };
            assert_eq!((p.flip_x, p.flip_y), want, "{deg} {vertical}");
            assert!(close(p.rotation, (360.0 - deg) % 360.0), "{deg}: {p:?}");
            assert_affine(world(&h, r), t * w0);
        }
    }
}

#[test]
fn node_rotated_180_snaps_on_resize() {
    // Под 180° — та же матрица, что у отражения по обеим осям: рамка выровнена по
    // осям мира, и ручка ресайза привязывается к соседям.
    let (mut h, r) = rotated(180.0);
    // Рамка узла видимо 200..300 × 250..300; сосед левее, его правый край — 150.
    h.rect(None, 100.0, 250.0, 50.0, 50.0);
    h.ed.select(&[r]);
    // Ручка Right (локально x = 100) — видимый левый край x = 200.
    assert_eq!(handle(&h, 1.0, 0.5), Point::new(200.0, 275.0));
    h.drag_hold((200.0, 275.0), (153.0, 275.0), none());
    assert!(
        !h.ed.overlay().snap_lines.is_empty(),
        "линия привязки к соседу"
    );
    h.up(153.0, 275.0, none());
    let p = h.props(r);
    // Без привязки ширина была бы 147.
    assert_eq!(p.w, 150.0);
    assert_eq!(p.rotation, 180.0);
    assert_eq!((p.flip_x, p.flip_y), (false, false));
}
