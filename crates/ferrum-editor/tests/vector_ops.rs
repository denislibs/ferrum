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
        // Концы стоят на месте; режим ручек у них — `Free` (см. следующий тест).
        for i in 0..2 {
            assert_eq!(
                (b.points[i].x, b.points[i].y),
                (n.points[i].x, n.points[i].y)
            );
        }
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

/// Закрашена ли точка сетью: сумма поворотов петель каждой области по её правилу.
fn filled_at(n: &VectorNetwork, p: Point) -> bool {
    use kurbo::Shape;
    n.regions.iter().any(|r| {
        let w: i32 = r
            .loops
            .iter()
            .filter_map(|l| loop_path(n, l))
            .map(|path| path.winding(p))
            .sum();
        match r.rule {
            FillRule::NonZero => w != 0,
            FillRule::EvenOdd => w.rem_euclid(2) == 1,
        }
    })
}

fn assert_round_trips(n: &VectorNetwork) {
    assert!(n.validate().is_ok(), "{:?}", n.validate());
    assert_eq!(&VectorNetwork::decode(&n.encode()).unwrap(), n);
}

#[test]
fn paint_bucket_ignores_a_dangling_edge_inside_a_face() {
    // Квадрат и «хвост» из угла внутрь: грань не заходит в хвост и обратно.
    let sq = net(
        &[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)],
        &[(0, 1), (1, 2), (2, 3), (3, 0)],
    );
    let mut with_spur = sq.clone();
    with_spur.points.push(VectorPoint::new(50.0, 50.0));
    with_spur.segments.push(VectorSegment::line(0, 4));
    let f = faces(&with_spur);
    assert_eq!(f.len(), 1, "{f:?}");
    let mut l = f[0].clone();
    l.sort();
    assert_eq!(l, vec![0, 1, 2, 3]);
    let filled = toggle_fill(&with_spur, Point::new(70.0, 30.0), false).unwrap();
    assert_eq!(filled.regions[0].loops[0].len(), 4);
    assert_round_trips(&filled);
}

#[test]
fn bend_then_dragging_the_handle_keeps_neighbours_straight() {
    let rect = shape_network(&props(NodeKind::Rect, 100.0, 60.0)).unwrap();
    let bent = bend(&rect, 0, 0.5, Vec2::new(0.0, -20.0));
    let dragged = set_handle(&bent, 0, HandleEnd::Start, Vec2::new(30.0, -30.0), false);
    for i in 1..4 {
        assert!(dragged.segments[i].is_straight(), "сегмент {i}");
    }
    assert_round_trips(&dragged);
}

#[test]
fn paint_bucket_empties_exactly_the_clicked_face_of_a_bigger_filled_loop() {
    let mut sq = net(
        &[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)],
        &[(0, 1), (1, 2), (2, 3), (3, 0), (0, 2)],
    );
    sq.regions.push(VectorRegion {
        rule: FillRule::NonZero,
        loops: vec![vec![0, 1, 2, 3]],
        fills: Some(vec![Fill::solid(Color::SHAPE_GRAY)]),
    });
    let (upper, lower) = (Point::new(70.0, 30.0), Point::new(30.0, 70.0));
    assert!(filled_at(&sq, upper) && filled_at(&sq, lower));
    let r = toggle_fill(&sq, upper, false).unwrap();
    assert!(!filled_at(&r, upper));
    assert!(filled_at(&r, lower));
    assert_eq!(r.regions.len(), 1);
    assert_eq!(r.regions[0].fills, sq.regions[0].fills);
    assert_round_trips(&r);
    // Повторный щелчок заливает грань снова.
    let again = toggle_fill(&r, upper, false).unwrap();
    assert!(filled_at(&again, upper) && filled_at(&again, lower));
    assert_round_trips(&again);
}

#[test]
fn paint_bucket_on_a_ring_clears_the_ring_not_the_hole() {
    let mut n = net(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
            (30.0, 30.0),
            (70.0, 30.0),
            (70.0, 70.0),
            (30.0, 70.0),
        ],
        &[
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
        ],
    );
    n.regions.push(VectorRegion {
        rule: FillRule::EvenOdd,
        loops: vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]],
        fills: Some(vec![Fill::solid(Color::SHAPE_GRAY)]),
    });
    let (ring, hole) = (Point::new(10.0, 50.0), Point::new(50.0, 50.0));
    assert!(filled_at(&n, ring) && !filled_at(&n, hole));
    let r = toggle_fill(&n, ring, false).unwrap();
    assert!(!filled_at(&r, ring) && !filled_at(&r, hole));
    assert_round_trips(&r);
}

/// Квадрат 0..100 и квадрат 30..70 внутри него (две связные части, без областей).
fn nested_squares() -> VectorNetwork {
    net(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
            (30.0, 30.0),
            (70.0, 30.0),
            (70.0, 70.0),
            (30.0, 70.0),
        ],
        &[
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
        ],
    )
}

#[test]
fn paint_bucket_ignores_a_bridge_between_nested_squares() {
    // Мост из угла внешнего квадрата к углу внутреннего не ограничивает грань.
    let mut n = nested_squares();
    n.segments.push(VectorSegment::line(0, 4));
    for f in faces(&n) {
        let mut s = f.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), f.len(), "сегмент дважды в грани {f:?}");
    }
    let (ring, hole) = (Point::new(10.0, 50.0), Point::new(50.0, 50.0));
    let r = toggle_fill(&n, ring, false).unwrap();
    assert_round_trips(&r);
    assert!(filled_at(&r, ring) && !filled_at(&r, hole));
    let h = toggle_fill(&n, hole, false).unwrap();
    assert_round_trips(&h);
    assert!(!filled_at(&h, ring) && filled_at(&h, hole));
    // Залитое кольцо + щелчок в дыру — залито всё; снова в кольцо — только дыра.
    let both = toggle_fill(&r, hole, false).unwrap();
    assert_round_trips(&both);
    assert!(filled_at(&both, ring) && filled_at(&both, hole));
    let only_hole = toggle_fill(&both, ring, false).unwrap();
    assert_round_trips(&only_hole);
    assert!(!filled_at(&only_hole, ring) && filled_at(&only_hole, hole));
}

#[test]
fn paint_bucket_on_a_separate_square_keeps_the_hole_of_a_ring_empty() {
    let mut n = nested_squares();
    let o = n.points.len() as u32;
    for (x, y) in [(200.0, 0.0), (300.0, 0.0), (300.0, 100.0), (200.0, 100.0)] {
        n.points.push(VectorPoint::new(x, y));
    }
    for i in 0..4 {
        n.segments.push(VectorSegment::line(o + i, o + (i + 1) % 4));
    }
    n.regions.push(VectorRegion {
        rule: FillRule::EvenOdd,
        loops: vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7], vec![8, 9, 10, 11]],
        fills: Some(vec![Fill::solid(Color::SHAPE_GRAY)]),
    });
    let (ring, hole, side) = (
        Point::new(10.0, 50.0),
        Point::new(50.0, 50.0),
        Point::new(250.0, 50.0),
    );
    assert!(filled_at(&n, ring) && !filled_at(&n, hole) && filled_at(&n, side));
    let r = toggle_fill(&n, side, false).unwrap();
    assert_round_trips(&r);
    assert!(filled_at(&r, ring), "кольцо осталось");
    assert!(!filled_at(&r, hole), "дыра пуста");
    assert!(!filled_at(&r, side), "квадрат снят");
    assert_eq!(r.regions[0].fills, n.regions[0].fills);
}

#[test]
fn paint_bucket_clears_an_inner_square_inside_a_filled_one() {
    let mut n = nested_squares();
    n.regions.push(VectorRegion {
        rule: FillRule::NonZero,
        loops: vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]],
        fills: Some(vec![Fill::solid(Color::SHAPE_GRAY)]),
    });
    let (ring, hole) = (Point::new(10.0, 50.0), Point::new(50.0, 50.0));
    assert!(filled_at(&n, ring) && filled_at(&n, hole));
    let r = toggle_fill(&n, hole, false).unwrap();
    assert_round_trips(&r);
    assert!(filled_at(&r, ring) && !filled_at(&r, hole));
    let again = toggle_fill(&r, hole, false).unwrap();
    assert_round_trips(&again);
    assert!(filled_at(&again, ring) && filled_at(&again, hole));
    // Снятие внешней части оставляет внутренний квадрат залитым.
    let outer = toggle_fill(&n, ring, false).unwrap();
    assert_round_trips(&outer);
    assert!(!filled_at(&outer, ring) && filled_at(&outer, hole));
}

fn squares_in_a_frame(k: usize) -> VectorNetwork {
    ferrum_doc::fixture::squares_network(k)
}

#[test]
fn paint_bucket_on_many_contours_is_local_and_fast() {
    // 4000 квадратов в рамке: щелчок решает только про участки у точки, а не
    // сравнивает каждый контур с каждой гранью (было ≈ 1.7 с в release).
    let n = squares_in_a_frame(4000);
    let (inside, between) = (Point::new(10.0, 10.0), Point::new(17.0, 17.0));
    let t = std::time::Instant::now();
    let one = toggle_fill(&n, inside, false).unwrap();
    let frame = toggle_fill(&one, between, false).unwrap();
    let cleared = toggle_fill(&frame, inside, false).unwrap();
    let elapsed = t.elapsed();
    assert!(filled_at(&one, inside) && !filled_at(&one, between));
    assert!(filled_at(&frame, inside) && filled_at(&frame, between));
    assert!(
        !filled_at(&frame, Point::new(30.0, 10.0)),
        "чужой квадрат — дыра рамки"
    );
    assert!(!filled_at(&cleared, inside) && filled_at(&cleared, between));
    assert_round_trips(&cleared);
    let empty = toggle_fill(&frame, between, false).expect("снятие с рамки");
    assert!(filled_at(&empty, inside) && !filled_at(&empty, between));
    // Отладочная сборка медленнее release в десятки раз — предел с запасом.
    assert!(elapsed.as_secs_f64() < 3.0, "{elapsed:?}");
}

#[test]
fn clearing_one_square_of_a_region_over_many_keeps_the_rest() {
    // Одна область на 2000 квадратов (как после «Сплющить»): снятие заливки с
    // одного переписывает область остальными, не трогая чужие участки.
    let mut n = squares_in_a_frame(2000);
    n.regions.push(VectorRegion {
        rule: FillRule::NonZero,
        loops: (0..2000u32).map(|i| (4 * i..4 * i + 4).collect()).collect(),
        fills: None,
    });
    let t = std::time::Instant::now();
    let r = toggle_fill(&n, Point::new(10.0, 10.0), false).unwrap();
    let elapsed = t.elapsed();
    assert!(!filled_at(&r, Point::new(10.0, 10.0)));
    assert!(filled_at(&r, Point::new(30.0, 10.0)) && filled_at(&r, Point::new(30.0, 30.0)));
    assert!(!filled_at(&r, Point::new(17.0, 17.0)), "рамка не залита");
    assert_eq!(r.regions.len(), 1);
    assert_eq!(r.regions[0].loops.len(), 1999);
    assert!(elapsed.as_secs_f64() < 3.0, "{elapsed:?}");
}
