//! Рисование текстового узла (спека подсистемы 3, раздел 6): глифы — заливка
//! контуров `skrifa` через трейт бэкенда (как подписи оверлеев), украшения —
//! прямоугольники, мелкий текст — полосы по строкам (LOD, как Figma при отдалении).

use std::time::Duration;

use ferrum_doc::{Props, TextSizing};
use ferrum_text::{text_color, Fonts, LineBox, TextLayout};
use kurbo::{Affine, Rect};
use vello_cpu::peniko::Fill;

use crate::backend::{RenderBackend, Rgba};
use crate::display::color;

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

/// Глифы и украшения раскладки. `to_device` — локальные координаты узла →
/// пиксели устройства.
pub(crate) fn paint_glyphs(
    backend: &mut impl RenderBackend,
    fonts: &mut Fonts,
    layout: &TextLayout,
    to_device: Affine,
    alpha: f32,
) {
    for run in &layout.runs {
        let upem = fonts.units_per_em(&run.font);
        let k = run.size as f64 / upem;
        // Синтетический курсив: наклон контура вправо на угол `skew`.
        let skew = run.skew.map_or(Affine::IDENTITY, |deg| {
            Affine::skew(-(deg as f64).to_radians().tan(), 0.0)
        });
        backend.set_color(color(run.color).multiply_alpha(alpha));
        for g in &run.glyphs {
            let path = fonts.outline(&run.font, g.id, &run.coords);
            if path.elements().is_empty() {
                continue;
            }
            backend.set_transform(
                to_device
                    * Affine::translate((g.x as f64, g.y as f64))
                    * skew
                    * Affine::scale_non_uniform(k, -k),
            );
            backend.fill_path(&path, Fill::NonZero);
        }
    }
    backend.set_transform(to_device);
    for d in &layout.decorations {
        backend.set_color(color(d.color).multiply_alpha(alpha));
        backend.fill_rect(&d.rect);
    }
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
