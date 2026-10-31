# Ferrum, подсистема 3, этап 4: булевы операции — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Булевы операции как в Figma: узел `Boolean` (операция `union` | `subtract` | `intersect` | `exclude`) с детьми — как группа, результат которого считает сцена и рисует заливками и обводками самого узла; четыре операции на выделение (Alt+Shift+U/S/I/E, как в справке Figma) и смена операции выделенных групп одним шагом, кнопка-список «Булевы операции» в тулбаре, иконки операций в слоях; дети видны в слоях и правятся как обычно, попадание и подсветка — по результату; «Сплющить» булевой группы и текста (контуры глифов) и «Контур обводки» (Ctrl/Cmd+Alt+O); операция над 1000 перекрывающихся контуров ≤ 50 мс, перетаскивание ребёнка большой группы — в бюджете кадра.

**Architecture:** Модель (`ferrum-doc`) хранит операцию строковым ключом `booleanOp` узла `boolean` (без ключа — объединение); результат в документе не хранится. Кэш сцены — версия 5 (код узла 8 и байт операции), буфер обмена — поле `booleanOp`. Геометрия (`ferrum-scene::boolean`) дробит кривые детей (`flatten_path`, точность `BOOLEAN_TOLERANCE = 0,005` в координатах группы), приводит каждую форму к простым контурам её правилом (`normalize`) и складывает формы библиотекой `i_overlay` (`combine`); результат (`BooleanOutline`) лежит в узле сцены и пересчитывается по ключу содержимого детей, во время жеста над одним ребёнком — только части у этого ребёнка (`combine_rest`, `combine_with`). Текст в группе даёт контуры глифов через источник, который ставит сцене редактор (`Scene::set_text_outlines`, контуры — `ferrum_render::text_outline` из реестра шрифтов). Рендер рисует результат как путь узла (дети не рисуются), подсветка — контуром результата. Редактор (`ferrum-editor`) получает команды операций (`boolean.rs`), «Сплющить» булевой группы и текста (`flatten.rs`), «Контур обводки» (`outline.rs`); ядро worker'а (`ferrum-wasm`) — операцию в строке слоя и в сводке выделения; интерфейс — кнопку-список тулбара и иконки слоёв. Основной поток ничего не считает: команды — обычный `Execute`, геометрия в протокол не идёт.

**Tech Stack:** Rust 1.96 (новая зависимость — `i_overlay = "=9.1.0"` без фич по умолчанию, с ней в `Cargo.lock` — `i_float 5.0.0`, `i_shape 5.0.0`, `i_tree 0.20.0`, `i_key_sort 0.12.0`; `kurbo 0.13.1` — `flatten`, `stroke`; `loro 1.16.2`; `vello_cpu 0.3.0`), SolidJS 1.9.17, TypeScript 6.0.3, @playwright/test 1.64.0 (`chromium`, `firefox`).

**Спеки:** `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` — раздел 10 и части 8 и 12, нужные этапу 4 (Scale и экспорт — раздел 11 — не входят); спека ядра `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` («ядро §N»).

## Global Constraints

- Ветка `feat/sub3-boolean` от `main` (0797cf7, этапы 1–3 подсистемы 3 слиты). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Каталог сборки — у каждой копии свой: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0` (в `apps/web` — `$PWD/../../target`); команды — из корня копии, если не сказано иное. `scripts/compat-old-build.sh` собирает старую сборку в свой каталог `$CARGO_TARGET_DIR/compat-old` (задача 1): в общем каталоге старый `ferrum-doc` перезаписывал отладочную библиотеку новой сборки.
- Одна новая внешняя зависимость — `i_overlay =9.1.0` (точная версия, `default-features = false`: без `allow_multithreading` — rayon в wasm нет); `package-lock.json` не меняется.
- Основной поток тонкий (ядро §4.2): дробление, операции, попадание, контуры глифов — в worker'е ядра. В основном потоке — кнопка-список тулбара и иконки слоёв; чтения layout — только в фазе `measure` планировщика (E2E проверяют `__ferrumWarnings`).
- Недоверенный ввод (спека 8, ядро §4.3): операция из файла — только известная строка (`DocError::InvalidBooleanOp`: узел с поддеревом скрыт, документ только для чтения, как неизвестный тип), из буфера обмена — отказ вставки, из кэша сцены — кэш отбрасывается.
- Одна правка — один шаг отмены: новая булева группа, смена операции, «Сплющить», «Контур обводки», жест над ребёнком (превью — в сцене, запись при отпускании). Нечего делать — сообщение без шага.
- Поза: булева группа — «как группа» (`w = h = 0`, поза — `x, y, rotation, flip`); ресайз, панель, отражение, перетаскивание в слоях, разгруппировка — через `NodeKind::is_group_like()`; отражение ребёнка разворачивает обход его контуров.
- Тесты не проходят вхолостую: модель — перезагрузка из Loro и кэша, буфер обмена; геометрия — площади, число контуров, рамки, попадание, побайтный снимок сцены; рендер — пиксели каждой операции, затем эталоны; редактор — документ, шаги отмены, сообщения, клавиши; ядро — строки слоёв и сводка; E2E — строки слоёв, шаги и пиксели экрана.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Сначала прототип всего этапа собран в отдельной копии (`plan-verify-wt4`, ветка `verify/sub3-boolean` от 0797cf7, по коммиту на задачу; вершина — e2c9d2c) и прошёл ворота: `cargo fmt --check`, `clippy -D warnings` (нативно и `wasm32-unknown-unknown`), `cargo test --workspace` (877 тестов), `render_report` и `editor_report` (release, 45 и 82 строки PASS, FAIL нет), wasm-сборка, `npm run typecheck`, модульные (134), сборка, E2E `chromium` целиком (`--grep-invert @perf` — 326 passed), `booleans`, `vectors`, `text`, `text-edit`, `roundtrip` в Firefox (8 passed), совместимость: `scripts/compat-old-build.sh 0797cf7 boolean_fixture` и `scripts/compat-old-build.sh main vector_fixture`. Библиотеку выбрал отдельный прототип с замерами `linesweeper 0.5.0` и `i_overlay 9.1.0` (решение 14).

Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом, который читает сам этот документ, к чистой копии от 0797cf7 (`plan-apply-wt4`, свой каталог сборки) в порядке задач: тесты шага 1 — и проверка, что команда шага 2 действительно падает (как написано в «Expected»), затем реализация — каждый фрагмент «заменить» найден ровно один раз (операций по задачам: 55, 22, 6, 21, 28, 14, 11, 8, 3), эталоны PNG — командой шага генерации, после каждой задачи — все команды её шага «Проверить». После последней задачи дерево совпало с прототипом байт в байт (тот же хэш дерева у каждого из девяти коммитов, эталоны `boolean_ops.png` и `boolean_nested.png` — побайтно). Числа прогона на чистой копии: `cargo test --workspace` — 877 passed (≈ 2 мин); `render_report` — объединение 1000 кругов 18,3 мс, вычитание 17,9 мс, пересечение непересекающихся 0,02 мс, исключение 21,3 мс, пересечение 1000 почти совпадающих 19,2 мс, сцена с результатом 22,4 мс, кадр 0,54 мс, правка ребёнка без жеста 19,6 мс (цели ≤ 50 / ≤ 16,7 мс); `editor_report` — кадр жеста над ребёнком 2,4 мс (медиана), запись (отпускание) 5,8 мс, один шаг; FAIL нет ни в одном отчёте; E2E задачи 7 — 69 passed, Firefox — 2; `e2e:ci` задачи 9 — 325 passed и 1 flaky за 6,8 мин; Firefox задачи 9 — 8 passed (13,3 с); совместимость — оба примера только для чтения.

Порт 4173 на машине проверки занят посторонним процессом, поэтому E2E шли через временный конфиг Playwright с портами 4283/4284 (и временной правкой портов в `core.spec.ts` и `settings.spec.ts`, откатываемой после каждой команды), с `--retries=1`. Нестабильным в полном прогоне был `layers.spec.ts` «виртуальный список: в DOM только окно строк» (`toHaveAttribute` с первого раза не дождался, со второго прошёл; отдельно — passed за 1,4 с) — вне этапа 4. Известные нестабильные прошлых этапов — `text-edit.spec.ts` «фрагмент со стилями…», `files.spec.ts` «1000 записей…», `picker.spec.ts` «обводка: ширина жестом…» — в этом прогоне прошли с первого раза.

Исправлено по ходу проверки: (1) `scripts/compat-old-build.sh` собирал старую сборку в общий `CARGO_TARGET_DIR`, и старый `ferrum-doc` перезаписывал отладочную библиотеку новой сборки (у членов workspace хэш артефакта не зависит от пути) — следующая сборка падала на «нет `BooleanOp`»; теперь старая сборка — в `$CARGO_TARGET_DIR/compat-old` (задача 1), после правки обе проверки совместимости и обычная сборка проходят, библиотеки нового `target` не тронуты. (2) Команда шага 2 задачи 7 в первом прогоне не собирала стенд, и E2E падали вхолостую (приложение не открывалось, `waitForFunction` 90 с); теперь шаг 2 — `npm run wasm && npm run build && …`, и он проверен отдельно на копии в состоянии после задачи 6 тем же скриптом (шаг 1 из этого документа): 1 failed (нет кнопки «Объединение» в тулбаре), 3 passed. Реализация шагов от этого не менялась.

Найденное прототипом и вошедшее в шаги: клавиши Alt+Shift+U/S/I/E по справке Figma вместо Ctrl/Cmd+Alt+U/S/I/X из спеки (спека исправлена, задача 9); запись жеста над ребёнком большой группы в первой редакции стоила два полных пересчёта (≈ 37 мс) — теперь оба шага по частям (≈ 6 мс, задача 8); занятый реестр шрифтов молча оставлял текст булевой группы без формы — теперь флаг и пересчёт на следующем `emit` (задача 5); AltGr глушит все сочетания с Alt (задача 4). Локальная ветка `main` машины проверки — старая (cd26851, до подсистемы 3), поэтому содержательная проверка совместимости — против 0797cf7. Не запускались: проект Playwright `gpu` (пиксели WebGPU/WebGL2 против CPU с булевыми группами) и замеры `@perf`, Safari/WebKit, приложение Tauri, замеры в wasm (только нативные отчёты), ручная проверка (задача 9); эталоны PNG созданы и сверены на одной машине (macOS, arm64). Тексты ожидаемых результатов — с этого прогона.

---

## Этапы подсистемы 3

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Текст: модель и рендер (слит) | узел `Text`, шрифты, раскладка, отрисовка, отсутствующие шрифты | 3, 4, 5, 6 |
| 2. Инструмент «Текст» и правка (слит) | T, правка на холсте, ввод и IME, курсор, буфер обмена, секция «Текст» | 7, 8 |
| 3. Векторные сети и перо (слит) | узел `Vector`, перо, режим правки, «Сплющить», окончания и соединения, стрелка | 9 |
| **4. Булевы операции** (этот план) | узел `Boolean`, операции и их смена, результат в сцене, «Сплющить» булевой группы и текста, «Контур обводки» | 10 |
| 5 | Scale и экспорт SVG/PDF | 11 |

## Решения, которых в спеке нет (на согласование)

Пользователь просил не останавливаться на вопросах: где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**.

1. Операция — строковый ключ `booleanOp` узла (`union` | `subtract` | `intersect` | `exclude`), без ключа — объединение; в `Props` — `Option<BooleanOp>` только у `Boolean` (как `text` и `vector`). Узел спеки `BooleanOperation` назван `boolean`. Результат в документе не хранится — его считает сцена.
2. Неизвестная операция из файла (будущая сборка, порча) — узел с поддеревом скрыт, документ только для чтения (как неизвестный тип, `DocError::InvalidBooleanOp`); из буфера обмена — отказ вставки «булева операция»; из кэша сцены — кэш отбрасывается. `FORMAT_VERSION` не повышается, кэш сцены — версия 5.
3. Булева группа — «как группа»: своей рамки нет (`w = h = 0`, поза — `x, y, rotation, flip`), дети в её координатах; ресайз масштабирует детей, панель W/H — по границам результата. `NodeKind::is_group_like()` заменил проверки `== Group` в ресайзе, панели, отражении, перетаскивании в слоях.
4. **(поведение)** Новая группа берёт заливки, обводки и стиль обводки верхнего слоя, у «Вычитания» — нижнего (как в справке Figma «Boolean operations»); у группы-источника — её верхнего (нижнего) листа. Группа встаёт на место верхнего слоя.
5. **(поведение)** Имя — по операции: «Объединение», «Вычитание», «Пересечение», «Исключение». Смена операции меняет имя, только если оно было именем по умолчанию какой-то операции.
6. **(поведение)** Нужно хотя бы два слоя (как в Figma): один — «Для булевой операции выделите хотя бы два слоя», без шага. Фреймы в группу не входят: только фреймы — «Булевы операции не применяются к фреймам»; вместе с другими — фреймы остаются, остальные в группу, сообщение «Фреймы в булеву группу не входят».
7. **(поведение)** Выделены только булевы группы — та же команда (клавиша или пункт списка) меняет их операцию одним шагом; та же операция — ничего.
8. **(поведение)** Вклад детей: фигура — контур заливки (прямоугольник со скруглением, эллипс; фрейм и изображение — рамка), вектор — его области, каждая своим правилом, текст — контуры глифов и прямоугольники украшений (правило ненулевое), группа — объединение видимых детей, вложенная булева — её результат. Линии и открытые пути без областей не дают ничего (у них нет заливки, как в Figma); обводки детей не участвуют; скрытые дети не участвуют; заблокированные — участвуют (блокировка — только для холста).
9. Семантика N форм: каждая форма приводится к простым контурам своим правилом, затем объединение — хотя бы одна, вычитание — нижняя непустая без объединения остальных, пересечение — все (свёртка), исключение — нечётное число перекрытий.
10. **(поведение)** Пустой результат (пересечение непересекающихся, все дети скрыты) — ничего не рисуется и не ловит щелчок; рамка группы для выделения — по детям.
11. **(поведение)** Попадание: в группу — по результату; в ребёнка — по его форме, но только внутри результата внешней булевой группы (щелчок в дыру «Вычитания» — мимо). Двойной щелчок — к ребёнку под курсором (как в группе), Ctrl/Cmd+щелчок — самый глубокий.
12. **(поведение)** Дети в слоях видны и правятся как обычно; правка ребёнка (перетаскивание, ресайз, правка вектора) — результат в превью сцены сразу, запись одним шагом. Разгруппировка (Ctrl/Cmd+Shift+G) возвращает детей (снова со своими заливками). Перенос в слоях внутрь булевой группы разрешён.
13. **(поведение)** Подсветка наведения и выделения булевой группы — контуром результата (пустой — рамкой). Дети не рисуются — и в растровом кэше, и в экспорте PNG: `subtree_bounds` группы — только её результат.
14. **(поведение)** Библиотека — `i_overlay 9.1.0` по раздроблённым кривым, а не `linesweeper 0.5.0` (кандидат спеки, операции над кривыми без дробления). Прототип (release, Rust 1.96, одна машина) на 1000 кругах r = 10 с шагом 15 (до восьми соседей): объединение — `linesweeper` 92 мс (75 мс при eps 1e-3…1e-2; одна топология — 71 мс), `i_overlay` — 21 мс при точности 0,005 (19,5 мс при 0,01, 46 мс при 0,001, 5,5 мс при 0,25); шаг 18 — 69 против 20 мс; без пересечений (шаг 25) — 22 против 14 мс; прямоугольник − 999 кругов — 91 против 19 мс; пересечение и исключение у `linesweeper` — 92 и 94 мс; 1000 квадратов встык — 6 против 0,2 мс (обе: один контур, площадь точная). Точность одинаковая: касание встык ∪ / ∩ — один контур / пусто, касание углом — два контура, совпадающие ∪ / ⊕ / − — один / пусто / пусто, общая сторона − — один контур; касающиеся круги (точно 628,319) — 628,365 у `linesweeper` (кривые), 627,628 у `i_overlay` (0,01); площадь объединения 1000 кругов — 226 445,6 у `linesweeper`, 226 425,6 у `i_overlay` при 0,005. Решило время: `linesweeper` не укладывается в 50 мс спеки на перекрывающихся 1000 контуров (69–94 мс), `i_overlay` — с запасом (≈ 17–21 мс в `render_report`). Обе — MIT OR Apache-2.0 (зависимости `i_overlay` — MIT), все версии старше двух недель, `i_overlay` требует Rust 1.88 и собирается под `wasm32` без фич. Цена: результат — ломаные (точность 0,005 в координатах группы, круг r = 100 — ~300 отрезков; четверть пикселя при ×50, при ×256 — ~1,3 px), «Сплющить» булевой группы даёт сотни точек на кривую. Выпуклый одиночный контур (прямоугольник, эллипс) не прогоняется через приведение — только ориентация; отражение ребёнка разворачивает обход контуров (иначе отражённая форма вычиталась бы из объединения).
15. Кэш: результат лежит в узле сцены и пересчитывается, когда меняется ключ содержимого (у листа — версия, у текста — ещё эпоха шрифтов, у контейнера — тип, операция, ключи, положения и видимость детей). Сдвиг и поворот самой группы и её заливки результат не пересчитывают. Новый результат — новая `version` узла (рендер перестраивает список). Формы листьев кэшируются в узле (`fill`), формы детей — в результате по ключу.
16. Во время жеста (есть превью) над ровно одним сменившимся ребёнком остальные складываются один раз (`combine_rest`, по частям «внешний контур + дыры» с рамками), на каждое движение — операция только с частями, чья рамка задевает ребёнка (`combine_with`). Запись жеста (редактор снимает превью — ребёнок на прежнем месте, — затем применяет правку) — тот же ребёнок, сложенные остальные в кэше: оба шага по частям, полного пересчёта нет; ключ, совпавший с прошлым, не пересчитывает ничего. Правка другого ребёнка или нескольких без жеста, смена операции — полный пересчёт (части сбрасываются). Результат по частям может отличаться от полного на округление `i_overlay` (доли миллионной): тест сверяет площадь и рамку с допуском, полный пересчёт — снимок сцены побайтно. Сложенные остальные держатся в результате до следующего полного пересчёта (память — ещё одна копия результата).
17. Текст в булевой группе: сцена шрифтов не знает — редактор ставит ей источник контуров (`install_text_outlines` на каждую новую сцену и в `set_fonts`). Реестр берётся через `try_borrow_mut`; занят (пересчёт сцены посреди раскладки) — текст на этот раз без формы, а флаг `outlines_missed` помечается. `sync_text_outlines` (из каждого `emit` и из `after_fonts_changed` ядра; реестр читает через `try_borrow`) пересчитывает группы с текстом при смене поколения реестра или по флагу — то есть на следующей команде или кадре.
18. **(поведение)** «Сплющить»: булева группа — в вектор с её результатом, текст — в вектор контуров глифов; оба — новым узлом на своём месте (дети и содержимое уходят), имя, заливки, обводки и поза сохраняются. В составе нескольких слоёв — как остальные. Текст без загруженных шрифтов — «Шрифты ещё загружаются — текст не сплющен», без шага (и в составе — ничего не теряется). Группа с текстом теперь сплющивается.
19. **(поведение)** «Контур обводки» (Ctrl/Cmd+Alt+O): у каждого выделенного слоя (не группы и не фрейма; видимого, не заблокированного) с видимыми обводками — вектор прямо над ним с его именем и позой, без заливок и обводок, по области на каждую видимую обводку её цветом (верхняя обводка — верхняя область). Слой с видимыми заливками остаётся без обводок, без заливок — удаляется. Кольцевые фигуры — кольцо (even-odd, кривые); обводка по центру — `vector::stroke_outline` (кривые, окончания, соединение, предел); внутри и снаружи замкнутого — обрезка контуром формы булевой операцией (ломаные); текст — `kurbo::stroke` глифов (внутри и снаружи — двойной толщиной с обрезкой). Без обводок — «У выделенных слоёв нет видимых обводок».
20. Протокол: `NodeType::boolean` и `LayerRow.booleanOp` (иконка по операции), `Summary.booleanOp` (только у выделения из булевых групп с одной операцией); команды — обычный `Execute`; геометрия в протокол не идёт, основной поток ничего не считает.
21. **(поведение)** Тулбар: кнопка-список «Булевы операции» после «Текста»; кнопка выполняет операцию на ней (операция выделенной булевой группы, иначе последняя выбранная, сначала — объединение), список — 4 операции (`menuitemcheckbox`) с сочетаниями и отметкой, разделитель, «Сплющить», «Контур обводки»; без выделения кнопка и пункты неактивны; `aria-pressed` — у выделенной булевой группы. Подсказка кнопки берёт сочетание из действующей раскладки.
22. **(поведение)** Клавиши — как в Figma (справка «Boolean operations»): объединение Alt+Shift+U, вычитание Alt+Shift+S, пересечение Alt+Shift+I, исключение Alt+Shift+E (Option+Shift на macOS), на всех платформах; «Контур обводки» — Ctrl/Cmd+Alt+O, «Сплющить» — Ctrl/Cmd+E; всего команд 53. Спека называла Ctrl/Cmd+Alt+U/S/I/X — исправлено в спеке (разделы 8 и 10). В таблице нет других сочетаний Alt+Shift+буква, Shift+E и Alt+E свободны (тест `boolean_ops_are_alt_shift_letters_like_figma`; общий тест умолчаний — без конфликтов и без зарезервированных браузером на всех платформах). Правка текста перехватывает клавиши сама (Option+Shift+буква на маке набирает символ). AltGr на Windows и Linux теперь глушит все сочетания с Alt, не только Mod+Alt (AltGr+Shift+E — «Ę» на польской раскладке). Вне приложения: на Windows Alt+Shift без буквы по умолчанию переключает раскладку — с буквой не мешает.
23. Совместимость: `boolean_fixture` (прямоугольник + «Вычитание» из прямоугольника и эллипса); сборка без булевых групп скрывает группу с детьми и открывает файл только для чтения. Содержательная проверка — против 0797cf7 (`scripts/compat-old-build.sh 0797cf7 boolean_fixture`): локальная ветка `main` машины проверки старая (cd26851, до подсистемы 3), и `main boolean_fixture` проверял бы против сборки без текста и векторов. Старая сборка — в своём каталоге `$CARGO_TARGET_DIR/compat-old`.

## Как план закрывает повторявшиеся дефекты этапов 2–3

- **Поворот и отражение.** Булева группа — group-like во всех путях позы (`is_group_like`); результат считается в координатах группы, отражённый ребёнок разворачивает обход (тест `flipped_child_still_subtracts`, сцена); «Контур обводки» и «Сплющить» сохраняют позу узла.
- **Превью и запись — один шаг.** Новая группа, смена операции, «Сплющить», «Контур обводки» — по одной транзакции; жест над ребёнком — превью сцены и одна запись при отпускании (тесты сверяют число шагов, отмену и повтор).
- **Чтения layout вне `measure`.** Новый код основного потока — только JSX и команды; E2E булевых групп проверяют `__ferrumWarnings` после каждого теста.
- **Клавиши.** Сочетания — по физическим `code`; AltGr глушит все сочетания с Alt (тест раскладки), правка текста клавиши перехватывает сама.
- **Недоверенные значения отклоняются, а не зажимаются** — неизвестная операция из файла, буфера обмена и кэша сцены.
- **Пиксели на каждый вариант:** четыре операции, вложенная булева группа, вычитание с дырой, пустой результат, подсветка по контуру, затем эталоны.
- **Бюджет кадра.** Замеры в `render_report` и `editor_report` на 1000 кругов с целями (операция ≤ 50 мс, кадр и запись жеста ≤ 16,7 мс); первая редакция прототипа писала жест двумя полными пересчётами (≈ 37 мс) — отсюда пересчёт по частям и при записи (задача 8).

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-doc/src/boolean.rs` (новый), `{types,codec,page,scene_cache,container,error,lib}.rs`, `examples/boolean_fixture.rs` (новый) | операция, ключ узла, кэш сцены v5, пример для совместимости |
| `crates/ferrum-scene/src/boolean.rs` (новый), `scene.rs`, `shape.rs`, `vector.rs` | дробление, приведение, операции, результат в узле сцены, кэш по ключу, части для жеста, границы и попадание |
| `crates/ferrum-render/src/{display,draw,overlay,text_node,lib}.rs` | результат как путь узла, подсветка, контуры глифов для сцены |
| `crates/ferrum-editor/src/boolean.rs` (новый) | команды операций и смена операции |
| `crates/ferrum-editor/src/outline.rs` (новый), `{flatten,editor,text,vector_edit,vector_ops}.rs` | «Контур обводки», «Сплющить» булевой группы и текста, источник контуров глифов |
| `crates/ferrum-editor/src/{actions,commands,keymap,geometry,panel,clipboard,lib}.rs`, `tools/select.rs` | клавиши, раскладка, group-like, буфер обмена |
| `crates/ferrum-wasm/src/{summary,layers}.rs`, `core/fonts.rs` | строка слоя, сводка, пересчёт по приходу шрифтов |
| `apps/web/src/ui/{toolbar,layers,icons}.tsx` | кнопка-список, иконки операций |
| `apps/web/e2e/…`, `scripts/compat-old-build.sh` | E2E булевых групп, совместимость |
| `crates/{ferrum-render,ferrum-editor}/examples/*_report.rs`, спека | замеры 1000 кругов; решения этапа 4 |

---

### Task 1: Модель: узел `Boolean`

Спека 10: булева группа — узел с операцией и детьми, «как группа». Здесь — `BooleanOp` и узел `NodeKind::Boolean` (ключ `booleanOp`, без ключа — объединение), `NodeKind::is_group_like()`, `Props::boolean_op` и `PropsPatch::boolean_op`, неверная операция из Loro — как неизвестный тип, кэш сцены версии 5, поле буфера обмена, пример `boolean_fixture` и третий вариант скрипта совместимости — старая сборка в своём каталоге сборки. Чтобы рабочее пространство собиралось, сцена пока не даёт булевой группе геометрии, слой — иконка группы.

**Files:**
- Create: `crates/ferrum-doc/src/boolean.rs`, `crates/ferrum-doc/examples/boolean_fixture.rs`
- Modify: `crates/ferrum-doc/src/{types,codec,page,scene_cache,container,error,lib}.rs`, `crates/ferrum-editor/src/clipboard.rs`, `crates/ferrum-scene/src/shape.rs` (временно `Geometry::None`), `crates/ferrum-wasm/src/layers.rs`, `apps/web/src/ui/layers.tsx` (временно иконка группы), `scripts/compat-old-build.sh`
- Test: `crates/ferrum-doc/tests/boolean.rs` (новый), `crates/ferrum-doc/tests/{container,flip}.rs`, `crates/ferrum-editor/tests/clipboard_format.rs`

**Interfaces:**
- Produces: `BooleanOp { Union, Subtract, Intersect, Exclude }` (`ALL`, `as_str`, `parse`, `name`), `NodeKind::Boolean` (`"boolean"`, имя «Объединение»), `NodeKind::is_group_like()` (Group | Boolean), `Props::boolean_op: Option<BooleanOp>` (только у Boolean), `PropsPatch::boolean_op` (у другого узла пропускается), ключ `booleanOp`, `DocError::InvalidBooleanOp(id, raw)`, `PageDoc::force_boolean_op_for_tests`, `SCENE_CACHE_VERSION = 5`, поле буфера обмена `booleanOp`, `NodeType::Boolean`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-doc/tests/boolean.rs`:

````rust
//! Узел `Boolean` (подсистема 3, этап 4): операция в документе, зеркало против
//! перезагрузки из Loro и из кэша сцены, отмена, чужая операция в патче, не
//! доверенная операция в файле.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;

/// Булева группа «Вычитание» с двумя прямоугольниками.
fn doc_with_boolean() -> (Document, PageId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            let b = tx.create_with_props(
                None,
                None,
                &Props {
                    name: "Вычитание".into(),
                    boolean_op: Some(BooleanOp::Subtract),
                    ..Props::defaults(NodeKind::Boolean)
                },
            )?;
            for x in [0.0, 50.0] {
                let r = tx.create_node(Some(b), None, NodeKind::Rect)?;
                tx.set_props(
                    r,
                    &PropsPatch {
                        x: Some(x),
                        ..Default::default()
                    },
                )?;
            }
            Ok(vec![b])
        })
        .unwrap()[0];
    (doc, page, id)
}

#[test]
fn op_strings_round_trip_and_unknown_is_rejected() {
    for op in BooleanOp::ALL {
        assert_eq!(BooleanOp::parse(op.as_str()), Some(op));
    }
    assert_eq!(BooleanOp::parse("divide"), None);
    assert_eq!(BooleanOp::parse("Union"), None);
    assert_eq!(NodeKind::parse("boolean"), Some(NodeKind::Boolean));
    assert!(NodeKind::Boolean.is_group_like() && NodeKind::Group.is_group_like());
    assert!(!NodeKind::Frame.is_group_like());
    let d = Props::defaults(NodeKind::Boolean);
    assert_eq!((d.w, d.h, d.boolean_op), (0.0, 0.0, Some(BooleanOp::Union)));
    assert_eq!(d.name, "Объединение");
    assert_eq!(Props::defaults(NodeKind::Group).boolean_op, None);
}

#[test]
fn boolean_survives_reload_from_loro_and_cache() {
    let (mut doc, page, id) = doc_with_boolean();
    let before = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(before.boolean_op, Some(BooleanOp::Subtract));
    assert_eq!(doc.page(page).unwrap().children(Some(id)).len(), 2);
    let snapshot = doc.page(page).unwrap().export_snapshot().unwrap();
    let loro = PageDoc::from_snapshot(&snapshot).unwrap();
    assert_eq!(loro.props(id).unwrap(), before);
    assert_eq!(loro.children(Some(id)).len(), 2);
    let mut opened = decode(&encode(&doc).unwrap()).unwrap().document;
    let p = opened.page(page).unwrap();
    assert!(p.loaded_from_cache());
    assert_eq!(p.props(id).unwrap(), before);
    assert!(!opened.is_read_only());
}

#[test]
fn op_edit_is_one_undo_step_and_foreign_op_is_ignored() {
    let (mut doc, page, id) = doc_with_boolean();
    doc.transact(page, vec![id], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                boolean_op: Some(BooleanOp::Exclude),
                name: Some("Исключение".into()),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    })
    .unwrap();
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.boolean_op, Some(BooleanOp::Exclude));
    doc.undo().unwrap();
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.boolean_op, Some(BooleanOp::Subtract));
    assert_eq!(p.name, "Вычитание");
    // Операция в патче прямоугольника пропускается, остальное пишется.
    let rect = doc.page(page).unwrap().children(Some(id))[0];
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            rect,
            &PropsPatch {
                boolean_op: Some(BooleanOp::Intersect),
                x: Some(7.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let r = doc.page(page).unwrap().props(rect).unwrap();
    assert_eq!((r.boolean_op, r.x), (None, 7.0));
    let reloaded =
        PageDoc::from_snapshot(&doc.page(page).unwrap().export_snapshot().unwrap()).unwrap();
    assert_eq!(reloaded.props(rect).unwrap().boolean_op, None);
}

#[test]
fn unknown_op_in_file_hides_the_subtree_and_opens_read_only() {
    let (mut doc, page, id) = doc_with_boolean();
    let p = doc.page(page).unwrap();
    for raw in ["divide", ""] {
        p.force_boolean_op_for_tests(id, raw);
        let reopened = PageDoc::from_snapshot(&p.export_snapshot().unwrap()).unwrap();
        assert!(reopened.has_unknown_nodes(), "только для чтения");
        assert!(reopened.walk().is_empty(), "группа с детьми скрыта");
    }
    // Известная операция снова читается.
    p.force_boolean_op_for_tests(id, "intersect");
    let reopened = PageDoc::from_snapshot(&p.export_snapshot().unwrap()).unwrap();
    assert!(!reopened.has_unknown_nodes());
    assert_eq!(
        reopened.props(id).unwrap().boolean_op,
        Some(BooleanOp::Intersect)
    );
    assert!(DocError::InvalidBooleanOp("1@2".into(), "divide".into())
        .to_string()
        .contains("«divide»"));
}

#[test]
fn boolean_cannot_be_converted_to_a_vector() {
    let (mut doc, page, id) = doc_with_boolean();
    let err = doc.transact(page, vec![], |tx| {
        tx.convert_to_vector(id, std::sync::Arc::new(VectorNetwork::default()))?;
        Ok(vec![])
    });
    assert!(matches!(err, Err(DocError::NotConvertible(_))), "{err:?}");
}
````

В `crates/ferrum-doc/tests/container.rs` заменить:

````rust
        NodeKind::Text,
        NodeKind::Vector,
    ];
    doc.transact(page, vec![], |p| {
````

на:

````rust
        NodeKind::Text,
        NodeKind::Vector,
        NodeKind::Boolean,
    ];
    doc.transact(page, vec![], |p| {
````

В `crates/ferrum-doc/tests/container.rs` заменить:

````rust
                        regions: vec![],
                    })),
                },
            )?;
````

на:

````rust
                        regions: vec![],
                    })),
                    // У не булевой группы пропускается; у неё — не объединение.
                    boolean_op: Some(BooleanOp::Exclude),
                },
            )?;
````

В `crates/ferrum-doc/tests/flip.rs` заменить:

````rust
fn scene_cache_version_covers_flips() {
    assert_eq!(
        SCENE_CACHE_VERSION, 4,
        "таблица с флагами отражения (2), текстом (3), стилем обводки и сетью (4)"
    );
    // Кэш прежней версии с честным хэшем снапшота игнорируется без предупреждений.
````

на:

````rust
fn scene_cache_version_covers_flips() {
    assert_eq!(
        SCENE_CACHE_VERSION, 5,
        "таблица с флагами отражения (2), текстом (3), стилем обводки и сетью (4), булевой группой (5)"
    );
    // Кэш прежней версии с честным хэшем снапшота игнорируется без предупреждений.
````

В `crates/ferrum-editor/tests/clipboard_format.rs` заменить:

````rust
        Some(&VectorNetwork::default())
    );
}
````

на:

````rust
        Some(&VectorNetwork::default())
    );
}

#[test]
fn boolean_with_children_round_trips_and_op_is_checked() {
    use ferrum_doc::BooleanOp;
    let mut b = Props::defaults(NodeKind::Boolean);
    b.boolean_op = Some(BooleanOp::Intersect);
    let p = ClipPayload {
        nodes: vec![
            ClipNode {
                parent: None,
                props: b,
            },
            ClipNode {
                parent: Some(0),
                props: Props::defaults(NodeKind::Rect),
            },
            ClipNode {
                parent: Some(0),
                props: Props::defaults(NodeKind::Ellipse),
            },
        ],
        images: vec![],
        bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
    };
    let bytes = p.encode();
    let json = json_of(&bytes);
    assert!(json.contains("\"booleanOp\":\"intersect\""), "{json}");
    assert_eq!(ClipPayload::decode(&bytes).unwrap(), p);
    // Неизвестная операция — отказ вставки.
    let bad = json.replace("\"intersect\"", "\"divide\"");
    assert!(matches!(
        ClipPayload::decode(&with_json(&bad)),
        Err(ClipboardError::Corrupted(_))
    ));
    // Без поля — объединение; размер группы из чужих данных не принимается.
    let plain = ClipPayload::decode(&with_json(&doc_json(
        &[node_json(None, "boolean")],
        &[],
        "[0.0,0.0,10.0,10.0]",
    )))
    .unwrap();
    let props = &plain.nodes[0].props;
    assert_eq!(props.boolean_op, Some(BooleanOp::Union));
    assert_eq!((props.w, props.h), (0.0, 0.0));
    // У прямоугольника поле операции пропускается.
    assert!(p.nodes[1].props.boolean_op.is_none());
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc --test boolean`
Expected: FAIL — не собирается: нет `BooleanOp`, `NodeKind::Boolean`, `NodeKind::is_group_like`, полей `Props::boolean_op` и `PropsPatch::boolean_op`, `PageDoc::force_boolean_op_for_tests`, `DocError::InvalidBooleanOp` (32 ошибки)

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/layers.tsx` заменить:

````tsx
  text: 'text',
  vector: 'pen',
};

````

на:

````tsx
  text: 'text',
  vector: 'pen',
  boolean: 'group',
};

````

Создать `crates/ferrum-doc/examples/boolean_fixture.rs`:

````rust
//! Файл `.ferrum` с булевой группой — для проверки совместимости: сборка без
//! булевых групп (`main` до этапа 4) открывает его только для чтения — группа с
//! детьми скрыта, как узел неизвестного типа (спека подсистемы 3, раздел 10).
//! `cargo run -p ferrum-doc --example boolean_fixture -- <путь>`; проверку старой
//! сборкой делает `scripts/compat-old-build.sh <ref> boolean_fixture`.

use ferrum_doc::container::encode;
use ferrum_doc::{BooleanOp, Document, NodeKind, Props, PropsPatch};

fn main() {
    let path = std::env::args().nth(1).expect("путь к файлу");
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.transact(page, vec![], |tx| {
        let rect = tx.create_node(None, None, NodeKind::Rect)?;
        tx.set_props(
            rect,
            &PropsPatch {
                name: Some("Прямоугольник".into()),
                ..Default::default()
            },
        )?;
        let b = tx.create_with_props(
            None,
            None,
            &Props {
                name: BooleanOp::Subtract.name().into(),
                y: 150.0,
                boolean_op: Some(BooleanOp::Subtract),
                ..Props::defaults(NodeKind::Boolean)
            },
        )?;
        for (kind, x) in [(NodeKind::Rect, 0.0), (NodeKind::Ellipse, 50.0)] {
            let c = tx.create_node(Some(b), None, kind)?;
            tx.set_props(
                c,
                &PropsPatch {
                    x: Some(x),
                    ..Default::default()
                },
            )?;
        }
        Ok(vec![])
    })
    .unwrap();
    std::fs::write(&path, encode(&doc).unwrap()).unwrap();
    println!("{path}: прямоугольник и булева группа с двумя детьми");
}
````

Создать `crates/ferrum-doc/src/boolean.rs`:

````rust
//! Булева операция (спека подсистемы 3, раздел 10): узел `boolean` — контейнер,
//! как группа, а его форма — результат операции над формами детей. Сама
//! операция — значение ключа `booleanOp` узла; результат документ не хранит, его
//! считает сцена.

/// Операция булевой группы, как в Figma: объединение, вычитание верхних слоёв из
/// нижнего, пересечение всех, исключение (нечётное число перекрытий).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BooleanOp {
    #[default]
    Union,
    Subtract,
    Intersect,
    Exclude,
}

impl BooleanOp {
    pub const ALL: [BooleanOp; 4] = [
        BooleanOp::Union,
        BooleanOp::Subtract,
        BooleanOp::Intersect,
        BooleanOp::Exclude,
    ];

    /// Значение в документе, буфере обмена и протоколе.
    pub fn as_str(self) -> &'static str {
        match self {
            BooleanOp::Union => "union",
            BooleanOp::Subtract => "subtract",
            BooleanOp::Intersect => "intersect",
            BooleanOp::Exclude => "exclude",
        }
    }

    /// Только известные значения: данные из файла и буфера обмена не доверенные.
    pub fn parse(s: &str) -> Option<BooleanOp> {
        BooleanOp::ALL.into_iter().find(|op| op.as_str() == s)
    }

    /// Название операции в интерфейсе — им же называется новая булева группа.
    pub fn name(self) -> &'static str {
        match self {
            BooleanOp::Union => "Объединение",
            BooleanOp::Subtract => "Вычитание",
            BooleanOp::Intersect => "Пересечение",
            BooleanOp::Exclude => "Исключение",
        }
    }
}
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
use loro::{Container, LoroMap, LoroValue, ValueOrContainer};

use crate::error::{DocError, DocResult};
use crate::text::TextData;
````

на:

````rust
use loro::{Container, LoroMap, LoroValue, ValueOrContainer};

use crate::boolean::BooleanOp;
use crate::error::{DocError, DocResult};
use crate::text::TextData;
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
    /// Сеть узла `vector` — двоичное значение (`VectorNetwork::encode`).
    pub const VECTOR: &str = "vector";
}

````

на:

````rust
    /// Сеть узла `vector` — двоичное значение (`VectorNetwork::encode`).
    pub const VECTOR: &str = "vector";
    /// Операция узла `boolean` — строка (`BooleanOp::as_str`).
    pub const BOOLEAN_OP: &str = "booleanOp";
}

````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
}

/// Тип узла читается отдельно: по нему выбираются значения по умолчанию.
pub(crate) fn read_kind(map: &LoroMap, id: &NodeId) -> DocResult<NodeKind> {
````

на:

````rust
}

/// Операция булевой группы; нет ключа — объединение. Неизвестная (будущая
/// сборка, повреждение) — ошибка: узел скрывается, как узел неизвестного типа,
/// документ — только для чтения (`PageDoc::rebuild_from_loro`).
fn read_boolean_op(map: &LoroMap, id: &NodeId) -> DocResult<BooleanOp> {
    match value(map, key::BOOLEAN_OP) {
        None => Ok(BooleanOp::Union),
        Some(v) => {
            let raw = as_str(&v).unwrap_or_default();
            BooleanOp::parse(&raw).ok_or_else(|| DocError::InvalidBooleanOp(id.to_string(), raw))
        }
    }
}

/// Тип узла читается отдельно: по нему выбираются значения по умолчанию.
pub(crate) fn read_kind(map: &LoroMap, id: &NodeId) -> DocResult<NodeKind> {
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
        vector: match kind {
            NodeKind::Vector => Some(Arc::new(read_vector(map, id)?)),
            _ => None,
        },
````

на:

````rust
        vector: match kind {
            NodeKind::Vector => Some(Arc::new(read_vector(map, id)?)),
            _ => None,
        },
        boolean_op: match kind {
            NodeKind::Boolean => Some(read_boolean_op(map, id)?),
            _ => None,
        },
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
        stroke_miter_limit: neutral(p.stroke_style.miter_limit, crate::DEFAULT_MITER_LIMIT),
        vector: p.vector.clone(),
    };
    write_patch(map, &patch)?;
````

на:

````rust
        stroke_miter_limit: neutral(p.stroke_style.miter_limit, crate::DEFAULT_MITER_LIMIT),
        vector: p.vector.clone(),
        boolean_op: p.boolean_op,
    };
    write_patch(map, &patch)?;
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
        map.insert(key::STROKE_MITER_LIMIT, v)?;
    }
    // Вызывающий убирает `text` и `vector` из патча узла другого типа
    // (`PageDoc::set_props`).
    if let Some(text) = &p.text {
````

на:

````rust
        map.insert(key::STROKE_MITER_LIMIT, v)?;
    }
    // Вызывающий убирает `text`, `vector` и `boolean_op` из патча узла другого типа
    // (`PageDoc::set_props`).
    if let Some(text) = &p.text {
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
    if let Some(v) = &p.vector {
        map.insert(key::VECTOR, LoroValue::Binary(v.encode().into()))?;
    }
    Ok(())
````

на:

````rust
    if let Some(v) = &p.vector {
        map.insert(key::VECTOR, LoroValue::Binary(v.encode().into()))?;
    }
    if let Some(op) = p.boolean_op {
        map.insert(key::BOOLEAN_OP, op.as_str())?;
    }
    Ok(())
````

В `crates/ferrum-doc/src/container.rs` заменить:

````rust
/// таблицу без флагов рядом со снапшотом, где они есть, а зеркало страницы
/// берётся из таблицы — флаги тихо пропали бы и из правок.
/// 3 — текст, 4 — стиль обводки и векторная сеть.
pub const SCENE_CACHE_VERSION: u8 = 4;

const ROOT: u8 = 1;
````

на:

````rust
/// таблицу без флагов рядом со снапшотом, где они есть, а зеркало страницы
/// берётся из таблицы — флаги тихо пропали бы и из правок.
/// 3 — текст, 4 — стиль обводки и векторная сеть, 5 — булева группа.
pub const SCENE_CACHE_VERSION: u8 = 5;

const ROOT: u8 = 1;
````

В `crates/ferrum-doc/src/error.rs` заменить:

````rust
    #[error("Узел {0}: {1}")]
    InvalidVector(String, crate::vector::VectorError),
    #[error("Вектор длиннее 100 000 точек — правка не применена")]
    VectorTooLarge,
````

на:

````rust
    #[error("Узел {0}: {1}")]
    InvalidVector(String, crate::vector::VectorError),
    /// Операция булевой группы из файла неизвестна: узел скрывается, как узел
    /// неизвестного типа, и документ открывается только для чтения.
    #[error("Узел {0} хранит неизвестную булеву операцию «{1}»")]
    InvalidBooleanOp(String, String),
    #[error("Вектор длиннее 100 000 точек — правка не применена")]
    VectorTooLarge,
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust

mod binary;
mod changes;
mod codec;
````

на:

````rust

mod binary;
mod boolean;
mod changes;
mod codec;
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
mod vector;

pub use changes::Changes;
pub use document::{Document, Restore, SCHEMA_VERSION};
````

на:

````rust
mod vector;

pub use boolean::BooleanOp;
pub use changes::Changes;
pub use document::{Document, Restore, SCHEMA_VERSION};
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
            let props = match codec::read_props(&self.tree.get_meta(tid)?, &id) {
                Ok(props) => props,
                // Неверная сеть вектора (чужая сборка, повреждение) — как
                // неизвестный тип: узел скрыт, документ только для чтения.
                Err(DocError::UnknownKind(..) | DocError::InvalidVector(..)) => {
                    unknown = true;
                    self.collect_hidden_refs(tid, &mut hidden_refs)?;
````

на:

````rust
            let props = match codec::read_props(&self.tree.get_meta(tid)?, &id) {
                Ok(props) => props,
                // Неверная сеть вектора и неизвестная булева операция (чужая
                // сборка, повреждение) — как неизвестный тип: узел скрыт,
                // документ только для чтения.
                Err(
                    DocError::UnknownKind(..)
                    | DocError::InvalidVector(..)
                    | DocError::InvalidBooleanOp(..),
                ) => {
                    unknown = true;
                    self.collect_hidden_refs(tid, &mut hidden_refs)?;
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        let meta = self.tree.get_meta(tid).expect("карта узла");
        meta.insert(codec::key::VECTOR, LoroValue::Binary(bytes.to_vec().into()))
            .expect("запись в Loro");
        self.doc.commit();
````

на:

````rust
        let meta = self.tree.get_meta(tid).expect("карта узла");
        meta.insert(codec::key::VECTOR, LoroValue::Binary(bytes.to_vec().into()))
            .expect("запись в Loro");
        self.doc.commit();
    }

    /// Только для тестов: значение `booleanOp` узла в Loro как есть (как в файле
    /// чужой сборки) — без проверки и без зеркала.
    #[doc(hidden)]
    pub fn force_boolean_op_for_tests(&self, id: NodeId, op: &str) {
        let tid = self.tid(id).expect("узел есть");
        let meta = self.tree.get_meta(tid).expect("карта узла");
        meta.insert(codec::key::BOOLEAN_OP, op)
            .expect("запись в Loro");
        self.doc.commit();
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        let foreign_text = before.text.is_none() && patch.text.is_some();
        let foreign_vector = before.vector.is_none() && patch.vector.is_some();
        let patch = if foreign_text || foreign_vector {
            stripped = PropsPatch {
                text: if foreign_text {
````

на:

````rust
        let foreign_text = before.text.is_none() && patch.text.is_some();
        let foreign_vector = before.vector.is_none() && patch.vector.is_some();
        let foreign_op = before.boolean_op.is_none() && patch.boolean_op.is_some();
        let patch = if foreign_text || foreign_vector || foreign_op {
            stripped = PropsPatch {
                text: if foreign_text {
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
                    patch.vector.clone()
                },
                ..patch.clone()
            };
````

на:

````rust
                    patch.vector.clone()
                },
                boolean_op: if foreign_op { None } else { patch.boolean_op },
                ..patch.clone()
            };
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    ) -> DocResult<()> {
        let before = self.props(id)?;
        if before.text.is_some() || matches!(before.kind, NodeKind::Frame | NodeKind::Group) {
            return Err(DocError::NotConvertible(id.to_string()));
        }
````

на:

````rust
    ) -> DocResult<()> {
        let before = self.props(id)?;
        if before.text.is_some()
            || matches!(
                before.kind,
                NodeKind::Frame | NodeKind::Group | NodeKind::Boolean
            )
        {
            return Err(DocError::NotConvertible(id.to_string()));
        }
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust

use crate::binary::{Reader, Writer};
use crate::text::*;
use crate::types::*;
````

на:

````rust

use crate::binary::{Reader, Writer};
use crate::boolean::BooleanOp;
use crate::text::*;
use crate::types::*;
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
        // обводок — стиль обводки, у вектора после него — сеть (`write_vector`).
        NodeKind::Vector => 7,
    }
}
````

на:

````rust
        // обводок — стиль обводки, у вектора после него — сеть (`write_vector`).
        NodeKind::Vector => 7,
        // Пятая версия (`SCENE_CACHE_VERSION = 5`): у булевой группы после стиля
        // обводки — код операции.
        NodeKind::Boolean => 8,
    }
}
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
        NodeKind::Text,
        NodeKind::Vector,
    ]
    .get(c as usize)
````

на:

````rust
        NodeKind::Text,
        NodeKind::Vector,
        NodeKind::Boolean,
    ]
    .get(c as usize)
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
            w.u32(u32::try_from(bytes.len()).ok()?);
            w.bytes(&bytes);
        }
    }
````

на:

````rust
            w.u32(u32::try_from(bytes.len()).ok()?);
            w.bytes(&bytes);
        }
        if let Some(op) = p.boolean_op {
            w.u8(code(&BooleanOp::ALL, op));
        }
    }
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
        let boolean_op = match kind {
            NodeKind::Boolean => Some(decode_enum(&BooleanOp::ALL, r.u8()?)?),
            _ => None,
        };
        let [x, y, w, h, rotation, opacity, corner_radius] = nums;
        rows.push(SceneRow {
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
                text,
                vector,
            },
        });
````

на:

````rust
                text,
                vector,
                boolean_op,
            },
        });
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
use loro::TreeID;

use crate::error::DocError;
use crate::text::{TextContent, TextData, TextProps, TEXT_COLOR};
````

на:

````rust
use loro::TreeID;

use crate::boolean::BooleanOp;
use crate::error::DocError;
use crate::text::{TextContent, TextData, TextProps, TEXT_COLOR};
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    /// Векторная сеть (спека подсистемы 3, раздел 9).
    Vector,
}

````

на:

````rust
    /// Векторная сеть (спека подсистемы 3, раздел 9).
    Vector,
    /// Булева группа (спека подсистемы 3, раздел 10): дети, как у группы, форма —
    /// результат операции над ними.
    Boolean,
}

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            NodeKind::Text => "text",
            NodeKind::Vector => "vector",
        }
    }
````

на:

````rust
            NodeKind::Text => "text",
            NodeKind::Vector => "vector",
            NodeKind::Boolean => "boolean",
        }
    }
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            "text" => NodeKind::Text,
            "vector" => NodeKind::Vector,
            _ => return None,
        })
````

на:

````rust
            "text" => NodeKind::Text,
            "vector" => NodeKind::Vector,
            "boolean" => NodeKind::Boolean,
            _ => return None,
        })
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            NodeKind::Text => "Текст",
            NodeKind::Vector => "Вектор",
        }
    }
}
````

на:

````rust
            NodeKind::Text => "Текст",
            NodeKind::Vector => "Вектор",
            NodeKind::Boolean => BooleanOp::Union.name(),
        }
    }

    /// Своей рамки нет, границы — по детям (группа и булева группа): размер не
    /// хранится, ресайз масштабирует детей.
    pub fn is_group_like(self) -> bool {
        matches!(self, NodeKind::Group | NodeKind::Boolean)
    }
}
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    /// 100 000 точек, а сравнение одной и той же сети — по указателю.
    pub vector: Option<Arc<VectorNetwork>>,
}

````

на:

````rust
    /// 100 000 точек, а сравнение одной и той же сети — по указателю.
    pub vector: Option<Arc<VectorNetwork>>,
    /// Операция — только у `NodeKind::Boolean`.
    pub boolean_op: Option<BooleanOp>,
}

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            NodeKind::Frame => (100.0, 100.0, vec![Fill::solid(Color::WHITE)], vec![]),
            NodeKind::Group => (0.0, 0.0, vec![], vec![]),
            NodeKind::Rect | NodeKind::Ellipse => {
                (100.0, 100.0, vec![Fill::solid(Color::SHAPE_GRAY)], vec![])
````

на:

````rust
            NodeKind::Frame => (100.0, 100.0, vec![Fill::solid(Color::WHITE)], vec![]),
            NodeKind::Group => (0.0, 0.0, vec![], vec![]),
            // Заливки и обводки булевой группы берутся у детей при создании
            // (редактор); по умолчанию — серая, как у фигуры.
            NodeKind::Boolean => (0.0, 0.0, vec![Fill::solid(Color::SHAPE_GRAY)], vec![]),
            NodeKind::Rect | NodeKind::Ellipse => {
                (100.0, 100.0, vec![Fill::solid(Color::SHAPE_GRAY)], vec![])
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            text: (kind == NodeKind::Text).then(|| Arc::new(TextData::default())),
            vector: (kind == NodeKind::Vector).then(|| Arc::new(VectorNetwork::default())),
        }
    }
````

на:

````rust
            text: (kind == NodeKind::Text).then(|| Arc::new(TextData::default())),
            vector: (kind == NodeKind::Vector).then(|| Arc::new(VectorNetwork::default())),
            boolean_op: (kind == NodeKind::Boolean).then_some(BooleanOp::Union),
        }
    }
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    /// Сеть целиком (значение узла заменяется). У не вектора пропускается.
    pub vector: Option<Arc<VectorNetwork>>,
}

````

на:

````rust
    /// Сеть целиком (значение узла заменяется). У не вектора пропускается.
    pub vector: Option<Arc<VectorNetwork>>,
    /// Операция булевой группы. У другого узла пропускается.
    pub boolean_op: Option<BooleanOp>,
}

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
                _ => None,
            },
        }
    }
````

на:

````rust
                _ => None,
            },
            boolean_op: self.boolean_op.and(before.boolean_op),
        }
    }
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            *dst = v.clone();
        }

        if let Some(t) = &mut p.text {
````

на:

````rust
            *dst = v.clone();
        }
        if let (Some(v), Some(dst)) = (self.boolean_op, &mut p.boolean_op) {
            *dst = v;
        }

        if let Some(t) = &mut p.text {
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
//! `strokeMiterLimit` — только не нейтральные) и сеть вектора (`vector` — base64
//! двоичной формы `VectorNetwork`): сборка без векторов пропускает узел «vector»
//! как неизвестный.
//!
//! Вставка не доверяет данным: размеры и количества ограничены, длины проверяются
````

на:

````rust
//! `strokeMiterLimit` — только не нейтральные) и сеть вектора (`vector` — base64
//! двоичной формы `VectorNetwork`): сборка без векторов пропускает узел «vector»
//! как неизвестный. Булева группа — узел «boolean» с полем `booleanOp` (только
//! известные операции; без поля — объединение): сборка без булевых групп
//! пропускает её вместе с детьми.
//!
//! Вставка не доверяет данным: размеры и количества ограничены, длины проверяются
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust

use ferrum_doc::{
    Color, Fill, FillKind, NodeKind, Props, PropsPatch, ScaleMode, Stroke, StrokeAlign, StrokeCap,
    StrokeJoin, StrokeStyle, TextData, VectorNetwork, DEFAULT_MITER_LIMIT, MITER_LIMIT_RANGE,
};
use ferrum_scene::shape;
````

на:

````rust

use ferrum_doc::{
    BooleanOp, Color, Fill, FillKind, NodeKind, Props, PropsPatch, ScaleMode, Stroke, StrokeAlign,
    StrokeCap, StrokeJoin, StrokeStyle, TextData, VectorNetwork, DEFAULT_MITER_LIMIT,
    MITER_LIMIT_RANGE,
};
use ferrum_scene::shape;
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vector: Option<String>,
}

````

на:

````rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vector: Option<String>,
    /// Операция булевой группы — только у неё.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    boolean_op: Option<String>,
}

````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
                            .filter(|m| *m != DEFAULT_MITER_LIMIT),
                        vector: p.vector.as_ref().map(|v| v.to_base64()),
                    }
                })
````

на:

````rust
                            .filter(|m| *m != DEFAULT_MITER_LIMIT),
                        vector: p.vector.as_ref().map(|v| v.to_base64()),
                        boolean_op: p.boolean_op.map(|op| op.as_str().to_string()),
                    }
                })
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
            None => (None, 0),
            Some(k) => {
                if !matches!(nodes[k].props.kind, NodeKind::Frame | NodeKind::Group) {
                    return Err(corrupted("у узла этого типа не бывает потомков"));
                }
````

на:

````rust
            None => (None, 0),
            Some(k) => {
                if !matches!(
                    nodes[k].props.kind,
                    NodeKind::Frame | NodeKind::Group | NodeKind::Boolean
                ) {
                    return Err(corrupted("у узла этого типа не бывает потомков"));
                }
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
        // Операция — только у булевой группы; неизвестная — отказ вставки.
        boolean_op: match (kind, n.boolean_op.as_deref()) {
            (NodeKind::Boolean, Some(s)) => {
                Some(BooleanOp::parse(s).ok_or_else(|| corrupted("булева операция"))?)
            }
            (NodeKind::Boolean, None) => Some(BooleanOp::Union),
            _ => None,
        },
    };
    // Нечисловые и огромные значения — к допустимым, как у строк кэша сцены.
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
    match kind {
        NodeKind::Line => props.h = 0.0,
        NodeKind::Group => (props.w, props.h) = (0.0, 0.0),
        _ => {}
    }
````

на:

````rust
    match kind {
        NodeKind::Line => props.h = 0.0,
        NodeKind::Group | NodeKind::Boolean => (props.w, props.h) = (0.0, 0.0),
        _ => {}
    }
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
        stroke_miter_limit: Some(p.stroke_style.miter_limit).filter(|m| *m != DEFAULT_MITER_LIMIT),
        vector: p.vector.clone(),
    }
}
````

на:

````rust
        stroke_miter_limit: Some(p.stroke_style.miter_limit).filter(|m| *m != DEFAULT_MITER_LIMIT),
        vector: p.vector.clone(),
        boolean_op: p.boolean_op,
    }
}
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
    let rect = Rect::new(0.0, 0.0, w, h);
    match p.kind {
        NodeKind::Group => Geometry::None,
        NodeKind::Frame | NodeKind::Rect | NodeKind::Image => Geometry::Rect {
            rect,
````

на:

````rust
    let rect = Rect::new(0.0, 0.0, w, h);
    match p.kind {
        // Булева группа пока — как группа: результат операции — следующая задача.
        NodeKind::Group | NodeKind::Boolean => Geometry::None,
        NodeKind::Frame | NodeKind::Rect | NodeKind::Image => Geometry::Rect {
            rect,
````

В `crates/ferrum-wasm/src/layers.rs` заменить:

````rust
    Text,
    Vector,
}

````

на:

````rust
    Text,
    Vector,
    Boolean,
}

````

В `crates/ferrum-wasm/src/layers.rs` заменить:

````rust
            NodeKind::Text => NodeType::Text,
            NodeKind::Vector => NodeType::Vector,
        }
    }
````

на:

````rust
            NodeKind::Text => NodeType::Text,
            NodeKind::Vector => NodeType::Vector,
            NodeKind::Boolean => NodeType::Boolean,
        }
    }
````

В `scripts/compat-old-build.sh` заменить:

````
#!/bin/sh
# Совместимость (спека подсистемы 3, разделы 3, 9 и 12): файл с узлом нового вида,
# собранный новой сборкой, открывается сборкой без этого вида только для чтения:
# прямоугольник виден, новый узел — узел неизвестного типа — скрыт, правка
````

на:

````
#!/bin/sh
# Совместимость (спека подсистемы 3, разделы 3, 9, 10 и 12): файл с узлом нового вида,
# собранный новой сборкой, открывается сборкой без этого вида только для чтения:
# прямоугольник виден, новый узел — узел неизвестного типа — скрыт, правка
````

В `scripts/compat-old-build.sh` заменить:

````
#   scripts/compat-old-build.sh [ref] [fixture]
# fixture — пример `ferrum-doc`: `text_fixture` (по умолчанию; ref — сборка без
# текста, 2710c85) или `vector_fixture` (ref — сборка без векторов, например main
# до этапа 3).
set -eu
REF="${1:-2710c85}"
````

на:

````
#   scripts/compat-old-build.sh [ref] [fixture]
# fixture — пример `ferrum-doc`: `text_fixture` (по умолчанию; ref — сборка без
# текста, 2710c85), `vector_fixture` (ref — сборка без векторов, например main
# до этапа 3) или `boolean_fixture` (ref — сборка без булевых групп, например main
# до этапа 4: группа скрыта вместе с детьми).
# Старая сборка идёт в свой каталог `$CARGO_TARGET_DIR/compat-old`: у членов workspace
# хэш артефакта не зависит от пути, и в общем каталоге старый `ferrum-doc` перезаписал
# бы отладочную библиотеку новой сборки.
set -eu
REF="${1:-2710c85}"
````

В `scripts/compat-old-build.sh` заменить:

````
}
RS
(cd "$TMP/old" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}" FERRUM_FIXTURE_FILE="$FILE" \
  cargo test -q -p ferrum-doc --test compat_fixture)
echo "Совместимость с $REF ($FIXTURE): файл открыт только для чтения"
````

на:

````
}
RS
(cd "$TMP/old" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/compat-old" FERRUM_FIXTURE_FILE="$FILE" \
  cargo test -q -p ferrum-doc --test compat_fixture)
echo "Совместимость с $REF ($FIXTURE): файл открыт только для чтения"
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-scene -p ferrum-editor 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && ./scripts/compat-old-build.sh 0797cf7 boolean_fixture`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && ./scripts/compat-old-build.sh main vector_fixture`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Expected: PASS: fmt и clippy без замечаний; `boolean` модели — 5 тестов, `clipboard_format` — 21, остальные тесты `ferrum-doc`, `ferrum-scene`, `ferrum-editor` — как были; «Совместимость с 0797cf7 (boolean_fixture): файл открыт только для чтения» и «Совместимость с main (vector_fixture): файл открыт только для чтения»; wasm собран, `npm run typecheck` без ошибок

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/src/ui/layers.tsx crates/ferrum-doc/examples/boolean_fixture.rs crates/ferrum-doc/src/boolean.rs crates/ferrum-doc/src/codec.rs crates/ferrum-doc/src/container.rs crates/ferrum-doc/src/error.rs crates/ferrum-doc/src/lib.rs crates/ferrum-doc/src/page.rs crates/ferrum-doc/src/scene_cache.rs crates/ferrum-doc/src/types.rs crates/ferrum-doc/tests/boolean.rs crates/ferrum-doc/tests/container.rs crates/ferrum-doc/tests/flip.rs crates/ferrum-editor/src/clipboard.rs crates/ferrum-editor/tests/clipboard_format.rs crates/ferrum-scene/src/shape.rs crates/ferrum-wasm/src/layers.rs scripts/compat-old-build.sh
git commit -m "feat(doc): булева группа — узел Boolean с операцией (объединение, вычитание, пересечение, исключение), кэш сцены v5, буфер обмена, совместимость

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2: Сцена: результат булевой группы

Спека 10: результат — контур, пересчитанный ядром при изменении детей. Здесь — модуль `ferrum_scene::boolean` (дробление кривых, приведение формы к простым контурам её правилом, операция над N формами через `i_overlay`), результат `BooleanOutline` в узле сцены с кэшем по ключу содержимого детей, границы и попадание по результату (в детей — только внутри результата внешней группы), текст — контурами глифов через источник `TextOutlines`, который ставит редактор.

**Files:**
- Create: `crates/ferrum-scene/src/boolean.rs`
- Modify: `Cargo.toml`, `Cargo.lock`, `crates/ferrum-scene/Cargo.toml`, `crates/ferrum-scene/src/{scene,shape,vector,lib}.rs`
- Test: `crates/ferrum-scene/tests/boolean.rs` (новый)

**Interfaces:**
- Consumes: `BooleanOp`, `NodeKind::is_group_like`, `vector::network_shape`.
- Produces: `ferrum_scene::boolean::{BOOLEAN_TOLERANCE = 0.005, Contour, Contours (transformed, to_path, bounds, points, is_empty), flatten_path, normalize, combine, leaf_fill, outline_shape}`; `SceneNode::boolean: Option<Arc<BooleanOutline>>` (`contours`, `shape`), `SceneNode::path_shape()` у булевой — её результат; `Scene::set_text_outlines(Option<TextOutlines>)`, `Scene::refresh_text_outlines()`, `TextOutlines = Rc<dyn Fn(&Props) -> Option<BezPath>>`; `vector::uses_path_stroke` — true у Boolean, `vector::hit_path` — области у Boolean.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-scene/tests/boolean.rs`:

````rust
//! Булевы операции (подсистема 3, этап 4): операция над контурами — свойства и
//! точность на касаниях и общих рёбрах; результат булевой группы в сцене —
//! границы, попадание, пересчёт при правке и превью детей, вложенность,
//! скрытые дети, пустой результат, текст через контуры глифов.

use std::rc::Rc;
use std::sync::Arc;

use ferrum_doc::{BooleanOp, Document, FillRule, NodeId, NodeKind, PageId, Props, PropsPatch};
use ferrum_scene::boolean::{combine, flatten_path, normalize, Contours};
use ferrum_scene::{testing::snapshot, Scene};
use kurbo::{BezPath, Circle, Point, Rect, Shape};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Contours {
    normalize(
        flatten_path(&Rect::new(x0, y0, x1, y1).to_path(0.1), 0.01),
        FillRule::NonZero,
    )
}

fn area(c: &Contours) -> f64 {
    c.to_path().area().abs()
}

#[test]
fn ops_follow_figma_semantics_on_two_squares() {
    let a = rect(0.0, 0.0, 10.0, 10.0);
    let b = rect(5.0, 5.0, 15.0, 15.0);
    let ab = [a.clone(), b.clone()];
    assert!((area(&combine(BooleanOp::Union, &ab)) - 175.0).abs() < 1e-9);
    assert!((area(&combine(BooleanOp::Subtract, &ab)) - 75.0).abs() < 1e-9);
    assert!((area(&combine(BooleanOp::Intersect, &ab)) - 25.0).abs() < 1e-9);
    assert!((area(&combine(BooleanOp::Exclude, &ab)) - 150.0).abs() < 1e-9);
    // Вычитание — из нижнего: порядок важен.
    let ba = [b, a];
    assert!((area(&combine(BooleanOp::Subtract, &ba)) - 75.0).abs() < 1e-9);
    // Одна форма — она сама.
    let one = [rect(0.0, 0.0, 3.0, 4.0)];
    for op in BooleanOp::ALL {
        assert!((area(&combine(op, &one)) - 12.0).abs() < 1e-9);
    }
    assert!(combine(BooleanOp::Union, &[]).is_empty());
}

#[test]
fn touching_and_coincident_edges_are_exact() {
    let a = rect(0.0, 0.0, 10.0, 10.0);
    let right = rect(10.0, 0.0, 20.0, 10.0);
    // Встык — один контур, без щели и без шва.
    let u = combine(BooleanOp::Union, &[a.clone(), right.clone()]);
    assert_eq!(u.0.len(), 1);
    assert!((area(&u) - 200.0).abs() < 1e-9);
    assert!(combine(BooleanOp::Intersect, &[a.clone(), right]).is_empty());
    // Совпадающие — исключение и вычитание пусты, объединение — один квадрат.
    let same = a.clone();
    assert!(combine(BooleanOp::Exclude, &[a.clone(), same.clone()]).is_empty());
    assert!(combine(BooleanOp::Subtract, &[a.clone(), same.clone()]).is_empty());
    let u = combine(BooleanOp::Union, &[a.clone(), same]);
    assert_eq!(u.0.len(), 1);
    assert!((area(&u) - 100.0).abs() < 1e-9);
    // Касание углом — два контура.
    let corner = rect(10.0, 10.0, 20.0, 20.0);
    assert_eq!(combine(BooleanOp::Union, &[a.clone(), corner]).0.len(), 2);
    // Общая сторона при вычитании — ровно половина.
    let half = rect(0.0, 5.0, 10.0, 15.0);
    assert!((area(&combine(BooleanOp::Subtract, &[a, half])) - 50.0).abs() < 1e-9);
}

#[test]
fn shapes_are_normalized_by_their_own_rule_and_orientation() {
    // Звезда из пересекающихся отрезков: ненулевое правило заливает середину,
    // чётно-нечётное — нет; форма из обращённого контура — та же.
    let mut star = BezPath::new();
    let pts = (0..5).map(|i| {
        let a = std::f64::consts::TAU * (i as f64 * 2.0) / 5.0;
        Point::new(50.0 + 40.0 * a.sin(), 50.0 - 40.0 * a.cos())
    });
    for (i, p) in pts.enumerate() {
        if i == 0 {
            star.move_to(p);
        } else {
            star.line_to(p);
        }
    }
    star.close_path();
    let nz = normalize(flatten_path(&star, 0.01), FillRule::NonZero);
    let eo = normalize(flatten_path(&star, 0.01), FillRule::EvenOdd);
    assert!(area(&nz) > area(&eo) + 100.0);
    // Обращённый квадрат объединяется с прямым без дыры на пересечении.
    let mut rev = flatten_path(&Rect::new(5.0, 5.0, 15.0, 15.0).to_path(0.1), 0.01);
    rev[0].reverse();
    let rev = normalize(rev, FillRule::NonZero);
    let u = combine(BooleanOp::Union, &[rect(0.0, 0.0, 10.0, 10.0), rev]);
    assert!((area(&u) - 175.0).abs() < 1e-9);
    // Круг раздроблен с точностью: площадь — до долей процента.
    let c = normalize(
        flatten_path(&Circle::new((0.0, 0.0), 100.0).to_path(1e-4), 0.005),
        FillRule::NonZero,
    );
    let exact = std::f64::consts::PI * 1e4;
    assert!((area(&c) - exact).abs() / exact < 1e-4);
}

fn patch(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

/// Булева группа `op` в (100, 100) с прямоугольником 0..100 и эллипсом 50..150
/// по x. Возвращает [группа, прямоугольник, эллипс].
fn sample(op: BooleanOp) -> (Document, PageId, [NodeId; 3]) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |tx| {
            let b = tx.create_with_props(
                None,
                None,
                &Props {
                    x: 100.0,
                    y: 100.0,
                    boolean_op: Some(op),
                    ..Props::defaults(NodeKind::Boolean)
                },
            )?;
            let r = tx.create_node(Some(b), None, NodeKind::Rect)?;
            tx.set_props(r, &patch(0.0, 0.0, 100.0, 100.0))?;
            let e = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
            tx.set_props(e, &patch(50.0, 0.0, 100.0, 100.0))?;
            Ok(vec![b, r, e])
        })
        .unwrap();
    doc.take_changes(page);
    (doc, page, ids.try_into().unwrap())
}

fn local_bounds(scene: &Scene, id: NodeId) -> Rect {
    scene.node(id).unwrap().local_bounds.unwrap()
}

/// Совпадение рамок с точностью дробления кривых.
fn near(a: Rect, b: Rect) -> bool {
    let ok = [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
        .iter()
        .all(|d| d.abs() < 0.01);
    if !ok {
        eprintln!("{a:?} ≠ {b:?}");
    }
    ok
}

#[test]
fn result_gives_bounds_and_hits_and_children_are_hit_only_inside_it() {
    let (mut doc, page, [b, r, e]) = sample(BooleanOp::Subtract);
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Вычитание: прямоугольник без правой половины эллипса.
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 100.0, 100.0)
    ));
    assert_eq!(scene.hit_test(Point::new(110.0, 150.0), 0.0), Some(r));
    // Внутри эллипса (дыра) — мимо, хотя прямоугольник и эллипс там есть.
    assert_eq!(scene.hit_test(Point::new(190.0, 150.0), 0.0), None);
    assert_eq!(scene.hit_test(Point::new(240.0, 150.0), 0.0), None);
    let hits = scene.hit_test_all(Point::new(110.0, 150.0), 0.0);
    assert!(hits.contains(&b) && !hits.contains(&e), "{hits:?}");
    // Объединение: и эллипс справа.
    let (mut doc, page, [b, _, e]) = sample(BooleanOp::Union);
    let scene = Scene::from_page(doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 150.0, 100.0)
    ));
    assert_eq!(scene.hit_test(Point::new(240.0, 150.0), 0.0), Some(e));
    let shape = scene.node(b).unwrap().path_shape().unwrap().clone();
    assert!(shape.closed && shape.regions.len() == 1);
}

#[test]
fn child_edit_and_preview_recompute_the_result_live() {
    let (mut doc, page, [b, r, e]) = sample(BooleanOp::Intersect);
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(50.0, 0.0, 100.0, 100.0)
    ));
    let v0 = scene.node(b).unwrap().version;
    // Превью жеста ребёнка (перетаскивание) — результат уже в сцене.
    scene.set_preview(&[(
        e,
        PropsPatch {
            x: Some(80.0),
            ..Default::default()
        },
    )]);
    // Эллипс 80..180: у x = 100 он от 10 до 90 по y.
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(80.0, 10.0, 100.0, 90.0)
    ));
    assert_ne!(
        scene.node(b).unwrap().version,
        v0,
        "рендер перестроит список"
    );
    scene.clear_preview();
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(50.0, 0.0, 100.0, 100.0)
    ));
    // Правка документа: ребёнок сжался.
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            r,
            &PropsPatch {
                w: Some(70.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    scene.apply(&doc.take_changes(page), doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(50.0, 10.0, 70.0, 90.0)
    ));
    assert_eq!(
        snapshot(&scene),
        snapshot(&Scene::from_page(doc.page(page).unwrap()))
    );
    // Сдвиг самой группы результат не пересчитывает (тот же `Arc`).
    let before = scene.node(b).unwrap().boolean.clone().unwrap();
    scene.set_preview(&[(
        b,
        PropsPatch {
            x: Some(300.0),
            ..Default::default()
        },
    )]);
    let after = scene.node(b).unwrap().boolean.clone().unwrap();
    assert!(Arc::ptr_eq(&before, &after));
    // Смена операции — новый результат.
    scene.clear_preview();
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            b,
            &PropsPatch {
                boolean_op: Some(BooleanOp::Union),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    scene.apply(&doc.take_changes(page), doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 150.0, 100.0)
    ));
}

#[test]
fn hidden_children_lines_and_empty_results() {
    let (mut doc, page, [b, r, e]) = sample(BooleanOp::Union);
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            e,
            &PropsPatch {
                visible: Some(false),
                ..Default::default()
            },
        )?;
        // Линия формы заливки не имеет — в результат не входит.
        let l = tx.create_node(Some(b), None, NodeKind::Line)?;
        tx.set_props(l, &patch(0.0, 300.0, 500.0, 0.0))?;
        Ok(vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 100.0, 100.0)
    ));
    // Пустое пересечение: ничего не рисуется и не ловит щелчок, рамка — по детям.
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            e,
            &PropsPatch {
                visible: Some(true),
                x: Some(500.0),
                ..Default::default()
            },
        )?;
        tx.set_props(
            b,
            &PropsPatch {
                boolean_op: Some(BooleanOp::Intersect),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    let n = scene.node(b).unwrap();
    assert!(n.boolean.as_ref().unwrap().contours.is_empty());
    assert!(near(
        n.local_bounds.unwrap(),
        Rect::new(0.0, 0.0, 600.0, 300.0)
    ));
    assert_eq!(scene.hit_test(Point::new(150.0, 150.0), 0.0), None);
    let _ = r;
}

#[test]
fn nested_booleans_groups_and_flipped_children() {
    let (mut doc, page, [b, _, _]) = sample(BooleanOp::Union);
    let inner = doc
        .transact(page, vec![], |tx| {
            // Вложенная «Вычитание»: квадрат 200..300 без 250..300 по x.
            let s = tx.create_with_props(
                Some(b),
                None,
                &Props {
                    x: 200.0,
                    boolean_op: Some(BooleanOp::Subtract),
                    ..Props::defaults(NodeKind::Boolean)
                },
            )?;
            let base = tx.create_node(Some(s), None, NodeKind::Rect)?;
            tx.set_props(base, &patch(0.0, 0.0, 100.0, 100.0))?;
            let cut = tx.create_node(Some(s), None, NodeKind::Rect)?;
            tx.set_props(cut, &patch(50.0, 0.0, 100.0, 100.0))?;
            // Группа с отражённым прямоугольником: объединяется без дыр.
            let g = tx.create_node(Some(b), None, NodeKind::Group)?;
            let f = tx.create_node(Some(g), None, NodeKind::Rect)?;
            tx.set_props(
                f,
                &PropsPatch {
                    flip_x: Some(true),
                    ..patch(155.0, 0.0, 30.0, 100.0)
                },
            )?;
            Ok(vec![s])
        })
        .unwrap()[0];
    let scene = Scene::from_page(doc.page(page).unwrap());
    assert!(near(
        local_bounds(&scene, inner),
        Rect::new(0.0, 0.0, 50.0, 100.0)
    ));
    let outer = &scene.node(b).unwrap().boolean.as_ref().unwrap().contours;
    // 0..150 (прямоугольник и эллипс) ∪ 155..185 ∪ 200..250: три куска.
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 250.0, 100.0)
    ));
    let total = area(outer);
    let ellipse_right = std::f64::consts::PI * 50.0 * 50.0 / 2.0;
    let expected = 100.0 * 100.0 + ellipse_right + 30.0 * 100.0 + 50.0 * 100.0;
    assert!((total - expected).abs() < 5.0, "{total} ≈ {expected}");
}

#[test]
fn text_uses_glyph_outlines_from_the_editor_and_refreshes() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let [b, _t]: [NodeId; 2] = doc
        .transact(page, vec![], |tx| {
            let b = tx.create_node(None, None, NodeKind::Boolean)?;
            let t = tx.create_node(Some(b), None, NodeKind::Text)?;
            tx.set_props(t, &patch(0.0, 0.0, 40.0, 20.0))?;
            Ok(vec![b, t])
        })
        .unwrap()
        .try_into()
        .unwrap();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    // Без источника контуров текст формы не даёт.
    assert!(scene
        .node(b)
        .unwrap()
        .boolean
        .as_ref()
        .unwrap()
        .contours
        .is_empty());
    let size = Rc::new(std::cell::Cell::new(10.0));
    let s = size.clone();
    scene.set_text_outlines(Some(Rc::new(move |_p: &Props| {
        Some(Rect::new(0.0, 0.0, s.get(), 10.0).to_path(0.1))
    })));
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 10.0, 10.0)
    ));
    // Шрифт пришёл — контуры другие, группа пересчитана.
    size.set(30.0);
    let counter = scene.change_counter();
    scene.refresh_text_outlines();
    assert!(near(
        local_bounds(&scene, b),
        Rect::new(0.0, 0.0, 30.0, 10.0)
    ));
    assert!(scene.change_counter() > counter, "нужен новый кадр");
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-scene --test boolean`
Expected: FAIL — не собирается: нет модуля `ferrum_scene::boolean`, поля `SceneNode::boolean`, `Scene::set_text_outlines` и `Scene::refresh_text_outlines`

- [ ] **Step 3: Реализация**

В `Cargo.lock` заменить:

````
dependencies = [
 "ferrum-doc",
 "kurbo",
 "proptest",
````

на:

````
dependencies = [
 "ferrum-doc",
 "i_overlay",
 "kurbo",
 "proptest",
````

В `Cargo.lock` заменить:

````
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "2304e00983f87ffb38b55b444b5e3b60a884b5d30c0fca7d82fe33449bbe55ea"

[[package]]
````

на:

````
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "2304e00983f87ffb38b55b444b5e3b60a884b5d30c0fca7d82fe33449bbe55ea"

[[package]]
name = "i_float"
version = "5.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "9ecaff1ce29bcacd72cdf581ba6fce636ef4c4e0b2da7e7846dd0137d98141e2"
dependencies = [
 "libm",
]

[[package]]
name = "i_key_sort"
version = "0.12.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "14269ee362009550495e20650ff3c2bca14a8147fe251e216760381ffed90b6f"

[[package]]
name = "i_overlay"
version = "9.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "913fd692c02408adc76cead636f8e7e07c705f8b995a8b54b454b995e8e197d5"
dependencies = [
 "i_float",
 "i_key_sort",
 "i_shape",
 "i_tree",
]

[[package]]
name = "i_shape"
version = "5.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "fa1878f91ef7921feaf1b2346253ebc5e661878b22d5715195c1d5dcf0f20ae5"
dependencies = [
 "i_float",
]

[[package]]
name = "i_tree"
version = "0.20.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "048af31de955890c62a8506fec511335a5089e91e1c516537add16f96c507d84"

[[package]]
````

В `Cargo.toml` заменить:

````toml
# (листья на разной глубине, georust/rstar#254 ещё не выпущен).
rstar = "=0.12.2"
# Рендер: vello_cpu сейчас, vello_gpu (бывший vello_hybrid) — на этапе 4. Без
# текстовых возможностей vello: глифы рисуются контурами из skrifa (см. text.rs).
````

на:

````toml
# (листья на разной глубине, georust/rstar#254 ещё не выпущен).
rstar = "=0.12.2"
# Булевы операции (спека подсистемы 3, раздел 10): по замеру прототипа этапа 4
# быстрее `linesweeper` на перекрытиях (1000 кругов — ~20 мс против 70–90 мс), на
# касаниях и общих рёбрах — то же. Без `allow_multithreading` (rayon в wasm нет).
i_overlay = { version = "=9.1.0", default-features = false }
# Рендер: vello_cpu сейчас, vello_gpu (бывший vello_hybrid) — на этапе 4. Без
# текстовых возможностей vello: глифы рисуются контурами из skrifa (см. text.rs).
````

В `crates/ferrum-scene/Cargo.toml` заменить:

````toml
ferrum-doc = { path = "../ferrum-doc" }
kurbo.workspace = true
rstar.workspace = true

````

на:

````toml
ferrum-doc = { path = "../ferrum-doc" }
kurbo.workspace = true
i_overlay.workspace = true
rstar.workspace = true

````

Создать `crates/ferrum-scene/src/boolean.rs`:

````rust
//! Булевы операции (спека подсистемы 3, раздел 10): форма заливки узла как набор
//! контуров-ломаных и операция над формами детей булевой группы.
//!
//! Библиотека — `i_overlay` по раздроблённым кривым (выбор замером в плане
//! этапа 4): на 1000 перекрывающихся кругах она укладывается в ~20 мс, а
//! `linesweeper` (операции над кривыми без дробления) — 70–90 мс, больше
//! предела 50 мс; на касаниях и совпадающих рёбрах обе дают одно и то же.
//! Кривые дробятся с точностью `BOOLEAN_TOLERANCE` в координатах узла, поэтому
//! результат — ломаные (кривые сохраняются только у «Контура обводки»).
//!
//! Каждая форма сначала приводится к простым контурам своим правилом заливки
//! (внешние — по одну сторону, дыры — по другую), поэтому число перекрытий в
//! точке равно числу форм, её покрывающих: объединение — хотя бы одна,
//! пересечение — все, исключение — нечётное число, вычитание — нижняя без
//! остальных (как в Figma).

use ferrum_doc::{BooleanOp, FillRule};
use i_overlay::core::fill_rule::FillRule as IFillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::overlay::FloatOverlay;
use kurbo::{Affine, BezPath, PathEl, Point, Rect};

/// Точность дробления кривых в координатах узла: 0,005 — четверть пикселя
/// при приближении ×50; круг радиуса 100 — около 300 отрезков.
pub const BOOLEAN_TOLERANCE: f64 = 0.005;

/// Контур — замкнутая ломаная (последняя точка не повторяет первую).
pub type Contour = Vec<[f64; 2]>;

/// Форма заливки: простые контуры без самопересечений, внешние и дыры с
/// противоположным обходом (правило заливки для них не важно).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Contours(pub Vec<Contour>);

impl Contours {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Число точек всех контуров.
    pub fn points(&self) -> usize {
        self.0.iter().map(Vec::len).sum()
    }

    /// Форма под аффинным преобразованием. Отражение обратило бы обход
    /// контуров, и форма в объединении с другими вычиталась бы из них: обход
    /// тогда возвращается разворотом каждого контура.
    pub fn transformed(&self, a: Affine) -> Contours {
        if a == Affine::IDENTITY {
            return self.clone();
        }
        let flip = a.determinant() < 0.0;
        Contours(
            self.0
                .iter()
                .map(|c| {
                    let mut out: Contour = c
                        .iter()
                        .map(|p| {
                            let q = a * Point::new(p[0], p[1]);
                            [q.x, q.y]
                        })
                        .collect();
                    if flip {
                        out.reverse();
                    }
                    out
                })
                .collect(),
        )
    }

    /// Контуры как путь (подпути замкнуты).
    pub fn to_path(&self) -> BezPath {
        let mut out = BezPath::new();
        for c in &self.0 {
            let Some((first, rest)) = c.split_first() else {
                continue;
            };
            out.move_to((first[0], first[1]));
            for p in rest {
                out.line_to((p[0], p[1]));
            }
            out.close_path();
        }
        out
    }

    /// Рамка всех точек; `None` — форма пуста.
    pub fn bounds(&self) -> Option<Rect> {
        self.0
            .iter()
            .flatten()
            .map(|p| Rect::from_points((p[0], p[1]), (p[0], p[1])))
            .reduce(|a, b| a.union(b))
    }
}

/// Контуры пути: кривые дробятся с точностью `tolerance`, открытые подпути
/// замыкаются (как заливка в рендере), вырожденные (меньше трёх точек и
/// нечисловые) пропускаются.
pub fn flatten_path(path: &BezPath, tolerance: f64) -> Vec<Contour> {
    let mut out: Vec<Contour> = Vec::new();
    let mut cur: Contour = Vec::new();
    let finish = |cur: &mut Contour, out: &mut Vec<Contour>| {
        if cur.len() > 1 && cur.first() == cur.last() {
            cur.pop();
        }
        let ok = cur.len() >= 3 && cur.iter().all(|p| p[0].is_finite() && p[1].is_finite());
        if ok {
            out.push(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    kurbo::flatten(path.iter(), tolerance, |el| match el {
        PathEl::MoveTo(p) => {
            finish(&mut cur, &mut out);
            cur.push([p.x, p.y]);
        }
        PathEl::LineTo(p) => cur.push([p.x, p.y]),
        PathEl::ClosePath => finish(&mut cur, &mut out),
        // `flatten` отдаёт только отрезки.
        _ => {}
    });
    finish(&mut cur, &mut out);
    out
}

fn ifill(rule: FillRule) -> IFillRule {
    match rule {
        FillRule::NonZero => IFillRule::NonZero,
        FillRule::EvenOdd => IFillRule::EvenOdd,
    }
}

fn collect(shapes: Vec<Vec<Contour>>) -> Contours {
    Contours(shapes.into_iter().flatten().collect())
}

/// Приводит контуры с правилом `rule` к простым (`Contours`). Один контур
/// без самопересечений (прямоугольник, эллипс) возвращается как есть, только с
/// обходом, общим у всех простых форм, — без запуска операции.
pub fn normalize(contours: Vec<Contour>, rule: FillRule) -> Contours {
    if contours.is_empty() {
        return Contours::default();
    }
    if let [one] = &contours[..] {
        if is_simple_convex(one) {
            let mut c = one.clone();
            if signed_area(&c) < 0.0 {
                c.reverse();
            }
            return Contours(vec![c]);
        }
    }
    collect(FloatOverlay::with_subj(&contours).overlay(OverlayRule::Subject, ifill(rule)))
}

/// Удвоенная площадь со знаком (положительная — обход по часовой на экране с
/// осью Y вниз).
fn signed_area(c: &[[f64; 2]]) -> f64 {
    let n = c.len();
    (0..n)
        .map(|i| {
            let (p, q) = (c[i], c[(i + 1) % n]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum()
}

/// Выпуклый контур без самопересечений: все повороты в одну сторону и обход
/// — один оборот. Такой форме операция приведения не нужна.
fn is_simple_convex(c: &[[f64; 2]]) -> bool {
    let n = c.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0f64;
    let mut turn = 0.0f64;
    for i in 0..n {
        let (a, b, d) = (c[i], c[(i + 1) % n], c[(i + 2) % n]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1]], [d[0] - b[0], d[1] - b[1]]);
        let cross = u[0] * v[1] - u[1] * v[0];
        if cross.abs() > 1e-12 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
        turn += cross.atan2(u[0] * v[0] + u[1] * v[1]);
    }
    (turn.abs() - std::f64::consts::TAU).abs() < 1e-6
}

/// Операция над формами в порядке слоёв снизу вверх. Пустые формы (линии,
/// открытые пути, пустые группы) уже отброшены вызывающим: основа вычитания —
/// нижняя непустая.
pub fn combine(op: BooleanOp, inputs: &[Contours]) -> Contours {
    let all = |from: usize| -> Vec<Contour> {
        inputs[from..]
            .iter()
            .flat_map(|c| c.0.iter().cloned())
            .collect()
    };
    match (op, inputs) {
        (_, []) => Contours::default(),
        // Одна форма: объединение, исключение и пересечение — она сама.
        (BooleanOp::Subtract, [one]) | (_, [one]) => one.clone(),
        (BooleanOp::Union, _) => collect(
            FloatOverlay::with_subj(&all(0)).overlay(OverlayRule::Subject, IFillRule::NonZero),
        ),
        (BooleanOp::Exclude, _) => collect(
            FloatOverlay::with_subj(&all(0)).overlay(OverlayRule::Subject, IFillRule::EvenOdd),
        ),
        (BooleanOp::Subtract, [base, ..]) => collect(
            FloatOverlay::with_subj_and_clip(&base.0, &all(1))
                .overlay(OverlayRule::Difference, IFillRule::NonZero),
        ),
        (BooleanOp::Intersect, [first, rest @ ..]) => {
            let mut acc = first.clone();
            for next in rest {
                if acc.is_empty() {
                    break;
                }
                acc = collect(
                    FloatOverlay::with_subj_and_clip(&acc.0, &next.0)
                        .overlay(OverlayRule::Intersect, IFillRule::NonZero),
                );
            }
            acc
        }
    }
}

/// Форма заливки узла-листа в его локальных координатах (без детей): фигура —
/// её контур (у фрейма и изображения — прямоугольник), вектор — его области
/// каждая своим правилом (`path` — контуры узла, `vector::path_shape`), текст —
/// контуры глифов `text` (правило ненулевое). У линии, открытого пути без
/// областей и группы формы нет. Обводки детей в булеву операцию не входят (как
/// в Figma).
pub fn leaf_fill(
    p: &ferrum_doc::Props,
    path: Option<&crate::vector::PathShape>,
    text: Option<&BezPath>,
) -> Contours {
    use crate::shape::{geometry, Geometry};
    use ferrum_doc::NodeKind;
    match p.kind {
        NodeKind::Vector => {
            let Some(shape) = path else {
                return Contours::default();
            };
            let parts: Vec<Contours> = shape
                .regions
                .iter()
                .map(|r| normalize(flatten_path(&r.path, BOOLEAN_TOLERANCE), r.rule))
                .filter(|c| !c.is_empty())
                .collect();
            combine(BooleanOp::Union, &parts)
        }
        NodeKind::Text => match text {
            Some(t) => normalize(flatten_path(t, BOOLEAN_TOLERANCE), FillRule::NonZero),
            None => Contours::default(),
        },
        _ => match geometry(p) {
            g @ (Geometry::Rect { .. } | Geometry::Ellipse(_)) => {
                let path = g.fill_path().unwrap_or_default();
                normalize(flatten_path(&path, BOOLEAN_TOLERANCE), FillRule::NonZero)
            }
            _ => Contours::default(),
        },
    }
}

/// Результат булевой группы для рендера и попадания: контуры и форма для
/// обводки по контуру (одна область — сам результат, путь замкнут).
pub fn outline_shape(c: &Contours) -> crate::vector::PathShape {
    let path = c.to_path();
    crate::vector::PathShape {
        stroke: path.clone(),
        regions: vec![crate::vector::RegionPath {
            path,
            rule: FillRule::NonZero,
            fills: None,
        }],
        ends: vec![],
        junctions: vec![],
        closed: true,
    }
}
````

В `crates/ferrum-scene/src/lib.rs` заменить:

````rust
//! не меняет и видит только через публичный API `ferrum-doc`.

mod scene;
pub mod shape;
pub mod vector;

pub use scene::{testing, Draft, Scene, SceneNode, Slot};
````

на:

````rust
//! не меняет и видит только через публичный API `ferrum-doc`.

pub mod boolean;
mod scene;
pub mod shape;
pub mod vector;

pub use scene::{testing, BooleanOutline, Draft, Scene, SceneNode, Slot, TextOutlines};
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use ferrum_doc::{Changes, NodeId, NodeKind, PageDoc, Props, PropsPatch, SceneRow};
use kurbo::{Affine, Point, Rect};
use rstar::primitives::{GeomWithData, Rectangle};
use rstar::{RStarInsertionStrategy, RTree, RTreeParams, SelectionFunction, AABB};

use crate::shape::{self, Geometry};
use crate::vector::{self, PathShape};
````

на:

````rust

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use ferrum_doc::{Changes, NodeId, NodeKind, PageDoc, Props, PropsPatch, SceneRow};
use kurbo::{Affine, BezPath, Point, Rect};
use rstar::primitives::{GeomWithData, Rectangle};
use rstar::{RStarInsertionStrategy, RTree, RTreeParams, SelectionFunction, AABB};

use crate::boolean::{self as boolean, Contours};
use crate::shape::{self, Geometry};
use crate::vector::{self, PathShape};
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
    /// строило. Сбрасываются вместе с `version`.
    path: OnceLock<Option<Arc<PathShape>>>,
}

````

на:

````rust
    /// строило. Сбрасываются вместе с `version`.
    path: OnceLock<Option<Arc<PathShape>>>,
    /// Форма заливки узла-листа для булевой группы-предка (`boolean::leaf_fill`),
    /// построенная при первом запросе; сбрасывается вместе с `version`. Правка
    /// одного ребёнка большой булевой группы не дробит кривые остальных заново.
    fill: OnceLock<Arc<Contours>>,
    /// Результат булевой группы (у остальных узлов — `None`).
    pub boolean: Option<Arc<BooleanOutline>>,
    /// Ключ содержимого: версия листа или (у группы и булевой группы) тип,
    /// операция и ключи, положения и видимость детей. Результат булевой группы пересчитывается, только когда
    /// ключ сменился: сдвиг самой группы его не меняет.
    content: u64,
}

/// Результат булевой группы в её локальных координатах.
#[derive(Debug, Clone, PartialEq)]
pub struct BooleanOutline {
    pub contours: Contours,
    /// Форма для рендера и попадания: одна область и замкнутый путь обводки.
    pub shape: Arc<PathShape>,
}

````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust

    /// Контуры обводки по контуру — из кэша узла (`None` — узел обводится
    /// кольцами).
    pub fn path_shape(&self) -> Option<&Arc<PathShape>> {
        self.path
            .get_or_init(|| vector::path_shape(&self.props).map(Arc::new))
````

на:

````rust

    /// Контуры обводки по контуру — из кэша узла (`None` — узел обводится
    /// кольцами). У булевой группы — её результат.
    pub fn path_shape(&self) -> Option<&Arc<PathShape>> {
        if self.props.kind == NodeKind::Boolean {
            return self.boolean.as_ref().map(|b| &b.shape);
        }
        self.path
            .get_or_init(|| vector::path_shape(&self.props).map(Arc::new))
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
    }
}

/// Временный узел жеста, которого нет в документе: фигура, которую рисуют, или копия
````

на:

````rust
    }
}

/// Контуры глифов текстового узла в его локальных координатах для булевых групп
/// (сцена шрифтов не знает — их даёт редактор); `None` — шрифты ещё грузятся.
pub type TextOutlines = Rc<dyn Fn(&Props) -> Option<BezPath>>;

/// Временный узел жеста, которого нет в документе: фигура, которую рисуют, или копия
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
    /// каждое событие указателя и на каждого кандидата привязки.
    draft_set: HashSet<Slot>,
}

````

на:

````rust
    /// каждое событие указателя и на каждого кандидата привязки.
    draft_set: HashSet<Slot>,
    /// Контуры глифов для текста в булевых группах (`set_text_outlines`).
    text_outlines: Option<TextOutlines>,
    /// Растёт, когда меняются шрифты (`refresh_text_outlines`): входит в ключ
    /// содержимого текста, и булевы группы с текстом пересчитываются.
    text_epoch: u64,
}

````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
            subtree_version: version,
            path: OnceLock::new(),
        };
        let slot = match self.free.pop() {
````

на:

````rust
            subtree_version: version,
            path: OnceLock::new(),
            fill: OnceLock::new(),
            boolean: None,
            content: 0,
        };
        let slot = match self.free.pop() {
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
    /// строится пакетно).
    fn update_bounds(&mut self, slot: Slot, update_tree: bool) {
        let n = self.at(slot);
        let geom = n.geometry();
        let local_bounds = match geom {
            Geometry::None => n
                .children
````

на:

````rust
    /// строится пакетно).
    fn update_bounds(&mut self, slot: Slot, update_tree: bool) {
        self.update_content(slot);
        let n = self.at(slot);
        let geom = n.geometry();
        let result = n
            .boolean
            .as_ref()
            .and_then(|b| b.contours.bounds())
            .filter(|_| n.props.kind == NodeKind::Boolean);
        let local_bounds = match geom {
            // Булева группа — по результату; пустой результат — по детям, чтобы
            // рамка выделения показывала, где группа.
            Geometry::None if result.is_some() => result,
            Geometry::None => n
                .children
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
                self.tree_inserts.push((slot, b));
            }
        }
    }

    /// Применяет накопленные изменения R-tree. Удаление по одной записи стоит
````

на:

````rust
                self.tree_inserts.push((slot, b));
            }
        }
    }

    /// Ключ содержимого узла и, у булевой группы со сменившимся ключом, её
    /// результат. Дети уже посчитаны (обход снизу вверх).
    fn update_content(&mut self, slot: Slot) {
        let n = self.at(slot);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        // У контейнера — тип и операция: его версию меняют и свои заливки, а
        // результат булевой группы сам меняет её версию (иначе ключ не сошёлся
        // бы ни разу).
        if n.props.kind.is_group_like() {
            n.props.kind.hash(&mut h);
            n.props.boolean_op.hash(&mut h);
        } else {
            n.version.hash(&mut h);
        }
        if n.props.kind == NodeKind::Text {
            self.text_epoch.hash(&mut h);
        }
        // Дети — только у группы и булевой группы: форма фрейма для булевой
        // группы — его рамка, а перебор 100 000 детей фрейма на каждую правку
        // ребёнка стоил бы миллисекунды.
        let children: &[Slot] = if n.props.kind.is_group_like() {
            &n.children
        } else {
            &[]
        };
        for &c in children {
            let c = self.at(c);
            c.content.hash(&mut h);
            c.props.visible.hash(&mut h);
            for v in c.local.as_coeffs() {
                v.to_bits().hash(&mut h);
            }
        }
        let key = h.finish();
        if key == n.content && (n.boolean.is_some() || n.props.kind != NodeKind::Boolean) {
            return;
        }
        let outline = (n.props.kind == NodeKind::Boolean).then(|| {
            let op = n.props.boolean_op.unwrap_or_default();
            let inputs: Vec<Contours> = n
                .children
                .iter()
                .filter(|&&c| self.at(c).props.visible)
                .map(|&c| self.fill_of(c).transformed(self.at(c).local))
                .filter(|c| !c.is_empty())
                .collect();
            let contours = boolean::combine(op, &inputs);
            let shape = Arc::new(boolean::outline_shape(&contours));
            Arc::new(BooleanOutline { contours, shape })
        });
        let n = self.at_mut(slot);
        n.content = key;
        if let Some(o) = outline {
            if n.boolean.as_ref() != Some(&o) {
                // Другой результат — другой вид: рендер строит список заново.
                n.version = fresh_version();
            }
            n.boolean = Some(o);
        }
    }

    /// Форма заливки узла с видимыми детьми в его локальных координатах: у
    /// булевой группы — её результат, у группы — объединение детей, у листа —
    /// `boolean::leaf_fill` (у текста — по контурам глифов редактора).
    fn fill_of(&self, slot: Slot) -> Contours {
        let n = self.at(slot);
        match n.props.kind {
            NodeKind::Boolean => n
                .boolean
                .as_ref()
                .map(|b| b.contours.clone())
                .unwrap_or_default(),
            NodeKind::Group => {
                let parts: Vec<Contours> = n
                    .children
                    .iter()
                    .filter(|&&c| self.at(c).props.visible)
                    .map(|&c| self.fill_of(c).transformed(self.at(c).local))
                    .filter(|c| !c.is_empty())
                    .collect();
                boolean::combine(ferrum_doc::BooleanOp::Union, &parts)
            }
            NodeKind::Text => {
                let glyphs = self.text_outlines.as_ref().and_then(|f| f(&n.props));
                boolean::leaf_fill(&n.props, None, glyphs.as_ref())
            }
            _ => (**n.fill.get_or_init(|| {
                Arc::new(boolean::leaf_fill(
                    &n.props,
                    n.path_shape().map(|s| &**s),
                    None,
                ))
            }))
            .clone(),
        }
    }

    /// Источник контуров глифов для текста в булевых группах (редактор ставит
    /// его на каждую новую сцену). Булевы группы с текстом пересчитываются.
    pub fn set_text_outlines(&mut self, f: Option<TextOutlines>) {
        self.text_outlines = f;
        self.refresh_text_outlines();
    }

    /// Шрифты сменились (пришли байты, отказ в загрузке): булевы группы с
    /// текстом пересчитываются с новыми контурами глифов. Без таких групп —
    /// ничего не делает.
    pub fn refresh_text_outlines(&mut self) {
        let dirty: Vec<Slot> = self
            .iter()
            .filter(|(_, n)| n.props.kind == NodeKind::Text)
            .filter(|(s, _)| self.has_boolean_ancestor(*s))
            .map(|(s, _)| s)
            .collect();
        if dirty.is_empty() {
            return;
        }
        self.text_epoch += 1;
        self.changes += 1;
        self.recompute(dirty, Vec::new());
        self.flush_tree();
    }

    fn has_boolean_ancestor(&self, slot: Slot) -> bool {
        self.boolean_root(slot).is_some()
    }

    /// Самая внешняя булева группа среди предков узла.
    fn boolean_root(&self, slot: Slot) -> Option<Slot> {
        let mut cur = self.at(slot).parent;
        let mut found = None;
        while let Some(p) = cur {
            if self.at(p).props.kind == NodeKind::Boolean {
                found = Some(p);
            }
            cur = self.at(p).parent;
        }
        found
    }

    /// Применяет накопленные изменения R-tree. Удаление по одной записи стоит
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
                .map(|(&s, (_, patch))| (self.at(s).id, patch.clone()))
                .collect();
            *self = Scene::from_page(page);
            // Ревизия текстов, как и счётчик, не откатывается: пересборка — смена.
            self.text_revision += text_revision + 1;
            self.set_preview(&preview);
        }
````

на:

````rust
                .map(|(&s, (_, patch))| (self.at(s).id, patch.clone()))
                .collect();
            let outlines = self.text_outlines.take();
            *self = Scene::from_page(page);
            // Ревизия текстов, как и счётчик, не откатывается: пересборка — смена.
            self.text_revision += text_revision + 1;
            if outlines.is_some() {
                self.set_text_outlines(outlines);
            }
            self.set_preview(&preview);
        }
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
            n.version = version;
            n.path = OnceLock::new();
        }
        // Тот же `Arc` (правка нетекстовых свойств) — сравнение бесплатно.
````

на:

````rust
            n.version = version;
            n.path = OnceLock::new();
            n.fill = OnceLock::new();
        }
        // Тот же `Arc` (правка нетекстовых свойств) — сравнение бесплатно.
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust

    fn hits(&self, slot: Slot, point: Point, tolerance: f64) -> bool {
        let n = self.at(slot);
        if n.props.kind == NodeKind::Group || !self.is_visible(slot) {
            return false;
        }
````

на:

````rust

    fn hits(&self, slot: Slot, point: Point, tolerance: f64) -> bool {
        if !self.is_visible(slot) || !self.hits_own(slot, point, tolerance) {
            return false;
        }
        // Дети булевой группы не рисуются: в них попадают только там, где
        // рисуется сама внешняя группа (дыра «Вычитания» — мимо).
        match self.boolean_root(slot) {
            Some(b) => self.hits_own(b, point, tolerance),
            None => true,
        }
    }

    /// Попадание в собственную форму узла (без проверки видимости).
    fn hits_own(&self, slot: Slot, point: Point, tolerance: f64) -> bool {
        let n = self.at(slot);
        if n.props.kind == NodeKind::Group {
            return false;
        }
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
    let rect = Rect::new(0.0, 0.0, w, h);
    match p.kind {
        // Булева группа пока — как группа: результат операции — следующая задача.
        NodeKind::Group | NodeKind::Boolean => Geometry::None,
        NodeKind::Frame | NodeKind::Rect | NodeKind::Image => Geometry::Rect {
````

на:

````rust
    let rect = Rect::new(0.0, 0.0, w, h);
    match p.kind {
        // Булева группа своей рамки не имеет, как группа: её форма — результат
        // операции над детьми (`SceneNode::boolean`).
        NodeKind::Group | NodeKind::Boolean => Geometry::None,
        NodeKind::Frame | NodeKind::Rect | NodeKind::Image => Geometry::Rect {
````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
    let s = &p.stroke_style;
    match p.kind {
        NodeKind::Vector => true,
        NodeKind::Line => s.cap_start != StrokeCap::None || s.cap_end != StrokeCap::None,
        // Углы есть только у прямоугольника без скругления.
````

на:

````rust
    let s = &p.stroke_style;
    match p.kind {
        // Результат булевой группы — замкнутый путь, обводится как вектор.
        NodeKind::Vector | NodeKind::Boolean => true,
        NodeKind::Line => s.cap_start != StrokeCap::None || s.cap_end != StrokeCap::None,
        // Углы есть только у прямоугольника без скругления.
````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
/// прямоугольника. `None` — узел обводится кольцами.
pub fn path_shape(p: &Props) -> Option<PathShape> {
    if !uses_path_stroke(p) {
        return None;
    }
````

на:

````rust
/// прямоугольника. `None` — узел обводится кольцами.
pub fn path_shape(p: &Props) -> Option<PathShape> {
    // Форму булевой группы по одним свойствам не построить: её считает сцена
    // (`SceneNode::path_shape`).
    if !uses_path_stroke(p) || p.kind == NodeKind::Boolean {
        return None;
    }
````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
pub fn hit_path(p: &Props, shape: &PathShape, local: Point, tolerance: f64) -> bool {
    let node_filled = p.fills.iter().any(|f| f.visible && f.opacity > 0.0);
    if p.kind == NodeKind::Vector {
        for r in &shape.regions {
            let filled = match &r.fills {
````

на:

````rust
pub fn hit_path(p: &Props, shape: &PathShape, local: Point, tolerance: f64) -> bool {
    let node_filled = p.fills.iter().any(|f| f.visible && f.opacity > 0.0);
    if matches!(p.kind, NodeKind::Vector | NodeKind::Boolean) {
        for r in &shape.regions {
            let filled = match &r.fills {
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-scene -p ferrum-render 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: clippy wasm32 без замечаний (`i_overlay` собирается под wasm); `boolean` сцены — 8 тестов, тесты рендера — без изменений

- [ ] **Step 5: Закоммитить**

```bash
git add Cargo.lock Cargo.toml crates/ferrum-scene/Cargo.toml crates/ferrum-scene/src/boolean.rs crates/ferrum-scene/src/lib.rs crates/ferrum-scene/src/scene.rs crates/ferrum-scene/src/shape.rs crates/ferrum-scene/src/vector.rs crates/ferrum-scene/tests/boolean.rs
git commit -m "feat(scene): результат булевой группы — i_overlay по раздроблённым кривым, кэш по содержимому детей, границы и попадание по результату, текст контурами глифов редактора

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3: Рендер булевой группы

Спека 10: результат рисуется заливками и обводками самого узла. Здесь — булева группа рисуется как путь узла (обводка — по контуру результата), дети не рисуются (`subtree_bounds` группы — её `world_bounds`, растровый кэш и экспорт PNG тоже без детей), подсветка наведения и выделения — контуром результата (пустой — рамкой); эталоны.

**Files:**
- Modify: `crates/ferrum-render/src/{display,draw,overlay}.rs`, `crates/ferrum-scene/src/scene.rs`
- Test: `crates/ferrum-render/tests/boolean.rs` (новый); эталоны `crates/ferrum-render/tests/golden/boolean_{ops,nested}.png`

**Interfaces:**
- Consumes: `SceneNode::boolean`, `SceneNode::path_shape`, `vector::uses_path_stroke`.
- Produces: эталоны `boolean_ops.png`, `boolean_nested.png`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-render/tests/boolean.rs`:

````rust
//! Рендер булевых групп (подсистема 3, этап 4): результат каждой операции
//! заливками и обводкой самой группы, дети не рисуются, вложенная группа,
//! правка ребёнка перестраивает картинку, подсветка — по контуру результата.
//! Пересоздать эталоны: `FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test boolean`.

mod common;

use common::*;
use ferrum_doc::*;

fn solid(r: u8, g: u8, b: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, 255))])
}

/// Булева группа `op` в `(x, y)`: красный квадрат 0..60 и синий эллипс 30..90
/// по x (дети своих цветов не показывают). Группа — зелёная с чёрной обводкой 2.
fn add_boolean(p: &mut Page, op: BooleanOp, x: f64, y: f64) -> (NodeId, NodeId) {
    let b = p.add(
        None,
        NodeKind::Boolean,
        PropsPatch {
            x: Some(x),
            y: Some(y),
            boolean_op: Some(op),
            fills: solid(0, 160, 0),
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 2.0,
                align: StrokeAlign::Center,
            }]),
            ..Default::default()
        },
    );
    p.add(
        Some(b),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0),
            ..rect(0.0, 0.0, 60.0, 60.0)
        },
    );
    let e = p.add(
        Some(b),
        NodeKind::Ellipse,
        PropsPatch {
            fills: solid(0, 0, 255),
            ..rect(30.0, 0.0, 60.0, 60.0)
        },
    );
    (b, e)
}

const GREEN: [u8; 3] = [0, 160, 0];
const WHITE: [u8; 3] = [255, 255, 255];

#[test]
fn every_op_is_drawn_with_the_group_paint() {
    let mut p = Page::new();
    for (i, op) in BooleanOp::ALL.into_iter().enumerate() {
        add_boolean(&mut p, op, 10.0 + 110.0 * i as f64, 10.0);
    }
    let img = render(&p.scene(), &camera(450.0, 80.0));
    // Объединение: квадрат и правая половина эллипса — зелёные.
    assert_px(&img, 15, 40, GREEN, 0);
    assert_px(&img, 95, 40, GREEN, 0);
    // Вычитание: левая часть квадрата есть, на месте эллипса — пусто.
    assert_px(&img, 125, 40, GREEN, 0);
    assert_px(&img, 175, 40, WHITE, 0);
    assert_px(&img, 205, 40, WHITE, 0);
    // Пересечение: только линза посередине.
    assert_px(&img, 235, 40, WHITE, 0);
    assert_px(&img, 285, 40, GREEN, 0);
    assert_px(&img, 315, 40, WHITE, 0);
    // Исключение: края есть, середина пуста.
    assert_px(&img, 345, 40, GREEN, 0);
    assert_px(&img, 395, 40, WHITE, 0);
    assert_px(&img, 425, 40, GREEN, 0);
    // Обводка — по контуру результата: граница вычитания посередине квадрата.
    assert_px(&img, 150, 40, [0, 0, 0], 90);
    // Ни красного, ни синего: дети не рисуются.
    for x in (0..450).step_by(3) {
        for y in (0..80).step_by(3) {
            let c = px(&img, x, y);
            assert!(!(c[0] > 200 && c[1] < 60), "красный в ({x}, {y})");
            assert!(!(c[2] > 200 && c[0] < 60), "синий в ({x}, {y})");
        }
    }
    golden("boolean_ops", &img);
}

#[test]
fn nested_boolean_and_live_child_edit() {
    let mut p = Page::new();
    let (outer, e) = add_boolean(&mut p, BooleanOp::Union, 10.0, 10.0);
    // Вложенная «Вычитание» внизу справа: квадрат без левой половины.
    let inner = p.add(
        Some(outer),
        NodeKind::Boolean,
        PropsPatch {
            x: Some(100.0),
            boolean_op: Some(BooleanOp::Subtract),
            ..Default::default()
        },
    );
    p.add(Some(inner), NodeKind::Rect, rect(0.0, 0.0, 60.0, 60.0));
    p.add(Some(inner), NodeKind::Rect, rect(-10.0, 0.0, 40.0, 60.0));
    let mut scene = p.scene();
    let img = render(&scene, &camera(200.0, 80.0));
    assert_px(&img, 115, 40, WHITE, 0);
    assert_px(&img, 160, 40, GREEN, 0);
    assert_px(&img, 95, 40, GREEN, 0);
    golden("boolean_nested", &img);
    // Ребёнок ушёл вправо: кадр сразу по превью, без записи в документ.
    scene.set_preview(&[(
        e,
        PropsPatch {
            x: Some(60.0),
            ..Default::default()
        },
    )]);
    let img = render(&scene, &camera(200.0, 80.0));
    assert_px(&img, 115, 40, GREEN, 0);
    assert_px(&img, 95, 40, GREEN, 0);
    // Правка документа после превью — та же картинка.
    scene.clear_preview();
    p.edit(
        e,
        PropsPatch {
            x: Some(60.0),
            ..Default::default()
        },
    );
    p.update(&mut scene);
    let img = render(&scene, &camera(200.0, 80.0));
    assert_px(&img, 115, 40, GREEN, 0);
}

#[test]
fn hover_outline_follows_the_result() {
    let mut p = Page::new();
    let (b, _) = add_boolean(&mut p, BooleanOp::Subtract, 10.0, 10.0);
    let scene = p.scene();
    let overlay = ferrum_render::Overlay {
        hover: Some(b),
        ..Default::default()
    };
    let img = render_overlay(
        &scene,
        &camera(120.0, 80.0),
        &overlay,
        ferrum_render::Rgba::from_rgba8(255, 255, 255, 255),
    );
    // Контур вычитания проходит по дуге эллипса внутри квадрата (x ≈ 40), а не
    // по рамке детей справа (x = 100).
    let edge = px(&img, 40, 40);
    assert!(edge[2] > edge[0] + 40, "подсветка на дуге: {edge:?}");
    assert_px(&img, 100, 40, WHITE, 0);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-render --test boolean`
Expected: FAIL — тесты собираются, но 2 из 3 падают: дети булевой группы рисуются своими заливками — `пиксель (15, 40) = [255, 0, 0, 255], ожидался [0, 160, 0] ±0` (`every_op_is_drawn_with_the_group_paint`) и `пиксель (115, 40) = [217, 217, 217, 255], ожидался [255, 255, 255] ±0` (`nested_boolean_and_live_child_edit`); эталонов ещё нет

- [ ] **Step 3: Реализация**

В `crates/ferrum-render/src/display.rs` заменить:

````rust
            .map(|s| color(s.color));
    }
    if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
        if let Some(f) = p.fills.iter().find(|f| f.visible && f.opacity > 0.0) {
````

на:

````rust
            .map(|s| color(s.color));
    }
    // У булевой группы — её заливки и обводки (результат один, без своих областей).
    if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
        if let Some(f) = p.fills.iter().find(|f| f.visible && f.opacity > 0.0) {
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
        let path_shape = node.path_shape().cloned();
        let mut items = Vec::new();
        if let (NodeKind::Vector, Some(ps)) = (p.kind, &path_shape) {
            // Области снизу вверх, у каждой — её заливки (или заливки узла).
            for (region, r) in ps.regions.iter().enumerate() {
                for f in r.fills.as_deref().unwrap_or(&p.fills) {
````

на:

````rust
        let path_shape = node.path_shape().cloned();
        let mut items = Vec::new();
        if let (NodeKind::Vector | NodeKind::Boolean, Some(ps)) = (p.kind, &path_shape) {
            // Области снизу вверх, у каждой — её заливки (или заливки узла); у
            // булевой группы область одна — результат операции.
            for (region, r) in ps.regions.iter().enumerate() {
                for f in r.fills.as_deref().unwrap_or(&p.fills) {
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
                continue;
            }
            let has_children = !n.children.is_empty();
            if ctx.raster
                && ctx.scale <= RASTER_MAX_SCALE
````

на:

````rust
                continue;
            }
            // Дети булевой группы не рисуются: рисуется её результат (как лист).
            let has_children = !n.children.is_empty() && n.props.kind != NodeKind::Boolean;
            if ctx.raster
                && ctx.scale <= RASTER_MAX_SCALE
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
            path.move_to(to_device * Point::ZERO);
            path.line_to(to_device * Point::new(len, 0.0));
        }
        Geometry::None => path = to_device * n.local_bounds?.to_path(0.1),
````

на:

````rust
            path.move_to(to_device * Point::ZERO);
            path.line_to(to_device * Point::new(len, 0.0));
        }
        // Булева группа — контуром результата (пустой — рамкой детей).
        Geometry::None
            if n.props.kind == NodeKind::Boolean
                && n.path_shape().is_some_and(|s| !s.stroke.is_empty()) =>
        {
            path = device_path(&n.path_shape()?.stroke, to_device, screen)
        }
        Geometry::None => path = to_device * n.local_bounds?.to_path(0.1),
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
            None
        } else if n.props.kind == NodeKind::Frame && n.props.clip_content {
            world_bounds
        } else {
````

на:

````rust
            None
        } else if n.props.kind == NodeKind::Frame && n.props.clip_content {
            world_bounds
        } else if n.props.kind == NodeKind::Boolean {
            // Дети булевой группы не рисуются: видно только результат.
            world_bounds
        } else {
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test boolean`
Expected: созданы эталоны `crates/ferrum-render/tests/golden/{boolean_ops,boolean_nested}.png` (четыре операции над прямоугольником и кругом зелёной заливкой группы с чёрной обводкой по контуру результата: объединение, вычитание, пересечение, исключение; вложенная булева группа и правка ребёнка — объединение и узкий прямоугольник), тесты — 3 passed

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-render 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `boolean` рендера — 3 теста, эталоны прежние совпадают

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-render/src/display.rs crates/ferrum-render/src/draw.rs crates/ferrum-render/src/overlay.rs crates/ferrum-render/tests/boolean.rs crates/ferrum-scene/src/scene.rs crates/ferrum-render/tests/golden/boolean_nested.png crates/ferrum-render/tests/golden/boolean_ops.png
git commit -m "feat(render): булева группа рисуется результатом своими заливками и обводкой по контуру, дети не рисуются, подсветка по контуру результата; эталоны

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 4: Команды булевых операций

Спека 10: четыре операции на выделение. Здесь — `Editor::boolean(op)`: новая группа на месте верхнего слоя со стилем верхнего (у «Вычитания» — нижнего), фреймы не входят, меньше двух слоёв — сообщение, смена операции выделенных групп; клавиши Alt+Shift+U/S/I/E (как в справке Figma), AltGr глушит все сочетания с Alt; group-like в ресайзе, панели, перетаскивании в слоях и разгруппировке.

**Files:**
- Create: `crates/ferrum-editor/src/boolean.rs`
- Modify: `crates/ferrum-editor/src/{actions,commands,geometry,keymap,panel,lib}.rs`, `crates/ferrum-editor/src/tools/select.rs`
- Test: `crates/ferrum-editor/tests/boolean.rs` (новый), `crates/ferrum-editor/tests/keymap.rs` (команд 48 → 52)

**Interfaces:**
- Consumes: `NodeKind::Boolean`, `BooleanOp`, `PropsPatch::boolean_op`, `SceneNode::boolean`.
- Produces: `CommandId::{BooleanUnion, BooleanSubtract, BooleanIntersect, BooleanExclude}` (`object.boolean_union|subtract|intersect|exclude`, `Alt+Shift+KeyU/S/I/E`), `Editor::boolean(op)` (crate); `Keymap::lookup` — с AltGr не срабатывает ни одно сочетание с Alt.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/boolean.rs`:

````rust
//! Булевы операции в редакторе (Alt+Shift+U/S/I/E) (подсистема 3, этап 4): команды на выделение и
//! смена операции, стиль и имя новой группы, фреймы и одиночный слой, дети из
//! разных родителей, правка ребёнка с живым результатом, выделение щелчком и
//! двойным щелчком, разгруппировка.

mod common;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;
use kurbo::{Point, Rect};

/// Alt+Shift — булевы операции, как в Figma (Option+Shift на macOS).
fn alt_shift() -> Modifiers {
    Modifiers {
        alt: true,
        shift: true,
        ..Modifiers::NONE
    }
}

fn solid(r: u8, g: u8, b: u8) -> Vec<Fill> {
    vec![Fill::solid(Color::rgba(r, g, b, 255))]
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

/// Красный прямоугольник 100..200 и синий 150..250 по x (y 100..200).
fn two_rects(h: &mut H) -> (NodeId, NodeId) {
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 150.0, 100.0, 100.0, 100.0);
    h.set(
        a,
        PropsPatch {
            fills: Some(solid(255, 0, 0)),
            ..Default::default()
        },
    );
    h.set(
        b,
        PropsPatch {
            fills: Some(solid(0, 0, 255)),
            name: Some("Синий".into()),
            ..Default::default()
        },
    );
    (a, b)
}

fn world_box(h: &mut H, id: NodeId) -> Rect {
    let n = h.ed.scene().node(id).unwrap();
    n.world.transform_rect_bbox(n.local_bounds.unwrap())
}

fn near(a: Rect, b: Rect) -> bool {
    [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
        .iter()
        .all(|d| d.abs() < 0.01)
}

#[test]
fn union_wraps_the_selection_in_one_step_with_the_top_style() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    let before_a = world_box(&mut h, a);
    h.ed.select(&[a, b]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyU", alt_shift());
    let roots = h.children(None);
    assert_eq!(roots.len(), 1);
    let g = roots[0];
    let p = h.props(g);
    assert_eq!(p.kind, NodeKind::Boolean);
    assert_eq!(p.boolean_op, Some(BooleanOp::Union));
    assert_eq!(p.name, "Объединение");
    assert_eq!(p.fills, solid(0, 0, 255), "стиль верхнего слоя");
    assert_eq!(h.children(Some(g)), vec![a, b]);
    assert_eq!(h.sel(), vec![g]);
    assert!(near(world_box(&mut h, a), before_a), "дети на месте");
    assert!(near(
        world_box(&mut h, g),
        Rect::new(100.0, 100.0, 250.0, 200.0)
    ));
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn subtract_takes_the_bottom_style_and_draws_the_difference() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    h.ed.select(&[b, a]);
    h.key("KeyS", alt_shift());
    let g = h.children(None)[0];
    let p = h.props(g);
    assert_eq!(
        (p.boolean_op, p.name.as_str()),
        (Some(BooleanOp::Subtract), "Вычитание")
    );
    assert_eq!(p.fills, solid(255, 0, 0), "стиль нижнего слоя");
    assert!(near(
        world_box(&mut h, g),
        Rect::new(100.0, 100.0, 150.0, 200.0)
    ));
    // Щелчок по результату выделяет группу, по вычтенной части — ничего.
    h.ed.select(&[]);
    h.click(120.0, 150.0, Modifiers::NONE);
    assert_eq!(h.sel(), vec![g]);
    h.click(220.0, 150.0, Modifiers::NONE);
    assert!(h.sel().is_empty());
}

#[test]
fn selected_booleans_switch_their_op_and_default_name() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    h.ed.select(&[a, b]);
    h.key("KeyU", alt_shift());
    let g = h.children(None)[0];
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyI", alt_shift());
    let p = h.props(g);
    assert_eq!(
        (p.boolean_op, p.name.as_str()),
        (Some(BooleanOp::Intersect), "Пересечение")
    );
    assert_eq!(h.children(Some(g)), vec![a, b], "не обёрнута заново");
    assert!(near(
        world_box(&mut h, g),
        Rect::new(150.0, 100.0, 200.0, 200.0)
    ));
    h.assert_one_step(steps, &before);
    // Своё имя остаётся; та же операция — без шага.
    h.set(
        g,
        PropsPatch {
            name: Some("Мой знак".into()),
            ..Default::default()
        },
    );
    h.key("KeyE", alt_shift());
    assert_eq!(h.props(g).name, "Мой знак");
    assert_eq!(h.props(g).boolean_op, Some(BooleanOp::Exclude));
    let steps = h.ed.step_count();
    h.key("KeyE", alt_shift());
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn single_layers_and_frames_are_refused_with_a_message() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    let f = h.frame(None, 300.0, 0.0, 100.0, 100.0);
    let b = h.rect(None, 100.0, 0.0, 50.0, 50.0);
    let steps = h.ed.step_count();
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
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(
        errors(&mut h),
        vec!["Булевы операции не применяются к фреймам"]
    );
    // Фрейм пропускается, остальные — в группу.
    h.ed.select(&[a, f, b]);
    h.key("KeyU", alt_shift());
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(errors(&mut h), vec!["Фреймы в булеву группу не входят"]);
    let g = h.sel()[0];
    assert_eq!(h.children(Some(g)), vec![a, b]);
    assert_eq!(h.parent(f), None);
}

#[test]
fn children_from_a_rotated_group_keep_their_place() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let grp = h.add(None, NodeKind::Group, patch(400.0, 100.0, 0.0, 0.0));
    let b = h.rect(Some(grp), 0.0, 0.0, 60.0, 40.0);
    h.set(
        grp,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    let corner = h.ed.scene().node(b).unwrap().world * Point::new(60.0, 40.0);
    h.ed.select(&[a, b]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    assert_eq!(h.parent(b), Some(g));
    let now = h.ed.scene().node(b).unwrap().world * Point::new(60.0, 40.0);
    assert!((now - corner).hypot() < 1e-6, "{now:?} ≠ {corner:?}");
    h.assert_scene_clean();
}

#[test]
fn dragging_a_child_updates_the_result_live_and_is_one_step() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    h.ed.select(&[a, b]);
    h.key("KeyI", alt_shift());
    let g = h.sel()[0];
    // Двойной щелчок — внутрь группы, к ребёнку под курсором.
    h.ed.select(&[]);
    h.click(175.0, 150.0, Modifiers::NONE);
    assert_eq!(h.sel(), vec![g]);
    h.dbl(175.0, 150.0);
    assert_eq!(h.sel(), vec![b]);
    let before = h.state();
    let steps = h.ed.step_count();
    // Синий уходит влево на 30: пересечение 150..200 → 120..200 уже в превью.
    h.drag_hold((175.0, 150.0), (145.0, 150.0), Modifiers::NONE);
    assert!(
        near(world_box(&mut h, g), Rect::new(120.0, 100.0, 200.0, 200.0)),
        "{:?}",
        world_box(&mut h, g)
    );
    assert_eq!(h.props(b).x, 150.0, "документ ещё не тронут");
    h.up(145.0, 150.0, Modifiers::NONE);
    assert!(near(
        world_box(&mut h, g),
        Rect::new(120.0, 100.0, 200.0, 200.0)
    ));
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn ungroup_returns_the_children_and_resize_scales_them() {
    let mut h = H::new();
    let (a, b) = two_rects(&mut h);
    h.ed.select(&[a, b]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    // Ресайз группы за правый край: дети растягиваются, как в группе.
    h.ed.execute(CommandId::Escape);
    h.ed.select(&[g]);
    h.drag((250.0, 150.0), (400.0, 150.0), Modifiers::NONE);
    assert!(near(
        world_box(&mut h, g),
        Rect::new(100.0, 100.0, 400.0, 200.0)
    ));
    assert!((h.props(a).w - 200.0).abs() < 1e-6, "{}", h.props(a).w);
    h.ed.select(&[g]);
    h.key(
        "KeyG",
        Modifiers {
            shift: true,
            ..ctrl()
        },
    );
    assert_eq!(h.children(None), vec![a, b]);
    assert_eq!(h.sel(), vec![a, b]);
    h.assert_scene_clean();
}
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 48);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 52);
}

````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    q.alt_graph = true;
    assert_eq!(map.lookup(&q), None);
    // Без Ctrl+Alt в сочетании AltGr ничему не мешает.
    let mut esc = KeyEvent::new("Escape", Modifiers::NONE);
    esc.alt_graph = true;
````

на:

````rust
    q.alt_graph = true;
    assert_eq!(map.lookup(&q), None);
    // Alt+Shift+E (исключение) с AltGr — символ («ę» на польской раскладке).
    let mut e2 = key("KeyE", false, true, true, false);
    assert_eq!(win.lookup(&e2), Some(CommandId::BooleanExclude));
    e2.alt_graph = true;
    assert_eq!(win.lookup(&e2), None);
    // Без Alt в сочетании AltGr ничему не мешает.
    let mut esc = KeyEvent::new("Escape", Modifiers::NONE);
    esc.alt_graph = true;
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
        vec!["Неизвестная платформа раскладки — сочетания как в файле".to_string()]
    );
}
````

на:

````rust
        vec!["Неизвестная платформа раскладки — сочетания как в файле".to_string()]
    );
}

#[test]
fn boolean_ops_are_alt_shift_letters_like_figma() {
    for platform in [Platform::Mac, Platform::Windows, Platform::Linux] {
        let map = Keymap::new(platform);
        for (code, id) in [
            ("KeyU", CommandId::BooleanUnion),
            ("KeyS", CommandId::BooleanSubtract),
            ("KeyI", CommandId::BooleanIntersect),
            ("KeyE", CommandId::BooleanExclude),
        ] {
            assert_eq!(map.lookup(&key(code, false, true, true, false)), Some(id));
            // Сочетание занято только булевой операцией.
            let s = map.bindings(id)[0].clone();
            assert_eq!(map.commands_for(&s), vec![id], "{platform:?} {code}");
        }
        // Ctrl/Cmd+Alt+буква — не булевы операции; Shift+E и Alt+E свободны.
        let primary = platform == Platform::Mac;
        assert_eq!(
            map.lookup(&key("KeyU", !primary, true, false, primary)),
            None
        );
        assert_eq!(map.lookup(&key("KeyE", false, false, true, false)), None);
        assert_eq!(map.lookup(&key("KeyE", false, true, false, false)), None);
    }
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test boolean`
Expected: FAIL — тесты собираются, но все 7 падают: Alt+Shift+U не делает группу (`left: 2, right: 1` — слоёв по-прежнему два), одиночный слой — без сообщения (`left: []`), смены операции нет (`left: (None, "Прямоугольник")`)

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::ToolArrow => self.set_tool(ToolKind::Arrow),
            C::Flatten => self.flatten(),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
````

на:

````rust
            C::ToolArrow => self.set_tool(ToolKind::Arrow),
            C::Flatten => self.flatten(),
            C::BooleanUnion => self.boolean(ferrum_doc::BooleanOp::Union),
            C::BooleanSubtract => self.boolean(ferrum_doc::BooleanOp::Subtract),
            C::BooleanIntersect => self.boolean(ferrum_doc::BooleanOp::Intersect),
            C::BooleanExclude => self.boolean(ferrum_doc::BooleanOp::Exclude),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
    }

    /// Ctrl+Shift+G: дети групп и фреймов выделения встают на их место у родителя.
    fn ungroup(&mut self) {
        let mut ids: Vec<(Vec<u32>, NodeId)> = top_level(&self.scene, &self.selection)
````

на:

````rust
    }

    /// Ctrl+Shift+G: дети групп, булевых групп и фреймов выделения встают на их
    /// место у родителя (у булевой группы — снова со своими заливками).
    fn ungroup(&mut self) {
        let mut ids: Vec<(Vec<u32>, NodeId)> = top_level(&self.scene, &self.selection)
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            .filter(|(s, _)| {
                let n = self.scene.at(*s);
                matches!(n.props.kind, NodeKind::Group | NodeKind::Frame) && !n.children.is_empty()
            })
            .map(|(s, id)| (self.scene.order_key(s), id))
````

на:

````rust
            .filter(|(s, _)| {
                let n = self.scene.at(*s);
                matches!(
                    n.props.kind,
                    NodeKind::Group | NodeKind::Frame | NodeKind::Boolean
                ) && !n.children.is_empty()
            })
            .map(|(s, id)| (self.scene.order_key(s), id))
````

Создать `crates/ferrum-editor/src/boolean.rs`:

````rust
//! Булевы операции над выделением (спека подсистемы 3, раздел 10): Alt+Shift+U,
//! S, I, E (Option+Shift на macOS, как в Figma) и выпадающий список тулбара. Выделение оборачивается булевой группой
//! на месте верхнего слоя одним шагом отмены; у выделенных булевых групп та же
//! команда меняет операцию.
//!
//! Как в Figma: группа берёт заливки и обводки верхнего слоя (у «Вычитания» —
//! нижнего, из которого вычитают), называется по операции, фреймы в булеву
//! группу не входят, нужно хотя бы два слоя.

use crate::editor::{Editor, EditorEvent};
use crate::geometry::parent_world;
use crate::ops::reposition;
use ferrum_doc::{BooleanOp, NodeId, NodeKind, Props, PropsPatch};

/// Слой, чей стиль берёт новая булева группа: у группы — её верхний (у
/// «Вычитания» — нижний) потомок со своими заливками.
fn style_source(editor: &Editor, id: NodeId, top: bool) -> Option<Props> {
    let n = editor.scene.node(id)?;
    if n.props.kind != NodeKind::Group {
        return Some(n.props.clone());
    }
    let mut kids: Vec<NodeId> = n.children.iter().map(|&c| editor.scene.at(c).id).collect();
    if top {
        kids.reverse();
    }
    kids.into_iter().find_map(|c| style_source(editor, c, top))
}

impl Editor {
    /// Булева операция `op` над выделением (одна команда — один шаг).
    pub(crate) fn boolean(&mut self, op: BooleanOp) {
        self.finish_vector_edit();
        let ids = self.ordered_selection();
        if ids.is_empty() {
            return;
        }
        // Выделены только булевы группы — у них меняется операция.
        let all_booleans = ids.iter().all(|id| {
            self.scene
                .node(*id)
                .is_some_and(|n| n.props.kind == NodeKind::Boolean)
        });
        if all_booleans {
            self.set_boolean_op(&ids, op);
            return;
        }
        let frames = ids.iter().any(|id| {
            self.scene
                .node(*id)
                .is_some_and(|n| n.props.kind == NodeKind::Frame)
        });
        let layers: Vec<NodeId> = ids
            .into_iter()
            .filter(|id| {
                self.scene
                    .node(*id)
                    .is_some_and(|n| n.props.kind != NodeKind::Frame)
            })
            .collect();
        if layers.len() < 2 {
            let msg = if frames {
                "Булевы операции не применяются к фреймам"
            } else {
                "Для булевой операции выделите хотя бы два слоя"
            };
            self.out.events.push(EditorEvent::Error(msg.into()));
            return;
        }
        if frames {
            self.out.events.push(EditorEvent::Error(
                "Фреймы в булеву группу не входят".into(),
            ));
        }
        let top = *layers.last().expect("слоёв не меньше двух");
        let top_slot = self.scene.slot(top).expect("выделение в сцене");
        let parent_slot = self.scene.at(top_slot).parent;
        let parent = parent_slot.map(|p| self.scene.at(p).id);
        let pw = parent_world(&self.scene, top_slot);
        let index = self.scene.index(top_slot) as usize + 1;
        let source = if op == BooleanOp::Subtract {
            style_source(self, layers[0], false)
        } else {
            style_source(self, top, true)
        };
        let mut props = Props {
            name: op.name().into(),
            boolean_op: Some(op),
            ..Props::defaults(NodeKind::Boolean)
        };
        if let Some(s) = source {
            props.fills = s.fills;
            props.strokes = s.strokes;
            props.stroke_style = s.stroke_style;
        }
        self.ctx().commit(|tx, scene| {
            let b = tx.create_with_props(parent, Some(index), &props)?;
            tx.move_nodes(&layers, Some(b), 0)?;
            for id in &layers {
                let n = scene.node(*id).expect("выделение в сцене");
                // Тот же родитель: координаты те же (группа в начале координат).
                if n.parent != parent_slot {
                    tx.set_props(*id, &reposition(pw, n.world, &n.props))?;
                }
            }
            Ok(vec![b])
        });
    }

    /// Меняет операцию булевых групп `ids`; имя по умолчанию («Объединение» и
    /// т. п.) меняется вместе с ней, своё — остаётся.
    fn set_boolean_op(&mut self, ids: &[NodeId], op: BooleanOp) {
        let patches: Vec<(NodeId, PropsPatch)> = ids
            .iter()
            .filter_map(|id| {
                let p = &self.scene.node(*id)?.props;
                if p.boolean_op == Some(op) {
                    return None;
                }
                let default_name = BooleanOp::ALL.iter().any(|o| o.name() == p.name);
                Some((
                    *id,
                    PropsPatch {
                        boolean_op: Some(op),
                        name: default_name.then(|| op.name().to_string()),
                        ..Default::default()
                    },
                ))
            })
            .collect();
        if patches.is_empty() {
            return;
        }
        let selection = ids.to_vec();
        self.ctx().commit(|tx, _| {
            for (id, patch) in &patches {
                tx.set_props(*id, patch)?;
            }
            Ok(selection)
        });
    }
}
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// «Сплющить»: выделение — в один вектор.
    Flatten,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
````

на:

````rust
    /// «Сплющить»: выделение — в один вектор.
    Flatten,
    /// Булевы операции над выделением (подсистема 3, этап 4); у выделенных
    /// булевых групп — смена операции.
    BooleanUnion,
    BooleanSubtract,
    BooleanIntersect,
    BooleanExclude,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    ),
    cmd(C::Flatten, "object.flatten", "Сплющить", &["Mod+KeyE"]),
    cmd(
        C::FrameSelection,
````

на:

````rust
    ),
    cmd(C::Flatten, "object.flatten", "Сплющить", &["Mod+KeyE"]),
    // Как в Figma: Alt+Shift+буква (Option+Shift на macOS), исключение — E.
    // Свободны на обеих платформах: Alt+Shift и Shift+E в таблице ничем не заняты.
    cmd(
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
        C::FrameSelection,
````

В `crates/ferrum-editor/src/geometry.rs` заменить:

````rust
            parent_world: pw,
        });
        if !with_group_children || n.props.kind != NodeKind::Group {
            continue;
        }
        // Потомки групп (не фреймов: дети фрейма не масштабируются).
        let mut stack = vec![(slot, out.len() - 1)];
        while let Some((s, at)) = stack.pop() {
````

на:

````rust
            parent_world: pw,
        });
        if !with_group_children || !n.props.kind.is_group_like() {
            continue;
        }
        // Потомки групп и булевых групп (не фреймов: дети фрейма не
        // масштабируются).
        let mut stack = vec![(slot, out.len() - 1)];
        while let Some((s, at)) = stack.pop() {
````

В `crates/ferrum-editor/src/geometry.rs` заменить:

````rust
                    parent_world: Affine::IDENTITY,
                });
                if cn.props.kind == NodeKind::Group {
                    stack.push((c, out.len() - 1));
                }
````

на:

````rust
                    parent_world: Affine::IDENTITY,
                });
                if cn.props.kind.is_group_like() {
                    stack.push((c, out.len() - 1));
                }
````

В `crates/ferrum-editor/src/geometry.rs` заменить:

````rust
        patch.flip_y = Some(pose.flip_y);
    }
    if size && base.kind != NodeKind::Group {
        patch.w = Some(pose.w);
        if base.kind != NodeKind::Line {
````

на:

````rust
        patch.flip_y = Some(pose.flip_y);
    }
    if size && !base.kind.is_group_like() {
        patch.w = Some(pose.w);
        if base.kind != NodeKind::Line {
````

В `crates/ferrum-editor/src/geometry.rs` заменить:

````rust
        };
        in_parent.push(tp);
        if b.props.kind == NodeKind::Group {
            continue;
        }
````

на:

````rust
        };
        in_parent.push(tp);
        if b.props.kind.is_group_like() {
            continue;
        }
````

В `crates/ferrum-editor/src/keymap.rs` заменить:

````rust
    /// Команда для нажатой клавиши. При конфликте — первая по таблице команд.
    ///
    /// С AltGr (`KeyEvent::alt_graph`) на Windows и Linux сочетания с Mod+Alt не
    /// срабатывают: там AltGr приходит как Ctrl+Alt, и набор символа (например,
    /// «@» на немецкой раскладке) иначе вызывал бы команду. На macOS AltGr нет, а
    /// Option некоторые браузеры отдают как AltGraph — правило убило бы ⌥⌘] и ⌥⌘[,
    /// поэтому там флаг не учитывается.
````

на:

````rust
    /// Команда для нажатой клавиши. При конфликте — первая по таблице команд.
    ///
    /// С AltGr (`KeyEvent::alt_graph`) на Windows и Linux сочетания с Alt не
    /// срабатывают: там AltGr приходит как Ctrl+Alt (или как Alt), и набор символа
    /// (например, «@» на немецкой раскладке, «ę» — AltGr+Shift+E на польской)
    /// иначе вызывал бы команду (Mod+Alt+G, булевы Alt+Shift+U/S/I/E). На macOS AltGr нет, а
    /// Option некоторые браузеры отдают как AltGraph — правило убило бы ⌥⌘] и ⌥⌘[,
    /// поэтому там флаг не учитывается.
````

В `crates/ferrum-editor/src/keymap.rs` заменить:

````rust
            mods: e.modifiers.into(),
        };
        if e.alt_graph && self.platform != Platform::Mac && s.mods.alt && s.mods.ctrl {
            return None;
        }
````

на:

````rust
            mods: e.modifiers.into(),
        };
        if e.alt_graph && self.platform != Platform::Mac && s.mods.alt {
            return None;
        }
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust

mod actions;
pub mod clipboard;
pub mod commands;
````

на:

````rust

mod actions;
mod boolean;
pub mod clipboard;
pub mod commands;
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
    let n = scene.at(slot);
    let p = &n.props;
    if p.kind == NodeKind::Group {
        let b = n.local_bounds.unwrap_or(Rect::ZERO);
        let corner = n.local * Point::new(b.x0, b.y0);
````

на:

````rust
    let n = scene.at(slot);
    let p = &n.props;
    if p.kind.is_group_like() {
        let b = n.local_bounds.unwrap_or(Rect::ZERO);
        let corner = n.local * Point::new(b.x0, b.y0);
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
        return vec![];
    }
    if t.props.kind != NodeKind::Group {
        return vec![(
            t.id,
````

на:

````rust
        return vec![];
    }
    if !t.props.kind.is_group_like() {
        return vec![(
            t.id,
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
            DropTarget::Into(a) => {
                let kind = self.scene.at(anchor_slot).props.kind;
                if kind != NodeKind::Frame && kind != NodeKind::Group {
                    return false;
                }
````

на:

````rust
            DropTarget::Into(a) => {
                let kind = self.scene.at(anchor_slot).props.kind;
                // Внутрь — фрейма, группы и булевой группы (как в Figma).
                if kind != NodeKind::Frame && !kind.is_group_like() {
                    return false;
                }
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
    let (mut wide_x, mut wide_y) = (false, false);
    for b in bases.iter_mut() {
        if matches!(b.props.kind, NodeKind::Line | NodeKind::Group) {
            continue;
        }
````

на:

````rust
    let (mut wide_x, mut wide_y) = (false, false);
    for b in bases.iter_mut() {
        if b.props.kind == NodeKind::Line || b.props.kind.is_group_like() {
            continue;
        }
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `boolean` редактора — 7 тестов, `keymap` — 15 тестов (52 команды), остальные тесты `ferrum-editor` зелёные

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/boolean.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/geometry.rs crates/ferrum-editor/src/keymap.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/panel.rs crates/ferrum-editor/src/tools/select.rs crates/ferrum-editor/tests/boolean.rs crates/ferrum-editor/tests/keymap.rs
git commit -m "feat(editor): булевы операции над выделением (Alt+Shift+U/S/I/E, как в Figma) и смена операции выделенных групп; стиль верхнего слоя (у «Вычитания» — нижнего), фреймы не входят, ресайз и разгруппировка как у группы; AltGr не вызывает сочетаний с Alt

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5: «Сплющить» булевой группы и текста, «Контур обводки», текст по контурам глифов

Спека 10: «Сплющить» (результат в `Vector`) и «Контур обводки» (Ctrl/Cmd+Alt+O — обводка в заливку `Vector`). Здесь — «Сплющить» булевой группы (результат) и текста (контуры глифов), «Контур обводки» (`outline.rs`), источник контуров глифов для сцены из реестра шрифтов редактора (`install_text_outlines`, `sync_text_outlines`, флаг `outlines_missed` у занятого реестра).

**Files:**
- Create: `crates/ferrum-editor/src/outline.rs`
- Modify: `crates/ferrum-editor/src/{actions,commands,editor,flatten,text,vector_edit,vector_ops,lib}.rs`, `crates/ferrum-render/src/{text_node,lib}.rs`
- Test: `crates/ferrum-editor/tests/outline.rs` (новый), `crates/ferrum-editor/tests/keymap.rs` (команд 52 → 53)

**Interfaces:**
- Consumes: `boolean::{flatten_path, normalize, combine, leaf_fill}`, `vector::stroke_outline`, `Scene::set_text_outlines`, `flatten::*`.
- Produces: `CommandId::OutlineStroke` (`object.outline_stroke`, `Mod+Alt+KeyO`); `ferrum_render::text_outline(&mut Fonts, &Props) -> Option<BezPath>`; `vector_ops::path_network(&BezPath, FillRule) -> VectorNetwork`; `Editor::node_network(slot)` (crate), `Editor::sync_text_outlines()` (pub, вызывается из `emit`), `install_text_outlines(scene, fonts, missed)` (crate), `Editor::outlines_missed: Rc<Cell<bool>>`; `as_vector` сбрасывает `boolean_op`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 52);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 53);
}

````

Создать `crates/ferrum-editor/tests/outline.rs`:

````rust
//! «Сплющить» булевой группы и текста, «Контур обводки», текст в булевой
//! группе по контурам глифов (подсистема 3, этап 4).

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::vector_ops::path_network;
use ferrum_editor::*;
use ferrum_scene::vector::fit;
use kurbo::{BezPath, Circle, Point, Rect, Shape};

/// «Контур обводки» — Ctrl/Cmd+Alt+O.
fn ctrl_alt() -> Modifiers {
    Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::NONE
    }
}

/// Булевы операции — Alt+Shift+буква, как в Figma.
fn alt_shift() -> Modifiers {
    Modifiers {
        alt: true,
        shift: true,
        ..Modifiers::NONE
    }
}

fn world_points(h: &mut H, id: NodeId) -> Vec<Point> {
    let n = h.ed.scene().node(id).unwrap().clone();
    let net = n.props.vector.as_deref().unwrap().clone();
    let m = n.world * fit(&n.props);
    net.points
        .iter()
        .map(|p| m * Point::new(p.x, p.y))
        .collect()
}

fn world_box(h: &mut H, id: NodeId) -> Rect {
    let n = h.ed.scene().node(id).unwrap();
    n.world.transform_rect_bbox(n.local_bounds.unwrap())
}

fn near(a: Rect, b: Rect, tol: f64) -> bool {
    [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
        .iter()
        .all(|d| d.abs() < tol)
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
fn path_network_keeps_curves_and_closes_loops() {
    let mut p = BezPath::new();
    p.move_to((0.0, 0.0));
    p.line_to((10.0, 0.0));
    p.quad_to((10.0, 10.0), (0.0, 10.0));
    p.close_path();
    // Подпуть, замкнутый своей точкой: замыкающий отрезок не повторяется.
    p.move_to((20.0, 0.0));
    p.line_to((30.0, 0.0));
    p.line_to((30.0, 10.0));
    p.line_to((20.0, 0.0));
    p.close_path();
    // Открытый подпуть — без области.
    p.move_to((50.0, 0.0));
    p.curve_to((60.0, 0.0), (60.0, 10.0), (50.0, 10.0));
    let net = path_network(&p, FillRule::EvenOdd);
    assert_eq!(net.validate(), Ok(()));
    assert_eq!(
        (net.points.len(), net.segments.len()),
        (3 + 3 + 2, 3 + 3 + 1)
    );
    assert_eq!(net.regions.len(), 1);
    assert_eq!(net.regions[0].rule, FillRule::EvenOdd);
    assert_eq!(net.regions[0].loops, vec![vec![0, 1, 2], vec![3, 4, 5]]);
    assert!(!net.segments[1].is_straight(), "квадратичная — кубической");
    // Круг: четыре кривые, одна петля.
    let c = path_network(
        &Circle::new((0.0, 0.0), 5.0).to_path(1e-3),
        FillRule::NonZero,
    );
    assert_eq!(c.validate(), Ok(()));
    assert_eq!(c.points.len(), c.segments.len());
    assert!(c.segments.iter().all(|s| !s.is_straight()));
}

#[test]
fn flatten_boolean_gives_its_result_in_place_in_one_step() {
    let mut h = H::new();
    let below = h.rect(None, 0.0, 300.0, 10.0, 10.0);
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 150.0, 100.0, 100.0, 100.0);
    let above = h.rect(None, 0.0, 400.0, 10.0, 10.0);
    h.ed.select(&[a, b]);
    h.key("KeyS", alt_shift());
    let g = h.sel()[0];
    h.set(
        g,
        PropsPatch {
            name: Some("Знак".into()),
            ..Default::default()
        },
    );
    let fills = h.props(g).fills;
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyE", ctrl());
    let roots = h.children(None);
    assert_eq!(roots.len(), 3, "дети ушли вместе с группой");
    assert_eq!((roots[0], roots[2]), (below, above), "на месте группы");
    let v = roots[1];
    assert_eq!(h.sel(), vec![v]);
    let p = h.props(v);
    assert_eq!((p.kind, p.name.as_str()), (NodeKind::Vector, "Знак"));
    assert_eq!(p.fills, fills);
    assert!(p.boolean_op.is_none());
    let w = world_points(&mut h, v);
    assert_eq!(w.len(), 4, "{w:?}");
    for c in [
        (100.0, 100.0),
        (150.0, 100.0),
        (150.0, 200.0),
        (100.0, 200.0),
    ] {
        assert!(
            w.iter().any(|q| (*q - Point::from(c)).hypot() < 1e-6),
            "{c:?}"
        );
    }
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn text_flattens_and_booleans_use_glyph_outlines() {
    let mut h = text_h();
    let t = add_text(&mut h, 100.0, 100.0, "Ok", 40.0);
    let tb = world_box(&mut h, t);
    // Объединение текста и прямоугольника справа: результат — глифы, а не рамка
    // текста (между буквами пусто).
    let r = h.rect(None, tb.x1 + 50.0, tb.y0, 20.0, 20.0);
    h.ed.select(&[t, r]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    let n = h.ed.scene().node(g).unwrap();
    let area = n.boolean.as_ref().unwrap().contours.to_path().area().abs();
    assert!(area > 400.0 + 100.0, "глифы в результате: {area}");
    assert!(
        area < 400.0 + 0.8 * tb.area(),
        "не рамка текста: {area} при рамке {}",
        tb.area()
    );
    h.ed.execute(CommandId::Undo);
    // Текст сплющивается в контуры глифов на своём месте.
    h.ed.select(&[t]);
    h.ed.take_events();
    h.key("KeyE", ctrl());
    assert!(errors(&mut h).is_empty());
    let v = h.sel()[0];
    let p = h.props(v);
    assert_eq!(p.kind, NodeKind::Vector);
    assert!(p.text.is_none());
    let net = p.vector.as_deref().unwrap();
    // «O» — две петли (внешняя и внутренняя), «k» — одна.
    assert!(
        net.regions[0].loops.len() >= 3,
        "{}",
        net.regions[0].loops.len()
    );
    let vb = world_box(&mut h, v);
    assert!(tb.contains(vb.center()) && vb.width() <= tb.width() + 1.0);
}

#[test]
fn text_without_fonts_is_not_flattened() {
    let mut h = H::new();
    let t = h.add(None, NodeKind::Text, patch(0.0, 0.0, 50.0, 20.0));
    h.ed.select(&[t]);
    h.ed.take_events();
    let steps = h.ed.step_count();
    h.key("KeyE", ctrl());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(
        errors(&mut h),
        vec!["Шрифты ещё загружаются — текст не сплющен"]
    );
    assert_eq!(h.props(t).kind, NodeKind::Text);
}

#[test]
fn outline_stroke_of_a_filled_rect_adds_a_ring_vector_above() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    let red = Color::rgba(255, 0, 0, 255);
    h.set(
        r,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: red,
                width: 10.0,
                align: StrokeAlign::Inside,
            }]),
            ..Default::default()
        },
    );
    h.ed.select(&[r]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyO", ctrl_alt());
    let roots = h.children(None);
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0], r, "слой с заливкой остаётся под контуром");
    assert!(h.props(r).strokes.is_empty());
    let v = roots[1];
    assert_eq!(h.sel(), vec![v]);
    let p = h.props(v);
    assert_eq!(p.kind, NodeKind::Vector);
    assert!(p.fills.is_empty() && p.strokes.is_empty());
    assert_eq!(p.name, "Прямоугольник");
    let net = p.vector.as_deref().unwrap();
    assert_eq!(net.regions.len(), 1);
    assert_eq!(net.regions[0].fills, Some(vec![Fill::solid(red)]));
    assert_eq!(net.regions[0].rule, FillRule::EvenOdd);
    // Внутренняя обводка — внутри рамки.
    assert!(near(
        world_box(&mut h, v),
        Rect::new(100.0, 100.0, 200.0, 150.0),
        1e-6
    ));
    // Кольцо: щелчок в середину — мимо контура, по краю — в него.
    h.ed.select(&[]);
    h.click(105.0, 125.0, Modifiers::NONE);
    assert_eq!(h.sel(), vec![v]);
    h.click(150.0, 125.0, Modifiers::NONE);
    assert_eq!(h.sel(), vec![r]);
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn outline_stroke_of_an_open_path_keeps_curves_caps_and_removes_the_layer() {
    let mut h = H::new();
    let l = h.add(None, NodeKind::Line, patch(100.0, 100.0, 200.0, 0.0));
    h.set(
        l,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 4.0,
                align: StrokeAlign::Center,
            }]),
            stroke_cap_end: Some(StrokeCap::Round),
            ..Default::default()
        },
    );
    h.ed.select(&[l]);
    h.key("KeyO", ctrl_alt());
    let roots = h.children(None);
    assert_eq!(roots.len(), 1, "линия без заливок — заменена контуром");
    let v = roots[0];
    let p = h.props(v);
    let net = p.vector.as_deref().unwrap();
    assert!(
        net.segments.iter().any(|s| !s.is_straight()),
        "круглое окончание — кривые"
    );
    let b = world_box(&mut h, v);
    assert!(near(b, Rect::new(100.0, 98.0, 302.0, 102.0), 1e-3), "{b:?}");
}

#[test]
fn outline_stroke_without_strokes_is_a_message() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[r]);
    h.ed.take_events();
    let steps = h.ed.step_count();
    h.key("KeyO", ctrl_alt());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(
        errors(&mut h),
        vec!["У выделенных слоёв нет видимых обводок"]
    );
}

#[test]
fn outline_stroke_of_a_boolean_outlines_its_result() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 150.0, 150.0, 100.0, 100.0);
    h.set(
        b,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 2.0,
                align: StrokeAlign::Outside,
            }]),
            ..Default::default()
        },
    );
    h.ed.select(&[a, b]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    h.key("KeyO", ctrl_alt());
    let v = *h.children(None).last().unwrap();
    assert_ne!(v, g);
    assert_eq!(h.props(v).kind, NodeKind::Vector);
    // Снаружи результата: рамка шире на толщину, внутри объединения пусто.
    assert!(near(
        world_box(&mut h, v),
        Rect::new(98.0, 98.0, 252.0, 252.0),
        0.01
    ));
    let n = h.ed.scene().node(v).unwrap();
    let inside = n.world.inverse() * Point::new(160.0, 160.0);
    assert!(!ferrum_scene::shape::hit_shape(
        &n.props,
        n.path_shape().map(|s| &**s),
        inside,
        0.0
    ));
}

#[test]
fn text_missed_while_the_font_registry_is_busy_is_recomputed_on_the_next_frame() {
    let mut h = text_h();
    let t = add_text(&mut h, 100.0, 100.0, "Ok", 40.0);
    let r = h.rect(None, 400.0, 100.0, 20.0, 20.0);
    h.ed.select(&[t, r]);
    // Реестр занят, пока сцена считает группу (как посреди раскладки):
    // текст формы не даёт.
    let fonts = h.ed.fonts().clone();
    let busy = fonts.borrow_mut();
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    let area = |h: &H| {
        h.ed.scene()
            .node(g)
            .unwrap()
            .boolean
            .as_ref()
            .unwrap()
            .contours
            .to_path()
            .area()
            .abs()
    };
    assert!((area(&h) - 400.0).abs() < 1e-6, "пока только прямоугольник");
    drop(busy);
    // Шрифты не менялись, но следующий кадр пересчитывает пропущенное.
    h.hover(10.0, 10.0);
    assert!(area(&h) > 500.0, "глифы в результате: {}", area(&h));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test outline`
Expected: FAIL — не собирается: нет `ferrum_editor::vector_ops::path_network`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::BooleanIntersect => self.boolean(ferrum_doc::BooleanOp::Intersect),
            C::BooleanExclude => self.boolean(ferrum_doc::BooleanOp::Exclude),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
````

на:

````rust
            C::BooleanIntersect => self.boolean(ferrum_doc::BooleanOp::Intersect),
            C::BooleanExclude => self.boolean(ferrum_doc::BooleanOp::Exclude),
            C::OutlineStroke => self.outline_stroke(),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    BooleanIntersect,
    BooleanExclude,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
````

на:

````rust
    BooleanIntersect,
    BooleanExclude,
    /// «Контур обводки»: обводки выделения — в векторы с заливкой их цветом.
    OutlineStroke,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    ),
    cmd(
        C::FrameSelection,
        "object.frame_selection",
````

на:

````rust
    ),
    cmd(
        C::OutlineStroke,
        "object.outline_stroke",
        "Контур обводки",
        &["Mod+Alt+KeyO"],
    ),
    cmd(
        C::FrameSelection,
        "object.frame_selection",
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
//! `camera()` и `overlay()` по ссылке.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

````

на:

````rust
//! `camera()` и `overlay()` по ссылке.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Правка вектора и перо (спека подсистемы 3, раздел 9).
    pub(crate) vector_edit: Option<VectorEdit>,
}

````

на:

````rust
    /// Правка вектора и перо (спека подсистемы 3, раздел 9).
    pub(crate) vector_edit: Option<VectorEdit>,
    /// Поколение реестра шрифтов, с которым посчитаны контуры глифов текста в
    /// булевых группах сцены (`sync_text_outlines`).
    pub(crate) outlines_generation: u64,
    /// Источник контуров глифов застал реестр шрифтов занятым: текст булевой
    /// группы остался без формы — `sync_text_outlines` пересчитает её на
    /// следующем `emit`.
    pub(crate) outlines_missed: Rc<Cell<bool>>,
}

/// Ставит сцене источник контуров глифов для текста в булевых группах — реестр
/// шрифтов редактора. Реестр занят (сцена пересчитывается посреди раскладки) —
/// текст пока без формы, а `missed` помечается: пересчёт — на ближайшем `emit`.
pub(crate) fn install_text_outlines(
    scene: &mut Scene,
    fonts: &SharedFonts,
    missed: &Rc<Cell<bool>>,
) {
    let fonts = fonts.clone();
    let missed = missed.clone();
    scene.set_text_outlines(Some(Rc::new(move |p: &ferrum_doc::Props| {
        let Ok(mut f) = fonts.try_borrow_mut() else {
            missed.set(true);
            return None;
        };
        ferrum_render::text_outline(&mut f, p)
    })));
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            None => return Err(DocError::PageNotFound("—".into())),
        };
        let scene = Scene::from_page(doc.page(page)?);
        // Изменения, накопленные до редактора, уже в сцене.
        for (p, _) in doc.pages() {
````

на:

````rust
            None => return Err(DocError::PageNotFound("—".into())),
        };
        let mut scene = Scene::from_page(doc.page(page)?);
        let fonts = Fonts::shared();
        let outlines_missed = Rc::new(Cell::new(false));
        install_text_outlines(&mut scene, &fonts, &outlines_missed);
        // Изменения, накопленные до редактора, уже в сцене.
        for (p, _) in doc.pages() {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            measures: Vec::new(),
            measure_cache: None,
            fonts: Fonts::shared(),
            text_edit: None,
            edit_layout_cache: RefCell::new(None),
````

на:

````rust
            measures: Vec::new(),
            measure_cache: None,
            fonts,
            text_edit: None,
            edit_layout_cache: RefCell::new(None),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            text_hover: false,
            vector_edit: None,
        })
    }

````

на:

````rust
            text_hover: false,
            vector_edit: None,
            outlines_generation: 0,
            outlines_missed,
        })
    }

    /// Шрифты сменились (пришли байты, отказ в загрузке) — булевы группы с
    /// текстом пересчитываются с новыми контурами глифов. Вызывается с каждым
    /// `emit`; ядро зовёт его и после загрузки шрифта.
    /// Так же пересчитываются группы, чей текст застал реестр занятым.
    pub fn sync_text_outlines(&mut self) {
        // Реестр занят — дождаться следующего вызова.
        let Ok(generation) = self.fonts.try_borrow().map(|f| f.generation()) else {
            return;
        };
        if generation != self.outlines_generation || self.outlines_missed.get() {
            self.outlines_generation = generation;
            self.outlines_missed.set(false);
            self.scene.refresh_text_outlines();
        }
    }

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        let pd = self.doc.page(page)?;
        self.scene = Scene::from_page(pd);
        self.scene_epoch += 1;
        self.page = page;
````

на:

````rust
        let pd = self.doc.page(page)?;
        self.scene = Scene::from_page(pd);
        install_text_outlines(&mut self.scene, &self.fonts, &self.outlines_missed);
        self.scene_epoch += 1;
        self.page = page;
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                    if let Ok(pd) = self.doc.page(restore.page) {
                        self.scene = Scene::from_page(pd);
                        self.scene_epoch += 1;
                        self.page = restore.page;
````

на:

````rust
                    if let Ok(pd) = self.doc.page(restore.page) {
                        self.scene = Scene::from_page(pd);
                        install_text_outlines(&mut self.scene, &self.fonts, &self.outlines_missed);
                        self.scene_epoch += 1;
                        self.page = restore.page;
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        // Правимый вектор изменился (запись, отмена, панель) — сеть заново.
        self.resync_vector_edit();
        self.update_measures();
        let now = Seen {
````

на:

````rust
        // Правимый вектор изменился (запись, отмена, панель) — сеть заново.
        self.resync_vector_edit();
        self.sync_text_outlines();
        self.update_measures();
        let now = Seen {
````

В `crates/ferrum-editor/src/flatten.rs` заменить:

````rust
//! узлов сохраняют свои заливки цветом. Группа с заблокированным слоем не
//! сплющивается; скрытые слои группы в вектор не попадают (как в Figma).

use crate::editor::{Editor, EditorEvent};
````

на:

````rust
//! узлов сохраняют свои заливки цветом. Группа с заблокированным слоем не
//! сплющивается; скрытые слои группы в вектор не попадают (как в Figma).
//! Этап 4: булева группа сплющивается в свой результат (дети уходят вместе с
//! ней), текст — в контуры глифов (с подчёркиванием и зачёркиванием); оба — новым
//! вектором на своём месте.

use crate::editor::{Editor, EditorEvent};
````

В `crates/ferrum-editor/src/flatten.rs` заменить:

````rust
use crate::selection::locked;
use crate::vector_edit::{as_vector, convert_to_vector};
use crate::vector_ops::{merge_networks, reframe, shape_network};
use ferrum_doc::{FillKind, NodeId, NodeKind, Props};

/// Сплющивается ли узел этого типа.
````

на:

````rust
use crate::selection::locked;
use crate::vector_edit::{as_vector, convert_to_vector};
use crate::vector_ops::{merge_networks, path_network, reframe, shape_network};
use ferrum_doc::{FillKind, FillRule, NodeId, NodeKind, Props, VectorNetwork};
use ferrum_scene::Slot;

/// Сплющивается ли узел этого типа.
````

В `crates/ferrum-editor/src/flatten.rs` заменить:

````rust
    matches!(
        k,
        NodeKind::Vector | NodeKind::Rect | NodeKind::Ellipse | NodeKind::Line
    )
}

impl Editor {
    /// Сплющивает выделение в один вектор.
    pub(crate) fn flatten(&mut self) {
````

на:

````rust
    matches!(
        k,
        NodeKind::Vector
            | NodeKind::Rect
            | NodeKind::Ellipse
            | NodeKind::Line
            | NodeKind::Boolean
            | NodeKind::Text
    )
}

impl Editor {
    /// Сеть узла в координатах его формы: фигура и вектор — `shape_network`,
    /// булева группа — её результат, текст — контуры глифов (`None`, пока
    /// шрифты грузятся).
    pub(crate) fn node_network(&self, slot: Slot) -> Option<VectorNetwork> {
        let n = self.scene.at(slot);
        match n.props.kind {
            NodeKind::Boolean => {
                let b = n.boolean.as_ref()?;
                Some(path_network(&b.contours.to_path(), FillRule::NonZero))
            }
            NodeKind::Text => {
                let mut fonts = self.fonts.try_borrow_mut().ok()?;
                let path = ferrum_render::text_outline(&mut fonts, &n.props)?;
                Some(path_network(&path, FillRule::NonZero))
            }
            _ => shape_network(&n.props),
        }
    }

    /// Сплющивает выделение в один вектор.
    pub(crate) fn flatten(&mut self) {
````

В `crates/ferrum-editor/src/flatten.rs` заменить:

````rust
                    match n.props.kind {
                        NodeKind::Group => stack.extend(n.children.iter().rev()),
                        k if flattenable(k) => inner.push((self.scene.order_key(s), n.id)),
                        _ => foreign = true,
                    }
                }
                // Группу с текстом, изображением или фреймом сплющить целиком нельзя:
                // их пришлось бы потерять вместе с группой.
                if foreign || inner.is_empty() {
````

на:

````rust
                    match n.props.kind {
                        NodeKind::Group => stack.extend(n.children.iter().rev()),
                        // Булева группа внутри группы — её результатом.
                        k if flattenable(k) => inner.push((self.scene.order_key(s), n.id)),
                        _ => foreign = true,
                    }
                }
                // Группу с изображением или фреймом сплющить целиком нельзя:
                // их пришлось бы потерять вместе с группой.
                if foreign || inner.is_empty() {
````

В `crates/ferrum-editor/src/flatten.rs` заменить:

````rust
            if !skipped {
                self.out.events.push(EditorEvent::Error(
                    "Нечего сплющить: сплющиваются векторы, прямоугольники, эллипсы, линии и их группы"
                        .into(),
                ));
````

на:

````rust
            if !skipped {
                self.out.events.push(EditorEvent::Error(
                    "Нечего сплющить: сплющиваются векторы, фигуры, текст, булевы группы и их группы"
                        .into(),
                ));
````

В `crates/ferrum-editor/src/flatten.rs` заменить:

````rust
                let n = self.scene.at(top_slot);
                let props = n.props.clone();
                let Some(net) = shape_network(&props) else {
                    return;
                };
                if props.kind == NodeKind::Vector {
                    return;
                }
                let patch = reframe(&props, net);
                let id = *one;
                self.ctx()
                    .commit(|tx, _| Ok(vec![convert_to_vector(tx, id, &patch)?]));
````

на:

````rust
                let n = self.scene.at(top_slot);
                let props = n.props.clone();
                if props.kind == NodeKind::Vector {
                    return;
                }
                let Some(net) = self.node_network(top_slot) else {
                    self.out.events.push(EditorEvent::Error(
                        "Шрифты ещё загружаются — текст не сплющен".into(),
                    ));
                    return;
                };
                let id = *one;
                // Булева группа и текст — новым вектором на своём месте (у них
                // есть дети или содержимое, которые вектору не нужны).
                if matches!(props.kind, NodeKind::Boolean | NodeKind::Text) {
                    let mut base = props.clone();
                    as_vector(&mut base);
                    let patch = reframe(&base, net);
                    patch.apply_to(&mut base);
                    let parent = n.parent.map(|p| self.scene.at(p).id);
                    let index = self.scene.index(top_slot) as usize;
                    self.ctx().commit(|tx, _| {
                        tx.delete_nodes(&[id])?;
                        Ok(vec![tx.create_with_props(parent, Some(index), &base)?])
                    });
                    return;
                }
                let patch = reframe(&props, net);
                self.ctx()
                    .commit(|tx, _| Ok(vec![convert_to_vector(tx, id, &patch)?]));
````

В `crates/ferrum-editor/src/flatten.rs` заменить:

````rust
        let mut parts = Vec::new();
        for id in &leaves {
            let n = self.scene.node(*id).expect("в сцене");
            let Some(mut net) = shape_network(&n.props) else {
                continue;
            };
            // Заливки узла, отличные от заливок результата, остаются у его областей.
````

на:

````rust
        let mut parts = Vec::new();
        for id in &leaves {
            let slot = self.scene.slot(*id).expect("в сцене");
            let n = self.scene.at(slot);
            // Текст без шрифтов не терять: ничего не сплющивается.
            let Some(mut net) = self.node_network(slot) else {
                self.out.events.push(EditorEvent::Error(
                    "Шрифты ещё загружаются — текст не сплющен".into(),
                ));
                return;
            };
            // Заливки узла, отличные от заливок результата, остаются у его областей.
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod measure;
mod ops;
pub mod pages;
pub mod panel;
````

на:

````rust
pub mod measure;
mod ops;
mod outline;
pub mod pages;
pub mod panel;
````

Создать `crates/ferrum-editor/src/outline.rs`:

````rust
//! «Контур обводки» (Ctrl/Cmd+Alt+O, спека подсистемы 3, раздел 10): обводки
//! выделенных слоёв становятся вектором, чья заливка — краска обводки, одним
//! шагом отмены.
//!
//! Вектор встаёт прямо над слоем, с его именем и позой; у каждой видимой
//! обводки — своя область своего цвета (верхняя обводка — верхняя область).
//! Слой с видимыми заливками остаётся без обводок, без заливок — удаляется (как
//! в Figma, где от такого слоя остаётся только контур). Обводка по центру
//! сохраняет кривые; внутри и снаружи — обрезка контуром формы булевой операцией
//! (ломаные с точностью `BOOLEAN_TOLERANCE`).

use ferrum_doc::{BooleanOp, Fill, FillRule, NodeId, NodeKind, Props, Stroke, StrokeAlign};
use ferrum_scene::boolean::{combine, flatten_path, normalize, BOOLEAN_TOLERANCE};
use ferrum_scene::vector::{self as vpath, StrokeClip};
use ferrum_scene::Slot;
use kurbo::{Affine, BezPath, Join};

use crate::editor::{Editor, EditorEvent};
use crate::selection::locked;
use crate::vector_edit::as_vector;
use crate::vector_ops::{merge_networks, path_network, reframe};

/// Видимая обводка: ненулевая толщина и не прозрачный цвет.
fn visible(s: &Stroke) -> bool {
    s.width > 0.0 && s.color.a > 0
}

/// Обрезка контура `outline` формой `shape` (оба — пути в координатах формы).
fn clip(outline: &BezPath, shape: &BezPath, inside: bool) -> BezPath {
    let o = normalize(flatten_path(outline, BOOLEAN_TOLERANCE), FillRule::NonZero);
    let s = normalize(flatten_path(shape, BOOLEAN_TOLERANCE), FillRule::NonZero);
    let op = if inside {
        BooleanOp::Intersect
    } else {
        BooleanOp::Subtract
    };
    combine(op, &[o, s]).to_path()
}

impl Editor {
    /// Контур одной обводки узла `slot` в координатах его формы и правило заливки.
    fn stroke_outline(&self, slot: Slot, s: &Stroke) -> Option<(BezPath, FillRule)> {
        let n = self.scene.at(slot);
        let p = &n.props;
        if p.kind == NodeKind::Text {
            // Глифы: по центру — как есть, внутри и снаружи — двойной толщиной с
            // обрезкой по глифам (как рисует рендер).
            let glyphs = {
                let mut fonts = self.fonts.try_borrow_mut().ok()?;
                ferrum_render::text_outline(&mut fonts, p)?
            };
            let w = match s.align {
                StrokeAlign::Center => s.width,
                _ => 2.0 * s.width,
            };
            let k = kurbo::Stroke::new(w).with_join(Join::Miter);
            let out = kurbo::stroke(
                glyphs.iter(),
                &k,
                &kurbo::StrokeOpts::default(),
                ferrum_scene::shape::PATH_TOLERANCE,
            );
            return Some(match s.align {
                StrokeAlign::Center => (out, FillRule::NonZero),
                a => (
                    clip(&out, &glyphs, a == StrokeAlign::Inside),
                    FillRule::NonZero,
                ),
            });
        }
        if let Some(ps) = n.path_shape() {
            let out = vpath::stroke_outline(ps, s, &p.stroke_style);
            return Some(match vpath::stroke_clip(ps, s) {
                StrokeClip::None => (out, FillRule::NonZero),
                c => (
                    clip(&out, &ps.stroke, c == StrokeClip::Inside),
                    FillRule::NonZero,
                ),
            });
        }
        // Кольцо фигуры (прямоугольник, эллипс, линия) — уже с положением.
        n.geometry().stroke_ring(s).map(|r| (r, FillRule::EvenOdd))
    }

    /// Обводки выделения — в векторы (одна команда — один шаг).
    pub(crate) fn outline_stroke(&mut self) {
        self.finish_vector_edit();
        let ids = self.ordered_selection();
        let mut jobs: Vec<(NodeId, Props, bool)> = Vec::new();
        let mut loading = false;
        for id in ids {
            let Some(slot) = self.scene.slot(id) else {
                continue;
            };
            if locked(&self.scene, slot) || !self.scene.is_visible(slot) {
                continue;
            }
            let p = self.scene.at(slot).props.clone();
            // Своей обводки по форме нет у группы; обводка фрейма — рамка
            // монтажной области, её контур не строится (как в Figma).
            if matches!(p.kind, NodeKind::Group | NodeKind::Frame) {
                continue;
            }
            let mut parts = Vec::new();
            for s in p.strokes.iter().filter(|s| visible(s)) {
                let Some((path, rule)) = self.stroke_outline(slot, s) else {
                    loading |= p.kind == NodeKind::Text;
                    continue;
                };
                let mut net = path_network(&path, rule);
                for r in &mut net.regions {
                    r.fills = Some(vec![Fill::solid(s.color)]);
                }
                if !net.regions.is_empty() {
                    parts.push((net, Affine::IDENTITY));
                }
            }
            if parts.is_empty() {
                continue;
            }
            let mut base = p.clone();
            as_vector(&mut base);
            base.fills = Vec::new();
            base.strokes = Vec::new();
            base.stroke_style = Default::default();
            let patch = reframe(&base, merge_networks(&parts));
            patch.apply_to(&mut base);
            let keep = p.fills.iter().any(|f| f.visible && f.opacity > 0.0);
            jobs.push((id, base, keep));
        }
        if jobs.is_empty() {
            let msg = if loading {
                "Шрифты ещё загружаются — контур обводки текста не построен"
            } else {
                "У выделенных слоёв нет видимых обводок"
            };
            self.out.events.push(EditorEvent::Error(msg.into()));
            return;
        }
        self.ctx().commit(|tx, _| {
            let mut created = Vec::new();
            for (id, props, keep) in &jobs {
                let parent = tx.parent(*id);
                let index = tx
                    .children(parent)
                    .iter()
                    .position(|c| c == id)
                    .map_or(usize::MAX, |i| i + 1);
                created.push(tx.create_with_props(parent, Some(index), props)?);
                if *keep {
                    tx.set_props(
                        *id,
                        &ferrum_doc::PropsPatch {
                            strokes: Some(Vec::new()),
                            ..Default::default()
                        },
                    )?;
                } else {
                    tx.delete_nodes(&[*id])?;
                }
            }
            Ok(created)
        });
    }
}
````

В `crates/ferrum-editor/src/text.rs` заменить:

````rust
    pub fn set_fonts(&mut self, fonts: SharedFonts) {
        self.fonts = fonts;
    }

````

на:

````rust
    pub fn set_fonts(&mut self, fonts: SharedFonts) {
        self.fonts = fonts;
        crate::editor::install_text_outlines(&mut self.scene, &self.fonts, &self.outlines_missed);
    }

````

В `crates/ferrum-editor/src/vector_edit.rs` заменить:

````rust
    p.clip_content = false;
    p.text = None;
    p.vector = Some(Arc::new(VectorNetwork::default()));
}
````

на:

````rust
    p.clip_content = false;
    p.text = None;
    p.boolean_op = None;
    p.vector = Some(Arc::new(VectorNetwork::default()));
}
````

В `crates/ferrum-editor/src/vector_ops.rs` заменить:

````rust
use ferrum_scene::vector::{fit, network_bounds, segment_curve};
use kurbo::{
    Affine, BezPath, CubicBez, ParamCurve, ParamCurveNearest, PathSeg, Point, Rect, Shape, Vec2,
};

````

на:

````rust
use ferrum_scene::vector::{fit, network_bounds, segment_curve};
use kurbo::{
    Affine, BezPath, CubicBez, ParamCurve, ParamCurveNearest, PathEl, PathSeg, Point, Rect, Shape,
    Vec2,
};

````

В `crates/ferrum-editor/src/vector_ops.rs` заменить:

````rust
        }));
    }
    out
}
````

на:

````rust
        }));
    }
    out
}

/// Путь → сеть (результат булевой группы, контуры глифов, контур обводки):
/// каждый подпуть — цепочка точек и сегментов (квадратичные кривые — кубическими),
/// замкнутые подпути — петли одной области с правилом `rule` и заливками узла.
/// Отрезки нулевой длины пропускаются; подпуть меньше двух точек — тоже.
pub fn path_network(path: &BezPath, rule: FillRule) -> VectorNetwork {
    let mut net = VectorNetwork::default();
    let mut loops = Vec::new();
    // Точки и сегменты текущего подпути: (конечная точка, ручки).
    let mut start: Option<Point> = None;
    let mut sub: Vec<(Point, [f64; 2], [f64; 2])> = Vec::new();
    let mut flush = |start: Point,
                     sub: &mut Vec<(Point, [f64; 2], [f64; 2])>,
                     closed: bool,
                     net: &mut VectorNetwork| {
        let mut items = std::mem::take(sub);
        // Замыкающий отрезок — тот же, что `close_path` дорисует сам.
        if closed {
            match items.last() {
                Some((p, _, _)) if (*p - start).hypot() < 1e-9 => {}
                _ => items.push((start, [0.0; 2], [0.0; 2])),
            }
        }
        if items.is_empty() || (closed && items.len() < 2) {
            return;
        }
        let first = net.points.len() as u32;
        net.points.push(VectorPoint::new(start.x, start.y));
        let n = items.len();
        let mut seg_ids = Vec::with_capacity(n);
        for (k, (p, ts, te)) in items.into_iter().enumerate() {
            let from = net.points.len() as u32 - 1;
            let to = if closed && k == n - 1 {
                first
            } else {
                net.points.push(VectorPoint::new(p.x, p.y));
                net.points.len() as u32 - 1
            };
            seg_ids.push(net.segments.len() as u32);
            net.segments.push(VectorSegment {
                start: from,
                end: to,
                tangent_start: ts,
                tangent_end: te,
            });
        }
        if closed {
            loops.push(seg_ids);
        }
    };
    let mut last = Point::ZERO;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                if let Some(s) = start.take() {
                    flush(s, &mut sub, false, &mut net);
                }
                start = Some(p);
                last = p;
            }
            PathEl::LineTo(p) => {
                if (p - last).hypot() > 1e-9 {
                    sub.push((p, [0.0; 2], [0.0; 2]));
                    last = p;
                }
            }
            PathEl::QuadTo(c, p) => {
                let c = kurbo::QuadBez::new(last, c, p).raise();
                sub.push((p, arr(c.p1 - c.p0), arr(c.p2 - c.p3)));
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                sub.push((p, arr(a - last), arr(b - p)));
                last = p;
            }
            PathEl::ClosePath => {
                if let Some(s) = start.take() {
                    flush(s, &mut sub, true, &mut net);
                    last = s;
                }
            }
        }
    }
    if let Some(s) = start.take() {
        flush(s, &mut sub, false, &mut net);
    }
    if !loops.is_empty() {
        net.regions.push(VectorRegion {
            rule,
            loops,
            fills: None,
        });
    }
    net
}
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
};
pub use text::{Label, TextEngine, ONEST};
pub use text_node::{TEXT_GREEK_ALPHA, TEXT_LAYOUT_TIME_PER_FRAME, TEXT_LOD_PX};
#[cfg(feature = "gpu")]
pub use vello_common::{multi_atlas::AtlasConfig, paint::ImageId};
````

на:

````rust
};
pub use text::{Label, TextEngine, ONEST};
pub use text_node::{text_outline, TEXT_GREEK_ALPHA, TEXT_LAYOUT_TIME_PER_FRAME, TEXT_LOD_PX};
#[cfg(feature = "gpu")]
pub use vello_common::{multi_atlas::AtlasConfig, paint::ImageId};
````

В `crates/ferrum-render/src/text_node.rs` заменить:

````rust
        }
    }
    out
}

/// Заливка глифов прогона краской `paint`. Изображение — по рамке узла `area`;
````

на:

````rust
        }
    }
    out
}

/// Контуры текста узла в его локальных координатах — глифы всех прогонов и
/// прямоугольники подчёркивания и зачёркивания (спека подсистемы 3, раздел 10:
/// булевы операции, «Сплющить» и «Контур обводки» текста). `None` — шрифты ещё
/// грузятся (раскладка не окончательная) или узел не текст.
pub fn text_outline(fonts: &mut Fonts, p: &Props) -> Option<BezPath> {
    let t = p.text.as_deref()?;
    let layout = fonts.layout(&ferrum_text::LayoutRequest::for_node(t, &p.fills, p.w, p.h));
    if layout.loading {
        return None;
    }
    let mut out = glyphs_path(fonts, layout.runs.iter());
    for d in &layout.decorations {
        out.extend(d.rect.path_elements(0.1));
    }
    Some(out)
}

/// Заливка глифов прогона краской `paint`. Изображение — по рамке узла `area`;
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: clippy wasm32 без замечаний; `outline` — 9 тестов, `keymap` — 53 команды; рабочее пространство — 873 теста, все зелёные

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/flatten.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/outline.rs crates/ferrum-editor/src/text.rs crates/ferrum-editor/src/vector_edit.rs crates/ferrum-editor/src/vector_ops.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/outline.rs crates/ferrum-render/src/lib.rs crates/ferrum-render/src/text_node.rs
git commit -m "feat(editor): «Сплющить» булевой группы и текста (контуры глифов), «Контур обводки» (Ctrl/Cmd+Alt+O), текст в булевых группах по контурам глифов реестра шрифтов (занятый реестр — пересчёт на следующем кадре)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6: Протокол и ядро: строка слоя, сводка, шрифты

Спека 8 и 10: тип строки слоя `boolean` с операцией, сводка выделения с операцией, команды — обычный `Execute`. Здесь — `BooleanOpDto`, `LayerRow.booleanOp`, `Summary.booleanOp` (только выделение из булевых групп с одной операцией), пересчёт булевых групп с текстом по приходу шрифтов (`after_fonts_changed` → `sync_text_outlines`).

**Files:**
- Modify: `crates/ferrum-wasm/src/{summary,layers}.rs`, `crates/ferrum-wasm/src/core/fonts.rs`
- Test: `crates/ferrum-wasm/tests/boolean.rs` (новый), `crates/ferrum-wasm/tests/protocol.rs`, `crates/ferrum-editor/tests/outline.rs`

**Interfaces:**
- Consumes: `Editor::sync_text_outlines`, `CommandId::Boolean*`, `CommandId::OutlineStroke`.
- Produces: `BooleanOpDto` (`union|subtract|intersect|exclude`), `LayerRow.booleanOp`, `Summary.booleanOp`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-editor/tests/outline.rs` заменить:

````rust
    assert!(area(&h) > 500.0, "глифы в результате: {}", area(&h));
}
````

на:

````rust
    assert!(area(&h) > 500.0, "глифы в результате: {}", area(&h));
}

#[test]
fn boolean_with_text_is_recomputed_when_fonts_arrive() {
    use ferrum_text::{group, read_faces, Fonts, SourceKind};
    // Реестр знает вшитый Inter, но байтов ещё нет.
    let fonts = Fonts::shared();
    let files = [
        "inter/Inter-Variable.ttf",
        "inter/Inter-Italic-Variable.ttf",
    ];
    {
        let mut f = fonts.borrow_mut();
        let faces = files
            .iter()
            .flat_map(|r| read_faces(&font_file(r), r))
            .collect::<Vec<_>>();
        f.add_catalog(SourceKind::Bundled, group(faces));
    }
    let mut h = H::new();
    h.ed.set_fonts(fonts.clone());
    let t = h.add(None, NodeKind::Text, patch(100.0, 100.0, 200.0, 50.0));
    h.ed.transact(|tx| {
        tx.set_text_content(t, &TextContent::plain("Ok"))?;
        Ok(vec![])
    })
    .unwrap();
    let r = h.rect(None, 400.0, 100.0, 20.0, 20.0);
    h.ed.select(&[t, r]);
    h.key("KeyU", alt_shift());
    let g = h.sel()[0];
    let area = |h: &mut H| {
        h.ed.scene()
            .node(g)
            .unwrap()
            .boolean
            .as_ref()
            .unwrap()
            .contours
            .to_path()
            .area()
            .abs()
    };
    assert!(
        (area(&mut h) - 400.0).abs() < 1e-6,
        "пока только прямоугольник"
    );
    // Байты пришли (поколение реестра сменилось) — следующий кадр с глифами.
    {
        let mut f = fonts.borrow_mut();
        for rel in files {
            f.load("Inter", rel, font_file(rel)).unwrap();
        }
    }
    h.hover(10.0, 10.0);
    // «Ok» кеглем по умолчанию (12) — десятки квадратных пикселей.
    assert!(area(&mut h) > 420.0, "глифы в результате: {}", area(&mut h));
}
````

Создать `crates/ferrum-wasm/tests/boolean.rs`:

````rust
//! Булевы группы в ядре worker'а (подсистема 3, этап 4): команды раскладки
//! через протокол, строка слоя с операцией, сводка выделения с операцией и
//! смена операции выделенной группы.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_wasm::core::Core;
use ferrum_wasm::layers::NodeType;
use ferrum_wasm::protocol::*;
use ferrum_wasm::summary::BooleanOpDto;
use kurbo::Size;

fn core() -> Core {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::Demo { nodes: 0 });
    c.take_events();
    c
}

fn drag(c: &mut Core, from: (f64, f64), to: (f64, f64), t: f64) {
    let ev = |p: (f64, f64), dt: f64| PointerEvent {
        point: p.into(),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t + dt,
    };
    c.handle(Input::PointerDown(ev(from, 0.0)));
    c.handle(Input::PointerMove(ev(to, 10.0)));
    c.handle(Input::PointerUp(ev(to, 20.0)));
}

fn summary(ev: &[CoreEvent]) -> Option<Summary> {
    ev.iter().rev().find_map(|e| match e {
        CoreEvent::Properties { summary } => Some(summary.clone()),
        _ => None,
    })
}

fn run(c: &mut Core, command: &str) -> Vec<CoreEvent> {
    c.command(Command::Execute {
        command: command.into(),
    });
    c.take_events()
}

/// Два прямоугольника инструментом R, оба выделены. Зум демо — 0,5: в мире
/// это 200 × 200 с перекрытием 100.
fn two_rects(c: &mut Core) {
    for (i, x) in [100.0, 150.0].into_iter().enumerate() {
        c.command(Command::SetTool { tool: Tool::Rect });
        drag(c, (x, 100.0), (x + 100.0, 200.0), 1000.0 * (i + 1) as f64);
    }
    let ids: Vec<String> = c
        .editor()
        .scene()
        .iter()
        .map(|(_, n)| n.id.to_string())
        .collect();
    c.command(Command::Select {
        ids,
        mode: SelectMode::Replace,
    });
    c.take_events();
}

#[test]
fn boolean_commands_reach_the_editor_and_the_summary_shows_the_op() {
    let mut c = core();
    two_rects(&mut c);
    let ev = run(&mut c, "object.boolean_subtract");
    let s = summary(&ev).expect("сводка");
    assert_eq!(s.kind, Some(NodeType::Boolean));
    assert_eq!(s.boolean_op, Some(BooleanOpDto::Subtract));
    assert_eq!(s.name.as_deref(), Some("Вычитание"));
    assert_eq!(s.w.value, 100.0, "рамка — по результату");
    // Та же команда на выделенной группе меняет операцию.
    let ev = run(&mut c, "object.boolean_union");
    let s = summary(&ev).expect("сводка");
    assert_eq!(s.boolean_op, Some(BooleanOpDto::Union));
    assert_eq!(s.w.value, 300.0);
    // Строка слоя несёт операцию для иконки.
    c.command(Command::LayersWindow {
        start: 0,
        count: 10,
    });
    let ev = c.take_events();
    let rows = ev
        .iter()
        .rev()
        .find_map(|e| match e {
            CoreEvent::Layers { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .expect("строки слоёв");
    let row = rows.iter().find(|r| r.kind == NodeType::Boolean).unwrap();
    assert_eq!(row.boolean_op, Some(BooleanOpDto::Union));
    assert!(rows
        .iter()
        .filter(|r| r.kind == NodeType::Rect)
        .all(|r| r.boolean_op.is_none()));
}

#[test]
fn boolean_and_outline_commands_are_in_the_keymap() {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    let ev = c.take_events();
    let commands = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::Keymap { commands, .. } => Some(commands.clone()),
            _ => None,
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 53);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
        ("object.boolean_subtract", "Alt+Shift+KeyS"),
        ("object.boolean_intersect", "Alt+Shift+KeyI"),
        ("object.boolean_exclude", "Alt+Shift+KeyE"),
        ("object.outline_stroke", "Ctrl+Alt+KeyO"),
    ] {
        let e = commands.iter().find(|e| e.key == key).expect(key);
        assert_eq!(e.bindings, vec![binding.to_string()], "{key}");
    }
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
                id: "1@2".into(),
                name: "Фон".into(),
                kind: NodeType::Rect,
                depth: 1,
                has_children: false,
````

на:

````rust
                id: "1@2".into(),
                name: "Фон".into(),
                kind: NodeType::Boolean,
                depth: 1,
                has_children: false,
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
                ancestor_hidden: false,
                ancestor_locked: true,
            }],
            reveal: Some(0),
````

на:

````rust
                ancestor_hidden: false,
                ancestor_locked: true,
                boolean_op: Some(ferrum_wasm::summary::BooleanOpDto::Subtract),
            }],
            reveal: Some(0),
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
    assert_eq!(row["hasChildren"], json!(false));
    assert_eq!(row["ancestorLocked"], json!(true));
    let s = serde_json::to_value(Summary::empty()).unwrap();
    assert_eq!(s["cornerRadius"], json!(null));
    assert_eq!(s["opacity"], json!({"value": 1.0, "mixed": false}));
}
````

на:

````rust
    assert_eq!(row["hasChildren"], json!(false));
    assert_eq!(row["ancestorLocked"], json!(true));
    assert_eq!(row["kind"], json!("boolean"));
    assert_eq!(row["booleanOp"], json!("subtract"));
    let s = serde_json::to_value(Summary::empty()).unwrap();
    assert_eq!(s["cornerRadius"], json!(null));
    assert_eq!(s["booleanOp"], json!(null));
    assert_eq!(s["opacity"], json!({"value": 1.0, "mixed": false}));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test boolean`
Expected: FAIL — не собирается: нет `ferrum_wasm::summary::BooleanOpDto`, полей `LayerRow::boolean_op` и `Summary::boolean_op`

- [ ] **Step 3: Реализация**

В `crates/ferrum-wasm/src/core/fonts.rs` заменить:

````rust
    fn after_fonts_changed(&mut self) {
        self.dirty = true;
        let waiting: Vec<Waiting> = self.font_state.waiting.drain(..).collect();
        self.font_state.waiting_since = None;
````

на:

````rust
    fn after_fonts_changed(&mut self) {
        self.dirty = true;
        // Булевы группы с текстом — по новым контурам глифов.
        self.editor.sync_text_outlines();
        let waiting: Vec<Waiting> = self.font_state.waiting.drain(..).collect();
        self.font_state.waiting_since = None;
````

В `crates/ferrum-wasm/src/layers.rs` заменить:

````rust
    pub ancestor_hidden: bool,
    pub ancestor_locked: bool,
}

````

на:

````rust
    pub ancestor_hidden: bool,
    pub ancestor_locked: bool,
    /// Операция булевой группы — для иконки строки (у других узлов нет).
    pub boolean_op: Option<crate::summary::BooleanOpDto>,
}

````

В `crates/ferrum-wasm/src/layers.rs` заменить:

````rust
                    ancestor_hidden,
                    ancestor_locked,
                })
            })
````

на:

````rust
                    ancestor_hidden,
                    ancestor_locked,
                    boolean_op: n.props.boolean_op.map(Into::into),
                })
            })
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
}

/// Соединение сегментов обводки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
````

на:

````rust
}

/// Булева операция (спека подсистемы 3, раздел 10): строка слоя (иконка) и
/// сводка выделения (отмеченный пункт списка тулбара).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum BooleanOpDto {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

impl From<ferrum_doc::BooleanOp> for BooleanOpDto {
    fn from(op: ferrum_doc::BooleanOp) -> BooleanOpDto {
        use ferrum_doc::BooleanOp as B;
        match op {
            B::Union => BooleanOpDto::Union,
            B::Subtract => BooleanOpDto::Subtract,
            B::Intersect => BooleanOpDto::Intersect,
            B::Exclude => BooleanOpDto::Exclude,
        }
    }
}

/// Соединение сегментов обводки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
    /// Окончания и соединения обводки (у групп и текста их нет).
    pub stroke_style: Option<StrokeStyleDto>,
}

````

на:

````rust
    /// Окончания и соединения обводки (у групп и текста их нет).
    pub stroke_style: Option<StrokeStyleDto>,
    /// Операция, если выделены только булевы группы с одной операцией.
    pub boolean_op: Option<BooleanOpDto>,
}

````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
            strokes: None,
            stroke_style: None,
        }
    }
````

на:

````rust
            strokes: None,
            stroke_style: None,
            boolean_op: None,
        }
    }
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
    let (mut ends, mut corners) = (false, false);
    let mut name = None;
    for id in ids {
        let Some(slot) = scene.slot(*id) else {
````

на:

````rust
    let (mut ends, mut corners) = (false, false);
    let mut name = None;
    let mut op: Acc<Option<ferrum_doc::BooleanOp>> = Acc::default();
    for id in ids {
        let Some(slot) = scene.slot(*id) else {
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
        count += 1;
        kind.add(p.kind);
        let pose = panel_pose(scene, slot);
        x.add_num(pose.x);
````

на:

````rust
        count += 1;
        kind.add(p.kind);
        op.add(p.boolean_op);
        let pose = panel_pose(scene, slot);
        x.add_num(pose.x);
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
            }),
        }),
    }
}
````

на:

````rust
            }),
        }),
        boolean_op: op.first.flatten().filter(|_| !op.mixed).map(Into::into),
    }
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm -p ferrum-editor 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Expected: PASS: `boolean` ядра — 2 теста, `protocol` — 5, `outline` — 10 (пересчёт по приходу шрифтов); wasm собран, `npm run typecheck` без ошибок

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/tests/outline.rs crates/ferrum-wasm/src/core/fonts.rs crates/ferrum-wasm/src/layers.rs crates/ferrum-wasm/src/summary.rs crates/ferrum-wasm/tests/boolean.rs crates/ferrum-wasm/tests/protocol.rs
git commit -m "feat(wasm): булева операция в строке слоя и в сводке выделения, команды булевых операций и «Контура обводки» в раскладке; пересчёт булевых групп с текстом по приходу шрифтов

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 7: Интерфейс: список «Булевы операции» и иконки слоёв

Спека 10: команды на выделение из интерфейса. Здесь — кнопка-список тулбара «Булевы операции» после «Текста» (4 операции с отметкой, разделитель, «Сплющить», «Контур обводки»), иконки операций в слоях, E2E булевых групп (Chromium и Firefox).

**Files:**
- Modify: `apps/web/src/ui/{toolbar,layers,icons}.tsx`
- Test: `apps/web/e2e/booleans.spec.ts` (новый), `apps/web/e2e/app.spec.ts` (команд 48 → 53)

**Interfaces:**
- Consumes: `LayerRow.booleanOp`, `Summary.booleanOp`, команды раскладки `object.boolean_*`, `object.flatten`, `object.outline_stroke`.
- Produces: иконки `union|subtract|intersect|exclude`; кнопка-список «Булевы операции» (`menuitemcheckbox`, `aria-pressed` у выделенной булевой группы).

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 48, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 53, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

Создать `apps/web/e2e/booleans.spec.ts`:

````ts
// Булевы группы (подсистема 3, этап 4): Alt+Shift+U (как в Figma) одним шагом, список
// «Булевы операции» в тулбаре (новая группа и смена операции выделенной), строка
// слоя с иконкой операции и детьми, правка ребёнка с живым результатом,
// «Сплющить» группы. Документ тест читает сводкой панели свойств и строками
// слоёв, вид — пикселями экрана.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, live, liveWhen, openApp, pixel } from './helpers';

type LiveState = Awaited<ReturnType<typeof live>>;

// Чтений layout вне фазы measure планировщика не было (тестовый режим их копит).
test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

const summary = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)));
const rows = (page: Page) => page.getByRole('tree', { name: 'Слои' }).getByRole('treeitem');

/** Пустой файл на белом фоне при зуме 100 % (пиксель экрана — единица мира), фокус на холсте. */
async function blank(page: Page): Promise<LiveState> {
  await openApp(page, 0);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setPageBackground', color: '#ffffff', phase: 'commit' }));
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+Digit0');
  return liveWhen(page, 'l.zoom === 1');
}

/** Два серых квадрата 100 × 100 с перекрытием 50 по x (левый — от cameraX − 100), оба выделены. */
async function twoRects(page: Page, l: LiveState): Promise<void> {
  for (const dx of [-100, -50]) {
    await page.keyboard.press('KeyR');
    await drag(page, await appScreen(page, l, l.cameraX + dx, l.cameraY - 50), 100, 100);
    await liveWhen(page, 'l.busy === 0');
  }
  await page.keyboard.press('ControlOrMeta+a');
  await expect.poll(async () => (await summary(page))?.count).toBe(2);
}

/** Красный канал пикселя мира: 217 — серая заливка, 255 — белый фон. */
async function red(page: Page, l: LiveState, x: number, y: number): Promise<number> {
  const at = await appScreen(page, l, x, y);
  return (await pixel(page, at.x, at.y))[0];
}

test('Alt+Shift+U — объединение одним шагом, слой с детьми, отмена @firefox', async ({ page }) => {
  const l = await blank(page);
  await twoRects(page, l);
  const before = await live(page);
  await page.keyboard.press('Alt+Shift+KeyU');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  const s = await summary(page);
  expect([s.kind, s.booleanOp, s.name, s.w.value, s.h.value]).toEqual(['boolean', 'union', 'Объединение', 150, 100]);
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toHaveAccessibleName('Объединение');
  // Заливка группы — серая (стиль верхнего квадрата), перекрытие тоже серое.
  await expect.poll(() => red(page, l, l.cameraX - 25, l.cameraY)).toBe(217);
  await page.keyboard.press('ControlOrMeta+z');
  await liveWhen(page, `l.nodes === ${before.nodes}`);
  await expect(rows(page)).toHaveCount(2);
});

test('список «Булевы операции»: вычитание, затем смена операции выделенной группы', async ({ page }) => {
  const l = await blank(page);
  await twoRects(page, l);
  const toolbar = page.getByRole('toolbar', { name: 'Инструменты' });
  // Подсказка кнопки — с сочетанием из раскладки: Alt+Shift+U (⌥⇧U на маке).
  await toolbar.getByRole('button', { name: 'Объединение' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText(/^Объединение {2}.*U$/);
  await expect(page.getByRole('tooltip')).not.toHaveText(/Ctrl|⌘/);
  await toolbar.getByRole('button', { name: 'Булевы операции' }).click();
  const menu = page.getByRole('menu', { name: 'Булевы операции' });
  await expect(menu.getByRole('menuitemcheckbox')).toHaveCount(4);
  const before = await live(page);
  await menu.getByRole('menuitemcheckbox', { name: /Вычитание/ }).click();
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect.poll(async () => (await summary(page)).booleanOp).toBe('subtract');
  // Кнопка показывает операцию выделенной группы.
  await expect(toolbar.getByRole('button', { name: 'Вычитание' })).toHaveAttribute('aria-pressed', 'true');
  // Левая половина осталась, правый квадрат вычтен (белый фон).
  await expect.poll(() => red(page, l, l.cameraX - 75, l.cameraY)).toBe(217);
  await expect.poll(() => red(page, l, l.cameraX - 25, l.cameraY)).toBe(255);
  await expect.poll(() => red(page, l, l.cameraX + 25, l.cameraY)).toBe(255);
  // Та же группа, другая операция: слоёв не прибавилось.
  await toolbar.getByRole('button', { name: 'Булевы операции' }).click();
  await menu.getByRole('menuitemcheckbox', { name: /Пересечение/ }).click();
  await liveWhen(page, `l.steps === ${before.steps + 2}`);
  await expect.poll(async () => (await summary(page)).booleanOp).toBe('intersect');
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toHaveAccessibleName('Пересечение');
  await expect.poll(() => red(page, l, l.cameraX - 25, l.cameraY)).toBe(217);
  await expect.poll(() => red(page, l, l.cameraX - 75, l.cameraY)).toBe(255);
});

test('двойной щелчок по ребёнку и перетаскивание — результат меняется одним шагом @firefox', async ({ page }) => {
  const l = await blank(page);
  await twoRects(page, l);
  await page.keyboard.press('Alt+Shift+KeyI');
  await expect.poll(async () => (await summary(page))?.booleanOp).toBe('intersect');
  // Пересечение — полоса −50…0; правее нуля пусто.
  await expect.poll(() => red(page, l, l.cameraX + 25, l.cameraY)).toBe(255);
  const inside = await appScreen(page, l, l.cameraX - 25, l.cameraY);
  await page.mouse.dblclick(inside.x, inside.y);
  await expect.poll(async () => (await summary(page))?.kind).toBe('rect');
  // Пауза дольше двойного щелчка: нажатие перетаскивания — не третий щелчок.
  await page.waitForTimeout(600);
  // Верхний квадрат (−50…50) — влево на 30: пересечение −80…0.
  const before = await live(page);
  await drag(page, inside, -30, 0);
  await liveWhen(page, `l.steps === ${before.steps + 1} && l.busy === 0`);
  await expect.poll(async () => (await summary(page)).x.value).toBe(before.selectionX - 30);
  await expect.poll(() => red(page, l, l.cameraX - 65, l.cameraY)).toBe(217);
  // Отмена — одним шагом: ребёнок на месте, пересечение прежнее.
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(async () => (await summary(page)).x.value).toBe(before.selectionX);
  await expect.poll(() => red(page, l, l.cameraX - 65, l.cameraY)).toBe(255);
});

test('«Сплющить» булевой группы — один вектор с результатом', async ({ page }) => {
  const l = await blank(page);
  await twoRects(page, l);
  await page.keyboard.press('Alt+Shift+KeyE');
  await expect.poll(async () => (await summary(page))?.booleanOp).toBe('exclude');
  const before = await live(page);
  await page.keyboard.press('ControlOrMeta+e');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect.poll(async () => (await summary(page)).kind).toBe('vector');
  const s = await summary(page);
  expect([s.name, s.w.value, s.h.value]).toEqual(['Исключение', 150, 100]);
  await expect(rows(page)).toHaveCount(1);
  // Середина исключения пуста, края — серые.
  await expect.poll(() => red(page, l, l.cameraX - 25, l.cameraY)).toBe(255);
  await expect.poll(() => red(page, l, l.cameraX - 75, l.cameraY)).toBe(217);
  await expect.poll(() => red(page, l, l.cameraX + 25, l.cameraY)).toBe(217);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/booleans.spec.ts --project=chromium`
Expected: FAIL — 1 failed, 3 passed: «список «Булевы операции»: вычитание, затем смена операции…» — `locator.hover: Test timeout of 120000ms exceeded`, ожидание `getByRole('toolbar', { name: 'Инструменты' }).getByRole('button', { name: 'Объединение' })` (кнопки в тулбаре нет); три теста через клавиши и слои уже проходят — команды ядро получило в задаче 6

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/icons.tsx` заменить:

````tsx
  line: '<path d="M22 2 2 22"/>',
  arrow: '<path d="M7 17 17 7"/><path d="M7 7h10v10"/>',
  image: '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
  pen: '<path d="m18 13-.654-4.736a4 4 0 00-2.756-3.266l-9.163-2.9a2 2 0 00-2.017.493l-.82.819a2 2 0 00-.492 2.017l2.9 9.163a4 4 0 003.266 2.756L13 18"/><path d="M18.648 12.352a1.205 1.205 0 011.704 0l1.296 1.296a1.205 1.205 0 010 1.704l-6.296 6.296a1.205 1.205 0 01-1.704 0l-1.296-1.296a1.205 1.205 0 010-1.704z"/><path d="m3 3 6.586 6.586"/><circle cx="11" cy="11" r="2"/>',
````

на:

````tsx
  line: '<path d="M22 2 2 22"/>',
  arrow: '<path d="M7 17 17 7"/><path d="M7 7h10v10"/>',
  // Булевы операции (тулбар и строки слоёв): два квадрата, закрашено то, что
  // остаётся.
  union: '<path d="M4 4h10v6h6v10H10v-6H4z" fill="currentColor" fill-opacity=".35"/>',
  subtract: '<path d="M4 4h10v6h-4v4H4z" fill="currentColor" fill-opacity=".35"/><path d="M14 10h6v10H10v-6" stroke-dasharray="2 2"/>',
  intersect: '<path d="M10 14H4V4h10v6M14 14h6v6H10v-6" stroke-dasharray="2 2"/><rect x="10" y="10" width="4" height="4" fill="currentColor" fill-opacity=".35"/>',
  exclude: '<path d="M4 4h10v6h-4v4H4z" fill="currentColor" fill-opacity=".35"/><path d="M14 10h6v10H10v-6h4z" fill="currentColor" fill-opacity=".35"/>',
  image: '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
  pen: '<path d="m18 13-.654-4.736a4 4 0 00-2.756-3.266l-9.163-2.9a2 2 0 00-2.017.493l-.82.819a2 2 0 00-.492 2.017l2.9 9.163a4 4 0 003.266 2.756L13 18"/><path d="M18.648 12.352a1.205 1.205 0 011.704 0l1.296 1.296a1.205 1.205 0 010 1.704l-6.296 6.296a1.205 1.205 0 01-1.704 0l-1.296-1.296a1.205 1.205 0 010-1.704z"/><path d="m3 3 6.586 6.586"/><circle cx="11" cy="11" r="2"/>',
````

В `apps/web/src/ui/layers.tsx` заменить:

````tsx
  text: 'text',
  vector: 'pen',
  boolean: 'group',
};

/** Окно строк для прокрутки `scrollTop` и высоты `height`. */
````

на:

````tsx
  text: 'text',
  vector: 'pen',
  boolean: 'union',
};

/** Иконка строки: у булевой группы — по её операции. */
function rowIcon(row: LayerRow): IconName {
  return row.kind === 'boolean' && row.booleanOp ? row.booleanOp : KIND_ICON[row.kind];
}

/** Окно строк для прокрутки `scrollTop` и высоты `height`. */
````

В `apps/web/src/ui/layers.tsx` заменить:

````tsx
                </span>
                <span class="fe-layer__icon">
                  <Icon name={KIND_ICON[row().kind]} />
                </span>
                <Show when={renaming()?.id === row().id} fallback={<span class="fe-layer__name">{row().name}</span>}>
````

на:

````tsx
                </span>
                <span class="fe-layer__icon">
                  <Icon name={rowIcon(row())} />
                </span>
                <Show when={renaming()?.id === row().id} fallback={<span class="fe-layer__name">{row().name}</span>}>
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
// Плавающий тулбар внизу по центру холста (раскладка UI3): перемещение, фрейм с
// пресетами, фигуры со стрелкой (последняя выбранная на кнопке), перо, текст и
// отключённые комментарии — с подсказкой, когда они появятся. Рука (H, удержание
// Space) работает с клавиатуры и в тулбар не выносится.

import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import type { Tool } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { Caret, Icon, type IconName } from './icons';
````

на:

````tsx
// Плавающий тулбар внизу по центру холста (раскладка UI3): перемещение, фрейм с
// пресетами, фигуры со стрелкой (последняя выбранная на кнопке), перо, текст,
// булевы операции со стрелкой (как в Figma: операция выделенной булевой группы
// или последняя выбранная на кнопке, в списке — ещё «Сплющить» и «Контур
// обводки») и отключённые комментарии — с подсказкой, когда они появятся. Рука (H, удержание
// Space) работает с клавиатуры и в тулбар не выносится.

import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import type { BooleanOpDto, Tool } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { Caret, Icon, type IconName } from './icons';
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
];

const LATER: { name: string; key: string; when: string; icon: IconName }[] = [
  { name: 'Комментарии', key: 'C', when: 'подсистема 4', icon: 'comment' },
````

на:

````tsx
];

const BOOLEANS: { op: BooleanOpDto; name: string; command: string }[] = [
  { op: 'union', name: 'Объединение', command: 'object.boolean_union' },
  { op: 'subtract', name: 'Вычитание', command: 'object.boolean_subtract' },
  { op: 'intersect', name: 'Пересечение', command: 'object.boolean_intersect' },
  { op: 'exclude', name: 'Исключение', command: 'object.boolean_exclude' },
];

const LATER: { name: string; key: string; when: string; icon: IconName }[] = [
  { name: 'Комментарии', key: 'C', when: 'подсистема 4', icon: 'comment' },
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
  const ed = useEditor();
  const [shape, setShape] = createSignal<Shape>('rect');
  const [menu, setMenu] = createSignal<{ kind: 'frame' | 'shapes'; anchor: HTMLElement } | null>(null);
  let bar!: HTMLDivElement;
  onMount(() => onCleanup(roving(bar, 'button')));
````

на:

````tsx
  const ed = useEditor();
  const [shape, setShape] = createSignal<Shape>('rect');
  const [menu, setMenu] = createSignal<{ kind: 'frame' | 'shapes' | 'boolean'; anchor: HTMLElement } | null>(null);
  // Операция на кнопке: у выделенной булевой группы — её, иначе последняя выбранная.
  const [lastOp, setLastOp] = createSignal<BooleanOpDto>('union');
  const boolean = createMemo(() => {
    const op = ed.store.state.summary?.booleanOp ?? lastOp();
    return BOOLEANS.find((b) => b.op === op)!;
  });
  const hasSelection = () => (ed.store.state.summary?.count ?? 0) > 0;
  const runBoolean = (b: (typeof BOOLEANS)[number]) => {
    setLastOp(b.op);
    ed.run(b.command);
  };
  let bar!: HTMLDivElement;
  onMount(() => onCleanup(roving(bar, 'button')));
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
      },
    }));

  return (
````

на:

````tsx
      },
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

  const menuLabel = (kind: 'frame' | 'shapes' | 'boolean') =>
    kind === 'frame' ? 'Пресеты фреймов' : kind === 'shapes' ? 'Фигуры' : 'Булевы операции';
  const menuItems = (kind: 'frame' | 'shapes' | 'boolean') =>
    kind === 'frame' ? frameItems() : kind === 'shapes' ? shapeItems() : booleanItems();

  return (
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
        <Icon name="text" />
      </button>
      <span class="fe-toolbar__sep" role="separator" />
      <For each={LATER}>
````

на:

````tsx
        <Icon name="text" />
      </button>
      <div class="fe-tool-split">
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
        </button>
        <button
          class="fe-tool fe-tool--caret"
          aria-haspopup="menu"
          aria-expanded={menu()?.kind === 'boolean'}
          aria-label="Булевы операции"
          data-tip="Булевы операции"
          data-tip-side="top"
          onClick={(e) => setMenu({ kind: 'boolean', anchor: e.currentTarget })}
        >
          <Caret />
        </button>
      </div>
      <span class="fe-toolbar__sep" role="separator" />
      <For each={LATER}>
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
        {(m) => (
          <Menu
            label={m().kind === 'frame' ? 'Пресеты фреймов' : 'Фигуры'}
            items={m().kind === 'frame' ? frameItems() : shapeItems()}
            anchor={m().anchor}
            above
````

на:

````tsx
        {(m) => (
          <Menu
            label={menuLabel(m().kind)}
            items={menuItems(m().kind)}
            anchor={m().anchor}
            above
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npx playwright test e2e/booleans.spec.ts e2e/app.spec.ts e2e/chrome.spec.ts e2e/layers.spec.ts e2e/vectors.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npx playwright test e2e/booleans.spec.ts --project=firefox`

Expected: модульные — 134 passed, сборка — ок; E2E `booleans` (4), `app`, `chrome`, `layers`, `vectors` в Chromium — 69 passed; Firefox — 2 passed (тесты `booleans` с @firefox)

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts apps/web/e2e/booleans.spec.ts apps/web/src/ui/icons.tsx apps/web/src/ui/layers.tsx apps/web/src/ui/toolbar.tsx
git commit -m "feat(web): список «Булевы операции» в тулбаре (новая группа и смена операции, «Сплющить», «Контур обводки»), иконки операций в слоях; E2E булевых групп

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 8: Жест над ребёнком большой группы в бюджете кадра, замеры

Спека 10 и 12: булевы на 1000 контуров ≤ 50 мс. Полный пересчёт 1000 кругов — ≈ 20 мс на каждое движение и два при записи жеста (≈ 37 мс). Поэтому: во время жеста над одним ребёнком остальные складываются один раз (`combine_rest`), на движение и при записи — операция только с частями у ребёнка (`combine_with`). Замеры — `render_report` (операции на 1000 кругов, сцена, кадр, правка ребёнка) и `editor_report` (кадр жеста, первое движение, запись, один шаг).

**Files:**
- Modify: `crates/ferrum-scene/src/{boolean,scene}.rs`, `crates/ferrum-render/examples/render_report.rs`, `crates/ferrum-editor/examples/editor_report.rs`
- Test: `crates/ferrum-scene/tests/boolean.rs`

**Interfaces:**
- Produces: `boolean::{Piece, Rest, combine_rest, combine_with}`, `BooleanOutline::is_partial()`; строки `render_report` «Булева операция …: 1000 кругов», «Булева группа 1000 кругов: …», строка `editor_report` «запись перетаскивания ребёнка (отпускание)» с целью ≤ 16,7 мс.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-scene/tests/boolean.rs` заменить:

````rust
    assert!(scene.change_counter() > counter, "нужен новый кадр");
}
````

на:

````rust
    assert!(scene.change_counter() > counter, "нужен новый кадр");
}

/// Сетка 6 × 5 перекрывающихся кругов (по 15, радиус 10) в булевой группе `op`.
fn grid(op: BooleanOp) -> (Document, PageId, NodeId, Vec<NodeId>) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut ids = doc
        .transact(page, vec![], |tx| {
            let b = tx.create_with_props(
                None,
                None,
                &Props {
                    boolean_op: Some(op),
                    ..Props::defaults(NodeKind::Boolean)
                },
            )?;
            let mut ids = vec![b];
            for i in 0..30 {
                let e = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
                tx.set_props(
                    e,
                    &patch((i % 6) as f64 * 15.0, (i / 6) as f64 * 15.0, 20.0, 20.0),
                )?;
                ids.push(e);
            }
            Ok(ids)
        })
        .unwrap();
    let b = ids.remove(0);
    (doc, page, b, ids)
}

#[test]
fn gesture_on_one_child_recomputes_only_near_it_and_matches_the_full_result() {
    for op in BooleanOp::ALL {
        for moved in [0usize, 14] {
            let (mut doc, page, b, kids) = grid(op);
            let mut scene = Scene::from_page(doc.page(page).unwrap());
            for dx in [3.0, 7.0, 40.0] {
                scene.set_preview(&[(
                    kids[moved],
                    PropsPatch {
                        x: Some((moved % 6) as f64 * 15.0 + dx),
                        y: Some(-2.0),
                        ..Default::default()
                    },
                )]);
                // Тот же документ, записанный, — полный пересчёт.
                doc.transact(page, vec![], |tx| {
                    tx.set_props(
                        kids[moved],
                        &PropsPatch {
                            x: Some((moved % 6) as f64 * 15.0 + dx),
                            y: Some(-2.0),
                            ..Default::default()
                        },
                    )?;
                    Ok(vec![])
                })
                .unwrap();
                let full = Scene::from_page(doc.page(page).unwrap());
                let outline = scene.node(b).unwrap().boolean.clone().unwrap();
                assert!(outline.is_partial(), "во время жеста — по частям");
                let got = area(&outline.contours);
                let want = area(&full.node(b).unwrap().boolean.as_ref().unwrap().contours);
                assert!(
                    (got - want).abs() < 1e-3 * want.max(1.0),
                    "{op:?}, ребёнок {moved}, сдвиг {dx}: {got} ≠ {want}"
                );
                assert!(
                    near(local_bounds(&scene, b), local_bounds(&full, b)),
                    "{op:?} {moved} {dx}"
                );
            }
            // Жест кончился (как у редактора: превью снято, документ записан) —
            // без полного пересчёта: те же сложенные остальные.
            scene.clear_preview();
            let changes = doc.take_changes(page);
            scene.apply(&changes, doc.page(page).unwrap());
            let fresh = Scene::from_page(doc.page(page).unwrap());
            let outline = scene.node(b).unwrap().boolean.clone().unwrap();
            assert!(outline.is_partial(), "запись — по частям, {op:?} {moved}");
            let want = area(&fresh.node(b).unwrap().boolean.as_ref().unwrap().contours);
            assert!((area(&outline.contours) - want).abs() < 1e-3 * want.max(1.0));
            assert!(near(local_bounds(&scene, b), local_bounds(&fresh, b)));
            // Правка другого ребёнка без жеста — полный пересчёт, как у открытого.
            doc.transact(page, vec![], |tx| {
                tx.set_props(
                    kids[(moved + 7) % kids.len()],
                    &PropsPatch {
                        y: Some(-5.0),
                        ..Default::default()
                    },
                )?;
                Ok(vec![])
            })
            .unwrap();
            let changes = doc.take_changes(page);
            scene.apply(&changes, doc.page(page).unwrap());
            assert!(!scene
                .node(b)
                .unwrap()
                .boolean
                .as_ref()
                .unwrap()
                .is_partial());
            assert_eq!(
                snapshot(&scene),
                snapshot(&Scene::from_page(doc.page(page).unwrap())),
                "{op:?} {moved}"
            );
        }
    }
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-scene --test boolean`
Expected: FAIL — не собирается: нет `BooleanOutline::is_partial`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
        ferrum_doc::fixture::curvy_network(100_000, 2000.0),
    );

    println!("| Метрика | Значение | Цель | Итог |");
````

на:

````rust
        ferrum_doc::fixture::curvy_network(100_000, 2000.0),
    );
    boolean_child_drag(&mut report);

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

/// Правка ребёнка большой булевой группы (подсистема 3, этап 4): 1000
/// перекрывающихся кругов в «Объединении», перетаскивание одного круга — кадр
/// жеста (превью сцены пересчитывает результат) и запись одним шагом.
fn boolean_child_drag(report: &mut Report) {
    let mut ed =
        Editor::new(Document::new(), None, VIEWPORT, 1.0, Platform::Mac).expect("страница");
    let ids = ed
        .transact(|tx| {
            let b = tx.create_node(None, None, ferrum_doc::NodeKind::Boolean)?;
            let mut kids = vec![b];
            for i in 0..1000 {
                let e = tx.create_node(Some(b), None, ferrum_doc::NodeKind::Ellipse)?;
                tx.set_props(
                    e,
                    &PropsPatch {
                        x: Some((i % 40) as f64 * 15.0),
                        y: Some((i / 40) as f64 * 15.0),
                        w: Some(20.0),
                        h: Some(20.0),
                        ..Default::default()
                    },
                )?;
                kids.push(e);
            }
            Ok(kids)
        })
        .unwrap();
    let (b, child) = (ids[0], ids[501]);
    let bounds = ed.scene().node(b).unwrap().world_bounds.unwrap();
    let mut cam = Camera::new(VIEWPORT, 1.0);
    cam.fit(bounds, 48.0);
    ed.set_camera(cam);
    // Ребёнок выделен, как после двойного щелчка по группе.
    ed.select(&[child]);
    ed.take_events();
    let steps = ed.step_count();
    let c = ed.scene().node(child).unwrap();
    let center = c.world * Point::new(10.0, 10.0);
    let from = ed.camera().world_to_screen(center);
    let mut clock = Clock(0.0);
    // Шаг 3 px экрана — больше пикселя мира: каждое движение сдвигает круг.
    let d = drag_moves(&mut ed, &mut clock, &[child], from, 3.0);
    report.gate_median(
        "Булева группа 1000 кругов: перетаскивание ребёнка — кадр жеста (части у ребёнка)",
        &d.times,
        16.7,
    );
    report.info(
        "Булева группа 1000 кругов: первое движение (остальные складываются один раз)",
        ms(d.first),
    );
    let t = Instant::now();
    ed.handle(Input::PointerUp(clock.at(d.end)));
    let commit = t.elapsed();
    ed.take_events();
    // Запись: превью снимается, документ пишется — тот же ребёнок, остальные
    // уже сложены, полного пересчёта нет.
    report.gate(
        "Булева группа 1000 кругов: запись перетаскивания ребёнка (отпускание)",
        commit,
        16.7,
    );
    report.check(
        "Булева группа 1000 кругов: один шаг, ребёнок сдвинут",
        ed.step_count() == steps + 1 && d.moved == 1,
        format!("{} шаг, {} узел", ed.step_count() - steps, d.moved),
        "1 шаг, 1 узел",
    );
}

````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
        pass: Some(built.stroke.elements().len() <= shape.stroke.elements().len()),
    });
}

````

на:

````rust
        pass: Some(built.stroke.elements().len() <= shape.stroke.elements().len()),
    });
}

/// Булева группа из 1000 перекрывающихся кругов (спека подсистемы 3, раздел 10:
/// булевы операции на 1000 контуров ≤ 50 мс): каждый круг задевает до восьми
/// соседей — тысячи пересечений.
pub fn circles_boolean(doc: &mut Document, page: PageId, op: ferrum_doc::BooleanOp) -> NodeId {
    doc.transact(page, vec![], |tx| {
        let b = tx.create_node(None, None, NodeKind::Boolean)?;
        tx.set_props(
            b,
            &PropsPatch {
                boolean_op: Some(op),
                ..Default::default()
            },
        )?;
        for i in 0..1000 {
            let e = tx.create_node(Some(b), None, NodeKind::Ellipse)?;
            tx.set_props(
                e,
                &PropsPatch {
                    x: Some((i % 40) as f64 * 15.0),
                    y: Some((i / 40) as f64 * 15.0),
                    w: Some(20.0),
                    h: Some(20.0),
                    ..Default::default()
                },
            )?;
        }
        Ok(vec![b])
    })
    .unwrap()[0]
}

fn boolean_section(report: &mut Report) {
    use ferrum_doc::BooleanOp;
    use ferrum_scene::boolean::{combine, flatten_path, normalize, BOOLEAN_TOLERANCE};
    // Операция сама по себе: формы кругов уже раздроблены.
    let circles: Vec<_> = (0..1000)
        .map(|i| {
            let c = kurbo::Circle::new(
                ((i % 40) as f64 * 15.0 + 10.0, (i / 40) as f64 * 15.0 + 10.0),
                10.0,
            );
            normalize(
                flatten_path(&kurbo::Shape::to_path(&c, 1e-3), BOOLEAN_TOLERANCE),
                ferrum_doc::FillRule::NonZero,
            )
        })
        .collect();
    for op in BooleanOp::ALL {
        let runs = (0..5)
            .map(|_| {
                let t = Instant::now();
                std::hint::black_box(combine(op, &circles));
                t.elapsed()
            })
            .collect();
        report.gate(
            &format!("Булева операция «{}»: 1000 кругов", op.name()),
            median(runs),
            50.0,
            "≤ 50 мс",
        );
    }
    // Пересечение, у которого общее у всех не пусто: 1000 кругов радиуса 10 со
    // сдвигом по 0,005 — каждый шаг режет почти весь результат.
    let stacked: Vec<_> = (0..1000)
        .map(|i| {
            let c = kurbo::Circle::new((10.0 + 0.005 * i as f64, 10.0), 10.0);
            normalize(
                flatten_path(&kurbo::Shape::to_path(&c, 1e-3), BOOLEAN_TOLERANCE),
                ferrum_doc::FillRule::NonZero,
            )
        })
        .collect();
    let runs = (0..5)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(combine(BooleanOp::Intersect, &stacked));
            t.elapsed()
        })
        .collect();
    report.gate(
        "Булева операция «Пересечение»: 1000 почти совпадающих кругов",
        median(runs),
        50.0,
        "≤ 50 мс",
    );
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let b = circles_boolean(&mut doc, page, BooleanOp::Union);
    let build = median(
        (0..5)
            .map(|_| {
                let t = Instant::now();
                std::hint::black_box(Scene::from_page(doc.page(page).unwrap()));
                t.elapsed()
            })
            .collect(),
    );
    report.gate(
        "Булева группа 1000 кругов: сцена с результатом (дробление и операция)",
        build,
        50.0,
        "≤ 50 мс",
    );
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let bounds = scene.node(b).unwrap().world_bounds.unwrap();
    let mut camera = Camera::new(VIEWPORT, 1.0);
    camera.fit(bounds, 48.0);
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    let mut renderer = Renderer::new();
    renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
    let (steady, _) = frame(&mut renderer, &mut backend, &scene, &camera);
    report.gate(
        "Булева группа 1000 кругов: кадр (результат в кэше)",
        steady,
        16.7,
        "≤ 16.7 мс",
    );
    // Правка одного ребёнка: запись, сцена (результат заново) и кадр.
    let child = doc.page(page).unwrap().children(Some(b))[500];
    let runs = (0..5)
        .map(|k| {
            let t = Instant::now();
            doc.transact(page, vec![], |tx| {
                tx.set_props(
                    child,
                    &PropsPatch {
                        x: Some(3.0 * k as f64),
                        ..Default::default()
                    },
                )
                .map(|_| vec![])
            })
            .unwrap();
            let changes = doc.take_changes(page);
            scene.apply(&changes, doc.page(page).unwrap());
            backend.reset(w, h);
            renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
            t.elapsed()
        })
        .collect();
    report.gate(
        "Булева группа 1000 кругов: правка ребёнка — запись, сцена и кадр",
        median(runs),
        50.0,
        "≤ 50 мс (кадр жеста — editor_report)",
    );
}

````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
    vector_section(&mut report);
    curvy_vector_section(&mut report);

    // Экспорт фрейма @3x — справочно.
````

на:

````rust
    vector_section(&mut report);
    curvy_vector_section(&mut report);
    boolean_section(&mut report);

    // Экспорт фрейма @3x — справочно.
````

В `crates/ferrum-scene/src/boolean.rs` заменить:

````rust
}

/// Форма заливки узла-листа в его локальных координатах (без детей): фигура —
/// её контур (у фрейма и изображения — прямоугольник), вектор — его области
````

на:

````rust
}

/// Связная часть формы: внешний контур с его дырами и рамка.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub bounds: Rect,
    pub contours: Contours,
}

fn pieces(shapes: Vec<Vec<Contour>>) -> Vec<Piece> {
    shapes
        .into_iter()
        .filter_map(|shape| {
            let contours = Contours(shape);
            Some(Piece {
                bounds: contours.bounds()?,
                contours,
            })
        })
        .collect()
}

/// Что делать с одной изменившейся формой и операцией над остальными
/// (`combine_rest`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rest {
    /// Объединение остальных ∪ форма.
    Union,
    /// Исключение остальных ⊕ форма.
    Exclude,
    /// Пересечение остальных ∩ форма.
    Intersect,
    /// Вычитание (основа — не изменившаяся) − форма.
    Minus,
    /// Форма (изменилась основа вычитания) − объединение остальных.
    From,
}

/// Операция над всеми формами, кроме `skip`, по частям — для пересчёта, когда
/// меняется одна форма (жест над ребёнком большой булевой группы): дальше
/// `combine_with` трогает только части у этой формы.
pub fn combine_rest(op: BooleanOp, inputs: &[Contours], skip: usize) -> (Rest, Vec<Piece>) {
    let others: Vec<Contours> = inputs
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != skip)
        .map(|(_, c)| c.clone())
        .collect();
    let (rest, op2) = match op {
        BooleanOp::Union => (Rest::Union, BooleanOp::Union),
        BooleanOp::Exclude => (Rest::Exclude, BooleanOp::Exclude),
        BooleanOp::Intersect => (Rest::Intersect, BooleanOp::Intersect),
        BooleanOp::Subtract if skip == 0 => (Rest::From, BooleanOp::Union),
        BooleanOp::Subtract => (Rest::Minus, BooleanOp::Subtract),
    };
    let all = combine(op2, &others);
    // По частям: внешние контуры с их дырами.
    let shapes = FloatOverlay::with_subj(&all.0).overlay(OverlayRule::Subject, IFillRule::NonZero);
    (rest, pieces(shapes))
}

/// Результат по операции над остальными формами (`combine_rest`) и новой
/// форме `changed`: части, чья рамка не задевает форму, остаются как есть (у
/// пересечения — уходят), остальные — одной операцией с формой.
pub fn combine_with(rest: Rest, parts: &[Piece], changed: &Contours) -> Contours {
    let Some(cb) = changed.bounds() else {
        return match rest {
            Rest::Intersect | Rest::From => Contours::default(),
            _ => Contours(parts.iter().flat_map(|p| p.contours.0.clone()).collect()),
        };
    };
    let touches = |b: &Rect| b.x0 <= cb.x1 && b.x1 >= cb.x0 && b.y0 <= cb.y1 && b.y1 >= cb.y0;
    let mut near: Vec<Contour> = Vec::new();
    let mut out: Vec<Contour> = Vec::new();
    for p in parts {
        if touches(&p.bounds) {
            near.extend(p.contours.0.iter().cloned());
        } else if !matches!(rest, Rest::Intersect | Rest::From) {
            out.extend(p.contours.0.iter().cloned());
        }
    }
    let local = match rest {
        Rest::Union | Rest::Exclude => {
            near.extend(changed.0.iter().cloned());
            let rule = if rest == Rest::Union {
                IFillRule::NonZero
            } else {
                IFillRule::EvenOdd
            };
            FloatOverlay::with_subj(&near).overlay(OverlayRule::Subject, rule)
        }
        Rest::Intersect => FloatOverlay::with_subj_and_clip(&near, &changed.0)
            .overlay(OverlayRule::Intersect, IFillRule::NonZero),
        Rest::Minus => FloatOverlay::with_subj_and_clip(&near, &changed.0)
            .overlay(OverlayRule::Difference, IFillRule::NonZero),
        Rest::From => FloatOverlay::with_subj_and_clip(&changed.0, &near)
            .overlay(OverlayRule::Difference, IFillRule::NonZero),
    };
    out.extend(local.into_iter().flatten());
    Contours(out)
}

/// Форма заливки узла-листа в его локальных координатах (без детей): фигура —
/// её контур (у фрейма и изображения — прямоугольник), вектор — его области
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust

/// Результат булевой группы в её локальных координатах.
#[derive(Debug, Clone, PartialEq)]
pub struct BooleanOutline {
    pub contours: Contours,
    /// Форма для рендера и попадания: одна область и замкнутый путь обводки.
    pub shape: Arc<PathShape>,
}

````

на:

````rust

/// Результат булевой группы в её локальных координатах.
#[derive(Debug, Clone)]
pub struct BooleanOutline {
    pub contours: Contours,
    /// Форма для рендера и попадания: одна область и замкнутый путь обводки.
    pub shape: Arc<PathShape>,
    op: ferrum_doc::BooleanOp,
    /// Формы видимых детей в координатах группы с их ключами — для
    /// пересчёта только сменившихся.
    inputs: Vec<(u64, Arc<Contours>)>,
    /// Операция над всеми, кроме одного ребёнка (номер, вид, части), — пока
    /// жест двигает этого ребёнка.
    rest: Option<Arc<(usize, boolean::Rest, Vec<boolean::Piece>)>>,
    /// Посчитан по частям (жест над одним ребёнком и его запись).
    partial: bool,
}

impl BooleanOutline {
    /// Посчитан по частям — жест над одним ребёнком и его запись
    /// (`Scene::boolean_outline`).
    pub fn is_partial(&self) -> bool {
        self.partial
    }
}

````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
            return;
        }
        let outline = (n.props.kind == NodeKind::Boolean).then(|| {
            let op = n.props.boolean_op.unwrap_or_default();
            let inputs: Vec<Contours> = n
                .children
                .iter()
                .filter(|&&c| self.at(c).props.visible)
                .map(|&c| self.fill_of(c).transformed(self.at(c).local))
                .filter(|c| !c.is_empty())
                .collect();
            let contours = boolean::combine(op, &inputs);
            let shape = Arc::new(boolean::outline_shape(&contours));
            Arc::new(BooleanOutline { contours, shape })
        });
        let n = self.at_mut(slot);
        n.content = key;
        if let Some(o) = outline {
            if n.boolean.as_ref() != Some(&o) {
                // Другой результат — другой вид: рендер строит список заново.
                n.version = fresh_version();
            }
            n.boolean = Some(o);
        }
    }
````

на:

````rust
            return;
        }
        let outline = (n.props.kind == NodeKind::Boolean).then(|| self.boolean_outline(slot));
        let n = self.at_mut(slot);
        n.content = key;
        if let Some(o) = outline {
            if n.boolean.as_ref().map(|b| &b.contours) != Some(&o.contours) {
                // Другой результат — другой вид: рендер строит список заново.
                n.version = fresh_version();
            }
            n.boolean = Some(Arc::new(o));
        }
    }

    /// Результат булевой группы. Формы детей, чей ключ (содержимое и
    /// положение) не сменился, берутся из прошлого результата. Пока идёт жест
    /// (превью) и сменилась ровно одна форма, остальные складываются один раз
    /// (`boolean::combine_rest`), а на каждое движение пересчитываются только
    /// части у этой формы: правка ребёнка группы из 1000 кругов — доли
    /// миллисекунды, а не 20 мс на кадр. Запись жеста (снятие превью и правка
    /// документа — снова тот же ребёнок) берёт те же сложенные остальные и
    /// полного пересчёта не делает; ключ записи, совпавший с последним превью,
    /// не пересчитывает ничего. Другой ребёнок, несколько сразу или смена
    /// операции без жеста — полный пересчёт. Результат по частям может
    /// отличаться от полного на округление `i_overlay` (доли миллионной).
    fn boolean_outline(&self, slot: Slot) -> BooleanOutline {
        let n = self.at(slot);
        let op = n.props.boolean_op.unwrap_or_default();
        let prev = n.boolean.as_deref().filter(|b| b.op == op);
        let mut changed = Vec::new();
        let inputs: Vec<(u64, Arc<Contours>)> = n
            .children
            .iter()
            .filter(|&&c| self.at(c).props.visible)
            .enumerate()
            .map(|(i, &c)| {
                let cn = self.at(c);
                let mut h = std::collections::hash_map::DefaultHasher::new();
                cn.content.hash(&mut h);
                for v in cn.local.as_coeffs() {
                    v.to_bits().hash(&mut h);
                }
                let key = h.finish();
                match prev.and_then(|p| p.inputs.get(i)).filter(|p| p.0 == key) {
                    Some(p) => p.clone(),
                    None => {
                        changed.push(i);
                        (key, Arc::new(self.fill_of(c).transformed(cn.local)))
                    }
                }
            })
            .collect();
        let shapes: Vec<Contours> = inputs.iter().map(|(_, c)| (**c).clone()).collect();
        let single = match (prev, &changed[..]) {
            (Some(p), [i])
                if p.inputs.len() == inputs.len()
                    && (!self.preview.is_empty() || p.rest.as_ref().is_some_and(|r| r.0 == *i)) =>
            {
                // Пустые формы в операции не участвуют — тогда без частей.
                let was = &p.inputs[*i].1;
                (!was.is_empty() && !shapes[*i].is_empty() && shapes.iter().all(|c| !c.is_empty()))
                    .then_some(*i)
            }
            _ => None,
        };
        let (contours, rest) = match single {
            Some(i) => {
                let rest = match prev.and_then(|p| p.rest.clone()).filter(|r| r.0 == i) {
                    Some(r) => r,
                    None => {
                        let (mode, parts) = boolean::combine_rest(op, &shapes, i);
                        Arc::new((i, mode, parts))
                    }
                };
                (
                    boolean::combine_with(rest.1, &rest.2, &shapes[i]),
                    Some(rest),
                )
            }
            None => {
                let nonempty: Vec<Contours> =
                    shapes.into_iter().filter(|c| !c.is_empty()).collect();
                (boolean::combine(op, &nonempty), None)
            }
        };
        let shape = Arc::new(boolean::outline_shape(&contours));
        BooleanOutline {
            partial: rest.is_some(),
            contours,
            shape,
            op,
            inputs,
            rest,
        }
    }
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-scene -p ferrum-render -p ferrum-editor 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-render --example render_report | grep -E "FAIL|Булев"`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Булев"`

Expected: PASS: `boolean` сцены — 9 тестов, остальные зелёные; `render_report` — 8 строк «Булева …» PASS, FAIL нет (объединение ≈ 18 мс, вычитание ≈ 17 мс, пересечение непересекающихся 0,02 мс, исключение ≈ 21 мс, пересечение почти совпадающих ≈ 19 мс; сцена с результатом ≈ 23 мс, кадр ≈ 0,55 мс, правка ребёнка ≈ 20 мс); `editor_report` — кадр жеста ≈ 2,4 мс (медиана), первое движение ≈ 25 мс (без цели), запись ≈ 5,7 мс (≤ 16,7, PASS), один шаг, FAIL нет

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/examples/editor_report.rs crates/ferrum-render/examples/render_report.rs crates/ferrum-scene/src/boolean.rs crates/ferrum-scene/src/scene.rs crates/ferrum-scene/tests/boolean.rs
git commit -m "perf(scene): жест над ребёнком большой булевой группы и его запись пересчитывают только части у ребёнка; замеры булевых операций на 1000 кругов в render_report и editor_report

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 9: Спека и финальная проверка

Спека получает решения этапа 4: «Сделано (этап 4)» в разделах 8 и 10, клавиши Alt+Shift+U/S/I/E в разделе 10, совместимость в разделе 12. Шаг «Проверить» — все ворота этапа.

**Files:**
- Modify: `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md`

- [ ] **Step 1: Реализация**

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **Сделано (этап 2):** `TextEditStart { id }`, `TextEditEnd`, `TextInput { kind, data? }` (`insert`, `paragraph`, `paste` — вне правки новый текст, `compose`, `commit`, `delete*`), `TextKey { key, mods }` (`key` — `KeyboardEvent.code`), `SetTextStyle { patch, phase, ids? }` и `SetTextProps { patch, phase, ids? }` (диапазон — выделение правки в ядре, поля `range` нет; превью — фазой, как у полей панели); события `TextCaret { editing, rect?, selection?, draft }`, `TextStyleSummary { summary? }` (перечисления — строками), сверх списка — `PageFonts { families }` (группа «В документе»); `Clipboard` получил `plain` (обычный текст фрагмента). Курсор `text` — у инструмента T и над текстом в правке. Очередь правок, ждущих шрифтов: пока она не пуста, в неё же по порядку встают и все следующие команды, меняющие документ, выделение, камеру или правку (`Paste`, `Execute` — «Вырезать», отмена, — `Select`, `SetTool`, `SetPage` и т. д.), и ввод из кольца (щелчки, клавиши; сдвиги указателя подряд — последний), и выполняются по приходу шрифтов — щелчок не сдвигает курсор раньше ждущего набора, щелчок мимо заканчивает правку после него. Сразу выполняются загрузка шрифтов, замена документа (очередь сбрасывается), снимок и журнал, чтения для тестов, тема и вид панели слоёв. «Вырезать», Backspace и вставка фрагмента, сами упёршиеся в незагруженный шрифт, тоже встают в очередь (без ошибки). Очередь — не больше 4096 записей (сверх — сбрасывается с ошибкой «Шрифты загружаются слишком долго…»); ответа на запрос шрифта ждут не дольше 15 с, затем запрошенные шрифты считаются не полученными (как `LoadFont` без байтов), очередь выполняется запасным шрифтом, поздние байты всё равно принимаются.
- **Сделано (этап 3):** инструменты `pen` и `arrow` (команды раскладки `tool.pen` — P, `tool.arrow` — Shift+L, `object.flatten` — Ctrl/Cmd+E), курсоры `pen` и `bucket`, тип строки слоя `vector`; правка панели `strokeStyle { capStart?, capEnd?, join?, miterLimit? }` (предел вне 1…100 — ошибка, панель зажимает его теми же числами из `limits.ts`), сводка `strokeStyle { ends, corners, capStart?, capEnd?, join?, miterLimit }` (`undefined` — «Смешанное»). Сеть в протокол не идёт: правка вектора идёт вводом холста.
- **Сделано (этап 1):** `FontsAvailable { source: bundled | system, families: [{ family, faces: [{ style, weight?, italic?, stretch?, file, index? }] }] }` (у системных Chrome вес и курсив ядро угадывает по имени начертания), `LoadFont { family, file, bytes? }` — по одному файлу (без байтов — файл не получить), `FontRequest { family, source, files }` (вшитые выполняет worker ядра, системные — основной поток), `MissingFonts { list: [{ family, style, nodes }] }`, `ReplaceMissingFont { from: { family, style }, to: { family, style } }`; сверх списка — события `FontCatalog { families: [{ family, source, styles }] }` (выбор замены) и `Log { message }` (журнал ошибок), команды `DemoText { texts }` (замер) и `TestCreateText` (только тестовый режим страницы). Правки, которым нужны ещё не загруженные шрифты, ядро держит в очереди и выполняет по приходу байтов.

````

на:

````markdown
- **Сделано (этап 2):** `TextEditStart { id }`, `TextEditEnd`, `TextInput { kind, data? }` (`insert`, `paragraph`, `paste` — вне правки новый текст, `compose`, `commit`, `delete*`), `TextKey { key, mods }` (`key` — `KeyboardEvent.code`), `SetTextStyle { patch, phase, ids? }` и `SetTextProps { patch, phase, ids? }` (диапазон — выделение правки в ядре, поля `range` нет; превью — фазой, как у полей панели); события `TextCaret { editing, rect?, selection?, draft }`, `TextStyleSummary { summary? }` (перечисления — строками), сверх списка — `PageFonts { families }` (группа «В документе»); `Clipboard` получил `plain` (обычный текст фрагмента). Курсор `text` — у инструмента T и над текстом в правке. Очередь правок, ждущих шрифтов: пока она не пуста, в неё же по порядку встают и все следующие команды, меняющие документ, выделение, камеру или правку (`Paste`, `Execute` — «Вырезать», отмена, — `Select`, `SetTool`, `SetPage` и т. д.), и ввод из кольца (щелчки, клавиши; сдвиги указателя подряд — последний), и выполняются по приходу шрифтов — щелчок не сдвигает курсор раньше ждущего набора, щелчок мимо заканчивает правку после него. Сразу выполняются загрузка шрифтов, замена документа (очередь сбрасывается), снимок и журнал, чтения для тестов, тема и вид панели слоёв. «Вырезать», Backspace и вставка фрагмента, сами упёршиеся в незагруженный шрифт, тоже встают в очередь (без ошибки). Очередь — не больше 4096 записей (сверх — сбрасывается с ошибкой «Шрифты загружаются слишком долго…»); ответа на запрос шрифта ждут не дольше 15 с, затем запрошенные шрифты считаются не полученными (как `LoadFont` без байтов), очередь выполняется запасным шрифтом, поздние байты всё равно принимаются.
- **Сделано (этап 3):** инструменты `pen` и `arrow` (команды раскладки `tool.pen` — P, `tool.arrow` — Shift+L, `object.flatten` — Ctrl/Cmd+E), курсоры `pen` и `bucket`, тип строки слоя `vector`; правка панели `strokeStyle { capStart?, capEnd?, join?, miterLimit? }` (предел вне 1…100 — ошибка, панель зажимает его теми же числами из `limits.ts`), сводка `strokeStyle { ends, corners, capStart?, capEnd?, join?, miterLimit }` (`undefined` — «Смешанное»). Сеть в протокол не идёт: правка вектора идёт вводом холста.
- **Сделано (этап 4):** тип строки слоя `boolean` с полем `booleanOp` (иконка операции), сводка выделения — `booleanOp` (у выделения только из булевых групп с одной операцией; список тулбара отмечает её); команды раскладки `object.boolean_union`, `object.boolean_subtract`, `object.boolean_intersect`, `object.boolean_exclude` (Alt+Shift+U/S/I/E), `object.outline_stroke` (Ctrl/Cmd+Alt+O) идут обычным `Execute`; подсказка кнопки тулбара — с сочетанием из действующей раскладки. Геометрия в протокол не идёт.
- **Сделано (этап 1):** `FontsAvailable { source: bundled | system, families: [{ family, faces: [{ style, weight?, italic?, stretch?, file, index? }] }] }` (у системных Chrome вес и курсив ядро угадывает по имени начертания), `LoadFont { family, file, bytes? }` — по одному файлу (без байтов — файл не получить), `FontRequest { family, source, files }` (вшитые выполняет worker ядра, системные — основной поток), `MissingFonts { list: [{ family, style, nodes }] }`, `ReplaceMissingFont { from: { family, style }, to: { family, style } }`; сверх списка — события `FontCatalog { families: [{ family, source, styles }] }` (выбор замены) и `Log { message }` (журнал ошибок), команды `DemoText { texts }` (замер) и `TestCreateText` (только тестовый режим страницы). Правки, которым нужны ещё не загруженные шрифты, ядро держит в очереди и выполняет по приходу байтов.

````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown

- Узел `BooleanOperation` (`union` | `subtract` | `intersect` | `exclude`) с детьми — как группа; результат — контур, пересчитанный ядром при изменении детей (кэш по `subtree_version`, ядро §6), рисуется заливками и обводками самого узла. Дети видны в слоях и правятся как обычно.
- Команды: четыре операции на выделение (Ctrl/Cmd+Alt+U/S/I/X — по раскладке Figma), «Сплющить» (результат в `Vector`), «Контур обводки» (Ctrl/Cmd+Alt+O — обводка в заливку `Vector`).
- Библиотека: кандидат `linesweeper` (операции над кривыми без дробления), запасной — `i_overlay` по раздроблённым кривым; выбор — прототипом с замером в плане этапа 4 (точность на касаниях и совпадающих рёбрах, время на 1000 контуров ≤ 50 мс).

## 11. Scale и экспорт (этап 5)
````

на:

````markdown

- Узел `BooleanOperation` (`union` | `subtract` | `intersect` | `exclude`) с детьми — как группа; результат — контур, пересчитанный ядром при изменении детей (кэш по `subtree_version`, ядро §6), рисуется заливками и обводками самого узла. Дети видны в слоях и правятся как обычно.
- Команды: четыре операции на выделение (Alt+Shift+U/S/I/E, на macOS Option+Shift — по раскладке Figma), «Сплющить» (результат в `Vector`), «Контур обводки» (Ctrl/Cmd+Alt+O — обводка в заливку `Vector`).
- Библиотека: кандидат `linesweeper` (операции над кривыми без дробления), запасной — `i_overlay` по раздроблённым кривым; выбор — прототипом с замером в плане этапа 4 (точность на касаниях и совпадающих рёбрах, время на 1000 контуров ≤ 50 мс).
- **Сделано (этап 4):** узел `boolean` (ключ `booleanOp`: `union` | `subtract` | `intersect` | `exclude`, без ключа — объединение); неизвестная операция из файла скрывает группу с детьми и открывает документ только для чтения, как неизвестный тип (`DocError::InvalidBooleanOp`), из буфера обмена — отказ вставки; кэш сцены — версия 5 (код операции у строки группы); совместимость — `scripts/compat-old-build.sh main boolean_fixture`. Библиотека — `i_overlay 9.1.0` по раздроблённым кривым (точность 0,005 в координатах группы): `linesweeper 0.5.0` на 1000 перекрывающихся кругах — 70–93 мс (без перекрытий — 22 мс), `i_overlay` — 15–21 мс; на касаниях встык, касаниях углом и совпадающих рёбрах обе дают точный ответ. Результат — ломаные (кривые остаются только у «Контура обводки» по центру). Форма каждого ребёнка сначала приводится к простым контурам своим правилом заливки, и операция считает перекрытия: объединение — хотя бы одна форма, вычитание — нижняя без остальных, пересечение — все, исключение — нечётное число. Форма ребёнка: фигура — её контур (у фрейма и изображения — рамка), вектор — его области, текст — контуры глифов и украшений (контуры даёт редактор из реестра шрифтов; пока шрифты грузятся, текст формы не даёт, по приходу байтов группы с текстом пересчитываются; реестр, занятый в момент пересчёта, помечает его — группа пересчитывается на следующем кадре или команде), группа — объединение детей, вложенная булева группа — её результат; линии, открытые пути без областей, скрытые слои и обводки детей в результат не входят. Результат — в узле сцены, пересчитывается, когда меняется ключ содержимого (версии листьев, положения и видимость детей, операция; сдвиг самой группы его не меняет); во время жеста над одним ребёнком остальные складываются один раз, и на каждое движение и при записи жеста пересчитываются только части у этого ребёнка (результат по частям может отличаться от полного на округление, доли миллионной); правка другого ребёнка без жеста — полный пересчёт. Рамка группы — по результату (пустой — по детям), попадание — по результату, в детей щелчок попадает только внутри результата внешней группы (дыра вычитания — мимо); дети не рисуются, подсветка — контуром результата. Команды `object.boolean_union` | `_subtract` | `_intersect` | `_exclude` (Alt+Shift+U/S/I/E, как в справке Figma; Alt+Shift и Shift+E в раскладке больше ничем не заняты; с AltGr на Windows и Linux сочетания с Alt не срабатывают — это набор символа) и `object.outline_stroke` (Ctrl/Cmd+Alt+O) — всего 53. Новая группа встаёт на место верхнего слоя, называется по операции, берёт заливки, обводки и стиль обводки верхнего слоя (у «Вычитания» — нижнего; у группы — её верхнего или нижнего листа); фреймы в группу не входят (сообщение), меньше двух слоёв — сообщение без шага; выделены только булевы группы — та же команда меняет их операцию (имя по умолчанию — вместе с ней). Ресайз, разгруппировка, перенос в слоях внутрь — как у группы; двойной щелчок — к ребёнку. «Сплющить» булевой группы и текста — новый вектор на их месте (результат, контуры глифов); текст без шрифтов не сплющивается (сообщение). «Контур обводки» — вектор прямо над слоем с его именем и позой, по области на каждую видимую обводку её цветом (у кольцевых фигур — правило even-odd, по центру — кривые `kurbo::stroke` с окончаниями и соединением, внутри и снаружи — обрезка контуром формы булевой операцией); слой с видимыми заливками остаётся без обводок, без заливок — удаляется. Замер (`render_report`, `editor_report`, нативно, release), 1000 перекрывающихся кругов: объединение ≈ 18 мс, вычитание ≈ 17 мс, исключение ≈ 21 мс, пересечение почти совпадающих ≈ 19 мс; сцена с результатом ≈ 23 мс, кадр ≈ 0,6 мс, правка ребёнка (запись, сцена, кадр) ≈ 20 мс; перетаскивание ребёнка — кадр жеста ≈ 2,3 мс (первое движение, складывающее остальные, ≈ 25 мс), запись (отпускание) ≈ 6 мс. Открыто: результат — ломаные (точность 0,005 — четверть пикселя при ×50, при ×256 грани видны); обводка текста по швам перекрывающихся контуров глифов (раздел 6) не исправлена — рендер обводит глифы без объединения.

## 11. Scale и экспорт (этап 5)
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown

- Как в ядре §12: модульные тесты крейтов (модель текста и разметка, отмена правок текста, раскладка с эталонами позиций глифов на вшитых шрифтах, векторная сеть, булевы операции — свойства и эталоны, писатель SVG — сравнение с эталоном и разбор), эталоны рендера (текст, LOD, векторы, обводки, стрелки) на `vello_cpu` и пиксели GPU против CPU, E2E (ввод, IME — синтетическими событиями композиции, буфер обмена, выбор шрифта, отсутствующий шрифт, перо, булевы, экспорт), замеры (раскладка и рисование 10 000 текстов, булевы на 1000 контуров).
- Совместимость: файл с текстом открывается в сборке подсистемы 1 только для чтения (тест на документе, собранном новой сборкой): `scripts/compat-old-build.sh [ref]` собирает `ferrum-doc` из `ref` (по умолчанию `main`) во временной копии и открывает им файл примера `text_fixture`; файл с вектором — `scripts/compat-old-build.sh <ref> vector_fixture` (сборка без векторов открывает его только для чтения).
````

на:

````markdown

- Как в ядре §12: модульные тесты крейтов (модель текста и разметка, отмена правок текста, раскладка с эталонами позиций глифов на вшитых шрифтах, векторная сеть, булевы операции — свойства и эталоны, писатель SVG — сравнение с эталоном и разбор), эталоны рендера (текст, LOD, векторы, обводки, стрелки) на `vello_cpu` и пиксели GPU против CPU, E2E (ввод, IME — синтетическими событиями композиции, буфер обмена, выбор шрифта, отсутствующий шрифт, перо, булевы, экспорт), замеры (раскладка и рисование 10 000 текстов, булевы на 1000 контуров).
- Совместимость: файл с текстом открывается в сборке подсистемы 1 только для чтения (тест на документе, собранном новой сборкой): `scripts/compat-old-build.sh [ref]` собирает `ferrum-doc` из `ref` (по умолчанию `main`) во временной копии и открывает им файл примера `text_fixture`; файл с вектором — `scripts/compat-old-build.sh <ref> vector_fixture` (сборка без векторов открывает его только для чтения); файл с булевой группой — `scripts/compat-old-build.sh <ref> boolean_fixture` (сборка без булевых групп скрывает группу с детьми и открывает файл только для чтения).
````

- [ ] **Step 2: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-render --example render_report | grep -E "FAIL|Булев"`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Булев"`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && ./scripts/compat-old-build.sh 0797cf7 boolean_fixture`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && ./scripts/compat-old-build.sh main vector_fixture`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npx playwright test e2e/booleans.spec.ts e2e/vectors.spec.ts e2e/text.spec.ts e2e/text-edit.spec.ts e2e/roundtrip.spec.ts --project=firefox`

Expected: всё PASS: `cargo test --workspace` — 877 passed; `render_report` и `editor_report` — строки «Булева …» PASS, FAIL нет (те же числа, что в задаче 8); совместимость — только для чтения (оба примера); модульные — 134 passed; `e2e:ci` — 326 тестов passed (один может пройти со второго раза — см. «Проверка плана»); Firefox — 8 passed

- [ ] **Step 3: Закоммитить**

```bash
git add docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md
git commit -m "docs(spec): сделанное в этапе 4 — булевы группы, библиотека и замеры, «Контур обводки», протокол, совместимость

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Ручная проверка**

1. Chrome: `#/new`, прямоугольник и эллипс внахлёст, Alt+Shift+U — «Объединение» в слоях с детьми; Alt+Shift+S, I, E — смена операции и имени; Ctrl/Cmd+Z.
2. Список «Булевы операции» в тулбаре: отметка операции выделенной группы, «Сплющить», «Контур обводки»; без выделения — неактивен.
3. Двойной щелчок по ребёнку, перетаскивание — результат следует за курсором, отпускание — один шаг; щелчок в дыру «Вычитания» — мимо.
4. Текст в булевой группе — по глифам; «Сплющить» текста; прямоугольник с обводкой внутри — Ctrl/Cmd+Alt+O.
5. Приближение ×256 — грани ломаных видны (известное ограничение).
6. Firefox: Alt+Shift+U и перетаскивание ребёнка.

---

## Что этап 4 сознательно не делает

- **Кривые в результате.** Результат булевой группы — ломаные (точность 0,005 в координатах группы): при ×50 — четверть пикселя, при ×256 грани видны (~1,3 px); «Сплющить» булевой группы даёт сотни точек на кривую. Кривые остаются только у «Контура обводки» по центру.
- **Первое движение жеста над ребёнком большой группы** — ≈ 25 мс (остальные складываются один раз), больше кадра; следующие движения ≈ 2,3 мс, запись ≈ 6 мс.
- **Швы обводки текста** по перекрывающимся контурам глифов (ограничение этапа 2, отложенное «до этапа 4») не исправлены: рендер по-прежнему обводит глифы без объединения.
- **Узел `Boolean`, созданный не командой** (например, вставкой без стиля), получает серую заливку по умолчанию, а не стиль верхнего слоя.
- **Сложенные остальные дети** держатся в результате до следующего полного пересчёта — ещё одна копия результата в памяти.
- **Пиксели GPU против CPU для булевых групп** (проект `gpu`), замеры `@perf`, Safari/WebKit, Tauri — вручную; эталоны созданы на одной машине.
- **Замер булевых операций в wasm** — только нативные `render_report` и `editor_report`.
- **Многопоточность `i_overlay`** (`allow_multithreading`, rayon) — выключена.
