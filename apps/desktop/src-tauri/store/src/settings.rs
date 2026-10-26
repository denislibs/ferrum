//! Настройки пользователя десктопа — файл конфигурации (спека 7.6) `settings.json`:
//! объект полей, как хранилище `settings` веб-версии. Запись — только переданные поля
//! (поверх прочитанного файла) и атомарно; значения проверяет интерфейс при чтении.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::atomic::atomic_write;
use crate::io_error;

/// Поля настроек — те же, что `SETTINGS_FIELDS` интерфейса.
pub const SETTINGS_FIELDS: [&str; 7] = [
    "theme",
    "pixelSnap",
    "keymap",
    "leftWidth",
    "rightWidth",
    "pagesOpen",
    "filesView",
];
/// Запись настроек не больше этого (как `MAX_SETTINGS_BYTES`).
pub const MAX_SETTINGS_BYTES: usize = 80 * 1024;

pub struct Settings {
    path: PathBuf,
    values: Map<String, Value>,
}

impl Settings {
    /// Испорченный файл — пустые настройки (умолчания), он перезапишется при правке.
    pub fn open(path: &Path) -> io::Result<Settings> {
        let values = match fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default()
                .into_iter()
                // Только известные поля с простыми значениями: чужое не раздувает файл.
                .filter(|(k, v)| {
                    SETTINGS_FIELDS.contains(&k.as_str())
                        && (v.is_string() || v.is_number() || v.is_boolean())
                })
                .collect(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Map::new(),
            Err(e) => return Err(e),
        };
        Ok(Settings {
            path: path.to_path_buf(),
            values,
        })
    }

    pub fn all(&self) -> Map<String, Value> {
        self.values.clone()
    }

    /// Записать поля: известные, простые значения, вместе — не больше предела.
    pub fn set(&mut self, patch: Map<String, Value>) -> Result<(), String> {
        let bad = || "Неверный запрос к хранилищу настроек".to_string();
        if patch.is_empty() {
            return Err(bad());
        }
        for (k, v) in &patch {
            if !SETTINGS_FIELDS.contains(&k.as_str())
                || !(v.is_string() || v.is_number() || v.is_boolean())
            {
                return Err(bad());
            }
        }
        let mut next = self.values.clone();
        next.extend(patch);
        let json = serde_json::to_vec_pretty(&next)
            .map_err(|_| "не удалось подготовить настройки".to_string())?;
        if json.len() > MAX_SETTINGS_BYTES {
            return Err("Слишком большой объём данных: настройки больше 80 КБ".into());
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| io_error(&e))?;
        }
        atomic_write(&self.path, &json).map_err(|e| io_error(&e))?;
        self.values = next;
        Ok(())
    }
}
