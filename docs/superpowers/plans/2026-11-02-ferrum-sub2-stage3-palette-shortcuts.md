# Ferrum, подсистема 2, этап 3: быстрые действия и панель сочетаний — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Палитра «Быстрые действия» (Ctrl/Cmd+K и Ctrl/Cmd+/, как в Figma): поле сверху по центру холста и один список — недавние, команды с группой, сочетанием и серыми недоступными, слои текущей страницы (поиск в worker'е, ≤ 30 мс на 100 000 узлов; выбор выделяет слой, раскрывает предков и показывает его в виде) и страницы; поиск нечёткий, без учёта регистра, «ё» и раскладки; недавние команды — в настройках. Панель «Сочетания клавиш» (Ctrl+Shift+? и «Справка → Сочетания клавиш»): поднимается снизу окна, вкладки по группам, клавиши из действующей раскладки, использованные недавно отмечены, ссылка «Изменить в настройках», окно не модальное.

**Architecture:** Модель документа и формат буфера не меняются. Редактор (`ferrum-editor`): `find.rs` — `LayerIndex` (сложенные имена страницы одним массивом до сдвига `(generation, structure_stamp)` сцены), нечёткий счёт и `Editor::{find_layers, reveal_layer}`; команда интерфейса `ui.quick_actions` и сочетание `ui.shortcuts` в таблице. Ядро (`ferrum-wasm`): `FindLayers { query, limit }` → `LayersFound { query, items }` (не ждёт шрифтов), `RevealLayer { id }`. Интерфейс: `fuzzy.ts` — тот же счёт, что в ядре, для команд и страниц в основном потоке (их ≈ 90); `palette.ts` — чистая функция состава списка; `quick-actions.tsx` — комбобокс внутри `.fe-stage`, положение задаёт CSS, прокрутка к строке читает раскладку только в `measure`, ответы `LayersFound` идут через очередь планировщика и отбрасываются по `query`; `shortcuts-panel.tsx` — панель на тех же `KeymapEntry`, что вкладка настроек, без новых данных в протоколе. Недавние — поле настроек `recentCommands` (строка ключей: файл настроек десктопа принимает только простые значения, `diffSettings` сравнивает по `!==`).

**Tech Stack:** Rust 1.96 (`rust-toolchain.toml`, новых зависимостей нет), SolidJS 1.9.17, TypeScript 6.0.3, @playwright/test 1.64.0 (`chromium`, `firefox`); `package-lock.json` и `Cargo.lock` не меняются.

**Спеки:** `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` — разделы 7, 8, 11, 12 (этап 3); спека ядра `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` (протокол — §4.3, тонкий основной поток — §4.2, команды и раскладка — §7.6).

## Global Constraints

- Ветка `feat/sub2-palette` от `main` с этапами 1–2 (bcd547e). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Каталог сборки — у каждой копии свой: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0` (в `apps/web` — `"$PWD/../../target"`, в `apps/desktop/src-tauri` — `"$PWD/../../../target"`); команды — из корня копии, если не сказано иное.
- Новых зависимостей нет ни в Rust, ни в npm; модель документа и формат буфера не меняются.
- Основной поток тонкий (ядро §4.2): слои ищет и переход выполняет ядро в worker'е; основной поток ищет только по командам и страницам (их мало) и рисует не больше 50 строк. Чтения layout — только в фазе `measure` планировщика (положение палитры и панели задаёт CSS; единственное чтение — прокрутка к выбранной строке). E2E проверяют `__ferrumWarnings` — пусто.
- Поведение — как в Figma; где спека молчит, решение помечено **(поведение)** (раздел «Решения»).
- Бюджеты (`editor_report`, нативно, release): поиск слоёв на 100 000 узлах — ≤ 30 мс на запрос, включая первый после правки имён; пересчёт `CommandStates` после шага на 100 000 узлах при одном выделенном — ≤ 1 мс (новые команды не должны его ухудшить).
- Числа команд таблицы зашиты в трёх тестах — `crates/ferrum-editor/tests/keymap.rs`, `crates/ferrum-wasm/tests/boolean.rs`, `apps/web/e2e/app.spec.ts`: задача 2 обновляет все три (85 → 86).
- Поля настроек перечислены дважды — `SETTINGS_FIELDS` в `apps/web/src/core/file-list.ts` и в `apps/desktop/src-tauri/store/src/settings.rs`: задача 4 правит оба.
- Тесты не проходят вхолостую: редактор — порядок имён, шаги истории и камера после перехода; ядро — события протокола с их `query`, строки панели слоёв и прокрутка; модульные — чистые функции (`fuzzyScore`, `paletteRows`, `parseRecent`, `clipQuery`); E2E — роли и `aria-*`, геометрия окна, выделение и вид после перехода, настройки после перезагрузки.
- Порт 4173 на машине проверки занят посторонним процессом: его не трогать, E2E — через временный конфиг Playwright с другими портами (не коммитить).
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Сначала прототип всего этапа собран в отдельной копии (`plan-verify-s2s3`, ветка `verify/sub2-s3` от bcd547e, по коммиту на задачу; вершина — 5dc663f, код — bf5a114) и прошёл ворота на вершине кода (нагрузка машины 7–15 от других процессов): `cargo fmt --check`, `clippy -D warnings` (нативно и `wasm32-unknown-unknown`), `cargo test --workspace` (1055 passed, 2 ignored), `render_report` (FAIL нет), `editor_report` (release; прогон 1 — одна строка FAIL: «Кривой вектор 100 000 точек: перетаскивание точки при ×8» 16,784 мс при цели ≤ 16,7 мс, код векторов этап не трогает; прогон 2 — 94 строки PASS, FAIL нет; «Поиск слоёв, 100 000 узлов»: первый запрос после правки имён 9,1 мс, следующие 1,1 мс при цели ≤ 30 мс; «Состояния команд после шага, 100 000 узлов, выделен 1» 0,001 мс при цели ≤ 1 мс), wasm-сборка, `npm run typecheck`, модульные (166), сборка, E2E `chromium` целиком (`--grep-invert @perf --retries=1`: 372 passed, 1 flaky — `picker.spec` «обводка: ширина жестом…», прошла на повторе), Firefox — новые и затронутые спеки (`palette`, `shortcuts-panel`, `main-menu`, `app`, `settings-store`, `context-menu`: 52 passed, 1 skipped — `@perf` палитры только в Chromium), `palette.spec` `@perf` в Chromium (20 000 узлов, ввод и 30 стрелок: без длинных задач, `__ferrumWarnings` пуст), десктоп — `cargo check` и тесты `store` (25 passed, на a119193). На промежуточных коммитах гонялись только свои тесты, clippy редактора и ядра (задачи 1–2), typecheck и затронутые спеки; полные ворота — только на вершине кода.

Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом, который читает сам этот документ, к чистой копии от bcd547e (`plan-apply-s2s3`, ветка `verify/sub2-s3-apply`) в порядке задач с коммитом после каждой: каждый фрагмент «заменить» найден ровно один раз (операций по задачам: 19, 13, 10, 18, 8, 14, 5). После каждой задачи дерево копии совпало с деревом соответствующего коммита прототипа (тот же хэш дерева у всех семи), `git diff 5dc663f HEAD` — пустой. Команды шагов «Убедиться, что тесты падают» и «Проверить» на чистой копии не повторялись: ожидаемые результаты шагов «Проверить» — из прогона прототипа на том же коде (для промежуточных задач — выборочно, см. выше; числа новых тестов — по самим тестовым файлам), ожидаемые падения — по тому, каких элементов API и поведения тестам задачи не хватает до её реализации, и прогоном не подтверждены.

Порт 4173 на машине проверки занят посторонним процессом, поэтому E2E прототипа шли через временный конфиг Playwright с портами 4283/4284 (и временной правкой портов в `core.spec.ts` и `settings.spec.ts`, откатываемой после каждой команды).

Найденное прототипом и вошедшее в шаги: `Bind` добавляет сочетание, а не заменяет (тест панели ждёт `['R', 'Q']`, задача 6); Playwright не щёлкает элемент с `aria-disabled` без `force: true` — тест серой строки зависал на таймауте 120 с (задача 5); без правила «весь текст без пробелов» запрос `слой 5` давал `Слой 59` раньше `Слой 5` (задачи 1, 3). Не запускались: проект Playwright `gpu`, `@perf` кроме палитры, Safari/WebKit, запуск десктопа, замеры в wasm и на 100 000 узлах в браузере (B-148), `scripts/compat-old-build.sh` (модель документа и формат буфера не менялись), ручная проверка (только снимки экрана палитры и панели); заявления про Figma по справке не сверялись.

---

## Этапы подсистемы 2

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Состояние команд и главное меню | описание команд в ядре, состояние «доступна / отмечена», главное меню с подменю и поиском, компонент меню с подменю и первой буквой | 3, 4 |
| 2. Контекстные меню и вставка | меню холста по выделению, меню слоёв и страниц; свойства, вставка с заменой и поверх, умный дубликат, «Выделить подобные» | 5, 6 |
| **3. Быстрые действия и сочетания** (этот план) | палитра Ctrl/Cmd+K и Ctrl/Cmd+/: команды, слои, страницы, недавние; панель сочетаний клавиш | 7, 8 |
| 4. Линейки, направляющие и полировка панелей | линейки, направляющие с привязкой, сворачиваемые секции, уведомления с действием, страницы | 9, 10 |

## Решения

Где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**. Заявления про Figma по справке не сверялись: Ctrl+Shift+? и Ctrl/Cmd+K, / взяты из спеки; состав и вид панели, поведение перехода к слою — решения по памяти о Figma (B-140 просит сверить недавние).

Поиск:

1. Счёт один в Rust (`find.rs`) и TS (`fuzzy.ts`): буквы запроса встречаются в тексте по порядку; совпадение 1, начало слова +8, буква сразу за прошлой +5, иначе минус пропуск (не больше 3), начало текста +6, иначе минус сдвиг (не больше 3), весь текст без пробелов +10; пробелы запроса не учитываются; лучшее из трёх выравниваний (первые вхождения, начала слов, подстрока); текст — первые 256 символов. Порога веса нет (B-149). Без правила «весь текст без пробелов» запрос `слой 5` давал `Слой 59` раньше `Слой 5`.
2. Первая версия поиска складывала имена на каждый запрос: 24 мс на 100 000 узлов при цели 30 мс, главная цена — `to_lowercase` и обход узлов. Поэтому `LayerIndex`: один массив символов до сдвига `(generation, structure_stamp)`; у кириллицы и латиницы своя быстрая ветка свёртки. Плата — память (B-142).
3. Лимиты: `query` длиннее 256 байт — `Error` без ответа; `limit` больше 200 — урезается; интерфейс обрезает запрос по границе символа. `FindLayers` не ждёт шрифтов: документ не меняет. Отмены выполняющегося поиска нет: команды worker'а идут по очереди, интерфейс отбрасывает устаревший ответ по `query`.
4. **(поведение)** `RevealLayer`: виден целиком — вид прежний; влезает при нынешнем масштабе — центр на слое; не влезает — вписан (`FIT_PADDING_PX`); скрытый — только выделение. Предки раскрываются через обычный путь выделения «не из панели». Не шаг истории.

Палитра:

5. **(поведение)** `ui.quick_actions` — группа «Вид», в главном меню пункта нет (поле поиска меню делает то же), исключение в тесте `main_menu`.
6. **(поведение)** Состав списка: пустой запрос — «Недавние» и «Команды»; запрос — «Команды» до 20, «Слои» в остаток, «Страницы» до 10, всего 50 строк с действиями. Нет удержания руки, Esc, сдвигов и самой палитры; файла на диске без File System Access — тоже.
7. **(поведение)** Вес синонима −4, недавность +3 за каждое место до восьмого.
8. **(поведение)** Серая команда: стрелками можно, Enter и щелчок не выполняют, палитра открыта. Новый список выбирает первую доступную. То же сочетание в поле закрывает палитру. Закрытие по потере фокуса окна не сделано (запрос сохраняется при переключении окон), щелчок мимо закрывает.
9. **(поведение)** Поиск главного меню теперь тоже нечёткий (спека §4), без недавности.
10. **(поведение)** Недавние — только из палитры и меню (`noteRecent`), не сочетания и кнопки панелей (B-140). Отдельно за сеанс (в настройки не пишется) копятся команды, вызванные сочетанием: отметка панели = недавние ∪ сеанс (B-141).
11. `recentCommands` — строка, а не массив: файл настроек десктопа принимает только простые значения, `diffSettings` сравнивает по `!==`.

Панель:

12. **(поведение)** Панель: на всю ширину, 320 px, не больше половины окна, 160 мс; не модальная; команды без сочетания остаются с прочерком; Esc из панели и то же сочетание закрывают; фокус возвращается, только если был в панели. «Изменить в настройках» открывает диалог на вкладке «Горячие клавиши» (она и так первая, отдельного свойства у диалога нет).
13. `ui.shortcuts` — `Ctrl+Shift+Slash`, Ctrl и на маке; подпись — физической клавишей (`⌃⇧/`, B-144). `Bind` добавляет сочетание, а не заменяет (тест панели ждёт `['R', 'Q']`).

Протокол: `FindLayers { query, limit }` → `LayersFound { query, items: [{ id, name, kind }] }`; `RevealLayer { id }` (неверный формат и слоя нет на текущей странице — `Error`).

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-editor/src/find.rs` (новый), `editor.rs`, `lib.rs` | нечёткий счёт, `LayerIndex`, `find_layers`, `reveal_layer` |
| `crates/ferrum-editor/src/{commands,actions,command_state}.rs` | `ui.quick_actions`, сочетание `ui.shortcuts` |
| `crates/ferrum-wasm/src/protocol.rs`, `core.rs`, `core/panels.rs` | `FindLayers`, `RevealLayer`, `LayersFound`, `FoundLayerDto` |
| `apps/web/src/ui/fuzzy.ts`, `palette.ts` (новые), `main-menu.ts` | счёт в TS, состав списка, недавние, поиск меню через `rankCommands` |
| `apps/web/src/ui/settings.ts`, `app.tsx`, `context.ts`, `menu.tsx`, `src/core/file-list.ts`; `apps/desktop/src-tauri/store/src/settings.rs` | поле `recentCommands`, `Editor.noteRecent` |
| `apps/web/src/ui/quick-actions.tsx` (новый), `layers.tsx`, `styles/chrome.css` | окно палитры, `KIND_ICON` наружу |
| `apps/web/src/ui/shortcuts-panel.tsx` (новый), `boot.ts`, `app.tsx`, `styles/chrome.css` | панель сочетаний, `BootOptions.onForwardedKey`, отметка использованных |
| `crates/ferrum-editor/tests/find_layers.rs`, `crates/ferrum-wasm/tests/find_layers.rs`, `apps/web/unit/{fuzzy,palette}.test.ts`, `apps/web/e2e/{palette,shortcuts-panel}.spec.ts` (новые) | тесты |
| `crates/ferrum-editor/examples/editor_report.rs`, спека, `docs/backlog.md` | замер поиска слоёв; «Сделано (этап 3)», B-140…B-149 |

---

### Task 1: Поиск слоёв и переход к слою в ядре

Спека §7, §11: `FindLayers { query, limit: 50 }` → `LayersFound`, на 100 000 узлах ≤ 30 мс. Здесь — `find.rs`: свёртка символов (регистр, «ё»), варианты запроса в обеих раскладках, счёт по трём выравниваниям, `LayerIndex` со сложенными именами страницы, обход сверху вниз как в панели; `Editor::find_layers` и `Editor::reveal_layer`; команды протокола и событие с тем же `query`; `FindLayers` не ждёт шрифтов; строки замера в `editor_report`.

**Files:**
- Create: `crates/ferrum-editor/src/find.rs`
- Modify: `crates/ferrum-editor/src/{editor,lib}.rs`, `crates/ferrum-editor/examples/editor_report.rs`, `crates/ferrum-wasm/src/{protocol,core}.rs`, `crates/ferrum-wasm/src/core/panels.rs`
- Test: `crates/ferrum-editor/tests/find_layers.rs` (новый), `crates/ferrum-wasm/tests/find_layers.rs` (новый), `crates/ferrum-wasm/tests/protocol.rs`

**Interfaces:**
- Consumes: `Scene::{generation, structure_stamp, roots, at, node_at, is_draft_slot, slot}`, `Editor::select`, `geometry::world_box`, `Camera::{visible_world_rect, viewport, zoom, set_center, fit}`, `FIT_PADDING_PX`, `waits_behind_fonts`, `NodeType`, `Report::{gate_median, check}`.
- Produces: `pub struct FoundLayer { id, name, kind }`; `Editor::find_layers(&self, query: &str, limit: usize) -> Vec<FoundLayer>`; `Editor::reveal_layer(&mut self, id: NodeId) -> bool`; `LayerIndex::find(&mut self, scene: &Scene, query: &str, limit: usize) -> Vec<FoundLayer>`; `fuzzy_score(text: &str, query: &str) -> Option<i32>`, `find::{layout_variants, fuzzy_score_folded}`; `MAX_FIND_QUERY_BYTES = 256`, `MAX_FIND_LIMIT = 200`, `FIND_NAME_CHARS = 256`. Протокол: `Command::FindLayers { query: String, limit: u32 }`, `Command::RevealLayer { id: String }`, `CoreEvent::LayersFound { query: String, items: Vec<FoundLayerDto> }`, `FoundLayerDto { id, name, kind: NodeType }`. Строки `editor_report` «Поиск слоёв, 100 000 узлов, …» (`find_layers(&mut report)`).

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/find_layers.rs`:

````rust
//! Поиск слоёв и переход к слою для «Быстрых действий» (спека подсистемы 2,
//! раздел 7): нечёткий поиск по подпоследовательности с весом, без учёта
//! регистра, «ё» и раскладки; переход выделяет слой и показывает его в виде.

mod common;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn named(h: &mut H, parent: Option<NodeId>, name: &str, x: f64, y: f64) -> NodeId {
    let id = h.rect(parent, x, y, 40.0, 40.0);
    h.set(
        id,
        PropsPatch {
            name: Some(name.into()),
            ..Default::default()
        },
    );
    id
}

fn names(h: &H, query: &str, limit: usize) -> Vec<String> {
    h.ed.find_layers(query, limit)
        .into_iter()
        .map(|f| f.name)
        .collect()
}

#[test]
fn subsequence_with_word_starts_first() {
    assert!(fuzzy_score("Кнопка отправить", "кно").is_some());
    assert!(
        fuzzy_score("Кнопка отправить", "котп").is_some(),
        "подпоследовательность"
    );
    assert_eq!(fuzzy_score("Кнопка", "кнопки"), None);
    assert_eq!(fuzzy_score("Кнопка", ""), None);
    assert_eq!(fuzzy_score("Кнопка", "   "), None);
    // Начало слова весит больше, чем буквы в середине.
    let start = fuzzy_score("Order summary", "su").unwrap();
    let middle = fuzzy_score("Ordersum", "su").unwrap();
    assert!(start > middle, "{start} > {middle}");
    // Буквы подряд весят больше, чем вразброс.
    let run = fuzzy_score("Карточка товара", "кар").unwrap();
    let spread = fuzzy_score("Кнопка в ряд", "кар").unwrap();
    assert!(run > spread, "{run} > {spread}");
    // Имя целиком — выше, чем его начало в длинном имени.
    assert!(fuzzy_score("Фон", "фон").unwrap() > fuzzy_score("Фон карточки", "фон").unwrap());
    // И с пробелом: «Слой 5» целиком выше «Слой 59».
    assert!(fuzzy_score("Слой 5", "слой 5").unwrap() > fuzzy_score("Слой 59", "слой 5").unwrap());
}

#[test]
fn case_yo_and_keyboard_layout_do_not_matter() {
    assert_eq!(fuzzy_score("ЁЛКА", "елка"), fuzzy_score("ёлка", "ЕЛКА"));
    assert!(fuzzy_score("Ёлка", "елк").is_some());
    // «щквук» набрано в русской раскладке вместо «order».
    assert_eq!(fuzzy_score("Order", "щквук"), fuzzy_score("Order", "order"));
    // И наоборот: «afqk» — это «файл».
    assert!(fuzzy_score("Файл", "afqk").is_some());
    // Пробелы запроса не учитываются.
    assert!(fuzzy_score("Шапка сайта", "шап сай").is_some());
}

#[test]
fn finds_on_the_current_page_best_first_then_panel_order() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    named(&mut h, Some(f), "Кнопка", 10.0, 10.0);
    named(&mut h, Some(f), "Иконка", 60.0, 10.0);
    named(&mut h, None, "Кнопка отправки", 500.0, 0.0);
    let bg = named(&mut h, None, "Фон", 600.0, 0.0);
    // «кнопка» целиком — первой; остальное с «кн…» — по порядку панели.
    assert_eq!(names(&h, "кнопка", 50), ["Кнопка", "Кнопка отправки"]);
    // «ка» с начала слова весит больше, чем в середине («Иконка» — последней).
    assert_eq!(names(&h, "ка", 50), ["Кнопка отправки", "Кнопка", "Иконка"]);
    // Равный вес — порядок панели: сверху вниз (верхний слой первым).
    assert_eq!(names(&h, "нопк", 50), ["Кнопка отправки", "Кнопка"]);
    // Раскладка: «ryjgrf» — «кнопка».
    assert_eq!(names(&h, "ryjgrf", 50)[0], "Кнопка");
    assert!(names(&h, "", 50).is_empty());
    assert!(names(&h, "zzz", 50).is_empty());
    // Имена сложены заранее, но переименование и удаление видны сразу.
    assert_eq!(names(&h, "фон", 50), ["Фон"]);
    h.ed.rename_node(bg, "Задник");
    assert!(names(&h, "фон", 50).is_empty());
    assert_eq!(names(&h, "задн", 50), ["Задник"]);
    h.ed.select(&[bg]);
    h.ed.execute(CommandId::Delete);
    assert!(names(&h, "задн", 50).is_empty());
    // Предел — сколько просили, но не больше MAX_FIND_LIMIT.
    assert_eq!(names(&h, "к", 2).len(), 2);
    for i in 0..(MAX_FIND_LIMIT + 10) {
        named(&mut h, None, &format!("Слой {i}"), 0.0, 500.0);
    }
    assert_eq!(h.ed.find_layers("слой", 10_000).len(), MAX_FIND_LIMIT);
    // Другая страница — свои слои.
    h.ed.create_page().unwrap();
    assert!(names(&h, "кнопка", 50).is_empty());
}

#[test]
fn long_names_are_clipped() {
    let mut h = H::new();
    let long = "я".repeat(FIND_NAME_CHARS * 4);
    named(&mut h, None, &long, 0.0, 0.0);
    let found = h.ed.find_layers("я", 5);
    assert_eq!(found[0].name.chars().count(), FIND_NAME_CHARS);
    assert_eq!(found[0].kind, NodeKind::Rect);
}

#[test]
fn reveal_selects_and_shows_the_layer() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    let near = named(&mut h, Some(f), "Рядом", 100.0, 100.0);
    let far = named(&mut h, None, "Далеко", 5000.0, 3000.0);
    let huge = h.add(
        None,
        NodeKind::Rect,
        patch(-20_000.0, 0.0, 10_000.0, 8000.0),
    );
    let before = h.ed.camera().clone();
    let steps = h.ed.step_count();

    // Виден целиком — вид прежний, слой выделен.
    assert!(h.ed.reveal_layer(near));
    assert_eq!(h.sel(), [near]);
    assert_eq!(h.ed.camera().center(), before.center());
    assert_eq!(h.ed.camera().zoom(), before.zoom());

    // Не виден, но влезает: масштаб прежний, центр — центр слоя.
    assert!(h.ed.reveal_layer(far));
    assert_eq!(h.sel(), [far]);
    assert_eq!(h.ed.camera().zoom(), before.zoom());
    let c = h.ed.camera().center();
    assert!(close(c.x, 5020.0) && close(c.y, 3020.0), "{c:?}");

    // Не влезает — вписан с отступом, как «Показать выделение».
    assert!(h.ed.reveal_layer(huge));
    assert!(h.ed.camera().zoom() < before.zoom());
    let view = h.ed.camera().visible_world_rect();
    assert!(view.x0 <= -20_000.0 && view.x1 >= -10_000.0, "{view:?}");

    // Выбор — не шаг истории.
    assert_eq!(h.ed.step_count(), steps);
    // Слоя нет на странице (удалён, на другой странице) — ничего не меняется.
    h.ed.select(&[near]);
    h.ed.execute(CommandId::Delete);
    let cam = h.ed.camera().clone();
    assert!(!h.ed.reveal_layer(near));
    assert!(h.sel().is_empty());
    assert_eq!(h.ed.camera().center(), cam.center());
    h.ed.create_page().unwrap();
    assert!(!h.ed.reveal_layer(far));
}
````

Создать `crates/ferrum-wasm/tests/find_layers.rs`:

````rust
//! Поиск слоёв и переход к слою в ядре worker'а (спека подсистемы 2, разделы 7
//! и 11): `FindLayers { query, limit }` → `LayersFound { query, items }` с
//! пределами запроса и числа; `RevealLayer { id }` выделяет слой, раскрывает
//! предков в панели и прокручивает её к строке.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::Platform;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;

const VIEW: Size = Size::new(800.0, 600.0);

fn demo(nodes: u32) -> Core {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    core.command(Command::Demo { nodes });
    core.take_events();
    core
}

fn found(core: &mut Core, query: &str, limit: u32) -> Vec<FoundLayerDto> {
    core.command(Command::FindLayers {
        query: query.into(),
        limit,
    });
    let ev = core.take_events();
    let mut answers = ev.iter().filter_map(|e| match e {
        CoreEvent::LayersFound { query: q, items } => Some((q.clone(), items.clone())),
        _ => None,
    });
    let (q, items) = answers.next().expect("ответ LayersFound");
    assert!(answers.next().is_none(), "один ответ на запрос");
    assert_eq!(q, query, "ответ несёт свой запрос");
    items
}

fn layer_rows(core: &mut Core) -> (u32, Vec<LayerRow>, Option<u32>) {
    core.command(Command::LayersWindow {
        start: 0,
        count: 200,
    });
    core.take_events()
        .into_iter()
        .rev()
        .find_map(|e| match e {
            CoreEvent::Layers {
                total,
                rows,
                reveal,
                ..
            } => Some((total, rows, reveal)),
            _ => None,
        })
        .expect("строки слоёв")
}

#[test]
fn find_layers_answers_with_its_query_best_first() {
    let mut core = demo(200);
    let items = found(&mut core, "экран 1", 50);
    assert_eq!(items[0].name, "Экран 1");
    assert_eq!(items[0].kind, NodeType::Frame);
    // Раскладка: «'rhfy» — «экран».
    let names: Vec<String> = found(&mut core, "'rhfy", 50)
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(names, ["Экран 1", "Экран 0"], "сверху вниз, как в панели");
    // Предел: сколько просили, но не больше 200.
    assert_eq!(found(&mut core, "слой", 7).len(), 7);
    assert_eq!(found(&mut core, "слой", 100_000).len(), 198);
    let mut big = demo(1000);
    assert_eq!(found(&mut big, "слой", u32::MAX).len(), 200);
    assert!(found(&mut core, "", 50).is_empty());
    assert!(found(&mut core, "нет такого", 50).is_empty());
}

#[test]
fn query_over_the_limit_is_an_error_without_an_answer() {
    let mut core = demo(100);
    // 129 букв кириллицы — 258 байт.
    core.command(Command::FindLayers {
        query: "я".repeat(129),
        limit: 50,
    });
    let ev = core.take_events();
    assert!(ev.iter().any(|e| matches!(e, CoreEvent::Error { .. })));
    assert!(!ev
        .iter()
        .any(|e| matches!(e, CoreEvent::LayersFound { .. })));
    // Ровно 256 байт — можно.
    assert!(found(&mut core, &"я".repeat(128), 50).is_empty());
}

#[test]
fn reveal_selects_expands_ancestors_and_scrolls_the_panel() {
    let mut core = demo(200);
    let (total, rows, _) = layer_rows(&mut core);
    assert_eq!(total, 2, "фреймы свёрнуты");
    // «Слой 5» второго фрейма («Экран 1» — верхний, первая строка).
    let target = found(&mut core, "слой 5", 1).remove(0);
    assert_eq!(target.name, "Слой 5");
    core.command(Command::RevealLayer {
        id: target.id.clone(),
    });
    let ev = core.take_events();
    assert!(ev.iter().any(
        |e| matches!(e, CoreEvent::Selection { ids, .. } if ids == std::slice::from_ref(&target.id))
    ));
    // Панель: предок раскрыт и прокручена к строке слоя.
    let (total, after, reveal) = ev
        .iter()
        .rev()
        .find_map(|e| match e {
            CoreEvent::Layers {
                total,
                rows,
                reveal,
                ..
            } => Some((*total, rows.clone(), *reveal)),
            _ => None,
        })
        .expect("строки слоёв после перехода");
    assert_eq!(total, 2 + 99);
    let at = reveal.expect("прокрутка к строке") as usize;
    assert_eq!(after[at].id, target.id);
    assert!(after[at].selected);
    assert!(rows.iter().any(|r| r.name == "Экран 1"));
    // Слой в виде.
    let b = core.editor().camera().visible_world_rect();
    let id = target.id.parse().unwrap();
    let n = core.editor().scene().node(id).unwrap();
    let world = n.world.transform_rect_bbox(n.local_bounds.unwrap());
    assert!(b.contains(world.origin()) && b.contains(world.origin() + world.size().to_vec2()));
}

#[test]
fn reveal_of_a_wrong_or_missing_layer_is_an_error() {
    let mut core = demo(100);
    for id in ["мусор", "999@999"] {
        core.command(Command::RevealLayer { id: id.into() });
        let ev = core.take_events();
        assert!(
            ev.iter().any(|e| matches!(e, CoreEvent::Error { .. })),
            "{id}: {ev:?}"
        );
        assert!(!ev.iter().any(|e| matches!(e, CoreEvent::Selection { .. })));
    }
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        Command::ExpandAllLayers { expanded: true },
        Command::DuplicatePage { id: "3@4".into() },
        Command::Snapshot { thumbnail: true },
        Command::MarkSaved { revision: 3.0 },
````

на:

````rust
        Command::ExpandAllLayers { expanded: true },
        Command::DuplicatePage { id: "3@4".into() },
        Command::FindLayers {
            query: "кнопка".into(),
            limit: 50,
        },
        Command::RevealLayer { id: "1@2".into() },
        Command::Snapshot { thumbnail: true },
        Command::MarkSaved { revision: 3.0 },
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
                },
            ],
        },
        CoreEvent::RequestImage,
````

на:

````rust
                },
            ],
        },
        CoreEvent::LayersFound {
            query: "кн".into(),
            items: vec![FoundLayerDto {
                id: "1@2".into(),
                name: "Кнопка".into(),
                kind: NodeType::Rect,
            }],
        },
        CoreEvent::RequestImage,
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
    assert_eq!(Backend::parse("vulkan"), None);
}
````

на:

````rust
    assert_eq!(Backend::parse("vulkan"), None);
}

#[test]
fn find_layers_field_names() {
    assert_eq!(
        serde_json::to_value(Command::FindLayers {
            query: "кн".into(),
            limit: 50
        })
        .unwrap(),
        json!({"type": "findLayers", "query": "кн", "limit": 50})
    );
    assert_eq!(
        serde_json::to_value(Command::RevealLayer { id: "1@2".into() }).unwrap(),
        json!({"type": "revealLayer", "id": "1@2"})
    );
    assert_eq!(
        serde_json::to_value(CoreEvent::LayersFound {
            query: "кн".into(),
            items: vec![FoundLayerDto {
                id: "1@2".into(),
                name: "Кнопка".into(),
                kind: NodeType::Frame,
            }],
        })
        .unwrap(),
        json!({"type": "layersFound", "query": "кн", "items": [{"id": "1@2", "name": "Кнопка", "kind": "frame"}]})
    );
    // Предел числа — целое без знака: отрицательное — ошибка разбора.
    assert!(serde_json::from_value::<Command>(
        json!({"type": "findLayers", "query": "", "limit": -1})
    )
    .is_err());
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test find_layers`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test find_layers --test protocol`
Expected: FAIL — не собирается: редактор — нет `Editor::{find_layers, reveal_layer}` и экспортов `ferrum_editor::{MAX_FIND_LIMIT, FIND_NAME_CHARS, …}`; ядро — нет `Command::{FindLayers, RevealLayer}`, `CoreEvent::LayersFound`, `FoundLayerDto`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
    command_states(&mut report);
    select_similar(&mut report);

    println!("| Метрика | Значение | Цель | Итог |");
````

на:

````rust
    command_states(&mut report);
    select_similar(&mut report);
    find_layers(&mut report);

    println!("| Метрика | Значение | Цель | Итог |");
````

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
        format!("{found}"),
        &format!("{}", NODES / 100),
    );
}
````

на:

````rust
        format!("{found}"),
        &format!("{}", NODES / 100),
    );
}

/// Поиск слоёв «Быстрых действий» (подсистема 2, этап 3): нечёткий запрос по
/// именам страницы из 100 000 узлов, лучшие 50. Первый запрос после правки имён
/// складывает имена всей страницы заново, следующие идут по готовому массиву.
/// Запросы — короткий (совпадает почти всё: отбор лучших), точное имя, в другой
/// раскладке и не совпадающий ни с чем. Цель — 30 мс на запрос.
fn find_layers(report: &mut Report) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids: Vec<NodeId> = doc.transact(page, vec![], |p| fill(p, NODES)).unwrap();
    let mut ed = Editor::new(doc, Some(page), VIEWPORT, 1.0, Platform::Mac).unwrap();
    let queries = ["с", "слой 42", "'rhfy 999", "кнопка отправки"];
    let (mut cold, mut warm) = (Vec::new(), Vec::new());
    let mut found = Vec::new();
    for run in 0..COMMIT_RUNS {
        // Правка имени — метка состава сцены сдвинулась, имена складываются заново.
        ed.rename_node(ids[1], &format!("Слой {run}"));
        ed.take_events();
        for (i, q) in queries.iter().enumerate() {
            let t = Instant::now();
            let items = ed.find_layers(q, 50);
            if i == 0 {
                cold.push(t.elapsed())
            } else {
                warm.push(t.elapsed())
            }
            found.push((*q, items));
        }
    }
    report.gate_median(
        "Поиск слоёв, 100 000 узлов, первый запрос после правки имён",
        &cold,
        30.0,
    );
    report.gate_median("Поиск слоёв, 100 000 узлов, следующие запросы", &warm, 30.0);
    let first = |q: &str| {
        found
            .iter()
            .find(|(fq, _)| *fq == q)
            .and_then(|(_, items)| items.first().map(|f| f.name.clone()))
            .unwrap_or_default()
    };
    report.check(
        "Поиск слоёв: точное имя и другая раскладка — первыми",
        first("слой 42") == "Слой 42" && first("'rhfy 999") == "Экран 999",
        format!("{} / {}", first("слой 42"), first("'rhfy 999")),
        "Слой 42 / Экран 999",
    );
}
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Раскладка правимого текста по ключу (узел, версия узла, поколение шрифтов).
    pub(crate) edit_layout_cache: RefCell<Option<EditLayout>>,
    /// Время ядра, мс (`set_time`): мигание курсора и склейка ввода.
    pub(crate) now_ms: f64,
````

на:

````rust
    /// Раскладка правимого текста по ключу (узел, версия узла, поколение шрифтов).
    pub(crate) edit_layout_cache: RefCell<Option<EditLayout>>,
    /// Сложенные имена страницы для поиска слоёв «Быстрых действий».
    pub(crate) find_index: RefCell<crate::find::LayerIndex>,
    /// Время ядра, мс (`set_time`): мигание курсора и склейка ввода.
    pub(crate) now_ms: f64,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            text_edit: None,
            edit_layout_cache: RefCell::new(None),
            now_ms: 0.0,
            text_hover: false,
````

на:

````rust
            text_edit: None,
            edit_layout_cache: RefCell::new(None),
            find_index: RefCell::default(),
            now_ms: 0.0,
            text_hover: false,
````

Создать `crates/ferrum-editor/src/find.rs`:

````rust
//! Поиск слоёв для «Быстрых действий» (спека подсистемы 2, раздел 7) и переход
//! к слою.
//!
//! Поиск нечёткий: буквы запроса должны встретиться в имени по порядку
//! (подпоследовательность), а вес растёт за начало слова, буквы подряд и начало
//! имени и падает за пропуски. Регистр и «ё» не важны, раскладка тоже: запрос
//! пробуется как набран и переведённым ЙЦУКЕН ↔ QWERTY («щквук» находит
//! «order»). Пробелы запроса не учитываются. Тот же счёт — у поиска команд в
//! интерфейсе (`apps/web/src/ui/fuzzy.ts`).
//!
//! Обход — вся текущая страница сверху вниз, как строки панели слоёв; имя
//! участвует первыми `FIND_NAME_CHARS` символами. Результат — лучшие `limit`
//! по весу, при равном весе — в порядке панели.

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};

use crate::actions::FIT_PADDING_PX;
use crate::editor::Editor;
use crate::geometry::world_box;

/// Наибольшая длина запроса поиска слоёв в байтах UTF-8 (`FindLayers.query`).
pub const MAX_FIND_QUERY_BYTES: usize = 256;
/// Наибольшее число найденных слоёв за запрос (`FindLayers.limit`).
pub const MAX_FIND_LIMIT: usize = 200;
/// Сколько первых символов имени участвует в поиске и уходит в результат: имя из
/// файла может быть мегабайтным.
pub const FIND_NAME_CHARS: usize = 256;

/// Найденный слой.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundLayer {
    pub id: NodeId,
    /// Имя — первые `FIND_NAME_CHARS` символов.
    pub name: String,
    pub kind: NodeKind,
}

const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбюё";

/// Символ для сравнения: нижний регистр, «ё» как «е». Латиница и кириллица —
/// без таблиц Unicode (`to_lowercase` на 100 000 имён — основная цена поиска).
fn fold(c: char, out: &mut Vec<char>) {
    match c {
        'a'..='z' | 'а'..='я' | '0'..='9' | ' ' => out.push(c),
        'A'..='Z' => out.push(c.to_ascii_lowercase()),
        'А'..='Я' => out.push(char::from_u32(c as u32 + 32).unwrap_or(c)),
        'ё' | 'Ё' => out.push('е'),
        _ if c.is_ascii() => out.push(c),
        _ => {
            for l in c.to_lowercase() {
                out.push(if l == 'ё' { 'е' } else { l });
            }
        }
    }
}

fn folded(s: &str, max_chars: usize, out: &mut Vec<char>) {
    out.clear();
    for c in s.chars().take(max_chars) {
        fold(c, out);
    }
}

/// Запрос в обеих раскладках: как набран, ЙЦУКЕН → QWERTY и обратно; без
/// пробелов, в нижнем регистре, «ё» как «е», без повторов. Пустой запрос — пусто.
pub fn layout_variants(query: &str) -> Vec<Vec<char>> {
    let mut base = Vec::new();
    for c in query.chars().filter(|c| !c.is_whitespace()) {
        for l in c.to_lowercase() {
            base.push(l);
        }
    }
    if base.is_empty() {
        return Vec::new();
    }
    let swap = |from: &str, to: &str| -> Vec<char> {
        let to: Vec<char> = to.chars().collect();
        base.iter()
            .map(|c| from.chars().position(|f| f == *c).map_or(*c, |i| to[i]))
            .collect()
    };
    let mut out: Vec<Vec<char>> = Vec::new();
    for v in [base.clone(), swap(EN, RU), swap(RU, EN)] {
        let v: Vec<char> = v
            .into_iter()
            .map(|c| if c == 'ё' { 'е' } else { c })
            .collect();
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

/// Начало слова: первый символ или после символа, который не буква и не цифра.
fn is_start(t: &[char], j: usize) -> bool {
    if j == 0 {
        return true;
    }
    let p = t[j - 1];
    let alnum = match p {
        'a'..='z' | '0'..='9' | 'а'..='я' => true,
        _ if p.is_ascii() => false,
        _ => p.is_alphanumeric(),
    };
    !alnum
}

/// Совпадение буквы — 1; начало слова — ещё 8; буква сразу за прошлой — ещё 5,
/// иначе минус пропуск (не больше 3); начало имени — ещё 6, иначе минус сдвиг
/// (не больше 3); имя целиком (без пробелов) — ещё 10.
const MATCH: i32 = 1;
const WORD_START: i32 = 8;
const CONSECUTIVE: i32 = 5;
const PREFIX: i32 = 6;
const MAX_GAP: usize = 3;
const EXACT: i32 = 10;

fn score_positions(t: &[char], pos: &[usize]) -> i32 {
    let mut s = 0;
    for (i, &j) in pos.iter().enumerate() {
        s += MATCH;
        if is_start(t, j) {
            s += WORD_START;
        }
        if i > 0 {
            let gap = j - pos[i - 1] - 1;
            if gap == 0 {
                s += CONSECUTIVE;
            } else {
                s -= gap.min(MAX_GAP) as i32;
            }
        }
    }
    match pos.first() {
        Some(0) => s += PREFIX,
        Some(&p) => s -= p.min(MAX_GAP) as i32,
        None => {}
    }
    // Совпали все буквы имени (пробелы запроса не учитываются — и имени тоже).
    if pos.len() == t.iter().filter(|c| !c.is_whitespace()).count() {
        s += EXACT;
    }
    s
}

/// Буквы запроса по порядку: каждая — первое вхождение после прошлой; с
/// `prefer_start` — сначала вхождение в начале слова, если оно есть.
fn align(t: &[char], q: &[char], prefer_start: bool, pos: &mut Vec<usize>) -> bool {
    pos.clear();
    let mut from = 0;
    for &qc in q {
        let start = if prefer_start {
            (from..t.len()).find(|&j| t[j] == qc && is_start(t, j))
        } else {
            None
        };
        let Some(j) = start.or_else(|| (from..t.len()).find(|&j| t[j] == qc)) else {
            return false;
        };
        pos.push(j);
        from = j + 1;
    }
    true
}

/// Запрос подстрокой: лучшее из вхождений (в начале слова — первое такое).
fn substring(t: &[char], q: &[char], pos: &mut Vec<usize>) -> bool {
    if q.len() > t.len() {
        return false;
    }
    let mut best = None;
    for i in 0..=t.len() - q.len() {
        if t[i..i + q.len()] == *q {
            if is_start(t, i) {
                best = Some(i);
                break;
            }
            best.get_or_insert(i);
        }
    }
    let Some(i) = best else { return false };
    pos.clear();
    pos.extend(i..i + q.len());
    true
}

/// Вес совпадения запроса (одного варианта, уже сложенного) с текстом (сложенным);
/// `None` — буквы запроса не встречаются по порядку.
pub fn fuzzy_score_folded(t: &[char], q: &[char], pos: &mut Vec<usize>) -> Option<i32> {
    if q.is_empty() || !align(t, q, false, pos) {
        return None;
    }
    let mut best = score_positions(t, pos);
    if align(t, q, true, pos) {
        best = best.max(score_positions(t, pos));
    }
    if substring(t, q, pos) {
        best = best.max(score_positions(t, pos));
    }
    Some(best)
}

/// Вес совпадения запроса с текстом по всем раскладкам; `None` — не совпало.
pub fn fuzzy_score(text: &str, query: &str) -> Option<i32> {
    let mut t = Vec::new();
    folded(text, FIND_NAME_CHARS, &mut t);
    let mut pos = Vec::new();
    layout_variants(query)
        .iter()
        .filter_map(|q| fuzzy_score_folded(&t, q, &mut pos))
        .max()
}

/// Сложенные имена страницы в порядке панели (сверху вниз) — одним массивом:
/// повторный запрос не трогает узлы сцены и не складывает имена заново. Живёт,
/// пока не сменились экземпляр сцены и её метка состава и имён.
#[derive(Debug, Default)]
pub struct LayerIndex {
    key: Option<(u64, u64)>,
    /// Слот, узел и диапазон его имени в `chars`.
    entries: Vec<(Slot, NodeId, u32, u32)>,
    chars: Vec<char>,
}

impl LayerIndex {
    fn sync(&mut self, scene: &Scene) {
        let key = (scene.generation(), scene.structure_stamp());
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.entries.clear();
        self.chars.clear();
        let mut name = Vec::new();
        let mut stack: Vec<Slot> = scene.roots().to_vec();
        while let Some(s) = stack.pop() {
            if scene.is_draft_slot(s) {
                continue;
            }
            let n = scene.at(s);
            folded(&n.props.name, FIND_NAME_CHARS, &mut name);
            let from = self.chars.len() as u32;
            self.chars.extend_from_slice(&name);
            self.entries.push((s, n.id, from, self.chars.len() as u32));
            // Дети — в прямом порядке: верхний (последний) снимается первым.
            stack.extend(n.children.iter().copied());
        }
    }

    /// Слои страницы `scene` по нечёткому запросу: не больше `limit` (и
    /// `MAX_FIND_LIMIT`), лучшие первыми, при равном весе — в порядке панели
    /// слоёв (сверху вниз). Пустой запрос — пусто. Черновики жеста не ищутся.
    pub fn find(&mut self, scene: &Scene, query: &str, limit: usize) -> Vec<FoundLayer> {
        let limit = limit.min(MAX_FIND_LIMIT);
        let variants = layout_variants(query);
        if variants.is_empty() || limit == 0 {
            return Vec::new();
        }
        self.sync(scene);
        // (вес, порядок в панели)
        let mut hits: Vec<(i32, u32)> = Vec::new();
        let mut pos = Vec::new();
        for (i, &(_, _, a, b)) in self.entries.iter().enumerate() {
            let t = &self.chars[a as usize..b as usize];
            let best = variants
                .iter()
                .filter_map(|q| fuzzy_score_folded(t, q, &mut pos))
                .max();
            if let Some(score) = best {
                hits.push((score, i as u32));
            }
        }
        let key = |h: &(i32, u32)| (std::cmp::Reverse(h.0), h.1);
        let mut out = Vec::new();
        // Черновики и узлы, чей слот уже занят другим, пропускаются: отбор — с запасом.
        while out.len() < limit && !hits.is_empty() {
            let want = (limit - out.len()).min(hits.len());
            if hits.len() > want {
                hits.select_nth_unstable_by_key(want - 1, key);
            }
            let mut head: Vec<(i32, u32)> = hits.drain(..want).collect();
            head.sort_unstable_by_key(key);
            for (_, i) in head {
                let (slot, id, _, _) = self.entries[i as usize];
                let Some(n) = scene.node_at(slot).filter(|n| n.id == id) else {
                    continue;
                };
                if scene.is_draft_slot(slot) {
                    continue;
                }
                out.push(FoundLayer {
                    id,
                    name: n.props.name.chars().take(FIND_NAME_CHARS).collect(),
                    kind: n.props.kind,
                });
            }
        }
        out
    }
}

impl Editor {
    /// Слои текущей страницы по нечёткому запросу (`LayerIndex::find`); сложенные
    /// имена страницы держатся между запросами.
    pub fn find_layers(&self, query: &str, limit: usize) -> Vec<FoundLayer> {
        self.find_index.borrow_mut().find(&self.scene, query, limit)
    }

    /// Переход к слою (выбор в «Быстрых действиях»): слой выделяется, как из
    /// панели, и показывается в виде — уже целиком виден: вид прежний; влезает
    /// при нынешнем масштабе: вид сдвигается к его центру; не влезает: вписывается,
    /// как «Показать выделение». У скрытого слоя (рамки нет) — только выделение.
    /// Слоя нет на странице — `false`, ничего не меняется.
    pub fn reveal_layer(&mut self, id: NodeId) -> bool {
        let Some(slot) = self.scene.slot(id) else {
            return false;
        };
        if self.scene.is_draft_slot(slot) {
            return false;
        }
        self.select(&[id]);
        if let Some(b) = world_box(&self.scene, slot) {
            let view = self.camera.visible_world_rect();
            let inside = b.x0 >= view.x0 && b.y0 >= view.y0 && b.x1 <= view.x1 && b.y1 <= view.y1;
            if !inside {
                let v = self.camera.viewport();
                let zoom = self.camera.zoom();
                let fits = b.width() * zoom <= v.width - 2.0 * FIT_PADDING_PX
                    && b.height() * zoom <= v.height - 2.0 * FIT_PADDING_PX;
                if fits {
                    self.camera.set_center(b.center());
                } else {
                    self.camera.fit(b, FIT_PADDING_PX);
                }
            }
        }
        self.emit();
        true
    }
}
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod context_menu;
mod editor;
mod flatten;
pub mod geometry;
````

на:

````rust
pub mod context_menu;
mod editor;
pub mod find;
mod flatten;
pub mod geometry;
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub use context_menu::{ContextTarget, MAX_SELECT_LAYER_ITEMS};
pub use editor::{Editor, EditorEvent, Settings, ThemeChoice, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
````

на:

````rust
pub use context_menu::{ContextTarget, MAX_SELECT_LAYER_ITEMS};
pub use editor::{Editor, EditorEvent, Settings, ThemeChoice, WHEEL_ZOOM_SPEED};
pub use find::{
    fuzzy_score, FoundLayer, LayerIndex, FIND_NAME_CHARS, MAX_FIND_LIMIT, MAX_FIND_QUERY_BYTES,
};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            | Command::ExpandAllLayers { .. }
            | Command::FilterLayers { .. }
            | Command::PrepareSearch
            | Command::HoverLayer { .. }
````

на:

````rust
            | Command::ExpandAllLayers { .. }
            | Command::FilterLayers { .. }
            | Command::FindLayers { .. }
            | Command::PrepareSearch
            | Command::HoverLayer { .. }
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
    browser_reserved, frame_presets, main_menu, BindError, CommandId, ContextTarget, DropTarget,
    KeymapError, MenuAction, MenuNode, NumField, PanelEdit, Phase, Shortcut, COMMANDS,
    MAX_NAME_CHARS,
};
use ferrum_render::{decode_png, export_area, ExportBudget, ExportError, SvgOptions};
````

на:

````rust
    browser_reserved, frame_presets, main_menu, BindError, CommandId, ContextTarget, DropTarget,
    KeymapError, MenuAction, MenuNode, NumField, PanelEdit, Phase, Shortcut, COMMANDS,
    MAX_FIND_QUERY_BYTES, MAX_NAME_CHARS,
};
use ferrum_render::{decode_png, export_area, ExportBudget, ExportError, SvgOptions};
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
use crate::protocol::{
    Align, Command, ConflictInfo, ContextTarget as TargetDto, CoreEvent, DropPosition, Edit,
    EditPhase, ExportFormat, Field, FillDto, KeymapEntry, MenuActionDto, MenuItemDto, PageInfo,
    PaintKind, PresetInfo, ScaleModeDto, SelectMode, StrokeDto, EXPORT_SCALES, MAX_TEXT_BYTES,
    SELECTION_IDS_MAX,
};
use crate::summary::{cap_of, join_of, summarize, MAX_PAINTS};
````

на:

````rust
use crate::protocol::{
    Align, Command, ConflictInfo, ContextTarget as TargetDto, CoreEvent, DropPosition, Edit,
    EditPhase, ExportFormat, Field, FillDto, FoundLayerDto, KeymapEntry, MenuActionDto,
    MenuItemDto, PageInfo, PaintKind, PresetInfo, ScaleModeDto, SelectMode, StrokeDto,
    EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{cap_of, join_of, summarize, MAX_PAINTS};
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
                }
            },
            Command::ContextMenu { target } => self.context_menu(target),
            Command::ExpandAllLayers { expanded } => {
````

на:

````rust
                }
            },
            Command::FindLayers { query, limit } => {
                if query.len() > MAX_FIND_QUERY_BYTES {
                    self.error(format!(
                        "Запрос поиска слоёв длиннее {MAX_FIND_QUERY_BYTES} байт"
                    ));
                    return;
                }
                let items = self
                    .editor
                    .find_layers(&query, limit as usize)
                    .into_iter()
                    .map(|f| FoundLayerDto {
                        id: f.id.to_string(),
                        name: f.name,
                        kind: f.kind.into(),
                    })
                    .collect();
                self.events.push(CoreEvent::LayersFound { query, items });
            }
            Command::RevealLayer { id } => {
                if let Some(id) = self.node(&id) {
                    // Выделение не из панели: предки раскрываются, панель прокручивается.
                    if !self.editor.reveal_layer(id) {
                        self.error("Слой не найден на текущей странице");
                    }
                }
            }
            Command::ContextMenu { target } => self.context_menu(target),
            Command::ExpandAllLayers { expanded } => {
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        target: ContextTarget,
    },
    /// Развернуть (`true`) или свернуть все слои страницы.
    ExpandAllLayers {
````

на:

````rust
        target: ContextTarget,
    },
    /// Поиск слоёв текущей страницы для «Быстрых действий» (спека подсистемы 2,
    /// раздел 7): нечёткий, без учёта регистра, «ё» и раскладки. Ответ —
    /// `LayersFound` с тем же `query`; `query` — не больше 256 байт, `limit` —
    /// не больше 200 (больше — урезается).
    FindLayers {
        query: String,
        limit: u32,
    },
    /// Переход к слою: выделить его, показать в виде и раскрыть предков в панели.
    RevealLayer {
        id: String,
    },
    /// Развернуть (`true`) или свернуть все слои страницы.
    ExpandAllLayers {
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
}

/// Состояние команды: доступна ли и (у переключателя) отмечена ли.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

на:

````rust
}

/// Найденный слой: идентификатор, имя (не длиннее 256 символов) и тип для иконки.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct FoundLayerDto {
    pub id: String,
    pub name: String,
    pub kind: NodeType,
}

/// Состояние команды: доступна ли и (у переключателя) отмечена ли.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        items: Vec<MenuItemDto>,
    },
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
    /// пунктирных направляющих.
````

на:

````rust
        items: Vec<MenuItemDto>,
    },
    /// Найденные слои (ответ на `FindLayers`): `query` — запрос, на который это
    /// ответ (интерфейс отбрасывает ответы на прежние запросы); лучшие первыми.
    LayersFound {
        query: String,
        items: Vec<FoundLayerDto>,
    },
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
    /// пунктирных направляющих.
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Поиск слоёв|Состояния команд после шага, 100"`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Expected: PASS: fmt и clippy (нативно и wasm32) без замечаний; `ferrum-editor` и `ferrum-wasm` без падений (новые: `ferrum-editor/tests/find_layers.rs` — 5 тестов, `ferrum-wasm/tests/find_layers.rs` — 4, `protocol.rs` — +1); `editor_report` — «Поиск слоёв, 100 000 узлов, первый запрос после правки имён» ≈ 9 мс и «следующие запросы» ≈ 1,1 мс (цель ≤ 30 мс) PASS, «Поиск слоёв: точное имя и другая раскладка — первыми» PASS, «Состояния команд после шага, 100 000 узлов, выделен 1» ≈ 0,001 мс (≤ 1 мс) PASS. Строки «перетаскивание точки» кривого вектора у границы 16,7 мс при загруженной машине могут дать FAIL — повторить прогон (код векторов этап не трогает). wasm собран, `npm run typecheck` без ошибок

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/examples/editor_report.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/find.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/tests/find_layers.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/find_layers.rs crates/ferrum-wasm/tests/protocol.rs
git commit -F - <<'EOF'
feat(editor,wasm): поиск слоёв и переход к слою для «Быстрых действий»

Ядро ищет слои текущей страницы нечётко (подпоследовательность с весом за
начало слова, буквы подряд и начало имени; регистр, «ё» и раскладка не
важны): `FindLayers { query, limit }` → `LayersFound { query, items }`,
запрос — не больше 256 байт, число — не больше 200. Сложенные имена
страницы держатся между запросами (`LayerIndex`) до правки состава или имён.
`RevealLayer { id }` выделяет слой, раскрывает предков в панели и
показывает слой в виде. Строки замера в `editor_report`.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 2: Команды «Быстрые действия» и сочетание «Сочетаний клавиш»

Спека §7, §8: `ui.quick_actions` — команда интерфейса, сочетание на холсте идёт через ядро (`UiCommand`); `ui.shortcuts` получает Ctrl+Shift+? (Ctrl и на маке). Команды всегда доступны. Пункта в главном меню у палитры нет. Интерфейс на команду пока не отвечает — окно в задаче 5, панель в задаче 6.

**Files:**
- Modify: `crates/ferrum-editor/src/{commands,actions,command_state}.rs`
- Test: `crates/ferrum-editor/tests/{keymap,main_menu,command_state,command_table}.rs`, `crates/ferrum-wasm/tests/boolean.rs`, `apps/web/e2e/app.spec.ts`

**Interfaces:**
- Consumes: таблица `COMMANDS`, `EditorEvent::UiCommand`, `CommandGroup::View`.
- Produces: `CommandId::UiQuickActions` (`ui.quick_actions`, «Быстрые действия…», группа `View`, `Mod+KeyK`, `Mod+Slash`); у `CommandId::UiShortcuts` — `Ctrl+Shift+Slash`; таблица — 86 команд.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 85, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 86, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `crates/ferrum-editor/tests/command_state.rs` заменить:

````rust
        C::PixelGrid,
        C::UiMainMenu,
        C::UiSettings,
    ] {
````

на:

````rust
        C::PixelGrid,
        C::UiMainMenu,
        C::UiQuickActions,
        C::UiSettings,
    ] {
````

В `crates/ferrum-editor/tests/command_table.rs` заменить:

````rust
}

#[test]
fn ui_commands_keep_vector_editing() {
````

на:

````rust
}

/// «Быстрые действия» (Ctrl/Cmd+K и Ctrl/Cmd+/) и «Сочетания клавиш»
/// (Ctrl+Shift+? — Ctrl и на маке) с холста доходят до интерфейса событием.
#[test]
fn palette_and_shortcuts_panel_shortcuts_reach_the_interface() {
    for (platform, primary) in [
        (Platform::Mac, Modifiers::META),
        (Platform::Windows, ctrl()),
    ] {
        let mut h = H::with_platform(platform);
        h.ed.take_events();
        for (code, mods, want) in [
            ("KeyK", primary, CommandId::UiQuickActions),
            ("Slash", primary, CommandId::UiQuickActions),
            (
                "Slash",
                Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Modifiers::NONE
                },
                CommandId::UiShortcuts,
            ),
        ] {
            h.key(code, mods);
            assert_eq!(
                h.ed.take_events(),
                vec![EditorEvent::UiCommand(want)],
                "{platform:?} {code}"
            );
        }
    }
}

#[test]
fn ui_commands_keep_vector_editing() {
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    // «Экспорт…» и «Привязка к пикселям» — как в справке Figma.
    (CommandId::UiMainMenu, &["Alt+Slash"]),
    (CommandId::UiExport, &["Mod+Shift+KeyE"]),
    (CommandId::UiPixelSnap, &["Mod+Shift+Quote"]),
````

на:

````rust
    // «Экспорт…» и «Привязка к пикселям» — как в справке Figma.
    (CommandId::UiMainMenu, &["Alt+Slash"]),
    // Подсистема 2, этап 3: «Быстрые действия» и «Сочетания клавиш» — как в
    // справке Figma (Ctrl+Shift+? — Ctrl и на маке).
    (CommandId::UiQuickActions, &["Mod+KeyK", "Mod+Slash"]),
    (CommandId::UiShortcuts, &["Ctrl+Shift+Slash"]),
    (CommandId::UiExport, &["Mod+Shift+KeyE"]),
    (CommandId::UiPixelSnap, &["Mod+Shift+Quote"]),
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    CommandId::UiAllFiles,
    CommandId::UiSettings,
    CommandId::UiShortcuts,
    CommandId::UiAbout,
    CommandId::UiThemeDark,
````

на:

````rust
    CommandId::UiAllFiles,
    CommandId::UiSettings,
    CommandId::UiAbout,
    CommandId::UiThemeDark,
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 85);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 86);
}

````

В `crates/ferrum-editor/tests/main_menu.rs` заменить:

````rust
    let all: HashSet<CommandId> = menu_commands(&main_menu()).into_iter().collect();
    // Команды групп меню — все в дереве, кроме тех, что живут только на
    // клавишах: Esc, сдвиги, инструменты (у них тулбар), само главное меню.
    for c in COMMANDS {
        let only_keys = matches!(c.group, CommandGroup::Tool | CommandGroup::Nudge)
            || matches!(c.id, CommandId::Escape | CommandId::UiMainMenu);
        assert_eq!(
            all.contains(&c.id),
````

на:

````rust
    let all: HashSet<CommandId> = menu_commands(&main_menu()).into_iter().collect();
    // Команды групп меню — все в дереве, кроме тех, что живут только на
    // клавишах: Esc, сдвиги, инструменты (у них тулбар), само главное меню и
    // «Быстрые действия» (их поиск — поле вверху главного меню).
    for c in COMMANDS {
        let only_keys = matches!(c.group, CommandGroup::Tool | CommandGroup::Nudge)
            || matches!(
                c.id,
                CommandId::Escape | CommandId::UiMainMenu | CommandId::UiQuickActions
            );
        assert_eq!(
            all.contains(&c.id),
````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 85);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

на:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 86);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test keymap --test main_menu --test command_state --test command_table`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test boolean`
Expected: FAIL — редактор не собирается: нет `CommandId::UiQuickActions`; ядро собирается, но тест числа команд в `boolean.rs` падает (`left: 85, right: 86`). `e2e/app.spec.ts` (86 команд) проверяется в шаге «Проверить»

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::Save => self.out.events.push(EditorEvent::SaveRequested),
            C::UiMainMenu
            | C::UiNewFile
            | C::UiOpen
````

на:

````rust
            C::Save => self.out.events.push(EditorEvent::SaveRequested),
            C::UiMainMenu
            | C::UiQuickActions
            | C::UiNewFile
            | C::UiOpen
````

В `crates/ferrum-editor/src/command_state.rs` заменить:

````rust
            C::UiThemeSystem => (true, checked(self.settings.theme == ThemeChoice::System)),
            C::UiMainMenu
            | C::UiNewFile
            | C::UiOpen
````

на:

````rust
            C::UiThemeSystem => (true, checked(self.settings.theme == ThemeChoice::System)),
            C::UiMainMenu
            | C::UiQuickActions
            | C::UiNewFile
            | C::UiOpen
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// Открыть главное меню (Alt+/).
    UiMainMenu,
    UiNewFile,
    UiOpen,
````

на:

````rust
    /// Открыть главное меню (Alt+/).
    UiMainMenu,
    /// «Быстрые действия» (Ctrl/Cmd+K и Ctrl/Cmd+/, как в Figma): палитра команд,
    /// слоёв и страниц (спека подсистемы 2, раздел 7).
    UiQuickActions,
    UiNewFile,
    UiOpen,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        &["Alt+Slash"],
    ),
    cmd(
        C::UiNewFile,
````

на:

````rust
        &["Alt+Slash"],
    ),
    // Как в Figma: «Quick actions» — Ctrl/Cmd+K и Ctrl/Cmd+/.
    cmd(
        C::UiQuickActions,
        "ui.quick_actions",
        G::View,
        "Быстрые действия…",
        &[
            "палитра команд",
            "поиск",
            "найти слой",
            "quick actions",
            "command palette",
            "actions",
        ],
        &["Mod+KeyK", "Mod+Slash"],
    ),
    cmd(
        C::UiNewFile,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
            "hotkeys",
        ],
        &[],
    ),
    cmd(
````

на:

````rust
            "hotkeys",
        ],
        // Как в Figma: Ctrl+Shift+? — и на маке тоже Ctrl, а не Cmd.
        &["Ctrl+Shift+Slash"],
    ),
    cmd(
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/app.spec.ts e2e/main-menu.spec.ts --project=chromium`

Expected: PASS: fmt и clippy без замечаний; `ferrum-editor`, `ferrum-wasm` без падений (новый `palette_and_shortcuts_panel_shortcuts_reach_the_interface` в `command_table.rs`; команд 86); wasm собран, typecheck без ошибок; E2E `app` (86 команд) и `main-menu` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/command_state.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/tests/command_state.rs crates/ferrum-editor/tests/command_table.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/main_menu.rs crates/ferrum-wasm/tests/boolean.rs
git commit -F - <<'EOF'
feat(editor): команды «Быстрые действия» и сочетание «Сочетаний клавиш»

`ui.quick_actions` («Быстрые действия…», группа «Вид», Ctrl/Cmd+K и
Ctrl/Cmd+/, как в Figma) — команда интерфейса: ядро сообщает о ней событием
`UiCommand`. «Сочетания клавиш» (`ui.shortcuts`) получили Ctrl+Shift+? —
Ctrl и на маке, как в справке Figma. Таблица — 86 команд.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3: Нечёткий поиск и состав списка «Быстрых действий»

Спека §7: чистые функции без окна. `fuzzy.ts` — тот же счёт, что в `find.rs`; `normalize` и `layoutVariants` переезжают из `main-menu.ts` (там остаётся реэкспорт). `palette.ts` — строки списка из источников: недавние, команды, слои, страницы с заголовками, не больше 50; вес синонима и недавности; разбор недавних, обрезка запроса. `searchMenu` главного меню идёт через `rankCommands` без недавности.

**Files:**
- Create: `apps/web/src/ui/fuzzy.ts`, `apps/web/src/ui/palette.ts`
- Modify: `apps/web/src/ui/main-menu.ts`
- Test: `apps/web/unit/fuzzy.test.ts` (новый), `apps/web/unit/palette.test.ts` (новый), `apps/web/unit/main-menu.test.ts`

**Interfaces:**
- Consumes: `KeymapEntry`, `FoundLayerDto`, `PageInfo`, `NodeType` из `wasm/ferrum_wasm` (типы — задача 1).
- Produces: `fuzzy.ts` — `fold`, `queryVariants(query): string[][]`, `scoreFolded(t, q)`, `scoreVariants(text, variants)`, `fuzzyScore(text, query): number | null`, `normalize`, `layoutVariants`, `MAX_TEXT_CHARS`. `palette.ts` — `paletteRows(src: PaletteSources): PaletteRow[]`, `PaletteRow`, `rankCommands(entries, query, recent?, limit?)`, `scoreCommand`, `inPalette`, `parseRecent(s): string[]`, `pushRecent(recent, key)`, `clipQuery(q)`, `PALETTE_ROWS = 50`, `RECENT_MAX = 8`, `LAYER_LIMIT = 50`, `MAX_QUERY_BYTES = 256`, `COMMANDS_MAX = 20`, `PAGES_MAX = 10`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/unit/fuzzy.test.ts`:

````ts
// Нечёткий поиск «Быстрых действий»: подпоследовательность с весом за начало
// слова, буквы подряд, начало и всё имя; регистр, «ё» и раскладка не важны.
// Те же правила — у поиска слоёв в ядре (`crates/ferrum-editor/tests/find_layers.rs`).

import { expect, test } from '@playwright/test';
import { fuzzyScore, layoutVariants, queryVariants } from '../src/ui/fuzzy';

test('буквы запроса — по порядку, не обязательно подряд', () => {
  expect(fuzzyScore('Кнопка отправить', 'кно')).not.toBeNull();
  expect(fuzzyScore('Кнопка отправить', 'котп')).not.toBeNull();
  expect(fuzzyScore('Кнопка', 'кнопки')).toBeNull();
  expect(fuzzyScore('Кнопка', 'акн')).toBeNull();
  expect(fuzzyScore('Кнопка', '')).toBeNull();
  expect(fuzzyScore('Кнопка', '   ')).toBeNull();
});

test('веса: начало слова, буквы подряд, имя целиком', () => {
  const s = (t: string, q: string) => fuzzyScore(t, q) ?? -Infinity;
  expect(s('Order summary', 'su')).toBeGreaterThan(s('Ordersum', 'su'));
  expect(s('Карточка товара', 'кар')).toBeGreaterThan(s('Кнопка в ряд', 'кар'));
  expect(s('Фон', 'фон')).toBeGreaterThan(s('Фон карточки', 'фон'));
  expect(s('Слой 5', 'слой 5')).toBeGreaterThan(s('Слой 59', 'слой 5'));
  // Лучшее из выравниваний: «вп» — начала слов «Вставить поверх», а не «в» из «Вставить».
  expect(s('Вставить поверх', 'вп')).toBeGreaterThan(s('Вставить', 'вп'));
});

test('регистр, «ё» и раскладка не важны; пробелы запроса не учитываются', () => {
  expect(fuzzyScore('ЁЛКА', 'елка')).toBe(fuzzyScore('ёлка', 'ЕЛКА'));
  expect(fuzzyScore('Order', 'щквук')).toBe(fuzzyScore('Order', 'order'));
  expect(fuzzyScore('Файл', 'afqk')).not.toBeNull();
  expect(fuzzyScore('Шапка сайта', 'шап сай')).not.toBeNull();
  expect(layoutVariants('щквук')).toContain('order');
  expect(queryVariants('ab c').map((v) => v.join(''))).toContain('abc');
  expect(queryVariants('  ')).toEqual([]);
});

test('длинный текст участвует первыми 256 символами', () => {
  const long = 'а'.repeat(300) + 'я';
  expect(fuzzyScore(long, 'я')).toBeNull();
  expect(fuzzyScore('а'.repeat(255) + 'я', 'я')).not.toBeNull();
});
````

В `apps/web/unit/main-menu.test.ts` заменить:

````ts
// Главное меню: пункты из дерева ядра и поиск по командам (подстрока, синонимы,
// раскладка ЙЦУКЕН ↔ QWERTY).

import { expect, test } from '@playwright/test';
````

на:

````ts
// Главное меню: пункты из дерева ядра и поиск по командам — нечёткий, как в
// «Быстрых действиях» (синонимы, раскладка ЙЦУКЕН ↔ QWERTY).

import { expect, test } from '@playwright/test';
````

В `apps/web/unit/main-menu.test.ts` заменить:

````ts
});

test('поиск: подстрока названия и синонима, сначала начало названия', () => {
  const keys = (q: string) => searchMenu(tree, entries, q).map((m) => ('command' in m ? m.command : ''));
  expect(keys('сохр')).toEqual(['file.save', 'ui.save_disk']);
````

на:

````ts
});

test('поиск: название и синоним, сначала начало названия', () => {
  const keys = (q: string) => searchMenu(tree, entries, q).map((m) => ('command' in m ? m.command : ''));
  expect(keys('сохр')).toEqual(['file.save', 'ui.save_disk']);
````

В `apps/web/unit/main-menu.test.ts` заменить:

````ts
  expect(keys('  ')).toEqual([]);
  expect(keys('нет такого')).toEqual([]);
});

````

на:

````ts
  expect(keys('  ')).toEqual([]);
  expect(keys('нет такого')).toEqual([]);
});

test('поиск нечёткий: буквы по порядку, лучшие первыми', () => {
  const keys = (q: string) => searchMenu(tree, entries, q).map((m) => ('command' in m ? m.command : ''));
  // «сх диск» — не подстрока, но буквы идут по порядку в «Сохранить на диск…».
  expect(keys('сх диск')).toEqual(['ui.save_disk']);
  // Начало слова «PNG» весит больше букв вразброс.
  expect(keys('png')[0]).toBe('edit.copy_png');
});

````

Создать `apps/web/unit/palette.test.ts`:

````ts
// Список «Быстрых действий»: недавние, команды, слои и страницы с заголовками;
// не больше 50 строк; недавние — в настройках строкой ключей.

import { expect, test } from '@playwright/test';
import type { FoundLayerDto, KeymapEntry } from '../src/wasm/ferrum_wasm';
import {
  clipQuery,
  COMMANDS_MAX,
  MAX_QUERY_BYTES,
  PAGES_MAX,
  PALETTE_ROWS,
  paletteRows,
  parseRecent,
  pushRecent,
  rankCommands,
  RECENT_MAX,
  type PaletteRow,
} from '../src/ui/palette';

const entry = (key: string, name: string, keywords: string[] = [], group = key.split('.')[0]): KeymapEntry => ({
  key,
  name,
  group,
  keywords,
  toggle: false,
  bindings: [],
  labels: [],
  defaults: [],
  overridden: false,
});

const commands = [
  entry('edit.undo', 'Отменить', ['undo']),
  entry('edit.duplicate', 'Дублировать', ['дубль', 'копия', 'duplicate']),
  entry('edit.delete', 'Удалить', ['стереть', 'delete']),
  entry('arrange.front', 'На передний план', ['порядок', 'bring to front', 'order']),
  entry('arrange.back', 'На задний план', ['порядок', 'send to back', 'order']),
  entry('tool.hand_hold', 'Рука, пока клавиша нажата', ['hand']),
  entry('nudge.left', 'Сдвинуть влево', ['nudge']),
  entry('ui.quick_actions', 'Быстрые действия…', ['quick actions']),
  entry('ui.open_disk', 'Открыть с диска…', ['open from disk']),
];

const rows = (over: Partial<Parameters<typeof paletteRows>[0]> = {}) =>
  paletteRows({ query: '', commands, hidden: new Set(), recent: [], layers: [], pages: [], ...over });
const show = (r: PaletteRow[]) => r.map((x) => (x.kind === 'heading' ? `# ${x.title}` : x.kind === 'command' ? x.key : `${x.kind}:${x.name}`));

test('пустой запрос: недавние, затем остальные команды; без удержания, сдвигов и самой палитры', () => {
  expect(show(rows({ recent: ['edit.delete', 'ui.unknown', 'edit.undo'], hidden: new Set(['ui.open_disk']) }))).toEqual([
    '# Недавние',
    'edit.delete',
    'edit.undo',
    '# Команды',
    'edit.duplicate',
    'arrange.front',
    'arrange.back',
  ]);
  // Нет недавних — нет и заголовка.
  expect(show(rows())[0]).toBe('# Команды');
});

test('запрос: команды, слои, страницы с заголовками; раскладка и недавность', () => {
  const layers: FoundLayerDto[] = [{ id: '1@2', name: 'Порядок карточек', kind: 'frame' }];
  const r = rows({ query: 'порядок', layers, pages: [{ id: '0@1', name: 'Порядок' }, { id: '0@2', name: 'Обложка' }] });
  expect(show(r)).toEqual(['# Команды', 'arrange.front', 'arrange.back', '# Слои', 'layer:Порядок карточек', '# Страницы', 'page:Порядок']);
  // «щквук» — «order» в русской раскладке; недавняя команда — первой.
  expect(show(rows({ query: 'щквук', recent: ['arrange.back'] }))).toEqual(['# Команды', 'arrange.back', 'arrange.front']);
  // Ничего не совпало — пусто.
  expect(rows({ query: 'нет такого' })).toEqual([]);
});

test('не больше 50 строк: команды до 20, страницы до 10, остальное — слоям', () => {
  const many = Array.from({ length: 40 }, (_, i) => entry(`x.c${i}`, `Слой команда ${i}`));
  const layers: FoundLayerDto[] = Array.from({ length: 50 }, (_, i) => ({ id: `${i}@1`, name: `Слой ${i}`, kind: 'rect' }));
  const pages = Array.from({ length: 15 }, (_, i) => ({ id: `${i}@9`, name: `Слой-страница ${i}` }));
  const r = rows({ query: 'слой', commands: many, layers, pages });
  const count = (k: PaletteRow['kind']) => r.filter((x) => x.kind === k).length;
  expect(count('command')).toBe(COMMANDS_MAX);
  expect(count('page')).toBe(PAGES_MAX);
  expect(count('layer')).toBe(PALETTE_ROWS - COMMANDS_MAX - PAGES_MAX);
  expect(r.length - count('heading')).toBe(PALETTE_ROWS);
  // Пустой запрос и сотня команд — тоже 50.
  const hundred = Array.from({ length: 100 }, (_, i) => entry(`x.c${i}`, `Команда ${i}`));
  expect(rows({ commands: hundred }).filter((x) => x.kind === 'command')).toHaveLength(PALETTE_ROWS);
});

test('поиск команд: название выше синонима, равный вес — порядок таблицы', () => {
  const keys = (q: string, recent: string[] = []) => rankCommands(commands, q, recent).map((e) => e.key);
  // «дубл» — название «Дублировать» и синоним «дубль»: одна команда, не две.
  expect(keys('дубл')).toEqual(['edit.duplicate']);
  // «копия» только у синонима — команда находится.
  expect(keys('копия')).toEqual(['edit.duplicate']);
  // Подряд в «план» — выше букв вразброс («Рука, пока клавиша нажата»).
  expect(keys('план').slice(0, 2)).toEqual(['arrange.front', 'arrange.back']);
  expect(keys('план', ['arrange.back']).slice(0, 2)).toEqual(['arrange.back', 'arrange.front']);
});

test('недавние: в начало, без повторов, не больше 8; из настроек — только верные ключи', () => {
  let r: string[] = [];
  for (let i = 0; i < 10; i++) r = pushRecent(r, `edit.c${i}`);
  expect(r).toHaveLength(RECENT_MAX);
  expect(r[0]).toBe('edit.c9');
  r = pushRecent(r, 'edit.c5');
  expect(r[0]).toBe('edit.c5');
  expect(r.filter((k) => k === 'edit.c5')).toHaveLength(1);
  expect(pushRecent(r, 'мусор')).toEqual(r);
  expect(parseRecent('edit.undo  edit.undo ui.export <script> x'.padEnd(10))).toEqual(['edit.undo', 'ui.export']);
  expect(parseRecent(Array.from({ length: 20 }, (_, i) => `edit.c${i}`).join(' '))).toHaveLength(RECENT_MAX);
  expect(parseRecent('')).toEqual([]);
});

test('запрос к worker\'у — не длиннее 256 байт, по границе символа', () => {
  expect(clipQuery('кнопка')).toBe('кнопка');
  const long = 'я'.repeat(200);
  const clipped = clipQuery(long);
  expect(new TextEncoder().encode(clipped).length).toBeLessThanOrEqual(MAX_QUERY_BYTES);
  expect(clipped).toBe('я'.repeat(128));
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/fuzzy.test.ts unit/palette.test.ts unit/main-menu.test.ts`
Expected: FAIL — модульные — `Cannot find module '…/apps/web/src/ui/fuzzy'` и `…/palette` (No tests found); запуск падает на загрузке файлов; по отдельности новый тест `main-menu.test.ts` («сх диск») падает на старом поиске-подстроке

- [ ] **Step 3: Реализация**

Создать `apps/web/src/ui/fuzzy.ts`:

````ts
// Нечёткий поиск «Быстрых действий» и главного меню (спека подсистемы 2,
// раздел 7): буквы запроса должны встретиться в тексте по порядку
// (подпоследовательность), вес растёт за начало слова, буквы подряд и начало
// текста и падает за пропуски. Регистр и «ё» не важны, раскладка тоже: запрос
// пробуется как набран и переведённым ЙЦУКЕН ↔ QWERTY («щквук» находит «order»).
// Пробелы запроса не учитываются. Тот же счёт — у поиска слоёв в ядре
// (`crates/ferrum-editor/src/find.rs`): веса и правила совпадают.

const EN = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
const RU = 'йцукенгшщзхъфывапролджэячсмитьбюё';

const toRu = new Map([...EN].map((c, i) => [c, RU[i]]));
const toEn = new Map([...RU].map((c, i) => [c, EN[i]]));

/** Строка для сравнения: нижний регистр, «ё» как «е», без пробелов по краям. */
export function normalize(s: string): string {
  return s.trim().toLowerCase().replaceAll('ё', 'е');
}

/** Запрос в обеих раскладках: как набран, ЙЦУКЕН → QWERTY и обратно. */
export function layoutVariants(query: string): string[] {
  const q = query.trim().toLowerCase();
  const swap = (m: Map<string, string>) => [...q].map((c) => m.get(c) ?? c).join('');
  return [...new Set([q, swap(toRu), swap(toEn)].map(normalize))].filter(Boolean);
}

/** Сколько первых символов текста участвует в поиске (как `FIND_NAME_CHARS` ядра). */
export const MAX_TEXT_CHARS = 256;

const MATCH = 1;
const WORD_START = 8;
const CONSECUTIVE = 5;
const PREFIX = 6;
const MAX_GAP = 3;
const EXACT = 10;

/** Символы для сравнения: нижний регистр, «ё» как «е», первые `MAX_TEXT_CHARS`. */
export function fold(s: string): string[] {
  return [...[...s].slice(0, MAX_TEXT_CHARS).join('').toLowerCase().replaceAll('ё', 'е')];
}

/** Варианты запроса для `scoreFolded`: обе раскладки, без пробелов; пусто — нечего искать. */
export function queryVariants(query: string): string[][] {
  const out: string[][] = [];
  for (const v of layoutVariants(query)) {
    const q = [...v.replace(/\s+/gu, '')];
    if (q.length && !out.some((o) => o.join('') === q.join(''))) out.push(q);
  }
  return out;
}

const ALNUM = /[\p{L}\p{N}]/u;

/** Начало слова: первый символ или после символа, который не буква и не цифра. */
function isStart(t: string[], j: number): boolean {
  return j === 0 || !ALNUM.test(t[j - 1]);
}

function scorePositions(t: string[], pos: number[]): number {
  let s = 0;
  pos.forEach((j, i) => {
    s += MATCH;
    if (isStart(t, j)) s += WORD_START;
    if (i > 0) {
      const gap = j - pos[i - 1] - 1;
      s += gap === 0 ? CONSECUTIVE : -Math.min(gap, MAX_GAP);
    }
  });
  if (pos[0] === 0) s += PREFIX;
  else s -= Math.min(pos[0], MAX_GAP);
  // Совпали все буквы текста (пробелы запроса не учитываются — и текста тоже).
  if (pos.length === t.filter((c) => !/\s/u.test(c)).length) s += EXACT;
  return s;
}

/** Буквы запроса по порядку: каждая — первое вхождение после прошлой; с
 * `preferStart` — сначала вхождение в начале слова, если оно есть. */
function align(t: string[], q: string[], preferStart: boolean): number[] | null {
  const pos: number[] = [];
  let from = 0;
  for (const qc of q) {
    let j = -1;
    if (preferStart) {
      for (let k = from; k < t.length; k++) {
        if (t[k] === qc && isStart(t, k)) {
          j = k;
          break;
        }
      }
    }
    if (j < 0) j = t.indexOf(qc, from);
    if (j < 0) return null;
    pos.push(j);
    from = j + 1;
  }
  return pos;
}

/** Запрос подстрокой: вхождение в начале слова, иначе первое. */
function substring(t: string[], q: string[]): number[] | null {
  let best = -1;
  for (let i = 0; i + q.length <= t.length; i++) {
    let ok = true;
    for (let k = 0; k < q.length && ok; k++) ok = t[i + k] === q[k];
    if (!ok) continue;
    if (isStart(t, i)) {
      best = i;
      break;
    }
    if (best < 0) best = i;
  }
  return best < 0 ? null : q.map((_, k) => best + k);
}

/** Вес совпадения одного варианта запроса со сложенным текстом; `null` — не совпало. */
export function scoreFolded(t: string[], q: string[]): number | null {
  if (!q.length) return null;
  const greedy = align(t, q, false);
  if (!greedy) return null;
  let best = scorePositions(t, greedy);
  for (const pos of [align(t, q, true), substring(t, q)]) {
    if (pos) best = Math.max(best, scorePositions(t, pos));
  }
  return best;
}

/** Лучший вес по вариантам запроса (`queryVariants`); `null` — не совпало ни одним. */
export function scoreVariants(text: string, variants: readonly string[][]): number | null {
  const t = fold(text);
  let best: number | null = null;
  for (const q of variants) {
    const s = scoreFolded(t, q);
    if (s !== null && (best === null || s > best)) best = s;
  }
  return best;
}

/** Вес совпадения запроса с текстом в любой раскладке; `null` — не совпало. */
export function fuzzyScore(text: string, query: string): number | null {
  return scoreVariants(text, queryVariants(query));
}
````

В `apps/web/src/ui/main-menu.ts` заменить:

````ts
// Главное меню (спека подсистемы 2, раздел 4): пункты из дерева ядра
// (`MainMenu`) с названиями из таблицы команд и поиск по командам меню —
// подстрока названия или синонима, без учёта регистра, «ё» и раскладки
// (запрос «щквук» находит «order»: буквы переводятся ЙЦУКЕН ↔ QWERTY).
// Основной поток ничего не вычисляет о документе: доступность и галочки —
// из состояния команд ядра, здесь — только названия и отбор по строке.
````

на:

````ts
// Главное меню (спека подсистемы 2, раздел 4): пункты из дерева ядра
// (`MainMenu`) с названиями из таблицы команд и поиск по командам меню — тот
// же нечёткий поиск, что в «Быстрых действиях» (`fuzzy.ts`, `palette.ts`), без
// учёта регистра, «ё» и раскладки (запрос «щквук» находит «order»: буквы
// переводятся ЙЦУКЕН ↔ QWERTY).
// Основной поток ничего не вычисляет о документе: доступность и галочки —
// из состояния команд ядра, здесь — только названия и отбор по строке.
````

В `apps/web/src/ui/main-menu.ts` заменить:

````ts
import type { KeymapEntry, MenuItemDto } from '../wasm/ferrum_wasm';
import type { MenuItem } from './menu';

/** Сколько результатов поиска показывать (как у «Быстрых действий»). */
export const MAX_RESULTS = 50;

const EN = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
const RU = 'йцукенгшщзхъфывапролджэячсмитьбюё';

const toRu = new Map([...EN].map((c, i) => [c, RU[i]]));
const toEn = new Map([...RU].map((c, i) => [c, EN[i]]));

/** Строка для сравнения: нижний регистр, «ё» как «е», без пробелов по краям. */
export function normalize(s: string): string {
  return s.trim().toLowerCase().replaceAll('ё', 'е');
}

/** Запрос в обеих раскладках: как набран, ЙЦУКЕН → QWERTY и обратно. */
export function layoutVariants(query: string): string[] {
  const q = query.trim().toLowerCase();
  const swap = (m: Map<string, string>) => [...q].map((c) => m.get(c) ?? c).join('');
  return [...new Set([q, swap(toRu), swap(toEn)].map(normalize))].filter(Boolean);
}

type Entries = ReadonlyMap<string, KeymapEntry>;
````

на:

````ts
import type { KeymapEntry, MenuItemDto } from '../wasm/ferrum_wasm';
import type { MenuItem } from './menu';
import { layoutVariants, normalize } from './fuzzy';
import { rankCommands } from './palette';

export { layoutVariants, normalize };

/** Сколько результатов поиска показывать (как у «Быстрых действий»). */
export const MAX_RESULTS = 50;

type Entries = ReadonlyMap<string, KeymapEntry>;
````

В `apps/web/src/ui/main-menu.ts` заменить:

````ts
}

/** Поиск по командам меню: подстрока названия или синонима в любой раскладке.
 * Сначала команды, чьё название начинается с запроса, затем остальные — в
 * порядке меню; не больше `MAX_RESULTS`. */
export function searchMenu(tree: readonly MenuItemDto[], entries: Entries, query: string, hidden: ReadonlySet<string> = new Set()): MenuItem[] {
  const variants = layoutVariants(query);
  if (!variants.length) return [];
  const first: MenuItem[] = [];
  const rest: MenuItem[] = [];
  for (const key of treeKeys(tree)) {
    const e = entries.get(key);
    if (!e || hidden.has(key)) continue;
    const name = normalize(e.name);
    const words = e.keywords.map(normalize);
    const item: MenuItem = { label: e.name, command: e.key };
    if (variants.some((v) => name.startsWith(v))) first.push(item);
    else if (variants.some((v) => name.includes(v) || words.some((w) => w.includes(v)))) rest.push(item);
  }
  return [...first, ...rest].slice(0, MAX_RESULTS);
}
````

на:

````ts
}

/** Поиск по командам меню, как в «Быстрых действиях»: нечётко по названию и
 * синонимам в любой раскладке, лучшие первыми, при равном весе — в порядке
 * меню; не больше `MAX_RESULTS`. */
export function searchMenu(tree: readonly MenuItemDto[], entries: Entries, query: string, hidden: ReadonlySet<string> = new Set()): MenuItem[] {
  const list: KeymapEntry[] = [];
  for (const key of treeKeys(tree)) {
    const e = entries.get(key);
    if (e && !hidden.has(key)) list.push(e);
  }
  return rankCommands(list, query, [], MAX_RESULTS).map((e) => ({ label: e.name, command: e.key }));
}
````

Создать `apps/web/src/ui/palette.ts`:

````ts
// «Быстрые действия» (спека подсистемы 2, раздел 7): что показать в списке
// палитры. Источники — недавние команды, команды таблицы, слои текущей страницы
// (их ищет worker: `FindLayers` → `LayersFound`) и страницы; один список с
// заголовками, не больше `PALETTE_ROWS` строк. Команды ищутся здесь, в основном
// потоке (их ≈ 90): нечётко по названию и синонимам (`fuzzy.ts`), с весом за
// недавность. Недавние — последние `RECENT_MAX` выполненных команд, хранятся в
// настройках строкой ключей через пробел.

import type { FoundLayerDto, KeymapEntry, NodeType, PageInfo } from '../wasm/ferrum_wasm';
import { queryVariants, scoreVariants } from './fuzzy';

/** Строк в списке, не считая заголовков (как у поиска главного меню). */
export const PALETTE_ROWS = 50;
/** Недавних команд. */
export const RECENT_MAX = 8;
/** Сколько слоёв просить у worker'а (`FindLayers.limit`). */
export const LAYER_LIMIT = 50;
/** Предел запроса поиска слоёв в байтах UTF-8 (как у ядра). */
export const MAX_QUERY_BYTES = 256;
/** Команд и страниц при непустом запросе: остальное место — слоям. */
export const COMMANDS_MAX = 20;
export const PAGES_MAX = 10;
/** Синоним весит меньше названия; недавняя команда — больше остальных. */
const KEYWORD_PENALTY = 4;
const RECENT_BONUS = 3;

/** Команды, которых нет в палитре: удержание клавиши, Esc, сдвиги и сама палитра. */
const EXCLUDED = new Set(['tool.hand_hold', 'edit.escape', 'ui.quick_actions']);

export function inPalette(e: KeymapEntry): boolean {
  return e.group !== 'nudge' && !EXCLUDED.has(e.key);
}

export type PaletteRow =
  | { kind: 'heading'; title: string }
  | { kind: 'command'; key: string }
  | { kind: 'layer'; id: string; name: string; type: NodeType }
  | { kind: 'page'; id: string; name: string };

const KEY = /^[a-z]+\.[a-z0-9_]+$/;

/** Недавние из настроек (не доверенные): ключи команд через пробел, без повторов. */
export function parseRecent(s: string): string[] {
  const out: string[] = [];
  for (const k of s.split(' ')) {
    if (k.length <= 64 && KEY.test(k) && !out.includes(k)) out.push(k);
    if (out.length === RECENT_MAX) break;
  }
  return out;
}

/** Выполненная команда — в начало недавних (повтор переезжает), не больше `RECENT_MAX`. */
export function pushRecent(recent: readonly string[], key: string): string[] {
  if (!KEY.test(key)) return [...recent];
  return [key, ...recent.filter((k) => k !== key)].slice(0, RECENT_MAX);
}

/** Вес команды по запросу: лучшее из названия и синонимов (синоним — с вычетом),
 * плюс недавность; `null` — не совпала. */
export function scoreCommand(e: KeymapEntry, variants: readonly string[][], recent: readonly string[] = []): number | null {
  let best = scoreVariants(e.name, variants);
  for (const k of e.keywords) {
    const s = scoreVariants(k, variants);
    if (s !== null && (best === null || s - KEYWORD_PENALTY > best)) best = s - KEYWORD_PENALTY;
  }
  if (best === null) return null;
  const r = recent.indexOf(e.key);
  return r < 0 ? best : best + (RECENT_MAX - r) * RECENT_BONUS;
}

/** Совпавшие команды, лучшие первыми, при равном весе — в порядке `entries`. */
export function rankCommands(entries: readonly KeymapEntry[], query: string, recent: readonly string[] = [], limit = PALETTE_ROWS): KeymapEntry[] {
  const variants = queryVariants(query);
  if (!variants.length) return [];
  const hits: { e: KeymapEntry; s: number; i: number }[] = [];
  entries.forEach((e, i) => {
    const s = scoreCommand(e, variants, recent);
    if (s !== null) hits.push({ e, s, i });
  });
  hits.sort((a, b) => b.s - a.s || a.i - b.i);
  return hits.slice(0, limit).map((h) => h.e);
}

/** Запрос, обрезанный до `MAX_QUERY_BYTES` байт UTF-8 по границе символа. */
export function clipQuery(q: string): string {
  const enc = new TextEncoder();
  if (enc.encode(q).length <= MAX_QUERY_BYTES) return q;
  let out = '';
  let bytes = 0;
  for (const c of q) {
    const n = enc.encode(c).length;
    if (bytes + n > MAX_QUERY_BYTES) break;
    out += c;
    bytes += n;
  }
  return out;
}

export interface PaletteSources {
  query: string;
  /** Таблица команд (`KeymapEntry`) в порядке ядра. */
  commands: readonly KeymapEntry[];
  /** Команды, которых нет на этой платформе (файл на диске без File System Access). */
  hidden: ReadonlySet<string>;
  recent: readonly string[];
  /** Ответ worker'а на этот запрос (на прежний — не передавать). */
  layers: readonly FoundLayerDto[];
  pages: readonly PageInfo[];
}

/** Строки палитры. Пустой запрос — недавние и все команды; иначе — команды (не
 * больше `COMMANDS_MAX`), слои и страницы (не больше `PAGES_MAX`) с заголовками
 * непустых разделов; строк с действиями — не больше `PALETTE_ROWS`. */
export function paletteRows(src: PaletteSources): PaletteRow[] {
  const usable = src.commands.filter((e) => inPalette(e) && !src.hidden.has(e.key));
  const out: PaletteRow[] = [];
  let left = PALETTE_ROWS;
  const section = (title: string, rows: PaletteRow[]) => {
    const take = rows.slice(0, left);
    if (!take.length) return;
    out.push({ kind: 'heading', title }, ...take);
    left -= take.length;
  };
  if (!queryVariants(src.query).length) {
    const byKey = new Map(usable.map((e) => [e.key, e]));
    const recent = src.recent.filter((k) => byKey.has(k));
    section('Недавние', recent.map((key) => ({ kind: 'command', key })));
    section('Команды', usable.filter((e) => !recent.includes(e.key)).map((e) => ({ kind: 'command', key: e.key })));
    return out;
  }
  const commands = rankCommands(usable, src.query, src.recent, COMMANDS_MAX);
  const variants = queryVariants(src.query);
  const pages = src.pages
    .map((p, i) => ({ p, i, s: scoreVariants(p.name, variants) }))
    .filter((x): x is { p: PageInfo; i: number; s: number } => x.s !== null)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .slice(0, PAGES_MAX)
    .map((x): PaletteRow => ({ kind: 'page', id: x.p.id, name: x.p.name }));
  const layerRoom = PALETTE_ROWS - Math.min(commands.length, PALETTE_ROWS) - pages.length;
  section('Команды', commands.map((e) => ({ kind: 'command', key: e.key })));
  section(
    'Слои',
    src.layers.slice(0, Math.max(0, layerRoom)).map((l) => ({ kind: 'layer', id: l.id, name: l.name, type: l.kind })),
  );
  section('Страницы', pages);
  return out;
}
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/main-menu.spec.ts --project=chromium`

Expected: модульные — `fuzzy.test.ts` (4), `palette.test.ts` (6), `main-menu.test.ts` (+1) зелёные, typecheck и сборка — ок; E2E `main-menu` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/src/ui/fuzzy.ts apps/web/src/ui/main-menu.ts apps/web/src/ui/palette.ts apps/web/unit/fuzzy.test.ts apps/web/unit/main-menu.test.ts apps/web/unit/palette.test.ts
git commit -F - <<'EOF'
feat(web): нечёткий поиск и состав списка «Быстрых действий»

`fuzzy.ts` — подпоследовательность с весом за начало слова, буквы подряд и
начало имени; регистр, «ё» и раскладка не важны; те же правила, что у поиска
слоёв в ядре. `palette.ts` — строки палитры: недавние, команды, слои и
страницы с заголовками, не больше 50; вес синонима и недавности; разбор
недавних из настроек. Поиск главного меню — тот же нечёткий.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4: Недавние команды в настройках

Спека §7: поле `recentCommands` — ключи последних восьми команд через пробел; значения из хранилища не доверенные — на чтении только верные ключи. Команды из главного и контекстного меню попадают в недавние; изменения других вкладок применяются на лету. Палитра подключит `noteRecent` в задаче 5.

**Files:**
- Modify: `apps/web/src/ui/{settings.ts, app.tsx, context.ts, menu.tsx}`, `apps/web/src/core/file-list.ts`, `apps/desktop/src-tauri/store/src/settings.rs`
- Test: `apps/web/unit/settings.test.ts`, `apps/desktop/src-tauri/store/tests/store.rs`

**Interfaces:**
- Consumes: `parseRecent`, `pushRecent` (задача 3), `diffSettings`, `FIELDS`, `SETTINGS_FIELDS` (TS и Rust).
- Produces: `Settings.recentCommands: string`; `Editor.noteRecent?: (key: string) => void` (ставит `App`); сигнал `recent` в `App`.

- [ ] **Step 1: Написать тесты**

В `apps/desktop/src-tauri/store/tests/store.rs` заменить:

````rust
    s.set(patch(serde_json::json!({"filesView": "list"})))
        .unwrap();
    assert!(s.set(patch(serde_json::json!({"чужое": 1}))).is_err());
    assert!(s
````

на:

````rust
    s.set(patch(serde_json::json!({"filesView": "list"})))
        .unwrap();
    s.set(patch(
        serde_json::json!({"recentCommands": "edit.undo ui.export"}),
    ))
    .unwrap();
    assert!(s.set(patch(serde_json::json!({"чужое": 1}))).is_err());
    assert!(s
````

В `apps/desktop/src-tauri/store/tests/store.rs` заменить:

````rust
    assert_eq!(
        serde_json::Value::Object(again.all()),
        serde_json::json!({"theme": "light", "filesView": "list"})
    );
    fs::write(&path, b"[]").unwrap();
````

на:

````rust
    assert_eq!(
        serde_json::Value::Object(again.all()),
        serde_json::json!({
            "theme": "light",
            "filesView": "list",
            "recentCommands": "edit.undo ui.export"
        })
    );
    fs::write(&path, b"[]").unwrap();
````

В `apps/web/unit/settings.test.ts` заменить:

````ts
  s.setItem(SETTINGS_KEY, JSON.stringify({ filesView: 'плитка' }));
  expect(loadSettings(s).filesView).toBe('grid');
});

````

на:

````ts
  s.setItem(SETTINGS_KEY, JSON.stringify({ filesView: 'плитка' }));
  expect(loadSettings(s).filesView).toBe('grid');
});

test('недавние команды: нет в старых записях — пусто; только верные ключи, не больше восьми', () => {
  const s = memory();
  s.setItem(SETTINGS_KEY, JSON.stringify({ theme: 'light' }));
  expect(loadSettings(s).recentCommands).toBe('');
  expect(parseSettingsPatch({ recentCommands: 'edit.undo <b> edit.undo ui.export' })).toEqual({ recentCommands: 'edit.undo ui.export' });
  const many = Array.from({ length: 20 }, (_, i) => `edit.c${i}`).join(' ');
  expect(parseSettingsPatch({ recentCommands: many }).recentCommands?.split(' ')).toHaveLength(8);
  // Не строка и слишком длинная строка — поле отбрасывается.
  expect(parseSettingsPatch({ recentCommands: ['edit.undo'] })).toEqual({});
  expect(parseSettingsPatch({ recentCommands: 'x'.repeat(2000) })).toEqual({});
  expect(diffSettings(DEFAULT_SETTINGS, { recentCommands: 'edit.undo' })).toEqual({ recentCommands: 'edit.undo' });
});

````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/settings.test.ts`
Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-desktop-store --test store`
Expected: FAIL — модульные — новый тест `settings.test.ts` падает: `loadSettings(s).recentCommands` — `undefined`, поле не разбирается; `store` — `settings_are_written_by_field_and_checked` падает на `.unwrap()`: поля `recentCommands` нет в `SETTINGS_FIELDS` десктопа

- [ ] **Step 3: Реализация**

В `apps/desktop/src-tauri/store/src/settings.rs` заменить:

````rust

/// Поля настроек — те же, что `SETTINGS_FIELDS` интерфейса.
pub const SETTINGS_FIELDS: [&str; 7] = [
    "theme",
    "pixelSnap",
````

на:

````rust

/// Поля настроек — те же, что `SETTINGS_FIELDS` интерфейса.
pub const SETTINGS_FIELDS: [&str; 8] = [
    "theme",
    "pixelSnap",
````

В `apps/desktop/src-tauri/store/src/settings.rs` заменить:

````rust
    "pagesOpen",
    "filesView",
];
/// Запись настроек не больше этого (как `MAX_SETTINGS_BYTES`).
````

на:

````rust
    "pagesOpen",
    "filesView",
    "recentCommands",
];
/// Запись настроек не больше этого (как `MAX_SETTINGS_BYTES`).
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
/** Поля настроек пользователя в хранилище `settings` — по ключу на поле: вкладки
 * пишут только свои изменения и не затирают чужие. Значения проверяет интерфейс. */
export const SETTINGS_FIELDS = ['theme', 'pixelSnap', 'keymap', 'leftWidth', 'rightWidth', 'pagesOpen', 'filesView'] as const;

/** Запись о файле — без байтов: список читает только метаданные. Время — мс эпохи. */
````

на:

````ts
/** Поля настроек пользователя в хранилище `settings` — по ключу на поле: вкладки
 * пишут только свои изменения и не затирают чужие. Значения проверяет интерфейс. */
export const SETTINGS_FIELDS = ['theme', 'pixelSnap', 'keymap', 'leftWidth', 'rightWidth', 'pagesOpen', 'filesView', 'recentCommands'] as const;

/** Запись о файле — без байтов: список читает только метаданные. Время — мс эпохи. */
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { Menu, type MenuItem, type MenuSearch } from './menu';
import { buildMainMenu, entriesByKey, searchMenu } from './main-menu';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
````

на:

````tsx
import { Menu, type MenuItem, type MenuSearch } from './menu';
import { buildMainMenu, entriesByKey, searchMenu } from './main-menu';
import { parseRecent, pushRecent } from './palette';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  /** Открытое контекстное меню: пункты из ядра, точка окна и название. */
  const [context, setContext] = createSignal<{ items: MenuItem[]; at: { x: number; y: number }; label: string } | null>(null);
````

на:

````tsx
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  /** Недавние команды «Быстрых действий»: новые первыми, общие для вкладок. */
  const [recent, setRecent] = createSignal(parseRecent(saved.recentCommands));
  /** Открытое контекстное меню: пункты из ядра, точка окна и название. */
  const [context, setContext] = createSignal<{ items: MenuItem[]; at: { x: number; y: number }; label: string } | null>(null);
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      setPixelSnap(p.pixelSnap);
    }
    if (p.keymap !== undefined && p.keymap !== keymap()) {
      const prev = keymap();
````

на:

````tsx
      setPixelSnap(p.pixelSnap);
    }
    if (p.recentCommands !== undefined) {
      written = { ...written, recentCommands: p.recentCommands };
      setRecent(parseRecent(p.recentCommands));
    }
    if (p.keymap !== undefined && p.keymap !== keymap()) {
      const prev = keymap();
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      rightWidth: right(),
      pagesOpen: pagesOpen(),
    };
    clearTimeout(writeTimer);
````

на:

````tsx
      rightWidth: right(),
      pagesOpen: pagesOpen(),
      recentCommands: recent().join(' '),
    };
    clearTimeout(writeTimer);
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        }
        ed.uiCommand = uiCommand;
        ed.contextMenu = (target: ContextTarget, at, label, onAction) => {
          setContext(null);
````

на:

````tsx
        }
        ed.uiCommand = uiCommand;
        ed.noteRecent = (key) => setRecent((r) => pushRecent(r, key));
        ed.contextMenu = (target: ContextTarget, at, label, onAction) => {
          setContext(null);
````

В `apps/web/src/ui/context.ts` заменить:

````ts
   * над целью (переименовать, страницы). Ставит редактор приложения. */
  contextMenu?: (target: ContextTarget, at: { x: number; y: number }, label: string, onAction?: (a: MenuActionDto) => void) => void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
````

на:

````ts
   * над целью (переименовать, страницы). Ставит редактор приложения. */
  contextMenu?: (target: ContextTarget, at: { x: number; y: number }, label: string, onAction?: (a: MenuActionDto) => void) => void;
  /** Команда выполнена из меню или «Быстрых действий»: запомнить в недавних.
   * Ставит редактор приложения. */
  noteRecent?: (key: string) => void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
    props.closeAll(true);
    if (it.onSelect) it.onSelect();
    else if (it.command) ed?.run(it.command);
  };

````

на:

````tsx
    props.closeAll(true);
    if (it.onSelect) it.onSelect();
    else if (it.command) {
      ed?.noteRecent?.(it.command);
      ed?.run(it.command);
    }
  };

````

В `apps/web/src/ui/settings.ts` заменить:

````ts

// Настройки пользователя: тема, привязка к пикселям, отличия раскладки клавиш,
// ширина панелей, вид экрана файлов (сетка или список). С этапа 6 хранятся в
// IndexedDB по полю (`settings-store.ts`); здесь — поля, их разбор (значения из
// хранилища не доверенные: неверное поле — умолчание) и прежняя запись
````

на:

````ts

// Настройки пользователя: тема, привязка к пикселям, отличия раскладки клавиш,
// ширина панелей, вид экрана файлов (сетка или список), недавние команды
// «Быстрых действий». С этапа 6 хранятся в
// IndexedDB по полю (`settings-store.ts`); здесь — поля, их разбор (значения из
// хранилища не доверенные: неверное поле — умолчание) и прежняя запись
````

В `apps/web/src/ui/settings.ts` заменить:

````ts
   * в старых записях его нет — тогда сетка). */
  filesView: FilesView;
}

````

на:

````ts
   * в старых записях его нет — тогда сетка). */
  filesView: FilesView;
  /** Недавние команды «Быстрых действий»: ключи через пробел, новые первыми (поле
   * появилось в подсистеме 2; в старых записях его нет — тогда пусто). */
  recentCommands: string;
}

````

В `apps/web/src/ui/settings.ts` заменить:

````ts

import { MAX_KEYMAP_BYTES, MAX_SETTINGS_BYTES } from '../core/limits';

/** Прежняя запись настроек в `localStorage` (до этапа 6): переносится и удаляется. */
````

на:

````ts

import { MAX_KEYMAP_BYTES, MAX_SETTINGS_BYTES } from '../core/limits';
import { parseRecent } from './palette';

/** Прежняя запись настроек в `localStorage` (до этапа 6): переносится и удаляется. */
````

В `apps/web/src/ui/settings.ts` заменить:

````ts
  pagesOpen: true,
  filesView: 'grid',
};

````

на:

````ts
  pagesOpen: true,
  filesView: 'grid',
  recentCommands: '',
};

````

В `apps/web/src/ui/settings.ts` заменить:

````ts
  pagesOpen: (v) => (typeof v === 'boolean' ? v : undefined),
  filesView: (v) => (v === 'list' || v === 'grid' ? v : undefined),
};

````

на:

````ts
  pagesOpen: (v) => (typeof v === 'boolean' ? v : undefined),
  filesView: (v) => (v === 'list' || v === 'grid' ? v : undefined),
  // Только верные ключи, не больше восьми; строка длиннее не бывает (предел — на разбор).
  recentCommands: (v) => (typeof v === 'string' && v.length <= 1024 ? parseRecent(v).join(' ') : undefined),
};

````

- [ ] **Step 4: Проверить**

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-desktop-store 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo check`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/settings-store.spec.ts e2e/main-menu.spec.ts --project=chromium`

Expected: тесты `store` десктопа — 25 passed; `cargo check` десктопа — без ошибок; модульные (`settings.test.ts` +1) и сборка — ок; E2E `settings-store`, `main-menu` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/desktop/src-tauri/store/src/settings.rs apps/desktop/src-tauri/store/tests/store.rs apps/web/src/core/file-list.ts apps/web/src/ui/app.tsx apps/web/src/ui/context.ts apps/web/src/ui/menu.tsx apps/web/src/ui/settings.ts apps/web/unit/settings.test.ts
git commit -F - <<'EOF'
feat(web,desktop): недавние команды «Быстрых действий» в настройках

Поле настроек `recentCommands` — ключи последних восьми команд через пробел
(значения в файле десктопа — только простые); не доверенное: на чтении только
верные ключи. Команды из меню попадают в недавние; изменения других вкладок
применяются на лету.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 5: Палитра «Быстрые действия»

Спека §7: окно внутри холста, положение задаёт CSS; фокус остаётся в поле (`combobox`, `aria-activedescendant`); стрелки ходят по строкам, включая серые и по кругу; Enter и щелчок выполняют, как главное меню; Esc и то же сочетание закрывают с возвратом фокуса; щелчок мимо и Tab — без возврата. Каждый символ — новый `FindLayers`; ответы на прежние запросы отбрасываются по `query`. Выбор слоя — `RevealLayer`, страницы — `SetPage`.

**Files:**
- Create: `apps/web/src/ui/quick-actions.tsx`
- Modify: `apps/web/src/ui/{app.tsx, layers.tsx}` (`KIND_ICON` экспортирован), `apps/web/src/ui/styles/chrome.css`
- Test: `apps/web/e2e/palette.spec.ts` (новый)

**Interfaces:**
- Consumes: `paletteRows`, `clipQuery`, `LAYER_LIMIT` (задача 3), `Editor.noteRecent`, `recent` (задача 4), `Command::{FindLayers, RevealLayer, SetPage}`, `CoreEvent::LayersFound`, `ed.run`, `ed.scheduler.{measure, mutate, enqueue}`, `GROUPS`, `entriesByKey`, `shortcutLabel`, `eventShortcut`.
- Produces: `QuickActions(props: { hidden: ReadonlySet<string>; recent: readonly string[]; onClose: () => void })`; роли: `dialog` «Быстрые действия», `combobox`, `listbox` «Результаты», `option` с `data-command`; `export const KIND_ICON` из `layers.tsx`; ветка `ui.quick_actions` в `uiCommand`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/e2e/palette.spec.ts`:

````ts
// «Быстрые действия» (спека подсистемы 2, раздел 7): Ctrl/Cmd+K и Ctrl/Cmd+/,
// поле сверху по центру холста, команды с сочетаниями и серыми пунктами, поиск в
// другой раскладке, слои из worker'а с переходом к слою, страницы, недавние в
// настройках. Открытие и ввод не читают раскладку вне фазы measure.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, editorReady, live, liveWhen, mainMenu, openApp } from './helpers';

const palette = (page: Page) => page.getByRole('dialog', { name: 'Быстрые действия' });
const field = (page: Page) => page.getByRole('combobox', { name: 'Быстрые действия' });
const list = (page: Page) => palette(page).getByRole('listbox');
const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);
const stored = (page: Page) => page.evaluate(() => window.__ferrumFiles!.store.settings());

async function openPalette(page: Page, key = 'ControlOrMeta+KeyK'): Promise<void> {
  await page.locator('canvas').focus();
  await page.keyboard.press(key);
  await expect(field(page)).toBeFocused();
}

/** Выбранная строка списка (`aria-activedescendant`). */
const active = (page: Page) => list(page).locator('[role="option"][aria-selected="true"]');

test('Ctrl/Cmd+K и Ctrl/Cmd+/ открывают поле сверху по центру холста; Esc закрывает и возвращает фокус', async ({ page }) => {
  await openApp(page);
  for (const key of ['ControlOrMeta+KeyK', 'ControlOrMeta+Slash']) {
    await openPalette(page, key);
    const box = (await palette(page).boundingBox())!;
    const canvas = (await page.locator('canvas').boundingBox())!;
    expect(Math.abs(box.x + box.width / 2 - (canvas.x + canvas.width / 2))).toBeLessThanOrEqual(1);
    expect(box.y - canvas.y).toBeLessThanOrEqual(24);
    await page.keyboard.press('Escape');
    await expect(palette(page)).toHaveCount(0);
    await expect(page.locator('canvas')).toBeFocused();
  }
  // Сочетание в самом поле закрывает палитру.
  await openPalette(page);
  await page.keyboard.press('ControlOrMeta+KeyK');
  await expect(palette(page)).toHaveCount(0);
  // Щелчок мимо закрывает, выделение и шаги не меняются от самой палитры.
  await openPalette(page);
  await page.locator('canvas').click({ position: { x: 10, y: 300 } });
  await expect(palette(page)).toHaveCount(0);
  expect(await warnings(page)).toEqual([]);
});

test('команды: название, группа и сочетание; серая не выполняется; Enter выполняет и закрывает', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('дубл');
  const dup = list(page).getByRole('option', { name: /Дублировать/ }).first();
  await expect(dup).toHaveAttribute('data-command', 'edit.duplicate');
  await expect(dup).toContainText('Правка');
  await expect(dup).toContainText(/D/);
  // Ничего не выделено — серая: Enter и щелчок не выполняют, палитра открыта.
  await expect(dup).toHaveAttribute('aria-disabled', 'true');
  const before = await live(page);
  await page.keyboard.press('Enter');
  await dup.click({ force: true });
  await expect(palette(page)).toBeVisible();
  expect((await live(page)).steps).toBe(before.steps);
  await page.keyboard.press('Escape');

  // Выделяем слой — та же команда доступна и даёт один шаг.
  const at = await appScreen(page, before, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await openPalette(page);
  await field(page).fill('дубл');
  await expect(active(page)).toHaveAttribute('data-command', 'edit.duplicate');
  await expect(active(page)).not.toHaveAttribute('aria-disabled', 'true');
  await page.keyboard.press('Enter');
  await expect(palette(page)).toHaveCount(0);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
});

test('стрелки ходят по строкам, в том числе по серым и по кругу; команда интерфейса выполняется', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('настройки');
  const opts = list(page).getByRole('option');
  await expect(opts.first()).toHaveAttribute('aria-selected', 'true');
  await expect(active(page)).toHaveAttribute('data-command', 'ui.settings');
  await page.keyboard.press('ArrowUp');
  await expect(opts.last()).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('ArrowDown');
  await expect(opts.first()).toHaveAttribute('aria-selected', 'true');
  const activeId = await field(page).getAttribute('aria-activedescendant');
  expect(activeId).toBe(await opts.first().getAttribute('id'));
  await page.keyboard.press('Enter');
  await expect(page.getByRole('dialog', { name: 'Настройки' })).toBeVisible();
  await expect(palette(page)).toHaveCount(0);
  await page.keyboard.press('Escape');
  // Длинный список: выбранная строка прокручивается в видимую часть.
  await openPalette(page);
  for (let i = 0; i < 25; i++) await page.keyboard.press('ArrowDown');
  await expect(active(page)).toBeInViewport({ ratio: 1 });
  expect(await warnings(page)).toEqual([]);
});

test('раскладка не важна: «щквук» находит команды порядка, «ё» и регистр — тоже', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('щквук');
  await expect(list(page).getByRole('option', { name: /На передний план/ })).toBeVisible();
  await field(page).fill('ПЕРЕДНИЙ');
  await expect(list(page).getByRole('option', { name: /На передний план/ })).toBeVisible();
  await field(page).fill('темная');
  await expect(list(page).getByRole('option', { name: /Тёмная тема/ })).toBeVisible();
  await field(page).fill('нет такой команды');
  await expect(palette(page).getByText('Ничего не найдено')).toBeVisible();
  expect(await warnings(page)).toEqual([]);
});

test('слои: поиск в worker\'е, переход выделяет слой, показывает его и раскрывает предков', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  // Фрейм «Экран 9» на странице-примере; ищем в другой раскладке.
  await field(page).fill("'rhfy 9");
  const row = list(page).getByRole('option', { name: 'Экран 9' });
  await expect(row).toBeVisible();
  await expect(palette(page).getByText('Слои', { exact: true })).toBeVisible();
  await row.click();
  await expect(palette(page)).toHaveCount(0);
  const after = await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 1440');
  // Фрейм в виде целиком (1440×1024 — вписан, если не влезал).
  const tree = page.getByRole('tree', { name: 'Слои' });
  await expect(tree.getByRole('treeitem', { name: 'Экран 9', exact: true })).toHaveAttribute('aria-selected', 'true');
  expect(after.zoom).toBeGreaterThan(0);
  // Вложенный слой: предок раскрыт, строка выделена.
  await openPalette(page);
  await field(page).fill('слой 5');
  const child = list(page).getByRole('option', { name: 'Слой 5', exact: true }).first();
  await expect(child).toBeVisible();
  await child.click();
  await expect(tree.getByRole('treeitem', { name: 'Слой 5', exact: true })).toHaveAttribute('aria-selected', 'true');
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 120');
  expect(await warnings(page)).toEqual([]);
});

test('ответ worker\'а на прежний запрос не показывается', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('слой 42');
  await expect(list(page).getByRole('option', { name: 'Слой 42', exact: true }).first()).toBeVisible();
  // Новый запрос, а ядро ещё раз отвечает на старый (ответ пришёл после нового).
  await field(page).fill('экран 1');
  await expect(list(page).getByRole('option', { name: 'Экран 1', exact: true })).toBeVisible();
  const answers = () => page.evaluate(() => window.__ferrum!.uiEvents!.filter((e) => e.type === 'layersFound').length);
  const was = await answers();
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'findLayers', query: 'слой 42', limit: 50 }));
  await expect.poll(answers).toBe(was + 1);
  await page.waitForTimeout(100);
  await expect(list(page).getByRole('option', { name: 'Слой 42', exact: true })).toHaveCount(0);
  await expect(list(page).getByRole('option', { name: 'Экран 1', exact: true })).toBeVisible();
});

test('страницы: поиск по имени и переход', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Добавить страницу' }).click();
  await page.waitForFunction(() => window.__ferrum!.ui!.pages.list.length === 2);
  const first = await page.evaluate(() => window.__ferrum!.ui!.pages.list[0]);
  const second = await page.evaluate(() => window.__ferrum!.ui!.pages.current);
  expect(second).not.toBe(first.id);
  await openPalette(page);
  await field(page).fill(first.name);
  await expect(palette(page).getByText('Страницы', { exact: true })).toBeVisible();
  await list(page).getByRole('option', { name: first.name, exact: true }).last().click();
  await expect(palette(page)).toHaveCount(0);
  await page.waitForFunction((id) => window.__ferrum!.ui!.pages.current === id, first.id);
});

test('недавние: последние команды первыми, переживают перезагрузку; пункт меню тоже попадает', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await expect(palette(page).getByText('Недавние', { exact: true })).toHaveCount(0);
  await field(page).fill('показать все');
  await expect(active(page)).toHaveAttribute('data-command', 'view.fit_all');
  await page.keyboard.press('Enter');
  await openPalette(page);
  await field(page).fill('масштаб 100');
  await expect(active(page)).toHaveAttribute('data-command', 'view.zoom_100');
  await page.keyboard.press('Enter');
  await openPalette(page);
  await expect(palette(page).getByText('Недавние', { exact: true })).toBeVisible();
  const opts = list(page).getByRole('option');
  await expect(opts.nth(0)).toHaveAttribute('data-command', 'view.zoom_100');
  await expect(opts.nth(1)).toHaveAttribute('data-command', 'view.fit_all');
  await page.keyboard.press('Escape');
  await expect.poll(() => stored(page)).toMatchObject({ recentCommands: 'view.zoom_100 view.fit_all' });
  await page.reload();
  await editorReady(page);
  await openPalette(page);
  await expect(list(page).getByRole('option').nth(0)).toHaveAttribute('data-command', 'view.zoom_100');
  // Недавняя команда выше при поиске: «показать» — и «Показать всё» первой.
  await field(page).fill('показать');
  await expect(active(page)).toHaveAttribute('data-command', 'view.fit_all');
  await page.keyboard.press('Escape');
  // Команда из главного меню — тоже в недавних.
  await mainMenu(page, 'Правка', 'Выделить всё');
  await openPalette(page);
  await expect(list(page).getByRole('option').nth(0)).toHaveAttribute('data-command', 'edit.select_all');
});

test('в поле — обычный ввод: буквы не вызывают инструменты, стрелки не двигают выделение', async ({ page }) => {
  await openApp(page);
  const before = await live(page);
  await openPalette(page);
  await page.keyboard.type('rvl');
  await expect(field(page)).toHaveValue('rvl');
  await page.keyboard.press('ArrowLeft');
  const l = await live(page);
  expect(l.steps).toBe(before.steps);
  expect(await page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('move');
});

test('ввод в палитру на 20 000 узлов: без длинных задач и чтений раскладки вне measure @perf', async ({ page }, info) => {
  test.skip(info.project.name !== 'chromium', 'длинные задачи — только в Chromium');
  await openApp(page, 20_000);
  await page.evaluate(() => {
    const w = window as unknown as { __long: number[] };
    w.__long = [];
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) w.__long.push(e.duration);
    }).observe({ type: 'longtask', buffered: false });
  });
  await openPalette(page);
  for (const c of 'слой 42') await page.keyboard.type(c);
  await expect(list(page).getByRole('option', { name: 'Слой 42', exact: true }).first()).toBeVisible();
  for (let i = 0; i < 30; i++) await page.keyboard.press('ArrowDown');
  await page.waitForTimeout(100);
  expect(await page.evaluate(() => (window as unknown as { __long: number[] }).__long)).toEqual([]);
  expect(await warnings(page)).toEqual([]);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/palette.spec.ts --project=chromium --grep-invert @perf`
Expected: FAIL — новые тесты `palette.spec.ts` падают: Ctrl/Cmd+K доходит до ядра, `UiCommand` приходит, но интерфейс не знает `ui.quick_actions`: поле `combobox` «Быстрые действия» не появляется и не получает фокус

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { PagesPanel } from './pages';
import { PropertiesPanel } from './properties';
import { diffSettings, resolveTheme, type Settings, type ThemeChoice } from './settings';
import type { SettingsStore } from './settings-store';
````

на:

````tsx
import { PagesPanel } from './pages';
import { PropertiesPanel } from './properties';
import { QuickActions } from './quick-actions';
import { diffSettings, resolveTheme, type Settings, type ThemeChoice } from './settings';
import type { SettingsStore } from './settings-store';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  // таблицы команд. Файла на диске без File System Access нет: его пунктов тоже.
  const [mainMenu, setMainMenu] = createSignal(false);
  const hidden = createMemo(() => new Set(props.files.disk ? [] : ['ui.open_disk', 'ui.save_disk']));
  const entries = createMemo(() => entriesByKey(store.state.keymap.commands));
````

на:

````tsx
  // таблицы команд. Файла на диске без File System Access нет: его пунктов тоже.
  const [mainMenu, setMainMenu] = createSignal(false);
  /** Открыты ли «Быстрые действия» (спека подсистемы 2, раздел 7). */
  const [palette, setPalette] = createSignal(false);
  const hidden = createMemo(() => new Set(props.files.disk ? [] : ['ui.open_disk', 'ui.save_disk']));
  const entries = createMemo(() => entriesByKey(store.state.keymap.commands));
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      case 'ui.main_menu':
        setMainMenu(true);
        break;
      case 'ui.new_file':
````

на:

````tsx
      case 'ui.main_menu':
        setMainMenu(true);
        break;
      case 'ui.quick_actions':
        setPalette(true);
        break;
      case 'ui.new_file':
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
            <EditorContext.Provider value={ed()}>
              <Toolbar />
            </EditorContext.Provider>
          )}
````

на:

````tsx
            <EditorContext.Provider value={ed()}>
              <Toolbar />
              <Show when={palette()}>
                <QuickActions hidden={hidden()} recent={recent()} onClose={() => setPalette(false)} />
              </Show>
            </EditorContext.Provider>
          )}
````

В `apps/web/src/ui/layers.tsx` заменить:

````tsx
const DRAG_START_PX = 4;

const KIND_ICON: Record<NodeType, IconName> = {
  frame: 'frame',
  group: 'group',
````

на:

````tsx
const DRAG_START_PX = 4;

export const KIND_ICON: Record<NodeType, IconName> = {
  frame: 'frame',
  group: 'group',
````

Создать `apps/web/src/ui/quick-actions.tsx`:

````tsx
// «Быстрые действия» (спека подсистемы 2, раздел 7; Ctrl/Cmd+K и Ctrl/Cmd+/, как
// в Figma): поле сверху по центру холста и один список с заголовками — недавние,
// команды, слои, страницы. Фокус остаётся в поле (комбобокс и `aria-activedescendant`):
// стрелки ходят по строкам, в том числе по серым, Enter выполняет, Esc и щелчок мимо
// закрывают, фокус возвращается туда, откуда открыли. Команда выполняется, как из
// главного меню (`ed.run`, включая `ui.*`). Слои ищет worker: каждый символ — новый
// запрос `FindLayers`, ответы на прежние отбрасываются по `query`. Положение задаёт
// CSS; прокрутка к строке читает раскладку только в фазе measure.

import { createEffect, createMemo, createSignal, Index, Match, on, onCleanup, onMount, Show, Switch, type JSX } from 'solid-js';
import type { FoundLayerDto, KeymapEntry } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { Icon } from './icons';
import { GROUPS } from './keymap-ui';
import { KIND_ICON } from './layers';
import { entriesByKey } from './main-menu';
import { clipQuery, LAYER_LIMIT, paletteRows, type PaletteRow } from './palette';
import { eventShortcut, shortcutLabel } from './shortcuts';

type Action = Exclude<PaletteRow, { kind: 'heading' }>;

const GROUP_TITLE = new Map(GROUPS.map((g) => [g.id, g.title]));

export interface QuickActionsProps {
  /** Команды, которых нет на этой платформе. */
  hidden: ReadonlySet<string>;
  /** Недавние команды, новые первыми. */
  recent: readonly string[];
  onClose: () => void;
}

export function QuickActions(props: QuickActionsProps): JSX.Element {
  const ed = useEditor();
  const returnFocus = document.activeElement as HTMLElement | null;
  const [query, setQuery] = createSignal('');
  /** Ответ worker'а на последний отправленный запрос. */
  const [layers, setLayers] = createSignal<readonly FoundLayerDto[]>([]);
  /** Номер строки с действием среди строк без заголовков. */
  const [active, setActive] = createSignal(0);
  let root!: HTMLDivElement;
  let list!: HTMLDivElement;
  let input!: HTMLInputElement;
  let sent = '';

  const entries = createMemo(() => entriesByKey(ed.store.state.keymap.commands));
  const rows = createMemo(() =>
    paletteRows({
      query: query(),
      commands: ed.store.state.keymap.commands,
      hidden: props.hidden,
      recent: props.recent,
      layers: layers(),
      pages: ed.store.state.pages.list,
    }),
  );
  const actions = createMemo(() => rows().filter((r): r is Action => r.kind !== 'heading'));
  /** Номер строки с действием для строки списка (заголовки не считаются). */
  const actionIndex = createMemo(() => {
    let n = 0;
    return rows().map((r) => (r.kind === 'heading' ? -1 : n++));
  });
  const disabled = (r: Action) => r.kind === 'command' && ed.store.state.commands[r.key]?.enabled === false;

  const close = (restore: boolean) => {
    props.onClose();
    if (restore) returnFocus?.focus?.();
  };
  const execute = (r: Action | undefined) => {
    if (!r || disabled(r)) return;
    close(true);
    if (r.kind === 'command') {
      ed.noteRecent?.(r.key);
      ed.run(r.key);
    } else if (r.kind === 'layer') ed.send({ type: 'revealLayer', id: r.id });
    else ed.send({ type: 'setPage', id: r.id });
  };

  // Новый список — выбрана первая доступная строка.
  createEffect(
    on(actions, (a) => {
      const i = a.findIndex((r) => !disabled(r));
      setActive(Math.max(0, i));
    }),
  );
  // Выбранная строка — в видимой части списка: чтение в measure, запись в mutate.
  createEffect(
    on(active, (i) => {
      ed.scheduler.measure(() => {
        const el = list?.querySelector<HTMLElement>(`[data-action="${i}"]`);
        if (!el) return;
        const top = el.offsetTop;
        const bottom = top + el.offsetHeight;
        const { scrollTop, clientHeight } = list;
        if (top < scrollTop) ed.scheduler.mutate(() => (list.scrollTop = top));
        else if (bottom > scrollTop + clientHeight) ed.scheduler.mutate(() => (list.scrollTop = bottom - clientHeight));
      });
    }),
  );

  const search = (q: string) => {
    setQuery(q);
    sent = clipQuery(q.trim());
    if (sent) ed.send({ type: 'findLayers', query: sent, limit: LAYER_LIMIT });
    else setLayers([]);
  };

  const onKey = (e: KeyboardEvent) => {
    const n = actions().length;
    switch (e.key) {
      case 'ArrowDown':
        if (n) setActive((active() + 1) % n);
        break;
      case 'ArrowUp':
        if (n) setActive((active() - 1 + n) % n);
        break;
      case 'Enter':
        execute(actions()[active()]);
        break;
      case 'Escape':
        close(true);
        break;
      case 'Tab':
        close(false);
        return;
      default: {
        // Сочетание палитры в её же поле закрывает её, как в Figma.
        const s = eventShortcut(e);
        if (!s || !entries().get('ui.quick_actions')?.bindings.includes(s)) return;
        close(true);
      }
    }
    e.preventDefault();
    e.stopPropagation();
  };

  onMount(() => {
    input.focus();
    // Ответы — через очередь планировщика; ответ на прежний запрос не нужен.
    const off = ed.core.on((e) => {
      if (e.type !== 'layersFound') return;
      ed.scheduler.enqueue(() => {
        if (e.query === sent) setLayers(e.items);
      });
    });
    const outside = (e: PointerEvent) => {
      if (!(e.target instanceof Node && root.contains(e.target))) close(false);
    };
    document.addEventListener('pointerdown', outside, true);
    onCleanup(() => {
      off();
      document.removeEventListener('pointerdown', outside, true);
    });
  });

  const key = (k: string) => {
    const e: KeymapEntry | undefined = entries().get(k);
    return e?.bindings[0] ? shortcutLabel(e.bindings[0], ed.platform, ed.layout()) : '';
  };
  const optionId = (i: number) => `fe-palette-option-${i}`;

  const Row = (p: { row: () => Action; at: () => number }) => {
    const entry = () => {
      const r = p.row();
      return r.kind === 'command' ? entries().get(r.key) : undefined;
    };
    return (
      <div
        id={optionId(p.at())}
        class="fe-palette__row"
        classList={{ 'fe-palette__row--active': active() === p.at() }}
        role="option"
        aria-selected={active() === p.at()}
        aria-disabled={disabled(p.row()) ? 'true' : undefined}
        data-action={p.at()}
        data-command={entry()?.key}
        onPointerMove={() => {
          if (active() !== p.at()) setActive(p.at());
        }}
        onClick={() => execute(p.row())}
      >
        <span class="fe-palette__icon">
          <Switch>
            <Match when={p.row().kind === 'layer' && (p.row() as Extract<Action, { kind: 'layer' }>)}>
              {(l) => <Icon name={KIND_ICON[l().type]} />}
            </Match>
            <Match when={p.row().kind === 'page'}>
              <Icon name="file" />
            </Match>
            <Match when={entry() && ed.store.state.commands[entry()!.key]?.checked}>
              <Icon name="check" />
            </Match>
          </Switch>
        </span>
        <span class="fe-palette__name">{entry()?.name ?? (p.row() as Exclude<Action, { kind: 'command' }>).name}</span>
        <Show when={entry()}>
          {(c) => (
            <>
              <span class="fe-palette__group">{GROUP_TITLE.get(c().group) ?? c().group}</span>
              <span class="fe-palette__key">{key(c().key)}</span>
            </>
          )}
        </Show>
      </div>
    );
  };

  return (
    <div ref={root} class="fe-palette" role="dialog" aria-label="Быстрые действия">
      <div class="fe-palette__field">
        <Icon name="search" />
        <input
          ref={input}
          type="text"
          role="combobox"
          aria-label="Быстрые действия"
          aria-expanded="true"
          aria-controls="fe-palette-list"
          aria-autocomplete="list"
          aria-activedescendant={actions().length ? optionId(active()) : undefined}
          placeholder="Команда, слой или страница"
          autocomplete="off"
          spellcheck={false}
          value={query()}
          onInput={(e) => search(e.currentTarget.value)}
          onKeyDown={onKey}
        />
      </div>
      {/* Щелчок по списку не забирает фокус из поля. */}
      <div ref={list} id="fe-palette-list" class="fe-palette__list" role="listbox" aria-label="Результаты" onPointerDown={(e) => e.preventDefault()}>
        <Show when={rows().length === 0}>
          <div class="fe-palette__empty">Ничего не найдено</div>
        </Show>
        {/* Index, не For: узлы строк остаются, ввод не пересоздаёт список. */}
        <Index each={rows()}>
          {(row, i) => (
            <Switch>
              <Match when={row().kind === 'heading' && (row() as Extract<PaletteRow, { kind: 'heading' }>)}>
                {(h) => (
                  <div class="fe-palette__heading" role="presentation">
                    {h().title}
                  </div>
                )}
              </Match>
              <Match when={row().kind !== 'heading'}>
                <Row row={() => row() as Action} at={() => actionIndex()[i]} />
              </Match>
            </Switch>
          )}
        </Index>
      </div>
    </div>
  );
}
````

В `apps/web/src/ui/styles/chrome.css` заменить:

````css
  padding-bottom: var(--space-1);
}
/* Кнопка с логотипом: главное меню (как в Figma UI3). */
.fe-logo {
````

на:

````css
  padding-bottom: var(--space-1);
}
/* «Быстрые действия»: поле сверху по центру холста, список под ним. */
.fe-palette {
  position: absolute;
  z-index: 40;
  top: var(--space-4);
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  flex-direction: column;
  width: min(560px, calc(100% - 32px));
  max-height: calc(100% - 32px);
  padding: var(--space-1);
  border-radius: var(--radius-md);
  background: var(--surface-raised);
  box-shadow: var(--shadow-popover);
  color: var(--ink);
  font-size: 13px;
  line-height: 20px;
}
.fe-palette__field {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--space-2);
  padding: 0 var(--space-2);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--ink-muted);
}
.fe-palette__field:focus-within {
  outline: 2px solid var(--select);
  outline-offset: -2px;
}
.fe-palette__field input {
  flex: 1;
  min-width: 0;
  height: 32px;
  border: 0;
  outline: 0;
  background: transparent;
  color: var(--ink);
  font: inherit;
}
.fe-palette__list {
  position: relative;
  overflow: auto;
  min-height: 0;
  max-height: 420px;
  margin-top: var(--space-1);
}
.fe-palette__heading {
  padding: var(--space-1) var(--space-2) 0;
  color: var(--ink-muted);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.01em;
}
.fe-palette__empty {
  padding: var(--space-2);
  color: var(--ink-muted);
}
.fe-palette__row {
  display: grid;
  grid-template-columns: 16px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: var(--space-2);
  min-height: var(--row-height);
  padding: 0 var(--space-2);
  border-radius: var(--radius-sm);
  cursor: default;
  white-space: nowrap;
}
.fe-palette__row--active {
  background: var(--surface-hover);
}
.fe-palette__row[aria-disabled='true'] {
  color: var(--ink-faint);
}
.fe-palette__icon {
  display: flex;
  align-items: center;
  color: var(--ink-muted);
}
.fe-palette__name {
  overflow: hidden;
  text-overflow: ellipsis;
}
.fe-palette__group,
.fe-palette__key {
  color: var(--ink-muted);
  font-size: 12px;
}
.fe-palette__key {
  min-width: 48px;
  text-align: right;
}
.fe-palette__row[aria-disabled='true'] .fe-palette__group,
.fe-palette__row[aria-disabled='true'] .fe-palette__key,
.fe-palette__row[aria-disabled='true'] .fe-palette__icon {
  color: var(--ink-faint);
}
/* Кнопка с логотипом: главное меню (как в Figma UI3). */
.fe-logo {
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/palette.spec.ts e2e/main-menu.spec.ts e2e/app.spec.ts --project=chromium`

Expected: модульные и сборка — ок; E2E `palette.spec.ts` (10 тестов, с `@perf`: без длинных задач, `__ferrumWarnings` пуст), `main-menu`, `app` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/palette.spec.ts apps/web/src/ui/app.tsx apps/web/src/ui/layers.tsx apps/web/src/ui/quick-actions.tsx apps/web/src/ui/styles/chrome.css
git commit -F - <<'EOF'
feat(web): палитра «Быстрые действия» (Ctrl/Cmd+K и Ctrl/Cmd+/)

Поле сверху по центру холста и один список с заголовками: недавние, команды
(название, группа, сочетание; недоступные серые и не выполняются), слои из
worker'а (`FindLayers`; ответы на прежние запросы отбрасываются по `query`,
переход — `RevealLayer`) и страницы. Фокус остаётся в поле, стрелки ходят по
строкам, Enter выполняет, как главное меню, Esc и щелчок мимо закрывают.
Положение задаёт CSS; прокрутка к строке читает раскладку только в measure.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 6: Панель «Сочетания клавиш»

Спека §8: панель поднимается снизу окна, вкладки в порядке вкладки настроек, клавиши из действующей раскладки, прочерк у команд без сочетания; использованные отмечены точкой и цветом — недавние и вызванные сочетанием за сеанс (сопоставление с раскладкой в основном потоке, без новых данных в протоколе). «Справка → Сочетания клавиш» теперь открывает панель, а не настройки. Окно не модальное: холст работает, то же сочетание закрывает панель и с холста.

**Files:**
- Create: `apps/web/src/ui/shortcuts-panel.tsx`
- Modify: `apps/web/src/ui/{app.tsx, boot.ts, palette.ts}`, `apps/web/src/ui/styles/chrome.css`
- Test: `apps/web/unit/palette.test.ts`, `apps/web/e2e/shortcuts-panel.spec.ts` (новый)

**Interfaces:**
- Consumes: `groupCommands` (`keymap-ui`), `store.state.keymap.commands`, `shortcutLabel`, `eventShortcut`, `forwardKey`, `recent` (задача 4), диалог настроек.
- Produces: `ShortcutsPanel(props: { used: ReadonlySet<string>; onSettings: () => void; onClose: () => void })`; `BootOptions.onForwardedKey?: (e: KeyboardEvent) => void`; `bindingsToCommands(commands): Map<string, string>`; сигналы `shortcuts`, `keysUsed` в `App`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/e2e/shortcuts-panel.spec.ts`:

````ts
// Панель «Сочетания клавиш» (спека подсистемы 2, раздел 8): Ctrl+Shift+? и пункт
// «Справка → Сочетания клавиш», вкладки по группам, клавиши из действующей раскладки,
// отметка использованных недавно, ссылка в настройки. Открытие не читает раскладку
// вне фазы measure.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

const panel = (page: Page) => page.getByRole('dialog', { name: 'Сочетания клавиш' });
const row = (page: Page, key: string) => panel(page).locator(`[data-command="${key}"]`);
const tab = (page: Page, name: string) => panel(page).getByRole('tab', { name, exact: true });
const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

async function openByKey(page: Page): Promise<void> {
  await page.locator('canvas').focus();
  await page.keyboard.press('Control+Shift+Slash');
  await expect(panel(page)).toBeVisible();
}

test('Ctrl+Shift+? поднимает панель снизу окна; то же сочетание и Esc закрывают', async ({ page }) => {
  await openApp(page);
  await openByKey(page);
  await panel(page).evaluate((el) => Promise.all(el.getAnimations().map((a) => a.finished)));
  const box = (await panel(page).boundingBox())!;
  const size = page.viewportSize()!;
  expect(box.x).toBe(0);
  expect(box.width).toBe(size.width);
  expect(box.y + box.height).toBe(size.height);
  expect(box.height).toBeLessThanOrEqual(size.height / 2);
  // Фокус — на выбранной вкладке, клавиши холсту не уходят.
  await expect(tab(page, 'Инструменты')).toBeFocused();
  await page.keyboard.press('KeyR');
  expect(await page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('move');
  await page.keyboard.press('Control+Shift+Slash');
  await expect(panel(page)).toHaveCount(0);
  await expect(page.locator('canvas')).toBeFocused();
  await openByKey(page);
  await page.keyboard.press('Escape');
  await expect(panel(page)).toHaveCount(0);
  await expect(page.locator('canvas')).toBeFocused();
  expect(await warnings(page)).toEqual([]);
});

test('панель не модальная: холст работает, сочетание с холста её закрывает', async ({ page }) => {
  await openApp(page);
  await openByKey(page);
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await expect(panel(page)).toBeVisible();
  await page.keyboard.press('Control+Shift+Slash');
  await expect(panel(page)).toHaveCount(0);
});

test('пункт «Справка → Сочетания клавиш» и вкладки: клавиши из действующей раскладки', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Справка', 'Сочетания клавиш');
  await expect(panel(page)).toBeVisible();
  await expect(panel(page).getByRole('tab')).toHaveText(['Инструменты', 'Правка', 'Объект', 'Текст', 'Вектор', 'Порядок', 'Сдвиг', 'Вид', 'Файл', 'Настройки', 'Справка']);
  await expect(row(page, 'tool.rect')).toContainText('Прямоугольник');
  await expect(row(page, 'tool.rect').locator('kbd')).toHaveText(['R']);
  await expect(row(page, 'tool.frame').locator('kbd')).toHaveText(['F', 'A']);
  // Стрелки ходят по вкладкам, в «Правке» — её команды.
  await tab(page, 'Инструменты').focus();
  await page.keyboard.press('ArrowRight');
  await expect(tab(page, 'Правка')).toBeFocused();
  await expect(tab(page, 'Правка')).toHaveAttribute('aria-selected', 'true');
  await expect(row(page, 'edit.duplicate')).toBeVisible();
  await expect(row(page, 'tool.rect')).toHaveCount(0);
  // Команда без сочетания — прочерк.
  await tab(page, 'Файл').click();
  await expect(row(page, 'ui.new_file')).toContainText('—');
  await expect(row(page, 'ui.new_file').locator('kbd')).toHaveCount(0);
  // Новое сочетание в раскладке — панель показывает и его.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'bind', command: 'tool.rect', shortcut: 'KeyQ', replace: false }));
  await tab(page, 'Инструменты').click();
  await expect(row(page, 'tool.rect').locator('kbd')).toHaveText(['R', 'Q']);
});

test('использованные недавно отмечены: команда из меню и сочетание на холсте', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Вид', 'Масштаб 100%');
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyR');
  await page.keyboard.press('Escape');
  await openByKey(page);
  await expect(row(page, 'tool.rect')).toHaveClass(/fe-keys__row--used/);
  await expect(row(page, 'tool.rect').getByRole('img', { name: 'использовалась недавно' })).toBeVisible();
  await expect(row(page, 'tool.ellipse')).not.toHaveClass(/fe-keys__row--used/);
  await tab(page, 'Вид').click();
  await expect(row(page, 'view.zoom_100')).toHaveClass(/fe-keys__row--used/);
  await expect(row(page, 'view.fit_all')).not.toHaveClass(/fe-keys__row--used/);
});

test('«Изменить в настройках» открывает настройки на вкладке сочетаний', async ({ page }) => {
  await openApp(page);
  await openByKey(page);
  await panel(page).getByRole('button', { name: 'Изменить в настройках' }).click();
  await expect(panel(page)).toHaveCount(0);
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('tab', { name: 'Горячие клавиши' })).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  expect(await warnings(page)).toEqual([]);
});
````

В `apps/web/unit/palette.test.ts` заменить:

````ts
import type { FoundLayerDto, KeymapEntry } from '../src/wasm/ferrum_wasm';
import {
  clipQuery,
  COMMANDS_MAX,
````

на:

````ts
import type { FoundLayerDto, KeymapEntry } from '../src/wasm/ferrum_wasm';
import {
  bindingsToCommands,
  clipQuery,
  COMMANDS_MAX,
````

В `apps/web/unit/palette.test.ts` заменить:

````ts
  expect(clipped).toBe('я'.repeat(128));
});
````

на:

````ts
  expect(clipped).toBe('я'.repeat(128));
});

test('сочетание → команда: первая в таблице, если занято двумя', () => {
  const list = [
    { ...entry('edit.undo', 'Отменить'), bindings: ['Ctrl+KeyZ'] },
    { ...entry('edit.redo', 'Повторить'), bindings: ['Ctrl+Shift+KeyZ', 'Ctrl+KeyY'] },
    { ...entry('x.same', 'Другая'), bindings: ['Ctrl+KeyZ'] },
  ];
  const m = bindingsToCommands(list);
  expect(m.get('Ctrl+KeyZ')).toBe('edit.undo');
  expect(m.get('Ctrl+KeyY')).toBe('edit.redo');
  expect(m.get('KeyQ')).toBeUndefined();
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/palette.test.ts`
Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/shortcuts-panel.spec.ts --project=chromium`
Expected: FAIL — модульные — `bindingsToCommands is not a function` в новом тесте `palette.test.ts`; E2E (стенд собран) — новые тесты `shortcuts-panel.spec.ts` падают: Ctrl+Shift+? и «Справка → Сочетания клавиш» открывают настройки, панели `Сочетания клавиш` нет

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { Menu, type MenuItem, type MenuSearch } from './menu';
import { buildMainMenu, entriesByKey, searchMenu } from './main-menu';
import { parseRecent, pushRecent } from './palette';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
````

на:

````tsx
import { Menu, type MenuItem, type MenuSearch } from './menu';
import { buildMainMenu, entriesByKey, searchMenu } from './main-menu';
import { bindingsToCommands, parseRecent, pushRecent } from './palette';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import type { SettingsStore } from './settings-store';
import { SettingsDialog } from './settings-dialog';
import { downloadBytes, fileName } from './files';
import { importFromDisk, type FileMeta, type FileStore } from './file-store';
````

на:

````tsx
import type { SettingsStore } from './settings-store';
import { SettingsDialog } from './settings-dialog';
import { ShortcutsPanel } from './shortcuts-panel';
import { downloadBytes, fileName } from './files';
import { importFromDisk, type FileMeta, type FileStore } from './file-store';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        if (!s || !action) return false;
        const key = { copy: 'edit.copy', cut: 'edit.cut', paste: 'edit.paste', pasteOver: 'edit.paste_over' }[action];
        return !!store.state.keymap.commands.find((c) => c.key === key)?.bindings.includes(s);
      },
    })
````

на:

````tsx
        if (!s || !action) return false;
        const key = { copy: 'edit.copy', cut: 'edit.cut', paste: 'edit.paste', pasteOver: 'edit.paste_over' }[action];
        const bound = !!store.state.keymap.commands.find((c) => c.key === key)?.bindings.includes(s);
        if (bound) noteKeyUsed(key);
        return bound;
      },
      onForwardedKey: (e) => {
        if (e.repeat) return;
        const s = eventShortcut(e);
        const key = s && bindingIndex().get(s);
        if (key) noteKeyUsed(key);
      },
    })
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  /** Открыты ли «Быстрые действия» (спека подсистемы 2, раздел 7). */
  const [palette, setPalette] = createSignal(false);
  const hidden = createMemo(() => new Set(props.files.disk ? [] : ['ui.open_disk', 'ui.save_disk']));
  const entries = createMemo(() => entriesByKey(store.state.keymap.commands));
````

на:

````tsx
  /** Открыты ли «Быстрые действия» (спека подсистемы 2, раздел 7). */
  const [palette, setPalette] = createSignal(false);
  /** Открыта ли панель сочетаний (раздел 8). */
  const [shortcuts, setShortcuts] = createSignal(false);
  /** Команды, вызванные сочетанием за этот сеанс (в настройки не пишутся): отмечены в
   * панели вместе с недавними. Клавиша сопоставляется по действующей раскладке; что
   * сделало ядро (например, ввод в правке текста), панели не важно. */
  const [keysUsed, setKeysUsed] = createSignal<ReadonlySet<string>>(new Set());
  const noteKeyUsed = (key: string) => {
    if (!keysUsed().has(key)) setKeysUsed(new Set([...keysUsed(), key]));
  };
  const bindingIndex = createMemo(() => bindingsToCommands(store.state.keymap.commands));
  const usedCommands = createMemo(() => new Set([...recent(), ...keysUsed()]));
  const hidden = createMemo(() => new Set(props.files.disk ? [] : ['ui.open_disk', 'ui.save_disk']));
  const entries = createMemo(() => entriesByKey(store.state.keymap.commands));
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        break;
      case 'ui.settings':
      case 'ui.shortcuts':
        setSettingsOpen(true);
        break;
      case 'ui.about':
````

на:

````tsx
        break;
      case 'ui.settings':
        setSettingsOpen(true);
        break;
      case 'ui.shortcuts':
        // Как в Figma: то же сочетание закрывает панель.
        setShortcuts((v) => !v);
        break;
      case 'ui.about':
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
            </aside>
            <TooltipLayer />
            <Show when={settingsOpen()}>
              <SettingsDialog
````

на:

````tsx
            </aside>
            <TooltipLayer />
            <Show when={shortcuts()}>
              <ShortcutsPanel used={usedCommands()} onSettings={() => setSettingsOpen(true)} onClose={() => setShortcuts(false)} />
            </Show>
            <Show when={settingsOpen()}>
              <SettingsDialog
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
  /** Клавиша — сочетание буфера обмена, которое ведут события copy/cut/paste. */
  isClipboardKey?: (e: KeyboardEvent) => boolean;
  /** Перед документом: настройки пользователя (тема, раскладка, привязка) — до первого кадра. */
  beforeDocument?: (editor: Editor) => void;
````

на:

````ts
  /** Клавиша — сочетание буфера обмена, которое ведут события copy/cut/paste. */
  isClipboardKey?: (e: KeyboardEvent) => boolean;
  /** Клавиша ушла ядру (не полю и не меню): панель сочетаний отмечает использованные. */
  onForwardedKey?: (e: KeyboardEvent) => void;
  /** Перед документом: настройки пользователя (тема, раскладка, привязка) — до первого кадра. */
  beforeDocument?: (editor: Editor) => void;
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
    test,
    transport: choice<TransportMode>(params, 'transport', ['sab', 'message']),
    keyFilter: (e) => forwardKey(e, isClipboardKey),
    // Свой формат не поддерживается: текст с меткой для буфера готовит worker.
    clipboardText: !supportsCustomFormat(),
````

на:

````ts
    test,
    transport: choice<TransportMode>(params, 'transport', ['sab', 'message']),
    keyFilter: (e) => {
      const forward = forwardKey(e, isClipboardKey);
      if (forward) opts.onForwardedKey?.(e);
      return forward;
    },
    // Свой формат не поддерживается: текст с меткой для буфера готовит worker.
    clipboardText: !supportsCustomFormat(),
````

В `apps/web/src/ui/palette.ts` заменить:

````ts
  if (!KEY.test(key)) return [...recent];
  return [key, ...recent.filter((k) => k !== key)].slice(0, RECENT_MAX);
}

````

на:

````ts
  if (!KEY.test(key)) return [...recent];
  return [key, ...recent.filter((k) => k !== key)].slice(0, RECENT_MAX);
}

/** Сочетание → ключ команды (первая в таблице, если занято двумя): для отметки
 * использованных сочетаний панелью сочетаний. */
export function bindingsToCommands(commands: readonly KeymapEntry[]): Map<string, string> {
  const out = new Map<string, string>();
  for (const c of commands) {
    for (const b of c.bindings) if (!out.has(b)) out.set(b, c.key);
  }
  return out;
}

````

Создать `apps/web/src/ui/shortcuts-panel.tsx`:

````tsx
// Панель «Сочетания клавиш» (спека подсистемы 2, раздел 8; Ctrl+Shift+? и «Справка →
// Сочетания клавиш», как в Figma): поднимается снизу окна, вкладки по группам, у
// каждой команды — клавиши из действующей раскладки, использованные недавно отмечены,
// ссылка «Изменить в настройках». Данные — те же `KeymapEntry`, что у вкладки настроек.
// Окно не модальное: холст доступен, клавиши в самой панели — её (`keys.ts`); Esc
// закрывает панель, если фокус в ней, сочетание панели закрывает её и с холста.
// Раскладку панель не читает: положение и размер задаёт CSS.

import { createMemo, createSignal, For, onMount, Show, type JSX } from 'solid-js';
import { useEditor } from './context';
import { Icon } from './icons';
import { groupCommands } from './keymap-ui';
import { eventShortcut, shortcutLabel } from './shortcuts';

export interface ShortcutsPanelProps {
  /** Команды, использованные недавно: они отмечены. */
  used: ReadonlySet<string>;
  /** Открыть настройки на вкладке сочетаний (панель уже закрылась). */
  onSettings: () => void;
  onClose: () => void;
}

export function ShortcutsPanel(props: ShortcutsPanelProps): JSX.Element {
  const ed = useEditor();
  const returnFocus = document.activeElement as HTMLElement | null;
  let root!: HTMLDivElement;
  const groups = createMemo(() => groupCommands(ed.store.state.keymap.commands));
  const [tab, setTab] = createSignal(groups()[0]?.id ?? '');
  const current = () => groups().find((g) => g.id === tab()) ?? groups()[0];
  const label = (s: string) => shortcutLabel(s, ed.platform, ed.layout());

  const close = () => {
    // Фокус ушёл на холст (панель не модальная) — его не трогаем.
    const inside = root.contains(document.activeElement);
    props.onClose();
    if (inside) returnFocus?.focus?.();
  };

  const onKey = (e: KeyboardEvent) => {
    const s = eventShortcut(e);
    const own = s && ed.store.state.keymap.commands.find((c) => c.key === 'ui.shortcuts')?.bindings.includes(s);
    if (e.key === 'Escape' || own) {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  };

  const focusTab = (id: string) => root.querySelector<HTMLElement>(`[data-group="${id}"]`)?.focus();
  const onTabKey = (e: KeyboardEvent) => {
    const list = groups();
    const i = list.findIndex((g) => g.id === tab());
    const next =
      e.key === 'ArrowRight' ? (i + 1) % list.length
      : e.key === 'ArrowLeft' ? (i - 1 + list.length) % list.length
      : e.key === 'Home' ? 0
      : e.key === 'End' ? list.length - 1
      : -1;
    if (next < 0) return;
    e.preventDefault();
    setTab(list[next].id);
    focusTab(list[next].id);
  };

  onMount(() => focusTab(tab()));

  return (
    <div ref={root} class="fe-keys" role="dialog" aria-label="Сочетания клавиш" onKeyDown={onKey}>
      <div class="fe-keys__head">
        <h2 class="t-strong">Сочетания клавиш</h2>
        <div class="fe-tabs" role="tablist" aria-label="Группы сочетаний">
          <For each={groups()}>
            {(g) => (
              <button
                class="fe-tab"
                role="tab"
                data-group={g.id}
                aria-selected={tab() === g.id}
                aria-controls="fe-keys-list"
                tabindex={tab() === g.id ? 0 : -1}
                onClick={() => setTab(g.id)}
                onKeyDown={onTabKey}
              >
                {g.title}
              </button>
            )}
          </For>
        </div>
        <button
          class="fe-keys__settings"
          onClick={() => {
            close();
            props.onSettings();
          }}
        >
          Изменить в настройках
        </button>
        <button class="fe-tool fe-tool--small" aria-label="Закрыть сочетания клавиш" data-tip="Закрыть" data-tip-key="Esc" onClick={close}>
          <Icon name="close" />
        </button>
      </div>
      <div id="fe-keys-list" class="fe-keys__list" role="tabpanel" aria-label={current()?.title}>
        <For each={current()?.commands ?? []}>
          {(c) => (
            <div class="fe-keys__row" classList={{ 'fe-keys__row--used': props.used.has(c.key) }} data-command={c.key}>
              <span class="fe-keys__name">
                {c.name}
                <Show when={props.used.has(c.key)}>
                  <span class="fe-keys__mark" role="img" aria-label="использовалась недавно" title="Использовалась недавно" />
                </Show>
              </span>
              <span class="fe-keys__caps">
                <For each={c.bindings} fallback={<span class="fe-keys__none">—</span>}>
                  {(b) => <kbd>{label(b)}</kbd>}
                </For>
              </span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}
````

В `apps/web/src/ui/styles/chrome.css` заменить:

````css
  color: var(--ink-faint);
}
/* Кнопка с логотипом: главное меню (как в Figma UI3). */
.fe-logo {
````

на:

````css
  color: var(--ink-faint);
}
/* Панель «Сочетания клавиш»: поднимается снизу окна. */
.fe-keys {
  position: fixed;
  z-index: 35;
  left: 0;
  right: 0;
  bottom: 0;
  display: flex;
  flex-direction: column;
  height: min(320px, 50vh);
  border-top: 1px solid var(--line);
  background: var(--surface-raised);
  box-shadow: var(--shadow-popover);
  color: var(--ink);
  font-size: 13px;
  line-height: 20px;
  animation: fe-rise 160ms ease-out;
}
@keyframes fe-rise {
  from {
    transform: translateY(100%);
  }
}
.fe-keys__head {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--line);
}
.fe-keys__head h2 {
  flex: none;
  margin: 0;
  font-size: 13px;
}
.fe-keys__head .fe-tabs {
  flex: 1;
  min-width: 0;
  flex-wrap: wrap;
  padding: 0;
  border-bottom: 0;
}
.fe-keys__settings {
  flex: none;
  border: 0;
  background: transparent;
  color: var(--select);
  font: inherit;
  cursor: pointer;
}
.fe-keys__settings:hover {
  text-decoration: underline;
}
.fe-keys__list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: var(--space-2) var(--space-3);
  columns: 280px;
  column-gap: var(--space-6);
}
.fe-keys__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  min-height: var(--row-height);
  break-inside: avoid;
}
.fe-keys__name {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}
.fe-keys__mark {
  flex: none;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--select);
}
.fe-keys__caps {
  display: flex;
  flex: none;
  gap: var(--space-1);
}
.fe-keys__caps kbd {
  padding: 0 var(--space-1);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--ink-muted);
  font: inherit;
  font-size: 12px;
}
.fe-keys__row--used .fe-keys__caps kbd {
  color: var(--select);
}
.fe-keys__none {
  color: var(--ink-faint);
}
/* Кнопка с логотипом: главное меню (как в Figma UI3). */
.fe-logo {
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/shortcuts-panel.spec.ts e2e/palette.spec.ts e2e/settings.spec.ts --project=chromium`

Expected: модульные (`palette.test.ts` — 7, всего 166) и сборка — ок; E2E `shortcuts-panel.spec.ts` (5 тестов), `palette`, `settings` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/shortcuts-panel.spec.ts apps/web/src/ui/app.tsx apps/web/src/ui/boot.ts apps/web/src/ui/palette.ts apps/web/src/ui/shortcuts-panel.tsx apps/web/src/ui/styles/chrome.css apps/web/unit/palette.test.ts
git commit -F - <<'EOF'
feat(web): панель «Сочетания клавиш» (Ctrl+Shift+?)

Поднимается снизу окна, как в Figma: вкладки по группам, у команды — клавиши
из действующей раскладки (без них — прочерк), использованные недавно
отмечены точкой и цветом: недавние команды и вызванные сочетанием за сеанс.
Ссылка «Изменить в настройках» открывает настройки на вкладке «Горячие
клавиши». «Справка → Сочетания клавиш» открывает панель, а не настройки.
Окно не модальное: холст работает, то же сочетание закрывает панель.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 7: Спека, бэклог и финальная проверка

Спека §12: тесты и замеры. Здесь — «Сделано (этап 3)» в разделах 7, 8, 11, 12 спеки оболочки (решения с пометкой «(поведение)», протокол, тесты, замер поиска слоёв); в `docs/backlog.md` — раздел «Оболочка (этап 3 подсистемы 2)» (B-140…B-149). Шаг «Проверить» — все ворота этапа.

**Files:**
- Modify: `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md`, `docs/backlog.md`

**Interfaces:**
- Consumes: результаты задач 1–6 (числа замеров — из своего прогона `editor_report`).
- Produces: документы; кода нет.

- [ ] **Step 1: Реализация**

В `docs/backlog.md` заменить:

````markdown
| B-139 | «Выделить подобные» из меню в правке текста меняет выделение, пока правка открыта (как «Выделить всё»); сочетанием туда не попасть (`textKeyAction` отдаёт Ctrl+Alt браузеру). Поворот или ресайз копии за левую или верхнюю ручку меняет мировую позицию, и следующий Ctrl/Cmd+D повторяет этот сдвиг. Вложенный `match` сдвига в `smart_duplicate` — стиль. | P3 | подсистема 2, этап 2, финальное ревью |

## Код

````

на:

````markdown
| B-139 | «Выделить подобные» из меню в правке текста меняет выделение, пока правка открыта (как «Выделить всё»); сочетанием туда не попасть (`textKeyAction` отдаёт Ctrl+Alt браузеру). Поворот или ресайз копии за левую или верхнюю ручку меняет мировую позицию, и следующий Ctrl/Cmd+D повторяет этот сдвиг. Вложенный `match` сдвига в `smart_duplicate` — стиль. | P3 | подсистема 2, этап 2, финальное ревью |

## Оболочка (этап 3 подсистемы 2)

| № | Что | Пр. | Источник |
|---|---|---|---|
| B-140 | «Недавние» палитры — только команды из палитры и меню. Сочетания и кнопки панелей (тулбар, отражение) могли бы учитываться с отбором: без него инструменты и сдвиги вытеснили бы всё остальное. Сверить со справкой Figma. | P3 | этап 3, решение |
| B-141 | Отметка сочетаний в панели — только за сеанс и по нажатию: команда, которую сочетание не выполнило (серая, ввод в правке текста), тоже отмечается; после перезагрузки отметки остаются только у недавних. | P3 | этап 3, решение |
| B-142 | `LayerIndex` держит сложенные имена текущей страницы (4 байта на символ) после поиска и до смены страницы: обычно единицы МБ на 100 000 узлов, в худшем случае (имена по 256 символов) — около 100 МБ. Освобождать при закрытии палитры или хранить UTF-8. | P3 | этап 3 |
| B-143 | Поле палитры не открывается из полей панелей: Ctrl/Cmd+K и Ctrl/Cmd+/ там — клавиши поля (как у всех сочетаний холста, `keys.ts`), в отличие от Ctrl/Cmd+S. | P3 | этап 3 |
| B-144 | Ctrl+Shift+? подписан физической клавишей (`⌃⇧/`, `Ctrl+Shift+/`), а не знаком «?»: подписи знают только клавишу без Shift. | P3 | этап 3 |
| B-145 | В палитре нет подсветки совпавших букв и пути слоя (родительский фрейм): одинаковые имена в разных фреймах неразличимы. Ядро отдаёт только `id`, `name`, `kind`. | P3 | этап 3 |
| B-146 | Переход к слою при активном поиске в панели слоёв: если строка отфильтрована, панель не прокручивается (`reveal` = `None`), поиск панели не сбрасывается. | P3 | этап 3 |
| B-147 | Вкладки панели сочетаний — группы таблицы команд, а не разделы Figma («Essential», «Zoom», «Selection»…): у команд нет такого признака. | P3 | этап 3, решение |
| B-148 | Замеры поиска слоёв — только нативные (`editor_report`); в wasm и на 100 000 узлах в браузере не мерялись. E2E `@perf` — ввод в палитру на 20 000 узлах без длинных задач. | P3 | этап 3 |
| B-149 | Нечёткий поиск принимает слабые совпадения (буквы вразброс в длинных названиях): они внизу списка, но занимают место среди 20 команд. Порог веса не задан. | P3 | этап 3 |

## Код

````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
  - Команды ищутся в основном потоке (их ≈ 70). Слои ищет worker: `FindLayers { query, limit: 50 }` → `LayersFound`, на 100 000 узлах ≤ 30 мс. Каждый символ ввода сбрасывает предыдущий запрос.
- **Тонкий основной поток.** Список в окне — не больше 50 строк, без виртуализации; раскладка читается только в фазе `measure`.

## 8. Панель сочетаний клавиш (этап 3)
````

на:

````markdown
  - Команды ищутся в основном потоке (их ≈ 70). Слои ищет worker: `FindLayers { query, limit: 50 }` → `LayersFound`, на 100 000 узлах ≤ 30 мс. Каждый символ ввода сбрасывает предыдущий запрос.
- **Тонкий основной поток.** Список в окне — не больше 50 строк, без виртуализации; раскладка читается только в фазе `measure`.
- **Сделано (этап 3):**
  - Команда `ui.quick_actions` («Быстрые действия…», группа «Вид», Ctrl/Cmd+K и Ctrl/Cmd+/, конфликтов в раскладке нет) — команда интерфейса: сочетание на холсте идёт через ядро (`UiCommand`). В главном меню пункта нет (его поиск — поле вверху самого меню). Таблица — 86 команд.
  - Поиск — `crates/ferrum-editor/src/find.rs` (слои) и `apps/web/src/ui/fuzzy.ts` (команды, страницы) с одними правилами: буквы запроса встречаются в тексте по порядку; вес — совпадение 1, начало слова +8, буква сразу за прошлой +5, иначе минус пропуск (не больше 3), начало текста +6, иначе минус сдвиг (не больше 3), весь текст без пробелов +10; пробелы запроса не учитываются, текст — первые 256 символов. Команда: лучшее из названия и синонимов (синоним −4), недавняя +3 за каждое место до восьмого (`palette.ts`). Поиск главного меню — тот же нечёткий (раздел 4, этап 1 — подстрока): только по командам дерева меню, без недавности.
  - **(поведение)** Пустой запрос: «Недавние» (если есть) и «Команды» в порядке таблицы. Запрос: «Команды» (до 20), «Слои» (из worker'а, сколько влезет), «Страницы» (до 10); строк с действиями — не больше 50, пустые разделы без заголовков; ничего — «Ничего не найдено». В палитре нет удержания руки, Esc, сдвигов и самой палитры; команды файла на диске без File System Access — тоже нет.
  - **(поведение)** Строка команды: галочка у отмеченного переключателя, название, группа, первое сочетание; недоступная серая, по ней можно пройти стрелками, Enter и щелчок её не выполняют и палитру не закрывают. Новый список — выбрана первая доступная. Стрелки — по кругу; щелчок — как Enter; Esc и то же сочетание — закрыть с возвратом фокуса; щелчок мимо и Tab — закрыть без возврата. Команда выполняется, как из главного меню (`ed.run`, `ui.*` — сразу, из жеста пользователя).
  - **(поведение)** Недавние — последние 8 команд, выполненных из палитры и из меню (главного и контекстного), новые первыми; хранятся в настройках (`recentCommands` — ключи через пробел; общие для вкладок; файл настроек десктопа — то же поле). Сочетания и кнопки панелей в недавние не попадают: инструмент на клавише вытеснил бы всё остальное (B-140).
  - **(поведение)** Слой: строка — иконка типа и имя. Выбор — `RevealLayer`: слой выделяется, предки раскрываются, панель слоёв прокручивается к строке; вид: виден целиком — прежний, влезает при нынешнем масштабе — центр на слое, не влезает — вписан с отступом, как «Показать выделение». У скрытого слоя (рамки нет) — только выделение. Страница: `SetPage`.
  - Палитра — внутри холста (`.fe-stage`), положение задаёт CSS, чтений раскладки нет, кроме прокрутки выбранной строки (в `measure`). Ответы `LayersFound` — через очередь планировщика; ответ не на последний запрос отбрасывается по `query`. Запрос обрезается до 256 байт по границе символа.

## 8. Панель сочетаний клавиш (этап 3)
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
- Ctrl+Shift+? и пункт «Справка → Сочетания клавиш». Внизу окна поднимается панель, как в Figma: вкладки по группам, у каждой команды — клавиши из действующей раскладки, а уже использованные недавно отмечены. Есть ссылка «Изменить в настройках».
- Данные — те же `KeymapEntry`, что у вкладки «Сочетания» в настройках. Новых данных в протоколе нет.

## 9. Линейки и направляющие (этап 4)
````

на:

````markdown
- Ctrl+Shift+? и пункт «Справка → Сочетания клавиш». Внизу окна поднимается панель, как в Figma: вкладки по группам, у каждой команды — клавиши из действующей раскладки, а уже использованные недавно отмечены. Есть ссылка «Изменить в настройках».
- Данные — те же `KeymapEntry`, что у вкладки «Сочетания» в настройках. Новых данных в протоколе нет.
- **Сделано (этап 3):**
  - `ui.shortcuts` получила Ctrl+Shift+? (`Ctrl+Shift+Slash` — Ctrl и на маке, как в справке Figma; физическая клавиша — та, где «/» в раскладке US, подпись — `⌃⇧/`, B-144). «Справка → Сочетания клавиш» и «Настройки → Сочетания клавиш» открывают панель, а не настройки (этап 1).
  - **(поведение)** Панель — на всю ширину окна, у нижнего края, высота 320 px (не больше половины окна), поднимается за 160 мс. Заголовок, вкладки в порядке вкладки настроек, ссылка «Изменить в настройках» (панель закрывается, настройки открываются на вкладке «Горячие клавиши»), кнопка закрытия. У команды — все клавиши действующей раскладки подписями под раскладку пользователя; без сочетания — прочерк (команда остаётся в списке).
  - **(поведение)** Окно не модальное, как в Figma: холст работает под панелью. Фокус при открытии — на выбранной вкладке, стрелки, Home и End — по вкладкам. Esc (фокус в панели) и то же сочетание (из панели и с холста) закрывают; фокус возвращается, только если он был в панели.
  - **(поведение)** Отмечены (точка у названия и цвет клавиш) команды из недавних палитры и вызванные сочетанием за этот сеанс — включая Ctrl/Cmd+C, X и V. Сочетание сопоставляется с действующей раскладкой в основном потоке (без новых данных в протоколе): отметка — о нажатии, а не о том, что сделало ядро; сочетания сеанса не сохраняются (B-141).

## 9. Линейки и направляющие (этап 4)
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
  - `ExpandAllLayers { expanded }`, `DuplicatePage { id }`.
  - `Paste { bytes, mode? }` и `PasteRequested { mode }`, `mode` — `normal | over | replace | props`; на `props` интерфейс отвечает `PasteProps { bytes? | text? }` (байты `web application/x-ferrum` или текст с меткой свойств или узлов; без них — внутренний буфер; текст — не длиннее предела текста буфера). `ClipboardProps { text }` — копия свойств для `text/plain`. Сообщение worker'у `pasteText` несёт `mode`.

## 12. Тестирование
````

на:

````markdown
  - `ExpandAllLayers { expanded }`, `DuplicatePage { id }`.
  - `Paste { bytes, mode? }` и `PasteRequested { mode }`, `mode` — `normal | over | replace | props`; на `props` интерфейс отвечает `PasteProps { bytes? | text? }` (байты `web application/x-ferrum` или текст с меткой свойств или узлов; без них — внутренний буфер; текст — не длиннее предела текста буфера). `ClipboardProps { text }` — копия свойств для `text/plain`. Сообщение worker'у `pasteText` несёт `mode`.
- **Сделано (этап 3):**
  - `FindLayers { query, limit }` → `LayersFound { query, items }`, `items` — `[{ id, name, kind }]` (имя — не длиннее 256 символов, `kind` — как у строки панели слоёв; лучшие первыми, при равном весе — в порядке панели). `query` длиннее 256 байт — `Error` без ответа; `limit` больше 200 — урезается; пустой запрос — пустой ответ. Шрифтов не ждёт: документ не меняет. Отмены выполняющегося поиска нет: команды worker'а идут по очереди, интерфейс отбрасывает устаревший ответ.
  - `RevealLayer { id }`: неверный формат — `Error`, слоя нет на текущей странице — `Error`; выбор — как с холста (предки раскрываются, панель прокручивается), не шаг истории.

## 12. Тестирование
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
- **Сделано (этап 1):** модульные тесты ядра — `command_table` (заполненность таблицы, переключатели, команды `ui.*` только сообщают о себе), `command_state` (без выделения, одна фигура, группы и булевы, история, правка текста и вектора, «только чтение», недоступная команда без событий, отметки вида и настроек, события только с изменениями и не во время жеста), `main_menu` (порядок Figma UI3, состав подменю, без пустых подменю и лишних разделителей, все команды групп меню в дереве), `menu_commands` (сетка, «Войти в правку», форматирование в правке и вне её по раскладке); ядро worker'а — состояния целиком при запуске и после открытия, изменения одним событием, тема «как в системе», недоступная команда без ошибки, нативная вставка (`Paste`, `PasteImage`, `TextInput { paste }`) в документе только для чтения — без ошибки, шага и прерывания жеста, `MainMenu` один раз. Замер `editor_report`: «Состояния команд после шага, 100 000 узлов, выделен 1» — 0,001 мс (цель ≤ 1 мс; нативно, release), справочно при 10 000 выделенных — 0,25 мс, при всех 100 000 — 4,6–5,6 мс (B-95). E2E: `main-menu.spec.ts` (Chromium и Firefox) — подменю клавиатурой и наведением, Alt+/, серые пункты и галочки из ядра, тема, поиск в другой раскладке, первая буква, подменю у края окна, корневое меню с десятками результатов поиска не уходит за низ окна, команды интерфейса, `__ferrumWarnings` пуст; в `chrome.spec.ts` — серые пункты стрелками и первая буква, контекстное меню и тулбар по состоянию ядра. Прежние тесты меню файла открывают пункты через главное меню (`mainMenu` в `helpers.ts`).
- **Сделано (этап 2):** модульные тесты ядра — `context_menu` (выбор на правой кнопке: невыделенный, выделенный, пустое место в рамке и вне её; порядок блоков холста; «Выделить слой» сверху вниз с контейнерами; «Войти в правку» только у фигур; меню слоя и страницы, их доступность в документе только для чтения; неизвестная цель; правка вектора и текста не кончается; «Дублировать страницу» с новыми id, фоном и местом в списке); `ferrum-doc` — `duplicating_an_unopened_page_with_unknown_nodes_is_refused`, `paste_modes` (повтор +10, сброс новой копией, центр вида; поверх; замена с местом в слоях и родителем — один шаг; заблокированные и без выделения; состояния и `PasteRequested` с режимом), `props_clip` (что подходит каждому типу — один шаг; текст с меткой — в другой документ; свойства копии узлов; ошибки без шага; стиль текста с заливкой и подгонкой рамки — один шаг; вставка из копии узлов добавляет в документ только картинки перенесённых заливок; копия с текста берёт собственную заливку первого участка, вставка заливки снимает заливки участков тем же шагом; состояния), `smart_duplicate` (сдвиг протяжкой и стрелками повторяется, сброс выбором и отменой дубликата, несколько узлов; «Выделить подобные»: тип, заливки, обводки, скрытые, образцы, контейнер без потомков), `journal` («Дублировать страницу» переживает журнал и полное сохранение); ядро worker'а — `context_menu` (меню последним после выделения и состояний, ошибки цели, слой без прокрутки, «Развернуть всё», дубль страницы), `paste` (режимы в `PasteRequested`, замена; свойства текстом и из внутреннего буфера), `protocol` (новые сообщения, имена полей, `Paste` без `mode`). Замер `editor_report`: «Выделить подобные, 100 000 узлов» — 1,3 мс (цель ≤ 100 мс; нативно, release), выделено 1000 двойников. E2E: `context-menu.spec.ts` (Chromium и Firefox) — холст: выбор, место меню, состав, «Выделить слой», команда — шаг; пустое место; слои: выбор строки, «Развернуть всё», «Свернуть всё», «Переименовать»; страницы: серое «Удалить», «Дублировать», ссылка на вторую страницу открывает её, а не первую; правый щелчок в поле переименования слоя и страницы не открывает меню ядра; «Копировать ссылку» без буфера обмена — уведомление об ошибке; `__ferrumWarnings` пуст. `paste.spec.ts` — повтор +10 и Ctrl/Cmd+Shift+V, свойства Ctrl/Cmd+Alt+C и V (оба браузера); замена и поверх из меню, Ctrl/Cmd+Shift+R на холсте не перезагружает страницу (оба браузера), «Вставить с заменой» им одним шагом (Chromium), копия свойств в системном буфере (только Chromium, B-131). `duplicate.spec.ts` — умный дубликат стрелками и протяжкой, «Выделить подобные» из меню и сочетанием. `chrome.spec`, `clipboard.spec`, `main-menu.spec` — под новый состав меню.
- **E2E в Chromium и Firefox:**
  - главное меню с подменю, клавиатурой и состоянием пунктов;
````

на:

````markdown
- **Сделано (этап 1):** модульные тесты ядра — `command_table` (заполненность таблицы, переключатели, команды `ui.*` только сообщают о себе), `command_state` (без выделения, одна фигура, группы и булевы, история, правка текста и вектора, «только чтение», недоступная команда без событий, отметки вида и настроек, события только с изменениями и не во время жеста), `main_menu` (порядок Figma UI3, состав подменю, без пустых подменю и лишних разделителей, все команды групп меню в дереве), `menu_commands` (сетка, «Войти в правку», форматирование в правке и вне её по раскладке); ядро worker'а — состояния целиком при запуске и после открытия, изменения одним событием, тема «как в системе», недоступная команда без ошибки, нативная вставка (`Paste`, `PasteImage`, `TextInput { paste }`) в документе только для чтения — без ошибки, шага и прерывания жеста, `MainMenu` один раз. Замер `editor_report`: «Состояния команд после шага, 100 000 узлов, выделен 1» — 0,001 мс (цель ≤ 1 мс; нативно, release), справочно при 10 000 выделенных — 0,25 мс, при всех 100 000 — 4,6–5,6 мс (B-95). E2E: `main-menu.spec.ts` (Chromium и Firefox) — подменю клавиатурой и наведением, Alt+/, серые пункты и галочки из ядра, тема, поиск в другой раскладке, первая буква, подменю у края окна, корневое меню с десятками результатов поиска не уходит за низ окна, команды интерфейса, `__ferrumWarnings` пуст; в `chrome.spec.ts` — серые пункты стрелками и первая буква, контекстное меню и тулбар по состоянию ядра. Прежние тесты меню файла открывают пункты через главное меню (`mainMenu` в `helpers.ts`).
- **Сделано (этап 2):** модульные тесты ядра — `context_menu` (выбор на правой кнопке: невыделенный, выделенный, пустое место в рамке и вне её; порядок блоков холста; «Выделить слой» сверху вниз с контейнерами; «Войти в правку» только у фигур; меню слоя и страницы, их доступность в документе только для чтения; неизвестная цель; правка вектора и текста не кончается; «Дублировать страницу» с новыми id, фоном и местом в списке); `ferrum-doc` — `duplicating_an_unopened_page_with_unknown_nodes_is_refused`, `paste_modes` (повтор +10, сброс новой копией, центр вида; поверх; замена с местом в слоях и родителем — один шаг; заблокированные и без выделения; состояния и `PasteRequested` с режимом), `props_clip` (что подходит каждому типу — один шаг; текст с меткой — в другой документ; свойства копии узлов; ошибки без шага; стиль текста с заливкой и подгонкой рамки — один шаг; вставка из копии узлов добавляет в документ только картинки перенесённых заливок; копия с текста берёт собственную заливку первого участка, вставка заливки снимает заливки участков тем же шагом; состояния), `smart_duplicate` (сдвиг протяжкой и стрелками повторяется, сброс выбором и отменой дубликата, несколько узлов; «Выделить подобные»: тип, заливки, обводки, скрытые, образцы, контейнер без потомков), `journal` («Дублировать страницу» переживает журнал и полное сохранение); ядро worker'а — `context_menu` (меню последним после выделения и состояний, ошибки цели, слой без прокрутки, «Развернуть всё», дубль страницы), `paste` (режимы в `PasteRequested`, замена; свойства текстом и из внутреннего буфера), `protocol` (новые сообщения, имена полей, `Paste` без `mode`). Замер `editor_report`: «Выделить подобные, 100 000 узлов» — 1,3 мс (цель ≤ 100 мс; нативно, release), выделено 1000 двойников. E2E: `context-menu.spec.ts` (Chromium и Firefox) — холст: выбор, место меню, состав, «Выделить слой», команда — шаг; пустое место; слои: выбор строки, «Развернуть всё», «Свернуть всё», «Переименовать»; страницы: серое «Удалить», «Дублировать», ссылка на вторую страницу открывает её, а не первую; правый щелчок в поле переименования слоя и страницы не открывает меню ядра; «Копировать ссылку» без буфера обмена — уведомление об ошибке; `__ferrumWarnings` пуст. `paste.spec.ts` — повтор +10 и Ctrl/Cmd+Shift+V, свойства Ctrl/Cmd+Alt+C и V (оба браузера); замена и поверх из меню, Ctrl/Cmd+Shift+R на холсте не перезагружает страницу (оба браузера), «Вставить с заменой» им одним шагом (Chromium), копия свойств в системном буфере (только Chromium, B-131). `duplicate.spec.ts` — умный дубликат стрелками и протяжкой, «Выделить подобные» из меню и сочетанием. `chrome.spec`, `clipboard.spec`, `main-menu.spec` — под новый состав меню.
- **Сделано (этап 3):** модульные тесты ядра — `find_layers` редактора (подпоследовательность, веса, раскладка, регистр и «ё», порядок, пределы, правка имён и удаление при готовых именах, длинные имена; переход: виден, влезает, не влезает, не шаг истории, слоя нет), `find_layers` и `protocol` ядра worker'а (`LayersFound` со своим запросом, пределы, переход раскрывает предков и прокручивает панель, ошибки цели, имена полей), `command_table` и `keymap` (новые сочетания на обеих платформах); модульные интерфейса — `fuzzy`, `palette` (состав списка, 50 строк, недавние, обрезка запроса), `settings` (`recentCommands`), `main-menu`; `store` десктопа — поле настроек. Замер `editor_report`: «Поиск слоёв, 100 000 узлов» — первый запрос после правки имён (складывает имена страницы) 9,1 мс, следующие 1,1 мс (цель ≤ 30 мс; нативно, release). E2E (Chromium и Firefox): `palette.spec.ts` — открытие обоими сочетаниями и место окна, команды с серыми, стрелки и команда интерфейса, раскладка, слои с переходом, устаревший ответ, страницы, недавние после перезагрузки и из меню, ввод не вызывает инструменты; `@perf` (Chromium): ввод на 20 000 узлах — без длинных задач; `shortcuts-panel.spec.ts` — место панели, закрытие, холст под панелью, вкладки и клавиши из раскладки, отметка использованных, ссылка в настройки. `__ferrumWarnings` пуст.
- **E2E в Chromium и Firefox:**
  - главное меню с подменю, клавиатурой и состоянием пунктов;
````

- [ ] **Step 2: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-render --example render_report | grep -E "FAIL" || echo "FAIL нет"`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Поиск слоёв|Состояния команд после шага, 100"`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-desktop-store 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo check`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/palette.spec.ts --project=chromium --grep @perf`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/palette.spec.ts e2e/shortcuts-panel.spec.ts e2e/main-menu.spec.ts e2e/app.spec.ts e2e/settings-store.spec.ts e2e/context-menu.spec.ts --project=firefox`

Expected: всё PASS: fmt, clippy (нативно и wasm32) без замечаний; `cargo test --workspace` — 1055 passed, 2 ignored; `render_report` — FAIL нет; `editor_report` — 94 строки PASS, «Поиск слоёв» 9,1 и 1,1 мс, «Состояния команд…, выделен 1» 0,001 мс (строки «Кривой вектор 100 000 точек: перетаскивание точки» у границы 16,7 мс при загруженной машине могут дать FAIL — повторный прогон PASS; код векторов этап не трогает); `store` десктопа — 25 passed, `cargo check` десктопа — без ошибок; модульные — 166 passed; `e2e:ci` — 373 теста без падений (известная нестабильная `picker.spec` «обводка: ширина жестом…» проходит на повторе); `@perf` палитры — passed; Firefox — 52 passed, 1 skipped (`@perf` палитры — только Chromium)

- [ ] **Step 3: Закоммитить**

```bash
git add docs/backlog.md docs/superpowers/specs/2026-11-02-ferrum-shell-design.md
git commit -F - <<'EOF'
docs: оболочка, этап 3 — «Сделано» в спеке (§7, §8, §11, §12) и B-140…B-149

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

- [ ] **Ручная проверка**

1. Chrome: `#/new`; на холсте Ctrl/Cmd+K, Esc, Ctrl/Cmd+/ — поле сверху по центру холста, Esc возвращает фокус, то же сочетание в поле закрывает.
2. Пустой запрос — «Недавние» (если есть) и «Команды»; у строки — группа и сочетание, недоступная серая: стрелками доходим, Enter не выполняет, палитра открыта.
3. Запрос `щквук` — команды порядка; `ёлка` находит «Елка»; запрос по имени слоя — раздел «Слои», выбор: слой выделен, предки раскрыты, панель прокручена, вид показывает слой; скрытый слой — только выделение.
4. Выбор страницы — страница открыта. Команда из палитры и из меню — в «Недавних» и после перезагрузки.
5. Ctrl+Shift+? (Ctrl и на маке) — панель снизу; вкладки стрелками; клавиши подписаны под раскладку; холст работает под панелью; то же сочетание с холста закрывает; Esc из панели закрывает и возвращает фокус.
6. Нажать сочетание на холсте — команда отмечена в панели; «Изменить в настройках» — настройки на вкладке «Горячие клавиши»; «Справка → Сочетания клавиш» открывает панель.
7. Firefox: те же шаги 1–6.

---

## Что этап 3 сознательно не делает

- **Недавние по сочетаниям и кнопкам панелей** — только палитра и меню (B-140); отметка панели за сеанс — по нажатию, а не по результату, и не сохраняется (B-141).
- **Освобождение сложенных имён** — `LayerIndex` держит память до смены страницы (B-142).
- **Открытие палитры из полей панелей** — там Ctrl/Cmd+K и / принадлежат полю (B-143).
- **Подпись «?»** — Ctrl+Shift+? подписан физической клавишей (`⌃⇧/`, B-144).
- **Подсветка совпавших букв и путь слоя** в строке палитры (B-145); переход при активном поиске в панели слоёв не прокручивает панель к отфильтрованной строке (B-146).
- **Разделы панели как в Figma** — вкладки по группам таблицы команд (B-147).
- **Порог веса поиска** — слабые совпадения внизу списка, но занимают место (B-149).
- **Не запускались:** проект Playwright `gpu`, `@perf` кроме палитры, Safari/WebKit, запуск десктопа, замеры в wasm и на 100 000 узлах в браузере (B-148), `scripts/compat-old-build.sh` (модель документа и формат буфера не менялись).
