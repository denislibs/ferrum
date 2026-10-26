//! Рендер Ferrum: камера, сборка кадра со списками отрисовки, отсечением и LOD,
//! оверлеи, экспорт PNG. Получает сцену, камеру и описание оверлеев; о Loro и
//! инструментах не знает.

mod backend;
mod camera;
mod clock;
mod display;
mod draw;
#[cfg(feature = "gpu")]
mod gpu;
mod image;
mod overlay;
mod text;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::Scene;
use kurbo::{Affine, Rect};

pub use backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend, Rgba};
pub use camera::{Camera, MAX_DEVICE_PIXELS, MAX_DEVICE_SIDE, MAX_ZOOM, MIN_ZOOM};
#[cfg(not(target_arch = "wasm32"))]
pub use clock::StdClock;
pub use clock::{default_clock, Clock};
pub use draw::{
    FrameStats, EVICT_AFTER_FRAMES, LOD_RECT_PX, LOD_SKIP_PX, RASTER_BUILDS_PER_FRAME,
    RASTER_BUILD_TIME_PER_FRAME, RASTER_CACHE_BYTES, RASTER_MAX_PX, RASTER_MAX_SCALE,
};
#[cfg(feature = "gpu")]
pub use gpu::{atlas_config, AtlasStats, GpuBackend, GpuDevice, GPU_ATLAS_PAGES, GPU_ATLAS_SIDE};
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use overlay::{
    format_size, Measure, MeasureKind, Overlay, OverlayTheme, HANDLE_SIZE, LABEL_SIZE,
    MEASURE_DASH, MEASURE_GAP, PIXEL_GRID_MIN_ZOOM,
};
pub use text::{Label, TextEngine, ONEST};
#[cfg(feature = "gpu")]
pub use vello_common::{multi_atlas::AtlasConfig, paint::ImageId};
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

/// Рендерер одного холста: кэши списков отрисовки, растров и шрифтов живут между кадрами.
#[derive(Default)]
pub struct Renderer {
    painter: Painter,
    text: TextEngine,
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
            text: TextEngine::new(),
        }
    }

    /// Предел времени на построение растров фреймов за кадр (по умолчанию
    /// `RASTER_BUILD_TIME_PER_FRAME`). `None` — только счётный предел
    /// `RASTER_BUILDS_PER_FRAME`: число кадров прогрева не зависит от скорости машины.
    pub fn set_raster_build_time(&mut self, budget: Option<std::time::Duration>) {
        self.painter.set_raster_time(budget);
    }

    /// Часы для предела времени на растры. По умолчанию нативно — `StdClock`, в
    /// wasm32 часов нет (там `Instant` паникует) и работает только счётный предел,
    /// пока `ferrum-wasm` не поставит `performance.now()`.
    pub fn set_clock(&mut self, clock: Option<Box<dyn Clock>>) {
        self.painter.set_clock(clock);
    }

    /// Сколько байт сейчас занимает растровый кэш фреймов.
    pub fn raster_cache_bytes(&self) -> usize {
        self.painter.raster_bytes()
    }

    /// Рисует кадр: фон, содержимое, оверлеи. Сначала бэкенд начинает новый кадр
    /// размера `camera.device_size()` (`RenderBackend::begin_frame`): прошлый кадр
    /// стирается, поэтому один бэкенд можно переиспользовать.
    pub fn render(
        &mut self,
        backend: &mut impl RenderBackend,
        scene: &Scene,
        camera: &Camera,
        overlay: &Overlay,
        images: &dyn ImageProvider,
        background: Option<Rgba>,
    ) -> FrameStats {
        let (w, h) = camera.device_size();
        backend.begin_frame(w, h);
        if let Some(bg) = background {
            let (w, h) = backend.size();
            backend.set_transform(Affine::IDENTITY);
            backend.set_color(bg);
            backend.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
        }
        let stats = self.draw_scene(backend, scene, camera, images);
        overlay::draw(backend, &mut self.text, scene, camera, overlay);
        stats
    }

    /// Сколько раскладок подписей выполнено за всё время (для проверки кэша).
    pub fn text_layouts(&self) -> usize {
        self.text.layout_count()
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
        self.painter.finish_frame()
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
        self.export_png_nodes(scene, &[id], scale, images)
    }

    /// Экспорт нескольких узлов одной картинкой («Скопировать как PNG» для
    /// выделения): область — объединение областей узлов (как у `export_png`), узлы
    /// рисуются в порядке отрисовки сцены, а не выделения. Неизвестные и скрытые
    /// узлы пропускаются; если не осталось ни одного — ошибка.
    pub fn export_png_nodes(
        &mut self,
        scene: &Scene,
        ids: &[NodeId],
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Vec<u8>, ExportError> {
        let pixmap = self.export_pixmap_nodes(scene, ids, scale, images)?;
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
        self.export_pixmap_nodes(scene, &[id], scale, images)
    }

    pub fn export_pixmap_nodes(
        &mut self,
        scene: &Scene,
        ids: &[NodeId],
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Pixmap, ExportError> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(ExportError::BadScale);
        }
        let mut items = Vec::with_capacity(ids.len());
        let mut known = false;
        for id in ids {
            let Some(slot) = scene.slot(*id) else {
                continue;
            };
            known = true;
            let n = scene.at(slot);
            // Скрытый узел (и узел без видимого содержимого) картинки не даёт. Скрытые
            // предки здесь не проверяются: скрытый родитель скрывает всё поддерево,
            // так что `subtree_bounds` у потомка уже пусто.
            if n.subtree_bounds.is_none() {
                continue;
            }
            let own = match (n.props.kind, n.local_bounds) {
                (NodeKind::Frame, Some(b)) => Some(n.world.transform_rect_bbox(b)),
                _ => match (n.world_bounds, n.subtree_bounds) {
                    (Some(a), Some(b)) => Some(a.union(b)),
                    (a, b) => a.or(b),
                },
            };
            if let Some(own) = own {
                items.push((slot, own));
            }
        }
        if !known {
            return Err(ExportError::NotFound);
        }
        // Узел, чей предок тоже экспортируется, уже нарисован вместе с предком:
        // второй раз он лёг бы поверх (полупрозрачный — темнее). Область считаем
        // только по оставшимся: потомок, выступающий за обрезающий фрейм, холст
        // не расширяет.
        let chosen: std::collections::HashSet<_> = items.iter().map(|(s, _)| *s).collect();
        items.retain(|&(s, _)| {
            let mut cur = scene.at(s).parent;
            while let Some(p) = cur {
                if chosen.contains(&p) {
                    return false;
                }
                cur = scene.at(p).parent;
            }
            true
        });
        let area = items
            .iter()
            .map(|&(_, r)| r)
            .reduce(|a, b| a.union(b))
            .ok_or(ExportError::Empty)?;
        // Порядок отрисовки: ключ — позиции от корня, без повторов.
        let mut slots: Vec<_> = items.into_iter().map(|(s, _)| s).collect();
        slots.sort_by_cached_key(|s| scene.order_key(*s));
        slots.dedup();
        let w = (area.width() * scale).round();
        let h = (area.height() * scale).round();
        // Нечисловые границы (NaN, бесконечность) картинки не дают: до приведения
        // к целым они дали бы мусорный размер.
        if !(w.is_finite() && h.is_finite()) {
            return Err(ExportError::Empty);
        }
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
        backend.begin_frame(w as u16, h as u16);
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
        // Свой рисовальщик: экспорт — не кадр экрана. Общий сдвинул бы счётчик кадров,
        // от которого зависит вытеснение растров и контуров на экране, и сбросил бы
        // бюджет и статистику текущего кадра. Растров и сохранённых списков экспорт
        // не использует (`raster`, `retain` выключены), так что терять нечего.
        let mut painter = Painter::with_raster_limit(0);
        painter.begin_frame();
        painter.draw(&mut backend, &ctx, &slots);
        Ok(backend.render())
    }
}
