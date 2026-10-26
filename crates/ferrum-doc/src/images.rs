//! Байты изображений по хэшу blake3. В историю Loro не попадают.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Default)]
pub struct ImageStore {
    by_hash: HashMap<String, Vec<u8>>,
    /// Добавленные с прошлой пачки журнала (`take_fresh`); прочитанные из файла
    /// сюда не попадают — они уже в нём.
    /// Под `Mutex`: полное сохранение (`&Document`) возвращает сюда картинки, которых
    /// нет в файле.
    fresh: Mutex<Vec<String>>,
}

impl ImageStore {
    /// Сохраняет байты и возвращает хэш (hex). Повторное добавление — без копии.
    pub fn add(&mut self, bytes: Vec<u8>) -> String {
        let hash = blake3::hash(&bytes).to_hex().to_string();
        if let Entry::Vacant(e) = self.by_hash.entry(hash.clone()) {
            e.insert(bytes);
            self.fresh_list().push(hash.clone());
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

    fn fresh_list(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
        self.fresh.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Полное сохранение записало только изображения со ссылками, а пачки до файла
    /// стёрты: остальные снова считаются свежими и попадут в следующую пачку журнала —
    /// иначе отмена, вернувшая ссылку, оставила бы её без байтов.
    pub(crate) fn mark_unsaved(&self, saved: &HashSet<String>) {
        let mut fresh = self.fresh_list();
        for hash in self.by_hash.keys().filter(|h| !saved.contains(*h)) {
            if !fresh.contains(hash) {
                fresh.push(hash.clone());
            }
        }
    }

    /// Изображения, добавленные с прошлого вызова, — для пачки журнала.
    pub(crate) fn take_fresh(&mut self) -> Vec<(String, Vec<u8>)> {
        let fresh = std::mem::take(&mut *self.fresh_list());
        fresh
            .into_iter()
            .filter_map(|h| {
                let bytes = self.by_hash.get(&h)?.clone();
                Some((h, bytes))
            })
            .collect()
    }
}
