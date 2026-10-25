# Ferrum, подсистема 1, этап 5: интерфейс — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Редактор Ferrum с интерфейсом Figma UI3 на SolidJS поверх ядра этапа 4: левая панель (меню файла, страницы, виртуальное дерево слоёв), плавающий тулбар внизу, правая панель (зум, «Поделиться», вкладки, секции свойств), палитра цвета, диалог настроек (горячие клавиши, тема, привязка к пикселям), контекстное меню и подсказки из действующей раскладки, системный буфер обмена, минимальные «Открыть…» и «Сохранить как…». Основной поток только показывает готовые данные и пересылает ввод: дерево слоёв, сводку выделения, диапазоны, поиск и разбор буфера обмена считает worker. Стенд этапа 4 остаётся отдельной страницей `debug.html` для тестов ядра.

**Architecture:** Ядро получает команды и события панелей (протокол этапа 4: serde + tsify, всё проверяется и ограничивается). Worker держит дерево слоёв (`layers.rs`: раскрытие, поиск, окно строк) и сводку выделения (`summary.rs`: значения и «Смешанные»), шлёт их только при изменении. Правки панелей — методы редактора (`panel.rs`, `pages.rs`): каждая — одна транзакция; перетаскивание подписи поля и движение в палитре — жест с превью в сцене и одним шагом отмены при отпускании. На основном потоке собственный планировщик (идеи fasterdom, свой код): ядро в каждом `requestAnimationFrame` вызывает кадр планировщика — фаза measure (чтения layout и живого состояния из общей памяти), затем фаза mutate: события ядра, пришедшие с прошлого кадра, живые значения и ручные записи в DOM — в одной `batch()` Solid. Время кадра планировщика входит в бюджет основного потока, который меряет `Core`. Интерфейс собран из токенов и компонентов дизайн-системы Ferrum (тёмная тема основная, светлая равноправная), иконки — формы Lucide, вставленные разметкой SVG.

**Tech Stack:** Rust 1.96 — новых крейтов нет (`ferrum-wasm` получает зависимость на `ferrum-scene` из workspace); solid-js 1.9.17, vite-plugin-solid 2.11.14 (babel-preset-solid 1.9.16, @babel/core 7.29.7), @fontsource-variable/onest 5.3.1, @fontsource/jetbrains-mono 5.3.0; Vite 8.3.4, TypeScript 6.0.3, @playwright/test 1.64.0 (E2E и модульные тесты чистой логики тем же раннером, без браузера); иконки — формы Lucide 1.55.0 (ISC), вставленные в `icons.tsx`.

**Спека:** `docs/superpowers/specs/2026-10-23-ferrum-core-design.md`, разделы 4.2–4.4, 7.6, 7.7, 8.7, 9, 11, 12. **Дизайн-система:** https://claude.ai/artifact/L9ZDGW6XCRGquBLYTtYYUK (токены `tokens.json`, компоненты Button, Toolbar, PanelHeader, PropertySection, NumberField, LayerRow, SelectionBox).

## Global Constraints

- Ветка `feat/stage5-ui` от головы этапа 4 (8e2adf0, PR #4). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Новых крейтов нет: в `Cargo.lock` меняется только список зависимостей `ferrum-wasm` (+`ferrum-scene`).
- npm-пакеты — точными версиями; `apps/web/.npmrc` (`save-exact=true`, `before=2026-10-10`) не меняется — и зависимости зависимостей не новее этой даты. Новые пакеты: `solid-js 1.9.17`, `@fontsource-variable/onest 5.3.1`, `@fontsource/jetbrains-mono 5.3.0` (зависимости), `vite-plugin-solid 2.11.14` (разработка); `npm install` даёт ровно тот `package-lock.json`, что в Task 6 (107 пакетов). Пакет иконок не ставится: `lucide-solid` занимает 55 МБ и даёт компонент на иконку, а нужно 40 форм — около 5 КБ разметки.
- Основной поток тонкий (спека 4.2): ничего, что зависит от размера документа. Дерево слоёв — только окно видимых строк (≤ 200 за запрос), сводка выделения, диапазоны выделения, поиск по слоям, разбор буфера обмена и перекодирование картинок — в worker'е. Чтения layout — в фазе measure планировщика; в режиме разработки чтение вне неё даёт предупреждение. Бюджет: ≤ 4 мс за кадр на обработчики, планировщик и обновления Solid; длинных задач (> 50 мс) нет.
- Всё, что пересекает границу потоков, не доверенное: идентификаторы (≤ 64 байт), строки (≤ 4096 байт), списки (≤ 1000 идентификаторов, ≤ 64 заливок и обводок), числа (конечные, в пределах сцены), цвета, JSON раскладки (≤ 64 КБ), байты (≤ 64 МБ, картинки ≤ 8192 px по заголовку). Неверное — событие `error` на русском, не паника и не правка.
- Каждая правка из панелей — ровно один шаг отмены; правка без изменений шага не создаёт. Жест поля или палитры — превью до отпускания и один шаг; Esc отменяет без следа. Страницы и фон страницы в историю не попадают (как в этапе 1), поэтому удаление страницы — с подтверждением.
- Каждая ошибка доходит до пользователя ровно один раз, на русском, не в цикле; остановка ядра — постоянная плашка поверх холста.
- Интерфейс — из токенов и компонентов дизайн-системы с её точными значениями и текстами; тёмная тема основная, светлая равноправная, «как в системе» — по `prefers-color-scheme`. Фокус с клавиатуры — кольцо `select` 2px; роли toolbar, tree, tablist, listbox, menu, dialog; контраст текста — как в дизайн-системе; `prefers-reduced-motion` выключает анимации.
- Тесты не проходят вхолостую: правка проверяется по документу и числу шагов, виртуальный список — по числу строк в DOM, живые значения — по тому, что панель их получала, пиксели холста — снимком.
- Тексты интерфейса, ошибок и комментарии — на русском. Каталог сборки (`CARGO_TARGET_DIR`) у каждой копии свой; `build-wasm.mjs` передаёт cargo абсолютный путь.

---

## Этапы подсистемы 1

Подсистема 1 делится на шесть этапов (таблица — в плане этапа 1, `docs/superpowers/plans/2026-10-23-ferrum-stage1-document-model.md`). Этот документ — план этапа 5:

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Модель документа (готов) | `ferrum-doc` | 3, 5 |
| 2. Сцена и рендер (готов) | `ferrum-scene`, `ferrum-render` | 6, 8 |
| 3. Редактор без браузера (готов) | `ferrum-editor` | 7 |
| 4. Worker и обмен данными (PR #4) | `ferrum-wasm`, Web Worker + OffscreenCanvas, `Transport`, стенд `apps/web` | 4.2–4.3, 8.1–8.2 |
| **5. Интерфейс** (этот план) | `apps/web` на SolidJS: планировщик measure/mutate, раскладка UI3, панели, палитра цвета, настройки горячих клавиш, системный буфер обмена | 4.4, 9 |
| 6. Файлы и десктоп | `FileStore`, воркер ввода-вывода, автосохранение, Tauri | 10, 11, 12 |

---

## Структура файлов этапа 5

| Файл | Ответственность |
|---|---|
| `crates/ferrum-doc/src/{document,page,error}.rs` | фон страницы (`page_background`), запрет удалить последнюю страницу, правка без изменений — не шаг |
| `crates/ferrum-editor/src/pages.rs` | страницы из панели: создать, переименовать, удалить с переходом, фон; имя документа; `clean_name` |
| `crates/ferrum-editor/src/panel.rs` | правки из панелей: поля (X, Y, W, H, поворот, прозрачность, радиус, толщина обводки), размер пресета, заливки, обводки, обрезка; жест превью/запись/отмена; флаги и имя слоя; перенос слоёв с сохранением положения |
| `crates/ferrum-wasm/src/layers.rs` | дерево слоёв в worker'е: раскрытие, поиск, окно строк, диапазон, раскрытие предков выделения |
| `crates/ferrum-wasm/src/summary.rs` | сводка выделения для панели свойств: значения, «Смешанные», поля по типам узлов |
| `crates/ferrum-wasm/src/{protocol,core}.rs`, `core/panels.rs` | команды и события панелей, проверки и пределы, раскладка и пресеты при запуске, экспорт PNG, вставка картинки, фон страницы |
| `apps/web/debug.html`, `src/debug.ts` | стенд этапа 4 (тесты ядра) |
| `apps/web/index.html`, `src/main.tsx` | редактор |
| `apps/web/src/ui/scheduler.ts` | планировщик measure/mutate, предупреждение о чтениях layout |
| `apps/web/src/ui/{boot,store,context,keys}.ts` | запуск, состояние из событий, контекст, маршрут клавиш |
| `apps/web/src/ui/styles/*.css`, `icons.tsx` | токены и компоненты дизайн-системы, иконки Lucide |
| `apps/web/src/ui/{app,chrome,toolbar,menu,tooltip,toasts}.tsx` | раскладка UI3, шапки, тулбар, меню, подсказки, уведомления |
| `apps/web/src/ui/{layers,pages}.tsx` | панель слоёв (виртуальный список) и страниц |
| `apps/web/src/ui/{properties,number-field,paints,picker}.tsx`, `number.ts`, `colour.ts` | панель свойств, поле числа, заливки и обводки, палитра |
| `apps/web/src/ui/{settings,keymap-ui,shortcuts}.ts`, `settings-dialog.tsx` | настройки, таблица раскладки, подписи сочетаний |
| `apps/web/src/ui/{clipboard,system-clipboard,files}.ts`, `src/core/image-size.ts` | буфер обмена, файлы, размер картинки по заголовку |
| `apps/web/unit/*.test.ts`, `playwright.unit.config.ts` | модульные тесты чистой логики |
| `apps/web/e2e/{app,chrome,layers,properties,picker,settings,clipboard,a11y}.spec.ts`, `perf.spec.ts` | E2E интерфейса и бюджет основного потока |

Все команды запускаются из корня репозитория. Перед Task 1 закоммитить нечего; ветка — `feat/stage5-ui`.

---

### Task 1: Страницы: фон, создать, переименовать, удалить

Панель «Страницы» и раздел «Страница» панели свойств (без выделения) работают через редактор. Фон страницы — цвет `#rrggbbaa` в метаданных узла страницы в дереве страниц корневого документа (`background`); нейтральное значение (как у холста в теме) не пишется, ключ снимается. Операции со страницами в историю отмены не попадают — как и в этапе 1 (`add_page`, `rename_page`). Последнюю страницу удалить нельзя (`DocError::LastPage`): документ без страниц не открыть. Удаление текущей страницы переключает на следующую (у последней — на предыдущую).

Имена из интерфейса не доверенные: `clean_name` заменяет управляющие символы пробелом, обрезает пробелы по краям и длину до 256 символов; пустое имя ничего не меняет — поле возвращает прежнее, как в Figma.

Здесь же — правка свойств без изменений больше не пишется: `Tx::set_props` с теми же значениями не трогает Loro и не записывает обратную операцию, поэтому транзакция из одних таких правок шага не создаёт. Поля панели применяют значение и при уходе фокуса — без этого простой проход по полям Tab-ом копил бы пустые шаги отмены. Тест сцены этапа 2, который правил обрезку прямоугольника на то же значение (`false`), теперь правит её на `true`.

**Files:**
- Modify: `crates/ferrum-doc/src/document.rs`
- Modify: `crates/ferrum-doc/src/error.rs`
- Modify: `crates/ferrum-doc/src/page.rs`
- Test: `crates/ferrum-doc/tests/page_background.rs`
- Modify: `crates/ferrum-scene/tests/scene.rs`
- Create: `crates/ferrum-editor/src/pages.rs`
- Modify: `crates/ferrum-editor/src/lib.rs`
- Test: `crates/ferrum-editor/tests/pages.rs`

**Interfaces:**
- Consumes: `Document::{add_page, rename_page, delete_page, pages}`, `Editor::{set_page, add_page}` этапов 1 и 3.
- Produces: `Document::{page_background(PageId) -> Option<Color>, set_page_background(PageId, Option<Color>)}`, `DocError::LastPage`; `Editor::{create_page, rename_page, delete_page, page_background, set_page_background, set_document_name}`; `ferrum_editor::{clean_name, MAX_NAME_CHARS}`. `Tx::set_props` с теми же значениями — без записи.

- [ ] **Step 1: Написать падающие тесты модели**

`crates/ferrum-doc/tests/page_background.rs`:

````rust
//! Фон страницы (этап 5): цвет в метаданных страницы, без записи нейтрального
//! значения, переживает сохранение; последнюю страницу удалить нельзя. И правка
//! свойств, которая ничего не меняет, не создаёт шага отмены (поля панелей
//! применяют значение и при уходе фокуса).

use ferrum_doc::*;

#[test]
fn background_is_unset_by_default_and_round_trips() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    assert_eq!(doc.page_background(page), None, "по умолчанию — цвет темы");
    let c = Color::parse("#ff8800").unwrap();
    doc.set_page_background(page, Some(c)).unwrap();
    assert_eq!(doc.page_background(page), Some(c));
    let bytes = container::encode(&doc).unwrap();
    let opened = container::decode(&bytes).unwrap();
    assert_eq!(
        opened.document.page_background(page),
        Some(c),
        "фон сохраняется в файле"
    );
    doc.set_page_background(page, None).unwrap();
    assert_eq!(doc.page_background(page), None, "сброс снимает ключ");
}

#[test]
fn background_is_not_in_undo_history() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.set_page_background(page, Some(Color::WHITE)).unwrap();
    assert!(
        !doc.can_undo(),
        "операции со страницами не попадают в историю"
    );
}

#[test]
fn unknown_page_and_last_page_are_errors() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let other = doc.add_page("Вторая").unwrap();
    doc.delete_page(other).unwrap();
    assert!(matches!(
        doc.set_page_background(other, Some(Color::WHITE)),
        Err(DocError::PageNotFound(_))
    ));
    assert_eq!(doc.page_background(other), None);
    assert!(matches!(doc.delete_page(page), Err(DocError::LastPage)));
    assert_eq!(doc.pages().len(), 1);
}

#[test]
fn unchanged_props_are_not_a_step() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    doc.clear_history();
    let same = PropsPatch {
        opacity: Some(1.0),
        x: Some(0.0),
        ..Default::default()
    };
    doc.transact(page, vec![], |tx| {
        tx.set_props(id, &same)?;
        Ok(vec![])
    })
    .unwrap();
    assert!(!doc.can_undo(), "те же значения — не шаг");
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                opacity: Some(0.5),
                ..same.clone()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    assert!(doc.can_undo());
    doc.undo().unwrap();
    assert_eq!(doc.page(page).unwrap().props(id).unwrap().opacity, 1.0);
}
````

Run: `cargo test -p ferrum-doc --test page_background`
Expected: FAIL — не компилируется: нет методов `page_background`, `set_page_background`, варианта `DocError::LastPage`.

- [ ] **Step 2: Фон страницы и правка без изменений**

В `crates/ferrum-doc/src/error.rs` заменить:

````rust
    PageNotFound(String),
    #[error("Узел {0} хранит неизвестный тип «{1}»")]
````

на:

````rust
    PageNotFound(String),
    #[error("Нельзя удалить единственную страницу")]
    LastPage,
    #[error("Узел {0} хранит неизвестный тип «{1}»")]
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
use crate::scene_cache::SceneRow;
use crate::types::{NodeId, PageId};

````

на:

````rust
use crate::scene_cache::SceneRow;
use crate::types::{Color, NodeId, PageId};

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
const KEY_NAME: &str = "name";

````

на:

````rust
const KEY_NAME: &str = "name";
/// Цвет фона страницы (`#rrggbbaa`) в метаданных её узла в дереве страниц. Нет
/// ключа — фон как у холста в теме интерфейса (нейтральное значение не пишется).
const KEY_BACKGROUND: &str = "background";

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

    pub fn delete_page(&mut self, id: PageId) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        self.pages_tree.delete(id.0)?;
````

на:

````rust

    /// Удаляет страницу. Единственную — нельзя: документ без страниц не открыть.
    pub fn delete_page(&mut self, id: PageId) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        if self.pages().len() <= 1 {
            return Err(DocError::LastPage);
        }
        self.pages_tree.delete(id.0)?;
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        self.changes.remove(&id);
        Ok(())
````

на:

````rust
        self.changes.remove(&id);
        Ok(())
    }

    /// Цвет фона страницы; `None` — не задан (холст в цвете темы).
    pub fn page_background(&self, id: PageId) -> Option<Color> {
        match self.pages_tree.get_meta(id.0).ok()?.get(KEY_BACKGROUND) {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => Color::parse(&s).ok(),
            _ => None,
        }
    }

    /// Задаёт или снимает (`None`) цвет фона страницы. Как и остальные операции со
    /// страницами, в историю отмены не попадает.
    pub fn set_page_background(&mut self, id: PageId, color: Option<Color>) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        let meta = self.pages_tree.get_meta(id.0)?;
        match color {
            Some(c) => meta.insert(KEY_BACKGROUND, c.to_hex())?,
            None => meta.delete(KEY_BACKGROUND)?,
        }
        self.root.commit();
        Ok(())
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust

    pub(crate) fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        let tid = self.tid(id)?;
        let before = self.props(id)?;
        codec::write_patch(&self.tree.get_meta(tid)?, patch)?;
````

на:

````rust

    /// Правка, которая ничего не меняет (те же значения), не пишется и не
    /// записывает обратную операцию: транзакция без изменений шага не создаёт.
    pub(crate) fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        let tid = self.tid(id)?;
        let before = self.props(id)?;
        let mut after = before.clone();
        patch.apply_to(&mut after);
        if after == before {
            return Ok(());
        }
        codec::write_patch(&self.tree.get_meta(tid)?, patch)?;
````

В `crates/ferrum-scene/tests/scene.rs` заменить:

````rust
        },
        PropsPatch {
            clip_content: Some(false),
            ..Default::default()
````

на:

````rust
        },
        // У прямоугольника обрезка по умолчанию выключена: правка — включить
        // (то же значение изменением не считается и в сцену не приходит).
        PropsPatch {
            clip_content: Some(true),
            ..Default::default()
````

Run: `cargo test -p ferrum-doc --test page_background`
Expected: PASS, `4 passed`.

Run: `cargo test -p ferrum-doc -p ferrum-scene`
Expected: PASS — наборы этапов 1 и 2 (`opacity_and_clip_bump_subtree_version_of_node_and_ancestors_only` — с новой правкой обрезки).

- [ ] **Step 3: Написать падающие тесты редактора**

`crates/ferrum-editor/tests/pages.rs`:

````rust
//! Страницы из панели (этап 5): создать и переключиться, переименовать, удалить
//! текущую с переходом на соседнюю, фон страницы. В историю отмены не попадают.

mod common;

use common::*;
use ferrum_doc::{Color, DocError};
use ferrum_editor::*;

#[test]
fn create_page_switches_and_picks_free_name() {
    let mut h = H::new();
    let first = h.ed.page();
    h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.take_events();
    let p2 = h.ed.create_page().unwrap();
    assert_eq!(h.ed.page(), p2, "новая страница — текущая");
    assert!(h.ed.scene().is_empty());
    assert!(h.ed.take_events().contains(&EditorEvent::PageChanged(p2)));
    h.ed.rename_page(p2, "Страница 3").unwrap();
    let p3 = h.ed.create_page().unwrap();
    let names: Vec<String> = h.ed.document().pages().into_iter().map(|p| p.1).collect();
    assert_eq!(
        names,
        ["Страница 1", "Страница 3", "Страница 4"],
        "имя не повторяется"
    );
    assert_ne!(p3, first);
}

#[test]
fn rename_cleans_name_and_ignores_empty() {
    let mut h = H::new();
    let p = h.ed.page();
    h.ed.rename_page(p, "  Экраны\nвхода\t ").unwrap();
    assert_eq!(h.ed.document().pages()[0].1, "Экраны входа");
    h.ed.rename_page(p, "   ").unwrap();
    assert_eq!(
        h.ed.document().pages()[0].1,
        "Экраны входа",
        "пустое имя не меняет"
    );
    let long = "я".repeat(1000);
    h.ed.rename_page(p, &long).unwrap();
    assert_eq!(h.ed.document().pages()[0].1.chars().count(), MAX_NAME_CHARS);
    assert_eq!(clean_name("\u{7}"), None);
}

#[test]
fn delete_current_page_moves_to_neighbour() {
    let mut h = H::new();
    let p1 = h.ed.page();
    let p2 = h.ed.create_page().unwrap();
    let p3 = h.ed.create_page().unwrap();
    h.ed.set_page(p2).unwrap();
    h.ed.delete_page(p2).unwrap();
    assert_eq!(h.ed.page(), p3, "следующая");
    h.ed.delete_page(p3).unwrap();
    assert_eq!(h.ed.page(), p1, "у последней — предыдущая");
    assert!(matches!(h.ed.delete_page(p1), Err(DocError::LastPage)));
    assert_eq!(h.ed.document().pages().len(), 1);
}

#[test]
fn delete_other_page_keeps_current_and_selection() {
    let mut h = H::new();
    let p1 = h.ed.page();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let p2 = h.ed.create_page().unwrap();
    h.ed.set_page(p1).unwrap();
    h.ed.select(&[a]);
    h.ed.delete_page(p2).unwrap();
    assert_eq!(h.ed.page(), p1);
    assert_eq!(h.sel(), vec![a]);
}

#[test]
fn background_redraws_current_page_only() {
    let mut h = H::new();
    let p1 = h.ed.page();
    let p2 = h.ed.add_page("Фон").unwrap();
    h.ed.take_events();
    let c = Color::parse("#102030").unwrap();
    h.ed.set_page_background(p2, Some(c)).unwrap();
    assert!(!h.ed.take_events().contains(&EditorEvent::NeedsRedraw));
    assert_eq!(h.ed.page_background(), None);
    h.ed.set_page_background(p1, Some(c)).unwrap();
    assert!(h.ed.take_events().contains(&EditorEvent::NeedsRedraw));
    assert_eq!(h.ed.page_background(), Some(c));
    assert!(!h.ed.document().can_undo(), "фон не в истории");
}

#[test]
fn document_name_is_cleaned() {
    let mut h = H::new();
    assert_eq!(h.ed.set_document_name("  Лендинг\n ").unwrap(), "Лендинг");
    assert_eq!(
        h.ed.set_document_name("").unwrap(),
        "Лендинг",
        "пустое не меняет"
    );
    assert_eq!(h.ed.document().name(), "Лендинг");
}
````

Run: `cargo test -p ferrum-editor --test pages`
Expected: FAIL — не компилируется: нет `create_page`, `clean_name`, `MAX_NAME_CHARS`.

- [ ] **Step 4: Страницы в редакторе**

`crates/ferrum-editor/src/pages.rs`:

````rust
//! Страницы из панели «Страницы» (этап 5): создать, переименовать, удалить,
//! цвет фона. Как и в `ferrum-doc`, операции со страницами не попадают в историю
//! отмены. Имена из интерфейса не доверенные: `clean_name` убирает управляющие
//! символы и ограничивает длину.

use ferrum_doc::{Color, DocResult, PageId};

use crate::editor::{Editor, EditorEvent};

/// Наибольшая длина имени слоя, страницы и файла в символах.
pub const MAX_NAME_CHARS: usize = 256;

/// Имя из поля ввода: без управляющих символов (перевод строки, табуляция — в
/// пробел), без пробелов по краям, не длиннее `MAX_NAME_CHARS`. Пустое — `None`:
/// интерфейс возвращает прежнее имя, как в Figma.
pub fn clean_name(name: &str) -> Option<String> {
    let s: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let s: String = s.trim().chars().take(MAX_NAME_CHARS).collect();
    let s = s.trim_end().to_string();
    (!s.is_empty()).then_some(s)
}

impl Editor {
    /// Новая страница в конце списка с первым свободным именем «Страница N»;
    /// она сразу становится текущей.
    pub fn create_page(&mut self) -> DocResult<PageId> {
        let names: Vec<String> = self.doc.pages().into_iter().map(|(_, n)| n).collect();
        let mut n = names.len() + 1;
        while names.iter().any(|x| *x == format!("Страница {n}")) {
            n += 1;
        }
        let id = self.doc.add_page(&format!("Страница {n}"))?;
        self.set_page(id)?;
        Ok(id)
    }

    /// Переименование; пустое имя ничего не меняет.
    pub fn rename_page(&mut self, id: PageId, name: &str) -> DocResult<()> {
        match clean_name(name) {
            Some(name) => self.doc.rename_page(id, &name),
            None => Ok(()),
        }
    }

    /// Удаляет страницу. Если она текущая, сначала переключает на соседнюю
    /// (следующую, у последней — предыдущую). Единственную удалить нельзя.
    pub fn delete_page(&mut self, id: PageId) -> DocResult<()> {
        let pages = self.doc.pages();
        if pages.len() <= 1 {
            return Err(ferrum_doc::DocError::LastPage);
        }
        if id == self.page {
            let at = pages.iter().position(|(p, _)| *p == id).unwrap_or(0);
            let next = if at + 1 < pages.len() { at + 1 } else { at - 1 };
            self.set_page(pages[next].0)?;
        }
        self.doc.delete_page(id)
    }

    /// Имя документа (шапка панели, имя файла); пустое ничего не меняет.
    /// Возвращает имя после правки.
    pub fn set_document_name(&mut self, name: &str) -> DocResult<String> {
        if let Some(name) = clean_name(name) {
            self.doc.set_name(&name)?;
        }
        Ok(self.doc.name())
    }

    /// Цвет фона текущей страницы; `None` — как у холста в теме.
    pub fn page_background(&self) -> Option<Color> {
        self.doc.page_background(self.page)
    }

    /// Задаёт фон страницы. Холст перерисовывается, если страница текущая.
    pub fn set_page_background(&mut self, id: PageId, color: Option<Color>) -> DocResult<()> {
        self.doc.set_page_background(id, color)?;
        if id == self.page {
            self.out.events.push(EditorEvent::NeedsRedraw);
        }
        Ok(())
    }
}
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
mod ops;
pub mod presets;
````

на:

````rust
mod ops;
pub mod pages;
pub mod presets;
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
````

на:

````rust
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
pub use pages::{clean_name, MAX_NAME_CHARS};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
````

Run: `cargo test -p ferrum-editor --test pages`
Expected: PASS, `6 passed`.

Run: `cargo test -p ferrum-editor`
Expected: PASS — наборы этапа 3 без изменений.

Run: `cargo clippy -p ferrum-doc -p ferrum-editor --all-targets -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-doc crates/ferrum-scene crates/ferrum-editor
git commit -m "feat(editor): страницы из панели — фон, создать, переименовать, удалить"
```

---

### Task 2: Правки из панелей: поля, заливки, слои

Всё, что панели меняют в документе, проходит через редактор (`panel.rs`), — сцена совпадает с документом после каждой правки, и каждая правка — одна транзакция.

- **Поля панели свойств** (`PanelEdit::Number`): значение или прибавка (`relative`) к своему значению у каждого выделенного — так перетаскивание подписи у «Смешанных» двигает каждый узел на свою дельту. X и Y — как показывает панель: у группы — угол рамки её детей в координатах родителя, у остальных — свойства. W и H у фрейма, фигуры и изображения меняют `w`/`h`, левый верхний угол в осях узла остаётся на месте (как в Figma); у группы масштабируются дети тем же разложением, что ресайз (`scale_patches_in`). У линии высоты нет. Поворот — вокруг центра рамки, против часовой стрелки, в панели — в `(-180, 180]`. Прозрачность — `[0, 1]`, радиус — у фрейма, прямоугольника и изображения, толщина — у всех обводок узла. `PanelEdit::Size` — ширина и высота пресета одним шагом.
- **Заливки, обводки, обрезка** заменяются списком целиком; у групп и линий заливок нет, обрезка — только у фреймов.
- **Жест панели** (перетаскивание подписи поля, движение в палитре): `Phase::Preview` кладёт правку в превью сцены, считая её от свойств на начало жеста (относительные правки не накапливаются), `Phase::Commit` пишет одну транзакцию, `Phase::Cancel` убирает превью. Нажатие, клавиша или потеря фокуса на холсте, команда редактора, `select`, `transact`, отмена и переключение страницы отменяют жест панели.
- **Слои:** `set_flags` (скрыть, заблокировать), `rename_node` (пустое или то же имя — не правка), `move_layers` — перенос блоком к строке (над ней, под ней, внутрь фрейма или группы) в прежнем взаимном порядке; узлы, сменившие родителя, остаются на месте на экране (`reposition`). Перенос в себя, в своего потомка или в лист не делается; бросок туда, где слои и стоят, — тоже не правка.

**Files:**
- Create: `crates/ferrum-editor/src/panel.rs`
- Modify: `crates/ferrum-editor/src/editor.rs`
- Modify: `crates/ferrum-editor/src/lib.rs`
- Test: `crates/ferrum-editor/tests/panel.rs`

**Interfaces:**
- Consumes: `geometry::{bases, scale_patches_in, rigid_patches, top_level, MIN_SIDE}`, `ops::reposition`, `Ctx::commit`, `Scene::set_preview` этапов 2–3; `clean_name` (Task 1).
- Produces: `NumField`, `PanelEdit { Number, Fills, Strokes, ClipContent, Size }`, `Phase { Preview, Commit, Cancel }`, `DropTarget { Above, Below, Into }`, `PanelPose`, `panel_pose(scene, slot)`, `display_rotation(deg)`, `field_applies(field, kind)`; `Editor::{edit_selection, set_flags, rename_node, move_layers}`. `Editor::cancel_gesture` и `execute` отменяют и жест панели.

- [ ] **Step 1: Написать падающие тесты**

`crates/ferrum-editor/tests/panel.rs`:

````rust
//! Правки из панелей (этап 5): поля панели свойств, палитра, кнопки и
//! переименование слоёв, перетаскивание слоёв. Каждая правка — один шаг отмены;
//! жест поля или палитры — превью и один шаг при отпускании.

mod common;

use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch, Stroke, StrokeAlign};
use ferrum_editor::*;
use kurbo::Point;

fn num(field: NumField, value: f64) -> PanelEdit {
    PanelEdit::Number {
        field,
        value,
        relative: false,
    }
}

fn add(field: NumField, value: f64) -> PanelEdit {
    PanelEdit::Number {
        field,
        value,
        relative: true,
    }
}

fn center(h: &H, id: ferrum_doc::NodeId) -> Point {
    let n = h.ed.scene().node(id).unwrap();
    n.world * n.local_bounds.unwrap().center()
}

#[test]
fn typed_value_is_one_step_for_every_selected() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 0.0, 50.0, 50.0);
    let b = h.rect(None, 50.0, 0.0, 50.0, 50.0);
    h.ed.select(&[a, b]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::X, 100.0), Phase::Commit);
    assert_eq!((h.props(a).x, h.props(b).x), (100.0, 100.0));
    h.assert_one_step(steps, &before);
    // «Смешанные»: прибавка к своему значению у каждого.
    h.ed.edit_selection(&num(NumField::Y, 7.0), Phase::Commit);
    h.set(
        b,
        PropsPatch {
            y: Some(20.0),
            ..Default::default()
        },
    );
    h.ed.edit_selection(&add(NumField::Y, 5.0), Phase::Commit);
    assert_eq!((h.props(a).y, h.props(b).y), (12.0, 25.0));
    assert_eq!(h.sel(), vec![a, b], "выделение остаётся");
}

#[test]
fn width_resizes_node_and_scales_group_children() {
    let mut h = H::new();
    let r = h.rect(None, 10.0, 20.0, 50.0, 40.0);
    h.ed.select(&[r]);
    h.ed.edit_selection(&num(NumField::W, 200.0), Phase::Commit);
    let p = h.props(r);
    assert_eq!(
        (p.x, p.y, p.w, p.h),
        (10.0, 20.0, 200.0, 40.0),
        "угол на месте"
    );
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 30.0, 0.0, 10.0, 10.0);
    h.ed.select(&[a, b]);
    h.key("KeyG", ctrl());
    let g = h.sel()[0];
    assert_eq!(h.props(g).kind, NodeKind::Group);
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::W, 80.0), Phase::Commit);
    assert_eq!(h.ed.step_count(), steps + 1);
    let pose = panel_pose(h.ed.scene(), h.ed.scene().slot(g).unwrap());
    assert!(close(pose.w, 80.0) && close(pose.h, 10.0), "{pose:?}");
    assert!(close(h.props(b).x, 60.0) && close(h.props(b).w, 20.0));
    let line = h.add(None, NodeKind::Line, patch(0.0, 300.0, 100.0, 0.0));
    h.ed.select(&[line]);
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::H, 50.0), Phase::Commit);
    assert_eq!(
        (h.ed.step_count(), h.props(line).h),
        (steps, 0.0),
        "у линии нет высоты"
    );
}

#[test]
fn rotation_turns_around_center() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 100.0, 50.0);
    h.ed.select(&[r]);
    let c = center(&h, r);
    h.ed.edit_selection(&num(NumField::Rotation, 90.0), Phase::Commit);
    assert!(close(h.props(r).rotation, 90.0));
    let c2 = center(&h, r);
    assert!(close(c.x, c2.x) && close(c.y, c2.y), "{c:?} → {c2:?}");
    h.ed.edit_selection(&num(NumField::Rotation, -90.0), Phase::Commit);
    assert!(close(h.props(r).rotation, 270.0));
    assert_eq!(display_rotation(270.0), -90.0, "панель показывает −90°");
}

#[test]
fn opacity_radius_and_stroke_apply_where_they_make_sense() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let e = h.add(None, NodeKind::Ellipse, patch(20.0, 0.0, 10.0, 10.0));
    h.ed.select(&[r, e]);
    h.ed.edit_selection(&num(NumField::Opacity, 1.7), Phase::Commit);
    assert_eq!((h.props(r).opacity, h.props(e).opacity), (1.0, 1.0));
    h.ed.edit_selection(&num(NumField::Opacity, 0.25), Phase::Commit);
    assert_eq!(h.props(e).opacity, 0.25);
    h.ed.edit_selection(&num(NumField::CornerRadius, 6.0), Phase::Commit);
    assert_eq!(
        (h.props(r).corner_radius, h.props(e).corner_radius),
        (6.0, 0.0)
    );
    let stroke = Stroke {
        color: Color::BLACK,
        width: 2.0,
        align: StrokeAlign::Inside,
    };
    h.ed.edit_selection(&PanelEdit::Strokes(vec![stroke]), Phase::Commit);
    h.ed.edit_selection(&add(NumField::StrokeWidth, 3.0), Phase::Commit);
    assert_eq!(h.props(r).strokes[0].width, 5.0);
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::X, f64::NAN), Phase::Commit);
    assert_eq!(h.ed.step_count(), steps, "NaN не правка");
}

#[test]
fn scrub_is_preview_then_one_step() {
    let mut h = H::new();
    let r = h.rect(None, 10.0, 0.0, 50.0, 50.0);
    h.ed.select(&[r]);
    let before = h.state();
    let steps = h.ed.step_count();
    for d in [1.0, 4.0, 9.0] {
        h.ed.edit_selection(&add(NumField::X, d), Phase::Preview);
        assert_eq!(h.props(r).x, 10.0, "документ не меняется во время жеста");
        assert_eq!(
            h.ed.scene().node(r).unwrap().props.x,
            10.0 + d,
            "превью от начала жеста"
        );
    }
    assert_eq!(h.ed.step_count(), steps);
    h.ed.edit_selection(&add(NumField::X, 9.0), Phase::Commit);
    assert_eq!(h.props(r).x, 19.0);
    h.assert_one_step(steps, &before);
}

#[test]
fn scrub_cancel_and_canvas_input_leave_no_history() {
    let mut h = H::new();
    let r = h.rect(None, 10.0, 0.0, 50.0, 50.0);
    h.ed.select(&[r]);
    let steps = h.ed.step_count();
    h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Preview);
    h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Cancel);
    assert_eq!(h.ed.step_count(), steps);
    h.assert_scene_clean();
    h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Preview);
    h.down(500.0, 500.0, none());
    assert_eq!(
        h.ed.scene().node(r).unwrap().props.x,
        10.0,
        "превью снято нажатием"
    );
    h.up(500.0, 500.0, none());
    assert_eq!(
        h.ed.step_count(),
        steps,
        "ввод на холсте отменяет жест панели"
    );
    h.assert_scene_clean();
}

#[test]
fn fill_preview_and_commit_skip_groups() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 20.0, 0.0, 10.0, 10.0);
    h.ed.select(&[a, b]);
    h.key("KeyG", ctrl());
    let g = h.sel()[0];
    let c = h.rect(None, 50.0, 0.0, 10.0, 10.0);
    h.ed.select(&[g, c]);
    let red = vec![Fill::solid(Color::rgba(255, 0, 0, 255))];
    let steps = h.ed.step_count();
    h.ed.edit_selection(&PanelEdit::Fills(red.clone()), Phase::Preview);
    assert_eq!(h.ed.scene().node(c).unwrap().props.fills, red);
    assert_ne!(h.props(c).fills, red);
    h.ed.edit_selection(&PanelEdit::Fills(red.clone()), Phase::Commit);
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(h.props(c).fills, red);
    assert!(h.props(g).fills.is_empty(), "у группы заливок нет");
    h.ed.select(&[c]);
    h.ed.edit_selection(&PanelEdit::ClipContent(false), Phase::Commit);
    assert_eq!(h.ed.step_count(), steps + 1, "обрезка — только у фреймов");
}

#[test]
fn flags_and_rename_are_one_step_each() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.set_flags(&[r], Some(false), None);
    assert!(!h.props(r).visible);
    h.assert_one_step(steps, &before);
    h.ed.set_flags(&[r], None, Some(true));
    assert!(h.props(r).locked);
    let steps = h.ed.step_count();
    h.ed.rename_node(r, "  Кнопка  ");
    assert_eq!(h.props(r).name, "Кнопка");
    h.ed.rename_node(r, "");
    h.ed.rename_node(r, "Кнопка");
    assert_eq!(
        h.ed.step_count(),
        steps + 1,
        "пустое и то же имя — не правка"
    );
}

#[test]
fn move_layers_reorders_within_parent() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 20.0, 0.0, 10.0, 10.0);
    let c = h.rect(None, 40.0, 0.0, 10.0, 10.0);
    let before = h.state();
    let steps = h.ed.step_count();
    // Строки сверху вниз: c, b, a. «a» над «c» — на передний план.
    assert!(h.ed.move_layers(&[a], DropTarget::Above(c)));
    assert_eq!(h.children(None), vec![b, c, a]);
    h.assert_one_step(steps, &before);
    assert!(h.ed.move_layers(&[a], DropTarget::Below(b)));
    assert_eq!(h.children(None), vec![a, b, c]);
    assert_eq!(h.sel(), vec![a], "перенесённые выделены");
}

#[test]
fn move_into_frame_keeps_screen_position() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 300.0, 300.0);
    h.set(
        f,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    let r = h.rect(None, 150.0, 120.0, 20.0, 20.0);
    let world = h.ed.scene().node(r).unwrap().world;
    assert!(h.ed.move_layers(&[r], DropTarget::Into(f)));
    assert_eq!(h.parent(r), Some(f));
    let after = h.ed.scene().node(r).unwrap().world;
    for (x, y) in world.as_coeffs().iter().zip(after.as_coeffs()) {
        assert!(close(*x, y), "{world:?} → {after:?}");
    }
    assert!(
        !h.ed.move_layers(&[f], DropTarget::Into(r)),
        "в лист нельзя"
    );
    assert!(
        !h.ed.move_layers(&[f], DropTarget::Into(f)),
        "в себя нельзя"
    );
    let g = h.frame(Some(f), 0.0, 0.0, 50.0, 50.0);
    assert!(
        !h.ed.move_layers(&[f], DropTarget::Above(g)),
        "внутрь своего потомка нельзя"
    );
    assert_eq!(h.parent(g), Some(f));
}

#[test]
fn preset_size_is_one_step_for_frame_and_group() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 100.0, 100.0);
    h.ed.select(&[f]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.edit_selection(&PanelEdit::Size { w: 393.0, h: 852.0 }, Phase::Commit);
    let p = h.props(f);
    assert_eq!((p.w, p.h), (393.0, 852.0));
    h.assert_one_step(steps, &before);
    let a = h.rect(None, 500.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 530.0, 30.0, 10.0, 10.0);
    h.ed.select(&[a, b]);
    h.key("KeyG", ctrl());
    let g = h.sel()[0];
    h.ed.edit_selection(&PanelEdit::Size { w: 80.0, h: 20.0 }, Phase::Commit);
    let pose = panel_pose(h.ed.scene(), h.ed.scene().slot(g).unwrap());
    assert!(close(pose.w, 80.0) && close(pose.h, 20.0), "{pose:?}");
}

#[test]
fn drop_in_place_and_same_value_make_no_step() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(None, 20.0, 0.0, 10.0, 10.0);
    let steps = h.ed.step_count();
    // «a» уже под «b»: бросить её под «b» — ничего не менять.
    assert!(!h.ed.move_layers(&[a], DropTarget::Below(b)));
    h.ed.select(&[a]);
    h.ed.edit_selection(&num(NumField::Opacity, 1.0), Phase::Commit);
    h.ed.set_flags(&[a], Some(true), Some(false));
    assert_eq!(
        h.ed.step_count(),
        steps,
        "то же место и те же значения — не шаг"
    );
}

#[test]
fn command_during_panel_gesture_cancels_it() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[a]);
    h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Preview);
    h.ed.execute(CommandId::Delete);
    h.ed.take_events();
    // Отпускание после команды — без ошибки «узел не найден» и без правки.
    h.ed.edit_selection(&num(NumField::X, 300.0), Phase::Commit);
    assert!(!h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::Error(_))));
    assert!(h.children(None).is_empty());
    h.assert_scene_clean();
}
````

Run: `cargo test -p ferrum-editor --test panel`
Expected: FAIL — не компилируется: нет `NumField`, `PanelEdit`, `Phase`, `DropTarget`, методов `edit_selection`, `move_layers`.

- [ ] **Step 2: Правки из панелей**

`crates/ferrum-editor/src/panel.rs`:

````rust
//! Правки из панелей (этап 5): поля и палитра панели свойств, кнопки и
//! переименование в слоях, перетаскивание слоёв. Каждая правка — одна транзакция,
//! то есть один шаг отмены.
//!
//! Перетаскивание подписи поля и движение в палитре цвета — жест, как на холсте:
//! пока он идёт, изменения живут только в превью сцены (`Phase::Preview`), а
//! отпускание записывает одну транзакцию (`Phase::Commit`); `Phase::Cancel`
//! (Esc) убирает превью без следа в истории. Каждое превью считается от свойств
//! на начало жеста, поэтому относительные правки (`relative`) не накапливаются.

use std::collections::HashSet;

use ferrum_doc::{Fill, NodeId, NodeKind, Props, PropsPatch, Stroke};
use ferrum_scene::shape::COORD_LIMIT;
use ferrum_scene::{Scene, Slot};
use kurbo::{Affine, Point, Rect};

use crate::editor::Editor;
use crate::geometry::{
    bases, normalize_deg, parent_world, rigid_patches, scale_patches_in, top_level, Base,
    PixelSnap, MIN_SIDE,
};
use crate::ops::reposition;
use crate::pages::clean_name;

/// Числовое поле панели свойств.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumField {
    X,
    Y,
    W,
    H,
    /// Градусы против часовой стрелки, как в Figma.
    Rotation,
    /// Прозрачность слоя, от 0 до 1.
    Opacity,
    CornerRadius,
    /// Толщина всех обводок узла.
    StrokeWidth,
}

/// Правка выделения из панели свойств.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelEdit {
    /// Значение поля у каждого выделенного узла; `relative` — прибавить к его
    /// собственному значению (перетаскивание подписи у «Смешанных»).
    Number {
        field: NumField,
        value: f64,
        relative: bool,
    },
    /// Заменить заливки целиком (у групп и линий заливок нет — пропускаются).
    Fills(Vec<Fill>),
    /// Заменить обводки целиком (у групп обводок нет).
    Strokes(Vec<Stroke>),
    /// Обрезка содержимого — только у фреймов.
    ClipContent(bool),
    /// Ширина и высота сразу (пресет фрейма) — одним шагом.
    Size { w: f64, h: f64 },
}

/// Фаза правки: превью жеста, запись одним шагом или отмена жеста.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Preview,
    Commit,
    Cancel,
}

/// Значения полей «Положение» узла, как их показывает панель: у группы — рамка её
/// детей в координатах родителя, у остальных — свойства узла. Поворот — в
/// `(-180, 180]`, как в Figma.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelPose {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rotation: f64,
}

/// Поворот для панели: `[0, 360)` → `(-180, 180]`.
pub fn display_rotation(deg: f64) -> f64 {
    let r = normalize_deg(deg);
    if r > 180.0 {
        r - 360.0
    } else {
        r
    }
}

pub fn panel_pose(scene: &Scene, slot: Slot) -> PanelPose {
    let n = scene.at(slot);
    let p = &n.props;
    if p.kind == NodeKind::Group {
        let b = n.local_bounds.unwrap_or(Rect::ZERO);
        let corner = n.local * Point::new(b.x0, b.y0);
        return PanelPose {
            x: corner.x,
            y: corner.y,
            w: b.width(),
            h: b.height(),
            rotation: display_rotation(p.rotation),
        };
    }
    PanelPose {
        x: p.x,
        y: p.y,
        w: p.w,
        h: p.h,
        rotation: display_rotation(p.rotation),
    }
}

/// Может ли поле относиться к узлу этого типа.
pub fn field_applies(field: NumField, kind: NodeKind) -> bool {
    match field {
        NumField::H => kind != NodeKind::Line,
        NumField::CornerRadius => {
            matches!(kind, NodeKind::Frame | NodeKind::Rect | NodeKind::Image)
        }
        NumField::StrokeWidth => kind != NodeKind::Group,
        _ => true,
    }
}

/// Узел на начало жеста панели: свойства документа, матрицы и рамка.
#[derive(Debug, Clone)]
struct Target {
    id: NodeId,
    props: Props,
    local: Affine,
    parent_world: Affine,
    world: Affine,
    bounds: Option<Rect>,
    pose: PanelPose,
    /// Поддерево для масштаба (группа масштабирует детей).
    bases: Vec<Base>,
}

/// Жест панели: узлы и их состояние на начало.
#[derive(Debug, Clone, Default)]
pub(crate) struct PanelGesture {
    targets: Vec<Target>,
}

fn capture(scene: &Scene, ids: &[NodeId]) -> PanelGesture {
    let targets = ids
        .iter()
        .filter(|id| !scene.is_draft(**id))
        .filter_map(|id| scene.slot(*id).map(|s| (*id, s)))
        .map(|(id, slot)| {
            let n = scene.at(slot);
            Target {
                id,
                props: n.props.clone(),
                local: n.local,
                parent_world: parent_world(scene, slot),
                world: n.world,
                bounds: n.local_bounds,
                pose: panel_pose(scene, slot),
                bases: bases(scene, &[id], true),
            }
        })
        .collect();
    PanelGesture { targets }
}

fn finite_or(v: f64, base: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        base
    }
}

/// Новое значение поля: абсолютное или прибавка к своему.
fn target_value(base: f64, value: f64, relative: bool) -> f64 {
    finite_or(if relative { base + value } else { value }, base)
}

/// Новые ширина и (или) высота узла. У фрейма, фигуры и изображения меняются
/// `w`, `h`, левый верхний угол в осях узла остаётся на месте (как в Figma). У
/// группы своих размеров нет: масштабируются дети в её осях от левого верхнего
/// угла рамки.
fn size_patches(t: &Target, w: Option<f64>, h: Option<f64>) -> Vec<(NodeId, PropsPatch)> {
    let side = |v: f64| v.clamp(MIN_SIDE, COORD_LIMIT);
    let w = w.filter(|v| v.is_finite()).map(side);
    let h = h
        .filter(|v| v.is_finite() && t.props.kind != NodeKind::Line)
        .map(side);
    if w.is_none() && h.is_none() {
        return vec![];
    }
    if t.props.kind != NodeKind::Group {
        return vec![(
            t.id,
            PropsPatch {
                w,
                h,
                ..Default::default()
            },
        )];
    }
    let Some(b) = t.bounds.filter(|b| b.width() > 0.0 && b.height() > 0.0) else {
        return vec![];
    };
    let sx = w.map_or(1.0, |w| w / b.width());
    let sy = h.map_or(1.0, |h| h / b.height());
    let local = Affine::translate((b.x0, b.y0))
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate((-b.x0, -b.y0));
    let world = t.world * local * t.world.inverse();
    scale_patches_in(&t.bases, world, t.world, PixelSnap { x: false, y: false })
}

/// Правки свойств одного узла для числового поля.
fn number_patches(
    t: &Target,
    field: NumField,
    value: f64,
    relative: bool,
) -> Vec<(NodeId, PropsPatch)> {
    if !value.is_finite() || !field_applies(field, t.props.kind) {
        return vec![];
    }
    let p = &t.props;
    let one = |patch: PropsPatch| vec![(t.id, patch)];
    match field {
        NumField::X | NumField::Y => {
            let (base, cur) = if field == NumField::X {
                (t.pose.x, p.x)
            } else {
                (t.pose.y, p.y)
            };
            let next =
                (cur + target_value(base, value, relative) - base).clamp(-COORD_LIMIT, COORD_LIMIT);
            one(if field == NumField::X {
                PropsPatch {
                    x: Some(next),
                    ..Default::default()
                }
            } else {
                PropsPatch {
                    y: Some(next),
                    ..Default::default()
                }
            })
        }
        NumField::W => size_patches(t, Some(target_value(t.pose.w, value, relative)), None),
        NumField::H => size_patches(t, None, Some(target_value(t.pose.h, value, relative))),
        NumField::Rotation => {
            let delta = target_value(t.pose.rotation, value, relative) - t.pose.rotation;
            if delta.abs() < 1e-12 {
                return vec![];
            }
            // Поворот вокруг центра рамки (как в Figma); в модели — против часовой
            // стрелки вокруг левого верхнего угла, поэтому меняются и x, y.
            let c = t.local * t.bounds.unwrap_or(Rect::ZERO).center();
            let in_parent = Affine::translate(c.to_vec2())
                * Affine::rotate(-delta.to_radians())
                * Affine::translate(-c.to_vec2());
            let world = t.parent_world * in_parent * t.parent_world.inverse();
            let top: Vec<Base> = t.bases.iter().take(1).cloned().collect();
            rigid_patches(&top, world, false)
        }
        NumField::Opacity => one(PropsPatch {
            opacity: Some(target_value(p.opacity, value, relative).clamp(0.0, 1.0)),
            ..Default::default()
        }),
        NumField::CornerRadius => one(PropsPatch {
            corner_radius: Some(
                target_value(p.corner_radius, value, relative).clamp(0.0, COORD_LIMIT),
            ),
            ..Default::default()
        }),
        NumField::StrokeWidth => {
            if p.strokes.is_empty() {
                return vec![];
            }
            let strokes = p
                .strokes
                .iter()
                .map(|s| Stroke {
                    width: target_value(s.width, value, relative).clamp(0.0, COORD_LIMIT),
                    ..s.clone()
                })
                .collect();
            one(PropsPatch {
                strokes: Some(strokes),
                ..Default::default()
            })
        }
    }
}

fn patches(g: &PanelGesture, edit: &PanelEdit) -> Vec<(NodeId, PropsPatch)> {
    let mut out = Vec::new();
    for t in &g.targets {
        let kind = t.props.kind;
        match edit {
            PanelEdit::Number {
                field,
                value,
                relative,
            } => out.extend(number_patches(t, *field, *value, *relative)),
            PanelEdit::Fills(fills) => {
                if kind != NodeKind::Group && kind != NodeKind::Line {
                    out.push((
                        t.id,
                        PropsPatch {
                            fills: Some(fills.clone()),
                            ..Default::default()
                        },
                    ));
                }
            }
            PanelEdit::Strokes(strokes) => {
                if kind != NodeKind::Group {
                    out.push((
                        t.id,
                        PropsPatch {
                            strokes: Some(strokes.clone()),
                            ..Default::default()
                        },
                    ));
                }
            }
            PanelEdit::Size { w, h } => {
                if w.is_finite() && h.is_finite() {
                    let h = field_applies(NumField::H, kind).then_some(*h);
                    out.extend(size_patches(t, Some(*w), h));
                }
            }
            PanelEdit::ClipContent(on) => {
                if kind == NodeKind::Frame {
                    out.push((
                        t.id,
                        PropsPatch {
                            clip_content: Some(*on),
                            ..Default::default()
                        },
                    ));
                }
            }
        }
    }
    // Узел мог попасть дважды (группа и её выделенный ребёнок при масштабе группы):
    // правки одного узла сливаются, последняя — главнее.
    let mut seen: HashSet<NodeId> = HashSet::new();
    let mut merged: Vec<(NodeId, PropsPatch)> = Vec::with_capacity(out.len());
    for (id, p) in out.into_iter().rev() {
        if seen.insert(id) {
            merged.push((id, p));
        }
    }
    merged.reverse();
    merged
}

/// Куда положить слои, перетащенные в панели слоёв (строки сверху вниз — от
/// переднего плана к заднему).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    /// Над строкой: перед узлом, у того же родителя.
    Above(NodeId),
    /// Под строкой: за узлом, у того же родителя.
    Below(NodeId),
    /// Внутрь фрейма или группы, поверх его детей.
    Into(NodeId),
}

impl Editor {
    /// Правка выделения из панели свойств (см. описание модуля). Жест холста,
    /// если он идёт, отменяется.
    pub fn edit_selection(&mut self, edit: &PanelEdit, phase: Phase) {
        match phase {
            Phase::Cancel => self.cancel_panel(),
            Phase::Preview => {
                if self.panel.is_none() {
                    self.cancel_gesture();
                    self.panel = Some(capture(&self.scene, &self.selection));
                }
                let p = patches(self.panel.as_ref().expect("задан выше"), edit);
                self.scene.set_preview(&p);
            }
            Phase::Commit => {
                let g = match self.panel.take() {
                    Some(g) => g,
                    None => {
                        self.cancel_gesture();
                        capture(&self.scene, &self.selection)
                    }
                };
                let p = patches(&g, edit);
                self.scene.clear_preview();
                if !p.is_empty() {
                    let selection = self.selection.to_vec();
                    self.ctx().commit(|tx, _| {
                        for (id, patch) in &p {
                            tx.set_props(*id, patch)?;
                        }
                        Ok(selection)
                    });
                }
            }
        }
        self.emit();
    }

    /// Убирает превью жеста панели без записи.
    pub(crate) fn cancel_panel(&mut self) {
        if self.panel.take().is_some() {
            self.scene.clear_preview();
        }
    }

    /// Скрыть или заблокировать узлы (кнопки строки слоя) одним шагом.
    pub fn set_flags(&mut self, ids: &[NodeId], visible: Option<bool>, locked: Option<bool>) {
        self.cancel_gesture();
        let ids = self.alive(ids);
        if ids.is_empty() || (visible.is_none() && locked.is_none()) {
            return;
        }
        let patch = PropsPatch {
            visible,
            locked,
            ..Default::default()
        };
        let selection = self.selection.to_vec();
        self.ctx().commit(|tx, _| {
            for id in &ids {
                tx.set_props(*id, &patch)?;
            }
            Ok(selection)
        });
        self.emit();
    }

    /// Переименование слоя одним шагом; пустое или то же имя ничего не меняет.
    pub fn rename_node(&mut self, id: NodeId, name: &str) {
        self.cancel_gesture();
        let Some(name) = clean_name(name) else {
            return;
        };
        if self.scene.node(id).is_none_or(|n| n.props.name == name) || self.scene.is_draft(id) {
            return;
        }
        let selection = self.selection.to_vec();
        self.ctx().commit(|tx, _| {
            tx.set_props(
                id,
                &PropsPatch {
                    name: Some(name),
                    ..Default::default()
                },
            )?;
            Ok(selection)
        });
        self.emit();
    }

    /// Порядок и вложенность из панели слоёв: узлы `ids` (без тех, у кого выделен
    /// предок) встают блоком к цели в прежнем взаимном порядке и остаются на месте
    /// на экране. Один шаг; перенос узла внутрь самого себя или в лист ничего не
    /// делает. Возвращает, изменился ли документ.
    pub fn move_layers(&mut self, ids: &[NodeId], target: DropTarget) -> bool {
        self.cancel_gesture();
        let ids = top_level(&self.scene, &self.alive(ids));
        let anchor = match target {
            DropTarget::Above(a) | DropTarget::Below(a) | DropTarget::Into(a) => a,
        };
        let Some(anchor_slot) = self.scene.slot(anchor) else {
            return false;
        };
        let parent = match target {
            DropTarget::Into(a) => {
                let kind = self.scene.at(anchor_slot).props.kind;
                if kind != NodeKind::Frame && kind != NodeKind::Group {
                    return false;
                }
                Some(a)
            }
            _ => self
                .scene
                .at(anchor_slot)
                .parent
                .map(|p| self.scene.at(p).id),
        };
        // Цель не может лежать внутри переносимых (и быть одним из них при «внутрь»).
        let moving: HashSet<NodeId> = ids.iter().copied().collect();
        let mut cur = parent.and_then(|p| self.scene.slot(p));
        while let Some(s) = cur {
            if moving.contains(&self.scene.at(s).id) {
                return false;
            }
            cur = self.scene.at(s).parent;
        }
        if ids.is_empty() || (target != DropTarget::Into(anchor) && moving.contains(&anchor)) {
            return false;
        }
        // Блок — в порядке отрисовки (снизу вверх), как стоят сейчас.
        let mut block: Vec<(Vec<u32>, NodeId)> = ids
            .iter()
            .map(|id| {
                (
                    self.scene.order_key(self.scene.slot(*id).expect("живой")),
                    *id,
                )
            })
            .collect();
        block.sort_by(|a, b| a.0.cmp(&b.0));
        let block: Vec<NodeId> = block.into_iter().map(|(_, id)| id).collect();
        let parent_world = parent
            .and_then(|p| self.scene.slot(p))
            .map_or(Affine::IDENTITY, |s| self.scene.at(s).world);
        // Положение на экране: узлам, у которых меняется родитель, — новые x, y,
        // поворот и отражение в координатах нового родителя.
        let repos: Vec<(NodeId, PropsPatch)> = block
            .iter()
            .filter_map(|id| {
                let s = self.scene.slot(*id)?;
                let n = self.scene.at(s);
                let old = n.parent.map(|p| self.scene.at(p).id);
                (old != parent).then(|| (*id, reposition(parent_world, n.world, &n.props)))
            })
            .collect();
        // Позиция блока среди остальных детей цели.
        let place = |others: &[NodeId]| match target {
            DropTarget::Into(_) => others.len(),
            DropTarget::Above(a) => others
                .iter()
                .position(|c| *c == a)
                .map_or(others.len(), |i| i + 1),
            DropTarget::Below(a) => others.iter().position(|c| *c == a).unwrap_or(0),
        };
        // Бросили туда, где слои и стоят, — правки нет (и шага отмены тоже).
        let current = self.page_doc().children(parent);
        let mut want: Vec<NodeId> = current
            .iter()
            .copied()
            .filter(|c| !moving.contains(c))
            .collect();
        let at = place(&want);
        want.splice(at..at, block.iter().copied());
        if repos.is_empty() && want == current {
            return false;
        }
        let steps = self.out.steps;
        let selection = block.clone();
        self.ctx().commit(|tx, _| {
            let others: Vec<NodeId> = tx
                .children(parent)
                .into_iter()
                .filter(|c| !moving.contains(c))
                .collect();
            tx.move_nodes(&block, parent, place(&others))?;
            for (id, patch) in &repos {
                tx.set_props(*id, patch)?;
            }
            Ok(selection)
        });
        self.emit();
        self.out.steps != steps
    }
}
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
use crate::keymap::Keymap;
use crate::tools::{
````

на:

````rust
use crate::keymap::Keymap;
use crate::panel::PanelGesture;
use crate::tools::{
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    seen: Seen,
}
````

на:

````rust
    seen: Seen,
    /// Жест панели свойств (перетаскивание поля, палитра): свойства на начало.
    pub(crate) panel: Option<PanelGesture>,
}
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            seen,
        })
````

на:

````rust
            seen,
            panel: None,
        })
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        }
        match input {
````

на:

````rust
        }
        // Нажатие, клавиша или потеря фокуса на холсте во время жеста панели —
        // жест панели отменяется: его превью и жест холста не живут вместе.
        if !matches!(
            input,
            Input::PointerMove(_) | Input::Wheel(_) | Input::Pinch { .. }
        ) {
            self.cancel_panel();
        }
        match input {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust

    /// Отменяет жест: превью убирается, документ и история не меняются.
    pub fn cancel_gesture(&mut self) {
        if let Some(mut hand) = self.pan.take() {
````

на:

````rust

    /// Отменяет жест (холста или панели): превью убирается, документ и история
    /// не меняются.
    pub fn cancel_gesture(&mut self) {
        self.cancel_panel();
        if let Some(mut hand) = self.pan.take() {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    pub fn execute(&mut self, cmd: CommandId) {
        if self.is_busy() {
````

на:

````rust
    pub fn execute(&mut self, cmd: CommandId) {
        // Жест панели (поле, палитра) держит узлы на начало жеста: команда могла бы
        // их удалить или сдвинуть, поэтому он отменяется.
        self.cancel_panel();
        if self.is_busy() {
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod pages;
pub mod presets;
````

на:

````rust
pub mod pages;
pub mod panel;
pub mod presets;
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub use pages::{clean_name, MAX_NAME_CHARS};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
````

на:

````rust
pub use pages::{clean_name, MAX_NAME_CHARS};
pub use panel::{
    display_rotation, field_applies, panel_pose, DropTarget, NumField, PanelEdit, PanelPose, Phase,
};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
````

Run: `cargo test -p ferrum-editor --test panel`
Expected: PASS, `13 passed`.

Run: `cargo test -p ferrum-editor`
Expected: PASS — все наборы редактора.

Run: `cargo clippy -p ferrum-editor --all-targets -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 3: Закоммитить**

```bash
git add crates/ferrum-editor
git commit -m "feat(editor): правки из панелей — поля, заливки, слои, жест с превью"
```

---

### Task 3: Дерево слоёв в worker'е

Основной поток не держит дерево слоёв. Он сообщает окно строк, которое видно (`start`, `count` ≤ 200), а worker разворачивает дерево с учётом раскрытых узлов и поиска и отдаёт только эти строки. Строки идут сверху вниз, как в Figma: верхний (передний) узел первым.

Список строк страницы (узел сцены и глубина) строится заново лишь при изменении структуры — правке документа, другой странице, раскрытии, поиске — и стоит O(строк), а не O(узлов): свёрнутый фрейм на 100 узлов — одна строка, страница из 100 000 узлов с одним раскрытым фреймом — около 1100 строк. Поиск (без учёта регистра) показывает совпавшие узлы и всех их предков, всё раскрыто; он обходит страницу целиком, но только при изменении запроса или документа. Черновики жеста (рисуемая фигура, копии с Alt) в дерево не попадают.

`window` отдаёт строки, только если окно изменилось с прошлой отправки: имя, тип, глубина, есть ли дети, раскрыт ли, выделен ли, выделен ли предок (слабая подсветка), скрыт ли и заблокирован ли он сам и его предки. `range` — строки от одной до другой (выделение с Shift); `reveal` раскрывает предков первых 100 выделенных с холста и возвращает строку первого — панель прокрутит к ней.

**Files:**
- Modify: `crates/ferrum-wasm/Cargo.toml`
- Create: `crates/ferrum-wasm/src/layers.rs`
- Modify: `crates/ferrum-wasm/src/lib.rs`
- Test: `crates/ferrum-wasm/tests/layers.rs`

**Interfaces:**
- Consumes: `Scene::{roots, at, slot, is_draft}`, `fixture::fill` (тесты).
- Produces: `layers::{Layers, LayerRow, LayerWindow, NodeType, MAX_LAYER_WINDOW, MAX_FILTER_CHARS, MAX_ROW_NAME_CHARS, REVEAL_MAX}`; `Layers::{new, invalidate, reset, set_window, set_expanded, set_filter, set_anchor, anchor, total, range, reveal, window, resend}`. `LayerRow` и `NodeType` — с `Tsify` (типы для TypeScript).

- [ ] **Step 1: Зависимость `ferrum-scene`**

В `crates/ferrum-wasm/Cargo.toml` заменить:

````toml
ferrum-render = { path = "../ferrum-render" }
kurbo.workspace = true
````

на:

````toml
ferrum-render = { path = "../ferrum-render" }
ferrum-scene = { path = "../ferrum-scene" }
kurbo.workspace = true
````

Run: `cargo metadata --format-version 1 > /dev/null`
Expected: без вывода; в `Cargo.lock` у `ferrum-wasm` появилась зависимость `ferrum-scene` (других изменений нет: крейт уже в workspace).

- [ ] **Step 2: Написать падающие тесты**

`crates/ferrum-wasm/tests/layers.rs`:

````rust
//! Дерево слоёв в worker'е: окно строк ограничено, свёрнутый фрейм — одна строка,
//! раскрытие, поиск с предками, диапазон для Shift, раскрытие предков выделения
//! с холста; одинаковое окно второй раз не отправляется.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashSet;

use ferrum_doc::{fixture, Document, NodeId};
use ferrum_scene::Scene;
use ferrum_wasm::layers::{Layers, NodeType, MAX_LAYER_WINDOW};

/// Страница-пример: фреймы «Экран N» по 99 фигур «Слой i».
fn page(nodes: usize) -> (Scene, Vec<NodeId>) {
    let mut doc = Document::new();
    let p = doc.pages()[0].0;
    let mut ids = vec![];
    doc.transact(p, vec![], |tx| {
        ids = fixture::fill(tx, nodes)?;
        Ok(vec![])
    })
    .unwrap();
    (Scene::from_page(doc.page(p).unwrap()), ids)
}

fn none() -> HashSet<NodeId> {
    HashSet::new()
}

#[test]
fn collapsed_frames_are_one_row_each_top_first() {
    let (scene, _) = page(1000);
    let mut l = Layers::new();
    l.set_window(0, 5);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.total, 10, "10 фреймов, дети свёрнуты");
    assert_eq!(w.rows.len(), 5);
    assert_eq!(w.rows[0].name, "Экран 9", "верхний — первым");
    assert_eq!((w.rows[0].kind, w.rows[0].depth), (NodeType::Frame, 0));
    assert!(w.rows[0].has_children && !w.rows[0].expanded);
    assert!(
        l.window(&scene, &none()).is_none(),
        "то же окно не повторяется"
    );
    l.resend();
    assert!(l.window(&scene, &none()).is_some());
}

#[test]
fn window_is_capped_and_clamped() {
    let (scene, ids) = page(1000);
    let mut l = Layers::new();
    l.set_expanded(ids[0], true);
    l.set_window(0, 10_000);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.total, 10 + 99);
    assert!(w.rows.len() as u32 <= MAX_LAYER_WINDOW);
    l.set_window(5000, 20);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(
        (w.start, w.rows.len()),
        (109, 0),
        "начало за концом — пустое окно"
    );
}

#[test]
fn expanded_frame_lists_children_top_first() {
    let (scene, ids) = page(200);
    let mut l = Layers::new();
    // ids[0] — «Экран 0», нижний фрейм; его строка — вторая.
    l.set_expanded(ids[0], true);
    l.set_window(0, 4);
    let w = l.window(&scene, &none()).unwrap();
    let names: Vec<&str> = w.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Экран 1", "Экран 0", "Слой 98", "Слой 97"]);
    assert_eq!(w.rows[2].depth, 1);
    let mut sel = HashSet::new();
    sel.insert(ids[0]);
    let w = l.window(&scene, &sel).unwrap();
    assert!(w.rows[1].selected && w.rows[2].parent_selected && !w.rows[2].selected);
}

#[test]
fn search_shows_matches_with_ancestors() {
    let (scene, _) = page(300);
    let mut l = Layers::new();
    l.set_filter("  слой 42 ");
    l.set_window(0, 50);
    let w = l.window(&scene, &none()).unwrap();
    // В каждом из трёх фреймов «Слой 42» и его фрейм.
    assert_eq!(w.total, 6);
    assert_eq!(w.rows[0].name, "Экран 2");
    assert!(w.rows[0].expanded, "при поиске всё раскрыто");
    assert_eq!((w.rows[1].name.as_str(), w.rows[1].depth), ("Слой 42", 1));
    l.set_filter("");
    assert_eq!(l.window(&scene, &none()).unwrap().total, 3);
}

#[test]
fn range_and_reveal() {
    let (scene, ids) = page(200);
    let mut l = Layers::new();
    // Выделение с холста внутри «Экран 0»: фрейм раскрывается, строка — 2-я снизу
    // вверх по фрейму.
    let child = ids[5];
    let at = l.reveal(&scene, &[child]).unwrap();
    l.set_window(at, 1);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.rows[0].id, child.to_string());
    // От «Экран 1» (первая строка) до «Слой 0» (нижний ребёнок «Экран 0»).
    let rows = l.range(&scene, ids[100], ids[1]);
    assert_eq!(rows.len(), 1 + 1 + 99, "оба фрейма и дети раскрытого");
    assert_eq!((rows[0], rows[100]), (ids[100], ids[1]));
    assert_eq!(
        l.range(&scene, ids[1], ids[100]),
        rows,
        "в любом направлении"
    );
    let missing = ids[150];
    assert_eq!(
        l.range(&scene, missing, ids[0]),
        vec![ids[0]],
        "свёрнутый — только цель"
    );
}

#[test]
fn rows_follow_frames_not_nodes() {
    let (scene, ids) = page(30_000);
    let mut l = Layers::new();
    l.set_expanded(ids[0], true);
    l.set_window(0, 40);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.total, 300 + 99);
    assert_eq!(w.rows.len(), 40);
    // Строк — по числу фреймов и раскрытых детей, не по числу узлов.
    assert_eq!(w.rows[1].name, "Экран 298");
}
````

Run: `cargo test -p ferrum-wasm --test layers`
Expected: FAIL — не компилируется: нет модуля `ferrum_wasm::layers`.

- [ ] **Step 3: Дерево слоёв**

`crates/ferrum-wasm/src/layers.rs`:

````rust
//! Дерево слоёв для панели (этап 5). Основной поток не держит дерево: он просит
//! окно видимых строк (`start`, `count`), а worker разворачивает дерево с учётом
//! раскрытых узлов и поиска и отдаёт только эти строки. Список строк страницы
//! (узел и глубина) строится заново лишь при изменении структуры — правке
//! документа, другой странице, раскрытии, поиске, — и стоит O(строк), а не
//! O(узлов страницы): свёрнутый фрейм на 100 узлов — одна строка. Поиск обходит
//! страницу целиком, но только при изменении запроса или документа.
//!
//! Строки идут сверху вниз, как в Figma: верхний (передний) узел первым.

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

/// Наибольшее окно строк за один запрос: панель высотой в экран — около 40 строк.
pub const MAX_LAYER_WINDOW: u32 = 200;
/// Наибольшая длина поискового запроса в символах.
pub const MAX_FILTER_CHARS: usize = 128;
/// Длина имени в строке слоя: имя из файла может быть любым.
pub const MAX_ROW_NAME_CHARS: usize = 256;
/// У скольких выделенных узлов раскрываются предки, когда выделение пришло с холста.
pub const REVEAL_MAX: usize = 100;

/// Тип узла для иконки строки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum NodeType {
    Frame,
    Group,
    Rect,
    Ellipse,
    Line,
    Image,
}

impl From<NodeKind> for NodeType {
    fn from(k: NodeKind) -> NodeType {
        match k {
            NodeKind::Frame => NodeType::Frame,
            NodeKind::Group => NodeType::Group,
            NodeKind::Rect => NodeType::Rect,
            NodeKind::Ellipse => NodeType::Ellipse,
            NodeKind::Line => NodeType::Line,
            NodeKind::Image => NodeType::Image,
        }
    }
}

/// Строка панели слоёв — всё, что нужно для отрисовки без обращения к документу.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct LayerRow {
    pub id: String,
    pub name: String,
    pub kind: NodeType,
    pub depth: u32,
    pub has_children: bool,
    pub expanded: bool,
    pub selected: bool,
    /// Выделен предок: строка подсвечена слабее, как в Figma.
    pub parent_selected: bool,
    pub visible: bool,
    pub locked: bool,
    /// Скрыт или заблокирован предок: значок строки тусклый.
    pub ancestor_hidden: bool,
    pub ancestor_locked: bool,
}

/// Окно строк для события `Layers`.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerWindow {
    pub start: u32,
    pub total: u32,
    pub rows: Vec<LayerRow>,
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[derive(Debug, Default)]
pub struct Layers {
    expanded: HashSet<NodeId>,
    /// Запрос поиска в нижнем регистре; пустой — без поиска.
    filter: String,
    rows: Vec<(Slot, u32)>,
    /// Список строк устарел (структура, раскрытие, поиск).
    stale: bool,
    start: u32,
    count: u32,
    last: Option<LayerWindow>,
    /// Начало диапазона для выделения с Shift.
    anchor: Option<NodeId>,
}

impl Layers {
    pub fn new() -> Layers {
        Layers {
            stale: true,
            ..Default::default()
        }
    }

    /// Структура страницы изменилась (правка, отмена, другая страница).
    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    /// Новая страница или документ: раскрытие и поиск сбрасываются.
    pub fn reset(&mut self) {
        self.expanded.clear();
        self.filter.clear();
        self.anchor = None;
        self.last = None;
        self.stale = true;
    }

    /// Окно, которое видит панель. Больше `MAX_LAYER_WINDOW` строк не отдаётся.
    pub fn set_window(&mut self, start: u32, count: u32) {
        self.start = start;
        self.count = count.min(MAX_LAYER_WINDOW);
    }

    pub fn set_expanded(&mut self, id: NodeId, expanded: bool) {
        let changed = if expanded {
            self.expanded.insert(id)
        } else {
            self.expanded.remove(&id)
        };
        self.stale |= changed;
    }

    /// Поиск по имени (без учёта регистра): показываются совпавшие узлы и их
    /// предки, всё раскрыто.
    pub fn set_filter(&mut self, query: &str) {
        let q = clip(query.trim(), MAX_FILTER_CHARS).to_lowercase();
        if q != self.filter {
            self.filter = q;
            self.stale = true;
        }
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn set_anchor(&mut self, id: Option<NodeId>) {
        self.anchor = id;
    }

    pub fn anchor(&self) -> Option<NodeId> {
        self.anchor
    }

    fn ensure(&mut self, scene: &Scene) {
        if !self.stale {
            return;
        }
        self.stale = false;
        // Узлы, удалённые из документа, не держат место в множестве.
        self.expanded.retain(|id| scene.slot(*id).is_some());
        self.rows.clear();
        if self.filter.is_empty() {
            self.flatten(scene);
        } else {
            self.flatten_filtered(scene);
        }
    }

    fn flatten(&mut self, scene: &Scene) {
        // Обход в глубину без рекурсии, сверху вниз: дети в стек в прямом порядке,
        // поэтому верхний (последний) ребёнок снимается первым.
        let mut stack: Vec<(Slot, u32)> = scene.roots().iter().map(|s| (*s, 0)).collect();
        while let Some((s, depth)) = stack.pop() {
            let n = scene.at(s);
            if scene.is_draft(n.id) {
                continue;
            }
            self.rows.push((s, depth));
            if !n.children.is_empty() && self.expanded.contains(&n.id) {
                stack.extend(n.children.iter().map(|c| (*c, depth + 1)));
            }
        }
    }

    fn flatten_filtered(&mut self, scene: &Scene) {
        // Сначала — какие узлы показать: совпавшие и все их предки.
        let mut keep: HashSet<Slot> = HashSet::new();
        let mut stack: Vec<Slot> = scene.roots().to_vec();
        while let Some(s) = stack.pop() {
            let n = scene.at(s);
            if !scene.is_draft(n.id) && n.props.name.to_lowercase().contains(&self.filter) {
                let mut cur = Some(s);
                while let Some(c) = cur {
                    if !keep.insert(c) {
                        break;
                    }
                    cur = scene.at(c).parent;
                }
            }
            stack.extend(n.children.iter().copied());
        }
        let mut stack: Vec<(Slot, u32)> = scene
            .roots()
            .iter()
            .filter(|s| keep.contains(s))
            .map(|s| (*s, 0))
            .collect();
        while let Some((s, depth)) = stack.pop() {
            self.rows.push((s, depth));
            stack.extend(
                scene
                    .at(s)
                    .children
                    .iter()
                    .filter(|c| keep.contains(c))
                    .map(|c| (*c, depth + 1)),
            );
        }
    }

    /// Всего строк (для высоты прокрутки).
    pub fn total(&mut self, scene: &Scene) -> u32 {
        self.ensure(scene);
        self.rows.len() as u32
    }

    /// Идентификаторы строк по порядку от `a` до `b` включительно (выделение с
    /// Shift). Узла нет среди строк — только `b`.
    pub fn range(&mut self, scene: &Scene, a: NodeId, b: NodeId) -> Vec<NodeId> {
        self.ensure(scene);
        let pos = |id: NodeId| self.rows.iter().position(|(s, _)| scene.at(*s).id == id);
        match (pos(a), pos(b)) {
            (Some(i), Some(j)) => {
                let (i, j) = (i.min(j), i.max(j));
                self.rows[i..=j]
                    .iter()
                    .map(|(s, _)| scene.at(*s).id)
                    .collect()
            }
            _ => vec![b],
        }
    }

    /// Раскрывает предков первых `REVEAL_MAX` выделенных (выделение с холста) и
    /// возвращает строку первого из них — панель прокрутит к ней.
    pub fn reveal(&mut self, scene: &Scene, ids: &[NodeId]) -> Option<u32> {
        for id in ids.iter().take(REVEAL_MAX) {
            let mut cur = scene.slot(*id).and_then(|s| scene.at(s).parent);
            while let Some(p) = cur {
                let pid = scene.at(p).id;
                if self.expanded.insert(pid) {
                    self.stale = true;
                }
                cur = scene.at(p).parent;
            }
        }
        let first = scene.slot(*ids.first()?)?;
        self.ensure(scene);
        self.rows
            .iter()
            .position(|(s, _)| *s == first)
            .map(|i| i as u32)
    }

    /// Строки текущего окна; `None` — те же, что ушли в прошлый раз.
    pub fn window(&mut self, scene: &Scene, selected: &HashSet<NodeId>) -> Option<LayerWindow> {
        self.ensure(scene);
        let total = self.rows.len() as u32;
        let start = self.start.min(total);
        let end = start.saturating_add(self.count).min(total);
        let searching = !self.filter.is_empty();
        let rows = self.rows[start as usize..end as usize]
            .iter()
            .map(|(s, depth)| {
                let n = scene.at(*s);
                let (mut parent_selected, mut ancestor_hidden, mut ancestor_locked) =
                    (false, false, false);
                let mut cur = n.parent;
                while let Some(p) = cur {
                    let pn = scene.at(p);
                    parent_selected |= selected.contains(&pn.id);
                    ancestor_hidden |= !pn.props.visible;
                    ancestor_locked |= pn.props.locked;
                    cur = pn.parent;
                }
                let has_children = n.children.iter().any(|c| !scene.is_draft(scene.at(*c).id));
                LayerRow {
                    id: n.id.to_string(),
                    name: clip(&n.props.name, MAX_ROW_NAME_CHARS),
                    kind: n.props.kind.into(),
                    depth: *depth,
                    has_children,
                    expanded: has_children && (searching || self.expanded.contains(&n.id)),
                    selected: selected.contains(&n.id),
                    parent_selected,
                    visible: n.props.visible,
                    locked: n.props.locked,
                    ancestor_hidden,
                    ancestor_locked,
                }
            })
            .collect();
        let w = LayerWindow { start, total, rows };
        if self.last.as_ref() == Some(&w) {
            return None;
        }
        self.last = Some(w.clone());
        Some(w)
    }

    /// Следующее окно уйдёт, даже если не изменилось (панель запросила заново).
    pub fn resend(&mut self) {
        self.last = None;
    }
}
````

В `crates/ferrum-wasm/src/lib.rs` заменить:

````rust
pub mod input;
pub mod layout;
````

на:

````rust
pub mod input;
pub mod layers;
pub mod layout;
````

Run: `cargo test -p ferrum-wasm --test layers`
Expected: PASS, `6 passed`.

Run: `cargo clippy -p ferrum-wasm --all-targets -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 4: Закоммитить**

```bash
git add Cargo.lock crates/ferrum-wasm
git commit -m "feat(wasm): дерево слоёв в worker'е — окно строк, раскрытие, поиск"
```

---

### Task 4: Сводка выделения для панели свойств

Worker сводит свойства всех выделенных узлов сам и отдаёт готовые значения: у одинаковых — значение, у разных — «Смешанные» (`mixed`; значение первого узла — от него считается перетаскивание подписи). Поле, которое не относится ни к одному выделенному узлу, отсутствует: радиус — только у фреймов, прямоугольников и изображений (у прямоугольника с эллипсом радиус прямоугольника — не «Смешанные»), высота — не у линий, обрезка — только у фреймов, заливки — не у групп и линий, обводки — не у групп. Положение — как в панели (`panel_pose`, Task 2): у группы — рамка детей, поворот — в `(-180, 180]`.

Заливки и обводки — списком, если у всех выделенных он одинаковый. Список длиннее `MAX_PAINTS` (64) в панель не уходит и считается «Смешанными»: панель меняет список целиком, и усечённый список, отправленный обратно, потерял бы остальное; заменить его можно явно, кнопкой «+». Сводка считается при смене выделения или документа, а не в каждом кадре: O(выделенных).

**Files:**
- Create: `crates/ferrum-wasm/src/summary.rs`
- Modify: `crates/ferrum-wasm/src/lib.rs`
- Test: `crates/ferrum-wasm/tests/summary.rs`

**Interfaces:**
- Consumes: `panel_pose`, `field_applies`, `NumField` (Task 2), `NodeType` (Task 3).
- Produces: `summary::{summarize(scene, ids) -> Summary, Summary::empty, Num, Flag, Paints<T>, FillDto, StrokeDto, PaintKind, ScaleModeDto, Align, fill_dto, stroke_dto, MAX_PAINTS}` — все с `Tsify`.

- [ ] **Step 1: Написать падающие тесты**

`crates/ferrum-wasm/tests/summary.rs`:

````rust
//! Сводка выделения для панели свойств: одинаковые значения, «Смешанные»,
//! поля только там, где они есть у узлов, рамка группы, списки длиннее предела.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_doc::{Color, Document, Fill, NodeId, NodeKind, PropsPatch, Stroke, StrokeAlign};
use ferrum_scene::Scene;
use ferrum_wasm::layers::NodeType;
use ferrum_wasm::summary::{summarize, Num, MAX_PAINTS};

struct Page {
    doc: Document,
}

impl Page {
    fn new() -> Page {
        Page {
            doc: Document::new(),
        }
    }

    fn add(&mut self, parent: Option<NodeId>, kind: NodeKind, patch: PropsPatch) -> NodeId {
        let p = self.doc.pages()[0].0;
        self.doc
            .transact(p, vec![], |tx| {
                let id = tx.create_node(parent, None, kind)?;
                tx.set_props(id, &patch)?;
                Ok(vec![id])
            })
            .unwrap()[0]
    }

    fn scene(&mut self) -> Scene {
        let p = self.doc.pages()[0].0;
        Scene::from_page(self.doc.page(p).unwrap())
    }
}

fn at(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

fn same(v: f64) -> Num {
    Num {
        value: v,
        mixed: false,
    }
}

#[test]
fn empty_selection_and_unknown_ids() {
    let mut p = Page::new();
    let s = summarize(&p.scene(), &[]);
    assert_eq!(s.count, 0);
    let ghost: NodeId = "7@7".parse().unwrap();
    assert_eq!(
        summarize(&p.scene(), &[ghost]).count,
        0,
        "неизвестные пропущены"
    );
}

#[test]
fn single_rect_gives_exact_values() {
    let mut p = Page::new();
    let r = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            name: Some("Кнопка".into()),
            corner_radius: Some(4.0),
            rotation: Some(270.0),
            ..at(10.0, 20.0, 30.0, 40.0)
        },
    );
    let s = summarize(&p.scene(), &[r]);
    assert_eq!(
        (s.count, s.kind, s.name.as_deref()),
        (1, Some(NodeType::Rect), Some("Кнопка"))
    );
    assert_eq!([s.x, s.y, s.w], [same(10.0), same(20.0), same(30.0)]);
    assert_eq!(s.h, Some(same(40.0)));
    assert_eq!(s.rotation, same(-90.0), "поворот как в Figma");
    assert_eq!(s.corner_radius, Some(same(4.0)));
    assert_eq!(s.clip_content, None, "обрезка — только у фреймов");
    let fills = s.fills.unwrap();
    assert!(!fills.mixed);
    assert_eq!(fills.list[0].color, "#d9d9d9ff");
}

#[test]
fn differing_values_are_mixed() {
    let mut p = Page::new();
    let a = p.add(None, NodeKind::Rect, at(10.0, 5.0, 30.0, 40.0));
    let b = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(vec![Fill::solid(Color::rgba(255, 0, 0, 255))]),
            ..at(50.0, 5.0, 30.0, 40.0)
        },
    );
    let s = summarize(&p.scene(), &[a, b]);
    assert_eq!(
        s.x,
        Num {
            value: 10.0,
            mixed: true
        },
        "значение первого"
    );
    assert_eq!(s.y, same(5.0));
    let fills = s.fills.unwrap();
    assert!(fills.mixed && fills.list.is_empty());
    assert_eq!(s.name, None);
    assert_eq!(s.kind, Some(NodeType::Rect));
}

#[test]
fn fields_only_where_they_apply() {
    let mut p = Page::new();
    let r = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            corner_radius: Some(8.0),
            ..at(0.0, 0.0, 10.0, 10.0)
        },
    );
    let e = p.add(None, NodeKind::Ellipse, at(20.0, 0.0, 10.0, 10.0));
    let s = summarize(&p.scene(), &[r, e]);
    assert_eq!(s.kind, None, "разные типы");
    assert_eq!(
        s.corner_radius,
        Some(same(8.0)),
        "у эллипса радиуса нет — не «Смешанные»"
    );
    let l = p.add(None, NodeKind::Line, at(0.0, 50.0, 100.0, 0.0));
    let s = summarize(&p.scene(), &[l]);
    assert_eq!(
        (s.h, s.fills.is_none()),
        (None, true),
        "у линии нет высоты и заливок"
    );
    assert_eq!(s.strokes.unwrap().list[0].width, 1.0);
    let f = p.add(None, NodeKind::Frame, at(0.0, 100.0, 50.0, 50.0));
    let s = summarize(&p.scene(), &[f, r]);
    assert_eq!(
        s.clip_content.map(|c| (c.value, c.mixed)),
        Some((true, false))
    );
}

#[test]
fn group_shows_bounds_of_children() {
    let mut p = Page::new();
    let g = p.add(
        None,
        NodeKind::Group,
        PropsPatch {
            x: Some(100.0),
            y: Some(50.0),
            ..Default::default()
        },
    );
    p.add(Some(g), NodeKind::Rect, at(10.0, 20.0, 30.0, 30.0));
    p.add(Some(g), NodeKind::Rect, at(60.0, 20.0, 20.0, 60.0));
    let s = summarize(&p.scene(), &[g]);
    assert_eq!([s.x, s.y, s.w], [same(110.0), same(70.0), same(70.0)]);
    assert_eq!(s.h, Some(same(60.0)));
    assert!(
        s.fills.is_none() && s.strokes.is_none(),
        "у группы своих заливок нет"
    );
}

#[test]
fn paint_lists_are_capped() {
    let mut p = Page::new();
    let fills: Vec<Fill> = (0..100)
        .map(|i| Fill::solid(Color::rgba(i, 0, 0, 255)))
        .collect();
    let strokes: Vec<Stroke> = (0..=MAX_PAINTS)
        .map(|_| Stroke {
            color: Color::BLACK,
            width: 1.0,
            align: StrokeAlign::Center,
        })
        .collect();
    let r = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(fills),
            strokes: Some(strokes),
            ..at(0.0, 0.0, 10.0, 10.0)
        },
    );
    // Длиннее предела — как «Смешанные»: усечённый список панель не получает и
    // не может отправить обратно вместо полного.
    let s = summarize(&p.scene(), &[r]);
    let (f, st) = (s.fills.unwrap(), s.strokes.unwrap());
    assert!(f.mixed && f.list.is_empty() && st.mixed && st.list.is_empty());
}
````

Run: `cargo test -p ferrum-wasm --test summary`
Expected: FAIL — не компилируется: нет модуля `ferrum_wasm::summary`.

- [ ] **Step 2: Сводка выделения**

`crates/ferrum-wasm/src/summary.rs`:

````rust
//! Сводка выделения для панели свойств (этап 5). Worker сводит свойства всех
//! выделенных узлов сам и отдаёт готовые значения: у одинаковых — значение, у
//! разных — «Смешанные» (`mixed`, значение первого узла — для перетаскивания
//! подписи). Поле, которое не относится ни к одному выделенному узлу (радиус у
//! эллипсов, обрезка не у фреймов), отсутствует (`None`). Считается при смене
//! выделения или документа, а не в каждом кадре: O(выделенных).

use ferrum_doc::{Fill, FillKind, NodeId, NodeKind, ScaleMode, Stroke, StrokeAlign};
use ferrum_editor::{field_applies, panel_pose, NumField};
use ferrum_scene::Scene;
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::layers::{NodeType, MAX_ROW_NAME_CHARS};

/// Сколько заливок и обводок уходит в сводке (длиннее — «Смешанные») и
/// принимается в команде.
pub const MAX_PAINTS: usize = 64;

/// Числовое поле: значение (у «Смешанных» — первого узла) и признак различия.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Num {
    pub value: f64,
    pub mixed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Flag {
    pub value: bool,
    pub mixed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum PaintKind {
    Solid,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum ScaleModeDto {
    Fill,
    Fit,
    Tile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Inside,
    Center,
    Outside,
}

/// Заливка для панели и команды `setFills`. Цвет — `#rrggbbaa`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FillDto {
    pub kind: PaintKind,
    pub color: String,
    pub opacity: f64,
    pub visible: bool,
    /// Только у изображения: хэш байтов и режим. Панель показывает их, но не меняет.
    #[serde(default)]
    pub image_hash: Option<String>,
    #[serde(default)]
    pub scale_mode: Option<ScaleModeDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrokeDto {
    pub color: String,
    pub width: f64,
    pub align: Align,
}

/// Заливки или обводки выделения: список, если у всех одинаковый.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Paints<T> {
    pub mixed: bool,
    pub list: Vec<T>,
}

/// Сводка выделения.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub count: u32,
    /// Тип, если у всех выделенных он один.
    pub kind: Option<NodeType>,
    /// Имя единственного выделенного (для экспорта и заголовка).
    pub name: Option<String>,
    pub x: Num,
    pub y: Num,
    pub w: Num,
    pub h: Option<Num>,
    pub rotation: Num,
    pub opacity: Num,
    pub corner_radius: Option<Num>,
    pub clip_content: Option<Flag>,
    pub fills: Option<Paints<FillDto>>,
    pub strokes: Option<Paints<StrokeDto>>,
}

impl Summary {
    pub fn empty() -> Summary {
        let zero = Num {
            value: 0.0,
            mixed: false,
        };
        Summary {
            count: 0,
            kind: None,
            name: None,
            x: zero,
            y: zero,
            w: zero,
            h: None,
            rotation: zero,
            opacity: Num {
                value: 1.0,
                mixed: false,
            },
            corner_radius: None,
            clip_content: None,
            fills: None,
            strokes: None,
        }
    }
}

pub fn fill_dto(f: &Fill) -> FillDto {
    FillDto {
        kind: match f.kind {
            FillKind::Solid => PaintKind::Solid,
            FillKind::Image => PaintKind::Image,
        },
        color: f.color.to_hex(),
        opacity: f.opacity,
        visible: f.visible,
        image_hash: f.image_hash.clone(),
        scale_mode: (f.kind == FillKind::Image).then_some(match f.scale_mode {
            ScaleMode::Fill => ScaleModeDto::Fill,
            ScaleMode::Fit => ScaleModeDto::Fit,
            ScaleMode::Tile => ScaleModeDto::Tile,
        }),
    }
}

pub fn stroke_dto(s: &Stroke) -> StrokeDto {
    StrokeDto {
        color: s.color.to_hex(),
        width: s.width,
        align: match s.align {
            StrokeAlign::Inside => Align::Inside,
            StrokeAlign::Center => Align::Center,
            StrokeAlign::Outside => Align::Outside,
        },
    }
}

/// Накопитель поля: первое значение и было ли отличие.
struct Acc<T> {
    first: Option<T>,
    mixed: bool,
}

impl<T> Default for Acc<T> {
    fn default() -> Self {
        Acc {
            first: None,
            mixed: false,
        }
    }
}

impl<T: PartialEq> Acc<T> {
    fn add(&mut self, v: T) {
        match &self.first {
            None => self.first = Some(v),
            Some(f) => {
                if !self.mixed && *f != v {
                    self.mixed = true;
                }
            }
        }
    }
}

impl Acc<f64> {
    fn num(self) -> Option<Num> {
        self.first.map(|value| Num {
            value,
            mixed: self.mixed,
        })
    }
}

/// Сводка по выделенным узлам (неизвестные пропускаются).
pub fn summarize(scene: &Scene, ids: &[NodeId]) -> Summary {
    let mut count = 0u32;
    let mut kind: Acc<NodeKind> = Acc::default();
    let (mut x, mut y, mut w, mut h, mut rot) = (
        Acc::default(),
        Acc::default(),
        Acc::default(),
        Acc::default(),
        Acc::default(),
    );
    let mut opacity: Acc<f64> = Acc::default();
    let mut radius: Acc<f64> = Acc::default();
    let mut clip: Acc<bool> = Acc::default();
    let mut fills: Acc<&[Fill]> = Acc::default();
    let mut strokes: Acc<&[Stroke]> = Acc::default();
    let mut name = None;
    for id in ids {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        let n = scene.at(slot);
        let p = &n.props;
        count += 1;
        kind.add(p.kind);
        let pose = panel_pose(scene, slot);
        x.add(pose.x);
        y.add(pose.y);
        w.add(pose.w);
        if field_applies(NumField::H, p.kind) {
            h.add(pose.h);
        }
        rot.add(pose.rotation);
        opacity.add(p.opacity);
        if field_applies(NumField::CornerRadius, p.kind) {
            radius.add(p.corner_radius);
        }
        if p.kind == NodeKind::Frame {
            clip.add(p.clip_content);
        }
        if p.kind != NodeKind::Group && p.kind != NodeKind::Line {
            fills.add(&p.fills);
        }
        if p.kind != NodeKind::Group {
            strokes.add(&p.strokes);
        }
        if count == 1 {
            name = Some(p.name.chars().take(MAX_ROW_NAME_CHARS).collect::<String>());
        }
    }
    if count == 0 {
        return Summary::empty();
    }
    let zero = Num {
        value: 0.0,
        mixed: false,
    };
    Summary {
        count,
        kind: (!kind.mixed)
            .then(|| kind.first.map(NodeType::from))
            .flatten(),
        name: (count == 1).then_some(name).flatten(),
        x: x.num().unwrap_or(zero),
        y: y.num().unwrap_or(zero),
        w: w.num().unwrap_or(zero),
        h: h.num(),
        rotation: rot.num().unwrap_or(zero),
        opacity: opacity.num().unwrap_or(zero),
        corner_radius: radius.num(),
        clip_content: clip.first.map(|value| Flag {
            value,
            mixed: clip.mixed,
        }),
        // Список длиннее `MAX_PAINTS` не уходит в панель целиком; правка списка
        // заменила бы его усечённым и потеряла бы остальное. Поэтому он — как
        // «Смешанные»: панель предлагает заменить его явно («+»).
        fills: fills.first.map(|list| {
            let whole = !fills.mixed && list.len() <= MAX_PAINTS;
            Paints {
                mixed: !whole,
                list: if whole {
                    list.iter().map(fill_dto).collect()
                } else {
                    vec![]
                },
            }
        }),
        strokes: strokes.first.map(|list| {
            let whole = !strokes.mixed && list.len() <= MAX_PAINTS;
            Paints {
                mixed: !whole,
                list: if whole {
                    list.iter().map(stroke_dto).collect()
                } else {
                    vec![]
                },
            }
        }),
    }
}
````

В `crates/ferrum-wasm/src/lib.rs` заменить:

````rust
pub mod protocol;
#[cfg(target_arch = "wasm32")]
````

на:

````rust
pub mod protocol;
pub mod summary;
#[cfg(target_arch = "wasm32")]
````

Run: `cargo test -p ferrum-wasm --test summary`
Expected: PASS, `6 passed`.

Run: `cargo clippy -p ferrum-wasm --all-targets -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 3: Закоммитить**

```bash
git add crates/ferrum-wasm
git commit -m "feat(wasm): сводка выделения со «Смешанными» для панели свойств"
```

---

### Task 5: Протокол панелей: команды, события, пределы

Ядро worker'а получает команды панелей и шлёт им события — тем же путём, что в этапе 4 (serde + tsify, `deny_unknown_fields`; неверная форма — «Неверная команда: …»). Новые команды:

| Команда | Что делает |
|---|---|
| `layersWindow { start, count }`, `expandLayer`, `filterLayers`, `hoverLayer` | окно строк, раскрытие, поиск (≤ 4096 байт), подсветка узла на холсте |
| `select { ids, mode: replace \| toggle \| range }` | выделение из панели (≤ 1000 идентификаторов); такое выделение не раскрывает предков и не прокручивает панель |
| `renameLayer`, `moveLayers { ids?, target, position }`, `setLayerFlags` | имя, порядок и вложенность (без `ids` — всё выделение: его полный список есть только у worker'а), скрыть и заблокировать |
| `editSelection { edit, phase }` | правка панели свойств: `number`, `fills`, `strokes`, `clipContent`, `size`; фазы `preview`, `commit`, `cancel` |
| `addPage`, `renamePage`, `deletePage`, `setPage`, `setPageBackground { color?, phase }`, `setDocumentName` | страницы, фон (жест палитры — превью только в кадре, в документ — при отпускании), имя файла |
| `addFramePreset { index }`, `setZoom { zoom }`, `export { scale: 1 \| 2 \| 3 }`, `pasteImage { bytes }` | пресет, зум, PNG выделения или всей страницы, картинка из буфера в центр видимой части |
| `bind { command, shortcut, replace }`, `unbind`, `resetKeymap { command? }`, `importKeymap { json }`, `setSettings { pixelSnap }` | раскладка и настройки |

События: `layers { start, total, rows, reveal? }`, `properties { summary }`, `pages { pages, current, background? }`, `keymap { commands, conflicts, reserved, overrides }` (таблица команд с группами, действующими сочетаниями, подписями US и умолчаниями; отличия в JSON интерфейс хранит в настройках), `bindConflict` (сочетание занято — интерфейс предлагает заменить), `keymapImported { warnings }`, `presets`, `exported { name, bytes }`, `documentName`; у `opened` появилось имя документа. Раскладка и пресеты уходят сразу при создании ядра — вместе с `ready`.

Окно слоёв, сводка и страницы пересчитываются после пачки событий редактора и только по поводу (правка документа, выделение, страница, команда панели) и уходят, только если изменились. Выделение с холста раскрывает предков в дереве (`reveal`). Живое состояние выделения одного узла — те же значения, что в сводке (`panel_pose`). Раскладка клавиш и настройки при новом или открытом документе сохраняются — они пользователя, а не документа.

Пределы и проверки: идентификаторы узлов и страниц (≤ 64 байт, страница должна существовать), строки (≤ 4096 байт), списки (≤ 1000), заливки и обводки (≤ 64; цвета `#rrggbb[aa]`, прозрачность и толщина конечные и в пределах, хэш картинки — 64 hex-знака), числа конечные, масштаб экспорта — 1, 2 или 3, пресет — из списка, сочетание (≤ 64 байт). Ошибка формата импорта раскладки — русским текстом, без английского сообщения разбора JSON.

Фон холста без своего фона страницы — токен `canvas` дизайн-системы: #1a1918 (тёмная тема) и #e9e7e4 (светлая); E2E этапа 4 сверяют новый цвет.

**Files:**
- Modify: `crates/ferrum-wasm/src/protocol.rs`
- Modify: `crates/ferrum-wasm/src/core.rs`
- Create: `crates/ferrum-wasm/src/core/panels.rs`
- Modify: `crates/ferrum-wasm/src/live.rs`
- Test: `crates/ferrum-wasm/tests/panels.rs`
- Modify: `crates/ferrum-wasm/tests/protocol.rs`
- Modify: `crates/ferrum-wasm/tests/core.rs`
- Modify: `crates/ferrum-wasm/tests/web.rs`
- Modify: `apps/web/e2e/core.spec.ts`
- Modify: `apps/web/e2e/fallback.spec.ts`

**Interfaces:**
- Consumes: `Layers` (Task 3), `summarize` (Task 4), `Editor::{edit_selection, set_flags, rename_node, move_layers, create_page, rename_page, delete_page, set_page_background, set_document_name, add_frame_preset, insert_image, keymap_mut}` (Tasks 1–2, этап 3).
- Produces: `protocol::{SelectMode, DropPosition, Field, EditPhase, Edit, PageInfo, KeymapEntry, ConflictInfo, PresetInfo, MAX_TEXT_BYTES, EXPORT_SCALES}` и реэкспорт типов `layers`/`summary`; новые варианты `Command` и `CoreEvent` (таблица выше); `CoreEvent::Opened::name`; `Core::background()` учитывает фон страницы и его превью; `CANVAS_DARK`/`CANVAS_LIGHT` — токен `canvas`. TypeScript-типы — из `ferrum_wasm.d.ts` (`Option` — `T | undefined`).

- [ ] **Step 1: Написать падающие тесты**

Сценарии ядра (`panels.rs`): события при запуске и открытии, выделение из панели и с холста, жест поля — превью и один шаг, проверка заливок, слои, страницы, экспорт, раскладка, неверные команды, настройки и раскладка после нового документа. Протокол: новые команды и события переживают сериализацию, имена полей — как ждёт TypeScript, неверные формы отвергаются. Wasm-тест — разбор вложенной правки из JS-объекта.

`crates/ferrum-wasm/tests/panels.rs`:

````rust
//! Команды и события панелей (этап 5) в ядре worker'а: окно слоёв, выделение из
//! панели и с холста, правки свойств одним шагом, страницы, экспорт, раскладка,
//! настройки. Неверные данные — событие `Error`, а не паника и не правка.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_render::CpuBackend;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

const VIEW: Size = Size::new(800.0, 600.0);

fn demo(nodes: u32) -> Core {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    core.command(Command::Demo { nodes });
    core
}

fn layers(ev: &[CoreEvent]) -> Option<(u32, u32, Vec<LayerRow>, Option<u32>)> {
    ev.iter().rev().find_map(|e| match e {
        CoreEvent::Layers {
            start,
            total,
            rows,
            reveal,
        } => Some((*start, *total, rows.clone(), *reveal)),
        _ => None,
    })
}

fn summary(ev: &[CoreEvent]) -> Option<Summary> {
    ev.iter().rev().find_map(|e| match e {
        CoreEvent::Properties { summary } => Some(summary.clone()),
        _ => None,
    })
}

fn errors(ev: &[CoreEvent]) -> Vec<String> {
    ev.iter()
        .filter_map(|e| match e {
            CoreEvent::Error { message } => Some(message.clone()),
            _ => None,
        })
        .collect()
}

fn click(core: &mut Core, x: f64, y: f64, t: f64) {
    let e = PointerEvent {
        point: (x, y).into(),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t,
    };
    core.handle(Input::PointerDown(e));
    core.handle(Input::PointerUp(e));
}

fn draw(core: &mut Core) {
    let (w, h) = core.device_size();
    core.draw(&mut CpuBackend::new(w, h));
}

fn document_steps(core: &Core) -> f64 {
    core.live().steps
}

#[test]
fn keymap_and_presets_arrive_before_any_document() {
    let mut core = Core::new(Platform::Mac, VIEW, 1.0, Backend::Cpu);
    let ev = core.take_events();
    let Some(CoreEvent::Keymap {
        commands,
        overrides,
        reserved,
        ..
    }) = ev.iter().find(|e| matches!(e, CoreEvent::Keymap { .. }))
    else {
        panic!("нет раскладки: {ev:?}");
    };
    let rect = commands.iter().find(|c| c.key == "tool.rect").unwrap();
    assert_eq!(
        (rect.name.as_str(), rect.group.as_str()),
        ("Прямоугольник", "tool")
    );
    assert_eq!(
        (rect.bindings.clone(), rect.labels.clone()),
        (vec!["KeyR".to_string()], vec!["R".to_string()])
    );
    let undo = commands.iter().find(|c| c.key == "edit.undo").unwrap();
    assert_eq!(undo.labels, ["⌘Z"], "подпись macOS");
    assert!(overrides.contains("ferrum-keymap"));
    assert!(reserved.contains(&"Meta+KeyW".to_string()));
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Presets { presets } if presets.len() >= 11)));
}

#[test]
fn opening_sends_layers_properties_and_pages() {
    let mut core = demo(1000);
    let ev = core.take_events();
    let (start, total, rows, _) = layers(&ev).expect("строки слоёв");
    // Окно по умолчанию — пустое, пока панель не попросит.
    assert_eq!((start, total, rows.len()), (0, 10, 0));
    assert_eq!(summary(&ev).unwrap().count, 0);
    assert!(ev.iter().any(
        |e| matches!(e, CoreEvent::Pages { pages, background: None, .. } if pages.len() == 1)
    ));
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0].name, "Экран 9");
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    assert!(
        layers(&core.take_events()).is_none(),
        "то же окно не повторяется"
    );
}

#[test]
fn select_from_panel_does_not_reveal_but_canvas_selection_does() {
    let mut core = demo(200);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    let frame = rows[1].id.clone();
    core.command(Command::Select {
        ids: vec![frame.clone()],
        mode: SelectMode::Replace,
    });
    let ev = core.take_events();
    let (_, _, rows, reveal) = layers(&ev).unwrap();
    assert!(rows[1].selected && reveal.is_none());
    let s = summary(&ev).unwrap();
    assert_eq!(
        (s.count, s.kind, s.w.value),
        (1, Some(NodeType::Frame), 1440.0)
    );
    // Щелчок на холсте по «Слой 1» в «Экран 0» (мир (200, 40), экран (140, 64)):
    // фрейм раскрывается, панель получает строку для прокрутки — сверху «Экран 1»,
    // «Экран 0», затем дети сверху вниз от «Слой 98».
    click(&mut core, 140.0, 64.0, 1000.0);
    let (_, total, _, reveal) = layers(&core.take_events()).unwrap();
    assert_eq!((total, reveal), (2 + 99, Some(2 + 97)));
}

#[test]
fn edit_selection_is_one_step_and_updates_summary() {
    let mut core = demo(200);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    core.command(Command::Select {
        ids: vec![rows[0].id.clone()],
        mode: SelectMode::Replace,
    });
    core.take_events();
    let steps = document_steps(&core);
    for v in [10.0, 20.0, 30.0] {
        core.command(Command::EditSelection {
            edit: Edit::Number {
                field: Field::X,
                value: v,
                relative: true,
            },
            phase: EditPhase::Preview,
        });
    }
    assert_eq!(document_steps(&core), steps, "превью — без шагов");
    core.command(Command::EditSelection {
        edit: Edit::Number {
            field: Field::X,
            value: 30.0,
            relative: true,
        },
        phase: EditPhase::Commit,
    });
    assert_eq!(document_steps(&core), steps + 1.0);
    let s = summary(&core.take_events()).expect("сводка обновилась");
    assert_eq!(s.x.value, 1600.0 + 30.0);
    draw(&mut core);
    assert_eq!(
        core.live().selection_x,
        1630.0,
        "живое состояние — те же значения"
    );
    core.command(Command::Execute {
        command: "edit.undo".into(),
    });
    assert_eq!(summary(&core.take_events()).unwrap().x.value, 1600.0);
}

#[test]
fn fills_from_panel_are_validated() {
    let mut core = demo(100);
    core.command(Command::Execute {
        command: "edit.select_all".into(),
    });
    core.take_events();
    let steps = document_steps(&core);
    let red = FillDto {
        kind: PaintKind::Solid,
        color: "#ff0000".into(),
        opacity: 0.5,
        visible: true,
        image_hash: None,
        scale_mode: None,
    };
    core.command(Command::EditSelection {
        edit: Edit::Fills {
            fills: vec![red.clone()],
        },
        phase: EditPhase::Commit,
    });
    let s = summary(&core.take_events()).unwrap();
    assert_eq!(s.fills.unwrap().list[0].color, "#ff0000ff");
    for bad in [
        FillDto {
            color: "красный".into(),
            ..red.clone()
        },
        FillDto {
            opacity: f64::NAN,
            ..red.clone()
        },
        FillDto {
            kind: PaintKind::Image,
            image_hash: Some("zz".into()),
            ..red.clone()
        },
    ] {
        core.command(Command::EditSelection {
            edit: Edit::Fills { fills: vec![bad] },
            phase: EditPhase::Commit,
        });
        assert_eq!(errors(&core.take_events()).len(), 1);
    }
    core.command(Command::EditSelection {
        edit: Edit::Fills {
            fills: vec![red; 65],
        },
        phase: EditPhase::Commit,
    });
    assert_eq!(errors(&core.take_events()).len(), 1, "предел заливок");
    assert_eq!(document_steps(&core), steps + 1.0, "неверное не правит");
}

#[test]
fn rename_move_and_flags_from_layers() {
    let mut core = demo(200);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    let (top, bottom) = (rows[0].id.clone(), rows[1].id.clone());
    core.command(Command::RenameLayer {
        id: top.clone(),
        name: "Главная".into(),
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    assert_eq!(rows[0].name, "Главная");
    core.command(Command::RenameLayer {
        id: top.clone(),
        name: "   ".into(),
    });
    let (_, _, rows, _) =
        layers(&core.take_events()).expect("строки вернулись после пустого имени");
    assert_eq!(rows[0].name, "Главная");
    core.command(Command::MoveLayers {
        ids: Some(vec![top.clone()]),
        target: bottom.clone(),
        position: DropPosition::Below,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    assert_eq!(
        (rows[0].id.as_str(), rows[1].id.as_str()),
        (bottom.as_str(), top.as_str())
    );
    core.command(Command::SetLayerFlags {
        ids: vec![top.clone()],
        visible: Some(false),
        locked: Some(true),
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    assert!(!rows[1].visible && rows[1].locked);
    core.command(Command::MoveLayers {
        ids: None,
        target: top.clone(),
        position: DropPosition::Into,
    });
    assert!(
        errors(&core.take_events()).is_empty(),
        "пустое выделение — без ошибки и без правки"
    );
}

#[test]
fn pages_add_switch_rename_background_delete() {
    let mut core = demo(100);
    core.take_events();
    core.command(Command::AddPage);
    let ev = core.take_events();
    let Some(CoreEvent::Pages { pages, current, .. }) = ev
        .iter()
        .rev()
        .find(|e| matches!(e, CoreEvent::Pages { .. }))
    else {
        panic!("{ev:?}")
    };
    assert_eq!(pages.len(), 2);
    assert_eq!(*current, pages[1].id, "новая — текущая");
    assert_eq!(layers(&ev).unwrap().1, 0, "на новой странице слоёв нет");
    let first = pages[0].id.clone();
    let second = pages[1].id.clone();
    // Превью палитры — только в кадре: документ и список страниц не меняются.
    core.command(Command::SetPageBackground {
        color: Some("#102030".into()),
        phase: EditPhase::Preview,
    });
    assert_eq!(core.background().to_rgba8().r, 0x10);
    assert!(!core
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Pages { .. })));
    core.command(Command::SetPageBackground {
        color: None,
        phase: EditPhase::Cancel,
    });
    assert_eq!(core.background(), ferrum_wasm::core::CANVAS_DARK);
    core.command(Command::SetPageBackground {
        color: Some("#336699".into()),
        phase: EditPhase::Commit,
    });
    let ev = core.take_events();
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Pages { background: Some(b), .. } if b == "#336699ff")));
    assert!(core.wants_frame());
    core.command(Command::RenamePage {
        id: second.clone(),
        name: "Черновик".into(),
    });
    core.command(Command::SetPage { id: first.clone() });
    let ev = core.take_events();
    assert_eq!(layers(&ev).unwrap().1, 1, "первая страница: один фрейм");
    core.command(Command::DeletePage { id: first });
    let ev = core.take_events();
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Pages { pages, current, .. }
        if pages.len() == 1 && pages[0].name == "Черновик" && *current == second)));
    core.command(Command::DeletePage { id: second });
    assert_eq!(
        errors(&core.take_events()),
        ["Нельзя удалить единственную страницу"]
    );
    core.command(Command::SetPage { id: "1@1".into() });
    assert_eq!(errors(&core.take_events()), ["Страница не найдена"]);
}

#[test]
fn export_selection_and_page() {
    let mut core = demo(100);
    core.command(Command::Export { scale: 2.0 });
    let ev = core.take_events();
    let Some(CoreEvent::Exported { name, bytes }) =
        ev.iter().find(|e| matches!(e, CoreEvent::Exported { .. }))
    else {
        panic!("{ev:?}")
    };
    assert_eq!(name, "Страница 1@2x.png");
    assert_eq!(&bytes[1..4], b"PNG");
    // 1440 × 1024 при 2× — 2880 × 2048 (IHDR: ширина и высота big-endian).
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 2880);
    core.command(Command::Export { scale: 4.0 });
    assert_eq!(
        errors(&core.take_events()),
        ["Масштаб экспорта — 1×, 2× или 3×"]
    );
    core.command(Command::AddPage);
    core.command(Command::Export { scale: 1.0 });
    assert_eq!(
        errors(&core.take_events()),
        ["Нечего экспортировать: страница пуста"]
    );
}

#[test]
fn bind_conflict_replace_reset_and_import() {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    core.take_events();
    core.command(Command::Bind {
        command: "tool.rect".into(),
        shortcut: "KeyE".into(),
        replace: false,
    });
    let ev = core.take_events();
    let Some(CoreEvent::Keymap {
        commands,
        overrides,
        ..
    }) = ev.iter().find(|e| matches!(e, CoreEvent::Keymap { .. }))
    else {
        panic!("{ev:?}")
    };
    let rect = commands.iter().find(|c| c.key == "tool.rect").unwrap();
    assert_eq!(rect.bindings, ["KeyR", "KeyE"]);
    assert!(rect.overridden && overrides.contains("KeyE"));
    let saved = overrides.clone();
    core.command(Command::Bind {
        command: "tool.ellipse".into(),
        shortcut: "KeyR".into(),
        replace: false,
    });
    let ev = core.take_events();
    assert!(ev.contains(&CoreEvent::BindConflict {
        command: "tool.ellipse".into(),
        shortcut: "KeyR".into(),
        commands: vec!["tool.rect".into()],
    }));
    core.command(Command::Bind {
        command: "tool.ellipse".into(),
        shortcut: "KeyR".into(),
        replace: true,
    });
    core.command(Command::ResetKeymap { command: None });
    core.command(Command::ImportKeymap { json: saved });
    let ev = core.take_events();
    assert!(ev.contains(&CoreEvent::KeymapImported { warnings: vec![] }));
    let Some(CoreEvent::Keymap { commands, .. }) = ev
        .iter()
        .rev()
        .find(|e| matches!(e, CoreEvent::Keymap { .. }))
    else {
        panic!()
    };
    assert_eq!(
        commands
            .iter()
            .find(|c| c.key == "tool.ellipse")
            .unwrap()
            .bindings,
        ["KeyO"]
    );
    core.command(Command::ImportKeymap { json: "{".into() });
    assert_eq!(
        errors(&core.take_events()),
        ["Не удалось импортировать раскладку: это не файл раскладки Ferrum (JSON)"],
        "без английского текста разбора"
    );
    core.command(Command::Bind {
        command: "tool.pen".into(),
        shortcut: "KeyP".into(),
        replace: true,
    });
    core.command(Command::Bind {
        command: "tool.rect".into(),
        shortcut: "Hyper+KeyP".into(),
        replace: true,
    });
    assert_eq!(errors(&core.take_events()).len(), 2);
}

#[test]
fn malformed_panel_commands_are_errors() {
    let mut core = demo(100);
    core.take_events();
    let steps = document_steps(&core);
    let many: Vec<String> = (0..1001).map(|i| format!("{i}@1")).collect();
    let cases = [
        Command::Select {
            ids: many,
            mode: SelectMode::Replace,
        },
        Command::Select {
            ids: vec!["узел".into()],
            mode: SelectMode::Toggle,
        },
        Command::RenameLayer {
            id: "1@1".into(),
            name: "я".repeat(5000),
        },
        Command::FilterLayers {
            query: "x".repeat(5000),
        },
        Command::ExpandLayer {
            id: "x".repeat(100),
            expanded: true,
        },
        Command::EditSelection {
            edit: Edit::Number {
                field: Field::W,
                value: f64::INFINITY,
                relative: false,
            },
            phase: EditPhase::Commit,
        },
        Command::SetPageBackground {
            color: Some("#12".into()),
            phase: EditPhase::Commit,
        },
        Command::AddFramePreset { index: 999 },
        Command::SetZoom { zoom: -1.0 },
        Command::PasteImage {
            bytes: ByteBuf::from(vec![1, 2, 3]),
        },
    ];
    let n = cases.len();
    for c in cases {
        core.command(c);
    }
    assert_eq!(errors(&core.take_events()).len(), n);
    assert_eq!(document_steps(&core), steps);
}

#[test]
fn settings_zoom_preset_and_paste_image() {
    let mut core = demo(100);
    core.command(Command::SetSettings { pixel_snap: false });
    assert!(!core.editor().settings().pixel_snap);
    core.command(Command::SetZoom { zoom: 2.0 });
    assert_eq!(core.live().zoom, 2.0);
    core.command(Command::AddFramePreset { index: 0 });
    let s = summary(&core.take_events()).unwrap();
    assert_eq!((s.count, s.w.value, s.h.unwrap().value), (1, 393.0, 852.0));
    // PNG 2 × 1 из ferrum-render.
    let png = {
        let mut p = ferrum_render::Pixmap::new(2, 1);
        p.data_as_u8_slice_mut().fill(255);
        p.into_png().unwrap()
    };
    core.command(Command::PasteImage {
        bytes: ByteBuf::from(png),
    });
    let s = summary(&core.take_events()).unwrap();
    assert_eq!(
        (s.kind, s.w.value, s.h.unwrap().value),
        (Some(NodeType::Image), 2.0, 1.0)
    );
}

#[test]
fn keymap_and_settings_survive_a_new_document() {
    let mut core = demo(100);
    core.command(Command::Bind {
        command: "tool.rect".into(),
        shortcut: "KeyQ".into(),
        replace: true,
    });
    core.command(Command::SetSettings { pixel_snap: false });
    core.command(Command::Demo { nodes: 0 });
    let km = core.editor().keymap();
    let rect = ferrum_editor::CommandId::ToolRect;
    assert_eq!(
        km.bindings(rect)
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        ["KeyR", "KeyQ"]
    );
    assert!(
        !core.editor().settings().pixel_snap,
        "настройки — пользователя, не документа"
    );
}
````

`crates/ferrum-wasm/tests/protocol.rs` (целиком):

````rust
//! Сообщения worker'а: каждая команда и каждое событие переживают сериализацию,
//! имена полей — как ждёт TypeScript, неверные команды дают ошибку, а не панику.
//! (Тот же serde, что использует serde-wasm-bindgen в браузере; разбор настоящих
//! JS-объектов проверяет wasm-тест `tests/web.rs`.)
#![cfg(not(target_arch = "wasm32"))]

use ferrum_wasm::protocol::*;
use serde_bytes::ByteBuf;
use serde_json::json;

fn bytes(b: &[u8]) -> ByteBuf {
    ByteBuf::from(b.to_vec())
}

fn commands() -> Vec<Command> {
    vec![
        Command::Open {
            bytes: bytes(b"FERRUM"),
        },
        Command::Demo { nodes: 100_000 },
        Command::Execute {
            command: "edit.undo".into(),
        },
        Command::SetTool {
            tool: Tool::Ellipse,
        },
        Command::SetTheme {
            theme: Theme::Light,
        },
        Command::Paste { bytes: bytes(&[1]) },
        Command::PlaceImage {
            bytes: bytes(&[0x89, b'P']),
        },
        Command::Save,
        Command::ReadPixels { reference: true },
        Command::LayersWindow {
            start: 10,
            count: 40,
        },
        Command::ExpandLayer {
            id: "1@2".into(),
            expanded: true,
        },
        Command::FilterLayers {
            query: "кнопка".into(),
        },
        Command::Select {
            ids: vec!["1@2".into()],
            mode: SelectMode::Range,
        },
        Command::HoverLayer { id: None },
        Command::RenameLayer {
            id: "1@2".into(),
            name: "Фон".into(),
        },
        Command::MoveLayers {
            ids: None,
            target: "3@4".into(),
            position: DropPosition::Into,
        },
        Command::SetLayerFlags {
            ids: vec!["1@2".into()],
            visible: Some(false),
            locked: None,
        },
        Command::EditSelection {
            edit: Edit::Number {
                field: Field::CornerRadius,
                value: 4.0,
                relative: false,
            },
            phase: EditPhase::Preview,
        },
        Command::EditSelection {
            edit: Edit::Fills {
                fills: vec![FillDto {
                    kind: PaintKind::Solid,
                    color: "#ff0000ff".into(),
                    opacity: 1.0,
                    visible: true,
                    image_hash: None,
                    scale_mode: None,
                }],
            },
            phase: EditPhase::Commit,
        },
        Command::EditSelection {
            edit: Edit::Strokes {
                strokes: vec![StrokeDto {
                    color: "#000000ff".into(),
                    width: 1.0,
                    align: Align::Inside,
                }],
            },
            phase: EditPhase::Cancel,
        },
        Command::EditSelection {
            edit: Edit::ClipContent { value: false },
            phase: EditPhase::Commit,
        },
        Command::EditSelection {
            edit: Edit::Size { w: 393.0, h: 852.0 },
            phase: EditPhase::Commit,
        },
        Command::AddPage,
        Command::RenamePage {
            id: "5@6".into(),
            name: "Главная".into(),
        },
        Command::DeletePage { id: "5@6".into() },
        Command::SetPage { id: "5@6".into() },
        Command::SetPageBackground {
            color: Some("#ffffff".into()),
            phase: EditPhase::Commit,
        },
        Command::SetDocumentName {
            name: "Лендинг".into(),
        },
        Command::AddFramePreset { index: 3 },
        Command::SetZoom { zoom: 2.0 },
        Command::Export { scale: 3.0 },
        Command::PasteImage {
            bytes: bytes(&[0x89]),
        },
        Command::Bind {
            command: "tool.rect".into(),
            shortcut: "KeyQ".into(),
            replace: false,
        },
        Command::Unbind {
            command: "tool.rect".into(),
            shortcut: "KeyR".into(),
        },
        Command::ResetKeymap { command: None },
        Command::ImportKeymap { json: "{}".into() },
        Command::SetSettings { pixel_snap: false },
    ]
}

fn events() -> Vec<CoreEvent> {
    vec![
        CoreEvent::Ready {
            backend: Backend::Webgl,
            fallbacks: vec!["webgpu: нет адаптера".into()],
        },
        CoreEvent::Opened {
            pages: 2,
            nodes: 10,
            warnings: vec![],
            name: "Лендинг".into(),
        },
        CoreEvent::Selection {
            count: 2,
            ids: vec!["1@2".into(), "3@4".into()],
        },
        CoreEvent::Tool { tool: Tool::Hand },
        CoreEvent::Cursor {
            cursor: CursorKind::Resize,
            angle: 45.0,
        },
        CoreEvent::Document {
            can_undo: true,
            can_redo: false,
            steps: 3.0,
        },
        CoreEvent::Page { id: "p1".into() },
        CoreEvent::Clipboard { bytes: bytes(b"x") },
        CoreEvent::ClipboardPng { bytes: bytes(b"p") },
        CoreEvent::PasteRequested,
        CoreEvent::RequestImage,
        CoreEvent::ToggleUi,
        CoreEvent::Saved { bytes: bytes(b"s") },
        CoreEvent::Pixels {
            backend: Backend::Cpu,
            width: 1,
            height: 1,
            rgba: bytes(&[1, 2, 3, 4]),
        },
        CoreEvent::Error {
            message: "ой".into(),
        },
        CoreEvent::Layers {
            start: 0,
            total: 1,
            rows: vec![LayerRow {
                id: "1@2".into(),
                name: "Фон".into(),
                kind: NodeType::Rect,
                depth: 1,
                has_children: false,
                expanded: false,
                selected: true,
                parent_selected: false,
                visible: true,
                locked: false,
                ancestor_hidden: false,
                ancestor_locked: true,
            }],
            reveal: Some(0),
        },
        CoreEvent::Properties {
            summary: Summary::empty(),
        },
        CoreEvent::Pages {
            pages: vec![PageInfo {
                id: "5@6".into(),
                name: "Страница 1".into(),
            }],
            current: "5@6".into(),
            background: None,
        },
        CoreEvent::Keymap {
            commands: vec![KeymapEntry {
                key: "tool.rect".into(),
                name: "Прямоугольник".into(),
                group: "tool".into(),
                bindings: vec!["KeyR".into()],
                labels: vec!["R".into()],
                defaults: vec!["KeyR".into()],
                overridden: false,
            }],
            conflicts: vec![ConflictInfo {
                shortcut: "KeyR".into(),
                label: "R".into(),
                commands: vec![],
            }],
            reserved: vec!["Ctrl+KeyW".into()],
            overrides: "{}".into(),
        },
        CoreEvent::BindConflict {
            command: "a".into(),
            shortcut: "KeyR".into(),
            commands: vec!["b".into()],
        },
        CoreEvent::KeymapImported {
            warnings: vec!["пропущено".into()],
        },
        CoreEvent::Presets {
            presets: vec![PresetInfo {
                group: "Телефоны".into(),
                name: "iPhone 16".into(),
                width: 393.0,
                height: 852.0,
            }],
        },
        CoreEvent::Exported {
            name: "Фон@2x.png".into(),
            bytes: bytes(b"png"),
        },
        CoreEvent::DocumentName {
            name: "Лендинг".into(),
        },
    ]
}

#[test]
fn every_message_round_trips() {
    for c in commands() {
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(serde_json::from_value::<Command>(v).unwrap(), c);
    }
    for e in events() {
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(serde_json::from_value::<CoreEvent>(v).unwrap(), e);
    }
}

#[test]
fn field_names_match_typescript() {
    assert_eq!(
        serde_json::to_value(Command::SetTool { tool: Tool::Rect }).unwrap(),
        json!({"type": "setTool", "tool": "rect"})
    );
    assert_eq!(
        serde_json::to_value(Command::ReadPixels { reference: false }).unwrap(),
        json!({"type": "readPixels", "reference": false})
    );
    assert_eq!(
        serde_json::to_value(&events()[5]).unwrap(),
        json!({"type": "document", "canUndo": true, "canRedo": false, "steps": 3.0})
    );
    assert_eq!(
        serde_json::to_value(&events()[8]).unwrap()["type"],
        json!("clipboardPng")
    );
    assert_eq!(
        serde_json::to_value(Command::Save).unwrap(),
        json!({"type": "save"})
    );
    assert_eq!(
        serde_json::to_value(Command::EditSelection {
            edit: Edit::Number {
                field: Field::CornerRadius,
                value: 4.0,
                relative: true
            },
            phase: EditPhase::Preview,
        })
        .unwrap(),
        json!({"type": "editSelection", "edit": {"type": "number", "field": "cornerRadius", "value": 4.0, "relative": true}, "phase": "preview"})
    );
    assert_eq!(
        serde_json::to_value(Command::MoveLayers {
            ids: None,
            target: "1@2".into(),
            position: DropPosition::Above
        })
        .unwrap(),
        json!({"type": "moveLayers", "ids": null, "target": "1@2", "position": "above"})
    );
    let row = &serde_json::to_value(
        events()
            .iter()
            .find(|e| matches!(e, CoreEvent::Layers { .. }))
            .unwrap(),
    )
    .unwrap()["rows"][0];
    assert_eq!(row["hasChildren"], json!(false));
    assert_eq!(row["ancestorLocked"], json!(true));
    let s = serde_json::to_value(Summary::empty()).unwrap();
    assert_eq!(s["cornerRadius"], json!(null));
    assert_eq!(s["opacity"], json!({"value": 1.0, "mixed": false}));
}

#[test]
fn bad_commands_are_errors_not_panics() {
    for bad in [
        json!({"type": "format"}),
        json!({"type": "demo"}),
        json!({"type": "demo", "nodes": -1}),
        json!({"type": "demo", "nodes": 1, "extra": 1}),
        json!({"type": "setTool", "tool": "pen"}),
        json!({"tool": "rect"}),
        json!("save"),
        json!(null),
    ] {
        assert!(
            serde_json::from_value::<Command>(bad.clone()).is_err(),
            "принято: {bad}"
        );
    }
}

/// Команды приходят из основного потока и не доверенные: неверные типы полей,
/// значения вне диапазона и лишние поля — ошибка разбора, а не паника.
#[test]
fn malformed_commands_never_panic() {
    let bad = [
        // Неверные типы полей.
        json!({"type": "demo", "nodes": "100"}),
        json!({"type": "demo", "nodes": 1.5}),
        json!({"type": "demo", "nodes": null}),
        json!({"type": "demo", "nodes": [1]}),
        json!({"type": "execute", "command": 5}),
        json!({"type": "execute", "command": null}),
        // (Строка вместо байтов serde_bytes::ByteBuf принимает как UTF-8: это не паника
        // и не ошибка безопасности, поэтому здесь её нет.)
        json!({"type": "open", "bytes": 5}),
        json!({"type": "open", "bytes": [256]}),
        json!({"type": "open", "bytes": [-1]}),
        json!({"type": "open", "bytes": [1.5]}),
        json!({"type": "paste", "bytes": {"a": 1}}),
        json!({"type": "readPixels", "reference": "yes"}),
        json!({"type": "readPixels", "reference": 1}),
        json!({"type": "setTheme", "theme": 1}),
        json!({"type": "setTheme", "theme": "Dark"}),
        // Вне диапазона.
        json!({"type": "demo", "nodes": 4_294_967_296u64}),
        json!({"type": "demo", "nodes": 1e300}),
        json!({"type": "demo", "nodes": -0.5}),
        // Неизвестный или неверно написанный тег.
        json!({"type": "Save"}),
        json!({"type": "set_tool", "tool": "rect"}),
        json!({"type": ""}),
        json!({"type": 7}),
        json!({"type": null}),
        json!({"type": ["save"]}),
        // Лишние поля.
        // (`save` без полей — единственное исключение: serde не применяет
        // deny_unknown_fields к unit-вариантам внутренне тегированного enum, лишнее
        // поле там молча отбрасывается. Безвредно: команда ничего не читает.)
        json!({"type": "setTool", "tool": "rect", "x": 1}),
        // Команды этапа 5: неверные значения перечислений, вложенные поля, типы.
        json!({"type": "select", "ids": ["1@2"], "mode": "all"}),
        json!({"type": "select", "ids": "1@2", "mode": "replace"}),
        json!({"type": "layersWindow", "start": -1, "count": 10}),
        json!({"type": "moveLayers", "ids": null, "target": "1@2", "position": "inside"}),
        json!({"type": "editSelection", "edit": {"type": "number", "field": "z", "value": 1, "relative": false}, "phase": "commit"}),
        json!({"type": "editSelection", "edit": {"type": "number", "field": "x", "value": "1", "relative": false}, "phase": "commit"}),
        json!({"type": "editSelection", "edit": {"type": "number", "field": "x", "value": 1, "relative": false, "y": 2}, "phase": "commit"}),
        json!({"type": "editSelection", "edit": {"type": "fills", "fills": [{"kind": "solid", "color": "#fff"}]}, "phase": "commit"}),
        json!({"type": "editSelection", "edit": {"type": "fills", "fills": [{"kind": "gradient", "color": "#ffffff", "opacity": 1, "visible": true}]}, "phase": "commit"}),
        json!({"type": "editSelection", "edit": {"type": "clipContent", "value": true}, "phase": "later"}),
        json!({"type": "editSelection", "edit": {"type": "size", "w": 1}, "phase": "commit"}),
        json!({"type": "setLayerFlags", "ids": ["1@2"], "visible": "no", "locked": null}),
        json!({"type": "bind", "command": "tool.rect", "shortcut": "KeyQ"}),
        json!({"type": "export", "scale": "2"}),
        json!({"type": "addFramePreset", "index": -3}),
        json!({"type": "setSettings", "pixelSnap": 1}),
        json!({"type": "setSettings", "pixel_snap": true}),
        // Не объекты.
        json!([]),
        json!([{"type": "save"}]),
        json!(true),
        json!(1),
        json!({}),
    ];
    for b in bad {
        let r = std::panic::catch_unwind(|| serde_json::from_value::<Command>(b.clone()));
        let r = r.unwrap_or_else(|_| panic!("паника на {b}"));
        assert!(r.is_err(), "принято: {b}");
    }
    // Сырые строки JSON, не доходящие даже до значения.
    for raw in ["", "{", "{\"type\":", "\u{0}", "{\"type\":\"save\"} x"] {
        assert!(serde_json::from_str::<Command>(raw).is_err());
    }
}

#[test]
fn backend_codes_match_layout() {
    for b in [Backend::Webgpu, Backend::Webgl, Backend::Cpu] {
        assert_eq!(ferrum_wasm::layout::BACKENDS[b.code() as usize], b.name());
        assert_eq!(Backend::parse(b.name()), Some(b));
    }
    assert_eq!(Backend::parse("vulkan"), None);
}
````

В `crates/ferrum-wasm/tests/core.rs` заменить:

````rust
            nodes: 1000,
            warnings: vec![]
        }),
````

на:

````rust
            nodes: 1000,
            warnings: vec![],
            name: "Без названия".into(),
        }),
````

В `crates/ferrum-wasm/tests/core.rs` заменить:

````rust
        nodes: 300,
        warnings: vec![]
    }));
````

на:

````rust
        nodes: 300,
        warnings: vec![],
        name: "Без названия".into(),
    }));
````

В `crates/ferrum-wasm/tests/web.rs` заменить:

````rust
    assert!(message.chars().count() < 120, "{}", message.chars().count());
}
````

на:

````rust
    assert!(message.chars().count() < 120, "{}", message.chars().count());
}

#[wasm_bindgen_test]
fn panel_commands_parse_and_none_becomes_undefined() {
    use ferrum_wasm::protocol::{Edit, EditPhase, Field};
    let edit = obj(&[
        ("type", "number".into()),
        ("field", "x".into()),
        ("value", 12.5.into()),
        ("relative", false.into()),
    ]);
    let cmd = obj(&[
        ("type", "editSelection".into()),
        ("edit", edit),
        ("phase", "commit".into()),
    ]);
    assert_eq!(
        parse_command(cmd),
        Ok(Command::EditSelection {
            edit: Edit::Number {
                field: Field::X,
                value: 12.5,
                relative: false
            },
            phase: EditPhase::Commit,
        })
    );
    // `ids` пропущено или `null` — текущее выделение.
    let mv = obj(&[
        ("type", "moveLayers".into()),
        ("target", "1@2".into()),
        ("position", "into".into()),
    ]);
    assert!(matches!(
        parse_command(mv),
        Ok(Command::MoveLayers { ids: None, .. })
    ));
    // `None` в событии — `undefined`, как в типах tsify (`T | undefined`).
    let js = events_to_js(&[CoreEvent::Pages {
        pages: vec![],
        current: "1@2".into(),
        background: None,
    }]);
    let arr: js_sys::Array = js.dyn_into().unwrap();
    assert!(Reflect::get(&arr.get(0), &"background".into())
        .unwrap()
        .is_undefined());
}
````

Run: `cargo test -p ferrum-wasm --test panels`
Expected: FAIL — не компилируется: нет `Command::LayersWindow`, `CoreEvent::Layers`, `SelectMode`, `EditPhase` и других.

- [ ] **Step 2: Команды и события**

`crates/ferrum-wasm/src/protocol.rs` (целиком):

````rust
//! Сообщения между основным потоком и worker'ом. Типы описаны здесь, TypeScript-
//! типы генерирует `tsify` (они попадают в `ferrum_wasm.d.ts` при сборке
//! wasm-bindgen). Через `postMessage` они идут как обычные объекты
//! (serde-wasm-bindgen), байты — `Uint8Array`, буфер которых передаётся как
//! transferable.
//!
//! Команды от основного потока не доверенные: неизвестная команда, лишние или
//! неверные поля дают событие `Error`, а не панику worker'а.

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use tsify::Tsify;

pub use crate::layers::{LayerRow, NodeType};
pub use crate::summary::{
    Align, FillDto, Flag, Num, PaintKind, Paints, ScaleModeDto, StrokeDto, Summary,
};

/// Наибольшая страница-пример: столько узлов генерирует `Demo`.
pub const MAX_DEMO_NODES: u32 = 500_000;
/// Сколько идентификаторов выделения уходит в событии `Selection`: панели
/// получают из worker'а готовые срезы, полный список в основной поток не нужен.
pub const SELECTION_IDS_MAX: usize = 1000;
/// Предел байтов строки в команде (имя, запрос поиска, сочетание): имена режутся
/// до 256 символов, но строка из основного потока может быть любой длины.
pub const MAX_TEXT_BYTES: usize = 4096;
/// Масштабы экспорта PNG (раздел 8.7 спеки).
pub const EXPORT_SCALES: [f64; 3] = [1.0, 2.0, 3.0];

/// Инструмент.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    Move,
    Frame,
    Rect,
    Ellipse,
    Line,
    Hand,
    Image,
}

/// Тема оверлеев.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    Dark,
    Light,
}

/// CSS-курсор над холстом. У ресайза и поворота — угол на экране в градусах.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum CursorKind {
    Default,
    Crosshair,
    Grab,
    Grabbing,
    Duplicate,
    Resize,
    Rotate,
}

/// Бэкенд рендера (`layout::BACKENDS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Backend {
    Webgpu,
    Webgl,
    Cpu,
}

impl Backend {
    /// Индекс в `layout::BACKENDS` (живое состояние).
    pub fn code(self) -> f64 {
        match self {
            Backend::Webgpu => 1.0,
            Backend::Webgl => 2.0,
            Backend::Cpu => 3.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Backend::Webgpu => "webgpu",
            Backend::Webgl => "webgl",
            Backend::Cpu => "cpu",
        }
    }

    pub fn parse(s: &str) -> Option<Backend> {
        match s {
            "webgpu" => Some(Backend::Webgpu),
            "webgl" => Some(Backend::Webgl),
            "cpu" => Some(Backend::Cpu),
            _ => None,
        }
    }
}

/// Как выделять из панели слоёв: заменить, добавить или убрать (Ctrl/Cmd),
/// диапазон от прошлого щелчка (Shift).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum SelectMode {
    Replace,
    Toggle,
    Range,
}

/// Куда бросили слои относительно строки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum DropPosition {
    Above,
    Below,
    Into,
}

/// Числовое поле панели свойств.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Field {
    X,
    Y,
    W,
    H,
    Rotation,
    Opacity,
    CornerRadius,
    StrokeWidth,
}

/// Фаза правки из панели: превью жеста, запись одним шагом, отмена жеста.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum EditPhase {
    Preview,
    Commit,
    Cancel,
}

/// Правка выделения из панели свойств.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Edit {
    Number {
        field: Field,
        value: f64,
        relative: bool,
    },
    Fills {
        fills: Vec<FillDto>,
    },
    Strokes {
        strokes: Vec<StrokeDto>,
    },
    ClipContent {
        value: bool,
    },
    Size {
        w: f64,
        h: f64,
    },
}

/// Команда основного потока worker'у.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Command {
    /// Открыть файл `.ferrum`.
    Open {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Новый документ — страница-пример из `nodes` узлов (фреймы по 100 узлов).
    Demo {
        nodes: u32,
    },
    /// Команда редактора по ключу таблицы команд (`edit.undo`, `tool.rect`…).
    Execute {
        command: String,
    },
    SetTool {
        tool: Tool,
    },
    SetTheme {
        theme: Theme,
    },
    /// Вставить содержимое буфера обмена (формат `web application/x-ferrum`).
    Paste {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Разместить изображение (PNG): следующий клик ставит его на холст.
    PlaceImage {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Сохранить документ: ответ — событие `Saved`.
    Save,
    /// Прочитать пиксели текущего кадра (для тестов): ответ — `Pixels`.
    /// `reference` — тот же кадр, нарисованный `vello_cpu`, для сравнения с GPU.
    ReadPixels {
        reference: bool,
    },
    // ——— Этап 5: панели ———
    /// Окно строк панели слоёв, которое сейчас видно (с запасом).
    LayersWindow {
        start: u32,
        count: u32,
    },
    ExpandLayer {
        id: String,
        expanded: bool,
    },
    /// Поиск по слоям; пустая строка — без поиска.
    FilterLayers {
        query: String,
    },
    /// Выделение из панели слоёв (не больше `SELECTION_IDS_MAX` идентификаторов).
    Select {
        ids: Vec<String>,
        mode: SelectMode,
    },
    /// Наведение на строку слоя — подсветка узла на холсте.
    HoverLayer {
        id: Option<String>,
    },
    RenameLayer {
        id: String,
        name: String,
    },
    /// Перенос слоёв к строке `target`. Без `ids` — текущее выделение целиком
    /// (оно может быть больше того, что видит панель).
    MoveLayers {
        ids: Option<Vec<String>>,
        target: String,
        position: DropPosition,
    },
    /// Скрыть или заблокировать (кнопки строки слоя).
    SetLayerFlags {
        ids: Vec<String>,
        visible: Option<bool>,
        locked: Option<bool>,
    },
    /// Правка выделения из панели свойств.
    EditSelection {
        edit: Edit,
        phase: EditPhase,
    },
    /// Новая страница; она становится текущей.
    AddPage,
    RenamePage {
        id: String,
        name: String,
    },
    DeletePage {
        id: String,
    },
    SetPage {
        id: String,
    },
    /// Фон текущей страницы (`#rrggbb[aa]`); без цвета — как у холста в теме.
    /// Жест палитры — превью только в кадре (`preview`), в документ — при
    /// отпускании (`commit`), `cancel` возвращает фон документа.
    SetPageBackground {
        color: Option<String>,
        phase: EditPhase,
    },
    SetDocumentName {
        name: String,
    },
    /// Фрейм из пресета (номер в списке события `Presets`).
    AddFramePreset {
        index: u32,
    },
    /// Зум к центру холста (1 — 100 %).
    SetZoom {
        zoom: f64,
    },
    /// Экспорт PNG выделения (без выделения — всей страницы): ответ — `Exported`.
    Export {
        scale: f64,
    },
    /// Изображение из системного буфера: встаёт в центр видимой части.
    PasteImage {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Назначить сочетание. Занято другими и `replace: false` — ответ `BindConflict`.
    Bind {
        command: String,
        shortcut: String,
        replace: bool,
    },
    Unbind {
        command: String,
        shortcut: String,
    },
    /// Сбросить сочетания команды; без команды — всех.
    ResetKeymap {
        command: Option<String>,
    },
    /// Отличия раскладки в JSON (`ferrum-keymap`) — из настроек или из файла.
    ImportKeymap {
        json: String,
    },
    SetSettings {
        pixel_snap: bool,
    },
}

/// Страница в списке.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub id: String,
    pub name: String,
}

/// Команда в таблице раскладки: ключ, название, группа (`tool`, `edit`…),
/// действующие сочетания (каноническая запись и подпись US) и умолчания.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct KeymapEntry {
    pub key: String,
    pub name: String,
    pub group: String,
    pub bindings: Vec<String>,
    pub labels: Vec<String>,
    pub defaults: Vec<String>,
    pub overridden: bool,
}

/// Сочетание, занятое несколькими командами (ключи команд).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub shortcut: String,
    pub label: String,
    pub commands: Vec<String>,
}

/// Пресет фрейма.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub group: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
}

/// Событие worker'а основному потоку.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CoreEvent {
    /// Ядро запущено: бэкенд рендера и почему не подошли более быстрые.
    Ready {
        backend: Backend,
        fallbacks: Vec<String>,
    },
    /// Документ открыт; предупреждения — о частях повреждённого файла.
    Opened {
        pages: u32,
        nodes: u32,
        warnings: Vec<String>,
        /// Имя документа (для шапки и «Сохранить как…»).
        name: String,
    },
    /// Выделение: число узлов и первые `SELECTION_IDS_MAX` идентификаторов.
    Selection {
        count: u32,
        ids: Vec<String>,
    },
    Tool {
        tool: Tool,
    },
    Cursor {
        cursor: CursorKind,
        angle: f64,
    },
    /// Документ изменён правкой, отменой или повтором.
    Document {
        can_undo: bool,
        can_redo: bool,
        steps: f64,
    },
    Page {
        id: String,
    },
    /// Байты для системного буфера обмена (`web application/x-ferrum`).
    Clipboard {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// PNG выделения для буфера обмена («Скопировать как PNG»).
    ClipboardPng {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Пора прочитать системный буфер и прислать `Paste`.
    PasteRequested,
    /// Выбрать файл изображения и прислать `PlaceImage`.
    RequestImage,
    ToggleUi,
    /// Сохранённый файл `.ferrum`.
    Saved {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Пиксели кадра RGBA, строки сверху вниз.
    Pixels {
        backend: Backend,
        width: u32,
        height: u32,
        #[tsify(type = "Uint8Array")]
        rgba: ByteBuf,
    },
    /// Сообщение об ошибке для пользователя.
    Error {
        message: String,
    },
    // ——— Этап 5: панели ———
    /// Окно строк панели слоёв. `reveal` — строка первого выделенного, если
    /// выделение пришло с холста (панель прокручивает к ней).
    Layers {
        start: u32,
        total: u32,
        rows: Vec<LayerRow>,
        reveal: Option<u32>,
    },
    /// Сводка выделения для панели свойств.
    Properties {
        summary: Summary,
    },
    /// Страницы, текущая и её фон (нет — как у холста в теме).
    Pages {
        pages: Vec<PageInfo>,
        current: String,
        background: Option<String>,
    },
    /// Раскладка: все команды, конфликты, сочетания браузера и отличия от
    /// умолчаний в JSON (их интерфейс хранит в настройках).
    Keymap {
        commands: Vec<KeymapEntry>,
        conflicts: Vec<ConflictInfo>,
        reserved: Vec<String>,
        overrides: String,
    },
    /// Сочетание занято: интерфейс предлагает заменить (`Bind` с `replace`).
    BindConflict {
        command: String,
        shortcut: String,
        commands: Vec<String>,
    },
    /// Раскладка импортирована; предупреждения — о пропущенном.
    KeymapImported {
        warnings: Vec<String>,
    },
    Presets {
        presets: Vec<PresetInfo>,
    },
    /// PNG экспорта и имя файла.
    Exported {
        name: String,
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Имя документа изменилось.
    DocumentName {
        name: String,
    },
}
````

- [ ] **Step 3: Ядро: панели, пределы, события по поводу**

Обработчики — в дочернем модуле `core/panels.rs`: ему видны поля `Core`. `flush_ui` в конце `drain_editor` шлёт окно слоёв, сводку и страницы, только если их пометили изменившимися.

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust

use std::collections::VecDeque;

````

на:

````rust

mod panels;

use std::collections::{HashSet, VecDeque};

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
use crate::input::{decode_all, Record};
use crate::live::LiveState;
use crate::protocol::{
    Backend, Command, CoreEvent, CursorKind, Theme, Tool, MAX_DEMO_NODES, SELECTION_IDS_MAX,
};

/// Сколько последних кадров учитывают медиана и 95-й процентиль времени кадра.
pub const FRAME_WINDOW: usize = 120;
/// Фон холста (до цвета фона страницы в документе): тёмная и светлая темы.
pub const CANVAS_DARK: Rgba = Rgba::new([0.141, 0.137, 0.133, 1.0]);
pub const CANVAS_LIGHT: Rgba = Rgba::new([0.949, 0.941, 0.929, 1.0]);
/// Масштаб «Скопировать как PNG».
````

на:

````rust
use crate::input::{decode_all, Record};
use crate::layers::Layers;
use crate::live::LiveState;
use crate::protocol::{
    Backend, Command, CoreEvent, CursorKind, Theme, Tool, MAX_DEMO_NODES, SELECTION_IDS_MAX,
};
use crate::summary::Summary;

/// Сколько последних кадров учитывают медиана и 95-й процентиль времени кадра.
pub const FRAME_WINDOW: usize = 120;
/// Фон холста, если у страницы свой не задан: токен `canvas` дизайн-системы
/// (#1a1918 в тёмной теме, #e9e7e4 в светлой).
pub const CANVAS_DARK: Rgba = Rgba::new([26.0 / 255.0, 25.0 / 255.0, 24.0 / 255.0, 1.0]);
pub const CANVAS_LIGHT: Rgba = Rgba::new([233.0 / 255.0, 231.0 / 255.0, 228.0 / 255.0, 1.0]);
/// Масштаб «Скопировать как PNG».
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    atlas: (usize, u64),
}
````

на:

````rust
    atlas: (usize, u64),
    /// Панели (этап 5): дерево слоёв, выделенные для строк, что пересчитать.
    layers: Layers,
    selected: HashSet<NodeId>,
    ui: UiDirty,
    /// Выделение меняет панель слоёв: предков не раскрывать, к строке не прокручивать.
    from_panel: bool,
    reveal: Option<u32>,
    summary: Option<Summary>,
    /// Фон страницы во время жеста палитры: в кадре, но ещё не в документе.
    background_preview: Option<Option<ferrum_doc::Color>>,
}

/// Что из панелей пересчитать после пачки событий редактора.
#[derive(Debug, Default, Clone, Copy)]
struct UiDirty {
    layers: bool,
    properties: bool,
    pages: bool,
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            .expect("в новом документе есть страница");
        Core {
            editor,
````

на:

````rust
            .expect("в новом документе есть страница");
        let mut core = Core {
            editor,
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            atlas: (0, 0),
        }
    }
````

на:

````rust
            atlas: (0, 0),
            layers: Layers::new(),
            selected: HashSet::new(),
            ui: UiDirty::default(),
            from_panel: false,
            reveal: None,
            summary: None,
            background_preview: None,
        };
        // Таблица команд и пресеты нужны интерфейсу сразу, до документа.
        core.push_keymap();
        core.push_presets();
        core
    }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust

    /// Заменяет документ: новый редактор с тем же холстом и темой.
    fn replace_document(&mut self, doc: Document) -> Result<(), String> {
````

на:

````rust

    /// Заменяет документ: новый редактор с тем же холстом, темой, раскладкой клавиш
    /// и настройками — они принадлежат пользователю, а не документу.
    fn replace_document(&mut self, doc: Document) -> Result<(), String> {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        editor.set_theme(overlay_theme(self.theme));
        editor.take_events();
        self.editor = editor;
        self.images = DocImages::default();
        self.selection = None;
        self.dirty = true;
````

на:

````rust
        editor.set_theme(overlay_theme(self.theme));
        editor.set_settings(self.editor.settings().clone());
        *editor.keymap_mut() = self.editor.keymap().clone();
        editor.take_events();
        self.editor = editor;
        self.images = DocImages::default();
        self.selection = None;
        self.layers.reset();
        self.selected.clear();
        self.summary = None;
        self.dirty = true;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        let nodes = self.editor.scene().len() as u32;
        self.events.push(CoreEvent::Opened {
            pages,
            nodes,
            warnings,
        });
````

на:

````rust
        let nodes = self.editor.scene().len() as u32;
        let name = self.editor.document().name();
        self.events.push(CoreEvent::Opened {
            pages,
            nodes,
            warnings,
            name,
        });
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            tool: tool(self.editor.tool_kind()),
        });
    }

````

на:

````rust
            tool: tool(self.editor.tool_kind()),
        });
        self.ui = UiDirty {
            layers: true,
            properties: true,
            pages: true,
        };
        self.flush_ui();
    }

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                self.dirty = true;
            }
        }
        self.drain_editor();
````

на:

````rust
                self.dirty = true;
            }
            other => self.panel_command(other),
        }
        self.drain_editor();
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                EditorEvent::NeedsRedraw => self.dirty = true,
                EditorEvent::DocumentChanged => document = true,
                EditorEvent::SelectionChanged(ids) => selection = Some(ids),
````

на:

````rust
                EditorEvent::NeedsRedraw => self.dirty = true,
                EditorEvent::DocumentChanged => {
                    document = true;
                    self.layers.invalidate();
                    self.ui.layers = true;
                    self.ui.properties = true;
                }
                EditorEvent::SelectionChanged(ids) => selection = Some(ids),
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                EditorEvent::PageChanged(p) => {
                    self.events.push(CoreEvent::Page { id: p.to_string() })
                }
````

на:

````rust
                EditorEvent::PageChanged(p) => {
                    self.events.push(CoreEvent::Page { id: p.to_string() });
                    self.background_preview = None;
                    self.layers.reset();
                    self.ui.layers = true;
                    self.ui.pages = true;
                    self.dirty = true;
                }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            ));
        }
````

на:

````rust
            ));
            self.selected = ids.iter().copied().collect();
            // Выделение с холста: раскрыть предков и прокрутить панель к строке.
            if !self.from_panel && !ids.is_empty() {
                self.reveal = self.layers.reveal(self.editor.scene(), &ids);
            }
            self.ui.layers = true;
            self.ui.properties = true;
        }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            self.push_document();
        }
    }

````

на:

````rust
            self.push_document();
        }
        self.flush_ui();
    }

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust

    pub fn background(&self) -> Rgba {
        match self.theme {
````

на:

````rust

    /// Фон кадра: цвет фона страницы, если задан, иначе холст темы.
    pub fn background(&self) -> Rgba {
        let page = match self.background_preview {
            Some(preview) => preview,
            None => self.editor.page_background(),
        };
        if let Some(c) = page {
            let k = |v: u8| v as f32 / 255.0;
            return Rgba::new([k(c.r), k(c.g), k(c.b), k(c.a)]);
        }
        match self.theme {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        if let [one] = ids {
            if let Some(n) = scene.node(*one) {
                info.x = n.props.x;
                info.y = n.props.y;
                info.w = n.props.w;
                info.h = n.props.h;
                info.rotation = n.props.rotation;
            }
````

на:

````rust
        if let [one] = ids {
            // Те же значения, что в сводке панели (у группы — рамка детей, поворот
            // как в Figma): во время жеста панель читает их отсюда.
            if let Some(slot) = scene.slot(*one) {
                let p = ferrum_editor::panel_pose(scene, slot);
                (info.x, info.y, info.w, info.h, info.rotation) = (p.x, p.y, p.w, p.h, p.rotation);
            }
````

`crates/ferrum-wasm/src/core/panels.rs`:

````rust
//! Команды панелей (этап 5): слои, свойства, страницы, экспорт, раскладка,
//! настройки — и события для них. Всё, что приходит из основного потока, не
//! доверенное: идентификаторы, строки и списки проверяются и ограничиваются,
//! неверное — событие `Error` на русском, документ не меняется.

use ferrum_doc::{Color, Fill, FillKind, NodeId, PageId, ScaleMode, Stroke, StrokeAlign};
use ferrum_editor::{
    browser_reserved, frame_presets, BindError, CommandId, DropTarget, KeymapError, NumField,
    PanelEdit, Phase, Shortcut, COMMANDS,
};
use ferrum_render::decode_png;
use ferrum_scene::shape::COORD_LIMIT;
use serde_bytes::ByteBuf;

use super::Core;
use crate::protocol::{
    Align, Command, ConflictInfo, CoreEvent, DropPosition, Edit, EditPhase, Field, FillDto,
    KeymapEntry, PageInfo, PaintKind, PresetInfo, ScaleModeDto, SelectMode, StrokeDto,
    EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{summarize, MAX_PAINTS};

/// Имя файла экспорта: без знаков, которые не любят файловые системы.
fn file_name(name: &str, scale: f64) -> String {
    let base: String = name
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let base = base.trim();
    let base = if base.is_empty() { "Ferrum" } else { base };
    format!("{base}@{scale}x.png")
}

fn field(f: Field) -> NumField {
    match f {
        Field::X => NumField::X,
        Field::Y => NumField::Y,
        Field::W => NumField::W,
        Field::H => NumField::H,
        Field::Rotation => NumField::Rotation,
        Field::Opacity => NumField::Opacity,
        Field::CornerRadius => NumField::CornerRadius,
        Field::StrokeWidth => NumField::StrokeWidth,
    }
}

fn phase(p: EditPhase) -> Phase {
    match p {
        EditPhase::Preview => Phase::Preview,
        EditPhase::Commit => Phase::Commit,
        EditPhase::Cancel => Phase::Cancel,
    }
}

fn color(s: &str) -> Result<Color, String> {
    if s.len() > 16 {
        return Err("Неверный цвет".into());
    }
    Color::parse(s).map_err(|e| e.to_string())
}

/// Хэш изображения — 64 шестнадцатеричных знака (blake3).
fn image_hash(h: &str) -> Option<String> {
    (h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())).then(|| h.to_ascii_lowercase())
}

fn fill(d: &FillDto) -> Result<Fill, String> {
    let opacity = if d.opacity.is_finite() {
        d.opacity.clamp(0.0, 1.0)
    } else {
        return Err("Неверная прозрачность заливки".into());
    };
    let mut f = match d.kind {
        PaintKind::Solid => Fill::solid(color(&d.color)?),
        PaintKind::Image => {
            let hash = d
                .image_hash
                .as_deref()
                .and_then(image_hash)
                .ok_or("У заливки изображением нет хэша")?;
            let mut f = Fill::image(hash);
            f.scale_mode = match d.scale_mode.unwrap_or(ScaleModeDto::Fill) {
                ScaleModeDto::Fill => ScaleMode::Fill,
                ScaleModeDto::Fit => ScaleMode::Fit,
                ScaleModeDto::Tile => ScaleMode::Tile,
            };
            f.color = color(&d.color)?;
            f
        }
    };
    debug_assert!(f.kind == FillKind::Solid || f.image_hash.is_some());
    f.opacity = opacity;
    f.visible = d.visible;
    Ok(f)
}

fn stroke(d: &StrokeDto) -> Result<Stroke, String> {
    if !d.width.is_finite() {
        return Err("Неверная толщина обводки".into());
    }
    Ok(Stroke {
        color: color(&d.color)?,
        width: d.width.clamp(0.0, COORD_LIMIT),
        align: match d.align {
            Align::Inside => StrokeAlign::Inside,
            Align::Center => StrokeAlign::Center,
            Align::Outside => StrokeAlign::Outside,
        },
    })
}

fn panel_edit(e: &Edit) -> Result<PanelEdit, String> {
    Ok(match e {
        Edit::Number {
            field: f,
            value,
            relative,
        } => {
            if !value.is_finite() {
                return Err("Значение поля — не число".into());
            }
            PanelEdit::Number {
                field: field(*f),
                value: *value,
                relative: *relative,
            }
        }
        Edit::Fills { fills } => {
            if fills.len() > MAX_PAINTS {
                return Err(format!("Заливок не больше {MAX_PAINTS}"));
            }
            PanelEdit::Fills(fills.iter().map(fill).collect::<Result<_, _>>()?)
        }
        Edit::Strokes { strokes } => {
            if strokes.len() > MAX_PAINTS {
                return Err(format!("Обводок не больше {MAX_PAINTS}"));
            }
            PanelEdit::Strokes(strokes.iter().map(stroke).collect::<Result<_, _>>()?)
        }
        Edit::ClipContent { value } => PanelEdit::ClipContent(*value),
        Edit::Size { w, h } => {
            if !w.is_finite() || !h.is_finite() {
                return Err("Размер — не число".into());
            }
            PanelEdit::Size { w: *w, h: *h }
        }
    })
}

impl Core {
    /// Строка из команды не длиннее `MAX_TEXT_BYTES`.
    fn text(&mut self, s: &str) -> bool {
        if s.len() > MAX_TEXT_BYTES {
            self.error("Слишком длинная строка в команде");
            return false;
        }
        true
    }

    fn node(&mut self, s: &str) -> Option<NodeId> {
        match s.parse() {
            Ok(id) if s.len() <= 64 => Some(id),
            _ => {
                self.error("Неверный идентификатор слоя");
                None
            }
        }
    }

    fn nodes(&mut self, list: &[String]) -> Option<Vec<NodeId>> {
        if list.len() > SELECTION_IDS_MAX {
            self.error(format!("Не больше {SELECTION_IDS_MAX} слоёв в команде"));
            return None;
        }
        list.iter().map(|s| self.node(s)).collect()
    }

    fn page_id(&mut self, s: &str) -> Option<PageId> {
        match s.parse::<PageId>() {
            Ok(id) if s.len() <= 64 && self.editor.document().pages().iter().any(|p| p.0 == id) => {
                Some(id)
            }
            _ => {
                self.error("Страница не найдена");
                None
            }
        }
    }

    pub(super) fn panel_command(&mut self, cmd: Command) {
        match cmd {
            Command::LayersWindow { start, count } => {
                self.layers.set_window(start, count);
                self.ui.layers = true;
            }
            Command::ExpandLayer { id, expanded } => {
                if let Some(id) = self.node(&id) {
                    self.layers.set_expanded(id, expanded);
                    self.ui.layers = true;
                }
            }
            Command::FilterLayers { query } => {
                if self.text(&query) {
                    self.layers.set_filter(&query);
                    self.ui.layers = true;
                }
            }
            Command::Select { ids, mode } => {
                if let Some(ids) = self.nodes(&ids) {
                    self.select(ids, mode);
                }
            }
            Command::HoverLayer { id } => match id {
                None => self.editor.set_hover(None),
                Some(s) => {
                    if let Some(id) = self.node(&s) {
                        self.editor.set_hover(Some(id));
                    }
                }
            },
            Command::RenameLayer { id, name } => {
                if self.text(&name) {
                    if let Some(id) = self.node(&id) {
                        self.editor.rename_node(id, &name);
                        // Пустое имя ничего не меняет: панель должна вернуть прежнее.
                        self.layers.resend();
                        self.ui.layers = true;
                    }
                }
            }
            Command::MoveLayers {
                ids,
                target,
                position,
            } => {
                let ids = match ids {
                    Some(list) => self.nodes(&list),
                    None => Some(self.editor.selection().to_vec()),
                };
                if let (Some(ids), Some(t)) = (ids, self.node(&target)) {
                    let target = match position {
                        DropPosition::Above => DropTarget::Above(t),
                        DropPosition::Below => DropTarget::Below(t),
                        DropPosition::Into => DropTarget::Into(t),
                    };
                    if !self.editor.move_layers(&ids, target) {
                        // Перенос не удался (в себя, в лист): строки вернуть как были.
                        self.layers.resend();
                        self.ui.layers = true;
                    }
                }
            }
            Command::SetLayerFlags {
                ids,
                visible,
                locked,
            } => {
                if let Some(ids) = self.nodes(&ids) {
                    self.editor.set_flags(&ids, visible, locked);
                }
            }
            Command::EditSelection { edit, phase: p } => match panel_edit(&edit) {
                Ok(e) => self.editor.edit_selection(&e, phase(p)),
                Err(m) => {
                    // Жест не должен остаться висеть из-за неверного шага.
                    if p != EditPhase::Preview {
                        self.editor.cancel_gesture();
                    }
                    self.error(format!("Неверная правка: {m}"));
                }
            },
            Command::AddPage => {
                if let Err(e) = self.editor.create_page() {
                    self.error(e.to_string());
                }
                self.ui.pages = true;
            }
            Command::RenamePage { id, name } => {
                if self.text(&name) {
                    if let Some(id) = self.page_id(&id) {
                        if let Err(e) = self.editor.rename_page(id, &name) {
                            self.error(e.to_string());
                        }
                    }
                }
                self.ui.pages = true;
            }
            Command::DeletePage { id } => {
                if let Some(id) = self.page_id(&id) {
                    if let Err(e) = self.editor.delete_page(id) {
                        self.error(e.to_string());
                    }
                }
                self.ui.pages = true;
            }
            Command::SetPage { id } => {
                if let Some(id) = self.page_id(&id) {
                    if id != self.editor.page() {
                        if let Err(e) = self.editor.set_page(id) {
                            self.error(e.to_string());
                        }
                    }
                }
            }
            Command::SetPageBackground { color: c, phase } => {
                let parsed = match c.as_deref().map(color).transpose() {
                    Ok(v) => v,
                    Err(m) => {
                        self.error(m);
                        return;
                    }
                };
                self.dirty = true;
                match phase {
                    // Фон страницы не в истории отмены, но и сотни записей в документ
                    // за один жест палитры не нужны: превью живёт только в кадре.
                    EditPhase::Preview => self.background_preview = Some(parsed),
                    EditPhase::Cancel => self.background_preview = None,
                    EditPhase::Commit => {
                        self.background_preview = None;
                        let page = self.editor.page();
                        if let Err(e) = self.editor.set_page_background(page, parsed) {
                            self.error(e.to_string());
                        }
                        self.ui.pages = true;
                    }
                }
            }
            Command::SetDocumentName { name } => {
                if self.text(&name) {
                    match self.editor.set_document_name(&name) {
                        Ok(name) => self.events.push(CoreEvent::DocumentName { name }),
                        Err(e) => self.error(e.to_string()),
                    }
                }
            }
            Command::AddFramePreset { index } => match frame_presets().get(index as usize) {
                Some(p) => self.editor.add_frame_preset(p),
                None => self.error("Нет такого пресета"),
            },
            Command::SetZoom { zoom } => {
                if zoom.is_finite() && zoom > 0.0 {
                    let mut cam = self.editor.camera().clone();
                    cam.set_zoom(zoom);
                    self.editor.set_camera(cam);
                } else {
                    self.error("Неверный масштаб");
                }
            }
            Command::Export { scale } => self.export(scale),
            Command::PasteImage { bytes } => self.paste_image(bytes),
            Command::Bind {
                command,
                shortcut,
                replace,
            } => self.bind(&command, &shortcut, replace),
            Command::Unbind { command, shortcut } => {
                if let (Some(id), Some(s)) = (self.command_id(&command), self.shortcut(&shortcut)) {
                    self.editor.keymap_mut().unbind(id, &s);
                    self.push_keymap();
                }
            }
            Command::ResetKeymap { command } => {
                match command {
                    None => self.editor.keymap_mut().reset_all(),
                    Some(c) => match self.command_id(&c) {
                        Some(id) => self.editor.keymap_mut().reset(id),
                        None => return,
                    },
                }
                self.push_keymap();
            }
            Command::ImportKeymap { json } => match self.editor.keymap_mut().import_json(&json) {
                Ok(warnings) => {
                    self.events.push(CoreEvent::KeymapImported { warnings });
                    self.push_keymap();
                }
                // Текст разбора JSON — английский: пользователю — своими словами.
                Err(KeymapError::BadFormat(_)) => self.error(
                    "Не удалось импортировать раскладку: это не файл раскладки Ferrum (JSON)",
                ),
                Err(e) => self.error(format!("Не удалось импортировать раскладку: {e}")),
            },
            Command::SetSettings { pixel_snap } => {
                let mut s = self.editor.settings().clone();
                s.pixel_snap = pixel_snap;
                self.editor.set_settings(s);
            }
            // Команды этапа 4 разбирает `Core::command`.
            _ => {}
        }
    }

    fn select(&mut self, ids: Vec<NodeId>, mode: SelectMode) {
        let next = match mode {
            SelectMode::Replace => {
                self.layers.set_anchor(ids.last().copied());
                ids
            }
            SelectMode::Toggle => {
                let mut cur = self.editor.selection().to_vec();
                for id in &ids {
                    match cur.iter().position(|c| c == id) {
                        Some(i) => {
                            cur.remove(i);
                        }
                        None => cur.push(*id),
                    }
                }
                self.layers.set_anchor(ids.last().copied());
                cur
            }
            SelectMode::Range => {
                let Some(to) = ids.last().copied() else {
                    return;
                };
                let from = self.layers.anchor().unwrap_or(to);
                self.layers.range(self.editor.scene(), from, to)
            }
        };
        self.from_panel = true;
        self.editor.select(&next);
        self.drain_editor();
        self.from_panel = false;
    }

    fn command_id(&mut self, key: &str) -> Option<CommandId> {
        let id = CommandId::from_key(key);
        if id.is_none() {
            self.error(format!("Неизвестная команда «{}»", super::clip(key)));
        }
        id
    }

    fn shortcut(&mut self, s: &str) -> Option<Shortcut> {
        if s.len() > 64 {
            self.error("Неверное сочетание");
            return None;
        }
        match Shortcut::parse(s) {
            Ok(sc) => Some(sc),
            Err(e) => {
                self.error(e.to_string());
                None
            }
        }
    }

    fn bind(&mut self, command: &str, shortcut: &str, replace: bool) {
        let (Some(id), Some(s)) = (self.command_id(command), self.shortcut(shortcut)) else {
            return;
        };
        match self.editor.keymap_mut().bind(id, s.clone(), replace) {
            Ok(()) => self.push_keymap(),
            Err(BindError::Conflict(c)) => self.events.push(CoreEvent::BindConflict {
                command: id.key().to_string(),
                shortcut: s.to_string(),
                commands: c.commands.iter().map(|c| c.key().to_string()).collect(),
            }),
            Err(e @ BindError::TooMany) => self.error(e.to_string()),
        }
    }

    /// Таблица команд и раскладка — после каждого изменения раскладки.
    pub(super) fn push_keymap(&mut self) {
        let km = self.editor.keymap();
        let platform = km.platform();
        let commands = COMMANDS
            .iter()
            .map(|c| KeymapEntry {
                key: c.key.to_string(),
                name: c.name.to_string(),
                group: c.key.split('.').next().unwrap_or("").to_string(),
                bindings: km.bindings(c.id).iter().map(|s| s.to_string()).collect(),
                labels: km
                    .bindings(c.id)
                    .iter()
                    .map(|s| s.label(platform))
                    .collect(),
                defaults: km
                    .default_bindings(c.id)
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                overridden: km.is_overridden(c.id),
            })
            .collect();
        let conflicts = km
            .conflicts()
            .into_iter()
            .map(|c| ConflictInfo {
                shortcut: c.shortcut.to_string(),
                label: c.shortcut.label(platform),
                commands: c.commands.iter().map(|id| id.key().to_string()).collect(),
            })
            .collect();
        let reserved = browser_reserved(platform)
            .iter()
            .map(|s| s.to_string())
            .collect();
        let overrides = km.export_json();
        self.events
            .retain(|e| !matches!(e, CoreEvent::Keymap { .. }));
        self.events.push(CoreEvent::Keymap {
            commands,
            conflicts,
            reserved,
            overrides,
        });
    }

    pub(super) fn push_presets(&mut self) {
        let presets = frame_presets()
            .iter()
            .map(|p| PresetInfo {
                group: p.group.clone(),
                name: p.name.clone(),
                width: p.width,
                height: p.height,
            })
            .collect();
        self.events.push(CoreEvent::Presets { presets });
    }

    fn export(&mut self, scale: f64) {
        if !EXPORT_SCALES.contains(&scale) {
            self.error("Масштаб экспорта — 1×, 2× или 3×");
            return;
        }
        let scene = self.editor.scene();
        let selection = self.editor.selection().to_vec();
        let (ids, name) = if selection.is_empty() {
            let page = self.editor.page();
            let name = self
                .editor
                .document()
                .pages()
                .into_iter()
                .find(|p| p.0 == page)
                .map(|p| p.1)
                .unwrap_or_default();
            (
                scene
                    .roots()
                    .iter()
                    .map(|s| scene.at(*s).id)
                    .collect::<Vec<_>>(),
                name,
            )
        } else {
            let name = match selection.as_slice() {
                [one] => scene
                    .node(*one)
                    .map(|n| n.props.name.clone())
                    .unwrap_or_default(),
                _ => "Выделение".to_string(),
            };
            (selection, name)
        };
        if ids.is_empty() {
            self.error("Нечего экспортировать: страница пуста");
            return;
        }
        let before = self.images.placeholders_served();
        let provider = self.images.export_provider(&self.editor.document().images);
        let result = self
            .renderer
            .export_png_nodes(self.editor.scene(), &ids, scale, &provider);
        if self.images.placeholders_served() != before {
            self.error("Не удалось экспортировать: часть изображений не читается или не помещается в память");
            return;
        }
        match result {
            Ok(png) => self.events.push(CoreEvent::Exported {
                name: file_name(&name, scale),
                bytes: ByteBuf::from(png),
            }),
            Err(e) => self.error(format!("Не удалось экспортировать: {e}")),
        }
    }

    fn paste_image(&mut self, bytes: ByteBuf) {
        let Some(pixmap) = decode_png(&bytes) else {
            self.error("Не удалось вставить изображение: нужен PNG не больше 8192 px");
            return;
        };
        let (w, h) = (pixmap.width() as f64, pixmap.height() as f64);
        let hash = self.editor.add_image(bytes.into_vec());
        self.images.insert(&hash, pixmap);
        self.editor.insert_image(&hash, w, h);
    }

    /// События панелей после пачки: окно слоёв, сводка, страницы — только если
    /// изменились.
    pub(super) fn flush_ui(&mut self) {
        let ui = std::mem::take(&mut self.ui);
        if ui.layers {
            if self.reveal.is_some() {
                self.layers.resend();
            }
            if let Some(w) = self.layers.window(self.editor.scene(), &self.selected) {
                self.events
                    .retain(|e| !matches!(e, CoreEvent::Layers { .. }));
                self.events.push(CoreEvent::Layers {
                    start: w.start,
                    total: w.total,
                    rows: w.rows,
                    reveal: self.reveal.take(),
                });
            }
        }
        if ui.properties {
            let s = summarize(self.editor.scene(), self.editor.selection());
            if self.summary.as_ref() != Some(&s) {
                self.summary = Some(s.clone());
                self.events
                    .retain(|e| !matches!(e, CoreEvent::Properties { .. }));
                self.events.push(CoreEvent::Properties { summary: s });
            }
        }
        if ui.pages {
            let doc = self.editor.document();
            let pages = doc
                .pages()
                .into_iter()
                .map(|(id, name)| PageInfo {
                    id: id.to_string(),
                    name,
                })
                .collect();
            let background = self.editor.page_background().map(|c| c.to_hex());
            self.events
                .retain(|e| !matches!(e, CoreEvent::Pages { .. }));
            self.events.push(CoreEvent::Pages {
                pages,
                current: self.editor.page().to_string(),
                background,
            });
        }
    }
}
````

В `crates/ferrum-wasm/src/live.rs` заменить:

````rust
    /// Выделение: число узлов и значения панели «Положение» — у одного узла его
    /// x, y (у родителя), ширина, высота и поворот; у нескольких — общая рамка
    /// в мире.
    selection_count => SELECTION_COUNT, "selectionCount";
````

на:

````rust
    /// Выделение: число узлов и значения панели «Положение» — у одного узла его
    /// x, y (у родителя), ширина, высота и поворот, как в панели (у группы — рамка
    /// детей, поворот в `(-180, 180]`); у нескольких — общая рамка в мире.
    selection_count => SELECTION_COUNT, "selectionCount";
````

Run: `cargo test -p ferrum-wasm --test panels`
Expected: PASS, `12 passed`.

Run: `cargo test -p ferrum-wasm`
Expected: PASS — все наборы `ferrum-wasm` (протокол — `5 passed`, ядро — `24 passed`, 1 пропущен, как в этапе 4).

Run: `cargo test --target wasm32-unknown-unknown -p ferrum-wasm --test web`
Expected: PASS, `10 passed`.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: без предупреждений.

Run: `cargo clippy --target wasm32-unknown-unknown -p ferrum-wasm --lib --tests -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 4: Цвет холста в E2E этапа 4**

Пустой холст теперь — токен `canvas` (#1a1918). Зависимости стенда ставятся из `package-lock.json` этапа 4.

В `apps/web/e2e/core.spec.ts` заменить:

````ts

const CANVAS_DARK = [36, 35, 34];

````

на:

````ts

// Токен `canvas` дизайн-системы в тёмной теме (#1a1918).
const CANVAS_DARK = [26, 25, 24];

````

В `apps/web/e2e/fallback.spec.ts` заменить:

````ts
  await expectPixel(page, at.x, at.y, [1, 120, 200]);
  await expectPixel(page, 600, l.viewportH - 10, [36, 35, 34]);
}
````

на:

````ts
  await expectPixel(page, at.x, at.y, [1, 120, 200]);
  // Пустой холст — токен `canvas` дизайн-системы (#1a1918).
  await expectPixel(page, 600, l.viewportH - 10, [26, 25, 24]);
}
````

В `apps/web/e2e/fallback.spec.ts` заменить:

````ts
test('холст больше MAX_TEXTURE_SIZE при создании: WebGL2 отклонён, рисует CPU', async ({ page }) => {
  await page.goto('/?nodes=10');
  const width = await pastWebglLimit(page);
````

на:

````ts
test('холст больше MAX_TEXTURE_SIZE при создании: WebGL2 отклонён, рисует CPU', async ({ page }) => {
  await page.goto('/debug.html?nodes=10');
  const width = await pastWebglLimit(page);
````

В `apps/web/e2e/fallback.spec.ts` заменить:

````ts
  page.on('worker', (w) => w.on('close', () => (closed = true)));
  await page.goto('/?nodes=10');
  await expect(page.locator('#hud')).toHaveText('Ядро не запустилось: истекло время ожидания', { timeout: 40_000 });
````

на:

````ts
  page.on('worker', (w) => w.on('close', () => (closed = true)));
  await page.goto('/debug.html?nodes=10');
  await expect(page.locator('#hud')).toHaveText('Ядро не запустилось: истекло время ожидания', { timeout: 40_000 });
````

Run: `cd apps/web && npm ci`
Expected: ставит 46 пакетов из `package-lock.json`.

Run: `cd apps/web && npm run wasm && npm run build && npx playwright test e2e/core.spec.ts e2e/fallback.spec.ts --project=chromium`
Expected: PASS, `28 passed` (проектные пропуски — как в этапе 4).

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-wasm apps/web/e2e
git commit -m "feat(wasm): протокол панелей — слои, свойства, страницы, экспорт, раскладка"
```

---

### Task 6: Каркас интерфейса: SolidJS, планировщик, токены

Редактор (`index.html`, `src/main.tsx`) — SolidJS поверх класса `Core` этапа 4; стенд этапа 4 переезжает в `debug.html` (`src/debug.ts`) без изменений — на нём остаются тесты ядра (E2E этапа 4 открывают `/debug.html?nodes=…`).

**Планировщик** (`scheduler.ts`, спека 4.4) — свой код по идеям fasterdom. Ядро в каждом `requestAnimationFrame` после тика транспорта и чтения живого состояния вызывает `onFrame`-обработчики — среди них кадр планировщика, поэтому его время и обновления Solid входят в бюджет основного потока, который меряет `Core.mainThreadStats`. Кадр: фаза measure (подписчики `everyFrame` читают layout и живое состояние и возвращают запись; однократные `measure`; чтения, запрошенные из чтений, — в этом же кадре), затем фаза mutate в одной `batch()` Solid: события ядра, пришедшие с прошлого кадра, записи подписчиков, однократные `mutate` (запрошенные из measure — в этом кадре, из mutate — в следующем). Ошибка одного подписчика не срывает кадр. В режиме разработки `guardLayout` оборачивает чтения layout (`getBoundingClientRect`, `offsetWidth`, `scrollTop` и другие) и предупреждает, если чтение вне measure, — один раз на имя. Ошибки ядра применяются сразу, мимо очереди: после остановки ядра кадров нет, и сообщение «Ядро остановлено: …» иначе не дошло бы до пользователя; оно становится постоянной плашкой поверх холста.

**Ядро** (`core.ts`): `onFrame`, `costed` (обработчик интерфейса с учётом его времени в бюджете), `keyFilter` (какие клавиши пересылать — `keys.ts`: не из полей ввода, меню и диалогов, не Tab, не активация кнопок, не сочетания буфера обмена), `onEvent` (подписка до запуска: раскладка и пресеты приходят вместе с `ready`). Отпускание клавиш пересылается всегда — удержание Space не залипнет.

**Состояние** (`store.ts`) — только то, что прислало ядро; строки слоёв сверяются по `id` (`reconcile`). Уведомления: одно и то же подряд — одно, не больше трёх, 6 секунд.

**Оформление**: `theme.css` — токены `tokens.json` с точными значениями (тёмная тема — `:root`, светлая — `[data-theme='light']`), `base.css` — типографика, кнопки, инструменты, поля, состояния, фокус `:focus-visible` (кольцо `select` 2px с отступом 1px), подсказки и уведомления, `prefers-reduced-motion`. Шрифты Onest и JetBrains Mono вшиты в бандл (fontsource): работают без сети, в Tauri и под COEP. Иконки — формы Lucide 1.55.0 (ISC), вставленные разметкой (`icons.tsx`, около 5 КБ на 40 форм) вместо `lucide-solid`: пакет весит 55 МБ, а компонент на иконку не нужен. Сетка Lucide — 24 px, на экране 16 px и линия 1,5 px (`vector-effect: non-scaling-stroke`), как в дизайн-системе.

Модульные тесты чистой логики идут тем же раннером Playwright без браузера (`npm run unit`, `playwright.unit.config.ts`), поэтому новых пакетов для тестов нет. Планировщик получает `batch` снаружи, и тест не зависит от Solid.

`build-wasm.mjs` передаёт cargo абсолютный `CARGO_TARGET_DIR`: относительный путь cargo считал от корня репозитория, а скрипт — от `apps/web`.

**Files:**
- Modify: `apps/web/package.json`
- Modify: `apps/web/package-lock.json (npm install)`
- Create: `apps/web/debug.html`
- Create: `apps/web/src/debug.ts`
- Delete: `apps/web/src/main.ts`
- Modify: `apps/web/index.html`
- Modify: `apps/web/e2e/helpers.ts`
- Modify: `apps/web/e2e/core.spec.ts`
- Modify: `apps/web/e2e/perf.spec.ts`
- Modify: `apps/web/src/window.d.ts`
- Create: `apps/web/playwright.unit.config.ts`
- Test: `apps/web/unit/scheduler.test.ts`
- Create: `apps/web/src/ui/scheduler.ts`
- Modify: `apps/web/src/core/core.ts`
- Modify: `apps/web/scripts/build-wasm.mjs`
- Modify: `apps/web/vite.config.ts`
- Modify: `apps/web/tsconfig.json`
- Modify: `apps/web/tsconfig.e2e.json`
- Create: `apps/web/src/vite-env.d.ts`
- Create: `apps/web/src/ui/styles/theme.css`
- Create: `apps/web/src/ui/styles/base.css`
- Create: `apps/web/src/ui/styles/app.css`
- Create: `apps/web/src/ui/icons.tsx`
- Create: `apps/web/src/ui/store.ts`
- Create: `apps/web/src/ui/context.ts`
- Create: `apps/web/src/ui/keys.ts`
- Create: `apps/web/src/ui/boot.ts`
- Create: `apps/web/src/ui/toasts.tsx`
- Create: `apps/web/src/ui/app.tsx`
- Create: `apps/web/src/main.tsx`
- Test: `apps/web/e2e/app.spec.ts`

**Interfaces:**
- Consumes: `Core` этапа 4, типы `CoreEvent`/`Command` из `ferrum_wasm.d.ts` (Task 5).
- Produces: `createScheduler({ batch, onError }) → { enqueue, measure, mutate, everyFrame, frame, phase }`, `guardLayout(protos, scheduler, warn)`, `LAYOUT_READS`; `Core.onFrame(hook)`, `Core.costed(fn)`, `CoreOptions.keyFilter`, `CoreOptions.onEvent`; `createUiStore() → { state, setState, apply, applyLive, toast }`, `UiState`; `EditorContext`, `useEditor()`, `Editor { core, store, scheduler, platform, send, run }`; `forwardKey(e, isClipboardKey)`; `boot(canvas, store, opts)`; `Icon`, `Caret`, `IconName`; `window.__ferrum.ui` — состояние интерфейса для тестов; `openApp(page, nodes)` в `e2e/helpers.ts`. Параметры адреса редактора: `nodes` (страница-пример; по умолчанию пустой документ), `backend`, `transport`.

- [ ] **Step 1: Пакеты**

Solid и шрифты — зависимости приложения, плагин сборки — для разработки. `.npmrc` с датой не меняется.

В `apps/web/package.json` заменить:

````json
    "typecheck": "tsc -p tsconfig.json && tsc -p tsconfig.worker.json && tsc -p tsconfig.e2e.json",
    "e2e": "playwright test",
    "e2e:ci": "playwright test --project=chromium"
  },
````

на:

````json
    "typecheck": "tsc -p tsconfig.json && tsc -p tsconfig.worker.json && tsc -p tsconfig.e2e.json",
    "unit": "playwright test -c playwright.unit.config.ts",
    "e2e": "playwright test",
    "e2e:ci": "playwright test --project=chromium"
  },
  "dependencies": {
    "@fontsource-variable/onest": "5.3.1",
    "@fontsource/jetbrains-mono": "5.3.0",
    "solid-js": "1.9.17"
  },
````

В `apps/web/package.json` заменить:

````json
    "typescript": "6.0.3",
    "vite": "8.3.4"
  }
````

на:

````json
    "typescript": "6.0.3",
    "vite": "8.3.4",
    "vite-plugin-solid": "2.11.14"
  }
````

Run: `cd apps/web && npm install`
Expected: 107 пакетов; `package-lock.json` — как в коммите этой задачи (новые: `solid-js 1.9.17`, `vite-plugin-solid 2.11.14`, `babel-preset-solid 1.9.16`, `@babel/core 7.29.7`, `@fontsource-variable/onest 5.3.1`, `@fontsource/jetbrains-mono 5.3.0` и их зависимости).

- [ ] **Step 2: Стенд этапа 4 — в `debug.html`**

Стенд переезжает без изменений поведения; `index.html` и `src/main.tsx` — редактор (ниже).

Run: `git rm -q apps/web/src/main.ts`
Expected: без вывода.

`apps/web/debug.html`:

````html
<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Ferrum — стенд ядра</title>
    <style>
      html,
      body {
        margin: 0;
        height: 100%;
        overflow: hidden;
        background: #242322;
      }
      #canvas {
        display: block;
        width: 100vw;
        height: 100vh;
        touch-action: none;
        outline: none;
      }
      #hud {
        position: fixed;
        top: 8px;
        left: 8px;
        padding: 6px 8px;
        border-radius: 6px;
        background: rgb(26 25 24 / 0.85);
        color: #e8e4df;
        font: 12px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace;
        white-space: pre;
        pointer-events: none;
      }
    </style>
  </head>
  <body>
    <canvas id="canvas" tabindex="0"></canvas>
    <div id="hud">запуск…</div>
    <script type="module" src="/src/debug.ts"></script>
  </body>
</html>
````

`apps/web/src/debug.ts`:

````ts
// Стенд этапа 4 (debug.html): холст на всё окно под управлением worker'а и
// отладочная плашка — для тестов ядра; редактор с панелями — index.html.
// Параметры адреса: `nodes` — размер страницы-примера (по умолчанию 1000),
// `backend` — webgpu | webgl | cpu, `transport=message` — без общей памяти,
// `atlas` — сторона единственной страницы атласа GPU (тесты вытеснения).
// `window.__ferrum` — для E2E-тестов.
//
// Буфер обмена здесь — внутренний (последние байты события `clipboard`):
// системный буфер, диалоги файлов и панели — в редакторе (src/ui).

import type { Backend, CoreEvent } from './wasm/ferrum_wasm';
import { Core } from './core/core';
import type { TransportMode } from './core/transport';
import { startHud } from './hud';

const params = new URLSearchParams(location.search);
const canvas = document.getElementById('canvas') as HTMLCanvasElement;
const hud = document.getElementById('hud') as HTMLElement;

/** Параметр адреса из списка допустимых значений; иное — как будто не задан. */
function choice<T extends string>(name: string, allowed: readonly T[]): T | undefined {
  const v = params.get(name);
  return allowed.find((a) => a === v);
}

/** Сторона атласа — целое от 1 до 65535 (u16 в ядре); иное игнорируется. */
function atlasSide(): number | undefined {
  const n = Number(params.get('atlas'));
  return Number.isInteger(n) && n >= 1 && n <= 65535 ? n : undefined;
}

async function main(): Promise<void> {
  const core = await Core.start(canvas, {
    backend: choice<Backend>('backend', ['webgpu', 'webgl', 'cpu']),
    transport: choice<TransportMode>('transport', ['sab', 'message']),
    atlasSide: atlasSide(),
  });
  // Журнал событий для тестов (после `ready` — его забирает `Core.start`).
  const events: CoreEvent[] = [];
  window.__ferrum = { core, events, ready: false };
  let clipboard: Uint8Array | null = null;
  core.on((e) => {
    events.push(e);
    if (e.type === 'error') console.warn('ferrum:', e.message);
    if (e.type === 'opened') window.__ferrum!.ready = true;
    if (e.type === 'clipboard') clipboard = e.bytes;
    // Копия: байты уходят в worker с передачей буфера, свои остаются.
    if (e.type === 'pasteRequested' && clipboard) core.send({ type: 'paste', bytes: clipboard.slice() });
  });
  startHud(hud, core);
  core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 1000) });
  canvas.focus();
}

// Отказ `Core.start` уже начинается с «Ядро не запустилось: …».
main().catch((e: unknown) => {
  hud.textContent = e instanceof Error ? e.message : `Ядро не запустилось: ${String(e)}`;
});
````

В `apps/web/e2e/helpers.ts` заменить:

````ts
export async function open(page: Page, query = '', origin = ''): Promise<void> {
  await page.goto(`${origin}/?nodes=1000${query}`);
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
````

на:

````ts
export async function open(page: Page, query = '', origin = ''): Promise<void> {
  await page.goto(`${origin}/debug.html?nodes=1000${query}`);
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
````

В `apps/web/e2e/helpers.ts` заменить:

````ts
  await page.mouse.up();
}
````

на:

````ts
  await page.mouse.up();
}

/** Редактор (index.html) со страницей-примером из `nodes` узлов. */
export async function openApp(page: Page, nodes = 1000, query = ''): Promise<void> {
  await page.goto(`/?nodes=${nodes}${query}`);
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true, null, {
    timeout: 90_000,
  });
  await page.waitForFunction(() => (window.__ferrum!.core.live?.frames ?? 0) > 0);
}
````

В `apps/web/e2e/core.spec.ts` заменить:

````ts
    await page.route('**/*.wasm', (route) => route.fulfill({ status: 200, contentType: 'application/wasm', body: 'не wasm' }));
    await page.goto('/?nodes=10');
    // Причина — ошибка компиляции wasm, а не только «не запустилось».
````

на:

````ts
    await page.route('**/*.wasm', (route) => route.fulfill({ status: 200, contentType: 'application/wasm', body: 'не wasm' }));
    await page.goto('/debug.html?nodes=10');
    // Причина — ошибка компиляции wasm, а не только «не запустилось».
````

В `apps/web/e2e/core.spec.ts` заменить:

````ts
    );
    await page.goto('/?nodes=10');
    await expect(page.locator('#hud')).toHaveText('Ядро не запустилось: скрипт worker\'а не загрузился', {
````

на:

````ts
    );
    await page.goto('/debug.html?nodes=10');
    await expect(page.locator('#hud')).toHaveText('Ядро не запустилось: скрипт worker\'а не загрузился', {
````

В `apps/web/e2e/perf.spec.ts` заменить:

````ts
  const t0 = Date.now();
  await page.goto(`/?nodes=${NODES}${query}`);
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 120_000 });
````

на:

````ts
  const t0 = Date.now();
  await page.goto(`/debug.html?nodes=${NODES}${query}`);
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 120_000 });
````

В `apps/web/src/window.d.ts` заменить:

````ts
// Стенд кладёт ядро и журнал событий в `window.__ferrum` — для E2E-тестов.

import type { Core } from './core/core';
import type { CoreEvent } from './wasm/ferrum_wasm';

````

на:

````ts
// Стенд и редактор кладут ядро и журнал событий в `window.__ferrum` — для E2E-тестов.

import type { Core } from './core/core';
import type { CoreEvent } from './wasm/ferrum_wasm';
import type { UiState } from './ui/store';

````

В `apps/web/src/window.d.ts` заменить:

````ts
      ready: boolean;
    };
````

на:

````ts
      ready: boolean;
      /** Редактор: состояние интерфейса (только чтение в тестах). */
      ui?: UiState;
    };
````

- [ ] **Step 3: Написать падающий тест планировщика**

`apps/web/playwright.unit.config.ts`:

````ts
import { defineConfig } from '@playwright/test';

// Модульные тесты чистой логики интерфейса (планировщик, поле числа, цвет,
// раскладка, буфер обмена) — тем же раннером Playwright, без браузера и сервера.
export default defineConfig({
  testDir: 'unit',
  testMatch: '**/*.test.ts',
  reporter: [['list']],
});
````

`apps/web/unit/scheduler.test.ts`:

````ts
// Планировщик: события применяются раз в кадр одной batch(), measure идёт до
// mutate, чтения layout вне measure дают предупреждение.

import { expect, test } from '@playwright/test';
import { createScheduler, guardLayout } from '../src/ui/scheduler';

function setup() {
  const log: string[] = [];
  let batches = 0;
  const s = createScheduler({
    batch: (fn) => {
      batches++;
      log.push('batch{');
      fn();
      log.push('}');
    },
  });
  return { s, log, batches: () => batches };
}

test('события между кадрами применяются в одной batch() следующего кадра', () => {
  const { s, log, batches } = setup();
  for (let i = 0; i < 3; i++) s.enqueue(() => log.push(`e${i}`));
  expect(log).toEqual([]);
  s.frame();
  expect(log).toEqual(['batch{', 'e0', 'e1', 'e2', '}']);
  expect(batches()).toBe(1);
  s.frame();
  expect(log.filter((l) => l.startsWith('e'))).toHaveLength(3);
});

test('measure раньше mutate; запись из чтения — в этом кадре, чтение из записи — в следующем', () => {
  const { s, log } = setup();
  s.everyFrame(() => {
    log.push('read');
    return () => log.push('write');
  });
  s.mutate(() => {
    log.push('m1');
    s.measure(() => log.push('next-measure'));
  });
  s.measure(() => {
    log.push('r1');
    s.mutate(() => log.push('m2'));
  });
  s.enqueue(() => log.push('event'));
  s.frame();
  expect(log).toEqual(['read', 'r1', 'batch{', 'event', 'write', 'm1', 'm2', '}']);
  log.length = 0;
  s.frame();
  expect(log).toEqual(['read', 'next-measure', 'batch{', 'write', '}']);
});

test('фаза видна подписчикам; отписка работает', () => {
  const { s } = setup();
  const phases: string[] = [];
  const off = s.everyFrame(() => {
    phases.push(s.phase());
    return () => phases.push(s.phase());
  });
  s.frame();
  expect(phases).toEqual(['measure', 'mutate']);
  expect(s.phase()).toBe('idle');
  off();
  s.frame();
  expect(phases).toHaveLength(2);
});

test('ошибка одного подписчика не срывает кадр', () => {
  const errors: unknown[] = [];
  const s = createScheduler({ batch: (fn) => fn(), onError: (e) => errors.push(e) });
  const done: string[] = [];
  s.enqueue(() => {
    throw new Error('сбой');
  });
  s.enqueue(() => done.push('после'));
  s.frame();
  expect(done).toEqual(['после']);
  expect(errors).toHaveLength(1);
});

test('чтение layout вне measure предупреждает один раз, внутри — нет', () => {
  const { s } = setup();
  const proto = {
    getBoundingClientRect: () => ({ width: 10 }),
    get offsetWidth() {
      return 10;
    },
  };
  const el = Object.create(proto) as typeof proto;
  const warnings: string[] = [];
  const restore = guardLayout([proto], s, (m) => warnings.push(m));
  s.measure(() => {
    el.getBoundingClientRect();
    void el.offsetWidth;
  });
  s.frame();
  expect(warnings).toEqual([]);
  el.getBoundingClientRect();
  el.getBoundingClientRect();
  void el.offsetWidth;
  expect(warnings).toHaveLength(2);
  expect(warnings[0]).toContain('getBoundingClientRect');
  restore();
  el.getBoundingClientRect();
  expect(warnings).toHaveLength(2);
  expect(el.getBoundingClientRect()).toEqual({ width: 10 });
});
````

В `apps/web/tsconfig.e2e.json` заменить:

````json
  },
  "include": ["e2e/**/*.ts", "src/window.d.ts", "playwright.config.ts", "vite.config.ts", "scripts/**/*.mjs"],
  "exclude": []
````

на:

````json
  },
  "include": [
    "e2e/**/*.ts",
    "unit/**/*.ts",
    "src/window.d.ts",
    "playwright.config.ts",
    "playwright.unit.config.ts",
    "vite.config.ts",
    "scripts/**/*.mjs"
  ],
  "exclude": []
````

Run: `cd apps/web && npm run unit`
Expected: FAIL — `Cannot find module '../src/ui/scheduler'`.

- [ ] **Step 4: Планировщик**

`apps/web/src/ui/scheduler.ts`:

````ts
// Планировщик интерфейса (спека 4.4) — своя реализация идей fasterdom, без кода
// Telegram Web A. Один кадр — один вызов `frame()` из `requestAnimationFrame`
// ядра (`Core.onFrame`), поэтому время планировщика и обновлений Solid входит в
// бюджет основного потока, который меряет `Core`.
//
// Порядок кадра:
// 1. `measure` — чтения layout (размеры, прокрутка) и живого состояния из общей
//    памяти. DOM ещё не тронут в этом кадре, поэтому чтения не заставляют браузер
//    пересчитывать раскладку.
// 2. `mutate` — одна `batch()`: события ядра, пришедшие с прошлого кадра, затем то,
//    что вернули подписчики `everyFrame`, затем ручные записи в DOM. Solid
//    обновляет DOM один раз на кадр, сколько бы событий ни пришло.
//
// В режиме разработки `guardLayout` предупреждает о чтении layout вне `measure`.

export type Phase = 'idle' | 'measure' | 'mutate';

/** Подписчик кадра: читает в фазе measure и может вернуть запись для mutate. */
export type FrameReader = () => (() => void) | void;

export interface Scheduler {
  /** Событие ядра: применится в `batch()` следующего кадра. */
  enqueue(apply: () => void): void;
  /** Однократное чтение layout в ближайшей фазе measure. */
  measure(fn: () => void): void;
  /** Однократная запись в DOM в ближайшей фазе mutate (запрос из measure — в том же
   * кадре, из mutate — в следующем). */
  mutate(fn: () => void): void;
  /** Подписка на каждый кадр; возвращает отписку. */
  everyFrame(reader: FrameReader): () => void;
  /** Кадр: measure, затем mutate в одной `batch()`. */
  frame(): void;
  phase(): Phase;
}

export interface SchedulerOptions {
  /** `batch` из solid-js (в тестах — заглушка). */
  batch: (fn: () => void) => void;
  /** Ошибка подписчика не останавливает кадр: она уходит сюда. */
  onError?: (e: unknown) => void;
}

export function createScheduler(opts: SchedulerOptions): Scheduler {
  let phase: Phase = 'idle';
  let events: (() => void)[] = [];
  let measures: (() => void)[] = [];
  let mutates: (() => void)[] = [];
  const readers = new Set<FrameReader>();
  const onError = opts.onError ?? ((e: unknown) => console.error(e));
  const run = (fn: () => void) => {
    try {
      fn();
    } catch (e) {
      onError(e);
    }
  };
  return {
    enqueue(apply) {
      events.push(apply);
    },
    measure(fn) {
      measures.push(fn);
    },
    mutate(fn) {
      mutates.push(fn);
    },
    everyFrame(reader) {
      readers.add(reader);
      return () => readers.delete(reader);
    },
    phase: () => phase,
    frame() {
      phase = 'measure';
      const writes: (() => void)[] = [];
      for (const r of readers) {
        run(() => {
          const w = r();
          if (w) writes.push(w);
        });
      }
      // Чтения, запрошенные из чтений, — в этом же кадре.
      while (measures.length) {
        const batch = measures;
        measures = [];
        batch.forEach(run);
      }
      phase = 'mutate';
      const evs = events;
      events = [];
      const muts = mutates;
      mutates = [];
      opts.batch(() => {
        evs.forEach(run);
        writes.forEach(run);
        muts.forEach(run);
      });
      phase = 'idle';
    },
  };
}

/** Чтения layout, о которых предупреждает `guardLayout`. */
export const LAYOUT_READS = [
  'getBoundingClientRect',
  'getClientRects',
  'offsetWidth',
  'offsetHeight',
  'offsetTop',
  'offsetLeft',
  'clientWidth',
  'clientHeight',
  'scrollWidth',
  'scrollHeight',
  'scrollTop',
  'scrollLeft',
] as const;

/** Режим разработки: оборачивает методы и геттеры `proto` (Element.prototype,
 * HTMLElement.prototype) — чтение вне фазы measure даёт одно предупреждение на имя.
 * Возвращает снятие обёрток. */
export function guardLayout(
  protos: object[],
  scheduler: Pick<Scheduler, 'phase'>,
  warn: (message: string) => void = (m) => console.warn(m),
): () => void {
  const warned = new Set<string>();
  const restore: (() => void)[] = [];
  const check = (name: string) => {
    if (scheduler.phase() !== 'measure' && !warned.has(name)) {
      warned.add(name);
      warn(`Чтение layout «${name}» вне фазы measure: перенесите его в scheduler.measure`);
    }
  };
  for (const proto of protos) {
    for (const name of LAYOUT_READS) {
      const d = Object.getOwnPropertyDescriptor(proto, name);
      if (!d) continue;
      if (typeof d.value === 'function') {
        const f = d.value as (...a: unknown[]) => unknown;
        Object.defineProperty(proto, name, {
          ...d,
          value(this: unknown, ...a: unknown[]) {
            check(name);
            return f.apply(this, a);
          },
        });
      } else if (d.get) {
        const get = d.get;
        Object.defineProperty(proto, name, {
          ...d,
          get(this: unknown) {
            check(name);
            return get.call(this);
          },
        });
      } else {
        continue;
      }
      restore.push(() => Object.defineProperty(proto, name, d));
    }
  }
  return () => restore.forEach((r) => r());
}
````

Run: `cd apps/web && npm run unit`
Expected: PASS, `5 passed`.

- [ ] **Step 5: Ядро для интерфейса и сборка**

В `apps/web/src/core/core.ts` заменить:

````ts
  atlasSide?: number;
}
````

на:

````ts
  atlasSide?: number;
  /** Пересылать ли клавишу редактору. По умолчанию — все, кроме сочетаний браузера;
   * интерфейс оставляет себе клавиши полей ввода, меню и диалогов и сочетания
   * буфера обмена (их ведут события `copy`/`cut`/`paste`). */
  keyFilter?: (e: KeyboardEvent) => boolean;
  /** Подписка до запуска: события, пришедшие вместе с `ready` (раскладка, пресеты),
   * иначе прошли бы раньше, чем вызывающий успеет вызвать `on`. */
  onEvent?: (e: CoreEvent) => void;
}
````

В `apps/web/src/core/core.ts` заменить:

````ts
  private failed: ((e: Error) => void) | null = null;
  /** Ядро остановлено (сбой worker'а): сообщение было, кадров и ввода больше нет. */
````

на:

````ts
  private failed: ((e: Error) => void) | null = null;
  private readonly frameHooks = new Set<() => void>();
  private keyFilter: (e: KeyboardEvent) => boolean = () => true;
  /** Ядро остановлено (сбой worker'а): сообщение было, кадров и ввода больше нет. */
````

В `apps/web/src/core/core.ts` заменить:

````ts
    const core = new Core(worker, transport, canvas);
    let timer = 0;
````

на:

````ts
    const core = new Core(worker, transport, canvas);
    if (opts.keyFilter) core.keyFilter = opts.keyFilter;
    if (opts.onEvent) core.on(opts.onEvent);
    let timer = 0;
````

В `apps/web/src/core/core.ts` заменить:

````ts

  /** Обработчик с учётом времени в бюджет кадра. */
````

на:

````ts

  /** Вызов в каждом кадре после тика транспорта и чтения живого состояния — внутри
   * замера бюджета (планировщик интерфейса). Возвращает отписку. */
  onFrame(hook: () => void): () => void {
    this.frameHooks.add(hook);
    return () => this.frameHooks.delete(hook);
  }

  /** Обработчик интерфейса с учётом его времени в бюджет кадра (прокрутка панелей,
   * перетаскивание полей). */
  costed<A extends unknown[]>(fn: (...args: A) => void): (...args: A) => void {
    return this.timed(fn);
  }

  /** Обработчик с учётом времени в бюджет кадра. */
````

В `apps/web/src/core/core.ts` заменить:

````ts
    this.add(window, 'keydown', (e) => {
      if (browserKey(e)) return;
      e.preventDefault();
      enc.key(sink, Kind.KEY_DOWN, e);
    });
    this.add(window, 'keyup', (e) => {
      if (!browserKey(e)) enc.key(sink, Kind.KEY_UP, e);
````

на:

````ts
    this.add(window, 'keydown', (e) => {
      if (browserKey(e) || !this.keyFilter(e)) return;
      e.preventDefault();
      enc.key(sink, Kind.KEY_DOWN, e);
    });
    this.add(window, 'keyup', (e) => {
      // Отпускание — всегда: удержание (Space — рука) не должно залипнуть, если
      // фокус ушёл в поле, пока клавиша была нажата.
      if (!browserKey(e)) enc.key(sink, Kind.KEY_UP, e);
````

В `apps/web/src/core/core.ts` заменить:

````ts
      }
      this.frameCost += performance.now() - t;
````

на:

````ts
      }
      for (const hook of this.frameHooks) hook();
      this.frameCost += performance.now() - t;
````

В `apps/web/scripts/build-wasm.mjs` заменить:

````js
const SIMD = ['-C', 'target-feature=+simd128'];
const env = { ...process.env };
for (const [name, sep] of [['CARGO_ENCODED_RUSTFLAGS', '\x1f'], ['RUSTFLAGS', ' ']]) {
````

на:

````js
const SIMD = ['-C', 'target-feature=+simd128'];
// Каталог сборки — уже абсолютный: cargo считает относительный CARGO_TARGET_DIR от
// корня репозитория (cwd ниже), а этот скрипт — от текущего каталога.
const env = { ...process.env, CARGO_TARGET_DIR: target };
for (const [name, sep] of [['CARGO_ENCODED_RUSTFLAGS', '\x1f'], ['RUSTFLAGS', ' ']]) {
````

В `apps/web/vite.config.ts` заменить:

````ts
import { defineConfig } from 'vite';

````

на:

````ts
import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

````

В `apps/web/vite.config.ts` заменить:

````ts

export default defineConfig({
  server: { headers },
  preview: { headers },
  worker: { format: 'es' },
  build: { target: 'es2022' },
});
````

на:

````ts

// Две страницы: редактор (index.html, интерфейс на SolidJS) и стенд этапа 4
// (debug.html: холст на всё окно и отладочная плашка) — для тестов ядра.
export default defineConfig({
  plugins: [solid()],
  server: { headers },
  preview: { headers },
  worker: { format: 'es' },
  build: {
    target: 'es2022',
    rolldownOptions: { input: { main: 'index.html', debug: 'debug.html' } },
  },
});
````

В `apps/web/tsconfig.json` заменить:

````json
    "skipLibCheck": true,
    "types": []
  },
  "include": ["src/**/*.ts"],
  "exclude": ["src/worker/**"]
````

на:

````json
    "skipLibCheck": true,
    "types": [],
    "jsx": "preserve",
    "jsxImportSource": "solid-js"
  },
  "include": ["src/**/*.ts", "src/**/*.tsx"],
  "exclude": ["src/worker/**"]
````

`apps/web/src/vite-env.d.ts`:

````ts
/// <reference types="vite/client" />
````

- [ ] **Step 6: Токены, иконки, состояние, запуск**

`apps/web/src/ui/styles/theme.css`:

````css
/* Токены дизайн-системы Ferrum (tokens.json, версия 1). Тёмная тема — основная,
   светлая — равноправная; тему ставит интерфейс атрибутом data-theme на <html>
   (выбор «системная» превращается в dark или light по prefers-color-scheme). */

:root {
  --font-sans: 'Onest Variable', Onest, 'Segoe UI', system-ui, sans-serif;
  --font-mono: 'JetBrains Mono', ui-monospace, Menlo, monospace;
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-6: 24px;
  --space-8: 32px;
  --row-height: 28px;
  --toolbar-height: 44px;
  --panel-width: 248px;
  --handle-size: 8px;
  --radius-sm: 4px;
  --radius-md: 6px;
  --radius-lg: 10px;
}

:root,
:root[data-theme='dark'] {
  color-scheme: dark;
  --canvas: #1a1918;
  --surface: #232120;
  --surface-raised: #2c2a28;
  --surface-hover: #353230;
  --line: #3b3835;
  --line-control: #827c74;
  --ink: #edebe8;
  --ink-muted: #a9a39c;
  --ink-faint: #77716b;
  --oxide: #f2814a;
  --on-oxide: #1a1918;
  --oxide-soft: #4a2718;
  --select: #4aa3ff;
  --on-select: #1a1918;
  --select-soft: #1b3350;
  --component: #b49cff;
  --success: #5cc98a;
  --warning: #f2c14e;
  --danger: #ff7a7a;
  --shadow-popover: 0 8px 24px rgba(0, 0, 0, 0.45), 0 0 0 1px #3b3835;
  --shadow-dialog: 0 24px 64px rgba(0, 0, 0, 0.55), 0 0 0 1px #3b3835;
}

:root[data-theme='light'] {
  color-scheme: light;
  --canvas: #e9e7e4;
  --surface: #ffffff;
  --surface-raised: #ffffff;
  --surface-hover: #f2f0ed;
  --line: #e4e1dd;
  --line-control: #8f8881;
  --ink: #1c1b1a;
  --ink-muted: #655f58;
  --ink-faint: #a19a93;
  --oxide: #b4461a;
  --on-oxide: #ffffff;
  --oxide-soft: #fbe7dc;
  --select: #1769e0;
  --on-select: #ffffff;
  --select-soft: #e3eefd;
  --component: #6d44d9;
  --success: #1d7a46;
  --warning: #8a5a00;
  --danger: #c22c2c;
  --shadow-popover: 0 8px 24px rgba(28, 27, 26, 0.12), 0 0 0 1px #e4e1dd;
  --shadow-dialog: 0 24px 64px rgba(28, 27, 26, 0.18), 0 0 0 1px #e4e1dd;
}
````

`apps/web/src/ui/styles/base.css`:

````css
/* Основа интерфейса: типографика дизайн-системы, кнопки, инструменты, поля,
   состояния (наведение, нажатие, выбор, отключение, фокус с клавиатуры),
   всплывающие подсказки и уведомления. Классы компонентов — из
   components/bundle.css дизайн-системы (префикс fe-). */

html,
body {
  margin: 0;
  height: 100%;
  overflow: hidden;
  background: var(--canvas);
  color: var(--ink);
  font-family: var(--font-sans);
  font-size: 12px;
  line-height: 16px;
  -webkit-font-smoothing: antialiased;
}

#app {
  height: 100%;
}

* {
  box-sizing: border-box;
}

button,
input,
select {
  font: inherit;
  color: inherit;
}

/* Типографика (tokens.json → type). */
.t-label {
  font-size: 11px;
  line-height: 16px;
  font-weight: 500;
  letter-spacing: 0.01em;
}
.t-strong {
  font-weight: 600;
}
.t-menu {
  font-size: 13px;
  line-height: 20px;
}
.t-heading {
  font-size: 15px;
  line-height: 20px;
  font-weight: 600;
}
.t-num {
  font: 400 11px/16px var(--font-mono);
}
.t-muted {
  color: var(--ink-muted);
}

/* Фокус с клавиатуры: кольцо 2px select с отступом 1px; мышью не показываем. */
:focus {
  outline: none;
}
:focus-visible {
  outline: 2px solid var(--select);
  outline-offset: 1px;
}

.fe-ico {
  width: 16px;
  height: 16px;
  flex: none;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.5;
  stroke-linecap: round;
  stroke-linejoin: round;
}
/* Lucide нарисован в сетке 24 px: толщина линии 1.5 px на экране при любом размере. */
.fe-ico * {
  vector-effect: non-scaling-stroke;
}

/* Button */
.fe-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  height: var(--row-height);
  padding: 0 var(--space-3);
  border: 0;
  border-radius: var(--radius-md);
  font: 500 12px/16px var(--font-sans);
  cursor: pointer;
  background: transparent;
  color: var(--ink);
  white-space: nowrap;
}
.fe-btn:hover {
  background: var(--surface-hover);
}
.fe-btn:active {
  background: var(--line);
}
.fe-btn--primary {
  background: var(--oxide);
  color: var(--on-oxide);
}
.fe-btn--primary:hover {
  background: var(--oxide);
  filter: brightness(1.08);
}
.fe-btn--secondary {
  box-shadow: inset 0 0 0 1px var(--line-control);
}
.fe-btn--danger {
  color: var(--danger);
}
.fe-btn:disabled {
  color: var(--ink-faint);
  background: transparent;
  box-shadow: none;
  cursor: default;
  filter: none;
}
.fe-btn--primary:disabled {
  background: var(--surface-hover);
}

/* Квадратная кнопка-иконка (тулбар, шапки, строки). */
.fe-tool {
  display: inline-grid;
  place-items: center;
  width: 32px;
  height: 32px;
  padding: 0;
  border: 0;
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--ink);
  cursor: pointer;
  flex: none;
}
.fe-tool:hover {
  background: var(--surface-hover);
}
.fe-tool:active {
  background: var(--line);
}
.fe-tool[aria-pressed='true'] {
  background: var(--oxide-soft);
}
.fe-tool:disabled {
  color: var(--ink-faint);
  background: transparent;
  cursor: default;
}
.fe-tool--small {
  width: 24px;
  height: 24px;
}
.fe-tool--muted {
  color: var(--ink-muted);
}
.fe-tool__caret {
  width: 8px;
  height: 8px;
  color: var(--ink-muted);
}

/* Поле ввода. */
.fe-field {
  display: flex;
  align-items: center;
  height: var(--row-height);
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
  box-shadow: inset 0 0 0 1px transparent;
  min-width: 0;
}
.fe-field:hover {
  box-shadow: inset 0 0 0 1px var(--line-control);
}
.fe-field:focus-within {
  outline: 2px solid var(--select);
  outline-offset: 1px;
}
.fe-field input {
  width: 100%;
  min-width: 0;
  height: 100%;
  border: 0;
  background: transparent;
  color: var(--ink);
  font: 400 11px/16px var(--font-mono);
  padding: 0 var(--space-2);
  outline: none;
}
.fe-field input::placeholder {
  color: var(--ink-faint);
}
.fe-field--text input {
  font: 400 12px/16px var(--font-sans);
}

/* Подсказка «Название  Клавиша»: всплывающее, surface-raised, 120 мс. */
.fe-tooltip {
  position: fixed;
  z-index: 50;
  display: flex;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-md);
  background: var(--surface-raised);
  box-shadow: var(--shadow-popover);
  color: var(--ink);
  font-size: 12px;
  line-height: 16px;
  white-space: nowrap;
  pointer-events: none;
  animation: fe-pop 120ms ease-out;
}
.fe-tooltip__key {
  color: var(--ink-muted);
}

/* Уведомления: ошибка — со словом и иконкой, не только цветом. */
.fe-toasts {
  position: fixed;
  z-index: 60;
  left: 50%;
  bottom: calc(var(--toolbar-height) + var(--space-8));
  transform: translateX(-50%);
  display: grid;
  gap: var(--space-2);
  justify-items: center;
  pointer-events: none;
}
.fe-toast {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  max-width: 560px;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-md);
  background: var(--surface-raised);
  box-shadow: var(--shadow-popover);
  color: var(--ink);
  pointer-events: auto;
  animation: fe-pop 120ms ease-out;
}
.fe-toast--error .fe-ico {
  color: var(--danger);
}

@keyframes fe-pop {
  from {
    opacity: 0;
    transform: translateY(2px);
  }
}

@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    transition: none !important;
    animation: none !important;
  }
}
````

`apps/web/src/ui/styles/app.css`:

````css
/* Раскладка редактора (Figma UI3): левая панель, холст, правая панель; тулбар
   плавает внизу по центру холста. Панели отделены от холста линией line. */

.fe-app {
  display: grid;
  grid-template-columns: var(--left-w, var(--panel-width)) minmax(0, 1fr) var(--right-w, var(--panel-width));
  height: 100%;
}
.fe-app--bare {
  grid-template-columns: 0 minmax(0, 1fr) 0;
}

.fe-stage {
  position: relative;
  grid-column: 2;
  min-width: 0;
  overflow: hidden;
  background: var(--canvas);
}
.fe-canvas {
  display: block;
  width: 100%;
  height: 100%;
  touch-action: none;
  outline: none;
}

.fe-boot {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  color: var(--ink-muted);
}
.fe-boot--error {
  color: var(--ink);
}
.fe-boot--stopped {
  padding: var(--space-6);
  background: color-mix(in srgb, var(--canvas) 85%, transparent);
  text-align: center;
}
````

`apps/web/src/ui/icons.tsx`:

````tsx
// Иконки — временный набор Lucide (ISC, © Lucide Icons and Contributors), как
// велит дизайн-система, пока нет своего. «Отразить по горизонтали» — стрелки
// влево-вправо (форма `flip-vertical-2` Lucide 1.55 — у неё ось вертикальная),
// «по вертикали» — вверх-вниз (`flip-horizontal-2`). Только нужные формы, вставленные
// разметкой SVG (около 5 КБ), а не пакет lucide-solid (55 МБ в node_modules и
// по компоненту на иконку). Сетка Lucide — 24 px; на экране 16 px, линия 1.5 px
// (`vector-effect: non-scaling-stroke` в base.css).

import type { JSX } from 'solid-js';

const ICONS = {
  move: '<path d="M4.037 4.688a.495.495 0 0 1 .651-.651l16 6.5a.5.5 0 0 1-.063.947l-6.124 1.58a2 2 0 0 0-1.438 1.435l-1.579 6.126a.5.5 0 0 1-.947.063z"/>',
  frame: '<line x1="22" x2="2" y1="6" y2="6"/><line x1="22" x2="2" y1="18" y2="18"/><line x1="6" x2="6" y1="2" y2="22"/><line x1="18" x2="18" y1="2" y2="22"/>',
  rect: '<rect width="18" height="18" x="3" y="3" rx="2"/>',
  ellipse: '<circle cx="12" cy="12" r="10"/>',
  line: '<path d="M22 2 2 22"/>',
  image: '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
  pen: '<path d="m18 13-.654-4.736a4 4 0 00-2.756-3.266l-9.163-2.9a2 2 0 00-2.017.493l-.82.819a2 2 0 00-.492 2.017l2.9 9.163a4 4 0 003.266 2.756L13 18"/><path d="M18.648 12.352a1.205 1.205 0 011.704 0l1.296 1.296a1.205 1.205 0 010 1.704l-6.296 6.296a1.205 1.205 0 01-1.704 0l-1.296-1.296a1.205 1.205 0 010-1.704z"/><path d="m3 3 6.586 6.586"/><circle cx="11" cy="11" r="2"/>',
  text: '<path d="M12 4v16"/><path d="M4 7V5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2"/><path d="M9 20h6"/>',
  comment: '<path d="M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719"/>',
  caret: '<path d="m6 9 6 6 6-6"/>',
  expand: '<path d="m9 18 6-6-6-6"/>',
  plus: '<path d="M5 12h14"/><path d="M12 5v14"/>',
  minus: '<path d="M5 12h14"/>',
  eye: '<path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0"/><circle cx="12" cy="12" r="3"/>',
  eyeOff: '<path d="M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49"/><path d="M14.084 14.158a3 3 0 0 1-4.242-4.242"/><path d="M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143"/><path d="m2 2 20 20"/>',
  lock: '<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
  unlock: '<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 9.9-1"/>',
  search: '<path d="m21 21-4.34-4.34"/><circle cx="11" cy="11" r="8"/>',
  close: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
  settings: '<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/><circle cx="12" cy="12" r="3"/>',
  group: '<path d="M3 7V5c0-1.1.9-2 2-2h2"/><path d="M17 3h2c1.1 0 2 .9 2 2v2"/><path d="M21 17v2c0 1.1-.9 2-2 2h-2"/><path d="M7 21H5c-1.1 0-2-.9-2-2v-2"/><rect width="7" height="5" x="7" y="7" rx="1"/><rect width="7" height="5" x="10" y="12" rx="1"/>',
  flipH: '<path d="M12 14v2"/><path d="M12 20v2"/><path d="M12 2v2"/><path d="M12 8v2"/><path d="M20.288 16.703A1 1 0 0022 16V8a1 1 0 00-1.712-.703l-3.99 3.991a1 1 0 00-.001 1.424z"/><path d="M3.712 16.703A1 1 0 012 16V8a1 1 0 011.712-.703l3.99 3.991a1 1 0 01.001 1.424z"/>',
  flipV: '<path d="M10 12H8"/><path d="M16 12h-2"/><path d="M22 12h-2"/><path d="M4 12H2"/><path d="M7.298 20.288A1 1 0 008 22h8a1 1 0 00.703-1.712l-3.991-3.99a1 1 0 00-1.424-.001z"/><path d="M7.298 3.712A1 1 0 018 2h8a1 1 0 01.703 1.712l-3.991 3.99a1 1 0 01-1.424.001z"/>',
  rotate: '<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/>',
  clip: '<path d="M6 2v14a2 2 0 0 0 2 2h14"/><path d="M18 22V8a2 2 0 0 0-2-2H2"/>',
  opacity: '<circle cx="15" cy="9" r="7"/><circle cx="9" cy="15" r="7"/>',
  pipette: '<path d="m12 9-8.414 8.414A2 2 0 0 0 3 18.828v1.344a2 2 0 0 1-.586 1.414A2 2 0 0 1 3.828 21h1.344a2 2 0 0 0 1.414-.586L15 12"/><path d="m18 9 .4.4a1 1 0 1 1-3 3l-3.8-3.8a1 1 0 1 1 3-3l.4.4 3.4-3.4a1 1 0 1 1 3 3z"/><path d="m2 22 .414-.414"/>',
  trash: '<path d="M10 11v6"/><path d="M14 11v6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>',
  moon: '<path d="M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401"/>',
  monitor: '<rect width="20" height="14" x="2" y="3" rx="2"/><line x1="8" x2="16" y1="21" y2="21"/><line x1="12" x2="12" y1="17" y2="21"/>',
  download: '<path d="M12 15V3"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/>',
  upload: '<path d="M12 3v12"/><path d="m17 8-5-5-5 5"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>',
  radius: '<path d="M21 11a8 8 0 0 0-8-8"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/>',
  more: '<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>',
  warning: '<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>',
  keyboard: '<path d="M10 8h.01"/><path d="M12 12h.01"/><path d="M14 8h.01"/><path d="M16 12h.01"/><path d="M18 8h.01"/><path d="M6 8h.01"/><path d="M7 16h10"/><path d="M8 12h.01"/><rect width="20" height="16" x="2" y="4" rx="2"/>',
} as const;

export type IconName = keyof typeof ICONS;

export function Icon(props: { name: IconName; class?: string; size?: number }): JSX.Element {
  return (
    <svg
      class={`fe-ico ${props.class ?? ''}`}
      viewBox="0 0 24 24"
      width={props.size ?? 16}
      height={props.size ?? 16}
      aria-hidden="true"
      innerHTML={ICONS[props.name]}
    />
  );
}

/** Маленькая стрелка выпадающего списка (8 px, как fe-tool__caret). */
export function Caret(): JSX.Element {
  return (
    <svg class="fe-tool__caret" viewBox="0 0 8 8" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5">
      <path d="M2 3l2 2 2-2" />
    </svg>
  );
}
````

`apps/web/src/ui/store.ts`:

````ts
// Состояние интерфейса — только то, что прислало ядро: события применяются в
// фазе mutate планировщика (одна `batch()` на кадр), живые значения жеста — из
// общей памяти в фазе measure. Здесь ничего не вычисляется о документе.

import { createStore, reconcile } from 'solid-js/store';
import type {
  ConflictInfo,
  CoreEvent,
  KeymapEntry,
  LayerRow,
  PageInfo,
  PresetInfo,
  Summary,
  Tool,
} from '../wasm/ferrum_wasm';
import type { LiveState } from '../core/core';

/** Уведомление: текст и вид (ошибка — с иконкой и словом). */
export interface Toast {
  id: number;
  text: string;
  kind: 'error' | 'info';
}

export interface UiState {
  ready: boolean;
  tool: Tool;
  selectionCount: number;
  canUndo: boolean;
  canRedo: boolean;
  documentName: string;
  layers: { start: number; total: number; rows: LayerRow[]; reveal: number | null; revealSeq: number };
  summary: Summary | null;
  pages: { list: PageInfo[]; current: string; background: string | null };
  keymap: { commands: KeymapEntry[]; conflicts: ConflictInfo[]; reserved: string[]; overrides: string };
  presets: PresetInfo[];
  /** Ядро остановлено (сбой worker'а): причина; интерфейс показывает её поверх холста. */
  stopped: string | null;
  /** Живое состояние, нужное панелям во время жеста. */
  live: { zoom: number; busy: boolean; x: number; y: number; w: number; h: number; rotation: number; count: number };
  uiHidden: boolean;
  toasts: Toast[];
}

const initial: UiState = {
  ready: false,
  tool: 'move',
  selectionCount: 0,
  canUndo: false,
  canRedo: false,
  documentName: '',
  layers: { start: 0, total: 0, rows: [], reveal: null, revealSeq: 0 },
  summary: null,
  pages: { list: [], current: '', background: null },
  keymap: { commands: [], conflicts: [], reserved: [], overrides: '' },
  presets: [],
  stopped: null,
  live: { zoom: 1, busy: false, x: 0, y: 0, w: 0, h: 0, rotation: 0, count: 0 },
  uiHidden: false,
  toasts: [],
};

/** Сколько уведомлений видно одновременно и сколько секунд живёт каждое. */
export const MAX_TOASTS = 3;
export const TOAST_MS = 6000;

export function createUiStore() {
  const [state, setState] = createStore<UiState>(structuredClone(initial));
  let toastId = 0;

  function toast(text: string, kind: Toast['kind'] = 'info'): void {
    // То же сообщение подряд — одно уведомление, а не стопка.
    if (state.toasts.some((t) => t.text === text)) return;
    const id = ++toastId;
    setState('toasts', (list) => [...list, { id, text, kind }].slice(-MAX_TOASTS));
    setTimeout(() => setState('toasts', (list) => list.filter((t) => t.id !== id)), TOAST_MS);
  }

  /** Событие ядра → состояние. Вызывается внутри `batch()` планировщика. */
  function apply(e: CoreEvent): void {
    switch (e.type) {
      case 'opened':
        setState({ ready: true, documentName: e.name });
        for (const w of e.warnings) toast(w, 'error');
        break;
      case 'documentName':
        setState('documentName', e.name);
        break;
      case 'tool':
        setState('tool', e.tool);
        break;
      case 'selection':
        setState('selectionCount', e.count);
        break;
      case 'document':
        setState({ canUndo: e.canUndo, canRedo: e.canRedo });
        break;
      case 'layers':
        // Строки сверяются по id: Solid перерисовывает только изменившиеся.
        setState('layers', 'rows', reconcile(e.rows, { key: 'id' }));
        setState('layers', { start: e.start, total: e.total, reveal: e.reveal ?? null });
        if (e.reveal != null) setState('layers', 'revealSeq', (n) => n + 1);
        break;
      case 'properties':
        setState('summary', reconcile(e.summary));
        break;
      case 'pages':
        setState('pages', { list: e.pages, current: e.current, background: e.background ?? null });
        break;
      case 'keymap':
        setState('keymap', {
          commands: e.commands,
          conflicts: e.conflicts,
          reserved: e.reserved,
          overrides: e.overrides,
        });
        break;
      case 'presets':
        setState('presets', e.presets);
        break;
      case 'toggleUi':
        setState('uiHidden', (h) => !h);
        break;
      case 'error':
        // «Ядро остановлено: …» — одно сообщение навсегда, а не уведомление.
        if (e.message.startsWith('Ядро остановлено:')) setState('stopped', e.message);
        else toast(e.message, 'error');
        break;
      default:
        break;
    }
  }

  /** Живое состояние кадра → поля панели (только изменившиеся). */
  function applyLive(l: LiveState): void {
    setState('live', {
      zoom: l.zoom,
      busy: l.busy === 1,
      x: l.selectionX,
      y: l.selectionY,
      w: l.selectionW,
      h: l.selectionH,
      rotation: l.selectionRotation,
      count: l.selectionCount,
    });
  }

  return { state, setState, apply, applyLive, toast };
}

export type UiStore = ReturnType<typeof createUiStore>;
````

`apps/web/src/ui/context.ts`:

````ts
// Общее для компонентов редактора: ядро, состояние, планировщик, платформа.

import { createContext, useContext } from 'solid-js';
import type { Command } from '../wasm/ferrum_wasm';
import type { Core, Platform } from '../core/core';
import type { Scheduler } from './scheduler';
import type { UiStore } from './store';

export interface Editor {
  core: Core;
  store: UiStore;
  scheduler: Scheduler;
  platform: Platform;
  send(cmd: Command): void;
  /** Команда редактора по ключу таблицы (`edit.undo`). */
  run(command: string): void;
}

export const EditorContext = createContext<Editor>();

export function useEditor(): Editor {
  const e = useContext(EditorContext);
  if (!e) throw new Error('Компонент редактора вне EditorContext');
  return e;
}
````

`apps/web/src/ui/keys.ts`:

````ts
// Какие клавиши оставить интерфейсу, а какие переслать редактору (спека 7.6:
// «если фокус в поле ввода панели, клавиши обрабатывает интерфейс»).

/** Элемент, где клавиши — ввод текста или навигация по виджету. */
function ownsKeys(t: EventTarget | null): boolean {
  if (!(t instanceof Element)) return false;
  if (t.closest('input, textarea, select, [contenteditable="true"]')) return true;
  // Меню, диалоги, списки: стрелки, Enter, Esc, Tab — их клавиатурная навигация.
  return !!t.closest('[role="menu"], [role="dialog"], [role="listbox"]');
}

/** Клавиши кнопок и вкладок: активировать и переходить фокусом. */
const WIDGET_KEYS = new Set(['Tab', 'Enter', 'Space', 'NumpadEnter']);

/** Пересылать ли клавишу редактору. */
export function forwardKey(e: KeyboardEvent, isClipboardKey: (e: KeyboardEvent) => boolean): boolean {
  if (ownsKeys(e.target)) return false;
  // Tab всегда у браузера: фокус ходит по интерфейсу.
  if (e.code === 'Tab') return false;
  const onControl = e.target instanceof Element && !!e.target.closest('button, [role="tab"], [role="treeitem"], a');
  if (onControl && WIDGET_KEYS.has(e.code)) return false;
  // Ctrl/Cmd+C, X, V идут событиями copy/cut/paste (системный буфер), а не в ядро.
  return !isClipboardKey(e);
}
````

`apps/web/src/ui/boot.ts`:

````ts
// Запуск редактора: ядро на холсте, события в состояние через планировщик,
// документ. Параметры адреса: `nodes` — страница-пример из N узлов (по умолчанию
// пустой документ), `backend`, `transport` — как у стенда (тесты).

import { batch } from 'solid-js';
import type { Backend, CoreEvent } from '../wasm/ferrum_wasm';
import { Core, detectPlatform } from '../core/core';
import type { TransportMode } from '../core/transport';
import type { Editor } from './context';
import { forwardKey } from './keys';
import { createScheduler, guardLayout } from './scheduler';
import type { UiStore } from './store';

/** Журнал событий для тестов — последние события, не больше этого числа. */
const EVENT_LOG = 2000;

function choice<T extends string>(params: URLSearchParams, name: string, allowed: readonly T[]): T | undefined {
  const v = params.get(name);
  return allowed.find((a) => a === v);
}

export interface BootOptions {
  /** Клавиша — сочетание буфера обмена, которое ведут события copy/cut/paste. */
  isClipboardKey?: (e: KeyboardEvent) => boolean;
}

export async function boot(canvas: HTMLCanvasElement, store: UiStore, opts: BootOptions = {}): Promise<Editor> {
  const params = new URLSearchParams(location.search);
  const platform = detectPlatform();
  const scheduler = createScheduler({ batch });
  if (import.meta.env.DEV) guardLayout([Element.prototype, HTMLElement.prototype], scheduler);
  const events: CoreEvent[] = [];
  const isClipboardKey = opts.isClipboardKey ?? (() => false);
  const core = await Core.start(canvas, {
    platform,
    backend: choice<Backend>(params, 'backend', ['webgpu', 'webgl', 'cpu']),
    transport: choice<TransportMode>(params, 'transport', ['sab', 'message']),
    keyFilter: (e) => forwardKey(e, isClipboardKey),
    onEvent: (e) => {
      events.push(e);
      if (events.length > EVENT_LOG) events.splice(0, EVENT_LOG / 2);
      if (e.type === 'opened' && window.__ferrum) window.__ferrum.ready = true;
      // Ошибка — сразу, мимо планировщика: при остановке ядра кадров больше нет, и
      // из очереди сообщение не дошло бы до пользователя никогда.
      if (e.type === 'error') store.apply(e);
      else scheduler.enqueue(() => store.apply(e));
    },
  });
  core.onFrame(() => scheduler.frame());
  scheduler.everyFrame(() => {
    const l = core.live;
    if (l) return () => store.applyLive(l);
  });
  window.__ferrum = { core, events, ready: false, ui: store.state };
  const editor: Editor = {
    core,
    store,
    scheduler,
    platform,
    send: (cmd) => core.send(cmd),
    run: (command) => core.send({ type: 'execute', command }),
  };
  core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 0) });
  return editor;
}
````

`apps/web/src/ui/toasts.tsx`:

````tsx
// Уведомления: ошибки ядра и подтверждения действий («Экспортировано: …»).

import { For, Show } from 'solid-js';
import { Icon } from './icons';
import type { UiStore } from './store';

export function Toasts(props: { store: UiStore }) {
  return (
    <div class="fe-toasts" role="status" aria-live="polite">
      <For each={props.store.state.toasts}>
        {(t) => (
          <div class={`fe-toast fe-toast--${t.kind}`}>
            <Show when={t.kind === 'error'}>
              <Icon name="warning" />
              <span class="t-strong">Ошибка:</span>
            </Show>
            <span>{t.text}</span>
          </div>
        )}
      </For>
    </div>
  );
}
````

`apps/web/src/ui/app.tsx`:

````tsx
// Редактор: холст под управлением worker'а и панели вокруг него.

import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { boot } from './boot';
import { EditorContext, type Editor } from './context';
import { createUiStore } from './store';
import { Toasts } from './toasts';

export function App() {
  const store = createUiStore();
  const [editor, setEditor] = createSignal<Editor>();
  const [failure, setFailure] = createSignal('');
  let canvas!: HTMLCanvasElement;

  onMount(() => {
    boot(canvas, store)
      .then((ed) => {
        setEditor(ed);
        canvas.focus();
      })
      // Отказ `Core.start` уже начинается с «Ядро не запустилось: …».
      .catch((e: unknown) => setFailure(e instanceof Error ? e.message : `Ядро не запустилось: ${String(e)}`));
  });
  onCleanup(() => editor()?.core.dispose());

  return (
    <div class="fe-app" classList={{ 'fe-app--bare': store.state.uiHidden }}>
      <main class="fe-stage">
        <canvas ref={canvas} class="fe-canvas" tabindex="0" aria-label="Холст" />
        <Show when={!store.state.ready}>
          <div class="fe-boot" classList={{ 'fe-boot--error': !!failure() }} role="status">
            {failure() || 'Запуск редактора…'}
          </div>
        </Show>
        <Show when={store.state.stopped}>
          {(message) => (
            <div class="fe-boot fe-boot--error fe-boot--stopped" role="alert">
              {message()}. Правки больше не принимаются — перезагрузите страницу
            </div>
          )}
        </Show>
      </main>
      <Show when={editor()}>{(ed) => <EditorContext.Provider value={ed()}>{null}</EditorContext.Provider>}</Show>
      <Toasts store={store} />
    </div>
  );
}
````

`apps/web/index.html` (целиком):

````html
<!doctype html>
<html lang="ru" data-theme="dark">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Ferrum</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
````

`apps/web/src/main.tsx`:

````tsx
// Редактор Ferrum: интерфейс UI3 на SolidJS поверх ядра в worker'е (стенд ядра
// без панелей — debug.html).

import '@fontsource-variable/onest';
import '@fontsource/jetbrains-mono/400.css';
import '@fontsource/jetbrains-mono/500.css';
import './ui/styles/theme.css';
import './ui/styles/base.css';
import './ui/styles/app.css';
import { render } from 'solid-js/web';
import { App } from './ui/app';

render(() => <App />, document.getElementById('app')!);
````

`apps/web/e2e/app.spec.ts`:

````ts
// Редактор (index.html): запуск ядра под интерфейсом на SolidJS, события ядра в
// состоянии интерфейса, шрифты и токены дизайн-системы.

import { expect, test } from '@playwright/test';
import { liveWhen, openApp } from './helpers';

test('редактор запускает ядро и получает раскладку, пресеты и документ', async ({ page }) => {
  await openApp(page);
  const info = await page.evaluate(() => {
    const f = window.__ferrum!;
    return {
      mode: f.core.mode,
      commands: f.ui!.keymap.commands.length,
      presets: f.ui!.presets.length,
      pages: f.ui!.pages.list.length,
      total: f.ui!.layers.total,
      name: f.ui!.documentName,
      errors: f.events.filter((e) => e.type === 'error'),
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 43, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

test('шрифты и цвета — из дизайн-системы', async ({ page }) => {
  await openApp(page);
  const style = await page.evaluate(async () => {
    // Шрифты вшиты в бандл (fontsource): грузятся с того же сервера, без сети.
    const onest = await document.fonts.load('12px "Onest Variable"', 'Слой');
    const mono = await document.fonts.load('11px "JetBrains Mono"', '1440');
    const body = getComputedStyle(document.body);
    return {
      onest: onest.length > 0,
      mono: mono.length > 0,
      bg: body.backgroundColor,
      font: body.fontFamily,
    };
  });
  expect(style.onest && style.mono).toBe(true);
  expect(style.bg).toBe('rgb(26, 25, 24)');
  expect(style.font).toContain('Onest Variable');
});

test('клавиши с холста доходят до редактора', async ({ page }) => {
  await openApp(page);
  await page.locator('canvas').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('KeyR');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('rect');
  await page.keyboard.press('Escape');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('move');
  await liveWhen(page, 'l.frames > 0');
});

test('сбой ядра после запуска виден пользователю один раз', async ({ page }) => {
  await openApp(page);
  // Сбой worker'а после `ready` — тот же путь, что `fatal` от worker'а.
  await page.evaluate(() => (window.__ferrum!.core as unknown as { fail(m: string): void }).fail('проверка'));
  await expect(page.getByRole('alert')).toHaveText('Ядро остановлено: проверка. Правки больше не принимаются — перезагрузите страницу');
  // Команды после остановки — без новых окон и без вечной очереди.
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'execute', command: 'edit.undo' }));
  await expect(page.getByRole('alert')).toHaveCount(1);
});
````

Run: `cd apps/web && npm run typecheck`
Expected: без ошибок.

Run: `cd apps/web && npm run wasm && npm run build && npx playwright test e2e/app.spec.ts e2e/core.spec.ts --project=chromium`
Expected: PASS, `21 passed` (`app.spec.ts` — 4; `core.spec.ts` на `debug.html` — как в этапе 4).

- [ ] **Step 7: Закоммитить**

```bash
git add apps/web
git commit -m "feat(web): каркас интерфейса на SolidJS — планировщик, токены, стенд в debug.html"
```

---

### Task 7: Раскладка UI3: тулбар, шапки, меню, подсказки

Раскладка Figma UI3 из компонентов дизайн-системы: слева панель с шапкой файла (знак Ferrum, имя файла — двойной щелчок переименовывает, проект «Черновики», стрелка меню файла), справа — шапка с зумом (`num`, выпадающий список масштабов) и «Поделиться» (основная кнопка, отключена до подсистемы 4), вкладки «Дизайн» и «Прототип» (вторая отключена до подсистемы 6). Панели отделены от холста линией `line`; ширина меняется перетаскиванием края (200–480 px) или стрелками с клавиатуры. Холст — между панелями: `ResizeObserver` ядра пересылает новый размер.

**Тулбар** — плавающий внизу по центру холста: «Перемещение», «Фрейм» со стрелкой пресетов (по разделам, с размерами), «Фигуры» (последняя выбранная на кнопке; прямоугольник, эллипс, линия, изображение), разделитель, отключённые «Перо», «Текст», «Комментарии» — подсказка говорит, когда они появятся. Активный инструмент — `aria-pressed` и фон `oxide-soft`. Отключённые кнопки — `aria-disabled`, а не `disabled`: такие получают наведение и фокус, и подсказка видна.

**Подсказки** — один слой на интерфейс, делегирование событий по `data-tip` и `data-command`: «Название  Клавиша» (два пробела — часть текста), клавиша — первое действующее сочетание команды из раскладки, поэтому после переназначения подсказка сразу другая. Наведение — через 500 мс, фокус с клавиатуры — сразу; положение — в фазе measure. Подписи сочетаний (`shortcuts.ts`) — те же, что у ядра (`⇧⌘K`, `Ctrl+Shift+K`; E2E сверяет все команды); по раскладке пользователя — через `navigator.keyboard.getLayoutMap()`, где он есть.

**Меню** (`role="menu"`): стрелки, Home/End, Enter/Space, Esc возвращает фокус туда, откуда меню открыли, Tab закрывает; появляется невидимым, встаёт у якоря в пределах окна в фазе measure. **Контекстное меню холста** — команды редактора по выделению с подписями сочетаний. **Ctrl+\\** (команда `view.toggle_ui`) скрывает панели и тулбар — холст на всё окно.

**Files:**
- Create: `apps/web/src/ui/shortcuts.ts`
- Test: `apps/web/unit/shortcuts.test.ts`
- Create: `apps/web/src/ui/tooltip.tsx`
- Create: `apps/web/src/ui/menu.tsx`
- Create: `apps/web/src/ui/toolbar.tsx`
- Create: `apps/web/src/ui/chrome.tsx`
- Create: `apps/web/src/ui/styles/chrome.css`
- Modify: `apps/web/src/ui/styles/base.css`
- Modify: `apps/web/src/ui/app.tsx`
- Modify: `apps/web/src/main.tsx`
- Modify: `apps/web/e2e/helpers.ts`
- Test: `apps/web/e2e/chrome.spec.ts`

**Interfaces:**
- Consumes: `useEditor`, `store.state.{keymap, presets, tool, live, selectionCount, uiHidden}` (Task 6), команды `setTool`, `addFramePreset`, `setZoom`, `setDocumentName`, `execute` (Task 5).
- Produces: `parseShortcut`, `formatShortcut`, `keyLabel`, `shortcutLabel(s, platform, layout?)`, `tipText`, `eventShortcut`, `clipboardAction`, `loadLayoutMap`; `TooltipLayer`, `TOOLTIP_DELAY_MS`; `Menu`, `MenuItem`; `Toolbar`; `FileHeader`, `DesignHeader`, `ResizeEdge`, `clampPanel`, `PANEL_MIN`, `PANEL_MAX`, `canvasMenu(selectionCount)`, `Section`; `appScreen(page, live, x, y)` в `e2e/helpers.ts`.

- [ ] **Step 1: Написать падающий тест подписей**

`apps/web/unit/shortcuts.test.ts`:

````ts
// Сочетания в интерфейсе: та же каноническая запись и те же подписи, что у ядра
// (`Shortcut::to_string`, `Shortcut::label`), сочетание из события — по
// физической клавише, сочетания буфера обмена.

import { expect, test } from '@playwright/test';
import {
  clipboardAction,
  eventShortcut,
  formatShortcut,
  keyLabel,
  parseShortcut,
  shortcutLabel,
  tipText,
} from '../src/ui/shortcuts';

test('разбор и каноническая запись', () => {
  expect(parseShortcut('Shift+Ctrl+KeyZ')).toEqual({ code: 'KeyZ', ctrl: true, alt: false, shift: true, meta: false });
  expect(formatShortcut(parseShortcut('Meta+Shift+Alt+Ctrl+KeyK')!)).toBe('Ctrl+Alt+Shift+Meta+KeyK');
  expect(parseShortcut('Cmd+KeyC')?.meta).toBe(true);
  expect(parseShortcut('Ctrl+Ctrl+KeyC')).toBeNull();
  expect(parseShortcut('Hyper+KeyC')).toBeNull();
  expect(parseShortcut('')).toBeNull();
});

test('подписи как у ядра: macOS и Windows/Linux', () => {
  // Пары из `Shortcut::label` (crates/ferrum-editor/src/keymap.rs).
  expect(shortcutLabel('Shift+Meta+KeyK', 'mac')).toBe('⇧⌘K');
  expect(shortcutLabel('Ctrl+Shift+KeyK', 'windows')).toBe('Ctrl+Shift+K');
  expect(shortcutLabel('Alt+Meta+BracketRight', 'mac')).toBe('⌥⌘]');
  expect(shortcutLabel('Ctrl+Backslash', 'linux')).toBe('Ctrl+\\');
  expect(shortcutLabel('Shift+Digit1', 'windows')).toBe('Shift+1');
  expect(shortcutLabel('Escape', 'mac')).toBe('Esc');
  expect(shortcutLabel('Shift+ArrowLeft', 'linux')).toBe('Shift+←');
  expect(shortcutLabel('Meta+KeyZ', 'windows')).toBe('Win+Z');
  expect(shortcutLabel('Delete', 'windows')).toBe('Delete');
});

test('подпись по раскладке пользователя', () => {
  const ru = new Map([
    ['KeyR', 'к'],
    ['BracketLeft', 'х'],
  ]);
  expect(keyLabel('KeyR', ru)).toBe('К');
  expect(shortcutLabel('Ctrl+BracketLeft', 'windows', ru)).toBe('Ctrl+Х');
  expect(keyLabel('ArrowUp', ru)).toBe('↑');
  // Нет в раскладке — подпись US.
  expect(keyLabel('KeyQ', ru)).toBe('Q');
});

test('подсказка «Название  Клавиша»', () => {
  expect(tipText('Прямоугольник', 'R')).toBe('Прямоугольник  R');
  expect(tipText('Поделиться')).toBe('Поделиться');
});

test('сочетание из события — по физической клавише', () => {
  const ev = (code: string, mods: Partial<Record<'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey', boolean>> = {}) => ({
    code,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...mods,
  });
  // Русская раскладка: «к» — та же клавиша KeyR.
  expect(eventShortcut(ev('KeyR'))).toBe('KeyR');
  expect(eventShortcut(ev('KeyZ', { metaKey: true, shiftKey: true }))).toBe('Shift+Meta+KeyZ');
  expect(eventShortcut(ev('ShiftLeft', { shiftKey: true }))).toBeNull();
  expect(eventShortcut(ev(''))).toBeNull();
});

test('сочетания буфера обмена по платформе', () => {
  expect(clipboardAction('Meta+KeyC', 'mac')).toBe('copy');
  expect(clipboardAction('Ctrl+KeyC', 'mac')).toBeNull();
  expect(clipboardAction('Ctrl+KeyX', 'windows')).toBe('cut');
  expect(clipboardAction('Ctrl+KeyV', 'linux')).toBe('paste');
  expect(clipboardAction('Ctrl+Shift+KeyC', 'linux')).toBeNull();
});
````

Run: `cd apps/web && npm run unit`
Expected: FAIL — `Cannot find module '../src/ui/shortcuts'`.

- [ ] **Step 2: Подписи и сочетания**

`apps/web/src/ui/shortcuts.ts`:

````ts
// Сочетания клавиш в интерфейсе: разбор и каноническая запись (как в Rust:
// «Ctrl+Alt+Shift+Meta+KeyZ»), подписи для подсказок и настроек, сочетание из
// события клавиатуры (по физической клавише `code` — работает на русской
// раскладке), сочетания буфера обмена. Подписи — для текущей раскладки через
// `navigator.keyboard.getLayoutMap()`, где доступно; без неё — US, как в Figma.

import type { Platform } from '../core/core';

export interface Shortcut {
  code: string;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
}

/** Раскладка: физическая клавиша → символ (`KeyboardLayoutMap`). */
export type LayoutMap = ReadonlyMap<string, string>;

const MODS = ['Ctrl', 'Alt', 'Shift', 'Meta'] as const;

/** Клавиши-модификаторы сами по себе сочетанием не бывают. */
const MODIFIER_CODES = new Set([
  'ControlLeft',
  'ControlRight',
  'AltLeft',
  'AltRight',
  'ShiftLeft',
  'ShiftRight',
  'MetaLeft',
  'MetaRight',
  'OSLeft',
  'OSRight',
  'CapsLock',
  'Fn',
]);

export function parseShortcut(s: string): Shortcut | null {
  const parts = s.split('+');
  const code = parts.pop();
  if (!code) return null;
  const sc: Shortcut = { code, ctrl: false, alt: false, shift: false, meta: false };
  for (const p of parts) {
    const key = p === 'Cmd' ? 'meta' : (p.toLowerCase() as keyof Shortcut);
    if (!(MODS as readonly string[]).includes(p) && p !== 'Cmd') return null;
    if (sc[key]) return null;
    (sc as unknown as Record<string, boolean>)[key] = true;
  }
  return sc;
}

/** Каноническая запись: модификаторы в порядке Ctrl, Alt, Shift, Meta. */
export function formatShortcut(sc: Shortcut): string {
  const mods = [sc.ctrl && 'Ctrl', sc.alt && 'Alt', sc.shift && 'Shift', sc.meta && 'Meta'].filter(Boolean);
  return [...mods, sc.code].join('+');
}

const NAMED: Record<string, string> = {
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Minus: '-',
  Equal: '=',
  ArrowLeft: '←',
  ArrowRight: '→',
  ArrowUp: '↑',
  ArrowDown: '↓',
  Escape: 'Esc',
};

/** Подпись клавиши: буква, цифра или знак — по раскладке, остальное — по таблице. */
export function keyLabel(code: string, layout?: LayoutMap): string {
  const printable = /^(Key[A-Z]|Digit\d|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Comma|Period|Slash|Backquote)$/;
  const fromLayout = printable.test(code) ? layout?.get(code) : undefined;
  if (fromLayout && fromLayout.trim()) return fromLayout.toUpperCase();
  const m = /^(?:Key|Digit)(.)$/.exec(code);
  if (m) return m[1];
  return NAMED[code] ?? code;
}

/** Подпись сочетания: «⇧⌘K» на macOS, «Ctrl+Shift+K» на остальных (как
 * `Shortcut::label` в Rust). */
export function shortcutLabel(s: string, platform: Platform, layout?: LayoutMap): string {
  const sc = parseShortcut(s);
  if (!sc) return s;
  const key = keyLabel(sc.code, layout);
  if (platform === 'mac') {
    return `${sc.ctrl ? '⌃' : ''}${sc.alt ? '⌥' : ''}${sc.shift ? '⇧' : ''}${sc.meta ? '⌘' : ''}${key}`;
  }
  const mods = [sc.ctrl && 'Ctrl', sc.alt && 'Alt', sc.shift && 'Shift', sc.meta && 'Win'].filter(Boolean);
  return [...mods, key].join('+');
}

/** Текст подсказки: «Название  Клавиша» (два пробела, как в дизайн-системе) или
 * только название, если сочетаний нет. */
export function tipText(name: string, label?: string): string {
  return label ? `${name}  ${label}` : name;
}

/** Сочетание из события клавиатуры (по `code`); `null` — нажат только модификатор. */
export function eventShortcut(e: Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>): string | null {
  if (!e.code || MODIFIER_CODES.has(e.code)) return null;
  return formatShortcut({ code: e.code, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey });
}

/** Сочетание, которое браузер превращает в событие буфера обмена. */
export function clipboardAction(s: string, platform: Platform): 'copy' | 'cut' | 'paste' | null {
  const mod = platform === 'mac' ? 'Meta' : 'Ctrl';
  switch (s) {
    case `${mod}+KeyC`:
      return 'copy';
    case `${mod}+KeyX`:
      return 'cut';
    case `${mod}+KeyV`:
      return 'paste';
    default:
      return null;
  }
}

/** Раскладка клавиатуры пользователя, если браузер её отдаёт (Chromium). */
export async function loadLayoutMap(): Promise<LayoutMap | undefined> {
  const kb = (navigator as Navigator & { keyboard?: { getLayoutMap?: () => Promise<LayoutMap> } }).keyboard;
  try {
    return (await kb?.getLayoutMap?.()) ?? undefined;
  } catch {
    return undefined;
  }
}
````

Run: `cd apps/web && npm run unit`
Expected: PASS, `11 passed`.

- [ ] **Step 3: Тулбар, шапки, меню, подсказки**

`apps/web/src/ui/tooltip.tsx`:

````tsx
// Всплывающие подсказки: у каждой кнопки-иконки — «Название  Клавиша», клавиша
// из действующей раскладки (переназначили — подсказка сразу другая). Один слой на
// весь интерфейс, делегирование событий: элемент с `data-tip` (и `data-command` —
// ключ команды, чьё сочетание показать). Положение считается в фазе measure.

import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { useEditor } from './context';
import { shortcutLabel, tipText, type LayoutMap } from './shortcuts';

/** Задержка перед показом, как у системных подсказок. */
export const TOOLTIP_DELAY_MS = 500;

interface Tip {
  text: string;
  key: string;
  x: number;
  y: number;
  above: boolean;
}

export function TooltipLayer(props: { layout?: LayoutMap }) {
  const ed = useEditor();
  const [tip, setTip] = createSignal<Tip | null>(null);
  let timer = 0;
  let target: HTMLElement | null = null;

  const keyOf = (command: string | undefined): string => {
    if (!command) return '';
    const c = ed.store.state.keymap.commands.find((x) => x.key === command);
    const first = c?.bindings[0];
    return first ? shortcutLabel(first, ed.platform, props.layout) : '';
  };

  const hide = () => {
    clearTimeout(timer);
    target = null;
    setTip(null);
  };

  const show = (el: HTMLElement, delay: number) => {
    clearTimeout(timer);
    target = el;
    timer = window.setTimeout(() => {
      ed.scheduler.measure(() => {
        if (target !== el || !el.isConnected) return;
        const r = el.getBoundingClientRect();
        const above = el.dataset.tipSide === 'top' || r.bottom + 40 > innerHeight;
        const x = Math.min(Math.max(r.left + r.width / 2, 80), innerWidth - 80);
        const text = el.dataset.tip ?? '';
        const key = keyOf(el.dataset.command);
        ed.scheduler.mutate(() => setTip({ text, key, x, y: above ? r.top - 6 : r.bottom + 6, above }));
      });
    }, delay);
  };

  const tipped = (t: EventTarget | null) => (t instanceof Element ? (t.closest('[data-tip]') as HTMLElement | null) : null);

  onMount(() => {
    const over = (e: PointerEvent) => {
      const el = tipped(e.target);
      if (el && el !== target) show(el, TOOLTIP_DELAY_MS);
      if (!el && target) hide();
    };
    const focus = (e: FocusEvent) => {
      const el = tipped(e.target);
      // Фокус с клавиатуры — подсказка сразу; фокус мышью — без неё.
      if (el && el.matches(':focus-visible')) show(el, 0);
    };
    document.addEventListener('pointerover', over);
    document.addEventListener('pointerdown', hide, true);
    document.addEventListener('focusin', focus);
    document.addEventListener('focusout', hide);
    document.addEventListener('keydown', hide, true);
    onCleanup(() => {
      document.removeEventListener('pointerover', over);
      document.removeEventListener('pointerdown', hide, true);
      document.removeEventListener('focusin', focus);
      document.removeEventListener('focusout', hide);
      document.removeEventListener('keydown', hide, true);
      clearTimeout(timer);
    });
  });

  return (
    <Show when={tip()}>
      {(t) => (
        <div
          class="fe-tooltip"
          role="tooltip"
          style={{
            left: `${t().x}px`,
            top: `${t().y}px`,
            transform: `translate(-50%, ${t().above ? '-100%' : '0'})`,
          }}
        >
          {/* «Название  Клавиша»: два пробела — часть текста (white-space: pre). */}
          <span>{t().text}</span>
          <Show when={t().key}>
            <span class="fe-tooltip__key">{tipText('', t().key)}</span>
          </Show>
        </div>
      )}
    </Show>
  );
}
````

`apps/web/src/ui/menu.tsx`:

````tsx
// Меню: выпадающие списки (файл, фигуры, пресеты, зум) и контекстное меню холста.
// role="menu" и клавиатура: стрелки, Home/End, Enter/Space, Esc (фокус
// возвращается туда, откуда меню открыли), Tab закрывает. Подпись сочетания у
// пункта — из действующей раскладки. Положение считается в фазе measure: меню
// сначала появляется невидимым, затем встаёт у якоря в пределах окна.

import { For, onCleanup, onMount, Show, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';
import { useEditor } from './context';
import { Icon } from './icons';
import { shortcutLabel } from './shortcuts';

export type MenuItem =
  | {
      label: string;
      /** Ключ команды редактора: выполнить её и показать её сочетание. */
      command?: string;
      /** Своя подпись справа (размер пресета). */
      hint?: string;
      disabled?: boolean;
      checked?: boolean;
      onSelect?: () => void;
    }
  | { separator: true }
  | { heading: string };

export interface MenuProps {
  items: MenuItem[];
  /** Элемент, под которым открыть, или точка (контекстное меню). */
  anchor: HTMLElement | { x: number; y: number };
  label: string;
  /** Открыть над якорем (тулбар внизу экрана). */
  above?: boolean;
  onClose: () => void;
}

export function Menu(props: MenuProps): JSX.Element {
  const ed = useEditor();
  let el!: HTMLDivElement;
  const returnFocus = document.activeElement as HTMLElement | null;

  const items = () => Array.from(el.querySelectorAll<HTMLElement>('[role="menuitem"]:not([aria-disabled="true"]), [role="menuitemcheckbox"]:not([aria-disabled="true"])'));
  const close = (restore: boolean) => {
    props.onClose();
    if (restore) returnFocus?.focus?.();
  };

  const select = (item: Extract<MenuItem, { label: string }>) => {
    if (item.disabled) return;
    close(true);
    if (item.onSelect) item.onSelect();
    else if (item.command) ed.run(item.command);
  };

  const onKey = (e: KeyboardEvent) => {
    const list = items();
    const at = list.indexOf(document.activeElement as HTMLElement);
    const go = (i: number) => list[(i + list.length) % list.length]?.focus();
    switch (e.key) {
      case 'ArrowDown':
        go(at + 1);
        break;
      case 'ArrowUp':
        go(at - 1);
        break;
      case 'Home':
        go(0);
        break;
      case 'End':
        go(list.length - 1);
        break;
      case 'Escape':
        close(true);
        break;
      case 'Tab':
        close(false);
        return;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  };

  onMount(() => {
    ed.scheduler.measure(() => {
      const box = el.getBoundingClientRect();
      const a = props.anchor;
      const r = a instanceof HTMLElement ? a.getBoundingClientRect() : { left: a.x, right: a.x, top: a.y, bottom: a.y };
      let x = r.left;
      let y = props.above ? r.top - box.height - 4 : r.bottom + 4;
      if (x + box.width > innerWidth - 8) x = Math.max(8, innerWidth - 8 - box.width);
      if (y + box.height > innerHeight - 8) y = Math.max(8, r.top - box.height - 4);
      if (y < 8) y = 8;
      ed.scheduler.mutate(() => {
        el.style.left = `${x}px`;
        el.style.top = `${y}px`;
        el.style.visibility = 'visible';
        items()[0]?.focus();
      });
    });
    const outside = (e: PointerEvent) => {
      if (!el.contains(e.target as Node)) close(false);
    };
    // Нажатие, которым меню открыли, ещё всплывает: слушаем со следующей задачи.
    const t = setTimeout(() => document.addEventListener('pointerdown', outside, true));
    window.addEventListener('blur', props.onClose);
    onCleanup(() => {
      clearTimeout(t);
      document.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('blur', props.onClose);
    });
  });

  const key = (command?: string) => {
    if (!command) return '';
    const c = ed.store.state.keymap.commands.find((x) => x.key === command);
    return c?.bindings[0] ? shortcutLabel(c.bindings[0], ed.platform) : '';
  };

  return (
    <Portal>
      <div ref={el} class="fe-menu" role="menu" aria-label={props.label} style={{ visibility: 'hidden' }} onKeyDown={onKey}>
        <For each={props.items}>
          {(item) => {
            if ('separator' in item) return <div class="fe-menu__sep" role="separator" />;
            if ('heading' in item) return <div class="fe-menu__heading">{item.heading}</div>;
            return (
              <div
                class="fe-menu__item"
                role={item.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
                aria-checked={item.checked === undefined ? undefined : item.checked}
                aria-disabled={item.disabled ? 'true' : undefined}
                tabindex="-1"
                onClick={() => select(item)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    select(item);
                  }
                }}
              >
                <span class="fe-menu__check">
                  <Show when={item.checked}>
                    <Icon name="check" />
                  </Show>
                </span>
                <span class="fe-menu__label">{item.label}</span>
                <span class="fe-menu__key">{item.hint ?? key(item.command)}</span>
              </div>
            );
          }}
        </For>
      </div>
    </Portal>
  );
}
````

`apps/web/src/ui/toolbar.tsx`:

````tsx
// Плавающий тулбар внизу по центру холста (раскладка UI3): перемещение, фрейм с
// пресетами, фигуры (последняя выбранная на кнопке), отключённые перо, текст и
// комментарии — с подсказкой, когда они появятся. Рука (H, удержание Space)
// работает с клавиатуры и в тулбар не выносится.

import { createEffect, createMemo, createSignal, For, Show } from 'solid-js';
import type { Tool } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { Caret, Icon, type IconName } from './icons';
import { Menu, type MenuItem } from './menu';

type Shape = 'rect' | 'ellipse' | 'line' | 'image';

const SHAPES: { tool: Shape; name: string; command: string; icon: IconName }[] = [
  { tool: 'rect', name: 'Прямоугольник', command: 'tool.rect', icon: 'rect' },
  { tool: 'ellipse', name: 'Эллипс', command: 'tool.ellipse', icon: 'ellipse' },
  { tool: 'line', name: 'Линия', command: 'tool.line', icon: 'line' },
  { tool: 'image', name: 'Изображение', command: 'tool.image', icon: 'image' },
];

const LATER: { name: string; key: string; when: string; icon: IconName }[] = [
  { name: 'Перо', key: 'P', when: 'подсистема 3', icon: 'pen' },
  { name: 'Текст', key: 'T', when: 'подсистема 3', icon: 'text' },
  { name: 'Комментарии', key: 'C', when: 'подсистема 4', icon: 'comment' },
];

export function Toolbar() {
  const ed = useEditor();
  const [shape, setShape] = createSignal<Shape>('rect');
  const [menu, setMenu] = createSignal<{ kind: 'frame' | 'shapes'; anchor: HTMLElement } | null>(null);
  const tool = () => ed.store.state.tool;
  const current = createMemo(() => SHAPES.find((s) => s.tool === shape())!);
  // Фигура, выбранная клавишей, становится и фигурой кнопки.
  createEffect(() => {
    const t = tool() as Tool;
    if (SHAPES.some((s) => s.tool === t)) setShape(t as Shape);
  });

  const setTool = (t: Tool) => ed.send({ type: 'setTool', tool: t });

  const frameItems = (): MenuItem[] => {
    const out: MenuItem[] = [{ label: 'Фрейм', command: 'tool.frame' }];
    let group = '';
    ed.store.state.presets.forEach((p, index) => {
      if (p.group !== group) {
        group = p.group;
        out.push({ separator: true }, { heading: group });
      }
      out.push({
        label: p.name,
        hint: `${p.width} × ${p.height}`,
        onSelect: () => ed.send({ type: 'addFramePreset', index }),
      });
    });
    return out;
  };

  const shapeItems = (): MenuItem[] =>
    SHAPES.map((s) => ({
      label: s.name,
      command: s.command,
      checked: s.tool === shape(),
      onSelect: () => {
        setShape(s.tool);
        setTool(s.tool);
      },
    }));

  return (
    <div class="fe-toolbar" role="toolbar" aria-label="Инструменты">
      <button
        class="fe-tool"
        aria-pressed={tool() === 'move'}
        aria-label="Перемещение"
        data-tip="Перемещение"
        data-command="tool.move"
        data-tip-side="top"
        onClick={() => setTool('move')}
      >
        <Icon name="move" />
      </button>
      <div class="fe-tool-split">
        <button
          class="fe-tool"
          aria-pressed={tool() === 'frame'}
          aria-label="Фрейм"
          data-tip="Фрейм"
          data-command="tool.frame"
          data-tip-side="top"
          onClick={() => setTool('frame')}
        >
          <Icon name="frame" />
        </button>
        <button
          class="fe-tool fe-tool--caret"
          aria-haspopup="menu"
          aria-expanded={menu()?.kind === 'frame'}
          aria-label="Пресеты фреймов"
          data-tip="Пресеты фреймов"
          data-tip-side="top"
          onClick={(e) => setMenu({ kind: 'frame', anchor: e.currentTarget })}
        >
          <Caret />
        </button>
      </div>
      <div class="fe-tool-split">
        <button
          class="fe-tool"
          aria-pressed={tool() === current().tool}
          aria-label={current().name}
          data-tip={current().name}
          data-command={current().command}
          data-tip-side="top"
          onClick={() => setTool(current().tool)}
        >
          <Icon name={current().icon} />
        </button>
        <button
          class="fe-tool fe-tool--caret"
          aria-haspopup="menu"
          aria-expanded={menu()?.kind === 'shapes'}
          aria-label="Фигуры"
          data-tip="Фигуры"
          data-tip-side="top"
          onClick={(e) => setMenu({ kind: 'shapes', anchor: e.currentTarget })}
        >
          <Caret />
        </button>
      </div>
      <span class="fe-toolbar__sep" role="separator" />
      <For each={LATER}>
        {(t) => (
          <button
            class="fe-tool"
            aria-disabled="true"
            aria-label={`${t.name} — ${t.when}`}
            data-tip={`${t.name}  ${t.key} — ${t.when}`}
            data-tip-side="top"
          >
            <Icon name={t.icon} />
          </button>
        )}
      </For>
      <Show when={menu()}>
        {(m) => (
          <Menu
            label={m().kind === 'frame' ? 'Пресеты фреймов' : 'Фигуры'}
            items={m().kind === 'frame' ? frameItems() : shapeItems()}
            anchor={m().anchor}
            above
            onClose={() => setMenu(null)}
          />
        )}
      </Show>
    </div>
  );
}
````

`apps/web/src/ui/chrome.tsx`:

````tsx
// Шапки панелей (PanelHeader), меню файла, зум, «Поделиться», вкладки, края
// панелей для изменения ширины и контекстное меню холста.

import { createSignal, Show, type JSX } from 'solid-js';
import { useEditor } from './context';
import { Caret, Icon } from './icons';
import { Menu, type MenuItem } from './menu';

/** Пределы ширины боковых панелей. */
export const PANEL_MIN = 200;
export const PANEL_MAX = 480;

export function clampPanel(w: number): number {
  return Math.round(Math.min(PANEL_MAX, Math.max(PANEL_MIN, w)));
}

/** Пункты меню файла: другие части интерфейса добавляют свои (открыть, сохранить,
 * настройки). */
export function FileHeader(props: { items: () => MenuItem[] }) {
  const ed = useEditor();
  const [menu, setMenu] = createSignal<HTMLElement | null>(null);
  const [editing, setEditing] = createSignal(false);
  const commit = (value: string) => {
    // Esc уже закрыл поле: blur удаляемого поля не должен применять набранное.
    if (!editing()) return;
    setEditing(false);
    if (value.trim() && value !== ed.store.state.documentName) ed.send({ type: 'setDocumentName', name: value });
  };
  return (
    <div class="fe-panel-head">
      <span class="fe-panel-head__mark" aria-hidden="true" />
      <div class="fe-panel-head__file">
        <Show
          when={editing()}
          fallback={
            <span
              class="fe-panel-head__name"
              title={ed.store.state.documentName}
              onDblClick={() => setEditing(true)}
            >
              {ed.store.state.documentName}
            </span>
          }
        >
          <input
            class="fe-rename"
            aria-label="Имя файла"
            value={ed.store.state.documentName}
            ref={(el) => queueMicrotask(() => el.select())}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commit(e.currentTarget.value);
              if (e.key === 'Escape') setEditing(false);
            }}
            onBlur={(e) => commit(e.currentTarget.value)}
          />
        </Show>
        <span class="fe-panel-head__project">Черновики</span>
      </div>
      <button
        class="fe-tool fe-tool--small"
        aria-haspopup="menu"
        aria-expanded={!!menu()}
        aria-label="Меню файла"
        data-tip="Меню файла"
        onClick={(e) => setMenu(e.currentTarget)}
      >
        <Caret />
      </button>
      <Show when={menu()}>
        {(a) => <Menu label="Меню файла" items={props.items()} anchor={a()} onClose={() => setMenu(null)} />}
      </Show>
    </div>
  );
}

const ZOOMS = [0.5, 1, 2];

export function DesignHeader() {
  const ed = useEditor();
  const [menu, setMenu] = createSignal<HTMLElement | null>(null);
  const zoomItems = (): MenuItem[] => [
    { label: 'Приблизить', command: 'view.zoom_in' },
    { label: 'Отдалить', command: 'view.zoom_out' },
    { separator: true },
    { label: 'Показать всё', command: 'view.fit_all' },
    { label: 'Показать выделение', command: 'view.fit_selection', disabled: ed.store.state.selectionCount === 0 },
    { separator: true },
    ...ZOOMS.map((z) => ({
      label: `Масштаб ${z * 100}%`,
      command: z === 1 ? 'view.zoom_100' : undefined,
      checked: Math.abs(ed.store.state.live.zoom - z) < 1e-6,
      onSelect: () => ed.send({ type: 'setZoom', zoom: z }),
    })),
  ];
  return (
    <>
      <div class="fe-panel-head">
        <span class="fe-panel-head__spacer" />
        <button
          class="fe-zoom"
          aria-haspopup="menu"
          aria-expanded={!!menu()}
          aria-label="Масштаб"
          data-tip="Масштаб"
          onClick={(e) => setMenu(e.currentTarget)}
        >
          <span class="t-num">{Math.round(ed.store.state.live.zoom * 100)}%</span>
          <Caret />
        </button>
        <button
          class="fe-btn fe-btn--primary"
          aria-disabled="true"
          data-tip="Совместная работа появится в подсистеме 4"
        >
          Поделиться
        </button>
        <Show when={menu()}>
          {(a) => <Menu label="Масштаб" items={zoomItems()} anchor={a()} onClose={() => setMenu(null)} />}
        </Show>
      </div>
      <div class="fe-tabs" role="tablist" aria-label="Режим панели">
        <button class="fe-tab" role="tab" aria-selected="true" id="tab-design" aria-controls="panel-design">
          Дизайн
        </button>
        <button class="fe-tab" role="tab" aria-selected="false" aria-disabled="true" data-tip="Прототипы появятся в подсистеме 6">
          Прототип
        </button>
      </div>
    </>
  );
}

/** Край панели: перетаскивание меняет ширину, стрелки с клавиатуры — на 8 px. */
export function ResizeEdge(props: { side: 'left' | 'right'; width: number; onWidth: (w: number) => void }) {
  const ed = useEditor();
  let start = 0;
  let base = 0;
  const move = ed.core.costed((e: PointerEvent) => {
    const dx = e.clientX - start;
    const w = clampPanel(props.side === 'left' ? base + dx : base - dx);
    ed.scheduler.mutate(() => props.onWidth(w));
  });
  return (
    <div
      class={`fe-resize fe-resize--${props.side}`}
      role="separator"
      aria-orientation="vertical"
      aria-label={props.side === 'left' ? 'Ширина левой панели' : 'Ширина правой панели'}
      aria-valuemin={PANEL_MIN}
      aria-valuemax={PANEL_MAX}
      aria-valuenow={props.width}
      tabindex="0"
      onPointerDown={(e) => {
        start = e.clientX;
        base = props.width;
        e.currentTarget.setPointerCapture(e.pointerId);
        e.currentTarget.addEventListener('pointermove', move);
      }}
      onPointerUp={(e) => e.currentTarget.removeEventListener('pointermove', move)}
      onPointerCancel={(e) => e.currentTarget.removeEventListener('pointermove', move)}
      onKeyDown={(e) => {
        const grow = props.side === 'left' ? 'ArrowRight' : 'ArrowLeft';
        const shrink = props.side === 'left' ? 'ArrowLeft' : 'ArrowRight';
        if (e.key === grow) props.onWidth(clampPanel(props.width + 8));
        else if (e.key === shrink) props.onWidth(clampPanel(props.width - 8));
        else return;
        e.preventDefault();
      }}
    />
  );
}

/** Контекстное меню холста: команды редактора по выделению. */
export function canvasMenu(selection: number): MenuItem[] {
  const none = selection === 0;
  const item = (label: string, command: string, needs = true): MenuItem => ({ label, command, disabled: needs && none });
  return [
    item('Копировать', 'edit.copy'),
    item('Вырезать', 'edit.cut'),
    item('Вставить', 'edit.paste', false),
    item('Дублировать', 'edit.duplicate'),
    item('Удалить', 'edit.delete'),
    { separator: true },
    item('Сгруппировать', 'object.group'),
    item('Разгруппировать', 'object.ungroup'),
    item('Обернуть во фрейм', 'object.frame_selection'),
    { separator: true },
    item('На передний план', 'arrange.front'),
    item('Переместить вперёд', 'arrange.forward'),
    item('Переместить назад', 'arrange.backward'),
    item('На задний план', 'arrange.back'),
    { separator: true },
    item('Отразить по горизонтали', 'object.flip_horizontal'),
    item('Отразить по вертикали', 'object.flip_vertical'),
    item('Скрыть или показать', 'object.toggle_visible'),
    item('Заблокировать или разблокировать', 'object.toggle_lock'),
    { separator: true },
    item('Копировать как PNG', 'edit.copy_png'),
    item('Выделить всё', 'edit.select_all', false),
    item('Показать всё', 'view.fit_all', false),
  ];
}

export function Section(props: { title: string; children: JSX.Element; actions?: JSX.Element; muted?: boolean }) {
  return (
    <section class="fe-section" aria-label={props.title}>
      <div class="fe-section__head" classList={{ 't-muted': props.muted }}>
        <h2 class="fe-section__title">{props.title}</h2>
        <div class="fe-section__actions">{props.actions}</div>
      </div>
      {props.children}
    </section>
  );
}
````

`apps/web/src/ui/styles/chrome.css`:

````css
/* Панели, шапки, вкладки, тулбар, меню, края панелей (компоненты PanelHeader,
   Toolbar, Button дизайн-системы). */

.fe-side {
  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--surface);
  color: var(--ink);
}
.fe-side--left {
  grid-column: 1;
  grid-row: 1;
  border-right: 1px solid var(--line);
}
.fe-side--right {
  grid-column: 3;
  grid-row: 1;
  border-left: 1px solid var(--line);
}
.fe-app--bare .fe-side,
.fe-app--bare .fe-toolbar {
  display: none;
}

/* Шапка панели. */
.fe-panel-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-height: var(--toolbar-height);
  padding: 0 var(--space-3);
  border-bottom: 1px solid var(--line);
  flex: none;
}
.fe-panel-head__mark {
  width: 20px;
  height: 20px;
  border-radius: var(--radius-sm);
  background: var(--oxide);
  flex: none;
}
.fe-panel-head__file {
  display: grid;
  min-width: 0;
  flex: 1;
}
.fe-panel-head__name {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: default;
}
.fe-panel-head__project {
  font-size: 11px;
  color: var(--ink-muted);
}
.fe-panel-head__spacer {
  flex: 1;
}
.fe-rename {
  width: 100%;
  height: 20px;
  padding: 0 var(--space-1);
  border: 0;
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
  outline: 2px solid var(--select);
  font-weight: 600;
}

/* Зум в шапке: число в num и стрелка. */
.fe-zoom {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  height: var(--row-height);
  padding: 0 var(--space-2);
  border: 0;
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--ink-muted);
  cursor: pointer;
}
.fe-zoom:hover {
  background: var(--surface-hover);
  color: var(--ink);
}

/* Вкладки «Дизайн» / «Прототип». */
.fe-tabs {
  display: flex;
  gap: var(--space-1);
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--line);
  flex: none;
}
.fe-tab {
  height: 24px;
  padding: 0 var(--space-2);
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--ink-muted);
  font: 500 12px/16px var(--font-sans);
  cursor: pointer;
}
.fe-tab[aria-selected='true'] {
  background: var(--surface-hover);
  color: var(--ink);
}
.fe-tab[aria-disabled='true'],
.fe-tool[aria-disabled='true'],
.fe-btn[aria-disabled='true'] {
  color: var(--ink-faint);
  background: transparent;
  box-shadow: none;
  cursor: default;
  filter: none;
}

/* Тулбар — плавающий, внизу по центру холста. */
.fe-toolbar {
  position: absolute;
  left: 50%;
  bottom: var(--space-4);
  transform: translateX(-50%);
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  height: var(--toolbar-height);
  padding: 0 var(--space-1);
  background: var(--surface-raised);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-popover);
}
.fe-toolbar__sep {
  width: 1px;
  height: 20px;
  background: var(--line);
  margin: 0 var(--space-1);
}
.fe-tool-split {
  display: inline-flex;
  align-items: center;
}
.fe-tool--caret {
  width: 14px;
  margin-left: -2px;
}

/* Меню. */
.fe-menu {
  position: fixed;
  z-index: 40;
  min-width: 200px;
  max-height: calc(100vh - 16px);
  overflow: auto;
  padding: var(--space-1);
  border-radius: var(--radius-md);
  background: var(--surface-raised);
  box-shadow: var(--shadow-popover);
  color: var(--ink);
  font-size: 13px;
  line-height: 20px;
  animation: fe-pop 120ms ease-out;
}
.fe-menu__item {
  display: grid;
  grid-template-columns: 16px 1fr auto;
  align-items: center;
  gap: var(--space-2);
  min-height: var(--row-height);
  padding: 0 var(--space-2);
  border-radius: var(--radius-sm);
  cursor: default;
  white-space: nowrap;
}
.fe-menu__item:hover,
.fe-menu__item:focus {
  background: var(--surface-hover);
}
.fe-menu__item:focus-visible {
  outline: 2px solid var(--select);
  outline-offset: -2px;
}
.fe-menu__item[aria-disabled='true'] {
  color: var(--ink-faint);
  background: transparent;
}
.fe-menu__key {
  color: var(--ink-muted);
  font-size: 12px;
  padding-left: var(--space-4);
}
.fe-menu__item[aria-disabled='true'] .fe-menu__key {
  color: var(--ink-faint);
}
.fe-menu__sep {
  height: 1px;
  margin: var(--space-1) 0;
  background: var(--line);
}
.fe-menu__heading {
  padding: var(--space-1) var(--space-2) 0 calc(16px + var(--space-4));
  color: var(--ink-muted);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.01em;
}

/* Край панели. */
.fe-resize {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 6px;
  z-index: 5;
  cursor: ew-resize;
}
.fe-resize--left {
  right: -3px;
}
.fe-resize--right {
  left: -3px;
}
.fe-resize:hover,
.fe-resize:focus-visible {
  background: linear-gradient(var(--select), var(--select)) center / 2px 100% no-repeat;
  outline: none;
}

/* Секция панели свойств и слоёв (PropertySection). */
.fe-section {
  padding: var(--space-3);
  border-bottom: 1px solid var(--line);
  display: grid;
  gap: var(--space-2);
}
.fe-section__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 24px;
}
.fe-section__title {
  margin: 0;
  font-size: 11px;
  line-height: 16px;
  font-weight: 500;
  letter-spacing: 0.01em;
}
.fe-section__actions {
  display: flex;
  gap: 2px;
}
````

В `apps/web/src/ui/styles/base.css` заменить:

````css
}
.fe-btn--primary:disabled {
  background: var(--surface-hover);
}

````

на:

````css
}

````

В `apps/web/src/ui/styles/base.css` заменить:

````css
  display: flex;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-2);
````

на:

````css
  display: flex;
  white-space: pre;
  padding: var(--space-1) var(--space-2);
````

В `apps/web/src/ui/styles/base.css` заменить:

````css
  line-height: 16px;
  white-space: nowrap;
  pointer-events: none;
````

на:

````css
  line-height: 16px;
  pointer-events: none;
````

`apps/web/src/ui/app.tsx` (целиком):

````tsx
// Редактор: холст под управлением worker'а и панели вокруг него (раскладка UI3).

import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { boot } from './boot';
import { canvasMenu, DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { EditorContext, type Editor } from './context';
import { Menu, type MenuItem } from './menu';
import { createUiStore } from './store';
import { Toasts } from './toasts';
import { Toolbar } from './toolbar';
import { TooltipLayer } from './tooltip';

export function App() {
  const store = createUiStore();
  const [editor, setEditor] = createSignal<Editor>();
  const [failure, setFailure] = createSignal('');
  const [left, setLeft] = createSignal(248);
  const [right, setRight] = createSignal(248);
  const [context, setContext] = createSignal<{ x: number; y: number } | null>(null);
  let canvas!: HTMLCanvasElement;

  onMount(() => {
    boot(canvas, store)
      .then((ed) => {
        setEditor(ed);
        canvas.focus();
      })
      // Отказ `Core.start` уже начинается с «Ядро не запустилось: …».
      .catch((e: unknown) => setFailure(e instanceof Error ? e.message : `Ядро не запустилось: ${String(e)}`));
  });
  onCleanup(() => editor()?.core.dispose());

  const fileItems = (ed: Editor): MenuItem[] => [
    { label: 'Создать', onSelect: () => ed.send({ type: 'demo', nodes: 0 }) },
  ];

  return (
    <div
      class="fe-app"
      classList={{ 'fe-app--bare': store.state.uiHidden }}
      style={{ '--left-w': `${left()}px`, '--right-w': `${right()}px` }}
    >
      <main class="fe-stage">
        <canvas
          ref={canvas}
          class="fe-canvas"
          tabindex="0"
          aria-label="Холст"
          onContextMenu={(e) => {
            if (editor()) setContext({ x: e.clientX, y: e.clientY });
          }}
        />
        <Show when={!store.state.ready}>
          <div class="fe-boot" classList={{ 'fe-boot--error': !!failure() }} role="status">
            {failure() || 'Запуск редактора…'}
          </div>
        </Show>
        <Show when={store.state.stopped}>
          {(message) => (
            <div class="fe-boot fe-boot--error fe-boot--stopped" role="alert">
              {message()}. Правки больше не принимаются — перезагрузите страницу
            </div>
          )}
        </Show>
        <Show when={editor()}>
          {(ed) => (
            <EditorContext.Provider value={ed()}>
              <Toolbar />
            </EditorContext.Provider>
          )}
        </Show>
      </main>
      <Show when={editor()}>
        {(ed) => (
          <EditorContext.Provider value={ed()}>
            <aside class="fe-side fe-side--left" aria-label="Страницы и слои">
              <FileHeader items={() => fileItems(ed())} />
              <ResizeEdge side="left" width={left()} onWidth={setLeft} />
            </aside>
            <aside class="fe-side fe-side--right" aria-label="Свойства">
              <DesignHeader />
              <ResizeEdge side="right" width={right()} onWidth={setRight} />
            </aside>
            <TooltipLayer />
            <Show when={context()}>
              {(at) => (
                <Menu
                  label="Действия с выделением"
                  items={canvasMenu(store.state.selectionCount)}
                  anchor={at()}
                  onClose={() => setContext(null)}
                />
              )}
            </Show>
          </EditorContext.Provider>
        )}
      </Show>
      <Toasts store={store} />
    </div>
  );
}
````

В `apps/web/src/main.tsx` заменить:

````tsx
import './ui/styles/app.css';
import { render } from 'solid-js/web';
````

на:

````tsx
import './ui/styles/app.css';
import './ui/styles/chrome.css';
import { render } from 'solid-js/web';
````

- [ ] **Step 4: E2E раскладки**

Точка мира на экране редактора считается от угла холста: он теперь между панелями.

В `apps/web/e2e/helpers.ts` заменить:

````ts
  });
  await page.waitForFunction(() => (window.__ferrum!.core.live?.frames ?? 0) > 0);
}
````

на:

````ts
  });
  await page.waitForFunction(() => (window.__ferrum!.core.live?.frames ?? 0) > 0);
}

/** Точка мира на экране редактора: холст между панелями, его угол — со страницы. */
export async function appScreen(page: Page, l: LiveState, x: number, y: number): Promise<{ x: number; y: number }> {
  const box = (await page.locator('canvas').boundingBox())!;
  const p = screen(l, x, y);
  return { x: box.x + p.x, y: box.y + p.y };
}
````

`apps/web/e2e/chrome.spec.ts`:

````ts
// Раскладка UI3: тулбар рисует фигуры, меню и подсказки берут сочетания из
// раскладки, Ctrl+\ скрывает интерфейс, зум из шапки, ширина панелей,
// контекстное меню холста, роли для клавиатуры и экранного диктора.

import { expect, test, type Page } from '@playwright/test';
import { shortcutLabel } from '../src/ui/shortcuts';
import type { Platform } from '../src/core/core';
import { appScreen, drag, live, liveWhen, openApp } from './helpers';

const ui = <T>(page: Page, f: (s: NonNullable<Window['__ferrum']>['ui'] & object) => T) =>
  page.evaluate((src) => new Function('s', `return (${src})(s)`)(window.__ferrum!.ui), f.toString()) as Promise<T>;

test('прямоугольник рисуется инструментом из тулбара — один шаг', async ({ page }) => {
  await openApp(page);
  const toolbar = page.getByRole('toolbar', { name: 'Инструменты' });
  await toolbar.getByRole('button', { name: 'Фигуры' }).click();
  const menu = page.getByRole('menu', { name: 'Фигуры' });
  await expect(menu.getByRole('menuitemcheckbox')).toHaveCount(4);
  await menu.getByRole('menuitemcheckbox', { name: /Эллипс/ }).click();
  await expect.poll(() => ui(page, (s) => s.tool)).toBe('ellipse');
  await expect(toolbar.getByRole('button', { name: 'Эллипс' })).toHaveAttribute('aria-pressed', 'true');
  const before = await live(page);
  // Пустое место холста под первым фреймом: мир (100, 1080).
  const at = await appScreen(page, before, 100, 1080);
  await drag(page, at, 80, 40);
  const after = await liveWhen(page, `l.steps === ${before.steps + 1} && l.busy === 0`);
  expect([after.selectionCount, after.selectionW, after.selectionH]).toEqual([1, 160, 80]);
  await expect.poll(() => ui(page, (s) => s.tool)).toBe('move');
  await expect(toolbar.getByRole('button', { name: 'Эллипс' })).toHaveAttribute('aria-pressed', 'false');
});

test('Ctrl+\\ скрывает и показывает интерфейс', async ({ page }) => {
  await openApp(page);
  const left = page.getByRole('complementary', { name: 'Страницы и слои' });
  await expect(left).toBeVisible();
  await page.locator('canvas').click({ position: { x: 600, y: 40 } });
  await page.keyboard.press('ControlOrMeta+Backslash');
  await expect(left).toBeHidden();
  await expect(page.getByRole('toolbar')).toBeHidden();
  await liveWhen(page, 'l.viewportW === 1280');
  await page.keyboard.press('ControlOrMeta+Backslash');
  await expect(left).toBeVisible();
  await liveWhen(page, 'l.viewportW === 1280 - 2 * 248');
});

test('подсказка «Название  Клавиша» — из действующей раскладки', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Перемещение' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  V');
  await page.getByRole('button', { name: 'Перо — подсистема 3' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перо  P — подсистема 3');
  // Переназначили — подсказка другая.
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'bind', command: 'tool.move', shortcut: 'KeyQ', replace: true }));
  await page.mouse.move(5, 300);
  await expect(page.getByRole('tooltip')).toHaveCount(0);
  await page.getByRole('button', { name: 'Перемещение' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  V');
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'unbind', command: 'tool.move', shortcut: 'KeyV' }));
  await page.mouse.move(5, 300);
  await page.getByRole('button', { name: 'Перемещение' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  Q');
});

test('подписи сочетаний в интерфейсе совпадают с подписями ядра', async ({ page }) => {
  await openApp(page);
  const { platform, commands } = await page.evaluate(() => ({
    platform: navigator.platform.toLowerCase().startsWith('mac') ? 'mac' : 'linux',
    commands: window.__ferrum!.ui!.keymap.commands.map((c) => ({ bindings: [...c.bindings], labels: [...c.labels] })),
  }));
  let checked = 0;
  for (const c of commands) {
    c.bindings.forEach((b, i) => {
      expect(shortcutLabel(b, platform as Platform)).toBe(c.labels[i]);
      checked++;
    });
  }
  expect(checked).toBeGreaterThanOrEqual(commands.length);
});

test('зум из шапки и меню масштабов', async ({ page }) => {
  await openApp(page);
  const zoom = page.getByRole('button', { name: 'Масштаб', exact: true });
  await expect(zoom).toHaveText('50%');
  await zoom.click();
  await page.getByRole('menu', { name: 'Масштаб' }).getByRole('menuitemcheckbox', { name: /Масштаб 100%/ }).click();
  await liveWhen(page, 'l.zoom === 1');
  await expect(zoom).toHaveText('100%');
  // Клавиатура: меню открывается Enter, стрелки ходят по пунктам, Esc закрывает.
  await zoom.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('menuitem', { name: /Приблизить/ })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('menuitem', { name: /Отдалить/ })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('menu')).toHaveCount(0);
  await expect(zoom).toBeFocused();
});

test('ширина панели меняется перетаскиванием края', async ({ page }) => {
  await openApp(page);
  const edge = page.getByRole('separator', { name: 'Ширина левой панели' });
  const box = (await edge.boundingBox())!;
  await drag(page, { x: box.x + box.width / 2, y: 400 }, 100, 0);
  const l = await liveWhen(page, 'l.viewportW === 1280 - 248 - 348');
  expect(l.viewportW).toBe(684);
  await expect(edge).toHaveAttribute('aria-valuenow', '348');
  await edge.focus();
  await page.keyboard.press('ArrowLeft');
  await expect(edge).toHaveAttribute('aria-valuenow', '340');
});

test('контекстное меню холста выполняет команды редактора', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.mouse.click(node.x, node.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu.getByRole('menuitem', { name: /Дублировать/ })).toContainText(/D$/);
  await menu.getByRole('menuitem', { name: /Дублировать/ }).click();
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expect(menu).toHaveCount(0);
});

test('роли для клавиатуры: тулбар, вкладки, отключённые инструменты', async ({ page }) => {
  await openApp(page);
  await expect(page.getByRole('tablist')).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Дизайн' })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('tab', { name: 'Прототип' })).toHaveAttribute('aria-disabled', 'true');
  await expect(page.getByRole('button', { name: 'Поделиться' })).toHaveAttribute('aria-disabled', 'true');
  // Фокус с клавиатуры: кольцо select 2px.
  await page.getByRole('button', { name: 'Перемещение' }).focus();
  await page.keyboard.press('Tab');
  const outline = await page.evaluate(() => getComputedStyle(document.activeElement!).outlineWidth);
  expect(outline).toBe('2px');
});
````

Run: `cd apps/web && npm run typecheck && npm run build && npx playwright test e2e/chrome.spec.ts e2e/app.spec.ts`
Expected: PASS, `12 passed`.

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web
git commit -m "feat(web): раскладка UI3 — тулбар, шапки панелей, меню, подсказки из раскладки"
```

---

### Task 8: Панели слоёв и страниц

**Слои** — виртуальный список: панель просит у worker'а окно видимых строк с запасом по 10 строк сверху и снизу (`layersWindow`), строки стоят абсолютно по своему номеру, высота прокрутки — по общему числу строк. Чтение прокрутки — в фазе measure, окно запрашивается, только если сменилось. DOM-строки переиспользуются по месту (`Index`, а не `For`): при прокрутке меняются текст и атрибуты, а не создаются узлы с иконками заново — на 100 000 узлах это вдвое снижает худший кадр основного потока. Фокус клавиатуры привязан к слою, а не к месту: после прокрутки он возвращается к своей строке, стрелки за пределами окна прокручивают список и переводят фокус, когда строка придёт.

Щелчок — выделить, Shift — диапазон (считает worker), Ctrl/Cmd — добавить или убрать; наведение подсвечивает узел на холсте; выделение с холста раскрывает предков и прокручивает к строке. Двойной щелчок или F2 — переименование на месте (Enter применяет, Esc отменяет — и уход фокуса удаляемого поля имени не применяет). Перетаскивание строки (после 4 px): верхняя и нижняя четверть строки — над и под ней, середина фрейма или группы — внутрь; выделенную строку переносит всё выделение. Кнопки «Скрыть» и «Заблокировать» появляются при наведении, включённые видны всегда; с клавиатуры — команды редактора (Ctrl+Shift+H, Ctrl+Shift+L). Скрытые и лежащие в скрытых — тусклые. Поиск по слоям считает worker.

**Страницы** — сворачиваемый список (`listbox`): добавить («Страница N», сразу текущая), переключить, двойной щелчок или F2 — переименовать, удалить (кнопка или Delete) — с подтверждением: удаление страницы не отменяется. Единственную удалить нельзя.

**Files:**
- Create: `apps/web/src/ui/layers.tsx`
- Create: `apps/web/src/ui/pages.tsx`
- Create: `apps/web/src/ui/styles/layers.css`
- Modify: `apps/web/src/ui/keys.ts`
- Modify: `apps/web/src/ui/app.tsx`
- Modify: `apps/web/src/main.tsx`
- Test: `apps/web/e2e/layers.spec.ts`

**Interfaces:**
- Consumes: `store.state.{layers, pages}`, команды `layersWindow`, `expandLayer`, `filterLayers`, `select`, `hoverLayer`, `renameLayer`, `moveLayers`, `setLayerFlags`, `addPage`, `setPage`, `renamePage`, `deletePage` (Task 5); `scheduler.measure/mutate`, `core.costed`.
- Produces: `LayersPanel`, `ROW`, `OVERSCAN`, `windowFor(scrollTop, height)`, `dropPosition(offset, row)`; `PagesPanel`. `keys.ts`: строки дерева оставляют себе стрелки и F2, `alertdialog` — все клавиши.

- [ ] **Step 1: Написать E2E слоёв и страниц**

`apps/web/e2e/layers.spec.ts`:

````ts
// Панель слоёв и страниц: выделение в обе стороны, переименование, порядок
// перетаскиванием, скрыть и заблокировать, страницы, виртуальный список
// (в DOM — только окно строк).

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

const tree = (page: Page) => page.getByRole('tree', { name: 'Слои' });
const row = (page: Page, name: string) => tree(page).getByRole('treeitem', { name, exact: true });
const names = (page: Page) => tree(page).locator('.fe-layer__name').allTextContents();

test('выделение из панели слоёв и с холста', async ({ page }) => {
  await openApp(page);
  await row(page, 'Экран 8').click();
  await expect(row(page, 'Экран 8')).toHaveAttribute('aria-selected', 'true');
  const l = await liveWhen(page, 'l.selectionCount === 1');
  expect([l.selectionW, l.selectionH]).toEqual([1440, 1024]);
  // Щелчок по узлу на холсте: фрейм раскрывается, строка выделена.
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y);
  await expect(row(page, 'Экран 0')).toHaveAttribute('aria-expanded', 'true');
  await expect(row(page, 'Слой 1')).toHaveAttribute('aria-selected', 'true');
  await expect(row(page, 'Слой 1')).toBeInViewport();
  // Shift — диапазон, Ctrl/Cmd — добавить. Строки фреймов наверху списка: список
  // прокручен к «Слой 1», их нет в DOM, пока не прокрутить обратно.
  await tree(page).evaluate((el) => (el.scrollTop = 0));
  await row(page, 'Экран 9').click();
  await row(page, 'Экран 7').click({ modifiers: ['Shift'] });
  await liveWhen(page, 'l.selectionCount === 3');
  await row(page, 'Экран 5').click({ modifiers: ['ControlOrMeta'] });
  await liveWhen(page, 'l.selectionCount === 4');
});

test('переименование на месте — один шаг отмены', async ({ page }) => {
  await openApp(page);
  const before = await live(page);
  await row(page, 'Экран 9').dblclick();
  const input = tree(page).getByRole('textbox', { name: 'Имя слоя' });
  await input.fill('Главная');
  await input.press('Enter');
  await expect(row(page, 'Главная')).toBeVisible();
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(row(page, 'Экран 9')).toBeVisible();
  // Esc отменяет переименование.
  await row(page, 'Экран 9').dblclick();
  await input.fill('Не то');
  await input.press('Escape');
  await expect(row(page, 'Экран 9')).toBeVisible();
  // Поле убрано, а blur при удалении поля имени не применил.
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before.steps + 1);
});

test('перетаскивание меняет порядок и вложенность', async ({ page }) => {
  await openApp(page);
  const before = await live(page);
  const from = (await row(page, 'Экран 9').boundingBox())!;
  const to = (await row(page, 'Экран 8').boundingBox())!;
  // Нижняя четверть строки «Экран 8» — под ней.
  await page.mouse.move(from.x + 60, from.y + 14);
  await page.mouse.down();
  await page.mouse.move(from.x + 60, to.y + 10, { steps: 4 });
  await page.mouse.move(from.x + 60, to.y + 25, { steps: 4 });
  await expect(row(page, 'Экран 8')).toHaveClass(/fe-layer--drop-below/);
  await page.mouse.up();
  await expect.poll(() => names(page).then((n) => n.slice(0, 2))).toEqual(['Экран 8', 'Экран 9']);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  // Середина строки фрейма — внутрь.
  const a = (await row(page, 'Экран 7').boundingBox())!;
  const b = (await row(page, 'Экран 6').boundingBox())!;
  await page.mouse.move(a.x + 60, a.y + 14);
  await page.mouse.down();
  await page.mouse.move(a.x + 60, b.y + 14, { steps: 6 });
  await page.mouse.up();
  await expect(row(page, 'Экран 6')).toHaveAttribute('aria-expanded', 'false');
  await row(page, 'Экран 6').getByRole('button', { name: 'Развернуть' }).click();
  await expect(row(page, 'Экран 7')).toHaveAttribute('aria-level', '2');
});

test('скрыть и заблокировать кнопками строки', async ({ page }) => {
  await openApp(page);
  const r = row(page, 'Экран 9');
  await r.hover();
  await r.getByRole('button', { name: 'Скрыть' }).click();
  await expect(r.getByRole('button', { name: 'Показать' })).toHaveAttribute('aria-pressed', 'true');
  await expect(r).toHaveClass(/fe-layer--hidden/);
  await r.getByRole('button', { name: 'Заблокировать' }).click();
  await expect(r.getByRole('button', { name: 'Разблокировать' })).toBeVisible();
  // Кнопки включённых флагов видны и без наведения.
  await page.mouse.move(700, 400);
  await expect(r.getByRole('button', { name: 'Показать' })).toBeVisible();
  const l = await live(page);
  expect(l.steps).toBe(2);
});

test('страницы: добавить, переключить, переименовать, удалить', async ({ page }) => {
  await openApp(page);
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  await page.getByRole('button', { name: 'Добавить страницу' }).click();
  await expect(pages.getByRole('option')).toHaveCount(2);
  await expect(pages.getByRole('option', { name: /Страница 2/ })).toHaveAttribute('aria-selected', 'true');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(0);
  await pages.getByRole('option', { name: /Страница 1/ }).click();
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(10);
  await pages.getByRole('option', { name: /Страница 2/ }).dblclick();
  await pages.getByRole('textbox', { name: 'Имя страницы' }).fill('Черновик');
  await pages.getByRole('textbox', { name: 'Имя страницы' }).press('Enter');
  await expect(pages.getByRole('option', { name: /Черновик/ })).toBeVisible();
  await pages.getByRole('option', { name: /Черновик/ }).hover();
  await pages.getByRole('button', { name: 'Удалить страницу «Черновик»' }).click();
  // Удаление страницы не отменяется: сначала вопрос, Отмена — в фокусе.
  const confirm = page.getByRole('alertdialog', { name: 'Удалить страницу?' });
  await expect(confirm.getByRole('button', { name: 'Отмена' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(confirm).toHaveCount(0);
  await expect(pages.getByRole('option')).toHaveCount(2);
  await pages.getByRole('option', { name: /Черновик/ }).focus();
  await page.keyboard.press('Delete');
  await confirm.getByRole('button', { name: 'Удалить' }).click();
  await expect(pages.getByRole('option')).toHaveCount(1);
  // Сворачивается.
  await page.getByRole('button', { name: 'Страницы' }).click();
  await expect(pages).toHaveCount(0);
});

test('виртуальный список: в DOM только окно строк', async ({ page }) => {
  await openApp(page, 20_000);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(200);
  const count = () => tree(page).getByRole('treeitem').count();
  expect(await count()).toBeLessThanOrEqual(60);
  await tree(page).evaluate((el) => (el.scrollTop = 150 * 28));
  await expect(row(page, 'Экран 49')).toBeVisible();
  expect(await count()).toBeLessThanOrEqual(60);
  // Клавиатура: стрелки ведут фокус за пределы окна, список подкручивается, фокус
  // остаётся на своём слое (DOM-строки переиспользуются по месту).
  await tree(page).evaluate((el) => (el.scrollTop = 0));
  await row(page, 'Экран 199').focus();
  for (let i = 0; i < 70; i++) await page.keyboard.press('ArrowDown');
  await expect(page.locator(':focus')).toHaveAttribute('aria-label', 'Экран 129');
  await page.keyboard.press('Enter');
  await expect(row(page, 'Экран 129')).toHaveAttribute('aria-selected', 'true');
  // Поиск считает worker: совпавшие и их предки.
  await page.getByRole('searchbox', { name: 'Поиск слоёв' }).fill('Слой 98');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(400);
});
````

Run: `cd apps/web && npm run build && npx playwright test e2e/layers.spec.ts --timeout 15000 --max-failures 1`
Expected: FAIL — на странице ещё нет дерева `tree «Слои»` (первый тест падает по таймауту, остальные не запускаются).

- [ ] **Step 2: Панели слоёв и страниц**

`apps/web/src/ui/layers.tsx`:

````tsx
// Панель слоёв: виртуальный список. Основной поток не держит дерево — он просит
// у worker'а окно строк, которое видно (с запасом `OVERSCAN` сверху и снизу), и
// рисует только их; высота прокрутки — по общему числу строк. Раскрытие, поиск,
// диапазон для Shift считает worker. Переименование на месте, перетаскивание для
// порядка и вложенности, кнопки «скрыть» и «заблокировать», наведение
// подсвечивает узел на холсте, выделение синхронно в обе стороны.

import { createEffect, createSignal, Index, on, onCleanup, onMount, Show } from 'solid-js';
import type { DropPosition, LayerRow, NodeType } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { Icon, type IconName } from './icons';

/** Высота строки — `row-height` дизайн-системы. */
export const ROW = 28;
/** Строк сверху и снизу от видимых: прокрутка не показывает пустоты. */
export const OVERSCAN = 10;
/** Сдвиг указателя, после которого нажатие на строке становится перетаскиванием. */
const DRAG_START_PX = 4;

const KIND_ICON: Record<NodeType, IconName> = {
  frame: 'frame',
  group: 'group',
  rect: 'rect',
  ellipse: 'ellipse',
  line: 'line',
  image: 'image',
};

/** Окно строк для прокрутки `scrollTop` и высоты `height`. */
export function windowFor(scrollTop: number, height: number): { start: number; count: number } {
  const first = Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN);
  return { start: first, count: Math.ceil(height / ROW) + 2 * OVERSCAN };
}

/** Куда бросить: верхняя четверть строки — над ней, нижняя — под ней, середина
 * контейнера — внутрь; у листа середина делится пополам. */
export function dropPosition(offset: number, row: Pick<LayerRow, 'kind'>): DropPosition {
  const container = row.kind === 'frame' || row.kind === 'group';
  if (container) {
    if (offset < ROW / 4) return 'above';
    if (offset > (ROW * 3) / 4) return 'below';
    return 'into';
  }
  return offset < ROW / 2 ? 'above' : 'below';
}

interface Drag {
  id: string;
  selected: boolean;
  y0: number;
  active: boolean;
  target: { index: number; position: DropPosition } | null;
}

export function LayersPanel() {
  const ed = useEditor();
  const layers = () => ed.store.state.layers;
  const [renaming, setRenaming] = createSignal<string | null>(null);
  const [drop, setDrop] = createSignal<{ index: number; position: DropPosition } | null>(null);
  let tree!: HTMLDivElement;
  let asked = { start: -1, count: -1 };
  let drag: Drag | null = null;
  /** Прокрутка и верх списка с последнего замера (для перетаскивания строк). */
  let scrollTop = 0;
  let treeTop: number | null = null;
  /** Строка с фокусом клавиатуры: DOM-строки переиспользуются по месту, и после
   * прокрутки фокус возвращается к своему слою, а не к месту. */
  let focusedId: string | null = null;
  /** Номер строки, куда перевести фокус, когда она придёт от worker'а. */
  let pendingFocus: number | null = null;

  /** Чтение прокрутки — в фазе measure; запрос окна — только если оно сменилось. */
  const requestWindow = () =>
    ed.scheduler.measure(() => {
      scrollTop = tree.scrollTop;
      treeTop = tree.getBoundingClientRect().top;
      const w = windowFor(tree.scrollTop, tree.clientHeight);
      if (w.start !== asked.start || w.count !== asked.count) {
        asked = w;
        ed.send({ type: 'layersWindow', start: w.start, count: w.count });
      }
    });

  onMount(() => {
    requestWindow();
    const ro = new ResizeObserver(() => requestWindow());
    ro.observe(tree);
    onCleanup(() => ro.disconnect());
  });

  // Выделение пришло с холста: worker раскрыл предков и прислал строку — прокрутить
  // к ней, если её не видно.
  createEffect(
    on(
      () => layers().revealSeq,
      () => {
        const at = layers().reveal;
        if (at == null) return;
        ed.scheduler.measure(() => {
          const top = at * ROW;
          if (top < tree.scrollTop || top + ROW > tree.scrollTop + tree.clientHeight) {
            const next = Math.max(0, top - tree.clientHeight / 2);
            ed.scheduler.mutate(() => {
              tree.scrollTop = next;
              requestWindow();
            });
          }
        });
      },
      { defer: true },
    ),
  );

  // Новые строки окна: фокус — к своему слою или к строке, которую ждали.
  createEffect(
    on(
      () => [layers().start, layers().rows.map((r) => r.id).join()],
      () => {
        if (pendingFocus != null) {
          const el = tree.querySelector<HTMLElement>(`[data-index="${pendingFocus}"]`);
          if (el) {
            pendingFocus = null;
            el.focus();
          }
          return;
        }
        const active = document.activeElement as HTMLElement | null;
        if (!focusedId || !active || !tree.contains(active) || active.dataset.id === focusedId) return;
        const own = tree.querySelector<HTMLElement>(`[data-id="${CSS.escape(focusedId)}"]`);
        if (own) own.focus();
        else tree.focus();
      },
      { defer: true },
    ),
  );

  /** Фокус на строку `index`: сразу, если строка в DOM, иначе — когда придёт от
   * worker'а; список подкручивается к ней в фазах кадра. */
  const focusAt = (index: number) => {
    const i = Math.max(0, Math.min(layers().total - 1, index));
    const el = tree.querySelector<HTMLElement>(`[data-index="${i}"]`);
    pendingFocus = el ? null : i;
    el?.focus({ preventScroll: true });
    ed.scheduler.measure(() => {
      const top = i * ROW;
      const next =
        top < tree.scrollTop ? top : top + ROW > tree.scrollTop + tree.clientHeight ? top + ROW - tree.clientHeight : null;
      if (next == null) return;
      ed.scheduler.mutate(() => {
        tree.scrollTop = next;
        requestWindow();
      });
    });
  };

  const select = (row: LayerRow, e: MouseEvent) => {
    const mode = e.shiftKey ? 'range' : e.metaKey || e.ctrlKey ? 'toggle' : 'replace';
    ed.send({ type: 'select', ids: [row.id], mode });
  };

  const onPointerDown = (row: LayerRow, e: PointerEvent) => {
    if (e.button !== 0 || renaming()) return;
    const el = e.currentTarget as HTMLElement;
    drag = { id: row.id, selected: row.selected, y0: e.clientY, active: false, target: null };
    el.setPointerCapture(e.pointerId);
    requestWindow();
  };

  const onPointerMove = ed.core.costed((e: PointerEvent) => {
    if (!drag || treeTop === null) return;
    if (!drag.active && Math.abs(e.clientY - drag.y0) < DRAG_START_PX) return;
    drag.active = true;
    // Прокрутка во время перетаскивания обновляет `scrollTop` (замер окна).
    const y = e.clientY - treeTop + scrollTop;
    const index = Math.floor(y / ROW);
    const row = layers().rows[index - layers().start];
    drag.target = row && row.id !== drag.id ? { index, position: dropPosition(y - index * ROW, row) } : null;
    setDrop(drag.target);
  });

  const onPointerUp = (row: LayerRow, e: PointerEvent) => {
    const d = drag;
    drag = null;
    setDrop(null);
    if (!d) return;
    if (!d.active) {
      select(row, e);
      return;
    }
    const target = d.target && layers().rows[d.target.index - layers().start];
    if (!target) return;
    // Перетащили выделенную строку — переносится всё выделение (его полный список
    // есть только у worker'а).
    ed.send({
      type: 'moveLayers',
      ids: d.selected ? undefined : [d.id],
      target: target.id,
      position: d.target!.position,
    });
  };

  const rename = (row: LayerRow, name: string) => {
    // Esc уже закрыл поле: blur удаляемого поля не должен применять набранное.
    if (renaming() !== row.id) return;
    setRenaming(null);
    if (name !== row.name) ed.send({ type: 'renameLayer', id: row.id, name });
  };

  const onKey = (row: LayerRow, index: number, e: KeyboardEvent) => {
    switch (e.key) {
      // Пока ждём строку от worker'а, следующие нажатия считаются от неё.
      case 'ArrowDown':
        focusAt((pendingFocus ?? index) + 1);
        break;
      case 'ArrowUp':
        focusAt((pendingFocus ?? index) - 1);
        break;
      case 'ArrowRight':
        if (row.hasChildren && !row.expanded) ed.send({ type: 'expandLayer', id: row.id, expanded: true });
        break;
      case 'ArrowLeft':
        if (row.expanded) ed.send({ type: 'expandLayer', id: row.id, expanded: false });
        break;
      case 'Enter':
      case ' ':
        ed.send({ type: 'select', ids: [row.id], mode: 'replace' });
        break;
      case 'F2':
        setRenaming(row.id);
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  return (
    <section class="fe-layers-panel" aria-label="Слои">
      <div class="fe-section__head fe-layers-panel__head">
        <h2 class="fe-section__title">Слои</h2>
      </div>
      <label class="fe-field fe-field--text fe-layers-panel__search">
        <Icon name="search" class="t-muted fe-field__icon" />
        <input
          type="search"
          placeholder="Поиск слоёв"
          aria-label="Поиск слоёв"
          onInput={(e) => ed.send({ type: 'filterLayers', query: e.currentTarget.value })}
        />
      </label>
      <div
        ref={tree}
        class="fe-tree"
        role="tree"
        tabindex="-1"
        aria-label="Слои"
        onScroll={ed.core.costed(() => requestWindow())}
        onPointerLeave={() => ed.send({ type: 'hoverLayer', id: undefined })}
      >
        <div class="fe-tree__spacer" style={{ height: `${layers().total * ROW}px` }} />
        {/* Index, а не For: при прокрутке строки DOM переиспользуются по месту —
            меняются текст и атрибуты, а не создаются узлы с иконками заново. */}
        <Index each={layers().rows}>
          {(row, i) => {
            const index = () => layers().start + i;
            return (
              <div
                class="fe-layer"
                classList={{
                  'fe-layer--child-of-selected': row().parentSelected && !row().selected,
                  'fe-layer--hidden': !row().visible || row().ancestorHidden,
                  'fe-layer--drop-into': drop()?.index === index() && drop()?.position === 'into',
                  'fe-layer--drop-above': drop()?.index === index() && drop()?.position === 'above',
                  'fe-layer--drop-below': drop()?.index === index() && drop()?.position === 'below',
                }}
                style={{ top: `${index() * ROW}px`, 'padding-left': `${row().depth * 12 + 4}px` }}
                role="treeitem"
                aria-label={row().name}
                data-id={row().id}
                data-index={index()}
                aria-level={row().depth + 1}
                aria-selected={row().selected}
                aria-expanded={row().hasChildren ? row().expanded : undefined}
                tabindex={row().selected || (index() === layers().start && !layers().rows.some((r) => r.selected)) ? 0 : -1}
                title={row().name}
                onPointerEnter={() => ed.send({ type: 'hoverLayer', id: row().id })}
                onPointerDown={(e) => onPointerDown(row(), e)}
                onPointerMove={onPointerMove}
                onPointerUp={(e) => onPointerUp(row(), e)}
                onPointerCancel={() => {
                  drag = null;
                  setDrop(null);
                }}
                onFocus={() => (focusedId = row().id)}
                onDblClick={() => setRenaming(row().id)}
                onKeyDown={(e) => onKey(row(), index(), e)}
              >
                <span class="fe-layer__caret">
                  <Show when={row().hasChildren}>
                    <button
                      class="fe-layer__toggle"
                      classList={{ 'fe-layer__toggle--open': row().expanded }}
                      tabindex="-1"
                      aria-label={row().expanded ? 'Свернуть' : 'Развернуть'}
                      onPointerDown={(e) => e.stopPropagation()}
                      onClick={(e) => {
                        e.stopPropagation();
                        ed.send({ type: 'expandLayer', id: row().id, expanded: !row().expanded });
                      }}
                    >
                      <Icon name="expand" size={12} />
                    </button>
                  </Show>
                </span>
                <span class="fe-layer__icon">
                  <Icon name={KIND_ICON[row().kind]} />
                </span>
                <Show when={renaming() === row().id} fallback={<span class="fe-layer__name">{row().name}</span>}>
                  <input
                    class="fe-rename fe-layer__rename"
                    aria-label="Имя слоя"
                    value={row().name}
                    ref={(el) => queueMicrotask(() => el.select())}
                    onPointerDown={(e) => e.stopPropagation()}
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === 'Enter') rename(row(), e.currentTarget.value);
                      if (e.key === 'Escape') setRenaming(null);
                    }}
                    onBlur={(e) => rename(row(), e.currentTarget.value)}
                  />
                </Show>
                <span class="fe-layer__actions">
                  <button
                    class="fe-tool fe-tool--small fe-tool--muted"
                    classList={{ 'fe-layer__flag--on': row().locked }}
                    tabindex="-1"
                    aria-label={row().locked ? 'Разблокировать' : 'Заблокировать'}
                    aria-pressed={row().locked}
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={(e) => {
                      e.stopPropagation();
                      ed.send({ type: 'setLayerFlags', ids: [row().id], visible: undefined, locked: !row().locked });
                    }}
                  >
                    <Icon name={row().locked ? 'lock' : 'unlock'} />
                  </button>
                  <button
                    class="fe-tool fe-tool--small fe-tool--muted"
                    classList={{ 'fe-layer__flag--on': !row().visible }}
                    tabindex="-1"
                    aria-label={row().visible ? 'Скрыть' : 'Показать'}
                    aria-pressed={!row().visible}
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={(e) => {
                      e.stopPropagation();
                      ed.send({ type: 'setLayerFlags', ids: [row().id], visible: !row().visible, locked: undefined });
                    }}
                  >
                    <Icon name={row().visible ? 'eye' : 'eyeOff'} />
                  </button>
                </span>
              </div>
            );
          }}
        </Index>
      </div>
    </section>
  );
}
````

`apps/web/src/ui/pages.tsx`:

````tsx
// Список страниц (сворачиваемый): добавить, переименовать на месте, удалить,
// переключить. Операции со страницами в историю отмены не попадают, поэтому
// удаление — только после подтверждения.

import { createSignal, For, Show } from 'solid-js';
import type { PageInfo } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { Icon } from './icons';

export function PagesPanel(props: { open: boolean; onToggle: (open: boolean) => void }) {
  const ed = useEditor();
  const pages = () => ed.store.state.pages;
  const [renaming, setRenaming] = createSignal<string | null>(null);
  const [deleting, setDeleting] = createSignal<PageInfo | null>(null);
  const askDelete = (p: PageInfo) => {
    if (pages().list.length > 1) setDeleting(p);
  };
  const rename = (p: PageInfo, name: string) => {
    // Esc уже закрыл поле: blur удаляемого поля не должен применять набранное.
    if (renaming() !== p.id) return;
    setRenaming(null);
    if (name.trim() && name !== p.name) ed.send({ type: 'renamePage', id: p.id, name });
  };
  return (
    <section class="fe-pages" aria-label="Страницы">
      <div class="fe-section__head fe-pages__head">
        <button
          class="fe-pages__toggle"
          aria-expanded={props.open}
          aria-controls="fe-pages-list"
          onClick={() => props.onToggle(!props.open)}
        >
          <Icon name="expand" size={12} class={props.open ? 'fe-rot90' : ''} />
          <h2 class="fe-section__title">Страницы</h2>
        </button>
        <button
          class="fe-tool fe-tool--small"
          aria-label="Добавить страницу"
          data-tip="Добавить страницу"
          onClick={() => ed.send({ type: 'addPage' })}
        >
          <Icon name="plus" />
        </button>
      </div>
      <Show when={props.open}>
        <ul class="fe-pages__list" id="fe-pages-list" role="listbox" aria-label="Страницы">
          <For each={pages().list}>
            {(p) => {
              const current = () => p.id === pages().current;
              return (
                <li
                  class="fe-page"
                  role="option"
                  aria-selected={current()}
                  tabindex={current() ? 0 : -1}
                  onClick={() => !current() && ed.send({ type: 'setPage', id: p.id })}
                  onDblClick={() => setRenaming(p.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') ed.send({ type: 'setPage', id: p.id });
                    else if (e.key === 'F2') setRenaming(p.id);
                    else if (e.key === 'Delete' || e.key === 'Backspace') askDelete(p);
                    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                      const items = Array.from((e.currentTarget.parentElement as HTMLElement).children) as HTMLElement[];
                      items[items.indexOf(e.currentTarget) + (e.key === 'ArrowDown' ? 1 : -1)]?.focus();
                    } else return;
                    e.preventDefault();
                  }}
                >
                  <span class="fe-page__check">
                    <Show when={current()}>
                      <Icon name="check" />
                    </Show>
                  </span>
                  <Show when={renaming() === p.id} fallback={<span class="fe-page__name">{p.name}</span>}>
                    <input
                      class="fe-rename"
                      aria-label="Имя страницы"
                      value={p.name}
                      ref={(el) => queueMicrotask(() => el.select())}
                      onClick={(e) => e.stopPropagation()}
                      onKeyDown={(e) => {
                        e.stopPropagation();
                        if (e.key === 'Enter') rename(p, e.currentTarget.value);
                        if (e.key === 'Escape') setRenaming(null);
                      }}
                      onBlur={(e) => rename(p, e.currentTarget.value)}
                    />
                  </Show>
                  <button
                    class="fe-tool fe-tool--small fe-tool--muted fe-page__delete"
                    aria-label={`Удалить страницу «${p.name}»`}
                    aria-disabled={pages().list.length <= 1 ? 'true' : undefined}
                    data-tip={pages().list.length <= 1 ? 'Единственную страницу удалить нельзя' : 'Удалить страницу'}
                    tabindex="-1"
                    onClick={(e) => {
                      e.stopPropagation();
                      askDelete(p);
                    }}
                  >
                    <Icon name="trash" />
                  </button>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>
      <Show when={deleting()}>
        {(p) => (
          <div class="fe-confirm fe-pages__confirm" role="alertdialog" aria-label="Удалить страницу?">
            <span class="fe-row__grow">Удалить страницу «{p().name}»? Отменить это нельзя.</span>
            <button
              class="fe-btn fe-btn--danger"
              onClick={() => {
                ed.send({ type: 'deletePage', id: p().id });
                setDeleting(null);
              }}
            >
              Удалить
            </button>
            <button
              class="fe-btn"
              ref={(el) => queueMicrotask(() => el.focus())}
              onClick={() => setDeleting(null)}
              onKeyDown={(e) => e.key === 'Escape' && setDeleting(null)}
            >
              Отмена
            </button>
          </div>
        )}
      </Show>
    </section>
  );
}
````

`apps/web/src/ui/styles/layers.css`:

````css
/* Левая панель: страницы и дерево слоёв (компонент LayerRow). */

.fe-pages {
  flex: none;
  max-height: 30%;
  display: flex;
  flex-direction: column;
  border-bottom: 1px solid var(--line);
  padding: var(--space-2) 0;
}
.fe-pages__head {
  padding: 0 var(--space-2) 0 var(--space-1);
}
.fe-pages__toggle {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  height: 24px;
  padding: 0 var(--space-1);
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  cursor: pointer;
}
.fe-pages__toggle:hover {
  background: var(--surface-hover);
}
.fe-rot90 {
  transform: rotate(90deg);
}
.fe-pages__list {
  list-style: none;
  margin: 0;
  padding: 0;
  overflow: auto;
}
.fe-page {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  height: var(--row-height);
  padding: 0 var(--space-2) 0 var(--space-1);
  cursor: default;
}
.fe-page:hover {
  background: var(--surface-hover);
}
.fe-page:focus-visible {
  outline-offset: -2px;
}
.fe-page[aria-selected='true'] .fe-page__name {
  font-weight: 600;
}
.fe-page__check {
  width: 20px;
  display: grid;
  place-items: center;
  flex: none;
}
.fe-page__name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fe-page__delete {
  visibility: hidden;
}
.fe-pages__confirm {
  flex-wrap: wrap;
  margin: var(--space-2) var(--space-2) 0;
}
.fe-page:hover .fe-page__delete,
.fe-page:focus-within .fe-page__delete {
  visibility: visible;
}

.fe-layers-panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding-top: var(--space-2);
}
.fe-layers-panel__head {
  padding: 0 var(--space-3);
}
.fe-layers-panel__search {
  margin: var(--space-1) var(--space-3) var(--space-2);
  flex: none;
}
.fe-field__icon {
  margin-left: var(--space-2);
}
.fe-layers-panel__search input {
  padding-left: var(--space-1);
}

/* Виртуальный список: строки стоят абсолютно по своему номеру. */
.fe-tree {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: auto;
  contain: strict;
}
.fe-tree__spacer {
  width: 1px;
}
.fe-layer {
  position: absolute;
  left: 0;
  right: 0;
  display: flex;
  align-items: center;
  gap: var(--space-1);
  height: var(--row-height);
  padding-right: var(--space-2);
  color: var(--ink);
  cursor: default;
  user-select: none;
}
.fe-layer:hover {
  background: var(--surface-hover);
}
.fe-layer[aria-selected='true'] {
  background: var(--select-soft);
}
.fe-layer--child-of-selected {
  background: color-mix(in srgb, var(--select-soft) 45%, transparent);
}
.fe-layer:focus-visible {
  outline-offset: -2px;
}
.fe-layer__icon {
  display: grid;
  color: var(--ink-muted);
}
.fe-layer__name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fe-layer[aria-selected='true'] .fe-layer__name {
  font-weight: 600;
}
.fe-layer--hidden .fe-layer__name,
.fe-layer--hidden .fe-layer__icon {
  color: var(--ink-faint);
}
.fe-layer__caret {
  width: 16px;
  flex: none;
  display: grid;
  place-items: center;
}
.fe-layer__toggle {
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--ink-muted);
  cursor: pointer;
}
.fe-layer__toggle--open .fe-ico {
  transform: rotate(90deg);
}
.fe-layer__rename {
  flex: 1;
  min-width: 0;
}
.fe-layer__actions {
  display: flex;
  gap: 2px;
}
.fe-layer__actions .fe-tool {
  visibility: hidden;
}
.fe-layer:hover .fe-layer__actions .fe-tool,
.fe-layer__actions .fe-layer__flag--on {
  visibility: visible;
}
.fe-layer--drop-into {
  box-shadow: inset 0 0 0 2px var(--select);
}
.fe-layer--drop-above::before,
.fe-layer--drop-below::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--select);
}
.fe-layer--drop-above::before {
  top: -1px;
}
.fe-layer--drop-below::after {
  bottom: -1px;
}
````

В `apps/web/src/ui/keys.ts` заменить:

````ts
  // Меню, диалоги, списки: стрелки, Enter, Esc, Tab — их клавиатурная навигация.
  return !!t.closest('[role="menu"], [role="dialog"], [role="listbox"]');
}

/** Клавиши кнопок и вкладок: активировать и переходить фокусом. */
const WIDGET_KEYS = new Set(['Tab', 'Enter', 'Space', 'NumpadEnter']);

````

на:

````ts
  // Меню, диалоги, списки: стрелки, Enter, Esc, Tab — их клавиатурная навигация.
  return !!t.closest('[role="menu"], [role="dialog"], [role="alertdialog"], [role="listbox"]');
}

/** Клавиши кнопок и вкладок: активировать и переходить фокусом. */
const WIDGET_KEYS = new Set(['Tab', 'Enter', 'Space', 'NumpadEnter']);
/** Строки дерева слоёв: ещё стрелки (ходить и раскрывать) и F2 (переименовать). */
const TREE_KEYS = new Set([...WIDGET_KEYS, 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'F2']);

````

В `apps/web/src/ui/keys.ts` заменить:

````ts
  if (e.code === 'Tab') return false;
  const onControl = e.target instanceof Element && !!e.target.closest('button, [role="tab"], [role="treeitem"], a');
  if (onControl && WIDGET_KEYS.has(e.code)) return false;
  // Ctrl/Cmd+C, X, V идут событиями copy/cut/paste (системный буфер), а не в ядро.
````

на:

````ts
  if (e.code === 'Tab') return false;
  const el = e.target instanceof Element ? e.target : null;
  if (el?.closest('[role="treeitem"]') && TREE_KEYS.has(e.code)) return false;
  if (el?.closest('button, [role="tab"], a') && WIDGET_KEYS.has(e.code)) return false;
  // Ctrl/Cmd+C, X, V идут событиями copy/cut/paste (системный буфер), а не в ядро.
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { Menu, type MenuItem } from './menu';
import { createUiStore } from './store';
````

на:

````tsx
import { Menu, type MenuItem } from './menu';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
import { createUiStore } from './store';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  const [context, setContext] = createSignal<{ x: number; y: number } | null>(null);
  let canvas!: HTMLCanvasElement;
````

на:

````tsx
  const [context, setContext] = createSignal<{ x: number; y: number } | null>(null);
  const [pagesOpen, setPagesOpen] = createSignal(true);
  let canvas!: HTMLCanvasElement;
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
              <FileHeader items={() => fileItems(ed())} />
              <ResizeEdge side="left" width={left()} onWidth={setLeft} />
````

на:

````tsx
              <FileHeader items={() => fileItems(ed())} />
              <PagesPanel open={pagesOpen()} onToggle={setPagesOpen} />
              <LayersPanel />
              <ResizeEdge side="left" width={left()} onWidth={setLeft} />
````

В `apps/web/src/main.tsx` заменить:

````tsx
import './ui/styles/chrome.css';
import { render } from 'solid-js/web';
````

на:

````tsx
import './ui/styles/chrome.css';
import './ui/styles/layers.css';
import { render } from 'solid-js/web';
````

Run: `cd apps/web && npm run typecheck && npm run build && npx playwright test e2e/layers.spec.ts`
Expected: PASS, `6 passed`.

- [ ] **Step 3: Закоммитить**

```bash
git add apps/web
git commit -m "feat(web): виртуальное дерево слоёв и список страниц"
```

---

### Task 9: Панель свойств: поле числа, положение, вид, экспорт

Панель свойств показывает сводку выделения, которую прислал worker, и шлёт правки командами. Секции по порядку дизайн-системы: «Фрейм» (у одного фрейма — выбор пресета размера; ширина и высота одним шагом) или «Положение» — X, Y, W, H, поворот, кнопки «Отразить по горизонтали / по вертикали»; «Вид» — прозрачность (проценты), радиус скругления (у фреймов, прямоугольников, изображений), «Обрезать содержимое» (у фреймов; у «Смешанных» — `indeterminate`); «Экспорт» — 1×, 2×, 3×, PNG, «Экспортировать «Имя»» (без выделения — страницу). Файл экспорта скачивается, уведомление — «Экспортировано: имя». Во время жеста на холсте поля одного узла показывают живое состояние из общей памяти (фаза measure), а не события.

**Поле числа** (NumberField дизайн-системы, `number.ts` — чистая математика): выражения (`120+16`, `2*(3+4)`, запятая — десятичный разделитель), проценты (`50%` — от текущего значения, у прозрачности — от 100), начатое с `+ * /` продолжает текущее (`*2`), у «Смешанных» `+N`/`-N` — прибавка к каждому узлу. Enter применяет, Esc откатывает, стрелки — шаг 1 (Shift — 10), каждый — шаг отмены. Перетаскивание подписи — шаг за пиксель, Shift ×10, Alt ×0,1: превью, затем один шаг при отпускании, Esc во время жеста — отмена. Поле не применяет текст повторно: набранное отмечается флагом, а значение из сводки его не затирает; текст читается до снятия флага — иначе эффект Solid вернул бы в поле прежнее значение раньше, чем его прочтут.

**Files:**
- Create: `apps/web/src/ui/number.ts`
- Test: `apps/web/unit/number.test.ts`
- Create: `apps/web/src/ui/number-field.tsx`
- Create: `apps/web/src/ui/properties.tsx`
- Create: `apps/web/src/ui/files.ts`
- Create: `apps/web/src/ui/styles/properties.css`
- Modify: `apps/web/src/ui/app.tsx`
- Modify: `apps/web/src/main.tsx`
- Test: `apps/web/e2e/properties.spec.ts`

**Interfaces:**
- Consumes: `store.state.{summary, live, presets}`, команды `editSelection`, `export`, `execute` (Task 5), `Section`, `Menu` (Task 7), `core.costed`.
- Produces: `parseFieldInput(text, { current, mixed, percentOf }) → FieldInput | null`, `FieldInput { set | add }`, `scrubValue`, `scrubFactor`, `arrowStep`, `fieldKey`, `formatNumber`, `clamp`; `NumberField`; `PropertiesPanel`, `sendEdit`; `downloadBytes(name, bytes, type)`.

- [ ] **Step 1: Написать падающий тест математики поля**

`apps/web/unit/number.test.ts`:

````ts
// Поле числа: выражения, проценты, «Смешанные», шаг перетаскивания подписи с
// Shift ×10 и Alt ×0.1, Enter и Esc, формат.

import { expect, test } from '@playwright/test';
import { arrowStep, fieldKey, formatNumber, parseFieldInput, scrubValue } from '../src/ui/number';

const set = (value: number) => ({ kind: 'set', value });

test('выражения', () => {
  const p = (t: string, current = 100) => parseFieldInput(t, { current });
  expect(p('120+16')).toEqual(set(136));
  expect(p('2*(3+4)')).toEqual(set(14));
  expect(p('10 - 2 * 3')).toEqual(set(4));
  expect(p('-5')).toEqual(set(-5));
  // Запятая — тоже десятичный разделитель.
  expect(p('12,5')).toEqual(set(12.5));
  // Начатое с оператора продолжает текущее значение.
  expect(p('*2', 40)).toEqual(set(80));
  expect(p('/4', 40)).toEqual(set(10));
  expect(p('+16', 40)).toEqual(set(56));
  expect(p('1e3')).toEqual(set(1000));
});

test('проценты: от текущего или от заданной базы', () => {
  expect(parseFieldInput('50%', { current: 300 })).toEqual(set(150));
  expect(parseFieldInput('50%', { current: 80, percentOf: 100 })).toEqual(set(50));
  expect(parseFieldInput('100%+10', { current: 40 })).toEqual(set(50));
});

test('у «Смешанных» +N и -N — прибавка к каждому', () => {
  expect(parseFieldInput('+10', { current: 5, mixed: true })).toEqual({ kind: 'add', delta: 10 });
  expect(parseFieldInput('-2.5', { current: 5, mixed: true })).toEqual({ kind: 'add', delta: -2.5 });
  expect(parseFieldInput('20', { current: 5, mixed: true })).toEqual(set(20));
});

test('не число — null', () => {
  for (const bad of ['', '  ', 'abc', '1+', '(1', '1)', '2**3', '1/0', 'Смешанные']) {
    expect(parseFieldInput(bad, { current: 1 }), bad).toBeNull();
  }
});

test('перетаскивание подписи: шаг, Shift ×10, Alt ×0.1', () => {
  expect(scrubValue(100, 7, 1, { shift: false, alt: false })).toBe(107);
  expect(scrubValue(100, 7, 1, { shift: true, alt: false })).toBe(170);
  expect(scrubValue(100, 7, 1, { shift: false, alt: true })).toBe(100.7);
  expect(scrubValue(0.1, 2, 0.1, { shift: false, alt: false })).toBe(0.3);
  expect(scrubValue(10, -3.4, 1, { shift: false, alt: false })).toBe(7);
});

test('клавиши поля: Enter применяет, Esc откатывает, стрелки — шаг', () => {
  expect(fieldKey('Enter')).toBe('commit');
  expect(fieldKey('NumpadEnter')).toBe('commit');
  expect(fieldKey('Escape')).toBe('revert');
  expect(fieldKey('a')).toBeNull();
  expect(arrowStep('ArrowUp', false)).toBe(1);
  expect(arrowStep('ArrowDown', true)).toBe(-10);
  expect(arrowStep('Tab', false)).toBe(0);
});

test('формат числа', () => {
  expect(formatNumber(120)).toBe('120');
  expect(formatNumber(12.3456)).toBe('12.35');
  expect(formatNumber(-0.0001)).toBe('0');
  expect(formatNumber(1440, 0)).toBe('1440');
  expect(formatNumber(NaN)).toBe('');
});
````

Run: `cd apps/web && npm run unit`
Expected: FAIL — `Cannot find module '../src/ui/number'`.

- [ ] **Step 2: Математика поля**

`apps/web/src/ui/number.ts`:

````ts
// Математика поля числа (NumberField): выражения (`120+16`, `50%`, `*2`),
// шаг перетаскивания подписи (Shift ×10, Alt ×0.1), клавиши поля, формат.

/** Что ввёл пользователь: новое значение или прибавка к значению каждого узла. */
export type FieldInput = { kind: 'set'; value: number } | { kind: 'add'; delta: number };

type Tok = { t: 'num'; v: number } | { t: 'op'; v: string };

/** Токены выражения; запятая — тоже десятичный разделитель (русская раскладка). */
function tokenize(text: string): Tok[] | null {
  const out: Tok[] = [];
  const s = text.replace(/\s+/g, '').replace(/,/g, '.').replace(/[×х]/g, '*').replace(/[−–]/g, '-');
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    const m = /^(\d+\.?\d*|\.\d+)(e[+-]?\d+)?/i.exec(s.slice(i));
    if (m) {
      out.push({ t: 'num', v: Number(m[0]) });
      i += m[0].length;
    } else if ('+-*/()%'.includes(c)) {
      out.push({ t: 'op', v: c });
      i++;
    } else {
      return null;
    }
  }
  return out;
}

/** Разбор с приоритетами: `+ -` < `* /` < унарный минус < `%` < скобки. Процент —
 * доля от `percentOf` (у ширины — от текущей ширины, у прозрачности — само число). */
function parse(tokens: Tok[], percentOf: number): number | null {
  let i = 0;
  const peek = () => tokens[i];
  const op = (v: string) => peek()?.t === 'op' && peek().v === v;
  function primary(): number | null {
    if (op('(')) {
      i++;
      const v = sum();
      if (v === null || !op(')')) return null;
      i++;
      return v;
    }
    if (op('-')) {
      i++;
      const v = postfix();
      return v === null ? null : -v;
    }
    if (op('+')) {
      i++;
      return postfix();
    }
    const t = peek();
    if (t?.t !== 'num') return null;
    i++;
    return t.v;
  }
  function postfix(): number | null {
    let v = primary();
    while (v !== null && op('%')) {
      i++;
      v = (v / 100) * percentOf;
    }
    return v;
  }
  function product(): number | null {
    let v = postfix();
    while (v !== null && (op('*') || op('/'))) {
      const o = tokens[i++].v;
      const r = postfix();
      if (r === null) return null;
      v = o === '*' ? v * r : v / r;
    }
    return v;
  }
  function sum(): number | null {
    let v = product();
    while (v !== null && (op('+') || op('-'))) {
      const o = tokens[i++].v;
      const r = product();
      if (r === null) return null;
      v = o === '+' ? v + r : v - r;
    }
    return v;
  }
  const v = sum();
  return v !== null && i === tokens.length && Number.isFinite(v) ? v : null;
}

export interface ParseOptions {
  /** Текущее значение (у «Смешанных» — первого узла). */
  current: number;
  /** Значения у выделенных разные: «+10» прибавит 10 к каждому. */
  mixed?: boolean;
  /** От чего считать «%» (по умолчанию от текущего значения). */
  percentOf?: number;
}

/** Ввод поля. Выражение, начатое с `+ * /`, продолжает текущее значение (`*2` —
 * вдвое); у «Смешанных» `+N` и `-N` с пробелом нет — это прибавка к каждому узлу.
 * `null` — не число. */
export function parseFieldInput(text: string, opts: ParseOptions): FieldInput | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const tokens = tokenize(trimmed);
  if (!tokens || tokens.length === 0) return null;
  const percentOf = opts.percentOf ?? opts.current;
  const first = tokens[0];
  const continues = first.t === 'op' && (first.v === '*' || first.v === '/' || first.v === '+');
  if (opts.mixed && first.t === 'op' && (first.v === '+' || first.v === '-')) {
    const delta = parse(tokens, percentOf);
    return delta === null ? null : { kind: 'add', delta };
  }
  const value = parse(continues ? [{ t: 'num', v: opts.current }, ...tokens] : tokens, percentOf);
  return value === null ? null : { kind: 'set', value };
}

/** Модификаторы перетаскивания подписи. */
export interface ScrubMods {
  shift: boolean;
  alt: boolean;
}

/** Множитель шага: Shift — ×10, Alt — ×0.1 (дизайн-система, NumberField). */
export function scrubFactor(mods: ScrubMods): number {
  if (mods.shift) return 10;
  if (mods.alt) return 0.1;
  return 1;
}

/** Значение при перетаскивании подписи на `dx` пикселей от `start`: шаг за
 * пиксель, округление до точности шага (без хвостов вида 0.30000000000000004). */
export function scrubValue(start: number, dx: number, step: number, mods: ScrubMods): number {
  const k = step * scrubFactor(mods);
  const v = start + Math.round(dx) * k;
  const digits = Math.max(0, Math.ceil(-Math.log10(k)));
  return Number(v.toFixed(Math.min(digits, 6)));
}

/** Прибавка стрелкой в поле: 1, с Shift — 10. */
export function arrowStep(key: string, shift: boolean): number {
  const d = shift ? 10 : 1;
  if (key === 'ArrowUp') return d;
  if (key === 'ArrowDown') return -d;
  return 0;
}

/** Что делать полю по клавише: Enter применяет, Esc откатывает. */
export function fieldKey(key: string): 'commit' | 'revert' | null {
  if (key === 'Enter' || key === 'NumpadEnter') return 'commit';
  if (key === 'Escape') return 'revert';
  return null;
}

/** Число для поля: до `decimals` знаков, без лишних нулей, точка как разделитель. */
export function formatNumber(v: number, decimals = 2): string {
  if (!Number.isFinite(v)) return '';
  const r = Number(v.toFixed(decimals));
  return String(Object.is(r, -0) ? 0 : r);
}

export function clamp(v: number, min = -Infinity, max = Infinity): number {
  return Math.min(max, Math.max(min, v));
}
````

Run: `cd apps/web && npm run unit`
Expected: PASS, `18 passed`.

- [ ] **Step 3: Поле числа и панель свойств**

`apps/web/src/ui/number-field.tsx`:

````tsx
// Поле числа (NumberField дизайн-системы): подпись, которую можно тянуть мышью
// (шаг за пиксель, Shift ×10, Alt ×0.1), ввод выражений (`120+16`, `50%`),
// Enter применяет, Esc откатывает, стрелки — шаг 1 (Shift — 10). «Смешанные» —
// у выделенных разные значения. Перетаскивание подписи — жест: превью, затем
// один шаг отмены при отпускании; набранное значение — тоже один шаг.

import { createEffect, createSignal, type JSX } from 'solid-js';
import type { EditPhase } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import {
  arrowStep,
  clamp,
  fieldKey,
  formatNumber,
  parseFieldInput,
  scrubValue,
  type FieldInput,
} from './number';

export interface NumberFieldProps {
  /** Подпись (1–2 символа) или иконка. */
  label: JSX.Element;
  /** Название поля для чтения с экрана («Ширина»). */
  name: string;
  value: number;
  mixed?: boolean;
  /** Отображение: значение поля → текст (проценты, градусы). */
  scale?: number;
  suffix?: string;
  decimals?: number;
  step?: number;
  min?: number;
  max?: number;
  /** От чего считать «%» при вводе (в единицах поля). */
  percentOf?: number;
  disabled?: boolean;
  /** Правка: значение в единицах поля (`set`) или прибавка к каждому (`add`). */
  onEdit: (input: FieldInput, phase: EditPhase) => void;
  class?: string;
}

export function NumberField(props: NumberFieldProps) {
  const ed = useEditor();
  const scale = () => props.scale ?? 1;
  const shown = () => (props.mixed ? 'Смешанные' : `${formatNumber(props.value * scale(), props.decimals ?? 2)}${props.suffix ?? ''}`);
  const [focused, setFocused] = createSignal(false);
  // Пользователь набирает: текст поля его, значение из сводки его не затирает.
  const [dirty, setDirty] = createSignal(false);
  let input!: HTMLInputElement;
  createEffect(() => {
    const text = focused() && props.mixed ? '' : shown();
    if (!dirty()) input.value = text;
  });
  let scrub: { x: number; start: number; moved: boolean } | null = null;

  const toField = (v: number) => clamp(v, props.min, props.max) / scale();
  const emit = (inp: FieldInput, phase: EditPhase) => {
    if (inp.kind === 'set') props.onEdit({ kind: 'set', value: toField(inp.value) }, phase);
    else props.onEdit({ kind: 'add', delta: inp.delta / scale() }, phase);
  };

  const commitText = () => {
    if (!dirty()) return;
    // Текст — до снятия флага: эффект выше сразу вернёт в поле значение сводки.
    const text = input.value.replace(props.suffix ?? '\u0000', '');
    setDirty(false);
    const parsed = parseFieldInput(text, {
      current: props.value * scale(),
      mixed: props.mixed,
      percentOf: props.percentOf,
    });
    // Не число или то же значение — без правки, поле показывает прежнее.
    if (parsed && !(parsed.kind === 'set' && !props.mixed && toField(parsed.value) === props.value)) {
      emit(parsed, 'commit');
    }
    input.value = focused() && props.mixed ? '' : shown();
  };

  // Esc во время перетаскивания подписи — отмена жеста без следа в истории.
  const onEscape = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || !scrub) return;
    e.preventDefault();
    e.stopPropagation();
    if (scrub.moved) props.onEdit({ kind: 'add', delta: 0 }, 'cancel');
    scrub = null;
  };
  const endScrub = () => window.removeEventListener('keydown', onEscape, true);

  const onPointerDown = (e: PointerEvent) => {
    if (props.disabled || e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    scrub = { x: e.clientX, start: props.mixed ? 0 : props.value * scale(), moved: false };
    window.addEventListener('keydown', onEscape, true);
  };
  const onPointerMove = ed.core.costed((e: PointerEvent) => {
    if (!scrub) return;
    const dx = e.clientX - scrub.x;
    if (!scrub.moved && Math.abs(dx) < 1) return;
    scrub.moved = true;
    const v = scrubValue(scrub.start, dx, props.step ?? 1, { shift: e.shiftKey, alt: e.altKey });
    // У «Смешанных» перетаскивание прибавляет к каждому своё.
    emit(props.mixed ? { kind: 'add', delta: v } : { kind: 'set', value: v }, 'preview');
  });
  const onPointerUp = (e: PointerEvent) => {
    endScrub();
    const s = scrub;
    scrub = null;
    if (!s?.moved) return;
    const v = scrubValue(s.start, e.clientX - s.x, props.step ?? 1, { shift: e.shiftKey, alt: e.altKey });
    emit(props.mixed ? { kind: 'add', delta: v } : { kind: 'set', value: v }, 'commit');
  };

  return (
    <label class={`fe-field fe-num ${props.class ?? ''}`} classList={{ 'fe-field--disabled': props.disabled }}>
      <span
        class="fe-field__label"
        aria-hidden="true"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={() => {
          endScrub();
          if (scrub?.moved) props.onEdit({ kind: 'add', delta: 0 }, 'cancel');
          scrub = null;
        }}
      >
        {props.label}
      </span>
      <input
        ref={input}
        aria-label={props.name}
        classList={{ 'fe-num--mixed': props.mixed && !focused() }}
        placeholder={props.mixed ? 'Смешанные' : undefined}
        disabled={props.disabled}
        inputMode="decimal"
        spellcheck={false}
        onFocus={() => {
          setFocused(true);
          queueMicrotask(() => input.select());
        }}
        onInput={() => setDirty(true)}
        onBlur={() => {
          commitText();
          setFocused(false);
        }}
        onKeyDown={(e) => {
          const action = fieldKey(e.key);
          if (action === 'commit') {
            commitText();
            input.select();
          } else if (action === 'revert') {
            setDirty(false);
            input.blur();
          } else {
            const d = arrowStep(e.key, e.shiftKey);
            if (!d) return;
            emit(props.mixed ? { kind: 'add', delta: d } : { kind: 'set', value: props.value * scale() + d }, 'commit');
          }
          e.preventDefault();
        }}
      />
    </label>
  );
}
````

`apps/web/src/ui/files.ts`:

````ts
// Файлы в браузере (этап 5 — минимум): скачать байты как файл. Открытие,
// «Сохранить как…» и хранение — в files.ts дополняются дальше.

/** Скачивает байты файлом `name` (ссылка с `download`, адрес освобождается). */
export function downloadBytes(name: string, bytes: Uint8Array, type: string): void {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
````

`apps/web/src/ui/properties.tsx`:

````tsx
// Панель свойств («Дизайн»): сводку выделения готовит worker (значения и
// «Смешанные»), панель только показывает её и шлёт правки командами. Во время
// жеста на холсте значения «Положения» одного узла — из живого состояния (общая
// память, фаза measure), а не из событий. Секции по порядку дизайн-системы:
// Фрейм / Положение → Вид → Заливка → Обводка → Экспорт. Без выделения —
// страница (фон) и экспорт всей страницы.

import { createEffect, createSignal, For, Show } from 'solid-js';
import type { Edit, EditPhase, Field, Num } from '../wasm/ferrum_wasm';
import { Section } from './chrome';
import { useEditor } from './context';
import { Caret, Icon } from './icons';
import { Menu, type MenuItem } from './menu';
import { NumberField } from './number-field';
import type { FieldInput } from './number';

export function sendEdit(send: (c: { type: 'editSelection'; edit: Edit; phase: EditPhase }) => void, edit: Edit, phase: EditPhase) {
  send({ type: 'editSelection', edit, phase });
}

/** Правка числового поля командой (значение или прибавка к каждому). */
function numberEdit(field: Field, input: FieldInput): Edit {
  return input.kind === 'set'
    ? { type: 'number', field, value: input.value, relative: false }
    : { type: 'number', field, value: input.delta, relative: true };
}

export function PropertiesPanel() {
  const ed = useEditor();
  const s = () => ed.store.state.summary;
  const count = () => s()?.count ?? 0;
  const edit = (field: Field) => (input: FieldInput, phase: EditPhase) =>
    sendEdit(ed.send, numberEdit(field, input), phase);
  // Один узел во время жеста холста — живые значения из общей памяти.
  const live = () => ed.store.state.live;
  const liveOne = () => live().busy && live().count === 1 && count() === 1;
  const val = (n: Num | undefined, liveValue: number): { value: number; mixed: boolean } =>
    liveOne() ? { value: liveValue, mixed: false } : { value: n?.value ?? 0, mixed: n?.mixed ?? false };

  return (
    <div class="fe-props" id="panel-design" role="tabpanel" aria-labelledby="tab-design">
      <Show when={count() > 0}>
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
      </Show>
      <ExportSection />
    </div>
  );
}

function PositionSection(props: {
  edit: (f: Field) => (i: FieldInput, p: EditPhase) => void;
  val: (n: Num | undefined, live: number) => { value: number; mixed: boolean };
}) {
  const ed = useEditor();
  const s = () => ed.store.state.summary!;
  const live = () => ed.store.state.live;
  const frame = () => s().kind === 'frame';
  const [presets, setPresets] = createSignal<HTMLElement | null>(null);
  const presetItems = (): MenuItem[] => {
    const out: MenuItem[] = [];
    let group = '';
    for (const p of ed.store.state.presets) {
      if (p.group !== group) {
        group = p.group;
        if (out.length) out.push({ separator: true });
        out.push({ heading: group });
      }
      out.push({
        label: p.name,
        hint: `${p.width} × ${p.height}`,
        checked: s().w.value === p.width && s().h?.value === p.height,
        onSelect: () => sendEdit(ed.send, { type: 'size', w: p.width, h: p.height }, 'commit'),
      });
    }
    return out;
  };
  const presetName = () =>
    ed.store.state.presets.find((p) => p.width === s().w.value && p.height === s().h?.value && !s().w.mixed)?.name ?? 'Свой размер';
  const x = () => props.val(s().x, live().x);
  const y = () => props.val(s().y, live().y);
  const w = () => props.val(s().w, live().w);
  const h = () => props.val(s().h, live().h);
  const r = () => props.val(s().rotation, live().rotation);
  return (
    <Section title={frame() ? 'Фрейм' : 'Положение'}>
      <Show when={frame() && s().count === 1}>
        <button
          class="fe-select"
          aria-haspopup="menu"
          aria-expanded={!!presets()}
          aria-label="Пресет размера"
          onClick={(e) => setPresets(e.currentTarget)}
        >
          <span>{presetName()}</span>
          <Caret />
        </button>
        <Show when={presets()}>
          {(a) => <Menu label="Пресеты размера" items={presetItems()} anchor={a()} onClose={() => setPresets(null)} />}
        </Show>
      </Show>
      <div class="fe-grid2">
        <NumberField label="X" name="X" value={x().value} mixed={x().mixed} onEdit={props.edit('x')} />
        <NumberField label="Y" name="Y" value={y().value} mixed={y().mixed} onEdit={props.edit('y')} />
        <NumberField label="W" name="Ширина" value={w().value} mixed={w().mixed} min={0.01} onEdit={props.edit('w')} />
        <NumberField
          label="H"
          name="Высота"
          value={h().value}
          mixed={h().mixed}
          min={0.01}
          disabled={!s().h}
          onEdit={props.edit('h')}
        />
      </div>
      <div class="fe-row">
        <NumberField
          label={<Icon name="rotate" />}
          name="Поворот"
          value={r().value}
          mixed={r().mixed}
          suffix="°"
          onEdit={props.edit('rotation')}
          class="fe-row__grow"
        />
        <button
          class="fe-tool fe-tool--small"
          aria-label="Отразить по горизонтали"
          data-tip="Отразить по горизонтали"
          data-command="object.flip_horizontal"
          onClick={() => ed.run('object.flip_horizontal')}
        >
          <Icon name="flipH" />
        </button>
        <button
          class="fe-tool fe-tool--small"
          aria-label="Отразить по вертикали"
          data-tip="Отразить по вертикали"
          data-command="object.flip_vertical"
          onClick={() => ed.run('object.flip_vertical')}
        >
          <Icon name="flipV" />
        </button>
      </div>
    </Section>
  );
}

function AppearanceSection(props: { edit: (f: Field) => (i: FieldInput, p: EditPhase) => void }) {
  const ed = useEditor();
  const s = () => ed.store.state.summary!;
  return (
    <Section title="Вид">
      <div class="fe-grid2">
        <NumberField
          label={<Icon name="opacity" />}
          name="Прозрачность"
          value={s().opacity.value}
          mixed={s().opacity.mixed}
          scale={100}
          suffix="%"
          decimals={0}
          min={0}
          max={100}
          percentOf={100}
          onEdit={props.edit('opacity')}
        />
        <Show when={s().cornerRadius}>
          {(radius) => (
            <NumberField
              label={<Icon name="radius" />}
              name="Радиус скругления"
              value={radius().value}
              mixed={radius().mixed}
              min={0}
              onEdit={props.edit('cornerRadius')}
            />
          )}
        </Show>
      </div>
      <Show when={s().clipContent}>
        {(clip) => (
          <label class="fe-check">
            <input
              type="checkbox"
              checked={clip().value && !clip().mixed}
              ref={(el) => createEffect(() => (el.indeterminate = clip().mixed))}
              onChange={(e) => sendEdit(ed.send, { type: 'clipContent', value: e.currentTarget.checked }, 'commit')}
            />
            <span>Обрезать содержимое</span>
          </label>
        )}
      </Show>
    </Section>
  );
}

const SCALES = [1, 2, 3] as const;

function ExportSection() {
  const ed = useEditor();
  const [scale, setScale] = createSignal<number>(1);
  const target = () => {
    const s = ed.store.state.summary;
    if (!s || s.count === 0) return 'страницу';
    return s.count === 1 ? `«${s.name ?? ''}»` : 'выделение';
  };
  return (
    <Section title="Экспорт">
      <div class="fe-row">
        <div class="fe-segmented" role="radiogroup" aria-label="Масштаб экспорта">
          <For each={SCALES}>
            {(k) => (
              <button role="radio" aria-checked={scale() === k} class="fe-segmented__item t-num" onClick={() => setScale(k)}>
                {k}×
              </button>
            )}
          </For>
        </div>
        <span class="fe-format t-num" aria-label="Формат">
          PNG
        </span>
      </div>
      <button class="fe-btn fe-btn--secondary fe-btn--wide" onClick={() => ed.send({ type: 'export', scale: scale() })}>
        <Icon name="download" />
        <span class="fe-ellipsis">Экспортировать {target()}</span>
      </button>
    </Section>
  );
}
````

`apps/web/src/ui/styles/properties.css`:

````css
/* Панель свойств: секции, поля чисел (NumberField), переключатели. */

.fe-props {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
.fe-grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-2);
}
.fe-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}
.fe-row__grow {
  flex: 1;
}

.fe-field__label {
  width: 24px;
  height: 100%;
  flex: none;
  display: grid;
  place-items: center;
  color: var(--ink-muted);
  font-size: 11px;
  cursor: ew-resize;
  user-select: none;
  touch-action: none;
}
.fe-num input {
  padding-left: 0;
}
.fe-num--mixed {
  color: var(--ink-muted) !important;
  font-family: var(--font-sans) !important;
}
.fe-field--disabled {
  opacity: 0.5;
}
.fe-field--disabled .fe-field__label {
  cursor: default;
}

/* Выпадающий выбор (пресеты размера, выравнивание обводки). */
.fe-select {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  height: var(--row-height);
  padding: 0 var(--space-2);
  border: 0;
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
  color: var(--ink);
  cursor: pointer;
  min-width: 0;
}
.fe-select:hover {
  box-shadow: inset 0 0 0 1px var(--line-control);
}

.fe-check {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-height: 24px;
  cursor: pointer;
}
.fe-check input {
  width: 14px;
  height: 14px;
  margin: 0;
  accent-color: var(--select);
}

.fe-segmented {
  display: inline-flex;
  padding: 2px;
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
}
.fe-segmented__item {
  height: 24px;
  min-width: 32px;
  padding: 0 var(--space-2);
  border: 0;
  border-radius: 3px;
  background: transparent;
  color: var(--ink-muted);
  cursor: pointer;
}
.fe-segmented__item[aria-checked='true'] {
  background: var(--surface);
  color: var(--ink);
  box-shadow: 0 0 0 1px var(--line);
}
.fe-format {
  color: var(--ink-muted);
}
.fe-btn--wide {
  width: 100%;
  justify-content: center;
}
.fe-ellipsis {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { PagesPanel } from './pages';
import { createUiStore } from './store';
````

на:

````tsx
import { PagesPanel } from './pages';
import { PropertiesPanel } from './properties';
import { downloadBytes } from './files';
import { createUiStore } from './store';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        canvas.focus();
      })
````

на:

````tsx
        canvas.focus();
        ed.core.on((e) => {
          if (e.type === 'exported') {
            downloadBytes(e.name, e.bytes, 'image/png');
            store.toast(`Экспортировано: ${e.name}`);
          }
        });
      })
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
              <DesignHeader />
              <ResizeEdge side="right" width={right()} onWidth={setRight} />
````

на:

````tsx
              <DesignHeader />
              <PropertiesPanel />
              <ResizeEdge side="right" width={right()} onWidth={setRight} />
````

В `apps/web/src/main.tsx` заменить:

````tsx
import './ui/styles/layers.css';
import { render } from 'solid-js/web';
````

на:

````tsx
import './ui/styles/layers.css';
import './ui/styles/properties.css';
import { render } from 'solid-js/web';
````

`apps/web/e2e/properties.spec.ts`:

````ts
// Панель свойств: правка X и отмена, выражения, перетаскивание подписи — превью
// и один шаг, Esc, «Смешанные», живые значения во время жеста на холсте,
// пресет фрейма, экспорт PNG.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

const field = (page: Page, name: string) => page.getByRole('tabpanel').getByRole('textbox', { name, exact: true });
const summary = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)));
const selectRow = (page: Page, name: string, modifiers: ('Shift' | 'ControlOrMeta')[] = []) =>
  page.getByRole('tree', { name: 'Слои' }).getByRole('treeitem', { name, exact: true }).click({ modifiers });

test('X из панели — один шаг, отмена возвращает', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const x = field(page, 'X');
  await expect(x).toHaveValue('14400');
  const before = await live(page);
  await x.fill('100');
  await x.press('Enter');
  await expect.poll(async () => (await summary(page)).x.value).toBe(100);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(x).toHaveValue('14400');
  // Выражение и проценты.
  const w = field(page, 'Ширина');
  await w.fill('1440/2+16');
  await w.press('Enter');
  await expect(w).toHaveValue('736');
  await w.fill('50%');
  await w.press('Enter');
  await expect(w).toHaveValue('368');
  // Esc откатывает набранное.
  await w.fill('9999');
  await w.press('Escape');
  await expect(w).toHaveValue('368');
});

test('перетаскивание подписи: превью, затем один шаг; Shift ×10', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const label = page.getByRole('tabpanel').locator('.fe-field__label', { hasText: /^X$/ });
  const box = (await label.boundingBox())!;
  const before = await live(page);
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 25, box.y + 5, { steps: 5 });
  // Во время жеста документ не меняется: шагов истории нет.
  expect((await live(page)).steps).toBe(before.steps);
  await page.mouse.up();
  await expect.poll(async () => (await summary(page)).x.value).toBe(14420);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await page.keyboard.down('Shift');
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 8, box.y + 5, { steps: 3 });
  await page.mouse.up();
  await page.keyboard.up('Shift');
  await expect.poll(async () => (await summary(page)).x.value).toBe(14450);
  // Esc во время жеста — без следа.
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 40, box.y + 5, { steps: 3 });
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await page.waitForTimeout(200);
  expect((await summary(page)).x.value).toBe(14450);
  expect((await live(page)).steps).toBe(before.steps + 2);
});

test('«Смешанные» и прибавка к каждому', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  await selectRow(page, 'Экран 8', ['Shift']);
  const x = field(page, 'X');
  await expect(x).toHaveValue('Смешанные');
  await expect(field(page, 'Y')).toHaveValue('0');
  await x.fill('+10');
  await x.press('Enter');
  await selectRow(page, 'Экран 8');
  await expect(x).toHaveValue('12810');
  await selectRow(page, 'Экран 9');
  await expect(x).toHaveValue('14410');
});

test('во время перетаскивания на холсте поля показывают живые значения', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  const x = field(page, 'X');
  await expect(x).toHaveValue('140');
  await page.mouse.down();
  await page.mouse.move(at.x + 30, at.y, { steps: 6 });
  // Документ ещё не изменён, а поле уже показывает новое положение.
  await expect(x).toHaveValue('200');
  expect((await live(page)).steps).toBe(l.steps);
  await page.mouse.up();
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expect(x).toHaveValue('200');
});

test('пресет фрейма — размер одним шагом', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  await page.getByRole('button', { name: 'Пресет размера' }).click();
  await page.getByRole('menu', { name: 'Пресеты размера' }).getByRole('menuitemcheckbox', { name: /iPhone 16 Pro Max/ }).click();
  await expect(field(page, 'Ширина')).toHaveValue('440');
  await expect(field(page, 'Высота')).toHaveValue('956');
  expect((await live(page)).steps).toBe(1);
});

test('экспорт PNG выделения и страницы', async ({ page }) => {
  await openApp(page, 100);
  const button = page.getByRole('button', { name: /Экспортировать/ });
  await expect(button).toHaveText('Экспортировать страницу');
  let download = page.waitForEvent('download');
  await button.click();
  expect((await download).suggestedFilename()).toBe('Страница 1@1x.png');
  await selectRow(page, 'Экран 0');
  await page.getByRole('radio', { name: '2×' }).click();
  download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Экспортировать «Экран 0»' }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('Экран 0@2x.png');
  const bytes = await (await file.createReadStream()).toArray();
  const png = Buffer.concat(bytes);
  // IHDR: 1440 × 1024 при 2× — 2880 × 2048.
  expect([png.readUInt32BE(16), png.readUInt32BE(20)]).toEqual([2880, 2048]);
  await expect(page.getByRole('status').filter({ hasText: 'Экспортировано: Экран 0@2x.png' })).toBeVisible();
});
````

Run: `cd apps/web && npm run typecheck && npm run build && npx playwright test e2e/properties.spec.ts`
Expected: PASS, `6 passed`.

- [ ] **Step 4: Закоммитить**

```bash
git add apps/web
git commit -m "feat(web): панель свойств — поле числа, положение, вид, экспорт PNG"
```

---

### Task 10: Палитра цвета, заливки, обводки, фон страницы

**Палитра** (`picker.tsx`): область HSB (насыщенность × яркость), полосы тона и прозрачности, HEX, прозрачность в процентах, пипетка — через `EyeDropper`, где он есть (иначе кнопки нет). Палитра держит тон сама: у серого он не теряется. Движение внутри палитры — жест: превью, один шаг при отпускании; щелчок раньше первого замера тоже применяется по точке отпускания; Esc, потеря указателя или закрытие палитры во время жеста — цвет на начало жеста, без шага. Стрелки на области и полосах — шаг 1 % (Shift — 10 %), тон — 1°. HEX и % применяются по Enter и уходу фокуса — только если значение изменилось. Фокус ушёл из палитры — она закрывается.

**Заливка и обводка** (`paints.tsx`): список из сводки, верхняя заливка — первой; свотч (цвет пользователя на шахматке), HEX, прозрачность, «Скрыть», «Удалить»; «+» добавляет (у «Смешанных» — заменяет списком из одной). Заливка изображением показывается («Изображение · Заполнить»), но не правится. Обводка: цвет, толщина (поле числа), выравнивание (внутри, по центру, снаружи). Панель показывает HEX без альфы и одну прозрачность (альфа цвета × прозрачность заливки), правка пишет альфу 255 и прозрачность заливки — как в Figma. Цвета — `colour.ts`: HEX ↔ RGB ↔ HSB без потерь (тест проходит по сетке 14 976 цветов).

**Фон страницы** (без выделения, раздел «Страница»): свотч и HEX, «Фон как в теме» снимает свой. Жест палитры — превью только в кадре ядра (`setPageBackground` с фазой `preview`), в документ — при отпускании: сотни записей в документ за один жест не нужны, а фон страницы в историю не попадает.

**Files:**
- Create: `apps/web/src/ui/colour.ts`
- Test: `apps/web/unit/colour.test.ts`
- Create: `apps/web/src/ui/picker.tsx`
- Create: `apps/web/src/ui/paints.tsx`
- Modify: `apps/web/src/ui/properties.tsx`
- Modify: `apps/web/src/ui/styles/properties.css`
- Modify: `apps/web/src/ui/app.tsx`
- Test: `apps/web/e2e/picker.spec.ts`
- Modify: `apps/web/e2e/layers.spec.ts`

**Interfaces:**
- Consumes: `store.state.{summary.fills, summary.strokes, pages.background}`, команды `editSelection` (`fills`, `strokes`), `setPageBackground` (Task 5); `NumberField`, `sendEdit` (Task 9).
- Produces: `parseHex`, `toHex`, `toHex6`, `rgbToHsb`, `hsbToRgb`, `cssColor`, `paintOpacity`, `Rgb`, `Rgba`, `Hsb`; `ColorPicker`, `hasEyeDropper`; `FillsSection`, `StrokesSection`, `PageSection`, `THEME_CANVAS`. `PropertiesPanel` принимает тему (для цвета холста в разделе «Страница»).

- [ ] **Step 1: Написать падающий тест цвета**

`apps/web/unit/colour.test.ts`:

````ts
// Цвет: HEX ↔ RGB ↔ HSB и обратно без потерь, разбор HEX из поля.

import { expect, test } from '@playwright/test';
import { hsbToRgb, paintOpacity, parseHex, rgbToHsb, toHex, toHex6 } from '../src/ui/colour';

test('разбор HEX во всех записях', () => {
  expect(parseHex('#F2814A')).toEqual({ r: 242, g: 129, b: 74, a: 255 });
  expect(parseHex('f2814a80')).toEqual({ r: 242, g: 129, b: 74, a: 128 });
  expect(parseHex(' #f84 ')).toEqual({ r: 255, g: 136, b: 68, a: 255 });
  expect(parseHex('f848')).toEqual({ r: 255, g: 136, b: 68, a: 136 });
  for (const bad of ['', '#', 'ggg', '12345', '#1234567', 'красный']) expect(parseHex(bad), bad).toBeNull();
});

test('HEX документа и поля', () => {
  expect(toHex({ r: 242, g: 129, b: 74, a: 255 })).toBe('#f2814aff');
  expect(toHex6({ r: 1, g: 2, b: 255 })).toBe('0102FF');
  expect(toHex({ r: 300, g: -4, b: 12.6, a: 0 })).toBe('#ff000d00');
});

test('RGB → HSB: опорные цвета', () => {
  expect(rgbToHsb({ r: 255, g: 0, b: 0 })).toEqual({ h: 0, s: 1, b: 1 });
  expect(rgbToHsb({ r: 0, g: 255, b: 0 })).toEqual({ h: 120, s: 1, b: 1 });
  expect(rgbToHsb({ r: 0, g: 0, b: 255 })).toEqual({ h: 240, s: 1, b: 1 });
  expect(rgbToHsb({ r: 0, g: 0, b: 0 })).toEqual({ h: 0, s: 0, b: 0 });
  expect(rgbToHsb({ r: 255, g: 255, b: 255 })).toEqual({ h: 0, s: 0, b: 1 });
  const g = rgbToHsb({ r: 128, g: 128, b: 128 });
  expect([g.h, g.s]).toEqual([0, 0]);
  expect(hsbToRgb({ h: 360, s: 1, b: 1 })).toEqual({ r: 255, g: 0, b: 0 });
  expect(hsbToRgb({ h: -60, s: 1, b: 1 })).toEqual({ r: 255, g: 0, b: 255 });
});

test('RGB → HSB → RGB и HEX → RGB → HEX без потерь', () => {
  let n = 0;
  const lost: string[] = [];
  for (let r = 0; r <= 255; r += 15) {
    for (let g = 0; g <= 255; g += 17) {
      for (let b = 0; b <= 255; b += 5) {
        const c = { r, g, b };
        const back = hsbToRgb(rgbToHsb(c));
        const hex = toHex({ ...c, a: 255 });
        if (back.r !== r || back.g !== g || back.b !== b || toHex(parseHex(hex)!) !== hex) lost.push(hex);
        n++;
      }
    }
  }
  expect(lost).toEqual([]);
  expect(n).toBe(18 * 16 * 52);
});

test('прозрачность в панели — альфа цвета × прозрачность заливки', () => {
  expect(paintOpacity({ r: 0, g: 0, b: 0, a: 255 }, 0.5)).toBe(0.5);
  expect(paintOpacity({ r: 0, g: 0, b: 0, a: 51 }, 1)).toBeCloseTo(0.2);
});
````

Run: `cd apps/web && npm run unit`
Expected: FAIL — `Cannot find module '../src/ui/colour'`.

- [ ] **Step 2: Цвет: HEX, RGB, HSB**

`apps/web/src/ui/colour.ts`:

````ts
// Цвет для палитры: HEX ↔ RGB ↔ HSB. В документе цвет — `#rrggbbaa`, у заливки
// ещё своя прозрачность; панель показывает HEX без альфы и одну прозрачность
// (альфа цвета × прозрачность заливки), как в Figma.

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export interface Rgba extends Rgb {
  /** 0–255. */
  a: number;
}

/** Тон 0–360, насыщенность и яркость 0–1. */
export interface Hsb {
  h: number;
  s: number;
  b: number;
}

const byte = (v: number) => Math.min(255, Math.max(0, Math.round(v)));
const hex2 = (v: number) => byte(v).toString(16).padStart(2, '0');

/** Разбор HEX: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, с решёткой и без. */
export function parseHex(text: string): Rgba | null {
  let h = text.trim().replace(/^#/, '');
  if (!/^[0-9a-f]+$/i.test(h)) return null;
  if (h.length === 3 || h.length === 4) h = [...h].map((c) => c + c).join('');
  if (h.length !== 6 && h.length !== 8) return null;
  const n = (i: number) => parseInt(h.slice(i, i + 2), 16);
  return { r: n(0), g: n(2), b: n(4), a: h.length === 8 ? n(6) : 255 };
}

/** `#rrggbbaa` для документа. */
export function toHex(c: Rgba): string {
  return `#${hex2(c.r)}${hex2(c.g)}${hex2(c.b)}${hex2(c.a)}`;
}

/** HEX без альфы для поля панели: `F2814A`. */
export function toHex6(c: Rgb): string {
  return `${hex2(c.r)}${hex2(c.g)}${hex2(c.b)}`.toUpperCase();
}

export function rgbToHsb({ r, g, b }: Rgb): Hsb {
  const R = r / 255;
  const G = g / 255;
  const B = b / 255;
  const max = Math.max(R, G, B);
  const min = Math.min(R, G, B);
  const d = max - min;
  let h = 0;
  if (d > 0) {
    if (max === R) h = ((G - B) / d) % 6;
    else if (max === G) h = (B - R) / d + 2;
    else h = (R - G) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  return { h, s: max === 0 ? 0 : d / max, b: max };
}

export function hsbToRgb({ h, s, b }: Hsb): Rgb {
  const hh = (((h % 360) + 360) % 360) / 60;
  const c = b * s;
  const x = c * (1 - Math.abs((hh % 2) - 1));
  const m = b - c;
  const [r, g, bl] =
    hh < 1 ? [c, x, 0] : hh < 2 ? [x, c, 0] : hh < 3 ? [0, c, x] : hh < 4 ? [0, x, c] : hh < 5 ? [x, 0, c] : [c, 0, x];
  return { r: byte((r + m) * 255), g: byte((g + m) * 255), b: byte((bl + m) * 255) };
}

/** CSS-цвет для свотча. */
export function cssColor(c: Rgb, alpha = 1): string {
  return `rgb(${c.r} ${c.g} ${c.b} / ${alpha})`;
}

/** Прозрачность для панели: альфа цвета × прозрачность заливки, 0–1. */
export function paintOpacity(color: Rgba, opacity: number): number {
  return (color.a / 255) * opacity;
}
````

Run: `cd apps/web && npm run unit`
Expected: PASS, `23 passed`.

- [ ] **Step 3: Палитра, заливки, обводки, фон страницы**

У кнопки свотча фона страницы имя «Фон страницы» — тест страниц ищет теперь переключатель «Страницы» точным именем.

`apps/web/src/ui/picker.tsx`:

````tsx
// Палитра цвета: область HSB (насыщенность × яркость), полосы тона и
// прозрачности, HEX, прозрачность в процентах, пипетка (`EyeDropper`, где он
// есть; иначе кнопки нет). Движение внутри палитры — жест: превью, затем один шаг
// отмены при отпускании; Esc во время жеста — отмена. Палитра держит тон сама:
// у серого (насыщенность 0) тон не теряется.

import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import type { EditPhase } from '../wasm/ferrum_wasm';
import { cssColor, hsbToRgb, parseHex, rgbToHsb, toHex6, type Hsb, type Rgb } from './colour';
import { useEditor } from './context';
import { Icon } from './icons';
import { clamp, formatNumber, parseFieldInput } from './number';

export interface PickerProps {
  title: string;
  color: Rgb;
  /** 0–1. */
  alpha: number;
  anchor: HTMLElement;
  onChange: (color: Rgb, alpha: number, phase: EditPhase) => void;
  onClose: () => void;
}

type Area = 'sb' | 'hue' | 'alpha';

interface EyeDropperApi {
  open(): Promise<{ sRGBHex: string }>;
}

export function hasEyeDropper(): boolean {
  return typeof (window as unknown as { EyeDropper?: unknown }).EyeDropper === 'function';
}

export function ColorPicker(props: PickerProps) {
  const ed = useEditor();
  const [hsb, setHsb] = createSignal<Hsb>(rgbToHsb(props.color));
  const [alpha, setAlpha] = createSignal(props.alpha);
  let el!: HTMLDivElement;
  let drag: {
    area: Area;
    target: HTMLElement;
    rect: DOMRect | null;
    start: { hsb: Hsb; alpha: number };
  } | null = null;
  const returnFocus = document.activeElement as HTMLElement | null;

  const rgb = () => hsbToRgb(hsb());
  const emit = (phase: EditPhase) => props.onChange(rgb(), alpha(), phase);

  const apply = (area: Area, rect: DOMRect, x: number, y: number) => {
    const fx = clamp((x - rect.left) / rect.width, 0, 1);
    const fy = clamp((y - rect.top) / rect.height, 0, 1);
    if (area === 'sb') setHsb({ h: hsb().h, s: fx, b: 1 - fy });
    else if (area === 'hue') setHsb({ ...hsb(), h: fx * 360 });
    else setAlpha(Math.round(fx * 100) / 100);
  };

  const down = (area: Area) => (e: PointerEvent) => {
    if (e.button !== 0) return;
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    target.focus();
    const at = { x: e.clientX, y: e.clientY };
    drag = { area, target, rect: null, start: { hsb: hsb(), alpha: alpha() } };
    ed.scheduler.measure(() => {
      if (!drag) return;
      drag.rect = target.getBoundingClientRect();
      const rect = drag.rect;
      ed.scheduler.mutate(() => {
        apply(area, rect, at.x, at.y);
        emit('preview');
      });
    });
  };
  const move = ed.core.costed((e: PointerEvent) => {
    if (!drag?.rect) return;
    apply(drag.area, drag.rect, e.clientX, e.clientY);
    emit('preview');
  });
  const up = (e: PointerEvent) => {
    const d = drag;
    drag = null;
    if (!d) return;
    if (d.rect) {
      emit('commit');
      return;
    }
    // Отпустили раньше первого замера (быстрый щелчок): значение — по точке
    // отпускания, и сразу запись.
    const at = { x: e.clientX, y: e.clientY };
    ed.scheduler.measure(() => {
      const rect = d.target.getBoundingClientRect();
      ed.scheduler.mutate(() => {
        apply(d.area, rect, at.x, at.y);
        emit('commit');
      });
    });
  };

  /** Жест прерван (потеря указателя, закрытие палитры): цвет на начало жеста. */
  const cancelDrag = () => {
    const d = drag;
    drag = null;
    if (!d) return;
    setHsb(d.start.hsb);
    setAlpha(d.start.alpha);
    emit('cancel');
  };
  onCleanup(cancelDrag);

  /** Стрелки на области и полосах: шаг 1 % (Shift — 10 %), тон — 1° (10°). */
  const key = (area: Area) => (e: KeyboardEvent) => {
    const k = e.shiftKey ? 10 : 1;
    const dx = e.key === 'ArrowRight' ? k : e.key === 'ArrowLeft' ? -k : 0;
    const dy = e.key === 'ArrowUp' ? k : e.key === 'ArrowDown' ? -k : 0;
    if (!dx && !dy) return;
    e.preventDefault();
    const c = hsb();
    if (area === 'sb') setHsb({ h: c.h, s: clamp(c.s + dx / 100, 0, 1), b: clamp(c.b + dy / 100, 0, 1) });
    else if (area === 'hue') setHsb({ ...c, h: clamp(c.h + dx + dy, 0, 360) });
    else setAlpha(clamp(Math.round(alpha() * 100 + dx + dy) / 100, 0, 1));
    emit('commit');
  };

  const close = () => {
    props.onClose();
    returnFocus?.focus?.();
  };

  onMount(() => {
    ed.scheduler.measure(() => {
      const r = props.anchor.getBoundingClientRect();
      const box = el.getBoundingClientRect();
      const x = Math.max(8, Math.min(r.left - box.width - 8, innerWidth - box.width - 8));
      const y = Math.max(8, Math.min(r.top - 40, innerHeight - box.height - 8));
      ed.scheduler.mutate(() => {
        el.style.left = `${x}px`;
        el.style.top = `${y}px`;
        el.style.visibility = 'visible';
        el.querySelector<HTMLElement>('.fe-picker__sb')?.focus();
      });
    });
    const outside = (e: PointerEvent) => {
      if (!el.contains(e.target as Node) && !props.anchor.contains(e.target as Node)) props.onClose();
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopPropagation();
      // Esc во время жеста: цвет на начало жеста, без шага истории.
      if (drag) cancelDrag();
      else close();
    };
    // Фокус ушёл из палитры (Tab) — палитра закрывается.
    const leave = (e: FocusEvent) => {
      const t = e.target as Node;
      if (!el.contains(t) && !props.anchor.contains(t)) props.onClose();
    };
    const t = setTimeout(() => document.addEventListener('pointerdown', outside, true));
    window.addEventListener('keydown', esc, true);
    document.addEventListener('focusin', leave);
    onCleanup(() => {
      clearTimeout(t);
      document.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('keydown', esc, true);
      document.removeEventListener('focusin', leave);
    });
  });

  const pick = async () => {
    const Ctor = (window as unknown as { EyeDropper: new () => EyeDropperApi }).EyeDropper;
    try {
      const res = await new Ctor().open();
      const c = parseHex(res.sRGBHex);
      if (c) {
        setHsb(rgbToHsb(c));
        emit('commit');
      }
    } catch {
      // Пользователь отменил пипетку (Esc) — ничего не меняем.
    }
  };

  // Поля HEX и % применяются по Enter и при уходе фокуса — только если значение
  // изменилось: Enter и затем уход фокуса — одна правка, а не две.
  const hexInput = (text: string) => {
    const c = parseHex(text);
    if (!c) return;
    const a = text.replace('#', '').trim().length === 8 ? c.a / 255 : alpha();
    if (toHex6(c) === toHex6(rgb()) && a === alpha()) return;
    setHsb(rgbToHsb(c));
    setAlpha(a);
    emit('commit');
  };

  const opacityInput = (text: string) => {
    const p = parseFieldInput(text.replace('%', ''), { current: alpha() * 100, percentOf: 100 });
    if (p?.kind !== 'set') return;
    const a = clamp(p.value, 0, 100) / 100;
    if (Math.round(a * 100) === Math.round(alpha() * 100)) return;
    setAlpha(a);
    emit('commit');
  };

  const pure = () => cssColor(hsbToRgb({ h: hsb().h, s: 1, b: 1 }));

  return (
    <Portal>
      <div ref={el} class="fe-picker" role="dialog" aria-label={props.title} style={{ visibility: 'hidden' }}>
        <div class="fe-picker__head">
          <span class="t-strong">{props.title}</span>
          <button class="fe-tool fe-tool--small" aria-label="Закрыть палитру" onClick={close}>
            <Icon name="close" />
          </button>
        </div>
        <div
          class="fe-picker__sb"
          role="slider"
          tabindex="0"
          aria-label="Насыщенность и яркость"
          aria-valuetext={`насыщенность ${Math.round(hsb().s * 100)}%, яркость ${Math.round(hsb().b * 100)}%`}
          style={{ background: pure() }}
          onPointerDown={down('sb')}
          onPointerMove={move}
          onPointerUp={up}
          onPointerCancel={cancelDrag}
          onKeyDown={key('sb')}
        >
          <span
            class="fe-picker__thumb"
            style={{ left: `${hsb().s * 100}%`, top: `${(1 - hsb().b) * 100}%`, background: cssColor(rgb()) }}
          />
        </div>
        <div class="fe-picker__sliders">
          <Show when={hasEyeDropper()}>
            <button class="fe-tool fe-tool--small" aria-label="Пипетка" data-tip="Пипетка" onClick={pick}>
              <Icon name="pipette" />
            </button>
          </Show>
          <div class="fe-picker__bars">
            <div
              class="fe-picker__bar fe-picker__hue"
              role="slider"
              tabindex="0"
              aria-label="Тон"
              aria-valuemin={0}
              aria-valuemax={360}
              aria-valuenow={Math.round(hsb().h)}
              onPointerDown={down('hue')}
              onPointerMove={move}
              onPointerUp={up}
              onPointerCancel={cancelDrag}
              onKeyDown={key('hue')}
            >
              <span class="fe-picker__knob" style={{ left: `${(hsb().h / 360) * 100}%`, background: pure() }} />
            </div>
            <div
              class="fe-picker__bar fe-picker__alpha"
              role="slider"
              tabindex="0"
              aria-label="Прозрачность"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(alpha() * 100)}
              style={{ '--picker-color': cssColor(rgb()) }}
              onPointerDown={down('alpha')}
              onPointerMove={move}
              onPointerUp={up}
              onPointerCancel={cancelDrag}
              onKeyDown={key('alpha')}
            >
              <span class="fe-picker__knob" style={{ left: `${alpha() * 100}%`, background: cssColor(rgb(), alpha()) }} />
            </div>
          </div>
        </div>
        <div class="fe-row">
          <label class="fe-field fe-row__grow">
            <span class="fe-field__label">#</span>
            <input
              aria-label="HEX"
              value={toHex6(rgb())}
              spellcheck={false}
              onKeyDown={(e) => {
                if (e.key === 'Enter') hexInput(e.currentTarget.value);
              }}
              onBlur={(e) => hexInput(e.currentTarget.value)}
            />
          </label>
          <label class="fe-field fe-picker__opacity">
            <input
              aria-label="Прозрачность цвета"
              value={`${formatNumber(alpha() * 100, 0)}%`}
              onKeyDown={(e) => {
                if (e.key === 'Enter') opacityInput(e.currentTarget.value);
              }}
              onBlur={(e) => opacityInput(e.currentTarget.value)}
            />
          </label>
        </div>
      </div>
    </Portal>
  );
}
````

`apps/web/src/ui/paints.tsx`:

````tsx
// Секции «Заливка» и «Обводка» и фон страницы. Списки заливок и обводок
// приходят в сводке (у всех выделенных одинаковые — список, разные —
// «Смешанные»); любая правка шлёт новый список целиком (`editSelection` →
// `fills`/`strokes`), палитра — превью во время жеста и один шаг при отпускании.
// Заливка изображением показывается, но не правится (картинку меняет этап 6).

import { createSignal, For, Index, Show } from 'solid-js';
import type { Align, EditPhase, FillDto, StrokeDto } from '../wasm/ferrum_wasm';
import { Section } from './chrome';
import { cssColor, paintOpacity, parseHex, toHex, toHex6, type Rgb } from './colour';
import { useEditor } from './context';
import { Icon } from './icons';
import { formatNumber, parseFieldInput } from './number';
import { NumberField } from './number-field';
import { ColorPicker } from './picker';
import { sendEdit } from './properties';

/** Цвет новой заливки (как у новой фигуры) и обводки. */
const NEW_FILL: FillDto = { kind: 'solid', color: '#d9d9d9ff', opacity: 1, visible: true, imageHash: undefined, scaleMode: undefined };
const NEW_STROKE: StrokeDto = { color: '#000000ff', width: 1, align: 'inside' };

const ALIGNS: { value: Align; label: string }[] = [
  { value: 'inside', label: 'Внутри' },
  { value: 'center', label: 'По центру' },
  { value: 'outside', label: 'Снаружи' },
];

const SCALE_MODES = { fill: 'Заполнить', fit: 'Вписать', tile: 'Плитка' } as const;

/** Цвет из `#rrggbbaa` и прозрачность для панели. */
function shown(color: string, opacity: number): { rgb: Rgb; alpha: number } {
  const c = parseHex(color) ?? { r: 0, g: 0, b: 0, a: 255 };
  return { rgb: c, alpha: paintOpacity(c, opacity) };
}

/** Поле HEX: Enter или уход фокуса применяют, неверный текст возвращает прежний. */
function HexField(props: { rgb: Rgb; onSet: (rgb: Rgb) => void; name: string }) {
  const apply = (input: HTMLInputElement) => {
    const c = parseHex(input.value);
    if (c && toHex6(c) !== toHex6(props.rgb)) props.onSet(c);
    else input.value = toHex6(props.rgb);
  };
  return (
    <label class="fe-field fe-row__grow">
      <input
        aria-label={props.name}
        value={toHex6(props.rgb)}
        spellcheck={false}
        onKeyDown={(e) => {
          if (e.key === 'Enter') apply(e.currentTarget);
          if (e.key === 'Escape') {
            e.currentTarget.value = toHex6(props.rgb);
            e.currentTarget.blur();
          }
        }}
        onBlur={(e) => apply(e.currentTarget)}
      />
    </label>
  );
}

/** Поле прозрачности в процентах. */
function OpacityField(props: { value: number; onSet: (v: number) => void; name: string }) {
  const apply = (input: HTMLInputElement) => {
    const p = parseFieldInput(input.value.replace('%', ''), { current: props.value * 100, percentOf: 100 });
    // Уход фокуса без правки — не шаг отмены.
    const v = p?.kind === 'set' ? Math.min(100, Math.max(0, p.value)) / 100 : null;
    if (v !== null && Math.round(v * 100) !== Math.round(props.value * 100)) props.onSet(v);
    input.value = `${formatNumber(props.value * 100, 0)}%`;
  };
  return (
    <label class="fe-field fe-paint__opacity">
      <input
        aria-label={props.name}
        value={`${formatNumber(props.value * 100, 0)}%`}
        onKeyDown={(e) => e.key === 'Enter' && apply(e.currentTarget)}
        onBlur={(e) => apply(e.currentTarget)}
      />
    </label>
  );
}

export function FillsSection() {
  const ed = useEditor();
  const paints = () => ed.store.state.summary?.fills;
  const [picker, setPicker] = createSignal<{ index: number; anchor: HTMLElement } | null>(null);
  const list = () => (paints()?.list ?? []) as FillDto[];
  const send = (fills: FillDto[], phase: EditPhase) => sendEdit(ed.send, { type: 'fills', fills }, phase);
  const replace = (i: number, f: Partial<FillDto>, phase: EditPhase = 'commit') =>
    send(list().map((x, j) => (j === i ? { ...x, ...f } : { ...x })), phase);
  const remove = (i: number) => send(list().filter((_, j) => j !== i).map((x) => ({ ...x })), 'commit');
  return (
    <Show when={paints()}>
      {(p) => (
        <Section
          title="Заливка"
          muted={!p().mixed && p().list.length === 0}
          actions={
            <button
              class="fe-tool fe-tool--small"
              aria-label="Добавить заливку"
              data-tip="Добавить заливку"
              onClick={() => send(p().mixed ? [NEW_FILL] : [...list().map((x) => ({ ...x })), NEW_FILL], 'commit')}
            >
              <Icon name="plus" />
            </button>
          }
        >
          <Show when={p().mixed}>
            <p class="fe-paint__mixed">Смешанные заливки — нажмите «+», чтобы заменить</p>
          </Show>
          {/* Верхняя заливка — первой, как в Figma: список документа идёт снизу вверх. */}
          <Index each={[...list()].map((_, i) => list().length - 1 - i)}>
            {(index) => {
              const fill = () => list()[index()];
              const c = () => shown(fill().color, fill().opacity);
              return (
                <div class="fe-paint" classList={{ 'fe-paint--hidden': !fill().visible }}>
                  <Show
                    when={fill().kind === 'solid'}
                    fallback={
                      <>
                        <span class="fe-swatch fe-swatch--image" aria-hidden="true">
                          <Icon name="image" />
                        </span>
                        <span class="fe-row__grow fe-paint__label">
                          Изображение · {SCALE_MODES[fill().scaleMode ?? 'fill']}
                        </span>
                      </>
                    }
                  >
                    <button
                      class="fe-swatch"
                      aria-label="Цвет заливки"
                      aria-haspopup="dialog"
                      style={{ '--swatch': cssColor(c().rgb, c().alpha) }}
                      onClick={(e) => setPicker({ index: index(), anchor: e.currentTarget })}
                    />
                    <HexField
                      name="HEX заливки"
                      rgb={c().rgb}
                      onSet={(rgb) => replace(index(), { color: toHex({ ...rgb, a: 255 }), opacity: c().alpha })}
                    />
                    <OpacityField
                      name="Прозрачность заливки"
                      value={c().alpha}
                      onSet={(v) => replace(index(), { color: toHex({ ...c().rgb, a: 255 }), opacity: v })}
                    />
                  </Show>
                  <button
                    class="fe-tool fe-tool--small fe-tool--muted"
                    aria-label={fill().visible ? 'Скрыть заливку' : 'Показать заливку'}
                    aria-pressed={!fill().visible}
                    onClick={() => replace(index(), { visible: !fill().visible })}
                  >
                    <Icon name={fill().visible ? 'eye' : 'eyeOff'} />
                  </button>
                  <button class="fe-tool fe-tool--small fe-tool--muted" aria-label="Удалить заливку" onClick={() => remove(index())}>
                    <Icon name="minus" />
                  </button>
                </div>
              );
            }}
          </Index>
          <Show when={picker()}>
            {(pk) => {
              const start = shown(list()[pk().index].color, list()[pk().index].opacity);
              return (
                <ColorPicker
                  title="Цвет заливки"
                  color={start.rgb}
                  alpha={start.alpha}
                  anchor={pk().anchor}
                  onChange={(rgb, alpha, phase) =>
                    replace(pk().index, { color: toHex({ ...rgb, a: 255 }), opacity: alpha }, phase)
                  }
                  onClose={() => setPicker(null)}
                />
              );
            }}
          </Show>
        </Section>
      )}
    </Show>
  );
}

export function StrokesSection() {
  const ed = useEditor();
  const paints = () => ed.store.state.summary?.strokes;
  const [picker, setPicker] = createSignal<{ index: number; anchor: HTMLElement } | null>(null);
  const list = () => (paints()?.list ?? []) as StrokeDto[];
  const send = (strokes: StrokeDto[], phase: EditPhase) => sendEdit(ed.send, { type: 'strokes', strokes }, phase);
  const replace = (i: number, s: Partial<StrokeDto>, phase: EditPhase = 'commit') =>
    send(list().map((x, j) => (j === i ? { ...x, ...s } : { ...x })), phase);
  return (
    <Show when={paints()}>
      {(p) => (
        <Section
          title="Обводка"
          muted={!p().mixed && p().list.length === 0}
          actions={
            <button
              class="fe-tool fe-tool--small"
              aria-label="Добавить обводку"
              data-tip="Добавить обводку"
              onClick={() => send(p().mixed ? [NEW_STROKE] : [...list().map((x) => ({ ...x })), NEW_STROKE], 'commit')}
            >
              <Icon name="plus" />
            </button>
          }
        >
          <Show when={p().mixed}>
            <p class="fe-paint__mixed">Смешанные обводки — нажмите «+», чтобы заменить</p>
          </Show>
          <For each={list()}>
            {(stroke, i) => {
              const c = () => shown(stroke.color, 1);
              return (
                <div class="fe-stroke">
                  <div class="fe-paint">
                    <button
                      class="fe-swatch"
                      aria-label="Цвет обводки"
                      aria-haspopup="dialog"
                      style={{ '--swatch': cssColor(c().rgb, c().alpha) }}
                      onClick={(e) => setPicker({ index: i(), anchor: e.currentTarget })}
                    />
                    <HexField name="HEX обводки" rgb={c().rgb} onSet={(rgb) => replace(i(), { color: toHex({ ...rgb, a: 255 }) })} />
                    <button
                      class="fe-tool fe-tool--small fe-tool--muted"
                      aria-label="Удалить обводку"
                      onClick={() => send(list().filter((_, j) => j !== i()).map((x) => ({ ...x })), 'commit')}
                    >
                      <Icon name="minus" />
                    </button>
                  </div>
                  <div class="fe-grid2">
                    <NumberField
                      label="W"
                      name="Толщина обводки"
                      value={stroke.width}
                      min={0}
                      onEdit={(input, phase) =>
                        replace(i(), { width: input.kind === 'set' ? input.value : stroke.width + input.delta }, phase)
                      }
                    />
                    <label class="fe-field">
                      <select
                        class="fe-native-select"
                        aria-label="Выравнивание обводки"
                        value={stroke.align}
                        onChange={(e) => replace(i(), { align: e.currentTarget.value as Align })}
                      >
                        <For each={ALIGNS}>{(a) => <option value={a.value}>{a.label}</option>}</For>
                      </select>
                    </label>
                  </div>
                </div>
              );
            }}
          </For>
          <Show when={picker()}>
            {(pk) => {
              const start = shown(list()[pk().index].color, 1);
              return (
                <ColorPicker
                  title="Цвет обводки"
                  color={start.rgb}
                  alpha={start.alpha}
                  anchor={pk().anchor}
                  onChange={(rgb, alpha, phase) =>
                    replace(pk().index, { color: toHex({ ...rgb, a: Math.round(alpha * 255) }) }, phase)
                  }
                  onClose={() => setPicker(null)}
                />
              );
            }}
          </Show>
        </Section>
      )}
    </Show>
  );
}

/** Холст темы, если у страницы свой фон не задан (токен `canvas`). */
export const THEME_CANVAS = { dark: '#1a1918ff', light: '#e9e7e4ff' } as const;

/** Без выделения: фон страницы. В историю отмены не попадает (как и всё о
 * страницах); жест палитры — превью в кадре, в документ — при отпускании, Esc
 * возвращает фон документа. */
export function PageSection(props: { theme: 'dark' | 'light' }) {
  const ed = useEditor();
  const [picker, setPicker] = createSignal<HTMLElement | null>(null);
  const bg = () => ed.store.state.pages.background;
  const c = () => shown(bg() ?? THEME_CANVAS[props.theme], 1);
  const set = (rgb: Rgb, alpha: number, phase: EditPhase) =>
    ed.send({ type: 'setPageBackground', color: toHex({ ...rgb, a: Math.round(alpha * 255) }), phase });
  return (
    <Section title="Страница">
      <div class="fe-paint">
        <button
          class="fe-swatch"
          aria-label="Фон страницы"
          aria-haspopup="dialog"
          style={{ '--swatch': cssColor(c().rgb, c().alpha) }}
          onClick={(e) => setPicker(e.currentTarget)}
        />
        <HexField name="HEX фона страницы" rgb={c().rgb} onSet={(rgb) => set(rgb, 1, 'commit')} />
        <Show when={bg()}>
          <button
            class="fe-tool fe-tool--small fe-tool--muted"
            aria-label="Фон как в теме"
            data-tip="Фон как в теме"
            onClick={() => ed.send({ type: 'setPageBackground', color: undefined, phase: 'commit' })}
          >
            <Icon name="close" />
          </button>
        </Show>
      </div>
      <Show when={picker()}>
        {(a) => (
          <ColorPicker
            title="Фон страницы"
            color={c().rgb}
            alpha={c().alpha}
            anchor={a()}
            onChange={(rgb, alpha, phase) => set(rgb, alpha, phase)}
            onClose={() => setPicker(null)}
          />
        )}
      </Show>
    </Section>
  );
}
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
import type { FieldInput } from './number';

````

на:

````tsx
import type { FieldInput } from './number';
import { FillsSection, PageSection, StrokesSection } from './paints';

````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx

export function PropertiesPanel() {
  const ed = useEditor();
````

на:

````tsx

export function PropertiesPanel(props: { theme: 'dark' | 'light' }) {
  const ed = useEditor();
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
    <div class="fe-props" id="panel-design" role="tabpanel" aria-labelledby="tab-design">
      <Show when={count() > 0}>
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
      </Show>
````

на:

````tsx
    <div class="fe-props" id="panel-design" role="tabpanel" aria-labelledby="tab-design">
      <Show when={count() > 0} fallback={<PageSection theme={props.theme} />}>
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
        <FillsSection />
        <StrokesSection />
      </Show>
````

`apps/web/src/ui/styles/properties.css` (целиком):

````css
/* Панель свойств: секции, поля чисел (NumberField), переключатели. */

.fe-props {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
.fe-grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-2);
}
.fe-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}
.fe-row__grow {
  flex: 1;
}

.fe-field__label {
  width: 24px;
  height: 100%;
  flex: none;
  display: grid;
  place-items: center;
  color: var(--ink-muted);
  font-size: 11px;
  cursor: ew-resize;
  user-select: none;
  touch-action: none;
}
.fe-num input {
  padding-left: 0;
}
.fe-num--mixed {
  color: var(--ink-muted) !important;
  font-family: var(--font-sans) !important;
}
.fe-field--disabled {
  opacity: 0.5;
}
.fe-field--disabled .fe-field__label {
  cursor: default;
}

/* Выпадающий выбор (пресеты размера, выравнивание обводки). */
.fe-select {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  height: var(--row-height);
  padding: 0 var(--space-2);
  border: 0;
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
  color: var(--ink);
  cursor: pointer;
  min-width: 0;
}
.fe-select:hover {
  box-shadow: inset 0 0 0 1px var(--line-control);
}

.fe-check {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-height: 24px;
  cursor: pointer;
}
.fe-check input {
  width: 14px;
  height: 14px;
  margin: 0;
  accent-color: var(--select);
}

.fe-segmented {
  display: inline-flex;
  padding: 2px;
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
}
.fe-segmented__item {
  height: 24px;
  min-width: 32px;
  padding: 0 var(--space-2);
  border: 0;
  border-radius: 3px;
  background: transparent;
  color: var(--ink-muted);
  cursor: pointer;
}
.fe-segmented__item[aria-checked='true'] {
  background: var(--surface);
  color: var(--ink);
  box-shadow: 0 0 0 1px var(--line);
}
.fe-format {
  color: var(--ink-muted);
}
.fe-btn--wide {
  width: 100%;
  justify-content: center;
}
.fe-ellipsis {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Заливки и обводки (свотч — цвет пользователя, не токен). */
.fe-paint {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  min-width: 0;
}
.fe-paint--hidden .fe-field,
.fe-paint--hidden .fe-swatch {
  opacity: 0.45;
}
.fe-paint__opacity {
  width: 56px;
  flex: none;
}
.fe-paint__label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fe-paint__mixed {
  margin: 0;
  color: var(--ink-muted);
}
.fe-stroke {
  display: grid;
  gap: var(--space-2);
}
.fe-swatch {
  width: var(--row-height);
  height: var(--row-height);
  flex: none;
  padding: 0;
  border: 0;
  border-radius: var(--radius-sm);
  background:
    linear-gradient(var(--swatch), var(--swatch)),
    repeating-conic-gradient(#c8c8c8 0 25%, #ffffff 0 50%) 0 0 / 8px 8px;
  box-shadow: inset 0 0 0 1px var(--line);
  cursor: pointer;
}
.fe-swatch--image {
  display: grid;
  place-items: center;
  background: var(--surface-hover);
  color: var(--ink-muted);
  cursor: default;
}
.fe-native-select {
  width: 100%;
  height: 100%;
  padding: 0 var(--space-2);
  border: 0;
  background: transparent;
  color: var(--ink);
  font: 400 12px/16px var(--font-sans);
  outline: none;
}

/* Палитра цвета — всплывающее, shadow-popover. */
.fe-picker {
  position: fixed;
  z-index: 45;
  width: 240px;
  display: grid;
  gap: var(--space-3);
  padding: var(--space-3);
  border-radius: var(--radius-md);
  background: var(--surface-raised);
  box-shadow: var(--shadow-popover);
  animation: fe-pop 120ms ease-out;
}
.fe-picker__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.fe-picker__sb {
  position: relative;
  height: 160px;
  border-radius: var(--radius-sm);
  cursor: crosshair;
  touch-action: none;
}
.fe-picker__sb::before {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background:
    linear-gradient(to top, #000, transparent),
    linear-gradient(to right, #fff, transparent);
}
.fe-picker__thumb {
  position: absolute;
  width: 12px;
  height: 12px;
  margin: -6px 0 0 -6px;
  border-radius: 50%;
  box-shadow: 0 0 0 2px #fff, 0 0 0 3px rgba(0, 0, 0, 0.35);
  pointer-events: none;
}
.fe-picker__sliders {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
.fe-picker__bars {
  flex: 1;
  display: grid;
  gap: var(--space-2);
}
.fe-picker__bar {
  position: relative;
  height: 12px;
  border-radius: 6px;
  cursor: pointer;
  touch-action: none;
}
.fe-picker__hue {
  background: linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00);
}
.fe-picker__alpha {
  background:
    linear-gradient(to right, transparent, var(--picker-color)),
    repeating-conic-gradient(#c8c8c8 0 25%, #ffffff 0 50%) 0 0 / 8px 8px;
}
.fe-picker__knob {
  position: absolute;
  top: 50%;
  width: 14px;
  height: 14px;
  margin: -7px 0 0 -7px;
  border-radius: 50%;
  box-shadow: 0 0 0 2px #fff, 0 0 0 3px rgba(0, 0, 0, 0.35);
  pointer-events: none;
}
.fe-picker__opacity {
  width: 64px;
  flex: none;
}
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
              <DesignHeader />
              <PropertiesPanel />
              <ResizeEdge side="right" width={right()} onWidth={setRight} />
````

на:

````tsx
              <DesignHeader />
              <PropertiesPanel theme="dark" />
              <ResizeEdge side="right" width={right()} onWidth={setRight} />
````

`apps/web/e2e/picker.spec.ts`:

````ts
// Палитра и заливки: жест в палитре — превью и один шаг, HEX и прозрачность,
// добавить и убрать заливку, обводка, фон страницы, пипетка — только где есть
// EyeDropper.

import { expect, test, type Page } from '@playwright/test';
import { hsbToRgb, rgbToHsb, toHex } from '../src/ui/colour';
import { appScreen, expectPixel, live, liveWhen, openApp, pixel } from './helpers';

const fills = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary?.fills ?? null)));

/** Выделить «Слой 1» первого фрейма щелчком на холсте (мир (200, 40)). */
async function selectShape(page: Page): Promise<{ x: number; y: number }> {
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  return at;
}

test('жест в палитре — превью, затем один шаг; отмена возвращает цвет', async ({ page }) => {
  await openApp(page);
  const at = await selectShape(page);
  const before = await live(page);
  const start = (await fills(page)).list[0].color;
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  const picker = page.getByRole('dialog', { name: 'Цвет заливки' });
  const area = picker.getByRole('slider', { name: 'Насыщенность и яркость' });
  const box = (await area.boundingBox())!;
  // За правый верхний угол (указатель захвачен, значение упирается в край) —
  // чистый тон: насыщенность и яркость 100 %.
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width + 20, box.y - 20, { steps: 8 });
  expect((await live(page)).steps).toBe(before.steps);
  // Превью уже на холсте, документ — нет.
  await page.mouse.up();
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  const after = (await fills(page)).list[0].color;
  expect(after).not.toBe(start);
  // Тон — прежний (у «Слой 1» цвет (1, 120, 200)), насыщенность и яркость — 100 %.
  const hue = rgbToHsb({ r: 1, g: 120, b: 200 }).h;
  expect(after).toBe(toHex({ ...hsbToRgb({ h: hue, s: 1, b: 1 }), a: 255 }));
  await page.keyboard.press('Escape');
  await expect(picker).toHaveCount(0);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(async () => (await fills(page)).list[0].color).toBe(start);
  await expectPixel(page, at.x, at.y, [1, 120, 200]);
});

test('HEX и прозрачность заливки; заливку добавить и убрать', async ({ page }) => {
  await openApp(page);
  const at = await selectShape(page);
  const hex = page.getByRole('textbox', { name: 'HEX заливки' });
  await hex.fill('ff0000');
  await hex.press('Enter');
  await expect.poll(async () => (await fills(page)).list[0].color).toBe('#ff0000ff');
  await expectPixel(page, at.x, at.y, [255, 0, 0]);
  const opacity = page.getByRole('textbox', { name: 'Прозрачность заливки' });
  await opacity.fill('50');
  await opacity.press('Enter');
  await expect.poll(async () => (await fills(page)).list[0].opacity).toBe(0.5);
  // Пройти по полю Tab-ом без правки — не шаг отмены.
  const before = (await live(page)).steps;
  await opacity.focus();
  await page.keyboard.press('Tab');
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before);
  await page.getByRole('button', { name: 'Добавить заливку' }).click();
  await expect.poll(async () => (await fills(page)).list.length).toBe(2);
  await page.getByRole('button', { name: 'Удалить заливку' }).first().click();
  await expect.poll(async () => (await fills(page)).list.length).toBe(1);
  // Неверный HEX — поле возвращает прежнее значение, правки нет.
  const steps = (await live(page)).steps;
  await hex.fill('красный');
  await hex.press('Enter');
  await expect(hex).toHaveValue('FF0000');
  expect((await live(page)).steps).toBe(steps);
});

test('обводка: цвет, толщина, выравнивание', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  const strokes = () => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary!.strokes)));
  const width = page.getByRole('textbox', { name: 'Толщина обводки' });
  await width.fill('4');
  await width.press('Enter');
  await expect.poll(async () => (await strokes()).list[0].width).toBe(4);
  await page.getByRole('combobox', { name: 'Выравнивание обводки' }).selectOption('outside');
  await expect.poll(async () => (await strokes()).list[0].align).toBe('outside');
  await page.getByRole('textbox', { name: 'HEX обводки' }).fill('00ff00');
  await page.getByRole('textbox', { name: 'HEX обводки' }).press('Enter');
  await expect.poll(async () => (await strokes()).list[0].color).toBe('#00ff00ff');
});

test('фон страницы без выделения', async ({ page }) => {
  await openApp(page, 100);
  const hex = page.getByRole('textbox', { name: 'HEX фона страницы' });
  await expect(hex).toHaveValue('1A1918');
  await hex.fill('336699');
  await hex.press('Enter');
  const l = await live(page);
  const box = (await page.locator('canvas').boundingBox())!;
  await expectPixel(page, box.x + 20, box.y + l.viewportH - 20, [51, 102, 153]);
  await page.getByRole('button', { name: 'Фон как в теме' }).click();
  await expectPixel(page, box.x + 20, box.y + l.viewportH - 20, [26, 25, 24]);
  expect((await live(page)).canUndo).toBe(0);
  // Жест палитры — превью в кадре, документ не меняется до отпускания; Esc во время
  // жеста возвращает фон документа.
  const background = () => page.evaluate(() => window.__ferrum!.ui!.pages.background);
  await page.getByRole('button', { name: 'Фон страницы' }).click();
  const area = page.getByRole('dialog', { name: 'Фон страницы' }).getByRole('slider', { name: 'Насыщенность и яркость' });
  const a = (await area.boundingBox())!;
  await page.mouse.move(a.x + 5, a.y + 5);
  await page.mouse.down();
  await page.mouse.move(a.x + 10, a.y + 6, { steps: 4 });
  // Почти белый (левый верхний угол области — малая насыщенность, полная яркость).
  await expect.poll(async () => (await pixel(page, box.x + 20, box.y + l.viewportH - 20))[1]).toBeGreaterThan(220);
  expect(await background()).toBeNull();
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expectPixel(page, box.x + 20, box.y + l.viewportH - 20, [26, 25, 24]);
  expect(await background()).toBeNull();
});

test('пипетка — только если есть EyeDropper', async ({ page }) => {
  await page.addInitScript(() => {
    delete (window as unknown as { EyeDropper?: unknown }).EyeDropper;
  });
  await openApp(page);
  await selectShape(page);
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  await expect(page.getByRole('dialog', { name: 'Цвет заливки' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Пипетка' })).toHaveCount(0);
});
````

В `apps/web/e2e/layers.spec.ts` заменить:

````ts
  // Сворачивается.
  await page.getByRole('button', { name: 'Страницы' }).click();
  await expect(pages).toHaveCount(0);
````

на:

````ts
  // Сворачивается.
  await page.getByRole('button', { name: 'Страницы', exact: true }).click();
  await expect(pages).toHaveCount(0);
````

Run: `cd apps/web && npm run typecheck && npm run build && npx playwright test e2e/picker.spec.ts e2e/layers.spec.ts e2e/properties.spec.ts`
Expected: PASS, `17 passed`.

- [ ] **Step 4: Закоммитить**

```bash
git add apps/web
git commit -m "feat(web): палитра цвета, заливки, обводки и фон страницы"
```

---

### Task 11: Настройки: горячие клавиши, тема, привязка к пикселям

Диалог «Настройки» (меню файла → «Настройки…»; `role="dialog"`, `aria-modal`, фокус не уходит из диалога, Esc закрывает и возвращает фокус) с вкладками:

- **Горячие клавиши** — вся таблица команд по группам (Инструменты, Правка, Объект, Порядок, Сдвиг, Вид), поиск по названию или подписи сочетания. «+» записывает сочетание с клавиатуры по физической клавише (`code`): на русской раскладке «й» — это `KeyQ`; Esc без модификаторов отменяет запись, нажатый одиночный модификатор — ждать дальше. Занятое сочетание — вопрос «R уже у команды «Прямоугольник». Назначить «Эллипс»?» (ядро сообщает `bindConflict`, «Заменить» шлёт `bind` с `replace`). Сочетание у нескольких команд помечено «Конфликт: …», сочетание браузера (`browser_reserved`) — «Перехватывает браузер»; оба — словом и иконкой, не только цветом. «Сбросить» у изменённой команды, «Сбросить все», «Импорт…» (JSON до 64 КБ, пропущенное — в уведомлении) и «Экспорт» (`ferrum-keymap.json`).
- **Тема** — тёмная, светлая или как в системе (`prefers-color-scheme`, меняется на лету); тема интерфейса и оверлеев ядра одна.
- **Холст** — привязка к пикселям.

Настройки — тема, привязка, отличия раскладки, ширина панелей, свёрнутость страниц — хранятся в `localStorage` (`ferrum.settings.v1`) и применяются при запуске; доступ к хранилищу может бросить или вернуть мусор — тогда умолчания по полям, запись тихо не удаётся. Первая раскладка от ядра — умолчания: пока не пришла сохранённая, её не пишут поверх (иначе неудачный импорт стёр бы настройки пользователя); импорт сохранённой раскладки при запуске — без уведомления. Пока открыт модальный диалог, клавиши холсту не пересылаются.

**Files:**
- Create: `apps/web/src/ui/settings.ts`
- Create: `apps/web/src/ui/keymap-ui.ts`
- Test: `apps/web/unit/settings.test.ts`
- Create: `apps/web/src/ui/settings-dialog.tsx`
- Modify: `apps/web/src/ui/store.ts`
- Modify: `apps/web/src/ui/context.ts`
- Modify: `apps/web/src/ui/boot.ts`
- Modify: `apps/web/src/ui/tooltip.tsx`
- Modify: `apps/web/src/ui/menu.tsx`
- Modify: `apps/web/src/ui/keys.ts`
- Modify: `apps/web/src/ui/styles/chrome.css`
- Modify: `apps/web/src/ui/app.tsx`
- Modify: `apps/web/index.html`
- Test: `apps/web/e2e/settings.spec.ts`

**Interfaces:**
- Consumes: `store.state.keymap`, события `bindConflict`, `keymapImported`, команды `bind`, `unbind`, `resetKeymap`, `importKeymap`, `setSettings`, `setTheme` (Task 5); `shortcutLabel`, `eventShortcut` (Task 7), `downloadBytes` (Task 9).
- Produces: `Settings`, `ThemeChoice`, `SETTINGS_KEY`, `DEFAULT_SETTINGS`, `storage()`, `loadSettings`, `saveSettings`, `resolveTheme`; `GROUPS`, `groupCommands`, `filterGroups`, `reservedOf`, `conflictsOf`, `recordKey`; `SettingsDialog`; `UiState.bindConflict`, `store.quietKeymapImport()`; `Editor.layout()` (раскладка клавиатуры для подписей); `BootOptions.beforeDocument`.

- [ ] **Step 1: Написать падающий тест настроек и таблицы раскладки**

`apps/web/unit/settings.test.ts`:

````ts
// Настройки и таблица раскладки: чтение с умолчаниями при мусоре и отказе
// хранилища, тема по системе, группы и поиск команд, браузерные сочетания,
// конфликты, запись сочетания.

import { expect, test } from '@playwright/test';
import type { KeymapEntry } from '../src/wasm/ferrum_wasm';
import { conflictsOf, filterGroups, groupCommands, recordKey, reservedOf } from '../src/ui/keymap-ui';
import { DEFAULT_SETTINGS, loadSettings, resolveTheme, saveSettings, SETTINGS_KEY } from '../src/ui/settings';

function memory(): Storage {
  const m = new Map<string, string>();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => void m.set(k, v),
    removeItem: (k) => void m.delete(k),
    clear: () => m.clear(),
    key: () => null,
    length: 0,
  };
}

test('настройки: запись и чтение, мусор и отказ хранилища', () => {
  const s = memory();
  expect(loadSettings(s)).toEqual(DEFAULT_SETTINGS);
  const mine = { ...DEFAULT_SETTINGS, theme: 'light' as const, pixelSnap: false, keymap: '{"x":1}', leftWidth: 300 };
  expect(saveSettings(s, mine)).toBe(true);
  expect(loadSettings(s)).toEqual(mine);
  s.setItem(SETTINGS_KEY, '{не json');
  expect(loadSettings(s)).toEqual(DEFAULT_SETTINGS);
  s.setItem(SETTINGS_KEY, JSON.stringify({ theme: 'синяя', pixelSnap: 'да', leftWidth: 1e9, pagesOpen: false }));
  expect(loadSettings(s)).toEqual({ ...DEFAULT_SETTINGS, pagesOpen: false });
  const broken = {
    getItem: () => {
      throw new Error('SecurityError');
    },
    setItem: () => {
      throw new Error('QuotaExceededError');
    },
  };
  expect(loadSettings(broken)).toEqual(DEFAULT_SETTINGS);
  expect(saveSettings(broken, mine)).toBe(false);
  expect(loadSettings(null)).toEqual(DEFAULT_SETTINGS);
});

test('тема «системная» — по prefers-color-scheme', () => {
  expect(resolveTheme('system', true)).toBe('dark');
  expect(resolveTheme('system', false)).toBe('light');
  expect(resolveTheme('light', true)).toBe('light');
});

const entry = (key: string, name: string, bindings: string[]): KeymapEntry => ({
  key,
  name,
  group: key.split('.')[0],
  bindings,
  labels: [],
  defaults: bindings,
  overridden: false,
});

test('группы, поиск, браузерные сочетания и конфликты', () => {
  const commands = [
    entry('view.zoom_in', 'Приблизить', ['Equal']),
    entry('tool.rect', 'Прямоугольник', ['KeyR']),
    entry('tool.ellipse', 'Эллипс', ['KeyR', 'KeyO']),
    entry('edit.duplicate', 'Дублировать', ['Ctrl+KeyD', 'Ctrl+KeyW']),
    entry('plugin.run', 'Плагин', []),
  ];
  const groups = groupCommands(commands);
  expect(groups.map((g) => g.title)).toEqual(['Инструменты', 'Правка', 'Вид', 'plugin']);
  expect(filterGroups(groups, 'ЭЛЛ', 'windows')[0].commands.map((c) => c.key)).toEqual(['tool.ellipse']);
  expect(filterGroups(groups, 'r', 'windows')[0].commands.map((c) => c.key)).toEqual(['tool.rect', 'tool.ellipse']);
  expect(filterGroups(groups, 'ctrl+d', 'windows').map((g) => g.id)).toEqual(['edit']);
  expect(reservedOf(commands[3], ['Ctrl+KeyW', 'Ctrl+KeyT'])).toEqual(['Ctrl+KeyW']);
  const conflicts = [{ shortcut: 'KeyR', label: 'R', commands: ['tool.rect', 'tool.ellipse'] }];
  expect(conflictsOf(commands[1], 'KeyR', conflicts)).toEqual(['tool.ellipse']);
  expect(conflictsOf(commands[1], 'KeyO', conflicts)).toEqual([]);
});

test('запись сочетания: Esc — отмена, модификатор — ждать, клавиша — сочетание', () => {
  const k = (code: string, mods: Partial<Record<'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey', boolean>> = {}) => ({
    code,
    key: '',
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...mods,
  });
  expect(recordKey(k('Escape'))).toEqual({ kind: 'cancel' });
  expect(recordKey(k('Escape', { shiftKey: true }))).toEqual({ kind: 'shortcut', shortcut: 'Shift+Escape' });
  expect(recordKey(k('ShiftLeft', { shiftKey: true }))).toEqual({ kind: 'wait' });
  // Русская раскладка: «й» — физическая KeyQ.
  expect(recordKey({ ...k('KeyQ', { ctrlKey: true }), key: 'й' })).toEqual({ kind: 'shortcut', shortcut: 'Ctrl+KeyQ' });
});
````

Run: `cd apps/web && npm run unit`
Expected: FAIL — `Cannot find module '../src/ui/keymap-ui'`.

- [ ] **Step 2: Настройки и таблица раскладки**

`apps/web/src/ui/settings.ts`:

````ts
// Настройки пользователя: тема, привязка к пикселям, отличия раскладки клавиш,
// ширина панелей. Хранятся в localStorage (полноценное хранилище — этап 6);
// доступ к нему может бросить (закрытые cookie, приватный режим) или вернуть
// мусор — тогда настройки по умолчанию, а запись тихо не удаётся.

export type ThemeChoice = 'dark' | 'light' | 'system';

export interface Settings {
  theme: ThemeChoice;
  pixelSnap: boolean;
  /** JSON отличий раскладки (`ferrum-keymap`), как его отдаёт ядро; пусто — умолчания. */
  keymap: string;
  leftWidth: number;
  rightWidth: number;
  pagesOpen: boolean;
}

export const SETTINGS_KEY = 'ferrum.settings.v1';
/** Предел размера записи: раскладка ядра — до 64 КБ, остальное — мелочь. */
const MAX_BYTES = 80 * 1024;

export const DEFAULT_SETTINGS: Settings = {
  theme: 'dark',
  pixelSnap: true,
  keymap: '',
  leftWidth: 248,
  rightWidth: 248,
  pagesOpen: true,
};

/** Хранилище: `localStorage`, если доступ к нему не бросает. */
export function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

const isWidth = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v) && v >= 120 && v <= 800;

/** Читает настройки; неверные поля заменяются умолчаниями поодиночке. */
export function loadSettings(store: Pick<Storage, 'getItem'> | null): Settings {
  let raw: unknown = null;
  try {
    const text = store?.getItem(SETTINGS_KEY);
    if (text && text.length <= MAX_BYTES) raw = JSON.parse(text);
  } catch {
    raw = null;
  }
  const o = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  return {
    theme: o.theme === 'light' || o.theme === 'system' || o.theme === 'dark' ? o.theme : DEFAULT_SETTINGS.theme,
    pixelSnap: typeof o.pixelSnap === 'boolean' ? o.pixelSnap : DEFAULT_SETTINGS.pixelSnap,
    keymap: typeof o.keymap === 'string' && o.keymap.length <= 64 * 1024 ? o.keymap : DEFAULT_SETTINGS.keymap,
    leftWidth: isWidth(o.leftWidth) ? o.leftWidth : DEFAULT_SETTINGS.leftWidth,
    rightWidth: isWidth(o.rightWidth) ? o.rightWidth : DEFAULT_SETTINGS.rightWidth,
    pagesOpen: typeof o.pagesOpen === 'boolean' ? o.pagesOpen : DEFAULT_SETTINGS.pagesOpen,
  };
}

/** Записывает настройки; `false` — не удалось (нет места, запрещено). */
export function saveSettings(store: Pick<Storage, 'setItem'> | null, s: Settings): boolean {
  try {
    if (!store) return false;
    store.setItem(SETTINGS_KEY, JSON.stringify(s));
    return true;
  } catch {
    return false;
  }
}

/** Тема интерфейса для выбора «системная» — по `prefers-color-scheme`. */
export function resolveTheme(choice: ThemeChoice, prefersDark: boolean): 'dark' | 'light' {
  if (choice === 'system') return prefersDark ? 'dark' : 'light';
  return choice;
}
````

`apps/web/src/ui/keymap-ui.ts`:

````ts
// Таблица раскладки для настроек: группы команд по ключу (`tool.*`, `edit.*`…),
// поиск по названию и подписи, сочетания, которые перехватывает браузер,
// конфликты, запись сочетания с клавиатуры.

import type { ConflictInfo, KeymapEntry } from '../wasm/ferrum_wasm';
import type { Platform } from '../core/core';
import { eventShortcut, shortcutLabel, type LayoutMap } from './shortcuts';

export interface KeymapGroup {
  id: string;
  title: string;
  commands: KeymapEntry[];
}

/** Порядок и названия групп; неизвестная группа — в конце под своим ключом. */
export const GROUPS: { id: string; title: string }[] = [
  { id: 'tool', title: 'Инструменты' },
  { id: 'edit', title: 'Правка' },
  { id: 'object', title: 'Объект' },
  { id: 'arrange', title: 'Порядок' },
  { id: 'nudge', title: 'Сдвиг' },
  { id: 'view', title: 'Вид' },
];

export function groupCommands(commands: readonly KeymapEntry[]): KeymapGroup[] {
  const out: KeymapGroup[] = GROUPS.map((g) => ({ ...g, commands: [] }));
  for (const c of commands) {
    let g = out.find((x) => x.id === c.group);
    if (!g) {
      g = { id: c.group, title: c.group, commands: [] };
      out.push(g);
    }
    g.commands.push(c);
  }
  return out.filter((g) => g.commands.length > 0);
}

/** Поиск: по названию команды (без учёта регистра) или по подписи сочетания. */
export function filterGroups(groups: KeymapGroup[], query: string, platform: Platform, layout?: LayoutMap): KeymapGroup[] {
  const q = query.trim().toLowerCase();
  if (!q) return groups;
  return groups
    .map((g) => ({
      ...g,
      commands: g.commands.filter(
        (c) =>
          c.name.toLowerCase().includes(q) ||
          c.bindings.some((b) => shortcutLabel(b, platform, layout).toLowerCase() === q),
      ),
    }))
    .filter((g) => g.commands.length > 0);
}

/** Сочетания команды, которые перехватывает браузер. */
export function reservedOf(entry: KeymapEntry, reserved: readonly string[]): string[] {
  return entry.bindings.filter((b) => reserved.includes(b));
}

/** Другие команды на том же сочетании (ключи). */
export function conflictsOf(entry: KeymapEntry, shortcut: string, conflicts: readonly ConflictInfo[]): string[] {
  return conflicts.find((c) => c.shortcut === shortcut)?.commands.filter((k) => k !== entry.key) ?? [];
}

/** Что сделать при нажатии во время записи сочетания. */
export type RecordResult = { kind: 'cancel' } | { kind: 'wait' } | { kind: 'shortcut'; shortcut: string };

export function recordKey(e: Pick<KeyboardEvent, 'code' | 'key' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>): RecordResult {
  // Esc без модификаторов — отмена записи (как и в редакторе, Esc всегда выход).
  if (e.code === 'Escape' && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) return { kind: 'cancel' };
  const s = eventShortcut(e);
  return s ? { kind: 'shortcut', shortcut: s } : { kind: 'wait' };
}
````

Run: `cd apps/web && npm run unit`
Expected: PASS, `27 passed`.

- [ ] **Step 3: Диалог «Настройки» и тема**

`apps/web/src/ui/settings-dialog.tsx`:

````tsx
// Диалог «Настройки»: горячие клавиши (вся таблица команд по группам, запись
// сочетания с клавиатуры по физической клавише — работает на русской раскладке,
// конфликты с предложением заменить, предупреждение о сочетаниях браузера, сброс
// одной команды и всех, импорт и экспорт JSON), тема, привязка к пикселям.

import { createMemo, createSignal, For, onCleanup, onMount, Show, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';
import type { KeymapEntry } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { downloadBytes } from './files';
import { Icon } from './icons';
import { conflictsOf, filterGroups, groupCommands, recordKey, reservedOf } from './keymap-ui';
import type { ThemeChoice } from './settings';
import { shortcutLabel } from './shortcuts';

type Tab = 'keys' | 'theme' | 'canvas';

const TABS: { id: Tab; title: string }[] = [
  { id: 'keys', title: 'Горячие клавиши' },
  { id: 'theme', title: 'Тема' },
  { id: 'canvas', title: 'Холст' },
];

/** Предел файла раскладки — как у ядра (64 КБ). */
const MAX_KEYMAP_FILE = 64 * 1024;

export interface SettingsDialogProps {
  theme: ThemeChoice;
  onTheme: (t: ThemeChoice) => void;
  pixelSnap: boolean;
  onPixelSnap: (on: boolean) => void;
  onClose: () => void;
}

export function SettingsDialog(props: SettingsDialogProps) {
  const ed = useEditor();
  const [tab, setTab] = createSignal<Tab>('keys');
  let el!: HTMLDivElement;
  const returnFocus = document.activeElement as HTMLElement | null;
  const close = () => {
    props.onClose();
    returnFocus?.focus?.();
  };

  onMount(() => {
    el.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
    // Esc — на окне: фокус мог остаться на кнопке, которую уже убрали.
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !el.querySelector('[data-recording="true"]')) {
        e.preventDefault();
        close();
      }
    };
    // Фокус не уходит из диалога (aria-modal): ушёл — возвращается к первой вкладке.
    const trap = (e: FocusEvent) => {
      if (e.target instanceof Node && !el.contains(e.target)) {
        el.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
      }
    };
    window.addEventListener('keydown', key);
    document.addEventListener('focusin', trap);
    onCleanup(() => {
      window.removeEventListener('keydown', key);
      document.removeEventListener('focusin', trap);
    });
  });

  return (
    <Portal>
      <div class="fe-overlay" onPointerDown={(e) => e.target === e.currentTarget && close()}>
        <div ref={el} class="fe-dialog" role="dialog" aria-modal="true" aria-labelledby="fe-settings-title">
          <div class="fe-dialog__head">
            <h1 class="t-heading" id="fe-settings-title">
              Настройки
            </h1>
            <button class="fe-tool fe-tool--small" aria-label="Закрыть настройки" onClick={close}>
              <Icon name="close" />
            </button>
          </div>
          <div class="fe-tabs" role="tablist" aria-label="Разделы настроек">
            <For each={TABS}>
              {(t) => (
                <button
                  class="fe-tab"
                  role="tab"
                  id={`settings-tab-${t.id}`}
                  aria-selected={tab() === t.id}
                  aria-controls={`settings-${t.id}`}
                  tabindex={tab() === t.id ? 0 : -1}
                  onClick={() => setTab(t.id)}
                  onKeyDown={(e) => {
                    const i = TABS.findIndex((x) => x.id === tab());
                    const d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
                    if (!d) return;
                    const next = TABS[(i + d + TABS.length) % TABS.length].id;
                    setTab(next);
                    el.querySelector<HTMLElement>(`#settings-tab-${next}`)?.focus();
                  }}
                >
                  {t.title}
                </button>
              )}
            </For>
          </div>
          <div class="fe-dialog__body" role="tabpanel" id={`settings-${tab()}`} aria-labelledby={`settings-tab-${tab()}`}>
            <Show when={tab() === 'keys'}>
              <KeymapTab />
            </Show>
            <Show when={tab() === 'theme'}>
              <Radio
                label="Тема интерфейса"
                value={props.theme}
                options={[
                  { value: 'dark', label: 'Тёмная', icon: 'moon' },
                  { value: 'light', label: 'Светлая', icon: 'sun' },
                  { value: 'system', label: 'Как в системе', icon: 'monitor' },
                ]}
                onChange={(v) => props.onTheme(v as ThemeChoice)}
              />
            </Show>
            <Show when={tab() === 'canvas'}>
              <label class="fe-check fe-check--block">
                <input type="checkbox" checked={props.pixelSnap} onChange={(e) => props.onPixelSnap(e.currentTarget.checked)} />
                <span>
                  <span class="t-strong">Привязка к пикселям</span>
                  <span class="t-muted fe-hint">Положение и размеры при перетаскивании округляются до целых пикселей</span>
                </span>
              </label>
            </Show>
          </div>
        </div>
      </div>
    </Portal>
  );
}

function Radio(props: {
  label: string;
  value: string;
  options: { value: string; label: string; icon: 'moon' | 'sun' | 'monitor' }[];
  onChange: (v: string) => void;
}): JSX.Element {
  return (
    <div class="fe-radio" role="radiogroup" aria-label={props.label}>
      <For each={props.options}>
        {(o) => (
          <label class="fe-radio__item" classList={{ 'fe-radio__item--on': props.value === o.value }}>
            <input type="radio" name={props.label} checked={props.value === o.value} onChange={() => props.onChange(o.value)} />
            <Icon name={o.icon} />
            <span>{o.label}</span>
          </label>
        )}
      </For>
    </div>
  );
}

function KeymapTab() {
  const ed = useEditor();
  const km = () => ed.store.state.keymap;
  const [query, setQuery] = createSignal('');
  const [recording, setRecording] = createSignal<string | null>(null);
  const groups = createMemo(() => filterGroups(groupCommands(km().commands), query(), ed.platform, ed.layout()));
  const label = (s: string) => shortcutLabel(s, ed.platform, ed.layout());
  const name = (key: string) => km().commands.find((c) => c.key === key)?.name ?? key;
  const conflict = () => ed.store.state.bindConflict;
  let fileInput!: HTMLInputElement;
  let search!: HTMLInputElement;
  // Вопрос о замене исчезает вместе с кнопкой в фокусе: фокус — обратно в поиск.
  const closeConflict = () => {
    ed.store.setState('bindConflict', null);
    search.focus();
  };

  // Запись сочетания: клавиши ловятся до всех (захват), браузеру не уходят.
  const onRecord = (e: KeyboardEvent) => {
    const command = recording();
    if (!command) return;
    e.preventDefault();
    e.stopPropagation();
    const r = recordKey(e);
    if (r.kind === 'wait') return;
    setRecording(null);
    if (r.kind === 'shortcut') ed.send({ type: 'bind', command, shortcut: r.shortcut, replace: false });
  };
  onMount(() => window.addEventListener('keydown', onRecord, true));
  onCleanup(() => window.removeEventListener('keydown', onRecord, true));

  const importFile = async (file: File | undefined) => {
    if (!file) return;
    if (file.size > MAX_KEYMAP_FILE) {
      ed.store.toast('Файл раскладки слишком большой (больше 64 КБ)', 'error');
      return;
    }
    ed.send({ type: 'importKeymap', json: await file.text() });
  };

  const row = (c: KeymapEntry) => (
    <div class="fe-keyrow" role="row">
      <span class="fe-keyrow__name" role="cell">
        {c.name}
      </span>
      <span class="fe-keyrow__keys" role="cell">
        <For each={c.bindings}>
          {(b) => {
            const others = () => conflictsOf(c, b, km().conflicts);
            const reserved = () => reservedOf(c, km().reserved).includes(b);
            return (
              <span class="fe-key" classList={{ 'fe-key--warn': reserved() || others().length > 0 }}>
                <span class="t-num">{label(b)}</span>
                <button
                  class="fe-key__remove"
                  aria-label={`Убрать ${label(b)} у команды «${c.name}»`}
                  onClick={() => ed.send({ type: 'unbind', command: c.key, shortcut: b })}
                >
                  <Icon name="close" size={12} />
                </button>
                <Show when={reserved()}>
                  <span class="fe-key__note">
                    <Icon name="warning" size={12} /> Перехватывает браузер
                  </span>
                </Show>
                <Show when={others().length > 0}>
                  <span class="fe-key__note">
                    <Icon name="warning" size={12} /> Конфликт: {others().map(name).join(', ')}
                  </span>
                </Show>
              </span>
            );
          }}
        </For>
        <Show
          when={recording() === c.key}
          fallback={
            <button
              class="fe-tool fe-tool--small fe-tool--muted"
              aria-label={`Добавить сочетание для «${c.name}»`}
              data-tip="Добавить сочетание"
              onClick={() => setRecording(c.key)}
            >
              <Icon name="plus" />
            </button>
          }
        >
          <span class="fe-key fe-key--recording" data-recording="true" role="status">
            Нажмите сочетание…
          </span>
        </Show>
        <Show when={c.overridden}>
          <button class="fe-btn fe-btn--small" onClick={() => ed.send({ type: 'resetKeymap', command: c.key })}>
            Сбросить
          </button>
        </Show>
      </span>
    </div>
  );

  return (
    <div class="fe-keymap">
      <label class="fe-field fe-field--text">
        <Icon name="search" class="t-muted fe-field__icon" />
        <input
          ref={search}
          type="search"
          placeholder="Поиск команды или сочетания"
          aria-label="Поиск команд"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
        />
      </label>
      <Show when={conflict()}>
        {(cf) => (
          <div class="fe-confirm" role="alertdialog" aria-label="Сочетание занято">
            <Icon name="warning" />
            <span class="fe-row__grow">
              {label(cf().shortcut)} уже у команды «{cf().commands.map(name).join('», «')}». Назначить «{name(cf().command)}»?
            </span>
            <button
              class="fe-btn fe-btn--secondary"
              onClick={() => {
                ed.send({ type: 'bind', command: cf().command, shortcut: cf().shortcut, replace: true });
                closeConflict();
              }}
            >
              Заменить
            </button>
            <button class="fe-btn" onClick={closeConflict}>
              Отмена
            </button>
          </div>
        )}
      </Show>
      <div class="fe-keymap__table" role="table" aria-label="Горячие клавиши">
        <For each={groups()}>
          {(g) => (
            <div role="rowgroup" class="fe-keymap__group">
              <h2 class="fe-section__title t-muted">{g.title}</h2>
              <For each={g.commands}>{row}</For>
            </div>
          )}
        </For>
      </div>
      <div class="fe-dialog__foot">
        <button class="fe-btn fe-btn--danger" onClick={() => ed.send({ type: 'resetKeymap', command: undefined })}>
          Сбросить все
        </button>
        <span class="fe-row__grow" />
        <input
          ref={fileInput}
          type="file"
          accept="application/json,.json"
          hidden
          onChange={(e) => {
            void importFile(e.currentTarget.files?.[0]);
            e.currentTarget.value = '';
          }}
        />
        <button class="fe-btn fe-btn--secondary" onClick={() => fileInput.click()}>
          <Icon name="upload" />
          Импорт…
        </button>
        <button
          class="fe-btn fe-btn--secondary"
          onClick={() => downloadBytes('ferrum-keymap.json', new TextEncoder().encode(km().overrides), 'application/json')}
        >
          <Icon name="download" />
          Экспорт
        </button>
      </div>
    </div>
  );
}
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  toasts: Toast[];
}
````

на:

````ts
  toasts: Toast[];
  /** Сочетание занято другими командами: настройки предлагают заменить. */
  bindConflict: { command: string; shortcut: string; commands: string[] } | null;
}
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  toasts: [],
};
````

на:

````ts
  toasts: [],
  bindConflict: null,
};
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  let toastId = 0;

````

на:

````ts
  let toastId = 0;
  let quietImport = false;

````

В `apps/web/src/ui/store.ts` заменить:

````ts
        break;
      case 'toggleUi':
````

на:

````ts
        break;
      case 'bindConflict':
        setState('bindConflict', { command: e.command, shortcut: e.shortcut, commands: e.commands });
        break;
      case 'keymapImported':
        // Сохранённая раскладка при запуске — молча, если в ней всё понятно.
        if (quietImport) {
          quietImport = false;
          if (e.warnings.length) toast(`Сохранённая раскладка: пропущено ${e.warnings.length}`, 'error');
          break;
        }
        toast(
          e.warnings.length
            ? `Раскладка импортирована, пропущено: ${e.warnings.length} (${e.warnings.slice(0, 2).join('; ')})`
            : 'Раскладка импортирована',
        );
        break;
      case 'toggleUi':
````

В `apps/web/src/ui/store.ts` заменить:

````ts

  return { state, setState, apply, applyLive, toast };
}
````

на:

````ts

  /** Следующий импорт раскладки — сохранённая при запуске: без уведомления. */
  function quietKeymapImport(): void {
    quietImport = true;
  }

  return { state, setState, apply, applyLive, toast, quietKeymapImport };
}
````

В `apps/web/src/ui/context.ts` заменить:

````ts
import type { Scheduler } from './scheduler';
import type { UiStore } from './store';
````

на:

````ts
import type { Scheduler } from './scheduler';
import type { LayoutMap } from './shortcuts';
import type { UiStore } from './store';
````

В `apps/web/src/ui/context.ts` заменить:

````ts
  run(command: string): void;
}
````

на:

````ts
  run(command: string): void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт). */
  layout(): LayoutMap | undefined;
}
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
import { createScheduler, guardLayout } from './scheduler';
import type { UiStore } from './store';
````

на:

````ts
import { createScheduler, guardLayout } from './scheduler';
import { loadLayoutMap, type LayoutMap } from './shortcuts';
import type { UiStore } from './store';
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
  isClipboardKey?: (e: KeyboardEvent) => boolean;
}
````

на:

````ts
  isClipboardKey?: (e: KeyboardEvent) => boolean;
  /** Перед документом: настройки пользователя (тема, раскладка, привязка). */
  beforeDocument?: (editor: Editor) => void;
}
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
  window.__ferrum = { core, events, ready: false, ui: store.state };
  const editor: Editor = {
````

на:

````ts
  window.__ferrum = { core, events, ready: false, ui: store.state };
  let layout: LayoutMap | undefined;
  void loadLayoutMap().then((m) => (layout = m));
  const editor: Editor = {
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
    run: (command) => core.send({ type: 'execute', command }),
  };
  core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 0) });
````

на:

````ts
    run: (command) => core.send({ type: 'execute', command }),
    layout: () => layout,
  };
  opts.beforeDocument?.(editor);
  core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 0) });
````

В `apps/web/src/ui/tooltip.tsx` заменить:

````tsx
import { useEditor } from './context';
import { shortcutLabel, tipText, type LayoutMap } from './shortcuts';

````

на:

````tsx
import { useEditor } from './context';
import { shortcutLabel, tipText } from './shortcuts';

````

В `apps/web/src/ui/tooltip.tsx` заменить:

````tsx

export function TooltipLayer(props: { layout?: LayoutMap }) {
  const ed = useEditor();
````

на:

````tsx

export function TooltipLayer() {
  const ed = useEditor();
````

В `apps/web/src/ui/tooltip.tsx` заменить:

````tsx
    const first = c?.bindings[0];
    return first ? shortcutLabel(first, ed.platform, props.layout) : '';
  };
````

на:

````tsx
    const first = c?.bindings[0];
    return first ? shortcutLabel(first, ed.platform, ed.layout()) : '';
  };
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
    const c = ed.store.state.keymap.commands.find((x) => x.key === command);
    return c?.bindings[0] ? shortcutLabel(c.bindings[0], ed.platform) : '';
  };
````

на:

````tsx
    const c = ed.store.state.keymap.commands.find((x) => x.key === command);
    return c?.bindings[0] ? shortcutLabel(c.bindings[0], ed.platform, ed.layout()) : '';
  };
````

В `apps/web/src/ui/keys.ts` заменить:

````ts
  if (ownsKeys(e.target)) return false;
  // Tab всегда у браузера: фокус ходит по интерфейсу.
````

на:

````ts
  if (ownsKeys(e.target)) return false;
  // Открыт модальный диалог: холст клавиш не получает, даже если фокус потерялся.
  if (document.querySelector('[role="dialog"][aria-modal="true"]')) return false;
  // Tab всегда у браузера: фокус ходит по интерфейсу.
````

В `apps/web/src/ui/styles/chrome.css` заменить:

````css
  gap: 2px;
}
````

на:

````css
  gap: 2px;
}

/* Диалог (настройки): shadow-dialog, radius-lg, отступы space-6. */
.fe-overlay {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 0.35);
}
.fe-dialog {
  width: min(720px, calc(100vw - 32px));
  max-height: calc(100vh - 64px);
  display: flex;
  flex-direction: column;
  border-radius: var(--radius-lg);
  background: var(--surface-raised);
  box-shadow: var(--shadow-dialog);
  color: var(--ink);
  font-size: 13px;
  line-height: 20px;
  animation: fe-pop 120ms ease-out;
}
.fe-dialog__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-4) var(--space-6) var(--space-2);
}
.fe-dialog__head h1 {
  margin: 0;
}
.fe-dialog .fe-tabs {
  padding: 0 var(--space-6) var(--space-2);
}
.fe-dialog__body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: var(--space-4) var(--space-6) var(--space-6);
}
.fe-dialog__foot {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding-top: var(--space-4);
  border-top: 1px solid var(--line);
  margin-top: var(--space-4);
}
.fe-hint {
  display: block;
  font-size: 12px;
  line-height: 16px;
}
.fe-check--block {
  align-items: flex-start;
}
.fe-check--block input {
  margin-top: 3px;
}

.fe-radio {
  display: flex;
  gap: var(--space-2);
}
.fe-radio__item {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  height: 36px;
  padding: 0 var(--space-3);
  border-radius: var(--radius-md);
  box-shadow: inset 0 0 0 1px var(--line);
  cursor: pointer;
}
.fe-radio__item input {
  margin: 0;
  accent-color: var(--select);
}
.fe-radio__item--on {
  background: var(--select-soft);
  box-shadow: inset 0 0 0 1px var(--select);
}
.fe-radio__item:has(input:focus-visible) {
  outline: 2px solid var(--select);
  outline-offset: 1px;
}

/* Таблица горячих клавиш. */
.fe-keymap {
  display: grid;
  gap: var(--space-3);
}
.fe-keymap__group {
  display: grid;
}
.fe-keymap__group h2 {
  padding: var(--space-3) 0 var(--space-1);
}
.fe-keyrow {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  min-height: var(--row-height);
  padding: 2px 0;
  border-bottom: 1px solid var(--line);
}
.fe-keyrow__name {
  flex: 1;
  min-width: 0;
  padding-top: 4px;
}
.fe-keyrow__keys {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  align-items: center;
  gap: var(--space-1);
  max-width: 60%;
}
.fe-key {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 2px;
  min-height: 24px;
  padding: 0 2px 0 var(--space-2);
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
}
.fe-key--warn {
  box-shadow: inset 0 0 0 1px var(--warning);
}
.fe-key--recording {
  padding: 0 var(--space-2);
  box-shadow: inset 0 0 0 1px var(--select);
  color: var(--ink-muted);
}
.fe-key__remove {
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  padding: 0;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--ink-muted);
  cursor: pointer;
}
.fe-key__remove:hover {
  background: var(--line);
}
.fe-key__note {
  flex-basis: 100%;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding-bottom: 2px;
  color: var(--ink-muted);
  font-size: 11px;
  line-height: 14px;
}
.fe-key__note .fe-ico {
  color: var(--warning);
}
.fe-btn--small {
  height: 24px;
  padding: 0 var(--space-2);
}
.fe-confirm {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-md);
  box-shadow: inset 0 0 0 1px var(--warning);
}
.fe-confirm > .fe-ico {
  color: var(--warning);
}
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
// Редактор: холст под управлением worker'а и панели вокруг него (раскладка UI3).

import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { boot } from './boot';
import { canvasMenu, DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { EditorContext, type Editor } from './context';
import { Menu, type MenuItem } from './menu';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
import { PropertiesPanel } from './properties';
import { downloadBytes } from './files';
import { createUiStore } from './store';
````

на:

````tsx
// Редактор: холст под управлением worker'а и панели вокруг него (раскладка UI3).
// Настройки пользователя (тема, привязка к пикселям, раскладка клавиш, ширина
// панелей) читаются при запуске и сохраняются при каждом изменении.

import { createEffect, createMemo, createSignal, on, onCleanup, onMount, Show } from 'solid-js';
import { boot } from './boot';
import { canvasMenu, DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { EditorContext, type Editor } from './context';
import { downloadBytes } from './files';
import { LayersPanel } from './layers';
import { Menu, type MenuItem } from './menu';
import { PagesPanel } from './pages';
import { PropertiesPanel } from './properties';
import { loadSettings, resolveTheme, saveSettings, storage, type ThemeChoice } from './settings';
import { SettingsDialog } from './settings-dialog';
import { createUiStore } from './store';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  const store = createUiStore();
  const [editor, setEditor] = createSignal<Editor>();
  const [failure, setFailure] = createSignal('');
  const [left, setLeft] = createSignal(248);
  const [right, setRight] = createSignal(248);
  const [context, setContext] = createSignal<{ x: number; y: number } | null>(null);
  const [pagesOpen, setPagesOpen] = createSignal(true);
  let canvas!: HTMLCanvasElement;

  onMount(() => {
    boot(canvas, store)
      .then((ed) => {
````

на:

````tsx
  const store = createUiStore();
  const saved = loadSettings(storage());
  const [editor, setEditor] = createSignal<Editor>();
  const [failure, setFailure] = createSignal('');
  const [left, setLeft] = createSignal(saved.leftWidth);
  const [right, setRight] = createSignal(saved.rightWidth);
  const [pagesOpen, setPagesOpen] = createSignal(saved.pagesOpen);
  const [themeChoice, setThemeChoice] = createSignal<ThemeChoice>(saved.theme);
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [context, setContext] = createSignal<{ x: number; y: number } | null>(null);
  const media = matchMedia('(prefers-color-scheme: dark)');
  const [prefersDark, setPrefersDark] = createSignal(media.matches);
  const onScheme = () => setPrefersDark(media.matches);
  media.addEventListener('change', onScheme);
  onCleanup(() => media.removeEventListener('change', onScheme));
  const theme = createMemo(() => resolveTheme(themeChoice(), prefersDark()));
  let canvas!: HTMLCanvasElement;

  // Первая раскладка от ядра — умолчания: пока не пришла сохранённая, её не пишем
  // поверх (иначе неудачный импорт стёр бы настройки пользователя).
  let keymapSeen = saved.keymap ? 0 : 1;
  const [keymap, setKeymap] = createSignal(saved.keymap);
  createEffect(
    on(
      () => store.state.keymap.overrides,
      (o) => {
        if (!o) return;
        keymapSeen += 1;
        if (keymapSeen > 1) setKeymap(o);
      },
    ),
  );

  createEffect(() => {
    document.documentElement.dataset.theme = theme();
    editor()?.send({ type: 'setTheme', theme: theme() });
  });
  createEffect(() => editor()?.send({ type: 'setSettings', pixelSnap: pixelSnap() }));
  createEffect(() =>
    saveSettings(storage(), {
      theme: themeChoice(),
      pixelSnap: pixelSnap(),
      keymap: keymap(),
      leftWidth: left(),
      rightWidth: right(),
      pagesOpen: pagesOpen(),
    }),
  );

  onMount(() => {
    boot(canvas, store, {
      beforeDocument: (ed) => {
        if (saved.keymap) {
          store.quietKeymapImport();
          ed.send({ type: 'importKeymap', json: saved.keymap });
        }
      },
    })
      .then((ed) => {
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
    { label: 'Создать', onSelect: () => ed.send({ type: 'demo', nodes: 0 }) },
  ];
````

на:

````tsx
    { label: 'Создать', onSelect: () => ed.send({ type: 'demo', nodes: 0 }) },
    { separator: true },
    { label: 'Настройки…', onSelect: () => setSettingsOpen(true) },
  ];
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
              <DesignHeader />
              <PropertiesPanel theme="dark" />
              <ResizeEdge side="right" width={right()} onWidth={setRight} />
````

на:

````tsx
              <DesignHeader />
              <PropertiesPanel theme={theme()} />
              <ResizeEdge side="right" width={right()} onWidth={setRight} />
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
            </Show>
          </EditorContext.Provider>
````

на:

````tsx
            </Show>
            <Show when={settingsOpen()}>
              <SettingsDialog
                theme={themeChoice()}
                onTheme={setThemeChoice}
                pixelSnap={pixelSnap()}
                onPixelSnap={setPixelSnap}
                onClose={() => setSettingsOpen(false)}
              />
            </Show>
          </EditorContext.Provider>
````

В `apps/web/index.html` заменить:

````html
<!doctype html>
<html lang="ru" data-theme="dark">
  <head>
````

на:

````html
<!doctype html>
<html lang="ru">
  <head>
````

`apps/web/e2e/settings.spec.ts`:

````ts
// Настройки: переназначить сочетание и пользоваться им (по физической клавише),
// конфликт с заменой, сочетания браузера, сброс, импорт и экспорт, тема,
// привязка к пикселям, сохранение между запусками.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, expectPixel, live, liveWhen, openApp } from './helpers';

async function openSettings(page: Page) {
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  return page.getByRole('dialog', { name: 'Настройки' });
}

const tool = (page: Page) => page.evaluate(() => window.__ferrum!.ui!.tool);

test('переназначить сочетание и пользоваться им; конфликт — с заменой', async ({ page }) => {
  await openApp(page);
  const dialog = await openSettings(page);
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('Прямоугольник');
  await dialog.getByRole('button', { name: 'Добавить сочетание для «Прямоугольник»' }).click();
  await expect(dialog.getByRole('status')).toHaveText('Нажмите сочетание…');
  // Физическая клавиша Q — на русской раскладке это «й», сочетание то же.
  await page.keyboard.press('KeyQ');
  await expect(dialog.getByRole('row').filter({ hasText: 'Прямоугольник' })).toContainText('Q');
  await dialog.getByRole('button', { name: 'Сбросить', exact: true }).waitFor();
  // Конфликт: R уже у прямоугольника.
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('Эллипс');
  await dialog.getByRole('button', { name: 'Добавить сочетание для «Эллипс»' }).click();
  await page.keyboard.press('KeyR');
  const confirm = dialog.getByRole('alertdialog', { name: 'Сочетание занято' });
  await expect(confirm).toContainText('R уже у команды «Прямоугольник»');
  await confirm.getByRole('button', { name: 'Заменить' }).click();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyQ');
  await expect.poll(() => tool(page)).toBe('rect');
  await page.keyboard.press('KeyR');
  await expect.poll(() => tool(page)).toBe('ellipse');
  // Сохранено: после перезагрузки раскладка та же.
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true);
  // Сохранённая раскладка при запуске — без уведомления.
  await expect(page.getByRole('status').filter({ hasText: 'Раскладка импортирована' })).toHaveCount(0);
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyQ');
  await expect.poll(() => tool(page)).toBe('rect');
  // Сбросить все.
  const again = await openSettings(page);
  await again.getByRole('button', { name: 'Сбросить все' }).click();
  await page.keyboard.press('Escape');
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyR');
  await expect.poll(() => tool(page)).toBe('rect');
});

test('импорт, экспорт и предупреждение о сочетаниях браузера', async ({ page }) => {
  await openApp(page);
  const mod = await page.evaluate(() => (navigator.platform.toLowerCase().startsWith('mac') ? 'Meta' : 'Ctrl'));
  const dialog = await openSettings(page);
  const json = JSON.stringify({ format: 'ferrum-keymap', version: 1, overrides: { 'edit.duplicate': [`${mod}+KeyW`], 'tool.pen': ['KeyP'] } });
  await dialog.locator('input[type=file]').setInputFiles({ name: 'keys.json', mimeType: 'application/json', buffer: Buffer.from(json) });
  await expect(page.getByRole('status').filter({ hasText: /Раскладка импортирована, пропущено: 1/ })).toBeVisible();
  const row = dialog.getByRole('row').filter({ hasText: 'Дублировать' });
  await expect(row).toContainText('Перехватывает браузер');
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Экспорт' }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('ferrum-keymap.json');
  const text = Buffer.concat(await (await file.createReadStream()).toArray()).toString();
  expect(JSON.parse(text)).toMatchObject({ format: 'ferrum-keymap', overrides: { 'edit.duplicate': [`${mod}+KeyW`] } });
  // Неверный файл — ошибка, раскладка не меняется.
  await dialog.locator('input[type=file]').setInputFiles({ name: 'bad.json', mimeType: 'application/json', buffer: Buffer.from('{') });
  await expect(page.getByRole('status').filter({ hasText: 'Не удалось импортировать раскладку' })).toBeVisible();
  await expect(row).toContainText('Перехватывает браузер');
});

test('тема: светлая, тёмная и как в системе', async ({ page }) => {
  await openApp(page, 100);
  const dialog = await openSettings(page);
  await dialog.getByRole('tab', { name: 'Тема' }).click();
  await dialog.getByRole('radio', { name: 'Светлая' }).check();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe('rgb(233, 231, 228)');
  await page.keyboard.press('Escape');
  // Холст в цвете темы рисует ядро.
  const box = (await page.locator('canvas').boundingBox())!;
  const l = await live(page);
  await expectPixel(page, box.x + 20, box.y + l.viewportH - 20, [233, 231, 228]);
  await page.emulateMedia({ colorScheme: 'dark' });
  const again = await openSettings(page);
  await again.getByRole('tab', { name: 'Тема' }).click();
  await again.getByRole('radio', { name: 'Как в системе' }).check();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('привязка к пикселям выключается в настройках', async ({ page }) => {
  await openApp(page, 0);
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'setZoom', zoom: 3 }));
  await liveWhen(page, 'l.zoom === 3');
  // Каждый прямоугольник — на своём месте: иначе второй притянулся бы к краю первого.
  let offset = 0;
  const draw = async () => {
    const l = await live(page);
    await page.keyboard.press('KeyR');
    const at = await appScreen(page, l, l.cameraX + offset, l.cameraY);
    offset += 40;
    await drag(page, at, 10, 10);
    return liveWhen(page, `l.steps === ${l.steps + 1} && l.busy === 0`);
  };
  await page.locator('canvas').focus();
  const snapped = await draw();
  expect(snapped.selectionW).toBe(Math.round(snapped.selectionW));
  const dialog = await openSettings(page);
  await dialog.getByRole('tab', { name: 'Холст' }).click();
  await dialog.getByRole('checkbox', { name: /Привязка к пикселям/ }).uncheck();
  await page.keyboard.press('Escape');
  await page.locator('canvas').focus();
  await page.keyboard.press('Escape');
  const free = await draw();
  expect(free.selectionW).not.toBe(Math.round(free.selectionW));
});

test('диалог: роли, фокус внутри, Esc закрывает', async ({ page }) => {
  await openApp(page);
  const dialog = await openSettings(page);
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
  await expect(dialog.getByRole('tab', { name: 'Горячие клавиши' })).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(dialog.getByRole('tab', { name: 'Тема' })).toBeFocused();
  for (let i = 0; i < 12; i++) await page.keyboard.press('Tab');
  expect(await dialog.evaluate((d) => d.contains(document.activeElement))).toBe(true);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Меню файла' })).toBeFocused();
});
````

Run: `cd apps/web && npm run typecheck && npm run build && npx playwright test e2e/settings.spec.ts e2e/chrome.spec.ts`
Expected: PASS, `13 passed`.

- [ ] **Step 4: Закоммитить**

```bash
git add apps/web
git commit -m "feat(web): настройки — горячие клавиши, тема, привязка к пикселям"
```

---

### Task 12: Системный буфер обмена и файлы

Спека 7.7: буфер обмена — работа интерфейса, разбор — работа worker'а. Ctrl/Cmd+C, X, V, пока они вызывают копирование, вырезание и вставку в раскладке, не пересылаются ядру клавишами: браузер вызывает события `copy`, `cut`, `paste` документа (в полях ввода и диалогах — обычный текст).

- **Копирование и вырезание.** Событие шлёт команду `edit.copy`/`edit.cut`; байты формата Ferrum приходят событием ядра `clipboard`. Где асинхронный Clipboard API умеет свои форматы (`ClipboardItem.supports('web application/x-ferrum')`), байты пишутся под `web application/x-ferrum`. Иначе — запасной путь: `document.execCommand('copy')` вызывает ещё одно событие `copy`, и в его `clipboardData` байты уходят текстом base64 за меткой `ferrum-clipboard:v1:`. То же — для команд из контекстного меню и переназначенных сочетаний.
- **Вставка.** Сначала `clipboardData` события: метка в тексте — байты Ferrum (`paste`), файл PNG, JPEG или WebP — картинка (`pasteImage`, в центр видимой части в исходном размере); затем асинхронное чтение своего формата. Картинка не PNG перекодируется в PNG в worker'е (`createImageBitmap` и `OffscreenCanvas`); размер проверяется по заголовку до декодирования (`image-size.ts`: PNG — IHDR, JPEG — SOF, WebP — VP8/VP8L/VP8X): файл в несколько байт может объявить 60 000 × 60 000. Пока картинка перекодируется, следующие команды ждут — порядок команд остаётся порядком пользователя. «Вставить» из меню (`pasteRequested`) читает буфер асинхронно; без доступа — уведомление.
- **Копировать как PNG** (Ctrl+Shift+C): PNG выделения от ядра (`clipboardPng`) — в буфер как `image/png`, уведомление «Скопировано как PNG».
- **Файлы** (минимум этапа 5): «Открыть…» — `showOpenFilePicker`, где он есть, иначе `<input type=file>`, байты — команде `open`; «Сохранить как…» — `save`, ответ `saved` скачивается файлом «Имя.ferrum». Инструмент «Изображение» (Ctrl+Shift+K) выбирает файл тем же путём и ставит картинку щелчком. Отказы выбора и чтения, картинки больше 64 МБ — уведомлением.

**Files:**
- Create: `apps/web/src/ui/clipboard.ts`
- Test: `apps/web/unit/clipboard.test.ts`
- Create: `apps/web/src/core/image-size.ts`
- Test: `apps/web/unit/image-size.test.ts`
- Modify: `apps/web/src/worker/core.worker.ts`
- Modify: `apps/web/src/core/messages.ts`
- Modify: `apps/web/tsconfig.worker.json`
- Modify: `apps/web/src/ui/files.ts`
- Create: `apps/web/src/ui/system-clipboard.ts`
- Modify: `apps/web/src/ui/app.tsx`
- Test: `apps/web/e2e/clipboard.spec.ts`

**Interfaces:**
- Consumes: события `clipboard`, `clipboardPng`, `pasteRequested`, `requestImage`, `saved`; команды `paste`, `pasteImage`, `placeImage`, `open`, `save` (этап 4, Task 5); `clipboardAction`, `eventShortcut` (Task 7).
- Produces: `FERRUM_MIME`, `FERRUM_TEXT_MARKER`, `MAX_CLIPBOARD_BYTES`, `IMAGE_TYPES`, `toBase64`, `fromBase64`, `encodeText`, `decodeText`, `fromDataTransfer`, `fromClipboardItems`, `supportsCustomFormat`, `PasteData`; `imageSize(bytes) → { width, height, type } | null`; `installClipboard(editor) → снятие`; `pickFile(kind)`, `fileBytes`, `FERRUM_FILE`, `IMAGE_FILE`. `MAX_COMMAND_BYTES.pasteImage` — 64 МБ.

- [ ] **Step 1: Написать падающие тесты буфера и размера картинки**

`apps/web/unit/clipboard.test.ts`:

````ts
// Буфер обмена: байты Ferrum текстом за меткой и обратно (в том числе большие),
// выбор того, что вставлять, из события и из асинхронного API.

import { expect, test } from '@playwright/test';
import {
  decodeText,
  encodeText,
  FERRUM_MIME,
  FERRUM_TEXT_MARKER,
  fromClipboardItems,
  fromDataTransfer,
  MAX_CLIPBOARD_BYTES,
} from '../src/ui/clipboard';

test('байты → текст с меткой → байты', () => {
  const bytes = new Uint8Array([0x46, 0x45, 0x52, 0, 255, 128, 7]);
  const text = encodeText(bytes);
  expect(text.startsWith(FERRUM_TEXT_MARKER)).toBe(true);
  expect(decodeText(text)).toEqual(bytes);
  // 3 МБ — без переполнения стека.
  const big = new Uint8Array(3 * 1024 * 1024).map((_, i) => i % 251);
  expect(decodeText(encodeText(big))).toEqual(big);
});

test('чужой и испорченный текст — не наши байты', () => {
  expect(decodeText('привет')).toBeNull();
  expect(decodeText(`${FERRUM_TEXT_MARKER}***`)).toBeNull();
  expect(decodeText(`${FERRUM_TEXT_MARKER}${'A'.repeat(Math.ceil((MAX_CLIPBOARD_BYTES * 4) / 3) + 8)}`)).toBeNull();
});

function dt(text: string, files: File[] = []) {
  return { getData: (t: string) => (t === 'text/plain' ? text : ''), files: files as unknown as FileList };
}

test('вставка из события: свои байты важнее картинки, картинка — PNG/JPEG/WebP', () => {
  const png = new File([new Uint8Array([137, 80])], 'a.png', { type: 'image/png' });
  const gif = new File([new Uint8Array([71])], 'a.gif', { type: 'image/gif' });
  expect(fromDataTransfer(dt(encodeText(new Uint8Array([1, 2])), [png]))).toEqual({ kind: 'ferrum', bytes: new Uint8Array([1, 2]) });
  expect(fromDataTransfer(dt('текст', [gif, png]))).toMatchObject({ kind: 'image', type: 'image/png' });
  expect(fromDataTransfer(dt('текст', [gif]))).toBeNull();
  expect(fromDataTransfer(null)).toBeNull();
});

test('вставка из асинхронного API: свой формат, текст с меткой, картинка', async () => {
  const item = (map: Record<string, Blob>) => ({ types: Object.keys(map), getType: async (t: string) => map[t] });
  expect(await fromClipboardItems([item({ [FERRUM_MIME]: new Blob([new Uint8Array([9, 8])]) })])).toEqual({
    kind: 'ferrum',
    bytes: new Uint8Array([9, 8]),
  });
  expect(await fromClipboardItems([item({ 'text/plain': new Blob([encodeText(new Uint8Array([5]))]) })])).toEqual({
    kind: 'ferrum',
    bytes: new Uint8Array([5]),
  });
  expect(await fromClipboardItems([item({ 'text/plain': new Blob(['hello']), 'image/jpeg': new Blob([new Uint8Array([255])]) })])).toMatchObject({
    kind: 'image',
    type: 'image/jpeg',
  });
  expect(await fromClipboardItems([item({ 'text/html': new Blob(['<b>']) })])).toBeNull();
});
````

`apps/web/unit/image-size.test.ts`:

````ts
// Размер картинки по заголовку — до декодирования (PNG, JPEG, WebP трёх видов).

import { expect, test } from '@playwright/test';
import { imageSize } from '../src/core/image-size';

const bytes = (...parts: (number[] | string)[]) =>
  new Uint8Array(parts.flatMap((p) => (typeof p === 'string' ? [...p].map((c) => c.charCodeAt(0)) : p)));

test('PNG: IHDR', () => {
  const png = bytes([0x89], 'PNG', [13, 10, 26, 10, 0, 0, 0, 13], 'IHDR', [0, 0, 0x01, 0x00, 0, 0, 0, 0x80]);
  expect(imageSize(png)).toEqual({ width: 256, height: 128, type: 'png' });
});

test('JPEG: маркер SOF после APP0, огромный размер читается без декодирования', () => {
  const app0 = [0xff, 0xe0, 0, 16, ...new Array(14).fill(0)];
  const sof = [0xff, 0xc2, 0, 17, 8, 0xea, 0x60, 0xea, 0x60, 3, ...new Array(9).fill(0)];
  expect(imageSize(bytes([0xff, 0xd8], app0, sof))).toEqual({ width: 60000, height: 60000, type: 'jpeg' });
  expect(imageSize(bytes([0xff, 0xd8], app0))).toBeNull();
});

test('WebP: VP8, VP8L, VP8X', () => {
  const head = (kind: string) => bytes('RIFF', [0, 0, 0, 0], 'WEBP', kind, [0, 0, 0, 0]);
  const vp8 = new Uint8Array([...head('VP8 '), 0, 0, 0, 0x9d, 0x01, 0x2a, 0x40, 0x01, 0xf0, 0x00, 0, 0]);
  expect(imageSize(vp8)).toEqual({ width: 320, height: 240, type: 'webp' });
  // VP8L: 14 бит ширины и высоты (минус один) после подписи 0x2f.
  const w = 99;
  const h = 49;
  const bits = w | (h << 14);
  const vp8l = new Uint8Array([...head('VP8L'), 0x2f, bits & 0xff, (bits >> 8) & 0xff, (bits >> 16) & 0xff, (bits >> 24) & 0xff, 0, 0, 0, 0]);
  expect(imageSize(vp8l)).toEqual({ width: 100, height: 50, type: 'webp' });
  const vp8x = new Uint8Array([...head('VP8X'), 0, 0, 0, 0, 0x3f, 0x1f, 0, 0x0f, 0x27, 0]);
  expect(imageSize(vp8x)).toEqual({ width: 8000, height: 10000, type: 'webp' });
});

test('не картинка — null', () => {
  expect(imageSize(bytes('GIF89a', [1, 0, 1, 0]))).toBeNull();
  expect(imageSize(new Uint8Array([]))).toBeNull();
});
````

Run: `cd apps/web && npm run unit`
Expected: FAIL — `Cannot find module '../src/ui/clipboard'` и `'../src/core/image-size'`.

- [ ] **Step 2: Форматы буфера и размер по заголовку**

`apps/web/src/ui/clipboard.ts`:

````ts
// Системный буфер обмена (спека 7.7: работа интерфейса). Ctrl/Cmd+C, X, V идут
// событиями `copy`, `cut`, `paste` документа. Байты формата Ferrum готовит
// worker (событие `clipboard`), разбирает при вставке тоже worker — основной поток
// только переносит байты.
//
// Запись: где асинхронный Clipboard API умеет свои форматы
// (`ClipboardItem.supports('web application/x-ferrum')`), байты кладутся под
// `web application/x-ferrum`. Иначе — запасной путь: `document.execCommand('copy')`
// вызывает ещё одно событие `copy`, и в его `clipboardData` байты уходят текстом
// base64 за меткой `FERRUM_TEXT_MARKER`.
//
// Вставка: сначала `clipboardData` события (метка в тексте, файлы картинок), затем
// асинхронное чтение своего формата. Картинки из других приложений (PNG, JPEG,
// WebP) ставятся узлом-изображением; не-PNG перекодирует worker.

/** Свой формат в асинхронном Clipboard API (префикс `web ` обязателен). */
export const FERRUM_MIME = 'web application/x-ferrum';
/** Метка байтов Ferrum в `text/plain` (запасной путь). */
export const FERRUM_TEXT_MARKER = 'ferrum-clipboard:v1:';
/** Предел байтов из буфера — как у команды `paste` (64 МБ). */
export const MAX_CLIPBOARD_BYTES = 64 * 1024 * 1024;
/** Картинки, которые вставляются узлом-изображением. */
export const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/webp'] as const;

/** base64 без переполнения стека на больших массивах. */
export function toBase64(bytes: Uint8Array): string {
  let s = '';
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    s += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(s);
}

export function fromBase64(text: string): Uint8Array | null {
  try {
    const bin = atob(text);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  } catch {
    return null;
  }
}

/** Байты Ferrum → текст для запасного пути. */
export function encodeText(bytes: Uint8Array): string {
  return FERRUM_TEXT_MARKER + toBase64(bytes);
}

/** Текст из буфера → байты Ferrum; `null` — не наш текст или он испорчен. */
export function decodeText(text: string): Uint8Array | null {
  if (!text.startsWith(FERRUM_TEXT_MARKER)) return null;
  const body = text.slice(FERRUM_TEXT_MARKER.length).trim();
  // base64 длиннее байтов на треть: дальше предела не декодируем.
  if (body.length > Math.ceil((MAX_CLIPBOARD_BYTES * 4) / 3) + 4) return null;
  return fromBase64(body);
}

/** Что вставить. */
export type PasteData = { kind: 'ferrum'; bytes: Uint8Array } | { kind: 'image'; type: string; blob: Blob } | null;

/** Выбор из `clipboardData` события вставки: свои байты (метка в тексте) важнее
 * картинки — Ferrum может положить и то и другое. */
export function fromDataTransfer(dt: Pick<DataTransfer, 'getData' | 'files'> | null): PasteData {
  if (!dt) return null;
  const bytes = decodeText(dt.getData('text/plain') ?? '');
  if (bytes) return { kind: 'ferrum', bytes };
  for (const f of Array.from(dt.files ?? [])) {
    if ((IMAGE_TYPES as readonly string[]).includes(f.type)) return { kind: 'image', type: f.type, blob: f };
  }
  return null;
}

/** Выбор из элементов асинхронного Clipboard API. */
export async function fromClipboardItems(items: readonly Pick<ClipboardItem, 'types' | 'getType'>[]): Promise<PasteData> {
  for (const item of items) {
    if (item.types.includes(FERRUM_MIME)) {
      const blob = await item.getType(FERRUM_MIME);
      if (blob.size > MAX_CLIPBOARD_BYTES) return null;
      return { kind: 'ferrum', bytes: new Uint8Array(await blob.arrayBuffer()) };
    }
    if (item.types.includes('text/plain')) {
      const bytes = decodeText(await (await item.getType('text/plain')).text());
      if (bytes) return { kind: 'ferrum', bytes };
    }
    const image = item.types.find((t) => (IMAGE_TYPES as readonly string[]).includes(t));
    if (image) return { kind: 'image', type: image, blob: await item.getType(image) };
  }
  return null;
}

/** Умеет ли асинхронный API записывать свой формат. */
export function supportsCustomFormat(): boolean {
  const C = (globalThis as { ClipboardItem?: { supports?: (t: string) => boolean } }).ClipboardItem;
  try {
    return typeof C?.supports === 'function' && C.supports(FERRUM_MIME);
  } catch {
    return false;
  }
}
````

`apps/web/src/core/image-size.ts`:

````ts
// Размер картинки по заголовку — до декодирования. Вставленный из буфера JPEG или
// WebP в несколько килобайт может объявить 60 000 × 60 000 пикселей: декодировать
// его, чтобы узнать размер, значит отдать worker'у гигабайты памяти. Поэтому
// размер читается из заголовка (PNG — IHDR, JPEG — маркер SOF, WebP — VP8/VP8L/VP8X).

export interface ImageSize {
  width: number;
  height: number;
  type: 'png' | 'jpeg' | 'webp';
}

const be16 = (b: Uint8Array, i: number) => (b[i] << 8) | b[i + 1];
const le16 = (b: Uint8Array, i: number) => b[i] | (b[i + 1] << 8);
const le24 = (b: Uint8Array, i: number) => b[i] | (b[i + 1] << 8) | (b[i + 2] << 16);
const be32 = (b: Uint8Array, i: number) => ((b[i] << 24) >>> 0) + ((b[i + 1] << 16) | (b[i + 2] << 8) | b[i + 3]);
const ascii = (b: Uint8Array, i: number, s: string) => [...s].every((c, k) => b[i + k] === c.charCodeAt(0));

/** Маркеры JPEG с размером кадра (SOF0–SOF15 без DHT, JPG и DAC). */
const SOF = new Set([0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf]);

function jpeg(b: Uint8Array): ImageSize | null {
  let i = 2;
  while (i + 9 < b.length) {
    if (b[i] !== 0xff) return null;
    const m = b[i + 1];
    if (m === 0xff) {
      i++;
      continue;
    }
    if (SOF.has(m)) return { width: be16(b, i + 7), height: be16(b, i + 5), type: 'jpeg' };
    // Маркеры без длины: RST, SOI, EOI, TEM.
    if ((m >= 0xd0 && m <= 0xd9) || m === 0x01) {
      i += 2;
      continue;
    }
    i += 2 + be16(b, i + 2);
  }
  return null;
}

function webp(b: Uint8Array): ImageSize | null {
  if (ascii(b, 12, 'VP8 ') && b.length >= 30) {
    return { width: le16(b, 26) & 0x3fff, height: le16(b, 28) & 0x3fff, type: 'webp' };
  }
  if (ascii(b, 12, 'VP8L') && b.length >= 25) {
    const width = 1 + (((b[22] & 0x3f) << 8) | b[21]);
    const height = 1 + (((b[24] & 0x0f) << 10) | (b[23] << 2) | ((b[22] & 0xc0) >> 6));
    return { width, height, type: 'webp' };
  }
  if (ascii(b, 12, 'VP8X') && b.length >= 30) return { width: 1 + le24(b, 24), height: 1 + le24(b, 27), type: 'webp' };
  return null;
}

/** Размер по заголовку; `null` — не PNG, JPEG или WebP либо заголовок испорчен. */
export function imageSize(b: Uint8Array): ImageSize | null {
  if (b.length >= 24 && b[0] === 0x89 && ascii(b, 1, 'PNG') && ascii(b, 12, 'IHDR')) {
    return { width: be32(b, 16), height: be32(b, 20), type: 'png' };
  }
  if (b.length >= 4 && b[0] === 0xff && b[1] === 0xd8) return jpeg(b);
  if (b.length >= 16 && ascii(b, 0, 'RIFF') && ascii(b, 8, 'WEBP')) return webp(b);
  return null;
}
````

Run: `cd apps/web && npm run unit`
Expected: PASS, `35 passed`.

- [ ] **Step 3: Перекодирование картинок в worker'е**

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
import { commandSizeError, isToWorker, type FromWorker, type ToWorker } from '../core/messages';
import { drainRing, writeLive as writeLiveSlots } from '../core/shared';
````

на:

````ts
import { commandSizeError, isToWorker, type FromWorker, type ToWorker } from '../core/messages';
import { imageSize } from '../core/image-size';
import { drainRing, writeLive as writeLiveSlots } from '../core/shared';
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  post({ t: 'events', events: [{ type: 'error', message }] });
}

/** Байты событий уходят без копии: их буферы передаются основному потоку. */
````

на:

````ts
  post({ t: 'events', events: [{ type: 'error', message }] });
}

/** Наибольшая сторона картинки, как у `decode_png` в ядре. */
const MAX_IMAGE_SIDE = 8192;

/** Картинка не PNG (JPEG, WebP из буфера обмена или файла) — перекодируется в
 * PNG здесь, в worker'е: ядро читает только PNG, а основной поток пиксели не трогает.
 * Размер проверяется по заголовку до декодирования: крошечный файл может объявить
 * гигапиксели. */
async function toPng(bytes: Uint8Array): Promise<Uint8Array> {
  const size = imageSize(bytes);
  if (!size) throw new Error('нужен PNG, JPEG или WebP');
  if (size.width > MAX_IMAGE_SIDE || size.height > MAX_IMAGE_SIDE) {
    throw new Error(`изображение больше ${MAX_IMAGE_SIDE} px`);
  }
  const bmp = await createImageBitmap(new Blob([bytes as BlobPart]));
  try {
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    c.getContext('2d')!.drawImage(bmp, 0, 0);
    return new Uint8Array(await (await c.convertToBlob({ type: 'image/png' })).arrayBuffer());
  } finally {
    bmp.close();
  }
}

/** Пока картинка перекодируется, следующие команды ждут: порядок команд — порядок
 * пользователя (вставка, затем отмена — отменяется вставка). */
let converting = false;
const waiting: ToWorker[] = [];

/** Байты событий уходят без копии: их буферы передаются основному потоку. */
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
      case 'cmd': {
        // Предел размера проверяем и здесь: основной поток не доверенный.
````

на:

````ts
      case 'cmd': {
        if (converting) {
          waiting.push(msg);
          return;
        }
        // Предел размера проверяем и здесь: основной поток не доверенный.
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
        }
        core.command(msg.cmd);
        // Ответы на команды (сохранение, ошибки) — сразу, кадр нарисуется по тику.
````

на:

````ts
        }
        const cmd = msg.cmd;
        // Команда не доверенная: не объект — её отвергнет разбор в ядре.
        const image = typeof cmd === 'object' && cmd !== null && (cmd.type === 'pasteImage' || cmd.type === 'placeImage');
        if (image && cmd.bytes instanceof Uint8Array && imageSize(cmd.bytes)?.type !== 'png') {
          converting = true;
          toPng(cmd.bytes)
            .then(
              (png) => {
                if (!core) return;
                core.command({ ...cmd, bytes: png });
                sendEvents();
              },
              (e: unknown) => postError(`Не удалось прочитать изображение: ${e instanceof Error ? e.message : 'формат не поддерживается'}`),
            )
            .finally(() => {
              converting = false;
              // Ждавшие команды — по порядку; новая перекодировка снова остановит очередь.
              while (waiting.length && !converting) handle(waiting.shift()!);
            });
          return;
        }
        core.command(cmd);
        // Ответы на команды (сохранение, ошибки) — сразу, кадр нарисуется по тику.
````

В `apps/web/src/core/messages.ts` заменить:

````ts
  placeImage: 64 * 1024 * 1024,
} as const;
````

на:

````ts
  placeImage: 64 * 1024 * 1024,
  pasteImage: 64 * 1024 * 1024,
} as const;
````

В `apps/web/src/core/messages.ts` заменить:

````ts
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type !== 'open' && cmd.type !== 'paste' && cmd.type !== 'placeImage') return null;
  const bytes = (cmd as { bytes: unknown }).bytes;
````

на:

````ts
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type !== 'open' && cmd.type !== 'paste' && cmd.type !== 'placeImage' && cmd.type !== 'pasteImage') return null;
  const bytes = (cmd as { bytes: unknown }).bytes;
````

В `apps/web/tsconfig.worker.json` заменить:

````json
  },
  "include": ["src/worker/**/*.ts", "src/core/layout.gen.ts", "src/core/messages.ts"],
  "exclude": []
````

на:

````json
  },
  "include": ["src/worker/**/*.ts", "src/core/layout.gen.ts", "src/core/messages.ts", "src/core/image-size.ts"],
  "exclude": []
````

- [ ] **Step 4: Системный буфер обмена и файлы**

`apps/web/src/ui/files.ts` (целиком):

````ts
// Файлы в браузере (этап 5 — минимум): выбрать файл («Открыть…», изображение) —
// `showOpenFilePicker`, где он есть, иначе `<input type=file>`; скачать байты
// («Сохранить как…», экспорт PNG). Байты сразу уходят в worker. Недавние файлы,
// IndexedDB, автосохранение и экран файлов — этап 6.

/** Скачивает байты файлом `name` (ссылка с `download`, адрес освобождается). */
export function downloadBytes(name: string, bytes: Uint8Array, type: string): void {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

export interface FileKind {
  description: string;
  /** MIME → расширения (`{'image/png': ['.png']}`). */
  accept: Record<string, string[]>;
}

export const FERRUM_FILE: FileKind = { description: 'Файл Ferrum', accept: { 'application/x-ferrum': ['.ferrum'] } };
export const IMAGE_FILE: FileKind = {
  description: 'Изображение',
  accept: { 'image/png': ['.png'], 'image/jpeg': ['.jpg', '.jpeg'], 'image/webp': ['.webp'] },
};

type OpenPicker = (o: { types: FileKind[]; multiple?: boolean }) => Promise<{ getFile(): Promise<File> }[]>;

/** Выбор одного файла; `null` — пользователь отказался. */
export async function pickFile(kind: FileKind): Promise<File | null> {
  const picker = (window as unknown as { showOpenFilePicker?: OpenPicker }).showOpenFilePicker;
  if (picker) {
    try {
      const [handle] = await picker({ types: [kind] });
      return handle ? await handle.getFile() : null;
    } catch (e) {
      if (e instanceof DOMException && e.name === 'AbortError') return null;
      throw e;
    }
  }
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = Object.entries(kind.accept)
      .flatMap(([mime, exts]) => [mime, ...exts])
      .join(',');
    input.addEventListener('change', () => resolve(input.files?.[0] ?? null), { once: true });
    input.addEventListener('cancel', () => resolve(null), { once: true });
    input.click();
  });
}

/** Байты файла (передаются worker'у без копии). */
export async function fileBytes(file: Blob): Promise<Uint8Array> {
  return new Uint8Array(await file.arrayBuffer());
}
````

`apps/web/src/ui/system-clipboard.ts`:

````ts
// Связь ядра с системным буфером обмена и выбором файлов: события `copy`, `cut`,
// `paste` документа, события ядра `clipboard`, `clipboardPng`, `pasteRequested`,
// `requestImage`. Разбор форматов — в clipboard.ts, байты разбирает worker.

import type { Editor } from './context';
import {
  encodeText,
  FERRUM_MIME,
  fromClipboardItems,
  fromDataTransfer,
  MAX_CLIPBOARD_BYTES,
  supportsCustomFormat,
  type PasteData,
} from './clipboard';
import { fileBytes, IMAGE_FILE, pickFile } from './files';

/** Событие из поля ввода или из модального диалога — это обычный текст, не холст. */
function textTarget(t: EventTarget | null): boolean {
  if (document.querySelector('[role="dialog"][aria-modal="true"]')) return true;
  return t instanceof Element && !!t.closest('input, textarea, select, [contenteditable="true"]');
}

export function installClipboard(ed: Editor): () => void {
  // Запасной путь записи: байты ждут события `copy`, которое вызывает execCommand.
  let pending: Uint8Array | null = null;
  const toast = (m: string) => ed.store.toast(m, 'error');

  const writeFallback = (bytes: Uint8Array) => {
    pending = bytes;
    let ok = false;
    try {
      ok = document.execCommand('copy');
    } catch {
      ok = false;
    }
    if (!ok || pending) {
      pending = null;
      toast('Не удалось записать в буфер обмена. Браузер не дал доступа');
    }
  };

  const write = (bytes: Uint8Array) => {
    if (!supportsCustomFormat()) {
      writeFallback(bytes);
      return;
    }
    navigator.clipboard
      .write([new ClipboardItem({ [FERRUM_MIME]: new Blob([bytes as BlobPart], { type: FERRUM_MIME }) })])
      .catch(() => writeFallback(bytes));
  };

  const writePng = (bytes: Uint8Array) => {
    navigator.clipboard
      .write([new ClipboardItem({ 'image/png': new Blob([bytes as BlobPart], { type: 'image/png' }) })])
      .then(() => ed.store.toast('Скопировано как PNG'))
      .catch(() => toast('Не удалось скопировать картинку: браузер не дал доступа к буферу обмена'));
  };

  const readAsync = async (): Promise<PasteData> => {
    try {
      return await fromClipboardItems(await navigator.clipboard.read());
    } catch {
      toast('Нет доступа к буферу обмена. Вставьте сочетанием клавиш');
      return null;
    }
  };

  const deliver = async (data: PasteData) => {
    if (!data) return;
    if (data.kind === 'ferrum') {
      ed.send({ type: 'paste', bytes: data.bytes });
      return;
    }
    // Размер — до чтения: байты не копируются в память ради отказа.
    if (data.blob.size > MAX_CLIPBOARD_BYTES) {
      toast('Изображение слишком большое для вставки (больше 64 МБ)');
      return;
    }
    try {
      ed.send({ type: 'pasteImage', bytes: await fileBytes(data.blob) });
    } catch {
      toast('Не удалось прочитать изображение из буфера обмена');
    }
  };

  const onCopy = (command: 'edit.copy' | 'edit.cut') => (e: ClipboardEvent) => {
    if (pending && command === 'edit.copy') {
      e.clipboardData?.setData('text/plain', encodeText(pending));
      pending = null;
      e.preventDefault();
      return;
    }
    if (textTarget(e.target)) return;
    e.preventDefault();
    ed.run(command);
  };
  const copy = onCopy('edit.copy');
  const cut = onCopy('edit.cut');
  const paste = (e: ClipboardEvent) => {
    if (textTarget(e.target)) return;
    e.preventDefault();
    const sync = fromDataTransfer(e.clipboardData);
    void (sync ? deliver(sync) : readAsync().then(deliver));
  };
  document.addEventListener('copy', copy);
  document.addEventListener('cut', cut);
  document.addEventListener('paste', paste);
  const off = ed.core.on((e) => {
    if (e.type === 'clipboard') write(e.bytes);
    else if (e.type === 'clipboardPng') writePng(e.bytes);
    else if (e.type === 'pasteRequested') void readAsync().then(deliver);
    else if (e.type === 'requestImage') {
      pickFile(IMAGE_FILE)
        .then(async (f) => {
          if (!f) return;
          if (f.size > MAX_CLIPBOARD_BYTES) {
            toast('Изображение слишком большое (больше 64 МБ)');
            return;
          }
          ed.send({ type: 'placeImage', bytes: await fileBytes(f) });
        })
        .catch((err: unknown) =>
          toast(`Не удалось выбрать изображение: ${err instanceof Error ? err.message : String(err)}`),
        );
    }
  });
  return () => {
    document.removeEventListener('copy', copy);
    document.removeEventListener('cut', cut);
    document.removeEventListener('paste', paste);
    off();
  };
}
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { createEffect, createMemo, createSignal, on, onCleanup, onMount, Show } from 'solid-js';
import { boot } from './boot';
import { canvasMenu, DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { EditorContext, type Editor } from './context';
import { downloadBytes } from './files';
import { LayersPanel } from './layers';
````

на:

````tsx
import { createEffect, createMemo, createSignal, on, onCleanup, onMount, Show } from 'solid-js';
import { detectPlatform } from '../core/core';
import { boot } from './boot';
import { canvasMenu, DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { EditorContext, type Editor } from './context';
import { downloadBytes, FERRUM_FILE, fileBytes, pickFile } from './files';
import { LayersPanel } from './layers';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { SettingsDialog } from './settings-dialog';
import { createUiStore } from './store';
````

на:

````tsx
import { SettingsDialog } from './settings-dialog';
import { clipboardAction, eventShortcut } from './shortcuts';
import { installClipboard } from './system-clipboard';
import { createUiStore } from './store';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  const theme = createMemo(() => resolveTheme(themeChoice(), prefersDark()));
  let canvas!: HTMLCanvasElement;
````

на:

````tsx
  const theme = createMemo(() => resolveTheme(themeChoice(), prefersDark()));
  const platform = detectPlatform();
  let uninstall: (() => void) | undefined;
  let canvas!: HTMLCanvasElement;
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      },
    })
      .then((ed) => {
        setEditor(ed);
        canvas.focus();
        ed.core.on((e) => {
          if (e.type === 'exported') {
            downloadBytes(e.name, e.bytes, 'image/png');
            store.toast(`Экспортировано: ${e.name}`);
          }
````

на:

````tsx
      },
      // Ctrl/Cmd+C, X, V, пока они вызывают копирование, вырезание и вставку, идут
      // событиями буфера обмена (системный буфер), а не клавишами в ядро.
      isClipboardKey: (e) => {
        const s = eventShortcut(e);
        const action = s && clipboardAction(s, platform);
        if (!s || !action) return false;
        const key = { copy: 'edit.copy', cut: 'edit.cut', paste: 'edit.paste' }[action];
        return !!store.state.keymap.commands.find((c) => c.key === key)?.bindings.includes(s);
      },
    })
      .then((ed) => {
        setEditor(ed);
        canvas.focus();
        uninstall = installClipboard(ed);
        ed.core.on((e) => {
          if (e.type === 'exported') {
            downloadBytes(e.name, e.bytes, 'image/png');
            store.toast(`Экспортировано: ${e.name}`);
          } else if (e.type === 'saved') {
            downloadBytes(`${store.state.documentName || 'Без названия'}.ferrum`, e.bytes, 'application/octet-stream');
          }
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  });
  onCleanup(() => editor()?.core.dispose());

  const fileItems = (ed: Editor): MenuItem[] => [
    { label: 'Создать', onSelect: () => ed.send({ type: 'demo', nodes: 0 }) },
    { separator: true },
````

на:

````tsx
  });
  onCleanup(() => {
    uninstall?.();
    editor()?.core.dispose();
  });

  const openFile = async (ed: Editor) => {
    try {
      const f = await pickFile(FERRUM_FILE);
      if (f) ed.send({ type: 'open', bytes: await fileBytes(f) });
    } catch (e) {
      store.toast(`Не удалось открыть файл: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  };

  const fileItems = (ed: Editor): MenuItem[] => [
    { label: 'Создать', onSelect: () => ed.send({ type: 'demo', nodes: 0 }) },
    { label: 'Открыть…', onSelect: () => void openFile(ed) },
    { label: 'Сохранить как…', onSelect: () => ed.send({ type: 'save' }) },
    { separator: true },
````

`apps/web/e2e/clipboard.spec.ts`:

````ts
// Системный буфер обмена и файлы: копировать и вставить на другую страницу (свой
// формат в асинхронном API и запасной путь — текст с меткой в событии), вырезать,
// вставить картинку из другого приложения, копировать как PNG, обычное
// копирование в полях ввода, «Сохранить как…» и «Открыть…», изображение из файла.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

test.use({ permissions: ['clipboard-read', 'clipboard-write'] });

const ui = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!)));
const events = (page: Page, type: string) => page.evaluate((t) => window.__ferrum!.events.filter((e) => e.type === t).length, type);

/** Выделить «Слой 1» первого фрейма щелчком на холсте. */
async function selectShape(page: Page) {
  const at = await appScreen(page, await live(page), 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
}

async function copyToNewPage(page: Page) {
  await selectShape(page);
  await page.keyboard.press('ControlOrMeta+c');
  await expect.poll(() => events(page, 'clipboard')).toBe(1);
  await page.getByRole('button', { name: 'Добавить страницу' }).click();
  await expect.poll(async () => (await ui(page)).pages.list.length).toBe(2);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+v');
  await expect.poll(async () => (await ui(page)).layers.total).toBe(1);
  const s = (await ui(page)).summary;
  expect([s.count, s.kind, s.w.value, s.h.value]).toEqual([1, 'rect', 120, 80]);
}

test('копировать и вставить на другую страницу — свой формат', async ({ page }) => {
  await openApp(page);
  expect(await page.evaluate(() => ClipboardItem.supports('web application/x-ferrum'))).toBe(true);
  await copyToNewPage(page);
  const types = await page.evaluate(async () => (await navigator.clipboard.read())[0].types);
  expect(types).toContain('web application/x-ferrum');
});

test('запасной путь: текст с меткой в событии copy', async ({ page }) => {
  await page.addInitScript(() => {
    (ClipboardItem as unknown as { supports?: unknown }).supports = undefined;
  });
  await openApp(page);
  await copyToNewPage(page);
  const text = await page.evaluate(() => navigator.clipboard.readText());
  expect(text.startsWith('ferrum-clipboard:v1:')).toBe(true);
});

test('вырезать удаляет и кладёт в буфер', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  const before = await live(page);
  await page.keyboard.press('ControlOrMeta+x');
  await liveWhen(page, `l.nodes === ${before.nodes - 1} && l.selectionCount === 0`);
  await expect.poll(() => events(page, 'clipboard')).toBe(1);
  await page.keyboard.press('ControlOrMeta+v');
  await liveWhen(page, `l.nodes === ${before.nodes}`);
});

test('картинка из другого приложения вставляется узлом-изображением', async ({ page }) => {
  await openApp(page, 100);
  // PNG 3 × 2 и JPEG 5 × 4 — как их кладёт в буфер другое приложение.
  for (const [type, w, h] of [['image/png', 3, 2], ['image/jpeg', 5, 4]] as const) {
    await page.evaluate(
      async ({ type, w, h }) => {
        const c = new OffscreenCanvas(w, h);
        const ctx = c.getContext('2d')!;
        ctx.fillStyle = '#ff0000';
        ctx.fillRect(0, 0, w, h);
        const blob = await c.convertToBlob({ type });
        // Буфер Chromium принимает картинки только как PNG: JPEG кладём файлом в событие.
        if (type === 'image/png') {
          await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
          return;
        }
        const dt = new DataTransfer();
        dt.items.add(new File([blob], 'photo.jpg', { type }));
        document.querySelector('canvas')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
      },
      { type, w, h },
    );
    if (type === 'image/png') {
      await page.locator('canvas').focus();
      await page.keyboard.press('ControlOrMeta+v');
    }
    await expect.poll(async () => (await ui(page)).summary?.kind).toBe('image');
    const s = (await ui(page)).summary;
    expect([s.w.value, s.h.value]).toEqual([w, h]);
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await ui(page)).summary?.count).toBe(0);
  }
  expect((await ui(page)).layers.total).toBe(1 + 2);
});

test('копировать как PNG кладёт картинку в буфер', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  await page.keyboard.press('ControlOrMeta+Shift+c');
  await expect(page.getByRole('status').filter({ hasText: 'Скопировано как PNG' })).toBeVisible();
  const size = await page.evaluate(async () => {
    const item = (await navigator.clipboard.read())[0];
    const bmp = await createImageBitmap(await item.getType('image/png'));
    return [bmp.width, bmp.height];
  });
  // «Слой 1» 120 × 80, обводка внутри — размер не растёт.
  expect(size).toEqual([120, 80]);
});

test('в поле ввода копирование — обычный текст', async ({ page }) => {
  await openApp(page);
  const search = page.getByRole('searchbox', { name: 'Поиск слоёв' });
  await search.fill('Экран');
  await search.selectText();
  await page.keyboard.press('ControlOrMeta+c');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Экран');
  expect(await events(page, 'clipboard')).toBe(0);
});

test('«Сохранить как…» и «Открыть…»', async ({ page }) => {
  await page.addInitScript(() => {
    delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker;
  });
  await openApp(page, 300);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const download = page.waitForEvent('download');
  await page.getByRole('menuitem', { name: 'Сохранить как…' }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('Без названия.ferrum');
  const bytes = Buffer.concat(await (await file.createReadStream()).toArray());
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: 'Создать' }).click();
  await expect.poll(async () => (await ui(page)).layers.total).toBe(0);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('menuitem', { name: 'Открыть…' }).click();
  await (await chooser).setFiles({ name: 'проект.ferrum', mimeType: 'application/octet-stream', buffer: bytes });
  await expect.poll(async () => (await ui(page)).layers.total).toBe(3);
  await liveWhen(page, 'l.nodes === 300');
});

test('изображение из файла: Ctrl+Shift+K, выбор, щелчок на холсте', async ({ page }) => {
  await page.addInitScript(() => {
    delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker;
  });
  await openApp(page, 0);
  const png = await page.evaluate(async () => {
    const c = new OffscreenCanvas(40, 30);
    c.getContext('2d')!.fillRect(0, 0, 40, 30);
    return Array.from(new Uint8Array(await (await c.convertToBlob({ type: 'image/png' })).arrayBuffer()));
  });
  await page.locator('canvas').focus();
  const chooser = page.waitForEvent('filechooser');
  await page.keyboard.press('ControlOrMeta+Shift+k');
  await (await chooser).setFiles({ name: 'logo.png', mimeType: 'image/png', buffer: Buffer.from(png) });
  const box = (await page.locator('canvas').boundingBox())!;
  await page.waitForTimeout(200);
  await page.mouse.click(box.x + 200, box.y + 200);
  await expect.poll(async () => (await ui(page)).summary?.kind).toBe('image');
  expect((await ui(page)).summary.w.value).toBe(40);
});

test('огромная картинка отклоняется по заголовку, команды после неё — по порядку', async ({ page }) => {
  await openApp(page, 100);
  const before = await live(page);
  // JPEG в несколько байт объявляет 60 000 × 60 000: декодировать его нельзя.
  await page.evaluate(() => {
    const app0 = [0xff, 0xe0, 0, 16, ...new Array(14).fill(0)];
    const sof = [0xff, 0xc0, 0, 17, 8, 0xea, 0x60, 0xea, 0x60, 3, ...new Array(9).fill(0)];
    const bytes = new Uint8Array([0xff, 0xd8, ...app0, ...sof, 0xff, 0xd9]);
    window.__ferrum!.core.send({ type: 'pasteImage', bytes });
    // Следом — команда, которая не должна обогнать вставку.
    window.__ferrum!.core.send({ type: 'execute', command: 'view.zoom_in' });
  });
  await expect(page.getByRole('status').filter({ hasText: 'изображение больше 8192 px' })).toBeVisible();
  await liveWhen(page, `l.zoom === ${before.zoom * 2}`);
  expect((await live(page)).nodes).toBe(before.nodes);
});
````

Run: `cd apps/web && npm run typecheck && npm run build && npx playwright test e2e/clipboard.spec.ts e2e/core.spec.ts --project=chromium`
Expected: PASS, `26 passed` (`clipboard.spec.ts` — 9; `core.spec.ts` — как в этапе 4: неверный конверт сообщения worker'у не роняет ядро).

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web
git commit -m "feat(web): системный буфер обмена, картинки из буфера, открыть и сохранить файл"
```

---

### Task 13: Бюджет основного потока интерфейса, доступность, финальная проверка

**Замер** (`perf.spec.ts`, проект `chromium`): редактор на 100 000 узлах, окно 1440 × 900, все панели открыты, в дереве раскрыты десять фреймов, выделено 1000 строк (10 фреймов и 990 их детей) — выделение из панели диапазоном, панель свойств показывает сводку. Затем 3 секунды выделение перетаскивается на холсте, а дерево слоёв прокручивается (на 37 строк за шаг). Ворота: длинных задач (> 50 мс) нет; основной поток — обработчики, планировщик, обновления Solid — не дольше 4 мс за худший кадр; в DOM дерева не больше строк окна (`ceil(900 / 28) + 20 + 1`); потерь ввода нет; жест записан одним шагом; рамка выделения из общей памяти доходила до интерфейса во время жеста (больше 10 разных значений). Результат — в `test-results/chromium/perf-ui.json`. Машина под нагрузкой (Chrome, OrbStack) даёт единичные выбросы: при провале ворот повторить прогон, прежде чем считать это регрессией; ворота не ослаблять.

**Доступность** (`a11y.spec.ts`): роли toolbar, tree, tablist, listbox, menu, dialog; у каждой кнопки есть имя; контраст по токенам в обеих темах — `ink` и `ink-muted` на `surface`, `surface-raised`, `surface-hover` не ниже 4,5:1, кольцо `select` не ниже 3:1, текст на `oxide` — 4,5:1; при `prefers-reduced-motion` анимаций нет.

**Files:**
- Modify: `apps/web/e2e/perf.spec.ts`
- Test: `apps/web/e2e/a11y.spec.ts`

**Interfaces:**
- Consumes: всё интерфейса (Tasks 6–12), `Core.mainThreadStats`, `droppedInput`.
- Produces: тест `интерфейс на 100 000 узлах: перетаскивание 1000 выделенных и прокрутка слоёв @perf`; `a11y.spec.ts`.

- [ ] **Step 1: Замер интерфейса на 100 000 узлах**

В `apps/web/e2e/perf.spec.ts` заменить:

````ts
import path from 'node:path';
import { live, screen } from './helpers';

````

на:

````ts
import path from 'node:path';
import { live, liveWhen, screen } from './helpers';

````

В `apps/web/e2e/perf.spec.ts` заменить:

````ts
    }
  });
}
````

на:

````ts
    }
  });
}

// --- Интерфейс (этап 5) -------------------------------------------------------
//
// Редактор с панелями на 100 000 узлах: дерево слоёв раскрыто на десяти фреймах,
// выделено 1000 строк (10 фреймов и 990 их детей), панель свойств показывает
// сводку выделения. 3 секунды выделение перетаскивается на холсте, а дерево
// слоёв прокручивается. Ворота: длинных задач нет, основной поток (обработчики,
// планировщик, обновления Solid) — не дольше 4 мс за кадр, в DOM дерева — только
// окно строк, панель получает живые значения жеста.

/** Наибольшее окно строк: экран 900 px / 28 px + запас по 10 строк сверху и снизу. */
const MAX_TREE_ROWS = Math.ceil(900 / 28) + 20 + 1;

test('интерфейс на 100 000 узлах: перетаскивание 1000 выделенных и прокрутка слоёв @perf', async ({ page }, info) => {
  test.skip(info.project.name !== 'chromium', 'замер интерфейса — в проекте chromium');
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.addInitScript(() => {
    const w = window as unknown as { __longTasks: { start: number; duration: number }[] };
    w.__longTasks = [];
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) w.__longTasks.push({ start: e.startTime, duration: e.duration });
    }).observe({ type: 'longtask', buffered: true });
  });
  await page.goto(`/?nodes=${NODES}`);
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true, null, { timeout: 120_000 });
  const tree = page.getByRole('tree', { name: 'Слои' });
  // Раскрыть десять верхних фреймов («Экран 999» … «Экран 990»): первый — кнопкой
  // строки, остальные — той же командой (их строки уходят за окно списка).
  await tree.getByRole('treeitem', { name: 'Экран 999', exact: true }).getByRole('button', { name: 'Развернуть' }).click();
  await page.evaluate(() => {
    const rows = window.__ferrum!.ui!.layers.rows.filter((r) => r.depth === 0).slice(0, 10);
    for (const r of rows) window.__ferrum!.core.send({ type: 'expandLayer', id: r.id, expanded: true });
  });
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(1000 + 990);
  // Выделить строки 0 … 999 (10 фреймов и 990 их детей): первая — щелчком,
  // последняя («Слой 0» фрейма «Экран 990») — с Shift после прокрутки.
  await tree.getByRole('treeitem', { name: 'Экран 999', exact: true }).click();
  await tree.evaluate((el) => (el.scrollTop = 999 * 28 - 400));
  const last = tree.locator(`[role="treeitem"][style*="top: ${999 * 28}px"]`);
  await expect(last).toHaveAccessibleName('Слой 0');
  await last.click({ modifiers: ['Shift'] });
  await liveWhen(page, 'l.selectionCount === 1000');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.summary?.count)).toBe(1000);
  // Выделение — в кадр, затем жест из центра холста (внутри общей рамки).
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+Digit2');
  await page.waitForTimeout(500);
  const box = (await page.locator('canvas').boundingBox())!;
  const from = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const start = await page.evaluate(() => {
    window.__ferrum!.core.resetMainThreadStats();
    return performance.now();
  });
  const steps0 = (await live(page)).steps;
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  let maxRows = 0;
  // Значения живого состояния, которые дошли до панели (общая рамка выделения).
  const liveX = new Set<number>();
  const end = Date.now() + 3000;
  for (let i = 0; Date.now() < end; i++) {
    await page.mouse.move(from.x + (i % 60) * 3, from.y + Math.sin(i / 8) * 30);
    // Прокрутка дерева слоёв во время жеста.
    const rows = await tree.evaluate((el, i) => {
      el.scrollTop = ((i * 37) % 900) * 28;
      return el.querySelectorAll('[role="treeitem"]').length;
    }, i);
    maxRows = Math.max(maxRows, rows);
    liveX.add(await page.evaluate(() => window.__ferrum!.ui!.live.x));
    await page.waitForTimeout(16);
  }
  await page.mouse.up();
  const after = await liveWhen(page, `l.steps === ${steps0 + 1} && l.busy === 0`);
  const sample = await page.evaluate((start) => {
    const w = window as unknown as { __longTasks: { start: number; duration: number }[] };
    const main = window.__ferrum!.core.mainThreadStats();
    return {
      longTasks: w.__longTasks
        .filter((t) => t.start >= start && t.duration > 50)
        .map((t) => ({ startMs: Math.round(t.start - start), durationMs: Math.round(t.duration) })),
      mainMaxMs: main.maxMs,
      mainP95Ms: main.p95Ms,
      mainFrames: main.frames,
      dropped: window.__ferrum!.core.droppedInput(),
    };
  }, start);
  const out = path.join(info.project.outputDir, 'perf-ui.json');
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, JSON.stringify({ ...sample, maxRows, liveX: liveX.size }, null, 2));
  console.log(`perf ui: ${JSON.stringify({ ...sample, maxRows, liveX: liveX.size })}`);
  expect(after.nodes).toBe(NODES);
  expect(sample.mainFrames).toBeGreaterThan(100);
  expect(sample.longTasks, 'длинные задачи в основном потоке').toEqual([]);
  expect(sample.mainMaxMs, 'обработчики, планировщик и Solid за кадр').toBeLessThanOrEqual(MAIN_BUDGET_MS);
  expect(sample.dropped).toBe(0);
  expect(maxRows, 'строк дерева в DOM').toBeLessThanOrEqual(MAX_TREE_ROWS);
  // Рамка выделения во время жеста доходила до интерфейса из общей памяти.
  expect(liveX.size, 'живые значения жеста в панели').toBeGreaterThan(10);
});
````

Run: `cd apps/web && npm run build && npx playwright test e2e/perf.spec.ts -g "интерфейс"`
Expected: PASS, `1 passed`; строка `perf ui: {…}` — длинных задач `[]`, `mainMaxMs` ≤ 4, `maxRows` ≤ 54.

- [ ] **Step 2: Доступность**

`apps/web/e2e/a11y.spec.ts`:

````ts
// Доступность: роли основных виджетов, имена у всех кнопок, контраст текста по
// токенам в обеих темах, кольцо фокуса, prefers-reduced-motion.

import { expect, test, type Page } from '@playwright/test';
import { openApp } from './helpers';

/** Контраст двух цветов `#rgb` / `#rrggbb` по WCAG (сборка сокращает `#ffffff`). */
function contrast(a: string, b: string): number {
  const lum = (text: string) => {
    const hex = text.length === 4 ? `#${[...text.slice(1)].map((c) => c + c).join('')}` : text;
    const [r, g, bl] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    const f = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(bl);
  };
  const [x, y] = [lum(a), lum(b)].sort((m, n) => n - m);
  return (x + 0.05) / (y + 0.05);
}

const token = (page: Page, name: string) =>
  page.evaluate((n) => getComputedStyle(document.documentElement).getPropertyValue(`--${n}`).trim(), name);

test('роли: тулбар, дерево, вкладки, список страниц, меню, диалог', async ({ page }) => {
  await openApp(page);
  await expect(page.getByRole('toolbar', { name: 'Инструменты' })).toBeVisible();
  await expect(page.getByRole('tree', { name: 'Слои' })).toBeVisible();
  await expect(page.getByRole('tablist', { name: 'Режим панели' })).toBeVisible();
  await expect(page.getByRole('listbox', { name: 'Страницы' })).toBeVisible();
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await expect(page.getByRole('menu', { name: 'Меню файла' })).toBeVisible();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  await expect(page.getByRole('dialog', { name: 'Настройки' })).toBeVisible();
});

test('у каждой кнопки есть имя', async ({ page }) => {
  await openApp(page);
  await page.getByRole('treeitem', { name: 'Экран 9', exact: true }).click();
  const unnamed = await page.evaluate(() =>
    Array.from(document.querySelectorAll('button')).filter(
      (b) => !(b.getAttribute('aria-label') || b.textContent?.trim()),
    ).map((b) => b.outerHTML.slice(0, 80)),
  );
  expect(unnamed).toEqual([]);
});

for (const theme of ['dark', 'light'] as const) {
  test(`контраст текста и кольца фокуса по токенам (${theme})`, async ({ page }) => {
    await openApp(page, 0);
    await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
    const t = Object.fromEntries(
      await Promise.all(
        ['ink', 'ink-muted', 'surface', 'surface-raised', 'surface-hover', 'canvas', 'select', 'oxide', 'on-oxide'].map(
          async (n) => [n, await token(page, n)] as const,
        ),
      ),
    );
    for (const bg of ['surface', 'surface-raised', 'surface-hover']) {
      expect(contrast(t.ink, t[bg]), `ink на ${bg}`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(t['ink-muted'], t[bg]), `ink-muted на ${bg}`).toBeGreaterThanOrEqual(4.5);
    }
    expect(contrast(t.ink, t.canvas)).toBeGreaterThanOrEqual(4.5);
    // Кольцо фокуса select — не ниже 3:1 на поверхностях.
    for (const bg of ['surface', 'surface-raised']) expect(contrast(t.select, t[bg])).toBeGreaterThanOrEqual(3);
    expect(contrast(t['on-oxide'], t.oxide)).toBeGreaterThanOrEqual(4.5);
  });
}

test('prefers-reduced-motion: без анимаций', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await openApp(page);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const anim = await page.getByRole('menu').evaluate((el) => getComputedStyle(el).animationName);
  expect(anim).toBe('none');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Меню файла' }).click();
  expect(await page.getByRole('menu').evaluate((el) => getComputedStyle(el).animationName)).toBe('fe-pop');
});
````

Run: `cd apps/web && npx playwright test e2e/a11y.spec.ts`
Expected: PASS, `5 passed`.

- [ ] **Step 3: Полная проверка**

Все ворота этапов 1–5.

Run: `cargo fmt --check`
Expected: без вывода, код 0.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: без предупреждений.

Run: `cargo clippy -p ferrum-render --all-targets --features gpu -- -D warnings`
Expected: без предупреждений.

Run: `cargo clippy --target wasm32-unknown-unknown -p ferrum-wasm --lib --tests -- -D warnings`
Expected: без предупреждений.

Run: `cargo test --workspace`
Expected: PASS — все наборы этапов 1–5 (56 наборов, 460 тестов).

Run: `cargo test --target wasm32-unknown-unknown -p ferrum-wasm --test web`
Expected: PASS, `10 passed`.

Run: `cargo bench --workspace --no-run`
Expected: сборка бенчмарков без ошибок.

Run: `cargo run --release -p ferrum-render --example render_report`
Expected: все строки PASS, код 0.

Run: `cargo run --release -p ferrum-editor --example editor_report`
Expected: все строки PASS, код 0.

Run: `cargo run --release -p ferrum-doc --example bench_report`
Expected: все строки PASS, код 0.

Run: `cd apps/web && npm run typecheck && npm run unit && npm run wasm && npm run build && npx playwright test`
Expected: PASS — модульные `35 passed`; E2E — `117 passed`, 5 пропущено (проектные пропуски, как в этапе 4).

- [ ] **Step 4: Закоммитить**

```bash
git add apps/web/e2e
git commit -m "test(web): бюджет основного потока интерфейса на 100 000 узлах и доступность"
```

---

## Что этап 5 сознательно не делает

- **Хранение файлов.** Экран файлов, недавние, IndexedDB, File System Access как хранилище, автосохранение, журнал и воркер ввода-вывода — этап 6. «Открыть…» читает выбранный файл, «Сохранить как…» скачивает байты `.ferrum` из worker'а; настройки пользователя пока в `localStorage`.
- **Tauri и восстановление после сбоев.** Перезапуск worker'а, восстановление из журнала, пересоздание устройства GPU — этап 6; этап 5 показывает остановку ядра постоянной плашкой.
- **Подсистемы 2–7.** Текст, перо, булевы операции (кнопки «Перо» и «Текст» отключены), комментарии и совместная работа («Комментарии» и «Поделиться» отключены), автолейаут, компоненты, эффекты, градиенты (секций «Автолейаут» и «Эффекты» нет), прототипы (вкладка «Прототип» отключена), плагины, импорт `.fig`, палитра команд.
- **Заливка изображением в панели.** Показывается подписью («Изображение · Заполнить») без уменьшенной копии; выбрать другую картинку или режим нельзя. Уменьшенные копии готовит worker — вместе с декодированием под зум.
- **Контекстное меню по объекту под курсором.** Меню работает с текущим выделением; правая кнопка мыши не выделяет объект, как в Figma.
- **Отмена операций со страницами.** Как и в этапе 1, страницы и их фон в историю не попадают; удаление — после подтверждения.
- **Автопрокрутка дерева слоёв при перетаскивании строки** у краёв списка и фильтры поиска по типу слоя.
- **Подписи сочетаний по раскладке пользователя** — только где есть `navigator.keyboard.getLayoutMap()` (Chromium); в остальных браузерах — подписи US.
- **Firefox и Safari в E2E.** Запасной путь буфера обмена (`execCommand` и текст с меткой) проверен в Chromium с выключенным `ClipboardItem.supports`; в других браузерах — этап 6.
- **Свои пресеты фреймов и поиск по ним.** Список — встроенный JSON этапа 3.
