//! Дерево слоёв для панели (этап 5). Основной поток не держит дерево: он просит
//! окно видимых строк (`start`, `count`), а worker разворачивает дерево с учётом
//! раскрытых узлов и поиска и отдаёт только эти строки. Список строк страницы
//! (узел и глубина) строится заново лишь при изменении структуры — правке
//! документа, другой странице, раскрытии, поиске, — и стоит O(строк), а не
//! O(узлов страницы): свёрнутый фрейм на 100 узлов — одна строка. Поиск обходит
//! страницу целиком, но только при изменении запроса или документа.
//!
//! Кэш строк сам устаревает: он помечен `Scene::structure_stamp` (состав, имена,
//! новая сцена), а каждая строка хранит и слот, и `NodeId`, поэтому на чтении
//! слот сверяется с узлом без паники. Метка не меняется от превью геометрии и
//! черновиков, так что перетаскивание не пересобирает строки.
//!
//! Поиск: имена в нижнем регистре кэшируются по слотам (первые `SEARCH_NAME_CHARS`
//! символов), следующий запрос, продолжающий предыдущий, просматривает только его
//! совпадения. `prepare_search` строит кэш заранее.
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
/// Сколько первых символов имени участвует в поиске: имя из файла может быть
/// мегабайтным, и разбирать его целиком на каждый кэш нельзя.
pub const SEARCH_NAME_CHARS: usize = 256;

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

/// Строка списка: слот для быстрого доступа, идентификатор для проверки слота.
type Row = (Slot, NodeId, u32);

#[derive(Debug, Default)]
pub struct Layers {
    expanded: HashSet<NodeId>,
    /// Запрос поиска в нижнем регистре; пустой — без поиска.
    filter: String,
    rows: Vec<Row>,
    /// Список строк устарел (структура, раскрытие, поиск).
    stale: bool,
    /// Метка сцены, по которой построены строки и кэши поиска.
    stamp: Option<u64>,
    /// Имена в нижнем регистре по слотам; живёт, пока не сменится метка сцены.
    lower: Vec<Option<Box<str>>>,
    /// В `lower` есть имена всех узлов страницы: полный проход — линейный просмотр
    /// кэша, без обхода дерева.
    lower_full: bool,
    /// Совпавшие узлы последнего поиска и запрос, для которого они найдены.
    matches: Vec<Slot>,
    matches_query: String,
    /// Показываемые при поиске узлы (совпавшие и предки) по слотам.
    kept: Vec<bool>,
    start: u32,
    count: u32,
    last: Option<LayerWindow>,
    /// Начало диапазона для выделения с Shift.
    anchor: Option<NodeId>,
}

fn lowered<'a>(lower: &'a mut Vec<Option<Box<str>>>, scene: &Scene, slot: Slot) -> &'a str {
    let i = slot as usize;
    if lower.len() <= i {
        lower.resize_with(i + 1, || None);
    }
    lower[i].get_or_insert_with(|| {
        clip(&scene.at(slot).props.name, SEARCH_NAME_CHARS)
            .to_lowercase()
            .into_boxed_str()
    })
}

impl Layers {
    pub fn new() -> Layers {
        Layers {
            stale: true,
            ..Default::default()
        }
    }

    /// Структура страницы изменилась (правка, отмена, другая страница). Вызывать
    /// необязательно — кэш сверяется с меткой сцены, — но вызов всё сбрасывает.
    pub fn invalidate(&mut self) {
        self.stamp = None;
        self.stale = true;
    }

    /// Новая страница или документ: раскрытие и поиск сбрасываются.
    pub fn reset(&mut self) {
        self.expanded.clear();
        self.filter.clear();
        self.anchor = None;
        self.last = None;
        self.invalidate();
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
    /// предки, всё раскрыто. Запрос сначала обрезается, потом очищается от пробелов.
    pub fn set_filter(&mut self, query: &str) {
        let q = clip(query, MAX_FILTER_CHARS).trim().to_lowercase();
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

    /// Сверяет кэши с меткой сцены; чужая или новая метка сбрасывает всё.
    fn sync(&mut self, scene: &Scene) {
        let stamp = scene.structure_stamp();
        if self.stamp != Some(stamp) {
            self.stamp = Some(stamp);
            self.stale = true;
            self.lower.clear();
            self.lower_full = false;
            self.matches.clear();
            self.matches_query.clear();
            self.kept.clear();
        }
    }

    /// Строит кэш имён заранее (например, когда поле поиска получило фокус), чтобы
    /// первый запрос не обходил всю страницу.
    pub fn prepare_search(&mut self, scene: &Scene) {
        self.sync(scene);
        if self.lower_full {
            return;
        }
        let mut stack: Vec<Slot> = scene.roots().to_vec();
        while let Some(s) = stack.pop() {
            if scene.is_draft_slot(s) {
                continue;
            }
            lowered(&mut self.lower, scene, s);
            stack.extend(scene.at(s).children.iter().copied());
        }
        self.lower_full = true;
    }

    fn ensure(&mut self, scene: &Scene) {
        self.sync(scene);
        if !self.stale {
            return;
        }
        self.stale = false;
        // Узлы, удалённые из документа, не держат место в множестве.
        self.expanded.retain(|id| scene.slot(*id).is_some());
        self.rows.clear();
        if self.filter.is_empty() {
            self.matches.clear();
            self.matches_query.clear();
            self.kept.clear();
            self.flatten(scene);
        } else {
            self.search(scene);
            self.flatten_filtered(scene);
        }
    }

    fn flatten(&mut self, scene: &Scene) {
        // Обход в глубину без рекурсии, сверху вниз: дети в стек в прямом порядке,
        // поэтому верхний (последний) ребёнок снимается первым.
        let mut stack: Vec<(Slot, u32)> = scene.roots().iter().map(|s| (*s, 0)).collect();
        while let Some((s, depth)) = stack.pop() {
            if scene.is_draft_slot(s) {
                continue;
            }
            let n = scene.at(s);
            self.rows.push((s, n.id, depth));
            if !n.children.is_empty() && self.expanded.contains(&n.id) {
                stack.extend(n.children.iter().map(|c| (*c, depth + 1)));
            }
        }
    }

    /// Находит совпавшие узлы. Запрос, продолжающий прошлый, просматривает только
    /// прошлые совпадения; иначе — вся страница по кэшу имён.
    fn search(&mut self, scene: &Scene) {
        if self.matches_query.is_empty() || !self.filter.starts_with(&self.matches_query) {
            self.prepare_search(scene);
        }
        let mut lower = std::mem::take(&mut self.lower);
        let mut found = Vec::new();
        let narrow = !self.matches_query.is_empty() && self.filter.starts_with(&self.matches_query);
        if narrow {
            for &s in &self.matches {
                if lowered(&mut lower, scene, s).contains(self.filter.as_str()) {
                    found.push(s);
                }
            }
        } else {
            // Кэш полон: просмотр по слотам, без обращения к узлам сцены.
            for (i, name) in lower.iter().enumerate() {
                if name
                    .as_deref()
                    .is_some_and(|n| n.contains(self.filter.as_str()))
                {
                    found.push(i as Slot);
                }
            }
        }
        // Показываем совпавшие и всех их предков.
        self.kept.clear();
        self.kept.resize(lower.len(), false);
        for &s in &found {
            let mut cur = Some(s);
            while let Some(c) = cur {
                let k = &mut self.kept[c as usize];
                if *k {
                    break;
                }
                *k = true;
                cur = scene.at(c).parent;
            }
        }
        self.lower = lower;
        self.matches = found;
        self.matches_query = self.filter.clone();
    }

    fn is_kept(&self, slot: Slot) -> bool {
        self.kept.get(slot as usize).copied().unwrap_or(false)
    }

    fn flatten_filtered(&mut self, scene: &Scene) {
        let mut stack: Vec<(Slot, u32)> = scene
            .roots()
            .iter()
            .filter(|s| self.is_kept(**s))
            .map(|s| (*s, 0))
            .collect();
        while let Some((s, depth)) = stack.pop() {
            let n = scene.at(s);
            self.rows.push((s, n.id, depth));
            stack.extend(
                n.children
                    .iter()
                    .filter(|c| self.is_kept(**c))
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
        let pos = |id: NodeId| self.rows.iter().position(|(_, rid, _)| *rid == id);
        match (pos(a), pos(b)) {
            (Some(i), Some(j)) => {
                let (i, j) = (i.min(j), i.max(j));
                self.rows[i..=j].iter().map(|(_, id, _)| *id).collect()
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
        let first = *ids.first()?;
        self.ensure(scene);
        self.rows
            .iter()
            .position(|(_, id, _)| *id == first)
            .map(|i| i as u32)
    }

    /// Узел строки, если слот ещё указывает на тот же узел.
    fn live<'a>(scene: &'a Scene, row: &Row) -> Option<&'a ferrum_scene::SceneNode> {
        scene.node_at(row.0).filter(|n| n.id == row.1)
    }

    /// Строки текущего окна; `None` — те же, что ушли в прошлый раз.
    pub fn window(&mut self, scene: &Scene, selected: &HashSet<NodeId>) -> Option<LayerWindow> {
        self.ensure(scene);
        let range = |l: &Layers| {
            let total = l.rows.len() as u32;
            let start = l.start.min(total);
            (start, start.saturating_add(l.count).min(total))
        };
        let (start, end) = range(self);
        // Метка сцены не поймала расхождения (не должно быть): один раз
        // пересобираем, а оставшиеся мёртвые строки пропускаем.
        if self.rows[start as usize..end as usize]
            .iter()
            .any(|r| Self::live(scene, r).is_none())
        {
            self.stale = true;
            self.ensure(scene);
        }
        let total = self.rows.len() as u32;
        let (start, end) = range(self);
        let searching = !self.filter.is_empty();
        let rows = self.rows[start as usize..end as usize]
            .iter()
            .filter_map(|row| {
                let n = Self::live(scene, row)?;
                let (mut parent_selected, mut ancestor_hidden, mut ancestor_locked) =
                    (false, false, false);
                let mut cur = n.parent;
                while let Some(p) = cur {
                    let Some(pn) = scene.node_at(p) else { break };
                    parent_selected |= selected.contains(&pn.id);
                    ancestor_hidden |= !pn.props.visible;
                    ancestor_locked |= pn.props.locked;
                    cur = pn.parent;
                }
                let has_children = n.children.iter().any(|c| !scene.is_draft_slot(*c));
                let open = if searching {
                    n.children.iter().any(|c| self.is_kept(*c))
                } else {
                    self.expanded.contains(&n.id)
                };
                Some(LayerRow {
                    id: n.id.to_string(),
                    name: clip(&n.props.name, MAX_ROW_NAME_CHARS),
                    kind: n.props.kind.into(),
                    depth: row.2,
                    has_children,
                    expanded: has_children && open,
                    selected: selected.contains(&n.id),
                    parent_selected,
                    visible: n.props.visible,
                    locked: n.props.locked,
                    ancestor_hidden,
                    ancestor_locked,
                })
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
