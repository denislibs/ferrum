//! Документ одной страницы: дерево узлов в собственном `LoroDoc`.
//!
//! Три решения, к которым привели бенчмарк и тесты:
//! - Стабильный `NodeId` — это `TreeID` первого создания узла, записанный ключом `id`
//!   в его свойства; индекс сопоставляет его текущему `TreeID`. Восстановленный после
//!   отмены удаления узел получает в Loro новый `TreeID`, но сохраняет свой `NodeId`.
//! - Чтение свойств из Loro стоит ~9 мкс на узел, поэтому страница держит зеркало
//!   свойств в памяти; оно заполняется из кэша сцены (или один раз из Loro) и
//!   обновляется каждой правкой.
//! - `UndoManager` Loro на большом документе строит внутренние структуры
//!   (~4.5 КБ на узел). Поэтому отмена своя: каждая правка записывает обратную
//!   операцию в журнал транзакции, и память растёт с числом правок, а не с размером
//!   документа.

use std::cell::RefCell;
use std::collections::HashMap;

use loro::{ExportMode, LoroDoc, LoroTree, LoroValue, TreeID, TreeParentId, ValueOrContainer};

use crate::codec;
use crate::error::{DocError, DocResult};
use crate::scene_cache::SceneRow;
use crate::types::*;

const TREE: &str = "nodes";
const KEY_ID: &str = "id";

/// Узел, сохранённый перед удалением, чтобы восстановить его отменой.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SavedNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub index: usize,
    pub props: Props,
}

/// Обратная операция для отмены одной правки.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Inverse {
    /// Отмена создания.
    Delete(NodeId),
    /// Отмена удаления: поддерево в порядке обхода, родители раньше детей.
    Restore(Vec<SavedNode>),
    /// Отмена изменения свойств: прежние значения изменённых полей.
    Props(NodeId, PropsPatch),
    /// Отмена перемещения.
    Move {
        id: NodeId,
        parent: Option<NodeId>,
        index: usize,
    },
}

/// Индекс стабильных ID и зеркало свойств живых узлов.
#[derive(Default)]
struct State {
    by_id: HashMap<NodeId, TreeID>,
    by_tree: HashMap<TreeID, NodeId>,
    props: HashMap<NodeId, Props>,
}

impl State {
    fn insert(&mut self, id: NodeId, tid: TreeID, props: Props) {
        self.by_id.insert(id, tid);
        self.by_tree.insert(tid, id);
        self.props.insert(id, props);
    }

    fn remove(&mut self, id: NodeId) {
        if let Some(tid) = self.by_id.remove(&id) {
            self.by_tree.remove(&tid);
        }
        self.props.remove(&id);
    }
}

pub struct PageDoc {
    doc: LoroDoc,
    tree: LoroTree,
    state: RefCell<State>,
    /// Журнал обратных операций текущей транзакции; `None` — запись выключена.
    journal: RefCell<Option<Vec<Inverse>>>,
}

impl PageDoc {
    pub fn new() -> PageDoc {
        PageDoc::wrap(LoroDoc::new())
    }

    /// Загружает страницу из снапшота, читая свойства всех узлов из Loro.
    pub fn from_snapshot(bytes: &[u8]) -> DocResult<PageDoc> {
        PageDoc::load(bytes, None)
    }

    /// Загружает страницу. С кэшем сцены (уже сверенным со снапшотом по хэшу)
    /// индекс и зеркало строятся без чтения свойств из Loro.
    pub(crate) fn load(bytes: &[u8], cache: Option<&[SceneRow]>) -> DocResult<PageDoc> {
        let page = PageDoc::wrap(LoroDoc::from_snapshot(bytes)?);
        match cache {
            Some(rows) => {
                let mut state = State::default();
                for row in rows {
                    state.insert(row.id, row.tree_id, row.props.clone());
                }
                *page.state.borrow_mut() = state;
            }
            None => page.rebuild_from_loro()?,
        }
        Ok(page)
    }

    fn wrap(doc: LoroDoc) -> PageDoc {
        let tree = doc.get_tree(TREE);
        tree.enable_fractional_index(0);
        PageDoc {
            doc,
            tree,
            state: RefCell::default(),
            journal: RefCell::default(),
        }
    }

    /// Стабильный ID узла по его текущему `TreeID`. Узел без ключа `id`
    /// (не должен встречаться) считается созданным под своим `TreeID`.
    fn stable_id(&self, tid: TreeID) -> NodeId {
        let stored = self
            .tree
            .get_meta(tid)
            .ok()
            .and_then(|m| match m.get(KEY_ID) {
                Some(ValueOrContainer::Value(LoroValue::String(s))) => s.parse::<NodeId>().ok(),
                _ => None,
            });
        stored.unwrap_or(NodeId(tid))
    }

    /// Полное построение индекса и зеркала из Loro — при загрузке без кэша.
    fn rebuild_from_loro(&self) -> DocResult<()> {
        let mut state = State::default();
        let mut stack = self.tree.children(TreeParentId::Root).unwrap_or_default();
        while let Some(tid) = stack.pop() {
            let id = self.stable_id(tid);
            let props = codec::read_props(&self.tree.get_meta(tid)?, &id)?;
            state.insert(id, tid, props);
            stack.extend(
                self.tree
                    .children(TreeParentId::Node(tid))
                    .unwrap_or_default(),
            );
        }
        *self.state.borrow_mut() = state;
        Ok(())
    }

    /// Shallow-снапшот: текущее состояние без истории правок. Меньше полного,
    /// быстрее раскодируется и на документе после правок экспортируется быстрее
    /// (история сессии в него не входит). История между сессиями не нужна:
    /// отмена работает в пределах сессии, версии файла появятся с сервером.
    pub fn export_snapshot(&self) -> DocResult<Vec<u8>> {
        let frontiers = self.doc.oplog_frontiers();
        if frontiers.is_empty() {
            return Ok(self.doc.export(ExportMode::Snapshot)?);
        }
        Ok(self.doc.export(ExportMode::shallow_snapshot(&frontiers))?)
    }

    pub(crate) fn commit(&self) {
        self.doc.commit();
    }

    #[doc(hidden)]
    pub fn commit_for_tests(&self) {
        self.doc.commit();
    }

    pub(crate) fn begin_journal(&self) {
        *self.journal.borrow_mut() = Some(Vec::new());
    }

    pub(crate) fn take_journal(&self) -> Vec<Inverse> {
        self.journal.borrow_mut().take().unwrap_or_default()
    }

    fn record(&self, op: Inverse) {
        if let Some(journal) = self.journal.borrow_mut().as_mut() {
            journal.push(op);
        }
    }

    /// Применяет обратные операции в обратном порядке и возвращает операции,
    /// которые отменят это применение (для redo).
    pub(crate) fn apply_inverse(&self, ops: &[Inverse]) -> DocResult<Vec<Inverse>> {
        self.begin_journal();
        let result = ops.iter().rev().try_for_each(|op| match op {
            Inverse::Delete(id) => self.delete_nodes(&[*id]),
            Inverse::Restore(nodes) => nodes.iter().try_for_each(|n| {
                self.create_with(n.id, n.parent, Some(n.index), n.props.clone())
                    .map(|_| ())
            }),
            Inverse::Props(id, patch) => self.set_props(*id, patch),
            Inverse::Move { id, parent, index } => self.move_node(*id, *parent, *index),
        });
        let redo = self.take_journal();
        self.commit();
        result.map(|_| redo)
    }

    fn tid(&self, id: NodeId) -> DocResult<TreeID> {
        let tid = self.state.borrow().by_id.get(&id).copied();
        match tid {
            Some(t) if !self.tree.is_node_deleted(&t).unwrap_or(true) => Ok(t),
            _ => Err(DocError::NodeNotFound(id.to_string())),
        }
    }

    fn parent_tid(&self, parent: Option<NodeId>) -> DocResult<TreeParentId> {
        Ok(match parent {
            Some(p) => TreeParentId::Node(self.tid(p)?),
            None => TreeParentId::Root,
        })
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.tid(id).is_ok()
    }

    /// Создаёт узел со свойствами по умолчанию. `index = None` — поверх всех детей.
    pub fn create_node(
        &self,
        parent: Option<NodeId>,
        index: Option<usize>,
        kind: NodeKind,
    ) -> DocResult<NodeId> {
        let tid = self.create_tree_node(parent, index)?;
        let id = NodeId(tid);
        self.finish_create(id, tid, Props::defaults(kind))?;
        Ok(id)
    }

    /// Восстанавливает узел под прежним стабильным ID (отмена удаления).
    fn create_with(
        &self,
        id: NodeId,
        parent: Option<NodeId>,
        index: Option<usize>,
        props: Props,
    ) -> DocResult<NodeId> {
        let tid = self.create_tree_node(parent, index)?;
        self.finish_create(id, tid, props)?;
        Ok(id)
    }

    fn create_tree_node(&self, parent: Option<NodeId>, index: Option<usize>) -> DocResult<TreeID> {
        let parent = self.parent_tid(parent)?;
        Ok(match index {
            Some(i) => self.tree.create_at(parent, i)?,
            None => self.tree.create(parent)?,
        })
    }

    fn finish_create(&self, id: NodeId, tid: TreeID, props: Props) -> DocResult<()> {
        let meta = self.tree.get_meta(tid)?;
        meta.insert(KEY_ID, id.to_string())?;
        codec::write_props(&meta, &props)?;
        self.state.borrow_mut().insert(id, tid, props);
        self.record(Inverse::Delete(id));
        Ok(())
    }

    pub fn props(&self, id: NodeId) -> DocResult<Props> {
        self.tid(id)?;
        self.state
            .borrow()
            .props
            .get(&id)
            .cloned()
            .ok_or_else(|| DocError::NodeNotFound(id.to_string()))
    }

    pub fn kind(&self, id: NodeId) -> DocResult<NodeKind> {
        Ok(self.props(id)?.kind)
    }

    pub fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        let tid = self.tid(id)?;
        let before = self.props(id)?;
        codec::write_patch(&self.tree.get_meta(tid)?, patch)?;
        if let Some(props) = self.state.borrow_mut().props.get_mut(&id) {
            patch.apply_to(props);
        }
        self.record(Inverse::Props(id, patch.previous(&before)));
        Ok(())
    }

    /// Позиция узла среди детей его родителя.
    fn index_of(&self, id: NodeId, parent: Option<NodeId>) -> usize {
        self.children(parent)
            .iter()
            .position(|c| *c == id)
            .unwrap_or(0)
    }

    /// Переносит узел к новому родителю на позицию `index` среди его детей.
    pub fn move_node(&self, id: NodeId, parent: Option<NodeId>, index: usize) -> DocResult<()> {
        let tid = self.tid(id)?;
        if let Some(p) = parent {
            if p == id || self.is_ancestor(id, p) {
                return Err(DocError::CyclicMove);
            }
        }
        let old_parent = self.parent(id);
        let old_index = self.index_of(id, old_parent);
        self.tree.mov_to(tid, self.parent_tid(parent)?, index)?;
        self.record(Inverse::Move {
            id,
            parent: old_parent,
            index: old_index,
        });
        Ok(())
    }

    fn is_ancestor(&self, ancestor: NodeId, mut node: NodeId) -> bool {
        while let Some(p) = self.parent(node) {
            if p == ancestor {
                return true;
            }
            node = p;
        }
        false
    }

    /// Удаляет узлы вместе с потомками. Уже удалённые пропускаются.
    pub fn delete_nodes(&self, ids: &[NodeId]) -> DocResult<()> {
        for id in ids {
            let Ok(tid) = self.tid(*id) else { continue };
            let parent = self.parent(*id);
            let mut saved = vec![SavedNode {
                id: *id,
                parent,
                index: self.index_of(*id, parent),
                props: self.props(*id)?,
            }];
            for node in self.walk_from(Some(*id)) {
                let parent = self.parent(node);
                saved.push(SavedNode {
                    id: node,
                    parent,
                    index: self.index_of(node, parent),
                    props: self.props(node)?,
                });
            }
            self.tree.delete(tid)?;
            let mut state = self.state.borrow_mut();
            for node in &saved {
                state.remove(node.id);
            }
            drop(state);
            self.record(Inverse::Restore(saved));
        }
        Ok(())
    }

    /// Дети снизу вверх (последний рисуется поверх). `None` — корень страницы.
    pub fn children(&self, parent: Option<NodeId>) -> Vec<NodeId> {
        let Ok(parent) = self.parent_tid(parent) else {
            return Vec::new();
        };
        let state = self.state.borrow();
        self.tree
            .children(parent)
            .unwrap_or_default()
            .into_iter()
            .map(|t| state.by_tree.get(&t).copied().unwrap_or(NodeId(t)))
            .collect()
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        match self.tree.parent(self.tid(id).ok()?)? {
            TreeParentId::Node(p) => Some(
                self.state
                    .borrow()
                    .by_tree
                    .get(&p)
                    .copied()
                    .unwrap_or(NodeId(p)),
            ),
            _ => None,
        }
    }

    /// Все живые узлы страницы в порядке обхода в глубину.
    pub fn walk(&self) -> Vec<NodeId> {
        self.walk_from(None)
    }

    /// Потомки узла (без него самого) в порядке обхода в глубину.
    fn walk_from(&self, root: Option<NodeId>) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.children(root).into_iter().rev().collect();
        while let Some(id) = stack.pop() {
            out.push(id);
            stack.extend(self.children(Some(id)).into_iter().rev());
        }
        out
    }

    /// Таблица всех узлов для кэша сцены: обход в глубину, родитель раньше детей.
    pub fn scene_rows(&self) -> DocResult<Vec<SceneRow>> {
        let mut rows = Vec::new();
        let mut stack: Vec<(NodeId, Option<u32>)> = self
            .children(None)
            .into_iter()
            .rev()
            .map(|id| (id, None))
            .collect();
        while let Some((id, parent)) = stack.pop() {
            let row = rows.len() as u32;
            rows.push(SceneRow {
                id,
                tree_id: self.tid(id)?,
                parent,
                props: self.props(id)?,
            });
            stack.extend(
                self.children(Some(id))
                    .into_iter()
                    .rev()
                    .map(|c| (c, Some(row))),
            );
        }
        Ok(rows)
    }

    /// Хэши изображений, на которые ссылаются заливки живых узлов страницы.
    pub fn image_refs(&self) -> DocResult<Vec<String>> {
        let state = self.state.borrow();
        let mut refs: Vec<String> = state
            .props
            .values()
            .flat_map(|p| p.fills.iter().filter_map(|f| f.image_hash.clone()))
            .collect();
        refs.sort();
        refs.dedup();
        Ok(refs)
    }
}

impl Default for PageDoc {
    fn default() -> Self {
        PageDoc::new()
    }
}
