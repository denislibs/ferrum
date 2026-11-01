//! Раскладка текстового узла (спека подсистемы 3, раздел 5): содержимое,
//! свойства и ширина рамки → строки, глифы с позициями и украшения в
//! координатах узла (0, 0 — левый верхний угол рамки).
//!
//! Каждый абзац раскладывается отдельно: у абзаца свой отступ списка, маркер
//! («•», «1.»), отступ первой строки и интервал после. Стили участков —
//! диапазоны `parley`; кисть — номер цвета в палитре раскладки. Регистр
//! (`upper`, `lower`, `title`) меняет текст до формирования, капитель —
//! функция OpenType `smcp`.

use std::ops::Range;

use ferrum_doc::{
    AlignH, AlignV, Color, Decoration, Fill, FillKind, FontName, LetterSpacing, LineHeight,
    ListKind, TextAttrs, TextCase, TextData, TextSizing, MAX_LIST_INDENT,
};
use kurbo::Rect;
use parley::{
    Alignment, AlignmentOptions, FontFamily, FontStyle, FontWeight, FontWidth, IndentOptions,
    Layout, OverflowWrap, PositionedLayoutItem, StyleProperty,
};

use crate::fonts::{FontStatus, Fonts, Resolved};

/// Ширина ступени отступа списка в кеглях первого символа абзаца.
pub const LIST_INDENT_EM: f64 = 1.5;

/// Что раскладывать.
#[derive(Debug, Clone, Copy)]
pub struct LayoutRequest<'a> {
    pub text: &'a TextData,
    /// Заливки узла: цвет текста по умолчанию — верхняя видимая сплошная.
    pub fills: &'a [Fill],
    /// Ширина рамки: строки переносятся по ней (`AutoHeight`, `Fixed`); у
    /// `AutoWidth` — `None`, строки рвутся только переводами строк.
    pub width: Option<f64>,
    /// Высота рамки (`Fixed`): обрезка и выравнивание по вертикали; `None` —
    /// высота по тексту.
    pub height: Option<f64>,
}

impl<'a> LayoutRequest<'a> {
    /// Запрос по свойствам узла: размеры рамки берутся по режиму размера.
    pub fn for_node(text: &'a TextData, fills: &'a [Fill], w: f64, h: f64) -> LayoutRequest<'a> {
        let sizing = text.props.sizing;
        LayoutRequest {
            text,
            fills,
            width: (sizing != TextSizing::AutoWidth).then_some(w),
            height: (sizing == TextSizing::Fixed).then_some(h),
        }
    }
}

/// Глиф: номер в шрифте и позиция левого края на базовой линии.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    /// Ширина глифа (шаг по базовой линии) — по ней ставится многоточие.
    pub advance: f32,
    /// Байты кластера глифа в `GlyphRun::text` (экспорт текста в SVG и PDF: какие
    /// символы он рисует).
    pub cluster: [u32; 2],
}

/// Начертание прогона в коллекции шрифтов (для многоточия тем же шрифтом).
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub family: String,
    pub weight: f32,
    pub italic: bool,
    pub stretch: f32,
}

/// Подряд идущие глифы одного шрифта, кегля и цвета.
#[derive(Debug, Clone)]
pub struct GlyphRun {
    pub font: parley::FontData,
    pub face: Face,
    pub size: f32,
    /// Нормализованные координаты вариативного шрифта (биты `F2Dot14`).
    pub coords: Vec<i16>,
    pub color: Color,
    /// Заливка участка (разметка `fill`): цвет или изображение; `None` — заливки
    /// узла (спека 6: заливки диапазонов — этап 2).
    pub paint: Option<Fill>,
    /// Синтетический наклон (курсив без своего файла), градусы.
    pub skew: Option<f32>,
    /// Трекинг участка в пикселях (экспорт `<text>` в SVG).
    pub letter_spacing: f32,
    /// Текст прогона (с учётом регистра), на который ссылаются кластеры глифов.
    pub text: String,
    pub glyphs: Vec<PlacedGlyph>,
}

impl GlyphRun {
    /// Символы, которые рисуют оставшиеся глифы (обрезка с многоточием снимает
    /// глифы с конца): от начала первого кластера до конца последнего.
    pub fn glyph_text(&self) -> &str {
        let lo = self.glyphs.iter().map(|g| g.cluster[0]).min().unwrap_or(0) as usize;
        let hi = self.glyphs.iter().map(|g| g.cluster[1]).max().unwrap_or(0) as usize;
        self.text.get(lo..hi.max(lo)).unwrap_or("")
    }
}

/// Строка: прямоугольник (для LOD «полосой») и базовая линия.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineBox {
    pub rect: Rect,
    pub baseline: f64,
    /// Цвет полосы LOD — цвет первого участка строки.
    pub color: Color,
}

/// Подчёркивание или зачёркивание — прямоугольник цвета текста.
#[derive(Debug, Clone, PartialEq)]
pub struct DecorationBox {
    pub rect: Rect,
    pub color: Color,
    /// Заливка участка (как у глифов): украшение рисуется ею, а скрытая или
    /// прозрачная — не рисуется вовсе; `None` — заливки узла.
    pub paint: Option<Fill>,
}

/// Строка для правки текста: какие символы на ней, где она по вертикали и где
/// между кластерами может стоять курсор (спека 7). Позиции — символы Unicode
/// содержимого узла.
#[derive(Debug, Clone, PartialEq)]
pub struct CaretLine {
    /// Позиция первого символа строки.
    pub start: usize,
    /// Позиция за последним символом строки; у последней строки абзаца — позиция
    /// его перевода строки.
    pub end: usize,
    pub top: f64,
    pub bottom: f64,
    pub baseline: f64,
    /// Позиции курсора по возрастанию символа: `(символ, x)`, от `start` до `end`.
    pub stops: Vec<(usize, f64)>,
    /// Строка кончается абзацем, а не переносом.
    pub hard: bool,
}

/// Результат раскладки.
#[derive(Debug, Clone, Default)]
pub struct TextLayout {
    /// Размер содержимого: ширина самой длинной строки и высота всех строк с
    /// интервалами абзацев (у обрезанного — видимых).
    pub width: f64,
    pub height: f64,
    pub lines: Vec<LineBox>,
    pub runs: Vec<GlyphRun>,
    pub decorations: Vec<DecorationBox>,
    /// Шрифты, которых нет: разложены запасным (имена — как в документе).
    pub missing: Vec<FontName>,
    /// Какой-то шрифт ещё грузится или список вшитых не пришёл: раскладка не
    /// окончательная, рисовать её нельзя.
    pub loading: bool,
    /// Текст обрезан (обрезка с многоточием или по рамке).
    pub truncated: bool,
    /// Строки для правки (курсор, выделение, попадание) — все, и у обрезанного.
    pub carets: Vec<CaretLine>,
}

/// Стиль участка после разрешения шрифта.
#[derive(Debug, Clone)]
struct SpanStyle {
    resolved: Resolved,
    size: f64,
    line_height: LineHeight,
    letter_spacing: LetterSpacing,
    decoration: Decoration,
    case: TextCase,
    color: Color,
    /// Заливка участка из разметки (любого вида).
    paint: Option<Fill>,
}

/// Где в раскладке лежит абзац (для выравнивания текста «авто» по ширине).
struct ParaSpan {
    lines: Range<usize>,
    carets: Range<usize>,
    runs: Range<usize>,
    decos: Range<usize>,
    width: f64,
}

struct Paragraph<'a> {
    runs: Vec<(&'a str, &'a TextAttrs)>,
    /// Разметка перевода строки абзаца (список и его уровень).
    attrs: &'a TextAttrs,
}

/// Цвет текста по умолчанию: верхняя видимая сплошная заливка узла с её
/// прозрачностью; без такой — чёрный (градиенты и картинки — этап 2).
pub fn text_color(fills: &[Fill]) -> Color {
    fills
        .iter()
        .rev()
        .find(|f| f.visible && f.kind == FillKind::Solid)
        .map(|f| {
            let mut c = f.color;
            c.a = (c.a as f64 * f.opacity.clamp(0.0, 1.0)).round() as u8;
            c
        })
        .unwrap_or(Color::BLACK)
}

fn split_paragraphs(data: &TextData) -> Vec<Paragraph<'_>> {
    static PLAIN: std::sync::OnceLock<TextAttrs> = std::sync::OnceLock::new();
    let plain = PLAIN.get_or_init(TextAttrs::default);
    let mut out = Vec::new();
    let mut cur: Vec<(&str, &TextAttrs)> = Vec::new();
    for run in &data.content.runs {
        let mut rest = run.text.as_str();
        while let Some(i) = rest.find('\n') {
            if i > 0 {
                cur.push((&rest[..i], &run.attrs));
            }
            out.push(Paragraph {
                runs: std::mem::take(&mut cur),
                attrs: &run.attrs,
            });
            rest = &rest[i + 1..];
        }
        if !rest.is_empty() {
            cur.push((rest, &run.attrs));
        }
    }
    // Текст без перевода строки в конце (не должно быть) — последний абзац.
    if !cur.is_empty() {
        out.push(Paragraph {
            runs: cur,
            attrs: plain,
        });
    }
    out
}

/// Символ с учётом регистра — по одному: так каждый символ содержимого знает
/// свои байты в тексте раскладки (курсор и попадание, спека 7).
fn push_case(out: &mut String, c: char, case: TextCase, word_start: &mut bool) {
    match case {
        TextCase::Upper => out.extend(c.to_uppercase()),
        TextCase::Lower => out.extend(c.to_lowercase()),
        TextCase::Title => {
            if *word_start && c.is_alphabetic() {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            *word_start = !c.is_alphanumeric();
        }
        TextCase::None | TextCase::SmallCaps => out.push(c),
    }
}

/// Число из документа без NaN и бесконечности: иначе — `default` (файл чужой
/// сборки или испорченный кэш не должен довести NaN до `parley`).
fn finite_or(v: f64, default: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        default
    }
}

/// Байты текста абзаца → символы содержимого узла.
struct CharMap {
    /// Начало байтов каждого символа абзаца и его позиция в содержимом.
    starts: Vec<(usize, usize)>,
    /// Позиция перевода строки абзаца (за последним символом).
    end: usize,
    text_len: usize,
}

impl CharMap {
    fn char_at(&self, byte: usize) -> usize {
        if byte >= self.text_len {
            return self.end;
        }
        match self.starts.partition_point(|(b, _)| *b <= byte) {
            0 => self.starts.first().map_or(self.end, |s| s.1),
            i => self.starts[i - 1].1,
        }
    }
}

fn line_height(v: LineHeight) -> parley::LineHeight {
    match v {
        LineHeight::Auto => parley::LineHeight::MetricsRelative(1.0),
        // Нечисловой интерлиньяж из файла — «Авто», а не схлопнутые строки.
        LineHeight::Px(px) if !px.is_finite() => parley::LineHeight::MetricsRelative(1.0),
        LineHeight::Px(px) => parley::LineHeight::Absolute(px.max(0.0) as f32),
        LineHeight::Percent(p) => {
            parley::LineHeight::FontSizeRelative((finite_or(p, 100.0) / 100.0).max(0.0) as f32)
        }
    }
}

fn letter_spacing(v: LetterSpacing, size: f64) -> f32 {
    match v {
        LetterSpacing::Px(px) => finite_or(px, 0.0) as f32,
        LetterSpacing::Percent(p) => finite_or(size * p / 100.0, 0.0) as f32,
    }
}

impl Fonts {
    /// Раскладывает текст узла.
    pub fn layout(&mut self, req: &LayoutRequest) -> TextLayout {
        let props = &req.text.props;
        let default_color = text_color(req.fills);
        let mut out = TextLayout::default();
        let mut y = 0.0f64;
        // Абзацы без ширины рамки: (строки, прогоны, украшения, ширина) для
        // выравнивания по самой широкой строке.
        let mut spans: Vec<ParaSpan> = Vec::new();
        let mut counters = [0u32; MAX_LIST_INDENT as usize + 1];
        let paragraphs = split_paragraphs(req.text);
        // Позиция первого символа абзаца в содержимом узла.
        let mut para_start = 0usize;
        for (pi, para) in paragraphs.iter().enumerate() {
            if pi > 0 {
                y += finite_or(props.paragraph_spacing, 0.0).max(0.0);
            }
            let styles: Vec<SpanStyle> = if para.runs.is_empty() {
                vec![self.span_style(para.attrs, req, default_color, &mut out)]
            } else {
                para.runs
                    .iter()
                    .map(|(_, a)| self.span_style(a, req, default_color, &mut out))
                    .collect()
            };
            let list = para.attrs.list.unwrap_or(ListKind::None);
            let level = para.attrs.indent.unwrap_or(0).min(MAX_LIST_INDENT) as usize;
            let marker = match list {
                ListKind::None => {
                    counters = [0; MAX_LIST_INDENT as usize + 1];
                    None
                }
                ListKind::Bullet => {
                    counters[level..].iter_mut().for_each(|c| *c = 0);
                    Some("•".to_string())
                }
                ListKind::Ordered => {
                    counters[level + 1..].iter_mut().for_each(|c| *c = 0);
                    counters[level] += 1;
                    Some(format!("{}.", counters[level]))
                }
            };
            let first = &styles[0];
            let indent_x = if marker.is_some() {
                (level as f64 + 1.0) * LIST_INDENT_EM * first.size
            } else {
                0.0
            };
            // Текст абзаца с учётом регистра, диапазоны участков в байтах и
            // символы содержимого для курсора.
            let mut text = String::new();
            let mut ranges: Vec<Range<usize>> = Vec::new();
            let mut starts: Vec<(usize, usize)> = Vec::new();
            let mut word_start = true;
            let mut ci = para_start;
            for ((s, _), style) in para.runs.iter().zip(&styles) {
                let start = text.len();
                for c in s.chars() {
                    starts.push((text.len(), ci));
                    push_case(&mut text, c, style.case, &mut word_start);
                    ci += 1;
                }
                ranges.push(start..text.len());
            }
            let map = CharMap {
                starts,
                end: ci,
                text_len: text.len(),
            };
            para_start = ci + 1;
            // Пустой абзац — строка высотой своего стиля: невидимый пробел нулевой ширины.
            if text.is_empty() {
                text.push('\u{200B}');
                ranges.clear();
                ranges.push(0..text.len());
            }
            // Кисть участка — его номер: по ней прогон узнаёт цвет и начертание.
            let mut builder = self.lcx.ranged_builder(&mut self.fcx, &text, 1.0, false);
            for (i, (range, style)) in ranges.iter().zip(&styles).enumerate() {
                push_style(&mut builder, style, i as u32, range.clone());
            }
            let mut layout: Layout<u32> = builder.build(&text);
            let indent = finite_or(props.paragraph_indent, 0.0);
            if indent != 0.0 {
                layout.set_text_indent(indent as f32, IndentOptions::default());
            }
            let avail = req.width.map(|w| (w - indent_x).max(0.0) as f32);
            layout.break_all_lines(avail);
            layout.align(
                match props.align_h {
                    AlignH::Left => Alignment::Left,
                    AlignH::Center => Alignment::Center,
                    AlignH::Right => Alignment::Right,
                    AlignH::Justify => Alignment::Justify,
                },
                AlignmentOptions::default(),
            );
            let first_line = out.lines.len();
            let first_caret = out.carets.len();
            let (first_run, first_deco) = (out.runs.len(), out.decorations.len());
            collect_lines(&layout, &text, &styles, indent_x, y, &mut out);
            collect_carets(&layout, &map, indent_x, y, &mut out.carets);
            if let (Some(m), Some(line)) = (marker, out.lines.get(first_line).copied()) {
                let x = level as f64 * LIST_INDENT_EM * first.size;
                self.place_marker(&m, first, x, line.baseline, &mut out);
            }
            if req.width.is_none() {
                let w = out.lines[first_line..]
                    .iter()
                    .map(|l| l.rect.x1)
                    .fold(0.0, f64::max);
                spans.push(ParaSpan {
                    lines: first_line..out.lines.len(),
                    carets: first_caret..out.carets.len(),
                    runs: first_run..out.runs.len(),
                    decos: first_deco..out.decorations.len(),
                    width: w,
                });
            }
            y += layout.height() as f64;
        }
        // `parley` выравнивает абзац внутри его собственной ширины, а у текста
        // «авто» по ширине рамка — самая широкая строка: сдвиг вручную.
        let k = match props.align_h {
            AlignH::Center => 0.5,
            AlignH::Right => 1.0,
            AlignH::Left | AlignH::Justify => 0.0,
        };
        if k > 0.0 {
            let widest = spans.iter().map(|s| s.width).fold(0.0, f64::max);
            for ParaSpan {
                lines,
                carets,
                runs,
                decos,
                width,
            } in spans
            {
                let dx = (widest - width) * k;
                for l in &mut out.lines[lines] {
                    l.rect = l.rect + kurbo::Vec2::new(dx, 0.0);
                }
                for c in &mut out.carets[carets] {
                    for s in &mut c.stops {
                        s.1 += dx;
                    }
                }
                for r in &mut out.runs[runs] {
                    for g in &mut r.glyphs {
                        g.x += dx as f32;
                    }
                }
                for d in &mut out.decorations[decos] {
                    d.rect = d.rect + kurbo::Vec2::new(dx, 0.0);
                }
            }
        }
        out.height = y;
        out.width = out.lines.iter().map(|l| l.rect.x1).fold(0.0, f64::max);
        self.truncate(req, &mut out);
        // Выравнивание по вертикали внутри рамки фиксированной высоты.
        if let Some(h) = req.height {
            let dy = match props.align_v {
                AlignV::Top => 0.0,
                AlignV::Middle => (h - out.height) / 2.0,
                AlignV::Bottom => h - out.height,
            };
            if dy != 0.0 {
                shift(&mut out, dy);
            }
        }
        out.missing.sort();
        out.missing.dedup();
        out
    }

    fn span_style(
        &mut self,
        a: &TextAttrs,
        req: &LayoutRequest,
        default_color: Color,
        out: &mut TextLayout,
    ) -> SpanStyle {
        let d = &req.text.props.style;
        let font = a.font_name(d);
        let resolved = self.resolve(&font);
        match resolved.status {
            FontStatus::Loading | FontStatus::Pending => out.loading = true,
            _ => {}
        }
        if resolved.substituted {
            out.missing.push(font);
        }
        SpanStyle {
            resolved,
            size: finite_or(a.size.unwrap_or(d.size), 12.0).clamp(0.1, 10_000.0),
            line_height: a.line_height.unwrap_or(d.line_height),
            letter_spacing: a.letter_spacing.unwrap_or(d.letter_spacing),
            decoration: a.decoration.unwrap_or(d.decoration),
            case: a.case.unwrap_or(d.case),
            color: match &a.fill {
                Some(f) if f.kind == FillKind::Solid => text_color(std::slice::from_ref(f)),
                _ => default_color,
            },
            paint: a.fill.clone(),
        }
    }

    /// Маркер списка: отдельная однострочная раскладка стилем первого участка.
    fn place_marker(
        &mut self,
        marker: &str,
        style: &SpanStyle,
        x: f64,
        baseline: f64,
        out: &mut TextLayout,
    ) {
        let mut builder = self.lcx.ranged_builder(&mut self.fcx, marker, 1.0, false);
        let plain = SpanStyle {
            decoration: Decoration::None,
            case: TextCase::None,
            ..style.clone()
        };
        push_style(&mut builder, &plain, 0, 0..marker.len());
        let mut layout: Layout<u32> = builder.build(marker);
        layout.break_all_lines(None);
        let mut tmp = TextLayout::default();
        collect_lines(
            &layout,
            marker,
            std::slice::from_ref(&plain),
            0.0,
            0.0,
            &mut tmp,
        );
        let Some(line) = tmp.lines.first() else {
            return;
        };
        let (dx, dy) = (x as f32, (baseline - line.baseline) as f32);
        for mut run in tmp.runs {
            for g in &mut run.glyphs {
                g.x += dx;
                g.y += dy;
            }
            out.runs.push(run);
        }
    }

    /// Обрезка (спека 3, `truncate`): у фиксированной рамки — строки, которые
    /// помещаются по высоте, и не больше `max_lines`; у последней видимой при
    /// продолжении — многоточие. Без `truncate` строки за рамкой остаются
    /// (их обрежет рамка при рисовании — как в Figma, текст виден за краем).
    fn truncate(&mut self, req: &LayoutRequest, out: &mut TextLayout) {
        let props = &req.text.props;
        if !props.truncate || out.lines.is_empty() {
            return;
        }
        let mut keep = out.lines.len();
        if let Some(h) = req.height {
            keep = out
                .lines
                .iter()
                .take_while(|l| l.rect.y1 <= h + 0.01)
                .count()
                .max(1);
        }
        if let Some(n) = props.max_lines {
            keep = keep.min(n as usize).max(1);
        }
        if keep >= out.lines.len() {
            return;
        }
        out.truncated = true;
        let last = out.lines[keep - 1];
        let cut_y = last.rect.y1;
        out.lines.truncate(keep);
        for run in &mut out.runs {
            run.glyphs.retain(|g| (g.y as f64) <= cut_y);
        }
        out.decorations.retain(|d| d.rect.y0 < cut_y);
        // Многоточие стилем последнего глифа строки — справа, в пределах рамки.
        let avail = req.width.unwrap_or(last.rect.x1);
        let on_line = |g: &PlacedGlyph| (g.y as f64 - last.baseline).abs() < 0.5;
        let Some(style_run) = out
            .runs
            .iter()
            .rev()
            .find(|r| r.glyphs.iter().any(on_line))
            .cloned()
        else {
            return;
        };
        let ellipsis = self.shape_ellipsis(&style_run);
        let Some(e_width) = ellipsis.as_ref().map(|(_, w)| *w) else {
            return;
        };
        // Глифы строки, не помещающиеся вместе с многоточием, снимаются.
        let limit = (avail - e_width) as f32;
        let mut end_x = last.rect.x0 as f32;
        for run in &mut out.runs {
            run.glyphs
                .retain(|g| !on_line(g) || g.x + g.advance <= limit);
        }
        // Пробелы в конце строки снимаются: «слово…», а не «слово …».
        loop {
            let tail = out
                .runs
                .iter()
                .enumerate()
                .flat_map(|(ri, r)| {
                    r.glyphs
                        .iter()
                        .enumerate()
                        .filter(|(_, g)| on_line(g))
                        .map(move |(gi, g)| (ri, gi, g.x))
                })
                .max_by(|a, b| a.2.total_cmp(&b.2));
            let Some((ri, gi, _)) = tail else { break };
            let run = &out.runs[ri];
            if !is_space(&run.font, run.glyphs[gi].id) {
                break;
            }
            out.runs[ri].glyphs.remove(gi);
        }
        for run in &out.runs {
            for g in run.glyphs.iter().filter(|g| on_line(g)) {
                end_x = end_x.max(g.x + g.advance);
            }
        }
        out.decorations.retain(|d| {
            (d.rect.y0 - last.baseline).abs() > last.rect.height() || d.rect.x0 < limit as f64
        });
        for d in &mut out.decorations {
            if d.rect.y0 >= last.rect.y0 && d.rect.y1 <= last.rect.y1 + 1.0 {
                d.rect.x1 = d.rect.x1.min(end_x as f64);
            }
        }
        if let Some((mut run, w)) = ellipsis {
            for g in &mut run.glyphs {
                g.x += end_x;
                g.y = last.baseline as f32;
            }
            out.runs.push(run);
            out.lines[keep - 1].rect.x1 = end_x as f64 + w;
        }
        out.height = cut_y;
        out.width = out.lines.iter().map(|l| l.rect.x1).fold(0.0, f64::max);
    }

    /// «…» тем же шрифтом и кеглем, что глифы `like`: глифы от нуля и ширина.
    fn shape_ellipsis(&mut self, like: &GlyphRun) -> Option<(GlyphRun, f64)> {
        let style = SpanStyle {
            resolved: Resolved {
                status: FontStatus::Ready,
                family: like.face.family.clone(),
                weight: like.face.weight,
                italic: like.face.italic,
                stretch: like.face.stretch,
                substituted: false,
            },
            size: like.size as f64,
            line_height: LineHeight::Auto,
            letter_spacing: LetterSpacing::Px(0.0),
            decoration: Decoration::None,
            case: TextCase::None,
            color: like.color,
            paint: like.paint.clone(),
        };
        let text = "…";
        let mut builder = self.lcx.ranged_builder(&mut self.fcx, text, 1.0, false);
        push_style(&mut builder, &style, 0, 0..text.len());
        let mut layout: Layout<u32> = builder.build(text);
        layout.break_all_lines(None);
        let mut tmp = TextLayout::default();
        collect_lines(
            &layout,
            text,
            std::slice::from_ref(&style),
            0.0,
            0.0,
            &mut tmp,
        );
        let width = layout.width() as f64;
        let mut run = tmp.runs.into_iter().next()?;
        run.color = like.color;
        for g in &mut run.glyphs {
            g.y = 0.0;
        }
        Some((run, width))
    }
}

/// Глиф пробела в шрифте прогона?
fn is_space(font: &parley::FontData, glyph: u32) -> bool {
    skrifa::FontRef::from_index(font.data.as_ref(), font.index)
        .ok()
        .and_then(|f| skrifa::MetadataProvider::charmap(&f).map(' '))
        .is_some_and(|g| g.to_u32() == glyph)
}

/// Свойства `parley` стиля участка.
fn properties(s: &SpanStyle, brush: u32) -> Vec<StyleProperty<'_, u32>> {
    let mut out = vec![
        StyleProperty::FontFamily(FontFamily::named(&s.resolved.family)),
        StyleProperty::FontWeight(FontWeight::new(s.resolved.weight)),
        StyleProperty::FontStyle(if s.resolved.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        }),
        StyleProperty::FontWidth(FontWidth::from_ratio(s.resolved.stretch)),
        StyleProperty::FontSize(s.size as f32),
        StyleProperty::LineHeight(line_height(s.line_height)),
        StyleProperty::LetterSpacing(letter_spacing(s.letter_spacing, s.size)),
        StyleProperty::Brush(brush),
        // Слово длиннее рамки переносится по буквам, а не вылезает за неё (как в Figma).
        StyleProperty::OverflowWrap(OverflowWrap::BreakWord),
        StyleProperty::Underline(s.decoration == Decoration::Underline),
        StyleProperty::Strikethrough(s.decoration == Decoration::Strike),
    ];
    if s.case == TextCase::SmallCaps {
        out.push(StyleProperty::FontFeatures("\"smcp\" on".into()));
    }
    out
}

/// Стиль участка на диапазон. Корневой стиль абзаца — стиль первого участка:
/// иначе строку подпирал бы корневой стиль `parley` по умолчанию (16 px) и
/// мелкий текст получал бы лишний интерлиньяж.
fn push_style(
    builder: &mut parley::RangedBuilder<'_, u32>,
    s: &SpanStyle,
    brush: u32,
    range: Range<usize>,
) {
    if range.start == 0 {
        for prop in properties(s, brush) {
            builder.push_default(prop);
        }
    }
    for prop in properties(s, brush) {
        builder.push(prop, range.clone());
    }
}

/// Строки и глифы раскладки абзаца со сдвигом `(dx, dy)`.
fn collect_lines(
    layout: &Layout<u32>,
    text: &str,
    styles: &[SpanStyle],
    dx: f64,
    dy: f64,
    out: &mut TextLayout,
) {
    for line in layout.lines() {
        let m = line.metrics();
        let mut line_color = None;
        // Кластеры текущего шейпинга: его текст, глифы по порядку и сколько взято.
        let mut shaped: Option<(Range<usize>, Vec<Range<usize>>, usize)> = None;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let style = glyph_run.style();
            let Some(span) = styles.get(style.brush as usize) else {
                continue;
            };
            let color = span.color;
            line_color.get_or_insert(color);
            // Текст прогона и кластер каждого глифа. Прогоны одного шейпинга
            // (`Run`) идут на строке подряд в визуальном порядке, как и его
            // кластеры: прогон берёт следующие свои глифы из общего списка.
            let positioned: Vec<_> = glyph_run.positioned_glyphs().collect();
            let key = run.text_range();
            if shaped.as_ref().is_none_or(|(k, _, _)| *k != key) {
                let mut list: Vec<Range<usize>> = Vec::new();
                for c in run.visual_clusters() {
                    let r = c.text_range();
                    list.extend(std::iter::repeat_n(r, c.glyphs().count()));
                }
                shaped = Some((key, list, 0));
            }
            let (_, list, at) = shaped.as_mut().expect("задан выше");
            let mine: Vec<Range<usize>> = match list.get(*at..*at + positioned.len()) {
                Some(slice) => slice.to_vec(),
                // Так быть не должно: каждому глифу — весь текст шейпинга.
                None => vec![run.text_range(); positioned.len()],
            };
            *at += positioned.len();
            let lo = mine.iter().map(|r| r.start).min().unwrap_or(0);
            let hi = mine.iter().map(|r| r.end).max().unwrap_or(lo);
            let run_text = text.get(lo..hi).unwrap_or("").to_string();
            let clusters = mine
                .iter()
                .map(|r| [(r.start - lo) as u32, (r.end - lo) as u32]);
            let glyphs: Vec<PlacedGlyph> = positioned
                .into_iter()
                .zip(clusters)
                .map(|(g, cluster)| PlacedGlyph {
                    id: g.id,
                    x: g.x + dx as f32,
                    y: g.y + dy as f32,
                    advance: g.advance,
                    cluster,
                })
                .collect();
            let fm = run.font_metrics();
            let x0 = glyph_run.offset() as f64 + dx;
            let x1 = x0 + glyph_run.advance() as f64;
            let base = glyph_run.baseline() as f64 + dy;
            if style.underline.is_some() {
                let size = fm.underline_size.max(0.5) as f64;
                let top = base - fm.underline_offset as f64;
                out.decorations.push(DecorationBox {
                    rect: Rect::new(x0, top, x1, top + size),
                    color,
                    paint: span.paint.clone(),
                });
            }
            if style.strikethrough.is_some() {
                let size = fm.strikethrough_size.max(0.5) as f64;
                let top = base - fm.strikethrough_offset as f64;
                out.decorations.push(DecorationBox {
                    rect: Rect::new(x0, top, x1, top + size),
                    color,
                    paint: span.paint.clone(),
                });
            }
            if glyphs.is_empty() {
                continue;
            }
            let synthesis = run.synthesis();
            out.runs.push(GlyphRun {
                font: run.font().clone(),
                face: Face {
                    family: span.resolved.family.clone(),
                    weight: span.resolved.weight,
                    italic: span.resolved.italic,
                    stretch: span.resolved.stretch,
                },
                size: run.font_size(),
                coords: run
                    .normalized_coords()
                    .iter()
                    .map(|c| c.to_bits())
                    .collect(),
                color,
                paint: span.paint.clone(),
                skew: synthesis.skew(),
                letter_spacing: letter_spacing(span.letter_spacing, span.size),
                text: run_text,
                glyphs,
            });
        }
        let x0 = m.offset as f64 + dx;
        out.lines.push(LineBox {
            rect: Rect::new(
                x0,
                m.block_min_coord as f64 + dy,
                x0 + m.advance as f64,
                m.block_max_coord as f64 + dy,
            ),
            baseline: m.baseline as f64 + dy,
            color: line_color.unwrap_or(Color::BLACK),
        });
    }
}

/// Строки абзаца для правки: символы, вертикаль и позиции курсора по кластерам
/// (`parley`), со сдвигом `(dx, dy)`.
fn collect_carets(layout: &Layout<u32>, map: &CharMap, dx: f64, dy: f64, out: &mut Vec<CaretLine>) {
    let count = layout.lines().count();
    for (i, line) in layout.lines().enumerate() {
        let m = line.metrics();
        let range = line.text_range();
        let (start, end) = (map.char_at(range.start), map.char_at(range.end));
        let mut stops: Vec<(usize, f64)> = Vec::new();
        let mut tail = None;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(gr) = item else {
                continue;
            };
            let mut x = gr.offset() as f64 + dx;
            for cl in gr.run().visual_clusters() {
                let r = cl.text_range();
                stops.push((map.char_at(r.start), x));
                x += cl.advance() as f64;
                tail = Some((map.char_at(r.end), x));
            }
        }
        let line_x = m.offset as f64 + dx;
        if stops.is_empty() {
            stops.push((start, line_x));
        }
        stops.push(match tail {
            Some((c, x)) => (c.max(end), x),
            None => (end, line_x),
        });
        // Кластер из нескольких символов (и пустой абзац) — одна позиция.
        stops.sort_by_key(|s| s.0);
        stops.dedup_by_key(|s| s.0);
        out.push(CaretLine {
            start,
            end,
            top: m.block_min_coord as f64 + dy,
            bottom: m.block_max_coord as f64 + dy,
            baseline: m.baseline as f64 + dy,
            stops,
            hard: i + 1 == count,
        });
    }
}

fn shift(out: &mut TextLayout, dy: f64) {
    for c in &mut out.carets {
        c.top += dy;
        c.bottom += dy;
        c.baseline += dy;
    }
    for l in &mut out.lines {
        l.rect = l.rect + kurbo::Vec2::new(0.0, dy);
        l.baseline += dy;
    }
    for r in &mut out.runs {
        for g in &mut r.glyphs {
            g.y += dy as f32;
        }
    }
    for d in &mut out.decorations {
        d.rect = d.rect + kurbo::Vec2::new(0.0, dy);
    }
}
