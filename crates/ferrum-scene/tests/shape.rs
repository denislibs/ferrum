use ferrum_doc::{Color, Fill, NodeKind, Props, Stroke, StrokeAlign};
use ferrum_scene::shape::{self, Geometry};
use kurbo::{Point, Rect, Shape};

fn props(kind: NodeKind, w: f64, h: f64) -> Props {
    let mut p = Props::defaults(kind);
    p.w = w;
    p.h = h;
    p
}

fn stroke(width: f64, align: StrokeAlign) -> Stroke {
    Stroke {
        color: Color::BLACK,
        width,
        align,
    }
}

#[test]
fn rotation_is_counterclockwise_about_top_left() {
    let mut p = props(NodeKind::Rect, 100.0, 50.0);
    p.x = 10.0;
    p.y = 20.0;
    p.rotation = 90.0;
    let t = shape::local_transform(&p);
    let top_left = t * Point::new(0.0, 0.0);
    let top_right = t * Point::new(100.0, 0.0);
    assert!((top_left - Point::new(10.0, 20.0)).hypot() < 1e-9);
    // Против часовой стрелки на экране: правый край уходит вверх.
    assert!((top_right - Point::new(10.0, -80.0)).hypot() < 1e-9);
}

#[test]
fn sanitize_replaces_values_unsafe_for_rendering() {
    let mut p = props(NodeKind::Rect, f64::INFINITY, -5.0);
    p.x = f64::NAN;
    p.y = 1e300;
    p.rotation = 370.0;
    p.opacity = 2.0;
    p.corner_radius = -1.0;
    p.fills = vec![Fill {
        opacity: f64::NAN,
        ..Fill::solid(Color::BLACK)
    }];
    p.strokes = vec![stroke(f64::NAN, StrokeAlign::Center)];
    shape::sanitize(&mut p);
    assert_eq!((p.x, p.y), (0.0, shape::COORD_LIMIT));
    assert_eq!((p.w, p.h), (0.0, 0.0));
    assert!((p.rotation - 10.0).abs() < 1e-9);
    assert_eq!(p.opacity, 1.0);
    assert_eq!(p.corner_radius, 0.0);
    assert_eq!(p.fills[0].opacity, 1.0);
    assert_eq!(p.strokes[0].width, 0.0);
}

#[test]
fn rounded_rect_hit_respects_corners() {
    let mut p = props(NodeKind::Rect, 100.0, 100.0);
    p.corner_radius = 30.0;
    assert!(shape::hit(&p, Point::new(50.0, 50.0), 0.0));
    assert!(shape::hit(&p, Point::new(50.0, 1.0), 0.0));
    assert!(!shape::hit(&p, Point::new(2.0, 2.0), 0.0), "срезанный угол");
    assert!(!shape::hit(&p, Point::new(101.0, 50.0), 0.0));
}

#[test]
fn ellipse_hit_excludes_bounding_box_corners() {
    let p = props(NodeKind::Ellipse, 100.0, 50.0);
    assert!(shape::hit(&p, Point::new(50.0, 25.0), 0.0));
    assert!(shape::hit(&p, Point::new(98.0, 25.0), 0.0));
    assert!(!shape::hit(&p, Point::new(5.0, 5.0), 0.0));
}

#[test]
fn thin_line_uses_tolerance_thick_line_its_width() {
    let p = props(NodeKind::Line, 100.0, 0.0);
    assert!(shape::hit(&p, Point::new(50.0, 3.0), 4.0));
    assert!(!shape::hit(&p, Point::new(50.0, 5.0), 4.0));
    assert!(
        shape::hit(&p, Point::new(103.0, 0.0), 4.0),
        "за концом в пределах допуска"
    );
    assert!(!shape::hit(&p, Point::new(105.0, 0.0), 4.0));
    let mut thick = p.clone();
    thick.strokes = vec![stroke(20.0, StrokeAlign::Center)];
    assert!(shape::hit(&thick, Point::new(50.0, 9.0), 1.0));
    assert!(!shape::hit(&thick, Point::new(50.0, 11.0), 1.0));
}

#[test]
fn stroke_only_shape_is_hit_on_its_stroke_band() {
    let mut p = props(NodeKind::Rect, 100.0, 100.0);
    p.fills.clear();
    p.strokes = vec![stroke(10.0, StrokeAlign::Outside)];
    assert!(
        !shape::hit(&p, Point::new(50.0, 50.0), 1.0),
        "пустая середина"
    );
    assert!(
        shape::hit(&p, Point::new(50.0, -8.0), 1.0),
        "внешняя обводка"
    );
    assert!(!shape::hit(&p, Point::new(50.0, -12.0), 1.0));
    p.strokes = vec![stroke(10.0, StrokeAlign::Inside)];
    assert!(
        shape::hit(&p, Point::new(50.0, 8.0), 1.0),
        "внутренняя обводка"
    );
    assert!(!shape::hit(&p, Point::new(50.0, -3.0), 1.0));
    // Тонкая обводка ловится в полосе допуска.
    p.strokes = vec![stroke(1.0, StrokeAlign::Center)];
    assert!(shape::hit(&p, Point::new(50.0, 3.5), 4.0));
}

#[test]
fn frame_without_fills_is_hit_inside() {
    let mut p = props(NodeKind::Frame, 100.0, 100.0);
    p.fills.clear();
    assert!(shape::hit(&p, Point::new(50.0, 50.0), 0.0));
    assert!(!shape::hit(
        &props(NodeKind::Group, 0.0, 0.0),
        Point::ZERO,
        10.0
    ));
}

#[test]
fn stroke_ring_follows_alignment() {
    let p = props(NodeKind::Rect, 100.0, 100.0);
    let g = shape::geometry(&p);
    let ring = |align| g.stroke_ring(&stroke(10.0, align)).unwrap();
    let inside = ring(StrokeAlign::Inside);
    let outside = ring(StrokeAlign::Outside);
    let center = ring(StrokeAlign::Center);
    let close = |a: Rect, b: Rect| (a.x0 - b.x0).abs() + (a.x1 - b.x1).abs() < 1e-6;
    assert!(close(
        inside.bounding_box(),
        Rect::new(0.0, 0.0, 100.0, 100.0)
    ));
    assert!(close(
        outside.bounding_box(),
        Rect::new(-10.0, -10.0, 110.0, 110.0)
    ));
    assert!(close(
        center.bounding_box(),
        Rect::new(-5.0, -5.0, 105.0, 105.0)
    ));
    // Кольцо заливается по even-odd: середина пустая, полоса закрашена.
    let filled = |path: &kurbo::BezPath, pt: (f64, f64)| path.winding(Point::from(pt)) % 2 != 0;
    assert!(filled(&inside, (5.0, 50.0)));
    assert!(!filled(&inside, (15.0, 50.0)));
    assert!(filled(&outside, (-5.0, 50.0)));
    assert!(!filled(&outside, (5.0, 50.0)));
    assert_eq!(g.stroke_outset(&[stroke(10.0, StrokeAlign::Outside)]), 10.0);
    assert_eq!(g.stroke_outset(&[stroke(10.0, StrokeAlign::Inside)]), 0.0);
    // Обводка толще формы: дыры нет, кольцо — сплошная заливка.
    let small = shape::geometry(&props(NodeKind::Rect, 6.0, 6.0));
    let solid = small
        .stroke_ring(&stroke(10.0, StrokeAlign::Inside))
        .unwrap();
    assert!(filled(&solid, (3.0, 3.0)));
    assert_eq!(
        shape::geometry(&props(NodeKind::Group, 1.0, 1.0)),
        Geometry::None
    );
}

#[test]
fn default_image_is_hit_over_its_whole_rect() {
    // Изображение без заливок и обводок попадает внутрь прямоугольника, как фрейм.
    let p = Props::defaults(NodeKind::Image);
    assert!(p.fills.is_empty() && p.strokes.is_empty());
    assert!(shape::hit(&p, Point::new(50.0, 50.0), 0.0));
    assert!(shape::hit(&p, Point::new(1.0, 1.0), 0.0));
    assert!(!shape::hit(&p, Point::new(150.0, 50.0), 0.0));
    assert!(!shape::hit(&p, Point::new(50.0, -5.0), 0.0));
}

#[test]
fn negative_corner_radius_is_clamped_to_plain_rect() {
    // Отрицательный радиус не должен менять контур и расходиться с попаданием.
    let mut p = props(NodeKind::Rect, 100.0, 100.0);
    p.corner_radius = -30.0;
    let g = shape::geometry(&p);
    let fill = g.fill_path().unwrap();
    let close = |a: Rect, b: Rect| (a.x0 - b.x0).abs() + (a.x1 - b.x1).abs() < 1e-6;
    assert!(close(
        fill.bounding_box(),
        Rect::new(0.0, 0.0, 100.0, 100.0)
    ));
    // Угол прямоугольника без скругления попадает, как и любая внутренняя точка.
    assert!(shape::hit(&p, Point::new(2.0, 2.0), 0.0));
    assert!(g.contains(Point::new(2.0, 2.0)));
}
