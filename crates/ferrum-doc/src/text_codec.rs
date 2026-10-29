//! Текст узла в Loro: свойства текста — ключи карты узла (как остальные
//! свойства), содержимое — дочерний `LoroText` с разметкой, правки — дельты Loro
//! (формат Quill) и обратные к ним для отмены.
//!
//! `FxHashMap` атрибутов дельты — тот же тип, что в `loro::TextDelta`; он
//! публично реэкспортируется `loro-internal` (это не внутренний API).

use loro::{
    Container, ExpandType, LoroMap, LoroText, LoroValue, StyleConfig, StyleConfigMap, TextDelta,
    ValueOrContainer,
};
use loro_internal::FxHashMap;

use crate::codec::{as_bool, as_f64, as_str, field, fill_value, obj, read_fill, value};
use crate::error::{DocError, DocResult};
use crate::text::*;

/// Ключ дочернего `LoroText` в карте текстового узла.
pub(crate) const KEY_TEXT: &str = "text";

pub(crate) mod key {
    pub const SIZING: &str = "sizing";
    pub const ALIGN_H: &str = "alignH";
    pub const ALIGN_V: &str = "alignV";
    pub const PARAGRAPH_SPACING: &str = "paragraphSpacing";
    pub const PARAGRAPH_INDENT: &str = "paragraphIndent";
    pub const TRUNCATE: &str = "truncate";
    pub const MAX_LINES: &str = "maxLines";
    // Умолчания стиля — те же ключи, что у разметки.
    pub const FONT: &str = "font";
    pub const STYLE: &str = "style";
    pub const SIZE: &str = "size";
    pub const LINE_HEIGHT: &str = "lineHeight";
    pub const LETTER_SPACING: &str = "letterSpacing";
    pub const DECORATION: &str = "decoration";
    pub const CASE: &str = "case";
}

/// Как разметка расширяется при вставке на границе (спека 3): символьная
/// наследуется вставкой слева (`After`, как в текстовых редакторах), разметка
/// абзаца на переводе строки не расширяется. Настройка не хранится в документе —
/// её ставит каждый `LoroDoc` страницы (`PageDoc::wrap`).
pub(crate) fn style_config() -> StyleConfigMap {
    let mut map = StyleConfigMap::new();
    for k in AttrKey::ALL {
        let expand = if k.is_paragraph() {
            ExpandType::None
        } else {
            ExpandType::After
        };
        map.insert(k.as_str().into(), StyleConfig { expand });
    }
    map
}

fn sizing_str(v: TextSizing) -> &'static str {
    match v {
        TextSizing::AutoWidth => "autoWidth",
        TextSizing::AutoHeight => "autoHeight",
        TextSizing::Fixed => "fixed",
    }
}

fn align_h_str(v: AlignH) -> &'static str {
    match v {
        AlignH::Left => "left",
        AlignH::Center => "center",
        AlignH::Right => "right",
        AlignH::Justify => "justify",
    }
}

fn align_v_str(v: AlignV) -> &'static str {
    match v {
        AlignV::Top => "top",
        AlignV::Middle => "middle",
        AlignV::Bottom => "bottom",
    }
}

fn decoration_str(v: Decoration) -> &'static str {
    match v {
        Decoration::None => "none",
        Decoration::Underline => "underline",
        Decoration::Strike => "strike",
    }
}

fn case_str(v: TextCase) -> &'static str {
    match v {
        TextCase::None => "none",
        TextCase::Upper => "upper",
        TextCase::Lower => "lower",
        TextCase::Title => "title",
        TextCase::SmallCaps => "smallCaps",
    }
}

fn list_str(v: ListKind) -> &'static str {
    match v {
        ListKind::None => "none",
        ListKind::Bullet => "bullet",
        ListKind::Ordered => "ordered",
    }
}

fn parse_sizing(s: &str) -> Option<TextSizing> {
    Some(match s {
        "autoWidth" => TextSizing::AutoWidth,
        "autoHeight" => TextSizing::AutoHeight,
        "fixed" => TextSizing::Fixed,
        _ => return None,
    })
}

fn parse_align_h(s: &str) -> Option<AlignH> {
    Some(match s {
        "left" => AlignH::Left,
        "center" => AlignH::Center,
        "right" => AlignH::Right,
        "justify" => AlignH::Justify,
        _ => return None,
    })
}

fn parse_align_v(s: &str) -> Option<AlignV> {
    Some(match s {
        "top" => AlignV::Top,
        "middle" => AlignV::Middle,
        "bottom" => AlignV::Bottom,
        _ => return None,
    })
}

fn parse_decoration(s: &str) -> Option<Decoration> {
    Some(match s {
        "none" => Decoration::None,
        "underline" => Decoration::Underline,
        "strike" => Decoration::Strike,
        _ => return None,
    })
}

fn parse_case(s: &str) -> Option<TextCase> {
    Some(match s {
        "none" => TextCase::None,
        "upper" => TextCase::Upper,
        "lower" => TextCase::Lower,
        "title" => TextCase::Title,
        "smallCaps" => TextCase::SmallCaps,
        _ => return None,
    })
}

fn parse_list(s: &str) -> Option<ListKind> {
    Some(match s {
        "none" => ListKind::None,
        "bullet" => ListKind::Bullet,
        "ordered" => ListKind::Ordered,
        _ => return None,
    })
}

/// `"auto"` или `{ unit: "px" | "percent", value }`.
fn line_height_value(v: LineHeight) -> LoroValue {
    match v {
        LineHeight::Auto => LoroValue::from("auto"),
        LineHeight::Px(px) => obj(vec![("unit", "px".into()), ("value", px.into())]),
        LineHeight::Percent(p) => obj(vec![("unit", "percent".into()), ("value", p.into())]),
    }
}

fn read_line_height(v: &LoroValue) -> Option<LineHeight> {
    if as_str(v).as_deref() == Some("auto") {
        return Some(LineHeight::Auto);
    }
    let value = field(v, "value")
        .and_then(as_f64)
        .filter(|x| x.is_finite())?;
    match field(v, "unit").and_then(as_str).as_deref() {
        Some("px") => Some(LineHeight::Px(value)),
        Some("percent") => Some(LineHeight::Percent(value)),
        _ => None,
    }
}

fn letter_spacing_value(v: LetterSpacing) -> LoroValue {
    match v {
        LetterSpacing::Px(px) => obj(vec![("unit", "px".into()), ("value", px.into())]),
        LetterSpacing::Percent(p) => obj(vec![("unit", "percent".into()), ("value", p.into())]),
    }
}

fn read_letter_spacing(v: &LoroValue) -> Option<LetterSpacing> {
    let value = field(v, "value")
        .and_then(as_f64)
        .filter(|x| x.is_finite())?;
    match field(v, "unit").and_then(as_str).as_deref() {
        Some("px") => Some(LetterSpacing::Px(value)),
        Some("percent") => Some(LetterSpacing::Percent(value)),
        _ => None,
    }
}

/// Свойства текста ключами карты узла; `None` — ключ удаляется (`maxLines`).
fn text_props_entries(p: &TextProps) -> Vec<(&'static str, Option<LoroValue>)> {
    let s = &p.style;
    vec![
        (key::SIZING, Some(sizing_str(p.sizing).into())),
        (key::ALIGN_H, Some(align_h_str(p.align_h).into())),
        (key::ALIGN_V, Some(align_v_str(p.align_v).into())),
        (key::PARAGRAPH_SPACING, Some(p.paragraph_spacing.into())),
        (key::PARAGRAPH_INDENT, Some(p.paragraph_indent.into())),
        (key::TRUNCATE, Some(p.truncate.into())),
        (key::MAX_LINES, p.max_lines.map(|n| (n as i64).into())),
        (key::FONT, Some(s.font.family.as_str().into())),
        (key::STYLE, Some(s.font.style.as_str().into())),
        (key::SIZE, Some(s.size.into())),
        (key::LINE_HEIGHT, Some(line_height_value(s.line_height))),
        (
            key::LETTER_SPACING,
            Some(letter_spacing_value(s.letter_spacing)),
        ),
        (key::DECORATION, Some(decoration_str(s.decoration).into())),
        (key::CASE, Some(case_str(s.case).into())),
    ]
}

/// Записывает свойства текста целиком. `maxLines` без значения — ключ удаляется.
pub(crate) fn write_text_props(map: &LoroMap, p: &TextProps) -> DocResult<()> {
    for (k, v) in text_props_entries(p) {
        match v {
            Some(v) => map.insert(k, v)?,
            None => map.delete(k)?,
        }
    }
    Ok(())
}

/// Свойства текста из карты узла; отсутствующие и неверные — умолчания.
pub(crate) fn read_text_props(map: &LoroMap) -> TextProps {
    read_text_props_with(|k| value(map, k))
}

fn read_text_props_with(get: impl Fn(&str) -> Option<LoroValue>) -> TextProps {
    let d = TextProps::default();
    let s = |k: &str| get(k).as_ref().and_then(as_str);
    let num = |k: &str| get(k).as_ref().and_then(as_f64).filter(|v| v.is_finite());
    TextProps {
        sizing: s(key::SIZING)
            .and_then(|v| parse_sizing(&v))
            .unwrap_or(d.sizing),
        align_h: s(key::ALIGN_H)
            .and_then(|v| parse_align_h(&v))
            .unwrap_or(d.align_h),
        align_v: s(key::ALIGN_V)
            .and_then(|v| parse_align_v(&v))
            .unwrap_or(d.align_v),
        paragraph_spacing: num(key::PARAGRAPH_SPACING).unwrap_or(d.paragraph_spacing),
        paragraph_indent: num(key::PARAGRAPH_INDENT).unwrap_or(d.paragraph_indent),
        truncate: get(key::TRUNCATE)
            .as_ref()
            .and_then(as_bool)
            .unwrap_or(d.truncate),
        max_lines: num(key::MAX_LINES)
            .filter(|v| *v >= 1.0)
            .map(|v| v.min(u32::MAX as f64) as u32),
        style: TextStyle {
            font: FontName::new(
                s(key::FONT).unwrap_or(d.style.font.family),
                s(key::STYLE).unwrap_or(d.style.font.style),
            ),
            size: num(key::SIZE).filter(|v| *v > 0.0).unwrap_or(d.style.size),
            line_height: get(key::LINE_HEIGHT)
                .as_ref()
                .and_then(read_line_height)
                .unwrap_or(d.style.line_height),
            letter_spacing: get(key::LETTER_SPACING)
                .as_ref()
                .and_then(read_letter_spacing)
                .unwrap_or(d.style.letter_spacing),
            decoration: s(key::DECORATION)
                .and_then(|v| parse_decoration(&v))
                .unwrap_or(d.style.decoration),
            case: s(key::CASE)
                .and_then(|v| parse_case(&v))
                .unwrap_or(d.style.case),
        },
    }
}

/// Разметка участка в атрибуты Loro (только заданные ключи).
pub(crate) fn attrs_to_loro(a: &TextAttrs) -> FxHashMap<String, LoroValue> {
    let mut m = FxHashMap::default();
    let mut put = |k: AttrKey, v: LoroValue| {
        m.insert(k.as_str().to_string(), v);
    };
    if let Some(v) = &a.font {
        put(AttrKey::Font, v.as_str().into());
    }
    if let Some(v) = &a.style {
        put(AttrKey::Style, v.as_str().into());
    }
    if let Some(v) = a.size {
        put(AttrKey::Size, v.into());
    }
    if let Some(v) = &a.fill {
        put(AttrKey::Fill, fill_value(v));
    }
    if let Some(v) = a.line_height {
        put(AttrKey::LineHeight, line_height_value(v));
    }
    if let Some(v) = a.letter_spacing {
        put(AttrKey::LetterSpacing, letter_spacing_value(v));
    }
    if let Some(v) = a.decoration {
        put(AttrKey::Decoration, decoration_str(v).into());
    }
    if let Some(v) = a.case {
        put(AttrKey::Case, case_str(v).into());
    }
    if let Some(v) = a.list {
        put(AttrKey::List, list_str(v).into());
    }
    if let Some(v) = a.indent {
        put(AttrKey::Indent, (v as i64).into());
    }
    m
}

/// Атрибуты Loro в разметку. Неизвестные ключи (из будущих версий) и неверные
/// значения пропускаются; `null` — ключ снят.
pub(crate) fn attrs_from_loro<'a>(
    pairs: impl Iterator<Item = (&'a str, &'a LoroValue)>,
) -> TextAttrs {
    let mut a = TextAttrs::default();
    for (k, v) in pairs {
        match k {
            "font" => a.font = as_str(v),
            "style" => a.style = as_str(v),
            "size" => a.size = as_f64(v).filter(|x| x.is_finite() && *x > 0.0),
            "fill" => a.fill = read_fill(v),
            "lineHeight" => a.line_height = read_line_height(v),
            "letterSpacing" => a.letter_spacing = read_letter_spacing(v),
            "decoration" => a.decoration = as_str(v).and_then(|s| parse_decoration(&s)),
            "case" => a.case = as_str(v).and_then(|s| parse_case(&s)),
            "list" => a.list = as_str(v).and_then(|s| parse_list(&s)),
            "indent" => {
                a.indent = as_f64(v)
                    .filter(|x| (0.0..=MAX_LIST_INDENT as f64).contains(x))
                    .map(|x| x as u8)
            }
            _ => {}
        }
    }
    a
}

/// Дочерний `LoroText` узла, если он есть.
pub(crate) fn existing_text(map: &LoroMap) -> Option<LoroText> {
    match map.get(KEY_TEXT)? {
        ValueOrContainer::Container(Container::Text(t)) => Some(t),
        _ => None,
    }
}

/// Дочерний `LoroText` узла; нет — создаётся пустым (обычный дочерний
/// контейнер: текст создаёт тот же, кто создаёт узел).
pub(crate) fn text_container(map: &LoroMap) -> DocResult<LoroText> {
    match existing_text(map) {
        Some(t) => Ok(t),
        None => Ok(map.insert_container(KEY_TEXT, LoroText::new())?),
    }
}

/// Содержимое из `LoroText`: участки по порядку, соседние с одной разметкой слиты.
/// Текст без перевода строки в конце (повреждённый или чужой) получает его.
pub(crate) fn read_content(text: &LoroText) -> TextContent {
    let mut runs: Vec<TextRun> = Vec::new();
    if let LoroValue::List(items) = text.get_richtext_value() {
        for item in items.iter() {
            let Some(s) = field(item, "insert").and_then(as_str) else {
                continue;
            };
            if s.is_empty() {
                continue;
            }
            let attrs = match field(item, "attributes") {
                Some(LoroValue::Map(m)) => attrs_from_loro(m.iter().map(|(k, v)| (k.as_str(), v))),
                _ => TextAttrs::default(),
            };
            push_run(&mut runs, s, attrs);
        }
    }
    let mut content = TextContent { runs };
    if !content.text().ends_with('\n') {
        push_run(&mut content.runs, "\n".to_string(), TextAttrs::default());
    }
    content
}

fn push_run(runs: &mut Vec<TextRun>, text: String, attrs: TextAttrs) {
    match runs.last_mut() {
        Some(last) if last.attrs == attrs => last.text.push_str(&text),
        _ => runs.push(TextRun { text, attrs }),
    }
}

/// Дельта, вставляющая содержимое в начало текста с точной разметкой каждого участка.
pub(crate) fn insert_delta(content: &TextContent) -> Vec<TextDelta> {
    content
        .runs
        .iter()
        .map(|r| TextDelta::Insert {
            insert: r.text.clone(),
            attributes: Some(attrs_to_loro(&r.attrs)),
        })
        .collect()
}

fn delta_len(s: &str) -> usize {
    s.chars().count()
}

/// Обратная дельта: применённая после `delta`, возвращает текст к нынешнему
/// состоянию (Quill `invert`). Позиции и стиль удаляемого и переразмечаемого
/// берутся из текущего текста, поэтому вызывать — до применения `delta`.
pub(crate) fn invert(text: &LoroText, delta: &[TextDelta]) -> DocResult<Vec<TextDelta>> {
    let len = text.len_unicode();
    let mut out = Vec::new();
    let mut at = 0usize;
    for op in delta {
        match op {
            TextDelta::Retain { retain, attributes } => {
                let end = at
                    .checked_add(*retain)
                    .filter(|e| *e <= len)
                    .ok_or(DocError::TextRange(at.saturating_add(*retain), len))?;
                match attributes {
                    Some(attrs) if !attrs.is_empty() => {
                        for piece in text.slice_delta(at, end, loro::cursor::PosType::Unicode)? {
                            let TextDelta::Insert {
                                insert,
                                attributes: before,
                            } = piece
                            else {
                                continue;
                            };
                            let mut prev = FxHashMap::default();
                            for k in attrs.keys() {
                                let old = before
                                    .as_ref()
                                    .and_then(|b| b.get(k))
                                    .cloned()
                                    .unwrap_or(LoroValue::Null);
                                prev.insert(k.clone(), old);
                            }
                            out.push(TextDelta::Retain {
                                retain: delta_len(&insert),
                                attributes: Some(prev),
                            });
                        }
                    }
                    _ => out.push(TextDelta::Retain {
                        retain: *retain,
                        attributes: None,
                    }),
                }
                at = end;
            }
            TextDelta::Insert { insert, .. } => out.push(TextDelta::Delete {
                delete: delta_len(insert),
            }),
            TextDelta::Delete { delete } => {
                let end = at
                    .checked_add(*delete)
                    .filter(|e| *e <= len)
                    .ok_or(DocError::TextRange(at.saturating_add(*delete), len))?;
                for piece in text.slice_delta(at, end, loro::cursor::PosType::Unicode)? {
                    if let TextDelta::Insert { insert, attributes } = piece {
                        out.push(TextDelta::Insert {
                            insert,
                            attributes: Some(attributes.unwrap_or_default()),
                        });
                    }
                }
                at = end;
            }
        }
    }
    Ok(out)
}
