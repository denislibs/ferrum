//! Общее подмножество API `vello_cpu::RenderContext` и `vello_gpu::Scene`, через
//! которое рендер рисует. Оба бэкенда — sparse strips: пути разбираются на полосы
//! при каждом вызове заливки, поэтому сборка кадра повторяет список отрисовки
//! узлов, а не склеивает готовые фрагменты.

use std::sync::Arc;

use kurbo::{Affine, BezPath, Rect, Stroke};
use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::peniko::{Extend, Fill, ImageQuality, ImageSampler};
use vello_cpu::{Image, ImageSource, Pixmap, RenderContext, Resources};

pub type Rgba = AlphaColor<Srgb>;

/// Изображение как краска.
#[derive(Clone)]
pub struct ImagePaint {
    pub pixmap: Arc<Pixmap>,
    /// Переводит пиксели изображения в координаты текущей трансформации.
    pub transform: Affine,
    /// `Repeat` — плитка, `Pad` — одно изображение.
    pub extend: Extend,
    pub alpha: f32,
}

pub trait RenderBackend {
    /// Размер области рисования в пикселях устройства.
    fn size(&self) -> (u16, u16);
    /// Трансформация из координат следующих путей в пиксели устройства.
    fn set_transform(&mut self, transform: Affine);
    fn set_color(&mut self, color: Rgba);
    fn set_image(&mut self, image: &ImagePaint);
    fn fill_path(&mut self, path: &BezPath, rule: Fill);
    fn fill_rect(&mut self, rect: &Rect);
    /// Обводка в координатах текущей трансформации — только для оверлеев: обводки
    /// узлов рисуются заливкой колец (см. `ferrum_scene::shape`).
    fn stroke_path(&mut self, path: &BezPath, stroke: &Stroke);
    /// Обрезка без отдельного слоя композиции — дешевле слоя.
    fn push_clip(&mut self, path: &BezPath);
    fn pop_clip(&mut self);
    fn push_opacity_layer(&mut self, alpha: f32);
    fn pop_layer(&mut self);
    /// Рисует ли бэкенд изображения из `Pixmap`. Без этого рендер не использует
    /// растровый кэш фреймов (он хранится как изображения).
    fn supports_images(&self) -> bool {
        true
    }
}

/// Растеризация на CPU через `vello_cpu`: эталонные тесты, экспорт PNG и запасной
/// рендер без GPU.
pub struct CpuBackend {
    ctx: RenderContext,
    resources: Resources,
}

impl CpuBackend {
    pub fn new(width: u16, height: u16) -> CpuBackend {
        CpuBackend {
            ctx: RenderContext::new(width, height),
            resources: Resources::new(),
        }
    }

    /// Очищает записанные команды перед новым кадром; размер меняется при ресайзе.
    pub fn reset(&mut self, width: u16, height: u16) {
        if (width, height) == self.size() {
            self.ctx.reset();
        } else {
            self.ctx.reset_and_resize(width, height);
        }
    }

    /// Растеризует записанный кадр в прозрачный `Pixmap`. Память кадра — ширина ×
    /// высота × 4 байта на каждый вызов: размер задаёт вызывающий (камера ограничивает
    /// его через `MAX_DEVICE_SIDE` и `MAX_DEVICE_PIXELS`).
    pub fn render(&mut self) -> Pixmap {
        let (w, h) = self.size();
        let mut target = Pixmap::new(w, h);
        self.ctx.flush();
        self.ctx.render(&mut target, &mut self.resources);
        target
    }
}

impl RenderBackend for CpuBackend {
    fn size(&self) -> (u16, u16) {
        (self.ctx.width(), self.ctx.height())
    }

    fn set_transform(&mut self, transform: Affine) {
        self.ctx.set_transform(transform);
    }

    fn set_color(&mut self, color: Rgba) {
        self.ctx.reset_paint_transform();
        self.ctx.set_paint(color);
    }

    fn set_image(&mut self, image: &ImagePaint) {
        self.ctx.set_paint(Image {
            image: ImageSource::Pixmap(image.pixmap.clone()),
            sampler: ImageSampler {
                x_extend: image.extend,
                y_extend: image.extend,
                quality: ImageQuality::Medium,
                alpha: image.alpha,
            },
        });
        self.ctx.set_paint_transform(image.transform);
    }

    fn fill_path(&mut self, path: &BezPath, rule: Fill) {
        self.ctx.set_fill_rule(rule);
        self.ctx.fill_path(path);
    }

    fn fill_rect(&mut self, rect: &Rect) {
        self.ctx.fill_rect(rect);
    }

    fn stroke_path(&mut self, path: &BezPath, stroke: &Stroke) {
        self.ctx.set_stroke(stroke.clone());
        self.ctx.stroke_path(path);
    }

    fn push_clip(&mut self, path: &BezPath) {
        self.ctx.set_fill_rule(Fill::NonZero);
        self.ctx.push_clip_path(path);
    }

    fn pop_clip(&mut self) {
        self.ctx.pop_clip();
    }

    fn push_opacity_layer(&mut self, alpha: f32) {
        self.ctx.push_opacity_layer(alpha);
    }

    fn pop_layer(&mut self) {
        self.ctx.pop_layer();
    }
}
