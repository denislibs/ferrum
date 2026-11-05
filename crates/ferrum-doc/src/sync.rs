//! Синхронизация с сервером (спека подсистемы 4, разделы 7 и 8): какой документ файла,
//! вектор версий на проводе и курсоры: что сервер уже знает и что ему отправлено.
//! Курсор считает версии независимо от журнала правок (`take_journal`).

use std::collections::HashMap;
use std::fmt;

use loro::{ExportMode, LoroDoc, TextDelta, VersionVector, ID};

use crate::error::DocResult;
use crate::types::{NodeId, PageId};

/// Предел одной пачки: кадр сервера 16 МБ (спека, раздел 7.3), запас — на заголовок кадра.
pub const MAX_SYNC_BATCH_BYTES: usize = 16 * 1024 * 1024 - 1024;
/// Больше стольких записей в векторе версий файл не принимает: столько сеансов правки на один
/// документ не набирается, а чужой кадр не должен задавать размер разбора.
pub const MAX_VERSION_PEERS: usize = 65_536;
/// За один вызов `take_sync_updates`: вставка на сотни мегабайт идёт порциями, по мере места в сокете.
pub const MAX_SYNC_FLUSH_BYTES: usize = 32 * 1024 * 1024;

/// Единица синхронизации: корневой документ или страница.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocKey {
    Root,
    Page(PageId),
}

impl DocKey {
    pub const ROOT: &'static str = "root";

    pub fn parse(s: &str) -> Option<DocKey> {
        if s == DocKey::ROOT {
            return Some(DocKey::Root);
        }
        s.parse().ok().map(DocKey::Page)
    }
}

impl fmt::Display for DocKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocKey::Root => f.write_str(DocKey::ROOT),
            DocKey::Page(p) => p.fmt(f),
        }
    }
}

/// Вектор версий документа: остальные крейты Loro не видят.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncVersion(pub(crate) VersionVector);

impl SyncVersion {
    /// `n: u32 · (peer: u64 · counter: u32) × n`, всё LE: формат `subscribe` и `subscribed`.
    pub fn encode(&self) -> Vec<u8> {
        let mut peers: Vec<_> = self.0.iter().map(|(p, c)| (*p, *c)).collect();
        peers.sort_unstable();
        let mut out = Vec::with_capacity(4 + peers.len() * 12);
        out.extend_from_slice(&(peers.len() as u32).to_le_bytes());
        for (peer, counter) in peers {
            out.extend_from_slice(&peer.to_le_bytes());
            out.extend_from_slice(&(counter as u32).to_le_bytes());
        }
        out
    }

    /// Вектор с провода: сверх предела, обрезанный и с хвостом — `None`.
    pub fn decode(bytes: &[u8]) -> Option<SyncVersion> {
        let n = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
        if n > MAX_VERSION_PEERS || bytes.len() != 4 + n * 12 {
            return None;
        }
        let mut vv = VersionVector::default();
        for i in 0..n {
            let at = 4 + i * 12;
            let peer = u64::from_le_bytes(bytes[at..at + 8].try_into().ok()?);
            let counter = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().ok()?);
            let counter = i32::try_from(counter).ok()?;
            if counter > 0 {
                vv.insert(peer, counter);
            }
        }
        Some(SyncVersion(vv))
    }

    /// Содержит ли этот вектор все правки `other`.
    pub fn includes(&self, other: &SyncVersion) -> bool {
        self.0.includes_vv(&other.0)
    }
}

/// Пачка своих правок для сервера. `end` — версия, которую сервер узнает, приняв пачку.
#[derive(Debug, Clone)]
pub struct SyncBatch {
    pub doc: DocKey,
    pub bytes: Vec<u8>,
    pub end: SyncVersion,
}

/// Чужая вставка или удаление в тексте: куда переехала позиция каретки.
#[derive(Debug, Clone, PartialEq)]
pub struct TextShift(pub(crate) Vec<TextDelta>);

impl TextShift {
    /// Новая позиция символа `at` (символы Unicode). Чужая вставка ровно в позиции `at` курсор не
    /// сдвигает: человек продолжает печатать там, где печатал **(поведение)**.
    pub fn map(&self, at: usize) -> usize {
        let (mut old, mut new) = (0usize, 0usize);
        for op in &self.0 {
            match op {
                TextDelta::Retain { retain, .. } => {
                    if at < old + retain {
                        return new + (at - old);
                    }
                    old += retain;
                    new += retain;
                }
                TextDelta::Insert { insert, .. } => {
                    if old == at {
                        return new;
                    }
                    new += insert.chars().count();
                }
                TextDelta::Delete { delete } => {
                    if at < old + delete {
                        return new;
                    }
                    old += delete;
                }
            }
        }
        new + at.saturating_sub(old)
    }
}

/// Что изменилось после применения пачки чужих правок. Изменения узлов — обычным путём
/// (`Document::take_changes`), кроме `rebuilt`: зеркало страницы построено заново, сцену — тоже.
#[derive(Debug, Clone, Default)]
pub struct RemoteOutcome {
    /// Страница пачки; `None` — корневой документ или страница, которой нет.
    pub page: Option<PageId>,
    /// Часть байтов не применена: повреждена (её отверг бы и сервер) — интерфейс скажет об этом.
    pub failed: bool,
    /// Пачке не хватает предыдущих правок: Loro применит её, когда они придут.
    pub pending: bool,
    pub rebuilt: bool,
    /// Чужие правки текста по узлам, по порядку.
    pub text: Vec<(NodeId, TextShift)>,
    /// Узлы с изменённой сетью вектора.
    pub vectors: Vec<NodeId>,
    /// Корень: список страниц, имя или направляющие могли измениться.
    pub root_changed: bool,
    pub removed_pages: Vec<PageId>,
    /// Новые страницы другого участника.
    pub added_pages: Vec<PageId>,
    /// Изображения, на которые теперь ссылаются узлы, а байтов нет.
    pub missing_images: Vec<String>,
    /// Файл стал «только для чтения»: чужая правка из более новой версии Ferrum.
    pub became_read_only: bool,
}

/// Что сервер знает о документе и что ему отправлено.
#[derive(Debug, Default, Clone)]
pub(crate) struct Cursor {
    /// До ответа `subscribed` версия сервера неизвестна, и отправлять нечего.
    pub ready: bool,
    pub sent: VersionVector,
    pub acked: VersionVector,
}

#[derive(Default)]
pub(crate) struct SyncCursors {
    pub docs: HashMap<DocKey, Cursor>,
    /// Новых пачек не собираем, пока их не примет сторона соединения.
    pub subscribed: std::collections::HashSet<DocKey>,
}

/// Операций за раз, больше которых незачем пробовать собирать одним куском: на большой вставке целый
/// экспорт стоил бы сотни мегабайт на каждый вызов.
const FIRST_STEP_OPS: i32 = 200_000;

/// Обновления `from` … текущая версия кусками не больше `limit` байт, всего — примерно до `budget` байт
/// за вызов: остальное заберёт следующий. Куски идут в порядке счётчиков внутри пира, старые сеансы
/// (пиры прошлых открытий) раньше нынешнего: правки нынешнего могут зависеть от них. Второе значение
/// куска — версия, которую сервер узнает, приняв его.
pub(crate) fn split_updates(
    doc: &LoroDoc,
    from: &VersionVector,
    limit: usize,
    budget: usize,
) -> DocResult<Vec<(Vec<u8>, VersionVector)>> {
    let vv = doc.oplog_vv();
    let own = doc.peer_id();
    let mut spans: Vec<(u64, i32, i32)> = vv
        .iter()
        .filter_map(|(p, c)| {
            let start = from.get(p).copied().unwrap_or(0);
            (*c > start).then_some((*p, start, *c))
        })
        .collect();
    let total: i64 = spans.iter().map(|(_, a, b)| (*b - *a) as i64).sum();
    if total <= FIRST_STEP_OPS as i64 {
        let whole = doc.export(ExportMode::updates(from))?;
        if whole.len() <= limit {
            return Ok(vec![(whole, vv)]);
        }
    }
    spans.sort_by_key(|(p, ..)| (*p == own, *p));
    let mut out = Vec::new();
    let mut end = from.clone();
    let mut size = 0usize;
    for (peer, start, stop) in spans {
        let mut at = start;
        let mut step = (stop - start).clamp(1, FIRST_STEP_OPS);
        while at < stop {
            if size >= budget {
                return Ok(out);
            }
            let upto = (at + step).min(stop);
            let span = loro::IdSpan::new(peer, at, upto);
            let bytes = doc.export(ExportMode::updates_in_range(vec![span]))?;
            // Одна операция сверх предела делиться не может: её откажет сервер, причину увидит человек.
            if bytes.len() > limit && upto - at > 1 {
                step = ((upto - at) / 2).max(1);
                continue;
            }
            end.extend_to_include_last_id(ID::new(peer, upto - 1));
            size += bytes.len();
            out.push((bytes, end.clone()));
            at = upto;
        }
    }
    Ok(out)
}

/// Объединяет: всё, что есть в `other`, считается и в `into`.
pub(crate) fn merge(into: &mut VersionVector, other: &VersionVector) {
    into.extend_to_include_vv(other.iter());
}

/// Что принесла пачка: конец её версий по пирам. Снимок или повреждённые байты — пусто.
pub(crate) fn blob_end(bytes: &[u8]) -> VersionVector {
    LoroDoc::decode_import_blob_meta(bytes, false)
        .map(|m| m.partial_end_vv)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use loro::LoroDoc;

    /// Документ с правками одного пира: каждая вставка — своя операция.
    fn doc_with_ops(n: usize) -> LoroDoc {
        let doc = LoroDoc::new();
        let list = doc.get_list("l");
        for i in 0..n {
            list.insert(i, "значение, которое занимает место в пачке")
                .unwrap();
        }
        doc.commit();
        doc
    }

    /// Все куски, забирая вызов за вызовом, как `take_sync_updates`.
    fn drain(doc: &LoroDoc, limit: usize, budget: usize) -> (Vec<Vec<u8>>, usize) {
        let mut from = VersionVector::default();
        let mut all = Vec::new();
        let mut calls = 0;
        while !from.includes_vv(&doc.oplog_vv()) {
            calls += 1;
            assert!(calls < 10_000, "остаток не уменьшается");
            let part = split_updates(doc, &from, limit, budget).unwrap();
            assert!(!part.is_empty(), "вызов ничего не отдал");
            for (bytes, end) in part {
                merge(&mut from, &end);
                all.push(bytes);
            }
        }
        (all, calls)
    }

    #[test]
    fn budget_splits_work_between_calls_and_nothing_is_lost() {
        let doc = doc_with_ops(4000);
        let limit = 4 * 1024;
        let budget = 16 * 1024;
        let first = split_updates(&doc, &VersionVector::default(), limit, budget).unwrap();
        let size: usize = first.iter().map(|(b, _)| b.len()).sum();
        assert!(size >= budget && size < budget + limit, "порция {size}");
        assert!(
            !first.last().unwrap().1.includes_vv(&doc.oplog_vv()),
            "за один вызов не всё"
        );
        let (chunks, calls) = drain(&doc, limit, budget);
        assert!(calls > 1);
        assert!(chunks.iter().all(|c| c.len() <= limit));
        let got = LoroDoc::new();
        for c in &chunks {
            got.import(c).unwrap();
        }
        assert_eq!(got.get_deep_value(), doc.get_deep_value());
    }

    #[test]
    fn small_edit_goes_whole_in_one_chunk() {
        let doc = doc_with_ops(10);
        let (chunks, calls) = drain(&doc, 1024 * 1024, 16);
        assert_eq!((chunks.len(), calls), (1, 1));
    }
}
