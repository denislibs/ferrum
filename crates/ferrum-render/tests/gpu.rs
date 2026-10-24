//! Бэкенд `vello_gpu` принимает тот же кадр, что и CPU: журнал вызовов совпадает.
//! Запуск: `cargo test -p ferrum-render --features gpu --test gpu`.
#![cfg(feature = "gpu")]

mod common;

use std::sync::Arc;

use common::*;
use ferrum_doc::{Fill, NodeKind, PropsPatch};
use ferrum_render::{
    Camera, CpuBackend, GpuBackend, ImagePaint, NoImages, Overlay, Pixmap, RenderBackend, Renderer,
    Rgba,
};
use kurbo::{Affine, BezPath, Circle, Point, Rect, Shape, Size, Stroke};
// Оба бэкенда используют один и тот же `peniko` (см. `cargo tree -d`).
use vello_cpu::peniko::{Extend, Fill as FillRule};

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
    let mut gpu = Recording::new(GpuBackend::new(w, h));
    let mut cpu = Recording::new(CpuBackend::new(w, h));
    assert!(!gpu.supports_images());
    let gpu_stats =
        Renderer::new().render(&mut gpu, &scene, &cam, &overlay, &NoImages, Some(WHITE));
    // CPU без растрового кэша, чтобы кадры были сравнимы.
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
        (|b: &mut GpuBackend| b.scene.pop_layer()) as fn(&mut GpuBackend),
        |b| b.scene.pop_clip(),
    ] {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extra(&mut gpu.inner)));
        assert!(res.is_err(), "в сцене остались незакрытые слои/обрезки");
    }

    // Сильное отдаление: растрового кэша у GPU-бэкенда нет — узлы рисуются LOD.
    cam.set_zoom(0.02);
    gpu.inner.reset(w, h);
    gpu.log.clear();
    let stats = Renderer::new().render(&mut gpu, &scene, &cam, &overlay, &NoImages, None);
    assert_eq!(stats.rasters, 0);
    assert!(stats.lod_rects > 0, "LOD-путь не сработал: {stats:?}");
    assert!(gpu.count("fill_rect") + gpu.count("fill_path") > 0);
    assert_eq!((gpu.clips, gpu.layers), (0, 0));

    // Смена размера.
    gpu.inner.reset(100, 50);
    assert_eq!(gpu.size(), (100, 50));
}

#[test]
fn image_placeholder_is_accepted() {
    let mut gpu = Recording::new(GpuBackend::new(64, 64));
    let paint = ImagePaint {
        pixmap: Arc::new(Pixmap::new(4, 4)),
        transform: Affine::IDENTITY,
        extend: Extend::Pad,
        alpha: 0.5,
    };
    gpu.set_image(&paint);
    gpu.fill_path(
        &Circle::new((32.0, 32.0), 20.0).to_path(0.1),
        FillRule::NonZero,
    );
    // Заглушка: серая краска с учётом альфы, изображения в сцене нет.
    let placeholder = format!("{:?}", gpu.inner.scene.paint());
    assert!(!placeholder.contains("Image"), "{placeholder}");
    gpu.set_color(Rgba::new([1.0, 0.0, 0.0, 1.0]));
    let red = format!("{:?}", gpu.inner.scene.paint());
    assert_ne!(red, placeholder, "set_color не сменил краску");
    let stroke = Stroke::new(3.0);
    gpu.stroke_path(&Circle::new((32.0, 32.0), 10.0).to_path(0.1), &stroke);
    assert_eq!(gpu.inner.scene.stroke().width, 3.0);
    gpu.fill_rect(&Rect::new(0.0, 0.0, 10.0, 10.0));
    // Заглушка идёт через `set_color`, отдельного вызова изображения нет.
    assert_eq!(
        gpu.log,
        [
            "set_image",
            "fill_path",
            "set_color",
            "stroke_path",
            "fill_rect"
        ]
    );
}
