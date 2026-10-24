//! Однострочные подписи оверлеев (названия фреймов, бейдж размера) встроенным
//! шрифтом Onest.
//!
//! Раскладка — `parley`; глифы рисуются заливкой своих контуров из `skrifa`, а не
//! через `glyph_run` vello: тому в `vello_gpu` нужны ресурсы рендерера с
//! GPU-устройством, а контуры одинаково рисуются любым бэкендом и в эталонных
//! тестах. Контуры кэшируются в единицах шрифта, раскладки — по тексту и стилю.

use std::collections::{HashMap, VecDeque};
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

/// Сколько символов названия просматривается вообще. Подпись строится заново на
/// каждом кадре (до кэша), и название из мегабайт пробелов иначе просматривалось бы
/// целиком на каждом кадре ради пустой подписи.
const LABEL_SCAN_CHARS: usize = 4 * LABEL_MAX_CHARS;

/// Кэш с вытеснением давно не использованных записей (LRU) за O(1) в среднем.
/// Очередь хранит обращения по порядку; запись в ней устарела, если у ключа с тех
/// пор было обращение новее (его отметка в `map` больше). Вытеснение снимает с
/// головы очереди устаревшие записи до первой живой. Прежний поиск минимума по
/// всем 512 записям на каждой вставке стоил десятки микросекунд на подпись при зуме.
struct Lru<K, V> {
    map: HashMap<K, (V, u64)>,
    order: VecDeque<(K, u64)>,
    tick: u64,
    limit: usize,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Lru<K, V> {
    fn new() -> Self {
        Lru::with_limit(LABEL_CACHE_LIMIT)
    }

    fn with_limit(limit: usize) -> Self {
        Lru {
            map: HashMap::new(),
            order: VecDeque::new(),
            tick: 0,
            limit: limit.max(1),
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        let (v, used) = self.map.get_mut(key)?;
        self.tick += 1;
        *used = self.tick;
        let v = v.clone();
        self.order.push_back((key.clone(), self.tick));
        self.compact();
        Some(v)
    }

    fn insert(&mut self, key: K, value: V) {
        self.tick += 1;
        if self.map.len() >= self.limit && !self.map.contains_key(&key) {
            while let Some((k, t)) = self.order.pop_front() {
                if self.map.get(&k).is_some_and(|(_, used)| *used == t) {
                    self.map.remove(&k);
                    break;
                }
            }
        }
        self.map.insert(key.clone(), (value, self.tick));
        self.order.push_back((key, self.tick));
        self.compact();
    }

    /// Очередь растёт с каждым обращением; когда в ней вчетверо больше записей, чем
    /// ключей, устаревшие выбрасываются одним проходом — в среднем O(1) на обращение.
    fn compact(&mut self) {
        if self.order.len() > 4 * self.limit {
            let map = &self.map;
            self.order
                .retain(|(k, t)| map.get(k).is_some_and(|(_, used)| used == t));
        }
    }
}

/// Невидимые символы форматирования, которые выбрасываются из названия:
/// встраивания, переопределения и изоляты направления письма (U+202A–U+202E,
/// U+2066–U+2069), пробел нулевой ширины, метки направления LRM/RLM (U+200B,
/// U+200E, U+200F) и BOM (U+FEFF). Ими название из чужого файла подделывает свой
/// вид («txt.exe» справа налево). Соединители ZWNJ/ZWJ (U+200C, U+200D) остаются:
/// без них распадаются эмодзи-последовательности и ломается начертание в
/// персидском и индийских письменностях.
fn is_hidden_format(c: char) -> bool {
    matches!(
        c,
        '\u{200B}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
    )
}

/// Приводит название к одной строке: управляющие символы и переводы строк (в том
/// числе U+2028) становятся пробелами, серии пробелов схлопываются, невидимые
/// символы форматирования (`is_hidden_format`) выбрасываются. Результат — не больше
/// `LABEL_MAX_CHARS` символов и многоточие, если название длиннее; просматривается
/// не больше `LABEL_SCAN_CHARS` символов.
fn sanitize(text: &str) -> String {
    let mut out = String::new();
    let mut chars = 0;
    let mut pending_space = false;
    let mut scan = text.chars();
    for c in scan.by_ref().take(LABEL_SCAN_CHARS) {
        if c.is_control() || c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if is_hidden_format(c) {
            continue;
        }
        // Пробел перед символом тоже занимает место: без его учёта строка выходила
        // на символ длиннее предела.
        if chars + 1 + usize::from(pending_space) > LABEL_MAX_CHARS {
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
    // Название не кончилось, а просмотр — да: подпись оборвана.
    if !out.is_empty() && scan.next().is_some() {
        out.push('…');
    }
    out
}

type TextKey = (String, u32, u16);

/// Граница кластера в раскладке полного текста: усечение режет строку только по
/// ним, и подпись любой ширины собирается из готовых глифов без новой раскладки.
#[derive(Clone, Copy)]
struct Cut {
    /// Глифов до конца кластера включительно.
    glyphs: usize,
    /// Правый край кластера по базовой линии.
    x: f64,
    /// Кластер — пробел: на нём строка перед многоточием не кончается.
    space: bool,
}

/// Разложенная подпись: глифы в координатах строки (ноль — левый край базовой линии).
pub struct Label {
    glyphs: Vec<(Arc<BezPath>, Point, f64)>,
    /// Границы кластеров слева направо. `None` — текст справа налево или раскладка,
    /// где глифы не сопоставились с кластерами: тогда усечение раскладывает префиксы.
    cuts: Option<Vec<Cut>>,
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
        let ellipsis = self.full_layout(&("…".to_string(), base.1, base.2), size);
        if ellipsis.width > max_width {
            return None;
        }
        // Быстрый путь: префикс берётся из раскладки полного текста по границам
        // кластеров. При зуме ширина фрейма меняется каждый кадр, и прежний поиск
        // делением пополам раскладывал до восьми префиксов на каждую подпись.
        if let Some(cuts) = &full.cuts {
            let fit = cuts.partition_point(|c| c.x + ellipsis.width <= max_width);
            return Some(match (0..fit).rev().find(|&k| !cuts[k].space) {
                Some(k) => Arc::new(full.prefix(cuts[k], &ellipsis)),
                None => ellipsis,
            });
        }
        Some(self.fit_by_layout(base, size, max_width, ellipsis))
    }

    /// Медленный путь усечения (текст справа налево): самый длинный префикс, который
    /// вместе с «…» помещается в ширину, — поиском делением пополам с раскладкой
    /// каждого кандидата.
    fn fit_by_layout(
        &mut self,
        base: &TextKey,
        size: f64,
        max_width: f64,
        ellipsis: Arc<Label>,
    ) -> Arc<Label> {
        let text = base.0.as_str();
        let cuts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let mut best = ellipsis;
        let (mut lo, mut hi) = (0, cuts.len() - 1);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let candidate = format!("{}…", text[..cuts[mid]].trim_end());
            let laid = self.layout(&candidate, size, base.2);
            if laid.width <= max_width {
                best = Arc::new(laid);
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        best
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
        // Границы кластеров: число глифов к концу кластера и признак пробела.
        let mut clusters: Vec<(usize, bool)> = Vec::new();
        let mut prefixable = true;
        let (mut ascent, mut descent) = (0.0f64, 0.0f64);
        let mut baseline = None;
        for line in layout.lines() {
            let line_baseline = *baseline.get_or_insert(line.metrics().baseline as f64);
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let r = run.run();
                prefixable &= !r.is_rtl();
                let before = clusters.last().map_or(0, |c| c.0);
                let mut count = before;
                for c in r.visual_clusters() {
                    count += c.glyphs().count();
                    clusters.push((count, c.is_space_or_nbsp()));
                }
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
        let width = layout.width() as f64;
        // Правый край кластера — начало первого глифа следующего (у последнего —
        // ширина строки). Если глифов не столько, сколько насчитали кластеры,
        // сопоставление ненадёжно: усечение пойдёт медленным путём.
        prefixable &=
            layout.lines().count() <= 1 && clusters.last().map_or(0, |c| c.0) == glyphs.len();
        let cuts = prefixable.then(|| {
            clusters
                .iter()
                .map(|&(end, space)| Cut {
                    glyphs: end,
                    x: glyphs.get(end).map_or(width, |g| g.1.x),
                    space,
                })
                .collect()
        });
        Label {
            glyphs,
            cuts,
            width,
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
    /// Префикс подписи до границы `cut` и многоточие сразу за ним.
    fn prefix(&self, cut: Cut, ellipsis: &Label) -> Label {
        let mut glyphs = self.glyphs[..cut.glyphs].to_vec();
        glyphs.extend(
            ellipsis
                .glyphs
                .iter()
                .map(|(p, at, s)| (p.clone(), Point::new(at.x + cut.x, at.y), *s)),
        );
        Label {
            glyphs,
            cuts: None,
            width: cut.x + ellipsis.width,
            ascent: self.ascent.max(ellipsis.ascent),
            descent: self.descent.max(ellipsis.descent),
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_bidi_and_invisible_format_characters() {
        // Переопределения и изоляты направления письма, невидимые пробелы и метки
        // направления, BOM: в подписи их быть не должно (подмена вида названия).
        let hostile = "a\u{202A}b\u{202B}c\u{202C}d\u{202D}e\u{202E}f\u{2066}g\u{2067}h\u{2068}i\u{2069}j\u{200B}k\u{200E}l\u{200F}m\u{FEFF}n";
        assert_eq!(sanitize(hostile), "abcdefghijklmn");
        // Соединители ZWJ/ZWNJ остаются: без них распадаются эмодзи-последовательности
        // и меняется начертание в персидском и индийских письменностях.
        let family = "👨\u{200D}👩\u{200D}👧";
        assert_eq!(sanitize(family), family);
    }

    #[test]
    fn sanitize_never_exceeds_the_char_limit() {
        let count = |s: &str| s.trim_end_matches('…').chars().count();
        // Ровно предел — без многоточия.
        let exact = "ы".repeat(LABEL_MAX_CHARS);
        assert_eq!(sanitize(&exact), exact);
        // На символ больше — предел и многоточие.
        let over = sanitize(&"ы".repeat(LABEL_MAX_CHARS + 1));
        assert_eq!((count(&over), over.ends_with('…')), (LABEL_MAX_CHARS, true));
        // Пробел на границе: прежде строка выходила на 257 символов без многоточия.
        let edge = format!("{} b", "a".repeat(LABEL_MAX_CHARS - 1));
        let out = sanitize(&edge);
        assert!(count(&out) <= LABEL_MAX_CHARS, "{} символов", count(&out));
        assert!(out.ends_with('…'), "{out}");
    }

    #[test]
    fn sanitize_scans_a_bounded_prefix() {
        // Мегабайты пробелов перед текстом не просматриваются на каждом кадре:
        // смотрится только начало названия.
        let spaces = format!("{}x", " ".repeat(8 * 1024 * 1024));
        let start = std::time::Instant::now();
        for _ in 0..50 {
            assert_eq!(sanitize(&spaces), "");
        }
        assert!(start.elapsed().as_millis() < 50, "{:?}", start.elapsed());
        // Длинное название, оборванное пределом просмотра, кончается многоточием.
        let long = format!("{}abcdef", " ".repeat(LABEL_SCAN_CHARS - 3));
        assert_eq!(sanitize(&long), "abc…");
    }

    #[test]
    fn lru_evicts_least_recently_used_and_keeps_queue_bounded() {
        let mut lru: Lru<u32, u32> = Lru::with_limit(3);
        for k in 0..3 {
            lru.insert(k, k * 10);
        }
        // Обращение к 0 делает его свежим: вытесняется 1.
        assert_eq!(lru.get(&0), Some(0));
        lru.insert(3, 30);
        assert_eq!(lru.get(&1), None);
        assert_eq!(
            (lru.get(&0), lru.get(&2), lru.get(&3)),
            (Some(0), Some(20), Some(30))
        );
        // Тысячи обращений не раздувают очередь сверх 4 × предел.
        for i in 0..10_000 {
            lru.get(&(i % 3 * 3 / 3));
            assert!(lru.order.len() <= 4 * 3 + 1, "{}", lru.order.len());
        }
        assert_eq!(lru.map.len(), 3);
    }

    #[test]
    fn prefix_truncation_matches_relayout() {
        // Усечение по раскладке полного текста даёт ту же подпись, что раскладка
        // каждого префикса заново: тот же префикс, ширина — с точностью до кернинга
        // последней буквы со следующей (в полном тексте её пара — не «…»), меньше
        // пикселя на кегле 11.
        let mut t = TextEngine::new();
        for text in [
            "Главный экран оформления",
            "Profile settings — dark",
            "Экран 12: вход",
        ] {
            let base: TextKey = (text.to_string(), (11.0 * 64.0) as u32, 400);
            let full = t.full_layout(&base, 11.0);
            assert!(full.cuts.is_some(), "{text}");
            for w in (8..full.width as usize).step_by(3) {
                let w = w as f64;
                let fast = t.fit(&base, 11.0, w);
                let ellipsis = t.full_layout(&("…".to_string(), base.1, 400), 11.0);
                let slow = (ellipsis.width <= w).then(|| t.fit_by_layout(&base, 11.0, w, ellipsis));
                match (fast, slow) {
                    (Some(f), Some(s)) => {
                        assert!(f.width <= w, "{text} {w}: {}", f.width);
                        assert!(
                            (f.width - s.width).abs() < 1.0,
                            "{text} {w}: {} vs {}",
                            f.width,
                            s.width
                        );
                        assert_eq!(f.glyphs.len(), s.glyphs.len(), "{text} {w}");
                    }
                    (None, None) => {}
                    other => panic!("{text} {w}: {:?}", (other.0.is_some(), other.1.is_some())),
                }
            }
        }
        // Справа налево — медленный путь.
        let rtl: TextKey = ("שלום עולם".to_string(), (11.0 * 64.0) as u32, 400);
        assert!(t.full_layout(&rtl, 11.0).cuts.is_none());
    }
}
