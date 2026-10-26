//! Байты изображений по хэшу blake3. В историю Loro не попадают.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

#[derive(Default)]
pub struct ImageStore {
    by_hash: HashMap<String, Vec<u8>>,
    /// Добавленные с прошлой пачки журнала (`take_fresh`); прочитанные из файла
    /// сюда не попадают — они уже в нём.
    fresh: Vec<String>,
}

impl ImageStore {
    /// Сохраняет байты и возвращает хэш (hex). Повторное добавление — без копии.
    pub fn add(&mut self, bytes: Vec<u8>) -> String {
        let hash = blake3::hash(&bytes).to_hex().to_string();
        if let Entry::Vacant(e) = self.by_hash.entry(hash.clone()) {
            e.insert(bytes);
            self.fresh.push(hash.clone());
        }
        hash
    }

    pub fn get(&self, hash: &str) -> Option<&[u8]> {
        self.by_hash.get(hash).map(Vec::as_slice)
    }

    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    /// Вставляет байты без пересчёта хэша. Вызывающий обязан до вызова проверить,
    /// что `hash` — это blake3 от `bytes` (в контейнере это делает `container::decode`).
    pub(crate) fn insert_verified(&mut self, hash: String, bytes: Vec<u8>) {
        self.by_hash.insert(hash, bytes);
    }

    /// Изображения, добавленные с прошлого вызова, — для пачки журнала.
    pub(crate) fn take_fresh(&mut self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.fresh)
            .into_iter()
            .filter_map(|h| {
                let bytes = self.by_hash.get(&h)?.clone();
                Some((h, bytes))
            })
            .collect()
    }
}
