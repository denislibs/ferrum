//! Экспорт в PDF через `krilla` (спека подсистемы 3, раздел 11): векторные
//! страницы, подмножества шрифтов, картинки. Узел — страница размера его области
//! (фрейм — своего размера, 1 px = 1 pt, как в Figma). Рисует тот же
//! рисовальщик, что экран и PNG (`Painter`), через свой бэкенд: заливки и
//! обводки — путями PDF, обрезка фреймов и обводок — обрезкой PDF, прозрачность
//! — группами прозрачности, картинки — изображениями PDF; текст сплошной краской
//! — настоящим текстом (шрифт с подмножеством глифов, текст ищется и
//! копируется), прочий (картинкой, обводки) — контурами глифов.
//!
//! Страница PDF не больше 14 400 pt по стороне (предел формата): большая область
//! уменьшается до него целиком.

use std::collections::HashMap;
use std::sync::Arc;

use ferrum_doc::NodeId;
use ferrum_scene::Scene;
use ferrum_text::{GlyphRun, SharedFonts};
use krilla::color::rgb;
use krilla::error::KrillaError;
use krilla::geom::{Path, PathBuilder, Point as KPoint, Size as KSize, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill as KFill, FillRule, LineCap, LineJoin, Pattern, Stroke as KStroke};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph, Tag};
use krilla::{Data, Document, SerializeSettings};
use kurbo::{Affine, BezPath, PathEl, Rect, Shape};
use skrifa::instance::{LocationRef, Size};
use skrifa::raw::types::NameId;
use skrifa::raw::TableProvider;
use skrifa::MetadataProvider;
use vello_cpu::peniko::{Extend, Fill};

use crate::backend::{ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::draw::{DrawContext, Painter};
use crate::export::{node_area, ExportBudget};
use crate::image::ImageProvider;
use crate::ExportError;

/// Наибольшая сторона страницы PDF, pt (предел формата без `UserUnit`).
pub const MAX_PDF_SIDE: f64 = 14_400.0;
/// Плиток картинки «Замостить» на одну заливку не больше (дальше — без плиток).
const MAX_TILES: usize = 4096;
/// Как часто бэкенд сверяется с часами (вызовов рисования).
const CHECK_EVERY: usize = 4096;
/// Оценка размера содержимого страницы до сжатия, байт: путь, элемент пути,
/// прогон текста, глиф прогона, плитка картинки (`q … cm /Im Do Q`).
const PATH_BASE_BYTES: usize = 48;
const PATH_ELEMENT_BYTES: usize = 24;
const GLYPH_RUN_BYTES: usize = 96;
const GLYPH_BYTES: usize = 8;
const TILE_BYTES: usize = 80;

/// Текущая краска.
#[derive(Clone)]
enum Paint {
    Solid(Rgba),
    Image(ImagePaint),
}

/// Ключ шрифта: байты (по `Blob::id`), номер в коллекции, координаты осей, вес и
/// ширина начертания (`font_of` ставит по ним оси `wght` и `wdth`).
type FontKey = (u64, u32, Vec<i16>, u32, u32);

/// Причина неудачи сборки: пределы и прочие ошибки экспорта или шрифт, который
/// `krilla` не смог встроить (тогда файл собирается заново с текстом контурами).
enum Fail {
    Export(ExportError),
    Font,
}

impl From<ExportError> for Fail {
    fn from(e: ExportError) -> Self {
        Fail::Export(e)
    }
}

/// Кэши одного документа: шрифты и картинки встраиваются один раз.
#[derive(Default)]
struct Cache {
    fonts: HashMap<FontKey, Option<Font>>,
    images: HashMap<ImageKey, Image>,
}

/// Бэкенд поверх поверхности страницы `krilla`. Матрица применяется к
/// координатам путей (обрезка в PDF живёт в стеке состояния, и снять матрицу,
/// не сняв обрезку, нельзя), у обводок и картинок — на время одного вызова.
struct PdfBackend<'s, 'p> {
    surface: &'s mut Surface<'p>,
    cache: &'s mut Cache,
    size: (u16, u16),
    transform: Affine,
    paint: Paint,
    /// Текст — настоящим текстом (иначе контурами).
    text: bool,
    budget: &'s ExportBudget<'s>,
    /// Учтённая работа: вызовы рисования (плитка и прогон — по вызову) и оценка
    /// размера содержимого в байтах (до сжатия) — по ним `charge` сверяется с
    /// пределами, не дожидаясь конца страницы.
    calls: usize,
    next_check: usize,
    estimate: usize,
    /// Предел превышен: дальше бэкенд ничего не рисует.
    failure: Option<ExportError>,
}

fn finite(v: f64) -> f32 {
    if v.is_finite() {
        v.clamp(-1.0e9, 1.0e9) as f32
    } else {
        0.0
    }
}

fn transform(a: Affine) -> Transform {
    let c = a.as_coeffs();
    Transform::from_row(
        finite(c[0]),
        finite(c[1]),
        finite(c[2]),
        finite(c[3]),
        finite(c[4]),
        finite(c[5]),
    )
}

/// Путь `krilla` из пути `kurbo` с матрицей `t`; пустой — `None`.
fn path(p: &BezPath, t: Affine) -> Option<Path> {
    let mut b = PathBuilder::new();
    for el in p.elements() {
        match t * *el {
            PathEl::MoveTo(a) => b.move_to(finite(a.x), finite(a.y)),
            PathEl::LineTo(a) => b.line_to(finite(a.x), finite(a.y)),
            PathEl::QuadTo(a, c) => b.quad_to(finite(a.x), finite(a.y), finite(c.x), finite(c.y)),
            PathEl::CurveTo(a, c, d) => b.cubic_to(
                finite(a.x),
                finite(a.y),
                finite(c.x),
                finite(c.y),
                finite(d.x),
                finite(d.y),
            ),
            PathEl::ClosePath => b.close(),
        }
    }
    b.finish()
}

fn rgb_of(c: Rgba) -> (rgb::Color, NormalizedF32) {
    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let [r, g, b, a] = c.components;
    (
        rgb::Color::new(to8(r), to8(g), to8(b)),
        NormalizedF32::new(a.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE),
    )
}

fn rule(r: Fill) -> FillRule {
    match r {
        Fill::NonZero => FillRule::NonZero,
        Fill::EvenOdd => FillRule::EvenOdd,
    }
}

/// Картинка PDF из пикселей с премультипликацией (PDF ждёт прямую альфу).
fn image_of(pixmap: &vello_cpu::Pixmap) -> Image {
    let mut data = pixmap.data_as_u8_slice().to_vec();
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a == 0 {
            px.fill(0);
        } else if a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Image::from_rgba8(data, pixmap.width() as u32, pixmap.height() as u32)
}

/// Шрифт прогона для PDF: байты шрифта и положение на осях вариативного
/// шрифта — вес и ширина из начертания прогона, остальные оси — из его
/// нормализованных координат (без `avar`). `None` — шрифт не читается или
/// `krilla` на нём упадёт (см. `krilla_safe`): текст тогда рисуется контурами.
fn font_of(run: &GlyphRun) -> Option<Font> {
    let data = run.font.data.clone();
    let index = run.font.index;
    let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(data);
    let data = Data::from(bytes);
    let font = skrifa::FontRef::from_index(data.as_ref(), index).ok()?;
    let mut axes = Vec::new();
    if run.coords.iter().any(|c| *c != 0) {
        for (i, axis) in font.axes().iter().enumerate() {
            let tag = axis.tag().to_be_bytes();
            let n = run.coords.get(i).copied().unwrap_or(0) as f32 / 16384.0;
            let value = match &tag {
                b"wght" => run.face.weight,
                b"wdth" => run.face.stretch * 100.0,
                _ => {
                    let (min, def, max) =
                        (axis.min_value(), axis.default_value(), axis.max_value());
                    if n >= 0.0 {
                        def + n * (max - def)
                    } else {
                        def + n * (def - min)
                    }
                }
            };
            axes.push((Tag::new(&tag), value));
        }
    }
    if !krilla_safe(&font, &axes) {
        return None;
    }
    if axes.is_empty() {
        Font::new(data, index)
    } else {
        Font::new_variable(data, index, &axes)
    }
}

/// Данные шрифта, на которых `krilla` 0.8.2 паникует (а в воркере паника
/// убивает ядро), — шрифты приходят от пользователя:
/// - PostScript-имя (name ID 6) режется по байтам: `cid.rs` `base_font_name`
///   (`[..min(len, 109)]`) и `type3.rs` (`[..min(len, 115)]`) — не на границе
///   символа падает в `doc.finish()`; берём только ASCII-имя, как и бывает;
/// - `cap_height` и `italic_angle` метрик (`font.rs`) идут в `FiniteF32::new(..)
///   .unwrap()` — они должны быть конечными.
///
/// Прочие `unwrap` krilla на пути текста (`FontRef::from_index` после проверки
/// в `FontInfo::new`, номера глифов в `u16`/`u8`, рамка `Rect::from_ltrb` из
/// заведомо проверенных границ) от данных шрифта не зависят.
fn krilla_safe(font: &skrifa::FontRef, axes: &[(Tag, f32)]) -> bool {
    let name_ok = font.name().map_or(true, |name| {
        name.name_record()
            .iter()
            .find_map(|n| {
                if n.name_id.get() == NameId::POSTSCRIPT_NAME {
                    return n.string(name.string_data()).ok().map(|s| s.to_string());
                }
                None
            })
            .is_none_or(|s| s.is_ascii())
    });
    let location = font
        .axes()
        .location(axes.iter().map(|(t, v)| (skrifa::Tag::new(t.get()), *v)));
    let metrics = font.metrics(Size::unscaled(), LocationRef::from(&location));
    name_ok && metrics.italic_angle.is_finite() && metrics.cap_height.is_none_or(f32::is_finite)
}

impl PdfBackend<'_, '_> {
    /// Учесть работу: `ops` вызовов рисования и ~`bytes` байт содержимого.
    /// Размер сверяется с пределом сразу, часы — раз в `CHECK_EVERY` вызовов.
    /// `false` — предел превышен, рисовать дальше не нужно.
    fn charge(&mut self, ops: usize, bytes: usize) -> bool {
        if self.failure.is_some() {
            return false;
        }
        self.calls = self.calls.saturating_add(ops);
        self.estimate = self.estimate.saturating_add(bytes);
        let due = self.calls >= self.next_check;
        if due {
            self.next_check = self.calls.saturating_add(CHECK_EVERY);
        }
        let verdict = if due {
            self.budget.check(self.estimate)
        } else {
            self.budget.check_bytes(self.estimate)
        };
        match verdict {
            Ok(()) => true,
            Err(e) => {
                self.failure = Some(e);
                false
            }
        }
    }

    fn fill_solid(&mut self, p: &Path, c: Rgba, r: FillRule) {
        let (color, opacity) = rgb_of(c);
        self.surface.set_stroke(None);
        self.surface.set_fill(Some(KFill {
            paint: color.into(),
            opacity,
            rule: r,
        }));
        self.surface.draw_path(p);
    }

    /// Картинка по форме `p` (координаты страницы): обрезка формой, внутри —
    /// одна картинка или плитки.
    fn fill_image(&mut self, p: &Path, bounds: Rect, r: FillRule, img: &ImagePaint) {
        let key = img.key;
        let image = self
            .cache
            .images
            .entry(key)
            .or_insert_with(|| image_of(&img.pixmap))
            .clone();
        let (iw, ih) = (img.pixmap.width() as f64, img.pixmap.height() as f64);
        let place = self.transform * img.transform;
        let Some(size) = KSize::from_wh(iw as f32, ih as f32) else {
            return;
        };
        self.surface.push_clip_path(p, &r);
        let alpha = NormalizedF32::new(img.alpha.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE);
        let layered = img.alpha < 1.0;
        if layered {
            self.surface.push_opacity(alpha);
        }
        match img.extend {
            Extend::Repeat => {
                if place.determinant().abs() > 1e-12 {
                    let b = place.inverse().transform_rect_bbox(bounds);
                    let (i0, i1) = ((b.x0 / iw).floor() as i64, (b.x1 / iw).ceil() as i64);
                    let (j0, j1) = ((b.y0 / ih).floor() as i64, (b.y1 / ih).ceil() as i64);
                    let n = ((i1 - i0).max(0) as u64).saturating_mul((j1 - j0).max(0) as u64);
                    if n as usize <= MAX_TILES {
                        // Каждая плитка — вызов рисования с оценкой размера.
                        if !self.charge(n as usize, TILE_BYTES * n as usize) {
                            self.surface.pop();
                            if layered {
                                self.surface.pop();
                            }
                            return;
                        }
                        for j in j0..j1 {
                            for i in i0..i1 {
                                let t = place * Affine::translate((i as f64 * iw, j as f64 * ih));
                                self.surface.push_transform(&transform(t));
                                self.surface.draw_image(image.clone(), size);
                                self.surface.pop();
                            }
                        }
                    } else if self.charge(1, PATH_BASE_BYTES) {
                        // Слишком много плиток для отдельных рисований: одна
                        // плитка — узор PDF (повторяется в программе просмотра).
                        let mut sb = self.surface.stream_builder();
                        let mut tile = sb.surface();
                        tile.draw_image(image, size);
                        tile.finish();
                        let stream = sb.finish();
                        let pattern = Pattern {
                            stream,
                            transform: transform(place),
                            width: size.width(),
                            height: size.height(),
                        };
                        self.surface.set_stroke(None);
                        self.surface.set_fill(Some(KFill {
                            paint: pattern.into(),
                            opacity: NormalizedF32::ONE,
                            rule: r,
                        }));
                        self.surface.draw_path(p);
                    }
                }
            }
            _ => {
                self.surface.push_transform(&transform(place));
                self.surface.draw_image(image, size);
                self.surface.pop();
            }
        }
        if layered {
            self.surface.pop();
        }
        self.surface.pop();
    }
}

impl RenderBackend for PdfBackend<'_, '_> {
    fn begin_frame(&mut self, _width: u16, _height: u16) {}

    fn size(&self) -> (u16, u16) {
        self.size
    }

    fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn set_color(&mut self, color: Rgba) {
        self.paint = Paint::Solid(color);
    }

    fn set_image(&mut self, image: &ImagePaint) {
        self.paint = Paint::Image(image.clone());
    }

    fn fill_path(&mut self, p: &BezPath, r: Fill) {
        if !self.charge(1, PATH_BASE_BYTES + PATH_ELEMENT_BYTES * p.elements().len()) {
            return;
        }
        let Some(kp) = path(p, self.transform) else {
            return;
        };
        match self.paint.clone() {
            Paint::Solid(c) => self.fill_solid(&kp, c, rule(r)),
            Paint::Image(img) => {
                let bounds = self.transform.transform_rect_bbox(p.bounding_box());
                self.fill_image(&kp, bounds, rule(r), &img);
            }
        }
    }

    fn fill_rect(&mut self, r: &Rect) {
        let mut p = BezPath::new();
        p.extend(r.path_elements(0.1));
        self.fill_path(&p, Fill::NonZero);
    }

    fn stroke_path(&mut self, p: &BezPath, stroke: &kurbo::Stroke) {
        if !self.charge(1, PATH_BASE_BYTES + PATH_ELEMENT_BYTES * p.elements().len()) {
            return;
        }
        let Paint::Solid(c) = self.paint else {
            return;
        };
        let Some(kp) = path(p, Affine::IDENTITY) else {
            return;
        };
        let (color, opacity) = rgb_of(c);
        let line_join = match stroke.join {
            kurbo::Join::Miter => LineJoin::Miter,
            kurbo::Join::Round => LineJoin::Round,
            kurbo::Join::Bevel => LineJoin::Bevel,
        };
        let line_cap = match stroke.start_cap {
            kurbo::Cap::Butt => LineCap::Butt,
            kurbo::Cap::Round => LineCap::Round,
            kurbo::Cap::Square => LineCap::Square,
        };
        self.surface.push_transform(&transform(self.transform));
        self.surface.set_fill(None);
        self.surface.set_stroke(Some(KStroke {
            paint: color.into(),
            width: finite(stroke.width),
            miter_limit: finite(stroke.miter_limit),
            line_cap,
            line_join,
            opacity,
            dash: None,
        }));
        self.surface.draw_path(&kp);
        self.surface.set_stroke(None);
        self.surface.pop();
    }

    fn push_clip(&mut self, p: &BezPath) {
        // Пустая обрезка не рисует ничего — как прямоугольник нулевой площади.
        let kp = path(p, self.transform).unwrap_or_else(|| {
            let mut b = PathBuilder::new();
            b.move_to(0.0, 0.0);
            b.line_to(0.0, 0.0);
            b.close();
            b.finish().expect("непустой путь")
        });
        self.surface.push_clip_path(&kp, &FillRule::NonZero);
    }

    fn pop_clip(&mut self) {
        self.surface.pop();
    }

    fn push_opacity_layer(&mut self, alpha: f32) {
        self.surface
            .push_opacity(NormalizedF32::new(alpha.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE));
    }

    fn pop_layer(&mut self) {
        self.surface.pop();
    }

    fn fill_glyphs(&mut self, run: &GlyphRun, to_device: Affine, color: Rgba) -> bool {
        if !self.text {
            return false;
        }
        if !self.charge(
            1,
            GLYPH_RUN_BYTES + GLYPH_BYTES * run.glyphs.len() + run.text.len(),
        ) {
            return true;
        }
        let Some(first) = run.glyphs.first().copied() else {
            return true;
        };
        let key: FontKey = (
            run.font.data.id(),
            run.font.index,
            run.coords.clone(),
            run.face.weight.to_bits(),
            run.face.stretch.to_bits(),
        );
        let font = self
            .cache
            .fonts
            .entry(key)
            .or_insert_with(|| font_of(run))
            .clone();
        let Some(font) = font else {
            return false;
        };
        let size = run.size;
        if !(size.is_finite() && size > 0.0) {
            return true;
        }
        let glyphs: Vec<KrillaGlyph> = run
            .glyphs
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let next = run.glyphs.get(i + 1).map_or(g.x + g.advance, |n| n.x);
                KrillaGlyph::new(
                    GlyphId::new(g.id),
                    (next - g.x) / size,
                    0.0,
                    -(g.y - first.y) / size,
                    0.0,
                    g.cluster[0] as usize..g.cluster[1] as usize,
                    None,
                )
            })
            .collect();
        // Синтетический курсив — наклон вокруг базовой линии.
        let skew = run.skew.map_or(Affine::IDENTITY, |deg| {
            let by = first.y as f64;
            Affine::translate((0.0, by))
                * Affine::skew(-(deg as f64).to_radians().tan(), 0.0)
                * Affine::translate((0.0, -by))
        });
        let (rgb, opacity) = rgb_of(color);
        self.surface.push_transform(&transform(to_device * skew));
        self.surface.set_stroke(None);
        self.surface.set_fill(Some(KFill {
            paint: rgb.into(),
            opacity,
            rule: FillRule::NonZero,
        }));
        self.surface.draw_glyphs(
            KPoint::from_xy(first.x, first.y),
            &glyphs,
            font,
            &run.text,
            size,
            false,
        );
        self.surface.pop();
        true
    }
}

/// Страницы: узел — страница размера его области (не больше `MAX_PDF_SIDE`).
/// Скрытые и неизвестные узлы пропускаются; не осталось ни одного — ошибка.
pub(crate) fn export_pdf(
    scene: &Scene,
    pages: &[NodeId],
    images: &dyn ImageProvider,
    fonts: &SharedFonts,
    budget: &ExportBudget,
) -> Result<Vec<u8>, ExportError> {
    match build(scene, pages, images, fonts, budget, true) {
        // Шрифт, который `krilla` не смог встроить (повреждённые таблицы и т. п.):
        // текст — контурами глифов, как у SVG с «Текстом кривыми».
        Err(Fail::Font) => build(scene, pages, images, fonts, budget, false),
        other => other,
    }
    .map_err(|f| match f {
        Fail::Export(e) => e,
        // Повтор без текста шрифта не бывает.
        Fail::Font => ExportError::Write("шрифт".into()),
    })
}

fn build(
    scene: &Scene,
    pages: &[NodeId],
    images: &dyn ImageProvider,
    fonts: &SharedFonts,
    budget: &ExportBudget,
    text: bool,
) -> Result<Vec<u8>, Fail> {
    let mut known = false;
    let mut list = Vec::new();
    for id in pages {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        known = true;
        if scene.at(slot).subtree_bounds.is_none() {
            continue;
        }
        let Some(area) = node_area(scene, slot) else {
            continue;
        };
        if !(area.width().is_finite() && area.height().is_finite())
            || area.width() < 0.5
            || area.height() < 0.5
        {
            continue;
        }
        list.push((slot, area));
    }
    if !known {
        return Err(ExportError::NotFound.into());
    }
    if list.is_empty() {
        return Err(ExportError::Empty.into());
    }
    let settings = SerializeSettings {
        // Тегированный PDF нужен для доступности, но без структуры документа теги
        // пусты: файл меньше и пишется быстрее.
        enable_tagging: false,
        ..SerializeSettings::default()
    };
    let mut doc = Document::new_with(settings);
    let mut cache = Cache::default();
    // Оценка размера содержимого уже нарисованных страниц.
    let mut carried = 0usize;
    for (slot, area) in list {
        let k = (MAX_PDF_SIDE / area.width().max(area.height())).min(1.0);
        let (w, h) = (area.width() * k, area.height() * k);
        let size = KSize::from_wh(w as f32, h as f32).ok_or(ExportError::Empty)?;
        let mut page = doc.start_page_with(PageSettings::new(size));
        let mut surface = page.surface();
        let mut backend = PdfBackend {
            surface: &mut surface,
            cache: &mut cache,
            size: (w.min(u16::MAX as f64) as u16, h.min(u16::MAX as f64) as u16),
            transform: Affine::IDENTITY,
            paint: Paint::Solid(Rgba::new([0.0, 0.0, 0.0, 1.0])),
            text,
            budget,
            calls: 0,
            next_check: CHECK_EVERY,
            estimate: carried,
            failure: None,
        };
        let ctx = DrawContext {
            scene,
            images,
            fonts,
            transform: Affine::scale(k) * Affine::translate(-area.origin().to_vec2()),
            scale: k,
            cull: None,
            lod: false,
            raster: false,
            retain: false,
        };
        let mut painter = Painter::with_raster_limit(0);
        painter.begin_frame();
        painter.draw(&mut backend, &ctx, &[slot]);
        let failure = backend.failure.take();
        carried = backend.estimate;
        surface.finish();
        page.finish();
        if let Some(e) = failure {
            return Err(e.into());
        }
        budget.check(carried)?;
    }
    let bytes = doc.finish().map_err(|e| match e {
        KrillaError::Font(..) => Fail::Font,
        e => Fail::Export(ExportError::Write(format!("{e:?}"))),
    })?;
    budget.check(bytes.len())?;
    Ok(bytes)
}
