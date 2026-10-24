//! Пресеты размеров фреймов. Список — отдельный JSON (`assets/frame_presets.json`),
//! вшитый в крейт: он дополняется без изменения кода.

use std::sync::OnceLock;

use serde::Deserialize;

/// Наибольшая сторона пресета: больше не бывает ни у экранов, ни у бумаги.
pub const MAX_PRESET_SIDE: f64 = 100_000.0;

const BUILTIN: &str = include_str!("../assets/frame_presets.json");

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FramePreset {
    /// Раздел списка в интерфейсе («Телефоны», «Планшеты»…).
    pub group: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Deserialize)]
struct PresetFile {
    version: u32,
    presets: Vec<FramePreset>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PresetError {
    #[error("Не удалось прочитать список пресетов: {0}")]
    Parse(String),
    #[error("Список пресетов из более новой версии Ferrum ({0})")]
    NewerVersion(u32),
    #[error("Пресет «{0}»: размеры должны быть от 1 до 100000")]
    BadSize(String),
}

/// Разбирает и проверяет список пресетов.
pub fn parse_presets(json: &str) -> Result<Vec<FramePreset>, PresetError> {
    let file: PresetFile =
        serde_json::from_str(json).map_err(|e| PresetError::Parse(e.to_string()))?;
    if file.version > 1 {
        return Err(PresetError::NewerVersion(file.version));
    }
    for p in &file.presets {
        let ok = |v: f64| v.is_finite() && (1.0..=MAX_PRESET_SIDE).contains(&v);
        if !ok(p.width) || !ok(p.height) {
            return Err(PresetError::BadSize(p.name.clone()));
        }
    }
    Ok(file.presets)
}

/// Встроенный список (раздел 7.5 спеки).
pub fn frame_presets() -> &'static [FramePreset] {
    static LIST: OnceLock<Vec<FramePreset>> = OnceLock::new();
    LIST.get_or_init(|| parse_presets(BUILTIN).expect("встроенный список пресетов корректен"))
}
