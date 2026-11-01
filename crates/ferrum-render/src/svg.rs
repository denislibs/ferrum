//! Экспорт в SVG (спека подсистемы 3, раздел 11): свой писатель по сцене, как
//! «Export → SVG» в Figma. Узлы — элементы `<g>`, `<rect>`, `<ellipse>`,
//! `<line>`, `<path>`; фрейм — `<g>` с обрезкой `<clipPath>` по своей форме,
//! заливка под детьми и обводка над ними; векторы и булевы группы — пути их
//! областей; текст — контуры глифов («Текст кривыми») или элементы `<text>`. Заливки — цвет (`fill` и `fill-opacity`) и
//! изображение (`<pattern>` с `<image>` — `data:` PNG, байты один раз на файл).
//! Обводки — настоящие `stroke` с соединением и пределом угла; положение «внутри»
//! — двойная толщина с `<clipPath>` по форме, «снаружи» — двойная толщина с
//! `<mask>` (как у Figma); окончания стрелок, круги и ромбы — залитые фигуры.
//! Прозрачность узла — `opacity` у `<g>`.
//!
//! Координаты — мира со сдвигом к левому верхнему углу области экспорта (та же
//! область, что у PNG). Матрица узла без поворота и отражения запекается в
//! координаты (`x`, `y`, `d`), иначе — атрибут `transform` (у узлов нет масштаба,
//! поэтому толщина обводки не искажается). Обход — без рекурсии: глубина дерева
//! из файла не ограничена.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use ferrum_doc::{
    Color, Fill, FillKind, FillRule, ImageStore, NodeKind, Props, StrokeAlign, StrokeJoin,
};
use ferrum_scene::shape::{Geometry, PATH_TOLERANCE};
use ferrum_scene::vector::{self, PathShape};
use ferrum_scene::{Scene, Slot};
use ferrum_text::{LayoutRequest, SharedFonts};
use kurbo::{Affine, BezPath, PathEl, Rect, Shape as _};

use crate::draw::place_image;
use crate::export::{export_items, ExportBudget};
use crate::text_node::glyphs_path;
use crate::ExportError;

/// Настройки экспорта SVG, как в разделе «Export» Figma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SvgOptions {
    /// «Текст кривыми»: текст — контурами глифов (по умолчанию, как в Figma), без
    /// него — элементы `<text>` со шрифтом (семейство, кегль, вес, курсив,
    /// трекинг): текст можно править, но вид зависит от шрифтов у зрителя.
    pub outline_text: bool,
    /// «Атрибут id»: у элементов — `id` из имён слоёв (по умолчанию нет).
    pub ids: bool,
}

impl Default for SvgOptions {
    fn default() -> SvgOptions {
        SvgOptions {
            outline_text: true,
            ids: false,
        }
    }
}

/// Байты изображений документа по хэшу (PNG) — для `data:` в SVG и картинок PDF.
pub trait ImageBytes {
    fn png(&self, hash: &str) -> Option<&[u8]>;
}

impl ImageBytes for ImageStore {
    fn png(&self, hash: &str) -> Option<&[u8]> {
        self.get(hash)
    }
}

impl ImageBytes for HashMap<String, Vec<u8>> {
    fn png(&self, hash: &str) -> Option<&[u8]> {
        self.get(hash).map(Vec::as_slice)
    }
}

/// Без изображений: заливка картинкой пропускается.
pub struct NoImageBytes;

impl ImageBytes for NoImageBytes {
    fn png(&self, _: &str) -> Option<&[u8]> {
        None
    }
}

/// Размер PNG по заголовку (`IHDR`); не PNG — `None`.
pub(crate) fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let side = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4 байта"));
    let (w, h) = (side(16), side(20));
    (w > 0 && h > 0).then_some((w, h))
}

/// Число в SVG: до тысячных, без лишних нулей, без `-0`.
pub(crate) fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v * 1000.0).round() / 1000.0;
    // `v * 1000` переполняется у очень больших чисел: в SVG не бывает `inf`.
    if r == 0.0 || !r.is_finite() {
        return "0".into();
    }
    let mut s = format!("{r:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

/// Данные пути `d` с матрицей `g`.
pub(crate) fn path_data(path: &BezPath, g: Affine) -> String {
    let mut d = String::with_capacity(path.elements().len() * 16);
    for el in path.elements() {
        match g * *el {
            PathEl::MoveTo(p) => {
                let _ = write!(d, "M{} {}", num(p.x), num(p.y));
            }
            PathEl::LineTo(p) => {
                let _ = write!(d, "L{} {}", num(p.x), num(p.y));
            }
            PathEl::QuadTo(a, p) => {
                let _ = write!(d, "Q{} {} {} {}", num(a.x), num(a.y), num(p.x), num(p.y));
            }
            PathEl::CurveTo(a, b, p) => {
                let _ = write!(
                    d,
                    "C{} {} {} {} {} {}",
                    num(a.x),
                    num(a.y),
                    num(b.x),
                    num(b.y),
                    num(p.x),
                    num(p.y)
                );
            }
            PathEl::ClosePath => d.push('Z'),
        }
    }
    d
}

/// Допустимое первое символ имени XML 1.0 (5-е издание) без `:`.
fn xml_name_start(c: char) -> bool {
    matches!(c,
        'A'..='Z' | 'a'..='z' | '_'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
}

/// Экранирование текста и значений атрибутов XML.
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Управляющие символы в XML 1.0 недопустимы (кроме табуляции и переводов строки).
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// Base64 (RFC 4648) без переносов — для `data:` URL.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Цвет `#rrggbb` и непрозрачность (альфа цвета × `alpha`).
fn color_attr(attr: &str, c: Color, alpha: f64) -> String {
    let a = (c.a as f64 / 255.0 * alpha).clamp(0.0, 1.0);
    let mut s = format!(" {attr}=\"#{:02X}{:02X}{:02X}\"", c.r, c.g, c.b);
    if a < 1.0 {
        let _ = write!(s, " {attr}-opacity=\"{}\"", num(a));
    }
    s
}

/// Как ставить матрицу узла: сдвиг запекается в координаты (`g`), остальное —
/// атрибутом `transform` (`attr`), координаты тогда локальные.
#[derive(Debug, Clone)]
pub(crate) struct Place {
    pub g: Affine,
    pub attr: String,
}

impl Place {
    pub fn of(m: Affine) -> Place {
        let c = m.as_coeffs();
        if c[0] == 1.0 && c[1] == 0.0 && c[2] == 0.0 && c[3] == 1.0 {
            Place {
                g: m,
                attr: String::new(),
            }
        } else {
            Place {
                g: Affine::IDENTITY,
                attr: format!(
                    " transform=\"matrix({} {} {} {} {} {})\"",
                    num(c[0]),
                    num(c[1]),
                    num(c[2]),
                    num(c[3]),
                    num(c[4]),
                    num(c[5])
                ),
            }
        }
    }
}

/// Форма элемента в координатах узла.
#[derive(Debug, Clone)]
enum Form {
    Rect {
        rect: Rect,
        radius: f64,
    },
    Ellipse(Rect),
    Line(f64),
    Path(BezPath, FillRule),
    /// Текст элементом `<text>`: атрибуты шрифта, строки `(x, y, текст)` на базовой
    /// линии и рамка (для маски внешней обводки).
    Text {
        font: String,
        spans: Vec<(f64, f64, String)>,
        bbox: Rect,
    },
}

impl Form {
    /// Элемент целиком с атрибутами `attrs` (краска, обрезка).
    fn el(&self, p: &Place, attrs: &str) -> String {
        let Form::Text { font, spans, .. } = self else {
            return format!("{}{attrs}/>", self.open(p));
        };
        let mut s = format!("<text{font}{}{attrs}>", p.attr);
        for (x, y, text) in spans {
            let at = p.g * kurbo::Point::new(*x, *y);
            let _ = write!(
                s,
                "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
                num(at.x),
                num(at.y),
                escape(text)
            );
        }
        s.push_str("</text>");
        s
    }

    /// Открывающий тег без закрытия (`<rect … `): дальше — атрибуты краски и `/>`.
    fn open(&self, p: &Place) -> String {
        let g = p.g;
        match self {
            Form::Rect { rect, radius } => {
                let r = g.transform_rect_bbox(*rect);
                let mut s = format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                    num(r.x0),
                    num(r.y0),
                    num(r.width()),
                    num(r.height())
                );
                if *radius > 0.0 {
                    let _ = write!(s, " rx=\"{}\"", num(*radius));
                }
                s.push_str(&p.attr);
                s
            }
            Form::Ellipse(rect) => {
                let r = g.transform_rect_bbox(*rect);
                let c = r.center();
                format!(
                    "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{}",
                    num(c.x),
                    num(c.y),
                    num(r.width() / 2.0),
                    num(r.height() / 2.0),
                    p.attr
                )
            }
            Form::Line(len) => {
                let a = g * kurbo::Point::ZERO;
                let b = g * kurbo::Point::new(*len, 0.0);
                format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{}",
                    num(a.x),
                    num(a.y),
                    num(b.x),
                    num(b.y),
                    p.attr
                )
            }
            Form::Path(path, rule) => {
                let mut s = format!("<path d=\"{}\"", path_data(path, g));
                if *rule == FillRule::EvenOdd {
                    s.push_str(" fill-rule=\"evenodd\" clip-rule=\"evenodd\"");
                }
                s.push_str(&p.attr);
                s
            }
            Form::Text { .. } => unreachable!("текст пишется целиком (`el`)"),
        }
    }

    /// Контур формы (для внешности маски и границ).
    fn path(&self) -> BezPath {
        match self {
            Form::Rect { rect, radius } => {
                let mut out = BezPath::new();
                Geometry::Rect {
                    rect: *rect,
                    radius: *radius,
                }
                .fill_path_into(&mut out);
                out
            }
            Form::Ellipse(r) => {
                let mut out = BezPath::new();
                Geometry::Ellipse(*r).fill_path_into(&mut out);
                out
            }
            Form::Line(len) => {
                let mut out = BezPath::new();
                out.move_to((0.0, 0.0));
                out.line_to((*len, 0.0));
                out
            }
            Form::Path(p, _) => p.clone(),
            Form::Text { bbox, .. } => bbox.to_path(PATH_TOLERANCE),
        }
    }
}

/// Что положить в стек обхода.
enum Visit {
    Enter(Slot),
    /// Закрыть теги (`</g>…`).
    Close(&'static str),
    /// Обводки фрейма — над детьми, вне обрезки.
    FrameStrokes(Slot),
    /// Закрыть `<g opacity>` / `<g id>` фрейма после его обводок.
    CloseOwn(bool),
}

/// Писатель одного файла.
pub(crate) struct Writer<'a> {
    scene: &'a Scene,
    images: &'a dyn ImageBytes,
    fonts: &'a SharedFonts,
    opts: SvgOptions,
    budget: &'a ExportBudget<'a>,
    root: Affine,
    body: String,
    defs: String,
    next: u32,
    ids: HashSet<String>,
    /// Изображения, уже положенные в `<defs>`: хэш → id и размер.
    image_defs: HashMap<String, (String, u32, u32)>,
    xlink: bool,
    visited: usize,
}

impl<'a> Writer<'a> {
    fn new(
        scene: &'a Scene,
        images: &'a dyn ImageBytes,
        fonts: &'a SharedFonts,
        opts: SvgOptions,
        budget: &'a ExportBudget<'a>,
        root: Affine,
    ) -> Writer<'a> {
        Writer {
            scene,
            images,
            fonts,
            opts,
            budget,
            root,
            body: String::new(),
            defs: String::new(),
            next: 0,
            ids: HashSet::new(),
            image_defs: HashMap::new(),
            xlink: false,
            visited: 0,
        }
    }

    /// Служебный `id` (обрезка, маска, узор, картинка): уникален и среди имён
    /// слоёв — в любом порядке создания (`layer_id` сверяется с тем же набором).
    fn fresh(&mut self, prefix: &str) -> String {
        loop {
            let id = format!("{prefix}{}", self.next);
            self.next += 1;
            if self.ids.insert(id.clone()) {
                return id;
            }
        }
    }

    /// `id` из имени слоя: допустимые в имени XML буквы, цифры, `-` и `_`; остальное
    /// (пробелы, знаки, `ª`, `²`) — `_`; уникально.
    fn layer_id(&mut self, name: &str) -> String {
        let mut base: String = name
            .chars()
            .map(|c| {
                if xml_name_start(c) || c.is_ascii_digit() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if !base.starts_with(xml_name_start) {
            base.insert(0, '_');
        }
        let mut id = base.clone();
        let mut n = 2;
        while !self.ids.insert(id.clone()) {
            id = format!("{base}_{n}");
            n += 1;
        }
        id
    }

    fn id_attr(&mut self, props: &Props) -> String {
        if self.opts.ids {
            format!(" id=\"{}\"", escape(&self.layer_id(&props.name)))
        } else {
            String::new()
        }
    }

    /// Размер файла с запасом `extra` на то, что собираются дописать.
    fn check_size(&self, extra: usize) -> Result<(), ExportError> {
        self.budget
            .check_bytes(self.body.len() + self.defs.len() + extra)
    }

    fn check(&self) -> Result<(), ExportError> {
        self.budget.check(self.body.len() + self.defs.len())
    }

    /// Узел и его поддерево.
    fn write_tree(&mut self, top: Slot) -> Result<(), ExportError> {
        let mut stack = vec![Visit::Enter(top)];
        while let Some(v) = stack.pop() {
            let slot = match v {
                Visit::Enter(s) => s,
                Visit::Close(tags) => {
                    self.body.push_str(tags);
                    continue;
                }
                Visit::FrameStrokes(s) => {
                    self.strokes(s)?;
                    continue;
                }
                Visit::CloseOwn(open) => {
                    if open {
                        self.body.push_str("</g>");
                    }
                    continue;
                }
            };
            self.visited += 1;
            // Размер — после каждого узла (O(1)), часы — раз в 256.
            self.check_size(0)?;
            if self.visited.is_multiple_of(256) {
                self.check()?;
            }
            let n = self.scene.at(slot);
            // Скрытый узел ничего не рисует: его прозрачность не проверяется.
            if n.subtree_bounds.is_none() {
                continue;
            }
            if n.props.opacity.is_nan() {
                return Err(ExportError::Write("прозрачность узла не число".to_string()));
            }
            if n.props.opacity <= 0.0 {
                continue;
            }
            let id = self.id_attr(&n.props);
            let opacity = n.props.opacity.clamp(0.0, 1.0);
            let wrap = opacity < 1.0 || !id.is_empty();
            if wrap {
                self.body.push_str("<g");
                self.body.push_str(&id);
                if opacity < 1.0 {
                    let _ = write!(self.body, " opacity=\"{}\"", num(opacity));
                }
                self.body.push('>');
            }
            let kind = n.props.kind;
            // Дети булевой группы не рисуются: её форма — результат операции.
            let has_children = !n.children.is_empty() && kind != NodeKind::Boolean;
            match kind {
                NodeKind::Group => {
                    stack.push(Visit::CloseOwn(wrap));
                    stack.extend(n.children.iter().rev().map(|&c| Visit::Enter(c)));
                }
                NodeKind::Frame if has_children => {
                    self.fills(slot)?;
                    stack.push(Visit::CloseOwn(wrap));
                    stack.push(Visit::FrameStrokes(slot));
                    if n.props.clip_content {
                        self.check_size(0)?;
                        let clip = self.fresh("clip");
                        let place = self.place(slot);
                        let form = form_of(n.geometry()).expect("у фрейма есть форма");
                        let _ = write!(
                            self.defs,
                            "<clipPath id=\"{clip}\">{}</clipPath>",
                            form.el(&place, "")
                        );
                        let _ = write!(self.body, "<g clip-path=\"url(#{clip})\">");
                        stack.push(Visit::Close("</g>"));
                    }
                    stack.extend(n.children.iter().rev().map(|&c| Visit::Enter(c)));
                }
                _ => {
                    self.fills(slot)?;
                    self.strokes(slot)?;
                    if wrap {
                        self.body.push_str("</g>");
                    }
                }
            }
        }
        Ok(())
    }

    fn place(&self, slot: Slot) -> Place {
        Place::of(self.root * self.scene.at(slot).world)
    }

    /// Изображение в `<defs>` (один раз на хэш): id и размер. Нет байтов или не
    /// PNG — `None`.
    fn image_def(&mut self, hash: &str) -> Result<Option<(String, u32, u32)>, ExportError> {
        if let Some(d) = self.image_defs.get(hash) {
            return Ok(Some(d.clone()));
        }
        let Some(bytes) = self.images.png(hash) else {
            return Ok(None);
        };
        let Some((w, h)) = png_size(bytes) else {
            return Ok(None);
        };
        // Размер base64 известен заранее: сверяемся с пределом до кодирования.
        self.check_size(bytes.len().div_ceil(3) * 4 + 160)?;
        let id = self.fresh("image");
        self.xlink = true;
        let _ = write!(
            self.defs,
            "<image id=\"{id}\" width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\" xlink:href=\"data:image/png;base64,{}\"/>",
            base64(bytes)
        );
        self.image_defs.insert(hash.to_string(), (id.clone(), w, h));
        Ok(Some((id, w, h)))
    }

    /// Атрибут заливки краской `f` для формы с рамкой `area` (локальные
    /// координаты узла). `None` — нечем заливать (скрыта, прозрачна, нет байтов).
    fn paint(
        &mut self,
        f: &Fill,
        area: Rect,
        place: &Place,
    ) -> Result<Option<String>, ExportError> {
        // Скрытая заливка ничего не рисует: её прозрачность не проверяется.
        if !f.visible {
            return Ok(None);
        }
        if f.opacity.is_nan() {
            return Err(ExportError::Write(
                "прозрачность заливки не число".to_string(),
            ));
        }
        if f.opacity <= 0.0 {
            return Ok(None);
        }
        match f.kind {
            FillKind::Solid => Ok(Some(color_attr("fill", f.color, f.opacity))),
            FillKind::Image => {
                if !(area.width() > 0.0 && area.height() > 0.0) {
                    return Ok(None);
                }
                let Some(hash) = f.image_hash.as_deref() else {
                    return Ok(None);
                };
                let Some((img, iw, ih)) = self.image_def(hash)? else {
                    return Ok(None);
                };
                self.check_size(512)?;
                let (t, _, _) = place_image(f.scale_mode, area, iw as f64, ih as f64);
                let pat = self.fresh("pattern");
                let g = place.g;
                // Плитка — узор размером с картинку; «Заполнить» и «Вписать» — одна
                // картинка на всю рамку (у «Вписать» поля прозрачны).
                let (tile, content) = match f.scale_mode {
                    ferrum_doc::ScaleMode::Tile => (
                        g.transform_rect_bbox(Rect::new(
                            area.x0,
                            area.y0,
                            area.x0 + iw as f64,
                            area.y0 + ih as f64,
                        )),
                        Affine::IDENTITY,
                    ),
                    _ => {
                        let r = g.transform_rect_bbox(area);
                        // Содержимое узора — от его левого верхнего угла.
                        (r, Affine::translate(-r.origin().to_vec2()) * g * t)
                    }
                };
                let c = content.as_coeffs();
                let _ = write!(
                    self.defs,
                    "<pattern id=\"{pat}\" patternUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"><use xlink:href=\"#{img}\" transform=\"matrix({} {} {} {} {} {})\"/></pattern>",
                    num(tile.x0),
                    num(tile.y0),
                    num(tile.width()),
                    num(tile.height()),
                    num(c[0]),
                    num(c[1]),
                    num(c[2]),
                    num(c[3]),
                    num(c[4]),
                    num(c[5]),
                );
                let mut s = format!(" fill=\"url(#{pat})\"");
                if f.opacity < 1.0 {
                    let _ = write!(s, " fill-opacity=\"{}\"", num(f.opacity));
                }
                Ok(Some(s))
            }
        }
    }

    /// Заливки узла снизу вверх.
    fn fills(&mut self, slot: Slot) -> Result<(), ExportError> {
        let n = self.scene.at(slot);
        let p = &n.props;
        let place = self.place(slot);
        let area = Rect::new(0.0, 0.0, p.w.max(0.0), p.h.max(0.0));
        match p.kind {
            NodeKind::Group | NodeKind::Line => {}
            NodeKind::Vector | NodeKind::Boolean => {
                let Some(ps) = n.path_shape().cloned() else {
                    return Ok(());
                };
                for r in &ps.regions {
                    let fills = r.fills.as_deref().unwrap_or(&p.fills);
                    for f in fills {
                        // Как у рендера: у булевой группы заливка картинкой не рисуется
                        // (у результата операции нет рамки картинки).
                        if p.kind == NodeKind::Boolean && f.kind == FillKind::Image {
                            continue;
                        }
                        self.check_size(r.path.elements().len() * 24)?;
                        if let Some(attr) = self.paint(f, area, &place)? {
                            let form = Form::Path(r.path.clone(), r.rule);
                            self.body.push_str(&form.el(&place, &attr));
                        }
                    }
                }
            }
            NodeKind::Text => self.text_fills(slot, &place)?,
            _ => {
                let Some(form) = form_of(n.geometry()) else {
                    return Ok(());
                };
                for f in &p.fills {
                    self.check_size(0)?;
                    if let Some(attr) = self.paint(f, area, &place)? {
                        self.body.push_str(&form.el(&place, &attr));
                    }
                }
            }
        }
        Ok(())
    }

    /// Текст контурами глифов («Текст кривыми») или элементами `<text>`: прогоны
    /// своей краской (участка или заливками узла), украшения — прямоугольниками.
    fn text_fills(&mut self, slot: Slot, place: &Place) -> Result<(), ExportError> {
        let n = self.scene.at(slot);
        let p = &n.props;
        let Some(t) = &p.text else { return Ok(()) };
        let fonts = self.fonts;
        let mut fonts = fonts.borrow_mut();
        let layout = fonts.layout(&LayoutRequest::for_node(t, &p.fills, p.w, p.h));
        if layout.loading {
            return Ok(());
        }
        let area = Rect::new(0.0, 0.0, p.w.max(0.0), p.h.max(0.0));
        let forms = text_forms(&mut fonts, &layout, self.opts.outline_text, area);
        for (run, form) in layout.runs.iter().zip(forms) {
            let Some(form) = form else { continue };
            let paints: Vec<&Fill> = match &run.paint {
                Some(f) => vec![f],
                None => p.fills.iter().collect(),
            };
            for f in paints {
                self.check_size(form_len(&form))?;
                if let Some(attr) = self.paint(f, area, place)? {
                    self.body.push_str(&form.el(place, &attr));
                }
            }
        }
        let visible = p.fills.iter().any(|f| f.visible && f.opacity > 0.0);
        for d in &layout.decorations {
            let attr = match &d.paint {
                Some(f) if f.visible && f.opacity > 0.0 => match f.kind {
                    FillKind::Solid => color_attr("fill", f.color, f.opacity),
                    FillKind::Image => color_attr("fill", d.color, f.opacity),
                },
                Some(_) => continue,
                None if !visible => continue,
                None => color_attr("fill", d.color, 1.0),
            };
            let form = Form::Rect {
                rect: d.rect,
                radius: 0.0,
            };
            self.check_size(0)?;
            self.body.push_str(&form.el(place, &attr));
        }
        Ok(())
    }

    /// Обводки узла по порядку списка.
    fn strokes(&mut self, slot: Slot) -> Result<(), ExportError> {
        let n = self.scene.at(slot);
        let p = &n.props;
        if p.kind == NodeKind::Group {
            return Ok(());
        }
        let strokes: Vec<_> = p
            .strokes
            .iter()
            .filter(|s| s.width > 0.0 && s.color.a > 0)
            .cloned()
            .collect();
        if strokes.is_empty() {
            return Ok(());
        }
        let place = self.place(slot);
        if p.kind == NodeKind::Text {
            let Some(t) = &p.text else { return Ok(()) };
            let area = Rect::new(0.0, 0.0, p.w.max(0.0), p.h.max(0.0));
            let forms: Vec<Form> = {
                let mut fonts = self.fonts.borrow_mut();
                let layout = fonts.layout(&LayoutRequest::for_node(t, &p.fills, p.w, p.h));
                if layout.loading {
                    return Ok(());
                }
                if self.opts.outline_text {
                    // Контуром — все глифы одним путём (как обводит рендер).
                    vec![Form::Path(
                        glyphs_path(&mut fonts, layout.runs.iter()),
                        FillRule::NonZero,
                    )]
                } else {
                    text_forms(&mut fonts, &layout, false, area)
                        .into_iter()
                        .flatten()
                        .collect()
                }
            };
            for s in &strokes {
                for form in &forms {
                    self.ring_stroke(form, s, StrokeJoin::Miter, 4.0, &place, true)?;
                }
            }
            return Ok(());
        }
        if let Some(ps) = n.path_shape().cloned() {
            for s in &strokes {
                self.path_stroke(&ps, s, p, &place)?;
            }
            return Ok(());
        }
        let Some(form) = form_of(n.geometry()) else {
            return Ok(());
        };
        for s in &strokes {
            self.ring_stroke(&form, s, StrokeJoin::Miter, 4.0, &place, true)?;
        }
        Ok(())
    }

    /// Ссылка на обрезку или маску положения обводки (` clip-path="…"` /
    /// ` mask="…"`), определение — в `<defs>`; по центру — пусто. Геометрия
    /// определения без `transform` элемента: `userSpaceOnUse` разбирается в
    /// пространстве элемента, которому ссылка и `transform` поставлены вместе.
    fn align_ref(
        &mut self,
        form: &Form,
        align: StrokeAlign,
        w: f64,
        miter: f64,
        place: &Place,
    ) -> Result<String, ExportError> {
        let local = Place {
            g: place.g,
            attr: String::new(),
        };
        match align {
            StrokeAlign::Center => Ok(String::new()),
            StrokeAlign::Inside => {
                self.check_size(form_len(form) + 256)?;
                let clip = self.fresh("clip");
                let _ = write!(
                    self.defs,
                    "<clipPath id=\"{clip}\">{}</clipPath>",
                    form.el(&local, "")
                );
                Ok(format!(" clip-path=\"url(#{clip})\""))
            }
            StrokeAlign::Outside => {
                self.check_size(form_len(form) + 512)?;
                let mask = self.fresh("mask");
                // Рамка маски — форма с запасом на толщину и острия стыков.
                let reach = w * miter.max(1.0) + 1.0;
                let b = place
                    .g
                    .transform_rect_bbox(form.path().bounding_box())
                    .inflate(reach, reach);
                let _ = write!(
                    self.defs,
                    "<mask id=\"{mask}\" maskUnits=\"userSpaceOnUse\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"><rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"white\"/>{}</mask>",
                    form.el(&local, " fill=\"black\""),
                    x = num(b.x0),
                    y = num(b.y0),
                    w = num(b.width()),
                    h = num(b.height()),
                );
                Ok(format!(" mask=\"url(#{mask})\""))
            }
        }
    }

    /// Обводка формы: по центру — как есть, внутри — двойной толщины с обрезкой
    /// формой, снаружи — двойной толщины с маской «всё, кроме формы». `bind` —
    /// ставить ли обрезку или маску самой обводке (иначе их ставит вызывающий на
    /// группу вокруг).
    fn ring_stroke(
        &mut self,
        form: &Form,
        s: &ferrum_doc::Stroke,
        join: StrokeJoin,
        miter: f64,
        place: &Place,
        bind: bool,
    ) -> Result<(), ExportError> {
        self.check_size(form_len(form) * 2 + 256)?;
        let open = matches!(form, Form::Line(_));
        let align = if open { StrokeAlign::Center } else { s.align };
        let w = if align == StrokeAlign::Center {
            s.width
        } else {
            2.0 * s.width
        };
        let mut attrs = color_attr("stroke", s.color, 1.0);
        let _ = write!(attrs, " stroke-width=\"{}\"", num(w));
        match join {
            StrokeJoin::Miter => {
                if (miter - 4.0).abs() > 1e-9 {
                    let _ = write!(attrs, " stroke-miterlimit=\"{}\"", num(miter));
                }
            }
            StrokeJoin::Round => attrs.push_str(" stroke-linejoin=\"round\""),
            StrokeJoin::Bevel => attrs.push_str(" stroke-linejoin=\"bevel\""),
        }
        if bind {
            attrs.push_str(&self.align_ref(form, align, w, miter, place)?);
        }
        self.body.push_str(&form.el(place, &attrs));
        Ok(())
    }

    /// Обводка по контуру (вектор, результат булевой группы, линия с окончаниями,
    /// прямоугольник с соединением не «острым»): путь с соединением узла и
    /// плоскими концами, фигуры окончаний и круги в точках ветвления — заливкой
    /// цветом обводки. Полупрозрачная обводка с фигурами — в `<g opacity>`, чтобы
    /// перекрытия не темнели (как слой у рендера).
    fn path_stroke(
        &mut self,
        ps: &PathShape,
        s: &ferrum_doc::Stroke,
        p: &Props,
        place: &Place,
    ) -> Result<(), ExportError> {
        let style = &p.stroke_style;
        let w = vector::effective_width(ps, s);
        let extras = !ps.ends.is_empty() || !ps.junctions.is_empty();
        let alpha = s.color.a as f64 / 255.0;
        let layered = extras && alpha < 1.0;
        let color = if layered {
            Color { a: 255, ..s.color }
        } else {
            s.color
        };
        if layered {
            let _ = write!(self.body, "<g opacity=\"{}\">", num(alpha));
        }
        let stroke = ferrum_doc::Stroke {
            color,
            width: s.width,
            align: if ps.closed {
                s.align
            } else {
                StrokeAlign::Center
            },
        };
        let form = Form::Path(ps.stroke.clone(), FillRule::NonZero);
        // Круги в точках ветвления замкнутого контура режутся тем же положением,
        // что и обводка (как у рендера): обводка и круги — в одной группе с обрезкой
        // или маской, а `transform` узла — на группе.
        let shared = ps.closed && stroke.align != StrokeAlign::Center && !ps.junctions.is_empty();
        let mut place = place.clone();
        if shared {
            let reference = self.align_ref(&form, stroke.align, w, style.miter_limit, &place)?;
            let _ = write!(self.body, "<g{}{reference}>", place.attr);
            place.attr.clear();
        }
        if !ps.stroke.elements().is_empty() {
            self.ring_stroke(
                &form,
                &stroke,
                style.join,
                style.miter_limit,
                &place,
                !shared,
            )?;
        }
        let place = &place;
        let mut fill = BezPath::new();
        for j in &ps.junctions {
            fill.extend(kurbo::Circle::new(*j, w / 2.0).path_elements(PATH_TOLERANCE));
        }
        let mut lines = BezPath::new();
        for e in &ps.ends {
            let (f, l) = vector::cap_shapes(e, w);
            if f.area() < 0.0 {
                fill.extend(f.reverse_subpaths().iter());
            } else {
                fill.extend(f.iter());
            }
            if let Some(l) = l {
                lines.extend(l.iter());
            }
        }
        self.check_size(fill.elements().len() * 24 + lines.elements().len() * 24)?;
        if !fill.elements().is_empty() {
            let form = Form::Path(fill, FillRule::NonZero);
            self.body
                .push_str(&form.el(place, &color_attr("fill", color, 1.0)));
        }
        if !lines.elements().is_empty() {
            let form = Form::Path(lines, FillRule::NonZero);
            let attrs = format!(
                "{} stroke-width=\"{}\"",
                color_attr("stroke", color, 1.0),
                num(w)
            );
            self.body.push_str(&form.el(place, &attrs));
        }
        if shared {
            self.body.push_str("</g>");
        }
        if layered {
            self.body.push_str("</g>");
        }
        Ok(())
    }

    /// Собирает файл на месте: заголовок вставляется перед телом, без второй
    /// полной копии (память не вдвое больше предела).
    fn finish(self, area: Rect) -> Result<String, ExportError> {
        let (w, h) = (area.width(), area.height());
        let mut head = String::with_capacity(256);
        let _ = write!(
            head,
            "<svg width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" fill=\"none\" xmlns=\"http://www.w3.org/2000/svg\"",
            w = num(w),
            h = num(h)
        );
        if self.xlink {
            head.push_str(" xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
        }
        head.push_str(">\n");
        let tail = if self.defs.is_empty() {
            "\n</svg>\n".len()
        } else {
            "\n<defs></defs>\n</svg>\n".len()
        };
        self.budget
            .check(head.len() + self.body.len() + self.defs.len() + tail)?;
        let mut out = self.body;
        out.reserve(head.len() + self.defs.len() + tail);
        out.insert_str(0, &head);
        if !self.defs.is_empty() {
            out.push_str("\n<defs>");
            out.push_str(&self.defs);
            out.push_str("</defs>");
        }
        out.push_str("\n</svg>\n");
        Ok(out)
    }
}

/// Имя семейства для `font-family`: буквы, цифры, пробелы и дефисы — как есть,
/// иначе в кавычках.
fn font_family(name: &str) -> String {
    let plain = !name.is_empty()
        && name.starts_with(|c: char| c.is_alphabetic())
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '-');
    if plain {
        escape(name)
    } else {
        format!("'{}'", escape(&name.replace(['\\', '\''], "")))
    }
}

/// Форма каждого прогона раскладки: контуром глифов (`outline`) или элементом
/// `<text>` (шрифт, кегль, вес, курсив, ширина и трекинг прогона, строка от
/// первого глифа на базовой линии). Пустой прогон — `None`.
fn text_forms(
    fonts: &mut ferrum_text::Fonts,
    layout: &ferrum_text::TextLayout,
    outline: bool,
    area: Rect,
) -> Vec<Option<Form>> {
    layout
        .runs
        .iter()
        .map(|run| {
            if outline {
                let glyphs = glyphs_path(fonts, std::iter::once(run));
                return (!glyphs.elements().is_empty())
                    .then_some(Form::Path(glyphs, FillRule::NonZero));
            }
            let first = run.glyphs.first()?;
            let text = run.glyph_text();
            if text.trim().is_empty() {
                return None;
            }
            let f = &run.face;
            let mut font = format!(
                " xml:space=\"preserve\" style=\"white-space: pre\" font-family=\"{}\" font-size=\"{}\"",
                font_family(&f.family),
                num(run.size as f64)
            );
            if (f.weight - 400.0).abs() > 0.5 {
                let _ = write!(font, " font-weight=\"{}\"", num(f.weight as f64));
            }
            if f.italic || run.skew.is_some() {
                font.push_str(" font-style=\"italic\"");
            }
            if (f.stretch - 1.0).abs() > 1e-3 {
                let _ = write!(font, " font-stretch=\"{}%\"", num(f.stretch as f64 * 100.0));
            }
            if run.letter_spacing != 0.0 {
                let _ = write!(font, " letter-spacing=\"{}\"", num(run.letter_spacing as f64));
            }
            Some(Form::Text {
                font,
                spans: vec![(first.x as f64, first.y as f64, text.to_string())],
                bbox: area,
            })
        })
        .collect()
}

/// Грубо: сколько байт займёт форма в файле (для проверки предела до записи):
/// путь — по числу элементов, текст — атрибуты шрифта и строки с худшим
/// экранированием (`&quot;` — 6 байт на символ).
fn form_len(form: &Form) -> usize {
    match form {
        Form::Path(p, _) => p.elements().len() * 24,
        Form::Text { font, spans, .. } => {
            font.len()
                + spans
                    .iter()
                    .map(|(_, _, t)| t.len() * 6 + 64)
                    .sum::<usize>()
        }
        _ => 0,
    }
}

fn form_of(g: Geometry) -> Option<Form> {
    match g {
        Geometry::Rect { rect, radius } => Some(Form::Rect { rect, radius }),
        Geometry::Ellipse(r) => Some(Form::Ellipse(r)),
        Geometry::Line(len) => Some(Form::Line(len)),
        Geometry::None | Geometry::Vector(_) => None,
    }
}

/// SVG узлов `ids` (как экспорт PNG: область — объединение областей узлов,
/// порядок — порядок отрисовки, потомки выбранных предков не повторяются).
pub(crate) fn export_svg(
    scene: &Scene,
    ids: &[ferrum_doc::NodeId],
    images: &dyn ImageBytes,
    fonts: &SharedFonts,
    opts: SvgOptions,
    budget: &ExportBudget,
) -> Result<String, ExportError> {
    let (items, area) = export_items(scene, ids)?;
    if !(area.width().is_finite() && area.height().is_finite())
        || area.width() <= 0.0
        || area.height() <= 0.0
    {
        return Err(ExportError::Empty);
    }
    let root = Affine::translate(-area.origin().to_vec2());
    let mut w = Writer::new(scene, images, fonts, opts, budget, root);
    for (slot, _) in items {
        w.write_tree(slot)?;
        w.check()?;
    }
    w.finish(area)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short_and_never_negative_zero() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(0.5), "0.5");
        assert_eq!(num(2.0 / 3.0), "0.667");
        assert_eq!(num(-12.3456), "-12.346");
        assert_eq!(num(f64::NAN), "0");
        assert_eq!(num(1e306), "0");
        assert_eq!(num(f64::INFINITY), "0");
    }

    #[test]
    fn base64_matches_rfc_4648() {
        for (raw, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(raw.as_bytes()), enc);
        }
    }

    #[test]
    fn escape_keeps_xml_well_formed() {
        assert_eq!(escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
        assert_eq!(escape("x\u{1}y\tz"), "xy\tz");
    }

    #[test]
    fn png_size_reads_the_header() {
        let mut px = vello_cpu::Pixmap::new(3, 2);
        px.data_as_u8_slice_mut().fill(255);
        assert_eq!(png_size(&px.into_png().unwrap()), Some((3, 2)));
        assert_eq!(png_size(b"GIF89a"), None);
    }
}
