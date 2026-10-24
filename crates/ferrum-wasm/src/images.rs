//! Картинки документа для рендера: декодируются в worker'е по первому запросу и
//! живут в кэше с пределом памяти. Рендер спрашивает картинку по хэшу на каждом
//! кадре; если её ещё нет, рисуется заглушка и кадр помечается неполным — следующий
//! кадр декодирует дальше. Так открытие страницы с сотней картинок не останавливает
//! worker на секунды: за кадр декодируется не больше `DECODES_PER_FRAME`.
//!
//! Картинка без байтов в документе или неразборчивая заменяется постоянной
//! заглушкой (1×1 цвета заглушки рендера): иначе её «отсутствие» требовало бы
//! нового кадра вечно.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use ferrum_doc::ImageStore;
use ferrum_render::{decode_png, ImageProvider, Pixmap};

/// Предел памяти декодированных картинок.
pub const IMAGE_CACHE_BYTES: usize = 256 * 1024 * 1024;
/// Сколько картинок декодировать за кадр.
pub const DECODES_PER_FRAME: usize = 2;

struct Entry {
    pixmap: Arc<Pixmap>,
    last_used: u64,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.pixmap.width() as usize * self.pixmap.height() as usize * 4
    }
}

struct Inner {
    map: HashMap<String, Entry>,
    bytes: usize,
    limit: usize,
    frame: u64,
    decodes_left: usize,
    decoded: u64,
    broken: Arc<Pixmap>,
}

pub struct DocImages {
    inner: RefCell<Inner>,
}

impl Default for DocImages {
    fn default() -> DocImages {
        DocImages::with_limit(IMAGE_CACHE_BYTES)
    }
}

impl DocImages {
    pub fn with_limit(limit: usize) -> DocImages {
        // Цвет заглушки рендера (0.85 серого), непрозрачный, с премультипликацией.
        let mut broken = Pixmap::new(1, 1);
        broken
            .data_as_u8_slice_mut()
            .copy_from_slice(&[217, 217, 217, 255]);
        DocImages {
            inner: RefCell::new(Inner {
                map: HashMap::new(),
                bytes: 0,
                limit,
                frame: 0,
                decodes_left: DECODES_PER_FRAME,
                decoded: 0,
                broken: Arc::new(broken),
            }),
        }
    }

    /// Начало кадра: новый бюджет декодирования.
    pub fn begin_frame(&self) {
        let mut i = self.inner.borrow_mut();
        i.frame += 1;
        i.decodes_left = DECODES_PER_FRAME;
    }

    /// Кладёт уже декодированную картинку (размещение изображения).
    pub fn insert(&self, hash: &str, pixmap: Pixmap) {
        let mut i = self.inner.borrow_mut();
        let frame = i.frame;
        i.put(hash, Arc::new(pixmap), frame);
    }

    /// Поставщик для рендера: картинки из байтов `store`.
    pub fn provider<'a>(&'a self, store: &'a ImageStore) -> Provider<'a> {
        Provider {
            images: self,
            store,
        }
    }

    /// Картинок в кэше, их байт и сколько всего декодировано.
    pub fn stats(&self) -> (usize, usize, u64) {
        let i = self.inner.borrow();
        (i.map.len(), i.bytes, i.decoded)
    }
}

impl Inner {
    fn put(&mut self, hash: &str, pixmap: Arc<Pixmap>, frame: u64) {
        let entry = Entry {
            pixmap,
            last_used: frame,
        };
        self.bytes += entry.bytes();
        if let Some(old) = self.map.insert(hash.to_string(), entry) {
            self.bytes -= old.bytes();
        }
        // Предел: вытесняются давно не рисовавшиеся, кроме нужных в этом кадре.
        while self.bytes > self.limit {
            let oldest = self
                .map
                .iter()
                .filter(|(_, e)| e.last_used < frame)
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone());
            let Some(key) = oldest else { break };
            let e = self.map.remove(&key).expect("запись есть");
            self.bytes -= e.bytes();
        }
    }
}

pub struct Provider<'a> {
    images: &'a DocImages,
    store: &'a ImageStore,
}

impl ImageProvider for Provider<'_> {
    fn image(&self, hash: &str) -> Option<Arc<Pixmap>> {
        let mut i = self.images.inner.borrow_mut();
        let frame = i.frame;
        if let Some(e) = i.map.get_mut(hash) {
            e.last_used = frame;
            return Some(e.pixmap.clone());
        }
        let Some(bytes) = self.store.get(hash) else {
            // Байтов нет (файл открыт без части картинок): постоянная заглушка.
            return Some(i.broken.clone());
        };
        if i.decodes_left == 0 {
            return None;
        }
        i.decodes_left -= 1;
        i.decoded += 1;
        let pixmap = match decode_png(bytes) {
            Some(p) => Arc::new(p),
            None => i.broken.clone(),
        };
        i.put(hash, pixmap.clone(), frame);
        Some(pixmap)
    }
}
