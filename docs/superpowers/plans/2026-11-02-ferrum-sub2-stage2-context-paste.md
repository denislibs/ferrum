# Ferrum, подсистема 2, этап 2: контекстные меню и вставка — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Контекстные меню холста, слоёв и страниц — как в Figma UI3, деревом из ядра: правый щелчок выделяет, как в Figma, меню открывается у точки щелчка, недоступные пункты серые; «Выделить слой ▸» со всеми узлами под точкой; у слоя — «Переименовать», «Развернуть всё», «Свернуть всё»; у страницы — «Переименовать», «Дублировать страницу», «Удалить страницу», «Копировать ссылку на страницу» (`#/file/<id>?page=<pageId>`, маршрут её разбирает). Вставка: «Вставить поверх» (Ctrl/Cmd+Shift+V), «Вставить с заменой» (Ctrl/Cmd+Shift+R), повтор вставки того же содержимого со сдвигом +10, +10; «Копировать свойства» и «Вставить свойства» (Ctrl/Cmd+Alt+C и V); умный дубликат (Ctrl/Cmd+D повторяет сдвиг копии); «Выделить подобные» (Ctrl/Cmd+Alt+A) — ≤ 100 мс на 100 000 узлах.

**Architecture:** Модель документа не меняется (дубль страницы — те же страница и узлы; копия свойств — тот же формат буфера). Редактор (`ferrum-editor`) — источник правды: `context_menu.rs` — `Editor::context_menu(ContextTarget)` выбирает узел на правой кнопке и строит дерево `MenuNode` (узлы «Выделить слой» — `MenuNode::Node`, действия над целью — `MenuNode::Action { action: MenuAction, enabled }`; команды — по-прежнему ключи таблицы); `actions.rs` — `PasteMode { Normal, Over, Replace, Props }`, `paste_with`, повтор вставки (`last_paste`), умный дубликат (`last_duplicate`), `select_similar`; `props_clip.rs` — копия и вставка свойств. `ferrum-doc` получает `Document::duplicate_page`. Ядро worker'а (`ferrum-wasm`) переводит это в протокол: `Command::{ContextMenu, ExpandAllLayers, DuplicatePage, PasteProps}`, `Paste { bytes, mode? }`, `CoreEvent::{ContextMenu, ClipboardProps}`, `PasteRequested { mode }`, `MenuItemDto::{Node, Action}`; меню уходит последним в пачке событий. Основной поток (SolidJS) ничего не вычисляет: правый щелчок шлёт `ContextMenu` (точка — `offsetX/Y`, без чтения раскладки), пункты приходят событием и встают в очередь планировщика вслед за выделением и `CommandStates`; `buildContextMenu` превращает дерево в пункты компонента меню этапа 1; действия над целью выполняет тот, кто открыл меню (холст, панель слоёв, вкладка страницы).

**Tech Stack:** Rust 1.96 (`rust-toolchain.toml`, новых зависимостей нет), SolidJS 1.9.17, TypeScript 6.0.3, @playwright/test 1.64.0 (`chromium`, `firefox`); `package-lock.json` и `Cargo.lock` не меняются.

**Спеки:** `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` — разделы 5, 6, 11, 12 (этап 2); спека ядра `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` (протокол — §4.3, буфер обмена — §7.4, команды и раскладка — §7.6).

## Global Constraints

- Ветка `feat/sub2-context` от `main` с этапом 1 (a54ce7d). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Каталог сборки — у каждой копии свой: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0` (в `apps/web` — `"$PWD/../../target"`); команды — из корня копии, если не сказано иное.
- Новых зависимостей нет ни в Rust, ни в npm; модель документа и формат буфера не меняются.
- Основной поток тонкий (ядро §4.2): выбор на правой кнопке, состав и доступность пунктов меню, режимы вставки, копия свойств, шаг дубликата и «Выделить подобные» считает ядро в worker'е; основной поток только шлёт цель меню и рисует пришедшее дерево. Чтения layout (размер и место меню) — только в фазе `measure` планировщика; точка щелчка холста — `offsetX/Y` события, без `getBoundingClientRect`. E2E проверяют `__ferrumWarnings`.
- Поведение — как в Figma; где спека молчит, решение помечено **(поведение)** (раздел «Решения»).
- Бюджеты (`editor_report`, нативно, release): пересчёт `CommandStates` после шага на 100 000 узлах при одном выделенном — ≤ 1 мс (новые команды не должны его ухудшить); «Выделить подобные» на 100 000 узлах — ≤ 100 мс.
- Числа команд таблицы зашиты в трёх тестах — `crates/ferrum-editor/tests/keymap.rs`, `crates/ferrum-wasm/tests/boolean.rs`, `apps/web/e2e/app.spec.ts`: задачи 4, 5, 6 обновляют все три (80 → 82 → 84 → 85).
- Тесты не проходят вхолостую: редактор — дерево меню и выбор после реальных щелчков, шаги отмены и документ после вставки; ядро — события протокола и их порядок в пачке; модульные — чистые функции (`buildContextMenu`, маршрут, разбор буфера); E2E — роли и `aria-*` пунктов, число шагов документа, `__ferrumWarnings`.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Сначала прототип всего этапа собран в отдельной копии (`plan-verify-s2s2`, ветка `verify/sub2-s2` от a54ce7d, по коммиту на задачу; вершина — 463a7c7, код — 5fb5322) и прошёл ворота на вершине: `cargo fmt --check`, `clippy -D warnings` (нативно и `wasm32-unknown-unknown`), `cargo test --workspace` (1038 passed, 2 ignored), `render_report` (FAIL нет) и `editor_report` (release, 91 строка PASS; «Состояния команд после шага, 100 000 узлов, выделен 1» — 0,001 мс при цели ≤ 1 мс; «„Выделить подобные“, 100 000 узлов» — 1,2–1,4 мс при цели ≤ 100 мс, двойников 1000 из 1000; строка «Масштаб (K): 10 000 узлов» один раз дала 69,5 мс сразу после `cargo test --workspace` — два повторных прогона PASS, 30 и 29 мс, код масштаба этап не трогает), wasm-сборка, `npm run typecheck`, модульные (152), сборка, E2E `chromium` целиком (`--grep-invert @perf`: 353 passed, 2 failed — `booleans.spec` «Alt+Shift+U — объединение…» и `roundtrip.spec` «нарисовать фрейм → сохранить…», оба проходят поодиночке: гонка теста — сводка панели читается до события `Properties`; код булевых и сохранения этап не трогает), Firefox — новые и изменённые спеки (`context-menu`, `paste`, `duplicate`, `chrome`, `main-menu`, `app`: 60 passed, 2 skipped — чтение системного буфера только в Chromium). По ходу каждой задачи прототипа — её тесты, `cargo test` редактора и ядра, clippy, typecheck, модульные и затронутые E2E.

Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом, который читает сам этот документ, к чистой копии от a54ce7d (`plan-apply-s2s2`, ветка `verify/sub2-s2-apply`) в порядке задач с коммитом после каждой: каждый фрагмент «заменить» найден ровно один раз (операций по задачам: 33, 17, 22, 66, 52, 19, 5). После каждой задачи дерево копии совпало с деревом соответствующего коммита прототипа (тот же хэш дерева у всех семи), `git diff 463a7c7 HEAD` — пустой. Команды шагов «Убедиться, что тесты падают» и «Проверить» на чистой копии не повторялись: ожидаемые результаты шагов «Проверить» — из прогона прототипа на том же коде (числа новых тестов — по самим тестовым файлам), ожидаемые падения — по тому, каких элементов API и поведения тестам задачи не хватает до её реализации.

Порт 4173 на машине проверки занят посторонним процессом, поэтому E2E прототипа шли через временный конфиг Playwright с портами 4283/4284 (и временной правкой портов в `core.spec.ts` и `settings.spec.ts`, откатываемой после каждой команды).

Найденное прототипом и вошедшее в шаги: в каноничной записи сочетаний Shift идёт раньше Meta (`Shift+Meta+KeyV`, а не `Meta+Shift+KeyV`) — первая версия `clipboardAction` на маке не узнавала Ctrl/Cmd+Shift+V, модульный тест это закрепляет (задача 4); байты события `clipboard` основной поток сразу передаёт worker'у (запасной путь записи в Firefox), поэтому тесты копируют их до этого или берут текст из `encodeClipboard` (задачи 4, 5); у `MenuItemDto` появились варианты `node` и `action` — `main-menu.ts` проверяет `n.type === 'command'`, иначе TS не собирается (задача 1); тест «поиск с десятками результатов» в `main-menu.spec` брал последний `menuitem`, а последним результатом стал переключатель (`menuitemcheckbox`) — исправлен на любой пункт (задача 5); `browserKey` в `core.ts` отсекает Mod+R во всех средах — «Вставить с заменой» сочетанием недостижима (B-125). Не запускались: проект Playwright `gpu` и замеры `@perf`, Safari/WebKit, десктоп (крейт не тронут), замеры в wasm, `scripts/compat-old-build.sh` (модель документа и формат буфера не менялись), ручная проверка (задача 7).

---

## Этапы подсистемы 2

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Состояние команд и главное меню | описание команд в ядре, состояние «доступна / отмечена», главное меню с подменю и поиском, компонент меню с подменю и первой буквой | 3, 4 |
| **2. Контекстные меню и вставка** (этот план) | меню холста по выделению, меню слоёв и страниц; свойства, вставка с заменой и поверх, умный дубликат, «Выделить подобные» | 5, 6 |
| 3. Быстрые действия и сочетания | палитра Ctrl/Cmd+K и Ctrl/Cmd+/; панель сочетаний клавиш | 7, 8 |
| 4. Линейки, направляющие и полировка панелей | линейки, направляющие с привязкой, сворачиваемые секции, уведомления с действием, страницы | 9, 10 |

## Решения

Где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**.

Контекстные меню:

1. Действия над целью меню (переименовать слой, развернуть и свернуть всё, переименовать, дублировать и удалить страницу, ссылка на страницу) — не команды таблицы, а закрытое перечисление `MenuAction` в узле `MenuNode::Action { action, enabled }`: без цели у них нет смысла и нет сочетаний; ключи команд в меню — по-прежнему только из таблицы. Выполняет их тот, кто открыл меню (`onAction`).
2. **(поведение)** Выбор на правой кнопке — как левый щелчок (правила контекста, название фрейма): невыделенный узел выделяется; выделенный узел и пустое место внутри рамки выделения сохраняют выделение; пустое место вне её снимает выделение (меню пустого места). Правка текста и вектора правым щелчком не кончается.
3. **(поведение)** Порядок блоков холста — как в спеке §5; недоступные пункты серые, а не скрыты. «Войти в правку» — только у одного вектора или фигуры. Нет: подменю недавних шрифтов (нет данных, B-120), линеек (этап 4, B-121), «Вырезать» и «Дублировать» (их нет в составе §5 и в Figma). «Сплющить», «Контур обводки», отражение — уже были. «Копировать как PNG» уже был (`ClipboardPng`) — теперь в подменю «Копировать как» меню холста.
4. **(поведение)** Меню слоя: пункты холста без «Выделить слой», затем «Переименовать», «Развернуть всё», «Свернуть всё» перед «Удалить». Правый щелчок по строке выбирает как панель (без раскрытия предков и прокрутки, B-126).
5. **(поведение)** Меню страницы: «Переименовать», «Дублировать страницу», «Удалить страницу» (единственную — серым; с подтверждением), «Копировать ссылку на страницу» (полный адрес, уведомление). Дубль — сразу после оригинала, «<имя> (копия)», становится текущим, не шаг отмены, как прочие операции со страницами (шаг — этап 4, B-124).
6. **(поведение)** Ссылка `#/file/<id>?page=<pageId>`: файл открывается на этой странице по первому списку страниц, в адресе остаётся только файл; такой страницы нет — первая, без ошибки; `page` недоверенный (`\d+@\d+`, не длиннее 64 знаков), неверный отбрасывается.
7. **(поведение)** Меню у точки щелчка: длинное поднимается, чтобы влезть в окно (прежний `placeMenu`). Меню строки слоя и страницы с клавиатуры — в координатах события (B-130).
8. Порядок в пачке: `CoreEvent::ContextMenu` — последним (в конце `drain_editor`), чтобы интерфейс сначала получил выделение и `CommandStates`; пункты ставятся в очередь планировщика (`scheduler.enqueue`).

Вставка и дубликат:

9. **(поведение)** «Вставить поверх» — исходные координаты, цель — как у обычной вставки, без шага повтора. Ctrl/Cmd+Shift+V в правке текста — обычный текст, на холсте — «поверх» (браузер шлёт `paste`, режим — по нажатой клавише). В каноничной записи сочетаний Shift идёт раньше Meta: `clipboardAction` узнаёт `Ctrl+Shift+KeyV` и `Shift+Meta+KeyV`.
10. **(поведение)** «Вставить с заменой»: левый верхний угол рамки вставки — в угол рамки каждого выделенного узла верхнего уровня, тот же родитель и место в слоях, узел удаляется; заблокированные пропускаются; один шаг; без выделения — обычная вставка. Ctrl/Cmd+Shift+R занято браузером: сочетание в `browser_reserved` (вместе с Ctrl/Cmd+R), настройки предупреждают; тест «умолчания не задевают сочетаний браузера» допускает ровно это исключение. До ядра сочетание не доходит нигде, и в десктопе (`browserKey` в `core.ts`, B-125) — команда из меню.
11. **(поведение)** Повтор вставки: то же содержимое (blake3 байтов) и «исходное видно» (прежнее правило вставки на место) → +10, +10 от прошлой вставки (`PASTE_STEP = 10`); новая копия сбрасывает счёт (B-127).
12. **(поведение)** Свойства: копия — первого выделенного; системный буфер — `ferrum-props:v1:` + base64 формата буфера (один узел без геометрии и детей, с изображениями своих заливок), тот же разбор с проверками, что у вставки; вставка берёт копию свойств или копию узлов (свойства первого узла, как в Figma), иначе внутренний буфер, иначе «Сначала скопируйте свойства». Что подходит типу — как поля панели свойств. Стиль текста — первого участка источника, на весь текст цели с подгонкой рамки; одна запись со всеми правками (`restyle_nodes_with`, `apply_text_changes_with`). Недоступна в правке текста. Уведомление «Свойства скопированы»; отказ браузера в записи — без ошибки (внутренний буфер работает). Обычная вставка копию свойств игнорирует.
13. **(поведение)** Умный дубликат: шаг — сдвиг первой копии от её оригинала в мире, пока выделены именно копии прошлого дубликата и версия выделения та же (протяжка, стрелки, отмена и повтор сдвига её не меняют); выбор и отмена дубликата — сбрасывают.
14. **(поведение)** «Выделить подобные» = Figma «Select matching layers» (Ctrl/Cmd+Alt+A): тип + заливки + обводки точным равенством (B-129), видимые и не заблокированные, совпавший контейнер — без потомков, образцы — все выделенные (до 64 разных, `MAX_SIMILAR_SAMPLES`); не шаг истории. В «Правке» главного меню; в меню холста не добавлено (нет в §5).

Протокол: `ContextMenu { target }`, `target` — `{ kind: "canvas", x, y } | { kind: "layer", id } | { kind: "page", id }` (`deny_unknown_fields`); ответ `ContextMenu { items }` с `MenuItemDto::{ { type: "node", id, name }, { type: "action", action, title, enabled } }`; `ExpandAllLayers { expanded }`, `DuplicatePage { id }`; `Paste { bytes, mode? }`, `PasteRequested { mode }` (`normal | over | replace | props`), `PasteProps { bytes? | text? }`, `ClipboardProps { text }`; сообщение worker'у `pasteText` несёт `mode`. Совместимость документа не затронута — модель не менялась, `scripts/compat-old-build.sh` не нужен.

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-editor/src/context_menu.rs` (новый), `menu.rs`, `pages.rs` | цель меню, выбор на правой кнопке, деревья меню холста, слоя и страницы, `MenuAction`, дубль страницы |
| `crates/ferrum-doc/src/document.rs` | `Document::duplicate_page` |
| `crates/ferrum-editor/src/actions.rs`, `commands.rs`, `command_state.rs`, `editor.rs`, `keymap.rs`, `lib.rs` | команды вставки поверх и с заменой, свойств и «Выделить подобные», повтор вставки, умный дубликат |
| `crates/ferrum-editor/src/props_clip.rs` (новый), `text_style.rs`; `crates/ferrum-doc/src/vector.rs` | копия и вставка свойств, стиль текста одной записью; `base64_*` открыты |
| `crates/ferrum-wasm/src/{protocol,core,layers}.rs`, `core/panels.rs` | `ContextMenu`, `ExpandAllLayers`, `DuplicatePage`, режимы вставки, `PasteProps`, `ClipboardProps` |
| `apps/web/src/ui/context-menu.ts` (новый), `app.tsx`, `chrome.tsx`, `context.ts`, `main-menu.ts` | меню холста из ядра у точки щелчка |
| `apps/web/src/ui/{layers,pages,router,root}.tsx/ts` | меню слоёв и страниц, ссылка на страницу |
| `apps/web/src/core/{core,messages,clipboard-text}.ts`, `worker/core.worker.ts`, `ui/{shortcuts,system-clipboard,clipboard}.ts` | режим вставки в событии `paste`, копия свойств в системном буфере |
| `crates/ferrum-editor/tests/{context_menu,paste_modes,props_clip,smart_duplicate}.rs`, `crates/ferrum-wasm/tests/{context_menu,paste}.rs` (новые), `apps/web/unit/context-menu.test.ts` (новый), `apps/web/e2e/{context-menu,paste,duplicate}.spec.ts` (новые) | тесты |
| `crates/ferrum-editor/examples/editor_report.rs`, спека, `docs/backlog.md` | замер «Выделить подобные»; «Сделано (этап 2)» |

---

### Task 1: Контекстные меню данными в ядре — выбор на правой кнопке, холст, слой, страница

Спека §5: меню холста, слоёв и страниц строит ядро. Здесь — `ContextTarget` и `Editor::context_menu`: выбор на правой кнопке, дерево холста (блоки §5, «Выделить слой ▸» — до 50 узлов под точкой с их контейнерами, сверху вниз), слоя и страницы; узлы дерева `MenuNode::Node` и `MenuNode::Action` с `MenuAction`; `Editor::duplicate_page` и `Document::duplicate_page` (копия с новыми id сразу после страницы, мимо истории; журнал пишет новую страницу целиком). Ядро worker'а — `Command::{ContextMenu, ExpandAllLayers, DuplicatePage}`, `CoreEvent::ContextMenu` последним в пачке, `Layers::set_all_expanded`, общий `panels::menu_dto` для главного и контекстного меню. В `apps/web/src/ui/main-menu.ts` — только проверка `n.type === 'command'` (у `MenuItemDto` появились новые варианты — без неё не собирается TS).

Тесты протокола для `ContextMenu` (round-trip, имена полей) прототип положил в задачу 4 (`crates/ferrum-wasm/tests/protocol.rs`) — план сохраняет это расположение: тесты каждой задачи проходят при применении по порядку.

**Files:**
- Create: `crates/ferrum-editor/src/context_menu.rs`
- Modify: `crates/ferrum-editor/src/{menu,pages,lib}.rs`, `crates/ferrum-doc/src/document.rs`, `crates/ferrum-wasm/src/{protocol,core,layers}.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `apps/web/src/ui/main-menu.ts`
- Test: `crates/ferrum-editor/tests/context_menu.rs` (новый), `crates/ferrum-editor/tests/main_menu.rs`, `crates/ferrum-wasm/tests/context_menu.rs` (новый), `crates/ferrum-doc/tests/journal.rs`

**Interfaces:**
- Consumes: `MenuNode`, `main_menu`, выбор щелчком инструмента выделения (правила контекста), `Editor::command_states`, операции страниц `pages.rs`, `Tx`, `Layers`, `CoreEvent::MainMenu`, `MenuItemDto`.
- Produces: `pub enum ContextTarget { Canvas(Point), Layer(NodeId), Page(PageId) }`, `pub const MAX_SELECT_LAYER_ITEMS: usize = 50`, `Editor::context_menu(&mut self, target: ContextTarget) -> Vec<MenuNode>` (внутри — `context_select_at`, `nodes_under`); `MenuNode::{Node { id, name }, Action { action: MenuAction, enabled }}`; `pub enum MenuAction { RenameLayer, ExpandAll, CollapseAll, RenamePage, DuplicatePage, DeletePage, CopyPageLink }` (`ALL`, `title()`); `menu::sub` — `pub(crate)`; `Editor::duplicate_page(PageId) -> DocResult<PageId>`; `Document::duplicate_page(&mut self, id: PageId, name: &str) -> DocResult<PageId>`. Протокол: `ContextTarget { canvas { x, y } | layer { id } | page { id } }` (тег `kind`), `Command::{ContextMenu { target }, ExpandAllLayers { expanded }, DuplicatePage { id }}`, `CoreEvent::ContextMenu { items }`, `MenuItemDto::{Node { id, name }, Action { action, title, enabled }}`, `MenuActionDto`; `Layers::set_all_expanded(&mut self, scene, expanded)`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-doc/tests/journal.rs` заменить:

````rust
        prop_assert_eq!(state(&mut back), state(&mut doc));
    }
}
````

на:

````rust
        prop_assert_eq!(state(&mut back), state(&mut doc));
    }
}

#[test]
fn duplicated_page_survives_journal_and_full_save() {
    let (file, mut doc) = opened();
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 4.0);
    let copy = doc.duplicate_page(page, "Копия").unwrap();
    assert_eq!(doc.pages()[1], (copy, "Копия".to_string()));
    // Копия правится независимо от оригинала.
    rect(&mut doc, copy, 6.0);
    let b1 = doc.take_journal().unwrap().unwrap();
    let (mut back, replayed) = recover(&file, &[&b1]);
    assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
    assert_eq!(back.pages(), doc.pages());
    assert_eq!(state(&mut back), state(&mut doc));
    let mut again = decode(&encode(&back).unwrap()).unwrap().document;
    assert_eq!(state(&mut again), state(&mut doc));
}
````

Создать `crates/ferrum-editor/tests/context_menu.rs`:

````rust
//! Контекстные меню (спека подсистемы 2, раздел 5): выбор на правой кнопке, как в
//! Figma, состав меню холста по выделению, пустого места, строки слоя и
//! страницы; «Дублировать страницу».

mod common;

use std::collections::HashSet;

use common::*;
use ferrum_doc::{Color, NodeKind};
use ferrum_editor::*;
use kurbo::Point;

fn canvas(h: &mut H, x: f64, y: f64) -> Vec<MenuNode> {
    h.ed.context_menu(ContextTarget::Canvas(Point::new(x, y)))
}

fn keys(items: &[MenuNode]) -> Vec<&'static str> {
    items
        .iter()
        .filter_map(|n| match n {
            MenuNode::Command(c) => Some(c.key()),
            _ => None,
        })
        .collect()
}

fn submenu<'a>(items: &'a [MenuNode], title: &str) -> Option<&'a [MenuNode]> {
    items.iter().find_map(|n| match n {
        MenuNode::Submenu { title: t, items } if *t == title => Some(items.as_slice()),
        _ => None,
    })
}

fn actions(items: &[MenuNode]) -> Vec<(MenuAction, bool)> {
    items
        .iter()
        .filter_map(|n| match n {
            MenuNode::Action { action, enabled } => Some((*action, *enabled)),
            _ => None,
        })
        .collect()
}

/// Уровень меню: не пуст, без разделителей с краю и подряд, без повторов команд.
fn check_level(items: &[MenuNode]) {
    assert!(!items.is_empty());
    assert!(!matches!(items.first(), Some(MenuNode::Separator)));
    assert!(!matches!(items.last(), Some(MenuNode::Separator)));
    for w in items.windows(2) {
        assert!(!(matches!(w[0], MenuNode::Separator) && matches!(w[1], MenuNode::Separator)));
    }
    let mut seen = HashSet::new();
    for n in items {
        match n {
            MenuNode::Command(c) => assert!(seen.insert(*c), "{c:?} дважды"),
            MenuNode::Submenu { items, .. } => check_level(items),
            _ => {}
        }
    }
}

#[test]
fn right_click_selects_an_unselected_node_and_keeps_a_selected_one() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 300.0, 100.0, 100.0, 100.0);
    let menu = canvas(&mut h, 150.0, 150.0);
    assert_eq!(h.sel(), [a], "невыделенный узел под точкой выделяется");
    check_level(&menu);
    // Выделены оба: правый щелчок по одному из них сохраняет выделение.
    h.ed.select(&[a, b]);
    canvas(&mut h, 350.0, 150.0);
    assert_eq!(h.sel(), [a, b]);
    // Пустое место внутри общей рамки выделения — тоже.
    canvas(&mut h, 250.0, 150.0);
    assert_eq!(h.sel(), [a, b]);
    // Пустое место вне рамки снимает выделение: меню пустого места.
    let empty = canvas(&mut h, 600.0, 600.0);
    assert!(h.sel().is_empty());
    assert_eq!(
        keys(&empty),
        ["edit.paste", "edit.select_all", "view.fit_all"]
    );
    check_level(&empty);
}

#[test]
fn canvas_menu_follows_figma_block_order() {
    let mut h = H::new();
    h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let menu = canvas(&mut h, 150.0, 150.0);
    assert_eq!(
        keys(&menu),
        [
            "edit.copy",
            "edit.paste",
            "arrange.forward",
            "arrange.backward",
            "arrange.front",
            "arrange.back",
            "object.group",
            "object.ungroup",
            "object.frame_selection",
            "object.flatten",
            "object.outline_stroke",
            "object.flip_horizontal",
            "object.flip_vertical",
            "object.toggle_visible",
            "object.toggle_lock",
            "vector.edit",
            "edit.delete",
        ]
    );
    let titles: Vec<&str> = menu
        .iter()
        .filter_map(|n| match n {
            MenuNode::Submenu { title, .. } => Some(*title),
            _ => None,
        })
        .collect();
    assert_eq!(
        titles,
        ["Копировать как", "Выделить слой", "Булевы операции"]
    );
    assert_eq!(
        keys(submenu(&menu, "Копировать как").unwrap()),
        ["edit.copy_png", "edit.copy_svg"]
    );
    assert!(
        actions(&menu).is_empty(),
        "у холста нет действий строки слоя"
    );
    // Каждая команда меню — из таблицы.
    for k in keys(&menu) {
        assert!(CommandId::from_key(k).is_some(), "{k}");
    }
}

#[test]
fn select_layer_lists_every_node_under_the_point_top_first() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    let low = h.rect(Some(f), 50.0, 50.0, 200.0, 200.0);
    let high = h.rect(Some(f), 100.0, 100.0, 50.0, 50.0);
    h.set(
        high,
        ferrum_doc::PropsPatch {
            name: Some("Верхний".into()),
            ..Default::default()
        },
    );
    let menu = canvas(&mut h, 120.0, 120.0);
    let under: Vec<(ferrum_doc::NodeId, String)> = submenu(&menu, "Выделить слой")
        .unwrap()
        .iter()
        .map(|n| match n {
            MenuNode::Node { id, name } => (*id, name.clone()),
            other => panic!("{other:?}"),
        })
        .collect();
    let ids: Vec<_> = under.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        ids,
        [high, f, low],
        "сверху вниз, контейнер — после ребёнка"
    );
    assert_eq!(under[0].1, "Верхний");
    // Ребёнок фрейма верхнего уровня выбирается щелчком, как левой кнопкой.
    assert_eq!(h.sel(), [high]);
}

#[test]
fn edit_object_only_for_vectors_and_shapes() {
    let mut h = H::new();
    let f = h.frame(None, 500.0, 100.0, 100.0, 100.0);
    h.rect(None, 100.0, 100.0, 100.0, 100.0);
    assert!(keys(&canvas(&mut h, 150.0, 150.0)).contains(&"vector.edit"));
    h.ed.select(&[f]);
    let menu = h.ed.context_menu(ContextTarget::Layer(f));
    assert!(!keys(&menu).contains(&"vector.edit"));
}

#[test]
fn layer_menu_selects_the_row_and_adds_rename_and_expand() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 300.0, 100.0, 100.0, 100.0);
    h.ed.select(&[a]);
    let menu = h.ed.context_menu(ContextTarget::Layer(b));
    assert_eq!(h.sel(), [b], "невыделенная строка выделяется");
    check_level(&menu);
    assert!(submenu(&menu, "Выделить слой").is_none());
    assert_eq!(
        actions(&menu),
        [
            (MenuAction::RenameLayer, true),
            (MenuAction::ExpandAll, true),
            (MenuAction::CollapseAll, true)
        ]
    );
    assert_eq!(keys(&menu).last(), Some(&"edit.delete"));
    // Выделенная строка выделение сохраняет.
    h.ed.select(&[a, b]);
    h.ed.context_menu(ContextTarget::Layer(a));
    assert_eq!(h.sel(), [a, b]);
}

#[test]
fn page_menu_actions_and_their_state() {
    let mut h = H::new();
    let first = h.ed.page();
    let menu = h.ed.context_menu(ContextTarget::Page(first));
    check_level(&menu);
    assert_eq!(
        actions(&menu),
        [
            (MenuAction::RenamePage, true),
            (MenuAction::DuplicatePage, true),
            (MenuAction::DeletePage, false),
            (MenuAction::CopyPageLink, true)
        ],
        "единственную страницу удалить нельзя"
    );
    h.ed.create_page().unwrap();
    let menu = h.ed.context_menu(ContextTarget::Page(first));
    assert!(actions(&menu).contains(&(MenuAction::DeletePage, true)));
    // Только для чтения: правки страниц и слоя недоступны, ссылка — доступна.
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    let menu = h.ed.context_menu(ContextTarget::Page(first));
    assert_eq!(
        actions(&menu),
        [
            (MenuAction::RenamePage, false),
            (MenuAction::DuplicatePage, false),
            (MenuAction::DeletePage, false),
            (MenuAction::CopyPageLink, true)
        ]
    );
}

#[test]
fn unknown_targets_give_no_menu() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let page = h.ed.page();
    h.ed.transact(|tx| {
        tx.delete_nodes(&[a])?;
        Ok(vec![])
    })
    .unwrap();
    assert!(h.ed.context_menu(ContextTarget::Layer(a)).is_empty());
    h.ed.create_page().unwrap();
    h.ed.delete_page(page).unwrap();
    assert!(h.ed.context_menu(ContextTarget::Page(page)).is_empty());
}

#[test]
fn right_click_in_text_or_vector_edit_keeps_the_edit() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    h.ed.select(&[r]);
    h.ed.execute(CommandId::EditObject);
    assert!(h.ed.vector_edit_node().is_some());
    canvas(&mut h, 600.0, 600.0);
    assert_eq!(h.ed.vector_edit_node(), Some(r), "правка вектора идёт");
    assert_eq!(h.sel(), [r]);
}

#[test]
fn duplicate_page_copies_nodes_after_the_source_and_opens_it() {
    let mut h = H::new();
    let first = h.ed.page();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    h.rect(Some(f), 10.0, 10.0, 50.0, 50.0);
    h.ed.set_page_background(first, Some(Color::rgba(10, 20, 30, 255)))
        .unwrap();
    let last = h.ed.create_page().unwrap();
    h.ed.set_page(first).unwrap();
    let steps = h.ed.step_count();
    let before = h.state();
    let copy = h.ed.duplicate_page(first).unwrap();
    let pages = h.ed.document().pages();
    assert_eq!(
        pages.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        [first, copy, last],
        "копия — сразу после страницы"
    );
    assert_eq!(pages[1].1, "Страница 1 (копия)");
    assert_eq!(h.ed.page(), copy, "копия — текущая");
    assert_eq!(h.ed.step_count(), steps, "в историю отмены не попадает");
    assert_eq!(h.ed.page_background(), Some(Color::rgba(10, 20, 30, 255)));
    let after = h.state();
    assert_eq!(after.len(), before.len());
    for ((id_a, parent_a, pa), (id_b, parent_b, pb)) in before.iter().zip(&after) {
        assert_ne!(id_a, id_b, "у копий новые идентификаторы");
        assert_eq!(parent_a.is_some(), parent_b.is_some());
        assert_eq!(
            (pa.kind, pa.x, pa.y, pa.w, pa.h),
            (pb.kind, pb.x, pb.y, pb.w, pb.h)
        );
    }
    assert_eq!(after[0].2.kind, NodeKind::Frame);
    // Узлы копии правятся отдельно от оригинала.
    let (child, _, _) = after[1].clone();
    h.set(child, patch(200.0, 200.0, 50.0, 50.0));
    h.ed.set_page(first).unwrap();
    assert_eq!(h.state(), before);
}
````

В `crates/ferrum-editor/tests/main_menu.rs` заменить:

````rust
            MenuNode::Submenu { title, items } => check_level(items, &format!("{path} › {title}")),
            MenuNode::Separator => {}
        }
    }
````

на:

````rust
            MenuNode::Submenu { title, items } => check_level(items, &format!("{path} › {title}")),
            MenuNode::Separator => {}
            // Узлы и действия бывают только в контекстных меню.
            other => panic!("{path}: {other:?} в главном меню"),
        }
    }
````

Создать `crates/ferrum-wasm/tests/context_menu.rs`:

````rust
//! Контекстные меню в ядре worker'а (спека подсистемы 2, разделы 5 и 11):
//! `ContextMenu { target }` — выбор на правой кнопке и событие `ContextMenu`
//! последним в пачке, после выделения и состояний команд; неверная цель —
//! ошибка и пустое меню; «Развернуть всё» и «Дублировать страницу».
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

fn menu(ev: &[CoreEvent]) -> Vec<MenuItemDto> {
    match ev.last() {
        Some(CoreEvent::ContextMenu { items }) => items.clone(),
        other => panic!("последним должно быть меню: {other:?}"),
    }
}

fn find_sub<'a>(items: &'a [MenuItemDto], name: &str) -> &'a [MenuItemDto] {
    items
        .iter()
        .find_map(|n| match n {
            MenuItemDto::Submenu { title, items } if title == name => Some(items.as_slice()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("нет «{name}»"))
}

fn layer_rows(core: &mut Core) -> (u32, Vec<LayerRow>, Option<u32>) {
    core.command(Command::LayersWindow {
        start: 0,
        count: 400,
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
fn canvas_menu_comes_after_selection_and_states() {
    let mut core = demo(200);
    // Мир (200, 40) — «Слой 1» в «Экран 0», на экране (140, 64).
    core.command(Command::ContextMenu {
        target: ContextTarget::Canvas { x: 140.0, y: 64.0 },
    });
    let ev = core.take_events();
    let at = |f: fn(&CoreEvent) -> bool| ev.iter().position(f).expect("событие");
    let sel = at(|e| matches!(e, CoreEvent::Selection { count: 1, .. }));
    let states = at(|e| matches!(e, CoreEvent::CommandStates { .. }));
    assert!(sel < ev.len() - 1 && states < ev.len() - 1);
    let items = menu(&ev);
    assert!(matches!(&items[0], MenuItemDto::Command { key } if key == "edit.copy"));
    let under = find_sub(&items, "Выделить слой");
    let names: Vec<&str> = under
        .iter()
        .map(|n| match n {
            MenuItemDto::Node { name, .. } => name.as_str(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(names, ["Слой 1", "Экран 0"]);
}

#[test]
fn wrong_targets_are_errors_with_an_empty_menu() {
    let mut core = demo(100);
    for target in [
        ContextTarget::Canvas {
            x: f64::NAN,
            y: 0.0,
        },
        ContextTarget::Layer {
            id: "мусор".into()
        },
        ContextTarget::Page { id: "1@1".into() },
    ] {
        core.command(Command::ContextMenu { target });
        let ev = core.take_events();
        assert!(
            ev.iter().any(|e| matches!(e, CoreEvent::Error { .. })),
            "{ev:?}"
        );
        assert!(menu(&ev).is_empty());
    }
}

#[test]
fn layer_menu_selects_like_the_panel_and_expands_all() {
    let mut core = demo(200);
    let (total, rows, _) = layer_rows(&mut core);
    assert_eq!(total, 2, "фреймы свёрнуты");
    core.command(Command::ContextMenu {
        target: ContextTarget::Layer {
            id: rows[0].id.clone(),
        },
    });
    let ev = core.take_events();
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Selection { ids, .. } if ids == &[rows[0].id.clone()])));
    let items = menu(&ev);
    let actions: Vec<(MenuActionDto, String)> = items
        .iter()
        .filter_map(|n| match n {
            MenuItemDto::Action { action, title, .. } => Some((*action, title.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        actions,
        [
            (MenuActionDto::RenameLayer, "Переименовать".into()),
            (MenuActionDto::ExpandAll, "Развернуть всё".into()),
            (MenuActionDto::CollapseAll, "Свернуть всё".into())
        ]
    );
    // Выбор из панели: строка не прокручивается к себе.
    core.command(Command::ExpandAllLayers { expanded: true });
    let (total, _, reveal) = layer_rows(&mut core);
    assert_eq!((total, reveal), (200, None));
    core.command(Command::ExpandAllLayers { expanded: false });
    assert_eq!(layer_rows(&mut core).0, 2);
}

#[test]
fn page_menu_and_duplicate_page() {
    let mut core = demo(100);
    core.command(Command::AddPage);
    let pages = |ev: &[CoreEvent]| {
        ev.iter()
            .rev()
            .find_map(|e| match e {
                CoreEvent::Pages { pages, current, .. } => Some((pages.clone(), current.clone())),
                _ => None,
            })
            .expect("страницы")
    };
    let (list, _) = pages(&core.take_events());
    let first = list[0].id.clone();
    core.command(Command::ContextMenu {
        target: ContextTarget::Page { id: first.clone() },
    });
    let items = menu(&core.take_events());
    assert!(items.iter().any(|n| matches!(
        n,
        MenuItemDto::Action {
            action: MenuActionDto::DeletePage,
            enabled: true,
            ..
        }
    )));
    core.command(Command::DuplicatePage { id: first });
    let ev = core.take_events();
    let (list, current) = pages(&ev);
    let names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Страница 1", "Страница 1 (копия)", "Страница 2"]);
    assert_eq!(current, list[1].id);
    assert_eq!(layer_rows(&mut core).0, 1, "копия с фреймом оригинала");
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc --test journal`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test context_menu --test main_menu`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test context_menu`
Expected: FAIL — не собирается: `ferrum-doc` — нет `Document::duplicate_page`; редактор — нет `ContextTarget`, `MAX_SELECT_LAYER_ITEMS`, `MenuAction`, `MenuNode::{Node, Action}`, методов `Editor::{context_menu, duplicate_page}`; ядро — нет `Command::{ContextMenu, ExpandAllLayers, DuplicatePage}`, `CoreEvent::ContextMenu`, `MenuItemDto::{Node, Action}`

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/main-menu.ts` заменить:

````ts
      const items = buildMainMenu(n.items, entries, hidden);
      if (items.length) out.push({ label: n.title, submenu: items });
    } else if (!hidden.has(n.key)) {
      const e = entries.get(n.key);
      if (e) out.push({ label: e.name, command: e.key });
````

на:

````ts
      const items = buildMainMenu(n.items, entries, hidden);
      if (items.length) out.push({ label: n.title, submenu: items });
    } else if (n.type === 'command' && !hidden.has(n.key)) {
      const e = entries.get(n.key);
      if (e) out.push({ label: e.name, command: e.key });
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        self.journaled_pages.insert(id, VersionVector::default());
        Ok(id)
    }

````

на:

````rust
        self.journaled_pages.insert(id, VersionVector::default());
        Ok(id)
    }

    /// Копия страницы `id` с именем `name` сразу после неё в списке: те же узлы
    /// (с новыми идентификаторами) и фон. Как и остальные операции со страницами,
    /// в историю отмены не попадает; копия целиком уйдёт в следующую пачку журнала.
    pub fn duplicate_page(&mut self, id: PageId, name: &str) -> DocResult<PageId> {
        self.writable()?;
        self.page(id)?;
        let at = self
            .pages()
            .iter()
            .position(|(p, _)| *p == id)
            .map_or(0, |i| i + 1);
        let background = self.page_background(id);
        // Узлы копируются до создания страницы: отказ не оставляет пустой копии.
        let copy = PageDoc::new();
        {
            let src = self.loaded.get(&id).expect("страница загружена выше");
            let tx = Tx::new(&copy);
            // Обход без рекурсии: глубина не ограничена.
            let mut stack: Vec<(Option<NodeId>, Option<NodeId>)> = vec![(None, None)];
            while let Some((from, to)) = stack.pop() {
                for child in src.children(from) {
                    let made = tx.create_with_props(to, None, &src.props(child)?)?;
                    stack.push((Some(child), Some(made)));
                }
            }
        }
        copy.commit();
        let t = self.pages_tree.create_at(TreeParentId::Root, at)?;
        let meta = self.pages_tree.get_meta(t)?;
        meta.insert(KEY_NAME, name)?;
        if let Some(c) = background {
            meta.insert(KEY_BACKGROUND, c.to_hex())?;
        }
        self.root.commit();
        let new = PageId(t);
        self.loaded.insert(new, copy);
        self.journaled_pages.insert(new, VersionVector::default());
        Ok(new)
    }

````

Создать `crates/ferrum-editor/src/context_menu.rs`:

````rust
//! Контекстные меню (спека подсистемы 2, раздел 5): что показать по правому
//! щелчку на холсте, по строке слоя и по странице. Сначала — выбор, как в Figma
//! на правой кнопке: невыделенный узел под точкой (или строка слоя) выделяется,
//! выделенный сохраняет выделение. Затем — дерево пунктов: команды таблицы (их
//! доступность интерфейс берёт из `CommandStates`), «Выделить слой» с узлами под
//! точкой и действия над целью (переименовать, развернуть всё, страницы).
//!
//! В дереве — только команды, которые уже есть: пункты следующих этапов
//! (линейки, недавние шрифты) появятся вместе с ними.

use ferrum_doc::{NodeId, NodeKind, PageId};
use kurbo::Point;

use crate::commands::CommandId;
use crate::editor::Editor;
use crate::geometry::SelectionBox;
use crate::menu::{sub, MenuAction, MenuNode};
use crate::selection::{frame_label_at, resolve, top_hit, Pick};
use crate::tools::HIT_TOLERANCE_PX;

/// Сколько узлов под точкой показывает «Выделить слой».
pub const MAX_SELECT_LAYER_ITEMS: usize = 50;

/// По чему щёлкнули правой кнопкой.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContextTarget {
    /// Точка холста в CSS-пикселях (как у событий указателя).
    Canvas(Point),
    /// Строка панели слоёв.
    Layer(NodeId),
    /// Строка списка страниц.
    Page(PageId),
}

use CommandId as C;
use MenuNode::{Command as I, Separator as S};

fn action(action: MenuAction, enabled: bool) -> MenuNode {
    MenuNode::Action { action, enabled }
}

/// Название узла для пункта «Выделить слой»: имя, а без него — тип, как в
/// панели слоёв.
fn node_name(name: &str, kind: NodeKind) -> String {
    if !name.trim().is_empty() {
        return name.to_string();
    }
    match kind {
        NodeKind::Frame => "Фрейм",
        NodeKind::Group => "Группа",
        NodeKind::Rect => "Прямоугольник",
        NodeKind::Ellipse => "Эллипс",
        NodeKind::Line => "Линия",
        NodeKind::Image => "Изображение",
        NodeKind::Text => "Текст",
        NodeKind::Vector => "Вектор",
        NodeKind::Boolean => "Булева группа",
    }
    .to_string()
}

impl Editor {
    /// Контекстное меню цели: выбор на правой кнопке и дерево пунктов. Пустое
    /// дерево — меню не открывать (цель не найдена, идёт жест).
    pub fn context_menu(&mut self, target: ContextTarget) -> Vec<MenuNode> {
        // Жест панели держит узлы на начало жеста: выбор мог бы их сменить.
        self.cancel_panel();
        if self.is_busy() {
            return Vec::new();
        }
        let items = match target {
            ContextTarget::Canvas(p) => {
                let under = self.context_select_at(p);
                self.node_menu(under, false)
            }
            ContextTarget::Layer(id) => {
                if self.scene.slot(id).is_none() || self.scene.is_draft(id) {
                    return Vec::new();
                }
                // Правый щелчок по невыделенной строке выделяет её (как в Figma).
                if !self.selection.contains(&id) {
                    self.select(&[id]);
                }
                self.node_menu(None, true)
            }
            ContextTarget::Page(id) => {
                if !self.doc.pages().iter().any(|(p, _)| *p == id) {
                    return Vec::new();
                }
                self.page_menu()
            }
        };
        self.emit();
        items
    }

    /// Выбор правой кнопкой в экранной точке `p`, как в Figma: узел под точкой,
    /// который выбрал бы щелчок, выделяется, если он не выделен; выделенный
    /// (или пустое место внутри рамки выделения) выделение сохраняет; пустое
    /// место вне её снимает выделение. Правку текста и вектора правый щелчок не
    /// заканчивает: меню — по их узлу. Возвращает узлы под точкой для «Выделить
    /// слой», сверху вниз.
    fn context_select_at(&mut self, p: Point) -> Option<Vec<(NodeId, String)>> {
        let world = self.camera.screen_to_world(p);
        let tolerance = HIT_TOLERANCE_PX / self.camera.zoom();
        let under = self.nodes_under(world, tolerance);
        if self.text_edit.is_some() || self.vector_edit.is_some() {
            return Some(under);
        }
        let picked = frame_label_at(&self.scene, &self.camera, p).or_else(|| {
            let hit = top_hit(&self.scene, world, tolerance)?;
            resolve(&self.scene, hit, &self.selection, Pick::Normal)
        });
        match picked {
            Some(id) if self.selection.contains(&id) => {}
            Some(id) => self.selection.set(vec![id]),
            None => {
                let inside = SelectionBox::of(&self.scene, &self.selection)
                    .is_some_and(|b| b.rect.contains(b.to_world.inverse() * world));
                if !inside {
                    self.selection.clear();
                }
            }
        }
        Some(under)
    }

    /// Все узлы под точкой мира, от верхнего к нижнему, с предками (как «Select
    /// layer» в Figma): у каждого попадания — оно само, затем его контейнеры.
    /// Черновики пропускаются; не больше `MAX_SELECT_LAYER_ITEMS`.
    fn nodes_under(&self, world: Point, tolerance: f64) -> Vec<(NodeId, String)> {
        let mut out: Vec<(NodeId, String)> = Vec::new();
        'hits: for id in self.scene.hit_test_all(world, tolerance) {
            let mut cur = self.scene.slot(id);
            while let Some(s) = cur {
                let n = self.scene.at(s);
                if !self.scene.is_draft(n.id) && !out.iter().any(|(x, _)| *x == n.id) {
                    if out.len() == MAX_SELECT_LAYER_ITEMS {
                        break 'hits;
                    }
                    out.push((n.id, node_name(&n.props.name, n.props.kind)));
                }
                cur = n.parent;
            }
        }
        out
    }

    /// Меню выделения (холст и строка слоя) или пустого места холста. Блоки — в
    /// порядке Figma (спека, раздел 5); `under` — узлы под точкой щелчка для
    /// «Выделить слой», `layer` — меню строки слоя.
    fn node_menu(&self, under: Option<Vec<(NodeId, String)>>, layer: bool) -> Vec<MenuNode> {
        if self.selection.is_empty() {
            // Пустое место холста.
            return vec![I(C::Paste), S, I(C::SelectAll), I(C::ZoomToFit)];
        }
        let mut items = vec![
            I(C::Copy),
            I(C::Paste),
            sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
        ];
        if let Some(under) = under.filter(|u| !u.is_empty()) {
            items.push(S);
            items.push(sub(
                "Выделить слой",
                under
                    .into_iter()
                    .map(|(id, name)| MenuNode::Node { id, name })
                    .collect(),
            ));
        }
        items.extend([
            S,
            I(C::BringForward),
            I(C::SendBackward),
            I(C::BringToFront),
            I(C::SendToBack),
            S,
            I(C::Group),
            I(C::Ungroup),
            I(C::FrameSelection),
            sub(
                "Булевы операции",
                vec![
                    I(C::BooleanUnion),
                    I(C::BooleanSubtract),
                    I(C::BooleanIntersect),
                    I(C::BooleanExclude),
                ],
            ),
            I(C::Flatten),
            I(C::OutlineStroke),
            S,
            I(C::FlipHorizontal),
            I(C::FlipVertical),
            I(C::ToggleVisible),
            I(C::ToggleLock),
        ]);
        // Только у вектора и фигуры: «Войти в правку».
        let shape = match self.selection[..] {
            [one] => self.scene.node(one).is_some_and(|n| {
                matches!(
                    n.props.kind,
                    NodeKind::Vector | NodeKind::Rect | NodeKind::Ellipse | NodeKind::Line
                )
            }),
            _ => false,
        };
        if shape {
            items.extend([S, I(C::EditObject)]);
        }
        if layer {
            let edit = !self.doc.is_read_only();
            items.extend([
                S,
                action(MenuAction::RenameLayer, edit),
                action(MenuAction::ExpandAll, true),
                action(MenuAction::CollapseAll, true),
            ]);
        }
        items.extend([S, I(C::Delete)]);
        items
    }

    /// Меню страницы: переименовать, дублировать, удалить (не единственную),
    /// ссылка на страницу. Правки страниц в документе только для чтения
    /// недоступны.
    fn page_menu(&self) -> Vec<MenuNode> {
        let edit = !self.doc.is_read_only();
        let many = self.doc.pages().len() > 1;
        vec![
            action(MenuAction::RenamePage, edit),
            action(MenuAction::DuplicatePage, edit),
            action(MenuAction::DeletePage, edit && many),
            S,
            action(MenuAction::CopyPageLink, true),
        ]
    }
}
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod command_state;
pub mod commands;
mod editor;
mod flatten;
````

на:

````rust
pub mod command_state;
pub mod commands;
pub mod context_menu;
mod editor;
mod flatten;
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub use command_state::CommandState;
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
pub use editor::{Editor, EditorEvent, Settings, ThemeChoice, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
pub use menu::{main_menu, menu_commands, MenuNode};
pub use pages::{clean_name, MAX_NAME_CHARS};
pub use panel::{
````

на:

````rust
pub use command_state::CommandState;
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
pub use context_menu::{ContextTarget, MAX_SELECT_LAYER_ITEMS};
pub use editor::{Editor, EditorEvent, Settings, ThemeChoice, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
pub use menu::{main_menu, menu_commands, MenuAction, MenuNode};
pub use pages::{clean_name, MAX_NAME_CHARS};
pub use panel::{
````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
//! Главное меню данными (спека подсистемы 2, разделы 3 и 4): дерево команд по
//! ключам таблицы, подменю и разделителей. Порядок и состав повторяют главное
//! меню Figma UI3 без плагинов, виджетов и Dev Mode. В дереве — только команды,
//! которые уже есть: пункты следующих этапов (вставить поверх, с заменой,
````

на:

````rust
//! Главное и контекстные меню данными (спека подсистемы 2, разделы 3–5): дерево
//! команд по ключам таблицы, подменю и разделителей. Порядок и состав повторяют главное
//! меню Figma UI3 без плагинов, виджетов и Dev Mode. В дереве — только команды,
//! которые уже есть: пункты следующих этапов (вставить поверх, с заменой,
````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
//! `Execute` с ключом команды.

use crate::commands::{CommandGroup, CommandId};

/// Узел меню.
````

на:

````rust
//! `Execute` с ключом команды.

use ferrum_doc::NodeId;

use crate::commands::{CommandGroup, CommandId};

/// Действие контекстного меню над его целью (слой, страница). Это не команда
/// таблицы: у него нет сочетания и смысла без цели, а выполняет его интерфейс
/// (поле имени, подтверждение удаления, ссылка в буфер обмена) — ядро только
/// говорит, доступно ли оно.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    /// Переименовать слой (поле имени в его строке).
    RenameLayer,
    /// Развернуть все слои страницы.
    ExpandAll,
    /// Свернуть все слои страницы.
    CollapseAll,
    RenamePage,
    DuplicatePage,
    DeletePage,
    /// Ссылка на страницу (`#/file/<id>?page=<pageId>`) — в буфер обмена.
    CopyPageLink,
}

impl MenuAction {
    pub const ALL: [MenuAction; 7] = [
        MenuAction::RenameLayer,
        MenuAction::ExpandAll,
        MenuAction::CollapseAll,
        MenuAction::RenamePage,
        MenuAction::DuplicatePage,
        MenuAction::DeletePage,
        MenuAction::CopyPageLink,
    ];

    /// Название пункта.
    pub fn title(self) -> &'static str {
        match self {
            MenuAction::RenameLayer => "Переименовать",
            MenuAction::ExpandAll => "Развернуть всё",
            MenuAction::CollapseAll => "Свернуть всё",
            MenuAction::RenamePage => "Переименовать",
            MenuAction::DuplicatePage => "Дублировать страницу",
            MenuAction::DeletePage => "Удалить страницу",
            MenuAction::CopyPageLink => "Копировать ссылку на страницу",
        }
    }
}

/// Узел меню.
````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
    },
    Separator,
}

````

на:

````rust
    },
    Separator,
    /// Пункт «Выделить слой»: узел под точкой щелчка и его имя.
    Node {
        id: NodeId,
        name: String,
    },
    /// Действие над целью контекстного меню и доступно ли оно.
    Action {
        action: MenuAction,
        enabled: bool,
    },
}

````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
use MenuNode::{Command as I, Separator as S};

fn sub(title: &'static str, items: Vec<MenuNode>) -> MenuNode {
    MenuNode::Submenu { title, items }
}
````

на:

````rust
use MenuNode::{Command as I, Separator as S};

pub(crate) fn sub(title: &'static str, items: Vec<MenuNode>) -> MenuNode {
    MenuNode::Submenu { title, items }
}
````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
            MenuNode::Command(c) => out.push(*c),
            MenuNode::Submenu { items, .. } => out.extend(menu_commands(items)),
            MenuNode::Separator => {}
        }
    }
````

на:

````rust
            MenuNode::Command(c) => out.push(*c),
            MenuNode::Submenu { items, .. } => out.extend(menu_commands(items)),
            MenuNode::Separator | MenuNode::Node { .. } | MenuNode::Action { .. } => {}
        }
    }
````

В `crates/ferrum-editor/src/pages.rs` заменить:

````rust
//! Страницы из панели «Страницы» (этап 5): создать, переименовать, удалить,
//! цвет фона. Как и в `ferrum-doc`, операции со страницами не попадают в историю
//! отмены. Имена из интерфейса не доверенные: `clean_name` убирает управляющие
````

на:

````rust
//! Страницы из панели «Страницы» (этап 5): создать, дублировать, переименовать, удалить,
//! цвет фона. Как и в `ferrum-doc`, операции со страницами не попадают в историю
//! отмены. Имена из интерфейса не доверенные: `clean_name` убирает управляющие
````

В `crates/ferrum-editor/src/pages.rs` заменить:

````rust
        self.set_page(id)?;
        Ok(id)
    }

````

на:

````rust
        self.set_page(id)?;
        Ok(id)
    }

    /// «Дублировать страницу»: копия встаёт сразу после страницы с именем
    /// «<имя> (копия)» и становится текущей, как новая страница (в историю
    /// отмены не попадает, как и остальные операции со страницами).
    pub fn duplicate_page(&mut self, id: PageId) -> DocResult<PageId> {
        let Some((_, name)) = self.doc.pages().into_iter().find(|(p, _)| *p == id) else {
            return Err(ferrum_doc::DocError::PageNotFound(id.to_string()));
        };
        self.finish_pen_path();
        // Имя копии — не длиннее предела, с пометкой в конце.
        const MARK: &str = " (копия)";
        let base: String = name
            .chars()
            .take(MAX_NAME_CHARS - MARK.chars().count())
            .collect();
        let copy = self
            .doc
            .duplicate_page(id, &format!("{}{MARK}", base.trim_end()))?;
        self.set_page(copy)?;
        Ok(copy)
    }

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    /// Часы пределов экспорта SVG и PDF (в worker'е — `performance.now()`).
    export_clock: Option<Box<dyn Clock>>,
}

````

на:

````rust
    /// Часы пределов экспорта SVG и PDF (в worker'е — `performance.now()`).
    export_clock: Option<Box<dyn Clock>>,
    /// Пункты контекстного меню: уходят последними в пачке событий, после
    /// выделения и состояний команд, которые изменил выбор на правой кнопке.
    context: Option<Vec<crate::protocol::MenuItemDto>>,
}

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            | Command::LayersWindow { .. }
            | Command::ExpandLayer { .. }
            | Command::FilterLayers { .. }
            | Command::PrepareSearch
````

на:

````rust
            | Command::LayersWindow { .. }
            | Command::ExpandLayer { .. }
            | Command::ExpandAllLayers { .. }
            | Command::FilterLayers { .. }
            | Command::PrepareSearch
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            now_ms: 0.0,
            export_clock: ferrum_render::default_clock(),
        };
        // Таблица команд, их состояния и пресеты нужны интерфейсу сразу, до документа.
````

на:

````rust
            now_ms: 0.0,
            export_clock: ferrum_render::default_clock(),
            context: None,
        };
        // Таблица команд, их состояния и пресеты нужны интерфейсу сразу, до документа.
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        self.push_modified();
        self.push_font_requests();
    }

````

на:

````rust
        self.push_modified();
        self.push_font_requests();
        if let Some(items) = self.context.take() {
            self.events.push(CoreEvent::ContextMenu { items });
        }
    }

````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
use ferrum_editor::scale::{Anchor, MAX_SCALE_FACTOR, MIN_SCALE_FACTOR};
use ferrum_editor::{
    browser_reserved, frame_presets, main_menu, BindError, CommandId, DropTarget, KeymapError,
    MenuNode, NumField, PanelEdit, Phase, Shortcut, COMMANDS, MAX_NAME_CHARS,
};
use ferrum_render::{decode_png, export_area, ExportBudget, ExportError, SvgOptions};
````

на:

````rust
use ferrum_editor::scale::{Anchor, MAX_SCALE_FACTOR, MIN_SCALE_FACTOR};
use ferrum_editor::{
    browser_reserved, frame_presets, main_menu, BindError, CommandId, ContextTarget, DropTarget,
    KeymapError, MenuAction, MenuNode, NumField, PanelEdit, Phase, Shortcut, COMMANDS,
    MAX_NAME_CHARS,
};
use ferrum_render::{decode_png, export_area, ExportBudget, ExportError, SvgOptions};
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
use super::Core;
use crate::protocol::{
    Align, Command, ConflictInfo, CoreEvent, DropPosition, Edit, EditPhase, ExportFormat, Field,
    FillDto, KeymapEntry, MenuItemDto, PageInfo, PaintKind, PresetInfo, ScaleModeDto, SelectMode,
    StrokeDto, EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{cap_of, join_of, summarize, MAX_PAINTS};
````

на:

````rust
use super::Core;
use crate::protocol::{
    Align, Command, ConflictInfo, ContextTarget as TargetDto, CoreEvent, DropPosition, Edit,
    EditPhase, ExportFormat, Field, FillDto, KeymapEntry, MenuActionDto, MenuItemDto, PageInfo,
    PaintKind, PresetInfo, ScaleModeDto, SelectMode, StrokeDto, EXPORT_SCALES, MAX_TEXT_BYTES,
    SELECTION_IDS_MAX,
};
use crate::summary::{cap_of, join_of, summarize, MAX_PAINTS};
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
        ExportError::Write(why) => format!("Не удалось скопировать: SVG не собран ({why})"),
        e => export_message(e),
    }
}
````

на:

````rust
        ExportError::Write(why) => format!("Не удалось скопировать: SVG не собран ({why})"),
        e => export_message(e),
    }
}

/// Узел меню ядра для протокола: команды — ключами таблицы.
pub(super) fn menu_dto(n: &MenuNode) -> MenuItemDto {
    match n {
        MenuNode::Command(c) => MenuItemDto::Command {
            key: c.key().to_string(),
        },
        MenuNode::Submenu { title, items } => MenuItemDto::Submenu {
            title: title.to_string(),
            items: items.iter().map(menu_dto).collect(),
        },
        MenuNode::Separator => MenuItemDto::Separator,
        MenuNode::Node { id, name } => MenuItemDto::Node {
            id: id.to_string(),
            name: event_name(name),
        },
        MenuNode::Action { action, enabled } => MenuItemDto::Action {
            action: match action {
                MenuAction::RenameLayer => MenuActionDto::RenameLayer,
                MenuAction::ExpandAll => MenuActionDto::ExpandAll,
                MenuAction::CollapseAll => MenuActionDto::CollapseAll,
                MenuAction::RenamePage => MenuActionDto::RenamePage,
                MenuAction::DuplicatePage => MenuActionDto::DuplicatePage,
                MenuAction::DeletePage => MenuActionDto::DeletePage,
                MenuAction::CopyPageLink => MenuActionDto::CopyPageLink,
            },
            title: action.title().to_string(),
            enabled: *enabled,
        },
    }
}
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
                }
            },
            Command::AddPage => {
                match self.editor.create_page() {
````

на:

````rust
                }
            },
            Command::ContextMenu { target } => self.context_menu(target),
            Command::ExpandAllLayers { expanded } => {
                self.layers.set_all_expanded(self.editor.scene(), expanded);
                self.ui.layers = true;
            }
            Command::DuplicatePage { id } => {
                if let Some(id) = self.page_id(&id) {
                    match self.editor.duplicate_page(id) {
                        Ok(_) => self.touch(),
                        Err(e) => self.error(e.to_string()),
                    }
                }
                self.ui.pages = true;
            }
            Command::AddPage => {
                match self.editor.create_page() {
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
    }

    fn select(&mut self, ids: Vec<NodeId>, mode: SelectMode) {
        let next = match mode {
````

на:

````rust
    }

    /// Контекстное меню: выбор на правой кнопке (у строки слоя — как выбор из
    /// панели: без раскрытия предков и прокрутки) и пункты — событием в конце
    /// пачки. Неверная цель — ошибка и пустое меню.
    fn context_menu(&mut self, target: TargetDto) {
        let target = match target {
            TargetDto::Canvas { x, y } if x.is_finite() && y.is_finite() => {
                Some(ContextTarget::Canvas(kurbo::Point::new(x, y)))
            }
            TargetDto::Canvas { .. } => {
                self.error("Неверная точка контекстного меню");
                None
            }
            TargetDto::Layer { id } => self.node(&id).map(ContextTarget::Layer),
            TargetDto::Page { id } => self.page_id(&id).map(ContextTarget::Page),
        };
        let Some(target) = target else {
            self.context = Some(Vec::new());
            return;
        };
        let layer = matches!(target, ContextTarget::Layer(_));
        self.from_panel = layer;
        let items = self.editor.context_menu(target);
        if layer {
            self.drain_editor();
            self.from_panel = false;
        }
        self.context = Some(items.iter().map(menu_dto).collect());
    }

    fn select(&mut self, ids: Vec<NodeId>, mode: SelectMode) {
        let next = match mode {
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
    /// Главное меню: дерево ядра ключами команд.
    pub(super) fn push_main_menu(&mut self) {
        fn dto(n: &MenuNode) -> MenuItemDto {
            match n {
                MenuNode::Command(c) => MenuItemDto::Command {
                    key: c.key().to_string(),
                },
                MenuNode::Submenu { title, items } => MenuItemDto::Submenu {
                    title: title.to_string(),
                    items: items.iter().map(dto).collect(),
                },
                MenuNode::Separator => MenuItemDto::Separator,
            }
        }
        let tree = main_menu().iter().map(dto).collect();
        self.events.push(CoreEvent::MainMenu { tree });
    }
````

на:

````rust
    /// Главное меню: дерево ядра ключами команд.
    pub(super) fn push_main_menu(&mut self) {
        let tree = main_menu().iter().map(menu_dto).collect();
        self.events.push(CoreEvent::MainMenu { tree });
    }
````

В `crates/ferrum-wasm/src/layers.rs` заменить:

````rust
        };
        self.stale |= changed;
    }

````

на:

````rust
        };
        self.stale |= changed;
    }

    /// Развернуть все узлы страницы с детьми или свернуть все.
    pub fn set_all_expanded(&mut self, scene: &Scene, expanded: bool) {
        let before = self.expanded.len();
        if expanded {
            let mut stack: Vec<Slot> = scene.roots().to_vec();
            while let Some(s) = stack.pop() {
                let n = scene.at(s);
                if !n.children.is_empty() && !scene.is_draft_slot(s) {
                    self.expanded.insert(n.id);
                    stack.extend(n.children.iter().copied());
                }
            }
        } else {
            self.expanded.clear();
        }
        self.stale |= self.expanded.len() != before;
    }

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
}

/// Команда основного потока worker'у.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

на:

````rust
}

/// По чему щёлкнули правой кнопкой (спека подсистемы 2, раздел 5): точка холста
/// в CSS-пикселях (как у событий указателя), строка слоя или страница.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ContextTarget {
    Canvas { x: f64, y: f64 },
    Layer { id: String },
    Page { id: String },
}

/// Команда основного потока worker'у.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        selection_version: Option<f64>,
    },
    /// Новая страница; она становится текущей.
    AddPage,
````

на:

````rust
        selection_version: Option<f64>,
    },
    /// Контекстное меню (спека подсистемы 2, раздел 5): выбор на правой кнопке
    /// и ответ — событие `ContextMenu` с пунктами.
    ContextMenu {
        target: ContextTarget,
    },
    /// Развернуть (`true`) или свернуть все слои страницы.
    ExpandAllLayers {
        expanded: bool,
    },
    /// Новая страница; она становится текущей.
    AddPage,
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    },
    DeletePage {
        id: String,
    },
````

на:

````rust
    },
    DeletePage {
        id: String,
    },
    /// Копия страницы сразу после неё; копия становится текущей.
    DuplicatePage {
        id: String,
    },
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
}

/// Пункт меню: команда по ключу таблицы, подменю или разделитель.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
````

на:

````rust
}

/// Действие контекстного меню над его целью: его выполняет интерфейс.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum MenuActionDto {
    RenameLayer,
    ExpandAll,
    CollapseAll,
    RenamePage,
    DuplicatePage,
    DeletePage,
    CopyPageLink,
}

/// Пункт меню: команда по ключу таблицы, подменю, разделитель; в контекстном
/// меню ещё узел под точкой щелчка («Выделить слой») и действие над целью.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    },
    Separator,
}

````

на:

````rust
    },
    Separator,
    /// Выделить узел `id` (`Select` с ним одним).
    Node {
        id: String,
        name: String,
    },
    Action {
        action: MenuActionDto,
        title: String,
        enabled: bool,
    },
}

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        tree: Vec<MenuItemDto>,
    },
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
    /// пунктирных направляющих.
````

на:

````rust
        tree: Vec<MenuItemDto>,
    },
    /// Контекстное меню (ответ на `ContextMenu`): пункты после выбора на правой
    /// кнопке — состояния команд уже пришли раньше в той же пачке. Пусто —
    /// меню не открывать.
    ContextMenu {
        items: Vec<MenuItemDto>,
    },
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
    /// пунктирных направляющих.
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Expected: PASS: fmt и clippy (нативно и wasm32) без замечаний; `ferrum-doc`, `ferrum-editor` и `ferrum-wasm` без падений (новые: `ferrum-editor/tests/context_menu.rs` — 9 тестов, `ferrum-wasm/tests/context_menu.rs` — 4, `journal.rs` — 11); wasm собран, `npm run typecheck` без ошибок

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/src/ui/main-menu.ts crates/ferrum-doc/src/document.rs crates/ferrum-doc/tests/journal.rs crates/ferrum-editor/src/context_menu.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/menu.rs crates/ferrum-editor/src/pages.rs crates/ferrum-editor/tests/context_menu.rs crates/ferrum-editor/tests/main_menu.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/layers.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/context_menu.rs
git commit -F - <<'EOF'
feat(editor,wasm): контекстные меню данными в ядре — выбор на правой кнопке, холст, слой, страница

ContextMenu { target: canvas | layer | page } выделяет, как Figma на правой кнопке
(невыделенный узел — выделяется, выделенный и пустое место в рамке выделения —
сохраняют, пустое место вне её — снимает), и отвечает событием ContextMenu с
деревом MenuItemDto последним в пачке. Новые пункты дерева: узел «Выделить слой»
(все узлы под точкой с предками, до 50) и действие над целью (переименовать,
развернуть и свернуть всё, переименовать, дублировать и удалить страницу, ссылка
на страницу). Команды ExpandAllLayers и DuplicatePage; Document::duplicate_page —
копия страницы сразу после неё с новыми идентификаторами, мимо истории отмены.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 2: Контекстное меню холста из ядра — у точки щелчка, «Выделить слой»

Спека §5: правый щелчок по холсту шлёт `ContextMenu { target: canvas }` (точка — `offsetX/Y`), пункты приходят событием и встают в очередь планировщика после выделения и `CommandStates` той же пачки. `buildContextMenu` превращает дерево ядра в пункты компонента меню: команды — названия и сочетания из таблицы, «Выделить слой» — выбор узла, действия цели — `onAction` того, кто открыл меню. Локальное меню холста (`canvasMenu` в `chrome.tsx`) удаляется; тесты `chrome.spec.ts` — под состав Figma.

**Files:**
- Create: `apps/web/src/ui/context-menu.ts`
- Modify: `apps/web/src/ui/{app.tsx, chrome.tsx, context.ts, main-menu.ts}` (`tidy` экспортирован)
- Test: `apps/web/unit/context-menu.test.ts` (новый), `apps/web/e2e/context-menu.spec.ts` (новый), `apps/web/e2e/chrome.spec.ts`

**Interfaces:**
- Consumes: `CoreEvent::ContextMenu`, `MenuItemDto`, `store.state.commands`, `KeymapEntry`, компонент меню этапа 1, `placeMenu`, `scheduler.enqueue`.
- Produces: `buildContextMenu(tree: readonly MenuItemDto[], entries: ReadonlyMap<string, KeymapEntry>, on: ContextHandlers): MenuItem[]`, `interface ContextHandlers { select, action }`; `tidy(items)` из `main-menu.ts`; `Editor.contextMenu?(target, at, label, onAction?)` (ставит `App`).

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  await page.mouse.click(node.x, node.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu.getByRole('menuitem', { name: /Дублировать/ })).toContainText(/D$/);
  await menu.getByRole('menuitem', { name: /Дублировать/ }).click();
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expect(menu).toHaveCount(0);
````

на:

````ts
  await page.mouse.click(node.x, node.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu.getByRole('menuitem', { name: /^Сгруппировать/ })).toContainText(/G$/);
  await menu.getByRole('menuitem', { name: /^Сгруппировать/ }).click();
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expect(menu).toHaveCount(0);
````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  await openApp(page);
  const l = await live(page);
  // Пустое место холста: выделения нет — «Копировать» серый.
  const empty = await appScreen(page, l, 100, 1080);
  await page.mouse.click(empty.x, empty.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  const copy = menu.getByRole('menuitem', { name: /^Копировать/ }).first();
  await expect(copy).toHaveAttribute('aria-disabled', 'true');
  await expect(copy).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(menu).toBeVisible();
  // Буква «В» (русская раскладка: Playwright набирает её без keydown — событие
  // отправляется прямо): «Вырезать» (серый), затем «Вставить».
  const letter = (key: string) =>
    page.evaluate((k) => document.activeElement!.dispatchEvent(new KeyboardEvent('keydown', { key: k, code: 'KeyD', bubbles: true })), key);
  await letter('в');
  await expect(menu.getByRole('menuitem', { name: /^Вырезать/ })).toBeFocused();
  await letter('В');
  await expect(menu.getByRole('menuitem', { name: /^Вставить/ })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
````

на:

````ts
  await openApp(page);
  const l = await live(page);
  // Один прямоугольник: «Разгруппировать» серый.
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu.getByRole('menuitem', { name: /^Копировать/ }).first()).toBeFocused();
  // Буквы — русские (Playwright набирает их без keydown — событие отправляется
  // прямо): «Р» — «Разгруппировать» (серый), Enter его не выполняет.
  const letter = (key: string) =>
    page.evaluate((k) => document.activeElement!.dispatchEvent(new KeyboardEvent('keydown', { key: k, code: 'KeyD', bubbles: true })), key);
  await letter('р');
  const ungroup = menu.getByRole('menuitem', { name: /^Разгруппировать/ });
  await expect(ungroup).toHaveAttribute('aria-disabled', 'true');
  await expect(ungroup).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(menu).toBeVisible();
  // «В» — по кругу после серого пункта: «Войти в правку», «Вставить», «Выделить слой».
  await letter('в');
  await expect(menu.getByRole('menuitem', { name: /^Войти в правку/ })).toBeFocused();
  await letter('в');
  await expect(menu.getByRole('menuitem', { name: /^Вставить/ })).toBeFocused();
  await letter('В');
  await expect(menu.getByRole('menuitem', { name: /^Выделить слой/ })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
````

Создать `apps/web/e2e/context-menu.spec.ts`:

````ts
// Контекстные меню (спека подсистемы 2, раздел 5): дерево и выбор на правой
// кнопке — из ядра, меню открывается у точки щелчка; основной поток не читает
// раскладку вне фазы measure (`__ferrumWarnings` пуст).

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

const labels = (page: Page, name: string) =>
  page
    .getByRole('menu', { name, exact: true })
    .locator(':scope > [role^="menuitem"]')
    .evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.label));

test('холст: правый щелчок выделяет узел, меню у точки, «Выделить слой» — узлы под точкой', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  expect(l.selectionCount).toBe(0);
  // Мир (200, 40) — «Слой 1» в «Экран 0».
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu).toBeVisible();
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 120');
  // Меню — у точки щелчка; длинное поднимается, чтобы целиком влезть в окно.
  const box = (await menu.boundingBox())!;
  expect(Math.abs(box.x - node.x)).toBeLessThanOrEqual(1);
  expect(box.y).toBeGreaterThanOrEqual(8);
  expect(box.y + box.height).toBeLessThanOrEqual(720 - 8 + 1);
  expect(await labels(page, 'Действия с выделением')).toEqual([
    'Копировать',
    'Вставить',
    'Копировать как',
    'Выделить слой',
    'Переместить вперёд',
    'Переместить назад',
    'На передний план',
    'На задний план',
    'Сгруппировать',
    'Разгруппировать',
    'Обернуть во фрейм',
    'Булевы операции',
    'Сплющить',
    'Контур обводки',
    'Отразить по горизонтали',
    'Отразить по вертикали',
    'Скрыть или показать',
    'Заблокировать или разблокировать',
    'Войти в правку',
    'Удалить',
  ]);
  await menu.getByRole('menuitem', { name: 'Выделить слой' }).click();
  const under = page.getByRole('menu', { name: 'Выделить слой' });
  await expect(under.getByRole('menuitem', { name: 'Экран 0' })).toBeVisible();
  expect(await labels(page, 'Выделить слой')).toEqual(['Слой 1', 'Экран 0']);
  await under.getByRole('menuitem', { name: 'Экран 0' }).click();
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 1440');
  await expect(menu).toHaveCount(0);
  expect(await warnings(page)).toEqual([]);
});

test('холст: правый щелчок по выделенному сохраняет выделение, команда из меню — шаг', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const a = await appScreen(page, l, 200, 40);
  const b = await appScreen(page, l, 340, 40);
  await page.mouse.click(a.x, a.y);
  await page.keyboard.down('Shift');
  await page.mouse.click(b.x, b.y);
  await page.keyboard.up('Shift');
  await liveWhen(page, 'l.selectionCount === 2');
  await page.mouse.click(b.x, b.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await menu.getByRole('menuitem', { name: /^Сгруппировать/ }).click();
  const after = await liveWhen(page, `l.steps === ${l.steps + 1}`);
  expect(after.selectionCount).toBe(1);
  expect(after.selectionW).toBe(260);
});

test('пустое место холста: выделение снимается, меню — вставить, выделить всё, показать всё', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y);
  await liveWhen(page, 'l.selectionCount === 1');
  // Мир (100, 1080) — между фреймами.
  const empty = await appScreen(page, l, 100, 1080);
  await page.mouse.click(empty.x, empty.y, { button: 'right' });
  await liveWhen(page, 'l.selectionCount === 0');
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu).toBeVisible();
  // Короткое меню — у точки щелчка: под ней или, у низа окна, над ней (отступ 4 px).
  const box = (await menu.boundingBox())!;
  expect(Math.abs(box.x - empty.x)).toBeLessThanOrEqual(1);
  expect(Math.min(Math.abs(box.y - (empty.y + 4)), Math.abs(box.y + box.height - (empty.y - 4)))).toBeLessThanOrEqual(2);
  expect(await labels(page, 'Действия с выделением')).toEqual(['Вставить', 'Выделить всё', 'Показать всё']);
  await menu.getByRole('menuitem', { name: /^Выделить всё/ }).click();
  await liveWhen(page, 'l.selectionCount === 10');
  expect(await warnings(page)).toEqual([]);
});
````

Создать `apps/web/unit/context-menu.test.ts`:

````ts
// Контекстное меню: пункты из дерева ядра — команды с названиями из таблицы,
// «Выделить слой» и действия над целью.

import { expect, test } from '@playwright/test';
import type { KeymapEntry, MenuActionDto, MenuItemDto } from '../src/wasm/ferrum_wasm';
import { buildContextMenu } from '../src/ui/context-menu';
import { entriesByKey } from '../src/ui/main-menu';
import type { MenuItem } from '../src/ui/menu';

const entry = (key: string, name: string): KeymapEntry => ({
  key,
  name,
  group: key.split('.')[0],
  keywords: [],
  toggle: false,
  bindings: [],
  labels: [],
  defaults: [],
  overridden: false,
});

const entries = entriesByKey([entry('edit.copy', 'Копировать'), entry('edit.delete', 'Удалить')]);
const c = (key: string): MenuItemDto => ({ type: 'command', key });
const sep: MenuItemDto = { type: 'separator' };

test('команды, узлы «Выделить слой» и действия; пустое подменю и лишние разделители убраны', () => {
  const selected: string[] = [];
  const actions: MenuActionDto[] = [];
  const tree: MenuItemDto[] = [
    c('edit.copy'),
    sep,
    { type: 'submenu', title: 'Выделить слой', items: [{ type: 'node', id: '1@2', name: 'Прямоугольник' }] },
    sep,
    { type: 'submenu', title: 'Пусто', items: [c('ui.unknown')] },
    sep,
    { type: 'action', action: 'renameLayer', title: 'Переименовать', enabled: false },
    { type: 'action', action: 'expandAll', title: 'Развернуть всё', enabled: true },
    sep,
    c('edit.delete'),
  ];
  const menu = buildContextMenu(tree, entries, { select: (id) => selected.push(id), action: (a) => actions.push(a) });
  const labels = menu.map((m) => ('label' in m ? m.label : 'separator' in m ? '—' : ''));
  expect(labels).toEqual(['Копировать', '—', 'Выделить слой', '—', 'Переименовать', 'Развернуть всё', '—', 'Удалить']);
  const pick = (i: number) => menu[i] as Extract<MenuItem, { label: string }>;
  expect(pick(0).command).toBe('edit.copy');
  (pick(2).submenu![0] as Extract<MenuItem, { label: string }>).onSelect!();
  expect(selected).toEqual(['1@2']);
  expect(pick(4).disabled).toBe(true);
  expect(pick(5).disabled).toBe(false);
  pick(5).onSelect!();
  expect(actions).toEqual(['expandAll']);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/context-menu.test.ts`
Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/context-menu.spec.ts e2e/chrome.spec.ts --project=chromium`
Expected: FAIL — модульные — `Cannot find module '…/apps/web/src/ui/context-menu'` (No tests found); E2E (стенд собран) — новые тесты `context-menu.spec.ts` падают (правый щелчок открывает прежнее локальное меню: нет «Выделить слой ▸», «Копировать как ▸», блоков §5 и меню пустого места), в `chrome.spec.ts` — тест первой буквы (в прежнем меню нет «Войти в правку» и «Выделить слой»)

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES } from '../core/limits';
import { boot, dismissSplash, isTestMode } from './boot';
import { canvasMenu, DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { EditorContext, type Editor } from './context';
import { ConfirmDialog } from './dialog';
import { Menu, type MenuSearch } from './menu';
import { buildMainMenu, entriesByKey, searchMenu } from './main-menu';
import { LayersPanel } from './layers';
````

на:

````tsx
import { MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES } from '../core/limits';
import { boot, dismissSplash, isTestMode } from './boot';
import { DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { buildContextMenu, type ContextTarget } from './context-menu';
import { EditorContext, type Editor } from './context';
import { ConfirmDialog } from './dialog';
import { Menu, type MenuItem, type MenuSearch } from './menu';
import { buildMainMenu, entriesByKey, searchMenu } from './main-menu';
import { LayersPanel } from './layers';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { systemFonts } from './system-fonts';
import { isTauri, saveExport } from './tauri';
import type { Backend } from '../wasm/ferrum_wasm';
import { restartText, type Restart } from './restart';
import type { Route } from './router';
````

на:

````tsx
import { systemFonts } from './system-fonts';
import { isTauri, saveExport } from './tauri';
import type { Backend, MenuActionDto } from '../wasm/ferrum_wasm';
import { restartText, type Restart } from './restart';
import type { Route } from './router';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [context, setContext] = createSignal<{ x: number; y: number } | null>(null);
  // «Как в системе» следует за prefers-color-scheme на лету.
  const media = matchMedia('(prefers-color-scheme: dark)');
````

на:

````tsx
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  /** Открытое контекстное меню: пункты из ядра, точка окна и название. */
  const [context, setContext] = createSignal<{ items: MenuItem[]; at: { x: number; y: number }; label: string } | null>(null);
  /** Запрошенное у ядра меню: куда его поставить и кто выполняет действия цели. */
  let pendingContext: { at: { x: number; y: number }; label: string; onAction?: (a: MenuActionDto) => void } | null = null;
  // «Как в системе» следует за prefers-color-scheme на лету.
  const media = matchMedia('(prefers-color-scheme: dark)');
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        }
        ed.uiCommand = uiCommand;
        setEditor(ed);
        canvas.focus();
````

на:

````tsx
        }
        ed.uiCommand = uiCommand;
        ed.contextMenu = (target: ContextTarget, at, label, onAction) => {
          setContext(null);
          pendingContext = { at, label, onAction };
          ed.send({ type: 'contextMenu', target });
        };
        // Пункты — в той же пачке после выделения и состояний команд (ядро шлёт
        // меню последним): через очередь планировщика, вслед за ними.
        const offContext = ed.core.on((e) => {
          if (e.type !== 'contextMenu') return;
          ed.scheduler.enqueue(() => {
            const p = pendingContext;
            pendingContext = null;
            if (!p || e.items.length === 0) return;
            const items = buildContextMenu(e.items, entries(), {
              select: (id) => ed.send({ type: 'select', ids: [id], mode: 'replace' }),
              action: (a) => p.onAction?.(a),
            });
            setContext({ items, at: p.at, label: p.label });
          });
        });
        setEditor(ed);
        canvas.focus();
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        const offText = installTextInput(ed, canvas.parentElement ?? document.body, canvas);
        uninstall = () => {
          offText();
          offClipboard();
````

на:

````tsx
        const offText = installTextInput(ed, canvas.parentElement ?? document.body, canvas);
        uninstall = () => {
          offContext();
          offText();
          offClipboard();
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
          aria-label="Холст"
          onContextMenu={(e) => {
            if (editor()) setContext({ x: e.clientX, y: e.clientY });
          }}
        />
````

на:

````tsx
          aria-label="Холст"
          onContextMenu={(e) => {
            // Точка холста — `offset*` (не чтение layout); окна — для меню.
            editor()?.contextMenu?.({ kind: 'canvas', x: e.offsetX, y: e.offsetY }, { x: e.clientX, y: e.clientY }, 'Действия с выделением');
          }}
        />
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
            </Show>
            <Show when={context()}>
              {(at) => (
                <Menu
                  label="Действия с выделением"
                  items={canvasMenu()}
                  anchor={at()}
                  onClose={() => setContext(null)}
                />
              )}
            </Show>
          </EditorContext.Provider>
````

на:

````tsx
            </Show>
            <Show when={context()}>
              {(m) => <Menu label={m().label} items={m().items} anchor={m().at} onClose={() => setContext(null)} />}
            </Show>
          </EditorContext.Provider>
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
// Шапки панелей (PanelHeader), главное меню, зум, «Поделиться», вкладки, края
// панелей для изменения ширины и контекстное меню холста.

import { createSignal, onCleanup, onMount, Show, type JSX } from 'solid-js';
````

на:

````tsx
// Шапки панелей (PanelHeader), главное меню, зум, «Поделиться», вкладки, края
// панелей для изменения ширины.

import { createSignal, onCleanup, onMount, Show, type JSX } from 'solid-js';
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
}

/** Контекстное меню холста: команды редактора; доступность — из состояния
 * команд ядра (основной поток о выделении ничего не вычисляет). */
export function canvasMenu(): MenuItem[] {
  const item = (label: string, command: string): MenuItem => ({ label, command });
  return [
    item('Копировать', 'edit.copy'),
    item('Вырезать', 'edit.cut'),
    item('Вставить', 'edit.paste'),
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
    item('Копировать как SVG', 'edit.copy_svg'),
    item('Выделить всё', 'edit.select_all'),
    item('Показать всё', 'view.fit_all'),
  ];
}

export function Section(props: { title: string; children: JSX.Element; actions?: JSX.Element; muted?: boolean }) {
  return (
````

на:

````tsx
}

export function Section(props: { title: string; children: JSX.Element; actions?: JSX.Element; muted?: boolean }) {
  return (
````

Создать `apps/web/src/ui/context-menu.ts`:

````ts
// Контекстные меню (спека подсистемы 2, раздел 5): дерево — из ядра (событие
// `contextMenu` в ответ на команду `contextMenu`), здесь — только названия из
// таблицы команд и что делать с пунктами, которых в таблице нет: узел «Выделить
// слой» и действие над целью меню (их выполняет тот, кто меню открыл). Основной
// поток ничего не вычисляет о выделении: выбор на правой кнопке и состав меню
// делает ядро, доступность команд — из `CommandStates`.

import type { ContextTarget, KeymapEntry, MenuActionDto, MenuItemDto } from '../wasm/ferrum_wasm';
import { tidy } from './main-menu';
import type { MenuItem } from './menu';

export type { ContextTarget };

/** Что делать с пунктами не из таблицы команд. */
export interface ContextHandlers {
  /** «Выделить слой» → узел. */
  select(id: string): void;
  /** Действие над целью меню (переименовать, страницы…). */
  action(action: MenuActionDto): void;
}

/** Пункты меню интерфейса из дерева ядра. Команды без записи в таблице
 * пропускаются, пустые подменю тоже. */
export function buildContextMenu(tree: readonly MenuItemDto[], entries: ReadonlyMap<string, KeymapEntry>, on: ContextHandlers): MenuItem[] {
  const out: MenuItem[] = [];
  for (const n of tree) {
    switch (n.type) {
      case 'separator':
        out.push({ separator: true });
        break;
      case 'submenu': {
        const items = buildContextMenu(n.items, entries, on);
        if (items.length) out.push({ label: n.title, submenu: items });
        break;
      }
      case 'command': {
        const e = entries.get(n.key);
        if (e) out.push({ label: e.name, command: e.key });
        break;
      }
      case 'node': {
        const id = n.id;
        out.push({ label: n.name, onSelect: () => on.select(id) });
        break;
      }
      case 'action': {
        const a = n.action;
        out.push({ label: n.title, disabled: !n.enabled, onSelect: () => on.action(a) });
        break;
      }
    }
  }
  return tidy(out);
}
````

В `apps/web/src/ui/context.ts` заменить:

````ts

import { createContext, useContext } from 'solid-js';
import type { Command } from '../wasm/ferrum_wasm';
import type { Core, Platform } from '../core/core';
import type { Scheduler } from './scheduler';
````

на:

````ts

import { createContext, useContext } from 'solid-js';
import type { Command, ContextTarget, MenuActionDto } from '../wasm/ferrum_wasm';
import type { Core, Platform } from '../core/core';
import type { Scheduler } from './scheduler';
````

В `apps/web/src/ui/context.ts` заменить:

````ts
   * сообщает о них событием `uiCommand` (сочетание клавиш на холсте). */
  uiCommand?: (key: string) => void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
````

на:

````ts
   * сообщает о них событием `uiCommand` (сочетание клавиш на холсте). */
  uiCommand?: (key: string) => void;
  /** Открыть контекстное меню цели у точки окна `at` (спека подсистемы 2, раздел
   * 5): ядро выбирает на правой кнопке и присылает пункты. `onAction` — действия
   * над целью (переименовать, страницы). Ставит редактор приложения. */
  contextMenu?: (target: ContextTarget, at: { x: number; y: number }, label: string, onAction?: (a: MenuActionDto) => void) => void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
````

В `apps/web/src/ui/main-menu.ts` заменить:

````ts

/** Убирает разделители в начале, в конце и подряд (после пропуска пунктов). */
function tidy(items: MenuItem[]): MenuItem[] {
  const out: MenuItem[] = [];
  for (const it of items) {
````

на:

````ts

/** Убирает разделители в начале, в конце и подряд (после пропуска пунктов). */
export function tidy(items: MenuItem[]): MenuItem[] {
  const out: MenuItem[] = [];
  for (const it of items) {
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/context-menu.spec.ts e2e/chrome.spec.ts e2e/main-menu.spec.ts --project=chromium`

Expected: модульные — `context-menu.test.ts` зелёный, сборка — ок; E2E `context-menu` (3 теста), `chrome`, `main-menu` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/chrome.spec.ts apps/web/e2e/context-menu.spec.ts apps/web/src/ui/app.tsx apps/web/src/ui/chrome.tsx apps/web/src/ui/context-menu.ts apps/web/src/ui/context.ts apps/web/src/ui/main-menu.ts apps/web/unit/context-menu.test.ts
git commit -F - <<'EOF'
feat(web): контекстное меню холста из ядра — у точки щелчка, «Выделить слой»

Правый щелчок по холсту шлёт ContextMenu { target: canvas } (точка — offsetX/Y,
без чтения раскладки), пункты приходят событием и встают в очередь планировщика
вслед за выделением и состояниями команд. Меню собирается из дерева ядра
(buildContextMenu): команды — названия из таблицы, «Выделить слой» — выбор узла,
действия цели — тому, кто открыл меню (Editor.contextMenu). Локальное меню холста
(canvasMenu) удалено; тесты chrome.spec — под состав Figma.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3: Меню слоёв и страниц из ядра, ссылка на страницу

Спека §5: правый щелчок по строке слоя — `ContextMenu { target: layer }` (невыделенная строка выделяется, как из панели); «Переименовать» — поле имени в строке, «Развернуть всё» и «Свернуть всё» — `ExpandAllLayers`. По вкладке страницы — «Переименовать», «Дублировать страницу» (`DuplicatePage`), «Удалить страницу» (с подтверждением; единственную — серым) и «Копировать ссылку на страницу». Маршрут разбирает `?page=`, редактор открывает эту страницу по первому списку страниц и оставляет в адресе только файл.

**Files:**
- Modify: `apps/web/src/ui/{layers.tsx, pages.tsx, router.ts, root.tsx, app.tsx}`
- Test: `apps/web/unit/router.test.ts`, `apps/web/e2e/context-menu.spec.ts`

**Interfaces:**
- Consumes: `Editor.contextMenu`, `Command::{ContextMenu, ExpandAllLayers, DuplicatePage}`, `MenuAction`.
- Produces: `Route` файла — `{ kind: 'file'; id: string; page?: string }`, `parseRoute` разбирает `?page=`; `View.editor.page`, `App.page`; `pageLink(fileId: string, pageId: string): string` в `pages.tsx`; `PagesPanel(props: { open; onToggle; fileId })`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/context-menu.spec.ts` заменить:

````ts

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

const labels = (page: Page, name: string) =>
  page
    .getByRole('menu', { name, exact: true })
    .locator(':scope > [role^="menuitem"]')
    .evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.label));

test('холст: правый щелчок выделяет узел, меню у точки, «Выделить слой» — узлы под точкой', async ({ page }) => {
````

на:

````ts

import { expect, test, type Page } from '@playwright/test';
import { appScreen, fileId, live, liveWhen, openApp } from './helpers';

const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

/** Названия пунктов открытого меню (сначала ждём, пока оно встанет на место). */
async function labels(page: Page, name: string): Promise<(string | undefined)[]> {
  const items = page.getByRole('menu', { name, exact: true }).locator(':scope > [role^="menuitem"]');
  await expect(items.first()).toBeVisible();
  return items.evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.label));
}

test('холст: правый щелчок выделяет узел, меню у точки, «Выделить слой» — узлы под точкой', async ({ page }) => {
````

В `apps/web/e2e/context-menu.spec.ts` заменить:

````ts
  await liveWhen(page, 'l.selectionCount === 10');
  expect(await warnings(page)).toEqual([]);
});
````

на:

````ts
  await liveWhen(page, 'l.selectionCount === 10');
  expect(await warnings(page)).toEqual([]);
});

test('слои: правый щелчок выделяет строку; «Развернуть всё», «Свернуть всё», «Переименовать»', async ({ page }) => {
  await openApp(page);
  const tree = page.getByRole('tree', { name: 'Слои' });
  const row = tree.getByRole('treeitem', { name: 'Экран 3', exact: true });
  await row.click({ button: 'right' });
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 1440');
  await expect(row).toHaveAttribute('aria-selected', 'true');
  const menu = page.getByRole('menu', { name: 'Действия со слоем' });
  await expect(menu).toBeVisible();
  const items = await labels(page, 'Действия со слоем');
  expect(items.slice(-4)).toEqual(['Переименовать', 'Развернуть всё', 'Свернуть всё', 'Удалить']);
  expect(items).not.toContain('Выделить слой');
  await menu.getByRole('menuitem', { name: 'Развернуть всё' }).click();
  // Все 10 фреймов раскрыты: 10 + 990 строк.
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(1000);
  await tree.getByRole('treeitem').first().click({ button: 'right' });
  await menu.getByRole('menuitem', { name: 'Свернуть всё' }).click();
  await expect(row).toHaveAttribute('aria-expanded', 'false');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(10);
  await row.click({ button: 'right' });
  await menu.getByRole('menuitem', { name: 'Переименовать' }).click();
  const input = page.getByRole('textbox', { name: 'Имя слоя' });
  await expect(input).toBeFocused();
  await input.fill('Главный');
  await input.press('Enter');
  await expect(tree.getByRole('treeitem', { name: 'Главный', exact: true })).toBeVisible();
  expect(await warnings(page)).toEqual([]);
});

test('страницы: удалить единственную нельзя, «Дублировать», ссылка на страницу открывает её', async ({ page, browserName }) => {
  if (browserName === 'chromium') await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await openApp(page);
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  const first = pages.getByRole('option').first();
  await first.click({ button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия со страницей' });
  expect(await labels(page, 'Действия со страницей')).toEqual([
    'Переименовать',
    'Дублировать страницу',
    'Удалить страницу',
    'Копировать ссылку на страницу',
  ]);
  await expect(menu.getByRole('menuitem', { name: 'Удалить страницу' })).toHaveAttribute('aria-disabled', 'true');
  await menu.getByRole('menuitem', { name: 'Дублировать страницу' }).click();
  const copy = pages.getByRole('option', { name: /Страница 1 \(копия\)/ });
  await expect(copy).toHaveAttribute('aria-selected', 'true');
  await expect(pages.getByRole('option')).toHaveCount(2);
  await liveWhen(page, 'l.nodes === 1000');
  // Ссылка на первую страницу (сейчас открыта копия).
  await first.click({ button: 'right' });
  await menu.getByRole('menuitem', { name: 'Копировать ссылку на страницу' }).click();
  await expect(page.getByText('Ссылка на страницу скопирована')).toBeVisible();
  const firstId = await page.evaluate(() => window.__ferrum!.ui!.pages.list[0].id);
  const id = fileId(page);
  const link = await page.evaluate(([f, p]) => `${location.origin}${location.pathname}${location.search}#/file/${f}?page=${encodeURIComponent(p)}`, [id, firstId]);
  if (browserName === 'chromium') expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(link);
  // Переход по ссылке: файл открывается заново (сохранившись) — на этой странице.
  await page.evaluate((l) => (location.href = l), link);
  await page.waitForFunction(
    (p) => window.__ferrum?.ui?.ready === true && window.__ferrum.ui.pages.list.length === 2 && window.__ferrum.ui.pages.current === p,
    firstId,
    { timeout: 30_000 },
  );
  // В адресе остаётся только файл.
  await expect.poll(() => page.url()).toMatch(new RegExp(`#/file/${id}$`));
  expect(await warnings(page)).toEqual([]);
});
````

В `apps/web/unit/router.test.ts` заменить:

````ts
  expect(parseRoute('#/file/a'.padEnd(72, 'a'))).toEqual({ kind: 'file', id: '' });
});
````

на:

````ts
  expect(parseRoute('#/file/a'.padEnd(72, 'a'))).toEqual({ kind: 'file', id: '' });
});

test('ссылка на страницу: `?page=` разбирается, неверная страница отбрасывается', () => {
  expect(parseRoute('#/file/abc?page=12@345')).toEqual({ kind: 'file', id: 'abc', page: '12@345' });
  expect(parseRoute(routeHash({ kind: 'file', id: 'abc', page: '1@2' }))).toEqual({ kind: 'file', id: 'abc', page: '1@2' });
  expect(routeHash({ kind: 'file', id: 'abc', page: '1@2' })).toBe('#/file/abc?page=1%402');
  for (const bad of ['<script>', '1@', '@2', '1@2@3', '1'.repeat(70) + '@2', '']) {
    expect(parseRoute(`#/file/abc?page=${encodeURIComponent(bad)}`)).toEqual({ kind: 'file', id: 'abc' });
  }
  expect(parseRoute('#/file/abc?other=1')).toEqual({ kind: 'file', id: 'abc' });
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/router.test.ts`
Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/context-menu.spec.ts --project=chromium`
Expected: FAIL — модульные — новый тест «ссылка на страницу…» в `router.test.ts` падает: `parseRoute` и `routeHash` не знают `page`; E2E (стенд собран) — новые тесты меню слоя и страницы в `context-menu.spec.ts` падают: правый щелчок по строке слоя и вкладке страницы меню ядра не открывает

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  onRestart: (r: Restart) => boolean;
  navigate: (r: Route) => void;
}

````

на:

````tsx
  onRestart: (r: Restart) => boolean;
  navigate: (r: Route) => void;
  /** Страница из ссылки (`?page=`): открыть её, если она есть в файле. */
  page?: string;
}

````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        let opening = true;
        let recovered = false;
        ed.core.on((e) => {
          if (e.type === 'uiCommand') uiCommand(e.key);
          if (e.type === 'recovered') recovered = true;
          if (opening && e.type === 'opened') {
````

на:

````tsx
        let opening = true;
        let recovered = false;
        // Страница из ссылки — по первому списку страниц после открытия; её нет в
        // файле (удалили) — файл остаётся на первой странице, без ошибки.
        let linkedPage = props.page;
        ed.core.on((e) => {
          if (e.type === 'uiCommand') uiCommand(e.key);
          if (!opening && linkedPage && e.type === 'pages') {
            const want = linkedPage;
            linkedPage = undefined;
            if (want !== e.current && e.pages.some((p) => p.id === want)) ed.send({ type: 'setPage', id: want });
          }
          if (e.type === 'recovered') recovered = true;
          if (opening && e.type === 'opened') {
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
                onBack={() => props.navigate({ kind: 'files', section: 'recent' })}
              />
              <PagesPanel open={pagesOpen()} onToggle={setPagesOpen} />
              <LayersPanel />
              <ResizeEdge side="left" width={left()} onWidth={setLeft} />
````

на:

````tsx
                onBack={() => props.navigate({ kind: 'files', section: 'recent' })}
              />
              <PagesPanel open={pagesOpen()} onToggle={setPagesOpen} fileId={id} />
              <LayersPanel />
              <ResizeEdge side="left" width={left()} onWidth={setLeft} />
````

В `apps/web/src/ui/layers.tsx` заменить:

````tsx
// диапазон для Shift считает worker. Переименование на месте, перетаскивание для
// порядка и вложенности, кнопки «скрыть» и «заблокировать», наведение
// подсвечивает узел на холсте, выделение синхронно в обе стороны.

import { createEffect, createMemo, createSignal, Index, on, onCleanup, onMount, Show, untrack } from 'solid-js';
````

на:

````tsx
// диапазон для Shift считает worker. Переименование на месте, перетаскивание для
// порядка и вложенности, кнопки «скрыть» и «заблокировать», наведение
// подсвечивает узел на холсте, выделение синхронно в обе стороны. Контекстное
// меню строки — из ядра: команды выделения, «Переименовать», «Развернуть всё» и
// «Свернуть всё».

import { createEffect, createMemo, createSignal, Index, on, onCleanup, onMount, Show, untrack } from 'solid-js';
````

В `apps/web/src/ui/layers.tsx` заменить:

````tsx
                onFocus={() => setFocusedSig(row().id)}
                onDblClick={() => startRename(row())}
                onKeyDown={(e) => onKey(row(), index(), e)}
              >
````

на:

````tsx
                onFocus={() => setFocusedSig(row().id)}
                onDblClick={() => startRename(row())}
                onContextMenu={(e) => {
                  // Меню строки — из ядра: невыделенную строку ядро выделяет.
                  e.preventDefault();
                  const r = row();
                  ed.contextMenu?.({ kind: 'layer', id: r.id }, { x: e.clientX, y: e.clientY }, 'Действия со слоем', (a) => {
                    if (a === 'renameLayer') startRename(r);
                    else if (a === 'expandAll' || a === 'collapseAll') ed.send({ type: 'expandAllLayers', expanded: a === 'expandAll' });
                  });
                }}
                onKeyDown={(e) => onKey(row(), index(), e)}
              >
````

В `apps/web/src/ui/pages.tsx` заменить:

````tsx
// Список страниц (сворачиваемый): добавить, переименовать на месте, удалить,
// переключить. Операции со страницами в историю отмены не попадают, поэтому
// удаление — только после подтверждения.

import { createSignal, Index, Show, untrack } from 'solid-js';
````

на:

````tsx
// Список страниц (сворачиваемый): добавить, переименовать на месте, удалить,
// переключить; контекстное меню строки (из ядра) — переименовать, дублировать,
// удалить, ссылка на страницу. Операции со страницами в историю отмены не
// попадают, поэтому удаление — только после подтверждения.

import { createSignal, Index, Show, untrack } from 'solid-js';
````

В `apps/web/src/ui/pages.tsx` заменить:

````tsx
import { Icon } from './icons';
import { RenameInput } from './rename-input';

export function PagesPanel(props: { open: boolean; onToggle: (open: boolean) => void }) {
  const ed = useEditor();
  const pages = () => ed.store.state.pages;
````

на:

````tsx
import { Icon } from './icons';
import { RenameInput } from './rename-input';
import { routeUrl } from './router';

/** Ссылка на страницу файла: открывает файл на этой странице. */
export function pageLink(fileId: string, pageId: string): string {
  return `${location.origin}${routeUrl({ kind: 'file', id: fileId, page: pageId })}`;
}

export function PagesPanel(props: { open: boolean; onToggle: (open: boolean) => void; fileId: string }) {
  const ed = useEditor();
  const pages = () => ed.store.state.pages;
````

В `apps/web/src/ui/pages.tsx` заменить:

````tsx
  const rename = (id: string, original: string, name: string) => {
    if (name.trim() && name !== original) ed.send({ type: 'renamePage', id, name });
  };
  return (
````

на:

````tsx
  const rename = (id: string, original: string, name: string) => {
    if (name.trim() && name !== original) ed.send({ type: 'renamePage', id, name });
  };
  const copyLink = (id: string) => {
    navigator.clipboard.writeText(pageLink(props.fileId, id)).then(
      () => ed.store.toast('Ссылка на страницу скопирована'),
      () => ed.store.toast('Не удалось записать ссылку в буфер обмена: браузер не дал доступа', 'error'),
    );
  };
  /** Контекстное меню строки: пункты и их доступность — из ядра. */
  const openMenu = (p: PageInfo, e: MouseEvent) => {
    e.preventDefault();
    ed.contextMenu?.({ kind: 'page', id: p.id }, { x: e.clientX, y: e.clientY }, 'Действия со страницей', (a) => {
      if (a === 'renamePage') setRenaming(p.id);
      else if (a === 'duplicatePage') ed.send({ type: 'duplicatePage', id: p.id });
      else if (a === 'deletePage') askDelete(p);
      else if (a === 'copyPageLink') copyLink(p.id);
    });
  };
  return (
````

В `apps/web/src/ui/pages.tsx` заменить:

````tsx
                  onClick={() => !current() && ed.send({ type: 'setPage', id: page().id })}
                  onDblClick={() => setRenaming(page().id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') ed.send({ type: 'setPage', id: page().id });
````

на:

````tsx
                  onClick={() => !current() && ed.send({ type: 'setPage', id: page().id })}
                  onDblClick={() => setRenaming(page().id)}
                  onContextMenu={(e) => openMenu(page(), e)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') ed.send({ type: 'setPage', id: page().id });
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
  /** `restart` — редактор запущен заново после сбоя; `backend` — бэкенд рендера
   * (после потери GPU — запасной, и для следующих перезапусков тоже). */
  | { kind: 'editor'; file: FileMeta; restart?: Restart; backend?: Backend }
  /** Файл на диске без доступа (браузер перезапущен): вопрос, ответ — по щелчку. */
  | { kind: 'access'; file: FileMeta; answer: (ok: boolean) => void };
````

на:

````tsx
  /** `restart` — редактор запущен заново после сбоя; `backend` — бэкенд рендера
   * (после потери GPU — запасной, и для следующих перезапусков тоже). */
  | { kind: 'editor'; file: FileMeta; restart?: Restart; backend?: Backend; page?: string }
  /** Файл на диске без доступа (браузер перезапущен): вопрос, ответ — по щелчку. */
  | { kind: 'access'; file: FileMeta; answer: (ok: boolean) => void };
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx

  /** Открыть файл в редакторе: занят другой вкладкой — к экрану файлов. */
  async function edit(file: FileMeta, my: number): Promise<void> {
    // Файл на диске: доступ к нему после перезапуска браузера снова разрешает
    // пользователь — щелчком в вопросе (запросу нужно действие пользователя).
````

на:

````tsx

  /** Открыть файл в редакторе: занят другой вкладкой — к экрану файлов. */
  async function edit(file: FileMeta, my: number, page?: string): Promise<void> {
    // Файл на диске: доступ к нему после перезапуска браузера снова разрешает
    // пользователь — щелчком в вопросе (запросу нужно действие пользователя).
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
    }
    unlock = lock.release;
    current = { kind: 'file', id: file.id };
    replace(current);
    setView({ kind: 'editor', file });
  }

````

на:

````tsx
    }
    unlock = lock.release;
    // Страница из ссылки открывается один раз: в адресе остаётся только файл.
    current = { kind: 'file', id: file.id };
    replace(current);
    setView({ kind: 'editor', file, page });
  }

````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
        // Файл из корзины для правки не открывается: сначала восстановить.
        else if (file.trashedAt !== undefined) toFiles('Файл в корзине — восстановите его, чтобы открыть');
        else await edit(file, my);
      }
    } catch (e) {
````

на:

````tsx
        // Файл из корзины для правки не открывается: сначала восстановить.
        else if (file.trashedAt !== undefined) toFiles('Файл в корзине — восстановите его, чтобы открыть');
        else await edit(file, my, route.page);
      }
    } catch (e) {
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
            restart={v.restart}
            backend={v.backend}
            onRestart={(r) => restart(v.file, r, v.backend)}
            navigate={navigate}
````

на:

````tsx
            restart={v.restart}
            backend={v.backend}
            page={v.page}
            onRestart={(r) => restart(v.file, r, v.backend)}
            navigate={navigate}
````

В `apps/web/src/ui/router.ts` заменить:

````ts
// Маршруты приложения в адресе после `#` (назад и вперёд браузера работают):
// - `#/` (и пустой) — экран файлов, «Недавние»; `#/drafts`, `#/trash` — другие разделы;
// - `#/file/<id>` — редактор с файлом из хранилища;
// - `#/new` — создать файл в хранилище и открыть его (адрес заменяется на `#/file/<id>`).
// Параметры поиска (`?nodes=`, `?backend=` — тесты) маршрутизатор не трогает.
````

на:

````ts
// Маршруты приложения в адресе после `#` (назад и вперёд браузера работают):
// - `#/` (и пустой) — экран файлов, «Недавние»; `#/drafts`, `#/trash` — другие разделы;
// - `#/file/<id>` — редактор с файлом из хранилища; `#/file/<id>?page=<pageId>` —
//   с этой страницей («Копировать ссылку на страницу»);
// - `#/new` — создать файл в хранилище и открыть его (адрес заменяется на `#/file/<id>`).
// Параметры поиска (`?nodes=`, `?backend=` — тесты) маршрутизатор не трогает.
````

В `apps/web/src/ui/router.ts` заменить:

````ts
import { MAX_ID_BYTES } from '../core/file-list';

export type Route = { kind: 'files'; section: Section } | { kind: 'file'; id: string } | { kind: 'new' };

const SECTION_PATH: Record<Section, string> = { recent: '', drafts: 'drafts', trash: 'trash' };
````

на:

````ts
import { MAX_ID_BYTES } from '../core/file-list';

export type Route = { kind: 'files'; section: Section } | { kind: 'file'; id: string; page?: string } | { kind: 'new' };

const SECTION_PATH: Record<Section, string> = { recent: '', drafts: 'drafts', trash: 'trash' };
````

В `apps/web/src/ui/router.ts` заменить:

````ts
}

/** Маршрут по части адреса после `#` (с решёткой или без); неизвестное — экран файлов. */
export function parseRoute(hash: string): Route {
  const path = hash.replace(/^#/, '').replace(/^\/+/, '').replace(/\/+$/, '');
  if (path === 'new') return { kind: 'new' };
  if (path === 'drafts') return { kind: 'files', section: 'drafts' };
````

на:

````ts
}

/** Идентификатор страницы из адреса — не доверенный: `счётчик@узел` Loro, не длиннее 64. */
function validPage(id: string): boolean {
  return id.length <= 64 && /^\d+@\d+$/.test(id);
}

/** Маршрут по части адреса после `#` (с решёткой или без); неизвестное — экран файлов. */
export function parseRoute(hash: string): Route {
  const [rawPath, query = ''] = hash.replace(/^#/, '').split('?', 2);
  const path = rawPath.replace(/^\/+/, '').replace(/\/+$/, '');
  if (path === 'new') return { kind: 'new' };
  if (path === 'drafts') return { kind: 'files', section: 'drafts' };
````

В `apps/web/src/ui/router.ts` заменить:

````ts
    }
    // Неверный id — тот же путь, что и неизвестный: редактор сообщит «Файл не найден».
    return { kind: 'file', id: validId(id) ? id : '' };
  }
  return { kind: 'files', section: 'recent' };
````

на:

````ts
    }
    // Неверный id — тот же путь, что и неизвестный: редактор сообщит «Файл не найден».
    const route: Route = { kind: 'file', id: validId(id) ? id : '' };
    // Неверная страница отбрасывается: файл откроется на первой.
    const page = new URLSearchParams(query).get('page');
    if (page !== null && validPage(page)) route.page = page;
    return route;
  }
  return { kind: 'files', section: 'recent' };
````

В `apps/web/src/ui/router.ts` заменить:

````ts
      return '#/new';
    case 'file':
      return `#/file/${encodeURIComponent(r.id)}`;
    case 'files':
      return `#/${SECTION_PATH[r.section]}`;
````

на:

````ts
      return '#/new';
    case 'file':
      return `#/file/${encodeURIComponent(r.id)}${r.page ? `?page=${encodeURIComponent(r.page)}` : ''}`;
    case 'files':
      return `#/${SECTION_PATH[r.section]}`;
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/context-menu.spec.ts e2e/files.spec.ts --project=chromium`

Expected: модульные — `router.test.ts` (3) зелёные, сборка — ок; E2E `context-menu` (5 тестов) и `files` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/context-menu.spec.ts apps/web/src/ui/app.tsx apps/web/src/ui/layers.tsx apps/web/src/ui/pages.tsx apps/web/src/ui/root.tsx apps/web/src/ui/router.ts apps/web/unit/router.test.ts
git commit -F - <<'EOF'
feat(web): меню слоёв и страниц из ядра, ссылка на страницу `#/file/<id>?page=<pageId>`

Правый щелчок по строке слоя — ContextMenu { target: layer } (невыделенная
строка выделяется, как из панели): команды выделения, «Переименовать»
(поле имени в строке), «Развернуть всё» и «Свернуть всё» (ExpandAllLayers).
По странице — «Переименовать», «Дублировать страницу» (DuplicatePage),
«Удалить страницу» (с подтверждением; единственную — серым) и «Копировать
ссылку на страницу». Маршрут разбирает `?page=` (недоверенный: счётчик@узел,
до 64 знаков), редактор открывает эту страницу по первому списку страниц и
оставляет в адресе только файл.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4: «Вставить поверх» и «Вставить с заменой», повтор вставки со сдвигом +10, +10

Спека §6: команды `edit.paste_over` (Ctrl/Cmd+Shift+V) и `edit.paste_replace` (Ctrl/Cmd+Shift+R — в списке сочетаний браузера); `PasteRequested` и `Paste` несут способ вставки. Поверх — на исходные координаты; с заменой — содержимое буфера на место каждого выделенного узла одним шагом; повторная вставка того же содержимого, пока исходное видно, — +10, +10 от прошлой. Ctrl/Cmd+Shift+V на холсте — событие `paste` с режимом «поверх», в правке текста — обычный текст. Пункты — в «Правке» главного меню и в меню холста; команд 80 → 82. Сюда же входят тесты протокола `ContextMenu` задачи 1 (round-trip, имена полей).

**Files:**
- Modify: `crates/ferrum-editor/src/{actions,commands,command_state,editor,keymap,menu,context_menu,lib}.rs`, `crates/ferrum-wasm/src/{protocol,core}.rs`, `apps/web/src/{core/core.ts, core/messages.ts, worker/core.worker.ts, ui/app.tsx, ui/shortcuts.ts, ui/system-clipboard.ts}`
- Test: `crates/ferrum-editor/tests/paste_modes.rs` (новый), `crates/ferrum-editor/tests/{commands,context_menu,keymap,main_menu}.rs`, `crates/ferrum-wasm/tests/paste.rs` (новый), `crates/ferrum-wasm/tests/{protocol,core,text_edit,boolean}.rs`, `apps/web/unit/shortcuts.test.ts`, `apps/web/e2e/paste.spec.ts` (новый), `apps/web/e2e/{app,chrome,clipboard,context-menu}.spec.ts`

**Interfaces:**
- Consumes: `Editor::paste`, буфер `clipboard.rs` (`MAX_PAYLOAD`, разбор), правило «вставка на место», `browser_reserved`, `clipboardAction`, `CoreEvent::PasteRequested`.
- Produces: `CommandId::{PasteOver, PasteReplace}`; `pub enum PasteMode { Normal, Over, Replace }` (вариант `Props` — задача 5), `pub const PASTE_STEP: f64 = 10.0`; `EditorEvent::PasteRequested(PasteMode)`; `Editor::paste_with(&mut self, bytes: &[u8], mode: PasteMode) -> Result<(), ClipboardError>`; `Editor.last_paste: Option<([u8; 32], Vec2)>`; `KeyR`, `Shift+KeyR` в `browser_reserved`. Протокол: `PasteModeDto`, `Command::Paste { bytes, mode? }`, `CoreEvent::PasteRequested { mode }`; `pasteText` несёт `mode`; `clipboardAction(...)` → `'pasteOver'`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 80, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 82, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  await page.keyboard.press('Enter');
  await expect(menu).toBeVisible();
  // «В» — по кругу после серого пункта: «Войти в правку», «Вставить», «Выделить слой».
  await letter('в');
  await expect(menu.getByRole('menuitem', { name: /^Войти в правку/ })).toBeFocused();
  await letter('в');
  await expect(menu.getByRole('menuitem', { name: /^Вставить/ })).toBeFocused();
  await letter('В');
  await expect(menu.getByRole('menuitem', { name: /^Выделить слой/ })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
````

на:

````ts
  await page.keyboard.press('Enter');
  await expect(menu).toBeVisible();
  // «В» — по кругу после серого пункта: «Войти в правку», затем пункты вставки
  // и «Выделить слой».
  const item = (label: string) => menu.locator(`[data-label="${label}"]`);
  for (const label of ['Войти в правку', 'Вставить', 'Вставить поверх', 'Вставить с заменой']) {
    await letter('в');
    await expect(item(label)).toBeFocused();
  }
  await letter('В');
  await expect(item('Выделить слой')).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
  const at = await appScreen(page, await live(page), 200, 40);
  await page.mouse.click(at.x, at.y, { button: 'right' });
  await page.getByRole('menu').getByRole('menuitem', { name: /Копировать как SVG/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Скопировано как SVG' })).toBeVisible();
  const got = await page.evaluate(async () => {
````

на:

````ts
  const at = await appScreen(page, await live(page), 200, 40);
  await page.mouse.click(at.x, at.y, { button: 'right' });
  // Как в Figma: «Копировать как ▸ SVG» — подменю контекстного меню.
  await page.getByRole('menu', { name: 'Действия с выделением' }).getByRole('menuitem', { name: 'Копировать как', exact: true }).click();
  await page.getByRole('menu', { name: 'Копировать как' }).getByRole('menuitem', { name: /Копировать как SVG/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Скопировано как SVG' })).toBeVisible();
  const got = await page.evaluate(async () => {
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
    const at = await appScreen(page, await live(page), 0, 0);
    await page.mouse.click(at.x, at.y, { button: 'right' });
    await page.getByRole('menuitem', { name: /Вставить/ }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Нет доступа к буферу обмена' })).toHaveCount(1);
  });
````

на:

````ts
    const at = await appScreen(page, await live(page), 0, 0);
    await page.mouse.click(at.x, at.y, { button: 'right' });
    await page.getByRole('menuitem', { name: /^Вставить/ }).first().click();
    await expect(page.getByRole('status').filter({ hasText: 'Нет доступа к буферу обмена' })).toHaveCount(1);
  });
````

В `apps/web/e2e/context-menu.spec.ts` заменить:

````ts
    'Копировать',
    'Вставить',
    'Копировать как',
    'Выделить слой',
````

на:

````ts
    'Копировать',
    'Вставить',
    'Вставить поверх',
    'Вставить с заменой',
    'Копировать как',
    'Выделить слой',
````

В `apps/web/e2e/context-menu.spec.ts` заменить:

````ts
});

test('пустое место холста: выделение снимается, меню — вставить, выделить всё, показать всё', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
````

на:

````ts
});

test('пустое место холста: выделение снимается, меню — вставить, вставить поверх, выделить всё, показать всё', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
````

В `apps/web/e2e/context-menu.spec.ts` заменить:

````ts
  expect(Math.abs(box.x - empty.x)).toBeLessThanOrEqual(1);
  expect(Math.min(Math.abs(box.y - (empty.y + 4)), Math.abs(box.y + box.height - (empty.y - 4)))).toBeLessThanOrEqual(2);
  expect(await labels(page, 'Действия с выделением')).toEqual(['Вставить', 'Выделить всё', 'Показать всё']);
  await menu.getByRole('menuitem', { name: /^Выделить всё/ }).click();
  await liveWhen(page, 'l.selectionCount === 10');
````

на:

````ts
  expect(Math.abs(box.x - empty.x)).toBeLessThanOrEqual(1);
  expect(Math.min(Math.abs(box.y - (empty.y + 4)), Math.abs(box.y + box.height - (empty.y - 4)))).toBeLessThanOrEqual(2);
  expect(await labels(page, 'Действия с выделением')).toEqual(['Вставить', 'Вставить поверх', 'Выделить всё', 'Показать всё']);
  await menu.getByRole('menuitem', { name: /^Выделить всё/ }).click();
  await liveWhen(page, 'l.selectionCount === 10');
````

Создать `apps/web/e2e/paste.spec.ts`:

````ts
// Вставка (спека подсистемы 2, раздел 6): повтор вставки того же содержимого —
// +10, +10; Ctrl/Cmd+Shift+V — «Вставить поверх» на исходные координаты;
// «Вставить с заменой» из меню; предупреждение о сочетании, занятом браузером.
// Первый тест не читает системный буфер (события `paste` — как от браузера) и
// идёт и в Firefox; чтение буфера (`clipboard-read`) — только в Chromium.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

/** Щелчок по узлу в точке мира. */
async function pick(page: Page, x: number, y: number) {
  const at = await appScreen(page, await live(page), x, y);
  await page.mouse.click(at.x, at.y);
}

/** Копия выделения текстом с меткой (как запасной путь кладёт её в буфер). Байты
 * события `clipboard` интерфейс может сразу передать worker'у (запасной путь
 * записи — Firefox): тогда текст берётся из его же `encodeClipboard`. */
async function copiedText(page: Page): Promise<string> {
  await page.evaluate(() => {
    const w = window as { __clip?: Uint8Array; __text?: string };
    const core = window.__ferrum!.core!;
    w.__clip = undefined;
    w.__text = undefined;
    const encode = core.encodeClipboard.bind(core);
    core.encodeClipboard = async (b) => {
      const t = await encode(b);
      if (t) w.__text = t;
      return t;
    };
    core.on((e) => {
      if (e.type === 'clipboard' && e.bytes.byteLength > 0) w.__clip = e.bytes.slice();
    });
  });
  // Команда ядра, а не Ctrl+C: Firefox без фокуса окна событие `copy` не шлёт.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'execute', command: 'edit.copy' }));
  await page.waitForFunction(() => {
    const w = window as { __clip?: Uint8Array; __text?: string };
    return w.__clip !== undefined || w.__text !== undefined;
  });
  return page.evaluate(async () => {
    const w = window as { __clip?: Uint8Array; __text?: string };
    return w.__text ?? (await window.__ferrum!.core!.encodeClipboard(w.__clip!))!;
  });
}

/** Событие `paste` на холсте с текстом; `over` — перед ним нажато Ctrl/Cmd+Shift+V. */
async function paste(page: Page, text: string, over = false) {
  await page.evaluate(
    ({ text, over }) => {
      const canvas = document.querySelector('canvas')!;
      const mac = navigator.platform.toLowerCase().startsWith('mac');
      canvas.dispatchEvent(
        new KeyboardEvent('keydown', { code: 'KeyV', key: 'V', shiftKey: over, ctrlKey: !mac, metaKey: mac, bubbles: true }),
      );
      const dt = new DataTransfer();
      dt.setData('text/plain', text);
      canvas.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
    },
    { text, over },
  );
}

test('повтор вставки — +10, +10; Ctrl/Cmd+Shift+V — поверх, на исходное место', async ({ page }) => {
  await openApp(page);
  // «Слой 1» в «Экран 0»: мир (140, 0) — (260, 80).
  await pick(page, 200, 40);
  const l = await liveWhen(page, 'l.selectionCount === 1');
  const text = await copiedText(page);
  for (const [i, x] of [140, 150, 160].entries()) {
    await paste(page, text);
    await liveWhen(page, `l.nodes === ${l.nodes + i + 1} && l.selectionX === ${x} && l.selectionY === ${x - 140}`);
  }
  await paste(page, text, true);
  const after = await liveWhen(page, `l.nodes === ${l.nodes + 4} && l.selectionX === 140 && l.selectionY === 0`);
  expect(after.steps).toBe(l.steps + 4);
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

test('«Вставить с заменой» и «Вставить поверх» из меню читают буфер; Ctrl+Shift+R — с предупреждением', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'чтение буфера из меню — разрешение clipboard-read есть только в Chromium');
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await openApp(page);
  // «Слой 0» — эллипс в мире (0, 0) — (120, 80).
  await pick(page, 60, 40);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.keyboard.press('ControlOrMeta+c');
  await expect.poll(() => page.evaluate(async () => (await navigator.clipboard.read())[0]?.types ?? [])).toContain('web application/x-ferrum');
  // Заменить «Слой 1» (прямоугольник в (140, 0)): эллипс на его месте, одним шагом.
  await pick(page, 200, 40);
  const l = await liveWhen(page, 'l.selectionCount === 1 && l.selectionX === 140');
  await mainMenu(page, 'Правка', 'Вставить с заменой');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.summary?.kind)).toBe('ellipse');
  const r = await liveWhen(page, `l.steps === ${l.steps + 1}`);
  expect([r.nodes, r.selectionX, r.selectionY, r.selectionCount]).toEqual([l.nodes, 140, 0, 1]);
  // Поверх из контекстного меню: на исходное место эллипса.
  const at = await appScreen(page, r, 200, 40);
  await page.mouse.click(at.x, at.y, { button: 'right' });
  await page.getByRole('menu', { name: 'Действия с выделением' }).getByRole('menuitem', { name: /^Вставить поверх/ }).click();
  await liveWhen(page, `l.nodes === ${l.nodes + 1} && l.selectionX === 0 && l.selectionY === 0`);
  // Сочетание «Вставить с заменой» занято браузером: настройки предупреждают.
  await mainMenu(page, 'Настройки', 'Настройки…');
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('с заменой');
  await expect(dialog.getByRole('row').filter({ hasText: 'Вставить с заменой' })).toContainText('Перехватывает браузер');
});
````

В `apps/web/unit/shortcuts.test.ts` заменить:

````ts
  expect(clipboardAction('Ctrl+KeyV', 'linux')).toBe('paste');
  expect(clipboardAction('Ctrl+Shift+KeyC', 'linux')).toBeNull();
});
````

на:

````ts
  expect(clipboardAction('Ctrl+KeyV', 'linux')).toBe('paste');
  expect(clipboardAction('Ctrl+Shift+KeyC', 'linux')).toBeNull();
  // Ctrl/Cmd+Shift+V — тоже событие `paste`: «Вставить поверх».
  expect(clipboardAction('Ctrl+Shift+KeyV', 'windows')).toBe('pasteOver');
  expect(clipboardAction('Shift+Meta+KeyV', 'mac')).toBe('pasteOver');
  expect(clipboardAction('Ctrl+Shift+KeyV', 'mac')).toBeNull();
});
````

В `crates/ferrum-editor/tests/commands.rs` заменить:

````rust
    assert!(events.contains(&EditorEvent::SaveRequested));
    assert!(events.contains(&EditorEvent::ExportPng(vec![b])));
    assert!(events.contains(&EditorEvent::PasteRequested));
    // «Копировать как SVG» — только командой (сочетания нет, как в Figma).
    h.ed.execute(CommandId::CopyAsSvg);
````

на:

````rust
    assert!(events.contains(&EditorEvent::SaveRequested));
    assert!(events.contains(&EditorEvent::ExportPng(vec![b])));
    assert!(events.contains(&EditorEvent::PasteRequested(PasteMode::Normal)));
    // «Копировать как SVG» — только командой (сочетания нет, как в Figma).
    h.ed.execute(CommandId::CopyAsSvg);
````

В `crates/ferrum-editor/tests/context_menu.rs` заменить:

````rust
    assert_eq!(
        keys(&empty),
        ["edit.paste", "edit.select_all", "view.fit_all"]
    );
    check_level(&empty);
````

на:

````rust
    assert_eq!(
        keys(&empty),
        [
            "edit.paste",
            "edit.paste_over",
            "edit.select_all",
            "view.fit_all"
        ]
    );
    check_level(&empty);
````

В `crates/ferrum-editor/tests/context_menu.rs` заменить:

````rust
            "edit.copy",
            "edit.paste",
            "arrange.forward",
            "arrange.backward",
````

на:

````rust
            "edit.copy",
            "edit.paste",
            "edit.paste_over",
            "edit.paste_replace",
            "arrange.forward",
            "arrange.backward",
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    (CommandId::Copy, &["Mod+KeyC"]),
    (CommandId::Paste, &["Mod+KeyV"]),
    (CommandId::Cut, &["Mod+KeyX"]),
    (CommandId::CopyAsPng, &["Mod+Shift+KeyC"]),
````

на:

````rust
    (CommandId::Copy, &["Mod+KeyC"]),
    (CommandId::Paste, &["Mod+KeyV"]),
    (CommandId::PasteOver, &["Mod+Shift+KeyV"]),
    (CommandId::PasteReplace, &["Mod+Shift+KeyR"]),
    (CommandId::Cut, &["Mod+KeyX"]),
    (CommandId::CopyAsPng, &["Mod+Shift+KeyC"]),
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
        }
        assert!(map.conflicts().is_empty(), "{:?}", map.conflicts());
        assert!(
            map.browser_conflicts().is_empty(),
            "умолчания не задевают сочетаний браузера"
        );
    }
````

на:

````rust
        }
        assert!(map.conflicts().is_empty(), "{:?}", map.conflicts());
        // Умолчания не задевают сочетаний браузера, кроме «Вставить с заменой»:
        // Ctrl/Cmd+Shift+R — как в Figma, настройки предупреждают о перезагрузке.
        assert_eq!(
            map.browser_conflicts(),
            [(
                CommandId::PasteReplace,
                sc(&format!("{primary}+Shift+KeyR"))
            )]
        );
    }
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 80);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 82);
}

````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    assert!(reserved.contains(&sc("Ctrl+KeyT")));
    assert!(reserved.contains(&sc("Ctrl+KeyN")));
    assert!(browser_reserved(Platform::Mac).contains(&sc("Meta+KeyW")));
    let mut map = Keymap::new(Platform::Windows);
````

на:

````rust
    assert!(reserved.contains(&sc("Ctrl+KeyT")));
    assert!(reserved.contains(&sc("Ctrl+KeyN")));
    assert!(
        reserved.contains(&sc("Ctrl+Shift+KeyR")),
        "перезагрузка без кэша"
    );
    assert!(browser_reserved(Platform::Mac).contains(&sc("Meta+Shift+KeyR")));
    assert!(browser_reserved(Platform::Mac).contains(&sc("Meta+KeyW")));
    let mut map = Keymap::new(Platform::Windows);
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    assert_eq!(
        map.browser_conflicts(),
        vec![(CommandId::Duplicate, sc("Ctrl+KeyW"))]
    );
}
````

на:

````rust
    assert_eq!(
        map.browser_conflicts(),
        vec![
            (CommandId::PasteReplace, sc("Ctrl+Shift+KeyR")),
            (CommandId::Duplicate, sc("Ctrl+KeyW"))
        ]
    );
}
````

В `crates/ferrum-editor/tests/main_menu.rs` заменить:

````rust
        "edit.undo",
        "edit.redo",
        "edit.duplicate",
        "edit.select_all",
````

на:

````rust
        "edit.undo",
        "edit.redo",
        "edit.paste_over",
        "edit.paste_replace",
        "edit.duplicate",
        "edit.select_all",
````

Создать `crates/ferrum-editor/tests/paste_modes.rs`:

````rust
//! Способы вставки (спека подсистемы 2, раздел 6): повтор вставки со сдвигом
//! +10, +10, «Вставить поверх» на исходные координаты, «Вставить с заменой» на
//! место каждого выделенного узла одним шагом.

mod common;

use common::*;
use ferrum_doc::{NodeKind, PropsPatch};
use ferrum_editor::*;
use kurbo::Point;

fn copy_of(h: &mut H, ids: &[ferrum_doc::NodeId]) -> Vec<u8> {
    h.ed.select(ids);
    h.ed.take_events();
    h.ed.execute(CommandId::Copy);
    h.ed.take_events()
        .into_iter()
        .find_map(|e| match e {
            EditorEvent::Clipboard(b) => Some(b),
            _ => None,
        })
        .expect("копия в событиях")
}

fn xy(h: &mut H, id: ferrum_doc::NodeId) -> (f64, f64) {
    let p = h.props(id);
    (p.x, p.y)
}

fn away(h: &mut H) {
    let mut cam = ferrum_render::Camera::new(VIEW, 1.0);
    cam.set_center(Point::new(5000.0, 5000.0));
    h.ed.set_camera(cam);
}

#[test]
fn repeated_paste_of_the_same_content_steps_by_ten() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    let bytes = copy_of(&mut h, &[r]);
    let mut at = Vec::new();
    for _ in 0..3 {
        h.ed.paste(&bytes).unwrap();
        let id = h.sel()[0];
        at.push(xy(&mut h, id));
    }
    assert_eq!(at, [(100.0, 100.0), (110.0, 110.0), (120.0, 120.0)]);
    // Другое содержимое — снова на место.
    let other = h.rect(None, 300.0, 100.0, 40.0, 40.0);
    let b2 = copy_of(&mut h, &[other]);
    h.ed.paste(&b2).unwrap();
    let id = h.sel()[0];
    assert_eq!(xy(&mut h, id), (300.0, 100.0));
    // Новая копия того же — счёт заново.
    let b1 = copy_of(&mut h, &[r]);
    assert_eq!(b1, bytes);
    h.ed.paste(&b1).unwrap();
    let id = h.sel()[0];
    assert_eq!(xy(&mut h, id), (100.0, 100.0));
    // Исходное не видно — в центр вида, как раньше.
    away(&mut h);
    h.ed.paste(&b1).unwrap();
    let id = h.sel()[0];
    assert_eq!(xy(&mut h, id), (4980.0, 4980.0));
}

#[test]
fn paste_over_keeps_original_coordinates() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    let bytes = copy_of(&mut h, &[r]);
    away(&mut h);
    for _ in 0..2 {
        h.ed.paste_with(&bytes, PasteMode::Over).unwrap();
        let id = h.sel()[0];
        assert_eq!(xy(&mut h, id), (100.0, 100.0), "без сдвига к центру и шага");
    }
}

#[test]
fn paste_replace_takes_place_parent_and_order_in_one_step() {
    let mut h = H::new();
    let src = h.add(None, NodeKind::Ellipse, patch(700.0, 700.0, 30.0, 20.0));
    let bytes = copy_of(&mut h, &[src]);
    let a = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    let above = h.rect(None, 500.0, 500.0, 10.0, 10.0);
    let f = h.frame(None, 200.0, 200.0, 300.0, 300.0);
    let low = h.rect(Some(f), 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(Some(f), 50.0, 60.0, 40.0, 40.0);
    let high = h.rect(Some(f), 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[a, b]);
    let steps = h.ed.step_count();
    let before = h.state();
    h.ed.paste_with(&bytes, PasteMode::Replace).unwrap();
    let new = h.sel();
    assert_eq!(new.len(), 2);
    // Узлы заменены: их больше нет, на их местах — эллипсы.
    let roots = h.children(None);
    assert!(!roots.contains(&a) && !h.children(Some(f)).contains(&b));
    let at_a = roots[1];
    assert_eq!(roots, [src, at_a, above, f], "место в слоях корня");
    assert_eq!(
        h.children(Some(f)),
        [low, new_in(&new, &h.children(Some(f))), high]
    );
    let p = h.props(at_a);
    assert_eq!(
        (p.kind, p.x, p.y, p.w, p.h),
        (NodeKind::Ellipse, 100.0, 100.0, 30.0, 20.0)
    );
    let in_f = new_in(&new, &h.children(Some(f)));
    let p = h.props(in_f);
    assert_eq!((p.x, p.y), (50.0, 60.0), "координаты родителя-фрейма");
    h.assert_one_step(steps, &before);
}

/// Вставленный узел, который лежит среди `children`.
fn new_in(new: &[ferrum_doc::NodeId], children: &[ferrum_doc::NodeId]) -> ferrum_doc::NodeId {
    *new.iter()
        .find(|n| children.contains(n))
        .expect("вставка в этом родителе")
}

#[test]
fn paste_replace_skips_locked_and_without_selection_is_a_paste() {
    let mut h = H::new();
    let src = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    let bytes = copy_of(&mut h, &[src]);
    let locked = h.rect(None, 300.0, 300.0, 40.0, 40.0);
    h.set(
        locked,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    let free = h.rect(None, 500.0, 300.0, 40.0, 40.0);
    h.ed.select(&[locked, free]);
    h.ed.paste_with(&bytes, PasteMode::Replace).unwrap();
    let roots = h.children(None);
    assert!(roots.contains(&locked) && !roots.contains(&free));
    assert_eq!(h.sel().len(), 1);
    // Выделение сняли, пока читался буфер: обычная вставка.
    h.ed.select(&[]);
    let n = h.children(None).len();
    h.ed.paste_with(&bytes, PasteMode::Replace).unwrap();
    assert_eq!(h.children(None).len(), n + 1);
}

#[test]
fn paste_commands_ask_the_ui_with_their_mode_and_follow_state() {
    let mut h = H::new();
    assert!(h.ed.command_state(CommandId::PasteOver).enabled);
    assert!(
        !h.ed.command_state(CommandId::PasteReplace).enabled,
        "заменять нечего"
    );
    h.ed.execute(CommandId::PasteReplace);
    assert!(!h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::PasteRequested(_))));
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    h.ed.take_events();
    h.ed.execute(CommandId::PasteOver);
    h.ed.execute(CommandId::PasteReplace);
    let asked: Vec<PasteMode> =
        h.ed.take_events()
            .into_iter()
            .filter_map(|e| match e {
                EditorEvent::PasteRequested(m) => Some(m),
                _ => None,
            })
            .collect();
    assert_eq!(asked, [PasteMode::Over, PasteMode::Replace]);
    h.key("KeyV", ctrl().with_shift());
    assert!(h
        .ed
        .take_events()
        .contains(&EditorEvent::PasteRequested(PasteMode::Over)));
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    assert!(!h.ed.command_state(CommandId::PasteOver).enabled);
    assert!(!h.ed.command_state(CommandId::PasteReplace).enabled);
}
````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 80);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

на:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 82);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

В `crates/ferrum-wasm/tests/core.rs` заменить:

````rust
    core.command(Command::Paste {
        bytes: ByteBuf::from(vec![1, 2, 3]),
    });
    core.command(Command::PlaceImage {
````

на:

````rust
    core.command(Command::Paste {
        bytes: ByteBuf::from(vec![1, 2, 3]),
        mode: None,
    });
    core.command(Command::PlaceImage {
````

Создать `crates/ferrum-wasm/tests/paste.rs`:

````rust
//! Способы вставки в ядре worker'а (спека подсистемы 2, раздел 6): команды
//! «Вставить поверх» и «Вставить с заменой» просят буфер событием
//! `PasteRequested { mode }`, `Paste { mode }` вставляет этим способом.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;

const VIEW: Size = Size::new(800.0, 600.0);

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

fn execute(core: &mut Core, key: &str) -> Vec<CoreEvent> {
    core.command(Command::Execute {
        command: key.into(),
    });
    core.take_events()
}

#[test]
fn paste_commands_ask_with_mode_and_replace_swaps_the_selection() {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    core.command(Command::Demo { nodes: 100 });
    // «Слой 1» в «Экран 0»: мир (200, 40), экран (140, 64).
    click(&mut core, 140.0, 64.0, 1000.0);
    let ev = execute(&mut core, "edit.copy");
    let bytes = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::Clipboard { bytes, .. } => Some(bytes.clone()),
            _ => None,
        })
        .expect("копия");
    for (key, mode) in [
        ("edit.paste", PasteModeDto::Normal),
        ("edit.paste_over", PasteModeDto::Over),
        ("edit.paste_replace", PasteModeDto::Replace),
    ] {
        let ev = execute(&mut core, key);
        assert!(
            ev.contains(&CoreEvent::PasteRequested { mode }),
            "{key}: {ev:?}"
        );
    }
    // Замена «Слоя 2» (мир (340, 40)) копией «Слоя 1»: узлов столько же.
    click(&mut core, 280.0, 64.0, 2000.0);
    let x = |core: &Core| {
        let ed = core.editor();
        let id = ed.selection()[0];
        ed.scene().node(id).unwrap().world_bounds.unwrap().x0
    };
    let (nodes, before) = (core.live().nodes, x(&core));
    let replaced = core.editor().selection()[0];
    core.command(Command::Paste {
        bytes,
        mode: Some(PasteModeDto::Replace),
    });
    let ed = core.editor();
    assert_eq!(ed.selection().len(), 1);
    assert_ne!(ed.selection()[0], replaced);
    assert!(ed.scene().node(replaced).is_none(), "заменённого нет");
    assert_eq!(core.live().nodes, nodes);
    assert_eq!(x(&core), before, "на место заменённого");
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
            system: Some(true),
        },
        Command::Paste { bytes: bytes(&[1]) },
        Command::PlaceImage {
            bytes: bytes(&[0x89, b'P']),
        },
        Command::Save,
        Command::Snapshot { thumbnail: true },
        Command::MarkSaved { revision: 3.0 },
````

на:

````rust
            system: Some(true),
        },
        Command::Paste {
            bytes: bytes(&[1]),
            mode: None,
        },
        Command::Paste {
            bytes: bytes(&[1]),
            mode: Some(PasteModeDto::Replace),
        },
        Command::PlaceImage {
            bytes: bytes(&[0x89, b'P']),
        },
        Command::Save,
        Command::ContextMenu {
            target: ContextTarget::Canvas { x: 1.5, y: 2.0 },
        },
        Command::ContextMenu {
            target: ContextTarget::Layer { id: "1@2".into() },
        },
        Command::ContextMenu {
            target: ContextTarget::Page { id: "3@4".into() },
        },
        Command::ExpandAllLayers { expanded: true },
        Command::DuplicatePage { id: "3@4".into() },
        Command::Snapshot { thumbnail: true },
        Command::MarkSaved { revision: 3.0 },
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        },
        CoreEvent::ClipboardPng { bytes: bytes(b"p") },
        CoreEvent::PasteRequested,
        CoreEvent::RequestImage,
        CoreEvent::ToggleUi,
````

на:

````rust
        },
        CoreEvent::ClipboardPng { bytes: bytes(b"p") },
        CoreEvent::PasteRequested {
            mode: PasteModeDto::Over,
        },
        CoreEvent::ContextMenu {
            items: vec![
                MenuItemDto::Command {
                    key: "edit.copy".into(),
                },
                MenuItemDto::Submenu {
                    title: "Выделить слой".into(),
                    items: vec![MenuItemDto::Node {
                        id: "1@2".into(),
                        name: "Прямоугольник".into(),
                    }],
                },
                MenuItemDto::Separator,
                MenuItemDto::Action {
                    action: MenuActionDto::DuplicatePage,
                    title: "Дублировать страницу".into(),
                    enabled: true,
                },
            ],
        },
        CoreEvent::RequestImage,
        CoreEvent::ToggleUi,
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        assert_eq!(serde_json::from_value::<CoreEvent>(v).unwrap(), e);
    }
}

````

на:

````rust
        assert_eq!(serde_json::from_value::<CoreEvent>(v).unwrap(), e);
    }
}

#[test]
fn context_menu_and_paste_mode_field_names() {
    assert_eq!(
        serde_json::to_value(Command::ContextMenu {
            target: ContextTarget::Canvas { x: 1.0, y: 2.0 }
        })
        .unwrap(),
        json!({"type": "contextMenu", "target": {"kind": "canvas", "x": 1.0, "y": 2.0}})
    );
    assert_eq!(
        serde_json::to_value(MenuItemDto::Action {
            action: MenuActionDto::CopyPageLink,
            title: "Ссылка".into(),
            enabled: false
        })
        .unwrap(),
        json!({"type": "action", "action": "copyPageLink", "title": "Ссылка", "enabled": false})
    );
    // Без способа — обычная вставка (прежний вид команды).
    let old: Command = serde_json::from_value(json!({"type": "paste", "bytes": [1]})).unwrap();
    assert!(matches!(old, Command::Paste { mode: None, .. }));
    assert_eq!(
        serde_json::to_value(CoreEvent::PasteRequested {
            mode: PasteModeDto::Replace
        })
        .unwrap(),
        json!({"type": "pasteRequested", "mode": "replace"})
    );
    // Цель меню — только известных видов и полей.
    assert!(serde_json::from_value::<Command>(
        json!({"type": "contextMenu", "target": {"kind": "canvas", "x": 1, "y": 2, "z": 3}})
    )
    .is_err());
    assert!(serde_json::from_value::<Command>(
        json!({"type": "contextMenu", "target": {"kind": "ruler"}})
    )
    .is_err());
}

````

В `crates/ferrum-wasm/tests/text_edit.rs` заменить:

````rust
        Command::Paste {
            bytes: ByteBuf::from(b"not a ferrum clip".to_vec()),
        },
        Command::PasteImage {
````

на:

````rust
        Command::Paste {
            bytes: ByteBuf::from(b"not a ferrum clip".to_vec()),
            mode: None,
        },
        Command::PasteImage {
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test paste_modes --test commands --test context_menu --test keymap --test main_menu`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test paste --test protocol --test core --test text_edit --test boolean`
Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/shortcuts.test.ts`
Expected: FAIL — не собирается: редактор — нет `CommandId::{PasteOver, PasteReplace}`, `PasteMode`, `PASTE_STEP`, `Editor::paste_with`, `EditorEvent::PasteRequested(PasteMode)`; ядро — нет `PasteModeDto`, поля `mode` у `Command::Paste` и `CoreEvent::PasteRequested` (тесты протокола `ContextMenu` задачи 1 лежат в том же файле и собираются вместе с ними); модульные — `clipboardAction` не возвращает `'pasteOver'` для `Ctrl+Shift+KeyV` и `Shift+Meta+KeyV`

- [ ] **Step 3: Реализация**

В `apps/web/src/core/core.ts` заменить:

````ts
// Интерфейс (этап 5) работает с ядром только через этот класс.

import type { Backend, Command, CoreEvent, CursorKind } from '../wasm/ferrum_wasm';
import * as enc from './input';
import { BACKENDS, Kind, LIVE_NAMES } from './layout.gen';
````

на:

````ts
// Интерфейс (этап 5) работает с ядром только через этот класс.

import type { Backend, Command, CoreEvent, CursorKind, PasteModeDto } from '../wasm/ferrum_wasm';
import * as enc from './input';
import { BACKENDS, Kind, LIVE_NAMES } from './layout.gen';
````

В `apps/web/src/core/core.ts` заменить:

````ts

  /** Вставка текста с меткой из буфера обмена: текст уходит worker'у как есть, base64
   * декодирует он (предел длины — до отправки и ещё раз в worker'е). */
  pasteText(text: string): void {
    if (this.stopped) {
      this.emit({ type: 'error', message: 'Ядро остановлено, команда не выполнена' });
````

на:

````ts

  /** Вставка текста с меткой из буфера обмена: текст уходит worker'у как есть, base64
   * декодирует он (предел длины — до отправки и ещё раз в worker'е). `mode` —
   * способ вставки (поверх, с заменой). */
  pasteText(text: string, mode?: PasteModeDto): void {
    if (this.stopped) {
      this.emit({ type: 'error', message: 'Ядро остановлено, команда не выполнена' });
````

В `apps/web/src/core/core.ts` заменить:

````ts
    // Как у остальных вставок: набранное до вставки — раньше неё.
    this.transport.flush();
    const error = this.transport.post({ t: 'pasteText', text });
    if (error) this.emit({ type: 'error', message: error });
  }
````

на:

````ts
    // Как у остальных вставок: набранное до вставки — раньше неё.
    this.transport.flush();
    const error = this.transport.post({ t: 'pasteText', text, mode });
    if (error) this.emit({ type: 'error', message: error });
  }
````

В `apps/web/src/core/messages.ts` заменить:

````ts
// здесь — только конверт.

import type { Command, CoreEvent } from '../wasm/ferrum_wasm';
import { MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from './clipboard-text';
import { MAX_DATA_BYTES, MAX_FONT_BYTES, MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES, mb, tooLarge } from './limits';
````

на:

````ts
// здесь — только конверт.

import type { Command, CoreEvent, PasteModeDto } from '../wasm/ferrum_wasm';
import { MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from './clipboard-text';
import { MAX_DATA_BYTES, MAX_FONT_BYTES, MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES, mb, tooLarge } from './limits';
````

В `apps/web/src/core/messages.ts` заменить:

````ts
  /** Вставка текста с меткой из буфера обмена: base64 декодирует worker (основной
   * поток только переносит текст), затем — команда `paste`. */
  | { t: 'pasteText'; text: string }
  /** Текст с меткой для байтов буфера обмена (браузер отказал асинхронному API):
   * ответ — `clipboardText`. */
````

на:

````ts
  /** Вставка текста с меткой из буфера обмена: base64 декодирует worker (основной
   * поток только переносит текст), затем — команда `paste`. */
  | { t: 'pasteText'; text: string; mode?: PasteModeDto }
  /** Текст с меткой для байтов буфера обмена (браузер отказал асинхронному API):
   * ответ — `clipboardText`. */
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        }
      },
      // Ctrl/Cmd+C, X, V, пока они вызывают копирование, вырезание и вставку, идут
      // событиями буфера обмена (системный буфер), а не клавишами в ядро.
      isClipboardKey: (e) => {
````

на:

````tsx
        }
      },
      // Ctrl/Cmd+C, X, V (и Shift+V — «Вставить поверх»), пока они вызывают
      // копирование, вырезание и вставку, идут
      // событиями буфера обмена (системный буфер), а не клавишами в ядро.
      isClipboardKey: (e) => {
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        const action = s && clipboardAction(s, platform);
        if (!s || !action) return false;
        const key = { copy: 'edit.copy', cut: 'edit.cut', paste: 'edit.paste' }[action];
        return !!store.state.keymap.commands.find((c) => c.key === key)?.bindings.includes(s);
      },
````

на:

````tsx
        const action = s && clipboardAction(s, platform);
        if (!s || !action) return false;
        const key = { copy: 'edit.copy', cut: 'edit.cut', paste: 'edit.paste', pasteOver: 'edit.paste_over' }[action];
        return !!store.state.keymap.commands.find((c) => c.key === key)?.bindings.includes(s);
      },
````

В `apps/web/src/ui/shortcuts.ts` заменить:

````ts
}

/** Сочетание, которое браузер превращает в событие буфера обмена. */
export function clipboardAction(s: string, platform: Platform): 'copy' | 'cut' | 'paste' | null {
  const mod = platform === 'mac' ? 'Meta' : 'Ctrl';
  switch (s) {
````

на:

````ts
}

/** Сочетание, которое браузер превращает в событие буфера обмена. Ctrl/Cmd+Shift+V
 * — тоже `paste` (в браузере — «вставить обычным текстом»): на холсте это
 * «Вставить поверх», в правке текста — обычный текст. */
export function clipboardAction(s: string, platform: Platform): 'copy' | 'cut' | 'paste' | 'pasteOver' | null {
  const mod = platform === 'mac' ? 'Meta' : 'Ctrl';
  switch (s) {
````

В `apps/web/src/ui/shortcuts.ts` заменить:

````ts
    case `${mod}+KeyV`:
      return 'paste';
    default:
      return null;
````

на:

````ts
    case `${mod}+KeyV`:
      return 'paste';
    // Каноническая запись: Shift раньше Meta, но после Ctrl.
    case mod === 'Meta' ? 'Shift+Meta+KeyV' : 'Ctrl+Shift+KeyV':
      return 'pasteOver';
    default:
      return null;
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
import { tooLarge } from '../core/limits';
import type { ClipboardExtra } from '../core/messages';
import type { Editor } from './context';
import {
````

на:

````ts
import { tooLarge } from '../core/limits';
import type { ClipboardExtra } from '../core/messages';
import type { PasteModeDto } from '../wasm/ferrum_wasm';
import type { Editor } from './context';
import {
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
  };

  const deliver = async (data: PasteData) => {
    if (!data) return;
    switch (data.kind) {
      case 'ferrum':
        ed.send({ type: 'paste', bytes: data.bytes });
        return;
      case 'text':
        ed.core.pasteText(data.text);
        return;
      case 'tooBig':
````

на:

````ts
  };

  /** Вставка прочитанного: объекты Ferrum — способом `mode` (поверх, с заменой);
   * картинка из другого приложения — всегда обычной вставкой. */
  const deliver = async (data: PasteData, mode: PasteModeDto = 'normal') => {
    if (!data) return;
    switch (data.kind) {
      case 'ferrum':
        ed.send({ type: 'paste', bytes: data.bytes, mode });
        return;
      case 'text':
        ed.core.pasteText(data.text, mode);
        return;
      case 'tooBig':
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
    ed.send({ type: 'textInput', kind: 'paste', data: text });
  };
  /** Вставка с Shift: обычным текстом, в стиле у курсора. */
  let plainNext = false;
  const onKeyDown = (e: KeyboardEvent) => {
````

на:

````ts
    ed.send({ type: 'textInput', kind: 'paste', data: text });
  };
  /** Вставка с Shift: в правке текста — обычным текстом, в стиле у курсора; на
   * холсте — «Вставить поверх». */
  let plainNext = false;
  const onKeyDown = (e: KeyboardEvent) => {
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
    // проверяет то же самое.
    if (ed.store.state.commands['edit.paste']?.enabled === false) return;
    const sync = fromDataTransfer(e.clipboardData);
    if (sync) void deliver(sync);
    // Свой формат асинхронного API в событии не виден: событие пустое — читаем его.
    else if (eventIsEmpty(e.clipboardData)) void readAsync(true).then(deliver);
    else {
      const text = e.clipboardData?.getData('text/plain') ?? '';
````

на:

````ts
    // проверяет то же самое.
    if (ed.store.state.commands['edit.paste']?.enabled === false) return;
    const mode: PasteModeDto = plainNext && !isTextField(e.target) ? 'over' : 'normal';
    const sync = fromDataTransfer(e.clipboardData);
    if (sync) void deliver(sync, mode);
    // Свой формат асинхронного API в событии не виден: событие пустое — читаем его.
    else if (eventIsEmpty(e.clipboardData)) void readAsync(true).then((d) => deliver(d, mode));
    else {
      const text = e.clipboardData?.getData('text/plain') ?? '';
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
    else if (e.type === 'clipboardPng') void copyPng(e.bytes, sink);
    else if (e.type === 'clipboardSvg') void copySvg(e.bytes, sink);
    else if (e.type === 'pasteRequested') void readAsync(false).then(deliver);
    else if (e.type === 'requestImage') {
      pickFile(IMAGE_FILE)
````

на:

````ts
    else if (e.type === 'clipboardPng') void copyPng(e.bytes, sink);
    else if (e.type === 'clipboardSvg') void copySvg(e.bytes, sink);
    else if (e.type === 'pasteRequested') void readAsync(false).then((d) => deliver(d, e.mode));
    else if (e.type === 'requestImage') {
      pickFile(IMAGE_FILE)
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
          return;
        }
        run({ t: 'cmd', cmd: { type: 'paste', bytes } });
        break;
      }
````

на:

````ts
          return;
        }
        // Способ вставки не доверенный: его проверяет разбор команды в ядре.
        run({ t: 'cmd', cmd: { type: 'paste', bytes, mode: msg.mode } });
        break;
      }
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
/// Зазор между новым фреймом из пресета и содержимым страницы.
pub const PRESET_GAP: f64 = 100.0;

/// Выделенные дети одного родителя: родитель, число детей, позиции и узлы.
````

на:

````rust
/// Зазор между новым фреймом из пресета и содержимым страницы.
pub const PRESET_GAP: f64 = 100.0;

/// Сдвиг повторной вставки того же содержимого по каждой оси (спека подсистемы 2,
/// раздел 6).
pub const PASTE_STEP: f64 = 10.0;

/// Способ вставки из буфера обмена (спека подсистемы 2, раздел 6).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PasteMode {
    /// «Вставить» (Ctrl/Cmd+V): на место, если исходное видно и помещается в
    /// цель, иначе в центр цели; повтор того же содержимого — на +10, +10 от
    /// прошлой вставки.
    #[default]
    Normal,
    /// «Вставить поверх» (Ctrl/Cmd+Shift+V): на исходные координаты, без сдвига.
    Over,
    /// «Вставить с заменой» (Ctrl/Cmd+Shift+R): на место каждого выделенного узла.
    Replace,
}

/// Узел, который заменит вставка: родитель, его мировая матрица, место в слоях
/// и левый верхний угол рамки в мире.
struct ReplaceTarget {
    id: NodeId,
    parent: Option<NodeId>,
    parent_world: Affine,
    index: usize,
    origin: Point,
}

/// Выделенные дети одного родителя: родитель, число детей, позиции и узлы.
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
                }
            }
            C::Paste => self.out.events.push(EditorEvent::PasteRequested),
            C::CopyAsPng => {
                let ids = self.ordered_selection();
````

на:

````rust
                }
            }
            C::Paste => self
                .out
                .events
                .push(EditorEvent::PasteRequested(PasteMode::Normal)),
            C::PasteOver => self
                .out
                .events
                .push(EditorEvent::PasteRequested(PasteMode::Over)),
            C::PasteReplace => self
                .out
                .events
                .push(EditorEvent::PasteRequested(PasteMode::Replace)),
            C::CopyAsPng => {
                let ids = self.ordered_selection();
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust

    /// Копия выделения в формате буфера обмена Ferrum; `None` — нечего копировать.
    pub fn copy(&mut self) -> Option<Vec<u8>> {
        let ids = self.ordered_selection();
````

на:

````rust

    /// Копия выделения в формате буфера обмена Ferrum; `None` — нечего копировать.
    /// Новая копия начинает счёт повторных вставок заново.
    pub fn copy(&mut self) -> Option<Vec<u8>> {
        let ids = self.ordered_selection();
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            return None;
        }
        let page = self.doc.page(self.page).ok()?;
        let mut nodes: Vec<ClipNode> = Vec::new();
````

на:

````rust
            return None;
        }
        self.last_paste = None;
        let page = self.doc.page(self.page).ok()?;
        let mut nodes: Vec<ClipNode> = Vec::new();
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
    /// и история не меняются.
    pub fn paste(&mut self, bytes: &[u8]) -> Result<(), ClipboardError> {
        self.begin_doc_edit();
        let payload = match ClipPayload::decode(bytes) {
````

на:

````rust
    /// и история не меняются.
    pub fn paste(&mut self, bytes: &[u8]) -> Result<(), ClipboardError> {
        self.paste_with(bytes, PasteMode::Normal)
    }

    /// Вставка способом `mode` (обычная, поверх, с заменой) — одним шагом. В
    /// правке текста любой способ вставляет тексты в правку. «С заменой» без
    /// выделения (его сняли, пока читался буфер) — обычная вставка.
    pub fn paste_with(&mut self, bytes: &[u8], mode: PasteMode) -> Result<(), ClipboardError> {
        self.begin_doc_edit();
        let payload = match ClipPayload::decode(bytes) {
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            self.end_text_edit();
        }
        let (target, target_world, target_box) = self.paste_target();
        let offset = self.paste_offset(payload.bounds, target_box);
        let shift = Affine::translate(offset);
        let ok = self.ctx().commit(|tx, _| {
````

на:

````rust
            self.end_text_edit();
        }
        if mode == PasteMode::Replace {
            let targets = self.replace_targets();
            if !targets.is_empty() {
                let ok = self.commit_replace(&payload, &targets);
                return self.finish_paste(&payload, ok);
            }
        }
        let (target, target_world, target_box) = self.paste_target();
        let key = *blake3::hash(bytes).as_bytes();
        let offset = match mode {
            PasteMode::Over => Vec2::ZERO,
            _ => {
                let base = self.paste_offset(payload.bounds, target_box);
                // То же содержимое, а исходное видно: на шаг дальше прошлой вставки.
                match self.last_paste {
                    Some((k, prev)) if k == key && base == Vec2::ZERO => {
                        prev + Vec2::new(PASTE_STEP, PASTE_STEP)
                    }
                    _ => base,
                }
            }
        };
        let shift = Affine::translate(offset);
        let ok = self.ctx().commit(|tx, _| {
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            Ok(roots)
        });
        if ok {
            // Байты кладутся только после успешной вставки: хэши уже проверены
````

на:

````rust
            Ok(roots)
        });
        if ok && mode != PasteMode::Over {
            self.last_paste = Some((key, offset));
        }
        self.finish_paste(&payload, ok)
    }

    /// Выделенные узлы, которые заменит вставка, — верхнего уровня, без
    /// заблокированных, сверху вниз: вставка над верхним не сдвигает места нижних.
    fn replace_targets(&self) -> Vec<ReplaceTarget> {
        let mut out: Vec<ReplaceTarget> = self
            .ordered_selection()
            .into_iter()
            .filter_map(|id| {
                let slot = self.scene.slot(id)?;
                if locked(&self.scene, slot) || self.scene.is_draft(id) {
                    return None;
                }
                let n = self.scene.at(slot);
                Some(ReplaceTarget {
                    id,
                    parent: n.parent.map(|p| self.scene.at(p).id),
                    parent_world: parent_world(&self.scene, slot),
                    index: self.scene.index(slot) as usize,
                    origin: world_box(&self.scene, slot)?.origin(),
                })
            })
            .collect();
        out.reverse();
        out
    }

    /// «Вставить с заменой» одним шагом: содержимое буфера встаёт на место каждого
    /// узла — левый верхний угол его рамки, тот же родитель и место в слоях (как в
    /// Figma), — а узел удаляется. Выделяются вставленные корни.
    fn commit_replace(&mut self, payload: &ClipPayload, targets: &[ReplaceTarget]) -> bool {
        self.ctx().commit(|tx, _| {
            let mut all = Vec::new();
            for t in targets {
                let shift = Affine::translate(t.origin - payload.bounds.origin());
                let mut made: Vec<NodeId> = Vec::with_capacity(payload.nodes.len());
                let mut index = t.index;
                for n in &payload.nodes {
                    let mut props = n.props.clone();
                    let (parent, at) = match n.parent {
                        Some(i) => (Some(made[i]), None),
                        None => {
                            let world = shift * ferrum_scene::shape::local_transform(&props);
                            let pose = decompose_props(t.parent_world.inverse() * world, &props);
                            props.x = pose.x;
                            props.y = pose.y;
                            props.rotation = pose.rotation;
                            (props.flip_x, props.flip_y) = (pose.flip_x, pose.flip_y);
                            // Корни — над заменяемым узлом по порядку, снизу вверх.
                            index += 1;
                            (t.parent, Some(index))
                        }
                    };
                    let id = tx.create_with_props(parent, at, &props)?;
                    if n.parent.is_none() {
                        all.push(id);
                    }
                    made.push(id);
                }
                tx.delete_nodes(&[t.id])?;
            }
            Ok(all)
        })
    }

    /// Конец вставки: байты изображений — в документ, если она удалась; ошибка —
    /// её текстом из события.
    fn finish_paste(&mut self, payload: &ClipPayload, ok: bool) -> Result<(), ClipboardError> {
        if ok {
            // Байты кладутся только после успешной вставки: хэши уже проверены
````

В `crates/ferrum-editor/src/command_state.rs` заменить:

````rust
            C::Copy => (sel || text, None),
            C::Cut => (edit && (sel || text), None),
            C::Paste => (edit, None),
            C::CopyAsPng | C::CopyAsSvg | C::ZoomToSelection => (sel, None),
            // В правке вектора удаление и сдвиг — у точек.
````

на:

````rust
            C::Copy => (sel || text, None),
            C::Cut => (edit && (sel || text), None),
            C::Paste | C::PasteOver => (edit, None),
            // Заменять нечего без выделения.
            C::PasteReplace => (edit && sel, None),
            C::CopyAsPng | C::CopyAsSvg | C::ZoomToSelection => (sel, None),
            // В правке вектора удаление и сдвиг — у точек.
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    Cut,
    Paste,
    CopyAsPng,
    /// «Копировать как SVG» (подсистема 3, этап 5): без сочетания, как в Figma.
````

на:

````rust
    Cut,
    Paste,
    /// «Вставить поверх» (Ctrl/Cmd+Shift+V): на исходные координаты, без сдвига
    /// к центру вида (спека подсистемы 2, раздел 6).
    PasteOver,
    /// «Вставить с заменой» (Ctrl/Cmd+Shift+R, как в Figma): содержимое буфера —
    /// на место каждого выделенного узла.
    PasteReplace,
    CopyAsPng,
    /// «Копировать как SVG» (подсистема 3, этап 5): без сочетания, как в Figma.
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        &["буфер", "paste"],
        &["Mod+KeyV"],
    ),
    cmd(
````

на:

````rust
        &["буфер", "paste"],
        &["Mod+KeyV"],
    ),
    // Как в Figma: Ctrl/Cmd+Shift+V. В правке текста то же сочетание по-прежнему
    // вставляет обычный текст (его решает интерфейс по событию `paste`).
    cmd(
        C::PasteOver,
        "edit.paste_over",
        G::Edit,
        "Вставить поверх",
        &["на место", "на то же место", "paste over", "paste in place"],
        &["Mod+Shift+KeyV"],
    ),
    // Как в Figma: Ctrl/Cmd+Shift+R. В браузере сочетание занято перезагрузкой
    // (настройки предупреждают); команда есть в меню.
    cmd(
        C::PasteReplace,
        "edit.paste_replace",
        G::Edit,
        "Вставить с заменой",
        &["заменить", "подменить", "paste to replace", "replace"],
        &["Mod+Shift+KeyR"],
    ),
    cmd(
````

В `crates/ferrum-editor/src/context_menu.rs` заменить:

````rust
        if self.selection.is_empty() {
            // Пустое место холста.
            return vec![I(C::Paste), S, I(C::SelectAll), I(C::ZoomToFit)];
        }
        let mut items = vec![
            I(C::Copy),
            I(C::Paste),
            sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
        ];
````

на:

````rust
        if self.selection.is_empty() {
            // Пустое место холста.
            return vec![
                I(C::Paste),
                I(C::PasteOver),
                S,
                I(C::SelectAll),
                I(C::ZoomToFit),
            ];
        }
        let mut items = vec![
            I(C::Copy),
            I(C::Paste),
            I(C::PasteOver),
            I(C::PasteReplace),
            sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
        ];
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        plain: String,
    },
    /// Интерфейсу пора прочитать системный буфер и вызвать `paste`.
    PasteRequested,
    /// Скопировать как PNG: интерфейс экспортирует узлы и кладёт картинку в буфер.
    ExportPng(Vec<NodeId>),
````

на:

````rust
        plain: String,
    },
    /// Интерфейсу пора прочитать системный буфер и вызвать `paste_with` с этим
    /// способом вставки.
    PasteRequested(crate::actions::PasteMode),
    /// Скопировать как PNG: интерфейс экспортирует узлы и кладёт картинку в буфер.
    ExportPng(Vec<NodeId>),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Состояния команд, отправленные интерфейсу, и их ключ.
    pub(crate) states: StatesCache,
}

````

на:

````rust
    /// Состояния команд, отправленные интерфейсу, и их ключ.
    pub(crate) states: StatesCache,
    /// Прошлая вставка: хэш содержимого и сдвиг — следующая вставка того же
    /// содержимого встаёт на +10, +10 от неё (спека подсистемы 2, раздел 6).
    pub(crate) last_paste: Option<([u8; 32], Vec2)>,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            outlines_missed,
            states: StatesCache::default(),
        })
    }
````

на:

````rust
            outlines_missed,
            states: StatesCache::default(),
            last_paste: None,
        })
    }
````

В `crates/ferrum-editor/src/keymap.rs` заменить:

````rust
        "KeyT",
        "KeyN",
        "Shift+KeyW",
        "Shift+KeyT",
````

на:

````rust
        "KeyT",
        "KeyN",
        // Перезагрузка и перезагрузка без кэша.
        "KeyR",
        "Shift+KeyR",
        "Shift+KeyW",
        "Shift+KeyT",
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
mod vector_select;

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
pub use command_state::CommandState;
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
````

на:

````rust
mod vector_select;

pub use actions::{PasteMode, FIT_PADDING_PX, PASTE_STEP, PRESET_GAP};
pub use command_state::CommandState;
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
//! команд по ключам таблицы, подменю и разделителей. Порядок и состав повторяют главное
//! меню Figma UI3 без плагинов, виджетов и Dev Mode. В дереве — только команды,
//! которые уже есть: пункты следующих этапов (вставить поверх, с заменой,
//! копирование свойств, линейки, выделить подобные) появятся вместе с ними, а не
//! серыми заглушками. Интерфейс получает дерево один раз и по пункту присылает
//! `Execute` с ключом команды.

````

на:

````rust
//! команд по ключам таблицы, подменю и разделителей. Порядок и состав повторяют главное
//! меню Figma UI3 без плагинов, виджетов и Dev Mode. В дереве — только команды,
//! которые уже есть: пункты следующих этапов (линейки) появятся вместе с ними,
//! а не серыми заглушками. Интерфейс получает дерево один раз и по пункту присылает
//! `Execute` с ключом команды.

````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
                I(C::Cut),
                I(C::Paste),
                sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
                S,
````

на:

````rust
                I(C::Cut),
                I(C::Paste),
                I(C::PasteOver),
                I(C::PasteReplace),
                sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
                S,
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
use ferrum_editor::geometry::SelectionBox;
use ferrum_editor::{
    CommandId, CommandState, Cursor, Editor, EditorEvent, Input, Platform, ThemeChoice, ToolKind,
};
use ferrum_render::{
````

на:

````rust
use ferrum_editor::geometry::SelectionBox;
use ferrum_editor::{
    CommandId, CommandState, Cursor, Editor, EditorEvent, Input, PasteMode, Platform, ThemeChoice,
    ToolKind,
};
use ferrum_render::{
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
use crate::live::LiveState;
use crate::protocol::{
    Backend, Command, CommandStateDto, CoreEvent, CursorKind, TextInputKindDto, Theme, Tool,
    MAX_DEMO_NODES, MAX_DEMO_TEXTS, MAX_JOURNAL_BATCHES, SELECTION_IDS_MAX, THUMB_HEIGHT,
    THUMB_WIDTH,
};
use crate::summary::Summary;
````

на:

````rust
use crate::live::LiveState;
use crate::protocol::{
    Backend, Command, CommandStateDto, CoreEvent, CursorKind, PasteModeDto, TextInputKindDto,
    Theme, Tool, MAX_DEMO_NODES, MAX_DEMO_TEXTS, MAX_JOURNAL_BATCHES, SELECTION_IDS_MAX,
    THUMB_HEIGHT, THUMB_WIDTH,
};
use crate::summary::Summary;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                self.editor.set_settings(s);
            }
            Command::Paste { bytes } => {
                self.editor.take_text_pending();
                // Ошибку вставки редактор сам отдаёт событием `Error`.
                let _ = self.editor.paste(&bytes);
                // Фрагмент в правку текста ждёт шрифтов: повторить по приходу.
                if self.editor.take_text_pending() {
                    self.wait(fonts::Waiting::Command(Command::Paste { bytes }));
                }
            }
````

на:

````rust
                self.editor.set_settings(s);
            }
            Command::Paste { bytes, mode } => {
                self.editor.take_text_pending();
                let how = match mode.unwrap_or_default() {
                    PasteModeDto::Normal => PasteMode::Normal,
                    PasteModeDto::Over => PasteMode::Over,
                    PasteModeDto::Replace => PasteMode::Replace,
                };
                // Ошибку вставки редактор сам отдаёт событием `Error`.
                let _ = self.editor.paste_with(&bytes, how);
                // Фрагмент в правку текста ждёт шрифтов: повторить по приходу.
                if self.editor.take_text_pending() {
                    self.wait(fonts::Waiting::Command(Command::Paste { bytes, mode }));
                }
            }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                    })
                }
                EditorEvent::PasteRequested => self.events.push(CoreEvent::PasteRequested),
                EditorEvent::ExportPng(ids) => self.copy_png(&ids),
                EditorEvent::ExportSvg(ids) => self.copy_svg(&ids),
````

на:

````rust
                    })
                }
                EditorEvent::PasteRequested(m) => self.events.push(CoreEvent::PasteRequested {
                    mode: match m {
                        PasteMode::Normal => PasteModeDto::Normal,
                        PasteMode::Over => PasteModeDto::Over,
                        PasteMode::Replace => PasteModeDto::Replace,
                    },
                }),
                EditorEvent::ExportPng(ids) => self.copy_png(&ids),
                EditorEvent::ExportSvg(ids) => self.copy_svg(&ids),
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
            ExportFormat::Pdf => "application/pdf",
        }
    }
}

/// Тема оверлеев.
````

на:

````rust
            ExportFormat::Pdf => "application/pdf",
        }
    }
}

/// Способ вставки (спека подсистемы 2, раздел 6): обычная, поверх (на исходные
/// координаты), с заменой выделенного.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum PasteModeDto {
    #[default]
    Normal,
    Over,
    Replace,
}

/// Тема оверлеев.
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        system: Option<bool>,
    },
    /// Вставить содержимое буфера обмена (формат `web application/x-ferrum`).
    Paste {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Разместить изображение (PNG): следующий клик ставит его на холст.
````

на:

````rust
        system: Option<bool>,
    },
    /// Вставить содержимое буфера обмена (формат `web application/x-ferrum`)
    /// способом `mode` (без него — обычная вставка).
    Paste {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
        #[serde(default)]
        #[tsify(optional)]
        mode: Option<PasteModeDto>,
    },
    /// Разместить изображение (PNG): следующий клик ставит его на холст.
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        bytes: ByteBuf,
    },
    /// Пора прочитать системный буфер и прислать `Paste`.
    PasteRequested,
    /// Выбрать файл изображения и прислать `PlaceImage`.
    RequestImage,
````

на:

````rust
        bytes: ByteBuf,
    },
    /// Пора прочитать системный буфер и прислать `Paste` с этим способом.
    PasteRequested {
        mode: PasteModeDto,
    },
    /// Выбрать файл изображения и прислать `PlaceImage`.
    RequestImage,
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/paste.spec.ts e2e/app.spec.ts e2e/chrome.spec.ts e2e/clipboard.spec.ts e2e/context-menu.spec.ts e2e/settings.spec.ts --project=chromium`

Expected: PASS: fmt и clippy без замечаний; `ferrum-doc`, `ferrum-editor`, `ferrum-wasm` без падений (`paste_modes` — 5 тестов, `paste` — 1, `protocol` — 6; команд 82); модульные и сборка — ок; E2E `paste`, `app`, `chrome`, `clipboard`, `context-menu`, `settings` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts apps/web/e2e/chrome.spec.ts apps/web/e2e/clipboard.spec.ts apps/web/e2e/context-menu.spec.ts apps/web/e2e/paste.spec.ts apps/web/src/core/core.ts apps/web/src/core/messages.ts apps/web/src/ui/app.tsx apps/web/src/ui/shortcuts.ts apps/web/src/ui/system-clipboard.ts apps/web/src/worker/core.worker.ts apps/web/unit/shortcuts.test.ts crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/command_state.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/context_menu.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/keymap.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/menu.rs crates/ferrum-editor/tests/commands.rs crates/ferrum-editor/tests/context_menu.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/main_menu.rs crates/ferrum-editor/tests/paste_modes.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/boolean.rs crates/ferrum-wasm/tests/core.rs crates/ferrum-wasm/tests/paste.rs crates/ferrum-wasm/tests/protocol.rs crates/ferrum-wasm/tests/text_edit.rs
git commit -F - <<'EOF'
feat: «Вставить поверх» и «Вставить с заменой», повтор вставки со сдвигом +10, +10

Команды edit.paste_over (Ctrl/Cmd+Shift+V) и edit.paste_replace
(Ctrl/Cmd+Shift+R, как в Figma; сочетание браузер забирает под перезагрузку —
оно в списке занятых, настройки предупреждают). PasteRequested и Paste несут
способ вставки (PasteModeDto). Поверх — на исходные координаты; с заменой —
содержимое буфера на место каждого выделенного узла (угол рамки, родитель,
место в слоях) одним шагом. Повторная вставка того же содержимого, пока
исходное видно, — +10, +10 от прошлой; новая копия начинает счёт заново.
Ctrl/Cmd+Shift+V на холсте — событие paste с режимом «поверх», в правке
текста — по-прежнему обычный текст. Пункты — в «Правке» главного меню и в
контекстном меню холста; число команд в тестах — 82.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 5: «Копировать свойства» и «Вставить свойства» (Ctrl/Cmd+Alt+C и V)

Спека §6: копия свойств первого выделенного — во внутренний буфер редактора и текстом с меткой `ferrum-props:v1:` в системный буфер (`ClipboardProps`). «Вставить свойства» просит буфер (`PasteRequested { mode: props }`) и получает `PasteProps { bytes | text }`: копия свойств или копия узлов, а без них — внутренний буфер. Каждому выделенному — то, что подходит его типу, одной записью; тексту — стиль первого участка источника с подгонкой рамки. Пункты — в «Правке» и в меню холста; команд 82 → 84.

**Files:**
- Create: `crates/ferrum-editor/src/props_clip.rs`
- Modify: `crates/ferrum-editor/src/{text_style,actions,commands,command_state,editor,menu,context_menu,lib}.rs`, `crates/ferrum-doc/src/vector.rs` (`base64_encode`, `base64_decode` — `pub`), `crates/ferrum-wasm/src/{protocol,core}.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `apps/web/src/{core/clipboard-text.ts, core/messages.ts, ui/clipboard.ts, ui/system-clipboard.ts}`
- Test: `crates/ferrum-editor/tests/props_clip.rs` (новый), `crates/ferrum-editor/tests/{context_menu,keymap,main_menu}.rs`, `crates/ferrum-wasm/tests/{paste,protocol,boolean}.rs`, `apps/web/unit/{clipboard,messages}.test.ts`, `apps/web/e2e/{paste,chrome,context-menu,main-menu,app}.spec.ts`

**Interfaces:**
- Consumes: `PasteMode`, `paste_with`, формат буфера (`clipboard.rs`), `apply_text_changes` и `restyle` из `text_style.rs`, `base64_*` из `ferrum-doc`.
- Produces: `CommandId::{CopyProps, PasteProps}`; `PasteMode::Props`; `EditorEvent::ClipboardProps(String)`; `Editor::paste_props(&mut self, clip: Option<&[u8]>)`; `props_clip::{PROPS_MARKER, NODES_MARKER, MAX_PROPS_TEXT, NO_PROPS, props_text_bytes}`; `pub(crate) fn Editor::restyle_nodes_with(&mut self, ids: &[NodeId], patch: &TextStylePatch, extra: Vec<(NodeId, PropsPatch)>) -> Result<(), TextError>`, `apply_text_changes_with(edits, phase, patch, extra)` (`apply_text_changes` — через него с пустым `extra`). Протокол: `PasteModeDto::Props`, `Command::PasteProps { bytes?, text? }`, `CoreEvent::ClipboardProps { text }`. UI: `FERRUM_PROPS_MARKER`, `isPropsText(text)`, `propsFromClipboardItems(items): Promise<PropsData>`, `type PropsData`; `commandSizeError` для `pasteProps`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 82, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 84, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  // и «Выделить слой».
  const item = (label: string) => menu.locator(`[data-label="${label}"]`);
  for (const label of ['Войти в правку', 'Вставить', 'Вставить поверх', 'Вставить с заменой']) {
    await letter('в');
    await expect(item(label)).toBeFocused();
````

на:

````ts
  // и «Выделить слой».
  const item = (label: string) => menu.locator(`[data-label="${label}"]`);
  for (const label of ['Войти в правку', 'Вставить', 'Вставить поверх', 'Вставить с заменой', 'Вставить свойства']) {
    await letter('в');
    await expect(item(label)).toBeFocused();
````

В `apps/web/e2e/context-menu.spec.ts` заменить:

````ts
    'Вставить с заменой',
    'Копировать как',
    'Выделить слой',
    'Переместить вперёд',
````

на:

````ts
    'Вставить с заменой',
    'Копировать как',
    'Копировать свойства',
    'Вставить свойства',
    'Выделить слой',
    'Переместить вперёд',
````

В `apps/web/e2e/main-menu.spec.ts` заменить:

````ts
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  const last = menu.getByRole('menuitem').last();
  await expect(last).toBeFocused();
  const lb = (await last.boundingBox())!;
````

на:

````ts
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  // Последний результат — любой пункт (и переключатель — menuitemcheckbox).
  const last = menu.locator('[role^="menuitem"]').last();
  await expect(last).toBeFocused();
  const lb = (await last.boundingBox())!;
````

В `apps/web/e2e/paste.spec.ts` заменить:

````ts
  await expect(dialog.getByRole('row').filter({ hasText: 'Вставить с заменой' })).toContainText('Перехватывает браузер');
});
````

на:

````ts
  await expect(dialog.getByRole('row').filter({ hasText: 'Вставить с заменой' })).toContainText('Перехватывает браузер');
});

const summary = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)));

test('свойства: Ctrl/Cmd+Alt+C и Ctrl/Cmd+Alt+V — к фрейму одним шагом, что подходит его типу', async ({ page }) => {
  await openApp(page);
  // «Слой 1» — прямоугольник: скругление 8, обводка 1 внутри.
  await pick(page, 200, 40);
  await liveWhen(page, 'l.selectionCount === 1');
  const rect = await summary(page);
  await page.keyboard.press('ControlOrMeta+Alt+KeyC');
  await expect(page.getByRole('status').filter({ hasText: 'Свойства скопированы' })).toBeVisible();
  // Фрейм «Экран 0» — щелчком по его названию над левым верхним углом.
  const corner = await appScreen(page, await live(page), 0, 0);
  await page.mouse.click(corner.x + 10, corner.y - 6);
  const l = await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 1440');
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+Alt+KeyV');
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expect.poll(async () => (await summary(page)).cornerRadius?.value).toBe(8);
  const frame = await summary(page);
  expect(frame.fills).toEqual(rect.fills);
  expect(frame.strokes).toEqual(rect.strokes);
  expect([frame.w.value, frame.h.value]).toEqual([1440, 1024]);
});

test('копия свойств — текстом с меткой в системном буфере; Ctrl+V её не вставляет', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'чтение буфера — разрешение clipboard-read есть только в Chromium');
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await openApp(page);
  await pick(page, 200, 40);
  const l = await liveWhen(page, 'l.selectionCount === 1');
  await page.keyboard.press('ControlOrMeta+Alt+KeyC');
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toMatch(/^ferrum-props:v1:/);
  await page.keyboard.press('ControlOrMeta+v');
  await page.waitForTimeout(300);
  expect((await live(page)).nodes).toBe(l.nodes);
  // Из меню — свойства из системного буфера (к эллипсу «Слой 0»: скругления у него нет).
  await pick(page, 60, 40);
  const e = await liveWhen(page, 'l.selectionCount === 1 && l.selectionX === 0');
  await mainMenu(page, 'Правка', 'Вставить свойства');
  await liveWhen(page, `l.steps === ${e.steps + 1}`);
  await expect.poll(async () => (await summary(page)).kind).toBe('ellipse');
});
````

В `apps/web/unit/clipboard.test.ts` заменить:

````ts
  encodeText,
  FERRUM_MIME,
  FERRUM_TEXT_MARKER,
  fromClipboardItems,
  fromDataTransfer,
  MAX_CLIPBOARD_BYTES,
````

на:

````ts
  encodeText,
  FERRUM_MIME,
  FERRUM_PROPS_MARKER,
  FERRUM_TEXT_MARKER,
  fromClipboardItems,
  propsFromClipboardItems,
  fromDataTransfer,
  MAX_CLIPBOARD_BYTES,
````

В `apps/web/unit/clipboard.test.ts` заменить:

````ts
  });
});
````

на:

````ts
  });
});

test('«Вставить свойства» из асинхронного API: байты Ferrum, текст свойств или узлов; чужое — пусто', async () => {
  const item = (map: Record<string, Blob>) => ({ types: Object.keys(map), getType: async (t: string) => map[t] });
  expect(await propsFromClipboardItems([item({ [FERRUM_MIME]: new Blob([new Uint8Array([9])]) })])).toEqual({ bytes: new Uint8Array([9]) });
  const props = `${FERRUM_PROPS_MARKER}AA==`;
  expect(await propsFromClipboardItems([item({ 'text/plain': new Blob([props]) })])).toEqual({ text: props });
  const nodes = encodeText(new Uint8Array([5]));
  expect(await propsFromClipboardItems([item({ 'text/plain': new Blob([nodes]) })])).toEqual({ text: nodes });
  expect(await propsFromClipboardItems([item({ 'text/plain': new Blob(['просто текст']) })])).toEqual({});
  expect(await propsFromClipboardItems([])).toEqual({});
  // Копия свойств обычной вставкой не вставляется ни текстом, ни объектами.
  const dt = { getData: () => props, files: [] as unknown as FileList };
  expect(fromDataTransfer(dt)).toBeNull();
});
````

В `apps/web/unit/messages.test.ts` заменить:

````ts
  expect(cmd.journal[2].buffer.byteLength).toBe(8);
});
````

на:

````ts
  expect(cmd.journal[2].buffer.byteLength).toBe(8);
});

test('pasteProps: текст — не длиннее предела, байты — как у вставки', () => {
  expect(commandSizeError({ type: 'pasteProps', text: `${FERRUM_TEXT_MARKER}AQI=` })).toBeNull();
  expect(commandSizeError({ type: 'pasteProps' })).toBeNull();
  expect(commandSizeError({ type: 'pasteProps', text: 'A'.repeat(MAX_CLIPBOARD_TEXT + 1) })).toBe('Слишком большой объём данных: предел — 64 МБ');
  expect(commandSizeError({ type: 'pasteProps', bytes: new Uint8Array(4) })).toBeNull();
  expect(commandSizeError({ type: 'pasteProps', bytes: [1] as unknown as Uint8Array })).toBe('Неверная команда: данные не в виде байтов');
});
````

В `crates/ferrum-editor/tests/context_menu.rs` заменить:

````rust
            "edit.paste_over",
            "edit.paste_replace",
            "arrange.forward",
            "arrange.backward",
````

на:

````rust
            "edit.paste_over",
            "edit.paste_replace",
            "edit.copy_props",
            "edit.paste_props",
            "arrange.forward",
            "arrange.backward",
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    (CommandId::PasteOver, &["Mod+Shift+KeyV"]),
    (CommandId::PasteReplace, &["Mod+Shift+KeyR"]),
    (CommandId::Cut, &["Mod+KeyX"]),
    (CommandId::CopyAsPng, &["Mod+Shift+KeyC"]),
````

на:

````rust
    (CommandId::PasteOver, &["Mod+Shift+KeyV"]),
    (CommandId::PasteReplace, &["Mod+Shift+KeyR"]),
    (CommandId::CopyProps, &["Mod+Alt+KeyC"]),
    (CommandId::PasteProps, &["Mod+Alt+KeyV"]),
    (CommandId::Cut, &["Mod+KeyX"]),
    (CommandId::CopyAsPng, &["Mod+Shift+KeyC"]),
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 82);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 84);
}

````

В `crates/ferrum-editor/tests/main_menu.rs` заменить:

````rust
        "edit.paste_over",
        "edit.paste_replace",
        "edit.duplicate",
        "edit.select_all",
````

на:

````rust
        "edit.paste_over",
        "edit.paste_replace",
        "edit.copy_props",
        "edit.paste_props",
        "edit.duplicate",
        "edit.select_all",
````

Создать `crates/ferrum-editor/tests/props_clip.rs`:

````rust
//! «Копировать свойства» и «Вставить свойства» (спека подсистемы 2, раздел 6):
//! внутренний буфер и текст с меткой для системного, к каждому узлу — то, что
//! подходит его типу, одним шагом; копия узлов тоже несёт свойства (первого
//! узла); стиль текста; не доверенные данные.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::props_clip::{props_text_bytes, NO_PROPS, PROPS_MARKER};
use ferrum_editor::*;

fn styled(h: &mut H) -> NodeId {
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.set(
        r,
        PropsPatch {
            opacity: Some(0.5),
            corner_radius: Some(12.0),
            fills: Some(vec![Fill::solid(Color::rgba(200, 10, 20, 255))]),
            strokes: Some(vec![Stroke {
                color: Color::rgba(0, 0, 255, 255),
                width: 3.0,
                align: StrokeAlign::Outside,
            }]),
            stroke_join: Some(StrokeJoin::Round),
            ..Default::default()
        },
    );
    r
}

/// «Копировать свойства» выделения `ids`: текст для системного буфера.
fn copy_props(h: &mut H, ids: &[NodeId]) -> String {
    h.ed.select(ids);
    h.ed.take_events();
    h.ed.execute(CommandId::CopyProps);
    h.ed.take_events()
        .into_iter()
        .find_map(|e| match e {
            EditorEvent::ClipboardProps(t) => Some(t),
            _ => None,
        })
        .expect("текст свойств")
}

fn errors(h: &mut H) -> Vec<String> {
    h.ed.take_events()
        .into_iter()
        .filter_map(|e| match e {
            EditorEvent::Error(m) => Some(m),
            _ => None,
        })
        .collect()
}

#[test]
fn paste_applies_what_fits_each_type_in_one_step() {
    let mut h = H::new();
    let src = styled(&mut h);
    let text = copy_props(&mut h, &[src]);
    assert!(text.starts_with(PROPS_MARKER));
    let e = h.add(None, NodeKind::Ellipse, patch(300.0, 100.0, 40.0, 40.0));
    let line = h.add(None, NodeKind::Line, patch(300.0, 300.0, 100.0, 0.0));
    let f = h.frame(None, 500.0, 100.0, 100.0, 100.0);
    h.ed.select(&[e, line, f]);
    let steps = h.ed.step_count();
    let before = h.state();
    h.ed.paste_props(None);
    let s = h.props(src);
    let pe = h.props(e);
    assert_eq!(
        (pe.fills.clone(), pe.strokes.clone(), pe.opacity),
        (s.fills.clone(), s.strokes.clone(), 0.5)
    );
    assert_eq!(pe.stroke_style.join, StrokeJoin::Round);
    assert_eq!(pe.corner_radius, 0.0, "у эллипса скругления нет");
    assert_eq!((pe.x, pe.w), (300.0, 40.0), "геометрия не меняется");
    let pl = h.props(line);
    assert!(pl.fills.is_empty(), "у линии нет заливок");
    assert_eq!(pl.strokes, s.strokes);
    let pf = h.props(f);
    assert_eq!(
        (pf.corner_radius, pf.fills.clone()),
        (12.0, s.fills.clone())
    );
    assert_eq!(h.sel(), [e, line, f], "выделение то же");
    h.assert_one_step(steps, &before);
}

#[test]
fn system_clipboard_text_carries_props_to_another_document() {
    let mut h = H::new();
    let src = styled(&mut h);
    let text = copy_props(&mut h, &[src]);
    let bytes = props_text_bytes(&text).expect("метка и base64");
    let mut other = H::new();
    let r = other.rect(None, 0.0, 0.0, 10.0, 10.0);
    other.ed.select(&[r]);
    other.ed.paste_props(Some(&bytes));
    let p = other.props(r);
    assert_eq!((p.opacity, p.corner_radius), (0.5, 12.0));
    assert_eq!(p.fills, h.props(src).fills);
    // Чужой текст — не наши данные: `None`, вставка возьмёт внутренний буфер.
    assert_eq!(props_text_bytes("просто текст"), None);
    assert_eq!(props_text_bytes(&format!("{PROPS_MARKER}не base64!")), None);
}

#[test]
fn node_copy_gives_the_props_of_its_first_node() {
    let mut h = H::new();
    let src = styled(&mut h);
    h.ed.select(&[src]);
    h.ed.take_events();
    h.ed.execute(CommandId::Copy);
    let bytes =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::Clipboard(b) => Some(b),
                _ => None,
            })
            .unwrap();
    let r = h.rect(None, 400.0, 400.0, 30.0, 30.0);
    h.ed.select(&[r]);
    h.ed.paste_props(Some(&bytes));
    assert_eq!(h.props(r).corner_radius, 12.0);
    assert_eq!((h.props(r).x, h.props(r).w), (400.0, 30.0));
}

#[test]
fn nothing_copied_or_hostile_data_is_an_error_without_a_step() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[r]);
    let steps = h.ed.step_count();
    h.ed.take_events();
    h.ed.paste_props(None);
    assert_eq!(errors(&mut h), [NO_PROPS]);
    h.ed.paste_props(Some(b"FERRUMCB\x01\x00\x00\x00\xff\xff\xff\xff"));
    assert_eq!(errors(&mut h).len(), 1);
    h.ed.paste_props(Some("не данные".as_bytes()));
    assert_eq!(errors(&mut h).len(), 1);
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn text_style_goes_to_texts_with_fills_in_one_step() {
    let mut h = text_h();
    let a = add_text(&mut h, 100.0, 100.0, "Образец", 30.0);
    h.set(
        a,
        PropsPatch {
            fills: Some(vec![Fill::solid(Color::rgba(0, 128, 0, 255))]),
            ..Default::default()
        },
    );
    let b = add_text(&mut h, 100.0, 300.0, "Цель", 12.0);
    let r = h.rect(None, 400.0, 400.0, 30.0, 30.0);
    copy_props(&mut h, &[a]);
    h.ed.select(&[b, r]);
    let steps = h.ed.step_count();
    let before = h.state();
    h.ed.paste_props(None);
    assert!(!h.ed.take_text_pending());
    let pb = h.props(b);
    let t = pb.text.as_ref().unwrap();
    assert_eq!(t.props.style.size, 30.0);
    assert_eq!(pb.fills, h.props(a).fills);
    assert!(pb.w > 50.0, "рамка подогнана под новый кегль: {}", pb.w);
    assert_eq!(text_of(&mut h, b), "Цель\n", "содержимое не меняется");
    assert_eq!(
        h.props(r).fills,
        h.props(a).fills,
        "у фигуры — заливка, стиля текста нет"
    );
    h.assert_one_step(steps, &before);
}

#[test]
fn copy_and_paste_props_follow_state() {
    let mut h = H::new();
    assert!(!h.ed.command_state(CommandId::CopyProps).enabled);
    assert!(!h.ed.command_state(CommandId::PasteProps).enabled);
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[r]);
    assert!(h.ed.command_state(CommandId::CopyProps).enabled);
    assert!(h.ed.command_state(CommandId::PasteProps).enabled);
    h.ed.take_events();
    h.ed.execute(CommandId::PasteProps);
    assert!(h
        .ed
        .take_events()
        .contains(&EditorEvent::PasteRequested(PasteMode::Props)));
    h.key("KeyC", ctrl().with_alt());
    assert!(h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::ClipboardProps(_))));
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    assert!(
        h.ed.command_state(CommandId::CopyProps).enabled,
        "копировать можно"
    );
    assert!(!h.ed.command_state(CommandId::PasteProps).enabled);
}
````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 82);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

на:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 84);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

В `crates/ferrum-wasm/tests/paste.rs` заменить:

````rust
    assert_eq!(x(&core), before, "на место заменённого");
}
````

на:

````rust
    assert_eq!(x(&core), before, "на место заменённого");
}

#[test]
fn props_go_through_text_with_a_marker_or_the_internal_buffer() {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    core.command(Command::Demo { nodes: 100 });
    // «Слой 1» — прямоугольник со скруглением 8: копия его свойств.
    click(&mut core, 140.0, 64.0, 1000.0);
    let ev = execute(&mut core, "edit.copy_props");
    let text = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::ClipboardProps { text } => Some(text.clone()),
            _ => None,
        })
        .expect("текст свойств");
    assert!(text.starts_with("ferrum-props:v1:"));
    let ev = execute(&mut core, "edit.paste_props");
    assert!(ev.contains(&CoreEvent::PasteRequested {
        mode: PasteModeDto::Props
    }));
    // Фрейм «Экран 0» — из панели слоёв: свойства из текста.
    let frame = {
        let scene = core.editor().scene();
        scene.at(scene.roots()[0]).id
    };
    core.command(Command::Select {
        ids: vec![frame.to_string()],
        mode: SelectMode::Replace,
    });
    let steps = core.live().steps;
    core.command(Command::PasteProps {
        bytes: None,
        text: Some(text),
    });
    let radius = |core: &Core| {
        core.editor()
            .scene()
            .node(frame)
            .unwrap()
            .props
            .corner_radius
    };
    assert_eq!(radius(&core), 8.0);
    assert_eq!(core.live().steps, steps + 1.0);
    // Чужой текст — внутренний буфер ядра (та же копия): шаг без изменений не пишется.
    core.command(Command::PasteProps {
        bytes: None,
        text: Some("просто текст".into()),
    });
    assert_eq!(radius(&core), 8.0);
    assert!(!core
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { .. })));
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        },
        Command::Save,
        Command::ContextMenu {
            target: ContextTarget::Canvas { x: 1.5, y: 2.0 },
````

на:

````rust
        },
        Command::Save,
        Command::PasteProps {
            bytes: None,
            text: Some("ferrum-props:v1:AA==".into()),
        },
        Command::PasteProps {
            bytes: Some(bytes(&[1])),
            text: None,
        },
        Command::ContextMenu {
            target: ContextTarget::Canvas { x: 1.5, y: 2.0 },
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        CoreEvent::PasteRequested {
            mode: PasteModeDto::Over,
        },
        CoreEvent::ContextMenu {
````

на:

````rust
        CoreEvent::PasteRequested {
            mode: PasteModeDto::Over,
        },
        CoreEvent::ClipboardProps {
            text: "ferrum-props:v1:AA==".into(),
        },
        CoreEvent::ContextMenu {
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test props_clip --test context_menu --test keymap --test main_menu`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test paste --test protocol --test boolean`
Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/clipboard.test.ts unit/messages.test.ts`
Expected: FAIL — не собирается: редактор — нет модуля `props_clip`, `CommandId::{CopyProps, PasteProps}`, `PasteMode::Props`, `EditorEvent::ClipboardProps`, `Editor::paste_props`; ядро — нет `Command::PasteProps`, `CoreEvent::ClipboardProps`, `PasteModeDto::Props`; модульные — новые тесты падают: нет экспортов `isPropsText`, `propsFromClipboardItems`, `FERRUM_PROPS_MARKER` (`clipboard.test.ts`), `commandSizeError` не знает сообщения `pasteProps` (`messages.test.ts`)

- [ ] **Step 3: Реализация**

В `apps/web/src/core/clipboard-text.ts` заменить:

````ts
}

/** Текст с меткой (не декодируя). */
export function isMarkedText(text: string): boolean {
````

на:

````ts
}

/** Метка копии свойств в `text/plain` («Копировать свойства»; текст готовит ядро).
 * Обычной вставкой такой текст не вставляется. */
export const FERRUM_PROPS_MARKER = 'ferrum-props:v1:';

export function isPropsText(text: string): boolean {
  return text.startsWith(FERRUM_PROPS_MARKER);
}

/** Текст с меткой (не декодируя). */
export function isMarkedText(text: string): boolean {
````

В `apps/web/src/core/messages.ts` заменить:

````ts
  }
  if (cmd.type === 'openWithJournal') return journalCommandError(cmd);
  if (cmd.type === 'loadFont' && (cmd as { bytes?: unknown }).bytes === undefined) return null;
  if (
````

на:

````ts
  }
  if (cmd.type === 'openWithJournal') return journalCommandError(cmd);
  if (cmd.type === 'pasteProps') {
    // Текст из буфера: предел — до отправки (его разбирает ядро); байты — как у вставки.
    const { bytes, text } = cmd as { bytes?: unknown; text?: unknown };
    if (text !== undefined && (typeof text !== 'string' || text.length > MAX_CLIPBOARD_TEXT)) return tooLarge(MAX_CLIPBOARD_BYTES);
    if (bytes === undefined) return null;
    const kind = bytesKindError(bytes);
    if (kind) return kind;
    const size = (bytes as Uint8Array).byteLength;
    return size > MAX_COMMAND_BYTES.paste ? tooLarge(MAX_COMMAND_BYTES.paste, size) : null;
  }
  if (cmd.type === 'loadFont' && (cmd as { bytes?: unknown }).bytes === undefined) return null;
  if (
````

В `apps/web/src/ui/clipboard.ts` заменить:

````ts
// WebP) ставятся узлом-изображением; не-PNG перекодирует worker.

import { FERRUM_TEXT_MARKER, isMarkedText, MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from '../core/clipboard-text';

export {
  decodeText,
  encodeText,
  FERRUM_TEXT_MARKER,
  fromBase64,
  MAX_CLIPBOARD_BYTES,
  MAX_CLIPBOARD_TEXT,
````

на:

````ts
// WebP) ставятся узлом-изображением; не-PNG перекодирует worker.

import { FERRUM_PROPS_MARKER, FERRUM_TEXT_MARKER, isMarkedText, isPropsText, MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from '../core/clipboard-text';

export {
  decodeText,
  encodeText,
  FERRUM_PROPS_MARKER,
  FERRUM_TEXT_MARKER,
  fromBase64,
  isPropsText,
  MAX_CLIPBOARD_BYTES,
  MAX_CLIPBOARD_TEXT,
````

В `apps/web/src/ui/clipboard.ts` заменить:

````ts
  }
}
````

на:

````ts
  }
}

/** Источник «Вставить свойства» в системном буфере: байты копии Ferrum или текст
 * с меткой свойств или узлов (его разбирает ядро); ничего — внутренний буфер ядра. */
export type PropsData = { bytes: Uint8Array } | { text: string } | Record<string, never>;

/** Выбор для «Вставить свойства» из элементов асинхронного Clipboard API. Чужой
 * текст не читается целиком: сначала — только начало (метка). */
export async function propsFromClipboardItems(items: readonly Pick<ClipboardItem, 'types' | 'getType'>[]): Promise<PropsData> {
  for (const item of items) {
    if (item.types.includes(FERRUM_MIME)) {
      const blob = await item.getType(FERRUM_MIME);
      if (blob.size > MAX_CLIPBOARD_BYTES) return {};
      return { bytes: new Uint8Array(await blob.arrayBuffer()) };
    }
    if (item.types.includes('text/plain')) {
      const blob = await item.getType('text/plain');
      const head = await blob.slice(0, Math.max(FERRUM_PROPS_MARKER.length, FERRUM_TEXT_MARKER.length)).text();
      if ((isPropsText(head) || isMarkedText(head)) && blob.size <= MAX_CLIPBOARD_TEXT) return { text: await blob.text() };
    }
  }
  return {};
}
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
  fromClipboardItems,
  fromDataTransfer,
  MAX_CLIPBOARD_BYTES,
  supportsCustomFormat,
  type PasteData,
} from './clipboard';
import { asyncWriter, copyPng, copySvg, type ClipboardSink } from './clipboard-write';
````

на:

````ts
  fromClipboardItems,
  fromDataTransfer,
  isPropsText,
  MAX_CLIPBOARD_BYTES,
  propsFromClipboardItems,
  supportsCustomFormat,
  type PasteData,
  type PropsData,
} from './clipboard';
import { asyncWriter, copyPng, copySvg, type ClipboardSink } from './clipboard-write';
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
  };

  const onCopy = (command: 'edit.copy' | 'edit.cut') => (e: ClipboardEvent) => {
    if (pending && command === 'edit.copy') {
````

на:

````ts
  };

  /** «Копировать свойства»: текст с меткой — в системный буфер. Отказ браузера не
   * мешает вставке в этой вкладке (у ядра свой буфер), поэтому без ошибки. */
  const writeProps = (text: string) => {
    const done = () => ed.store.toast('Свойства скопированы');
    const clip = (globalThis as { navigator?: { clipboard?: Clipboard } }).navigator?.clipboard;
    if (typeof clip?.writeText !== 'function') {
      execText(text);
      done();
      return;
    }
    clip.writeText(text).then(done, () => {
      execText(text);
      done();
    });
  };
  /** «Вставить свойства»: что лежит в системном буфере. Нет доступа или там не
   * наше — пусто: ядро возьмёт свойства из своего буфера, без уведомлений. */
  const readProps = async (): Promise<PropsData> => {
    try {
      return await propsFromClipboardItems(await navigator.clipboard.read());
    } catch {
      return {};
    }
  };

  const onCopy = (command: 'edit.copy' | 'edit.cut') => (e: ClipboardEvent) => {
    if (pending && command === 'edit.copy') {
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
    if (ed.store.state.commands['edit.paste']?.enabled === false) return;
    const mode: PasteModeDto = plainNext && !isTextField(e.target) ? 'over' : 'normal';
    const sync = fromDataTransfer(e.clipboardData);
    if (sync) void deliver(sync, mode);
````

на:

````ts
    if (ed.store.state.commands['edit.paste']?.enabled === false) return;
    const mode: PasteModeDto = plainNext && !isTextField(e.target) ? 'over' : 'normal';
    // Копия свойств вставляется только «Вставить свойства»: обычной вставкой — ничего.
    if (isPropsText(e.clipboardData?.getData('text/plain') ?? '')) return;
    const sync = fromDataTransfer(e.clipboardData);
    if (sync) void deliver(sync, mode);
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
    else if (e.type === 'clipboardPng') void copyPng(e.bytes, sink);
    else if (e.type === 'clipboardSvg') void copySvg(e.bytes, sink);
    else if (e.type === 'pasteRequested') void readAsync(false).then((d) => deliver(d, e.mode));
    else if (e.type === 'requestImage') {
````

на:

````ts
    else if (e.type === 'clipboardPng') void copyPng(e.bytes, sink);
    else if (e.type === 'clipboardSvg') void copySvg(e.bytes, sink);
    else if (e.type === 'clipboardProps') writeProps(e.text);
    else if (e.type === 'pasteRequested' && e.mode === 'props') void readProps().then((p) => ed.send({ type: 'pasteProps', ...p }));
    else if (e.type === 'pasteRequested') void readAsync(false).then((d) => deliver(d, e.mode));
    else if (e.type === 'requestImage') {
````

В `crates/ferrum-doc/src/vector.rs` заменить:

````rust
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
````

на:

````rust
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// base64 (стандартный алфавит, с `=`) — для текста в буфере обмена.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
````

В `crates/ferrum-doc/src/vector.rs` заменить:

````rust
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.as_bytes();
    if !s.len().is_multiple_of(4) {
````

на:

````rust
}

/// Обратно из base64; `None` — не base64 (длина не кратна 4, чужой знак).
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.as_bytes();
    if !s.len().is_multiple_of(4) {
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
    /// «Вставить с заменой» (Ctrl/Cmd+Shift+R): на место каждого выделенного узла.
    Replace,
}

````

на:

````rust
    /// «Вставить с заменой» (Ctrl/Cmd+Shift+R): на место каждого выделенного узла.
    Replace,
    /// «Вставить свойства» (Ctrl/Cmd+Alt+V): интерфейс читает буфер и зовёт
    /// `paste_props`, а не `paste_with`.
    Props,
}

````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
                .events
                .push(EditorEvent::PasteRequested(PasteMode::Replace)),
            C::CopyAsPng => {
                let ids = self.ordered_selection();
````

на:

````rust
                .events
                .push(EditorEvent::PasteRequested(PasteMode::Replace)),
            C::CopyProps => self.copy_props(),
            C::PasteProps => self
                .out
                .events
                .push(EditorEvent::PasteRequested(PasteMode::Props)),
            C::CopyAsPng => {
                let ids = self.ordered_selection();
````

В `crates/ferrum-editor/src/command_state.rs` заменить:

````rust
            // Заменять нечего без выделения.
            C::PasteReplace => (edit && sel, None),
            C::CopyAsPng | C::CopyAsSvg | C::ZoomToSelection => (sel, None),
            // В правке вектора удаление и сдвиг — у точек.
            C::Delete
````

на:

````rust
            // Заменять нечего без выделения.
            C::PasteReplace => (edit && sel, None),
            C::CopyAsPng | C::CopyAsSvg | C::ZoomToSelection | C::CopyProps => (sel, None),
            // В правке текста сочетания — у текста; свойства — к узлам вне неё.
            C::PasteProps => (edit && sel && !text, None),
            // В правке вектора удаление и сдвиг — у точек.
            C::Delete
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    PasteReplace,
    CopyAsPng,
    /// «Копировать как SVG» (подсистема 3, этап 5): без сочетания, как в Figma.
    CopyAsSvg,
````

на:

````rust
    PasteReplace,
    CopyAsPng,
    /// «Копировать свойства» и «Вставить свойства» (Ctrl/Cmd+Alt+C и V, как в
    /// Figma): заливки, обводки, прозрачность, скругление, стиль текста.
    CopyProps,
    PasteProps,
    /// «Копировать как SVG» (подсистема 3, этап 5): без сочетания, как в Figma.
    CopyAsSvg,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        &["картинка", "растр", "copy as png", "image"],
        &["Mod+Shift+KeyC"],
    ),
    cmd(
````

на:

````rust
        &["картинка", "растр", "copy as png", "image"],
        &["Mod+Shift+KeyC"],
    ),
    cmd(
        C::CopyProps,
        "edit.copy_props",
        G::Edit,
        "Копировать свойства",
        &["стиль", "оформление", "copy properties", "style"],
        &["Mod+Alt+KeyC"],
    ),
    cmd(
        C::PasteProps,
        "edit.paste_props",
        G::Edit,
        "Вставить свойства",
        &["стиль", "оформление", "paste properties", "style"],
        &["Mod+Alt+KeyV"],
    ),
    cmd(
````

В `crates/ferrum-editor/src/context_menu.rs` заменить:

````rust
            I(C::PasteReplace),
            sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
        ];
        if let Some(under) = under.filter(|u| !u.is_empty()) {
````

на:

````rust
            I(C::PasteReplace),
            sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
            S,
            I(C::CopyProps),
            I(C::PasteProps),
        ];
        if let Some(under) = under.filter(|u| !u.is_empty()) {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        plain: String,
    },
    /// Интерфейсу пора прочитать системный буфер и вызвать `paste_with` с этим
    /// способом вставки.
````

на:

````rust
        plain: String,
    },
    /// «Копировать свойства»: текст с меткой для `text/plain` системного буфера.
    ClipboardProps(String),
    /// Интерфейсу пора прочитать системный буфер и вызвать `paste_with` с этим
    /// способом вставки.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// содержимого встаёт на +10, +10 от неё (спека подсистемы 2, раздел 6).
    pub(crate) last_paste: Option<([u8; 32], Vec2)>,
}

````

на:

````rust
    /// содержимого встаёт на +10, +10 от неё (спека подсистемы 2, раздел 6).
    pub(crate) last_paste: Option<([u8; 32], Vec2)>,
    /// Внутренний буфер «Копировать свойства».
    pub(crate) props_clip: Option<ferrum_doc::Props>,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            states: StatesCache::default(),
            last_paste: None,
        })
    }
````

на:

````rust
            states: StatesCache::default(),
            last_paste: None,
            props_clip: None,
        })
    }
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod panel;
pub mod presets;
pub mod scale;
pub mod selection;
````

на:

````rust
pub mod panel;
pub mod presets;
pub mod props_clip;
pub mod scale;
pub mod selection;
````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
                sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
                S,
                I(C::Duplicate),
                I(C::Delete),
````

на:

````rust
                sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
                S,
                I(C::CopyProps),
                I(C::PasteProps),
                S,
                I(C::Duplicate),
                I(C::Delete),
````

Создать `crates/ferrum-editor/src/props_clip.rs`:

````rust
//! «Копировать свойства» и «Вставить свойства» (спека подсистемы 2, раздел 6):
//! заливки, обводки, стиль обводки, прозрачность, скругление и стиль текста.
//! Копия кладётся во внутренний буфер редактора и в системный — текстом с
//! меткой `PROPS_MARKER`: base64 того же формата, что у копии узлов (один узел
//! без детей и с теми изображениями, на которые ссылаются его заливки). Вставка
//! берёт свойства из системного буфера — копии свойств или копии узлов (свойства
//! её первого узла, как в Figma), — а без них из внутреннего буфера и применяет к
//! каждому выделенному то, что подходит его типу, одним шагом. Эффектов и режимов
//! наложения в модели пока нет.

use std::sync::Arc;

use ferrum_doc::{
    base64_decode, base64_encode, NodeId, NodeKind, Props, PropsPatch, TextContent, TextData,
};
use kurbo::Rect;

use crate::clipboard::{encodable, ClipNode, ClipPayload, ClipboardError};
use crate::editor::{Editor, EditorEvent};
use crate::panel::{field_applies, NumField};
use crate::text::TextError;
use crate::text_style::TextStylePatch;

/// Метка копии свойств в `text/plain`.
pub const PROPS_MARKER: &str = "ferrum-props:v1:";
/// Метка копии узлов текстом (запасной путь буфера обмена интерфейса).
pub const NODES_MARKER: &str = "ferrum-clipboard:v1:";
/// Предел текста свойств или узлов из буфера: base64 предела вставки.
pub const MAX_PROPS_TEXT: usize = crate::clipboard::MAX_PAYLOAD / 3 * 4 + 64;

/// Нечего вставить: свойства не копировали.
pub const NO_PROPS: &str = "Сначала скопируйте свойства";

/// Свойства для буфера: без геометрии, имени, детей и сети; у текста — стиль
/// первого участка как стиль узла и пустое содержимое (переносится только стиль).
fn strip(p: &Props) -> Props {
    let mut out = Props::defaults(p.kind);
    out.opacity = p.opacity;
    out.corner_radius = p.corner_radius;
    out.fills = p.fills.clone();
    out.strokes = p.strokes.clone();
    out.stroke_style = p.stroke_style;
    out.boolean_op = p.boolean_op;
    if let Some(t) = &p.text {
        let mut props = t.props.clone();
        let d = props.style.clone();
        if let Some(a) = t.content.runs.first().map(|r| &r.attrs) {
            props.style.font = a.font_name(&d);
            props.style.size = a.size.unwrap_or(d.size);
            props.style.line_height = a.line_height.unwrap_or(d.line_height);
            props.style.letter_spacing = a.letter_spacing.unwrap_or(d.letter_spacing);
            props.style.decoration = a.decoration.unwrap_or(d.decoration);
            props.style.case = a.case.unwrap_or(d.case);
        }
        out.text = Some(Arc::new(TextData {
            props,
            content: TextContent::plain(""),
        }));
    }
    out
}

/// Есть ли у узла этого типа заливки и обводки (и стиль обводки).
fn has_fills(kind: NodeKind) -> bool {
    !matches!(kind, NodeKind::Group | NodeKind::Line)
}

fn has_strokes(kind: NodeKind) -> bool {
    kind != NodeKind::Group
}

fn has_stroke_style(kind: NodeKind) -> bool {
    !matches!(kind, NodeKind::Group | NodeKind::Text)
}

/// Правка узла типа `kind` свойствами `src`: только то, что есть у обоих типов.
pub(crate) fn props_patch(src: &Props, kind: NodeKind) -> PropsPatch {
    let mut p = PropsPatch {
        opacity: Some(src.opacity),
        ..Default::default()
    };
    if has_fills(src.kind) && has_fills(kind) {
        p.fills = Some(src.fills.clone());
    }
    if has_strokes(src.kind) && has_strokes(kind) {
        p.strokes = Some(src.strokes.clone());
    }
    if has_stroke_style(src.kind) && has_stroke_style(kind) {
        let s = src.stroke_style;
        p.stroke_cap_start = Some(s.cap_start);
        p.stroke_cap_end = Some(s.cap_end);
        p.stroke_join = Some(s.join);
        p.stroke_miter_limit = Some(s.miter_limit);
    }
    if field_applies(NumField::CornerRadius, src.kind)
        && field_applies(NumField::CornerRadius, kind)
    {
        p.corner_radius = Some(src.corner_radius);
    }
    p
}

/// Стиль текста-источника для всего текста цели.
fn text_patch(src: &Props) -> Option<TextStylePatch> {
    let s = &src.text.as_ref()?.props.style;
    Some(TextStylePatch {
        font: Some(s.font.clone()),
        size: Some(s.size),
        line_height: Some(s.line_height),
        letter_spacing: Some(s.letter_spacing),
        decoration: Some(s.decoration),
        case: Some(s.case),
        ..Default::default()
    })
}

/// Байты копии из текста буфера: метка свойств или узлов и base64; иначе `None`.
pub fn props_text_bytes(text: &str) -> Option<Vec<u8>> {
    if text.len() > MAX_PROPS_TEXT {
        return None;
    }
    let body = text
        .strip_prefix(PROPS_MARKER)
        .or_else(|| text.strip_prefix(NODES_MARKER))?;
    base64_decode(body.trim())
}

impl Editor {
    /// «Копировать свойства»: свойства первого выделенного — во внутренний буфер и
    /// текстом с меткой для системного (событие `ClipboardProps`).
    pub(crate) fn copy_props(&mut self) {
        let Some(&id) = self.selection.first() else {
            return;
        };
        let Some(props) = self.doc.page(self.page).ok().and_then(|p| p.props(id).ok()) else {
            return;
        };
        let props = strip(&props);
        let images: Vec<(String, Vec<u8>)> = {
            let mut hashes: Vec<&String> = props
                .fills
                .iter()
                .filter_map(|f| f.image_hash.as_ref())
                .collect();
            hashes.sort();
            hashes.dedup();
            hashes
                .into_iter()
                .filter_map(|h| Some((h.clone(), self.doc.images.get(h)?.to_vec())))
                .collect()
        };
        let payload = ClipPayload {
            nodes: vec![ClipNode {
                parent: None,
                props: props.clone(),
            }],
            images,
            bounds: Rect::ZERO,
        };
        if let Err(e) = encodable(&payload) {
            self.out.events.push(EditorEvent::Error(e.to_string()));
            return;
        }
        self.props_clip = Some(props);
        let text = format!("{PROPS_MARKER}{}", base64_encode(&payload.encode()));
        self.out.events.push(EditorEvent::ClipboardProps(text));
    }

    /// «Вставить свойства» к выделению одним шагом. `clip` — байты копии из
    /// системного буфера (свойств или узлов: берётся первый узел), без них —
    /// внутренний буфер. Данные не доверенные: при ошибке документ не меняется,
    /// причина — событием `Error`. Шрифт стиля текста ещё грузится — ничего не
    /// сделано, `take_text_pending` вернёт `true` (ядро повторит).
    pub fn paste_props(&mut self, clip: Option<&[u8]>) {
        self.begin_doc_edit();
        let (src, images) = match clip {
            Some(bytes) => match ClipPayload::decode(bytes) {
                Ok(p) => {
                    let first = p.nodes.into_iter().next().map(|n| strip(&n.props));
                    (first, p.images)
                }
                Err(e) => {
                    self.out.events.push(EditorEvent::Error(e.to_string()));
                    self.emit();
                    return;
                }
            },
            None => (self.props_clip.clone(), Vec::new()),
        };
        let Some(src) = src else {
            let message = match clip {
                Some(_) => ClipboardError::Empty.to_string(),
                None => NO_PROPS.to_string(),
            };
            self.out.events.push(EditorEvent::Error(message));
            self.emit();
            return;
        };
        let ids: Vec<NodeId> = self.alive(&self.selection);
        let extra: Vec<(NodeId, PropsPatch)> = ids
            .iter()
            .filter_map(|id| Some((*id, props_patch(&src, self.scene.node(*id)?.props.kind))))
            .collect();
        let texts: Vec<NodeId> = match text_patch(&src) {
            Some(_) => ids
                .iter()
                .copied()
                .filter(|id| {
                    self.scene
                        .node(*id)
                        .is_some_and(|n| n.props.kind == NodeKind::Text)
                })
                .collect(),
            None => Vec::new(),
        };
        let before = self.out.steps;
        let patch = text_patch(&src).unwrap_or_default();
        match self.restyle_nodes_with(&texts, &patch, extra) {
            Ok(()) => {}
            Err(TextError::Pending) => {
                self.out.text_pending = true;
                return;
            }
            // Отказ документа `commit` уже отдал событием.
            Err(TextError::Doc(_)) => return,
        }
        if self.out.steps != before {
            // Изображения заливок — в документ, когда свойства легли.
            for (hash, data) in images {
                if self.doc.images.get(&hash).is_none() {
                    self.doc.images.add(data);
                }
            }
        }
    }
}
````

В `crates/ferrum-editor/src/text_style.rs` заменить:

````rust
    }

    /// Превью или запись правок текстов: свойства, содержимое и размер рамки.
    fn apply_text_changes(
````

на:

````rust
    }

    /// Стиль всего текста узлов `ids` (не тексты пропускаются) и правки `extra`
    /// других свойств (заливки, обводки…) — одной записью, одним шагом отмены:
    /// «Вставить свойства» (спека подсистемы 2, раздел 6). Шрифт грузится —
    /// `Err(Pending)`, документ не тронут.
    pub(crate) fn restyle_nodes_with(
        &mut self,
        ids: &[NodeId],
        patch: &TextStylePatch,
        extra: Vec<(NodeId, PropsPatch)>,
    ) -> Result<(), TextError> {
        let mut edits = Vec::new();
        for id in ids {
            let Some(before) = self.doc_props(*id) else {
                continue;
            };
            let Some(t) = before.text.as_ref() else {
                continue;
            };
            let mut p = t.props.clone();
            p.style = apply_style(&p.style, patch);
            let content = restyle(&t.content, 0..t.content.char_len(), patch, &p.style);
            let mut after = before.clone();
            after.text = Some(Arc::new(TextData { props: p, content }));
            edits.push((*id, before, after, None));
        }
        self.apply_text_changes_with(edits, Phase::Commit, patch, extra)
    }

    /// Превью или запись правок текстов: свойства, содержимое и размер рамки.
    fn apply_text_changes(
````

В `crates/ferrum-editor/src/text_style.rs` заменить:

````rust
        patch: &TextStylePatch,
    ) -> Result<(), TextError> {
        let mut patches: Vec<(NodeId, PropsPatch, Props, Option<Range<usize>>)> = Vec::new();
        let mut pending = false;
````

на:

````rust
        patch: &TextStylePatch,
    ) -> Result<(), TextError> {
        self.apply_text_changes_with(edits, phase, patch, Vec::new())
    }

    /// `apply_text_changes` и правки `extra` других свойств узлов той же записью
    /// (только при `Commit`; черновику и превью их не передают).
    fn apply_text_changes_with(
        &mut self,
        edits: Vec<(NodeId, Props, Props, Option<Range<usize>>)>,
        phase: Phase,
        patch: &TextStylePatch,
        extra: Vec<(NodeId, PropsPatch)>,
    ) -> Result<(), TextError> {
        debug_assert!(extra.is_empty() || phase == Phase::Commit);
        let mut patches: Vec<(NodeId, PropsPatch, Props, Option<Range<usize>>)> = Vec::new();
        let mut pending = false;
````

В `crates/ferrum-editor/src/text_style.rs` заменить:

````rust
                    return Err(TextError::Pending);
                }
                if patches.is_empty() {
                    self.emit();
                    return Ok(());
````

на:

````rust
                    return Err(TextError::Pending);
                }
                if patches.is_empty() && extra.is_empty() {
                    self.emit();
                    return Ok(());
````

В `crates/ferrum-editor/src/text_style.rs` заменить:

````rust
                // Отказ документа `commit` сам отдаёт событием `Error`.
                self.ctx().commit(|tx, _| {
                    for (id, p, before, range) in &patches {
                        let t = before.text.as_ref().expect("текст");
````

на:

````rust
                // Отказ документа `commit` сам отдаёт событием `Error`.
                self.ctx().commit(|tx, _| {
                    for (id, p) in &extra {
                        tx.set_props(*id, p)?;
                    }
                    for (id, p, before, range) in &patches {
                        let t = before.text.as_ref().expect("текст");
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        let paste = match cmd {
            Command::Paste { .. } | Command::PasteImage { .. } => true,
            Command::TextInput {
                kind: TextInputKindDto::Paste,
````

на:

````rust
        let paste = match cmd {
            Command::Paste { .. } | Command::PasteImage { .. } => true,
            Command::PasteProps { .. } => {
                return !self.editor.command_state(CommandId::PasteProps).enabled;
            }
            Command::TextInput {
                kind: TextInputKindDto::Paste,
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                self.editor.take_text_pending();
                let how = match mode.unwrap_or_default() {
                    PasteModeDto::Normal => PasteMode::Normal,
                    PasteModeDto::Over => PasteMode::Over,
                    PasteModeDto::Replace => PasteMode::Replace,
````

на:

````rust
                self.editor.take_text_pending();
                let how = match mode.unwrap_or_default() {
                    // Свойства приходят командой `PasteProps`; здесь — как обычная.
                    PasteModeDto::Normal | PasteModeDto::Props => PasteMode::Normal,
                    PasteModeDto::Over => PasteMode::Over,
                    PasteModeDto::Replace => PasteMode::Replace,
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                if self.editor.take_text_pending() {
                    self.wait(fonts::Waiting::Command(Command::Paste { bytes, mode }));
                }
            }
````

на:

````rust
                if self.editor.take_text_pending() {
                    self.wait(fonts::Waiting::Command(Command::Paste { bytes, mode }));
                }
            }
            Command::PasteProps { bytes, text } => {
                self.editor.take_text_pending();
                // Текст из буфера — не доверенный: чужой — внутренний буфер ядра.
                let clip = match (&bytes, &text) {
                    (Some(b), _) => Some(b.to_vec()),
                    (None, Some(t)) => ferrum_editor::props_clip::props_text_bytes(t),
                    (None, None) => None,
                };
                self.editor.paste_props(clip.as_deref());
                // Шрифт стиля текста грузится: повторить по приходу.
                if self.editor.take_text_pending() {
                    self.wait(fonts::Waiting::Command(Command::PasteProps { bytes, text }));
                }
            }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                        PasteMode::Over => PasteModeDto::Over,
                        PasteMode::Replace => PasteModeDto::Replace,
                    },
                }),
                EditorEvent::ExportPng(ids) => self.copy_png(&ids),
                EditorEvent::ExportSvg(ids) => self.copy_svg(&ids),
````

на:

````rust
                        PasteMode::Over => PasteModeDto::Over,
                        PasteMode::Replace => PasteModeDto::Replace,
                        PasteMode::Props => PasteModeDto::Props,
                    },
                }),
                EditorEvent::ClipboardProps(text) => {
                    self.events.push(CoreEvent::ClipboardProps { text })
                }
                EditorEvent::ExportPng(ids) => self.copy_png(&ids),
                EditorEvent::ExportSvg(ids) => self.copy_svg(&ids),
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            | Command::SetTheme { .. }
            | Command::Paste { .. }
            | Command::PlaceImage { .. }
            | Command::Save
````

на:

````rust
            | Command::SetTheme { .. }
            | Command::Paste { .. }
            | Command::PasteProps { .. }
            | Command::PlaceImage { .. }
            | Command::Save
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust

/// Способ вставки (спека подсистемы 2, раздел 6): обычная, поверх (на исходные
/// координаты), с заменой выделенного.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
````

на:

````rust

/// Способ вставки (спека подсистемы 2, раздел 6): обычная, поверх (на исходные
/// координаты), с заменой выделенного; `props` — «Вставить свойства» (ответ —
/// команда `PasteProps`, а не `Paste`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    Over,
    Replace,
}

````

на:

````rust
    Over,
    Replace,
    Props,
}

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        mode: Option<PasteModeDto>,
    },
    /// Разместить изображение (PNG): следующий клик ставит его на холст.
    PlaceImage {
````

на:

````rust
        mode: Option<PasteModeDto>,
    },
    /// «Вставить свойства» к выделению (спека подсистемы 2, раздел 6): из
    /// системного буфера — байты копии Ферума (`web application/x-ferrum`) или
    /// текст с меткой свойств или узлов; без них — из внутреннего буфера ядра.
    PasteProps {
        #[serde(default)]
        #[tsify(type = "Uint8Array | undefined")]
        bytes: Option<ByteBuf>,
        #[serde(default)]
        #[tsify(optional)]
        text: Option<String>,
    },
    /// Разместить изображение (PNG): следующий клик ставит его на холст.
    PlaceImage {
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        plain: Option<String>,
    },
    /// PNG выделения для буфера обмена («Скопировать как PNG»).
    ClipboardPng {
````

на:

````rust
        plain: Option<String>,
    },
    /// «Копировать свойства»: текст с меткой для `text/plain` системного буфера.
    ClipboardProps {
        text: String,
    },
    /// PNG выделения для буфера обмена («Скопировать как PNG»).
    ClipboardPng {
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/paste.spec.ts e2e/app.spec.ts e2e/chrome.spec.ts e2e/context-menu.spec.ts e2e/main-menu.spec.ts e2e/clipboard.spec.ts --project=chromium`

Expected: PASS: fmt и clippy без замечаний; `ferrum-doc`, `ferrum-editor`, `ferrum-wasm` без падений (`props_clip` — 6 тестов, `paste` — 2; команд 84); модульные (`clipboard` — 7, `messages` — 6) и сборка — ок; E2E `paste`, `app`, `chrome`, `context-menu`, `main-menu`, `clipboard` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts apps/web/e2e/chrome.spec.ts apps/web/e2e/context-menu.spec.ts apps/web/e2e/main-menu.spec.ts apps/web/e2e/paste.spec.ts apps/web/src/core/clipboard-text.ts apps/web/src/core/messages.ts apps/web/src/ui/clipboard.ts apps/web/src/ui/system-clipboard.ts apps/web/unit/clipboard.test.ts apps/web/unit/messages.test.ts crates/ferrum-doc/src/vector.rs crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/command_state.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/context_menu.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/menu.rs crates/ferrum-editor/src/props_clip.rs crates/ferrum-editor/src/text_style.rs crates/ferrum-editor/tests/context_menu.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/main_menu.rs crates/ferrum-editor/tests/props_clip.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/boolean.rs crates/ferrum-wasm/tests/paste.rs crates/ferrum-wasm/tests/protocol.rs
git commit -F - <<'EOF'
feat: «Копировать свойства» и «Вставить свойства» (Ctrl/Cmd+Alt+C и V)

Копия свойств первого выделенного — во внутренний буфер редактора и текстом
с меткой ferrum-props:v1: (base64 формата буфера: один узел без геометрии и
детей, с изображениями своих заливок) в системный буфер (ClipboardProps).
«Вставить свойства» просит буфер (PasteRequested { mode: props }) и получает
PasteProps { bytes | text }: копия свойств или копия узлов (свойства первого
узла, как в Figma), а без них — внутренний буфер. К каждому выделенному —
то, что подходит его типу (заливки, обводки и стиль обводки, прозрачность,
скругление; тексту — стиль первого участка источника с подгонкой рамки),
одной записью: restyle_nodes_with и правки свойств в одном шаге. Копия
свойств обычной вставкой не вставляется. Пункты — в «Правке» и в меню
холста; число команд — 84.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 6: Умный дубликат и «Выделить подобные» (Ctrl/Cmd+Alt+A), замер на 100 000 узлах

Спека §6: Ctrl/Cmd+D помнит оригиналы, копии и версию выделения — если выделены копии прошлого дубликата и выделение не менялось (копии только двигали), новый дубликат сдвигается так же от них. «Выделить подобные» (`edit.select_similar`) — видимые и не заблокированные узлы страницы того же типа с теми же заливками и обводками, что у выделенных (до 64 образцов), один обход в worker'е. В «Правке» главного меню; строка `editor_report` (цель ≤ 100 мс на 100 000 узлах); команд 84 → 85.

**Files:**
- Modify: `crates/ferrum-editor/src/{actions,commands,command_state,editor,menu,lib}.rs`, `crates/ferrum-editor/examples/editor_report.rs`
- Test: `crates/ferrum-editor/tests/smart_duplicate.rs` (новый), `crates/ferrum-editor/tests/{keymap,main_menu}.rs`, `crates/ferrum-wasm/tests/boolean.rs`, `apps/web/e2e/duplicate.spec.ts` (новый), `apps/web/e2e/app.spec.ts`

**Interfaces:**
- Consumes: дубликат (`ops.rs`), версия выделения, обход сцены страницы, `editor_report` (`Report`).
- Produces: `CommandId::SelectSimilar` (`edit.select_similar`, Mod+Alt+A); `pub const MAX_SIMILAR_SAMPLES: usize = 64`; `Editor.last_duplicate: Option<Duplicated { sources, copies, version }>`; `smart_duplicate()`, `select_similar()`; строка `editor_report` «„Выделить подобные“, 100 000 узлов» (`select_similar(&mut report)`).

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 84, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 85, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

Создать `apps/web/e2e/duplicate.spec.ts`:

````ts
// Умный дубликат и «Выделить подобные» (спека подсистемы 2, раздел 6): после
// Ctrl/Cmd+D и сдвига копии следующий Ctrl/Cmd+D повторяет сдвиг; «Выделить
// подобные» — узлы страницы того же типа с теми же заливками и обводками.

import { expect, test } from '@playwright/test';
import { appScreen, drag, live, liveWhen, mainMenu, openApp } from './helpers';

test('Ctrl/Cmd+D после сдвига копии повторяет тот же сдвиг', async ({ page }) => {
  await openApp(page);
  // «Слой 1» в «Экран 0»: мир (140, 0) — (260, 80).
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionX === 140');
  await page.keyboard.press('ControlOrMeta+KeyD');
  await liveWhen(page, `l.nodes === ${l.nodes + 1} && l.selectionX === 140`);
  // Копию — стрелками: Shift+→ дважды и Shift+↓ (20, 10).
  for (const key of ['Shift+ArrowRight', 'Shift+ArrowRight', 'Shift+ArrowDown']) await page.keyboard.press(key);
  await liveWhen(page, 'l.selectionX === 160 && l.selectionY === 10');
  await page.keyboard.press('ControlOrMeta+KeyD');
  await liveWhen(page, `l.nodes === ${l.nodes + 2} && l.selectionX === 180 && l.selectionY === 20`);
  await page.keyboard.press('ControlOrMeta+KeyD');
  await liveWhen(page, `l.nodes === ${l.nodes + 3} && l.selectionX === 200 && l.selectionY === 30`);
  // Протяжкой — тоже сдвиг: следующий дубликат повторяет шаг от прошлой копии
  // (180, 20) до этой, где бы её ни оставила привязка.
  const copy = await appScreen(page, await live(page), 300, 90);
  await drag(page, copy, 0, 40);
  const moved = await liveWhen(page, `l.steps === ${l.steps + 7}`);
  expect(moved.selectionY).toBeGreaterThan(30);
  await page.keyboard.press('ControlOrMeta+KeyD');
  await liveWhen(
    page,
    `l.nodes === ${l.nodes + 4} && l.selectionX === ${2 * moved.selectionX - 180} && l.selectionY === ${2 * moved.selectionY - 20}`,
  );
});

test('«Выделить подобные» из меню и Ctrl/Cmd+Alt+A', async ({ page }) => {
  await openApp(page);
  const at = await appScreen(page, await live(page), 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  // Двойник «Слоя 1» — в каждом из 10 фреймов.
  await mainMenu(page, 'Правка', 'Выделить подобные');
  await liveWhen(page, 'l.selectionCount === 10');
  // Снова один «Слой 1»: щелчок мимо фреймов снимает выделение.
  const empty = await appScreen(page, await live(page), 100, 1080);
  await page.mouse.click(empty.x, empty.y);
  await liveWhen(page, 'l.selectionCount === 0');
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.keyboard.press('ControlOrMeta+Alt+KeyA');
  await liveWhen(page, 'l.selectionCount === 10');
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    (CommandId::Redo, &["Mod+Shift+KeyZ"]),
    (CommandId::SelectAll, &["Mod+KeyA"]),
    (CommandId::Escape, &["Escape"]),
    // Подсистема 2: главное меню — Alt+/ (в Figma поиск в меню — по логотипу);
````

на:

````rust
    (CommandId::Redo, &["Mod+Shift+KeyZ"]),
    (CommandId::SelectAll, &["Mod+KeyA"]),
    (CommandId::SelectSimilar, &["Mod+Alt+KeyA"]),
    (CommandId::Escape, &["Escape"]),
    // Подсистема 2: главное меню — Alt+/ (в Figma поиск в меню — по логотипу);
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 84);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 85);
}

````

В `crates/ferrum-editor/tests/main_menu.rs` заменить:

````rust
        "edit.duplicate",
        "edit.select_all",
    ] {
        assert!(keys(edit).contains(&k), "{k}");
````

на:

````rust
        "edit.duplicate",
        "edit.select_all",
        "edit.select_similar",
    ] {
        assert!(keys(edit).contains(&k), "{k}");
````

Создать `crates/ferrum-editor/tests/smart_duplicate.rs`:

````rust
//! Умный дубликат и «Выделить подобные» (спека подсистемы 2, раздел 6): после
//! Ctrl/Cmd+D и сдвига копии следующий Ctrl/Cmd+D повторяет тот же сдвиг, пока
//! выделение не менялось; «Выделить подобные» — тот же тип, заливки и обводки.

mod common;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn dup(h: &mut H) -> Vec<NodeId> {
    h.key("KeyD", ctrl());
    h.sel()
}

fn xy(h: &mut H, id: NodeId) -> (f64, f64) {
    let p = h.props(id);
    (p.x, p.y)
}

#[test]
fn duplicate_repeats_the_move_of_the_copy() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    let c1 = dup(&mut h);
    assert_eq!(xy(&mut h, c1[0]), (100.0, 100.0), "первая копия — на месте");
    // Копию сдвинули протяжкой на (30, 20).
    h.drag((120.0, 120.0), (150.0, 140.0), Modifiers::NONE);
    assert_eq!(xy(&mut h, c1[0]), (130.0, 120.0));
    let c2 = dup(&mut h);
    assert_eq!(xy(&mut h, c2[0]), (160.0, 140.0), "тот же сдвиг");
    let c3 = dup(&mut h);
    assert_eq!(xy(&mut h, c3[0]), (190.0, 160.0), "и снова");
    assert_eq!(h.children(None).len(), 4);
}

#[test]
fn nudge_counts_as_a_move_and_other_selection_resets() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    let c1 = dup(&mut h);
    h.key("ArrowRight", Modifiers::SHIFT);
    let c2 = dup(&mut h);
    assert_eq!(xy(&mut h, c2[0]), (120.0, 100.0));
    // Другое выделение и обратно: шаг забыт — копия на месте.
    h.ed.select(&[r]);
    h.ed.select(&c2);
    let c3 = dup(&mut h);
    assert_eq!(xy(&mut h, c3[0]), (120.0, 100.0));
    // Отмена и повтор сдвига копии выделения не меняют: шаг — по итогу сдвига.
    let _ = c1;
    h.key("ArrowDown", Modifiers::SHIFT);
    h.ed.execute(CommandId::Undo);
    h.ed.execute(CommandId::Redo);
    let c4 = dup(&mut h);
    assert_eq!(xy(&mut h, c4[0]), (120.0, 120.0));
    // Отмена дубликата возвращает прежнее выделение: шаг забыт.
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.sel(), c3);
    h.ed.select(&[r]);
    h.ed.select(&c3);
    let c5 = dup(&mut h);
    assert_eq!(xy(&mut h, c5[0]), (120.0, 110.0));
}

#[test]
fn several_nodes_keep_their_layout() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    let b = h.rect(None, 200.0, 100.0, 40.0, 40.0);
    h.ed.select(&[b, a]);
    dup(&mut h);
    h.key("ArrowDown", Modifiers::SHIFT);
    let copies = dup(&mut h);
    let mut at: Vec<(f64, f64)> = copies.iter().map(|id| xy(&mut h, *id)).collect();
    at.sort_by(|p, q| p.0.total_cmp(&q.0));
    assert_eq!(at, [(100.0, 120.0), (200.0, 120.0)]);
}

fn fill(r: u8) -> Vec<Fill> {
    vec![Fill::solid(Color::rgba(r, 0, 0, 255))]
}

#[test]
fn select_similar_matches_type_fills_and_strokes_on_the_page() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 500.0, 500.0);
    let mk = |h: &mut H, parent, kind, r: u8| {
        let id = h.add(parent, kind, patch(10.0, 10.0, 20.0, 20.0));
        h.set(
            id,
            PropsPatch {
                fills: Some(fill(r)),
                ..Default::default()
            },
        );
        id
    };
    let a = mk(&mut h, None, NodeKind::Rect, 1);
    let b = mk(&mut h, Some(f), NodeKind::Rect, 1);
    let other_fill = mk(&mut h, None, NodeKind::Rect, 2);
    let ellipse = mk(&mut h, None, NodeKind::Ellipse, 1);
    let hidden = mk(&mut h, None, NodeKind::Rect, 1);
    h.set(
        hidden,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    let stroked = mk(&mut h, None, NodeKind::Rect, 1);
    h.set(
        stroked,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 1.0,
                align: StrokeAlign::Inside,
            }]),
            ..Default::default()
        },
    );
    assert!(!h.ed.command_state(CommandId::SelectSimilar).enabled);
    h.ed.select(&[a]);
    let steps = h.ed.step_count();
    h.ed.execute(CommandId::SelectSimilar);
    // Порядок обхода страницы: фрейм (его ребёнок), затем корни по порядку.
    assert_eq!(h.sel(), [b, a]);
    assert_eq!(h.ed.step_count(), steps, "выбор — не шаг истории");
    // Образцы — все выделенные: эллипс добавляет свой вид.
    h.ed.select(&[a, ellipse]);
    h.ed.execute(CommandId::SelectSimilar);
    assert_eq!(h.sel().len(), 3);
    assert!(!h.sel().contains(&other_fill) && !h.sel().contains(&stroked));
    // Совпавший контейнер выделяется без потомков.
    let g = h.add(None, NodeKind::Group, PropsPatch::default());
    let g2 = h.add(None, NodeKind::Group, PropsPatch::default());
    h.ed.select(&[g]);
    h.ed.execute(CommandId::SelectSimilar);
    assert_eq!(h.sel(), [g, g2]);
}
````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 84);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

на:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 85);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test smart_duplicate --test keymap --test main_menu`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test boolean`
Expected: FAIL — не собирается: редактор — нет `CommandId::SelectSimilar`, `MAX_SIMILAR_SAMPLES` (`smart_duplicate`, `keymap`, `main_menu`); ядро собирается, но тест числа команд в `boolean.rs` падает (`left: 84, right: 85`)

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
    scale_selection(&mut report);
    command_states(&mut report);

    println!("| Метрика | Значение | Цель | Итог |");
````

на:

````rust
    scale_selection(&mut report);
    command_states(&mut report);
    select_similar(&mut report);

    println!("| Метрика | Значение | Цель | Итог |");
````

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
        "Состояния команд после шага, выделено всё — 100 000 (медиана)",
        ms(median(whole)),
    );
}
````

на:

````rust
        "Состояния команд после шага, выделено всё — 100 000 (медиана)",
        ms(median(whole)),
    );
}

/// «Выделить подобные» (подсистема 2, этап 2): на странице из 100 000 узлов
/// выделен один прямоугольник — совпадают его двойники в каждом фрейме (тот же
/// тип, заливка и обводка: 1000 узлов). Цель — 100 мс на весь обход страницы.
fn select_similar(report: &mut Report) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids: Vec<NodeId> = doc.transact(page, vec![], |p| fill(p, NODES)).unwrap();
    let mut ed = Editor::new(doc, Some(page), VIEWPORT, 1.0, Platform::Mac).unwrap();
    // «Слой 1» первого фрейма — прямоугольник (каждый третий — эллипс).
    let sample = ids[2];
    let mut runs = Vec::new();
    let mut found = 0;
    for _ in 0..COMMIT_RUNS * 2 {
        ed.select(&[sample]);
        ed.take_events();
        let t = Instant::now();
        ed.execute(CommandId::SelectSimilar);
        runs.push(t.elapsed());
        found = ed.selection().len();
        ed.take_events();
    }
    report.gate_median("«Выделить подобные», 100 000 узлов", &runs, 100.0);
    report.check(
        "«Выделить подобные»: двойник в каждом фрейме",
        found == NODES / 100,
        format!("{found}"),
        &format!("{}", NODES / 100),
    );
}
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
/// раздел 6).
pub const PASTE_STEP: f64 = 10.0;

/// Способ вставки из буфера обмена (спека подсистемы 2, раздел 6).
````

на:

````rust
/// раздел 6).
pub const PASTE_STEP: f64 = 10.0;

/// Сколько разных образцов (тип, заливки, обводки) берёт «Выделить подобные».
pub const MAX_SIMILAR_SAMPLES: usize = 64;

/// Прошлый дубликат: оригиналы и копии по порядку слоёв и версия выделения после
/// него — пока она та же, следующий Ctrl/Cmd+D повторяет сдвиг копий.
#[derive(Debug, Clone)]
pub(crate) struct Duplicated {
    sources: Vec<NodeId>,
    copies: Vec<NodeId>,
    version: u64,
}

/// Способ вставки из буфера обмена (спека подсистемы 2, раздел 6).
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
                }
            }
            C::Duplicate => {
                let ids = top_level(&self.scene, &self.selection);
                if !ids.is_empty() {
                    self.ctx()
                        .commit(|tx, scene| duplicate(tx, scene, &ids, Vec2::ZERO));
                }
            }
            C::Delete => self.delete_selection(),
            C::SelectAll => self.select_all(),
            C::Escape => {
                if self.tool.kind() != ToolKind::Move {
````

на:

````rust
                }
            }
            C::Duplicate => self.smart_duplicate(),
            C::Delete => self.delete_selection(),
            C::SelectAll => self.select_all(),
            C::SelectSimilar => self.select_similar(),
            C::Escape => {
                if self.tool.kind() != ToolKind::Move {
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            .collect();
        self.selection.set(all);
    }

````

на:

````rust
            .collect();
        self.selection.set(all);
    }

    /// Ctrl/Cmd+D, «умный дубликат» (спека подсистемы 2, раздел 6): копии встают на
    /// место оригиналов, а если выделены копии прошлого дубликата (выделение с тех
    /// пор не менялось — их только двигали), то со сдвигом этих копий от их
    /// оригиналов: каждый следующий Ctrl/Cmd+D повторяет тот же шаг.
    fn smart_duplicate(&mut self) {
        let ids = self.ordered_selection();
        if ids.is_empty() {
            return;
        }
        let offset = match &self.last_duplicate {
            Some(d) if d.version == self.selection.version() && d.copies == self.selection[..] => {
                let at = |id: NodeId| self.scene.node(id).map(|n| n.world.translation());
                match (d.copies.first().copied(), d.sources.first().copied()) {
                    (Some(c), Some(s)) => match (at(c), at(s)) {
                        (Some(c), Some(s)) => c - s,
                        _ => Vec2::ZERO,
                    },
                    _ => Vec2::ZERO,
                }
            }
            _ => Vec2::ZERO,
        };
        let ok = self
            .ctx()
            .commit(|tx, scene| duplicate(tx, scene, &ids, offset));
        // Копии — по порядку слоёв, как и оригиналы: пары по номеру.
        self.last_duplicate = ok.then(|| Duplicated {
            sources: ids,
            copies: self.selection.to_vec(),
            version: self.selection.version(),
        });
    }

    /// «Выделить подобные»: видимые и не заблокированные узлы страницы того же
    /// типа с теми же заливками и обводками, что у кого-то из выделенных (образцов
    /// — не больше `MAX_SIMILAR_SAMPLES` разных). Совпавший узел выделяется без
    /// потомков. Обход — один, без рекурсии.
    fn select_similar(&mut self) {
        let mut samples: Vec<(NodeKind, &[Fill], &[ferrum_doc::Stroke])> = Vec::new();
        for id in self.selection.iter() {
            let Some(n) = self.scene.node(*id) else {
                continue;
            };
            let key = (n.props.kind, &n.props.fills[..], &n.props.strokes[..]);
            if !samples.contains(&key) {
                samples.push(key);
                if samples.len() == MAX_SIMILAR_SAMPLES {
                    break;
                }
            }
        }
        let mut found = Vec::new();
        let mut stack: Vec<ferrum_scene::Slot> = self.scene.roots().iter().rev().copied().collect();
        while let Some(s) = stack.pop() {
            if self.scene.is_draft_slot(s) {
                continue;
            }
            let n = self.scene.at(s);
            if !n.props.visible || n.props.locked {
                continue;
            }
            let p = &n.props;
            if samples
                .iter()
                .any(|(k, f, st)| *k == p.kind && *f == &p.fills[..] && *st == &p.strokes[..])
            {
                found.push(n.id);
            } else {
                stack.extend(n.children.iter().rev());
            }
        }
        self.selection.set(found);
    }

````

В `crates/ferrum-editor/src/command_state.rs` заменить:

````rust
            // Заменять нечего без выделения.
            C::PasteReplace => (edit && sel, None),
            C::CopyAsPng | C::CopyAsSvg | C::ZoomToSelection | C::CopyProps => (sel, None),
            // В правке текста сочетания — у текста; свойства — к узлам вне неё.
            C::PasteProps => (edit && sel && !text, None),
````

на:

````rust
            // Заменять нечего без выделения.
            C::PasteReplace => (edit && sel, None),
            C::CopyAsPng | C::CopyAsSvg | C::ZoomToSelection | C::CopyProps | C::SelectSimilar => {
                (sel, None)
            }
            // В правке текста сочетания — у текста; свойства — к узлам вне неё.
            C::PasteProps => (edit && sel && !text, None),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    Delete,
    SelectAll,
    /// Отменить жест, вернуться к «Перемещению», снять выделение — по очереди.
    Escape,
````

на:

````rust
    Delete,
    SelectAll,
    /// «Выделить подобные»: узлы страницы того же типа с теми же заливками и
    /// обводками (как «Select matching layers» в Figma).
    SelectSimilar,
    /// Отменить жест, вернуться к «Перемещению», снять выделение — по очереди.
    Escape,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        &["все слои", "select all"],
        &["Mod+KeyA"],
    ),
    cmd(
````

на:

````rust
        &["все слои", "select all"],
        &["Mod+KeyA"],
    ),
    // Как «Select matching layers» в Figma: Ctrl/Cmd+Alt+A.
    cmd(
        C::SelectSimilar,
        "edit.select_similar",
        G::Edit,
        "Выделить подобные",
        &["такие же", "одинаковые", "select matching", "similar"],
        &["Mod+Alt+KeyA"],
    ),
    cmd(
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// содержимого встаёт на +10, +10 от неё (спека подсистемы 2, раздел 6).
    pub(crate) last_paste: Option<([u8; 32], Vec2)>,
    /// Внутренний буфер «Копировать свойства».
    pub(crate) props_clip: Option<ferrum_doc::Props>,
````

на:

````rust
    /// содержимого встаёт на +10, +10 от неё (спека подсистемы 2, раздел 6).
    pub(crate) last_paste: Option<([u8; 32], Vec2)>,
    /// Прошлый Ctrl/Cmd+D: для «умного дубликата».
    pub(crate) last_duplicate: Option<crate::actions::Duplicated>,
    /// Внутренний буфер «Копировать свойства».
    pub(crate) props_clip: Option<ferrum_doc::Props>,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            last_paste: None,
            props_clip: None,
        })
    }
````

на:

````rust
            last_paste: None,
            props_clip: None,
            last_duplicate: None,
        })
    }
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
mod vector_select;

pub use actions::{PasteMode, FIT_PADDING_PX, PASTE_STEP, PRESET_GAP};
pub use command_state::CommandState;
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
````

на:

````rust
mod vector_select;

pub use actions::{PasteMode, FIT_PADDING_PX, MAX_SIMILAR_SAMPLES, PASTE_STEP, PRESET_GAP};
pub use command_state::CommandState;
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
                S,
                I(C::SelectAll),
            ],
        ),
````

на:

````rust
                S,
                I(C::SelectAll),
                I(C::SelectSimilar),
            ],
        ),
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Состояния команд после шага, 100|Выделить подобные"`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/duplicate.spec.ts e2e/app.spec.ts --project=chromium`

Expected: PASS: fmt и clippy без замечаний; `ferrum-doc`, `ferrum-editor`, `ferrum-wasm` без падений (`smart_duplicate` — 4 теста; команд 85); `editor_report` — «Состояния команд после шага, 100 000 узлов, выделен 1» ≈ 0,001 мс (≤ 1 мс) PASS, «„Выделить подобные“, 100 000 узлов» ≈ 1,2–1,4 мс (≤ 100 мс) PASS, двойников 1000 из 1000, FAIL нет; модульные и сборка — ок; E2E `duplicate` (2 теста) и `app` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts apps/web/e2e/duplicate.spec.ts crates/ferrum-editor/examples/editor_report.rs crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/command_state.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/menu.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/main_menu.rs crates/ferrum-editor/tests/smart_duplicate.rs crates/ferrum-wasm/tests/boolean.rs
git commit -F - <<'EOF'
feat(editor): умный дубликат и «Выделить подобные» (Ctrl/Cmd+Alt+A), замер на 100 000 узлах

Ctrl/Cmd+D помнит оригиналы, копии и версию выделения: если выделены копии
прошлого дубликата и выделение не менялось (копии только двигали — протяжкой
или стрелками), новый дубликат сдвигается так же от них; любое другое
действие с выделением шаг забывает. «Выделить подобные» (edit.select_similar,
как «Select matching layers» в Figma) — видимые и не заблокированные узлы
страницы того же типа с теми же заливками и обводками, что у выделенных (до
64 образцов), совпавший — без потомков; один обход в worker'е. В «Правке»
главного меню; строка editor_report — 1,4 мс на 100 000 узлах (цель ≤ 100 мс).
Число команд — 85.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 7: Спека, бэклог и финальная проверка

Спека §12: тесты и замеры. Здесь — «Сделано (этап 2)» в разделах 5, 6, 11, 12 спеки оболочки (решения с пометкой «(поведение)», протокол, тесты, замер «Выделить подобные»); в `docs/backlog.md` — раздел «Оболочка (этап 2 подсистемы 2)» (B-120…B-132). Шаг «Проверить» — все ворота этапа.

**Files:**
- Modify: `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md`, `docs/backlog.md`

- [ ] **Step 1: Реализация**

В `docs/backlog.md` заменить:

````markdown
| B-119 | Нативная вставка в документе только для чтения (I1) проверена тестом ядра; ранний выход в `system-clipboard.ts` (без чтения буфера и запроса разрешения) без E2E — в E2E нет способа открыть файл «из будущего». | P3 | подсистема 2, этап 1, финальное ревью |

## Код

````

на:

````markdown
| B-119 | Нативная вставка в документе только для чтения (I1) проверена тестом ядра; ранний выход в `system-clipboard.ts` (без чтения буфера и запроса разрешения) без E2E — в E2E нет способа открыть файл «из будущего». | P3 | подсистема 2, этап 1, финальное ревью |

## Оболочка (этап 2 подсистемы 2)

| № | Что | Пр. | Источник |
|---|---|---|---|
| B-120 | Меню холста для текста без подменю недавних шрифтов (§5, блок 7): списка недавних шрифтов в ядре нет. Появится вместе с ним; «Войти в правку» у текста в меню тоже не показывается (только у вектора и фигур). | P2 | подсистема 2, этап 2 |
| B-121 | Меню пустого места холста без «Линейки» (§5): команды нет до этапа 4. | P2 | подсистема 2, этап 2 |
| B-122 | «Вставить поверх» и «Вставить с заменой» для картинки из другого приложения и обычного текста — обычная вставка (способ вставки учитывают только данные Ferrum). | P2 | подсистема 2, этап 2 |
| B-123 | «Вставить свойства»: эффектов и режима наложения в модели нет — не переносятся; участки текста со своей заливкой (разметка `fill`) заливка узла не перекрашивает; стиль текста — первого участка источника. | P2 | подсистема 2, этап 2 |
| B-124 | «Дублировать страницу» из меню страницы — не шаг отмены, как остальные операции со страницами; шагом — на этапе 4 (§10). Копия встаёт сразу после страницы и становится текущей. | P2 | подсистема 2, этап 2 |
| B-125 | Ctrl/Cmd+Shift+R («Вставить с заменой») не доходит до ядра нигде, и в десктопе: `browserKey` в `core.ts` отсекает Mod+R во всех средах. Команда — из меню (и палитры этапа 3). | P2 | подсистема 2, этап 2 |
| B-126 | «Выделить слой» из контекстного меню выделяет как панель слоёв (`Select`): панель не раскрывает предков и не прокручивает к узлу. | P3 | подсистема 2, этап 2 |
| B-127 | Повтор вставки: «исходное видно» — прежнее правило вставки на место (рамка видна и помещается в цель); после вставки в центр вида (исходное не видно) следующая при видимом исходном идёт +10, +10 от неё, а не от исходного места — буквально по §6. Новая копия (Ctrl/Cmd+C) счёт сбрасывает. | P3 | подсистема 2, этап 2 |
| B-128 | Копия свойств берёт в системный буфер и байты изображений заливок (base64 в ядре, до предела вставки 256 МБ) — у большой картинки это десятки мегабайт текста. Можно класть только хэш, когда вставка в тот же документ. | P3 | подсистема 2, этап 2 |
| B-129 | «Выделить подобные» сравнивает заливки и обводки точным равенством чисел (цвет, прозрачность, толщина) без допуска; у групп заливок нет — совпадают все группы страницы. | P3 | подсистема 2, этап 2 |
| B-130 | Меню строки слоя и страницы с клавиатуры (клавиша меню, Shift+F10) открывается в координатах события, а не у строки: место строки читалось бы из раскладки (только в `measure`). | P3 | подсистема 2, этап 2 |
| B-131 | E2E «Вставить с заменой» и «поверх» из меню, чтение копии свойств из системного буфера — только Chromium (в Firefox Playwright не даёт `clipboard-read`); Firefox проверяет повтор вставки, Ctrl/Cmd+Shift+V и свойства через внутренний буфер. | P3 | подсистема 2, этап 2 |
| B-132 | Захардкоженное число команд (B-72) — теперь 85 в трёх тестах. | P3 | подсистема 2, этап 2 |

## Код

````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
- **Слои.** Те же пункты, что на холсте для этого узла, плюс «Переименовать» и «Развернуть всё» / «Свернуть всё». Правый щелчок по невыделенной строке выделяет её.
- **Страницы.** Переименовать, дублировать, удалить, «Копировать ссылку на страницу». Ссылка — `#/file/<id>?page=<pageId>`, маршрут её разбирает.

## 6. Вставка и дубликат (этап 2)
````

на:

````markdown
- **Слои.** Те же пункты, что на холсте для этого узла, плюс «Переименовать» и «Развернуть всё» / «Свернуть всё». Правый щелчок по невыделенной строке выделяет её.
- **Страницы.** Переименовать, дублировать, удалить, «Копировать ссылку на страницу». Ссылка — `#/file/<id>?page=<pageId>`, маршрут её разбирает.
- **Сделано (этап 2):**
  - Дерево — `Editor::context_menu(ContextTarget)` (`context_menu.rs`); узлы дерева, кроме команд, подменю и разделителей, — `MenuNode::Node { id, name }` («Выделить слой», не больше 50, сверху вниз, у каждого попадания — оно и его контейнеры) и `MenuNode::Action { action, enabled }` — действие над целью меню, которое выполняет интерфейс (`MenuAction`: переименовать слой, развернуть и свернуть всё, переименовать, дублировать и удалить страницу, ссылка на страницу). Неизвестная цель, идущий жест — пустое меню (интерфейс его не открывает).
  - **(поведение)** Выбор на правой кнопке, как в Figma: узел, который выбрал бы щелчок (с правилами контекста и названием фрейма), выделяется, если не выделен; выделенный и пустое место внутри рамки выделения выделение сохраняют; пустое место вне её снимает выделение — меню пустого места. В правке текста и вектора правый щелчок правку не заканчивает.
  - **(поведение)** Холст: копировать, вставить, вставить поверх, вставить с заменой, «Копировать как ▸» (PNG, SVG) | копировать и вставить свойства | «Выделить слой ▸» | вперёд, назад, на передний план, на задний | сгруппировать, разгруппировать, обернуть во фрейм, «Булевы операции ▸», сплющить, контур обводки | отразить по горизонтали и вертикали, скрыть или показать, заблокировать или разблокировать | войти в правку (только у одного вектора или фигуры) | удалить. Недоступные — серые (состояние из `CommandStates`), а не скрыты. Пустое место — вставить, вставить поверх | выделить всё, показать всё. Подменю недавних шрифтов и линеек нет (B-120, B-121).
  - **(поведение)** Слой: невыделенная строка выделяется (как выбор из панели — без раскрытия и прокрутки); пункты холста без «Выделить слой», затем «Переименовать» (поле имени в строке), «Развернуть всё», «Свернуть всё» и «Удалить».
  - **(поведение)** Страница: «Переименовать», «Дублировать страницу», «Удалить страницу» (с подтверждением; единственную — серым), | «Копировать ссылку на страницу» (полный адрес, уведомление «Ссылка на страницу скопирована»). «Дублировать страницу» — копия с новыми идентификаторами и тем же фоном сразу после страницы, имя «<имя> (копия)», становится текущей; как и остальные операции со страницами, не шаг отмены (B-124). Ссылка открывает файл на этой странице (её нет — на первой) и оставляет в адресе только файл; неверный `page` (не `счётчик@узел` до 64 знаков) отбрасывается.
  - Интерфейс: правый щелчок шлёт `ContextMenu` (точка холста — `offsetX/Y`, без чтения раскладки), пункты приходят событием последними в пачке и встают в очередь планировщика вслед за выделением и состояниями команд; меню открывается у точки щелчка (длинное поднимается, чтобы влезть в окно). Локального меню холста больше нет.

## 6. Вставка и дубликат (этап 2)
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
- **Умный дубликат** **(поведение)**. После Ctrl/Cmd+D и сдвига копии следующий Ctrl/Cmd+D повторяет тот же сдвиг. Запомненный сдвиг сбрасывается при любом другом действии с выделением.
- **Выделить подобные.** Выделяются узлы того же типа с теми же заливками и обводками на странице. Работает в worker'е, на 100 000 узлах ≤ 100 мс.

## 7. Быстрые действия (этап 3)
````

на:

````markdown
- **Умный дубликат** **(поведение)**. После Ctrl/Cmd+D и сдвига копии следующий Ctrl/Cmd+D повторяет тот же сдвиг. Запомненный сдвиг сбрасывается при любом другом действии с выделением.
- **Выделить подобные.** Выделяются узлы того же типа с теми же заливками и обводками на странице. Работает в worker'е, на 100 000 узлах ≤ 100 мс.
- **Сделано (этап 2):**
  - Команды (группа «Правка», в главном меню и на холсте): `edit.paste_over` (Ctrl/Cmd+Shift+V), `edit.paste_replace` (Ctrl/Cmd+Shift+R — в списке сочетаний браузера, настройки пишут «Перехватывает браузер»; до ядра сочетание не доходит, B-125), `edit.copy_props` и `edit.paste_props` (Ctrl/Cmd+Alt+C и V), `edit.select_similar` (Ctrl/Cmd+Alt+A, как «Select matching layers»). Таблица — 85 команд.
  - **(поведение)** Вставка поверх — на исходные координаты в ту же цель, что у обычной вставки, без сдвига к центру и без шага повтора. Ctrl/Cmd+Shift+V на холсте приходит событием `paste` браузера с режимом «поверх»; в правке текста — по-прежнему обычный текст.
  - **(поведение)** Вставка с заменой — для каждого выделенного узла верхнего уровня, кроме заблокированных: содержимое буфера левым верхним углом рамки — в угол рамки узла, у того же родителя и на его месте в слоях, узел удаляется; выделены вставленные; один шаг. Выделения нет (сняли, пока читался буфер) — обычная вставка.
  - **(поведение)** Повтор вставки того же содержимого (хэш байтов), пока исходное видно и помещается в цель, — +10, +10 от прошлой вставки; иначе — в центр, как раньше. Новая копия начинает счёт заново (B-127).
  - **(поведение)** Свойства: копия — свойства первого выделенного (заливки, обводки и их стиль, прозрачность, скругление, у текста — стиль первого участка); во внутренний буфер ядра и текстом `ferrum-props:v1:` + base64 формата буфера (один узел, с изображениями своих заливок) в `text/plain`; уведомление «Свойства скопированы». Вставка берёт копию свойств или копию узлов (свойства первого узла, как в Figma) из системного буфера, без доступа или без наших данных — внутренний буфер, без него — «Сначала скопируйте свойства». Каждому выделенному — что подходит типу: у группы и линии нет заливок, у группы — обводок, у группы и текста — стиля обводки, скругление — у фрейма, прямоугольника и изображения, стиль текста — тексту (рамка подгоняется); одним шагом. В правке текста «Вставить свойства» недоступна. Копия свойств обычной вставкой не вставляется.
  - **(поведение)** Умный дубликат: пока выделены копии прошлого Ctrl/Cmd+D и выделение не менялось (их двигали протяжкой или стрелками, отменяли и повторяли сдвиг), новый дубликат сдвигается от них на тот же шаг, что они от своих оригиналов. Любое другое действие с выделением (выбор, отмена дубликата) шаг забывает.
  - **(поведение)** «Выделить подобные»: видимые и не заблокированные узлы страницы того же типа с теми же заливками и обводками, что у любого из выделенных (до 64 разных образцов); совпавший контейнер — без потомков; выбор — не шаг истории. На 100 000 узлах — 1,4 мс нативно (`editor_report`).
  - «Копировать как PNG» уже был (этап 5 подсистемы 3): событие `ClipboardPng`, теперь — в подменю «Копировать как» меню холста.

## 7. Быстрые действия (этап 3)
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
  - `SetTheme` получил необязательный `system` — тема выбрана «как в системе» (для отметки «Тема как в системе»).
  - `KeymapEntry` получил `keywords` и `toggle`, а `group` теперь — группа из таблицы, а не префикс ключа.

## 12. Тестирование
````

на:

````markdown
  - `SetTheme` получил необязательный `system` — тема выбрана «как в системе» (для отметки «Тема как в системе»).
  - `KeymapEntry` получил `keywords` и `toggle`, а `group` теперь — группа из таблицы, а не префикс ключа.
- **Сделано (этап 2):**
  - `ContextMenu { target }`, `target` — `{ kind: "canvas", x, y } | { kind: "layer", id } | { kind: "page", id }` (лишние поля и неизвестный вид — ошибка разбора; нечисловая точка, неверный id — `Error` и пустое меню). Ответ — `ContextMenu { items }`: `MenuItemDto` получил `{ type: "node", id, name }` и `{ type: "action", action, title, enabled }` (`action` — `renameLayer | expandAll | collapseAll | renamePage | duplicatePage | deletePage | copyPageLink`); ключи команд — только из таблицы.
  - `ExpandAllLayers { expanded }`, `DuplicatePage { id }`.
  - `Paste { bytes, mode? }` и `PasteRequested { mode }`, `mode` — `normal | over | replace | props`; на `props` интерфейс отвечает `PasteProps { bytes? | text? }` (байты `web application/x-ferrum` или текст с меткой свойств или узлов; без них — внутренний буфер; текст — не длиннее предела текста буфера). `ClipboardProps { text }` — копия свойств для `text/plain`. Сообщение worker'у `pasteText` несёт `mode`.

## 12. Тестирование
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
  - кадр с линейками — не больше кадра без них плюс 0,5 мс.
- **Сделано (этап 1):** модульные тесты ядра — `command_table` (заполненность таблицы, переключатели, команды `ui.*` только сообщают о себе), `command_state` (без выделения, одна фигура, группы и булевы, история, правка текста и вектора, «только чтение», недоступная команда без событий, отметки вида и настроек, события только с изменениями и не во время жеста), `main_menu` (порядок Figma UI3, состав подменю, без пустых подменю и лишних разделителей, все команды групп меню в дереве), `menu_commands` (сетка, «Войти в правку», форматирование в правке и вне её по раскладке); ядро worker'а — состояния целиком при запуске и после открытия, изменения одним событием, тема «как в системе», недоступная команда без ошибки, нативная вставка (`Paste`, `PasteImage`, `TextInput { paste }`) в документе только для чтения — без ошибки, шага и прерывания жеста, `MainMenu` один раз. Замер `editor_report`: «Состояния команд после шага, 100 000 узлов, выделен 1» — 0,001 мс (цель ≤ 1 мс; нативно, release), справочно при 10 000 выделенных — 0,25 мс, при всех 100 000 — 4,6–5,6 мс (B-95). E2E: `main-menu.spec.ts` (Chromium и Firefox) — подменю клавиатурой и наведением, Alt+/, серые пункты и галочки из ядра, тема, поиск в другой раскладке, первая буква, подменю у края окна, корневое меню с десятками результатов поиска не уходит за низ окна, команды интерфейса, `__ferrumWarnings` пуст; в `chrome.spec.ts` — серые пункты стрелками и первая буква, контекстное меню и тулбар по состоянию ядра. Прежние тесты меню файла открывают пункты через главное меню (`mainMenu` в `helpers.ts`).
- **E2E в Chromium и Firefox:**
  - главное меню с подменю, клавиатурой и состоянием пунктов;
````

на:

````markdown
  - кадр с линейками — не больше кадра без них плюс 0,5 мс.
- **Сделано (этап 1):** модульные тесты ядра — `command_table` (заполненность таблицы, переключатели, команды `ui.*` только сообщают о себе), `command_state` (без выделения, одна фигура, группы и булевы, история, правка текста и вектора, «только чтение», недоступная команда без событий, отметки вида и настроек, события только с изменениями и не во время жеста), `main_menu` (порядок Figma UI3, состав подменю, без пустых подменю и лишних разделителей, все команды групп меню в дереве), `menu_commands` (сетка, «Войти в правку», форматирование в правке и вне её по раскладке); ядро worker'а — состояния целиком при запуске и после открытия, изменения одним событием, тема «как в системе», недоступная команда без ошибки, нативная вставка (`Paste`, `PasteImage`, `TextInput { paste }`) в документе только для чтения — без ошибки, шага и прерывания жеста, `MainMenu` один раз. Замер `editor_report`: «Состояния команд после шага, 100 000 узлов, выделен 1» — 0,001 мс (цель ≤ 1 мс; нативно, release), справочно при 10 000 выделенных — 0,25 мс, при всех 100 000 — 4,6–5,6 мс (B-95). E2E: `main-menu.spec.ts` (Chromium и Firefox) — подменю клавиатурой и наведением, Alt+/, серые пункты и галочки из ядра, тема, поиск в другой раскладке, первая буква, подменю у края окна, корневое меню с десятками результатов поиска не уходит за низ окна, команды интерфейса, `__ferrumWarnings` пуст; в `chrome.spec.ts` — серые пункты стрелками и первая буква, контекстное меню и тулбар по состоянию ядра. Прежние тесты меню файла открывают пункты через главное меню (`mainMenu` в `helpers.ts`).
- **Сделано (этап 2):** модульные тесты ядра — `context_menu` (выбор на правой кнопке: невыделенный, выделенный, пустое место в рамке и вне её; порядок блоков холста; «Выделить слой» сверху вниз с контейнерами; «Войти в правку» только у фигур; меню слоя и страницы, их доступность в документе только для чтения; неизвестная цель; правка вектора не кончается; «Дублировать страницу» с новыми id, фоном и местом в списке), `paste_modes` (повтор +10, сброс новой копией, центр вида; поверх; замена с местом в слоях и родителем — один шаг; заблокированные и без выделения; состояния и `PasteRequested` с режимом), `props_clip` (что подходит каждому типу — один шаг; текст с меткой — в другой документ; свойства копии узлов; ошибки без шага; стиль текста с заливкой и подгонкой рамки — один шаг; состояния), `smart_duplicate` (сдвиг протяжкой и стрелками повторяется, сброс выбором и отменой дубликата, несколько узлов; «Выделить подобные»: тип, заливки, обводки, скрытые, образцы, контейнер без потомков), `journal` («Дублировать страницу» переживает журнал и полное сохранение); ядро worker'а — `context_menu` (меню последним после выделения и состояний, ошибки цели, слой без прокрутки, «Развернуть всё», дубль страницы), `paste` (режимы в `PasteRequested`, замена; свойства текстом и из внутреннего буфера), `protocol` (новые сообщения, имена полей, `Paste` без `mode`). Замер `editor_report`: «Выделить подобные, 100 000 узлов» — 1,4 мс (цель ≤ 100 мс; нативно, release), выделено 1000 двойников. E2E: `context-menu.spec.ts` (Chromium и Firefox) — холст: выбор, место меню, состав, «Выделить слой», команда — шаг; пустое место; слои: выбор строки, «Развернуть всё», «Свернуть всё», «Переименовать»; страницы: серое «Удалить», «Дублировать», ссылка открывает страницу; `__ferrumWarnings` пуст. `paste.spec.ts` — повтор +10 и Ctrl/Cmd+Shift+V, свойства Ctrl/Cmd+Alt+C и V (оба браузера); замена и поверх из меню, предупреждение о Ctrl/Cmd+Shift+R, копия свойств в системном буфере (только Chromium, B-131). `duplicate.spec.ts` — умный дубликат стрелками и протяжкой, «Выделить подобные» из меню и сочетанием. `chrome.spec`, `clipboard.spec`, `main-menu.spec` — под новый состав меню.
- **E2E в Chromium и Firefox:**
  - главное меню с подменю, клавиатурой и состоянием пунктов;
````

- [ ] **Step 2: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-render --example render_report | grep -E "FAIL" || echo "FAIL нет"`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Состояния команд после шага, 100|Выделить подобные"`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/context-menu.spec.ts e2e/paste.spec.ts e2e/duplicate.spec.ts e2e/chrome.spec.ts e2e/main-menu.spec.ts e2e/app.spec.ts --project=firefox`

Expected: всё PASS: fmt, clippy (нативно и wasm32) без замечаний; `cargo test --workspace` — 1038 passed, 2 ignored; `render_report` — FAIL нет; `editor_report` — 91 строка PASS, «Состояния команд после шага, 100 000 узлов, выделен 1» — 0,001 мс, «„Выделить подобные“, 100 000 узлов» — 1,2 мс, FAIL нет (строка «Масштаб (K): 10 000 узлов — превью поля „Масштаб“» сразу после `cargo test --workspace` может дать FAIL из-за загрузки машины — повторный прогон PASS; код масштаба этап не трогает); модульные — 152 passed; `e2e:ci` — ≈ 355 тестов без падений (известные нестабильные `text-edit` «фрагмент со стилями…», `booleans` «Alt+Shift+U — объединение…» и `roundtrip` «нарисовать фрейм → сохранить…» проходят поодиночке); Firefox — 60 passed, 2 skipped (чтение системного буфера в `paste.spec.ts` — только Chromium)

- [ ] **Step 3: Закоммитить**

```bash
git add docs/backlog.md docs/superpowers/specs/2026-11-02-ferrum-shell-design.md
git commit -F - <<'EOF'
docs: оболочка, этап 2 — «Сделано» в §5, §6, §11, §12 и ограничения в бэклоге (B-120…B-132)

Контекстные меню холста, слоёв и страниц из ядра и их состав (поведение),
ссылка на страницу; вставка поверх и с заменой, повтор вставки +10, +10,
копирование и вставка свойств, умный дубликат, «Выделить подобные» — 1,4 мс
на 100 000 узлах. Протокол: ContextMenu, узлы и действия меню, ExpandAllLayers,
DuplicatePage, способ вставки, PasteProps, ClipboardProps. Тесты этапа.
Новый раздел бэклога «Оболочка (этап 2 подсистемы 2)».

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

- [ ] **Ручная проверка**

1. Chrome: `#/new`; правый щелчок по невыделенному прямоугольнику — он выделен, меню у точки щелчка; блоки §5, «Разгруппировать» серая; «Выделить слой ▸» — узлы под точкой с фреймом.
2. Правый щелчок по пустому месту вне выделения — выделение снято, меню «Вставить», «Вставить поверх», «Выделить всё», «Показать всё»; внутри рамки выделения — выделение сохранено.
3. Панель слоёв: правый щелчок по строке — «Переименовать» (поле в строке), «Развернуть всё», «Свернуть всё». Вкладка страницы: «Дублировать страницу» — копия сразу после, текущая; «Копировать ссылку на страницу» — уведомление; ссылка в новой вкладке открывает файл на этой странице, в адресе — только файл.
4. Ctrl/Cmd+C, Ctrl/Cmd+V трижды — копии со сдвигом +10, +10; Ctrl/Cmd+Shift+V — на исходные координаты; «Вставить с заменой» из меню — копия на месте выделенного; в настройках у Ctrl/Cmd+Shift+R — «Перехватывает браузер».
5. Ctrl/Cmd+Alt+C на прямоугольнике с заливкой и обводкой, Ctrl/Cmd+Alt+V на эллипс и текст — одним шагом отмены; уведомление «Свойства скопированы».
6. Ctrl/Cmd+D, сдвиг копии стрелками, Ctrl/Cmd+D — тот же шаг; Ctrl/Cmd+Alt+A — выделены все такие же.
7. Firefox: меню холста, слоёв и страниц; повтор вставки; свойства через внутренний буфер.

---

## Что этап 2 сознательно не делает

- **Подменю недавних шрифтов и линейки** в меню холста — нет данных и команды до этапа 4 (B-120, B-121).
- **«Вставить поверх» и «с заменой» для картинки и обычного текста** — обычная вставка: режим учитывают только данные Ferrum (B-122).
- **Эффекты и режим наложения** в «Вставить свойства» — их нет в модели; стиль текста — первого участка источника (B-123).
- **«Дублировать страницу» — не шаг отмены**, как остальные операции со страницами; шагом — на этапе 4 (B-124).
- **Ctrl/Cmd+Shift+R до ядра не доходит** нигде, и в десктопе (`browserKey`, B-125) — команда из меню.
- **«Выделить слой» не раскрывает и не прокручивает** панель слоёв (B-126); меню строки с клавиатуры — в координатах события (B-130).
- **Копия свойств несёт байты картинок заливок** в системный буфер (B-128); «Выделить подобные» — точное равенство чисел (B-129).
- **Числа команд в тестах** по-прежнему зашиты (B-72, B-132): этап их только обновил до 85.
- **Не запускались:** проект Playwright `gpu` и замеры `@perf`, Safari/WebKit, десктоп (крейт не тронут), замеры в wasm (только нативный `editor_report`); чтение системного буфера в Firefox (B-131).
