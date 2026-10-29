//! Текстовый движок (спека подсистемы 3, разделы 4–5): реестр шрифтов (вшитые,
//! системные, отсутствующие с запасным Inter), раскладка текстового узла через
//! `parley` с собственной коллекцией `fontique` без системного поиска и контуры
//! глифов из `skrifa`. О Loro и рендере не знает: получает `TextData` узла,
//! отдаёт строки и глифы.

pub mod style_name;

pub use ferrum_fontmeta::{group, read_faces, FaceInfo, FamilyInfo, MAX_FONT_FILE};

/// Onest (SIL OFL 1.1, `assets/fonts/OFL.txt`) вшит в wasm: им набраны подписи
/// оверлеев (`ferrum-render`), и в реестре он — вшитое семейство, готовое без
/// загрузки.
pub const ONEST: &[u8] = include_bytes!("../assets/fonts/Onest-wght.ttf");
