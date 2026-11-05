//! Глобальный стек отмены для всех страниц. Шаг хранит обратные операции
//! своей транзакции и выделение до и после неё.

use crate::guides::Guide;
use crate::page::Inverse;
use crate::text_ot::{self, Tie};
use crate::types::{Color, NodeId, PageId};

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

/// Обратная операция над самим документом, а не над страницей: дерево страниц
/// живёт в корневом `LoroDoc` (спека подсистемы 2, разделы 9 и 10).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DocOp {
    /// Прежние направляющие страницы.
    PageGuides { page: PageId, guides: Vec<Guide> },
    /// Прежнее место страницы в списке.
    MovePage { page: PageId, index: usize },
    /// Отмена дубля: копия уходит из списка и ждёт повтора в `Document::parked`.
    RemovePage { page: PageId },
    /// Повтор дубля: страница из `Document::parked` возвращается в список. В дереве
    /// страниц Loro она получает новый идентификатор (`History::remap_page`).
    RestorePage {
        page: PageId,
        index: usize,
        name: String,
        background: Option<Color>,
        guides: Vec<Guide>,
    },
}

impl DocOp {
    fn page(&self) -> PageId {
        match self {
            DocOp::PageGuides { page, .. }
            | DocOp::MovePage { page, .. }
            | DocOp::RemovePage { page }
            | DocOp::RestorePage { page, .. } => *page,
        }
    }

    fn page_mut(&mut self) -> &mut PageId {
        match self {
            DocOp::PageGuides { page, .. }
            | DocOp::MovePage { page, .. }
            | DocOp::RemovePage { page }
            | DocOp::RestorePage { page, .. } => page,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HistoryEntry {
    /// Номер шага: один и тот же в стеках undo и redo. Его выдаёт `History::push`.
    pub id: u64,
    /// Страница шага: на неё возвращает отмена, ей принадлежат `ops`.
    pub page: PageId,
    /// Куда возвращает повтор, если не на `page` («Дублировать страницу» — на копию).
    pub page_after: Option<PageId>,
    /// Для шага в стеке undo — операции отмены; в стеке redo — операции повтора.
    pub ops: Vec<Inverse>,
    /// Операции над документом в том же направлении.
    pub doc_ops: Vec<DocOp>,
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
    next_id: u64,
}

impl HistoryEntry {
    /// Шаг опирается на страницу: она удалена — шаг не применить.
    fn references(&self, page: PageId) -> bool {
        self.page == page
            || self.page_after == Some(page)
            || self.doc_ops.iter().any(|op| op.page() == page)
    }

    fn remap_page(&mut self, old: PageId, new: PageId) {
        if self.page == old {
            self.page = new;
        }
        if self.page_after == Some(old) {
            self.page_after = Some(new);
        }
        for op in &mut self.doc_ops {
            if op.page() == old {
                *op.page_mut() = new;
            }
        }
    }
}

impl History {
    /// Новый шаг пользователя сбрасывает возможность повтора.
    pub fn push(&mut self, mut entry: HistoryEntry) {
        self.next_id += 1;
        entry.id = self.next_id;
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

    /// Следующая сливаемая правка начинает новый шаг (новый сеанс правки текста).
    pub fn break_merge(&mut self) {
        self.last_merge = None;
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

    /// Номер верхнего шага отмены: кнопка «Отменить» в уведомлении проверяет, что
    /// её шаг ещё последний.
    pub fn top_undo_id(&self) -> Option<u64> {
        self.undo.last().map(|e| e.id)
    }

    /// Забывает шаги, опирающиеся на удалённую страницу. Отброшенный шаг дубля
    /// уносит с собой копию, которая должна была вернуться повтором: шаги про неё
    /// тоже забываются (по цепочке). Возвращает такие копии — документ освободит их
    /// из парковки.
    pub fn forget_page(&mut self, page: PageId) -> Vec<PageId> {
        let mut orphans = Vec::new();
        let mut queue = vec![page];
        while let Some(p) = queue.pop() {
            for stack in [&mut self.undo, &mut self.redo] {
                stack.retain(|e| {
                    if !e.references(p) {
                        return true;
                    }
                    for op in &e.doc_ops {
                        if let DocOp::RestorePage { page, .. } = op {
                            if !orphans.contains(page) {
                                orphans.push(*page);
                                queue.push(*page);
                            }
                        }
                    }
                    false
                });
            }
        }
        self.last_merge = None;
        orphans
    }

    /// Страница, возвращённая повтором дубля, получила новый идентификатор:
    /// шаги повтора после дубля теперь про неё.
    pub fn remap_page(&mut self, old: PageId, new: PageId) {
        for e in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            e.remap_page(old, new);
        }
    }

    /// Другой участник правил текст `node`: дельты отмены и повтора этого текста сдвигаются. Верхний
    /// шаг написан к нынешнему тексту, следующий — к тексту после его применения, поэтому чужая правка
    /// проходит стек сверху вниз и сама преобразуется каждым шагом. Вставка в одно место: своя встаёт
    /// за чужой, поэтому в обратную сторону чужая идёт раньше своей, иначе текст, к которому приводятся
    /// нижние шаги, разойдётся с настоящим, и отмена снимет чужой символ.
    /// Склейка ввода рвётся: позиции курсора прошлой правки больше не те.
    pub fn transform_text(&mut self, page: PageId, node: NodeId, delta: &[loro::TextDelta]) {
        for stack in [&mut self.undo, &mut self.redo] {
            let mut incoming = delta.to_vec();
            for e in stack.iter_mut().rev().filter(|e| e.page == page) {
                for op in e.ops.iter_mut().rev() {
                    if let Inverse::Text(n, d) = op {
                        if *n == node {
                            let moved = text_ot::transform(d, &incoming, Tie::BFirst);
                            incoming = text_ot::transform(&incoming, d, Tie::AFirst);
                            *d = moved;
                        }
                    }
                }
            }
        }
        self.last_merge = None;
    }

    /// Шаги повтора ещё есть: припаркованная копия страницы может понадобиться.
    pub fn has_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
