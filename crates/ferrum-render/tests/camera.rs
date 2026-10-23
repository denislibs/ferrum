use ferrum_render::{Camera, MAX_DEVICE_PIXELS, MAX_DEVICE_SIDE, MAX_ZOOM, MIN_ZOOM};
use kurbo::{Point, Rect, Size, Vec2};

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-9
}

const BAD: [f64; 3] = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];

#[test]
fn zoom_is_clamped_to_2_and_25600_percent() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.set_zoom(1000.0);
    assert_eq!(c.zoom(), MAX_ZOOM);
    c.set_zoom(0.0001);
    assert_eq!(c.zoom(), MIN_ZOOM);
    // NaN не меняет зум: остаётся прежний (здесь — минимум).
    c.set_zoom(f64::NAN);
    assert_eq!(c.zoom(), MIN_ZOOM);
    assert_eq!((MIN_ZOOM, MAX_ZOOM), (0.02, 256.0));
}

#[test]
fn screen_and_world_round_trip() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 2.0);
    c.set_center(Point::new(1000.0, -500.0));
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
    c.set_center(Point::new(50.0, 50.0));
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
fn view_transform_precision_at_1e7() {
    // Только f64-матрица: соседние точки на 1e7 различаются на ожидаемый шаг.
    // Пиксельная проверка этого пути — в tests/backend.rs.
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.set_center(Point::new(1e7, 1e7));
    c.set_zoom(256.0);
    let a = c.view_transform() * Point::new(1e7 + 0.01, 1e7);
    let b = c.view_transform() * Point::new(1e7 + 0.02, 1e7);
    assert!(((b.x - a.x) - 2.56).abs() < 1e-4);
}

#[test]
fn non_finite_zoom_is_clamped_or_ignored() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.set_zoom(2.0);
    c.set_zoom(f64::INFINITY);
    assert_eq!(c.zoom(), MAX_ZOOM);
    c.set_zoom(2.0);
    c.set_zoom(f64::NEG_INFINITY);
    assert_eq!(c.zoom(), MIN_ZOOM);
    c.set_zoom(2.0);
    c.set_zoom(0.0);
    assert_eq!(c.zoom(), MIN_ZOOM);
    c.set_zoom(2.0);
    c.set_zoom(-5.0);
    assert_eq!(c.zoom(), MIN_ZOOM);
    c.set_zoom(2.0);
    c.set_zoom(f64::NAN);
    assert_eq!(c.zoom(), 2.0);
}

#[test]
fn non_finite_setters_keep_state() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 2.0);
    c.set_center(Point::new(5.0, 6.0));
    let before = c.clone();
    for v in BAD {
        c.set_center(Point::new(v, 0.0));
        c.set_center(Point::new(0.0, v));
        c.set_viewport(Size::new(v, 100.0));
        c.set_viewport(Size::new(100.0, v));
        c.set_dpr(v);
        assert_eq!(c, before, "bad value {v} changed the camera");
    }
    // Отрицательные размеры и dpr <= 0 тоже не принимаются.
    c.set_viewport(Size::new(-1.0, 100.0));
    c.set_dpr(0.0);
    c.set_dpr(-2.0);
    assert_eq!(c, before);
    // Конструктор: некорректный dpr становится 1, вьюпорт — пустым.
    let d = Camera::new(Size::new(f64::NAN, 10.0), f64::NAN);
    assert_eq!(d.dpr(), 1.0);
    assert_eq!(d.viewport(), Size::ZERO);
}

#[test]
fn non_finite_zoom_at_keeps_state() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.set_center(Point::new(50.0, 50.0));
    c.set_zoom(3.0);
    let before = c.clone();
    for v in BAD {
        c.zoom_at(Point::new(v, 100.0), 2.0);
        c.zoom_at(Point::new(100.0, v), 2.0);
        c.zoom_at(Point::new(100.0, 100.0), v);
    }
    c.zoom_at(Point::new(100.0, 100.0), 0.0);
    c.zoom_at(Point::new(100.0, 100.0), -1.0);
    assert_eq!(c, before);
    assert_eq!(c, before);
}

#[test]
fn non_finite_pan_keeps_state() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.set_center(Point::new(7.0, 8.0));
    let before = c.clone();
    for v in BAD {
        c.pan_by(Vec2::new(v, 0.0));
        c.pan_by(Vec2::new(0.0, v));
    }
    assert_eq!(c, before);
}

#[test]
fn non_finite_fit_is_noop() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.set_center(Point::new(7.0, 8.0));
    c.set_zoom(3.0);
    let before = c.clone();
    for v in BAD {
        c.fit(Rect::new(0.0, 0.0, v, 10.0), 0.0);
        c.fit(Rect::new(v, 0.0, 10.0, 10.0), 0.0);
        c.fit(Rect::new(0.0, 0.0, 10.0, 10.0), v);
    }
    // Пустой прямоугольник тоже ничего не меняет.
    c.fit(Rect::new(5.0, 5.0, 5.0, 10.0), 0.0);
    c.fit(Rect::new(0.0, 0.0, 0.0, 0.0), 0.0);
    assert_eq!(c, before);
}

#[test]
fn oversized_surface_is_capped_and_corners_map_to_device_corners() {
    // Ширина 20000 CSS-пикселей при dpr 1 не влезает в 16384 пикселя по стороне.
    let mut c = Camera::new(Size::new(20000.0, 100.0), 1.0);
    c.set_zoom(1.0);
    let (dw, dh) = c.device_size();
    assert_eq!(dw as u32, MAX_DEVICE_SIDE);
    assert!(dh >= 1);
    let t = c.view_transform();
    let top_left = t * c.screen_to_world(Point::new(0.0, 0.0));
    let bottom_right = t * c.screen_to_world(Point::new(20000.0, 100.0));
    // Левый верхний угол вьюпорта — на левом верхнем угле холста, правый нижний —
    // на правом нижнем, без обрезки по любой оси.
    assert!(top_left.to_vec2().hypot() < 1e-6);
    assert!((bottom_right.x - dw as f64).abs() < 1e-3);
    assert!(bottom_right.y <= dh as f64 + 1e-9);
    assert!(bottom_right.y >= dh as f64 - 1.0);
}

#[test]
fn area_cap_keeps_device_pixels_under_limit() {
    // 8000×8000 CSS при dpr 2 — 256 Мпикс без ограничения: площадь должна упасть.
    let c = {
        let mut c = Camera::new(Size::new(8000.0, 8000.0), 2.0);
        c.set_zoom(1.0);
        c
    };
    let (w, h) = c.device_size();
    assert!(w as u64 * h as u64 <= MAX_DEVICE_PIXELS);
    assert!(w as u32 <= MAX_DEVICE_SIDE && h as u32 <= MAX_DEVICE_SIDE);
    // Масштаб уменьшен, но вьюпорт по-прежнему покрыт холстом.
    let t = c.view_transform();
    let corner = t * c.screen_to_world(Point::new(8000.0, 8000.0));
    assert!(corner.x <= w as f64 + 1e-9 && corner.y <= h as f64 + 1e-9);
    assert!(corner.x >= w as f64 - 1.0 && corner.y >= h as f64 - 1.0);
}

#[test]
fn uncapped_surface_is_unchanged() {
    // Обычный случай без ограничения: dpr применяется как есть.
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.5);
    c.set_zoom(1.0);
    assert_eq!(c.device_size(), (1200, 900));
    assert_eq!(c.device_scale(), 1.5);
}
