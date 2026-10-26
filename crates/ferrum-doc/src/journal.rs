//! Журнал изменений (этап 6, спека 10): инкрементальные обновления Loro корневого
//! документа и страниц и новые изображения с прошлой записи. Автосохранение пишет
//! пачку журнала за миллисекунды, а полный файл — редко; после сбоя пачки, записанные
//! после последнего полного файла, применяются к нему при открытии.
//!
//! ```text
//! magic "FERRUMJ\0" (8 байт) · версия u32 LE · число секций u32 LE
//! секция — как в `.ferrum`: тип u8 · длина id u32 · id · длина данных u64 · blake3 · данные
//! ```
//! Типы секций: 1 — обновления корневого документа, 2 — обновления страницы
//! (id = PageId), 3 — изображение (id = хэш blake3).
//!
//! Пачка не доверенная (её читают из хранилища): неверный заголовок, обрезка или
//! чужая контрольная сумма — вся пачка пропускается с предупреждением. Обновления,
//! которые уже есть в документе, Loro пропускает сам, поэтому повтор пачки безвреден.

use std::collections::HashMap;

use crate::binary::Reader;
use crate::container::push_section;
use crate::types::PageId;

pub const MAGIC: &[u8; 8] = b"FERRUMJ\0";
pub const VERSION: u32 = 1;

pub(crate) const ROOT: u8 = 1;
pub(crate) const PAGE: u8 = 2;
pub(crate) const IMAGE: u8 = 3;

/// Собирает пачку из секций; `None` — секций нет.
pub(crate) struct BatchWriter {
    sections: Vec<u8>,
    count: u32,
}

impl BatchWriter {
    pub fn new() -> BatchWriter {
        BatchWriter {
            sections: Vec::new(),
            count: 0,
        }
    }

    pub fn push(&mut self, kind: u8, id: &str, data: &[u8]) {
        push_section(&mut self.sections, kind, id, data);
        self.count += 1;
    }

    pub fn finish(self) -> Option<Vec<u8>> {
        if self.count == 0 {
            return None;
        }
        let mut out = Vec::with_capacity(16 + self.sections.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&self.count.to_le_bytes());
        out.extend_from_slice(&self.sections);
        Some(out)
    }
}

/// Разобранная пачка: обновления корня, страниц (в порядке записи) и изображения.
#[derive(Default)]
pub(crate) struct Batch {
    pub root: Vec<Vec<u8>>,
    pub pages: Vec<(PageId, Vec<u8>)>,
    pub images: Vec<(String, Vec<u8>)>,
}

/// Разбирает пачку целиком или не принимает её: применять половину пачки нельзя —
/// её секции записаны одним шагом автосохранения.
pub(crate) fn parse(bytes: &[u8]) -> Option<Batch> {
    if bytes.len() < 16 || &bytes[..8] != MAGIC {
        return None;
    }
    let mut r = Reader::at(bytes, 8);
    if r.u32()? > VERSION {
        return None;
    }
    let count = r.u32()?;
    let mut batch = Batch::default();
    for _ in 0..count {
        let kind = r.u8()?;
        let id_len = r.u32()? as usize;
        let id = String::from_utf8(r.take(id_len)?.to_vec()).ok()?;
        let len = usize::try_from(r.u64()?).ok()?;
        let checksum: [u8; 32] = r.take(32)?.try_into().ok()?;
        let data = r.take(len)?;
        if blake3::hash(data).as_bytes() != &checksum {
            return None;
        }
        match kind {
            ROOT => batch.root.push(data.to_vec()),
            PAGE => batch.pages.push((id.parse().ok()?, data.to_vec())),
            // Хэш содержимого — проверка подмены id.
            IMAGE if blake3::hash(data).to_hex().as_str() == id => {
                batch.images.push((id, data.to_vec()))
            }
            IMAGE => return None,
            _ => {} // секции из будущих версий журнала пропускаем
        }
    }
    Some(batch)
}

/// Итог применения журнала при открытии.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Replayed {
    /// Пачек применено (повреждённые не считаются; 0, если документ не изменился).
    pub applied: usize,
    /// Что восстановить не удалось — по-русски, для пользователя.
    pub warnings: Vec<String>,
}

/// Обновления страниц по порядку, сгруппированные по странице.
pub(crate) fn group_pages(list: Vec<(PageId, Vec<u8>)>) -> Vec<(PageId, Vec<Vec<u8>>)> {
    let mut order = Vec::new();
    let mut by: HashMap<PageId, Vec<Vec<u8>>> = HashMap::new();
    for (id, data) in list {
        if !by.contains_key(&id) {
            order.push(id);
        }
        by.entry(id).or_default().push(data);
    }
    order
        .into_iter()
        .map(|id| {
            let list = by.remove(&id).unwrap_or_default();
            (id, list)
        })
        .collect()
}
