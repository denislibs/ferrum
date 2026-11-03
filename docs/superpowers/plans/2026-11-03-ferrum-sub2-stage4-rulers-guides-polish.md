# Ferrum, подсистема 2, этап 4: линейки, направляющие и полировка панелей — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Линейки сверху и слева от холста (Shift+R, «Вид → Линейки»): деления под масштаб, полоса выделения, начало отсчёта — фрейм верхнего уровня под выделением. Направляющие страницы и фрейма: протяжка из линейки, перенос, удаление на линейку, привязка наравне с краями узлов, каждая правка — шаг отмены; показ — Shift+G и «Вид → Направляющие». Полировка панелей: сворачиваемые секции и боковые панели, уведомления с «Отменить», кнопкой закрытия, 4 с и паузой под курсором, перестановка страниц протяжкой и «Дублировать страницу» — шагами отмены, имя текстового слоя из его содержимого.

**Architecture:** Модель (`ferrum-doc`): `guides.rs` — `Guide { axis, offset }`, неизменяемое значение-список под ключом `guides`; у страницы — в узле дерева страниц корневого документа (как фон), у фрейма — `Props.guides`, кэш сцены версии 6. История: `HistoryEntry` получает номер шага `id`, `page_after` и обратные операции над корнем `DocOp`, отмена дубля паркует `PageDoc` в `Document::parked`. Отрисовка (`ferrum-render`): `RulersOverlay`, `GuideMark`, `ActiveGuide` — последний слой оверлея, линии — прямоугольниками. Редактор (`ferrum-editor`): `guides.rs` — настройка вида, кэш линий, жест `GuideDrag` на событиях указателя внутри ядра, `press_tool`/`release_tool` отдают нажатие на линейку и линию жесту; `Snapper::with_guides` — цели привязки. Ядро (`ferrum-wasm`): `SetView`/`View`, `SetGuides`, `UndoStep`/`UndoNotice`, `MovePage`. Интерфейс: настройки `rulers`, `guides`, `leftCollapsed`, `rightCollapsed`, `collapsedSections`, `Section { name? }`, `PanelToggle`, уведомления в `store.ts`/`toasts.tsx`, протяжка в `pages.tsx`; основной поток только шлёт команды и читает раскладку списка страниц в `measure`.

**Tech Stack:** Rust 1.96 (`rust-toolchain.toml`, новых зависимостей нет), Loro, SolidJS 1.9.17, TypeScript 6.0.3, @playwright/test 1.64.0 (`chromium`, `firefox`); `package-lock.json` и `Cargo.lock` не меняются.

**Спеки:** `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` — разделы 9, 10, 11, 12 (этап 4); спека ядра `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` (протокол и проверка файла — §4.3, тонкий основной поток — §4.2, привязка — §7.3).

## Global Constraints

- Ветка `feat/sub2-rulers` от `main` с этапами 1–3 (fe1b2cd). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Каталог сборки — у каждой копии свой: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0` (в `apps/web` — `"$PWD/../../target"`, в `apps/desktop/src-tauri` — `"$PWD/../../../target"`); команды — из корня копии, если не сказано иное.
- Новых зависимостей нет ни в Rust, ни в npm.
- Основной поток тонкий (ядро §4.2): линейки и линии рисует ядро в кадре холста, жест направляющей идёт внутри ядра. Чтения layout — только в фазе `measure` планировщика (единственное новое чтение — место вставки в списке страниц). E2E проверяют `__ferrumWarnings` — пусто.
- Поведение — как в Figma; где спека молчит, решение помечено **(поведение)** (раздел «Решения»).
- Бюджеты: кадр с линейками ≤ кадр без них + 0,5 мс (`render_report`, нативно, release); пересчёт `CommandStates` после шага на 100 000 узлах при одном выделенном — ≤ 1 мс (`editor_report`).
- Пределы данных: направляющих ≤ 1000 на узел, смещение конечно и |offset| ≤ 1e9. Из файла неверные записи отбрасываются поодиночке, правка сверх пределов — отказ целиком (`DocError::InvalidGuides`).
- Совместимость со старой сборкой проверяет `scripts/compat-old-build.sh main guides_fixture` (скрипт сам создаёт и убирает временный worktree `main`; `main` — сборка без направляющих, пока этап не влит; скрипт берёт локальную ветку, поэтому перед проверкой она должна совпадать с `origin/main`, иначе сравнение идёт с более старой сборкой): файл открывается для записи, пересохранённый старой сборкой снова читается новой с теми же линиями.
- Числа команд таблицы зашиты в трёх тестах — `crates/ferrum-editor/tests/keymap.rs`, `crates/ferrum-wasm/tests/boolean.rs`, `apps/web/e2e/app.spec.ts`: задача 2 обновляет все три (86 → 88).
- Поля настроек перечислены дважды — `SETTINGS_FIELDS` в `apps/web/src/core/file-list.ts` и в `apps/desktop/src-tauri/store/src/settings.rs`: задачи 4 и 5 правят оба.
- Тесты не проходят вхолостую: модель — шаги истории, сохранение, чужие записи файла, отказы без следов; редактор — жесты с событиями указателя и шаги; ядро — события протокола; модульные — чистые функции и часы уведомлений; E2E — роли и `aria-*`, настройки после перезагрузки, пиксели холста и шаги отмены.
- Порт 4173 может быть занят посторонним процессом: его не трогать, E2E тогда — через временный конфиг Playwright с другими портами (не коммитить).
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Сначала прототип всего этапа собран в отдельной копии (`plan-verify-s2s4`, ветка `verify/sub2-s4` от fe1b2cd, по коммиту на задачу; вершина — 6de028d, код — fd93825) и прошёл ворота на коде этапа (нагрузка машины 5–11 от других процессов): `cargo fmt --check`, `clippy -D warnings` (нативно и `wasm32-unknown-unknown`), `cargo test --workspace` (1154 passed, 0 failed, 2 ignored; `rebase --exec` по коммитам кода: 1093, 1105, 1140, 1140, 1140, 1146, 1147, 1154, везде зелёно), `render_report` (FAIL нет; прибавка кадра с линейками 0,150 мс на 1920×1080 и 0,216 мс на 2×, цель ≤ 0,5 мс; с растеризацией CPU на пустой сцене 0,203 и 0,354 мс; с 2000 линий 1,43 и 2,96 мс), `editor_report` (на коде вершины FAIL нет: 0,003 / 0,005 / 0,043 мс на строках линий; в прогоне предыдущей ревизии одна посторонняя строка FAIL — «Поиск слоёв… 256 символов… самый первый на странице» 45,108 мс при цели ≤ 40, код поиска этап не трогает, B-150), wasm-сборка, `typecheck`, модульные (176), сборка, E2E `chromium` целиком (`--grep-invert @perf --retries=1`: 399 passed, 4 flaky, 0 failed; flaky — `fallback` «worker не ответил», `files` «1000 записей», `files` «вторая вкладка», `fsa` «запись на диске», три последних отдельным прогоном прошли, B-177), Firefox — новые и затронутые спеки (`rulers`, `panels`, `toasts`, `pages-order`, `text`, `text-edit`, `main-menu`, `palette`, `shortcuts-panel`, `context-menu`: 45 passed, 1 flaky — гонка самого теста `pages-order` «Дублировать страницу», исправлена, спека затем 3 раза подряд 4 passed; полный Firefox после правки не повторялся), `compat-old-build.sh main guides_fixture` и `main vector_fixture`, десктоп — `cargo check` и `ferrum-desktop-store` (25 passed). После этих ворот изменились только один E2E-тест (`pages-order.spec.ts`, проверены `typecheck` и спека в Firefox) и документы: полные ворота на самой вершине 6de028d не повторялись.

Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом, который читает сам этот документ, к чистой копии от fe1b2cd (`plan-apply-s2s4`, ветка `verify/sub2-s4-apply`) в порядке задач с коммитом после каждой: каждый фрагмент «заменить» найден ровно один раз (операций по задачам: 75, 51, 49, 17, 32, 18, 12, 11, 7). После каждой задачи дерево копии совпало с деревом соответствующего коммита прототипа (тот же хэш дерева у всех девяти), `git diff 6de028d HEAD` — пустой.

На этой копии выполнены все шаги «Убедиться, что тесты падают» задач 1–8: каждая команда упала, причины в шагах записаны из этого прогона. Новые E2E-спеки в шаги «падают» не входят и до реализации не запускались. Шаг «Проверить» на копии выполнен полностью только у задачи 1 (fmt, clippy, wasm32, тесты, совместимость); у задачи 2 — fmt, clippy, wasm32 и тесты (997 passed в четырёх крейтах). Сверх того на копии прошли `clippy --workspace --all-targets -D warnings` на коммитах задач 3, 6 и 7 и `compat-old-build.sh fe1b2cd guides_fixture`. Остальное в шагах «Проверить» — ожидание из прогона прототипа на том же коде: `render_report`, `editor_report`, wasm, `typecheck`, модульные и E2E на промежуточных коммитах не выполнялись (полные ворота — только на коде этапа); машина в это время была под нагрузкой 90–185 от других процессов, и замеры времени всё равно были бы недостоверны. Числа новых тестов — по самим тестовым файлам.

Ветка `main` в репозитории проверки отставала: локальная указывала на cd26851, `origin/main` — на fe1b2cd. Поэтому `main guides_fixture` и `main vector_fixture` прототипа сравнивали с более старой сборкой; с настоящей предшественницей этапа `guides_fixture` проверен отдельно (fe1b2cd, см. выше), `vector_fixture` — нет.

Найденное прототипом и вошедшее в шаги: штрих всех линий одним путём стоил 26–54 мс на 2000 линий, заменён прямоугольниками (1,4–3 мс, задача 2); `cancel_guides` через `||` не снимал превью, если шёл жест — поймал тест Esc (задача 3); переключатели мигали при удержании клавиши, все, не только новые — `e.repeat` на `toggle` теперь не выполняется (задача 3); `-0` в подписи деления исправлен в `tick_label` (задача 2); тесты, зашитые на Shift+R = «ничего», и порядок меню пустого места обновлены (задача 2); `cargo fmt --check` не проходил на `main` из-за `crates/ferrum-editor/tests/keymap.rs` (задача 1); протяжка в E2E за один и тот же щелчок считается двойным щелчком, поэтому тесты жмут в другой точке (задача 4).

Не запускались: проект Playwright `gpu`, `@perf`, WebKit и Safari, запуск десктопа (только `cargo check` и тесты хранилища), замеры в wasm. Firefox — не весь набор. Отдельного E2E для `SetGuides` нет: интерфейс команду не использует, она проверена тестами ядра. Порт 4173 на машине проверки не трогали.

---

## Этапы подсистемы 2

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Состояние команд и главное меню | описание команд в ядре, состояние «доступна / отмечена», главное меню с подменю и поиском, компонент меню с подменю и первой буквой | 3, 4 |
| 2. Контекстные меню и вставка | меню холста по выделению, меню слоёв и страниц; свойства, вставка с заменой и поверх, умный дубликат, «Выделить подобные» | 5, 6 |
| 3. Быстрые действия и сочетания | палитра Ctrl/Cmd+K и Ctrl/Cmd+/: команды, слои, страницы, недавние; панель сочетаний клавиш | 7, 8 |
| **4. Линейки, направляющие и полировка панелей** (этот план) | линейки, направляющие с привязкой, сворачиваемые секции, уведомления с действием, страницы | 9, 10 |

## Решения

Где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**. По справке Figma при сборке прототипа сверены только пункты 1 и 2; остальное — решения по памяти о Figma, не сверенные.

Линейки и показ:

1. **(поведение)** Направляющие видны только при включённых линейках: без линеек нет ни линий, ни привязки, «Вид → Направляющие» серый. Источник — справка Figma [«Add guides to the canvas or frames»](https://help.figma.com/hc/en-us/articles/360040449713-Add-guides-to-the-canvas-or-frames): "Users with view access can toggle Rulers on and off to show/hide Guides". Ctrl/Cmd+\ прячет линейки вместе с интерфейсом, линии остаются.
2. **(поведение)** Показ направляющих — Shift+G. **Не подтверждено как сочетание направляющих линеек:** спека §9 просит «сочетание из справки Figma», но справка такого сочетания не называет и отдельного пункта не описывает (запрос на него есть на форуме). Shift G в справке — [«Layout guides»](https://help.figma.com/hc/en-us/articles/360040450513-Create-layout-guides) ([раскладка](https://help.figma.com/hc/en-us/articles/5665442977431-Select-keyboard-layout)), то есть сетки раскладки, которых в Ferrum до подсистемы 5 нет. Сторонние сайты называют Shift+Cmd+H, Shift+Cmd+; и Ctrl+; — противоречиво, не взято. Конфликтов с раскладкой Ferrum нет (тест `keymap`). B-163.
3. **(поведение)** Линейки — 20 CSS px сверху и слева, последний слой оверлея. Шаг подписей 1, 2, 5 × 10ⁿ единиц мира, не теснее 72 px и не чаще единицы мира; мелкие деления — кратные, не чаще 6 px; подписи левой линейки идут снизу вверх. Начало отсчёта — левый верхний угол фрейма верхнего уровня, если всё выделение под одним таким фреймом без поворота (и если выделен он сам), иначе ноль страницы.
4. Настройка вида `rulers`, `guides` хранится интерфейсом; `SetView` при запуске и из другой вкладки, команда сообщает событие `View`, `SetView` — нет (иначе запуск затёр бы сохранённую). Удержание клавиши не мигает никаким переключателем: `e.repeat` на `toggle` не выполняется.

Направляющие:

5. **(поведение)** Страничные направляющие — в дереве страниц корня (как фон: читаются без загрузки страницы), смещение от нуля страницы, линия через весь холст. Фреймовые — `Props`, смещение от левого верхнего угла, линия от края до края рамки, едет с фреймом. Только фреймы верхнего уровня без поворота: вложенный линий не показывает (данные остаются), косые линии повёрнутого фрейма без попадания и привязки (B-164).
6. **(поведение)** Владелец определяется при отпускании: верхний видимый, не заблокированный и не повёрнутый фрейм верхнего уровня под указателем, иначе страница. Перенос страница ↔ фрейм — один шаг.
7. **(поведение)** Сквозь линию: щелчок без сдвига (порог 3 px) идёт узлу как обычный щелчок, ручки и выделенный лист важнее линии, линия выделенного фрейма доступна; за линию можно взяться только в «Перемещении» без модификаторов, допуск 3 px.
8. **(поведение)** Линейка забирает нажатие у инструмента и правок: щелчок не снимает выделение и не начинает рисование, правка текста и вектора не заканчивается (B-166).
9. **(поведение)** Тянущаяся линия привязывается к узлам (соседи во фрейме под ней или на странице) и другим линиям, иначе — к целым единицам мира при «Привязке к пикселям»; плашка показывает значение от начала отсчёта линеек. На своей линейке линии нет, отпускание удаляет её; новую — не создаёт. Esc, потеря фокуса и потерянное отпускание — отмена без следов.
10. **(поведение)** Привязка: порог тот же, 4 экранных пикселя; перетаскивание, ресайз и рисование. Линия фрейма действует только напротив него и не держит сам фрейм и его потомков. Правка вектора, перо и текст к линиям не привязываются (B-165).
11. **(поведение)** Кэш сцены версии 6 отвергает плохую строку целиком: файл читается из Loro. Значение `guides` заменяется целиком, как заливки; нет линий — нет ключа. В буфере обмена — необязательное поле у фрейма, копия и дубликат сохраняют линии, неверные — отказ вставки.
12. `SetGuides { owner, guides, phase }` — только для внешних клиентов протокола: протяжка на холсте идёт внутри ядра, интерфейс команду не шлёт, проверена тестами ядра, отдельного E2E нет.
13. Линии рисуются прямоугольниками: штрих одним путём стоил 26–54 мс на 2000 линий, прямоугольники — 1,4–3 мс.

Панели, уведомления, страницы, текст:

14. **(поведение)** Весь заголовок секции — кнопка; свёрнутость — настройка `collapsedSections` (имена через перевод строки), общая для всех выделений; имя стабильно, когда заголовок меняется (у «Фрейм» и «Положение» — «Положение»). «Экспорт…» разворачивает свою панель и секцию.
15. **(поведение)** Обе панели (спека §10 просит левую) сворачиваются кнопкой в шапке в полосу 48 px; содержимое не смонтировано, холст занимает место. Настройки `leftCollapsed`, `rightCollapsed`, ширина прежняя.
16. **(поведение)** Все уведомления живут 4 с (было 6, B-177), кнопка закрытия у каждого; под курсором или с фокусом внутри часов нет, после ухода — снова 4 с. Тот же текст про новый шаг заменяет прежнее уведомление.
17. «Отменить» (спека §10 говорит про `edit.undo`) шлёт `UndoStep { step }` с номером из `UndoNotice`: не последний шаг — ничего; тот же номер после отмены и повтора снова годится. Только если правка создала шаг и не в правке текста или вектора. Открытый путь пера блокирует отмену.
18. **(поведение)** `MovePage`: протяжка (порог 4 px, линия показывает место, Esc и потеря фокуса — отмена, щелчок после протяжки не переключает страницу) и Alt+стрелка; текущая страница не меняется, то же место — ни шага, ни правки.
19. Отмена дубля паркует `PageDoc`; повтор даёт новый `PageId` (дерево Loro не воскрешает узел), шаги повтора переписываются (`History::remap_page`). Новый шаг сбрасывает припаркованные. Отмена возвращает на страницу, с которой дублировали, повтор — на копию.
20. **(поведение)** Автоимя без признака в модели: имя следует за текстом, пока равно производному от прежнего содержимого или «Текст» (первые 40 символов первой строки, пробелы по краям убраны, управляющие — в пробел). Переименование в точности в текст снова включает слежение (B-168). Копия слоя сохраняет своё имя.

Протокол: `SetView { rulers?, guides? }`, событие `View { rulers, guides }`; `SetGuides { owner: { kind: "page" | "frame", id }, guides: [{ axis: "x" | "y", offset }], phase }`; `UndoStep { step }`, событие `UndoNotice { text, step }`; `MovePage { id, index }`. Таблица — 88 команд (`view.rulers`, `view.guides`).

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-doc/src/guides.rs` (новый), `codec`, `document`, `history`, `page`, `scene_cache`, `container`, `types`, `error`, `lib` | модель линий, чтение и запись, `DocOp`, шаги страниц, номер шага, кэш версии 6 |
| `crates/ferrum-doc/examples/guides_fixture.rs` (новый), `scripts/compat-old-build.sh` | проверка старой сборкой |
| `crates/ferrum-render/src/{overlay,text,lib}.rs` | линейки и линии в кадре, повёрнутая подпись |
| `crates/ferrum-editor/src/guides.rs` (новый), `snap`, `editor`, `tools/{mod,select,draw}` | показ, кэш линий, жест, привязка |
| `crates/ferrum-editor/src/{commands,actions,command_state,menu,context_menu,vector_select,clipboard,pages}.rs` | команды, меню, `UndoNotice`, `move_page`, поле буфера |
| `crates/ferrum-editor/src/{text_input,text_clipboard}.rs` | имя текста у новых слоёв |
| `crates/ferrum-wasm/src/protocol.rs`, `core.rs`, `core/panels.rs` | `SetView`, `View`, `SetGuides`, `UndoStep`, `UndoNotice`, `MovePage` |
| `apps/web/src/ui/{settings.ts,app.tsx,context.ts,chrome.tsx,icons.tsx,properties.tsx,missing-fonts.tsx}`, `src/core/file-list.ts`, `apps/desktop/src-tauri/store/src/settings.rs` | настройки, секции и панели |
| `apps/web/src/ui/{store.ts,toasts.tsx,pages.tsx}`, `styles/{base,chrome,layers,theme}.css` | уведомления, перестановка страниц, стили |
| `crates/*/tests/`, `apps/web/unit/`, `apps/web/e2e/{rulers,panels,toasts,pages-order}.spec.ts` (новые) | тесты |
| `crates/ferrum-render/examples/render_report.rs`, `crates/ferrum-editor/examples/editor_report.rs`, спека, `docs/backlog.md` | замеры; «Сделано (этап 4)», B-161…B-177 |

---

### Task 1: Модель направляющих и шаги отмены страниц

Спека §9, §10: ключ `guides` у страницы и фрейма, до 1000 штук, неверные данные из файла проверяются; каждая правка — шаг; «Дублировать страницу» и порядок страниц — шаги. Здесь — `guides.rs`, чтение и запись, `Document::{page_guides, set_guides, move_page, last_step}`, обратные операции над корнем `DocOp`, парковка дубля, кэш сцены версии 6, поле в буфере обмена, `guides_fixture` и ветка в `compat-old-build.sh`. Попутно: `cargo fmt --check` на `main` не проходил из-за `crates/ferrum-editor/tests/keymap.rs` — исправлено здесь.

**Files:**
- Create: `crates/ferrum-doc/examples/guides_fixture.rs`, `crates/ferrum-doc/src/guides.rs`
- Modify: `crates/ferrum-doc/src/codec.rs`, `crates/ferrum-doc/src/container.rs`, `crates/ferrum-doc/src/document.rs`, `crates/ferrum-doc/src/error.rs`, `crates/ferrum-doc/src/history.rs`, `crates/ferrum-doc/src/lib.rs`, `crates/ferrum-doc/src/page.rs`, `crates/ferrum-doc/src/scene_cache.rs`, `crates/ferrum-doc/src/types.rs`, `crates/ferrum-editor/src/clipboard.rs`, `crates/ferrum-editor/src/editor.rs`, `crates/ferrum-editor/src/pages.rs`, `crates/ferrum-wasm/src/core.rs`, `scripts/compat-old-build.sh`
- Test: `crates/ferrum-doc/tests/container.rs`, `crates/ferrum-doc/tests/flip.rs`, `crates/ferrum-doc/tests/guides.rs` (новый), `crates/ferrum-doc/tests/journal.rs`, `crates/ferrum-editor/tests/clipboard_format.rs`, `crates/ferrum-editor/tests/context_menu.rs`, `crates/ferrum-editor/tests/keymap.rs`, `crates/ferrum-wasm/tests/context_menu.rs`

**Interfaces:**
- Consumes: `PageDoc::{begin_journal, take_journal, apply_inverse, set_props}`, `Tx::set_props`, `History`, `HistoryEntry`, `Document::{writable, ensure_page, transact}`, `PropsPatch`, `codec::key`, `SCENE_CACHE_VERSION`, `LoroTree::mov_to`.
- Produces: `ferrum_doc::{Guide { axis, offset }, GuideAxis, MAX_GUIDES, MAX_GUIDE_OFFSET, check_guides, dedup_guides, GuideOwner}`; `Props.guides`, `PropsPatch.guides: Option<Arc<Vec<Guide>>>`; `DocError::InvalidGuides(String)`; `Document::{page_guides, guides_revision, last_step}`, `Document::set_guides(page, selection, edits: &[(GuideOwner, Vec<Guide>)])`, `Document::move_page(id, to, current, selection)`, `Document::duplicate_page(id, name, from, selection)` (изменён: шаг); `History::{top_undo_id, remap_page, has_redo}`, `HistoryEntry { id, page_after, doc_ops, … }`, `pub(crate) DocOp`; `EditorEvent::PagesChanged`; `SCENE_CACHE_VERSION = 6`; только для тестов — `Document::force_page_guides_for_tests`, `PageDoc::force_guides_for_tests`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-doc/tests/container.rs` заменить:

````rust
                    flip_x: Some(i % 2 == 0),
                    flip_y: Some(i % 3 == 1),
                    fills: Some(vec![
                        Fill::solid(Color::rgba(i as u8, 2, 3, 4)),
````

на:

````rust
                    flip_x: Some(i % 2 == 0),
                    flip_y: Some(i % 3 == 1),
                    guides: None,
                    fills: Some(vec![
                        Fill::solid(Color::rgba(i as u8, 2, 3, 4)),
````

В `crates/ferrum-doc/tests/flip.rs` заменить:

````rust
fn scene_cache_version_covers_flips() {
    assert_eq!(
        SCENE_CACHE_VERSION, 5,
        "таблица с флагами отражения (2), текстом (3), стилем обводки и сетью (4), булевой группой (5)"
    );
    // Кэш прежней версии с честным хэшем снапшота игнорируется без предупреждений.
````

на:

````rust
fn scene_cache_version_covers_flips() {
    assert_eq!(
        SCENE_CACHE_VERSION, 6,
        "таблица с флагами отражения (2), текстом (3), стилем обводки и сетью (4), булевой группой (5), направляющими (6)"
    );
    // Кэш прежней версии с честным хэшем снапшота игнорируется без предупреждений.
````

Создать `crates/ferrum-doc/tests/guides.rs`:

````rust
//! Направляющие в документе и шаги отмены страниц (спека подсистемы 2, разделы 9–11).

use ferrum_doc::container::{decode, encode, testing};
use ferrum_doc::{
    DocError, Document, Guide, GuideAxis, GuideOwner, NodeId, NodeKind, PageId, PropsPatch,
    MAX_GUIDES,
};

fn gx(v: f64) -> Guide {
    Guide::new(GuideAxis::X, v)
}
fn gy(v: f64) -> Guide {
    Guide::new(GuideAxis::Y, v)
}

fn doc_with_frame() -> (Document, PageId, NodeId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |tx| {
            let f = tx.create_node(None, None, NodeKind::Frame)?;
            let r = tx.create_node(None, None, NodeKind::Rect)?;
            Ok(vec![f, r])
        })
        .unwrap();
    doc.clear_history();
    (doc, page, ids[0], ids[1])
}

fn frame_guides(doc: &mut Document, page: PageId, id: NodeId) -> Vec<Guide> {
    let p = doc.page(page).unwrap().props(id).unwrap();
    p.guides.map(|g| g.to_vec()).unwrap_or_default()
}

#[test]
fn each_edit_is_one_undo_step_for_page_and_frame() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(page, vec![], &[(GuideOwner::Page(page), vec![gx(10.0)])])
        .unwrap();
    assert_eq!(doc.page_guides(page), [gx(10.0)]);
    let first = doc.last_step().expect("шаг есть");
    doc.set_guides(page, vec![], &[(GuideOwner::Frame(frame), vec![gy(5.0)])])
        .unwrap();
    assert_eq!(frame_guides(&mut doc, page, frame), [gy(5.0)]);
    assert_ne!(doc.last_step(), Some(first));

    doc.undo().unwrap();
    assert!(frame_guides(&mut doc, page, frame).is_empty());
    assert_eq!(
        doc.page_guides(page),
        [gx(10.0)],
        "шаг страницы ещё не отменён"
    );
    doc.undo().unwrap();
    assert!(doc.page_guides(page).is_empty());
    assert!(!doc.can_undo(), "ровно два шага");
    doc.redo().unwrap();
    doc.redo().unwrap();
    assert_eq!(doc.page_guides(page), [gx(10.0)]);
    assert_eq!(frame_guides(&mut doc, page, frame), [gy(5.0)]);
}

#[test]
fn moving_a_guide_between_page_and_frame_is_one_step() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(page, vec![], &[(GuideOwner::Page(page), vec![gx(10.0)])])
        .unwrap();
    doc.clear_history();
    doc.set_guides(
        page,
        vec![],
        &[
            (GuideOwner::Page(page), vec![]),
            (GuideOwner::Frame(frame), vec![gx(3.0)]),
        ],
    )
    .unwrap();
    assert!(doc.page_guides(page).is_empty());
    assert_eq!(frame_guides(&mut doc, page, frame), [gx(3.0)]);
    doc.undo().unwrap();
    assert!(!doc.can_undo());
    assert_eq!(doc.page_guides(page), [gx(10.0)]);
    assert!(frame_guides(&mut doc, page, frame).is_empty());
}

#[test]
fn same_guides_make_no_step() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(page, vec![], &[(GuideOwner::Page(page), vec![])])
        .unwrap();
    doc.set_guides(page, vec![], &[(GuideOwner::Frame(frame), vec![])])
        .unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn failed_edit_leaves_no_step_and_no_change() {
    let (mut doc, page, frame, rect) = doc_with_frame();
    let too_many = vec![gx(1.0); MAX_GUIDES + 1];
    for (owner, bad) in [
        (GuideOwner::Page(page), too_many.clone()),
        (GuideOwner::Frame(frame), too_many),
        (GuideOwner::Page(page), vec![gx(f64::NAN)]),
        (GuideOwner::Frame(frame), vec![gy(f64::INFINITY)]),
        (GuideOwner::Frame(frame), vec![gy(1.5e9)]),
    ] {
        let err = doc.set_guides(page, vec![], &[(owner, bad)]);
        assert!(matches!(err, Err(DocError::InvalidGuides(_))), "{err:?}");
    }
    // Не фрейм — отказ; и первая часть правки не остаётся.
    let err = doc.set_guides(
        page,
        vec![],
        &[
            (GuideOwner::Page(page), vec![gx(1.0)]),
            (GuideOwner::Frame(rect), vec![gx(1.0)]),
        ],
    );
    assert!(err.is_err());
    assert!(doc.page_guides(page).is_empty());
    assert!(!doc.can_undo());
    assert!(!doc.is_read_only());
}

#[test]
fn duplicates_are_dropped_on_write() {
    let (mut doc, page, _, _) = doc_with_frame();
    doc.set_guides(
        page,
        vec![],
        &[(GuideOwner::Page(page), vec![gx(1.0), gx(1.0), gy(1.0)])],
    )
    .unwrap();
    assert_eq!(doc.page_guides(page), [gx(1.0), gy(1.0)]);
}

#[test]
fn guides_patch_is_ignored_on_other_kinds() {
    let (mut doc, page, _, rect) = doc_with_frame();
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            rect,
            &PropsPatch {
                guides: Some(std::sync::Arc::new(vec![gx(1.0)])),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    assert!(doc
        .page(page)
        .unwrap()
        .props(rect)
        .unwrap()
        .guides
        .is_none());
    assert!(!doc.can_undo(), "правки нет — шага нет");
}

#[test]
fn guides_survive_save_open_and_scene_cache() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(
        page,
        vec![],
        &[
            (GuideOwner::Page(page), vec![gx(-12.5), gy(7.0)]),
            (GuideOwner::Frame(frame), vec![gy(5.0)]),
        ],
    )
    .unwrap();
    let bytes = encode(&doc).unwrap();
    let mut opened = decode(&bytes).unwrap();
    assert!(opened.warnings.is_empty());
    let d = &mut opened.document;
    assert_eq!(
        d.page_guides(page),
        [gx(-12.5), gy(7.0)],
        "без загрузки страницы"
    );
    let rows = d.cached_scene(page).expect("кэш принят");
    let row = rows.iter().find(|r| r.id == frame).unwrap();
    assert_eq!(row.props.guides.as_deref(), Some(&vec![gy(5.0)]));
    assert_eq!(frame_guides(d, page, frame), [gy(5.0)]);
    // И прямо из Loro, без кэша.
    let stale = testing::map_sections(&bytes, |kind, _, data| {
        if kind == 5 {
            data[32] = 1;
        }
    });
    let mut d = decode(&stale).unwrap().document;
    assert!(d.cached_scene(page).is_none());
    assert_eq!(frame_guides(&mut d, page, frame), [gy(5.0)]);
}

#[test]
fn bad_guides_in_file_are_dropped_and_document_stays_writable() {
    let (mut doc, page, frame, rect) = doc_with_frame();
    let raw = [
        ("x", 1.0),
        ("q", 2.0),
        ("y", f64::NAN),
        ("y", f64::NEG_INFINITY),
        ("y", 2e9),
        ("x", 1.0),
    ];
    doc.force_page_guides_for_tests(page, &raw);
    let p = doc.page(page).unwrap();
    p.force_guides_for_tests(frame, &raw);
    p.force_guides_for_tests(rect, &[("x", 1.0)]);
    let bytes = encode(&doc).unwrap();
    let no_cache = testing::map_sections(&bytes, |kind, _, data| {
        if kind == 5 {
            data[32] = 1;
        }
    });
    let mut d = decode(&no_cache).unwrap().document;
    assert_eq!(d.page_guides(page), [gx(1.0)]);
    assert_eq!(frame_guides(&mut d, page, frame), [gx(1.0)]);
    let p = d.page(page).unwrap().props(rect).unwrap();
    assert!(
        p.guides.is_none(),
        "у прямоугольника направляющих не бывает"
    );
    assert!(!d.is_read_only());
}

#[test]
fn more_than_the_limit_in_file_is_cut() {
    let (mut doc, page, frame, _) = doc_with_frame();
    let offsets: Vec<f64> = (0..MAX_GUIDES + 10).map(|i| i as f64).collect();
    let raw: Vec<(&str, f64)> = offsets.iter().map(|o| ("x", *o)).collect();
    doc.force_page_guides_for_tests(page, &raw);
    doc.page(page).unwrap().force_guides_for_tests(frame, &raw);
    assert_eq!(doc.page_guides(page).len(), MAX_GUIDES);
    let bytes = encode(&doc).unwrap();
    let no_cache = testing::map_sections(&bytes, |kind, _, data| {
        if kind == 5 {
            data[32] = 1;
        }
    });
    let mut d = decode(&no_cache).unwrap().document;
    assert_eq!(frame_guides(&mut d, page, frame).len(), MAX_GUIDES);
}

#[test]
fn nan_in_scene_cache_rejects_the_cache_not_the_file() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(page, vec![], &[(GuideOwner::Frame(frame), vec![gy(5.0)])])
        .unwrap();
    // Фрейм создан первым: подменяем смещение 5.0 внутри строки таблицы.
    let forged = testing::map_sections(&encode(&doc).unwrap(), |kind, _, data| {
        if kind == 5 {
            let five = 5.0f64.to_le_bytes();
            let at = data
                .windows(8)
                .rposition(|w| w == five)
                .expect("смещение в таблице");
            data[at..at + 8].copy_from_slice(&f64::NAN.to_le_bytes());
        }
    });
    let mut d = decode(&forged).unwrap().document;
    assert!(d.cached_scene(page).is_none(), "таблица отброшена");
    assert_eq!(
        frame_guides(&mut d, page, frame),
        [gy(5.0)],
        "значение — из Loro"
    );
}

#[test]
fn guides_edit_on_unloaded_page_with_unknown_nodes_is_refused() {
    // Файл с повреждённым узлом на незагруженной странице.
    let mut src = Document::new();
    let other = src.add_page("Вторая").unwrap();
    let v = src
        .transact(other, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Vector)?])
        })
        .unwrap()[0];
    src.page(other)
        .unwrap()
        .force_vector_bytes_for_tests(v, b"not a network");
    src.page(other).unwrap().commit_for_tests();
    let file = encode(&src).unwrap();
    let no_cache = testing::map_sections(&file, |kind, _, data| {
        if kind == 5 {
            data[32] = 1;
        }
    });
    let mut d = decode(&no_cache).unwrap().document;
    assert!(!d.is_page_loaded(other));
    assert!(!d.is_read_only());
    let err = d.set_guides(other, vec![], &[(GuideOwner::Page(other), vec![gx(1.0)])]);
    assert!(matches!(err, Err(DocError::ReadOnly)), "{err:?}");
    assert!(d.page_guides(other).is_empty());
    assert!(!d.can_undo());
}

#[test]
fn undo_returns_to_the_page_of_the_guide_step() {
    let (mut doc, page, _, _) = doc_with_frame();
    let other = doc.add_page("Вторая").unwrap();
    doc.set_guides(other, vec![], &[(GuideOwner::Page(other), vec![gx(1.0)])])
        .unwrap();
    let r = doc.undo().unwrap().unwrap();
    assert_eq!(r.page, other);
    assert!(doc.page_guides(other).is_empty());
    let r = doc.redo().unwrap().unwrap();
    assert_eq!(r.page, other);
    assert_eq!(doc.page_guides(other), [gx(1.0)]);
    assert!(doc.page_guides(page).is_empty());
}

#[test]
fn guides_survive_the_journal() {
    // Журнал: направляющие страницы — в пачке корня и переживают восстановление.
    let (doc, page, frame, _) = doc_with_frame();
    let file = encode(&doc).unwrap();
    let mut opened = decode(&file).unwrap().document;
    opened.page(page).unwrap();
    opened.mark_journal();
    opened
        .set_guides(
            page,
            vec![],
            &[
                (GuideOwner::Page(page), vec![gx(9.0)]),
                (GuideOwner::Frame(frame), vec![gy(8.0)]),
            ],
        )
        .unwrap();
    let batch = opened.take_journal().unwrap().expect("пачка");
    let mut back = decode(&file).unwrap().document;
    let replayed = back.apply_journal(&[&batch]);
    assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
    assert_eq!(back.page_guides(page), [gx(9.0)]);
    assert_eq!(frame_guides(&mut back, page, frame), [gy(8.0)]);
}

#[test]
fn page_move_is_one_step_and_same_place_none() {
    let mut doc = Document::new();
    let a = doc.pages()[0].0;
    let b = doc.add_page("B").unwrap();
    let c = doc.add_page("C").unwrap();
    let order = |d: &Document| d.pages().iter().map(|p| p.0).collect::<Vec<_>>();
    doc.clear_history();
    doc.move_page(a, 0, a, vec![]).unwrap();
    assert!(!doc.can_undo(), "то же место — без шага");
    doc.move_page(a, 2, a, vec![]).unwrap();
    assert_eq!(order(&doc), [b, c, a]);
    doc.move_page(c, 0, a, vec![]).unwrap();
    assert_eq!(order(&doc), [c, b, a]);
    doc.undo().unwrap();
    assert_eq!(order(&doc), [b, c, a]);
    doc.undo().unwrap();
    assert_eq!(order(&doc), [a, b, c]);
    assert!(!doc.can_undo());
    doc.redo().unwrap();
    doc.redo().unwrap();
    assert_eq!(order(&doc), [c, b, a]);
    // За концом — на последнее место.
    doc.move_page(c, 99, a, vec![]).unwrap();
    assert_eq!(order(&doc), [b, a, c]);
}

#[test]
fn page_order_survives_save_and_journal() {
    let mut doc = Document::new();
    let a = doc.pages()[0].0;
    let b = doc.add_page("B").unwrap();
    let file = encode(&doc).unwrap();
    let mut opened = decode(&file).unwrap().document;
    opened.page(a).unwrap();
    opened.page(b).unwrap();
    opened.mark_journal();
    opened.move_page(b, 0, a, vec![]).unwrap();
    let batch = opened.take_journal().unwrap().expect("пачка");
    let mut back = decode(&file).unwrap().document;
    back.apply_journal(&[&batch]);
    assert_eq!(back.pages()[0].0, b);
    let resaved = decode(&encode(&opened).unwrap()).unwrap().document;
    assert_eq!(resaved.pages()[0].0, b);
}

#[test]
fn duplicate_page_undo_redo_keeps_copy_and_later_steps() {
    let (mut doc, page, frame, _) = doc_with_frame();
    doc.set_guides(
        page,
        vec![],
        &[
            (GuideOwner::Page(page), vec![gx(1.0)]),
            (GuideOwner::Frame(frame), vec![gy(2.0)]),
        ],
    )
    .unwrap();
    doc.clear_history();
    let copy = doc
        .duplicate_page(page, "Копия", page, vec![frame])
        .unwrap();
    assert_eq!(doc.pages().len(), 2);
    assert_eq!(doc.page_guides(copy), [gx(1.0)]);
    let made = doc
        .transact(copy, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Ellipse)?])
        })
        .unwrap()[0];

    doc.undo().unwrap();
    let r = doc.undo().unwrap().unwrap();
    assert_eq!(r.page, page, "отмена — на страницу, с которой дублировали");
    assert_eq!(r.selection, [frame]);
    assert_eq!(doc.pages().len(), 1);
    assert!(!doc.can_undo());

    let r = doc.redo().unwrap().unwrap();
    assert_eq!(doc.pages().len(), 2);
    let back = doc.pages()[1].0;
    assert_eq!(r.page, back, "повтор — на копию");
    assert_eq!(doc.pages()[1].1, "Копия");
    assert_eq!(doc.page_guides(back), [gx(1.0)]);
    // Следующий шаг повтора — про ту же копию, под её новым идентификатором.
    let r = doc.redo().unwrap().unwrap();
    assert_eq!(r.page, back);
    assert!(doc.page(back).unwrap().contains(made));
    // И снова назад: цепочка не рвётся.
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.pages().len(), 1);
}

#[test]
fn undone_copy_is_not_in_the_file_and_redone_is() {
    let (mut doc, page, _, _) = doc_with_frame();
    doc.duplicate_page(page, "Копия", page, vec![]).unwrap();
    doc.undo().unwrap();
    assert_eq!(
        decode(&encode(&doc).unwrap())
            .unwrap()
            .document
            .pages()
            .len(),
        1
    );
    doc.redo().unwrap();
    let mut d = decode(&encode(&doc).unwrap()).unwrap().document;
    let pages = d.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(d.page(pages[1].0).unwrap().walk().len(), 2, "узлы копии");
}

#[test]
fn new_step_forgets_the_parked_copy() {
    let (mut doc, page, _, _) = doc_with_frame();
    doc.duplicate_page(page, "Копия", page, vec![]).unwrap();
    doc.undo().unwrap();
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_node(None, None, NodeKind::Ellipse)?])
    })
    .unwrap();
    assert!(!doc.can_redo());
    doc.undo().unwrap();
    assert_eq!(doc.pages().len(), 1);
    assert!(!doc.can_undo());
}

#[test]
fn deleting_a_page_forgets_steps_that_rely_on_it() {
    let mut doc = Document::new();
    let a = doc.pages()[0].0;
    let b = doc.add_page("B").unwrap();
    doc.move_page(b, 0, a, vec![]).unwrap();
    doc.delete_page(b).unwrap();
    assert!(!doc.can_undo(), "шаг перестановки удалённой страницы забыт");
    let copy = doc.duplicate_page(a, "Копия", a, vec![]).unwrap();
    doc.delete_page(a).unwrap();
    assert!(!doc.can_undo(), "дубль удалённой страницы забыт");
    assert_eq!(doc.pages()[0].0, copy);
}

#[test]
fn step_number_survives_undo_redo_and_changes_with_new_step() {
    let (mut doc, page, _, _) = doc_with_frame();
    assert_eq!(doc.last_step(), None);
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    let id = doc.last_step().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.last_step(), None);
    doc.redo().unwrap();
    assert_eq!(doc.last_step(), Some(id));
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap();
    assert_ne!(doc.last_step(), Some(id));
}

#[test]
fn duplicate_page_undo_redo_survive_the_journal() {
    let (doc, page, _, _) = doc_with_frame();
    let file = encode(&doc).unwrap();
    let mut open = decode(&file).unwrap().document;
    open.page(page).unwrap();
    open.mark_journal();
    let copy = open.duplicate_page(page, "Копия", page, vec![]).unwrap();
    let dup = open.take_journal().unwrap().expect("пачка дубля");
    open.undo().unwrap();
    let undo = open.take_journal().unwrap().expect("пачка отмены");
    open.redo().unwrap();
    let redo = open.take_journal().unwrap().expect("пачка повтора");

    let mut a = decode(&file).unwrap().document;
    a.apply_journal(&[&dup, &undo]);
    assert_eq!(a.pages().len(), 1, "отменённой копии нет");

    let mut b = decode(&file).unwrap().document;
    let replayed = b.apply_journal(&[&dup, &undo, &redo]);
    assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
    let pages = b.pages();
    assert_eq!(pages.len(), 2);
    assert_ne!(
        pages[1].0, copy,
        "повтор — новый идентификатор в дереве страниц"
    );
    assert_eq!(pages[1].1, "Копия");
    assert_eq!(b.page(pages[1].0).unwrap().walk().len(), 2, "узлы копии");
}
````

В `crates/ferrum-doc/tests/journal.rs` заменить:

````rust
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 4.0);
    let copy = doc.duplicate_page(page, "Копия").unwrap();
    assert_eq!(doc.pages()[1], (copy, "Копия".to_string()));
    // Копия правится независимо от оригинала.
````

на:

````rust
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 4.0);
    let copy = doc.duplicate_page(page, "Копия", page, vec![]).unwrap();
    assert_eq!(doc.pages()[1], (copy, "Копия".to_string()));
    // Копия правится независимо от оригинала.
````

В `crates/ferrum-editor/tests/clipboard_format.rs` заменить:

````rust
    assert!(foreign.nodes[0].props.boolean_op.is_none());
}
````

на:

````rust
    assert!(foreign.nodes[0].props.boolean_op.is_none());
}

#[test]
fn frame_guides_go_with_the_copy_and_are_validated() {
    use ferrum_doc::{Guide, GuideAxis, MAX_GUIDES};
    use std::sync::Arc;
    let mut p = sample();
    let guides = vec![
        Guide::new(GuideAxis::X, 5.0),
        Guide::new(GuideAxis::Y, -1.5),
    ];
    p.nodes[0].props.guides = Some(Arc::new(guides.clone()));
    let bytes = p.encode();
    let got = ClipPayload::decode(&bytes).unwrap();
    assert_eq!(got.nodes[0].props.guides.as_deref(), Some(&guides));
    assert!(json_of(&bytes).contains("\"guides\""));
    // Без направляющих поля нет: копия побайтно как у прежних сборок.
    assert!(!json_of(&sample().encode()).contains("guides"));

    let with_guides = |list: String| {
        let json = json_of(&bytes);
        let at = json.find("\"guides\":[").unwrap();
        let end = at + json[at..].find(']').unwrap() + 1;
        frame_bytes(
            &format!("{}\"guides\":{list}{}", &json[..at], &json[end..]),
            &[SAMPLE_IMAGE.to_vec()],
        )
    };
    assert!(ClipPayload::decode(&with_guides(r#"[{"axis":"q","offset":1}]"#.into())).is_err());
    assert!(ClipPayload::decode(&with_guides(r#"[{"axis":"x","offset":2e9}]"#.into())).is_err());
    let many: Vec<String> = (0..=MAX_GUIDES)
        .map(|i| format!(r#"{{"axis":"x","offset":{i}}}"#))
        .collect();
    assert!(ClipPayload::decode(&with_guides(format!("[{}]", many.join(",")))).is_err());
    // У не фрейма поле игнорируется.
    let json = json_of(&sample().encode()).replacen(
        "\"kind\":\"line\"",
        "\"guides\":[{\"axis\":\"x\",\"offset\":1}],\"kind\":\"line\"",
        1,
    );
    let got = ClipPayload::decode(&frame_bytes(&json, &[SAMPLE_IMAGE.to_vec()])).unwrap();
    assert!(got.nodes[2].props.guides.is_none());
}
````

В `crates/ferrum-editor/tests/context_menu.rs` заменить:

````rust
    assert_eq!(pages[1].1, "Страница 1 (копия)");
    assert_eq!(h.ed.page(), copy, "копия — текущая");
    assert_eq!(h.ed.step_count(), steps, "в историю отмены не попадает");
    assert_eq!(h.ed.page_background(), Some(Color::rgba(10, 20, 30, 255)));
    let after = h.state();
````

на:

````rust
    assert_eq!(pages[1].1, "Страница 1 (копия)");
    assert_eq!(h.ed.page(), copy, "копия — текущая");
    assert_eq!(h.ed.step_count(), steps + 1, "один шаг отмены");
    assert_eq!(h.ed.page_background(), Some(Color::rgba(10, 20, 30, 255)));
    let after = h.state();
````

В `crates/ferrum-editor/tests/context_menu.rs` заменить:

````rust
    assert_eq!(h.state(), before);
}
````

на:

````rust
    assert_eq!(h.state(), before);
}

#[test]
fn duplicate_page_undo_returns_and_redo_opens_the_copy() {
    let mut h = H::new();
    let first = h.ed.page();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    h.ed.select(&[f]);
    let copy = h.ed.duplicate_page(first).unwrap();
    h.ed.take_events();
    assert_eq!(h.ed.page(), copy);
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.ed.page(), first, "копия была текущей: назад на исходную");
    assert_eq!(h.ed.selection(), [f], "выделение на момент дубля");
    assert_eq!(h.ed.document().pages().len(), 1);
    let ev = h.ed.take_events();
    assert!(ev.contains(&EditorEvent::PagesChanged), "{ev:?}");
    assert!(ev
        .iter()
        .any(|e| matches!(e, EditorEvent::PageChanged(p) if *p == first)));
    h.ed.execute(CommandId::Redo);
    assert_eq!(h.ed.document().pages().len(), 2);
    assert_eq!(h.ed.page(), h.ed.document().pages()[1].0);
    assert!(h.ed.take_events().contains(&EditorEvent::PagesChanged));
    assert_eq!(h.state().len(), 1, "сцена копии на месте");
}
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(sc("Ctrl+Shift+Slash").label(Platform::Mac), "⌃⇧/");
    assert_eq!(sc("Ctrl+Shift+Slash").label(Platform::Linux), "Ctrl+Shift+/");
}
````

на:

````rust
    }
    assert_eq!(sc("Ctrl+Shift+Slash").label(Platform::Mac), "⌃⇧/");
    assert_eq!(
        sc("Ctrl+Shift+Slash").label(Platform::Linux),
        "Ctrl+Shift+/"
    );
}
````

В `crates/ferrum-wasm/tests/context_menu.rs` заменить:

````rust
    assert_eq!(current, list[1].id);
    assert_eq!(layer_rows(&mut core).0, 1, "копия с фреймом оригинала");
}
````

на:

````rust
    assert_eq!(current, list[1].id);
    assert_eq!(layer_rows(&mut core).0, 1, "копия с фреймом оригинала");
    // Один шаг отмены: список страниц приходит и после отмены, и после повтора.
    core.take_events();
    core.command(Command::Execute {
        command: "edit.undo".into(),
    });
    let (list, current) = pages(&core.take_events());
    assert_eq!(list.len(), 2);
    assert_eq!(
        current, list[1].id,
        "назад на страницу, с которой дублировали"
    );
    core.command(Command::Execute {
        command: "edit.redo".into(),
    });
    let (list, current) = pages(&core.take_events());
    assert_eq!(list.len(), 3);
    assert_eq!(current, list[1].id);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc --test guides`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test clipboard_format`
Expected: FAIL — `ferrum-doc --test guides` не собирается: нет `ferrum_doc::{Guide, GuideAxis, GuideOwner, MAX_GUIDES, …}`, `Document::{page_guides, set_guides, last_step, move_page}`, поля `Props.guides`, `DocError::InvalidGuides`, у `duplicate_page` 2 аргумента вместо 4; `ferrum-editor --test clipboard_format` не собирается: нет `ferrum_doc::{Guide, GuideAxis, MAX_GUIDES}` и `Props.guides`

- [ ] **Step 3: Реализация**

Создать `crates/ferrum-doc/examples/guides_fixture.rs`:

````rust
//! Файл `.ferrum` с направляющими у страницы и фрейма — для проверки совместимости:
//! сборка без направляющих открывает его как обычно, для записи, неизвестный ключ
//! просто не читается (спека подсистемы 2, раздел 9). Проверку старой сборкой делает
//! `scripts/compat-old-build.sh <ref> guides_fixture`.
//! `cargo run -p ferrum-doc --example guides_fixture -- <путь>` — записать файл;
//! `… -- --check <путь>` — файл, пересохранённый старой сборкой, хранит направляющие.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::{Document, Guide, GuideAxis, GuideOwner, NodeId, NodeKind};

fn expected_page() -> Vec<Guide> {
    vec![
        Guide::new(GuideAxis::X, 100.0),
        Guide::new(GuideAxis::Y, -40.5),
    ]
}

fn expected_frame() -> Vec<Guide> {
    vec![Guide::new(GuideAxis::Y, 25.0)]
}

fn check(path: &str) {
    let bytes = std::fs::read(path).expect("файл");
    let mut doc = decode(&bytes).expect("файл читается").document;
    let page = doc.pages()[0].0;
    assert_eq!(
        doc.page_guides(page),
        expected_page(),
        "направляющие страницы"
    );
    let p = doc.page(page).expect("страница");
    let frame: NodeId = p
        .walk()
        .into_iter()
        .find(|id| p.kind(*id).ok() == Some(NodeKind::Frame))
        .expect("фрейм");
    let props = p.props(frame).expect("свойства");
    assert_eq!(
        props.guides.as_deref(),
        Some(&expected_frame()),
        "направляющие фрейма"
    );
    assert_eq!(p.walk().len(), 3, "старая сборка дописала эллипс");
    println!("{path}: направляющие на месте");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next().expect("путь к файлу");
    if first == "--check" {
        check(&args.next().expect("путь к файлу"));
        return;
    }
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let frame = doc
        .transact(page, vec![], |tx| {
            tx.create_node(None, None, NodeKind::Rect)?;
            Ok(vec![tx.create_node(None, None, NodeKind::Frame)?])
        })
        .unwrap()[0];
    doc.set_guides(
        page,
        vec![],
        &[
            (GuideOwner::Page(page), expected_page()),
            (GuideOwner::Frame(frame), expected_frame()),
        ],
    )
    .unwrap();
    std::fs::write(&first, encode(&doc).unwrap()).unwrap();
    println!("{first}: прямоугольник, фрейм и направляющие");
}
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
use crate::boolean::BooleanOp;
use crate::error::{DocError, DocResult};
use crate::text::TextData;
use crate::text_codec;
````

на:

````rust
use crate::boolean::BooleanOp;
use crate::error::{DocError, DocResult};
use crate::guides::{guides_value, read_guides};
use crate::text::TextData;
use crate::text_codec;
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
    /// Операция узла `boolean` — строка (`BooleanOp::as_str`).
    pub const BOOLEAN_OP: &str = "booleanOp";
}

````

на:

````rust
    /// Операция узла `boolean` — строка (`BooleanOp::as_str`).
    pub const BOOLEAN_OP: &str = "booleanOp";
    /// Направляющие фрейма и страницы — список значений (`guides.rs`).
    pub const GUIDES: &str = "guides";
}

````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
            _ => None,
        },
    })
}
````

на:

````rust
            _ => None,
        },
        // Только у фрейма; неверные записи отброшены (`read_guides`).
        guides: match kind {
            NodeKind::Frame => value(map, key::GUIDES)
                .map(|v| read_guides(&v))
                .filter(|g| !g.is_empty())
                .map(Arc::new),
            _ => None,
        },
    })
}
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
        vector: p.vector.clone(),
        boolean_op: p.boolean_op,
    };
    write_patch(map, &patch)?;
````

на:

````rust
        vector: p.vector.clone(),
        boolean_op: p.boolean_op,
        guides: p.guides.clone(),
    };
    write_patch(map, &patch)?;
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
        map.insert(key::BOOLEAN_OP, op.as_str())?;
    }
    Ok(())
}
````

на:

````rust
        map.insert(key::BOOLEAN_OP, op.as_str())?;
    }
    // Нейтральное значение (нет направляющих) не пишется, как фон страницы.
    match &p.guides {
        Some(g) if g.is_empty() => {
            if map.get(key::GUIDES).is_some() {
                map.delete(key::GUIDES)?;
            }
        }
        Some(g) => map.insert(key::GUIDES, guides_value(g))?,
        None => {}
    }
    Ok(())
}
````

В `crates/ferrum-doc/src/container.rs` заменить:

````rust
/// таблицу без флагов рядом со снапшотом, где они есть, а зеркало страницы
/// берётся из таблицы — флаги тихо пропали бы и из правок.
/// 3 — текст, 4 — стиль обводки и векторная сеть, 5 — булева группа.
pub const SCENE_CACHE_VERSION: u8 = 5;

const ROOT: u8 = 1;
````

на:

````rust
/// таблицу без флагов рядом со снапшотом, где они есть, а зеркало страницы
/// берётся из таблицы — флаги тихо пропали бы и из правок.
/// 3 — текст, 4 — стиль обводки и векторная сеть, 5 — булева группа, 6 — направляющие
/// фрейма.
pub const SCENE_CACHE_VERSION: u8 = 6;

const ROOT: u8 = 1;
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

use crate::changes::Changes;
use crate::error::{DocError, DocResult};
use crate::history::{History, HistoryEntry, TextMerge};
use crate::images::ImageStore;
use crate::journal::{self, BatchWriter, Replayed};
use crate::page::{Inverse, PageDoc, Tx};
use crate::scene_cache::SceneRow;
use crate::types::{Color, NodeId, PageId};

/// Текущая версия схемы документа. Растёт при несовместимых изменениях схемы.
````

на:

````rust

use crate::changes::Changes;
use crate::codec::key::GUIDES as KEY_GUIDES;
use crate::error::{DocError, DocResult};
use crate::guides::{check_guides, dedup_guides, guides_value, read_guides, Guide};
use crate::history::{DocOp, History, HistoryEntry, TextMerge};
use crate::images::ImageStore;
use crate::journal::{self, BatchWriter, Replayed};
use crate::page::{Inverse, PageDoc, Tx};
use crate::scene_cache::SceneRow;
use crate::types::{Color, NodeId, PageId, PropsPatch};

/// Текущая версия схемы документа. Растёт при несовместимых изменениях схемы.
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
}

pub struct Document {
    root: LoroDoc,
````

на:

````rust
}

/// Чьи направляющие: страницы или фрейма на странице шага.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuideOwner {
    Page(PageId),
    Frame(NodeId),
}

/// Что вернул повтор или отмена шага: операции обратного направления и новые
/// идентификаторы страниц.
struct Replay {
    ops: Vec<Inverse>,
    doc_ops: Vec<DocOp>,
    remap: Option<(PageId, PageId)>,
}

pub struct Document {
    root: LoroDoc,
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    journaled_root: VersionVector,
    journaled_pages: HashMap<PageId, VersionVector>,
}

````

на:

````rust
    journaled_root: VersionVector,
    journaled_pages: HashMap<PageId, VersionVector>,
    /// Копии страниц, убранные отменой «Дублировать страницу», ждут повтора. Живут,
    /// только пока есть шаги повтора.
    parked: HashMap<PageId, PageDoc>,
    /// Растёт при каждой смене направляющих страницы (кэш линий редактора):
    /// у фрейма то же самое видно по счётчику сцены.
    guides_revision: u64,
}

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            journaled_root,
            journaled_pages: HashMap::new(),
        }
    }
````

на:

````rust
            journaled_root,
            journaled_pages: HashMap::new(),
            parked: HashMap::new(),
            guides_revision: 0,
        }
    }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    /// загрузке страницы с неизвестными узлами.
    #[doc(hidden)]
    pub fn force_read_only_for_tests(&mut self) {
        self.read_only = true;
````

на:

````rust
    /// загрузке страницы с неизвестными узлами.
    #[doc(hidden)]
    /// Записывает в страницу направляющие как есть — как чужой файл (тесты разбора).
    #[doc(hidden)]
    pub fn force_page_guides_for_tests(&mut self, id: PageId, raw: &[(&str, f64)]) {
        let meta = self.pages_tree.get_meta(id.0).expect("страница есть");
        meta.insert(KEY_GUIDES, crate::guides::raw_value(raw))
            .expect("запись");
        self.root.commit();
    }

    pub fn force_read_only_for_tests(&mut self) {
        self.read_only = true;
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

    /// Копия страницы `id` с именем `name` сразу после неё в списке: те же узлы
    /// (с новыми идентификаторами) и фон. Как и остальные операции со страницами,
    /// в историю отмены не попадает; копия целиком уйдёт в следующую пачку журнала.
    pub fn duplicate_page(&mut self, id: PageId, name: &str) -> DocResult<PageId> {
        // Сначала загрузка: страница с неизвестными узлами делает документ
        // доступным только для чтения, а копия их потеряла бы.
````

на:

````rust

    /// Копия страницы `id` с именем `name` сразу после неё в списке: те же узлы
    /// (с новыми идентификаторами), фон и направляющие. Один шаг отмены (спека
    /// подсистемы 2, раздел 10): отмена возвращает на `from` с выделением
    /// `selection`, повтор — на копию. Копия целиком уйдёт в следующую пачку журнала.
    pub fn duplicate_page(
        &mut self,
        id: PageId,
        name: &str,
        from: PageId,
        selection: Vec<NodeId>,
    ) -> DocResult<PageId> {
        // Сначала загрузка: страница с неизвестными узлами делает документ
        // доступным только для чтения, а копия их потеряла бы.
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            .position(|(p, _)| *p == id)
            .map_or(0, |i| i + 1);
        let background = self.page_background(id);
        // Узлы копируются до создания страницы: отказ не оставляет пустой копии.
        let copy = PageDoc::new();
````

на:

````rust
            .position(|(p, _)| *p == id)
            .map_or(0, |i| i + 1);
        self.ensure_page(from)?;
        let background = self.page_background(id);
        let guides = self.page_guides(id);
        // Узлы копируются до создания страницы: отказ не оставляет пустой копии.
        let copy = PageDoc::new();
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            meta.insert(KEY_BACKGROUND, c.to_hex())?;
        }
        self.root.commit();
        let new = PageId(t);
        self.loaded.insert(new, copy);
        self.journaled_pages.insert(new, VersionVector::default());
        Ok(new)
    }

````

на:

````rust
            meta.insert(KEY_BACKGROUND, c.to_hex())?;
        }
        if !guides.is_empty() {
            meta.insert(KEY_GUIDES, guides_value(&guides))?;
        }
        self.root.commit();
        let new = PageId(t);
        self.loaded.insert(new, copy);
        self.journaled_pages.insert(new, VersionVector::default());
        self.push_step(HistoryEntry {
            id: 0,
            page: from,
            page_after: Some(new),
            ops: Vec::new(),
            doc_ops: vec![DocOp::RemovePage { page: new }],
            selection_before: selection,
            selection_after: Vec::new(),
        });
        Ok(new)
    }

    /// Ставит страницу на место `to` в списке (после перестановки она там и
    /// стоит). Один шаг отмены; то же место — ни шага, ни правки. Отмена возвращает
    /// на страницу `current` с выделением `selection`.
    pub fn move_page(
        &mut self,
        id: PageId,
        to: usize,
        current: PageId,
        selection: Vec<NodeId>,
    ) -> DocResult<()> {
        self.writable()?;
        self.ensure_page(id)?;
        let from = self.page_index(id)?;
        let to = to.min(self.pages().len().saturating_sub(1));
        if from == to {
            return Ok(());
        }
        self.place_page(id, to)?;
        self.push_step(HistoryEntry {
            id: 0,
            page: current,
            page_after: None,
            ops: Vec::new(),
            doc_ops: vec![DocOp::MovePage {
                page: id,
                index: from,
            }],
            selection_after: selection.clone(),
            selection_before: selection,
        });
        Ok(())
    }

    fn page_index(&self, id: PageId) -> DocResult<usize> {
        self.pages()
            .iter()
            .position(|(p, _)| *p == id)
            .ok_or_else(|| DocError::PageNotFound(id.to_string()))
    }

    fn place_page(&mut self, id: PageId, to: usize) -> DocResult<()> {
        self.pages_tree.mov_to(id.0, TreeParentId::Root, to)?;
        self.root.commit();
        Ok(())
    }

    /// Направляющие страницы. Неверные записи файла отброшены (`read_guides`);
    /// страницу загружать не нужно: список лежит в корневом документе.
    pub fn page_guides(&self, id: PageId) -> Vec<Guide> {
        match self
            .pages_tree
            .get_meta(id.0)
            .ok()
            .and_then(|m| m.get(KEY_GUIDES))
        {
            Some(ValueOrContainer::Value(v)) => read_guides(&v),
            _ => Vec::new(),
        }
    }

    /// Номер версии направляющих страниц: кэш редактора сравнивает его за O(1).
    pub fn guides_revision(&self) -> u64 {
        self.guides_revision
    }

    fn write_page_guides(&mut self, id: PageId, guides: &[Guide]) -> DocResult<()> {
        let meta = self.pages_tree.get_meta(id.0)?;
        if guides.is_empty() {
            if meta.get(KEY_GUIDES).is_some() {
                meta.delete(KEY_GUIDES)?;
            }
        } else {
            meta.insert(KEY_GUIDES, guides_value(guides))?;
        }
        self.root.commit();
        self.guides_revision += 1;
        Ok(())
    }

    /// Новые списки направляющих одним шагом отмены (перенос из страницы во фрейм
    /// — два списка). Без изменений шага нет. Неверный список, неизвестный узел и
    /// узел не того типа — отказ целиком: ни правки, ни шага. Сначала загрузка
    /// страницы: на незагруженной могут быть неизвестные узлы, и документ станет
    /// доступен только для чтения.
    pub fn set_guides(
        &mut self,
        page: PageId,
        selection: Vec<NodeId>,
        edits: &[(GuideOwner, Vec<Guide>)],
    ) -> DocResult<()> {
        self.page(page)?;
        self.writable()?;
        let mut frames = Vec::new();
        let mut doc_ops = Vec::new();
        let mut page_edit: Vec<(PageId, Vec<Guide>)> = Vec::new();
        for (owner, guides) in edits {
            check_guides(guides)?;
            let guides = dedup_guides(guides.iter().copied());
            match *owner {
                GuideOwner::Page(id) => {
                    self.ensure_page(id)?;
                    if id != page {
                        return Err(DocError::PageNotFound(id.to_string()));
                    }
                    let before = self.page_guides(id);
                    if before != guides {
                        doc_ops.push(DocOp::PageGuides {
                            page: id,
                            guides: before,
                        });
                        page_edit.push((id, guides));
                    }
                }
                GuideOwner::Frame(id) => {
                    let p = self.loaded.get(&page).expect("страница загружена выше");
                    if p.kind(id)? != crate::types::NodeKind::Frame {
                        return Err(DocError::InvalidGuides("у этого узла их не бывает".into()));
                    }
                    frames.push((id, guides));
                }
            }
        }
        let p = self.loaded.get(&page).expect("страница загружена выше");
        p.begin_journal();
        let tx = Tx::new(p);
        let result = frames.iter().try_for_each(|(id, guides)| {
            tx.set_props(
                *id,
                &PropsPatch {
                    guides: Some(std::sync::Arc::new(guides.clone())),
                    ..Default::default()
                },
            )
        });
        let ops = p.take_journal();
        p.commit();
        self.changes.entry(page).or_default().record(&ops);
        if let Err(e) = result {
            return Err(rollback(p, &ops, e));
        }
        for (id, guides) in &page_edit {
            self.write_page_guides(*id, guides)?;
        }
        if !ops.is_empty() || !doc_ops.is_empty() {
            self.push_step(HistoryEntry {
                id: 0,
                page,
                page_after: None,
                ops,
                doc_ops,
                selection_after: selection.clone(),
                selection_before: selection,
            });
        }
        Ok(())
    }

    /// Новый шаг сбрасывает повтор, а с ним — припаркованные копии страниц.
    fn push_step(&mut self, entry: HistoryEntry) {
        self.history.push(entry);
        self.forget_parked();
    }

    fn forget_parked(&mut self) {
        if !self.history.has_redo() {
            self.parked.clear();
        }
    }

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        self.unloaded.remove(&id);
        self.history.forget_page(id);
        self.changes.remove(&id);
        self.journaled_pages.remove(&id);
````

на:

````rust
        self.unloaded.remove(&id);
        self.history.forget_page(id);
        self.forget_parked();
        self.changes.remove(&id);
        self.journaled_pages.remove(&id);
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
                if !ops.is_empty() {
                    let entry = HistoryEntry {
                        page,
                        ops,
                        selection_before,
                        selection_after: selection_after.clone(),
````

на:

````rust
                if !ops.is_empty() {
                    let entry = HistoryEntry {
                        id: 0,
                        page,
                        page_after: None,
                        ops,
                        doc_ops: Vec::new(),
                        selection_before,
                        selection_after: selection_after.clone(),
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
                        None => self.history.push(entry),
                    }
                }
                Ok(selection_after)
````

на:

````rust
                        None => self.history.push(entry),
                    }
                    self.forget_parked();
                }
                Ok(selection_after)
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    pub fn clear_history(&mut self) {
        self.history = History::default();
    }

````

на:

````rust
    pub fn clear_history(&mut self) {
        self.history = History::default();
        self.parked.clear();
    }

    /// Номер последнего шага отмены: тот же шаг остаётся тем же номером и после
    /// отмены с повтором; новый шаг — новый номер (спека подсистемы 2, раздел 10).
    pub fn last_step(&self) -> Option<u64> {
        self.history.top_undo_id()
    }

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        }
        self.mark_journal();
        out
    }
````

на:

````rust
        }
        self.mark_journal();
        // Журнал корня мог сменить направляющие страниц.
        self.guides_revision += 1;
        out
    }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    /// Применяет операции шага и возвращает операции для обратного направления.
    /// Шаг не изменяется: при ошибке вызывающий код вернёт его в стек целиком.
    fn replay(&mut self, entry: &HistoryEntry) -> DocResult<Vec<Inverse>> {
        self.page(entry.page)?;
        let p = self.loaded.get(&entry.page).expect("загружена");
        // При сбое apply_inverse сам откатывает уже применённую часть шага.
````

на:

````rust
    /// Применяет операции шага и возвращает операции для обратного направления.
    /// Шаг не изменяется: при ошибке вызывающий код вернёт его в стек целиком.
    fn replay(&mut self, entry: &HistoryEntry) -> DocResult<Replay> {
        // Страница, на которую ведёт шаг, могла быть не загружена; загрузка могла
        // сделать документ доступным только для чтения.
        self.page(entry.page)?;
        self.writable()?;
        let p = self.loaded.get(&entry.page).expect("загружена");
        // При сбое apply_inverse сам откатывает уже применённую часть шага.
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            Err(_) => changes.record(&entry.ops),
        }
        result
    }

````

на:

````rust
            Err(_) => changes.record(&entry.ops),
        }
        let ops = result?;
        match self.apply_doc_ops(&entry.doc_ops) {
            Ok((doc_ops, remap)) => Ok(Replay {
                ops,
                doc_ops,
                remap,
            }),
            // Операции страницы уже применены: назад, шаг вернётся в стек целиком.
            Err(e) => {
                let p = self.loaded.get(&entry.page).expect("загружена");
                let back = p.apply_inverse(&ops);
                let changes = self.changes.entry(entry.page).or_default();
                changes.record(&ops);
                Err(match back {
                    Ok(_) => e,
                    Err(rollback) => DocError::RollbackFailed {
                        original: Box::new(e),
                        rollback: Box::new(rollback),
                    },
                })
            }
        }
    }

    /// Операции над документом с конца; возвращает обратные к ним и новый
    /// идентификатор страницы, возвращённой повтором дубля.
    #[allow(clippy::type_complexity)]
    fn apply_doc_ops(
        &mut self,
        ops: &[DocOp],
    ) -> DocResult<(Vec<DocOp>, Option<(PageId, PageId)>)> {
        let mut done = Vec::with_capacity(ops.len());
        let mut remap = None;
        for op in ops.iter().rev() {
            match op {
                DocOp::PageGuides { page, guides } => {
                    self.ensure_page(*page)?;
                    let before = self.page_guides(*page);
                    self.write_page_guides(*page, guides)?;
                    done.push(DocOp::PageGuides {
                        page: *page,
                        guides: before,
                    });
                }
                DocOp::MovePage { page, index } => {
                    let before = self.page_index(*page)?;
                    self.place_page(*page, *index)?;
                    done.push(DocOp::MovePage {
                        page: *page,
                        index: before,
                    });
                }
                DocOp::RemovePage { page } => {
                    let index = self.page_index(*page)?;
                    if self.pages().len() <= 1 {
                        return Err(DocError::LastPage);
                    }
                    // Страницу с кэшем ещё не открывали — загрузка: парковка хранит её целиком.
                    self.page(*page)?;
                    let (name, background, guides) = (
                        self.pages()[index].1.clone(),
                        self.page_background(*page),
                        self.page_guides(*page),
                    );
                    self.pages_tree.delete(page.0)?;
                    self.root.commit();
                    let doc = self.loaded.remove(page).expect("загружена выше");
                    self.parked.insert(*page, doc);
                    self.changes.remove(page);
                    self.journaled_pages.remove(page);
                    self.guides_revision += 1;
                    done.push(DocOp::RestorePage {
                        page: *page,
                        index,
                        name,
                        background,
                        guides,
                    });
                }
                DocOp::RestorePage {
                    page,
                    index,
                    name,
                    background,
                    guides,
                } => {
                    if !self.parked.contains_key(page) {
                        return Err(DocError::PageNotFound(page.to_string()));
                    }
                    let at = (*index).min(self.pages().len());
                    let t = self.pages_tree.create_at(TreeParentId::Root, at)?;
                    let meta = self.pages_tree.get_meta(t)?;
                    meta.insert(KEY_NAME, name.as_str())?;
                    if let Some(c) = background {
                        meta.insert(KEY_BACKGROUND, c.to_hex())?;
                    }
                    if !guides.is_empty() {
                        meta.insert(KEY_GUIDES, guides_value(guides))?;
                    }
                    self.root.commit();
                    let new = PageId(t);
                    let doc = self.parked.remove(page).expect("проверено выше");
                    self.loaded.insert(new, doc);
                    // Копия целиком — в следующую пачку журнала, как новая страница.
                    self.journaled_pages.insert(new, VersionVector::default());
                    self.guides_revision += 1;
                    done.push(DocOp::RemovePage { page: new });
                    remap = Some((*page, new));
                }
            }
        }
        Ok((done, remap))
    }

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        };
        match self.replay(&entry) {
            Ok(ops) => {
                let restore = Restore {
                    page: entry.page,
                    selection: entry.selection_before.clone(),
                };
                self.history.push_redo(HistoryEntry { ops, ..entry });
                Ok(Some(restore))
            }
````

на:

````rust
        };
        match self.replay(&entry) {
            Ok(r) => {
                let restore = Restore {
                    page: entry.page,
                    selection: entry.selection_before.clone(),
                };
                self.history.push_redo(HistoryEntry {
                    ops: r.ops,
                    doc_ops: r.doc_ops,
                    ..entry
                });
                Ok(Some(restore))
            }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        };
        match self.replay(&entry) {
            Ok(ops) => {
                let restore = Restore {
                    page: entry.page,
                    selection: entry.selection_after.clone(),
                };
                self.history
                    .push_undo_keep_redo(HistoryEntry { ops, ..entry });
                Ok(Some(restore))
            }
````

на:

````rust
        };
        match self.replay(&entry) {
            Ok(r) => {
                let mut next = HistoryEntry {
                    ops: r.ops,
                    doc_ops: r.doc_ops,
                    ..entry
                };
                if let Some((old, new)) = r.remap {
                    self.history.remap_page(old, new);
                    if next.page_after == Some(old) {
                        next.page_after = Some(new);
                    }
                }
                let restore = Restore {
                    page: next.page_after.unwrap_or(next.page),
                    selection: next.selection_after.clone(),
                };
                self.history.push_undo_keep_redo(next);
                Ok(Some(restore))
            }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        assert!(!d2.is_page_loaded(page));
        let before = d2.pages();
        let err = d2.duplicate_page(page, "Копия");
        assert!(matches!(err, Err(DocError::ReadOnly)), "{err:?}");
        assert_eq!(d2.pages(), before, "копии нет");
````

на:

````rust
        assert!(!d2.is_page_loaded(page));
        let before = d2.pages();
        let err = d2.duplicate_page(page, "Копия", page, vec![]);
        assert!(matches!(err, Err(DocError::ReadOnly)), "{err:?}");
        assert_eq!(d2.pages(), before, "копии нет");
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        let page = doc.pages()[0].0;
        doc.history.push(HistoryEntry {
            page,
            ops: vec![missing_props_op()],
            selection_before: vec![],
            selection_after: vec![],
````

на:

````rust
        let page = doc.pages()[0].0;
        doc.history.push(HistoryEntry {
            id: 0,
            page,
            page_after: None,
            ops: vec![missing_props_op()],
            doc_ops: vec![],
            selection_before: vec![],
            selection_after: vec![],
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        let page = doc.pages()[0].0;
        doc.history.push_redo(HistoryEntry {
            page,
            ops: vec![missing_props_op()],
            selection_before: vec![],
            selection_after: vec![],
````

на:

````rust
        let page = doc.pages()[0].0;
        doc.history.push_redo(HistoryEntry {
            id: 0,
            page,
            page_after: None,
            ops: vec![missing_props_op()],
            doc_ops: vec![],
            selection_before: vec![],
            selection_after: vec![],
````

В `crates/ferrum-doc/src/error.rs` заменить:

````rust
    #[error("Правка вектора не применена: {0}")]
    VectorRejected(crate::vector::VectorError),
    #[error("Нельзя переместить узел внутрь самого себя")]
    CyclicMove,
````

на:

````rust
    #[error("Правка вектора не применена: {0}")]
    VectorRejected(crate::vector::VectorError),
    /// Правка направляющих вне пределов (спека подсистемы 2, раздел 11).
    #[error("Направляющие не применены: {0}")]
    InvalidGuides(String),
    #[error("Нельзя переместить узел внутрь самого себя")]
    CyclicMove,
````

Создать `crates/ferrum-doc/src/guides.rs`:

````rust
//! Направляющие (спека подсистемы 2, раздел 9): ключ `guides` у страницы и у
//! фрейма — список `{ axis: "x" | "y", offset }`. Значение неизменяемое и
//! заменяется целиком, как заливки: так совместная правка и отмена проще, а узлов
//! в Loro не прибавляется. Смещение у страницы — от нуля страницы, у фрейма — от
//! его левого верхнего угла в его координатах.
//!
//! Данные из файла не доверенные: неверные записи (ось, нечисловое смещение, вне
//! предела, повторы) отбрасываются поодиночке, сверх `MAX_GUIDES` — не читаются.

use std::collections::HashSet;

use loro::LoroValue;

use crate::codec::{as_f64, as_str, field, obj};
use crate::error::{DocError, DocResult};

/// Наибольшее число направляющих у одного узла.
pub const MAX_GUIDES: usize = 1000;
/// Предел смещения по модулю.
pub const MAX_GUIDE_OFFSET: f64 = 1e9;

/// Ось, вдоль которой считается смещение: `X` — вертикальная линия на отметке x,
/// `Y` — горизонтальная на отметке y.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuideAxis {
    X,
    Y,
}

impl GuideAxis {
    pub fn as_str(self) -> &'static str {
        match self {
            GuideAxis::X => "x",
            GuideAxis::Y => "y",
        }
    }

    pub fn parse(s: &str) -> Option<GuideAxis> {
        match s {
            "x" => Some(GuideAxis::X),
            "y" => Some(GuideAxis::Y),
            _ => None,
        }
    }
}

/// Одна направляющая.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guide {
    pub axis: GuideAxis,
    pub offset: f64,
}

impl Guide {
    pub fn new(axis: GuideAxis, offset: f64) -> Guide {
        Guide { axis, offset }
    }

    /// Смещение конечно и в пределе.
    pub fn is_valid(&self) -> bool {
        self.offset.is_finite() && self.offset.abs() <= MAX_GUIDE_OFFSET
    }
}

/// Проверка списка, который пишет правка: сверх предела или с неверным смещением —
/// отказ целиком (`DocError::InvalidGuides`).
pub fn check_guides(guides: &[Guide]) -> DocResult<()> {
    if guides.len() > MAX_GUIDES {
        return Err(DocError::InvalidGuides(format!(
            "больше {MAX_GUIDES} на один узел"
        )));
    }
    if guides.iter().any(|g| !g.is_valid()) {
        return Err(DocError::InvalidGuides(
            "смещение должно быть конечным и не больше 1e9 по модулю".into(),
        ));
    }
    Ok(())
}

/// Без повторов (та же ось и то же смещение), первые на месте; `-0` — как `0`.
pub fn dedup_guides(guides: impl IntoIterator<Item = Guide>) -> Vec<Guide> {
    let mut seen = HashSet::new();
    guides
        .into_iter()
        .map(|g| Guide::new(g.axis, g.offset + 0.0))
        .filter(|g| seen.insert((g.axis, g.offset.to_bits())))
        .collect()
}

pub(crate) fn guides_value(guides: &[Guide]) -> LoroValue {
    LoroValue::from(
        guides
            .iter()
            .map(|g| {
                obj(vec![
                    ("axis", g.axis.as_str().into()),
                    ("offset", g.offset.into()),
                ])
            })
            .collect::<Vec<_>>(),
    )
}

/// Список как есть, без проверок: так тесты пишут неверные данные чужого файла.
#[doc(hidden)]
pub(crate) fn raw_value(raw: &[(&str, f64)]) -> LoroValue {
    LoroValue::from(
        raw.iter()
            .map(|(axis, offset)| obj(vec![("axis", (*axis).into()), ("offset", (*offset).into())]))
            .collect::<Vec<_>>(),
    )
}

fn read_guide(v: &LoroValue) -> Option<Guide> {
    let axis = GuideAxis::parse(&as_str(field(v, "axis")?)?)?;
    let guide = Guide::new(axis, as_f64(field(v, "offset")?)?);
    guide.is_valid().then_some(guide)
}

/// Список из файла: неверные записи и повторы отброшены, не больше `MAX_GUIDES`.
/// Обход останавливается на пределе: чужой файл не заставит разбирать весь список.
pub(crate) fn read_guides(v: &LoroValue) -> Vec<Guide> {
    let LoroValue::List(list) = v else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in list.iter() {
        if out.len() == MAX_GUIDES {
            break;
        }
        if let Some(g) = read_guide(item) {
            let g = Guide::new(g.axis, g.offset + 0.0);
            if seen.insert((g.axis, g.offset.to_bits())) {
                out.push(g);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(axis: &str, offset: f64) -> LoroValue {
        obj(vec![("axis", axis.into()), ("offset", offset.into())])
    }

    #[test]
    fn reading_drops_bad_entries_one_by_one() {
        let list = LoroValue::from(vec![
            raw("x", 10.0),
            raw("z", 1.0),
            raw("y", f64::NAN),
            raw("y", f64::INFINITY),
            raw("y", 1e9 + 1.0),
            raw("y", -1e9),
            raw("x", 10.0),
            "мусор".into(),
        ]);
        assert_eq!(
            read_guides(&list),
            [
                Guide::new(GuideAxis::X, 10.0),
                Guide::new(GuideAxis::Y, -1e9)
            ]
        );
        assert!(read_guides(&LoroValue::from(1.0)).is_empty());
    }

    #[test]
    fn reading_stops_at_the_limit() {
        let list = LoroValue::from(
            (0..MAX_GUIDES + 50)
                .map(|i| raw("x", i as f64))
                .collect::<Vec<_>>(),
        );
        let read = read_guides(&list);
        assert_eq!(read.len(), MAX_GUIDES);
        assert_eq!(read[MAX_GUIDES - 1].offset, (MAX_GUIDES - 1) as f64);
    }

    #[test]
    fn writing_is_refused_whole() {
        let ok = vec![Guide::new(GuideAxis::X, 1.0); MAX_GUIDES];
        assert!(check_guides(&ok).is_ok());
        let too_many = vec![Guide::new(GuideAxis::X, 1.0); MAX_GUIDES + 1];
        assert!(matches!(
            check_guides(&too_many),
            Err(DocError::InvalidGuides(_))
        ));
        for bad in [f64::NAN, f64::INFINITY, -f64::INFINITY, 1.5e9] {
            let list = [Guide::new(GuideAxis::Y, 0.0), Guide::new(GuideAxis::X, bad)];
            assert!(matches!(
                check_guides(&list),
                Err(DocError::InvalidGuides(_))
            ));
        }
    }

    #[test]
    fn dedup_keeps_first_and_folds_negative_zero() {
        let got = dedup_guides([
            Guide::new(GuideAxis::X, 0.0),
            Guide::new(GuideAxis::X, -0.0),
            Guide::new(GuideAxis::Y, 0.0),
        ]);
        assert_eq!(got.len(), 2);
        assert!(got[0].offset.is_sign_positive());
    }
}
````

В `crates/ferrum-doc/src/history.rs` заменить:

````rust
//! своей транзакции и выделение до и после неё.

use crate::page::Inverse;
use crate::types::{NodeId, PageId};

/// Пауза, после которой ввод подряд начинает новый шаг отмены (как в Figma).
````

на:

````rust
//! своей транзакции и выделение до и после неё.

use crate::guides::Guide;
use crate::page::Inverse;
use crate::types::{Color, NodeId, PageId};

/// Пауза, после которой ввод подряд начинает новый шаг отмены (как в Figma).
````

В `crates/ferrum-doc/src/history.rs` заменить:

````rust
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HistoryEntry {
    pub page: PageId,
    /// Для шага в стеке undo — операции отмены; в стеке redo — операции повтора.
    pub ops: Vec<Inverse>,
    pub selection_before: Vec<NodeId>,
    pub selection_after: Vec<NodeId>,
````

на:

````rust
}

/// Обратная операция над самим документом, а не над страницей: дерево страниц
/// живёт в корневом `LoroDoc` (спека подсистемы 2, разделы 9 и 10).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DocOp {
    /// Прежние направляющие страницы.
    PageGuides { page: PageId, guides: Vec<Guide> },
    /// Прежнее место страницы в списке.
    MovePage { page: PageId, index: usize },
    /// Отмена дубля: копия уходит из списка и ждёт повтора в `Document::parked`.
    RemovePage { page: PageId },
    /// Повтор дубля: страница из `Document::parked` возвращается в список. В дереве
    /// страниц Loro она получает новый идентификатор (`History::remap_page`).
    RestorePage {
        page: PageId,
        index: usize,
        name: String,
        background: Option<Color>,
        guides: Vec<Guide>,
    },
}

impl DocOp {
    fn page(&self) -> PageId {
        match self {
            DocOp::PageGuides { page, .. }
            | DocOp::MovePage { page, .. }
            | DocOp::RemovePage { page }
            | DocOp::RestorePage { page, .. } => *page,
        }
    }

    fn page_mut(&mut self) -> &mut PageId {
        match self {
            DocOp::PageGuides { page, .. }
            | DocOp::MovePage { page, .. }
            | DocOp::RemovePage { page }
            | DocOp::RestorePage { page, .. } => page,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HistoryEntry {
    /// Номер шага: один и тот же в стеках undo и redo. Его выдаёт `History::push`.
    pub id: u64,
    /// Страница шага: на неё возвращает отмена, ей принадлежат `ops`.
    pub page: PageId,
    /// Куда возвращает повтор, если не на `page` («Дублировать страницу» — на копию).
    pub page_after: Option<PageId>,
    /// Для шага в стеке undo — операции отмены; в стеке redo — операции повтора.
    pub ops: Vec<Inverse>,
    /// Операции над документом в том же направлении.
    pub doc_ops: Vec<DocOp>,
    pub selection_before: Vec<NodeId>,
    pub selection_after: Vec<NodeId>,
````

В `crates/ferrum-doc/src/history.rs` заменить:

````rust
    /// тех пор стек не менялся; иначе `None` — следующей правке не с чем слиться.
    last_merge: Option<(PageId, TextMerge)>,
}

impl History {
    /// Новый шаг пользователя сбрасывает возможность повтора.
    pub fn push(&mut self, entry: HistoryEntry) {
        self.undo.push(entry);
        self.redo.clear();
````

на:

````rust
    /// тех пор стек не менялся; иначе `None` — следующей правке не с чем слиться.
    last_merge: Option<(PageId, TextMerge)>,
    next_id: u64,
}

impl HistoryEntry {
    /// Шаг опирается на страницу: она удалена — шаг не применить.
    fn references(&self, page: PageId) -> bool {
        self.page == page
            || self.page_after == Some(page)
            || self.doc_ops.iter().any(|op| op.page() == page)
    }

    fn remap_page(&mut self, old: PageId, new: PageId) {
        if self.page == old {
            self.page = new;
        }
        if self.page_after == Some(old) {
            self.page_after = Some(new);
        }
        for op in &mut self.doc_ops {
            if op.page() == old {
                *op.page_mut() = new;
            }
        }
    }
}

impl History {
    /// Новый шаг пользователя сбрасывает возможность повтора.
    pub fn push(&mut self, mut entry: HistoryEntry) {
        self.next_id += 1;
        entry.id = self.next_id;
        self.undo.push(entry);
        self.redo.clear();
````

В `crates/ferrum-doc/src/history.rs` заменить:

````rust
    }

    pub fn forget_page(&mut self, page: PageId) {
        self.undo.retain(|e| e.page != page);
        self.redo.retain(|e| e.page != page);
        self.last_merge = None;
    }
}
````

на:

````rust
    }

    /// Номер верхнего шага отмены: кнопка «Отменить» в уведомлении проверяет, что
    /// её шаг ещё последний.
    pub fn top_undo_id(&self) -> Option<u64> {
        self.undo.last().map(|e| e.id)
    }

    pub fn forget_page(&mut self, page: PageId) {
        self.undo.retain(|e| !e.references(page));
        self.redo.retain(|e| !e.references(page));
        self.last_merge = None;
    }

    /// Страница, возвращённая повтором дубля, получила новый идентификатор:
    /// шаги повтора после дубля теперь про неё.
    pub fn remap_page(&mut self, old: PageId, new: PageId) {
        for e in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            e.remap_page(old, new);
        }
    }

    /// Шаги повтора ещё есть: припаркованная копия страницы может понадобиться.
    pub fn has_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
mod document;
mod error;
mod history;
mod images;
````

на:

````rust
mod document;
mod error;
mod guides;
mod history;
mod images;
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
pub use boolean::BooleanOp;
pub use changes::Changes;
pub use document::{Document, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use history::{TextMerge, MERGE_PAUSE_MS};
pub use images::ImageStore;
````

на:

````rust
pub use boolean::BooleanOp;
pub use changes::Changes;
pub use document::{Document, GuideOwner, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use guides::{check_guides, dedup_guides, Guide, GuideAxis, MAX_GUIDES, MAX_GUIDE_OFFSET};
pub use history::{TextMerge, MERGE_PAUSE_MS};
pub use images::ImageStore;
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    /// чужой сборки) — без проверки и без зеркала.
    #[doc(hidden)]
    pub fn force_boolean_op_for_tests(&self, id: NodeId, op: &str) {
        let tid = self.tid(id).expect("узел есть");
````

на:

````rust
    /// чужой сборки) — без проверки и без зеркала.
    #[doc(hidden)]
    /// Записывает в узел направляющие как есть — как чужой файл (тесты разбора).
    #[doc(hidden)]
    pub fn force_guides_for_tests(&self, id: NodeId, raw: &[(&str, f64)]) {
        let tid = self.tid(id).expect("узел есть");
        let meta = self.tree.get_meta(tid).expect("узел есть");
        meta.insert(codec::key::GUIDES, crate::guides::raw_value(raw))
            .expect("запись");
        self.doc.commit();
    }

    #[doc(hidden)]
    pub fn force_boolean_op_for_tests(&self, id: NodeId, op: &str) {
        let tid = self.tid(id).expect("узел есть");
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        let foreign_vector = before.vector.is_none() && patch.vector.is_some();
        let foreign_op = before.boolean_op.is_none() && patch.boolean_op.is_some();
        let patch = if foreign_text || foreign_vector || foreign_op {
            stripped = PropsPatch {
                text: if foreign_text {
````

на:

````rust
        let foreign_vector = before.vector.is_none() && patch.vector.is_some();
        let foreign_op = before.boolean_op.is_none() && patch.boolean_op.is_some();
        let foreign_guides = before.kind != NodeKind::Frame && patch.guides.is_some();
        let patch = if foreign_text || foreign_vector || foreign_op || foreign_guides {
            stripped = PropsPatch {
                text: if foreign_text {
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
                },
                boolean_op: if foreign_op { None } else { patch.boolean_op },
                ..patch.clone()
            };
````

на:

````rust
                },
                boolean_op: if foreign_op { None } else { patch.boolean_op },
                guides: if foreign_guides {
                    None
                } else {
                    patch.guides.clone()
                },
                ..patch.clone()
            };
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        if let Some(v) = &patch.vector {
            check_vector(v)?;
        }

````

на:

````rust
        if let Some(v) = &patch.vector {
            check_vector(v)?;
        }
        if let Some(g) = &patch.guides {
            crate::guides::check_guides(g)?;
        }

````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
use crate::binary::{Reader, Writer};
use crate::boolean::BooleanOp;
use crate::text::*;
use crate::types::*;
````

на:

````rust
use crate::binary::{Reader, Writer};
use crate::boolean::BooleanOp;
use crate::guides::{Guide, GuideAxis, MAX_GUIDES};
use crate::text::*;
use crate::types::*;
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
        // обводки — код операции.
        NodeKind::Boolean => 8,
    }
}
````

на:

````rust
        // обводки — код операции.
        NodeKind::Boolean => 8,
        // Шестая версия (`SCENE_CACHE_VERSION = 6`): у фрейма в конце строки —
        // направляющие (`read_row_guides`).
    }
}
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
        if let Some(op) = p.boolean_op {
            w.u8(code(&BooleanOp::ALL, op));
        }
    }
````

на:

````rust
        if let Some(op) = p.boolean_op {
            w.u8(code(&BooleanOp::ALL, op));
        }
        if p.kind == NodeKind::Frame {
            let guides = p.guides.as_deref().map_or(&[][..], |g| g.as_slice());
            w.u16(u16::try_from(guides.len()).ok()?);
            for g in guides {
                w.u8(match g.axis {
                    GuideAxis::X => 0,
                    GuideAxis::Y => 1,
                });
                w.f64(g.offset);
            }
        }
    }
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
}

/// `None` — таблица повреждена; вызывающий код тогда просто не использует кэш.
pub(crate) fn decode(bytes: &[u8]) -> Option<Vec<SceneRow>> {
````

на:

````rust
}

/// Направляющие фрейма в строке. Внешний `None` — таблица повреждена: сверх
/// предела, нечисловое смещение, неизвестная ось (страница тогда читается из Loro,
/// где неверные записи отбрасываются поодиночке).
fn read_row_guides(r: &mut Reader) -> Option<Option<Arc<Vec<Guide>>>> {
    let n = r.u16()? as usize;
    if n > MAX_GUIDES {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let axis = match r.u8()? {
            0 => GuideAxis::X,
            1 => GuideAxis::Y,
            _ => return None,
        };
        let g = Guide::new(axis, r.f64()?);
        if !g.is_valid() {
            return None;
        }
        out.push(g);
    }
    Some((!out.is_empty()).then(|| Arc::new(out)))
}

/// `None` — таблица повреждена; вызывающий код тогда просто не использует кэш.
pub(crate) fn decode(bytes: &[u8]) -> Option<Vec<SceneRow>> {
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
            _ => None,
        };
        let [x, y, w, h, rotation, opacity, corner_radius] = nums;
        rows.push(SceneRow {
````

на:

````rust
            _ => None,
        };
        let guides = match kind {
            NodeKind::Frame => read_row_guides(&mut r)?,
            _ => None,
        };
        let [x, y, w, h, rotation, opacity, corner_radius] = nums;
        rows.push(SceneRow {
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
                vector,
                boolean_op,
            },
        });
````

на:

````rust
                vector,
                boolean_op,
                guides,
            },
        });
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
use crate::boolean::BooleanOp;
use crate::error::DocError;
use crate::text::{TextContent, TextData, TextProps, TEXT_COLOR};
use crate::vector::{StrokeCap, StrokeJoin, StrokeStyle, VectorNetwork};
````

на:

````rust
use crate::boolean::BooleanOp;
use crate::error::DocError;
use crate::guides::Guide;
use crate::text::{TextContent, TextData, TextProps, TEXT_COLOR};
use crate::vector::{StrokeCap, StrokeJoin, StrokeStyle, VectorNetwork};
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    /// Операция — только у `NodeKind::Boolean`.
    pub boolean_op: Option<BooleanOp>,
}

````

на:

````rust
    /// Операция — только у `NodeKind::Boolean`.
    pub boolean_op: Option<BooleanOp>,
    /// Направляющие — только у `NodeKind::Frame`; `None` — их нет (пустого списка
    /// не бывает). `Arc`: копия свойств не копирует список.
    pub guides: Option<Arc<Vec<Guide>>>,
}

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            vector: (kind == NodeKind::Vector).then(|| Arc::new(VectorNetwork::default())),
            boolean_op: (kind == NodeKind::Boolean).then_some(BooleanOp::Union),
        }
    }
````

на:

````rust
            vector: (kind == NodeKind::Vector).then(|| Arc::new(VectorNetwork::default())),
            boolean_op: (kind == NodeKind::Boolean).then_some(BooleanOp::Union),
            guides: None,
        }
    }
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    /// Операция булевой группы. У другого узла пропускается.
    pub boolean_op: Option<BooleanOp>,
}

````

на:

````rust
    /// Операция булевой группы. У другого узла пропускается.
    pub boolean_op: Option<BooleanOp>,
    /// Направляющие фрейма целиком; пустой список — убрать. У другого узла
    /// пропускается.
    pub guides: Option<Arc<Vec<Guide>>>,
}

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            },
            boolean_op: self.boolean_op.and(before.boolean_op),
        }
    }
````

на:

````rust
            },
            boolean_op: self.boolean_op.and(before.boolean_op),
            guides: self
                .guides
                .as_ref()
                .map(|_| before.guides.clone().unwrap_or_default()),
        }
    }
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
        }

        if let Some(t) = &mut p.text {
            if self.text.is_some() || self.content.is_some() {
````

на:

````rust
        }

        if let Some(v) = &self.guides {
            p.guides = (!v.is_empty()).then(|| v.clone());
        }

        if let Some(t) = &mut p.text {
            if self.text.is_some() || self.content.is_some() {
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
//! как неизвестный. Булева группа — узел «boolean» с полем `booleanOp` (только
//! известные операции; без поля — объединение): сборка без булевых групп
//! пропускает её вместе с детьми.
//!
//! Вставка не доверяет данным: размеры и количества ограничены, длины проверяются
````

на:

````rust
//! как неизвестный. Булева группа — узел «boolean» с полем `booleanOp` (только
//! известные операции; без поля — объединение): сборка без булевых групп
//! пропускает её вместе с детьми. Направляющие фрейма — необязательное поле `guides`
//! (`[{ axis, offset }]`): старая сборка его не читает, а без поля их просто нет.
//!
//! Вставка не доверяет данным: размеры и количества ограничены, длины проверяются
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust

use ferrum_doc::{
    BooleanOp, Color, Fill, FillKind, NodeKind, Props, PropsPatch, ScaleMode, Stroke, StrokeAlign,
    StrokeCap, StrokeJoin, StrokeStyle, TextData, VectorNetwork, DEFAULT_MITER_LIMIT,
    MITER_LIMIT_RANGE,
};
use ferrum_scene::shape;
````

на:

````rust

use ferrum_doc::{
    check_guides, dedup_guides, BooleanOp, Color, Fill, FillKind, Guide, GuideAxis, NodeKind,
    Props, PropsPatch, ScaleMode, Stroke, StrokeAlign, StrokeCap, StrokeJoin, StrokeStyle,
    TextData, VectorNetwork, DEFAULT_MITER_LIMIT, MITER_LIMIT_RANGE,
};
use ferrum_scene::shape;
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    boolean_op: Option<String>,
}

````

на:

````rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    boolean_op: Option<String>,
    /// Направляющие — только у фрейма (спека подсистемы 2, раздел 9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    guides: Option<Vec<JsonGuide>>,
}

#[derive(Serialize, Deserialize)]
struct JsonGuide {
    axis: String,
    offset: f64,
}

````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
                        vector: p.vector.as_ref().map(|v| v.to_base64()),
                        boolean_op: p.boolean_op.map(|op| op.as_str().to_string()),
                    }
                })
````

на:

````rust
                        vector: p.vector.as_ref().map(|v| v.to_base64()),
                        boolean_op: p.boolean_op.map(|op| op.as_str().to_string()),
                        guides: p.guides.as_ref().map(|list| {
                            list.iter()
                                .map(|g| JsonGuide {
                                    axis: g.axis.as_str().to_string(),
                                    offset: g.offset,
                                })
                                .collect()
                        }),
                    }
                })
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
            _ => None,
        },
    };
    // Нечисловые и огромные значения — к допустимым, как у строк кэша сцены.
````

на:

````rust
            _ => None,
        },
        // Направляющие — только у фрейма; неверные и сверх предела — отказ вставки.
        guides: match (kind, n.guides) {
            (NodeKind::Frame, Some(list)) => {
                let guides = list
                    .into_iter()
                    .map(|g| {
                        GuideAxis::parse(&g.axis)
                            .map(|axis| Guide::new(axis, g.offset))
                            .ok_or_else(|| corrupted("ось направляющей"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                check_guides(&guides).map_err(|e| corrupted(e.to_string()))?;
                let guides = dedup_guides(guides);
                (!guides.is_empty()).then(|| Arc::new(guides))
            }
            _ => None,
        },
    };
    // Нечисловые и огромные значения — к допустимым, как у строк кэша сцены.
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
        vector: p.vector.clone(),
        boolean_op: p.boolean_op,
    }
}
````

на:

````rust
        vector: p.vector.clone(),
        boolean_op: p.boolean_op,
        guides: p.guides.clone(),
    }
}
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    DocumentChanged,
    PageChanged(PageId),
    /// Готовы байты для системного буфера обмена (формат `web application/x-ferrum`).
    Clipboard(Vec<u8>),
````

на:

````rust
    DocumentChanged,
    PageChanged(PageId),
    /// Изменился список страниц или их порядок (отмена и повтор их шагов).
    PagesChanged,
    /// Готовы байты для системного буфера обмена (формат `web application/x-ferrum`).
    Clipboard(Vec<u8>),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        // Жест держит превью и черновики на узлах, которые отмена может убрать.
        self.cancel_gesture();
        let result = if redo {
            self.doc.redo()
````

на:

````rust
        // Жест держит превью и черновики на узлах, которые отмена может убрать.
        self.cancel_gesture();
        let pages = self.doc.pages();
        let guides = self.doc.guides_revision();
        let result = if redo {
            self.doc.redo()
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            self.doc.undo()
        };
        sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        match result {
            Ok(Some(restore)) => {
````

на:

````rust
            self.doc.undo()
        };
        let synced = sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        // Шаг мог не тронуть ни одного узла: список страниц и направляющие страницы
        // живут в корне документа, сцена о них не знает.
        let list_changed = self.doc.pages() != pages;
        if list_changed {
            self.out.events.push(EditorEvent::PagesChanged);
        }
        if !synced && (list_changed || self.doc.guides_revision() != guides) {
            self.out.events.push(EditorEvent::DocumentChanged);
            self.out.events.push(EditorEvent::NeedsRedraw);
        }
        match result {
            Ok(Some(restore)) => {
````

В `crates/ferrum-editor/src/pages.rs` заменить:

````rust
//! Страницы из панели «Страницы» (этап 5): создать, дублировать, переименовать, удалить,
//! цвет фона. Как и в `ferrum-doc`, операции со страницами не попадают в историю
//! отмены. Имена из интерфейса не доверенные: `clean_name` убирает управляющие
//! символы и ограничивает длину.

````

на:

````rust
//! Страницы из панели «Страницы» (этап 5): создать, дублировать, переименовать, удалить,
//! цвет фона. Как и в `ferrum-doc`, в историю отмены попадает только «Дублировать
//! страницу» (спека подсистемы 2, раздел 10), остальные операции — нет. Имена из интерфейса не доверенные: `clean_name` убирает управляющие
//! символы и ограничивает длину.

````

В `crates/ferrum-editor/src/pages.rs` заменить:

````rust

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
````

на:

````rust

    /// «Дублировать страницу»: копия встаёт сразу после страницы с именем
    /// «<имя> (копия)» и становится текущей, как новая страница. Один шаг отмены:
    /// отмена убирает копию и возвращает на прежнюю страницу.
    pub fn duplicate_page(&mut self, id: PageId) -> DocResult<PageId> {
        let Some((_, name)) = self.doc.pages().into_iter().find(|(p, _)| *p == id) else {
            return Err(ferrum_doc::DocError::PageNotFound(id.to_string()));
        };
        // Шаг дубля — последним: незаписанный текст и путь пера пишутся раньше него.
        self.cancel_gesture();
        self.finish_text_edit();
        self.finish_vector_edit();
        // Имя копии — не длиннее предела, с пометкой в конце.
        const MARK: &str = " (копия)";
````

В `crates/ferrum-editor/src/pages.rs` заменить:

````rust
            .take(MAX_NAME_CHARS - MARK.chars().count())
            .collect();
        let copy = self
            .doc
            .duplicate_page(id, &format!("{}{MARK}", base.trim_end()))?;
        self.set_page(copy)?;
        Ok(copy)
````

на:

````rust
            .take(MAX_NAME_CHARS - MARK.chars().count())
            .collect();
        let copy = self.doc.duplicate_page(
            id,
            &format!("{}{MARK}", base.trim_end()),
            self.page,
            self.selection.to_vec(),
        )?;
        self.out.steps += 1;
        self.set_page(copy)?;
        Ok(copy)
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                    self.dirty = true;
                }
                EditorEvent::Clipboard(bytes) => self.events.push(CoreEvent::Clipboard {
                    bytes: ByteBuf::from(bytes),
````

на:

````rust
                    self.dirty = true;
                }
                EditorEvent::PagesChanged => self.ui.pages = true,
                EditorEvent::Clipboard(bytes) => self.events.push(CoreEvent::Clipboard {
                    bytes: ByteBuf::from(bytes),
````

В `scripts/compat-old-build.sh` заменить:

````
# текста, 2710c85), `vector_fixture` (ref — сборка без векторов, например main
# до этапа 3) или `boolean_fixture` (ref — сборка без булевых групп, например main
# до этапа 4: группа скрыта вместе с детьми).
# Старая сборка идёт в свой каталог `$CARGO_TARGET_DIR/compat-old`: у членов workspace
# хэш артефакта не зависит от пути, и в общем каталоге старый `ferrum-doc` перезаписал
````

на:

````
# текста, 2710c85), `vector_fixture` (ref — сборка без векторов, например main
# до этапа 3) или `boolean_fixture` (ref — сборка без булевых групп, например main
# до этапа 4: группа скрыта вместе с детьми). `guides_fixture` (ref — сборка без
# направляющих, например main до этапа 4 подсистемы 2) — другое обещание: файл
# открывается как обычно, для записи (неизвестный ключ просто не читается), а
# пересохранённый старой сборкой не теряет направляющих — это проверяет новая сборка.
# Старая сборка идёт в свой каталог `$CARGO_TARGET_DIR/compat-old`: у членов workspace
# хэш артефакта не зависит от пути, и в общем каталоге старый `ferrum-doc` перезаписал
````

В `scripts/compat-old-build.sh` заменить:

````
(cd "$ROOT" && cargo run -q -p ferrum-doc --example "$FIXTURE" -- "$FILE")
git -C "$ROOT" worktree add --detach "$TMP/old" "$REF" >/dev/null
cat > "$TMP/old/crates/ferrum-doc/tests/compat_fixture.rs" <<'RS'
use ferrum_doc::container::decode;
````

на:

````
(cd "$ROOT" && cargo run -q -p ferrum-doc --example "$FIXTURE" -- "$FILE")
git -C "$ROOT" worktree add --detach "$TMP/old" "$REF" >/dev/null
if [ "$FIXTURE" = guides_fixture ]; then
  cat > "$TMP/old/crates/ferrum-doc/tests/compat_fixture.rs" <<'RS'
use ferrum_doc::container::{decode, encode};
use ferrum_doc::NodeKind;
#[test]
fn file_with_guides_opens_as_usual() {
    let bytes = std::fs::read(std::env::var("FERRUM_FIXTURE_FILE").unwrap()).unwrap();
    let opened = decode(&bytes).unwrap();
    assert!(opened.warnings.is_empty(), "{:?}", opened.warnings);
    let mut doc = opened.document;
    let page = doc.pages()[0].0;
    assert_eq!(doc.page(page).unwrap().walk().len(), 2, "фрейм и прямоугольник");
    assert!(!doc.is_read_only(), "неизвестный ключ не мешает правке");
    doc.transact(page, vec![], |tx| Ok(vec![tx.create_node(None, None, NodeKind::Ellipse)?]))
        .unwrap();
    std::fs::write(std::env::var("FERRUM_FIXTURE_OUT").unwrap(), encode(&doc).unwrap()).unwrap();
}
RS
  (cd "$TMP/old" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/compat-old" FERRUM_FIXTURE_FILE="$FILE" \
    FERRUM_FIXTURE_OUT="$TMP/resaved.ferrum" cargo test -q -p ferrum-doc --test compat_fixture)
  (cd "$ROOT" && cargo run -q -p ferrum-doc --example guides_fixture -- --check "$TMP/resaved.ferrum")
  echo "Совместимость с $REF ($FIXTURE): файл открыт как обычно, направляющие пережили пересохранение"
  exit 0
fi
cat > "$TMP/old/crates/ferrum-doc/tests/compat_fixture.rs" <<'RS'
use ferrum_doc::container::decode;
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && scripts/compat-old-build.sh main guides_fixture`

Expected: PASS: fmt и clippy (нативно и wasm32) без замечаний; `ferrum-doc`, `ferrum-editor`, `ferrum-wasm` без падений (новые: `ferrum-doc/tests/guides.rs` — 21 тест, `clipboard_format.rs` и `context_menu.rs` редактора — по 1; весь workspace в прогоне прототипа — 1093 passed); совместимость печатает «файл открыт как обычно, направляющие пережили пересохранение»

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-doc/examples/guides_fixture.rs crates/ferrum-doc/src/codec.rs crates/ferrum-doc/src/container.rs crates/ferrum-doc/src/document.rs crates/ferrum-doc/src/error.rs crates/ferrum-doc/src/guides.rs crates/ferrum-doc/src/history.rs crates/ferrum-doc/src/lib.rs crates/ferrum-doc/src/page.rs crates/ferrum-doc/src/scene_cache.rs crates/ferrum-doc/src/types.rs crates/ferrum-doc/tests/container.rs crates/ferrum-doc/tests/flip.rs crates/ferrum-doc/tests/guides.rs crates/ferrum-doc/tests/journal.rs crates/ferrum-editor/src/clipboard.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/pages.rs crates/ferrum-editor/tests/clipboard_format.rs crates/ferrum-editor/tests/context_menu.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/tests/context_menu.rs scripts/compat-old-build.sh
git commit -F - <<'EOF'
feat(doc): направляющие страницы и фрейма, шаги отмены страниц

Ключ `guides` у страницы и фрейма (до 1000, неверные записи отбрасываются),
кэш сцены версии 6, поле в буфере обмена, `set_guides` одним шагом,
`move_page` и `duplicate_page` — шаги отмены, номер шага для кнопки в
уведомлении. Проверка старой сборкой: `guides_fixture`.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 2: Линейки в кадре, команды «Линейки» и «Направляющие»

Спека §9: Shift+R, «Вид → Линейки», деления под масштаб, полоса выделения, начало отсчёта; рисует ядро последним слоем оверлея. Здесь — `RulersOverlay` и линии в `overlay.rs`, показ и начало отсчёта в `ferrum-editor/src/guides.rs`, команды, пункты «Вид» и меню пустого места (закрывает B-121), `SetView` и событие `View`, строки замера в `render_report`. Протяжки пока нет.

**Files:**
- Create: `crates/ferrum-editor/src/guides.rs`
- Modify: `crates/ferrum-editor/src/actions.rs`, `crates/ferrum-editor/src/command_state.rs`, `crates/ferrum-editor/src/commands.rs`, `crates/ferrum-editor/src/context_menu.rs`, `crates/ferrum-editor/src/editor.rs`, `crates/ferrum-editor/src/lib.rs`, `crates/ferrum-editor/src/menu.rs`, `crates/ferrum-editor/src/vector_select.rs`, `crates/ferrum-render/examples/render_report.rs`, `crates/ferrum-render/src/lib.rs`, `crates/ferrum-render/src/overlay.rs`, `crates/ferrum-render/src/text.rs`, `crates/ferrum-wasm/src/core.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `crates/ferrum-wasm/src/protocol.rs`
- Test: `apps/web/e2e/app.spec.ts`, `apps/web/e2e/context-menu.spec.ts`, `crates/ferrum-editor/tests/command_table.rs`, `crates/ferrum-editor/tests/context_menu.rs`, `crates/ferrum-editor/tests/keymap.rs`, `crates/ferrum-editor/tests/rulers.rs` (новый), `crates/ferrum-render/tests/render.rs`, `crates/ferrum-wasm/tests/boolean.rs`, `crates/ferrum-wasm/tests/context_menu.rs`

**Interfaces:**
- Consumes: `Overlay`, `OverlayTheme`, `Label` (`text.rs`), `CommandId`, `COMMANDS`, `Editor::{settings, command_state}`, таблица главного меню и меню холста; `Document::{page_guides, guides_revision}` (задача 1); `geometry::world_box`; `ferrum_scene::{Scene, Slot}`.
- Produces: `ferrum_render::{RulersOverlay { origin, band, … }, GuideMark { line, infinite }, ActiveGuide { mark, value }, ruler_steps(zoom) -> (f64, Option<f64>), RULER_SIZE = 20.0}`, `Label::draw_rotated`, поля `Overlay.{rulers, guides, active_guide}`, `OverlayTheme.{ruler, ruler_ink, ruler_line, guide}`; `CommandId::{ViewRulers, ViewGuides}` (88 команд); `Settings.{rulers, guides}`; `Editor::{set_view, rulers_visible, guides_visible}`; `EditorEvent::ViewChanged { rulers, guides }`; `Command::SetView { rulers?, guides? }`, `CoreEvent::View { rulers, guides }`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 86, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 88, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `apps/web/e2e/context-menu.spec.ts` заменить:

````ts
  expect(Math.abs(box.x - empty.x)).toBeLessThanOrEqual(1);
  expect(Math.min(Math.abs(box.y - (empty.y + 4)), Math.abs(box.y + box.height - (empty.y - 4)))).toBeLessThanOrEqual(2);
  expect(await labels(page, 'Действия с выделением')).toEqual(['Вставить', 'Вставить поверх', 'Выделить всё', 'Показать всё']);
  await menu.getByRole('menuitem', { name: /^Выделить всё/ }).click();
  await liveWhen(page, 'l.selectionCount === 10');
````

на:

````ts
  expect(Math.abs(box.x - empty.x)).toBeLessThanOrEqual(1);
  expect(Math.min(Math.abs(box.y - (empty.y + 4)), Math.abs(box.y + box.height - (empty.y - 4)))).toBeLessThanOrEqual(2);
  expect(await labels(page, 'Действия с выделением')).toEqual(['Вставить', 'Вставить поверх', 'Выделить всё', 'Показать всё', 'Линейки']);
  await menu.getByRole('menuitem', { name: /^Выделить всё/ }).click();
  await liveWhen(page, 'l.selectionCount === 10');
````

В `crates/ferrum-editor/tests/command_table.rs` заменить:

````rust
            C::ToggleUi,
            C::PixelGrid,
            C::UiPixelSnap,
            C::UiThemeDark,
````

на:

````rust
            C::ToggleUi,
            C::PixelGrid,
            C::ViewRulers,
            C::ViewGuides,
            C::UiPixelSnap,
            C::UiThemeDark,
````

В `crates/ferrum-editor/tests/context_menu.rs` заменить:

````rust
            "edit.paste_over",
            "edit.select_all",
            "view.fit_all"
        ]
    );
````

на:

````rust
            "edit.paste_over",
            "edit.select_all",
            "view.fit_all",
            "view.rulers"
        ]
    );
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    (CommandId::UiPixelSnap, &["Mod+Shift+Quote"]),
    (CommandId::PixelGrid, &["Shift+Quote"]),
    (CommandId::EditObject, &["Enter"]),
    (CommandId::TextBold, &["Mod+KeyB"]),
````

на:

````rust
    (CommandId::UiPixelSnap, &["Mod+Shift+Quote"]),
    (CommandId::PixelGrid, &["Shift+Quote"]),
    (CommandId::ViewRulers, &["Shift+KeyR"]),
    (CommandId::ViewGuides, &["Shift+KeyG"]),
    (CommandId::EditObject, &["Enter"]),
    (CommandId::TextBold, &["Mod+KeyB"]),
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 86);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 88);
}

````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
        Some(CommandId::Paste)
    );
    // Shift без своей команды не падает на команду без Shift: Shift+R — не «Прямоугольник».
    assert_eq!(win.lookup(&key("KeyR", false, false, true, false)), None);
    assert_eq!(
        Keymap::new(Platform::Mac).lookup(&key("KeyR", false, false, true, false)),
        None
    );
}
````

на:

````rust
        Some(CommandId::Paste)
    );
    // Shift без своей команды не падает на команду без Shift: Shift+O — не «Эллипс».
    assert_eq!(win.lookup(&key("KeyO", false, false, true, false)), None);
    assert_eq!(
        Keymap::new(Platform::Mac).lookup(&key("KeyO", false, false, true, false)),
        None
    );
    // А Shift+R — своя команда, «Линейки».
    assert_eq!(
        win.lookup(&key("KeyR", false, false, true, false)),
        Some(CommandId::ViewRulers)
    );
}
````

Создать `crates/ferrum-editor/tests/rulers.rs`:

````rust
//! Линейки и их команды (спека подсистемы 2, раздел 9).

mod common;

use common::*;
use ferrum_doc::{Guide, GuideAxis, GuideOwner};
use ferrum_editor::*;
use kurbo::Point;

fn checked(h: &H, c: CommandId) -> Option<bool> {
    h.ed.command_state(c).checked
}

#[test]
fn rulers_are_off_by_default_and_toggle_by_command_and_shift_r() {
    let mut h = H::new();
    assert!(h.ed.overlay().rulers.is_none());
    assert_eq!(checked(&h, CommandId::ViewRulers), Some(false));
    let steps = h.ed.step_count();
    h.ed.execute(CommandId::ViewRulers);
    assert!(h.ed.overlay().rulers.is_some());
    assert_eq!(checked(&h, CommandId::ViewRulers), Some(true));
    let ev = h.ed.take_events();
    assert!(
        ev.contains(&EditorEvent::ViewChanged {
            rulers: true,
            guides: true
        }),
        "{ev:?}"
    );
    assert!(ev.contains(&EditorEvent::NeedsRedraw));
    assert_eq!(h.ed.step_count(), steps, "настройка вида — не шаг");
    assert!(!h.ed.document().can_undo());
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    h.ed.handle(Input::KeyDown(KeyEvent::new("KeyR", shift)));
    assert!(h.ed.overlay().rulers.is_none());
    // Простая R — по-прежнему прямоугольник.
    h.ed.handle(Input::KeyDown(KeyEvent::new("KeyR", Modifiers::NONE)));
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
    assert!(h.ed.overlay().rulers.is_none());
}

#[test]
fn set_view_does_not_report_back() {
    let mut h = H::new();
    h.ed.take_events();
    h.ed.set_view(Some(true), Some(false));
    assert_eq!(checked(&h, CommandId::ViewRulers), Some(true));
    assert_eq!(checked(&h, CommandId::ViewGuides), Some(false));
    let ev = h.ed.take_events();
    assert!(
        !ev.iter()
            .any(|e| matches!(e, EditorEvent::ViewChanged { .. })),
        "настройка пришла от интерфейса: {ev:?}"
    );
    assert!(ev.contains(&EditorEvent::NeedsRedraw));
}

#[test]
fn guides_command_is_grey_without_rulers_and_hides_marks() {
    let mut h = H::new();
    let page = h.ed.page();
    h.ed.document_mut_for_tests()
        .set_guides(
            page,
            vec![],
            &[(GuideOwner::Page(page), vec![Guide::new(GuideAxis::X, 50.0)])],
        )
        .unwrap();
    h.ed.set_view(None, None);
    assert!(
        h.ed.overlay().guides.is_empty(),
        "как в Figma: без линеек не видны"
    );
    assert!(!h.ed.command_state(CommandId::ViewGuides).enabled);
    let events = h.ed.take_events().len();
    h.ed.execute(CommandId::ViewGuides);
    assert_eq!(
        checked(&h, CommandId::ViewGuides),
        Some(true),
        "серая не выполняется"
    );
    assert!(h.ed.take_events().len() <= events);

    h.ed.execute(CommandId::ViewRulers);
    assert_eq!(h.ed.overlay().guides.len(), 1);
    assert!(h.ed.command_state(CommandId::ViewGuides).enabled);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    h.ed.handle(Input::KeyDown(KeyEvent::new("KeyG", shift)));
    assert_eq!(checked(&h, CommandId::ViewGuides), Some(false));
    assert!(h.ed.overlay().guides.is_empty());
    assert!(h.ed.overlay().rulers.is_some(), "линейки остаются");
}

#[test]
fn origin_is_the_top_level_frame_under_selection_else_page_zero() {
    let mut h = H::new();
    h.ed.set_view(Some(true), None);
    let origin = |h: &H| h.ed.overlay().rulers.unwrap().origin;
    let band = |h: &H| h.ed.overlay().rulers.unwrap().band;
    let f = h.frame(None, 100.0, 50.0, 300.0, 200.0);
    let inner = h.frame(Some(f), 10.0, 10.0, 100.0, 100.0);
    let deep = h.rect(Some(inner), 5.0, 6.0, 20.0, 30.0);
    let loose = h.rect(None, 600.0, 600.0, 40.0, 40.0);
    assert_eq!(origin(&h), Point::ZERO, "без выделения — ноль страницы");
    assert_eq!(band(&h), None);

    h.ed.select(&[deep]);
    assert_eq!(
        origin(&h),
        Point::new(100.0, 50.0),
        "фрейм верхнего уровня, а не вложенный"
    );
    let b = band(&h).unwrap();
    assert_eq!((b.x0, b.y0, b.x1, b.y1), (115.0, 66.0, 135.0, 96.0));

    h.ed.select(&[f]);
    assert_eq!(origin(&h), Point::new(100.0, 50.0), "сам фрейм");

    h.ed.select(&[loose]);
    assert_eq!(origin(&h), Point::ZERO, "узел вне фрейма");

    h.ed.select(&[deep, loose]);
    assert_eq!(origin(&h), Point::ZERO, "разные корни");
    let b = band(&h).unwrap();
    assert_eq!((b.x0, b.x1), (115.0, 640.0), "полоса — общая рамка");

    // Двигаем фрейм — начало отсчёта за ним, а не из кэша.
    h.ed.select(&[deep]);
    h.set(
        f,
        ferrum_doc::PropsPatch {
            x: Some(10.0),
            ..Default::default()
        },
    );
    assert_eq!(origin(&h), Point::new(10.0, 50.0));
    // Повёрнутый фрейм — ноль страницы: оси линейки с его осями не совпадают.
    h.set(
        f,
        ferrum_doc::PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    assert_eq!(origin(&h), Point::ZERO);
}

#[test]
fn hidden_interface_hides_rulers() {
    let mut h = H::new();
    h.ed.set_view(Some(true), None);
    let mut s = h.ed.settings().clone();
    s.ui_hidden = true;
    h.ed.set_settings(s);
    assert!(h.ed.overlay().rulers.is_none());
    assert_eq!(
        checked(&h, CommandId::ViewRulers),
        Some(true),
        "настройка та же"
    );
}

#[test]
fn rulers_in_menus() {
    let menu = main_menu();
    let flat = format!("{menu:?}");
    assert!(flat.contains("ViewRulers") && flat.contains("ViewGuides"));
    let mut h = H::new();
    let items =
        h.ed.context_menu(ContextTarget::Canvas(Point::new(500.0, 500.0)));
    assert!(
        items
            .iter()
            .any(|i| matches!(i, MenuNode::Command(CommandId::ViewRulers))),
        "«Линейки» в меню пустого места (B-121): {items:?}"
    );
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[r]);
    let items =
        h.ed.context_menu(ContextTarget::Canvas(Point::new(120.0, 120.0)));
    assert!(!items
        .iter()
        .any(|i| matches!(i, MenuNode::Command(CommandId::ViewRulers))));
}
````

В `crates/ferrum-render/tests/render.rs` заменить:

````rust
    assert_eq!(stats.drawn, 2, "фрейм и ребёнок нарисованы");
}
````

на:

````rust
    assert_eq!(stats.drawn, 2, "фрейм и ребёнок нарисованы");
}

// ——— Линейки и направляющие (спека подсистемы 2, раздел 9) ———

fn overlay_frame(cam: &Camera, overlay: &ferrum_render::Overlay) -> Pixmap {
    let (w, h) = cam.device_size();
    let mut backend = CpuBackend::new(w, h);
    let mut r = Renderer::new();
    let scene = Scene::new();
    r.render(&mut backend, &scene, cam, overlay, &NoImages, Some(WHITE));
    backend.render()
}

fn rgb(img: &Pixmap, x: u16, y: u16) -> (u8, u8, u8) {
    let p = img.data()[y as usize * img.width() as usize + x as usize];
    (p.r, p.g, p.b)
}

#[test]
fn ruler_steps_are_round_and_keep_their_gap() {
    for zoom in [0.001, 0.02, 0.25, 0.3, 1.0, 1.4, 3.0, 8.0, 64.0, 256.0] {
        let (step, minor) = ferrum_render::ruler_steps(zoom);
        let mut k = step;
        while k >= 10.0 {
            k /= 10.0;
        }
        assert!(
            [1.0, 2.0, 5.0].iter().any(|m| (k - m).abs() < 1e-9),
            "круглый шаг: {step} при {zoom}"
        );
        assert!(step >= 1.0, "деления не чаще единицы мира");
        // Подписи не теснее 72 px, если шаг не упёрся в единицу мира.
        assert!(step * zoom >= 72.0 || step == 1.0, "{step} при {zoom}");
        if step > 1.0 {
            assert!(
                step * zoom < 72.0 * 5.0,
                "и не реже следующего шага: {step} при {zoom}"
            );
        }
        if let Some(m) = minor {
            assert!(m < step && m >= 1.0 && m * zoom >= 6.0, "{m} при {zoom}");
            assert_eq!((step / m).fract(), 0.0, "мелкие делят шаг нацело");
        }
    }
    assert_eq!(ferrum_render::ruler_steps(1.0).0, 100.0);
    assert_eq!(ferrum_render::ruler_steps(0.25).0, 500.0);
    assert_eq!(ferrum_render::ruler_steps(f64::NAN), (1.0, None));
    assert_eq!(ferrum_render::ruler_steps(0.0), (1.0, None));
}

#[test]
fn rulers_are_drawn_on_top_of_the_canvas_edges_only() {
    use ferrum_render::{Overlay, OverlayTheme, RulersOverlay, RULER_SIZE};
    let mut cam = Camera::new(Size::new(400.0, 300.0), 1.0);
    cam.set_center(Point::new(200.0, 150.0));
    let plain = overlay_frame(&cam, &Overlay::default());
    assert_eq!(rgb(&plain, 100, 5), (255, 255, 255));
    let overlay = Overlay {
        rulers: Some(RulersOverlay {
            origin: Point::ZERO,
            band: Some(Rect::new(100.0, 120.0, 160.0, 180.0)),
        }),
        ..Default::default()
    };
    let img = overlay_frame(&cam, &overlay);
    let ruler = OverlayTheme::dark().ruler.to_rgba8();
    let bg = (ruler.r, ruler.g, ruler.b);
    let size = RULER_SIZE as u16;
    // Фон линеек сверху и слева, вне делений и подписей.
    assert_eq!(rgb(&img, 60, 1), bg);
    assert_eq!(rgb(&img, 1, 60), bg);
    assert_eq!(rgb(&img, 1, 1), bg, "угол");
    // Холст дальше линеек не тронут.
    assert_eq!(rgb(&img, 200, size + 2), (255, 255, 255));
    assert_eq!(rgb(&img, size + 2, 200), (255, 255, 255));
    // Полоса выделения — другой цвет на обеих линейках и только в своих границах.
    assert_ne!(rgb(&img, 130, 1), bg);
    assert_ne!(rgb(&img, 1, 150), bg);
    assert_eq!(rgb(&img, 165, 1), bg);
    assert_eq!(rgb(&img, 1, 185), bg);
    // Деление на кратном есть, между кратными — нет.
    let dark = |x: u16, y: u16| rgb(&img, x, y) != bg;
    assert!(dark(200, size - 3), "деление на отметке 200 мира");
    assert!(!dark(245, size - 3), "между мелкими делениями пусто");
}

#[test]
fn rulers_follow_their_origin() {
    use ferrum_render::{Overlay, OverlayTheme, RulersOverlay, RULER_SIZE};
    let mut cam = Camera::new(Size::new(400.0, 300.0), 1.0);
    cam.set_center(Point::new(200.0, 150.0));
    let at = |origin: Point| {
        overlay_frame(
            &cam,
            &Overlay {
                rulers: Some(RulersOverlay { origin, band: None }),
                ..Default::default()
            },
        )
    };
    let ruler = OverlayTheme::dark().ruler.to_rgba8();
    let bg = (ruler.r, ruler.g, ruler.b);
    let tick = |img: &Pixmap, x: u16| rgb(img, x, RULER_SIZE as u16 - 3) != bg;
    let zero = at(Point::ZERO);
    assert!(tick(&zero, 100), "кратное 100 от нуля страницы");
    let moved = at(Point::new(37.0, 0.0));
    assert!(tick(&moved, 137), "кратное 100 от начала отсчёта");
    assert!(!tick(&moved, 100), "и уже не на прежнем месте");
}

#[test]
fn guides_page_spans_the_canvas_and_frame_only_its_segment() {
    use ferrum_render::{GuideMark, Overlay, OverlayTheme};
    let mut cam = Camera::new(Size::new(400.0, 300.0), 1.0);
    cam.set_center(Point::new(200.0, 150.0));
    let overlay = Overlay {
        guides: vec![
            GuideMark {
                line: kurbo::Line::new((120.0, 0.0), (120.0, 1.0)),
                infinite: true,
            },
            GuideMark {
                line: kurbo::Line::new((10.0, 100.0), (80.0, 100.0)),
                infinite: false,
            },
        ],
        ..Default::default()
    };
    let img = overlay_frame(&cam, &overlay);
    let g = OverlayTheme::dark().guide.to_rgba8();
    let guide = (g.r, g.g, g.b);
    for y in [0, 150, 299] {
        assert_eq!(rgb(&img, 120, y), guide, "бесконечная вся, y = {y}");
    }
    assert_eq!(rgb(&img, 40, 100), guide, "отрезок фрейма");
    assert_eq!(
        rgb(&img, 200, 100),
        (255, 255, 255),
        "дальше края фрейма нет"
    );
}
````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 86);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

на:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 88);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

В `crates/ferrum-wasm/tests/context_menu.rs` заменить:

````rust
    assert_eq!(current, list[1].id);
}
````

на:

````rust
    assert_eq!(current, list[1].id);
}

#[test]
fn rulers_setting_goes_through_set_view_and_the_command_reports_back() {
    let mut core = demo(10);
    core.command(Command::SetView {
        rulers: Some(true),
        guides: None,
    });
    let ev = core.take_events();
    assert!(
        !ev.iter().any(|e| matches!(e, CoreEvent::View { .. })),
        "настройка пришла от интерфейса"
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        CoreEvent::CommandStates { changed } if changed.iter().any(|s| s.key == "view.rulers" && s.checked == Some(true))
    )));
    core.command(Command::Execute {
        command: "view.rulers".into(),
    });
    let ev = core.take_events();
    assert!(ev.iter().any(|e| matches!(
        e,
        CoreEvent::View {
            rulers: false,
            guides: true
        }
    )));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test rulers`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-render --test render`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test boolean`
Expected: FAIL — `rulers` не собирается: нет `CommandId::{ViewRulers, ViewGuides}`, `Editor::set_view`, `EditorEvent::ViewChanged`, `Overlay.{rulers, guides}`; `render` не собирается: нет `ruler_steps`, `RulersOverlay`, `RULER_SIZE`, `GuideMark`; `boolean` собирается и падает на `boolean_and_outline_commands_are_in_the_keymap` (`left: 86, right: 88`). `e2e/app.spec.ts` и `e2e/context-menu.spec.ts` проверяются в шаге «Проверить»

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::FlipVertical => self.mirror(Vec2::new(1.0, -1.0)),
            C::PixelGrid => self.settings.pixel_grid = !self.settings.pixel_grid,
            C::EditObject => self.edit_object(),
            C::TextBold
````

на:

````rust
            C::FlipVertical => self.mirror(Vec2::new(1.0, -1.0)),
            C::PixelGrid => self.settings.pixel_grid = !self.settings.pixel_grid,
            C::ViewRulers => {
                self.settings.rulers = !self.settings.rulers;
                self.push_view();
            }
            C::ViewGuides => {
                self.settings.guides = !self.settings.guides;
                self.push_view();
            }
            C::EditObject => self.edit_object(),
            C::TextBold
````

В `crates/ferrum-editor/src/command_state.rs` заменить:

````rust
            C::ToggleUi => (true, checked(self.settings.ui_hidden)),
            C::PixelGrid => (true, checked(self.settings.pixel_grid)),
            C::EditObject => (edit && !text && !vector && f.editable, None),
            C::TextBold
````

на:

````rust
            C::ToggleUi => (true, checked(self.settings.ui_hidden)),
            C::PixelGrid => (true, checked(self.settings.pixel_grid)),
            C::ViewRulers => (true, checked(self.settings.rulers)),
            // Как в Figma, без линеек направляющих не видно — пункт серый.
            C::ViewGuides => (self.settings.rulers, checked(self.settings.guides)),
            C::EditObject => (edit && !text && !vector && f.editable, None),
            C::TextBold
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// Пиксельная сетка при зуме от 800% (Shift+', как в Figma).
    PixelGrid,
    /// Войти в правку выделенного: текст — правка текста, вектор и фигура — правка
    /// вектора (как Enter).
````

на:

````rust
    /// Пиксельная сетка при зуме от 800% (Shift+', как в Figma).
    PixelGrid,
    /// Линейки (Shift+R, как в Figma).
    ViewRulers,
    /// Направляющие (Shift+G, спека подсистемы 2, раздел 9): видны при линейках.
    ViewGuides,
    /// Войти в правку выделенного: текст — правка текста, вектор и фигура — правка
    /// вектора (как Enter).
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    )
    .toggle(),
    // Enter — физическая клавиша правки (как Esc у жеста); в таблице — для меню
    // и подписи.
````

на:

````rust
    )
    .toggle(),
    // Как в Figma: «Rulers» — Shift+R.
    cmd(
        C::ViewRulers,
        "view.rulers",
        G::View,
        "Линейки",
        &["линейка", "деления", "rulers", "ruler"],
        &["Shift+KeyR"],
    )
    .toggle(),
    // В справке Figma отдельного сочетания направляющих линеек нет (их прячет
    // Shift+R); Shift+G — «Layout guides» там же, и в раскладке Ferrum свободно.
    cmd(
        C::ViewGuides,
        "view.guides",
        G::View,
        "Направляющие",
        &["гайды", "линии", "guides", "guide"],
        &["Shift+KeyG"],
    )
    .toggle(),
    // Enter — физическая клавиша правки (как Esc у жеста); в таблице — для меню
    // и подписи.
````

В `crates/ferrum-editor/src/context_menu.rs` заменить:

````rust
                I(C::SelectAll),
                I(C::ZoomToFit),
            ];
        }
````

на:

````rust
                I(C::SelectAll),
                I(C::ZoomToFit),
                S,
                I(C::ViewRulers),
            ];
        }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Интерфейс скрыт (Ctrl/Cmd+\): отметка у «Скрыть интерфейс».
    pub ui_hidden: bool,
}

````

на:

````rust
    /// Интерфейс скрыт (Ctrl/Cmd+\): отметка у «Скрыть интерфейс».
    pub ui_hidden: bool,
    /// Линейки (Shift+R): настройка вида, хранит её интерфейс. По умолчанию выключены.
    pub rulers: bool,
    /// Направляющие (Shift+G): видны при включённых линейках. По умолчанию включены.
    pub guides: bool,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            theme: ThemeChoice::Dark,
            ui_hidden: false,
        }
    }
````

на:

````rust
            theme: ThemeChoice::Dark,
            ui_hidden: false,
            rulers: false,
            guides: true,
        }
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Вызвана команда интерфейса (`ui.*`): её выполняет основной поток.
    UiCommand(CommandId),
    /// Изменились состояния команд (доступна, отмечена) — только изменившиеся.
    CommandStates(Vec<(CommandId, CommandState)>),
````

на:

````rust
    /// Вызвана команда интерфейса (`ui.*`): её выполняет основной поток.
    UiCommand(CommandId),
    /// Команда сменила показ линеек или направляющих: интерфейс сохраняет настройку.
    ViewChanged {
        rulers: bool,
        guides: bool,
    },
    /// Изменились состояния команд (доступна, отмечена) — только изменившиеся.
    CommandStates(Vec<(CommandId, CommandState)>),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    theme: OverlayTheme,
    pixel_grid: bool,
    /// Измерения с Alt: кадр нужен, когда они изменились, и не нужен, пока те же.
    measures: Vec<Measure>,
````

на:

````rust
    theme: OverlayTheme,
    pixel_grid: bool,
    /// Линейки и направляющие: начало отсчёта и полоса зависят от сцены и выделения,
    /// они уже в ключе; у линий — номер пересчёта.
    rulers: bool,
    guides_shown: bool,
    guides_revision: u64,
    /// Измерения с Alt: кадр нужен, когда они изменились, и не нужен, пока те же.
    measures: Vec<Measure>,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Внутренний буфер «Копировать свойства».
    pub(crate) props_clip: Option<ferrum_doc::Props>,
}

````

на:

````rust
    /// Внутренний буфер «Копировать свойства».
    pub(crate) props_clip: Option<ferrum_doc::Props>,
    /// Линии направляющих страницы (`guides.rs`).
    pub(crate) guides: crate::guides::GuideCache,
    pub(crate) rulers_cache: crate::guides::RulersCache,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                theme: theme.clone(),
                pixel_grid: true,
                measures: Vec::new(),
                text: None,
````

на:

````rust
                theme: theme.clone(),
                pixel_grid: true,
                rulers: false,
                guides_shown: false,
                guides_revision: 0,
                measures: Vec::new(),
                text: None,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            props_clip: None,
            last_duplicate: None,
        })
    }
````

на:

````rust
            props_clip: None,
            last_duplicate: None,
            guides: Default::default(),
            rulers_cache: Default::default(),
        })
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            text_edit: text,
            vector_edit: vector,
        }
    }
````

на:

````rust
            text_edit: text,
            vector_edit: vector,
            rulers: self.rulers_overlay(),
            guides: self.guide_marks(),
            active_guide: None,
        }
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.sync_text_outlines();
        self.update_measures();
        let now = Seen {
            selection: self.selection.version(),
````

на:

````rust
        self.sync_text_outlines();
        self.update_measures();
        self.refresh_guides();
        let now = Seen {
            selection: self.selection.version(),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                theme: self.theme.clone(),
                pixel_grid: self.settings.pixel_grid,
                measures: self.measures.clone(),
                text: self.text_overlay().map(|o| o.overlay),
````

на:

````rust
                theme: self.theme.clone(),
                pixel_grid: self.settings.pixel_grid,
                rulers: self.rulers_visible(),
                guides_shown: self.guides_visible(),
                guides_revision: self.guides.revision,
                measures: self.measures.clone(),
                text: self.text_overlay().map(|o| o.overlay),
````

Создать `crates/ferrum-editor/src/guides.rs`:

````rust
//! Линейки и направляющие в редакторе (спека подсистемы 2, раздел 9): где они на
//! холсте, начало отсчёта линеек и кэш линий. Рисует их рендер, данные — в документе.
//!
//! Как в Figma, направляющие видны, пока включены линейки: «Вид → Направляющие»
//! дополнительно прячет их при включённых линейках.

use std::cell::RefCell;
use std::sync::Arc;

use ferrum_doc::{Guide, GuideAxis, GuideOwner, NodeKind};
use ferrum_render::{GuideMark, RulersOverlay};
use ferrum_scene::{Scene, Slot};
use kurbo::{Line, Point, Rect};

use crate::editor::Editor;
use crate::geometry::world_box;

/// Направляющая на холсте.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlacedGuide {
    pub owner: GuideOwner,
    pub guide: Guide,
    pub mark: GuideMark,
    /// Фрейм повёрнут: линия косая, к ней нет ни привязки, ни попадания.
    pub rotated: bool,
}

/// Линии текущей страницы; считаются заново, только когда изменился ключ.
#[derive(Debug, Default)]
pub(crate) struct GuideCache {
    /// Номер пересчёта: `FrameKey` сравнивает его, а не список.
    pub revision: u64,
    key: Option<(u64, u64, u64)>,
    pub lines: Arc<Vec<PlacedGuide>>,
    /// Фреймы верхнего уровня с направляющими и метка состава, при которой они
    /// найдены: обход корней страницы — только при изменении состава сцены.
    owners: Vec<Slot>,
    owners_key: Option<(u64, u64)>,
}

/// Начало отсчёта и полоса выделения: считаются обходом выделения, поэтому до
/// смены сцены или выделения хранятся (`Editor::overlay` зовётся на каждый кадр).
pub(crate) type RulersCache = RefCell<Option<((u64, u64, u64), RulersOverlay)>>;

/// Верхний узел цепочки родителей.
fn root_of(scene: &Scene, mut slot: Slot) -> Slot {
    while let Some(p) = scene.at(slot).parent {
        slot = p;
    }
    slot
}

/// Фрейм без поворота и наклона: его оси — оси мира.
pub(crate) fn axis_aligned(scene: &Scene, slot: Slot) -> bool {
    let [a, b, c, d, ..] = scene.at(slot).world.as_coeffs();
    b.abs() < 1e-9 && c.abs() < 1e-9 && a > 0.0 && d > 0.0
}

/// Линия фрейма в мире: от края до края его рамки. У повёрнутого — косая.
fn frame_line(scene: &Scene, slot: Slot, g: Guide) -> Line {
    let n = scene.at(slot);
    let (w, h) = (n.props.w, n.props.h);
    let (a, b) = match g.axis {
        GuideAxis::X => (Point::new(g.offset, 0.0), Point::new(g.offset, h)),
        GuideAxis::Y => (Point::new(0.0, g.offset), Point::new(w, g.offset)),
    };
    Line::new(n.world * a, n.world * b)
}

/// Линия страницы: концы задают только её ось, рисуется через весь холст.
pub(crate) fn page_line(g: Guide) -> Line {
    match g.axis {
        GuideAxis::X => Line::new((g.offset, 0.0), (g.offset, 1.0)),
        GuideAxis::Y => Line::new((0.0, g.offset), (1.0, g.offset)),
    }
}

impl Editor {
    /// Линейки на холсте: включены, и интерфейс не скрыт (Ctrl/Cmd+\ прячет и их).
    pub fn rulers_visible(&self) -> bool {
        self.settings.rulers && !self.settings.ui_hidden
    }

    /// Направляющие на холсте и как цели привязки: как в Figma — только при линейках.
    pub fn guides_visible(&self) -> bool {
        self.settings.rulers && self.settings.guides
    }

    /// Показ линеек и направляющих — настройка вида, хранит её интерфейс.
    /// `None` — не менять. Шага истории нет.
    pub fn set_view(&mut self, rulers: Option<bool>, guides: Option<bool>) {
        if let Some(v) = rulers {
            self.settings.rulers = v;
        }
        if let Some(v) = guides {
            self.settings.guides = v;
        }
        self.emit();
    }

    /// Сообщает интерфейсу новый показ: настройку хранит он.
    pub(crate) fn push_view(&mut self) {
        self.out
            .events
            .push(crate::editor::EditorEvent::ViewChanged {
                rulers: self.settings.rulers,
                guides: self.settings.guides,
            });
    }

    /// Начало отсчёта линеек — левый верхний угол фрейма верхнего уровня, под которым
    /// всё выделение (без поворота), иначе ноль страницы; полоса — рамка выделения.
    pub(crate) fn rulers_overlay(&self) -> Option<RulersOverlay> {
        if !self.rulers_visible() {
            return None;
        }
        let key = (
            self.scene_epoch,
            self.scene.change_counter(),
            self.selection.version(),
        );
        if let Some((k, r)) = *self.rulers_cache.borrow() {
            if k == key {
                return Some(r);
            }
        }
        let mut band: Option<Rect> = None;
        let mut top: Option<Option<Slot>> = None;
        for id in self.selection.iter() {
            let Some(slot) = self.scene.slot(*id) else {
                continue;
            };
            if let Some(b) = world_box(&self.scene, slot) {
                band = Some(band.map_or(b, |u| u.union(b)));
            }
            let root = root_of(&self.scene, slot);
            top = match top {
                None => Some(Some(root)),
                Some(Some(t)) if t == root => top,
                _ => Some(None),
            };
        }
        let origin = top
            .flatten()
            .filter(|s| self.scene.at(*s).props.kind == NodeKind::Frame)
            .filter(|s| axis_aligned(&self.scene, *s))
            .and_then(|s| world_box(&self.scene, s))
            .map_or(Point::ZERO, |b| b.origin());
        let r = RulersOverlay { origin, band };
        *self.rulers_cache.borrow_mut() = Some((key, r));
        Some(r)
    }

    /// Пересчитывает линии, если изменились сцена, страница или её направляющие.
    pub(crate) fn refresh_guides(&mut self) {
        let key = (
            self.scene.generation(),
            self.scene.change_counter(),
            self.doc.guides_revision(),
        );
        if self.guides.key == Some(key) {
            return;
        }
        let owners_key = (self.scene.generation(), self.scene.structure_stamp());
        if self.guides.owners_key != Some(owners_key) {
            let scene = &self.scene;
            self.guides.owners = scene
                .roots()
                .iter()
                .copied()
                .filter(|s| {
                    let p = &scene.at(*s).props;
                    p.kind == NodeKind::Frame && p.guides.is_some()
                })
                .collect();
            self.guides.owners_key = Some(owners_key);
        }
        let mut lines = Vec::new();
        for g in self.doc.page_guides(self.page) {
            lines.push(PlacedGuide {
                owner: GuideOwner::Page(self.page),
                guide: g,
                mark: GuideMark {
                    line: page_line(g),
                    infinite: true,
                },
                rotated: false,
            });
        }
        for &slot in &self.guides.owners {
            let Some(n) = self.scene.node_at(slot) else {
                continue;
            };
            let Some(list) = n.props.guides.as_ref() else {
                continue;
            };
            if !self.scene.is_visible(slot) {
                continue;
            }
            let rotated = !axis_aligned(&self.scene, slot);
            for g in list.iter() {
                lines.push(PlacedGuide {
                    owner: GuideOwner::Frame(n.id),
                    guide: *g,
                    mark: GuideMark {
                        line: frame_line(&self.scene, slot, *g),
                        infinite: false,
                    },
                    rotated,
                });
            }
        }
        // Пустой список остаётся пустым без новой ревизии, если и был пустым.
        if !(lines.is_empty() && self.guides.lines.is_empty()) {
            self.guides.lines = Arc::new(lines);
            self.guides.revision += 1;
        }
        self.guides.key = Some(key);
    }

    /// Метки направляющих для оверлея.
    pub(crate) fn guide_marks(&self) -> Vec<GuideMark> {
        if !self.guides_visible() {
            return Vec::new();
        }
        self.guides.lines.iter().map(|g| g.mark).collect()
    }
}
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
mod flatten;
pub mod geometry;
pub mod input;
pub mod keymap;
````

на:

````rust
mod flatten;
pub mod geometry;
mod guides;
pub mod input;
pub mod keymap;
````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
//! команд по ключам таблицы, подменю и разделителей. Порядок и состав повторяют главное
//! меню Figma UI3 без плагинов, виджетов и Dev Mode. В дереве — только команды,
//! которые уже есть: пункты следующих этапов (линейки) появятся вместе с ними,
//! а не серыми заглушками. Интерфейс получает дерево один раз и по пункту присылает
//! `Execute` с ключом команды.

````

на:

````rust
//! команд по ключам таблицы, подменю и разделителей. Порядок и состав повторяют главное
//! меню Figma UI3 без плагинов, виджетов и Dev Mode. В дереве — только команды,
//! которые уже есть, без серых заглушек. Интерфейс получает дерево один раз и по пункту присылает
//! `Execute` с ключом команды.

````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
            vec![
                I(C::PixelGrid),
                S,
                I(C::ToggleUi),
````

на:

````rust
            vec![
                I(C::PixelGrid),
                I(C::ViewRulers),
                I(C::ViewGuides),
                S,
                I(C::ToggleUi),
````

В `crates/ferrum-editor/src/vector_select.rs` заменить:

````rust
            | C::ToggleUi
            | C::PixelGrid
            | C::EditObject
            | C::Save => return false,
````

на:

````rust
            | C::ToggleUi
            | C::PixelGrid
            | C::ViewRulers
            | C::ViewGuides
            | C::EditObject
            | C::Save => return false,
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{Camera, CpuBackend, NoImages, Overlay, Renderer, TextEngine, LABEL_SIZE};
use ferrum_scene::Scene;
use kurbo::{Point, Size};
````

на:

````rust
use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{
    Camera, CpuBackend, GuideMark, NoImages, Overlay, Renderer, RulersOverlay, TextEngine,
    LABEL_SIZE,
};
use ferrum_scene::Scene;
use kurbo::{Point, Size};
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
/// (фреймы по 100: фрейм + 99 фигур) и PDF страницы с текстом и вектором в
/// 100 000 точек — в worker'е, без замирания интерфейса; цель — секунда.
fn export_section(report: &mut Report) {
    use ferrum_render::{ExportBudget, SvgOptions};
````

на:

````rust
/// (фреймы по 100: фрейм + 99 фигур) и PDF страницы с текстом и вектором в
/// 100 000 точек — в worker'е, без замирания интерфейса; цель — секунда.
/// Кадр с линейками и без них (спека подсистемы 2, раздел 12): вид каждый кадр
/// сдвигается, поэтому подписи делений меняются; замеры чередуются, чтобы загрузка
/// машины легла на обе стороны поровну. Самый тяжёлый случай — экран 2×.
fn rulers_section(report: &mut Report, scene: &Scene, base: &Camera) {
    const RUNS: usize = 41;
    for dpr in [1.0, 2.0] {
        let mut camera = Camera::new(VIEWPORT, dpr);
        camera.set_zoom(base.zoom());
        camera.set_center(base.center());
        let (w, h) = camera.device_size();
        let mut backend = CpuBackend::new(w, h);
        let mut renderer = Renderer::new();
        let plain = Overlay::default();
        let mut with = Overlay {
            rulers: Some(RulersOverlay {
                origin: Point::new(40.0, 40.0),
                band: Some(kurbo::Rect::new(3000.0, 1800.0, 3400.0, 2100.0)),
            }),
            ..Default::default()
        };
        renderer.render(&mut backend, scene, &camera, &plain, &NoImages, None);
        renderer.render(&mut backend, scene, &camera, &with, &NoImages, None);
        let (mut off, mut on) = (Vec::new(), Vec::new());
        for i in 0..RUNS {
            camera.pan_by(kurbo::Vec2::new(if i % 2 == 0 { 7.0 } else { -3.0 }, 2.0));
            for (overlay, runs) in [(&plain, &mut off), (&with, &mut on)] {
                backend.reset(w, h);
                let t = Instant::now();
                renderer.render(&mut backend, scene, &camera, overlay, &NoImages, None);
                runs.push(t.elapsed());
            }
        }
        let (off, on) = (median(off), median(on));
        report.gate(
            if dpr == 1.0 {
                "Кадр с линейками: прибавка к спискам кадра, 1920×1080"
            } else {
                "Кадр с линейками: прибавка к спискам кадра, 3840×2160 (2×)"
            },
            on.saturating_sub(off),
            0.5,
            "≤ 0.5 мс",
        );
        // Сами линейки на пустой сцене — с растеризацией CPU.
        let empty = Scene::new();
        let (mut off, mut on) = (Vec::new(), Vec::new());
        for i in 0..RUNS {
            camera.pan_by(kurbo::Vec2::new(if i % 2 == 0 { 7.0 } else { -3.0 }, 2.0));
            for (overlay, runs) in [(&plain, &mut off), (&with, &mut on)] {
                backend.reset(w, h);
                let t = Instant::now();
                renderer.render(&mut backend, &empty, &camera, overlay, &NoImages, None);
                std::hint::black_box(backend.render());
                runs.push(t.elapsed());
            }
        }
        let extra = median(on).saturating_sub(median(off));
        report.info(
            if dpr == 1.0 {
                "Кадр с линейками: прибавка с растеризацией CPU, 1920×1080"
            } else {
                "Кадр с линейками: прибавка с растеризацией CPU, 3840×2160 (2×)"
            },
            ms(extra),
        );
        // 2000 направляющих на экране — справочно.
        with.guides = (0..2000)
            .map(|i| GuideMark {
                line: kurbo::Line::new((i as f64 * 3.0, 0.0), (i as f64 * 3.0, 1.0)),
                infinite: true,
            })
            .collect();
        let (mut off, mut on) = (Vec::new(), Vec::new());
        for _ in 0..RUNS {
            for (overlay, runs) in [(&plain, &mut off), (&with, &mut on)] {
                backend.reset(w, h);
                let t = Instant::now();
                renderer.render(&mut backend, &empty, &camera, overlay, &NoImages, None);
                runs.push(t.elapsed());
            }
        }
        report.info(
            "Кадр с линейками и 2000 направляющими: прибавка к спискам кадра",
            ms(median(on).saturating_sub(median(off))),
        );
    }
}

fn export_section(report: &mut Report) {
    use ferrum_render::{ExportBudget, SvgOptions};
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
        ms(t.elapsed()),
    );

    // Вся страница при сильном отдалении: LOD и растровый кэш фреймов. Кэш
````

на:

````rust
        ms(t.elapsed()),
    );
    rulers_section(&mut report, &scene, &camera);

    // Вся страница при сильном отдалении: LOD и растровый кэш фреймов. Кэш
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use overlay::{
    format_size, Measure, MeasureKind, Overlay, OverlayTheme, TextEditOverlay, VectorEditOverlay,
    HANDLE_SIZE, LABEL_SIZE, MEASURE_DASH, MEASURE_GAP, PIXEL_GRID_MIN_ZOOM, TEXT_SELECTION_ALPHA,
    VECTOR_HANDLE_RADIUS, VECTOR_MAX_POINTS_DRAWN, VECTOR_POINT_RADIUS,
};
pub use pdf::MAX_PDF_SIDE;
````

на:

````rust
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use overlay::{
    format_size, ruler_steps, ActiveGuide, GuideMark, Measure, MeasureKind, Overlay, OverlayTheme,
    RulersOverlay, TextEditOverlay, VectorEditOverlay, HANDLE_SIZE, LABEL_SIZE, MEASURE_DASH,
    MEASURE_GAP, PIXEL_GRID_MIN_ZOOM, RULER_SIZE, TEXT_SELECTION_ALPHA, VECTOR_HANDLE_RADIUS,
    VECTOR_MAX_POINTS_DRAWN, VECTOR_POINT_RADIUS,
};
pub use pdf::MAX_PDF_SIDE;
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
//! Оверлеи поверх содержимого в экранных пикселях: пиксельная сетка, названия
//! фреймов, подсветка наведения, рамка выделения мышью, линии привязки, рамка
//! выделения с ручками, бейдж «W × H» и измерения расстояний с Alt. Толщины и
//! размеры задаются в CSS-пикселях и умножаются на `devicePixelRatio`.

use ferrum_doc::{NodeId, NodeKind};
````

на:

````rust
//! Оверлеи поверх содержимого в экранных пикселях: пиксельная сетка, названия
//! фреймов, направляющие, подсветка наведения, рамка выделения мышью, линии привязки,
//! рамка выделения с ручками, бейдж «W × H», измерения расстояний с Alt и линейки.
//! Толщины и размеры задаются в CSS-пикселях и умножаются на `devicePixelRatio`.

use ferrum_doc::{NodeId, NodeKind};
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
pub const MEASURE_DASH: f64 = 4.0;
pub const MEASURE_GAP: f64 = 3.0;

/// Цвета оверлеев из дизайн-системы Ferrum.
````

на:

````rust
pub const MEASURE_DASH: f64 = 4.0;
pub const MEASURE_GAP: f64 = 3.0;
/// Толщина линейки — CSS-пиксели (спека подсистемы 2, раздел 9).
pub const RULER_SIZE: f64 = 20.0;
/// Кегль делений линейки.
const RULER_LABEL_SIZE: f64 = 10.0;
/// Подписанные деления не ближе этого — CSS-пиксели.
const RULER_MIN_GAP: f64 = 72.0;
/// Мелкие деления не чаще этого — CSS-пиксели.
const RULER_MIN_MINOR_GAP: f64 = 6.0;
/// Наибольшее число делений за кадр по одной линейке: страховка от чисел камеры.
const RULER_MAX_TICKS: usize = 2048;

/// Цвета оверлеев из дизайн-системы Ferrum.
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
    /// Линии и плашки измерений с Alt (`danger`); текст плашки — `on_select`.
    pub measure: Rgba,
}

````

на:

````rust
    /// Линии и плашки измерений с Alt (`danger`); текст плашки — `on_select`.
    pub measure: Rgba,
    /// Фон линеек (`surface`), деления и подписи (`ink-muted`), граница (`line`).
    pub ruler: Rgba,
    pub ruler_ink: Rgba,
    pub ruler_line: Rgba,
    /// Направляющие.
    pub guide: Rgba,
}

````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
            pixel_grid: Rgba::from_rgba8(255, 255, 255, 26),
            measure: hex(0xff7a7a),
        }
    }
````

на:

````rust
            pixel_grid: Rgba::from_rgba8(255, 255, 255, 26),
            measure: hex(0xff7a7a),
            ruler: hex(RULER_DARK[0]),
            ruler_ink: hex(RULER_DARK[1]),
            ruler_line: hex(RULER_DARK[2]),
            guide: hex(0xff5a5f),
        }
    }
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
            pixel_grid: Rgba::from_rgba8(0, 0, 0, 20),
            measure: hex(0xc22c2c),
        }
    }
````

на:

````rust
            pixel_grid: Rgba::from_rgba8(0, 0, 0, 20),
            measure: hex(0xc22c2c),
            ruler: hex(RULER_LIGHT[0]),
            ruler_ink: hex(RULER_LIGHT[1]),
            ruler_line: hex(RULER_LIGHT[2]),
            guide: hex(0xe5313a),
        }
    }
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
}

/// Что показать поверх содержимого. Заполняет редактор (этап 3).
#[derive(Debug, Clone)]
````

на:

````rust
}

/// Линейки: начало отсчёта и выделение полосой — в мировых координатах.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RulersOverlay {
    pub origin: Point,
    /// Рамка выделения: её стороны подсвечены на линейках.
    pub band: Option<Rect>,
}

/// Направляющая в мировых координатах. Бесконечная (страницы) рисуется через весь
/// холст, остальные — отрезком `line` (фрейм: от края до края).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuideMark {
    pub line: Line,
    pub infinite: bool,
}

/// Тянущаяся направляющая: линия и значение на плашке (от начала отсчёта линеек).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveGuide {
    pub mark: GuideMark,
    pub value: f64,
}

/// Что показать поверх содержимого. Заполняет редактор (этап 3).
#[derive(Debug, Clone)]
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
    /// Правка вектора: сеть, точки и ручки.
    pub vector_edit: Option<VectorEditOverlay>,
}

````

на:

````rust
    /// Правка вектора: сеть, точки и ручки.
    pub vector_edit: Option<VectorEditOverlay>,
    /// Линейки (спека подсистемы 2, раздел 9); `None` — выключены.
    pub rulers: Option<RulersOverlay>,
    /// Направляющие страницы и фреймов.
    pub guides: Vec<GuideMark>,
    /// Направляющая, которую сейчас тянут.
    pub active_guide: Option<ActiveGuide>,
}

````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
            text_edit: None,
            vector_edit: None,
        }
    }
}

/// Координата тонкой линии, выровненная по пикселям: линия нечётной толщины
````

на:

````rust
            text_edit: None,
            vector_edit: None,
            rulers: None,
            guides: Vec::new(),
            active_guide: None,
        }
    }
}

/// Цвета линеек: как `--surface`, `--ink-muted` и `--line` темы интерфейса.
const RULER_DARK: [u32; 3] = [0x232120, 0xa9a39c, 0x3b3835];
const RULER_LIGHT: [u32; 3] = [0xffffff, 0x655f58, 0xe4e1dd];

/// Координата тонкой линии, выровненная по пикселям: линия нечётной толщины
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
        frame_labels(backend, text, scene, camera, overlay);
    }
    // Скрытый узел (или узел внутри скрытого) на холсте не виден — и обводки
    // наведения у него нет, даже если редактор передал его из панели слоёв.
````

на:

````rust
        frame_labels(backend, text, scene, camera, overlay);
    }
    guides(backend, view, dpr, theme.guide, overlay);
    // Скрытый узел (или узел внутри скрытого) на холсте не виден — и обводки
    // наведения у него нет, даже если редактор передал его из панели слоёв.
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
}

````

на:

````rust
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
    if let Some(r) = &overlay.rulers {
        rulers(backend, text, camera, theme, r);
    }
    if let Some(a) = &overlay.active_guide {
        active_guide_badge(backend, text, view, dpr, theme, a);
    }
}

/// Концы линии на экране: у бесконечной — от края до края холста по её оси.
fn guide_segment(m: &GuideMark, view: Affine, screen: Rect, dpr: f64) -> Option<(Point, Point)> {
    let (a, b) = (view * m.line.p0, view * m.line.p1);
    if !m.infinite {
        // Оси параллельны — линия выравнивается по пикселям; повёрнутый фрейм — как есть.
        let (a, b) = if a.x == b.x {
            let x = crisp(a.x, dpr);
            (Point::new(x, a.y), Point::new(x, b.y))
        } else if a.y == b.y {
            let y = crisp(a.y, dpr);
            (Point::new(a.x, y), Point::new(b.x, y))
        } else {
            (a, b)
        };
        return visible(a, b, screen);
    }
    if a.x == b.x {
        let x = crisp(a.x, dpr);
        (x >= screen.x0 && x <= screen.x1)
            .then_some((Point::new(x, screen.y0), Point::new(x, screen.y1)))
    } else {
        let y = crisp(a.y, dpr);
        (y >= screen.y0 && y <= screen.y1)
            .then_some((Point::new(screen.x0, y), Point::new(screen.x1, y)))
    }
}

/// Направляющие. Параллельные осям — прямоугольниками: штрих всех сразу одним путём
/// на тысячах линий стоил десятки миллисекунд, заливка выровненного прямоугольника
/// ничего не сглаживает. Косые (повёрнутый фрейм) — штрихом.
fn guides(
    backend: &mut impl RenderBackend,
    view: Affine,
    dpr: f64,
    color: Rgba,
    overlay: &Overlay,
) {
    if overlay.guides.is_empty() && overlay.active_guide.is_none() {
        return;
    }
    let (w, h) = backend.size();
    let screen = Rect::new(0.0, 0.0, w as f64, h as f64);
    let half = dpr / 2.0;
    let mut slanted = BezPath::new();
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(color);
    let active = overlay.active_guide.as_ref().map(|a| &a.mark);
    for m in overlay.guides.iter().chain(active) {
        let Some((a, b)) = guide_segment(m, view, screen, dpr) else {
            continue;
        };
        if a.x == b.x {
            backend.fill_rect(&Rect::new(
                a.x - half,
                a.y.min(b.y),
                a.x + half,
                a.y.max(b.y),
            ));
        } else if a.y == b.y {
            backend.fill_rect(&Rect::new(
                a.x.min(b.x),
                a.y - half,
                a.x.max(b.x),
                a.y + half,
            ));
        } else {
            slanted.move_to(a);
            slanted.line_to(b);
        }
    }
    if !slanted.elements().is_empty() {
        backend.stroke_path(&slanted, &Stroke::new(dpr));
    }
}

/// Шаг подписанных делений в единицах мира при зуме `zoom` и шаг мелких (`None` —
/// мелких нет): 1, 2, 5, 10, 20, 50… Деления не бывают чаще единицы мира.
pub fn ruler_steps(zoom: f64) -> (f64, Option<f64>) {
    if !(zoom.is_finite() && zoom > 0.0) {
        return (1.0, None);
    }
    let want = (RULER_MIN_GAP / zoom).max(1.0);
    let mut decade = 10f64.powf(want.log10().floor());
    let mut step = None;
    // Запас одного десятка: `log10` у точных степеней бывает на волос меньше.
    for _ in 0..2 {
        for k in [1.0, 2.0, 5.0] {
            if step.is_none() && k * decade >= want {
                step = Some((k, k * decade));
            }
        }
        decade *= 10.0;
    }
    let (k, step) = step.unwrap_or((1.0, want));
    let parts = if k == 2.0 { 4.0 } else { 5.0 };
    // 2 делится на 4 не нацело: мелкие только целые.
    let minor = step / parts;
    let minor = if minor >= 1.0 && minor.fract() == 0.0 {
        Some(minor)
    } else if step > 1.0 && (step / 2.0).fract() == 0.0 {
        Some(step / 2.0)
    } else {
        None
    };
    (step, minor.filter(|m| m * zoom >= RULER_MIN_MINOR_GAP))
}

/// Подпись деления. `ceil` от малого отрицательного даёт −0: подписью был бы «-0».
fn tick_label(v: f64) -> String {
    format_size(v + 0.0)
}

/// Первое кратное `step` не меньше `lo` и число кратных до `hi` (не больше предела).
fn ticks(lo: f64, hi: f64, step: f64) -> (f64, usize) {
    let first = (lo / step).ceil() * step;
    let n = ((hi - first) / step).floor();
    let n = if n.is_finite() && n >= 0.0 {
        (n as usize + 1).min(RULER_MAX_TICKS)
    } else {
        0
    };
    (first, n)
}

/// Линейки сверху и слева: полоса выделения, деления и подписи — в том же кадре,
/// отдельным слоем поверх оверлеев. Подписи — один размер, кэш `TextEngine`.
fn rulers(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    camera: &Camera,
    theme: &OverlayTheme,
    r: &RulersOverlay,
) {
    let dpr = camera.effective_dpr();
    let size = RULER_SIZE * dpr;
    let (w, h) = backend.size();
    let (w, h) = (w as f64, h as f64);
    let view = camera.view_transform();
    let scale = view.as_coeffs()[0];
    let tx = view.as_coeffs()[4];
    let ty = view.as_coeffs()[5];
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.ruler);
    backend.fill_rect(&Rect::new(0.0, 0.0, w, size));
    backend.fill_rect(&Rect::new(0.0, size, size, h));

    if let Some(b) = r.band {
        backend.set_color(theme.select.multiply_alpha(0.25));
        let (x0, x1) = (tx + b.x0 * scale, tx + b.x1 * scale);
        let (y0, y1) = (ty + b.y0 * scale, ty + b.y1 * scale);
        if x1 >= size && x0 <= w {
            backend.fill_rect(&Rect::new(x0.max(size), 0.0, x1.min(w), size));
        }
        if y1 >= size && y0 <= h {
            backend.fill_rect(&Rect::new(0.0, y0.max(size), size, y1.min(h)));
        }
    }

    let zoom = camera.zoom();
    let (step, minor) = ruler_steps(zoom);
    // Значения делений: мир минус начало отсчёта. Видимое — на столько-то правее
    // линейки слева и ниже линейки сверху.
    let lo_x = (size - tx) / scale - r.origin.x;
    let hi_x = (w - tx) / scale - r.origin.x;
    let lo_y = (size - ty) / scale - r.origin.y;
    let hi_y = (h - ty) / scale - r.origin.y;
    let to_x = |v: f64| crisp(tx + (v + r.origin.x) * scale, dpr);
    let to_y = |v: f64| crisp(ty + (v + r.origin.y) * scale, dpr);

    let mut lines = BezPath::new();
    // Граница линеек.
    let edge = crisp(size - dpr, dpr);
    lines.move_to((size, edge));
    lines.line_to((w, edge));
    lines.move_to((edge, size));
    lines.line_to((edge, h));
    backend.set_color(theme.ruler_line);
    backend.stroke_path(&lines, &Stroke::new(dpr));

    let mut lines = BezPath::new();
    let (major_len, minor_len) = (6.0 * dpr, 3.0 * dpr);
    if let Some(m) = minor {
        let (first, n) = ticks(lo_x, hi_x, m);
        for i in 0..n {
            let x = to_x(first + i as f64 * m);
            lines.move_to((x, size - minor_len));
            lines.line_to((x, size));
        }
        let (first, n) = ticks(lo_y, hi_y, m);
        for i in 0..n {
            let y = to_y(first + i as f64 * m);
            lines.move_to((size - minor_len, y));
            lines.line_to((size, y));
        }
    }
    let (first_x, nx) = ticks(lo_x, hi_x, step);
    for i in 0..nx {
        let x = to_x(first_x + i as f64 * step);
        lines.move_to((x, size - major_len));
        lines.line_to((x, size));
    }
    let (first_y, ny) = ticks(lo_y, hi_y, step);
    for i in 0..ny {
        let y = to_y(first_y + i as f64 * step);
        lines.move_to((size - major_len, y));
        lines.line_to((size, y));
    }
    backend.set_color(theme.ruler_ink);
    backend.stroke_path(&lines, &Stroke::new(dpr));

    let label_size = RULER_LABEL_SIZE * dpr;
    let gap = 3.0 * dpr;
    let room = RULER_MIN_GAP * dpr;
    for i in 0..nx {
        let v = first_x + i as f64 * step;
        if let Some(l) = text.label(&tick_label(v), label_size, 400, room) {
            l.draw(
                backend,
                Point::new(to_x(v) + gap, size - 8.0 * dpr),
                theme.ruler_ink,
            );
        }
    }
    for i in 0..ny {
        let v = first_y + i as f64 * step;
        if let Some(l) = text.label(&tick_label(v), label_size, 400, room) {
            // Слева подписи идут снизу вверх, как в Figma.
            l.draw_rotated(
                backend,
                Point::new(size - 8.0 * dpr, to_y(v) - gap),
                theme.ruler_ink,
            );
        }
    }
    backend.set_transform(Affine::IDENTITY);
}

/// Плашка значения у тянущейся направляющей: на линейке, если она включена, иначе
/// у края холста.
fn active_guide_badge(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    view: Affine,
    dpr: f64,
    theme: &OverlayTheme,
    a: &ActiveGuide,
) {
    let Some(label) = text.label(&format_size(a.value), LABEL_SIZE * dpr, 500, 200.0 * dpr) else {
        return;
    };
    let p = view * a.mark.line.p0;
    let vertical = a.mark.line.p0.x == a.mark.line.p1.x;
    let top = if vertical {
        2.0 * dpr
    } else {
        p.y - badge_height(&label, dpr) / 2.0
    };
    let center = if vertical {
        p.x
    } else {
        RULER_SIZE * dpr + label.width / 2.0 + 8.0 * dpr
    };
    badge(
        backend,
        &label,
        dpr,
        center,
        top,
        theme.guide,
        theme.on_select,
    );
}

````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust

    #[test]
    fn grid_lines_are_bounded_and_exact() {
        assert_eq!(
````

на:

````rust

    #[test]
    fn tick_label_has_no_negative_zero() {
        let (first, _) = ticks(-50.0, 300.0, 100.0);
        assert!(first == 0.0 && first.is_sign_negative(), "сам ceil даёт −0");
        assert_eq!(tick_label(first), "0");
        assert_eq!(tick_label(-100.0), "-100");
    }

    #[test]
    fn grid_lines_are_bounded_and_exact() {
        assert_eq!(
````

В `crates/ferrum-render/src/text.rs` заменить:

````rust
            backend.fill_path(path, Fill::NonZero);
        }
    }
}

struct Pen(BezPath);
````

на:

````rust
            backend.fill_path(path, Fill::NonZero);
        }
    }

    /// То же, повёрнутое на 90° против часовой: текст идёт снизу вверх (левая линейка).
    pub fn draw_rotated(&self, backend: &mut impl RenderBackend, origin: Point, color: Rgba) {
        backend.set_color(color);
        let turn =
            Affine::translate(origin.to_vec2()) * Affine::rotate(-std::f64::consts::FRAC_PI_2);
        for (path, at, scale) in &self.glyphs {
            backend.set_transform(
                turn * Affine::translate(at.to_vec2()) * Affine::scale_non_uniform(*scale, -*scale),
            );
            backend.fill_path(path, Fill::NonZero);
        }
    }
}

struct Pen(BezPath);
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                }
                EditorEvent::PagesChanged => self.ui.pages = true,
                EditorEvent::Clipboard(bytes) => self.events.push(CoreEvent::Clipboard {
                    bytes: ByteBuf::from(bytes),
````

на:

````rust
                }
                EditorEvent::PagesChanged => self.ui.pages = true,
                EditorEvent::ViewChanged { rulers, guides } => {
                    self.events.push(CoreEvent::View { rulers, guides })
                }
                EditorEvent::Clipboard(bytes) => self.events.push(CoreEvent::Clipboard {
                    bytes: ByteBuf::from(bytes),
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
                self.editor.set_settings(s);
            }
            // Команды этапа 4 разбирает `Core::command`. Список полный, без `_`:
            // новая команда без ветки здесь — ошибка сборки, а не молчаливый пропуск.
````

на:

````rust
                self.editor.set_settings(s);
            }
            Command::SetView { rulers, guides } => self.editor.set_view(rulers, guides),
            // Команды этапа 4 разбирает `Core::command`. Список полный, без `_`:
            // новая команда без ветки здесь — ошибка сборки, а не молчаливый пропуск.
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        pixel_snap: bool,
    },
    // ——— Подсистема 3, этап 1: шрифты ———
    /// Список шрифтов источника: вшитые (манифест сборки — его читает worker ядра)
````

на:

````rust
        pixel_snap: bool,
    },
    /// Показ линеек и направляющих (спека подсистемы 2, разделы 9 и 11): настройка
    /// вида, не шаг истории. Нет поля — не менять.
    SetView {
        rulers: Option<bool>,
        guides: Option<bool>,
    },
    // ——— Подсистема 3, этап 1: шрифты ———
    /// Список шрифтов источника: вшитые (манифест сборки — его читает worker ядра)
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    RequestImage,
    ToggleUi,
    /// Сохранённый файл `.ferrum`.
    Saved {
````

на:

````rust
    RequestImage,
    ToggleUi,
    /// Команда сменила показ линеек или направляющих: интерфейс сохраняет настройку.
    View {
        rulers: bool,
        guides: bool,
    },
    /// Сохранённый файл `.ferrum`.
    Saved {
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-editor -p ferrum-render -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-render --example render_report | grep -E "FAIL|Кадр с линейками"`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/app.spec.ts e2e/context-menu.spec.ts --project=chromium`

Expected: PASS: fmt и clippy без замечаний; тесты без падений (новые: `ferrum-editor/tests/rulers.rs` — 6, `ferrum-render/tests/render.rs` — +4, `ferrum-wasm/tests/context_menu.rs` — +1; весь workspace — 1105 passed; команд 88); `render_report`: FAIL нет, «Кадр с линейками: прибавка к спискам кадра» ≈ 0,15 мс (1920×1080) и ≈ 0,22 мс (2×), цель ≤ 0,5 мс; wasm собран, typecheck без ошибок; E2E `app`, `context-menu` в Chromium без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts apps/web/e2e/context-menu.spec.ts crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/command_state.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/context_menu.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/guides.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/menu.rs crates/ferrum-editor/src/vector_select.rs crates/ferrum-editor/tests/command_table.rs crates/ferrum-editor/tests/context_menu.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/rulers.rs crates/ferrum-render/examples/render_report.rs crates/ferrum-render/src/lib.rs crates/ferrum-render/src/overlay.rs crates/ferrum-render/src/text.rs crates/ferrum-render/tests/render.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/boolean.rs crates/ferrum-wasm/tests/context_menu.rs
git commit -F - <<'EOF'
feat(render,editor): линейки в кадре, команды «Линейки» и «Направляющие»

Линейки рисует ядро последним слоем оверлея: деления под зум, полоса выделения,
начало отсчёта — фрейм верхнего уровня под выделением. Shift+R и Shift+G, пункты
«Вид» и меню пустого места (B-121), `SetView` и событие `View`. Строки замера в
`render_report`: прибавка к кадру ≤ 0,5 мс.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3: Направляющие протяжкой, привязка, `SetGuides`

Спека §9, §11: протяжка из линейки создаёт линию у фрейма верхнего уровня под курсором, иначе у страницы; за неё можно тянуть, на своей линейке — удаляется; привязка как к краям узлов, 4 экранных пикселя. Здесь — жест `GuideDrag`, `press_tool`/`release_tool`, `Snapper::with_guides`, щелчок сквозь линию, команда `SetGuides`, строки «Линейки и 2000 линий» в `editor_report`. Удержание клавиши больше не мигает переключателями.

**Files:**
- Modify: `crates/ferrum-editor/examples/editor_report.rs`, `crates/ferrum-editor/src/context_menu.rs`, `crates/ferrum-editor/src/editor.rs`, `crates/ferrum-editor/src/guides.rs`, `crates/ferrum-editor/src/snap.rs`, `crates/ferrum-editor/src/tools/draw.rs`, `crates/ferrum-editor/src/tools/mod.rs`, `crates/ferrum-editor/src/tools/select.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `crates/ferrum-wasm/src/protocol.rs`
- Test: `crates/ferrum-editor/tests/guides.rs` (новый), `crates/ferrum-wasm/tests/context_menu.rs`

**Interfaces:**
- Consumes: `Document::set_guides` (задача 1); `Editor::{set_view, rulers_visible, guides_visible, rulers_overlay, refresh_guides}` (задача 2), `Snapper`, `SNAP_THRESHOLD_PX`, `DRAG_THRESHOLD_PX`, `PointerEvent`, `Phase`, `EditPhase`, `ToolKind`, `Cursor`.
- Produces: `pub const GUIDE_HIT_PX: f64 = 3.0`; `Editor::set_guides(owner, guides, phase)`; внутри крейта — `GuideDrag`, `GuideHover`, `PlacedGuide { owner, guide, mark, rotated }`, `Snapper::{with_guides, skipping_guide}`, `Editor::{press_tool, release_tool, guide_press, guide_drag_move, guide_drag_up, cancel_guides, active_guide, snap_guides}`; `Command::SetGuides { owner, guides, phase }`, `GuideOwnerDto`, `GuideAxisDto`, `GuideDto`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/guides.rs`:

````rust
//! Направляющие: протяжка, привязка, щелчки сквозь линию, отмена (спека подсистемы 2,
//! раздел 9). Стенд: зум 1, экран совпадает с миром; линейка — 20 px сверху и слева.

mod common;

use common::*;
use ferrum_doc::{Guide, GuideAxis, GuideOwner, NodeId, PropsPatch, MAX_GUIDES};
use ferrum_editor::*;
use ferrum_render::Camera;
use kurbo::Point;

const NONE: Modifiers = Modifiers::NONE;

fn gx(v: f64) -> Guide {
    Guide::new(GuideAxis::X, v)
}
fn gy(v: f64) -> Guide {
    Guide::new(GuideAxis::Y, v)
}

fn with_rulers() -> H {
    let mut h = H::new();
    h.ed.set_view(Some(true), Some(true));
    h.ed.take_events();
    h
}

fn page_guides(h: &H) -> Vec<Guide> {
    h.ed.document().page_guides(h.ed.page())
}

fn frame_guides(h: &mut H, f: NodeId) -> Vec<Guide> {
    h.props(f).guides.map(|g| g.to_vec()).unwrap_or_default()
}

/// Направляющая страницы одним шагом через документ.
fn seed_page(h: &mut H, guides: Vec<Guide>) {
    let page = h.ed.page();
    h.ed.set_guides(GuideOwner::Page(page), guides, Phase::Commit)
        .unwrap();
}

#[test]
fn drag_from_top_ruler_creates_horizontal_page_guide_in_one_step() {
    let mut h = with_rulers();
    let steps = h.ed.step_count();
    let tool = h.ed.tool_kind();
    h.down(300.0, 10.0, NONE);
    assert_eq!(h.ed.selection().len(), 0);
    h.mv(300.0, 150.0, NONE);
    assert!(h.ed.is_busy(), "пока кнопка нажата — только превью");
    assert!(page_guides(&h).is_empty(), "документ не тронут");
    let a = h.ed.overlay().active_guide.expect("линия видна");
    assert!(a.mark.infinite);
    assert_eq!(a.value, 150.0);
    h.up(300.0, 150.0, NONE);
    assert_eq!(page_guides(&h), [gy(150.0)]);
    assert_eq!(h.ed.step_count(), steps + 1, "ровно один шаг");
    assert_eq!(h.ed.tool_kind(), tool);
    assert!(h.ed.overlay().active_guide.is_none());
    assert_eq!(h.ed.overlay().guides.len(), 1);
    h.ed.execute(CommandId::Undo);
    assert!(page_guides(&h).is_empty());
    assert!(h.ed.overlay().guides.is_empty(), "оверлей после отмены");
    h.ed.execute(CommandId::Redo);
    assert_eq!(h.ed.overlay().guides.len(), 1, "оверлей после повтора");
    assert_eq!(page_guides(&h), [gy(150.0)]);
}

#[test]
fn drag_from_left_ruler_creates_vertical_guide_and_uses_world_coordinates() {
    let mut h = with_rulers();
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_zoom(2.0);
    cam.set_center(Point::new(100.0, 100.0));
    h.ed.set_camera(cam);
    h.drag((10.0, 300.0), (500.0, 300.0), NONE);
    // Экран 500 при зуме 2 и центре 100: мир 100 + (500 − 500) / 2.
    assert_eq!(page_guides(&h), [gx(100.0)]);
}

#[test]
fn guide_goes_to_the_top_level_frame_under_the_cursor() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 300.0, 300.0);
    let inner = h.frame(Some(f), 50.0, 50.0, 100.0, 100.0);
    // Вглубь вложенного: всё равно фрейм верхнего уровня, смещение — от его угла.
    h.drag((10.0, 170.0), (170.0, 170.0), NONE);
    assert_eq!(frame_guides(&mut h, f), [gx(70.0)]);
    assert!(frame_guides(&mut h, inner).is_empty());
    assert!(page_guides(&h).is_empty());
    // Линия фрейма — только в его границах и едет вместе с ним.
    let m = h.ed.overlay().guides[0];
    assert!(!m.infinite);
    assert_eq!(
        (m.line.p0.x, m.line.p0.y, m.line.p1.y),
        (170.0, 100.0, 400.0)
    );
    h.set(
        f,
        PropsPatch {
            x: Some(200.0),
            ..Default::default()
        },
    );
    assert_eq!(h.ed.overlay().guides[0].line.p0.x, 270.0);
}

#[test]
fn hidden_locked_and_rotated_frames_do_not_own_guides() {
    for patch in [
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    ] {
        let mut h = with_rulers();
        let f = h.frame(None, 100.0, 100.0, 300.0, 300.0);
        h.set(f, patch);
        h.drag((10.0, 220.0), (250.0, 220.0), NONE);
        assert_eq!(page_guides(&h), [gx(250.0)]);
        assert!(frame_guides(&mut h, f).is_empty());
    }
}

#[test]
fn nested_top_level_is_found_even_without_fill() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 300.0, 300.0);
    h.set(
        f,
        PropsPatch {
            fills: Some(vec![]),
            ..Default::default()
        },
    );
    h.drag((10.0, 220.0), (250.0, 220.0), NONE);
    assert_eq!(frame_guides(&mut h, f), [gx(150.0)]);
}

#[test]
fn dragged_guide_moves_in_one_step_and_back_on_ruler_deletes() {
    let mut h = with_rulers();
    seed_page(&mut h, vec![gy(150.0), gx(600.0)]);
    let steps = h.ed.step_count();
    let list = |h: &H| page_guides(h);
    h.drag((300.0, 150.0), (300.0, 250.0), NONE);
    assert_eq!(list(&h), [gy(250.0), gx(600.0)], "порядок тот же");
    assert_eq!(h.ed.step_count(), steps + 1);
    // Во время протяжки прежней линии нет, новая — активная.
    h.down(300.0, 250.0, NONE);
    h.mv(300.0, 300.0, NONE);
    assert_eq!(h.ed.overlay().guides.len(), 1, "прежняя скрыта превью");
    assert!(h.ed.overlay().active_guide.is_some());
    // Над своей линейкой линии нет: отпускание удалит.
    h.mv(300.0, 12.0, NONE);
    assert!(h.ed.overlay().active_guide.is_none());
    h.up(300.0, 12.0, NONE);
    assert_eq!(list(&h), [gx(600.0)]);
    assert_eq!(h.ed.step_count(), steps + 2, "удаление — один шаг");
    h.ed.execute(CommandId::Undo);
    assert_eq!(list(&h), [gy(250.0), gx(600.0)]);
}

#[test]
fn new_guide_released_over_its_ruler_is_not_created() {
    let mut h = with_rulers();
    let steps = h.ed.step_count();
    h.down(300.0, 10.0, NONE);
    h.mv(300.0, 15.0, NONE);
    h.up(300.0, 15.0, NONE);
    assert!(page_guides(&h).is_empty());
    assert_eq!(h.ed.step_count(), steps);
    assert!(!h.ed.document().can_undo());
}

#[test]
fn guide_moves_between_page_and_frame_in_one_step() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 300.0, 300.0);
    seed_page(&mut h, vec![gx(600.0)]);
    let steps = h.ed.step_count();
    h.drag((600.0, 300.0), (250.0, 300.0), NONE);
    assert!(page_guides(&h).is_empty());
    assert_eq!(frame_guides(&mut h, f), [gx(150.0)]);
    assert_eq!(h.ed.step_count(), steps + 1);
    // И обратно.
    h.drag((250.0, 300.0), (700.0, 300.0), NONE);
    assert_eq!(page_guides(&h), [gx(700.0)]);
    assert!(frame_guides(&mut h, f).is_empty());
    h.ed.execute(CommandId::Undo);
    assert_eq!(frame_guides(&mut h, f), [gx(150.0)]);
    h.ed.execute(CommandId::Undo);
    assert_eq!(page_guides(&h), [gx(600.0)]);
}

#[test]
fn escape_and_blur_cancel_without_a_trace() {
    let mut h = with_rulers();
    seed_page(&mut h, vec![gy(150.0)]);
    let steps = h.ed.step_count();
    h.down(300.0, 150.0, NONE);
    h.mv(300.0, 300.0, NONE);
    h.key("Escape", NONE);
    assert!(!h.ed.is_busy());
    assert_eq!(page_guides(&h), [gy(150.0)]);
    assert_eq!(h.ed.overlay().guides.len(), 1, "линия снова на месте");
    h.up(300.0, 300.0, NONE);
    assert_eq!(
        page_guides(&h),
        [gy(150.0)],
        "отпускание после отмены ничего не пишет"
    );
    assert_eq!(h.ed.step_count(), steps);

    h.down(300.0, 10.0, NONE);
    h.mv(300.0, 300.0, NONE);
    h.ev(Input::Blur);
    assert!(!h.ed.is_busy());
    assert_eq!(page_guides(&h), [gy(150.0)]);
    assert_eq!(h.ed.step_count(), steps);
    assert!(h.ed.overlay().active_guide.is_none());
}

#[test]
fn nan_pointer_up_cancels() {
    let mut h = with_rulers();
    h.down(300.0, 10.0, NONE);
    h.mv(300.0, 300.0, NONE);
    let steps = h.ed.step_count();
    h.ev(Input::PointerUp(PointerEvent::new(
        (f64::NAN, 0.0),
        NONE,
        1.0,
    )));
    assert!(!h.ed.is_busy());
    assert!(page_guides(&h).is_empty());
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn click_on_guide_selects_what_is_under_it_and_double_click_still_works() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 200.0, 200.0);
    seed_page(&mut h, vec![gx(200.0)]);
    let steps = h.ed.step_count();
    h.click(200.0, 150.0, NONE);
    assert_eq!(h.sel(), [r], "щелчок прошёл к узлу");
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(page_guides(&h), [gx(200.0)]);
    h.click(200.0, 700.0, NONE);
    assert!(
        h.sel().is_empty(),
        "щелчок по линии на пустом месте снимает выделение"
    );
}

#[test]
fn modifiers_and_tools_keep_nodes_reachable_under_a_guide() {
    let mut h = with_rulers();
    let a = h.rect(None, 100.0, 100.0, 200.0, 200.0);
    let b = h.rect(None, 400.0, 100.0, 200.0, 200.0);
    seed_page(&mut h, vec![gx(500.0)]);
    h.click(150.0, 150.0, NONE);
    // Shift — добавление в выделение, а не линия.
    h.click(
        500.0,
        150.0,
        Modifiers {
            shift: true,
            ..NONE
        },
    );
    assert_eq!(h.sel(), [a, b]);
    // Другой инструмент рисует, а не тянет линию.
    h.ed.select(&[]);
    h.ed.set_tool(ToolKind::Rect);
    let before = h.children(None).len();
    h.drag((500.0, 500.0), (560.0, 560.0), NONE);
    assert_eq!(h.children(None).len(), before + 1);
    assert_eq!(page_guides(&h), [gx(500.0)]);
}

#[test]
fn pressing_inside_a_selected_node_moves_it_not_the_guide() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 200.0, 200.0);
    seed_page(&mut h, vec![gx(200.0)]);
    h.click(150.0, 150.0, NONE);
    assert_eq!(h.sel(), [r]);
    h.drag((200.0, 150.0), (260.0, 150.0), NONE);
    assert_eq!(page_guides(&h), [gx(200.0)], "линия на месте");
    assert_eq!(h.props(r).x, 160.0, "узел сдвинут");
}

#[test]
fn guide_of_a_selected_frame_stays_grabbable() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 400.0, 400.0);
    h.ed.set_guides(GuideOwner::Frame(f), vec![gx(100.0)], Phase::Commit)
        .unwrap();
    h.click(150.0, 150.0, NONE);
    assert_eq!(h.sel(), [f]);
    h.drag((200.0, 300.0), (300.0, 300.0), NONE);
    assert_eq!(frame_guides(&mut h, f), [gx(200.0)]);
    assert_eq!(h.props(f).x, 100.0);
}

#[test]
fn selection_handles_win_over_a_guide() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 200.0, 200.0);
    seed_page(&mut h, vec![gx(300.0)]);
    h.ed.select(&[r]);
    h.hover(300.0, 200.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { angle } if angle == 0.0));
    h.drag((300.0, 200.0), (350.0, 200.0), NONE);
    assert_eq!(h.props(r).w, 250.0, "ручка рамки");
    assert_eq!(page_guides(&h), [gx(300.0)]);
}

#[test]
fn hover_shows_resize_cursor_on_ruler_and_guide_without_node_hover() {
    let mut h = with_rulers();
    let r = h.rect(None, 0.0, 0.0, 500.0, 500.0);
    seed_page(&mut h, vec![gx(200.0)]);
    h.hover(100.0, 10.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { angle } if angle == 90.0));
    assert_eq!(h.ed.overlay().hover, None, "узел под линейкой не наводится");
    h.hover(10.0, 100.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { angle } if angle == 0.0));
    h.hover(10.0, 10.0);
    assert_eq!(h.ed.cursor(), Cursor::Default, "угол");
    h.hover(200.0, 300.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { angle } if angle == 0.0));
    assert_eq!(h.ed.overlay().hover, None);
    h.hover(260.0, 300.0);
    assert_eq!(h.ed.cursor(), Cursor::Default);
    assert_eq!(h.ed.overlay().hover, Some(r));
}

#[test]
fn ruler_click_does_not_reach_tool_or_end_edit() {
    let mut h = with_rulers();
    let r = h.rect(None, 0.0, 0.0, 500.0, 500.0);
    h.ed.select(&[r]);
    h.click(100.0, 10.0, NONE);
    assert_eq!(h.sel(), [r], "щелчок по линейке не снимает выделение");
    h.ed.set_tool(ToolKind::Rect);
    let before = h.children(None).len();
    h.drag((100.0, 10.0), (100.0, 15.0), NONE);
    assert_eq!(
        h.children(None).len(),
        before,
        "рисование под линейкой не начинается"
    );
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
}

#[test]
fn rulers_off_or_read_only_means_no_gesture() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 500.0, 500.0);
    h.click(100.0, 10.0, NONE);
    assert_eq!(h.sel(), [r], "без линеек это обычный холст");
    let mut h = with_rulers();
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    h.down(100.0, 10.0, NONE);
    assert!(!h.ed.is_busy());
    h.mv(100.0, 200.0, NONE);
    h.up(100.0, 200.0, NONE);
    assert!(page_guides(&h).is_empty());
    let mut h = with_rulers();
    h.ed.set_view(None, Some(false));
    h.down(100.0, 10.0, NONE);
    assert!(
        !h.ed.is_busy(),
        "скрытые направляющие из линейки не тянутся"
    );
    h.up(100.0, 10.0, NONE);
}

#[test]
fn hidden_guides_are_not_hit_and_do_not_snap() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    seed_page(&mut h, vec![gx(303.0)]);
    h.ed.set_view(None, Some(false));
    h.hover(303.0, 400.0);
    assert_eq!(h.ed.cursor(), Cursor::Default);
    h.drag((125.0, 125.0), (327.0, 125.0), NONE);
    assert_eq!(h.props(r).x, 302.0, "без привязки: целые пиксели");
}

// ——— Привязка ———

#[test]
fn moving_node_snaps_edge_and_center_to_guides_within_4_px() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    seed_page(&mut h, vec![gx(303.0), gy(407.0)]);
    h.drag((125.0, 125.0), (327.0, 125.0), NONE);
    // Левый край 302 → 303.
    assert_eq!(h.props(r).x, 303.0);
    h.ed.execute(CommandId::Undo);
    // Правый край: 150 + 149 = 299, до линии 4 — на пороге; 5 — уже нет.
    h.drag((125.0, 125.0), (274.0, 125.0), NONE);
    assert_eq!(
        h.props(r).x + h.props(r).w,
        303.0,
        "на самом пороге привязка есть"
    );
    h.ed.execute(CommandId::Undo);
    h.drag((125.0, 125.0), (273.0, 125.0), NONE);
    assert_eq!(h.props(r).x, 248.0, "дальше порога — нет");
    h.ed.execute(CommandId::Undo);
    // По y и центр: центр 125 + 281 = 406 → 407.
    h.drag((125.0, 125.0), (125.0, 406.0), NONE);
    assert_eq!(h.props(r).y + 25.0, 407.0);
}

#[test]
fn snap_threshold_is_in_screen_pixels() {
    let mut h = with_rulers();
    let r = h.rect(None, 250.0, 100.0, 50.0, 50.0);
    seed_page(&mut h, vec![gx(300.0)]);
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_zoom(4.0);
    cam.set_center(Point::new(300.0, 125.0));
    h.ed.set_camera(cam);
    let to_screen = |x: f64| 500.0 + (x - 300.0) * 4.0;
    // Хотим левый край на 298 (на экране 8 px от линии): без привязки.
    h.ed.select(&[r]);
    h.drag((to_screen(275.0), 400.0), (to_screen(323.0), 400.0), NONE);
    assert_eq!(h.props(r).x, 298.0);
    h.ed.execute(CommandId::Undo);
    // На 299.5 (2 px на экране): привязка.
    h.drag((to_screen(275.0), 400.0), (to_screen(324.5), 400.0), NONE);
    assert_eq!(h.props(r).x, 300.0);
}

#[test]
fn snapped_position_shows_a_snap_line() {
    let mut h = with_rulers();
    h.rect(None, 100.0, 100.0, 50.0, 50.0);
    seed_page(&mut h, vec![gx(303.0)]);
    h.down(125.0, 125.0, NONE);
    h.mv(327.0, 125.0, NONE);
    let lines = h.ed.overlay().snap_lines;
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!((lines[0].p0.x, lines[0].p1.x), (303.0, 303.0));
    h.up(327.0, 125.0, NONE);
}

#[test]
fn resizing_and_drawing_snap_to_guides() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    seed_page(&mut h, vec![gx(303.0), gy(407.0)]);
    h.ed.select(&[r]);
    // Правая сторона ближе порога к линии.
    h.drag((200.0, 150.0), (301.0, 150.0), NONE);
    assert_eq!(h.props(r).x + h.props(r).w, 303.0);
    h.ed.select(&[]);
    h.ed.set_tool(ToolKind::Rect);
    h.drag((600.0, 300.0), (701.0, 405.0), NONE);
    let made = *h.children(None).last().unwrap();
    assert_eq!(h.props(made).y + h.props(made).h, 407.0, "рисование");
    h.ed.set_tool(ToolKind::Rect);
    h.drag((302.0, 500.0), (340.0, 540.0), NONE);
    let made = *h.children(None).last().unwrap();
    // Начало фигуры — точка нажатия; привязка — у конца.
    assert_eq!(h.props(made).x, 302.0);
}

#[test]
fn frame_guide_snaps_only_opposite_the_frame_and_not_the_frame_itself() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 200.0, 200.0);
    h.ed.set_guides(GuideOwner::Frame(f), vec![gx(50.0)], Phase::Commit)
        .unwrap();
    // Узел напротив фрейма: линия x = 150 действует.
    let r = h.rect(None, 400.0, 150.0, 20.0, 20.0);
    h.drag((410.0, 160.0), (162.0, 160.0), NONE);
    // Узел перенесён: снимем его с родителя фрейма — важен только x.
    assert_eq!(
        h.props(r).x,
        50.0,
        "левый край на линии (в координатах фрейма)"
    );
    h.ed.execute(CommandId::Undo);
    // Ниже фрейма линии нет: края узла свободны, целые пиксели.
    let low = h.rect(None, 400.0, 500.0, 20.0, 20.0);
    h.drag((410.0, 510.0), (162.0, 510.0), NONE);
    assert_eq!(h.props(low).x, 152.0);
    // Сам фрейм к своей линии не привязывается: она едет вместе с ним.
    h.drag((110.0, 110.0), (118.0, 110.0), NONE);
    assert_eq!(h.props(f).x, 108.0);
}

#[test]
fn guide_itself_snaps_to_node_edges_and_other_guides() {
    let mut h = with_rulers();
    h.rect(None, 100.0, 100.0, 50.0, 50.0);
    seed_page(&mut h, vec![gx(400.0)]);
    h.drag((10.0, 500.0), (152.0, 500.0), NONE);
    assert_eq!(page_guides(&h), [gx(400.0), gx(150.0)], "к краю узла");
    h.drag((10.0, 500.0), (402.0, 500.0), NONE);
    let list = page_guides(&h);
    assert_eq!(
        list.len(),
        2,
        "к другой линии — то же место, повтор отброшен"
    );
}

#[test]
fn guide_without_snapping_lands_on_whole_pixels() {
    let mut h = with_rulers();
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_zoom(4.0);
    cam.set_center(Point::new(100.0, 100.0));
    h.ed.set_camera(cam);
    h.drag((10.0, 300.0), (503.0, 300.0), NONE);
    assert_eq!(page_guides(&h), [gx(101.0)], "100,75 → 101");
}

// ——— Отмена и страницы ———

#[test]
fn undo_across_page_switch_returns_to_the_page() {
    let mut h = with_rulers();
    let first = h.ed.page();
    h.drag((10.0, 300.0), (400.0, 300.0), NONE);
    let second = h.ed.create_page().unwrap();
    assert!(
        h.ed.overlay().guides.is_empty(),
        "на новой странице своих нет"
    );
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.ed.page(), first);
    assert!(h.ed.overlay().guides.is_empty());
    h.ed.execute(CommandId::Redo);
    assert_eq!(h.ed.overlay().guides.len(), 1);
    h.ed.set_page(second).unwrap();
    assert!(h.ed.overlay().guides.is_empty());
    h.ed.set_page(first).unwrap();
    assert_eq!(h.ed.overlay().guides.len(), 1);
}

#[test]
fn guides_copy_with_duplicate_and_paste_of_a_frame() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 200.0, 200.0);
    h.ed.set_guides(GuideOwner::Frame(f), vec![gx(50.0)], Phase::Commit)
        .unwrap();
    h.ed.select(&[f]);
    h.ed.execute(CommandId::Duplicate);
    let copy = h.sel()[0];
    assert_ne!(copy, f);
    assert_eq!(frame_guides(&mut h, copy), [gx(50.0)]);
    assert_eq!(h.ed.overlay().guides.len(), 2, "и видна сразу");
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.ed.overlay().guides.len(), 1);
    // Удаление фрейма убирает линии, отмена возвращает.
    h.ed.select(&[f]);
    h.ed.execute(CommandId::Delete);
    assert!(h.ed.overlay().guides.is_empty());
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.ed.overlay().guides.len(), 1);
    assert_eq!(frame_guides(&mut h, f), [gx(50.0)]);
}

#[test]
fn only_top_level_frames_show_guides() {
    let mut h = with_rulers();
    let outer = h.frame(None, 0.0, 0.0, 600.0, 600.0);
    let f = h.frame(None, 700.0, 100.0, 200.0, 200.0);
    h.ed.set_guides(GuideOwner::Frame(f), vec![gx(50.0)], Phase::Commit)
        .unwrap();
    assert_eq!(h.ed.overlay().guides.len(), 1);
    h.ed.transact(|tx| {
        tx.move_node(f, Some(outer), 0)?;
        Ok(vec![])
    })
    .unwrap();
    assert!(
        h.ed.overlay().guides.is_empty(),
        "вложенный фрейм линий не показывает"
    );
    assert_eq!(frame_guides(&mut h, f), [gx(50.0)], "данные на месте");
}

// ——— `set_guides` ———

#[test]
fn protocol_preview_commit_and_cancel() {
    let mut h = with_rulers();
    let page = h.ed.page();
    let owner = GuideOwner::Page(page);
    let steps = h.ed.step_count();
    h.ed.set_guides(owner, vec![gx(10.0)], Phase::Preview)
        .unwrap();
    assert_eq!(h.ed.overlay().guides.len(), 1, "превью в кадре");
    assert!(page_guides(&h).is_empty(), "в документе пусто");
    h.ed.set_guides(owner, vec![gx(10.0), gy(5.0)], Phase::Preview)
        .unwrap();
    assert_eq!(h.ed.overlay().guides.len(), 2);
    assert_eq!(h.ed.step_count(), steps);
    h.ed.set_guides(owner, vec![], Phase::Cancel).unwrap();
    assert!(h.ed.overlay().guides.is_empty());
    assert_eq!(h.ed.step_count(), steps);
    assert!(!h.ed.document().can_undo());
    h.ed.set_guides(owner, vec![gx(10.0)], Phase::Preview)
        .unwrap();
    h.ed.set_guides(owner, vec![gx(11.0)], Phase::Commit)
        .unwrap();
    assert_eq!(page_guides(&h), [gx(11.0)]);
    assert_eq!(h.ed.step_count(), steps + 1);
    h.ed.execute(CommandId::Undo);
    assert!(page_guides(&h).is_empty());
}

#[test]
fn protocol_refuses_bad_data_without_a_step() {
    let mut h = with_rulers();
    let page = h.ed.page();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let f = h.frame(None, 0.0, 0.0, 100.0, 100.0);
    let nested = h.frame(Some(f), 0.0, 0.0, 10.0, 10.0);
    let steps = h.ed.step_count();
    let many: Vec<Guide> = (0..=MAX_GUIDES).map(|i| gx(i as f64)).collect();
    for (owner, list) in [
        (GuideOwner::Page(page), many),
        (GuideOwner::Page(page), vec![gx(f64::NAN)]),
        (GuideOwner::Page(page), vec![gy(f64::INFINITY)]),
        (GuideOwner::Page(page), vec![gy(1.1e9)]),
        (GuideOwner::Frame(r), vec![gy(1.0)]),
        (GuideOwner::Frame(nested), vec![gy(1.0)]),
    ] {
        for phase in [Phase::Preview, Phase::Commit] {
            assert!(h.ed.set_guides(owner, list.clone(), phase).is_err());
        }
    }
    assert_eq!(h.ed.step_count(), steps);
    assert!(page_guides(&h).is_empty());
    assert!(
        h.ed.overlay().guides.is_empty(),
        "неверное превью не показано"
    );
}

#[test]
fn limit_on_gesture_reports_error_and_leaves_no_step() {
    let mut h = with_rulers();
    let list: Vec<Guide> = (0..MAX_GUIDES)
        .map(|i| gx(i as f64 * 2.0 + 1000.0))
        .collect();
    seed_page(&mut h, list);
    let steps = h.ed.step_count();
    h.ed.take_events();
    h.drag((10.0, 300.0), (400.0, 300.0), NONE);
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(page_guides(&h).len(), MAX_GUIDES);
    let ev = h.ed.take_events();
    assert!(
        ev.iter().any(|e| matches!(e, EditorEvent::Error(_))),
        "{ev:?}"
    );
}

#[test]
fn held_shortcut_does_not_flicker_the_toggle() {
    let mut h = with_rulers();
    let shift = Modifiers {
        shift: true,
        ..NONE
    };
    let mut e = KeyEvent::new("KeyR", shift);
    e.repeat = true;
    h.ev(Input::KeyDown(e));
    assert!(h.ed.settings().rulers, "повтор клавиши не переключает");
}

#[test]
fn remembered_hover_does_not_outlive_tool_view_or_edit_changes() {
    let mut h = with_rulers();
    seed_page(&mut h, vec![gx(200.0)]);
    let r = h.rect(None, 500.0, 500.0, 60.0, 60.0);
    let resize = |h: &H| matches!(h.ed.cursor(), Cursor::Resize { .. });
    // Инструмент сменился, указатель не двигался.
    h.hover(200.0, 300.0);
    assert!(resize(&h));
    h.ed.set_tool(ToolKind::Rect);
    assert!(
        !resize(&h),
        "у «Прямоугольника» своя стрелка, линия не берётся"
    );
    h.ed.set_tool(ToolKind::Move);
    // Показ выключен.
    h.hover(200.0, 300.0);
    assert!(resize(&h));
    h.ed.set_view(None, Some(false));
    assert!(!resize(&h));
    h.ed.set_view(None, Some(true));
    // Линейка: выключена или направляющие скрыты.
    h.hover(300.0, 10.0);
    assert!(resize(&h));
    h.ed.set_view(None, Some(false));
    assert_eq!(h.ed.cursor(), Cursor::Default, "из линейки не тянется");
    h.ed.set_view(None, Some(true));
    assert!(resize(&h));
    h.ed.set_view(Some(false), None);
    assert!(!resize(&h));
    h.ed.set_view(Some(true), None);
    // В правке вектора нажатие на линейку ничего не тянет — и курсор это говорит.
    h.ed.select(&[r]);
    h.ed.execute(CommandId::EditObject);
    h.hover(300.0, 10.0);
    assert_eq!(h.ed.cursor(), Cursor::Default);
    h.down(300.0, 10.0, NONE);
    assert!(!h.ed.is_busy());
    h.up(300.0, 10.0, NONE);
}
````

В `crates/ferrum-wasm/tests/context_menu.rs` заменить:

````rust
    )));
}
````

на:

````rust
    )));
}

#[test]
fn set_guides_preview_commit_and_errors() {
    let mut core = demo(10);
    let page = core.editor().document().pages()[0].0.to_string();
    let owner = GuideOwnerDto::Page { id: page };
    let list = vec![GuideDto {
        axis: GuideAxisDto::X,
        offset: 12.0,
    }];
    let errors = |ev: Vec<CoreEvent>| {
        ev.into_iter()
            .filter(|e| matches!(e, CoreEvent::Error { .. }))
            .count()
    };
    core.take_events();
    core.command(Command::SetGuides {
        owner: owner.clone(),
        guides: list.clone(),
        phase: EditPhase::Preview,
    });
    assert_eq!(errors(core.take_events()), 0);
    assert!(!core.editor().document().can_undo());
    core.command(Command::SetGuides {
        owner: owner.clone(),
        guides: list.clone(),
        phase: EditPhase::Commit,
    });
    let ev = core.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::Modified { modified: true, .. })),
        "{ev:?}"
    );
    assert_eq!(errors(ev), 0);
    assert!(core.editor().document().can_undo());
    for bad in [
        vec![GuideDto {
            axis: GuideAxisDto::Y,
            offset: f64::NAN,
        }],
        vec![GuideDto {
            axis: GuideAxisDto::Y,
            offset: 2e9,
        }],
        vec![
            GuideDto {
                axis: GuideAxisDto::Y,
                offset: 0.0
            };
            1001
        ],
    ] {
        core.command(Command::SetGuides {
            owner: owner.clone(),
            guides: bad,
            phase: EditPhase::Commit,
        });
        assert_eq!(errors(core.take_events()), 1);
    }
    core.command(Command::SetGuides {
        owner: GuideOwnerDto::Page {
            id: "мусор".into()
        },
        guides: list.clone(),
        phase: EditPhase::Commit,
    });
    assert_eq!(errors(core.take_events()), 1);
    core.command(Command::SetGuides {
        owner: GuideOwnerDto::Frame { id: "1@1".into() },
        guides: list,
        phase: EditPhase::Commit,
    });
    assert_eq!(errors(core.take_events()), 1, "такого узла нет");
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test guides`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test context_menu`
Expected: FAIL — `guides` не собирается: нет `Editor::set_guides`; `context_menu` ядра не собирается: нет `Command::SetGuides`, `GuideOwnerDto`, `GuideAxisDto`, `GuideDto`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
    command_states(&mut report);
    select_similar(&mut report);
    find_layers(&mut report);

````

на:

````rust
    command_states(&mut report);
    select_similar(&mut report);
    guides_section(&mut report);
    find_layers(&mut report);

````

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
/// выделен один прямоугольник — совпадают его двойники в каждом фрейме (тот же
/// тип, заливка и обводка: 1000 узлов). Цель — 100 мс на весь обход страницы.
fn select_similar(report: &mut Report) {
    let mut doc = Document::new();
````

на:

````rust
/// выделен один прямоугольник — совпадают его двойники в каждом фрейме (тот же
/// тип, заливка и обводка: 1000 узлов). Цель — 100 мс на весь обход страницы.
/// Направляющие (спека подсистемы 2, раздел 9): 100 000 узлов, линейки и по 1000 линий
/// у страницы и фрейма. Движение указателя считает попадание по линиям, протяжка —
/// привязку к ним; обе не должны зависеть от размера страницы.
fn guides_section(report: &mut Report) {
    use ferrum_doc::{Guide, GuideAxis, GuideOwner};
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids: Vec<NodeId> = doc.transact(page, vec![], |p| fill(p, NODES)).unwrap();
    let mut ed = Editor::new(doc, Some(page), VIEWPORT, 1.0, Platform::Mac).unwrap();
    let mut cam = Camera::new(VIEWPORT, 1.0);
    cam.set_center(Point::new(960.0, 540.0));
    ed.set_camera(cam);
    ed.set_view(Some(true), Some(true));
    let list = |axis| {
        (0..ferrum_doc::MAX_GUIDES)
            .map(|i| Guide::new(axis, 13.0 + i as f64 * 7.0))
            .collect::<Vec<_>>()
    };
    ed.set_guides(
        GuideOwner::Page(page),
        list(GuideAxis::X),
        ferrum_editor::Phase::Commit,
    )
    .unwrap();
    ed.set_guides(
        GuideOwner::Frame(ids[0]),
        list(GuideAxis::Y),
        ferrum_editor::Phase::Commit,
    )
    .unwrap();
    ed.take_events();
    let mut clock = Clock(0.0);
    let hover: Vec<Duration> = (0..300)
        .map(|i| {
            let p = Point::new(300.0 + (i % 200) as f64, 300.0 + (i % 150) as f64);
            let e = clock.at(p);
            timed(&mut ed, Input::PointerMove(e))
        })
        .collect();
    report.gate_median("Линейки и 2000 линий: движение указателя", &hover, 8.0);
    // Протяжка из линейки: привязка и превью на каждый шаг.
    let e = clock.at(Point::new(10.0, 300.0));
    ed.handle(Input::PointerDown(e));
    let moves: Vec<Duration> = (0..300)
        .map(|i| {
            let p = Point::new(30.0 + i as f64 * 2.0, 300.0);
            let e = clock.at(p);
            timed(&mut ed, Input::PointerMove(e))
        })
        .collect();
    report.gate_median("Линейки и 2000 линий: протяжка линии, шаг", &moves, 8.0);
    report.check(
        "Линейки и 2000 линий: протяжка идёт",
        ed.overlay().active_guide.is_some(),
        "да".to_string(),
        "да",
    );
    let t = Instant::now();
    ed.handle(Input::PointerCancel);
    report.info("Линейки и 2000 линий: отмена протяжки", ms(t.elapsed()));
    // Перетаскивание узла: привязка к линиям и пересчёт линий при превью сцены.
    let node = ids[1];
    let at = origin(&ed, node);
    let from = screen(&ed, at.x + 40.0, at.y + 40.0); // ниже линейки
    ed.select(&[node]);
    let e = clock.at(from);
    ed.handle(Input::PointerMove(e));
    let e = clock.at(from);
    ed.handle(Input::PointerDown(e));
    let moves: Vec<Duration> = (0..300)
        .map(|i| {
            let p = Point::new(from.x + i as f64 * 0.7, from.y + i as f64 * 0.3);
            let e = clock.at(p);
            timed(&mut ed, Input::PointerMove(e))
        })
        .collect();
    report.gate_median(
        "Линейки и 2000 линий: перетаскивание узла, шаг",
        &moves,
        8.0,
    );
    report.check(
        "Линейки и 2000 линий: узел сдвинут в превью",
        origin(&ed, node) != at,
        format!("{:?}", origin(&ed, node) - at),
        "не ноль",
    );
    ed.handle(Input::PointerCancel);
}

fn select_similar(report: &mut Report) {
    let mut doc = Document::new();
````

В `crates/ferrum-editor/src/context_menu.rs` заменить:

````rust
        let items = match target {
            ContextTarget::Canvas(p) => {
                let under = self.context_select_at(p);
                self.node_menu(under, false)
            }
````

на:

````rust
        let items = match target {
            ContextTarget::Canvas(p) => {
                // Над линейкой выбора нет: меню — по нынешнему выделению.
                let under = if self.over_ruler(p).is_some() {
                    None
                } else {
                    self.context_select_at(p)
                };
                self.node_menu(under, false)
            }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    guides_shown: bool,
    guides_revision: u64,
    /// Измерения с Alt: кадр нужен, когда они изменились, и не нужен, пока те же.
    measures: Vec<Measure>,
````

на:

````rust
    guides_shown: bool,
    guides_revision: u64,
    /// Тянущаяся направляющая: положение, длина и значение на плашке.
    active_guide: Option<ferrum_render::ActiveGuide>,
    /// Измерения с Alt: кадр нужен, когда они изменились, и не нужен, пока те же.
    measures: Vec<Measure>,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    pub(crate) guides: crate::guides::GuideCache,
    pub(crate) rulers_cache: crate::guides::RulersCache,
}

````

на:

````rust
    pub(crate) guides: crate::guides::GuideCache,
    pub(crate) rulers_cache: crate::guides::RulersCache,
    /// Протяжка направляющей из линейки или за неё саму.
    pub(crate) guide_drag: Option<crate::guides::GuideDrag>,
    /// Превью `SetGuides`: список владельца вместо его направляющих, пока жест идёт.
    pub(crate) guides_preview: Option<(ferrum_doc::GuideOwner, Vec<ferrum_doc::Guide>)>,
    /// Что под указателем без жеста: линейка или направляющая — курсор и нажатие.
    pub(crate) guide_hover: Option<crate::guides::GuideHover>,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                guides_shown: false,
                guides_revision: 0,
                measures: Vec::new(),
                text: None,
````

на:

````rust
                guides_shown: false,
                guides_revision: 0,
                active_guide: None,
                measures: Vec::new(),
                text: None,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            guides: Default::default(),
            rulers_cache: Default::default(),
        })
    }
````

на:

````rust
            guides: Default::default(),
            rulers_cache: Default::default(),
            guide_drag: None,
            guides_preview: None,
            guide_hover: None,
        })
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.tool.is_busy()
            || self.pan.is_some()
            || self.vector_edit.as_ref().is_some_and(|v| v.drag.is_some())
    }
````

на:

````rust
        self.tool.is_busy()
            || self.pan.is_some()
            || self.guide_drag.is_some()
            || self.vector_edit.as_ref().is_some_and(|v| v.drag.is_some())
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        if self.pan.is_some() {
            Cursor::Grabbing
        } else if self.text_edit.is_some() {
            // Рамка с ручками во время правки скрыта: ни Resize, ни Rotate.
````

на:

````rust
        if self.pan.is_some() {
            Cursor::Grabbing
        } else if let Some(c) = self.guide_cursor() {
            c
        } else if self.text_edit.is_some() {
            // Рамка с ручками во время правки скрыта: ни Resize, ни Rotate.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            t.snap_lines = v.snap_lines.clone();
        }
        let editing = text.is_some() || self.vector_edit.is_some();
        Overlay {
````

на:

````rust
            t.snap_lines = v.snap_lines.clone();
        }
        if let Some(d) = &self.guide_drag {
            t.snap_lines = d.lines.clone();
        }
        let editing = text.is_some() || self.vector_edit.is_some();
        Overlay {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            rulers: self.rulers_overlay(),
            guides: self.guide_marks(),
            active_guide: None,
        }
    }
````

на:

````rust
            rulers: self.rulers_overlay(),
            guides: self.guide_marks(),
            active_guide: self.active_guide(),
        }
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            tool,
            fonts,
            ..
        } = self;
````

на:

````rust
            tool,
            fonts,
            guides,
            ..
        } = self;
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                out,
                fonts,
            },
        )
````

на:

````rust
                out,
                fonts,
                guides,
            },
        )
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                    return;
                }
                // Правка текста: нажатие в тексте ставит курсор; мимо — правка
                // кончается, и нажатие достаётся инструменту.
````

на:

````rust
                    return;
                }
                // Линейка и направляющая — раньше инструмента и правки.
                if self.guide_press(&e) {
                    return;
                }
                // Правка текста: нажатие в тексте ставит курсор; мимо — правка
                // кончается, и нажатие достаётся инструменту.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                    self.cancel_gesture();
                }
                let (tool, mut cx) = self.split();
                tool.pointer_down(&mut cx, &e);
                // Двойной щелчок по тексту (или нажатие инструментом «Текст»):
                // инструмент просит войти в правку.
                if let Some((id, world)) = self.out.edit_text.take() {
                    if self.tool.is_busy() {
                        let (tool, mut cx) = self.split();
                        tool.cancel(&mut cx);
                    }
                    // Инструмент «Текст» своё сделал — после правки снова «Перемещение».
                    if self.tool.kind() == ToolKind::Text {
                        self.set_tool(ToolKind::Move);
                        self.selection.set(vec![id]);
                    }
                    self.start_text_edit(id, EnterAt::Point(world));
                }
                // Двойной щелчок по вектору или фигуре — правка вектора.
                if let Some(id) = self.out.edit_vector.take() {
                    if self.tool.is_busy() {
                        let (tool, mut cx) = self.split();
                        tool.cancel(&mut cx);
                    }
                    self.start_vector_edit(id);
                }
            }
        }
    }
````

на:

````rust
                    self.cancel_gesture();
                }
                self.press_tool(&e);
            }
        }
    }

    /// Нажатие достаётся инструменту.
    pub(crate) fn press_tool(&mut self, e: &PointerEvent) {
        let (tool, mut cx) = self.split();
        tool.pointer_down(&mut cx, e);
        // Двойной щелчок по тексту (или нажатие инструментом «Текст»):
        // инструмент просит войти в правку.
        if let Some((id, world)) = self.out.edit_text.take() {
            if self.tool.is_busy() {
                let (tool, mut cx) = self.split();
                tool.cancel(&mut cx);
            }
            // Инструмент «Текст» своё сделал — после правки снова «Перемещение».
            if self.tool.kind() == ToolKind::Text {
                self.set_tool(ToolKind::Move);
                self.selection.set(vec![id]);
            }
            self.start_text_edit(id, EnterAt::Point(world));
        }
        // Двойной щелчок по вектору или фигуре — правка вектора.
        if let Some(id) = self.out.edit_vector.take() {
            if self.tool.is_busy() {
                let (tool, mut cx) = self.split();
                tool.cancel(&mut cx);
            }
            self.start_vector_edit(id);
        }
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            hand.pointer_move(&mut self.ctx(), &e);
            self.pan = Some(hand);
            return;
        }
````

на:

````rust
            hand.pointer_move(&mut self.ctx(), &e);
            self.pan = Some(hand);
            return;
        }
        if self.guide_drag.is_some() {
            self.guide_drag_move(&e);
            return;
        }
        // Над линейкой инструмента нет: ни наведения, ни его курсора.
        if self.ruler_hover_update(&e) {
            return;
        }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        }
        let (tool, mut cx) = self.split();
        tool.pointer_move(&mut cx, &e);
    }

    fn pointer_up(&mut self, e: PointerEvent) {
````

на:

````rust
        }
        let (tool, mut cx) = self.split();
        tool.pointer_move(&mut cx, &e);
        self.guide_hover_update(&e);
    }

    fn pointer_up(&mut self, e: PointerEvent) {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            if let Some(mut hand) = self.pan.take() {
                hand.pointer_up(&mut self.ctx(), &e);
            }
            return;
````

на:

````rust
            if let Some(mut hand) = self.pan.take() {
                hand.pointer_up(&mut self.ctx(), &e);
            }
            return;
        }
        if self.guide_drag.is_some() {
            if e.button == PointerButton::Primary {
                self.guide_drag_up(&e);
            }
            return;
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            return;
        }
        let (tool, mut cx) = self.split();
        if tool.pointer_up(&mut cx, &e) == Flow::Finished {
````

на:

````rust
            return;
        }
        self.release_tool(&e);
    }

    /// Отпускание достаётся инструменту.
    pub(crate) fn release_tool(&mut self, e: &PointerEvent) {
        let e = *e;
        let (tool, mut cx) = self.split();
        if tool.pointer_up(&mut cx, &e) == Flow::Finished {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// местом мира, жест пересчитывается.
    fn refresh_gesture(&mut self) {
        if let (true, Some(e)) = (self.tool.is_busy(), self.last_pointer) {
            let (tool, mut cx) = self.split();
````

на:

````rust
    /// местом мира, жест пересчитывается.
    fn refresh_gesture(&mut self) {
        if let (true, Some(e)) = (self.guide_drag.is_some(), self.last_pointer) {
            self.guide_drag_move(&e);
        }
        if let (true, Some(e)) = (self.tool.is_busy(), self.last_pointer) {
            let (tool, mut cx) = self.split();
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        }
        self.last_pointer = Some(e);
        let (tool, mut cx) = self.split();
        tool.pointer_move(&mut cx, &e);
    }

````

на:

````rust
        }
        self.last_pointer = Some(e);
        if self.ruler_hover_update(&e) {
            return;
        }
        let (tool, mut cx) = self.split();
        tool.pointer_move(&mut cx, &e);
        self.guide_hover_update(&e);
    }

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            return;
        };
        // Удержанная клавиша не входит в правку (как физический Enter выше).
        if cmd == CommandId::EditObject && e.repeat {
            return;
        }
````

на:

````rust
            return;
        };
        // Удержанная клавиша не входит в правку (как физический Enter выше) и не
        // мигает переключателем (линейки, направляющие, сетка).
        if (cmd == CommandId::EditObject || cmd.info().toggle) && e.repeat {
            return;
        }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    pub fn cancel_gesture(&mut self) {
        self.cancel_panel();
        self.cancel_vector_drag();
        if let Some(mut hand) = self.pan.take() {
````

на:

````rust
    pub fn cancel_gesture(&mut self) {
        self.cancel_panel();
        self.cancel_guides();
        self.cancel_vector_drag();
        if let Some(mut hand) = self.pan.take() {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                guides_shown: self.guides_visible(),
                guides_revision: self.guides.revision,
                measures: self.measures.clone(),
                text: self.text_overlay().map(|o| o.overlay),
````

на:

````rust
                guides_shown: self.guides_visible(),
                guides_revision: self.guides.revision,
                active_guide: self.active_guide(),
                measures: self.measures.clone(),
                text: self.text_overlay().map(|o| o.overlay),
````

В `crates/ferrum-editor/src/guides.rs` заменить:

````rust
//! Как в Figma, направляющие видны, пока включены линейки: «Вид → Направляющие»
//! дополнительно прячет их при включённых линейках.

use std::cell::RefCell;
use std::sync::Arc;

use ferrum_doc::{Guide, GuideAxis, GuideOwner, NodeKind};
use ferrum_render::{GuideMark, RulersOverlay};
use ferrum_scene::{Scene, Slot};
use kurbo::{Line, Point, Rect};

use crate::editor::Editor;
use crate::geometry::world_box;

/// Направляющая на холсте.
````

на:

````rust
//! Как в Figma, направляющие видны, пока включены линейки: «Вид → Направляющие»
//! дополнительно прячет их при включённых линейках.
//!
//! Жест: нажатие на линейке или на линии ведёт `GuideDrag`, пока кнопка нажата живёт
//! только превью, отпускание пишет один шаг (`Document::set_guides`), Esc и потеря
//! фокуса — отмена без следов (правило всех жестов, `tools/mod.rs`).

use std::cell::RefCell;
use std::sync::Arc;

use ferrum_doc::{dedup_guides, DocError, DocResult, Guide, GuideAxis, GuideOwner, NodeKind};
use ferrum_render::{ActiveGuide, GuideMark, RulersOverlay, RULER_SIZE};
use ferrum_scene::{Scene, Slot};
use kurbo::{Line, Point, Rect};

use crate::editor::{Editor, EditorEvent};
use crate::geometry::world_box;
use crate::input::{PointerButton, PointerEvent};
use crate::panel::Phase;
use crate::snap::{Snapper, SNAP_THRESHOLD_PX};
use crate::tools::{sync, Cursor, ToolKind, DRAG_THRESHOLD_PX};

/// Допуск попадания по направляющей — экранные пиксели. Меньше допуска узлов (4):
/// узкий узел на линии остаётся достижимым.
pub const GUIDE_HIT_PX: f64 = 3.0;
/// Сколько выделенных узлов проверяется на «нажатие в выделенном».
const MAX_LEAF_CHECK: usize = 256;

/// Направляющая на холсте.
````

В `crates/ferrum-editor/src/guides.rs` заменить:

````rust
    owners: Vec<Slot>,
    owners_key: Option<(u64, u64)>,
}

````

на:

````rust
    owners: Vec<Slot>,
    owners_key: Option<(u64, u64)>,
    /// Разобранный список страницы: его ключ меняется реже сцены (перетаскивание узла
    /// двигает сцену каждый кадр, а разбор 1000 значений Loro того не стоит).
    page_list: Vec<Guide>,
    page_key: Option<(ferrum_doc::PageId, u64)>,
}

/// Что под указателем без жеста.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum GuideHover {
    /// Линейка, из которой тянется направляющая этой оси.
    Ruler(GuideAxis),
    /// Угол между линейками: ничего не делает.
    Corner,
    /// Направляющая.
    Guide(PlacedGuide),
}

/// Протяжка направляющей. Новая (из линейки) и существующая — одно и то же; у
/// существующей короткое нажатие без сдвига достаётся инструменту как щелчок: линия
/// не мешает выбрать узел под ней.
#[derive(Debug, Clone)]
pub(crate) struct GuideDrag {
    axis: GuideAxis,
    /// Направляющая, которую тянут; `None` — новая.
    source: Option<PlacedGuide>,
    press: PointerEvent,
    /// Указатель отошёл от точки нажатия дальше порога.
    started: bool,
    /// Текущее положение в мире (с привязкой).
    at: f64,
    /// Фрейм верхнего уровня под указателем: будущий владелец и длина линии.
    frame: Option<Slot>,
    /// Указатель на своей линейке: отпускание удалит направляющую.
    over_ruler: bool,
    pub lines: Vec<Line>,
}

````

В `crates/ferrum-editor/src/guides.rs` заменить:

````rust
            return;
        }
        let owners_key = (self.scene.generation(), self.scene.structure_stamp());
        if self.guides.owners_key != Some(owners_key) {
````

на:

````rust
            return;
        }
        // Линии могли измениться: взятая прежде за наведённую — уже не та.
        if matches!(self.guide_hover, Some(GuideHover::Guide(_))) {
            self.guide_hover = None;
        }
        let owners_key = (self.scene.generation(), self.scene.structure_stamp());
        if self.guides.owners_key != Some(owners_key) {
````

В `crates/ferrum-editor/src/guides.rs` заменить:

````rust
        }
        let mut lines = Vec::new();
        for g in self.doc.page_guides(self.page) {
            lines.push(PlacedGuide {
                owner: GuideOwner::Page(self.page),
````

на:

````rust
        }
        let mut lines = Vec::new();
        let preview = self.guides_preview.as_ref();
        let page_key = (self.page, self.doc.guides_revision());
        if self.guides.page_key != Some(page_key) {
            self.guides.page_list = self.doc.page_guides(self.page);
            self.guides.page_key = Some(page_key);
        }
        let of_page: &[Guide] = match preview {
            Some((GuideOwner::Page(p), list)) if *p == self.page => list,
            _ => &self.guides.page_list,
        };
        for &g in of_page {
            lines.push(PlacedGuide {
                owner: GuideOwner::Page(self.page),
````

В `crates/ferrum-editor/src/guides.rs` заменить:

````rust
            });
        }
        for &slot in &self.guides.owners {
            let Some(n) = self.scene.node_at(slot) else {
                continue;
            };
            let Some(list) = n.props.guides.as_ref() else {
                continue;
            };
            if !self.scene.is_visible(slot) {
````

на:

````rust
            });
        }
        // Превью фрейма без направляющих (первая у него) тоже рисуется.
        let previewed = preview.and_then(|(o, list)| match o {
            GuideOwner::Frame(id) => self.scene.slot(*id).map(|s| (s, list.as_slice())),
            GuideOwner::Page(_) => None,
        });
        let slots = self.guides.owners.iter().copied().chain(
            previewed
                .map(|(s, _)| s)
                .filter(|s| !self.guides.owners.contains(s)),
        );
        for slot in slots {
            let Some(n) = self.scene.node_at(slot) else {
                continue;
            };
            let list: &[Guide] = match previewed {
                Some((s, list)) if s == slot => list,
                _ => n.props.guides.as_deref().map_or(&[], |g| g.as_slice()),
            };
            if !self.scene.is_visible(slot) {
````

В `crates/ferrum-editor/src/guides.rs` заменить:

````rust
            }
            let rotated = !axis_aligned(&self.scene, slot);
            for g in list.iter() {
                lines.push(PlacedGuide {
                    owner: GuideOwner::Frame(n.id),
````

на:

````rust
            }
            let rotated = !axis_aligned(&self.scene, slot);
            for g in list {
                lines.push(PlacedGuide {
                    owner: GuideOwner::Frame(n.id),
````

В `crates/ferrum-editor/src/guides.rs` заменить:

````rust
            }
        }
        // Пустой список остаётся пустым без новой ревизии, если и был пустым.
        if !(lines.is_empty() && self.guides.lines.is_empty()) {
            self.guides.lines = Arc::new(lines);
            self.guides.revision += 1;
````

на:

````rust
            }
        }
        // Те же линии — та же ревизия: перетаскивание узла не перерисовывает их заново.
        if *self.guides.lines != lines {
            self.guides.lines = Arc::new(lines);
            self.guides.revision += 1;
````

В `crates/ferrum-editor/src/guides.rs` заменить:

````rust
        self.guides.lines.iter().map(|g| g.mark).collect()
    }
}
````

на:

````rust
        self.guides.lines.iter().map(|g| g.mark).collect()
    }

    /// Какая линейка под экранной точкой.
    pub(crate) fn over_ruler(&self, p: Point) -> Option<GuideHover> {
        if !self.rulers_visible() {
            return None;
        }
        match (p.x < RULER_SIZE, p.y < RULER_SIZE) {
            (true, true) => Some(GuideHover::Corner),
            // Сверху — горизонтальная, смещение по y; слева — вертикальная.
            (false, true) => Some(GuideHover::Ruler(GuideAxis::Y)),
            (true, false) => Some(GuideHover::Ruler(GuideAxis::X)),
            (false, false) => None,
        }
    }

    /// Ближайшая направляющая под экранной точкой. Косые линии не ловятся.
    fn guide_at(&self, p: Point) -> Option<PlacedGuide> {
        if !self.guides_visible() || self.over_ruler(p).is_some() {
            return None;
        }
        let world = self.camera.screen_to_world(p);
        let tol = GUIDE_HIT_PX / self.camera.zoom();
        let mut best: Option<(f64, PlacedGuide)> = None;
        for g in self.guides.lines.iter().filter(|g| !g.rotated) {
            let line = g.mark.line;
            let (d, cross, a, b) = match g.guide.axis {
                GuideAxis::X => ((world.x - line.p0.x).abs(), world.y, line.p0.y, line.p1.y),
                GuideAxis::Y => ((world.y - line.p0.y).abs(), world.x, line.p0.x, line.p1.x),
            };
            if d > tol || (!g.mark.infinite && (cross < a.min(b) || cross > a.max(b))) {
                continue;
            }
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, *g));
            }
        }
        best.map(|(_, g)| g)
    }

    /// Рамка выделенных узлов, кроме фреймов: нажатие в ней двигает их, а не линию
    /// (у выделенного фрейма его направляющие остаются доступными).
    fn pressing_selected_leaf(&self, world: Point) -> bool {
        self.selection.iter().any(|id| {
            self.scene
                .slot(*id)
                .filter(|s| self.scene.at(*s).props.kind != NodeKind::Frame)
                .and_then(|s| world_box(&self.scene, s))
                .is_some_and(|b| b.contains(world))
        })
    }

    /// Можно ли взяться за направляющую в этой точке, а не отдать её инструменту:
    /// только «Перемещение» без модификаторов, без правок, не над ручками рамки.
    fn grabbable_guide(&self, e: &PointerEvent) -> Option<PlacedGuide> {
        let m = e.modifiers;
        if self.tool.kind() != ToolKind::Move
            || self.doc.is_read_only()
            || self.text_edit.is_some()
            || self.vector_edit.is_some()
            || self.is_busy()
            || m.shift
            || m.alt
            || m.ctrl
            || m.meta
            || matches!(
                self.tool.cursor(),
                Cursor::Resize { .. } | Cursor::Rotate { .. }
            )
        {
            return None;
        }
        let g = self.guide_at(e.point)?;
        // Обход выделения на каждое движение: при тысячах выделенных он не окупается,
        // и такое выделение просто не защищает нажатие от линии.
        if self.selection.len() <= MAX_LEAF_CHECK
            && self.pressing_selected_leaf(self.camera.screen_to_world(e.point))
        {
            return None;
        }
        Some(g)
    }

    /// Указатель над линейкой без жеста: инструмент его не видит, наведения нет.
    /// `true` — событие обработано.
    pub(crate) fn ruler_hover_update(&mut self, e: &PointerEvent) -> bool {
        let busy = self.is_busy() || self.text_edit.as_ref().is_some_and(|t| t.drag.is_some());
        let over = if busy { None } else { self.over_ruler(e.point) };
        match over {
            Some(h) => {
                self.guide_hover = Some(h);
                self.hover = None;
                self.text_hover = false;
                true
            }
            None => {
                self.guide_hover = None;
                false
            }
        }
    }

    /// Указатель над направляющей: наведения на узел под ней нет, курсор — протяжки.
    pub(crate) fn guide_hover_update(&mut self, e: &PointerEvent) {
        if let Some(g) = self.grabbable_guide(e) {
            self.guide_hover = Some(GuideHover::Guide(g));
            self.hover = None;
        }
    }

    pub(crate) fn guide_cursor(&self) -> Option<Cursor> {
        let resize = |axis| Cursor::Resize {
            angle: if axis == GuideAxis::X { 0.0 } else { 90.0 },
        };
        if let Some(d) = &self.guide_drag {
            return Some(resize(d.axis));
        }
        // Наведение запомнено при прошлом движении: инструмент, показ или правка с тех
        // пор могли смениться, а указатель ещё не двигался.
        let editing = self.text_edit.is_some() || self.vector_edit.is_some();
        match self.guide_hover? {
            GuideHover::Ruler(_) | GuideHover::Corner if !self.rulers_visible() => None,
            GuideHover::Ruler(axis) => {
                let usable = !editing && !self.doc.is_read_only() && self.guides_visible();
                Some(if usable {
                    resize(axis)
                } else {
                    Cursor::Default
                })
            }
            GuideHover::Corner => Some(Cursor::Default),
            GuideHover::Guide(g) => {
                (self.tool.kind() == ToolKind::Move && !editing && self.guides_visible())
                    .then(|| resize(g.guide.axis))
            }
        }
    }

    /// Нажатие левой кнопкой: `true` — его забрала линейка или направляющая. Нажатие
    /// на линейку инструменту не достаётся никогда; правка текста и вектора от него
    /// не кончается.
    pub(crate) fn guide_press(&mut self, e: &PointerEvent) -> bool {
        // Прежнее нажатие без отпускания (событие потеряно): жест отменяется.
        self.cancel_guides();
        if let Some(over) = self.over_ruler(e.point) {
            let editing = self.text_edit.is_some() || self.vector_edit.is_some();
            if let GuideHover::Ruler(axis) = over {
                if !editing && !self.doc.is_read_only() && self.guides_visible() {
                    if self.tool.is_busy() {
                        self.cancel_gesture();
                    }
                    self.guide_drag = Some(GuideDrag {
                        axis,
                        source: None,
                        press: *e,
                        started: false,
                        at: 0.0,
                        frame: None,
                        over_ruler: true,
                        lines: Vec::new(),
                    });
                }
            }
            return true;
        }
        let Some(g) = self.grabbable_guide(e) else {
            return false;
        };
        self.guide_drag = Some(GuideDrag {
            axis: g.guide.axis,
            source: Some(g),
            press: *e,
            started: false,
            at: 0.0,
            frame: None,
            over_ruler: false,
            lines: Vec::new(),
        });
        true
    }

    /// Фрейм верхнего уровня под точкой мира: видимый, не заблокированный, без
    /// поворота. Он и станет владельцем направляющей.
    fn top_frame_under(&self, world: Point) -> Option<Slot> {
        // Не `hit_test_all`: у фрейма без заливки попадание только по краю и детям.
        self.scene
            .query_rect(Rect::from_points(world, world))
            .into_iter()
            .filter(|id| !self.scene.is_draft(*id))
            .filter_map(|id| self.scene.slot(id))
            .map(|s| root_of(&self.scene, s))
            .filter(|&s| {
                let n = self.scene.at(s);
                n.props.kind == NodeKind::Frame
                    && self.scene.is_visible(s)
                    && !n.props.locked
                    && axis_aligned(&self.scene, s)
                    && world_box(&self.scene, s).is_some_and(|b| b.contains(world))
            })
            .max_by_key(|&s| self.scene.order_key(s))
    }

    pub(crate) fn guide_drag_move(&mut self, e: &PointerEvent) {
        let Some(mut d) = self.guide_drag.take() else {
            return;
        };
        if !d.started && (e.point - d.press.point).hypot() >= DRAG_THRESHOLD_PX {
            d.started = true;
            // Прежняя линия — в превью, пока жест идёт.
            if let Some(g) = d.source {
                let rest = self.owner_guides_without(g);
                self.guides_preview = Some((g.owner, rest));
                self.guides.key = None;
            }
        }
        if d.started {
            let world = self.camera.screen_to_world(e.point);
            d.frame = self.top_frame_under(world);
            d.over_ruler = match d.axis {
                GuideAxis::X => e.point.x < RULER_SIZE,
                GuideAxis::Y => e.point.y < RULER_SIZE,
            };
            let raw = match d.axis {
                GuideAxis::X => world.x,
                GuideAxis::Y => world.y,
            };
            d.lines.clear();
            let mut snapped = None;
            if self.settings.snapping && !d.over_ruler {
                let parent = d.frame.map(|s| self.scene.at(s).id);
                let snapper = Snapper::for_parent(&self.scene, parent, &[])
                    .with_guides(self.guides.lines.clone())
                    .skipping_guide(d.source);
                let threshold = SNAP_THRESHOLD_PX / self.camera.zoom();
                let moving = Rect::from_points(world, world);
                let view = self.camera.visible_world_rect();
                let s = match d.axis {
                    GuideAxis::X => snapper.snap(&self.scene, &[raw], &[], moving, threshold, view),
                    GuideAxis::Y => snapper.snap(&self.scene, &[], &[raw], moving, threshold, view),
                };
                snapped = match d.axis {
                    GuideAxis::X => s.dx,
                    GuideAxis::Y => s.dy,
                };
                d.lines = s.lines;
            }
            d.at = match snapped {
                Some(delta) => raw + delta,
                None if self.settings.pixel_snap => raw.round(),
                None => raw,
            };
        }
        self.guide_drag = Some(d);
    }

    fn owner_guides_without(&self, g: PlacedGuide) -> Vec<Guide> {
        self.owner_guides(g.owner)
            .into_iter()
            .filter(|x| *x != g.guide)
            .collect()
    }

    /// Список владельца из документа и сцены.
    fn owner_guides(&self, owner: GuideOwner) -> Vec<Guide> {
        match owner {
            GuideOwner::Page(p) => self.doc.page_guides(p),
            GuideOwner::Frame(id) => self
                .scene
                .node(id)
                .and_then(|n| n.props.guides.as_ref())
                .map_or_else(Vec::new, |g| g.to_vec()),
        }
    }

    pub(crate) fn guide_drag_up(&mut self, e: &PointerEvent) {
        self.guide_drag_move(e);
        let Some(d) = self.guide_drag.take() else {
            return;
        };
        self.guides_preview = None;
        self.guides.key = None;
        if !d.started {
            // Щелчок по линии — как щелчок по тому, что под ней. Щелчок по линейке — ничего.
            if d.source.is_some() {
                self.press_tool(&d.press);
                let mut up = *e;
                up.button = PointerButton::Primary;
                if self.tool.is_busy() {
                    self.release_tool(&up);
                }
            }
            return;
        }
        if d.over_ruler {
            if let Some(g) = d.source {
                let rest = self.owner_guides_without(g);
                self.write_guides(&[(g.owner, rest)]);
            }
            return;
        }
        let (owner, offset) = match d.frame {
            Some(slot) => {
                let n = self.scene.at(slot);
                let origin = world_box(&self.scene, slot).map_or(Point::ZERO, |b| b.origin());
                let base = match d.axis {
                    GuideAxis::X => origin.x,
                    GuideAxis::Y => origin.y,
                };
                (GuideOwner::Frame(n.id), d.at - base)
            }
            None => (GuideOwner::Page(self.page), d.at),
        };
        let guide = Guide::new(d.axis, offset);
        if !guide.is_valid() {
            return;
        }
        let edits = match d.source {
            Some(g) if g.owner == owner => {
                let mut list = self.owner_guides(owner);
                if let Some(slot) = list.iter_mut().find(|x| **x == g.guide) {
                    *slot = guide;
                }
                vec![(owner, list)]
            }
            Some(g) => {
                let rest = self.owner_guides_without(g);
                let mut list = self.owner_guides(owner);
                list.push(guide);
                vec![(g.owner, rest), (owner, list)]
            }
            None => {
                let mut list = self.owner_guides(owner);
                list.push(guide);
                vec![(owner, list)]
            }
        };
        self.write_guides(&edits);
    }

    /// Запись одним шагом; отказ (предел) — сообщение, шага нет.
    fn write_guides(&mut self, edits: &[(GuideOwner, Vec<Guide>)]) {
        if let Err(e) = self.commit_guides(self.page, self.selection.to_vec(), edits) {
            self.out.events.push(EditorEvent::Error(e.to_string()));
        }
    }

    fn commit_guides(
        &mut self,
        page: ferrum_doc::PageId,
        selection: Vec<ferrum_doc::NodeId>,
        edits: &[(GuideOwner, Vec<Guide>)],
    ) -> DocResult<()> {
        let before = self.doc.last_step();
        let result = self.doc.set_guides(page, selection, edits);
        let synced = sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        if self.doc.last_step() != before {
            self.out.steps += 1;
            if !synced {
                self.out.events.push(EditorEvent::DocumentChanged);
            }
        }
        result
    }

    /// `SetGuides` из протокола (спека подсистемы 2, раздел 11): превью — без
    /// шага, запись — один шаг, отмена — назад к документу. Фрейм — только верхнего
    /// уровня на текущей странице; сверх предела и неверные смещения — отказ. Страница
    /// может быть не загружена: сначала загрузка, затем проверка «только для чтения».
    pub fn set_guides(
        &mut self,
        owner: GuideOwner,
        guides: Vec<Guide>,
        phase: Phase,
    ) -> DocResult<()> {
        if phase == Phase::Cancel {
            self.cancel_guides();
            self.emit();
            return Ok(());
        }
        ferrum_doc::check_guides(&guides)?;
        let page = match owner {
            GuideOwner::Page(p) => p,
            GuideOwner::Frame(id) => {
                let slot = self
                    .scene
                    .slot(id)
                    .ok_or_else(|| DocError::NodeNotFound(id.to_string()))?;
                let n = self.scene.at(slot);
                if n.props.kind != NodeKind::Frame || n.parent.is_some() {
                    return Err(DocError::InvalidGuides(
                        "они бывают у страницы и фрейма верхнего уровня".into(),
                    ));
                }
                self.page
            }
        };
        self.doc.page(page)?;
        if self.doc.is_read_only() {
            return Err(DocError::ReadOnly);
        }
        match phase {
            Phase::Preview => {
                if page != self.page {
                    return Err(DocError::InvalidGuides(
                        "превью только на текущей странице".into(),
                    ));
                }
                // Жест протокола не живёт вместе с жестом холста.
                if self.guide_drag.is_some() || self.tool.is_busy() {
                    self.cancel_gesture();
                }
                self.guides_preview = Some((owner, dedup_guides(guides)));
                self.guides.key = None;
            }
            Phase::Commit => {
                self.begin_doc_edit();
                self.guides_preview = None;
                self.guides.key = None;
                let selection = if page == self.page {
                    self.selection.to_vec()
                } else {
                    Vec::new()
                };
                let result = self.commit_guides(page, selection, &[(owner, guides)]);
                self.emit();
                return result;
            }
            Phase::Cancel => unreachable!("разобрано выше"),
        }
        self.emit();
        Ok(())
    }

    /// Отмена жеста: превью и протяжки больше нет, документ не менялся.
    pub(crate) fn cancel_guides(&mut self) {
        let dragged = self.guide_drag.take().is_some();
        let previewed = self.guides_preview.take().is_some();
        if dragged || previewed {
            self.guides.key = None;
        }
    }

    /// Линия, которую тянут: пока указатель не на своей линейке.
    pub(crate) fn active_guide(&self) -> Option<ActiveGuide> {
        let d = self.guide_drag.as_ref()?;
        if !d.started || d.over_ruler {
            return None;
        }
        let guide = Guide::new(d.axis, d.at);
        let origin = self.rulers_overlay().map_or(Point::ZERO, |r| r.origin);
        let value = d.at
            - match d.axis {
                GuideAxis::X => origin.x,
                GuideAxis::Y => origin.y,
            };
        let box_ = d.frame.and_then(|s| {
            self.scene
                .node_at(s)
                .and_then(|_| world_box(&self.scene, s))
        });
        let mark = match box_ {
            Some(b) => GuideMark {
                line: match d.axis {
                    GuideAxis::X => Line::new((d.at, b.y0), (d.at, b.y1)),
                    GuideAxis::Y => Line::new((b.x0, d.at), (b.x1, d.at)),
                },
                infinite: false,
            },
            None => GuideMark {
                line: page_line(guide),
                infinite: true,
            },
        };
        Some(ActiveGuide { mark, value })
    }
}
````

В `crates/ferrum-editor/src/snap.rs` заменить:

````rust
//! полосы находят ровно все привязки в видимой части, а работа зависит от числа
//! узлов рядом, а не от размера страницы.

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};
use kurbo::{Line, Rect};

use crate::geometry::world_box;

/// Порог привязки в экранных пикселях.
````

на:

````rust
//! полосы находят ровно все привязки в видимой части, а работа зависит от числа
//! узлов рядом, а не от размера страницы.
//!
//! Направляющие — такие же цели, как края узлов (спека подсистемы 2, раздел 9): порог
//! тот же, сдвиг считает тот же `best_offset`. В полосах R-tree их нет: они перебираются
//! в списке линий страницы, отдельно по каждой оси.

use std::collections::HashSet;
use std::sync::Arc;

use ferrum_doc::{GuideAxis, GuideOwner, NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};
use kurbo::{Line, Rect};

use crate::geometry::world_box;
use crate::guides::PlacedGuide;

/// Порог привязки в экранных пикселях.
````

В `crates/ferrum-editor/src/snap.rs` заменить:

````rust
    /// привязываемся. Порядок нужен для детерминированного выбора при равных сдвигах.
    frames: Vec<(Rect, Vec<u32>)>,
}

````

на:

````rust
    /// привязываемся. Порядок нужен для детерминированного выбора при равных сдвигах.
    frames: Vec<(Rect, Vec<u32>)>,
    /// Линии направляющих на начало жеста; видимость уже учтена (`Ctx::snap_guides`).
    guides: Arc<Vec<PlacedGuide>>,
    /// Направляющая, которую тянут: к себе не привязывается.
    skip_guide: Option<PlacedGuide>,
}

````

В `crates/ferrum-editor/src/snap.rs` заменить:

````rust
            parents,
            frames,
        }
    }

````

на:

````rust
            parents,
            frames,
            guides: Arc::default(),
            skip_guide: None,
        }
    }

    /// Привязка и к линиям `guides`, кроме линий фреймов из самого жеста.
    pub(crate) fn with_guides(mut self, guides: Arc<Vec<PlacedGuide>>) -> Snapper {
        self.guides = guides;
        self
    }

    pub(crate) fn skipping_guide(mut self, guide: Option<PlacedGuide>) -> Snapper {
        self.skip_guide = guide;
        self
    }

    /// Цели направляющих по оси `axis` рядом с `[lo, hi]`. Цель — вырожденная рамка
    /// на линии: по этой оси у неё одна координата, поперёк — длина линии подсказки.
    /// Линия фрейма действует только напротив него, как его края.
    fn guide_candidates(
        &self,
        axis: GuideAxis,
        lo: f64,
        hi: f64,
        threshold: f64,
        moving: Rect,
    ) -> Vec<(Rect, Vec<u32>)> {
        let mut out = Vec::new();
        for g in self.guides.iter() {
            if g.guide.axis != axis || g.rotated || self.skip_guide.as_ref() == Some(g) {
                continue;
            }
            if let GuideOwner::Frame(id) = g.owner {
                if self.excluded.contains(&id) {
                    continue;
                }
            }
            let line = g.mark.line;
            let (at, cross_a, cross_b, moving_lo, moving_hi) = match axis {
                GuideAxis::X => (line.p0.x, line.p0.y, line.p1.y, moving.y0, moving.y1),
                GuideAxis::Y => (line.p0.y, line.p0.x, line.p1.x, moving.x0, moving.x1),
            };
            if at < lo - threshold || at > hi + threshold {
                continue;
            }
            let (from, to) = if g.mark.infinite {
                (moving_lo, moving_hi)
            } else {
                if cross_b.max(cross_a) < moving_lo || cross_b.min(cross_a) > moving_hi {
                    continue;
                }
                (cross_a.min(cross_b), cross_a.max(cross_b))
            };
            out.push(match axis {
                GuideAxis::X => (Rect::new(at, from, at, to), Vec::new()),
                GuideAxis::Y => (Rect::new(from, at, to, at), Vec::new()),
            });
        }
        out
    }

````

В `crates/ferrum-editor/src/snap.rs` заменить:

````rust
        } else {
            let (lo, hi) = span(xs);
            self.candidates(
                scene,
                Rect::new(lo - threshold, view.y0, hi + threshold, view.y1),
            )
        };
        let y_cands = if ys.is_empty() {
````

на:

````rust
        } else {
            let (lo, hi) = span(xs);
            let mut c = self.candidates(
                scene,
                Rect::new(lo - threshold, view.y0, hi + threshold, view.y1),
            );
            c.extend(self.guide_candidates(GuideAxis::X, lo, hi, threshold, moving));
            c
        };
        let y_cands = if ys.is_empty() {
````

В `crates/ferrum-editor/src/snap.rs` заменить:

````rust
        } else {
            let (lo, hi) = span(ys);
            self.candidates(
                scene,
                Rect::new(view.x0, lo - threshold, view.x1, hi + threshold),
            )
        };
        let dx = best_offset(xs, &x_cands, x_targets, threshold);
````

на:

````rust
        } else {
            let (lo, hi) = span(ys);
            let mut c = self.candidates(
                scene,
                Rect::new(view.x0, lo - threshold, view.x1, hi + threshold),
            );
            c.extend(self.guide_candidates(GuideAxis::Y, lo, hi, threshold, moving));
            c
        };
        let dx = best_offset(xs, &x_cands, x_targets, threshold);
````

В `crates/ferrum-editor/src/tools/draw.rs` заменить:

````rust
            parent,
            parent_world,
            snapper: Snapper::for_parent(cx.scene, parent, &[]),
        });
    }
````

на:

````rust
            parent,
            parent_world,
            snapper: Snapper::for_parent(cx.scene, parent, &[]).with_guides(cx.snap_guides()),
        });
    }
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    /// `Editor::begin_doc_edit`).
    pub(crate) pen_open: bool,
}

````

на:

````rust
    /// `Editor::begin_doc_edit`).
    pub(crate) pen_open: bool,
    /// Линии направляющих страницы: цели привязки в жестах.
    pub(crate) guides: &'a crate::guides::GuideCache,
}

````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    pub(crate) fn view(&self) -> Rect {
        self.camera.visible_world_rect()
    }

````

на:

````rust
    pub(crate) fn view(&self) -> Rect {
        self.camera.visible_world_rect()
    }

    /// Направляющие как цели привязки: пустой список, если их не видно.
    pub(crate) fn snap_guides(&self) -> std::sync::Arc<Vec<crate::guides::PlacedGuide>> {
        if self.settings.rulers && self.settings.guides {
            self.guides.lines.clone()
        } else {
            Default::default()
        }
    }

````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
                State::Dragging(Drag {
                    start: world,
                    snapper: Snapper::new(cx.scene, &ids),
                    originals,
                    copies: None,
````

на:

````rust
                State::Dragging(Drag {
                    start: world,
                    snapper: Snapper::new(cx.scene, &ids).with_guides(cx.snap_guides()),
                    originals,
                    copies: None,
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
                    sbox,
                    bases,
                    snapper: Snapper::new(cx.scene, &ids),
                    transform: Affine::IDENTITY,
                    k: 1.0,
````

на:

````rust
                    sbox,
                    bases,
                    snapper: Snapper::new(cx.scene, &ids).with_guides(cx.snap_guides()),
                    transform: Affine::IDENTITY,
                    k: 1.0,
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
//! неверное — событие `Error` на русском, документ не меняется.

use ferrum_doc::{Color, Fill, FillKind, NodeId, PageId, ScaleMode, Stroke, StrokeAlign};
use ferrum_editor::scale::{Anchor, MAX_SCALE_FACTOR, MIN_SCALE_FACTOR};
use ferrum_editor::{
````

на:

````rust
//! неверное — событие `Error` на русском, документ не меняется.

use ferrum_doc::{
    Color, Fill, FillKind, Guide, GuideAxis, GuideOwner, NodeId, PageId, ScaleMode, Stroke,
    StrokeAlign,
};
use ferrum_editor::scale::{Anchor, MAX_SCALE_FACTOR, MIN_SCALE_FACTOR};
use ferrum_editor::{
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
use crate::protocol::{
    Align, Command, ConflictInfo, ContextTarget as TargetDto, CoreEvent, DropPosition, Edit,
    EditPhase, ExportFormat, Field, FillDto, FoundLayerDto, KeymapEntry, MenuActionDto,
    MenuItemDto, PageInfo, PaintKind, PresetInfo, ScaleModeDto, SelectMode, StrokeDto,
    EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{cap_of, join_of, summarize, MAX_PAINTS};
````

на:

````rust
use crate::protocol::{
    Align, Command, ConflictInfo, ContextTarget as TargetDto, CoreEvent, DropPosition, Edit,
    EditPhase, ExportFormat, Field, FillDto, FoundLayerDto, GuideAxisDto, GuideOwnerDto,
    KeymapEntry, MenuActionDto, MenuItemDto, PageInfo, PaintKind, PresetInfo, ScaleModeDto,
    SelectMode, StrokeDto, EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{cap_of, join_of, summarize, MAX_PAINTS};
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            }
            Command::SetView { rulers, guides } => self.editor.set_view(rulers, guides),
            // Команды этапа 4 разбирает `Core::command`. Список полный, без `_`:
            // новая команда без ветки здесь — ошибка сборки, а не молчаливый пропуск.
````

на:

````rust
            }
            Command::SetView { rulers, guides } => self.editor.set_view(rulers, guides),
            Command::SetGuides {
                owner,
                guides,
                phase: p,
            } => {
                // Сверх предела — до разбора: запрос отвергается целиком.
                if guides.len() > ferrum_doc::MAX_GUIDES {
                    self.error(format!(
                        "Направляющих не больше {} на один узел",
                        ferrum_doc::MAX_GUIDES
                    ));
                    return;
                }
                let owner = match owner {
                    GuideOwnerDto::Page { id } => self.page_id(&id).map(GuideOwner::Page),
                    GuideOwnerDto::Frame { id } => self.node(&id).map(GuideOwner::Frame),
                };
                let Some(owner) = owner else {
                    return;
                };
                let list = guides
                    .iter()
                    .map(|g| {
                        let axis = match g.axis {
                            GuideAxisDto::X => GuideAxis::X,
                            GuideAxisDto::Y => GuideAxis::Y,
                        };
                        Guide::new(axis, g.offset)
                    })
                    .collect();
                if let Err(e) = self.editor.set_guides(owner, list, phase(p)) {
                    self.error(e.to_string());
                }
            }
            // Команды этапа 4 разбирает `Core::command`. Список полный, без `_`:
            // новая команда без ветки здесь — ошибка сборки, а не молчаливый пропуск.
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

/// Чьи направляющие в `SetGuides`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum GuideOwnerDto {
    Page { id: String },
    Frame { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum GuideAxisDto {
    X,
    Y,
}

/// Направляющая: ось и смещение.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(deny_unknown_fields)]
pub struct GuideDto {
    pub axis: GuideAxisDto,
    pub offset: f64,
}

/// Команда основного потока worker'у.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    SetSettings {
        pixel_snap: bool,
    },
    /// Показ линеек и направляющих (спека подсистемы 2, разделы 9 и 11): настройка
````

на:

````rust
    SetSettings {
        pixel_snap: bool,
    },
    /// Направляющие страницы или фрейма верхнего уровня целиком (спека подсистемы 2,
    /// раздел 11): превью — без шага, запись — один шаг, отмена — назад к документу.
    /// Не больше 1000; смещение конечно и по модулю не больше 1e9; иначе — `Error`.
    SetGuides {
        owner: GuideOwnerDto,
        guides: Vec<GuideDto>,
        phase: EditPhase,
    },
    /// Показ линеек и направляющих (спека подсистемы 2, разделы 9 и 11): настройка
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Линейки и 2000|Состояния команд после шага, 100"`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Expected: PASS: fmt и clippy без замечаний; тесты без падений (новые: `ferrum-editor/tests/guides.rs` — 34, `ferrum-wasm/tests/context_menu.rs` — +1; весь workspace — 1140 passed); `editor_report`: «Линейки и 2000 линий» — движение указателя ≈ 0,003 мс, протяжка ≈ 0,005 мс, перетаскивание узла ≈ 0,043 мс (цель ≤ 8 мс) PASS; «Состояния команд после шага, 100 000 узлов, выделен 1» ≤ 1 мс PASS; wasm собран, typecheck без ошибок

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/examples/editor_report.rs crates/ferrum-editor/src/context_menu.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/guides.rs crates/ferrum-editor/src/snap.rs crates/ferrum-editor/src/tools/draw.rs crates/ferrum-editor/src/tools/mod.rs crates/ferrum-editor/src/tools/select.rs crates/ferrum-editor/tests/guides.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/context_menu.rs
git commit -F - <<'EOF'
feat(editor,wasm): направляющие протяжкой, привязка к ним, `SetGuides`

Протяжка из линейки создаёт линию у фрейма верхнего уровня под курсором, иначе у
страницы; за неё можно тянуть, на своей линейке — удаляется, каждый жест — один шаг.
Щелчок сквозь линию достаётся узлу. Линии — цели привязки в перетаскивании, ресайзе и
рисовании. Удержание клавиши не мигает переключателями.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4: Линейки и направляющие в интерфейсе

Спека §9: настройка вида сохраняется. Здесь — поля `rulers`, `guides` (веб и файл десктопа), `SetView` при запуске и из другой вкладки, запись по событию `View`; E2E `rulers.spec.ts` на весь сценарий.

**Files:**
- Modify: `apps/desktop/src-tauri/store/src/settings.rs`, `apps/web/src/core/file-list.ts`, `apps/web/src/ui/app.tsx`, `apps/web/src/ui/settings.ts`
- Test: `apps/desktop/src-tauri/store/tests/store.rs`, `apps/web/e2e/rulers.spec.ts` (новый), `apps/web/unit/settings.test.ts`

**Interfaces:**
- Consumes: `Command::SetView`, `CoreEvent::View` (задача 2), жест на холсте (задача 3); `DEFAULT_SETTINGS`, `loadSettings`, `SETTINGS_FIELDS`, `Editor.send`.
- Produces: `Settings.{rulers: boolean, guides: boolean}`; `SETTINGS_FIELDS` (10 полей в обоих файлах).

- [ ] **Step 1: Написать тесты**

В `apps/desktop/src-tauri/store/tests/store.rs` заменить:

````rust
    s.set(patch(serde_json::json!({"filesView": "list"})))
        .unwrap();
    s.set(patch(
        serde_json::json!({"recentCommands": "edit.undo ui.export"}),
````

на:

````rust
    s.set(patch(serde_json::json!({"filesView": "list"})))
        .unwrap();
    s.set(patch(serde_json::json!({"rulers": true, "guides": false})))
        .unwrap();
    s.set(patch(
        serde_json::json!({"recentCommands": "edit.undo ui.export"}),
````

В `apps/desktop/src-tauri/store/tests/store.rs` заменить:

````rust
            "theme": "light",
            "filesView": "list",
            "recentCommands": "edit.undo ui.export"
        })
````

на:

````rust
            "theme": "light",
            "filesView": "list",
            "rulers": true,
            "guides": false,
            "recentCommands": "edit.undo ui.export"
        })
````

Создать `apps/web/e2e/rulers.spec.ts`:

````ts
// Линейки и направляющие протяжкой (спека подсистемы 2, раздел 9): рисует их ядро в
// кадре, поэтому проверка — по пикселям снимка и по живому состоянию; настройка
// переживает перезагрузку. Линии в страницах примера: фреймы до (1380, 1020), справа
// и снизу от них пусто.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, expectPixel, live, liveWhen, mainMenu, openApp, pixel } from './helpers';

/** Цвет линейки и направляющей в тёмной теме (`OverlayTheme::dark`). */
const RULER = [0x23, 0x21, 0x20];
const GUIDE = [0xff, 0x5a, 0x5f];

/** Есть ли цвет направляющей в строке снимка рядом с `y`: положение линии — в целых
 * единицах мира, на экране при зуме 0,5 строка может быть соседней. */
async function guideRow(page: Page, x: number, y: number): Promise<boolean> {
  for (const row of [y - 1, y, y + 1]) {
    const p = await pixel(page, x, row);
    if (Math.abs(p[0] - GUIDE[0]) <= 40 && Math.abs(p[1] - GUIDE[1]) <= 50 && Math.abs(p[2] - GUIDE[2]) <= 50) return true;
  }
  return false;
}

async function expectGuide(page: Page, x: number, y: number, on = true): Promise<void> {
  await expect.poll(() => guideRow(page, x, y), { timeout: 10_000 }).toBe(on);
}

async function canvasBox(page: Page) {
  return (await page.locator('canvas').boundingBox())!;
}

async function rulersOn(page: Page) {
  const box = await canvasBox(page);
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+KeyR');
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  return box;
}

test('@firefox Shift+R и пункт «Вид → Линейки» включают линейки, настройка переживает перезагрузку', async ({ page }) => {
  await openApp(page);
  const box = await canvasBox(page);
  const canvas = await pixel(page, box.x + 60, box.y + 2);
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+KeyR');
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  await expectPixel(page, box.x + 2, box.y + 60, RULER);
  expect(await live(page).then((l) => l.steps)).toBe(0);
  // Пункт меню отмечен и снимает отметку.
  await mainMenu(page, 'Вид', 'Линейки');
  await expectPixel(page, box.x + 60, box.y + 2, canvas.slice(0, 3));
  // Настройка сохраняется: снова включаем — после перезагрузки включена.
  await mainMenu(page, 'Вид', 'Линейки');
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  await page.waitForTimeout(400); // запись настроек — раз в четверть секунды
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true);
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

test('@firefox «Линейки» в меню пустого места холста', async ({ page }) => {
  await openApp(page);
  const box = await canvasBox(page);
  const at = await appScreen(page, await live(page), 100, 1080);
  await page.mouse.click(at.x, at.y, { button: 'right' });
  const item = page.getByRole('menuitemcheckbox', { name: /Линейки/ });
  await expect(item).toBeVisible();
  await expect(item).toHaveAttribute('aria-checked', 'false');
  await item.click();
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
});

test('@firefox направляющая из линейки: один шаг, тянется, на линейке удаляется, отмена', async ({ page }) => {
  await openApp(page);
  const box = await rulersOn(page);
  const l = await live(page);
  // Ниже страницы примера: пусто, линия принадлежит странице.
  const low = await appScreen(page, l, 100, 1080);
  const y = Math.round(low.y);
  const x = box.x + 120; // левее тулбара
  expect(y).toBeLessThan(box.y + box.height - 40);
  await drag(page, { x, y: box.y + 10 }, 0, y - box.y - 10);
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expectGuide(page, box.x + 60, y);
  // Подвинуть за саму линию.
  await drag(page, { x, y }, 0, 30);
  await liveWhen(page, `l.steps === ${l.steps + 2}`);
  await expectGuide(page, box.x + 60, y + 30);
  // Обратно на линейку — удаление, тоже один шаг.
  await drag(page, { x, y: y + 30 }, 0, box.y + 10 - (y + 30));
  await liveWhen(page, `l.steps === ${l.steps + 3}`);
  await expectGuide(page, box.x + 60, y + 30, false);
  // Отмена возвращает линию, ещё отмена — на прежнее место.
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await expectGuide(page, box.x + 60, y + 30);
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await expectGuide(page, box.x + 60, y);
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

test('@firefox Esc во время протяжки — без следов', async ({ page }) => {
  await openApp(page);
  const box = await rulersOn(page);
  const l = await live(page);
  await page.mouse.move(box.x + 400, box.y + 10);
  await page.mouse.down();
  // Ниже страницы примера: рядом нет краёв узлов, привязка не сдвинет линию.
  await page.mouse.move(box.x + 400, box.y + 660, { steps: 5 });
  await expectGuide(page, box.x + 60, box.y + 660);
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expectGuide(page, box.x + 60, box.y + 660, false);
  expect((await live(page)).steps).toBe(l.steps);
});

test('@firefox к направляющей привязываются узлы, щелчок по линии выделяет узел под ней', async ({ page }) => {
  await openApp(page);
  const box = await rulersOn(page);
  const l = await live(page);
  // Вертикальная линия на мировой x = 400: «Слой 1» в «Экран 0» — (140, 0)–(260, 80).
  const line = await appScreen(page, l, 400, 40);
  const lineX = Math.round(line.x);
  await drag(page, { x: box.x + 10, y: box.y + 300 }, lineX - box.x - 10, 0);
  const placed = await liveWhen(page, `l.steps === ${l.steps + 1}`);
  // Положение линии — от координат экрана; читаем его сдвигом узла ниже.
  const node = await appScreen(page, placed, 200, 40);
  await page.mouse.click(node.x, node.y);
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionX === 140');
  // Левый край дойдёт почти до линии: остаётся 2 экранных пикселя — привязка.
  const lineWorld = Math.round((lineX - box.x - box.width / 2) / placed.zoom + placed.cameraX);
  const dx = (lineWorld - 140) * placed.zoom - 2;
  // Нажатие не в той же точке: иначе это был бы двойной щелчок, вход в правку.
  await drag(page, { x: node.x + 10, y: node.y + 5 }, dx, 0, 8);
  const snapped = await liveWhen(page, `l.steps === ${l.steps + 2}`);
  expect(snapped.selectionX).toBe(lineWorld);
  // Щелчок по линии над другим фреймом выделяет то, что под ней.
  await page.keyboard.press('Escape');
  await liveWhen(page, 'l.selectionCount === 0');
  await page.mouse.click(lineX, node.y + 40);
  await liveWhen(page, 'l.selectionCount === 1');
  expect((await live(page)).steps).toBe(snapped.steps);
});

test('@firefox Shift+G прячет направляющие: линий и привязки нет; без линеек пункт серый', async ({ page }) => {
  await openApp(page);
  const box = await rulersOn(page);
  const l = await live(page);
  const y = Math.round(box.y + box.height - 60);
  await drag(page, { x: box.x + 400, y: box.y + 10 }, 0, y - box.y - 10);
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expectGuide(page, box.x + 60, y);
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+KeyG');
  await expectGuide(page, box.x + 60, y, false);
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  // Отметка в меню снята.
  await mainMenu(page, 'Вид', 'Направляющие');
  await expectGuide(page, box.x + 60, y);
  // Линейки выключены — и линии не видно (как в Figma), пункт серый.
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+KeyR');
  await expectGuide(page, box.x + 60, y, false);
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await page.getByRole('menuitem', { name: 'Вид', exact: true }).hover();
  await expect(page.getByRole('menuitemcheckbox', { name: /Направляющие/ })).toHaveAttribute('aria-disabled', 'true');
});

test('удержание Shift+R не мигает линейками', async ({ page }) => {
  await openApp(page);
  const box = await rulersOn(page);
  // Повтор клавиши: то же сочетание без отпускания.
  await page.keyboard.down('Shift');
  await page.keyboard.down('KeyR');
  await page.keyboard.down('KeyR');
  await page.keyboard.down('KeyR');
  await page.keyboard.up('KeyR');
  await page.keyboard.up('Shift');
  const before = await live(page);
  await page.locator('canvas').hover({ position: { x: 300, y: 300 } });
  await page.waitForFunction((f) => window.__ferrum!.core!.live!.frames >= f, before.frames);
  // Первое нажатие выключило линейки, повторы ничего не поменяли.
  const px = await pixel(page, box.x + 60, box.y + 2);
  expect(px.slice(0, 3)).not.toEqual(RULER);
});
````

В `apps/web/unit/settings.test.ts` заменить:

````ts
  expect((await openSettings(fakeBackend({ theme: 'dark' }).backend, legacy)).current.theme).toBe('dark');
});
````

на:

````ts
  expect((await openSettings(fakeBackend({ theme: 'dark' }).backend, legacy)).current.theme).toBe('dark');
});

test('линейки и направляющие: умолчания и проверка значений', () => {
  expect(DEFAULT_SETTINGS.rulers).toBe(false);
  expect(DEFAULT_SETTINGS.guides).toBe(true);
  expect(parseSettingsPatch({ rulers: true, guides: false })).toEqual({ rulers: true, guides: false });
  expect(parseSettingsPatch({ rulers: 'да', guides: 1 })).toEqual({});
  expect(parseSettings({ rulers: 'да' }).rulers).toBe(false);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/settings.test.ts`
Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-desktop-store --test store`
Expected: FAIL — модульные — новый тест «линейки и направляющие: умолчания и проверка значений» падает (`Expected: false`, `Received: undefined`), остальные 26 проходят; `store` — `settings_are_written_by_field_and_checked` падает на `.unwrap()`: полей нет в `SETTINGS_FIELDS` десктопа. `e2e/rulers.spec.ts` проверяется в шаге «Проверить»

- [ ] **Step 3: Реализация**

В `apps/desktop/src-tauri/store/src/settings.rs` заменить:

````rust

/// Поля настроек — те же, что `SETTINGS_FIELDS` интерфейса.
pub const SETTINGS_FIELDS: [&str; 8] = [
    "theme",
    "pixelSnap",
````

на:

````rust

/// Поля настроек — те же, что `SETTINGS_FIELDS` интерфейса.
pub const SETTINGS_FIELDS: [&str; 10] = [
    "theme",
    "pixelSnap",
````

В `apps/desktop/src-tauri/store/src/settings.rs` заменить:

````rust
    "filesView",
    "recentCommands",
];
/// Запись настроек не больше этого (как `MAX_SETTINGS_BYTES`).
````

на:

````rust
    "filesView",
    "recentCommands",
    "rulers",
    "guides",
];
/// Запись настроек не больше этого (как `MAX_SETTINGS_BYTES`).
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
/** Поля настроек пользователя в хранилище `settings` — по ключу на поле: вкладки
 * пишут только свои изменения и не затирают чужие. Значения проверяет интерфейс. */
export const SETTINGS_FIELDS = ['theme', 'pixelSnap', 'keymap', 'leftWidth', 'rightWidth', 'pagesOpen', 'filesView', 'recentCommands'] as const;

/** Запись о файле — без байтов: список читает только метаданные. Время — мс эпохи. */
````

на:

````ts
/** Поля настроек пользователя в хранилище `settings` — по ключу на поле: вкладки
 * пишут только свои изменения и не затирают чужие. Значения проверяет интерфейс. */
export const SETTINGS_FIELDS = ['theme', 'pixelSnap', 'keymap', 'leftWidth', 'rightWidth', 'pagesOpen', 'filesView', 'recentCommands', 'rulers', 'guides'] as const;

/** Запись о файле — без байтов: список читает только метаданные. Время — мс эпохи. */
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  const [themeChoice, setThemeChoice] = createSignal<ThemeChoice>(saved.theme);
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  /** Недавние команды «Быстрых действий»: новые первыми, общие для вкладок. */
````

на:

````tsx
  const [themeChoice, setThemeChoice] = createSignal<ThemeChoice>(saved.theme);
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
  // Настройка вида: команду выполняет ядро (событие `view`), хранит — интерфейс.
  const [rulers, setRulers] = createSignal(saved.rulers);
  const [guides, setGuides] = createSignal(saved.guides);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  /** Недавние команды «Быстрых действий»: новые первыми, общие для вкладок. */
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  });
  createEffect(() => editor()?.send({ type: 'setSettings', pixelSnap: pixelSnap() }));
  // Запись — не чаще раза в четверть секунды (края панелей тянутся каждый кадр), и
  // сразу при уходе со страницы.
````

на:

````tsx
  });
  createEffect(() => editor()?.send({ type: 'setSettings', pixelSnap: pixelSnap() }));
  createEffect(() => editor()?.send({ type: 'setView', rulers: rulers(), guides: guides() }));
  // Запись — не чаще раза в четверть секунды (края панелей тянутся каждый кадр), и
  // сразу при уходе со страницы.
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      written = { ...written, pixelSnap: p.pixelSnap };
      setPixelSnap(p.pixelSnap);
    }
    if (p.recentCommands !== undefined) {
````

на:

````tsx
      written = { ...written, pixelSnap: p.pixelSnap };
      setPixelSnap(p.pixelSnap);
    }
    if (p.rulers !== undefined) {
      written = { ...written, rulers: p.rulers };
      setRulers(p.rulers);
    }
    if (p.guides !== undefined) {
      written = { ...written, guides: p.guides };
      setGuides(p.guides);
    }
    if (p.recentCommands !== undefined) {
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      theme: themeChoice(),
      pixelSnap: pixelSnap(),
      keymap: keymap(),
      leftWidth: left(),
````

на:

````tsx
      theme: themeChoice(),
      pixelSnap: pixelSnap(),
      rulers: rulers(),
      guides: guides(),
      keymap: keymap(),
      leftWidth: left(),
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        ed.send({ type: 'setTheme', theme: theme(), system: themeChoice() === 'system' });
        ed.send({ type: 'setSettings', pixelSnap: pixelSnap() });
        // Нынешняя раскладка: другая вкладка могла сменить её, пока ядро запускалось.
        const km = keymap();
````

на:

````tsx
        ed.send({ type: 'setTheme', theme: theme(), system: themeChoice() === 'system' });
        ed.send({ type: 'setSettings', pixelSnap: pixelSnap() });
        ed.send({ type: 'setView', rulers: rulers(), guides: guides() });
        // Нынешняя раскладка: другая вкладка могла сменить её, пока ядро запускалось.
        const km = keymap();
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        ed.core.on((e) => {
          if (e.type === 'uiCommand') uiCommand(e.key);
          if (!opening && linkedPage && e.type === 'pages') {
            const want = linkedPage;
````

на:

````tsx
        ed.core.on((e) => {
          if (e.type === 'uiCommand') uiCommand(e.key);
          if (e.type === 'view') {
            setRulers(e.rulers);
            setGuides(e.guides);
          }
          if (!opening && linkedPage && e.type === 'pages') {
            const want = linkedPage;
````

В `apps/web/src/ui/settings.ts` заменить:

````ts
// Настройки пользователя: тема, привязка к пикселям, отличия раскладки клавиш,
// ширина панелей, вид экрана файлов (сетка или список), недавние команды
// «Быстрых действий». С этапа 6 хранятся в
// IndexedDB по полю (`settings-store.ts`); здесь — поля, их разбор (значения из
// хранилища не доверенные: неверное поле — умолчание) и прежняя запись
````

на:

````ts
// Настройки пользователя: тема, привязка к пикселям, отличия раскладки клавиш,
// ширина панелей, вид экрана файлов (сетка или список), недавние команды
// «Быстрых действий», линейки и направляющие. С этапа 6 хранятся в
// IndexedDB по полю (`settings-store.ts`); здесь — поля, их разбор (значения из
// хранилища не доверенные: неверное поле — умолчание) и прежняя запись
````

В `apps/web/src/ui/settings.ts` заменить:

````ts
   * появилось в подсистеме 2; в старых записях его нет — тогда пусто). */
  recentCommands: string;
}

````

на:

````ts
   * появилось в подсистеме 2; в старых записях его нет — тогда пусто). */
  recentCommands: string;
  /** Линейки и направляющие (спека подсистемы 2, раздел 9): настройка вида. */
  rulers: boolean;
  guides: boolean;
}

````

В `apps/web/src/ui/settings.ts` заменить:

````ts
  filesView: 'grid',
  recentCommands: '',
};

````

на:

````ts
  filesView: 'grid',
  recentCommands: '',
  rulers: false,
  guides: true,
};

````

В `apps/web/src/ui/settings.ts` заменить:

````ts
  // Только верные ключи, не больше восьми; строка длиннее не бывает (предел — на разбор).
  recentCommands: (v) => (typeof v === 'string' && v.length <= 1024 ? parseRecent(v).join(' ') : undefined),
};

````

на:

````ts
  // Только верные ключи, не больше восьми; строка длиннее не бывает (предел — на разбор).
  recentCommands: (v) => (typeof v === 'string' && v.length <= 1024 ? parseRecent(v).join(' ') : undefined),
  rulers: (v) => (typeof v === 'boolean' ? v : undefined),
  guides: (v) => (typeof v === 'boolean' ? v : undefined),
};

````

- [ ] **Step 4: Проверить**

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-desktop-store 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo check`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/rulers.spec.ts e2e/settings-store.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/rulers.spec.ts --project=firefox`

Expected: тесты `store` без падений, `cargo check` десктопа без ошибок; модульные (`settings.test.ts` +1) и сборка — ок; E2E `rulers.spec.ts` (7 тестов) и `settings-store` в Chromium, `rulers.spec.ts` в Firefox (тесты с меткой `@firefox`) — без падений, `__ferrumWarnings` пуст

- [ ] **Step 5: Закоммитить**

```bash
git add apps/desktop/src-tauri/store/src/settings.rs apps/desktop/src-tauri/store/tests/store.rs apps/web/e2e/rulers.spec.ts apps/web/src/core/file-list.ts apps/web/src/ui/app.tsx apps/web/src/ui/settings.ts apps/web/unit/settings.test.ts
git commit -F - <<'EOF'
feat(web): линейки и направляющие в интерфейсе, настройка вида сохраняется

Поля настроек `rulers` и `guides` (веб и десктоп), `SetView` при запуске и из другой
вкладки, событие `View` от команды. E2E `rulers.spec.ts` (Chromium и Firefox): включение,
перезагрузка, меню, протяжка, удаление, отмена, Esc, привязка, Shift+G.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 5: Сворачиваемые секции и боковые панели

Спека §10: заголовок секции сворачивает её, свёрнутость хранится по имени; панель сворачивается кнопкой в шапке; ширина и свёрнутость запоминаются.

**Files:**
- Modify: `apps/desktop/src-tauri/store/src/settings.rs`, `apps/web/src/core/file-list.ts`, `apps/web/src/ui/app.tsx`, `apps/web/src/ui/chrome.tsx`, `apps/web/src/ui/context.ts`, `apps/web/src/ui/icons.tsx`, `apps/web/src/ui/missing-fonts.tsx`, `apps/web/src/ui/properties.tsx`, `apps/web/src/ui/settings.ts`, `apps/web/src/ui/styles/chrome.css`, `apps/web/src/ui/styles/theme.css`
- Test: `apps/web/e2e/panels.spec.ts` (новый), `apps/web/e2e/rulers.spec.ts`, `apps/web/unit/settings.test.ts`

**Interfaces:**
- Consumes: `useEditor`, `FileHeader`, `ResizeEdge`, `Icon`, `DEFAULT_SETTINGS`, `SETTINGS_FIELDS`.
- Produces: `Section { title, name?, children, actions?, muted? }`, `PanelToggle { side, collapsed, onToggle }`, `parseSections(text)`; `Settings.{leftCollapsed, rightCollapsed, collapsedSections}`; `Editor.{sectionCollapsed, toggleSection}` в контексте; `SETTINGS_FIELDS` (13 полей); значки `panelLeft`, `panelRight`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/e2e/panels.spec.ts`:

````ts
// Полировка панелей (спека подсистемы 2, раздел 10): секции сворачиваются и помнят
// это по имени, боковые панели сворачиваются кнопкой в шапке, ширина и свёрнутость
// переживают перезагрузку.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

/** Настройки в базе: запись идёт раз в четверть секунды — ждём её, а не время. */
const stored = (page: Page) => page.evaluate(() => window.__ferrumFiles!.store.settings());

const reload = async (page: Page) => {
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true);
};

const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

test('@firefox заголовок секции сворачивает её, свёрнутость хранится по имени', async ({ page }) => {
  await openApp(page);
  const exportSection = page.getByRole('region', { name: 'Экспорт' });
  const toggle = exportSection.getByRole('button', { name: 'Экспорт', exact: true });
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await expect(exportSection.locator('[data-export-run]')).toBeVisible();
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await expect(exportSection.locator('[data-export-run]')).toHaveCount(0);
  // Другие секции не тронуты.
  await expect(page.getByRole('region', { name: 'Страница' }).getByRole('button', { name: 'Страница' })).toHaveAttribute('aria-expanded', 'true');
  // С клавиатуры: Enter по заголовку.
  await toggle.focus();
  await page.keyboard.press('Enter');
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('Enter');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');

  // Та же секция при другом выделении — свёрнута; имя «Положение» и у фрейма.
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await expect(exportSection.getByRole('button', { name: 'Экспорт', exact: true })).toHaveAttribute('aria-expanded', 'false');
  const position = page.getByRole('region', { name: 'Положение' });
  await position.getByRole('button', { name: 'Положение' }).click();
  await expect(position.getByRole('button', { name: 'Положение' })).toHaveAttribute('aria-expanded', 'false');
  // Полей больше не видно.
  await expect(position.getByRole('textbox')).toHaveCount(0);

  await expect.poll(() => stored(page)).toMatchObject({ collapsedSections: 'Экспорт\nПоложение' });
  await reload(page);
  await expect(page.getByRole('region', { name: 'Экспорт' }).getByRole('button', { name: 'Экспорт', exact: true })).toHaveAttribute('aria-expanded', 'false');
  expect(await warnings(page)).toEqual([]);
});

test('@firefox «Экспорт…» из меню разворачивает свёрнутую секцию и панель', async ({ page }) => {
  await openApp(page);
  await page.getByRole('region', { name: 'Экспорт' }).getByRole('button', { name: 'Экспорт', exact: true }).click();
  await page.getByRole('button', { name: 'Свернуть правую панель' }).click();
  await expect(page.locator('[data-export-run]')).toHaveCount(0);
  await mainMenu(page, 'Файл', /Экспорт/);
  await expect(page.locator('[data-export-run]')).toBeFocused();
});

test('@firefox левая панель сворачивается кнопкой в шапке, меню остаётся, ширина и свёрнутость запоминаются', async ({ page }) => {
  await openApp(page);
  const canvas = page.locator('canvas');
  const before = (await canvas.boundingBox())!;
  const collapse = page.getByRole('button', { name: 'Свернуть левую панель' });
  await expect(collapse).toHaveAttribute('aria-expanded', 'true');
  await collapse.focus();
  await page.keyboard.press('Enter');
  const expand = page.getByRole('button', { name: 'Развернуть левую панель' });
  await expect(expand).toBeFocused();
  await expect(page.getByRole('region', { name: 'Слои' })).toHaveCount(0);
  await expect(page.getByRole('region', { name: 'Страницы' })).toHaveCount(0);
  await expect.poll(async () => (await canvas.boundingBox())!.width).toBe(before.width + 248 - 48);
  await expect.poll(async () => (await live(page)).viewportW).toBe(before.width + 248 - 48);
  // Главное меню — на месте.
  await mainMenu(page, 'Вид', 'Линейки');
  await liveWhen(page, 'true');

  await expect.poll(() => stored(page)).toMatchObject({ leftCollapsed: true });
  await reload(page);
  await expect(page.getByRole('button', { name: 'Развернуть левую панель' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Слои' })).toHaveCount(0);

  // Развернуть — с прежней шириной, затем сменить её и снова перезагрузить.
  await page.getByRole('button', { name: 'Развернуть левую панель' }).click();
  await expect(page.getByRole('region', { name: 'Слои' })).toBeVisible();
  await expect.poll(async () => (await canvas.boundingBox())!.x).toBe(248);
  const edge = page.getByRole('separator', { name: 'Ширина левой панели' });
  await edge.focus();
  for (const x of [256, 264, 272]) {
    await page.keyboard.press('ArrowRight');
    await expect.poll(async () => (await canvas.boundingBox())!.x).toBe(x);
  }
  await expect.poll(() => stored(page)).toMatchObject({ leftCollapsed: false, leftWidth: 272 });
  await reload(page);
  await expect.poll(async () => (await page.locator('canvas').boundingBox())!.x).toBe(272);
  expect(await warnings(page)).toEqual([]);
});

test('@firefox правая панель сворачивается и помнит это', async ({ page }) => {
  await openApp(page);
  const canvas = page.locator('canvas');
  const before = (await canvas.boundingBox())!;
  await page.getByRole('button', { name: 'Свернуть правую панель' }).click();
  await expect(page.getByRole('tabpanel')).toHaveCount(0);
  await expect.poll(async () => (await canvas.boundingBox())!.width).toBe(before.width + 248 - 48);
  await expect.poll(() => stored(page)).toMatchObject({ rightCollapsed: true });
  await reload(page);
  await expect(page.getByRole('button', { name: 'Развернуть правую панель' })).toBeVisible();
  await page.getByRole('button', { name: 'Развернуть правую панель' }).click();
  await expect(page.getByRole('tabpanel')).toBeVisible();
  await expect.poll(async () => (await page.locator('canvas').boundingBox())!.width).toBe(before.width);
});
````

В `apps/web/e2e/rulers.spec.ts` заменить:

````ts
  await mainMenu(page, 'Вид', 'Линейки');
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  await page.waitForTimeout(400); // запись настроек — раз в четверть секунды
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true);
````

на:

````ts
  await mainMenu(page, 'Вид', 'Линейки');
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  await expect.poll(() => page.evaluate(() => window.__ferrumFiles!.store.settings())).toMatchObject({ rulers: true });
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true);
````

В `apps/web/unit/settings.test.ts` заменить:

````ts
  parseSettings,
  parseSettingsPatch,
  THEME_KEY,
  PANEL_MAX,
````

на:

````ts
  parseSettings,
  parseSettingsPatch,
  parseSections,
  THEME_KEY,
  PANEL_MAX,
````

В `apps/web/unit/settings.test.ts` заменить:

````ts
  expect(parseSettings({ rulers: 'да' }).rulers).toBe(false);
});
````

на:

````ts
  expect(parseSettings({ rulers: 'да' }).rulers).toBe(false);
});

test('свёрнутые панели и секции: умолчания и не доверенные значения', () => {
  expect(DEFAULT_SETTINGS.leftCollapsed).toBe(false);
  expect(DEFAULT_SETTINGS.rightCollapsed).toBe(false);
  expect(parseSettingsPatch({ leftCollapsed: true, rightCollapsed: 1 })).toEqual({ leftCollapsed: true });
  expect(parseSections('Вид\n\nВид\nЭкспорт')).toEqual(['Вид', 'Экспорт']);
  expect(parseSections(`${'я'.repeat(65)}\nВид`)).toEqual(['Вид']);
  expect(parseSections(Array.from({ length: 50 }, (_, i) => `с${i}`).join('\n'))).toHaveLength(32);
  expect(parseSettingsPatch({ collapsedSections: 'x'.repeat(4000) })).toEqual({});
  expect(parseSettingsPatch({ collapsedSections: ['Вид'] })).toEqual({});
  expect(parseSettingsPatch({ collapsedSections: 'Вид\nВид' })).toEqual({ collapsedSections: 'Вид' });
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/settings.test.ts`
Expected: FAIL — модульные: `SyntaxError: … does not provide an export named 'parseSections'` при загрузке `settings.test.ts`, «No tests found». `e2e/panels.spec.ts` проверяется в шаге «Проверить»

- [ ] **Step 3: Реализация**

В `apps/desktop/src-tauri/store/src/settings.rs` заменить:

````rust

/// Поля настроек — те же, что `SETTINGS_FIELDS` интерфейса.
pub const SETTINGS_FIELDS: [&str; 10] = [
    "theme",
    "pixelSnap",
````

на:

````rust

/// Поля настроек — те же, что `SETTINGS_FIELDS` интерфейса.
pub const SETTINGS_FIELDS: [&str; 13] = [
    "theme",
    "pixelSnap",
````

В `apps/desktop/src-tauri/store/src/settings.rs` заменить:

````rust
    "rulers",
    "guides",
];
/// Запись настроек не больше этого (как `MAX_SETTINGS_BYTES`).
````

на:

````rust
    "rulers",
    "guides",
    "leftCollapsed",
    "rightCollapsed",
    "collapsedSections",
];
/// Запись настроек не больше этого (как `MAX_SETTINGS_BYTES`).
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
/** Поля настроек пользователя в хранилище `settings` — по ключу на поле: вкладки
 * пишут только свои изменения и не затирают чужие. Значения проверяет интерфейс. */
export const SETTINGS_FIELDS = ['theme', 'pixelSnap', 'keymap', 'leftWidth', 'rightWidth', 'pagesOpen', 'filesView', 'recentCommands', 'rulers', 'guides'] as const;

/** Запись о файле — без байтов: список читает только метаданные. Время — мс эпохи. */
````

на:

````ts
/** Поля настроек пользователя в хранилище `settings` — по ключу на поле: вкладки
 * пишут только свои изменения и не затирают чужие. Значения проверяет интерфейс. */
export const SETTINGS_FIELDS = ['theme', 'pixelSnap', 'keymap', 'leftWidth', 'rightWidth', 'pagesOpen', 'filesView', 'recentCommands', 'rulers', 'guides', 'leftCollapsed', 'rightCollapsed', 'collapsedSections'] as const;

/** Запись о файле — без байтов: список читает только метаданные. Время — мс эпохи. */
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES } from '../core/limits';
import { boot, dismissSplash, isTestMode } from './boot';
import { DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { buildContextMenu, type ContextTarget } from './context-menu';
import { EditorContext, type Editor } from './context';
````

на:

````tsx
import { MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES } from '../core/limits';
import { boot, dismissSplash, isTestMode } from './boot';
import { DesignHeader, FileHeader, PanelToggle, ResizeEdge } from './chrome';
import { buildContextMenu, type ContextTarget } from './context-menu';
import { EditorContext, type Editor } from './context';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { PropertiesPanel } from './properties';
import { QuickActions } from './quick-actions';
import { diffSettings, resolveTheme, type Settings, type ThemeChoice } from './settings';
import type { SettingsStore } from './settings-store';
import { SettingsDialog } from './settings-dialog';
````

на:

````tsx
import { PropertiesPanel } from './properties';
import { QuickActions } from './quick-actions';
import { diffSettings, parseSections, resolveTheme, type Settings, type ThemeChoice } from './settings';
import type { SettingsStore } from './settings-store';
import { SettingsDialog } from './settings-dialog';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  const [right, setRight] = createSignal(saved.rightWidth);
  const [pagesOpen, setPagesOpen] = createSignal(saved.pagesOpen);
  const [themeChoice, setThemeChoice] = createSignal<ThemeChoice>(saved.theme);
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
````

на:

````tsx
  const [right, setRight] = createSignal(saved.rightWidth);
  const [pagesOpen, setPagesOpen] = createSignal(saved.pagesOpen);
  const [leftCollapsed, setLeftCollapsed] = createSignal(saved.leftCollapsed);
  const [rightCollapsed, setRightCollapsed] = createSignal(saved.rightCollapsed);
  const [sections, setSections] = createSignal<ReadonlySet<string>>(new Set(parseSections(saved.collapsedSections)));
  const [themeChoice, setThemeChoice] = createSignal<ThemeChoice>(saved.theme);
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      rightWidth: right(),
      pagesOpen: pagesOpen(),
      recentCommands: recent().join(' '),
    };
````

на:

````tsx
      rightWidth: right(),
      pagesOpen: pagesOpen(),
      leftCollapsed: leftCollapsed(),
      rightCollapsed: rightCollapsed(),
      collapsedSections: [...sections()].join('\n'),
      recentCommands: recent().join(' '),
    };
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        ed.uiCommand = uiCommand;
        ed.noteRecent = (key) => setRecent((r) => pushRecent(r, key));
        ed.contextMenu = (target: ContextTarget, at, label, onAction) => {
          setContext(null);
````

на:

````tsx
        ed.uiCommand = uiCommand;
        ed.noteRecent = (key) => setRecent((r) => pushRecent(r, key));
        ed.sectionCollapsed = (name) => sections().has(name);
        ed.toggleSection = (name) =>
          setSections((list) => {
            const next = new Set(list);
            if (!next.delete(name)) next.add(name);
            return next;
          });
        ed.contextMenu = (target: ContextTarget, at, label, onAction) => {
          setContext(null);
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      case 'ui.export':
        // «Экспорт…»: к кнопке секции экспорта панели свойств (настройки — там же).
        document.querySelector<HTMLElement>('[data-export-run]')?.focus();
        break;
````

на:

````tsx
      case 'ui.export':
        // «Экспорт…»: к кнопке секции экспорта панели свойств (настройки — там же).
        // Панель и секция могли быть свёрнуты: сигналы вне пачки обновляют сразу.
        setRightCollapsed(false);
        setSections((list) => new Set([...list].filter((n) => n !== 'Экспорт')));
        document.querySelector<HTMLElement>('[data-export-run]')?.focus();
        break;
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      class="fe-app"
      classList={{ 'fe-app--bare': store.state.uiHidden }}
      style={{ '--left-w': `${left()}px`, '--right-w': `${right()}px` }}
    >
      <main class="fe-stage">
````

на:

````tsx
      class="fe-app"
      classList={{ 'fe-app--bare': store.state.uiHidden }}
      style={{ '--left-w': leftCollapsed() ? 'var(--panel-rail)' : `${left()}px`, '--right-w': rightCollapsed() ? 'var(--panel-rail)' : `${right()}px` }}
    >
      <main class="fe-stage">
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
                onOpen={setMainMenu}
                onBack={() => props.navigate({ kind: 'files', section: 'recent' })}
              />
              <PagesPanel open={pagesOpen()} onToggle={setPagesOpen} fileId={id} />
              <LayersPanel />
              <ResizeEdge side="left" width={left()} onWidth={setLeft} />
            </aside>
            <aside class="fe-side fe-side--right" aria-label="Свойства">
              <DesignHeader />
              <PropertiesPanel theme={theme()} />
              <ResizeEdge side="right" width={right()} onWidth={setRight} />
            </aside>
            <TooltipLayer />
````

на:

````tsx
                onOpen={setMainMenu}
                onBack={() => props.navigate({ kind: 'files', section: 'recent' })}
                collapsed={leftCollapsed()}
                onCollapse={setLeftCollapsed}
              />
              <Show when={!leftCollapsed()}>
                <PagesPanel open={pagesOpen()} onToggle={setPagesOpen} fileId={id} />
                <LayersPanel />
                <ResizeEdge side="left" width={left()} onWidth={setLeft} />
              </Show>
            </aside>
            <aside class="fe-side fe-side--right" aria-label="Свойства">
              <div class="fe-side__toggle">
                <PanelToggle side="right" collapsed={rightCollapsed()} onToggle={setRightCollapsed} />
              </div>
              <Show when={!rightCollapsed()}>
                <DesignHeader />
                <PropertiesPanel theme={theme()} />
                <ResizeEdge side="right" width={right()} onWidth={setRight} />
              </Show>
            </aside>
            <TooltipLayer />
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
  onOpen: (open: boolean) => void;
  onBack?: () => void;
}) {
  const ed = useEditor();
````

на:

````tsx
  onOpen: (open: boolean) => void;
  onBack?: () => void;
  /** Панель свёрнута в полосу: остаются логотип с меню и кнопка разворота. */
  collapsed?: boolean;
  onCollapse?: (collapsed: boolean) => void;
}) {
  const ed = useEditor();
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
  let logo!: HTMLButtonElement;
  return (
    <div class="fe-panel-head">
      <button
        ref={logo}
````

на:

````tsx
  let logo!: HTMLButtonElement;
  return (
    <div class="fe-panel-head" classList={{ 'fe-panel-head--rail': props.collapsed }}>
      <button
        ref={logo}
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
        <Caret />
      </button>
      <div ref={file} class="fe-panel-head__file">
        <Show
````

на:

````tsx
        <Caret />
      </button>
      <Show when={!props.collapsed}>
      <div ref={file} class="fe-panel-head__file">
        <Show
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
          </button>
        )}
      </Show>
      <Show when={props.open}>
````

на:

````tsx
          </button>
        )}
      </Show>
      </Show>
      <Show when={props.onCollapse}>
        {(toggle) => <PanelToggle side="left" collapsed={!!props.collapsed} onToggle={toggle()} />}
      </Show>
      <Show when={props.open}>
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
}

/** Край панели: перетаскивание меняет ширину, стрелки с клавиатуры — на 8 px. */
export function ResizeEdge(props: { side: 'left' | 'right'; width: number; onWidth: (w: number) => void }) {
````

на:

````tsx
}

/** Кнопка в шапке панели: свернуть в полосу или развернуть, как в Figma UI3. Элемент
 * один и тот же в обоих состояниях: фокус с него не теряется. */
export function PanelToggle(props: { side: 'left' | 'right'; collapsed: boolean; onToggle: (collapsed: boolean) => void }) {
  const name = () => `${props.collapsed ? 'Развернуть' : 'Свернуть'} ${props.side === 'left' ? 'левую' : 'правую'} панель`;
  return (
    <button
      class="fe-tool fe-tool--small fe-tool--muted"
      aria-label={name()}
      aria-expanded={!props.collapsed}
      data-tip={name()}
      onClick={() => props.onToggle(!props.collapsed)}
    >
      <Icon name={props.side === 'left' ? 'panelLeft' : 'panelRight'} />
    </button>
  );
}

/** Край панели: перетаскивание меняет ширину, стрелки с клавиатуры — на 8 px. */
export function ResizeEdge(props: { side: 'left' | 'right'; width: number; onWidth: (w: number) => void }) {
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
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
````

на:

````tsx
}

/** Секция панели свойств: заголовок сворачивает её (спека подсистемы 2, раздел 10).
 * `name` — под каким именем свёрнутость хранится, если заголовок меняется. */
export function Section(props: { title: string; name?: string; children: JSX.Element; actions?: JSX.Element; muted?: boolean }) {
  const ed = useEditor();
  const name = () => props.name ?? props.title;
  const collapsed = () => ed.sectionCollapsed?.(name()) ?? false;
  return (
    <section class="fe-section" aria-label={props.title}>
      <div class="fe-section__head" classList={{ 't-muted': props.muted }}>
        <button class="fe-section__toggle" aria-expanded={!collapsed()} onClick={() => ed.toggleSection?.(name())}>
          <Icon name="expand" size={12} class={collapsed() ? '' : 'fe-rot90'} />
          <h2 class="fe-section__title">{props.title}</h2>
        </button>
        <div class="fe-section__actions">{props.actions}</div>
      </div>
      <Show when={!collapsed()}>{props.children}</Show>
    </section>
  );
````

В `apps/web/src/ui/context.ts` заменить:

````ts
   * Ставит редактор приложения. */
  noteRecent?: (key: string) => void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
````

на:

````ts
   * Ставит редактор приложения. */
  noteRecent?: (key: string) => void;
  /** Секция панели свойств свёрнута (сигнал; хранится в настройках). Ставит редактор
   * приложения; без него секции раскрыты. */
  sectionCollapsed?: (name: string) => boolean;
  toggleSection?: (name: string) => void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
````

В `apps/web/src/ui/icons.tsx` заменить:

````tsx
  autoWidth: '<path d="m18 8 4 4-4 4"/><path d="M2 12h20"/><path d="m6 8-4 4 4 4"/>',
  autoHeight: '<path d="M12 2v20"/><path d="m8 18 4 4 4-4"/><path d="m8 6 4-4 4 4"/>',
  keyboard: '<path d="M10 8h.01"/><path d="M12 12h.01"/><path d="M14 8h.01"/><path d="M16 12h.01"/><path d="M18 8h.01"/><path d="M6 8h.01"/><path d="M7 16h10"/><path d="M8 12h.01"/><rect width="20" height="16" x="2" y="4" rx="2"/>',
} as const;
````

на:

````tsx
  autoWidth: '<path d="m18 8 4 4-4 4"/><path d="M2 12h20"/><path d="m6 8-4 4 4 4"/>',
  autoHeight: '<path d="M12 2v20"/><path d="m8 18 4 4 4-4"/><path d="m8 6 4-4 4 4"/>',
  panelLeft: '<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/>',
  panelRight: '<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M15 3v18"/>',
  keyboard: '<path d="M10 8h.01"/><path d="M12 12h.01"/><path d="M14 8h.01"/><path d="M16 12h.01"/><path d="M18 8h.01"/><path d="M6 8h.01"/><path d="M7 16h10"/><path d="M8 12h.01"/><rect width="20" height="16" x="2" y="4" rx="2"/>',
} as const;
````

В `apps/web/src/ui/missing-fonts.tsx` заменить:

````tsx
  return (
    <Show when={list().length > 0}>
      <Section title={`Отсутствующие шрифты (${list().length})`}>
        <For each={list()}>{(f) => <MissingRow font={f} catalog={catalog()} />}</For>
      </Section>
````

на:

````tsx
  return (
    <Show when={list().length > 0}>
      <Section name="Отсутствующие шрифты" title={`Отсутствующие шрифты (${list().length})`}>
        <For each={list()}>{(f) => <MissingRow font={f} catalog={catalog()} />}</For>
      </Section>
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
  const r = () => props.val(s().rotation, live().rotation);
  return (
    <Section title={frame() ? 'Фрейм' : 'Положение'}>
      <Show when={frame() && s().count === 1}>
        <button
````

на:

````tsx
  const r = () => props.val(s().rotation, live().rotation);
  return (
    <Section title={frame() ? 'Фрейм' : 'Положение'} name="Положение">
      <Show when={frame() && s().count === 1}>
        <button
````

В `apps/web/src/ui/settings.ts` заменить:

````ts
  rulers: boolean;
  guides: boolean;
}

````

на:

````ts
  rulers: boolean;
  guides: boolean;
  /** Боковые панели свёрнуты в узкую полосу (спека подсистемы 2, раздел 10). */
  leftCollapsed: boolean;
  rightCollapsed: boolean;
  /** Свёрнутые секции панели свойств: имена через перевод строки. */
  collapsedSections: string;
}

````

В `apps/web/src/ui/settings.ts` заменить:

````ts
  rulers: false,
  guides: true,
};

````

на:

````ts
  rulers: false,
  guides: true,
  leftCollapsed: false,
  rightCollapsed: false,
  collapsedSections: '',
};

````

В `apps/web/src/ui/settings.ts` заменить:

````ts
    return null;
  }
}

````

на:

````ts
    return null;
  }
}

/** Свёрнутых секций не больше стольких, имя секции — не длиннее; их мало и они короткие. */
const MAX_SECTIONS = 32;
const MAX_SECTION_NAME = 64;
const MAX_SECTIONS_BYTES = 2048;

/** Имена свёрнутых секций из настройки: значение не доверенное — пустые, длинные и
 * повторы отбрасываются, лишние не читаются. */
export function parseSections(text: string): string[] {
  const out = new Set<string>();
  for (const name of text.split('\n')) {
    if (out.size === MAX_SECTIONS) break;
    if (name && name.length <= MAX_SECTION_NAME) out.add(name);
  }
  return [...out];
}

````

В `apps/web/src/ui/settings.ts` заменить:

````ts
  rulers: (v) => (typeof v === 'boolean' ? v : undefined),
  guides: (v) => (typeof v === 'boolean' ? v : undefined),
};

````

на:

````ts
  rulers: (v) => (typeof v === 'boolean' ? v : undefined),
  guides: (v) => (typeof v === 'boolean' ? v : undefined),
  leftCollapsed: (v) => (typeof v === 'boolean' ? v : undefined),
  rightCollapsed: (v) => (typeof v === 'boolean' ? v : undefined),
  collapsedSections: (v) => (typeof v === 'string' && v.length <= MAX_SECTIONS_BYTES ? parseSections(v).join('\n') : undefined),
};

````

В `apps/web/src/ui/styles/chrome.css` заменить:

````css
  border-bottom: 1px solid var(--line);
  flex: none;
}
.fe-panel-head__mark {
````

на:

````css
  border-bottom: 1px solid var(--line);
  flex: none;
}
/* Свёрнутая панель (спека подсистемы 2, раздел 10): всё в столбик. */
.fe-panel-head--rail {
  flex-direction: column;
  justify-content: center;
  gap: var(--space-1);
  padding: var(--space-2) 0;
}
.fe-side__toggle {
  position: absolute;
  z-index: 1;
  top: 0;
  left: 0;
  width: var(--panel-rail);
  height: var(--toolbar-height);
  display: grid;
  place-items: center;
}
.fe-panel-head__mark {
````

В `apps/web/src/ui/styles/chrome.css` заменить:

````css
  justify-content: space-between;
  min-height: 24px;
}
.fe-section__title {
````

на:

````css
  justify-content: space-between;
  min-height: 24px;
}
/* Заголовок секции сворачивает её: стрелка и название — одна кнопка. */
.fe-section__toggle {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  min-width: 0;
  height: 24px;
  margin-left: calc(-1 * var(--space-1));
  padding: 0 var(--space-1);
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: inherit;
  cursor: pointer;
}
.fe-section__toggle:hover {
  background: var(--surface-hover);
}
.fe-section__title {
````

В `apps/web/src/ui/styles/theme.css` заменить:

````css
  --toolbar-height: 44px;
  --panel-width: 248px;
  --handle-size: 8px;
  --radius-sm: 4px;
````

на:

````css
  --toolbar-height: 44px;
  --panel-width: 248px;
  /* Свёрнутая боковая панель: полоса с логотипом и кнопкой разворота. */
  --panel-rail: 48px;
  --handle-size: 8px;
  --radius-sm: 4px;
````

- [ ] **Step 4: Проверить**

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-desktop-store 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo check`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/panels.spec.ts e2e/rulers.spec.ts e2e/settings-store.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/panels.spec.ts --project=firefox`

Expected: тесты `store` без падений, `cargo check` десктопа без ошибок; модульные (`settings.test.ts` +1) и сборка — ок; E2E `panels.spec.ts` (4 теста), `rulers`, `settings-store` в Chromium, `panels.spec.ts` в Firefox — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/desktop/src-tauri/store/src/settings.rs apps/web/e2e/panels.spec.ts apps/web/e2e/rulers.spec.ts apps/web/src/core/file-list.ts apps/web/src/ui/app.tsx apps/web/src/ui/chrome.tsx apps/web/src/ui/context.ts apps/web/src/ui/icons.tsx apps/web/src/ui/missing-fonts.tsx apps/web/src/ui/properties.tsx apps/web/src/ui/settings.ts apps/web/src/ui/styles/chrome.css apps/web/src/ui/styles/theme.css apps/web/unit/settings.test.ts
git commit -F - <<'EOF'
feat(web): сворачиваемые секции, сворачиваемые боковые панели

Заголовок секции панели свойств сворачивает её, свёрнутость хранится по имени секции.
Левая и правая панели сворачиваются в полосу кнопкой в шапке, ширина и свёрнутость
запоминаются (веб и десктоп). «Экспорт…» разворачивает панель и секцию.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 6: Уведомление с «Отменить», закрытие, 4 с и пауза

Спека §10: кнопка «Отменить» у удаления, «Сплющить», «Контура обводки», разгруппировки и вставки с заменой, если шаг ещё последний; кнопка закрытия у каждого; 4 с; под курсором не исчезает.

**Files:**
- Modify: `apps/web/src/ui/app.tsx`, `apps/web/src/ui/store.ts`, `apps/web/src/ui/styles/base.css`, `apps/web/src/ui/toasts.tsx`, `crates/ferrum-editor/src/actions.rs`, `crates/ferrum-editor/src/editor.rs`, `crates/ferrum-wasm/src/core.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `crates/ferrum-wasm/src/protocol.rs`
- Test: `apps/web/e2e/toasts.spec.ts` (новый), `apps/web/unit/toasts.test.ts` (новый), `crates/ferrum-editor/tests/undo_notice.rs` (новый)

**Interfaces:**
- Consumes: `Document::last_step` (задача 1), `Editor::{command_state, emit}`, `UiStore`, `Toast`.
- Produces: `EditorEvent::UndoNotice { text, step }`, `Editor::undo_step(step) -> bool`, `Editor::notify_step` (внутри крейта); `Command::UndoStep { step }`, `CoreEvent::UndoNotice { text, step }`; `TOAST_MS = 4000`, `UiStore.{dismissToast, pauseToast, startToast}`, `Toast.undoStep?`, `Toasts { store, onUndo? }`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/e2e/toasts.spec.ts`:

````ts
// Уведомление с «Отменить» (спека подсистемы 2, раздел 10): кнопка отменяет шаг, пока он
// последний; у каждого уведомления — кнопка закрытия; 4 с; пока над ним курсор — живёт.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

const notice = (page: Page) => page.locator('.fe-toast', { hasText: 'Удалено' });

async function selectLayer(page: Page) {
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  return l;
}

test('@firefox «Отменить» возвращает удалённое, фокус остаётся на холсте', async ({ page }) => {
  await openApp(page);
  const l = await selectLayer(page);
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes - 1}`);
  await expect(notice(page)).toBeVisible();
  await notice(page).getByRole('button', { name: 'Отменить' }).click();
  await liveWhen(page, `l.nodes === ${l.nodes} && l.selectionCount === 1`);
  await expect(notice(page)).toHaveCount(0);
  // Клавиши по-прежнему доходят до холста: Delete снова удаляет.
  await expect(page.locator('canvas')).toBeFocused();
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes - 1}`);
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

test('@firefox шаг уже не последний: кнопка ничего не отменяет', async ({ page }) => {
  await openApp(page);
  const l = await selectLayer(page);
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes - 1}`);
  // Новая правка поверх: новый шаг.
  await page.keyboard.press('ControlOrMeta+KeyA');
  await page.keyboard.press('ArrowRight');
  const after = await liveWhen(page, `l.steps === ${l.steps + 2}`);
  await notice(page).getByRole('button', { name: 'Отменить' }).click();
  await expect(notice(page)).toHaveCount(0);
  // Команда дошла (следующая в очереди отвечает), шаги и узлы те же.
  await page.keyboard.press('ArrowRight');
  await liveWhen(page, `l.steps === ${after.steps + 1}`);
  expect((await live(page)).nodes).toBe(l.nodes - 1);
});

test('@firefox кнопка закрытия у любого уведомления; 4 с; пока над ним курсор — не исчезает', async ({ page }) => {
  await openApp(page);
  // Обычное уведомление без действия тоже закрывается.
  await mainMenu(page, 'Справка', 'О программе');
  const about = page.locator('.fe-toast', { hasText: 'Ferrum' });
  await expect(about.getByRole('button', { name: 'Отменить' })).toHaveCount(0);
  await about.getByRole('button', { name: 'Закрыть уведомление' }).click();
  await expect(about).toHaveCount(0);

  const l = await selectLayer(page);
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes - 1}`);
  await expect(notice(page)).toBeVisible();
  // Без курсора уходит само: и не раньше, и не сильно позже.
  const started = Date.now();
  await expect(notice(page)).toHaveCount(0, { timeout: 6000 });
  expect(Date.now() - started).toBeGreaterThan(2500);

  await selectAgain(page, l.nodes - 1);
  await page.keyboard.press('Delete');
  await expect(notice(page)).toBeVisible();
  await notice(page).hover();
  await page.waitForTimeout(4500);
  await expect(notice(page)).toBeVisible();
  await page.mouse.move(5, 5);
  await expect(notice(page)).toHaveCount(0, { timeout: 6000 });
});

async function selectAgain(page: Page, nodes: number) {
  const l = await live(page);
  const at = await appScreen(page, l, 340, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, `l.selectionCount === 1 && l.nodes === ${nodes}`);
}

test('разгруппировка и «Сплющить» из меню дают уведомление с отменой', async ({ page }) => {
  await openApp(page);
  const l = await selectLayer(page);
  await page.keyboard.press('ControlOrMeta+KeyG');
  await liveWhen(page, `l.nodes === ${l.nodes + 1}`);
  await expect(page.locator('.fe-toast')).toHaveCount(0);
  await page.keyboard.press('ControlOrMeta+Shift+KeyG');
  await liveWhen(page, `l.nodes === ${l.nodes}`);
  const t = page.locator('.fe-toast', { hasText: 'Разгруппировано' });
  await t.getByRole('button', { name: 'Отменить' }).click();
  await liveWhen(page, `l.nodes === ${l.nodes + 1}`);
  await mainMenu(page, 'Объект', 'Сплющить');
  const f = page.locator('.fe-toast', { hasText: 'Сплющено' });
  await expect(f.getByRole('button', { name: 'Отменить' })).toBeVisible();
  const steps = (await live(page)).steps;
  await f.getByRole('button', { name: 'Отменить' }).click();
  await expect(f).toHaveCount(0);
  await liveWhen(page, `l.canRedo === 1 && l.steps === ${steps}`);
});
````

Создать `apps/web/unit/toasts.test.ts`:

````ts
// Часы уведомлений (спека подсистемы 2, раздел 10): 4 с, пока над уведомлением курсор,
// не исчезает, после ухода курсора — снова 4 с.

import { expect, test } from '@playwright/test';
import { createRoot } from 'solid-js';
import { createUiStore, MAX_TOASTS, TOAST_MS } from '../src/ui/store';

/** Ручные часы вместо `setTimeout`: тест не ждёт и не зависит от машины. */
function fakeTimers() {
  const realSet = globalThis.setTimeout;
  const realClear = globalThis.clearTimeout;
  let now = 0;
  let next = 1;
  const queue = new Map<number, { at: number; f: () => void }>();
  globalThis.setTimeout = ((f: () => void, ms: number) => {
    const id = next++;
    queue.set(id, { at: now + ms, f });
    return id;
  }) as unknown as typeof setTimeout;
  globalThis.clearTimeout = ((id: number) => void queue.delete(id)) as unknown as typeof clearTimeout;
  return {
    advance(ms: number) {
      now += ms;
      for (const [id, t] of [...queue]) {
        if (t.at <= now) {
          queue.delete(id);
          t.f();
        }
      }
    },
    restore() {
      globalThis.setTimeout = realSet;
      globalThis.clearTimeout = realClear;
    },
  };
}

function withStore(run: (s: ReturnType<typeof createUiStore>, clock: ReturnType<typeof fakeTimers>) => void) {
  const clock = fakeTimers();
  try {
    createRoot((dispose) => {
      run(createUiStore(), clock);
      dispose();
    });
  } finally {
    clock.restore();
  }
}

test('уведомление живёт 4 с', () =>
  withStore((s, clock) => {
    expect(TOAST_MS).toBe(4000);
    s.toast('Привет');
    clock.advance(3999);
    expect(s.state.toasts).toHaveLength(1);
    clock.advance(1);
    expect(s.state.toasts).toHaveLength(0);
  }));

test('пока курсор над уведомлением, оно живёт; уход курсора — снова 4 с', () =>
  withStore((s, clock) => {
    s.toast('Привет');
    const id = s.state.toasts[0].id;
    clock.advance(3000);
    s.pauseToast(id);
    clock.advance(60_000);
    expect(s.state.toasts).toHaveLength(1);
    s.startToast(id);
    clock.advance(3999);
    expect(s.state.toasts).toHaveLength(1);
    clock.advance(1);
    expect(s.state.toasts).toHaveLength(0);
  }));

test('закрытие кнопкой снимает часы; соседнее не трогает', () =>
  withStore((s, clock) => {
    s.toast('Первое');
    clock.advance(1000);
    s.toast('Второе');
    const [a, b] = s.state.toasts;
    s.dismissToast(a.id);
    expect(s.state.toasts.map((t) => t.id)).toEqual([b.id]);
    clock.advance(3999);
    expect(s.state.toasts).toHaveLength(1);
    clock.advance(1);
    expect(s.state.toasts).toHaveLength(0);
  }));

test('«Отменить»: тот же текст про новый шаг заменяет прежнее, простое не дублируется', () =>
  withStore((s, clock) => {
    s.apply({ type: 'undoNotice', text: 'Удалено', step: 1 });
    clock.advance(2000);
    s.apply({ type: 'undoNotice', text: 'Удалено', step: 2 });
    expect(s.state.toasts.map((t) => t.undoStep)).toEqual([2]);
    clock.advance(2500);
    expect(s.state.toasts).toHaveLength(1);
    s.toast('Текст');
    s.toast('Текст');
    expect(s.state.toasts.filter((t) => t.text === 'Текст')).toHaveLength(1);
  }));

test('больше предела: старые уходят вместе с часами', () =>
  withStore((s, clock) => {
    for (let i = 0; i < MAX_TOASTS + 2; i++) s.toast(`т${i}`);
    expect(s.state.toasts).toHaveLength(MAX_TOASTS);
    clock.advance(TOAST_MS);
    expect(s.state.toasts).toHaveLength(0);
  }));
````

Создать `crates/ferrum-editor/tests/undo_notice.rs`:

````rust
//! Уведомление с «Отменить» (спека подсистемы 2, раздел 10): шаг удаления, «Сплющить»,
//! «Контура обводки», разгруппировки и вставки с заменой сообщается; кнопка отменяет,
//! только если он ещё последний.

mod common;

use common::*;
use ferrum_doc::{Color, NodeKind, PropsPatch, Stroke, StrokeAlign};
use ferrum_editor::*;

fn notice(ev: &[EditorEvent]) -> Option<(&'static str, u64)> {
    ev.iter().find_map(|e| match e {
        EditorEvent::UndoNotice { text, step } => Some((*text, *step)),
        _ => None,
    })
}

#[test]
fn each_listed_command_reports_its_step_and_others_do_not() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(None, 200.0, 100.0, 50.0, 50.0);
    h.ed.take_events();

    h.ed.select(&[a, b]);
    h.ed.execute(CommandId::Group);
    assert_eq!(
        notice(&h.ed.take_events()),
        None,
        "группировка — без уведомления"
    );

    h.ed.execute(CommandId::Ungroup);
    let (text, step) = notice(&h.ed.take_events()).expect("разгруппировка");
    assert_eq!(text, "Разгруппировано");
    assert_eq!(Some(step), h.ed.document().last_step());

    h.ed.select(&[a, b]);
    h.ed.execute(CommandId::Flatten);
    assert_eq!(notice(&h.ed.take_events()).unwrap().0, "Сплющено");

    let c = h.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 2.0,
                align: StrokeAlign::Center,
            }]),
            ..patch(400.0, 100.0, 50.0, 50.0)
        },
    );
    h.ed.select(&[c]);
    h.ed.take_events();
    h.ed.execute(CommandId::OutlineStroke);
    assert_eq!(
        notice(&h.ed.take_events()).unwrap().0,
        "Контур обводки создан"
    );

    let sel = h.ed.selection().to_vec();
    assert!(!sel.is_empty());
    h.ed.execute(CommandId::Delete);
    assert_eq!(notice(&h.ed.take_events()).unwrap().0, "Удалено");
}

#[test]
fn nothing_to_delete_means_no_notice() {
    let mut h = H::new();
    h.ed.take_events();
    h.ed.execute(CommandId::Delete);
    assert_eq!(notice(&h.ed.take_events()), None);
}

#[test]
fn replace_paste_reports_but_plain_paste_does_not() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a]);
    h.ed.execute(CommandId::Copy);
    let bytes =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::Clipboard(b) => Some(b),
                _ => None,
            })
            .unwrap();
    h.ed.paste(&bytes).unwrap();
    assert_eq!(notice(&h.ed.take_events()), None);
    h.ed.paste_with(&bytes, PasteMode::Replace).unwrap();
    assert_eq!(notice(&h.ed.take_events()).unwrap().0, "Заменено");
    // Без выделения это обычная вставка.
    h.ed.select(&[]);
    h.ed.take_events();
    h.ed.paste_with(&bytes, PasteMode::Replace).unwrap();
    assert_eq!(notice(&h.ed.take_events()), None);
}

#[test]
fn undo_step_only_while_it_is_last() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(None, 200.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a]);
    h.ed.take_events();
    h.ed.execute(CommandId::Delete);
    let (_, step) = notice(&h.ed.take_events()).unwrap();
    assert!(!h.ed.page_doc().contains(a));

    // Другой шаг после него: кнопка ничего не делает.
    h.set(
        b,
        PropsPatch {
            x: Some(300.0),
            ..Default::default()
        },
    );
    assert!(!h.ed.undo_step(step));
    assert!(!h.ed.page_doc().contains(a));
    assert_eq!(h.props(b).x, 300.0);
    // Отменили тот шаг сами: снова верхний — удаления, кнопка его отменяет.
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.props(b).x, 200.0);
    assert!(h.ed.undo_step(step));
    assert!(h.ed.page_doc().contains(a));
    // Уже отменён: второй раз — ничего, повтора не трогает.
    assert!(!h.ed.undo_step(step));
    assert!(h.ed.document().can_redo());
    // Неизвестный номер.
    assert!(!h.ed.undo_step(step + 1000));
}

#[test]
fn delete_inside_vector_and_text_edit_shows_no_layer_notice() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[r]);
    h.ed.execute(CommandId::EditObject);
    h.ed.take_events();
    h.click(100.0, 100.0, Modifiers::NONE);
    h.ed.execute(CommandId::Delete);
    assert_eq!(notice(&h.ed.take_events()), None, "точка вектора — не слой");
}

#[test]
fn undo_step_does_not_touch_an_open_pen_path() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a]);
    h.ed.take_events();
    h.ed.execute(CommandId::Delete);
    let (_, step) = notice(&h.ed.take_events()).unwrap();
    h.ed.execute(CommandId::ToolPen);
    h.click(300.0, 300.0, Modifiers::NONE);
    h.click(400.0, 300.0, Modifiers::NONE);
    assert!(
        !h.ed.undo_step(step),
        "отмена сняла бы точку пера, а не удаление"
    );
    assert!(!h.ed.page_doc().contains(a));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test undo_notice`
Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/toasts.test.ts`
Expected: FAIL — `undo_notice` не собирается: нет `Editor::undo_step`, `EditorEvent::UndoNotice`; модульные — 4 из 5 тестов `toasts.test.ts` падают (`TOAST_MS` равен 6000, `s.pauseToast is not a function`, `s.dismissToast is not a function`, нет `undoStep`). `e2e/toasts.spec.ts` проверяется в шаге «Проверить»

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        )}
      </Show>
      <Toasts store={store} />
    </div>
    </ErrorBoundary>
````

на:

````tsx
        )}
      </Show>
      <Toasts store={store} onUndo={(step) => editor()?.send({ type: 'undoStep', step })} />
    </div>
    </ErrorBoundary>
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  text: string;
  kind: 'error' | 'info';
}

````

на:

````ts
  text: string;
  kind: 'error' | 'info';
  /** Шаг, который отменяет кнопка «Отменить» (`undoStep`): только у `undoNotice`. */
  undoStep?: number;
}

````

В `apps/web/src/ui/store.ts` заменить:

````ts
/** Сколько уведомлений видно одновременно и сколько секунд живёт каждое. */
export const MAX_TOASTS = 3;
export const TOAST_MS = 6000;

export function createUiStore() {
````

на:

````ts
/** Сколько уведомлений видно одновременно и сколько секунд живёт каждое. */
export const MAX_TOASTS = 3;
export const TOAST_MS = 4000;

export function createUiStore() {
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  let quiet: { onDone?: () => void; onFail?: () => void } | null = null;

  function toast(text: string, kind: Toast['kind'] = 'info'): void {
    // То же сообщение подряд — одно уведомление, а не стопка.
    if (state.toasts.some((t) => t.text === text)) return;
    const id = ++toastId;
    setState('toasts', (list) => [...list, { id, text, kind }].slice(-MAX_TOASTS));
    setTimeout(() => setState('toasts', (list) => list.filter((t) => t.id !== id)), TOAST_MS);
  }

````

на:

````ts
  let quiet: { onDone?: () => void; onFail?: () => void } | null = null;

  /** Часы уведомлений; пока над уведомлением курсор, его часов нет. */
  const toastTimers = new Map<number, ReturnType<typeof setTimeout>>();

  function dismissToast(id: number): void {
    clearTimeout(toastTimers.get(id));
    toastTimers.delete(id);
    setState('toasts', (list) => list.filter((t) => t.id !== id));
  }

  /** Часы заново: уведомление живёт `TOAST_MS` с этой минуты. */
  function startToast(id: number): void {
    clearTimeout(toastTimers.get(id));
    toastTimers.set(id, setTimeout(() => dismissToast(id), TOAST_MS));
  }

  function pauseToast(id: number): void {
    clearTimeout(toastTimers.get(id));
    toastTimers.delete(id);
  }

  function addToast(item: Omit<Toast, 'id'>): void {
    const id = ++toastId;
    const list = [...state.toasts, { ...item, id }];
    // Вытесненные старые — с их часами.
    for (const t of list.slice(0, -MAX_TOASTS)) pauseToast(t.id);
    setState('toasts', list.slice(-MAX_TOASTS));
    startToast(id);
  }

  function toast(text: string, kind: Toast['kind'] = 'info'): void {
    // То же сообщение подряд — одно уведомление, а не стопка.
    if (state.toasts.some((t) => t.text === text)) return;
    addToast({ text, kind });
  }

  /** Уведомление с «Отменить»: тот же текст подряд — про новый шаг, прежнее уходит. */
  function undoToast(text: string, step: number): void {
    for (const t of state.toasts) if (t.text === text) dismissToast(t.id);
    addToast({ text, kind: 'info', undoStep: step });
  }

````

В `apps/web/src/ui/store.ts` заменить:

````ts
        for (const c of e.changed) setState('commands', c.key, { enabled: c.enabled, checked: c.checked ?? undefined });
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
````

на:

````ts
        for (const c of e.changed) setState('commands', c.key, { enabled: c.enabled, checked: c.checked ?? undefined });
        break;
      case 'undoNotice':
        undoToast(e.text, e.step);
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  }

  return { state, setState, apply, applyLive, toast, quietKeymapImport };
}

````

на:

````ts
  }

  return { state, setState, apply, applyLive, toast, dismissToast, pauseToast, startToast, quietKeymapImport };
}

````

В `apps/web/src/ui/styles/base.css` заменить:

````css
  animation: fe-pop 120ms ease-out;
}
.fe-toast--error .fe-ico {
  color: var(--danger);
````

на:

````css
  animation: fe-pop 120ms ease-out;
}
.fe-toast__action {
  flex: none;
  padding: 2px var(--space-1);
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--select, var(--ink));
  font: inherit;
  font-weight: 500;
  cursor: pointer;
}
.fe-toast__close {
  flex: none;
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
.fe-toast__action:hover,
.fe-toast__close:hover {
  background: var(--surface-hover);
}
.fe-toast--error .fe-ico {
  color: var(--danger);
````

В `apps/web/src/ui/toasts.tsx` заменить:

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
````

на:

````tsx
// Уведомления: ошибки ядра и подтверждения действий («Экспортировано: …»), у правок с
// большими последствиями — кнопка «Отменить» (спека подсистемы 2, раздел 10). Каждое
// живёт 4 с и закрывается кнопкой; пока над ним курсор или в нём фокус, не исчезает.

import { For, Show } from 'solid-js';
import { Icon } from './icons';
import type { Toast, UiStore } from './store';

export function Toasts(props: { store: UiStore; onUndo?: (step: number) => void }) {
  /** Кнопки не забирают фокус у холста щелчком: уведомление уйдёт, а клавиши должны
   * по-прежнему доходить до него. С клавиатуры фокус возвращается на холст сам. */
  const keepFocus = (e: Event) => e.preventDefault();
  const close = (t: Toast, el: HTMLElement) => {
    const inside = el.contains(document.activeElement);
    props.store.dismissToast(t.id);
    if (inside) document.querySelector<HTMLElement>('canvas')?.focus();
  };
  return (
    <div class="fe-toasts" role="status" aria-live="polite">
      <For each={props.store.state.toasts}>
        {(t) => {
          let el!: HTMLDivElement;
          return (
            <div
              ref={el}
              class={`fe-toast fe-toast--${t.kind}`}
              onPointerEnter={() => props.store.pauseToast(t.id)}
              onPointerLeave={() => {
                if (!el.contains(document.activeElement)) props.store.startToast(t.id);
              }}
              onFocusIn={() => props.store.pauseToast(t.id)}
              onFocusOut={(e) => {
                if (!el.contains(e.relatedTarget as Node | null) && !el.matches(':hover')) props.store.startToast(t.id);
              }}
            >
              <Show when={t.kind === 'error'}>
                <Icon name="warning" />
                <span class="t-strong">Ошибка:</span>
              </Show>
              <span>{t.text}</span>
              <Show when={t.undoStep !== undefined && props.onUndo}>
                <button
                  class="fe-toast__action"
                  onPointerDown={keepFocus}
                  onMouseDown={keepFocus}
                  onClick={() => {
                    props.onUndo?.(t.undoStep!);
                    close(t, el);
                  }}
                >
                  Отменить
                </button>
              </Show>
              <button
                class="fe-toast__close"
                aria-label="Закрыть уведомление"
                onPointerDown={keepFocus}
                onMouseDown={keepFocus}
                onClick={() => close(t, el)}
              >
                <Icon name="close" size={12} />
              </button>
            </div>
          );
        }}
      </For>
    </div>
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
}

impl Editor {
    pub(crate) fn run(&mut self, cmd: CommandId) {
        use CommandId as C;
        // Недоступная команда ничего не делает: ни шага, ни сообщения (спека
````

на:

````rust
}

/// Текст уведомления с кнопкой «Отменить» (спека подсистемы 2, раздел 10) у команд,
/// после которых оно положено.
fn undo_notice(cmd: CommandId) -> Option<&'static str> {
    match cmd {
        CommandId::Delete => Some("Удалено"),
        CommandId::Flatten => Some("Сплющено"),
        CommandId::OutlineStroke => Some("Контур обводки создан"),
        CommandId::Ungroup => Some("Разгруппировано"),
        _ => None,
    }
}

impl Editor {
    /// Команда; у удаления, «Сплющить», «Контура обводки» и разгруппировки её шаг
    /// сообщается интерфейсу. Правка текста и вектора шлют эти же команды своим
    /// жестам (удалить точку, символ): уведомление — только про слои.
    pub(crate) fn run(&mut self, cmd: CommandId) {
        let notice =
            undo_notice(cmd).filter(|_| self.text_edit.is_none() && self.vector_edit.is_none());
        let before = self.doc.last_step();
        self.run_command(cmd);
        if let Some(text) = notice {
            self.notify_step(text, before);
        }
    }

    /// Появился новый шаг — интерфейс показывает уведомление с его номером.
    pub(crate) fn notify_step(&mut self, text: &'static str, before: Option<u64>) {
        if let Some(step) = self.doc.last_step().filter(|s| Some(*s) != before) {
            self.out.events.push(EditorEvent::UndoNotice { text, step });
        }
    }

    /// Кнопка «Отменить» в уведомлении: отмена, только если шаг `step` ещё последний.
    /// `true` — отменён.
    pub fn undo_step(&mut self, step: u64) -> bool {
        // Открытый путь пера: отмена сняла бы его точку, а не этот шаг.
        let pen_open = self.vector_edit.as_ref().is_some_and(|v| v.pen.dirty);
        if pen_open
            || self.doc.last_step() != Some(step)
            || !self.command_state(CommandId::Undo).enabled
        {
            return false;
        }
        self.run_command(CommandId::Undo);
        self.emit();
        true
    }

    fn run_command(&mut self, cmd: CommandId) {
        use CommandId as C;
        // Недоступная команда ничего не делает: ни шага, ни сообщения (спека
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            let targets = self.replace_targets();
            if !targets.is_empty() {
                let ok = self.commit_replace(&payload, &targets);
                return self.finish_paste(&payload, ok);
            }
````

на:

````rust
            let targets = self.replace_targets();
            if !targets.is_empty() {
                let before = self.doc.last_step();
                let ok = self.commit_replace(&payload, &targets);
                self.notify_step("Заменено", before);
                return self.finish_paste(&payload, ok);
            }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Вызвана команда интерфейса (`ui.*`): её выполняет основной поток.
    UiCommand(CommandId),
    /// Команда сменила показ линеек или направляющих: интерфейс сохраняет настройку.
    ViewChanged {
````

на:

````rust
    /// Вызвана команда интерфейса (`ui.*`): её выполняет основной поток.
    UiCommand(CommandId),
    /// Шаг, у которого уведомление предлагает «Отменить» (`Editor::undo_step`).
    UndoNotice {
        text: &'static str,
        step: u64,
    },
    /// Команда сменила показ линеек или направляющих: интерфейс сохраняет настройку.
    ViewChanged {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                }
                EditorEvent::PagesChanged => self.ui.pages = true,
                EditorEvent::ViewChanged { rulers, guides } => {
                    self.events.push(CoreEvent::View { rulers, guides })
````

на:

````rust
                }
                EditorEvent::PagesChanged => self.ui.pages = true,
                EditorEvent::UndoNotice { text, step } => self.events.push(CoreEvent::UndoNotice {
                    text: text.to_string(),
                    step: step as f64,
                }),
                EditorEvent::ViewChanged { rulers, guides } => {
                    self.events.push(CoreEvent::View { rulers, guides })
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
                self.editor.set_settings(s);
            }
            Command::SetView { rulers, guides } => self.editor.set_view(rulers, guides),
            Command::SetGuides {
````

на:

````rust
                self.editor.set_settings(s);
            }
            Command::UndoStep { step } => {
                // Номер из события: только неотрицательные целые в точности f64.
                if step.is_finite() && step >= 0.0 && step.fract() == 0.0 {
                    self.editor.undo_step(step as u64);
                }
            }
            Command::SetView { rulers, guides } => self.editor.set_view(rulers, guides),
            Command::SetGuides {
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        phase: EditPhase,
    },
    /// Показ линеек и направляющих (спека подсистемы 2, разделы 9 и 11): настройка
    /// вида, не шаг истории. Нет поля — не менять.
````

на:

````rust
        phase: EditPhase,
    },
    /// Кнопка «Отменить» в уведомлении: отмена, только если шаг `step` ещё последний.
    UndoStep {
        step: f64,
    },
    /// Показ линеек и направляющих (спека подсистемы 2, разделы 9 и 11): настройка
    /// вида, не шаг истории. Нет поля — не менять.
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    RequestImage,
    ToggleUi,
    /// Команда сменила показ линеек или направляющих: интерфейс сохраняет настройку.
    View {
````

на:

````rust
    RequestImage,
    ToggleUi,
    /// Уведомление с кнопкой «Отменить» (спека подсистемы 2, раздел 10): номер шага
    /// возвращается командой `UndoStep`.
    UndoNotice {
        text: String,
        step: f64,
    },
    /// Команда сменила показ линеек или направляющих: интерфейс сохраняет настройку.
    View {
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/toasts.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/toasts.spec.ts --project=firefox`

Expected: PASS: fmt и clippy без замечаний; тесты без падений (новые: `undo_notice.rs` — 6; весь workspace — 1146 passed); модульные (`toasts.test.ts` — 5) и сборка — ок; E2E `toasts.spec.ts` (4 теста) в Chromium и Firefox — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/toasts.spec.ts apps/web/src/ui/app.tsx apps/web/src/ui/store.ts apps/web/src/ui/styles/base.css apps/web/src/ui/toasts.tsx apps/web/unit/toasts.test.ts crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/tests/undo_notice.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs
git commit -F - <<'EOF'
feat(editor,web): уведомление с «Отменить», закрытие, 4 с и пауза под курсором

Удаление, «Сплющить», «Контур обводки», разгруппировка и вставка с заменой сообщают
номер шага (`UndoNotice`), кнопка шлёт `UndoStep` — отмена, только если шаг ещё последний.
У каждого уведомления — кнопка закрытия; живёт 4 с, под курсором и с фокусом не исчезает;
щелчок по кнопкам не забирает фокус у холста.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 7: Перестановка страниц протяжкой

Спека §10: перестановка и «Дублировать страницу» — одним шагом каждая. Модель уже готова (задача 1); здесь — `Editor::move_page`, команда `MovePage` и протяжка в `pages.tsx`, E2E на оба шага (закрывает B-124).

**Files:**
- Modify: `apps/web/src/ui/pages.tsx`, `apps/web/src/ui/styles/layers.css`, `crates/ferrum-editor/src/pages.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `crates/ferrum-wasm/src/protocol.rs`
- Test: `apps/web/e2e/pages-order.spec.ts` (новый), `crates/ferrum-editor/tests/pages.rs`

**Interfaces:**
- Consumes: `Document::{move_page, last_step}`, `EditorEvent::PagesChanged` (задача 1), `Editor::begin_doc_edit`, `scheduler.measure`.
- Produces: `Editor::move_page(id, to)`; `Command::MovePage { id, index }`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/e2e/pages-order.spec.ts`:

````ts
// Порядок страниц и дубль — шаги отмены (спека подсистемы 2, раздел 10).

import { expect, test, type Page } from '@playwright/test';
import { live, liveWhen, openApp } from './helpers';

const rows = (page: Page) => page.getByRole('listbox', { name: 'Страницы' }).getByRole('option');
const selected = (page: Page) => page.getByRole('listbox', { name: 'Страницы' }).getByRole('option', { selected: true });
const names = (page: Page) => rows(page).allInnerTexts().then((list) => list.map((s) => s.trim()));

async function threePages(page: Page) {
  await openApp(page);
  const add = page.getByRole('button', { name: 'Добавить страницу' });
  await add.click();
  await add.click();
  await expect(rows(page)).toHaveCount(3);
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 2', 'Страница 3']);
}

async function dragRow(page: Page, from: number, to: number, finish = true) {
  const a = (await rows(page).nth(from).boundingBox())!;
  const b = (await rows(page).nth(to).boundingBox())!;
  await page.mouse.move(a.x + 60, a.y + a.height / 2);
  await page.mouse.down();
  // Край строки: перед строкой `to`, если тянем вверх, за ней — если вниз.
  const y = to > from ? b.y + b.height - 1 : b.y + 1;
  await page.mouse.move(a.x + 60, y, { steps: 6 });
  if (finish) await page.mouse.up();
}

test('@firefox протяжка меняет порядок одним шагом, текущая страница та же; отмена и повтор', async ({ page }) => {
  await threePages(page);
  const before = await live(page);
  const current = selected(page);
  await expect(current).toContainText('Страница 3');
  await dragRow(page, 0, 2);
  await expect.poll(() => names(page)).toEqual(['Страница 2', 'Страница 3', 'Страница 1']);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect(current).toContainText('Страница 3');
  // Щелчок, которым закончилась протяжка, не переключил страницу.
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 2', 'Страница 3']);
  await page.keyboard.press('ControlOrMeta+Shift+KeyZ');
  await expect.poll(() => names(page)).toEqual(['Страница 2', 'Страница 3', 'Страница 1']);
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

test('@firefox вверх, Esc и обратно на место — без шага; короткое нажатие переключает страницу', async ({ page }) => {
  await threePages(page);
  const before = await live(page);
  // Esc посреди жеста.
  await dragRow(page, 2, 0, false);
  await expect(page.locator('.fe-page--drop-before')).toHaveCount(1);
  await page.keyboard.press('Escape');
  await expect(page.locator('.fe-page--drop-before')).toHaveCount(0);
  await page.mouse.up();
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 2', 'Страница 3']);
  // На своё же место.
  await dragRow(page, 1, 1);
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 2', 'Страница 3']);
  expect((await live(page)).steps).toBe(before.steps);
  // Вверх.
  await dragRow(page, 2, 0);
  await expect.poll(() => names(page)).toEqual(['Страница 3', 'Страница 1', 'Страница 2']);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await rows(page).nth(1).click();
  await expect(selected(page)).toContainText('Страница 1');
});

test('@firefox порядок переживает перезагрузку; Alt+стрелка с клавиатуры', async ({ page }) => {
  await threePages(page);
  const row = selected(page);
  await row.focus();
  await page.keyboard.press('Alt+ArrowUp');
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 3', 'Страница 2']);
  // Фокус — за строкой: ещё раз вверх двигает ту же страницу.
  await page.keyboard.press('Alt+ArrowUp');
  await expect.poll(() => names(page)).toEqual(['Страница 3', 'Страница 1', 'Страница 2']);
  await page.keyboard.press('Alt+ArrowDown');
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 3', 'Страница 2']);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true);
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 3', 'Страница 2']);
});

test('@firefox «Дублировать страницу» — шаг: отмена убирает копию и возвращает назад', async ({ page }) => {
  await threePages(page);
  await rows(page).nth(0).click();
  await expect(selected(page)).toContainText('Страница 1');
  // Живое состояние обновляется кадром, позже списка: страница примера — с узлами.
  const before = await liveWhen(page, 'l.nodes > 0');
  await rows(page).nth(0).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Дублировать страницу' }).click();
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 1 (копия)', 'Страница 2', 'Страница 3']);
  await expect(selected(page)).toContainText('Страница 1 (копия)');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 2', 'Страница 3']);
  await expect(selected(page)).toContainText('Страница 1');
  await page.keyboard.press('ControlOrMeta+Shift+KeyZ');
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 1 (копия)', 'Страница 2', 'Страница 3']);
  await expect(selected(page)).toContainText('Страница 1 (копия)');
  // Узлы копии на месте после повтора.
  await liveWhen(page, `l.nodes === ${before.nodes}`);
});
````

В `crates/ferrum-editor/tests/pages.rs` заменить:

````rust
    assert_eq!(h.ed.document().name(), "Лендинг");
}
````

на:

````rust
    assert_eq!(h.ed.document().name(), "Лендинг");
}

#[test]
fn move_page_is_one_step_and_keeps_the_current_page() {
    let mut h = H::new();
    let a = h.ed.page();
    let b = h.ed.create_page().unwrap();
    let c = h.ed.create_page().unwrap();
    h.ed.set_page(a).unwrap();
    let order = |h: &H| {
        h.ed.document()
            .pages()
            .iter()
            .map(|p| p.0)
            .collect::<Vec<_>>()
    };
    h.ed.take_events();
    let steps = h.ed.step_count();
    h.ed.move_page(c, 0).unwrap();
    assert_eq!(order(&h), [c, a, b]);
    assert_eq!(h.ed.page(), a, "текущая страница не меняется");
    assert_eq!(h.ed.step_count(), steps + 1);
    let ev = h.ed.take_events();
    assert!(ev.contains(&EditorEvent::PagesChanged) && ev.contains(&EditorEvent::DocumentChanged));
    // То же место: ни шага, ни событий списка.
    h.ed.move_page(c, 0).unwrap();
    assert_eq!(h.ed.step_count(), steps + 1);
    assert!(!h.ed.take_events().contains(&EditorEvent::PagesChanged));

    h.ed.execute(CommandId::Undo);
    assert_eq!(order(&h), [a, b, c]);
    assert_eq!(h.ed.page(), a);
    assert!(h.ed.take_events().contains(&EditorEvent::PagesChanged));
    h.ed.execute(CommandId::Redo);
    assert_eq!(order(&h), [c, a, b]);

    h.ed.document_mut_for_tests().force_read_only_for_tests();
    assert!(h.ed.move_page(a, 2).is_err());
    assert_eq!(order(&h), [c, a, b]);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test pages`
Expected: FAIL — `pages` не собирается: нет `Editor::move_page`. `e2e/pages-order.spec.ts` проверяется в шаге «Проверить»

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/pages.tsx` заменить:

````tsx
// Список страниц (сворачиваемый): добавить, переименовать на месте, удалить,
// переключить; контекстное меню строки (из ядра) — переименовать, дублировать,
// удалить, ссылка на страницу. Операции со страницами в историю отмены не
// попадают, поэтому удаление — только после подтверждения.

import { createSignal, Index, Show, untrack } from 'solid-js';
import type { PageInfo } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
````

на:

````tsx
// Список страниц (сворачиваемый): добавить, переименовать на месте, удалить,
// переключить, переставить протяжкой (и Alt+стрелками); контекстное меню строки (из
// ядра) — переименовать, дублировать, удалить, ссылка на страницу. Порядок и дубль —
// шаги отмены; удаление шагом не бывает, поэтому — только после подтверждения.

import { createEffect, createSignal, Index, on, onCleanup, Show, untrack } from 'solid-js';
import type { PageInfo } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
````

В `apps/web/src/ui/pages.tsx` заменить:

````tsx
    });
  };
  return (
    <section class="fe-pages" aria-label="Страницы">
````

на:

````tsx
    });
  };
  // Перестановка протяжкой. Строки одной высоты: геометрия читается один раз, в фазе
  // `measure` на старте жеста; дальше место считается из `clientY` без чтений.
  const [drop, setDrop] = createSignal<number | null>(null);
  let drag: { id: string; from: number; startY: number; top: number; rowH: number; ready: boolean; active: boolean } | null = null;
  /** Щелчок после протяжки не переключает страницу. */
  let swallowClick = false;
  const endDrag = () => {
    drag = null;
    setDrop(null);
    window.removeEventListener('keydown', onDragKey, true);
    window.removeEventListener('blur', endDrag);
  };
  /** Клавиши во время жеста глушатся (Delete спросил бы об удалении), Esc отменяет. */
  const onDragKey = (e: KeyboardEvent) => {
    if (!drag?.active) return;
    e.stopPropagation();
    e.preventDefault();
    if (e.key !== 'Escape') return;
    swallowClick = true;
    endDrag();
  };
  onCleanup(endDrag);
  // Alt+стрелка: когда придёт новый список, фокус — за переставленной строкой.
  let list: HTMLUListElement | undefined;
  let refocus: string | null = null;
  createEffect(
    on(
      () => pages().list,
      (now) => {
        const at = refocus === null ? -1 : now.findIndex((p) => p.id === refocus);
        if (at < 0) return;
        refocus = null;
        (list?.children[at] as HTMLElement | undefined)?.focus();
      },
    ),
  );
  /** Промежуток между строками под указателем: 0…число строк. */
  const slotAt = (clientY: number, d: NonNullable<typeof drag>) =>
    Math.max(0, Math.min(pages().list.length, Math.round((clientY - d.top) / d.rowH)));
  /** Промежуток → итоговое место строки: её саму из списка уже вынули. */
  const placeOf = (slot: number, from: number) => (slot > from ? slot - 1 : slot);
  const startDrag = (e: PointerEvent, id: string, from: number) => {
    // Esc мог оставить отметку без щелчка: она про прошлый жест.
    swallowClick = false;
    if (e.button !== 0 || !e.isPrimary || editableTarget(e.target) || (e.target as Element).closest('button')) return;
    if (pages().list.length < 2) return;
    const row = e.currentTarget as HTMLElement;
    const parent = row.parentElement as HTMLElement;
    const d = { id, from, startY: e.clientY, top: 0, rowH: 1, ready: false, active: false };
    drag = d;
    row.setPointerCapture(e.pointerId);
    window.addEventListener('keydown', onDragKey, true);
    window.addEventListener('blur', endDrag);
    ed.scheduler.measure(() => {
      const rect = parent.getBoundingClientRect();
      const first = parent.firstElementChild?.getBoundingClientRect();
      d.top = rect.top - parent.scrollTop;
      d.rowH = first?.height || 1;
      d.ready = true;
    });
  };
  const moveDrag = (e: PointerEvent) => {
    const d = drag;
    if (!d) return;
    if (e.buttons === 0) {
      endDrag();
      return;
    }
    if (!d.ready || (!d.active && Math.abs(e.clientY - d.startY) < 4)) return;
    d.active = true;
    setDrop(slotAt(e.clientY, d));
  };
  const finishDrag = (e: PointerEvent) => {
    const d = drag;
    if (!d) return;
    swallowClick = d.active;
    if (d.active && d.ready) {
      const to = placeOf(slotAt(e.clientY, d), d.from);
      if (to !== d.from) ed.send({ type: 'movePage', id: d.id, index: to });
    }
    endDrag();
  };
  return (
    <section class="fe-pages" aria-label="Страницы">
````

В `apps/web/src/ui/pages.tsx` заменить:

````tsx
      </div>
      <Show when={props.open}>
        <ul class="fe-pages__list" id="fe-pages-list" role="listbox" aria-label="Страницы">
          {/* Index: событие страниц присылает новые объекты, и For пересоздавал бы строки
            вместе с открытым полем имени. */}
          <Index each={pages().list}>
            {(page) => {
              const current = () => page().id === pages().current;
              let li!: HTMLLIElement;
````

на:

````tsx
      </div>
      <Show when={props.open}>
        <ul ref={list} class="fe-pages__list" id="fe-pages-list" role="listbox" aria-label="Страницы">
          {/* Index: событие страниц присылает новые объекты, и For пересоздавал бы строки
            вместе с открытым полем имени. */}
          <Index each={pages().list}>
            {(page, i) => {
              const current = () => page().id === pages().current;
              let li!: HTMLLIElement;
````

В `apps/web/src/ui/pages.tsx` заменить:

````tsx
                  ref={li}
                  class="fe-page"
                  role="option"
                  aria-selected={current()}
                  tabindex={current() ? 0 : -1}
                  onClick={() => !current() && ed.send({ type: 'setPage', id: page().id })}
                  onDblClick={() => setRenaming(page().id)}
                  onContextMenu={(e) => openMenu(page(), e)}
````

на:

````tsx
                  ref={li}
                  class="fe-page"
                  classList={{
                    'fe-page--drop-before': drop() === i,
                    'fe-page--drop-after': drop() === pages().list.length && i === pages().list.length - 1,
                  }}
                  role="option"
                  aria-selected={current()}
                  tabindex={current() ? 0 : -1}
                  onPointerDown={(e) => startDrag(e, page().id, i)}
                  onPointerMove={moveDrag}
                  onPointerUp={finishDrag}
                  onPointerCancel={endDrag}
                  onClick={() => {
                    if (swallowClick) {
                      swallowClick = false;
                      return;
                    }
                    if (!current()) ed.send({ type: 'setPage', id: page().id });
                  }}
                  onDblClick={() => setRenaming(page().id)}
                  onContextMenu={(e) => openMenu(page(), e)}
````

В `apps/web/src/ui/pages.tsx` заменить:

````tsx
                    else if (e.key === 'F2') setRenaming(page().id);
                    else if (e.key === 'Delete' || e.key === 'Backspace') askDelete(page());
                    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                      const items = Array.from((e.currentTarget.parentElement as HTMLElement).children) as HTMLElement[];
                      items[items.indexOf(e.currentTarget) + (e.key === 'ArrowDown' ? 1 : -1)]?.focus();
````

на:

````tsx
                    else if (e.key === 'F2') setRenaming(page().id);
                    else if (e.key === 'Delete' || e.key === 'Backspace') askDelete(page());
                    else if (e.altKey && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
                      // Перестановка с клавиатуры: Alt+стрелка, фокус идёт за строкой.
                      const to = i + (e.key === 'ArrowDown' ? 1 : -1);
                      if (to >= 0 && to < pages().list.length) {
                        refocus = page().id;
                        ed.send({ type: 'movePage', id: page().id, index: to });
                      }
                    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                      const items = Array.from((e.currentTarget.parentElement as HTMLElement).children) as HTMLElement[];
                      items[items.indexOf(e.currentTarget) + (e.key === 'ArrowDown' ? 1 : -1)]?.focus();
````

В `apps/web/src/ui/styles/layers.css` заменить:

````css
.fe-page:hover {
  background: var(--surface-hover);
}
.fe-page:focus-visible {
````

на:

````css
.fe-page:hover {
  background: var(--surface-hover);
}
/* Куда встанет страница при протяжке. */
.fe-page--drop-before {
  box-shadow: inset 0 2px 0 var(--select);
}
.fe-page--drop-after {
  box-shadow: inset 0 -2px 0 var(--select);
}
.fe-page:focus-visible {
````

В `crates/ferrum-editor/src/pages.rs` заменить:

````rust
//! Страницы из панели «Страницы» (этап 5): создать, дублировать, переименовать, удалить,
//! цвет фона. Как и в `ferrum-doc`, в историю отмены попадает только «Дублировать
//! страницу» (спека подсистемы 2, раздел 10), остальные операции — нет. Имена из интерфейса не доверенные: `clean_name` убирает управляющие
//! символы и ограничивает длину.

````

на:

````rust
//! Страницы из панели «Страницы» (этап 5): создать, дублировать, переименовать, удалить,
//! цвет фона, порядок. Как и в `ferrum-doc`, в историю отмены попадают «Дублировать
//! страницу» и порядок (спека подсистемы 2, раздел 10), остальные операции — нет. Имена из интерфейса не доверенные: `clean_name` убирает управляющие
//! символы и ограничивает длину.

````

В `crates/ferrum-editor/src/pages.rs` заменить:

````rust
    }

    /// Переименование; пустое имя ничего не меняет.
    pub fn rename_page(&mut self, id: PageId, name: &str) -> DocResult<()> {
````

на:

````rust
    }

    /// Перестановка в списке: страница встаёт на место `to`. Один шаг отмены (спека
    /// подсистемы 2, раздел 10); то же место — ни шага, ни правки.
    pub fn move_page(&mut self, id: PageId, to: usize) -> DocResult<()> {
        self.begin_doc_edit();
        let before = self.doc.last_step();
        self.doc
            .move_page(id, to, self.page, self.selection.to_vec())?;
        if self.doc.last_step() != before {
            self.out.steps += 1;
            self.out.events.push(EditorEvent::PagesChanged);
            self.out.events.push(EditorEvent::DocumentChanged);
        }
        self.emit();
        Ok(())
    }

    /// Переименование; пустое имя ничего не меняет.
    pub fn rename_page(&mut self, id: PageId, name: &str) -> DocResult<()> {
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
                        Ok(_) => self.touch(),
                        Err(e) => self.error(e.to_string()),
                    }
                }
````

на:

````rust
                        Ok(_) => self.touch(),
                        Err(e) => self.error(e.to_string()),
                    }
                }
                self.ui.pages = true;
            }
            Command::MovePage { id, index } => {
                if let Some(id) = self.page_id(&id) {
                    if let Err(e) = self.editor.move_page(id, index as usize) {
                        self.error(e.to_string());
                    }
                }
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        id: String,
    },
    SetPage {
        id: String,
````

на:

````rust
        id: String,
    },
    /// Перестановка страницы: встаёт на место `index` в списке; один шаг отмены.
    MovePage {
        id: String,
        index: u32,
    },
    SetPage {
        id: String,
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/pages-order.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/pages-order.spec.ts --project=firefox`

Expected: PASS: fmt и clippy без замечаний; тесты без падений (`ferrum-editor/tests/pages.rs` +1; весь workspace — 1147 passed); модульные и сборка — ок; E2E `pages-order.spec.ts` (4 теста) в Chromium и Firefox — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/pages-order.spec.ts apps/web/src/ui/pages.tsx apps/web/src/ui/styles/layers.css crates/ferrum-editor/src/pages.rs crates/ferrum-editor/tests/pages.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs
git commit -F - <<'EOF'
feat(editor,web): перестановка страниц протяжкой — шаг отмены

`MovePage`: протяжка в списке и Alt+стрелка, Esc отменяет, щелчок после протяжки не
переключает страницу. Отмена и повтор присылают новый список; «Дублировать страницу» —
шаг (B-124), E2E на оба.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 8: Имя текстового слоя

Спека §10 (закрывает B-28): новый текст называется своим содержимым, первые 40 символов первой строки, пока имя не переименовали вручную. Правка содержимого тем же шагом ведёт за собой имя.

**Files:**
- Modify: `crates/ferrum-doc/src/page.rs`, `crates/ferrum-doc/src/text.rs`, `crates/ferrum-editor/src/text_clipboard.rs`, `crates/ferrum-editor/src/text_input.rs`
- Test: `apps/web/e2e/text-edit.spec.ts`, `apps/web/e2e/text.spec.ts`, `crates/ferrum-doc/tests/text.rs`, `crates/ferrum-editor/tests/text_edit.rs`

**Interfaces:**
- Consumes: `Tx`, `TextContent`, `NodeKind::default_name`, `PageDoc::{props, set_props}`.
- Produces: `ferrum_doc::{text_layer_name(content) -> String, TEXT_NAME_CHARS = 40}`; `Tx::renaming_text` (внутри крейта).

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/text-edit.spec.ts` заменить:

````ts
  expect((await ui(page)).selectionCount).toBe(1);
  expect((await live(page)).steps).toBe(l.steps + 1);
  await expect(page.getByRole('treeitem', { name: 'Текст', exact: true })).toBeVisible();
  const at = await appScreen(page, await live(page), 100, 300);
  await expect.poll(() => darkPixels(page, at, 60), { timeout: 10_000 }).toBeGreaterThan(20);
````

на:

````ts
  expect((await ui(page)).selectionCount).toBe(1);
  expect((await live(page)).steps).toBe(l.steps + 1);
  await expect(page.getByRole('treeitem', { name: 'Привет, мир', exact: true })).toBeVisible();
  const at = await appScreen(page, await live(page), 100, 300);
  await expect.poll(() => darkPixels(page, at, 60), { timeout: 10_000 }).toBeGreaterThan(20);
````

В `apps/web/e2e/text.spec.ts` заменить:

````ts
  expect(fonts).toContain('/fonts/manifest.json');
  expect(fonts).toContain('/fonts/inter/Inter-Variable.ttf');
  // Строка слоя — «Текст» с иконкой текста.
  await expect(page.getByRole('treeitem', { name: 'Текст', exact: true })).toBeVisible();
  // Системные шрифты в Firefox не предлагаются.
  if (test.info().project.name === 'firefox') {
````

на:

````ts
  expect(fonts).toContain('/fonts/manifest.json');
  expect(fonts).toContain('/fonts/inter/Inter-Variable.ttf');
  // Строка слоя называется содержимым текста, с иконкой текста.
  await expect(page.getByRole('treeitem', { name: 'Ferrum', exact: true })).toBeVisible();
  // Системные шрифты в Firefox не предлагаются.
  if (test.info().project.name === 'firefox') {
````

В `crates/ferrum-doc/tests/text.rs` заменить:

````rust
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.kind, NodeKind::Text);
    assert_eq!(p.name, "Текст");
    assert_eq!(p.fills, vec![Fill::solid(Color::BLACK)]);
    let t = p.text.unwrap();
````

на:

````rust
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.kind, NodeKind::Text);
    assert_eq!(
        p.name, "Привет, мир",
        "новый текст называется своим содержимым"
    );
    assert_eq!(p.fills, vec![Fill::solid(Color::BLACK)]);
    let t = p.text.unwrap();
````

В `crates/ferrum-doc/tests/text.rs` заменить:

````rust
    assert_eq!((count(NodeKind::Text), count(NodeKind::Frame)), (250, 3));
}
````

на:

````rust
    assert_eq!((count(NodeKind::Text), count(NodeKind::Frame)), (250, 3));
}

// ——— Имя слоя из содержимого (спека подсистемы 2, раздел 10; B-28) ———

mod layer_name {
    use super::*;
    use ferrum_doc::{text_layer_name, Document, TextContent, TEXT_NAME_CHARS};

    fn name_of(text: &str) -> String {
        text_layer_name(&TextContent::plain(text))
    }

    #[test]
    fn first_40_chars_of_the_first_line() {
        assert_eq!(name_of("Заголовок"), "Заголовок");
        assert_eq!(name_of("Первая\nВторая"), "Первая");
        let long = "я".repeat(100);
        assert_eq!(name_of(&long).chars().count(), TEXT_NAME_CHARS);
        assert_eq!(name_of("  с пробелами \t"), "с пробелами");
        assert_eq!(name_of("a\tb"), "a b");
        // Пустая первая строка и пустой текст — имя по умолчанию.
        assert_eq!(name_of("\nвторая"), "Текст");
        assert_eq!(name_of(""), "Текст");
        assert_eq!(name_of("   "), "Текст");
    }

    #[test]
    fn first_line_spans_styled_runs() {
        use ferrum_doc::{TextAttrs, TextRun};
        let content = TextContent {
            runs: vec![
                TextRun {
                    text: "Жир".into(),
                    attrs: TextAttrs::default(),
                },
                TextRun {
                    text: "ный\nдальше\n".into(),
                    attrs: TextAttrs::default(),
                },
            ],
        };
        assert_eq!(text_layer_name(&content), "Жирный");
    }

    fn fresh() -> (Document, ferrum_doc::PageId, ferrum_doc::NodeId) {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        let id = doc
            .transact(page, vec![], |tx| {
                Ok(vec![tx.create_node(None, None, NodeKind::Text)?])
            })
            .unwrap()[0];
        doc.clear_history();
        (doc, page, id)
    }

    fn name(doc: &mut Document, page: ferrum_doc::PageId, id: ferrum_doc::NodeId) -> String {
        doc.page(page).unwrap().props(id).unwrap().name
    }

    #[test]
    fn name_follows_edits_in_the_same_undo_step() {
        let (mut doc, page, id) = fresh();
        doc.transact(page, vec![], |tx| {
            tx.insert_text(id, 0, "Привет", None)?;
            Ok(vec![id])
        })
        .unwrap();
        assert_eq!(name(&mut doc, page, id), "Привет");
        doc.transact(page, vec![], |tx| {
            tx.delete_text(id, 0..3)?;
            Ok(vec![id])
        })
        .unwrap();
        assert_eq!(name(&mut doc, page, id), "вет");
        doc.undo().unwrap();
        assert_eq!(
            name(&mut doc, page, id),
            "Привет",
            "отмена — вместе с текстом"
        );
        doc.undo().unwrap();
        assert_eq!(name(&mut doc, page, id), "Текст");
        assert!(
            !doc.can_undo(),
            "по шагу на правку, имя отдельного шага не даёт"
        );
        doc.redo().unwrap();
        assert_eq!(name(&mut doc, page, id), "Привет");
    }

    #[test]
    fn manual_name_stays() {
        let (mut doc, page, id) = fresh();
        doc.transact(page, vec![], |tx| {
            tx.insert_text(id, 0, "Привет", None)?;
            tx.set_props(
                id,
                &PropsPatch {
                    name: Some("Заголовок".into()),
                    ..Default::default()
                },
            )?;
            Ok(vec![id])
        })
        .unwrap();
        doc.transact(page, vec![], |tx| {
            tx.insert_text(id, 0, "Ещё ", None)?;
            tx.set_text_content(id, &TextContent::plain("Другое"))?;
            Ok(vec![id])
        })
        .unwrap();
        assert_eq!(name(&mut doc, page, id), "Заголовок");
    }

    #[test]
    fn edit_of_a_later_line_or_past_40_chars_makes_no_name_write() {
        let (mut doc, page, id) = fresh();
        let long = "я".repeat(50);
        doc.transact(page, vec![], |tx| {
            tx.insert_text(id, 0, &format!("{long}\nвторая"), None)?;
            Ok(vec![id])
        })
        .unwrap();
        let before = name(&mut doc, page, id);
        doc.transact(page, vec![], |tx| {
            tx.insert_text(id, 45, "ё", None)?;
            tx.insert_text(id, 52, "ё", None)?;
            Ok(vec![id])
        })
        .unwrap();
        assert_eq!(name(&mut doc, page, id), before);
    }

    #[test]
    fn name_survives_save_and_reopen_keeps_following() {
        let (mut doc, page, id) = fresh();
        doc.transact(page, vec![], |tx| {
            tx.insert_text(id, 0, "Привет", None)?;
            Ok(vec![id])
        })
        .unwrap();
        let bytes = ferrum_doc::container::encode(&doc).unwrap();
        let mut d = ferrum_doc::container::decode(&bytes).unwrap().document;
        assert_eq!(name(&mut d, page, id), "Привет");
        d.transact(page, vec![], |tx| {
            tx.insert_text(id, 6, "!", None)?;
            Ok(vec![id])
        })
        .unwrap();
        assert_eq!(
            name(&mut d, page, id),
            "Привет!",
            "признак — само имя: хранить нечего"
        );
    }
}
````

В `crates/ferrum-editor/tests/text_edit.rs` заменить:

````rust
    assert!(info(&h).draft);
}
````

на:

````rust
    assert!(info(&h).draft);
}

#[test]
fn new_text_is_named_by_its_content_until_renamed() {
    let mut h = text_h();
    h.ed.set_tool(ToolKind::Text);
    h.click(100.0, 100.0, Modifiers::NONE);
    for ch in ["А", "б"] {
        h.ed.text_input(TextInputKind::Insert, ch).unwrap();
    }
    let id = h.ed.selection()[0];
    assert_eq!(h.props(id).name, "Аб", "имя с первой же буквы");
    h.key("Escape", Modifiers::NONE);
    h.ed.rename_node(id, "Заголовок");
    h.key("Enter", Modifiers::NONE);
    h.ed.text_input(TextInputKind::Insert, "в").unwrap();
    assert_eq!(h.props(id).name, "Заголовок", "переименованный остаётся");
    h.key("Escape", Modifiers::NONE);
    // Вставка текста в пустоту — слой с именем из вставленного.
    h.ed.select(&[]);
    h.ed.paste_plain("Строка из буфера\nвторая").unwrap();
    let made = h.ed.selection()[0];
    assert_eq!(h.props(made).name, "Строка из буфера");
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc --test text`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test text_edit`
Expected: FAIL — `ferrum-doc --test text` не собирается: нет `ferrum_doc::{text_layer_name, TEXT_NAME_CHARS}`; `text_edit` собирается и падает на `new_text_is_named_by_its_content_until_renamed` (`left: "Текст", right: "Аб"`). E2E `text`, `text-edit` проверяются в шаге «Проверить»

- [ ] **Step 3: Реализация**

В `crates/ferrum-doc/src/page.rs` заменить:

````rust

    pub fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        self.page.set_props(id, patch)
    }

````

на:

````rust

    pub fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        if patch.content.is_some() && patch.name.is_none() {
            return self.renaming_text(id, || self.page.set_props(id, patch));
        }
        self.page.set_props(id, patch)
    }

    /// Правка содержимого текста тем же шагом ведёт за собой имя слоя, пока оно своё:
    /// такое, какое дало бы содержимое до правки, или имя по умолчанию. Имя, заданное
    /// вручную, отличается от них и остаётся (спека подсистемы 2, раздел 10). Отмена
    /// возвращает имя вместе с текстом: обе правки — в журнале одного шага.
    fn renaming_text(&self, id: NodeId, edit: impl FnOnce() -> DocResult<()>) -> DocResult<()> {
        let before = self.page.props(id)?;
        let auto = before.text.as_ref().is_some_and(|t| {
            before.name == before.kind.default_name()
                || before.name == crate::text::text_layer_name(&t.content)
        });
        edit()?;
        if auto {
            let after = self.page.props(id)?;
            if let Some(t) = &after.text {
                let name = crate::text::text_layer_name(&t.content);
                if name != after.name {
                    self.page.set_props(
                        id,
                        &PropsPatch {
                            name: Some(name),
                            ..Default::default()
                        },
                    )?;
                }
            }
        }
        Ok(())
    }

````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        attrs: Option<&TextAttrs>,
    ) -> DocResult<()> {
        self.page.insert_text(id, at, text, attrs)
    }

    /// Удаление символов текста (последний перевод строки не удаляется).
    pub fn delete_text(&self, id: NodeId, range: Range<usize>) -> DocResult<()> {
        self.page.delete_text(id, range)
    }

````

на:

````rust
        attrs: Option<&TextAttrs>,
    ) -> DocResult<()> {
        self.renaming_text(id, || self.page.insert_text(id, at, text, attrs))
    }

    /// Удаление символов текста (последний перевод строки не удаляется).
    pub fn delete_text(&self, id: NodeId, range: Range<usize>) -> DocResult<()> {
        self.renaming_text(id, || self.page.delete_text(id, range))
    }

````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    /// Содержимое текста целиком.
    pub fn set_text_content(&self, id: NodeId, content: &TextContent) -> DocResult<()> {
        self.page.set_text_content(id, content)
    }

    /// Вставка участков с их разметкой в позицию `at` (символы Unicode).
    pub fn insert_runs(&self, id: NodeId, at: usize, runs: &[TextRun]) -> DocResult<()> {
        self.page.insert_runs(id, at, runs)
    }
}
````

на:

````rust
    /// Содержимое текста целиком.
    pub fn set_text_content(&self, id: NodeId, content: &TextContent) -> DocResult<()> {
        self.renaming_text(id, || self.page.set_text_content(id, content))
    }

    /// Вставка участков с их разметкой в позицию `at` (символы Unicode).
    pub fn insert_runs(&self, id: NodeId, at: usize, runs: &[TextRun]) -> DocResult<()> {
        self.renaming_text(id, || self.page.insert_runs(id, at, runs))
    }
}
````

В `crates/ferrum-doc/src/text.rs` заменить:

````rust
}

impl TextContent {
    /// Текст без разметки; перевод строки в конце добавляется, если его нет.
````

на:

````rust
}

/// Сколько символов первой строки текста идёт в имя его слоя.
pub const TEXT_NAME_CHARS: usize = 40;

/// Имя текстового слоя по содержимому (спека подсистемы 2, раздел 10), как в Figma:
/// первые 40 символов первой строки без пробелов по краям; пустая строка — имя по
/// умолчанию. Обход останавливается на границе: стоимость не зависит от длины текста.
pub fn text_layer_name(content: &TextContent) -> String {
    let mut name = String::new();
    let mut taken = 0;
    'runs: for run in &content.runs {
        for c in run.text.chars() {
            if c == '\n' || taken == TEXT_NAME_CHARS {
                break 'runs;
            }
            // Табуляция и прочие управляющие — как в именах из поля ввода (`clean_name`).
            name.push(if c.is_control() { ' ' } else { c });
            taken += 1;
        }
    }
    let name = name.trim();
    if name.is_empty() {
        crate::types::NodeKind::Text.default_name().to_string()
    } else {
        name.to_string()
    }
}

impl TextContent {
    /// Текст без разметки; перевод строки в конце добавляется, если его нет.
````

В `crates/ferrum-editor/src/text_clipboard.rs` заменить:

````rust
            (p.w, p.h) = (w, h);
        }
        let (target, target_world, target_box) = self.paste_target();
        let center = target_box.map_or(self.view_center(), |b| b.center());
````

на:

````rust
            (p.w, p.h) = (w, h);
        }
        // Новый текст называется своим содержимым (раздел 10 спеки подсистемы 2).
        if let Some(t) = &p.text {
            p.name = ferrum_doc::text_layer_name(&t.content);
        }
        let (target, target_world, target_box) = self.paste_target();
        let center = target_box.map_or(self.view_center(), |b| b.center());
````

В `crates/ferrum-editor/src/text_input.rs` заменить:

````rust
                    let mut props = after.clone();
                    fit.apply_to(&mut props);
                    let id = tx.create_with_props(d.parent, None, &props)?;
                    Ok(vec![id])
````

на:

````rust
                    let mut props = after.clone();
                    fit.apply_to(&mut props);
                    // Новый текст называется своим содержимым (раздел 10 спеки подсистемы 2).
                    if let Some(t) = &props.text {
                        props.name = ferrum_doc::text_layer_name(&t.content);
                    }
                    let id = tx.create_with_props(d.parent, None, &props)?;
                    Ok(vec![id])
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/text.spec.ts e2e/text-edit.spec.ts --project=chromium`

Expected: PASS: fmt и clippy без замечаний; тесты без падений (`ferrum-doc/tests/text.rs` +6, `ferrum-editor/tests/text_edit.rs` +1; весь workspace — 1154 passed); wasm собран, typecheck без ошибок; E2E `text`, `text-edit` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/text-edit.spec.ts apps/web/e2e/text.spec.ts crates/ferrum-doc/src/page.rs crates/ferrum-doc/src/text.rs crates/ferrum-doc/tests/text.rs crates/ferrum-editor/src/text_clipboard.rs crates/ferrum-editor/src/text_input.rs crates/ferrum-editor/tests/text_edit.rs
git commit -F - <<'EOF'
feat(doc,editor): имя текстового слоя — первые 40 символов первой строки

Правка содержимого тем же шагом ведёт за собой имя, пока оно своё (B-28): имя по
умолчанию или то, что дало бы содержимое до правки. Переименованное вручную остаётся,
признак хранить не нужно. Новый текст с инструмента T и вставка обычного текста — тоже.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 9: Спека, бэклог и финальная проверка

Спека §12: тесты и замеры. Здесь — «Сделано (этап 4)» в разделах 9, 10, 11, 12 спеки оболочки; в `docs/backlog.md` — раздел «Оболочка (этап 4 подсистемы 2)» (B-161…B-177), закрыты B-28, B-121, B-124. Шаг «Проверить» — все ворота этапа.

**Files:**
- Modify: `docs/backlog.md`, `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md`

**Interfaces:**
- Consumes: результаты задач 1–8 (числа замеров — из своего прогона отчётов).
- Produces: документы; кода нет.

- [ ] **Step 1: Реализация**

В `docs/backlog.md` заменить:

````markdown
| B-26 | Изгиб сегмента ломает гладкость у зеркальных точек и ограничивает параметр [0,1; 0,9]. Точка на сегменте при t = 0 или 1 создаёт сегмент нулевой длины. | ferrum-editor `vector_ops` | P3 | этап 3, ревью задачи 4 |
| B-27 | Правила ресайза Figma для текста (горизонтальный ресайз `AutoWidth` → `AutoHeight`): сейчас любой ресайз делает текст `Fixed`. | ferrum-editor | P2 | этап 1 подсистемы 3 |
| B-28 | Имя текстового слоя не берётся из его текста: в Figma слой называется своим текстом. | ferrum-editor | P3 | этап 2 подсистемы 3 |
| B-29 | Запасных шрифтов для отдельных символов (CJK, эмодзи) нет: вместо глифа пустое место. | ferrum-text | P2 | этап 1 подсистемы 3 |
| B-30 | Текст справа налево, вертикальный текст, Shift+Enter. `truncate` и `maxLines` есть только в модели. | ferrum-text, apps/web | P3 | этап 2 подсистемы 3 |
````

на:

````markdown
| B-26 | Изгиб сегмента ломает гладкость у зеркальных точек и ограничивает параметр [0,1; 0,9]. Точка на сегменте при t = 0 или 1 создаёт сегмент нулевой длины. | ferrum-editor `vector_ops` | P3 | этап 3, ревью задачи 4 |
| B-27 | Правила ресайза Figma для текста (горизонтальный ресайз `AutoWidth` → `AutoHeight`): сейчас любой ресайз делает текст `Fixed`. | ferrum-editor | P2 | этап 1 подсистемы 3 |
| B-29 | Запасных шрифтов для отдельных символов (CJK, эмодзи) нет: вместо глифа пустое место. | ferrum-text | P2 | этап 1 подсистемы 3 |
| B-30 | Текст справа налево, вертикальный текст, Shift+Enter. `truncate` и `maxLines` есть только в модели. | ferrum-text, apps/web | P3 | этап 2 подсистемы 3 |
````

В `docs/backlog.md` заменить:

````markdown
|---|---|---|---|
| B-120 | Меню холста для текста без подменю недавних шрифтов (§5, блок 7): списка недавних шрифтов в ядре нет. Появится вместе с ним; «Войти в правку» у текста в меню тоже не показывается (только у вектора и фигур). | P2 | подсистема 2, этап 2 |
| B-121 | Меню пустого места холста без «Линейки» (§5): команды нет до этапа 4. | P2 | подсистема 2, этап 2 |
| B-122 | «Вставить поверх» и «Вставить с заменой» для картинки из другого приложения и обычного текста — обычная вставка (способ вставки учитывают только данные Ferrum). | P2 | подсистема 2, этап 2 |
| B-123 | «Вставить свойства»: эффектов и режима наложения в модели нет — не переносятся; стиль текста — первого участка источника. | P2 | подсистема 2, этап 2 |
| B-124 | «Дублировать страницу» из меню страницы — не шаг отмены, как остальные операции со страницами; шагом — на этапе 4 (§10). Копия встаёт сразу после страницы и становится текущей. | P2 | подсистема 2, этап 2 |
| B-125 | Ctrl/Cmd+Shift+R («Вставить с заменой») перехватывается страницей и перезагрузки нет — проверено E2E в Chromium и Firefox (сама вставка из буфера — только Chromium, B-131); Safari/WebKit не проверен. Простое Mod+R остаётся перезагрузкой браузера. | P3 | подсистема 2, этап 2 |
| B-126 | «Выделить слой» из контекстного меню выделяет как панель слоёв (`Select`): панель не раскрывает предков и не прокручивает к узлу. | P3 | подсистема 2, этап 2 |
````

на:

````markdown
|---|---|---|---|
| B-120 | Меню холста для текста без подменю недавних шрифтов (§5, блок 7): списка недавних шрифтов в ядре нет. Появится вместе с ним; «Войти в правку» у текста в меню тоже не показывается (только у вектора и фигур). | P2 | подсистема 2, этап 2 |
| B-122 | «Вставить поверх» и «Вставить с заменой» для картинки из другого приложения и обычного текста — обычная вставка (способ вставки учитывают только данные Ferrum). | P2 | подсистема 2, этап 2 |
| B-123 | «Вставить свойства»: эффектов и режима наложения в модели нет — не переносятся; стиль текста — первого участка источника. | P2 | подсистема 2, этап 2 |
| B-125 | Ctrl/Cmd+Shift+R («Вставить с заменой») перехватывается страницей и перезагрузки нет — проверено E2E в Chromium и Firefox (сама вставка из буфера — только Chromium, B-131); Safari/WebKit не проверен. Простое Mod+R остаётся перезагрузкой браузера. | P3 | подсистема 2, этап 2 |
| B-126 | «Выделить слой» из контекстного меню выделяет как панель слоёв (`Select`): панель не раскрывает предков и не прокручивает к узлу. | P3 | подсистема 2, этап 2 |
````

В `docs/backlog.md` заменить:

````markdown
| B-160 | Высота списка палитры скачет, пока приходит ответ по слоям (строки слоёв добавляются после команд). Зарезервировать высоту или не показывать раздел до ответа. | P3 | этап 3, перепроверка задачи 5 |

## Код

````

на:

````markdown
| B-160 | Высота списка палитры скачет, пока приходит ответ по слоям (строки слоёв добавляются после команд). Зарезервировать высоту или не показывать раздел до ответа. | P3 | этап 3, перепроверка задачи 5 |

## Оболочка (этап 4 подсистемы 2)

Закрыты: B-28 (имя текста), B-121 («Линейки» в меню пустого места), B-124 («Дублировать страницу» — шаг).

| № | Что | Пр. | Источник |
|---|---|---|---|
| B-161 | Направляющая не выделяется: нет удаления клавишей Delete и пункта «Убрать направляющую» в контекстном меню, как в Figma. Удаление — только утаскиванием на линейку. | P2 | этап 4, прототип |
| B-162 | Нет копии линии с Alt при протяжке и измерений расстояний от тянущейся линии до фрейма и его узлов (справка Figma). | P3 | этап 4, прототип |
| B-163 | Сочетание Shift+G — не подтверждённое для направляющих линеек: в справке Figma оно у «Layout guides». Подсистема 5 (сетки раскладки) должна решить, делят ли они его. | P2 | этап 4, прототип |
| B-164 | Линии повёрнутого фрейма рисуются, но не ловятся и не привязывают; за них не взяться и удалить их можно только вместе с фреймом. Новая линия над повёрнутым фреймом уходит странице. Линии вложенного фрейма (его вложили в другой) не показаны и недоступны. | P3 | этап 4, прототип |
| B-165 | Правка вектора и перо не привязываются к направляющим: у них своя привязка (пиксели). Инструмент «Текст» — тоже. | P3 | этап 4, прототип |
| B-166 | Над линейкой правка текста и вектора показывает курсор по умолчанию и нажатие не кончает правку; в Figma поведение не сверено. | P3 | этап 4, прототип |
| B-167 | Нажатие внутри выделенного узла важнее линии только при выделении до 256 узлов: дальше обход на каждое движение не окупается, и линия перехватывает нажатие. | P3 | этап 4, прототип |
| B-168 | Автоимя текста без признака: переименование в точности в текущий текст или в «Текст» снова включает слежение. Настоящий признак — поле модели (`autoRename` в Figma). | P3 | этап 4, прототип |
| B-169 | Кнопка «Отменить» в уведомлении не гаснет, когда шаг перестал быть последним: нажатие просто ничего не делает. Кнопка шлёт команду, и ядро проверяет её без ответа интерфейсу. | P3 | этап 4, прототип |
| B-170 | Перестановка страниц в документе только для чтения начинается и заканчивается сообщением об ошибке: интерфейс не знает о режиме до команды. Перетаскивание без автопрокрутки длинного списка страниц. | P3 | этап 4, прототип |
| B-171 | Отмена перестановки и дубля переключает на страницу шага и возвращает выделение на момент шага, даже когда пользователь ушёл на другую. Отмена шага перестановки чужой страницы тоже переключает. Сверить с Figma. | P3 | этап 4, прототип |
| B-172 | Операции «Новая страница», «Переименовать», «Удалить страницу» и фон по-прежнему не шаги отмены; индекс в шаге перестановки сдвигается, если между шагом и отменой список менялся не шагами (страницу ставит на тот же номер). | P3 | этап 4, прототип |
| B-173 | Свёрнутость панелей и секций у каждой вкладки своя, между вкладками на лету не передаётся (как ширина). Свёрнутая секция размонтирована: развёрнутая заново заново читает значения, открытые в ней поля теряют ввод. | P3 | этап 4, прототип |
| B-174 | Нет сочетания для сворачивания боковых панелей по одной: Ctrl/Cmd+\ прячет весь интерфейс. В Figma различаются «Minimize UI» и «Show/Hide UI» — сверить. | P3 | этап 4, прототип |
| B-175 | Ширина панели с клавиатуры: быстрые нажатия стрелки считают от прежней ширины до следующего кадра (`scheduler.mutate`) — шаг теряется. Было и до этапа. | P3 | этап 4, прототип |
| B-177 | Уведомления теперь живут 4 с, а не 6: E2E `files.spec` «вторая вкладка» под нагрузкой (прогон 40 мин вместо 8,5) не застал уведомление при первой попытке, повтор прошёл. Если повторится — ждать его не по локатору, а по журналу событий. | P3 | этап 4, прототип |
| B-176 | E2E: `@firefox` нет у разгруппировки и «Сплющить» с уведомлением и у удержания Shift+R; WebKit и настоящая GPU не проверялись (`render_report` мерит линейки на CPU, в wasm не мерились). | P3 | этап 4, прототип |

## Код

````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
  - Привязка: к направляющим, как к краям узлов, 4 экранных пикселя (ядро §7.3).
  - Показ и скрытие направляющих — пункт «Вид → Направляющие» и сочетание из справки Figma. Точное сочетание сверяется со справкой Figma в плане этапа 4 и проверяется на конфликт с раскладкой.
- **Совместимость.** Файл с направляющими открывается старой сборкой как обычно: неизвестный ключ просто не читается. Проверка — `scripts/compat-old-build.sh <ref> guides_fixture`.

## 10. Полировка панелей (этап 4)
````

на:

````markdown
  - Привязка: к направляющим, как к краям узлов, 4 экранных пикселя (ядро §7.3).
  - Показ и скрытие направляющих — пункт «Вид → Направляющие» и сочетание из справки Figma. Точное сочетание сверяется со справкой Figma в плане этапа 4 и проверяется на конфликт с раскладкой.
- **Совместимость.** Файл с направляющими открывается старой сборкой как обычно: неизвестный ключ просто не читается. Проверка — `scripts/compat-old-build.sh <ref> guides_fixture`: файл открыт для записи, виден целиком, а пересохранённый старой сборкой снова читается новой с теми же линиями.
- **Сделано (этап 4):**
  - Модель: `ferrum_doc::Guide { axis, offset }`; ключ `guides` — неизменяемое значение-список, заменяется целиком, как заливки; нет направляющих — ключа нет. У страницы — в её узле дерева страниц (корневой документ, как фон: читается без загрузки страницы), смещение от нуля страницы; у фрейма — `Props::guides`, смещение от его левого верхнего угла, у остальных узлов ключ не читается и не пишется. Чтение из файла отбрасывает поодиночке неверную ось, нечисловое смещение, |offset| > 1e9 и повторы, останавливается на 1000; документ остаётся доступным для записи. Правка сверх пределов — отказ целиком (`DocError::InvalidGuides`), ни шага, ни следов. Кэш сцены — версия 6: неверная строка отбрасывает кэш, значение читается из Loro. В буфере обмена — необязательное поле `guides` у фрейма; копия и дубликат фрейма сохраняют линии, неверные — отказ вставки.
  - **(поведение)** Как в Figma ([справка: «Users with view access can toggle Rulers on and off to show/hide Guides»](https://help.figma.com/hc/en-us/articles/360040449713-Add-guides-to-the-canvas-or-frames)), направляющие видны, только пока включены линейки: без линеек нет ни линий, ни привязки, а «Вид → Направляющие» серый. Ctrl/Cmd+\ прячет линейки вместе с интерфейсом, линии остаются.
  - **(поведение)** Сочетание показа и скрытия — Shift+G. Отдельного сочетания у направляющих линеек в справке Figma не нашлось: там их прячет Shift+R, а Shift+G — [«Layout guides»](https://help.figma.com/hc/en-us/articles/360040450513-Create-layout-guides), то есть сетки раскладки, которых в Ferrum до подсистемы 5 нет. Это ближайшее по смыслу сочетание Figma, не подтверждённая привязка к этой команде; конфликта в раскладке нет (тест `keymap`).
  - Линейки — 20 CSS px сверху и слева, поверх холста, последний слой оверлея (`RulersOverlay`). Шаг подписей 1, 2, 5 × 10ⁿ единиц мира, не теснее 72 px и не чаще единицы мира; мелкие деления — кратные, не чаще 6 px; подписи левой линейки идут снизу вверх. Полоса — общая рамка выделения. **(поведение)** Начало отсчёта — левый верхний угол фрейма верхнего уровня, если всё выделение под одним таким фреймом без поворота (и если выделен он сам), иначе ноль страницы.
  - **(поведение)** Новая линия: протяжка из верхней линейки — горизонтальная, из левой — вертикальная. Владелец — при отпускании: верхний видимый, не заблокированный и не повёрнутый фрейм верхнего уровня под указателем, иначе страница. Линия фрейма — от края до края рамки и едет с ним; страницы — через весь холст. Вложенный фрейм линий не показывает (данные остаются, если его вынести наверх). Перенос линии из страницы во фрейм и обратно — один шаг.
  - **(поведение)** За линию можно взяться в «Перемещении» без модификаторов, допуск 3 px. Ручки рамки выделения важнее линии, нажатие внутри выделенного узла (кроме фрейма) двигает узел; линия выделенного фрейма остаётся доступной. Щелчок по линии без сдвига (порог 3 px) достаётся тому, что под ней, как обычный щелчок: выбор узла и двойной щелчок работают. Над линией — курсор протяжки, без наведения на узел; над линейкой инструмент указателя не видит, щелчок по ней не снимает выделение и не начинает рисование. Правка текста и вектора нажатие на линейку забирает и не заканчивается.
  - **(поведение)** Тянущаяся линия привязывается к узлам (соседи во фрейме под ней или на странице) и другим линиям, иначе — к целым единицам мира при «Привязке к пикселям»; плашка показывает значение от начала отсчёта линеек. На своей линейке линии нет, отпускание удаляет её; новую — не создаёт. Esc, потеря фокуса и потерянное отпускание — отмена без следов.
  - Привязка: линии — цели в `Snapper` наравне с краями узлов, порог тот же (4 экранных px), у перетаскивания, ресайза и рисования. Линия фрейма действует только напротив него и не держит сам фрейм и его потомков. Косые линии повёрнутого фрейма — без попадания и привязки.
  - Настройка `rulers`, `guides` — в настройках (веб и файл десктопа), `SetView` при запуске и из другой вкладки. Удержание клавиши не мигает никаким переключателем.

## 10. Полировка панелей (этап 4)
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
- **Страницы.** Перестановка протяжкой в списке и «Дублировать страницу» — одним шагом каждая.
- **Имя текстового слоя** **(поведение)**, как в Figma: новый текст называется своим содержимым, первые 40 символов первой строки, пока имя не переименовали вручную. Закрывает бэклог B-28.

## 11. Протокол (дополнения к ядру §4.3)
````

на:

````markdown
- **Страницы.** Перестановка протяжкой в списке и «Дублировать страницу» — одним шагом каждая.
- **Имя текстового слоя** **(поведение)**, как в Figma: новый текст называется своим содержимым, первые 40 символов первой строки, пока имя не переименовали вручную. Закрывает бэклог B-28.
- **Сделано (этап 4):**
  - **(поведение)** Весь заголовок секции (стрелка и название) — кнопка; свёрнутость — настройка `collapsedSections`, имена через перевод строки, общая для всех выделений. Имя стабильно, когда заголовок меняется: у «Фрейм» и «Положение» — «Положение». Кнопка «+» секции остаётся и у свёрнутой. «Экспорт…» разворачивает свою панель и секцию и ставит фокус на кнопку.
  - **(поведение)** Кнопка в шапке есть у обеих панелей. Свёрнутая панель — полоса 48 px: у левой остаются логотип с главным меню и кнопка разворота, у правой — кнопка. Содержимое не смонтировано, холст занимает освободившееся место. Настройки — `leftCollapsed`, `rightCollapsed`, ширина прежняя. Фокус с кнопки не теряется.
  - Уведомления: живут 4 с (все, раньше — 6), кнопка закрытия у каждого; под курсором или с фокусом внутри часов нет, после ухода — снова 4 с. «Отменить» — у удаления («Удалено»), «Сплющить», «Контура обводки», разгруппировки и вставки с заменой, только если правка создала шаг и не в правке текста или вектора. Ядро сообщает номер шага (`UndoNotice`), кнопка шлёт `UndoStep`; не последний шаг — ничего, тот же номер после отмены и повтора снова годится. Тот же текст про новый шаг заменяет прежнее уведомление. Щелчок по кнопкам не забирает фокус с холста; с клавиатуры фокус возвращается на холст.
  - Страницы: `MovePage` — протяжка в списке (порог 4 px, линия показывает место, Esc и потеря фокуса — отмена, клавиши на время жеста глушатся, щелчок после протяжки не переключает страницу) и **(поведение)** Alt+стрелка, фокус идёт за строкой. Текущая страница не меняется, то же место — ни шага, ни правки. Шаги хранят операции над корнем (`DocOp`). Отмена дубля уводит копию из списка в `Document::parked`, пока есть повтор; повтор возвращает её под новым `PageId` (дерево страниц Loro не восстанавливает удалённый узел) и переписывает на него шаги повтора. Отмена возвращает на страницу, с которой дублировали, повтор — на копию; страница, удалённая позже, забирает шаги, которые на неё опирались. Копия — с фоном и направляющими.
  - **(поведение)** Имя текста: правка содержимого тем же шагом ведёт за собой имя, пока оно имя по умолчанию или то, что дало бы содержимое до правки; признак не хранится, отмена возвращает его вместе с текстом. Начальные и конечные пробелы убираются, управляющие символы — в пробел, пустая первая строка — «Текст». Переименование в точности в текст слоя снова включает слежение (как автоимя в Figma — B-168). Вставка обычного текста как нового слоя — тоже по содержимому; копия слоя сохраняет своё имя.

## 11. Протокол (дополнения к ядру §4.3)
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
  - `RevealLayer { id }`: неверный формат — `Error`, слоя нет на текущей странице — `Error`; выбор — как с холста (предки раскрываются, панель прокручивается — и у уже выделенного слоя), не шаг истории; у слоя, которого не видно (скрыт или лежит в скрытом фрейме), камера не двигается.

## 12. Тестирование

````

на:

````markdown
  - `RevealLayer { id }`: неверный формат — `Error`, слоя нет на текущей странице — `Error`; выбор — как с холста (предки раскрываются, панель прокручивается — и у уже выделенного слоя), не шаг истории; у слоя, которого не видно (скрыт или лежит в скрытом фрейме), камера не двигается.

- **Сделано (этап 4):**
  - `SetGuides { owner, guides, phase }`: `owner` — `{ kind: "page" | "frame", id }`, `guides` — `[{ axis: "x" | "y", offset }]`, `phase` — `preview | commit | cancel`. Фрейм — только верхнего уровня на текущей странице, превью — только на текущей странице; страница может быть не загружена — сначала загрузка, затем проверка «только для чтения». Сверх 1000, нечисловое или |offset| > 1e9 — `Error`, без шага и без превью. Протяжка на холсте идёт внутри ядра по событиям указателя, этой командой не пользуется.
  - `SetView { rulers?, guides? }` и событие `View { rulers, guides }`: команда `view.rulers` или `view.guides` сообщает новую настройку, `SetView` — нет (иначе запуск затёр бы сохранённую).
  - `UndoStep { step }` и событие `UndoNotice { text, step }`; `MovePage { id, index }`. Таблица — 88 команд.

## 12. Тестирование

````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
- **Сделано (этап 2):** модульные тесты ядра — `context_menu` (выбор на правой кнопке: невыделенный, выделенный, пустое место в рамке и вне её; порядок блоков холста; «Выделить слой» сверху вниз с контейнерами; «Войти в правку» только у фигур; меню слоя и страницы, их доступность в документе только для чтения; неизвестная цель; правка вектора и текста не кончается; «Дублировать страницу» с новыми id, фоном и местом в списке); `ferrum-doc` — `duplicating_an_unopened_page_with_unknown_nodes_is_refused`, `paste_modes` (повтор +10, сброс новой копией, центр вида; поверх; замена с местом в слоях и родителем — один шаг; заблокированные и без выделения; состояния и `PasteRequested` с режимом), `props_clip` (что подходит каждому типу — один шаг; текст с меткой — в другой документ; свойства копии узлов; ошибки без шага; стиль текста с заливкой и подгонкой рамки — один шаг; вставка из копии узлов добавляет в документ только картинки перенесённых заливок; копия с текста берёт собственную заливку первого участка, вставка заливки снимает заливки участков тем же шагом; состояния), `smart_duplicate` (сдвиг протяжкой и стрелками повторяется, сброс выбором и отменой дубликата, несколько узлов; «Выделить подобные»: тип, заливки, обводки, скрытые, образцы, контейнер без потомков), `journal` («Дублировать страницу» переживает журнал и полное сохранение); ядро worker'а — `context_menu` (меню последним после выделения и состояний, ошибки цели, слой без прокрутки, «Развернуть всё», дубль страницы), `paste` (режимы в `PasteRequested`, замена; свойства текстом и из внутреннего буфера), `protocol` (новые сообщения, имена полей, `Paste` без `mode`). Замер `editor_report`: «Выделить подобные, 100 000 узлов» — 1,3 мс (цель ≤ 100 мс; нативно, release), выделено 1000 двойников. E2E: `context-menu.spec.ts` (Chromium и Firefox) — холст: выбор, место меню, состав, «Выделить слой», команда — шаг; пустое место; слои: выбор строки, «Развернуть всё», «Свернуть всё», «Переименовать»; страницы: серое «Удалить», «Дублировать», ссылка на вторую страницу открывает её, а не первую; правый щелчок в поле переименования слоя и страницы не открывает меню ядра; «Копировать ссылку» без буфера обмена — уведомление об ошибке; `__ferrumWarnings` пуст. `paste.spec.ts` — повтор +10 и Ctrl/Cmd+Shift+V, свойства Ctrl/Cmd+Alt+C и V (оба браузера); замена и поверх из меню, Ctrl/Cmd+Shift+R на холсте не перезагружает страницу (оба браузера), «Вставить с заменой» им одним шагом (Chromium), копия свойств в системном буфере (только Chromium, B-131). `duplicate.spec.ts` — умный дубликат стрелками и протяжкой, «Выделить подобные» из меню и сочетанием. `chrome.spec`, `clipboard.spec`, `main-menu.spec` — под новый состав меню.
- **Сделано (этап 3):** модульные тесты ядра — `find_layers` редактора (подпоследовательность и начала слов, регистр, «ё» и другая раскладка, порядок: лучшие первыми, при равном весе — в порядке панели, пределы; длинные имена ищутся по началу и возвращаются целиком; запрос длиннее предела обрезается; отмена и правка имён обновляют индекс, превью имени тоже; эталон по определению весов на случайных текстах до 70 символов многих алфавитов — `fast_scoring_matches_the_definition` (убивает мутантов в оценке) и сравнение лучших с полной сортировкой; общие векторы весов с интерфейсом — `score_vectors_shared_with_the_interface`; границы таблицы свёртки по всем символам Unicode — `fast_fold_paths_match_unicode`; переход: виден, влезает, не влезает, не шаг истории, слоя нет; скрытый слой и видимый ребёнок скрытого фрейма выделяются без движения камеры), `find_layers` и `protocol` ядра worker'а (`LayersFound` со своим запросом, пределы, переход раскрывает предков и прокручивает панель, в том числе у уже выделенного слоя; ошибки цели, имена полей), `command_table` и `keymap` (новые сочетания на обеих платформах); модульные интерфейса (169 прошли) — `fuzzy`, `palette` (состав списка, 50 строк, недавние, обрезка запроса), `settings` (`recentCommands`), `main-menu`; `store` десктопа — поле настроек (25 прошли). Замер `editor_report` (нативно, release, 100 000 узлов, самый медленный запрос; первый после правки имён / следующий / самый первый на странице): обычные имена — 13,2 / 1,2 / 10,1 мс (цель ≤ 30 / 30 / 40), 40 символов — 31,3 / 8,4 / 30,3 мс, 256 символов — 30,6 / 8,2 / 29,4 мс, немецкий, 40 символов — 38,9 / 7,9 / 33,6 мс, украинский — 30,0 / 7,4 / 28,5 мс, китайский — 27,7 / 8,0 / 36,5 мс, «я» × 256 — 33,0 / 7,7 / 30,2 мс (цели на длинных именах ≤ 40 / 30 / 40); синтетика «две раскладки, 40 символов» — 42,8 / 19,4 / 38,8 мс (≤ 45 / 30 / 45); память индекса 6,3 МБ на обычных именах и 21,2 МБ на длинных (B-142); «Состояния команд после шага, 100 000 узлов, выделен 1» — 0,001 мс. Холодные строки зависят от загрузки машины: на длинных именах они колеблются около 25–39 мс (при другой загрузке бывали выше 30 мс, потому цель 40). E2E (Chromium и Firefox): `palette.spec.ts` — открытие обоими сочетаниями и место окна, Esc с возвратом фокуса, Tab и щелчок мимо закрывают без возврата, IME (Enter и Esc составления текста), щелчок внутри окна оставляет фокус в поле и Delete не удаляет выделение, фокус ушёл из поля — Esc и сочетание закрывают, клавиши не доходят до холста, удержание Enter и сочетания, Ctrl/Cmd+K из поля панели открывает, в модальном диалоге — нет, команды с серыми, стрелки и команда интерфейса, раскладка, слои с переходом, устаревший ответ, новый запрос убирает слои прежнего (Enter к ним не переходит), поздний ответ не сбрасывает выбор стрелками, страницы, недавние после перезагрузки и из меню (кнопки масштаба и тулбара их не пополняют), привязка окна к одной букве (в полях буква набирается, на холсте окно открывается), ввод не вызывает инструменты и не двигает выделение; `@perf` (только Chromium): ввод на 20 000 узлах — без длинных задач; `shortcuts-panel.spec.ts` — место панели, закрытие, холст под панелью, вкладки и клавиши из раскладки, отметка использованных, ссылка в настройки. `__ferrumWarnings` пуст.
- **E2E в Chromium и Firefox** (`palette.spec.ts` и `shortcuts-panel.spec.ts` помечены `@firefox`, стандартный конфиг гоняет их в обоих; подписи знаков без карты раскладки — модульные тесты `shortcuts.test.ts` и `keymap.rs`):
  - главное меню с подменю, клавиатурой и состоянием пунктов;
````

на:

````markdown
- **Сделано (этап 2):** модульные тесты ядра — `context_menu` (выбор на правой кнопке: невыделенный, выделенный, пустое место в рамке и вне её; порядок блоков холста; «Выделить слой» сверху вниз с контейнерами; «Войти в правку» только у фигур; меню слоя и страницы, их доступность в документе только для чтения; неизвестная цель; правка вектора и текста не кончается; «Дублировать страницу» с новыми id, фоном и местом в списке); `ferrum-doc` — `duplicating_an_unopened_page_with_unknown_nodes_is_refused`, `paste_modes` (повтор +10, сброс новой копией, центр вида; поверх; замена с местом в слоях и родителем — один шаг; заблокированные и без выделения; состояния и `PasteRequested` с режимом), `props_clip` (что подходит каждому типу — один шаг; текст с меткой — в другой документ; свойства копии узлов; ошибки без шага; стиль текста с заливкой и подгонкой рамки — один шаг; вставка из копии узлов добавляет в документ только картинки перенесённых заливок; копия с текста берёт собственную заливку первого участка, вставка заливки снимает заливки участков тем же шагом; состояния), `smart_duplicate` (сдвиг протяжкой и стрелками повторяется, сброс выбором и отменой дубликата, несколько узлов; «Выделить подобные»: тип, заливки, обводки, скрытые, образцы, контейнер без потомков), `journal` («Дублировать страницу» переживает журнал и полное сохранение); ядро worker'а — `context_menu` (меню последним после выделения и состояний, ошибки цели, слой без прокрутки, «Развернуть всё», дубль страницы), `paste` (режимы в `PasteRequested`, замена; свойства текстом и из внутреннего буфера), `protocol` (новые сообщения, имена полей, `Paste` без `mode`). Замер `editor_report`: «Выделить подобные, 100 000 узлов» — 1,3 мс (цель ≤ 100 мс; нативно, release), выделено 1000 двойников. E2E: `context-menu.spec.ts` (Chromium и Firefox) — холст: выбор, место меню, состав, «Выделить слой», команда — шаг; пустое место; слои: выбор строки, «Развернуть всё», «Свернуть всё», «Переименовать»; страницы: серое «Удалить», «Дублировать», ссылка на вторую страницу открывает её, а не первую; правый щелчок в поле переименования слоя и страницы не открывает меню ядра; «Копировать ссылку» без буфера обмена — уведомление об ошибке; `__ferrumWarnings` пуст. `paste.spec.ts` — повтор +10 и Ctrl/Cmd+Shift+V, свойства Ctrl/Cmd+Alt+C и V (оба браузера); замена и поверх из меню, Ctrl/Cmd+Shift+R на холсте не перезагружает страницу (оба браузера), «Вставить с заменой» им одним шагом (Chromium), копия свойств в системном буфере (только Chromium, B-131). `duplicate.spec.ts` — умный дубликат стрелками и протяжкой, «Выделить подобные» из меню и сочетанием. `chrome.spec`, `clipboard.spec`, `main-menu.spec` — под новый состав меню.
- **Сделано (этап 3):** модульные тесты ядра — `find_layers` редактора (подпоследовательность и начала слов, регистр, «ё» и другая раскладка, порядок: лучшие первыми, при равном весе — в порядке панели, пределы; длинные имена ищутся по началу и возвращаются целиком; запрос длиннее предела обрезается; отмена и правка имён обновляют индекс, превью имени тоже; эталон по определению весов на случайных текстах до 70 символов многих алфавитов — `fast_scoring_matches_the_definition` (убивает мутантов в оценке) и сравнение лучших с полной сортировкой; общие векторы весов с интерфейсом — `score_vectors_shared_with_the_interface`; границы таблицы свёртки по всем символам Unicode — `fast_fold_paths_match_unicode`; переход: виден, влезает, не влезает, не шаг истории, слоя нет; скрытый слой и видимый ребёнок скрытого фрейма выделяются без движения камеры), `find_layers` и `protocol` ядра worker'а (`LayersFound` со своим запросом, пределы, переход раскрывает предков и прокручивает панель, в том числе у уже выделенного слоя; ошибки цели, имена полей), `command_table` и `keymap` (новые сочетания на обеих платформах); модульные интерфейса (169 прошли) — `fuzzy`, `palette` (состав списка, 50 строк, недавние, обрезка запроса), `settings` (`recentCommands`), `main-menu`; `store` десктопа — поле настроек (25 прошли). Замер `editor_report` (нативно, release, 100 000 узлов, самый медленный запрос; первый после правки имён / следующий / самый первый на странице): обычные имена — 13,2 / 1,2 / 10,1 мс (цель ≤ 30 / 30 / 40), 40 символов — 31,3 / 8,4 / 30,3 мс, 256 символов — 30,6 / 8,2 / 29,4 мс, немецкий, 40 символов — 38,9 / 7,9 / 33,6 мс, украинский — 30,0 / 7,4 / 28,5 мс, китайский — 27,7 / 8,0 / 36,5 мс, «я» × 256 — 33,0 / 7,7 / 30,2 мс (цели на длинных именах ≤ 40 / 30 / 40); синтетика «две раскладки, 40 символов» — 42,8 / 19,4 / 38,8 мс (≤ 45 / 30 / 45); память индекса 6,3 МБ на обычных именах и 21,2 МБ на длинных (B-142); «Состояния команд после шага, 100 000 узлов, выделен 1» — 0,001 мс. Холодные строки зависят от загрузки машины: на длинных именах они колеблются около 25–39 мс (при другой загрузке бывали выше 30 мс, потому цель 40). E2E (Chromium и Firefox): `palette.spec.ts` — открытие обоими сочетаниями и место окна, Esc с возвратом фокуса, Tab и щелчок мимо закрывают без возврата, IME (Enter и Esc составления текста), щелчок внутри окна оставляет фокус в поле и Delete не удаляет выделение, фокус ушёл из поля — Esc и сочетание закрывают, клавиши не доходят до холста, удержание Enter и сочетания, Ctrl/Cmd+K из поля панели открывает, в модальном диалоге — нет, команды с серыми, стрелки и команда интерфейса, раскладка, слои с переходом, устаревший ответ, новый запрос убирает слои прежнего (Enter к ним не переходит), поздний ответ не сбрасывает выбор стрелками, страницы, недавние после перезагрузки и из меню (кнопки масштаба и тулбара их не пополняют), привязка окна к одной букве (в полях буква набирается, на холсте окно открывается), ввод не вызывает инструменты и не двигает выделение; `@perf` (только Chromium): ввод на 20 000 узлах — без длинных задач; `shortcuts-panel.spec.ts` — место панели, закрытие, холст под панелью, вкладки и клавиши из раскладки, отметка использованных, ссылка в настройки. `__ferrumWarnings` пуст.
- **Сделано (этап 4):** модульные тесты ядра — `ferrum-doc`: `guides` (шаг на правку, перенос одним шагом, без изменений шага нет, отказы без следов, повторы, чужой тип узла, сохранение и кэш, неверные записи файла, сверх предела, NaN в кэше, незагруженная страница с неизвестными узлами, отмена и смена страницы, журнал, порядок страниц, дубль с отменой и повтором, парковка, удаление страницы, номер шага), `text::layer_name`; редактор: `rulers` (команды, Shift+R, показ, начало отсчёта, меню, скрытый интерфейс), `guides` (протяжка из обеих линеек, владелец, линия фрейма, перенос, удаление, отмена Esc и потерей фокуса, NaN, щелчок сквозь линию, модификаторы и инструменты, ручки и выделенный узел, курсоры, линейка и инструмент, только чтение, привязка на пороге и в экранных пикселях, ресайз и рисование, отмена через страницу, копия и удаление фрейма, вложенный фрейм, `set_guides`, предел), `undo_notice`, `pages`, `text_edit`, `clipboard_format`, `keymap`; рендер: шаг делений, линейки и полоса по пикселям, начало отсчёта, отрезок и бесконечная линия. Ядро worker'а: `SetView`, `SetGuides`, список страниц после отмены дубля. Модульные интерфейса — `settings`, `toasts` (часы на ручном таймере). `store` десктопа — новые поля. Всего 1154 прошли, 2 пропущены; `store` десктопа — 25; модульные интерфейса — 176.
- **Замеры (этап 4):** `render_report` (нативно, release, прибавка к спискам кадра, медиана чередующихся кадров со сдвигом вида) — 1920×1080: 0,150 мс, 3840×2160: 0,216 мс (цель ≤ 0,5 мс); с растеризацией CPU на пустой сцене — 0,203 и 0,354 мс; с 2000 линий на экране — 1,43 и 2,96 мс (линии — прямоугольниками: штрих одним путём стоил 26–54 мс). `editor_report`, 100 000 узлов и 2000 линий (медиана шага): движение указателя 0,003 мс, протяжка линии 0,005 мс, перетаскивание узла с привязкой 0,043 мс.
- **E2E этапа 4 (Chromium и Firefox, метка `@firefox`):** `rulers.spec.ts` — включение и перезагрузка, меню пустого места, протяжка, перенос, удаление, отмена, Esc, привязка узла, щелчок сквозь линию, Shift+G; `panels.spec.ts` — секции (мышь, клавиатура, по имени, перезагрузка), панели, ширина, «Экспорт…»; `toasts.spec.ts` — «Отменить», фокус, не последний шаг, закрытие, 4 с и курсор; `pages-order.spec.ts` — протяжка вниз и вверх, Esc, то же место, Alt+стрелки, перезагрузка, дубль с отменой и повтором. `__ferrumWarnings` пуст.
- **E2E в Chromium и Firefox** (`palette.spec.ts` и `shortcuts-panel.spec.ts` помечены `@firefox`, стандартный конфиг гоняет их в обоих; подписи знаков без карты раскладки — модульные тесты `shortcuts.test.ts` и `keymap.rs`):
  - главное меню с подменю, клавиатурой и состоянием пунктов;
````

- [ ] **Step 2: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-render --example render_report | grep -E "FAIL|Кадр с линейками"`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Линейки и 2000|Состояния команд после шага, 100"`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && scripts/compat-old-build.sh main guides_fixture`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && scripts/compat-old-build.sh main vector_fixture`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-desktop-store 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo check`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/rulers.spec.ts e2e/panels.spec.ts e2e/toasts.spec.ts e2e/pages-order.spec.ts e2e/text.spec.ts e2e/text-edit.spec.ts e2e/main-menu.spec.ts e2e/palette.spec.ts e2e/shortcuts-panel.spec.ts e2e/context-menu.spec.ts --project=firefox`

Expected: всё PASS: fmt, clippy (нативно и wasm32) без замечаний; `cargo test --workspace` — 1154 passed, 0 failed, 2 ignored; `render_report` — FAIL нет, прибавка кадра с линейками 0,150 и 0,216 мс при цели ≤ 0,5 мс; `editor_report` — FAIL нет (строка «Поиск слоёв… 256 символов… самый первый на странице» у границы 40 мс при загруженной машине может дать FAIL — повторить прогон, код поиска этап не трогает, B-150); совместимость `guides_fixture` — файл открыт как обычно, линии пережили пересохранение старой сборкой, `vector_fixture` — только чтение; `store` — 25 passed, `cargo check` десктопа без ошибок; модульные — 176 passed; `e2e:ci` без настоящих падений (прототип с `--retries=1`: 399 passed, 4 flaky: `fallback` «worker не ответил», `files` «1000 записей», `files` «вторая вкладка», `fsa` «запись на диске»; все проходят отдельным прогоном, B-177); Firefox без настоящих падений (прототип: 45 passed, 1 flaky до правки гонки в `pages-order`)

- [ ] **Step 3: Закоммитить**

```bash
git add docs/backlog.md docs/superpowers/specs/2026-11-02-ferrum-shell-design.md
git commit -F - <<'EOF'
docs: оболочка, этап 4 — «Сделано» в спеке (§9, §10, §11, §12) и B-161…B-177

Закрыты B-28, B-121, B-124; ограничения этапа — в новом разделе бэклога.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

- [ ] **Ручная проверка**

1. Chrome: `#/new`; Shift+R — линейки сверху и слева, деления меняются с масштабом, выделение подсвечено полосой, у узла во фрейме отсчёт от угла фрейма. Перезагрузка — линейки на месте.
2. Протянуть из верхней линейки на пустое место и во фрейм — горизонтальная линия страницы через весь холст и линия от края до края фрейма; подвинуть фрейм — его линия едет. Левая линейка — вертикальная.
3. Перенести линию из страницы во фрейм: Ctrl/Cmd+Z возвращает её одним шагом. На линейку — удаление, новая — не создаётся. Esc посреди жеста — без следов.
4. Перетащить узел к линии: привязка на 4 экранных пикселях. Щелчок по линии над узлом выбирает узел, двойной — как обычно.
5. Shift+G прячет линии; без линеек пункт «Вид → Направляющие» серый. Меню пустого места содержит «Линейки» и «Направляющие».
6. Заголовок секции сворачивает её, после перезагрузки свёрнута; кнопка в шапке сворачивает панель в полосу 48 px, холст занимает место, ширина после разворота прежняя; «Экспорт…» разворачивает панель и секцию.
7. Удалить слой: уведомление с «Отменить», под курсором не исчезает, ушли — исчезло через 4 с; кнопка отменяет, после новой правки — ничего. Закрытие — у каждого уведомления.
8. Страницы: протяжка меняет порядок, линия показывает место, Esc отменяет, Alt+стрелка двигает, Ctrl/Cmd+Z возвращает. «Дублировать страницу», отмена, повтор.
9. Новый текст: имя слоя — первые 40 символов первой строки; переименовать вручную — больше не следует за текстом.
10. Firefox: те же шаги 1–9.

---

## Что этап 4 сознательно не делает

- **Выделение линии**: удаление клавишей Delete и пункт «Убрать направляющую» (B-161); копия с Alt и измерения от линии (B-162).
- **Сочетание Shift+G не подтверждено** как сочетание направляющих линеек; подсистема 5 (сетки раскладки) должна решить (B-163).
- **Линии повёрнутого фрейма** рисуются, но не ловятся и не привязывают (B-164); правка вектора, перо и текст к линиям не привязываются (B-165); курсор над линейкой в правке не сверен (B-166); выделенный лист важнее линии только при выделении до 256 узлов (B-167).
- **Признака автоимени нет**: переименование в точности в текст снова включает слежение (B-168).
- **Кнопка «Отменить» не гаснет**, когда шаг перестал быть последним (B-169). Перестановка в режиме только чтения кончается сообщением об ошибке (B-170). Отмена перестановки и дубля переключает на страницу шага (B-171). «Новая страница», «Переименовать», «Удалить страницу» и фон — по-прежнему не шаги (B-172).
- **Свёрнутость** у каждой вкладки своя, свёрнутая секция размонтирована (B-173); нет сочетания для сворачивания панелей по одной (B-174); стрелка на ширине панели теряет быстрый шаг (B-175).
- **Не запускались:** проект Playwright `gpu`, `@perf`, WebKit и Safari, запуск десктопа (только `cargo check` и тесты `store`), замеры в wasm; Firefox — не весь набор (B-176, B-177).
