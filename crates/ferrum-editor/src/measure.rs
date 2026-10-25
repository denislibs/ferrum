//! Измерение расстояний с Alt, как в Figma (разделы 7.3 и 8.6 спеки): пока Alt
//! нажат, выделение не пустое и жеста нет, редактор отдаёт в оверлей отрезки от
//! рамки выделения S до наведённого объекта T или до родительского фрейма.
//!
//! Геометрия — рамки в мире (у повёрнутых тоже, как в Figma), без обводок, как у
//! бейджа «W × H». Отрезки всегда идут от края S к краю T; значения — в единицах
//! документа, округлённые до сотых, как в панели свойств; нулевые не рисуются.
//! Рендер рисует их в экранных пикселях (`ferrum_render::Measure`).

use std::collections::{HashMap, HashSet};

use ferrum_doc::{NodeId, NodeKind};
use ferrum_render::{Measure, MeasureKind};
use ferrum_scene::{Scene, Slot};
use kurbo::{Line, Rect};

use crate::geometry::world_box;
use crate::input::{KeyEvent, Modifiers, Platform};

/// Значение до сотых, как в панели свойств и бейдже размера.
pub fn round_value(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Отрезки перекрываются с положительной длиной общей части (касание — нет).
fn overlaps(a0: f64, a1: f64, b0: f64, b1: f64) -> bool {
    a0 < b1 && b0 < a1
}

/// Добавляет расстояние, если округлённое значение не ноль, и возвращает, добавлено ли.
fn distance(out: &mut Vec<Measure>, line: Line) -> bool {
    let value = round_value(line.length());
    if value == 0.0 || !value.is_finite() {
        return false;
    }
    out.push(Measure {
        line,
        value,
        kind: MeasureKind::Distance,
    });
    true
}

fn guide(out: &mut Vec<Measure>, line: Line) {
    if line.length() > 0.0 {
        out.push(Measure {
            line,
            value: 0.0,
            kind: MeasureKind::Guide,
        });
    }
}

/// Измерения от рамки выделения `s` до рамки объекта `t` (обе — в мире).
///
/// - Рамки пересекаются (одна внутри другой или частично): расстояния между
///   одноимёнными краями — слева, справа, сверху, снизу; линия — посередине общей
///   части рамок по другой оси (у вложенных — по центру внутренней).
/// - Не пересекаются: по каждой оси, где между ними зазор, — зазор между
///   ближайшими краями. Если по другой оси рамки перекрываются, линия — посередине
///   перекрытия; иначе — от центра S, и пунктирная направляющая вдоль края T от
///   конца линии до самого T.
pub fn measure(s: Rect, t: Rect) -> Vec<Measure> {
    let mut out = Vec::new();
    let ox = overlaps(s.x0, s.x1, t.x0, t.x1);
    let oy = overlaps(s.y0, s.y1, t.y0, t.y1);
    // Середины перекрытия по осям (имеют смысл, только если оно есть).
    let mx = (s.x0.max(t.x0) + s.x1.min(t.x1)) / 2.0;
    let my = (s.y0.max(t.y0) + s.y1.min(t.y1)) / 2.0;
    if ox && oy {
        distance(&mut out, Line::new((s.x0, my), (t.x0, my)));
        distance(&mut out, Line::new((s.x1, my), (t.x1, my)));
        distance(&mut out, Line::new((mx, s.y0), (mx, t.y0)));
        distance(&mut out, Line::new((mx, s.y1), (mx, t.y1)));
        return out;
    }
    let c = s.center();
    if !ox {
        // Зазор по x: от ближнего края S до ближнего края T.
        let (a, b) = if s.x1 <= t.x0 {
            (s.x1, t.x0)
        } else {
            (s.x0, t.x1)
        };
        let y = if oy { my } else { c.y };
        // Направляющая — только если линия проходит мимо края T (у S нулевой высоты
        // на уровне края T линия и так приходит в его угол).
        if distance(&mut out, Line::new((a, y), (b, y))) && (y < t.y0 || y > t.y1) {
            let edge = if y < t.y0 { t.y0 } else { t.y1 };
            guide(&mut out, Line::new((b, y), (b, edge)));
        }
    }
    if !oy {
        let (a, b) = if s.y1 <= t.y0 {
            (s.y1, t.y0)
        } else {
            (s.y0, t.y1)
        };
        let x = if ox { mx } else { c.x };
        if distance(&mut out, Line::new((x, a), (x, b))) && (x < t.x0 || x > t.x1) {
            let edge = if x < t.x0 { t.x0 } else { t.x1 };
            guide(&mut out, Line::new((x, b), (edge, b)));
        }
    }
    out
}

/// Нажат ли Alt для измерений. Ведётся и по клавишам, и по модификаторам
/// указателя: потерянное нажатие или отпускание (фокус был в поле, окно
/// переключали) исправляет первое же движение мыши. AltGr на Windows и Linux
/// измерений не включает: он приходит как Ctrl+Alt, и указатель его от Alt не
/// отличает, поэтому признак запоминается по последней клавише.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AltState {
    held: bool,
    alt_graph: bool,
}

impl AltState {
    pub fn held(self) -> bool {
        self.held
    }

    pub fn key(&mut self, e: &KeyEvent, platform: Platform) {
        // На macOS признак AltGraph не учитывается (раздел 7.6): Option там иногда
        // приходит как AltGraph.
        self.alt_graph = platform != Platform::Mac && e.alt_graph && e.modifiers.alt;
        self.held = e.modifiers.alt && !self.alt_graph;
    }

    pub fn pointer(&mut self, m: Modifiers) {
        if !m.alt {
            *self = AltState::default();
        } else if !self.alt_graph {
            self.held = true;
        }
    }

    pub fn clear(&mut self) {
        *self = AltState::default();
    }
}

/// Что измерения берут из выделения: его рамку в мире, множество выделенных и
/// рамку родительского фрейма. Посчитать их — O(размера выделения), поэтому они
/// кэшируются по номеру сцены, её счётчику изменений и версии выделения: на
/// движение указателя остаётся O(1).
#[derive(Debug, Clone)]
pub(crate) struct MeasureCache {
    key: (u64, u64, u64),
    /// Общая рамка выделенных в мире; `None` — у выделенных нет границ.
    pub sbox: Option<Rect>,
    pub selected: HashSet<NodeId>,
    /// Рамка ближайшего фрейма-предка общего родителя выделенных; `None` — корень.
    pub parent: Option<Rect>,
}

impl MeasureCache {
    pub fn key(&self) -> (u64, u64, u64) {
        self.key
    }

    pub fn new(key: (u64, u64, u64), scene: &Scene, selection: &[NodeId]) -> MeasureCache {
        let slots: Vec<Slot> = selection.iter().filter_map(|id| scene.slot(*id)).collect();
        let sbox = slots
            .iter()
            .filter_map(|s| world_box(scene, *s))
            .reduce(|a, b| a.union(b));
        MeasureCache {
            key,
            sbox,
            selected: selection.iter().copied().collect(),
            parent: common_parent(scene, &slots)
                .and_then(|p| frame_at_or_above(scene, p))
                .and_then(|f| world_box(scene, f)),
        }
    }
}

/// Предки узла снизу вверх, без него самого.
fn ancestors(scene: &Scene, slot: Slot) -> impl Iterator<Item = Slot> + '_ {
    std::iter::successors(scene.at(slot).parent, move |s| scene.at(*s).parent)
}

/// Ближайший общий предок родителей выделенных (`None` — корень страницы).
/// O(размера выделения × глубины), только при смене выделения или сцены.
fn common_parent(scene: &Scene, slots: &[Slot]) -> Option<Slot> {
    let (first, rest) = slots.split_first()?;
    // Предки первого узла снизу вверх; общий предок — один из них, и с каждым
    // следующим узлом он может только подняться (номер в цепочке растёт).
    let chain: Vec<Slot> = ancestors(scene, *first).collect();
    let index: HashMap<Slot, usize> = chain.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    let mut lca = 0;
    for s in rest {
        lca = ancestors(scene, *s).find_map(|a| index.get(&a).copied().filter(|i| *i >= lca))?;
    }
    chain.get(lca).copied()
}

/// `slot` или ближайший его предок-фрейм.
fn frame_at_or_above(scene: &Scene, slot: Slot) -> Option<Slot> {
    std::iter::once(slot)
        .chain(ancestors(scene, slot))
        .find(|s| scene.at(*s).props.kind == NodeKind::Frame)
}
