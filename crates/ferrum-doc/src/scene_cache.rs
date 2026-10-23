//! Кэш сцены: плоская таблица всех узлов страницы для мгновенного открытия.
//!
//! Раскодировать снапшот Loro на 100 000 узлов — больше секунды. Поэтому при сохранении
//! рядом со снапшотом пишется эта таблица: по ней холст рисуется сразу, а Loro
//! раскодируется в фоне. Таблица — производные данные: при несовпадении со снапшотом
//! она просто игнорируется.

use loro::TreeID;

use crate::binary::{Reader, Writer};
use crate::types::*;

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

fn kind_code(k: NodeKind) -> u8 {
    match k {
        NodeKind::Frame => 0,
        NodeKind::Group => 1,
        NodeKind::Rect => 2,
        NodeKind::Ellipse => 3,
        NodeKind::Line => 4,
        NodeKind::Image => 5,
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
        w.u8(flags);
        for v in [p.x, p.y, p.w, p.h, p.rotation, p.opacity, p.corner_radius] {
            w.f64(v);
        }
        w.str16(&p.name)?;
        w.u8(u8::try_from(p.fills.len()).ok()?);
        for f in &p.fills {
            w.u8(match f.kind {
                FillKind::Solid => 0,
                FillKind::Image => 1,
            });
            color(&mut w, f.color);
            w.u8(match f.scale_mode {
                ScaleMode::Fill => 0,
                ScaleMode::Fit => 1,
                ScaleMode::Tile => 2,
            });
            w.f64(f.opacity);
            w.u8(f.visible as u8);
            w.str16(f.image_hash.as_deref().unwrap_or(""))?;
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
    }
    Some(w.buf)
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
            let kind = if r.u8()? == 1 {
                FillKind::Image
            } else {
                FillKind::Solid
            };
            let color = read_color(&mut r)?;
            let scale_mode = match r.u8()? {
                1 => ScaleMode::Fit,
                2 => ScaleMode::Tile,
                _ => ScaleMode::Fill,
            };
            let opacity = r.f64()?;
            let visible = r.u8()? != 0;
            let hash = r.str16()?;
            fills.push(Fill {
                kind,
                color,
                image_hash: (!hash.is_empty()).then_some(hash),
                scale_mode,
                opacity,
                visible,
            });
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
                fills,
                strokes,
            },
        });
    }
    Some(rows)
}
