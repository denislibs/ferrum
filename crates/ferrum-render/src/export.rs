//! Общее у экспорта PNG, SVG и PDF: какие узлы и какая область, пределы размера и
//! времени (спека подсистемы 3, раздел 11: экспорт недоверенного документа не
//! должен съесть память и время worker'а).

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};
use kurbo::Rect;

use crate::clock::Clock;
use crate::ExportError;

/// Наибольший файл экспорта SVG и PDF: 256 МБ (как предел байтов картинки
/// документа × 4). Больше — отказ с сообщением, а не исчерпание памяти worker'а.
pub const MAX_EXPORT_BYTES: usize = 256 * 1024 * 1024;
/// Наибольшее время экспорта SVG и PDF, мс. Дольше — отказ: worker не должен
/// стоять минутами (интерфейс при этом не замирает — экспорт идёт в worker'е).
pub const EXPORT_TIME_MS: f64 = 30_000.0;

/// Пределы одного экспорта: байты и время (часы — от вызывающего: в wasm32
/// `Instant` нет).
pub struct ExportBudget<'a> {
    pub max_bytes: usize,
    pub clock: Option<&'a dyn Clock>,
    /// Конец срока по часам `clock` (мс).
    pub deadline_ms: f64,
}

impl<'a> ExportBudget<'a> {
    /// Пределы по умолчанию: `MAX_EXPORT_BYTES` и `EXPORT_TIME_MS` от «сейчас».
    pub fn new(clock: Option<&'a dyn Clock>) -> ExportBudget<'a> {
        let now = clock.map_or(0.0, |c| c.now_ms());
        ExportBudget {
            max_bytes: MAX_EXPORT_BYTES,
            clock,
            deadline_ms: now + EXPORT_TIME_MS,
        }
    }

    /// Без пределов (тесты).
    pub fn unlimited() -> ExportBudget<'static> {
        ExportBudget {
            max_bytes: usize::MAX,
            clock: None,
            deadline_ms: f64::INFINITY,
        }
    }

    pub(crate) fn check(&self, bytes: usize) -> Result<(), ExportError> {
        if bytes > self.max_bytes {
            return Err(ExportError::TooBig);
        }
        if let Some(c) = self.clock {
            if c.now_ms() > self.deadline_ms {
                return Err(ExportError::Timeout);
            }
        }
        Ok(())
    }
}

/// Что экспортировать: узлы верхнего уровня (без потомков других выбранных) в
/// порядке отрисовки и их общая область — форма фрейма (за его краем всё
/// обрезается) или границы узла с обводкой и потомками. Неизвестные и скрытые
/// узлы пропускаются; если не осталось ни одного — ошибка.
pub(crate) fn export_items(
    scene: &Scene,
    ids: &[NodeId],
) -> Result<(Vec<(Slot, Rect)>, Rect), ExportError> {
    let mut items = Vec::with_capacity(ids.len());
    let mut known = false;
    for id in ids {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        known = true;
        let n = scene.at(slot);
        // Скрытый узел (и узел без видимого содержимого) картинки не даёт. Скрытые
        // предки здесь не проверяются: скрытый родитель скрывает всё поддерево,
        // так что `subtree_bounds` у потомка уже пусто.
        if n.subtree_bounds.is_none() {
            continue;
        }
        if let Some(own) = node_area(scene, slot) {
            items.push((slot, own));
        }
    }
    if !known {
        return Err(ExportError::NotFound);
    }
    // Узел, чей предок тоже экспортируется, уже нарисован вместе с предком:
    // второй раз он лёг бы поверх (полупрозрачный — темнее). Область считаем
    // только по оставшимся: потомок, выступающий за обрезающий фрейм, холст
    // не расширяет.
    let chosen: HashSet<_> = items.iter().map(|(s, _)| *s).collect();
    items.retain(|&(s, _)| {
        let mut cur = scene.at(s).parent;
        while let Some(p) = cur {
            if chosen.contains(&p) {
                return false;
            }
            cur = scene.at(p).parent;
        }
        true
    });
    let area = items
        .iter()
        .map(|&(_, r)| r)
        .reduce(|a, b| a.union(b))
        .ok_or(ExportError::Empty)?;
    // Порядок отрисовки: ключ — позиции от корня, без повторов.
    items.sort_by_cached_key(|(s, _)| scene.order_key(*s));
    items.dedup_by_key(|(s, _)| *s);
    Ok((items, area))
}

/// Область узла для экспорта: у фрейма — его форма, у остальных — границы с
/// обводкой и потомками.
pub(crate) fn node_area(scene: &Scene, slot: Slot) -> Option<Rect> {
    let n = scene.at(slot);
    match (n.props.kind, n.local_bounds) {
        (NodeKind::Frame, Some(b)) => Some(n.world.transform_rect_bbox(b)),
        _ => match (n.world_bounds, n.subtree_bounds) {
            (Some(a), Some(b)) => Some(a.union(b)),
            (a, b) => a.or(b),
        },
    }
}
