//! Свой формат буфера обмена (`web application/x-ferrum`): самодостаточная копия
//! поддеревьев со свойствами, структурой и байтами изображений. Системный буфер —
//! забота интерфейса (этап 5); редактор отдаёт и принимает байты.
//!
//! Раскладка: `FERRUMCB` · версия (u32 LE) · длина JSON (u32 LE) · JSON · число
//! изображений (u32 LE) · для каждого: длина (u32 LE) и байты. JSON описывает узлы в
//! порядке обхода (родитель раньше детей) и хэши изображений в порядке их байтов.
//! У узлов верхнего уровня положение и поворот — мировые, у остальных — как в
//! документе, относительно родителя.
//!
//! Вставка не доверяет данным: размеры и количества ограничены, числа проверяются,
//! ссылки на родителя — только назад, хэш изображения сверяется с байтами.
//! Узлы неизвестного типа (из будущих версий) пропускаются вместе с поддеревом.

use ferrum_doc::{
    Color, Fill, FillKind, NodeKind, Props, PropsPatch, ScaleMode, Stroke, StrokeAlign,
};
use ferrum_scene::shape;
use kurbo::Rect;
use serde::{Deserialize, Serialize};

pub const MAGIC: &[u8; 8] = b"FERRUMCB";
pub const VERSION: u32 = 1;
/// Пределы для не доверенных данных.
pub const MAX_PAYLOAD: usize = 256 * 1024 * 1024;
pub const MAX_NODES: usize = 200_000;
pub const MAX_IMAGES: usize = 4096;
pub const MAX_PAINTS: usize = 64;
pub const MAX_NAME: usize = 1024;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ClipboardError {
    #[error("В буфере обмена не данные Ferrum")]
    NotFerrum,
    #[error("Данные из более новой версии Ferrum ({0}). Обновите Ferrum")]
    NewerVersion(u32),
    #[error("Данные в буфере обмена повреждены: {0}")]
    Corrupted(String),
    #[error("Слишком большая вставка: {0}")]
    TooLarge(&'static str),
    #[error("Нечего вставить")]
    Empty,
}

fn corrupted(s: impl Into<String>) -> ClipboardError {
    ClipboardError::Corrupted(s.into())
}

/// Узел копии: родитель — индекс раньше в списке.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipNode {
    pub parent: Option<usize>,
    pub props: Props,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipPayload {
    pub nodes: Vec<ClipNode>,
    /// Заявленный хэш и байты изображения.
    pub images: Vec<(String, Vec<u8>)>,
    /// Общая рамка узлов верхнего уровня в мире на момент копирования.
    pub bounds: Rect,
}

#[derive(Serialize, Deserialize)]
struct JsonPayload {
    nodes: Vec<JsonNode>,
    images: Vec<String>,
    bounds: [f64; 4],
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonNode {
    parent: Option<usize>,
    kind: String,
    name: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    rotation: f64,
    opacity: f64,
    visible: bool,
    locked: bool,
    corner_radius: f64,
    clip_content: bool,
    fills: Vec<JsonFill>,
    strokes: Vec<JsonStroke>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonFill {
    kind: String,
    color: String,
    image_hash: Option<String>,
    scale_mode: String,
    opacity: f64,
    visible: bool,
}

#[derive(Serialize, Deserialize)]
struct JsonStroke {
    color: String,
    width: f64,
    align: String,
}

fn fill_kind(k: FillKind) -> &'static str {
    match k {
        FillKind::Solid => "solid",
        FillKind::Image => "image",
    }
}

fn scale_mode(m: ScaleMode) -> &'static str {
    match m {
        ScaleMode::Fill => "fill",
        ScaleMode::Fit => "fit",
        ScaleMode::Tile => "tile",
    }
}

fn align(a: StrokeAlign) -> &'static str {
    match a {
        StrokeAlign::Inside => "inside",
        StrokeAlign::Center => "center",
        StrokeAlign::Outside => "outside",
    }
}

impl ClipPayload {
    pub fn encode(&self) -> Vec<u8> {
        let json = JsonPayload {
            nodes: self
                .nodes
                .iter()
                .map(|n| {
                    let p = &n.props;
                    JsonNode {
                        parent: n.parent,
                        kind: p.kind.as_str().to_string(),
                        name: p.name.clone(),
                        x: p.x,
                        y: p.y,
                        w: p.w,
                        h: p.h,
                        rotation: p.rotation,
                        opacity: p.opacity,
                        visible: p.visible,
                        locked: p.locked,
                        corner_radius: p.corner_radius,
                        clip_content: p.clip_content,
                        fills: p
                            .fills
                            .iter()
                            .map(|f| JsonFill {
                                kind: fill_kind(f.kind).to_string(),
                                color: f.color.to_hex(),
                                image_hash: f.image_hash.clone(),
                                scale_mode: scale_mode(f.scale_mode).to_string(),
                                opacity: f.opacity,
                                visible: f.visible,
                            })
                            .collect(),
                        strokes: p
                            .strokes
                            .iter()
                            .map(|s| JsonStroke {
                                color: s.color.to_hex(),
                                width: s.width,
                                align: align(s.align).to_string(),
                            })
                            .collect(),
                    }
                })
                .collect(),
            images: self.images.iter().map(|(h, _)| h.clone()).collect(),
            bounds: [
                self.bounds.x0,
                self.bounds.y0,
                self.bounds.x1,
                self.bounds.y1,
            ],
        };
        let text = serde_json::to_vec(&json).expect("сериализация копии");
        let mut out = Vec::with_capacity(
            24 + text.len() + self.images.iter().map(|(_, b)| b.len() + 4).sum::<usize>(),
        );
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(&text);
        out.extend_from_slice(&(self.images.len() as u32).to_le_bytes());
        for (_, bytes) in &self.images {
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<ClipPayload, ClipboardError> {
        if bytes.len() > MAX_PAYLOAD {
            return Err(ClipboardError::TooLarge("больше 256 МБ"));
        }
        let mut r = Reader { bytes, at: 0 };
        if r.take(8).ok() != Some(MAGIC.as_slice()) {
            return Err(ClipboardError::NotFerrum);
        }
        let version = r.u32()?;
        if version > VERSION {
            return Err(ClipboardError::NewerVersion(version));
        }
        let len = r.u32()? as usize;
        let text = r.take(len)?;
        let json: JsonPayload =
            serde_json::from_slice(text).map_err(|e| corrupted(format!("описание узлов: {e}")))?;
        if json.nodes.len() > MAX_NODES {
            return Err(ClipboardError::TooLarge("больше 200 000 узлов"));
        }
        if json.images.len() > MAX_IMAGES {
            return Err(ClipboardError::TooLarge("больше 4096 изображений"));
        }
        let count = r.u32()? as usize;
        if count != json.images.len() {
            return Err(corrupted("число изображений не совпадает"));
        }
        let mut images = Vec::with_capacity(count);
        for hash in json.images {
            let n = r.u32()? as usize;
            images.push((hash, r.take(n)?.to_vec()));
        }
        if r.at != bytes.len() {
            return Err(corrupted("лишние байты в конце"));
        }
        let b = json.bounds;
        if !b.iter().all(|v| v.is_finite()) {
            return Err(corrupted("границы"));
        }
        // Узел неизвестного типа и его потомки пропускаются; индексы родителей
        // пересчитываются под оставшиеся узлы.
        let mut kept: Vec<Option<usize>> = Vec::with_capacity(json.nodes.len());
        let mut nodes = Vec::new();
        for (i, n) in json.nodes.into_iter().enumerate() {
            let parent = match n.parent {
                None => None,
                Some(p) if p < i => match kept[p] {
                    Some(k) => Some(k),
                    None => {
                        kept.push(None);
                        continue;
                    }
                },
                Some(_) => return Err(corrupted("ссылка на родителя вперёд")),
            };
            let Some(kind) = NodeKind::parse(&n.kind) else {
                kept.push(None);
                continue;
            };
            let props = props_from_json(kind, n)?;
            kept.push(Some(nodes.len()));
            nodes.push(ClipNode { parent, props });
        }
        if nodes.is_empty() {
            return Err(ClipboardError::Empty);
        }
        Ok(ClipPayload {
            nodes,
            images,
            bounds: Rect::new(b[0], b[1], b[2], b[3]).abs(),
        })
    }
}

fn props_from_json(kind: NodeKind, n: JsonNode) -> Result<Props, ClipboardError> {
    if n.fills.len() > MAX_PAINTS || n.strokes.len() > MAX_PAINTS {
        return Err(ClipboardError::TooLarge(
            "слишком много заливок или обводок",
        ));
    }
    let color = |s: &str| Color::parse(s).map_err(|_| corrupted("цвет"));
    let mut fills = Vec::with_capacity(n.fills.len());
    for f in n.fills {
        let kind = match f.kind.as_str() {
            "solid" => FillKind::Solid,
            "image" => FillKind::Image,
            _ => return Err(corrupted("вид заливки")),
        };
        let scale_mode = match f.scale_mode.as_str() {
            "fill" => ScaleMode::Fill,
            "fit" => ScaleMode::Fit,
            "tile" => ScaleMode::Tile,
            _ => return Err(corrupted("режим изображения")),
        };
        let image_hash = match f.image_hash {
            Some(h) if h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()) => Some(h),
            Some(_) => return Err(corrupted("хэш изображения")),
            None => None,
        };
        fills.push(Fill {
            kind,
            color: color(&f.color)?,
            image_hash,
            scale_mode,
            opacity: f.opacity,
            visible: f.visible,
        });
    }
    let mut strokes = Vec::with_capacity(n.strokes.len());
    for s in n.strokes {
        let align = match s.align.as_str() {
            "inside" => StrokeAlign::Inside,
            "center" => StrokeAlign::Center,
            "outside" => StrokeAlign::Outside,
            _ => return Err(corrupted("выравнивание обводки")),
        };
        strokes.push(Stroke {
            color: color(&s.color)?,
            width: s.width,
            align,
        });
    }
    let mut props = Props {
        kind,
        name: n.name.chars().take(MAX_NAME).collect(),
        x: n.x,
        y: n.y,
        w: n.w,
        h: n.h,
        rotation: n.rotation,
        opacity: n.opacity,
        visible: n.visible,
        locked: n.locked,
        corner_radius: n.corner_radius,
        clip_content: n.clip_content,
        fills,
        strokes,
    };
    // Нечисловые и огромные значения — к допустимым, как у строк кэша сцены.
    shape::sanitize(&mut props);
    if kind == NodeKind::Line {
        props.h = 0.0;
    }
    Ok(props)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ClipboardError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|e| *e <= self.bytes.len())
            .ok_or_else(|| corrupted("данные обрываются"))?;
        let s = &self.bytes[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, ClipboardError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Полный патч из свойств — для создания копии узла.
pub fn full_patch(p: &Props) -> PropsPatch {
    PropsPatch {
        name: Some(p.name.clone()),
        x: Some(p.x),
        y: Some(p.y),
        w: Some(p.w),
        h: Some(p.h),
        rotation: Some(p.rotation),
        opacity: Some(p.opacity),
        visible: Some(p.visible),
        locked: Some(p.locked),
        corner_radius: Some(p.corner_radius),
        clip_content: Some(p.clip_content),
        fills: Some(p.fills.clone()),
        strokes: Some(p.strokes.clone()),
    }
}
