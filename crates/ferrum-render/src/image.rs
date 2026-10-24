//! Изображения для заливок. Рендер не декодирует байты документа сам: картинки
//! даёт поставщик по хэшу (в браузере их декодирует `createImageBitmap` в воркере,
//! в тестах и экспорте — `decode_png`).

use std::collections::HashMap;
use std::sync::Arc;

use vello_cpu::Pixmap;

/// Наибольшая сторона изображения: `Pixmap` хранит размеры в `u16`, а картинка из
/// файла не должна съесть память (16384² × 4 байта = 1 ГБ — уже слишком).
///
/// Это лимит одной картинки. Общий бюджет памяти на все декодированные картинки
/// держит поставщик изображений (он решает, что держать, а что выгружать; в этой
/// задаче его ещё нет). Сам `decode_png` ничего не кэширует.
pub const MAX_IMAGE_SIDE: u16 = 8192;

/// Наибольший размер декодированной картинки в RGBA8: 256 МБ. При текущем
/// `MAX_IMAGE_SIDE` (8192² × 4 = ровно 256 МБ) проверка не срабатывает. Она нужна,
/// если сторону поднимут выше.
const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

pub trait ImageProvider {
    /// Готовое к отрисовке изображение (RGBA с премультипликацией) или `None`,
    /// если его ещё нет — тогда рисуется заглушка.
    fn image(&self, hash: &str) -> Option<Arc<Pixmap>>;
}

impl ImageProvider for HashMap<String, Arc<Pixmap>> {
    fn image(&self, hash: &str) -> Option<Arc<Pixmap>> {
        self.get(hash).cloned()
    }
}

/// Поставщик без изображений.
pub struct NoImages;

impl ImageProvider for NoImages {
    fn image(&self, _: &str) -> Option<Arc<Pixmap>> {
        None
    }
}

/// Декодирует PNG. Повреждённые и слишком большие картинки дают `None`.
///
/// Размер проверяется по заголовку до декодирования: иначе файл с размером
/// 65535×65535 в заголовке заставил бы декодер выделить 16 ГБ.
pub fn decode_png(bytes: &[u8]) -> Option<Pixmap> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let side = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4 байта"));
    let (w, h) = (side(16), side(20));
    let max = MAX_IMAGE_SIDE as u32;
    if w == 0 || h == 0 || w > max || h > max {
        return None;
    }
    // Декодированный размер: u64, чтобы произведение не переполнилось.
    if w as u64 * h as u64 * 4 > MAX_IMAGE_BYTES {
        return None;
    }
    Pixmap::from_png(std::io::Cursor::new(bytes)).ok()
}
