//! Бэкенд `vello_gpu`: кадр кодируется в `vello_gpu::Scene` тем же кодом, что и
//! для CPU. Устройство и поверхность (WebGPU или WebGL2) даёт `ferrum-wasm` через
//! трейт `GpuDevice`; здесь — кодирование и атлас изображений.
//!
//! `vello_gpu` рисует изображения только из своего атласа (`ImageSource::OpaqueId`).
//! Картинки документа и растры фреймов загружаются туда по ключу (`ImageKey`) при
//! первом использовании и вытесняются, когда давно не рисовались или когда место
//! нужно новой картинке. Вытесняется только то, что не рисовалось в текущем кадре:
//! сцена кадра ссылается на картинки по номеру, и освобождённый номер в ней дал бы
//! чужие пиксели.
//!
//! Атлас glyph'ов vello делит `image_cache` с атласом изображений. Ferrum рисует
//! текст контурами; рисование текста средствами vello разошлось бы с зеркалом.
//!
//! Место в атласе выделяется зеркальным кэшем (`vello_common::image_cache`) с той же
//! конфигурацией, что у рендерера устройства. Алгоритм выделения детерминирован,
//! поэтому зеркало заранее знает, поместится ли картинка: `vello_gpu` с wgpu при
//! нехватке места паникует (`upload_image` разворачивает ошибку), а паника в wasm
//! роняет весь worker. Номер, выданный устройством, сверяется с зеркальным; при
//! расхождении атлас считается сломанным и картинки рисуются заглушкой.

use std::collections::HashMap;
use std::sync::Arc;

use kurbo::{Affine, BezPath, Rect, Stroke};
use vello_common::image_cache::ImageCache;
use vello_common::multi_atlas::{AllocationStrategy, AtlasConfig};
use vello_common::paint::{Image, ImageId, ImageSource};
// vello_cpu и vello_gpu используют один и тот же `peniko` (проверено `cargo tree -d`).
use vello_cpu::peniko::{Extend, Fill, ImageQuality, ImageSampler};
use vello_cpu::Pixmap;

use crate::backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::display::IMAGE_PLACEHOLDER;
use crate::draw::EVICT_AFTER_FRAMES;

/// Сторона страницы атласа, если устройство позволяет (WebGL2 гарантирует только
/// 2048, WebGPU — 8192).
pub const GPU_ATLAS_SIDE: u16 = 4096;
/// Страниц атласа не больше: 4 × 4096² × 4 байта = 256 МБ видеопамяти в худшем
/// случае.
pub const GPU_ATLAS_PAGES: usize = 4;

/// Устройство, в атлас которого загружаются картинки (`vello_gpu::Renderer` с wgpu
/// или `WebGlRenderer`).
///
/// Если устройство заменено или создано заново (потеря контекста), вызывающий обязан
/// вызвать `GpuBackend::reset_atlas`: прежние `ImageId` недействительны, и
/// разбор краски vello паникует на неизвестном номере.
pub trait GpuDevice {
    /// Загружает картинку в атлас и возвращает её номер. Атлас устройства выделяет
    /// место тем же алгоритмом и с той же конфигурацией (`atlas_config`), что и
    /// зеркало `GpuBackend`, поэтому номер обязан совпасть с зеркальным.
    fn upload(&mut self, pixmap: &Pixmap) -> Result<ImageId, String>;
    /// Освобождает место картинки в атласе.
    fn destroy(&mut self, id: ImageId);
}

/// Конфигурация атласа для устройства с наибольшей стороной текстуры
/// `max_texture_side`. Та же конфигурация передаётся рендереру устройства
/// (`RenderSettings::memory_settings`) и зеркалу.
pub fn atlas_config(max_texture_side: u32) -> AtlasConfig {
    let side = max_texture_side.clamp(1, GPU_ATLAS_SIDE as u32) as u16;
    AtlasConfig {
        initial_atlas_count: 0,
        max_atlases: GPU_ATLAS_PAGES,
        atlas_size: (side, side),
        auto_grow: true,
        allocation_strategy: AllocationStrategy::FirstFit,
    }
}

/// Состояние атласа — для статистики в интерфейсе и тестов.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AtlasStats {
    /// Картинок в атласе и их байт.
    pub images: usize,
    pub bytes: usize,
    /// Загрузок, вытеснений и картинок, которым не хватило места (нарисованы
    /// заглушкой), за всё время.
    pub uploads: u64,
    pub evictions: u64,
    pub misses: u64,
    /// Картинок больше страницы атласа, уменьшенных перед загрузкой.
    pub downscaled: u64,
}

struct Entry {
    id: ImageId,
    /// Размер исходной картинки и загруженной (меньше, если не влезла в страницу).
    source: (u16, u16),
    uploaded: (u16, u16),
    last_used: u64,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.uploaded.0 as usize * self.uploaded.1 as usize * 4
    }
}

pub struct GpuBackend<D: GpuDevice> {
    pub scene: vello_gpu::Scene,
    device: D,
    mirror: ImageCache,
    config: AtlasConfig,
    side: u16,
    entries: HashMap<ImageKey, Entry>,
    frame: u64,
    stats: AtlasStats,
    broken: Option<String>,
}

impl<D: GpuDevice> GpuBackend<D> {
    /// Бэкенд кадра `width × height` для устройства с атласом `atlas` — той же
    /// конфигурацией, что получил рендерер устройства.
    pub fn new(width: u16, height: u16, device: D, atlas: AtlasConfig) -> GpuBackend<D> {
        GpuBackend {
            scene: vello_gpu::Scene::new(width, height),
            device,
            side: atlas.atlas_size.0.min(atlas.atlas_size.1),
            mirror: ImageCache::new_with_config(atlas),
            config: atlas,
            entries: HashMap::new(),
            frame: 0,
            stats: AtlasStats::default(),
            broken: None,
        }
    }

    pub fn reset(&mut self, width: u16, height: u16) {
        self.scene.reset_and_resize(width, height);
    }

    pub fn device(&self) -> &D {
        &self.device
    }

    /// Если устройство заменено или создано заново (потеря контекста), после этого
    /// нужно вызвать `reset_atlas`: номера в сцене и в записях относятся к
    /// прежнему устройству.
    pub fn device_mut(&mut self) -> &mut D {
        &mut self.device
    }

    /// Сцена кадра и устройство вместе — чтобы отдать сцену рендереру устройства.
    pub fn parts(&mut self) -> (&vello_gpu::Scene, &mut D) {
        (&self.scene, &mut self.device)
    }

    /// Забывает все картинки и сбрасывает ошибку атласа: вызывать после замены или
    /// пересоздания устройства. Старые `OpaqueId` в краске паникуют в разборе краски
    /// vello, поэтому и сцену кадра нужно строить заново (`begin_frame`). Счётчики
    /// статистики сохраняются.
    pub fn reset_atlas(&mut self) {
        self.entries.clear();
        self.mirror = ImageCache::new_with_config(self.config);
        self.broken = None;
    }

    pub fn atlas_stats(&self) -> AtlasStats {
        AtlasStats {
            images: self.entries.len(),
            bytes: self.entries.values().map(Entry::bytes).sum(),
            ..self.stats
        }
    }

    /// Ошибка атласа, после которой картинки рисуются заглушкой.
    pub fn atlas_error(&self) -> Option<&str> {
        self.broken.as_deref()
    }

    /// Номер картинки в атласе (загружает при первом использовании) и множители
    /// уменьшения по осям. `None` — места нет или атлас сломан.
    fn image_id(&mut self, image: &ImagePaint) -> Option<(ImageId, f64, f64)> {
        if self.broken.is_some() {
            return None;
        }
        let size = (image.pixmap.width(), image.pixmap.height());
        // Пустая картинка: нечего выделять, а множители уменьшения дали бы NaN.
        if size.0 == 0 || size.1 == 0 {
            return None;
        }
        if let Some(e) = self.entries.get_mut(&image.key) {
            if e.source == size {
                e.last_used = self.frame;
                return Some(scaled(e));
            }
            // Под тем же ключом картинка другого размера (перекодирована): заменить.
            // Но не в кадре, где прежний номер уже в сцене: освобождённый номер
            // достанется новой картинке (место выдаётся с конца), и ранее
            // нарисованное в этом кадре показало бы её пиксели. Заглушка сейчас,
            // замена в следующем кадре.
            if e.last_used == self.frame {
                self.stats.misses += 1;
                return None;
            }
            let old = self.entries.remove(&image.key).expect("запись есть");
            self.free(old.id);
        }
        let (fitted, uploaded) = fit_to_side(&image.pixmap, self.side);
        if fitted.is_some() {
            self.stats.downscaled += 1;
        }
        let id = loop {
            match self.mirror.allocate(uploaded.0, uploaded.1, 0) {
                Ok(id) => break id,
                Err(_) => {
                    if !self.evict_one() {
                        self.stats.misses += 1;
                        return None;
                    }
                }
            }
        };
        let pixmap: &Pixmap = fitted.as_ref().unwrap_or(&image.pixmap);
        match self.device.upload(pixmap) {
            Ok(real) if real == id => {}
            Ok(real) => {
                self.device.destroy(real);
                self.mirror.deallocate(id);
                self.broken = Some(format!(
                    "Атлас изображений GPU рассогласован (номер {} вместо {})",
                    real.as_u32(),
                    id.as_u32()
                ));
                return None;
            }
            Err(e) => {
                self.mirror.deallocate(id);
                self.broken = Some(format!("Не удалось загрузить изображение в GPU: {e}"));
                return None;
            }
        }
        self.stats.uploads += 1;
        let entry = Entry {
            id,
            source: size,
            uploaded,
            last_used: self.frame,
        };
        let out = scaled(&entry);
        self.entries.insert(image.key, entry);
        Some(out)
    }

    /// Вытесняет картинку, дольше всех не рисовавшуюся, кроме нарисованных в этом
    /// кадре. `false` — вытеснять нечего.
    fn evict_one(&mut self) -> bool {
        let frame = self.frame;
        let oldest = self
            .entries
            .iter()
            .filter(|(_, e)| e.last_used < frame)
            .min_by_key(|(_, e)| e.last_used)
            .map(|(k, _)| *k);
        let Some(key) = oldest else { return false };
        let e = self.entries.remove(&key).expect("запись есть");
        self.free(e.id);
        self.stats.evictions += 1;
        true
    }

    fn free(&mut self, id: ImageId) {
        self.mirror.deallocate(id);
        self.device.destroy(id);
    }

    /// Выбрасывает картинки, не рисовавшиеся `EVICT_AFTER_FRAMES` кадров (например,
    /// растры фреймов, перестроенные под новыми ключами).
    fn evict_aged(&mut self) {
        let frame = self.frame;
        let aged: Vec<ImageKey> = self
            .entries
            .iter()
            .filter(|(_, e)| e.last_used + EVICT_AFTER_FRAMES < frame)
            .map(|(k, _)| *k)
            .collect();
        for key in aged {
            let e = self.entries.remove(&key).expect("запись есть");
            self.free(e.id);
            self.stats.evictions += 1;
        }
    }
}

/// Номер и множители уменьшения записи.
fn scaled(e: &Entry) -> (ImageId, f64, f64) {
    (
        e.id,
        e.uploaded.0 as f64 / e.source.0 as f64,
        e.uploaded.1 as f64 / e.source.1 as f64,
    )
}

/// Картинка больше страницы атласа уменьшается с сохранением пропорций (на CPU,
/// один раз при загрузке). Возвращает уменьшенную копию, если она понадобилась, и
/// размер загружаемой картинки.
fn fit_to_side(pixmap: &Arc<Pixmap>, side: u16) -> (Option<Pixmap>, (u16, u16)) {
    let (w, h) = (pixmap.width(), pixmap.height());
    if w <= side && h <= side {
        return (None, (w, h));
    }
    let k = side as f64 / w.max(h) as f64;
    let nw = ((w as f64 * k).floor() as u16).clamp(1, side);
    let nh = ((h as f64 * k).floor() as u16).clamp(1, side);
    let mut cpu = CpuBackend::new(nw, nh);
    cpu.begin_frame(nw, nh);
    cpu.set_transform(Affine::IDENTITY);
    cpu.set_image(&ImagePaint {
        key: ImageKey::Raster(0),
        pixmap: pixmap.clone(),
        transform: Affine::scale_non_uniform(nw as f64 / w as f64, nh as f64 / h as f64),
        extend: Extend::Pad,
        alpha: 1.0,
    });
    cpu.fill_rect(&Rect::new(0.0, 0.0, nw as f64, nh as f64));
    (Some(cpu.render()), (nw, nh))
}

impl<D: GpuDevice> RenderBackend for GpuBackend<D> {
    fn begin_frame(&mut self, width: u16, height: u16) {
        self.reset(width, height);
        self.frame += 1;
        if self.frame.is_multiple_of(64) {
            self.evict_aged();
        }
    }

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
        let Some((id, sx, sy)) = self.image_id(image) else {
            self.set_color(IMAGE_PLACEHOLDER.multiply_alpha(image.alpha));
            return;
        };
        self.scene.set_paint(Image {
            image: ImageSource::opaque_id(id),
            sampler: ImageSampler {
                x_extend: image.extend,
                y_extend: image.extend,
                quality: ImageQuality::Medium,
                alpha: image.alpha,
            },
        });
        // Пиксель уменьшенной копии — `1/s` пикселей исходной картинки.
        self.scene
            .set_paint_transform(image.transform * Affine::scale_non_uniform(1.0 / sx, 1.0 / sy));
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
}
