mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch, ScaleMode, Stroke as DocStroke, StrokeAlign};
use ferrum_render::{
    decode_png, Camera, CpuBackend, ExportError, ImagePaint, NoImages, Pixmap, RenderBackend,
    Renderer, Rgba, LOD_RECT_PX, RASTER_BUILDS_PER_FRAME,
};
use ferrum_scene::Scene;
use kurbo::{Affine, BezPath, Point, Rect, Size, Stroke};
use vello_cpu::peniko::Fill as FillRule;

fn solid(r: u8, g: u8, b: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, 255))])
}

/// Сетка фреймов `cols × rows` по `kids` прямоугольников — уменьшенная копия
/// страницы из бенчмарка.
fn grid(cols: usize, rows: usize, kids: usize) -> (Page, Vec<ferrum_doc::NodeId>) {
    let mut p = Page::new();
    let mut frames = Vec::new();
    for i in 0..cols * rows {
        let f = p.add(
            None,
            NodeKind::Frame,
            PropsPatch {
                fills: solid(240, 240, 240),
                ..rect(
                    (i % cols) as f64 * 220.0,
                    (i / cols) as f64 * 220.0,
                    200.0,
                    200.0,
                )
            },
        );
        for k in 0..kids {
            p.add(
                Some(f),
                NodeKind::Rect,
                PropsPatch {
                    fills: solid((k * 40 % 256) as u8, 100, 200),
                    ..rect((k % 5) as f64 * 40.0, (k / 5) as f64 * 40.0, 30.0, 30.0)
                },
            );
        }
        frames.push(f);
    }
    (p, frames)
}

fn draw(renderer: &mut Renderer, scene: &Scene, camera: &Camera) -> ferrum_render::FrameStats {
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    renderer.draw_scene(&mut backend, scene, camera, &NoImages)
}

#[test]
fn display_lists_rebuild_only_for_changed_nodes() {
    let (mut p, frames) = grid(2, 1, 4);
    let mut scene = p.scene();
    let cam = camera(440.0, 220.0);
    let mut r = Renderer::new();
    let first = draw(&mut r, &scene, &cam);
    assert_eq!(first.drawn, 10);
    assert_eq!(first.lists_built, 10);
    // Камера сдвинулась — списки те же.
    let mut moved = cam.clone();
    moved.pan_by(kurbo::Vec2::new(5.0, 0.0));
    assert_eq!(draw(&mut r, &scene, &moved).lists_built, 0);
    // Перемещение узла тоже не трогает его список: он в локальных координатах.
    p.edit(frames[0], rect(3.0, 0.0, 200.0, 200.0));
    p.update(&mut scene);
    assert_eq!(draw(&mut r, &scene, &cam).lists_built, 0);
    // Смена цвета — пересобирается только этот узел.
    p.edit(
        frames[1],
        PropsPatch {
            fills: solid(1, 2, 3),
            ..Default::default()
        },
    );
    p.update(&mut scene);
    assert_eq!(draw(&mut r, &scene, &cam).lists_built, 1);
}

#[test]
fn offscreen_subtrees_are_culled_whole() {
    let (mut p, _) = grid(10, 10, 10);
    let scene = p.scene();
    let mut r = Renderer::new();
    // Вьюпорт на один фрейм: обход не заходит в остальные 99 поддеревьев.
    let stats = draw(&mut r, &scene, &camera(200.0, 200.0));
    assert_eq!(stats.drawn, 11);
    assert_eq!(stats.culled, 99);
    assert_eq!(stats.visited, 100 + 10);
}

#[test]
fn tiny_nodes_become_rects_and_specks_disappear() {
    let (mut p, _) = grid(2, 1, 10);
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(400.0, 200.0), 1.0);
    cam.set_center(Point::new(210.0, 100.0));
    // Дети 30 px: при зуме 0.08 — 2.4 px, прямоугольники LOD.
    cam.set_zoom(0.08);
    assert!(30.0 * cam.zoom() < LOD_RECT_PX);
    let mut r = Renderer::new();
    let mut backend = CpuBackend::new(400, 200);
    // Бэкенд без изображений: проверяем LOD узлов, а не растровый кэш.
    let stats = r.draw_scene(&mut NoRaster(&mut backend), &scene, &cam, &NoImages);
    assert_eq!(stats.lod_rects, 20);
    assert_eq!(stats.drawn, 2);
    // Узел 20 px при минимальном зуме 2% — 0.4 px: не рисуется вовсе.
    let mut tiny = Page::new();
    tiny.add(None, NodeKind::Rect, rect(0.0, 0.0, 20.0, 20.0));
    cam.set_zoom(0.0);
    cam.set_center(Point::ZERO);
    let stats = r.draw_scene(&mut NoRaster(&mut backend), &tiny.scene(), &cam, &NoImages);
    assert_eq!((stats.drawn, stats.lod_rects, stats.too_small), (0, 0, 1));
}

/// Обёртка, которая выдаёт бэкенд за не умеющий изображения.
struct NoRaster<'a>(&'a mut CpuBackend);

impl RenderBackend for NoRaster<'_> {
    fn begin_frame(&mut self, w: u16, h: u16) {
        self.0.begin_frame(w, h)
    }
    fn size(&self) -> (u16, u16) {
        self.0.size()
    }
    fn set_transform(&mut self, t: Affine) {
        self.0.set_transform(t)
    }
    fn set_color(&mut self, c: Rgba) {
        self.0.set_color(c)
    }
    fn set_image(&mut self, i: &ImagePaint) {
        self.0.set_image(i)
    }
    fn fill_path(&mut self, p: &BezPath, r: FillRule) {
        self.0.fill_path(p, r)
    }
    fn fill_rect(&mut self, r: &Rect) {
        self.0.fill_rect(r)
    }
    fn stroke_path(&mut self, p: &BezPath, s: &Stroke) {
        self.0.stroke_path(p, s)
    }
    fn push_clip(&mut self, p: &BezPath) {
        self.0.push_clip(p)
    }
    fn pop_clip(&mut self) {
        self.0.pop_clip()
    }
    fn push_opacity_layer(&mut self, a: f32) {
        self.0.push_opacity_layer(a)
    }
    fn pop_layer(&mut self) {
        self.0.pop_layer()
    }
    fn supports_images(&self) -> bool {
        false
    }
}

#[test]
fn raster_cache_matches_live_drawing_and_follows_changes() {
    let (mut p, frames) = grid(8, 8, 10);
    let mut scene = p.scene();
    let mut cam = Camera::new(Size::new(400.0, 400.0), 1.0);
    cam.fit(scene.bounds().unwrap(), 10.0);
    let (w, h) = cam.device_size();
    let mut r = Renderer::new();
    // Проверяется счётный предел построений за кадр: предел по времени выключен,
    // иначе число кадров прогрева зависело бы от скорости машины.
    r.set_raster_build_time(None);

    let mut live = CpuBackend::new(w, h);
    r.draw_scene(&mut NoRaster(&mut live), &scene, &cam, &NoImages);
    let live = live.render();

    // Кэш строится не больше `RASTER_BUILDS_PER_FRAME` фреймов за кадр.
    let mut frames_to_warm = 0;
    let cached = loop {
        let mut b = CpuBackend::new(w, h);
        let stats = r.draw_scene(&mut b, &scene, &cam, &NoImages);
        assert!(stats.raster_builds <= RASTER_BUILDS_PER_FRAME);
        if stats.raster_builds == 0 {
            assert_eq!(stats.rasters, 64);
            break b.render();
        }
        frames_to_warm += 1;
    };
    assert_eq!(frames_to_warm, 2);
    // Растр с плотностью до 2× от нужной почти не отличается от рисования узлами.
    let diff: u64 = live
        .data()
        .iter()
        .zip(cached.data())
        .map(|(a, b)| {
            a.r.abs_diff(b.r)
                .max(a.g.abs_diff(b.g))
                .max(a.b.abs_diff(b.b)) as u64
        })
        .sum();
    let mean = diff as f64 / (w as f64 * h as f64);
    assert!(mean < 4.0, "среднее отличие {mean}");

    // Правка ребёнка перестраивает растр только его фрейма.
    let kid = scene.at(scene.slot(frames[5]).unwrap()).children[0];
    let kid = scene.at(kid).id;
    p.edit(
        kid,
        PropsPatch {
            fills: solid(0, 0, 0),
            ..Default::default()
        },
    );
    p.update(&mut scene);
    let stats = draw(&mut r, &scene, &cam);
    assert_eq!(stats.raster_builds, 1);
    // Немного приблизили — растры годятся (плотность с запасом до 2×).
    cam.set_zoom(cam.zoom() * 1.1);
    assert_eq!(draw(&mut r, &scene, &cam).raster_builds, 0);
    // Отдалили вдвое — растры перестраиваются в меньшей плотности.
    cam.set_zoom(cam.zoom() * 0.45);
    assert_eq!(draw(&mut r, &scene, &cam).raster_builds, 32);
}

#[test]
fn export_is_pixel_exact_at_each_scale() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(10, 20, 30),
            ..rect(100.0, 50.0, 393.0, 852.0)
        },
    );
    p.add(Some(f), NodeKind::Rect, rect(-50.0, 0.0, 100.0, 100.0));
    let scene = p.scene();
    let mut r = Renderer::new();
    for (scale, w, h) in [(1.0, 393, 852), (2.0, 786, 1704), (3.0, 1179, 2556)] {
        let png = r.export_png(&scene, f, scale, &NoImages).unwrap();
        // Размер из заголовка PNG.
        let pw = u32::from_be_bytes(png[16..20].try_into().unwrap());
        let ph = u32::from_be_bytes(png[20..24].try_into().unwrap());
        assert_eq!((pw, ph), (w, h), "@{scale}x");
    }
    let img = r.export_pixmap(&scene, f, 1.0, &NoImages).unwrap();
    // Ребёнок, выступающий за левый край, обрезан краем картинки; фон фрейма — в углу.
    assert_eq!(px(&img, 392, 851), [10, 20, 30, 255]);
    assert_eq!(px(&img, 10, 10), [217, 217, 217, 255]);
}

#[test]
fn export_errors_are_reported() {
    let mut p = Page::new();
    let big = p.add(None, NodeKind::Frame, rect(0.0, 0.0, 10_000.0, 10.0));
    let empty = p.add(None, NodeKind::Group, PropsPatch::default());
    let scene = p.scene();
    let mut r = Renderer::new();
    assert_eq!(
        r.export_png(&scene, big, 2.0, &NoImages),
        Err(ExportError::TooLarge(20_000, 20))
    );
    assert_eq!(
        r.export_png(&scene, big, 0.0, &NoImages),
        Err(ExportError::BadScale)
    );
    assert_eq!(
        r.export_png(&scene, empty, 1.0, &NoImages),
        Err(ExportError::Empty)
    );
    let other = Page::new().add(None, NodeKind::Rect, PropsPatch::default());
    assert_eq!(
        r.export_png(&scene, other, 1.0, &NoImages),
        Err(ExportError::NotFound)
    );
}

#[test]
fn export_ignores_ancestor_opacity() {
    let mut p = Page::new();
    let g = p.add(
        None,
        NodeKind::Group,
        PropsPatch {
            opacity: Some(0.2),
            ..Default::default()
        },
    );
    let leaf = p.add(
        Some(g),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let img = Renderer::new()
        .export_pixmap(&p.scene(), leaf, 1.0, &NoImages)
        .unwrap();
    assert_eq!(px(&img, 5, 5), [255, 0, 0, 255]);
}

#[test]
fn missing_image_draws_placeholder() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Image,
        PropsPatch {
            fills: Some(vec![Fill::image("нет-такого")]),
            ..rect(0.0, 0.0, 20.0, 20.0)
        },
    );
    let img = render(&p.scene(), &camera(20.0, 20.0));
    assert_px(&img, 10, 10, [217, 217, 217], 1);
    // Та же картинка, если поставщик её знает, рисуется ею.
    let mut red = Pixmap::new(1, 1);
    red.set_pixel(
        0,
        0,
        vello_cpu::color::PremulRgba8 {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
    );
    let images: HashMap<String, Arc<Pixmap>> = [("нет-такого".to_string(), Arc::new(red))].into();
    let img = render_with(
        &mut Renderer::new(),
        &p.scene(),
        &camera(20.0, 20.0),
        &images,
    );
    assert_px(&img, 10, 10, [255, 0, 0], 1);
}

#[test]
fn decode_png_rejects_garbage_and_huge_headers() {
    let mut img = Pixmap::new(3, 2);
    img.set_pixel(
        1,
        1,
        vello_cpu::color::PremulRgba8 {
            r: 9,
            g: 8,
            b: 7,
            a: 255,
        },
    );
    let png = img.into_png().unwrap();
    let back = decode_png(&png).unwrap();
    assert_eq!((back.width(), back.height()), (3, 2));
    assert!(decode_png(b"not a png").is_none());
    let mut huge = png.clone();
    huge[16..20].copy_from_slice(&60_000u32.to_be_bytes());
    assert!(
        decode_png(&huge).is_none(),
        "размер из заголовка проверяется до декодирования"
    );
}

fn stroke(r: u8, g: u8, b: u8, width: f64, align: StrokeAlign) -> DocStroke {
    DocStroke {
        color: Color::rgba(r, g, b, 255),
        width,
        align,
    }
}

/// Одна красная картинка 1×1 для заливок.
fn red_image() -> Arc<Pixmap> {
    let mut red = Pixmap::new(1, 1);
    red.set_pixel(
        0,
        0,
        vello_cpu::color::PremulRgba8 {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
    );
    Arc::new(red)
}

#[test]
fn child_opacity_change_reaches_raster_cache() {
    // Прозрачность не входит в версию узла, но растр фрейма должен о ней узнать.
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(255, 255, 255),
            ..rect(0.0, 0.0, 200.0, 200.0)
        },
    );
    let kid = p.add(
        Some(f),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 0, 0),
            ..rect(50.0, 50.0, 100.0, 100.0)
        },
    );
    let mut scene = p.scene();
    let mut cam = Camera::new(Size::new(40.0, 40.0), 1.0);
    cam.set_center(Point::new(100.0, 100.0));
    cam.set_zoom(0.2);
    let mut r = Renderer::new();
    let before = render_with(&mut r, &scene, &cam, &NoImages);
    assert_px(&before, 20, 20, [0, 0, 0], 2);
    p.edit(
        kid,
        PropsPatch {
            opacity: Some(0.5),
            ..Default::default()
        },
    );
    p.update(&mut scene);
    let after = render_with(&mut r, &scene, &cam, &NoImages);
    assert_px(&after, 20, 20, [128, 128, 128], 4);
    // И собственная прозрачность контейнера верхнего уровня.
    p.edit(
        f,
        PropsPatch {
            opacity: Some(0.5),
            ..Default::default()
        },
    );
    p.update(&mut scene);
    let faded = render_with(&mut r, &scene, &cam, &NoImages);
    assert_px(&faded, 20, 20, [191, 191, 191], 4);
}

#[test]
fn image_arriving_after_raster_is_shown() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(255, 255, 255),
            ..rect(0.0, 0.0, 200.0, 200.0)
        },
    );
    p.add(
        Some(f),
        NodeKind::Image,
        PropsPatch {
            fills: Some(vec![Fill::image("позже")]),
            ..rect(50.0, 50.0, 100.0, 100.0)
        },
    );
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(40.0, 40.0), 1.0);
    cam.set_center(Point::new(100.0, 100.0));
    cam.set_zoom(0.2);
    let mut r = Renderer::new();
    let first = render_with(&mut r, &scene, &cam, &NoImages);
    assert_px(&first, 20, 20, [217, 217, 217], 2);
    let images: HashMap<String, Arc<Pixmap>> = [("позже".to_string(), red_image())].into();
    let second = render_with(&mut r, &scene, &cam, &images);
    assert_px(&second, 20, 20, [255, 0, 0], 2);
}

#[test]
fn leaf_opacity_applies_to_the_whole_leaf_not_each_paint() {
    // Заливка и внутренняя обводка при прозрачности 0.5: внутри только синяя
    // заливка на белом, обводка не просвечивает сквозь неё.
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 0, 255),
            strokes: Some(vec![stroke(255, 0, 0, 4.0, StrokeAlign::Inside)]),
            opacity: Some(0.5),
            ..rect(0.0, 0.0, 40.0, 40.0)
        },
    );
    let img = render(&p.scene(), &camera(40.0, 40.0));
    assert_px(&img, 20, 20, [128, 128, 255], 2);
    assert_px(&img, 1, 20, [255, 128, 128], 2);
    // Несколько заливок: нижняя не просвечивает через верхнюю.
    let mut q = Page::new();
    q.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(vec![
                Fill::solid(Color::rgba(255, 0, 0, 255)),
                Fill::solid(Color::rgba(0, 0, 255, 255)),
            ]),
            opacity: Some(0.5),
            ..rect(0.0, 0.0, 40.0, 40.0)
        },
    );
    let img = render(&q.scene(), &camera(40.0, 40.0));
    assert_px(&img, 20, 20, [128, 128, 255], 2);
}

#[test]
fn hidden_node_export_is_empty() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            visible: Some(false),
            ..rect(0.0, 0.0, 50.0, 50.0)
        },
    );
    assert_eq!(
        Renderer::new().export_png(&p.scene(), f, 1.0, &NoImages),
        Err(ExportError::Empty)
    );
}

#[test]
fn export_rejects_too_many_pixels() {
    // Каждая сторона в пределах, но площадь больше MAX_DEVICE_PIXELS (64 Mpx).
    let mut p = Page::new();
    let f = p.add(None, NodeKind::Frame, rect(0.0, 0.0, 9000.0, 9000.0));
    assert_eq!(
        Renderer::new().export_png(&p.scene(), f, 1.0, &NoImages),
        Err(ExportError::TooLarge(9000, 9000))
    );
}

#[test]
fn image_in_zero_sized_node_is_skipped() {
    for mode in [ScaleMode::Fit, ScaleMode::Fill, ScaleMode::Tile] {
        for (w, h) in [(0.0, 20.0), (20.0, 0.0), (0.0, 0.0)] {
            let mut p = Page::new();
            p.add(
                None,
                NodeKind::Rect,
                PropsPatch {
                    fills: solid(0, 255, 0),
                    ..rect(0.0, 0.0, 20.0, 20.0)
                },
            );
            let mut fill = Fill::image("к");
            fill.scale_mode = mode;
            p.add(
                None,
                NodeKind::Image,
                PropsPatch {
                    fills: Some(vec![fill]),
                    ..rect(0.0, 0.0, w, h)
                },
            );
            let images: HashMap<String, Arc<Pixmap>> = [("к".to_string(), red_image())].into();
            let img = render_with(
                &mut Renderer::new(),
                &p.scene(),
                &camera(20.0, 20.0),
                &images,
            );
            // Ничего красного: вырожденный узел картинку не рисует.
            assert_px(&img, 10, 10, [0, 255, 0], 1);
        }
    }
}

#[test]
fn frame_stroke_is_drawn_above_children() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(255, 255, 255),
            strokes: Some(vec![stroke(0, 0, 0, 10.0, StrokeAlign::Inside)]),
            ..rect(0.0, 0.0, 100.0, 100.0)
        },
    );
    p.add(
        Some(f),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0),
            ..rect(0.0, 40.0, 60.0, 20.0)
        },
    );
    let img = render(&p.scene(), &camera(100.0, 100.0));
    // Ребёнок перекрывает край фрейма: обводка поверх него, внутри — ребёнок.
    assert_px(&img, 5, 50, [0, 0, 0], 1);
    assert_px(&img, 30, 50, [255, 0, 0], 1);
    assert_px(&img, 30, 20, [255, 255, 255], 1);
}

#[test]
fn raster_cache_respects_byte_budget() {
    let (mut p, _) = grid(8, 8, 10);
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(400.0, 400.0), 1.0);
    cam.fit(scene.bounds().unwrap(), 10.0);
    let warm = |r: &mut Renderer| {
        for _ in 0..4 {
            draw(r, &scene, &cam);
        }
    };
    // Прогрев за фиксированное число кадров — только со счётным пределом.
    let mut free = Renderer::new();
    free.set_raster_build_time(None);
    warm(&mut free);
    let all = free.raster_cache_bytes();
    assert!(all > 0);
    assert_eq!(draw(&mut free, &scene, &cam).raster_builds, 0);
    // Бюджет на четверть: кэш не растёт выше, фреймы перестраиваются.
    let limit = all / 4;
    let mut small = Renderer::with_raster_cache_bytes(limit);
    small.set_raster_build_time(None);
    warm(&mut small);
    assert!(small.raster_cache_bytes() <= limit);
    assert!(draw(&mut small, &scene, &cam).raster_builds > 0);
    assert!(small.raster_cache_bytes() <= limit);
}

/// Медиана длительностей (для замеров времени кадра в тестах).
fn median_of(mut d: Vec<std::time::Duration>) -> std::time::Duration {
    d.sort();
    d[d.len() / 2]
}

#[test]
fn reused_backend_starts_each_frame_clean() {
    // Один бэкенд на 300 кадров, как в приложении: прошлые кадры не должны
    // оставаться под новым (иначе время кадра растёт, а vello_cpu через сотню-другую
    // кадров падает на переполнении индекса альфы).
    let (mut p, _) = grid(3, 3, 10);
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(160.0, 160.0), 1.0);
    cam.fit(scene.bounds().unwrap(), 0.0);
    cam.set_zoom(cam.zoom() * 1.5);
    let (w, h) = cam.device_size();
    let mut backend = CpuBackend::new(w, h);
    let mut r = Renderer::new();
    let overlay = ferrum_render::Overlay::default();
    let mut first: Option<Pixmap> = None;
    let mut last = None;
    let mut times = Vec::new();
    for _ in 0..300 {
        let t = std::time::Instant::now();
        r.render(&mut backend, &scene, &cam, &overlay, &NoImages, Some(WHITE));
        let img = backend.render();
        times.push(t.elapsed());
        if first.is_none() {
            first = Some(img);
        } else {
            last = Some(img);
        }
    }
    let (first, last) = (first.unwrap(), last.unwrap());
    assert!(
        first.data() == last.data(),
        "последний кадр отличается от первого"
    );
    let head = median_of(times[..20].to_vec());
    let tail = median_of(times[times.len() - 20..].to_vec());
    assert!(
        tail <= head * 2,
        "время кадра растёт: первые {head:?}, последние {tail:?}"
    );
}

/// Мера отличия двух картинок: среднее по пикселям наибольшее отличие канала.
fn mean_diff(a: &Pixmap, b: &Pixmap) -> f64 {
    let sum: u64 = a
        .data()
        .iter()
        .zip(b.data())
        .map(|(a, b)| {
            a.r.abs_diff(b.r)
                .max(a.g.abs_diff(b.g))
                .max(a.b.abs_diff(b.b)) as u64
        })
        .sum();
    sum as f64 / (a.width() as f64 * a.height() as f64)
}

/// Кадр в новый бэкенд: статистика и картинка.
fn frame_image(
    r: &mut Renderer,
    scene: &Scene,
    cam: &Camera,
) -> (ferrum_render::FrameStats, Pixmap) {
    let (w, h) = cam.device_size();
    let mut b = CpuBackend::new(w, h);
    let s = r.draw_scene(&mut b, scene, cam, &NoImages);
    (s, b.render())
}

#[test]
fn zoom_past_power_of_two_draws_previous_raster_scaled() {
    let (mut p, _) = grid(8, 8, 10);
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(200.0, 200.0), 1.0);
    cam.fit(scene.bounds().unwrap(), 10.0);
    let mut r = Renderer::new();
    r.set_raster_build_time(None);
    while draw(&mut r, &scene, &cam).raster_builds > 0 {}
    // Приблизили через степень двойки: растры прежней плотности уже не годятся.
    let z = cam.zoom();
    cam.set_zoom(2f64.powf(z.log2().floor() + 1.0) * 1.01);
    // Время на растры — ноль: за кадр строится только один (гарантия прогресса).
    r.set_raster_build_time(Some(std::time::Duration::ZERO));
    let (s, stale) = frame_image(&mut r, &scene, &cam);
    assert_eq!(s.raster_builds, 1, "{s:?}");
    // Остальные 63 фрейма — прежние растры в новом масштабе, а не поштучно узлами.
    assert_eq!(s.rasters, 64, "{s:?}");
    assert_eq!((s.drawn, s.lod_rects), (0, 0), "{s:?}");
    assert!(s.needs_another_frame, "растры ещё догоняют: {s:?}");
    // Растянутый старый растр стоит на месте фрейма: картинка почти как у живой отрисовки.
    let mut live = Renderer::new();
    let (w, h) = cam.device_size();
    let mut b = CpuBackend::new(w, h);
    live.draw_scene(&mut NoRaster(&mut b), &scene, &cam, &NoImages);
    let diff = mean_diff(&b.render(), &stale);
    assert!(diff < 6.0, "среднее отличие {diff}");
}

#[test]
fn raster_builds_are_budgeted_by_time_with_progress_guaranteed() {
    let (mut p, _) = grid(2, 2, 10);
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(100.0, 100.0), 1.0);
    cam.fit(scene.bounds().unwrap(), 10.0);
    let mut r = Renderer::new();
    r.set_raster_build_time(Some(std::time::Duration::ZERO));
    let builds: Vec<(usize, bool)> = (0..5)
        .map(|_| {
            let s = draw(&mut r, &scene, &cam);
            (s.raster_builds, s.needs_another_frame)
        })
        .collect();
    assert_eq!(
        builds,
        [(1, true), (1, true), (1, true), (1, false), (0, false)]
    );
}

#[test]
fn needs_another_frame_reports_pending_rasters_and_placeholders() {
    // Растры: 64 фрейма по 32 за кадр — второй кадр строит последние, третий готов.
    let (mut p, _) = grid(8, 8, 10);
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(400.0, 400.0), 1.0);
    cam.fit(scene.bounds().unwrap(), 10.0);
    let mut r = Renderer::new();
    r.set_raster_build_time(None);
    let pending: Vec<bool> = (0..3)
        .map(|_| draw(&mut r, &scene, &cam).needs_another_frame)
        .collect();
    assert_eq!(pending, [true, false, false]);

    // Заглушка картинки: и узлом вблизи, и внутри растра фрейма издалека.
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(255, 255, 255),
            ..rect(0.0, 0.0, 200.0, 200.0)
        },
    );
    p.add(
        Some(f),
        NodeKind::Image,
        PropsPatch {
            fills: Some(vec![Fill::image("позже")]),
            ..rect(50.0, 50.0, 100.0, 100.0)
        },
    );
    let scene = p.scene();
    let images: HashMap<String, Arc<Pixmap>> = [("позже".to_string(), red_image())].into();
    let near = camera(200.0, 200.0);
    let mut far = Camera::new(Size::new(40.0, 40.0), 1.0);
    far.set_center(Point::new(100.0, 100.0));
    far.set_zoom(0.2);
    for cam in [near, far] {
        let mut r = Renderer::new();
        let (w, h) = cam.device_size();
        let mut b = CpuBackend::new(w, h);
        for _ in 0..2 {
            let s = r.draw_scene(&mut b, &scene, &cam, &NoImages);
            assert!(
                s.needs_another_frame,
                "картинки нет — нарисована заглушка: {s:?}"
            );
        }
        let s = r.draw_scene(&mut b, &scene, &cam, &images);
        assert!(!s.needs_another_frame, "картинка пришла: {s:?}");
    }
}

#[test]
fn over_budget_cold_frames_show_their_fill_and_edited_frames_draw_nodes() {
    let (mut p, frames) = grid(2, 2, 10);
    let mut scene = p.scene();
    let mut cam = Camera::new(Size::new(100.0, 100.0), 1.0);
    cam.fit(scene.bounds().unwrap(), 10.0);
    let mut r = Renderer::new();
    r.set_raster_build_time(Some(std::time::Duration::ZERO));
    // Холодный кадр: один растр построен, у трёх фреймов растра ещё не было — пока
    // только их собственная заливка, без 30 детей поштучно.
    let s = draw(&mut r, &scene, &cam);
    assert_eq!(
        (s.raster_builds, s.rasters, s.drawn, s.lod_rects),
        (1, 1, 3, 0)
    );
    assert!(s.needs_another_frame);
    while draw(&mut r, &scene, &cam).raster_builds > 0 {}
    // Правка в двух фреймах: один растр перестроен, второй фрейм — точно, узлами
    // (его старый растр показывал бы прежнее содержимое).
    for f in &frames[..2] {
        let kid = scene.at(scene.slot(*f).unwrap()).children[0];
        p.edit(
            scene.at(kid).id,
            PropsPatch {
                fills: solid(0, 0, 0),
                ..Default::default()
            },
        );
    }
    p.update(&mut scene);
    let s = draw(&mut r, &scene, &cam);
    assert_eq!((s.raster_builds, s.rasters), (1, 3), "{s:?}");
    assert_eq!(s.drawn + s.lod_rects, 11, "{s:?}");
    assert!(s.needs_another_frame);
}
