//! Документ целиком: корневой `LoroDoc` (метаданные, список страниц) и по одному
//! `PageDoc` на страницу. Страницы загружаются по требованию.

use std::collections::HashMap;

use loro::{LoroDoc, LoroTree, LoroValue, TreeParentId, ValueOrContainer};

use crate::error::{DocError, DocResult};
use crate::history::{History, HistoryEntry};
use crate::images::ImageStore;
use crate::page::PageDoc;
use crate::scene_cache::SceneRow;
use crate::types::{NodeId, PageId};

/// Текущая версия схемы документа. Растёт при несовместимых изменениях схемы.
pub const SCHEMA_VERSION: u32 = 1;

const META: &str = "meta";
const PAGES: &str = "pages";
const KEY_SCHEMA: &str = "schemaVersion";
const KEY_NAME: &str = "name";

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
    pub images: ImageStore,
    read_only: bool,
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
        Document {
            root,
            pages_tree,
            loaded: HashMap::new(),
            unloaded,
            history: History::default(),
            images,
            read_only: false,
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
        for (id, _) in doc.pages() {
            if !doc.unloaded.contains_key(&id) {
                warnings.push(format!("Страница {id} повреждена и открыта пустой"));
                doc.loaded.insert(id, PageDoc::new());
            }
        }
        Ok((doc, warnings))
    }

    pub(crate) fn root_snapshot(&self) -> DocResult<Vec<u8>> {
        Ok(self.root.export(loro::ExportMode::Snapshot)?)
    }

    pub fn schema_version(&self) -> u32 {
        match self.root.get_map(META).get(KEY_SCHEMA) {
            Some(ValueOrContainer::Value(LoroValue::I64(v))) => v as u32,
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
        Ok(id)
    }

    pub fn rename_page(&mut self, id: PageId, name: &str) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        self.pages_tree.get_meta(id.0)?.insert(KEY_NAME, name)?;
        self.root.commit();
        Ok(())
    }

    pub fn delete_page(&mut self, id: PageId) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        self.pages_tree.delete(id.0)?;
        self.root.commit();
        self.loaded.remove(&id);
        self.unloaded.remove(&id);
        self.history.forget_page(id);
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
        if let Some(u) = self.unloaded.remove(&id) {
            self.loaded
                .insert(id, PageDoc::load(&u.snapshot, u.scene.as_deref())?);
        }
        self.loaded
            .get(&id)
            .ok_or_else(|| DocError::PageNotFound(id.to_string()))
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
        F: FnOnce(&PageDoc) -> DocResult<Vec<NodeId>>,
    {
        self.writable()?;
        self.page(page)?;
        let p = self.loaded.get(&page).expect("страница загружена выше");
        p.begin_journal();
        let result = f(p);
        let ops = p.take_journal();
        p.commit();
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
            Err(e) => {
                // Откат: применяем обратные операции, в историю ничего не попадает.
                p.apply_inverse(&ops)?;
                Err(e)
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Применяет обратные операции шага и возвращает шаг для обратного направления.
    fn replay(&mut self, mut entry: HistoryEntry) -> DocResult<HistoryEntry> {
        self.page(entry.page)?;
        let p = self.loaded.get(&entry.page).expect("загружена");
        entry.ops = p.apply_inverse(&entry.ops)?;
        Ok(entry)
    }

    pub fn undo(&mut self) -> DocResult<Option<Restore>> {
        let Some(entry) = self.history.pop_undo() else {
            return Ok(None);
        };
        let entry = self.replay(entry)?;
        let restore = Restore {
            page: entry.page,
            selection: entry.selection_before.clone(),
        };
        self.history.push_redo(entry);
        Ok(Some(restore))
    }

    pub fn redo(&mut self) -> DocResult<Option<Restore>> {
        let Some(entry) = self.history.pop_redo() else {
            return Ok(None);
        };
        let entry = self.replay(entry)?;
        let restore = Restore {
            page: entry.page,
            selection: entry.selection_after.clone(),
        };
        self.history.push_undo_keep_redo(entry);
        Ok(Some(restore))
    }
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}
