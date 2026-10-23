//! Байты изображений по хэшу blake3. В историю Loro не попадают.

use std::collections::HashMap;

#[derive(Default)]
pub struct ImageStore {
    by_hash: HashMap<String, Vec<u8>>,
}

impl ImageStore {
    /// Сохраняет байты и возвращает хэш (hex). Повторное добавление — без копии.
    pub fn add(&mut self, bytes: Vec<u8>) -> String {
        let hash = blake3::hash(&bytes).to_hex().to_string();
        self.by_hash.entry(hash.clone()).or_insert(bytes);
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

    pub(crate) fn insert_verified(&mut self, hash: String, bytes: Vec<u8>) {
        self.by_hash.insert(hash, bytes);
    }
}
