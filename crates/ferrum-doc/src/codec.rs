//! Чтение и запись свойств узла в `LoroMap`. Каждое свойство — отдельный ключ,
//! чтобы одновременные правки разных свойств одного узла не затирали друг друга.

use std::collections::HashMap;
use std::sync::Arc;

use loro::{Container, LoroMap, LoroValue, ValueOrContainer};

use crate::boolean::BooleanOp;
use crate::error::{DocError, DocResult};
use crate::text::TextData;
use crate::text_codec;
use crate::types::*;
use crate::vector::{StrokeCap, StrokeJoin, StrokeStyle, VectorNetwork};

pub(crate) mod key {
    pub const TYPE: &str = "type";
    pub const NAME: &str = "name";
    pub const X: &str = "x";
    pub const Y: &str = "y";
    pub const W: &str = "w";
    pub const H: &str = "h";
    pub const ROTATION: &str = "rotation";
    pub const OPACITY: &str = "opacity";
    pub const VISIBLE: &str = "visible";
    pub const LOCKED: &str = "locked";
    pub const CORNER_RADIUS: &str = "cornerRadius";
    pub const CLIP_CONTENT: &str = "clipContent";
    pub const FLIP_X: &str = "flipX";
    pub const FLIP_Y: &str = "flipY";
    pub const FILLS: &str = "fills";
    pub const STROKES: &str = "strokes";
    pub const STROKE_CAP_START: &str = "strokeCapStart";
    pub const STROKE_CAP_END: &str = "strokeCapEnd";
    pub const STROKE_JOIN: &str = "strokeJoin";
    pub const STROKE_MITER_LIMIT: &str = "strokeMiterLimit";
    /// Сеть узла `vector` — двоичное значение (`VectorNetwork::encode`).
    pub const VECTOR: &str = "vector";
    /// Операция узла `boolean` — строка (`BooleanOp::as_str`).
    pub const BOOLEAN_OP: &str = "booleanOp";
}

pub(crate) fn value(map: &LoroMap, k: &str) -> Option<LoroValue> {
    match map.get(k)? {
        ValueOrContainer::Value(v) => Some(v),
        ValueOrContainer::Container(Container::List(l)) => Some(l.get_deep_value()),
        ValueOrContainer::Container(Container::Map(m)) => Some(m.get_deep_value()),
        ValueOrContainer::Container(_) => None,
    }
}

pub(crate) fn as_f64(v: &LoroValue) -> Option<f64> {
    match v {
        LoroValue::Double(d) => Some(*d),
        LoroValue::I64(i) => Some(*i as f64),
        _ => None,
    }
}

pub(crate) fn as_bool(v: &LoroValue) -> Option<bool> {
    match v {
        LoroValue::Bool(b) => Some(*b),
        _ => None,
    }
}

pub(crate) fn as_str(v: &LoroValue) -> Option<String> {
    match v {
        LoroValue::String(s) => Some(s.to_string()),
        _ => None,
    }
}

fn f64_or(map: &LoroMap, k: &str, default: f64) -> f64 {
    value(map, k).as_ref().and_then(as_f64).unwrap_or(default)
}

fn bool_or(map: &LoroMap, k: &str, default: bool) -> bool {
    value(map, k).as_ref().and_then(as_bool).unwrap_or(default)
}

/// Стиль обводки: неизвестное окончание или соединение (из будущей сборки) —
/// умолчание, предел острого угла — в допустимых пределах.
fn read_stroke_style(map: &LoroMap) -> StrokeStyle {
    let d = StrokeStyle::default();
    let s = |k| value(map, k).as_ref().and_then(as_str);
    let limit = f64_or(map, key::STROKE_MITER_LIMIT, d.miter_limit);
    StrokeStyle {
        cap_start: s(key::STROKE_CAP_START)
            .and_then(|v| StrokeCap::parse(&v))
            .unwrap_or(d.cap_start),
        cap_end: s(key::STROKE_CAP_END)
            .and_then(|v| StrokeCap::parse(&v))
            .unwrap_or(d.cap_end),
        join: s(key::STROKE_JOIN)
            .and_then(|v| StrokeJoin::parse(&v))
            .unwrap_or(d.join),
        miter_limit: if limit.is_finite() {
            limit.clamp(
                *crate::MITER_LIMIT_RANGE.start(),
                *crate::MITER_LIMIT_RANGE.end(),
            )
        } else {
            d.miter_limit
        },
    }
}

/// Сеть узла `vector`; нет ключа — пустая сеть. Неверная — ошибка: узел
/// скрывается, документ — только для чтения (`PageDoc::rebuild_from_loro`).
fn read_vector(map: &LoroMap, id: &NodeId) -> DocResult<VectorNetwork> {
    match value(map, key::VECTOR) {
        None => Ok(VectorNetwork::default()),
        Some(LoroValue::Binary(b)) => {
            VectorNetwork::decode(&b).map_err(|e| DocError::InvalidVector(id.to_string(), e))
        }
        Some(_) => Err(DocError::InvalidVector(
            id.to_string(),
            crate::vector::VectorError::Invalid("не двоичное значение"),
        )),
    }
}

/// Операция булевой группы; нет ключа — объединение. Неизвестная (будущая
/// сборка, повреждение) — ошибка: узел скрывается, как узел неизвестного типа,
/// документ — только для чтения (`PageDoc::rebuild_from_loro`).
fn read_boolean_op(map: &LoroMap, id: &NodeId) -> DocResult<BooleanOp> {
    match value(map, key::BOOLEAN_OP) {
        None => Ok(BooleanOp::Union),
        Some(v) => {
            let raw = as_str(&v).unwrap_or_default();
            BooleanOp::parse(&raw).ok_or_else(|| DocError::InvalidBooleanOp(id.to_string(), raw))
        }
    }
}

/// Тип узла читается отдельно: по нему выбираются значения по умолчанию.
pub(crate) fn read_kind(map: &LoroMap, id: &NodeId) -> DocResult<NodeKind> {
    let raw = value(map, key::TYPE)
        .as_ref()
        .and_then(as_str)
        .ok_or_else(|| DocError::Corrupted(format!("у узла {id} не записан тип")))?;
    NodeKind::parse(&raw).ok_or_else(|| DocError::UnknownKind(id.to_string(), raw))
}

/// Отсутствующие ключи заменяются значениями по умолчанию для типа узла,
/// поэтому новые свойства не требуют миграции старых файлов.
pub(crate) fn read_props(map: &LoroMap, id: &NodeId) -> DocResult<Props> {
    let kind = read_kind(map, id)?;
    let d = Props::defaults(kind);
    Ok(Props {
        kind,
        name: value(map, key::NAME)
            .as_ref()
            .and_then(as_str)
            .unwrap_or(d.name),
        x: f64_or(map, key::X, d.x),
        y: f64_or(map, key::Y, d.y),
        w: f64_or(map, key::W, d.w),
        h: f64_or(map, key::H, d.h),
        rotation: f64_or(map, key::ROTATION, d.rotation),
        opacity: f64_or(map, key::OPACITY, d.opacity),
        visible: bool_or(map, key::VISIBLE, d.visible),
        locked: bool_or(map, key::LOCKED, d.locked),
        corner_radius: f64_or(map, key::CORNER_RADIUS, d.corner_radius),
        clip_content: bool_or(map, key::CLIP_CONTENT, d.clip_content),
        flip_x: bool_or(map, key::FLIP_X, d.flip_x),
        flip_y: bool_or(map, key::FLIP_Y, d.flip_y),
        fills: match value(map, key::FILLS) {
            Some(v) => read_list(&v, read_fill),
            None => d.fills,
        },
        strokes: match value(map, key::STROKES) {
            Some(v) => read_list(&v, read_stroke),
            None => d.strokes,
        },
        stroke_style: read_stroke_style(map),
        text: (kind == NodeKind::Text).then(|| {
            Arc::new(TextData {
                props: text_codec::read_text_props(map),
                content: text_codec::existing_text(map)
                    .map(|t| text_codec::read_content(&t))
                    .unwrap_or_default(),
            })
        }),
        vector: match kind {
            NodeKind::Vector => Some(Arc::new(read_vector(map, id)?)),
            _ => None,
        },
        boolean_op: match kind {
            NodeKind::Boolean => Some(read_boolean_op(map, id)?),
            _ => None,
        },
    })
}

/// Хэши изображений в заливках узла — без разбора остальных свойств и типа узла:
/// так читаются и узлы неизвестного типа, скрытые от модели.
pub(crate) fn read_image_refs(map: &LoroMap) -> Vec<String> {
    match value(map, key::FILLS) {
        Some(v) => read_list(&v, read_fill)
            .into_iter()
            .filter_map(|f| f.image_hash)
            .collect(),
        None => Vec::new(),
    }
}

/// Элементы, которые не удалось разобрать, пропускаются: лучше открыть файл
/// без одной заливки, чем не открыть совсем.
fn read_list<T>(v: &LoroValue, item: fn(&LoroValue) -> Option<T>) -> Vec<T> {
    match v {
        LoroValue::List(list) => list.iter().filter_map(item).collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn field<'a>(v: &'a LoroValue, k: &str) -> Option<&'a LoroValue> {
    match v {
        LoroValue::Map(m) => m.get(k),
        _ => None,
    }
}

pub(crate) fn read_fill(v: &LoroValue) -> Option<Fill> {
    let kind = match field(v, "kind").and_then(as_str)?.as_str() {
        "solid" => FillKind::Solid,
        "image" => FillKind::Image,
        _ => return None,
    };
    let scale_mode = match field(v, "scaleMode").and_then(as_str).as_deref() {
        Some("fit") => ScaleMode::Fit,
        Some("tile") => ScaleMode::Tile,
        _ => ScaleMode::Fill,
    };
    Some(Fill {
        kind,
        color: field(v, "color")
            .and_then(as_str)
            .and_then(|s| Color::parse(&s).ok())
            .unwrap_or(Color::WHITE),
        image_hash: field(v, "imageHash").and_then(as_str),
        scale_mode,
        opacity: field(v, "opacity").and_then(as_f64).unwrap_or(1.0),
        visible: field(v, "visible").and_then(as_bool).unwrap_or(true),
    })
}

fn read_stroke(v: &LoroValue) -> Option<Stroke> {
    let align = match field(v, "align").and_then(as_str).as_deref() {
        Some("inside") => StrokeAlign::Inside,
        Some("outside") => StrokeAlign::Outside,
        _ => StrokeAlign::Center,
    };
    Some(Stroke {
        color: Color::parse(&field(v, "color").and_then(as_str)?).ok()?,
        width: field(v, "width").and_then(as_f64).unwrap_or(1.0),
        align,
    })
}

/// Записывает свойства нового узла. Нейтральные значения (0, opacity 1, видим,
/// не заблокирован, не отражён) не пишутся: при чтении они подставляются как значения по умолчанию.
/// Поэтому нейтральные значения по умолчанию в схеме менять НЕЛЬЗЯ.
pub(crate) fn write_props(map: &LoroMap, p: &Props) -> DocResult<()> {
    map.insert(key::TYPE, p.kind.as_str())?;
    let neutral = |v: f64, n: f64| (v != n).then_some(v);
    let patch = PropsPatch {
        name: Some(p.name.clone()),
        x: neutral(p.x, 0.0),
        y: neutral(p.y, 0.0),
        w: Some(p.w),
        h: Some(p.h),
        rotation: neutral(p.rotation, 0.0),
        opacity: neutral(p.opacity, 1.0),
        visible: (!p.visible).then_some(false),
        locked: p.locked.then_some(true),
        corner_radius: neutral(p.corner_radius, 0.0),
        clip_content: Some(p.clip_content),
        flip_x: p.flip_x.then_some(true),
        flip_y: p.flip_y.then_some(true),
        fills: Some(p.fills.clone()),
        strokes: Some(p.strokes.clone()),
        text: p.text.as_ref().map(|t| t.props.clone()),
        // Содержимое пишется ниже, в новый контейнер.
        content: None,
        stroke_cap_start: Some(p.stroke_style.cap_start).filter(|v| *v != StrokeCap::None),
        stroke_cap_end: Some(p.stroke_style.cap_end).filter(|v| *v != StrokeCap::None),
        stroke_join: Some(p.stroke_style.join).filter(|v| *v != StrokeJoin::Miter),
        stroke_miter_limit: neutral(p.stroke_style.miter_limit, crate::DEFAULT_MITER_LIMIT),
        vector: p.vector.clone(),
        boolean_op: p.boolean_op,
    };
    write_patch(map, &patch)?;
    // Содержимое — в новый `LoroText` узла: узел только что создан (или
    // восстановлен отменой удаления под новым `TreeID`), контейнер пуст.
    if let Some(t) = &p.text {
        let text = text_codec::text_container(map)?;
        text.apply_delta(&text_codec::insert_delta(&t.content))?;
    }
    Ok(())
}

pub(crate) fn write_patch(map: &LoroMap, p: &PropsPatch) -> DocResult<()> {
    if let Some(v) = &p.name {
        map.insert(key::NAME, v.as_str())?;
    }
    for (k, v) in [
        (key::X, p.x),
        (key::Y, p.y),
        (key::W, p.w),
        (key::H, p.h),
        (key::ROTATION, p.rotation),
        (key::OPACITY, p.opacity),
        (key::CORNER_RADIUS, p.corner_radius),
    ] {
        if let Some(v) = v {
            map.insert(k, v)?;
        }
    }
    for (k, v) in [
        (key::VISIBLE, p.visible),
        (key::LOCKED, p.locked),
        (key::CLIP_CONTENT, p.clip_content),
        (key::FLIP_X, p.flip_x),
        (key::FLIP_Y, p.flip_y),
    ] {
        if let Some(v) = v {
            map.insert(k, v)?;
        }
    }
    // Заливки и обводки — неизменяемые значения, а не вложенные контейнеры Loro:
    // контейнер на каждую заливку раздувает документ в разы. Список заменяется целиком.
    if let Some(fills) = &p.fills {
        map.insert(
            key::FILLS,
            LoroValue::from(fills.iter().map(fill_value).collect::<Vec<_>>()),
        )?;
    }
    if let Some(strokes) = &p.strokes {
        map.insert(
            key::STROKES,
            LoroValue::from(strokes.iter().map(stroke_value).collect::<Vec<_>>()),
        )?;
    }
    for (k, v) in [
        (
            key::STROKE_CAP_START,
            p.stroke_cap_start.map(StrokeCap::as_str),
        ),
        (key::STROKE_CAP_END, p.stroke_cap_end.map(StrokeCap::as_str)),
        (key::STROKE_JOIN, p.stroke_join.map(StrokeJoin::as_str)),
    ] {
        if let Some(v) = v {
            map.insert(k, v)?;
        }
    }
    if let Some(v) = p.stroke_miter_limit {
        map.insert(key::STROKE_MITER_LIMIT, v)?;
    }
    // Вызывающий убирает `text`, `vector` и `boolean_op` из патча узла другого типа
    // (`PageDoc::set_props`).
    if let Some(text) = &p.text {
        text_codec::write_text_props(map, text)?;
    }
    if let Some(v) = &p.vector {
        map.insert(key::VECTOR, LoroValue::Binary(v.encode().into()))?;
    }
    if let Some(op) = p.boolean_op {
        map.insert(key::BOOLEAN_OP, op.as_str())?;
    }
    Ok(())
}

pub(crate) fn obj(fields: Vec<(&str, LoroValue)>) -> LoroValue {
    LoroValue::from(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<HashMap<_, _>>(),
    )
}

pub(crate) fn fill_value(f: &Fill) -> LoroValue {
    let mut fields = vec![
        (
            "kind",
            LoroValue::from(match f.kind {
                FillKind::Solid => "solid",
                FillKind::Image => "image",
            }),
        ),
        ("color", LoroValue::from(f.color.to_hex())),
        (
            "scaleMode",
            LoroValue::from(match f.scale_mode {
                ScaleMode::Fill => "fill",
                ScaleMode::Fit => "fit",
                ScaleMode::Tile => "tile",
            }),
        ),
        ("opacity", LoroValue::from(f.opacity)),
        ("visible", LoroValue::from(f.visible)),
    ];
    if let Some(h) = &f.image_hash {
        fields.push(("imageHash", LoroValue::from(h.as_str())));
    }
    obj(fields)
}

fn stroke_value(s: &Stroke) -> LoroValue {
    obj(vec![
        ("color", LoroValue::from(s.color.to_hex())),
        ("width", LoroValue::from(s.width)),
        (
            "align",
            LoroValue::from(match s.align {
                StrokeAlign::Inside => "inside",
                StrokeAlign::Center => "center",
                StrokeAlign::Outside => "outside",
            }),
        ),
    ])
}
