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
/// Сколько байт декодированного за кадр: две картинки по 256 МБ в один кадр не
/// декодируются. Одна картинка за кадр разрешена всегда, иначе большая не
/// появилась бы никогда.
pub const DECODE_BYTES_PER_FRAME: usize = 64 * 1024 * 1024;

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
    frame_bytes: usize,
    frame_limit: usize,
    decoded: u64,
    placeholders: u64,
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
        DocImages::with_limits(limit, DECODE_BYTES_PER_FRAME)
    }

    /// Предел кэша и предел декодированных за кадр байт.
    pub fn with_limits(limit: usize, frame_limit: usize) -> DocImages {
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
                frame_bytes: 0,
                frame_limit,
                decoded: 0,
                placeholders: 0,
                broken: Arc::new(broken),
            }),
        }
    }

    /// Начало кадра: новый бюджет декодирования.
    pub fn begin_frame(&self) {
        let mut i = self.inner.borrow_mut();
        i.frame += 1;
        i.decodes_left = DECODES_PER_FRAME;
        i.frame_bytes = 0;
    }

    /// Сколько раз вместо картинки отдана заглушка (нет байтов, не читается, не
    /// помещается в предел памяти).
    pub fn placeholders_served(&self) -> u64 {
        self.inner.borrow().placeholders
    }

    /// Кладёт уже декодированную картинку (размещение изображения).
    pub fn insert(&self, hash: &str, pixmap: Pixmap) {
        let mut i = self.inner.borrow_mut();
        let frame = i.frame;
        let need = pixmap.width() as usize * pixmap.height() as usize * 4;
        // Не помещается — не кэшируем: рендер декодирует по байтам документа или
        // отдаст заглушку.
        if i.make_room(need, frame) {
            i.put(hash, Arc::new(pixmap), frame);
        }
    }

    /// Поставщик для рендера: картинки из байтов `store`.
    pub fn provider<'a>(&'a self, store: &'a ImageStore) -> Provider<'a> {
        Provider {
            images: self,
            store,
            unbudgeted: false,
        }
    }

    /// Поставщик для экспорта: без счётного и байтового предела на кадр (экспорт
    /// не может «доделать» картинку в следующем кадре), предел памяти кэша прежний.
    pub fn export_provider<'a>(&'a self, store: &'a ImageStore) -> Provider<'a> {
        self.begin_frame();
        Provider {
            images: self,
            store,
            unbudgeted: true,
        }
    }

    /// Картинок в кэше, их байт и сколько всего декодировано.
    pub fn stats(&self) -> (usize, usize, u64) {
        let i = self.inner.borrow();
        (i.map.len(), i.bytes, i.decoded)
    }
}

/// Размер декодированной картинки по заголовку PNG, не декодируя её.
fn png_decoded_bytes(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let side = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4 байта"));
    usize::try_from(side(16) as u64 * side(20) as u64 * 4).ok()
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
    }

    /// Вытесняет давно не рисовавшиеся картинки (кроме нужных в этом кадре), пока
    /// не освободится `need` байт. `false` — места нет: видимые картинки сами
    /// занимают предел.
    fn make_room(&mut self, need: usize, frame: u64) -> bool {
        while self.bytes + need > self.limit {
            let oldest = self
                .map
                .iter()
                .filter(|(_, e)| e.last_used < frame)
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone());
            let Some(key) = oldest else { return false };
            let e = self.map.remove(&key).expect("запись есть");
            self.bytes -= e.bytes();
        }
        true
    }
}

pub struct Provider<'a> {
    images: &'a DocImages,
    store: &'a ImageStore,
    unbudgeted: bool,
}

impl ImageProvider for Provider<'_> {
    fn image(&self, hash: &str) -> Option<Arc<Pixmap>> {
        let mut i = self.images.inner.borrow_mut();
        let frame = i.frame;
        if let Some(e) = i.map.get_mut(hash) {
            e.last_used = frame;
            let pixmap = e.pixmap.clone();
            if Arc::ptr_eq(&pixmap, &i.broken) {
                i.placeholders += 1;
            }
            return Some(pixmap);
        }
        let Some(bytes) = self.store.get(hash) else {
            // Байтов нет (файл открыт без части картинок): постоянная заглушка.
            i.placeholders += 1;
            return Some(i.broken.clone());
        };
        let Some(cost) = png_decoded_bytes(bytes) else {
            i.placeholders += 1;
            return Some(i.broken.clone());
        };
        if !self.unbudgeted {
            if i.decodes_left == 0 {
                return None;
            }
            // Байтовый бюджет кадра; первая картинка кадра проходит всегда.
            if i.frame_bytes > 0 && i.frame_bytes + cost > i.frame_limit {
                return None;
            }
        }
        // Память кэша: видимые картинки не вытесняются, и если новая не
        // помещается — заглушка без нового кадра (иначе кадры требовались бы вечно,
        // а память уходила за 4 ГБ wasm32). Заглушку не кэшируем: место может
        // освободиться.
        if !i.make_room(cost, frame) {
            i.placeholders += 1;
            return Some(i.broken.clone());
        }
        i.decodes_left = i.decodes_left.saturating_sub(1);
        i.frame_bytes += cost;
        i.decoded += 1;
        let Some(pixmap) = decode_png(bytes).map(Arc::new) else {
            // Неразборчивая картинка: постоянная заглушка в кэше.
            i.placeholders += 1;
            let broken = i.broken.clone();
            i.put(hash, broken.clone(), frame);
            return Some(broken);
        };
        i.put(hash, pixmap.clone(), frame);
        Some(pixmap)
    }
}
