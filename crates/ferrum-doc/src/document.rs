//! Документ целиком: корневой `LoroDoc` (метаданные, список страниц) и по одному
//! `PageDoc` на страницу. Страницы загружаются по требованию.

use std::collections::HashMap;

use loro::{
    ExportMode, LoroDoc, LoroTree, LoroValue, TreeParentId, ValueOrContainer, VersionVector,
};

use crate::changes::Changes;
use crate::error::{DocError, DocResult};
use crate::history::{History, HistoryEntry};
use crate::images::ImageStore;
use crate::journal::{self, BatchWriter, Replayed};
use crate::page::{Inverse, PageDoc, Tx};
use crate::scene_cache::SceneRow;
use crate::types::{Color, NodeId, PageId};

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
    /// Журнал (этап 6): версии Loro корня и загруженных страниц, до которых правки
    /// уже отданы в журнал (`take_journal`). Страница получает версию при загрузке.
    journaled_root: VersionVector,
    journaled_pages: HashMap<PageId, VersionVector>,
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
            journaled_root,
            journaled_pages: HashMap::new(),
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

    pub(crate) fn force_schema_version(&mut self, version: u32) {
        let meta = self.root.get_map(META);
        meta.insert(KEY_SCHEMA, version as i64)
            .expect("запись в корневой документ");
        self.root.commit();
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    fn writable(&self) -> DocResult<()> {
        if self.read_only {
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
        self.history.forget_page(id);
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
            if page.has_unknown_nodes() {
                self.read_only = true;
            }
            self.unloaded.remove(&id);
            // Загруженное из файла уже в нём: журнал пишет правки после загрузки.
            self.journaled_pages.insert(id, page.oplog_vv());
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
    pub(crate) fn page_payload(&self, id: PageId) -> DocResult<PagePayload> {
        if let Some(p) = self.loaded.get(&id) {
            return Ok(PagePayload {
                snapshot: p.export_snapshot()?,
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
        match result {
            Ok(selection_after) => {
                if !ops.is_empty() {
                    self.history.push(HistoryEntry {
                        page,
                        ops,
                        selection_before,
                        selection_after: selection_after.clone(),
                    });
                }
                Ok(selection_after)
            }
            // Откат: применяем обратные операции, в историю ничего не попадает.
            Err(e) => Err(rollback(p, &ops, e)),
        }
    }

    /// Узлы страницы, изменённые правками, отменой и повтором с прошлого вызова.
    /// Сцена вызывает это раз за кадр и пересчитывает только эти узлы.
    pub fn take_changes(&mut self, page: PageId) -> Changes {
        self.changes.remove(&page).unwrap_or_default()
    }

    /// Забывает историю отмены и повтора: документ, собранный программно (пример,
    /// шаблон), открывается как только что прочитанный файл — без шага «создать всё».
    pub fn clear_history(&mut self) {
        self.history = History::default();
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
        if self.read_only {
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
                Ok(status) => lost |= status.pending.is_some(),
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
            if self.read_only {
                // Страница с неизвестными узлами (найдена при загрузке) — только чтение,
                // как в ветке раннего выхода: правки этой и следующих страниц не применяем.
                skipped_read_only = true;
                continue;
            }
            let page = self.loaded.get(&id).expect("загружена выше");
            let refs: Vec<&[u8]> = list.iter().map(Vec::as_slice).collect();
            match page.import_updates(&refs) {
                Ok(complete) => lost |= !complete,
                Err(_) => lost = true,
            }
            if page.has_unknown_nodes() {
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
        self.mark_journal();
        out
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Применяет операции шага и возвращает операции для обратного направления.
    /// Шаг не изменяется: при ошибке вызывающий код вернёт его в стек целиком.
    fn replay(&mut self, entry: &HistoryEntry) -> DocResult<Vec<Inverse>> {
        self.page(entry.page)?;
        let p = self.loaded.get(&entry.page).expect("загружена");
        // При сбое apply_inverse сам откатывает уже применённую часть шага.
        let result = p.apply_inverse(&entry.ops);
        // Применённые операции записывают обратные к себе — это и есть изменения.
        // При сбое сверяем все узлы шага: откат мог и не удаться.
        let changes = self.changes.entry(entry.page).or_default();
        match &result {
            Ok(done) => changes.record(done),
            Err(_) => changes.record(&entry.ops),
        }
        result
    }

    pub fn undo(&mut self) -> DocResult<Option<Restore>> {
        let Some(entry) = self.history.pop_undo() else {
            return Ok(None);
        };
        match self.replay(&entry) {
            Ok(ops) => {
                let restore = Restore {
                    page: entry.page,
                    selection: entry.selection_before.clone(),
                };
                self.history.push_redo(HistoryEntry { ops, ..entry });
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
        let Some(entry) = self.history.pop_redo() else {
            return Ok(None);
        };
        match self.replay(&entry) {
            Ok(ops) => {
                let restore = Restore {
                    page: entry.page,
                    selection: entry.selection_after.clone(),
                };
                self.history
                    .push_undo_keep_redo(HistoryEntry { ops, ..entry });
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
            PropsPatch {
                x: Some(1.0),
                ..Default::default()
            },
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
        let payload = doc.page_payload(page).unwrap();
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
        let s1 = doc.page_payload(p1).unwrap();

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
        let payload = d2.page_payload(p2).unwrap();
        assert_eq!(payload.snapshot, b"garbage".to_vec());
        assert!(d2.pages().iter().any(|(id, _)| *id == p2));
        assert!(d2.page(p1).is_ok());
    }

    #[test]
    fn failed_undo_keeps_history_entry() {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        doc.history.push(HistoryEntry {
            page,
            ops: vec![missing_props_op()],
            selection_before: vec![],
            selection_after: vec![],
        });
        assert!(doc.undo().is_err());
        assert!(doc.can_undo());
        assert!(!doc.can_redo());
    }

    #[test]
    fn failed_redo_keeps_history_entry() {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        doc.history.push_redo(HistoryEntry {
            page,
            ops: vec![missing_props_op()],
            selection_before: vec![],
            selection_after: vec![],
        });
        assert!(doc.redo().is_err());
        assert!(doc.can_redo());
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
            &[Inverse::Props(missing_node(), patch)],
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
