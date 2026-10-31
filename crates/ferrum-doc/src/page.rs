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

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use loro::{
    ExportMode, LoroDoc, LoroText, LoroTree, LoroValue, TextDelta, TreeID, TreeParentId,
    ValueOrContainer, VersionVector,
};

use crate::codec;
use crate::error::{DocError, DocResult};
use crate::loro_ext;
use crate::scene_cache::SceneRow;
use crate::text::{
    AttrKey, TextAttrs, TextContent, TextData, TextRun, MAX_TEXT_CHARS, MAX_TEXT_RUNS,
};
use crate::text_codec;
use crate::types::*;
use crate::vector::{VectorError, VectorNetwork};

const TREE: &str = "nodes";
const KEY_ID: &str = "id";

/// Сеть, которую пишет правка: сверх 100 000 точек — отказ с сообщением, иначе
/// неверная сеть (ошибка редактора) не попадает в документ.
fn check_vector(v: &VectorNetwork) -> DocResult<()> {
    match v.validate() {
        Ok(()) => Ok(()),
        Err(VectorError::TooManyPoints) => Err(DocError::VectorTooLarge),
        Err(e) => Err(DocError::VectorRejected(e)),
    }
}

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
    /// Отмена изменения свойств: прежние значения изменённых полей. За `Box`:
    /// с текстом патч стал большим, а варианты журнала живут тысячами.
    Props(NodeId, Box<PropsPatch>),
    /// Отмена смены вида узла (фигура ↔ вектор): тип, скругление, обрезка и
    /// сеть прежними. Применение пишет обратную к себе — повтор работает так же.
    Shape(NodeId, Box<Props>),
    /// Отмена перемещения.
    Move {
        id: NodeId,
        parent: Option<NodeId>,
        index: usize,
    },
    /// Отмена правки текста: обратная дельта Loro (спека подсистемы 3, раздел 3).
    /// Применяется к текущему `LoroText` узла, найденному по стабильному id, —
    /// поэтому переживает удаление и восстановление узла под новым `TreeID`.
    Text(NodeId, Vec<TextDelta>),
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

/// Правка страницы. Выдаётся только замыканию `Document::transact`: так каждая
/// правка проходит проверку «только для чтения», попадает в историю отмены и
/// фиксируется в Loro. Чтение — через `Deref` на `PageDoc`.
///
/// Мимо транзакции страницу изменить нельзя:
/// ```compile_fail
/// let mut doc = ferrum_doc::Document::new();
/// let page = doc.pages()[0].0;
/// doc.page(page)
///     .unwrap()
///     .create_node(None, None, ferrum_doc::NodeKind::Rect)
///     .unwrap();
/// ```
pub struct Tx<'a> {
    page: &'a PageDoc,
}

pub struct PageDoc {
    doc: LoroDoc,
    tree: LoroTree,
    state: RefCell<State>,
    /// Журнал обратных операций текущей транзакции; `None` — запись выключена.
    journal: RefCell<Option<Vec<Inverse>>>,
    /// При загрузке встретились узлы неизвестного типа (см. `rebuild_from_loro`).
    unknown_nodes: Cell<bool>,
    /// Есть текст сверх пределов (спека 3): из файла чужой или старой сборки.
    /// Правка такого текста нарушила бы пределы, поэтому документ — только для чтения.
    oversized_text: Cell<bool>,
    /// Изображения, на которые ссылаются скрытые узлы неизвестного типа и их
    /// потомки. Зеркало их не видит, но при сохранении эти байты терять нельзя.
    hidden_refs: RefCell<Vec<String>>,
    /// Индекс и зеркало взяты из кэша сцены, а не прочитаны из Loro.
    from_cache: Cell<bool>,
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
    /// индекс и зеркало строятся без чтения свойств из Loro; кэш, который не
    /// сходится с деревом Loro, отбрасывается.
    pub(crate) fn load(bytes: &[u8], cache: Option<&[SceneRow]>) -> DocResult<PageDoc> {
        let page = PageDoc::wrap(LoroDoc::from_snapshot(bytes)?);
        match cache {
            Some(rows) => {
                let mut state = State::default();
                for row in rows {
                    state.insert(row.id, row.tree_id, row.props.clone());
                }
                *page.state.borrow_mut() = state;
                if page.cache_matches_tree(rows) {
                    page.from_cache.set(true);
                    // Кэш — тоже ввод из файла: текст сверх пределов в нём делает
                    // страницу только для чтения, как и при чтении из Loro.
                    page.oversized_text.set(rows.iter().any(|r| {
                        r.props
                            .text
                            .as_ref()
                            .is_some_and(|t| t.content.check_limits().is_err())
                    }));
                } else {
                    page.rebuild_from_loro()?;
                }
            }
            None => page.rebuild_from_loro()?,
        }
        Ok(page)
    }

    /// Совпадение хэша говорит лишь, что кэш записан рядом с этим снапшотом, но не
    /// что он верен: таблица из чужой или ошибочной сборки рассинхронизирует индекс,
    /// и страницу потом нельзя сохранить. Проверка за O(n) без чтения свойств:
    /// ID и `TreeID` уникальны (индекс не схлопнул строки), родитель каждой строки
    /// совпадает с родителем в Loro — значит, все строки живы (цепочка доходит до
    /// корня), а сумма детей корня и всех строк равна числу строк — значит, ни один
    /// живой узел не пропущен.
    fn cache_matches_tree(&self, rows: &[SceneRow]) -> bool {
        let state = self.state.borrow();
        if state.by_id.len() != rows.len() || state.by_tree.len() != rows.len() {
            return false;
        }
        let mut live = self.tree.children_num(TreeParentId::Root).unwrap_or(0);
        for row in rows {
            let expected = match row.parent {
                None => TreeParentId::Root,
                Some(p) => match rows.get(p as usize) {
                    Some(parent) => TreeParentId::Node(parent.tree_id),
                    None => return false,
                },
            };
            if self.tree.parent(row.tree_id) != Some(expected) {
                return false;
            }
            live += self
                .tree
                .children_num(TreeParentId::Node(row.tree_id))
                .unwrap_or(0);
        }
        live == rows.len()
    }

    fn wrap(doc: LoroDoc) -> PageDoc {
        // Расширение разметки текста на вставку не хранится в документе: его
        // задаёт каждый `LoroDoc` страницы (новый, из снапшота, из журнала).
        doc.config_text_style(text_codec::style_config());
        let tree = doc.get_tree(TREE);
        tree.enable_fractional_index(0);
        PageDoc {
            doc,
            tree,
            state: RefCell::default(),
            journal: RefCell::default(),
            unknown_nodes: Cell::default(),
            oversized_text: Cell::default(),
            hidden_refs: RefCell::default(),
            from_cache: Cell::default(),
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
    ///
    /// Узел неизвестного типа (из более новой версии Ferrum) не роняет загрузку:
    /// он и его поддерево не попадают в индекс и для модели не существуют — их не
    /// видно в `children`, `walk` и кэше сцены, но в снапшоте Loro они остаются.
    /// Чтобы правка не потеряла их (удаление предка, отмена), документ с такой
    /// страницей открывается только для чтения (`has_unknown_nodes`), а ссылки
    /// скрытого поддерева на изображения запоминаются для сохранения.
    fn rebuild_from_loro(&self) -> DocResult<()> {
        let mut state = State::default();
        let mut unknown = false;
        let mut oversized = false;
        let mut hidden_refs = Vec::new();
        let mut stack = self.tree.children(TreeParentId::Root).unwrap_or_default();
        while let Some(tid) = stack.pop() {
            let id = self.stable_id(tid);
            let props = match codec::read_props(&self.tree.get_meta(tid)?, &id) {
                Ok(props) => props,
                // Неверная сеть вектора (чужая сборка, повреждение) — как
                // неизвестный тип: узел скрыт, документ только для чтения.
                Err(DocError::UnknownKind(..) | DocError::InvalidVector(..)) => {
                    unknown = true;
                    self.collect_hidden_refs(tid, &mut hidden_refs)?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            oversized |= props
                .text
                .as_ref()
                .is_some_and(|t| t.content.check_limits().is_err());
            state.insert(id, tid, props);
            stack.extend(
                self.tree
                    .children(TreeParentId::Node(tid))
                    .unwrap_or_default(),
            );
        }
        *self.state.borrow_mut() = state;
        self.unknown_nodes.set(unknown);
        self.oversized_text.set(oversized);
        hidden_refs.sort();
        hidden_refs.dedup();
        *self.hidden_refs.borrow_mut() = hidden_refs;
        Ok(())
    }

    /// Хэши изображений в заливках скрытого поддерева (сам узел и все потомки,
    /// какого бы типа они ни были).
    fn collect_hidden_refs(&self, root: TreeID, out: &mut Vec<String>) -> DocResult<()> {
        let mut stack = vec![root];
        while let Some(tid) = stack.pop() {
            out.extend(codec::read_image_refs(&self.tree.get_meta(tid)?));
            stack.extend(
                self.tree
                    .children(TreeParentId::Node(tid))
                    .unwrap_or_default(),
            );
        }
        Ok(())
    }

    /// Загружена ли страница по кэшу сцены — чтобы тесты проверяли именно быстрый путь.
    #[doc(hidden)]
    pub fn loaded_from_cache(&self) -> bool {
        self.from_cache.get()
    }

    /// Есть ли на странице узлы неизвестного типа, скрытые от модели.
    pub fn has_unknown_nodes(&self) -> bool {
        self.unknown_nodes.get()
    }

    /// Есть ли на странице текст сверх пределов (`MAX_TEXT_CHARS`, `MAX_TEXT_RUNS`).
    pub fn has_oversized_text(&self) -> bool {
        self.oversized_text.get()
    }

    /// Только для тестов: текст узла в Loro без проверки пределов (как в файле
    /// чужой сборки).
    #[doc(hidden)]
    pub fn force_text_for_tests(&self, id: NodeId, text: &str) {
        let tid = self.tid(id).expect("узел есть");
        let t = text_codec::text_container(&self.tree.get_meta(tid).expect("карта узла"))
            .expect("текст узла");
        t.insert(0, text).expect("вставка в Loro");
        self.doc.commit();
    }

    /// Только для тестов: значение `vector` узла в Loro как есть (как в файле
    /// чужой сборки) — без проверки и без зеркала.
    #[doc(hidden)]
    pub fn force_vector_bytes_for_tests(&self, id: NodeId, bytes: &[u8]) {
        let tid = self.tid(id).expect("узел есть");
        let meta = self.tree.get_meta(tid).expect("карта узла");
        meta.insert(codec::key::VECTOR, LoroValue::Binary(bytes.to_vec().into()))
            .expect("запись в Loro");
        self.doc.commit();
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

    /// Версия операций страницы — отметка журнала.
    pub(crate) fn oplog_vv(&self) -> VersionVector {
        self.doc.oplog_vv()
    }

    /// Обновления Loro с версии `from` — секция пачки журнала.
    pub(crate) fn export_updates(&self, from: &VersionVector) -> DocResult<Vec<u8>> {
        Ok(self.doc.export(ExportMode::updates(from))?)
    }

    /// Применяет обновления журнала и строит индекс и зеркало заново из Loro: кэш
    /// сцены файла их уже не описывает. `false` — часть обновлений не применена
    /// (повреждена или ей не хватает предыдущих).
    pub(crate) fn import_updates(&self, batches: &[&[u8]]) -> DocResult<bool> {
        let mut complete = true;
        for data in batches {
            match self.doc.import(data) {
                Ok(status) => complete &= status.pending.is_none(),
                Err(_) => complete = false,
            }
        }
        self.from_cache.set(false);
        self.rebuild_from_loro()?;
        Ok(complete)
    }

    #[doc(hidden)]
    pub fn commit_for_tests(&self) {
        self.doc.commit();
    }

    /// Переписывает тип узла прямо в Loro — как сделала бы более новая версия Ferrum.
    /// `None` — ключ типа удаляется.
    #[cfg(test)]
    pub(crate) fn force_raw_kind_for_tests(&self, id: NodeId, raw: Option<&str>) {
        let meta = self.tree.get_meta(self.tid(id).unwrap()).unwrap();
        match raw {
            Some(raw) => meta.insert(codec::key::TYPE, raw).unwrap(),
            None => meta.delete(codec::key::TYPE).unwrap(),
        }
        self.commit();
    }

    /// Правка отдельной страницы без документа — для тестов и бенчмарков:
    /// без истории отмены и без фиксации (её делает `commit_for_tests`).
    #[doc(hidden)]
    pub fn tx_for_tests(&self) -> Tx<'_> {
        Tx::new(self)
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
    ///
    /// При сбое посреди списка уже применённая часть откатывается по своему же
    /// журналу: страница возвращается в состояние до вызова, и шаг истории, который
    /// вызывающий код вернёт в стек, по-прежнему соответствует странице.
    pub(crate) fn apply_inverse(&self, ops: &[Inverse]) -> DocResult<Vec<Inverse>> {
        self.begin_journal();
        let result = self.replay(ops);
        let redo = self.take_journal();
        let result = match result {
            Ok(()) => Ok(redo),
            // Журнал уже выключен: откат не пишет новых обратных операций.
            Err(original) => Err(match self.replay(&redo) {
                Ok(()) => original,
                Err(rollback) => DocError::RollbackFailed {
                    original: Box::new(original),
                    rollback: Box::new(rollback),
                },
            }),
        };
        self.commit();
        result
    }

    fn replay(&self, ops: &[Inverse]) -> DocResult<()> {
        ops.iter().rev().try_for_each(|op| match op {
            Inverse::Delete(id) => self.delete_nodes(&[*id]),
            Inverse::Restore(nodes) => nodes.iter().try_for_each(|n| {
                self.create_with(n.id, n.parent, Some(n.index), n.props.clone())
                    .map(|_| ())
            }),
            Inverse::Props(id, patch) => self.set_props(*id, patch),
            Inverse::Shape(id, props) => self.set_shape(*id, props),
            Inverse::Move { id, parent, index } => self.move_node(*id, *parent, *index),
            Inverse::Text(id, delta) => self.apply_text_delta(*id, delta),
        })
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
    pub(crate) fn create_node(
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

    /// Узел сразу со всеми свойствами — копия или вставка; у текста и
    /// содержимое. `index = None` — поверх всех детей.
    pub(crate) fn create_with_props(
        &self,
        parent: Option<NodeId>,
        index: Option<usize>,
        props: &Props,
    ) -> DocResult<NodeId> {
        // Пределы текста и сети — до создания узла: отказ не оставляет ни узла,
        // ни шага.
        if let Some(t) = &props.text {
            t.content.check_limits()?;
        }
        if let Some(v) = &props.vector {
            check_vector(v)?;
        }
        let tid = self.create_tree_node(parent, index)?;
        let id = NodeId(tid);
        self.finish_create(id, tid, props.clone())?;
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

    /// Создаёт узел у родителя. `index` ограничивается числом детей: позиция за концом
    /// списка означает «в конец», а не ошибку.
    fn create_tree_node(&self, parent: Option<NodeId>, index: Option<usize>) -> DocResult<TreeID> {
        let parent = self.parent_tid(parent)?;
        Ok(match index {
            Some(i) => {
                let max = self.child_count(parent, None);
                self.tree.create_at(parent, i.min(max))?
            }
            None => self.tree.create(parent)?,
        })
    }

    /// Число детей родителя, не считая узла `excluding` (если он уже среди них).
    /// Берётся из `children_num` Loro без построения списка: отмена удаления вызывает
    /// это на каждый восстановленный узел, и список соседей стоил бы O(k · соседи).
    fn child_count(&self, parent: TreeParentId, excluding: Option<TreeID>) -> usize {
        let n = self.tree.children_num(parent).unwrap_or(0);
        match excluding {
            Some(t) if self.tree.parent(t) == Some(parent) => n.saturating_sub(1),
            _ => n,
        }
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

    /// Правка, которая ничего не меняет (те же значения), не пишется и не
    /// записывает обратную операцию: транзакция без изменений шага не создаёт.
    pub(crate) fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        // Содержимое текста — своей правкой (обратная дельта в журнале отмены),
        // остальное — патчем свойств. У не текста содержимое пропускается.
        if let Some(content) = &patch.content {
            if self.props(id)?.text.is_some() {
                self.set_text_content(id, content)?;
            }
            return self.set_props(
                id,
                &PropsPatch {
                    content: None,
                    ..patch.clone()
                },
            );
        }
        let tid = self.tid(id)?;
        let before = self.props(id)?;
        // Свойства текста есть только у текста, сеть — только у вектора: у
        // остальных узлов их не пишем.
        let stripped;
        let foreign_text = before.text.is_none() && patch.text.is_some();
        let foreign_vector = before.vector.is_none() && patch.vector.is_some();
        let patch = if foreign_text || foreign_vector {
            stripped = PropsPatch {
                text: if foreign_text {
                    None
                } else {
                    patch.text.clone()
                },
                vector: if foreign_vector {
                    None
                } else {
                    patch.vector.clone()
                },
                ..patch.clone()
            };
            &stripped
        } else {
            patch
        };
        if let Some(v) = &patch.vector {
            check_vector(v)?;
        }

        let mut after = before.clone();
        patch.apply_to(&mut after);
        if after == before {
            return Ok(());
        }
        codec::write_patch(&self.tree.get_meta(tid)?, patch)?;
        if let Some(props) = self.state.borrow_mut().props.get_mut(&id) {
            patch.apply_to(props);
        }
        self.record(Inverse::Props(id, Box::new(patch.previous(&before))));
        Ok(())
    }

    /// Превращает фигуру (прямоугольник, эллипс, линию) в вектор с сетью `net`
    /// на месте: тот же узел и ID, позиция среди соседей, имя, заливки,
    /// обводки и поза сохраняются; скругление и обрезка фигуры не нужны вектору.
    /// У вектора только заменяет сеть. Текст и контейнеры не превращаются.
    pub(crate) fn convert_to_vector(
        &self,
        id: NodeId,
        net: std::sync::Arc<VectorNetwork>,
    ) -> DocResult<()> {
        let before = self.props(id)?;
        if before.text.is_some() || matches!(before.kind, NodeKind::Frame | NodeKind::Group) {
            return Err(DocError::NotConvertible(id.to_string()));
        }
        check_vector(&net)?;
        let mut after = before;
        after.kind = NodeKind::Vector;
        after.corner_radius = 0.0;
        after.clip_content = false;
        after.vector = Some(net);
        self.set_shape(id, &after)
    }

    /// Записывает вид узла (тип, скругление, обрезку, сеть) по `to` и запоминает
    /// прежний для отмены. Без изменений ничего не пишет.
    fn set_shape(&self, id: NodeId, to: &Props) -> DocResult<()> {
        let tid = self.tid(id)?;
        let before = self.props(id)?;
        if before.kind == to.kind
            && before.corner_radius == to.corner_radius
            && before.clip_content == to.clip_content
            && before.vector == to.vector
        {
            return Ok(());
        }
        let meta = self.tree.get_meta(tid)?;
        meta.insert(codec::key::TYPE, to.kind.as_str())?;
        meta.insert(codec::key::CORNER_RADIUS, to.corner_radius)?;
        meta.insert(codec::key::CLIP_CONTENT, to.clip_content)?;
        match &to.vector {
            Some(v) => meta.insert(codec::key::VECTOR, LoroValue::Binary(v.encode().into()))?,
            None => meta.delete(codec::key::VECTOR)?,
        }
        if let Some(props) = self.state.borrow_mut().props.get_mut(&id) {
            props.kind = to.kind;
            props.corner_radius = to.corner_radius;
            props.clip_content = to.clip_content;
            props.vector = to.vector.clone();
        }
        self.record(Inverse::Shape(id, Box::new(before)));
        Ok(())
    }

    /// Применяет дельту к тексту узла и записывает обратную для отмены.
    pub(crate) fn apply_text_delta(&self, id: NodeId, delta: &[TextDelta]) -> DocResult<()> {
        if delta.is_empty() {
            return Ok(());
        }
        let mut grown: isize = 0;
        for op in delta {
            match op {
                TextDelta::Insert { insert, .. } => grown += insert.chars().count() as isize,
                TextDelta::Delete { delete } => grown -= *delete as isize,
                TextDelta::Retain { .. } => {}
            }
        }
        self.edit_text(id, grown, |text| {
            let inverse = text_codec::invert(text, delta)?;
            text.apply_delta(delta)?;
            Ok(inverse)
        })
    }

    /// Правка текста узла: `edit` меняет `LoroText` и возвращает обратную дельту.
    /// Пределы (спека 3): длина — до правки (`grown` — на сколько символов текст
    /// вырастет), участки со стилями — после (превышение откатывается обратной
    /// дельтой). Зеркало получает содержимое заново, отмена — обратную дельту.
    fn edit_text(
        &self,
        id: NodeId,
        grown: isize,
        edit: impl FnOnce(&LoroText) -> DocResult<Vec<TextDelta>>,
    ) -> DocResult<()> {
        let tid = self.tid(id)?;
        if self.props(id)?.text.is_none() {
            return Err(DocError::NotText(id.to_string()));
        }
        let text = text_codec::text_container(&self.tree.get_meta(tid)?)?;
        // Последний перевод строки — не символ пользователя: предел без него.
        if text.len_unicode() as isize + grown > MAX_TEXT_CHARS as isize + 1 {
            return Err(DocError::TextTooLong);
        }
        // (Тот же предел, что `check_chars`; здесь длина считается до правки.)
        let inverse = edit(&text)?;
        let content = text_codec::read_content(&text);
        if content.styled_runs() > MAX_TEXT_RUNS {
            // Откат не удался — Loro изменён без записи в журнал и зеркала.
            return Err(match text.apply_delta(&inverse) {
                Ok(()) => DocError::TooManyTextRuns,
                Err(e) => DocError::RollbackFailed {
                    original: Box::new(DocError::TooManyTextRuns),
                    rollback: Box::new(e.into()),
                },
            });
        }
        if let Some(props) = self.state.borrow_mut().props.get_mut(&id) {
            if let Some(t) = &props.text {
                // Новый `Arc`: прежний `TextData` не копируется целиком.
                props.text = Some(Arc::new(TextData {
                    props: t.props.clone(),
                    content,
                }));
            }
        }
        self.record(Inverse::Text(id, inverse));
        Ok(())
    }

    /// Длина текста узла в символах (с последним переводом строки).
    fn text_len(&self, id: NodeId) -> DocResult<usize> {
        match &self.props(id)?.text {
            Some(t) => Ok(t.content.char_len()),
            None => Err(DocError::NotText(id.to_string())),
        }
    }

    /// Вставка текста в позицию `at` (символы Unicode). `attrs`: `None` —
    /// разметка наследуется слева (расширение `After`), `Some` — ровно эта.
    /// После последнего перевода строки вставлять нельзя.
    pub(crate) fn insert_text(
        &self,
        id: NodeId,
        at: usize,
        s: &str,
        attrs: Option<&TextAttrs>,
    ) -> DocResult<()> {
        let len = self.text_len(id)?;
        if at >= len {
            return Err(DocError::TextRange(at, len));
        }
        if s.is_empty() {
            return Ok(());
        }
        let mut delta = Vec::new();
        if at > 0 {
            delta.push(TextDelta::Retain {
                retain: at,
                attributes: None,
            });
        }
        match attrs {
            // Дельта вставляет с точной разметкой (без унаследованной слева).
            Some(a) => {
                delta.push(TextDelta::Insert {
                    insert: s.to_string(),
                    attributes: Some(text_codec::attrs_to_loro(a)),
                });
                self.apply_text_delta(id, &delta)
            }
            // Обычная вставка Loro: разметка расширяется по `style_config`.
            None => {
                let n = s.chars().count();
                delta.push(TextDelta::Delete { delete: n });
                self.edit_text(id, n as isize, |text| {
                    text.insert(at, s)?;
                    Ok(delta)
                })
            }
        }
    }

    /// Вставка участков с точной разметкой каждого (вставка из буфера обмена со
    /// стилями) в позицию `at`; после последнего перевода строки — нельзя.
    pub(crate) fn insert_runs(&self, id: NodeId, at: usize, runs: &[TextRun]) -> DocResult<()> {
        let len = self.text_len(id)?;
        if at >= len {
            return Err(DocError::TextRange(at, len));
        }
        let mut delta = Vec::new();
        if at > 0 {
            delta.push(TextDelta::Retain {
                retain: at,
                attributes: None,
            });
        }
        for r in runs.iter().filter(|r| !r.text.is_empty()) {
            delta.push(TextDelta::Insert {
                insert: r.text.clone(),
                attributes: Some(text_codec::attrs_to_loro(&r.attrs)),
            });
        }
        if delta.len() == usize::from(at > 0) {
            return Ok(());
        }
        self.apply_text_delta(id, &delta)
    }

    /// Удаление диапазона символов. Последний перевод строки не удаляется.
    pub(crate) fn delete_text(&self, id: NodeId, range: Range<usize>) -> DocResult<()> {
        let len = self.text_len(id)?;
        if range.start > range.end || range.end >= len {
            return Err(DocError::TextRange(range.end, len));
        }
        if range.is_empty() {
            return Ok(());
        }
        let mut delta = Vec::new();
        if range.start > 0 {
            delta.push(TextDelta::Retain {
                retain: range.start,
                attributes: None,
            });
        }
        delta.push(TextDelta::Delete {
            delete: range.len(),
        });
        self.apply_text_delta(id, &delta)
    }

    /// Разметка диапазона: ключи из `set` задаются, ключи из `clear` снимаются.
    pub(crate) fn format_text(
        &self,
        id: NodeId,
        range: Range<usize>,
        set: &TextAttrs,
        clear: &[AttrKey],
    ) -> DocResult<()> {
        let len = self.text_len(id)?;
        if range.start > range.end || range.end > len {
            return Err(DocError::TextRange(range.end, len));
        }
        let mut attrs = text_codec::attrs_to_loro(set);
        for k in clear {
            if !set.has(*k) {
                attrs.insert(k.as_str().to_string(), LoroValue::Null);
            }
        }
        if range.is_empty() || attrs.is_empty() {
            return Ok(());
        }
        let mut delta = Vec::new();
        if range.start > 0 {
            delta.push(TextDelta::Retain {
                retain: range.start,
                attributes: None,
            });
        }
        delta.push(TextDelta::Retain {
            retain: range.len(),
            attributes: Some(attrs),
        });
        self.apply_text_delta(id, &delta)
    }

    /// Заменяет содержимое целиком (вставка из буфера, тесты). Текст без
    /// перевода строки в конце его получает.
    pub(crate) fn set_text_content(&self, id: NodeId, content: &TextContent) -> DocResult<()> {
        let len = self.text_len(id)?;
        let mut content = content.clone();
        if !content.text().ends_with('\n') {
            content.runs.push(crate::text::TextRun {
                text: "\n".into(),
                attrs: TextAttrs::default(),
            });
        }
        // Сравнение — со слитыми участками: иначе то же содержимое, разбитое
        // по-другому, давало бы пустой шаг отмены.
        content.normalize();
        if self.props(id)?.text.as_ref().map(|t| &t.content) == Some(&content) {
            return Ok(());
        }
        let mut delta = vec![TextDelta::Delete { delete: len }];
        delta.extend(text_codec::insert_delta(&content));
        self.apply_text_delta(id, &delta)
    }

    /// Позицию узла среди детей его родителя за O(log n) (см. `loro_ext`).
    fn index_of(&self, tid: TreeID) -> usize {
        loro_ext::index_of(&self.tree, tid).unwrap_or(0)
    }

    /// Где стоит узел: родитель и позиция среди его детей — та же, что в
    /// `children(parent)`. Один поиск узла и O(log n) на позицию: сцене это нужно,
    /// чтобы вставить созданный или перемещённый узел без перестроения списка
    /// соседей. `None` — узла нет. Позиция `None` — на странице есть скрытые узлы
    /// неизвестного типа: их позиции в Loro не совпадают с позициями в `children`,
    /// и вызывающий код должен перечитать список целиком.
    pub fn placement(&self, id: NodeId) -> Option<(Option<NodeId>, Option<usize>)> {
        let tid = self.tid(id).ok()?;
        let parent = match self.tree.parent(tid)? {
            TreeParentId::Node(p) => Some(self.node_of(p)),
            _ => None,
        };
        let position = if self.has_unknown_nodes() {
            None
        } else {
            loro_ext::index_of(&self.tree, tid)
        };
        Some((parent, position))
    }

    /// Переносит узел к новому родителю на позицию `index` среди его детей
    /// (позиция после операции). Значение за пределами допустимого диапазона
    /// означает «в конец списка», а не ошибку. Узел, уже бывший ребёнком этого
    /// родителя, при подсчёте позиций не учитывается.
    pub(crate) fn move_node(
        &self,
        id: NodeId,
        parent: Option<NodeId>,
        index: usize,
    ) -> DocResult<()> {
        self.move_nodes(&[id], parent, index)
    }

    /// Переносит узлы к родителю одним блоком: после операции они стоят подряд
    /// в порядке `ids`, первый — на позиции `index` среди детей родителя, не
    /// считая самих перемещаемых (за концом — в конец). Повторы в `ids` игнорируются.
    ///
    /// Стоимость — O(k · log n), без построения списков соседей: группировка тысячи
    /// узлов плоской страницы не должна перебирать сто тысяч соседей на каждый узел.
    /// Каждый узел записывает своё `Inverse::Move` с позицией прямо перед своим
    /// переносом, поэтому обратный проход восстанавливает порядок точно.
    pub(crate) fn move_nodes(
        &self,
        ids: &[NodeId],
        parent: Option<NodeId>,
        index: usize,
    ) -> DocResult<()> {
        let target = self.parent_tid(parent)?;
        let mut seen = HashSet::new();
        let mut moving: Vec<(NodeId, TreeID)> = Vec::with_capacity(ids.len());
        for id in ids {
            if seen.insert(*id) {
                moving.push((*id, self.tid(*id)?));
            }
        }
        // Проверка до первого переноса: отказ не должен оставлять полусделанную операцию.
        if let Some(p) = parent {
            let ancestors: HashSet<NodeId> =
                std::iter::successors(Some(p), |n| self.parent(*n)).collect();
            if moving.iter().any(|(id, _)| ancestors.contains(id)) {
                return Err(DocError::CyclicMove);
            }
        }
        match self.anchor(target, &moving, index) {
            // Каждый узел встаёт прямо перед якорем — блок складывается в порядке `ids`.
            Some(anchor) => {
                for &(id, tid) in &moving {
                    self.record_move(id, tid);
                    self.tree.mov_before(tid, anchor)?;
                }
            }
            None => {
                for &(id, tid) in &moving {
                    self.record_move(id, tid);
                    let end = self.child_count(target, Some(tid));
                    self.tree.mov_to(tid, target, end)?;
                }
            }
        }
        Ok(())
    }

    /// Ребёнок `target`, перед которым встанет блок: `index`-й среди детей, которые
    /// не перемещаются. `None` — блок встаёт в конец. Позиции перемещаемых детей
    /// `target` берутся по одной за O(log n), а не списком всех соседей.
    fn anchor(
        &self,
        target: TreeParentId,
        moving: &[(NodeId, TreeID)],
        index: usize,
    ) -> Option<TreeID> {
        let mut inside: Vec<usize> = moving
            .iter()
            .filter(|(_, tid)| self.tree.parent(*tid) == Some(target))
            .map(|(_, tid)| self.index_of(*tid))
            .collect();
        inside.sort_unstable();
        // `index`-й неперемещаемый ребёнок стоит на позиции `index` плюс число
        // перемещаемых детей, которые стоят не правее него.
        let mut at = index;
        for q in inside {
            if q <= at {
                // `index` за концом (вплоть до `usize::MAX`) значит «в конец»: без
                // насыщения сложение переполнилось бы и блок встал бы в начало.
                at = at.saturating_add(1);
            } else {
                break;
            }
        }
        loro_ext::child_at(&self.tree, target, at)
    }

    fn record_move(&self, id: NodeId, tid: TreeID) {
        self.record(Inverse::Move {
            id,
            parent: self.parent(id),
            index: self.index_of(tid),
        });
    }

    /// Удаляет узлы вместе с потомками. Уже удалённые пропускаются.
    ///
    /// Стоимость одного вызова — O(соседи + размер поддеревьев), а не O(k · соседи):
    /// удаление 1000 узлов среди 100 000 соседей не должно искать позицию и сдвигать
    /// список для каждого удаляемого узла. Поэтому ids группируются по родителю, список
    /// детей и карта позиций строятся один раз на группу, а внутри группы узлы
    /// удаляются по убыванию позиции: так исходная позиция каждого узла верна
    /// без пересчёта (всё, что левее, ещё на месте). Откат идёт в обратном порядке,
    /// то есть по возрастанию позиций, и возвращает каждый узел на его исходный индекс.
    ///
    /// Предок и потомок в одном вызове допустимы в любом порядке: потомок предка,
    /// удалённого раньше, пропускается (`tid` не найдёт его); потомок, удалённый
    /// раньше предка, уже не попадёт в обход поддерева предка.
    pub(crate) fn delete_nodes(&self, ids: &[NodeId]) -> DocResult<()> {
        let mut order: Vec<TreeParentId> = Vec::new();
        let mut groups: HashMap<TreeParentId, Vec<NodeId>> = HashMap::new();
        for id in ids {
            let Ok(tid) = self.tid(*id) else { continue };
            let tree_parent = self.tree.parent(tid).unwrap_or(TreeParentId::Root);
            groups
                .entry(tree_parent)
                .or_insert_with(|| {
                    order.push(tree_parent);
                    Vec::new()
                })
                .push(*id);
        }
        for tree_parent in order {
            // Родитель мог исчезнуть вместе с удалённым ранее предком: тогда `tid`
            // не найдёт ни одного узла группы, и список детей строить не нужно.
            let mut alive: Vec<(usize, NodeId, TreeID)> = Vec::new();
            let mut positions: Option<HashMap<TreeID, usize>> = None;
            for id in &groups[&tree_parent] {
                let Ok(tid) = self.tid(*id) else { continue };
                let positions = positions.get_or_insert_with(|| {
                    self.tree
                        .children(tree_parent)
                        .unwrap_or_default()
                        .into_iter()
                        .enumerate()
                        .map(|(i, t)| (t, i))
                        .collect()
                });
                alive.push((positions.get(&tid).copied().unwrap_or(0), *id, tid));
            }
            alive.sort_by_key(|(pos, _, _)| std::cmp::Reverse(*pos));
            alive.dedup_by_key(|(_, _, tid)| *tid);
            for (pos, id, tid) in alive {
                let id = &id;
                let mut saved = vec![SavedNode {
                    id: *id,
                    parent: self.parent(*id),
                    index: pos,
                    props: self.props(*id)?,
                }];
                // Поддерево обходим один раз в глубину: родитель раньше детей, дети
                // в порядке соседей. Индекс ребёнка берётся из его текущего списка Loro.
                let mut stack = Vec::new();
                self.push_children(tid, *id, &mut stack);
                while let Some((child, index, parent)) = stack.pop() {
                    let node = self.node_of(child);
                    saved.push(SavedNode {
                        id: node,
                        parent: Some(parent),
                        index,
                        props: self.props(node)?,
                    });
                    self.push_children(child, node, &mut stack);
                }
                self.tree.delete(tid)?;
                let mut state = self.state.borrow_mut();
                for node in &saved {
                    state.remove(node.id);
                }
                drop(state);
                self.record(Inverse::Restore(saved));
            }
        }
        Ok(())
    }

    /// Кладёт детей узла в стек с их индексами и `NodeId` родителя.
    fn push_children(&self, tid: TreeID, parent: NodeId, stack: &mut Vec<(TreeID, usize, NodeId)>) {
        let kids = self
            .tree
            .children(TreeParentId::Node(tid))
            .unwrap_or_default();
        for (i, child) in kids.into_iter().enumerate().rev() {
            stack.push((child, i, parent));
        }
    }

    /// `NodeId` по сырому `TreeID`; без записи в индексе — стабильный ID по `TreeID`.
    fn node_of(&self, tid: TreeID) -> NodeId {
        self.state
            .borrow()
            .by_tree
            .get(&tid)
            .copied()
            .unwrap_or(NodeId(tid))
    }

    /// Дети снизу вверх (последний рисуется поверх). `None` — корень страницы.
    pub fn children(&self, parent: Option<NodeId>) -> Vec<NodeId> {
        let Ok(parent) = self.parent_tid(parent) else {
            return Vec::new();
        };
        let state = self.state.borrow();
        // Узлы вне индекса (неизвестного типа) для модели не существуют.
        self.tree
            .children(parent)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|t| state.by_tree.get(&t).copied())
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

    /// Хэши изображений, на которые ссылаются заливки живых узлов страницы,
    /// включая узлы неизвестного типа, скрытые от модели.
    pub fn image_refs(&self) -> DocResult<Vec<String>> {
        let state = self.state.borrow();
        let mut refs: Vec<String> = state
            .props
            .values()
            .flat_map(|p| p.fills.iter().filter_map(|f| f.image_hash.clone()))
            .chain(self.hidden_refs.borrow().iter().cloned())
            .collect();
        refs.sort();
        refs.dedup();
        Ok(refs)
    }
}

impl<'a> Tx<'a> {
    pub(crate) fn new(page: &'a PageDoc) -> Tx<'a> {
        Tx { page }
    }

    /// Создаёт узел со свойствами по умолчанию. `index = None` — поверх всех детей.
    pub fn create_node(
        &self,
        parent: Option<NodeId>,
        index: Option<usize>,
        kind: NodeKind,
    ) -> DocResult<NodeId> {
        self.page.create_node(parent, index, kind)
    }

    pub fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        self.page.set_props(id, patch)
    }

    /// Фигура становится вектором с сетью `net` на месте: тот же узел, ID и
    /// позиция среди соседей (см. `PageDoc::convert_to_vector`).
    pub fn convert_to_vector(
        &self,
        id: NodeId,
        net: std::sync::Arc<crate::vector::VectorNetwork>,
    ) -> DocResult<()> {
        self.page.convert_to_vector(id, net)
    }

    /// Узел сразу со всеми свойствами (копия, вставка; у текста — с содержимым).
    pub fn create_with_props(
        &self,
        parent: Option<NodeId>,
        index: Option<usize>,
        props: &Props,
    ) -> DocResult<NodeId> {
        self.page.create_with_props(parent, index, props)
    }

    /// См. [`Tx::move_nodes`]: перенос одного узла.
    pub fn move_node(&self, id: NodeId, parent: Option<NodeId>, index: usize) -> DocResult<()> {
        self.page.move_node(id, parent, index)
    }

    /// Переносит узлы одним блоком: после операции они стоят подряд в порядке `ids`,
    /// первый — на позиции `index` среди остальных детей родителя (за концом — в конец).
    pub fn move_nodes(
        &self,
        ids: &[NodeId],
        parent: Option<NodeId>,
        index: usize,
    ) -> DocResult<()> {
        self.page.move_nodes(ids, parent, index)
    }

    /// Удаляет узлы вместе с потомками. Уже удалённые пропускаются.
    pub fn delete_nodes(&self, ids: &[NodeId]) -> DocResult<()> {
        self.page.delete_nodes(ids)
    }

    /// Вставка в текст узла (позиции — символы Unicode). `attrs`: `None` —
    /// разметка наследуется слева, `Some` — ровно эта.
    pub fn insert_text(
        &self,
        id: NodeId,
        at: usize,
        text: &str,
        attrs: Option<&TextAttrs>,
    ) -> DocResult<()> {
        self.page.insert_text(id, at, text, attrs)
    }

    /// Удаление символов текста (последний перевод строки не удаляется).
    pub fn delete_text(&self, id: NodeId, range: Range<usize>) -> DocResult<()> {
        self.page.delete_text(id, range)
    }

    /// Разметка диапазона текста: задать ключи `set`, снять ключи `clear`.
    pub fn format_text(
        &self,
        id: NodeId,
        range: Range<usize>,
        set: &TextAttrs,
        clear: &[AttrKey],
    ) -> DocResult<()> {
        self.page.format_text(id, range, set, clear)
    }

    /// Содержимое текста целиком.
    pub fn set_text_content(&self, id: NodeId, content: &TextContent) -> DocResult<()> {
        self.page.set_text_content(id, content)
    }

    /// Вставка участков с их разметкой в позицию `at` (символы Unicode).
    pub fn insert_runs(&self, id: NodeId, at: usize, runs: &[TextRun]) -> DocResult<()> {
        self.page.insert_runs(id, at, runs)
    }
}

impl std::ops::Deref for Tx<'_> {
    type Target = PageDoc;

    fn deref(&self) -> &PageDoc {
        self.page
    }
}

impl Default for PageDoc {
    fn default() -> Self {
        PageDoc::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props_of(page: &PageDoc, ids: &[NodeId]) -> Vec<Props> {
        ids.iter().map(|id| page.props(*id).unwrap()).collect()
    }

    #[test]
    fn oversized_text_in_scene_cache_is_detected() {
        let page = PageDoc::new();
        let id = page.create_node(None, None, NodeKind::Text).unwrap();
        let snapshot = page.export_snapshot().unwrap();
        let mut rows = page.scene_rows().unwrap();
        let ok = PageDoc::load(&snapshot, Some(&rows)).unwrap();
        assert!(ok.loaded_from_cache());
        assert!(!ok.has_oversized_text());
        // Строка кэша из файла несёт текст сверх предела: дерево Loro то же, кэш
        // принимается, но страница должна стать «только для чтения».
        let row = rows.iter_mut().find(|r| r.id == id).unwrap();
        let mut data = (**row.props.text.as_ref().unwrap()).clone();
        data.content = TextContent::plain(&"я".repeat(MAX_TEXT_CHARS + 1));
        row.props.text = Some(Arc::new(data));
        let bad = PageDoc::load(&snapshot, Some(&rows)).unwrap();
        assert!(bad.loaded_from_cache());
        assert!(bad.has_oversized_text());
    }

    #[test]
    fn delete_siblings_then_inverse_restores_order_and_ids() {
        let page = PageDoc::new();
        let a = page.create_node(None, None, NodeKind::Rect).unwrap();
        let b = page.create_node(None, None, NodeKind::Frame).unwrap();
        let c = page.create_node(None, None, NodeKind::Rect).unwrap();
        let d = page.create_node(None, None, NodeKind::Ellipse).unwrap();
        let b1 = page.create_node(Some(b), None, NodeKind::Rect).unwrap();
        let b2 = page.create_node(Some(b), None, NodeKind::Line).unwrap();
        let all = [a, b, b1, b2, c, d];
        let before = props_of(&page, &all);

        page.begin_journal();
        page.delete_nodes(&[a, c, b]).unwrap();
        let ops = page.take_journal();
        page.apply_inverse(&ops).unwrap();

        assert_eq!(page.children(None), vec![a, b, c, d]);
        assert_eq!(page.children(Some(b)), vec![b1, b2]);
        for id in all {
            assert!(page.contains(id));
        }
        assert_eq!(props_of(&page, &all), before);
    }

    #[test]
    fn delete_ancestor_and_descendant_in_one_call() {
        for child_first in [true, false] {
            let page = PageDoc::new();
            let a = page.create_node(None, None, NodeKind::Rect).unwrap();
            let parent = page.create_node(None, None, NodeKind::Frame).unwrap();
            let c = page.create_node(None, None, NodeKind::Rect).unwrap();
            let k1 = page
                .create_node(Some(parent), None, NodeKind::Rect)
                .unwrap();
            let child = page
                .create_node(Some(parent), None, NodeKind::Frame)
                .unwrap();
            let k3 = page
                .create_node(Some(parent), None, NodeKind::Rect)
                .unwrap();
            let grandchild = page.create_node(Some(child), None, NodeKind::Rect).unwrap();
            let all = [a, parent, c, k1, child, k3, grandchild];
            let before = props_of(&page, &all);

            page.begin_journal();
            if child_first {
                page.delete_nodes(&[child, parent]).unwrap();
            } else {
                page.delete_nodes(&[parent, child]).unwrap();
            }
            let ops = page.take_journal();
            assert_eq!(page.children(None), vec![a, c]);
            for id in [parent, k1, child, k3, grandchild] {
                assert!(!page.contains(id));
            }
            page.apply_inverse(&ops).unwrap();

            assert_eq!(page.children(None), vec![a, parent, c]);
            assert_eq!(page.children(Some(parent)), vec![k1, child, k3]);
            assert_eq!(page.children(Some(child)), vec![grandchild]);
            assert_eq!(props_of(&page, &all), before);
        }
    }

    #[test]
    fn move_then_inverse_restores_order() {
        let page = PageDoc::new();
        let a = page.create_node(None, None, NodeKind::Rect).unwrap();
        let b = page.create_node(None, None, NodeKind::Rect).unwrap();
        let c = page.create_node(None, None, NodeKind::Rect).unwrap();
        let d = page.create_node(None, None, NodeKind::Rect).unwrap();

        page.begin_journal();
        page.move_node(d, None, 0).unwrap();
        let ops = page.take_journal();
        page.apply_inverse(&ops).unwrap();
        assert_eq!(page.children(None), vec![a, b, c, d]);

        page.begin_journal();
        page.move_node(a, None, 3).unwrap();
        let ops = page.take_journal();
        page.apply_inverse(&ops).unwrap();
        assert_eq!(page.children(None), vec![a, b, c, d]);
    }

    #[test]
    fn deleting_many_flat_siblings() {
        let page = PageDoc::new();
        let all: Vec<NodeId> = (0..20_000)
            .map(|_| page.create_node(None, None, NodeKind::Rect).unwrap())
            .collect();
        let doomed: Vec<NodeId> = all.iter().copied().step_by(20).collect();
        let kept: Vec<NodeId> = all
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, _)| i % 20 != 0)
            .map(|(_, id)| id)
            .collect();

        page.begin_journal();
        page.delete_nodes(&doomed).unwrap();
        let ops = page.take_journal();
        assert_eq!(page.children(None), kept);
        assert_eq!(page.children(None).len(), 19_000);

        page.apply_inverse(&ops).unwrap();
        assert_eq!(page.children(None), all);
    }

    #[test]
    fn move_index_past_end_goes_last() {
        let page = PageDoc::new();
        let a = page.create_node(None, None, NodeKind::Rect).unwrap();
        let b = page.create_node(None, None, NodeKind::Rect).unwrap();
        let c = page.create_node(None, None, NodeKind::Rect).unwrap();
        let other = page.create_node(None, None, NodeKind::Frame).unwrap();
        let before = props_of(&page, &[a, b, c, other]);

        // Тот же родитель: позиция 10 среди [a, b, c] — в конец, без ошибки.
        page.begin_journal();
        page.move_node(a, None, 10).unwrap();
        let ops = page.take_journal();
        assert_eq!(page.children(None), vec![b, c, other, a]);
        page.apply_inverse(&ops).unwrap();
        assert_eq!(page.children(None), vec![a, b, c, other]);
        assert_eq!(props_of(&page, &[a, b, c, other]), before);

        // Родитель без детей: позиция 5 — единственный ребёнок.
        let frame = page.create_node(None, None, NodeKind::Frame).unwrap();
        page.move_node(b, Some(frame), 5).unwrap();
        assert_eq!(page.children(Some(frame)), vec![b]);
    }

    #[test]
    fn move_nodes_places_block_in_given_order_and_inverse_restores() {
        let page = PageDoc::new();
        let ids: Vec<NodeId> = (0..5)
            .map(|_| page.create_node(None, None, NodeKind::Rect).unwrap())
            .collect();
        let [a, b, c, d, e] = ids[..] else {
            unreachable!()
        };

        // Тот же родитель: блок [d, a] встаёт на позицию 1 среди оставшихся [b, c, e].
        page.begin_journal();
        page.move_nodes(&[d, a, d], None, 1).unwrap();
        let ops = page.take_journal();
        assert_eq!(page.children(None), vec![b, d, a, c, e]);
        page.apply_inverse(&ops).unwrap();
        assert_eq!(page.children(None), ids);

        // В новый фрейм (Ctrl+G): порядок блока — порядок аргумента.
        let frame = page.create_node(None, None, NodeKind::Frame).unwrap();
        page.begin_journal();
        page.move_nodes(&[e, b, d], Some(frame), 0).unwrap();
        let ops = page.take_journal();
        assert_eq!(page.children(None), vec![a, c, frame]);
        assert_eq!(page.children(Some(frame)), vec![e, b, d]);
        page.apply_inverse(&ops).unwrap();
        assert_eq!(page.children(None), vec![a, b, c, d, e, frame]);
        assert!(page.children(Some(frame)).is_empty());

        // Позиция за концом — в конец.
        page.move_nodes(&[a, b], None, 99).unwrap();
        assert_eq!(page.children(None), vec![c, d, e, frame, a, b]);
    }

    #[test]
    fn move_nodes_rejects_cycle_before_moving_anything() {
        let page = PageDoc::new();
        let outer = page.create_node(None, None, NodeKind::Frame).unwrap();
        let inner = page
            .create_node(Some(outer), None, NodeKind::Frame)
            .unwrap();
        let other = page.create_node(None, None, NodeKind::Rect).unwrap();
        assert!(matches!(
            page.move_nodes(&[other, outer], Some(inner), 0),
            Err(DocError::CyclicMove)
        ));
        assert_eq!(page.children(None), vec![outer, other]);
        assert!(matches!(
            page.move_nodes(&[outer], Some(outer), 0),
            Err(DocError::CyclicMove)
        ));
    }

    /// Страница из фрейма с двумя детьми и прямоугольника в корне, зафиксированная.
    fn small_page() -> PageDoc {
        let page = PageDoc::new();
        let f = page.create_node(None, None, NodeKind::Frame).unwrap();
        page.create_node(Some(f), None, NodeKind::Rect).unwrap();
        page.create_node(Some(f), None, NodeKind::Ellipse).unwrap();
        page.create_node(None, None, NodeKind::Rect).unwrap();
        page.commit();
        page
    }

    #[test]
    fn forged_scene_cache_falls_back_to_loro() {
        let page = small_page();
        let snapshot = page.export_snapshot().unwrap();
        let rows = page.scene_rows().unwrap();
        assert_eq!(rows.len(), 4);

        let mut missing = rows.clone();
        missing.pop();
        let mut duplicate = rows.clone();
        duplicate.push(rows[1].clone());
        let mut same_id = rows.clone();
        same_id[3].id = rows[0].id;
        let mut wrong_parent = rows.clone();
        wrong_parent[1].parent = None;
        let mut dead_tree = rows.clone();
        dead_tree[3].tree_id = small_page().scene_rows().unwrap()[3].tree_id;

        for (name, forged) in [
            ("строки не хватает", missing),
            ("строка повторена", duplicate),
            ("NodeId повторён", same_id),
            ("чужой родитель", wrong_parent),
            ("TreeID не из этой страницы", dead_tree),
        ] {
            let loaded = PageDoc::load(&snapshot, Some(&forged)).unwrap();
            assert!(!loaded.loaded_from_cache(), "{name}");
            assert_eq!(loaded.walk(), page.walk(), "{name}");
            assert_eq!(loaded.scene_rows().unwrap(), rows, "{name}");
            assert!(loaded.export_snapshot().is_ok(), "{name}");
        }
        // Честный кэш принимается как есть.
        let loaded = PageDoc::load(&snapshot, Some(&rows)).unwrap();
        assert!(loaded.loaded_from_cache());
        assert_eq!(loaded.scene_rows().unwrap(), rows);
    }

    #[test]
    fn unknown_kind_hides_subtree_and_keeps_other_nodes() {
        let page = PageDoc::new();
        let f = page.create_node(None, None, NodeKind::Frame).unwrap();
        let a = page.create_node(Some(f), None, NodeKind::Rect).unwrap();
        let star = page.create_node(Some(f), None, NodeKind::Group).unwrap();
        page.create_node(Some(star), None, NodeKind::Rect).unwrap();
        let r = page.create_node(None, None, NodeKind::Ellipse).unwrap();
        page.set_props(
            a,
            &PropsPatch {
                x: Some(7.0),
                ..Default::default()
            },
        )
        .unwrap();
        let before = props_of(&page, &[f, a, r]);
        assert!(!page.has_unknown_nodes());
        page.force_raw_kind_for_tests(star, Some("star"));

        let loaded = PageDoc::from_snapshot(&page.export_snapshot().unwrap()).unwrap();
        assert!(loaded.has_unknown_nodes());
        assert_eq!(loaded.walk(), vec![f, a, r]);
        assert_eq!(loaded.children(Some(f)), vec![a]);
        assert!(!loaded.contains(star));
        assert_eq!(props_of(&loaded, &[f, a, r]), before);
        assert_eq!(loaded.scene_rows().unwrap().len(), 3);
    }

    #[test]
    fn missing_kind_is_corruption() {
        let page = PageDoc::new();
        let a = page.create_node(None, None, NodeKind::Rect).unwrap();
        page.force_raw_kind_for_tests(a, None);
        assert!(matches!(
            PageDoc::from_snapshot(&page.export_snapshot().unwrap()),
            Err(DocError::Corrupted(_))
        ));
    }

    #[test]
    fn failed_apply_inverse_returns_page_to_previous_state() {
        let page = small_page();
        let all = page.walk();
        let before = props_of(&page, &all);
        let missing = PageDoc::new()
            .create_node(None, None, NodeKind::Rect)
            .unwrap();
        // Применяются с конца: перенос, правка свойств и удаление проходят, затем сбой.
        let ops = vec![
            Inverse::Props(
                missing,
                Box::new(PropsPatch {
                    x: Some(1.0),
                    ..Default::default()
                }),
            ),
            Inverse::Delete(all[3]),
            Inverse::Props(
                all[1],
                Box::new(PropsPatch {
                    x: Some(5.0),
                    ..Default::default()
                }),
            ),
            Inverse::Move {
                id: all[2],
                parent: None,
                index: 0,
            },
        ];
        assert!(matches!(
            page.apply_inverse(&ops),
            Err(DocError::NodeNotFound(_))
        ));
        assert_eq!(page.walk(), all);
        assert_eq!(props_of(&page, &all), before);
    }

    #[test]
    fn create_index_past_end_goes_last() {
        let page = PageDoc::new();
        let a = page.create_node(None, None, NodeKind::Rect).unwrap();
        let b = page.create_node(None, None, NodeKind::Rect).unwrap();
        let c = page.create_node(None, Some(99), NodeKind::Rect).unwrap();
        assert_eq!(page.children(None), vec![a, b, c]);
    }
}
