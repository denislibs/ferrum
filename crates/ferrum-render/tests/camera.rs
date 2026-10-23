use ferrum_render::{Camera, MAX_ZOOM, MIN_ZOOM};
use kurbo::{Point, Rect, Size, Vec2};

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-9
}

#[test]
fn zoom_is_clamped_to_2_and_25600_percent() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.set_zoom(1000.0);
    assert_eq!(c.zoom(), MAX_ZOOM);
    c.set_zoom(0.0001);
    assert_eq!(c.zoom(), MIN_ZOOM);
    c.set_zoom(f64::NAN);
    assert_eq!(c.zoom(), 1.0);
    assert_eq!((MIN_ZOOM, MAX_ZOOM), (0.02, 256.0));
}

#[test]
fn screen_and_world_round_trip() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 2.0);
    c.center = Point::new(1000.0, -500.0);
    c.set_zoom(4.0);
    let world = Point::new(1010.0, -490.0);
    let screen = c.world_to_screen(world);
    assert!(near(screen, Point::new(440.0, 340.0)));
    assert!(near(c.screen_to_world(screen), world));
    // Матрица для содержимого — в пикселях устройства (dpr = 2).
    assert!(near(c.view_transform() * world, Point::new(880.0, 680.0)));
    assert_eq!(c.device_size(), (1600, 1200));
    assert_eq!(c.device_scale(), 8.0);
}

#[test]
fn zoom_at_keeps_point_under_cursor() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.center = Point::new(50.0, 50.0);
    let cursor = Point::new(600.0, 100.0);
    let before = c.screen_to_world(cursor);
    c.zoom_at(cursor, 3.0);
    assert_eq!(c.zoom(), 3.0);
    assert!(near(c.screen_to_world(cursor), before));
    // Упор в предел зума тоже не сдвигает точку под курсором.
    c.zoom_at(cursor, 1000.0);
    assert_eq!(c.zoom(), MAX_ZOOM);
    assert!((c.screen_to_world(cursor) - before).hypot() < 1e-6);
}

#[test]
fn pan_moves_content_with_cursor() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.set_zoom(2.0);
    let p = Point::new(10.0, 20.0);
    let before = c.world_to_screen(p);
    c.pan_by(Vec2::new(30.0, -10.0));
    assert!(near(c.world_to_screen(p), before + Vec2::new(30.0, -10.0)));
}

#[test]
fn fit_and_visible_rect() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.fit(Rect::new(0.0, 0.0, 1600.0, 600.0), 0.0);
    assert_eq!(c.zoom(), 0.5);
    let v = c.visible_world_rect();
    assert!((v.x0, v.x1, v.y0, v.y1) == (0.0, 1600.0, -300.0, 900.0));
}

#[test]
fn large_coordinates_stay_precise() {
    // Центр на 1e7: экранные координаты соседних точек мира различимы в f32.
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.center = Point::new(1e7, 1e7);
    c.set_zoom(256.0);
    let a = c.view_transform() * Point::new(1e7 + 0.01, 1e7);
    let b = c.view_transform() * Point::new(1e7 + 0.02, 1e7);
    assert!(((b.x - a.x) as f32 - 2.56).abs() < 1e-3);
}
