//! Кэш сцены: плоская таблица всех узлов страницы для мгновенного открытия.
//!
//! Раскодировать снапшот Loro на 100 000 узлов — больше секунды. Поэтому при сохранении
//! рядом со снапшотом пишется эта таблица: по ней холст рисуется сразу, а Loro
//! раскодируется в фоне. Таблица — производные данные: при несовпадении со снапшотом
//! она просто игнорируется.

use std::sync::Arc;

use loro::TreeID;

use crate::binary::{Reader, Writer};
use crate::boolean::BooleanOp;
use crate::text::*;
use crate::types::*;
use crate::vector::*;

/// Узел страницы в порядке обхода в глубину: родитель всегда раньше детей.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneRow {
    pub id: NodeId,
    /// Текущий `TreeID` узла в Loro — чтобы построить индекс без чтения Loro.
    pub(crate) tree_id: TreeID,
    /// Индекс строки родителя в той же таблице; `None` — корень страницы.
    pub parent: Option<u32>,
    pub props: Props,
}

const ROOT: u32 = u32::MAX;
const FLAG_VISIBLE: u8 = 1;
const FLAG_LOCKED: u8 = 2;
const FLAG_CLIP: u8 = 4;
/// Флаги отражения появились во второй версии таблицы (`SCENE_CACHE_VERSION = 2`).
const FLAG_FLIP_X: u8 = 8;
const FLAG_FLIP_Y: u8 = 16;

fn kind_code(k: NodeKind) -> u8 {
    match k {
        NodeKind::Frame => 0,
        NodeKind::Group => 1,
        NodeKind::Rect => 2,
        NodeKind::Ellipse => 3,
        NodeKind::Line => 4,
        NodeKind::Image => 5,
        // Третья версия таблицы (`SCENE_CACHE_VERSION = 3`): у строки текста после
        // обводок — свойства текста и содержимое (`write_text`).
        NodeKind::Text => 6,
        // Четвёртая версия (`SCENE_CACHE_VERSION = 4`): у каждой строки после
        // обводок — стиль обводки, у вектора после него — сеть (`write_vector`).
        NodeKind::Vector => 7,
        // Пятая версия (`SCENE_CACHE_VERSION = 5`): у булевой группы после стиля
        // обводки — код операции.
        NodeKind::Boolean => 8,
    }
}

fn kind_from(c: u8) -> Option<NodeKind> {
    [
        NodeKind::Frame,
        NodeKind::Group,
        NodeKind::Rect,
        NodeKind::Ellipse,
        NodeKind::Line,
        NodeKind::Image,
        NodeKind::Text,
        NodeKind::Vector,
        NodeKind::Boolean,
    ]
    .get(c as usize)
    .copied()
}

fn color(w: &mut Writer, c: Color) {
    w.bytes(&[c.r, c.g, c.b, c.a]);
}

fn read_color(r: &mut Reader) -> Option<Color> {
    let b = r.take(4)?;
    Some(Color::rgba(b[0], b[1], b[2], b[3]))
}

/// `None` — какая-то строка не помещается в формат таблицы (строка длиннее 65 535
/// байт, больше 255 заливок или обводок). Усечённый кэш разошёлся бы с документом,
/// поэтому для такой страницы кэш не пишется: она откроется из Loro.
pub(crate) fn encode(rows: &[SceneRow]) -> Option<Vec<u8>> {
    let mut w = Writer::new();
    w.u32(u32::try_from(rows.len()).ok()?);
    for row in rows {
        let p = &row.props;
        w.str16(&row.id.to_string())?;
        w.str16(&row.tree_id.to_string())?;
        w.u32(row.parent.unwrap_or(ROOT));
        w.u8(kind_code(p.kind));
        let mut flags = 0;
        if p.visible {
            flags |= FLAG_VISIBLE;
        }
        if p.locked {
            flags |= FLAG_LOCKED;
        }
        if p.clip_content {
            flags |= FLAG_CLIP;
        }
        if p.flip_x {
            flags |= FLAG_FLIP_X;
        }
        if p.flip_y {
            flags |= FLAG_FLIP_Y;
        }
        w.u8(flags);
        for v in [p.x, p.y, p.w, p.h, p.rotation, p.opacity, p.corner_radius] {
            w.f64(v);
        }
        w.str16(&p.name)?;
        w.u8(u8::try_from(p.fills.len()).ok()?);
        for f in &p.fills {
            write_fill(&mut w, f)?;
        }
        w.u8(u8::try_from(p.strokes.len()).ok()?);
        for s in &p.strokes {
            color(&mut w, s.color);
            w.f64(s.width);
            w.u8(match s.align {
                StrokeAlign::Inside => 0,
                StrokeAlign::Center => 1,
                StrokeAlign::Outside => 2,
            });
        }
        let s = &p.stroke_style;
        w.u8(code(&StrokeCap::ALL, s.cap_start));
        w.u8(code(&StrokeCap::ALL, s.cap_end));
        w.u8(code(&StrokeJoin::ALL, s.join));
        w.f64(s.miter_limit);
        if let Some(t) = &p.text {
            write_text(&mut w, t)?;
        }
        if let Some(v) = &p.vector {
            let bytes = v.encode();
            w.u32(u32::try_from(bytes.len()).ok()?);
            w.bytes(&bytes);
        }
        if let Some(op) = p.boolean_op {
            w.u8(code(&BooleanOp::ALL, op));
        }
    }
    Some(w.buf)
}

pub(crate) fn write_fill(w: &mut Writer, f: &Fill) -> Option<()> {
    w.u8(match f.kind {
        FillKind::Solid => 0,
        FillKind::Image => 1,
    });
    color(w, f.color);
    w.u8(match f.scale_mode {
        ScaleMode::Fill => 0,
        ScaleMode::Fit => 1,
        ScaleMode::Tile => 2,
    });
    w.f64(f.opacity);
    w.u8(f.visible as u8);
    w.str16(f.image_hash.as_deref().unwrap_or(""))
}

pub(crate) fn read_fill(r: &mut Reader) -> Option<Fill> {
    let kind = if r.u8()? == 1 {
        FillKind::Image
    } else {
        FillKind::Solid
    };
    let color = read_color(r)?;
    let scale_mode = match r.u8()? {
        1 => ScaleMode::Fit,
        2 => ScaleMode::Tile,
        _ => ScaleMode::Fill,
    };
    let opacity = r.f64()?;
    let visible = r.u8()? != 0;
    let hash = r.str16()?;
    Some(Fill {
        kind,
        color,
        image_hash: (!hash.is_empty()).then_some(hash),
        scale_mode,
        opacity,
        visible,
    })
}

/// Номер значения в списке всех значений перечисления (и обратно).
fn code<T: PartialEq + Copy>(all: &[T], v: T) -> u8 {
    all.iter().position(|x| *x == v).unwrap_or(0) as u8
}

fn decode_enum<T: Copy>(all: &[T], c: u8) -> Option<T> {
    all.get(c as usize).copied()
}

const SIZINGS: [TextSizing; 3] = [
    TextSizing::AutoWidth,
    TextSizing::AutoHeight,
    TextSizing::Fixed,
];
const ALIGNS_H: [AlignH; 4] = [AlignH::Left, AlignH::Center, AlignH::Right, AlignH::Justify];
const ALIGNS_V: [AlignV; 3] = [AlignV::Top, AlignV::Middle, AlignV::Bottom];
const DECORATIONS: [Decoration; 3] = [Decoration::None, Decoration::Underline, Decoration::Strike];
const CASES: [TextCase; 5] = [
    TextCase::None,
    TextCase::Upper,
    TextCase::Lower,
    TextCase::Title,
    TextCase::SmallCaps,
];
const LISTS: [ListKind; 3] = [ListKind::None, ListKind::Bullet, ListKind::Ordered];

fn write_line_height(w: &mut Writer, v: LineHeight) {
    let (tag, x) = match v {
        LineHeight::Auto => (0, 0.0),
        LineHeight::Px(x) => (1, x),
        LineHeight::Percent(x) => (2, x),
    };
    w.u8(tag);
    w.f64(x);
}

/// Число из файла: только конечное (как у читателя Loro); иначе строка — мусор.
fn finite(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

/// Размер шрифта: конечный и больше нуля (как у читателя Loro).
fn font_size(x: f64) -> Option<f64> {
    (x.is_finite() && x > 0.0).then_some(x)
}

fn read_line_height(r: &mut Reader) -> Option<LineHeight> {
    let (tag, x) = (r.u8()?, finite(r.f64()?)?);
    Some(match tag {
        0 => LineHeight::Auto,
        1 => LineHeight::Px(x),
        2 => LineHeight::Percent(x),
        _ => return None,
    })
}

fn write_letter_spacing(w: &mut Writer, v: LetterSpacing) {
    let (tag, x) = match v {
        LetterSpacing::Px(x) => (1, x),
        LetterSpacing::Percent(x) => (2, x),
    };
    w.u8(tag);
    w.f64(x);
}

fn read_letter_spacing(r: &mut Reader) -> Option<LetterSpacing> {
    let (tag, x) = (r.u8()?, finite(r.f64()?)?);
    Some(match tag {
        1 => LetterSpacing::Px(x),
        2 => LetterSpacing::Percent(x),
        _ => return None,
    })
}

/// Свойства текста и содержимое: участки с маской заданных ключей разметки
/// (бит — номер ключа в `AttrKey::ALL`) и значениями по порядку ключей.
fn write_text(w: &mut Writer, t: &TextData) -> Option<()> {
    let p = &t.props;
    w.u8(code(&SIZINGS, p.sizing));
    w.u8(code(&ALIGNS_H, p.align_h));
    w.u8(code(&ALIGNS_V, p.align_v));
    w.f64(p.paragraph_spacing);
    w.f64(p.paragraph_indent);
    w.u8(p.truncate as u8);
    w.u32(p.max_lines.unwrap_or(0));
    w.str16(&p.style.font.family)?;
    w.str16(&p.style.font.style)?;
    w.f64(p.style.size);
    write_line_height(w, p.style.line_height);
    write_letter_spacing(w, p.style.letter_spacing);
    w.u8(code(&DECORATIONS, p.style.decoration));
    w.u8(code(&CASES, p.style.case));
    w.u32(u32::try_from(t.content.runs.len()).ok()?);
    for run in &t.content.runs {
        w.str32(&run.text)?;
        let a = &run.attrs;
        let mask = AttrKey::ALL
            .iter()
            .enumerate()
            .filter(|(_, k)| a.has(**k))
            .fold(0u16, |m, (i, _)| m | (1 << i));
        w.u16(mask);
        if let Some(v) = &a.font {
            w.str16(v)?;
        }
        if let Some(v) = &a.style {
            w.str16(v)?;
        }
        if let Some(v) = a.size {
            w.f64(v);
        }
        if let Some(v) = &a.fill {
            write_fill(w, v)?;
        }
        if let Some(v) = a.line_height {
            write_line_height(w, v);
        }
        if let Some(v) = a.letter_spacing {
            write_letter_spacing(w, v);
        }
        if let Some(v) = a.decoration {
            w.u8(code(&DECORATIONS, v));
        }
        if let Some(v) = a.case {
            w.u8(code(&CASES, v));
        }
        if let Some(v) = a.list {
            w.u8(code(&LISTS, v));
        }
        if let Some(v) = a.indent {
            w.u8(v);
        }
    }
    Some(())
}

/// Свойства и содержимое текста из файла — не доверенные: те же правила, что у
/// читателя Loro (конечные числа, размер больше нуля, отступ списка в пределах),
/// и пределы текста (`check_limits`). Нарушение — `None`: кэш отбрасывается и
/// страница открывается из Loro, а не рисует NaN и не раскладывает мегабайты.
fn read_text(r: &mut Reader) -> Option<TextData> {
    let sizing = decode_enum(&SIZINGS, r.u8()?)?;
    let align_h = decode_enum(&ALIGNS_H, r.u8()?)?;
    let align_v = decode_enum(&ALIGNS_V, r.u8()?)?;
    let paragraph_spacing = finite(r.f64()?)?;
    let paragraph_indent = finite(r.f64()?)?;
    let truncate = r.u8()? != 0;
    let max_lines = Some(r.u32()?).filter(|n| *n > 0);
    let font = FontName::new(r.str16()?, r.str16()?);
    let size = font_size(r.f64()?)?;
    let line_height = read_line_height(r)?;
    let letter_spacing = read_letter_spacing(r)?;
    let decoration = decode_enum(&DECORATIONS, r.u8()?)?;
    let case = decode_enum(&CASES, r.u8()?)?;
    let n = r.u32()? as usize;
    // Участки без разметки чередуются с размеченными: больше — уже за пределом.
    if n > MAX_TEXT_RUNS * 2 + 1 {
        return None;
    }
    let mut runs = Vec::with_capacity(n);
    for _ in 0..n {
        let text = r.str32()?;
        let mask = r.u16()?;
        let on = |k: AttrKey| {
            let i = AttrKey::ALL.iter().position(|x| *x == k).unwrap_or(0);
            mask & (1 << i) != 0
        };
        let mut a = TextAttrs::default();
        if on(AttrKey::Font) {
            a.font = Some(r.str16()?);
        }
        if on(AttrKey::Style) {
            a.style = Some(r.str16()?);
        }
        if on(AttrKey::Size) {
            a.size = Some(font_size(r.f64()?)?);
        }
        if on(AttrKey::Fill) {
            a.fill = Some(read_fill(r)?);
        }
        if on(AttrKey::LineHeight) {
            a.line_height = Some(read_line_height(r)?);
        }
        if on(AttrKey::LetterSpacing) {
            a.letter_spacing = Some(read_letter_spacing(r)?);
        }
        if on(AttrKey::Decoration) {
            a.decoration = Some(decode_enum(&DECORATIONS, r.u8()?)?);
        }
        if on(AttrKey::Case) {
            a.case = Some(decode_enum(&CASES, r.u8()?)?);
        }
        if on(AttrKey::List) {
            a.list = Some(decode_enum(&LISTS, r.u8()?)?);
        }
        if on(AttrKey::Indent) {
            let v = r.u8()?;
            if v > MAX_LIST_INDENT {
                return None;
            }
            a.indent = Some(v);
        }
        runs.push(TextRun { text, attrs: a });
    }
    Some(TextData {
        props: TextProps {
            sizing,
            align_h,
            align_v,
            paragraph_spacing,
            paragraph_indent,
            truncate,
            max_lines,
            style: TextStyle {
                font,
                size,
                line_height,
                letter_spacing,
                decoration,
                case,
            },
        },
        content: {
            let content = TextContent { runs };
            content.check_limits().ok()?;
            content
        },
    })
}

/// `None` — таблица повреждена; вызывающий код тогда просто не использует кэш.
pub(crate) fn decode(bytes: &[u8]) -> Option<Vec<SceneRow>> {
    let mut r = Reader::new(bytes);
    let n = r.u32()? as usize;
    let mut rows = Vec::with_capacity(n.min(bytes.len() / 64));
    for i in 0..n {
        let id: NodeId = r.str16()?.parse().ok()?;
        let tree_id = TreeID::try_from(r.str16()?.as_str()).ok()?;
        let parent = match r.u32()? {
            ROOT => None,
            p if (p as usize) < i => Some(p),
            _ => return None,
        };
        let kind = kind_from(r.u8()?)?;
        let flags = r.u8()?;
        let mut nums = [0.0; 7];
        for v in &mut nums {
            *v = r.f64()?;
        }
        let name = r.str16()?;
        let mut fills = Vec::new();
        for _ in 0..r.u8()? {
            fills.push(read_fill(&mut r)?);
        }
        let mut strokes = Vec::new();
        for _ in 0..r.u8()? {
            let color = read_color(&mut r)?;
            let width = r.f64()?;
            let align = match r.u8()? {
                0 => StrokeAlign::Inside,
                2 => StrokeAlign::Outside,
                _ => StrokeAlign::Center,
            };
            strokes.push(Stroke {
                color,
                width,
                align,
            });
        }
        let stroke_style = StrokeStyle {
            cap_start: decode_enum(&StrokeCap::ALL, r.u8()?)?,
            cap_end: decode_enum(&StrokeCap::ALL, r.u8()?)?,
            join: decode_enum(&StrokeJoin::ALL, r.u8()?)?,
            miter_limit: Some(r.f64()?).filter(|v| MITER_LIMIT_RANGE.contains(v))?,
        };
        let text = match kind {
            NodeKind::Text => Some(Arc::new(read_text(&mut r)?)),
            _ => None,
        };
        // Сеть из файла — те же проверки, что у читателя Loro; неверная —
        // кэш отбрасывается, и страница читается из Loro (узел там скрывается).
        let vector = match kind {
            NodeKind::Vector => {
                let n = r.u32()? as usize;
                Some(Arc::new(VectorNetwork::decode(r.take(n)?).ok()?))
            }
            _ => None,
        };
        let boolean_op = match kind {
            NodeKind::Boolean => Some(decode_enum(&BooleanOp::ALL, r.u8()?)?),
            _ => None,
        };
        let [x, y, w, h, rotation, opacity, corner_radius] = nums;
        rows.push(SceneRow {
            id,
            tree_id,
            parent,
            props: Props {
                kind,
                name,
                x,
                y,
                w,
                h,
                rotation,
                opacity,
                visible: flags & FLAG_VISIBLE != 0,
                locked: flags & FLAG_LOCKED != 0,
                corner_radius,
                clip_content: flags & FLAG_CLIP != 0,
                flip_x: flags & FLAG_FLIP_X != 0,
                flip_y: flags & FLAG_FLIP_Y != 0,
                fills,
                strokes,
                stroke_style,
                text,
                vector,
                boolean_op,
            },
        });
    }
    Some(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    /// Строки страницы с одним текстом «Привет».
    fn text_rows() -> Vec<SceneRow> {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        doc.transact(page, vec![], |p| {
            let id = p.create_node(None, None, NodeKind::Text)?;
            p.set_text_content(id, &TextContent::plain("Привет"))?;
            Ok(vec![])
        })
        .unwrap();
        doc.page(page).unwrap().scene_rows().unwrap()
    }

    /// Кэш с текстом, изменённым `f`, — читается ли.
    fn decodes(f: impl FnOnce(&mut TextData)) -> bool {
        let mut rows = text_rows();
        let text = rows[0].props.text.as_mut().unwrap();
        f(Arc::make_mut(text));
        decode(&encode(&rows).unwrap()).is_some()
    }

    #[test]
    fn valid_text_row_round_trips() {
        let rows = text_rows();
        assert_eq!(decode(&encode(&rows).unwrap()), Some(rows));
    }

    #[test]
    fn non_finite_text_numbers_drop_the_cache() {
        assert!(!decodes(|t| t.props.style.size = f64::NAN));
        assert!(!decodes(|t| t.props.style.size = f64::INFINITY));
        assert!(!decodes(|t| t.props.style.size = 0.0));
        assert!(!decodes(|t| t.props.style.size = -3.0));
        assert!(!decodes(|t| t.props.paragraph_spacing = f64::NAN));
        assert!(!decodes(|t| t.props.paragraph_indent = f64::NEG_INFINITY));
        assert!(!decodes(
            |t| t.props.style.line_height = LineHeight::Px(f64::NAN)
        ));
        assert!(!decodes(
            |t| t.props.style.line_height = LineHeight::Percent(f64::INFINITY)
        ));
        assert!(!decodes(
            |t| t.props.style.letter_spacing = LetterSpacing::Px(f64::NAN)
        ));
        assert!(!decodes(|t| t.content.runs[0].attrs.size = Some(f64::NAN)));
        assert!(!decodes(
            |t| t.content.runs[0].attrs.line_height = Some(LineHeight::Px(f64::INFINITY))
        ));
        assert!(!decodes(
            |t| t.content.runs[0].attrs.indent = Some(MAX_LIST_INDENT + 1)
        ));
        // Предельные, но верные значения читаются.
        assert!(decodes(
            |t| t.content.runs[0].attrs.indent = Some(MAX_LIST_INDENT)
        ));
        assert!(decodes(
            |t| t.props.style.letter_spacing = LetterSpacing::Px(-2.0)
        ));
    }

    #[test]
    fn over_limit_text_drops_the_cache() {
        assert!(!decodes(
            |t| t.content = TextContent::plain(&"я".repeat(MAX_TEXT_CHARS + 1))
        ));
        assert!(decodes(
            |t| t.content = TextContent::plain(&"я".repeat(MAX_TEXT_CHARS))
        ));
        assert!(!decodes(|t| {
            t.content.runs = (0..=MAX_TEXT_RUNS)
                .map(|i| TextRun {
                    text: "a".into(),
                    attrs: TextAttrs {
                        size: Some(10.0 + (i % 2) as f64),
                        ..Default::default()
                    },
                })
                .collect();
        }));
    }
}
