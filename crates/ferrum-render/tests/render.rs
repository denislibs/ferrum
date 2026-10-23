mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch};
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
