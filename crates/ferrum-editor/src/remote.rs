//! Чужие правки в редакторе (спека подсистемы 4, раздел 8): сцена обновляется обычным путём, выделение
//! теряет удалённые узлы, жест над удалённым узлом отменяется, каретка следует за чужими вставками,
//! сеанс правки вектора закрывается, если другой изменил его сеть. Чужая правка не шаг отмены и не
//! делает документ изменённым.

use ferrum_doc::{DocKey, DocResult, NodeId, RemoteOutcome, SyncBatch, SyncVersion};
use ferrum_scene::Scene;

use crate::editor::{install_text_outlines, Editor, EditorEvent};
use crate::tools::sync;

/// Текст уведомления: сеть вектора изменил другой участник (`VectorEdit` закрыт).
pub const VECTOR_CHANGED_BY_OTHER: &str =
    "Другой участник изменил этот контур: правка контура закрыта";
/// Страницу, на которой вы были, удалил другой участник.
pub const PAGE_REMOVED_BY_OTHER: &str = "Другой участник удалил эту страницу";

/// Что осталось сделать вызывающему.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemoteEffects {
    /// Часть байтов не применена.
    pub failed: bool,
    /// Правкам не хватает предыдущих, подождут.
    pub pending: bool,
    /// Узлы ссылаются на эти картинки, а байтов нет: их надо скачать.
    pub missing_images: Vec<String>,
    /// Файл только что стал только для чтения: другой правит более новой версией.
    pub became_read_only: bool,
    /// Корень: мог смениться список страниц или имя документа.
    pub root_changed: bool,
    /// Правимая сцена построена заново.
    pub rebuilt: bool,
}

impl Editor {
    /// Файл на сервере: журналу не хватает правок, которые придут при догонянии.
    pub fn set_catches_up(&mut self) {
        self.doc.set_catches_up();
    }

    pub fn sync_start(&mut self) {
        self.doc.sync_start();
    }

    pub fn sync_reset(&mut self) {
        self.doc.sync_reset();
    }

    pub fn take_sync_subscribes(&mut self) -> Vec<(DocKey, SyncVersion)> {
        self.doc.take_sync_subscribes()
    }

    pub fn sync_subscribed(&mut self, doc: DocKey, server: SyncVersion) {
        self.doc.sync_subscribed(doc, server);
    }

    pub fn take_sync_updates(&mut self, limit: usize) -> DocResult<Vec<SyncBatch>> {
        self.doc.take_sync_updates(limit)
    }

    pub fn sync_acked(&mut self, doc: DocKey, end: &SyncVersion) {
        self.doc.sync_acked(doc, end);
    }

    pub fn sync_versions(&self) -> Vec<(DocKey, SyncVersion)> {
        self.doc.sync_versions()
    }

    pub fn sync_covers(&self, marks: &[(DocKey, SyncVersion)]) -> bool {
        self.doc.sync_covers(marks)
    }

    pub fn sync_unacked(&self) -> bool {
        self.doc.sync_unacked()
    }

    /// Новые картинки, байты которых сервер ещё не видел: `take_sync_updates` ссылается на них.
    pub fn take_sync_images(&mut self) -> Vec<(String, Vec<u8>)> {
        self.doc.images.take_sync_fresh()
    }

    /// Картинка по чужой ссылке: хэш сверяется, чужое имя не пройдёт. Сцена увидит её на следующем кадре.
    pub fn add_remote_image(&mut self, hash: &str, bytes: Vec<u8>) -> bool {
        self.doc.images.add_remote(hash, bytes)
    }

    /// Применяет чужие правки. Сбой разбора байтов не страшен (`failed`), редактор и документ целы.
    pub fn apply_remote(&mut self, doc: DocKey, batches: &[&[u8]]) -> DocResult<RemoteEffects> {
        let pages = self.doc.pages();
        let guides = self.doc.guides_revision();
        let outcome = self.doc.apply_remote(doc, batches)?;
        self.follow_text(&outcome);
        // Писать уже нельзя: жест и сеанс кончаются, причину человеку скажет вызывающий.
        if outcome.became_read_only {
            self.cancel_gesture();
            self.vector_edit = None;
            self.scene.clear_preview();
        }
        let on_this_page = outcome.page == Some(self.page);
        if on_this_page {
            self.close_vector_edit_if_changed(&outcome);
        }
        if on_this_page && outcome.rebuilt {
            self.rebuild_scene_after_remote();
        } else if on_this_page {
            self.leave_what_was_deleted();
        }
        let before = self.out.events.len();
        sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        // Правка другого — не своя: документ не становится изменённым.
        for e in &mut self.out.events[before..] {
            if matches!(e, EditorEvent::DocumentChanged) {
                *e = EditorEvent::RemoteChanged;
            }
        }
        let list_changed = self.doc.pages() != pages;
        if list_changed {
            self.out.events.push(EditorEvent::PagesChanged);
        }
        if outcome.root_changed {
            self.follow_pages(&outcome);
            if list_changed || self.doc.guides_revision() != guides {
                self.out.events.push(EditorEvent::RemoteChanged);
                self.out.events.push(EditorEvent::NeedsRedraw);
            }
        }
        self.emit();
        Ok(RemoteEffects {
            failed: outcome.failed,
            pending: outcome.pending,
            missing_images: outcome.missing_images,
            became_read_only: outcome.became_read_only,
            root_changed: outcome.root_changed,
            rebuilt: outcome.rebuilt && on_this_page,
        })
    }

    /// Каретка и выделение текста едут за чужими вставками; склейка ввода рвётся (её рвёт и история).
    fn follow_text(&mut self, outcome: &RemoteOutcome) {
        let Some(t) = &mut self.text_edit else {
            return;
        };
        for (node, shift) in &outcome.text {
            if *node == t.id && t.draft.is_none() {
                t.anchor = shift.map(t.anchor);
                t.focus.index = shift.map(t.focus.index);
                t.merge = None;
                t.goal_x = None;
            }
        }
    }

    /// Удалённые узлы тихо уходят из выделения; жест над ними отменяется, остальным правкам
    /// жест не мешает (при отпускании побеждает последняя запись по правилам Loro).
    fn leave_what_was_deleted(&mut self) {
        let Some(changes) = self.doc.pending_changes(self.page) else {
            return;
        };
        if changes.deleted.is_empty() {
            return;
        }
        // Узел мог вернуться той же пачкой (отмена удаления у другого): сверка с документом.
        let page = self.page;
        let deleted: Vec<_> = changes.deleted.clone();
        let Ok(doc) = self.doc.page(page) else {
            return;
        };
        let gone: std::collections::HashSet<_> =
            deleted.into_iter().filter(|n| !doc.contains(*n)).collect();
        if gone.is_empty() {
            return;
        }
        self.leave_nodes(&|n| gone.contains(&n));
    }

    /// Узлов больше нет: жест над ними отменяется, остальные жесты идут, выделение их теряет. Всё тихо.
    fn leave_nodes(&mut self, gone: &impl Fn(NodeId) -> bool) {
        let over_tool = self.is_busy()
            && (self.selection.iter().any(|n| gone(*n))
                || self.tool.new_node_parent().is_some_and(gone));
        if over_tool {
            self.cancel_gesture();
        }
        if self.panel.as_ref().is_some_and(|g| g.touches(gone)) {
            self.cancel_panel();
        }
        if self.guide_drag_touches(gone) {
            self.cancel_guides();
        }
        self.selection.retain(|n| !gone(*n));
    }

    /// Зеркало страницы построено заново: сцена — тоже, как при смене страницы, на месте. Жест над живыми
    /// узлами продолжается (спека, раздел 8: прерывает только удаление) вместе со своим превью.
    fn rebuild_scene_after_remote(&mut self) {
        let Ok(page) = self.doc.page(self.page) else {
            return;
        };
        let alive: std::collections::HashSet<_> = page.walk().into_iter().collect();
        self.leave_nodes(&|n| !alive.contains(&n));
        // Перетаскивание направляющей держит слоты старой сцены: другие жесты держат только `NodeId`.
        self.cancel_guides();
        let (drafts, patches) = self.scene.gesture();
        let Ok(page) = self.doc.page(self.page) else {
            return;
        };
        let scene = Scene::from_page(page);
        self.scene = scene;
        install_text_outlines(&mut self.scene, &self.fonts, &self.outlines_missed);
        self.scene.restore_gesture(&drafts, &patches);
        self.scene_epoch += 1;
        self.out.events.push(EditorEvent::RemoteRebuilt);
        self.out.events.push(EditorEvent::RemoteChanged);
        self.out.events.push(EditorEvent::NeedsRedraw);
    }

    /// Сеть — неделимое значение: последняя запись побеждает. Незаписанные точки сеанса пропадают,
    /// и человек об этом узнаёт.
    fn close_vector_edit_if_changed(&mut self, outcome: &RemoteOutcome) {
        let edited = self.vector_edit.as_ref().and_then(|v| v.id);
        if edited.is_some_and(|id| outcome.vectors.contains(&id)) {
            self.vector_edit = None;
            self.scene.clear_preview();
            self.out
                .events
                .push(EditorEvent::Notice(VECTOR_CHANGED_BY_OTHER));
        }
    }

    /// Страницу, на которой человек, удалил другой: уходим на первую. Все страницы удалены одновременно
    /// разными людьми: файл без страниц не открыть, появляется новая **(поведение)**.
    fn follow_pages(&mut self, outcome: &RemoteOutcome) {
        if self.doc.pages().is_empty() {
            let _ = self.doc.add_page("Страница 1");
        }
        if outcome.removed_pages.contains(&self.page) {
            if let Some((first, _)) = self.doc.pages().first().cloned() {
                self.text_edit = None;
                self.vector_edit = None;
                self.scene.clear_preview();
                let _ = self.set_page(first);
                self.out
                    .events
                    .push(EditorEvent::Notice(PAGE_REMOVED_BY_OTHER));
            }
        }
    }
}
