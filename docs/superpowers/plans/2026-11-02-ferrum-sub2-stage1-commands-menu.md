# Ferrum, подсистема 2, этап 1: состояние команд и главное меню — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Главное меню как в Figma UI3: кнопка с логотипом в шапке левой панели и команда `ui.main_menu` (Alt+/) открывают меню с подменю «Файл», «Правка», «Вид», «Объект», «Текст», «Расставить», «Вектор», «Настройки», «Справка» и полем поиска по командам; недоступные пункты серые, переключатели отмечены галочкой, подписи сочетаний — из действующей раскладки. Таблица команд ядра получает группу, синонимы и признак переключателя, команды интерфейса `ui.*` и команды меню (пиксельная сетка, «Войти в правку», форматирование текста); ядро считает состояние каждой команды («доступна / отмечена») и шлёт только изменившиеся (`CommandStates`), недоступная команда не выполняется — ни из меню, ни с клавиатуры. Контекстное меню холста, меню масштаба и тулбар берут доступность из того же состояния. Пересчёт состояний после шага на 100 000 узлах с одним выделенным — ≤ 1 мс.

**Architecture:** Модель документа не меняется. Редактор (`ferrum-editor`) — источник правды: таблица `COMMANDS` (`commands.rs`) с явной `CommandGroup`, `keywords`, `toggle`; команды `ui.*` только сообщают о себе (`EditorEvent::UiCommand`); `command_state.rs` считает `CommandState { enabled, checked }` за один проход по выделению, кэширует по ключу (`StatesKey`: версия выделения, счётчик и эпоха сцены, история, «только чтение», правка текста и вектора, настройки) и не пересчитывает во время жеста; `Editor::run` не выполняет недоступную команду; `menu.rs` описывает главное меню деревом `MenuNode`. Ядро worker'а (`ferrum-wasm`) переводит это в протокол: `KeymapEntry { group, keywords, toggle }`, `CoreEvent::{UiCommand, CommandStates, MainMenu}`, `Command::SetTheme { system? }`. Основной поток (SolidJS) ничего не вычисляет: `store.state.commands` — таблица `ключ → состояние`, `store.state.mainMenu` — дерево; компонент меню получает подменю, переход по первой букве, серые фокусируемые пункты и поле поиска; положение меню и подменю — в фазе `measure`; поиск (`main-menu.ts`) — подстрока названия и синонимов в обеих раскладках.

**Tech Stack:** Rust 1.96 (`rust-toolchain.toml`, новых зависимостей нет), SolidJS 1.9.17, TypeScript 6.0.3, @playwright/test 1.64.0 (`chromium`, `firefox`); `package-lock.json` и `Cargo.lock` не меняются.

**Спеки:** `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` — разделы 3, 4, 11, 12 (этап 1); спека ядра `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` («ядро §N»: протокол — §4.3, команды и раскладка — §7.6).

## Global Constraints

- Ветка `feat/sub2-shell` от `main` с коммитом спеки подсистемы 2 (53d262a). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Каталог сборки — у каждой копии свой: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0` (в `apps/web` — `"$PWD/../../target"`); команды — из корня копии, если не сказано иное.
- Новых зависимостей нет ни в Rust, ни в npm.
- Основной поток тонкий (ядро §4.2): доступность и отметки команд, дерево меню, раскладку и подписи считает ядро в worker'е; основной поток только рисует меню, выполняет команды `ui.*` (выбор файла требует жеста пользователя) и ищет по уже присланной таблице. Чтения layout (размер меню, положение подменю) — только в фазе `measure` планировщика; E2E проверяют `__ferrumWarnings`.
- Поведение — как в Figma; где спека молчит, решение помечено **(поведение)** (раздел «Решения»).
- Недоступная команда при вызове ничего не делает: ни шага, ни сообщения — одинаково для меню, `Execute` протокола и сочетаний клавиш.
- Бюджет: пересчёт `CommandStates` после шага на 100 000 узлах при одном выделенном — ≤ 1 мс (`editor_report`, «Состояния команд после шага…»); во время жеста не пересчитывается; в пачке событий ядра — одно `CommandStates`.
- Числа команд таблицы зашиты в трёх тестах — `crates/ferrum-editor/tests/keymap.rs`, `crates/ferrum-wasm/tests/boolean.rs`, `apps/web/e2e/app.spec.ts`: задачи 1 и 2 обновляют все три (55 → 70 → 80).
- Тесты не проходят вхолостую: редактор — таблица, состояния после реальных шагов, шаги отмены и сообщения; ядро — события протокола; модульные — чистые функции меню и поиска; E2E — роли и `aria-*` пунктов, фокус, выполнение команды по числу шагов документа.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Сначала прототип всего этапа собран в отдельной копии (`plan-verify-s2s1`, ветка `verify/sub2-s1` от 53d262a, по коммиту на задачу; вершина — 9f042d1, код — 4d324ba) и прошёл ворота на вершине: `cargo fmt --check`, `clippy -D warnings` (нативно и `wasm32-unknown-unknown`), `cargo test --workspace` (1004 passed, 2 ignored), `render_report` и `editor_report` (release, 47 и 89 строк PASS, FAIL нет; «Состояния команд после шага, 100 000 узлов, выделен 1» — 0,001 мс при цели ≤ 1 мс; 10 000 выделенных — 0,245 мс, все 100 000 — 5,455 мс, справочно), wasm-сборка, `npm run typecheck`, модульные (148), сборка, E2E `chromium` целиком (`--grep-invert @perf --retries=1`: 340 passed, 1 flaky — известная `text-edit.spec.ts` «фрагмент со стилями…», и `app.spec.ts` с устаревшим числом команд, исправленный в задачах 1 и 2 — после правки 13 passed), Firefox — `main-menu.spec.ts` и `chrome.spec.ts` (36 passed), совместимость `scripts/compat-old-build.sh main vector_fixture` («файл открыт только для чтения»). По ходу каждой задачи прототипа — её тесты, `cargo test` редактора и ядра, clippy, typecheck, модульные и затронутые E2E.

Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом, который читает сам этот документ, к чистой копии от 53d262a (`plan-apply-s2s1`, ветка `verify/sub2-s1-apply`, свой каталог сборки) в порядке задач с коммитом после каждой: тесты шага 1 — и проверка, что каждая команда шага «Убедиться, что тесты падают» действительно падает так, как написано в «Expected» (E2E задач 5–7 — после `npm run wasm && npm run build`, то есть против собранного стенда), затем реализация — каждый фрагмент «заменить» найден ровно один раз (операций по задачам: 53, 16, 34, 12, 16, 58, 8, 7). Из шага «Проверить» на чистой копии выполнены `cargo fmt --check`, оба `clippy -D warnings` и `cargo test` (числа в «Expected» задач 1–4 и 8 — с этого прогона: 560, 565, 578, 582 и 1004 passed, по 2 ignored); E2E шагов «Проверить», сборка wasm и стенда, typecheck, release-отчёты и совместимость на чистой копии не повторялись — их ожидаемые результаты взяты из прогона прототипа на том же коде. После каждой задачи дерево копии совпало с деревом соответствующего коммита прототипа (тот же хэш дерева у всех восьми), `git diff 9f042d1 HEAD` — пустой.

Порт 4173 на машине проверки занят посторонним процессом, поэтому E2E шли через временный конфиг Playwright с портами 4283/4284 (и временной правкой портов в `core.spec.ts` и `settings.spec.ts`, откатываемой после каждой команды).

Найденное прототипом и вошедшее в шаги: Enter стал сочетанием `vector.edit`, и удержанный Enter через раскладку входил в правку — нужна проверка `repeat` у команды в `key_down` (задача 2); числа команд зашиты в трёх тестах, последнее (`e2e/app.spec.ts`) нашёл только полный E2E (задачи 1 и 2); «Справка» содержит «правка» — локаторы `name: 'Правка'` нужны с `exact`, а «Экспорт…» и раздел «Настройки» конфликтуют с прежними локаторами диалога настроек (`exact`, `level: 1`, задача 1); Playwright не нажимает русские буквы (`keyboard.press('в')` — Unknown key) — переход по первой букве проверяется событием `keydown` в `document.activeElement` (задача 5); меню появляется с анимацией масштаба (`fe-pop`), и `boundingBox` сразу после `toBeVisible` даёт размер посреди анимации — тест подменю у края ждёт `getAnimations().finished` (задача 6); невидимое меню без `left/top` стоит в статической позиции потока — до расчёта оно стоит в (0, 0) (задача 6); щелчок по якорю считался «снаружи», и меню закрывалось и тут же открывалось снова (задача 6); сообщение «Для булевой операции выделите хотя бы два слоя» из команд теперь недостижимо (задача 3). Не запускались: проект Playwright `gpu` и замеры `@perf`, Safari/WebKit, десктоп (крейт не тронут), замеры в wasm, ручная проверка (задача 8).

---

## Этапы подсистемы 2

| Этап | Результат | Разделы спеки |
|---|---|---|
| **1. Состояние команд и главное меню** (этот план) | описание команд в ядре, состояние «доступна / отмечена», главное меню с подменю и поиском, компонент меню с подменю и первой буквой | 3, 4 |
| 2. Контекстные меню и вставка | меню холста по выделению, меню слоёв и страниц; свойства, вставка с заменой и поверх, умный дубликат | 5, 6 |
| 3. Быстрые действия и сочетания | палитра Ctrl/Cmd+K и Ctrl/Cmd+/; панель сочетаний клавиш | 7, 8 |
| 4. Линейки, направляющие и полировка панелей | линейки, направляющие с привязкой, сворачиваемые секции, уведомления с действием, страницы | 9, 10 |

## Решения

Где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**.

Команды:

1. Группа — перечисление `CommandGroup` (`Tool, File, Edit, View, Object, Text, Arrange, Vector, Nudge, Settings, Help`) с ключом и русским названием; ключ команды начинается с ключа группы (кроме `ui.*`, у которых группа своя: «Файл», «Настройки», «Справка», «Вид»). Названия групп — как подменю в спеке §4; «Инструменты» и «Сдвиг» — только для таблицы сочетаний.
2. «Сохранить» осталась `file.save` (событие `SaveRequested`), а не `ui.save`: ключ уже лежит в сохранённых раскладках пользователей.
3. **(поведение)** Без сочетаний по умолчанию: «Новый файл», «Открыть…» (Ctrl/Cmd+N и O перехватывает браузер), «Открыть с диска…», «Сохранить на диск…», «Сохранить копию на диск…», «Все файлы», «Настройки…», «Сочетания клавиш» (Ctrl+Shift+? — этап 3, вместе с панелью), «О программе», три темы и «Копировать как SVG». С сочетаниями: главное меню — Alt+/, «Экспорт…» — Ctrl/Cmd+Shift+E, «Привязка к пикселям» — Ctrl/Cmd+Shift+', «Пиксельная сетка» — Shift+' (как в справке Figma), «Войти в правку» — Enter, форматирование текста — как в правке текста (Ctrl/Cmd+B, I, U, Ctrl/Cmd+Shift+X, Ctrl/Cmd+Alt+L/T/R/J).
4. **(поведение)** Форматирование текста идёт через раскладку (`text_format_key` ищет команду группы «Текст» в действующей раскладке; зашитые B/I/U/X/L/T/R/J убраны, Tab — как был): переназначенное пользователем сочетание работает и в правке текста. Вне правки жирный и курсив применяются к участкам всех выделенных текстов одним шагом; «включено у всех» — по всем участкам вместе.
5. **(поведение)** Enter — «Войти в правку» (`vector.edit`): текст — правка текста, вектор и фигура — правка вектора. Удержанный Enter (`repeat`) в правку не входит — проверка в `key_down` (иначе через раскладку повтор входил в правку).
6. **(поведение)** Пункт меню `ui.*` выполняет основной поток сразу (выбор файла требует жеста пользователя); сочетание — через ядро (`EditorEvent::UiCommand` → `CoreEvent::UiCommand`). Команды `ui.*` не заканчивают правку вектора.
7. **(поведение)** Тема и привязка к пикселям — команды `ui.*` (их хранит интерфейс); отметку считает ядро по тому, что прислал интерфейс (`SetSettings`, `SetTheme { system }`).
8. **(поведение)** «Скрыть интерфейс» — переключатель (отметка при скрытом); флаг `Settings::ui_hidden` переживает смену документа, как и `uiHidden` интерфейса. Пиксельная сетка — настройка ядра, в настройки пользователя не сохраняется (B-94).

Состояние команд:

9. **(поведение)** Недоступная команда не выполняется в `Editor::run` — то же для клавиатуры: удаление без выделения или в документе только для чтения больше не шлёт сообщений.
10. **(поведение)** Булевы операции — от двух слоёв или у выделенных булевых групп (смена операции); над одним слоем — недоступны, как в Figma (раньше — сообщение «Для булевой операции выделите хотя бы два слоя»; оно осталось в `boolean()` только для прямого вызова).
11. **(поведение)** «Разгруппировать» — только у контейнера с детьми (группа, фрейм или булева группа); у одного прямоугольника — серая.
12. **(поведение)** Документ только для чтения: правки молча недоступны (серые, без сообщения); копирование, вид, инструменты и команды `ui.*` доступны. «Вставить» — всегда, кроме «только чтения»; «Отменить» доступна и с открытым путём пера.
13. Пересчёт — по ключу и не во время жеста; интерфейсу — только изменившиеся команды, первым событием — все (новое ядро, новый документ); в пачке событий ядра — одно `CommandStates`.

Главное меню:

14. **(поведение)** Кнопка с логотипом — слева в шапке левой панели, на месте меню файла; «Назад к файлам» — справа. Повторный щелчок по логотипу закрывает меню (щелчок по якорю не считается щелчком «снаружи»).
15. **(поведение)** Меню «Текст» и «Вектор» наполнены командами этого этапа (форматирование и «Войти в правку»); пункты следующих этапов (вставить поверх, с заменой, свойства, «Выделить подобные», линейки) не показываются серыми заглушками — появятся вместе с командами.
16. **(поведение)** Открытие — фокус в поле поиска; ↓ — к первому пункту, ↑ с первого — обратно в поле. Подменю — наведение 150 мс, щелчок, Enter, →; ← и Esc — к пункту-родителю. Серые пункты фокусируются стрелками, но не выполняются и меню не закрывают. Переход по первой букве — по кругу, без учёта регистра и «ё».
17. Подменю встаёт справа от пункта, у края окна — слева от пункта и поднимается (отступ 8 px); положение — в фазе `measure`, невидимое меню до расчёта стоит в (0, 0), чтобы размер не зависел от места.
18. **(поведение)** Поиск этапа 1 — подстрока названия или синонима, без учёта регистра и «ё», в обеих раскладках (запрос переводится ЙЦУКЕН ↔ QWERTY: «щквук» находит «order»; пример спеки «ыщквук» — это «sorder», лишняя первая буква, спека исправлена в задаче 8); сначала названия, начинающиеся с запроса, затем остальные в порядке меню; не больше 50; Enter — первая доступная; без групп-заголовков.
19. **(поведение)** «Открыть с диска…» и «Сохранить на диск…» — только при `files.disk` (File System Access или десктоп): без него интерфейс прячет пункты. Это возможность платформы, а не состояние команды.
20. **(поведение)** «Экспорт…» переводит фокус на кнопку секции «Экспорт» панели свойств (окна экспорта нет — B-96); «О программе» — уведомление; «Настройки…» и «Сочетания клавиш» — диалог настроек на вкладке «Горячие клавиши».
21. **(поведение)** Тулбар: кнопка и список «Булевы операции» — доступность из состояния команд; отметка в списке — только у операции выделенных булевых групп (раньше — у операции на кнопке); на кнопке — отмеченная, иначе последняя выбранная.

Протокол: `KeymapEntry` + `keywords`, `toggle` (`group` — из таблицы); `CoreEvent::UiCommand { key }`, `CoreEvent::CommandStates { changed: [{ key, enabled, checked? }] }`, `CoreEvent::MainMenu { tree: MenuItemDto[] }` (`{ type: "command", key } | { type: "submenu", title, items } | { type: "separator" }`); `Command::SetTheme { theme, system? }`. Совместимость документа не затронута — модель не менялась.

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-editor/src/commands.rs`, `actions.rs`, `editor.rs`, `text_style.rs`, `vector_select.rs`, `lib.rs` | таблица команд (группа, синонимы, переключатели), команды `ui.*` и меню, выполнение, форматирование по раскладке |
| `crates/ferrum-editor/src/command_state.rs` (новый) | `CommandState`, правила доступности и отметок, кэш по ключу |
| `crates/ferrum-editor/src/menu.rs` (новый) | дерево главного меню |
| `crates/ferrum-wasm/src/{protocol,core}.rs`, `core/panels.rs` | `KeymapEntry`, `UiCommand`, `CommandStates`, `MainMenu`, `SetTheme.system` |
| `apps/web/src/ui/menu.tsx`, `menu-logic.ts` (новый), `styles/chrome.css` | компонент меню: подменю, первая буква, серые пункты, поиск, положение |
| `apps/web/src/ui/main-menu.ts` (новый), `app.tsx`, `chrome.tsx`, `store.ts`, `context.ts`, `boot.ts`, `toolbar.tsx`, `properties.tsx`, `keymap-ui.ts` | главное меню, команды `ui.*`, состояние команд в интерфейсе |
| `crates/ferrum-editor/tests/{command_table,menu_commands,command_state,main_menu}.rs` (новые), `apps/web/unit/{menu,main-menu}.test.ts` (новые), `apps/web/e2e/main-menu.spec.ts` (новый) | тесты |
| `crates/ferrum-editor/examples/editor_report.rs`, спека, `docs/backlog.md` | замер состояний; «Сделано (этап 1)» |

---

### Task 1: Метаданные команд и команды интерфейса `ui.*`

Спека §3: таблица `COMMANDS` получает `group`, `keywords`, `toggle` (без них не собирается); действия основного потока лежат в той же таблице с ключами `ui.*`, ядро сообщает о них событием `UiCommand`. Здесь — `CommandGroup`, конструктор `cmd(id, key, group, name, keywords, defaults)` с `.toggle()`, `.pc(..)`, `.per_platform(mac, pc)` вместо прежних `cmd`/`cmd_per_platform`, 15 команд `ui.*` (команд 55 → 70), `KeymapEntry` с группой из таблицы, синонимами и признаком переключателя, группы «Текст», «Вектор», «Настройки», «Справка» в таблице сочетаний настроек.

**Files:**
- Modify: `crates/ferrum-editor/src/{commands,actions,editor,lib,vector_select}.rs`, `crates/ferrum-wasm/src/{core,protocol}.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `apps/web/src/ui/keymap-ui.ts`
- Test: `crates/ferrum-editor/tests/command_table.rs` (новый), `crates/ferrum-editor/tests/keymap.rs` (команд 70, `NO_DEFAULTS`), `crates/ferrum-wasm/tests/{boolean,panels,protocol}.rs`, `apps/web/unit/settings.test.ts`, `apps/web/e2e/app.spec.ts` (команд 70), `apps/web/e2e/chrome.spec.ts` (у команд без сочетаний нет подписей), `apps/web/e2e/settings.spec.ts` (кнопка «Экспорт» — `exact`, заголовок «Настройки» — `level: 1`: появились команда «Экспорт…» и раздел «Настройки»)

**Interfaces:**
- Consumes: `COMMANDS`, `Keymap`, `EditorEvent`, `CoreEvent::Keymap`.
- Produces: `pub enum CommandGroup { Tool, File, Edit, View, Object, Text, Arrange, Vector, Nudge, Settings, Help }` (`ALL`, `key()`, `title()`); `Command { id, key, group, name, keywords: &[&str], toggle, defaults, defaults_pc, defaults_mac }`; `const fn cmd(..)`, `.toggle()`, `.pc(..)`, `.per_platform(mac, pc)`; `CommandId::{group(), is_ui()}`; `CommandId::Ui{MainMenu, NewFile, Open, OpenDisk, SaveDisk, SaveCopy, Export, AllFiles, Settings, Shortcuts, About, PixelSnap, ThemeDark, ThemeLight, ThemeSystem}`; `EditorEvent::UiCommand(CommandId)`; `CoreEvent::UiCommand { key }`; `KeymapEntry { …, group, keywords: Vec<String>, toggle: bool }`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 55, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 70, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
    });
  }
  expect(checked).toBeGreaterThanOrEqual(commands.length);
});

````

на:

````ts
    });
  }
  // У команд интерфейса без привычного сочетания (новый файл, открыть…) подписей нет.
  expect(checked).toBeGreaterThanOrEqual(commands.filter((c) => c.bindings.length > 0).length);
  expect(checked).toBeGreaterThan(50);
});

````

В `apps/web/e2e/settings.spec.ts` заменить:

````ts
  await expect(row).toContainText('Перехватывает браузер');
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Экспорт' }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('ferrum-keymap.json');
````

на:

````ts
  await expect(row).toContainText('Перехватывает браузер');
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Экспорт', exact: true }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('ferrum-keymap.json');
````

В `apps/web/e2e/settings.spec.ts` заменить:

````ts
  const dialog = await openSettings(page);
  // Заголовок и поля диалога — не интерактивные: щелчок оставляет фокус на самом диалоге.
  await dialog.getByRole('heading', { name: 'Настройки' }).click();
  await expect(dialog).toBeFocused();
  await dialog.locator('.fe-dialog__body').click({ position: { x: 2, y: 2 } });
````

на:

````ts
  const dialog = await openSettings(page);
  // Заголовок и поля диалога — не интерактивные: щелчок оставляет фокус на самом диалоге.
  await dialog.getByRole('heading', { name: 'Настройки', level: 1 }).click();
  await expect(dialog).toBeFocused();
  await dialog.locator('.fe-dialog__body').click({ position: { x: 2, y: 2 } });
````

В `apps/web/e2e/settings.spec.ts` заменить:

````ts
  // Экспорт без отличий — всё равно верный файл раскладки.
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Экспорт' }).click();
  const text = Buffer.concat(await (await (await download).createReadStream()).toArray()).toString();
  expect(JSON.parse(text)).toMatchObject({ format: 'ferrum-keymap', version: 1 });
````

на:

````ts
  // Экспорт без отличий — всё равно верный файл раскладки.
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Экспорт', exact: true }).click();
  const text = Buffer.concat(await (await (await download).createReadStream()).toArray()).toString();
  expect(JSON.parse(text)).toMatchObject({ format: 'ferrum-keymap', version: 1 });
````

В `apps/web/e2e/settings.spec.ts` заменить:

````ts
  await expect.poll(keys).toEqual(['KeyR', 'KeyQ']);
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Экспорт' }).click();
  const bytes = Buffer.concat(await (await (await download).createReadStream()).toArray());
  await dialog.getByRole('button', { name: 'Сбросить все' }).click();
````

на:

````ts
  await expect.poll(keys).toEqual(['KeyR', 'KeyQ']);
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Экспорт', exact: true }).click();
  const bytes = Buffer.concat(await (await (await download).createReadStream()).toArray());
  await dialog.getByRole('button', { name: 'Сбросить все' }).click();
````

В `apps/web/unit/settings.test.ts` заменить:

````ts
  name,
  group: key.split('.')[0],
  bindings,
  labels: [],
````

на:

````ts
  name,
  group: key.split('.')[0],
  keywords: [],
  toggle: false,
  bindings,
  labels: [],
````

Создать `crates/ferrum-editor/tests/command_table.rs`:

````rust
//! Таблица команд (спека подсистемы 2, раздел 3): у каждой команды — группа,
//! синонимы на двух языках и признак переключателя; команды `ui.*` ядро не
//! выполняет, а сообщает о них интерфейсу.

mod common;

use std::collections::HashSet;

use common::*;
use ferrum_editor::*;

fn cyrillic(s: &str) -> bool {
    s.chars().any(|c| ('а'..='я').contains(&c) || c == 'ё')
}

fn latin(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_lowercase())
}

#[test]
fn every_command_has_group_keywords_and_unique_key() {
    let mut keys = HashSet::new();
    let mut ids = HashSet::new();
    for c in COMMANDS {
        assert!(keys.insert(c.key), "ключ {} повторяется", c.key);
        assert!(ids.insert(c.id), "{:?} повторяется", c.id);
        assert!(!c.name.is_empty(), "{:?} без названия", c.id);
        // Синонимы: хотя бы один русский и один английский, без пустых и повторов.
        assert!(
            c.keywords.iter().any(|k| cyrillic(k)),
            "{:?}: нет русских синонимов",
            c.id
        );
        assert!(
            c.keywords.iter().any(|k| latin(k) && !cyrillic(k)),
            "{:?}: нет английских синонимов",
            c.id
        );
        let unique: HashSet<_> = c.keywords.iter().collect();
        assert_eq!(
            unique.len(),
            c.keywords.len(),
            "{:?}: повтор синонима",
            c.id
        );
        for k in c.keywords {
            assert!(
                !k.is_empty() && *k == k.trim() && k.to_lowercase() == *k,
                "{:?}: синоним «{k}» — строчными и без пробелов по краям",
                c.id
            );
        }
        // Команды интерфейса — по ключу `ui.*`; у остальных ключ начинается с группы
        // (кроме «Сохранить»: `file.save` — в группе «Файл»).
        assert_eq!(c.id.is_ui(), c.key.starts_with("ui."));
        if !c.id.is_ui() {
            assert_eq!(c.key.split('.').next(), Some(c.group.key()), "{:?}", c.id);
        }
        assert_eq!(c.id.group(), c.group);
    }
    // Каждая группа меню непуста; ключи и названия групп различны.
    let groups: HashSet<_> = COMMANDS.iter().map(|c| c.group).collect();
    for g in CommandGroup::ALL {
        if g == CommandGroup::Text || g == CommandGroup::Vector {
            continue; // наполняются командами текста и правки вектора
        }
        assert!(groups.contains(&g), "группа {g:?} пуста");
    }
    let titles: HashSet<_> = CommandGroup::ALL.iter().map(|g| g.title()).collect();
    assert_eq!(titles.len(), CommandGroup::ALL.len());
}

#[test]
fn toggles_are_exactly_the_checkable_commands() {
    use CommandId as C;
    let toggles: Vec<CommandId> = COMMANDS.iter().filter(|c| c.toggle).map(|c| c.id).collect();
    assert_eq!(
        toggles,
        vec![
            C::BooleanUnion,
            C::BooleanSubtract,
            C::BooleanIntersect,
            C::BooleanExclude,
            C::UiPixelSnap,
            C::UiThemeDark,
            C::UiThemeLight,
            C::UiThemeSystem,
        ]
    );
}

#[test]
fn ui_commands_are_reported_not_performed() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a]);
    h.ed.take_events();
    let before = h.state();
    let steps = h.ed.step_count();
    for c in COMMANDS.iter().filter(|c| c.id.is_ui()) {
        h.ed.execute(c.id);
        let events = h.ed.take_events();
        assert_eq!(
            events,
            vec![EditorEvent::UiCommand(c.id)],
            "{} — только событие",
            c.key
        );
    }
    // Документ, история и выделение не тронуты.
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(h.state(), before);
    assert_eq!(h.sel(), vec![a]);
}

#[test]
fn main_menu_shortcut_reaches_the_interface() {
    for platform in [Platform::Mac, Platform::Windows] {
        let mut h = H::with_platform(platform);
        h.ed.take_events();
        h.key("Slash", alt());
        assert_eq!(
            h.ed.take_events(),
            vec![EditorEvent::UiCommand(CommandId::UiMainMenu)],
            "{platform:?}"
        );
    }
}

#[test]
fn ui_commands_keep_vector_editing() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a]);
    assert!(h.ed.start_vector_edit(a));
    h.ed.execute(CommandId::UiSettings);
    assert!(h.ed.is_editing_vector(), "правка вектора продолжается");
}
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    (CommandId::SelectAll, &["Mod+KeyA"]),
    (CommandId::Escape, &["Escape"]),
];

````

на:

````rust
    (CommandId::SelectAll, &["Mod+KeyA"]),
    (CommandId::Escape, &["Escape"]),
    // Подсистема 2: главное меню — Alt+/ (в Figma поиск в меню — по логотипу);
    // «Экспорт…» и «Привязка к пикселям» — как в справке Figma.
    (CommandId::UiMainMenu, &["Alt+Slash"]),
    (CommandId::UiExport, &["Mod+Shift+KeyE"]),
    (CommandId::UiPixelSnap, &["Mod+Shift+Quote"]),
];

/// Команды без сочетания по умолчанию: «Копировать как SVG» (как в Figma) и
/// команды интерфейса, чьи привычные сочетания занимает браузер (Ctrl/Cmd+N, O)
/// или которых в Figma нет.
const NO_DEFAULTS: &[CommandId] = &[
    CommandId::CopyAsSvg,
    CommandId::UiNewFile,
    CommandId::UiOpen,
    CommandId::UiOpenDisk,
    CommandId::UiSaveDisk,
    CommandId::UiSaveCopy,
    CommandId::UiAllFiles,
    CommandId::UiSettings,
    CommandId::UiShortcuts,
    CommandId::UiAbout,
    CommandId::UiThemeDark,
    CommandId::UiThemeLight,
    CommandId::UiThemeSystem,
];

````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    for c in COMMANDS {
        assert!(!c.name.is_empty() && CommandId::from_key(c.key) == Some(c.id));
        // «Копировать как SVG» — без сочетания, как в Figma (меню «Правка»).
        assert_eq!(
            map.bindings(c.id).is_empty(),
            c.id == CommandId::CopyAsSvg,
            "{:?}",
            c.id
````

на:

````rust
    for c in COMMANDS {
        assert!(!c.name.is_empty() && CommandId::from_key(c.key) == Some(c.id));
        assert_eq!(
            map.bindings(c.id).is_empty(),
            NO_DEFAULTS.contains(&c.id),
            "{:?}",
            c.id
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 55);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 70);
}

````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 55);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

на:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 70);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
    let undo = commands.iter().find(|c| c.key == "edit.undo").unwrap();
    assert_eq!(undo.labels, ["⌘Z"], "подпись macOS");
    assert!(overrides.contains("ferrum-keymap"));
    assert!(reserved.contains(&"Meta+KeyW".to_string()));
````

на:

````rust
    let undo = commands.iter().find(|c| c.key == "edit.undo").unwrap();
    assert_eq!(undo.labels, ["⌘Z"], "подпись macOS");
    // Группа — явная, из таблицы (а не префикс ключа); синонимы и переключатели.
    let dup = commands.iter().find(|c| c.key == "edit.duplicate").unwrap();
    assert!(dup.keywords.contains(&"duplicate".to_string()) && !dup.toggle);
    let settings = commands.iter().find(|c| c.key == "ui.settings").unwrap();
    assert_eq!(settings.group, "settings");
    let snap = commands.iter().find(|c| c.key == "ui.pixel_snap").unwrap();
    assert!(snap.toggle);
    assert!(overrides.contains("ferrum-keymap"));
    assert!(reserved.contains(&"Meta+KeyW".to_string()));
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
    exported(&core.take_events());
}
````

на:

````rust
    exported(&core.take_events());
}

#[test]
fn ui_commands_come_back_as_events() {
    let mut core = demo(10);
    core.take_events();
    core.command(Command::Execute {
        command: "ui.settings".into(),
    });
    let ev = core.take_events();
    assert_eq!(
        ev,
        vec![CoreEvent::UiCommand {
            key: "ui.settings".into()
        }]
    );
    assert_eq!(document_steps(&core), 0.0);
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
            background: None,
        },
        CoreEvent::Keymap {
            commands: vec![KeymapEntry {
````

на:

````rust
            background: None,
        },
        CoreEvent::UiCommand {
            key: "ui.main_menu".into(),
        },
        CoreEvent::Keymap {
            commands: vec![KeymapEntry {
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
                name: "Прямоугольник".into(),
                group: "tool".into(),
                bindings: vec!["KeyR".into()],
                labels: vec!["R".into()],
````

на:

````rust
                name: "Прямоугольник".into(),
                group: "tool".into(),
                keywords: vec!["квадрат".into(), "rectangle".into()],
                toggle: false,
                bindings: vec!["KeyR".into()],
                labels: vec!["R".into()],
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test command_table --test keymap`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test panels --test protocol --test boolean`
Expected: FAIL — не собирается: редактор — нет `CommandGroup`, полей `Command::{group, keywords, toggle}`, метода `CommandId::is_ui`, вариантов `CommandId::Ui*` и `EditorEvent::UiCommand` (`command_table` — 26 ошибок, `keymap` — 15); ядро — нет полей `KeymapEntry::{keywords, toggle}` и варианта `CoreEvent::UiCommand` (`panels` — 4 ошибки, `protocol` — 3)

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/keymap-ui.ts` заменить:

````ts

// Таблица раскладки для настроек: группы команд по ключу (`tool.*`, `edit.*`…),
// поиск по названию и подписи, сочетания, которые перехватывает браузер,
// конфликты, запись сочетания с клавиатуры.
````

на:

````ts

// Таблица раскладки для настроек: группы команд из таблицы ядра (`tool`, `edit`…),
// поиск по названию и подписи, сочетания, которые перехватывает браузер,
// конфликты, запись сочетания с клавиатуры.
````

В `apps/web/src/ui/keymap-ui.ts` заменить:

````ts
  { id: 'edit', title: 'Правка' },
  { id: 'object', title: 'Объект' },
  { id: 'arrange', title: 'Порядок' },
  { id: 'nudge', title: 'Сдвиг' },
  { id: 'view', title: 'Вид' },
  { id: 'file', title: 'Файл' },
];

````

на:

````ts
  { id: 'edit', title: 'Правка' },
  { id: 'object', title: 'Объект' },
  { id: 'text', title: 'Текст' },
  { id: 'vector', title: 'Вектор' },
  { id: 'arrange', title: 'Порядок' },
  { id: 'nudge', title: 'Сдвиг' },
  { id: 'view', title: 'Вид' },
  { id: 'file', title: 'Файл' },
  { id: 'settings', title: 'Настройки' },
  { id: 'help', title: 'Справка' },
];

````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::ToggleUi => self.out.events.push(EditorEvent::ToggleUi),
            C::Save => self.out.events.push(EditorEvent::SaveRequested),
            C::FlipHorizontal => self.mirror(Vec2::new(-1.0, 1.0)),
            C::FlipVertical => self.mirror(Vec2::new(1.0, -1.0)),
````

на:

````rust
            C::ToggleUi => self.out.events.push(EditorEvent::ToggleUi),
            C::Save => self.out.events.push(EditorEvent::SaveRequested),
            C::UiMainMenu
            | C::UiNewFile
            | C::UiOpen
            | C::UiOpenDisk
            | C::UiSaveDisk
            | C::UiSaveCopy
            | C::UiExport
            | C::UiAllFiles
            | C::UiSettings
            | C::UiShortcuts
            | C::UiAbout
            | C::UiPixelSnap
            | C::UiThemeDark
            | C::UiThemeLight
            | C::UiThemeSystem => self.out.events.push(EditorEvent::UiCommand(cmd)),
            C::FlipHorizontal => self.mirror(Vec2::new(-1.0, 1.0)),
            C::FlipVertical => self.mirror(Vec2::new(1.0, -1.0)),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
//! одно и то же по идентификатору. Сочетания по умолчанию повторяют Figma: на macOS
//! главная клавиша — Cmd, на Windows и Linux — Ctrl (в таблице это `Mod`).

use crate::input::Platform;
````

на:

````rust
//! одно и то же по идентификатору. Сочетания по умолчанию повторяют Figma: на macOS
//! главная клавиша — Cmd, на Windows и Linux — Ctrl (в таблице это `Mod`).
//!
//! У каждой команды — группа (подменю главного меню), синонимы для поиска и
//! признак переключателя (спека подсистемы 2, раздел 3). Команды `ui.*`
//! выполняет интерфейс: ядро только сообщает о вызове.

use crate::input::Platform;
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// Сохранить файл в хранилище (интерфейс получает `EditorEvent::SaveRequested`).
    Save,
}

````

на:

````rust
    /// Сохранить файл в хранилище (интерфейс получает `EditorEvent::SaveRequested`).
    Save,
    // ——— Команды интерфейса (`ui.*`): их выполняет основной поток, ядро только
    // сообщает о вызове событием `EditorEvent::UiCommand` (спека подсистемы 2,
    // раздел 3). ———
    /// Открыть главное меню (Alt+/).
    UiMainMenu,
    UiNewFile,
    UiOpen,
    /// «Открыть с диска…» (File System Access и десктоп).
    UiOpenDisk,
    UiSaveDisk,
    UiSaveCopy,
    /// «Экспорт…»: секция экспорта панели свойств.
    UiExport,
    UiAllFiles,
    UiSettings,
    UiShortcuts,
    UiAbout,
    /// Привязка к пикселям (настройка пользователя).
    UiPixelSnap,
    UiThemeDark,
    UiThemeLight,
    UiThemeSystem,
}

/// Группа команды: подменю главного меню и раздел таблицы сочетаний.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CommandGroup {
    Tool,
    File,
    Edit,
    View,
    Object,
    Text,
    Arrange,
    Vector,
    Nudge,
    Settings,
    Help,
}

impl CommandGroup {
    pub const ALL: [CommandGroup; 11] = [
        CommandGroup::Tool,
        CommandGroup::File,
        CommandGroup::Edit,
        CommandGroup::View,
        CommandGroup::Object,
        CommandGroup::Text,
        CommandGroup::Arrange,
        CommandGroup::Vector,
        CommandGroup::Nudge,
        CommandGroup::Settings,
        CommandGroup::Help,
    ];

    /// Ключ группы в протоколе (`KeymapEntry.group`).
    pub fn key(self) -> &'static str {
        match self {
            CommandGroup::Tool => "tool",
            CommandGroup::File => "file",
            CommandGroup::Edit => "edit",
            CommandGroup::View => "view",
            CommandGroup::Object => "object",
            CommandGroup::Text => "text",
            CommandGroup::Arrange => "arrange",
            CommandGroup::Vector => "vector",
            CommandGroup::Nudge => "nudge",
            CommandGroup::Settings => "settings",
            CommandGroup::Help => "help",
        }
    }

    /// Название подменю и раздела.
    pub fn title(self) -> &'static str {
        match self {
            CommandGroup::Tool => "Инструменты",
            CommandGroup::File => "Файл",
            CommandGroup::Edit => "Правка",
            CommandGroup::View => "Вид",
            CommandGroup::Object => "Объект",
            CommandGroup::Text => "Текст",
            CommandGroup::Arrange => "Расставить",
            CommandGroup::Vector => "Вектор",
            CommandGroup::Nudge => "Сдвиг",
            CommandGroup::Settings => "Настройки",
            CommandGroup::Help => "Справка",
        }
    }
}

````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// Стабильный ключ для настроек, меню и JSON раскладки.
    pub key: &'static str,
    /// Название в интерфейсе.
    pub name: &'static str,
    /// Сочетания по умолчанию: `Mod` — Cmd на macOS, Ctrl на остальных.
    pub defaults: &'static [&'static str],
````

на:

````rust
    /// Стабильный ключ для настроек, меню и JSON раскладки.
    pub key: &'static str,
    /// Группа: подменю главного меню и раздел таблицы сочетаний.
    pub group: CommandGroup,
    /// Название в интерфейсе.
    pub name: &'static str,
    /// Синонимы для поиска по командам — русские и английские («дубль, duplicate»).
    pub keywords: &'static [&'static str],
    /// Переключатель: у пункта меню галочка (`checked` в состоянии команды).
    pub toggle: bool,
    /// Сочетания по умолчанию: `Mod` — Cmd на macOS, Ctrl на остальных.
    pub defaults: &'static [&'static str],
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
}

const fn cmd(
    id: CommandId,
    key: &'static str,
    name: &'static str,
    defaults: &'static [&'static str],
) -> Command {
````

на:

````rust
}

/// Строка таблицы. Группа и синонимы обязательны: без них таблица не собирается.
const fn cmd(
    id: CommandId,
    key: &'static str,
    group: CommandGroup,
    name: &'static str,
    keywords: &'static [&'static str],
    defaults: &'static [&'static str],
) -> Command {
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        id,
        key,
        name,
        defaults,
        defaults_pc: &[],
````

на:

````rust
        id,
        key,
        group,
        name,
        keywords,
        toggle: false,
        defaults,
        defaults_pc: &[],
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
}

/// Команда с разными сочетаниями на macOS и на Windows/Linux.
const fn cmd_per_platform(
    id: CommandId,
    key: &'static str,
    name: &'static str,
    mac: &'static [&'static str],
    pc: &'static [&'static str],
) -> Command {
    Command {
        id,
        key,
        name,
        defaults: &[],
        defaults_pc: pc,
        defaults_mac: mac,
    }
}

use CommandId as C;

/// Все команды. Сочетания сверены с официальным списком горячих клавиш Figma.
pub const COMMANDS: &[Command] = &[
    cmd(C::ToolMove, "tool.move", "Перемещение", &["KeyV"]),
    cmd(C::ToolFrame, "tool.frame", "Фрейм", &["KeyF", "KeyA"]),
    cmd(C::ToolRect, "tool.rect", "Прямоугольник", &["KeyR"]),
    cmd(C::ToolEllipse, "tool.ellipse", "Эллипс", &["KeyO"]),
    cmd(C::ToolLine, "tool.line", "Линия", &["KeyL"]),
    cmd(C::ToolHand, "tool.hand", "Рука", &["KeyH"]),
    cmd(C::ToolText, "tool.text", "Текст", &["KeyT"]),
    cmd(C::ToolPen, "tool.pen", "Перо", &["KeyP"]),
    cmd(C::ToolScale, "tool.scale", "Масштабирование", &["KeyK"]),
    cmd(C::ToolArrow, "tool.arrow", "Стрелка", &["Shift+KeyL"]),
    cmd(
        C::HandHold,
        "tool.hand_hold",
        "Рука, пока клавиша нажата",
        &["Space"],
    ),
````

на:

````rust
}

impl Command {
    /// Переключатель (галочка в меню).
    const fn toggle(self) -> Command {
        Command {
            toggle: true,
            ..self
        }
    }

    /// Дополнительные сочетания только для Windows и Linux.
    const fn pc(self, pc: &'static [&'static str]) -> Command {
        Command {
            defaults_pc: pc,
            ..self
        }
    }

    /// Разные сочетания на macOS и на Windows/Linux (вместо общих).
    const fn per_platform(
        self,
        mac: &'static [&'static str],
        pc: &'static [&'static str],
    ) -> Command {
        Command {
            defaults: &[],
            defaults_mac: mac,
            defaults_pc: pc,
            ..self
        }
    }
}

use CommandGroup as G;
use CommandId as C;

/// Все команды. Сочетания сверены с официальным списком горячих клавиш Figma.
pub const COMMANDS: &[Command] = &[
    cmd(
        C::ToolMove,
        "tool.move",
        G::Tool,
        "Перемещение",
        &["выбор", "курсор", "стрелка", "move", "select"],
        &["KeyV"],
    ),
    cmd(
        C::ToolFrame,
        "tool.frame",
        G::Tool,
        "Фрейм",
        &["монтажная область", "артборд", "frame", "artboard"],
        &["KeyF", "KeyA"],
    ),
    cmd(
        C::ToolRect,
        "tool.rect",
        G::Tool,
        "Прямоугольник",
        &["квадрат", "фигура", "rectangle", "square"],
        &["KeyR"],
    ),
    cmd(
        C::ToolEllipse,
        "tool.ellipse",
        G::Tool,
        "Эллипс",
        &["круг", "овал", "ellipse", "circle"],
        &["KeyO"],
    ),
    cmd(
        C::ToolLine,
        "tool.line",
        G::Tool,
        "Линия",
        &["отрезок", "line"],
        &["KeyL"],
    ),
    cmd(
        C::ToolHand,
        "tool.hand",
        G::Tool,
        "Рука",
        &["панорама", "прокрутка", "hand", "pan"],
        &["KeyH"],
    ),
    cmd(
        C::ToolText,
        "tool.text",
        G::Tool,
        "Текст",
        &["надпись", "шрифт", "text", "type"],
        &["KeyT"],
    ),
    cmd(
        C::ToolPen,
        "tool.pen",
        G::Tool,
        "Перо",
        &["кривая", "путь", "pen", "path", "bezier"],
        &["KeyP"],
    ),
    cmd(
        C::ToolScale,
        "tool.scale",
        G::Tool,
        "Масштабирование",
        &["пропорционально", "размер", "scale", "resize"],
        &["KeyK"],
    ),
    cmd(
        C::ToolArrow,
        "tool.arrow",
        G::Tool,
        "Стрелка",
        &["указатель", "линия", "arrow"],
        &["Shift+KeyL"],
    ),
    cmd(
        C::HandHold,
        "tool.hand_hold",
        G::Tool,
        "Рука, пока клавиша нажата",
        &["пробел", "панорама", "hand", "space", "pan"],
        &["Space"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::ToolImage,
        "tool.image",
        "Поместить изображение",
        &["Mod+Shift+KeyK"],
    ),
    cmd(C::Undo, "edit.undo", "Отменить", &["Mod+KeyZ"]),
    Command {
        id: C::Redo,
        key: "edit.redo",
        name: "Повторить",
        defaults: &["Mod+Shift+KeyZ"],
        defaults_pc: &["Ctrl+KeyY"],
        defaults_mac: &[],
    },
    cmd(C::Copy, "edit.copy", "Копировать", &["Mod+KeyC"]),
    cmd(C::Cut, "edit.cut", "Вырезать", &["Mod+KeyX"]),
    cmd(C::Paste, "edit.paste", "Вставить", &["Mod+KeyV"]),
    cmd(
        C::CopyAsPng,
        "edit.copy_png",
        "Копировать как PNG",
        &["Mod+Shift+KeyC"],
    ),
    cmd(C::CopyAsSvg, "edit.copy_svg", "Копировать как SVG", &[]),
    cmd(C::Duplicate, "edit.duplicate", "Дублировать", &["Mod+KeyD"]),
    cmd(
        C::Delete,
        "edit.delete",
        "Удалить",
        &["Delete", "Backspace"],
    ),
````

на:

````rust
        C::ToolImage,
        "tool.image",
        G::Tool,
        "Поместить изображение",
        &["картинка", "фото", "image", "place", "picture"],
        &["Mod+Shift+KeyK"],
    ),
    cmd(
        C::Undo,
        "edit.undo",
        G::Edit,
        "Отменить",
        &["назад", "откат", "undo"],
        &["Mod+KeyZ"],
    ),
    cmd(
        C::Redo,
        "edit.redo",
        G::Edit,
        "Повторить",
        &["вернуть", "redo"],
        &["Mod+Shift+KeyZ"],
    )
    .pc(&["Ctrl+KeyY"]),
    cmd(
        C::Copy,
        "edit.copy",
        G::Edit,
        "Копировать",
        &["буфер", "copy"],
        &["Mod+KeyC"],
    ),
    cmd(
        C::Cut,
        "edit.cut",
        G::Edit,
        "Вырезать",
        &["буфер", "cut"],
        &["Mod+KeyX"],
    ),
    cmd(
        C::Paste,
        "edit.paste",
        G::Edit,
        "Вставить",
        &["буфер", "paste"],
        &["Mod+KeyV"],
    ),
    cmd(
        C::CopyAsPng,
        "edit.copy_png",
        G::Edit,
        "Копировать как PNG",
        &["картинка", "растр", "copy as png", "image"],
        &["Mod+Shift+KeyC"],
    ),
    cmd(
        C::CopyAsSvg,
        "edit.copy_svg",
        G::Edit,
        "Копировать как SVG",
        &["вектор", "код", "copy as svg", "vector"],
        &[],
    ),
    cmd(
        C::Duplicate,
        "edit.duplicate",
        G::Edit,
        "Дублировать",
        &["дубль", "копия", "duplicate", "clone"],
        &["Mod+KeyD"],
    ),
    cmd(
        C::Delete,
        "edit.delete",
        G::Edit,
        "Удалить",
        &["стереть", "убрать", "delete", "remove"],
        &["Delete", "Backspace"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::SelectAll,
        "edit.select_all",
        "Выделить всё",
        &["Mod+KeyA"],
    ),
    cmd(C::Escape, "edit.escape", "Отмена", &["Escape"]),
    cmd(C::Group, "object.group", "Сгруппировать", &["Mod+KeyG"]),
    cmd(
        C::Ungroup,
        "object.ungroup",
        "Разгруппировать",
        &["Mod+Shift+KeyG"],
    ),
    cmd(C::Flatten, "object.flatten", "Сплющить", &["Mod+KeyE"]),
    // Как в Figma: Alt+Shift+буква (Option+Shift на macOS), исключение — E.
    // Свободны на обеих платформах: Alt+Shift и Shift+E в таблице ничем не заняты.
````

на:

````rust
        C::SelectAll,
        "edit.select_all",
        G::Edit,
        "Выделить всё",
        &["все слои", "select all"],
        &["Mod+KeyA"],
    ),
    cmd(
        C::Escape,
        "edit.escape",
        G::Edit,
        "Отмена",
        &["снять выделение", "выход", "escape", "deselect"],
        &["Escape"],
    ),
    cmd(
        C::Group,
        "object.group",
        G::Object,
        "Сгруппировать",
        &["группа", "объединить", "group"],
        &["Mod+KeyG"],
    ),
    cmd(
        C::Ungroup,
        "object.ungroup",
        G::Object,
        "Разгруппировать",
        &["разобрать", "ungroup"],
        &["Mod+Shift+KeyG"],
    ),
    cmd(
        C::Flatten,
        "object.flatten",
        G::Object,
        "Сплющить",
        &["в вектор", "контуры", "flatten"],
        &["Mod+KeyE"],
    ),
    // Как в Figma: Alt+Shift+буква (Option+Shift на macOS), исключение — E.
    // Свободны на обеих платформах: Alt+Shift и Shift+E в таблице ничем не заняты.
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::BooleanUnion,
        "object.boolean_union",
        "Объединение",
        &["Alt+Shift+KeyU"],
    ),
    cmd(
        C::BooleanSubtract,
        "object.boolean_subtract",
        "Вычитание",
        &["Alt+Shift+KeyS"],
    ),
    cmd(
        C::BooleanIntersect,
        "object.boolean_intersect",
        "Пересечение",
        &["Alt+Shift+KeyI"],
    ),
    cmd(
        C::BooleanExclude,
        "object.boolean_exclude",
        "Исключение",
        &["Alt+Shift+KeyE"],
    ),
    cmd(
        C::OutlineStroke,
        "object.outline_stroke",
        "Контур обводки",
        &["Mod+Alt+KeyO"],
    ),
````

на:

````rust
        C::BooleanUnion,
        "object.boolean_union",
        G::Object,
        "Объединение",
        &["булева", "сложить", "union", "boolean"],
        &["Alt+Shift+KeyU"],
    )
    .toggle(),
    cmd(
        C::BooleanSubtract,
        "object.boolean_subtract",
        G::Object,
        "Вычитание",
        &["булева", "вычесть", "subtract", "boolean"],
        &["Alt+Shift+KeyS"],
    )
    .toggle(),
    cmd(
        C::BooleanIntersect,
        "object.boolean_intersect",
        G::Object,
        "Пересечение",
        &["булева", "общая часть", "intersect", "boolean"],
        &["Alt+Shift+KeyI"],
    )
    .toggle(),
    cmd(
        C::BooleanExclude,
        "object.boolean_exclude",
        G::Object,
        "Исключение",
        &["булева", "xor", "exclude", "boolean"],
        &["Alt+Shift+KeyE"],
    )
    .toggle(),
    cmd(
        C::OutlineStroke,
        "object.outline_stroke",
        G::Object,
        "Контур обводки",
        &["обводка в заливку", "outline stroke", "stroke"],
        &["Mod+Alt+KeyO"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::FrameSelection,
        "object.frame_selection",
        "Обернуть во фрейм",
        &["Mod+Alt+KeyG"],
    ),
````

на:

````rust
        C::FrameSelection,
        "object.frame_selection",
        G::Object,
        "Обернуть во фрейм",
        &["фрейм", "контейнер", "frame selection", "wrap"],
        &["Mod+Alt+KeyG"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::BringForward,
        "arrange.forward",
        "Переместить вперёд",
        &["Mod+BracketRight"],
    ),
````

на:

````rust
        C::BringForward,
        "arrange.forward",
        G::Arrange,
        "Переместить вперёд",
        &["выше", "порядок", "bring forward", "order"],
        &["Mod+BracketRight"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::SendBackward,
        "arrange.backward",
        "Переместить назад",
        &["Mod+BracketLeft"],
    ),
    // На Windows и Linux Ctrl+Alt — это AltGr (символы на многих раскладках),
    // поэтому там, как в Figma для Windows, — Ctrl+Shift+] / Ctrl+Shift+[.
    cmd_per_platform(
        C::BringToFront,
        "arrange.front",
        "На передний план",
        &["Mod+Alt+BracketRight"],
        &["Ctrl+Shift+BracketRight"],
    ),
    cmd_per_platform(
        C::SendToBack,
        "arrange.back",
        "На задний план",
        &["Mod+Alt+BracketLeft"],
        &["Ctrl+Shift+BracketLeft"],
    ),
    cmd(C::NudgeLeft, "nudge.left", "Сдвинуть влево", &["ArrowLeft"]),
    cmd(
        C::NudgeRight,
        "nudge.right",
        "Сдвинуть вправо",
        &["ArrowRight"],
    ),
    cmd(C::NudgeUp, "nudge.up", "Сдвинуть вверх", &["ArrowUp"]),
    cmd(C::NudgeDown, "nudge.down", "Сдвинуть вниз", &["ArrowDown"]),
    cmd(
        C::NudgeLeftBig,
        "nudge.left_big",
        "Сдвинуть влево на 10 px",
        &["Shift+ArrowLeft"],
    ),
````

на:

````rust
        C::SendBackward,
        "arrange.backward",
        G::Arrange,
        "Переместить назад",
        &["ниже", "порядок", "send backward", "order"],
        &["Mod+BracketLeft"],
    ),
    // На Windows и Linux Ctrl+Alt — это AltGr (символы на многих раскладках),
    // поэтому там, как в Figma для Windows, — Ctrl+Shift+] / Ctrl+Shift+[.
    cmd(
        C::BringToFront,
        "arrange.front",
        G::Arrange,
        "На передний план",
        &["наверх", "порядок", "bring to front", "order"],
        &[],
    )
    .per_platform(&["Mod+Alt+BracketRight"], &["Ctrl+Shift+BracketRight"]),
    cmd(
        C::SendToBack,
        "arrange.back",
        G::Arrange,
        "На задний план",
        &["вниз", "порядок", "send to back", "order"],
        &[],
    )
    .per_platform(&["Mod+Alt+BracketLeft"], &["Ctrl+Shift+BracketLeft"]),
    cmd(
        C::NudgeLeft,
        "nudge.left",
        G::Nudge,
        "Сдвинуть влево",
        &["сдвиг", "nudge", "left"],
        &["ArrowLeft"],
    ),
    cmd(
        C::NudgeRight,
        "nudge.right",
        G::Nudge,
        "Сдвинуть вправо",
        &["сдвиг", "nudge", "right"],
        &["ArrowRight"],
    ),
    cmd(
        C::NudgeUp,
        "nudge.up",
        G::Nudge,
        "Сдвинуть вверх",
        &["сдвиг", "nudge", "up"],
        &["ArrowUp"],
    ),
    cmd(
        C::NudgeDown,
        "nudge.down",
        G::Nudge,
        "Сдвинуть вниз",
        &["сдвиг", "nudge", "down"],
        &["ArrowDown"],
    ),
    cmd(
        C::NudgeLeftBig,
        "nudge.left_big",
        G::Nudge,
        "Сдвинуть влево на 10 px",
        &["сдвиг", "nudge", "left"],
        &["Shift+ArrowLeft"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::NudgeRightBig,
        "nudge.right_big",
        "Сдвинуть вправо на 10 px",
        &["Shift+ArrowRight"],
    ),
````

на:

````rust
        C::NudgeRightBig,
        "nudge.right_big",
        G::Nudge,
        "Сдвинуть вправо на 10 px",
        &["сдвиг", "nudge", "right"],
        &["Shift+ArrowRight"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::NudgeUpBig,
        "nudge.up_big",
        "Сдвинуть вверх на 10 px",
        &["Shift+ArrowUp"],
    ),
````

на:

````rust
        C::NudgeUpBig,
        "nudge.up_big",
        G::Nudge,
        "Сдвинуть вверх на 10 px",
        &["сдвиг", "nudge", "up"],
        &["Shift+ArrowUp"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::NudgeDownBig,
        "nudge.down_big",
        "Сдвинуть вниз на 10 px",
        &["Shift+ArrowDown"],
    ),
````

на:

````rust
        C::NudgeDownBig,
        "nudge.down_big",
        G::Nudge,
        "Сдвинуть вниз на 10 px",
        &["сдвиг", "nudge", "down"],
        &["Shift+ArrowDown"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::ZoomToFit,
        "view.fit_all",
        "Показать всё",
        &["Shift+Digit1"],
    ),
````

на:

````rust
        C::ZoomToFit,
        "view.fit_all",
        G::View,
        "Показать всё",
        &["масштаб", "вписать", "zoom to fit", "fit"],
        &["Shift+Digit1"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::ZoomToSelection,
        "view.fit_selection",
        "Показать выделение",
        &["Shift+Digit2"],
    ),
````

на:

````rust
        C::ZoomToSelection,
        "view.fit_selection",
        G::View,
        "Показать выделение",
        &["масштаб", "к выделению", "zoom to selection"],
        &["Shift+Digit2"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::Zoom100,
        "view.zoom_100",
        "Масштаб 100%",
        &["Shift+Digit0"],
    ),
````

на:

````rust
        C::Zoom100,
        "view.zoom_100",
        G::View,
        "Масштаб 100%",
        &["реальный размер", "zoom 100", "actual size"],
        &["Shift+Digit0"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::ZoomIn,
        "view.zoom_in",
        "Приблизить",
        &["Equal", "Shift+Equal", "NumpadAdd"],
    ),
````

на:

````rust
        C::ZoomIn,
        "view.zoom_in",
        G::View,
        "Приблизить",
        &["масштаб", "увеличить", "zoom in"],
        &["Equal", "Shift+Equal", "NumpadAdd"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::ZoomOut,
        "view.zoom_out",
        "Отдалить",
        &["Minus", "NumpadSubtract"],
    ),
````

на:

````rust
        C::ZoomOut,
        "view.zoom_out",
        G::View,
        "Отдалить",
        &["масштаб", "уменьшить", "zoom out"],
        &["Minus", "NumpadSubtract"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::ToggleVisible,
        "object.toggle_visible",
        "Скрыть или показать",
        &["Mod+Shift+KeyH"],
    ),
````

на:

````rust
        C::ToggleVisible,
        "object.toggle_visible",
        G::Object,
        "Скрыть или показать",
        &["видимость", "спрятать", "hide", "show", "visibility"],
        &["Mod+Shift+KeyH"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::ToggleLock,
        "object.toggle_lock",
        "Заблокировать или разблокировать",
        &["Mod+Shift+KeyL"],
    ),
````

на:

````rust
        C::ToggleLock,
        "object.toggle_lock",
        G::Object,
        "Заблокировать или разблокировать",
        &["замок", "закрепить", "lock", "unlock"],
        &["Mod+Shift+KeyL"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::ToggleUi,
        "view.toggle_ui",
        "Скрыть интерфейс",
        &["Mod+Backslash"],
    ),
````

на:

````rust
        C::ToggleUi,
        "view.toggle_ui",
        G::View,
        "Скрыть интерфейс",
        &["панели", "полный экран", "hide ui", "interface"],
        &["Mod+Backslash"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::FlipHorizontal,
        "object.flip_horizontal",
        "Отразить по горизонтали",
        &["Shift+KeyH"],
    ),
````

на:

````rust
        C::FlipHorizontal,
        "object.flip_horizontal",
        G::Object,
        "Отразить по горизонтали",
        &["зеркало", "перевернуть", "flip horizontal", "mirror"],
        &["Shift+KeyH"],
    ),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        C::FlipVertical,
        "object.flip_vertical",
        "Отразить по вертикали",
        &["Shift+KeyV"],
    ),
    cmd(C::Save, "file.save", "Сохранить", &["Mod+KeyS"]),
];

````

на:

````rust
        C::FlipVertical,
        "object.flip_vertical",
        G::Object,
        "Отразить по вертикали",
        &["зеркало", "перевернуть", "flip vertical", "mirror"],
        &["Shift+KeyV"],
    ),
    cmd(
        C::Save,
        "file.save",
        G::File,
        "Сохранить",
        &["запись", "save"],
        &["Mod+KeyS"],
    ),
    // Команды интерфейса. Ctrl/Cmd+N и Ctrl/Cmd+O у браузера (новое окно, открыть
    // файл страницей), поэтому у «Нового файла» и «Открыть…» сочетаний нет.
    cmd(
        C::UiMainMenu,
        "ui.main_menu",
        G::View,
        "Главное меню",
        &["меню", "поиск команд", "main menu", "menu"],
        &["Alt+Slash"],
    ),
    cmd(
        C::UiNewFile,
        "ui.new_file",
        G::File,
        "Новый файл",
        &["создать", "новый документ", "new file", "create"],
        &[],
    ),
    cmd(
        C::UiOpen,
        "ui.open",
        G::File,
        "Открыть…",
        &["импорт", "загрузить", "open", "import"],
        &[],
    ),
    cmd(
        C::UiOpenDisk,
        "ui.open_disk",
        G::File,
        "Открыть с диска…",
        &["файл на диске", "open from disk", "local file"],
        &[],
    ),
    cmd(
        C::UiSaveDisk,
        "ui.save_disk",
        G::File,
        "Сохранить на диск…",
        &["файл на диске", "save to disk", "save as"],
        &[],
    ),
    cmd(
        C::UiSaveCopy,
        "ui.save_copy",
        G::File,
        "Сохранить копию на диск…",
        &["скачать", "копия", "save copy", "download"],
        &[],
    ),
    // Ctrl/Cmd+Shift+E — «Экспорт…» в Figma.
    cmd(
        C::UiExport,
        "ui.export",
        G::File,
        "Экспорт…",
        &["выгрузить", "png", "svg", "pdf", "export"],
        &["Mod+Shift+KeyE"],
    ),
    cmd(
        C::UiAllFiles,
        "ui.all_files",
        G::File,
        "Все файлы",
        &[
            "назад к файлам",
            "список файлов",
            "all files",
            "back to files",
        ],
        &[],
    ),
    cmd(
        C::UiSettings,
        "ui.settings",
        G::Settings,
        "Настройки…",
        &["параметры", "preferences", "settings"],
        &[],
    ),
    cmd(
        C::UiShortcuts,
        "ui.shortcuts",
        G::Help,
        "Сочетания клавиш",
        &[
            "горячие клавиши",
            "раскладка",
            "keyboard shortcuts",
            "hotkeys",
        ],
        &[],
    ),
    cmd(
        C::UiAbout,
        "ui.about",
        G::Help,
        "О программе",
        &["версия", "справка", "about", "version"],
        &[],
    ),
    // Как в Figma: «Snap to pixel grid» — Ctrl/Cmd+Shift+'.
    cmd(
        C::UiPixelSnap,
        "ui.pixel_snap",
        G::Settings,
        "Привязка к пикселям",
        &[
            "пиксельная привязка",
            "целые пиксели",
            "snap to pixel grid",
            "pixel snap",
        ],
        &["Mod+Shift+Quote"],
    )
    .toggle(),
    cmd(
        C::UiThemeDark,
        "ui.theme_dark",
        G::Settings,
        "Тёмная тема",
        &["тема", "оформление", "dark theme", "theme"],
        &[],
    )
    .toggle(),
    cmd(
        C::UiThemeLight,
        "ui.theme_light",
        G::Settings,
        "Светлая тема",
        &["тема", "оформление", "light theme", "theme"],
        &[],
    )
    .toggle(),
    cmd(
        C::UiThemeSystem,
        "ui.theme_system",
        G::Settings,
        "Тема как в системе",
        &["тема", "оформление", "system theme", "theme"],
        &[],
    )
    .toggle(),
];

````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    pub fn name(self) -> &'static str {
        self.info().name
    }

````

на:

````rust
    pub fn name(self) -> &'static str {
        self.info().name
    }

    pub fn group(self) -> CommandGroup {
        self.info().group
    }

    /// Команда интерфейса (`ui.*`): её выполняет основной поток по событию
    /// `EditorEvent::UiCommand`.
    pub fn is_ui(self) -> bool {
        self.key().starts_with("ui.")
    }

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Сохранить файл (Ctrl/Cmd+S): запись в хранилище ведёт интерфейс.
    SaveRequested,
    /// Сообщение об ошибке для пользователя.
    Error(String),
````

на:

````rust
    /// Сохранить файл (Ctrl/Cmd+S): запись в хранилище ведёт интерфейс.
    SaveRequested,
    /// Вызвана команда интерфейса (`ui.*`): её выполняет основной поток.
    UiCommand(CommandId),
    /// Сообщение об ошибке для пользователя.
    Error(String),
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
pub use commands::{Command, CommandId, COMMANDS};
pub use editor::{Editor, EditorEvent, Settings, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
````

на:

````rust

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
pub use editor::{Editor, EditorEvent, Settings, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
````

В `crates/ferrum-editor/src/vector_select.rs` заменить:

````rust
            | C::ToggleUi
            | C::Save => return false,
            // Остальное — правки узлов: правка вектора кончается.
            _ => {
````

на:

````rust
            | C::ToggleUi
            | C::Save => return false,
            // Команды интерфейса правку не трогают (меню, настройки, файлы).
            c if c.is_ui() => return false,
            // Остальное — правки узлов: правка вектора кончается.
            _ => {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                EditorEvent::ToggleUi => self.events.push(CoreEvent::ToggleUi),
                EditorEvent::SaveRequested => self.events.push(CoreEvent::SaveRequested),
                EditorEvent::Error(message) => self.events.push(CoreEvent::Error { message }),
            }
````

на:

````rust
                EditorEvent::ToggleUi => self.events.push(CoreEvent::ToggleUi),
                EditorEvent::SaveRequested => self.events.push(CoreEvent::SaveRequested),
                EditorEvent::UiCommand(id) => self.events.push(CoreEvent::UiCommand {
                    key: id.key().to_string(),
                }),
                EditorEvent::Error(message) => self.events.push(CoreEvent::Error { message }),
            }
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
                key: c.key.to_string(),
                name: c.name.to_string(),
                group: c.key.split('.').next().unwrap_or("").to_string(),
                bindings: km.bindings(c.id).iter().map(|s| s.to_string()).collect(),
                labels: km
````

на:

````rust
                key: c.key.to_string(),
                name: c.name.to_string(),
                group: c.group.key().to_string(),
                keywords: c.keywords.iter().map(|k| k.to_string()).collect(),
                toggle: c.toggle,
                bindings: km.bindings(c.id).iter().map(|s| s.to_string()).collect(),
                labels: km
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust

/// Команда в таблице раскладки: ключ, название, группа (`tool`, `edit`…),
/// действующие сочетания (каноническая запись и подпись US) и умолчания.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
````

на:

````rust

/// Команда в таблице раскладки: ключ, название, группа (`tool`, `edit`…),
/// синонимы для поиска, признак переключателя, действующие сочетания
/// (каноническая запись и подпись US) и умолчания.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    pub name: String,
    pub group: String,
    pub bindings: Vec<String>,
    pub labels: Vec<String>,
````

на:

````rust
    pub name: String,
    pub group: String,
    pub keywords: Vec<String>,
    pub toggle: bool,
    pub bindings: Vec<String>,
    pub labels: Vec<String>,
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    /// Команда «Сохранить» (Ctrl/Cmd+S): записать файл в хранилище.
    SaveRequested,
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
    /// пунктирных направляющих.
````

на:

````rust
    /// Команда «Сохранить» (Ctrl/Cmd+S): записать файл в хранилище.
    SaveRequested,
    /// Вызвана команда интерфейса (`ui.*`: меню, файлы, настройки): её выполняет
    /// основной поток (спека подсистемы 2, раздел 3).
    UiCommand {
        key: String,
    },
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
    /// пунктирных направляющих.
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/app.spec.ts e2e/chrome.spec.ts e2e/settings.spec.ts --project=chromium`

Expected: PASS: fmt и clippy (нативно и wasm32) без замечаний; `ferrum-editor` и `ferrum-wasm` — 560 passed, 2 ignored (`command_table` — 5 тестов, `keymap` — 70 команд); wasm собран, `npm run typecheck` без ошибок, модульные и сборка — ок; E2E `app`, `chrome`, `settings` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts apps/web/e2e/chrome.spec.ts apps/web/e2e/settings.spec.ts apps/web/src/ui/keymap-ui.ts apps/web/unit/settings.test.ts crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/vector_select.rs crates/ferrum-editor/tests/command_table.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/boolean.rs crates/ferrum-wasm/tests/panels.rs crates/ferrum-wasm/tests/protocol.rs
git commit -m "команды: группа, синонимы и переключатели в таблице; команды интерфейса ui.*

Таблица COMMANDS получила явную группу (подменю главного меню), русские и
английские синонимы для поиска и признак переключателя — без них таблица не
собирается. Команды интерфейса (главное меню Alt+/, новый файл, открыть,
сохранить на диск, копия, экспорт, все файлы, настройки, сочетания, о
программе, привязка к пикселям, тема) лежат в той же таблице с ключами ui.*:
ядро сообщает о них событием UiCommand, правку вектора они не заканчивают.
KeymapEntry несёт группу из таблицы, синонимы и признак переключателя.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2: Команды меню ядра: пиксельная сетка, «Войти в правку», форматирование текста

Спека §4: подменю «Вид», «Текст» и «Вектор» — пиксельная сетка, жирный, курсив, подчёркнутый, выравнивание, войти в правку. Здесь — `view.pixel_grid` (Shift+'), `vector.edit` (Enter, `Editor::edit_object`: правка текста или вектора у единственного выделенного; удержанный Enter — нет), восемь команд текста (`Editor::text_command`); `text_format_key` ищет команду группы «Текст» в действующей раскладке; вне правки — по участкам всех выделенных текстов одним шагом. Команд 70 → 80.

**Files:**
- Modify: `crates/ferrum-editor/src/{commands,actions,editor,text_style,vector_select}.rs`
- Test: `crates/ferrum-editor/tests/menu_commands.rs` (новый), `crates/ferrum-editor/tests/{command_table,keymap}.rs` (команд 80), `crates/ferrum-wasm/tests/boolean.rs` (команд 80), `apps/web/e2e/app.spec.ts` (команд 80)

**Interfaces:**
- Consumes: `CommandGroup::{View, Text, Vector}`, `Settings::pixel_grid`, `start_text_edit`, `start_vector_edit`, `toggle_font`, `Keymap::lookup`.
- Produces: `CommandId::{PixelGrid, EditObject, TextBold, TextItalic, TextUnderline, TextStrike, TextAlignLeft, TextAlignCenter, TextAlignRight, TextAlignJustify}`; `Editor::edit_object()`, `Editor::text_command(CommandId)`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 70, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 80, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `crates/ferrum-editor/tests/command_table.rs` заменить:

````rust
    let groups: HashSet<_> = COMMANDS.iter().map(|c| c.group).collect();
    for g in CommandGroup::ALL {
        if g == CommandGroup::Text || g == CommandGroup::Vector {
            continue; // наполняются командами текста и правки вектора
        }
        assert!(groups.contains(&g), "группа {g:?} пуста");
    }
````

на:

````rust
    let groups: HashSet<_> = COMMANDS.iter().map(|c| c.group).collect();
    for g in CommandGroup::ALL {
        assert!(groups.contains(&g), "группа {g:?} пуста");
    }
````

В `crates/ferrum-editor/tests/command_table.rs` заменить:

````rust
            C::BooleanIntersect,
            C::BooleanExclude,
            C::UiPixelSnap,
            C::UiThemeDark,
````

на:

````rust
            C::BooleanIntersect,
            C::BooleanExclude,
            C::PixelGrid,
            C::UiPixelSnap,
            C::UiThemeDark,
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    (CommandId::UiExport, &["Mod+Shift+KeyE"]),
    (CommandId::UiPixelSnap, &["Mod+Shift+Quote"]),
];

````

на:

````rust
    (CommandId::UiExport, &["Mod+Shift+KeyE"]),
    (CommandId::UiPixelSnap, &["Mod+Shift+Quote"]),
    (CommandId::PixelGrid, &["Shift+Quote"]),
    (CommandId::EditObject, &["Enter"]),
    (CommandId::TextBold, &["Mod+KeyB"]),
    (CommandId::TextItalic, &["Mod+KeyI"]),
    (CommandId::TextUnderline, &["Mod+KeyU"]),
    (CommandId::TextStrike, &["Mod+Shift+KeyX"]),
    (CommandId::TextAlignLeft, &["Mod+Alt+KeyL"]),
    (CommandId::TextAlignCenter, &["Mod+Alt+KeyT"]),
    (CommandId::TextAlignRight, &["Mod+Alt+KeyR"]),
    (CommandId::TextAlignJustify, &["Mod+Alt+KeyJ"]),
];

````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 70);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 80);
}

````

Создать `crates/ferrum-editor/tests/menu_commands.rs`:

````rust
//! Команды главного меню, которых не было в раскладке (спека подсистемы 2,
//! раздел 4): пиксельная сетка, «Войти в правку», форматирование текста — в
//! правке и вне её, по действующей раскладке.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn font_of(h: &mut H, id: NodeId, at: usize) -> (String, String) {
    let p = h.props(id);
    let t = p.text.unwrap();
    let f = t.content.attrs_at(at).font_name(&t.props.style);
    (f.family, f.style)
}

/// «Привет мир»: первые три символа — Roboto, остальное — Inter Regular.
fn mixed_family_text(h: &mut H) -> NodeId {
    let id = add_text(h, 100.0, 100.0, "Привет мир", 24.0);
    h.ed.transact(|tx| {
        tx.format_text(
            id,
            0..3,
            &TextAttrs {
                font: Some("Roboto".into()),
                style: Some("Regular".into()),
                ..Default::default()
            },
            &[],
        )?;
        Ok(vec![id])
    })
    .unwrap();
    id
}

#[test]
fn pixel_grid_toggles_with_shift_quote() {
    let mut h = H::new();
    assert!(h.ed.settings().pixel_grid);
    h.ed.take_events();
    h.key("Quote", shift());
    assert!(!h.ed.settings().pixel_grid);
    assert!(
        h.ed.take_events().contains(&EditorEvent::NeedsRedraw),
        "сетка — часть кадра"
    );
    h.ed.execute(CommandId::PixelGrid);
    assert!(h.ed.settings().pixel_grid);
    assert_eq!(h.ed.step_count(), 0, "вид — не правка документа");
}

#[test]
fn edit_object_enters_vector_or_text_editing() {
    let mut h = text_h();
    let r = h.rect(None, 300.0, 100.0, 50.0, 50.0);
    h.ed.select(&[r]);
    h.ed.execute(CommandId::EditObject);
    assert!(h.ed.is_editing_vector());
    // В правке — ничего: правка та же.
    h.ed.execute(CommandId::EditObject);
    assert!(h.ed.is_editing_vector());
    h.key("Escape", none());
    h.key("Escape", none());
    let t = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.ed.select(&[t]);
    h.ed.execute(CommandId::EditObject);
    assert!(h.ed.is_editing_text());
}

#[test]
fn bold_outside_editing_toggles_whole_selected_texts_in_one_step() {
    let mut h = text_h();
    let id = mixed_family_text(&mut h);
    let other = add_text(&mut h, 100.0, 300.0, "Мир", 24.0);
    h.ed.select(&[id, other]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyB", ctrl());
    assert!(!h.ed.is_editing_text());
    assert_eq!(font_of(&mut h, id, 0), ("Roboto".into(), "Bold".into()));
    assert_eq!(font_of(&mut h, id, 5), ("Inter".into(), "Bold".into()));
    assert_eq!(font_of(&mut h, other, 1).1, "Bold");
    h.assert_one_step(steps, &before);
    // Всё уже жирное — снова обычный.
    h.ed.execute(CommandId::TextBold);
    assert_eq!(font_of(&mut h, id, 0), ("Roboto".into(), "Regular".into()));
    assert_eq!(font_of(&mut h, other, 1).1, "Regular");
}

#[test]
fn underline_and_align_from_the_menu_apply_to_the_selection() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.ed.select(&[id]);
    h.ed.execute(CommandId::TextUnderline);
    let t = h.props(id).text.unwrap();
    assert_eq!(t.props.style.decoration, Decoration::Underline);
    h.ed.execute(CommandId::TextAlignRight);
    assert_eq!(h.props(id).text.unwrap().props.align_h, AlignH::Right);
    // В правке — к фрагменту, правка продолжается.
    h.key("Enter", none());
    h.ed.execute(CommandId::TextAlignCenter);
    assert_eq!(h.props(id).text.unwrap().props.align_h, AlignH::Center);
    assert!(h.ed.is_editing_text());
}

#[test]
fn text_shortcuts_in_editing_follow_the_keymap() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    // Жирный переназначен на Ctrl+Shift+B: Ctrl+B в правке больше не действует.
    h.ed.keymap_mut().set_bindings(
        CommandId::TextBold,
        vec![Shortcut::parse("Ctrl+Shift+KeyB").unwrap()],
    );
    h.ed.select(&[id]);
    h.key("Enter", none());
    h.key("KeyB", ctrl());
    assert_eq!(font_of(&mut h, id, 0).1, "Regular");
    h.key("KeyB", ctrl().with_shift());
    assert_eq!(font_of(&mut h, id, 0).1, "Bold");
    assert!(h.ed.is_editing_text());
}
````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 70);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

на:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 80);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test menu_commands --test command_table --test keymap`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test boolean`
Expected: FAIL — редактор не собирается — нет `CommandId::{PixelGrid, EditObject, TextBold, TextItalic, TextUnderline, TextStrike, TextAlign*}` (`menu_commands` — 9 ошибок, `keymap` — 10, `command_table` — 1); ядро собирается, но `boolean_and_outline_commands_are_in_the_keymap` падает: `left: 70, right: 80`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::FlipHorizontal => self.mirror(Vec2::new(-1.0, 1.0)),
            C::FlipVertical => self.mirror(Vec2::new(1.0, -1.0)),
        }
    }
````

на:

````rust
            C::FlipHorizontal => self.mirror(Vec2::new(-1.0, 1.0)),
            C::FlipVertical => self.mirror(Vec2::new(1.0, -1.0)),
            C::PixelGrid => self.settings.pixel_grid = !self.settings.pixel_grid,
            C::EditObject => self.edit_object(),
            C::TextBold
            | C::TextItalic
            | C::TextUnderline
            | C::TextStrike
            | C::TextAlignLeft
            | C::TextAlignCenter
            | C::TextAlignRight
            | C::TextAlignJustify => self.text_command(cmd),
        }
    }

    /// «Войти в правку» (как Enter): единственный выделенный текст — правка
    /// текста, вектор или фигура — правка вектора.
    fn edit_object(&mut self) {
        if self.vector_edit.is_some() || self.text_edit.is_some() {
            return;
        }
        if let [id] = self.selection[..] {
            if !self.start_text_edit(id, crate::text_edit::EnterAt::All) {
                self.start_vector_edit(id);
            }
        }
    }
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// Сохранить файл в хранилище (интерфейс получает `EditorEvent::SaveRequested`).
    Save,
    // ——— Команды интерфейса (`ui.*`): их выполняет основной поток, ядро только
    // сообщает о вызове событием `EditorEvent::UiCommand` (спека подсистемы 2,
````

на:

````rust
    /// Сохранить файл в хранилище (интерфейс получает `EditorEvent::SaveRequested`).
    Save,
    /// Пиксельная сетка при зуме от 800% (Shift+', как в Figma).
    PixelGrid,
    /// Войти в правку выделенного: текст — правка текста, вектор и фигура — правка
    /// вектора (как Enter).
    EditObject,
    /// Стиль текста (в правке — выделенного фрагмента, иначе — всего текста
    /// выделенных узлов), как Ctrl/Cmd+B, I, U, Shift+X и выравнивание.
    TextBold,
    TextItalic,
    TextUnderline,
    TextStrike,
    TextAlignLeft,
    TextAlignCenter,
    TextAlignRight,
    TextAlignJustify,
    // ——— Команды интерфейса (`ui.*`): их выполняет основной поток, ядро только
    // сообщает о вызове событием `EditorEvent::UiCommand` (спека подсистемы 2,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        &["Mod+KeyS"],
    ),
    // Команды интерфейса. Ctrl/Cmd+N и Ctrl/Cmd+O у браузера (новое окно, открыть
    // файл страницей), поэтому у «Нового файла» и «Открыть…» сочетаний нет.
````

на:

````rust
        &["Mod+KeyS"],
    ),
    // Как в Figma: «Pixel grid» — Shift+'.
    cmd(
        C::PixelGrid,
        "view.pixel_grid",
        G::View,
        "Пиксельная сетка",
        &["сетка", "пиксели", "pixel grid", "grid"],
        &["Shift+Quote"],
    )
    .toggle(),
    // Enter — физическая клавиша правки (как Esc у жеста); в таблице — для меню
    // и подписи.
    cmd(
        C::EditObject,
        "vector.edit",
        G::Vector,
        "Войти в правку",
        &["редактировать", "точки", "узлы", "edit object", "edit"],
        &["Enter"],
    ),
    // Форматирование текста — как в справке Figma; в правке текста действует
    // та же раскладка.
    cmd(
        C::TextBold,
        "text.bold",
        G::Text,
        "Жирный",
        &["полужирный", "толщина", "bold"],
        &["Mod+KeyB"],
    ),
    cmd(
        C::TextItalic,
        "text.italic",
        G::Text,
        "Курсив",
        &["наклонный", "italic"],
        &["Mod+KeyI"],
    ),
    cmd(
        C::TextUnderline,
        "text.underline",
        G::Text,
        "Подчёркнутый",
        &["подчеркнуть", "underline"],
        &["Mod+KeyU"],
    ),
    cmd(
        C::TextStrike,
        "text.strike",
        G::Text,
        "Зачёркнутый",
        &["зачеркнуть", "strikethrough"],
        &["Mod+Shift+KeyX"],
    ),
    cmd(
        C::TextAlignLeft,
        "text.align_left",
        G::Text,
        "Выровнять по левому краю",
        &["выравнивание", "влево", "align left"],
        &["Mod+Alt+KeyL"],
    ),
    cmd(
        C::TextAlignCenter,
        "text.align_center",
        G::Text,
        "Выровнять по центру",
        &["выравнивание", "центр", "align center"],
        &["Mod+Alt+KeyT"],
    ),
    cmd(
        C::TextAlignRight,
        "text.align_right",
        G::Text,
        "Выровнять по правому краю",
        &["выравнивание", "вправо", "align right"],
        &["Mod+Alt+KeyR"],
    ),
    cmd(
        C::TextAlignJustify,
        "text.align_justify",
        G::Text,
        "Выровнять по ширине",
        &["выравнивание", "по ширине", "justify"],
        &["Mod+Alt+KeyJ"],
    ),
    // Команды интерфейса. Ctrl/Cmd+N и Ctrl/Cmd+O у браузера (новое окно, открыть
    // файл страницей), поэтому у «Нового файла» и «Открыть…» сочетаний нет.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            return;
        };
        if cmd == CommandId::HandHold {
            if !e.repeat && self.held.is_none() && self.tool.kind() != ToolKind::Hand {
````

на:

````rust
            return;
        };
        // Удержанная клавиша не входит в правку (как физический Enter выше).
        if cmd == CommandId::EditObject && e.repeat {
            return;
        }
        if cmd == CommandId::HandHold {
            if !e.repeat && self.held.is_none() && self.tool.kind() != ToolKind::Hand {
````

В `crates/ferrum-editor/src/text_style.rs` заменить:

````rust
};

use crate::editor::{Editor, EditorEvent};
use crate::input::Modifiers;
use crate::panel::{PanelGesture, Phase};
use crate::text::{fit_patch, TextError};
````

на:

````rust
};

use crate::commands::{CommandGroup, CommandId};
use crate::editor::{Editor, EditorEvent};
use crate::input::{KeyEvent, Modifiers};
use crate::panel::{PanelGesture, Phase};
use crate::text::{fit_patch, TextError};
````

В `crates/ferrum-editor/src/text_style.rs` заменить:

````rust
    /// (и вес, если переключается курсив). Все участки уже включены — выключает,
    /// иначе включает у всех (как подчёркивание). Начертания нет в семействе —
    /// участок не меняется.
    fn toggle_font(&mut self, which: FontToggle) -> Result<(), TextError> {
        let Some(t) = self.text_edit.as_ref() else {
            return Ok(());
        };
        let (id, range, draft) = (t.id, t.range(), t.draft.is_some());
        let usable = |ed: &Editor, f: &FontName| ed.fonts.borrow().status(f) != FontStatus::Missing;
        if range.is_empty() || draft {
            // Курсор и черновик: стиль один (следующего ввода или узла).
            let Some(s) = self.text_style_summary() else {
                return Ok(());
            };
            let old = FontName::new(s.family.value, s.style.value);
            let font = which.apply(&old, !which.is_on(&old));
            if !usable(self, &font) {
                return Ok(());
            }
            return self.edit_text_style(
                &TextStylePatch {
                    font: Some(font),
                    ..Default::default()
                },
                Phase::Commit,
                None,
            );
        }
        let Some(before) = self.doc_props(id) else {
            return Ok(());
        };
        let Some(td) = before.text.as_ref() else {
            return Ok(());
        };
        let d = &td.props.style;
        // Участки диапазона с их позициями и шрифтом.
        let mut runs: Vec<(Range<usize>, FontName, bool)> = Vec::new();
        let mut at = range.start;
        for r in td.content.slice(range) {
            let len = r.text.chars().count();
            runs.push((
                at..at + len,
                r.attrs.font_name(d),
                r.text.chars().any(|c| c != '\n'),
            ));
            at += len;
        }
        let on = runs.iter().filter(|r| r.2).all(|(_, f, _)| which.is_on(f));
        let mut content = td.content.clone();
        let mut changes: Vec<(Range<usize>, TextStylePatch)> = Vec::new();
        for (r, f, _) in runs {
            let font = which.apply(&f, !on);
            if font == f || !usable(self, &font) {
                continue;
            }
            let patch = TextStylePatch {
                font: Some(font),
                ..Default::default()
            };
            content = restyle(&content, r.clone(), &patch, d);
            changes.push((r, patch));
        }
        if changes.is_empty() {
            return Ok(());
        }
        let mut after = before.clone();
        after.text = Some(Arc::new(TextData {
            props: td.props.clone(),
            content,
        }));
        let fit = fit_patch(&mut self.fonts.borrow_mut(), &before, &after)
            .map_err(|_| TextError::Pending)?;
        self.cancel_text_preview();
        let selection = self.selection.to_vec();
        let (content, style) = (td.content.clone(), d.clone());
        self.ctx().commit(|tx, _| {
            for (r, patch) in &changes {
                restyle_ops(tx, id, &content, r.clone(), patch, &style)?;
            }
            tx.set_props(id, &fit)?;
            Ok(selection)
        });
````

на:

````rust
    /// (и вес, если переключается курсив). Все участки уже включены — выключает,
    /// иначе включает у всех (как подчёркивание). Начертания нет в семействе —
    /// участок не меняется. Вне правки диапазон — весь текст каждого выделенного
    /// текстового узла, одним шагом.
    fn toggle_font(&mut self, which: FontToggle) -> Result<(), TextError> {
        let usable = |ed: &Editor, f: &FontName| ed.fonts.borrow().status(f) != FontStatus::Missing;
        let edit = self
            .text_edit
            .as_ref()
            .map(|t| (t.id, t.range(), t.draft.is_some()));
        if let Some((_, range, draft)) = &edit {
            if range.is_empty() || *draft {
                // Курсор и черновик: стиль один (следующего ввода или узла).
                let Some(s) = self.text_style_summary() else {
                    return Ok(());
                };
                let old = FontName::new(s.family.value, s.style.value);
                let font = which.apply(&old, !which.is_on(&old));
                if !usable(self, &font) {
                    return Ok(());
                }
                return self.edit_text_style(
                    &TextStylePatch {
                        font: Some(font),
                        ..Default::default()
                    },
                    Phase::Commit,
                    None,
                );
            }
        }
        // Узлы и диапазоны: в правке — выделенный фрагмент, иначе весь текст.
        let targets: Vec<(NodeId, Range<usize>)> = match edit {
            Some((id, range, _)) => vec![(id, range)],
            None => self
                .style_targets(None)
                .into_iter()
                .filter_map(|id| {
                    let len = self.scene.node(id)?.props.text.as_ref()?.content.char_len();
                    Some((id, 0..len))
                })
                .collect(),
        };
        // Участки каждого узла с их позициями и шрифтом.
        type Runs = Vec<(Range<usize>, FontName, bool)>;
        let mut nodes: Vec<(NodeId, Props, Runs)> = Vec::new();
        for (id, range) in targets {
            let Some(before) = self.doc_props(id) else {
                continue;
            };
            let Some(td) = before.text.as_ref() else {
                continue;
            };
            let d = &td.props.style;
            let mut runs: Runs = Vec::new();
            let mut at = range.start;
            for r in td.content.slice(range) {
                let len = r.text.chars().count();
                runs.push((
                    at..at + len,
                    r.attrs.font_name(d),
                    r.text.chars().any(|c| c != '\n'),
                ));
                at += len;
            }
            nodes.push((id, before, runs));
        }
        let on = nodes
            .iter()
            .flat_map(|(_, _, runs)| runs.iter())
            .filter(|r| r.2)
            .all(|(_, f, _)| which.is_on(f));
        // Правки каждого узла: участки и подгонка размера под новую раскладку.
        type NodeChanges = (
            NodeId,
            TextContent,
            TextStyle,
            Vec<(Range<usize>, TextStylePatch)>,
            PropsPatch,
        );
        let mut all: Vec<NodeChanges> = Vec::new();
        for (id, before, runs) in nodes {
            let td = before.text.as_ref().expect("только тексты");
            let d = &td.props.style;
            let mut content = td.content.clone();
            let mut changes: Vec<(Range<usize>, TextStylePatch)> = Vec::new();
            for (r, f, _) in runs {
                let font = which.apply(&f, !on);
                if font == f || !usable(self, &font) {
                    continue;
                }
                let patch = TextStylePatch {
                    font: Some(font),
                    ..Default::default()
                };
                content = restyle(&content, r.clone(), &patch, d);
                changes.push((r, patch));
            }
            if changes.is_empty() {
                continue;
            }
            let mut after = before.clone();
            after.text = Some(Arc::new(TextData {
                props: td.props.clone(),
                content,
            }));
            let fit = fit_patch(&mut self.fonts.borrow_mut(), &before, &after)
                .map_err(|_| TextError::Pending)?;
            all.push((id, td.content.clone(), d.clone(), changes, fit));
        }
        if all.is_empty() {
            return Ok(());
        }
        self.cancel_text_preview();
        let selection = self.selection.to_vec();
        self.ctx().commit(|tx, _| {
            for (id, content, style, changes, fit) in &all {
                for (r, patch) in changes {
                    restyle_ops(tx, *id, content, r.clone(), patch, style)?;
                }
                tx.set_props(*id, fit)?;
            }
            Ok(selection)
        });
````

В `crates/ferrum-editor/src/text_style.rs` заменить:

````rust
    }

    /// Сочетания форматирования в правке. Возвращает, обработано ли.
    pub(crate) fn text_format_key(&mut self, code: &str, mods: Modifiers) -> bool {
        let platform = self.settings.platform;
        let primary = mods.primary(platform);
        let result = match code {
            "KeyB" | "KeyI" if primary && !mods.alt && !mods.shift => {
                self.toggle_font(if code == "KeyB" {
                    FontToggle::Bold
                } else {
                    FontToggle::Italic
                })
            }
            "KeyU" | "KeyX" if primary && !mods.alt && (code == "KeyU") != mods.shift => {
                let want = if code == "KeyU" {
                    Decoration::Underline
                } else {
````

на:

````rust
    }

    /// Команды форматирования текста (группа «Текст» таблицы): в правке — к
    /// выделенному фрагменту (у курсора — к следующему вводу), иначе — ко всему
    /// тексту выделенных узлов. Шрифты грузятся — ядро повторит команду.
    pub(crate) fn text_command(&mut self, cmd: CommandId) {
        use CommandId as C;
        let align = |a: AlignH| TextPropsPatch {
            align_h: Some(a),
            ..Default::default()
        };
        let result = match cmd {
            C::TextBold => self.toggle_font(FontToggle::Bold),
            C::TextItalic => self.toggle_font(FontToggle::Italic),
            C::TextUnderline | C::TextStrike => {
                let want = if cmd == C::TextUnderline {
                    Decoration::Underline
                } else {
````

В `crates/ferrum-editor/src/text_style.rs` заменить:

````rust
                )
            }
            "KeyL" | "KeyT" | "KeyR" | "KeyJ" if primary && mods.alt && !mods.shift => {
                let align = match code {
                    "KeyL" => AlignH::Left,
                    "KeyT" => AlignH::Center,
                    "KeyR" => AlignH::Right,
                    _ => AlignH::Justify,
                };
                self.edit_text_props(
                    &TextPropsPatch {
                        align_h: Some(align),
                        ..Default::default()
                    },
                    Phase::Commit,
                    None,
                )
            }
            "Tab" if !primary && !mods.alt => {
                // Уровень списка; вне списка Tab ничего не делает.
````

на:

````rust
                )
            }
            C::TextAlignLeft => self.edit_text_props(&align(AlignH::Left), Phase::Commit, None),
            C::TextAlignCenter => self.edit_text_props(&align(AlignH::Center), Phase::Commit, None),
            C::TextAlignRight => self.edit_text_props(&align(AlignH::Right), Phase::Commit, None),
            C::TextAlignJustify => {
                self.edit_text_props(&align(AlignH::Justify), Phase::Commit, None)
            }
            _ => return,
        };
        match result {
            Ok(()) => {}
            // Шрифты грузятся: ничего не сделано, ядро повторит команду.
            Err(TextError::Pending) => self.out.text_pending = true,
            Err(e) => self.out.events.push(EditorEvent::Error(e.to_string())),
        }
    }

    /// Сочетания форматирования в правке — по действующей раскладке (команды
    /// группы «Текст»), Tab — уровень списка. Возвращает, обработано ли.
    pub(crate) fn text_format_key(&mut self, code: &str, mods: Modifiers) -> bool {
        let key = KeyEvent::new(code, mods);
        if let Some(cmd) = self
            .keymap
            .lookup(&key)
            .filter(|c| c.group() == CommandGroup::Text)
        {
            self.text_command(cmd);
            return true;
        }
        let primary = mods.primary(self.settings.platform);
        let result = match code {
            "Tab" if !primary && !mods.alt => {
                // Уровень списка; вне списка Tab ничего не делает.
````

В `crates/ferrum-editor/src/vector_select.rs` заменить:

````rust
            | C::ZoomOut
            | C::ToggleUi
            | C::Save => return false,
            // Команды интерфейса правку не трогают (меню, настройки, файлы).
````

на:

````rust
            | C::ZoomOut
            | C::ToggleUi
            | C::PixelGrid
            | C::EditObject
            | C::Save => return false,
            // Команды интерфейса правку не трогают (меню, настройки, файлы).
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/app.spec.ts e2e/text-edit.spec.ts e2e/text-panel.spec.ts e2e/vectors.spec.ts --project=chromium`

Expected: PASS: `ferrum-editor` и `ferrum-wasm` — 565 passed, 2 ignored (`menu_commands` — 5 тестов, команд 80); модульные и сборка — ок; E2E `app`, `text-edit`, `text-panel`, `vectors` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/text_style.rs crates/ferrum-editor/src/vector_select.rs crates/ferrum-editor/tests/command_table.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/menu_commands.rs crates/ferrum-wasm/tests/boolean.rs
git commit -m "команды меню: пиксельная сетка, «Войти в правку», форматирование текста

Для подменю «Вид», «Текст» и «Вектор» главного меню в таблице появились
команды: пиксельная сетка (Shift+', как в Figma), «Войти в правку» (Enter:
текст — правка текста, вектор и фигура — правка вектора), жирный, курсив,
подчёркнутый, зачёркнутый и выравнивание (Ctrl/Cmd+B, I, U, Shift+X,
Alt+L/T/R/J). Форматирование в правке текста теперь идёт по действующей
раскладке, а вне правки применяется ко всему тексту выделенных узлов одним
шагом.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3: Состояние команд в ядре и событие `CommandStates`

Спека §3: ядро считает `enabled` и `checked` каждой команды и шлёт только изменившиеся после шага, который мог их изменить; недоступная команда при вызове ничего не делает. Здесь — `command_state.rs` (один проход по выделению, правила доступности и отметок), кэш по `StatesKey`, без пересчёта во время жеста; `Settings` + `theme: ThemeChoice`, `ui_hidden`; `view.toggle_ui` — переключатель; `Editor::run` начинается с проверки доступности; протокол — `CommandStateDto`, `CoreEvent::CommandStates` (одно на пачку, `Core::push_command_states`), `SetTheme.system`; `Core::new` и `replace_document` шлют все состояния; замер в `editor_report`.

**Files:**
- Create: `crates/ferrum-editor/src/command_state.rs`
- Modify: `crates/ferrum-editor/src/{editor,actions,commands,lib}.rs`, `crates/ferrum-editor/examples/editor_report.rs`, `crates/ferrum-wasm/src/{core,protocol}.rs`
- Test: `crates/ferrum-editor/tests/command_state.rs` (новый), `crates/ferrum-editor/tests/{boolean,command_table}.rs` (булева операция над одним слоем — недоступна, без сообщения), `crates/ferrum-wasm/tests/{panels,protocol,fonts}.rs`

**Interfaces:**
- Consumes: `Selection::version`, `Scene::change_counter`, `Document::{can_undo, can_redo, is_read_only}`, `Settings`, таблица `COMMANDS`.
- Produces: `pub struct CommandState { enabled: bool, checked: Option<bool> }`; `Editor::{command_state(id), command_states() -> Vec<CommandState>, take_command_states() -> Option<Vec<(CommandId, CommandState)>>, reset_command_states()}`; `ThemeChoice { Dark, Light, System }`; `EditorEvent::CommandStates(Vec<(CommandId, CommandState)>)`; `CommandStateDto { key, enabled, checked? }`; `CoreEvent::CommandStates { changed }`; `Command::SetTheme { theme, system? }`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-editor/tests/boolean.rs` заменить:

````rust

#[test]
fn single_layers_and_frames_are_refused_with_a_message() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
````

на:

````rust

#[test]
fn single_layers_are_unavailable_and_frames_are_refused_with_a_message() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
````

В `crates/ferrum-editor/tests/boolean.rs` заменить:

````rust
    h.ed.select(&[a]);
    h.ed.take_events();
    h.key("KeyU", alt_shift());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(
        errors(&mut h),
        vec!["Для булевой операции выделите хотя бы два слоя"]
    );
    h.ed.select(&[a, f]);
    h.key("KeyU", alt_shift());
````

на:

````rust
    h.ed.select(&[a]);
    h.ed.take_events();
    // Один слой: команда недоступна (как в Figma) — ни шага, ни сообщения.
    assert!(!h.ed.command_state(CommandId::BooleanUnion).enabled);
    h.key("KeyU", alt_shift());
    assert_eq!(h.ed.step_count(), steps);
    assert!(errors(&mut h).is_empty());
    h.ed.select(&[a, f]);
    h.key("KeyU", alt_shift());
````

Создать `crates/ferrum-editor/tests/command_state.rs`:

````rust
//! Состояние команд (спека подсистемы 2, раздел 3): доступна ли команда и
//! отмечена ли — для каждой группы ситуаций; события только с изменениями;
//! недоступная команда ничего не делает.

mod common;

use std::collections::BTreeMap;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

use CommandId as C;

fn on(h: &H, id: CommandId) -> bool {
    h.ed.command_state(id).enabled
}

fn checked(h: &H, id: CommandId) -> Option<bool> {
    h.ed.command_state(id).checked
}

/// Состояния из событий с прошлого вызова (последнее у каждой команды).
/// История без шагов создания стенда.
fn fresh(h: &mut H) {
    h.ed.document_mut_for_tests().clear_history();
}

fn changed(h: &mut H) -> BTreeMap<CommandId, CommandState> {
    let mut out = BTreeMap::new();
    for e in h.ed.take_events() {
        if let EditorEvent::CommandStates(list) = e {
            out.extend(list);
        }
    }
    out
}

#[test]
fn only_toggles_have_a_check_mark() {
    let mut h = text_h();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    let t = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    for sel in [vec![], vec![a], vec![a, t]] {
        h.ed.select(&sel);
        for (c, s) in COMMANDS.iter().zip(h.ed.command_states()) {
            assert_eq!(s.checked.is_some(), c.toggle, "{}", c.key);
            assert_eq!(s, h.ed.command_state(c.id), "{}", c.key);
        }
    }
}

#[test]
fn nothing_selected() {
    let mut h = H::new();
    h.rect(None, 0.0, 0.0, 50.0, 50.0);
    h.ed.select(&[]);
    for c in [
        C::Copy,
        C::Cut,
        C::Duplicate,
        C::Delete,
        C::Group,
        C::Ungroup,
        C::FrameSelection,
        C::Flatten,
        C::OutlineStroke,
        C::BooleanUnion,
        C::BringForward,
        C::SendToBack,
        C::NudgeLeft,
        C::ToggleVisible,
        C::ToggleLock,
        C::FlipHorizontal,
        C::CopyAsPng,
        C::CopyAsSvg,
        C::ZoomToSelection,
        C::EditObject,
        C::TextBold,
        C::TextAlignCenter,
    ] {
        assert!(!on(&h, c), "{c:?} без выделения недоступна");
    }
    for c in [
        C::Paste,
        C::SelectAll,
        C::ZoomToFit,
        C::ZoomIn,
        C::ToolRect,
        C::Escape,
        C::Save,
        C::PixelGrid,
        C::UiMainMenu,
        C::UiSettings,
    ] {
        assert!(on(&h, c), "{c:?} доступна всегда");
    }
}

#[test]
fn one_shape_selected() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    h.ed.select(&[a]);
    for c in [
        C::Copy,
        C::Cut,
        C::Duplicate,
        C::Delete,
        C::Group,
        C::FrameSelection,
        C::Flatten,
        C::BringToFront,
        C::NudgeDownBig,
        C::FlipVertical,
        C::ZoomToSelection,
        C::EditObject,
    ] {
        assert!(on(&h, c), "{c:?}");
    }
    // Разгруппировать нечего; булевой операции нужен второй слой; текста нет.
    for c in [C::Ungroup, C::BooleanUnion, C::BooleanExclude, C::TextBold] {
        assert!(!on(&h, c), "{c:?}");
    }
    // Заблокированный — не правится.
    h.set(
        a,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    h.ed.select(&[a]);
    assert!(!on(&h, C::EditObject));
}

#[test]
fn groups_and_booleans() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    let b = h.rect(None, 30.0, 0.0, 50.0, 50.0);
    h.ed.select(&[a, b]);
    assert!(on(&h, C::BooleanUnion) && on(&h, C::BooleanSubtract));
    assert_eq!(checked(&h, C::BooleanUnion), Some(false));
    h.ed.execute(C::BooleanSubtract);
    let g = h.sel()[0];
    // Выделена булева группа: операция отмечена, разгруппировать можно.
    assert_eq!(checked(&h, C::BooleanSubtract), Some(true));
    assert_eq!(checked(&h, C::BooleanUnion), Some(false));
    assert!(on(&h, C::Ungroup) && on(&h, C::BooleanIntersect));
    // Группа вместе с прямоугольником — ничего не отмечено.
    let c = h.rect(None, 200.0, 0.0, 50.0, 50.0);
    h.ed.select(&[g, c]);
    assert_eq!(checked(&h, C::BooleanSubtract), Some(false));
    assert!(on(&h, C::Ungroup));
    h.ed.select(&[c]);
    h.ed.execute(C::Group);
    assert!(on(&h, C::Ungroup), "обычная группа");
    assert!(!on(&h, C::BooleanUnion), "один слой — не булева группа");
}

#[test]
fn history() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    fresh(&mut h);
    h.ed.select(&[a]);
    assert!(!on(&h, C::Undo) && !on(&h, C::Redo));
    h.ed.execute(C::NudgeRight);
    assert!(on(&h, C::Undo) && !on(&h, C::Redo));
    h.ed.execute(C::Undo);
    assert!(on(&h, C::Redo));
}

#[test]
fn text_editing_and_text_selection() {
    let mut h = text_h();
    let t = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.ed.select(&[t]);
    assert!(on(&h, C::TextBold) && on(&h, C::TextAlignJustify));
    assert!(on(&h, C::EditObject));
    h.key("Enter", none());
    assert!(h.ed.is_editing_text());
    assert!(on(&h, C::Copy) && on(&h, C::Cut) && on(&h, C::TextUnderline));
    assert!(!on(&h, C::EditObject), "уже в правке");
}

#[test]
fn vector_editing_keeps_point_commands() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    h.ed.select(&[a]);
    assert!(h.ed.start_vector_edit(a));
    assert!(on(&h, C::Delete) && on(&h, C::NudgeLeft) && on(&h, C::Undo));
    assert!(!on(&h, C::EditObject));
}

#[test]
fn read_only_document_disables_edits_silently() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    h.ed.select(&[a]);
    fresh(&mut h);
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    h.ed.select(&[a]);
    let steps = h.ed.step_count();
    for c in [
        C::Delete,
        C::Cut,
        C::Paste,
        C::Duplicate,
        C::Group,
        C::NudgeUp,
        C::ToggleLock,
        C::EditObject,
    ] {
        assert!(!on(&h, c), "{c:?} в документе только для чтения");
    }
    assert!(on(&h, C::Copy) && on(&h, C::CopyAsPng) && on(&h, C::ZoomToSelection));
    // Недоступная команда — ни шага, ни сообщения.
    let before = h.state();
    h.ed.take_events();
    h.key("Delete", none());
    h.ed.execute(C::Duplicate);
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    assert!(h.ed.take_events().is_empty());
}

#[test]
fn disabled_commands_do_nothing() {
    let mut h = H::new();
    h.rect(None, 0.0, 0.0, 50.0, 50.0);
    fresh(&mut h);
    h.ed.select(&[]);
    h.ed.take_events();
    let steps = h.ed.step_count();
    for c in [C::Delete, C::CopyAsPng, C::Group, C::Copy, C::Undo] {
        h.ed.execute(c);
    }
    let ev = h.ed.take_events();
    assert!(ev.is_empty(), "ни событий буфера, ни ошибок: {ev:?}");
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn view_and_settings_toggles() {
    let mut h = H::new();
    assert_eq!(checked(&h, C::PixelGrid), Some(true));
    h.ed.execute(C::PixelGrid);
    assert_eq!(checked(&h, C::PixelGrid), Some(false));
    assert_eq!(checked(&h, C::ToggleUi), Some(false));
    h.ed.execute(C::ToggleUi);
    assert_eq!(checked(&h, C::ToggleUi), Some(true));
    assert_eq!(checked(&h, C::UiPixelSnap), Some(true));
    assert_eq!(checked(&h, C::UiThemeDark), Some(true));
    let mut s = h.ed.settings().clone();
    s.pixel_snap = false;
    s.theme = ThemeChoice::System;
    h.ed.set_settings(s);
    assert_eq!(checked(&h, C::UiPixelSnap), Some(false));
    assert_eq!(
        [C::UiThemeDark, C::UiThemeLight, C::UiThemeSystem].map(|c| checked(&h, c)),
        [Some(false), Some(false), Some(true)]
    );
}

#[test]
fn events_carry_only_changes_after_steps_that_may_change_them() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    fresh(&mut h);
    h.ed.select(&[]);
    h.ed.reset_command_states();
    h.ed.set_hover(None);
    // Первое — все команды таблицы.
    assert_eq!(changed(&mut h).len(), COMMANDS.len());
    // Наведение и движение указателя состояний не меняют — событий нет.
    h.hover(300.0, 300.0);
    h.hover(20.0, 20.0);
    assert!(changed(&mut h).is_empty());
    // Выделение: только то, что стало доступно.
    h.ed.select(&[a]);
    let ch = changed(&mut h);
    assert!(ch.contains_key(&C::Delete) && ch.contains_key(&C::Group));
    assert!(!ch.contains_key(&C::Paste) && !ch.contains_key(&C::ZoomIn));
    assert!(ch.values().all(|s| s.enabled));
    // Жест: во время перетаскивания событий нет, после отпускания — отмена доступна.
    h.down(20.0, 20.0, none());
    h.mv(60.0, 20.0, none());
    assert!(
        changed(&mut h).is_empty(),
        "во время жеста состояния не считаются"
    );
    h.up(60.0, 20.0, none());
    let ch = changed(&mut h);
    assert_eq!(ch.get(&C::Undo).map(|s| s.enabled), Some(true));
    // Вид: отметка пиксельной сетки — одна команда.
    h.ed.execute(C::PixelGrid);
    let ch = changed(&mut h);
    assert_eq!(ch.keys().copied().collect::<Vec<_>>(), vec![C::PixelGrid]);
}
````

В `crates/ferrum-editor/tests/command_table.rs` заменить:

````rust
            C::BooleanIntersect,
            C::BooleanExclude,
            C::PixelGrid,
            C::UiPixelSnap,
````

на:

````rust
            C::BooleanIntersect,
            C::BooleanExclude,
            C::ToggleUi,
            C::PixelGrid,
            C::UiPixelSnap,
````

В `crates/ferrum-wasm/tests/fonts.rs` заменить:

````rust
    c.command(Command::SetTheme {
        theme: Theme::Light,
    });
    // Камера примера (зум 0.5, центр 720×512) показывает x 320…1120, y 212…812.
````

на:

````rust
    c.command(Command::SetTheme {
        theme: Theme::Light,
        system: None,
    });
    // Камера примера (зум 0.5, центр 720×512) показывает x 320…1120, y 212…812.
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
    assert_eq!(document_steps(&core), 0.0);
}
````

на:

````rust
    assert_eq!(document_steps(&core), 0.0);
}

fn states(ev: &[CoreEvent]) -> Vec<Vec<CommandStateDto>> {
    ev.iter()
        .filter_map(|e| match e {
            CoreEvent::CommandStates { changed } => Some(changed.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn command_states_arrive_whole_then_as_changes() {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    let all = states(&core.take_events());
    assert_eq!(all.len(), 1, "одно событие");
    assert_eq!(
        all[0].len(),
        ferrum_editor::COMMANDS.len(),
        "до документа — все"
    );
    let grid = all[0].iter().find(|s| s.key == "view.pixel_grid").unwrap();
    assert_eq!((grid.enabled, grid.checked), (true, Some(true)));
    let delete = all[0].iter().find(|s| s.key == "edit.delete").unwrap();
    assert_eq!((delete.enabled, delete.checked), (false, None));
    // Новый документ — снова все, одним событием в пачке.
    core.command(Command::Demo { nodes: 10 });
    let ev = core.take_events();
    let got = states(&ev);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].len(), ferrum_editor::COMMANDS.len());
    // Выделение: только изменившиеся, удаление стало доступно.
    core.command(Command::Execute {
        command: "edit.select_all".into(),
    });
    let got = states(&core.take_events());
    assert_eq!(got.len(), 1, "изменения пачки — одним событием");
    assert!(got[0].len() < ferrum_editor::COMMANDS.len());
    assert!(got[0].iter().any(|s| s.key == "edit.delete" && s.enabled));
    // Тема «как в системе» — отметка в меню.
    core.command(Command::SetTheme {
        theme: Theme::Light,
        system: Some(true),
    });
    let got = states(&core.take_events());
    let checked = |key: &str| got[0].iter().find(|s| s.key == key).and_then(|s| s.checked);
    assert_eq!(checked("ui.theme_system"), Some(true));
    assert_eq!(checked("ui.theme_dark"), Some(false));
}

#[test]
fn disabled_command_from_the_interface_is_silent() {
    let mut core = demo(10);
    core.take_events();
    core.command(Command::Execute {
        command: "edit.delete".into(),
    });
    let ev = core.take_events();
    assert!(errors(&ev).is_empty() && ev.is_empty(), "{ev:?}");
    assert_eq!(document_steps(&core), 0.0);
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        Command::SetTheme {
            theme: Theme::Light,
        },
        Command::Paste { bytes: bytes(&[1]) },
````

на:

````rust
        Command::SetTheme {
            theme: Theme::Light,
            system: Some(true),
        },
        Command::Paste { bytes: bytes(&[1]) },
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
            key: "ui.main_menu".into(),
        },
        CoreEvent::Keymap {
            commands: vec![KeymapEntry {
````

на:

````rust
            key: "ui.main_menu".into(),
        },
        CoreEvent::CommandStates {
            changed: vec![
                CommandStateDto {
                    key: "edit.undo".into(),
                    enabled: false,
                    checked: None,
                },
                CommandStateDto {
                    key: "view.pixel_grid".into(),
                    enabled: true,
                    checked: Some(true),
                },
            ],
        },
        CoreEvent::Keymap {
            commands: vec![KeymapEntry {
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test command_state --test boolean --test command_table`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test panels --test protocol --test fonts`
Expected: FAIL — не собирается: редактор — нет `CommandState`, `ThemeChoice`, `Settings::theme`, методов `Editor::{command_state, command_states, reset_command_states}`, варианта `EditorEvent::CommandStates` (`command_state` — 9 ошибок, `boolean` — 1); ядро — нет `CommandStateDto`, `CoreEvent::CommandStates`, поля `system` у `Command::SetTheme` (`protocol` — 4, `panels` — 3, `fonts` — 1)

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
    boolean_child_drag(&mut report);
    scale_selection(&mut report);

    println!("| Метрика | Значение | Цель | Итог |");
````

на:

````rust
    boolean_child_drag(&mut report);
    scale_selection(&mut report);
    command_states(&mut report);

    println!("| Метрика | Значение | Цель | Итог |");
````

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
        std::process::exit(1);
    }
}

````

на:

````rust
        std::process::exit(1);
    }
}

/// Состояние команд (подсистема 2, этап 1): после шага на странице из 100 000
/// узлов ядро пересчитывает доступность и отметки всех команд и сравнивает с
/// отправленными. Замер — полный пересчёт с разницей (как после смены
/// выделения или шага истории), цель — 1 мс; справочно — при 10 000 и при всех
/// выделенных (проход по выделению — один).
fn command_states(report: &mut Report) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids: Vec<NodeId> = doc.transact(page, vec![], |p| fill(p, NODES)).unwrap();
    let mut ed = Editor::new(doc, Some(page), VIEWPORT, 1.0, Platform::Mac).unwrap();
    let leaf = ids[1];
    let measure = |ed: &mut Editor, sel: &[NodeId]| {
        ed.select(sel);
        let mut runs = Vec::new();
        let mut all = true;
        for _ in 0..COMMIT_RUNS * 4 {
            // Шаг: сдвиг выделенного (история и сцена меняются).
            ed.execute(CommandId::NudgeRight);
            ed.take_events();
            ed.reset_command_states();
            let t = Instant::now();
            let changed = ed.take_command_states();
            runs.push(t.elapsed());
            all &= changed.is_some_and(|c| c.len() == ferrum_editor::COMMANDS.len());
        }
        (runs, all)
    };
    let (one, ok) = measure(&mut ed, &[leaf]);
    report.gate_median(
        "Состояния команд после шага, 100 000 узлов, выделен 1",
        &one,
        1.0,
    );
    report.check(
        "Состояния команд: посчитаны все команды таблицы",
        ok,
        format!("{}", ferrum_editor::COMMANDS.len()),
        "все",
    );
    let many: Vec<NodeId> = ids
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 100 != 0)
        .map(|(_, id)| *id)
        .take(BIG_SELECTION)
        .collect();
    let (big, _) = measure(&mut ed, &many);
    report.info(
        "Состояния команд после шага, выделено 10 000 (медиана)",
        ms(median(big)),
    );
    let (whole, _) = measure(&mut ed, &ids);
    report.info(
        "Состояния команд после шага, выделено всё — 100 000 (медиана)",
        ms(median(whole)),
    );
}

````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
    pub(crate) fn run(&mut self, cmd: CommandId) {
        use CommandId as C;
        // Команда с зажатой кнопкой: жест отменяется, а не записывается.
        self.cancel_stale_vector_drag();
````

на:

````rust
    pub(crate) fn run(&mut self, cmd: CommandId) {
        use CommandId as C;
        // Недоступная команда ничего не делает: ни шага, ни сообщения (спека
        // подсистемы 2, раздел 3) — так же, как её пункт в меню.
        if !self.command_state(cmd).enabled {
            return;
        }
        // Команда с зажатой кнопкой: жест отменяется, а не записывается.
        self.cancel_stale_vector_drag();
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
                },
            ),
            C::ToggleUi => self.out.events.push(EditorEvent::ToggleUi),
            C::Save => self.out.events.push(EditorEvent::SaveRequested),
            C::UiMainMenu
````

на:

````rust
                },
            ),
            C::ToggleUi => {
                self.settings.ui_hidden = !self.settings.ui_hidden;
                self.out.events.push(EditorEvent::ToggleUi);
            }
            C::Save => self.out.events.push(EditorEvent::SaveRequested),
            C::UiMainMenu
````

Создать `crates/ferrum-editor/src/command_state.rs`:

````rust
//! Состояние команд (спека подсистемы 2, раздел 3): доступна ли команда и
//! отмечена ли (у переключателей). Считает ядро — основной поток только рисует
//! меню, тулбар и палитру по таблице `ключ → состояние`. Интерфейс получает
//! только изменившиеся команды и только после шага, который мог их изменить
//! (выделение, история, инструмент, вид, режим правки, «только чтение»).
//! Недоступная команда при вызове ничего не делает: ни шага, ни сообщения.

use ferrum_doc::{BooleanOp, NodeKind};

use crate::commands::{CommandId, COMMANDS};
use crate::editor::{Editor, ThemeChoice};
use crate::selection::locked;

/// Состояние команды: доступна ли и (у переключателя) отмечена ли.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandState {
    pub enabled: bool,
    /// `None` — команда не переключатель.
    pub checked: Option<bool>,
}

/// Всё, от чего зависят состояния: пока ключ тот же, пересчитывать нечего.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StatesKey {
    selection: u64,
    scene: u64,
    scene_epoch: u64,
    can_undo: bool,
    can_redo: bool,
    read_only: bool,
    text_edit: bool,
    vector_edit: bool,
    settings: crate::editor::Settings,
}

/// Что о выделении нужно состояниям — за один проход.
#[derive(Debug, Default)]
struct Facts {
    count: usize,
    /// Есть что разгруппировать: группа, фрейм или булева группа с детьми.
    ungroupable: bool,
    any_boolean: bool,
    /// Все выделенные — булевы группы с одной операцией.
    boolean_op: Option<BooleanOp>,
    has_text: bool,
    /// Один выделенный узел, у которого есть правка (текст, вектор, фигура).
    editable: bool,
}

/// Кэш отправленного интерфейсу: ключ и состояния в порядке `COMMANDS`.
#[derive(Debug, Default)]
pub(crate) struct StatesCache {
    key: Option<StatesKey>,
    sent: Vec<CommandState>,
}

impl Editor {
    fn states_key(&self) -> StatesKey {
        StatesKey {
            selection: self.selection.version(),
            scene: self.scene.change_counter(),
            scene_epoch: self.scene_epoch,
            can_undo: self.doc.can_undo(),
            can_redo: self.doc.can_redo(),
            read_only: self.doc.is_read_only(),
            text_edit: self.text_edit.is_some(),
            vector_edit: self.vector_edit.is_some(),
            settings: self.settings.clone(),
        }
    }

    fn facts(&self) -> Facts {
        let mut f = Facts {
            count: self.selection.len(),
            ..Default::default()
        };
        let mut same_op = true;
        for (i, id) in self.selection.iter().enumerate() {
            let Some(slot) = self.scene.slot(*id) else {
                continue;
            };
            let n = self.scene.at(slot);
            let kind = n.props.kind;
            if matches!(kind, NodeKind::Group | NodeKind::Frame | NodeKind::Boolean)
                && !n.children.is_empty()
            {
                f.ungroupable = true;
            }
            if kind == NodeKind::Boolean {
                f.any_boolean = true;
                let op = n.props.boolean_op.unwrap_or_default();
                if i == 0 {
                    f.boolean_op = Some(op);
                } else if f.boolean_op != Some(op) {
                    same_op = false;
                }
            } else {
                same_op = false;
            }
            if kind == NodeKind::Text {
                f.has_text = true;
            }
            if f.count == 1 {
                f.editable = matches!(
                    kind,
                    NodeKind::Text
                        | NodeKind::Vector
                        | NodeKind::Rect
                        | NodeKind::Ellipse
                        | NodeKind::Line
                ) && !locked(&self.scene, slot);
            }
            // Дальше ничего не изменится: проход по большому выделению кончается.
            if f.ungroupable && f.has_text && !same_op {
                break;
            }
        }
        if !same_op {
            f.boolean_op = None;
        }
        f
    }

    fn state_with(&self, id: CommandId, f: &Facts) -> CommandState {
        use CommandId as C;
        let ro = self.doc.is_read_only();
        let edit = !ro;
        let sel = f.count > 0;
        let text = self.text_edit.is_some();
        let vector = self.vector_edit.is_some();
        let checked = |on: bool| Some(on);
        let (enabled, checked) = match id {
            C::ToolMove
            | C::ToolFrame
            | C::ToolRect
            | C::ToolEllipse
            | C::ToolLine
            | C::ToolHand
            | C::ToolText
            | C::ToolPen
            | C::ToolArrow
            | C::ToolScale
            | C::HandHold
            | C::ToolImage
            | C::SelectAll
            | C::Escape
            | C::ZoomToFit
            | C::Zoom100
            | C::ZoomIn
            | C::ZoomOut
            | C::Save => (true, None),
            // Путь пера отменяется по точке, даже если истории ещё нет.
            C::Undo => (self.doc.can_undo() || vector, None),
            C::Redo => (self.doc.can_redo(), None),
            C::Copy => (sel || text, None),
            C::Cut => (edit && (sel || text), None),
            C::Paste => (edit, None),
            C::CopyAsPng | C::CopyAsSvg | C::ZoomToSelection => (sel, None),
            // В правке вектора удаление и сдвиг — у точек.
            C::Delete
            | C::NudgeLeft
            | C::NudgeRight
            | C::NudgeUp
            | C::NudgeDown
            | C::NudgeLeftBig
            | C::NudgeRightBig
            | C::NudgeUpBig
            | C::NudgeDownBig => (edit && (sel || vector), None),
            C::Duplicate
            | C::Group
            | C::FrameSelection
            | C::Flatten
            | C::OutlineStroke
            | C::BringForward
            | C::SendBackward
            | C::BringToFront
            | C::SendToBack
            | C::ToggleVisible
            | C::ToggleLock
            | C::FlipHorizontal
            | C::FlipVertical => (edit && sel, None),
            C::Ungroup => (edit && f.ungroupable, None),
            // Как в Figma: нужны хотя бы два слоя или выделенные булевы группы (смена
            // операции); отмечена операция выделенных групп.
            C::BooleanUnion | C::BooleanSubtract | C::BooleanIntersect | C::BooleanExclude => {
                let op = match id {
                    C::BooleanUnion => BooleanOp::Union,
                    C::BooleanSubtract => BooleanOp::Subtract,
                    C::BooleanIntersect => BooleanOp::Intersect,
                    _ => BooleanOp::Exclude,
                };
                (
                    edit && (f.count >= 2 || f.any_boolean),
                    checked(f.boolean_op == Some(op)),
                )
            }
            C::ToggleUi => (true, checked(self.settings.ui_hidden)),
            C::PixelGrid => (true, checked(self.settings.pixel_grid)),
            C::EditObject => (edit && !text && !vector && f.editable, None),
            C::TextBold
            | C::TextItalic
            | C::TextUnderline
            | C::TextStrike
            | C::TextAlignLeft
            | C::TextAlignCenter
            | C::TextAlignRight
            | C::TextAlignJustify => (edit && (text || f.has_text), None),
            C::UiPixelSnap => (true, checked(self.settings.pixel_snap)),
            C::UiThemeDark => (true, checked(self.settings.theme == ThemeChoice::Dark)),
            C::UiThemeLight => (true, checked(self.settings.theme == ThemeChoice::Light)),
            C::UiThemeSystem => (true, checked(self.settings.theme == ThemeChoice::System)),
            C::UiMainMenu
            | C::UiNewFile
            | C::UiOpen
            | C::UiOpenDisk
            | C::UiSaveDisk
            | C::UiSaveCopy
            | C::UiExport
            | C::UiAllFiles
            | C::UiSettings
            | C::UiShortcuts
            | C::UiAbout => (true, None),
        };
        CommandState { enabled, checked }
    }

    /// Состояние одной команды сейчас.
    pub fn command_state(&self, id: CommandId) -> CommandState {
        self.state_with(id, &self.facts())
    }

    /// Состояния всех команд в порядке `COMMANDS` (один проход по выделению).
    pub fn command_states(&self) -> Vec<CommandState> {
        let f = self.facts();
        COMMANDS.iter().map(|c| self.state_with(c.id, &f)).collect()
    }

    /// Изменившиеся с прошлого раза состояния (в первый раз и после
    /// `reset_command_states` — все); `None` — ничего не изменилось или ключ тот
    /// же. Во время жеста не считаются: состояния не меняются посреди него.
    pub fn take_command_states(&mut self) -> Option<Vec<(CommandId, CommandState)>> {
        if self.is_busy() {
            return None;
        }
        let key = self.states_key();
        if self.states.key.as_ref() == Some(&key) {
            return None;
        }
        let now = self.command_states();
        let changed: Vec<(CommandId, CommandState)> = COMMANDS
            .iter()
            .zip(&now)
            .enumerate()
            .filter(|(i, (_, s))| self.states.sent.get(*i) != Some(s))
            .map(|(_, (c, s))| (c.id, *s))
            .collect();
        self.states = StatesCache {
            key: Some(key),
            sent: now,
        };
        (!changed.is_empty()).then_some(changed)
    }

    /// Интерфейс забыл состояния (новый документ, новое ядро): следующий
    /// `take_command_states` вернёт все.
    pub fn reset_command_states(&mut self) {
        self.states = StatesCache::default();
    }
}
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        &["панели", "полный экран", "hide ui", "interface"],
        &["Mod+Backslash"],
    ),
    // Shift+H и Shift+V, как в Figma: H без модификаторов — «Рука», V — «Перемещение»,
    // Ctrl/Cmd+Shift+H — «Скрыть»; сочетания сравниваются точным набором модификаторов.
````

на:

````rust
        &["панели", "полный экран", "hide ui", "interface"],
        &["Mod+Backslash"],
    )
    .toggle(),
    // Shift+H и Shift+V, как в Figma: H без модификаторов — «Рука», V — «Перемещение»,
    // Ctrl/Cmd+Shift+H — «Скрыть»; сочетания сравниваются точным набором модификаторов.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
use kurbo::{Point, Size, Vec2};

use crate::commands::CommandId;
use crate::geometry::world_box;
````

на:

````rust
use kurbo::{Point, Size, Vec2};

use crate::command_state::{CommandState, StatesCache};
use crate::commands::CommandId;
use crate::geometry::world_box;
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Пиксельная сетка при зуме от 800%.
    pub pixel_grid: bool,
}

````

на:

````rust
    /// Пиксельная сетка при зуме от 800%.
    pub pixel_grid: bool,
    /// Выбранная тема интерфейса (для отметки в меню «Настройки»): её присылает
    /// интерфейс вместе с темой оверлеев.
    pub theme: ThemeChoice,
    /// Интерфейс скрыт (Ctrl/Cmd+\): отметка у «Скрыть интерфейс».
    pub ui_hidden: bool,
}

/// Тема, которую выбрал пользователь: тёмная, светлая или как в системе.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
    System,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            snapping: true,
            pixel_grid: true,
        }
    }
````

на:

````rust
            snapping: true,
            pixel_grid: true,
            theme: ThemeChoice::Dark,
            ui_hidden: false,
        }
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Вызвана команда интерфейса (`ui.*`): её выполняет основной поток.
    UiCommand(CommandId),
    /// Сообщение об ошибке для пользователя.
    Error(String),
````

на:

````rust
    /// Вызвана команда интерфейса (`ui.*`): её выполняет основной поток.
    UiCommand(CommandId),
    /// Изменились состояния команд (доступна, отмечена) — только изменившиеся.
    CommandStates(Vec<(CommandId, CommandState)>),
    /// Сообщение об ошибке для пользователя.
    Error(String),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    pub(crate) selection: Selection,
    /// Растёт при каждой замене сцены целиком (переключение страницы).
    scene_epoch: u64,
    pub(crate) hover: Option<NodeId>,
    pub(crate) tool: Box<dyn Tool>,
````

на:

````rust
    pub(crate) selection: Selection,
    /// Растёт при каждой замене сцены целиком (переключение страницы).
    pub(crate) scene_epoch: u64,
    pub(crate) hover: Option<NodeId>,
    pub(crate) tool: Box<dyn Tool>,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// следующем `emit`.
    pub(crate) outlines_missed: Rc<Cell<bool>>,
}

````

на:

````rust
    /// следующем `emit`.
    pub(crate) outlines_missed: Rc<Cell<bool>>,
    /// Состояния команд, отправленные интерфейсу, и их ключ.
    pub(crate) states: StatesCache,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            outlines_generation: 0,
            outlines_missed,
        })
    }
````

на:

````rust
            outlines_generation: 0,
            outlines_missed,
            states: StatesCache::default(),
        })
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        }
        self.seen = now;
    }

````

на:

````rust
        }
        self.seen = now;
        if let Some(changed) = self.take_command_states() {
            self.out.events.push(EditorEvent::CommandStates(changed));
        }
    }

````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
mod boolean;
pub mod clipboard;
pub mod commands;
mod editor;
````

на:

````rust
mod boolean;
pub mod clipboard;
pub mod command_state;
pub mod commands;
mod editor;
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
pub use editor::{Editor, EditorEvent, Settings, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
````

на:

````rust

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
pub use command_state::CommandState;
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
pub use editor::{Editor, EditorEvent, Settings, ThemeChoice, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
use ferrum_doc::{container, fixture, DocResult, Document, NodeId, Tx};
use ferrum_editor::geometry::SelectionBox;
use ferrum_editor::{CommandId, Cursor, Editor, EditorEvent, Input, Platform, ToolKind};
use ferrum_render::{
    decode_png, Clock, ExportBudget, FrameStats, OverlayTheme, RenderBackend, Renderer, Rgba,
````

на:

````rust
use ferrum_doc::{container, fixture, DocResult, Document, NodeId, Tx};
use ferrum_editor::geometry::SelectionBox;
use ferrum_editor::{
    CommandId, CommandState, Cursor, Editor, EditorEvent, Input, Platform, ThemeChoice, ToolKind,
};
use ferrum_render::{
    decode_png, Clock, ExportBudget, FrameStats, OverlayTheme, RenderBackend, Renderer, Rgba,
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
use crate::live::LiveState;
use crate::protocol::{
    Backend, Command, CoreEvent, CursorKind, Theme, Tool, MAX_DEMO_NODES, MAX_DEMO_TEXTS,
    MAX_JOURNAL_BATCHES, SELECTION_IDS_MAX, THUMB_HEIGHT, THUMB_WIDTH,
};
use crate::summary::Summary;
````

на:

````rust
use crate::live::LiveState;
use crate::protocol::{
    Backend, Command, CommandStateDto, CoreEvent, CursorKind, Theme, Tool, MAX_DEMO_NODES,
    MAX_DEMO_TEXTS, MAX_JOURNAL_BATCHES, SELECTION_IDS_MAX, THUMB_HEIGHT, THUMB_WIDTH,
};
use crate::summary::Summary;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            export_clock: ferrum_render::default_clock(),
        };
        // Таблица команд и пресеты нужны интерфейсу сразу, до документа.
        core.push_keymap();
        core.push_presets();
        core
    }
````

на:

````rust
            export_clock: ferrum_render::default_clock(),
        };
        // Таблица команд, их состояния и пресеты нужны интерфейсу сразу, до документа.
        core.push_keymap();
        core.push_presets();
        if let Some(changed) = core.editor.take_command_states() {
            core.push_command_states(changed);
        }
        core
    }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        editor.take_events();
        self.editor = editor;
        self.images = DocImages::default();
        self.selection = None;
````

на:

````rust
        editor.take_events();
        self.editor = editor;
        // Состояния команд нового документа — интерфейсу целиком.
        self.editor.reset_command_states();
        if let Some(changed) = self.editor.take_command_states() {
            self.push_command_states(changed);
        }
        self.images = DocImages::default();
        self.selection = None;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        self.preload_page_fonts();
        Ok(())
    }

````

на:

````rust
        self.preload_page_fonts();
        Ok(())
    }

    /// Событие `CommandStates`: изменения из одной пачки — одним событием
    /// (у команды — последнее состояние).
    fn push_command_states(&mut self, changed: Vec<(CommandId, CommandState)>) {
        let dto = |(id, s): (CommandId, CommandState)| CommandStateDto {
            key: id.key().to_string(),
            enabled: s.enabled,
            checked: s.checked,
        };
        if let Some(CoreEvent::CommandStates { changed: list }) = self
            .events
            .iter_mut()
            .find(|e| matches!(e, CoreEvent::CommandStates { .. }))
        {
            for c in changed.into_iter().map(dto) {
                match list.iter_mut().find(|x| x.key == c.key) {
                    Some(x) => *x = c,
                    None => list.push(c),
                }
            }
            return;
        }
        self.events.push(CoreEvent::CommandStates {
            changed: changed.into_iter().map(dto).collect(),
        });
    }

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            },
            Command::SetTool { tool } => self.editor.set_tool(tool_kind(tool)),
            Command::SetTheme { theme } => {
                self.theme = theme;
                self.editor.set_theme(overlay_theme(theme));
            }
            Command::Paste { bytes } => {
````

на:

````rust
            },
            Command::SetTool { tool } => self.editor.set_tool(tool_kind(tool)),
            Command::SetTheme { theme, system } => {
                self.theme = theme;
                self.editor.set_theme(overlay_theme(theme));
                let mut s = self.editor.settings().clone();
                s.theme = match (system, theme) {
                    (Some(true), _) => ThemeChoice::System,
                    (_, Theme::Dark) => ThemeChoice::Dark,
                    (_, Theme::Light) => ThemeChoice::Light,
                };
                self.editor.set_settings(s);
            }
            Command::Paste { bytes } => {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                    key: id.key().to_string(),
                }),
                EditorEvent::Error(message) => self.events.push(CoreEvent::Error { message }),
            }
````

на:

````rust
                    key: id.key().to_string(),
                }),
                EditorEvent::CommandStates(changed) => self.push_command_states(changed),
                EditorEvent::Error(message) => self.events.push(CoreEvent::Error { message }),
            }
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    SetTheme {
        theme: Theme,
    },
    /// Вставить содержимое буфера обмена (формат `web application/x-ferrum`).
````

на:

````rust
    SetTheme {
        theme: Theme,
        /// Тема выбрана «как в системе» (`theme` — то, что система дала сейчас):
        /// для отметки в меню «Настройки».
        #[serde(default)]
        system: Option<bool>,
    },
    /// Вставить содержимое буфера обмена (формат `web application/x-ferrum`).
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
}

/// Сочетание, занятое несколькими командами (ключи команд).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

на:

````rust
}

/// Состояние команды: доступна ли и (у переключателя) отмечена ли.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct CommandStateDto {
    pub key: String,
    pub enabled: bool,
    #[serde(default)]
    #[tsify(optional)]
    pub checked: Option<bool>,
}

/// Сочетание, занятое несколькими командами (ключи команд).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    UiCommand {
        key: String,
    },
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
````

на:

````rust
    UiCommand {
        key: String,
    },
    /// Состояния команд изменились (доступна, отмечена): только изменившиеся;
    /// первым — все команды таблицы. Основной поток ничего не вычисляет.
    CommandStates {
        changed: Vec<CommandStateDto>,
    },
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Состояния команд"`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/app.spec.ts e2e/booleans.spec.ts e2e/settings.spec.ts --project=chromium`

Expected: PASS: `ferrum-editor` и `ferrum-wasm` — 578 passed, 2 ignored (`command_state` — 11 тестов); `editor_report` — «Состояния команд после шага, 100 000 узлов, выделен 1» ≈ 0,001 мс (≤ 1 мс) PASS, «посчитаны все команды таблицы» — 80, справочно 10 000 выделенных ≈ 0,25 мс и все 100 000 ≈ 5 мс, FAIL нет; модульные и сборка — ок; E2E `app`, `booleans`, `settings` в Chromium — без падений

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/examples/editor_report.rs crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/command_state.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/tests/boolean.rs crates/ferrum-editor/tests/command_state.rs crates/ferrum-editor/tests/command_table.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/fonts.rs crates/ferrum-wasm/tests/panels.rs crates/ferrum-wasm/tests/protocol.rs
git commit -m "ядро: состояние команд — доступна и отмечена, событие CommandStates

Редактор считает для каждой команды таблицы, доступна ли она (выделение,
история, «только чтение», правка текста и вектора) и отмечена ли (булева
операция выделенных групп, пиксельная сетка, скрытый интерфейс, привязка к
пикселям, тема). Пересчёт — по ключу (версия выделения, счётчик сцены,
история, режим правки, настройки) и не во время жеста; интерфейсу уходят
только изменившиеся команды событием CommandStates, первым — все (новое ядро,
новый документ). Недоступная команда при вызове ничего не делает: ни шага, ни
сообщения — булева операция над одним слоем теперь недоступна, как в Figma.
SetTheme несёт признак темы «как в системе». editor_report: пересчёт после
шага на 100 000 узлах (цель 1 мс) и справочно при 10 000 и всех выделенных.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 4: Главное меню данными в ядре и событие `MainMenu`

Спека §3–4: главное меню описано данными в ядре — дерево «команда | подменю | разделитель» в порядке Figma UI3; интерфейс получает его один раз. Здесь — `menu.rs` (`main_menu()`, `menu_commands()`), `MenuItemDto` и `CoreEvent::MainMenu { tree }` из `Core::new` (`push_main_menu`).

**Files:**
- Create: `crates/ferrum-editor/src/menu.rs`
- Modify: `crates/ferrum-editor/src/lib.rs`, `crates/ferrum-wasm/src/{core,protocol}.rs`, `crates/ferrum-wasm/src/core/panels.rs`
- Test: `crates/ferrum-editor/tests/main_menu.rs` (новый), `crates/ferrum-wasm/tests/{panels,protocol}.rs`

**Interfaces:**
- Consumes: `CommandId`, `CommandGroup::title()`.
- Produces: `pub enum MenuNode { Command(CommandId), Submenu { title: &'static str, items: Vec<MenuNode> }, Separator }`; `pub fn main_menu() -> Vec<MenuNode>`; `pub fn menu_commands(&[MenuNode]) -> Vec<CommandId>`; `MenuItemDto` (тег `type`: `command | submenu | separator`); `CoreEvent::MainMenu { tree }`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/main_menu.rs`:

````rust
//! Главное меню данными (спека подсистемы 2, раздел 4): подменю в порядке
//! Figma UI3, только существующие команды, без пустых подменю и лишних
//! разделителей.

use std::collections::HashSet;

use ferrum_editor::*;

fn titles(items: &[MenuNode]) -> Vec<&'static str> {
    items
        .iter()
        .filter_map(|n| match n {
            MenuNode::Submenu { title, .. } => Some(*title),
            _ => None,
        })
        .collect()
}

fn submenu<'a>(items: &'a [MenuNode], title: &str) -> &'a [MenuNode] {
    items
        .iter()
        .find_map(|n| match n {
            MenuNode::Submenu { title: t, items } if *t == title => Some(items.as_slice()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("нет подменю «{title}»"))
}

fn keys(items: &[MenuNode]) -> Vec<&'static str> {
    menu_commands(items).into_iter().map(|c| c.key()).collect()
}

/// Каждый уровень: не пуст, без разделителей в начале, в конце и подряд, без
/// повторов команд.
fn check_level(items: &[MenuNode], path: &str) {
    assert!(!items.is_empty(), "{path}: пусто");
    assert!(
        !matches!(items.first(), Some(MenuNode::Separator))
            && !matches!(items.last(), Some(MenuNode::Separator)),
        "{path}: разделитель с краю"
    );
    let mut seen = HashSet::new();
    for w in items.windows(2) {
        assert!(
            !(matches!(w[0], MenuNode::Separator) && matches!(w[1], MenuNode::Separator)),
            "{path}: разделители подряд"
        );
    }
    for n in items {
        match n {
            MenuNode::Command(c) => assert!(seen.insert(*c), "{path}: {c:?} дважды"),
            MenuNode::Submenu { title, items } => check_level(items, &format!("{path} › {title}")),
            MenuNode::Separator => {}
        }
    }
}

#[test]
fn top_level_follows_figma_ui3() {
    let menu = main_menu();
    assert_eq!(
        titles(&menu),
        [
            "Файл",
            "Правка",
            "Вид",
            "Объект",
            "Текст",
            "Расставить",
            "Вектор",
            "Настройки",
            "Справка"
        ]
    );
    assert_eq!(menu.len(), 9, "только подменю, без плагинов и виджетов");
    check_level(&menu, "меню");
}

#[test]
fn submenus_hold_the_commands_of_the_spec() {
    let menu = main_menu();
    assert_eq!(
        keys(submenu(&menu, "Файл")),
        [
            "ui.new_file",
            "ui.open",
            "ui.open_disk",
            "file.save",
            "ui.save_disk",
            "ui.save_copy",
            "ui.export",
            "ui.all_files"
        ]
    );
    let edit = submenu(&menu, "Правка");
    assert_eq!(
        keys(submenu(edit, "Копировать как")),
        ["edit.copy_png", "edit.copy_svg"]
    );
    for k in [
        "edit.undo",
        "edit.redo",
        "edit.duplicate",
        "edit.select_all",
    ] {
        assert!(keys(edit).contains(&k), "{k}");
    }
    let object = submenu(&menu, "Объект");
    assert_eq!(
        keys(submenu(object, "Булевы операции")),
        [
            "object.boolean_union",
            "object.boolean_subtract",
            "object.boolean_intersect",
            "object.boolean_exclude"
        ]
    );
    assert!(keys(submenu(&menu, "Вид")).contains(&"view.pixel_grid"));
    assert_eq!(keys(submenu(&menu, "Вектор")), ["vector.edit", "tool.pen"]);
    assert_eq!(
        keys(submenu(submenu(&menu, "Настройки"), "Тема")),
        ["ui.theme_light", "ui.theme_dark", "ui.theme_system"]
    );
    assert_eq!(
        keys(submenu(&menu, "Справка")),
        ["ui.shortcuts", "ui.about"]
    );
}

#[test]
fn every_menu_command_is_in_the_table_and_groups_are_covered() {
    let all: HashSet<CommandId> = menu_commands(&main_menu()).into_iter().collect();
    // Команды групп меню — все в дереве, кроме тех, что живут только на
    // клавишах: Esc, сдвиги, инструменты (у них тулбар), само главное меню.
    for c in COMMANDS {
        let only_keys = matches!(c.group, CommandGroup::Tool | CommandGroup::Nudge)
            || matches!(c.id, CommandId::Escape | CommandId::UiMainMenu);
        assert_eq!(
            all.contains(&c.id),
            !only_keys || c.id == CommandId::ToolPen,
            "{}",
            c.key
        );
    }
}
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
    assert!(errors(&ev).is_empty() && ev.is_empty(), "{ev:?}");
    assert_eq!(document_steps(&core), 0.0);
}
````

на:

````rust
    assert!(errors(&ev).is_empty() && ev.is_empty(), "{ev:?}");
    assert_eq!(document_steps(&core), 0.0);
}

#[test]
fn main_menu_is_sent_once_with_keys_of_the_table() {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    let ev = core.take_events();
    let menus: Vec<&Vec<MenuItemDto>> = ev
        .iter()
        .filter_map(|e| match e {
            CoreEvent::MainMenu { tree } => Some(tree),
            _ => None,
        })
        .collect();
    assert_eq!(menus.len(), 1);
    let tree = menus[0];
    let MenuItemDto::Submenu { title, items } = &tree[0] else {
        panic!("первым — подменю «Файл»: {tree:?}");
    };
    assert_eq!(title, "Файл");
    assert_eq!(
        items[0],
        MenuItemDto::Command {
            key: "ui.new_file".into()
        }
    );
    // Новый документ дерево не присылает: оно не зависит от документа.
    core.command(Command::Demo { nodes: 10 });
    assert!(!core
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::MainMenu { .. })));
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
            key: "ui.main_menu".into(),
        },
        CoreEvent::CommandStates {
            changed: vec![
````

на:

````rust
            key: "ui.main_menu".into(),
        },
        CoreEvent::MainMenu {
            tree: vec![
                MenuItemDto::Submenu {
                    title: "Правка".into(),
                    items: vec![
                        MenuItemDto::Command {
                            key: "edit.undo".into(),
                        },
                        MenuItemDto::Separator,
                    ],
                },
                MenuItemDto::Separator,
            ],
        },
        CoreEvent::CommandStates {
            changed: vec![
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test main_menu`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test panels --test protocol`
Expected: FAIL — не собирается: редактор — нет `MenuNode`, `main_menu`, `menu_commands` (`main_menu` — 19 ошибок); ядро — нет `MenuItemDto` и `CoreEvent::MainMenu` (`protocol` и `panels` — по 5)

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod keymap;
pub mod measure;
mod ops;
mod outline;
````

на:

````rust
pub mod keymap;
pub mod measure;
pub mod menu;
mod ops;
mod outline;
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
pub use pages::{clean_name, MAX_NAME_CHARS};
pub use panel::{
````

на:

````rust
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
pub use menu::{main_menu, menu_commands, MenuNode};
pub use pages::{clean_name, MAX_NAME_CHARS};
pub use panel::{
````

Создать `crates/ferrum-editor/src/menu.rs`:

````rust
//! Главное меню данными (спека подсистемы 2, разделы 3 и 4): дерево команд по
//! ключам таблицы, подменю и разделителей. Порядок и состав повторяют главное
//! меню Figma UI3 без плагинов, виджетов и Dev Mode. В дереве — только команды,
//! которые уже есть: пункты следующих этапов (вставить поверх, с заменой,
//! копирование свойств, линейки, выделить подобные) появятся вместе с ними, а не
//! серыми заглушками. Интерфейс получает дерево один раз и по пункту присылает
//! `Execute` с ключом команды.

use crate::commands::{CommandGroup, CommandId};

/// Узел меню.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuNode {
    Command(CommandId),
    Submenu {
        title: &'static str,
        items: Vec<MenuNode>,
    },
    Separator,
}

use CommandId as C;
use MenuNode::{Command as I, Separator as S};

fn sub(title: &'static str, items: Vec<MenuNode>) -> MenuNode {
    MenuNode::Submenu { title, items }
}

fn group(g: CommandGroup, items: Vec<MenuNode>) -> MenuNode {
    sub(g.title(), items)
}

/// Главное меню (кнопка с логотипом, Alt+/).
pub fn main_menu() -> Vec<MenuNode> {
    use CommandGroup as G;
    vec![
        group(
            G::File,
            vec![
                I(C::UiNewFile),
                I(C::UiOpen),
                I(C::UiOpenDisk),
                S,
                I(C::Save),
                I(C::UiSaveDisk),
                I(C::UiSaveCopy),
                S,
                I(C::UiExport),
                S,
                I(C::UiAllFiles),
            ],
        ),
        group(
            G::Edit,
            vec![
                I(C::Undo),
                I(C::Redo),
                S,
                I(C::Copy),
                I(C::Cut),
                I(C::Paste),
                sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
                S,
                I(C::Duplicate),
                I(C::Delete),
                S,
                I(C::SelectAll),
            ],
        ),
        group(
            G::View,
            vec![
                I(C::PixelGrid),
                S,
                I(C::ToggleUi),
                S,
                I(C::ZoomIn),
                I(C::ZoomOut),
                I(C::Zoom100),
                I(C::ZoomToFit),
                I(C::ZoomToSelection),
            ],
        ),
        group(
            G::Object,
            vec![
                I(C::Group),
                I(C::Ungroup),
                I(C::FrameSelection),
                S,
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
                S,
                I(C::ToggleVisible),
                I(C::ToggleLock),
            ],
        ),
        group(
            G::Text,
            vec![
                I(C::TextBold),
                I(C::TextItalic),
                I(C::TextUnderline),
                I(C::TextStrike),
                S,
                sub(
                    "Выравнивание",
                    vec![
                        I(C::TextAlignLeft),
                        I(C::TextAlignCenter),
                        I(C::TextAlignRight),
                        I(C::TextAlignJustify),
                    ],
                ),
            ],
        ),
        group(
            G::Arrange,
            vec![
                I(C::BringToFront),
                I(C::BringForward),
                I(C::SendBackward),
                I(C::SendToBack),
            ],
        ),
        group(G::Vector, vec![I(C::EditObject), I(C::ToolPen)]),
        group(
            G::Settings,
            vec![
                sub(
                    "Тема",
                    vec![I(C::UiThemeLight), I(C::UiThemeDark), I(C::UiThemeSystem)],
                ),
                I(C::UiPixelSnap),
                S,
                I(C::UiShortcuts),
                I(C::UiSettings),
            ],
        ),
        group(G::Help, vec![I(C::UiShortcuts), I(C::UiAbout)]),
    ]
}

/// Все команды дерева по порядку (для проверок и поиска).
pub fn menu_commands(items: &[MenuNode]) -> Vec<CommandId> {
    let mut out = Vec::new();
    for n in items {
        match n {
            MenuNode::Command(c) => out.push(*c),
            MenuNode::Submenu { items, .. } => out.extend(menu_commands(items)),
            MenuNode::Separator => {}
        }
    }
    out
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        // Таблица команд, их состояния и пресеты нужны интерфейсу сразу, до документа.
        core.push_keymap();
        core.push_presets();
        if let Some(changed) = core.editor.take_command_states() {
````

на:

````rust
        // Таблица команд, их состояния и пресеты нужны интерфейсу сразу, до документа.
        core.push_keymap();
        core.push_main_menu();
        core.push_presets();
        if let Some(changed) = core.editor.take_command_states() {
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
use ferrum_editor::scale::{Anchor, MAX_SCALE_FACTOR, MIN_SCALE_FACTOR};
use ferrum_editor::{
    browser_reserved, frame_presets, BindError, CommandId, DropTarget, KeymapError, NumField,
    PanelEdit, Phase, Shortcut, COMMANDS, MAX_NAME_CHARS,
};
use ferrum_render::{decode_png, export_area, ExportBudget, ExportError, SvgOptions};
````

на:

````rust
use ferrum_editor::scale::{Anchor, MAX_SCALE_FACTOR, MIN_SCALE_FACTOR};
use ferrum_editor::{
    browser_reserved, frame_presets, main_menu, BindError, CommandId, DropTarget, KeymapError,
    MenuNode, NumField, PanelEdit, Phase, Shortcut, COMMANDS, MAX_NAME_CHARS,
};
use ferrum_render::{decode_png, export_area, ExportBudget, ExportError, SvgOptions};
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
use crate::protocol::{
    Align, Command, ConflictInfo, CoreEvent, DropPosition, Edit, EditPhase, ExportFormat, Field,
    FillDto, KeymapEntry, PageInfo, PaintKind, PresetInfo, ScaleModeDto, SelectMode, StrokeDto,
    EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{cap_of, join_of, summarize, MAX_PAINTS};
````

на:

````rust
use crate::protocol::{
    Align, Command, ConflictInfo, CoreEvent, DropPosition, Edit, EditPhase, ExportFormat, Field,
    FillDto, KeymapEntry, MenuItemDto, PageInfo, PaintKind, PresetInfo, ScaleModeDto, SelectMode,
    StrokeDto, EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{cap_of, join_of, summarize, MAX_PAINTS};
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
    }

    pub(super) fn push_presets(&mut self) {
        let presets = frame_presets()
````

на:

````rust
    }

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

    pub(super) fn push_presets(&mut self) {
        let presets = frame_presets()
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

/// Пункт меню: команда по ключу таблицы, подменю или разделитель.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MenuItemDto {
    Command {
        key: String,
    },
    Submenu {
        title: String,
        items: Vec<MenuItemDto>,
    },
    Separator,
}

/// Состояние команды: доступна ли и (у переключателя) отмечена ли.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        changed: Vec<CommandStateDto>,
    },
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
    /// пунктирных направляющих.
````

на:

````rust
        changed: Vec<CommandStateDto>,
    },
    /// Главное меню (спека подсистемы 2, раздел 4): дерево команд — один раз,
    /// при запуске ядра.
    MainMenu {
        tree: Vec<MenuItemDto>,
    },
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
    /// пунктирных направляющих.
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Expected: PASS: `ferrum-editor` и `ferrum-wasm` — 582 passed, 2 ignored (`main_menu` — 3 теста); wasm собран, `npm run typecheck` без ошибок

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/menu.rs crates/ferrum-editor/tests/main_menu.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/panels.rs crates/ferrum-wasm/tests/protocol.rs
git commit -m "ядро: главное меню данными — дерево команд и событие MainMenu

Главное меню описано в ferrum-editor деревом «команда | подменю |
разделитель» в порядке Figma UI3 без плагинов, виджетов и Dev Mode: «Файл»,
«Правка» (с подменю «Копировать как»), «Вид», «Объект» (с подменю «Булевы
операции»), «Текст» (с подменю «Выравнивание»), «Расставить», «Вектор»,
«Настройки» (с подменю «Тема») и «Справка». В дереве только существующие
команды: пункты следующих этапов появятся вместе с ними. Ядро шлёт дерево
ключами команд один раз при запуске (MainMenu).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5: Компонент меню: подменю, первая буква, состояние команд

Спека §4: подменю, переход по первой букве, серые пункты и галочки. Здесь — `MenuItem.submenu`, рекурсивный уровень меню со своим `Portal` (`data-menu-tree` для щелчка снаружи), подменю наведением 150 мс, щелчком, Enter и →, ← и Esc — к родителю; серые пункты фокусируются, но не выполняются; поле поиска (`MenuProps.search`); `disabled` и `checked` пункта команды по умолчанию — из `store.state.commands`; положение в фазе `measure` (`placeMenu`); `apply('commandStates')` в хранилище.

**Files:**
- Create: `apps/web/src/ui/menu-logic.ts`
- Modify: `apps/web/src/ui/{menu.tsx,store.ts}`, `apps/web/src/ui/styles/chrome.css`
- Test: `apps/web/unit/menu.test.ts` (новый), `apps/web/e2e/chrome.spec.ts`

**Interfaces:**
- Consumes: `CoreEvent::CommandStates`, раскладка (`labels`).
- Produces: `MenuItem { …, submenu?: MenuItem[], disabled?, checked? }`; `MenuProps.search?: MenuSearch { label, placeholder, results(query) }`; `menu-logic.ts`: `placeMenu(anchor, size, view, { above?, submenu? })`, `typeAhead(labels, from, ch)`, `EDGE = 8`, `SUBMENU_DELAY_MS = 150`; `UiState.commands: Record<string, CommandStatus>`, `CommandStatus { enabled, checked? }`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  await page.keyboard.press('Tab');
  await expect(menu).toHaveCount(0);
});

````

на:

````ts
  await page.keyboard.press('Tab');
  await expect(menu).toHaveCount(0);
});

test('меню: серые пункты доступны стрелками, но не выполняются; переход по первой букве', async ({ page }) => {
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
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

````

Создать `apps/web/unit/menu.test.ts`:

````ts
// Меню (спека подсистемы 2, раздел 4): положение у якоря и отражение подменю у
// края окна, переход по первой букве, состояние команд из событий ядра.

import { expect, test } from '@playwright/test';
import { EDGE, placeMenu, typeAhead } from '../src/ui/menu-logic';
import { createUiStore } from '../src/ui/store';

const view = { width: 1000, height: 600 };
const size = { width: 200, height: 300 };

test('корневое меню: под якорем, у нижнего края — над ним, у правого — сдвиг', () => {
  const a = { left: 10, right: 40, top: 10, bottom: 34 };
  expect(placeMenu(a, size, view)).toEqual({ x: 10, y: 38 });
  expect(placeMenu({ left: 10, right: 40, top: 500, bottom: 524 }, size, view)).toEqual({ x: 10, y: 196 });
  expect(placeMenu({ left: 950, right: 990, top: 10, bottom: 34 }, size, view).x).toBe(1000 - EDGE - 200);
  expect(placeMenu({ left: 10, right: 40, top: 400, bottom: 424 }, size, view, { above: true }).y).toBe(96);
});

test('подменю: справа от пункта, у правого края — слева, у нижнего — поднимается', () => {
  const item = { left: 100, right: 300, top: 50, bottom: 74 };
  expect(placeMenu(item, size, view, { submenu: true })).toEqual({ x: 300, y: 46 });
  const atRight = { left: 700, right: 900, top: 50, bottom: 74 };
  expect(placeMenu(atRight, size, view, { submenu: true })).toEqual({ x: 500, y: 46 });
  const low = { left: 100, right: 300, top: 500, bottom: 524 };
  expect(placeMenu(low, size, view, { submenu: true }).y).toBe(600 - EDGE - 300);
  // Не влезает ни справа, ни слева — прижато к краю окна.
  const narrow = { width: 300, height: 600 };
  expect(placeMenu({ left: 60, right: 260, top: 10, bottom: 34 }, size, narrow, { submenu: true }).x).toBe(300 - EDGE - 200);
});

test('переход по первой букве: по кругу, без учёта регистра и «ё»', () => {
  const labels = ['Файл', 'Правка', 'Вид', 'Объект', 'Текст', 'Вектор', 'Ёлка'];
  expect(typeAhead(labels, -1, 'в')).toBe(2);
  expect(typeAhead(labels, 2, 'В')).toBe(5);
  expect(typeAhead(labels, 5, 'в')).toBe(2);
  expect(typeAhead(labels, 0, 'е')).toBe(6);
  expect(typeAhead(labels, 0, 'ж')).toBe(-1);
  expect(typeAhead(labels, 0, ' ')).toBe(-1);
});

test('состояние команд: сначала все, потом только изменившиеся', () => {
  const store = createUiStore();
  store.apply({
    type: 'commandStates',
    changed: [
      { key: 'edit.delete', enabled: false },
      { key: 'view.pixel_grid', enabled: true, checked: true },
    ],
  });
  store.apply({ type: 'commandStates', changed: [{ key: 'edit.delete', enabled: true }] });
  expect(store.state.commands['edit.delete']).toEqual({ enabled: true, checked: undefined });
  expect(store.state.commands['view.pixel_grid']).toEqual({ enabled: true, checked: true });
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/menu.test.ts`
Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/chrome.spec.ts --project=chromium`
Expected: FAIL — модульные — `Error: Cannot find module '…/apps/web/src/ui/menu-logic'` (No tests found); E2E (стенд собран) — 25 passed, 1 failed: «меню: серые пункты доступны стрелками…» — `toBeFocused` у серого пункта «Копировать» (`Received: inactive`: прежнее меню фокусирует только доступные пункты)

- [ ] **Step 3: Реализация**

Создать `apps/web/src/ui/menu-logic.ts`:

````ts
// Чистая логика меню (спека подсистемы 2, раздел 4): где встать у якоря и как
// отразиться у края окна, переход по первой букве.

export interface Box {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

/** Отступ меню от края окна. */
export const EDGE = 8;
/** Задержка открытия и закрытия подменю наведением (мс), как в Figma. */
export const SUBMENU_DELAY_MS = 150;

/** Положение меню размера `size` у якоря в окне `view`.
 * - Корневое меню — под якорем (`above` — над ним); не влезает снизу — над якорем,
 *   справа — сдвигается влево.
 * - Подменю — справа от пункта, верхом вровень с ним; у правого края окна
 *   отражается налево от пункта, у нижнего — поднимается. */
export function placeMenu(
  anchor: Box,
  size: { width: number; height: number },
  view: { width: number; height: number },
  opts: { above?: boolean; submenu?: boolean } = {},
): { x: number; y: number } {
  const { width: w, height: h } = size;
  let x: number;
  let y: number;
  if (opts.submenu) {
    x = anchor.right;
    if (x + w > view.width - EDGE) x = anchor.left - w;
    if (x < EDGE) x = Math.max(EDGE, view.width - EDGE - w);
    // Первый пункт подменю — вровень с пунктом-родителем (отступ меню — 4 px).
    y = anchor.top - 4;
    if (y + h > view.height - EDGE) y = view.height - EDGE - h;
  } else {
    x = anchor.left;
    y = opts.above ? anchor.top - h - 4 : anchor.bottom + 4;
    if (x + w > view.width - EDGE) x = view.width - EDGE - w;
    if (y + h > view.height - EDGE) y = anchor.top - h - 4;
  }
  return { x: Math.max(EDGE, x), y: Math.max(EDGE, y) };
}

/** Переход по первой букве: следующий после `from` пункт, чьё название
 * начинается с `ch` (без учёта регистра и «ё»), по кругу; `-1` — такого нет. */
export function typeAhead(labels: readonly string[], from: number, ch: string): number {
  const norm = (s: string) => s.trim().toLowerCase().replaceAll('ё', 'е');
  const c = norm(ch);
  if (!c) return -1;
  const n = labels.length;
  for (let k = 1; k <= n; k++) {
    const i = (from + k + n) % n;
    if (norm(labels[i]).startsWith(c)) return i;
  }
  return -1;
}
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
// Меню: выпадающие списки (файл, фигуры, пресеты, зум) и контекстное меню холста.
// role="menu" и клавиатура: стрелки, Home/End, Enter/Space, Esc (фокус
// возвращается туда, откуда меню открыли), Tab закрывает. Подпись сочетания у
// пункта — из действующей раскладки. Положение считается в фазе measure: меню
// сначала появляется невидимым, затем встаёт у якоря в пределах окна. Вне
// редактора (экран файлов: ни ядра, ни планировщика) — чтение и запись в одном
// кадре `requestAnimationFrame`, пункты — только с `onSelect`.

import { Index, Match, onCleanup, onMount, Show, Switch, useContext, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';
import { EditorContext } from './context';
import { Icon } from './icons';
import { shortcutLabel } from './shortcuts';

````

на:

````tsx
// Меню: выпадающие списки (главное меню, фигуры, пресеты, зум) и контекстное
// меню холста. role="menu" и клавиатура: стрелки (в том числе по серым пунктам —
// они не выполняются), Home/End, Enter/Space, переход по первой букве, Esc
// (фокус возвращается туда, откуда меню открыли), Tab закрывает.
// Подменю (спека подсистемы 2, раздел 4) открываются стрелкой вправо, Enter,
// щелчком и наведением с задержкой, как в Figma; стрелка влево и Esc
// возвращают к пункту-родителю. Доступность и галочка пункта команды — из
// состояния команд ядра (`CommandStates`), подпись сочетания — из действующей
// раскладки. Положение считается в фазе measure: меню сначала появляется
// невидимым, затем встаёт у якоря в пределах окна (подменю у края отражается на
// другую сторону). Вне редактора (экран файлов: ни ядра, ни планировщика) —
// чтение и запись в одном кадре `requestAnimationFrame`, пункты — только с
// `onSelect`.

import { createEffect, createSignal, Index, Match, on, onCleanup, onMount, Show, Switch, useContext, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';
import { EditorContext } from './context';
import { Icon } from './icons';
import { placeMenu, SUBMENU_DELAY_MS, typeAhead } from './menu-logic';
import { shortcutLabel } from './shortcuts';

````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
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
````

на:

````tsx
  | {
      label: string;
      /** Ключ команды редактора: выполнить её, показать её сочетание и состояние. */
      command?: string;
      /** Своя подпись справа (размер пресета). */
      hint?: string;
      /** Явно; без него у команды — из состояния ядра. */
      disabled?: boolean;
      /** Явно; без него у переключателя — из состояния ядра. */
      checked?: boolean;
      onSelect?: () => void;
      /** Подменю. */
      submenu?: MenuItem[];
    }
  | { separator: true }
  | { heading: string };

type Entry = Extract<MenuItem, { label: string }>;

export interface MenuSearch {
  label: string;
  placeholder: string;
  /** Пункты по запросу (непустому). */
  results: (query: string) => MenuItem[];
}

export interface MenuProps {
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
  above?: boolean;
  onClose: () => void;
}

export function Menu(props: MenuProps): JSX.Element {
  const ed = useContext(EditorContext);
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
    else if (item.command) ed?.run(item.command);
  };

  const onKey = (e: KeyboardEvent) => {
    const list = items();
    const at = list.indexOf(document.activeElement as HTMLElement);
    const go = (i: number) => list[(i + list.length) % list.length]?.focus();
    switch (e.key) {
      case 'ArrowDown':
````

на:

````tsx
  above?: boolean;
  onClose: () => void;
  /** Поле поиска вверху (главное меню): результаты вместо пунктов. */
  search?: MenuSearch;
}

let trees = 0;

export function Menu(props: MenuProps): JSX.Element {
  const returnFocus = document.activeElement as HTMLElement | null;
  const tree = `menu-${++trees}`;
  const closeAll = (restore: boolean) => {
    props.onClose();
    if (restore) returnFocus?.focus?.();
  };
  onMount(() => {
    const outside = (e: PointerEvent) => {
      if (!(e.target instanceof Element && e.target.closest(`[data-menu-tree="${tree}"]`))) closeAll(false);
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
  return (
    <Level
      items={props.items}
      anchor={props.anchor}
      label={props.label}
      above={props.above}
      search={props.search}
      tree={tree}
      depth={0}
      focusFirst
      closeAll={closeAll}
      closeSelf={() => closeAll(true)}
    />
  );
}

interface LevelProps {
  items: MenuItem[];
  anchor: HTMLElement | { x: number; y: number };
  label: string;
  above?: boolean;
  search?: MenuSearch;
  tree: string;
  depth: number;
  /** Фокус на первый пункт при открытии (клавиатура); наведение фокус не трогает. */
  focusFirst: boolean;
  closeAll: (restore: boolean) => void;
  /** Закрыть этот уровень (у корня — всё меню). */
  closeSelf: () => void;
  /** Указатель вошёл в подменю: родитель не закрывает его по таймеру. */
  onHover?: () => void;
}

function Level(props: LevelProps): JSX.Element {
  const ed = useContext(EditorContext);
  let el!: HTMLDivElement;
  let input: HTMLInputElement | undefined;
  const [query, setQuery] = createSignal('');
  /** Открытое подменю: номер пункта, его элемент и нужен ли фокус в подменю. */
  const [sub, setSub] = createSignal<{ index: number; el: HTMLElement; focus: boolean } | null>(null);
  let timer = 0;
  onCleanup(() => clearTimeout(timer));

  const shown = (): MenuItem[] => (props.search && query().trim() ? props.search.results(query()) : props.items);
  const items = () => Array.from(el.querySelectorAll<HTMLElement>(':scope > [data-index]'));
  const state = (it: Entry) => (it.command && ed ? ed.store.state.commands[it.command] : undefined);
  const disabled = (it: Entry) => it.disabled ?? (state(it) ? !state(it)!.enabled : false);
  const checked = (it: Entry) => it.checked ?? state(it)?.checked ?? undefined;
  const entry = (i: number) => {
    const it = shown()[i];
    return it && 'label' in it ? it : undefined;
  };

  const openSub = (index: number, target: HTMLElement, focus: boolean) => {
    clearTimeout(timer);
    const cur = sub();
    if (cur && cur.index === index) {
      if (focus) setSub({ ...cur, focus: true });
      return;
    }
    setSub({ index, el: target, focus });
  };
  const closeSub = (refocus: boolean) => {
    clearTimeout(timer);
    const cur = sub();
    setSub(null);
    if (refocus) cur?.el.focus();
  };

  const select = (i: number, target: HTMLElement) => {
    const it = entry(i);
    if (!it) return;
    if (it.submenu) {
      openSub(i, target, true);
      return;
    }
    if (disabled(it)) return;
    props.closeAll(true);
    if (it.onSelect) it.onSelect();
    else if (it.command) ed?.run(it.command);
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.target === input) {
      // Поле поиска: стрелка вниз — к пунктам, Enter — первый доступный.
      if (e.key === 'ArrowDown') {
        items()[0]?.focus();
      } else if (e.key === 'Enter') {
        const list = items();
        const first = list.find((x) => x.getAttribute('aria-disabled') !== 'true' && !x.hasAttribute('aria-haspopup'));
        if (first) select(Number(first.dataset.index), first);
      } else if (e.key === 'Escape') {
        props.closeSelf();
      } else if (e.key === 'Tab') {
        props.closeAll(false);
        return;
      } else return;
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    const list = items();
    const at = list.indexOf(document.activeElement as HTMLElement);
    const go = (i: number) => list[(i + list.length) % list.length]?.focus();
    const current = at >= 0 ? list[at] : undefined;
    switch (e.key) {
      case 'ArrowDown':
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
        break;
      case 'ArrowUp':
        go(at - 1);
        break;
      case 'Home':
````

на:

````tsx
        break;
      case 'ArrowUp':
        // С первого пункта главного меню — обратно в поле поиска.
        if (at === 0 && input) input.focus();
        else go(at - 1);
        break;
      case 'Home':
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
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
````

на:

````tsx
        go(list.length - 1);
        break;
      case 'ArrowRight':
        if (current?.hasAttribute('aria-haspopup')) openSub(Number(current.dataset.index), current, true);
        break;
      case 'ArrowLeft':
        if (props.depth > 0) props.closeSelf();
        break;
      case 'Enter':
      case ' ':
        if (current) select(Number(current.dataset.index), current);
        break;
      case 'Escape':
        props.closeSelf();
        break;
      case 'Tab':
        props.closeAll(false);
        return;
      default: {
        // Переход по первой букве (без модификаторов).
        if (e.key.length !== 1 || e.ctrlKey || e.metaKey || e.altKey) return;
        const labels = list.map((x) => x.dataset.label ?? '');
        const i = typeAhead(labels, at, e.key);
        if (i >= 0) list[i].focus();
      }
    }
    e.preventDefault();
    e.stopPropagation();
  };

  // Подменю, открытое наведением, получает фокус, когда в него входят с клавиатуры.
  let placed = false;
  createEffect(
    on(
      () => props.focusFirst,
      (f) => {
        if (f && placed) items()[0]?.focus();
      },
      { defer: true },
    ),
  );

  onMount(() => {
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
      const a = props.anchor;
      const r = a instanceof HTMLElement ? a.getBoundingClientRect() : { left: a.x, right: a.x, top: a.y, bottom: a.y };
      let x = r.left;
      let y = props.above ? r.top - box.height - 4 : r.bottom + 4;
      if (x + box.width > innerWidth - 8) x = Math.max(8, innerWidth - 8 - box.width);
      if (y + box.height > innerHeight - 8) y = Math.max(8, r.top - box.height - 4);
      if (y < 8) y = 8;
      mutate(() => {
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
````

на:

````tsx
      const a = props.anchor;
      const r = a instanceof HTMLElement ? a.getBoundingClientRect() : { left: a.x, right: a.x, top: a.y, bottom: a.y };
      const { x, y } = placeMenu(r, box, { width: innerWidth, height: innerHeight }, { above: props.above, submenu: props.depth > 0 });
      mutate(() => {
        el.style.left = `${x}px`;
        el.style.top = `${y}px`;
        el.style.visibility = 'visible';
        placed = true;
        if (input) input.focus();
        else if (props.focusFirst) items()[0]?.focus();
      });
    });
  });
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
  return (
    <Portal>
      <div ref={el} class="fe-menu" role="menu" aria-label={props.label} style={{ visibility: 'hidden' }} onKeyDown={onKey}>
        {/* Index, не For: при перестройке пунктов (зум изменился) узлы остаются, фокус не теряется. */}
        <Index each={props.items}>
          {(item) => (
            <Switch>
              <Match when={'separator' in item() && item()}>
````

на:

````tsx
  return (
    <Portal>
      <div
        ref={el}
        class="fe-menu"
        role="menu"
        aria-label={props.label}
        data-menu-tree={props.tree}
        style={{ visibility: 'hidden' }}
        onKeyDown={onKey}
        onPointerEnter={() => props.onHover?.()}
      >
        <Show when={props.search}>
          {(s) => (
            <div class="fe-menu__search">
              <Icon name="search" />
              <input
                ref={input}
                type="search"
                aria-label={s().label}
                placeholder={s().placeholder}
                value={query()}
                onInput={(e) => {
                  closeSub(false);
                  setQuery(e.currentTarget.value);
                }}
              />
            </div>
          )}
        </Show>
        <Show when={props.search && query().trim() && shown().length === 0}>
          <div class="fe-menu__heading fe-menu__empty">Ничего не найдено</div>
        </Show>
        {/* Index, не For: при перестройке пунктов (зум изменился) узлы остаются, фокус не теряется. */}
        <Index each={shown()}>
          {(item, i) => (
            <Switch>
              <Match when={'separator' in item() && item()}>
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
                {(h) => <div class="fe-menu__heading">{h().heading}</div>}
              </Match>
              <Match when={'label' in item() && (item() as Extract<MenuItem, { label: string }>)}>
                {(it) => (
                  <div
                    class="fe-menu__item"
                    role={it().checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
                    aria-checked={it().checked === undefined ? undefined : it().checked}
                    aria-disabled={it().disabled ? 'true' : undefined}
                    tabindex="-1"
                    onClick={() => select(it())}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        select(it());
                      }
                    }}
                  >
                    <span class="fe-menu__check">
                      <Show when={it().checked}>
                        <Icon name="check" />
                      </Show>
                    </span>
                    <span class="fe-menu__label">{it().label}</span>
                    <span class="fe-menu__key">{it().hint ?? key(it().command)}</span>
                  </div>
                )}
````

на:

````tsx
                {(h) => <div class="fe-menu__heading">{h().heading}</div>}
              </Match>
              <Match when={'label' in item() && (item() as Entry)}>
                {(it) => (
                  <div
                    class="fe-menu__item"
                    role={checked(it()) === undefined ? 'menuitem' : 'menuitemcheckbox'}
                    aria-checked={checked(it()) === undefined ? undefined : checked(it())}
                    aria-disabled={!it().submenu && disabled(it()) ? 'true' : undefined}
                    aria-haspopup={it().submenu ? 'menu' : undefined}
                    aria-expanded={it().submenu ? sub()?.index === i : undefined}
                    data-index={i}
                    data-label={it().label}
                    data-command={it().command}
                    tabindex="-1"
                    onClick={(e) => select(i, e.currentTarget)}
                    onPointerEnter={(e) => {
                      // Наведение: подменю открывается с задержкой, чужое — закрывается.
                      clearTimeout(timer);
                      const target = e.currentTarget;
                      if (it().submenu) timer = window.setTimeout(() => openSub(i, target, false), SUBMENU_DELAY_MS);
                      else if (sub()) timer = window.setTimeout(() => closeSub(false), SUBMENU_DELAY_MS);
                    }}
                  >
                    <span class="fe-menu__check">
                      <Show when={checked(it())}>
                        <Icon name="check" />
                      </Show>
                    </span>
                    <span class="fe-menu__label">{it().label}</span>
                    <span class="fe-menu__key">
                      <Show when={it().submenu} fallback={it().hint ?? key(it().command)}>
                        <Icon name="expand" />
                      </Show>
                    </span>
                  </div>
                )}
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
        </Index>
      </div>
    </Portal>
  );
````

на:

````tsx
        </Index>
      </div>
      <Show when={sub()}>
        {(s) => (
          <Level
            items={entry(s().index)?.submenu ?? []}
            anchor={s().el}
            label={entry(s().index)?.label ?? ''}
            tree={props.tree}
            depth={props.depth + 1}
            focusFirst={s().focus}
            closeAll={props.closeAll}
            closeSelf={() => closeSub(true)}
            onHover={() => clearTimeout(timer)}
          />
        )}
      </Show>
    </Portal>
  );
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  /** Семейства текстов текущей страницы (группа «В документе» выбора шрифта). */
  pageFonts: string[];
}

````

на:

````ts
  /** Семейства текстов текущей страницы (группа «В документе» выбора шрифта). */
  pageFonts: string[];
  /** Состояние команд по ключу (доступна, отмечена) — как его прислало ядро. */
  commands: Record<string, CommandStatus>;
}

/** Состояние команды: `checked` есть только у переключателей. */
export interface CommandStatus {
  enabled: boolean;
  checked?: boolean;
}

````

В `apps/web/src/ui/store.ts` заменить:

````ts
  textStyle: null,
  pageFonts: [],
};

````

на:

````ts
  textStyle: null,
  pageFonts: [],
  commands: {},
};

````

В `apps/web/src/ui/store.ts` заменить:

````ts
        setState('pageFonts', e.families);
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
````

на:

````ts
        setState('pageFonts', e.families);
        break;
      case 'commandStates':
        // Только изменившиеся команды: остальные остаются как были.
        for (const c of e.changed) setState('commands', c.key, { enabled: c.enabled, checked: c.checked ?? undefined });
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
````

В `apps/web/src/ui/styles/chrome.css` заменить:

````css
.fe-confirm > .fe-ico {
  color: var(--warning);
}
````

на:

````css
.fe-confirm > .fe-ico {
  color: var(--warning);
}

/* Подменю и поиск главного меню (подсистема 2, этап 1). */
.fe-menu__item[aria-expanded='true'] {
  background: var(--surface-hover);
}
.fe-menu__item[aria-haspopup] .fe-menu__key {
  display: flex;
  align-items: center;
}
.fe-menu__item[aria-haspopup] .fe-menu__key svg {
  width: 12px;
  height: 12px;
}
.fe-menu__search {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-1);
  padding: 0 var(--space-2);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--ink-muted);
}
.fe-menu__search svg {
  width: 14px;
  height: 14px;
  flex: none;
}
.fe-menu__search input {
  flex: 1;
  min-width: 0;
  height: var(--row-height);
  border: 0;
  outline: 0;
  background: transparent;
  color: var(--ink);
  font: inherit;
}
.fe-menu__search:focus-within {
  outline: 2px solid var(--select);
  outline-offset: -2px;
}
.fe-menu__empty {
  padding-bottom: var(--space-1);
}
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/chrome.spec.ts --project=chromium`

Expected: модульные — 4 новых в `menu.test.ts` зелёные, сборка — ок; E2E `chrome` в Chromium — 26 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/chrome.spec.ts apps/web/src/ui/menu-logic.ts apps/web/src/ui/menu.tsx apps/web/src/ui/store.ts apps/web/src/ui/styles/chrome.css apps/web/unit/menu.test.ts
git commit -m "интерфейс: меню с подменю, первой буквой и состоянием команд

Компонент меню получил подменю (стрелка вправо, Enter, щелчок и наведение с
задержкой 150 мс, как в Figma; стрелка влево и Esc возвращают к пункту-
родителю), переход по первой букве (по кругу, без учёта регистра и «ё»),
ходьбу клавишами по серым пунктам (они не выполняются) и поле поиска вверху
для главного меню. Доступность и галочка пункта команды — из состояния
команд ядра (CommandStates в состоянии интерфейса), подпись — из действующей
раскладки. Положение — в фазе measure: подменю справа от пункта, у края окна
отражается налево и поднимается.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6: Главное меню в интерфейсе: логотип, Alt+/, поиск, команды `ui.*`

Спека §4: кнопка с логотипом на месте меню файла, команда `ui.main_menu` (Alt+/), поиск. Здесь — `main-menu.ts` (дерево ядра → пункты меню, поиск в обеих раскладках), `FileHeader` с кнопкой «Главное меню» (`.fe-logo`, `data-command="ui.main_menu"`) слева и «Назад к файлам» справа, исполнитель команд `ui.*` в `App` (`Editor.uiCommand`; событие ядра `uiCommand` — тот же обработчик), `SetTheme` с `system`; `UiState.mainMenu`; щелчок по якорю не закрывает-открывает меню заново; E2E главного меню, прежние тесты меню файла — через главное меню (`mainMenu(page, ...path)`).

**Files:**
- Create: `apps/web/src/ui/main-menu.ts`
- Modify: `apps/web/src/ui/{app.tsx,chrome.tsx,boot.ts,context.ts,menu.tsx,properties.tsx,store.ts}`, `apps/web/src/ui/styles/chrome.css`
- Test: `apps/web/unit/main-menu.test.ts` (новый), `apps/web/e2e/main-menu.spec.ts` (новый), `apps/web/e2e/helpers.ts` (`mainMenu`), `apps/web/e2e/{a11y,chrome,clipboard,files,fsa,journal,settings,settings-store}.spec.ts`

**Interfaces:**
- Consumes: `UiState.commands`, `CoreEvent::{MainMenu, UiCommand}`, `KeymapEntry { keywords }`, компонент меню с `search` и `submenu`.
- Produces: `main-menu.ts`: `normalize`, `layoutVariants`, `entriesByKey`, `buildMainMenu(tree, entries, hidden)`, `searchMenu(tree, entries, query, hidden)`, `MAX_RESULTS = 50`; `Editor.uiCommand?: (key) => void`; `FileHeader { menu, search, open, onOpen, onBack }`; `UiState.mainMenu: MenuItemDto[]`; `data-export-run` у кнопки секции экспорта; E2E-помощник `mainMenu(page, ...path)`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/a11y.spec.ts` заменить:

````ts

import { expect, test, type Page } from '@playwright/test';
import { openApp } from './helpers';

/** Контраст двух цветов `#rgb` / `#rrggbb` по WCAG (сборка сокращает `#ffffff`). */
````

на:

````ts

import { expect, test, type Page } from '@playwright/test';
import { mainMenu, openApp } from './helpers';

/** Контраст двух цветов `#rgb` / `#rrggbb` по WCAG (сборка сокращает `#ffffff`). */
````

В `apps/web/e2e/a11y.spec.ts` заменить:

````ts
  await expect(page.getByRole('tablist', { name: 'Режим панели' })).toBeVisible();
  await expect(page.getByRole('listbox', { name: 'Страницы' })).toBeVisible();
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await expect(page.getByRole('menu', { name: 'Меню файла' })).toBeVisible();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  await expect(page.getByRole('dialog', { name: 'Настройки' })).toBeVisible();
});
````

на:

````ts
  await expect(page.getByRole('tablist', { name: 'Режим панели' })).toBeVisible();
  await expect(page.getByRole('listbox', { name: 'Страницы' })).toBeVisible();
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await expect(page.getByRole('menu', { name: 'Главное меню' })).toBeVisible();
  await page.keyboard.press('Escape');
  await mainMenu(page, 'Настройки', 'Настройки…');
  await expect(page.getByRole('dialog', { name: 'Настройки' })).toBeVisible();
});
````

В `apps/web/e2e/a11y.spec.ts` заменить:

````ts
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

на:

````ts
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await openApp(page);
  await page.getByRole('button', { name: 'Главное меню' }).click();
  const anim = await page.getByRole('menu').evaluate((el) => getComputedStyle(el).animationName);
  expect(anim).toBe('none');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Главное меню' }).click();
  expect(await page.getByRole('menu').evaluate((el) => getComputedStyle(el).animationName)).toBe('fe-pop');
});
````

В `apps/web/e2e/a11y.spec.ts` заменить:

````ts
test('диалог настроек: модальный, фокус внутри, Esc закрывает', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
````

на:

````ts
test('диалог настроек: модальный, фокус внутри, Esc закрывает', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Настройки', 'Настройки…');
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
````

В `apps/web/e2e/a11y.spec.ts` заменить:

````ts
  await openApp(page);
  await page.getByRole('treeitem', { name: 'Экран 9', exact: true }).click();
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const scan = () =>
    page.evaluate(() =>
````

на:

````ts
  await openApp(page);
  await page.getByRole('treeitem', { name: 'Экран 9', exact: true }).click();
  await page.getByRole('button', { name: 'Главное меню' }).click();
  const scan = () =>
    page.evaluate(() =>
````

В `apps/web/e2e/a11y.spec.ts` заменить:

````ts
  await page.keyboard.press('Escape');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menu').waitFor();
  expect((await scan()).length, 'сканировка видит анимацию меню').toBeGreaterThan(0);
````

на:

````ts
  await page.keyboard.press('Escape');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await page.getByRole('menu').waitFor();
  expect((await scan()).length, 'сканировка видит анимацию меню').toBeGreaterThan(0);
````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
import { shortcutLabel } from '../src/ui/shortcuts';
import type { Platform } from '../src/core/core';
import { appScreen, drag, live, liveWhen, openApp } from './helpers';

const ui = <T>(page: Page, f: (s: NonNullable<Window['__ferrum']>['ui'] & object) => T) =>
````

на:

````ts
import { shortcutLabel } from '../src/ui/shortcuts';
import type { Platform } from '../src/core/core';
import { appScreen, drag, live, liveWhen, mainMenu, openApp } from './helpers';

const ui = <T>(page: Page, f: (s: NonNullable<Window['__ferrum']>['ui'] & object) => T) =>
````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  const total = (await live(page)).nodes;
  const first = page.url();
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: 'Новый файл' }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect.poll(() => page.url()).not.toBe(first);
````

на:

````ts
  const total = (await live(page)).nodes;
  const first = page.url();
  await mainMenu(page, 'Файл', 'Новый файл');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect.poll(() => page.url()).not.toBe(first);
````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
    panel.getByRole('button', { name: 'Цвет заливки' }).first(),
    panel.getByRole('button', { name: 'Удалить обводку' }),
    page.getByRole('button', { name: 'Меню файла' }),
    page.getByRole('button', { name: 'Масштаб', exact: true }),
    page.getByRole('button', { name: 'Страницы', exact: true }),
````

на:

````ts
    panel.getByRole('button', { name: 'Цвет заливки' }).first(),
    panel.getByRole('button', { name: 'Удалить обводку' }),
    page.getByRole('button', { name: 'Главное меню' }),
    page.getByRole('button', { name: 'Масштаб', exact: true }),
    page.getByRole('button', { name: 'Страницы', exact: true }),
````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts

const fileMenu = async (page: Page, item: string | RegExp) => {
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: item }).click();
};

````

на:

````ts

const fileMenu = async (page: Page, item: string | RegExp) => {
  await mainMenu(page, 'Файл', item);
};

````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

test.use({ permissions: ['clipboard-read', 'clipboard-write'] });
````

на:

````ts

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

test.use({ permissions: ['clipboard-read', 'clipboard-write'] });
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
  const before = await live(page);
  await page.evaluate(() => navigator.clipboard.writeText('исходный текст'));
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
````

на:

````ts
  const before = await live(page);
  await page.evaluate(() => navigator.clipboard.writeText('исходный текст'));
  await mainMenu(page, 'Настройки', 'Настройки…');
  const dialog = page.getByRole('dialog');
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
  });
  await openApp(page, 300);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const download = page.waitForEvent('download');
  await page.getByRole('menuitem', { name: 'Сохранить копию на диск…' }).click();
````

на:

````ts
  });
  await openApp(page, 300);
  await mainMenu(page, 'Файл');
  const download = page.waitForEvent('download');
  await page.getByRole('menuitem', { name: 'Сохранить копию на диск…' }).click();
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
  // «Открыть…» кладёт файл в хранилище новой записью и открывает её.
  const first = page.url();
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('menuitem', { name: 'Открыть…' }).click();
````

на:

````ts
  // «Открыть…» кладёт файл в хранилище новой записью и открывает её.
  const first = page.url();
  await mainMenu(page, 'Файл');
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('menuitem', { name: 'Открыть…' }).click();
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setDocumentName', name: 'План: этаж 1/2' }));
  await expect.poll(async () => (await ui(page)).documentName).toBe('План: этаж 1/2');
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const download = page.waitForEvent('download');
  await page.getByRole('menuitem', { name: 'Сохранить копию на диск…' }).click();
````

на:

````ts
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setDocumentName', name: 'План: этаж 1/2' }));
  await expect.poll(async () => (await ui(page)).documentName).toBe('План: этаж 1/2');
  await mainMenu(page, 'Файл');
  const download = page.waitForEvent('download');
  await page.getByRole('menuitem', { name: 'Сохранить копию на диск…' }).click();
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
  const edited = await liveWhen(page, 'l.steps === 1');
  const first = page.url();
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: 'Открыть…' }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await (await chooser).setFiles({ name: 'битый.ferrum', mimeType: 'application/octet-stream', buffer: Buffer.from('не ferrum') });
````

на:

````ts
  const edited = await liveWhen(page, 'l.steps === 1');
  const first = page.url();
  await mainMenu(page, 'Файл');
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('menu', { name: 'Файл', exact: true }).getByRole('menuitem', { name: 'Открыть…' }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await (await chooser).setFiles({ name: 'битый.ferrum', mimeType: 'application/octet-stream', buffer: Buffer.from('не ferrum') });
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
    }).observe({ type: 'longtask', buffered: false });
  });
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const download = page.waitForEvent('download');
  const t0 = await page.evaluate(() => performance.now());
````

на:

````ts
    }).observe({ type: 'longtask', buffered: false });
  });
  await mainMenu(page, 'Файл');
  const download = page.waitForEvent('download');
  const t0 = await page.evaluate(() => performance.now());
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
  });
  await openApp(page, 10);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: 'Открыть…' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Не удалось открыть файл: больше 512 МБ' })).toBeVisible();
````

на:

````ts
  });
  await openApp(page, 10);
  await mainMenu(page, 'Файл');
  await page.getByRole('menuitem', { name: 'Открыть…' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Не удалось открыть файл: больше 512 МБ' })).toBeVisible();
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
  await cutAt('.fe-picker button');
  await page.keyboard.press('Escape');
  // Меню файла.
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await expect(page.getByRole('menu', { name: 'Меню файла' })).toBeVisible();
  await cutAt('[role="menu"] [role="menuitem"]');
  await page.keyboard.press('Escape');
````

на:

````ts
  await cutAt('.fe-picker button');
  await page.keyboard.press('Escape');
  // Главное меню.
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await expect(page.getByRole('menu', { name: 'Главное меню' })).toBeVisible();
  await cutAt('[role="menu"] [role="menuitem"]');
  await page.keyboard.press('Escape');
````

В `apps/web/e2e/files.spec.ts` заменить:

````ts

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, expectCore, live, liveWhen } from './helpers';

/** Тестовый режим и учёт worker'ов: какие созданы и какие остановлены. */
````

на:

````ts

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, expectCore, live, liveWhen, mainMenu } from './helpers';

/** Тестовый режим и учёт worker'ов: какие созданы и какие остановлены. */
````

В `apps/web/e2e/files.spec.ts` заменить:

````ts

const fileMenu = async (page: Page, item: string) => {
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: item }).click();
};

````

на:

````ts

const fileMenu = async (page: Page, item: string) => {
  await mainMenu(page, 'Файл', item);
};

````

В `apps/web/e2e/fsa.spec.ts` заменить:

````ts

import { expect, test, type Page } from '@playwright/test';
import { drawRect, editorReady, fileId, liveWhen, newFileUrl, storedFile, testMode } from './helpers';

/** Диалоги выбора — файлы OPFS: открыть — `window.__pick` (или «проект.ferrum»),
````

на:

````ts

import { expect, test, type Page } from '@playwright/test';
import { drawRect, editorReady, fileId, liveWhen, mainMenu, newFileUrl, storedFile, testMode } from './helpers';

/** Диалоги выбора — файлы OPFS: открыть — `window.__pick` (или «проект.ferrum»),
````

В `apps/web/e2e/fsa.spec.ts` заменить:

````ts
const toast = (page: Page, text: string) => page.locator('.fe-toast', { hasText: text });
const fileMenu = async (page: Page, item: string) => {
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: item }).click();
};

````

на:

````ts
const toast = (page: Page, text: string) => page.locator('.fe-toast', { hasText: text });
const fileMenu = async (page: Page, item: string) => {
  await mainMenu(page, 'Файл', item);
};

````

В `apps/web/e2e/fsa.spec.ts` заменить:

````ts
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const menu = page.getByRole('menu', { name: 'Меню файла' });
  await expect(menu.getByRole('menuitem', { name: 'Открыть…' })).toBeVisible();
  await expect(menu.getByRole('menuitem', { name: 'Открыть с диска…' })).toHaveCount(0);
````

на:

````ts
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await mainMenu(page, 'Файл');
  const menu = page.getByRole('menu', { name: 'Файл', exact: true });
  await expect(menu.getByRole('menuitem', { name: 'Открыть…' })).toBeVisible();
  await expect(menu.getByRole('menuitem', { name: 'Открыть с диска…' })).toHaveCount(0);
````

В `apps/web/e2e/helpers.ts` заменить:

````ts
  return id;
}
````

на:

````ts
  return id;
}

/** Главное меню (кнопка с логотипом) и пункты по пути подменю:
 * `mainMenu(page, 'Файл', 'Новый файл')`. Подменю открываются щелчком; без пути —
 * только само меню, с одним шагом — меню и подменю. */
export async function mainMenu(page: Page, ...path: (string | RegExp)[]): Promise<void> {
  await page.getByRole('button', { name: 'Главное меню' }).click();
  let menu = page.getByRole('menu', { name: 'Главное меню' });
  for (const [i, step] of path.entries()) {
    const last = i === path.length - 1;
    const name = typeof step === 'string' && !last ? { name: step, exact: true } : { name: step };
    await menu.getByRole('menuitem', name).or(menu.getByRole('menuitemcheckbox', name)).first().click();
    if (!last || typeof step === 'string') menu = page.getByRole('menu', { name: step as string, exact: true });
  }
}
````

В `apps/web/e2e/journal.spec.ts` заменить:

````ts

import { expect, test, type Page } from '@playwright/test';
import { drawRect, editorReady, fileId, journalCount, live, liveWhen, storedFile } from './helpers';

const meta = (page: Page, id: string) => page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
````

на:

````ts

import { expect, test, type Page } from '@playwright/test';
import { drawRect, editorReady, fileId, journalCount, live, liveWhen, mainMenu, storedFile } from './helpers';

const meta = (page: Page, id: string) => page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
````

В `apps/web/e2e/journal.spec.ts` заменить:

````ts
  await drawRect(page, 150);
  await expect.poll(() => journalCount(page, id), { timeout: 10_000 }).toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: 'Все файлы' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  expect(await journalCount(page, id)).toBe(0);
````

на:

````ts
  await drawRect(page, 150);
  await expect.poll(() => journalCount(page, id), { timeout: 10_000 }).toBeGreaterThan(0);
  await mainMenu(page, 'Файл', 'Все файлы');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  expect(await journalCount(page, id)).toBe(0);
````

Создать `apps/web/e2e/main-menu.spec.ts`:

````ts
// Главное меню (спека подсистемы 2, раздел 4): кнопка с логотипом и Alt+/,
// подменю клавиатурой и наведением, серые пункты и галочки — из состояния
// команд ядра, сочетания из раскладки, поиск по командам в любой раскладке,
// подменю у края окна. Открытие меню и ввод в поиск не читают раскладку вне
// фазы measure (`__ferrumWarnings`).

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

const root = (page: Page) => page.getByRole('menu', { name: 'Главное меню' });
const submenu = (page: Page, name: string) => page.getByRole('menu', { name, exact: true });
const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

/** Выделить «Слой 1» первого фрейма щелчком на холсте (мир (200, 40)). */
async function selectNode(page: Page): Promise<void> {
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
}

test('логотип открывает меню с подменю Figma; клавиатура ходит внутрь и обратно', async ({ page }) => {
  await openApp(page);
  const logo = page.getByRole('button', { name: 'Главное меню' });
  await logo.click();
  await expect(logo).toHaveAttribute('aria-expanded', 'true');
  const menu = root(page);
  await expect(menu.getByRole('searchbox', { name: 'Поиск по командам' })).toBeFocused();
  await expect(menu.getByRole('menuitem')).toHaveText(['Файл', 'Правка', 'Вид', 'Объект', 'Текст', 'Расставить', 'Вектор', 'Настройки', 'Справка']);
  // Вниз — к первому пункту, вправо — в подменю на первый пункт.
  await page.keyboard.press('ArrowDown');
  const file = menu.getByRole('menuitem', { name: 'Файл' });
  await expect(file).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(file).toHaveAttribute('aria-expanded', 'true');
  await expect(submenu(page, 'Файл').getByRole('menuitem', { name: 'Новый файл' })).toBeFocused();
  // Влево — обратно к «Файлу», подменю закрыто.
  await page.keyboard.press('ArrowLeft');
  await expect(submenu(page, 'Файл')).toHaveCount(0);
  await expect(file).toBeFocused();
  // Вниз и Enter: «Правка», затем Esc — к «Правке», ещё Esc — меню закрыто, фокус — на логотипе.
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await expect(submenu(page, 'Правка').getByRole('menuitem').first()).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu.getByRole('menuitem', { name: 'Правка', exact: true })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
  await expect(logo).toBeFocused();
  expect(await warnings(page)).toEqual([]);
});

test('Alt+/ на холсте открывает главное меню (команда ui.main_menu через ядро)', async ({ page }) => {
  await openApp(page);
  await page.locator('canvas').click({ position: { x: 600, y: 40 } });
  await page.keyboard.press('Alt+Slash');
  await expect(root(page)).toBeVisible();
  await expect(root(page).getByRole('searchbox')).toBeFocused();
  // Повторный щелчок по логотипу закрывает меню.
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await expect(root(page)).toHaveCount(0);
});

test('подменю открывается наведением с задержкой и закрывается у соседнего пункта', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await root(page).getByRole('menuitem', { name: 'Объект' }).hover();
  const object = submenu(page, 'Объект');
  await expect(object).toBeVisible();
  // Подменю в подменю: «Булевы операции».
  await object.getByRole('menuitem', { name: 'Булевы операции' }).hover();
  await expect(submenu(page, 'Булевы операции').getByRole('menuitemcheckbox')).toHaveCount(4);
  // Соседний пункт верхнего уровня без подменю нет — «Вид» открывает своё.
  await root(page).getByRole('menuitem', { name: 'Вид' }).hover();
  await expect(submenu(page, 'Вид')).toBeVisible();
  await expect(object).toHaveCount(0);
  expect(await warnings(page)).toEqual([]);
});

test('серые пункты и галочки — из состояния команд ядра, сочетания — из раскладки', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Правка');
  const edit = submenu(page, 'Правка');
  const dup = edit.getByRole('menuitem', { name: /Дублировать/ });
  await expect(dup).toHaveAttribute('aria-disabled', 'true');
  await expect(dup).toContainText(/D$/);
  // Серый пункт не выполняется и меню не закрывает.
  await dup.click({ force: true });
  await expect(edit).toBeVisible();
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  // С выделением — доступен; выполнение — один шаг.
  await selectNode(page);
  const before = await live(page);
  await mainMenu(page, 'Правка', /Дублировать/);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  // Переключатель: «Пиксельная сетка» отмечена, щелчок снимает отметку (ядро).
  await mainMenu(page, 'Вид');
  const grid = submenu(page, 'Вид').getByRole('menuitemcheckbox', { name: /Пиксельная сетка/ });
  await expect(grid).toHaveAttribute('aria-checked', 'true');
  await grid.click();
  await mainMenu(page, 'Вид');
  await expect(submenu(page, 'Вид').getByRole('menuitemcheckbox', { name: /Пиксельная сетка/ })).toHaveAttribute('aria-checked', 'false');
  await page.keyboard.press('Escape');
});

test('тема из «Настройки › Тема»: отметка у выбранной', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Настройки', 'Тема', 'Светлая тема');
  await expect.poll(() => page.evaluate(() => document.documentElement.dataset.theme)).toBe('light');
  await mainMenu(page, 'Настройки', 'Тема');
  const theme = submenu(page, 'Тема');
  await expect(theme.getByRole('menuitemcheckbox', { name: 'Светлая тема' })).toHaveAttribute('aria-checked', 'true');
  await expect(theme.getByRole('menuitemcheckbox', { name: 'Тёмная тема' })).toHaveAttribute('aria-checked', 'false');
  await theme.getByRole('menuitemcheckbox', { name: 'Тёмная тема' }).click();
  await expect.poll(() => page.evaluate(() => document.documentElement.dataset.theme)).toBe('dark');
});

test('поиск по командам: подстрока, синонимы, другая раскладка; Enter выполняет', async ({ page }) => {
  await openApp(page);
  await selectNode(page);
  const before = await live(page);
  await page.getByRole('button', { name: 'Главное меню' }).click();
  const search = root(page).getByRole('searchbox');
  // «le,km» — «дубль» в английской раскладке (синоним «Дублировать»).
  await search.fill('le,km');
  await expect(root(page).getByRole('menuitem')).toHaveText([/Дублировать/]);
  // «щквук» — «order» в русской раскладке.
  await search.fill('щквук');
  await expect(root(page).getByRole('menuitem').first()).toContainText('На передний план');
  await search.fill('нет такой команды');
  await expect(root(page).getByText('Ничего не найдено')).toBeVisible();
  await search.fill('дубл');
  await page.keyboard.press('Enter');
  await expect(root(page)).toHaveCount(0);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  expect(await warnings(page)).toEqual([]);
});

test('переход по первой букве в подменю', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Правка');
  const edit = submenu(page, 'Правка');
  await edit.getByRole('menuitem').first().focus();
  await page.evaluate(() => document.activeElement!.dispatchEvent(new KeyboardEvent('keydown', { key: 'д', bubbles: true })));
  await expect(edit.getByRole('menuitem', { name: /Дублировать/ })).toBeFocused();
});

test('подменю у нижнего края окна поднимается и остаётся на экране', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 420 });
  await openApp(page);
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await root(page).getByRole('menuitem', { name: 'Объект' }).click();
  const object = submenu(page, 'Объект');
  await expect(object).toBeVisible();
  // Появление меню анимировано масштабом: размеры — после анимации.
  const settled = (m: ReturnType<typeof submenu>) => m.evaluate((el) => Promise.all(el.getAnimations().map((x) => x.finished)).then(() => true));
  await settled(object);
  await settled(root(page));
  const box = (await object.boundingBox())!;
  expect(box.y + box.height).toBeLessThanOrEqual(420 - 8 + 0.5);
  expect(box.y).toBeGreaterThanOrEqual(8);
  // Справа от пункта «Объект».
  const item = (await root(page).getByRole('menuitem', { name: 'Объект' }).boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(item.x + item.width - 1);
});

test('команды интерфейса: «Экспорт…» сочетанием, «О программе» и «Настройки…» из меню', async ({ page }) => {
  await openApp(page);
  await page.locator('canvas').click({ position: { x: 600, y: 40 } });
  await page.keyboard.press('ControlOrMeta+Shift+KeyE');
  await expect(page.getByRole('button', { name: /Экспортировать/ })).toBeFocused();
  await mainMenu(page, 'Справка', 'О программе');
  await expect(page.getByRole('status').filter({ hasText: 'Ferrum' })).toBeVisible();
  await mainMenu(page, 'Настройки', 'Настройки…');
  await expect(page.getByRole('dialog', { name: 'Настройки' })).toBeVisible();
});
````

В `apps/web/e2e/settings-store.spec.ts` заменить:

````ts

import { expect, test, type Page } from '@playwright/test';
import { editorReady, newFileUrl, testMode } from './helpers';

const stored = (page: Page) => page.evaluate(() => window.__ferrumFiles!.store.settings());
````

на:

````ts

import { expect, test, type Page } from '@playwright/test';
import { editorReady, mainMenu, newFileUrl, testMode } from './helpers';

const stored = (page: Page) => page.evaluate(() => window.__ferrumFiles!.store.settings());
````

В `apps/web/e2e/settings-store.spec.ts` заменить:

````ts
  await expect(b.getByRole('heading', { level: 1 })).toBeVisible();
  // A: тема «Светлая» в настройках редактора.
  await a.getByRole('button', { name: 'Меню файла' }).click();
  await a.getByRole('menuitem', { name: /Настройки/ }).click();
  const dialog = a.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
````

на:

````ts
  await expect(b.getByRole('heading', { level: 1 })).toBeVisible();
  // A: тема «Светлая» в настройках редактора.
  await mainMenu(a, 'Настройки', 'Настройки…');
  const dialog = a.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
````

В `apps/web/e2e/settings-store.spec.ts` заменить:

````ts
  await page.goto(newFileUrl(10));
  await editorReady(page);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
````

на:

````ts
  await page.goto(newFileUrl(10));
  await editorReady(page);
  await mainMenu(page, 'Настройки', 'Настройки…');
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
````

В `apps/web/e2e/settings-store.spec.ts` заменить:

````ts
  await expect(a.getByRole('heading', { level: 1 })).toBeVisible();
  // B (страницы открыты) меняет тему: пишет только тему.
  await b.getByRole('button', { name: 'Меню файла' }).click();
  await b.getByRole('menuitem', { name: /Настройки/ }).click();
  const dialog = b.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
````

на:

````ts
  await expect(a.getByRole('heading', { level: 1 })).toBeVisible();
  // B (страницы открыты) меняет тему: пишет только тему.
  await mainMenu(b, 'Настройки', 'Настройки…');
  const dialog = b.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
````

В `apps/web/e2e/settings-store.spec.ts` заменить:

````ts
  // Как при закрытии вкладки: worker уничтожается раньше, чем запись дойдёт до базы.
  await page.evaluate(() => window.__ferrumFiles!.store.holdSettings(true));
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
````

на:

````ts
  // Как при закрытии вкладки: worker уничтожается раньше, чем запись дойдёт до базы.
  await page.evaluate(() => window.__ferrumFiles!.store.holdSettings(true));
  await mainMenu(page, 'Настройки', 'Настройки…');
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
````

В `apps/web/e2e/settings.spec.ts` заменить:

````ts

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, expectPixel, live, liveWhen, openApp } from './helpers';

async function openSettings(page: Page) {
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  return page.getByRole('dialog', { name: 'Настройки' });
}
````

на:

````ts

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, expectPixel, live, liveWhen, mainMenu, openApp } from './helpers';

async function openSettings(page: Page) {
  await mainMenu(page, 'Настройки', 'Настройки…');
  return page.getByRole('dialog', { name: 'Настройки' });
}
````

В `apps/web/e2e/settings.spec.ts` заменить:

````ts
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Меню файла' })).toBeFocused();
});

````

на:

````ts
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Главное меню' })).toBeFocused();
});

````

Создать `apps/web/unit/main-menu.test.ts`:

````ts
// Главное меню: пункты из дерева ядра и поиск по командам (подстрока, синонимы,
// раскладка ЙЦУКЕН ↔ QWERTY).

import { expect, test } from '@playwright/test';
import type { KeymapEntry, MenuItemDto } from '../src/wasm/ferrum_wasm';
import { buildMainMenu, entriesByKey, layoutVariants, MAX_RESULTS, searchMenu } from '../src/ui/main-menu';

const entry = (key: string, name: string, keywords: string[] = []): KeymapEntry => ({
  key,
  name,
  group: key.split('.')[0],
  keywords,
  toggle: false,
  bindings: [],
  labels: [],
  defaults: [],
  overridden: false,
});

const entries = entriesByKey([
  entry('ui.new_file', 'Новый файл', ['создать', 'new file']),
  entry('ui.open_disk', 'Открыть с диска…', ['open from disk']),
  entry('ui.save_disk', 'Сохранить на диск…', ['save to disk']),
  entry('file.save', 'Сохранить', ['save']),
  entry('edit.duplicate', 'Дублировать', ['дубль', 'копия', 'duplicate']),
  entry('arrange.front', 'На передний план', ['порядок', 'bring to front', 'order']),
  entry('edit.copy_png', 'Копировать как PNG', ['copy as png']),
]);

const c = (key: string): MenuItemDto => ({ type: 'command', key });
const sep: MenuItemDto = { type: 'separator' };
const tree: MenuItemDto[] = [
  { type: 'submenu', title: 'Файл', items: [c('ui.new_file'), c('ui.open_disk'), sep, c('file.save'), c('ui.save_disk')] },
  { type: 'submenu', title: 'Правка', items: [c('edit.duplicate'), { type: 'submenu', title: 'Копировать как', items: [c('edit.copy_png')] }] },
  { type: 'submenu', title: 'Расставить', items: [c('arrange.front')] },
  { type: 'submenu', title: 'Пусто', items: [c('ui.unknown')] },
];

test('пункты из дерева: названия из таблицы, недоступное на платформе — пропущено', () => {
  const menu = buildMainMenu(tree, entries, new Set(['ui.open_disk', 'ui.save_disk']));
  expect(menu.map((m) => ('label' in m ? m.label : '—'))).toEqual(['Файл', 'Правка', 'Расставить']);
  const file = menu[0] as { submenu: { label?: string; command?: string; separator?: true }[] };
  expect(file.submenu.map((m) => m.command ?? '—')).toEqual(['ui.new_file', '—', 'file.save']);
  const edit = menu[1] as { submenu: { label: string; submenu?: unknown[] }[] };
  expect(edit.submenu[1].label).toBe('Копировать как');
});

test('поиск: подстрока названия и синонима, сначала начало названия', () => {
  const keys = (q: string) => searchMenu(tree, entries, q).map((m) => ('command' in m ? m.command : ''));
  expect(keys('сохр')).toEqual(['file.save', 'ui.save_disk']);
  expect(keys('копи')).toEqual(['edit.copy_png', 'edit.duplicate']);
  expect(keys('DUPLICATE')).toEqual(['edit.duplicate']);
  expect(keys('  ')).toEqual([]);
  expect(keys('нет такого')).toEqual([]);
});

test('поиск не зависит от раскладки: «щквук» — «order», «le,km» — «дубль»', () => {
  expect(layoutVariants('щквук')).toContain('order');
  expect(layoutVariants('le,km')).toContain('дубль');
  expect(searchMenu(tree, entries, 'щквук').map((m) => ('command' in m ? m.command : ''))).toEqual(['arrange.front']);
  expect(searchMenu(tree, entries, 'le,km').map((m) => ('command' in m ? m.command : ''))).toEqual(['edit.duplicate']);
  // «ё» и «е» — одно.
  expect(layoutVariants('ЁЛКА')).toContain('елка');
});

test('не больше 50 результатов', () => {
  const many = Array.from({ length: 80 }, (_, i) => entry(`x.c${i}`, `Команда ${i}`));
  const t: MenuItemDto[] = many.map((e) => c(e.key));
  expect(searchMenu(t, entriesByKey(many), 'команда')).toHaveLength(MAX_RESULTS);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/main-menu.test.ts`
Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/main-menu.spec.ts --project=chromium`
Expected: FAIL — модульные — `Error: Cannot find module '…/apps/web/src/ui/main-menu'` (No tests found); E2E (стенд собран) — 9 failed: семь — `locator.click: Test timeout of 120000ms exceeded` на `getByRole('button', { name: 'Главное меню' })` (кнопки нет), «Alt+/ на холсте…» — `toBeVisible` у `getByRole('menu', { name: 'Главное меню' })` (element(s) not found), ещё один — `toBeFocused` (`Received: inactive`)

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { EditorContext, type Editor } from './context';
import { ConfirmDialog } from './dialog';
import { Menu, type MenuItem } from './menu';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
````

на:

````tsx
import { EditorContext, type Editor } from './context';
import { ConfirmDialog } from './dialog';
import { Menu, type MenuSearch } from './menu';
import { buildMainMenu, entriesByKey, searchMenu } from './main-menu';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  createEffect(() => {
    document.documentElement.dataset.theme = theme();
    editor()?.send({ type: 'setTheme', theme: theme() });
  });
  createEffect(() => editor()?.send({ type: 'setSettings', pixelSnap: pixelSnap() }));
````

на:

````tsx
  createEffect(() => {
    document.documentElement.dataset.theme = theme();
    editor()?.send({ type: 'setTheme', theme: theme(), system: themeChoice() === 'system' });
  });
  createEffect(() => editor()?.send({ type: 'setSettings', pixelSnap: pixelSnap() }));
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      // До документа: тема (первый кадр сразу в цвете темы), привязка и сохранённая раскладка.
      beforeDocument: (ed) => {
        ed.send({ type: 'setTheme', theme: theme() });
        ed.send({ type: 'setSettings', pixelSnap: pixelSnap() });
        // Нынешняя раскладка: другая вкладка могла сменить её, пока ядро запускалось.
````

на:

````tsx
      // До документа: тема (первый кадр сразу в цвете темы), привязка и сохранённая раскладка.
      beforeDocument: (ed) => {
        ed.send({ type: 'setTheme', theme: theme(), system: themeChoice() === 'system' });
        ed.send({ type: 'setSettings', pixelSnap: pixelSnap() });
        // Нынешняя раскладка: другая вкладка могла сменить её, пока ядро запускалось.
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
          return;
        }
        setEditor(ed);
        canvas.focus();
````

на:

````tsx
          return;
        }
        ed.uiCommand = uiCommand;
        setEditor(ed);
        canvas.focus();
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        let recovered = false;
        ed.core.on((e) => {
          if (e.type === 'recovered') recovered = true;
          if (opening && e.type === 'opened') {
````

на:

````tsx
        let recovered = false;
        ed.core.on((e) => {
          if (e.type === 'uiCommand') uiCommand(e.key);
          if (e.type === 'recovered') recovered = true;
          if (opening && e.type === 'opened') {
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
    }
  };
  const disk = (items: MenuItem[]): MenuItem[] => (props.files.disk ? items : []);
  const fileItems = (ed: Editor): MenuItem[] => [
    { label: 'Новый файл', onSelect: () => props.navigate({ kind: 'new' }) },
    { label: 'Открыть…', onSelect: () => void openFromDisk() },
    ...disk([{ label: 'Открыть с диска…', onSelect: () => void openLinked() }]),
    { label: 'Сохранить', command: 'file.save', onSelect: () => void saver?.save() },
    ...disk([{ label: 'Сохранить на диск…', onSelect: () => void saveToDisk() }]),
    { label: 'Сохранить копию на диск…', onSelect: () => void saveCopy(ed) },
    { separator: true },
    { label: 'Все файлы', onSelect: () => props.navigate({ kind: 'files', section: 'recent' }) },
    { label: 'Настройки…', onSelect: () => setSettingsOpen(true) },
  ];

  return (
````

на:

````tsx
    }
  };
  // Главное меню (спека подсистемы 2, раздел 4): дерево — из ядра, названия — из
  // таблицы команд. Файла на диске без File System Access нет: его пунктов тоже.
  const [mainMenu, setMainMenu] = createSignal(false);
  const hidden = createMemo(() => new Set(props.files.disk ? [] : ['ui.open_disk', 'ui.save_disk']));
  const entries = createMemo(() => entriesByKey(store.state.keymap.commands));
  const mainItems = () => buildMainMenu(store.state.mainMenu, entries(), hidden());
  const search: MenuSearch = {
    label: 'Поиск по командам',
    placeholder: 'Найти команду',
    results: (q) => searchMenu(store.state.mainMenu, entries(), q, hidden()),
  };
  // Команды интерфейса (`ui.*`): пункт меню вызывает их сразу, сочетание на холсте —
  // через ядро (событие `uiCommand`).
  const uiCommand = (key: string) => {
    const ed = editor();
    if (!ed) return;
    switch (key) {
      case 'ui.main_menu':
        setMainMenu(true);
        break;
      case 'ui.new_file':
        props.navigate({ kind: 'new' });
        break;
      case 'ui.open':
        void openFromDisk();
        break;
      case 'ui.open_disk':
        if (props.files.disk) void openLinked();
        break;
      case 'ui.save_disk':
        if (props.files.disk) void saveToDisk();
        break;
      case 'ui.save_copy':
        void saveCopy(ed);
        break;
      case 'ui.export':
        // «Экспорт…»: к кнопке секции экспорта панели свойств (настройки — там же).
        document.querySelector<HTMLElement>('[data-export-run]')?.focus();
        break;
      case 'ui.all_files':
        props.navigate({ kind: 'files', section: 'recent' });
        break;
      case 'ui.settings':
      case 'ui.shortcuts':
        setSettingsOpen(true);
        break;
      case 'ui.about':
        store.toast('Ferrum — редактор векторной графики: ядро на Rust и WebAssembly, интерфейс — SolidJS');
        break;
      case 'ui.pixel_snap':
        setPixelSnap((v) => !v);
        break;
      case 'ui.theme_dark':
        setThemeChoice('dark');
        break;
      case 'ui.theme_light':
        setThemeChoice('light');
        break;
      case 'ui.theme_system':
        setThemeChoice('system');
        break;
    }
  };

  return (
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
          <EditorContext.Provider value={ed()}>
            <aside class="fe-side fe-side--left" aria-label="Страницы и слои">
              <FileHeader items={() => fileItems(ed())} onBack={() => props.navigate({ kind: 'files', section: 'recent' })} />
              <PagesPanel open={pagesOpen()} onToggle={setPagesOpen} />
              <LayersPanel />
````

на:

````tsx
          <EditorContext.Provider value={ed()}>
            <aside class="fe-side fe-side--left" aria-label="Страницы и слои">
              <FileHeader
                menu={mainItems}
                search={search}
                open={mainMenu()}
                onOpen={setMainMenu}
                onBack={() => props.navigate({ kind: 'files', section: 'recent' })}
              />
              <PagesPanel open={pagesOpen()} onToggle={setPagesOpen} />
              <LayersPanel />
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
    platform,
    send: (cmd) => core.send(cmd),
    run: (command) => core.send({ type: 'execute', command }),
    layout,
    showSystemFonts: () => void fonts.show(),
````

на:

````ts
    platform,
    send: (cmd) => core.send(cmd),
    run: (command) => {
      if (command.startsWith('ui.') && editor.uiCommand) editor.uiCommand(command);
      else core.send({ type: 'execute', command });
    },
    layout,
    showSystemFonts: () => void fonts.show(),
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
// Шапки панелей (PanelHeader), меню файла, зум, «Поделиться», вкладки, края
// панелей для изменения ширины и контекстное меню холста.

````

на:

````tsx
// Шапки панелей (PanelHeader), главное меню, зум, «Поделиться», вкладки, края
// панелей для изменения ширины и контекстное меню холста.

````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
import { useEditor } from './context';
import { Caret, Icon } from './icons';
import { Menu, type MenuItem } from './menu';
import { RenameInput } from './rename-input';
import { roving } from './roving';
````

на:

````tsx
import { useEditor } from './context';
import { Caret, Icon } from './icons';
import { Menu, type MenuItem, type MenuSearch } from './menu';
import { RenameInput } from './rename-input';
import { roving } from './roving';
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
export { clampPanel, PANEL_MAX, PANEL_MIN };

/** Пункты меню файла: другие части интерфейса добавляют свои (открыть, сохранить,
 * настройки). `onBack` — кнопка «Назад к файлам». */
export function FileHeader(props: { items: () => MenuItem[]; onBack?: () => void }) {
  const ed = useEditor();
  const [menu, setMenu] = createSignal<HTMLElement | null>(null);
  const [editing, setEditing] = createSignal(false);
  let file!: HTMLDivElement;
  return (
    <div class="fe-panel-head">
      <Show when={props.onBack} fallback={<span class="fe-panel-head__mark" aria-hidden="true" />}>
        {(back) => (
          <button class="fe-tool fe-tool--small fe-tool--muted" aria-label="Назад к файлам" data-tip="Назад к файлам" onClick={() => back()()}>
            <Icon name="back" />
          </button>
        )}
      </Show>
      <div ref={file} class="fe-panel-head__file">
        <Show
````

на:

````tsx
export { clampPanel, PANEL_MAX, PANEL_MIN };

/** Шапка левой панели: кнопка с логотипом открывает главное меню (как в Figma
 * UI3; оно же — по `ui.main_menu`, Alt+/), имя файла, «Назад к файлам».
 * Открыто ли меню, решает редактор (`open`): его открывает и сочетание. */
export function FileHeader(props: {
  menu: () => MenuItem[];
  search: MenuSearch;
  open: boolean;
  onOpen: (open: boolean) => void;
  onBack?: () => void;
}) {
  const ed = useEditor();
  const [editing, setEditing] = createSignal(false);
  let file!: HTMLDivElement;
  let logo!: HTMLButtonElement;
  return (
    <div class="fe-panel-head">
      <button
        ref={logo}
        class="fe-logo"
        aria-haspopup="menu"
        aria-expanded={props.open}
        aria-label="Главное меню"
        data-tip="Главное меню"
        data-command="ui.main_menu"
        onClick={() => props.onOpen(!props.open)}
      >
        <span class="fe-panel-head__mark" aria-hidden="true" />
        <Caret />
      </button>
      <div ref={file} class="fe-panel-head__file">
        <Show
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
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
````

на:

````tsx
        <span class="fe-panel-head__project">Черновики</span>
      </div>
      <Show when={props.onBack}>
        {(back) => (
          <button class="fe-tool fe-tool--small fe-tool--muted" aria-label="Назад к файлам" data-tip="Назад к файлам" onClick={() => back()()}>
            <Icon name="back" />
          </button>
        )}
      </Show>
      <Show when={props.open}>
        <Menu label="Главное меню" items={props.menu()} anchor={logo} search={props.search} onClose={() => props.onOpen(false)} />
      </Show>
    </div>
````

В `apps/web/src/ui/context.ts` заменить:

````ts
  platform: Platform;
  send(cmd: Command): void;
  /** Команда редактора по ключу таблицы (`edit.undo`). */
  run(command: string): void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
````

на:

````ts
  platform: Platform;
  send(cmd: Command): void;
  /** Команда редактора по ключу таблицы (`edit.undo`). Команды интерфейса
   * (`ui.*`) выполняются сразу здесь (`uiCommand`), без круга через ядро: выбор
   * файла требует, чтобы его вызвал жест пользователя. */
  run(command: string): void;
  /** Исполнитель команд интерфейса (`ui.*`): ставит редактор приложения; ядро
   * сообщает о них событием `uiCommand` (сочетание клавиш на холсте). */
  uiCommand?: (key: string) => void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
````

Создать `apps/web/src/ui/main-menu.ts`:

````ts
// Главное меню (спека подсистемы 2, раздел 4): пункты из дерева ядра
// (`MainMenu`) с названиями из таблицы команд и поиск по командам меню —
// подстрока названия или синонима, без учёта регистра, «ё» и раскладки
// (запрос «щквук» находит «order»: буквы переводятся ЙЦУКЕН ↔ QWERTY).
// Основной поток ничего не вычисляет о документе: доступность и галочки —
// из состояния команд ядра, здесь — только названия и отбор по строке.

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

export function entriesByKey(commands: readonly KeymapEntry[]): Map<string, KeymapEntry> {
  return new Map(commands.map((c) => [c.key, c]));
}

/** Убирает разделители в начале, в конце и подряд (после пропуска пунктов). */
function tidy(items: MenuItem[]): MenuItem[] {
  const out: MenuItem[] = [];
  for (const it of items) {
    if ('separator' in it && (out.length === 0 || 'separator' in out[out.length - 1])) continue;
    out.push(it);
  }
  while (out.length && 'separator' in out[out.length - 1]) out.pop();
  return out;
}

/** Пункты меню из дерева ядра. `hidden` — команды, которых на этой платформе
 * нет (файл на диске без File System Access): пункты пропускаются, пустые
 * подменю тоже. */
export function buildMainMenu(tree: readonly MenuItemDto[], entries: Entries, hidden: ReadonlySet<string> = new Set()): MenuItem[] {
  const out: MenuItem[] = [];
  for (const n of tree) {
    if (n.type === 'separator') out.push({ separator: true });
    else if (n.type === 'submenu') {
      const items = buildMainMenu(n.items, entries, hidden);
      if (items.length) out.push({ label: n.title, submenu: items });
    } else if (!hidden.has(n.key)) {
      const e = entries.get(n.key);
      if (e) out.push({ label: e.name, command: e.key });
    }
  }
  return tidy(out);
}

/** Ключи команд дерева по порядку, без повторов. */
function treeKeys(tree: readonly MenuItemDto[], out: string[] = []): string[] {
  for (const n of tree) {
    if (n.type === 'command' && !out.includes(n.key)) out.push(n.key);
    else if (n.type === 'submenu') treeKeys(n.items, out);
  }
  return out;
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

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
  onMount(() => {
    const outside = (e: PointerEvent) => {
      if (!(e.target instanceof Element && e.target.closest(`[data-menu-tree="${tree}"]`))) closeAll(false);
    };
    // Нажатие, которым меню открыли, ещё всплывает: слушаем со следующей задачи.
````

на:

````tsx
  onMount(() => {
    const outside = (e: PointerEvent) => {
      const t = e.target instanceof Element ? e.target : null;
      // Якорь-кнопка сама закрывает меню повторным щелчком.
      const anchor = props.anchor instanceof HTMLElement && t && props.anchor.contains(t);
      if (!(t?.closest(`[data-menu-tree="${tree}"]`) || anchor)) closeAll(false);
    };
    // Нажатие, которым меню открыли, ещё всплывает: слушаем со следующей задачи.
````

В `apps/web/src/ui/menu.tsx` заменить:

````tsx
        aria-label={props.label}
        data-menu-tree={props.tree}
        style={{ visibility: 'hidden' }}
        onKeyDown={onKey}
        onPointerEnter={() => props.onHover?.()}
````

на:

````tsx
        aria-label={props.label}
        data-menu-tree={props.tree}
        // Невидимым — в углу окна: размер не зависит от места в потоке документа.
        style={{ visibility: 'hidden', left: '0px', top: '0px' }}
        onKeyDown={onKey}
        onPointerEnter={() => props.onHover?.()}
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
        </label>
      </Show>
      <button class="fe-btn fe-btn--soft fe-btn--wide" onClick={run}>
        <Icon name="download" />
        <span class="fe-ellipsis">Экспортировать {target()}</span>
````

на:

````tsx
        </label>
      </Show>
      <button class="fe-btn fe-btn--soft fe-btn--wide" data-export-run onClick={run}>
        <Icon name="download" />
        <span class="fe-ellipsis">Экспортировать {target()}</span>
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  FontChoice,
  KeymapEntry,
  TextStyleDto,
  MissingFontDto,
````

на:

````ts
  FontChoice,
  KeymapEntry,
  MenuItemDto,
  TextStyleDto,
  MissingFontDto,
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  /** Состояние команд по ключу (доступна, отмечена) — как его прислало ядро. */
  commands: Record<string, CommandStatus>;
}

````

на:

````ts
  /** Состояние команд по ключу (доступна, отмечена) — как его прислало ядро. */
  commands: Record<string, CommandStatus>;
  /** Дерево главного меню из ядра (ключи команд). */
  mainMenu: MenuItemDto[];
}

````

В `apps/web/src/ui/store.ts` заменить:

````ts
  pageFonts: [],
  commands: {},
};

````

на:

````ts
  pageFonts: [],
  commands: {},
  mainMenu: [],
};

````

В `apps/web/src/ui/store.ts` заменить:

````ts
        setState('pageFonts', e.families);
        break;
      case 'commandStates':
        // Только изменившиеся команды: остальные остаются как были.
````

на:

````ts
        setState('pageFonts', e.families);
        break;
      case 'mainMenu':
        setState('mainMenu', e.tree);
        break;
      case 'commandStates':
        // Только изменившиеся команды: остальные остаются как были.
````

В `apps/web/src/ui/styles/chrome.css` заменить:

````css
  padding-bottom: var(--space-1);
}
````

на:

````css
  padding-bottom: var(--space-1);
}
/* Кнопка с логотипом: главное меню (как в Figma UI3). */
.fe-logo {
  display: flex;
  align-items: center;
  gap: 2px;
  flex: none;
  height: 28px;
  padding: 0 var(--space-1);
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--ink-muted);
}
.fe-logo:hover,
.fe-logo[aria-expanded='true'] {
  background: var(--surface-hover);
}
.fe-logo:focus-visible {
  outline: 2px solid var(--select);
  outline-offset: -2px;
}
.fe-logo svg {
  width: 12px;
  height: 12px;
}
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/main-menu.spec.ts e2e/chrome.spec.ts e2e/a11y.spec.ts e2e/clipboard.spec.ts e2e/files.spec.ts e2e/fsa.spec.ts e2e/journal.spec.ts e2e/settings.spec.ts e2e/settings-store.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/main-menu.spec.ts --project=firefox`

Expected: модульные — `main-menu.test.ts` (4) зелёные, всего 148 passed; сборка — ок; E2E главного меню (9 тестов) и перенесённые тесты меню файла в Chromium — без падений; `main-menu` в Firefox — 9 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/a11y.spec.ts apps/web/e2e/chrome.spec.ts apps/web/e2e/clipboard.spec.ts apps/web/e2e/files.spec.ts apps/web/e2e/fsa.spec.ts apps/web/e2e/helpers.ts apps/web/e2e/journal.spec.ts apps/web/e2e/main-menu.spec.ts apps/web/e2e/settings-store.spec.ts apps/web/e2e/settings.spec.ts apps/web/src/ui/app.tsx apps/web/src/ui/boot.ts apps/web/src/ui/chrome.tsx apps/web/src/ui/context.ts apps/web/src/ui/main-menu.ts apps/web/src/ui/menu.tsx apps/web/src/ui/properties.tsx apps/web/src/ui/store.ts apps/web/src/ui/styles/chrome.css apps/web/unit/main-menu.test.ts
git commit -m "интерфейс: главное меню по кнопке с логотипом и Alt+/, поиск по командам

Кнопка с логотипом в шапке левой панели заменила меню файла и открывает
главное меню из дерева ядра (названия — из таблицы команд); то же меню — по
команде ui.main_menu (Alt+/ на холсте, событие uiCommand). Пункты команд
интерфейса выполняются сразу в основном потоке (выбор файла требует жеста
пользователя), остальные — Execute. Поле поиска вверху отбирает команды меню
по подстроке названия и синонима без учёта регистра, «ё» и раскладки
(ЙЦУКЕН ↔ QWERTY), Enter выполняет первую доступную. Тема, привязка к
пикселям, настройки, сочетания, «О программе» и «Экспорт…» (к кнопке секции
экспорта) — команды ui.*; признак темы «как в системе» уходит ядру для
отметки. Файл на диске без File System Access — без своих пунктов. E2E: новый
main-menu.spec.ts, прежние тесты меню файла — через главное меню.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 7: Контекстное меню, меню масштаба и тулбар — по состоянию команд ядра

Спека §3: основной поток ничего не вычисляет. Здесь — `canvasMenu()` без локальной доступности по числу выделенных, «Показать выделение» в меню масштаба — без локального `disabled`; кнопка и список «Булевы операции» в тулбаре — доступность, отметка и `aria-pressed` из `store.state.commands`, на кнопке — отмеченная операция, иначе последняя выбранная.

**Files:**
- Modify: `apps/web/src/ui/{chrome.tsx,app.tsx,toolbar.tsx}`
- Test: `apps/web/e2e/chrome.spec.ts`

**Interfaces:**
- Consumes: `UiState.commands` (задача 5), `MenuItem` без `disabled` — из состояния.
- Produces: `canvasMenu()` без аргумента.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  await page.keyboard.press('Tab');
  await expect(menu).toHaveCount(0);
});

````

на:

````ts
  await page.keyboard.press('Tab');
  await expect(menu).toHaveCount(0);
});

test('контекстное меню и тулбар: доступность и отметки — из состояния команд ядра', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y);
  await liveWhen(page, 'l.selectionCount === 1');
  // Один прямоугольник: разгруппировать нечего, булевой операции нужен второй слой.
  const toolbar = page.getByRole('toolbar', { name: 'Инструменты' });
  await expect(toolbar.getByRole('button', { name: 'Объединение' })).toHaveAttribute('aria-disabled', 'true');
  await expect(toolbar.getByRole('button', { name: 'Объединение' })).toHaveAttribute('aria-pressed', 'false');
  await toolbar.getByRole('button', { name: 'Булевы операции' }).click();
  const ops = page.getByRole('menu', { name: 'Булевы операции' });
  await expect(ops.getByRole('menuitemcheckbox', { name: /Вычитание/ })).toHaveAttribute('aria-disabled', 'true');
  await expect(ops.getByRole('menuitem', { name: /Сплющить/ })).not.toHaveAttribute('aria-disabled', 'true');
  await page.keyboard.press('Escape');
  await page.mouse.click(node.x, node.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu.getByRole('menuitem', { name: /^Разгруппировать/ })).toHaveAttribute('aria-disabled', 'true');
  await expect(menu.getByRole('menuitem', { name: /^Сгруппировать/ })).not.toHaveAttribute('aria-disabled', 'true');
  // Сгруппировать из меню — группа выделена: теперь разгруппировать можно.
  await menu.getByRole('menuitem', { name: /^Сгруппировать/ }).click();
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await page.mouse.click(node.x, node.y, { button: 'right' });
  await expect(menu.getByRole('menuitem', { name: /^Разгруппировать/ })).not.toHaveAttribute('aria-disabled', 'true');
  await page.keyboard.press('Escape');
});

````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/chrome.spec.ts --project=chromium`
Expected: FAIL — E2E (стенд собран) — 26 passed, 1 failed: «контекстное меню и тулбар: доступность и отметки…» — у кнопки «Объединение» тулбара `aria-disabled` `Expected: "true"`, `Received: "false"` (доступность считается по сводке панели, а не по состоянию команд)

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/app.tsx` заменить:

````tsx
                <Menu
                  label="Действия с выделением"
                  items={canvasMenu(store.state.selectionCount)}
                  anchor={at()}
                  onClose={() => setContext(null)}
````

на:

````tsx
                <Menu
                  label="Действия с выделением"
                  items={canvasMenu()}
                  anchor={at()}
                  onClose={() => setContext(null)}
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
    { separator: true },
    { label: 'Показать всё', command: 'view.fit_all' },
    { label: 'Показать выделение', command: 'view.fit_selection', disabled: ed.store.state.selectionCount === 0 },
    { separator: true },
    ...ZOOMS.map((z) => ({
````

на:

````tsx
    { separator: true },
    { label: 'Показать всё', command: 'view.fit_all' },
    { label: 'Показать выделение', command: 'view.fit_selection' },
    { separator: true },
    ...ZOOMS.map((z) => ({
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
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
````

на:

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
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
    item('Копировать как PNG', 'edit.copy_png'),
    item('Копировать как SVG', 'edit.copy_svg'),
    item('Выделить всё', 'edit.select_all', false),
    item('Показать всё', 'view.fit_all', false),
  ];
}
````

на:

````tsx
    item('Копировать как PNG', 'edit.copy_png'),
    item('Копировать как SVG', 'edit.copy_svg'),
    item('Выделить всё', 'edit.select_all'),
    item('Показать всё', 'view.fit_all'),
  ];
}
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
  type MenuKind = 'move' | 'frame' | 'shapes' | 'boolean';
  const [menu, setMenu] = createSignal<{ kind: MenuKind; anchor: HTMLElement } | null>(null);
  // Операция на кнопке: у выделенной булевой группы — её, иначе последняя выбранная.
  const [lastOp, setLastOp] = createSignal<BooleanOpDto>('union');
  const boolean = createMemo(() => {
    const op = ed.store.state.summary?.booleanOp ?? lastOp();
    return BOOLEANS.find((b) => b.op === op)!;
  });
  const hasSelection = () => (ed.store.state.summary?.count ?? 0) > 0;
  const runBoolean = (b: (typeof BOOLEANS)[number]) => {
    setLastOp(b.op);
````

на:

````tsx
  type MenuKind = 'move' | 'frame' | 'shapes' | 'boolean';
  const [menu, setMenu] = createSignal<{ kind: MenuKind; anchor: HTMLElement } | null>(null);
  // Состояние команд — из ядра: доступна ли операция и какая отмечена у
  // выделенных булевых групп (основной поток ничего не вычисляет).
  const command = (key: string) => ed.store.state.commands[key];
  const enabled = (key: string) => command(key)?.enabled ?? false;
  // Операция на кнопке: отмеченная у выделенных групп, иначе последняя выбранная.
  const [lastOp, setLastOp] = createSignal<BooleanOpDto>('union');
  const selectedOp = () => BOOLEANS.find((b) => command(b.command)?.checked);
  const boolean = createMemo(() => selectedOp() ?? BOOLEANS.find((b) => b.op === lastOp())!);
  const runBoolean = (b: (typeof BOOLEANS)[number]) => {
    setLastOp(b.op);
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
    }));

  const booleanItems = (): MenuItem[] => [
    ...BOOLEANS.map((b) => ({
      label: b.name,
      command: b.command,
      checked: b.op === boolean().op,
      disabled: !hasSelection(),
      onSelect: () => runBoolean(b),
    })),
    { separator: true as const },
    { label: 'Сплющить', command: 'object.flatten', disabled: !hasSelection() },
    { label: 'Контур обводки', command: 'object.outline_stroke', disabled: !hasSelection() },
  ];

````

на:

````tsx
    }));

  // Доступность и отметка пунктов — из состояния команд (переключатели ядра).
  const booleanItems = (): MenuItem[] => [
    ...BOOLEANS.map((b) => ({
      label: b.name,
      command: b.command,
      onSelect: () => runBoolean(b),
    })),
    { separator: true as const },
    { label: 'Сплющить', command: 'object.flatten' },
    { label: 'Контур обводки', command: 'object.outline_stroke' },
  ];

````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
        <button
          class="fe-tool"
          aria-disabled={!hasSelection()}
          aria-pressed={ed.store.state.summary?.booleanOp != null}
          aria-label={boolean().name}
          data-tip={boolean().name}
          data-command={boolean().command}
          data-tip-side="top"
          onClick={() => hasSelection() && runBoolean(boolean())}
        >
          <Icon name={boolean().op} />
````

на:

````tsx
        <button
          class="fe-tool"
          aria-disabled={!enabled(boolean().command)}
          aria-pressed={selectedOp() !== undefined}
          aria-label={boolean().name}
          data-tip={boolean().name}
          data-command={boolean().command}
          data-tip-side="top"
          onClick={() => enabled(boolean().command) && runBoolean(boolean())}
        >
          <Icon name={boolean().op} />
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/chrome.spec.ts e2e/booleans.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/main-menu.spec.ts e2e/chrome.spec.ts --project=firefox`

Expected: модульные — 148 passed, сборка — ок; E2E `chrome` и `booleans` в Chromium — без падений; `main-menu` и `chrome` в Firefox — 36 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/chrome.spec.ts apps/web/src/ui/app.tsx apps/web/src/ui/chrome.tsx apps/web/src/ui/toolbar.tsx
git commit -m "интерфейс: контекстное меню, зум и тулбар — по состоянию команд ядра

Контекстное меню холста и пункт «Показать выделение» в меню масштаба больше
не вычисляют доступность по числу выделенных: серые пункты — из состояния
команд ядра (например, «Разгруппировать» серый у одного прямоугольника).
Кнопка и список «Булевы операции» в тулбаре берут доступность и отмеченную
операцию из состояния команд (операция выделенных групп), а не из сводки
панели; на кнопке — она, иначе последняя выбранная.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 8: Спека, бэклог и финальная проверка

Спека §12: тесты и замеры. Здесь — «Сделано (этап 1)» в разделах 3, 4, 11, 12 спеки оболочки (числа замеров), исправленный пример раскладки в §7 («щквук» — «order»); в `docs/backlog.md` — раздел «Оболочка (этап 1 подсистемы 2)» (B-94…B-97) и обновлённая B-72. Шаг «Проверить» — все ворота этапа.

**Files:**
- Modify: `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md`, `docs/backlog.md`

- [ ] **Step 1: Реализация**

В `docs/backlog.md` заменить:

````markdown
| B-93 | Wasm ядра вырос на 2,3 МБ (0,56 МБ сжатого): `krilla` и её зависимости, в том числе неиспользуемые декодеры картинок `raster-images`; вторая версия `skrifa` (0.42 рядом с 0.44). | P3 | этап 5 |

## Код

````

на:

````markdown
| B-93 | Wasm ядра вырос на 2,3 МБ (0,56 МБ сжатого): `krilla` и её зависимости, в том числе неиспользуемые декодеры картинок `raster-images`; вторая версия `skrifa` (0.42 рядом с 0.44). | P3 | этап 5 |

## Оболочка (этап 1 подсистемы 2)

| № | Что | Пр. | Источник |
|---|---|---|---|
| B-94 | Пиксельная сетка (`view.pixel_grid`, Shift+') не сохраняется в настройках пользователя: после перезагрузки снова включена (переживает только смену документа в том же ядре). | P3 | этап 1, решения плана |
| B-95 | Пересчёт состояний команд при выделенных всех 100 000 узлах — 4,6–5,5 мс нативно (один проход по выделению: группы, текст, булевы); цель 1 мс выполнена для шага при обычном выделении. | P3 | этап 1, `editor_report` |
| B-96 | «Экспорт…» (Ctrl/Cmd+Shift+E) переводит фокус на кнопку секции «Экспорт» панели свойств; окна экспорта, как в Figma, нет. «О программе» — уведомление без номера версии. | P3 | этап 1, решения плана |
| B-97 | Подменю без «безопасного треугольника»: косое движение к подменю через соседний пункт закрывает его через 150 мс. Главное меню по Alt+/ при скрытом интерфейсе (Ctrl/Cmd+\) встаёт в левый верхний угол окна. | P3 | этап 1 |

## Код

````

В `docs/backlog.md` заменить:

````markdown
| B-70 | «Булева группа рисуется листом» — четыре разрозненные проверки `kind == Boolean` (draw, scene, overlay, display). Нужна одна функция. Для «контейнер» тоже нет общей функции: в `clipboard.rs` и `page.rs` перечисляется `Frame \| Group \| Boolean`. | P3 | этап 4, ревью задач 1 и 3 |
| B-71 | Глубокая копия содержимого текста в `PropsPatch::apply_to` при смене только свойств. `TextContent` нужно держать за `Arc`. | P3 | этап 2 подсистемы 3 |
| B-72 | Захардкоженное число команд раскладки в тестах (`commands.len() == 53`). | P3 | этап 4, ревью задачи 6 |
````

на:

````markdown
| B-70 | «Булева группа рисуется листом» — четыре разрозненные проверки `kind == Boolean` (draw, scene, overlay, display). Нужна одна функция. Для «контейнер» тоже нет общей функции: в `clipboard.rs` и `page.rs` перечисляется `Frame \| Group \| Boolean`. | P3 | этап 4, ревью задач 1 и 3 |
| B-71 | Глубокая копия содержимого текста в `PropsPatch::apply_to` при смене только свойств. `TextContent` нужно держать за `Arc`. | P3 | этап 2 подсистемы 3 |
| B-72 | Захардкоженное число команд раскладки в тестах (`commands.len() == 80` в `ferrum-wasm/tests/boolean.rs`, `COMMANDS.len() == 80` в `ferrum-editor/tests/keymap.rs`; этап 1 подсистемы 2 их только обновил). | P3 | этап 4, ревью задачи 6 |
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
  Состояние приходит событием `CommandStates { changed: [{ key, enabled, checked? }] }` — только изменившиеся команды и только после шага, который мог их изменить (выделение, история, инструмент, вид, режим правки). Основной поток ничего не вычисляет, у него только таблица по ключу. Команда, которая недоступна, при вызове ничего не делает и не создаёт шага; так ядро и сейчас отвечает на сочетания клавиш.
- **Меню в ядре.** Главное меню и контекстные меню описаны данными в ядре: дерево `{ key } | { submenu, title, items } | separator`. Порядок и состав повторяют Figma UI3. Интерфейс получает дерево главного меню один раз (`MainMenu`), а контекстное — по запросу (§5). Интерфейс рисует меню и отправляет `Execute { key }`.

## 4. Главное меню (этап 1)
````

на:

````markdown
  Состояние приходит событием `CommandStates { changed: [{ key, enabled, checked? }] }` — только изменившиеся команды и только после шага, который мог их изменить (выделение, история, инструмент, вид, режим правки). Основной поток ничего не вычисляет, у него только таблица по ключу. Команда, которая недоступна, при вызове ничего не делает и не создаёт шага; так ядро и сейчас отвечает на сочетания клавиш.
- **Меню в ядре.** Главное меню и контекстные меню описаны данными в ядре: дерево `{ key } | { submenu, title, items } | separator`. Порядок и состав повторяют Figma UI3. Интерфейс получает дерево главного меню один раз (`MainMenu`), а контекстное — по запросу (§5). Интерфейс рисует меню и отправляет `Execute { key }`.
- **Сделано (этап 1):**
  - Таблица — 80 команд. Группа — перечисление `CommandGroup` (ключ и русское название: «Файл», «Правка», «Вид», «Объект», «Текст», «Расставить», «Вектор», «Настройки», «Справка» и для таблицы сочетаний — «Инструменты», «Сдвиг»); синонимы — строчные, хотя бы один русский и один английский; переключатели — четыре булевы операции, «Скрыть интерфейс», «Пиксельная сетка», «Привязка к пикселям» и три темы. Конструктор строки требует группу и синонимы; тест `command_table` проверяет заполненность, уникальность ключей и что ключ начинается с группы.
  - Команды `ui.*` (15): главное меню (Alt+/), новый файл, открыть, открыть с диска, сохранить на диск, копия на диск, экспорт (Ctrl/Cmd+Shift+E, как в Figma), все файлы, настройки, сочетания клавиш, о программе, привязка к пикселям (Ctrl/Cmd+Shift+', как «Snap to pixel grid»), три темы. «Сохранить» осталась `file.save` с прежним событием `SaveRequested`: её ключ — в сохранённых раскладках пользователей. Пункт меню команды `ui.*` основной поток выполняет сразу (выбор файла требует жеста пользователя); сочетание на холсте идёт через ядро и возвращается событием `UiCommand`. Правку вектора команды `ui.*` не заканчивают.
  - Для меню «Вид», «Текст» и «Вектор» добавлены команды ядра: «Пиксельная сетка» (Shift+', как в Figma), «Войти в правку» (`vector.edit`, Enter — физическая клавиша по-прежнему; удержанная клавиша в правку не входит), жирный, курсив, подчёркнутый, зачёркнутый (Ctrl/Cmd+B, I, U, Shift+X) и выравнивание (Ctrl/Cmd+Alt+L, T, R, J). Форматирование в правке текста идёт по действующей раскладке (раньше сочетания были зашиты), вне правки — ко всему тексту выделенных текстов одним шагом: жирный и курсив сохраняют семейство каждого участка.
  - Состояние считает `Editor::command_states` за один проход по выделению; пересчёт — по ключу (версия выделения, счётчик и эпоха сцены, история, «только чтение», правка текста и вектора, настройки) и не во время жеста. `CommandStates` в одной пачке событий ядра — одно событие (у команды — последнее состояние); первым — все команды (новое ядро, новый документ). Недоступная команда не выполняется в `Editor::run` — и с клавиатуры, и из меню: ни шага, ни сообщения.
  - **(поведение)** Как в Figma: булевы операции над одним слоем недоступны (раньше — сообщение «выделите хотя бы два слоя»); «Разгруппировать» доступна, только если выделен контейнер с детьми; в документе только для чтения недоступны правки (удалить, вставить, вырезать, дублировать, сгруппировать, сдвиг…), копирование и вид — доступны; инструменты доступны всегда (рисование в таком документе по-прежнему отвечает ошибкой). «Вставить» доступна всегда: что лежит в системном буфере, ядро не знает.
  - Дерево — `ferrum_editor::main_menu()`; событие `MainMenu` уходит один раз при запуске ядра.

## 4. Главное меню (этап 1)
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
  Подменю держится на экране: у края окна оно отражается на другую сторону.
- **Поиск в меню.** Поле поиска вверху главного меню — тот же поиск, что в «Быстрых действиях» (§7), но только по командам.

## 5. Контекстные меню (этап 2)
````

на:

````markdown
  Подменю держится на экране: у края окна оно отражается на другую сторону.
- **Поиск в меню.** Поле поиска вверху главного меню — тот же поиск, что в «Быстрых действиях» (§7), но только по командам.
- **Сделано (этап 1):**
  - **(поведение)** Кнопка с логотипом (оранжевый знак и стрелка) — слева в шапке левой панели; «Назад к файлам» переехала направо, на место прежней стрелки меню файла. Повторный щелчок по логотипу закрывает меню.
  - **(поведение)** Состав: «Файл» — новый, открыть, открыть с диска*, | сохранить, сохранить на диск*, копия на диск, | экспорт, | все файлы (* — только с File System Access или в десктопе, иначе пунктов нет); «Правка» — отменить, повторить, | копировать, вырезать, вставить, «Копировать как ▸» (PNG, SVG), | дублировать, удалить, | выделить всё; «Вид» — пиксельная сетка, | скрыть интерфейс, | приблизить, отдалить, 100 %, показать всё, показать выделение; «Объект» — сгруппировать, разгруппировать, обернуть во фрейм, | «Булевы операции ▸», сплющить, контур обводки, | отразить по горизонтали и вертикали, | скрыть или показать, заблокировать или разблокировать; «Текст» — жирный, курсив, подчёркнутый, зачёркнутый, | «Выравнивание ▸»; «Расставить» — на передний план, вперёд, назад, на задний план; «Вектор» — войти в правку, перо; «Настройки» — «Тема ▸» (светлая, тёмная, как в системе), привязка к пикселям, | сочетания клавиш, настройки…; «Справка» — сочетания клавиш, о программе. Вставка поверх и с заменой, свойства, «Выделить подобные», линейки — с их этапами.
  - **(поведение)** Открытие — фокус в поле поиска; стрелка вниз — к первому пункту, вверх с первого — обратно в поле. Подменю открывается наведением через 150 мс (наведение на пункт без подменю закрывает открытое через те же 150 мс), щелчком, Enter и стрелкой вправо; стрелка влево и Esc в подменю — к пункту-родителю, Esc в корне закрывает меню и возвращает фокус. Переход по первой букве — по кругу, без учёта регистра и «ё». Серые пункты фокусируются, но не выполняются и меню не закрывают. У подменю — стрелка «▸», у отмеченного — галочка, роль `menuitemcheckbox` у переключателей.
  - Подменю встаёт справа от пункта верхом вровень с ним; не влезает справа — слева от пункта, снизу — поднимается к краю окна (отступ 8 px). Положение считается в фазе `measure`; невидимое меню до этого стоит в углу окна, чтобы размер не зависел от места в потоке.
  - **(поведение)** Поиск этапа 1 — подстрока названия или синонима, без учёта регистра и «ё», в обеих раскладках (запрос переводится ЙЦУКЕН ↔ QWERTY); сначала команды, чьё название начинается с запроса, затем остальные в порядке меню; не больше 50; «Ничего не найдено» — строкой; Enter выполняет первую доступную. Нечёткий поиск с весами — этап 3.
  - **(поведение)** «Экспорт…» переводит фокус на кнопку секции «Экспорт» панели свойств (окна экспорта нет — B-96); «О программе» — уведомление; «Настройки…» и «Сочетания клавиш» открывают диалог настроек на вкладке «Горячие клавиши» (панель сочетаний — этап 3).

## 5. Контекстные меню (этап 2)
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
- **Поиск.**
  - Нечёткий, по подпоследовательности с весом: начало слова, совпадение с синонимом, недавность.
  - Раскладка не важна: «ыщквук» находит «order», запрос переводится из одной раскладки в другую.
  - Регистр и буква «ё» не важны.
  - Команды ищутся в основном потоке (их ≈ 70). Слои ищет worker: `FindLayers { query, limit: 50 }` → `LayersFound`, на 100 000 узлах ≤ 30 мс. Каждый символ ввода сбрасывает предыдущий запрос.
````

на:

````markdown
- **Поиск.**
  - Нечёткий, по подпоследовательности с весом: начало слова, совпадение с синонимом, недавность.
  - Раскладка не важна: «щквук» находит «order», запрос переводится из одной раскладки в другую. (Прежний пример «ыщквук» — это «sorder»: лишняя первая буква.)
  - Регистр и буква «ё» не важны.
  - Команды ищутся в основном потоке (их ≈ 70). Слои ищет worker: `FindLayers { query, limit: 50 }` → `LayersFound`, на 100 000 узлах ≤ 30 мс. Каждый символ ввода сбрасывает предыдущий запрос.
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
  - `offset` — конечное число, по модулю не больше 1e9;
  - ключи меню — только из таблицы.

## 12. Тестирование
````

на:

````markdown
  - `offset` — конечное число, по модулю не больше 1e9;
  - ключи меню — только из таблицы.
- **Сделано (этап 1):**
  - `MainMenu { tree }`, где `tree` — `MenuItemDto[]`: `{ type: "command", key } | { type: "submenu", title, items } | { type: "separator" }` (тег `type`, как у остальных событий).
  - `CommandStates { changed: [{ key, enabled, checked? }] }`; `UiCommand { key }`.
  - `SetTheme` получил необязательный `system` — тема выбрана «как в системе» (для отметки «Тема как в системе»).
  - `KeymapEntry` получил `keywords` и `toggle`, а `group` теперь — группа из таблицы, а не префикс ключа.

## 12. Тестирование
````

В `docs/superpowers/specs/2026-11-02-ferrum-shell-design.md` заменить:

````markdown
  - «Выделить подобные» — ≤ 100 мс;
  - кадр с линейками — не больше кадра без них плюс 0,5 мс.
- **E2E в Chromium и Firefox:**
  - главное меню с подменю, клавиатурой и состоянием пунктов;
````

на:

````markdown
  - «Выделить подобные» — ≤ 100 мс;
  - кадр с линейками — не больше кадра без них плюс 0,5 мс.
- **Сделано (этап 1):** модульные тесты ядра — `command_table` (заполненность таблицы, переключатели, команды `ui.*` только сообщают о себе), `command_state` (без выделения, одна фигура, группы и булевы, история, правка текста и вектора, «только чтение», недоступная команда без событий, отметки вида и настроек, события только с изменениями и не во время жеста), `main_menu` (порядок Figma UI3, состав подменю, без пустых подменю и лишних разделителей, все команды групп меню в дереве), `menu_commands` (сетка, «Войти в правку», форматирование в правке и вне её по раскладке); ядро worker'а — состояния целиком при запуске и после открытия, изменения одним событием, тема «как в системе», недоступная команда без ошибки, `MainMenu` один раз. Замер `editor_report`: «Состояния команд после шага, 100 000 узлов, выделен 1» — 0,001 мс (цель ≤ 1 мс; нативно, release), справочно при 10 000 выделенных — 0,25 мс, при всех 100 000 — 4,6–5,5 мс (B-95). E2E: `main-menu.spec.ts` (Chromium и Firefox) — подменю клавиатурой и наведением, Alt+/, серые пункты и галочки из ядра, тема, поиск в другой раскладке, первая буква, подменю у края окна, команды интерфейса, `__ferrumWarnings` пуст; в `chrome.spec.ts` — серые пункты стрелками и первая буква, контекстное меню и тулбар по состоянию ядра. Прежние тесты меню файла открывают пункты через главное меню (`mainMenu` в `helpers.ts`).
- **E2E в Chromium и Firefox:**
  - главное меню с подменю, клавиатурой и состоянием пунктов;
````

- [ ] **Step 2: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-render --example render_report | grep -E "FAIL" || echo "FAIL нет"`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Состояния команд"`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && ./scripts/compat-old-build.sh main vector_fixture`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/main-menu.spec.ts e2e/chrome.spec.ts --project=firefox`

Expected: всё PASS: fmt, clippy (нативно и wasm32) без замечаний; `cargo test --workspace` — 1004 passed, 2 ignored; `render_report` — FAIL нет (47 PASS); `editor_report` — «Состояния команд после шага, 100 000 узлов, выделен 1» ≈ 0,001 мс (≤ 1 мс), FAIL нет (89 PASS); совместимость — «файл открыт только для чтения»; модульные — 148 passed; `e2e:ci` — ≈ 342 теста, без падений (известная нестабильная `text-edit.spec.ts` «фрагмент со стилями…» может пройти со второго раза); Firefox `main-menu` и `chrome` — 36 passed

- [ ] **Step 3: Закоммитить**

```bash
git add docs/backlog.md docs/superpowers/specs/2026-11-02-ferrum-shell-design.md
git commit -m "docs: этап 1 подсистемы 2 — «Сделано» в спеке оболочки, бэклог

В спеке подсистемы 2 записано сделанное на этапе 1: таблица команд с
группами, синонимами и переключателями, команды ui.*, состояние команд и его
правила, главное меню (состав, клавиатура, подменю у края, поиск),
дополнения протокола и тесты с замером состояний на 100 000 узлах. Пример
раскладки в §7 исправлен («щквук» — «order»). В бэклоге — раздел этапа
(B-94…B-97), B-72 обновлена.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Ручная проверка**

1. Chrome: `#/new`; логотип слева в шапке — меню открыто, фокус в поиске; «Назад к файлам» — справа; повторный щелчок по логотипу закрывает меню.
2. Подменю наведением (задержка), →/←, Esc; серые пункты (например, «Разгруппировать» у одного прямоугольника) фокусируются, но не выполняются; галочки «Пиксельная сетка», «Привязка к пикселям», «Тема».
3. Поиск: «дубл», «duplicate», «щквук» (находит «order»); Enter выполняет первую доступную.
4. Alt+/ на холсте; Ctrl/Cmd+Shift+E — фокус на кнопке «Экспорт»; «Настройки…» и «Сочетания клавиш» — диалог на вкладке «Горячие клавиши».
5. Документ только для чтения: правки серые, Delete ничего не пишет.
6. Firefox: логотип, подменю, поиск.

---

## Что этап 1 сознательно не делает

- **Пересчёт при выделенных всех 100 000 узлах** — ≈ 5 мс нативно (один проход по выделению без раннего выхода); цель 1 мс — для шага при обычном выделении (B-95).
- **Пиксельная сетка не сохраняется** в настройках пользователя — переживает только смену документа в том же ядре (B-94).
- **Окна экспорта нет:** «Экспорт…» переводит фокус на кнопку секции «Экспорт»; «О программе» — уведомление без номера версии (B-96).
- **«Безопасного треугольника» у подменю нет:** косое движение через соседний пункт закрывает подменю через 150 мс; главное меню по Alt+/ при скрытом интерфейсе встаёт в левый верхний угол окна (B-97).
- **Пункты следующих этапов** — вставить поверх и с заменой, копирование свойств, «Выделить подобные», линейки, контекстные меню слоёв и страниц, палитра и панель сочетаний — этапы 2–4.
- **Числа команд в тестах** по-прежнему зашиты (B-72): этап их только обновил.
- **Не запускались:** проект Playwright `gpu` и замеры `@perf`, Safari/WebKit, десктоп (крейт не тронут), замеры в wasm (только нативный `editor_report`); Firefox — только `main-menu.spec.ts` и `chrome.spec.ts`.
