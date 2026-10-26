//! Формат файла `.ferrum`.
//!
//! ```text
//! magic "FERRUM\0\0" (8 байт) · версия формата u32 LE · число секций u32 LE
//! секция: тип u8 · длина id u32 · id (UTF-8) · длина данных u64 · blake3(данные) 32 байта · данные
//! ```
//! Типы секций: 1 — корневой документ, 2 — страница (id = PageId, shallow-снапшот Loro),
//! 3 — ссылки страницы на изображения (id = PageId, хэши через `\n`),
//! 4 — изображение (id = хэш blake3), 5 — кэш сцены страницы
//! (id = PageId, данные — blake3 снапшота страницы 32 байта · версия кэша u8 · таблица узлов).
//!
//! Кэш сцены — производные данные со своей версией: кэш неизвестной версии молча
//! игнорируется (страница строится из Loro), поэтому формат таблицы можно менять
//! без повышения `FORMAT_VERSION`.
//!
//! Секции неизвестного типа при чтении пропускаются без предупреждений и при
//! пересохранении теряются. Поэтому новый тип секции, который обязан пережить
//! открытие и сохранение в старой сборке, требует повышения `FORMAT_VERSION`
//! (старая сборка тогда откажется открывать файл, а не тихо потеряет данные).

use std::collections::{HashMap, HashSet};

use crate::binary::Reader;
use crate::document::{Document, UnloadedPage};
use crate::error::{DocError, DocResult};
use crate::images::ImageStore;
use crate::page::PageDoc;
use crate::scene_cache;
use crate::types::PageId;

pub const MAGIC: &[u8; 8] = b"FERRUM\0\0";
pub const FORMAT_VERSION: u32 = 1;
/// Версия таблицы в секции кэша сцены; растёт при любом изменении `scene_cache`.
/// 2 — флаги отражения `flipX`/`flipY`. Таблица версии 1 не принимается, хотя её
/// строки читались бы и сейчас: старая сборка, пересохранившая новый файл, пишет
/// таблицу без флагов рядом со снапшотом, где они есть, а зеркало страницы
/// берётся из таблицы — флаги тихо пропали бы и из правок.
pub const SCENE_CACHE_VERSION: u8 = 2;

const ROOT: u8 = 1;
const PAGE: u8 = 2;
const PAGE_REFS: u8 = 3;
const IMAGE: u8 = 4;
const SCENE: u8 = 5;

/// Открытый файл и предупреждения о том, что прочитать не удалось.
pub struct Opened {
    pub document: Document,
    pub warnings: Vec<String>,
}

fn push_section(out: &mut Vec<u8>, kind: u8, id: &str, data: &[u8]) {
    out.push(kind);
    out.extend_from_slice(&(id.len() as u32).to_le_bytes());
    out.extend_from_slice(id.as_bytes());
    out.extend_from_slice(&(data.len() as u64).to_le_bytes());
    out.extend_from_slice(blake3::hash(data).as_bytes());
    out.extend_from_slice(data);
}

/// Сериализует документ. Изображения без ссылок со страниц не записываются.
pub fn encode(doc: &Document) -> DocResult<Vec<u8>> {
    let mut sections = Vec::new();
    let mut count = 0u32;
    push_section(&mut sections, ROOT, "", &doc.root_snapshot()?);
    count += 1;
    let mut used = HashSet::new();
    for (id, _) in doc.pages() {
        let payload = doc.page_payload(id)?;
        let page = id.to_string();
        push_section(&mut sections, PAGE, &page, &payload.snapshot);
        push_section(
            &mut sections,
            PAGE_REFS,
            &page,
            payload.image_refs.join("\n").as_bytes(),
        );
        count += 2;
        // Страница, которая не помещается в таблицу кэша, сохраняется без него.
        if let Some(table) = payload.scene.as_deref().and_then(scene_cache::encode) {
            let mut data = blake3::hash(&payload.snapshot).as_bytes().to_vec();
            data.push(SCENE_CACHE_VERSION);
            data.extend_from_slice(&table);
            push_section(&mut sections, SCENE, &page, &data);
            count += 1;
        }
        used.extend(payload.image_refs);
    }
    let mut used: Vec<String> = used.into_iter().collect();
    used.sort();
    for hash in used {
        if let Some(bytes) = doc.images.get(&hash) {
            push_section(&mut sections, IMAGE, &hash, bytes);
            count += 1;
        }
    }
    let mut out = Vec::with_capacity(16 + sections.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&sections);
    Ok(out)
}

/// Читает файл. Повреждённые секции пропускаются с предупреждением;
/// без корневого документа файл открыть нельзя.
pub fn decode(bytes: &[u8]) -> DocResult<Opened> {
    if bytes.len() < 16 || &bytes[..8] != MAGIC {
        return Err(DocError::NotAFerrumFile);
    }
    let mut r = Reader::at(bytes, 8);
    let version = r.u32().ok_or(DocError::NotAFerrumFile)?;
    if version > FORMAT_VERSION {
        return Err(DocError::UnsupportedFormat(version));
    }
    let count = r.u32().ok_or(DocError::NotAFerrumFile)?;

    let mut warnings = Vec::new();
    let mut root = None;
    let mut snapshots: HashMap<PageId, Vec<u8>> = HashMap::new();
    let mut refs: HashMap<PageId, Vec<String>> = HashMap::new();
    let mut scenes: HashMap<PageId, &[u8]> = HashMap::new();
    let mut images = ImageStore::default();
    let mut bad_images = 0;

    for _ in 0..count {
        let header = (|| {
            let kind = r.u8()?;
            let id_len = r.u32()? as usize;
            let id = String::from_utf8(r.take(id_len)?.to_vec()).ok()?;
            let len = usize::try_from(r.u64()?).ok()?;
            let checksum: [u8; 32] = r.take(32)?.try_into().ok()?;
            let data = r.take(len)?;
            Some((kind, id, checksum, data))
        })();
        let Some((kind, id, checksum, data)) = header else {
            warnings.push("Файл обрезан: часть данных в конце не прочитана".to_string());
            break;
        };
        let intact = blake3::hash(data).as_bytes() == &checksum;
        match kind {
            ROOT if intact => root = Some(data.to_vec()),
            ROOT => {
                return Err(DocError::Corrupted(
                    "не удалось прочитать структуру документа".to_string(),
                ))
            }
            PAGE | PAGE_REFS | SCENE => {
                // Повреждённая страница станет пустой — предупредит Document;
                // повреждённый кэш просто не используется.
                let (Ok(page), true) = (id.parse::<PageId>(), intact) else {
                    continue;
                };
                match kind {
                    PAGE => {
                        snapshots.insert(page, data.to_vec());
                    }
                    PAGE_REFS => {
                        let list = String::from_utf8_lossy(data);
                        refs.insert(page, list.lines().map(str::to_string).collect());
                    }
                    _ => {
                        scenes.insert(page, data);
                    }
                }
            }
            // Контрольная сумма секции ловит порчу; хэш содержимого — подмену id.
            IMAGE if intact && blake3::hash(data).to_hex().as_str() == id => {
                images.insert_verified(id, data.to_vec())
            }
            IMAGE => bad_images += 1,
            _ => {} // секции из будущих версий формата пропускаем
        }
    }
    let root = root.ok_or_else(|| {
        DocError::Corrupted("не удалось прочитать структуру документа".to_string())
    })?;
    let mut unloaded = HashMap::new();
    for (id, snapshot) in snapshots {
        // Кэш годен, только если построен именно из этого снапшота и в понятной
        // этой сборке версии таблицы.
        let scene = scenes
            .get(&id)
            .filter(|d| {
                d.len() > 32
                    && d[..32] == *blake3::hash(&snapshot).as_bytes()
                    && d[32] == SCENE_CACHE_VERSION
            })
            .and_then(|d| scene_cache::decode(&d[33..]));
        let image_refs = match refs.remove(&id) {
            Some(list) => list,
            None => {
                // Без списка при сохранении пропали бы картинки страницы — пересчитываем из снапшота.
                warnings.push(format!(
                    "Список изображений страницы {id} повреждён и восстановлен из её содержимого"
                ));
                PageDoc::from_snapshot(&snapshot)
                    .and_then(|p| p.image_refs())
                    .unwrap_or_default()
            }
        };
        unloaded.insert(
            id,
            UnloadedPage {
                snapshot,
                image_refs,
                scene,
            },
        );
    }
    // Испорченные секции картинок: сколько картинок, на которые ссылаются страницы,
    // из-за них потеряно (документ открывается без них — на их месте заглушка).
    // Ссылки без картинки в целом файле — не потеря: её не было и в документе.
    if bad_images > 0 {
        let missing: HashSet<&str> = unloaded
            .values()
            .flat_map(|u: &UnloadedPage| u.image_refs.iter().map(String::as_str))
            .filter(|h| images.get(h).is_none())
            .collect();
        if !missing.is_empty() {
            warnings.push(format!(
                "Файл повреждён: не удалось прочитать секцию изображений. Документ открыт без {}",
                pictures(missing.len())
            ));
        }
    }
    let (document, mut doc_warnings) = Document::from_parts(&root, unloaded, images)?;
    warnings.append(&mut doc_warnings);
    Ok(Opened { document, warnings })
}

/// «1 картинки», «2 картинок», «21 картинки» — число после «без».
fn pictures(n: usize) -> String {
    let word = if n % 10 == 1 && n % 100 != 11 {
        "картинки"
    } else {
        "картинок"
    };
    format!("{n} {word}")
}

/// Помощники для тестов других модулей и крейтов. Не для продакшен-кода.
#[doc(hidden)]
pub mod testing {
    /// Переписывает версию схемы в файле — чтобы проверить открытие файлов из будущего.
    pub fn with_schema_version(bytes: &[u8], version: u32) -> Vec<u8> {
        let mut doc = super::decode(bytes).expect("валидный файл").document;
        doc.force_schema_version(version);
        super::encode(&doc).expect("кодирование")
    }

    /// Смещение данных первой секции типа `kind` — чтобы портить их в тестах.
    pub fn section_data_offset(bytes: &[u8], kind: u8) -> Option<usize> {
        let mut r = crate::binary::Reader::at(bytes, 12);
        let count = r.u32()?;
        let mut pos = 16usize;
        for _ in 0..count {
            let mut r = crate::binary::Reader::at(bytes, pos);
            let k = r.u8()?;
            let id_len = usize::try_from(r.u32()?).ok()?;
            r.take(id_len)?;
            let len = usize::try_from(r.u64()?).ok()?;
            r.take(32)?;
            // тип 1 + длина id 4 + id + длина данных 8 + checksum 32
            let data_at = pos.checked_add(45)?.checked_add(id_len)?;
            if k == kind {
                return Some(data_at);
            }
            pos = data_at.checked_add(len)?;
        }
        None
    }

    /// Дописывает в конец файла секцию произвольного типа — как файл из будущей версии.
    pub fn with_extra_section(bytes: &[u8], kind: u8, id: &str, data: &[u8]) -> Vec<u8> {
        let count = crate::binary::Reader::at(bytes, 12)
            .u32()
            .expect("валидный файл");
        let mut out = bytes.to_vec();
        out[12..16].copy_from_slice(&(count + 1).to_le_bytes());
        super::push_section(&mut out, kind, id, data);
        out
    }

    /// Перестраивает файл: `f` меняет тип, id и данные каждой секции,
    /// контрольные суммы пересчитываются — так портятся данные «тихо» для проверки.
    pub fn map_sections(bytes: &[u8], mut f: impl FnMut(u8, &mut String, &mut Vec<u8>)) -> Vec<u8> {
        let mut r = crate::binary::Reader::at(bytes, 12);
        let count = r.u32().expect("валидный файл");
        let mut r = crate::binary::Reader::at(bytes, 16);
        let mut out = bytes[..16].to_vec();
        for _ in 0..count {
            let kind = r.u8().expect("валидный файл");
            let id_len = usize::try_from(r.u32().expect("валидный файл")).expect("длина");
            let mut id =
                String::from_utf8(r.take(id_len).expect("валидный файл").to_vec()).expect("UTF-8");
            let len = usize::try_from(r.u64().expect("валидный файл")).expect("длина");
            r.take(32).expect("валидный файл");
            let mut data = r.take(len).expect("валидный файл").to_vec();
            f(kind, &mut id, &mut data);
            super::push_section(&mut out, kind, &id, &data);
        }
        out
    }
}
