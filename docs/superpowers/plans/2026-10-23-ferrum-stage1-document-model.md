# Ferrum, подсистема 1, этап 1: модель документа — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Крейт `ferrum-doc` — модель документа Ferrum на Loro: страницы, узлы, свойства, своя история отмены, формат `.ferrum` с кэшем сцены и контрольный бенчмарк, подтверждающий цели производительности.

**Architecture:** Корневой `LoroDoc` хранит метаданные и список страниц, у каждой страницы свой `LoroDoc` с `LoroTree` узлов. Страница держит индекс стабильных ID и зеркало свойств в памяти; правки пишут обратные операции в журнал транзакции для undo. Файл `.ferrum` — бинарный контейнер с shallow-снапшотами страниц, изображениями по хэшу и кэшем сцены для мгновенного открытия.

**Tech Stack:** Rust 1.96, loro 1.16.2 (+ loro-internal 1.16.2), thiserror 2, blake3 1.8, proptest 1.12, criterion 0.8.

**Спека:** `docs/superpowers/specs/2026-10-23-ferrum-core-design.md`, разделы 3, 5, 12.

## Global Constraints

- Rust toolchain `1.96.0` (`rust-toolchain.toml`), edition 2021.
- `loro = "=1.16.2"` и `loro-internal = "=1.16.2"`: с `loro-internal` 1.17 эта версия loro не собирается.
- Крейты кроме `ferrum-doc` не видят Loro: весь доступ — через публичный API `ferrum-doc`.
- Стабильный `NodeId` — `TreeID` первого создания узла, записанный ключом `id` в его свойства; узлы ищутся только через индекс страницы.
- Нейтральные значения свойств (`x`, `y`, `rotation`, `cornerRadius` = 0, `opacity` = 1, `visible` = true, `locked` = false) не записываются в Loro; эти значения по умолчанию менять нельзя.
- `fills` и `strokes` хранятся значениями-списками, не контейнерами Loro.
- Undo/redo — только через журнал обратных операций; `UndoManager` Loro не используется.
- Страницы сохраняются shallow-снапшотом (`ExportMode::shallow_snapshot`).
- Тексты ошибок и предупреждений — на русском, для пользователя: что случилось и что делать.
- Комментарии в коде — на русском, объясняют «почему», а не «что».
- Цели бенчмарка (100 000 узлов, release, нативно): первый кадр ≤ 300 мс, фоновая загрузка ≤ 1500 мс, память ≤ 400 МБ, файл ≤ 30 МБ, правка ≤ 1 мс, undo ≤ 50 мс, сдвиг 1000 узлов ≤ 50 мс, полное сохранение ≤ 1500 мс.

---

## Этапы подсистемы 1

Подсистема 1 (спека целиком) делится на шесть этапов. У каждого свой план, и каждый заканчивается работающим протестированным результатом. Этот документ — план этапа 1. Планы следующих этапов пишутся после завершения предыдущего: каждый строится на API, который реально получился.

| Этап | Результат | Разделы спеки |
|---|---|---|
| **1. Модель документа** (этот план) | `ferrum-doc`: страницы, узлы, undo/redo, `.ferrum`, кэш сцены, бенчмарк | 3, 5 |
| 2. Сцена и рендер | `ferrum-scene` (трансформы, bounds, R-tree, превью жеста), `ferrum-render` (vello, камера, оверлеи, LOD, экспорт PNG); эталонные картинки через `vello_cpu` | 6, 8 |
| 3. Редактор без браузера | `ferrum-editor`: инструменты, выделение, привязки, hit-testing, таблица команд, раскладка клавиш, модель буфера обмена; сценарные тесты | 7 |
| 4. Worker и обмен данными | `ferrum-wasm`, Web Worker + OffscreenCanvas, `Transport` (SAB + postMessage, запасной режим), цикл кадров, запасные рендеры; холст в браузере | 4.2–4.3, 8.1–8.2 |
| 5. Интерфейс | `apps/web`: SolidJS, планировщик measure/mutate, раскладка UI3, панели, палитра цвета, настройки горячих клавиш | 4.4, 9 |
| 6. Файлы и десктоп | `FileStore` (IndexedDB, File System Access, Tauri), автосохранение и журнал, восстановление после сбоя, обработка ошибок, `apps/desktop`, E2E в Playwright | 10, 11, 12 |

---

## Структура файлов этапа 1

| Файл | Ответственность |
|---|---|
| `Cargo.toml` | Cargo workspace, общие версии зависимостей |
| `rust-toolchain.toml` | версия Rust |
| `crates/ferrum-doc/src/lib.rs` | публичный API крейта |
| `crates/ferrum-doc/src/error.rs` | `DocError`, `DocResult` |
| `crates/ferrum-doc/src/types.rs` | `NodeId`, `PageId`, `Color`, `NodeKind`, `Fill`, `Stroke`, `Props`, `PropsPatch` |
| `crates/ferrum-doc/src/codec.rs` | чтение и запись свойств узла в `LoroMap` |
| `crates/ferrum-doc/src/binary.rs` | little-endian чтение и запись для формата файла |
| `crates/ferrum-doc/src/scene_cache.rs` | `SceneRow` и бинарная таблица кэша сцены |
| `crates/ferrum-doc/src/page.rs` | `PageDoc`: дерево узлов, индекс стабильных ID, зеркало свойств, журнал обратных операций |
| `crates/ferrum-doc/src/history.rs` | глобальный стек undo/redo |
| `crates/ferrum-doc/src/images.rs` | `ImageStore`: изображения по хэшу blake3 |
| `crates/ferrum-doc/src/document.rs` | `Document`: корневой документ, страницы, ленивая загрузка, транзакции, undo/redo |
| `crates/ferrum-doc/src/container.rs` | формат `.ferrum`: `encode`, `decode` |
| `crates/ferrum-doc/src/fixture.rs` | генератор больших страниц для бенчмарков |
| `crates/ferrum-doc/tests/*.rs` | интеграционные тесты по модулям |
| `crates/ferrum-doc/benches/doc_bench.rs` | регрессионные бенчмарки criterion |
| `crates/ferrum-doc/examples/bench_report.rs` | контрольный бенчмарк с целями этапа |

Все команды запускаются из корня репозитория.

---
### Task 1: Каркас workspace и базовые типы

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `.gitignore`
- Create: `crates/ferrum-doc/Cargo.toml`
- Create: `crates/ferrum-doc/src/lib.rs`
- Create: `crates/ferrum-doc/src/error.rs`
- Create: `crates/ferrum-doc/src/types.rs`
- Test: `crates/ferrum-doc/tests/types.rs`

**Interfaces:**
- Consumes: —
- Produces: `DocError` (варианты `InvalidId, InvalidColor, NodeNotFound, PageNotFound, UnknownKind, CyclicMove, ReadOnly, NotAFerrumFile, UnsupportedFormat(u32), Corrupted(String), Loro, LoroEncode`), `DocResult<T>`; `NodeId(pub(crate) TreeID)` и `PageId(pub(crate) TreeID)` с `Display`/`FromStr`; `Color { r, g, b, a }` с `rgba`, `parse`, `to_hex`, константами `BLACK, WHITE, SHAPE_GRAY`; `NodeKind` (`Frame, Group, Rect, Ellipse, Line, Image`) с `as_str`, `parse`, `default_name`; `Fill { kind, color, image_hash, scale_mode, opacity, visible }` с `solid`, `image`; `Stroke { color, width, align }`; `Props` с `defaults(kind)`; `PropsPatch` (все поля `Option`) с `apply_to(&mut Props)` и `previous(&Props) -> PropsPatch`.

- [ ] **Step 1: Создать workspace**

`Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/ferrum-doc"]

[workspace.package]
edition = "2021"
rust-version = "1.96"
license = "UNLICENSED"

[workspace.dependencies]
loro = "=1.16.2"
# loro 1.16.2 не собирается с loro-internal 1.17.x — фиксируем совместимую версию
loro-internal = "=1.16.2"
thiserror = "2.0.20"
blake3 = "1.8"
criterion = "0.8"
proptest = "1.12"
```

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.96.0"
components = ["rustfmt", "clippy"]
```

`.gitignore`:

```text
target/
```

`crates/ferrum-doc/Cargo.toml`:

```toml
[package]
name = "ferrum-doc"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
loro.workspace = true
loro-internal.workspace = true
thiserror.workspace = true
blake3.workspace = true

[dev-dependencies]
criterion.workspace = true
proptest.workspace = true
```

`crates/ferrum-doc/src/lib.rs`:

```rust
//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod error;
mod types;

pub use error::{DocError, DocResult};
pub use types::*;
```

- [ ] **Step 2: Написать падающий тест типов**

`crates/ferrum-doc/tests/types.rs`:

```rust
use ferrum_doc::{Color, NodeKind, Props};

#[test]
fn color_parses_rgb_and_rgba() {
    assert_eq!(
        Color::parse("#FF8000").unwrap(),
        Color::rgba(255, 128, 0, 255)
    );
    assert_eq!(
        Color::parse("#ff800080").unwrap(),
        Color::rgba(255, 128, 0, 128)
    );
}

#[test]
fn color_rejects_garbage() {
    for bad in ["ff8000", "#ff80", "#gg8000", "#ff8000ff00", "#ффф000"] {
        assert!(Color::parse(bad).is_err(), "{bad} должен быть отклонён");
    }
}

#[test]
fn color_hex_roundtrip_is_lowercase_rgba() {
    let c = Color::rgba(0xAB, 0x01, 0xFF, 0x7F);
    assert_eq!(c.to_hex(), "#ab01ff7f");
    assert_eq!(Color::parse(&c.to_hex()).unwrap(), c);
}

#[test]
fn node_kind_roundtrip() {
    for k in [
        NodeKind::Frame,
        NodeKind::Group,
        NodeKind::Rect,
        NodeKind::Ellipse,
        NodeKind::Line,
        NodeKind::Image,
    ] {
        assert_eq!(NodeKind::parse(k.as_str()), Some(k));
    }
    assert_eq!(NodeKind::parse("star"), None);
}

#[test]
fn defaults_follow_figma() {
    let frame = Props::defaults(NodeKind::Frame);
    assert!(frame.clip_content);
    assert_eq!(frame.fills[0].color, Color::WHITE);
    let rect = Props::defaults(NodeKind::Rect);
    assert_eq!(rect.fills[0].color, Color::SHAPE_GRAY);
    assert!(!rect.clip_content);
    let line = Props::defaults(NodeKind::Line);
    assert!(line.fills.is_empty());
    assert_eq!(line.strokes.len(), 1);
    assert_eq!(line.h, 0.0);
}
```

- [ ] **Step 3: Убедиться, что тест не компилируется**

Run: `cargo test -p ferrum-doc --test types`
Expected: FAIL — `error[E0583]: file not found for module` (`error`, `types`).

- [ ] **Step 4: Реализовать ошибки и типы**

`crates/ferrum-doc/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocError {
    #[error("Некорректный идентификатор: {0}")]
    InvalidId(String),
    #[error("Некорректный цвет: {0}. Ожидается #rrggbb или #rrggbbaa")]
    InvalidColor(String),
    #[error("Узел {0} не найден или удалён")]
    NodeNotFound(String),
    #[error("Страница {0} не найдена")]
    PageNotFound(String),
    #[error("Узел {0} хранит неизвестный тип «{1}»")]
    UnknownKind(String, String),
    #[error("Нельзя переместить узел внутрь самого себя")]
    CyclicMove,
    #[error("Документ открыт только для чтения")]
    ReadOnly,
    #[error("Это не файл Ferrum")]
    NotAFerrumFile,
    #[error("Файл создан в более новой версии формата ({0}). Обновите Ferrum")]
    UnsupportedFormat(u32),
    #[error("Файл повреждён: {0}")]
    Corrupted(String),
    #[error("Ошибка Loro: {0}")]
    Loro(#[from] loro::LoroError),
    #[error("Ошибка экспорта Loro: {0}")]
    LoroEncode(#[from] loro::LoroEncodeError),
}

pub type DocResult<T> = Result<T, DocError>;
```

`crates/ferrum-doc/src/types.rs`:

```rust
//! Типы данных документа: идентификаторы, цвета, узлы и их свойства.

use std::fmt;
use std::str::FromStr;

use loro::TreeID;

use crate::error::DocError;

/// Идентификатор узла. Обёртка над `TreeID` Loro: уникален и при совместной работе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) TreeID);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for NodeId {
    type Err = DocError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TreeID::try_from(s)
            .map(NodeId)
            .map_err(|_| DocError::InvalidId(s.to_string()))
    }
}

/// Идентификатор страницы — узел в дереве страниц корневого документа.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId(pub(crate) TreeID);

impl fmt::Display for PageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for PageId {
    type Err = DocError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TreeID::try_from(s)
            .map(PageId)
            .map_err(|_| DocError::InvalidId(s.to_string()))
    }
}

/// Цвет sRGB с альфой. В документе хранится строкой `#rrggbbaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgba(0, 0, 0, 255);
    pub const WHITE: Color = Color::rgba(255, 255, 255, 255);
    /// Серая заливка новых фигур, как в Figma.
    pub const SHAPE_GRAY: Color = Color::rgba(0xd9, 0xd9, 0xd9, 255);

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    /// Принимает `#rrggbb` и `#rrggbbaa` в любом регистре.
    pub fn parse(s: &str) -> Result<Color, DocError> {
        let bad = || DocError::InvalidColor(s.to_string());
        let hex = s.strip_prefix('#').ok_or_else(bad)?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
            return Err(bad());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        let a = if hex.len() == 8 { byte(6)? } else { 255 };
        Ok(Color::rgba(byte(0)?, byte(2)?, byte(4)?, a))
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Frame,
    Group,
    Rect,
    Ellipse,
    Line,
    Image,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Frame => "frame",
            NodeKind::Group => "group",
            NodeKind::Rect => "rect",
            NodeKind::Ellipse => "ellipse",
            NodeKind::Line => "line",
            NodeKind::Image => "image",
        }
    }

    pub fn parse(s: &str) -> Option<NodeKind> {
        Some(match s {
            "frame" => NodeKind::Frame,
            "group" => NodeKind::Group,
            "rect" => NodeKind::Rect,
            "ellipse" => NodeKind::Ellipse,
            "line" => NodeKind::Line,
            "image" => NodeKind::Image,
            _ => return None,
        })
    }

    /// Имя нового узла в интерфейсе.
    pub fn default_name(self) -> &'static str {
        match self {
            NodeKind::Frame => "Фрейм",
            NodeKind::Group => "Группа",
            NodeKind::Rect => "Прямоугольник",
            NodeKind::Ellipse => "Эллипс",
            NodeKind::Line => "Линия",
            NodeKind::Image => "Изображение",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillKind {
    Solid,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    Fill,
    Fit,
    Tile,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub kind: FillKind,
    pub color: Color,
    /// Хэш blake3 (hex) байтов изображения; только для `FillKind::Image`.
    pub image_hash: Option<String>,
    pub scale_mode: ScaleMode,
    pub opacity: f64,
    pub visible: bool,
}

impl Fill {
    pub fn solid(color: Color) -> Fill {
        Fill {
            kind: FillKind::Solid,
            color,
            image_hash: None,
            scale_mode: ScaleMode::Fill,
            opacity: 1.0,
            visible: true,
        }
    }

    pub fn image(hash: impl Into<String>) -> Fill {
        Fill {
            kind: FillKind::Image,
            color: Color::WHITE,
            image_hash: Some(hash.into()),
            scale_mode: ScaleMode::Fill,
            opacity: 1.0,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeAlign {
    Inside,
    Center,
    Outside,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f64,
    pub align: StrokeAlign,
}

/// Полный набор свойств узла после чтения из документа.
#[derive(Debug, Clone, PartialEq)]
pub struct Props {
    pub kind: NodeKind,
    pub name: String,
    pub x: f64,
    pub y: f64,
    /// У группы не хранится: границы вычисляет сцена по детям.
    pub w: f64,
    pub h: f64,
    pub rotation: f64,
    pub opacity: f64,
    pub visible: bool,
    pub locked: bool,
    pub corner_radius: f64,
    pub clip_content: bool,
    pub fills: Vec<Fill>,
    pub strokes: Vec<Stroke>,
}

impl Props {
    /// Значения по умолчанию для нового узла, как в Figma.
    pub fn defaults(kind: NodeKind) -> Props {
        let (w, h, fills, strokes) = match kind {
            NodeKind::Frame => (100.0, 100.0, vec![Fill::solid(Color::WHITE)], vec![]),
            NodeKind::Group => (0.0, 0.0, vec![], vec![]),
            NodeKind::Rect | NodeKind::Ellipse => {
                (100.0, 100.0, vec![Fill::solid(Color::SHAPE_GRAY)], vec![])
            }
            NodeKind::Line => (
                100.0,
                0.0,
                vec![],
                vec![Stroke {
                    color: Color::BLACK,
                    width: 1.0,
                    align: StrokeAlign::Center,
                }],
            ),
            NodeKind::Image => (100.0, 100.0, vec![], vec![]),
        };
        Props {
            kind,
            name: kind.default_name().to_string(),
            x: 0.0,
            y: 0.0,
            w,
            h,
            rotation: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            corner_radius: 0.0,
            clip_content: kind == NodeKind::Frame,
            fills,
            strokes,
        }
    }
}

/// Частичное изменение свойств: `None` — не трогать.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropsPatch {
    pub name: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub w: Option<f64>,
    pub h: Option<f64>,
    pub rotation: Option<f64>,
    pub opacity: Option<f64>,
    pub visible: Option<bool>,
    pub locked: Option<bool>,
    pub corner_radius: Option<f64>,
    pub clip_content: Option<bool>,
    pub fills: Option<Vec<Fill>>,
    pub strokes: Option<Vec<Stroke>>,
}

impl PropsPatch {
    /// Прежние значения тех полей, которые меняет этот патч, — патч для отмены.
    pub fn previous(&self, before: &Props) -> PropsPatch {
        PropsPatch {
            name: self.name.as_ref().map(|_| before.name.clone()),
            x: self.x.map(|_| before.x),
            y: self.y.map(|_| before.y),
            w: self.w.map(|_| before.w),
            h: self.h.map(|_| before.h),
            rotation: self.rotation.map(|_| before.rotation),
            opacity: self.opacity.map(|_| before.opacity),
            visible: self.visible.map(|_| before.visible),
            locked: self.locked.map(|_| before.locked),
            corner_radius: self.corner_radius.map(|_| before.corner_radius),
            clip_content: self.clip_content.map(|_| before.clip_content),
            fills: self.fills.as_ref().map(|_| before.fills.clone()),
            strokes: self.strokes.as_ref().map(|_| before.strokes.clone()),
        }
    }

    /// Применяет изменения к уже прочитанным свойствам (для зеркала страницы).
    pub fn apply_to(&self, p: &mut Props) {
        if let Some(v) = &self.name {
            p.name = v.clone();
        }
        for (dst, src) in [
            (&mut p.x, self.x),
            (&mut p.y, self.y),
            (&mut p.w, self.w),
            (&mut p.h, self.h),
            (&mut p.rotation, self.rotation),
            (&mut p.opacity, self.opacity),
            (&mut p.corner_radius, self.corner_radius),
        ] {
            if let Some(v) = src {
                *dst = v;
            }
        }
        for (dst, src) in [
            (&mut p.visible, self.visible),
            (&mut p.locked, self.locked),
            (&mut p.clip_content, self.clip_content),
        ] {
            if let Some(v) = src {
                *dst = v;
            }
        }
        if let Some(v) = &self.fills {
            p.fills = v.clone();
        }
        if let Some(v) = &self.strokes {
            p.strokes = v.clone();
        }
    }
}
```

- [ ] **Step 5: Запустить тесты**

Run: `cargo test -p ferrum-doc --test types`
Expected: PASS, `5 passed`. Первая сборка скачивает loro и занимает несколько минут.

- [ ] **Step 6: Закоммитить**

```bash
git add Cargo.toml Cargo.lock rust-toolchain.toml .gitignore crates/ferrum-doc
git commit -m "feat(doc): каркас workspace и базовые типы документа"
```

---

### Task 2: Страница: узлы, стабильные ID, зеркало свойств, журнал отмены

**Files:**
- Create: `crates/ferrum-doc/src/binary.rs`
- Create: `crates/ferrum-doc/src/codec.rs`
- Create: `crates/ferrum-doc/src/scene_cache.rs`
- Create: `crates/ferrum-doc/src/page.rs`
- Modify: `crates/ferrum-doc/src/lib.rs`
- Test: `crates/ferrum-doc/tests/page.rs`

**Interfaces:**
- Consumes: всё из Task 1.
- Produces: `PageDoc` с методами `new()`, `from_snapshot(&[u8])`, `pub(crate) load(&[u8], Option<&[SceneRow]>)`, `export_snapshot() -> DocResult<Vec<u8>>` (shallow), `create_node(parent: Option<NodeId>, index: Option<usize>, kind) -> DocResult<NodeId>`, `props(id) -> DocResult<Props>`, `kind(id)`, `set_props(id, &PropsPatch)`, `move_node(id, parent: Option<NodeId>, index: usize)`, `delete_nodes(&[NodeId])`, `children(Option<NodeId>) -> Vec<NodeId>`, `parent(id) -> Option<NodeId>`, `walk() -> Vec<NodeId>`, `contains(id) -> bool`, `scene_rows() -> DocResult<Vec<SceneRow>>`, `image_refs() -> DocResult<Vec<String>>`, `commit_for_tests()`; для Task 3: `pub(crate) commit()`, `begin_journal()`, `take_journal() -> Vec<Inverse>`, `apply_inverse(&[Inverse]) -> DocResult<Vec<Inverse>>` и `pub(crate) enum Inverse`. `SceneRow { id: NodeId, pub(crate) tree_id: TreeID, parent: Option<u32>, props: Props }`; `scene_cache::encode(&[SceneRow]) -> Vec<u8>`, `scene_cache::decode(&[u8]) -> Option<Vec<SceneRow>>`; `binary::{Reader, Writer}`.

- [ ] **Step 1: Написать падающие тесты страницы**

`crates/ferrum-doc/tests/page.rs`:

```rust
use ferrum_doc::*;

#[test]
fn create_and_read_node_with_defaults() {
    let page = PageDoc::new();
    let id = page.create_node(None, None, NodeKind::Rect).unwrap();
    let p = page.props(id).unwrap();
    assert_eq!(p, Props::defaults(NodeKind::Rect));
    assert_eq!(page.kind(id).unwrap(), NodeKind::Rect);
}

#[test]
fn set_props_changes_only_given_fields() {
    let page = PageDoc::new();
    let id = page.create_node(None, None, NodeKind::Rect).unwrap();
    page.set_props(
        id,
        &PropsPatch {
            x: Some(12.5),
            name: Some("Фон".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let p = page.props(id).unwrap();
    assert_eq!(p.x, 12.5);
    assert_eq!(p.name, "Фон");
    assert_eq!(p.y, 0.0);
    assert_eq!(p.w, 100.0);
}

#[test]
fn fills_and_strokes_roundtrip() {
    let page = PageDoc::new();
    let id = page.create_node(None, None, NodeKind::Rect).unwrap();
    let fills = vec![
        Fill::solid(Color::parse("#ff000080").unwrap()),
        Fill {
            scale_mode: ScaleMode::Tile,
            opacity: 0.5,
            ..Fill::image("abc123")
        },
    ];
    let strokes = vec![Stroke {
        color: Color::BLACK,
        width: 2.0,
        align: StrokeAlign::Inside,
    }];
    page.set_props(
        id,
        &PropsPatch {
            fills: Some(fills.clone()),
            strokes: Some(strokes.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let p = page.props(id).unwrap();
    assert_eq!(p.fills, fills);
    assert_eq!(p.strokes, strokes);
    assert_eq!(page.image_refs().unwrap(), vec!["abc123".to_string()]);
}

#[test]
fn children_keep_order_and_index_insert() {
    let page = PageDoc::new();
    let frame = page.create_node(None, None, NodeKind::Frame).unwrap();
    let a = page.create_node(Some(frame), None, NodeKind::Rect).unwrap();
    let b = page
        .create_node(Some(frame), None, NodeKind::Ellipse)
        .unwrap();
    let c = page
        .create_node(Some(frame), Some(0), NodeKind::Line)
        .unwrap();
    assert_eq!(page.children(Some(frame)), vec![c, a, b]);
    assert_eq!(page.parent(a), Some(frame));
    assert_eq!(page.parent(frame), None);
}

#[test]
fn move_node_reorders_and_reparents() {
    let page = PageDoc::new();
    let f1 = page.create_node(None, None, NodeKind::Frame).unwrap();
    let f2 = page.create_node(None, None, NodeKind::Frame).unwrap();
    let a = page.create_node(Some(f1), None, NodeKind::Rect).unwrap();
    let b = page.create_node(Some(f1), None, NodeKind::Rect).unwrap();
    page.move_node(b, Some(f1), 0).unwrap();
    assert_eq!(page.children(Some(f1)), vec![b, a]);
    page.move_node(a, Some(f2), 0).unwrap();
    assert_eq!(page.children(Some(f1)), vec![b]);
    assert_eq!(page.children(Some(f2)), vec![a]);
}

#[test]
fn move_into_own_descendant_is_rejected() {
    let page = PageDoc::new();
    let outer = page.create_node(None, None, NodeKind::Frame).unwrap();
    let inner = page
        .create_node(Some(outer), None, NodeKind::Frame)
        .unwrap();
    assert!(matches!(
        page.move_node(outer, Some(inner), 0),
        Err(DocError::CyclicMove)
    ));
    assert!(matches!(
        page.move_node(outer, Some(outer), 0),
        Err(DocError::CyclicMove)
    ));
}

#[test]
fn delete_removes_subtree() {
    let page = PageDoc::new();
    let frame = page.create_node(None, None, NodeKind::Frame).unwrap();
    let child = page.create_node(Some(frame), None, NodeKind::Rect).unwrap();
    page.delete_nodes(&[frame, frame]).unwrap();
    assert!(!page.contains(frame));
    assert!(!page.contains(child));
    assert!(page.children(None).is_empty());
    assert!(matches!(page.props(child), Err(DocError::NodeNotFound(_))));
}

#[test]
fn walk_is_depth_first_bottom_to_top() {
    let page = PageDoc::new();
    let f = page.create_node(None, None, NodeKind::Frame).unwrap();
    let a = page.create_node(Some(f), None, NodeKind::Rect).unwrap();
    let b = page.create_node(None, None, NodeKind::Rect).unwrap();
    assert_eq!(page.walk(), vec![f, a, b]);
}

#[test]
fn snapshot_roundtrip() {
    let page = PageDoc::new();
    let f = page.create_node(None, None, NodeKind::Frame).unwrap();
    page.set_props(
        f,
        &PropsPatch {
            w: Some(393.0),
            h: Some(852.0),
            ..Default::default()
        },
    )
    .unwrap();
    let copy = PageDoc::from_snapshot(&page.export_snapshot().unwrap()).unwrap();
    assert_eq!(copy.props(f).unwrap(), page.props(f).unwrap());
}

#[test]
fn node_id_string_roundtrip() {
    let page = PageDoc::new();
    let id = page.create_node(None, None, NodeKind::Rect).unwrap();
    let parsed: NodeId = id.to_string().parse().unwrap();
    assert_eq!(parsed, id);
    assert!("мусор".parse::<NodeId>().is_err());
}
```

- [ ] **Step 2: Подключить модули в `lib.rs`**

`crates/ferrum-doc/src/lib.rs`:

```rust
//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod binary;
mod codec;
mod error;
mod page;
mod scene_cache;
mod types;

pub use error::{DocError, DocResult};
pub use page::PageDoc;
pub use scene_cache::SceneRow;
pub use types::*;
```

- [ ] **Step 3: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-doc --test page`
Expected: FAIL — `file not found for module` (`binary`, `codec`, `page`, `scene_cache`).

- [ ] **Step 4: Реализовать двоичные помощники и кодек свойств**

Кодек хранит каждое свойство отдельным ключом, не пишет нейтральные значения и хранит заливки и обводки значениями: контейнер на каждую заливку раздувал файл почти втрое.

`crates/ferrum-doc/src/binary.rs`:

```rust
//! Простые чтение и запись little-endian для формата `.ferrum` и кэша сцены.

pub(crate) struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Writer {
        Writer { buf: Vec::new() }
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }

    /// Строка с длиной u16. Длинные строки обрезаются по границе символа.
    pub fn str16(&mut self, s: &str) {
        let mut end = s.len().min(u16::MAX as usize);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.u16(end as u16);
        self.bytes(&s.as_bytes()[..end]);
    }
}

pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Reader<'a> {
        Reader { buf, pos: 0 }
    }

    pub fn at(buf: &'a [u8], pos: usize) -> Reader<'a> {
        Reader { buf, pos }
    }

    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }

    pub fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    pub fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    pub fn str16(&mut self) -> Option<String> {
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}
```

`crates/ferrum-doc/src/codec.rs`:

```rust
//! Чтение и запись свойств узла в `LoroMap`. Каждое свойство — отдельный ключ,
//! чтобы одновременные правки разных свойств одного узла не затирали друг друга.

use std::collections::HashMap;

use loro::{Container, LoroMap, LoroValue, ValueOrContainer};

use crate::error::{DocError, DocResult};
use crate::types::*;

pub(crate) mod key {
    pub const TYPE: &str = "type";
    pub const NAME: &str = "name";
    pub const X: &str = "x";
    pub const Y: &str = "y";
    pub const W: &str = "w";
    pub const H: &str = "h";
    pub const ROTATION: &str = "rotation";
    pub const OPACITY: &str = "opacity";
    pub const VISIBLE: &str = "visible";
    pub const LOCKED: &str = "locked";
    pub const CORNER_RADIUS: &str = "cornerRadius";
    pub const CLIP_CONTENT: &str = "clipContent";
    pub const FILLS: &str = "fills";
    pub const STROKES: &str = "strokes";
}

fn value(map: &LoroMap, k: &str) -> Option<LoroValue> {
    match map.get(k)? {
        ValueOrContainer::Value(v) => Some(v),
        ValueOrContainer::Container(Container::List(l)) => Some(l.get_deep_value()),
        ValueOrContainer::Container(Container::Map(m)) => Some(m.get_deep_value()),
        ValueOrContainer::Container(_) => None,
    }
}

fn as_f64(v: &LoroValue) -> Option<f64> {
    match v {
        LoroValue::Double(d) => Some(*d),
        LoroValue::I64(i) => Some(*i as f64),
        _ => None,
    }
}

fn as_bool(v: &LoroValue) -> Option<bool> {
    match v {
        LoroValue::Bool(b) => Some(*b),
        _ => None,
    }
}

fn as_str(v: &LoroValue) -> Option<String> {
    match v {
        LoroValue::String(s) => Some(s.to_string()),
        _ => None,
    }
}

fn f64_or(map: &LoroMap, k: &str, default: f64) -> f64 {
    value(map, k).as_ref().and_then(as_f64).unwrap_or(default)
}

fn bool_or(map: &LoroMap, k: &str, default: bool) -> bool {
    value(map, k).as_ref().and_then(as_bool).unwrap_or(default)
}

/// Тип узла читается отдельно: по нему выбираются значения по умолчанию.
pub(crate) fn read_kind(map: &LoroMap, id: &NodeId) -> DocResult<NodeKind> {
    let raw = value(map, key::TYPE)
        .as_ref()
        .and_then(as_str)
        .ok_or_else(|| DocError::NodeNotFound(id.to_string()))?;
    NodeKind::parse(&raw).ok_or_else(|| DocError::UnknownKind(id.to_string(), raw))
}

/// Отсутствующие ключи заменяются значениями по умолчанию для типа узла,
/// поэтому новые свойства не требуют миграции старых файлов.
pub(crate) fn read_props(map: &LoroMap, id: &NodeId) -> DocResult<Props> {
    let kind = read_kind(map, id)?;
    let d = Props::defaults(kind);
    Ok(Props {
        kind,
        name: value(map, key::NAME)
            .as_ref()
            .and_then(as_str)
            .unwrap_or(d.name),
        x: f64_or(map, key::X, d.x),
        y: f64_or(map, key::Y, d.y),
        w: f64_or(map, key::W, d.w),
        h: f64_or(map, key::H, d.h),
        rotation: f64_or(map, key::ROTATION, d.rotation),
        opacity: f64_or(map, key::OPACITY, d.opacity),
        visible: bool_or(map, key::VISIBLE, d.visible),
        locked: bool_or(map, key::LOCKED, d.locked),
        corner_radius: f64_or(map, key::CORNER_RADIUS, d.corner_radius),
        clip_content: bool_or(map, key::CLIP_CONTENT, d.clip_content),
        fills: match value(map, key::FILLS) {
            Some(v) => read_list(&v, read_fill),
            None => d.fills,
        },
        strokes: match value(map, key::STROKES) {
            Some(v) => read_list(&v, read_stroke),
            None => d.strokes,
        },
    })
}

/// Элементы, которые не удалось разобрать, пропускаются: лучше открыть файл
/// без одной заливки, чем не открыть совсем.
fn read_list<T>(v: &LoroValue, item: fn(&LoroValue) -> Option<T>) -> Vec<T> {
    match v {
        LoroValue::List(list) => list.iter().filter_map(item).collect(),
        _ => Vec::new(),
    }
}

fn field<'a>(v: &'a LoroValue, k: &str) -> Option<&'a LoroValue> {
    match v {
        LoroValue::Map(m) => m.get(k),
        _ => None,
    }
}

fn read_fill(v: &LoroValue) -> Option<Fill> {
    let kind = match field(v, "kind").and_then(as_str)?.as_str() {
        "solid" => FillKind::Solid,
        "image" => FillKind::Image,
        _ => return None,
    };
    let scale_mode = match field(v, "scaleMode").and_then(as_str).as_deref() {
        Some("fit") => ScaleMode::Fit,
        Some("tile") => ScaleMode::Tile,
        _ => ScaleMode::Fill,
    };
    Some(Fill {
        kind,
        color: field(v, "color")
            .and_then(as_str)
            .and_then(|s| Color::parse(&s).ok())
            .unwrap_or(Color::WHITE),
        image_hash: field(v, "imageHash").and_then(as_str),
        scale_mode,
        opacity: field(v, "opacity").and_then(as_f64).unwrap_or(1.0),
        visible: field(v, "visible").and_then(as_bool).unwrap_or(true),
    })
}

fn read_stroke(v: &LoroValue) -> Option<Stroke> {
    let align = match field(v, "align").and_then(as_str).as_deref() {
        Some("inside") => StrokeAlign::Inside,
        Some("outside") => StrokeAlign::Outside,
        _ => StrokeAlign::Center,
    };
    Some(Stroke {
        color: Color::parse(&field(v, "color").and_then(as_str)?).ok()?,
        width: field(v, "width").and_then(as_f64).unwrap_or(1.0),
        align,
    })
}

/// Записывает свойства нового узла. Нейтральные значения (0, opacity 1, видим,
/// не заблокирован) не пишутся: при чтении они подставляются как значения по умолчанию.
/// Поэтому нейтральные значения по умолчанию в схеме менять НЕЛЬЗЯ.
pub(crate) fn write_props(map: &LoroMap, p: &Props) -> DocResult<()> {
    map.insert(key::TYPE, p.kind.as_str())?;
    let neutral = |v: f64, n: f64| (v != n).then_some(v);
    let patch = PropsPatch {
        name: Some(p.name.clone()),
        x: neutral(p.x, 0.0),
        y: neutral(p.y, 0.0),
        w: Some(p.w),
        h: Some(p.h),
        rotation: neutral(p.rotation, 0.0),
        opacity: neutral(p.opacity, 1.0),
        visible: (!p.visible).then_some(false),
        locked: p.locked.then_some(true),
        corner_radius: neutral(p.corner_radius, 0.0),
        clip_content: Some(p.clip_content),
        fills: Some(p.fills.clone()),
        strokes: Some(p.strokes.clone()),
    };
    write_patch(map, &patch)
}

pub(crate) fn write_patch(map: &LoroMap, p: &PropsPatch) -> DocResult<()> {
    if let Some(v) = &p.name {
        map.insert(key::NAME, v.as_str())?;
    }
    for (k, v) in [
        (key::X, p.x),
        (key::Y, p.y),
        (key::W, p.w),
        (key::H, p.h),
        (key::ROTATION, p.rotation),
        (key::OPACITY, p.opacity),
        (key::CORNER_RADIUS, p.corner_radius),
    ] {
        if let Some(v) = v {
            map.insert(k, v)?;
        }
    }
    for (k, v) in [
        (key::VISIBLE, p.visible),
        (key::LOCKED, p.locked),
        (key::CLIP_CONTENT, p.clip_content),
    ] {
        if let Some(v) = v {
            map.insert(k, v)?;
        }
    }
    // Заливки и обводки — неизменяемые значения, а не вложенные контейнеры Loro:
    // контейнер на каждую заливку раздувает документ в разы. Список заменяется целиком.
    if let Some(fills) = &p.fills {
        map.insert(
            key::FILLS,
            LoroValue::from(fills.iter().map(fill_value).collect::<Vec<_>>()),
        )?;
    }
    if let Some(strokes) = &p.strokes {
        map.insert(
            key::STROKES,
            LoroValue::from(strokes.iter().map(stroke_value).collect::<Vec<_>>()),
        )?;
    }
    Ok(())
}

fn obj(fields: Vec<(&str, LoroValue)>) -> LoroValue {
    LoroValue::from(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<HashMap<_, _>>(),
    )
}

fn fill_value(f: &Fill) -> LoroValue {
    let mut fields = vec![
        (
            "kind",
            LoroValue::from(match f.kind {
                FillKind::Solid => "solid",
                FillKind::Image => "image",
            }),
        ),
        ("color", LoroValue::from(f.color.to_hex())),
        (
            "scaleMode",
            LoroValue::from(match f.scale_mode {
                ScaleMode::Fill => "fill",
                ScaleMode::Fit => "fit",
                ScaleMode::Tile => "tile",
            }),
        ),
        ("opacity", LoroValue::from(f.opacity)),
        ("visible", LoroValue::from(f.visible)),
    ];
    if let Some(h) = &f.image_hash {
        fields.push(("imageHash", LoroValue::from(h.as_str())));
    }
    obj(fields)
}

fn stroke_value(s: &Stroke) -> LoroValue {
    obj(vec![
        ("color", LoroValue::from(s.color.to_hex())),
        ("width", LoroValue::from(s.width)),
        (
            "align",
            LoroValue::from(match s.align {
                StrokeAlign::Inside => "inside",
                StrokeAlign::Center => "center",
                StrokeAlign::Outside => "outside",
            }),
        ),
    ])
}
```

- [ ] **Step 5: Реализовать таблицу кэша сцены**

`SceneRow` хранит текущий `TreeID`, чтобы при открытии файла индекс строился без чтения Loro.

`crates/ferrum-doc/src/scene_cache.rs`:

```rust
//! Кэш сцены: плоская таблица всех узлов страницы для мгновенного открытия.
//!
//! Раскодировать снапшот Loro на 100 000 узлов — больше секунды. Поэтому при сохранении
//! рядом со снапшотом пишется эта таблица: по ней холст рисуется сразу, а Loro
//! раскодируется в фоне. Таблица — производные данные: при несовпадении со снапшотом
//! она просто игнорируется.

use loro::TreeID;

use crate::binary::{Reader, Writer};
use crate::types::*;

/// Узел страницы в порядке обхода в глубину: родитель всегда раньше детей.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneRow {
    pub id: NodeId,
    /// Текущий `TreeID` узла в Loro — чтобы построить индекс без чтения Loro.
    pub(crate) tree_id: TreeID,
    /// Индекс строки родителя в той же таблице; `None` — корень страницы.
    pub parent: Option<u32>,
    pub props: Props,
}

const ROOT: u32 = u32::MAX;
const FLAG_VISIBLE: u8 = 1;
const FLAG_LOCKED: u8 = 2;
const FLAG_CLIP: u8 = 4;

fn kind_code(k: NodeKind) -> u8 {
    match k {
        NodeKind::Frame => 0,
        NodeKind::Group => 1,
        NodeKind::Rect => 2,
        NodeKind::Ellipse => 3,
        NodeKind::Line => 4,
        NodeKind::Image => 5,
    }
}

fn kind_from(c: u8) -> Option<NodeKind> {
    [
        NodeKind::Frame,
        NodeKind::Group,
        NodeKind::Rect,
        NodeKind::Ellipse,
        NodeKind::Line,
        NodeKind::Image,
    ]
    .get(c as usize)
    .copied()
}

fn color(w: &mut Writer, c: Color) {
    w.bytes(&[c.r, c.g, c.b, c.a]);
}

fn read_color(r: &mut Reader) -> Option<Color> {
    let b = r.take(4)?;
    Some(Color::rgba(b[0], b[1], b[2], b[3]))
}

pub(crate) fn encode(rows: &[SceneRow]) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(rows.len() as u32);
    for row in rows {
        let p = &row.props;
        w.str16(&row.id.to_string());
        w.str16(&row.tree_id.to_string());
        w.u32(row.parent.unwrap_or(ROOT));
        w.u8(kind_code(p.kind));
        let mut flags = 0;
        if p.visible {
            flags |= FLAG_VISIBLE;
        }
        if p.locked {
            flags |= FLAG_LOCKED;
        }
        if p.clip_content {
            flags |= FLAG_CLIP;
        }
        w.u8(flags);
        for v in [p.x, p.y, p.w, p.h, p.rotation, p.opacity, p.corner_radius] {
            w.f64(v);
        }
        w.str16(&p.name);
        w.u8(p.fills.len().min(255) as u8);
        for f in p.fills.iter().take(255) {
            w.u8(match f.kind {
                FillKind::Solid => 0,
                FillKind::Image => 1,
            });
            color(&mut w, f.color);
            w.u8(match f.scale_mode {
                ScaleMode::Fill => 0,
                ScaleMode::Fit => 1,
                ScaleMode::Tile => 2,
            });
            w.f64(f.opacity);
            w.u8(f.visible as u8);
            w.str16(f.image_hash.as_deref().unwrap_or(""));
        }
        w.u8(p.strokes.len().min(255) as u8);
        for s in p.strokes.iter().take(255) {
            color(&mut w, s.color);
            w.f64(s.width);
            w.u8(match s.align {
                StrokeAlign::Inside => 0,
                StrokeAlign::Center => 1,
                StrokeAlign::Outside => 2,
            });
        }
    }
    w.buf
}

/// `None` — таблица повреждена; вызывающий код тогда просто не использует кэш.
pub(crate) fn decode(bytes: &[u8]) -> Option<Vec<SceneRow>> {
    let mut r = Reader::new(bytes);
    let n = r.u32()? as usize;
    let mut rows = Vec::with_capacity(n.min(bytes.len() / 64));
    for i in 0..n {
        let id: NodeId = r.str16()?.parse().ok()?;
        let tree_id = TreeID::try_from(r.str16()?.as_str()).ok()?;
        let parent = match r.u32()? {
            ROOT => None,
            p if (p as usize) < i => Some(p),
            _ => return None,
        };
        let kind = kind_from(r.u8()?)?;
        let flags = r.u8()?;
        let mut nums = [0.0; 7];
        for v in &mut nums {
            *v = r.f64()?;
        }
        let name = r.str16()?;
        let mut fills = Vec::new();
        for _ in 0..r.u8()? {
            let kind = if r.u8()? == 1 {
                FillKind::Image
            } else {
                FillKind::Solid
            };
            let color = read_color(&mut r)?;
            let scale_mode = match r.u8()? {
                1 => ScaleMode::Fit,
                2 => ScaleMode::Tile,
                _ => ScaleMode::Fill,
            };
            let opacity = r.f64()?;
            let visible = r.u8()? != 0;
            let hash = r.str16()?;
            fills.push(Fill {
                kind,
                color,
                image_hash: (!hash.is_empty()).then_some(hash),
                scale_mode,
                opacity,
                visible,
            });
        }
        let mut strokes = Vec::new();
        for _ in 0..r.u8()? {
            let color = read_color(&mut r)?;
            let width = r.f64()?;
            let align = match r.u8()? {
                0 => StrokeAlign::Inside,
                2 => StrokeAlign::Outside,
                _ => StrokeAlign::Center,
            };
            strokes.push(Stroke {
                color,
                width,
                align,
            });
        }
        let [x, y, w, h, rotation, opacity, corner_radius] = nums;
        rows.push(SceneRow {
            id,
            tree_id,
            parent,
            props: Props {
                kind,
                name,
                x,
                y,
                w,
                h,
                rotation,
                opacity,
                visible: flags & FLAG_VISIBLE != 0,
                locked: flags & FLAG_LOCKED != 0,
                corner_radius,
                clip_content: flags & FLAG_CLIP != 0,
                fills,
                strokes,
            },
        });
    }
    Some(rows)
}
```

- [ ] **Step 6: Реализовать `PageDoc`**

Главное здесь:
- `NodeId` хранится ключом `id`, а индекс `State` сопоставляет его текущему `TreeID`. Loro выдаёт восстановленному узлу новый `TreeID`.
- Зеркало свойств (`State::props`) обновляется каждой правкой, поэтому `props()` не читает Loro.
- Каждая правка вызывает `record(Inverse::…)`. Журнал включается только внутри транзакции (`begin_journal`).

`crates/ferrum-doc/src/page.rs`:

```rust
//! Документ одной страницы: дерево узлов в собственном `LoroDoc`.
//!
//! Три решения, к которым привели бенчмарк и тесты:
//! - Стабильный `NodeId` — это `TreeID` первого создания узла, записанный ключом `id`
//!   в его свойства; индекс сопоставляет его текущему `TreeID`. Восстановленный после
//!   отмены удаления узел получает в Loro новый `TreeID`, но сохраняет свой `NodeId`.
//! - Чтение свойств из Loro стоит ~9 мкс на узел, поэтому страница держит зеркало
//!   свойств в памяти; оно заполняется из кэша сцены (или один раз из Loro) и
//!   обновляется каждой правкой.
//! - `UndoManager` Loro на большом документе строит внутренние структуры
//!   (~4.5 КБ на узел). Поэтому отмена своя: каждая правка записывает обратную
//!   операцию в журнал транзакции, и память растёт с числом правок, а не с размером
//!   документа.

use std::cell::RefCell;
use std::collections::HashMap;

use loro::{ExportMode, LoroDoc, LoroTree, LoroValue, TreeID, TreeParentId, ValueOrContainer};

use crate::codec;
use crate::error::{DocError, DocResult};
use crate::scene_cache::SceneRow;
use crate::types::*;

const TREE: &str = "nodes";
const KEY_ID: &str = "id";

/// Узел, сохранённый перед удалением, чтобы восстановить его отменой.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SavedNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub index: usize,
    pub props: Props,
}

/// Обратная операция для отмены одной правки.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Inverse {
    /// Отмена создания.
    Delete(NodeId),
    /// Отмена удаления: поддерево в порядке обхода, родители раньше детей.
    Restore(Vec<SavedNode>),
    /// Отмена изменения свойств: прежние значения изменённых полей.
    Props(NodeId, PropsPatch),
    /// Отмена перемещения.
    Move {
        id: NodeId,
        parent: Option<NodeId>,
        index: usize,
    },
}

/// Индекс стабильных ID и зеркало свойств живых узлов.
#[derive(Default)]
struct State {
    by_id: HashMap<NodeId, TreeID>,
    by_tree: HashMap<TreeID, NodeId>,
    props: HashMap<NodeId, Props>,
}

impl State {
    fn insert(&mut self, id: NodeId, tid: TreeID, props: Props) {
        self.by_id.insert(id, tid);
        self.by_tree.insert(tid, id);
        self.props.insert(id, props);
    }

    fn remove(&mut self, id: NodeId) {
        if let Some(tid) = self.by_id.remove(&id) {
            self.by_tree.remove(&tid);
        }
        self.props.remove(&id);
    }
}

pub struct PageDoc {
    doc: LoroDoc,
    tree: LoroTree,
    state: RefCell<State>,
    /// Журнал обратных операций текущей транзакции; `None` — запись выключена.
    journal: RefCell<Option<Vec<Inverse>>>,
}

impl PageDoc {
    pub fn new() -> PageDoc {
        PageDoc::wrap(LoroDoc::new())
    }

    /// Загружает страницу из снапшота, читая свойства всех узлов из Loro.
    pub fn from_snapshot(bytes: &[u8]) -> DocResult<PageDoc> {
        PageDoc::load(bytes, None)
    }

    /// Загружает страницу. С кэшем сцены (уже сверенным со снапшотом по хэшу)
    /// индекс и зеркало строятся без чтения свойств из Loro.
    pub(crate) fn load(bytes: &[u8], cache: Option<&[SceneRow]>) -> DocResult<PageDoc> {
        let page = PageDoc::wrap(LoroDoc::from_snapshot(bytes)?);
        match cache {
            Some(rows) => {
                let mut state = State::default();
                for row in rows {
                    state.insert(row.id, row.tree_id, row.props.clone());
                }
                *page.state.borrow_mut() = state;
            }
            None => page.rebuild_from_loro()?,
        }
        Ok(page)
    }

    fn wrap(doc: LoroDoc) -> PageDoc {
        let tree = doc.get_tree(TREE);
        tree.enable_fractional_index(0);
        PageDoc {
            doc,
            tree,
            state: RefCell::default(),
            journal: RefCell::default(),
        }
    }

    /// Стабильный ID узла по его текущему `TreeID`. Узел без ключа `id`
    /// (не должен встречаться) считается созданным под своим `TreeID`.
    fn stable_id(&self, tid: TreeID) -> NodeId {
        let stored = self
            .tree
            .get_meta(tid)
            .ok()
            .and_then(|m| match m.get(KEY_ID) {
                Some(ValueOrContainer::Value(LoroValue::String(s))) => s.parse::<NodeId>().ok(),
                _ => None,
            });
        stored.unwrap_or(NodeId(tid))
    }

    /// Полное построение индекса и зеркала из Loro — при загрузке без кэша.
    fn rebuild_from_loro(&self) -> DocResult<()> {
        let mut state = State::default();
        let mut stack = self.tree.children(TreeParentId::Root).unwrap_or_default();
        while let Some(tid) = stack.pop() {
            let id = self.stable_id(tid);
            let props = codec::read_props(&self.tree.get_meta(tid)?, &id)?;
            state.insert(id, tid, props);
            stack.extend(
                self.tree
                    .children(TreeParentId::Node(tid))
                    .unwrap_or_default(),
            );
        }
        *self.state.borrow_mut() = state;
        Ok(())
    }

    /// Shallow-снапшот: текущее состояние без истории правок. Меньше полного,
    /// быстрее раскодируется и на документе после правок экспортируется быстрее
    /// (история сессии в него не входит). История между сессиями не нужна:
    /// отмена работает в пределах сессии, версии файла появятся с сервером.
    pub fn export_snapshot(&self) -> DocResult<Vec<u8>> {
        let frontiers = self.doc.oplog_frontiers();
        if frontiers.is_empty() {
            return Ok(self.doc.export(ExportMode::Snapshot)?);
        }
        Ok(self.doc.export(ExportMode::shallow_snapshot(&frontiers))?)
    }

    pub(crate) fn commit(&self) {
        self.doc.commit();
    }

    #[doc(hidden)]
    pub fn commit_for_tests(&self) {
        self.doc.commit();
    }

    pub(crate) fn begin_journal(&self) {
        *self.journal.borrow_mut() = Some(Vec::new());
    }

    pub(crate) fn take_journal(&self) -> Vec<Inverse> {
        self.journal.borrow_mut().take().unwrap_or_default()
    }

    fn record(&self, op: Inverse) {
        if let Some(journal) = self.journal.borrow_mut().as_mut() {
            journal.push(op);
        }
    }

    /// Применяет обратные операции в обратном порядке и возвращает операции,
    /// которые отменят это применение (для redo).
    pub(crate) fn apply_inverse(&self, ops: &[Inverse]) -> DocResult<Vec<Inverse>> {
        self.begin_journal();
        let result = ops.iter().rev().try_for_each(|op| match op {
            Inverse::Delete(id) => self.delete_nodes(&[*id]),
            Inverse::Restore(nodes) => nodes.iter().try_for_each(|n| {
                self.create_with(n.id, n.parent, Some(n.index), n.props.clone())
                    .map(|_| ())
            }),
            Inverse::Props(id, patch) => self.set_props(*id, patch),
            Inverse::Move { id, parent, index } => self.move_node(*id, *parent, *index),
        });
        let redo = self.take_journal();
        self.commit();
        result.map(|_| redo)
    }

    fn tid(&self, id: NodeId) -> DocResult<TreeID> {
        let tid = self.state.borrow().by_id.get(&id).copied();
        match tid {
            Some(t) if !self.tree.is_node_deleted(&t).unwrap_or(true) => Ok(t),
            _ => Err(DocError::NodeNotFound(id.to_string())),
        }
    }

    fn parent_tid(&self, parent: Option<NodeId>) -> DocResult<TreeParentId> {
        Ok(match parent {
            Some(p) => TreeParentId::Node(self.tid(p)?),
            None => TreeParentId::Root,
        })
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.tid(id).is_ok()
    }

    /// Создаёт узел со свойствами по умолчанию. `index = None` — поверх всех детей.
    pub fn create_node(
        &self,
        parent: Option<NodeId>,
        index: Option<usize>,
        kind: NodeKind,
    ) -> DocResult<NodeId> {
        let tid = self.create_tree_node(parent, index)?;
        let id = NodeId(tid);
        self.finish_create(id, tid, Props::defaults(kind))?;
        Ok(id)
    }

    /// Восстанавливает узел под прежним стабильным ID (отмена удаления).
    fn create_with(
        &self,
        id: NodeId,
        parent: Option<NodeId>,
        index: Option<usize>,
        props: Props,
    ) -> DocResult<NodeId> {
        let tid = self.create_tree_node(parent, index)?;
        self.finish_create(id, tid, props)?;
        Ok(id)
    }

    fn create_tree_node(&self, parent: Option<NodeId>, index: Option<usize>) -> DocResult<TreeID> {
        let parent = self.parent_tid(parent)?;
        Ok(match index {
            Some(i) => self.tree.create_at(parent, i)?,
            None => self.tree.create(parent)?,
        })
    }

    fn finish_create(&self, id: NodeId, tid: TreeID, props: Props) -> DocResult<()> {
        let meta = self.tree.get_meta(tid)?;
        meta.insert(KEY_ID, id.to_string())?;
        codec::write_props(&meta, &props)?;
        self.state.borrow_mut().insert(id, tid, props);
        self.record(Inverse::Delete(id));
        Ok(())
    }

    pub fn props(&self, id: NodeId) -> DocResult<Props> {
        self.tid(id)?;
        self.state
            .borrow()
            .props
            .get(&id)
            .cloned()
            .ok_or_else(|| DocError::NodeNotFound(id.to_string()))
    }

    pub fn kind(&self, id: NodeId) -> DocResult<NodeKind> {
        Ok(self.props(id)?.kind)
    }

    pub fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        let tid = self.tid(id)?;
        let before = self.props(id)?;
        codec::write_patch(&self.tree.get_meta(tid)?, patch)?;
        if let Some(props) = self.state.borrow_mut().props.get_mut(&id) {
            patch.apply_to(props);
        }
        self.record(Inverse::Props(id, patch.previous(&before)));
        Ok(())
    }

    /// Позиция узла среди детей его родителя.
    fn index_of(&self, id: NodeId, parent: Option<NodeId>) -> usize {
        self.children(parent)
            .iter()
            .position(|c| *c == id)
            .unwrap_or(0)
    }

    /// Переносит узел к новому родителю на позицию `index` среди его детей.
    pub fn move_node(&self, id: NodeId, parent: Option<NodeId>, index: usize) -> DocResult<()> {
        let tid = self.tid(id)?;
        if let Some(p) = parent {
            if p == id || self.is_ancestor(id, p) {
                return Err(DocError::CyclicMove);
            }
        }
        let old_parent = self.parent(id);
        let old_index = self.index_of(id, old_parent);
        self.tree.mov_to(tid, self.parent_tid(parent)?, index)?;
        self.record(Inverse::Move {
            id,
            parent: old_parent,
            index: old_index,
        });
        Ok(())
    }

    fn is_ancestor(&self, ancestor: NodeId, mut node: NodeId) -> bool {
        while let Some(p) = self.parent(node) {
            if p == ancestor {
                return true;
            }
            node = p;
        }
        false
    }

    /// Удаляет узлы вместе с потомками. Уже удалённые пропускаются.
    pub fn delete_nodes(&self, ids: &[NodeId]) -> DocResult<()> {
        for id in ids {
            let Ok(tid) = self.tid(*id) else { continue };
            let parent = self.parent(*id);
            let mut saved = vec![SavedNode {
                id: *id,
                parent,
                index: self.index_of(*id, parent),
                props: self.props(*id)?,
            }];
            for node in self.walk_from(Some(*id)) {
                let parent = self.parent(node);
                saved.push(SavedNode {
                    id: node,
                    parent,
                    index: self.index_of(node, parent),
                    props: self.props(node)?,
                });
            }
            self.tree.delete(tid)?;
            let mut state = self.state.borrow_mut();
            for node in &saved {
                state.remove(node.id);
            }
            drop(state);
            self.record(Inverse::Restore(saved));
        }
        Ok(())
    }

    /// Дети снизу вверх (последний рисуется поверх). `None` — корень страницы.
    pub fn children(&self, parent: Option<NodeId>) -> Vec<NodeId> {
        let Ok(parent) = self.parent_tid(parent) else {
            return Vec::new();
        };
        let state = self.state.borrow();
        self.tree
            .children(parent)
            .unwrap_or_default()
            .into_iter()
            .map(|t| state.by_tree.get(&t).copied().unwrap_or(NodeId(t)))
            .collect()
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        match self.tree.parent(self.tid(id).ok()?)? {
            TreeParentId::Node(p) => Some(
                self.state
                    .borrow()
                    .by_tree
                    .get(&p)
                    .copied()
                    .unwrap_or(NodeId(p)),
            ),
            _ => None,
        }
    }

    /// Все живые узлы страницы в порядке обхода в глубину.
    pub fn walk(&self) -> Vec<NodeId> {
        self.walk_from(None)
    }

    /// Потомки узла (без него самого) в порядке обхода в глубину.
    fn walk_from(&self, root: Option<NodeId>) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.children(root).into_iter().rev().collect();
        while let Some(id) = stack.pop() {
            out.push(id);
            stack.extend(self.children(Some(id)).into_iter().rev());
        }
        out
    }

    /// Таблица всех узлов для кэша сцены: обход в глубину, родитель раньше детей.
    pub fn scene_rows(&self) -> DocResult<Vec<SceneRow>> {
        let mut rows = Vec::new();
        let mut stack: Vec<(NodeId, Option<u32>)> = self
            .children(None)
            .into_iter()
            .rev()
            .map(|id| (id, None))
            .collect();
        while let Some((id, parent)) = stack.pop() {
            let row = rows.len() as u32;
            rows.push(SceneRow {
                id,
                tree_id: self.tid(id)?,
                parent,
                props: self.props(id)?,
            });
            stack.extend(
                self.children(Some(id))
                    .into_iter()
                    .rev()
                    .map(|c| (c, Some(row))),
            );
        }
        Ok(rows)
    }

    /// Хэши изображений, на которые ссылаются заливки живых узлов страницы.
    pub fn image_refs(&self) -> DocResult<Vec<String>> {
        let state = self.state.borrow();
        let mut refs: Vec<String> = state
            .props
            .values()
            .flat_map(|p| p.fills.iter().filter_map(|f| f.image_hash.clone()))
            .collect();
        refs.sort();
        refs.dedup();
        Ok(refs)
    }
}

impl Default for PageDoc {
    fn default() -> Self {
        PageDoc::new()
    }
}
```

- [ ] **Step 7: Запустить тесты**

Run: `cargo test -p ferrum-doc --test page --test types`
Expected: PASS, `10 passed` и `5 passed`. Предупреждения о неиспользуемых `begin_journal`, `take_journal`, `apply_inverse`, `Reader::at` уйдут в Task 3–4.

- [ ] **Step 8: Закоммитить**

```bash
git add crates/ferrum-doc
git commit -m "feat(doc): страница с узлами, стабильными ID и зеркалом свойств"
```

---

### Task 3: Документ: страницы, транзакции, undo/redo

**Files:**
- Create: `crates/ferrum-doc/src/history.rs`
- Create: `crates/ferrum-doc/src/images.rs`
- Create: `crates/ferrum-doc/src/document.rs`
- Modify: `crates/ferrum-doc/src/lib.rs`
- Test: `crates/ferrum-doc/tests/document.rs`

**Interfaces:**
- Consumes: `PageDoc`, `Inverse`, `SceneRow` из Task 2.
- Produces: `Document` с `new()`, `name()`, `set_name(&str)`, `schema_version() -> u32`, `is_read_only()`, `pages() -> Vec<(PageId, String)>`, `add_page(&str) -> DocResult<PageId>`, `rename_page`, `delete_page`, `page(PageId) -> DocResult<&PageDoc>` (ленивая загрузка), `is_page_loaded`, `cached_scene(PageId) -> Option<&[SceneRow]>`, `transact(page, selection_before: Vec<NodeId>, f: FnOnce(&PageDoc) -> DocResult<Vec<NodeId>>) -> DocResult<Vec<NodeId>>`, `undo()/redo() -> DocResult<Option<Restore>>`, `can_undo()/can_redo()`, публичное поле `images: ImageStore`; для Task 4: `pub(crate) from_parts(root_snapshot, HashMap<PageId, UnloadedPage>, ImageStore) -> DocResult<(Document, Vec<String>)>`, `root_snapshot()`, `page_payload(PageId) -> DocResult<PagePayload>`, `force_schema_version(u32)`, структуры `UnloadedPage { snapshot, image_refs, scene }` и `PagePayload { snapshot, image_refs, scene: Option<Vec<SceneRow>> }`. `Restore { page: PageId, selection: Vec<NodeId> }`; `SCHEMA_VERSION = 1`; `ImageStore` с `add(Vec<u8>) -> String`, `get(&str)`, `len()`, `is_empty()`.

- [ ] **Step 1: Написать падающие тесты документа**

`crates/ferrum-doc/tests/document.rs`:

```rust
use ferrum_doc::*;

fn first_page(doc: &Document) -> PageId {
    doc.pages()[0].0
}

#[test]
fn new_document_has_one_page() {
    let doc = Document::new();
    let pages = doc.pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].1, "Страница 1");
    assert_eq!(doc.schema_version(), SCHEMA_VERSION);
}

#[test]
fn pages_can_be_added_renamed_deleted() {
    let mut doc = Document::new();
    let p2 = doc.add_page("Черновики").unwrap();
    doc.rename_page(p2, "Идеи").unwrap();
    assert_eq!(doc.pages()[1], (p2, "Идеи".to_string()));
    doc.delete_page(p2).unwrap();
    assert_eq!(doc.pages().len(), 1);
    assert!(matches!(doc.page(p2), Err(DocError::PageNotFound(_))));
}

#[test]
fn transaction_is_one_undo_step_and_restores_selection() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    let mut created = None;
    let sel = doc
        .transact(page, vec![], |p| {
            let id = p.create_node(None, None, NodeKind::Rect)?;
            p.set_props(
                id,
                &PropsPatch {
                    x: Some(10.0),
                    ..Default::default()
                },
            )?;
            created = Some(id);
            Ok(vec![id])
        })
        .unwrap();
    let id = created.unwrap();
    assert_eq!(sel, vec![id]);

    let restore = doc.undo().unwrap().unwrap();
    assert_eq!(
        restore,
        Restore {
            page,
            selection: vec![]
        }
    );
    assert!(doc.page(page).unwrap().children(None).is_empty());

    let restore = doc.redo().unwrap().unwrap();
    assert_eq!(restore.selection, vec![id]);
    assert_eq!(doc.page(page).unwrap().props(id).unwrap().x, 10.0);
}

#[test]
fn undo_follows_user_order_across_pages() {
    let mut doc = Document::new();
    let p1 = first_page(&doc);
    let p2 = doc.add_page("Вторая").unwrap();
    doc.transact(p1, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    doc.transact(p2, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Ellipse)?])
    })
    .unwrap();
    doc.transact(p1, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Line)?])
    })
    .unwrap();

    assert_eq!(doc.undo().unwrap().unwrap().page, p1);
    assert_eq!(doc.page(p1).unwrap().children(None).len(), 1);
    assert_eq!(doc.undo().unwrap().unwrap().page, p2);
    assert!(doc.page(p2).unwrap().children(None).is_empty());
    assert_eq!(doc.undo().unwrap().unwrap().page, p1);
    assert!(doc.page(p1).unwrap().children(None).is_empty());
    assert!(doc.undo().unwrap().is_none());
}

#[test]
fn new_change_clears_redo() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    doc.transact(page, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    doc.undo().unwrap();
    assert!(doc.can_redo());
    doc.transact(page, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Ellipse)?])
    })
    .unwrap();
    assert!(!doc.can_redo());
}

#[test]
fn empty_transaction_adds_no_history() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    doc.transact(page, vec![], |_| Ok(vec![])).unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn failed_transaction_rolls_back() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    let err = doc.transact(page, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        p.move_node(f, Some(f), 0)?;
        Ok(vec![f])
    });
    assert!(matches!(err, Err(DocError::CyclicMove)));
    assert!(doc.page(page).unwrap().children(None).is_empty());
    assert!(!doc.can_undo());
}

#[test]
fn deleting_page_forgets_its_history() {
    let mut doc = Document::new();
    let p2 = doc.add_page("Вторая").unwrap();
    doc.transact(p2, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    doc.delete_page(p2).unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn images_are_deduplicated_by_hash() {
    let mut doc = Document::new();
    let a = doc.images.add(vec![1, 2, 3]);
    let b = doc.images.add(vec![1, 2, 3]);
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
    assert_eq!(doc.images.len(), 1);
    assert_eq!(doc.images.get(&a), Some(&[1u8, 2, 3][..]));
}

#[test]
fn node_ids_survive_undo_of_delete_with_subtree_and_fills() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    let (mut frame, mut child) = (None, None);
    doc.transact(page, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        let c = p.create_node(Some(f), None, NodeKind::Rect)?;
        p.set_props(
            c,
            &PropsPatch {
                fills: Some(vec![Fill::solid(Color::BLACK)]),
                ..Default::default()
            },
        )?;
        frame = Some(f);
        child = Some(c);
        Ok(vec![f])
    })
    .unwrap();
    let (frame, child) = (frame.unwrap(), child.unwrap());
    doc.transact(page, vec![frame], |p| {
        p.delete_nodes(&[frame])?;
        Ok(vec![])
    })
    .unwrap();

    let restore = doc.undo().unwrap().unwrap();
    assert_eq!(restore.selection, vec![frame]);
    let p = doc.page(page).unwrap();
    assert!(p.contains(frame) && p.contains(child));
    assert_eq!(p.children(Some(frame)), vec![child]);
    assert_eq!(p.parent(child), Some(frame));
    assert_eq!(
        p.props(child).unwrap().fills,
        vec![Fill::solid(Color::BLACK)]
    );
    assert_eq!(p.walk(), vec![frame, child]);
}

#[test]
fn node_ids_survive_reload_after_undo() {
    let mut doc = Document::new();
    let page = first_page(&doc);
    let id = doc
        .transact(page, vec![], |p| {
            Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    doc.undo().unwrap();
    doc.redo().unwrap();
    let snapshot = doc.page(page).unwrap().export_snapshot().unwrap();
    let reloaded = PageDoc::from_snapshot(&snapshot).unwrap();
    assert!(reloaded.contains(id));
    assert_eq!(reloaded.walk(), vec![id]);
}
```

- [ ] **Step 2: Подключить модули в `lib.rs`**

`crates/ferrum-doc/src/lib.rs`:

```rust
//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod binary;
mod codec;
mod document;
mod error;
mod history;
mod images;
mod page;
mod scene_cache;
mod types;

pub use document::{Document, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use images::ImageStore;
pub use page::PageDoc;
pub use scene_cache::SceneRow;
pub use types::*;
```

- [ ] **Step 3: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-doc --test document`
Expected: FAIL — `file not found for module` (`document`, `history`, `images`).

- [ ] **Step 4: Реализовать историю и хранилище изображений**

`crates/ferrum-doc/src/history.rs`:

```rust
//! Глобальный стек отмены для всех страниц. Шаг хранит обратные операции
//! своей транзакции и выделение до и после неё.

use crate::page::Inverse;
use crate::types::{NodeId, PageId};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HistoryEntry {
    pub page: PageId,
    /// Для шага в стеке undo — операции отмены; в стеке redo — операции повтора.
    pub ops: Vec<Inverse>,
    pub selection_before: Vec<NodeId>,
    pub selection_after: Vec<NodeId>,
}

#[derive(Default)]
pub(crate) struct History {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl History {
    /// Новый шаг пользователя сбрасывает возможность повтора.
    pub fn push(&mut self, entry: HistoryEntry) {
        self.undo.push(entry);
        self.redo.clear();
    }

    pub fn push_redo(&mut self, entry: HistoryEntry) {
        self.redo.push(entry);
    }

    pub fn push_undo_keep_redo(&mut self, entry: HistoryEntry) {
        self.undo.push(entry);
    }

    pub fn pop_undo(&mut self) -> Option<HistoryEntry> {
        self.undo.pop()
    }

    pub fn pop_redo(&mut self) -> Option<HistoryEntry> {
        self.redo.pop()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn forget_page(&mut self, page: PageId) {
        self.undo.retain(|e| e.page != page);
        self.redo.retain(|e| e.page != page);
    }
}
```

`crates/ferrum-doc/src/images.rs`:

```rust
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
```

- [ ] **Step 5: Реализовать `Document`**

`transact` включает журнал страницы, запускает изменения и кладёт обратные операции в шаг истории. При ошибке те же операции откатывают изменения. `undo`/`redo` применяют операции шага и сохраняют операции для обратного направления.

`crates/ferrum-doc/src/document.rs`:

```rust
//! Документ целиком: корневой `LoroDoc` (метаданные, список страниц) и по одному
//! `PageDoc` на страницу. Страницы загружаются по требованию.

use std::collections::HashMap;

use loro::{LoroDoc, LoroTree, LoroValue, TreeParentId, ValueOrContainer};

use crate::error::{DocError, DocResult};
use crate::history::{History, HistoryEntry};
use crate::images::ImageStore;
use crate::page::PageDoc;
use crate::scene_cache::SceneRow;
use crate::types::{NodeId, PageId};

/// Текущая версия схемы документа. Растёт при несовместимых изменениях схемы.
pub const SCHEMA_VERSION: u32 = 1;

const META: &str = "meta";
const PAGES: &str = "pages";
const KEY_SCHEMA: &str = "schemaVersion";
const KEY_NAME: &str = "name";

/// Страница, ещё не загруженная из файла.
pub(crate) struct UnloadedPage {
    pub snapshot: Vec<u8>,
    pub image_refs: Vec<String>,
    /// Кэш сцены из файла, если он совпал со снапшотом.
    pub scene: Option<Vec<SceneRow>>,
}

/// Всё, что пишется в файл для одной страницы.
pub(crate) struct PagePayload {
    pub snapshot: Vec<u8>,
    pub image_refs: Vec<String>,
    /// `None` — кэша нет (страница не открывалась); секция кэша тогда не пишется.
    pub scene: Option<Vec<SceneRow>>,
}

/// Что восстановить в интерфейсе после undo/redo.
#[derive(Debug, Clone, PartialEq)]
pub struct Restore {
    pub page: PageId,
    pub selection: Vec<NodeId>,
}

pub struct Document {
    root: LoroDoc,
    pages_tree: LoroTree,
    loaded: HashMap<PageId, PageDoc>,
    unloaded: HashMap<PageId, UnloadedPage>,
    history: History,
    pub images: ImageStore,
    read_only: bool,
}

impl Document {
    pub fn new() -> Document {
        let root = LoroDoc::new();
        let meta = root.get_map(META);
        meta.insert(KEY_SCHEMA, SCHEMA_VERSION as i64)
            .expect("новый документ");
        meta.insert(KEY_NAME, "Без названия")
            .expect("новый документ");
        let mut doc = Document::wrap(root, HashMap::new(), ImageStore::default());
        doc.add_page("Страница 1").expect("новый документ");
        doc
    }

    fn wrap(
        root: LoroDoc,
        unloaded: HashMap<PageId, UnloadedPage>,
        images: ImageStore,
    ) -> Document {
        let pages_tree = root.get_tree(PAGES);
        pages_tree.enable_fractional_index(0);
        Document {
            root,
            pages_tree,
            loaded: HashMap::new(),
            unloaded,
            history: History::default(),
            images,
            read_only: false,
        }
    }

    /// Собирает документ из прочитанного файла. Возвращает предупреждения.
    pub(crate) fn from_parts(
        root_snapshot: &[u8],
        unloaded: HashMap<PageId, UnloadedPage>,
        images: ImageStore,
    ) -> DocResult<(Document, Vec<String>)> {
        let root = LoroDoc::from_snapshot(root_snapshot)?;
        let mut doc = Document::wrap(root, unloaded, images);
        let mut warnings = Vec::new();
        let version = doc.schema_version();
        if version > SCHEMA_VERSION {
            doc.read_only = true;
            warnings.push(format!(
                "Файл создан в более новой версии Ferrum (схема {version}). Он открыт только для чтения"
            ));
        }
        for (id, _) in doc.pages() {
            if !doc.unloaded.contains_key(&id) {
                warnings.push(format!("Страница {id} повреждена и открыта пустой"));
                doc.loaded.insert(id, PageDoc::new());
            }
        }
        Ok((doc, warnings))
    }

    pub(crate) fn root_snapshot(&self) -> DocResult<Vec<u8>> {
        Ok(self.root.export(loro::ExportMode::Snapshot)?)
    }

    pub fn schema_version(&self) -> u32 {
        match self.root.get_map(META).get(KEY_SCHEMA) {
            Some(ValueOrContainer::Value(LoroValue::I64(v))) => v as u32,
            _ => SCHEMA_VERSION,
        }
    }

    pub(crate) fn force_schema_version(&mut self, version: u32) {
        let meta = self.root.get_map(META);
        meta.insert(KEY_SCHEMA, version as i64)
            .expect("запись в корневой документ");
        self.root.commit();
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    fn writable(&self) -> DocResult<()> {
        if self.read_only {
            Err(DocError::ReadOnly)
        } else {
            Ok(())
        }
    }

    pub fn name(&self) -> String {
        match self.root.get_map(META).get(KEY_NAME) {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => s.to_string(),
            _ => String::new(),
        }
    }

    pub fn set_name(&mut self, name: &str) -> DocResult<()> {
        self.writable()?;
        self.root.get_map(META).insert(KEY_NAME, name)?;
        self.root.commit();
        Ok(())
    }

    /// Страницы в порядке списка.
    pub fn pages(&self) -> Vec<(PageId, String)> {
        self.pages_tree
            .children(TreeParentId::Root)
            .unwrap_or_default()
            .into_iter()
            .map(|t| {
                let name = match self
                    .pages_tree
                    .get_meta(t)
                    .ok()
                    .and_then(|m| m.get(KEY_NAME))
                {
                    Some(ValueOrContainer::Value(LoroValue::String(s))) => s.to_string(),
                    _ => String::new(),
                };
                (PageId(t), name)
            })
            .collect()
    }

    /// Операции со страницами в первой версии не попадают в undo.
    pub fn add_page(&mut self, name: &str) -> DocResult<PageId> {
        self.writable()?;
        let t = self.pages_tree.create(TreeParentId::Root)?;
        self.pages_tree.get_meta(t)?.insert(KEY_NAME, name)?;
        self.root.commit();
        let id = PageId(t);
        self.loaded.insert(id, PageDoc::new());
        Ok(id)
    }

    pub fn rename_page(&mut self, id: PageId, name: &str) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        self.pages_tree.get_meta(id.0)?.insert(KEY_NAME, name)?;
        self.root.commit();
        Ok(())
    }

    pub fn delete_page(&mut self, id: PageId) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        self.pages_tree.delete(id.0)?;
        self.root.commit();
        self.loaded.remove(&id);
        self.unloaded.remove(&id);
        self.history.forget_page(id);
        Ok(())
    }

    fn ensure_page(&self, id: PageId) -> DocResult<()> {
        if self.loaded.contains_key(&id) || self.unloaded.contains_key(&id) {
            Ok(())
        } else {
            Err(DocError::PageNotFound(id.to_string()))
        }
    }

    pub fn is_page_loaded(&self, id: PageId) -> bool {
        self.loaded.contains_key(&id)
    }

    /// Загружает страницу при первом обращении.
    pub fn page(&mut self, id: PageId) -> DocResult<&PageDoc> {
        if let Some(u) = self.unloaded.remove(&id) {
            self.loaded
                .insert(id, PageDoc::load(&u.snapshot, u.scene.as_deref())?);
        }
        self.loaded
            .get(&id)
            .ok_or_else(|| DocError::PageNotFound(id.to_string()))
    }

    /// Кэш сцены незагруженной страницы: по нему холст рисуется до загрузки Loro.
    pub fn cached_scene(&self, id: PageId) -> Option<&[SceneRow]> {
        self.unloaded.get(&id)?.scene.as_deref()
    }

    /// Данные страницы для файла — без загрузки, если она не открыта.
    pub(crate) fn page_payload(&self, id: PageId) -> DocResult<PagePayload> {
        if let Some(p) = self.loaded.get(&id) {
            return Ok(PagePayload {
                snapshot: p.export_snapshot()?,
                image_refs: p.image_refs()?,
                scene: Some(p.scene_rows()?),
            });
        }
        let u = self
            .unloaded
            .get(&id)
            .ok_or_else(|| DocError::PageNotFound(id.to_string()))?;
        Ok(PagePayload {
            snapshot: u.snapshot.clone(),
            image_refs: u.image_refs.clone(),
            scene: u.scene.clone(),
        })
    }

    /// Выполняет изменения страницы как один шаг undo.
    /// `f` возвращает выделение после изменения. При ошибке изменения откатываются.
    pub fn transact<F>(
        &mut self,
        page: PageId,
        selection_before: Vec<NodeId>,
        f: F,
    ) -> DocResult<Vec<NodeId>>
    where
        F: FnOnce(&PageDoc) -> DocResult<Vec<NodeId>>,
    {
        self.writable()?;
        self.page(page)?;
        let p = self.loaded.get(&page).expect("страница загружена выше");
        p.begin_journal();
        let result = f(p);
        let ops = p.take_journal();
        p.commit();
        match result {
            Ok(selection_after) => {
                if !ops.is_empty() {
                    self.history.push(HistoryEntry {
                        page,
                        ops,
                        selection_before,
                        selection_after: selection_after.clone(),
                    });
                }
                Ok(selection_after)
            }
            Err(e) => {
                // Откат: применяем обратные операции, в историю ничего не попадает.
                p.apply_inverse(&ops)?;
                Err(e)
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Применяет обратные операции шага и возвращает шаг для обратного направления.
    fn replay(&mut self, mut entry: HistoryEntry) -> DocResult<HistoryEntry> {
        self.page(entry.page)?;
        let p = self.loaded.get(&entry.page).expect("загружена");
        entry.ops = p.apply_inverse(&entry.ops)?;
        Ok(entry)
    }

    pub fn undo(&mut self) -> DocResult<Option<Restore>> {
        let Some(entry) = self.history.pop_undo() else {
            return Ok(None);
        };
        let entry = self.replay(entry)?;
        let restore = Restore {
            page: entry.page,
            selection: entry.selection_before.clone(),
        };
        self.history.push_redo(entry);
        Ok(Some(restore))
    }

    pub fn redo(&mut self) -> DocResult<Option<Restore>> {
        let Some(entry) = self.history.pop_redo() else {
            return Ok(None);
        };
        let entry = self.replay(entry)?;
        let restore = Restore {
            page: entry.page,
            selection: entry.selection_after.clone(),
        };
        self.history.push_undo_keep_redo(entry);
        Ok(Some(restore))
    }
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}
```

- [ ] **Step 6: Запустить тесты**

Run: `cargo test -p ferrum-doc`
Expected: PASS — `types` 5, `page` 10, `document` 11.

- [ ] **Step 7: Закоммитить**

```bash
git add crates/ferrum-doc
git commit -m "feat(doc): документ со страницами, транзакциями и своей историей отмены"
```

---

### Task 4: Формат файла `.ferrum` с кэшем сцены

**Files:**
- Create: `crates/ferrum-doc/src/container.rs`
- Modify: `crates/ferrum-doc/src/lib.rs`
- Test: `crates/ferrum-doc/tests/container.rs`

**Interfaces:**
- Consumes: `Document::{from_parts, root_snapshot, page_payload, force_schema_version, pages}`, `UnloadedPage`, `ImageStore::insert_verified`, `scene_cache::{encode, decode}`, `binary::Reader` из Task 2–3.
- Produces: `container::encode(&Document) -> DocResult<Vec<u8>>`, `container::decode(&[u8]) -> DocResult<Opened>`, `Opened { document: Document, warnings: Vec<String> }`, константы `MAGIC`, `FORMAT_VERSION = 1`; для тестов `container::testing::{with_schema_version(&[u8], u32) -> Vec<u8>, section_data_offset(&[u8], kind: u8) -> Option<usize>}`.

- [ ] **Step 1: Написать падающие тесты формата**

`crates/ferrum-doc/tests/container.rs`:

```rust
use ferrum_doc::container::{decode, encode, FORMAT_VERSION, MAGIC};
use ferrum_doc::*;

fn sample() -> (Document, PageId, PageId, NodeId, String) {
    let mut doc = Document::new();
    doc.set_name("Лендинг кофейни").unwrap();
    let p1 = doc.pages()[0].0;
    let p2 = doc.add_page("Черновики").unwrap();
    let hash = doc.images.add(vec![9; 1000]);
    let _orphan = doc.images.add(vec![7; 10]);
    let h = hash.clone();
    let mut node = None;
    doc.transact(p1, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        p.set_props(
            f,
            &PropsPatch {
                w: Some(393.0),
                h: Some(852.0),
                ..Default::default()
            },
        )?;
        let img = p.create_node(Some(f), None, NodeKind::Image)?;
        p.set_props(
            img,
            &PropsPatch {
                fills: Some(vec![Fill::image(h)]),
                ..Default::default()
            },
        )?;
        node = Some(f);
        Ok(vec![f])
    })
    .unwrap();
    (doc, p1, p2, node.unwrap(), hash)
}

#[test]
fn roundtrip_keeps_pages_nodes_images() {
    let (doc, p1, p2, frame, hash) = sample();
    let bytes = encode(&doc).unwrap();
    assert_eq!(&bytes[..8], MAGIC);
    let mut opened = decode(&bytes).unwrap();
    assert!(opened.warnings.is_empty(), "{:?}", opened.warnings);
    let d = &mut opened.document;
    assert_eq!(d.name(), "Лендинг кофейни");
    assert_eq!(
        d.pages().iter().map(|p| p.0).collect::<Vec<_>>(),
        vec![p1, p2]
    );
    assert!(!d.is_page_loaded(p1), "страницы грузятся лениво");
    assert_eq!(d.page(p1).unwrap().props(frame).unwrap().w, 393.0);
    assert!(d.is_page_loaded(p1));
    assert_eq!(d.images.get(&hash).unwrap().len(), 1000);
}

#[test]
fn unreferenced_images_are_not_saved() {
    let (doc, ..) = sample();
    let opened = decode(&encode(&doc).unwrap()).unwrap();
    assert_eq!(opened.document.images.len(), 1);
}

#[test]
fn unloaded_pages_survive_resave() {
    let (doc, p1, _, frame, hash) = sample();
    let once = decode(&encode(&doc).unwrap()).unwrap().document;
    let mut twice = decode(&encode(&once).unwrap()).unwrap().document;
    assert_eq!(twice.page(p1).unwrap().props(frame).unwrap().h, 852.0);
    assert!(
        twice.images.get(&hash).is_some(),
        "ссылки незагруженной страницы сохраняются"
    );
}

#[test]
fn rejects_non_ferrum_and_future_format() {
    assert!(matches!(
        decode(b"PNG garbage here...."),
        Err(DocError::NotAFerrumFile)
    ));
    let mut bytes = encode(&Document::new()).unwrap();
    bytes[8..12].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
    assert!(matches!(
        decode(&bytes),
        Err(DocError::UnsupportedFormat(_))
    ));
}

#[test]
fn corrupted_image_is_skipped_with_warning() {
    let (doc, ..) = sample();
    let mut bytes = encode(&doc).unwrap();
    let last = bytes.len() - 1; // последняя секция — изображение
    bytes[last] ^= 0xff;
    let opened = decode(&bytes).unwrap();
    assert_eq!(opened.document.images.len(), 0);
    assert!(
        opened
            .warnings
            .iter()
            .any(|w| w.contains("изображений — 1")),
        "{:?}",
        opened.warnings
    );
}

#[test]
fn truncated_file_opens_what_it_can() {
    let (doc, p1, ..) = sample();
    let bytes = encode(&doc).unwrap();
    let cut = &bytes[..bytes.len() - 10];
    let mut opened = decode(cut).unwrap();
    assert!(opened.warnings.iter().any(|w| w.contains("обрезан")));
    assert_eq!(opened.document.page(p1).unwrap().children(None).len(), 1);
}

#[test]
fn newer_schema_opens_read_only() {
    let mut doc = Document::new();
    doc.set_name("x").unwrap();
    let bytes = encode(&doc).unwrap();
    // Подменяем версию схемы через публичный API нельзя — собираем документ из будущего вручную.
    let future = ferrum_doc::container::testing::with_schema_version(&bytes, SCHEMA_VERSION + 1);
    let mut opened = decode(&future).unwrap();
    assert!(opened.document.is_read_only());
    assert!(opened
        .warnings
        .iter()
        .any(|w| w.contains("только для чтения")));
    let page = opened.document.pages()[0].0;
    let err = opened.document.transact(page, vec![], |p| {
        Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
    });
    assert!(matches!(err, Err(DocError::ReadOnly)));
}

#[test]
fn scene_cache_is_available_before_page_loads() {
    let (doc, p1, _, frame, hash) = sample();
    let mut opened = decode(&encode(&doc).unwrap()).unwrap().document;
    let rows = opened.cached_scene(p1).expect("кэш сцены записан");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, frame);
    assert_eq!(rows[0].parent, None);
    assert_eq!(rows[0].props.w, 393.0);
    assert_eq!(rows[1].parent, Some(0));
    assert_eq!(
        rows[1].props.fills[0].image_hash.as_deref(),
        Some(hash.as_str())
    );
    // Кэш совпадает с тем, что даёт загруженный Loro.
    let expected = opened.page(p1).unwrap().scene_rows().unwrap();
    let again = decode(&encode(&doc).unwrap()).unwrap().document;
    assert_eq!(again.cached_scene(p1).unwrap(), expected.as_slice());
}

#[test]
fn stale_or_broken_scene_cache_is_ignored() {
    let (doc, p1, ..) = sample();
    let mut bytes = encode(&doc).unwrap();
    // Портим первый байт таблицы кэша: ищем секцию типа 5 и меняем данные после хэша.
    let pos = ferrum_doc::container::testing::section_data_offset(&bytes, 5).unwrap();
    bytes[pos] ^= 0xff;
    let mut opened = decode(&bytes).unwrap();
    assert!(opened.document.cached_scene(p1).is_none());
    assert!(
        opened.warnings.is_empty(),
        "кэш необязателен — без предупреждений"
    );
    assert_eq!(opened.document.page(p1).unwrap().children(None).len(), 1);
}

#[test]
fn opened_file_has_clean_undo_history() {
    let (doc, p1, _, frame, _) = sample();
    let mut opened = decode(&encode(&doc).unwrap()).unwrap().document;
    assert!(!opened.can_undo(), "прогрев не попадает в историю");
    opened
        .transact(p1, vec![], |p| {
            p.set_props(
                frame,
                &PropsPatch {
                    x: Some(50.0),
                    ..Default::default()
                },
            )?;
            Ok(vec![frame])
        })
        .unwrap();
    opened.undo().unwrap();
    assert!(opened.undo().unwrap().is_none());
    assert_eq!(opened.page(p1).unwrap().props(frame).unwrap().x, 0.0);
}
```

- [ ] **Step 2: Подключить модуль в `lib.rs`**

`crates/ferrum-doc/src/lib.rs`:

```rust
//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod binary;
mod codec;
pub mod container;
mod document;
mod error;
mod history;
mod images;
mod page;
mod scene_cache;
mod types;

pub use document::{Document, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use images::ImageStore;
pub use page::PageDoc;
pub use scene_cache::SceneRow;
pub use types::*;
```

- [ ] **Step 3: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-doc --test container`
Expected: FAIL — `file not found for module container`.

- [ ] **Step 4: Реализовать контейнер**

Каждая секция несёт blake3 своих данных: повреждённая секция пропускается, остальное открывается. Кэш сцены годен, только если его хэш совпадает с хэшем снапшота страницы.

`crates/ferrum-doc/src/container.rs`:

````rust
//! Формат файла `.ferrum`.
//!
//! ```text
//! magic "FERRUM\0\0" (8 байт) · версия формата u32 LE · число секций u32 LE
//! секция: тип u8 · длина id u32 · id (UTF-8) · длина данных u64 · blake3(данные) 32 байта · данные
//! ```
//! Типы секций: 1 — корневой документ, 2 — страница (id = PageId, shallow-снапшот Loro),
//! 3 — ссылки страницы на изображения (id = PageId, хэши через `\n`),
//! 4 — изображение (id = хэш blake3), 5 — кэш сцены страницы
//! (id = PageId, данные — blake3 снапшота страницы 32 байта + таблица узлов).

use std::collections::{HashMap, HashSet};

use crate::binary::Reader;
use crate::document::{Document, UnloadedPage};
use crate::error::{DocError, DocResult};
use crate::images::ImageStore;
use crate::scene_cache;
use crate::types::PageId;

pub const MAGIC: &[u8; 8] = b"FERRUM\0\0";
pub const FORMAT_VERSION: u32 = 1;

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
        if let Some(rows) = &payload.scene {
            let mut data = blake3::hash(&payload.snapshot).as_bytes().to_vec();
            data.extend_from_slice(&scene_cache::encode(rows));
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
            IMAGE if intact => images.insert_verified(id, data.to_vec()),
            IMAGE => bad_images += 1,
            _ => {} // секции из будущих версий формата пропускаем
        }
    }
    if bad_images > 0 {
        warnings.push(format!(
            "Файл повреждён: не удалось прочитать изображений — {bad_images}"
        ));
    }
    let root = root.ok_or_else(|| {
        DocError::Corrupted("не удалось прочитать структуру документа".to_string())
    })?;
    let unloaded = snapshots
        .into_iter()
        .map(|(id, snapshot)| {
            // Кэш годен, только если построен именно из этого снапшота.
            let scene = scenes
                .get(&id)
                .filter(|d| d.len() >= 32 && d[..32] == *blake3::hash(&snapshot).as_bytes())
                .and_then(|d| scene_cache::decode(&d[32..]));
            let image_refs = refs.remove(&id).unwrap_or_default();
            (
                id,
                UnloadedPage {
                    snapshot,
                    image_refs,
                    scene,
                },
            )
        })
        .collect();
    let (document, mut doc_warnings) = Document::from_parts(&root, unloaded, images)?;
    warnings.append(&mut doc_warnings);
    Ok(Opened { document, warnings })
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
        let mut pos = 16;
        for _ in 0..count {
            let mut r = crate::binary::Reader::at(bytes, pos);
            let k = r.u8()?;
            let id_len = r.u32()? as usize;
            r.take(id_len)?;
            let len = r.u64()? as usize;
            r.take(32)?;
            let data_at = pos + 1 + 4 + id_len + 8 + 32;
            if k == kind {
                return Some(data_at);
            }
            pos = data_at + len;
        }
        None
    }
}
````

- [ ] **Step 5: Запустить тесты**

Run: `cargo test -p ferrum-doc`
Expected: PASS — `types` 5, `page` 10, `document` 11, `container` 10.

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-doc
git commit -m "feat(doc): формат .ferrum с кэшем сцены и устойчивостью к повреждениям"
```

---

### Task 5: Property-тест инвариантов дерева и зеркала

**Files:**
- Test: `crates/ferrum-doc/tests/invariants.rs`

**Interfaces:**
- Consumes: публичный API из Task 1–4.
- Produces: —

- [ ] **Step 1: Написать property-тест**

Случайные последовательности из создания, перемещения, удаления, изменения `x` и заливки. После каждой операции, каждого undo и каждого redo проверяется:
- в дереве нет повторов;
- каждый узел есть среди детей своего родителя;
- зеркало свойств совпадает с честной перезагрузкой из Loro.

После полной отмены страница пуста.

`crates/ferrum-doc/tests/invariants.rs`:

```rust
//! Случайные последовательности операций не ломают дерево, а полная отмена
//! возвращает страницу в исходное (пустое) состояние.

use std::collections::HashSet;

use ferrum_doc::*;
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Create {
        parent: usize,
        kind: u8,
    },
    Move {
        node: usize,
        parent: usize,
        index: usize,
    },
    Delete {
        node: usize,
    },
    SetX {
        node: usize,
        x: i32,
    },
    Fill {
        node: usize,
        rgb: u8,
    },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<usize>(), 0u8..6).prop_map(|(parent, kind)| Op::Create { parent, kind }),
        (any::<usize>(), any::<usize>(), 0usize..5).prop_map(|(node, parent, index)| Op::Move {
            node,
            parent,
            index
        }),
        any::<usize>().prop_map(|node| Op::Delete { node }),
        (any::<usize>(), -1000i32..1000).prop_map(|(node, x)| Op::SetX { node, x }),
        (any::<usize>(), any::<u8>()).prop_map(|(node, rgb)| Op::Fill { node, rgb }),
    ]
}

const KINDS: [NodeKind; 6] = [
    NodeKind::Frame,
    NodeKind::Group,
    NodeKind::Rect,
    NodeKind::Ellipse,
    NodeKind::Line,
    NodeKind::Image,
];

/// `pick(i)` — живой узел или корень (None), чтобы операции попадали в существующие узлы.
fn pick(alive: &[NodeId], i: usize) -> Option<NodeId> {
    if alive.is_empty() || i % (alive.len() + 1) == alive.len() {
        None
    } else {
        Some(alive[i % (alive.len() + 1)])
    }
}

fn check_tree(p: &PageDoc) {
    let all = p.walk();
    let unique: HashSet<_> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "узел встречается в дереве дважды");
    for id in &all {
        assert!(p.contains(*id));
        let siblings = p.children(p.parent(*id));
        assert!(siblings.contains(id), "родитель не знает о ребёнке");
        p.props(*id).expect("свойства читаются");
    }
    // Зеркало свойств совпадает с тем, что честно читается из Loro.
    let fresh = PageDoc::from_snapshot(&p.export_snapshot().unwrap()).unwrap();
    assert_eq!(fresh.walk(), all, "порядок узлов после перезагрузки");
    for id in &all {
        assert_eq!(
            fresh.props(*id).unwrap(),
            p.props(*id).unwrap(),
            "зеркало разошлось с Loro"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn random_ops_keep_tree_valid_and_undo_all_empties_page(ops in prop::collection::vec(op(), 1..40)) {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        for op in ops {
            let alive = doc.page(page).unwrap().walk();
            let _ = doc.transact(page, vec![], |p| {
                match op {
                    Op::Create { parent, kind } => {
                        p.create_node(pick(&alive, parent), None, KINDS[kind as usize])?;
                    }
                    Op::Move { node, parent, index } => {
                        if let Some(n) = pick(&alive, node) {
                            p.move_node(n, pick(&alive, parent), index)?;
                        }
                    }
                    Op::Delete { node } => {
                        if let Some(n) = pick(&alive, node) {
                            p.delete_nodes(&[n])?;
                        }
                    }
                    Op::Fill { node, rgb } => {
                        if let Some(n) = pick(&alive, node) {
                            let fills = vec![Fill::solid(Color::rgba(rgb, rgb, 0, 255))];
                            p.set_props(n, &PropsPatch { fills: Some(fills), ..Default::default() })?;
                        }
                    }
                    Op::SetX { node, x } => {
                        if let Some(n) = pick(&alive, node) {
                            p.set_props(n, &PropsPatch { x: Some(x as f64), ..Default::default() })?;
                        }
                    }
                }
                Ok(vec![])
            });
            check_tree(doc.page(page).unwrap());
        }
        while doc.undo().unwrap().is_some() {
            check_tree(doc.page(page).unwrap());
        }
        prop_assert!(doc.page(page).unwrap().walk().is_empty());
        while doc.redo().unwrap().is_some() {
            check_tree(doc.page(page).unwrap());
        }
    }
}
```

- [ ] **Step 2: Запустить тест**

Run: `cargo test -p ferrum-doc --test invariants`
Expected: PASS, `1 passed` (64 случайных сценария, несколько секунд в debug).

Если тест падает, proptest печатает минимальную последовательность операций. Исправлять нужно `page.rs` или `document.rs`, а не ослаблять проверку.

- [ ] **Step 3: Закоммитить**

```bash
git add crates/ferrum-doc/tests/invariants.rs
git commit -m "test(doc): property-тест инвариантов дерева, undo/redo и зеркала свойств"
```

---

### Task 6: Контрольный бенчмарк и отчёт

**Files:**
- Create: `crates/ferrum-doc/src/fixture.rs`
- Create: `crates/ferrum-doc/benches/doc_bench.rs`
- Create: `crates/ferrum-doc/examples/bench_report.rs`
- Create: `docs/benchmarks/2026-10-23-ferrum-doc.md`
- Modify: `crates/ferrum-doc/src/lib.rs`
- Modify: `crates/ferrum-doc/Cargo.toml`

**Interfaces:**
- Consumes: публичный API из Task 1–4.
- Produces: `fixture::page_with_nodes(n) -> DocResult<(PageDoc, Vec<NodeId>)>`, `fixture::fill(&PageDoc, n) -> DocResult<Vec<NodeId>>`; пример `bench_report` (код выхода 1 при провале цели); бенчмарки `doc_bench`.

- [ ] **Step 1: Добавить генератор страниц**

`crates/ferrum-doc/src/fixture.rs`:

```rust
//! Генератор тестовых страниц для бенчмарков. Не для продакшен-кода.

use crate::{Color, DocResult, Fill, NodeId, NodeKind, PageDoc, PropsPatch, Stroke, StrokeAlign};

/// Отдельная страница из `nodes` узлов.
pub fn page_with_nodes(nodes: usize) -> DocResult<(PageDoc, Vec<NodeId>)> {
    let page = PageDoc::new();
    let ids = fill(&page, nodes)?;
    page.commit_for_tests();
    Ok((page, ids))
}

/// Наполняет страницу `nodes` узлами: фреймы по 100 узлов (фрейм + 99 фигур),
/// у каждой фигуры заданы координаты, имя, заливка и обводка — как в реальном макете.
/// Подходит и для вызова внутри `Document::transact`.
pub fn fill(page: &PageDoc, nodes: usize) -> DocResult<Vec<NodeId>> {
    let mut ids = Vec::with_capacity(nodes);
    let frames = nodes.div_ceil(100);
    for f in 0..frames {
        let frame = page.create_node(None, None, NodeKind::Frame)?;
        page.set_props(
            frame,
            &PropsPatch {
                name: Some(format!("Экран {f}")),
                x: Some((f % 50) as f64 * 1600.0),
                y: Some((f / 50) as f64 * 1200.0),
                w: Some(1440.0),
                h: Some(1024.0),
                ..Default::default()
            },
        )?;
        ids.push(frame);
        for i in 0..99 {
            if ids.len() == nodes {
                break;
            }
            let kind = if i % 3 == 0 {
                NodeKind::Ellipse
            } else {
                NodeKind::Rect
            };
            let id = page.create_node(Some(frame), None, kind)?;
            page.set_props(
                id,
                &PropsPatch {
                    name: Some(format!("Слой {i}")),
                    x: Some((i % 10) as f64 * 140.0),
                    y: Some((i / 10) as f64 * 100.0),
                    w: Some(120.0),
                    h: Some(80.0),
                    corner_radius: Some(8.0),
                    fills: Some(vec![Fill::solid(Color::rgba(i as u8, 120, 200, 255))]),
                    strokes: Some(vec![Stroke {
                        color: Color::BLACK,
                        width: 1.0,
                        align: StrokeAlign::Inside,
                    }]),
                    ..Default::default()
                },
            )?;
            ids.push(id);
        }
    }
    Ok(ids)
}
```

`crates/ferrum-doc/src/lib.rs`:

```rust
//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod binary;
mod codec;
pub mod container;
mod document;
mod error;
mod history;
mod images;
mod page;
mod scene_cache;
mod types;

pub use document::{Document, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use images::ImageStore;
pub use page::PageDoc;
pub use scene_cache::SceneRow;
pub use types::*;

#[doc(hidden)]
pub mod fixture;
```

- [ ] **Step 2: Подключить бенчмарк в манифест**

`crates/ferrum-doc/Cargo.toml`:

```toml
[package]
name = "ferrum-doc"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
loro.workspace = true
loro-internal.workspace = true
thiserror.workspace = true
blake3.workspace = true

[dev-dependencies]
criterion.workspace = true
proptest.workspace = true

[[bench]]
name = "doc_bench"
harness = false
```

- [ ] **Step 3: Добавить регрессионные бенчмарки**

`crates/ferrum-doc/benches/doc_bench.rs`:

```rust
//! Регрессионные бенчмарки модели документа: `cargo bench -p ferrum-doc`.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use ferrum_doc::fixture::page_with_nodes;
use ferrum_doc::{PageDoc, PropsPatch};

const N: usize = 10_000;

fn bench(c: &mut Criterion) {
    let (page, ids) = page_with_nodes(N).unwrap();
    let snapshot = page.export_snapshot().unwrap();

    c.bench_function("export_snapshot_10k", |b| {
        b.iter(|| page.export_snapshot().unwrap())
    });

    c.bench_function("open_and_read_all_10k", |b| {
        b.iter(|| {
            let p = PageDoc::from_snapshot(&snapshot).unwrap();
            p.walk()
                .into_iter()
                .map(|id| p.props(id).unwrap())
                .collect::<Vec<_>>()
        })
    });

    c.bench_function("set_props_single", |b| {
        let mut x = 0.0;
        b.iter(|| {
            x += 1.0;
            page.set_props(
                ids[N / 2],
                &PropsPatch {
                    x: Some(x),
                    ..Default::default()
                },
            )
            .unwrap();
            page.commit_for_tests();
        })
    });

    c.bench_function("move_1000_in_one_commit", |b| {
        b.iter_batched(
            || PageDoc::from_snapshot(&snapshot).unwrap(),
            |p| {
                for (i, id) in ids.iter().take(1000).enumerate() {
                    p.set_props(
                        *id,
                        &PropsPatch {
                            x: Some(i as f64),
                            y: Some(5.0),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                }
                p.commit_for_tests();
            },
            BatchSize::LargeInput,
        )
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
```

- [ ] **Step 4: Добавить контрольный бенчмарк**

Память меряется счётчиком аллокаций (`GlobalAlloc`), а не RSS: RSS не возвращается системе и даёт отрицательные приросты.

`crates/ferrum-doc/examples/bench_report.rs`:

```rust
//! Контрольный бенчмарк модели документа:
//! `cargo run --release -p ferrum-doc --example bench_report`.
//! Сверяет замеры с целями этапа 1 и печатает таблицу PASS/FAIL (код выхода 1 при провале).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ferrum_doc::container::{decode, encode};
use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, PropsPatch};

/// Считает живые байты в куче — точнее, чем RSS процесса.
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn heap_mb() -> f64 {
    LIVE.load(Ordering::Relaxed) as f64 / 1_048_576.0
}

fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed())
}

fn ms(d: Duration) -> String {
    format!("{:.1} мс", d.as_secs_f64() * 1000.0)
}

struct Row {
    metric: String,
    value: String,
    limit: &'static str,
    pass: Option<bool>,
}

fn main() {
    let mut rows = Vec::new();
    for &n in &[100_000usize, 500_000] {
        // Цели этапа — для 100 000 узлов; 500 000 — справочно.
        let gate = n == 100_000;
        let mut add = |metric: &str, value: String, limit: &'static str, pass: bool| {
            rows.push(Row {
                metric: format!("{metric} ({n})"),
                value,
                limit: if gate { limit } else { "—" },
                pass: gate.then_some(pass),
            })
        };

        let (bytes, build) = time(|| {
            let mut doc = Document::new();
            let page = doc.pages()[0].0;
            doc.transact(page, vec![], |p| fill(p, n).map(|_| vec![]))
                .unwrap();
            encode(&doc).unwrap()
        });
        add("Генерация и сохранение", ms(build), "—", true);
        add(
            "Размер файла",
            format!("{:.1} МБ", bytes.len() as f64 / 1_048_576.0),
            "≤ 30 МБ",
            bytes.len() <= 30 << 20,
        );

        let before = heap_mb();
        let (opened, open) = time(|| decode(&bytes).unwrap());
        let mut doc = opened.document;
        let page = doc.pages()[0].0;
        let cached = doc.cached_scene(page).map(<[_]>::len).unwrap_or(0);
        assert_eq!(cached, n, "кэш сцены должен покрывать все узлы");
        add(
            "Открытие до первого кадра (кэш сцены)",
            ms(open),
            "≤ 300 мс",
            open <= Duration::from_millis(300),
        );

        let (_, hydrate) = time(|| doc.page(page).unwrap().walk().len());
        add(
            "Фоновая загрузка Loro",
            ms(hydrate),
            "≤ 1500 мс",
            hydrate <= Duration::from_millis(1500),
        );
        let memory = heap_mb() - before;
        add(
            "Память открытого документа",
            format!("{memory:.0} МБ"),
            "≤ 400 МБ",
            memory <= 400.0,
        );

        let ids = doc.page(page).unwrap().walk();
        let mut edit_total = Duration::ZERO;
        for i in 0..200 {
            let id = ids[(i * 97) % n];
            let (_, d) = time(|| {
                doc.transact(page, vec![id], |p| {
                    p.set_props(
                        id,
                        &PropsPatch {
                            x: Some(i as f64),
                            ..Default::default()
                        },
                    )?;
                    Ok(vec![id])
                })
                .unwrap()
            });
            edit_total += d;
        }
        let edit = edit_total / 200;
        add(
            "Правка свойства (транзакция)",
            format!("{:.3} мс", edit.as_secs_f64() * 1000.0),
            "≤ 1 мс",
            edit <= Duration::from_millis(1),
        );

        let (_, undo) = time(|| doc.undo().unwrap());
        add(
            "Undo одной правки",
            ms(undo),
            "≤ 50 мс",
            undo <= Duration::from_millis(50),
        );

        let (_, batch) = time(|| {
            doc.transact(page, vec![], |p| {
                for (i, id) in ids.iter().take(1000).enumerate() {
                    p.set_props(
                        *id,
                        &PropsPatch {
                            x: Some(i as f64),
                            y: Some(1.0),
                            ..Default::default()
                        },
                    )?;
                }
                Ok(vec![])
            })
            .unwrap()
        });
        add(
            "Сдвиг 1000 узлов одной транзакцией",
            ms(batch),
            "≤ 50 мс",
            batch <= Duration::from_millis(50),
        );

        let (_, undo_batch) = time(|| doc.undo().unwrap());
        add(
            "Undo сдвига 1000 узлов",
            ms(undo_batch),
            "≤ 50 мс",
            undo_batch <= Duration::from_millis(50),
        );

        let (_, save) = time(|| encode(&doc).unwrap());
        add(
            "Полное сохранение (периодически и при закрытии)",
            ms(save),
            "≤ 1500 мс",
            save <= Duration::from_millis(1500),
        );
    }

    println!("| Метрика | Значение | Цель | Итог |");
    println!("|---|---|---|---|");
    let mut failed = false;
    for r in &rows {
        let verdict = match r.pass {
            Some(true) => "PASS",
            Some(false) => {
                failed = true;
                "FAIL"
            }
            None => "—",
        };
        println!("| {} | {} | {} | {} |", r.metric, r.value, r.limit, verdict);
    }
    println!();
    println!(
        "Замер нативный, release. WASM обычно в 1.5–2 раза медленнее — цели заданы с этим запасом."
    );
    if failed {
        std::process::exit(1);
    }
}
```

- [ ] **Step 5: Проверить сборку, линтер и все тесты**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test -p ferrum-doc && cargo bench -p ferrum-doc --no-run`
Expected: без ошибок и предупреждений; тесты — `types` 5, `page` 10, `document` 11, `container` 10, `invariants` 1.

- [ ] **Step 6: Запустить контрольный бенчмарк**

Run: `cargo run --release -p ferrum-doc --example bench_report | tee /tmp/ferrum-bench.md`
Expected: код выхода 0, все строки для 100 000 узлов — `PASS`. Ориентиры прототипа (Apple Silicon): первый кадр ~55 мс, фоновая загрузка ~300 мс, память ~149 МБ, файл ~26.6 МБ, сохранение ~780 мс. Прогон с 500 000 узлов занимает около минуты.

Если какая-то строка `FAIL`: не меняйте цель. Сравните значение с ориентиром, найдите регрессию (`cargo bench -p ferrum-doc` покажет, какая операция замедлилась) и исправьте код.

- [ ] **Step 7: Записать отчёт**

Создать `docs/benchmarks/2026-10-23-ferrum-doc.md`: заголовок `# Бенчмарк ferrum-doc`, строка с машиной (`sysctl -n machdep.cpu.brand_string` на macOS) и версией Rust (`rustc --version`), затем таблица из `/tmp/ferrum-bench.md` без изменений.

- [ ] **Step 8: Закоммитить**

```bash
git add crates/ferrum-doc docs/benchmarks/2026-10-23-ferrum-doc.md
git commit -m "bench(doc): контрольный бенчмарк модели документа и отчёт"
```

---

## Что этап 1 сознательно не делает

- Обновление зеркала и индекса по событиям Loro при импорте чужих правок — подсистема 4.
- Инкрементальный экспорт обновлений для журнала автосохранения — этап 6.
- Цепочка миграций схемы: сейчас есть только версия 1 и защита от файлов из будущего; первая миграция появится вместе с первым несовместимым изменением схемы.
- Операции со страницами (добавление, переименование, удаление) не попадают в undo.
