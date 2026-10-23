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

/// Сколько записей держать в каждом кэше подписей; при переполнении вытесняется
/// запись, которой не пользовались дольше всех.
const LABEL_CACHE_LIMIT: usize = 512;

/// Названия узлов приходят из документа (то есть извне): раскладываются не больше
/// стольких символов, остальное отбрасывается до раскладки. Иначе название в
/// несколько мегабайт раскладывалось бы целиком ради подписи в одну строку.
pub const LABEL_MAX_CHARS: usize = 256;

/// Кэш с вытеснением давно не использованных записей (LRU).
struct Lru<K, V> {
    map: HashMap<K, (V, u64)>,
    tick: u64,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Lru<K, V> {
    fn new() -> Self {
        Lru {
            map: HashMap::new(),
            tick: 0,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        self.tick += 1;
        let tick = self.tick;
        self.map.get_mut(key).map(|(v, used)| {
            *used = tick;
            v.clone()
        })
    }

    fn insert(&mut self, key: K, value: V) {
        self.tick += 1;
        if self.map.len() >= LABEL_CACHE_LIMIT && !self.map.contains_key(&key) {
            let oldest = self
                .map
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                self.map.remove(&k);
            }
        }
        self.map.insert(key, (value, self.tick));
    }
}

/// Приводит название к одной строке: управляющие символы и переводы строк (в том
/// числе U+2028) становятся пробелами, серии пробелов схлопываются, край
/// обрезается до `LABEL_MAX_CHARS` по границе символа с многоточием.
fn sanitize(text: &str) -> String {
    let mut out = String::new();
    let mut chars = 0;
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_control() || c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if chars >= LABEL_MAX_CHARS {
            out.push('…');
            return out;
        }
        if pending_space {
            out.push(' ');
            chars += 1;
            pending_space = false;
        }
        out.push(c);
        chars += 1;
    }
    out
}

type TextKey = (String, u32, u16);

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
    /// Раскладка полного текста — не зависит от доступной ширины.
    full: Lru<TextKey, Arc<Label>>,
    /// Результат усечения до ширины (включая `None`: не поместилось и многоточие).
    fitted: Lru<(TextKey, u32), Option<Arc<Label>>>,
    layout_count: usize,
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
            full: Lru::new(),
            fitted: Lru::new(),
            layout_count: 0,
        }
    }

    /// Подпись, усечённая с многоточием до `max_width` (`None` — не помещается даже
    /// многоточие или нечего рисовать). Размер — в пикселях, в которых подпись будет
    /// нарисована. Текст недоверенный: см. `sanitize`.
    pub fn label(
        &mut self,
        text: &str,
        size: f64,
        weight: u16,
        max_width: f64,
    ) -> Option<Arc<Label>> {
        // Отрицательные, нулевые и NaN-размеры подписи не дают.
        let positive = |v: f64| v > 0.0;
        if !positive(max_width) || !positive(size) {
            return None;
        }
        // Ширина округляется вниз до измерения: ключ кэша — целое число пикселей, и
        // результат, посчитанный для 40,9, не должен переиспользоваться для 40,1.
        let max_width = max_width.floor();
        if !positive(max_width) {
            return None;
        }
        let clean = sanitize(text);
        if clean.is_empty() {
            return None;
        }
        let base: TextKey = (clean, (size * 64.0) as u32, weight);
        let key = (base.clone(), max_width.min(1e9) as u32);
        if let Some(hit) = self.fitted.get(&key) {
            return hit;
        }
        let result = self.fit(&base, size, max_width);
        self.fitted.insert(key, result.clone());
        result
    }

    /// Ширина многоточия: уже неё подпись не бывает.
    pub fn ellipsis_width(&mut self, size: f64, weight: u16) -> f64 {
        if size.is_nan() || size <= 0.0 {
            return f64::INFINITY;
        }
        self.full_layout(&("…".to_string(), (size * 64.0) as u32, weight), size)
            .width
    }

    fn full_layout(&mut self, key: &TextKey, size: f64) -> Arc<Label> {
        if let Some(l) = self.full.get(key) {
            return l;
        }
        let l = Arc::new(self.layout(&key.0, size, key.2));
        self.full.insert(key.clone(), l.clone());
        l
    }

    fn fit(&mut self, base: &TextKey, size: f64, max_width: f64) -> Option<Arc<Label>> {
        let full = self.full_layout(base, size);
        if full.width <= max_width {
            return Some(full);
        }
        // Самый длинный префикс, который вместе с «…» помещается в ширину.
        let weight = base.2;
        let text = base.0.as_str();
        let ellipsis = self.full_layout(&("…".to_string(), base.1, weight), size);
        if ellipsis.width > max_width {
            return None;
        }
        let cuts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let mut best = ellipsis;
        let (mut lo, mut hi) = (0, cuts.len() - 1);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let candidate = format!("{}…", text[..cuts[mid]].trim_end());
            let laid = self.layout(&candidate, size, weight);
            if laid.width <= max_width {
                best = Arc::new(laid);
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        Some(best)
    }

    /// Сколько раскладок выполнено (для проверки кэша).
    pub fn layout_count(&self) -> usize {
        self.layout_count
    }

    fn layout(&mut self, text: &str, size: f64, weight: u16) -> Label {
        self.layout_count += 1;
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
