//! Текстовый узел (спека подсистемы 3, раздел 3): свойства узла, стиль символов
//! по умолчанию и содержимое — участки текста с разметкой.
//!
//! Содержимое в документе — свой `LoroText` на узел (ключ `text` его карты),
//! стили — разметка Loro на диапазонах. Позиции — в символах Unicode (скалярах):
//! так считает `LoroText` в нативной и wasm-сборке без фичи `wasm` у Loro.
//! Текст всегда кончается переводом строки: на нём лежит разметка последнего
//! абзаца (список, отступ), как в Quill; раскладка этот перевод строки не рисует.

use crate::error::{DocError, DocResult};
use crate::types::{Color, Fill};

/// Предел символов: `len` — длина вместе с последним переводом строки.
pub(crate) fn check_chars(len: usize) -> DocResult<()> {
    if len > MAX_TEXT_CHARS + 1 {
        return Err(DocError::TextTooLong);
    }
    Ok(())
}

/// Предел участков со стилями.
pub(crate) fn check_runs(runs: usize) -> DocResult<()> {
    if runs > MAX_TEXT_RUNS {
        return Err(DocError::TooManyTextRuns);
    }
    Ok(())
}

/// Предел символов текста узла (спека 3).
pub const MAX_TEXT_CHARS: usize = 100_000;
/// Предел участков с разметкой (спека 3).
pub const MAX_TEXT_RUNS: usize = 10_000;

/// Шрифт: семейство и начертание, как `fontName` в Figma («Inter», «Semi Bold Italic»).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontName {
    pub family: String,
    pub style: String,
}

impl FontName {
    pub fn new(family: impl Into<String>, style: impl Into<String>) -> FontName {
        FontName {
            family: family.into(),
            style: style.into(),
        }
    }

    /// Шрифт нового текста и запасной для отсутствующих — Inter Regular (как в Figma).
    pub fn inter_regular() -> FontName {
        FontName::new("Inter", "Regular")
    }
}

/// Режим размера текстового узла.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextSizing {
    /// Ширина и высота — по тексту, строки рвутся только переводами строк.
    AutoWidth,
    /// Ширина задана, высота — по тексту.
    AutoHeight,
    /// Оба размера заданы; лишнее обрезается (с многоточием, если `truncate`).
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignH {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignV {
    Top,
    Middle,
    Bottom,
}

/// Интерлиньяж: `Auto` — по метрикам шрифта.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    Auto,
    Px(f64),
    Percent(f64),
}

/// Трекинг: в пикселях или процентах кегля.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LetterSpacing {
    Px(f64),
    Percent(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decoration {
    None,
    Underline,
    Strike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextCase {
    None,
    Upper,
    Lower,
    Title,
    SmallCaps,
}

/// Список абзаца — разметка на его переводе строки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    None,
    Bullet,
    Ordered,
}

/// Наибольший уровень вложенности списка.
pub const MAX_LIST_INDENT: u8 = 5;

/// Стиль символов узла по умолчанию: ключи разметки, кроме заливки — у текста её
/// задаёт список `fills` узла (как в Figma: «Заливка» текстового слоя — цвет текста).
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub font: FontName,
    pub size: f64,
    pub line_height: LineHeight,
    pub letter_spacing: LetterSpacing,
    pub decoration: Decoration,
    pub case: TextCase,
}

impl Default for TextStyle {
    /// Умолчания Figma: Inter Regular 12, интерлиньяж «Авто», трекинг 0.
    fn default() -> Self {
        TextStyle {
            font: FontName::inter_regular(),
            size: 12.0,
            line_height: LineHeight::Auto,
            letter_spacing: LetterSpacing::Percent(0.0),
            decoration: Decoration::None,
            case: TextCase::None,
        }
    }
}

/// Свойства текстового узла, кроме содержимого.
#[derive(Debug, Clone, PartialEq)]
pub struct TextProps {
    pub sizing: TextSizing,
    pub align_h: AlignH,
    pub align_v: AlignV,
    pub paragraph_spacing: f64,
    /// Отступ первой строки абзаца.
    pub paragraph_indent: f64,
    /// Обрезка с многоточием (только при `Fixed` или `max_lines`).
    pub truncate: bool,
    /// Наибольшее число строк при обрезке; `None` — сколько поместится.
    pub max_lines: Option<u32>,
    pub style: TextStyle,
}

impl Default for TextProps {
    fn default() -> Self {
        TextProps {
            sizing: TextSizing::AutoWidth,
            align_h: AlignH::Left,
            align_v: AlignV::Top,
            paragraph_spacing: 0.0,
            paragraph_indent: 0.0,
            truncate: false,
            max_lines: None,
            style: TextStyle::default(),
        }
    }
}

/// Ключ разметки участка (имя ключа в Loro — `as_str`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttrKey {
    Font,
    Style,
    Size,
    Fill,
    LineHeight,
    LetterSpacing,
    Decoration,
    Case,
    List,
    Indent,
}

impl AttrKey {
    pub const ALL: [AttrKey; 10] = [
        AttrKey::Font,
        AttrKey::Style,
        AttrKey::Size,
        AttrKey::Fill,
        AttrKey::LineHeight,
        AttrKey::LetterSpacing,
        AttrKey::Decoration,
        AttrKey::Case,
        AttrKey::List,
        AttrKey::Indent,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AttrKey::Font => "font",
            AttrKey::Style => "style",
            AttrKey::Size => "size",
            AttrKey::Fill => "fill",
            AttrKey::LineHeight => "lineHeight",
            AttrKey::LetterSpacing => "letterSpacing",
            AttrKey::Decoration => "decoration",
            AttrKey::Case => "case",
            AttrKey::List => "list",
            AttrKey::Indent => "indent",
        }
    }

    /// Разметка абзаца (лежит на переводе строки) — не расширяется на вставку
    /// рядом; разметка символов наследуется вставкой на границе слева.
    pub fn is_paragraph(self) -> bool {
        matches!(self, AttrKey::List | AttrKey::Indent)
    }
}

/// Разметка участка: `None` — как у узла (или «не задано» у абзаца).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextAttrs {
    pub font: Option<String>,
    pub style: Option<String>,
    pub size: Option<f64>,
    pub fill: Option<Fill>,
    pub line_height: Option<LineHeight>,
    pub letter_spacing: Option<LetterSpacing>,
    pub decoration: Option<Decoration>,
    pub case: Option<TextCase>,
    pub list: Option<ListKind>,
    pub indent: Option<u8>,
}

impl TextAttrs {
    pub fn is_empty(&self) -> bool {
        *self == TextAttrs::default()
    }

    /// Задан ли ключ.
    pub fn has(&self, key: AttrKey) -> bool {
        match key {
            AttrKey::Font => self.font.is_some(),
            AttrKey::Style => self.style.is_some(),
            AttrKey::Size => self.size.is_some(),
            AttrKey::Fill => self.fill.is_some(),
            AttrKey::LineHeight => self.line_height.is_some(),
            AttrKey::LetterSpacing => self.letter_spacing.is_some(),
            AttrKey::Decoration => self.decoration.is_some(),
            AttrKey::Case => self.case.is_some(),
            AttrKey::List => self.list.is_some(),
            AttrKey::Indent => self.indent.is_some(),
        }
    }

    /// Шрифт участка с учётом умолчания узла.
    pub fn font_name(&self, default: &TextStyle) -> FontName {
        FontName::new(
            self.font.as_deref().unwrap_or(&default.font.family),
            self.style.as_deref().unwrap_or(&default.font.style),
        )
    }
}

/// Участок текста с одной разметкой.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub attrs: TextAttrs,
}

/// Содержимое текстового узла: участки по порядку. Соседние участки с одной
/// разметкой слиты, пустых нет; текст кончается переводом строки.
#[derive(Debug, Clone, PartialEq)]
pub struct TextContent {
    pub runs: Vec<TextRun>,
}

impl Default for TextContent {
    fn default() -> Self {
        TextContent::plain("")
    }
}

impl TextContent {
    /// Текст без разметки; перевод строки в конце добавляется, если его нет.
    pub fn plain(text: &str) -> TextContent {
        let mut s = text.to_string();
        if !s.ends_with('\n') {
            s.push('\n');
        }
        TextContent {
            runs: vec![TextRun {
                text: s,
                attrs: TextAttrs::default(),
            }],
        }
    }

    /// Текст целиком (с последним переводом строки).
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Длина в символах Unicode.
    pub fn char_len(&self) -> usize {
        self.runs.iter().map(|r| r.text.chars().count()).sum()
    }

    /// Пределы текста (спека 3): символов не больше `MAX_TEXT_CHARS` (последний
    /// перевод строки не считается) и участков со стилями не больше `MAX_TEXT_RUNS`.
    pub fn check_limits(&self) -> DocResult<()> {
        check_chars(self.char_len())?;
        check_runs(self.styled_runs())
    }

    /// Участки с разметкой (для предела `MAX_TEXT_RUNS`).
    pub fn styled_runs(&self) -> usize {
        self.runs.iter().filter(|r| !r.attrs.is_empty()).count()
    }

    /// Все шрифты содержимого с учётом умолчания узла, без повторов, по порядку.
    pub fn fonts(&self, default: &TextStyle) -> Vec<FontName> {
        let mut out: Vec<FontName> = Vec::new();
        for r in &self.runs {
            let f = r.attrs.font_name(default);
            if !out.contains(&f) {
                out.push(f);
            }
        }
        if out.is_empty() {
            out.push(default.font.clone());
        }
        out
    }

    /// Участки диапазона символов `range` (фрагмент: перевод строки в конце не
    /// добавляется). Диапазон за концом текста обрезается.
    pub fn slice(&self, range: std::ops::Range<usize>) -> Vec<TextRun> {
        let mut out: Vec<TextRun> = Vec::new();
        let mut at = 0;
        for r in &self.runs {
            let n = r.text.chars().count();
            let (lo, hi) = (range.start.max(at), range.end.min(at + n));
            if lo < hi {
                let text: String = r.text.chars().skip(lo - at).take(hi - lo).collect();
                match out.last_mut() {
                    Some(last) if last.attrs == r.attrs => last.text.push_str(&text),
                    _ => out.push(TextRun {
                        text,
                        attrs: r.attrs.clone(),
                    }),
                }
            }
            at += n;
            if at >= range.end {
                break;
            }
        }
        out
    }

    /// Разметка символа в позиции `at`; за концом — разметка последнего символа
    /// (перевода строки).
    pub fn attrs_at(&self, at: usize) -> &TextAttrs {
        static PLAIN: std::sync::OnceLock<TextAttrs> = std::sync::OnceLock::new();
        let mut pos = 0;
        for r in &self.runs {
            let n = r.text.chars().count();
            if at < pos + n {
                return &r.attrs;
            }
            pos += n;
        }
        self.runs
            .last()
            .map(|r| &r.attrs)
            .unwrap_or_else(|| PLAIN.get_or_init(TextAttrs::default))
    }

    /// Соседние участки с одной разметкой сливаются, пустые выбрасываются: так
    /// содержимое из буфера обмена или панели сравнивается с документом.
    pub fn normalize(&mut self) {
        let mut out: Vec<TextRun> = Vec::with_capacity(self.runs.len());
        for r in std::mem::take(&mut self.runs) {
            if r.text.is_empty() {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.attrs == r.attrs => last.text.push_str(&r.text),
                _ => out.push(r),
            }
        }
        self.runs = out;
    }

    /// Границы участков в символах, по порядку.
    pub fn run_ranges(&self) -> Vec<std::ops::Range<usize>> {
        let mut at = 0;
        self.runs
            .iter()
            .map(|r| {
                let n = r.text.chars().count();
                let range = at..at + n;
                at += n;
                range
            })
            .collect()
    }
}

/// Текст узла целиком: свойства и содержимое. В свойствах узла (`Props::text`)
/// лежит за `Arc`: копии свойств (зеркало, сцена, превью жеста) не копируют
/// сотни килобайт текста.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextData {
    pub props: TextProps,
    pub content: TextContent,
}

/// Цвет текста по умолчанию в Figma — чёрный.
pub const TEXT_COLOR: Color = Color::BLACK;
