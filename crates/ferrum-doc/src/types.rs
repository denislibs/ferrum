//! Типы данных документа: идентификаторы, цвета, узлы и их свойства.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use loro::TreeID;

use crate::error::DocError;
use crate::text::{TextContent, TextData, TextProps, TEXT_COLOR};
use crate::vector::{StrokeCap, StrokeJoin, StrokeStyle, VectorNetwork};

/// Идентификатор узла. Обёртка над `TreeID` Loro: уникален и при совместной работе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) TreeID);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for NodeId {
    type Err = DocError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TreeID::try_from(s)
            .map(NodeId)
            .map_err(|_| DocError::InvalidId(s.to_string()))
    }
}

/// Идентификатор страницы — узел в дереве страниц корневого документа.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId(pub(crate) TreeID);

impl fmt::Display for PageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for PageId {
    type Err = DocError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TreeID::try_from(s)
            .map(PageId)
            .map_err(|_| DocError::InvalidId(s.to_string()))
    }
}

/// Цвет sRGB с альфой. В документе хранится строкой `#rrggbbaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgba(0, 0, 0, 255);
    pub const WHITE: Color = Color::rgba(255, 255, 255, 255);
    /// Серая заливка новых фигур, как в Figma.
    pub const SHAPE_GRAY: Color = Color::rgba(0xd9, 0xd9, 0xd9, 255);

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    /// Принимает `#rrggbb` и `#rrggbbaa` в любом регистре.
    pub fn parse(s: &str) -> Result<Color, DocError> {
        let bad = || DocError::InvalidColor(s.to_string());
        let hex = s.strip_prefix('#').ok_or_else(bad)?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
            return Err(bad());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        let a = if hex.len() == 8 { byte(6)? } else { 255 };
        Ok(Color::rgba(byte(0)?, byte(2)?, byte(4)?, a))
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Frame,
    Group,
    Rect,
    Ellipse,
    Line,
    Image,
    Text,
    /// Векторная сеть (спека подсистемы 3, раздел 9).
    Vector,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Frame => "frame",
            NodeKind::Group => "group",
            NodeKind::Rect => "rect",
            NodeKind::Ellipse => "ellipse",
            NodeKind::Line => "line",
            NodeKind::Image => "image",
            NodeKind::Text => "text",
            NodeKind::Vector => "vector",
        }
    }

    pub fn parse(s: &str) -> Option<NodeKind> {
        Some(match s {
            "frame" => NodeKind::Frame,
            "group" => NodeKind::Group,
            "rect" => NodeKind::Rect,
            "ellipse" => NodeKind::Ellipse,
            "line" => NodeKind::Line,
            "image" => NodeKind::Image,
            "text" => NodeKind::Text,
            "vector" => NodeKind::Vector,
            _ => return None,
        })
    }

    /// Имя нового узла в интерфейсе.
    pub fn default_name(self) -> &'static str {
        match self {
            NodeKind::Frame => "Фрейм",
            NodeKind::Group => "Группа",
            NodeKind::Rect => "Прямоугольник",
            NodeKind::Ellipse => "Эллипс",
            NodeKind::Line => "Линия",
            NodeKind::Image => "Изображение",
            NodeKind::Text => "Текст",
            NodeKind::Vector => "Вектор",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillKind {
    Solid,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    Fill,
    Fit,
    Tile,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub kind: FillKind,
    pub color: Color,
    /// Хэш blake3 (hex) байтов изображения; только для `FillKind::Image`.
    pub image_hash: Option<String>,
    pub scale_mode: ScaleMode,
    pub opacity: f64,
    pub visible: bool,
}

impl Fill {
    pub fn solid(color: Color) -> Fill {
        Fill {
            kind: FillKind::Solid,
            color,
            image_hash: None,
            scale_mode: ScaleMode::Fill,
            opacity: 1.0,
            visible: true,
        }
    }

    pub fn image(hash: impl Into<String>) -> Fill {
        Fill {
            kind: FillKind::Image,
            color: Color::WHITE,
            image_hash: Some(hash.into()),
            scale_mode: ScaleMode::Fill,
            opacity: 1.0,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeAlign {
    Inside,
    Center,
    Outside,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f64,
    pub align: StrokeAlign,
}

/// Полный набор свойств узла после чтения из документа.
#[derive(Debug, Clone, PartialEq)]
pub struct Props {
    pub kind: NodeKind,
    pub name: String,
    pub x: f64,
    pub y: f64,
    /// У группы не хранится: границы вычисляет сцена по детям.
    pub w: f64,
    pub h: f64,
    pub rotation: f64,
    pub opacity: f64,
    pub visible: bool,
    pub locked: bool,
    pub corner_radius: f64,
    pub clip_content: bool,
    /// Отражение по оси x узла (слева направо) относительно центра его рамки, как
    /// в Figma: рамка остаётся на месте, зеркально рисуется содержимое — заливка
    /// изображением и дети. Нейтральное значение — `false`.
    pub flip_x: bool,
    /// Отражение по оси y узла (сверху вниз) относительно центра его рамки.
    pub flip_y: bool,
    pub fills: Vec<Fill>,
    pub strokes: Vec<Stroke>,
    /// Окончания, соединение и предел острого угла — общие для всех обводок узла
    /// (как в Figma). Нейтральные — без окончаний, острый стык, предел 4.
    pub stroke_style: StrokeStyle,
    /// Текст и его свойства — только у `NodeKind::Text`.
    pub text: Option<Arc<TextData>>,
    /// Сеть — только у `NodeKind::Vector`. `Arc`: правка свойств узла не копирует
    /// 100 000 точек, а сравнение одной и той же сети — по указателю.
    pub vector: Option<Arc<VectorNetwork>>,
}

impl Props {
    /// Значения по умолчанию для нового узла, как в Figma.
    pub fn defaults(kind: NodeKind) -> Props {
        let (w, h, fills, strokes) = match kind {
            NodeKind::Frame => (100.0, 100.0, vec![Fill::solid(Color::WHITE)], vec![]),
            NodeKind::Group => (0.0, 0.0, vec![], vec![]),
            NodeKind::Rect | NodeKind::Ellipse => {
                (100.0, 100.0, vec![Fill::solid(Color::SHAPE_GRAY)], vec![])
            }
            NodeKind::Line => (
                100.0,
                0.0,
                vec![],
                vec![Stroke {
                    color: Color::BLACK,
                    width: 1.0,
                    align: StrokeAlign::Center,
                }],
            ),
            NodeKind::Image => (100.0, 100.0, vec![], vec![]),
            // Размер пустого текста ставит редактор по раскладке; цвет — чёрный.
            NodeKind::Text => (0.0, 0.0, vec![Fill::solid(TEXT_COLOR)], vec![]),
            // Как у пера Figma: без заливки, чёрная обводка 1 по центру.
            NodeKind::Vector => (
                0.0,
                0.0,
                vec![],
                vec![Stroke {
                    color: Color::BLACK,
                    width: 1.0,
                    align: StrokeAlign::Center,
                }],
            ),
        };
        Props {
            kind,
            name: kind.default_name().to_string(),
            x: 0.0,
            y: 0.0,
            w,
            h,
            rotation: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            corner_radius: 0.0,
            clip_content: kind == NodeKind::Frame,
            flip_x: false,
            flip_y: false,
            fills,
            strokes,
            stroke_style: StrokeStyle::default(),
            text: (kind == NodeKind::Text).then(|| Arc::new(TextData::default())),
            vector: (kind == NodeKind::Vector).then(|| Arc::new(VectorNetwork::default())),
        }
    }

    /// Свойства текста (у не текста — `None`).
    pub fn text_props(&self) -> Option<&TextProps> {
        self.text.as_deref().map(|t| &t.props)
    }
}

/// Частичное изменение свойств: `None` — не трогать.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropsPatch {
    pub name: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub w: Option<f64>,
    pub h: Option<f64>,
    pub rotation: Option<f64>,
    pub opacity: Option<f64>,
    pub visible: Option<bool>,
    pub locked: Option<bool>,
    pub corner_radius: Option<f64>,
    pub clip_content: Option<bool>,
    pub flip_x: Option<bool>,
    pub flip_y: Option<bool>,
    pub fills: Option<Vec<Fill>>,
    pub strokes: Option<Vec<Stroke>>,
    /// Свойства текста целиком (содержимое правится `Tx::insert_text` и соседями).
    /// У не текста пропускается.
    pub text: Option<TextProps>,
    /// Содержимое текста целиком (`Tx::set_text_content` той же правкой): превью
    /// сцены показывает составляемый текст IME и стиль участка до отпускания поля
    /// панели. У не текста пропускается.
    pub content: Option<TextContent>,
    /// Окончания обводки в начале и в конце открытых путей, соединение и предел
    /// острого угла — ключами узла по отдельности (совместная правка не затирает
    /// соседний ключ).
    pub stroke_cap_start: Option<StrokeCap>,
    pub stroke_cap_end: Option<StrokeCap>,
    pub stroke_join: Option<StrokeJoin>,
    pub stroke_miter_limit: Option<f64>,
    /// Сеть целиком (значение узла заменяется). У не вектора пропускается.
    pub vector: Option<Arc<VectorNetwork>>,
}

impl PropsPatch {
    /// Прежние значения тех полей, которые меняет этот патч, — патч для отмены.
    pub fn previous(&self, before: &Props) -> PropsPatch {
        PropsPatch {
            name: self.name.as_ref().map(|_| before.name.clone()),
            x: self.x.map(|_| before.x),
            y: self.y.map(|_| before.y),
            w: self.w.map(|_| before.w),
            h: self.h.map(|_| before.h),
            rotation: self.rotation.map(|_| before.rotation),
            opacity: self.opacity.map(|_| before.opacity),
            visible: self.visible.map(|_| before.visible),
            locked: self.locked.map(|_| before.locked),
            corner_radius: self.corner_radius.map(|_| before.corner_radius),
            clip_content: self.clip_content.map(|_| before.clip_content),
            flip_x: self.flip_x.map(|_| before.flip_x),
            flip_y: self.flip_y.map(|_| before.flip_y),
            fills: self.fills.as_ref().map(|_| before.fills.clone()),
            strokes: self.strokes.as_ref().map(|_| before.strokes.clone()),
            text: match (&self.text, &before.text) {
                (Some(_), Some(t)) => Some(t.props.clone()),
                _ => None,
            },
            content: match (&self.content, &before.text) {
                (Some(_), Some(t)) => Some(t.content.clone()),
                _ => None,
            },
            stroke_cap_start: self.stroke_cap_start.map(|_| before.stroke_style.cap_start),
            stroke_cap_end: self.stroke_cap_end.map(|_| before.stroke_style.cap_end),
            stroke_join: self.stroke_join.map(|_| before.stroke_style.join),
            stroke_miter_limit: self
                .stroke_miter_limit
                .map(|_| before.stroke_style.miter_limit),
            vector: match (&self.vector, &before.vector) {
                (Some(_), Some(v)) => Some(v.clone()),
                _ => None,
            },
        }
    }

    /// Применяет изменения к уже прочитанным свойствам (для зеркала страницы).
    pub fn apply_to(&self, p: &mut Props) {
        if let Some(v) = &self.name {
            p.name = v.clone();
        }
        for (dst, src) in [
            (&mut p.x, self.x),
            (&mut p.y, self.y),
            (&mut p.w, self.w),
            (&mut p.h, self.h),
            (&mut p.rotation, self.rotation),
            (&mut p.opacity, self.opacity),
            (&mut p.corner_radius, self.corner_radius),
        ] {
            if let Some(v) = src {
                *dst = v;
            }
        }
        for (dst, src) in [
            (&mut p.visible, self.visible),
            (&mut p.locked, self.locked),
            (&mut p.clip_content, self.clip_content),
            (&mut p.flip_x, self.flip_x),
            (&mut p.flip_y, self.flip_y),
        ] {
            if let Some(v) = src {
                *dst = v;
            }
        }
        if let Some(v) = &self.fills {
            p.fills = v.clone();
        }
        if let Some(v) = &self.strokes {
            p.strokes = v.clone();
        }
        let s = &mut p.stroke_style;
        if let Some(v) = self.stroke_cap_start {
            s.cap_start = v;
        }
        if let Some(v) = self.stroke_cap_end {
            s.cap_end = v;
        }
        if let Some(v) = self.stroke_join {
            s.join = v;
        }
        if let Some(v) = self.stroke_miter_limit {
            s.miter_limit = v;
        }
        if let (Some(v), Some(dst)) = (&self.vector, &mut p.vector) {
            *dst = v.clone();
        }

        if let Some(t) = &mut p.text {
            if self.text.is_some() || self.content.is_some() {
                // Новый `Arc`: общий с зеркалом `TextData` не копируется дважды.
                *t = Arc::new(TextData {
                    props: self.text.clone().unwrap_or_else(|| t.props.clone()),
                    content: self.content.clone().unwrap_or_else(|| t.content.clone()),
                });
            }
        }
    }
}
