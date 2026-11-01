//! Таблица сцены: узлы страницы с мировыми трансформациями, границами и R-tree.
//!
//! Сцена — производный кэш документа. Она строится из строк кэша сцены (до
//! загрузки Loro) или из `PageDoc`, а дальше обновляется по `Changes`: пересчитываются
//! только затронутые узлы, их потомки (мировая трансформация) и предки (границы).

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use ferrum_doc::{Changes, NodeId, NodeKind, PageDoc, Props, PropsPatch, SceneRow};
use kurbo::{Affine, BezPath, Point, Rect};
use rstar::primitives::{GeomWithData, Rectangle};
use rstar::{RStarInsertionStrategy, RTree, RTreeParams, SelectionFunction, AABB};

use crate::boolean::{self as boolean, Contours};
use crate::shape::{self, Geometry};
use crate::vector::{self, PathShape};

/// Индекс узла в плотной таблице сцены. Освободившиеся индексы переиспользуются,
/// поэтому хранить их дольше одного кадра нельзя — для этого есть `NodeId`.
pub type Slot = u32;

type Entry = GeomWithData<Rectangle<[f64; 2]>, Slot>;

/// С какого числа удалений из R-tree выгоднее один проход по всему дереву.
const TREE_DRAIN_THRESHOLD: usize = 200;

/// Номера версий общие для всего процесса. Если бы каждая сцена считала их сама,
/// сцена, построенная заново (после перестройки или подмены сцены из кэша загруженной
/// сценой), выдала бы номера, которые уже видел рендер, и его кэш принял бы старое
/// за новое. Атомарный счётчик выдаёт каждый номер один раз.
static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

fn fresh_version() -> u64 {
    // Порядок не нужен: уникальность номеров даёт сама атомарная операция.
    NEXT_VERSION.fetch_add(1, Ordering::Relaxed)
}

/// Выборка записей R-tree по пометкам их слотов — для удаления пачкой.
struct Marked<'a> {
    marks: &'a [u32],
    mark: u32,
}

impl SelectionFunction<Entry> for Marked<'_> {
    fn should_unpack_parent(&self, _: &AABB<[f64; 2]>) -> bool {
        true
    }

    fn should_unpack_leaf(&self, e: &Entry) -> bool {
        self.marks[e.data as usize] == self.mark
    }
}

/// Родитель и позиция узла в документе (`PageDoc::placement`).
type Placement = (Option<NodeId>, Option<usize>);

/// Узлы R-tree крупнее, чем по умолчанию (6): пакетное построение 100 000 записей
/// вдвое быстрее, удаление и вставка не дороже, поиск по точке тот же.
struct Params;

impl RTreeParams for Params {
    const MIN_SIZE: usize = 10;
    const MAX_SIZE: usize = 24;
    const REINSERTION_COUNT: usize = 2;
    type DefaultInsertionStrategy = RStarInsertionStrategy;
}

#[derive(Debug, Clone)]
pub struct SceneNode {
    pub id: NodeId,
    /// Свойства после проверки (`shape::sanitize`) и с учётом превью жеста.
    pub props: Props,
    pub parent: Option<Slot>,
    /// Дети снизу вверх, как в документе.
    pub children: Vec<Slot>,
    pub depth: u32,
    pub local: Affine,
    pub world: Affine,
    /// Границы формы в локальных координатах без обводки; у группы — объединение
    /// видимых детей. `None` — пустая группа.
    pub local_bounds: Option<Rect>,
    /// Собственные границы в мире с обводкой — запись в R-tree.
    pub world_bounds: Option<Rect>,
    /// Границы всего видимого поддерева в мире (дети фрейма с обрезкой не выходят
    /// за фрейм); `None` — узел скрыт. По ним рендер отбрасывает невидимое.
    pub subtree_bounds: Option<Rect>,
    /// Растёт, когда меняется вид узла в его локальных координатах: тип, размер,
    /// скругление, заливки, обводки. По нему рендер понимает, что список
    /// отрисовки узла устарел; перемещение и поворот его не меняют.
    pub version: u64,
    /// Растёт при любом изменении в поддереве (свойства, состав, положение детей):
    /// по нему рендер понимает, что растровый кэш фрейма устарел.
    pub subtree_version: u64,
    /// Контуры обводки по контуру (`vector::path_shape`), построенные при первом
    /// запросе и общие для попадания, подсветки и списка отрисовки: сеть в
    /// 100 000 точек строится ~7 мс, и каждое движение указателя над ней их бы
    /// строило. Сбрасываются вместе с `version`.
    path: OnceLock<Option<Arc<PathShape>>>,
    /// Форма заливки узла-листа для булевой группы-предка (`boolean::leaf_fill`),
    /// построенная при первом запросе; сбрасывается вместе с `version`. Правка
    /// одного ребёнка большой булевой группы не дробит кривые остальных заново.
    fill: OnceLock<Arc<Contours>>,
    /// Результат булевой группы (у остальных узлов — `None`).
    pub boolean: Option<Arc<BooleanOutline>>,
    /// Ключ содержимого: версия листа или (у группы и булевой группы) тип,
    /// операция и ключи, положения и видимость детей. Результат булевой группы пересчитывается, только когда
    /// ключ сменился: сдвиг самой группы его не меняет.
    content: u64,
}

/// Результат булевой группы в её локальных координатах.
#[derive(Debug, Clone)]
pub struct BooleanOutline {
    pub contours: Contours,
    /// Форма для рендера и попадания: одна область и замкнутый путь обводки.
    pub shape: Arc<PathShape>,
    op: ferrum_doc::BooleanOp,
    /// Формы видимых детей в координатах группы с их ключами — для
    /// пересчёта только сменившихся.
    inputs: Vec<(u64, Arc<Contours>)>,
    /// Операция над всеми, кроме одного ребёнка (номер, вид, части), — пока
    /// жест двигает этого ребёнка.
    rest: Option<Arc<(usize, boolean::Rest, Vec<boolean::Piece>)>>,
    /// Посчитан по частям (жест над одним ребёнком и его запись).
    partial: bool,
}

impl BooleanOutline {
    /// Посчитан по частям — жест над одним ребёнком и его запись
    /// (`Scene::boolean_outline`).
    pub fn is_partial(&self) -> bool {
        self.partial
    }
}

impl SceneNode {
    pub fn geometry(&self) -> Geometry {
        shape::geometry(&self.props)
    }

    /// Контуры обводки по контуру — из кэша узла (`None` — узел обводится
    /// кольцами). У булевой группы — её результат.
    pub fn path_shape(&self) -> Option<&Arc<PathShape>> {
        if self.props.kind == NodeKind::Boolean {
            return self.boolean.as_ref().map(|b| &b.shape);
        }
        self.path
            .get_or_init(|| vector::path_shape(&self.props).map(Arc::new))
            .as_ref()
    }
}

/// Контуры глифов текстового узла в его локальных координатах для булевых групп
/// (сцена шрифтов не знает — их даёт редактор); `None` — шрифты ещё грузятся.
pub type TextOutlines = Rc<dyn Fn(&Props) -> Option<BezPath>>;

/// Временный узел жеста, которого нет в документе: фигура, которую рисуют, или копия
/// при перетаскивании с Alt. Живёт только в сцене до конца жеста.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// Идентификатор, которого нет в документе (редактор берёт зарезервированный).
    pub id: NodeId,
    /// Родитель: узел документа или черновик раньше в том же списке; `None` — корень.
    pub parent: Option<NodeId>,
    pub props: Props,
}

/// Узлы хранятся кусками постоянной ёмкости, а не одним `Vec`: перераспределение
/// массива на 100 000 узлов (~35 МБ) копирует его страницами с копированием при
/// записи, и первая запись в каждую страницу после этого стоит микросекунды —
/// создание одного узла превращалось в миллисекунды на следующей правке.
const CHUNK: usize = 4096;

#[derive(Default)]
struct Arena {
    chunks: Vec<Vec<Option<SceneNode>>>,
    len: usize,
}

impl Arena {
    fn get(&self, slot: Slot) -> &Option<SceneNode> {
        let s = slot as usize;
        &self.chunks[s / CHUNK][s % CHUNK]
    }

    fn get_mut(&mut self, slot: Slot) -> &mut Option<SceneNode> {
        let s = slot as usize;
        &mut self.chunks[s / CHUNK][s % CHUNK]
    }

    fn push(&mut self, node: SceneNode) -> Slot {
        if self.len.is_multiple_of(CHUNK) {
            self.chunks.push(Vec::with_capacity(CHUNK));
        }
        self.chunks
            .last_mut()
            .expect("кусок создан выше")
            .push(Some(node));
        self.len += 1;
        (self.len - 1) as Slot
    }
}

#[derive(Default)]
pub struct Scene {
    nodes: Arena,
    free: Vec<Slot>,
    by_id: HashMap<NodeId, Slot>,
    roots: Vec<Slot>,
    rtree: RTree<Entry, Params>,
    /// Узлы под превью жеста: свойства из документа и временная правка поверх них.
    preview: HashMap<Slot, (Props, PropsPatch)>,
    /// Изменения R-tree, накопленные за один пересчёт (см. `flush_tree`).
    tree_removals: Vec<(Slot, Rect)>,
    tree_inserts: Vec<(Slot, Rect)>,
    /// Позиция узла среди детей родителя (или корней страницы) по слотам. Отдельно
    /// от узлов: при вставке в начало списка позиции сдвигаются у всех соседей, и
    /// плотный массив здесь в разы быстрее записи в разбросанные по памяти узлы.
    index: Vec<u32>,
    /// Пометки для обходов без выделения памяти на каждый вызов.
    marks: Vec<u32>,
    mark: u32,
    /// Растёт при каждом изменении сцены (`apply`, превью): холст по нему понимает,
    /// что нужен новый кадр, не сравнивая сцены.
    changes: u64,
    /// Метка состава и имён: растёт на 1 при каждом `apply` и при превью имени.
    /// Превью геометрии и черновики её не меняют — по ней панель слоёв понимает,
    /// что кэш строк устарел.
    structure: u64,
    /// Номер этого экземпляра сцены (из `fresh_version`): новая сцена, в том числе
    /// пересобранная внутри `apply`, получает новый.
    generation: u64,
    /// Ревизия шрифтов текстов: растёт, когда текстовый узел появляется, исчезает
    /// или меняет `props.text` (документом или превью). Сдвиг, ресайз и прочие
    /// правки нетекстовых свойств её не меняют — по ней ядро не пересчитывает
    /// отсутствующие шрифты на каждую правку. Сравнивать вместе с `generation`.
    text_revision: u64,
    /// Черновики жеста в порядке `set_drafts` и их слоты (в том же порядке).
    drafts: Vec<Draft>,
    draft_slots: Vec<Slot>,
    /// Те же слоты, что в `draft_slots`, — для `is_draft` за O(1): его вызывают на
    /// каждое событие указателя и на каждого кандидата привязки.
    draft_set: HashSet<Slot>,
    /// Контуры глифов для текста в булевых группах (`set_text_outlines`).
    text_outlines: Option<TextOutlines>,
    /// Растёт, когда меняются шрифты (`refresh_text_outlines`): входит в ключ
    /// содержимого текста, и булевы группы с текстом пересчитываются.
    text_epoch: u64,
}

impl Scene {
    pub fn new() -> Scene {
        Scene {
            generation: fresh_version(),
            ..Scene::default()
        }
    }

    /// Метка состава и имён узлов внутри одного экземпляра сцены: растёт на 1 при
    /// каждом `apply` и при превью имени, не меняется от превью геометрии и
    /// черновиков жеста. Сравнивать имеет смысл вместе с `generation`.
    pub fn structure_stamp(&self) -> u64 {
        self.structure
    }

    /// Номер экземпляра сцены; не повторяется между сценами процесса.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Ревизия текстов внутри экземпляра сцены (см. поле `text_revision`).
    pub fn text_revision(&self) -> u64 {
        self.text_revision
    }

    /// Узел по индексу, если он жив: без паники на освобождённых и чужих индексах.
    pub fn node_at(&self, slot: Slot) -> Option<&SceneNode> {
        let s = slot as usize;
        self.nodes.chunks.get(s / CHUNK)?.get(s % CHUNK)?.as_ref()
    }

    /// Слот занят черновиком жеста (за O(1), без поиска по идентификатору).
    pub fn is_draft_slot(&self, slot: Slot) -> bool {
        self.draft_set.contains(&slot)
    }

    /// Сцена по строкам кэша сцены — до загрузки Loro. Строки идут в порядке обхода,
    /// родитель раньше детей. Данные из файла не доверенные: повтор `NodeId` или
    /// ссылка на пропущенную строку отбрасывают строку вместе с её поддеревом.
    pub fn from_rows(rows: &[SceneRow]) -> Scene {
        let mut scene = Scene::new();
        scene.by_id.reserve(rows.len());
        let mut slot_of: Vec<Option<Slot>> = Vec::with_capacity(rows.len());
        for row in rows {
            let parent = match row.parent {
                None => None,
                Some(p) => match slot_of.get(p as usize).copied().flatten() {
                    Some(s) => Some(s),
                    None => {
                        slot_of.push(None);
                        continue;
                    }
                },
            };
            if scene.by_id.contains_key(&row.id) {
                slot_of.push(None);
                continue;
            }
            let slot = scene.alloc(row.id, row.props.clone());
            scene.link(slot, parent);
            slot_of.push(Some(slot));
        }
        scene.rebuild_geometry();
        scene
    }

    /// Сцена по загруженной странице.
    pub fn from_page(page: &PageDoc) -> Scene {
        // Строки кэша сцены — тот же обход, что нужен сцене; при ошибке чтения
        // (страница в несогласованном состоянии) сцена пуста, а не падает.
        Scene::from_rows(&page.scene_rows().unwrap_or_default())
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Живые узлы документа (без черновиков жеста) в порядке слотов.
    pub fn iter(&self) -> impl Iterator<Item = (Slot, &SceneNode)> + '_ {
        self.nodes
            .chunks
            .iter()
            .flatten()
            .enumerate()
            .filter_map(|(i, n)| Some((i as Slot, n.as_ref()?)))
            .filter(|(s, _)| !self.draft_set.contains(s))
    }

    /// Счётчик изменений: растёт при каждом непустом `apply` и каждом превью,
    /// которое что-то поменяло. Значение само по себе смысла не имеет — только
    /// сравнение с запомненным.
    pub fn change_counter(&self) -> u64 {
        self.changes
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn roots(&self) -> &[Slot] {
        &self.roots
    }

    pub fn slot(&self, id: NodeId) -> Option<Slot> {
        self.by_id.get(&id).copied()
    }

    pub fn node(&self, id: NodeId) -> Option<&SceneNode> {
        self.slot(id).map(|s| self.at(s))
    }

    /// Узел по индексу. Индекс должен быть живым: из `roots`, `children` или `slot`.
    pub fn at(&self, slot: Slot) -> &SceneNode {
        self.nodes.get(slot).as_ref().expect("живой узел сцены")
    }

    fn at_mut(&mut self, slot: Slot) -> &mut SceneNode {
        self.nodes.get_mut(slot).as_mut().expect("живой узел сцены")
    }

    fn alive(&self, slot: Slot) -> bool {
        self.nodes.get(slot).is_some()
    }

    /// Объединение границ всех видимых узлов — для «показать всё».
    pub fn bounds(&self) -> Option<Rect> {
        self.roots
            .iter()
            .filter_map(|&s| self.at(s).subtree_bounds)
            .reduce(|a, b| a.union(b))
    }

    /// Позиция узла среди детей его родителя (или корней страницы).
    pub fn index(&self, slot: Slot) -> u32 {
        self.index[slot as usize]
    }

    /// Виден ли узел: он сам и все предки не скрыты.
    pub fn is_visible(&self, slot: Slot) -> bool {
        let mut cur = Some(slot);
        while let Some(s) = cur {
            let n = self.at(s);
            if !n.props.visible {
                return false;
            }
            cur = n.parent;
        }
        true
    }

    fn alloc(&mut self, id: NodeId, mut props: Props) -> Slot {
        shape::sanitize(&mut props);
        if props.text.is_some() {
            self.text_revision += 1;
        }
        let version = fresh_version();
        let node = SceneNode {
            id,
            local: shape::local_transform(&props),
            props,
            parent: None,
            children: Vec::new(),
            depth: 0,
            world: Affine::IDENTITY,
            local_bounds: None,
            world_bounds: None,
            subtree_bounds: None,
            version,
            subtree_version: version,
            path: OnceLock::new(),
            fill: OnceLock::new(),
            boolean: None,
            content: 0,
        };
        let slot = match self.free.pop() {
            Some(s) => {
                *self.nodes.get_mut(s) = Some(node);
                s
            }
            None => {
                self.marks.push(0);
                self.index.push(0);
                self.nodes.push(node)
            }
        };
        self.by_id.insert(id, slot);
        slot
    }

    /// Добавляет узел последним ребёнком родителя.
    fn link(&mut self, slot: Slot, parent: Option<Slot>) {
        let list = self.list_mut(parent);
        let index = list.len() as u32;
        list.push(slot);
        let depth = parent.map_or(0, |p| self.at(p).depth + 1);
        let n = self.at_mut(slot);
        n.parent = parent;
        n.depth = depth;
        self.index[slot as usize] = index;
    }

    fn list(&self, parent: Option<Slot>) -> &Vec<Slot> {
        match parent {
            Some(p) => &self.at(p).children,
            None => &self.roots,
        }
    }

    fn list_mut(&mut self, parent: Option<Slot>) -> &mut Vec<Slot> {
        match parent {
            Some(p) => &mut self.at_mut(p).children,
            None => &mut self.roots,
        }
    }

    fn next_mark(&mut self) -> u32 {
        self.mark = self.mark.wrapping_add(1);
        if self.mark == 0 {
            self.marks.fill(0);
            self.mark = 1;
        }
        self.mark
    }

    /// Полный пересчёт трансформаций, границ и R-tree — после построения.
    fn rebuild_geometry(&mut self) {
        let order = self.preorder(&self.roots.clone());
        for &s in &order {
            self.update_world(s);
        }
        for &s in order.iter().rev() {
            self.update_bounds(s, false);
        }
        let entries: Vec<Entry> = order
            .iter()
            .filter_map(|&s| self.at(s).world_bounds.map(|b| entry(b, s)))
            .collect();
        self.rtree = RTree::bulk_load_with_params(entries);
    }

    /// Поддеревья `starts` в порядке обхода в глубину, родитель раньше детей.
    fn preorder(&self, starts: &[Slot]) -> Vec<Slot> {
        let mut out = Vec::new();
        let mut stack: Vec<Slot> = starts.iter().rev().copied().collect();
        while let Some(s) = stack.pop() {
            out.push(s);
            stack.extend(self.at(s).children.iter().rev());
        }
        out
    }

    fn update_world(&mut self, slot: Slot) {
        let (parent_world, depth) = match self.at(slot).parent {
            Some(p) => {
                let p = self.at(p);
                (p.world, p.depth + 1)
            }
            None => (Affine::IDENTITY, 0),
        };
        let n = self.at_mut(slot);
        n.local = shape::local_transform(&n.props);
        n.world = parent_world * n.local;
        n.depth = depth;
    }

    /// Пересчитывает границы узла по его свойствам и уже готовым границам детей.
    /// `update_tree` — обновить запись в R-tree (при полном построении дерево
    /// строится пакетно).
    fn update_bounds(&mut self, slot: Slot, update_tree: bool) {
        self.update_content(slot);
        let n = self.at(slot);
        let geom = n.geometry();
        let result = n
            .boolean
            .as_ref()
            .and_then(|b| b.contours.bounds())
            .filter(|_| n.props.kind == NodeKind::Boolean);
        let local_bounds = match geom {
            // Булева группа — по результату; пустой результат — по детям, чтобы
            // рамка выделения показывала, где группа.
            Geometry::None if result.is_some() => result,
            Geometry::None => n
                .children
                .iter()
                .map(|&c| self.at(c))
                .filter(|c| c.props.visible)
                .filter_map(|c| c.local_bounds.map(|b| c.local.transform_rect_bbox(b)))
                .reduce(|a, b| a.union(b)),
            _ => geom.bounds(),
        };
        let outset = shape::stroke_outset(&n.props);
        let world_bounds =
            local_bounds.map(|b| n.world.transform_rect_bbox(b.inflate(outset, outset)));
        // Фрейм с обрезкой: дети не выходят за него, перебирать их не нужно.
        let subtree_bounds = if !n.props.visible {
            None
        } else if n.props.kind == NodeKind::Frame && n.props.clip_content {
            world_bounds
        } else if n.props.kind == NodeKind::Boolean {
            // Дети булевой группы не рисуются: видно только результат.
            world_bounds
        } else {
            n.children
                .iter()
                .filter_map(|&c| self.at(c).subtree_bounds)
                .fold(world_bounds, |acc, b| Some(acc.map_or(b, |a| a.union(b))))
        };
        let old = n.world_bounds;
        let n = self.at_mut(slot);
        n.local_bounds = local_bounds;
        n.world_bounds = world_bounds;
        n.subtree_bounds = subtree_bounds;
        if update_tree && old != world_bounds {
            if let Some(b) = old {
                self.tree_removals.push((slot, b));
            }
            if let Some(b) = world_bounds {
                self.tree_inserts.push((slot, b));
            }
        }
    }

    /// Ключ содержимого узла и, у булевой группы со сменившимся ключом, её
    /// результат. Дети уже посчитаны (обход снизу вверх).
    fn update_content(&mut self, slot: Slot) {
        let n = self.at(slot);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        // У контейнера — тип и операция: его версию меняют и свои заливки, а
        // результат булевой группы сам меняет её версию (иначе ключ не сошёлся
        // бы ни разу).
        if n.props.kind.is_group_like() {
            n.props.kind.hash(&mut h);
            n.props.boolean_op.hash(&mut h);
        } else {
            n.version.hash(&mut h);
        }
        if n.props.kind == NodeKind::Text {
            self.text_epoch.hash(&mut h);
        }
        // Дети — только у группы и булевой группы: форма фрейма для булевой
        // группы — его рамка, а перебор 100 000 детей фрейма на каждую правку
        // ребёнка стоил бы миллисекунды.
        let children: &[Slot] = if n.props.kind.is_group_like() {
            &n.children
        } else {
            &[]
        };
        for &c in children {
            let c = self.at(c);
            c.content.hash(&mut h);
            c.props.visible.hash(&mut h);
            for v in c.local.as_coeffs() {
                v.to_bits().hash(&mut h);
            }
        }
        let key = h.finish();
        if key == n.content && (n.boolean.is_some() || n.props.kind != NodeKind::Boolean) {
            return;
        }
        let outline = (n.props.kind == NodeKind::Boolean).then(|| self.boolean_outline(slot));
        let n = self.at_mut(slot);
        n.content = key;
        if let Some(o) = outline {
            if n.boolean.as_ref().map(|b| &b.contours) != Some(&o.contours) {
                // Другой результат — другой вид: рендер строит список заново.
                n.version = fresh_version();
            }
            n.boolean = Some(Arc::new(o));
        }
    }

    /// Полный пересчёт булевых групп поддерева `root` и булевых предков — если
    /// среди них есть посчитанные по частям (после жеста). Результат по частям
    /// зависит от истории правок; команды, что записывают его в документ
    /// («Сплющить»), берут результат, равный `Scene::from_page`.
    pub fn force_full_booleans(&mut self, root: Slot) {
        let order = self.preorder(&[root]);
        let is_partial = |s: &Self, x: Slot| s.at(x).boolean.as_ref().is_some_and(|b| b.partial);
        if !order.iter().any(|&x| is_partial(self, x)) {
            return;
        }
        let mut chain = Vec::new();
        let mut up = self.at(root).parent;
        while let Some(p) = up {
            chain.push(p);
            up = self.at(p).parent;
        }
        // Новая версия поддерева, как в `recompute`: кэши рендера по ней
        // (растровые кадры групп) не держат результат по частям.
        let version = fresh_version();
        for &x in order.iter().rev().chain(chain.iter()) {
            if self.at(x).props.kind == NodeKind::Boolean {
                self.at_mut(x).boolean = None;
            }
            self.update_bounds(x, true);
            self.at_mut(x).subtree_version = version;
        }
        self.flush_tree();
    }

    /// Результат булевой группы. Формы детей, чей ключ (содержимое и
    /// положение) не сменился, берутся из прошлого результата. Пока идёт жест
    /// (превью) и сменилась ровно одна форма, остальные складываются один раз
    /// (`boolean::combine_rest`), а на каждое движение пересчитываются только
    /// части у этой формы: правка ребёнка группы из 1000 кругов — доли
    /// миллисекунды, а не 20 мс на кадр. Запись жеста (снятие превью и правка
    /// документа — снова тот же ребёнок) берёт те же сложенные остальные и
    /// полного пересчёта не делает; ключ записи, совпавший с последним превью,
    /// не пересчитывает ничего. Другой ребёнок, несколько сразу или смена
    /// операции без жеста — полный пересчёт. Результат по частям может
    /// отличаться от полного на округление `i_overlay` (доли миллионной).
    fn boolean_outline(&self, slot: Slot) -> BooleanOutline {
        let n = self.at(slot);
        let op = n.props.boolean_op.unwrap_or_default();
        let prev = n.boolean.as_deref().filter(|b| b.op == op);
        let mut changed = Vec::new();
        let inputs: Vec<(u64, Arc<Contours>)> = n
            .children
            .iter()
            .filter(|&&c| self.at(c).props.visible)
            .enumerate()
            .map(|(i, &c)| {
                let cn = self.at(c);
                let mut h = std::collections::hash_map::DefaultHasher::new();
                cn.content.hash(&mut h);
                for v in cn.local.as_coeffs() {
                    v.to_bits().hash(&mut h);
                }
                let key = h.finish();
                match prev.and_then(|p| p.inputs.get(i)).filter(|p| p.0 == key) {
                    Some(p) => p.clone(),
                    None => {
                        changed.push(i);
                        (key, Arc::new(self.fill_of(c).transformed(cn.local)))
                    }
                }
            })
            .collect();
        let shapes: Vec<Contours> = inputs.iter().map(|(_, c)| (**c).clone()).collect();
        let single = match (prev, &changed[..]) {
            (Some(p), [i])
                if p.inputs.len() == inputs.len()
                    && inputs.len() > 1
                    && (!self.preview.is_empty() || p.rest.as_ref().is_some_and(|r| r.0 == *i)) =>
            {
                // Пустые формы в операции не участвуют — тогда без частей.
                let was = &p.inputs[*i].1;
                (!was.is_empty() && !shapes[*i].is_empty() && shapes.iter().all(|c| !c.is_empty()))
                    .then_some(*i)
            }
            _ => None,
        };
        let (contours, rest) = match single {
            Some(i) => {
                let rest = match prev.and_then(|p| p.rest.clone()).filter(|r| r.0 == i) {
                    Some(r) => r,
                    None => {
                        let (mode, parts) = boolean::combine_rest(op, &shapes, i);
                        Arc::new((i, mode, parts))
                    }
                };
                (
                    boolean::combine_with(rest.1, &rest.2, &shapes[i]),
                    Some(rest),
                )
            }
            None => {
                let nonempty: Vec<Contours> =
                    shapes.into_iter().filter(|c| !c.is_empty()).collect();
                (boolean::combine(op, &nonempty), None)
            }
        };
        let shape = Arc::new(boolean::outline_shape(&contours));
        BooleanOutline {
            partial: rest.is_some(),
            contours,
            shape,
            op,
            inputs,
            rest,
        }
    }

    /// Форма заливки узла с видимыми детьми в его локальных координатах: у
    /// булевой группы — её результат, у группы — объединение детей, у листа —
    /// `boolean::leaf_fill` (у текста — по контурам глифов редактора).
    ///
    /// Обход групп без рекурсии (снизу вверх, явным стеком): глубина дерева из
    /// файла не ограничена, а цепочка из тысяч вложенных групп в булевой
    /// группе переполнила бы стек worker'а при открытии.
    fn fill_of(&self, slot: Slot) -> Contours {
        if self.at(slot).props.kind != NodeKind::Group {
            return self.own_fill(slot);
        }
        // Группа, индекс следующего ребёнка и формы уже пройденных детей.
        let mut stack: Vec<(Slot, usize, Vec<Contours>)> = vec![(slot, 0, Vec::new())];
        loop {
            let top = stack.last_mut().expect("стек не пуст до выхода");
            let kids = &self.at(top.0).children;
            if let Some(&c) = kids.get(top.1) {
                top.1 += 1;
                let cn = self.at(c);
                if !cn.props.visible {
                    continue;
                }
                if cn.props.kind == NodeKind::Group {
                    stack.push((c, 0, Vec::new()));
                } else {
                    let f = self.own_fill(c).transformed(cn.local);
                    if !f.is_empty() {
                        top.2.push(f);
                    }
                }
                continue;
            }
            let (g, _, parts) = stack.pop().expect("стек не пуст");
            let f = boolean::combine(ferrum_doc::BooleanOp::Union, &parts);
            match stack.last_mut() {
                None => return f,
                Some(parent) => {
                    let f = f.transformed(self.at(g).local);
                    if !f.is_empty() {
                        parent.2.push(f);
                    }
                }
            }
        }
    }

    /// Форма заливки узла, не группы (у булевой группы — её результат).
    fn own_fill(&self, slot: Slot) -> Contours {
        let n = self.at(slot);
        match n.props.kind {
            NodeKind::Boolean => n
                .boolean
                .as_ref()
                .map(|b| b.contours.clone())
                .unwrap_or_default(),
            NodeKind::Group => unreachable!("группу складывает fill_of"),
            NodeKind::Text => {
                let glyphs = self.text_outlines.as_ref().and_then(|f| f(&n.props));
                boolean::leaf_fill(&n.props, None, glyphs.as_ref())
            }
            _ => (**n.fill.get_or_init(|| {
                Arc::new(boolean::leaf_fill(
                    &n.props,
                    n.path_shape().map(|s| &**s),
                    None,
                ))
            }))
            .clone(),
        }
    }

    /// Источник контуров глифов для текста в булевых группах (редактор ставит
    /// его на каждую новую сцену). Булевы группы с текстом пересчитываются.
    pub fn set_text_outlines(&mut self, f: Option<TextOutlines>) {
        self.text_outlines = f;
        self.refresh_text_outlines();
    }

    /// Шрифты сменились (пришли байты, отказ в загрузке): булевы группы с
    /// текстом пересчитываются с новыми контурами глифов. Без таких групп —
    /// ничего не делает.
    pub fn refresh_text_outlines(&mut self) {
        let dirty: Vec<Slot> = self
            .iter()
            .filter(|(_, n)| n.props.kind == NodeKind::Text)
            .filter(|(s, _)| self.has_boolean_ancestor(*s))
            .map(|(s, _)| s)
            .collect();
        if dirty.is_empty() {
            return;
        }
        self.text_epoch += 1;
        self.changes += 1;
        self.recompute(dirty, Vec::new());
        self.flush_tree();
    }

    fn has_boolean_ancestor(&self, slot: Slot) -> bool {
        self.boolean_root(slot).is_some()
    }

    /// Самая внешняя булева группа среди предков узла.
    fn boolean_root(&self, slot: Slot) -> Option<Slot> {
        let mut cur = self.at(slot).parent;
        let mut found = None;
        while let Some(p) = cur {
            if self.at(p).props.kind == NodeKind::Boolean {
                found = Some(p);
            }
            cur = self.at(p).parent;
        }
        found
    }

    /// Применяет накопленные изменения R-tree. Удаление по одной записи стоит
    /// ~1.3 мкс (поиск от корня), а один проход по всему дереву с выборкой по
    /// пометкам — ~0.3 мс на 100 000 записей, поэтому большие пачки удаляются проходом.
    fn flush_tree(&mut self) {
        let removals = std::mem::take(&mut self.tree_removals);
        if removals.len() > TREE_DRAIN_THRESHOLD {
            let mark = self.next_mark();
            for &(s, _) in &removals {
                self.marks[s as usize] = mark;
            }
            let marked = Marked {
                marks: &self.marks,
                mark,
            };
            self.rtree
                .drain_with_selection_function(marked)
                .for_each(drop);
        } else {
            for (s, b) in removals {
                self.rtree.remove(&entry(b, s));
            }
        }
        for (s, b) in std::mem::take(&mut self.tree_inserts) {
            self.rtree.insert(entry(b, s));
        }
    }

    /// Пересчитывает узлы `dirty`: мировую трансформацию и границы их поддеревьев,
    /// затем границы всех предков снизу вверх. `regrouped` — узлы, у которых
    /// поменялся только состав детей (ребёнок ушёл или удалён): им и их предкам
    /// пересчитываются границы. Каждый узел — не больше одного раза.
    fn recompute(&mut self, mut dirty: Vec<Slot>, regrouped: Vec<Slot>) {
        if dirty.is_empty() && regrouped.is_empty() {
            return;
        }
        dirty.sort_unstable_by_key(|&s| self.depth_of(s));
        dirty.dedup();
        let mark = self.next_mark();
        let mut subtree = Vec::new();
        let mut tops = Vec::new();
        for s in dirty {
            if self.marks[s as usize] == mark {
                continue;
            }
            tops.push(s);
            let start = subtree.len();
            subtree.push(s);
            let mut i = start;
            // Обход в ширину внутри поддерева: родитель всегда раньше детей.
            while i < subtree.len() {
                let cur = subtree[i];
                self.marks[cur as usize] = mark;
                subtree.extend(self.at(cur).children.iter().copied());
                i += 1;
            }
        }
        for &s in &subtree {
            self.update_world(s);
        }
        let version = fresh_version();
        for &s in subtree.iter().rev() {
            self.update_bounds(s, true);
            self.at_mut(s).subtree_version = version;
        }
        // Предки: границы группы и поддерева зависят от детей.
        let mut ancestors = Vec::new();
        let starts: Vec<Option<Slot>> = tops
            .into_iter()
            .map(|s| self.at(s).parent)
            .chain(regrouped.into_iter().map(Some))
            .collect();
        for start in starts {
            let mut cur = start;
            while let Some(p) = cur {
                if self.marks[p as usize] == mark {
                    break;
                }
                self.marks[p as usize] = mark;
                ancestors.push(p);
                cur = self.at(p).parent;
            }
        }
        ancestors.sort_unstable_by_key(|&s| std::cmp::Reverse(self.at(s).depth));
        for s in ancestors {
            self.update_bounds(s, true);
            self.at_mut(s).subtree_version = version;
        }
    }

    /// Глубина по цепочке родителей: после перемещения поле `depth` ещё старое.
    fn depth_of(&self, slot: Slot) -> u32 {
        let mut d = 0;
        let mut cur = self.at(slot).parent;
        while let Some(p) = cur {
            d += 1;
            cur = self.at(p).parent;
        }
        d
    }

    /// Применяет изменения документа. Каждый узел из `changes` сверяется с текущим
    /// состоянием страницы: есть ли он, где стоит, какие у него свойства. Поэтому
    /// повторы, пересечения списков и порядок операций внутри транзакции не важны.
    pub fn apply(&mut self, changes: &Changes, page: &PageDoc) {
        if changes.is_empty() {
            return;
        }
        let counter = self.changes + 1;
        let text_revision = self.text_revision;
        // Черновиков нет в документе, а позиции в `changes` посчитаны без них:
        // на время обновления они убираются и потом ставятся обратно поверх.
        let drafts = std::mem::take(&mut self.drafts);
        let draft_patches: Vec<(NodeId, PropsPatch)> = self
            .draft_slots
            .iter()
            .filter_map(|s| {
                let (_, patch) = self.preview.get(s)?;
                Some((self.at(*s).id, patch.clone()))
            })
            .collect();
        self.remove_drafts();
        if self.try_apply(changes, page).is_none() {
            // Сцена разошлась с документом (например, построена по кэшу, который
            // документ отверг) — перестраиваем целиком, превью сохраняется.
            let preview: Vec<(NodeId, PropsPatch)> = self
                .preview
                .iter()
                .map(|(&s, (_, patch))| (self.at(s).id, patch.clone()))
                .collect();
            let outlines = self.text_outlines.take();
            *self = Scene::from_page(page);
            // Ревизия текстов, как и счётчик, не откатывается: пересборка — смена.
            self.text_revision += text_revision + 1;
            if outlines.is_some() {
                self.set_text_outlines(outlines);
            }
            self.set_preview(&preview);
        }
        if !drafts.is_empty() {
            self.set_drafts(&drafts);
            let mut patches: Vec<(NodeId, PropsPatch)> = self
                .preview
                .iter()
                .map(|(&s, (_, patch))| (self.at(s).id, patch.clone()))
                .collect();
            patches.extend(draft_patches);
            self.set_preview(&patches);
        }
        // Пересборка целиком создаёт новую сцену: счётчик не должен откатиться.
        self.changes = counter;
        self.structure += 1;
    }

    fn try_apply(&mut self, changes: &Changes, page: &PageDoc) -> Option<()> {
        let mut seen = HashSet::with_capacity(changes.len());
        // Где каждый узел стоит в документе сейчас: `None` — узла больше нет.
        let placed: Vec<(NodeId, Option<Placement>)> = changes
            .created
            .iter()
            .chain(&changes.deleted)
            .chain(&changes.moved)
            .copied()
            .filter(|id| seen.insert(*id))
            .map(|id| (id, page.placement(id)))
            .collect();

        // 1. Отцепить все затронутые узлы, потом удалить исчезнувшие. Сначала только
        // отцепить: узел, вынесенный из удаляемого фрейма, не должен уйти вместе с ним.
        // Списки детей, которые изменились, и первая изменённая позиция в каждом.
        let mut touched: HashMap<Option<Slot>, usize> = HashMap::new();
        let detached: Vec<Slot> = placed.iter().filter_map(|(id, _)| self.slot(*id)).collect();
        self.detach(&detached, &mut touched);
        for (id, place) in &placed {
            if place.is_none() {
                if let Some(s) = self.slot(*id) {
                    self.remove_subtree(s);
                }
            }
        }

        // 2. Создать недостающие узлы, затем прицепить к новым родителям. Сначала все
        // узлы: родитель, созданный в той же транзакции, должен уже иметь слот.
        let mut attach = Vec::with_capacity(placed.len());
        for (id, place) in placed {
            let Some((parent, position)) = place else {
                continue;
            };
            let slot = match self.slot(id) {
                Some(s) => s,
                None => self.alloc(id, page.props(id).ok()?),
            };
            attach.push((slot, parent, position));
        }
        let mut inserts: HashMap<Option<Slot>, Vec<(Option<usize>, Slot)>> = HashMap::new();
        let mut dirty = Vec::new();
        for (slot, parent, position) in attach {
            let parent = match parent {
                Some(p) => Some(self.slot(p)?),
                None => None,
            };
            self.at_mut(slot).parent = parent;
            inserts.entry(parent).or_default().push((position, slot));
            dirty.push(slot);
        }
        for (parent, list) in inserts {
            let first = match merge(self.list(parent), list) {
                Some((merged, first)) => {
                    *self.list_mut(parent) = merged;
                    first
                }
                // Позиции неизвестны (скрытые узлы на странице) или не сходятся со
                // сценой — список перечитывается из документа.
                None => {
                    let order = page.children(parent.map(|p| self.at(p).id));
                    let slots: Option<Vec<Slot>> = order.iter().map(|id| self.slot(*id)).collect();
                    *self.list_mut(parent) = slots?;
                    0
                }
            };
            let at = touched.entry(parent).or_insert(first);
            *at = (*at).min(first);
        }
        // Позиции в списке сдвигаются только начиная с первой изменённой.
        for (&parent, &from) in &touched {
            // Освобождённый слот мог достаться новому узлу; его список уже верен.
            if parent.is_some_and(|p| !self.alive(p)) {
                continue;
            }
            let list = match parent {
                Some(p) => &self.nodes.get(p).as_ref().expect("живой узел").children,
                None => &self.roots,
            };
            for (i, &s) in list.iter().enumerate().skip(from) {
                self.index[s as usize] = i as u32;
            }
        }

        // 3. Свойства. Созданные узлы прочитаны при создании; перемещённый узел,
        // сменивший и свойства, есть и в `changes.props`.
        for id in &changes.props {
            let Some(slot) = self.slot(*id) else { continue };
            let Ok(props) = page.props(*id) else { continue };
            self.set_doc_props(slot, props);
            dirty.push(slot);
        }
        let regrouped = touched
            .into_keys()
            .flatten()
            .filter(|p| self.alive(*p))
            .collect();
        self.recompute(dirty, regrouped);
        self.flush_tree();
        Some(())
    }

    /// Свойства из документа; превью жеста, если оно есть, накладывается поверх.
    fn set_doc_props(&mut self, slot: Slot, mut props: Props) {
        if let Some((doc, patch)) = self.preview.get_mut(&slot) {
            *doc = props.clone();
            patch.apply_to(&mut props);
        }
        shape::sanitize(&mut props);
        let version = fresh_version();
        let n = self.at_mut(slot);
        if looks_different(&n.props, &props) {
            n.version = version;
            n.path = OnceLock::new();
            n.fill = OnceLock::new();
        }
        // Тот же `Arc` (правка нетекстовых свойств) — сравнение бесплатно.
        let text_changed = match (&n.props.text, &props.text) {
            (None, None) => false,
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b) && a != b,
            _ => true,
        };
        n.props = props;
        if text_changed {
            self.text_revision += 1;
        }
    }

    /// Убирает узлы из списков детей их родителей. Из одного списка — одним проходом.
    /// В `touched` отмечает первую изменённую позицию каждого списка.
    fn detach(&mut self, slots: &[Slot], touched: &mut HashMap<Option<Slot>, usize>) {
        let mut by_parent: HashMap<Option<Slot>, Vec<Slot>> = HashMap::new();
        for &s in slots {
            by_parent.entry(self.at(s).parent).or_default().push(s);
        }
        for (parent, gone) in by_parent {
            let first = gone
                .iter()
                .map(|&s| self.index[s as usize] as usize)
                .min()
                .unwrap_or(0);
            let at = touched.entry(parent).or_insert(first);
            *at = (*at).min(first);
            if let [one] = gone[..] {
                // Позиция уже известна из `index`: искать её в списке линейно незачем.
                let i = self.index[one as usize] as usize;
                let list = self.list_mut(parent);
                debug_assert_eq!(list.get(i), Some(&one), "индекс расходится со списком");
                list.remove(i);
            } else {
                let mark = self.next_mark();
                for &s in &gone {
                    self.marks[s as usize] = mark;
                }
                let marks = std::mem::take(&mut self.marks);
                self.list_mut(parent).retain(|&s| marks[s as usize] != mark);
                self.marks = marks;
            }
            for s in gone {
                self.at_mut(s).parent = None;
            }
        }
    }

    /// Освобождает узел и его поддерево (узел уже отцеплен от родителя).
    fn remove_subtree(&mut self, slot: Slot) {
        for s in self.preorder(&[slot]) {
            let n = self.nodes.get_mut(s).take().expect("живой узел сцены");
            if n.props.text.is_some() {
                self.text_revision += 1;
            }
            if let Some(b) = n.world_bounds {
                self.tree_removals.push((s, b));
            }
            self.by_id.remove(&n.id);
            self.preview.remove(&s);
            self.free.push(s);
        }
    }

    /// Временные правки свойств для текущего жеста (перетаскивание, ресайз, поворот).
    /// Документ не меняется; сцена показывает узлы так, будто правка применена.
    /// Каждый вызов заменяет превью целиком: узлы, которых нет в новом списке,
    /// возвращаются к свойствам документа. Неизвестные узлы пропускаются.
    pub fn set_preview(&mut self, patches: &[(NodeId, PropsPatch)]) {
        if patches.iter().any(|(_, p)| p.name.is_some())
            || self.preview.values().any(|(_, p)| p.name.is_some())
        {
            self.structure += 1;
        }
        let mut dirty = Vec::new();
        let new: HashSet<Slot> = patches
            .iter()
            .filter_map(|(id, _)| self.slot(*id))
            .collect();
        let stale: Vec<Slot> = self
            .preview
            .keys()
            .copied()
            .filter(|s| !new.contains(s))
            .collect();
        for s in stale {
            let (doc, _) = self.preview.remove(&s).expect("есть в превью");
            self.set_doc_props(s, doc);
            dirty.push(s);
        }
        for (id, patch) in patches {
            let Some(slot) = self.slot(*id) else { continue };
            let doc = match self.preview.remove(&slot) {
                Some((doc, _)) => doc,
                None => self.at(slot).props.clone(),
            };
            self.preview.insert(slot, (doc.clone(), patch.clone()));
            self.set_doc_props(slot, doc);
            dirty.push(slot);
        }
        if !dirty.is_empty() {
            self.changes += 1;
        }
        self.recompute(dirty, Vec::new());
        self.flush_tree();
    }

    /// Убирает превью и черновики: все узлы возвращаются к свойствам документа.
    pub fn clear_preview(&mut self) {
        self.set_preview(&[]);
        self.set_drafts(&[]);
    }

    /// Есть ли временные правки или черновики жеста.
    pub fn has_preview(&self) -> bool {
        !self.preview.is_empty() || !self.drafts.is_empty()
    }

    /// Заменяет черновики жеста целиком. Каждый встаёт верхним ребёнком своего
    /// родителя в порядке списка. Черновик с неизвестным родителем или с
    /// идентификатором, который уже есть в сцене, пропускается вместе с поддеревом.
    /// Двигать черновики можно обычным `set_preview`.
    pub fn set_drafts(&mut self, drafts: &[Draft]) {
        let had = !self.draft_slots.is_empty();
        let regrouped = self.remove_drafts();
        let mut dirty = Vec::new();
        for d in drafts {
            if self.by_id.contains_key(&d.id) {
                continue;
            }
            let parent = match d.parent {
                Some(p) => match self.slot(p) {
                    Some(s) => Some(s),
                    None => continue,
                },
                None => None,
            };
            let slot = self.alloc(d.id, d.props.clone());
            self.link(slot, parent);
            self.drafts.push(d.clone());
            self.draft_slots.push(slot);
            self.draft_set.insert(slot);
            dirty.push(slot);
        }
        if had || !dirty.is_empty() {
            self.changes += 1;
        }
        let regrouped = regrouped.into_iter().filter(|p| self.alive(*p)).collect();
        self.recompute(dirty, regrouped);
        self.flush_tree();
    }

    /// Узел — черновик жеста, а не узел документа.
    pub fn is_draft(&self, id: NodeId) -> bool {
        self.slot(id).is_some_and(|s| self.draft_set.contains(&s))
    }

    /// Убирает черновики из списков детей и из таблицы. Черновики всегда верхние
    /// дети своих родителей, поэтому позиции остальных детей не меняются.
    /// Возвращает родителей-узлы документа: их границы нужно пересчитать.
    fn remove_drafts(&mut self) -> Vec<Slot> {
        self.drafts.clear();
        self.draft_set.clear();
        let slots = std::mem::take(&mut self.draft_slots);
        let mut parents = Vec::new();
        // С конца: дети черновиков уходят раньше родителей-черновиков.
        for &s in slots.iter().rev() {
            if !self.alive(s) {
                continue;
            }
            let parent = self.at(s).parent;
            let i = self.index[s as usize] as usize;
            let list = self.list_mut(parent);
            debug_assert_eq!(list.get(i), Some(&s), "черновик расходится со списком");
            list.remove(i);
            self.remove_subtree(s);
            if let Some(p) = parent {
                parents.push(p);
            }
        }
        parents.retain(|p| self.alive(*p) && !slots.contains(p));
        parents
    }

    /// Узлы, чьи собственные границы пересекают прямоугольник (в мире), — запрос по
    /// R-tree без точной проверки формы. Для рамки выделения и отладки отсечения.
    /// Возвращает кандидатов по одним границам: скрытые узлы и части, обрезанные
    /// фреймом, в ответ тоже попадают, поэтому вызывающий должен отфильтровать их
    /// через `is_visible`.
    pub fn query_rect(&self, rect: Rect) -> Vec<NodeId> {
        self.rtree
            .locate_in_envelope_intersecting(&aabb(rect))
            .map(|e| self.at(e.data).id)
            .collect()
    }

    /// Все узлы под точкой, верхний первым. `tolerance` — допуск в мировых единицах
    /// для тонких линий и обводок (4 экранных пикселя, делённые на зум).
    ///
    /// R-tree отбирает кандидатов по границам, затем каждый проверяется точно: форма
    /// с поворотом и обводкой, видимость узла и предков, обрезка фреймами-предками.
    /// Заблокированные узлы тоже возвращаются — правила выделения решает редактор.
    pub fn hit_test_all(&self, point: Point, tolerance: f64) -> Vec<NodeId> {
        let probe = Rect::from_center_size(point, (2.0 * tolerance, 2.0 * tolerance));
        let mut hits: Vec<(Vec<u32>, NodeId)> = self
            .rtree
            .locate_in_envelope_intersecting(&aabb(probe))
            .map(|e| e.data)
            .filter(|&s| self.hits(s, point, tolerance))
            .map(|s| (self.order_key(s), self.at(s).id))
            .collect();
        hits.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        hits.into_iter().map(|(_, id)| id).collect()
    }

    /// Верхний узел под точкой.
    pub fn hit_test(&self, point: Point, tolerance: f64) -> Option<NodeId> {
        self.hit_test_all(point, tolerance).into_iter().next()
    }

    fn hits(&self, slot: Slot, point: Point, tolerance: f64) -> bool {
        if !self.is_visible(slot) || !self.hits_own(slot, point, tolerance) {
            return false;
        }
        // Дети булевой группы не рисуются: в них попадают только там, где
        // рисуется каждая булева группа над ними, а не одна внешняя (дыра
        // вложенного «Вычитания» — мимо, даже если её закрывает сосед).
        let mut cur = self.at(slot).parent;
        while let Some(p) = cur {
            if self.at(p).props.kind == NodeKind::Boolean && !self.hits_own(p, point, tolerance) {
                return false;
            }
            cur = self.at(p).parent;
        }
        true
    }

    /// Попадание в собственную форму узла (без проверки видимости).
    fn hits_own(&self, slot: Slot, point: Point, tolerance: f64) -> bool {
        let n = self.at(slot);
        if n.props.kind == NodeKind::Group {
            return false;
        }
        // Предки-фреймы с обрезкой скрывают всё, что за их формой.
        let mut cur = n.parent;
        while let Some(p) = cur {
            let a = self.at(p);
            if a.props.kind == NodeKind::Frame
                && a.props.clip_content
                && !a.geometry().contains(a.world.inverse() * point)
            {
                return false;
            }
            cur = a.parent;
        }
        shape::hit_shape(
            &n.props,
            n.path_shape().map(|s| &**s),
            n.world.inverse() * point,
            tolerance,
        )
    }

    /// Ключ порядка отрисовки: позиции от корня до узла. Больше — выше на экране;
    /// потомок выше предка.
    pub fn order_key(&self, slot: Slot) -> Vec<u32> {
        let mut key = Vec::new();
        let mut cur = Some(slot);
        while let Some(s) = cur {
            key.push(self.index[s as usize]);
            cur = self.at(s).parent;
        }
        key.reverse();
        key
    }
}

/// Вставляет узлы в список детей на их итоговые позиции за один проход:
/// неперемещённые дети сохраняют порядок, вставленные встают точно на свои места.
/// Возвращает новый список и первую изменённую позицию; `None` — позиции
/// неизвестны или противоречат длине списка.
fn merge(old: &[Slot], mut inserted: Vec<(Option<usize>, Slot)>) -> Option<(Vec<Slot>, usize)> {
    let total = old.len() + inserted.len();
    inserted.sort_unstable_by_key(|(i, _)| *i);
    let mut out = Vec::with_capacity(total);
    let mut rest = old.iter().copied();
    let mut next = inserted.iter().peekable();
    let first = inserted.first()?.0?;
    for at in 0..total {
        match next.peek() {
            Some((Some(i), slot)) if *i == at => {
                out.push(*slot);
                next.next();
            }
            Some((Some(i), _)) if *i < at => return None,
            Some((None, _)) => return None,
            _ => out.push(rest.next()?),
        }
    }
    Some((out, first))
}

/// Меняет ли правка то, что рисуется в локальных координатах узла.
fn looks_different(a: &Props, b: &Props) -> bool {
    a.kind != b.kind
        || a.w != b.w
        || a.h != b.h
        || a.corner_radius != b.corner_radius
        || a.fills != b.fills
        || a.strokes != b.strokes
        || a.stroke_style != b.stroke_style
        || a.text != b.text
        || match (&a.vector, &b.vector) {
            // Та же сеть (правка других свойств) — без сравнения точек.
            (Some(x), Some(y)) => !Arc::ptr_eq(x, y) && x != y,
            (x, y) => x.is_some() != y.is_some(),
        }
}

fn aabb(r: Rect) -> AABB<[f64; 2]> {
    AABB::from_corners([r.x0, r.y0], [r.x1, r.y1])
}

fn entry(r: Rect, slot: Slot) -> Entry {
    GeomWithData::new(Rectangle::from_corners([r.x0, r.y0], [r.x1, r.y1]), slot)
}

/// Внутреннее состояние для тестов: узлы в порядке обхода с родителями, позициями,
/// трансформациями и границами, плюс содержимое R-tree. Две сцены одной страницы,
/// построенные разными путями, должны давать одинаковый снимок.
#[doc(hidden)]
pub mod testing {
    use super::*;

    #[derive(Debug, PartialEq)]
    pub struct NodeState {
        pub id: NodeId,
        pub parent: Option<NodeId>,
        pub index: u32,
        pub depth: u32,
        pub props: Props,
        pub world: [f64; 6],
        pub local_bounds: Option<Rect>,
        pub world_bounds: Option<Rect>,
        pub subtree_bounds: Option<Rect>,
    }

    /// Удаляет узел из таблицы идентификаторов, оставляя его в списках. Такое
    /// расхождение сцены с документом заставляет `apply` перестроить сцену целиком.
    pub fn forget(scene: &mut Scene, id: NodeId) {
        scene.by_id.remove(&id);
    }

    pub fn snapshot(scene: &Scene) -> (Vec<NodeState>, Vec<(String, [f64; 4])>) {
        let order = scene.preorder(scene.roots());
        let nodes = order
            .iter()
            .map(|&s| {
                let n = scene.at(s);
                NodeState {
                    id: n.id,
                    parent: n.parent.map(|p| scene.at(p).id),
                    index: scene.index[s as usize],
                    depth: n.depth,
                    props: n.props.clone(),
                    world: n.world.as_coeffs(),
                    local_bounds: n.local_bounds,
                    world_bounds: n.world_bounds,
                    subtree_bounds: n.subtree_bounds,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(order.len(), scene.len(), "все узлы достижимы от корней");
        let mut tree: Vec<(String, [f64; 4])> = scene
            .rtree
            .iter()
            .map(|e| {
                let (lo, hi) = (e.geom().lower(), e.geom().upper());
                (
                    scene.at(e.data).id.to_string(),
                    [lo[0], lo[1], hi[0], hi[1]],
                )
            })
            .collect();
        tree.sort_by(|a, b| a.partial_cmp(b).expect("границы конечны"));
        (nodes, tree)
    }
}
