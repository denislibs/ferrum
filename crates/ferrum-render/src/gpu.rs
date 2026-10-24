//! Бэкенд `vello_gpu`: кадр кодируется в `vello_gpu::Scene` тем же кодом, что и
//! для CPU. Поверхность, устройство wgpu/WebGL и растеризация — этап 4.

use kurbo::{Affine, BezPath, Rect, Stroke};
use vello_cpu::peniko::Fill;

use crate::backend::{ImagePaint, RenderBackend, Rgba};

/// Цвет-заглушка вместо изображений. `vello_gpu` принимает картинки только
/// загруженными в атлас (`ImageSource::OpaqueId`), а атлас живёт в `Resources`
/// рендерера, которому нужно GPU-устройство. Загрузка изображений — этап 4.
const IMAGE_PLACEHOLDER: Rgba = Rgba::new([0.9, 0.9, 0.9, 1.0]);

pub struct GpuBackend {
    pub scene: vello_gpu::Scene,
}

impl GpuBackend {
    pub fn new(width: u16, height: u16) -> GpuBackend {
        GpuBackend {
            scene: vello_gpu::Scene::new(width, height),
        }
    }

    pub fn reset(&mut self, width: u16, height: u16) {
        self.scene.reset_and_resize(width, height);
    }
}

impl RenderBackend for GpuBackend {
    fn size(&self) -> (u16, u16) {
        (self.scene.width(), self.scene.height())
    }

    fn set_transform(&mut self, transform: Affine) {
        self.scene.set_transform(transform);
    }

    fn set_color(&mut self, color: Rgba) {
        self.scene.reset_paint_transform();
        self.scene.set_paint(color);
    }

    fn set_image(&mut self, image: &ImagePaint) {
        self.set_color(IMAGE_PLACEHOLDER.multiply_alpha(image.alpha));
    }

    fn fill_path(&mut self, path: &BezPath, rule: Fill) {
        self.scene.set_fill_rule(rule);
        self.scene.fill_path(path);
    }

    fn fill_rect(&mut self, rect: &Rect) {
        self.scene.fill_rect(rect);
    }

    fn stroke_path(&mut self, path: &BezPath, stroke: &Stroke) {
        self.scene.set_stroke(stroke.clone());
        self.scene.stroke_path(path);
    }

    fn push_clip(&mut self, path: &BezPath) {
        self.scene.set_fill_rule(Fill::NonZero);
        self.scene.push_clip_path(path);
    }

    fn pop_clip(&mut self) {
        self.scene.pop_clip();
    }

    fn push_opacity_layer(&mut self, alpha: f32) {
        self.scene.push_opacity_layer(alpha);
    }

    fn pop_layer(&mut self) {
        self.scene.pop_layer();
    }

    fn supports_images(&self) -> bool {
        false
    }
}
