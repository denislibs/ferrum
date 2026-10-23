//! Глобальный стек отмены для всех страниц. Шаг хранит обратные операции
//! своей транзакции и выделение до и после неё.

use crate::page::Inverse;
use crate::types::{NodeId, PageId};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HistoryEntry {
    pub page: PageId,
    /// Для шага в стеке undo — операции отмены; в стеке redo — операции повтора.
    pub ops: Vec<Inverse>,
    pub selection_before: Vec<NodeId>,
    pub selection_after: Vec<NodeId>,
}

#[derive(Default)]
pub(crate) struct History {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl History {
    /// Новый шаг пользователя сбрасывает возможность повтора.
    pub fn push(&mut self, entry: HistoryEntry) {
        self.undo.push(entry);
        self.redo.clear();
    }

    pub fn push_redo(&mut self, entry: HistoryEntry) {
        self.redo.push(entry);
    }

    pub fn push_undo_keep_redo(&mut self, entry: HistoryEntry) {
        self.undo.push(entry);
    }

    pub fn pop_undo(&mut self) -> Option<HistoryEntry> {
        self.undo.pop()
    }

    pub fn pop_redo(&mut self) -> Option<HistoryEntry> {
        self.redo.pop()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn forget_page(&mut self, page: PageId) {
        self.undo.retain(|e| e.page != page);
        self.redo.retain(|e| e.page != page);
    }
}
