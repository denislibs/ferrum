//! Документ целиком: корневой `LoroDoc` (метаданные, список страниц) и по одному
//! `PageDoc` на страницу. Страницы загружаются по требованию.

use std::collections::HashMap;

use loro::{
    ExportMode, LoroDoc, LoroTree, LoroValue, TreeParentId, ValueOrContainer, VersionVector,
};

use crate::changes::Changes;
use crate::codec::key::GUIDES as KEY_GUIDES;
use crate::error::{DocError, DocResult};
use crate::guides::{check_guides, dedup_guides, guides_value, read_guides, Guide};
use crate::history::{DocOp, History, HistoryEntry, TextMerge};
use crate::images::ImageStore;
use crate::journal::{self, BatchWriter, Replayed};
use crate::page::{Inverse, PageDoc, Tx};
use crate::scene_cache::SceneRow;
use crate::sync::{
    self, Cursor, DocKey, RemoteOutcome, SyncBatch, SyncCursors, SyncVersion, TextShift,
};
use crate::types::{Color, NodeId, PageId, PropsPatch};

/// Текущая версия схемы документа. Растёт при несовместимых изменениях схемы.
pub const SCHEMA_VERSION: u32 = 1;

const META: &str = "meta";
const PAGES: &str = "pages";
const KEY_SCHEMA: &str = "schemaVersion";
const KEY_NAME: &str = "name";
/// Цвет фона страницы (`#rrggbbaa`) в метаданных её узла в дереве страниц. Нет
/// ключа — фон как у холста в теме интерфейса (нейтральное значение не пишется).
const KEY_BACKGROUND: &str = "background";

/// Страница, ещё не загруженная из файла.
pub(crate) struct UnloadedPage {
    pub snapshot: Vec<u8>,
    pub image_refs: Vec<String>,
    /// Кэш сцены из файла, если он совпал со снапшотом.
    pub scene: Option<Vec<SceneRow>>,
}

/// Что пишется в снимок страницы: только состояние или состояние с историей правок.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SnapshotKind {
    Shallow,
    Full,
}

/// Всё, что пишется в файл для одной страницы.
pub(crate) struct PagePayload {
    pub snapshot: Vec<u8>,
    pub image_refs: Vec<String>,
    /// `None` — кэша нет (страница не открывалась); секция кэша тогда не пишется.
    pub scene: Option<Vec<SceneRow>>,
}

/// Что восстановить в интерфейсе после undo/redo.
#[derive(Debug, Clone, PartialEq)]
pub struct Restore {
    pub page: PageId,
    pub selection: Vec<NodeId>,
}

/// Чьи направляющие: страницы или фрейма на странице шага.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuideOwner {
    Page(PageId),
    Frame(NodeId),
}

/// Что вернул повтор или отмена шага: операции обратного направления и новые
/// идентификаторы страниц.
struct Replay {
    ops: Vec<Inverse>,
    doc_ops: Vec<DocOp>,
    remap: Option<(PageId, PageId)>,
}

pub struct Document {
    root: LoroDoc,
    pages_tree: LoroTree,
    loaded: HashMap<PageId, PageDoc>,
    unloaded: HashMap<PageId, UnloadedPage>,
    history: History,
    /// Изменённые узлы по страницам с последнего `take_changes`.
    changes: HashMap<PageId, Changes>,
    pub images: ImageStore,
    read_only: bool,
    /// Режим просмотра: так решил не файл, а роль человека (читатель, спека подсистемы 4,
    /// раздел 6.1). В отличие от `read_only` снимается, когда роль повысили.
    view_only: bool,
    /// Журнал (этап 6): версии Loro корня и загруженных страниц, до которых правки
    /// уже отданы в журнал (`take_journal`). Страница получает версию при загрузке.
    journaled_root: VersionVector,
    journaled_pages: HashMap<PageId, VersionVector>,
    /// Копии страниц, убранные отменой «Дублировать страницу», ждут повтора. Живут,
    /// только пока есть шаги повтора.
    parked: HashMap<PageId, PageDoc>,
    /// Растёт при каждой смене направляющих страницы (кэш линий редактора):
    /// у фрейма то же самое видно по счётчику сцены.
    guides_revision: u64,
    /// Файл на сервере: курсоры синхронизации. `None` — локальный файл, чужих правок не бывает.
    sync: Option<SyncCursors>,
    /// Файл на сервере: журналу не хватает правок, которые ещё придут с сервера, это не повреждение.
    catches_up: bool,
}

impl Document {
    pub fn new() -> Document {
        let root = LoroDoc::new();
        let meta = root.get_map(META);
        meta.insert(KEY_SCHEMA, SCHEMA_VERSION as i64)
            .expect("новый документ");
        meta.insert(KEY_NAME, "Без названия")
            .expect("новый документ");
        let mut doc = Document::wrap(root, HashMap::new(), ImageStore::default());
        doc.add_page("Страница 1").expect("новый документ");
        doc
    }

    fn wrap(
        root: LoroDoc,
        unloaded: HashMap<PageId, UnloadedPage>,
        images: ImageStore,
    ) -> Document {
        let pages_tree = root.get_tree(PAGES);
        pages_tree.enable_fractional_index(0);
        let journaled_root = root.oplog_vv();
        Document {
            root,
            pages_tree,
            loaded: HashMap::new(),
            unloaded,
            history: History::default(),
            changes: HashMap::new(),
            images,
            read_only: false,
            view_only: false,
            journaled_root,
            journaled_pages: HashMap::new(),
            parked: HashMap::new(),
            guides_revision: 0,
            sync: None,
            catches_up: false,
        }
    }

    /// Собирает документ из прочитанного файла. Возвращает предупреждения.
    pub(crate) fn from_parts(
        root_snapshot: &[u8],
        unloaded: HashMap<PageId, UnloadedPage>,
        images: ImageStore,
    ) -> DocResult<(Document, Vec<String>)> {
        let root = LoroDoc::from_snapshot(root_snapshot)?;
        let mut doc = Document::wrap(root, unloaded, images);
        let mut warnings = Vec::new();
        let version = doc.schema_version();
        if version > SCHEMA_VERSION {
            doc.read_only = true;
            warnings.push(format!(
                "Файл создан в более новой версии Ferrum (схема {version}). Он открыт только для чтения"
            ));
        }
        for (id, name) in doc.pages() {
            if !doc.unloaded.contains_key(&id) {
                let name = if name.is_empty() {
                    "без названия"
                } else {
                    &name
                };
                warnings.push(format!(
                    "Файл повреждён: не удалось прочитать страницу «{name}». Она открыта пустой"
                ));
                doc.loaded.insert(id, PageDoc::new());
                doc.journaled_pages.insert(id, VersionVector::default());
            }
        }
        Ok((doc, warnings))
    }

    pub(crate) fn root_snapshot(&self) -> DocResult<Vec<u8>> {
        Ok(self.root.export(loro::ExportMode::Snapshot)?)
    }

    pub fn schema_version(&self) -> u32 {
        match self.root.get_map(META).get(KEY_SCHEMA) {
            // Значение вне u32 (отрицательное или слишком большое) нельзя усекать:
            // 2^32 + 1 стало бы версией 1. Считаем его версией «из будущего» —
            // документ откроется только для чтения.
            Some(ValueOrContainer::Value(LoroValue::I64(v))) => {
                u32::try_from(v).unwrap_or(u32::MAX)
            }
            _ => SCHEMA_VERSION,
        }
    }

    /// Только для тестов: версия схемы, как будто корень правит более новая сборка.
    #[doc(hidden)]
    pub fn force_schema_version_for_tests(&mut self, version: u32) {
        self.force_schema_version(version);
    }

    pub(crate) fn force_schema_version(&mut self, version: u32) {
        let meta = self.root.get_map(META);
        meta.insert(KEY_SCHEMA, version as i64)
            .expect("запись в корневой документ");
        self.root.commit();
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only || self.view_only
    }

    /// Режим просмотра по роли: правки отвергаются так же, как у файла только для чтения.
    pub fn set_view_only(&mut self, on: bool) {
        self.view_only = on;
    }

    /// Записывает в страницу направляющие как есть — как чужой файл (тесты разбора).
    #[doc(hidden)]
    pub fn force_page_guides_for_tests(&mut self, id: PageId, raw: &[(&str, f64)]) {
        let meta = self.pages_tree.get_meta(id.0).expect("страница есть");
        meta.insert(KEY_GUIDES, crate::guides::raw_value(raw))
            .expect("запись");
        self.root.commit();
    }

    /// Только для тестов: документ становится «только чтение», как при
    /// загрузке страницы с неизвестными узлами.
    #[doc(hidden)]
    pub fn force_read_only_for_tests(&mut self) {
        self.read_only = true;
    }

    fn writable(&self) -> DocResult<()> {
        if self.is_read_only() {
            Err(DocError::ReadOnly)
        } else {
            Ok(())
        }
    }

    pub fn name(&self) -> String {
        match self.root.get_map(META).get(KEY_NAME) {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => s.to_string(),
            _ => String::new(),
        }
    }

    pub fn set_name(&mut self, name: &str) -> DocResult<()> {
        self.writable()?;
        self.root.get_map(META).insert(KEY_NAME, name)?;
        self.root.commit();
        Ok(())
    }

    /// Страницы в порядке списка.
    pub fn pages(&self) -> Vec<(PageId, String)> {
        self.pages_tree
            .children(TreeParentId::Root)
            .unwrap_or_default()
            .into_iter()
            .map(|t| {
                let name = match self
                    .pages_tree
                    .get_meta(t)
                    .ok()
                    .and_then(|m| m.get(KEY_NAME))
                {
                    Some(ValueOrContainer::Value(LoroValue::String(s))) => s.to_string(),
                    _ => String::new(),
                };
                (PageId(t), name)
            })
            .collect()
    }

    /// Операции со страницами в первой версии не попадают в undo.
    pub fn add_page(&mut self, name: &str) -> DocResult<PageId> {
        self.writable()?;
        let t = self.pages_tree.create(TreeParentId::Root)?;
        self.pages_tree.get_meta(t)?.insert(KEY_NAME, name)?;
        self.root.commit();
        let id = PageId(t);
        self.loaded.insert(id, PageDoc::new());
        // Новая страница целиком попадёт в следующую пачку журнала.
        self.journaled_pages.insert(id, VersionVector::default());
        Ok(id)
    }

    /// Копия страницы `id` с именем `name` сразу после неё в списке: те же узлы
    /// (с новыми идентификаторами), фон и направляющие. Один шаг отмены (спека
    /// подсистемы 2, раздел 10): отмена возвращает на `from` с выделением
    /// `selection`, повтор — на копию. Копия целиком уйдёт в следующую пачку журнала.
    pub fn duplicate_page(
        &mut self,
        id: PageId,
        name: &str,
        from: PageId,
        selection: Vec<NodeId>,
    ) -> DocResult<PageId> {
        // Сначала загрузка: страница с неизвестными узлами делает документ
        // доступным только для чтения, а копия их потеряла бы.
        self.page(id)?;
        self.writable()?;
        let at = self
            .pages()
            .iter()
            .position(|(p, _)| *p == id)
            .map_or(0, |i| i + 1);
        self.ensure_page(from)?;
        let background = self.page_background(id);
        let guides = self.page_guides(id);
        // Узлы копируются до создания страницы: отказ не оставляет пустой копии.
        let copy = PageDoc::new();
        {
            let src = self.loaded.get(&id).expect("страница загружена выше");
            let tx = Tx::new(&copy);
            // Обход без рекурсии: глубина не ограничена.
            let mut stack: Vec<(Option<NodeId>, Option<NodeId>)> = vec![(None, None)];
            while let Some((from, to)) = stack.pop() {
                for child in src.children(from) {
                    let made = tx.create_with_props(to, None, &src.props(child)?)?;
                    stack.push((Some(child), Some(made)));
                }
            }
        }
        copy.commit();
        let t = self.pages_tree.create_at(TreeParentId::Root, at)?;
        let meta = self.pages_tree.get_meta(t)?;
        meta.insert(KEY_NAME, name)?;
        if let Some(c) = background {
            meta.insert(KEY_BACKGROUND, c.to_hex())?;
        }
        if !guides.is_empty() {
            meta.insert(KEY_GUIDES, guides_value(&guides))?;
        }
        self.root.commit();
        let new = PageId(t);
        self.loaded.insert(new, copy);
        self.journaled_pages.insert(new, VersionVector::default());
        self.push_step(HistoryEntry {
            id: 0,
            page: from,
            page_after: Some(new),
            ops: Vec::new(),
            doc_ops: vec![DocOp::RemovePage { page: new }],
            selection_before: selection,
            selection_after: Vec::new(),
        });
        Ok(new)
    }

    /// Ставит страницу на место `to` в списке (после перестановки она там и
    /// стоит). Один шаг отмены; то же место — ни шага, ни правки. Отмена возвращает
    /// на страницу `current` с выделением `selection`.
    pub fn move_page(
        &mut self,
        id: PageId,
        to: usize,
        current: PageId,
        selection: Vec<NodeId>,
    ) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        let from = self.page_index(id)?;
        let to = to.min(self.pages().len().saturating_sub(1));
        if from == to {
            return Ok(());
        }
        self.place_page(id, to)?;
        self.push_step(HistoryEntry {
            id: 0,
            page: current,
            page_after: None,
            ops: Vec::new(),
            doc_ops: vec![DocOp::MovePage {
                page: id,
                index: from,
            }],
            selection_after: selection.clone(),
            selection_before: selection,
        });
        Ok(())
    }

    fn page_index(&self, id: PageId) -> DocResult<usize> {
        self.pages()
            .iter()
            .position(|(p, _)| *p == id)
            .ok_or_else(|| DocError::PageNotFound(id.to_string()))
    }

    /// Место за концом списка (после удаления другой страницы) — последнее.
    fn place_page(&mut self, id: PageId, to: usize) -> DocResult<()> {
        let to = to.min(self.pages().len().saturating_sub(1));
        self.pages_tree.mov_to(id.0, TreeParentId::Root, to)?;
        self.root.commit();
        Ok(())
    }

    /// Направляющие страницы. Неверные записи файла отброшены (`read_guides`);
    /// страницу загружать не нужно: список лежит в корневом документе.
    pub fn page_guides(&self, id: PageId) -> Vec<Guide> {
        match self
            .pages_tree
            .get_meta(id.0)
            .ok()
            .and_then(|m| m.get(KEY_GUIDES))
        {
            Some(ValueOrContainer::Value(v)) => read_guides(&v),
            _ => Vec::new(),
        }
    }

    /// Номер версии направляющих страниц: кэш редактора сравнивает его за O(1).
    pub fn guides_revision(&self) -> u64 {
        self.guides_revision
    }

    fn write_page_guides(&mut self, id: PageId, guides: &[Guide]) -> DocResult<()> {
        let meta = self.pages_tree.get_meta(id.0)?;
        if guides.is_empty() {
            if meta.get(KEY_GUIDES).is_some() {
                meta.delete(KEY_GUIDES)?;
            }
        } else {
            meta.insert(KEY_GUIDES, guides_value(guides))?;
        }
        self.root.commit();
        self.guides_revision += 1;
        Ok(())
    }

    /// Новые списки направляющих одним шагом отмены (перенос из страницы во фрейм
    /// — два списка). Без изменений шага нет. Неверный список, неизвестный узел и
    /// узел не того типа — отказ целиком: ни правки, ни шага. Сначала загрузка
    /// страницы: на незагруженной могут быть неизвестные узлы, и документ станет
    /// доступен только для чтения.
    pub fn set_guides(
        &mut self,
        page: PageId,
        selection: Vec<NodeId>,
        edits: &[(GuideOwner, Vec<Guide>)],
    ) -> DocResult<()> {
        self.page(page)?;
        self.writable()?;
        let mut frames = Vec::new();
        let mut doc_ops = Vec::new();
        let mut page_edit: Vec<(PageId, Vec<Guide>)> = Vec::new();
        for (owner, guides) in edits {
            check_guides(guides)?;
            let guides = dedup_guides(guides.iter().copied());
            match *owner {
                GuideOwner::Page(id) => {
                    self.ensure_page(id)?;
                    if id != page {
                        return Err(DocError::PageNotFound(id.to_string()));
                    }
                    let before = self.page_guides(id);
                    if before != guides {
                        doc_ops.push(DocOp::PageGuides {
                            page: id,
                            guides: before,
                        });
                        page_edit.push((id, guides));
                    }
                }
                GuideOwner::Frame(id) => {
                    let p = self.loaded.get(&page).expect("страница загружена выше");
                    if p.kind(id)? != crate::types::NodeKind::Frame {
                        return Err(DocError::InvalidGuides("у этого узла их не бывает".into()));
                    }
                    frames.push((id, guides));
                }
            }
        }
        let p = self.loaded.get(&page).expect("страница загружена выше");
        p.begin_journal();
        let tx = Tx::new(p);
        let result = frames.iter().try_for_each(|(id, guides)| {
            tx.set_props(
                *id,
                &PropsPatch {
                    guides: Some(std::sync::Arc::new(guides.clone())),
                    ..Default::default()
                },
            )
        });
        let ops = p.take_journal();
        p.commit();
        self.changes.entry(page).or_default().record(&ops);
        if let Err(e) = result {
            return Err(rollback(p, &ops, e));
        }
        for (id, guides) in &page_edit {
            self.write_page_guides(*id, guides)?;
        }
        if !ops.is_empty() || !doc_ops.is_empty() {
            self.push_step(HistoryEntry {
                id: 0,
                page,
                page_after: None,
                ops,
                doc_ops,
                selection_after: selection.clone(),
                selection_before: selection,
            });
        }
        Ok(())
    }

    /// Новый шаг сбрасывает повтор, а с ним — припаркованные копии страниц.
    fn push_step(&mut self, entry: HistoryEntry) {
        self.history.push(entry);
        self.forget_parked();
    }

    fn forget_parked(&mut self) {
        if !self.history.has_redo() {
            self.parked.clear();
        }
    }

    pub fn rename_page(&mut self, id: PageId, name: &str) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        self.pages_tree.get_meta(id.0)?.insert(KEY_NAME, name)?;
        self.root.commit();
        Ok(())
    }

    /// Удаляет страницу. Единственную — нельзя: документ без страниц не открыть.
    pub fn delete_page(&mut self, id: PageId) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        if self.pages().len() <= 1 {
            return Err(DocError::LastPage);
        }
        self.pages_tree.delete(id.0)?;
        self.root.commit();
        self.loaded.remove(&id);
        self.unloaded.remove(&id);
        for orphan in self.history.forget_page(id) {
            self.parked.remove(&orphan);
        }
        self.forget_parked();
        self.changes.remove(&id);
        self.journaled_pages.remove(&id);
        Ok(())
    }

    /// Цвет фона страницы; `None` — не задан (холст в цвете темы).
    pub fn page_background(&self, id: PageId) -> Option<Color> {
        match self.pages_tree.get_meta(id.0).ok()?.get(KEY_BACKGROUND) {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => Color::parse(&s).ok(),
            _ => None,
        }
    }

    /// Задаёт или снимает (`None`) цвет фона страницы. Как и остальные операции со
    /// страницами, в историю отмены не попадает.
    pub fn set_page_background(&mut self, id: PageId, color: Option<Color>) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        let meta = self.pages_tree.get_meta(id.0)?;
        match color {
            Some(c) => meta.insert(KEY_BACKGROUND, c.to_hex())?,
            None => meta.delete(KEY_BACKGROUND)?,
        }
        self.root.commit();
        Ok(())
    }

    fn ensure_page(&self, id: PageId) -> DocResult<()> {
        if self.loaded.contains_key(&id) || self.unloaded.contains_key(&id) {
            Ok(())
        } else {
            Err(DocError::PageNotFound(id.to_string()))
        }
    }

    pub fn is_page_loaded(&self, id: PageId) -> bool {
        self.loaded.contains_key(&id)
    }

    /// Загружает страницу при первом обращении.
    pub fn page(&mut self, id: PageId) -> DocResult<&PageDoc> {
        if let Some(u) = self.unloaded.get(&id) {
            // Сначала грузим по ссылке: при ошибке запись остаётся в unloaded,
            // иначе страница пропала бы из документа и не попала бы в сохранение.
            let page = PageDoc::load(&u.snapshot, u.scene.as_deref())?;
            // Узлы неизвестного типа скрыты от модели; правка могла бы их потерять.
            if page.has_unknown_nodes() || page.has_oversized_text() {
                self.read_only = true;
            }
            self.unloaded.remove(&id);
            // Загруженное из файла уже в нём: журнал пишет правки после загрузки.
            self.journaled_pages.insert(id, page.oplog_vv());
            if self.sync.is_some() {
                page.listen();
            }
            self.loaded.insert(id, page);
        }
        self.loaded
            .get(&id)
            .ok_or_else(|| DocError::PageNotFound(id.to_string()))
    }

    /// Уже загруженная страница — без загрузки (`None`, если её ещё не открывали).
    pub fn loaded_page(&self, id: PageId) -> Option<&PageDoc> {
        self.loaded.get(&id)
    }

    /// Кэш сцены незагруженной страницы: по нему холст рисуется до загрузки Loro.
    pub fn cached_scene(&self, id: PageId) -> Option<&[SceneRow]> {
        self.unloaded.get(&id)?.scene.as_deref()
    }

    /// Данные страницы для файла — без загрузки, если она не открыта.
    pub(crate) fn page_payload(&self, id: PageId, kind: SnapshotKind) -> DocResult<PagePayload> {
        if let Some(p) = self.loaded.get(&id) {
            return Ok(PagePayload {
                snapshot: match kind {
                    SnapshotKind::Shallow => p.export_snapshot()?,
                    SnapshotKind::Full => p.export_full_snapshot()?,
                },
                image_refs: p.image_refs()?,
                scene: Some(p.scene_rows()?),
            });
        }
        let u = self
            .unloaded
            .get(&id)
            .ok_or_else(|| DocError::PageNotFound(id.to_string()))?;
        Ok(PagePayload {
            snapshot: u.snapshot.clone(),
            image_refs: u.image_refs.clone(),
            scene: u.scene.clone(),
        })
    }

    /// Выполняет изменения страницы как один шаг undo.
    /// `f` возвращает выделение после изменения. При ошибке изменения откатываются.
    pub fn transact<F>(
        &mut self,
        page: PageId,
        selection_before: Vec<NodeId>,
        f: F,
    ) -> DocResult<Vec<NodeId>>
    where
        F: FnOnce(&Tx) -> DocResult<Vec<NodeId>>,
    {
        self.transact_with(page, selection_before, None, f)
    }

    /// Как `transact`, но шаг сливается с прошлым, если тот — продолжение этой
    /// правки текста (`TextMerge::continues`): ввод подряд — один шаг отмены.
    pub fn transact_merging<F>(
        &mut self,
        page: PageId,
        selection_before: Vec<NodeId>,
        merge: TextMerge,
        f: F,
    ) -> DocResult<Vec<NodeId>>
    where
        F: FnOnce(&Tx) -> DocResult<Vec<NodeId>>,
    {
        self.transact_with(page, selection_before, Some(merge), f)
    }

    fn transact_with<F>(
        &mut self,
        page: PageId,
        selection_before: Vec<NodeId>,
        merge: Option<TextMerge>,
        f: F,
    ) -> DocResult<Vec<NodeId>>
    where
        F: FnOnce(&Tx) -> DocResult<Vec<NodeId>>,
    {
        // Сначала загрузка: страница с неизвестными узлами делает документ
        // доступным только для чтения.
        self.page(page)?;
        self.writable()?;
        let p = self.loaded.get(&page).expect("страница загружена выше");
        p.begin_journal();
        let result = f(&Tx::new(p));
        let ops = p.take_journal();
        p.commit();
        // И при ошибке: откат вернул эти узлы в прежнее состояние, а если сорвался —
        // сцене тем более нужно сверить их со страницей.
        self.changes.entry(page).or_default().record(&ops);
        let out = match result {
            Ok(selection_after) => {
                if !ops.is_empty() {
                    let entry = HistoryEntry {
                        id: 0,
                        page,
                        page_after: None,
                        ops,
                        doc_ops: Vec::new(),
                        selection_before,
                        selection_after: selection_after.clone(),
                    };
                    match merge {
                        Some(m) => self.history.push_merging(entry, m),
                        None => self.history.push(entry),
                    }
                    self.forget_parked();
                }
                Ok(selection_after)
            }
            // Откат: применяем обратные операции, в историю ничего не попадает.
            Err(e) => Err(rollback(p, &ops, e)),
        };
        self.settle_duplicates(page, out)
    }

    /// Зеркало после своей правки, если на странице есть двойники (`PageDoc::settle_duplicates`).
    /// Ошибка самой правки важнее сбоя сборки и не заслоняется им.
    fn settle_duplicates<T>(&mut self, page: PageId, out: DocResult<T>) -> DocResult<T> {
        let settled = match self.loaded.get(&page) {
            Some(p) => p.settle_duplicates(self.changes.entry(page).or_default()),
            None => Ok(()),
        };
        let out = out?;
        settled.map(|()| out)
    }

    /// Следующая `transact_merging` не сольётся с прошлой правкой, даже если та её
    /// продолжает: редактор начал новый сеанс правки текста.
    pub fn break_text_merge(&mut self) {
        self.history.break_merge();
    }

    /// Узлы страницы, изменённые правками, отменой и повтором с прошлого вызова.
    /// Сцена вызывает это раз за кадр и пересчитывает только эти узлы.
    pub fn take_changes(&mut self, page: PageId) -> Changes {
        self.changes.remove(&page).unwrap_or_default()
    }

    /// Изменения страницы, ещё не забранные `take_changes`.
    pub fn pending_changes(&self, page: PageId) -> Option<&Changes> {
        self.changes.get(&page)
    }

    /// Забывает историю отмены и повтора: документ, собранный программно (пример,
    /// шаблон), открывается как только что прочитанный файл — без шага «создать всё».
    pub fn clear_history(&mut self) {
        self.history = History::default();
        self.parked.clear();
    }

    /// Номер последнего шага отмены: тот же шаг остаётся тем же номером и после
    /// отмены с повтором; новый шаг — новый номер (спека подсистемы 2, раздел 10).
    pub fn last_step(&self) -> Option<u64> {
        self.history.top_undo_id()
    }

    /// Правки с прошлого вызова одной пачкой журнала: обновления Loro корня и
    /// загруженных страниц и новые изображения. `None` — изменений нет. Версии
    /// журнала сдвигаются, только когда собрана вся пачка: ошибка ничего не теряет.
    pub fn take_journal(&mut self) -> DocResult<Option<Vec<u8>>> {
        let mut batch = BatchWriter::new();
        let mut next_root = None;
        let vv = self.root.oplog_vv();
        if vv != self.journaled_root {
            batch.push(
                journal::ROOT,
                "",
                &self
                    .root
                    .export(ExportMode::updates(&self.journaled_root))?,
            );
            next_root = Some(vv);
        }
        let mut ids: Vec<PageId> = self.loaded.keys().copied().collect();
        ids.sort_by_key(|id| id.to_string());
        let mut next_pages = Vec::new();
        for id in ids {
            let page = &self.loaded[&id];
            let vv = page.oplog_vv();
            let base = self.journaled_pages.get(&id).cloned().unwrap_or_default();
            if vv != base {
                batch.push(journal::PAGE, &id.to_string(), &page.export_updates(&base)?);
                next_pages.push((id, vv));
            }
        }
        if let Some(vv) = next_root {
            self.journaled_root = vv;
        }
        self.journaled_pages.extend(next_pages);
        for (hash, bytes) in self.images.take_fresh() {
            batch.push(journal::IMAGE, &hash, &bytes);
        }
        Ok(batch.finish())
    }

    /// Всё нынешнее считается уже в журнале: после открытия примера или
    /// восстановления журнал пишет только новые правки.
    pub fn mark_journal(&mut self) {
        self.journaled_root = self.root.oplog_vv();
        self.journaled_pages = self
            .loaded
            .iter()
            .map(|(id, p)| (*id, p.oplog_vv()))
            .collect();
        self.images.take_fresh();
    }

    /// Применяет пачки журнала, записанные после файла, из которого открыт документ,
    /// в порядке записи, и считает их уже в журнале. Повреждённая пачка
    /// пропускается целиком; обновления, которым не хватает предыдущих, Loro не
    /// применяет. Обо всём, что не восстановлено, — одно предупреждение.
    pub fn apply_journal(&mut self, batches: &[&[u8]]) -> Replayed {
        let mut out = Replayed::default();
        if batches.is_empty() {
            return out;
        }
        if self.is_read_only() {
            out.warnings
                .push("Несохранённые изменения не применены: файл открыт только для чтения".into());
            return out;
        }
        let mut lost = false;
        let mut skipped_read_only = false;
        let (mut root, mut pages, mut images) = (Vec::new(), Vec::new(), Vec::new());
        for bytes in batches {
            match journal::parse(bytes) {
                Some(b) => {
                    out.applied += 1;
                    root.extend(b.root);
                    pages.extend(b.pages);
                    images.extend(b.images);
                }
                None => lost = true,
            }
        }
        for data in &root {
            match self.root.import(data) {
                Ok(status) => lost |= status.pending.is_some() && !self.catches_up,
                Err(_) => lost = true,
            }
        }
        // Страницы, созданные и удалённые журналом корня.
        let listed: Vec<PageId> = self.pages().into_iter().map(|(id, _)| id).collect();
        for id in &listed {
            if !self.loaded.contains_key(id) && !self.unloaded.contains_key(id) {
                self.loaded.insert(*id, PageDoc::new());
            }
        }
        self.loaded.retain(|id, _| listed.contains(id));
        self.unloaded.retain(|id, _| listed.contains(id));
        for (id, list) in journal::group_pages(pages) {
            // Страницу удалили позже в том же журнале — её правки не нужны.
            if !listed.contains(&id) {
                continue;
            }
            if self.page(id).is_err() {
                lost = true;
                continue;
            }
            if self.is_read_only() {
                // Страница с неизвестными узлами (найдена при загрузке) — только чтение,
                // как в ветке раннего выхода: правки этой и следующих страниц не применяем.
                skipped_read_only = true;
                continue;
            }
            let page = self.loaded.get(&id).expect("загружена выше");
            let refs: Vec<&[u8]> = list.iter().map(Vec::as_slice).collect();
            match page.import_updates(&refs) {
                // Файл на сервере: Loro держит такие правки у себя и применит, когда придут предыдущие.
                Ok(complete) => lost |= !complete && !self.catches_up,
                Err(_) => lost = true,
            }
            if page.has_unknown_nodes() || page.has_oversized_text() {
                self.read_only = true;
            }
        }
        for (hash, bytes) in images {
            self.images.insert_verified(hash, bytes);
        }
        if skipped_read_only {
            out.warnings
                .push("Несохранённые изменения не применены: файл открыт только для чтения".into());
        }
        if lost {
            out.warnings.push(
                "Не все несохранённые изменения удалось восстановить: журнал изменений повреждён"
                    .into(),
            );
        }
        // Ничего не изменилось (журнал от другого файла, всё «ждёт» предыдущих
        // обновлений) — ничего и не применено.
        let changed = self.root.oplog_vv() != self.journaled_root
            || self
                .loaded
                .iter()
                .any(|(id, p)| self.journaled_pages.get(id) != Some(&p.oplog_vv()));
        if !changed {
            out.applied = 0;
        }
        self.mark_journal();
        // Журнал корня мог сменить направляющие страниц.
        self.guides_revision += 1;
        out
    }

    /// Файл на сервере, ещё до применения журнала: неполный журнал — не повреждение.
    pub fn set_catches_up(&mut self) {
        self.catches_up = true;
    }

    /// Включает синхронизацию: зеркала страниц начинают слушать чужие правки, а на следующем
    /// `take_sync_subscribes` — подписка на корень и все загруженные страницы.
    pub fn sync_start(&mut self) {
        self.catches_up = true;
        if self.sync.is_none() {
            self.sync = Some(SyncCursors::default());
        }
        for page in self.loaded.values() {
            page.listen();
        }
    }

    /// Соединение оборвалось: на новом всё начинается с подписки, а неподтверждённое уйдёт снова.
    pub fn sync_reset(&mut self) {
        if let Some(s) = &mut self.sync {
            s.subscribed.clear();
            for c in s.docs.values_mut() {
                c.ready = false;
            }
        }
    }

    /// Документы, на которые ещё нет подписки, с их версиями. Незагруженной страницы здесь нет:
    /// подписка — когда её откроют, её снимок потом догонит журнал.
    pub fn take_sync_subscribes(&mut self) -> Vec<(DocKey, SyncVersion)> {
        let Some(s) = &mut self.sync else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if s.subscribed.insert(DocKey::Root) {
            out.push((DocKey::Root, SyncVersion(self.root.oplog_vv())));
        }
        for (id, page) in &self.loaded {
            if s.subscribed.insert(DocKey::Page(*id)) {
                out.push((DocKey::Page(*id), SyncVersion(page.oplog_vv())));
            }
        }
        out
    }

    /// Сервер ответил на подписку: всё сверх его версии надо отправить.
    pub fn sync_subscribed(&mut self, doc: DocKey, server: SyncVersion) {
        if let Some(s) = &mut self.sync {
            s.docs.insert(
                doc,
                Cursor {
                    ready: true,
                    sent: server.0.clone(),
                    acked: server.0,
                },
            );
        }
    }

    /// Свои правки, которых сервер ещё не получал, кусками до `limit` байт. Просмотр и файл только для
    /// чтения правок не шлют, что бы ни было в журнале документа.
    pub fn take_sync_updates(&mut self, limit: usize) -> DocResult<Vec<SyncBatch>> {
        if self.is_read_only() {
            return Ok(Vec::new());
        }
        let Some(s) = &mut self.sync else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for (key, c) in s.docs.iter_mut().filter(|(_, c)| c.ready) {
            let doc = match key {
                DocKey::Root => Some(self.root.clone()),
                DocKey::Page(id) => self.loaded.get(id).map(|p| p.loro().clone()),
            };
            let Some(doc) = doc else { continue };
            if c.sent.includes_vv(&doc.oplog_vv()) {
                continue;
            }
            for (bytes, end) in
                sync::split_updates(&doc, &c.sent, limit, sync::MAX_SYNC_FLUSH_BYTES)?
            {
                // Отправлено ровно то, что собрано: остаток заберёт следующий вызов.
                sync::merge(&mut c.sent, &end);
                out.push(SyncBatch {
                    doc: *key,
                    bytes,
                    end: SyncVersion(end),
                });
            }
        }
        Ok(out)
    }

    /// Сервер записал пачку на диск базы.
    pub fn sync_acked(&mut self, doc: DocKey, end: &SyncVersion) {
        if let Some(c) = self.sync.as_mut().and_then(|s| s.docs.get_mut(&doc)) {
            sync::merge(&mut c.acked, &end.0);
        }
    }

    /// Текущие версии документов: отметка пачки журнала, которую потом можно снять.
    pub fn sync_versions(&self) -> Vec<(DocKey, SyncVersion)> {
        if self.sync.is_none() {
            return Vec::new();
        }
        let mut out = vec![(DocKey::Root, SyncVersion(self.root.oplog_vv()))];
        out.extend(
            self.loaded
                .iter()
                .map(|(id, p)| (DocKey::Page(*id), SyncVersion(p.oplog_vv()))),
        );
        out
    }

    /// Сервер подтвердил всё, что было в документе на момент отметки.
    pub fn sync_covers(&self, marks: &[(DocKey, SyncVersion)]) -> bool {
        let Some(s) = &self.sync else {
            return false;
        };
        marks.iter().all(|(k, v)| {
            // Документа больше нет (страницу удалили): его правки ждать незачем.
            let gone = matches!(k, DocKey::Page(id) if !self.loaded.contains_key(id));
            gone || s.docs.get(k).is_some_and(|c| c.acked.includes_vv(&v.0))
        })
    }

    /// Есть правки, которых сервер не подтвердил (или ещё не подключались).
    pub fn sync_unacked(&self) -> bool {
        let Some(s) = &self.sync else {
            return false;
        };
        // Сервера ещё не спрашивали: своё ли там, неизвестно, значит, не подтверждено.
        let behind = |key: DocKey, vv: &VersionVector| match s.docs.get(&key) {
            Some(c) => !c.acked.includes_vv(vv),
            None => true,
        };
        behind(DocKey::Root, &self.root.oplog_vv())
            || self
                .loaded
                .iter()
                .any(|(id, p)| behind(DocKey::Page(*id), &p.oplog_vv()))
    }

    /// Применяет пачку чужих правок документа `doc` (спека подсистемы 4, раздел 8). Шаги отмены
    /// остаются, зеркало обновляется по изменённым узлам, изменения — обычным путём `take_changes`.
    /// Работает и в режиме просмотра, и в файле только для чтения: правки других применяются всегда.
    pub fn apply_remote(&mut self, doc: DocKey, batches: &[&[u8]]) -> DocResult<RemoteOutcome> {
        let mut out = match doc {
            DocKey::Root => self.apply_remote_root(batches),
            DocKey::Page(id) => self.apply_remote_page(id, batches)?,
        };
        // Принесённое сервером отправлять незачем, и оно уже подтверждено.
        if let Some(c) = self.sync.as_mut().and_then(|s| s.docs.get_mut(&doc)) {
            for bytes in batches {
                let end = sync::blob_end(bytes);
                sync::merge(&mut c.sent, &end);
                sync::merge(&mut c.acked, &end);
            }
        }
        out.missing_images.sort();
        out.missing_images.dedup();
        Ok(out)
    }

    fn apply_remote_root(&mut self, batches: &[&[u8]]) -> RemoteOutcome {
        let mut out = RemoteOutcome {
            root_changed: true,
            ..RemoteOutcome::default()
        };
        let was_read_only = self.is_read_only();
        for data in batches {
            match self.root.import(data) {
                Ok(status) => out.pending |= status.pending.is_some(),
                Err(_) => out.failed = true,
            }
        }
        let listed: Vec<PageId> = self.pages().into_iter().map(|(id, _)| id).collect();
        for id in &listed {
            if !self.loaded.contains_key(id) && !self.unloaded.contains_key(id) {
                // Страница другого участника: содержимое придёт по подписке, загружать нечего.
                let page = PageDoc::new();
                if self.sync.is_some() {
                    page.listen();
                }
                self.loaded.insert(*id, page);
                self.journaled_pages.insert(*id, VersionVector::default());
                out.added_pages.push(*id);
            }
        }
        let known: Vec<PageId> = self
            .loaded
            .keys()
            .chain(self.unloaded.keys())
            .copied()
            .collect();
        for id in known {
            if !listed.contains(&id) {
                self.loaded.remove(&id);
                self.unloaded.remove(&id);
                self.changes.remove(&id);
                self.journaled_pages.remove(&id);
                for orphan in self.history.forget_page(id) {
                    self.parked.remove(&orphan);
                }
                if let Some(s) = &mut self.sync {
                    s.docs.remove(&DocKey::Page(id));
                    s.subscribed.remove(&DocKey::Page(id));
                }
                out.removed_pages.push(id);
            }
        }
        self.forget_parked();
        // Чужая правка могла сменить направляющие страницы и версию схемы.
        self.guides_revision += 1;
        if self.schema_version() > SCHEMA_VERSION {
            self.read_only = true;
        }
        out.became_read_only = self.is_read_only() && !was_read_only;
        out
    }

    fn apply_remote_page(&mut self, id: PageId, batches: &[&[u8]]) -> DocResult<RemoteOutcome> {
        // Страница, которую уже удалили, или ещё не загруженная: подписки на неё нет,
        // а опоздавший кадр догонит журнал при открытии.
        let Some(page) = self.loaded.get(&id) else {
            return Ok(RemoteOutcome::default());
        };
        let was_read_only = self.is_read_only();
        let applied = page.apply_remote(batches)?;
        let mut out = RemoteOutcome {
            page: Some(id),
            failed: applied.failed,
            pending: applied.pending,
            rebuilt: applied.rebuilt,
            vectors: applied.vectors,
            text: applied
                .text
                .iter()
                .map(|(n, d)| (*n, TextShift(d.clone())))
                .collect(),
            ..RemoteOutcome::default()
        };
        if page.has_unknown_nodes() || page.has_oversized_text() {
            self.read_only = true;
        }
        out.became_read_only = self.is_read_only() && !was_read_only;
        if applied.rebuilt {
            // Сцена строится заново, частичные изменения ей не нужны; сверить все хэши дешевле, чем помнить их.
            self.changes.remove(&id);
            let refs = page.image_refs()?;
            out.missing_images = refs
                .into_iter()
                .filter(|h| self.images.get(h).is_none())
                .collect();
        } else {
            out.missing_images = applied
                .images
                .into_iter()
                .filter(|h| self.images.get(h).is_none())
                .collect();
            self.changes.entry(id).or_default().extend(applied.changes);
        }
        for (node, delta) in &applied.text {
            self.history.transform_text(id, *node, delta);
        }
        Ok(out)
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Применяет операции шага и возвращает операции для обратного направления.
    /// Шаг не изменяется: при ошибке вызывающий код вернёт его в стек целиком.
    fn replay(&mut self, entry: &HistoryEntry) -> DocResult<Replay> {
        // Страница, на которую ведёт шаг, могла быть не загружена; загрузка могла
        // сделать документ доступным только для чтения.
        self.page(entry.page)?;
        self.writable()?;
        let p = self.loaded.get(&entry.page).expect("загружена");
        // При сбое apply_inverse сам откатывает уже применённую часть шага. Узлы, которые успел
        // убрать другой участник, шаг пропускает.
        let result = p.apply_step(&entry.ops);
        // Применённые операции записывают обратные к себе — это и есть изменения.
        // При сбое сверяем все узлы шага: откат мог и не удаться.
        let changes = self.changes.entry(entry.page).or_default();
        match &result {
            Ok(done) => changes.record(done),
            Err(_) => changes.record(&entry.ops),
        }
        let ops = self.settle_duplicates(entry.page, result)?;
        match self.apply_doc_ops(&entry.doc_ops) {
            Ok((doc_ops, remap)) => Ok(Replay {
                ops,
                doc_ops,
                remap,
            }),
            // Операции страницы уже применены: назад, шаг вернётся в стек целиком.
            Err(e) => {
                let p = self.loaded.get(&entry.page).expect("загружена");
                let back = p.apply_inverse(&ops);
                let changes = self.changes.entry(entry.page).or_default();
                changes.record(&ops);
                let failed = Err(match back {
                    Ok(_) => e,
                    Err(rollback) => DocError::RollbackFailed {
                        original: Box::new(e),
                        rollback: Box::new(rollback),
                    },
                });
                self.settle_duplicates(entry.page, failed)
            }
        }
    }

    /// Операции над документом с конца; возвращает обратные к ним и новый
    /// идентификатор страницы, возвращённой повтором дубля.
    #[allow(clippy::type_complexity)]
    fn apply_doc_ops(
        &mut self,
        ops: &[DocOp],
    ) -> DocResult<(Vec<DocOp>, Option<(PageId, PageId)>)> {
        let mut done = Vec::with_capacity(ops.len());
        let mut remap = None;
        for op in ops.iter().rev() {
            match op {
                DocOp::PageGuides { page, guides } => {
                    self.ensure_page(*page)?;
                    let before = self.page_guides(*page);
                    self.write_page_guides(*page, guides)?;
                    done.push(DocOp::PageGuides {
                        page: *page,
                        guides: before,
                    });
                }
                DocOp::MovePage { page, index } => {
                    let before = self.page_index(*page)?;
                    self.place_page(*page, *index)?;
                    done.push(DocOp::MovePage {
                        page: *page,
                        index: before,
                    });
                }
                DocOp::RemovePage { page } => {
                    let index = self.page_index(*page)?;
                    if self.pages().len() <= 1 {
                        return Err(DocError::LastPage);
                    }
                    // Копия всегда загружена (дубль или повтор): парковка хранит её целиком.
                    self.writable()?;
                    let (name, background, guides) = (
                        self.pages()[index].1.clone(),
                        self.page_background(*page),
                        self.page_guides(*page),
                    );
                    self.pages_tree.delete(page.0)?;
                    self.root.commit();
                    let doc = self.loaded.remove(page).expect("загружена выше");
                    self.parked.insert(*page, doc);
                    self.changes.remove(page);
                    self.journaled_pages.remove(page);
                    self.guides_revision += 1;
                    done.push(DocOp::RestorePage {
                        page: *page,
                        index,
                        name,
                        background,
                        guides,
                    });
                }
                DocOp::RestorePage {
                    page,
                    index,
                    name,
                    background,
                    guides,
                } => {
                    if !self.parked.contains_key(page) {
                        return Err(DocError::PageNotFound(page.to_string()));
                    }
                    let at = (*index).min(self.pages().len());
                    let t = self.pages_tree.create_at(TreeParentId::Root, at)?;
                    let meta = self.pages_tree.get_meta(t)?;
                    meta.insert(KEY_NAME, name.as_str())?;
                    if let Some(c) = background {
                        meta.insert(KEY_BACKGROUND, c.to_hex())?;
                    }
                    if !guides.is_empty() {
                        meta.insert(KEY_GUIDES, guides_value(guides))?;
                    }
                    self.root.commit();
                    let new = PageId(t);
                    let doc = self.parked.remove(page).expect("проверено выше");
                    self.loaded.insert(new, doc);
                    // Копия целиком — в следующую пачку журнала, как новая страница.
                    self.journaled_pages.insert(new, VersionVector::default());
                    self.guides_revision += 1;
                    done.push(DocOp::RemovePage { page: new });
                    remap = Some((*page, new));
                }
            }
        }
        Ok((done, remap))
    }

    pub fn undo(&mut self) -> DocResult<Option<Restore>> {
        // Шаг, от которого чужие правки не оставили ничего, пропускается: отмена идёт к следующему.
        while let Some(entry) = self.history.pop_undo() {
            match self.undo_entry(entry)? {
                Some(restore) => return Ok(Some(restore)),
                None => continue,
            }
        }
        Ok(None)
    }

    fn undo_entry(&mut self, entry: HistoryEntry) -> DocResult<Option<Restore>> {
        let empty = entry.ops.is_empty() && entry.doc_ops.is_empty();
        match self.replay(&entry) {
            Ok(r) if !empty && r.ops.is_empty() && r.doc_ops.is_empty() => Ok(None),
            Ok(r) => {
                let restore = Restore {
                    page: entry.page,
                    selection: entry.selection_before.clone(),
                };
                self.history.push_redo(HistoryEntry {
                    ops: r.ops,
                    doc_ops: r.doc_ops,
                    ..entry
                });
                Ok(Some(restore))
            }
            Err(e) => {
                // Шаг не теряется: возвращаем исходную запись в стек undo.
                self.history.push_undo_keep_redo(entry);
                Err(e)
            }
        }
    }

    pub fn redo(&mut self) -> DocResult<Option<Restore>> {
        while let Some(entry) = self.history.pop_redo() {
            match self.redo_entry(entry)? {
                Some(restore) => return Ok(Some(restore)),
                None => continue,
            }
        }
        Ok(None)
    }

    fn redo_entry(&mut self, entry: HistoryEntry) -> DocResult<Option<Restore>> {
        let empty = entry.ops.is_empty() && entry.doc_ops.is_empty();
        match self.replay(&entry) {
            Ok(r) if !empty && r.ops.is_empty() && r.doc_ops.is_empty() => Ok(None),
            Ok(r) => {
                let mut next = HistoryEntry {
                    ops: r.ops,
                    doc_ops: r.doc_ops,
                    ..entry
                };
                if let Some((old, new)) = r.remap {
                    self.history.remap_page(old, new);
                    if next.page_after == Some(old) {
                        next.page_after = Some(new);
                    }
                }
                let restore = Restore {
                    page: next.page_after.unwrap_or(next.page),
                    selection: next.selection_after.clone(),
                };
                self.history.push_undo_keep_redo(next);
                Ok(Some(restore))
            }
            Err(e) => {
                // Шаг не теряется: возвращаем исходную запись в стек redo.
                self.history.push_redo(entry);
                Err(e)
            }
        }
    }
}

/// Откат незавершённой транзакции. Если откат тоже сорвался, исходная ошибка
/// не должна теряться: возвращаем обе.
fn rollback(page: &PageDoc, ops: &[Inverse], original: DocError) -> DocError {
    match page.apply_inverse(ops) {
        Ok(_) => original,
        Err(rollback) => DocError::RollbackFailed {
            original: Box::new(original),
            rollback: Box::new(rollback),
        },
    }
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryEntry;
    use crate::types::{NodeKind, PropsPatch};

    /// Идентификатор узла, которого нет на свежей странице.
    fn missing_node() -> NodeId {
        let scratch = PageDoc::new();
        scratch
            .create_node(None, None, NodeKind::Rect)
            .expect("создание узла")
    }

    fn missing_props_op() -> Inverse {
        Inverse::Props(
            missing_node(),
            Box::new(PropsPatch {
                x: Some(1.0),
                ..Default::default()
            }),
        )
    }

    #[test]
    fn page_with_unknown_kind_makes_document_read_only() {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        let star = doc
            .transact(page, vec![], |p| {
                p.create_node(None, None, NodeKind::Rect)?;
                Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
            })
            .unwrap()[0];
        doc.page(page)
            .unwrap()
            .force_raw_kind_for_tests(star, Some("star"));
        let root = doc.root_snapshot().unwrap();
        let payload = doc.page_payload(page, SnapshotKind::Shallow).unwrap();
        let mut unloaded = HashMap::new();
        unloaded.insert(
            page,
            UnloadedPage {
                snapshot: payload.snapshot,
                image_refs: payload.image_refs,
                scene: None,
            },
        );
        let (mut d2, _) = Document::from_parts(&root, unloaded, ImageStore::default()).unwrap();
        assert!(!d2.is_read_only());
        assert_eq!(d2.page(page).unwrap().walk().len(), 1);
        assert!(d2.is_read_only(), "скрытые узлы нельзя потерять правкой");
        let err = d2.transact(page, vec![], |p| {
            Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
        });
        assert!(matches!(err, Err(DocError::ReadOnly)));
    }

    #[test]
    fn duplicating_an_unopened_page_with_unknown_nodes_is_refused() {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        let star = doc
            .transact(page, vec![], |p| {
                p.create_node(None, None, NodeKind::Rect)?;
                Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
            })
            .unwrap()[0];
        doc.page(page)
            .unwrap()
            .force_raw_kind_for_tests(star, Some("star"));
        let root = doc.root_snapshot().unwrap();
        let payload = doc.page_payload(page, SnapshotKind::Shallow).unwrap();
        let mut unloaded = HashMap::new();
        unloaded.insert(
            page,
            UnloadedPage {
                snapshot: payload.snapshot,
                image_refs: payload.image_refs,
                scene: None,
            },
        );
        let (mut d2, _) = Document::from_parts(&root, unloaded, ImageStore::default()).unwrap();
        assert!(!d2.is_page_loaded(page));
        let before = d2.pages();
        let err = d2.duplicate_page(page, "Копия", page, vec![]);
        assert!(matches!(err, Err(DocError::ReadOnly)), "{err:?}");
        assert_eq!(d2.pages(), before, "копии нет");
    }

    /// Документ, где изображение используется только узлом внутри скрытого
    /// поддерева (узел неизвестного типа), сохранённый в файл.
    fn file_with_hidden_image() -> (Vec<u8>, String) {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        let hash = doc.images.add(b"png bytes".to_vec());
        let star = doc
            .transact(page, vec![], |p| {
                p.create_node(None, None, NodeKind::Rect)?;
                let star = p.create_node(None, None, NodeKind::Group)?;
                let pic = p.create_node(Some(star), None, NodeKind::Rect)?;
                p.set_props(
                    pic,
                    &PropsPatch {
                        fills: Some(vec![crate::Fill::image(hash.clone())]),
                        ..Default::default()
                    },
                )?;
                Ok(vec![star])
            })
            .unwrap()[0];
        doc.page(page)
            .unwrap()
            .force_raw_kind_for_tests(star, Some("star"));
        let bytes = crate::container::encode(&doc).unwrap();
        // Кэш сцены записан по зеркалу, где узел ещё известного типа; файл из новой
        // версии Ferrum такого кэша не содержал бы. Убираем его (id не страницы —
        // секция не сопоставится), чтобы страница загружалась из Loro.
        let bytes = crate::container::testing::map_sections(&bytes, |kind, id, _| {
            if kind == 5 {
                *id = "не-страница".to_string();
            }
        });
        (bytes, hash)
    }

    #[test]
    fn hidden_node_images_survive_resave_after_page_load() {
        let (bytes, hash) = file_with_hidden_image();
        let mut doc = crate::container::decode(&bytes).unwrap().document;
        let page = doc.pages()[0].0;
        // Загрузка страницы: ссылки теперь берутся из зеркала, где скрытых узлов нет.
        assert_eq!(doc.page(page).unwrap().walk().len(), 1);
        let resaved = crate::container::encode(&doc).unwrap();
        let reopened = crate::container::decode(&resaved).unwrap().document;
        assert!(reopened.images.get(&hash).is_some());
    }

    #[test]
    fn hidden_node_images_survive_lost_refs_section() {
        let (bytes, hash) = file_with_hidden_image();
        // Секция ссылок с чужим id: читатель её не сопоставит со страницей и
        // пересчитает ссылки из снапшота.
        let damaged = crate::container::testing::map_sections(&bytes, |kind, id, _| {
            if kind == 3 {
                *id = "не-страница".to_string();
            }
        });
        let opened = crate::container::decode(&damaged).unwrap();
        assert!(opened.warnings.iter().any(|w| w.contains("изображений")));
        let resaved = crate::container::encode(&opened.document).unwrap();
        let reopened = crate::container::decode(&resaved).unwrap().document;
        assert!(reopened.images.get(&hash).is_some());
    }

    #[test]
    fn out_of_range_schema_version_opens_read_only() {
        // 2^32 + 1 при усечении до u32 стало бы версией 1 — файл открылся бы на запись.
        for raw in [(1i64 << 32) + 1, -1] {
            let doc = Document::new();
            doc.root.get_map(META).insert(KEY_SCHEMA, raw).unwrap();
            doc.root.commit();
            assert_eq!(doc.schema_version(), u32::MAX, "{raw}");
            let root = doc.root_snapshot().unwrap();
            let (opened, warnings) =
                Document::from_parts(&root, HashMap::new(), ImageStore::default()).unwrap();
            assert!(opened.is_read_only(), "{raw}");
            assert!(
                warnings.iter().any(|w| w.contains("только для чтения")),
                "{raw}: {warnings:?}"
            );
        }
    }

    #[test]
    fn failed_lazy_load_keeps_page() {
        let mut doc = Document::new();
        let p1 = doc.pages()[0].0;
        let p2 = doc.add_page("Вторая").unwrap();
        let root = doc.root_snapshot().unwrap();
        let s1 = doc.page_payload(p1, SnapshotKind::Shallow).unwrap();

        let mut unloaded = HashMap::new();
        unloaded.insert(
            p1,
            UnloadedPage {
                snapshot: s1.snapshot,
                image_refs: s1.image_refs,
                scene: s1.scene,
            },
        );
        unloaded.insert(
            p2,
            UnloadedPage {
                snapshot: b"garbage".to_vec(),
                image_refs: vec![],
                scene: None,
            },
        );
        let (mut d2, _) = Document::from_parts(&root, unloaded, ImageStore::default()).unwrap();

        assert!(d2.page(p2).is_err());
        let payload = d2.page_payload(p2, SnapshotKind::Shallow).unwrap();
        assert_eq!(payload.snapshot, b"garbage".to_vec());
        assert!(d2.pages().iter().any(|(id, _)| *id == p2));
        assert!(d2.page(p1).is_ok());
    }

    /// Шаг про узел, которого больше нет (убрал другой участник), пропускается, а не блокирует стек.
    #[test]
    fn undo_step_over_missing_node_is_skipped() {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        doc.history.push(HistoryEntry {
            id: 0,
            page,
            page_after: None,
            ops: vec![missing_props_op()],
            doc_ops: vec![],
            selection_before: vec![],
            selection_after: vec![],
        });
        assert_eq!(doc.undo().unwrap(), None);
        assert!(!doc.can_undo());
        assert!(!doc.can_redo());
    }

    #[test]
    fn redo_step_over_missing_node_is_skipped() {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        doc.history.push_redo(HistoryEntry {
            id: 0,
            page,
            page_after: None,
            ops: vec![missing_props_op()],
            doc_ops: vec![],
            selection_before: vec![],
            selection_after: vec![],
        });
        assert_eq!(doc.redo().unwrap(), None);
        assert!(!doc.can_redo());
    }

    #[test]
    fn rollback_failure_keeps_original_error() {
        let page = PageDoc::new();
        let patch = PropsPatch {
            x: Some(1.0),
            ..Default::default()
        };
        let err = rollback(
            &page,
            &[Inverse::Props(missing_node(), Box::new(patch))],
            DocError::CyclicMove,
        );
        match err {
            DocError::RollbackFailed { original, .. } => {
                assert!(matches!(*original, DocError::CyclicMove));
            }
            other => panic!("ожидалась RollbackFailed, получено {other:?}"),
        }
        let err = rollback(&page, &[], DocError::CyclicMove);
        assert!(matches!(err, DocError::CyclicMove));
    }
}
