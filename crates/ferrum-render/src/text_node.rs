//! Рисование текстового узла (спека подсистемы 3, раздел 6): глифы — заливка
//! контуров `skrifa` через трейт бэкенда (как подписи оверлеев), украшения —
//! прямоугольники, мелкий текст — полосы по строкам (LOD, как Figma при отдалении).
//! Этап 2: заливки узла по порядку (цвет и изображение по рамке узла), заливка
//! участка из разметки, обводки текста — по контуру глифов (снаружи — под
//! заливкой, по центру и внутри — над ней, внутри — с обрезкой по глифам).

use std::time::Duration;

use ferrum_doc::{Fill as Paint, FillKind, Props, StrokeAlign, TextSizing};
use ferrum_text::{text_color, Fonts, GlyphRun, LineBox, TextLayout};
use kurbo::{Affine, BezPath, Join, Rect, Shape, Stroke};
use vello_cpu::peniko::Fill;

use crate::backend::{ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::display::{color, IMAGE_PLACEHOLDER};
use crate::draw::place_image;
use crate::image::ImageProvider;

/// Строка ниже стольких пикселей устройства рисуется полосой.
pub const TEXT_LOD_PX: f64 = 3.0;
/// Прозрачность полосы LOD (спека 6).
pub const TEXT_GREEK_ALPHA: f32 = 0.25;
/// Сколько времени за кадр тратить на раскладку текстов, ещё не разложенных:
/// остальные — полосами по оценке строк, раскладка догоняет в следующих кадрах.
/// Хотя бы одна раскладка за кадр делается всегда.
pub const TEXT_LAYOUT_TIME_PER_FRAME: Duration = Duration::from_millis(4);
/// Межстрочный интервал «Авто» для оценки строк без раскладки, в кеглях.
const ESTIMATED_LINE_EM: f64 = 1.2;

/// Полоса строки: середина строки высотой 60 % — как строка текста издалека.
fn greek_bar(r: Rect) -> Rect {
    let h = r.height();
    Rect::new(r.x0, r.y0 + 0.2 * h, r.x1, r.y1 - 0.2 * h)
}

/// Рисует строки полосами цвета текста с прозрачностью `TEXT_GREEK_ALPHA`.
pub(crate) fn paint_greek(
    backend: &mut impl RenderBackend,
    lines: &[LineBox],
    to_device: Affine,
    alpha: f32,
) {
    backend.set_transform(to_device);
    for l in lines {
        if l.rect.width() <= 0.0 {
            continue;
        }
        backend.set_color(color(l.color).multiply_alpha(TEXT_GREEK_ALPHA * alpha));
        backend.fill_rect(&greek_bar(l.rect));
    }
}

/// Строки без раскладки — для полос, пока текст не разложен: кегль × 1.2 на
/// строку, во всю ширину рамки; у `AutoWidth` — по строке на абзац.
pub(crate) fn estimate_lines(p: &Props) -> Vec<LineBox> {
    let Some(t) = &p.text else {
        return Vec::new();
    };
    let lh = (t.props.style.size * ESTIMATED_LINE_EM).max(0.1);
    let rows = match t.props.sizing {
        TextSizing::AutoWidth => t.content.text().matches('\n').count().max(1),
        _ => ((p.h / lh).floor() as usize).clamp(1, 10_000),
    };
    let c = text_color(&p.fills);
    (0..rows)
        .map(|i| LineBox {
            rect: Rect::new(0.0, i as f64 * lh, p.w, (i + 1) as f64 * lh),
            baseline: (i as f64 + 0.8) * lh,
            color: c,
        })
        .collect()
}

/// Матрица глифа: позиция на базовой линии, синтетический наклон, масштаб из
/// единиц шрифта (ось y шрифта — вверх).
fn glyph_transform(fonts: &mut Fonts, run: &GlyphRun, x: f32, y: f32) -> Affine {
    let k = run.size as f64 / fonts.units_per_em(&run.font);
    // Синтетический курсив: наклон контура вправо на угол `skew`.
    let skew = run.skew.map_or(Affine::IDENTITY, |deg| {
        Affine::skew(-(deg as f64).to_radians().tan(), 0.0)
    });
    Affine::translate((x as f64, y as f64)) * skew * Affine::scale_non_uniform(k, -k)
}

/// Контур глифов прогонов одним путём в координатах узла (заливка изображением
/// и обводка).
pub(crate) fn glyphs_path<'a>(
    fonts: &mut Fonts,
    runs: impl Iterator<Item = &'a GlyphRun>,
) -> BezPath {
    let mut out = BezPath::new();
    for run in runs {
        for g in &run.glyphs {
            let path = fonts.outline(&run.font, g.id, &run.coords);
            let t = glyph_transform(fonts, run, g.x, g.y);
            out.extend(path.elements().iter().map(|e| t * *e));
        }
    }
    out
}

/// Контуры текста узла в его локальных координатах — глифы всех прогонов и
/// прямоугольники подчёркивания и зачёркивания (спека подсистемы 3, раздел 10:
/// булевы операции, «Сплющить» и «Контур обводки» текста). `None` — шрифты ещё
/// грузятся (раскладка не окончательная) или узел не текст.
pub fn text_outline(fonts: &mut Fonts, p: &Props) -> Option<BezPath> {
    text_paths(fonts, p).map(|(mut glyphs, decorations)| {
        glyphs.extend(decorations);
        glyphs
    })
}

/// Только контуры глифов текста (без подчёркивания и зачёркивания) — то, что
/// рендер обводит. `None` — шрифты ещё грузятся или узел не текст.
pub fn text_glyphs_outline(fonts: &mut Fonts, p: &Props) -> Option<BezPath> {
    text_paths(fonts, p).map(|(glyphs, _)| glyphs)
}

/// Глифы и прямоугольники украшений текста узла по отдельности.
fn text_paths(fonts: &mut Fonts, p: &Props) -> Option<(BezPath, BezPath)> {
    let t = p.text.as_deref()?;
    let layout = fonts.layout(&ferrum_text::LayoutRequest::for_node(t, &p.fills, p.w, p.h));
    if layout.loading {
        return None;
    }
    let glyphs = glyphs_path(fonts, layout.runs.iter());
    let mut decorations = BezPath::new();
    for d in &layout.decorations {
        decorations.extend(d.rect.path_elements(0.1));
    }
    Some((glyphs, decorations))
}

/// Заливка глифов прогона краской `paint`. Изображение — по рамке узла `area`;
/// `false` — картинки ещё нет (нарисована заглушка).
#[allow(clippy::too_many_arguments)]
fn fill_run(
    backend: &mut impl RenderBackend,
    fonts: &mut Fonts,
    run: &GlyphRun,
    paint: &Paint,
    area: Rect,
    to_device: Affine,
    alpha: f32,
    images: &dyn ImageProvider,
) -> bool {
    // Скрытая или прозрачная заливка не рисует ничего — и для участка тоже
    // (не откатывается ни к заливкам узла, ни к чёрному).
    if !paint.visible || paint.opacity <= 0.0 {
        return true;
    }
    let a = alpha * paint.opacity.clamp(0.0, 1.0) as f32;
    let image = match (paint.kind, &paint.image_hash) {
        (FillKind::Image, Some(h)) => Some(h),
        (FillKind::Image, None) => return true,
        (FillKind::Solid, _) => None,
    };
    let Some(hash) = image else {
        backend.set_color(color(paint.color).multiply_alpha(a));
        for g in &run.glyphs {
            let path = fonts.outline(&run.font, g.id, &run.coords);
            if path.elements().is_empty() {
                continue;
            }
            backend.set_transform(to_device * glyph_transform(fonts, run, g.x, g.y));
            backend.fill_path(&path, Fill::NonZero);
        }
        return true;
    };
    let path = glyphs_path(fonts, std::iter::once(run));
    backend.set_transform(to_device);
    if !(area.width() > 0.0 && area.height() > 0.0) {
        return true;
    }
    let Some(pixmap) = images.image(hash) else {
        backend.set_color(IMAGE_PLACEHOLDER.multiply_alpha(a));
        backend.fill_path(&path, Fill::NonZero);
        return false;
    };
    let (iw, ih) = (pixmap.width() as f64, pixmap.height() as f64);
    let (transform, extend, _) = place_image(paint.scale_mode, area, iw, ih);
    backend.set_image(&ImagePaint {
        key: ImageKey::for_hash(hash),
        pixmap,
        transform,
        extend,
        alpha: a,
    });
    backend.fill_path(&path, Fill::NonZero);
    true
}

/// Глифы, украшения и обводки раскладки текста узла `props`. `to_device` —
/// локальные координаты узла → пиксели устройства. `false` — какая-то картинка
/// заливки ещё не готова (кадр неполный).
pub(crate) fn paint_glyphs(
    backend: &mut impl RenderBackend,
    fonts: &mut Fonts,
    layout: &TextLayout,
    props: &Props,
    to_device: Affine,
    alpha: f32,
    images: &dyn ImageProvider,
) -> bool {
    let area = Rect::new(0.0, 0.0, props.w, props.h);
    let fills: Vec<&Paint> = props
        .fills
        .iter()
        .filter(|f| f.visible && f.opacity > 0.0)
        .collect();
    let strokes: Vec<_> = props
        .strokes
        .iter()
        .filter(|s| s.width > 0.0 && s.color.a > 0)
        .collect();
    let outline = (!strokes.is_empty()).then(|| glyphs_path(fonts, layout.runs.iter()));
    let mut ready = true;
    for run in &layout.runs {
        match &run.paint {
            Some(p) => ready &= fill_run(backend, fonts, run, p, area, to_device, alpha, images),
            None => {
                for f in &fills {
                    ready &= fill_run(backend, fonts, run, f, area, to_device, alpha, images);
                }
            }
        }
    }
    backend.set_transform(to_device);
    // Украшения берут ту же видимую краску, что и глифы; нет краски — не рисуются.
    for d in &layout.decorations {
        let (c, a) = match &d.paint {
            Some(p) if p.visible && p.opacity > 0.0 => {
                let c = match p.kind {
                    FillKind::Solid => color(p.color),
                    FillKind::Image => color(d.color),
                };
                (c, alpha * p.opacity.clamp(0.0, 1.0) as f32)
            }
            Some(_) => continue,
            None if fills.is_empty() => continue,
            None => (color(d.color), alpha),
        };
        backend.set_color(c.multiply_alpha(a));
        backend.fill_rect(&d.rect);
    }
    // Обводки — поверх заливок, в порядке списка.
    if let Some(path) = &outline {
        for s in &strokes {
            glyph_stroke(backend, path, s, to_device, alpha);
        }
    }
    ready
}

/// Обводка контура глифов (стык — острый, как у обводок фигур). По центру — как
/// есть; внутри — двойной толщины с обрезкой по глифам; снаружи — двойной
/// толщины с обрезкой по внешности глифов (кольцо только снаружи, заливка не
/// нужна). Перекрывающиеся контуры одного глифа (вариативные шрифты) обводятся
/// и по внутренним швам, как при любой обводке контура.
fn glyph_stroke(
    backend: &mut impl RenderBackend,
    path: &BezPath,
    s: &ferrum_doc::Stroke,
    to_device: Affine,
    alpha: f32,
) {
    let stroke = |width: f64| Stroke::new(width).with_join(Join::Miter);
    backend.set_transform(to_device);
    backend.set_color(color(s.color).multiply_alpha(alpha));
    match s.align {
        StrokeAlign::Center => backend.stroke_path(path, &stroke(s.width)),
        StrokeAlign::Inside => {
            backend.push_clip(path);
            backend.stroke_path(path, &stroke(2.0 * s.width));
            backend.pop_clip();
        }
        StrokeAlign::Outside => {
            backend.push_clip(&exterior(path, 2.0 * s.width + 1.0));
            backend.stroke_path(path, &stroke(2.0 * s.width));
            backend.pop_clip();
        }
    }
}

/// Внешность контура: прямоугольник вокруг (шире на `margin`) и сам контур с
/// обратным обходом — при заливке по ненулевому правилу контур вырезается.
fn exterior(path: &BezPath, margin: f64) -> BezPath {
    let r = path.bounding_box().inflate(margin, margin);
    // Контуры глифов (внешние) идут в одну сторону; рамка — в противоположную.
    let mut out = BezPath::new();
    out.move_to((r.x0, r.y0));
    out.line_to((r.x1, r.y0));
    out.line_to((r.x1, r.y1));
    out.line_to((r.x0, r.y1));
    out.close_path();
    // Площадь рамки при обходе выше положительна; глиф с той же ориентацией
    // нужно развернуть.
    if path.area() > 0.0 {
        out.extend(path.reverse_subpaths());
    } else {
        out.extend(path.elements().iter().copied());
    }
    out
}

/// Цвет мелкого текстового узла (LOD-прямоугольник): цвет текста с той же
/// прозрачностью, что у полос.
pub(crate) fn lod_color(p: &Props) -> Rgba {
    color(text_color(&p.fills)).multiply_alpha(TEXT_GREEK_ALPHA)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_doc::{NodeKind, TextContent, TextData, TextProps};

    fn lines(text: &str, sizing: TextSizing) -> usize {
        let mut p = Props::defaults(NodeKind::Text);
        p.text = Some(std::sync::Arc::new(TextData {
            props: TextProps {
                sizing,
                ..TextProps::default()
            },
            content: TextContent::plain(text),
        }));
        estimate_lines(&p).len()
    }

    #[test]
    fn auto_width_estimate_counts_every_paragraph() {
        // Содержимое всегда оканчивается переводом строки (он закрывает последний
        // абзац), поэтому число переводов строки и есть число абзацев.
        assert_eq!(lines("a", TextSizing::AutoWidth), 1);
        assert_eq!(lines("a\nb", TextSizing::AutoWidth), 2);
        assert_eq!(lines("a\nb\n", TextSizing::AutoWidth), 2);
    }
}
