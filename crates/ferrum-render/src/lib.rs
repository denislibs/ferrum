//! Рендер Ferrum: камера, сборка кадра со списками отрисовки, отсечением и LOD,
//! оверлеи, экспорт PNG. Получает сцену, камеру и описание оверлеев; о Loro и
//! инструментах не знает.

mod backend;
mod camera;
mod display;
mod draw;
mod image;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::Scene;
use kurbo::Affine;

pub use backend::{CpuBackend, ImagePaint, RenderBackend, Rgba};
pub use camera::{Camera, MAX_DEVICE_PIXELS, MAX_DEVICE_SIDE, MAX_ZOOM, MIN_ZOOM};
pub use draw::{
    FrameStats, EVICT_AFTER_FRAMES, LOD_RECT_PX, LOD_SKIP_PX, RASTER_BUILDS_PER_FRAME,
    RASTER_CACHE_BYTES, RASTER_MAX_PX, RASTER_MAX_SCALE,
};
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use vello_cpu::Pixmap;

use draw::{DrawContext, Painter};

/// Наибольшая сторона экспортируемой картинки в пикселях.
pub const MAX_EXPORT_SIDE: u32 = 16384;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExportError {
    #[error("Узел для экспорта не найден")]
    NotFound,
    #[error("Масштаб экспорта должен быть положительным")]
    BadScale,
    #[error("Нечего экспортировать: у узла нулевой размер или он скрыт")]
    Empty,
    #[error(
        "Картинка {0}×{1} слишком большая: сторона не больше 16384 пикселей, площадь не больше 64 мегапикселей. Уменьшите масштаб"
    )]
    TooLarge(u32, u32),
    #[error("Не удалось закодировать PNG: {0}")]
    Encode(String),
}

/// Рендерер одного холста: кэши списков отрисовки и растров живут между кадрами.
#[derive(Default)]
pub struct Renderer {
    painter: Painter,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer::default()
    }

    /// Рендерер с другим пределом памяти растрового кэша (по умолчанию
    /// `RASTER_CACHE_BYTES`).
    pub fn with_raster_cache_bytes(bytes: usize) -> Renderer {
        Renderer {
            painter: Painter::with_raster_limit(bytes),
        }
    }

    /// Сколько байт сейчас занимает растровый кэш фреймов.
    pub fn raster_cache_bytes(&self) -> usize {
        self.painter.raster_bytes()
    }

    /// Только содержимое сцены, без фона и оверлеев.
    pub fn draw_scene(
        &mut self,
        backend: &mut impl RenderBackend,
        scene: &Scene,
        camera: &Camera,
        images: &dyn ImageProvider,
    ) -> FrameStats {
        self.painter.begin_frame();
        let ctx = DrawContext {
            scene,
            images,
            transform: camera.view_transform(),
            scale: camera.device_scale(),
            cull: Some(camera.visible_world_rect()),
            lod: true,
            raster: backend.supports_images(),
            retain: true,
        };
        self.painter.draw(backend, &ctx, scene.roots());
        self.painter.stats
    }

    /// Экспорт узла в PNG в масштабе `scale` (1, 2, 3…). Область — форма фрейма
    /// (как в Figma: всё за его краем обрезается) или границы узла с обводкой и
    /// потомками для остальных. Размер — пиксель в пиксель: фрейм 393×852 при @3x
    /// даёт 1179×2556. Предки не влияют: их прозрачность и обрезка не применяются.
    pub fn export_png(
        &mut self,
        scene: &Scene,
        id: NodeId,
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Vec<u8>, ExportError> {
        let pixmap = self.export_pixmap(scene, id, scale, images)?;
        pixmap
            .into_png()
            .map_err(|e| ExportError::Encode(e.to_string()))
    }

    pub fn export_pixmap(
        &mut self,
        scene: &Scene,
        id: NodeId,
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Pixmap, ExportError> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(ExportError::BadScale);
        }
        let slot = scene.slot(id).ok_or(ExportError::NotFound)?;
        let n = scene.at(slot);
        // Скрытый узел (и узел без видимого содержимого) картинки не даёт.
        if n.subtree_bounds.is_none() {
            return Err(ExportError::Empty);
        }
        let area = match (n.props.kind, n.local_bounds) {
            (NodeKind::Frame, Some(b)) => Some(n.world.transform_rect_bbox(b)),
            _ => match (n.world_bounds, n.subtree_bounds) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (a, b) => a.or(b),
            },
        }
        .ok_or(ExportError::Empty)?;
        let w = (area.width() * scale).round();
        let h = (area.height() * scale).round();
        if w < 1.0 || h < 1.0 {
            return Err(ExportError::Empty);
        }
        if w > MAX_EXPORT_SIDE as f64 || h > MAX_EXPORT_SIDE as f64 {
            return Err(ExportError::TooLarge(w as u32, h as u32));
        }
        // Сторона в пределах, но площадь может быть огромной: столько памяти не даём,
        // сколько и холсту на экране.
        if w as u64 * h as u64 > MAX_DEVICE_PIXELS {
            return Err(ExportError::TooLarge(w as u32, h as u32));
        }
        let mut backend = CpuBackend::new(w as u16, h as u16);
        let ctx = DrawContext {
            scene,
            images,
            transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
            scale,
            cull: None,
            lod: false,
            raster: false,
            retain: false,
        };
        self.painter.begin_frame();
        self.painter.draw(&mut backend, &ctx, &[slot]);
        Ok(backend.render())
    }
}
