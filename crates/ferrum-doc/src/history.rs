//! Глобальный стек отмены для всех страниц. Шаг хранит обратные операции
//! своей транзакции и выделение до и после неё.

use crate::page::Inverse;
use crate::types::{NodeId, PageId};

/// Пауза, после которой ввод подряд начинает новый шаг отмены (как в Figma).
pub const MERGE_PAUSE_MS: f64 = 1000.0;

/// Метка правки текста, которая сливается с прошлым шагом отмены: ввод подряд в
/// одном месте — один шаг (спека подсистемы 3, раздел 3). Шаг рвут пауза больше
/// `MERGE_PAUSE_MS`, другая позиция курсора, другой узел или другой стиль.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMerge {
    pub node: NodeId,
    /// Курсор до и после правки, символы Unicode.
    pub before: usize,
    pub after: usize,
    /// Отпечаток стиля вставки (его считает редактор).
    pub style: u64,
    /// Время правки, мс (часы вызывающего).
    pub at_ms: f64,
}

impl TextMerge {
    /// Продолжает ли `next` эту правку.
    pub fn continues(&self, next: &TextMerge) -> bool {
        let pause = next.at_ms - self.at_ms;
        self.node == next.node
            && self.style == next.style
            && self.after == next.before
            && (0.0..=MERGE_PAUSE_MS).contains(&pause)
    }
}

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
    /// Метка верхнего шага undo, если его записала сливаемая правка текста и с
    /// тех пор стек не менялся; иначе `None` — следующей правке не с чем слиться.
    last_merge: Option<(PageId, TextMerge)>,
}

impl History {
    /// Новый шаг пользователя сбрасывает возможность повтора.
    pub fn push(&mut self, entry: HistoryEntry) {
        self.undo.push(entry);
        self.redo.clear();
        self.last_merge = None;
    }

    /// Шаг сливаемой правки: продолжает верхний шаг (операции дописываются в
    /// конец — отмена применяет их с конца) или становится новым шагом.
    pub fn push_merging(&mut self, entry: HistoryEntry, merge: TextMerge) {
        let page = entry.page;
        let fits =
            matches!(&self.last_merge, Some((p, prev)) if *p == page && prev.continues(&merge));
        match self.undo.last_mut() {
            Some(top) if fits => {
                top.ops.extend(entry.ops);
                top.selection_after = entry.selection_after;
                self.redo.clear();
            }
            _ => self.push(entry),
        }
        self.last_merge = Some((page, merge));
    }

    pub fn push_redo(&mut self, entry: HistoryEntry) {
        self.redo.push(entry);
        self.last_merge = None;
    }

    pub fn push_undo_keep_redo(&mut self, entry: HistoryEntry) {
        self.undo.push(entry);
        self.last_merge = None;
    }

    pub fn pop_undo(&mut self) -> Option<HistoryEntry> {
        self.last_merge = None;
        self.undo.pop()
    }

    pub fn pop_redo(&mut self) -> Option<HistoryEntry> {
        self.last_merge = None;
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
        self.last_merge = None;
    }
}
