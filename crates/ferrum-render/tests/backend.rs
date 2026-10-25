use std::sync::Arc;

use ferrum_render::{Camera, CpuBackend, ImageKey, ImagePaint, Pixmap, RenderBackend, Rgba};
use kurbo::{Affine, Point, Rect, Shape, Size};
use vello_cpu::peniko::Extend;

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

/// Пиксель в виде (r, g, b, a) с премультипликацией, как хранит `Pixmap`.
fn px(pm: &Pixmap, x: u16, y: u16) -> (u8, u8, u8, u8) {
    let p = pm.sample(x, y);
    (p.r, p.g, p.b, p.a)
}

const TRANSPARENT: (u8, u8, u8, u8) = (0, 0, 0, 0);
const RED_PX: (u8, u8, u8, u8) = (255, 0, 0, 255);
const GREEN_PX: (u8, u8, u8, u8) = (0, 255, 0, 255);
const BLUE_PX: (u8, u8, u8, u8) = (0, 0, 255, 255);

#[test]
fn fill_rect_paints_inside_and_leaves_outside_transparent() {
    let mut b = CpuBackend::new(16, 16);
    b.set_transform(Affine::IDENTITY);
    b.set_color(Rgba::new(RED));
    b.fill_rect(&Rect::new(4.0, 4.0, 12.0, 12.0));
    let pm = b.render();
    assert_eq!((pm.width(), pm.height()), (16, 16));
    assert_eq!(px(&pm, 8, 8), RED_PX);
    assert_eq!(px(&pm, 4, 4), RED_PX);
    assert_eq!(px(&pm, 1, 1), TRANSPARENT);
    assert_eq!(px(&pm, 12, 12), TRANSPARENT);
}

#[test]
fn clip_restricts_drawing_and_pop_restores_it() {
    let mut b = CpuBackend::new(16, 16);
    b.set_transform(Affine::IDENTITY);
    b.set_color(Rgba::new(RED));
    b.push_clip(&Rect::new(0.0, 0.0, 8.0, 8.0).to_path(0.1));
    // Заливка всего холста, но внутри обрезки: рисуется только левый верхний угол.
    b.fill_rect(&Rect::new(0.0, 0.0, 16.0, 16.0));
    b.pop_clip();
    // После pop обрезки снова рисуем без ограничений.
    b.set_color(Rgba::new(GREEN));
    b.fill_rect(&Rect::new(12.0, 12.0, 16.0, 16.0));
    let pm = b.render();
    assert_eq!(px(&pm, 2, 2), RED_PX);
    assert_eq!(
        px(&pm, 10, 10),
        TRANSPARENT,
        "pixel outside the clip was painted"
    );
    assert_eq!(
        px(&pm, 14, 14),
        GREEN_PX,
        "drawing not restored after pop_clip"
    );
}

#[test]
fn reset_to_new_size_changes_rendered_size() {
    let mut b = CpuBackend::new(8, 8);
    assert_eq!(b.size(), (8, 8));
    let pm = b.render();
    assert_eq!((pm.width(), pm.height()), (8, 8));
    b.reset(16, 4);
    assert_eq!(b.size(), (16, 4));
    let pm = b.render();
    assert_eq!((pm.width(), pm.height()), (16, 4));
    // Тот же размер: reset без пересоздания контекста.
    b.reset(16, 4);
    assert_eq!(b.render().width(), 16);
}

#[test]
fn set_image_after_set_color_paints_the_image() {
    // Картинка 2×2: слева зелёный, справа синий. Масштаб ×4 — каждый пиксель
    // картинки занимает квадрат 4×4 на холсте.
    let mut img = Pixmap::new(2, 2);
    img.set_pixel(
        0,
        0,
        vello_cpu::color::PremulRgba8 {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
    );
    img.set_pixel(
        1,
        0,
        vello_cpu::color::PremulRgba8 {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    );
    img.set_pixel(
        0,
        1,
        vello_cpu::color::PremulRgba8 {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
    );
    img.set_pixel(
        1,
        1,
        vello_cpu::color::PremulRgba8 {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    );

    let mut b = CpuBackend::new(8, 8);
    b.set_transform(Affine::IDENTITY);
    // Сначала цвет, потом картинка: картинка должна заменить краску.
    b.set_color(Rgba::new(RED));
    b.set_image(&ImagePaint {
        key: ImageKey::Raster(1),
        pixmap: Arc::new(img),
        transform: Affine::scale(4.0),
        extend: Extend::Pad,
        alpha: 1.0,
    });
    b.fill_rect(&Rect::new(0.0, 0.0, 8.0, 8.0));
    let pm = b.render();
    assert_eq!(px(&pm, 1, 1), GREEN_PX);
    assert_eq!(px(&pm, 6, 1), BLUE_PX);
    assert_eq!(px(&pm, 1, 6), GREEN_PX);
    assert_eq!(px(&pm, 6, 6), BLUE_PX);
}

#[test]
fn large_coordinates_render_exact_pixels_at_1e7() {
    // Камера в 1e7 с зумом 256: квадрат 1×1 единица мира — ровно 256×256 пикселей
    // в центре вьюпорта 800×600. Проверяется полный путь камера → бэкенд → пиксели.
    let mut cam = Camera::new(Size::new(800.0, 600.0), 1.0);
    cam.set_center(Point::new(1e7, 1e7));
    cam.set_zoom(256.0);
    let (w, h) = cam.device_size();
    let mut b = CpuBackend::new(w, h);
    b.set_transform(cam.view_transform());
    b.set_color(Rgba::new(GREEN));
    b.fill_rect(&Rect::new(1e7 - 0.5, 1e7 - 0.5, 1e7 + 0.5, 1e7 + 0.5));
    let pm = b.render();
    // Экранный квадрат: x ∈ [272, 528), y ∈ [172, 428).
    assert_eq!(px(&pm, 272, 172), GREEN_PX);
    assert_eq!(px(&pm, 527, 427), GREEN_PX);
    assert_eq!(px(&pm, 271, 300), TRANSPARENT);
    assert_eq!(px(&pm, 528, 300), TRANSPARENT);
    assert_eq!(px(&pm, 400, 171), TRANSPARENT);
    assert_eq!(px(&pm, 400, 428), TRANSPARENT);
}
