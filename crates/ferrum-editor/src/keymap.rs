//! Раскладка горячих клавиш: сочетания по умолчанию из таблицы команд плюс
//! пользовательские отличия. Хранятся только отличия — при обновлении Ferrum новые
//! сочетания по умолчанию доходят до пользователя сами.
//!
//! Сочетания сопоставляются по физической клавише (`KeyboardEvent.code`), поэтому
//! буквы и цифры работают на любой раскладке, в том числе на русской: V и М — одна
//! клавиша `KeyV`.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use serde_json::{json, Value};

use crate::commands::{CommandId, COMMANDS};
use crate::input::{KeyEvent, Modifiers, Platform};

/// Предел размера импортируемого JSON и числа сочетаний у одной команды: файл
/// не доверенный.
pub const MAX_KEYMAP_JSON: usize = 64 * 1024;
pub const MAX_SHORTCUTS_PER_COMMAND: usize = 8;
const FORMAT: &str = "ferrum-keymap";
const VERSION: u64 = 1;

/// Сочетание: физическая клавиша и точный набор модификаторов.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Shortcut {
    pub code: String,
    pub mods: Mods,
}

/// Модификаторы сочетания (упорядочиваемые — для детерминированных списков).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl From<Modifiers> for Mods {
    fn from(m: Modifiers) -> Mods {
        Mods {
            ctrl: m.ctrl,
            alt: m.alt,
            shift: m.shift,
            meta: m.meta,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeymapError {
    #[error("Не удалось разобрать сочетание «{0}»")]
    BadShortcut(String),
    #[error("Файл раскладки слишком большой (больше 64 КБ)")]
    TooLarge,
    #[error("Это не файл раскладки Ferrum: {0}")]
    BadFormat(String),
    #[error("Раскладка из более новой версии Ferrum ({0}). Обновите Ferrum")]
    NewerVersion(u64),
}

/// Сочетание уже занято другими командами.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub shortcut: Shortcut,
    pub commands: Vec<CommandId>,
}

/// Почему `bind` ничего не изменил.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BindError {
    /// Сочетание занято другими командами, а замена не разрешена: интерфейс
    /// предлагает заменить.
    #[error("Сочетание {} уже занято", .0.shortcut)]
    Conflict(Conflict),
    /// У команды уже `MAX_SHORTCUTS_PER_COMMAND` сочетаний.
    #[error("У команды уже {MAX_SHORTCUTS_PER_COMMAND} сочетаний — сначала уберите одно")]
    TooMany,
}

impl BindError {
    /// Конфликт, если ошибка — занятое сочетание.
    pub fn conflict(&self) -> Option<&Conflict> {
        match self {
            BindError::Conflict(c) => Some(c),
            BindError::TooMany => None,
        }
    }
}

/// Допустимые физические клавиши: буквы, цифры, F1–F24, навигация и знаки.
fn valid_code(code: &str) -> bool {
    const NAMED: &[&str] = &[
        "Space",
        "Enter",
        "Escape",
        "Tab",
        "Backspace",
        "Delete",
        "Insert",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "ArrowDown",
        "Minus",
        "Equal",
        "BracketLeft",
        "BracketRight",
        "Backslash",
        "Semicolon",
        "Quote",
        "Comma",
        "Period",
        "Slash",
        "Backquote",
        "IntlBackslash",
        "NumpadAdd",
        "NumpadSubtract",
        "NumpadMultiply",
        "NumpadDivide",
        "NumpadDecimal",
        "NumpadEnter",
    ];
    let one_of = |prefix: &str, ok: fn(&str) -> bool| code.strip_prefix(prefix).is_some_and(ok);
    NAMED.contains(&code)
        || one_of("Key", |r| {
            r.len() == 1 && r.as_bytes()[0].is_ascii_uppercase()
        })
        || one_of("Digit", |r| {
            r.len() == 1 && r.as_bytes()[0].is_ascii_digit()
        })
        || one_of("Numpad", |r| {
            r.len() == 1 && r.as_bytes()[0].is_ascii_digit()
        })
        || one_of("F", |r| {
            r.parse::<u8>().is_ok_and(|n| (1..=24).contains(&n))
        })
}

impl Shortcut {
    pub fn new(code: &str, mods: Modifiers) -> Shortcut {
        Shortcut {
            code: code.to_string(),
            mods: mods.into(),
        }
    }

    /// Разбирает «Ctrl+Shift+KeyZ». Модификаторы — Ctrl, Alt, Shift, Meta (или Cmd),
    /// в любом порядке, без повторов; последней идёт физическая клавиша.
    pub fn parse(s: &str) -> Result<Shortcut, KeymapError> {
        let bad = || KeymapError::BadShortcut(s.chars().take(64).collect());
        let mut parts: Vec<&str> = s.split('+').collect();
        let code = parts.pop().filter(|c| valid_code(c)).ok_or_else(bad)?;
        let mut mods = Mods::default();
        for p in parts {
            let flag = match p {
                "Ctrl" => &mut mods.ctrl,
                "Alt" => &mut mods.alt,
                "Shift" => &mut mods.shift,
                "Meta" | "Cmd" => &mut mods.meta,
                _ => return Err(bad()),
            };
            if *flag {
                return Err(bad());
            }
            *flag = true;
        }
        Ok(Shortcut {
            code: code.to_string(),
            mods,
        })
    }

    /// Подпись для интерфейса на раскладке US: «⇧⌘K» на macOS, «Ctrl+Shift+K» на
    /// остальных. Подпись для текущей раскладки делает интерфейс по
    /// `navigator.keyboard.getLayoutMap()`.
    pub fn label(&self, platform: Platform) -> String {
        let key = key_label(&self.code);
        if platform == Platform::Mac {
            let mut s = String::new();
            for (on, sym) in [
                (self.mods.ctrl, "⌃"),
                (self.mods.alt, "⌥"),
                (self.mods.shift, "⇧"),
                (self.mods.meta, "⌘"),
            ] {
                if on {
                    s.push_str(sym);
                }
            }
            s + &key
        } else {
            let mut parts = Vec::new();
            for (on, name) in [
                (self.mods.ctrl, "Ctrl"),
                (self.mods.alt, "Alt"),
                (self.mods.shift, "Shift"),
                (self.mods.meta, "Win"),
            ] {
                if on {
                    parts.push(name.to_string());
                }
            }
            parts.push(key);
            parts.join("+")
        }
    }
}

fn key_label(code: &str) -> String {
    if let Some(r) = code
        .strip_prefix("Key")
        .or_else(|| code.strip_prefix("Digit"))
    {
        return r.to_string();
    }
    match code {
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" => "\\",
        "Minus" => "-",
        "Equal" => "=",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "Escape" => "Esc",
        other => other,
    }
    .to_string()
}

impl fmt::Display for Shortcut {
    /// Каноническая запись: модификаторы в порядке Ctrl, Alt, Shift, Meta.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.mods.ctrl, "Ctrl"),
            (self.mods.alt, "Alt"),
            (self.mods.shift, "Shift"),
            (self.mods.meta, "Meta"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.code)
    }
}

/// Сочетания, которые браузер перехватывает до страницы (закрыть вкладку, новое
/// окно и т. п.). В веб-версии интерфейс помечает их предупреждением; в Tauri они
/// доступны.
pub fn browser_reserved(platform: Platform) -> Vec<Shortcut> {
    let primary = match platform {
        Platform::Mac => "Meta",
        _ => "Ctrl",
    };
    let mut list: Vec<String> = [
        "KeyW",
        "KeyT",
        "KeyN",
        "Shift+KeyW",
        "Shift+KeyT",
        "Shift+KeyN",
        "Tab",
        "Shift+Tab",
        "PageUp",
        "PageDown",
    ]
    .iter()
    .map(|s| format!("{primary}+{s}"))
    .collect();
    if platform == Platform::Mac {
        // Выйти, скрыть приложение, свернуть окно; Ctrl+Tab и Ctrl+Shift+Tab
        // переключают вкладки браузера и на маке.
        list.extend(
            [
                "Meta+KeyQ",
                "Meta+KeyH",
                "Meta+KeyM",
                "Ctrl+Tab",
                "Ctrl+Shift+Tab",
            ]
            .map(String::from),
        );
    } else {
        list.extend(["Alt+F4", "F11"].map(String::from));
    }
    let mut out: Vec<Shortcut> = list
        .iter()
        .map(|s| Shortcut::parse(s).expect("встроенный список корректен"))
        .collect();
    out.sort();
    out
}

#[derive(Debug, Clone)]
pub struct Keymap {
    platform: Platform,
    defaults: BTreeMap<CommandId, Vec<Shortcut>>,
    /// Только команды, чьи сочетания отличаются от умолчаний (полный новый список).
    overrides: BTreeMap<CommandId, Vec<Shortcut>>,
    /// Сочетание → команды, которые оно сейчас вызывает (в порядке таблицы).
    index: HashMap<Shortcut, Vec<CommandId>>,
}

impl Keymap {
    pub fn new(platform: Platform) -> Keymap {
        let defaults = COMMANDS
            .iter()
            .map(|c| {
                let list =
                    c.id.default_shortcuts(platform)
                        .iter()
                        .map(|s| Shortcut::parse(s).expect("таблица команд корректна"))
                        .collect();
                (c.id, list)
            })
            .collect();
        let mut map = Keymap {
            platform,
            defaults,
            overrides: BTreeMap::new(),
            index: HashMap::new(),
        };
        map.reindex();
        map
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    fn reindex(&mut self) {
        let mut index: HashMap<Shortcut, Vec<CommandId>> = HashMap::new();
        for c in COMMANDS {
            for s in self.bindings(c.id) {
                index.entry(s.clone()).or_default().push(c.id);
            }
        }
        self.index = index;
    }

    /// Действующие сочетания команды.
    pub fn bindings(&self, id: CommandId) -> &[Shortcut] {
        self.overrides
            .get(&id)
            .or_else(|| self.defaults.get(&id))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn default_bindings(&self, id: CommandId) -> &[Shortcut] {
        self.defaults.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn is_overridden(&self, id: CommandId) -> bool {
        self.overrides.contains_key(&id)
    }

    /// Команда для нажатой клавиши. При конфликте — первая по таблице команд.
    ///
    /// С AltGr (`KeyEvent::alt_graph`) на Windows и Linux сочетания с Alt не
    /// срабатывают: там AltGr приходит как Ctrl+Alt (или как Alt), и набор символа
    /// (например, «@» на немецкой раскладке, «ę» — AltGr+Shift+E на польской)
    /// иначе вызывал бы команду (Mod+Alt+G, булевы Alt+Shift+U/S/I/E). На macOS AltGr нет, а
    /// Option некоторые браузеры отдают как AltGraph — правило убило бы ⌥⌘] и ⌥⌘[,
    /// поэтому там флаг не учитывается.
    pub fn lookup(&self, e: &KeyEvent) -> Option<CommandId> {
        let s = Shortcut {
            code: e.code.clone(),
            mods: e.modifiers.into(),
        };
        if e.alt_graph && self.platform != Platform::Mac && s.mods.alt {
            return None;
        }
        self.index.get(&s).and_then(|ids| ids.first().copied())
    }

    /// Команды, которые вызывает сочетание.
    pub fn commands_for(&self, s: &Shortcut) -> Vec<CommandId> {
        self.index.get(s).cloned().unwrap_or_default()
    }

    /// Все сочетания, занятые больше чем одной командой.
    pub fn conflicts(&self) -> Vec<Conflict> {
        let mut out: Vec<Conflict> = self
            .index
            .iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(s, ids)| Conflict {
                shortcut: s.clone(),
                commands: ids.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.shortcut.cmp(&b.shortcut));
        out
    }

    /// Действующие сочетания, которые перехватывает браузер.
    pub fn browser_conflicts(&self) -> Vec<(CommandId, Shortcut)> {
        let reserved = browser_reserved(self.platform);
        let mut out = Vec::new();
        for c in COMMANDS {
            for s in self.bindings(c.id) {
                if reserved.contains(s) {
                    out.push((c.id, s.clone()));
                }
            }
        }
        out
    }

    /// Задаёт список сочетаний команды как есть (конфликты разрешены — их покажет
    /// `conflicts`). Список, совпадающий с умолчанием, перестаёт быть отличием.
    pub fn set_bindings(&mut self, id: CommandId, list: Vec<Shortcut>) {
        let mut unique: Vec<Shortcut> = Vec::with_capacity(list.len());
        for s in list {
            if !unique.contains(&s) {
                unique.push(s);
            }
        }
        let mut list = unique;
        list.truncate(MAX_SHORTCUTS_PER_COMMAND);
        if self.default_bindings(id) == list.as_slice() {
            self.overrides.remove(&id);
        } else {
            self.overrides.insert(id, list);
        }
        self.reindex();
    }

    /// Добавляет сочетание команде. Если оно занято другими командами: без `replace`
    /// — ничего не меняет и возвращает конфликт (интерфейс предлагает заменить),
    /// с `replace` — снимает сочетание с этих команд. Если у команды уже
    /// `MAX_SHORTCUTS_PER_COMMAND` сочетаний, ничего не меняется и возвращается
    /// `BindError::TooMany`: проверка идёт до снятия сочетания с других команд,
    /// иначе оно пропало бы у обеих.
    pub fn bind(&mut self, id: CommandId, s: Shortcut, replace: bool) -> Result<(), BindError> {
        let current = self.bindings(id);
        if !current.contains(&s) && current.len() >= MAX_SHORTCUTS_PER_COMMAND {
            return Err(BindError::TooMany);
        }
        let others: Vec<CommandId> = self
            .commands_for(&s)
            .into_iter()
            .filter(|c| *c != id)
            .collect();
        if !others.is_empty() {
            if !replace {
                return Err(BindError::Conflict(Conflict {
                    shortcut: s,
                    commands: others,
                }));
            }
            for other in others {
                let list = self
                    .bindings(other)
                    .iter()
                    .filter(|x| **x != s)
                    .cloned()
                    .collect();
                self.set_bindings(other, list);
            }
        }
        let mut list = self.bindings(id).to_vec();
        if !list.contains(&s) {
            list.push(s);
        }
        self.set_bindings(id, list);
        Ok(())
    }

    pub fn unbind(&mut self, id: CommandId, s: &Shortcut) {
        let list = self
            .bindings(id)
            .iter()
            .filter(|x| *x != s)
            .cloned()
            .collect();
        self.set_bindings(id, list);
    }

    pub fn reset(&mut self, id: CommandId) {
        self.overrides.remove(&id);
        self.reindex();
    }

    pub fn reset_all(&mut self) {
        self.overrides.clear();
        self.reindex();
    }

    /// Отличия от умолчаний в JSON — то, что хранится в настройках пользователя.
    pub fn export_json(&self) -> String {
        let overrides: serde_json::Map<String, Value> = self
            .overrides
            .iter()
            .map(|(id, list)| {
                let list: Vec<Value> = list.iter().map(|s| Value::from(s.to_string())).collect();
                (id.key().to_string(), Value::from(list))
            })
            .collect();
        let doc = json!({
            "format": FORMAT,
            "version": VERSION,
            "platform": platform_key(self.platform),
            "overrides": overrides,
        });
        serde_json::to_string_pretty(&doc).expect("JSON из строк всегда сериализуется")
    }

    /// Заменяет отличия импортированными. Файл не доверенный: при ошибке формата
    /// раскладка не меняется. Неизвестные команды и непонятные сочетания
    /// пропускаются — о них возвращаются предупреждения.
    pub fn import_json(&mut self, text: &str) -> Result<Vec<String>, KeymapError> {
        if text.len() > MAX_KEYMAP_JSON {
            return Err(KeymapError::TooLarge);
        }
        let doc: Value =
            serde_json::from_str(text).map_err(|e| KeymapError::BadFormat(e.to_string()))?;
        let obj = doc
            .as_object()
            .ok_or_else(|| KeymapError::BadFormat("ожидался объект".into()))?;
        if obj.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err(KeymapError::BadFormat("нет поля format".into()));
        }
        let version = obj
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| KeymapError::BadFormat("нет поля version".into()))?;
        if version > VERSION {
            return Err(KeymapError::NewerVersion(version));
        }
        let overrides = obj
            .get("overrides")
            .and_then(Value::as_object)
            .ok_or_else(|| KeymapError::BadFormat("нет поля overrides".into()))?;
        let mut warnings = Vec::new();
        // Файл другой группы платформ: Ctrl и Cmd меняются местами (этап 6).
        let swap = match obj.get("platform").and_then(Value::as_str) {
            None => false,
            Some(p) => match platform_from_key(p) {
                Some(from) => {
                    let swap = (from == Platform::Mac) != (self.platform == Platform::Mac);
                    if swap {
                        warnings.push(
                            if from == Platform::Mac {
                                "Раскладка с macOS: Cmd заменён на Ctrl"
                            } else {
                                "Раскладка с Windows/Linux: Ctrl заменён на Cmd"
                            }
                            .to_string(),
                        );
                    }
                    swap
                }
                None => {
                    warnings.push("Неизвестная платформа раскладки — сочетания как в файле".into());
                    false
                }
            },
        };
        let mut parsed = BTreeMap::new();
        for (key, value) in overrides {
            let Some(id) = CommandId::from_key(key) else {
                warnings.push(format!(
                    "Неизвестная команда «{}» пропущена",
                    key.chars().take(64).collect::<String>()
                ));
                continue;
            };
            let Some(items) = value.as_array() else {
                warnings.push(format!("У команды {key} не список сочетаний — пропущена"));
                continue;
            };
            // Сначала разбираем все, потом применяем предел: иначе 8 непонятных
            // сочетаний спрятали бы понятное девятое. Число элементов ограничено
            // размером файла (64 КБ).
            let mut list: Vec<Shortcut> = Vec::new();
            for item in items {
                match item.as_str().map(Shortcut::parse) {
                    Some(Ok(mut s)) => {
                        if swap {
                            std::mem::swap(&mut s.mods.ctrl, &mut s.mods.meta);
                        }
                        if !list.contains(&s) {
                            list.push(s);
                        }
                    }
                    Some(Err(e)) => warnings.push(format!("{key}: {e}")),
                    None => warnings.push(format!("{key}: сочетание должно быть строкой")),
                }
            }
            // Ни одного понятного сочетания из непустого списка — это не намерение
            // снять сочетания: команда пропускается и сохраняет умолчание.
            if list.is_empty() && !items.is_empty() {
                warnings.push(format!(
                    "У команды {key} нет ни одного понятного сочетания — пропущена"
                ));
                continue;
            }
            if list.len() > MAX_SHORTCUTS_PER_COMMAND {
                list.truncate(MAX_SHORTCUTS_PER_COMMAND);
                warnings.push(format!(
                    "{key}: больше {MAX_SHORTCUTS_PER_COMMAND} сочетаний, лишние пропущены"
                ));
            }
            parsed.insert(id, list);
        }
        self.overrides.clear();
        for (id, list) in parsed {
            self.set_bindings(id, list);
        }
        self.reindex();
        Ok(warnings)
    }
}

/// Платформа в файле раскладки.
fn platform_key(p: Platform) -> &'static str {
    match p {
        Platform::Mac => "mac",
        Platform::Windows => "windows",
        Platform::Linux => "linux",
    }
}

fn platform_from_key(s: &str) -> Option<Platform> {
    match s {
        "mac" => Some(Platform::Mac),
        "windows" => Some(Platform::Windows),
        "linux" => Some(Platform::Linux),
        _ => None,
    }
}
