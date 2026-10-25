//! Дерево слоёв для панели (этап 5). Основной поток не держит дерево: он просит
//! окно видимых строк (`start`, `count`), а worker разворачивает дерево с учётом
//! раскрытых узлов и поиска и отдаёт только эти строки. Список строк страницы
//! (узел и глубина) строится заново лишь при изменении структуры — правке
//! документа, другой странице, раскрытии, поиске, — и стоит O(строк), а не
//! O(узлов страницы): свёрнутый фрейм на 100 узлов — одна строка. Поиск обходит
//! страницу целиком, но только при изменении запроса или документа.
//!
//! Строки идут сверху вниз, как в Figma: верхний (передний) узел первым.

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

/// Наибольшее окно строк за один запрос: панель высотой в экран — около 40 строк.
pub const MAX_LAYER_WINDOW: u32 = 200;
/// Наибольшая длина поискового запроса в символах.
pub const MAX_FILTER_CHARS: usize = 128;
/// Длина имени в строке слоя: имя из файла может быть любым.
pub const MAX_ROW_NAME_CHARS: usize = 256;
/// У скольких выделенных узлов раскрываются предки, когда выделение пришло с холста.
pub const REVEAL_MAX: usize = 100;

/// Тип узла для иконки строки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum NodeType {
    Frame,
    Group,
    Rect,
    Ellipse,
    Line,
    Image,
}

impl From<NodeKind> for NodeType {
    fn from(k: NodeKind) -> NodeType {
        match k {
            NodeKind::Frame => NodeType::Frame,
            NodeKind::Group => NodeType::Group,
            NodeKind::Rect => NodeType::Rect,
            NodeKind::Ellipse => NodeType::Ellipse,
            NodeKind::Line => NodeType::Line,
            NodeKind::Image => NodeType::Image,
        }
    }
}

/// Строка панели слоёв — всё, что нужно для отрисовки без обращения к документу.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct LayerRow {
    pub id: String,
    pub name: String,
    pub kind: NodeType,
    pub depth: u32,
    pub has_children: bool,
    pub expanded: bool,
    pub selected: bool,
    /// Выделен предок: строка подсвечена слабее, как в Figma.
    pub parent_selected: bool,
    pub visible: bool,
    pub locked: bool,
    /// Скрыт или заблокирован предок: значок строки тусклый.
    pub ancestor_hidden: bool,
    pub ancestor_locked: bool,
}

/// Окно строк для события `Layers`.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerWindow {
    pub start: u32,
    pub total: u32,
    pub rows: Vec<LayerRow>,
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[derive(Debug, Default)]
pub struct Layers {
    expanded: HashSet<NodeId>,
    /// Запрос поиска в нижнем регистре; пустой — без поиска.
    filter: String,
    rows: Vec<(Slot, u32)>,
    /// Список строк устарел (структура, раскрытие, поиск).
    stale: bool,
    start: u32,
    count: u32,
    last: Option<LayerWindow>,
    /// Начало диапазона для выделения с Shift.
    anchor: Option<NodeId>,
}

impl Layers {
    pub fn new() -> Layers {
        Layers {
            stale: true,
            ..Default::default()
        }
    }

    /// Структура страницы изменилась (правка, отмена, другая страница).
    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    /// Новая страница или документ: раскрытие и поиск сбрасываются.
    pub fn reset(&mut self) {
        self.expanded.clear();
        self.filter.clear();
        self.anchor = None;
        self.last = None;
        self.stale = true;
    }

    /// Окно, которое видит панель. Больше `MAX_LAYER_WINDOW` строк не отдаётся.
    pub fn set_window(&mut self, start: u32, count: u32) {
        self.start = start;
        self.count = count.min(MAX_LAYER_WINDOW);
    }

    pub fn set_expanded(&mut self, id: NodeId, expanded: bool) {
        let changed = if expanded {
            self.expanded.insert(id)
        } else {
            self.expanded.remove(&id)
        };
        self.stale |= changed;
    }

    /// Поиск по имени (без учёта регистра): показываются совпавшие узлы и их
    /// предки, всё раскрыто.
    pub fn set_filter(&mut self, query: &str) {
        let q = clip(query.trim(), MAX_FILTER_CHARS).to_lowercase();
        if q != self.filter {
            self.filter = q;
            self.stale = true;
        }
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn set_anchor(&mut self, id: Option<NodeId>) {
        self.anchor = id;
    }

    pub fn anchor(&self) -> Option<NodeId> {
        self.anchor
    }

    fn ensure(&mut self, scene: &Scene) {
        if !self.stale {
            return;
        }
        self.stale = false;
        // Узлы, удалённые из документа, не держат место в множестве.
        self.expanded.retain(|id| scene.slot(*id).is_some());
        self.rows.clear();
        if self.filter.is_empty() {
            self.flatten(scene);
        } else {
            self.flatten_filtered(scene);
        }
    }

    fn flatten(&mut self, scene: &Scene) {
        // Обход в глубину без рекурсии, сверху вниз: дети в стек в прямом порядке,
        // поэтому верхний (последний) ребёнок снимается первым.
        let mut stack: Vec<(Slot, u32)> = scene.roots().iter().map(|s| (*s, 0)).collect();
        while let Some((s, depth)) = stack.pop() {
            let n = scene.at(s);
            if scene.is_draft(n.id) {
                continue;
            }
            self.rows.push((s, depth));
            if !n.children.is_empty() && self.expanded.contains(&n.id) {
                stack.extend(n.children.iter().map(|c| (*c, depth + 1)));
            }
        }
    }

    fn flatten_filtered(&mut self, scene: &Scene) {
        // Сначала — какие узлы показать: совпавшие и все их предки.
        let mut keep: HashSet<Slot> = HashSet::new();
        let mut stack: Vec<Slot> = scene.roots().to_vec();
        while let Some(s) = stack.pop() {
            let n = scene.at(s);
            if !scene.is_draft(n.id) && n.props.name.to_lowercase().contains(&self.filter) {
                let mut cur = Some(s);
                while let Some(c) = cur {
                    if !keep.insert(c) {
                        break;
                    }
                    cur = scene.at(c).parent;
                }
            }
            stack.extend(n.children.iter().copied());
        }
        let mut stack: Vec<(Slot, u32)> = scene
            .roots()
            .iter()
            .filter(|s| keep.contains(s))
            .map(|s| (*s, 0))
            .collect();
        while let Some((s, depth)) = stack.pop() {
            self.rows.push((s, depth));
            stack.extend(
                scene
                    .at(s)
                    .children
                    .iter()
                    .filter(|c| keep.contains(c))
                    .map(|c| (*c, depth + 1)),
            );
        }
    }

    /// Всего строк (для высоты прокрутки).
    pub fn total(&mut self, scene: &Scene) -> u32 {
        self.ensure(scene);
        self.rows.len() as u32
    }

    /// Идентификаторы строк по порядку от `a` до `b` включительно (выделение с
    /// Shift). Узла нет среди строк — только `b`.
    pub fn range(&mut self, scene: &Scene, a: NodeId, b: NodeId) -> Vec<NodeId> {
        self.ensure(scene);
        let pos = |id: NodeId| self.rows.iter().position(|(s, _)| scene.at(*s).id == id);
        match (pos(a), pos(b)) {
            (Some(i), Some(j)) => {
                let (i, j) = (i.min(j), i.max(j));
                self.rows[i..=j]
                    .iter()
                    .map(|(s, _)| scene.at(*s).id)
                    .collect()
            }
            _ => vec![b],
        }
    }

    /// Раскрывает предков первых `REVEAL_MAX` выделенных (выделение с холста) и
    /// возвращает строку первого из них — панель прокрутит к ней.
    pub fn reveal(&mut self, scene: &Scene, ids: &[NodeId]) -> Option<u32> {
        for id in ids.iter().take(REVEAL_MAX) {
            let mut cur = scene.slot(*id).and_then(|s| scene.at(s).parent);
            while let Some(p) = cur {
                let pid = scene.at(p).id;
                if self.expanded.insert(pid) {
                    self.stale = true;
                }
                cur = scene.at(p).parent;
            }
        }
        let first = scene.slot(*ids.first()?)?;
        self.ensure(scene);
        self.rows
            .iter()
            .position(|(s, _)| *s == first)
            .map(|i| i as u32)
    }

    /// Строки текущего окна; `None` — те же, что ушли в прошлый раз.
    pub fn window(&mut self, scene: &Scene, selected: &HashSet<NodeId>) -> Option<LayerWindow> {
        self.ensure(scene);
        let total = self.rows.len() as u32;
        let start = self.start.min(total);
        let end = start.saturating_add(self.count).min(total);
        let searching = !self.filter.is_empty();
        let rows = self.rows[start as usize..end as usize]
            .iter()
            .map(|(s, depth)| {
                let n = scene.at(*s);
                let (mut parent_selected, mut ancestor_hidden, mut ancestor_locked) =
                    (false, false, false);
                let mut cur = n.parent;
                while let Some(p) = cur {
                    let pn = scene.at(p);
                    parent_selected |= selected.contains(&pn.id);
                    ancestor_hidden |= !pn.props.visible;
                    ancestor_locked |= pn.props.locked;
                    cur = pn.parent;
                }
                let has_children = n.children.iter().any(|c| !scene.is_draft(scene.at(*c).id));
                LayerRow {
                    id: n.id.to_string(),
                    name: clip(&n.props.name, MAX_ROW_NAME_CHARS),
                    kind: n.props.kind.into(),
                    depth: *depth,
                    has_children,
                    expanded: has_children && (searching || self.expanded.contains(&n.id)),
                    selected: selected.contains(&n.id),
                    parent_selected,
                    visible: n.props.visible,
                    locked: n.props.locked,
                    ancestor_hidden,
                    ancestor_locked,
                }
            })
            .collect();
        let w = LayerWindow { start, total, rows };
        if self.last.as_ref() == Some(&w) {
            return None;
        }
        self.last = Some(w.clone());
        Some(w)
    }

    /// Следующее окно уйдёт, даже если не изменилось (панель запросила заново).
    pub fn resend(&mut self) {
        self.last = None;
    }
}
