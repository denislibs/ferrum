//! Бэкенд `vello_gpu` принимает тот же кадр, что и CPU (журнал вызовов совпадает),
//! и держит картинки в атласе по ключу: загрузка при первом использовании,
//! вытеснение давно не рисовавшихся, заглушка, когда места нет.
//! Запуск: `cargo test -p ferrum-render --features gpu --test gpu`.
#![cfg(feature = "gpu")]

mod common;

use std::sync::Arc;

use common::*;
use ferrum_doc::{Fill, NodeKind, PropsPatch};
use ferrum_render::{
    atlas_config, AtlasConfig, Camera, CpuBackend, GpuBackend, GpuDevice, ImageId, ImageKey,
    ImagePaint, NoImages, Overlay, Pixmap, RenderBackend, Renderer, Rgba, EVICT_AFTER_FRAMES,
};
use kurbo::{Affine, BezPath, Point, Rect, Size, Stroke};
use vello_common::image_cache::ImageCache;
// Оба бэкенда используют один и тот же `peniko` (см. `cargo tree -d`).
use vello_cpu::peniko::{Extend, Fill as FillRule};

/// Устройство без GPU: выделяет место своим кэшем той же конфигурации — так же,
/// как атлас настоящего рендерера, — и записывает загрузки и освобождения.
struct FakeDevice {
    atlas: ImageCache,
    log: Vec<String>,
    /// Сдвиг номеров: имитирует устройство, разошедшееся с зеркалом.
    skew: u32,
}

impl FakeDevice {
    fn new(config: AtlasConfig) -> FakeDevice {
        FakeDevice {
            atlas: ImageCache::new_with_config(config),
            log: vec![],
            skew: 0,
        }
    }
}

impl GpuDevice for FakeDevice {
    fn upload(&mut self, pixmap: &Pixmap) -> Result<ImageId, String> {
        let id = self
            .atlas
            .allocate(pixmap.width(), pixmap.height(), 0)
            .map_err(|e| e.to_string())?;
        self.log
            .push(format!("upload {}x{}", pixmap.width(), pixmap.height()));
        Ok(ImageId::new(id.as_u32() + self.skew))
    }

    fn destroy(&mut self, id: ImageId) {
        self.atlas.deallocate(id);
        self.log.push(format!("destroy {}", id.as_u32()));
    }
}

fn gpu(w: u16, h: u16, config: AtlasConfig) -> GpuBackend<FakeDevice> {
    GpuBackend::new(w, h, FakeDevice::new(config), config)
}

/// Атлас из одной страницы `side × side`.
fn tiny_atlas(side: u16) -> AtlasConfig {
    AtlasConfig {
        max_atlases: 1,
        atlas_size: (side, side),
        ..atlas_config(4096)
    }
}

fn paint(key: ImageKey, w: u16, h: u16) -> ImagePaint {
    ImagePaint {
        key,
        pixmap: Arc::new(Pixmap::new(w, h)),
        transform: Affine::IDENTITY,
        extend: Extend::Pad,
        alpha: 1.0,
    }
}

/// Рисует картинку прямоугольником и сообщает, попала ли в сцену картинка (а не
/// заглушка-цвет).
fn draw_image(b: &mut GpuBackend<FakeDevice>, p: &ImagePaint) -> bool {
    b.set_image(p);
    b.fill_rect(&Rect::new(0.0, 0.0, 10.0, 10.0));
    format!("{:?}", b.scene.paint()).contains("Image")
}

/// Пересылает вызовы во внутренний бэкенд и записывает их имена; следит за
/// глубиной обрезок и слоёв.
struct Recording<B: RenderBackend> {
    inner: B,
    log: Vec<&'static str>,
    clips: i32,
    layers: i32,
}

impl<B: RenderBackend> Recording<B> {
    fn new(inner: B) -> Self {
        Recording {
            inner,
            log: vec![],
            clips: 0,
            layers: 0,
        }
    }

    fn count(&self, name: &str) -> usize {
        self.log.iter().filter(|n| **n == name).count()
    }
}

impl<B: RenderBackend> RenderBackend for Recording<B> {
    fn begin_frame(&mut self, w: u16, h: u16) {
        self.log.push("begin_frame");
        self.inner.begin_frame(w, h);
    }
    fn size(&self) -> (u16, u16) {
        self.inner.size()
    }
    fn set_transform(&mut self, t: Affine) {
        self.log.push("set_transform");
        self.inner.set_transform(t);
    }
    fn set_color(&mut self, c: Rgba) {
        self.log.push("set_color");
        self.inner.set_color(c);
    }
    fn set_image(&mut self, i: &ImagePaint) {
        self.log.push("set_image");
        self.inner.set_image(i);
    }
    fn fill_path(&mut self, p: &BezPath, r: FillRule) {
        self.log.push("fill_path");
        self.inner.fill_path(p, r);
    }
    fn fill_rect(&mut self, r: &Rect) {
        self.log.push("fill_rect");
        self.inner.fill_rect(r);
    }
    fn stroke_path(&mut self, p: &BezPath, s: &Stroke) {
        self.log.push("stroke_path");
        self.inner.stroke_path(p, s);
    }
    fn push_clip(&mut self, p: &BezPath) {
        self.log.push("push_clip");
        self.clips += 1;
        self.inner.push_clip(p);
    }
    fn pop_clip(&mut self) {
        self.log.push("pop_clip");
        self.clips -= 1;
        assert!(self.clips >= 0, "pop_clip без push_clip");
        self.inner.pop_clip();
    }
    fn push_opacity_layer(&mut self, a: f32) {
        self.log.push("push_opacity_layer");
        self.layers += 1;
        self.inner.push_opacity_layer(a);
    }
    fn pop_layer(&mut self) {
        self.log.push("pop_layer");
        self.layers -= 1;
        assert!(self.layers >= 0, "pop_layer без push_opacity_layer");
        self.inner.pop_layer();
    }
    fn supports_images(&self) -> bool {
        self.inner.supports_images()
    }
}

#[test]
fn gpu_encodes_same_calls_as_cpu() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Экран".into()),
            opacity: Some(0.5),
            rotation: Some(10.0),
            ..rect(10.0, 10.0, 200.0, 150.0)
        },
    );
    let kid = p.add(Some(f), NodeKind::Ellipse, rect(150.0, 100.0, 100.0, 80.0));
    p.add(
        Some(f),
        NodeKind::Image,
        PropsPatch {
            fills: Some(vec![Fill::image("нет")]),
            ..rect(10.0, 10.0, 50.0, 50.0)
        },
    );
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(320.0, 240.0), 2.0);
    cam.set_center(Point::new(110.0, 90.0));
    let overlay = Overlay {
        selection: vec![kid],
        hover: Some(f),
        ..Default::default()
    };
    let (w, h) = cam.device_size();
    let mut gpu = Recording::new(gpu(w, h, atlas_config(8192)));
    let mut cpu = Recording::new(CpuBackend::new(w, h));
    // С атласом GPU-бэкенд умеет изображения, а значит и растровый кэш фреймов.
    assert!(gpu.supports_images());
    let gpu_stats =
        Renderer::new().render(&mut gpu, &scene, &cam, &overlay, &NoImages, Some(WHITE));
    // CPU без растрового кэша: при этом зуме растров нет и у GPU, кадры сравнимы.
    struct NoRaster(Recording<CpuBackend>);
    impl RenderBackend for NoRaster {
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
    let mut cpu_nr = NoRaster(cpu);
    let cpu_stats =
        Renderer::new().render(&mut cpu_nr, &scene, &cam, &overlay, &NoImages, Some(WHITE));
    cpu = cpu_nr.0;

    // Слой прозрачности и обрезка повёрнутого фрейма закодированы и закрыты.
    assert_eq!(
        (gpu_stats.layers, gpu_stats.clips, gpu_stats.drawn),
        (1, 1, 3)
    );
    assert_eq!(gpu_stats, cpu_stats);
    assert_eq!(gpu.log, cpu.log, "GPU и CPU получили разные вызовы");
    assert_eq!((gpu.clips, gpu.layers), (0, 0));
    assert_eq!((gpu.count("push_clip"), gpu.count("pop_clip")), (1, 1));
    assert_eq!(
        (gpu.count("push_opacity_layer"), gpu.count("pop_layer")),
        (1, 1)
    );
    assert!(gpu.count("fill_path") + gpu.count("fill_rect") > 0);
    // Рамки выделения и наведения — обводки оверлея.
    assert!(gpu.count("stroke_path") > 0, "оверлей не нарисовал обводок");
    assert_eq!(gpu.inner.size(), (640, 480));
    // Сцена вернулась в корень: лишний `pop_layer` / `pop_clip` в ней паникует.
    // Так видно, что настоящие закрывающие вызовы дошли до `vello_gpu`.
    // Сцену после проверки пересоздаёт `reset` ниже.
    for extra in [
        (|b: &mut GpuBackend<FakeDevice>| b.scene.pop_layer()) as fn(&mut GpuBackend<FakeDevice>),
        |b| b.scene.pop_clip(),
    ] {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extra(&mut gpu.inner)));
        assert!(res.is_err(), "в сцене остались незакрытые слои/обрезки");
    }

    // Сильное отдаление: фрейм рисуется из растра, растр загружается в атлас.
    cam.set_zoom(0.02);
    gpu.inner.reset(w, h);
    gpu.log.clear();
    let stats = Renderer::new().render(&mut gpu, &scene, &cam, &overlay, &NoImages, None);
    assert_eq!((stats.rasters, stats.raster_builds), (1, 1), "{stats:?}");
    assert_eq!(gpu.count("set_image"), 1);
    assert_eq!(gpu.inner.atlas_stats().uploads, 1);
    assert_eq!(
        gpu.inner.device().log.len(),
        1,
        "{:?}",
        gpu.inner.device().log
    );
    assert_eq!((gpu.clips, gpu.layers), (0, 0));

    // Смена размера.
    gpu.inner.reset(100, 50);
    assert_eq!(gpu.size(), (100, 50));
}

#[test]
fn image_is_uploaded_once_per_key() {
    let mut b = gpu(64, 64, atlas_config(8192));
    let a = paint(ImageKey::for_hash(&"ab".repeat(32)), 8, 8);
    for _ in 0..3 {
        b.begin_frame(64, 64);
        assert!(draw_image(&mut b, &a));
        // Та же картинка второй раз в кадре — без новой загрузки.
        assert!(draw_image(&mut b, &a));
    }
    // Та же картинка, другой `Arc` (заново декодирована) — ключ тот же.
    let again = paint(a.key, 8, 8);
    assert!(draw_image(&mut b, &again));
    // Другой ключ — другая картинка, даже с теми же пикселями.
    let other = paint(ImageKey::for_hash(&"cd".repeat(32)), 8, 8);
    assert!(draw_image(&mut b, &other));
    assert_eq!(b.device().log, ["upload 8x8", "upload 8x8"]);
    let s = b.atlas_stats();
    assert_eq!((s.images, s.bytes, s.uploads), (2, 2 * 8 * 8 * 4, 2));
}

#[test]
fn hash_keys_are_blake3_bytes_and_raster_keys_are_unique() {
    let hex = blake3::hash(b"png").to_hex().to_string();
    assert_eq!(
        ImageKey::for_hash(&hex),
        ImageKey::Hash(*blake3::hash(b"png").as_bytes())
    );
    // Повреждённый хэш из файла тоже даёт устойчивый ключ.
    assert_eq!(ImageKey::for_hash("не хэш"), ImageKey::for_hash("не хэш"));
    assert_ne!(ImageKey::for_hash("не хэш"), ImageKey::for_hash(&hex));
    assert_ne!(ImageKey::next_raster(), ImageKey::next_raster());
}

#[test]
fn full_atlas_evicts_only_images_not_drawn_this_frame() {
    // Одна страница 64×64: две картинки 40×40 вместе не помещаются.
    let mut b = gpu(64, 64, tiny_atlas(64));
    let a = paint(ImageKey::Raster(1), 40, 40);
    let c = paint(ImageKey::Raster(2), 40, 40);
    b.begin_frame(64, 64);
    assert!(draw_image(&mut b, &a));
    // В этом же кадре `a` уже в сцене — вытеснять её нельзя: заглушка.
    assert!(!draw_image(&mut b, &c));
    assert_eq!(b.atlas_stats().misses, 1);
    // Следующий кадр без `a`: она вытесняется ради `c`.
    b.begin_frame(64, 64);
    assert!(draw_image(&mut b, &c));
    assert_eq!(
        b.device().log,
        ["upload 40x40", "destroy 0", "upload 40x40"],
        "{:?}",
        b.device().log
    );
    let s = b.atlas_stats();
    assert_eq!((s.images, s.evictions, s.misses), (1, 1, 1));
}

#[test]
fn images_unused_for_long_are_evicted() {
    let mut b = gpu(64, 64, atlas_config(8192));
    let a = paint(ImageKey::Raster(7), 16, 16);
    b.begin_frame(64, 64);
    assert!(draw_image(&mut b, &a));
    for _ in 0..EVICT_AFTER_FRAMES + 64 {
        b.begin_frame(64, 64);
    }
    assert_eq!(b.atlas_stats().images, 0);
    assert_eq!(b.device().log, ["upload 16x16", "destroy 0"]);
}

#[test]
fn image_larger_than_atlas_page_is_downscaled() {
    let mut b = gpu(64, 64, tiny_atlas(64));
    let mut big = paint(ImageKey::Raster(3), 256, 128);
    big.transform = Affine::translate((5.0, 0.0));
    b.begin_frame(64, 64);
    assert!(draw_image(&mut b, &big));
    assert_eq!(b.device().log, ["upload 64x32"]);
    assert_eq!(b.atlas_stats().downscaled, 1);
    // Матрица краски растягивает уменьшенную копию обратно в размер исходной.
    let state = format!("{:?}", b.scene.save_current_state());
    let expected = format!(
        "{:?}",
        Affine::translate((5.0, 0.0)) * Affine::scale_non_uniform(4.0, 4.0)
    );
    assert!(
        state.contains(&expected),
        "нет матрицы {expected} в {state}"
    );
}

#[test]
fn device_ids_out_of_step_with_mirror_break_the_atlas() {
    let config = atlas_config(8192);
    let mut dev = FakeDevice::new(config);
    dev.skew = 1;
    let mut b = GpuBackend::new(64, 64, dev, config);
    b.begin_frame(64, 64);
    assert!(!draw_image(&mut b, &paint(ImageKey::Raster(1), 8, 8)));
    assert!(b.atlas_error().is_some_and(|e| e.contains("рассогласован")));
    // Номер устройства освобождён, дальше — только заглушки, без загрузок.
    assert!(!draw_image(&mut b, &paint(ImageKey::Raster(2), 8, 8)));
    assert_eq!(b.device().log, ["upload 8x8", "destroy 1"]);
}
