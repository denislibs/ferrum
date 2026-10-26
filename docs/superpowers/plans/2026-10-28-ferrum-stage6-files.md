# Ferrum, подсистема 1, этап 6 (часть 2): журнал, восстановление, диск и десктоп — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Довести хранение файлов до спеки (разделы 10–12): автосохранение пишет за миллисекунды пачки обновлений Loro в журнал файла (а полный файл — в простое и при уходе), после сбоя вкладки или паники ядра правки восстанавливаются из файла и журнала, ядро перезапускается само, потерянное устройство GPU пересоздаётся (не вышло — запасной рендер), испорченный файл открывается с понятным сообщением о потерянном; файлы на диске через File System Access (Chrome/Edge); настройки — в IndexedDB без затирания между вкладками; десктоп `apps/desktop` на Tauri 2 со своим хранилищем на диске (атомарная запись) и родными диалогами; E2E — круг «нарисовать → сохранить → перезагрузить → открыть → сравнить», сбой, журнал, Firefox.

**Architecture:** Модель (`ferrum-doc`) отдаёт пачку журнала — обновления Loro корня и загруженных страниц с прошлой пачки (`ExportMode::updates` от запомненной версии) и новые картинки — в контейнере того же вида, что `.ferrum`; при открытии пачки применяются поверх файла (`LoroDoc::import`), зеркало страницы перестраивается из Loro. Ядро worker'а отдаёт пачку по команде `flushJournal` и перед каждым снимком (номер пачки уходит в снимок); worker ввода-вывода дописывает её в хранилище `journal` IndexedDB (ключ `[файл, сеанс, пачка]`) и стирает пачки до номера снимка в той же транзакции, что пишет файл. Перезапуск ядра — новый экземпляр редактора с новым холстом (OffscreenCanvas второй раз не передать) для того же файла: байты и журнал читаются заново. Потеря GPU: worker создаёт поверхность заново на том же холсте (`WasmSurface.create` → `replaceSurface`), не вышло — перезапуск редактора с запасным бэкендом. File System Access — записи хранилища, связанные с `FileSystemFileHandle` (дескриптор хранится в IndexedDB, чтение и запись — в worker'е ввода-вывода). Tauri — своё хранилище на Rust (крейт `ferrum-desktop-store`: библиотека файлов, атомарная запись, журнал, превью, настройки), к которому интерфейс обращается командами IPC.

**Tech Stack:** Rust 1.96 (основной workspace — без новых крейтов), Loro 1.16.2 (`ExportMode::updates`, `oplog_vv`, `import`), wasm-bindgen 0.2.129; SolidJS 1.9.17, Vite 8.3.4, TypeScript 6.0.3, @playwright/test 1.64.0 (проекты `chromium`, `gpu`, новый `firefox`); десктоп — отдельный workspace `apps/desktop/src-tauri`: tauri 2.12.1, tauri-build 2.7.1, tauri-plugin-dialog 2.8.1, uuid 1.27.0, serde 1.0.229, serde_json 1.0.152; `@tauri-apps/cli` 2.12.1.

**Спека:** `docs/superpowers/specs/2026-10-23-ferrum-core-design.md`, разделы 4.2–4.3, 7.6, 9, 10, 11, 12. Часть 1 этапа 6 (экран файлов, IndexedDB в worker'е ввода-вывода, превью, полный снимок с ревизией и `MarkSaved`, Web Locks, маршруты) уже в `main` (PR #8, #9) — этот план строится поверх неё.

## Global Constraints

- Ветка `feat/stage6` от `main` (9edf571). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Каталог сборки у каждой копии свой: `CARGO_TARGET_DIR=<копия>/target` (для десктопа — `apps/desktop/src-tauri/target` внутри копии).
- Основной workspace — без новых крейтов; `Cargo.lock` корня не меняется. Десктоп — отдельный workspace `apps/desktop/src-tauri` со своим `Cargo.lock` (корневой `Cargo.toml` исключает каталог): прямые зависимости точными версиями, опубликованными до 2026-10-10 — `tauri =2.12.1`, `tauri-build =2.7.1`, `tauri-plugin-dialog =2.8.1`, `uuid =1.27.0`, `serde =1.0.229`, `serde_json =1.0.152`; `tauri-plugin-fs` (её тянет диалог) — `cargo update --precise 2.6.0`.
- npm: в `apps/web` новых пакетов нет (`package-lock.json` не меняется). `apps/desktop/package.json` — только `@tauri-apps/cli 2.12.1` (разработка), `.npmrc` как у `apps/web`: `save-exact=true`, `before=2026-10-10`. IPC Tauri — через `window.__TAURI_INTERNALS__.invoke` (то же, что внутри `@tauri-apps/api/core`), без пакета в `apps/web`.
- Основной поток тонкий (спека 4.2): байты файла, журнала и превью проходят его только передачей буферов (transfer), без копий и разбора; IndexedDB и File System Access — в worker'е ввода-вывода. Единственное исключение — Tauri: IPC есть только у окна, поэтому байты идут в Rust из основного потока тем же `ArrayBuffer` (сырое тело запроса IPC, без JSON и копий в JS), а запись и `fsync` — в потоке Tauri.
- Всё, что пересекает границу потоков и процессов, не доверенное и ограничено: пачка журнала ≤ 512 МБ, пачек при открытии ≤ 100 000, файл вместе с журналом ≤ 512 МБ; сеанс и номер пачки — целые ≥ 0 в пределах `Number.MAX_SAFE_INTEGER`; поля настроек — только из списка, запись ≤ 80 КБ; id записи Tauri — UUID; путей на диске интерфейс Tauri не передаёт вовсе (их дают только родные диалоги в Rust). Неверное — отказ на русском, не паника.
- Журнал пишется раньше файла: пачка с правками до снимка уходит перед ним; пачки до номера снимка стираются в той же транзакции IndexedDB, что пишет файл (на диске — после атомарной замены файла; оставшаяся пачка безвредна: повтор обновлений Loro пропускает).
- Каждая ошибка доходит до пользователя ровно один раз, на русском; тексты — в задачах дословно. Перезапуск ядра — не чаще двух раз за минуту, дальше — прежняя плашка остановки.
- Тесты не проходят вхолостую: восстановление сверяется с документом (страницы, узлы, родители, свойства, картинки), журнал — числом записей в хранилище, перезапуск — новым ядром и уведомлением, пиксели — снимком. Тестовые хуки — только в тестовом режиме (`__ferrumTest`), как `seed` и `failWrites` части 1, или в прологе worker'а из `fallback.spec.ts` (в бандле их нет).
- Тексты интерфейса, ошибок и комментарии — на русском.

## Проверка плана

Код плана проверен до его записи: все правки Tasks 1–14 применены скриптом к копии `main` (9edf571) в порядке задач — каждый фрагмент «заменить» найден ровно там, где указан, — и пройдены ворота: `cargo fmt --check`, три `clippy`, `cargo test --workspace` (548), wasm-тесты `web` (10), `journal_perf` и `snapshot_perf` в release, `npm run typecheck`, модульные (87), E2E `chromium` + `gpu` (282 passed), хранилище десктопа (9), сборка Tauri и проверка запуска (`smoke: {"files":1,"isolated":false,"ready":true}`), сборка `Ferrum.app`. Не запускались: проект `firefox` (Firefox для Playwright на машине не установлен — Task 13, Step 3) и ручные проверки Task 14, Step 3. Тексты ожидаемых результатов в задачах — с этого прогона.

---

## Этапы подсистемы 1

Подсистема 1 делится на шесть этапов (таблица — в плане этапа 1, `docs/superpowers/plans/2026-10-23-ferrum-stage1-document-model.md`). Этот документ — вторая часть плана этапа 6:

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1–5 (готовы) | модель, сцена и рендер, редактор, worker, интерфейс | 3–9 |
| 6, часть 1 (готова, PR #8, #9) | экран файлов, IndexedDB в worker'е ввода-вывода, превью, полный снимок, Web Locks, маршруты | 9, 10 |
| **6, часть 2** (этот план) | журнал и автосохранение, восстановление после сбоя и перезапуск ядра, потеря GPU, повреждённый файл, File System Access, настройки в IndexedDB, `apps/desktop` (Tauri 2), E2E круга сохранения, сбоя, журнала и Firefox | 10, 11, 12 |

---

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-doc/src/{container,document}.rs` | сообщения о потерянном в повреждённом файле (страница по имени, число картинок) |
| `crates/ferrum-doc/src/journal.rs` (новый) | формат пачки журнала, разбор, `Replayed` |
| `crates/ferrum-doc/src/{document,page,images,lib}.rs` | `take_journal`, `mark_journal`, `apply_journal`; версии журнала; новые картинки; обновления страницы и перестройка зеркала |
| `crates/ferrum-editor/src/editor.rs` | `take_journal`, `mark_journal` редактора |
| `crates/ferrum-editor/src/keymap.rs` | платформа в экспорте раскладки, перенос Cmd ↔ Ctrl при импорте с другой платформы |
| `crates/ferrum-wasm/src/{protocol,core}.rs`, `core/panels.rs` | `OpenWithJournal`, `FlushJournal`, события `Journal`, `Recovered`, номер пачки в `Snapshot`; события потери поверхности |
| `crates/ferrum-wasm/src/recover.rs` (новый) | политика потери GPU: пересоздать или запасной бэкенд |
| `crates/ferrum-wasm/src/web/mod.rs` | `WasmSurface`, `replaceSurface`, `surfaceFailed`, `deviceSize`; перехват паники с текстом; `panicForTests` |
| `apps/web/src/core/{limits,messages,file-list}.ts` | пределы журнала, конверт команд с журналом, запросы хранилища (журнал, настройки, диск), поля настроек |
| `apps/web/src/worker/{io,core}.worker.ts` | журнал, настройки и дескрипторы диска в IndexedDB (версии 2–4); пересоздание поверхности, тестовая паника |
| `apps/web/src/ui/{file-store,saver,boot,app,root,restart}.ts(x)` | журнал в хранилище, автосохранение журналом и полным файлом, восстановление при открытии, перезапуск редактора |
| `apps/web/src/ui/{settings,settings-store}.ts`, `public/theme-init.js` | настройки в IndexedDB по полям, перенос из `localStorage`, зеркало темы |
| `apps/web/src/ui/disk.ts`, `root.tsx`, `app.tsx`, `files-screen.tsx` | File System Access: «Открыть с диска…», «Сохранить на диск…», вопрос о доступе |
| `apps/web/src/ui/tauri.ts` (новый) | хранилище и настройки Tauri через IPC, родные диалоги, проверка запуска |
| `apps/desktop/src-tauri/store/` (новый крейт `ferrum-desktop-store`) | библиотека файлов на диске, атомарная запись, журнал, превью, настройки — без Tauri, тестируется `cargo test` |
| `apps/desktop/src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/,icons/,src/}` | приложение Tauri 2: команды IPC, диалоги, заголовки изоляции, проверка запуска |
| `apps/desktop/{package.json,.npmrc,scripts/make-icon.mjs}` | CLI Tauri, значок |
| `apps/web/e2e/{journal-store,journal,crash,fsa,settings-store,roundtrip}.spec.ts`, `fallback.spec.ts`, `helpers.ts`, `playwright.config.ts` | E2E этапа 6 и проект `firefox` |
| `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` | разделы 3, 4.2, 7.6, 9, 10, 11, 12 — сделанное и отложенное |

Все команды запускаются из корня копии (`files2-wt`), если не сказано иное; `export CARGO_TARGET_DIR=$PWD/target` — один раз в начале сеанса.

---

### Task 1: Повреждённый файл: что именно потеряно

Спека 11: «Файл повреждён: не удалось прочитать секцию изображений. Документ открыт без 2 картинок». Сейчас сообщения технические: «Страница 12@3 повреждена и открыта пустой» (идентификатор вместо имени) и «не удалось прочитать изображений — 1» (испорченные секции, а не потерянные картинки). После правки: страница — по имени; картинки — сколько картинок, на которые ссылаются страницы, потеряно из-за испорченных секций (в нужном падеже: «без 1 картинки», «без 2 картинок», «без 21 картинки»). Ссылка на картинку, которой не было и в документе, при целом файле — не потеря, сообщения нет.

**Files:**
- Modify: `crates/ferrum-doc/src/container.rs`
- Modify: `crates/ferrum-doc/src/document.rs`
- Test: `crates/ferrum-doc/tests/container.rs`

**Interfaces:**
- Consumes: `container::decode`, `Document::from_parts` этапа 1.
- Produces: тексты предупреждений `Opened::warnings`: `Файл повреждён: не удалось прочитать секцию изображений. Документ открыт без {N картинки|картинок}` и `Файл повреждён: не удалось прочитать страницу «{имя}». Она открыта пустой` (пустое имя — «без названия»). Интерфейс показывает предупреждения как раньше (уведомление на каждое).

- [ ] **Step 1: Обновить ожидания и написать падающий тест**

В `crates/ferrum-doc/tests/container.rs` заменить (два вхождения — испорченная картинка и подменённый id; заменить оба):

````rust
            .any(|w| w.contains("изображений — 1")),
````

на:

````rust
            .any(|w| w
                == "Файл повреждён: не удалось прочитать секцию изображений. Документ открыт без 1 картинки"),
````

В `crates/ferrum-doc/tests/container.rs` заменить:

````rust
    assert!(
        opened
            .warnings
            .iter()
            .any(|w| w.contains("повреждена и открыта пустой")),
````

на:

````rust
    assert!(
        opened.warnings.iter().any(|w| w
            == "Файл повреждён: не удалось прочитать страницу «Страница 1». Она открыта пустой"),
````

В конец `crates/ferrum-doc/tests/container.rs` добавить:

````rust

/// Сообщение о потерянных картинках (спека 11): сколько именно и в нужном падеже;
/// узлы остаются на месте — без картинок.
#[test]
fn lost_images_are_counted_in_the_warning() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let hashes: Vec<String> = (0..2u8).map(|i| doc.images.add(vec![i; 64])).collect();
    doc.transact(page, vec![], |p| {
        for h in &hashes {
            let n = p.create_node(None, None, NodeKind::Rect)?;
            p.set_props(
                n,
                &PropsPatch {
                    fills: Some(vec![Fill::image(h.clone())]),
                    ..Default::default()
                },
            )?;
        }
        Ok(vec![])
    })
    .unwrap();
    let bytes = encode(&doc).unwrap();
    // Обе секции изображений испорчены «тихо»: данные другие, сумма пересчитана.
    let broken = ferrum_doc::container::testing::map_sections(&bytes, |kind, _, data| {
        if kind == 4 {
            data[0] ^= 0xff;
        }
    });
    let opened = decode(&broken).unwrap();
    assert_eq!(
        opened.warnings,
        vec!["Файл повреждён: не удалось прочитать секцию изображений. Документ открыт без 2 картинок".to_string()]
    );
    let mut d = opened.document;
    assert_eq!(d.page(page).unwrap().children(None).len(), 2);
    assert_eq!(d.images.len(), 0);
}
````

Run: `cargo test -p ferrum-doc --test container`
Expected: FAIL — четыре теста (`corrupted_image_is_skipped_with_warning`, `image_with_mismatched_id_is_skipped_with_warning`, `corrupted_page_opens_empty_while_others_survive`, `lost_images_are_counted_in_the_warning`): прежние тексты предупреждений.

- [ ] **Step 2: Новые тексты**

В `crates/ferrum-doc/src/container.rs` заменить:

````rust
    if bad_images > 0 {
        warnings.push(format!(
            "Файл повреждён: не удалось прочитать изображений — {bad_images}"
        ));
    }
    let root = root.ok_or_else(|| {
````

на:

````rust
    let root = root.ok_or_else(|| {
````

В `crates/ferrum-doc/src/container.rs` заменить:

````rust
    let (document, mut doc_warnings) = Document::from_parts(&root, unloaded, images)?;
````

на:

````rust
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
````

В `crates/ferrum-doc/src/container.rs` заменить:

````rust
/// Помощники для тестов других модулей и крейтов. Не для продакшен-кода.
````

на:

````rust
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
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        for (id, _) in doc.pages() {
            if !doc.unloaded.contains_key(&id) {
                warnings.push(format!("Страница {id} повреждена и открыта пустой"));
````

на:

````rust
        for (id, name) in doc.pages() {
            if !doc.unloaded.contains_key(&id) {
                let name = if name.is_empty() {
                    "без названия"
                } else {
                    &name
                };
                warnings.push(format!(
                    "Файл повреждён: не удалось прочитать страницу «{name}». Она открыта пустой"
                ));
````

- [ ] **Step 3: Проверить**

Run: `cargo fmt && cargo test -p ferrum-doc && cargo clippy -p ferrum-doc --all-targets -- -D warnings`
Expected: PASS — все наборы `ferrum-doc` (в `container` — 21 тест), без предупреждений.

- [ ] **Step 4: Закоммитить**

```bash
git add crates/ferrum-doc
git commit -m "feat(doc): повреждённый файл — страница по имени и число потерянных картинок в сообщении

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Журнал изменений в модели документа

Пачка журнала — правки с прошлой пачки: обновления Loro корневого документа (имя, страницы, фон) и загруженных страниц от запомненной версии (`LoroDoc::oplog_vv` → `ExportMode::updates`) и байты картинок, добавленных с прошлой пачки. Формат — контейнер того же вида, что `.ferrum` (секции с blake3), с магией `FERRUMJ\0`. Версии журнала сдвигаются только после сборки всей пачки: ошибка экспорта ничего не теряет. Страница, загруженная из файла, получает версию при загрузке (всё до неё уже в файле), новая — пустую (в пачку попадёт целиком).

Восстановление (`apply_journal`): пачки по порядку записи; корень импортируется первым, затем страницы, созданные журналом, заводятся пустыми, удалённые — убираются, обновления страниц импортируются, а индекс и зеркало страницы перестраиваются из Loro (кэш сцены файла их больше не описывает; на 100 000 узлах это ≈ 1 с — только при восстановлении). Пачка, уже вошедшая в файл, безвредна: Loro пропускает известные операции (проверено: обновления поверх shallow-снапшота, содержащего их, — `Ok`, без `pending`). Повреждённая пачка (магия, обрезка, сумма) пропускается целиком; обновления, которым не хватает предыдущих (`ImportStatus::pending`), не применяются; обо всём этом — одно предупреждение «Не все несохранённые изменения удалось восстановить: журнал изменений повреждён». После применения всё восстановленное считается уже в журнале.

**Files:**
- Create: `crates/ferrum-doc/src/journal.rs`
- Modify: `crates/ferrum-doc/src/{container,document,page,images,lib}.rs`
- Test: `crates/ferrum-doc/tests/journal.rs`

**Interfaces:**
- Consumes: `container::push_section` (становится `pub(crate)`), `binary::Reader`, `PageDoc::rebuild_from_loro`.
- Produces: `Document::take_journal(&mut self) -> DocResult<Option<Vec<u8>>>`, `Document::mark_journal(&mut self)`, `Document::apply_journal(&mut self, batches: &[&[u8]]) -> Replayed`; `pub struct Replayed { pub applied: usize, pub warnings: Vec<String> }` (реэкспорт `ferrum_doc::Replayed`); `ferrum_doc::journal::{MAGIC, VERSION}`.

- [ ] **Step 1: Написать падающие тесты**

`crates/ferrum-doc/tests/journal.rs`:

````rust
//! Журнал изменений (этап 6): пачки обновлений Loro после файла восстанавливают
//! документ после сбоя — страницы, узлы, имя, фон и изображения; повреждённая или
//! неполная пачка — предупреждение, а не сбой; повтор пачки, уже вошедшей в файл,
//! безвреден; случайные правки с записью пачек и полных файлов в случайные моменты
//! восстанавливаются точно.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;
use proptest::prelude::*;

type PageState = (
    PageId,
    String,
    Option<Color>,
    Vec<(NodeId, Option<NodeId>, Props)>,
);

/// Всё, что видит пользователь: имя, страницы с фоном, узлы с родителями и свойствами.
fn state(doc: &mut Document) -> (String, Vec<PageState>) {
    let mut pages = Vec::new();
    for (id, name) in doc.pages() {
        let bg = doc.page_background(id);
        let page = doc.page(id).unwrap();
        let nodes = page
            .walk()
            .into_iter()
            .map(|n| (n, page.parent(n), page.props(n).unwrap()))
            .collect();
        pages.push((id, name, bg, nodes));
    }
    (doc.name(), pages)
}

fn rect(doc: &mut Document, page: PageId, x: f64) -> NodeId {
    doc.transact(page, vec![], |tx| {
        let id = tx.create_node(None, None, NodeKind::Rect)?;
        tx.set_props(
            id,
            &PropsPatch {
                x: Some(x),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    })
    .unwrap()[0]
}

/// Файл с фреймом на первой странице и открытый из него документ.
fn opened() -> (Vec<u8>, Document) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_node(None, None, NodeKind::Frame)?])
    })
    .unwrap();
    let bytes = encode(&doc).unwrap();
    let doc = decode(&bytes).unwrap().document;
    (bytes, doc)
}

fn recover(file: &[u8], batches: &[&[u8]]) -> (Document, Replayed) {
    let mut doc = decode(file).unwrap().document;
    let replayed = doc.apply_journal(batches);
    (doc, replayed)
}

#[test]
fn no_changes_no_batch() {
    let (_, mut doc) = opened();
    assert_eq!(doc.take_journal().unwrap(), None, "открытие — не правка");
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 10.0);
    assert!(doc.take_journal().unwrap().is_some());
    assert_eq!(doc.take_journal().unwrap(), None, "всё уже отдано");
}

#[test]
fn edits_after_open_are_restored_on_top_of_the_file() {
    let (file, mut doc) = opened();
    let p1 = doc.pages()[0].0;
    let r = rect(&mut doc, p1, 10.0);
    doc.set_name("Лендинг").unwrap();
    doc.rename_page(p1, "Главная").unwrap();
    doc.set_page_background(p1, Some(Color::WHITE)).unwrap();
    let hash = doc.images.add(vec![7; 100]);
    doc.transact(p1, vec![], |tx| {
        tx.set_props(
            r,
            &PropsPatch {
                fills: Some(vec![Fill::image(hash.clone())]),
                ..Default::default()
            },
        )?;
        Ok(vec![r])
    })
    .unwrap();
    let b1 = doc.take_journal().unwrap().unwrap();
    let p2 = doc.add_page("Вторая").unwrap();
    rect(&mut doc, p2, 5.0);
    doc.transact(p1, vec![], |tx| {
        tx.delete_nodes(&[r])?;
        Ok(vec![])
    })
    .unwrap();
    doc.undo().unwrap();
    let b2 = doc.take_journal().unwrap().unwrap();
    let (mut back, replayed) = recover(&file, &[&b1, &b2]);
    assert_eq!(replayed.applied, 2);
    assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
    assert_eq!(state(&mut back), state(&mut doc));
    assert_eq!(back.images.get(&hash), Some(&[7u8; 100][..]));
    // Восстановленное — уже в журнале: следующая пачка — только новые правки.
    assert_eq!(back.take_journal().unwrap(), None);
    // И переживает полное сохранение.
    let mut again = decode(&encode(&back).unwrap()).unwrap().document;
    assert_eq!(state(&mut again), state(&mut doc));
    assert!(again.images.get(&hash).is_some());
}

#[test]
fn batch_already_in_the_file_is_harmless() {
    let (_, mut doc) = opened();
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 1.0);
    let b1 = doc.take_journal().unwrap().unwrap();
    // Полное сохранение (журнал до него стирается, но пусть пачка осталась).
    let file2 = encode(&doc).unwrap();
    rect(&mut doc, page, 2.0);
    let b2 = doc.take_journal().unwrap().unwrap();
    for batches in [vec![&b2[..]], vec![&b1[..], &b2[..]]] {
        let (mut back, replayed) = recover(&file2, &batches);
        assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
        assert_eq!(state(&mut back), state(&mut doc));
    }
}

#[test]
fn damaged_batch_is_skipped_with_one_warning() {
    let (file, mut doc) = opened();
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 1.0);
    let good = doc.take_journal().unwrap().unwrap();
    let mut flipped = good.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 0xff;
    let truncated = good[..good.len() - 5].to_vec();
    let before = state(&mut decode(&file).unwrap().document);
    for bad in [flipped, truncated, b"garbage".to_vec(), Vec::new()] {
        let (mut back, replayed) = recover(&file, &[&bad]);
        assert_eq!(replayed.applied, 0);
        assert_eq!(replayed.warnings.len(), 1, "{:?}", replayed.warnings);
        assert!(replayed.warnings[0].contains("журнал изменений повреждён"));
        assert_eq!(state(&mut back), before, "документ — как в файле");
    }
}

#[test]
fn missing_earlier_batch_is_reported() {
    let (file, mut doc) = opened();
    let page = doc.pages()[0].0;
    let r = rect(&mut doc, page, 1.0);
    doc.take_journal().unwrap().unwrap();
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            r,
            &PropsPatch {
                x: Some(50.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let b2 = doc.take_journal().unwrap().unwrap();
    let (_, replayed) = recover(&file, &[&b2]);
    assert_eq!(
        replayed.warnings.len(),
        1,
        "правка узла, которого нет в файле"
    );
}

#[test]
fn deleted_page_and_its_edits_are_dropped() {
    let (file, mut doc) = opened();
    let p2 = doc.add_page("Вторая").unwrap();
    rect(&mut doc, p2, 3.0);
    let b1 = doc.take_journal().unwrap().unwrap();
    doc.delete_page(p2).unwrap();
    let b2 = doc.take_journal().unwrap().unwrap();
    let (mut back, replayed) = recover(&file, &[&b1, &b2]);
    assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
    assert_eq!(back.pages().len(), 1);
    assert_eq!(state(&mut back), state(&mut doc));
}

#[test]
fn mark_journal_skips_what_is_already_there() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 1.0);
    doc.images.add(vec![1, 2, 3]);
    doc.mark_journal();
    assert_eq!(doc.take_journal().unwrap(), None);
}

#[derive(Debug, Clone)]
enum Op {
    Rect(i16),
    MoveFirst(i16),
    DeleteLast,
    Undo,
    Redo,
    AddPage,
    RenamePage(u8),
    Image(u8),
    /// Пачка журнала.
    Flush,
    /// Полное сохранение: пачки до него больше не нужны.
    Save,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => any::<i16>().prop_map(Op::Rect),
        2 => any::<i16>().prop_map(Op::MoveFirst),
        1 => Just(Op::DeleteLast),
        1 => Just(Op::Undo),
        1 => Just(Op::Redo),
        1 => Just(Op::AddPage),
        1 => any::<u8>().prop_map(Op::RenamePage),
        1 => any::<u8>().prop_map(Op::Image),
        2 => Just(Op::Flush),
        1 => Just(Op::Save),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 48,
        rng_algorithm: proptest::test_runner::RngAlgorithm::ChaCha,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x6a_0e_2a_06),
        ..ProptestConfig::default()
    })]
    #[test]
    fn file_plus_journal_restores_the_document(ops in prop::collection::vec(op(), 1..40)) {
        let (mut file, mut doc) = opened();
        let mut batches: Vec<Vec<u8>> = Vec::new();
        for op in ops {
            let pages = doc.pages();
            let page = pages.last().unwrap().0;
            match op {
                Op::Rect(x) => { rect(&mut doc, page, x as f64); }
                Op::MoveFirst(x) => {
                    if let Some(&id) = doc.page(page).unwrap().children(None).first() {
                        doc.transact(page, vec![], |tx| {
                            tx.set_props(id, &PropsPatch { x: Some(x as f64), ..Default::default() })?;
                            Ok(vec![])
                        }).unwrap();
                    }
                }
                Op::DeleteLast => {
                    if let Some(&id) = doc.page(page).unwrap().children(None).last() {
                        doc.transact(page, vec![], |tx| { tx.delete_nodes(&[id])?; Ok(vec![]) }).unwrap();
                    }
                }
                Op::Undo => { let _ = doc.undo(); }
                Op::Redo => { let _ = doc.redo(); }
                Op::AddPage => { doc.add_page("Ещё").unwrap(); }
                Op::RenamePage(n) => doc.rename_page(page, &format!("Страница {n}")).unwrap(),
                Op::Image(n) => { doc.images.add(vec![n; 16]); }
                Op::Flush => {
                    if let Some(b) = doc.take_journal().unwrap() { batches.push(b); }
                }
                Op::Save => {
                    // Как автосохранение: сначала пачка, затем файл; пачки до него стираются.
                    doc.take_journal().unwrap();
                    file = encode(&doc).unwrap();
                    batches.clear();
                }
            }
        }
        // Сбой после последней пачки: восстанавливаем файл и все пачки после него.
        if let Some(b) = doc.take_journal().unwrap() { batches.push(b); }
        let refs: Vec<&[u8]> = batches.iter().map(Vec::as_slice).collect();
        let (mut back, replayed) = recover(&file, &refs);
        prop_assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
        prop_assert_eq!(state(&mut back), state(&mut doc));
    }
}
````

Run: `cargo test -p ferrum-doc --test journal`
Expected: FAIL — не компилируется: нет `take_journal`, `apply_journal`, `mark_journal`, `Replayed`.

- [ ] **Step 2: Формат пачки**

`crates/ferrum-doc/src/journal.rs`:

````rust
//! Журнал изменений (этап 6, спека 10): инкрементальные обновления Loro корневого
//! документа и страниц и новые изображения с прошлой записи. Автосохранение пишет
//! пачку журнала за миллисекунды, а полный файл — редко; после сбоя пачки, записанные
//! после последнего полного файла, применяются к нему при открытии.
//!
//! ```text
//! magic "FERRUMJ\0" (8 байт) · версия u32 LE · число секций u32 LE
//! секция — как в `.ferrum`: тип u8 · длина id u32 · id · длина данных u64 · blake3 · данные
//! ```
//! Типы секций: 1 — обновления корневого документа, 2 — обновления страницы
//! (id = PageId), 3 — изображение (id = хэш blake3).
//!
//! Пачка не доверенная (её читают из хранилища): неверный заголовок, обрезка или
//! чужая контрольная сумма — вся пачка пропускается с предупреждением. Обновления,
//! которые уже есть в документе, Loro пропускает сам, поэтому повтор пачки безвреден.

use std::collections::HashMap;

use crate::binary::Reader;
use crate::container::push_section;
use crate::types::PageId;

pub const MAGIC: &[u8; 8] = b"FERRUMJ\0";
pub const VERSION: u32 = 1;

pub(crate) const ROOT: u8 = 1;
pub(crate) const PAGE: u8 = 2;
pub(crate) const IMAGE: u8 = 3;

/// Собирает пачку из секций; `None` — секций нет.
pub(crate) struct BatchWriter {
    sections: Vec<u8>,
    count: u32,
}

impl BatchWriter {
    pub fn new() -> BatchWriter {
        BatchWriter {
            sections: Vec::new(),
            count: 0,
        }
    }

    pub fn push(&mut self, kind: u8, id: &str, data: &[u8]) {
        push_section(&mut self.sections, kind, id, data);
        self.count += 1;
    }

    pub fn finish(self) -> Option<Vec<u8>> {
        if self.count == 0 {
            return None;
        }
        let mut out = Vec::with_capacity(16 + self.sections.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&self.count.to_le_bytes());
        out.extend_from_slice(&self.sections);
        Some(out)
    }
}

/// Разобранная пачка: обновления корня, страниц (в порядке записи) и изображения.
#[derive(Default)]
pub(crate) struct Batch {
    pub root: Vec<Vec<u8>>,
    pub pages: Vec<(PageId, Vec<u8>)>,
    pub images: Vec<(String, Vec<u8>)>,
}

/// Разбирает пачку целиком или не принимает её: применять половину пачки нельзя —
/// её секции записаны одним шагом автосохранения.
pub(crate) fn parse(bytes: &[u8]) -> Option<Batch> {
    if bytes.len() < 16 || &bytes[..8] != MAGIC {
        return None;
    }
    let mut r = Reader::at(bytes, 8);
    if r.u32()? > VERSION {
        return None;
    }
    let count = r.u32()?;
    let mut batch = Batch::default();
    for _ in 0..count {
        let kind = r.u8()?;
        let id_len = r.u32()? as usize;
        let id = String::from_utf8(r.take(id_len)?.to_vec()).ok()?;
        let len = usize::try_from(r.u64()?).ok()?;
        let checksum: [u8; 32] = r.take(32)?.try_into().ok()?;
        let data = r.take(len)?;
        if blake3::hash(data).as_bytes() != &checksum {
            return None;
        }
        match kind {
            ROOT => batch.root.push(data.to_vec()),
            PAGE => batch.pages.push((id.parse().ok()?, data.to_vec())),
            // Хэш содержимого — проверка подмены id.
            IMAGE if blake3::hash(data).to_hex().as_str() == id => {
                batch.images.push((id, data.to_vec()))
            }
            IMAGE => return None,
            _ => {} // секции из будущих версий журнала пропускаем
        }
    }
    Some(batch)
}

/// Итог применения журнала при открытии.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Replayed {
    /// Пачек применено (повреждённые не считаются).
    pub applied: usize,
    /// Что восстановить не удалось — по-русски, для пользователя.
    pub warnings: Vec<String>,
}

/// Обновления страниц по порядку, сгруппированные по странице.
pub(crate) fn group_pages(list: Vec<(PageId, Vec<u8>)>) -> Vec<(PageId, Vec<Vec<u8>>)> {
    let mut order = Vec::new();
    let mut by: HashMap<PageId, Vec<Vec<u8>>> = HashMap::new();
    for (id, data) in list {
        if !by.contains_key(&id) {
            order.push(id);
        }
        by.entry(id).or_default().push(data);
    }
    order
        .into_iter()
        .map(|id| {
            let list = by.remove(&id).unwrap_or_default();
            (id, list)
        })
        .collect()
}
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
mod images;
mod loro_ext;
````

на:

````rust
mod images;
pub mod journal;
mod loro_ext;
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
pub use images::ImageStore;
````

на:

````rust
pub use images::ImageStore;
pub use journal::Replayed;
````

В `crates/ferrum-doc/src/container.rs` заменить:

````rust
fn push_section(out: &mut Vec<u8>, kind: u8, id: &str, data: &[u8]) {
````

на:

````rust
pub(crate) fn push_section(out: &mut Vec<u8>, kind: u8, id: &str, data: &[u8]) {
````

- [ ] **Step 3: Новые картинки и обновления страницы**

В `crates/ferrum-doc/src/images.rs` заменить:

````rust
use std::collections::HashMap;

#[derive(Default)]
pub struct ImageStore {
    by_hash: HashMap<String, Vec<u8>>,
}
````

на:

````rust
use std::collections::hash_map::Entry;
use std::collections::HashMap;

#[derive(Default)]
pub struct ImageStore {
    by_hash: HashMap<String, Vec<u8>>,
    /// Добавленные с прошлой пачки журнала (`take_fresh`); прочитанные из файла
    /// сюда не попадают — они уже в нём.
    fresh: Vec<String>,
}
````

В `crates/ferrum-doc/src/images.rs` заменить:

````rust
        let hash = blake3::hash(&bytes).to_hex().to_string();
        self.by_hash.entry(hash.clone()).or_insert(bytes);
        hash
````

на:

````rust
        let hash = blake3::hash(&bytes).to_hex().to_string();
        if let Entry::Vacant(e) = self.by_hash.entry(hash.clone()) {
            e.insert(bytes);
            self.fresh.push(hash.clone());
        }
        hash
````

В `crates/ferrum-doc/src/images.rs` заменить:

````rust
    pub(crate) fn insert_verified(&mut self, hash: String, bytes: Vec<u8>) {
        self.by_hash.insert(hash, bytes);
    }
````

на:

````rust
    pub(crate) fn insert_verified(&mut self, hash: String, bytes: Vec<u8>) {
        self.by_hash.insert(hash, bytes);
    }

    /// Изображения, добавленные с прошлого вызова, — для пачки журнала.
    pub(crate) fn take_fresh(&mut self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.fresh)
            .into_iter()
            .filter_map(|h| {
                let bytes = self.by_hash.get(&h)?.clone();
                Some((h, bytes))
            })
            .collect()
    }
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
use loro::{ExportMode, LoroDoc, LoroTree, LoroValue, TreeID, TreeParentId, ValueOrContainer};
````

на:

````rust
use loro::{
    ExportMode, LoroDoc, LoroTree, LoroValue, TreeID, TreeParentId, ValueOrContainer, VersionVector,
};
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    pub(crate) fn commit(&self) {
        self.doc.commit();
    }
````

на:

````rust
    pub(crate) fn commit(&self) {
        self.doc.commit();
    }

    /// Версия операций страницы — отметка журнала.
    pub(crate) fn oplog_vv(&self) -> VersionVector {
        self.doc.oplog_vv()
    }

    /// Обновления Loro с версии `from` — секция пачки журнала.
    pub(crate) fn export_updates(&self, from: &VersionVector) -> DocResult<Vec<u8>> {
        Ok(self.doc.export(ExportMode::updates(from))?)
    }

    /// Применяет обновления журнала и строит индекс и зеркало заново из Loro: кэш
    /// сцены файла их уже не описывает. `false` — часть обновлений не применена
    /// (повреждена или ей не хватает предыдущих).
    pub(crate) fn import_updates(&self, batches: &[&[u8]]) -> DocResult<bool> {
        let mut complete = true;
        for data in batches {
            match self.doc.import(data) {
                Ok(status) => complete &= status.pending.is_none(),
                Err(_) => complete = false,
            }
        }
        self.from_cache.set(false);
        self.rebuild_from_loro()?;
        Ok(complete)
    }
````

- [ ] **Step 4: Журнал документа**

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
use loro::{LoroDoc, LoroTree, LoroValue, TreeParentId, ValueOrContainer};
````

на:

````rust
use loro::{
    ExportMode, LoroDoc, LoroTree, LoroValue, TreeParentId, ValueOrContainer, VersionVector,
};
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
use crate::images::ImageStore;
````

на:

````rust
use crate::images::ImageStore;
use crate::journal::{self, BatchWriter, Replayed};
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    pub images: ImageStore,
    read_only: bool,
}
````

на:

````rust
    pub images: ImageStore,
    read_only: bool,
    /// Журнал (этап 6): версии Loro корня и загруженных страниц, до которых правки
    /// уже отданы в журнал (`take_journal`). Страница получает версию при загрузке.
    journaled_root: VersionVector,
    journaled_pages: HashMap<PageId, VersionVector>,
}
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        let pages_tree = root.get_tree(PAGES);
        pages_tree.enable_fractional_index(0);
        Document {
            root,
````

на:

````rust
        let pages_tree = root.get_tree(PAGES);
        pages_tree.enable_fractional_index(0);
        let journaled_root = root.oplog_vv();
        Document {
            root,
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            images,
            read_only: false,
        }
````

на:

````rust
            images,
            read_only: false,
            journaled_root,
            journaled_pages: HashMap::new(),
        }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
                ));
                doc.loaded.insert(id, PageDoc::new());
````

на:

````rust
                ));
                doc.loaded.insert(id, PageDoc::new());
                doc.journaled_pages.insert(id, VersionVector::default());
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        let id = PageId(t);
        self.loaded.insert(id, PageDoc::new());
        Ok(id)
````

на:

````rust
        let id = PageId(t);
        self.loaded.insert(id, PageDoc::new());
        // Новая страница целиком попадёт в следующую пачку журнала.
        self.journaled_pages.insert(id, VersionVector::default());
        Ok(id)
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        self.history.forget_page(id);
        self.changes.remove(&id);
        Ok(())
````

на:

````rust
        self.history.forget_page(id);
        self.changes.remove(&id);
        self.journaled_pages.remove(&id);
        Ok(())
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            self.unloaded.remove(&id);
            self.loaded.insert(id, page);
````

на:

````rust
            self.unloaded.remove(&id);
            // Загруженное из файла уже в нём: журнал пишет правки после загрузки.
            self.journaled_pages.insert(id, page.oplog_vv());
            self.loaded.insert(id, page);
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    pub fn can_undo(&self) -> bool {
````

на:

````rust
    /// Правки с прошлого вызова одной пачкой журнала: обновления Loro корня и
    /// загруженных страниц и новые изображения. `None` — изменений нет. Версии
    /// журнала сдвигаются, только когда собрана вся пачка: ошибка ничего не теряет.
    pub fn take_journal(&mut self) -> DocResult<Option<Vec<u8>>> {
        let mut batch = BatchWriter::new();
        let mut next_root = None;
        let vv = self.root.oplog_vv();
        if vv != self.journaled_root {
            batch.push(
                journal::ROOT,
                "",
                &self
                    .root
                    .export(ExportMode::updates(&self.journaled_root))?,
            );
            next_root = Some(vv);
        }
        let mut ids: Vec<PageId> = self.loaded.keys().copied().collect();
        ids.sort_by_key(|id| id.to_string());
        let mut next_pages = Vec::new();
        for id in ids {
            let page = &self.loaded[&id];
            let vv = page.oplog_vv();
            let base = self.journaled_pages.get(&id).cloned().unwrap_or_default();
            if vv != base {
                batch.push(journal::PAGE, &id.to_string(), &page.export_updates(&base)?);
                next_pages.push((id, vv));
            }
        }
        if let Some(vv) = next_root {
            self.journaled_root = vv;
        }
        self.journaled_pages.extend(next_pages);
        for (hash, bytes) in self.images.take_fresh() {
            batch.push(journal::IMAGE, &hash, &bytes);
        }
        Ok(batch.finish())
    }

    /// Всё нынешнее считается уже в журнале: после открытия примера или
    /// восстановления журнал пишет только новые правки.
    pub fn mark_journal(&mut self) {
        self.journaled_root = self.root.oplog_vv();
        self.journaled_pages = self
            .loaded
            .iter()
            .map(|(id, p)| (*id, p.oplog_vv()))
            .collect();
        self.images.take_fresh();
    }

    /// Применяет пачки журнала, записанные после файла, из которого открыт документ,
    /// в порядке записи, и считает их уже в журнале. Повреждённая пачка
    /// пропускается целиком; обновления, которым не хватает предыдущих, Loro не
    /// применяет. Обо всём, что не восстановлено, — одно предупреждение.
    pub fn apply_journal(&mut self, batches: &[&[u8]]) -> Replayed {
        let mut out = Replayed::default();
        if batches.is_empty() {
            return out;
        }
        if self.read_only {
            out.warnings
                .push("Несохранённые изменения не применены: файл открыт только для чтения".into());
            return out;
        }
        let mut lost = false;
        let (mut root, mut pages, mut images) = (Vec::new(), Vec::new(), Vec::new());
        for bytes in batches {
            match journal::parse(bytes) {
                Some(b) => {
                    out.applied += 1;
                    root.extend(b.root);
                    pages.extend(b.pages);
                    images.extend(b.images);
                }
                None => lost = true,
            }
        }
        for data in &root {
            match self.root.import(data) {
                Ok(status) => lost |= status.pending.is_some(),
                Err(_) => lost = true,
            }
        }
        // Страницы, созданные и удалённые журналом корня.
        let listed: Vec<PageId> = self.pages().into_iter().map(|(id, _)| id).collect();
        for id in &listed {
            if !self.loaded.contains_key(id) && !self.unloaded.contains_key(id) {
                self.loaded.insert(*id, PageDoc::new());
            }
        }
        self.loaded.retain(|id, _| listed.contains(id));
        self.unloaded.retain(|id, _| listed.contains(id));
        for (id, list) in journal::group_pages(pages) {
            // Страницу удалили позже в том же журнале — её правки не нужны.
            if !listed.contains(&id) {
                continue;
            }
            if self.page(id).is_err() {
                lost = true;
                continue;
            }
            let page = self.loaded.get(&id).expect("загружена выше");
            let refs: Vec<&[u8]> = list.iter().map(Vec::as_slice).collect();
            match page.import_updates(&refs) {
                Ok(complete) => lost |= !complete,
                Err(_) => lost = true,
            }
            if page.has_unknown_nodes() {
                self.read_only = true;
            }
        }
        for (hash, bytes) in images {
            self.images.insert_verified(hash, bytes);
        }
        if lost {
            out.warnings.push(
                "Не все несохранённые изменения удалось восстановить: журнал изменений повреждён"
                    .into(),
            );
        }
        self.mark_journal();
        out
    }

    pub fn can_undo(&self) -> bool {
````

- [ ] **Step 5: Проверить**

Run: `cargo fmt && cargo test -p ferrum-doc --test journal`
Expected: PASS, `8 passed` (проверено на прототипе этого кода: все 8, в том числе `proptest` на 48 сценариях).

Run: `cargo test -p ferrum-doc && cargo clippy -p ferrum-doc --all-targets -- -D warnings && cargo fmt --check`
Expected: PASS, без предупреждений.

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-doc
git commit -m "feat(doc): журнал изменений — пачки обновлений Loro и новых картинок после файла, восстановление поверх файла

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Журнал в ядре worker'а: команды, события, пределы

Ядро отдаёт пачку журнала по команде `FlushJournal` (ответ `Journal { batch, bytes }` приходит всегда; без правок — без байтов, чтобы интерфейс знал, что всё отдано) и само — перед каждым снимком (`Snapshot { journal }` несёт номер последней пачки, вошедшей в снимок; без правок с прошлой пачки отдельной пачки нет). Номер пачки растёт с каждой непустой пачкой. `OpenWithJournal { bytes, journal }` открывает файл и применяет пачки поверх него; если хоть одна применена, перед `Opened` уходит `Recovered { batches }`, а документ — изменён (восстановленного ещё нет в файле, его запишет следующее полное сохранение). Пример (`Demo`) журналом считается уже записанным: пачка — только с правками после него. Пачек — не больше 100 000 (`MAX_JOURNAL_BATCHES`), больше — «Не удалось открыть файл: в журнале изменений больше 100000 записей», документ не меняется.

На стороне TypeScript: конверт команды с журналом передаёт буферы всех пачек без копий (повторяющийся буфер — один раз в списке передачи); файл вместе с журналом — не больше предела открытия (512 МБ), проверяют обе стороны; worker передаёт байты события `journal`, только если они есть (`bytes` у пустой пачки — `undefined`).

**Files:**
- Modify: `crates/ferrum-editor/src/editor.rs`
- Modify: `crates/ferrum-wasm/src/protocol.rs`
- Modify: `crates/ferrum-wasm/src/core.rs`
- Modify: `crates/ferrum-wasm/src/core/panels.rs`
- Test: `crates/ferrum-wasm/tests/journal.rs`
- Test: `crates/ferrum-wasm/tests/journal_perf.rs`
- Modify: `crates/ferrum-wasm/tests/protocol.rs`
- Modify: `apps/web/src/core/limits.ts`
- Modify: `apps/web/src/core/messages.ts`
- Modify: `apps/web/src/worker/core.worker.ts`
- Modify: `apps/web/unit/limits.test.ts`
- Modify: `apps/web/unit/messages.test.ts`

**Interfaces:**
- Consumes: `Document::{take_journal, mark_journal, apply_journal}` (Task 2).
- Produces: `Editor::{take_journal, mark_journal}`; `Command::{OpenWithJournal { bytes, journal: Vec<ByteBuf> }, FlushJournal}`; `CoreEvent::{Journal { batch: f64, bytes: Option<ByteBuf> }, Recovered { batches: u32 }}`, поле `Snapshot::journal: f64`; `Core::open_with_journal(&mut self, bytes: &[u8], journal: &[ByteBuf])`; `protocol::MAX_JOURNAL_BATCHES = 100_000`. TypeScript (tsify): `{ type: "openWithJournal"; bytes: Uint8Array; journal: Uint8Array[] }`, `{ type: "flushJournal" }`, `{ type: "journal"; batch: number; bytes: Uint8Array | undefined }`, `{ type: "recovered"; batches: number }`, `snapshot.journal: number` (проверено сборкой прототипа); `limits.ts`: `MAX_JOURNAL_ENTRIES = 100_000`; `MAX_COMMAND_BYTES.openWithJournal`.

- [ ] **Step 1: Написать падающие тесты ядра**

`crates/ferrum-wasm/tests/journal.rs`:

````rust
//! Журнал изменений в ядре worker'а (этап 6): пачка правок по команде и перед
//! снимком, номер пачки в снимке, открытие файла с журналом — восстановленный
//! документ изменён, повреждённый журнал — предупреждение, а не отказ открыть.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::Platform;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

const VIEW: Size = Size::new(800.0, 600.0);

fn core() -> Core {
    Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu)
}

fn run(core: &mut Core, command: &str) {
    core.command(Command::Execute {
        command: command.into(),
    });
}

/// Файл с примером из `nodes` узлов и ядро, открывшее его (как редактор файла из хранилища).
fn stored(nodes: u32) -> (Vec<u8>, Core) {
    let mut c = core();
    c.command(Command::Demo { nodes });
    c.command(Command::Save);
    let bytes = c
        .take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Saved { bytes } => Some(bytes.into_vec()),
            _ => None,
        })
        .expect("нет Saved");
    let mut c = core();
    c.command(Command::Open {
        bytes: ByteBuf::from(bytes.clone()),
    });
    c.take_events();
    (bytes, c)
}

fn journal(ev: &[CoreEvent]) -> Vec<(f64, Option<Vec<u8>>)> {
    ev.iter()
        .filter_map(|e| match e {
            CoreEvent::Journal { batch, bytes } => {
                Some((*batch, bytes.as_ref().map(|b| b.to_vec())))
            }
            _ => None,
        })
        .collect()
}

fn flush(core: &mut Core) -> (f64, Option<Vec<u8>>) {
    core.command(Command::FlushJournal);
    let mut j = journal(&core.take_events());
    assert_eq!(j.len(), 1, "один ответ на команду");
    j.remove(0)
}

fn reopen(file: &[u8], batches: &[Vec<u8>]) -> (Core, Vec<CoreEvent>) {
    let mut c = core();
    c.command(Command::OpenWithJournal {
        bytes: ByteBuf::from(file.to_vec()),
        journal: batches.iter().cloned().map(ByteBuf::from).collect(),
    });
    let ev = c.take_events();
    (c, ev)
}

#[test]
fn flush_answers_with_a_batch_only_when_there_are_edits() {
    let (_, mut c) = stored(10);
    assert_eq!(flush(&mut c), (0.0, None), "открытие — не правка");
    run(&mut c, "edit.select_all");
    assert_eq!(flush(&mut c), (0.0, None), "выделение — не правка");
    run(&mut c, "edit.delete");
    let (batch, bytes) = flush(&mut c);
    assert_eq!(batch, 1.0);
    assert!(bytes.is_some());
    assert_eq!(flush(&mut c), (1.0, None), "всё уже отдано");
}

#[test]
fn demo_is_not_journaled() {
    let mut c = core();
    c.command(Command::Demo { nodes: 100 });
    c.take_events();
    assert_eq!(flush(&mut c), (0.0, None));
}

#[test]
fn journal_restores_edits_on_top_of_the_file_and_marks_modified() {
    let (file, mut c) = stored(10);
    run(&mut c, "edit.select_all");
    run(&mut c, "edit.delete");
    let (_, b1) = flush(&mut c);
    c.command(Command::AddPage);
    let (_, b2) = flush(&mut c);
    let (back, ev) = reopen(&file, &[b1.unwrap(), b2.unwrap()]);
    let at = |pred: &dyn Fn(&CoreEvent) -> bool| ev.iter().position(pred).expect("нет события");
    let recovered = at(&|e| *e == CoreEvent::Recovered { batches: 2 });
    let opened = at(&|e| matches!(e, CoreEvent::Opened { .. }));
    assert!(recovered < opened, "Recovered — до Opened: {ev:?}");
    let CoreEvent::Opened {
        pages,
        nodes,
        warnings,
        ..
    } = &ev[opened]
    else {
        unreachable!()
    };
    assert_eq!((*pages, *nodes), (2, 0), "{warnings:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(ev.contains(&CoreEvent::Modified { modified: true }));
    assert_eq!(
        back.editor().document().pages().len(),
        c.editor().document().pages().len()
    );
}

#[test]
fn snapshot_flushes_the_journal_first_and_carries_its_number() {
    let (_, mut c) = stored(10);
    run(&mut c, "edit.select_all");
    run(&mut c, "edit.delete");
    c.command(Command::Snapshot { thumbnail: false });
    let ev = c.take_events();
    let j = ev
        .iter()
        .position(
            |e| matches!(e, CoreEvent::Journal { batch, bytes: Some(_) } if *batch == 1.0),
        )
        .expect("пачка перед снимком");
    let (s, file2) = ev
        .iter()
        .enumerate()
        .find_map(|(i, e)| match e {
            CoreEvent::Snapshot { journal, bytes, .. } => {
                assert_eq!(*journal, 1.0);
                Some((i, bytes.to_vec()))
            }
            _ => None,
        })
        .expect("снимок");
    assert!(j < s);
    // Без правок снимок пачку не шлёт, номер — прежний.
    c.command(Command::Snapshot { thumbnail: false });
    let ev = c.take_events();
    assert!(journal(&ev).is_empty());
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Snapshot { journal, .. } if *journal == 1.0)));
    // Правка после снимка — следующая пачка; файл снимка и она дают документ.
    c.command(Command::AddPage);
    let (batch, b2) = flush(&mut c);
    assert_eq!(batch, 2.0);
    let (back, _) = reopen(&file2, &[b2.unwrap()]);
    assert_eq!(back.editor().document().pages().len(), 2);
}

#[test]
fn damaged_journal_opens_the_file_with_a_warning() {
    let (file, _) = stored(10);
    let (back, ev) = reopen(&file, &[b"garbage".to_vec()]);
    let warnings = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::Opened { warnings, .. } => Some(warnings.clone()),
            _ => None,
        })
        .expect("файл открыт");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("журнал изменений повреждён"));
    assert!(!ev.iter().any(|e| matches!(e, CoreEvent::Recovered { .. })));
    assert!(
        !ev.contains(&CoreEvent::Modified { modified: true }),
        "ничего не восстановлено"
    );
    assert_eq!(back.editor().scene().len(), 10);
}

#[test]
fn too_many_batches_is_an_error_and_keeps_the_document() {
    let (file, _) = stored(10);
    let mut c = core();
    c.command(Command::Demo { nodes: 5 });
    c.take_events();
    c.command(Command::OpenWithJournal {
        bytes: ByteBuf::from(file),
        journal: vec![ByteBuf::new(); MAX_JOURNAL_BATCHES + 1],
    });
    let ev = c.take_events();
    assert!(ev.iter().any(|e| matches!(e, CoreEvent::Error { message }
        if message.starts_with("Не удалось открыть файл: в журнале"))));
    assert_eq!(c.editor().scene().len(), 5);
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        Command::Demo { nodes: 100_000 },
````

на:

````rust
        Command::Demo { nodes: 100_000 },
        Command::OpenWithJournal {
            bytes: bytes(b"FERRUM"),
            journal: vec![bytes(b"FERRUMJ"), bytes(b"")],
        },
        Command::FlushJournal,
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
            thumbnail: Some(bytes(b"t")),
            keep_thumbnail: false,
        },
````

на:

````rust
            thumbnail: Some(bytes(b"t")),
            keep_thumbnail: false,
            journal: 4.0,
        },
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
            thumbnail: None,
            keep_thumbnail: true,
        },
````

на:

````rust
            thumbnail: None,
            keep_thumbnail: true,
            journal: 0.0,
        },
        CoreEvent::Journal {
            batch: 5.0,
            bytes: Some(bytes(b"j")),
        },
        CoreEvent::Journal {
            batch: 5.0,
            bytes: None,
        },
        CoreEvent::Recovered { batches: 2 },
````

Run: `cargo test -p ferrum-wasm --test journal --test protocol`
Expected: FAIL — не компилируется: нет `OpenWithJournal`, `FlushJournal`, `Journal`, `Recovered`, `MAX_JOURNAL_BATCHES`, поля `journal` у `Snapshot`.

- [ ] **Step 2: Журнал редактора**

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    pub fn document(&self) -> &Document {
        &self.doc
    }
````

на:

````rust
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Правки документа с прошлого вызова одной пачкой журнала автосохранения
    /// (этап 6); `None` — правок нет.
    pub fn take_journal(&mut self) -> DocResult<Option<Vec<u8>>> {
        self.doc.take_journal()
    }

    /// Всё нынешнее — уже в журнале (пример, только что собранный документ).
    pub fn mark_journal(&mut self) {
        self.doc.mark_journal();
    }
````

- [ ] **Step 3: Протокол**

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
pub const THUMB_WIDTH: u16 = 480;
pub const THUMB_HEIGHT: u16 = 320;
````

на:

````rust
pub const THUMB_WIDTH: u16 = 480;
pub const THUMB_HEIGHT: u16 = 320;
/// Наибольшее число пачек журнала изменений при открытии (этап 6): пачка пишется
/// не чаще раза в 2 с, а полный файл — хотя бы раз в 5 минут правок, так что столько
/// набирается, только если полное сохранение много часов не удаётся.
pub const MAX_JOURNAL_BATCHES: usize = 100_000;
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    Demo {
        nodes: u32,
    },
    /// Команда редактора по ключу таблицы команд (`edit.undo`, `tool.rect`…).
````

на:

````rust
    Demo {
        nodes: u32,
    },
    /// Открыть файл и применить поверх него пачки журнала изменений, записанные
    /// после него, по порядку записи (восстановление после сбоя, этап 6).
    OpenWithJournal {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
        #[tsify(type = "Uint8Array[]")]
        journal: Vec<ByteBuf>,
    },
    /// Отдать журналу правки с прошлой пачки: ответ — `Journal` (и без правок).
    FlushJournal,
    /// Команда редактора по ключу таблицы команд (`edit.undo`, `tool.rect`…).
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        keep_thumbnail: bool,
    },
    /// Команда «Сохранить» (Ctrl/Cmd+S): записать файл в хранилище.
    SaveRequested,
````

на:

````rust
        keep_thumbnail: bool,
        /// Номер последней пачки журнала, вошедшей в снимок: после записи снимка
        /// пачки до неё включительно не нужны.
        journal: f64,
    },
    /// Пачка журнала изменений: номер (растёт с каждой непустой пачкой) и байты;
    /// без байтов — правок с прошлой пачки нет.
    Journal {
        batch: f64,
        #[tsify(type = "Uint8Array | undefined")]
        bytes: Option<ByteBuf>,
    },
    /// Перед `Opened`: поверх файла применены пачки журнала — правки, которые не
    /// успели попасть в файл (документ изменён).
    Recovered {
        batches: u32,
    },
    /// Команда «Сохранить» (Ctrl/Cmd+S): записать файл в хранилище.
    SaveRequested,
````

- [ ] **Step 4: Ядро**

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    Backend, Command, CoreEvent, CursorKind, Theme, Tool, MAX_DEMO_NODES, SELECTION_IDS_MAX,
    THUMB_HEIGHT, THUMB_WIDTH,
};
````

на:

````rust
    Backend, Command, CoreEvent, CursorKind, Theme, Tool, MAX_DEMO_NODES, MAX_JOURNAL_BATCHES,
    SELECTION_IDS_MAX, THUMB_HEIGHT, THUMB_WIDTH,
};
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    /// а `MarkSaved` снимает `modified`, только если с тех пор она не сменилась.
    revision: u64,
}
````

на:

````rust
    /// а `MarkSaved` снимает `modified`, только если с тех пор она не сменилась.
    revision: u64,
    /// Номер последней непустой пачки журнала изменений (этап 6).
    journal_batch: u64,
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            modified_sent: Some(false),
            revision: 0,
        };
````

на:

````rust
            modified_sent: Some(false),
            revision: 0,
            journal_batch: 0,
        };
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    /// Снимок для хранилища: байты, ревизия, имя и превью первой страницы.
    fn snapshot(&mut self, thumbnail: bool) {
        let bytes = match container::encode(self.editor.document()) {
````

на:

````rust
    /// Пачка журнала: правки с прошлой пачки. `reply` — ответить и без правок
    /// (команда `FlushJournal`: интерфейс по ответу знает, что всё отдано); перед
    /// снимком — только если есть что записать.
    fn flush_journal(&mut self, reply: bool) {
        match self.editor.take_journal() {
            Ok(Some(bytes)) => {
                self.journal_batch += 1;
                self.events.push(CoreEvent::Journal {
                    batch: self.journal_batch as f64,
                    bytes: Some(ByteBuf::from(bytes)),
                });
            }
            Ok(None) if reply => self.events.push(CoreEvent::Journal {
                batch: self.journal_batch as f64,
                bytes: None,
            }),
            Ok(None) => {}
            Err(e) => self.error(format!("Не удалось записать журнал изменений: {e}")),
        }
    }

    /// Снимок для хранилища: байты, ревизия, имя и превью первой страницы. Сначала
    /// — пачка журнала с правками до снимка: не запишется снимок — они останутся в
    /// журнале, запишется — пачки до его номера больше не нужны.
    fn snapshot(&mut self, thumbnail: bool) {
        self.flush_journal(false);
        let bytes = match container::encode(self.editor.document()) {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            thumbnail: thumbnail.map(ByteBuf::from),
            keep_thumbnail: keep,
        });
````

на:

````rust
            thumbnail: thumbnail.map(ByteBuf::from),
            keep_thumbnail: keep,
            journal: self.journal_batch as f64,
        });
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    /// Открывает файл `.ferrum`. При ошибке текущий документ остаётся как был.
    pub fn open(&mut self, bytes: &[u8]) {
        match container::decode(bytes) {
            Ok(opened) => match self.replace_document(opened.document) {
                Ok(()) => self.push_opened(opened.warnings),
                Err(e) => self.error(format!("Не удалось открыть файл: {e}")),
            },
            Err(e) => self.error(format!("Не удалось открыть файл: {e}")),
        }
    }
````

на:

````rust
    /// Открывает файл `.ferrum`. При ошибке текущий документ остаётся как был.
    pub fn open(&mut self, bytes: &[u8]) {
        self.open_with_journal(bytes, &[]);
    }

    /// Открывает файл и применяет поверх него пачки журнала, записанные после него
    /// (восстановление после сбоя). Повреждённое в журнале — предупреждение в
    /// `Opened`; восстановленное ещё не в файле — документ изменён.
    pub fn open_with_journal(&mut self, bytes: &[u8], journal: &[ByteBuf]) {
        if journal.len() > MAX_JOURNAL_BATCHES {
            self.error(format!(
                "Не удалось открыть файл: в журнале изменений больше {MAX_JOURNAL_BATCHES} записей"
            ));
            return;
        }
        let mut opened = match container::decode(bytes) {
            Ok(o) => o,
            Err(e) => {
                self.error(format!("Не удалось открыть файл: {e}"));
                return;
            }
        };
        let batches: Vec<&[u8]> = journal.iter().map(|b| b.as_slice()).collect();
        let replayed = opened.document.apply_journal(&batches);
        opened.warnings.extend(replayed.warnings);
        if let Err(e) = self.replace_document(opened.document) {
            self.error(format!("Не удалось открыть файл: {e}"));
            return;
        }
        if replayed.applied > 0 {
            self.touch();
            self.events.push(CoreEvent::Recovered {
                batches: replayed.applied as u32,
            });
        }
        self.push_opened(opened.warnings);
    }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        let mut cam = self.editor.camera().clone();
        cam.set_zoom(0.5);
````

на:

````rust
        // Пример — как только что открытый файл: журнал пишет только правки после него.
        self.editor.mark_journal();
        let mut cam = self.editor.camera().clone();
        cam.set_zoom(0.5);
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            Command::Demo { nodes } => self.demo(nodes),
````

на:

````rust
            Command::Demo { nodes } => self.demo(nodes),
            Command::OpenWithJournal { bytes, journal } => {
                self.open_with_journal(&bytes, &journal)
            }
            Command::FlushJournal => self.flush_journal(true),
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            Command::Open { .. }
            | Command::Demo { .. }
````

на:

````rust
            Command::Open { .. }
            | Command::Demo { .. }
            | Command::OpenWithJournal { .. }
            | Command::FlushJournal
````

Run: `cargo fmt && cargo test -p ferrum-wasm --test journal --test protocol`
Expected: PASS — `journal` 6 passed, `protocol` 5 passed (проверено на прототипе).

Цена журнала на 100 000 узлах — тест только для release, как `snapshot_perf.rs`.

`crates/ferrum-wasm/tests/journal_perf.rs`:

````rust
//! Цена журнала на странице в 100 000 узлов (этап 6): пачка после сдвига 1000 узлов —
//! миллисекунды, а не полсекунды полного снимка; открытие файла с журналом — в пределах
//! цели открытия (спека 3). Время меряется только в release.
#![cfg(all(not(target_arch = "wasm32"), not(debug_assertions)))]

use std::time::{Duration, Instant};

use ferrum_editor::Platform;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

fn core() -> Core {
    Core::new(Platform::Windows, Size::new(1280.0, 720.0), 1.0, Backend::Cpu)
}

fn run(core: &mut Core, command: &str) {
    core.command(Command::Execute {
        command: command.into(),
    });
}

#[test]
fn journal_of_100k_nodes_is_cheap() {
    let mut c = core();
    c.command(Command::Demo { nodes: 100_000 });
    c.command(Command::Save);
    let file = c
        .take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Saved { bytes } => Some(bytes.into_vec()),
            _ => None,
        })
        .expect("нет Saved");
    let mut c = core();
    c.command(Command::Open {
        bytes: ByteBuf::from(file.clone()),
    });
    c.take_events();
    // Сдвиг 1000 фреймов верхнего уровня — правка 1000 узлов, затем пачка.
    run(&mut c, "edit.select_all");
    run(&mut c, "nudge.right");
    c.take_events();
    let t = Instant::now();
    c.command(Command::FlushJournal);
    let flush = t.elapsed();
    let batch = c
        .take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Journal { bytes, .. } => bytes,
            _ => None,
        })
        .expect("пачка");
    let t = Instant::now();
    let mut back = core();
    back.command(Command::OpenWithJournal {
        bytes: ByteBuf::from(file),
        journal: vec![batch.clone()],
    });
    let open = t.elapsed();
    assert!(back
        .take_events()
        .contains(&CoreEvent::Recovered { batches: 1 }));
    eprintln!(
        "journal: flush {flush:?}, {} КБ; open with journal {open:?}",
        batch.len() / 1024
    );
    assert!(flush < Duration::from_millis(20), "пачка: {flush:?}");
    assert!(open < Duration::from_millis(3000), "открытие с журналом: {open:?}");
}
````

Run: `cargo test --release -p ferrum-wasm --test journal_perf -- --nocapture`
Expected: PASS; строка `journal: flush …` — на прототипе (Apple M1 Pro) пачка 0,33 мс и 16 КБ, открытие с журналом 0,98 с (перестройка зеркала страницы из Loro).

- [ ] **Step 5: Пределы и конверт в TypeScript — падающие тесты**

Ядро пересобрать, чтобы TypeScript увидел новые типы: `cd apps/web && npm run wasm`.

В `apps/web/unit/limits.test.ts` заменить:

````ts
import {
  MAX_DATA_BYTES,
  MAX_IMAGE_SIDE,
````

на:

````ts
import {
  MAX_DATA_BYTES,
  MAX_IMAGE_SIDE,
  MAX_JOURNAL_ENTRIES,
````

В `apps/web/unit/limits.test.ts` заменить:

````ts
  expect(rustConst(`${CRATES}ferrum-editor/src/pages.rs`, 'MAX_NAME_CHARS')).toBe(MAX_NAME_CHARS);
});
````

на:

````ts
  expect(rustConst(`${CRATES}ferrum-editor/src/pages.rs`, 'MAX_NAME_CHARS')).toBe(MAX_NAME_CHARS);
  expect(rustConst(`${CRATES}ferrum-wasm/src/protocol.rs`, 'MAX_JOURNAL_BATCHES')).toBe(MAX_JOURNAL_ENTRIES);
});
````

В конец `apps/web/unit/messages.test.ts` добавить:

````ts

test('openWithJournal: файл и журнал вместе — не больше 512 МБ, пачки — байтами, буферы передаются', async () => {
  const { prepareCommand } = await import('../src/core/messages');
  const { MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES } = await import('../src/core/limits');
  const file = new Uint8Array(16);
  const ok = { type: 'openWithJournal' as const, bytes: file, journal: [new Uint8Array(8), new Uint8Array(4)] };
  expect(commandSizeError(ok)).toBeNull();
  const half = new Uint8Array(MAX_OPEN_BYTES / 2 + 1);
  expect(commandSizeError({ type: 'openWithJournal', bytes: half, journal: [half] })).toBe(
    'Слишком большой объём данных: 513 МБ, предел — 512 МБ',
  );
  expect(commandSizeError({ type: 'openWithJournal', bytes: file, journal: ['x' as unknown as Uint8Array] })).toBe(
    'Неверная команда: данные не в виде байтов',
  );
  expect(commandSizeError({ type: 'openWithJournal', bytes: file, journal: 'x' as unknown as Uint8Array[] })).toBe(
    'Неверная команда: данные не в виде байтов',
  );
  const many = new Array<Uint8Array>(MAX_JOURNAL_ENTRIES + 1).fill(new Uint8Array(1));
  expect(commandSizeError({ type: 'openWithJournal', bytes: file, journal: many })).toBe(
    'Неверная команда: в журнале изменений больше 100000 записей',
  );
  // Буферы передаются: файл, каждая пачка один раз (общий буфер — одной записью),
  // часть чужого буфера — копией.
  const shared = new Uint8Array(32);
  const part = new Uint8Array(new ArrayBuffer(64), 8, 8);
  const p = prepareCommand({ type: 'openWithJournal', bytes: file, journal: [shared, shared, part] });
  expect('error' in p).toBe(false);
  if ('error' in p) return;
  expect(p.transfer).toHaveLength(3);
  const cmd = p.cmd as { journal: Uint8Array[] };
  expect(cmd.journal[2].byteLength).toBe(8);
  expect(cmd.journal[2].buffer.byteLength).toBe(8);
});
````

Run: `cd apps/web && npm run unit -- -g "openWithJournal|пределы совпадают"`
Expected: FAIL — нет `MAX_JOURNAL_ENTRIES`; `commandSizeError` пропускает `openWithJournal`.

- [ ] **Step 6: Пределы и конверт**

В `apps/web/src/core/limits.ts` заменить:

````ts
/** Наибольшая длина имени слоя, страницы и файла в символах. */
export const MAX_NAME_CHARS = 256;
````

на:

````ts
/** Наибольшая длина имени слоя, страницы и файла в символах. */
export const MAX_NAME_CHARS = 256;
/** Пачек журнала изменений при открытии файла — как `MAX_JOURNAL_BATCHES` ядра. */
export const MAX_JOURNAL_ENTRIES = 100_000;
````

В `apps/web/src/core/messages.ts` заменить:

````ts
import { MAX_DATA_BYTES, MAX_OPEN_BYTES, mb, tooLarge } from './limits';
````

на:

````ts
import { MAX_DATA_BYTES, MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES, mb, tooLarge } from './limits';
````

В `apps/web/src/core/messages.ts` заменить:

````ts
export const MAX_COMMAND_BYTES = {
  open: MAX_OPEN_BYTES,
````

на:

````ts
export const MAX_COMMAND_BYTES = {
  open: MAX_OPEN_BYTES,
  /** Файл вместе со всеми пачками журнала. */
  openWithJournal: MAX_OPEN_BYTES,
````

В `apps/web/src/core/messages.ts` заменить:

````ts
export function commandSizeError(cmd: Command): string | null {
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type !== 'open' && cmd.type !== 'paste' && cmd.type !== 'placeImage' && cmd.type !== 'pasteImage') return null;
  const bytes = (cmd as { bytes: unknown }).bytes;
  if (!(bytes instanceof Uint8Array)) return BAD_BYTES;
  if (typeof SharedArrayBuffer === 'function' && bytes.buffer instanceof SharedArrayBuffer) return SHARED_BYTES;
  const max = MAX_COMMAND_BYTES[cmd.type];
  if (bytes.byteLength > max) return tooLarge(max, bytes.byteLength);
  return null;
}
````

на:

````ts
export function commandSizeError(cmd: Command): string | null {
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type === 'openWithJournal') return journalCommandError(cmd);
  if (cmd.type !== 'open' && cmd.type !== 'paste' && cmd.type !== 'placeImage' && cmd.type !== 'pasteImage') return null;
  const bytes = (cmd as { bytes: unknown }).bytes;
  const kind = bytesKindError(bytes);
  if (kind) return kind;
  const max = MAX_COMMAND_BYTES[cmd.type];
  const size = (bytes as Uint8Array).byteLength;
  if (size > max) return tooLarge(max, size);
  return null;
}

/** Байты не того вида или в общей памяти — текст ошибки; годятся — `null`. */
function bytesKindError(bytes: unknown): string | null {
  if (!(bytes instanceof Uint8Array)) return BAD_BYTES;
  if (typeof SharedArrayBuffer === 'function' && bytes.buffer instanceof SharedArrayBuffer) return SHARED_BYTES;
  return null;
}

/** Файл с журналом: каждая пачка — байтами, их не больше предела, а файл вместе с
 * журналом — не больше предела открытия. */
function journalCommandError(cmd: { bytes: unknown; journal: unknown }): string | null {
  const own = bytesKindError(cmd.bytes);
  if (own) return own;
  if (!Array.isArray(cmd.journal)) return BAD_BYTES;
  if (cmd.journal.length > MAX_JOURNAL_ENTRIES) return `Неверная команда: в журнале изменений больше ${MAX_JOURNAL_ENTRIES} записей`;
  let total = (cmd.bytes as Uint8Array).byteLength;
  for (const b of cmd.journal) {
    const e = bytesKindError(b);
    if (e) return e;
    total += (b as Uint8Array).byteLength;
  }
  return total > MAX_OPEN_BYTES ? tooLarge(MAX_OPEN_BYTES, total) : null;
}
````

В `apps/web/src/core/messages.ts` заменить:

````ts
export function prepareCommand(cmd: Command): { cmd: Command; transfer: Transferable[] } | { error: string } {
  const error = commandSizeError(cmd);
  if (error) return { error };
  if (typeof cmd !== 'object' || cmd === null || !('bytes' in cmd)) return { cmd, transfer: [] };
  let bytes = cmd.bytes as unknown as Uint8Array;
  if (bytes.byteOffset !== 0 || bytes.byteLength !== bytes.buffer.byteLength) bytes = bytes.slice();
  return { cmd: { ...cmd, bytes } as Command, transfer: [bytes.buffer as ArrayBuffer] };
}
````

на:

````ts
export function prepareCommand(cmd: Command): { cmd: Command; transfer: Transferable[] } | { error: string } {
  const error = commandSizeError(cmd);
  if (error) return { error };
  if (typeof cmd !== 'object' || cmd === null || !('bytes' in cmd)) return { cmd, transfer: [] };
  const own = (b: Uint8Array) => (b.byteOffset !== 0 || b.byteLength !== b.buffer.byteLength ? b.slice() : b);
  const bytes = own(cmd.bytes as unknown as Uint8Array);
  if (cmd.type === 'openWithJournal') {
    const journal = cmd.journal.map(own);
    // Один буфер в списке передачи дважды — DataCloneError: общий буфер — одной записью.
    const transfer = [...new Set([bytes.buffer, ...journal.map((j) => j.buffer)])] as ArrayBuffer[];
    return { cmd: { ...cmd, bytes, journal }, transfer };
  }
  return { cmd: { ...cmd, bytes } as Command, transfer: [bytes.buffer as ArrayBuffer] };
}
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  for (const e of events) {
    if ('bytes' in e) transfer.push(e.bytes.buffer);
````

на:

````ts
  for (const e of events) {
    // У пустой пачки журнала байтов нет (`bytes` — undefined).
    if ('bytes' in e && e.bytes) transfer.push(e.bytes.buffer);
````

- [ ] **Step 7: Проверить**

Run: `cd apps/web && npm run typecheck && npm run unit`
Expected: PASS — модульные тесты все (новый — `openWithJournal…`).

Run: `cargo test -p ferrum-wasm && cargo clippy --target wasm32-unknown-unknown -p ferrum-wasm --lib --tests -- -D warnings && cargo clippy --all-targets -- -D warnings`
Expected: PASS, без предупреждений.

- [ ] **Step 8: Закоммитить**

```bash
git add crates/ferrum-editor crates/ferrum-wasm apps/web/src apps/web/unit
git commit -m "feat(wasm): журнал в ядре — FlushJournal и пачка перед снимком, OpenWithJournal и Recovered; конверт и пределы журнала

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Журнал в хранилище IndexedDB

База `ferrum` — версия 2: новое хранилище `journal` с ключом `[id файла, сеанс, номер пачки]` и значением `ArrayBuffer` пачки. Сеанс — число из времени открытия редактора (Task 5), поэтому ключи одного файла упорядочены по времени: при открытии пачки читаются диапазоном `[id] … [id, []]` (массив больше любого числа) в порядке записи. Пачка дописывается своей транзакцией `durability: 'strict'`. Запись файла (`write`) с отметкой `{ session, upTo }` стирает в той же транзакции все пачки файла до `[id, session, upTo]` включительно — и прежних сеансов (они применены при открытии и вошли в снимок). Чтение файла ставится в ту же очередь, что правки: пачка, отправленная до чтения, в нём уже есть (после сбоя ядра редактор читает файл сразу). Журнал без байтов файла не отдаётся: пачки начаты на снимке, которого нет. Удаление навсегда и очистка корзины стирают и журнал.

**Files:**
- Modify: `apps/web/src/core/file-list.ts`
- Modify: `apps/web/src/worker/io.worker.ts`
- Modify: `apps/web/src/ui/file-store.ts`
- Modify: `apps/web/src/ui/saver.ts` (только вызов `write`: новый параметр, логика — в Task 5)
- Modify: `apps/web/e2e/files.spec.ts` (вызов `write` в тесте чужой записи)
- Test: `apps/web/unit/files.test.ts`
- Test: `apps/web/e2e/journal-store.spec.ts`

**Interfaces:**
- Consumes: очередь правок (`serial`), `update`, `rw`, `requestError` части 1.
- Produces: `JournalMark { session: number; batch: number }` (`core/file-list.ts`); `FileStore.read(id) → { meta, bytes, journal: ArrayBuffer[] }`; `FileStore.write(id, bytes, thumb, base, journal: { session: number; upTo: number } | null)`; `FileStore.appendJournal(id, mark: JournalMark, bytes: ArrayBuffer): Promise<void>`; тестовый хук `FileStoreTestHooks.journalCount(id): Promise<number>`; запросы worker'а `appendJournal`, `journalCount`, поле `journal` у `write`.

- [ ] **Step 1: Написать падающие тесты**

В конец `apps/web/unit/files.test.ts` добавить:

````ts

test('журнал: запросы к хранилищу проверяются', () => {
  const buf = new ArrayBuffer(4);
  const ok = { id: 1, op: 'appendJournal', file: 'f1', session: 1_700_000_000_000_123, batch: 3, bytes: buf };
  expect(requestError(ok)).toBeNull();
  for (const bad of [
    { ...ok, session: -1 },
    { ...ok, session: 1.5 },
    { ...ok, batch: Number.MAX_SAFE_INTEGER + 2 },
    { ...ok, bytes: 'x' },
    { ...ok, bytes: new Uint8Array(4) },
    { ...ok, file: '' },
  ]) {
    expect(requestError(bad), JSON.stringify(bad)).toBe('Неверный запрос к хранилищу файлов');
  }
  const write = { id: 2, op: 'write', file: 'f1', bytes: buf, thumb: 'keep', base: 1, journal: null };
  expect(requestError(write)).toBeNull();
  expect(requestError({ ...write, journal: { session: 5, upTo: 2 } })).toBeNull();
  for (const journal of [{ session: 5 }, { session: 'x', upTo: 1 }, { session: 1, upTo: -3 }, 7, undefined]) {
    expect(requestError({ ...write, journal }), JSON.stringify(journal)).toBe('Неверный запрос к хранилищу файлов');
  }
  expect(requestError({ id: 3, op: 'journalCount', file: 'f1' })).toBeNull();
  expect(requestError({ id: 3, op: 'journalCount', file: 7 })).toBe('Неверный запрос к хранилищу файлов');
});
````

`apps/web/e2e/journal-store.spec.ts`:

````ts
// Журнал изменений в хранилище IndexedDB (этап 6): пачки читаются по порядку сеансов
// и номеров вместе с файлом (и только если у файла есть байты), чтение после
// отправленной пачки её видит, запись файла стирает пачки до номера снимка, отказ
// записи их оставляет, удаление файла и корзины стирает журнал.

import { expect, test } from '@playwright/test';
import { testMode } from './helpers';

test.beforeEach(async ({ page }) => {
  await testMode(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
});

test('пачки — по порядку сеансов и номеров; без байтов файла журнала нет', async ({ page }) => {
  const r = await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const buf = (n: number) => new Uint8Array([n, n, n]).buffer;
    const m = await s.create('Журнал');
    await s.appendJournal(m.id, { session: 20, batch: 1 }, buf(3));
    const empty = await s.read(m.id);
    await s.write(m.id, new Uint8Array([1, 2, 3, 4]).buffer, null, m.version, null);
    await s.appendJournal(m.id, { session: 20, batch: 2 }, buf(4));
    // Без ожидания: чтение встаёт в ту же очередь и видит пачку.
    void s.appendJournal(m.id, { session: 10, batch: 7 }, buf(1));
    const full = await s.read(m.id);
    return {
      emptyJournal: empty.journal.length,
      order: full.journal.map((b) => new Uint8Array(b)[0]),
      count: await s.journalCount(m.id),
    };
  });
  expect(r).toEqual({ emptyJournal: 0, order: [1, 3, 4], count: 3 });
});

test('запись файла стирает пачки до номера снимка; отказ записи их оставляет', async ({ page }) => {
  const r = await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const m = await s.create('Снимок', new Blob([new Uint8Array([9])]));
    for (const [session, batch] of [[5, 1], [7, 1], [7, 2], [7, 3]]) {
      await s.appendJournal(m.id, { session, batch }, new Uint8Array([session * 10 + batch]).buffer);
    }
    await s.failWrites(true);
    let failed = '';
    try {
      await s.write(m.id, new Uint8Array([1]).buffer, null, m.version, { session: 7, upTo: 2 });
    } catch (e) {
      failed = (e as Error).message;
    }
    const kept = await s.journalCount(m.id);
    await s.failWrites(false);
    await s.write(m.id, new Uint8Array([1]).buffer, null, m.version, { session: 7, upTo: 2 });
    const left = (await s.read(m.id)).journal.map((b) => new Uint8Array(b)[0]);
    return { failed, kept, left };
  });
  expect(r).toEqual({ failed: 'недостаточно места в хранилище браузера', kept: 4, left: [73] });
});

test('удаление навсегда и очистка корзины стирают журнал', async ({ page }) => {
  const r = await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const one = await s.create('Один', new Blob([new Uint8Array([1])]));
    const two = await s.create('Два', new Blob([new Uint8Array([1])]));
    for (const m of [one, two]) await s.appendJournal(m.id, { session: 1, batch: 1 }, new ArrayBuffer(8));
    await s.remove(one.id);
    await s.trash(two.id);
    const inTrash = await s.journalCount(two.id);
    await s.emptyTrash();
    return [await s.journalCount(one.id), inTrash, await s.journalCount(two.id)];
  });
  expect(r).toEqual([0, 1, 0]);
});

test('пачка удалённого файла — отказ «Файл не найден»', async ({ page }) => {
  const message = await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const m = await s.create('Временный');
    await s.remove(m.id);
    return s.appendJournal(m.id, { session: 1, batch: 1 }, new ArrayBuffer(1)).then(
      () => 'записано',
      (e: Error) => e.message,
    );
  });
  expect(message).toBe('Файл не найден');
});
````

В конец `apps/web/e2e/helpers.ts` добавить:

````ts

/** Тестовый режим интерфейса до загрузки страницы: хуки хранилища, ядро в `__ferrum`. */
export async function testMode(page: Page): Promise<void> {
  await page.addInitScript(() => ((window as { __ferrumTest?: boolean }).__ferrumTest = true));
}

/** Редактор открыт и ядро готово (после перезапуска — новое ядро). */
export async function editorReady(page: Page): Promise<void> {
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true, null, {
    timeout: 90_000,
  });
  await expectCore(page);
  await page.waitForFunction(() => (window.__ferrum!.core!.live?.frames ?? 0) > 0);
}

/** id файла из адреса редактора (`#/file/<id>`). */
export function fileId(page: Page): string {
  const m = /#\/file\/([^/?#]+)/.exec(page.url());
  if (!m) throw new Error(`не адрес файла: ${page.url()}`);
  return decodeURIComponent(m[1]);
}

/** Прямоугольник инструментом R около центра холста (`dx` — сдвиг по x в мире);
 * ждёт записанный узел: в сцене на один больше и жест закончен (черновик рисования
 * тоже в сцене, поэтому одного числа узлов мало). */
export async function drawRect(page: Page, dx = 0): Promise<void> {
  const before = (await live(page)).nodes;
  await page.locator('canvas').click({ position: { x: 10, y: 10 } });
  await page.keyboard.press('KeyR');
  const l = await live(page);
  await drag(page, await appScreen(page, l, l.cameraX - 50 + dx, l.cameraY - 40), 100, 80);
  await liveWhen(page, `l.nodes === ${before + 1} && l.busy === 0`);
}

/** Пачек журнала файла в хранилище (тестовый хук worker'а ввода-вывода). */
export function journalCount(page: Page, id: string): Promise<number> {
  return page.evaluate((i) => window.__ferrumFiles!.store.journalCount(i), id);
}
````

В `apps/web/unit/files.test.ts` заменить (у записи теперь обязательное поле `journal`):

````ts
  const w = { id: 1, op: 'write', file: 'a', bytes: new ArrayBuffer(4), thumb: null, base: 0 };
````

на:

````ts
  const w = { id: 1, op: 'write', file: 'a', bytes: new ArrayBuffer(4), thumb: null, base: 0, journal: null };
````

Run: `cd apps/web && npm run unit -- -g "журнал"`
Expected: FAIL — `requestError` не знает `appendJournal` и `journalCount`, у `write` нет проверки `journal`.

- [ ] **Step 2: Запросы и проверки**

В `apps/web/src/core/file-list.ts` заменить:

````ts
/** Записи нет: файл удалили навсегда (другая вкладка), пока он был открыт. */
export const GONE = 'Файл удалён в другой вкладке — сохраните копию на диск';
````

на:

````ts
/** Записи нет: файл удалили навсегда (другая вкладка), пока он был открыт. */
export const GONE = 'Файл удалён в другой вкладке — сохраните копию на диск';

/** Пачка журнала изменений: сеанс редактора (число из времени открытия — сеансы
 * одного файла упорядочены по времени) и номер пачки ядра в этом сеансе. */
export interface JournalMark {
  session: number;
  batch: number;
}
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
  /** `thumb`: PNG, `null` — убрать (страница пуста), `'keep'` — оставить прежнее.
   * `base` — поколение, на котором основана запись. */
  | { op: 'write'; file: string; bytes: ArrayBuffer; thumb: ArrayBuffer | null | 'keep'; base: number }
````

на:

````ts
  /** `thumb`: PNG, `null` — убрать (страница пуста), `'keep'` — оставить прежнее.
   * `base` — поколение, на котором основана запись. `journal` — снимок вобрал пачки
   * этого сеанса до `upTo` включительно и все пачки прежних сеансов: они стираются в
   * той же транзакции; `null` — журнала у файла ещё нет. */
  | {
      op: 'write';
      file: string;
      bytes: ArrayBuffer;
      thumb: ArrayBuffer | null | 'keep';
      base: number;
      journal: { session: number; upTo: number } | null;
    }
  /** Пачка журнала изменений (байты передаются). */
  | { op: 'appendJournal'; file: string; session: number; batch: number; bytes: ArrayBuffer }
  /** Тесты: пачек журнала у файла. */
  | { op: 'journalCount'; file: string }
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
  read: { meta: FileMeta; bytes: ArrayBuffer | null };
  write: FileMeta;
````

на:

````ts
  /** `journal` — пачки, записанные после байтов файла, по порядку (без байтов — пусто). */
  read: { meta: FileMeta; bytes: ArrayBuffer | null; journal: ArrayBuffer[] };
  write: FileMeta;
  appendJournal: null;
  journalCount: number;
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
function isCount(v: unknown, max: number): v is number {
  return typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= max;
}
````

на:

````ts
function isCount(v: unknown, max: number): v is number {
  return typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= max;
}

/** Целое ≥ 0 не больше `Number.MAX_SAFE_INTEGER` (сеанс, номер пачки). */
function isWhole(v: unknown): v is number {
  return isCount(v, Number.MAX_SAFE_INTEGER) && Number.isInteger(v);
}

/** Отметка журнала у записи файла: `null` или `{ session, upTo }` целыми. */
function isJournalUpTo(v: unknown): boolean {
  if (v === null) return true;
  if (typeof v !== 'object') return false;
  const j = v as Record<string, unknown>;
  return isWhole(j.session) && isWhole(j.upTo);
}
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
    case 'delete':
    case 'thumb':
      return isId(m.file) ? null : BAD;
````

на:

````ts
    case 'delete':
    case 'thumb':
    case 'journalCount':
      return isId(m.file) ? null : BAD;
    case 'appendJournal':
      if (!isId(m.file) || !isWhole(m.session) || !isWhole(m.batch)) return BAD;
      if (!(m.bytes instanceof ArrayBuffer)) return BAD;
      return bytesError(m.bytes, MAX_OPEN_BYTES);
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
    case 'write':
      if (!isId(m.file) || !isCount(m.base, Number.MAX_SAFE_INTEGER)) return BAD;
````

на:

````ts
    case 'write':
      if (!isId(m.file) || !isCount(m.base, Number.MAX_SAFE_INTEGER)) return BAD;
      if (!isJournalUpTo(m.journal)) return BAD;
````

- [ ] **Step 3: Хранилище журнала в worker'е**

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
// База `ferrum`, версия 1:
// - `meta` — записи `FileMeta` (ключ `id`): список читает только их;
// - `bytes` — id → ArrayBuffer файла `.ferrum`;
// - `thumbs` — id → Blob PNG превью.
````

на:

````ts
// База `ferrum`, версия 2:
// - `meta` — записи `FileMeta` (ключ `id`): список читает только их;
// - `bytes` — id → ArrayBuffer файла `.ferrum`;
// - `thumbs` — id → Blob PNG превью;
// - `journal` (с версии 2) — `[id, сеанс, номер]` → ArrayBuffer пачки журнала
//   изменений: правки после байтов файла, по порядку записи. Запись файла стирает
//   пачки до номера своего снимка в той же транзакции.
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
const DB_VERSION = 1;
````

на:

````ts
const DB_VERSION = 2;
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
      if (!d.objectStoreNames.contains('thumbs')) d.createObjectStore('thumbs');
    };
````

на:

````ts
      if (!d.objectStoreNames.contains('thumbs')) d.createObjectStore('thumbs');
      if (!d.objectStoreNames.contains('journal')) d.createObjectStore('journal');
    };
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
async function readBytes(id: string): Promise<ArrayBuffer | null> {
````

на:

````ts
/** Все пачки журнала файла: ключи `[id, сеанс, номер]`, массив больше любого числа. */
function journalRange(id: string): IDBKeyRange {
  return IDBKeyRange.bound([id], [id, []]);
}

/** Байты файла и пачки журнала после них — одной транзакцией. Журнал без байтов не
 * отдаётся: его пачки начаты на снимке, которого нет. */
async function readFile(id: string): Promise<{ value: IoResults['read']; transfer: Transferable[] }> {
  const m = meta(id);
  const d = await open();
  const tx = d.transaction(['bytes', 'journal']);
  const [raw, entries] = await Promise.all([
    request(tx.objectStore('bytes').get(id)) as Promise<unknown>,
    request(tx.objectStore('journal').getAll(journalRange(id))) as Promise<unknown[]>,
  ]);
  const bytes = raw instanceof ArrayBuffer ? raw : null;
  const journal = bytes ? entries.filter((b): b is ArrayBuffer => b instanceof ArrayBuffer) : [];
  return { value: { meta: m, bytes, journal }, transfer: [...(bytes ? [bytes] : []), ...journal] };
}

async function readBytes(id: string): Promise<ArrayBuffer | null> {
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
  type FileMeta,
  type FromIo,
  type IoRequest,
  type ToIo,
} from '../core/file-list';
````

на:

````ts
  type FileMeta,
  type FromIo,
  type IoRequest,
  type IoResults,
  type ToIo,
} from '../core/file-list';
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
    case 'read': {
      const m = meta(r.file);
      const buf = await readBytes(r.file);
      return { value: { meta: m, bytes: buf }, transfer: buf ? [buf] : [] };
    }
````

на:

````ts
    case 'read':
      // В очереди правок: пачка журнала, отправленная раньше чтения, в нём уже есть
      // (после сбоя ядра редактор читает файл сразу).
      return serial(() => readFile(r.file));
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
      const { bytes, thumb, base } = r;
      // Имя записи здесь не меняется: оно — из хранилища (переименование — `rename`,
      // в том числе из другой вкладки), снимок его не затирает.
      const next = await update(
        r.file,
        ['bytes', 'thumbs'],
        (m, tx) => {
          if (m.version !== base) throw new Error(CONFLICT);
          tx.objectStore('bytes').put(bytes, m.id);
````

на:

````ts
      const { bytes, thumb, base, journal } = r;
      // Имя записи здесь не меняется: оно — из хранилища (переименование — `rename`,
      // в том числе из другой вкладки), снимок его не затирает.
      const next = await update(
        r.file,
        ['bytes', 'thumbs', 'journal'],
        (m, tx) => {
          if (m.version !== base) throw new Error(CONFLICT);
          tx.objectStore('bytes').put(bytes, m.id);
          // Пачки, вошедшие в снимок, — в той же транзакции: файл и журнал не расходятся.
          if (journal) tx.objectStore('journal').delete(IDBKeyRange.bound([m.id], [m.id, journal.session, journal.upTo]));
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
    case 'rename': {
      const name = cleanName(r.name);
````

на:

````ts
    case 'appendJournal': {
      // Запись есть (удалённый навсегда файл журнал не воскрешает).
      meta(r.file);
      const d = await open();
      const tx = rw(d, ['journal']);
      tx.objectStore('journal').put(r.bytes, [r.file, r.session, r.batch]);
      await done(tx);
      return { value: null };
    }
    case 'journalCount': {
      if (!test) throw new Error('Только в тестовом режиме');
      const d = await open();
      return { value: await request(d.transaction('journal').objectStore('journal').count(journalRange(r.file))) };
    }
    case 'rename': {
      const name = cleanName(r.name);
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
      const d = await open();
      const tx = rw(d, ['meta', 'bytes', 'thumbs']);
      for (const id of ids) for (const st of ['meta', 'bytes', 'thumbs']) tx.objectStore(st).delete(id);
      await done(tx);
````

на:

````ts
      const d = await open();
      const tx = rw(d, ['meta', 'bytes', 'thumbs', 'journal']);
      for (const id of ids) {
        for (const st of ['meta', 'bytes', 'thumbs']) tx.objectStore(st).delete(id);
        tx.objectStore('journal').delete(journalRange(id));
      }
      await done(tx);
````

- [ ] **Step 4: Хранилище файлов**

В `apps/web/src/ui/file-store.ts` заменить:

````ts
import type { FileMeta, FromIo, IoOp, IoResults, Section, ToIo } from '../core/file-list';
````

на:

````ts
import type { FileMeta, FromIo, IoOp, IoResults, JournalMark, Section, ToIo } from '../core/file-list';
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
export type { FileMeta, Section } from '../core/file-list';
````

на:

````ts
export type { FileMeta, JournalMark, Section } from '../core/file-list';
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  /** Байты файла; `null` — новый файл, ещё не сохранённый. */
  read(id: string): Promise<{ meta: FileMeta; bytes: ArrayBuffer | null }>;
  /** Содержимое и превью (буферы передаются и здесь больше не доступны); имя
   * записи не меняется — для него `rename`. `thumb`:
   * `null` — убрать превью, `'keep'` — оставить прежнее. `base` — поколение файла
   * (`FileMeta.version`), на котором основана запись; файл с тех пор записали —
   * отказ `CONFLICT`. */
  write(id: string, bytes: ArrayBuffer, thumb: ArrayBuffer | null | 'keep', base: number): Promise<FileMeta>;
````

на:

````ts
  /** Байты файла (`null` — новый файл, ещё не сохранённый) и пачки журнала,
   * записанные после них, по порядку записи. */
  read(id: string): Promise<{ meta: FileMeta; bytes: ArrayBuffer | null; journal: ArrayBuffer[] }>;
  /** Содержимое и превью (буферы передаются и здесь больше не доступны); имя
   * записи не меняется — для него `rename`. `thumb`:
   * `null` — убрать превью, `'keep'` — оставить прежнее. `base` — поколение файла
   * (`FileMeta.version`), на котором основана запись; файл с тех пор записали —
   * отказ `CONFLICT`. `journal` — пачки этого сеанса до `upTo` и прежних сеансов
   * вошли в снимок: стереть их вместе с записью; `null` — журнала ещё нет. */
  write(
    id: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ): Promise<FileMeta>;
  /** Дописать пачку журнала изменений (буфер передаётся). */
  appendJournal(id: string, mark: JournalMark, bytes: ArrayBuffer): Promise<void>;
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
export interface FileStoreTestHooks {
  seed(count: number): Promise<void>;
  failWrites(on: boolean): Promise<void>;
}
````

на:

````ts
export interface FileStoreTestHooks {
  seed(count: number): Promise<void>;
  failWrites(on: boolean): Promise<void>;
  /** Пачек журнала файла в базе. */
  journalCount(id: string): Promise<number>;
}
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  write(file: string, bytes: ArrayBuffer, thumb: ArrayBuffer | null | 'keep', base: number) {
    return this.call({ op: 'write', file, bytes, thumb, base }, thumb instanceof ArrayBuffer ? [bytes, thumb] : [bytes]);
  }
````

на:

````ts
  write(
    file: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ) {
    return this.call({ op: 'write', file, bytes, thumb, base, journal }, thumb instanceof ArrayBuffer ? [bytes, thumb] : [bytes]);
  }
  async appendJournal(file: string, mark: JournalMark, bytes: ArrayBuffer) {
    await this.call({ op: 'appendJournal', file, session: mark.session, batch: mark.batch, bytes }, [bytes]);
  }
  journalCount(file: string) {
    return this.call({ op: 'journalCount', file });
  }
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
      const meta = await files.write(id, ownBuffer(snap.bytes), thumb, base);
````

на:

````ts
      const meta = await files.write(id, ownBuffer(snap.bytes), thumb, base, null);
````

В `apps/web/e2e/files.spec.ts` заменить:

````ts
    await s.write(i, new ArrayBuffer(8), null, m.version);
````

на:

````ts
    await s.write(i, new ArrayBuffer(8), null, m.version, null);
````

- [ ] **Step 5: Проверить**

Run: `cd apps/web && npm run typecheck && npm run unit && npm run build && npx playwright test e2e/journal-store.spec.ts e2e/files.spec.ts`
Expected: PASS — `journal-store` 4 passed; `files.spec` — как раньше (база открывается с версией 2 поверх записей версии 1 без потерь: хранилища создаются по `contains`).

- [ ] **Step 6: Закоммитить**

```bash
git add apps/web/src apps/web/unit apps/web/e2e
git commit -m "feat(web): журнал изменений в IndexedDB — пачки по сеансам, чтение вместе с файлом, стирание в транзакции записи

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Автосохранение журналом, полный файл в простое, восстановление при открытии

`saver.ts` переписывается: правки уходят в журнал не позже чем через 2 с после первой правки (правки подряд пачку не откладывают — так после сбоя теряется не больше 2 с), полный файл пишется через 30 с после последней правки, не реже раза в 5 минут при работе без пауз, по Ctrl/Cmd+S и при уходе. Новый файл без байтов в хранилище (журнал не к чему применить) до первой записи сохраняется полным файлом через 2 с — как в части 1. Скрыли вкладку — сразу пачка; `pagehide` — пачка и попытка полного файла. Редактор при открытии читает пачки журнала вместе с файлом и открывает их командой `openWithJournal`; применено хоть что-то — уведомление «Восстановлены изменения, не записанные в файл» (документ изменён, следующий полный файл стирает журнал). Файл вместе с журналом больше 512 МБ — файл открывается без журнала с уведомлением «Журнал изменений слишком большой — файл открыт без несохранённых правок».

Тексты: «Не удалось записать журнал изменений: {причина}» (одно уведомление, пока запись снова не удастся; ядро тогда сохраняет полным файлом по таймеру простоя), «Не удалось сохранить файл: {причина}» — как в части 1.

В тестовом режиме интервалы задаются адресом: `?journal=<мс>` и `?fullSave=<мс>`.

**Files:**
- Modify: `apps/web/src/ui/saver.ts` (полностью)
- Modify: `apps/web/src/ui/boot.ts`
- Modify: `apps/web/src/ui/app.tsx`
- Test: `apps/web/unit/saver.test.ts`
- Test: `apps/web/e2e/journal.spec.ts`

**Interfaces:**
- Consumes: `FileStore.{read, write, appendJournal}` (Task 4); события `journal`, `recovered`, `snapshot.journal`, команды `flushJournal`, `openWithJournal` (Task 3).
- Produces: `createSaver(ed, files, id, base, stored, opts?: SaverOptions): Saver`; `Saver.{save, flushJournal, pending, failure, dispose}` (`flushJournal(): Promise<boolean>` — правки отданы и пачка записана; `pending(): boolean` — есть правки ни в журнале, ни в файле); `newSession(now?, random?): number`; константы `AUTOSAVE_MS = 2000`, `JOURNAL_MS = 2000`, `FULL_IDLE_MS = 30_000`, `FULL_MAX_MS = 300_000`; `BootOptions.document: Promise<{ bytes: ArrayBuffer; journal: ArrayBuffer[] } | null>`.

- [ ] **Step 1: Написать падающие тесты**

`apps/web/unit/saver.test.ts`:

````ts
// Сеанс журнала: ключи пачек одного файла упорядочены по времени открытия редактора
// и не совпадают у двух сеансов одной миллисекунды; число — целое в пределах
// Number.MAX_SAFE_INTEGER (так его принимает worker ввода-вывода).

import { expect, test } from '@playwright/test';
import { FULL_IDLE_MS, FULL_MAX_MS, JOURNAL_MS, newSession } from '../src/ui/saver';

test('сеанс журнала: по времени, целый, безопасный', () => {
  const t = Date.UTC(2026, 9, 28, 12, 0, 0);
  expect(newSession(t, 0)).toBe(t * 1000);
  expect(newSession(t, 0.9999)).toBe(t * 1000 + 999);
  expect(newSession(t + 1, 0)).toBeGreaterThan(newSession(t, 0.9999));
  // До 2250 года число остаётся безопасным целым.
  const far = Date.UTC(2250, 0, 1);
  expect(Number.isSafeInteger(newSession(far, 0.9999))).toBe(true);
});

test('интервалы: журнал — секунды, полный файл — реже', () => {
  expect(JOURNAL_MS).toBe(2000);
  expect(FULL_IDLE_MS).toBeGreaterThan(JOURNAL_MS * 10);
  expect(FULL_MAX_MS).toBeGreaterThan(FULL_IDLE_MS);
});
````

`apps/web/e2e/journal.spec.ts`:

````ts
// Автосохранение журналом (этап 6): правка — в журнал за 2 с, полный файл — позже;
// перезагрузка вкладки до полного файла восстанавливает правки с уведомлением; отказ
// записи файла правку не теряет (журнал пишется раньше); полный файл в простое и уход
// к экрану файлов стирают журнал.

import { expect, test, type Page } from '@playwright/test';
import { drawRect, editorReady, journalCount, liveWhen, storedFile } from './helpers';

const meta = (page: Page, id: string) => page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
const toast = (page: Page, text: string) => page.locator('.fe-toast', { hasText: text });
const modified = (page: Page) => page.evaluate(() => window.__ferrum!.ui!.modified);

/** Перезагрузка как сбой вкладки: полный файл при уходе не запишется (отказ записи
 * в этой вкладке; в новой worker ввода-вывода — новый, отказов нет). */
async function crashReload(page: Page): Promise<void> {
  await page.evaluate(() => window.__ferrumFiles!.store.failWrites(true));
  await page.reload();
  await editorReady(page);
}

test('правка — в журнал за 2 с, файл не переписан; перезагрузка восстанавливает @firefox', async ({ page }) => {
  const id = await storedFile(page, '&fullSave=600000');
  const version = (await meta(page, id))!.version;
  await drawRect(page, 150);
  await expect.poll(() => journalCount(page, id), { timeout: 10_000 }).toBeGreaterThan(0);
  expect((await meta(page, id))!.version, 'полный файл не писался').toBe(version);
  expect(await modified(page)).toBe(true);
  await crashReload(page);
  await liveWhen(page, 'l.nodes === 2');
  await expect(toast(page, 'Восстановлены изменения, не записанные в файл')).toHaveCount(1);
  expect(await modified(page), 'восстановленного нет в файле').toBe(true);
  // Ctrl+S: полный файл, журнал стёрт, признак снят; следующее открытие — без восстановления.
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => journalCount(page, id)).toBe(0);
  await expect.poll(() => modified(page)).toBe(false);
  await page.reload();
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 2');
  await expect(toast(page, 'Восстановлены')).toHaveCount(0);
});

test('журнал пишется раньше файла: отказ записи файла правку не теряет', async ({ page }) => {
  const id = await storedFile(page, '&fullSave=600000&journal=600000');
  await page.evaluate(() => window.__ferrumFiles!.store.failWrites(true));
  await drawRect(page, 150);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect(toast(page, 'Не удалось сохранить файл: недостаточно места в хранилище браузера')).toHaveCount(1);
  // Пачка с правкой ушла в журнал до попытки записать файл (таймер журнала — 10 минут).
  expect(await journalCount(page, id)).toBe(1);
  await crashReload(page);
  await liveWhen(page, 'l.nodes === 2');
});

test('полный файл в простое стирает журнал и снимает «изменён»', async ({ page }) => {
  const id = await storedFile(page, '&journal=500&fullSave=4000');
  const version = (await meta(page, id))!.version;
  await drawRect(page, 150);
  await expect.poll(() => journalCount(page, id), { timeout: 5_000 }).toBeGreaterThan(0);
  await expect.poll(async () => (await meta(page, id))!.version, { timeout: 15_000 }).toBeGreaterThan(version);
  expect(await journalCount(page, id)).toBe(0);
  await expect.poll(() => modified(page)).toBe(false);
});

test('уход к экрану файлов пишет полный файл, журнал пуст', async ({ page }) => {
  const id = await storedFile(page, '&fullSave=600000');
  await drawRect(page, 150);
  await expect.poll(() => journalCount(page, id), { timeout: 10_000 }).toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: 'Все файлы' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  expect(await journalCount(page, id)).toBe(0);
});

test('повреждённая пачка: файл открыт, одно уведомление о потерянном', async ({ page }) => {
  const id = await storedFile(page, '&fullSave=600000');
  await page.evaluate(
    (i) => window.__ferrumFiles!.store.appendJournal(i, { session: 1, batch: 1 }, new TextEncoder().encode('мусор').buffer as ArrayBuffer),
    id,
  );
  await page.reload();
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 1');
  await expect(
    toast(page, 'Не все несохранённые изменения удалось восстановить: журнал изменений повреждён'),
  ).toHaveCount(1);
});
````

В конец `apps/web/e2e/helpers.ts` добавить:

````ts

/** Новый файл в хранилище, уже записанный полным файлом (в нём один прямоугольник):
 * дальше правки идут в журнал. `query` — параметры адреса (`&fullSave=…`). */
export async function storedFile(page: Page, query = ''): Promise<string> {
  await testMode(page);
  await page.goto(`/?nodes=0${query}#/new`);
  await editorReady(page);
  const id = fileId(page);
  await drawRect(page);
  // Новый файл без байтов: первая запись — полным файлом через 2 с после правки.
  await expect
    .poll(() => page.evaluate((i) => window.__ferrumFiles!.store.get(i).then((m) => m?.size ?? 0), id), { timeout: 10_000 })
    .toBeGreaterThan(0);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  return id;
}
````

Run: `cd apps/web && npm run unit -- -g "сеанс журнала|интервалы"`
Expected: FAIL — нет `newSession`, `JOURNAL_MS`, `FULL_IDLE_MS`, `FULL_MAX_MS`.

- [ ] **Step 2: Автосохранение**

`apps/web/src/ui/saver.ts` (полностью):

````ts
// Сохранение открытого файла в хранилище (этап 6): журнал изменений и полный файл.
// - Журнал: не позже чем через `JOURNAL_MS` после правки ядро отдаёт пачку
//   обновлений Loro (`flushJournal` → событие `journal`, миллисекунды работы ядра),
//   она дописывается в журнал файла. Так правки переживают сбой ядра и закрытие
//   вкладки: при следующем открытии журнал применяется поверх файла.
// - Полный файл (снимок `snapshot`: на 100 000 узлах ≈ 0,45 с работы ядра) — в
//   простое: через `FULL_IDLE_MS` после последней правки и не реже `FULL_MAX_MS` при
//   работе без пауз; по Ctrl/Cmd+S; при уходе на экран файлов или к другому файлу
//   (уход ждёт записи). Перед снимком ядро отдаёт пачку с правками до него — она
//   дописывается раньше, чем пишется файл, а запись файла стирает пачки до номера
//   снимка в той же транзакции.
// - Новый файл без байтов в хранилище: журнал не к чему применить, поэтому до первой
//   записи он сохраняется полным файлом через `AUTOSAVE_MS` после правки.
// - Вкладку скрыли — сразу пачка журнала; `pagehide` — пачка и попытка полного файла
//   (без гарантии: вкладка может закрыться раньше записи).
// «Изменён» значит «не в файле»: признак снимает ядро (`markSaved`) только после
// записи полного файла и только если с момента снимка правок не было. Ошибка записи
// файла или журнала — одно уведомление, пока запись снова не удастся; документ
// остаётся открытым. Запись несёт поколение файла, прочитанное при открытии: файл
// записали в другой вкладке — отказ, а не тихая перезапись.

import type { CoreEvent } from '../wasm/ferrum_wasm';
import type { Editor } from './context';
import type { FileStore } from './file-store';
import { ownBuffer } from './file-store';

/** Новый файл без байтов в хранилище: полный файл через столько после правки. */
export const AUTOSAVE_MS = 2000;
/** Пачка журнала — не позже чем через столько после правки. */
export const JOURNAL_MS = 2000;
/** Полный файл — через столько после последней правки… */
export const FULL_IDLE_MS = 30_000;
/** …и не реже этого при правках без пауз. */
export const FULL_MAX_MS = 300_000;

export interface SaverOptions {
  /** Тесты: интервалы журнала и полного файла. */
  journalMs?: number;
  fullIdleMs?: number;
}

export interface Saver {
  /** Сохранить полный файл, если документ изменён; `true` — записано (или нечего). */
  save(): Promise<boolean>;
  /** Отдать журналу правки сейчас и дождаться записи пачки; `true` — всё в журнале. */
  flushJournal(): Promise<boolean>;
  /** Есть правки, которых нет ни в журнале, ни в файле (сообщение после сбоя ядра). */
  pending(): boolean;
  /** Почему не удалась последняя запись (для вопроса при уходе); `null` — удалась. */
  failure(): string | null;
  dispose(): void;
}

/** Сеанс редактора для ключей журнала: время открытия в мс и три случайных разряда —
 * сеансы одного файла упорядочены по времени и не совпадают. */
export function newSession(now = Date.now(), random = Math.random()): number {
  return now * 1000 + Math.min(999, Math.floor(random * 1000));
}

type Snapshot = CoreEvent & { type: 'snapshot' };
type Journal = CoreEvent & { type: 'journal' };

/** `base` — поколение файла (`FileMeta.version`) на момент открытия; `stored` — у
 * файла есть байты в хранилище (журнал есть к чему применить). */
export function createSaver(
  ed: Editor,
  files: FileStore,
  id: string,
  base: number,
  stored: boolean,
  opts: SaverOptions = {},
): Saver {
  const journalMs = opts.journalMs ?? JOURNAL_MS;
  const fullIdleMs = opts.fullIdleMs ?? FULL_IDLE_MS;
  const session = newSession();
  /** «Изменён» по событиям ядра сразу при получении, без ожидания кадра планировщика. */
  let modified = false;
  /** Правки после последней пачки журнала (у нового файла — после последней записи). */
  let dirty = false;
  let journalTimer = 0;
  let fullTimer = 0;
  let lastFull = performance.now();
  let running: Promise<boolean> | null = null;
  let again: Promise<boolean> | null = null;
  /** Последняя показанная ошибка записи: повтор не показываем, пока запись не удастся. */
  let reported: string | null = null;
  let journalReported: string | null = null;
  let failure: string | null = null;
  let waiting: ((e: Snapshot) => void) | null = null;
  let failWaiting: (() => void) | null = null;
  /** Пачки дописываются по порядку прихода; запись файла ждёт последнюю. */
  let appended: Promise<boolean> = Promise.resolve(true);
  /** Ждут ответа ядра на `flushJournal`. */
  const journalWaiters: ((done: Promise<boolean>) => void)[] = [];

  const edited = () => {
    dirty = true;
    clearTimeout(fullTimer);
    if (!stored) {
      fullTimer = window.setTimeout(() => void save(), AUTOSAVE_MS);
      return;
    }
    // Пачка — не позже `journalMs` после первой правки: правки подряд её не откладывают.
    if (!journalTimer) journalTimer = window.setTimeout(() => void flushJournal(), journalMs);
    fullTimer = window.setTimeout(() => void save(), fullIdleMs);
  };

  const off = ed.core.on((e) => {
    switch (e.type) {
      case 'modified':
        modified = e.modified;
        if (modified) edited();
        break;
      case 'document':
      case 'documentName':
      case 'pages':
        // Правка, пока документ уже изменён: журнал и полный файл — по таймерам.
        if (modified) edited();
        break;
      case 'journal':
        onJournal(e);
        break;
      case 'saveRequested':
        // После всей пачки событий: правка и Ctrl+S в одном кадре ядра присылают
        // `modified` позже `saveRequested`.
        queueMicrotask(() => void save());
        break;
      case 'snapshot':
        waiting?.(e);
        break;
      case 'error':
        // Снимок не удался или ядро остановлено: ответа не будет. Сообщение ядра уже показано.
        if (e.message.startsWith('Не удалось сохранить файл') || e.message.startsWith('Ядро остановлено')) {
          failure = e.message;
          failWaiting?.();
        }
        if (e.message.startsWith('Ядро остановлено')) for (const w of journalWaiters.splice(0)) w(Promise.resolve(false));
        break;
    }
  });

  const fail = (message: string) => {
    failure = message;
    if (reported === message) return;
    reported = message;
    ed.store.toast(message, 'error');
  };

  /** Пачка журнала от ядра (по `flushJournal` или перед снимком). У нового файла —
   * не нужна: её вберёт первая запись полного файла. */
  function onJournal(e: Journal): void {
    if (stored && e.bytes) {
      const bytes = ownBuffer(e.bytes);
      const p = files.appendJournal(id, { session, batch: e.batch }, bytes).then(
        () => {
          journalReported = null;
          return true;
        },
        (err: unknown) => {
          dirty = true;
          const message = `Не удалось записать журнал изменений: ${err instanceof Error ? err.message : String(err)}`;
          if (journalReported !== message) {
            journalReported = message;
            ed.store.toast(message, 'error');
          }
          return false;
        },
      );
      appended = appended.then(() => p);
    }
    // Всё до этой пачки — в ней (правки после неё снова поставят признак).
    if (stored) dirty = false;
    const done = appended;
    for (const w of journalWaiters.splice(0)) w(done);
    // Долгая работа без пауз: полный файл не реже `FULL_MAX_MS`.
    if (stored && modified && performance.now() - lastFull > FULL_MAX_MS) void save();
  }

  function flushJournal(): Promise<boolean> {
    clearTimeout(journalTimer);
    journalTimer = 0;
    if (!stored || ed.store.state.stopped) return Promise.resolve(!dirty);
    return new Promise<boolean>((resolve) => {
      journalWaiters.push((done) => void done.then((ok) => resolve(ok && !dirty)));
      ed.send({ type: 'flushJournal' });
    });
  }

  async function write(): Promise<boolean> {
    clearTimeout(fullTimer);
    if (!modified) return true;
    if (ed.store.state.stopped) {
      // Плашка остановки уже на экране: здесь — только причина для вопроса при уходе.
      failure = 'Ядро остановлено — изменения не сохранены';
      return false;
    }
    const snap = await new Promise<Snapshot | null>((resolve) => {
      waiting = resolve;
      failWaiting = () => resolve(null);
      ed.send({ type: 'snapshot', thumbnail: true });
    });
    waiting = null;
    failWaiting = null;
    if (!snap) return false;
    const wasStored = stored;
    // У нового файла журнала нет: правки до снимка вберёт запись (отказ вернёт признак).
    if (!wasStored) dirty = false;
    try {
      // Пачка с правками до снимка дописывается раньше файла.
      await appended;
      const thumb = snap.thumbnail ? ownBuffer(snap.thumbnail) : snap.keepThumbnail ? 'keep' : null;
      const journal = wasStored ? { session, upTo: snap.journal } : null;
      const meta = await files.write(id, ownBuffer(snap.bytes), thumb, base, journal);
      base = meta.version;
      stored = true;
      lastFull = performance.now();
      ed.send({ type: 'markSaved', revision: snap.revision });
      reported = null;
      failure = null;
      return true;
    } catch (err) {
      if (!wasStored) dirty = true;
      fail(`Не удалось сохранить файл: ${err instanceof Error ? err.message : String(err)}`);
      return false;
    }
  }

  /** Одна запись за раз; просьбы во время записи сливаются в одну следующую. */
  function save(): Promise<boolean> {
    if (!running) {
      const p = write().finally(() => {
        if (running === p) running = null;
      });
      running = p;
      return p;
    }
    again ??= running.then(() => {
      again = null;
      return save();
    });
    return again;
  }

  const onHide = () => {
    void flushJournal();
    void save();
  };
  const onVisibility = () => {
    if (document.visibilityState === 'hidden') void flushJournal();
  };
  window.addEventListener('pagehide', onHide);
  document.addEventListener('visibilitychange', onVisibility);

  return {
    save,
    flushJournal,
    pending: () => dirty || (!stored && modified),
    failure: () => failure,
    dispose() {
      clearTimeout(journalTimer);
      clearTimeout(fullTimer);
      off();
      window.removeEventListener('pagehide', onHide);
      document.removeEventListener('visibilitychange', onVisibility);
      failWaiting?.();
      for (const w of journalWaiters.splice(0)) w(Promise.resolve(false));
    },
  };
}
````

- [ ] **Step 3: Открытие с журналом**

В `apps/web/src/ui/boot.ts` заменить:

````ts
  /** Байты файла (читаются, пока запускается ядро); `null` — новый файл. */
  document?: Promise<ArrayBuffer | null>;
````

на:

````ts
  /** Байты файла и пачки журнала после них (читаются, пока запускается ядро);
   * `null` — новый файл. */
  document?: Promise<{ bytes: ArrayBuffer; journal: ArrayBuffer[] } | null>;
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
  const bytes = (await opts.document) ?? null;
  if (bytes) core.send({ type: 'open', bytes: new Uint8Array(bytes) });
  else core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 0) });
````

на:

````ts
  const doc = (await opts.document) ?? null;
  if (doc && doc.journal.length > 0) {
    // Правки после файла (сбой, закрытая вкладка): ядро применит их поверх него.
    core.send({ type: 'openWithJournal', bytes: new Uint8Array(doc.bytes), journal: doc.journal.map((b) => new Uint8Array(b)) });
  } else if (doc) core.send({ type: 'open', bytes: new Uint8Array(doc.bytes) });
  else core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 0) });
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
import { boot, dismissSplash } from './boot';
````

на:

````ts
import { MAX_OPEN_BYTES } from '../core/limits';
import { boot, dismissSplash, isTestMode } from './boot';
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
import { createSaver, type Saver } from './saver';
````

на:

````ts
import { createSaver, type Saver, type SaverOptions } from './saver';
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
export function App(props: AppProps) {
````

на:

````ts
/** Тестовый режим: интервалы журнала и полного файла из адреса (`?journal=`, `?fullSave=`). */
function testIntervals(): SaverOptions {
  if (!isTestMode()) return {};
  const p = new URLSearchParams(location.search);
  const ms = (k: string) => {
    const v = Number(p.get(k));
    return p.has(k) && Number.isFinite(v) && v > 0 ? v : undefined;
  };
  return { journalMs: ms('journal'), fullIdleMs: ms('fullSave') };
}

export function App(props: AppProps) {
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
  const bytes = props.files.read(id).then(
    (r) => ((stored = r.bytes !== null), (version = r.meta.version), r.bytes),
````

на:

````ts
  const bytes = props.files.read(id).then(
    (r) => {
      stored = r.bytes !== null;
      version = r.meta.version;
      if (!r.bytes) return null;
      let journal = r.journal;
      const total = journal.reduce((n, b) => n + b.byteLength, r.bytes.byteLength);
      if (total > MAX_OPEN_BYTES) {
        // Файл с журналом больше предела открытия: открыть хотя бы файл.
        store.toast('Журнал изменений слишком большой — файл открыт без несохранённых правок', 'error');
        journal = [];
      }
      return { bytes: r.bytes, journal };
    },
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
        saver = createSaver(ed, props.files, id, version);
````

на:

````ts
        saver = createSaver(ed, props.files, id, version, stored, testIntervals());
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
        let opening = true;
        ed.core.on((e) => {
          if (opening && e.type === 'opened') {
            opening = false;
````

на:

````ts
        let opening = true;
        let recovered = false;
        ed.core.on((e) => {
          if (e.type === 'recovered') recovered = true;
          if (opening && e.type === 'opened') {
            opening = false;
            if (recovered) store.toast('Восстановлены изменения, не записанные в файл');
````

Тест `chrome.spec.ts` «правки вне истории…» ждал, что отложенное сохранение снимет «изменён» за 10 с; теперь у записанного файла правки идут в журнал, а полный файл — через 30 с простоя. Тест задаёт тестовый интервал полного файла.

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
test('правки вне истории (страница, имя файла) — «изменён» и запись в хранилище; копия на диск признак не снимает', async ({ page }) => {
  await openApp(page);
````

на:

````ts
test('правки вне истории (страница, имя файла) — «изменён» и запись в хранилище; копия на диск признак не снимает', async ({ page }) => {
  // Полный файл в простое — через 3 с (по умолчанию 30 с): правка имени записывается им.
  await openApp(page, 1000, '&fullSave=3000');
````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  // Отложенное сохранение снимает признак само.
````

на:

````ts
  // Полный файл в простое снимает признак сам.
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && npm run typecheck && npm run unit && npm run build && npx playwright test e2e/journal.spec.ts e2e/files.spec.ts e2e/chrome.spec.ts`
Expected: PASS — `journal` 5 passed (проект `chromium`), `files.spec` — как раньше.

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/src apps/web/unit apps/web/e2e
git commit -m "feat(web): автосохранение журналом за 2 с, полный файл в простое и при уходе, восстановление правок при открытии

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Сбой ядра: перехват паники и перезапуск редактора

Спека 11: «Паника в Rust перехватывается, worker перезапускается, документ восстанавливается из журнала; интерфейс сообщает „Редактор перезапущен, изменения сохранены“». Свой обработчик паники вызывает `console_error_panic_hook` (текст и стек — в консоль worker'а) и отдаёт текст паники worker'у через `globalThis.__ferrumPanic` до того, как wasm остановится; сообщение об остановке тогда «Ядро остановлено: паника в ядре: …», а не «unreachable». Ядро после паники (как и после исключения в worker'е) непригодно, а холст второй раз не передать, поэтому перезапуск — это новый редактор с новым `<canvas>` для того же файла: корень перерисовывает `App` с отметкой `restart`, блокировка файла остаётся за вкладкой, метаданные читаются заново (имя могли сменить), байты и журнал — тоже. Новый редактор показывает одно уведомление: «Редактор перезапущен, изменения сохранены» или, если были правки ни в журнале, ни в файле (паника раньше пачки, ≤ 2 с правок), — «Редактор перезапущен. Правки последних секунд не сохранились». Не больше двух перезапусков за минуту: третий сбой оставляет прежнюю плашку «Ядро остановлено: … Правки больше не принимаются — перезагрузите страницу». Сбой во время открытия файла (до `opened`) перезапуска не вызывает — иначе испорченный журнал крутил бы его вечно.

Проверка — настоящей паникой: `WasmCore.panicForTests()` worker вызывает только в тестовом режиме (`init.test`), как `seed` и `failWrites` у worker'а ввода-вывода; в обычном режиме сообщение `testPanic` — «Неверное сообщение для ядра».

**Files:**
- Modify: `crates/ferrum-wasm/src/web/mod.rs`
- Create: `apps/web/src/ui/restart.ts`
- Modify: `apps/web/src/core/messages.ts`
- Modify: `apps/web/src/core/core.ts`
- Modify: `apps/web/src/worker/core.worker.ts`
- Modify: `apps/web/src/ui/boot.ts`
- Modify: `apps/web/src/ui/app.tsx`
- Modify: `apps/web/src/ui/root.tsx`
- Test: `apps/web/unit/restart.test.ts`
- Test: `apps/web/e2e/crash.spec.ts`

**Interfaces:**
- Consumes: `Saver.pending()` (Task 5); журнал при открытии (Task 5).
- Produces: `Restart { reason: 'crash' | 'gpu'; lost: boolean; backend?: Backend; message: string }`, `allowRestart(history: number[], now: number): boolean`, `restartText(r: Restart): string`, `RESTART_LIMIT = 2`, `RESTART_WINDOW_MS = 60_000` (`ui/restart.ts`); `AppProps.{restart?, backend?, onRestart(r): boolean}`; `BootOptions.backend?: Backend`; `CoreOptions.test?: boolean`, `Core.testPanic()`; `ToWorker` — `init.test: boolean`, `{ t: 'testPanic' }`; `WasmCore.panicForTests()`.

- [ ] **Step 1: Написать падающие тесты**

`apps/web/unit/restart.test.ts`:

````ts
// Перезапуск редактора после сбоя: не больше двух за минуту, тексты уведомлений.

import { expect, test } from '@playwright/test';
import { allowRestart, restartText, RESTART_LIMIT, RESTART_WINDOW_MS } from '../src/ui/restart';

test('не больше двух перезапусков за минуту', () => {
  expect(RESTART_LIMIT).toBe(2);
  const h: number[] = [];
  expect(allowRestart(h, 1000)).toBe(true);
  expect(allowRestart(h, 2000)).toBe(true);
  expect(allowRestart(h, 3000)).toBe(false);
  expect(h).toEqual([1000, 2000]);
  // Через минуту после первого — снова можно.
  expect(allowRestart(h, 1000 + RESTART_WINDOW_MS)).toBe(true);
  expect(h).toEqual([2000, 1000 + RESTART_WINDOW_MS]);
});

test('тексты: сохранено, потеряны секунды, запасной рендер', () => {
  expect(restartText({ reason: 'crash', lost: false, message: '' })).toBe('Редактор перезапущен, изменения сохранены');
  expect(restartText({ reason: 'crash', lost: true, message: '' })).toBe(
    'Редактор перезапущен. Правки последних секунд не сохранились',
  );
  expect(restartText({ reason: 'gpu', lost: false, backend: 'webgl', message: '' })).toBe(
    'Видеокарта недоступна — редактор перезапущен с рендером WebGL2',
  );
  expect(restartText({ reason: 'gpu', lost: false, backend: 'cpu', message: '' })).toBe(
    'Видеокарта недоступна — редактор перезапущен с рендером на процессоре',
  );
  expect(restartText({ reason: 'gpu', lost: true, backend: 'cpu', message: '' })).toBe(
    'Видеокарта недоступна — редактор перезапущен с рендером на процессоре. Правки последних секунд не сохранились',
  );
});
````

`apps/web/e2e/crash.spec.ts`:

````ts
// Сбой ядра (этап 6, спека 11): паника в Rust — редактор перезапущен сам, правки из
// журнала на месте, одно уведомление и никакой плашки остановки; паника раньше пачки
// журнала — уведомление о потерянных секундах; третий сбой за минуту — плашка.

import { expect, test, type Page } from '@playwright/test';
import { drawRect, editorReady, fileId, journalCount, liveWhen, storedFile } from './helpers';

const toast = (page: Page, text: string) => page.locator('.fe-toast', { hasText: text });

/** Настоящая паника в ядре (тестовый режим) и ожидание нового ядра. */
async function panicAndRestart(page: Page, text: string): Promise<void> {
  await page.evaluate(() => ((window.__ferrum!.core as unknown as { old?: boolean }).old = true));
  await page.evaluate(() => window.__ferrum!.core!.testPanic());
  await expect(toast(page, text)).toHaveCount(1);
  await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
  await editorReady(page);
}

test('паника ядра: редактор перезапущен, правки из журнала на месте', async ({ page }) => {
  const logs: string[] = [];
  page.on('console', (m) => logs.push(m.text()));
  const id = await storedFile(page, '&fullSave=600000');
  await drawRect(page, 150);
  await expect.poll(() => journalCount(page, id), { timeout: 10_000 }).toBeGreaterThan(0);
  await panicAndRestart(page, 'Редактор перезапущен, изменения сохранены');
  await liveWhen(page, 'l.nodes === 2');
  expect(fileId(page)).toBe(id);
  await expect(page.locator('.fe-boot--stopped')).toHaveCount(0);
  await expect(toast(page, 'Ядро остановлено')).toHaveCount(0);
  // Причина — в консоли: текст паники дошёл до worker'а раньше остановки wasm.
  expect(logs.some((t) => t.includes('Ядро остановлено: паника в ядре: тестовая паника ядра'))).toBe(true);
  // Новый редактор работает и сохраняет.
  await drawRect(page, 300);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  expect(await journalCount(page, id)).toBe(0);
});

test('паника раньше пачки журнала: перезапуск и уведомление о потерянных секундах', async ({ page }) => {
  const id = await storedFile(page, '&fullSave=600000&journal=600000');
  await drawRect(page, 150);
  await panicAndRestart(page, 'Редактор перезапущен. Правки последних секунд не сохранились');
  await liveWhen(page, 'l.nodes === 1');
  expect(await journalCount(page, id)).toBe(0);
});

test('третий сбой за минуту: плашка остановки, без перезапуска', async ({ page }) => {
  await storedFile(page);
  for (let i = 0; i < 2; i++) await panicAndRestart(page, 'Редактор перезапущен');
  await page.evaluate(() => window.__ferrum!.core!.testPanic());
  await expect(page.getByRole('alert')).toContainText('Ядро остановлено: паника в ядре: тестовая паника ядра');
});

test('вне тестового режима паника по сообщению не вызывается', async ({ page }) => {
  // Обычный режим: ядро не раскрыто, сообщение шлём в worker напрямую.
  await page.addInitScript(() => {
    const Base = window.Worker;
    window.Worker = class extends Base {
      constructor(url: string | URL, opts?: WorkerOptions) {
        super(url, opts);
        if (String(url).includes('core.worker')) (window as unknown as { __coreWorker: Worker }).__coreWorker = this;
      }
    };
  });
  await page.goto('/?nodes=10#/new');
  await page.waitForFunction(() => window.__ferrum?.ready === true);
  await page.evaluate(() => (window as unknown as { __coreWorker: Worker }).__coreWorker.postMessage({ t: 'testPanic' }));
  await expect(toast(page, 'Неверное сообщение для ядра')).toHaveCount(1);
  await expect(page.locator('.fe-boot--stopped')).toHaveCount(0);
});
````

Run: `cd apps/web && npm run unit -- -g "перезапуск|тексты: сохранено"`
Expected: FAIL — нет модуля `src/ui/restart.ts`.

- [ ] **Step 2: Перехват паники и тестовая паника в ядре**

В `crates/ferrum-wasm/src/web/mod.rs` заменить:

````rust
#[wasm_bindgen(start)]
pub fn start() {
    // Паника в Rust — в консоль worker'а с текстом и стеком, а не «unreachable».
    console_error_panic_hook::set_once();
}
````

на:

````rust
#[wasm_bindgen(start)]
pub fn start() {
    // Паника в Rust — в консоль worker'а с текстом и стеком (а не «unreachable»), и
    // её текст — worker'у до остановки wasm: «Ядро остановлено: …» несёт причину.
    std::panic::set_hook(Box::new(|info| {
        console_error_panic_hook::hook(info);
        report_panic(&panic_text(info));
    }));
}

/// Текст паники без места в коде: его увидит пользователь.
fn panic_text(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "без описания".into())
}

/// Отдаёт текст `globalThis.__ferrumPanic`, если worker его ждёт (в тестах wasm в
/// Node его нет — тогда только консоль).
fn report_panic(message: &str) {
    let global = js_sys::global();
    if let Ok(f) = js_sys::Reflect::get(&global, &JsValue::from_str("__ferrumPanic")) {
        if let Some(f) = f.dyn_ref::<js_sys::Function>() {
            let _ = f.call1(&global, &JsValue::from_str(message));
        }
    }
}
````

В `crates/ferrum-wasm/src/web/mod.rs` заменить:

````rust
    /// Активный бэкенд: `webgpu`, `webgl` или `cpu`.
    pub fn backend(&self) -> String {
````

на:

````rust
    /// Только для тестов (worker зовёт её лишь в тестовом режиме): настоящая паника
    /// ядра — проверка перехвата и перезапуска.
    #[wasm_bindgen(js_name = panicForTests)]
    pub fn panic_for_tests(&mut self) {
        panic!("тестовая паника ядра");
    }

    /// Активный бэкенд: `webgpu`, `webgl` или `cpu`.
    pub fn backend(&self) -> String {
````

Run: `cargo clippy --target wasm32-unknown-unknown -p ferrum-wasm --lib --tests -- -D warnings && cd apps/web && npm run wasm`
Expected: без предупреждений; `ferrum_wasm.d.ts` содержит `panicForTests(): void;` (проверено сборкой прототипа).

- [ ] **Step 3: Перезапуск — тексты и предел**

`apps/web/src/ui/restart.ts`:

````ts
// Перезапуск редактора (этап 6, спека 11): после сбоя ядра (паника в Rust, исключение
// в worker'е) и после потери GPU, которую не удалось пережить на месте, тот же файл
// открывается в новом редакторе с новым холстом (OffscreenCanvas второй раз не
// передать): документ — из файла и журнала. Не чаще `RESTART_LIMIT` раз за
// `RESTART_WINDOW_MS` — иначе повторяющийся сбой крутился бы вечно; дальше остаётся
// плашка остановки ядра.

import type { Backend } from '../wasm/ferrum_wasm';

export interface Restart {
  reason: 'crash' | 'gpu';
  /** Были правки ни в журнале, ни в файле — они потеряны. */
  lost: boolean;
  /** Бэкенд нового ядра (после потери GPU — запасной). */
  backend?: Backend;
  /** Причина — для консоли. */
  message: string;
}

export const RESTART_LIMIT = 2;
export const RESTART_WINDOW_MS = 60_000;

/** Можно ли перезапустить ещё раз; можно — момент записывается в `history`. */
export function allowRestart(history: number[], now: number): boolean {
  while (history.length && now - history[0] >= RESTART_WINDOW_MS) history.shift();
  if (history.length >= RESTART_LIMIT) return false;
  history.push(now);
  return true;
}

const LOST = 'Правки последних секунд не сохранились';

/** Уведомление в новом редакторе. */
export function restartText(r: Restart): string {
  if (r.reason === 'gpu') {
    const name = r.backend === 'webgl' ? 'WebGL2' : r.backend === 'webgpu' ? 'WebGPU' : 'на процессоре';
    return `Видеокарта недоступна — редактор перезапущен с рендером ${name}${r.lost ? `. ${LOST}` : ''}`;
  }
  return r.lost ? `Редактор перезапущен. ${LOST}` : 'Редактор перезапущен, изменения сохранены';
}
````

- [ ] **Step 4: Тестовый режим ядра и текст паники в worker'е**

В `apps/web/src/core/messages.ts` заменить:

````ts
      /** Асинхронный API не умеет свой формат: событие `clipboard` несёт и текст с меткой. */
      clipboardText: boolean;
    }
````

на:

````ts
      /** Асинхронный API не умеет свой формат: событие `clipboard` несёт и текст с меткой. */
      clipboardText: boolean;
      /** Тестовый режим страницы: доступны тестовые сообщения (`testPanic`). */
      test: boolean;
    }
  /** Только тестовый режим: настоящая паника в ядре (проверка перезапуска). */
  | { t: 'testPanic' }
````

В `apps/web/src/core/messages.ts` заменить:

````ts
export const TO_WORKER_KINDS: ReadonlySet<string> = new Set(['init', 'cmd', 'pasteText', 'encodeClipboard', 'tick']);
````

на:

````ts
export const TO_WORKER_KINDS: ReadonlySet<string> = new Set(['init', 'cmd', 'pasteText', 'encodeClipboard', 'tick', 'testPanic']);
````

В `apps/web/src/core/core.ts` заменить:

````ts
  clipboardText?: boolean;
  /** Пересылать ли клавишу редактору.
````

на:

````ts
  clipboardText?: boolean;
  /** Тестовый режим страницы: worker принимает тестовые сообщения (`testPanic`). */
  test?: boolean;
  /** Пересылать ли клавишу редактору.
````

В `apps/web/src/core/core.ts` заменить:

````ts
        clipboardText: opts.clipboardText ?? false,
      },
      [offscreen],
````

на:

````ts
        clipboardText: opts.clipboardText ?? false,
        test: opts.test ?? false,
      },
      [offscreen],
````

В `apps/web/src/core/core.ts` заменить:

````ts
  /** Текст с меткой для байтов буфера обмена — кодирует worker
````

на:

````ts
  /** Только тестовый режим: настоящая паника в ядре — проверка перезапуска. Вне него
   * worker отвечает «Неверное сообщение для ядра». */
  testPanic(): void {
    this.transport.post({ t: 'testPanic' });
  }

  /** Текст с меткой для байтов буфера обмена — кодирует worker
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
let clipboardText = false;
````

на:

````ts
let clipboardText = false;
/** Тестовый режим страницы (`init.test`): принимаются тестовые сообщения. */
let testHooks = false;
/** Текст паники Rust: обработчик паники отдаёт его до остановки wasm (`web/mod.rs`). */
let panicText: string | null = null;
(self as unknown as { __ferrumPanic?: (m: string) => void }).__ferrumPanic = (m) => {
  panicText = m;
};
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  post({ t: 'fatal', message: e instanceof Error ? e.message : String(e) });
````

на:

````ts
  // После паники wasm бросает «unreachable»: причина — текст самой паники.
  const message = panicText !== null ? `паника в ядре: ${panicText}` : e instanceof Error ? e.message : String(e);
  post({ t: 'fatal', message });
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
      case 'init':
        break;
    }
````

на:

````ts
      case 'testPanic':
        // Паника бросает исключение: его ловит общий обработчик ниже (`fatal`).
        if (testHooks) core.panicForTests();
        else postError('Неверное сообщение для ядра');
        break;
      case 'init':
        break;
    }
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  clipboardText = msg.clipboardText === true;
````

на:

````ts
  clipboardText = msg.clipboardText === true;
  testHooks = msg.test === true;
````

- [ ] **Step 5: Перезапуск в интерфейсе**

В `apps/web/src/ui/boot.ts` заменить:

````ts
  /** Байты файла и пачки журнала после них (читаются, пока запускается ядро);
````

на:

````ts
  /** Бэкенд рендера (после потери GPU — запасной); без него — из адреса или автовыбор. */
  backend?: Backend;
  /** Байты файла и пачки журнала после них (читаются, пока запускается ядро);
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
    backend: choice<Backend>(params, 'backend', ['webgpu', 'webgl', 'cpu']),
````

на:

````ts
    backend: opts.backend ?? choice<Backend>(params, 'backend', ['webgpu', 'webgl', 'cpu']),
    test,
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
import type { Route } from './router';
````

на:

````ts
import type { Backend } from '../wasm/ferrum_wasm';
import { restartText, type Restart } from './restart';
import type { Route } from './router';
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
  /** Файл из хранилища не открылся (испорчен): к экрану файлов с этим сообщением. */
  onOpenFailed: (message: string) => void;
````

на:

````ts
  /** Файл из хранилища не открылся (испорчен): к экрану файлов с этим сообщением. */
  onOpenFailed: (message: string) => void;
  /** Редактор запущен заново после сбоя ядра или потери GPU: что сказать пользователю. */
  restart?: Restart;
  /** Бэкенд рендера (после потери GPU — запасной). */
  backend?: Backend;
  /** Ядро остановилось (или GPU недоступен): открыть этот же файл в новом редакторе.
   * `false` — перезапусков слишком много, остаётся плашка остановки. */
  onRestart: (r: Restart) => boolean;
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
    boot(canvas, store, {
      document: bytes,
````

на:

````ts
    boot(canvas, store, {
      document: bytes,
      backend: props.backend,
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
            if (recovered) store.toast('Восстановлены изменения, не записанные в файл');
````

на:

````ts
            if (props.restart) store.toast(restartText(props.restart), props.restart.lost ? 'error' : 'info');
            else if (recovered) store.toast('Восстановлены изменения, не записанные в файл');
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
          if (e.type === 'exported') {
````

на:

````ts
          if (!opening && e.type === 'error' && e.message.startsWith('Ядро остановлено:')) {
            // Сбой ядра после открытия (паника, исключение в worker'е): тот же файл — в
            // новом редакторе, правки — из файла и журнала. Сбой во время открытия
            // перезапуска не вызывает: испорченный журнал крутил бы его вечно.
            console.error(e.message);
            props.onRestart({ reason: 'crash', lost: saver?.pending() ?? false, message: e.message });
          }
          if (e.type === 'exported') {
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
import { historyIndex, parseRoute, routeHash, routeUrl, type Route } from './router';

type View =
  | { kind: 'wait' }
  | { kind: 'files'; section: Section; notice: { text: string; seq: number } | null }
  | { kind: 'editor'; file: FileMeta };
````

на:

````ts
import type { Backend } from '../wasm/ferrum_wasm';
import { allowRestart, type Restart } from './restart';
import { historyIndex, parseRoute, routeHash, routeUrl, type Route } from './router';

type View =
  | { kind: 'wait' }
  | { kind: 'files'; section: Section; notice: { text: string; seq: number } | null }
  /** `restart` — редактор запущен заново после сбоя; `backend` — бэкенд рендера
   * (после потери GPU — запасной, и для следующих перезапусков тоже). */
  | { kind: 'editor'; file: FileMeta; restart?: Restart; backend?: Backend };
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
  /** Открыть файл в редакторе: занят другой вкладкой — к экрану файлов. */
````

на:

````ts
  /** Моменты перезапусков редактора — для предела `allowRestart`. */
  const restarts: number[] = [];
  /** Тот же файл — в новом редакторе (сбой ядра, потеря GPU). Блокировка файла
   * остаётся за вкладкой; метаданные — заново (имя могли сменить). `false` —
   * перезапусков слишком много. */
  const restart = (file: FileMeta, r: Restart, backend: Backend | undefined): boolean => {
    if (!allowRestart(restarts, performance.now())) return false;
    // Ядро остановлено: прежний уход (сохранить и остановить) больше не нужен.
    leave = null;
    const my = ++seq;
    const show = (m: FileMeta) => setView({ kind: 'editor', file: m, restart: r, backend: r.backend ?? backend });
    files.get(file.id).then(
      (m) => {
        if (my !== seq) return;
        if (m) show(m);
        else {
          release();
          toFiles('Файл не найден');
        }
      },
      () => my === seq && show(file),
    );
    return true;
  };

  /** Открыть файл в редакторе: занят другой вкладкой — к экрану файлов. */
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
          <App
            file={v.file}
            files={files}
            navigate={navigate}
````

на:

````ts
          <App
            file={v.file}
            files={files}
            restart={v.restart}
            backend={v.backend}
            onRestart={(r) => restart(v.file, r, v.backend)}
            navigate={navigate}
````

- [ ] **Step 6: Тест «ядро остановилось — уйти с вопросом» — после исчерпанного предела**

Тест `files.spec.ts` «ядро остановилось с несохранёнными правками: уйти можно, но с вопросом…» останавливает ядро вызовом `Core.fail('проверка')` после открытия — теперь это перезапуск. Вопрос об уходе остаётся для остановки, когда перезапуски исчерпаны: тест сначала проходит два перезапуска (каждый — с правкой в новом редакторе), третья остановка оставляет плашку.

В `apps/web/e2e/files.spec.ts` заменить:

````ts
  await page.evaluate(() => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка'));
  await expect(page.getByRole('alert')).toContainText('Ядро остановлено: проверка');
````

на:

````ts
  // Два перезапуска за минуту исчерпывают предел (этап 6): третья остановка остаётся.
  for (let i = 0; i < 2; i++) {
    await page.evaluate(() => ((window.__ferrum!.core as unknown as { old?: boolean }).old = true));
    await page.evaluate(() => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка'));
    await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
    await editorReady(page);
    await drawRect(page);
    await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  }
  await page.evaluate(() => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка'));
  await expect(page.getByRole('alert')).toContainText('Ядро остановлено: проверка');
````

Тест `app.spec.ts` «сбой ядра после запуска виден пользователю один раз» тоже останавливает ядро после открытия: теперь первые две остановки перезапускают редактор, плашка — после третьей.

В `apps/web/e2e/app.spec.ts` заменить:

````ts
  await openApp(page);
  // Сбой worker'а после `ready` — тот же путь, что `fatal` от worker'а.
  await page.evaluate(() => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка'));
````

на:

````ts
  await openApp(page);
  // Сбой worker'а после `ready` — тот же путь, что `fatal` от worker'а. Первые два —
  // перезапуск редактора (этап 6), третий за минуту оставляет плашку.
  for (let i = 0; i < 2; i++) {
    await page.evaluate(() => ((window.__ferrum!.core as unknown as { old?: boolean }).old = true));
    await page.evaluate(() => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка'));
    await expect(page.locator('.fe-toast', { hasText: 'Редактор перезапущен' })).toHaveCount(1);
    await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
    await page.waitForFunction(() => (window.__ferrum!.core!.live?.frames ?? 0) > 0);
  }
  await page.evaluate(() => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка'));
````

В `apps/web/e2e/app.spec.ts` заменить:

````ts
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(r)));
  await expect(page.locator('.fe-toast')).toHaveCount(0);
  expect(await page.evaluate(() => window.__ferrum!.ui!.toasts.length)).toBe(0);
````

на:

````ts
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(r)));
  // Новых уведомлений нет: осталось только «Редактор перезапущен…» последнего перезапуска.
  await expect(page.locator('.fe-toast', { hasText: 'Ядро остановлено' })).toHaveCount(0);
  expect(await page.evaluate(() => window.__ferrum!.ui!.toasts.filter((t) => !t.text.startsWith('Редактор перезапущен')).length)).toBe(0);
````

- [ ] **Step 7: Проверить**

Run: `cd apps/web && npm run typecheck && npm run unit && npm run build && npx playwright test e2e/crash.spec.ts e2e/journal.spec.ts e2e/files.spec.ts e2e/fallback.spec.ts e2e/shared.spec.ts e2e/app.spec.ts`
Expected: PASS — `crash` 4 passed; остальные наборы — как раньше (стенд `debug.html` запускает ядро без тестового режима и перезапусков не делает).

- [ ] **Step 8: Закоммитить**

```bash
git add crates/ferrum-wasm apps/web/src apps/web/unit apps/web/e2e
git commit -m "feat(web,wasm): паника ядра перехватывается с текстом, редактор перезапускается с файлом и журналом (не чаще двух раз в минуту)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Потеря устройства GPU: пересоздание и запасной рендер

Спека 11: «Потеря GPU-устройства: пересоздание устройства и текстур, при неудаче — переход на запасной рендер». Этап 4 только сообщал о потере (`error`) и останавливал поверхность. Теперь потеря (устройство WebGPU, контекст WebGL2) даёт событие `surfaceLost`, и worker создаёт поверхность того же бэкенда заново на том же холсте (`WasmSurface.create` → `replaceSurface`): новое устройство WebGPU или восстановленный браузером контекст WebGL2, новый `GpuBackend` — атлас и растры фреймов с нуля (картинки грузятся в новый атлас при следующем рисовании, номера прежнего устройства забыты). Попытки — сразу и через 1 с (контекст WebGL2 браузер возвращает не сразу). Потерянный, ещё не восстановленный контекст WebGL2 проверяется до рендерера: `WebGlRenderer` vello на нём падает (`getParameter(MAX_TEXTURE_SIZE)` даёт `null`, а он делает `unwrap`). Удалось — `surfaceRestored` и уведомление «Видеокарта перезапущена». Не удалось (или поверхность теряется третий раз за жизнь ядра) — `surfaceFailed { next }`: запасной бэкенд WebGPU → WebGL2 → CPU. Контекст холста выбирается один раз (на холсте с `webgl2` не получить `2d`), поэтому запасной бэкенд — это перезапуск редактора (Task 6) с принудительным бэкендом `next`; перед ним правки уходят в журнал (ядро живо), уведомление — «Видеокарта недоступна — редактор перезапущен с рендером на процессоре» (или «…WebGL2»). Принудительный бэкенд сохраняется и для следующих перезапусков этого редактора.

Проверка — заглушками пролога worker'а из `fallback.spec.ts` (`loseGl`, новый `restoreGl`, `loseDevice`): в бандле тестовых хуков GPU нет.

**Files:**
- Create: `crates/ferrum-wasm/src/recover.rs`
- Modify: `crates/ferrum-wasm/src/lib.rs`
- Modify: `crates/ferrum-wasm/src/protocol.rs`
- Modify: `crates/ferrum-wasm/src/web/mod.rs`
- Modify: `crates/ferrum-wasm/src/web/surface.rs`
- Test: `crates/ferrum-wasm/tests/recover.rs`
- Modify: `crates/ferrum-wasm/tests/protocol.rs`
- Modify: `apps/web/src/worker/core.worker.ts`
- Modify: `apps/web/src/ui/app.tsx`
- Test: `apps/web/e2e/fallback.spec.ts`

**Interfaces:**
- Consumes: `Surface::create` этапа 4; перезапуск `AppProps.onRestart` (Task 6); `Saver.flushJournal` (Task 5).
- Produces: `recover::{next_backend(Backend) -> Backend, OnLost { Recreate, Fallback(Backend) }, Recovery { lost(&self, Backend) -> OnLost, restored(&mut self) }, MAX_RESTORES = 2}`; `CoreEvent::{SurfaceLost { backend, message }, SurfaceRestored { backend }, SurfaceFailed { next, message }}`; `Surface::recreate(backend, canvas, w, h, atlas_side)`; wasm: `class WasmSurface { static create(canvas, backend, width, height, atlas_side?): Promise<WasmSurface> }`, `WasmCore.deviceSize(): Uint16Array`, `WasmCore.replaceSurface(surface)`, `WasmCore.surfaceFailed(message)` (проверено сборкой прототипа).

- [ ] **Step 1: Написать падающие тесты политики**

`crates/ferrum-wasm/tests/recover.rs`:

````rust
//! Потеря GPU (этап 6): пересоздать на месте, пока потери не повторяются, затем —
//! запасной бэкенд по порядку WebGPU → WebGL2 → CPU.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_wasm::protocol::Backend;
use ferrum_wasm::recover::{next_backend, OnLost, Recovery, MAX_RESTORES};

#[test]
fn fallback_order() {
    assert_eq!(next_backend(Backend::Webgpu), Backend::Webgl);
    assert_eq!(next_backend(Backend::Webgl), Backend::Cpu);
    assert_eq!(next_backend(Backend::Cpu), Backend::Cpu);
}

#[test]
fn recreate_until_losses_repeat() {
    let mut r = Recovery::default();
    for _ in 0..MAX_RESTORES {
        assert_eq!(r.lost(Backend::Webgpu), OnLost::Recreate);
        r.restored();
    }
    assert_eq!(r.lost(Backend::Webgpu), OnLost::Fallback(Backend::Webgl));
    assert_eq!(r.lost(Backend::Webgl), OnLost::Fallback(Backend::Cpu));
}

#[test]
fn cpu_is_never_recreated() {
    assert_eq!(
        Recovery::default().lost(Backend::Cpu),
        OnLost::Fallback(Backend::Cpu)
    );
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        CoreEvent::Error {
            message: "ой".into(),
        },
````

на:

````rust
        CoreEvent::Error {
            message: "ой".into(),
        },
        CoreEvent::SurfaceLost {
            backend: Backend::Webgl,
            message: "Контекст WebGL2 потерян".into(),
        },
        CoreEvent::SurfaceRestored {
            backend: Backend::Webgpu,
        },
        CoreEvent::SurfaceFailed {
            next: Backend::Cpu,
            message: "не вышло".into(),
        },
````

Run: `cargo test -p ferrum-wasm --test recover --test protocol`
Expected: FAIL — нет модуля `recover` и событий `Surface*`.

- [ ] **Step 2: Политика и события**

`crates/ferrum-wasm/src/recover.rs`:

````rust
//! Потеря устройства GPU (этап 6, спека 11): что делать, когда поверхность WebGPU или
//! WebGL2 потеряна. Отдельно от браузерного кода, чтобы проверяться обычными тестами.
//!
//! Потерю сначала пробуют пережить на месте: worker создаёт поверхность того же
//! бэкенда заново на том же холсте (новое устройство WebGPU, восстановленный
//! контекст WebGL2). Не вышло — или поверхность теряется снова и снова (драйвер
//! падает на каждом кадре) — запасной бэкенд: WebGPU → WebGL2 → CPU. Контекст холста
//! выбирается один раз, поэтому запасной бэкенд — это новое ядро с новым холстом, и
//! перезапуск ведёт интерфейс.

use crate::protocol::Backend;

/// Сколько раз поверхность пересоздаётся на месте за жизнь ядра.
pub const MAX_RESTORES: u32 = 2;

/// Запасной бэкенд после `b`.
pub fn next_backend(b: Backend) -> Backend {
    match b {
        Backend::Webgpu => Backend::Webgl,
        Backend::Webgl | Backend::Cpu => Backend::Cpu,
    }
}

/// Что делать с потерянной поверхностью.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnLost {
    /// Создать поверхность того же бэкенда заново.
    Recreate,
    /// Перезапустить ядро с этим бэкендом.
    Fallback(Backend),
}

#[derive(Debug, Default)]
pub struct Recovery {
    restored: u32,
}

impl Recovery {
    pub fn lost(&self, backend: Backend) -> OnLost {
        if backend == Backend::Cpu || self.restored >= MAX_RESTORES {
            OnLost::Fallback(next_backend(backend))
        } else {
            OnLost::Recreate
        }
    }

    /// Поверхность создана заново.
    pub fn restored(&mut self) {
        self.restored += 1;
    }
}
````

В `crates/ferrum-wasm/src/lib.rs` заменить:

````rust
pub mod protocol;
````

на:

````rust
pub mod protocol;
pub mod recover;
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    /// Сообщение об ошибке для пользователя.
    Error {
        message: String,
    },
    // ——— Этап 5: панели ———
````

на:

````rust
    /// Сообщение об ошибке для пользователя.
    Error {
        message: String,
    },
    /// Поверхность GPU потеряна (устройство WebGPU, контекст WebGL2): worker создаёт
    /// её заново; до `SurfaceRestored` кадров нет.
    SurfaceLost {
        backend: Backend,
        message: String,
    },
    /// Поверхность создана заново, кадры идут.
    SurfaceRestored {
        backend: Backend,
    },
    /// Пересоздать не удалось (или потери повторяются): интерфейс перезапускает ядро
    /// с бэкендом `next`, документ — из файла и журнала.
    SurfaceFailed {
        next: Backend,
        message: String,
    },
    // ——— Этап 5: панели ———
````

Run: `cargo test -p ferrum-wasm --test recover --test protocol`
Expected: PASS — `recover` 3 passed, `protocol` 5 passed.

- [ ] **Step 3: Поверхность заново в worker'е**

В `crates/ferrum-wasm/src/web/surface.rs` заменить:

````rust
    pub fn kind(&self) -> Backend {
        match self {
````

на:

````rust
    /// Та же поверхность заново после потери (этап 6): у холста уже есть контекст
    /// этого бэкенда, `getContext` вернёт его же. Потерянный контекст WebGL2 до
    /// восстановления браузером не годится — рендерер vello читает его пределы и на
    /// потерянном контексте падает (`getParameter` даёт `null`), поэтому сначала
    /// проверка. Программный GPU годится: этот бэкенд уже выбран при запуске.
    pub async fn recreate(
        backend: Backend,
        canvas: &OffscreenCanvas,
        w: u16,
        h: u16,
        atlas_side: Option<u16>,
    ) -> Result<Surface, String> {
        if backend == Backend::Webgl {
            let gl: WebGl2RenderingContext = canvas
                .get_context("webgl2")
                .map_err(js_err)?
                .ok_or_else(|| "контекст WebGL2 недоступен".to_string())?
                .dyn_into()
                .map_err(|_| "у холста не WebGL2".to_string())?;
            if gl.is_context_lost() {
                return Err("контекст WebGL2 не восстановлен".into());
            }
        }
        Surface::create(backend, canvas, w, h, true, atlas_side).await
    }

    pub fn kind(&self) -> Backend {
        match self {
````

В `crates/ferrum-wasm/src/web/mod.rs` заменить:

````rust
use crate::protocol::{Backend, Command, CoreEvent};
````

на:

````rust
use crate::protocol::{Backend, Command, CoreEvent};
use crate::recover::{OnLost, Recovery};
````

В `crates/ferrum-wasm/src/web/mod.rs` заменить:

````rust
    /// Ошибки WebGPU вне вызовов: каждый текст — один раз, слишком много — стоп.
    soft: SoftErrors,
}
````

на:

````rust
    /// Ошибки WebGPU вне вызовов: каждый текст — один раз, слишком много — стоп.
    soft: SoftErrors,
    /// Потери устройства GPU: пересоздать поверхность или перейти на запасную.
    recovery: Recovery,
}

/// Поверхность, созданная заново после потери устройства или контекста GPU (этап 6):
/// создаётся асинхронно на том же холсте и отдаётся ядру `replaceSurface`.
#[wasm_bindgen]
pub struct WasmSurface {
    surface: Option<Surface>,
}

#[wasm_bindgen]
impl WasmSurface {
    /// Поверхность бэкенда `backend` заново на холсте `width × height` пикселей
    /// устройства (`Surface::recreate`).
    pub async fn create(
        canvas: OffscreenCanvas,
        backend: String,
        width: u16,
        height: u16,
        atlas_side: Option<u16>,
    ) -> Result<WasmSurface, JsError> {
        let b = Backend::parse(&backend).ok_or_else(|| JsError::new("неизвестный бэкенд"))?;
        let s = Surface::recreate(b, &canvas, width.max(1), height.max(1), atlas_side)
            .await
            .map_err(|e| JsError::new(&e))?;
        Ok(WasmSurface { surface: Some(s) })
    }
}
````

В `crates/ferrum-wasm/src/web/mod.rs` заменить:

````rust
            watch: PresentWatch::default(),
            soft: SoftErrors::default(),
        })
    }
````

на:

````rust
            watch: PresentWatch::default(),
            soft: SoftErrors::default(),
            recovery: Recovery::default(),
        })
    }

    /// Размер кадра в пикселях устройства, `[ширина, высота]` — для новой поверхности.
    #[wasm_bindgen(js_name = deviceSize)]
    pub fn device_size(&self) -> Vec<u16> {
        let (w, h) = self.core.device_size();
        vec![w, h]
    }

    /// Новая поверхность вместо потерянной: кадры идут снова, атлас и кэши GPU — с
    /// нуля (новый `GpuBackend`), счётчики ошибок показа сброшены.
    #[wasm_bindgen(js_name = replaceSurface)]
    pub fn replace_surface(&mut self, surface: &mut WasmSurface) -> Result<(), JsError> {
        let s = surface
            .surface
            .take()
            .ok_or_else(|| JsError::new("поверхность уже отдана ядру"))?;
        self.surface = s;
        self.dead = false;
        self.awaiting = false;
        self.atlas_reported = false;
        self.watch = PresentWatch::default();
        self.soft = SoftErrors::default();
        self.recovery.restored();
        let backend = self.surface.kind();
        self.core.set_backend(backend);
        self.core.push_event(CoreEvent::SurfaceRestored { backend });
        Ok(())
    }

    /// Пересоздать поверхность не удалось: интерфейс перезапустит ядро с запасным
    /// бэкендом (`SurfaceFailed { next }`).
    #[wasm_bindgen(js_name = surfaceFailed)]
    pub fn surface_failed(&mut self, message: String) {
        let next = crate::recover::next_backend(self.surface.kind());
        self.core
            .push_event(CoreEvent::SurfaceFailed { next, message });
    }
````

В `crates/ferrum-wasm/src/web/mod.rs` заменить:

````rust
        // Потерянный контекст WebGL2 остаётся потерянным: `take_errors` отдаёт его
        // каждый кадр, а сообщение нужно одно.
        if let Some(message) = fatal.filter(|_| !self.dead) {
            self.dead = true;
            self.core.push_event(CoreEvent::Error { message });
        }
````

на:

````rust
        // Потеря устройства или контекста: поверхность больше не рисует. Worker создаёт
        // её заново (`replaceSurface`), а не выйдет или потери повторяются — интерфейс
        // перезапускает ядро с запасным бэкендом. Событие одно: потерянный контекст
        // WebGL2 `take_errors` отдаёт каждый кадр.
        if let Some(message) = fatal.filter(|_| !self.dead) {
            self.dead = true;
            let backend = self.surface.kind();
            self.core.push_event(match self.recovery.lost(backend) {
                OnLost::Recreate => CoreEvent::SurfaceLost { backend, message },
                OnLost::Fallback(next) => CoreEvent::SurfaceFailed { next, message },
            });
        }
````

Run: `cargo fmt && cargo clippy --target wasm32-unknown-unknown -p ferrum-wasm --lib --tests -- -D warnings && cd apps/web && npm run wasm`
Expected: без предупреждений (проверено на прототипе); в `ferrum_wasm.d.ts` — `class WasmSurface`, `deviceSize(): Uint16Array`, `replaceSurface(surface: WasmSurface): void`, `surfaceFailed(message: string): void`.

- [ ] **Step 4: Пересоздание в worker'е и перезапуск в интерфейсе**

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
import init, { WasmCore } from '../wasm/ferrum_wasm.js';
````

на:

````ts
import init, { WasmCore, WasmSurface } from '../wasm/ferrum_wasm.js';
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
/** Тестовый режим страницы (`init.test`): принимаются тестовые сообщения. */
let testHooks = false;
````

на:

````ts
/** Тестовый режим страницы (`init.test`): принимаются тестовые сообщения. */
let testHooks = false;
/** Холст и атлас ядра — для новой поверхности после потери GPU. */
let surfaceCanvas: OffscreenCanvas | null = null;
let atlasSide: number | null = null;
/** Паузы перед попытками создать потерянную поверхность заново: контекст WebGL2
 * браузер возвращает не сразу. */
const SURFACE_RETRY_MS = [0, 1000];

/** Потерянная поверхность GPU — заново на том же холсте (спека 11): не вышло — ядро
 * сообщает `surfaceFailed`, и интерфейс перезапускает его с запасным бэкендом. */
async function recoverSurface(backend: string, lost: string): Promise<void> {
  let reason = '';
  for (const wait of SURFACE_RETRY_MS) {
    await new Promise((r) => setTimeout(r, wait));
    if (!core || !surfaceCanvas) return;
    try {
      const [w, h] = core.deviceSize();
      const surface = await WasmSurface.create(surfaceCanvas, backend, w, h, atlasSide);
      if (!core) return;
      core.replaceSurface(surface);
      surface.free();
      sendEvents();
      return;
    } catch (e) {
      reason = e instanceof Error ? e.message : String(e);
    }
  }
  if (!core) return;
  try {
    core.surfaceFailed(`${lost}; пересоздать не удалось: ${reason}`);
    sendEvents();
  } catch (e) {
    fatal(e);
  }
}
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  let events: CoreEvent[] = core.takeEvents();
  if (events.length === 0) return;
````

на:

````ts
  let events: CoreEvent[] = core.takeEvents();
  if (events.length === 0) return;
  for (const e of events) if (e.type === 'surfaceLost') void recoverSurface(e.backend, e.message);
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  testHooks = msg.test === true;
````

на:

````ts
  testHooks = msg.test === true;
  surfaceCanvas = msg.canvas;
  atlasSide = msg.atlas;
  // Потерянный контекст WebGL2 браузер возвращает, только если потерю «приняли»
  // (preventDefault в `webglcontextlost`); иначе пересоздать его нельзя никогда.
  msg.canvas.addEventListener('webglcontextlost', (e) => e.preventDefault());
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
            props.onRestart({ reason: 'crash', lost: saver?.pending() ?? false, message: e.message });
          }
````

на:

````ts
            props.onRestart({ reason: 'crash', lost: saver?.pending() ?? false, message: e.message });
          }
          if (e.type === 'surfaceRestored') store.toast('Видеокарта перезапущена');
          if (e.type === 'surfaceFailed') {
            // Поверхность GPU не пережила потерю: правки — в журнал (ядро живо), и тот же
            // файл — в новом редакторе с запасным бэкендом (контекст холста — один раз).
            const { next, message } = e;
            console.error(message);
            void (saver?.flushJournal() ?? Promise.resolve(true)).then(() => {
              if (!props.onRestart({ reason: 'gpu', lost: saver?.pending() ?? false, backend: next, message })) {
                store.toast(`Рендер остановлен: ${message}`, 'error');
              }
            });
          }
````

- [ ] **Step 5: E2E — заменить проверки «поверхность не рисует» на пересоздание**

В `apps/web/e2e/fallback.spec.ts` заменить:

````ts
  let realGl = null;
````

на:

````ts
  let realGl = null;
  // Расширение потери контекста: у потерянного контекста getExtension даёт null,
  // поэтому для возврата оно запоминается при потере (пролог — строка в обратных
  // кавычках: без них в комментариях).
  let loseExt = null;
````

В `apps/web/e2e/fallback.spec.ts` заменить:

````ts
    if (name === 'loseGl') realGl.getExtension('WEBGL_lose_context').loseContext();
````

на:

````ts
    if (name === 'loseGl') {
      loseExt = realGl.getExtension('WEBGL_lose_context');
      loseExt.loseContext();
    }
    // Браузер вернул контекст (worker «принял» потерю — preventDefault, иначе возврата нет).
    if (name === 'restoreGl') loseExt.restoreContext();
````

В `apps/web/e2e/fallback.spec.ts` заменить:

````ts
import { drag, expectPixel, gotoDebug, live, liveWhen, open, screen } from './helpers';
````

на:

````ts
import { drag, drawRect, editorReady, expectPixel, fileId, gotoDebug, live, liveWhen, open, screen, storedFile } from './helpers';
````

В `apps/web/e2e/fallback.spec.ts` заменить:

````ts
test('потеря устройства WebGPU: сообщение, поверхность не рисует @gpu', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=webgpu');
  expect((await info(page)).backend).toBe('webgpu');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'loseDevice');
  await poke(page);
  await expectDead(page, /^Устройство WebGPU потеряно \(.*\): /);
});

test('потеря контекста WebGL2: сообщение, поверхность не рисует @gpu', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=webgl');
  expect((await info(page)).backend).toBe('webgl');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'loseGl');
  await poke(page);
  await expectDead(page, /^Контекст WebGL2 потерян/);
});
````

на:

````ts
/** Первое событие вида `type` в журнале стенда (ждёт его). */
async function firstEvent(page: Page, type: string): Promise<Record<string, unknown>> {
  await page.waitForFunction((t) => window.__ferrum!.events.some((e) => e.type === t), type, { timeout: 15_000 });
  return page.evaluate((t) => window.__ferrum!.events.find((e) => e.type === t) as unknown as Record<string, unknown>, type);
}

test('потеря устройства WebGPU: устройство создано заново, рисует снова @gpu', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=webgpu');
  expect((await info(page)).backend).toBe('webgpu');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'loseDevice');
  const lost = await firstEvent(page, 'surfaceLost');
  expect(lost.backend).toBe('webgpu');
  expect(lost.message).toMatch(/^Устройство WebGPU потеряно \(.*\): /);
  expect(await firstEvent(page, 'surfaceRestored')).toEqual({ type: 'surfaceRestored', backend: 'webgpu' });
  await poke(page);
  await expectDrawn(page);
  expect(await errors(page)).toEqual([]);
});

test('потеря контекста WebGL2 и его возврат: поверхность создана заново, рисует снова', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=webgl');
  expect((await info(page)).backend).toBe('webgl');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'loseGl');
  expect((await firstEvent(page, 'surfaceLost')).message).toMatch(/^Контекст WebGL2 потерян/);
  await fault(page, 'restoreGl');
  expect(await firstEvent(page, 'surfaceRestored')).toEqual({ type: 'surfaceRestored', backend: 'webgl' });
  await poke(page);
  await expectDrawn(page);
  expect(await errors(page)).toEqual([]);
  expect(await page.evaluate(() => window.__ferrum!.events.some((e) => e.type === 'surfaceFailed'))).toBe(false);
});

test('контекст WebGL2 не вернулся: запасной бэкенд CPU', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=webgl');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'loseGl');
  const failed = await firstEvent(page, 'surfaceFailed');
  expect(failed.next).toBe('cpu');
  expect(failed.message).toMatch(/^Контекст WebGL2 потерян; пересоздать не удалось: контекст WebGL2 не восстановлен$/);
  expect(await errors(page)).toEqual([]);
});

test('редактор: WebGL2 потерян навсегда — перезапуск на процессоре, документ на месте', async ({ page }) => {
  await stubWorker(page, {});
  const id = await storedFile(page, '&backend=webgl&fullSave=600000');
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('webgl');
  await drawRect(page, 150);
  await page.evaluate(() => ((window.__ferrum!.core as unknown as { old?: boolean }).old = true));
  await fault(page, 'loseGl');
  await expect(
    page.locator('.fe-toast', { hasText: 'Видеокарта недоступна — редактор перезапущен с рендером на процессоре' }),
  ).toHaveCount(1, { timeout: 15_000 });
  await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
  await editorReady(page);
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('cpu');
  // Правка до потери ушла в журнал перед перезапуском.
  await liveWhen(page, 'l.nodes === 2');
  expect(fileId(page)).toBe(id);
});
````

Функция `expectDead` остаётся: её по-прежнему вызывает тест «ошибка показа кадра на CPU».

- [ ] **Step 6: Проверить**

Run: `cargo test -p ferrum-wasm && cargo clippy --all-targets -- -D warnings`
Expected: PASS.

Run: `cd apps/web && npm run typecheck && npm run build && npx playwright test e2e/fallback.spec.ts e2e/crash.spec.ts`
Expected: PASS — в проекте `chromium` три новых теста WebGL2 (SwiftShader по `backend=webgl`), в проекте `gpu` — WebGPU.

- [ ] **Step 7: Закоммитить**

```bash
git add crates/ferrum-wasm apps/web/src apps/web/e2e
git commit -m "feat(wasm,web): потеря GPU — поверхность заново на том же холсте, иначе перезапуск с запасным бэкендом

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Раскладка: платформа в файле экспорта

Перенос со этапа 5: сочетания раскладки хранятся с настоящими модификаторами (`Meta+KeyK` на macOS, `Ctrl+KeyK` на остальных), поэтому файл, экспортированный на macOS и импортированный на Windows, назначал бы клавишу Windows. Экспорт пишет поле `"platform": "mac" | "windows" | "linux"` (версия формата остаётся 1: поле необязательное, старые файлы читаются как раньше). Импорт файла другой группы платформ (macOS против Windows/Linux) меняет местами Ctrl и Cmd в каждом сочетании и добавляет одно предупреждение «Раскладка с macOS: Cmd заменён на Ctrl» или «Раскладка с Windows/Linux: Ctrl заменён на Cmd». Неизвестная платформа в файле — предупреждение «Неизвестная платформа раскладки — сочетания как в файле».

**Files:**
- Modify: `crates/ferrum-editor/src/keymap.rs`
- Test: `crates/ferrum-editor/tests/keymap.rs`

**Interfaces:**
- Consumes: `Keymap::{export_json, import_json, platform}` этапа 3.
- Produces: поле `platform` в JSON экспорта; перенос Ctrl ↔ Meta при импорте с другой группы платформ; текст `KeymapImported.warnings`.

- [ ] **Step 1: Написать падающие тесты**

В конец `crates/ferrum-editor/tests/keymap.rs` добавить:

````rust

/// Платформа в файле экспорта (этап 6): файл с macOS на Windows меняет Cmd на Ctrl,
/// обратно — Ctrl на Cmd; та же группа платформ и старый файл без поля — как есть.
#[test]
fn import_from_another_platform_swaps_ctrl_and_cmd() {
    let mut mac = Keymap::new(Platform::Mac);
    mac.import_json(r#"{"format":"ferrum-keymap","version":1,"overrides":{"tool.rect":["Meta+KeyK","Ctrl+KeyJ"]}}"#)
        .unwrap();
    let exported = mac.export_json();
    let v: serde_json::Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(v["platform"], "mac");

    let mut win = Keymap::new(Platform::Windows);
    let warnings = win.import_json(&exported).unwrap();
    assert_eq!(warnings, vec!["Раскладка с macOS: Cmd заменён на Ctrl".to_string()]);
    let back: serde_json::Value = serde_json::from_str(&win.export_json()).unwrap();
    assert_eq!(back["platform"], "windows");
    assert_eq!(back["overrides"]["tool.rect"], serde_json::json!(["Ctrl+KeyK", "Meta+KeyJ"]));

    // Linux — та же группа, что Windows: без замены и без предупреждения.
    let mut linux = Keymap::new(Platform::Linux);
    assert!(linux.import_json(&win.export_json()).unwrap().is_empty());
    // Обратно на macOS: Ctrl → Cmd.
    let mut mac2 = Keymap::new(Platform::Mac);
    assert_eq!(
        mac2.import_json(&win.export_json()).unwrap(),
        vec!["Раскладка с Windows/Linux: Ctrl заменён на Cmd".to_string()]
    );
    let again: serde_json::Value = serde_json::from_str(&mac2.export_json()).unwrap();
    assert_eq!(again["overrides"]["tool.rect"], serde_json::json!(["Meta+KeyK", "Ctrl+KeyJ"]));
}

#[test]
fn import_without_or_with_unknown_platform() {
    let old = r#"{"format":"ferrum-keymap","version":1,"overrides":{"tool.rect":["Meta+KeyK"]}}"#;
    let mut win = Keymap::new(Platform::Windows);
    assert!(win.import_json(old).unwrap().is_empty(), "старый файл — как есть");
    let v: serde_json::Value = serde_json::from_str(&win.export_json()).unwrap();
    assert_eq!(v["overrides"]["tool.rect"], serde_json::json!(["Meta+KeyK"]));
    let odd = r#"{"format":"ferrum-keymap","version":1,"platform":"amiga","overrides":{}}"#;
    assert_eq!(
        win.import_json(odd).unwrap(),
        vec!["Неизвестная платформа раскладки — сочетания как в файле".to_string()]
    );
}
````

Run: `cargo test -p ferrum-editor --test keymap`
Expected: FAIL — в экспорте нет `platform`, импорт сочетаний не меняет.

- [ ] **Step 2: Платформа в экспорте и импорте**

В `crates/ferrum-editor/src/keymap.rs` заменить:

````rust
        let doc = json!({
            "format": FORMAT,
            "version": VERSION,
            "overrides": overrides,
        });
````

на:

````rust
        let doc = json!({
            "format": FORMAT,
            "version": VERSION,
            "platform": platform_key(self.platform),
            "overrides": overrides,
        });
````

В `crates/ferrum-editor/src/keymap.rs` заменить:

````rust
        let mut warnings = Vec::new();
        let mut parsed = BTreeMap::new();
        for (key, value) in overrides {
````

на:

````rust
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
````

В `crates/ferrum-editor/src/keymap.rs` заменить:

````rust
                match item.as_str().map(Shortcut::parse) {
                    Some(Ok(s)) => {
                        if !list.contains(&s) {
````

на:

````rust
                match item.as_str().map(Shortcut::parse) {
                    Some(Ok(mut s)) => {
                        if swap {
                            std::mem::swap(&mut s.mods.ctrl, &mut s.mods.meta);
                        }
                        if !list.contains(&s) {
````

В конец `crates/ferrum-editor/src/keymap.rs` добавить:

````rust

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
````

- [ ] **Step 3: Проверить**

Run: `cargo fmt && cargo test -p ferrum-editor && cargo clippy -p ferrum-editor --all-targets -- -D warnings`
Expected: PASS — `keymap` 14 passed (прежние 12 и два новых; проверено на прототипе). Прежние проверки экспорта в E2E (`settings.spec.ts`) сверяют файл через `toMatchObject` — новое поле им не мешает.

- [ ] **Step 4: Закоммитить**

```bash
git add crates/ferrum-editor
git commit -m "feat(editor): платформа в файле раскладки — импорт с другой платформы меняет Ctrl и Cmd с предупреждением

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Настройки в IndexedDB: по полю, без затирания между вкладками

Перенос со этапа 5: настройки лежали одной записью `localStorage` (`ferrum.settings.v1`), и вкладка, записывая свои поля, затирала чужие (тема, выбранная в одной вкладке, пропадала после правки ширины панели в другой). Спека 7.6: настройки — в IndexedDB. База `ferrum` — версия 3: хранилище `settings`, ключ — имя поля, значение — само поле; запись — только изменённые поля, чтение при запуске — все. Изменения уходят другим вкладкам через `BroadcastChannel('ferrum-settings')` и применяются на лету: тема, привязка к пикселям, раскладка (ширина панелей — у каждой вкладки своя, только запоминается). Значения из базы не доверенные: неверное поле — умолчание (при чтении) или пропуск (в чужом изменении). При первом запуске после этапа 6 база пуста — настройки переносятся из `ferrum.settings.v1`, запись удаляется. Тему до первого рисования ставит `public/theme-init.js`, у которого нет доступа к IndexedDB: тема зеркалится в `localStorage` ключом `ferrum.theme` (одно значение — затирать нечего). Ошибка чтения или записи — одно уведомление «Не удалось сохранить настройки: …» / «Не удалось прочитать настройки: …», пока запись снова не удастся.

**Files:**
- Modify: `apps/web/src/core/file-list.ts`
- Modify: `apps/web/src/worker/io.worker.ts`
- Modify: `apps/web/src/ui/file-store.ts`
- Modify: `apps/web/src/ui/settings.ts`
- Create: `apps/web/src/ui/settings-store.ts`
- Modify: `apps/web/src/ui/root.tsx`, `app.tsx`, `files-screen.tsx`
- Modify: `apps/web/public/theme-init.js`
- Modify: `apps/web/src/window.d.ts`
- Test: `apps/web/unit/settings.test.ts`
- Test: `apps/web/e2e/settings-store.spec.ts`
- Modify: `apps/web/e2e/settings.spec.ts`, `apps/web/e2e/files.spec.ts` (чтение и запись настроек в тестах — через хранилище)

**Interfaces:**
- Consumes: `IdbFileStore` (worker ввода-вывода), `loadSettings` этапа 5 (для переноса).
- Produces: `SETTINGS_FIELDS` (`core/file-list.ts`); запросы `settings`, `setSettings { patch }`, событие worker'а `{ t: 'settings', patch }`; `parseSettingsPatch(o): Partial<Settings>`, `parseSettings(o): Settings`, `diffSettings(base, patch): Partial<Settings>`, `THEME_KEY = 'ferrum.theme'` (`ui/settings.ts`); `SettingsBackend { settings(); setSettings(patch); onSettings(listener) }`, `SettingsStore { current; update(patch); onChange(listener); onError(listener); dispose() }`, `openSettings(backend, legacy: Storage | null): Promise<SettingsStore>` (`ui/settings-store.ts`); `FilesScreenProps.settings`, `AppProps.settings`; хуки `FileStoreTestHooks.{settings, setSettings}`.

- [ ] **Step 1: Написать падающие модульные тесты**

В `apps/web/unit/settings.test.ts` заменить:

````ts
import {
  DEFAULT_SETTINGS,
  loadSettings,
````

на:

````ts
import { openSettings, type SettingsBackend } from '../src/ui/settings-store';
import {
  DEFAULT_SETTINGS,
  diffSettings,
  loadSettings,
  parseSettings,
  parseSettingsPatch,
  THEME_KEY,
````

В конец `apps/web/unit/settings.test.ts` добавить:

````ts

/** Хранилище настроек в памяти: запись, чужие изменения, отказ. */
function fakeBackend(initial: Record<string, unknown> = {}) {
  const data: Record<string, unknown> = { ...initial };
  const listeners = new Set<(p: Record<string, unknown>) => void>();
  const writes: Record<string, unknown>[] = [];
  let fail = false;
  const backend: SettingsBackend = {
    settings: async () => ({ ...data }),
    setSettings: async (p) => {
      if (fail) throw new Error('нет места');
      writes.push({ ...p });
      Object.assign(data, p);
    },
    onSettings: (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
  };
  return {
    data,
    writes,
    backend,
    setFail: (v: boolean) => (fail = v),
    remote: (p: Record<string, unknown>) => {
      Object.assign(data, p);
      for (const l of listeners) l(p);
    },
  };
}
const tick = () => new Promise((r) => setTimeout(r, 0));

test('поля настроек: только верные, остальное — умолчания или пропуск', () => {
  expect(parseSettingsPatch({ theme: 'light', pixelSnap: 'да', leftWidth: 1000, чужое: 1 })).toEqual({ theme: 'light', leftWidth: 480 });
  expect(parseSettings({ theme: 'синяя', pagesOpen: false })).toEqual({ ...DEFAULT_SETTINGS, pagesOpen: false });
  expect(diffSettings(DEFAULT_SETTINGS, { theme: 'dark', pixelSnap: false, keymap: undefined })).toEqual({ pixelSnap: false });
});

test('перенос из localStorage: один раз, прежняя запись удаляется, тема зеркалится', async () => {
  const legacy = memory();
  legacy.setItem(SETTINGS_KEY, JSON.stringify({ theme: 'light', filesView: 'list', leftWidth: 300 }));
  const f = fakeBackend();
  const s = await openSettings(f.backend, legacy);
  expect(s.current).toMatchObject({ theme: 'light', filesView: 'list', leftWidth: 300 });
  expect(f.data).toMatchObject({ theme: 'light', filesView: 'list', leftWidth: 300, pixelSnap: true });
  expect(legacy.getItem(SETTINGS_KEY)).toBeNull();
  expect(legacy.getItem(THEME_KEY)).toBe('light');
  // В базе уже есть — прежняя запись (если вдруг снова появилась) не читается.
  legacy.setItem(SETTINGS_KEY, JSON.stringify({ theme: 'dark' }));
  expect((await openSettings(f.backend, legacy)).current.theme).toBe('light');
});

test('пишутся только изменённые поля; чужие применяются и обратно не пишутся', async () => {
  const f = fakeBackend({ theme: 'dark', filesView: 'grid' });
  const s = await openSettings(f.backend, memory());
  const seen: Partial<typeof DEFAULT_SETTINGS>[] = [];
  s.onChange((p) => seen.push(p));
  s.update({ ...s.current, leftWidth: 320 });
  await tick();
  expect(f.writes).toEqual([{ leftWidth: 320 }]);
  // Другая вкладка сменила вид и прислала мусор в теме: вид применяется, тема — нет.
  f.remote({ filesView: 'list', theme: 'синяя' });
  expect(seen).toEqual([{ filesView: 'list' }]);
  expect(s.current.filesView).toBe('list');
  // Своя запись всех полей после чужого изменения его не затирает.
  s.update({ ...s.current });
  await tick();
  expect(f.writes).toHaveLength(1);
  expect(f.data.filesView).toBe('list');
});

test('ошибка записи настроек — одно уведомление, пока запись не удастся', async () => {
  const f = fakeBackend();
  const s = await openSettings(f.backend, memory());
  const errors: string[] = [];
  s.onError((m) => errors.push(m));
  f.setFail(true);
  s.update({ theme: 'light' });
  s.update({ theme: 'system' });
  await tick();
  expect(errors).toEqual(['Не удалось сохранить настройки: нет места']);
  f.setFail(false);
  s.update({ theme: 'dark' });
  await tick();
  f.setFail(true);
  s.update({ theme: 'light' });
  await tick();
  expect(errors).toHaveLength(2);
});
````

Run: `cd apps/web && npm run unit -- -g "поля настроек|перенос из localStorage|только изменённые|ошибка записи настроек"`
Expected: FAIL — нет `settings-store.ts`, `parseSettingsPatch`, `diffSettings`, `THEME_KEY`.

- [ ] **Step 2: Поля настроек и разбор**

В `apps/web/src/ui/settings.ts` заменить:

````ts
// Настройки пользователя: тема, привязка к пикселям, отличия раскладки клавиш,
// ширина панелей, вид экрана файлов (сетка или список). Хранятся в localStorage (полноценное хранилище — этап 6);
// доступ к нему может бросить (закрытые cookie, приватный режим) или вернуть
// мусор — тогда настройки по умолчанию, а запись тихо не удаётся.
````

на:

````ts
// Настройки пользователя: тема, привязка к пикселям, отличия раскладки клавиш,
// ширина панелей, вид экрана файлов (сетка или список). С этапа 6 хранятся в
// IndexedDB по полю (`settings-store.ts`); здесь — поля, их разбор (значения из
// хранилища не доверенные: неверное поле — умолчание) и прежняя запись
// `localStorage`, из которой настройки переносятся при первом запуске.
````

В `apps/web/src/ui/settings.ts` заменить:

````ts
export const SETTINGS_KEY = 'ferrum.settings.v1';
````

на:

````ts
/** Прежняя запись настроек в `localStorage` (до этапа 6): переносится и удаляется. */
export const SETTINGS_KEY = 'ferrum.settings.v1';
/** Зеркало темы в `localStorage` для `theme-init.js` (тема до первого рисования). */
export const THEME_KEY = 'ferrum.theme';
````

В `apps/web/src/ui/settings.ts` заменить:

````ts
/** Читает настройки; неверные поля заменяются умолчаниями поодиночке. */
export function loadSettings(store: Pick<Storage, 'getItem'> | null): Settings {
  let raw: unknown = null;
  try {
    const text = store?.getItem(SETTINGS_KEY);
    if (text && text.length <= MAX_SETTINGS_BYTES) raw = JSON.parse(text);
  } catch {
    raw = null;
  }
  const o = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  return {
    theme: o.theme === 'light' || o.theme === 'system' || o.theme === 'dark' ? o.theme : DEFAULT_SETTINGS.theme,
    pixelSnap: typeof o.pixelSnap === 'boolean' ? o.pixelSnap : DEFAULT_SETTINGS.pixelSnap,
    keymap: typeof o.keymap === 'string' && o.keymap.length <= MAX_KEYMAP_BYTES ? o.keymap : DEFAULT_SETTINGS.keymap,
    // Ширина из хранилища — в пределах края панели (как при перетаскивании).
    leftWidth: isNumber(o.leftWidth) ? clampPanel(o.leftWidth) : DEFAULT_SETTINGS.leftWidth,
    rightWidth: isNumber(o.rightWidth) ? clampPanel(o.rightWidth) : DEFAULT_SETTINGS.rightWidth,
    pagesOpen: typeof o.pagesOpen === 'boolean' ? o.pagesOpen : DEFAULT_SETTINGS.pagesOpen,
    filesView: o.filesView === 'list' || o.filesView === 'grid' ? o.filesView : DEFAULT_SETTINGS.filesView,
  };
}
````

на:

````ts
/** Проверка каждого поля: верное значение или `undefined`. */
const FIELDS: { [K in keyof Settings]: (v: unknown) => Settings[K] | undefined } = {
  theme: (v) => (v === 'light' || v === 'system' || v === 'dark' ? v : undefined),
  pixelSnap: (v) => (typeof v === 'boolean' ? v : undefined),
  keymap: (v) => (typeof v === 'string' && v.length <= MAX_KEYMAP_BYTES ? v : undefined),
  // Ширина из хранилища — в пределах края панели (как при перетаскивании).
  leftWidth: (v) => (isNumber(v) ? clampPanel(v) : undefined),
  rightWidth: (v) => (isNumber(v) ? clampPanel(v) : undefined),
  pagesOpen: (v) => (typeof v === 'boolean' ? v : undefined),
  filesView: (v) => (v === 'list' || v === 'grid' ? v : undefined),
};

/** Только верные известные поля (чужое изменение из другой вкладки, запись базы). */
export function parseSettingsPatch(o: Record<string, unknown>): Partial<Settings> {
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(FIELDS) as (keyof Settings)[]) {
    if (!(k in o)) continue;
    const v = FIELDS[k](o[k]);
    if (v !== undefined) out[k] = v;
  }
  return out as Partial<Settings>;
}

/** Все поля: неверные и отсутствующие — умолчания поодиночке. */
export function parseSettings(o: Record<string, unknown>): Settings {
  return { ...DEFAULT_SETTINGS, ...parseSettingsPatch(o) };
}

/** Поля `patch`, отличные от `base` (`undefined` — не трогать). */
export function diffSettings(base: Settings, patch: Partial<Settings>): Partial<Settings> {
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(patch) as (keyof Settings)[]) {
    if (patch[k] !== undefined && patch[k] !== base[k]) out[k] = patch[k];
  }
  return out as Partial<Settings>;
}

/** Читает прежнюю запись `localStorage`; неверные поля заменяются умолчаниями поодиночке. */
export function loadSettings(store: Pick<Storage, 'getItem'> | null): Settings {
  let raw: unknown = null;
  try {
    const text = store?.getItem(SETTINGS_KEY);
    if (text && text.length <= MAX_SETTINGS_BYTES) raw = JSON.parse(text);
  } catch {
    raw = null;
  }
  return parseSettings((typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>);
}
````

`apps/web/src/ui/settings-store.ts`:

````ts
// Настройки пользователя в IndexedDB (этап 6, спека 7.6): по ключу на поле в хранилище
// `settings` worker'а ввода-вывода (в Tauri — файл конфигурации, `tauri.ts`). Вкладка
// пишет только изменённые поля, поэтому две вкладки не затирают настройки друг друга
// (так было с одной записью в `localStorage`); чужие изменения приходят событием и
// применяются на лету. При первом запуске настройки переносятся из прежней записи
// `localStorage` (`ferrum.settings.v1`), и она удаляется. Тему до первого рисования
// читает `theme-init.js` — для него тема зеркалится в `localStorage` (`ferrum.theme`).
// Ошибка чтения или записи — одно уведомление, пока запись снова не удастся.

import { loadSettings, parseSettings, parseSettingsPatch, diffSettings, SETTINGS_KEY, THEME_KEY, type Settings } from './settings';

/** Где лежат поля: worker ввода-вывода (`IdbFileStore`) или Tauri. */
export interface SettingsBackend {
  /** Все поля (не доверенные). */
  settings(): Promise<Record<string, unknown>>;
  /** Записать только эти поля. */
  setSettings(patch: Partial<Settings>): Promise<void>;
  /** Поля, записанные другой вкладкой или окном; возвращает отписку. */
  onSettings(listener: (patch: Record<string, unknown>) => void): () => void;
}

export interface SettingsStore {
  /** Нынешние настройки: прочитанные при запуске, свои и чужие изменения. */
  readonly current: Settings;
  /** Записать изменённые поля (те же значения — без записи). */
  update(patch: Partial<Settings>): void;
  /** Поля, изменённые в другой вкладке; возвращает отписку. */
  onChange(listener: (patch: Partial<Settings>) => void): () => void;
  /** Ошибки чтения и записи (каждая — один раз); ошибка до подписки отдаётся первому. */
  onError(listener: (message: string) => void): () => void;
  dispose(): void;
}

const text = (e: unknown) => (e instanceof Error ? e.message : String(e));

function hasLegacy(store: Storage | null): boolean {
  try {
    return !!store?.getItem(SETTINGS_KEY);
  } catch {
    return false;
  }
}

function mirrorTheme(store: Storage | null, theme: Settings['theme']): void {
  try {
    store?.setItem(THEME_KEY, theme);
  } catch {
    // Закрытое хранилище: тема до рисования — тёмная, после запуска — верная.
  }
}

export async function openSettings(backend: SettingsBackend, legacy: Storage | null): Promise<SettingsStore> {
  const errors = new Set<(m: string) => void>();
  let pendingError: string | null = null;
  let reported: string | null = null;
  const report = (m: string) => {
    if (reported === m) return;
    reported = m;
    if (errors.size === 0) pendingError = m;
    for (const l of errors) l(m);
  };

  let raw: Record<string, unknown> = {};
  try {
    raw = await backend.settings();
  } catch (e) {
    report(`Не удалось прочитать настройки: ${text(e)}`);
  }
  let current: Settings;
  if (Object.keys(raw).length === 0 && hasLegacy(legacy)) {
    // Первый запуск после этапа 6: перенести прежнюю запись и удалить её.
    current = loadSettings(legacy);
    try {
      await backend.setSettings({ ...current });
      legacy?.removeItem(SETTINGS_KEY);
    } catch (e) {
      report(`Не удалось сохранить настройки: ${text(e)}`);
    }
  } else current = parseSettings(raw);
  mirrorTheme(legacy, current.theme);

  const listeners = new Set<(p: Partial<Settings>) => void>();
  const off = backend.onSettings((p) => {
    // Запись другой вкладки — не доверенная: только верные поля и только изменения.
    const changed = diffSettings(current, parseSettingsPatch(p));
    if (Object.keys(changed).length === 0) return;
    current = { ...current, ...changed };
    if (changed.theme) mirrorTheme(legacy, changed.theme);
    for (const l of listeners) l(changed);
  });

  return {
    get current() {
      return current;
    },
    update(patch) {
      const changed = diffSettings(current, patch);
      if (Object.keys(changed).length === 0) return;
      current = { ...current, ...changed };
      if (changed.theme) mirrorTheme(legacy, changed.theme);
      backend.setSettings(changed).then(
        () => (reported = null),
        (e: unknown) => report(`Не удалось сохранить настройки: ${text(e)}`),
      );
    },
    onChange(l) {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    onError(l) {
      errors.add(l);
      if (pendingError) {
        l(pendingError);
        pendingError = null;
      }
      return () => errors.delete(l);
    },
    dispose() {
      off();
      listeners.clear();
      errors.clear();
    },
  };
}
````

Run: `cd apps/web && npm run unit -- -g "поля настроек|перенос из localStorage|только изменённые|ошибка записи настроек|настройки"`
Expected: PASS — новые четыре и прежние тесты настроек (`loadSettings`, `updateSettings`, `saveSettings` для прежней записи остаются).

- [ ] **Step 3: Хранилище настроек в worker'е ввода-вывода**

В `apps/web/src/core/file-list.ts` заменить:

````ts
import { MAX_NAME_CHARS, MAX_OPEN_BYTES, tooLarge } from './limits';
````

на:

````ts
import { MAX_NAME_CHARS, MAX_OPEN_BYTES, MAX_SETTINGS_BYTES, tooLarge } from './limits';

/** Поля настроек пользователя в хранилище `settings` — по ключу на поле: вкладки
 * пишут только свои изменения и не затирают чужие. Значения проверяет интерфейс. */
export const SETTINGS_FIELDS = ['theme', 'pixelSnap', 'keymap', 'leftWidth', 'rightWidth', 'pagesOpen', 'filesView'] as const;
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
  /** Тесты: пачек журнала у файла. */
  | { op: 'journalCount'; file: string }
````

на:

````ts
  /** Тесты: пачек журнала у файла. */
  | { op: 'journalCount'; file: string }
  /** Все поля настроек (не доверенные). */
  | { op: 'settings' }
  /** Записать только эти поля настроек; другие вкладки получат их событием. */
  | { op: 'setSettings'; patch: Record<string, unknown> }
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
  appendJournal: null;
  journalCount: number;
````

на:

````ts
  appendJournal: null;
  journalCount: number;
  settings: Record<string, unknown>;
  setSettings: null;
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
  /** Список изменился (здесь или в другой вкладке): экрану файлов — перечитать окно. */
  | { t: 'changed' };
````

на:

````ts
  /** Список изменился (здесь или в другой вкладке): экрану файлов — перечитать окно. */
  | { t: 'changed' }
  /** Другая вкладка записала поля настроек. */
  | { t: 'settings'; patch: Record<string, unknown> };
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
    case 'emptyTrash':
      return null;
````

на:

````ts
    case 'emptyTrash':
    case 'settings':
      return null;
    case 'setSettings':
      return settingsPatchError(m.patch);
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
/** Текст ошибки, если запрос к worker'у не годится; `null` — можно выполнять.
````

на:

````ts
/** Поля настроек к записи: непустой объект известных полей с простыми значениями,
 * весь — не больше предела записи настроек. */
function settingsPatchError(v: unknown): string | null {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) return BAD;
  const keys = Object.keys(v);
  if (keys.length === 0 || keys.some((k) => !(SETTINGS_FIELDS as readonly string[]).includes(k))) return BAD;
  if (Object.values(v).some((x) => !['string', 'number', 'boolean'].includes(typeof x))) return BAD;
  const size = utf8.encode(JSON.stringify(v)).length;
  return size > MAX_SETTINGS_BYTES ? tooLarge(MAX_SETTINGS_BYTES, size) : null;
}

/** Текст ошибки, если запрос к worker'у не годится; `null` — можно выполнять.
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
// База `ferrum`, версия 2:
````

на:

````ts
// База `ferrum`, версия 3:
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
//   пачки до номера своего снимка в той же транзакции.
````

на:

````ts
//   пачки до номера своего снимка в той же транзакции;
// - `settings` (с версии 3) — поле настроек пользователя → значение: вкладки пишут
//   только изменённые поля, другие узнают через `BroadcastChannel('ferrum-settings')`.
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
const DB_VERSION = 2;
const CHANNEL = 'ferrum-files';
````

на:

````ts
const DB_VERSION = 3;
const CHANNEL = 'ferrum-files';
const SETTINGS_CHANNEL = 'ferrum-settings';
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
const channel = typeof BroadcastChannel === 'function' ? new BroadcastChannel(CHANNEL) : null;
````

на:

````ts
const channel = typeof BroadcastChannel === 'function' ? new BroadcastChannel(CHANNEL) : null;
const settingsChannel = typeof BroadcastChannel === 'function' ? new BroadcastChannel(SETTINGS_CHANNEL) : null;
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
      if (!d.objectStoreNames.contains('journal')) d.createObjectStore('journal');
````

на:

````ts
      if (!d.objectStoreNames.contains('journal')) d.createObjectStore('journal');
      if (!d.objectStoreNames.contains('settings')) d.createObjectStore('settings');
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
    case 'thumb': {
      const m = metas.get(r.file);
````

на:

````ts
    case 'settings': {
      const d = await open();
      const st = d.transaction('settings').objectStore('settings');
      const [keys, values] = await Promise.all([request(st.getAllKeys()), request(st.getAll())]);
      const out: Record<string, unknown> = {};
      keys.forEach((k, i) => {
        if (typeof k === 'string') out[k] = values[i];
      });
      return { value: out };
    }
    case 'thumb': {
      const m = metas.get(r.file);
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
    case 'journalCount': {
````

на:

````ts
    case 'setSettings': {
      const d = await open();
      const tx = rw(d, ['settings']);
      for (const [k, v] of Object.entries(r.patch)) tx.objectStore('settings').put(v, k);
      await done(tx);
      settingsChannel?.postMessage({ patch: r.patch });
      return { value: null };
    }
    case 'journalCount': {
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
if (channel) {
  channel.onmessage = () => {
````

на:

````ts
if (settingsChannel) {
  // Поля настроек от другой вкладки — своему интерфейсу (он проверит значения).
  settingsChannel.onmessage = (e: MessageEvent<unknown>) => {
    const patch = (e.data as { patch?: unknown } | null)?.patch;
    if (typeof patch === 'object' && patch !== null && !Array.isArray(patch)) {
      post({ t: 'settings', patch: patch as Record<string, unknown> });
    }
  };
}

if (channel) {
  channel.onmessage = () => {
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
import { FERRUM_FILE, pickFile } from './files';
````

на:

````ts
import { FERRUM_FILE, pickFile } from './files';
import type { Settings } from './settings';
import type { SettingsBackend } from './settings-store';
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  /** Пачек журнала файла в базе. */
  journalCount(id: string): Promise<number>;
}
````

на:

````ts
  /** Пачек журнала файла в базе. */
  journalCount(id: string): Promise<number>;
  /** Поля настроек в базе и запись полей (как у `SettingsBackend`). */
  settings(): Promise<Record<string, unknown>>;
  setSettings(patch: Partial<Settings>): Promise<void>;
}
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
export class IdbFileStore implements FileStore, FileStoreTestHooks {
````

на:

````ts
export class IdbFileStore implements FileStore, FileStoreTestHooks, SettingsBackend {
  private readonly settingsListeners = new Set<(patch: Record<string, unknown>) => void>();
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
    if (m.t === 'changed') {
      for (const l of this.listeners) l();
      return;
    }
````

на:

````ts
    if (m.t === 'changed') {
      for (const l of this.listeners) l();
      return;
    }
    if (m.t === 'settings') {
      for (const l of this.settingsListeners) l(m.patch);
      return;
    }
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  onChange(listener: () => void) {
````

на:

````ts
  settings() {
    return this.call({ op: 'settings' });
  }
  async setSettings(patch: Partial<Settings>) {
    await this.call({ op: 'setSettings', patch });
  }
  onSettings(listener: (patch: Record<string, unknown>) => void) {
    this.settingsListeners.add(listener);
    return () => this.settingsListeners.delete(listener);
  }
  onChange(listener: () => void) {
````

- [ ] **Step 4: Интерфейс читает и пишет настройки через хранилище**

`apps/web/public/theme-init.js` (полностью):

````js
// Тема до первого рисования: зеркало темы `ferrum.theme`, которое пишет хранилище
// настроек (`settings-store.ts`; сами настройки — в IndexedDB, недоступном отсюда
// синхронно), а до первого запуска этапа 6 — прежняя запись `ferrum.settings.v1`.
// Ставит `data-theme` на <html> раньше, чем загрузится приложение. Отдельный файл, а
// не встроенный скрипт: не требует 'unsafe-inline' в CSP. Любой сбой (хранилище
// закрыто, мусор в записи) — тёмная, как по умолчанию.
(function () {
  var theme = 'dark';
  try {
    var choice = window.localStorage.getItem('ferrum.theme');
    if (choice === null) {
      var raw = window.localStorage.getItem('ferrum.settings.v1');
      choice = raw && raw.length <= 81920 ? JSON.parse(raw).theme : null;
    }
    if (choice === 'light' || choice === 'dark') theme = choice;
    else if (choice === 'system') theme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  } catch (e) {
    theme = 'dark';
  }
  document.documentElement.setAttribute('data-theme', theme);
})();
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
import { IdbFileStore, type FileMeta, type Section } from './file-store';
````

на:

````ts
import { IdbFileStore, type FileMeta, type Section } from './file-store';
import { storage } from './settings';
import { openSettings, type SettingsStore } from './settings-store';
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
  onCleanup(() => {
    window.removeEventListener('popstate', onPop);
    release();
    files.dispose();
  });
  void go(parseRoute(location.hash));
````

на:

````ts
  // Настройки — до первого экрана: тема, вид списка и раскладка нужны сразу.
  let settings: SettingsStore | null = null;
  void openSettings(files, storage()).then((s) => {
    settings = s;
    void go(parseRoute(location.hash));
  });
  onCleanup(() => {
    window.removeEventListener('popstate', onPop);
    release();
    settings?.dispose();
    files.dispose();
  });
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
        <FilesScreen
          files={files}
````

на:

````ts
        <FilesScreen
          files={files}
          settings={settings!}
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
          <App
            file={v.file}
            files={files}
````

на:

````ts
          <App
            file={v.file}
            files={files}
            settings={settings!}
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
import { loadSettings, resolveTheme, storage, updateSettings, type Settings, type ThemeChoice } from './settings';
````

на:

````ts
import { resolveTheme, type Settings, type ThemeChoice } from './settings';
import type { SettingsStore } from './settings-store';
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
export interface AppProps {
  file: FileMeta;
  files: FileStore;
````

на:

````ts
export interface AppProps {
  file: FileMeta;
  files: FileStore;
  /** Настройки пользователя (IndexedDB, общие для вкладок). */
  settings: SettingsStore;
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
  const saved = loadSettings(storage());
````

на:

````ts
  const saved = props.settings.current;
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
    // Поверх сохранённых: поля экрана файлов (вид списка) редактор не трогает.
    if (pending) updateSettings(storage(), pending);
    pending = null;
  };
````

на:

````ts
    // Только изменённые поля: чужие (вид списка, изменения другой вкладки) не затираются.
    if (pending) props.settings.update(pending);
    pending = null;
  };
  // Настройки, изменённые в другой вкладке: тема, привязка и раскладка — на лету
  // (ширина панелей у каждой вкладки своя).
  const offRemote = props.settings.onChange((p) => {
    if (p.theme) setThemeChoice(p.theme);
    if (p.pixelSnap !== undefined) setPixelSnap(p.pixelSnap);
    if (p.keymap !== undefined && p.keymap !== keymap()) {
      setKeymap(p.keymap);
      const ed = editor();
      if (ed) {
        store.quietKeymapImport();
        ed.send(p.keymap ? { type: 'importKeymap', json: p.keymap } : { type: 'resetKeymap', command: undefined });
      }
    }
  });
  const offErrors = props.settings.onError((m) => store.toast(m, 'error'));
  onCleanup(() => {
    offRemote();
    offErrors();
  });
````

Сигналы `keymap` и `editor`, которые читает обработчик, объявлены в `app.tsx` выше этого места.

В `apps/web/src/ui/files-screen.tsx` заменить:

````ts
import { loadSettings, resolveTheme, storage, updateSettings, type FilesView } from './settings';
````

на:

````ts
import { resolveTheme, type FilesView } from './settings';
import type { SettingsStore } from './settings-store';
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````ts
export interface FilesScreenProps {
  files: FileStore;
````

на:

````ts
export interface FilesScreenProps {
  files: FileStore;
  /** Настройки пользователя (вид списка, тема). */
  settings: SettingsStore;
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````ts
  const saved = loadSettings(storage());
  // Тему до первого рисования ставит theme-init.js, после редактора она уже на
  // <html>; здесь — только «как в системе», которая следует за системой на лету.
  const media = matchMedia('(prefers-color-scheme: dark)');
  const applyTheme = () => (document.documentElement.dataset.theme = resolveTheme(loadSettings(storage()).theme, media.matches));
  media.addEventListener('change', applyTheme);
  onCleanup(() => media.removeEventListener('change', applyTheme));
````

на:

````ts
  const saved = props.settings.current;
  // Тему до первого рисования ставит theme-init.js, после редактора она уже на
  // <html>; здесь — «как в системе» на лету и тема, сменённая в другой вкладке.
  const media = matchMedia('(prefers-color-scheme: dark)');
  const applyTheme = () => (document.documentElement.dataset.theme = resolveTheme(props.settings.current.theme, media.matches));
  applyTheme();
  media.addEventListener('change', applyTheme);
  onCleanup(() => media.removeEventListener('change', applyTheme));
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````ts
  const chooseView = (v: FilesView) => {
    setView(v);
    updateSettings(storage(), { filesView: v });
  };
````

на:

````ts
  const chooseView = (v: FilesView) => {
    setView(v);
    props.settings.update({ filesView: v });
  };
  const offRemote = props.settings.onChange((p) => {
    if (p.theme) applyTheme();
    if (p.filesView) setView(p.filesView);
  });
  const offErrors = props.settings.onError((m) => ui.toast(m, 'error'));
  onCleanup(() => {
    offRemote();
    offErrors();
  });
````

В `apps/web/src/window.d.ts` ничего менять не нужно: тип `__ferrumFiles.store` — `FileStore & FileStoreTestHooks`, а `settings`/`setSettings` добавлены в `FileStoreTestHooks`.

- [ ] **Step 5: E2E — две вкладки и перенос; прежние тесты читают настройки из хранилища**

`apps/web/e2e/settings-store.spec.ts`:

````ts
// Настройки в IndexedDB (этап 6): перенос из localStorage при первом запуске, две
// вкладки меняют разные поля и не затирают друг друга, тема сменяется в обеих на
// лету, тема до загрузки приложения — из зеркала `ferrum.theme`.

import { expect, test, type Page } from '@playwright/test';
import { editorReady, newFileUrl, testMode } from './helpers';

const stored = (page: Page) => page.evaluate(() => window.__ferrumFiles!.store.settings());

test('перенос из localStorage: один раз, прежняя запись удаляется, тема — в зеркале', async ({ page }) => {
  await testMode(page);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('seeded')) return;
    sessionStorage.setItem('seeded', '1');
    localStorage.setItem('ferrum.settings.v1', JSON.stringify({ theme: 'light', filesView: 'list', pixelSnap: false }));
  });
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByRole('radiogroup', { name: 'Вид' }).getByRole('radio', { name: 'Список' })).toHaveAttribute('aria-checked', 'true');
  expect(await stored(page)).toMatchObject({ theme: 'light', filesView: 'list', pixelSnap: false });
  expect(await page.evaluate(() => [localStorage.getItem('ferrum.settings.v1'), localStorage.getItem('ferrum.theme')])).toEqual([
    null,
    'light',
  ]);
  // Приложение не грузится — тему ставит только theme-init.js по зеркалу.
  await page.route('**/assets/main-*.js', (r) => r.abort());
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('две вкладки меняют разные поля — оба сохраняются; тема — на лету в обеих', async ({ context }) => {
  const a = await context.newPage();
  const b = await context.newPage();
  await testMode(a);
  await testMode(b);
  await a.goto(newFileUrl(10));
  await editorReady(a);
  await b.goto('/');
  await expect(b.getByRole('heading', { level: 1 })).toBeVisible();
  // A: тема «Светлая» в настройках редактора.
  await a.getByRole('button', { name: 'Меню файла' }).click();
  await a.getByRole('menuitem', { name: /Настройки/ }).click();
  const dialog = a.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
  await dialog.getByRole('radio', { name: 'Светлая' }).check();
  await expect(b.locator('html')).toHaveAttribute('data-theme', 'light');
  // B: вид списка. A снова меняет тему — вид из B не затёрт.
  await b.getByRole('radiogroup', { name: 'Вид' }).getByRole('radio', { name: 'Список' }).click();
  await expect.poll(() => stored(a)).toMatchObject({ theme: 'light', filesView: 'list' });
  await dialog.getByRole('radio', { name: 'Тёмная' }).check();
  await expect.poll(() => stored(a)).toMatchObject({ theme: 'dark', filesView: 'list' });
  await expect(b.locator('html')).toHaveAttribute('data-theme', 'dark');
  await b.reload();
  await expect(b.getByRole('radiogroup', { name: 'Вид' }).getByRole('radio', { name: 'Список' })).toHaveAttribute('aria-checked', 'true');
});
````

В `apps/web/e2e/settings.spec.ts` заменить:

````ts
    .poll(() => bad.evaluate(() => JSON.parse(localStorage.getItem('ferrum.settings.v1')!).keymap as string))
````

на:

````ts
    .poll(() => bad.evaluate(() => window.__ferrumFiles!.store.settings().then((s) => String(s.keymap ?? ''))))
````

В `apps/web/e2e/settings.spec.ts` заменить:

````ts
  const stored = () => page.evaluate(() => JSON.parse(localStorage.getItem('ferrum.settings.v1')!).keymap as string);
````

на:

````ts
  const stored = () => page.evaluate(() => window.__ferrumFiles!.store.settings().then((s) => String(s.keymap ?? '')));
````

В `apps/web/e2e/files.spec.ts` заменить:

````ts
  const stored = await page.evaluate(() => JSON.parse(localStorage.getItem('ferrum.settings.v1')!));
````

на:

````ts
  const stored = await page.evaluate(() => window.__ferrumFiles!.store.settings());
````

В `apps/web/e2e/files.spec.ts` заменить:

````ts
  await page.evaluate(() => localStorage.setItem('ferrum.settings.v1', JSON.stringify({ keymap: 'не раскладка' })));
````

на:

````ts
  await page.evaluate(() => window.__ferrumFiles!.store.setSettings({ keymap: 'не раскладка' }));
````

Прочие тесты, которые кладут прежнюю запись `ferrum.settings.v1` до загрузки (`addInitScript`), работают через перенос: контекст теста новый, база пуста.

- [ ] **Step 6: Проверить**

Run: `cd apps/web && npm run typecheck && npm run unit && npm run build && npx playwright test e2e/settings-store.spec.ts e2e/settings.spec.ts e2e/files.spec.ts e2e/app.spec.ts`
Expected: PASS — `settings-store` 2 passed, прежние наборы как раньше.

- [ ] **Step 7: Закоммитить**

```bash
git add apps/web/src apps/web/public apps/web/unit apps/web/e2e
git commit -m "feat(web): настройки в IndexedDB по полю — вкладки не затирают друг друга, изменения на лету, перенос из localStorage

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: File System Access: файлы на диске в Chrome и Edge

Спека 10: «File System Access API» как ещё одна реализация хранения. Файл на диске — запись хранилища, связанная с `FileSystemFileHandle`: метаданные, превью и журнал — в IndexedDB, как у других записей, байты — в файле на диске. Дескриптор хранится в IndexedDB (база `ferrum`, версия 4: хранилище `handles`: id → `{ handle, lastModified }`; дескрипторы сериализуются structured clone). Чтение (`handle.getFile()`) и запись (`createWritable` → `write` → `close`: браузер пишет во временный файл и подменяет им файл при `close` — запись атомарная) — в worker'е ввода-вывода; выбор файла (`showOpenFilePicker`, `showSaveFilePicker`) и запрос доступа (`requestPermission`) — в окне: им нужно действие пользователя.

- «Открыть с диска…» (меню файла и экран файлов) — новая запись, связанная с выбранным файлом (тот же файл ещё раз — та же запись, `isSameEntry`), и открыть её. Отличается от «Открыть…» / «Импорт…» (копия в хранилище) тем, что правки пишутся обратно в файл.
- «Сохранить на диск…» — сохранить и связать открытую запись с новым файлом: текущий файл записывается на диск, байты в IndexedDB удаляются, дальше автосохранение пишет в файл на диске. «Сохранить копию на диск…» (скачать копию) остаётся.
- Файл на диске изменён другой программой (его `lastModified` не тот, что при последнем чтении или записи): запись отказывает «Файл на диске изменён другой программой — сохраните копию на диск или откройте файл заново»; документ остаётся изменённым, журнал — в IndexedDB.
- Файла нет (перемещён, удалён): «Файл на диске не найден — он перемещён или удалён». Нет доступа: «Нет доступа к файлу на диске».
- После перезапуска браузера доступ к дескриптору снова нужно разрешить: открытие такой записи показывает вопрос «Разрешить доступ к файлу на диске?» («Ferrum откроет «проект.ferrum» и будет сохранять изменения в него», кнопки «Разрешить» / «Отмена»); щелчок «Разрешить» — действие пользователя для `requestPermission`. Отказ — экран файлов с «Нет доступа к файлу на диске — разрешите его или откройте файл заново».
- Карточка такой записи — «На диске · Изменён …». «Удалить навсегда» убирает запись, файл на диске не трогает (текст вопроса говорит об этом).
- Где API нет (Firefox, Safari), пунктов «Открыть с диска…» и «Сохранить на диск…» нет (проверка `showOpenFilePicker`, `showSaveFilePicker` и `FileSystemFileHandle.prototype.createWritable`).

**Files:**
- Modify: `apps/web/src/core/file-list.ts`
- Modify: `apps/web/src/worker/io.worker.ts`
- Create: `apps/web/src/ui/disk.ts`
- Modify: `apps/web/src/ui/file-store.ts`
- Modify: `apps/web/src/ui/root.tsx`
- Modify: `apps/web/src/ui/app.tsx`
- Modify: `apps/web/src/ui/files-screen.tsx`
- Test: `apps/web/unit/files.test.ts`
- Test: `apps/web/e2e/fsa.spec.ts`

**Interfaces:**
- Consumes: запись, чтение и журнал хранилища (Tasks 4–5), `ConfirmDialog`.
- Produces: `FileMeta.disk?: string` (имя файла на диске); тексты `DISK_CHANGED`, `DISK_MISSING`, `DISK_ACCESS`; запросы `createFromDisk { handle }`, `attachDisk { file, handle }`, `handle { file }`; `DiskFiles { open(): Promise<FileMeta | null>; saveAs(id, suggestedName): Promise<FileMeta | null>; access(id): Promise<Access>; requestAccess(id): Promise<boolean> }`, `Access = 'granted' | 'prompt' | 'denied'`, `fsaSupported(): boolean`, `createDiskFiles(io: DiskIo): DiskFiles` (`ui/disk.ts`); `FileStore.disk?: DiskFiles`.

- [ ] **Step 1: Написать падающие тесты**

В конец `apps/web/unit/files.test.ts` добавить:

````ts

test('диск: запросы с дескриптором — только с настоящим дескриптором', () => {
  // В Node нет FileSystemFileHandle: любой «дескриптор» — подделка.
  const fake = { kind: 'file', name: 'x.ferrum' };
  expect(requestError({ id: 1, op: 'createFromDisk', handle: fake })).toBe('Неверный запрос к хранилищу файлов');
  expect(requestError({ id: 2, op: 'attachDisk', file: 'f1', handle: fake })).toBe('Неверный запрос к хранилищу файлов');
  expect(requestError({ id: 3, op: 'handle', file: 'f1' })).toBeNull();
  expect(requestError({ id: 3, op: 'handle', file: '' })).toBe('Неверный запрос к хранилищу файлов');
});
````

`apps/web/e2e/fsa.spec.ts`:

````ts
// File System Access (этап 6): файл на диске — запись хранилища, связанная с ним.
// Системные диалоги заглушены файлами OPFS (`navigator.storage.getDirectory()`): у них
// тот же FileSystemFileHandle и запись `createWritable`, но без окна выбора.
// Проверяется: «Открыть с диска…» и правки в файл, «Сохранить на диск…», файл,
// изменённый другой программой, запрос доступа после перезапуска, нет API — нет пунктов.

import { expect, test, type Page } from '@playwright/test';
import { drawRect, editorReady, fileId, liveWhen, newFileUrl, storedFile, testMode } from './helpers';

/** Диалоги выбора — файлы OPFS: открыть — `window.__pick` (или «проект.ferrum»),
 * сохранить — предложенное имя. */
async function stubPickers(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    const root = () => navigator.storage.getDirectory();
    w.showOpenFilePicker = async () => [await (await root()).getFileHandle((w.__pick as string) ?? 'проект.ferrum')];
    w.showSaveFilePicker = async (o?: { suggestedName?: string }) =>
      (await root()).getFileHandle(o?.suggestedName ?? 'копия.ferrum', { create: true });
  });
}

const diskSize = (page: Page, name: string) =>
  page.evaluate(async (n) => (await (await (await navigator.storage.getDirectory()).getFileHandle(n)).getFile()).size, name);
const meta = (page: Page, id: string) => page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
const toast = (page: Page, text: string) => page.locator('.fe-toast', { hasText: text });
const fileMenu = async (page: Page, item: string) => {
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: item }).click();
};

/** Байты открытого документа — файлом `name` в OPFS («на диске»). */
async function writeDocToDisk(page: Page, name: string): Promise<void> {
  await page.evaluate(
    (n) =>
      new Promise<void>((resolve) => {
        const core = window.__ferrum!.core!;
        const off = core.on(async (e) => {
          if (e.type !== 'saved') return;
          off();
          const h = await (await navigator.storage.getDirectory()).getFileHandle(n, { create: true });
          const w = await h.createWritable();
          await w.write(e.bytes as BlobPart);
          await w.close();
          resolve();
        });
        core.send({ type: 'save' });
      }),
    name,
  );
}

/** Новое ядро после перехода к другому файлу. */
async function nextEditor(page: Page, go: () => Promise<void>): Promise<void> {
  await page.evaluate(() => ((window.__ferrum!.core as unknown as { old?: boolean }).old = true));
  await go();
  await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
  await editorReady(page);
}

test('«Открыть с диска…»: запись связана с файлом, правки пишутся в него', async ({ page }) => {
  await testMode(page);
  await stubPickers(page);
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await writeDocToDisk(page, 'проект.ferrum');
  await nextEditor(page, () => fileMenu(page, 'Открыть с диска…'));
  await liveWhen(page, 'l.nodes === 5');
  const id = fileId(page);
  expect((await meta(page, id))!.disk).toBe('проект.ferrum');
  await expect(page.locator('.fe-panel-head__name')).toHaveText('проект');
  const before = await diskSize(page, 'проект.ferrum');
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => diskSize(page, 'проект.ferrum')).not.toBe(before);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  // Экран файлов: «На диске»; тот же файл ещё раз — та же запись.
  await fileMenu(page, 'Все файлы');
  const card = page.getByRole('main').getByRole('listbox').getByRole('option', { name: 'проект', exact: true });
  await expect(card).toContainText('На диске · Изменён');
  await nextEditor(page, () => page.getByRole('button', { name: 'Открыть с диска…' }).click());
  expect(fileId(page)).toBe(id);
  await liveWhen(page, 'l.nodes === 6');
});

test('«Сохранить на диск…»: файл из хранилища переезжает на диск', async ({ page }) => {
  await stubPickers(page);
  const id = await storedFile(page, '&fullSave=600000');
  await fileMenu(page, 'Сохранить на диск…');
  await expect(toast(page, 'Файл сохранён на диск: Без названия.ferrum')).toHaveCount(1);
  expect((await meta(page, id))!.disk).toBe('Без названия.ferrum');
  const before = await diskSize(page, 'Без названия.ferrum');
  expect(before).toBeGreaterThan(0);
  await drawRect(page, 150);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => diskSize(page, 'Без названия.ferrum')).not.toBe(before);
  await page.reload();
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 2');
});

test('файл на диске изменён другой программой: запись отказывает, правка не теряется', async ({ page }) => {
  await testMode(page);
  await stubPickers(page);
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await writeDocToDisk(page, 'проект.ferrum');
  await nextEditor(page, () => fileMenu(page, 'Открыть с диска…'));
  await page.waitForTimeout(20);
  await page.evaluate(async () => {
    const h = await (await navigator.storage.getDirectory()).getFileHandle('проект.ferrum');
    const w = await h.createWritable();
    await w.write(new Uint8Array([1, 2, 3]));
    await w.close();
  });
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect(
    toast(page, 'Не удалось сохранить файл: Файл на диске изменён другой программой — сохраните копию на диск или откройте файл заново'),
  ).toHaveCount(1);
  expect(await page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  expect(await diskSize(page, 'проект.ferrum')).toBe(3);
});

test('доступ к файлу на диске — вопрос; «Разрешить» открывает, «Отмена» — экран файлов', async ({ page }) => {
  await testMode(page);
  await stubPickers(page);
  // Как после перезапуска браузера: доступа нет, пока его не разрешат.
  await page.addInitScript(() => {
    let granted = false;
    const proto = FileSystemHandle.prototype as unknown as Record<string, unknown>;
    proto.queryPermission = async () => (granted ? 'granted' : 'prompt');
    proto.requestPermission = async () => ((granted = true), 'granted');
  });
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await writeDocToDisk(page, 'проект.ferrum');
  await fileMenu(page, 'Открыть с диска…');
  const ask = page.getByRole('dialog', { name: 'Разрешить доступ к файлу на диске?' });
  await expect(ask).toContainText('Ferrum откроет «проект.ferrum» и будет сохранять изменения в него');
  await ask.getByRole('button', { name: 'Разрешить' }).click();
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 5');
  await page.reload();
  await page.getByRole('dialog', { name: 'Разрешить доступ к файлу на диске?' }).getByRole('button', { name: 'Отмена' }).click();
  await expect(toast(page, 'Нет доступа к файлу на диске — разрешите его или откройте файл заново')).toHaveCount(1);
  await expect(page).toHaveURL(/#\/$/);
});

test('без File System Access пунктов «с диска» и «на диск» нет', async ({ page }) => {
  await page.addInitScript(() => {
    delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker;
  });
  await testMode(page);
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Импорт…' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Открыть с диска…' })).toHaveCount(0);
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const menu = page.getByRole('menu', { name: 'Меню файла' });
  await expect(menu.getByRole('menuitem', { name: 'Открыть…' })).toBeVisible();
  await expect(menu.getByRole('menuitem', { name: 'Открыть с диска…' })).toHaveCount(0);
  await expect(menu.getByRole('menuitem', { name: 'Сохранить на диск…' })).toHaveCount(0);
});
````

Run: `cd apps/web && npm run unit -- -g "диск: запросы"`
Expected: FAIL — `requestError` не знает `createFromDisk`, `attachDisk`, `handle`.

- [ ] **Step 2: Записи на диске в хранилище**

В `apps/web/src/core/file-list.ts` заменить:

````ts
  /** Поколение содержимого: растёт с каждой записью байтов. Запись несёт поколение,
   * на котором основана, и не проходит, если файл с тех пор записали (другая вкладка). */
  version: number;
}
````

на:

````ts
  /** Поколение содержимого: растёт с каждой записью байтов. Запись несёт поколение,
   * на котором основана, и не проходит, если файл с тех пор записали (другая вкладка). */
  version: number;
  /** Запись связана с файлом на диске (File System Access): его имя. Байты — в нём,
   * а не в базе. */
  disk?: string;
}

/** Файл на диске записала другая программа после нашего чтения или записи. */
export const DISK_CHANGED = 'Файл на диске изменён другой программой — сохраните копию на диск или откройте файл заново';
/** Файла на диске нет: перемещён или удалён. */
export const DISK_MISSING = 'Файл на диске не найден — он перемещён или удалён';
/** Браузер не дал доступ к файлу на диске. */
export const DISK_ACCESS = 'Нет доступа к файлу на диске';
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
  /** Все поля настроек (не доверенные). */
  | { op: 'settings' }
````

на:

````ts
  /** Новая запись, связанная с файлом на диске (тот же файл — его прежняя запись). */
  | { op: 'createFromDisk'; handle: FileSystemFileHandle }
  /** Связать запись с файлом на диске: записать в него её байты, дальше — писать туда. */
  | { op: 'attachDisk'; file: string; handle: FileSystemFileHandle }
  /** Дескриптор файла на диске (запрос доступа — в окне); `null` — запись не на диске. */
  | { op: 'handle'; file: string }
  /** Все поля настроек (не доверенные). */
  | { op: 'settings' }
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
  settings: Record<string, unknown>;
  setSettings: null;
````

на:

````ts
  settings: Record<string, unknown>;
  setSettings: null;
  createFromDisk: FileMeta;
  attachDisk: FileMeta;
  handle: FileSystemFileHandle | null;
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
    case 'journalCount':
      return isId(m.file) ? null : BAD;
````

на:

````ts
    case 'journalCount':
    case 'handle':
      return isId(m.file) ? null : BAD;
    case 'createFromDisk':
      return isHandle(m.handle) ? null : BAD;
    case 'attachDisk':
      return isId(m.file) && isHandle(m.handle) ? null : BAD;
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
/** Целое ≥ 0 не больше `Number.MAX_SAFE_INTEGER` (сеанс, номер пачки). */
````

на:

````ts
/** Настоящий дескриптор файла (где File System Access нет — никакой не настоящий). */
function isHandle(v: unknown): v is FileSystemFileHandle {
  return typeof FileSystemFileHandle === 'function' && v instanceof FileSystemFileHandle;
}

/** Целое ≥ 0 не больше `Number.MAX_SAFE_INTEGER` (сеанс, номер пачки). */
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
import {
  CONFLICT,
  GONE,
````

на:

````ts
import {
  CONFLICT,
  DISK_ACCESS,
  DISK_CHANGED,
  DISK_MISSING,
  GONE,
  nameFromFile,
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
} from '../core/file-list';
````

на:

````ts
} from '../core/file-list';
import { MAX_OPEN_BYTES, tooLarge } from '../core/limits';
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
// База `ferrum`, версия 3:
````

на:

````ts
// База `ferrum`, версия 4:
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
//   только изменённые поля, другие узнают через `BroadcastChannel('ferrum-settings')`.
````

на:

````ts
//   только изменённые поля, другие узнают через `BroadcastChannel('ferrum-settings')`;
// - `handles` (с версии 4) — id → `{ handle, lastModified }` записи, связанной с
//   файлом на диске (File System Access): байты — в файле, не в `bytes`; запись — через
//   `createWritable` (браузер подменяет файл целиком при `close`), чужая правка файла
//   видна по `lastModified`.
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
const DB_VERSION = 3;
````

на:

````ts
const DB_VERSION = 4;
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
      if (!d.objectStoreNames.contains('settings')) d.createObjectStore('settings');
````

на:

````ts
      if (!d.objectStoreNames.contains('settings')) d.createObjectStore('settings');
      if (!d.objectStoreNames.contains('handles')) d.createObjectStore('handles');
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
  if (num(m.trashedAt)) meta.trashedAt = m.trashedAt as number;
  return meta;
````

на:

````ts
  if (num(m.trashedAt)) meta.trashedAt = m.trashedAt as number;
  if (typeof m.disk === 'string' && m.disk.length > 0 && m.disk.length <= 4096) meta.disk = m.disk;
  return meta;
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
/** Все пачки журнала файла: ключи `[id, сеанс, номер]`, массив больше любого числа. */
````

на:

````ts
interface DiskRecord {
  handle: FileSystemFileHandle;
  /** `lastModified` файла после нашего последнего чтения или записи. */
  lastModified: number;
}

/** Ошибка File System Access — словами для пользователя. */
function diskError(e: unknown): Error {
  const name = e instanceof DOMException ? e.name : '';
  if (name === 'NotFoundError') return new Error(DISK_MISSING);
  if (name === 'NotAllowedError' || name === 'SecurityError') return new Error(DISK_ACCESS);
  return storageError(e);
}

async function diskOf(id: string): Promise<DiskRecord | null> {
  const d = await open();
  const v = (await request(d.transaction('handles').objectStore('handles').get(id))) as unknown;
  const r = v as Partial<DiskRecord> | undefined;
  return r && r.handle instanceof FileSystemFileHandle && typeof r.lastModified === 'number'
    ? { handle: r.handle, lastModified: r.lastModified }
    : null;
}

async function fileOf(handle: FileSystemFileHandle): Promise<File> {
  try {
    return await handle.getFile();
  } catch (e) {
    throw diskError(e);
  }
}

/** Записать байты в файл на диске: браузер пишет во временный и подменяет им файл при `close`. */
async function writeDisk(handle: FileSystemFileHandle, bytes: ArrayBuffer): Promise<number> {
  let w: FileSystemWritableFileStream;
  try {
    w = await handle.createWritable();
  } catch (e) {
    throw diskError(e);
  }
  try {
    await w.write(bytes);
    await w.close();
  } catch (e) {
    await w.abort().catch(() => {});
    throw diskError(e);
  }
  return (await fileOf(handle)).lastModified;
}

/** Все пачки журнала файла: ключи `[id, сеанс, номер]`, массив больше любого числа. */
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
async function readFile(id: string): Promise<{ value: IoResults['read']; transfer: Transferable[] }> {
  const m = meta(id);
  const d = await open();
  const tx = d.transaction(['bytes', 'journal']);
  const [raw, entries] = await Promise.all([
    request(tx.objectStore('bytes').get(id)) as Promise<unknown>,
    request(tx.objectStore('journal').getAll(journalRange(id))) as Promise<unknown[]>,
  ]);
  const bytes = raw instanceof ArrayBuffer ? raw : null;
````

на:

````ts
async function readFile(id: string): Promise<{ value: IoResults['read']; transfer: Transferable[] }> {
  const m = meta(id);
  const d = await open();
  const tx = d.transaction(['bytes', 'journal']);
  const [raw, entries] = await Promise.all([
    request(tx.objectStore('bytes').get(id)) as Promise<unknown>,
    request(tx.objectStore('journal').getAll(journalRange(id))) as Promise<unknown[]>,
  ]);
  let bytes = raw instanceof ArrayBuffer ? raw : null;
  const disk = m.disk ? await diskOf(id) : null;
  if (disk) {
    // Байты — в файле на диске; пустой файл (только что выбранный для сохранения) — новый.
    const f = await fileOf(disk.handle);
    if (f.size > MAX_OPEN_BYTES) throw new Error(tooLarge(MAX_OPEN_BYTES, f.size));
    bytes = f.size > 0 ? await f.arrayBuffer() : null;
    if (f.lastModified !== disk.lastModified) {
      const w = rw(d, ['handles']);
      w.objectStore('handles').put({ handle: disk.handle, lastModified: f.lastModified }, id);
      await done(w);
    }
  }
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
    case 'blob': {
      meta(r.file);
      const buf = await readBytes(r.file);
````

на:

````ts
    case 'blob': {
      // Файл на диске — уже Blob (`File`): основной поток его не копирует.
      if (meta(r.file).disk) {
        const disk = await diskOf(r.file);
        if (disk) return { value: await fileOf(disk.handle) };
      }
      const buf = await readBytes(r.file);
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
    case 'settings': {
      const d = await open();
````

на:

````ts
    case 'handle':
      meta(r.file);
      return { value: (await diskOf(r.file))?.handle ?? null };
    case 'settings': {
      const d = await open();
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
      const { bytes, thumb, base, journal } = r;
      // Имя записи здесь не меняется: оно — из хранилища (переименование — `rename`,
      // в том числе из другой вкладки), снимок его не затирает.
      const next = await update(
        r.file,
        ['bytes', 'thumbs', 'journal'],
        (m, tx) => {
          if (m.version !== base) throw new Error(CONFLICT);
          tx.objectStore('bytes').put(bytes, m.id);
````

на:

````ts
      const { bytes, thumb, base, journal } = r;
      // Файл на диске: сначала он (атомарно), затем метаданные и журнал в базе. Сбой
      // между ними оставит пачки, уже вошедшие в файл, — их повтор безвреден.
      let disk: { handle: FileSystemFileHandle; lastModified: number } | null = null;
      const known = metas.get(r.file);
      if (known?.disk) {
        const rec = await diskOf(r.file);
        if (rec) {
          if (known.version !== base) throw new Error(CONFLICT);
          if ((await fileOf(rec.handle)).lastModified !== rec.lastModified) throw new Error(DISK_CHANGED);
          disk = { handle: rec.handle, lastModified: await writeDisk(rec.handle, bytes) };
        }
      }
      // Имя записи здесь не меняется: оно — из хранилища (переименование — `rename`,
      // в том числе из другой вкладки), снимок его не затирает.
      const next = await update(
        r.file,
        ['bytes', 'thumbs', 'journal', 'handles'],
        (m, tx) => {
          if (m.version !== base) throw new Error(CONFLICT);
          if (disk) tx.objectStore('handles').put(disk, m.id);
          else tx.objectStore('bytes').put(bytes, m.id);
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
    case 'appendJournal': {
````

на:

````ts
    case 'createFromDisk': {
      // Тот же файл уже в списке — его запись (без дубля).
      for (const m of metas.values()) {
        if (!m.disk) continue;
        const rec = await diskOf(m.id);
        if (rec && (await rec.handle.isSameEntry(r.handle))) return { value: m };
      }
      const f = await fileOf(r.handle);
      if (f.size > MAX_OPEN_BYTES) throw new Error(tooLarge(MAX_OPEN_BYTES, f.size));
      const m: FileMeta = {
        id: crypto.randomUUID(),
        name: nameFromFile(f.name),
        createdAt: now,
        editedAt: f.lastModified || now,
        openedAt: now,
        size: f.size,
        thumbVersion: 0,
        version: f.size > 0 ? 1 : 0,
        disk: f.name,
      };
      const d = await open();
      const tx = rw(d, ['meta', 'handles']);
      tx.objectStore('meta').put(m);
      tx.objectStore('handles').put({ handle: r.handle, lastModified: f.lastModified }, m.id);
      await done(tx);
      metas.set(m.id, m);
      changed();
      return { value: m };
    }
    case 'attachDisk': {
      meta(r.file);
      // Байты записи — в выбранный файл (их может не быть: новый файл, ещё не сохранённый).
      const bytes = await readBytes(r.file);
      const lastModified = bytes ? await writeDisk(r.handle, bytes) : (await fileOf(r.handle)).lastModified;
      // Поколение не меняется: содержимое то же, открытый редактор пишет дальше.
      const next = await update(r.file, ['bytes', 'handles'], (m, tx) => {
        tx.objectStore('handles').put({ handle: r.handle, lastModified }, m.id);
        tx.objectStore('bytes').delete(m.id);
        return { ...m, disk: r.handle.name };
      });
      changed();
      return { value: next };
    }
    case 'appendJournal': {
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
      const tx = rw(d, ['meta', 'bytes', 'thumbs', 'journal']);
      for (const id of ids) {
        for (const st of ['meta', 'bytes', 'thumbs']) tx.objectStore(st).delete(id);
````

на:

````ts
      const tx = rw(d, ['meta', 'bytes', 'thumbs', 'journal', 'handles']);
      for (const id of ids) {
        // Файл на диске не трогаем: удаляется только запись о нём.
        for (const st of ['meta', 'bytes', 'thumbs', 'handles']) tx.objectStore(st).delete(id);
````

- [ ] **Step 3: Выбор файла и доступ — в окне**

`apps/web/src/ui/disk.ts`:

````ts
// File System Access (Chrome, Edge; спека 10): записи хранилища, связанные с файлом на
// диске. Выбор файла и запрос доступа — здесь, в окне: диалогам и разрешениям нужно
// действие пользователя. Чтение и запись по дескриптору — в worker'е ввода-вывода.
// Где API нет (Firefox, Safari), `fsaSupported()` — `false`, и пунктов меню нет.

import type { FileMeta } from '../core/file-list';
import { FERRUM_FILE } from './files';

export type Access = 'granted' | 'prompt' | 'denied';

export interface DiskFiles {
  /** «Открыть с диска…»: выбрать файл и завести связанную с ним запись; `null` — отказ. */
  open(): Promise<FileMeta | null>;
  /** «Сохранить на диск…»: выбрать файл; дальше запись идёт в него. `null` — отказ. */
  saveAs(id: string, suggestedName: string): Promise<FileMeta | null>;
  /** Доступ к файлу записи (не на диске — `granted`). */
  access(id: string): Promise<Access>;
  /** Запросить доступ — только из обработчика действия пользователя. */
  requestAccess(id: string): Promise<boolean>;
}

/** Запросы к worker'у ввода-вывода, которые нужны диску (их даёт `IdbFileStore`). */
export interface DiskIo {
  createFromDisk(handle: FileSystemFileHandle): Promise<FileMeta>;
  attachDisk(id: string, handle: FileSystemFileHandle): Promise<FileMeta>;
  handle(id: string): Promise<FileSystemFileHandle | null>;
}

type Permission = { mode: 'readwrite' };
type Handle = FileSystemFileHandle & {
  queryPermission?(o: Permission): Promise<PermissionState>;
  requestPermission?(o: Permission): Promise<PermissionState>;
};
type Pickers = {
  showOpenFilePicker?(o: { types: unknown[] }): Promise<FileSystemFileHandle[]>;
  showSaveFilePicker?(o: { suggestedName: string; types: unknown[] }): Promise<FileSystemFileHandle>;
};

export function fsaSupported(): boolean {
  const w = window as unknown as Pickers;
  return (
    typeof w.showOpenFilePicker === 'function' &&
    typeof w.showSaveFilePicker === 'function' &&
    typeof FileSystemFileHandle === 'function' &&
    'createWritable' in FileSystemFileHandle.prototype
  );
}

const aborted = (e: unknown) => e instanceof DOMException && e.name === 'AbortError';

export function createDiskFiles(io: DiskIo): DiskFiles {
  const w = window as unknown as Required<Pickers>;
  const mode: Permission = { mode: 'readwrite' };
  /** Дескрипторы, полученные при проверке доступа: запрос по щелчку идёт сразу, без
   * обращения к worker'у (иначе действие пользователя могло бы истечь). */
  const known = new Map<string, Handle>();
  const handleOf = async (id: string) => known.get(id) ?? ((await io.handle(id)) as Handle | null);
  return {
    async open() {
      let handle: FileSystemFileHandle | undefined;
      try {
        [handle] = await w.showOpenFilePicker({ types: [FERRUM_FILE] });
      } catch (e) {
        if (aborted(e)) return null;
        throw e;
      }
      return handle ? io.createFromDisk(handle) : null;
    },
    async saveAs(id, suggestedName) {
      let handle: FileSystemFileHandle;
      try {
        handle = await w.showSaveFilePicker({ suggestedName, types: [FERRUM_FILE] });
      } catch (e) {
        if (aborted(e)) return null;
        throw e;
      }
      return io.attachDisk(id, handle);
    },
    async access(id) {
      const h = (await io.handle(id)) as Handle | null;
      if (h) known.set(id, h);
      if (!h?.queryPermission) return 'granted';
      return h.queryPermission(mode);
    },
    async requestAccess(id) {
      const h = await handleOf(id);
      if (!h?.requestPermission) return true;
      return (await h.requestPermission(mode)) === 'granted';
    },
  };
}
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
import type { SettingsBackend } from './settings-store';
````

на:

````ts
import type { SettingsBackend } from './settings-store';
import { createDiskFiles, fsaSupported, type DiskFiles, type DiskIo } from './disk';
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  /** Список изменился (здесь или в другой вкладке); возвращает отписку. */
  onChange(listener: () => void): () => void;
  dispose(): void;
}
````

на:

````ts
  /** Список изменился (здесь или в другой вкладке); возвращает отписку. */
  onChange(listener: () => void): () => void;
  /** Файлы на диске (File System Access, Tauri); нет — пунктов «с диска» нет. */
  readonly disk?: DiskFiles;
  dispose(): void;
}
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
export class IdbFileStore implements FileStore, FileStoreTestHooks, SettingsBackend {
  private readonly settingsListeners = new Set<(patch: Record<string, unknown>) => void>();
````

на:

````ts
export class IdbFileStore implements FileStore, FileStoreTestHooks, SettingsBackend, DiskIo {
  private readonly settingsListeners = new Set<(patch: Record<string, unknown>) => void>();
  readonly disk: DiskFiles | undefined = fsaSupported() ? createDiskFiles(this) : undefined;
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  settings() {
    return this.call({ op: 'settings' });
  }
````

на:

````ts
  createFromDisk(handle: FileSystemFileHandle) {
    return this.call({ op: 'createFromDisk', handle });
  }
  attachDisk(file: string, handle: FileSystemFileHandle) {
    return this.call({ op: 'attachDisk', file, handle });
  }
  handle(file: string) {
    return this.call({ op: 'handle', file });
  }
  settings() {
    return this.call({ op: 'settings' });
  }
````

- [ ] **Step 4: Меню, экран файлов и запрос доступа**

В `apps/web/src/ui/root.tsx` заменить:

````ts
import { IdbFileStore, type FileMeta, type Section } from './file-store';
````

на:

````ts
import { ConfirmDialog } from './dialog';
import { IdbFileStore, type FileMeta, type Section } from './file-store';
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
import { isTestMode } from './boot';
````

на:

````ts
import { dismissSplash, isTestMode } from './boot';
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
  | { kind: 'editor'; file: FileMeta; restart?: Restart; backend?: Backend };
````

на:

````ts
  | { kind: 'editor'; file: FileMeta; restart?: Restart; backend?: Backend }
  /** Файл на диске без доступа (браузер перезапущен): вопрос, ответ — по щелчку. */
  | { kind: 'access'; file: FileMeta; answer: (ok: boolean) => void };
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
  async function edit(file: FileMeta, my: number): Promise<void> {
    const lock = await lockFile(file.id);
````

на:

````ts
  async function edit(file: FileMeta, my: number): Promise<void> {
    // Файл на диске: доступ к нему после перезапуска браузера снова разрешает
    // пользователь — щелчком в вопросе (запросу нужно действие пользователя).
    if (file.disk && files.disk && (await files.disk.access(file.id).catch(() => 'denied' as const)) !== 'granted') {
      const disk = files.disk;
      // Вопрос — поверх заставки нельзя: она перехватывала бы щелчки.
      dismissSplash(true);
      const ok = await new Promise<boolean>((answer) => setView({ kind: 'access', file, answer }));
      const granted = ok && (await disk.requestAccess(file.id).catch(() => false));
      if (my !== seq) return;
      if (!granted) {
        toFiles('Нет доступа к файлу на диске — разрешите его или откройте файл заново');
        return;
      }
    }
    const lock = await lockFile(file.id);
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
    <Switch>
      <Match when={view().kind === 'files'}>
````

на:

````ts
    <Switch>
      <Match when={(() => { const v = view(); return v.kind === 'access' ? v : null; })()} keyed>
        {(v) => (
          <ConfirmDialog
            title="Разрешить доступ к файлу на диске?"
            text={`Ferrum откроет «${v.file.disk}» и будет сохранять изменения в него`}
            confirm="Разрешить"
            cancel="Отмена"
            onConfirm={() => v.answer(true)}
            onCancel={() => v.answer(false)}
          />
        )}
      </Match>
      <Match when={view().kind === 'files'}>
````

`requestAccess` идёт сразу после щелчка «Разрешить»: обещание `answer` разрешается в обработчике щелчка, дальше — микрозадачи, а дескриптор уже запомнен при `access` (без обращения к worker'у), поэтому браузер считает запрос вызванным пользователем.

В `apps/web/src/ui/app.tsx` заменить:

````ts
  const fileItems = (ed: Editor): MenuItem[] => [
    { label: 'Новый файл', onSelect: () => props.navigate({ kind: 'new' }) },
    { label: 'Открыть…', onSelect: () => void openFromDisk() },
    { label: 'Сохранить', command: 'file.save', onSelect: () => void saver?.save() },
````

на:

````ts
  // File System Access: файл на диске, связанный с записью (правки пишутся в него).
  const openLinked = async () => {
    try {
      const meta = await props.files.disk?.open();
      if (meta) props.navigate({ kind: 'file', id: meta.id });
    } catch (e) {
      store.toast(`Не удалось открыть файл: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  };
  const saveToDisk = async () => {
    try {
      // Сначала — последнее в хранилище: на диск уходит свежий файл.
      if (saver && !(await saver.save())) return;
      const meta = await props.files.disk?.saveAs(id, fileName(store.state.documentName, '.ferrum'));
      if (meta) store.toast(`Файл сохранён на диск: ${meta.disk}`);
    } catch (e) {
      store.toast(`Не удалось сохранить на диск: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  };
  const disk = (items: MenuItem[]): MenuItem[] => (props.files.disk ? items : []);
  const fileItems = (ed: Editor): MenuItem[] => [
    { label: 'Новый файл', onSelect: () => props.navigate({ kind: 'new' }) },
    { label: 'Открыть…', onSelect: () => void openFromDisk() },
    ...disk([{ label: 'Открыть с диска…', onSelect: () => void openLinked() }]),
    { label: 'Сохранить', command: 'file.save', onSelect: () => void saver?.save() },
    ...disk([{ label: 'Сохранить на диск…', onSelect: () => void saveToDisk() }]),
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````ts
  const importFile = async () => {
    const meta = await importFromDisk(props.files, (t) => ui.toast(t, 'error'));
    if (meta) open(meta);
  };
````

на:

````ts
  const importFile = async () => {
    const meta = await importFromDisk(props.files, (t) => ui.toast(t, 'error'));
    if (meta) open(meta);
  };
  const openLinked = async () => {
    try {
      const meta = await props.files.disk?.open();
      if (meta) open(meta);
    } catch (e) {
      ui.toast(errorText('Не удалось открыть файл', e), 'error');
    }
  };
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````ts
                <button class="fe-btn fe-btn--secondary" onClick={() => void importFile()}>
                  <Icon name="upload" />
                  Импорт…
                </button>
````

на:

````ts
                <button class="fe-btn fe-btn--secondary" onClick={() => void importFile()}>
                  <Icon name="upload" />
                  Импорт…
                </button>
                <Show when={props.files.disk}>
                  <button class="fe-btn fe-btn--secondary" onClick={() => void openLinked()}>
                    <Icon name="upload" />
                    Открыть с диска…
                  </button>
                </Show>
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````ts
      : `Изменён ${relativeTime(f.editedAt, now())}`;
````

на:

````ts
      : `${f.disk ? 'На диске · ' : ''}Изменён ${relativeTime(f.editedAt, now())}`;
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````ts
              text={`Файл «${v.file.name}» будет удалён без возможности восстановления`}
````

на:

````ts
              text={
                v.file.disk
                  ? `Запись «${v.file.name}» будет удалена из списка; файл «${v.file.disk}» на диске останется`
                  : `Файл «${v.file.name}» будет удалён без возможности восстановления`
              }
````

В Chromium теперь есть и «Сохранить на диск…»: выбор пункта «Сохранить» в `chrome.spec.ts` уточняется.

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  await fileMenu(page, /^Сохранить(?! копию)/);
````

на:

````ts
  await fileMenu(page, /^Сохранить(?! копию| на диск)/);
````

- [ ] **Step 5: Проверить**

Run: `cd apps/web && npm run typecheck && npm run unit && npm run build && npx playwright test e2e/fsa.spec.ts e2e/files.spec.ts e2e/journal.spec.ts e2e/chrome.spec.ts`
Expected: PASS — `fsa` 5 passed (headless Chromium: OPFS и `FileSystemFileHandle` есть, системного диалога нет — его заменяют заглушки), прежние наборы как раньше. Настоящий системный диалог Chrome — ручная проверка пользователя (Task 14).

- [ ] **Step 6: Закоммитить**

```bash
git add apps/web/src apps/web/unit apps/web/e2e
git commit -m "feat(web): File System Access — «Открыть с диска…» и «Сохранить на диск…», запись в файл атомарно, чужая правка файла и запрос доступа

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Хранилище десктопа: крейт `ferrum-desktop-store`

Хранилище Tauri — на Rust, без самого Tauri: его логику (библиотека файлов, атомарная запись, журнал, превью, настройки) проверяет обычный `cargo test` за секунды, без окна и без сборки сотен крейтов Tauri. Крейт лежит в отдельном workspace десктопа `apps/desktop/src-tauri` (его создаёт этот task; приложение Tauri добавит Task 12), корневой `Cargo.toml` его исключает.

Модель — та же, что у IndexedDB-хранилища (разделы, поиск, окна ≤ 200, поколение записи, тексты отказов), с поправками на диск:
- Библиотека — `library.json` в каталоге данных приложения (`app_data_dir`), пишется атомарно. Испорченная — откладывается в `library.json.bad`, список начинается заново (одно предупреждение в журнал приложения).
- Новый файл — в папке документов (`~/Documents/Ferrum/<имя>.ferrum`, занятое имя — «Имя 2»), создаётся при первой записи; файл, открытый родным диалогом, остаётся на своём месте (тот же файл ещё раз — та же запись).
- Запись атомарная (спека 10): временный файл в том же каталоге → `fsync` → `rename` поверх → `fsync` каталога. Отказ посреди записи оставляет прежний файл.
- Чужая запись: поколение (`version`) не совпало — «Файл изменён в другом окне…»; время изменения файла не то, что после нашей записи или чтения, — «Файл на диске изменён другой программой…».
- Журнал — `journal/<id>/<сеанс 20 цифр>-<пачка 20 цифр>.bin`: порядок имён — порядок записи; запись файла стирает пачки до номера снимка; без байтов файла журнал не отдаётся. Имя пачки из интерфейса — только своего вида (не путь).
- «Удалить навсегда» удаляет файл, только если его создал Ferrum; открытый с диска остаётся.
- «Сохранить на диск…» (`move_to`) переносит файл записи по пути из диалога (свой прежний — удаляется); «Сохранить копию на диск…» (`copy_to`) — копия, запись не меняется.
- Настройки — `settings.json` в каталоге конфигурации: запись по полю поверх прочитанного, только известные поля с простыми значениями, ≤ 80 КБ, атомарно; испорченный файл — умолчания.

**Files:**
- Create: `apps/desktop/src-tauri/Cargo.toml` (workspace; пакет приложения добавит Task 12)
- Create: `apps/desktop/src-tauri/store/Cargo.toml`
- Create: `apps/desktop/src-tauri/store/src/{lib,atomic,library,settings}.rs`
- Test: `apps/desktop/src-tauri/store/tests/store.rs`
- Create: `apps/desktop/.gitignore`
- Modify: `Cargo.toml` (корень: `exclude`)

**Interfaces:**
- Consumes: ничего из других задач (самостоятельный крейт); тексты и пределы — те же, что в `apps/web/src/core/file-list.ts` и `limits.ts`.
- Produces: `Library::{open(root, docs) -> io::Result<(Library, Vec<String>)>, list(section, query, start, count) -> ListResult, get(id) -> Option<FileMeta>, create(name) -> Result<FileMeta, String>, add_existing(path), read(id) -> Result<Option<Vec<u8>>, String>, write(id, bytes, base, journal: Option<(u64, u64)>) -> Result<FileMeta, String>, write_thumb(id, Option<&[u8]>), thumb(id) -> Option<Vec<u8>>, journal_append(id, session, batch, bytes), journal_names(id) -> Result<Vec<String>, String>, journal_read(id, name), rename, touch, trash(id, on), delete, empty_trash, move_to(id, path), copy_to(id, path)}`; `FileMeta` (camelCase, поле `disk` — имя файла), `Section { Recent, Drafts, Trash }`, `ListResult { total, rows }`; `Settings::{open(path), all() -> Map, set(patch: Map) -> Result<(), String>}`; `atomic_write(path, bytes) -> io::Result<()>`; константы `CONFLICT`, `GONE`, `NOT_FOUND`, `DISK_CHANGED`, `DISK_MISSING`, `MAX_OPEN_BYTES`, `MAX_WINDOW`, `UNTITLED`, `MAX_NAME_CHARS`, `SETTINGS_FIELDS`.

- [ ] **Step 1: Workspace десктопа и падающие тесты**

`apps/desktop/src-tauri/Cargo.toml` (пакет приложения Tauri добавит Task 12):

````toml
# Десктоп Ferrum на Tauri 2 (этап 6). Свой workspace — не часть корневого: Tauri
# тянет сотни крейтов и системные библиотеки окна, а `cargo test --workspace` ядра не
# должен их собирать.
[workspace]
members = ["store"]
resolver = "2"
````

`apps/desktop/.gitignore`:

````
/node_modules/
/src-tauri/target/
/src-tauri/gen/
````

В `Cargo.toml` (корень репозитория) заменить:

````toml
    "crates/ferrum-wasm",
]
````

на:

````toml
    "crates/ferrum-wasm",
]
# Десктоп — свой workspace со своим Cargo.lock (Tauri и сотни крейтов окна).
exclude = ["apps/desktop/src-tauri"]
````

`apps/desktop/src-tauri/store/Cargo.toml`:

````toml
[package]
name = "ferrum-desktop-store"
version = "0.1.0"
edition = "2021"
rust-version = "1.96"
license = "UNLICENSED"

# Хранилище десктопа без Tauri: библиотека файлов, атомарная запись, журнал, превью,
# настройки. Тестируется обычным `cargo test`, без окна и без сборки Tauri.
[dependencies]
serde = { version = "=1.0.229", features = ["derive"] }
serde_json = "=1.0.152"
uuid = { version = "=1.27.0", features = ["v4"] }
````

`apps/desktop/src-tauri/store/tests/store.rs`:

````rust
//! Хранилище десктопа (этап 6): новый файл и запись, атомарная запись без временных
//! файлов и без порчи при отказе, чужая запись (поколение) и чужая программа (время
//! изменения), журнал по порядку и его стирание, разделы и поиск, удаление своих и
//! чужих файлов, переезд на диск, испорченный список, настройки.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ferrum_desktop_store::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Свой пустой каталог на тест: данные приложения и документы.
fn dirs() -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "ferrum-store-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&base);
    (base.join("data"), base.join("docs"))
}

fn lib() -> (Library, PathBuf, PathBuf) {
    let (data, docs) = dirs();
    let (lib, warnings) = Library::open(&data, &docs).unwrap();
    assert!(warnings.is_empty());
    (lib, data, docs)
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|d| d.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn new_file_is_written_atomically_into_docs() {
    let (mut lib, _, docs) = lib();
    let m = lib.create("Макет").unwrap();
    assert_eq!(
        (m.version, m.size, m.disk.as_str()),
        (0.0, 0.0, "Макет.ferrum")
    );
    assert_eq!(lib.read(&m.id).unwrap(), None, "новый — без байтов");
    let w = lib.write(&m.id, b"FERRUM-1", 0.0, None).unwrap();
    assert_eq!((w.version, w.size), (1.0, 8.0));
    assert_eq!(lib.read(&m.id).unwrap().as_deref(), Some(&b"FERRUM-1"[..]));
    lib.write(&m.id, b"FERRUM-22", 1.0, None).unwrap();
    // Только сам файл: временных рядом не осталось.
    assert_eq!(files_in(&docs), vec!["Макет.ferrum".to_string()]);
    // Второе такое же имя — свой файл.
    let other = lib.create("Макет").unwrap();
    assert_eq!(other.disk, "Макет 2.ferrum");
}

#[cfg(unix)]
#[test]
fn failed_write_keeps_the_old_file() {
    use std::os::unix::fs::PermissionsExt;
    let (mut lib, _, docs) = lib();
    let m = lib.create("Макет").unwrap();
    lib.write(&m.id, b"OLD", 0.0, None).unwrap();
    // Каталог только для чтения: временный файл не создать.
    fs::set_permissions(&docs, fs::Permissions::from_mode(0o555)).unwrap();
    let err = lib.write(&m.id, b"NEW", 1.0, None);
    fs::set_permissions(&docs, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(err.is_err());
    assert_eq!(fs::read(docs.join("Макет.ferrum")).unwrap(), b"OLD");
    assert_eq!(
        lib.get(&m.id).unwrap().version,
        1.0,
        "поколение не сдвинулось"
    );
}

#[test]
fn stale_generation_and_foreign_program_are_refused() {
    let (mut lib, _, docs) = lib();
    let m = lib.create("Макет").unwrap();
    lib.write(&m.id, b"A", 0.0, None).unwrap();
    assert_eq!(lib.write(&m.id, b"B", 0.0, None), Err(CONFLICT.to_string()));
    // Другая программа записала файл.
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(docs.join("Макет.ferrum"), b"CHANGED").unwrap();
    assert_eq!(
        lib.write(&m.id, b"B", 1.0, None),
        Err(DISK_CHANGED.to_string())
    );
    // Открыли заново (прочитали) — запись снова идёт.
    assert_eq!(lib.read(&m.id).unwrap().as_deref(), Some(&b"CHANGED"[..]));
    assert!(lib.write(&m.id, b"B", 1.0, None).is_ok());
    // Удалённая запись: «Файл удалён…».
    lib.delete(&m.id).unwrap();
    assert_eq!(lib.write(&m.id, b"C", 2.0, None), Err(GONE.to_string()));
}

#[test]
fn journal_is_ordered_and_dropped_by_the_snapshot() {
    let (mut lib, ..) = lib();
    let m = lib.create("Журнал").unwrap();
    lib.journal_append(&m.id, 20, 1, b"a").unwrap();
    assert!(
        lib.journal_names(&m.id).unwrap().is_empty(),
        "без байтов файла журнала нет"
    );
    lib.write(&m.id, b"F", 0.0, None).unwrap();
    lib.journal_append(&m.id, 20, 2, b"b").unwrap();
    lib.journal_append(&m.id, 10, 7, b"c").unwrap();
    let read = |lib: &Library| -> Vec<Vec<u8>> {
        lib.journal_names(&m.id)
            .unwrap()
            .iter()
            .map(|n| lib.journal_read(&m.id, n).unwrap())
            .collect()
    };
    assert_eq!(
        read(&lib),
        vec![b"c".to_vec(), b"a".to_vec(), b"b".to_vec()]
    );
    lib.write(&m.id, b"F2", 1.0, Some((20, 1))).unwrap();
    assert_eq!(read(&lib), vec![b"b".to_vec()]);
    assert!(
        lib.journal_read(&m.id, "../library.json").is_err(),
        "только свои имена"
    );
}

#[test]
fn sections_search_and_window() {
    let (mut lib, ..) = lib();
    let a = lib.create("Альфа").unwrap();
    let b = lib.create("Бета").unwrap();
    lib.create("альфа два").unwrap();
    lib.trash(&b.id, true).unwrap();
    let names = |r: ListResult| r.rows.into_iter().map(|m| m.name).collect::<Vec<_>>();
    assert_eq!(lib.list(Section::Drafts, "", 0, 50).total, 2);
    assert_eq!(names(lib.list(Section::Trash, "", 0, 50)), vec!["Бета"]);
    assert_eq!(lib.list(Section::Drafts, "АЛЬФА", 0, 50).total, 2);
    assert_eq!(lib.list(Section::Drafts, "", 0, 1).rows.len(), 1);
    assert_eq!(lib.list(Section::Drafts, "", 0, 10_000).rows.len(), 2);
    lib.trash(&b.id, false).unwrap();
    assert_eq!(lib.list(Section::Trash, "", 0, 50).total, 0);
    let _ = a;
}

#[test]
fn delete_removes_own_files_but_not_foreign_ones() {
    let (mut lib, _, docs) = lib();
    let own = lib.create("Свой").unwrap();
    lib.write(&own.id, b"F", 0.0, None).unwrap();
    let outside = docs.parent().unwrap().join("чужой.ferrum");
    fs::write(&outside, b"FOREIGN").unwrap();
    let foreign = lib.add_existing(&outside).unwrap();
    assert_eq!(foreign.version, 1.0);
    assert_eq!(
        lib.add_existing(&outside).unwrap().id,
        foreign.id,
        "тот же файл — та же запись"
    );
    lib.trash(&own.id, true).unwrap();
    lib.trash(&foreign.id, true).unwrap();
    lib.empty_trash().unwrap();
    assert!(!docs.join("Свой.ferrum").exists());
    assert_eq!(fs::read(&outside).unwrap(), b"FOREIGN");
    assert_eq!(lib.list(Section::Drafts, "", 0, 50).total, 0);
}

#[test]
fn move_to_disk_and_copy() {
    let (mut lib, _, docs) = lib();
    let m = lib.create("Макет").unwrap();
    lib.write(&m.id, b"F", 0.0, None).unwrap();
    let target = docs.parent().unwrap().join("на диске.ferrum");
    let moved = lib.move_to(&m.id, &target).unwrap();
    assert_eq!(moved.disk, "на диске.ferrum");
    assert!(!docs.join("Макет.ferrum").exists(), "свой файл переехал");
    lib.write(&m.id, b"F2", 1.0, None).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"F2");
    let copy = docs.parent().unwrap().join("копия.ferrum");
    lib.copy_to(&m.id, &copy).unwrap();
    assert_eq!(fs::read(&copy).unwrap(), b"F2");
    assert_eq!(
        lib.get(&m.id).unwrap().disk,
        "на диске.ferrum",
        "копия запись не меняет"
    );
}

#[test]
fn library_survives_restart_and_damage() {
    let (data, docs) = dirs();
    let id = {
        let (mut lib, _) = Library::open(&data, &docs).unwrap();
        lib.create("Остаётся").unwrap().id
    };
    let (lib, warnings) = Library::open(&data, &docs).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(lib.get(&id).unwrap().name, "Остаётся");
    fs::write(data.join("library.json"), b"{ not json").unwrap();
    let (lib, warnings) = Library::open(&data, &docs).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(lib.get(&id).is_none());
    assert!(data.join("library.json.bad").exists());
}

#[test]
fn settings_are_written_by_field_and_checked() {
    let (data, _) = dirs();
    let path = data.join("settings.json");
    let mut s = Settings::open(&path).unwrap();
    let patch = |v: serde_json::Value| v.as_object().unwrap().clone();
    s.set(patch(serde_json::json!({"theme": "light"}))).unwrap();
    s.set(patch(serde_json::json!({"filesView": "list"})))
        .unwrap();
    assert!(s.set(patch(serde_json::json!({"чужое": 1}))).is_err());
    assert!(s
        .set(patch(serde_json::json!({"theme": {"x": 1}})))
        .is_err());
    let big = "x".repeat(90 * 1024);
    assert!(s.set(patch(serde_json::json!({ "keymap": big }))).is_err());
    let again = Settings::open(&path).unwrap();
    assert_eq!(
        serde_json::Value::Object(again.all()),
        serde_json::json!({"theme": "light", "filesView": "list"})
    );
    fs::write(&path, b"[]").unwrap();
    assert!(
        Settings::open(&path).unwrap().all().is_empty(),
        "испорченный — умолчания"
    );
}
````

Run: `cd apps/desktop/src-tauri && CARGO_TARGET_DIR=$PWD/target cargo test -p ferrum-desktop-store`
Expected: FAIL — нет `src/lib.rs`.

- [ ] **Step 2: Атомарная запись, библиотека, настройки**

`apps/desktop/src-tauri/store/src/lib.rs`:

````rust
//! Хранилище десктопа Ferrum (этап 6, спека 10): файлы `.ferrum` на диске для Tauri —
//! то же, что хранилище IndexedDB веб-версии (`apps/web/src/worker/io.worker.ts`):
//! список с разделами «Недавние», «Черновики», «Корзина», поиск и окна; байты файла,
//! превью, журнал изменений и настройки пользователя.
//!
//! - Библиотека (`library.json` в каталоге данных приложения) — метаданные записей и
//!   где лежит файл. Новый файл создаётся в папке документов Ferrum; открытый с диска
//!   остаётся на своём месте. Пути приходят только из родных диалогов (Rust), интерфейс
//!   их не передаёт.
//! - Запись файла атомарная (спека 10): временный файл рядом → `fsync` → переименование
//!   поверх → `fsync` каталога. Сбой посреди записи оставляет прежний файл целым.
//! - Поколение записи (`version`) и время изменения файла на диске (`disk_modified`)
//!   защищают от тихой перезаписи: файл, записанный другим окном или другой
//!   программой, — отказ.
//! - Журнал — файлы `journal/<id>/<сеанс>-<пачка>.bin` (номера с нулями слева: порядок
//!   имён — порядок записи); запись файла стирает пачки до номера своего снимка.
//! - Всё, что приходит из интерфейса, не доверенное: id — UUID, имена очищаются, байты
//!   ограничены, имена пачек журнала — только свои.

mod atomic;
mod library;
mod settings;

pub use atomic::atomic_write;
pub use library::{FileMeta, Library, ListResult, Section};
pub use settings::{Settings, SETTINGS_FIELDS};

/// Тексты отказов — те же, что у веб-хранилища (`core/file-list.ts`).
pub const CONFLICT: &str =
    "Файл изменён в другом окне — сохраните копию на диск или откройте файл заново";
pub const GONE: &str = "Файл удалён — сохраните копию на диск";
pub const NOT_FOUND: &str = "Файл не найден";
pub const DISK_CHANGED: &str =
    "Файл на диске изменён другой программой — сохраните копию на диск или откройте файл заново";
pub const DISK_MISSING: &str = "Файл на диске не найден — он перемещён или удалён";
/// Наибольший файл (как `MAX_OPEN_BYTES` интерфейса) и окно списка.
pub const MAX_OPEN_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_WINDOW: usize = 200;
pub const UNTITLED: &str = "Без названия";
/// Наибольшая длина имени записи в символах (как `MAX_NAME_CHARS`).
pub const MAX_NAME_CHARS: usize = 256;
````

`apps/desktop/src-tauri/store/src/atomic.rs`:

````rust
//! Атомарная запись файла (спека 10): временный файл в том же каталоге, `fsync`,
//! переименование поверх прежнего и `fsync` каталога — после сбоя на диске либо
//! прежний файл, либо новый, но не половина.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "у файла нет каталога"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "у файла нет имени"))?
        .to_string_lossy();
    let tmp = dir.join(format!(
        ".{name}.ferrum-tmp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)?;
        // Переименование — запись в каталог: без его fsync новое имя может не пережить сбой.
        #[cfg(unix)]
        fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
````

`apps/desktop/src-tauri/store/src/library.rs`:

````rust
//! Библиотека файлов десктопа: записи, их файлы на диске, превью и журнал.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::atomic::atomic_write;
use crate::{
    CONFLICT, DISK_CHANGED, DISK_MISSING, GONE, MAX_NAME_CHARS, MAX_OPEN_BYTES, MAX_WINDOW,
    NOT_FOUND, UNTITLED,
};

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
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Имя пачки журнала: `<сеанс>-<пачка>.bin` с 20 цифрами в каждом номере.
fn journal_name(session: u64, batch: u64) -> String {
    format!("{session:020}-{batch:020}.bin")
}

fn parse_journal_name(name: &str) -> Option<(u64, u64)> {
    let stem = name.strip_suffix(".bin")?;
    let (s, b) = stem.split_once('-')?;
    if s.len() != 20 || b.len() != 20 {
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
        self.entries.get(id).ok_or_else(|| NOT_FOUND.to_string())
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
            return Err(format!(
                "Слишком большой объём данных: предел — {} МБ",
                MAX_OPEN_BYTES / 1024 / 1024
            ));
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
            return Err(format!(
                "Слишком большой объём данных: предел — {} МБ",
                MAX_OPEN_BYTES / 1024 / 1024
            ));
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
        let e = self.entries.get(id).ok_or_else(|| GONE.to_string())?;
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
        if bytes.len() as u64 > MAX_OPEN_BYTES {
            return Err("Слишком большой объём данных".into());
        }
        let dir = self.journal_dir(id);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        atomic_write(&dir.join(journal_name(session, batch)), bytes).map_err(|e| e.to_string())
    }

    /// Имена пачек журнала по порядку записи. Без байтов файла журнала нет: его пачки
    /// начаты на снимке, которого нет.
    pub fn journal_names(&self, id: &str) -> Result<Vec<String>, String> {
        let e = self.entry(id)?;
        if !e.path.exists() {
            return Ok(Vec::new());
        }
        let mut names: Vec<String> = match fs::read_dir(self.journal_dir(id)) {
            Ok(dir) => dir
                .filter_map(|d| d.ok())
                .map(|d| d.file_name().to_string_lossy().into_owned())
                .filter(|n| parse_journal_name(n).is_some())
                .collect(),
            Err(_) => Vec::new(),
        };
        names.sort();
        Ok(names)
    }

    /// Пачка журнала по имени (только имена своего вида: не путь).
    pub fn journal_read(&self, id: &str, name: &str) -> Result<Vec<u8>, String> {
        self.entry(id)?;
        if parse_journal_name(name).is_none() {
            return Err("Неверный запрос к хранилищу файлов".into());
        }
        fs::read(self.journal_dir(id).join(name)).map_err(|e| e.to_string())
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
````

`apps/desktop/src-tauri/store/src/settings.rs`:

````rust
//! Настройки пользователя десктопа — файл конфигурации (спека 7.6) `settings.json`:
//! объект полей, как хранилище `settings` веб-версии. Запись — только переданные поля
//! (поверх прочитанного файла) и атомарно; значения проверяет интерфейс при чтении.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::atomic::atomic_write;

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
                .unwrap_or_default(),
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
        let json = serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?;
        if json.len() > MAX_SETTINGS_BYTES {
            return Err("Слишком большой объём данных: настройки больше 80 КБ".into());
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        atomic_write(&self.path, &json).map_err(|e| e.to_string())?;
        self.values = next;
        Ok(())
    }
}
````

- [ ] **Step 3: Проверить**

Run: `cd apps/desktop/src-tauri && CARGO_TARGET_DIR=$PWD/target cargo test -p ferrum-desktop-store && cargo clippy -p ferrum-desktop-store --all-targets -- -D warnings && cargo fmt --check`
Expected: PASS, `9 passed` (проверено на прототипе на этой машине, macOS 26.5, Apple M1 Pro), без предупреждений. `Cargo.lock` десктопа создаётся здесь; зависимости — `serde`, `serde_json`, `uuid` (и `getrandom` для UUID).

Run: `cargo test --workspace` (корень)
Expected: PASS — корневой workspace десктоп не видит (`exclude`).

- [ ] **Step 4: Закоммитить**

```bash
git add Cargo.toml apps/desktop
git commit -m "feat(desktop): хранилище десктопа — библиотека файлов на диске, атомарная запись, журнал, превью и настройки (без Tauri, cargo test)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: `apps/desktop`: приложение Tauri 2

Десктоп оборачивает сборку `apps/web` (`frontendDist: ../../web/dist`, ресурсы встраиваются в бинарник) и даёт ему хранилище Task 11 командами IPC. Интерфейс узнаёт Tauri по `window.__TAURI_INTERNALS__` и берёт `TauriFileStore` (файлы, журнал, превью, настройки — в Rust) вместо `IdbFileStore`; диалоги «Открыть с диска…», «Сохранить на диск…», «Сохранить копию на диск…» и «Скачать копию» — родные (`tauri-plugin-dialog`, вызываются из Rust: пути не покидают Rust). Байты — сырым телом IPC (`InvokeBody::Raw`) и ответом `tauri::ipc::Response` (в JS — `ArrayBuffer`), параметры записи — заголовком `x-ferrum`.

Заголовки изоляции (`Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp`; `credentialless` WebKit не знает) заданы в `app.security.headers`. Проверено на прототипе: в WKWebView (macOS 26.5) страница по `tauri://localhost` изолированной не становится (`crossOriginIsolated === false`) — ядро работает на запасном транспорте postMessage (этап 4), это видно в отчёте проверки запуска. SharedArrayBuffer в десктопе — отложено (раздел «Что этап 6 не делает»).

Проверка запуска без человека: `FERRUM_SMOKE=1` — библиотека и настройки во временном каталоге (библиотека пользователя не трогается), интерфейс открывает `#/new`, ждёт `opened` ядра и командой `smoke_done` отдаёт отчёт; приложение печатает `smoke: {…}` и выходит с кодом 0 (ядро запустилось) или 1; без ответа за 90 с — `smoke: timeout`, код 1. На прототипе (Apple M1 Pro, macOS 26.5): debug — `ready: true` за 2,1 с, собранный `.app` — за 0,25 с. Окно при проверке открывается на эти секунды. Данные WebKit окна (`~/Library/WebKit/app.ferrum.desktop`) проверка оставляет — это кэш webview.

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml` (пакет приложения в workspace Task 11)
- Create: `apps/desktop/src-tauri/{build.rs,tauri.conf.json}`, `capabilities/default.json`, `src/{main,lib}.rs`
- Create: `apps/desktop/{package.json,.npmrc}`, `apps/desktop/scripts/make-icon.mjs`
- Create: `apps/desktop/src-tauri/icons/{icon.png,32x32.png,128x128.png,128x128@2x.png,icon.icns,icon.ico}` (генерируются, затем коммитятся)
- Create: `apps/web/src/ui/tauri.ts`
- Modify: `apps/web/src/ui/{disk,root,app,files-screen}.ts(x)`, `apps/web/src/main.tsx`
- Test: `apps/web/unit/tauri.test.ts`

**Interfaces:**
- Consumes: `ferrum-desktop-store` (Task 11); `FileStore`, `DiskFiles` (Tasks 4, 10), `SettingsBackend` (Task 9).
- Produces: команды IPC `files_list`, `files_get`, `files_create`, `files_read`, `files_write` (сырое тело, заголовок `{ id, base, session?, upTo? }`), `files_write_thumb` (сырое тело, пустое — убрать), `files_thumb`, `files_journal_append` (сырое тело, `{ id, session, batch }`), `files_journal_names`, `files_journal_read`, `files_rename`, `files_touch`, `files_trash { id, on }`, `files_delete`, `files_empty_trash`, `files_open_dialog`, `files_save_dialog { id, name }`, `files_copy_dialog { id, name }`, `settings_get`, `settings_set { patch }`, `smoke_mode`, `smoke_done { report }`; TypeScript: `isTauri()`, `TauriFileStore implements FileStore, SettingsBackend`, `runSmoke()`; `DiskFiles.saveCopy?(id, suggestedName): Promise<boolean>`.

- [ ] **Step 1: Написать падающие модульные тесты IPC**

`apps/web/unit/tauri.test.ts`:

````ts
// Хранилище десктопа со стороны интерфейса (этап 6): вызовы IPC Tauri — без окна,
// подменой `window.__TAURI_INTERNALS__`. Запись — сырым телом с параметрами в
// заголовке, превью — отдельно (или не трогается), чтение — байты и пачки журнала по
// порядку, пустой ответ — новый файл без журнала, отказ Rust — ошибка с тем же текстом.

import { expect, test } from '@playwright/test';
import { isTauri, TauriFileStore } from '../src/ui/tauri';

const ID = '0f8e2a06-1111-4222-8333-444455556666';

function fakeIpc(replies: Record<string, (args: unknown) => unknown>) {
  const calls: { cmd: string; args: unknown; headers?: Record<string, string> }[] = [];
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd: string, args: unknown, opts?: { headers?: Record<string, string> }) => {
        calls.push({ cmd, args, headers: opts?.headers });
        const r = replies[cmd];
        if (!r) throw `нет команды ${cmd}`;
        return r(args);
      },
    },
  };
  return calls;
}

test.afterEach(() => {
  delete (globalThis as unknown as { window?: unknown }).window;
});

test('isTauri — только с IPC окна', () => {
  expect(isTauri()).toBe(false);
  fakeIpc({});
  expect(isTauri()).toBe(true);
});

test('запись — сырым телом с параметрами в заголовке; превью отдельно или не трогается', async () => {
  const calls = fakeIpc({
    files_write: () => ({ id: ID, version: 2 }),
    files_write_thumb: () => ({ id: ID, version: 2, thumbVersion: 1 }),
  });
  const s = new TauriFileStore();
  const m = await s.write(ID, new Uint8Array([1, 2, 3]).buffer, new Uint8Array([9]).buffer, 1, { session: 5, upTo: 2 });
  expect(m).toEqual({ id: ID, version: 2, thumbVersion: 1 });
  expect(calls.map((c) => c.cmd)).toEqual(['files_write', 'files_write_thumb']);
  expect(calls[0].args).toEqual(new Uint8Array([1, 2, 3]));
  expect(calls[0].headers).toEqual({ 'x-ferrum': JSON.stringify({ id: ID, base: 1, session: 5, upTo: 2 }) });
  expect(calls[1].args).toEqual(new Uint8Array([9]));
  calls.length = 0;
  await s.write(ID, new ArrayBuffer(4), 'keep', 2, null);
  expect(calls.map((c) => c.cmd)).toEqual(['files_write']);
  expect(calls[0].headers).toEqual({ 'x-ferrum': JSON.stringify({ id: ID, base: 2 }) });
  calls.length = 0;
  await s.write(ID, new ArrayBuffer(4), null, 3, null);
  expect(calls[1]).toMatchObject({ cmd: 'files_write_thumb', args: new Uint8Array(0) });
});

test('чтение — байты и пачки журнала по порядку; пустой ответ — новый файл без журнала', async () => {
  const meta = { id: ID, name: 'Макет', version: 1 };
  let bytes = new Uint8Array([7, 7]).buffer;
  const calls = fakeIpc({
    files_read: () => bytes,
    files_get: () => meta,
    files_journal_names: () => ['a', 'b'],
    files_journal_read: (a) => new Uint8Array([(a as { name: string }).name === 'a' ? 1 : 2]).buffer,
  });
  const s = new TauriFileStore();
  const r = await s.read(ID);
  expect(r.meta).toEqual(meta);
  expect(new Uint8Array(r.bytes!)).toEqual(new Uint8Array([7, 7]));
  expect(r.journal.map((b) => new Uint8Array(b)[0])).toEqual([1, 2]);
  bytes = new ArrayBuffer(0);
  calls.length = 0;
  const empty = await s.read(ID);
  expect(empty.bytes).toBeNull();
  expect(empty.journal).toEqual([]);
  expect(calls.some((c) => c.cmd === 'files_journal_names')).toBe(false);
});

test('отказ Rust — ошибка с тем же текстом; список узнаёт о своих правках', async () => {
  fakeIpc({
    files_rename: () => {
      throw 'Файл не найден';
    },
    files_trash: () => null,
  });
  const s = new TauriFileStore();
  await expect(s.rename(ID, 'x')).rejects.toThrow('Файл не найден');
  let changed = 0;
  s.onChange(() => changed++);
  await s.trash(ID);
  expect(changed).toBe(1);
});
````

Run: `cd apps/web && npm run unit -- -g "isTauri|сырым телом|пачки журнала по порядку|отказ Rust"`
Expected: FAIL — нет `src/ui/tauri.ts`.

- [ ] **Step 2: Хранилище Tauri в интерфейсе**

`apps/web/src/ui/tauri.ts`:

````ts
// Десктоп (Tauri 2, этап 6): хранилище файлов и настройки — в Rust (`apps/desktop`,
// крейт `ferrum-desktop-store`), интерфейс обращается к ним командами IPC. IPC есть
// только у окна, поэтому байты файла, журнала и превью идут в Rust из основного потока
// тем же `ArrayBuffer` сырым телом запроса (без JSON и без копий в JS), а запись,
// `fsync` и переименование — в потоке Tauri. Пути на диске интерфейс не видит и не
// передаёт: их дают только родные диалоги в Rust. Окно одно: изменения списка
// интерфейс узнаёт от своих же вызовов, `BroadcastChannel` не нужен.

import type { FileMeta, IoResults, JournalMark, Section } from '../core/file-list';
import type { DiskFiles } from './disk';
import type { FileStore } from './file-store';
import type { Settings } from './settings';
import type { SettingsBackend } from './settings-store';

type Invoke = (cmd: string, args?: unknown, options?: { headers?: Record<string, string> }) => Promise<unknown>;

function ipc(): Invoke | null {
  if (typeof window === 'undefined') return null;
  const internals = (window as unknown as { __TAURI_INTERNALS__?: { invoke?: Invoke } }).__TAURI_INTERNALS__;
  return typeof internals?.invoke === 'function' ? internals.invoke.bind(internals) : null;
}

/** Окно Tauri: IPC из `window.__TAURI_INTERNALS__` (его же вызывает `@tauri-apps/api/core`). */
export function isTauri(): boolean {
  return ipc() !== null;
}

/** Вызов команды; отказ Rust (строка) — `Error` с тем же текстом. */
async function invoke<T>(cmd: string, args?: unknown, headers?: Record<string, string>): Promise<T> {
  const call = ipc();
  if (!call) throw new Error('Нет связи с приложением');
  try {
    return (await call(cmd, args, headers ? { headers } : undefined)) as T;
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e));
  }
}

/** Сырое тело запроса и параметры заголовком `x-ferrum` (ASCII: UUID и числа). */
function raw<T>(cmd: string, bytes: ArrayBuffer, args: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, new Uint8Array(bytes), { 'x-ferrum': JSON.stringify(args) });
}

const bytesOrNull = (b: ArrayBuffer) => (b.byteLength > 0 ? b : null);

export class TauriFileStore implements FileStore, SettingsBackend {
  private readonly listeners = new Set<() => void>();

  /** Файлы на диске — родными диалогами Rust; доступ к файлу есть всегда. */
  readonly disk: DiskFiles = {
    open: () => this.changed(invoke<FileMeta | null>('files_open_dialog')),
    saveAs: (id, name) => this.changed(invoke<FileMeta | null>('files_save_dialog', { id, name })),
    saveCopy: (id, name) => invoke<boolean>('files_copy_dialog', { id, name }),
    access: async () => 'granted',
    requestAccess: async () => true,
  };

  /** Изменение списка — своему экрану. */
  private async changed<T>(p: Promise<T>): Promise<T> {
    const v = await p;
    for (const l of this.listeners) l();
    return v;
  }

  list(section: Section, query: string, start: number, count: number) {
    return invoke<IoResults['list']>('files_list', { section, query, start, count });
  }
  get(id: string) {
    return invoke<FileMeta | null>('files_get', { id });
  }
  async create(name: string, bytes?: Blob) {
    const meta = await invoke<FileMeta>('files_create', { name });
    // «Импорт…»: содержимое файла с диска — сразу записью (чтение Blob — ввод-вывод, не разбор).
    if (bytes) return this.changed(raw<FileMeta>('files_write', await bytes.arrayBuffer(), { id: meta.id, base: 0 }));
    for (const l of this.listeners) l();
    return meta;
  }
  async read(id: string) {
    const bytes = bytesOrNull(await invoke<ArrayBuffer>('files_read', { id }));
    const meta = await this.get(id);
    if (!meta) throw new Error('Файл не найден');
    const journal: ArrayBuffer[] = [];
    if (bytes) {
      for (const name of await invoke<string[]>('files_journal_names', { id })) {
        journal.push(await invoke<ArrayBuffer>('files_journal_read', { id, name }));
      }
    }
    return { meta, bytes, journal };
  }
  async write(
    id: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ) {
    const args = { id, base, ...(journal ? { session: journal.session, upTo: journal.upTo } : {}) };
    let meta = await raw<FileMeta>('files_write', bytes, args);
    if (thumb !== 'keep') meta = await raw<FileMeta>('files_write_thumb', thumb ?? new ArrayBuffer(0), { id });
    for (const l of this.listeners) l();
    return meta;
  }
  async appendJournal(id: string, mark: JournalMark, bytes: ArrayBuffer) {
    await raw<null>('files_journal_append', bytes, { id, session: mark.session, batch: mark.batch });
  }
  async blob(id: string) {
    const b = bytesOrNull(await invoke<ArrayBuffer>('files_read', { id }));
    return b ? new Blob([b], { type: 'application/octet-stream' }) : null;
  }
  rename(id: string, name: string) {
    return this.changed(invoke<FileMeta>('files_rename', { id, name }));
  }
  touch(id: string) {
    return this.changed(invoke<FileMeta>('files_touch', { id }));
  }
  async trash(id: string) {
    await this.changed(invoke<null>('files_trash', { id, on: true }));
  }
  async restore(id: string) {
    await this.changed(invoke<null>('files_trash', { id, on: false }));
  }
  async remove(id: string) {
    await this.changed(invoke<null>('files_delete', { id }));
  }
  async emptyTrash() {
    await this.changed(invoke<null>('files_empty_trash'));
  }
  async thumbnail(id: string) {
    const b = bytesOrNull(await invoke<ArrayBuffer>('files_thumb', { id }));
    return b ? new Blob([b], { type: 'image/png' }) : null;
  }
  onChange(listener: () => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  dispose() {
    this.listeners.clear();
  }

  // Настройки — файл конфигурации; окно одно — чужих изменений нет.
  settings() {
    return invoke<Record<string, unknown>>('settings_get');
  }
  async setSettings(patch: Partial<Settings>) {
    await invoke<null>('settings_set', { patch });
  }
  onSettings() {
    return () => {};
  }
}

/** Проверка запуска десктопа (`FERRUM_SMOKE=1`, `apps/desktop`): новый файл, ядро
 * готово — отчёт приложению, оно печатает его и выходит. */
export async function runSmoke(): Promise<void> {
  if (!isTauri() || !(await invoke<boolean>('smoke_mode'))) return;
  location.hash = '#/new';
  const t0 = performance.now();
  const ready = () => (window as { __ferrum?: { ready: boolean } }).__ferrum?.ready === true;
  while (performance.now() - t0 < 60_000 && !ready()) await new Promise((r) => setTimeout(r, 100));
  let files = -1;
  try {
    files = (await new TauriFileStore().list('drafts', '', 0, 10)).total;
  } catch {
    files = -1;
  }
  await invoke<null>('smoke_done', {
    report: { ready: ready(), isolated: crossOriginIsolated, files, ms: Math.round(performance.now() - t0) },
  });
}
````

В `apps/web/src/ui/disk.ts` заменить:

````ts
  /** Запросить доступ — только из обработчика действия пользователя. */
  requestAccess(id: string): Promise<boolean>;
}
````

на:

````ts
  /** Запросить доступ — только из обработчика действия пользователя. */
  requestAccess(id: string): Promise<boolean>;
  /** Копия файла в выбранное место родным диалогом (десктоп); нет — копия скачиванием. */
  saveCopy?(id: string, suggestedName: string): Promise<boolean>;
}
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
import { IdbFileStore, type FileMeta, type Section } from './file-store';
````

на:

````ts
import { IdbFileStore, type FileMeta, type FileStore, type Section } from './file-store';
import { isTauri, TauriFileStore } from './tauri';
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
import { openSettings, type SettingsStore } from './settings-store';
````

на:

````ts
import { openSettings, type SettingsBackend, type SettingsStore } from './settings-store';
````

В `apps/web/src/ui/root.tsx` заменить:

````ts
  const files = new IdbFileStore(test);
  if (test) window.__ferrumFiles = { store: files };
````

на:

````ts
  // Десктоп — хранилище на диске через Tauri; браузер — IndexedDB в worker'е.
  const idb = isTauri() ? null : new IdbFileStore(test);
  const files: FileStore & SettingsBackend = idb ?? new TauriFileStore();
  if (test && idb) window.__ferrumFiles = { store: idb };
````

В `apps/web/src/main.tsx` заменить:

````ts
import { Root } from './ui/root';

render(() => <Root />, document.getElementById('app')!);
````

на:

````ts
import { Root } from './ui/root';
import { runSmoke } from './ui/tauri';

render(() => <Root />, document.getElementById('app')!);
// Десктоп с `FERRUM_SMOKE=1`: проверка запуска (вне Tauri ничего не делает).
void runSmoke();
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
  const disk = (items: MenuItem[]): MenuItem[] => (props.files.disk ? items : []);
````

на:

````ts
  // «Сохранить копию на диск…»: в десктопе — родной диалог (копия сохранённого файла),
  // в браузере — байты из ядра скачиванием.
  const saveCopy = async (ed: Editor) => {
    const copy = props.files.disk?.saveCopy;
    if (!copy) {
      ed.send({ type: 'save' });
      return;
    }
    try {
      if (saver && !(await saver.save())) return;
      if (await copy(id, fileName(store.state.documentName, '.ferrum'))) store.toast('Копия сохранена');
    } catch (e) {
      store.toast(`Не удалось сохранить копию: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  };
  const disk = (items: MenuItem[]): MenuItem[] => (props.files.disk ? items : []);
````

В `apps/web/src/ui/app.tsx` заменить:

````ts
    { label: 'Сохранить копию на диск…', onSelect: () => ed.send({ type: 'save' }) },
````

на:

````ts
    { label: 'Сохранить копию на диск…', onSelect: () => void saveCopy(ed) },
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````ts
  const download = async (f: FileMeta) => {
    try {
      // Blob собирает worker: основной поток байты не копирует.
````

на:

````ts
  const download = async (f: FileMeta) => {
    try {
      // Десктоп: копия родным диалогом (скачивание ссылкой в окне Tauri ненадёжно).
      if (props.files.disk?.saveCopy) {
        if (await props.files.disk.saveCopy(f.id, fileName(f.name, '.ferrum'))) ui.toast('Копия сохранена');
        return;
      }
      // Blob собирает worker: основной поток байты не копирует.
````

Run: `cd apps/web && npm run typecheck && npm run unit && npm run build`
Expected: PASS — четыре теста `tauri.test.ts`; сборка `dist/` готова для десктопа.

- [ ] **Step 3: Приложение Tauri**

`apps/desktop/src-tauri/Cargo.toml` (полностью — workspace Task 11 и пакет приложения):

````toml
# Десктоп Ferrum на Tauri 2 (этап 6): оборачивает сборку apps/web. Свой workspace —
# не часть корневого: Tauri тянет сотни крейтов и системные библиотеки окна, а
# `cargo test --workspace` ядра не должен их собирать.
[workspace]
members = [".", "store"]
resolver = "2"

[package]
name = "ferrum-desktop"
version = "0.1.0"
edition = "2021"
rust-version = "1.96"
license = "UNLICENSED"

[lib]
name = "ferrum_desktop"
crate-type = ["rlib"]

[build-dependencies]
tauri-build = { version = "=2.7.1", features = [] }

[dependencies]
ferrum-desktop-store = { path = "store" }
tauri = { version = "=2.12.1", features = [] }
tauri-plugin-dialog = "=2.8.1"
serde = { version = "=1.0.229", features = ["derive"] }
serde_json = "=1.0.152"

[features]
# Сборка с ресурсами из frontendDist (так собирает `tauri build`); без неё debug-сборка
# ждала бы dev-сервер.
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
````

`apps/desktop/src-tauri/build.rs`:

````rust
fn main() {
    tauri_build::build()
}
````

`apps/desktop/src-tauri/tauri.conf.json`:

````json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Ferrum",
  "version": "0.1.0",
  "identifier": "app.ferrum.desktop",
  "build": {
    "frontendDist": "../../web/dist"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "Ferrum",
        "width": 1440,
        "height": 900,
        "minWidth": 960,
        "minHeight": 600
      }
    ],
    "security": {
      "csp": null,
      "headers": {
        "Cross-Origin-Opener-Policy": "same-origin",
        "Cross-Origin-Embedder-Policy": "require-corp"
      }
    }
  },
  "bundle": {
    "active": true,
    "targets": [
      "app"
    ],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
````

`apps/desktop/src-tauri/capabilities/default.json`:

````json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Окно редактора: только IPC ядра Tauri; свои команды приложения разрешены без списка.",
  "windows": ["main"],
  "permissions": ["core:default"]
}
````

`apps/desktop/src-tauri/src/main.rs`:

````rust
// Без окна консоли в Windows-сборке.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ferrum_desktop::run();
}
````

`apps/desktop/src-tauri/src/lib.rs`:

````rust
//! Десктоп Ferrum (этап 6, спека 10): окно с интерфейсом `apps/web` и команды IPC
//! хранилища на диске (`ferrum-desktop-store`): список файлов, чтение и атомарная
//! запись, журнал, превью, настройки в файле конфигурации, родные диалоги «Открыть»
//! и «Сохранить». Байты идут сырым телом IPC (без JSON): файл — `Uint8Array` в
//! запросе и `ArrayBuffer` в ответе; параметры записи — заголовком `x-ferrum`.
//!
//! Интерфейс не доверенный: id — только UUID из библиотеки, пути — только из родных
//! диалогов (интерфейс их не передаёт), имена пачек журнала — только своего вида.
//!
//! Проверка запуска (`FERRUM_SMOKE=1`): интерфейс открывает новый файл, дожидается
//! ядра и сообщает итог командой `smoke_done`; приложение печатает его и выходит
//! (код 0 — ядро запустилось), без ответа за 90 с — выход с кодом 1.

use std::sync::{Mutex, MutexGuard};

use ferrum_desktop_store::{FileMeta, Library, ListResult, Section, Settings};
use serde::Deserialize;
use serde_json::{Map, Value};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

struct Desk {
    lib: Mutex<Library>,
    settings: Mutex<Settings>,
}

type Res<T> = Result<T, String>;

const BAD: &str = "Неверный запрос к хранилищу файлов";

impl Desk {
    fn lib(&self) -> MutexGuard<'_, Library> {
        self.lib.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Сырое тело запроса и параметры из заголовка `x-ferrum` (JSON только из ASCII:
/// id — UUID, числа).
fn raw<T: for<'de> Deserialize<'de>>(request: &Request<'_>) -> Res<(Vec<u8>, T)> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(BAD.into());
    };
    let args = request
        .headers()
        .get("x-ferrum")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| BAD.to_string())?;
    let args = serde_json::from_str(args).map_err(|_| BAD.to_string())?;
    Ok((bytes.clone(), args))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteArgs {
    id: String,
    base: f64,
    session: Option<u64>,
    up_to: Option<u64>,
}

#[derive(Deserialize)]
struct ThumbArgs {
    id: String,
}

#[derive(Deserialize)]
struct JournalArgs {
    id: String,
    session: u64,
    batch: u64,
}

#[tauri::command]
fn files_list(
    section: Section,
    query: String,
    start: usize,
    count: usize,
    desk: State<'_, Desk>,
) -> ListResult {
    desk.lib().list(section, &query, start, count)
}

#[tauri::command]
fn files_get(id: String, desk: State<'_, Desk>) -> Option<FileMeta> {
    desk.lib().get(&id)
}

#[tauri::command]
fn files_create(name: String, desk: State<'_, Desk>) -> Res<FileMeta> {
    desk.lib().create(&name)
}

/// Байты файла; пустой ответ — новый файл без байтов.
#[tauri::command]
fn files_read(id: String, desk: State<'_, Desk>) -> Res<Response> {
    Ok(Response::new(desk.lib().read(&id)?.unwrap_or_default()))
}

#[tauri::command]
fn files_write(request: Request<'_>, desk: State<'_, Desk>) -> Res<FileMeta> {
    let (bytes, a): (Vec<u8>, WriteArgs) = raw(&request)?;
    let journal = a.session.zip(a.up_to);
    desk.lib().write(&a.id, &bytes, a.base, journal)
}

/// Превью: пустое тело — убрать.
#[tauri::command]
fn files_write_thumb(request: Request<'_>, desk: State<'_, Desk>) -> Res<FileMeta> {
    let (bytes, a): (Vec<u8>, ThumbArgs) = raw(&request)?;
    desk.lib()
        .write_thumb(&a.id, (!bytes.is_empty()).then_some(&bytes[..]))
}

#[tauri::command]
fn files_thumb(id: String, desk: State<'_, Desk>) -> Response {
    Response::new(desk.lib().thumb(&id).unwrap_or_default())
}

#[tauri::command]
fn files_journal_append(request: Request<'_>, desk: State<'_, Desk>) -> Res<()> {
    let (bytes, a): (Vec<u8>, JournalArgs) = raw(&request)?;
    desk.lib().journal_append(&a.id, a.session, a.batch, &bytes)
}

#[tauri::command]
fn files_journal_names(id: String, desk: State<'_, Desk>) -> Res<Vec<String>> {
    desk.lib().journal_names(&id)
}

#[tauri::command]
fn files_journal_read(id: String, name: String, desk: State<'_, Desk>) -> Res<Response> {
    Ok(Response::new(desk.lib().journal_read(&id, &name)?))
}

#[tauri::command]
fn files_rename(id: String, name: String, desk: State<'_, Desk>) -> Res<FileMeta> {
    desk.lib().rename(&id, &name)
}

#[tauri::command]
fn files_touch(id: String, desk: State<'_, Desk>) -> Res<FileMeta> {
    desk.lib().touch(&id)
}

#[tauri::command]
fn files_trash(id: String, on: bool, desk: State<'_, Desk>) -> Res<()> {
    desk.lib().trash(&id, on)
}

#[tauri::command]
fn files_delete(id: String, desk: State<'_, Desk>) -> Res<()> {
    desk.lib().delete(&id)
}

#[tauri::command]
fn files_empty_trash(desk: State<'_, Desk>) -> Res<()> {
    desk.lib().empty_trash()
}

/// «Открыть с диска…»: родной диалог; запись для выбранного файла. `None` — отказ.
#[tauri::command]
async fn files_open_dialog(app: AppHandle, desk: State<'_, Desk>) -> Res<Option<FileMeta>> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Файл Ferrum", &["ferrum"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    desk.lib().add_existing(&path).map(Some)
}

/// «Сохранить на диск…»: файл записи переезжает по выбранному пути.
#[tauri::command]
async fn files_save_dialog(
    id: String,
    name: String,
    app: AppHandle,
    desk: State<'_, Desk>,
) -> Res<Option<FileMeta>> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_file_name(&name)
        .add_filter("Файл Ferrum", &["ferrum"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    desk.lib().move_to(&id, &path).map(Some)
}

/// «Сохранить копию на диск…»: копия по выбранному пути; `false` — отказ.
#[tauri::command]
async fn files_copy_dialog(
    id: String,
    name: String,
    app: AppHandle,
    desk: State<'_, Desk>,
) -> Res<bool> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_file_name(&name)
        .add_filter("Файл Ferrum", &["ferrum"])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    desk.lib().copy_to(&id, &path).map(|()| true)
}

#[tauri::command]
fn settings_get(desk: State<'_, Desk>) -> Map<String, Value> {
    desk.settings
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .all()
}

#[tauri::command]
fn settings_set(patch: Map<String, Value>, desk: State<'_, Desk>) -> Res<()> {
    desk.settings
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .set(patch)
}

fn smoke() -> bool {
    std::env::var("FERRUM_SMOKE").is_ok_and(|v| v == "1")
}

#[tauri::command]
fn smoke_mode() -> bool {
    smoke()
}

#[tauri::command]
fn smoke_done(report: Value, app: AppHandle) {
    println!("smoke: {report}");
    let ok = report.get("ready").and_then(Value::as_bool) == Some(true);
    app.exit(if ok { 0 } else { 1 });
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Проверка запуска — во временном каталоге: библиотека пользователя не трогается.
            let (data, docs, config) = if smoke() {
                let tmp = std::env::temp_dir().join(format!("ferrum-smoke-{}", std::process::id()));
                (tmp.join("data"), tmp.join("docs"), tmp.join("config"))
            } else {
                let p = app.path();
                (
                    p.app_data_dir()?,
                    p.document_dir()?.join("Ferrum"),
                    p.app_config_dir()?,
                )
            };
            let (lib, warnings) = Library::open(&data, &docs)?;
            for w in warnings {
                eprintln!("{w}");
            }
            let settings = Settings::open(&config.join("settings.json"))?;
            app.manage(Desk {
                lib: Mutex::new(lib),
                settings: Mutex::new(settings),
            });
            if smoke() {
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_secs(90));
                    println!("smoke: timeout");
                    std::process::exit(1);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            files_list,
            files_get,
            files_create,
            files_read,
            files_write,
            files_write_thumb,
            files_thumb,
            files_journal_append,
            files_journal_names,
            files_journal_read,
            files_rename,
            files_touch,
            files_trash,
            files_delete,
            files_empty_trash,
            files_open_dialog,
            files_save_dialog,
            files_copy_dialog,
            settings_get,
            settings_set,
            smoke_mode,
            smoke_done,
        ])
        .run(tauri::generate_context!())
        .expect("Ferrum не запустился");
}
````

`apps/desktop/package.json`:

````json
{
  "name": "ferrum-desktop",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "icon": "node scripts/make-icon.mjs",
    "tauri": "tauri"
  },
  "devDependencies": {
    "@tauri-apps/cli": "2.12.1"
  }
}
````

`apps/desktop/.npmrc`:

````
save-exact=true
# Пакеты не новее этой даты — как у apps/web.
before=2026-10-10
````

`apps/desktop/scripts/make-icon.mjs`:

````js
// Значок приложения до своего набора: квадрат цвета `oxide` дизайн-системы с тёмной
// рамкой фрейма — PNG 1024 × 1024 RGBA (Tauri требует RGBA). Пишется без зависимостей
// (zlib из Node). Остальные размеры и .icns делает `npx tauri icon` из этого файла.
import { writeFileSync, mkdirSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const SIZE = 1024;
const OXIDE = [0xf2, 0x81, 0x4a, 0xff];
const INK = [0x1a, 0x19, 0x18, 0xff];
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../src-tauri/icons/icon.png');

const raw = Buffer.alloc((SIZE * 4 + 1) * SIZE);
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0; // фильтр строки: нет
  for (let x = 0; x < SIZE; x++) {
    // Рамка фрейма: полоса 48 px на отступе 240 px от края.
    const inFrame = x >= 240 && x < SIZE - 240 && y >= 240 && y < SIZE - 240;
    const inHole = x >= 288 && x < SIZE - 288 && y >= 288 && y < SIZE - 288;
    const c = inFrame && !inHole ? INK : OXIDE;
    raw.set(c, y * (SIZE * 4 + 1) + 1 + x * 4);
  }
}

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
};
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // бит на канал
ihdr[9] = 6; // RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk('IHDR', ihdr),
  chunk('IDAT', deflateSync(raw)),
  chunk('IEND', Buffer.alloc(0)),
]);
mkdirSync(path.dirname(out), { recursive: true });
writeFileSync(out, png);
console.log(`значок: ${out}`);
````

Значки: исходный PNG и набор для сборки `.app` (из одного PNG 1024 × 1024 сборщик `.icns` не делает — «No matching IconType», проверено на прототипе).

Run: `cd apps/desktop && npm install && npm run icon && npx tauri icon src-tauri/icons/icon.png -o src-tauri/icons && rm -rf src-tauri/icons/android src-tauri/icons/ios src-tauri/icons/Square*.png src-tauri/icons/StoreLogo.png src-tauri/icons/64x64.png && ls src-tauri/icons`
Expected: `npm install` ставит `@tauri-apps/cli 2.12.1` (с датой из `.npmrc`); в `icons/` — `128x128.png 128x128@2x.png 32x32.png icon.icns icon.ico icon.png`.

Run: `cd apps/desktop/src-tauri && CARGO_TARGET_DIR=$PWD/target cargo generate-lockfile && cargo update -p tauri-plugin-fs --precise 2.6.0`
Expected: `Cargo.lock` десктопа; `tauri-plugin-fs` 2.7.1 → 2.6.0 (2.7.x опубликованы после 2026-10-10). Остальные транзитивные версии — какие даст cargo (у cargo нет ограничения по дате; политика — для прямых зависимостей, как в корне).

- [ ] **Step 4: Собрать и проверить запуск**

Run: `cd apps/web && npm run wasm && npm run build`
Expected: `dist/` (её встраивает `generate_context!`).

Run: `cd apps/desktop/src-tauri && CARGO_TARGET_DIR=$PWD/target cargo build && cargo clippy --all-targets -- -D warnings && cargo test -p ferrum-desktop-store && cargo fmt --check`
Expected: сборка без ошибок (прототип: первая сборка ≈ 1 мин на M1 Pro после загрузки крейтов), без предупреждений, `9 passed`.

Run: `cd apps/desktop/src-tauri && FERRUM_SMOKE=1 ./target/debug/ferrum-desktop`
Expected: окно на пару секунд, строка `smoke: {"files":1,"isolated":false,"ms":…,"ready":true}`, код 0.

Run: `cd apps/desktop/src-tauri && CARGO_TARGET_DIR=$PWD/target npx tauri build --bundles app && FERRUM_SMOKE=1 ./target/release/bundle/macos/Ferrum.app/Contents/MacOS/ferrum-desktop`
Expected: `Finished 1 bundle at: …/Ferrum.app` (прототип: 11,4 МБ), затем `smoke: {… "ready":true}`, код 0. Подпись и нотаризация не делаются (локальная сборка).

- [ ] **Step 5: Закоммитить**

```bash
git add apps/desktop apps/web/src apps/web/unit
git commit -m "feat(desktop): приложение Tauri 2 — интерфейс apps/web, хранилище на диске командами IPC, родные диалоги, проверка запуска

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: E2E: круг «нарисовать → сохранить → перезагрузить → открыть → сравнить» и Firefox

Спека 12 откладывала на этап 6 Firefox и сценарий «нарисовать фрейм → сохранить → перезагрузить → открыть → сравнить». Сравнение — не снимком экрана (камера после открытия другая, оверлей наведения зависит от указателя), а тем, что от них не зависит: строки дерева слоёв (стабильные id, имена, типы, вложенность), сводка свойств выделенного фрейма и картинка документа без оверлеев — превью первой страницы, которое ядро рисует заново после открытия; PNG должен совпасть байт в байт.

Проект Playwright `firefox` запускает тесты с меткой `@firefox`: круг сохранения, журнал с восстановлением после перезагрузки (Task 5) и «без File System Access пунктов нет» (Task 10; в Firefox API и вправду нет). Проект подключается, только если Firefox для Playwright установлен (`npx playwright install firefox`, один раз, ≈ 100 МБ из сети): без него прогон не падает. В Firefox нет WebGPU и `Atomics.waitAsync` (тогда кадры будят сообщения, этап 4), программный WebGL2 при автовыборе отклоняется — рисует `vello_cpu`. `npm run e2e:ci` (CI на Linux) Firefox не запускает — в CI его нет.

**Files:**
- Test: `apps/web/e2e/roundtrip.spec.ts`
- Modify: `apps/web/playwright.config.ts`
- Modify: `apps/web/e2e/fsa.spec.ts` (метка `@firefox` у теста без API)

**Interfaces:**
- Consumes: helpers `testMode`, `editorReady`, `fileId`, `drawRect` (Task 4), команды `expandLayer`, `select`, `snapshot`.
- Produces: проект `firefox` (`grep: /@firefox/`).

- [ ] **Step 1: Круг сохранения**

`apps/web/e2e/roundtrip.spec.ts`:

````ts
// Круг сохранения (спека 12, этап 6): нарисовать фрейм → сохранить → перезагрузить →
// открыть → сравнить. Сравниваются строки дерева слоёв (id, имена, типы, вложенность),
// сводка свойств выделенного фрейма и картинка документа без оверлеев — превью первой
// страницы, которое ядро рисует заново после открытия (PNG байт в байт). Идёт и в Firefox.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, drawRect, editorReady, fileId, live, liveWhen, testMode } from './helpers';

/** Всё видимое о документе: строки слоёв (фрейм раскрыт), сводка фрейма, превью. */
async function look(page: Page) {
  const frame = await page.evaluate(() => window.__ferrum!.ui!.layers.rows.find((r) => r.kind === 'frame')!.id);
  await page.evaluate((id) => {
    window.__ferrum!.core!.send({ type: 'expandLayer', id, expanded: true });
    window.__ferrum!.core!.send({ type: 'select', ids: [id], mode: 'replace' });
  }, frame);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(2);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.summary?.count)).toBe(1);
  const thumb = await page.evaluate(
    () =>
      new Promise<number[]>((resolve) => {
        const core = window.__ferrum!.core!;
        const off = core.on((e) => {
          if (e.type !== 'snapshot') return;
          off();
          resolve(Array.from(e.thumbnail ?? []));
        });
        core.send({ type: 'snapshot', thumbnail: true });
      }),
  );
  return page.evaluate(
    (thumb) => ({
      rows: window.__ferrum!.ui!.layers.rows.map((r) => ({ id: r.id, name: r.name, kind: r.kind, depth: r.depth })),
      summary: JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)) as unknown,
      thumb,
    }),
    thumb,
  );
}

test('нарисовать фрейм → сохранить → перезагрузить → открыть → сравнить @firefox', async ({ page }) => {
  await testMode(page);
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  const id = fileId(page);
  // Фрейм инструментом F, в нём — прямоугольник (ребёнок фрейма).
  await page.locator('canvas').click({ position: { x: 10, y: 10 } });
  await page.keyboard.press('KeyF');
  const l = await live(page);
  await drag(page, await appScreen(page, l, l.cameraX - 150, l.cameraY - 100), 300, 200);
  await liveWhen(page, 'l.nodes === 1');
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  const before = await look(page);
  expect(before.rows.map((r) => [r.kind, r.depth])).toEqual([
    ['frame', 0],
    ['rect', 1],
  ]);
  expect(before.thumb.length).toBeGreaterThan(100);
  await page.reload();
  await editorReady(page);
  expect(fileId(page)).toBe(id);
  expect(await look(page)).toEqual(before);
});
````

Run: `cd apps/web && npx playwright test e2e/roundtrip.spec.ts --project=chromium`
Expected: PASS, `1 passed` (тест проверяет уже сделанное в Tasks 4–5; падает, только если круг сохранения сломан).

- [ ] **Step 2: Проект Firefox**

В `apps/web/playwright.config.ts` заменить:

````ts
import { defineConfig } from '@playwright/test';
````

на:

````ts
import { existsSync } from 'node:fs';
import { defineConfig, firefox } from '@playwright/test';

// Firefox (этап 6): тесты с меткой @firefox — круг сохранения, журнал, отсутствие
// File System Access. Проект есть, только если браузер установлен
// (`npx playwright install firefox`): без него прогон не падает. В CI его нет.
const FIREFOX = existsSync(firefox.executablePath());
````

В `apps/web/playwright.config.ts` заменить:

````ts
    {
      name: 'gpu',
      use: { browserName: 'chromium', launchOptions: { args: GPU_ARGS } },
      grep: /@gpu|@perf/,
    },
  ],
````

на:

````ts
    {
      name: 'gpu',
      use: { browserName: 'chromium', launchOptions: { args: GPU_ARGS } },
      grep: /@gpu|@perf/,
    },
    ...(FIREFOX ? [{ name: 'firefox', use: { browserName: 'firefox' as const }, grep: /@firefox/ }] : []),
  ],
````

В `apps/web/e2e/fsa.spec.ts` заменить:

````ts
test('без File System Access пунктов «с диска» и «на диск» нет', async ({ page }) => {
````

на:

````ts
test('без File System Access пунктов «с диска» и «на диск» нет @firefox', async ({ page }) => {
````

- [ ] **Step 3: Проверить в Firefox**

Run: `cd apps/web && npx playwright install firefox && npx playwright test --project=firefox`
Expected: PASS, `3 passed` — круг сохранения, журнал (Task 5, первый тест) и «без File System Access» в Firefox. Установка — однократная загрузка браузера Playwright из сети.

Run: `cd apps/web && npm run e2e:ci`
Expected: PASS — проект `chromium` без `@perf` (Firefox в CI не входит).

- [ ] **Step 4: Закоммитить**

```bash
git add apps/web/e2e apps/web/playwright.config.ts
git commit -m "test(web): круг сохранения — нарисовать, сохранить, перезагрузить, открыть, сравнить; проект Firefox для @firefox

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: Спека и финальная проверка

**Files:**
- Modify: `docs/superpowers/specs/2026-10-23-ferrum-core-design.md`

**Interfaces:**
- Consumes: всё этапа 6 (Tasks 1–13).
- Produces: спека, описывающая сделанное, и список отложенного.

- [ ] **Step 1: Спека — сделанное в части 2 этапа 6**

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
| Строк дерева слоёв в DOM | ≤ окна (54 при 900 px) | 46 |

## 4. Архитектура
````

на:

````markdown
| Строк дерева слоёв в DOM | ≤ окна (54 при 900 px) | 46 |

Журнал изменений (этап 6) проверен тестом `journal_perf` (нативно, release, Apple M1 Pro) на той же странице из 100 000 узлов: пачка журнала после сдвига 1000 узлов — 0,33 мс и 16 КБ (полный снимок — ≈ 0,45 с в worker'е); открытие файла с журналом — 0,98 с (зеркало страницы строится заново из Loro), цель — до 3 с.

## 4. Архитектура
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
- **Отдельный воркер ввода-вывода** для файлов: кодирование `.ferrum`, полное сохранение (до секунд на больших файлах), журнал автосохранения, работа с IndexedDB и File System Access. Воркер ядра передаёт ему снапшоты transferable-буферами и не блокируется на записи; холст не замирает во время сохранения.
````

на:

````markdown
- **Отдельный воркер ввода-вывода** для файлов: IndexedDB (записи, байты, превью, журнал изменений, настройки пользователя, дескрипторы файлов на диске) и чтение и запись файлов File System Access. Снимки и пачки журнала ядро отдаёт событиями, интерфейс передаёт их буферы worker'у ввода-вывода без копий. Исключение — десктоп: IPC Tauri есть только у окна, поэтому байты идут в Rust из основного потока тем же `ArrayBuffer` сырым телом запроса, а запись и `fsync` — в потоке Tauri.
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
- **Адресат правки из панели**
````

на:

````markdown
- **Журнал и восстановление** (этап 6): `FlushJournal` → `Journal { batch, bytes? }` (без байтов — правок с прошлой пачки нет); пачка уходит и перед каждым `Snapshot`, поле которого `journal` — номер последней вошедшей пачки. `OpenWithJournal { bytes, journal }` применяет пачки поверх файла; применено хоть что-то — `Recovered { batches }` перед `Opened`, документ изменён. Пачек ≤ 100 000, файл вместе с журналом ≤ 512 МБ (проверяют обе стороны). Потеря GPU: `SurfaceLost { backend }` (worker создаёт поверхность заново: `WasmSurface.create` → `replaceSurface`), `SurfaceRestored`, `SurfaceFailed { next }` (интерфейс перезапускает ядро с бэкендом `next`). Тестовая паника `testPanic` принимается только в тестовом режиме (`init.test`).
- **Адресат правки из панели**
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
- Настройки хранятся у пользователя (IndexedDB в браузере, файл конфигурации в Tauri), не в документе; раскладка и настройки переживают открытие другого документа. До хранилища этапа 6 — `localStorage` (`ferrum.settings.v1`): тема, привязка к пикселям, отличия раскладки, ширина панелей; неверные поля — умолчания поодиночке.
````

на:

````markdown
- Настройки хранятся у пользователя, не в документе: в браузере — IndexedDB (хранилище `settings`, по ключу на поле: тема, привязка к пикселям, отличия раскладки, ширина панелей, раскрытие страниц, вид экрана файлов), в Tauri — файл конфигурации `settings.json`. Вкладка пишет только изменённые поля, другие вкладки получают их через `BroadcastChannel('ferrum-settings')` и применяют тему, привязку и раскладку на лету — настройки вкладок не затирают друг друга. Значения не доверенные: неверное поле — умолчание. При первом запуске этапа 6 прежняя запись `localStorage` (`ferrum.settings.v1`) переносится и удаляется; тему до первого рисования `theme-init.js` берёт из зеркала `localStorage` `ferrum.theme`. Файл раскладки несёт платформу (`"platform": "mac" | "windows" | "linux"`): импорт с другой группы платформ меняет Ctrl и Cmd местами с одним предупреждением.
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
Другая вкладка с экраном файлов видит изменения через `BroadcastChannel('ferrum-files')`.
````

на:

````markdown
Другая вкладка с экраном файлов видит изменения через `BroadcastChannel('ferrum-files')`. Где есть File System Access (Chrome, Edge) и в десктопе — «Открыть с диска…» рядом с «Импорт…» и в меню файла, там же «Сохранить на диск…»; карточка файла на диске — «На диске · Изменён …»; файл на диске без доступа (браузер перезапущен) открывается после вопроса «Разрешить доступ к файлу на диске?».
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
- **Сохранение (часть 1, без журнала):**
````

на:

````markdown
- **База (часть 2):** версии 2–4 добавляют хранилища `journal` (`[id, сеанс, пачка]` → пачка журнала), `settings` (поле → значение) и `handles` (id → дескриптор файла на диске и его `lastModified`); хранилища создаются по `contains`, записи версии 1 читаются без переноса.
- **Журнал и автосохранение (часть 2):** правки уходят в журнал не позже 2 с после первой правки (правки подряд пачку не откладывают — после сбоя теряется не больше 2 с правок): ядро отдаёт пачку обновлений Loro за миллисекунды, worker ввода-вывода дописывает её своей транзакцией. Полный файл — через 30 с после последней правки, не реже раза в 5 минут при работе без пауз, по Ctrl/Cmd+S и при уходе; перед снимком ядро отдаёт пачку с правками до него, и она пишется раньше файла, а запись файла стирает пачки до номера снимка в той же транзакции. Новый файл без байтов (журнал не к чему применить) до первой записи сохраняется полным файлом через 2 с. Скрыли вкладку — сразу пачка; `pagehide` — пачка и попытка полного файла. «Изменён» значит «не в файле».
- **Восстановление (часть 2):** при открытии пачки журнала, записанные после файла, применяются поверх него («Восстановлены изменения, не записанные в файл», документ изменён до следующего полного файла). Повреждённая пачка пропускается целиком, правки без предыдущих не применяются — одно предупреждение. Файл вместе с журналом больше 512 МБ открывается без журнала с уведомлением.
- **File System Access (часть 2, Chrome и Edge):** запись хранилища, связанная с файлом на диске: метаданные, превью и журнал — в IndexedDB, байты — в файле; запись — `createWritable` (браузер подменяет файл целиком при `close`); файл, изменённый другой программой (`lastModified`), — отказ «Файл на диске изменён другой программой…»; нет файла — «Файл на диске не найден…»; доступ после перезапуска браузера — вопрос и `requestPermission` по щелчку. «Удалить навсегда» файл на диске не трогает. Где API нет, пунктов нет.
- **Tauri (часть 2, `apps/desktop`):** хранилище на Rust (`ferrum-desktop-store`): библиотека `library.json` в каталоге данных, новые файлы — в `~/Documents/Ferrum`, открытые с диска остаются на месте; запись атомарная (временный файл → `fsync` → переименование → `fsync` каталога); чужая запись — по поколению и времени изменения файла; журнал — файлы `journal/<id>/<сеанс>-<пачка>.bin`; превью; настройки в `settings.json`. Родные диалоги «Открыть с диска…», «Сохранить на диск…», «Сохранить копию на диск…» — из Rust (пути интерфейс не видит). В WKWebView страница не изолирована (`crossOriginIsolated === false`) — ядро работает на транспорте postMessage.
- **Сохранение (часть 1, без журнала; заменено частью 2):**
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
- **Следующие части этапа 6:** автосохранение инкрементальными обновлениями Loro в журнал (миллисекунды) вместо полного снимка; полное сохранение (shallow-снапшоты и кэш сцены, ~0.8 с на 100 000 узлов) — периодически в простое и при закрытии; журнал обновлений Loro (IndexedDB или файл рядом) пишется до основного файла, после сбоя изменения восстанавливаются при следующем открытии; File System Access API; Tauri (запись атомарная: временный файл → переименование).
````

на:

````markdown
- **Отложено после этапа 6:** быстрое открытие из кэша сцены (первый кадр до раскодирования Loro, правки — после фоновой загрузки): редактор держит загруженную страницу во всех путях (выделение, наведение, панели), а открытие 100 000 узлов уже укладывается в цель (≈ 0,95 с в worker'е при цели 2 с); обновление зеркала страницы по событиям Loro вместо полной перестройки при восстановлении (подсистема 4); SharedArrayBuffer в десктопе (WKWebView не даёт изоляции); вид после перезапуска редактора (страница, камера, выделение); экспорт PNG в десктопе родным диалогом; подпись, нотаризация, сборки Windows и Linux, автообновление и меню приложения десктопа; E2E в Safari/WebKit и Firefox в CI.
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
- Повреждённый файл: открывается всё, что прочитано, с понятным сообщением о потерянном (например, «Файл повреждён: не удалось прочитать секцию изображений. Документ открыт без 2 картинок»).
- Паника в Rust перехватывается (`console_error_panic_hook`), worker перезапускается, документ восстанавливается из журнала; интерфейс сообщает «Редактор перезапущен, изменения сохранены».
- Потеря GPU-устройства: пересоздание устройства и текстур, при неудаче — переход на запасной рендер. Этап 4 только обнаруживает потерю устройства WebGPU и контекста WebGL2 и ошибки WebGPU вне вызовов и сообщает о них событием `error`; пересоздание — этап 6.
````

на:

````markdown
- Повреждённый файл: открывается всё, что прочитано, с понятным сообщением о потерянном: «Файл повреждён: не удалось прочитать секцию изображений. Документ открыт без 2 картинок» (число — картинки, на которые ссылаются страницы, в нужном падеже), «Файл повреждён: не удалось прочитать страницу «Черновики». Она открыта пустой», «Файл обрезан: часть данных в конце не прочитана»; без корневого документа файл не открывается. Повреждённый журнал — «Не все несохранённые изменения удалось восстановить: журнал изменений повреждён».
- Паника в Rust перехватывается: свой обработчик пишет текст и стек в консоль (`console_error_panic_hook`) и отдаёт текст worker'у до остановки wasm («Ядро остановлено: паника в ядре: …»). После сбоя ядра (паника, исключение в worker'е) тот же файл открывается в новом редакторе с новым холстом, документ — из файла и журнала; уведомление «Редактор перезапущен, изменения сохранены» или, если правки последних секунд не успели в журнал, «Редактор перезапущен. Правки последних секунд не сохранились». Не больше двух перезапусков за минуту и не во время открытия файла — иначе остаётся плашка остановки.
- Потеря GPU-устройства: worker создаёт поверхность того же бэкенда заново на том же холсте (новое устройство WebGPU или восстановленный браузером контекст WebGL2; новый атлас и растры), попытки — сразу и через 1 с, удалось — «Видеокарта перезапущена». Не удалось или поверхность теряется третий раз за жизнь ядра — запасной бэкенд (WebGPU → WebGL2 → CPU): перезапуск редактора с ним (контекст холста выбирается один раз), правки перед этим уходят в журнал; «Видеокарта недоступна — редактор перезапущен с рендером …». Потерянный, не восстановленный контекст WebGL2 рендереру vello не отдаётся (на нём он падает).
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
- Сбой worker'а ядра: одно сообщение «Ядро остановлено: …» с причиной; цикл кадров и пересылка ввода останавливаются, команды получают ошибку «Ядро остановлено, команда не выполнена». Интерфейс показывает остановку постоянной плашкой поверх холста.
````

на:

````markdown
- Сбой worker'а ядра: одно сообщение «Ядро остановлено: …» с причиной; цикл кадров и пересылка ввода останавливаются, команды получают ошибку «Ядро остановлено, команда не выполнена». Редактор перезапускается (выше); когда перезапуски исчерпаны, интерфейс показывает остановку постоянной плашкой поверх холста.
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
Firefox и сценарий «нарисовать фрейм → сохранить → перезагрузить → открыть → сравнить» — этап 6.
````

на:

````markdown
Этап 6: круг «нарисовать фрейм → сохранить → перезагрузить → открыть → сравнить» (строки слоёв, свойства, превью байт в байт), журнал (правка в журнале раньше файла, восстановление после перезагрузки, полный файл в простое), сбой ядра (настоящая паника в тестовом режиме, перезапуск, предел), потеря GPU (заглушки пролога worker'а: потеря и возврат контекста WebGL2, потеря устройства WebGPU, запасной CPU с перезапуском редактора), File System Access (файлы OPFS вместо системных диалогов), настройки двух вкладок. Проект `firefox` (тесты `@firefox`: круг сохранения, журнал, отсутствие File System Access) — локально, когда Firefox для Playwright установлен; в CI — нет. Хранилище десктопа — `cargo test` крейта `ferrum-desktop-store`; приложение — проверка запуска `FERRUM_SMOKE=1` (ядро готово в WKWebView, IPC хранилища работает).
````

- [ ] **Step 2: Полная проверка**

Все ворота этапов 1–6.

Run: `cargo fmt --check`
Expected: без вывода, код 0.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: без предупреждений.

Run: `cargo clippy -p ferrum-render --all-targets --features gpu -- -D warnings`
Expected: без предупреждений.

Run: `cargo clippy --target wasm32-unknown-unknown -p ferrum-wasm --lib --tests -- -D warnings`
Expected: без предупреждений.

Run: `cargo test --workspace`
Expected: PASS — 548 тестов в 64 наборах (на копии со всеми задачами плана).

Run: `cargo test --target wasm32-unknown-unknown -p ferrum-wasm --test web`
Expected: PASS, `10 passed` (обработчик паники вызывает `__ferrumPanic`, только если он есть: в Node его нет).

Run: `cargo bench --workspace --no-run`
Expected: сборка бенчмарков без ошибок.

Run: `cargo run --release -p ferrum-render --example render_report && cargo run --release -p ferrum-editor --example editor_report && cargo run --release -p ferrum-doc --example bench_report`
Expected: все строки PASS, код 0.

Run: `cargo test --release -p ferrum-wasm --test journal_perf --test snapshot_perf -- --nocapture`
Expected: PASS; на копии: пачка 0,38 мс (16 КБ), открытие с журналом 1,02 с, превью 35 мс.

Run: `cd apps/web && npm run typecheck && npm run unit && npm run wasm && npm run build && npx playwright test`
Expected: PASS — модульные 87 passed; E2E `chromium` и `gpu` — 282 passed, 8 skipped (проектные пропуски), плюс `firefox`, если установлен. Замер `@perf` под нагрузкой машины изредка ловит одну длинную задачу при старте (на прогоне копии — 65 мс у `webgpu`, два повтора прошли): при провале повторить, ворота не ослаблять.

Run: `cd apps/web && npm run e2e:ci`
Expected: PASS.

Run: `cd apps/desktop/src-tauri && CARGO_TARGET_DIR=$PWD/target cargo test -p ferrum-desktop-store && cargo clippy --all-targets -- -D warnings && cargo fmt --check && cargo build && FERRUM_SMOKE=1 ./target/debug/ferrum-desktop`
Expected: `9 passed`; без предупреждений; `smoke: {… "ready":true}`, код 0.

- [ ] **Step 3: Что проверяет пользователь вручную**

Автотесты не открывают системные окна и не ломают настоящую видеокарту. Перед слиянием — руками (по 1–2 минуты):
1. Chrome: «Открыть с диска…» и «Сохранить на диск…» с настоящим системным диалогом; правка, Ctrl+S, файл на диске изменился (Finder); перезапуск Chrome — вопрос о доступе и «Разрешить».
2. Десктоп (`apps/desktop/src-tauri/target/release/bundle/macos/Ferrum.app`): новый файл появляется в `~/Documents/Ferrum` после первой правки; «Открыть с диска…» и «Сохранить на диск…» — родные диалоги; «Сохранить копию на диск…»; перезапуск приложения — «Недавние» на месте; экспорт PNG (скачивание в окне Tauri может не работать — тогда это отложенный пункт «экспорт PNG родным диалогом»).
3. Firefox (обычный, не Playwright): редактор открывается, правка переживает перезагрузку вкладки.

- [ ] **Step 4: Закоммитить**

```bash
git add docs/superpowers/specs/2026-10-23-ferrum-core-design.md
git commit -m "docs: спека — журнал и восстановление, перезапуск ядра, потеря GPU, повреждённый файл, File System Access, настройки в IndexedDB, десктоп Tauri; отложенное

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Что этап 6 сознательно не делает

- **Быстрое открытие из кэша сцены** (перенос со этапа 5; спека 5.3: первый кадр по таблице до раскодирования Loro, правки — после фоновой загрузки). Редактор держит загруженную страницу во всех путях (выделение, наведение, панели, транзакции — `doc.page()`), и открытие без Loro меняет их все; при этом открытие 100 000 узлов уже ≈ 0,95 с в worker'е при цели 2 с (спека 3). Отдельный этап.
- **Обновление зеркала страницы по событиям Loro.** Восстановление из журнала перестраивает зеркало страницы целиком (≈ 1 с на 100 000 узлах — только после сбоя); пообъектное обновление — вместе с импортом чужих правок (подсистема 4).
- **SharedArrayBuffer в десктопе.** WKWebView не делает страницу `tauri://` изолированной — ядро работает на транспорте postMessage (этап 4; бюджет основного потока он держит).
- **Вид после перезапуска.** Перезапущенный редактор открывает файл на первой странице с камерой по умолчанию, без прежнего выделения.
- **Восстановление файла, ни разу не записанного полностью.** Журнал применяется к байтам файла; новый файл получает их через 2 с после первой правки — правки до этого сбой не переживают.
- **Десктоп:** экспорт PNG родным диалогом, подпись и нотаризация macOS, сборки Windows и Linux, автообновление, меню приложения и «Недавние» в меню Dock. «Удалить навсегда» удаляет свой файл сразу, не в Корзину macOS.
- **E2E в Safari/WebKit и Firefox в CI.** Firefox — локально (`@firefox`), WebKit — ручная проверка десктопа.
