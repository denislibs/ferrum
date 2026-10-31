//! Геометрия векторной сети и обводки по контуру (подсистема 3, этап 3): цепочки
//! и петли, концы с окончаниями, растяжение под рамку, границы с экстремумами
//! кривых, попадание по обводке и областям, сцена с вектором.

use std::sync::Arc;

use ferrum_doc::*;
use ferrum_scene::shape;
use ferrum_scene::vector::*;
use ferrum_scene::Scene;
use kurbo::{PathEl, Point, Rect, Shape, Vec2};

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

/// Вектор с рамкой по границам сети (как его пишет редактор).
fn vector(n: VectorNetwork) -> Props {
    let b = network_bounds(&n).unwrap_or(Rect::ZERO);
    Props {
        w: b.width(),
        h: b.height(),
        vector: Some(Arc::new(n)),
        ..Props::defaults(NodeKind::Vector)
    }
}

fn subpaths(shape: &PathShape) -> (usize, usize) {
    let els = shape.stroke.elements();
    let moves = els
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count();
    let closes = els
        .iter()
        .filter(|e| matches!(e, PathEl::ClosePath))
        .count();
    (moves, closes)
}

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-9
}

#[test]
fn open_polyline_is_one_chain_with_capped_ends() {
    let mut p = vector(net(
        &[(0.0, 0.0), (50.0, 0.0), (50.0, 40.0)],
        &[(0, 1), (1, 2)],
    ));
    p.stroke_style.cap_start = StrokeCap::Round;
    p.stroke_style.cap_end = StrokeCap::ArrowLines;
    let s = network_shape(&p);
    assert_eq!(
        subpaths(&s),
        (1, 0),
        "одна открытая цепочка: соединение в средней точке"
    );
    assert!(!s.closed);
    assert_eq!(s.ends.len(), 2);
    let start = s.ends.iter().find(|e| e.cap == StrokeCap::Round).unwrap();
    let end = s
        .ends
        .iter()
        .find(|e| e.cap == StrokeCap::ArrowLines)
        .unwrap();
    assert!(near(start.point, Point::new(0.0, 0.0)));
    assert!(
        (start.dir - Vec2::new(-1.0, 0.0)).hypot() < 1e-9,
        "наружу — против хода"
    );
    assert!(near(end.point, Point::new(50.0, 40.0)));
    assert!(
        (end.dir - Vec2::new(0.0, 1.0)).hypot() < 1e-9,
        "наружу — по ходу"
    );
}

#[test]
fn cycle_is_closed_and_junction_is_marked() {
    let tri = vector(net(
        &[(0.0, 0.0), (40.0, 0.0), (20.0, 30.0)],
        &[(0, 1), (1, 2), (2, 0)],
    ));
    let s = network_shape(&tri);
    assert!(s.closed && s.ends.is_empty());
    assert_eq!(subpaths(&s), (1, 1));
    // Звезда: три луча из центра — точка ветвления и три конца.
    let star = vector(net(
        &[(20.0, 20.0), (0.0, 0.0), (40.0, 0.0), (20.0, 40.0)],
        &[(0, 1), (0, 2), (0, 3)],
    ));
    let s = network_shape(&star);
    assert_eq!(s.junctions, vec![Point::new(20.0, 20.0)]);
    assert_eq!(s.ends.len(), 3);
    assert_eq!(subpaths(&s).0, 3);
}

#[test]
fn network_is_stretched_into_the_frame_and_curve_extrema_count() {
    // Сеть с рамкой (10, 10)–(110, 60) в узле 200 × 100: вдвое по обеим осям.
    let mut p = vector(net(&[(10.0, 10.0), (110.0, 60.0)], &[(0, 1)]));
    p.w = 200.0;
    p.h = 100.0;
    let f = fit(&p);
    assert!(near(f * Point::new(10.0, 10.0), Point::ZERO));
    assert!(near(f * Point::new(110.0, 60.0), Point::new(200.0, 100.0)));
    // Горизонтальный отрезок по высоте не растягивается (делить не на что).
    let flat = vector(net(&[(0.0, 5.0), (30.0, 5.0)], &[(0, 1)]));
    assert!(near(
        fit(&flat) * Point::new(30.0, 5.0),
        Point::new(30.0, 0.0)
    ));
    // Дуга выше своих точек: границы — по кривой, а не по точкам.
    let mut arc = net(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1)]);
    arc.segments[0].tangent_start = [0.0, -40.0];
    arc.segments[0].tangent_end = [0.0, -40.0];
    let b = network_bounds(&arc).unwrap();
    assert!(
        (b.y0 + 30.0).abs() < 1e-9,
        "вершина кубической дуги — 3/4 ручки: {b:?}"
    );
}

#[test]
fn stroke_and_region_hits() {
    let mut p = vector(net(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1)]));
    p.strokes[0].width = 2.0;
    let s = network_shape(&p);
    assert!(hit_path(&p, &s, Point::new(50.0, 3.0), 4.0), "допуск 4 px");
    assert!(!hit_path(&p, &s, Point::new(50.0, 6.0), 4.0));
    // Стрелка на конце ловит щелчок за точкой конца.
    p.stroke_style.cap_end = StrokeCap::ArrowTriangle;
    let s = network_shape(&p);
    assert!(hit_path(&p, &s, Point::new(106.0, 0.0), 1.0));
    // Квадрат с дыркой по even-odd: дырка не ловит, кольцо ловит.
    let mut sq = net(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
            (40.0, 40.0),
            (60.0, 40.0),
            (60.0, 60.0),
            (40.0, 60.0),
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
    sq.regions = vec![VectorRegion {
        rule: FillRule::EvenOdd,
        loops: vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]],
        fills: None,
    }];
    let mut q = vector(sq);
    q.strokes.clear();
    let s = network_shape(&q);
    assert!(
        !hit_path(&q, &s, Point::new(20.0, 20.0), 1.0),
        "без заливок узла область пуста"
    );
    q.fills = vec![Fill::solid(Color::BLACK)];
    assert!(hit_path(&q, &s, Point::new(20.0, 20.0), 1.0));
    assert!(
        !hit_path(&q, &s, Point::new(50.0, 50.0), 1.0),
        "дырка even-odd"
    );
    // Своя заливка области — даже без заливок узла.
    let mut own = q.clone();
    own.fills.clear();
    Arc::make_mut(own.vector.as_mut().unwrap()).regions[0].fills =
        Some(vec![Fill::solid(Color::WHITE)]);
    let s = network_shape(&own);
    assert!(hit_path(&own, &s, Point::new(20.0, 20.0), 1.0));
}

#[test]
fn outline_covers_caps_and_inside_stroke_doubles_width() {
    let mut p = vector(net(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1)]));
    p.strokes[0].width = 4.0;
    p.stroke_style.cap_end = StrokeCap::Circle;
    let s = network_shape(&p);
    let out = stroke_outline(&s, &p.strokes[0], &p.stroke_style);
    assert!(out.contains(Point::new(50.0, 1.5)));
    assert!(!out.contains(Point::new(50.0, 2.5)));
    // Круг на конце радиуса 1,5w + 1,5 = 7,5.
    assert!(out.contains(Point::new(100.0, 7.0)));
    assert!(
        !out.contains(Point::new(-1.0, 0.0)),
        "в начале окончания нет — плоский конец"
    );
    // Замкнутый контур внутри: двойная толщина, обрезку делает рендер.
    let mut tri = vector(net(
        &[(0.0, 0.0), (40.0, 0.0), (20.0, 30.0)],
        &[(0, 1), (1, 2), (2, 0)],
    ));
    tri.strokes[0].align = StrokeAlign::Inside;
    let s = network_shape(&tri);
    assert_eq!(effective_width(&s, &tri.strokes[0]), 2.0);
    assert_eq!(stroke_clip(&s, &tri.strokes[0]), StrokeClip::Inside);
    // Открытый путь «внутри» не бывает: по центру, без обрезки.
    let open = network_shape(&p);
    assert_eq!(stroke_clip(&open, &p.strokes[0]), StrokeClip::None);
}

#[test]
fn line_caps_and_rect_joins_switch_to_path_stroke() {
    let mut line = Props {
        w: 100.0,
        ..Props::defaults(NodeKind::Line)
    };
    assert!(!uses_path_stroke(&line));
    assert_eq!(shape::stroke_outset(&line), 0.5);
    line.stroke_style.cap_end = StrokeCap::ArrowLines;
    assert!(uses_path_stroke(&line));
    assert_eq!(shape::stroke_outset(&line), arrow_length(1.0) + 1.0);
    assert!(
        shape::hit(&line, Point::new(104.0, 0.0), 0.5),
        "стрелка за концом"
    );
    let mut rect = Props::defaults(NodeKind::Rect);
    rect.strokes = vec![Stroke {
        color: Color::BLACK,
        width: 10.0,
        align: StrokeAlign::Center,
    }];
    rect.stroke_style.join = StrokeJoin::Round;
    assert!(uses_path_stroke(&rect));
    assert_eq!(
        shape::stroke_outset(&rect),
        5.0,
        "круглый стык не выступает за полтолщины"
    );
    rect.corner_radius = 4.0;
    assert!(!uses_path_stroke(&rect), "у скруглённого углов нет");
}

#[test]
fn scene_bounds_and_hit_test_follow_the_network() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut p = vector(net(&[(0.0, 0.0), (100.0, 50.0)], &[(0, 1)]));
    p.x = 10.0;
    p.y = 20.0;
    p.stroke_style.cap_end = StrokeCap::Diamond;
    let id = doc
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_with_props(None, None, &p)?])
        })
        .unwrap()[0];
    let scene = Scene::from_page(doc.page(page).unwrap());
    let n = scene.node(id).unwrap();
    assert_eq!(n.local_bounds, Some(Rect::new(0.0, 0.0, 100.0, 50.0)));
    let wb = n.world_bounds.unwrap();
    let r = dot_radius(1.0);
    assert!(
        wb.x1 >= 110.0 + r - 1e-9 && wb.y1 >= 70.0 + r - 1e-9,
        "{wb:?}"
    );
    // Середина отрезка — попадание, угол рамки вдали от отрезка — нет.
    assert_eq!(scene.hit_test(Point::new(60.0, 45.0), 1.0), Some(id));
    assert_eq!(scene.hit_test(Point::new(100.0, 25.0), 1.0), None);
}

#[test]
fn node_contours_are_built_once_and_rebuilt_only_when_the_shape_changes() {
    // Попадание и подсветка берут контуры из кэша узла: сеть в 100 000 точек не
    // строится заново на каждое движение указателя.
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let p = vector(net(&[(0.0, 0.0), (100.0, 50.0)], &[(0, 1)]));
    let id = doc
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_with_props(None, None, &p)?])
        })
        .unwrap()[0];
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let first = scene.node(id).unwrap().path_shape().unwrap().clone();
    assert_eq!(scene.hit_test(Point::new(50.0, 25.0), 1.0), Some(id));
    assert_eq!(scene.hit_test(Point::new(10.0, 40.0), 1.0), None);
    let same = scene.node(id).unwrap().path_shape().unwrap();
    assert!(
        Arc::ptr_eq(&first, same),
        "попадание контуры не перестроило"
    );
    // Сдвиг узла вида не меняет — контуры те же.
    scene.set_preview(&[(
        id,
        PropsPatch {
            x: Some(30.0),
            ..Default::default()
        },
    )]);
    assert!(Arc::ptr_eq(
        &first,
        scene.node(id).unwrap().path_shape().unwrap()
    ));
    // Другая сеть (превью жеста) — новые контуры, попадание идёт по ним.
    scene.set_preview(&[(
        id,
        PropsPatch {
            vector: Some(Arc::new(net(&[(0.0, 50.0), (100.0, 0.0)], &[(0, 1)]))),
            ..Default::default()
        },
    )]);
    let moved = scene.node(id).unwrap().path_shape().unwrap().clone();
    assert!(!Arc::ptr_eq(&first, &moved));
    assert_eq!(scene.hit_test(Point::new(10.0, 45.0), 1.0), Some(id));
    assert_eq!(scene.hit_test(Point::new(10.0, 5.0), 1.0), None);
    scene.clear_preview();
    let back = scene.node(id).unwrap().path_shape().unwrap();
    assert_eq!(**back, *first);
    assert_eq!(scene.hit_test(Point::new(10.0, 5.0), 1.0), Some(id));
}

#[test]
fn fit_stays_finite_for_a_tiny_extent_in_a_huge_frame() {
    let n = net(&[(0.0, 0.0), (1e-300, 5.0)], &[(0, 1)]);
    let p = Props {
        w: 1e9,
        h: 10.0,
        vector: Some(Arc::new(n)),
        ..Props::defaults(NodeKind::Vector)
    };
    let c = fit(&p).as_coeffs();
    assert!(c.iter().all(|v| v.is_finite()), "{c:?}");
    let s = network_shape(&p);
    for e in s.stroke.elements() {
        let ok = match e {
            PathEl::MoveTo(a) | PathEl::LineTo(a) => a.is_finite(),
            _ => true,
        };
        assert!(ok, "{e:?}");
    }
    assert!(s
        .ends
        .iter()
        .all(|e| e.point.is_finite() && e.dir.is_finite()));
}

#[test]
fn end_direction_uses_the_far_handle_when_the_near_one_is_zero() {
    let mut n = net(&[(0.0, 0.0), (10.0, 0.0)], &[(0, 1)]);
    // Задана только дальняя ручка: p2 = (10, 10); p1 совпадает с p0.
    n.segments[0].tangent_end = [0.0, 10.0];
    let p = vector(n);
    let s = network_shape(&p);
    let r = std::f64::consts::FRAC_1_SQRT_2;
    let start = s
        .ends
        .iter()
        .find(|e| near(e.point, Point::new(0.0, 0.0)))
        .unwrap();
    assert!(
        (start.dir - Vec2::new(-r, -r)).hypot() < 1e-9,
        "{:?}",
        start.dir
    );

    // Зеркально: задана только ближняя к началу ручка, конец — p3 - p1.
    let mut n = net(&[(0.0, 0.0), (10.0, 0.0)], &[(0, 1)]);
    n.segments[0].tangent_start = [0.0, 10.0];
    let p = vector(n);
    let s = network_shape(&p);
    let end = s
        .ends
        .iter()
        .find(|e| near(e.point, Point::new(10.0, 0.0)))
        .unwrap();
    assert!((end.dir - Vec2::new(r, -r)).hypot() < 1e-9, "{:?}", end.dir);
}

#[test]
fn big_networks_are_simplified_to_the_tolerance_small_ones_are_not() {
    let small = network_shape(&vector(net(
        &[(0.0, 0.0), (50.0, 0.1), (100.0, 0.0)],
        &[(0, 1), (1, 2)],
    )));
    assert_eq!(simplify(&small, 1.0), small, "маленький контур — как есть");
    let p = vector(ferrum_doc::fixture::wavy_network(100_000));
    let shape = network_shape(&p);
    let s = simplify(&shape, 0.25);
    let (before, after) = (shape.stroke.elements().len(), s.stroke.elements().len());
    assert!(after * 20 < before, "{before} → {after}");
    assert!(s.closed && s.regions.len() == 1);
    // Каждая точка упрощённого пути — на исходном контуре (не дальше допуска).
    for el in s.stroke.elements().iter().step_by(50) {
        if let PathEl::LineTo(q) | PathEl::MoveTo(q) = el {
            let d = distance_to(&shape.stroke, *q, 1.0).unwrap();
            assert!(d <= 0.25, "{q:?}: {d}");
        }
    }
}

#[test]
fn stroke_outset_without_paths_matches_the_built_shape() {
    // Выступ считается по степеням точек, без контуров; он должен совпадать с
    // посчитанным по готовой форме (окончания, замкнутость, соединение).
    let by_shape = |p: &Props| {
        let shape = network_shape(p);
        p.strokes
            .iter()
            .filter(|s| s.width > 0.0)
            .map(|s| {
                let w = effective_width(&shape, s);
                let join = if p.stroke_style.join == StrokeJoin::Miter {
                    w / 2.0 * p.stroke_style.miter_limit.max(1.0)
                } else {
                    w / 2.0
                };
                shape
                    .ends
                    .iter()
                    .map(|e| cap_extent(e.cap, w))
                    .fold(join, f64::max)
            })
            .fold(0.0, f64::max)
    };
    let nets = [
        net(&[(0.0, 0.0), (50.0, 0.0), (50.0, 40.0)], &[(0, 1), (1, 2)]),
        net(
            &[(0.0, 0.0), (50.0, 0.0), (50.0, 40.0)],
            &[(0, 1), (1, 2), (2, 0)],
        ),
        net(
            &[(0.0, 0.0), (50.0, 0.0), (50.0, 40.0), (0.0, 40.0)],
            &[(0, 1), (1, 2), (2, 0), (2, 3)],
        ),
    ];
    for n in nets {
        for align in [
            StrokeAlign::Center,
            StrokeAlign::Inside,
            StrokeAlign::Outside,
        ] {
            for (join, cap) in [
                (StrokeJoin::Miter, StrokeCap::None),
                (StrokeJoin::Round, StrokeCap::ArrowTriangle),
            ] {
                let mut p = vector(n.clone());
                p.strokes = vec![Stroke {
                    width: 3.0,
                    align,
                    color: Color::BLACK,
                }];
                p.stroke_style.join = join;
                p.stroke_style.cap_start = cap;
                p.stroke_style.cap_end = StrokeCap::Round;
                assert_eq!(path_stroke_outset(&p), by_shape(&p), "{align:?} {join:?}");
            }
        }
    }
}

#[test]
fn simplified_path_stays_within_the_tolerance_of_the_original() {
    use kurbo::ParamCurve;
    let shape = network_shape(&vector(ferrum_doc::fixture::wavy_network(100_000)));
    for tol in [0.25, 1.0] {
        let s = simplify(&shape, tol);
        let mut worst = 0.0f64;
        // Исходный контур отбирается плотно (каждый 15-й сегмент, 5 точек на нём).
        for seg in shape.stroke.segments().step_by(15) {
            for k in 0..5 {
                let q = seg.eval(k as f64 / 5.0);
                let d = distance_to(&s.stroke, q, 2.0 * tol + 1.0).unwrap_or(f64::MAX);
                worst = worst.max(d);
            }
        }
        assert!(worst <= tol, "допуск {tol}: отклонение {worst}");
    }
}

#[test]
fn simplified_curves_are_never_bigger_than_the_exact_path() {
    // Каждый сегмент — выгнутая кривая: на крупном масштабе упрощение оставляет
    // кривые кривыми, а не дробит их в ломаные больше исходного пути.
    for radius in [2000.0, 50_000.0] {
        let shape = network_shape(&vector(ferrum_doc::fixture::curvy_network(100_000, radius)));
        let exact = shape.stroke.elements().len();
        let region = shape.regions[0].path.elements().len();
        let mut scale = 1.0;
        while scale <= 1024.0 {
            let s = simplify(&shape, 0.25 / scale);
            assert!(
                s.stroke.elements().len() <= exact,
                "r {radius}, масштаб {scale}: {} > {exact}",
                s.stroke.elements().len()
            );
            assert!(s.regions[0].path.elements().len() <= region);
            scale *= 2.0;
        }
    }
    // Отдаление: почти прямые на экране кривые всё так же прореживаются.
    let shape = network_shape(&vector(ferrum_doc::fixture::curvy_network(100_000, 2000.0)));
    let far = simplify(&shape, 2.0);
    assert!(far.stroke.elements().len() * 20 < shape.stroke.elements().len());
}

#[test]
fn near_straight_networks_stay_simplified_when_zoomed_in() {
    // Волнистая сеть (каждый четвёртый сегмент — едва выгнутая кривая): при
    // приближении ×8 упрощение по-прежнему убирает большую часть точек, а не
    // отдаёт точный путь (иначе каждый кадр правки обводил бы весь путь; ближе
    // рендер обводит только куски у экрана).
    let shape = network_shape(&vector(ferrum_doc::fixture::wavy_network(100_000)));
    let exact = shape.stroke.elements().len();
    for scale in [4.0, 8.0] {
        let n = simplify(&shape, 0.25 / scale).stroke.elements().len();
        assert!(n * 5 < exact, "×{scale}: {n} из {exact}");
    }
}

#[test]
fn simplified_curvy_path_stays_within_the_tolerance() {
    use kurbo::ParamCurve;
    // Кривые, которые остаются и которые становятся хордами, — на одном контуре.
    // Сеть меньше порога `simplify` — путь упрощается напрямую.
    let shape = network_shape(&vector(ferrum_doc::fixture::curvy_network(5000, 100.0)));
    for tol in [0.1, 0.5, 2.0] {
        let s = simplify_path(&shape.stroke, tol);
        let mut worst = 0.0f64;
        for seg in shape.stroke.segments().step_by(3) {
            for k in 0..5 {
                let q = seg.eval(k as f64 / 5.0);
                let d = distance_to(&s, q, 2.0 * tol + 1.0).unwrap_or(f64::MAX);
                worst = worst.max(d);
            }
        }
        assert!(worst <= tol, "допуск {tol}: отклонение {worst}");
    }
}

#[test]
fn curvy_network_survives_tiny_sizes() {
    use ferrum_doc::fixture::curvy_network;
    assert!(curvy_network(0, 10.0).points.is_empty());
    assert_eq!(curvy_network(2, 10.0).segments.len(), 1);
    let n = curvy_network(3, 10.0);
    assert_eq!((n.segments.len(), n.regions.len()), (3, 1));
}

#[test]
fn wavy_network_survives_tiny_sizes() {
    use ferrum_doc::fixture::wavy_network;
    assert!(wavy_network(0).points.is_empty() && wavy_network(0).segments.is_empty());
    let one = wavy_network(1);
    assert_eq!((one.points.len(), one.segments.len()), (1, 0), "без петли");
    let two = wavy_network(2);
    assert_eq!((two.points.len(), two.segments.len()), (2, 1));
    assert!(two.regions.is_empty());
    assert_eq!(wavy_network(3).segments.len(), 3);
}
