//! Свой формат буфера обмена (`web application/x-ferrum`): самодостаточная копия
//! поддеревьев со свойствами, структурой и байтами изображений. Системный буфер —
//! забота интерфейса (этап 5); редактор отдаёт и принимает байты.
//!
//! Раскладка: `FERRUMCB` · версия (u32 LE) · длина JSON (u32 LE) · JSON · число
//! изображений (u32 LE) · для каждого: длина (u32 LE) и байты. JSON описывает узлы в
//! порядке обхода (родитель раньше детей) и хэши изображений в порядке их байтов.
//! У узлов верхнего уровня положение, поворот и отражение — мировые, у остальных —
//! как в документе, относительно родителя. Поля отражения `flipX`/`flipY`
//! необязательные и пишутся только со значением `true`: копия без отражений
//! побайтно та же, что у сборок до отражения, а копия из такой сборки разбирается
//! с `false`. Версия формата поэтому не менялась: старая сборка, получив копию с
//! отражением, вставит её неотражённой, но без ошибки и без небезопасных данных.
//!
//! Вставка не доверяет данным: размеры и количества ограничены, длины проверяются
//! до копирования, числа приводятся к пределам сцены, ссылки на родителя — только
//! назад и только на контейнеры, хэш каждого изображения сверяется с его байтами, а
//! каждая заливка с изображением должна ссылаться на переданные байты. Узлы
//! неизвестного типа (из будущих версий) пропускаются вместе с поддеревом.

use std::collections::HashSet;
use std::sync::Arc;

use ferrum_doc::{
    Color, Fill, FillKind, NodeKind, Props, PropsPatch, ScaleMode, Stroke, StrokeAlign, TextData,
};
use ferrum_scene::shape;
use kurbo::Rect;
use serde::{Deserialize, Serialize};

pub const MAGIC: &[u8; 8] = b"FERRUMCB";
pub const VERSION: u32 = 1;
/// Пределы для не доверенных данных.
pub const MAX_PAYLOAD: usize = 256 * 1024 * 1024;
/// Предел длины JSON-секции; проверяется до разбора.
pub const MAX_JSON: usize = 64 * 1024 * 1024;
/// Предел одного изображения; проверяется до копирования его байтов.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// Предел суммы байтов всех изображений.
pub const MAX_IMAGES_TOTAL: usize = 192 * 1024 * 1024;
pub const MAX_NODES: usize = 200_000;
pub const MAX_IMAGES: usize = 4096;
pub const MAX_PAINTS: usize = 64;
pub const MAX_NAME: usize = 1024;
/// Глубина вложенности: корень имеет глубину 0.
pub const MAX_DEPTH: usize = 256;

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
    #[serde(default, skip_serializing_if = "is_false")]
    flip_x: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    flip_y: bool,
    fills: Vec<JsonFill>,
    strokes: Vec<JsonStroke>,
    /// Текст узла (`TextData::to_json`) — только у текста. Сборка без текста поле
    /// не знает и пропускает узел «text» как неизвестный (версия формата та же).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

fn is_false(v: &bool) -> bool {
    !*v
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

/// Хэш в каноническом виде: 64 hex-символа в нижнем регистре, иначе `None`.
fn normalize_hash(h: &str) -> Option<String> {
    (h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())).then(|| h.to_ascii_lowercase())
}

/// Длина для раскладки. Длина, не помещающаяся в u32, не обрезается молча: она
/// насыщается, и разбор отвергает такой буфер.
fn len_bytes(n: usize) -> [u8; 4] {
    debug_assert!(u32::try_from(n).is_ok(), "длина {n} не помещается в u32");
    u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes()
}

/// Проверяет, что копию можно вставить обратно. Явно проверяются пределы, которые
/// разбор молча обрезал бы (имя), остальное — тем же разбором собранных байтов.
pub fn encodable(payload: &ClipPayload) -> Result<(), ClipboardError> {
    if payload.nodes.len() > MAX_NODES {
        return Err(ClipboardError::TooLarge("больше 200 000 узлов"));
    }
    if payload.images.len() > MAX_IMAGES {
        return Err(ClipboardError::TooLarge("больше 4096 изображений"));
    }
    for n in &payload.nodes {
        if n.props.name.chars().count() > MAX_NAME {
            return Err(ClipboardError::TooLarge("имя длиннее 1024 символов"));
        }
        if n.props.fills.len() > MAX_PAINTS || n.props.strokes.len() > MAX_PAINTS {
            return Err(ClipboardError::TooLarge(
                "слишком много заливок или обводок",
            ));
        }
    }
    ClipPayload::decode(&payload.encode()).map(|_| ())
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
                        flip_x: p.flip_x,
                        flip_y: p.flip_y,
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
                        text: p.text.as_ref().map(|t| t.to_json()),
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
        out.extend_from_slice(&len_bytes(text.len()));
        out.extend_from_slice(&text);
        out.extend_from_slice(&len_bytes(self.images.len()));
        for (_, bytes) in &self.images {
            out.extend_from_slice(&len_bytes(bytes.len()));
            out.extend_from_slice(bytes);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<ClipPayload, ClipboardError> {
        // Размер всего буфера и заголовок проверяются до любого выделения памяти.
        if bytes.len() > MAX_PAYLOAD {
            return Err(ClipboardError::TooLarge("больше 256 МБ"));
        }
        let mut r = Reader { bytes, at: 0 };
        // Короткий буфер тоже «не данные Ferrum», а не повреждённые данные.
        if r.take(8).ok() != Some(MAGIC.as_slice()) {
            return Err(ClipboardError::NotFerrum);
        }
        let version = r.u32()?;
        if version == 0 {
            return Err(corrupted("версия 0"));
        }
        if version > VERSION {
            return Err(ClipboardError::NewerVersion(version));
        }
        let len = r.u32()? as usize;
        if len > MAX_JSON {
            return Err(ClipboardError::TooLarge("описание узлов больше 64 МБ"));
        }
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
        let images = read_images(&mut r, json.images)?;
        if r.at != bytes.len() {
            return Err(corrupted("лишние байты в конце"));
        }
        let b = json.bounds;
        if !b.iter().all(|v| v.is_finite()) {
            return Err(corrupted("границы"));
        }
        let nodes = build_nodes(json.nodes)?;
        if nodes.is_empty() {
            return Err(ClipboardError::Empty);
        }
        // Каждая заливка с изображением ссылается на переданные байты: висячих ссылок нет.
        let have: HashSet<&str> = images.iter().map(|(h, _)| h.as_str()).collect();
        for n in &nodes {
            for f in &n.props.fills {
                let dangling = match &f.image_hash {
                    Some(h) => !have.contains(h.as_str()),
                    None => f.kind == FillKind::Image,
                };
                if dangling {
                    return Err(corrupted("заливка ссылается на отсутствующее изображение"));
                }
            }
        }
        // Рамка ограничена тем же пределом, что и координаты сцены: вставка не переполнится.
        let clamp = |v: f64| v.clamp(-shape::COORD_LIMIT, shape::COORD_LIMIT);
        Ok(ClipPayload {
            nodes,
            images,
            bounds: Rect::new(clamp(b[0]), clamp(b[1]), clamp(b[2]), clamp(b[3])).abs(),
        })
    }
}

/// Читает тела изображений: длина проверяется до копирования, хэш — до сохранения.
fn read_images(
    r: &mut Reader<'_>,
    hashes: Vec<String>,
) -> Result<Vec<(String, Vec<u8>)>, ClipboardError> {
    let mut images = Vec::with_capacity(hashes.len());
    let mut seen = HashSet::new();
    let mut total = 0usize;
    for claimed in hashes {
        let hash = normalize_hash(&claimed).ok_or_else(|| corrupted("хэш изображения"))?;
        let n = r.u32()? as usize;
        if n > MAX_IMAGE_BYTES {
            return Err(ClipboardError::TooLarge("изображение больше 64 МБ"));
        }
        total = total.saturating_add(n);
        if total > MAX_IMAGES_TOTAL {
            return Err(ClipboardError::TooLarge("изображения больше 192 МБ"));
        }
        let body = r.take(n)?;
        if blake3::hash(body).to_hex().as_str() != hash.as_str() {
            return Err(corrupted("байты изображения не сходятся с хэшем"));
        }
        if !seen.insert(hash.clone()) {
            return Err(corrupted("повторный хэш изображения"));
        }
        images.push((hash, body.to_vec()));
    }
    Ok(images)
}

/// Собирает узлы: родитель — раньше в списке, контейнер (рамка или группа), глубина
/// не больше `MAX_DEPTH`. Узел неизвестного типа пропускается вместе с потомками.
fn build_nodes(json_nodes: Vec<JsonNode>) -> Result<Vec<ClipNode>, ClipboardError> {
    // `kept[i]` — индекс узла `i` в результате или `None`, если он пропущен.
    let mut kept: Vec<Option<usize>> = Vec::with_capacity(json_nodes.len());
    let mut nodes: Vec<ClipNode> = Vec::new();
    // Глубина каждого узла результата; считается по родителю, без рекурсии.
    let mut depth: Vec<usize> = Vec::new();
    for (i, n) in json_nodes.into_iter().enumerate() {
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
        let (parent, d) = match parent {
            None => (None, 0),
            Some(k) => {
                if !matches!(nodes[k].props.kind, NodeKind::Frame | NodeKind::Group) {
                    return Err(corrupted("у узла этого типа не бывает потомков"));
                }
                let d = depth[k] + 1;
                if d > MAX_DEPTH {
                    return Err(ClipboardError::TooLarge("вложенность глубже 256 уровней"));
                }
                (Some(k), d)
            }
        };
        let props = props_from_json(kind, n)?;
        kept.push(Some(nodes.len()));
        nodes.push(ClipNode { parent, props });
        depth.push(d);
    }
    Ok(nodes)
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
        // Хэш приводится к нижнему регистру, чтобы совпасть с ключом изображения.
        let image_hash = match f.image_hash {
            Some(h) => Some(normalize_hash(&h).ok_or_else(|| corrupted("хэш изображения"))?),
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
        flip_x: n.flip_x,
        flip_y: n.flip_y,
        fills,
        strokes,
        // Текст — только у текстового узла; пределы проверяет `from_json`.
        text: match (kind, n.text.as_deref()) {
            (NodeKind::Text, Some(json)) => Some(Arc::new(
                TextData::from_json(json).map_err(|e| corrupted(e.to_string()))?,
            )),
            (NodeKind::Text, None) => Some(Arc::new(TextData::default())),
            _ => None,
        },
    };
    // Нечисловые и огромные значения — к допустимым, как у строк кэша сцены.
    shape::sanitize(&mut props);
    // Размер, которого у узла этого типа не бывает, не принимается из чужих данных:
    // у линии нет высоты, у группы — своей рамки (границы считаются по детям), а
    // отражение группы идёт относительно начала её координат (`w = h = 0`).
    match kind {
        NodeKind::Line => props.h = 0.0,
        NodeKind::Group => (props.w, props.h) = (0.0, 0.0),
        _ => {}
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
        // Патч ложится на только что созданный узел, у которого отражения нет:
        // нейтральное `false` не записывается, как и в `write_props` документа.
        flip_x: p.flip_x.then_some(true),
        flip_y: p.flip_y.then_some(true),
        fills: Some(p.fills.clone()),
        strokes: Some(p.strokes.clone()),
        text: p.text.as_ref().map(|t| t.props.clone()),
        content: None,
    }
}
