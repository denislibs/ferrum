//! Библиотека файлов десктопа: записи, их файлы на диске, превью и журнал.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::atomic::atomic_write;
use crate::{
    CONFLICT, DISK_CHANGED, DISK_MISSING, GONE, MAX_JOURNAL_BATCHES, MAX_NAME_CHARS,
    MAX_OPEN_BYTES, MAX_THUMB_BYTES, MAX_WINDOW, NOT_FOUND, UNTITLED,
};

/// Неверный запрос к хранилищу (не доверенный ввод: id, имена, номера).
const BAD: &str = "Неверный запрос к хранилищу файлов";

/// Как `tooLarge` интерфейса (`core/limits.ts`): мегабайты с округлением вверх.
fn too_large(limit: u64, actual: Option<u64>) -> String {
    const MIB: u64 = 1024 * 1024;
    match actual {
        None => format!(
            "Слишком большой объём данных: предел — {} МБ",
            limit.div_ceil(MIB)
        ),
        Some(a) => format!(
            "Слишком большой объём данных: {} МБ, предел — {} МБ",
            a.div_ceil(MIB),
            limit.div_ceil(MIB)
        ),
    }
}

/// Номера сеанса и пачки — целые не больше `Number.MAX_SAFE_INTEGER` (как в интерфейсе).
const MAX_SAFE: u64 = (1 << 53) - 1;

/// Байты файла и сеанс журнала: `session` — наибольший сеанс среди всех пачек файла
/// (`None` — пачек нет), как поле `session` ответа `read` веб-хранилища.
#[derive(Debug, Clone, PartialEq)]
pub struct FileRead {
    pub bytes: Option<Vec<u8>>,
    pub session: Option<u64>,
}

/// Метаданные записи — как `FileMeta` интерфейса (`core/file-list.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMeta {
    pub id: String,
    pub name: String,
    pub created_at: f64,
    pub edited_at: f64,
    pub opened_at: f64,
    pub size: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trashed_at: Option<f64>,
    pub thumb_version: f64,
    pub version: f64,
    /// Имя файла на диске.
    pub disk: String,
}

/// Запись библиотеки: метаданные и где лежит файл.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    #[serde(flatten)]
    meta: FileMeta,
    path: PathBuf,
    /// Время изменения файла после нашей записи или чтения, мс: другое — файл
    /// записала другая программа.
    disk_modified: f64,
    /// Файл создан Ferrum в его папке: удаление навсегда удаляет и файл.
    owned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Section {
    Recent,
    Drafts,
    Trash,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ListResult {
    pub total: usize,
    pub rows: Vec<FileMeta>,
}

pub struct Library {
    root: PathBuf,
    docs: PathBuf,
    entries: HashMap<String, Entry>,
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

fn mtime(path: &Path) -> io::Result<f64> {
    Ok(fs::metadata(path)?
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64))
}

/// Имя записи: без управляющих символов, обрезано; пустое — «Без названия».
pub fn clean_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    if s.is_empty() {
        UNTITLED.to_string()
    } else {
        s
    }
}

/// Имя файла на диске из имени записи — как `fileName` интерфейса.
fn file_stem(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let clean = clean.trim_matches(|c: char| c == '.' || c.is_whitespace());
    if clean.is_empty() {
        UNTITLED.to_string()
    } else {
        clean.to_string()
    }
}

/// id записи — UUID (интерфейс не доверенный: id идёт в пути превью и журнала).
fn valid_id(id: &str) -> bool {
    id.len() == 36 && uuid::Uuid::parse_str(id).is_ok()
}

/// Имя пачки журнала: `<сеанс>-<пачка>.bin` с 20 цифрами в каждом номере.
fn journal_name(session: u64, batch: u64) -> String {
    format!("{session:020}-{batch:020}.bin")
}

fn parse_journal_name(name: &str) -> Option<(u64, u64)> {
    let stem = name.strip_suffix(".bin")?;
    let (s, b) = stem.split_once('-')?;
    let digits = |x: &str| x.len() == 20 && x.bytes().all(|c| c.is_ascii_digit());
    if !digits(s) || !digits(b) {
        return None;
    }
    Some((s.parse().ok()?, b.parse().ok()?))
}

impl Library {
    /// Библиотека в каталоге данных `root`; новые файлы — в `docs`. Испорченный список
    /// не мешает запуску: он откладывается (`library.json.bad`), список начинается заново.
    pub fn open(root: &Path, docs: &Path) -> io::Result<(Library, Vec<String>)> {
        fs::create_dir_all(root.join("thumbs"))?;
        fs::create_dir_all(root.join("journal"))?;
        let mut warnings = Vec::new();
        let index = root.join("library.json");
        let mut entries = HashMap::new();
        match fs::read(&index) {
            Ok(bytes) => match serde_json::from_slice::<Vec<Entry>>(&bytes) {
                Ok(list) => {
                    for e in list {
                        if valid_id(&e.meta.id) {
                            entries.insert(e.meta.id.clone(), e);
                        }
                    }
                }
                Err(_) => {
                    let _ = fs::rename(&index, root.join("library.json.bad"));
                    warnings.push(
                        "Список файлов повреждён — он начат заново (прежний сохранён как library.json.bad)"
                            .to_string(),
                    );
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        Ok((
            Library {
                root: root.to_path_buf(),
                docs: docs.to_path_buf(),
                entries,
            },
            warnings,
        ))
    }

    fn save_index(&self) -> Result<(), String> {
        let mut list: Vec<&Entry> = self.entries.values().collect();
        list.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
        let json = serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?;
        atomic_write(&self.root.join("library.json"), &json).map_err(|e| e.to_string())
    }

    fn entry(&self, id: &str) -> Result<&Entry, String> {
        // Не-UUID не бывает ключом: путь превью и журнала из чужого id не строится.
        self.entries
            .get(id)
            .filter(|_| valid_id(id))
            .ok_or_else(|| NOT_FOUND.to_string())
    }

    fn entry_mut(&mut self, id: &str) -> Result<&mut Entry, String> {
        self.entries
            .get_mut(id)
            .ok_or_else(|| NOT_FOUND.to_string())
    }

    /// Окно раздела (не больше `MAX_WINDOW` строк) — как `sectionList` интерфейса.
    pub fn list(&self, section: Section, query: &str, start: usize, count: usize) -> ListResult {
        let q = query.trim().to_lowercase();
        let key = |m: &FileMeta| match section {
            Section::Recent => m.opened_at,
            Section::Drafts => m.edited_at,
            Section::Trash => m.trashed_at.unwrap_or(0.0),
        };
        let mut rows: Vec<&FileMeta> = self
            .entries
            .values()
            .map(|e| &e.meta)
            .filter(|m| (section == Section::Trash) == m.trashed_at.is_some())
            .filter(|m| section != Section::Recent || m.opened_at > 0.0)
            .filter(|m| q.is_empty() || m.name.to_lowercase().contains(&q))
            .collect();
        rows.sort_by(|a, b| {
            key(b)
                .total_cmp(&key(a))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.id.cmp(&b.id))
        });
        let total = rows.len();
        let rows = rows
            .into_iter()
            .skip(start)
            .take(count.min(MAX_WINDOW))
            .cloned()
            .collect();
        ListResult { total, rows }
    }

    pub fn get(&self, id: &str) -> Option<FileMeta> {
        self.entries.get(id).map(|e| e.meta.clone())
    }

    /// Свободное имя файла в папке документов: занятое на диске или записью — «Имя 2».
    fn free_path(&self, name: &str) -> PathBuf {
        let stem = file_stem(name);
        let taken = |p: &Path| p.exists() || self.entries.values().any(|e| e.path == p);
        let mut path = self.docs.join(format!("{stem}.ferrum"));
        let mut n = 2;
        while taken(&path) {
            path = self.docs.join(format!("{stem} {n}.ferrum"));
            n += 1;
        }
        path
    }

    /// Новая запись: файл появится в папке документов при первой записи.
    pub fn create(&mut self, name: &str) -> Result<FileMeta, String> {
        fs::create_dir_all(&self.docs).map_err(|e| e.to_string())?;
        let name = clean_name(name);
        let path = self.free_path(&name);
        let now = now_ms();
        let meta = FileMeta {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            created_at: now,
            edited_at: now,
            opened_at: now,
            size: 0.0,
            trashed_at: None,
            thumb_version: 0.0,
            version: 0.0,
            disk: path
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
        };
        self.entries.insert(
            meta.id.clone(),
            Entry {
                meta: meta.clone(),
                path,
                disk_modified: 0.0,
                owned: true,
            },
        );
        self.save_index()?;
        Ok(meta)
    }

    /// Запись для файла, выбранного в диалоге «Открыть…» (тот же файл — прежняя запись).
    pub fn add_existing(&mut self, path: &Path) -> Result<FileMeta, String> {
        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
        if let Some(e) = self.entries.values().find(|e| e.path == path) {
            return Ok(e.meta.clone());
        }
        let md = fs::metadata(&path).map_err(|e| e.to_string())?;
        if md.len() > MAX_OPEN_BYTES {
            return Err(too_large(MAX_OPEN_BYTES, Some(md.len())));
        }
        let now = now_ms();
        let stem = path
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().into_owned());
        let meta = FileMeta {
            id: uuid::Uuid::new_v4().to_string(),
            name: clean_name(&stem),
            created_at: now,
            edited_at: mtime(&path).unwrap_or(now),
            opened_at: now,
            size: md.len() as f64,
            trashed_at: None,
            thumb_version: 0.0,
            version: if md.len() > 0 { 1.0 } else { 0.0 },
            disk: path
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
        };
        self.entries.insert(
            meta.id.clone(),
            Entry {
                meta: meta.clone(),
                disk_modified: mtime(&path).unwrap_or(0.0),
                path,
                owned: false,
            },
        );
        self.save_index()?;
        Ok(meta)
    }

    /// Байты файла и наибольший сеанс журнала одним вызовом (как `read` веб-хранилища).
    pub fn read_file(&mut self, id: &str) -> Result<FileRead, String> {
        let bytes = self.read(id)?;
        let session = self.journal_session(id)?;
        Ok(FileRead { bytes, session })
    }

    /// Байты файла; `None` — новый файл, ещё не записанный.
    pub fn read(&mut self, id: &str) -> Result<Option<Vec<u8>>, String> {
        let e = self.entry(id)?;
        if !e.path.exists() {
            return if e.meta.size == 0.0 {
                Ok(None)
            } else {
                Err(DISK_MISSING.to_string())
            };
        }
        let len = fs::metadata(&e.path).map_err(|e| e.to_string())?.len();
        if len > MAX_OPEN_BYTES {
            return Err(too_large(MAX_OPEN_BYTES, Some(len)));
        }
        let bytes = fs::read(&e.path).map_err(|e| e.to_string())?;
        let modified = mtime(&e.path).map_err(|e| e.to_string())?;
        let e = self.entry_mut(id)?;
        if e.disk_modified != modified {
            e.disk_modified = modified;
            self.save_index()?;
        }
        Ok((!bytes.is_empty()).then_some(bytes))
    }

    /// Пишет файл атомарно. `base` — поколение, на котором основана запись; файл,
    /// записанный с тех пор другим окном или другой программой, — отказ. `journal` —
    /// снимок вобрал пачки сеанса до номера включительно и все прежних сеансов.
    pub fn write(
        &mut self,
        id: &str,
        bytes: &[u8],
        base: f64,
        journal: Option<(u64, u64)>,
    ) -> Result<FileMeta, String> {
        if bytes.len() as u64 > MAX_OPEN_BYTES {
            return Err(too_large(MAX_OPEN_BYTES, Some(bytes.len() as u64)));
        }
        if let Some((session, up_to)) = journal {
            if session > MAX_SAFE || up_to > MAX_SAFE {
                return Err(BAD.to_string());
            }
        }
        let e = self
            .entries
            .get(id)
            .filter(|_| valid_id(id))
            .ok_or_else(|| GONE.to_string())?;
        if e.meta.version != base {
            return Err(CONFLICT.to_string());
        }
        if e.path.exists() && e.meta.size > 0.0 {
            let now = mtime(&e.path).map_err(|e| e.to_string())?;
            if now != e.disk_modified {
                return Err(DISK_CHANGED.to_string());
            }
        }
        if let Some(dir) = e.path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        atomic_write(&e.path, bytes).map_err(|e| e.to_string())?;
        let modified = mtime(&e.path).map_err(|e| e.to_string())?;
        let e = self.entry_mut(id)?;
        e.disk_modified = modified;
        e.meta.size = bytes.len() as f64;
        e.meta.edited_at = now_ms();
        e.meta.version += 1.0;
        let meta = e.meta.clone();
        if let Some((session, up_to)) = journal {
            self.drop_journal(id, |s, b| s < session || (s == session && b <= up_to));
        }
        self.save_index()?;
        Ok(meta)
    }

    /// Превью: PNG или `None` — убрать.
    pub fn write_thumb(&mut self, id: &str, png: Option<&[u8]>) -> Result<FileMeta, String> {
        self.entry(id)?;
        if png.is_some_and(|b| b.len() as u64 > MAX_THUMB_BYTES) {
            return Err(too_large(MAX_THUMB_BYTES, png.map(|b| b.len() as u64)));
        }
        let path = self.root.join("thumbs").join(format!("{id}.png"));
        match png {
            Some(b) => atomic_write(&path, b).map_err(|e| e.to_string())?,
            None => {
                let _ = fs::remove_file(&path);
            }
        }
        let e = self.entry_mut(id)?;
        e.meta.thumb_version += 1.0;
        let meta = e.meta.clone();
        self.save_index()?;
        Ok(meta)
    }

    pub fn thumb(&self, id: &str) -> Option<Vec<u8>> {
        let e = self.entries.get(id)?;
        if e.meta.thumb_version == 0.0 {
            return None;
        }
        fs::read(self.root.join("thumbs").join(format!("{id}.png"))).ok()
    }

    fn journal_dir(&self, id: &str) -> PathBuf {
        self.root.join("journal").join(id)
    }

    /// Пачка журнала изменений (атомарно, своим файлом).
    pub fn journal_append(
        &mut self,
        id: &str,
        session: u64,
        batch: u64,
        bytes: &[u8],
    ) -> Result<(), String> {
        self.entry(id)?;
        if session > MAX_SAFE || batch > MAX_SAFE {
            return Err(BAD.to_string());
        }
        if bytes.len() as u64 > MAX_OPEN_BYTES {
            return Err(too_large(MAX_OPEN_BYTES, Some(bytes.len() as u64)));
        }
        let dir = self.journal_dir(id);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        atomic_write(&dir.join(journal_name(session, batch)), bytes).map_err(|e| e.to_string())
    }

    /// Пачки журнала на диске: `(сеанс, имя, размер)` по порядку записи. Больше
    /// `MAX_JOURNAL_BATCHES` пачек — отказ, не чтение всего каталога в память.
    fn journal_files(&self, id: &str) -> Result<Vec<(u64, String, u64)>, String> {
        let mut out = Vec::new();
        let Ok(dir) = fs::read_dir(self.journal_dir(id)) else {
            return Ok(out);
        };
        for d in dir.filter_map(|d| d.ok()) {
            let name = d.file_name().to_string_lossy().into_owned();
            if let Some((s, _)) = parse_journal_name(&name) {
                if out.len() >= MAX_JOURNAL_BATCHES {
                    return Err(format!(
                        "Неверная команда: в журнале изменений больше {MAX_JOURNAL_BATCHES} записей"
                    ));
                }
                let len = d.metadata().map_or(0, |m| m.len());
                out.push((s, name, len));
            }
        }
        out.sort_by(|a, b| a.1.cmp(&b.1));
        Ok(out)
    }

    /// Наибольший сеанс среди всех пачек файла (`None` — пачек нет); не зависит от того,
    /// есть ли байты файла.
    pub fn journal_session(&self, id: &str) -> Result<Option<u64>, String> {
        self.entry(id)?;
        Ok(self.journal_files(id)?.iter().map(|f| f.0).max())
    }

    /// Имена пачек журнала по порядку записи. Без байтов файла журнала нет: его пачки
    /// начаты на снимке, которого нет. Пачек не больше `MAX_JOURNAL_BATCHES`, а файл
    /// вместе с журналом — не больше `MAX_OPEN_BYTES` (иначе отказ).
    pub fn journal_names(&self, id: &str) -> Result<Vec<String>, String> {
        let e = self.entry(id)?;
        if !e.path.exists() {
            return Ok(Vec::new());
        }
        let files = self.journal_files(id)?;
        let mut total = fs::metadata(&e.path).map_err(|e| e.to_string())?.len();
        for f in &files {
            total = total.saturating_add(f.2);
        }
        if total > MAX_OPEN_BYTES {
            return Err(too_large(MAX_OPEN_BYTES, Some(total)));
        }
        Ok(files.into_iter().map(|f| f.1).collect())
    }

    /// Пачка журнала по имени (только имена своего вида: не путь).
    pub fn journal_read(&self, id: &str, name: &str) -> Result<Vec<u8>, String> {
        self.entry(id)?;
        if parse_journal_name(name).is_none() {
            return Err(BAD.into());
        }
        let path = self.journal_dir(id).join(name);
        let len = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if len > MAX_OPEN_BYTES {
            return Err(too_large(MAX_OPEN_BYTES, Some(len)));
        }
        fs::read(path).map_err(|e| e.to_string())
    }

    fn drop_journal(&self, id: &str, gone: impl Fn(u64, u64) -> bool) {
        if let Ok(dir) = fs::read_dir(self.journal_dir(id)) {
            for d in dir.filter_map(|d| d.ok()) {
                let name = d.file_name().to_string_lossy().into_owned();
                if parse_journal_name(&name).is_some_and(|(s, b)| gone(s, b)) {
                    let _ = fs::remove_file(d.path());
                }
            }
        }
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<FileMeta, String> {
        let e = self.entry_mut(id)?;
        let name = clean_name(name);
        if e.meta.name != name {
            e.meta.name = name;
            e.meta.edited_at = now_ms();
        }
        let meta = e.meta.clone();
        self.save_index()?;
        Ok(meta)
    }

    pub fn touch(&mut self, id: &str) -> Result<FileMeta, String> {
        let e = self.entry_mut(id)?;
        e.meta.opened_at = now_ms();
        let meta = e.meta.clone();
        self.save_index()?;
        Ok(meta)
    }

    pub fn trash(&mut self, id: &str, on: bool) -> Result<(), String> {
        let e = self.entry_mut(id)?;
        e.meta.trashed_at = on.then(now_ms);
        self.save_index()
    }

    /// Удалить навсегда: запись, превью и журнал; файл — только созданный Ferrum
    /// (открытый с диска остаётся на месте).
    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let e = self
            .entries
            .remove(id)
            .ok_or_else(|| NOT_FOUND.to_string())?;
        if e.owned {
            let _ = fs::remove_file(&e.path);
        }
        let _ = fs::remove_file(self.root.join("thumbs").join(format!("{id}.png")));
        let _ = fs::remove_dir_all(self.journal_dir(id));
        self.save_index()
    }

    pub fn empty_trash(&mut self) -> Result<(), String> {
        let ids: Vec<String> = self
            .entries
            .values()
            .filter(|e| e.meta.trashed_at.is_some())
            .map(|e| e.meta.id.clone())
            .collect();
        for id in ids {
            self.delete(&id)?;
        }
        Ok(())
    }

    /// «Сохранить на диск…»: файл записи переезжает по пути из диалога; дальше
    /// автосохранение пишет туда. Прежний файл в папке Ferrum удаляется.
    pub fn move_to(&mut self, id: &str, path: &Path) -> Result<FileMeta, String> {
        let e = self.entry(id)?.clone();
        if path != e.path && e.path.exists() {
            let bytes = fs::read(&e.path).map_err(|e| e.to_string())?;
            atomic_write(path, &bytes).map_err(|e| e.to_string())?;
            if e.owned {
                let _ = fs::remove_file(&e.path);
            }
        }
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let modified = mtime(&path).unwrap_or(0.0);
        let entry = self.entry_mut(id)?;
        entry.meta.disk = path
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        entry.path = path;
        entry.owned = false;
        entry.disk_modified = modified;
        let meta = entry.meta.clone();
        self.save_index()?;
        Ok(meta)
    }

    /// «Сохранить копию на диск…»: копия файла по пути из диалога, запись не меняется.
    pub fn copy_to(&self, id: &str, path: &Path) -> Result<(), String> {
        let e = self.entry(id)?;
        let bytes = fs::read(&e.path).map_err(|_| DISK_MISSING.to_string())?;
        atomic_write(path, &bytes).map_err(|e| e.to_string())
    }
}
