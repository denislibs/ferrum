//! Правки векторной сети без документа (подсистема 3, этап 3): фигура → сеть,
//! рамка после правки, точка на сегменте, удаление с сохранением формы и с
//! разрывом, ручки и зеркальность, изгиб, грани и «Заливка области», слияние.

use std::sync::Arc;

use ferrum_doc::*;
use ferrum_editor::vector_ops::*;
use ferrum_scene::shape::local_transform;
use ferrum_scene::vector::{fit, network_bounds, segment_curve};
use kurbo::{Affine, ParamCurve, Point, Rect, Vec2};

fn net(points: &[(f64, f64)], segs: &[(u32, u32)]) -> VectorNetwork {
    VectorNetwork {
        points: points
            .iter()
            .map(|&(x, y)| VectorPoint::new(x, y))
            .collect(),
        segments: segs
            .iter()
            .map(|&(a, b)| VectorSegment::line(a, b))
            .collect(),
        regions: vec![],
    }
}

fn near(a: Point, b: Point, eps: f64) -> bool {
    (a - b).hypot() <= eps
}

/// Наибольшее отклонение кривых сети `b` от окружности радиуса `r` с центром `c`.
fn circle_error(n: &VectorNetwork, c: Point, r: f64) -> f64 {
    let mut worst: f64 = 0.0;
    for s in &n.segments {
        let curve = segment_curve(n, s);
        for i in 0..=32 {
            let p = curve.eval(i as f64 / 32.0);
            worst = worst.max(((p - c).hypot() - r).abs());
        }
    }
    worst
}

fn props(kind: NodeKind, w: f64, h: f64) -> Props {
    Props {
        w,
        h,
        ..Props::defaults(kind)
    }
}

#[test]
fn shapes_become_networks_of_the_same_geometry() {
    let rect = shape_network(&props(NodeKind::Rect, 100.0, 60.0)).unwrap();
    assert_eq!(rect.points.len(), 4);
    assert_eq!(rect.regions[0].loops, vec![vec![0, 1, 2, 3]]);
    assert!(rect.validate().is_ok());
    let rounded = shape_network(&Props {
        corner_radius: 10.0,
        ..props(NodeKind::Rect, 100.0, 60.0)
    })
    .unwrap();
    assert_eq!((rounded.points.len(), rounded.segments.len()), (8, 8));
    assert_eq!(
        network_bounds(&rounded),
        Some(Rect::new(0.0, 0.0, 100.0, 60.0))
    );
    assert!(rounded.validate().is_ok());
    let circle = shape_network(&props(NodeKind::Ellipse, 200.0, 200.0)).unwrap();
    assert_eq!(circle.points[0].handle_mode, HandleMode::MirrorAll);
    assert!(circle_error(&circle, Point::new(100.0, 100.0), 100.0) < 0.03);
    assert_eq!(
        network_bounds(&circle),
        Some(Rect::new(0.0, 0.0, 200.0, 200.0))
    );
    let line = shape_network(&props(NodeKind::Line, 80.0, 0.0)).unwrap();
    assert_eq!((line.points.len(), line.regions.len()), (2, 0));
    assert!(shape_network(&props(NodeKind::Text, 10.0, 10.0)).is_none());
    // Вектор, растянутый ресайзом, — в координатах своей рамки.
    let mut v = props(NodeKind::Vector, 200.0, 100.0);
    v.vector = Some(Arc::new(net(&[(0.0, 0.0), (100.0, 50.0)], &[(0, 1)])));
    let n = shape_network(&v).unwrap();
    assert!(near(
        Point::new(n.points[1].x, n.points[1].y),
        Point::new(200.0, 100.0),
        1e-9
    ));
}

#[test]
fn reframe_keeps_points_in_place_under_rotation_and_flip() {
    let mut p = props(NodeKind::Vector, 100.0, 50.0);
    p.x = 30.0;
    p.y = -20.0;
    p.rotation = 35.0;
    p.flip_x = true;
    p.flip_y = true;
    p.vector = Some(Arc::new(net(&[(0.0, 0.0), (100.0, 50.0)], &[(0, 1)])));
    // Новая сеть вылезает за прежнюю рамку влево и вниз.
    let grown = net(
        &[(-40.0, 0.0), (100.0, 50.0), (60.0, 90.0)],
        &[(0, 1), (1, 2)],
    );
    let world_before: Vec<Point> = grown
        .points
        .iter()
        .map(|q| local_transform(&p) * Point::new(q.x, q.y))
        .collect();
    let patch = reframe(&p, grown);
    let mut after = p.clone();
    patch.apply_to(&mut after);
    assert_eq!((after.w, after.h), (140.0, 90.0));
    let n = after.vector.as_deref().unwrap();
    assert_eq!(network_bounds(n).unwrap().origin(), Point::ZERO);
    let m = local_transform(&after) * fit(&after);
    for (q, w) in n.points.iter().zip(&world_before) {
        assert!(near(m * Point::new(q.x, q.y), *w, 1e-9), "{q:?}");
    }
}

#[test]
fn add_point_splits_the_curve_without_changing_it() {
    let circle = shape_network(&props(NodeKind::Ellipse, 100.0, 100.0)).unwrap();
    let before = segment_curve(&circle, &circle.segments[1]);
    let (n, p) = add_point(&circle, 1, 0.25);
    assert_eq!(p, 4);
    assert_eq!(n.segments.len(), 5);
    assert_eq!(n.points[4].handle_mode, HandleMode::MirrorAngle);
    let at = before.eval(0.25);
    assert!(near(Point::new(n.points[4].x, n.points[4].y), at, 1e-9));
    // Первый кусок повторяет начало кривой, второй — конец.
    assert!(near(
        segment_curve(&n, &n.segments[1]).eval(0.5),
        before.eval(0.125),
        1e-9
    ));
    assert!(near(
        segment_curve(&n, &n.segments[4]).eval(0.5),
        before.eval(0.625),
        1e-9
    ));
    // Петля области — из пяти сегментов и по-прежнему замкнута.
    assert_eq!(n.regions[0].loops[0], vec![0, 1, 4, 2, 3]);
    assert!(n.validate().is_ok());
    // Точка на отрезке — без ручек.
    let (l, _) = add_point(&net(&[(0.0, 0.0), (10.0, 0.0)], &[(0, 1)]), 0, 0.5);
    assert_eq!(l.points[2], VectorPoint::new(5.0, 0.0));
}

#[test]
fn delete_keeps_shape_or_breaks() {
    let circle = shape_network(&props(NodeKind::Ellipse, 200.0, 200.0)).unwrap();
    let kept = delete_points_keep_shape(&circle, &[1]);
    assert_eq!((kept.points.len(), kept.segments.len()), (3, 3));
    assert!(kept.validate().is_ok());
    assert_eq!(kept.regions[0].loops[0].len(), 3, "область осталась");
    let err = circle_error(&kept, Point::new(100.0, 100.0), 100.0);
    assert!(
        err < 4.0,
        "одна кривая вместо двух — почти та же дуга: {err}"
    );
    // Угол между отрезками — отрезок.
    let corner = net(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], &[(0, 1), (1, 2)]);
    let line = delete_points_keep_shape(&corner, &[1]);
    assert_eq!(line.segments, vec![VectorSegment::line(0, 1)]);
    // Конец пути уходит со своим сегментом; несколько точек сразу.
    let path = net(
        &[(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)],
        &[(0, 1), (1, 2), (2, 3)],
    );
    let short = delete_points_keep_shape(&path, &[3, 1]);
    assert_eq!(short.points.len(), 2);
    assert_eq!(short.segments, vec![VectorSegment::line(0, 1)]);
    // С разрывом: точка уходит с обоими сегментами, петля области — тоже.
    let broken = delete_points_break(&circle, &[1]);
    assert_eq!((broken.points.len(), broken.segments.len()), (3, 2));
    assert!(broken.regions.is_empty());
    // Сегмент: концы остаются, пока у них есть другие сегменты.
    let gone = delete_segments(&path, &[0]);
    assert_eq!(gone.points.len(), 3, "точка 0 без сегментов ушла");
    assert_eq!(gone.segments.len(), 2);
}

#[test]
fn handles_mirror_by_mode_and_alt_breaks() {
    let circle = shape_network(&props(NodeKind::Ellipse, 100.0, 100.0)).unwrap();
    // Точка 1 (справа): сегмент 0 приходит в неё концом, сегмент 1 выходит началом.
    let n = set_handle(&circle, 1, HandleEnd::Start, Vec2::new(10.0, 40.0), false);
    let other = n.segments[0].tangent_end;
    assert!((Vec2::new(other[0], other[1]) + Vec2::new(10.0, 40.0)).hypot() < 1e-9);
    let mut angle = circle.clone();
    angle.points[1].handle_mode = HandleMode::MirrorAngle;
    let own = Vec2::new(
        circle.segments[0].tangent_end[0],
        circle.segments[0].tangent_end[1],
    )
    .hypot();
    let n = set_handle(&angle, 1, HandleEnd::Start, Vec2::new(0.0, 80.0), false);
    let t = n.segments[0].tangent_end;
    assert!(
        (Vec2::new(t[0], t[1]) - Vec2::new(0.0, -own)).hypot() < 1e-9,
        "длина своя"
    );
    let n = set_handle(&circle, 1, HandleEnd::Start, Vec2::new(10.0, 40.0), true);
    assert_eq!(n.points[1].handle_mode, HandleMode::Free);
    assert_eq!(n.segments[0].tangent_end, circle.segments[0].tangent_end);
}

#[test]
fn bend_moves_the_curve_point_under_the_cursor() {
    let n = net(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1)]);
    for t in [0.3, 0.5, 0.8] {
        let d = Vec2::new(5.0, 30.0);
        let b = bend(&n, 0, t, d);
        let before = segment_curve(&n, &n.segments[0]).eval(t);
        let after = segment_curve(&b, &b.segments[0]).eval(t);
        assert!(near(after, before + d, 1e-9), "t = {t}");
        assert_eq!((b.points[0], b.points[1]), (n.points[0], n.points[1]));
    }
}

#[test]
fn faces_and_paint_bucket() {
    // Квадрат с диагональю: две треугольные грани, внешняя не входит.
    let sq = net(
        &[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)],
        &[(0, 1), (1, 2), (2, 3), (3, 0), (0, 2)],
    );
    let f = faces(&sq);
    assert_eq!(f.len(), 2, "{f:?}");
    let filled = toggle_fill(&sq, Point::new(70.0, 30.0), false).unwrap();
    assert_eq!(filled.regions.len(), 1);
    let mut loop_ = filled.regions[0].loops[0].clone();
    loop_.sort();
    assert_eq!(loop_, vec![0, 1, 4], "правый верхний треугольник");
    assert_eq!(
        filled.regions[0].fills,
        Some(vec![Fill::solid(Color::SHAPE_GRAY)])
    );
    assert!(filled.validate().is_ok());
    // Узел с заливками: область берёт их (`None`).
    let own = toggle_fill(&sq, Point::new(30.0, 70.0), true).unwrap();
    assert_eq!(own.regions[0].fills, None);
    // Повторный щелчок снимает заливку; мимо граней — ничего.
    let cleared = toggle_fill(&filled, Point::new(70.0, 30.0), false).unwrap();
    assert!(cleared.regions.is_empty());
    assert!(toggle_fill(&sq, Point::new(150.0, 50.0), false).is_none());
    // Открытый путь граней не имеет.
    assert!(faces(&net(&[(0.0, 0.0), (10.0, 0.0)], &[(0, 1)])).is_empty());
}

#[test]
fn merge_offsets_indices_and_transforms_handles() {
    let a = shape_network(&props(NodeKind::Rect, 10.0, 10.0)).unwrap();
    let b = shape_network(&props(NodeKind::Ellipse, 20.0, 10.0)).unwrap();
    let m = merge_networks(&[
        (a, Affine::IDENTITY),
        (
            b.clone(),
            Affine::translate((100.0, 0.0)) * Affine::scale(2.0),
        ),
    ]);
    assert_eq!(
        (m.points.len(), m.segments.len(), m.regions.len()),
        (8, 8, 2)
    );
    assert_eq!(m.regions[1].loops[0], vec![4, 5, 6, 7]);
    assert_eq!(m.segments[4].start, 4);
    assert_eq!(m.points[4].x, 100.0 + 2.0 * b.points[0].x);
    assert_eq!(
        m.segments[4].tangent_start[0],
        2.0 * b.segments[0].tangent_start[0]
    );
    assert!(m.validate().is_ok());
}
