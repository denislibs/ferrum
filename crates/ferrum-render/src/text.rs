//! Однострочные подписи оверлеев (названия фреймов, бейдж размера) встроенным
//! шрифтом Onest.
//!
//! Раскладка — `parley`; глифы рисуются заливкой своих контуров из `skrifa`, а не
//! через `glyph_run` vello: тому в `vello_gpu` нужны ресурсы рендерера с
//! GPU-устройством, а контуры одинаково рисуются любым бэкендом и в эталонных
//! тестах. Контуры кэшируются в единицах шрифта, раскладки — по тексту и стилю.

use std::collections::HashMap;
use std::sync::Arc;

use kurbo::{Affine, BezPath, Point};
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    FontContext, FontFamily, FontWeight, Layout, LayoutContext, PositionedLayoutItem, StyleProperty,
};
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use vello_cpu::peniko::Fill;

use crate::backend::{RenderBackend, Rgba};

/// Onest (SIL Open Font License 1.1, лицензия — `assets/fonts/OFL.txt`).
pub const ONEST: &[u8] = include_bytes!("../assets/fonts/Onest-wght.ttf");

/// Сколько разных подписей держать в кэше раскладок; больше — кэш сбрасывается.
const LABEL_CACHE_LIMIT: usize = 512;

/// Разложенная подпись: глифы в координатах строки (ноль — левый край базовой линии).
pub struct Label {
    glyphs: Vec<(Arc<BezPath>, Point, f64)>,
    /// Ширина строки.
    pub width: f64,
    /// Подъём над базовой линией и спуск под ней.
    pub ascent: f64,
    pub descent: f64,
}

pub struct TextEngine {
    fonts: FontContext,
    layouts: LayoutContext<()>,
    family: String,
    outlines: HashMap<(u32, Vec<i16>), Arc<BezPath>>,
    labels: HashMap<(String, u32, u16, u32), Arc<Label>>,
}

impl Default for TextEngine {
    fn default() -> Self {
        TextEngine::new()
    }
}

impl TextEngine {
    pub fn new() -> TextEngine {
        // Без системных шрифтов: подпись выглядит одинаково везде, и в WASM нечего искать.
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let families = collection.register_fonts(Blob::new(Arc::new(ONEST)), None);
        let family = families
            .first()
            .and_then(|(id, _)| collection.family_name(*id))
            .unwrap_or("Onest")
            .to_string();
        TextEngine {
            fonts: FontContext {
                collection,
                source_cache: SourceCache::default(),
            },
            layouts: LayoutContext::new(),
            family,
            outlines: HashMap::new(),
            labels: HashMap::new(),
        }
    }

    /// Подпись, усечённая с многоточием до `max_width` (`None` — не помещается даже
    /// многоточие). Размер — в пикселях, в которых подпись будет нарисована.
    pub fn label(
        &mut self,
        text: &str,
        size: f64,
        weight: u16,
        max_width: f64,
    ) -> Option<Arc<Label>> {
        // Отрицательные, нулевые и NaN-размеры подписи не дают.
        let positive = |v: f64| v > 0.0;
        if text.is_empty() || !positive(max_width) || !positive(size) {
            return None;
        }
        let key = (
            text.to_string(),
            (size * 64.0) as u32,
            weight,
            max_width.clamp(0.0, 1e6) as u32,
        );
        if let Some(l) = self.labels.get(&key) {
            return Some(l.clone());
        }
        let full = self.layout(text, size, weight);
        let label = if full.width <= max_width {
            full
        } else {
            // Самый длинный префикс, который вместе с «…» помещается в ширину.
            let cuts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
            let mut shortened = |keep: usize| {
                let candidate = format!("{}…", text[..cuts[keep]].trim_end());
                self.layout(&candidate, size, weight)
            };
            let mut best = shortened(0);
            if best.width > max_width {
                return None;
            }
            let (mut lo, mut hi) = (0, cuts.len() - 1);
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                let candidate = shortened(mid);
                if candidate.width <= max_width {
                    best = candidate;
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            best
        };
        if self.labels.len() >= LABEL_CACHE_LIMIT {
            self.labels.clear();
        }
        let label = Arc::new(label);
        self.labels.insert(key, label.clone());
        Some(label)
    }

    fn layout(&mut self, text: &str, size: f64, weight: u16) -> Label {
        let mut builder = self
            .layouts
            .ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(&self.family)));
        builder.push_default(StyleProperty::FontSize(size as f32));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(weight as f32)));
        // Цифры одной ширины: значения в бейдже размера не прыгают при ресайзе.
        builder.push_default(StyleProperty::FontFeatures("\"tnum\" on".into()));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        let mut glyphs = Vec::new();
        let (mut ascent, mut descent) = (0.0f64, 0.0f64);
        let mut baseline = None;
        for line in layout.lines() {
            let line_baseline = *baseline.get_or_insert(line.metrics().baseline as f64);
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let r = run.run();
                let m = r.font_metrics();
                ascent = ascent.max(m.ascent as f64);
                descent = descent.max(m.descent.abs() as f64);
                let font = r.font();
                let Ok(font_ref) = FontRef::from_index(font.data.as_ref(), font.index) else {
                    continue;
                };
                let coords: Vec<i16> = r.normalized_coords().iter().map(|c| c.to_bits()).collect();
                let upem = font_ref
                    .metrics(Size::unscaled(), LocationRef::default())
                    .units_per_em
                    .max(1) as f64;
                let scale = r.font_size() as f64 / upem;
                for g in run.positioned_glyphs() {
                    let path = self.outline(&font_ref, g.id, &coords);
                    let at = Point::new(g.x as f64, g.y as f64 - line_baseline);
                    glyphs.push((path, at, scale));
                }
            }
        }
        Label {
            glyphs,
            width: layout.width() as f64,
            ascent,
            descent,
        }
    }

    /// Контур глифа в единицах шрифта (ось Y вверх), из кэша.
    fn outline(&mut self, font: &FontRef<'_>, glyph: u32, coords: &[i16]) -> Arc<BezPath> {
        let key = (glyph, coords.to_vec());
        if let Some(p) = self.outlines.get(&key) {
            return p.clone();
        }
        let location: Vec<NormalizedCoord> = coords
            .iter()
            .map(|c| NormalizedCoord::from_bits(*c))
            .collect();
        let mut pen = Pen(BezPath::new());
        if let Some(outline) = font.outline_glyphs().get(GlyphId::new(glyph)) {
            // Глиф с повреждённым контуром рисуется пустым, а не роняет кадр.
            let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::new(&location));
            if outline.draw(settings, &mut pen).is_err() {
                pen.0 = BezPath::new();
            }
        }
        let path = Arc::new(pen.0);
        self.outlines.insert(key, path.clone());
        path
    }
}

impl Label {
    /// Рисует подпись: `origin` — левый край базовой линии в пикселях устройства.
    pub fn draw(&self, backend: &mut impl RenderBackend, origin: Point, color: Rgba) {
        backend.set_color(color);
        for (path, at, scale) in &self.glyphs {
            backend.set_transform(
                Affine::translate(origin.to_vec2() + at.to_vec2())
                    * Affine::scale_non_uniform(*scale, -*scale),
            );
            backend.fill_path(path, Fill::NonZero);
        }
    }
}

struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.quad_to((cx as f64, cy as f64), (x as f64, y as f64));
    }
    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.0.curve_to(
            (c1x as f64, c1y as f64),
            (c2x as f64, c2y as f64),
            (x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}
