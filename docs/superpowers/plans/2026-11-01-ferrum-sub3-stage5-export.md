# Ferrum, подсистема 3, этап 5: Scale и экспорт SVG/PDF — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Инструмент «Масштабирование» (K) как Scale в Figma: ручки масштабируют выделение всегда пропорционально вместе с толщиной обводок, радиусами, кеглем, интерлиньяжем и трекингом в px, отступами абзацев и радиусами точек вектора, содержимое групп, булевых групп и фреймов — тоже; секция панели «Масштаб» — поле в процентах и точка рамки 3×3; превью в сцене, запись — один шаг. Экспорт SVG собственным писателем ядра (фигуры, фреймы с обрезкой, обводки по положению, картинки, текст элементами `<text>` или кривыми, флажки «Текст кривыми» и «Атрибут id»), «Копировать как SVG» (`text/plain` и `image/svg+xml`, где браузер умеет), экспорт PDF через `krilla` (фрейм — страница своего размера, текст — настоящим текстом с подмножеством шрифта, картинки); раздел «Экспорт» панели — формат PNG/SVG/PDF, в десктопе — родной диалог «Сохранить». Кадр жеста K на 10 000 узлах ≤ 50 мс, экспорт SVG 10 000 узлов и PDF с текстом и вектором в 100 000 точек ≤ 1 с.

**Architecture:** Модель документа не меняется. Редактор (`ferrum-editor`) получает инструмент `ToolKind::Scale` на общей с «Перемещением» основе (`SelectTool::scaling()`): равномерный коэффициент из ручки (`scale::uniform_factor`), позы поддерева (`geometry::bases_deep`, `scale_patches_deep`) и свойства в px (`scale::scale_props`), правку панели `PanelEdit::Scale { factor, anchor }` — тем же путём превью и записи, что ресайз. Рендер (`ferrum-render`) получает писатель SVG (`svg.rs`: `Renderer::export_svg`, область и порядок — общие с PNG в `export.rs`) и экспорт PDF (`pdf.rs`: тот же рисовальщик, что экран и PNG, через бэкенд `PdfBackend` поверх `krilla`, текст — новым методом бэкенда `fill_glyphs`); раскладка (`ferrum-text`) отдаёт текст и кластеры прогона для `<text>` и ToUnicode. Пределы — `ExportBudget` (256 МБ, 30 с по часам worker'а). Ядро worker'а (`ferrum-wasm`) — команда `Export { format, outlineText, svgIds }`, событие `Exported { name, bytes, mime }`, страницы PDF (`pdf_pages`), «Копировать как SVG» (`ClipboardSvg`). Основной поток только сохраняет готовые байты: в браузере — скачиванием, в десктопе — командой Tauri `export_save` (родной диалог из Rust, атомарная запись).

**Tech Stack:** Rust 1.96 (новая зависимость — `krilla = "=0.8.2"` без фич по умолчанию, с `raster-images`; с ней в `Cargo.lock` — 23 крейта, в том числе `pdf-writer`, `subsetter`, свой `skrifa 0.42`; dev — `roxmltree =0.21.1` и `flate2 =1.1.10`; `parley 0.12.0`, `kurbo 0.13.1`, `vello_cpu 0.3.0`), SolidJS 1.9.17, TypeScript 6.0.3, @playwright/test 1.64.0 (`chromium`, `firefox`), Tauri 2.12.1 (`tauri-plugin-dialog 2.8.1`).

**Спеки:** `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` — раздел 11 и части 8 и 12, нужные этапу 5; спека ядра `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` («ядро §N», экспорт — §8.7).

## Global Constraints

- Ветка `feat/sub3-export` от `main` (e202acf, этапы 1–4 подсистемы 3 слиты). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Каталог сборки — у каждой копии свой: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0` (в `apps/web` — `$PWD/../../target`, в `apps/desktop/src-tauri` — `$PWD/../../../target`); команды — из корня копии, если не сказано иное.
- Одна новая внешняя зависимость — `krilla =0.8.2` (точная версия, `default-features = false, features = ["raster-images"]`: собирается под `wasm32-unknown-unknown`; `simple-text` с rustybuzz не нужен — глифы раскладывает parley); dev — `roxmltree =0.21.1` (разбор SVG в тестах) и `flate2 =1.1.10` (распаковка потоков PDF в тестах, версия уже в `Cargo.lock`). `package-lock.json` не меняется.
- Основной поток тонкий (ядро §4.2): масштаб, писатель SVG, PDF, страницы — в worker'е ядра. В основном потоке — секции панели, кнопка-список тулбара, запись в буфер обмена и сохранение готовых байтов; чтения layout — только в фазе `measure` планировщика (E2E проверяют `__ferrumWarnings`).
- Недоверенный ввод (спека 8, ядро §4.3): коэффициент поля «Масштаб» вне 0,01…100 и точка рамки вне {0, 0,5, 1}² — ошибка команды, а не зажим; имя файла экспорта в десктопе — без разделителей пути и управляющих символов, расширение — только `png`, `svg`, `pdf`.
- Одна правка — один шаг отмены: жест K (превью — в сцене, запись при отпускании), поле «Масштаб». Экспорт и «Копировать как SVG» документ не меняют.
- Пределы экспорта: файл ≤ 256 МБ (`MAX_EXPORT_BYTES`), время ≤ 30 с (`EXPORT_TIME_MS`, часы worker'а), буфер обмена ≤ 64 МБ (`MAX_CLIPBOARD_SVG`), сторона страницы PDF ≤ 14 400 pt; превышение — ошибка с сообщением, без частичного файла.
- Тесты не проходят вхолостую: редактор — документ, шаги отмены, свойства в px; писатель SVG — разбор XML (`roxmltree`), затем эталоны побайтно; PDF — словари страниц и шрифтов, распакованные потоки; ядро — события и байты; E2E — пиксели, шаги, разбор SVG браузером, страницы PDF.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Сначала прототип всего этапа собран в отдельной копии (`plan-verify-wt5`, ветка `verify/sub3-export` от e202acf, по коммиту на задачу; вершина — 6255d4f) и прошёл ворота: `cargo fmt --check`, `clippy -D warnings` (нативно и `wasm32-unknown-unknown`), `cargo test --workspace` (937 passed, 2 ignored), `render_report` и `editor_report` (release, 47 и 87 строк PASS, FAIL нет), wasm-сборка, `npm run typecheck`, модульные (136), сборка, E2E `chromium` целиком (`--grep-invert @perf` — 331 passed), `export`, `scale`, `booleans`, `vectors`, `text`, `roundtrip` в Firefox (7 passed), совместимость: `scripts/compat-old-build.sh 0797cf7 boolean_fixture` и `scripts/compat-old-build.sh main vector_fixture`, десктоп `apps/desktop/src-tauri` — `cargo check`, `clippy -D warnings`, юнит-тест имён экспорта. Крейт PDF выбран зондом: `krilla 0.8.2` без фич по умолчанию с `raster-images` собирается под `wasm32-unknown-unknown` (release, 25 с); 0.7.0 тянет `skrifa 0.37` и `png 0.17` — дальше от наших версий.

Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом, который читает сам этот документ, к чистой копии от e202acf (`plan-apply-wt5`, свой каталог сборки) в порядке задач: тесты шага 1 — и проверка, что каждая команда шага 2 действительно падает (как написано в «Expected»; E2E шага 2 задач 2 и 8 — после `npm run wasm && npm run build`, то есть против собранного стенда), затем реализация — каждый фрагмент «заменить» найден ровно один раз (операций по задачам: 46, 17, 14, 29, 24, 37, 34, 14, 10), эталоны SVG — командой шага генерации, после каждой задачи — все команды её шага «Проверить». После последней задачи дерево совпало с прототипом байт в байт (тот же хэш дерева у каждого из девяти коммитов, эталоны `svg_frame_shapes.svg`, `svg_vector_boolean.svg`, `svg_image_fills.svg`, `svg_text_elements.svg` — побайтно). Числа прогона на чистой копии: `cargo test --workspace` — 937 passed, 2 ignored (≈ 3 мин); `render_report` — SVG страницы из 10 000 узлов 36,9 мс (2624 КБ), PDF со 100 текстами и вектором в 100 000 точек 108,1 мс (729 КБ), цели ≤ 1000 мс, PNG фрейма @3x 97,2 мс (справочно); `editor_report` — K на 10 000 узлах: кадр жеста 30,1 мс (медиана, ≤ 50), первое движение 38,1 мс, запись жеста 110,6 мс (≤ 500), превью поля 30,6 мс (≤ 50), запись поля 88,4 мс (≤ 500), по шагу; FAIL нет ни в одном отчёте; `e2e:ci` задачи 9 — 331 passed за 7,2 мин, Firefox — 7 passed (16,7 с); совместимость — оба примера только для чтения; десктоп — `cargo check` и `clippy` чисто, `export_names` — 1 passed.

Порт 4173 на машине проверки занят посторонним процессом, поэтому E2E шли через временный конфиг Playwright с портами 4283/4284 (и временной правкой портов в `core.spec.ts` и `settings.spec.ts`, откатываемой после каждой команды), с `--retries=1` (шаг 2 — без повторов). Нестабильным был новый тест этапа `scale.spec.ts` «поле «Масштаб»: 50 % вокруг центра…» в проверке задачи 2: с первого раза сводка выделения после шага ещё была прежней (`[100, 0, 0, 10, 2]` вместо `[50, 25, 25, 5, 1]`), со второго — прошёл; в полном прогоне задачи 9 и в прототипе — с первого раза. Известные нестабильные прошлых этапов — `text-edit.spec.ts` «фрагмент со стилями…», `files.spec.ts` «1000 записей…», `picker.spec.ts` «обводка: ширина жестом…», `layers.spec.ts` «виртуальный список: в DOM только окно строк» — в этом прогоне прошли с первого раза.

Найденное прототипом и вошедшее в шаги: инструмент назван «Масштабирование» — с именем «Масштаб» E2E зума находили две кнопки (задача 1); у parley `Run::text_range()` — весь шейпинг, а не прогон кисти, текст прогона и кластеры берутся из `visual_clusters()` (задача 4); Chromium очищает `image/svg+xml` при записи в буфер — побайтно сверяется только `text/plain` (задача 5); krilla пишет словари PDF без пробелов (`/MediaBox[0 0 w h]`) — тесты разбирают их так (задача 6); заголовок `x-ferrum` моста Tauri должен быть ASCII — имена файлов экспорта идут `\uXXXX` (задача 8); точный PDF вектора в 100 000 точек — 8,8 МБ и 1,4 с, отсюда допуск упрощения (задача 9). Локальная ветка `main` машины проверки — старая (cd26851, до подсистемы 3); модель документа этап не меняет, проверки совместимости — прежние. `scripts/compat-old-build.sh` создаёт и удаляет свой каталог `mktemp -d`. Не запускались: проект Playwright `gpu` и замеры `@perf`, Safari/WebKit, запуск десктопа и живой родной диалог «Сохранить», замеры в wasm (только нативные отчёты), ручная проверка (задача 9), сторонние рендеры SVG и PDF; эталоны SVG созданы и сверены на одной машине (macOS, arm64). Тексты ожидаемых результатов — с этого прогона.

---

## Этапы подсистемы 3

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Текст: модель и рендер (слит) | узел `Text`, шрифты, раскладка, отрисовка, отсутствующие шрифты | 3, 4, 5, 6 |
| 2. Инструмент «Текст» и правка (слит) | T, правка на холсте, ввод и IME, курсор, буфер обмена, секция «Текст» | 7, 8 |
| 3. Векторные сети и перо (слит) | узел `Vector`, перо, режим правки, «Сплющить», окончания и соединения, стрелка | 9 |
| 4. Булевы операции (слит) | узел `Boolean`, операции и их смена, результат в сцене, «Сплющить» булевой группы и текста, «Контур обводки» | 10 |
| **5. Scale и экспорт SVG/PDF** (этот план) | инструмент K и секция «Масштаб», писатель SVG, «Копировать как SVG», PDF через `krilla`, раздел «Экспорт», родной диалог в десктопе | 11 |

Этап 5 — последний: после него подсистема 3 (векторы и текст) закрыта; что осталось за её пределами — в «Что этап 5 сознательно не делает».

## Решения, которых в спеке нет (на согласование)

Пользователь просил не останавливаться на вопросах: где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**.

Scale:

1. **(поведение)** K — всегда пропорционально (Shift ничего не меняет), Alt — от центра; зоны поворота нет; у одной линии — рамка из 8 ручек, масштаб по длине.
2. **(поведение)** Масштабируются: толщина обводок, радиус скругления, кегль, интерлиньяж и трекинг в px, интервал и отступ абзацев, разметка участков текста в px, радиусы точек вектора. Проценты и «Авто» — нет; предел острого угла — нет; окончания и стрелки зависят от толщины и растут сами. Картинка «Замостить» — без масштаба плитки (в модели его нет).
3. **(поведение)** Содержимое групп, булевых групп и фреймов масштабируется (у ресайза V дети фрейма — нет). Поддерево собирается без рекурсии (`bases_deep`), ребёнок фрейма — в осях фрейма после его правки.
4. **(поведение)** Повёрнутый узел сохраняет угол; одна рамка — своя повёрнутая, несколько — общая мировая.
5. **(поведение)** Без пиксельной привязки — дробные размеры, как в Figma.
6. **(поведение)** Текст «авто» сохраняет режим; рамка — по раскладке (целыми вверх), пока шрифт грузится — масштабированная рамка; значения стиля зажимаются в пределы секции «Текст».
7. **(поведение)** После жеста инструмент остаётся K; Esc — «Перемещение».
8. **(поведение)** Секция «Масштаб» — над «Положением» при K и выделении: сетка 3×3 (по умолчанию левый верхний угол), поле в процентах (показывает 100 %, масштаб — от текущего размера), 1–10 000 % (`MIN_SCALE_FACTOR = 0.01`, `MAX_SCALE_FACTOR = 100`).
9. **(поведение)** Название инструмента — «Масштабирование»: «Масштаб» уже занят кнопкой зума (первая редакция прототипа так и называлась — E2E зума `getByRole('button', { name: 'Масштаб', exact: true })` нашли две кнопки). В тулбаре — кнопка-список «Инструменты перемещения» с «Перемещением» и «Масштабированием»; команда `tool.scale` (K), команд 53 → 54.

SVG:

10. **(поведение)** «Текст кривыми» по умолчанию включён (как в Figma); без него — элемент `<text>` на прогон (`font-family`, `font-weight`, `font-style`, `letter-spacing`, позиции глифов); отсутствующий шрифт пишется запасным Inter.
11. **(поведение)** «Атрибут id» по умолчанию выключен; id — из имён слоёв, очищенные и уникальные (`_2`, `_3`…).
12. Обводки: по центру — `stroke`; внутри — двойная толщина с `<clipPath>` по форме; снаружи — двойная толщина с `<mask>`; окончания и стрелки — залитые фигуры; полупрозрачная обводка вместе с фигурами — `<g opacity>`.
13. Картинки — `<pattern>` с `<use>` картинки из `<defs>`: `data:` PNG, байты один раз на файл; без байтов заливка пропускается.
14. **(поведение)** Градиентов, режимов наложения и эффектов в модели нет — ни в SVG, ни в PDF их нет (писать нечего).
15. Группа без прозрачности и id своего `<g>` не получает (как Figma); сдвиг запекается в координаты, поворот и отражение — `transform="matrix(…)"`. Скрытые слои и дети булевых групп не пишутся.
16. **(поведение)** «Копировать как SVG» — без сочетания клавиш (как в Figma), пункт контекстного меню холста; текст всегда кривыми; не больше 64 МБ. Chromium очищает `image/svg+xml` при записи в буфер — побайтно совпадает только `text/plain`.

PDF:

17. **(поведение)** Страницы: есть выделение — узел верхнего уровня выделения на страницу; нет — фреймы верхнего уровня; нет фреймов — вся страница одной; порядок — чтения (сверху вниз, слева направо). 1 px = 1 pt; сторона больше 14 400 pt — страница уменьшается целиком.
18. Текст сплошной краской — настоящий текст (подмножество шрифта, ToUnicode из кластеров); текст картинкой и обводки текста — контуры. Вариативный шрифт: вес и ширина — из начертания прогона, прочие оси — из нормализованных координат без `avar`. Если krilla отказал на шрифте — весь файл повторяется с текстом контурами.
19. Большие векторы (4096 элементов и больше) упрощаются с допуском не больше 1/64 pt (`PDF_DETAIL_SCALE = 16`): точный контур вектора в 100 000 точек — 8,8 МБ и 1,4 с, с допуском — 0,7 МБ и 0,11 с.
20. «Замостить» — плитками, не больше 4096 на заливку (больше — картинка без плиток). Теги PDF (доступность) выключены.

Экспорт и протокол:

21. **(поведение)** Формат — список PNG/SVG/PDF (JPG нет: в ядре нет кодировщика); масштаб 1×/2×/3× — только у PNG; у SVG — флажки «Текст кривыми» и «Атрибут id».
22. **(поведение)** Без выделения PNG и SVG — вся страница одним файлом (как было у PNG), PDF — по фрейму на страницу.
23. **(поведение)** Имена: «Слой@2x.png», «Слой.svg», «Страница 1.pdf»; несколько выделенных — «Выделение.*».
24. Пределы — 256 МБ и 30 с (часы `performance.now()` в worker'е, `Core::set_export_clock`); экспорт — в worker'е, основной поток не стоит. Шрифты ещё грузятся — ошибка (как у PNG); нечитаемые картинки в PDF — ошибка.
25. **(поведение)** Десктоп: любой экспорт (и PNG) сохраняется родным диалогом «Сохранить» из Rust (`export_save`: байты — сырым телом IPC, имя — заголовком `x-ferrum` в ASCII-JSON, фильтр по типу, атомарная запись); отказ в диалоге — без уведомления; путь интерфейс не видит. Снимает «экспорт PNG в десктопе родным диалогом» из отложенного ядра.
26. Протокол: `ToolKind::Scale`, `Edit::Scale { factor, anchor }`, `Command::Export { scale, format?, outlineText?, svgIds? }` (без `format` — PNG, старые клиенты не ломаются), `CoreEvent::Exported { name, bytes, mime }`, `CoreEvent::ClipboardSvg { text }`. Совместимость документа не затронута — модель не менялась, проверки совместимости прежние.

## Как план закрывает повторявшиеся дефекты этапов 2–4

- **Поворот и отражение.** K сохраняет угол повёрнутого узла (своя повёрнутая рамка у одного, общая мировая у нескольких); SVG пишет поворот и отражение матрицей, без масштаба узлов — толщина обводок не искажается; PDF рисует тот же рисовальщик, что экран.
- **Превью и запись — один шаг.** Жест K и поле «Масштаб» — превью в сцене и одна транзакция (тесты сверяют число шагов, отмену и повтор; `editor_report` — «по шагу»).
- **Чтения layout вне `measure`.** Новый код основного потока — JSX, команды и запись готовых байтов; E2E проверяют `__ferrumWarnings`.
- **Клавиши.** K — по физическому `code` (`KeyK`); «Копировать как SVG» — без сочетания, как в Figma (тест раскладки разрешает пустое сочетание только ей).
- **Недоверенные значения отклоняются, а не зажимаются** — коэффициент и точка рамки из протокола, имя файла и расширение в десктопе.
- **Проверка на каждый вариант:** обводки по трём положениям, обрезка фреймов, картинки «Заполнить/Вписать/Замостить», текст кривыми и `<text>`, страницы PDF; затем эталоны SVG побайтно.
- **Бюджет.** Замеры в `render_report` (SVG 10 000 узлов, PDF с текстом и вектором 100 000 точек) и `editor_report` (K на 10 000 узлах: кадр жеста, первое движение, запись, поле). Точный PDF вектора в 100 000 точек стоил 1,4 с — отсюда допуск упрощения (задача 9).

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-editor/src/scale.rs` (новый), `{geometry,panel,editor,commands,actions,lib}.rs`, `tools/{mod,select}.rs` | инструмент K, коэффициент, позы и свойства поддерева, правка панели |
| `crates/ferrum-render/src/svg.rs` (новый), `export.rs` (новый), `{lib,text_node}.rs` | писатель SVG, общая с PNG область экспорта, пределы |
| `crates/ferrum-text/src/layout.rs` | текст и кластеры прогона для `<text>` и ToUnicode |
| `crates/ferrum-render/src/pdf.rs` (новый), `backend.rs` | PDF через `krilla`, бэкенд рисовальщика, `fill_glyphs` |
| `crates/ferrum-wasm/src/{protocol,core}.rs`, `core/panels.rs`, `web/mod.rs` | `Tool::Scale`, `Edit::Scale`, `Export`, `Exported`, `ClipboardSvg`, страницы PDF, имена |
| `apps/web/src/ui/{toolbar,properties,icons,chrome,app,system-clipboard,tauri}.tsx/ts`, `styles/properties.css` | кнопка-список, секции «Масштаб» и «Экспорт», буфер обмена, сохранение |
| `apps/desktop/src-tauri/src/lib.rs` | `export_save` — родной диалог «Сохранить» |
| `apps/web/e2e/{scale,export,clipboard,…}.spec.ts`, `apps/web/unit/tauri.test.ts` | E2E и модульные |
| `crates/{ferrum-render,ferrum-editor}/examples/*_report.rs`, спеки | замеры; решения этапа 5 |

---

### Task 1: Инструмент «Масштабирование» (K) в редакторе

Спека 11: изменение размера выделения инструментом K масштабирует и обводки, радиусы, размеры текста, интерлиньяж и трекинг в px. Здесь — `ToolKind::Scale` на основе `SelectTool` (всегда пропорционально, Alt — от центра, без зоны поворота), позы поддерева групп, булевых групп и фреймов (`bases_deep`, `scale_patches_deep`), свойства в px (`scale_props`), правка панели `PanelEdit::Scale` тем же путём превью и записи; команда `tool.scale` (K). В протоколе пока только `Tool::Scale`.

**Files:**
- Create: `crates/ferrum-editor/src/scale.rs`
- Modify: `crates/ferrum-editor/src/{geometry,panel,editor,commands,actions,lib}.rs`, `crates/ferrum-editor/src/tools/{mod,select}.rs`, `crates/ferrum-wasm/src/{protocol,core}.rs`
- Test: `crates/ferrum-editor/tests/scale.rs` (новый), `crates/ferrum-editor/tests/keymap.rs` (команд 53 → 54), `crates/ferrum-wasm/tests/{boolean,panels,protocol}.rs`, `apps/web/e2e/{app,settings}.spec.ts` (`tool.scale` в тестах неизвестной команды заменён на `tool.slice`)

**Interfaces:**
- Consumes: `geometry::Base`, `ResizeFrame`, `PanelGesture`, `NodeKind::is_group_like`, раскладка текста (`Fonts`).
- Produces: `ToolKind::Scale`, `ToolKind::is_select_like()` (Move | Scale — наведение, Alt-измерения); `CommandId::ToolScale` (`tool.scale`, «Масштабирование», `KeyK`); `SelectTool::scaling()`; `geometry::bases_deep(scene, ids) -> Vec<Base>`, `geometry::scale_patches_deep(bases, t, frame) -> Vec<(NodeId, PropsPatch)>`; `scale::{Anchor { fx, fy } (new, CENTER, TOP_LEFT), MIN_SCALE_FACTOR = 0.01, MAX_SCALE_FACTOR = 100, uniform_factor, anchor_transform, scale_props, scale_patches}`; `PanelEdit::Scale { factor, anchor }`; `Tool::Scale` протокола.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 53, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 54, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `apps/web/e2e/settings.spec.ts` заменить:

````ts
  const mod = await page.evaluate(() => (navigator.platform.toLowerCase().startsWith('mac') ? 'Meta' : 'Ctrl'));
  const dialog = await openSettings(page);
  const json = JSON.stringify({ format: 'ferrum-keymap', version: 1, overrides: { 'edit.duplicate': [`${mod}+KeyW`], 'tool.scale': ['KeyK'] } });
  await dialog.locator('input[type=file]').setInputFiles({ name: 'keys.json', mimeType: 'application/json', buffer: Buffer.from(json) });
  await expect(page.getByRole('status').filter({ hasText: /Раскладка импортирована, пропущено: 1/ })).toBeVisible();
````

на:

````ts
  const mod = await page.evaluate(() => (navigator.platform.toLowerCase().startsWith('mac') ? 'Meta' : 'Ctrl'));
  const dialog = await openSettings(page);
  const json = JSON.stringify({ format: 'ferrum-keymap', version: 1, overrides: { 'edit.duplicate': [`${mod}+KeyW`], 'tool.slice': ['KeyS'] } });
  await dialog.locator('input[type=file]').setInputFiles({ name: 'keys.json', mimeType: 'application/json', buffer: Buffer.from(json) });
  await expect(page.getByRole('status').filter({ hasText: /Раскладка импортирована, пропущено: 1/ })).toBeVisible();
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    (CommandId::ToolLine, &["KeyL"]),
    (CommandId::ToolHand, &["KeyH"]),
    (CommandId::HandHold, &["Space"]),
    (CommandId::ToolImage, &["Mod+Shift+KeyK"]),
````

на:

````rust
    (CommandId::ToolLine, &["KeyL"]),
    (CommandId::ToolHand, &["KeyH"]),
    (CommandId::ToolScale, &["KeyK"]),
    (CommandId::HandHold, &["Space"]),
    (CommandId::ToolImage, &["Mod+Shift+KeyK"]),
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 53);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 54);
}

````

Создать `crates/ferrum-editor/tests/scale.rs`:

````rust
//! Инструмент «Масштаб» (K, спека подсистемы 3, раздел 11): ручки и поле
//! «Масштаб» панели масштабируют выделение равномерно вместе с обводками,
//! радиусами, текстом, окончаниями и содержимым групп и фреймов — одним шагом.

mod common;

use std::sync::Arc;

use common::text::{add_text, text_h};
use common::*;
use ferrum_doc::{
    Color, LetterSpacing, LineHeight, NodeKind, PropsPatch, Stroke, StrokeAlign, StrokeCap,
    TextAttrs, TextContent, TextRun, TextSizing, VectorNetwork, VectorPoint, VectorSegment,
};
use ferrum_editor::scale::{Anchor, MAX_SCALE_FACTOR};
use ferrum_editor::{CommandId, PanelEdit, Phase, ToolKind};

fn stroke(w: f64) -> Stroke {
    Stroke {
        color: Color::BLACK,
        width: w,
        align: StrokeAlign::Inside,
    }
}

/// Прямоугольник 100×100 в (200, 200) с обводкой 2 и радиусом 10, выделен; K.
fn styled_rect(h: &mut H) -> ferrum_doc::NodeId {
    let id = h.rect(None, 200.0, 200.0, 100.0, 100.0);
    h.set(
        id,
        PropsPatch {
            strokes: Some(vec![stroke(2.0)]),
            corner_radius: Some(10.0),
            ..Default::default()
        },
    );
    h.ed.select(&[id]);
    h.ed.execute(CommandId::ToolScale);
    id
}

#[test]
fn k_selects_the_scale_tool() {
    let mut h = H::new();
    h.key("KeyK", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Scale);
    h.key("Escape", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
}

#[test]
fn corner_drag_scales_size_stroke_and_radius_in_one_step() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag((300.0, 300.0), (400.0, 400.0), none());
    let p = h.props(id);
    assert!(close(p.w, 200.0) && close(p.h, 200.0), "{} × {}", p.w, p.h);
    assert!(close(p.x, 200.0) && close(p.y, 200.0));
    assert!(close(p.strokes[0].width, 4.0), "{}", p.strokes[0].width);
    assert!(close(p.corner_radius, 20.0));
    assert_eq!(h.ed.tool_kind(), ToolKind::Scale, "инструмент остаётся");
    h.assert_one_step(steps, &before);
}

#[test]
fn plain_resize_does_not_scale_properties() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    h.ed.execute(CommandId::ToolMove);
    h.drag((300.0, 300.0), (400.0, 400.0), none());
    let p = h.props(id);
    assert!(close(p.w, 200.0));
    assert!(close(p.strokes[0].width, 2.0));
    assert!(close(p.corner_radius, 10.0));
}

#[test]
fn side_handle_keeps_proportions() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    // Правая сторона на +50: и высота растёт в 1,5 раза (от середины левой стороны).
    h.drag((300.0, 250.0), (350.0, 250.0), none());
    let p = h.props(id);
    assert!(close(p.w, 150.0) && close(p.h, 150.0), "{} × {}", p.w, p.h);
    assert!(close(p.y, 175.0), "y {}", p.y);
    assert!(close(p.strokes[0].width, 3.0));
}

#[test]
fn alt_scales_from_center() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    h.drag((300.0, 300.0), (350.0, 350.0), alt());
    let p = h.props(id);
    assert!(close(p.w, 200.0), "{}", p.w);
    assert!(close(p.x, 150.0) && close(p.y, 150.0));
}

#[test]
fn escape_cancels_without_a_step() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    let steps = h.ed.step_count();
    h.drag_hold((300.0, 300.0), (400.0, 400.0), none());
    assert!(h.ed.scene().has_preview());
    h.key("Escape", none());
    assert_eq!(h.ed.step_count(), steps);
    assert!(close(h.props(id).w, 100.0));
    h.assert_scene_clean();
}

#[test]
fn group_and_frame_content_scales_with_them() {
    let mut h = H::new();
    let frame = h.frame(None, 0.0, 0.0, 100.0, 100.0);
    let child = h.rect(Some(frame), 10.0, 10.0, 20.0, 20.0);
    h.set(
        child,
        PropsPatch {
            strokes: Some(vec![stroke(1.0)]),
            ..Default::default()
        },
    );
    let a = h.rect(None, 300.0, 300.0, 50.0, 50.0);
    let b = h.rect(None, 400.0, 300.0, 50.0, 50.0);
    h.ed.select(&[a, b]);
    h.ed.execute(CommandId::Group);
    let group = h.sel()[0];
    // Фрейм: ×2 от левого верхнего угла — ребёнок вдвое дальше и вдвое больше.
    h.ed.select(&[frame]);
    h.ed.execute(CommandId::ToolScale);
    h.drag((100.0, 100.0), (200.0, 200.0), none());
    let f = h.props(frame);
    assert!(close(f.w, 200.0) && close(f.h, 200.0));
    let c = h.props(child);
    assert!(close(c.x, 20.0) && close(c.y, 20.0), "{} {}", c.x, c.y);
    assert!(close(c.w, 40.0) && close(c.strokes[0].width, 2.0));
    // Группа: дети масштабируются в её осях.
    h.ed.select(&[group]);
    h.drag((450.0, 350.0), (525.0, 375.0), none());
    let (pa, pb) = (h.props(a), h.props(b));
    assert!(
        close(pa.w, 75.0) && close(pb.w, 75.0),
        "{} {} {:?}",
        pa.w,
        pb.w,
        (pa.x, pb.x)
    );
    assert!(close(pb.x - pa.x, 150.0), "{}", pb.x - pa.x);
    h.assert_scene_clean();
}

#[test]
fn panel_factor_scales_around_the_anchor_in_one_step() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 0.5,
            anchor: Anchor::CENTER,
        },
        Phase::Commit,
    );
    let p = h.props(id);
    assert!(close(p.w, 50.0) && close(p.x, 225.0) && close(p.y, 225.0));
    assert!(close(p.strokes[0].width, 1.0) && close(p.corner_radius, 5.0));
    h.assert_one_step(steps, &before);
    // Левый верхний угол стоит на месте.
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 3.0,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    let p = h.props(id);
    assert!(close(p.x, 225.0) && close(p.w, 150.0));
}

#[test]
fn panel_preview_and_cancel_leave_no_trace() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    let steps = h.ed.step_count();
    let edit = PanelEdit::Scale {
        factor: 2.0,
        anchor: Anchor::CENTER,
    };
    h.ed.edit_selection(&edit, Phase::Preview);
    let n = h.ed.scene().node(id).unwrap();
    assert!(close(n.props.w, 200.0), "превью в сцене");
    h.ed.edit_selection(&edit, Phase::Cancel);
    assert_eq!(h.ed.step_count(), steps);
    h.assert_scene_clean();
    // Сто процентов и нечисла — ничего; сверх предела — зажимается.
    for f in [1.0, f64::NAN, f64::INFINITY] {
        h.ed.edit_selection(
            &PanelEdit::Scale {
                factor: f,
                anchor: Anchor::CENTER,
            },
            Phase::Commit,
        );
    }
    assert_eq!(h.ed.step_count(), steps);
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 1e9,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    assert!(close(h.props(id).w, 100.0 * MAX_SCALE_FACTOR));
}

#[test]
fn rotated_node_keeps_its_angle() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    h.set(
        id,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 2.0,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    let p = h.props(id);
    assert!(close(p.rotation, 30.0), "{}", p.rotation);
    assert!(close(p.w, 200.0) && close(p.h, 200.0));
    // Якорь — угол повёрнутой рамки узла: он на месте.
    assert!(close(p.x, 200.0) && close(p.y, 200.0), "{} {}", p.x, p.y);
}

#[test]
fn text_sizes_spacing_and_runs_scale_and_auto_width_stays() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет мир", 12.0);
    let mut t = (*h.props(id).text.unwrap()).clone();
    t.props.style.line_height = LineHeight::Px(20.0);
    t.props.style.letter_spacing = LetterSpacing::Px(1.0);
    t.props.paragraph_spacing = 4.0;
    let content = TextContent {
        runs: vec![
            TextRun {
                text: "Привет ".into(),
                attrs: TextAttrs {
                    size: Some(16.0),
                    line_height: Some(LineHeight::Percent(120.0)),
                    ..Default::default()
                },
            },
            TextRun {
                text: "мир\n".into(),
                attrs: TextAttrs::default(),
            },
        ],
    };
    h.set(
        id,
        PropsPatch {
            text: Some(t.props.clone()),
            content: Some(content),
            ..Default::default()
        },
    );
    let before = h.props(id);
    let w0 = ferrum_editor::fit_size(&mut h.ed.fonts().borrow_mut(), &before)
        .unwrap()
        .unwrap()
        .0;
    h.ed.select(&[id]);
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 2.0,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    let p = h.props(id);
    let t = p.text.as_ref().unwrap();
    assert_eq!(
        t.props.sizing,
        TextSizing::AutoWidth,
        "режим размера тот же"
    );
    assert!(close(t.props.style.size, 24.0));
    assert_eq!(t.props.style.line_height, LineHeight::Px(40.0));
    assert_eq!(t.props.style.letter_spacing, LetterSpacing::Px(2.0));
    assert!(close(t.props.paragraph_spacing, 8.0));
    assert_eq!(t.content.runs[0].attrs.size, Some(32.0));
    assert_eq!(
        t.content.runs[0].attrs.line_height,
        Some(LineHeight::Percent(120.0)),
        "проценты не масштабируются"
    );
    // Рамка «авто» — по раскладке: почти вдвое шире.
    assert!((p.w - 2.0 * w0).abs() <= 2.0, "{} против {}", p.w, 2.0 * w0);
}

#[test]
fn arrow_and_vector_point_radius_scale() {
    let mut h = H::new();
    let net = VectorNetwork {
        points: vec![
            VectorPoint {
                corner_radius: 3.0,
                ..VectorPoint::new(0.0, 0.0)
            },
            VectorPoint::new(100.0, 50.0),
        ],
        segments: vec![VectorSegment::line(0, 1)],
        regions: vec![],
    };
    let id = h.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            x: Some(100.0),
            y: Some(100.0),
            w: Some(100.0),
            h: Some(50.0),
            vector: Some(Arc::new(net)),
            stroke_cap_end: Some(StrokeCap::ArrowLines),
            ..Default::default()
        },
    );
    h.ed.select(&[id]);
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 2.0,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    let p = h.props(id);
    assert!(close(p.w, 200.0) && close(p.h, 100.0));
    assert!(
        close(p.strokes[0].width, 2.0),
        "окончание строится от толщины"
    );
    assert_eq!(p.stroke_style.cap_end, StrokeCap::ArrowLines);
    assert!(close(
        p.vector.as_ref().unwrap().points[0].corner_radius,
        6.0
    ));
}

#[test]
fn line_scales_by_its_length() {
    let mut h = H::new();
    let id = h.add(
        None,
        NodeKind::Line,
        PropsPatch {
            x: Some(100.0),
            y: Some(100.0),
            w: Some(100.0),
            ..Default::default()
        },
    );
    h.ed.select(&[id]);
    h.ed.execute(CommandId::ToolScale);
    // У «Масштаба» у линии — рамка: правый конец — угол рамки.
    h.drag((200.0, 100.0), (300.0, 100.0), none());
    let p = h.props(id);
    assert!(close(p.w, 200.0), "{}", p.w);
    assert!(close(p.strokes[0].width, 2.0));
}
````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 53);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

на:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 54);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
        "без английского текста разбора"
    );
    // Неизвестная команда (Scale — этап 5 подсистемы 3) и непонятное сочетание.
    core.command(Command::Bind {
        command: "tool.scale".into(),
        shortcut: "KeyK".into(),
        replace: true,
    });
````

на:

````rust
        "без английского текста разбора"
    );
    // Неизвестная команда («Нарезка» Figma у нас нет) и непонятное сочетание.
    core.command(Command::Bind {
        command: "tool.slice".into(),
        shortcut: "KeyS".into(),
        replace: true,
    });
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        json!({"type": "demo", "nodes": -1}),
        json!({"type": "demo", "nodes": 1, "extra": 1}),
        json!({"type": "setTool", "tool": "scale"}),
        json!({"tool": "rect"}),
        json!("save"),
````

на:

````rust
        json!({"type": "demo", "nodes": -1}),
        json!({"type": "demo", "nodes": 1, "extra": 1}),
        json!({"type": "setTool", "tool": "slice"}),
        json!({"tool": "rect"}),
        json!("save"),
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test scale`
Expected: FAIL — не собирается: нет модуля `ferrum_editor::scale`, `ToolKind::Scale`, `CommandId::ToolScale`, варианта `PanelEdit::Scale` (14 ошибок)

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::ToolPen => self.set_tool(ToolKind::Pen),
            C::ToolArrow => self.set_tool(ToolKind::Arrow),
            C::Flatten => self.flatten(),
            C::BooleanUnion => self.boolean(ferrum_doc::BooleanOp::Union),
````

на:

````rust
            C::ToolPen => self.set_tool(ToolKind::Pen),
            C::ToolArrow => self.set_tool(ToolKind::Arrow),
            C::ToolScale => self.set_tool(ToolKind::Scale),
            C::Flatten => self.flatten(),
            C::BooleanUnion => self.boolean(ferrum_doc::BooleanOp::Union),
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// «Стрелка»: линия со стрелкой на конце.
    ToolArrow,
    /// «Сплющить»: выделение — в один вектор.
    Flatten,
````

на:

````rust
    /// «Стрелка»: линия со стрелкой на конце.
    ToolArrow,
    /// «Масштаб» (K, подсистема 3, этап 5).
    ToolScale,
    /// «Сплющить»: выделение — в один вектор.
    Flatten,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    cmd(C::ToolText, "tool.text", "Текст", &["KeyT"]),
    cmd(C::ToolPen, "tool.pen", "Перо", &["KeyP"]),
    cmd(C::ToolArrow, "tool.arrow", "Стрелка", &["Shift+KeyL"]),
    cmd(
````

на:

````rust
    cmd(C::ToolText, "tool.text", "Текст", &["KeyT"]),
    cmd(C::ToolPen, "tool.pen", "Перо", &["KeyP"]),
    cmd(C::ToolScale, "tool.scale", "Масштабирование", &["KeyK"]),
    cmd(C::ToolArrow, "tool.arrow", "Стрелка", &["Shift+KeyL"]),
    cmd(
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// точка указателя. Наведение есть только у «Перемещения».
    fn rehover(&mut self, point: Option<Point>, modifiers: Option<Modifiers>) {
        if self.is_busy() || self.tool.kind() != ToolKind::Move {
            return;
        }
````

на:

````rust
    /// точка указателя. Наведение есть только у «Перемещения».
    fn rehover(&mut self, point: Option<Point>, modifiers: Option<Modifiers>) {
        if self.is_busy() || !self.tool.kind().is_select_like() {
            return;
        }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.tool = match kind {
            ToolKind::Move => Box::new(SelectTool::new()),
            ToolKind::Frame => Box::new(DrawTool::new(Shape::Frame)),
            ToolKind::Rect => Box::new(DrawTool::new(Shape::Rect)),
````

на:

````rust
        self.tool = match kind {
            ToolKind::Move => Box::new(SelectTool::new()),
            ToolKind::Scale => Box::new(SelectTool::scaling()),
            ToolKind::Frame => Box::new(DrawTool::new(Shape::Frame)),
            ToolKind::Rect => Box::new(DrawTool::new(Shape::Rect)),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            || self.is_busy()
            || self.panel.is_some()
            || self.tool.kind() != ToolKind::Move
            || self.selection.is_empty()
            || over_box
````

на:

````rust
            || self.is_busy()
            || self.panel.is_some()
            || !self.tool.kind().is_select_like()
            || self.selection.is_empty()
            || over_box
````

В `crates/ferrum-editor/src/geometry.rs` заменить:

````rust
}

/// Какие оси округлять до пикселя. Ось, которую притянул сосед (раздел 7.3 спеки),
/// не округляется: привязка к соседу важнее сетки пикселей.
````

на:

````rust
}

/// Базы для инструмента «Масштаб» (K): узлы верхнего уровня и всё их поддерево —
/// у групп, булевых групп и фреймов (масштаб меняет и содержимое фреймов, в
/// отличие от ресайза). Обход без рекурсии: глубина дерева из файла не ограничена.
pub fn bases_deep(scene: &Scene, ids: &[NodeId]) -> Vec<Base> {
    let mut out = Vec::new();
    for id in ids {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        let n = scene.at(slot);
        out.push(Base {
            id: n.id,
            props: n.props.clone(),
            local: n.local,
            parent: None,
            parent_world: parent_world(scene, slot),
        });
        let mut stack = vec![(slot, out.len() - 1)];
        while let Some((s, at)) = stack.pop() {
            for &c in &scene.at(s).children {
                let cn = scene.at(c);
                out.push(Base {
                    id: cn.id,
                    props: cn.props.clone(),
                    local: cn.local,
                    parent: Some(at),
                    parent_world: Affine::IDENTITY,
                });
                stack.push((c, out.len() - 1));
            }
        }
    }
    out
}

/// Правки положения и размера для равномерного масштаба `t` в мире (инструмент
/// «Масштаб»): как `scale_patches_in`, но поддерево фреймов тоже масштабируется —
/// ребёнок фрейма получает масштаб в осях фрейма после его правки. Без привязки к
/// пикселям: округление нарушило бы пропорции содержимого. `bases` — из
/// `bases_deep` (родитель всегда раньше ребёнка).
pub fn scale_patches_deep(bases: &[Base], t: Affine, frame: Affine) -> Vec<(NodeId, PropsPatch)> {
    let t = if finite_affine(t) {
        t
    } else {
        Affine::IDENTITY
    };
    let axes = mirror_axes(t, frame);
    let off = PixelSnap { x: false, y: false };
    // Масштаб в координатах родителя каждого узла и его новая локальная матрица.
    let mut in_parent: Vec<Affine> = Vec::with_capacity(bases.len());
    let mut new_local: Vec<Affine> = Vec::with_capacity(bases.len());
    let mut out = Vec::with_capacity(bases.len());
    for b in bases {
        let tp = match b.parent {
            None => b.parent_world.inverse() * t * b.parent_world,
            // Мир ребёнка после правки — новая матрица родителя × новая своя; она
            // должна совпасть с масштабом родителя поверх прежних матриц.
            Some(i) => new_local[i].inverse() * in_parent[i] * bases[i].local,
        };
        in_parent.push(tp);
        if b.props.kind.is_group_like() {
            new_local.push(b.local);
            continue;
        }
        let m = tp * b.local;
        let hint = match axes {
            Some((ax, ay)) => (b.props.flip_x ^ ax, b.props.flip_y ^ ay),
            None => mirror_hint(&b.props, b.local, m),
        };
        let pose = if b.props.kind == NodeKind::Line {
            decompose_hinted(m, b.props.w, b.props.h, b.props.rotation, hint)
        } else {
            decompose_centred(m, b.props.w, b.props.h, b.props.rotation, hint)
        };
        let patch = pose_patch(&b.props, pose, true, off);
        new_local.push(local_with(&b.props, &patch));
        out.push((b.id, patch));
    }
    out
}

/// Какие оси округлять до пикселя. Ось, которую притянул сосед (раздел 7.3 спеки),
/// не округляется: привязка к соседу важнее сетки пикселей.
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod panel;
pub mod presets;
pub mod selection;
pub mod snap;
````

на:

````rust
pub mod panel;
pub mod presets;
pub mod scale;
pub mod selection;
pub mod snap;
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
use crate::editor::Editor;
use crate::geometry::{
    bases, normalize_deg, parent_world, rigid_patches, scale_patches_in, top_level, Base,
    PixelSnap, MIN_SIDE,
};
use crate::ops::reposition;
use crate::pages::clean_name;
use crate::text::resize_texts;

````

на:

````rust
use crate::editor::Editor;
use crate::geometry::{
    bases, bases_deep, normalize_deg, parent_world, rigid_patches, scale_patches_in, top_level,
    Base, PixelSnap, SelectionBox, MIN_SIDE,
};
use crate::ops::reposition;
use crate::pages::clean_name;
use crate::scale::{anchor_transform, scale_patches, Anchor, MAX_SCALE_FACTOR, MIN_SCALE_FACTOR};
use crate::text::resize_texts;

````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
        miter_limit: Option<f64>,
    },
}

````

на:

````rust
        miter_limit: Option<f64>,
    },
    /// Поле «Масштаб» инструмента K: выделение (верхний уровень) масштабируется
    /// равномерно в `factor` раз вокруг точки `anchor` его рамки вместе с
    /// обводками, радиусами и текстом (`crate::scale`). Вне 1 %…10 000 % —
    /// зажимается.
    Scale { factor: f64, anchor: Anchor },
}

````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
    /// Базы групп собраны вместе с поддеревом (нужно только для размера).
    deep: bool,
}

````

на:

````rust
    /// Базы групп собраны вместе с поддеревом (нужно только для размера).
    deep: bool,
    /// Для поля «Масштаб»: всё поддерево верхнего уровня и рамка выделения.
    scale: Option<(Vec<Base>, SelectionBox)>,
}

````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
        })
        .collect();
    PanelGesture { targets, deep }
}

````

на:

````rust
        })
        .collect();
    PanelGesture {
        targets,
        deep,
        scale: None,
    }
}

/// Жест поля «Масштаб»: узлы верхнего уровня выделения с поддеревом и их рамка
/// (у одного узла — его повёрнутая рамка, как у ручек).
fn capture_scale(scene: &Scene, ids: &[NodeId]) -> PanelGesture {
    let mut g = capture(scene, ids, false);
    let tops: Vec<NodeId> = g.targets.iter().filter(|t| t.top).map(|t| t.id).collect();
    g.scale = SelectionBox::of(scene, &tops).map(|sbox| (bases_deep(scene, &tops), sbox));
    g
}

/// Правки поля «Масштаб».
fn scale_edit(
    g: &PanelGesture,
    factor: f64,
    anchor: Anchor,
    fonts: &mut ferrum_text::Fonts,
) -> Vec<(NodeId, PropsPatch)> {
    let Some((bases, sbox)) = &g.scale else {
        return vec![];
    };
    if !factor.is_finite() {
        return vec![];
    }
    let k = factor.clamp(MIN_SCALE_FACTOR, MAX_SCALE_FACTOR);
    if (k - 1.0).abs() < 1e-12 {
        return vec![];
    }
    let t = anchor_transform(sbox, anchor, k);
    scale_patches(bases, t, sbox.to_world, k, Some(fonts))
}

/// Правки жеста панели для `edit` (с правилами ресайза текстов).
fn gesture_patches(
    g: &PanelGesture,
    edit: &PanelEdit,
    fonts: &mut ferrum_text::Fonts,
) -> Vec<(NodeId, PropsPatch)> {
    if let PanelEdit::Scale { factor, anchor } = edit {
        return scale_edit(g, *factor, *anchor, fonts);
    }
    let mut p = patches(g, edit);
    resize_texts(fonts, gesture_bases(g), &mut p);
    p
}

/// Снимок начала жеста под правку `edit`.
fn capture_for(scene: &Scene, ids: &[NodeId], edit: &PanelEdit) -> PanelGesture {
    if matches!(edit, PanelEdit::Scale { .. }) {
        capture_scale(scene, ids)
    } else {
        capture(scene, ids, needs_children(edit))
    }
}

/// Снимок жеста годится для правки `edit` (иначе собирается заново).
fn gesture_fits(g: &PanelGesture, edit: &PanelEdit) -> bool {
    if matches!(edit, PanelEdit::Scale { .. }) {
        g.scale.is_some()
    } else {
        g.deep || !needs_children(edit)
    }
}

````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
                }
            }
            PanelEdit::ClipContent(on) => {
                if kind == NodeKind::Frame {
````

на:

````rust
                }
            }
            // Считается целиком в `scale_edit`.
            PanelEdit::Scale { .. } => {}
            PanelEdit::ClipContent(on) => {
                if kind == NodeKind::Frame {
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
            Phase::Cancel => self.cancel_panel(),
            Phase::Preview => {
                let deep = needs_children(edit);
                // Жест начался с правки без поддерева групп, а теперь оно нужно:
                // превью снимается, базы собираются заново.
                if self.panel.as_ref().is_some_and(|g| deep && !g.deep) {
                    self.cancel_panel();
                }
                if self.panel.is_none() {
                    self.cancel_gesture();
                    self.panel = Some(capture(&self.scene, &targets(self), deep));
                }
                let g = self.panel.as_ref().expect("задан выше");
                let mut p = patches(g, edit);
                resize_texts(&mut self.fonts.borrow_mut(), gesture_bases(g), &mut p);
                self.scene.set_preview(&p);
            }
            Phase::Commit => {
                let deep = needs_children(edit);
                let g = match self.panel.take() {
                    Some(g) if g.deep || !deep => g,
                    _ => {
                        // Адресат задан ids — жест холста, начавшийся позже, не трогаем.
````

на:

````rust
            Phase::Cancel => self.cancel_panel(),
            Phase::Preview => {
                // Жест начался с правки без поддерева групп (или не масштабом), а
                // теперь оно нужно: превью снимается, базы собираются заново.
                if self.panel.as_ref().is_some_and(|g| !gesture_fits(g, edit)) {
                    self.cancel_panel();
                }
                if self.panel.is_none() {
                    self.cancel_gesture();
                    self.panel = Some(capture_for(&self.scene, &targets(self), edit));
                }
                let g = self.panel.as_ref().expect("задан выше");
                let p = gesture_patches(g, edit, &mut self.fonts.borrow_mut());
                self.scene.set_preview(&p);
            }
            Phase::Commit => {
                let g = match self.panel.take() {
                    Some(g) if gesture_fits(&g, edit) => g,
                    _ => {
                        // Адресат задан ids — жест холста, начавшийся позже, не трогаем.
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
                        }
                        self.scene.clear_preview();
                        capture(&self.scene, &targets(self), deep)
                    }
                };
                let mut p = patches(&g, edit);
                resize_texts(&mut self.fonts.borrow_mut(), gesture_bases(&g), &mut p);
                self.scene.clear_preview();
                if !p.is_empty() {
````

на:

````rust
                        }
                        self.scene.clear_preview();
                        capture_for(&self.scene, &targets(self), edit)
                    }
                };
                let p = gesture_patches(&g, edit, &mut self.fonts.borrow_mut());
                self.scene.clear_preview();
                if !p.is_empty() {
````

Создать `crates/ferrum-editor/src/scale.rs`:

````rust
//! Инструмент «Масштаб» (K, спека подсистемы 3, раздел 11): ресайз выделения,
//! который масштабирует и свойства — толщину обводок, радиусы скругления, размер
//! текста, интерлиньяж и трекинг в пикселях, отступы абзацев, радиусы точек
//! вектора (окончания обводки и стрелки строятся от её толщины и растут вместе с
//! ней). Масштаб всегда равномерный — как в Figma: у обводки и текста одно число,
//! растянуть их по одной оси нельзя. Содержимое групп, булевых групп и фреймов
//! масштабируется вместе с ними.
//!
//! Жест холста и поле «Масштаб» панели считают правки одной функцией
//! `scale_patches`: превью и запись совпадают, запись — одна транзакция.

use std::collections::HashMap;
use std::sync::Arc;

use ferrum_doc::{LetterSpacing, LineHeight, NodeId, Props, PropsPatch, Stroke, TextSizing};
use ferrum_scene::shape::COORD_LIMIT;
use ferrum_text::Fonts;
use kurbo::{Affine, Point};

use crate::geometry::{scale_patches_deep, Base, SelectionBox};
use crate::text::fit_patch;

/// Пределы масштаба поля панели: 1 % … 10 000 %.
pub const MIN_SCALE_FACTOR: f64 = 0.01;
pub const MAX_SCALE_FACTOR: f64 = 100.0;
/// Пределы стиля текста (те же, что у секции «Текст»: размер 1–10 000 px,
/// интерлиньяж 0–10 000 px, трекинг и отступ ±10 000 px, интервал 0–10 000 px).
const TEXT_SIZE_MIN: f64 = 1.0;
const TEXT_LIMIT: f64 = 10_000.0;

/// Якорь масштаба из панели: доли рамки выделения по осям (0, 0,5 или 1), как
/// сетка 3×3 у инструмента Scale в Figma.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub fx: f64,
    pub fy: f64,
}

impl Anchor {
    pub const CENTER: Anchor = Anchor { fx: 0.5, fy: 0.5 };
    pub const TOP_LEFT: Anchor = Anchor { fx: 0.0, fy: 0.0 };

    /// Только девять точек сетки; остальное — `None` (недоверенный ввод).
    pub fn new(fx: f64, fy: f64) -> Option<Anchor> {
        let ok = |v: f64| v == 0.0 || v == 0.5 || v == 1.0;
        (ok(fx) && ok(fy)).then_some(Anchor { fx, fy })
    }
}

/// Равномерный коэффициент жеста `t`, заданного в рамке `frame` (её матрица в
/// мир), с рамкой `rect`: модуль масштаба по оси, у которой рамка не нулевая
/// (у горизонтальной линии высоты нет — масштаб по ширине).
pub fn uniform_factor(t: Affine, frame: Affine, width: f64, height: f64) -> f64 {
    if frame.determinant().abs() < 1e-24 {
        return 1.0;
    }
    let g = (frame.inverse() * t * frame).as_coeffs();
    let k = if width > 0.0 {
        g[0].abs()
    } else if height > 0.0 {
        g[3].abs()
    } else {
        1.0
    };
    if k.is_finite() {
        k
    } else {
        1.0
    }
}

/// Матрица масштаба `k` вокруг якоря рамки выделения (в мире).
pub fn anchor_transform(sbox: &SelectionBox, anchor: Anchor, k: f64) -> Affine {
    let r = sbox.rect;
    let p = Point::new(r.x0 + anchor.fx * r.width(), r.y0 + anchor.fy * r.height());
    let local = Affine::translate(p.to_vec2()) * Affine::scale(k) * Affine::translate(-p.to_vec2());
    sbox.to_world * local * sbox.to_world.inverse()
}

fn clamp_text(v: f64, lo: f64) -> f64 {
    v.clamp(lo, TEXT_LIMIT)
}

fn scale_line_height(v: LineHeight, k: f64) -> LineHeight {
    match v {
        LineHeight::Px(px) => LineHeight::Px(clamp_text(px * k, 0.0)),
        other => other,
    }
}

fn scale_letter_spacing(v: LetterSpacing, k: f64) -> LetterSpacing {
    match v {
        LetterSpacing::Px(px) => LetterSpacing::Px(clamp_text(px * k, -TEXT_LIMIT)),
        other => other,
    }
}

/// Добавляет в `patch` масштабированные на `k` свойства узла `p`: обводки,
/// радиус, стиль и разметку текста, радиусы точек вектора. Нечего менять — патч
/// не трогается.
pub fn scale_props(p: &Props, k: f64, patch: &mut PropsPatch) {
    if !(k.is_finite() && k > 0.0) || (k - 1.0).abs() < 1e-12 {
        return;
    }
    if !p.strokes.is_empty() {
        patch.strokes = Some(
            p.strokes
                .iter()
                .map(|s| Stroke {
                    width: (s.width * k).clamp(0.0, COORD_LIMIT),
                    ..s.clone()
                })
                .collect(),
        );
    }
    if p.corner_radius > 0.0 {
        patch.corner_radius = Some((p.corner_radius * k).clamp(0.0, COORD_LIMIT));
    }
    if let Some(t) = &p.text {
        let mut props = t.props.clone();
        let s = &mut props.style;
        s.size = clamp_text(s.size * k, TEXT_SIZE_MIN);
        s.line_height = scale_line_height(s.line_height, k);
        s.letter_spacing = scale_letter_spacing(s.letter_spacing, k);
        props.paragraph_spacing = clamp_text(props.paragraph_spacing * k, 0.0);
        props.paragraph_indent = clamp_text(props.paragraph_indent * k, -TEXT_LIMIT);
        patch.text = Some(props);
        // Разметка участков: только то, что задано в пикселях.
        let styled = t.content.runs.iter().any(|r| {
            r.attrs.size.is_some()
                || matches!(r.attrs.line_height, Some(LineHeight::Px(_)))
                || matches!(r.attrs.letter_spacing, Some(LetterSpacing::Px(_)))
        });
        if styled {
            let mut content = t.content.clone();
            for r in &mut content.runs {
                let a = &mut r.attrs;
                a.size = a.size.map(|v| clamp_text(v * k, TEXT_SIZE_MIN));
                a.line_height = a.line_height.map(|v| scale_line_height(v, k));
                a.letter_spacing = a.letter_spacing.map(|v| scale_letter_spacing(v, k));
            }
            patch.content = Some(content);
        }
    }
    if let Some(net) = &p.vector {
        if net.points.iter().any(|pt| pt.corner_radius > 0.0) {
            let mut net = (**net).clone();
            for pt in &mut net.points {
                pt.corner_radius = (pt.corner_radius * k).clamp(0.0, COORD_LIMIT);
            }
            patch.vector = Some(Arc::new(net));
        }
    }
}

/// Правки инструмента «Масштаб»: положение и размер (`scale_patches_deep`) и
/// масштабированные свойства каждого узла поддерева. `k` — равномерный
/// коэффициент жеста. С `fonts` у текстов «авто» размер рамки берётся по
/// раскладке с новым кеглем (как после правки текста; шрифт ещё грузится —
/// остаётся масштабированная рамка).
pub fn scale_patches(
    bases: &[Base],
    t: Affine,
    frame: Affine,
    k: f64,
    fonts: Option<&mut Fonts>,
) -> Vec<(NodeId, PropsPatch)> {
    let mut out = scale_patches_deep(bases, t, frame);
    let mut at: HashMap<NodeId, usize> = out
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    for b in bases {
        let mut extra = PropsPatch::default();
        scale_props(&b.props, k, &mut extra);
        if extra == PropsPatch::default() {
            continue;
        }
        match at.get(&b.id) {
            Some(&i) => merge(&mut out[i].1, extra),
            None => {
                at.insert(b.id, out.len());
                out.push((b.id, extra));
            }
        }
    }
    if let Some(fonts) = fonts {
        let by_id: HashMap<NodeId, &Props> = bases.iter().map(|b| (b.id, &b.props)).collect();
        for (id, patch) in &mut out {
            let Some(before) = by_id.get(id) else {
                continue;
            };
            let auto = before
                .text_props()
                .is_some_and(|t| t.sizing != TextSizing::Fixed);
            if !auto {
                continue;
            }
            let mut p = (*before).clone();
            patch.apply_to(&mut p);
            if let Ok(fit) = fit_patch(fonts, &p, &p) {
                patch.w = fit.w.or(patch.w);
                patch.h = fit.h.or(patch.h);
                patch.x = fit.x.or(patch.x);
                patch.y = fit.y.or(patch.y);
            }
        }
    }
    out
}

/// Поля `extra` поверх `patch` (только масштабируемые свойства).
fn merge(patch: &mut PropsPatch, extra: PropsPatch) {
    if extra.strokes.is_some() {
        patch.strokes = extra.strokes;
    }
    if extra.corner_radius.is_some() {
        patch.corner_radius = extra.corner_radius;
    }
    if extra.text.is_some() {
        patch.text = extra.text;
    }
    if extra.content.is_some() {
        patch.content = extra.content;
    }
    if extra.vector.is_some() {
        patch.vector = extra.vector;
    }
}
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    /// «Стрелка» (Shift+L): линия со стрелкой на конце.
    Arrow,
}

````

на:

````rust
    /// «Стрелка» (Shift+L): линия со стрелкой на конце.
    Arrow,
    /// «Масштаб» (K): ресайз выделения вместе с обводками, радиусами и текстом.
    Scale,
}

impl ToolKind {
    /// Выделяет и двигает, как «Перемещение» (наведение, измерения с Alt).
    pub fn is_select_like(self) -> bool {
        matches!(self, ToolKind::Move | ToolKind::Scale)
    }
}

````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
//! Инструмент «Перемещение» (V): выделение, перемещение, рамка выделения, ресайз
//! за восемь ручек (у линии — за концы), поворот за зоной угловых ручек.
//!
//! Состояния: `Idle → Pressed → Dragging | Marquee | Resizing | Rotating | LineEnd`.
````

на:

````rust
//! Инструмент «Перемещение» (V): выделение, перемещение, рамка выделения, ресайз
//! за восемь ручек (у линии — за концы), поворот за зоной угловых ручек. Он же —
//! инструмент «Масштаб» (K, `SelectTool::scaling`): ручки масштабируют выделение
//! равномерно вместе со свойствами (`crate::scale`), поворота нет, у линии —
//! обычная рамка.
//!
//! Состояния: `Idle → Pressed → Dragging | Marquee | Resizing | Rotating | LineEnd`.
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay, DRAG_THRESHOLD_PX};
use crate::geometry::{
    bases, normalize_deg, rect_map, resize_rect, rigid_patches, scale_patches_in, snap_angle,
    top_level, world_box, Base, Handle, SelectionBox, MIN_SIDE,
};
use crate::input::{Modifiers, PointerEvent};
use crate::ops::{duplicate, local_delta, reposition};
use crate::selection::{
    deeper, drop_frame_at, frame_label_at, locked, resolve_with, root_frame_at, selection_parents,
````

на:

````rust
use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay, DRAG_THRESHOLD_PX};
use crate::geometry::{
    bases, bases_deep, normalize_deg, rect_map, resize_rect, rigid_patches, scale_patches_in,
    snap_angle, top_level, world_box, Base, Handle, SelectionBox, MIN_SIDE,
};
use crate::input::{Modifiers, PointerEvent};
use crate::ops::{duplicate, local_delta, reposition};
use crate::scale::{scale_patches, uniform_factor};
use crate::selection::{
    deeper, drop_frame_at, frame_label_at, locked, resolve_with, root_frame_at, selection_parents,
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
    snapper: Snapper,
    transform: Affine,
}

````

на:

````rust
    snapper: Snapper,
    transform: Affine,
    /// Равномерный коэффициент жеста инструмента «Масштаб» (у ресайза — 1).
    k: f64,
}

````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
    /// То, что наведение и нажатие берут из выделения (см. `SelCache`).
    cache: Option<SelCache>,
}

````

на:

````rust
    /// То, что наведение и нажатие берут из выделения (см. `SelCache`).
    cache: Option<SelCache>,
    /// Инструмент «Масштаб» (K): ручки масштабируют выделение со свойствами.
    scale: bool,
}

````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
            last_click: None,
            cache: None,
        }
    }
````

на:

````rust
            last_click: None,
            cache: None,
            scale: false,
        }
    }

    /// Инструмент «Масштаб» (K).
    pub fn scaling() -> SelectTool {
        SelectTool {
            scale: true,
            ..SelectTool::new()
        }
    }
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
    }

    /// Единственная выделенная линия.
    fn single_line(cx: &Ctx) -> Option<NodeId> {
        match cx.selection[..] {
            [id] => cx
````

на:

````rust
    }

    /// Единственная выделенная линия (у «Масштаба» её концов нет — обычная рамка).
    fn single_line(cx: &Ctx, scale: bool) -> Option<NodeId> {
        if scale {
            return None;
        }
        match cx.selection[..] {
            [id] => cx
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
    /// Ручка или зона поворота под экранной точкой; `sbox` — рамка редактируемых
    /// выделенных из `SelCache`.
    fn box_hit(cx: &Ctx, sbox: Option<SelectionBox>, screen: Point) -> Option<(BoxHit, f64)> {
        let sbox = sbox?;
        let to_screen = |p: Point| cx.camera.world_to_screen(p);
        if Self::single_line(cx).is_some() {
            for h in [Handle::LineStart, Handle::LineEnd] {
                let (fx, fy) = h.frac();
````

на:

````rust
    /// Ручка или зона поворота под экранной точкой; `sbox` — рамка редактируемых
    /// выделенных из `SelCache`.
    fn box_hit(
        cx: &Ctx,
        sbox: Option<SelectionBox>,
        screen: Point,
        scale: bool,
    ) -> Option<(BoxHit, f64)> {
        let sbox = sbox?;
        let to_screen = |p: Point| cx.camera.world_to_screen(p);
        if Self::single_line(cx, scale).is_some() {
            for h in [Handle::LineStart, Handle::LineEnd] {
                let (fx, fy) = h.frac();
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
            sbox.rect.contains(local)
        };
        if !inside {
            for (fx, fy) in corners {
                let p = sbox.point(fx, fy);
````

на:

````rust
            sbox.rect.contains(local)
        };
        // У «Масштаба» поворота нет: зона за углом — пустое место.
        if !inside && !scale {
            for (fx, fy) in corners {
                let p = sbox.point(fx, fy);
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
        let cache = self.sel_cache(cx);
        let sbox = cache.sbox;
        if let Some((hit, angle)) = Self::box_hit(cx, sbox, e.point) {
            *cx.hover = None;
            self.cursor = match hit {
````

на:

````rust
        let cache = self.sel_cache(cx);
        let sbox = cache.sbox;
        if let Some((hit, angle)) = Self::box_hit(cx, sbox, e.point, self.scale) {
            *cx.hover = None;
            self.cursor = match hit {
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
            }
            Target::Handle(handle @ (Handle::LineStart | Handle::LineEnd)) => {
                let Some(id) = Self::single_line(cx) else {
                    self.state = State::Idle;
                    return;
````

на:

````rust
            }
            Target::Handle(handle @ (Handle::LineStart | Handle::LineEnd)) => {
                let Some(id) = Self::single_line(cx, self.scale) else {
                    self.state = State::Idle;
                    return;
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
                    return;
                };
                let mut bases = bases(cx.scene, &ids, true);
                let sbox = widen_zero_sides(&mut bases, sbox);
                State::Resizing(Resize {
````

на:

````rust
                    return;
                };
                let mut bases = if self.scale {
                    bases_deep(cx.scene, &ids)
                } else {
                    bases(cx.scene, &ids, true)
                };
                let sbox = widen_zero_sides(&mut bases, sbox);
                State::Resizing(Resize {
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
                    snapper: Snapper::new(cx.scene, &ids),
                    transform: Affine::IDENTITY,
                })
            }
````

на:

````rust
                    snapper: Snapper::new(cx.scene, &ids),
                    transform: Affine::IDENTITY,
                    k: 1.0,
                })
            }
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
        let threshold = SNAP_THRESHOLD_PX / cx.camera.zoom();
        let view = cx.view();
        self.lines.clear();
        match &mut self.state {
````

на:

````rust
        let threshold = SNAP_THRESHOLD_PX / cx.camera.zoom();
        let view = cx.view();
        let scale = self.scale;
        self.lines.clear();
        match &mut self.state {
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
                }
                let local = r.sbox.to_world.inverse() * p;
                let new = resize_rect(r.sbox.rect, r.handle, local, mods.shift, mods.alt);
                r.transform =
                    r.sbox.to_world * rect_map(r.sbox.rect, new) * r.sbox.to_world.inverse();
                let mut patches = scale_patches_in(
                    &r.bases,
````

на:

````rust
                }
                let local = r.sbox.to_world.inverse() * p;
                // «Масштаб» всегда сохраняет пропорции; Alt — от центра, как у ресайза.
                let keep = mods.shift || scale;
                let new = resize_rect(r.sbox.rect, r.handle, local, keep, mods.alt);
                r.transform =
                    r.sbox.to_world * rect_map(r.sbox.rect, new) * r.sbox.to_world.inverse();
                if scale {
                    let (w, h) = (r.sbox.rect.width(), r.sbox.rect.height());
                    r.k = uniform_factor(r.transform, r.sbox.to_world, w, h);
                    let patches = scale_patches(
                        &r.bases,
                        r.transform,
                        r.sbox.to_world,
                        r.k,
                        Some(&mut cx.fonts.borrow_mut()),
                    );
                    cx.scene.set_preview(&patches);
                    return;
                }
                let mut patches = scale_patches_in(
                    &r.bases,
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
            State::Dragging(d) => commit_drag(cx, d, cx.to_world(e.point)),
            State::Marquee(_) => {}
            State::Resizing(r) => {
                let mut patches = scale_patches_in(
````

на:

````rust
            State::Dragging(d) => commit_drag(cx, d, cx.to_world(e.point)),
            State::Marquee(_) => {}
            State::Resizing(r) if self.scale => {
                if r.transform == Affine::IDENTITY {
                    cx.scene.clear_preview();
                    return;
                }
                let patches = scale_patches(
                    &r.bases,
                    r.transform,
                    r.sbox.to_world,
                    r.k,
                    Some(&mut cx.fonts.borrow_mut()),
                );
                commit_patches(cx, patches);
            }
            State::Resizing(r) => {
                let mut patches = scale_patches_in(
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
impl Tool for SelectTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Move
    }

````

на:

````rust
impl Tool for SelectTool {
    fn kind(&self) -> ToolKind {
        if self.scale {
            ToolKind::Scale
        } else {
            ToolKind::Move
        }
    }

````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
        let world = cx.to_world(e.point);
        let shift = e.modifiers.shift;
        let cache = self.sel_cache(cx);
        let sbox = cache.sbox;
````

на:

````rust
        let world = cx.to_world(e.point);
        let shift = e.modifiers.shift;
        let scale = self.scale;
        let cache = self.sel_cache(cx);
        let sbox = cache.sbox;
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
            }
        }
        let target = if let Some((hit, _)) = Self::box_hit(cx, sbox, e.point) {
            match hit {
                BoxHit::Handle(h) => Target::Handle(h),
````

на:

````rust
            }
        }
        let target = if let Some((hit, _)) = Self::box_hit(cx, sbox, e.point, scale) {
            match hit {
                BoxHit::Handle(h) => Target::Handle(h),
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        Tool::Pen => ToolKind::Pen,
        Tool::Arrow => ToolKind::Arrow,
    }
}
````

на:

````rust
        Tool::Pen => ToolKind::Pen,
        Tool::Arrow => ToolKind::Arrow,
        Tool::Scale => ToolKind::Scale,
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        ToolKind::Pen => Tool::Pen,
        ToolKind::Arrow => Tool::Arrow,
    }
}
````

на:

````rust
        ToolKind::Pen => Tool::Pen,
        ToolKind::Arrow => Tool::Arrow,
        ToolKind::Scale => Tool::Scale,
    }
}
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    /// Стрелка: линия со стрелкой на конце.
    Arrow,
}

````

на:

````rust
    /// Стрелка: линия со стрелкой на конце.
    Arrow,
    /// Масштаб (K, подсистема 3, этап 5).
    Scale,
}

````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npx playwright test e2e/app.spec.ts e2e/settings.spec.ts --project=chromium`

Expected: PASS: fmt и clippy (нативно и wasm32) без замечаний; `ferrum-editor` и `ferrum-wasm` — 537 passed, 2 ignored (`scale` — 13 тестов, `keymap` — 15, 54 команды); wasm собран, `npm run typecheck` без ошибок, модульные — 134 passed, сборка — ок; E2E `app` и `settings` в Chromium — 33 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts apps/web/e2e/settings.spec.ts crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/geometry.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/panel.rs crates/ferrum-editor/src/scale.rs crates/ferrum-editor/src/tools/mod.rs crates/ferrum-editor/src/tools/select.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/scale.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/boolean.rs crates/ferrum-wasm/tests/panels.rs crates/ferrum-wasm/tests/protocol.rs
git commit -m "feat(editor): инструмент «Масштаб» (K) — равномерный масштаб с обводками, радиусами и текстом

Ручки инструмента K масштабируют выделение всегда пропорционально (Alt — от
центра), вместе с толщиной обводок, радиусами, кеглем, интерлиньяжем и
трекингом в px, отступами абзацев, разметкой участков и радиусами точек
вектора; содержимое групп, булевых групп и фреймов — тоже. Правка панели
PanelEdit::Scale { factor, anchor } — масштаб вокруг точки рамки (1 %…10 000 %).
Превью в сцене, запись — один шаг отмены. Команда tool.scale (K), Tool::Scale
в протоколе.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2: Интерфейс: «Масштабирование» в тулбаре и секция «Масштаб»

Спека 11: панель — поле «Масштаб» в процентах и якорь. Здесь — `Edit::Scale { factor, anchor }` протокола (вне 0,01…100 и {0, 0,5, 1}² — ошибка), секция «Масштаб» над «Положением» при K и выделении (сетка 3×3, поле в %), кнопка-список тулбара «Инструменты перемещения» («Перемещение», «Масштабирование»), иконка `scale`; E2E Scale в Chromium и Firefox.

**Files:**
- Modify: `apps/web/src/ui/{toolbar,properties,icons}.tsx`, `apps/web/src/ui/styles/properties.css`, `crates/ferrum-wasm/src/protocol.rs`, `crates/ferrum-wasm/src/core/panels.rs`
- Test: `apps/web/e2e/scale.spec.ts` (новый), `apps/web/e2e/chrome.spec.ts`, `crates/ferrum-wasm/tests/panels.rs`

**Interfaces:**
- Consumes: `PanelEdit::Scale`, `scale::Anchor`, `Tool::Scale`, команда `tool.scale`.
- Produces: `Edit::Scale { factor: f64, anchor: [f64; 2] }`; TS `scaleFactor(input)`, `SCALE_PERCENT_RANGE`, `ScaleSection`; иконка `scale`; кнопка-список «Инструменты перемещения».

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  // Фокус вошёл в тулбар (тулбар в DOM раньше панелей, холст — перед ним).
  await page.getByRole('button', { name: 'Перемещение' }).focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('button', { name: 'Фрейм', exact: true })).toBeFocused();
````

на:

````ts
  // Фокус вошёл в тулбар (тулбар в DOM раньше панелей, холст — перед ним).
  await page.getByRole('button', { name: 'Перемещение' }).focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('button', { name: 'Инструменты перемещения' })).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('button', { name: 'Фрейм', exact: true })).toBeFocused();
````

Создать `apps/web/e2e/scale.spec.ts`:

````ts
// Инструмент «Масштабирование» (K, подсистема 3, этап 5): угловая ручка
// масштабирует прямоугольник вместе с обводкой и радиусом одним шагом, поле
// «Масштаб» панели — вокруг выбранной точки рамки; кнопка тулбара и меню
// «Инструменты перемещения». Документ тест читает сводкой панели свойств.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, live, liveWhen, openApp } from './helpers';

type LiveState = Awaited<ReturnType<typeof live>>;

test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

const summary = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)));

/** Пустой файл при зуме 100 %, прямоугольник 100 × 100 от (cameraX − 100, cameraY − 50)
 * с обводкой 2 и радиусом 10, выделен. */
async function styledRect(page: Page): Promise<LiveState> {
  await openApp(page, 0);
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+Digit0');
  const l = await liveWhen(page, 'l.zoom === 1');
  await page.keyboard.press('KeyR');
  await drag(page, await appScreen(page, l, l.cameraX - 100, l.cameraY - 50), 100, 100);
  await liveWhen(page, 'l.busy === 0');
  const send = (edit: object) =>
    page.evaluate((e) => window.__ferrum!.core!.send({ type: 'editSelection', edit: e as never, phase: 'commit' } as never), edit);
  await send({ type: 'number', field: 'cornerRadius', value: 10, relative: false });
  await send({ type: 'strokes', strokes: [{ color: '#000000ff', width: 2, align: 'inside' }] });
  await expect.poll(async () => (await summary(page))?.strokes?.list?.[0]?.width).toBe(2);
  return l;
}

test('K: угловая ручка — масштаб с обводкой и радиусом одним шагом @firefox', async ({ page }) => {
  const l = await styledRect(page);
  await page.keyboard.press('KeyK');
  const toolbar = page.getByRole('toolbar', { name: 'Инструменты' });
  await expect(toolbar.getByRole('button', { name: 'Масштабирование' })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('radiogroup', { name: 'Точка масштаба' })).toBeVisible();
  const before = await live(page);
  // Правый нижний угол рамки — на (100, 100) дальше: ×2.
  await drag(page, await appScreen(page, l, l.cameraX, l.cameraY + 50), 100, 100);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  const s = await summary(page);
  expect([s.w.value, s.h.value, s.cornerRadius.value, s.strokes.list[0].width]).toEqual([200, 200, 20, 4]);
  // Инструмент остаётся (как в Figma), отмена — одним шагом.
  expect(await page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('scale');
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(async () => (await summary(page)).w.value).toBe(100);
  expect((await summary(page)).strokes.list[0].width).toBe(2);
});

test('поле «Масштаб»: 50 % вокруг центра, точка рамки стрелками', async ({ page }) => {
  await styledRect(page);
  const toolbar = page.getByRole('toolbar', { name: 'Инструменты' });
  await toolbar.getByRole('button', { name: 'Инструменты перемещения' }).click();
  await page.getByRole('menu', { name: 'Инструменты перемещения' }).getByRole('menuitemcheckbox', { name: /Масштабирование/ }).click();
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('scale');
  const s0 = await summary(page);
  // Точка — центр: из левого верхнего угла стрелками вправо и вниз.
  const grid = page.getByRole('radiogroup', { name: 'Точка масштаба' });
  await grid.getByRole('radio', { name: 'Слева сверху' }).focus();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowDown');
  await expect(grid.getByRole('radio', { name: 'По центру', exact: true })).toHaveAttribute('aria-checked', 'true');
  const field = page.getByRole('tabpanel').getByRole('textbox', { name: 'Масштаб', exact: true });
  await expect(field).toHaveValue('100%');
  const before = await live(page);
  await field.fill('50');
  await field.press('Enter');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  const s = await summary(page);
  expect([s.w.value, s.x.value - s0.x.value, s.y.value - s0.y.value, s.cornerRadius.value, s.strokes.list[0].width]).toEqual([
    50, 25, 25, 5, 1,
  ]);
  // Поле снова показывает 100 %: масштаб — относительно текущего размера.
  await expect(field).toHaveValue('100%');
});
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
    assert_eq!(core.background(), ferrum_wasm::core::CANVAS_DARK);
}
````

на:

````rust
    assert_eq!(core.background(), ferrum_wasm::core::CANVAS_DARK);
}

/// Поле «Масштаб» (инструмент K): правка одним шагом, сводка — вдвое больше;
/// коэффициент и точка вне сетки — ошибка без правки.
#[test]
fn scale_edit_is_one_step_and_rejects_bad_values() {
    let mut core = demo(100);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    core.command(Command::Select {
        ids: vec![rows[0].id.clone()],
        mode: SelectMode::Replace,
    });
    core.command(Command::SetTool { tool: Tool::Scale });
    let w0 = summary(&core.take_events()).unwrap().w.value;
    let steps = document_steps(&core);
    let scale = |factor: f64, anchor: [f64; 2], phase: EditPhase| Command::EditSelection {
        edit: Edit::Scale { factor, anchor },
        phase,
        ids: None,
        selection_version: None,
    };
    core.command(scale(1.5, [0.0, 0.0], EditPhase::Preview));
    core.command(scale(2.0, [0.0, 0.0], EditPhase::Preview));
    assert_eq!(document_steps(&core), steps, "превью — без шагов");
    core.command(scale(2.0, [0.0, 0.0], EditPhase::Commit));
    assert_eq!(document_steps(&core), steps + 1.0);
    let s = summary(&core.take_events()).expect("сводка обновилась");
    assert_eq!(s.w.value, 2.0 * w0);
    for bad in [
        scale(0.0, [0.0, 0.0], EditPhase::Commit),
        scale(101.0, [0.5, 0.5], EditPhase::Commit),
        scale(f64::NAN, [0.5, 0.5], EditPhase::Commit),
        scale(2.0, [0.3, 0.5], EditPhase::Commit),
    ] {
        core.command(bad);
    }
    assert_eq!(errors(&core.take_events()).len(), 4);
    assert_eq!(document_steps(&core), steps + 1.0);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test panels`
Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/scale.spec.ts --project=chromium`
Expected: FAIL — тесты ядра не собираются — нет варианта `Edit::Scale` протокола; E2E (стенд собран) — 2 failed: «K: угловая ручка…» — `toHaveAttribute` не нашёл `getByRole('toolbar', { name: 'Инструменты' }).getByRole('button', { name: 'Масштабирование' })`, «поле «Масштаб»…» — `locator.click: Test timeout of 120000ms exceeded` на кнопке-списке «Инструменты перемещения» (в тулбаре её нет)

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/icons.tsx` заменить:

````tsx
  line: '<path d="M22 2 2 22"/>',
  arrow: '<path d="M7 17 17 7"/><path d="M7 7h10v10"/>',
  // Булевы операции (тулбар и строки слоёв): два квадрата, закрашено то, что
  // остаётся.
````

на:

````tsx
  line: '<path d="M22 2 2 22"/>',
  arrow: '<path d="M7 17 17 7"/><path d="M7 7h10v10"/>',
  scale: '<path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M14 15H9v-5"/><path d="M16 3h5v5"/><path d="M21 3 9 15"/>',
  // Булевы операции (тулбар и строки слоёв): два квадрата, закрашено то, что
  // остаётся.
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
// память, фаза measure), а не из событий. Секции по порядку дизайн-системы:
// Фрейм / Положение → Вид → Заливка → Обводка → Экспорт. Без выделения —
// страница (фон) и экспорт всей страницы.

import { createEffect, createSignal, For, Show } from 'solid-js';
````

на:

````tsx
// память, фаза measure), а не из событий. Секции по порядку дизайн-системы:
// Фрейм / Положение → Вид → Заливка → Обводка → Экспорт. Без выделения —
// страница (фон) и экспорт всей страницы. У инструмента «Масштабирование» (K)
// сверху — секция «Масштаб»: поле в процентах и точка рамки 3×3, как в Figma.

import { createEffect, createSignal, For, Show } from 'solid-js';
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
        }
      >
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
````

на:

````tsx
        }
      >
        <Show when={ed.store.state.tool === 'scale'}>
          <ScaleSection />
        </Show>
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
}

const SCALES = [1, 2, 3] as const;

````

на:

````tsx
}

/** Точки рамки для масштаба (сетка 3×3 по строкам), подписи — для чтения с экрана. */
const ANCHORS: { at: [number, number]; name: string }[] = [
  { at: [0, 0], name: 'Слева сверху' },
  { at: [0.5, 0], name: 'Сверху по центру' },
  { at: [1, 0], name: 'Справа сверху' },
  { at: [0, 0.5], name: 'Слева по центру' },
  { at: [0.5, 0.5], name: 'По центру' },
  { at: [1, 0.5], name: 'Справа по центру' },
  { at: [0, 1], name: 'Слева снизу' },
  { at: [0.5, 1], name: 'Снизу по центру' },
  { at: [1, 1], name: 'Справа снизу' },
];

/** Масштаб в процентах: те же пределы, что у ядра (1 %…10 000 %). */
export const SCALE_PERCENT_RANGE = [1, 10_000] as const;

/** Коэффициент масштаба из ввода поля «Масштаб» (100 % — без изменений). */
export function scaleFactor(input: FieldInput): number {
  const percent = input.kind === 'set' ? input.value : 100 + input.delta;
  return Math.min(SCALE_PERCENT_RANGE[1], Math.max(SCALE_PERCENT_RANGE[0], percent)) / 100;
}

function ScaleSection() {
  const ed = useEditor();
  // Точка — левый верхний угол, как у полей W и H (Figma).
  const [anchor, setAnchor] = createSignal(0);
  let grid!: HTMLDivElement;
  const onKey = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    const at = anchor();
    const [col, row] = [at % 3, Math.floor(at / 3)];
    let next: number;
    switch (e.key) {
      case 'ArrowRight':
        next = row * 3 + ((col + 1) % 3);
        break;
      case 'ArrowLeft':
        next = row * 3 + ((col + 2) % 3);
        break;
      case 'ArrowDown':
        next = ((row + 1) % 3) * 3 + col;
        break;
      case 'ArrowUp':
        next = ((row + 2) % 3) * 3 + col;
        break;
      default:
        return;
    }
    e.preventDefault();
    setAnchor(next);
    grid.querySelectorAll<HTMLElement>('[role="radio"]')[next]?.focus();
  };
  return (
    <Section title="Масштаб">
      <div class="fe-scale">
        <div ref={grid} class="fe-anchor" role="radiogroup" aria-label="Точка масштаба" onKeyDown={onKey}>
          <For each={ANCHORS}>
            {(a, i) => (
              <button
                role="radio"
                aria-checked={anchor() === i()}
                aria-label={a.name}
                tabindex={anchor() === i() ? 0 : -1}
                class="fe-anchor__dot"
                onClick={() => setAnchor(i())}
              />
            )}
          </For>
        </div>
        <NumberField
          label={<Icon name="scale" />}
          name="Масштаб"
          value={100}
          suffix="%"
          decimals={2}
          min={SCALE_PERCENT_RANGE[0]}
          max={SCALE_PERCENT_RANGE[1]}
          percentOf={100}
          onEdit={(input, phase, target) =>
            sendEdit(ed.send, { type: 'scale', factor: scaleFactor(input), anchor: ANCHORS[anchor()].at }, phase, target)
          }
          class="fe-row__grow"
        />
      </div>
    </Section>
  );
}

const SCALES = [1, 2, 3] as const;

````

В `apps/web/src/ui/styles/properties.css` заменить:

````css
  background: var(--surface-hover);
  color: var(--ink);
}
.fe-export {
````

на:

````css
  background: var(--surface-hover);
  color: var(--ink);
}
.fe-scale {
  display: flex;
  gap: var(--space-2);
  align-items: center;
}
/* Точка масштаба: сетка 3×3 точек, выбранная — цветом выделения. */
.fe-anchor {
  display: grid;
  grid-template-columns: repeat(3, 12px);
  grid-template-rows: repeat(3, 12px);
  gap: 2px;
  padding: 3px;
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
}
.fe-anchor__dot {
  width: 12px;
  height: 12px;
  padding: 0;
  border: 0;
  border-radius: 2px;
  background: transparent;
  cursor: pointer;
  position: relative;
}
.fe-anchor__dot::after {
  content: '';
  position: absolute;
  inset: 4px;
  border-radius: 50%;
  background: var(--ink-muted);
}
.fe-anchor__dot[aria-checked='true']::after {
  inset: 2px;
  background: var(--select);
}
.fe-export {
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
// Плавающий тулбар внизу по центру холста (раскладка UI3): перемещение, фрейм с
// пресетами, фигуры со стрелкой (последняя выбранная на кнопке), перо, текст,
// булевы операции со стрелкой (как в Figma: операция выделенной булевой группы
````

на:

````tsx
// Плавающий тулбар внизу по центру холста (раскладка UI3): перемещение и масштаб
// (K) со стрелкой (как в Figma: последний выбранный на кнопке), фрейм с
// пресетами, фигуры со стрелкой (последняя выбранная на кнопке), перо, текст,
// булевы операции со стрелкой (как в Figma: операция выделенной булевой группы
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx

type Shape = 'rect' | 'ellipse' | 'line' | 'arrow' | 'image';

const SHAPES: { tool: Shape; name: string; command: string; icon: IconName }[] = [
````

на:

````tsx

type Shape = 'rect' | 'ellipse' | 'line' | 'arrow' | 'image';
type Mover = 'move' | 'scale';

const MOVERS: { tool: Mover; name: string; command: string; icon: IconName }[] = [
  { tool: 'move', name: 'Перемещение', command: 'tool.move', icon: 'move' },
  { tool: 'scale', name: 'Масштабирование', command: 'tool.scale', icon: 'scale' },
];

const SHAPES: { tool: Shape; name: string; command: string; icon: IconName }[] = [
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
  const ed = useEditor();
  const [shape, setShape] = createSignal<Shape>('rect');
  const [menu, setMenu] = createSignal<{ kind: 'frame' | 'shapes' | 'boolean'; anchor: HTMLElement } | null>(null);
  // Операция на кнопке: у выделенной булевой группы — её, иначе последняя выбранная.
  const [lastOp, setLastOp] = createSignal<BooleanOpDto>('union');
````

на:

````tsx
  const ed = useEditor();
  const [shape, setShape] = createSignal<Shape>('rect');
  const [mover, setMover] = createSignal<Mover>('move');
  type MenuKind = 'move' | 'frame' | 'shapes' | 'boolean';
  const [menu, setMenu] = createSignal<{ kind: MenuKind; anchor: HTMLElement } | null>(null);
  // Операция на кнопке: у выделенной булевой группы — её, иначе последняя выбранная.
  const [lastOp, setLastOp] = createSignal<BooleanOpDto>('union');
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
  const tool = () => ed.store.state.tool;
  const current = createMemo(() => SHAPES.find((s) => s.tool === shape())!);
  // Фигура, выбранная клавишей, становится и фигурой кнопки.
  createEffect(() => {
    const t = tool() as Tool;
    if (SHAPES.some((s) => s.tool === t)) setShape(t as Shape);
  });

````

на:

````tsx
  const tool = () => ed.store.state.tool;
  const current = createMemo(() => SHAPES.find((s) => s.tool === shape())!);
  const currentMover = createMemo(() => MOVERS.find((m) => m.tool === mover())!);
  // Фигура (и перемещение или масштаб), выбранные клавишей, становятся и на кнопке.
  createEffect(() => {
    const t = tool() as Tool;
    if (SHAPES.some((s) => s.tool === t)) setShape(t as Shape);
    if (MOVERS.some((m) => m.tool === t)) setMover(t as Mover);
  });

````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
    return out;
  };

  const shapeItems = (): MenuItem[] =>
````

на:

````tsx
    return out;
  };

  const moverItems = (): MenuItem[] =>
    MOVERS.map((m) => ({
      label: m.name,
      command: m.command,
      checked: m.tool === mover(),
      onSelect: () => {
        setMover(m.tool);
        setTool(m.tool);
      },
    }));

  const shapeItems = (): MenuItem[] =>
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
  ];

  const menuLabel = (kind: 'frame' | 'shapes' | 'boolean') =>
    kind === 'frame' ? 'Пресеты фреймов' : kind === 'shapes' ? 'Фигуры' : 'Булевы операции';
  const menuItems = (kind: 'frame' | 'shapes' | 'boolean') =>
    kind === 'frame' ? frameItems() : kind === 'shapes' ? shapeItems() : booleanItems();

  return (
    <div ref={bar} class="fe-toolbar" role="toolbar" aria-label="Инструменты">
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
````

на:

````tsx
  ];

  const menuLabel = (kind: MenuKind) =>
    kind === 'move'
      ? 'Инструменты перемещения'
      : kind === 'frame'
        ? 'Пресеты фреймов'
        : kind === 'shapes'
          ? 'Фигуры'
          : 'Булевы операции';
  const menuItems = (kind: MenuKind) =>
    kind === 'move' ? moverItems() : kind === 'frame' ? frameItems() : kind === 'shapes' ? shapeItems() : booleanItems();

  return (
    <div ref={bar} class="fe-toolbar" role="toolbar" aria-label="Инструменты">
      <div class="fe-tool-split">
        <button
          class="fe-tool"
          aria-pressed={tool() === currentMover().tool}
          aria-label={currentMover().name}
          data-tip={currentMover().name}
          data-command={currentMover().command}
          data-tip-side="top"
          onClick={() => setTool(currentMover().tool)}
        >
          <Icon name={currentMover().icon} />
        </button>
        <button
          class="fe-tool fe-tool--caret"
          aria-haspopup="menu"
          aria-expanded={menu()?.kind === 'move'}
          aria-label="Инструменты перемещения"
          data-tip="Инструменты перемещения"
          data-tip-side="top"
          onClick={(e) => setMenu({ kind: 'move', anchor: e.currentTarget })}
        >
          <Caret />
        </button>
      </div>
      <div class="fe-tool-split">
        <button
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust

use ferrum_doc::{Color, Fill, FillKind, NodeId, PageId, ScaleMode, Stroke, StrokeAlign};
use ferrum_editor::{
    browser_reserved, frame_presets, BindError, CommandId, DropTarget, KeymapError, NumField,
````

на:

````rust

use ferrum_doc::{Color, Fill, FillKind, NodeId, PageId, ScaleMode, Stroke, StrokeAlign};
use ferrum_editor::scale::{Anchor, MAX_SCALE_FACTOR, MIN_SCALE_FACTOR};
use ferrum_editor::{
    browser_reserved, frame_presets, BindError, CommandId, DropTarget, KeymapError, NumField,
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
                join: join.map(join_of),
                miter_limit: *miter_limit,
            }
        }
````

на:

````rust
                join: join.map(join_of),
                miter_limit: *miter_limit,
            }
        }
        Edit::Scale { factor, anchor } => {
            // Недоверенный ввод: вне пределов — отказ (панель зажимает сама).
            if !(factor.is_finite() && (MIN_SCALE_FACTOR..=MAX_SCALE_FACTOR).contains(factor)) {
                return Err("Масштаб — от 1 до 10 000 %".into());
            }
            let Some(anchor) = Anchor::new(anchor[0], anchor[1]) else {
                return Err("Точка масштаба — угол, середина стороны или центр рамки".into());
            };
            PanelEdit::Scale {
                factor: *factor,
                anchor,
            }
        }
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        #[serde(default)]
        miter_limit: Option<f64>,
    },
}
````

на:

````rust
        #[serde(default)]
        miter_limit: Option<f64>,
    },
    /// Поле «Масштаб» инструмента K: коэффициент (1 — 100 %, от 0,01 до 100) и
    /// точка рамки выделения `[x, y]` в долях (0, 0,5 или 1 по каждой оси).
    Scale {
        factor: f64,
        anchor: [f64; 2],
    },
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npx playwright test e2e/scale.spec.ts e2e/chrome.spec.ts e2e/app.spec.ts e2e/properties.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npx playwright test e2e/scale.spec.ts --project=firefox`

Expected: PASS: `ferrum-wasm` — 163 passed, 2 ignored (`panels` — 30 тестов); модульные — 134 passed, сборка — ок; E2E `scale`, `chrome`, `app`, `properties` в Chromium — 58 passed (в прогоне проверки «поле «Масштаб»…» прошёл со второго раза — см. «Проверка плана»); `scale` в Firefox — 1 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/chrome.spec.ts apps/web/e2e/scale.spec.ts apps/web/src/ui/icons.tsx apps/web/src/ui/properties.tsx apps/web/src/ui/styles/properties.css apps/web/src/ui/toolbar.tsx crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/panels.rs
git commit -m "feat(web): «Масштабирование» в тулбаре и секция «Масштаб» панели

Кнопка-список перемещения в тулбаре (как в Figma): «Перемещение» (V) и
«Масштабирование» (K), на кнопке — последний выбранный. С инструментом K
сверху панели «Дизайн» — секция «Масштаб»: точка рамки сеткой 3×3 (радиогруппа,
стрелки) и поле в процентах; набор и перетаскивание подписи шлют правку
Edit::Scale { factor, anchor } с фазами превью и записи. Ядро отклоняет
коэффициент вне 1 %…10 000 % и точку вне сетки. E2E scale.spec.ts.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3: Писатель SVG: фигуры, фреймы, обводки, картинки

Спека 11: собственный писатель SVG в ядре. Здесь — `ferrum_render::svg` (`Renderer::export_svg`): прямоугольники, эллипсы, векторы, булевы группы (результат), фреймы — заливка, дети в `<g clip-path>`, обводка над ними; обводки по центру, внутри и снаружи; окончания — залитые фигуры; прозрачность — `<g opacity>`; картинки — `<pattern>` с `data:` PNG один раз на файл; id из имён слоёв по флажку. Область и порядок экспорта вынесены из PNG в `export.rs` (`export_items`, `node_area`), пределы — `ExportBudget`. Текст пока — только кривыми.

**Files:**
- Create: `crates/ferrum-render/src/{svg,export}.rs`
- Modify: `Cargo.toml`, `Cargo.lock`, `crates/ferrum-render/Cargo.toml`, `crates/ferrum-render/src/{lib,text_node}.rs`
- Test: `crates/ferrum-render/tests/svg.rs` (новый); эталоны `crates/ferrum-render/tests/golden/svg_{frame_shapes,vector_boolean,image_fills}.svg`

**Interfaces:**
- Consumes: `export_pixmap_nodes` (область и порядок PNG), `ImageStore`, `SceneNode::boolean`, `vector::stroke_outline`.
- Produces: `Renderer::export_svg(&Scene, &[NodeId], &dyn ImageBytes, SvgOptions, &ExportBudget) -> Result<String, ExportError>`; `SvgOptions { outline_text: true, ids: false }`; `trait ImageBytes { fn png(&self, hash) -> Option<&[u8]> }` для `ImageStore`, `HashMap<String, Vec<u8>>`, `NoImageBytes`; `ExportBudget { max_bytes, clock, deadline_ms }` (`new(clock)`, `unlimited()`); `MAX_EXPORT_BYTES = 256 МБ`, `EXPORT_TIME_MS = 30 000`; `ExportError::{TooBig, Timeout, Write(String)}`; `export::{export_items, node_area}`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-render/tests/svg.rs`:

````rust
//! Экспорт SVG (спека подсистемы 3, раздел 11): каждый тест сначала проверяет
//! смысл файла разбором XML (элементы, атрибуты, обрезка, маски, картинки), затем
//! сравнивает его с эталоном `tests/golden/<имя>.svg` побайтно.
//! Пересоздать эталоны: `FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test svg`.

mod common;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_render::{ExportBudget, ExportError, NoImageBytes, Renderer, SvgOptions};
use roxmltree::{Document as Xml, Node};

fn solid(r: u8, g: u8, b: u8, a: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, a))])
}

fn stroke(width: f64, align: StrokeAlign) -> Option<Vec<Stroke>> {
    Some(vec![Stroke {
        color: Color::BLACK,
        width,
        align,
    }])
}

/// Сравнивает файл с эталоном `tests/golden/<name>.svg` (с `FERRUM_UPDATE_GOLDEN=1`
/// — перезаписывает).
#[track_caller]
fn golden_svg(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.svg"));
    if std::env::var("FERRUM_UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::write(&path, actual).expect("запись эталона");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "нет эталона {}: создайте его командой FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test svg",
            path.display()
        )
    });
    assert_eq!(expected, actual, "эталон {name}");
}

fn export(p: &mut Page, ids: &[NodeId], opts: SvgOptions) -> String {
    let scene = p.scene();
    Renderer::new()
        .export_svg(&scene, ids, &p.doc.images, opts, &ExportBudget::unlimited())
        .unwrap()
}

fn by_tag<'a>(doc: &'a Xml<'a>, tag: &str) -> Vec<Node<'a, 'a>> {
    doc.descendants()
        .filter(|n| n.tag_name().name() == tag)
        .collect()
}

#[test]
fn frame_shapes_strokes_and_opacity() {
    let mut p = Page::new();
    let frame = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Карточка".into()),
            fills: solid(255, 255, 255, 255),
            strokes: stroke(1.0, StrokeAlign::Inside),
            ..rect(10.0, 20.0, 200.0, 120.0)
        },
    );
    // Скруглённый прямоугольник с внутренней обводкой; выступает за фрейм — обрезка.
    p.add(
        Some(frame),
        NodeKind::Rect,
        PropsPatch {
            corner_radius: Some(8.0),
            fills: solid(255, 0, 0, 255),
            strokes: stroke(2.0, StrokeAlign::Inside),
            ..rect(150.0, 10.0, 80.0, 40.0)
        },
    );
    // Эллипс с внешней полупрозрачной обводкой и прозрачностью узла.
    p.add(
        Some(frame),
        NodeKind::Ellipse,
        PropsPatch {
            opacity: Some(0.5),
            fills: solid(0, 0, 255, 128),
            strokes: Some(vec![Stroke {
                color: Color::rgba(0, 128, 0, 255),
                width: 3.0,
                align: StrokeAlign::Outside,
            }]),
            ..rect(20.0, 20.0, 60.0, 40.0)
        },
    );
    // Повёрнутая линия: атрибут transform.
    p.add(
        Some(frame),
        NodeKind::Line,
        PropsPatch {
            rotation: Some(-30.0),
            strokes: stroke(2.0, StrokeAlign::Center),
            ..rect(20.0, 90.0, 100.0, 0.0)
        },
    );
    let svg = export(&mut p, &[frame], SvgOptions::default());
    let doc = Xml::parse(&svg).expect("SVG разбирается как XML");
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "svg");
    assert_eq!(root.attribute("width"), Some("200"));
    assert_eq!(root.attribute("height"), Some("120"));
    assert_eq!(root.attribute("viewBox"), Some("0 0 200 120"));
    // Фрейм — заливка, обрезанные дети, обводка над ними.
    let clips = by_tag(&doc, "clipPath");
    assert_eq!(clips.len(), 3, "обрезка фрейма и две внутренние обводки");
    let g = doc
        .descendants()
        .find(|n| n.attribute("clip-path").is_some() && n.tag_name().name() == "g")
        .expect("<g clip-path> фрейма");
    let kids: Vec<_> = g.children().filter(|n| n.is_element()).collect();
    assert!(kids.len() >= 4, "дети внутри обрезки");
    let rects = by_tag(&doc, "rect");
    let red = rects
        .iter()
        .find(|r| r.attribute("fill") == Some("#FF0000"))
        .expect("красный прямоугольник");
    assert_eq!(
        [red.attribute("x"), red.attribute("y"), red.attribute("rx")],
        [Some("150"), Some("10"), Some("8")],
        "сдвиг запечён в координаты"
    );
    let inside = rects
        .iter()
        .find(|r| r.attribute("stroke-width") == Some("4"))
        .expect("внутренняя обводка — двойная толщина");
    assert!(inside.attribute("clip-path").is_some());
    // Внешняя обводка — маска «всё, кроме формы».
    let masks = by_tag(&doc, "mask");
    assert_eq!(masks.len(), 1);
    let mask_kids: Vec<_> = masks[0].children().filter(|n| n.is_element()).collect();
    assert_eq!(mask_kids[0].attribute("fill"), Some("white"));
    assert_eq!(mask_kids[1].attribute("fill"), Some("black"));
    let ellipse: Vec<_> = by_tag(&doc, "ellipse")
        .into_iter()
        .filter(|n| !n.ancestors().any(|a| a.tag_name().name() == "defs"))
        .collect();
    assert_eq!(ellipse.len(), 2, "заливка и обводка (третий — в маске)");
    assert_eq!(ellipse[0].attribute("fill-opacity"), Some("0.502"));
    assert_eq!(
        ellipse[0].parent().unwrap().attribute("opacity"),
        Some("0.5")
    );
    let line = &by_tag(&doc, "line")[0];
    assert!(line.attribute("transform").unwrap().starts_with("matrix("));
    // Обводка фрейма — последней, вне обрезки детей.
    let last = root
        .descendants()
        .rfind(|n| n.is_element() && n.attribute("stroke").is_some())
        .unwrap();
    assert_eq!(last.attribute("stroke-width"), Some("2"));
    assert!(!last
        .ancestors()
        .skip(1)
        .any(|a| a.attribute("clip-path").is_some()));
    golden_svg("svg_frame_shapes", &svg);
}

#[test]
fn vectors_arrows_and_booleans_are_paths() {
    let mut p = Page::new();
    let net = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(100.0, 60.0),
        ],
        segments: vec![VectorSegment::line(0, 1), VectorSegment::line(1, 2)],
        regions: vec![],
    };
    let arrow = p.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(net)),
            strokes: stroke(2.0, StrokeAlign::Center),
            stroke_cap_end: Some(StrokeCap::ArrowLines),
            stroke_cap_start: Some(StrokeCap::Circle),
            stroke_join: Some(StrokeJoin::Round),
            ..rect(10.0, 10.0, 100.0, 60.0)
        },
    );
    let b = p.add(
        None,
        NodeKind::Boolean,
        PropsPatch {
            x: Some(150.0),
            y: Some(10.0),
            boolean_op: Some(BooleanOp::Subtract),
            fills: solid(0, 160, 0, 255),
            ..Default::default()
        },
    );
    p.add(
        Some(b),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0, 255),
            ..rect(0.0, 0.0, 60.0, 60.0)
        },
    );
    p.add(
        Some(b),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 0, 255, 255),
            ..rect(20.0, 20.0, 20.0, 20.0)
        },
    );
    let svg = export(&mut p, &[arrow, b], SvgOptions::default());
    let doc = Xml::parse(&svg).unwrap();
    let paths = by_tag(&doc, "path");
    let line = paths
        .iter()
        .find(|n| n.attribute("stroke-linejoin") == Some("round"))
        .expect("путь вектора — обводка со скруглённым стыком");
    assert!(line.attribute("d").unwrap().starts_with('M'));
    // Окончания — залитые фигуры (круг) и стрелка линиями.
    assert!(paths
        .iter()
        .any(|n| n.attribute("fill") == Some("#000000") && n.attribute("stroke").is_none()));
    // Булева группа — один путь цветом группы, дети не выводятся; дыра — evenodd или
    // второй контур.
    let green: Vec<_> = paths
        .iter()
        .filter(|n| n.attribute("fill") == Some("#00A000"))
        .collect();
    assert_eq!(green.len(), 1);
    assert_eq!(
        green[0].attribute("d").unwrap().matches('M').count(),
        2,
        "контур и дыра"
    );
    assert!(!svg.contains("#FF0000") && !svg.contains("#0000FF"));
    golden_svg("svg_vector_boolean", &svg);
}

/// PNG 2×1: красный и синий пиксели.
fn tiny_png() -> Vec<u8> {
    let mut px = ferrum_render::Pixmap::new(2, 1);
    px.data_as_u8_slice_mut()
        .copy_from_slice(&[255, 0, 0, 255, 0, 0, 255, 255]);
    px.into_png().unwrap()
}

#[test]
fn image_fills_are_data_png_once_per_file() {
    let mut p = Page::new();
    let hash = p.doc.images.add(tiny_png());
    let mut ids = vec![];
    for (i, mode) in [ScaleMode::Fill, ScaleMode::Fit, ScaleMode::Tile]
        .into_iter()
        .enumerate()
    {
        ids.push(p.add(
            None,
            NodeKind::Rect,
            PropsPatch {
                fills: Some(vec![Fill {
                    scale_mode: mode,
                    ..Fill::image(hash.clone())
                }]),
                ..rect(i as f64 * 50.0, 0.0, 40.0, 40.0)
            },
        ));
    }
    let svg = export(&mut p, &ids, SvgOptions::default());
    let doc = Xml::parse(&svg).unwrap();
    let images = by_tag(&doc, "image");
    assert_eq!(images.len(), 1, "байты картинки — один раз");
    let href = images[0]
        .attribute(("http://www.w3.org/1999/xlink", "href"))
        .unwrap();
    assert!(href.starts_with("data:image/png;base64,iVBORw0KGgo"));
    assert_eq!(by_tag(&doc, "pattern").len(), 3);
    assert_eq!(by_tag(&doc, "use").len(), 3);
    // Плитка — узор размером с картинку (2×1).
    let tile = by_tag(&doc, "pattern")[2];
    assert_eq!(
        [tile.attribute("width"), tile.attribute("height")],
        [Some("2"), Some("1")]
    );
    // Без байтов картинки заливка пропускается, а не ломает файл.
    let scene = p.scene();
    let no = Renderer::new()
        .export_svg(
            &scene,
            &ids,
            &NoImageBytes,
            SvgOptions::default(),
            &ExportBudget::unlimited(),
        )
        .unwrap();
    assert!(Xml::parse(&no).is_ok() && !no.contains("<image"));
    golden_svg("svg_image_fills", &svg);
}

#[test]
fn ids_come_from_layer_names_and_are_unique() {
    let mut p = Page::new();
    let g = p.add(
        None,
        NodeKind::Group,
        PropsPatch {
            name: Some("Иконка 1".into()),
            ..Default::default()
        },
    );
    for _ in 0..2 {
        p.add(
            Some(g),
            NodeKind::Rect,
            PropsPatch {
                name: Some("Фон & <тень>".into()),
                fills: solid(1, 2, 3, 255),
                ..rect(0.0, 0.0, 10.0, 10.0)
            },
        );
    }
    let svg = export(
        &mut p,
        &[g],
        SvgOptions {
            ids: true,
            ..SvgOptions::default()
        },
    );
    let doc = Xml::parse(&svg).unwrap();
    let ids: Vec<_> = doc
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .collect();
    assert_eq!(ids, ["Иконка_1", "Фон____тень_", "Фон____тень__2"]);
    // Без флажка группа без прозрачности — без своего <g>.
    let plain = export(&mut p, &[g], SvgOptions::default());
    assert!(!plain.contains("<g") && !plain.contains(" id="));
}

#[test]
fn errors_and_limits() {
    let mut p = Page::new();
    let id = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 0, 0, 255),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let hidden = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            visible: Some(false),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let scene = p.scene();
    let mut r = Renderer::new();
    let run = |r: &mut Renderer, ids: &[NodeId], budget: &ExportBudget| {
        r.export_svg(&scene, ids, &HashMap::new(), SvgOptions::default(), budget)
    };
    assert_eq!(
        run(&mut r, &[hidden], &ExportBudget::unlimited()),
        Err(ExportError::Empty)
    );
    assert_eq!(
        run(&mut r, &[], &ExportBudget::unlimited()),
        Err(ExportError::NotFound)
    );
    let tight = ExportBudget {
        max_bytes: 64,
        ..ExportBudget::unlimited()
    };
    assert_eq!(run(&mut r, &[id], &tight), Err(ExportError::TooBig));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-render --test svg`
Expected: FAIL — не собирается: нет крейта `roxmltree` в dev-зависимостях, `ferrum_render::{ExportBudget, NoImageBytes, SvgOptions}`, метода `Renderer::export_svg`, варианта `ExportError::TooBig` (6 ошибок)

- [ ] **Step 3: Реализация**

В `Cargo.lock` заменить:

````
 "kurbo",
 "parley",
 "skrifa",
 "thiserror 2.0.21",
````

на:

````
 "kurbo",
 "parley",
 "roxmltree",
 "skrifa",
 "thiserror 2.0.21",
````

В `Cargo.lock` заменить:

````

[[package]]
name = "rstar"
version = "0.12.2"
````

на:

````

[[package]]
name = "roxmltree"
version = "0.21.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "f1964b10c76125c36f8afe190065a4bf9a87bf324842c05701330bba9f1cacbb"
dependencies = [
 "memchr",
]

[[package]]
name = "rstar"
version = "0.12.2"
````

В `Cargo.toml` заменить:

````toml
console_error_panic_hook = "=0.1.7"
wasm-bindgen-test = "=0.3.79"
````

на:

````toml
console_error_panic_hook = "=0.1.7"
wasm-bindgen-test = "=0.3.79"
# Проверка экспорта SVG в тестах: файл разбирается как XML (только dev-зависимость).
roxmltree = "=0.21.1"
````

В `crates/ferrum-render/Cargo.toml` заменить:

````toml
skrifa.workspace = true
thiserror.workspace = true
````

на:

````toml
skrifa.workspace = true
thiserror.workspace = true

[dev-dependencies]
roxmltree.workspace = true
````

Создать `crates/ferrum-render/src/export.rs`:

````rust
//! Общее у экспорта PNG, SVG и PDF: какие узлы и какая область, пределы размера и
//! времени (спека подсистемы 3, раздел 11: экспорт недоверенного документа не
//! должен съесть память и время worker'а).

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};
use kurbo::Rect;

use crate::clock::Clock;
use crate::ExportError;

/// Наибольший файл экспорта SVG и PDF: 256 МБ (как предел байтов картинки
/// документа × 4). Больше — отказ с сообщением, а не исчерпание памяти worker'а.
pub const MAX_EXPORT_BYTES: usize = 256 * 1024 * 1024;
/// Наибольшее время экспорта SVG и PDF, мс. Дольше — отказ: worker не должен
/// стоять минутами (интерфейс при этом не замирает — экспорт идёт в worker'е).
pub const EXPORT_TIME_MS: f64 = 30_000.0;

/// Пределы одного экспорта: байты и время (часы — от вызывающего: в wasm32
/// `Instant` нет).
pub struct ExportBudget<'a> {
    pub max_bytes: usize,
    pub clock: Option<&'a dyn Clock>,
    /// Конец срока по часам `clock` (мс).
    pub deadline_ms: f64,
}

impl<'a> ExportBudget<'a> {
    /// Пределы по умолчанию: `MAX_EXPORT_BYTES` и `EXPORT_TIME_MS` от «сейчас».
    pub fn new(clock: Option<&'a dyn Clock>) -> ExportBudget<'a> {
        let now = clock.map_or(0.0, |c| c.now_ms());
        ExportBudget {
            max_bytes: MAX_EXPORT_BYTES,
            clock,
            deadline_ms: now + EXPORT_TIME_MS,
        }
    }

    /// Без пределов (тесты).
    pub fn unlimited() -> ExportBudget<'static> {
        ExportBudget {
            max_bytes: usize::MAX,
            clock: None,
            deadline_ms: f64::INFINITY,
        }
    }

    pub(crate) fn check(&self, bytes: usize) -> Result<(), ExportError> {
        if bytes > self.max_bytes {
            return Err(ExportError::TooBig);
        }
        if let Some(c) = self.clock {
            if c.now_ms() > self.deadline_ms {
                return Err(ExportError::Timeout);
            }
        }
        Ok(())
    }
}

/// Что экспортировать: узлы верхнего уровня (без потомков других выбранных) в
/// порядке отрисовки и их общая область — форма фрейма (за его краем всё
/// обрезается) или границы узла с обводкой и потомками. Неизвестные и скрытые
/// узлы пропускаются; если не осталось ни одного — ошибка.
pub(crate) fn export_items(
    scene: &Scene,
    ids: &[NodeId],
) -> Result<(Vec<(Slot, Rect)>, Rect), ExportError> {
    let mut items = Vec::with_capacity(ids.len());
    let mut known = false;
    for id in ids {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        known = true;
        let n = scene.at(slot);
        // Скрытый узел (и узел без видимого содержимого) картинки не даёт. Скрытые
        // предки здесь не проверяются: скрытый родитель скрывает всё поддерево,
        // так что `subtree_bounds` у потомка уже пусто.
        if n.subtree_bounds.is_none() {
            continue;
        }
        if let Some(own) = node_area(scene, slot) {
            items.push((slot, own));
        }
    }
    if !known {
        return Err(ExportError::NotFound);
    }
    // Узел, чей предок тоже экспортируется, уже нарисован вместе с предком:
    // второй раз он лёг бы поверх (полупрозрачный — темнее). Область считаем
    // только по оставшимся: потомок, выступающий за обрезающий фрейм, холст
    // не расширяет.
    let chosen: HashSet<_> = items.iter().map(|(s, _)| *s).collect();
    items.retain(|&(s, _)| {
        let mut cur = scene.at(s).parent;
        while let Some(p) = cur {
            if chosen.contains(&p) {
                return false;
            }
            cur = scene.at(p).parent;
        }
        true
    });
    let area = items
        .iter()
        .map(|&(_, r)| r)
        .reduce(|a, b| a.union(b))
        .ok_or(ExportError::Empty)?;
    // Порядок отрисовки: ключ — позиции от корня, без повторов.
    items.sort_by_cached_key(|(s, _)| scene.order_key(*s));
    items.dedup_by_key(|(s, _)| *s);
    Ok((items, area))
}

/// Область узла для экспорта: у фрейма — его форма, у остальных — границы с
/// обводкой и потомками.
pub(crate) fn node_area(scene: &Scene, slot: Slot) -> Option<Rect> {
    let n = scene.at(slot);
    match (n.props.kind, n.local_bounds) {
        (NodeKind::Frame, Some(b)) => Some(n.world.transform_rect_bbox(b)),
        _ => match (n.world_bounds, n.subtree_bounds) {
            (Some(a), Some(b)) => Some(a.union(b)),
            (a, b) => a.or(b),
        },
    }
}
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
mod display;
mod draw;
#[cfg(feature = "gpu")]
mod gpu;
mod image;
mod overlay;
mod text;
mod text_node;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::Scene;
use kurbo::{Affine, Rect};
````

на:

````rust
mod display;
mod draw;
mod export;
#[cfg(feature = "gpu")]
mod gpu;
mod image;
mod overlay;
mod svg;
mod text;
mod text_node;

use ferrum_doc::NodeId;
use ferrum_scene::Scene;
use kurbo::{Affine, Rect};
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    RASTER_BUILD_TIME_PER_FRAME, RASTER_CACHE_BYTES, RASTER_MAX_PX, RASTER_MAX_SCALE,
};
pub use ferrum_text::{Fonts, SharedFonts};
#[cfg(feature = "gpu")]
````

на:

````rust
    RASTER_BUILD_TIME_PER_FRAME, RASTER_CACHE_BYTES, RASTER_MAX_PX, RASTER_MAX_SCALE,
};
pub use export::{ExportBudget, EXPORT_TIME_MS, MAX_EXPORT_BYTES};
pub use ferrum_text::{Fonts, SharedFonts};
#[cfg(feature = "gpu")]
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    VECTOR_HANDLE_RADIUS, VECTOR_MAX_POINTS_DRAWN, VECTOR_POINT_RADIUS,
};
pub use text::{Label, TextEngine, ONEST};
pub use text_node::{
````

на:

````rust
    VECTOR_HANDLE_RADIUS, VECTOR_MAX_POINTS_DRAWN, VECTOR_POINT_RADIUS,
};
pub use svg::{ImageBytes, NoImageBytes, SvgOptions};
pub use text::{Label, TextEngine, ONEST};
pub use text_node::{
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    #[error("Не удалось закодировать PNG: {0}")]
    Encode(String),
}

````

на:

````rust
    #[error("Не удалось закодировать PNG: {0}")]
    Encode(String),
    #[error("Файл экспорта больше 256 МБ: экспортируйте часть документа")]
    TooBig,
    #[error("Экспорт идёт дольше 30 секунд и прерван: экспортируйте часть документа")]
    Timeout,
    #[error("Не удалось собрать файл: {0}")]
    Write(String),
}

````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
            return Err(ExportError::BadScale);
        }
        let mut items = Vec::with_capacity(ids.len());
        let mut known = false;
        for id in ids {
            let Some(slot) = scene.slot(*id) else {
                continue;
            };
            known = true;
            let n = scene.at(slot);
            // Скрытый узел (и узел без видимого содержимого) картинки не даёт. Скрытые
            // предки здесь не проверяются: скрытый родитель скрывает всё поддерево,
            // так что `subtree_bounds` у потомка уже пусто.
            if n.subtree_bounds.is_none() {
                continue;
            }
            let own = match (n.props.kind, n.local_bounds) {
                (NodeKind::Frame, Some(b)) => Some(n.world.transform_rect_bbox(b)),
                _ => match (n.world_bounds, n.subtree_bounds) {
                    (Some(a), Some(b)) => Some(a.union(b)),
                    (a, b) => a.or(b),
                },
            };
            if let Some(own) = own {
                items.push((slot, own));
            }
        }
        if !known {
            return Err(ExportError::NotFound);
        }
        // Узел, чей предок тоже экспортируется, уже нарисован вместе с предком:
        // второй раз он лёг бы поверх (полупрозрачный — темнее). Область считаем
        // только по оставшимся: потомок, выступающий за обрезающий фрейм, холст
        // не расширяет.
        let chosen: std::collections::HashSet<_> = items.iter().map(|(s, _)| *s).collect();
        items.retain(|&(s, _)| {
            let mut cur = scene.at(s).parent;
            while let Some(p) = cur {
                if chosen.contains(&p) {
                    return false;
                }
                cur = scene.at(p).parent;
            }
            true
        });
        let area = items
            .iter()
            .map(|&(_, r)| r)
            .reduce(|a, b| a.union(b))
            .ok_or(ExportError::Empty)?;
        // Порядок отрисовки: ключ — позиции от корня, без повторов.
        let mut slots: Vec<_> = items.into_iter().map(|(s, _)| s).collect();
        slots.sort_by_cached_key(|s| scene.order_key(*s));
        slots.dedup();
        let w = (area.width() * scale).round();
        let h = (area.height() * scale).round();
````

на:

````rust
            return Err(ExportError::BadScale);
        }
        let (items, area) = export::export_items(scene, ids)?;
        let slots: Vec<_> = items.into_iter().map(|(s, _)| s).collect();
        let w = (area.width() * scale).round();
        let h = (area.height() * scale).round();
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
        painter.draw(&mut backend, &ctx, &slots);
        Ok(backend.render())
    }

````

на:

````rust
        painter.draw(&mut backend, &ctx, &slots);
        Ok(backend.render())
    }

    /// Экспорт узлов в SVG (спека подсистемы 3, раздел 11): та же область и тот же
    /// порядок, что у `export_png_nodes`, в единицах документа (масштаб 1).
    /// Картинки заливок — из `images` (`data:` PNG); текст — по `opts`.
    pub fn export_svg(
        &mut self,
        scene: &Scene,
        ids: &[NodeId],
        images: &dyn ImageBytes,
        opts: SvgOptions,
        budget: &ExportBudget,
    ) -> Result<String, ExportError> {
        svg::export_svg(scene, ids, images, &self.fonts, opts, budget)
    }

````

Создать `crates/ferrum-render/src/svg.rs`:

````rust
//! Экспорт в SVG (спека подсистемы 3, раздел 11): свой писатель по сцене, как
//! «Export → SVG» в Figma. Узлы — элементы `<g>`, `<rect>`, `<ellipse>`,
//! `<line>`, `<path>`; фрейм — `<g>` с обрезкой `<clipPath>` по своей форме,
//! заливка под детьми и обводка над ними; векторы и булевы группы — пути их
//! областей; текст — контуры глифов. Заливки — цвет (`fill` и `fill-opacity`) и
//! изображение (`<pattern>` с `<image>` — `data:` PNG, байты один раз на файл).
//! Обводки — настоящие `stroke` с соединением и пределом угла; положение «внутри»
//! — двойная толщина с `<clipPath>` по форме, «снаружи» — двойная толщина с
//! `<mask>` (как у Figma); окончания стрелок, круги и ромбы — залитые фигуры.
//! Прозрачность узла — `opacity` у `<g>`.
//!
//! Координаты — мира со сдвигом к левому верхнему углу области экспорта (та же
//! область, что у PNG). Матрица узла без поворота и отражения запекается в
//! координаты (`x`, `y`, `d`), иначе — атрибут `transform` (у узлов нет масштаба,
//! поэтому толщина обводки не искажается). Обход — без рекурсии: глубина дерева
//! из файла не ограничена.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use ferrum_doc::{
    Color, Fill, FillKind, FillRule, ImageStore, NodeKind, Props, StrokeAlign, StrokeJoin,
};
use ferrum_scene::shape::{Geometry, PATH_TOLERANCE};
use ferrum_scene::vector::{self, PathShape};
use ferrum_scene::{Scene, Slot};
use ferrum_text::{LayoutRequest, SharedFonts};
use kurbo::{Affine, BezPath, PathEl, Rect, Shape as _};

use crate::draw::place_image;
use crate::export::{export_items, ExportBudget};
use crate::text_node::glyphs_path;
use crate::ExportError;

/// Настройки экспорта SVG, как в разделе «Export» Figma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SvgOptions {
    /// «Текст кривыми»: текст — контурами глифов (по умолчанию, как в Figma), без
    /// него — элементы `<text>` со шрифтом (этап 5, задача 4).
    pub outline_text: bool,
    /// «Атрибут id»: у элементов — `id` из имён слоёв (по умолчанию нет).
    pub ids: bool,
}

impl Default for SvgOptions {
    fn default() -> SvgOptions {
        SvgOptions {
            outline_text: true,
            ids: false,
        }
    }
}

/// Байты изображений документа по хэшу (PNG) — для `data:` в SVG и картинок PDF.
pub trait ImageBytes {
    fn png(&self, hash: &str) -> Option<&[u8]>;
}

impl ImageBytes for ImageStore {
    fn png(&self, hash: &str) -> Option<&[u8]> {
        self.get(hash)
    }
}

impl ImageBytes for HashMap<String, Vec<u8>> {
    fn png(&self, hash: &str) -> Option<&[u8]> {
        self.get(hash).map(Vec::as_slice)
    }
}

/// Без изображений: заливка картинкой пропускается.
pub struct NoImageBytes;

impl ImageBytes for NoImageBytes {
    fn png(&self, _: &str) -> Option<&[u8]> {
        None
    }
}

/// Размер PNG по заголовку (`IHDR`); не PNG — `None`.
pub(crate) fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let side = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4 байта"));
    let (w, h) = (side(16), side(20));
    (w > 0 && h > 0).then_some((w, h))
}

/// Число в SVG: до тысячных, без лишних нулей, без `-0`.
pub(crate) fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 {
        return "0".into();
    }
    let mut s = format!("{r:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

/// Данные пути `d` с матрицей `g`.
pub(crate) fn path_data(path: &BezPath, g: Affine) -> String {
    let mut d = String::with_capacity(path.elements().len() * 16);
    for el in path.elements() {
        match g * *el {
            PathEl::MoveTo(p) => {
                let _ = write!(d, "M{} {}", num(p.x), num(p.y));
            }
            PathEl::LineTo(p) => {
                let _ = write!(d, "L{} {}", num(p.x), num(p.y));
            }
            PathEl::QuadTo(a, p) => {
                let _ = write!(d, "Q{} {} {} {}", num(a.x), num(a.y), num(p.x), num(p.y));
            }
            PathEl::CurveTo(a, b, p) => {
                let _ = write!(
                    d,
                    "C{} {} {} {} {} {}",
                    num(a.x),
                    num(a.y),
                    num(b.x),
                    num(b.y),
                    num(p.x),
                    num(p.y)
                );
            }
            PathEl::ClosePath => d.push('Z'),
        }
    }
    d
}

/// Экранирование текста и значений атрибутов XML.
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Управляющие символы в XML 1.0 недопустимы (кроме табуляции и переводов строки).
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// Base64 (RFC 4648) без переносов — для `data:` URL.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Цвет `#rrggbb` и непрозрачность (альфа цвета × `alpha`).
fn color_attr(attr: &str, c: Color, alpha: f64) -> String {
    let a = (c.a as f64 / 255.0 * alpha).clamp(0.0, 1.0);
    let mut s = format!(" {attr}=\"#{:02X}{:02X}{:02X}\"", c.r, c.g, c.b);
    if a < 1.0 {
        let _ = write!(s, " {attr}-opacity=\"{}\"", num(a));
    }
    s
}

/// Как ставить матрицу узла: сдвиг запекается в координаты (`g`), остальное —
/// атрибутом `transform` (`attr`), координаты тогда локальные.
#[derive(Debug, Clone)]
pub(crate) struct Place {
    pub g: Affine,
    pub attr: String,
}

impl Place {
    pub fn of(m: Affine) -> Place {
        let c = m.as_coeffs();
        if c[0] == 1.0 && c[1] == 0.0 && c[2] == 0.0 && c[3] == 1.0 {
            Place {
                g: m,
                attr: String::new(),
            }
        } else {
            Place {
                g: Affine::IDENTITY,
                attr: format!(
                    " transform=\"matrix({} {} {} {} {} {})\"",
                    num(c[0]),
                    num(c[1]),
                    num(c[2]),
                    num(c[3]),
                    num(c[4]),
                    num(c[5])
                ),
            }
        }
    }
}

/// Форма элемента в координатах узла.
#[derive(Debug, Clone)]
enum Form {
    Rect { rect: Rect, radius: f64 },
    Ellipse(Rect),
    Line(f64),
    Path(BezPath, FillRule),
}

impl Form {
    /// Открывающий тег без закрытия (`<rect … `): дальше — атрибуты краски и `/>`.
    fn open(&self, p: &Place) -> String {
        let g = p.g;
        match self {
            Form::Rect { rect, radius } => {
                let r = g.transform_rect_bbox(*rect);
                let mut s = format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                    num(r.x0),
                    num(r.y0),
                    num(r.width()),
                    num(r.height())
                );
                if *radius > 0.0 {
                    let _ = write!(s, " rx=\"{}\"", num(*radius));
                }
                s.push_str(&p.attr);
                s
            }
            Form::Ellipse(rect) => {
                let r = g.transform_rect_bbox(*rect);
                let c = r.center();
                format!(
                    "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{}",
                    num(c.x),
                    num(c.y),
                    num(r.width() / 2.0),
                    num(r.height() / 2.0),
                    p.attr
                )
            }
            Form::Line(len) => {
                let a = g * kurbo::Point::ZERO;
                let b = g * kurbo::Point::new(*len, 0.0);
                format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{}",
                    num(a.x),
                    num(a.y),
                    num(b.x),
                    num(b.y),
                    p.attr
                )
            }
            Form::Path(path, rule) => {
                let mut s = format!("<path d=\"{}\"", path_data(path, g));
                if *rule == FillRule::EvenOdd {
                    s.push_str(" fill-rule=\"evenodd\" clip-rule=\"evenodd\"");
                }
                s.push_str(&p.attr);
                s
            }
        }
    }

    /// Контур формы (для внешности маски и границ).
    fn path(&self) -> BezPath {
        match self {
            Form::Rect { rect, radius } => {
                let mut out = BezPath::new();
                Geometry::Rect {
                    rect: *rect,
                    radius: *radius,
                }
                .fill_path_into(&mut out);
                out
            }
            Form::Ellipse(r) => {
                let mut out = BezPath::new();
                Geometry::Ellipse(*r).fill_path_into(&mut out);
                out
            }
            Form::Line(len) => {
                let mut out = BezPath::new();
                out.move_to((0.0, 0.0));
                out.line_to((*len, 0.0));
                out
            }
            Form::Path(p, _) => p.clone(),
        }
    }
}

/// Что положить в стек обхода.
enum Visit {
    Enter(Slot),
    /// Закрыть теги (`</g>…`).
    Close(&'static str),
    /// Обводки фрейма — над детьми, вне обрезки.
    FrameStrokes(Slot),
    /// Закрыть `<g opacity>` / `<g id>` фрейма после его обводок.
    CloseOwn(bool),
}

/// Писатель одного файла.
pub(crate) struct Writer<'a> {
    scene: &'a Scene,
    images: &'a dyn ImageBytes,
    fonts: &'a SharedFonts,
    opts: SvgOptions,
    budget: &'a ExportBudget<'a>,
    root: Affine,
    body: String,
    defs: String,
    next: u32,
    ids: HashSet<String>,
    /// Изображения, уже положенные в `<defs>`: хэш → id и размер.
    image_defs: HashMap<String, (String, u32, u32)>,
    xlink: bool,
    visited: usize,
}

impl<'a> Writer<'a> {
    fn new(
        scene: &'a Scene,
        images: &'a dyn ImageBytes,
        fonts: &'a SharedFonts,
        opts: SvgOptions,
        budget: &'a ExportBudget<'a>,
        root: Affine,
    ) -> Writer<'a> {
        Writer {
            scene,
            images,
            fonts,
            opts,
            budget,
            root,
            body: String::new(),
            defs: String::new(),
            next: 0,
            ids: HashSet::new(),
            image_defs: HashMap::new(),
            xlink: false,
            visited: 0,
        }
    }

    fn fresh(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}{}", self.next);
        self.next += 1;
        id
    }

    /// `id` из имени слоя: буквы, цифры, `-` и `_`; пробелы — `_`; уникально.
    fn layer_id(&mut self, name: &str) -> String {
        let mut base: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if base.is_empty() || !base.starts_with(|c: char| c.is_alphabetic() || c == '_') {
            base.insert(0, '_');
        }
        let mut id = base.clone();
        let mut n = 2;
        while !self.ids.insert(id.clone()) {
            id = format!("{base}_{n}");
            n += 1;
        }
        id
    }

    fn id_attr(&mut self, props: &Props) -> String {
        if self.opts.ids {
            format!(" id=\"{}\"", escape(&self.layer_id(&props.name)))
        } else {
            String::new()
        }
    }

    fn check(&self) -> Result<(), ExportError> {
        self.budget.check(self.body.len() + self.defs.len())
    }

    /// Узел и его поддерево.
    fn write_tree(&mut self, top: Slot) -> Result<(), ExportError> {
        let mut stack = vec![Visit::Enter(top)];
        while let Some(v) = stack.pop() {
            let slot = match v {
                Visit::Enter(s) => s,
                Visit::Close(tags) => {
                    self.body.push_str(tags);
                    continue;
                }
                Visit::FrameStrokes(s) => {
                    self.strokes(s)?;
                    continue;
                }
                Visit::CloseOwn(open) => {
                    if open {
                        self.body.push_str("</g>");
                    }
                    continue;
                }
            };
            self.visited += 1;
            if self.visited.is_multiple_of(256) {
                self.check()?;
            }
            let n = self.scene.at(slot);
            if n.subtree_bounds.is_none() || n.props.opacity <= 0.0 {
                continue;
            }
            let id = self.id_attr(&n.props);
            let opacity = n.props.opacity.clamp(0.0, 1.0);
            let wrap = opacity < 1.0 || !id.is_empty();
            if wrap {
                self.body.push_str("<g");
                self.body.push_str(&id);
                if opacity < 1.0 {
                    let _ = write!(self.body, " opacity=\"{}\"", num(opacity));
                }
                self.body.push('>');
            }
            let kind = n.props.kind;
            // Дети булевой группы не рисуются: её форма — результат операции.
            let has_children = !n.children.is_empty() && kind != NodeKind::Boolean;
            match kind {
                NodeKind::Group => {
                    stack.push(Visit::CloseOwn(wrap));
                    stack.extend(n.children.iter().rev().map(|&c| Visit::Enter(c)));
                }
                NodeKind::Frame if has_children => {
                    self.fills(slot)?;
                    stack.push(Visit::CloseOwn(wrap));
                    stack.push(Visit::FrameStrokes(slot));
                    if n.props.clip_content {
                        let clip = self.fresh("clip");
                        let place = self.place(slot);
                        let form = form_of(n.geometry()).expect("у фрейма есть форма");
                        let _ = write!(
                            self.defs,
                            "<clipPath id=\"{clip}\">{}/></clipPath>",
                            form.open(&place)
                        );
                        let _ = write!(self.body, "<g clip-path=\"url(#{clip})\">");
                        stack.push(Visit::Close("</g>"));
                    }
                    stack.extend(n.children.iter().rev().map(|&c| Visit::Enter(c)));
                }
                _ => {
                    self.fills(slot)?;
                    self.strokes(slot)?;
                    if wrap {
                        self.body.push_str("</g>");
                    }
                }
            }
        }
        Ok(())
    }

    fn place(&self, slot: Slot) -> Place {
        Place::of(self.root * self.scene.at(slot).world)
    }

    /// Изображение в `<defs>` (один раз на хэш): id и размер. Нет байтов или не
    /// PNG — `None`.
    fn image_def(&mut self, hash: &str) -> Option<(String, u32, u32)> {
        if let Some(d) = self.image_defs.get(hash) {
            return Some(d.clone());
        }
        let bytes = self.images.png(hash)?;
        let (w, h) = png_size(bytes)?;
        let id = self.fresh("image");
        self.xlink = true;
        let _ = write!(
            self.defs,
            "<image id=\"{id}\" width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\" xlink:href=\"data:image/png;base64,{}\"/>",
            base64(bytes)
        );
        self.image_defs.insert(hash.to_string(), (id.clone(), w, h));
        Some((id, w, h))
    }

    /// Атрибут заливки краской `f` для формы с рамкой `area` (локальные
    /// координаты узла). `None` — нечем заливать (скрыта, прозрачна, нет байтов).
    fn paint(&mut self, f: &Fill, area: Rect, place: &Place) -> Option<String> {
        if !f.visible || f.opacity <= 0.0 {
            return None;
        }
        match f.kind {
            FillKind::Solid => Some(color_attr("fill", f.color, f.opacity)),
            FillKind::Image => {
                if !(area.width() > 0.0 && area.height() > 0.0) {
                    return None;
                }
                let (img, iw, ih) = self.image_def(f.image_hash.as_deref()?)?;
                let (t, _, _) = place_image(f.scale_mode, area, iw as f64, ih as f64);
                let pat = self.fresh("pattern");
                let g = place.g;
                // Плитка — узор размером с картинку; «Заполнить» и «Вписать» — одна
                // картинка на всю рамку (у «Вписать» поля прозрачны).
                let (tile, content) = match f.scale_mode {
                    ferrum_doc::ScaleMode::Tile => (
                        g.transform_rect_bbox(Rect::new(
                            area.x0,
                            area.y0,
                            area.x0 + iw as f64,
                            area.y0 + ih as f64,
                        )),
                        Affine::IDENTITY,
                    ),
                    _ => {
                        let r = g.transform_rect_bbox(area);
                        // Содержимое узора — от его левого верхнего угла.
                        (r, Affine::translate(-r.origin().to_vec2()) * g * t)
                    }
                };
                let c = content.as_coeffs();
                let _ = write!(
                    self.defs,
                    "<pattern id=\"{pat}\" patternUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"><use xlink:href=\"#{img}\" transform=\"matrix({} {} {} {} {} {})\"/></pattern>",
                    num(tile.x0),
                    num(tile.y0),
                    num(tile.width()),
                    num(tile.height()),
                    num(c[0]),
                    num(c[1]),
                    num(c[2]),
                    num(c[3]),
                    num(c[4]),
                    num(c[5]),
                );
                let mut s = format!(" fill=\"url(#{pat})\"");
                if f.opacity < 1.0 {
                    let _ = write!(s, " fill-opacity=\"{}\"", num(f.opacity));
                }
                Some(s)
            }
        }
    }

    /// Заливки узла снизу вверх.
    fn fills(&mut self, slot: Slot) -> Result<(), ExportError> {
        let n = self.scene.at(slot);
        let p = &n.props;
        let place = self.place(slot);
        let area = Rect::new(0.0, 0.0, p.w.max(0.0), p.h.max(0.0));
        match p.kind {
            NodeKind::Group | NodeKind::Line => {}
            NodeKind::Vector | NodeKind::Boolean => {
                let Some(ps) = n.path_shape().cloned() else {
                    return Ok(());
                };
                // Рамка картинки у булевой группы — границы результата.
                let area = match p.kind {
                    NodeKind::Boolean => n.local_bounds.unwrap_or(area),
                    _ => area,
                };
                for r in &ps.regions {
                    let fills = r.fills.as_deref().unwrap_or(&p.fills);
                    for f in fills {
                        if let Some(attr) = self.paint(f, area, &place) {
                            let form = Form::Path(r.path.clone(), r.rule);
                            let _ = write!(self.body, "{}{attr}/>", form.open(&place));
                        }
                    }
                }
            }
            NodeKind::Text => self.text_fills(slot, &place)?,
            _ => {
                let Some(form) = form_of(n.geometry()) else {
                    return Ok(());
                };
                for f in &p.fills {
                    if let Some(attr) = self.paint(f, area, &place) {
                        let _ = write!(self.body, "{}{attr}/>", form.open(&place));
                    }
                }
            }
        }
        Ok(())
    }

    /// Текст контурами глифов: прогоны своей краской (участка или заливками
    /// узла), украшения — прямоугольниками.
    fn text_fills(&mut self, slot: Slot, place: &Place) -> Result<(), ExportError> {
        let n = self.scene.at(slot);
        let p = &n.props;
        let Some(t) = &p.text else { return Ok(()) };
        let fonts = self.fonts;
        let mut fonts = fonts.borrow_mut();
        let layout = fonts.layout(&LayoutRequest::for_node(t, &p.fills, p.w, p.h));
        if layout.loading {
            return Ok(());
        }
        let area = Rect::new(0.0, 0.0, p.w.max(0.0), p.h.max(0.0));
        for run in &layout.runs {
            let glyphs = glyphs_path(&mut fonts, std::iter::once(run));
            if glyphs.elements().is_empty() {
                continue;
            }
            let form = Form::Path(glyphs, FillRule::NonZero);
            let paints: Vec<&Fill> = match &run.paint {
                Some(f) => vec![f],
                None => p.fills.iter().collect(),
            };
            for f in paints {
                if let Some(attr) = self.paint(f, area, place) {
                    let _ = write!(self.body, "{}{attr}/>", form.open(place));
                }
            }
        }
        let visible = p.fills.iter().any(|f| f.visible && f.opacity > 0.0);
        for d in &layout.decorations {
            let attr = match &d.paint {
                Some(f) if f.visible && f.opacity > 0.0 => match f.kind {
                    FillKind::Solid => color_attr("fill", f.color, f.opacity),
                    FillKind::Image => color_attr("fill", d.color, f.opacity),
                },
                Some(_) => continue,
                None if !visible => continue,
                None => color_attr("fill", d.color, 1.0),
            };
            let form = Form::Rect {
                rect: d.rect,
                radius: 0.0,
            };
            let _ = write!(self.body, "{}{attr}/>", form.open(place));
        }
        Ok(())
    }

    /// Обводки узла по порядку списка.
    fn strokes(&mut self, slot: Slot) -> Result<(), ExportError> {
        let n = self.scene.at(slot);
        let p = &n.props;
        if p.kind == NodeKind::Group {
            return Ok(());
        }
        let strokes: Vec<_> = p
            .strokes
            .iter()
            .filter(|s| s.width > 0.0 && s.color.a > 0)
            .cloned()
            .collect();
        if strokes.is_empty() {
            return Ok(());
        }
        let place = self.place(slot);
        if p.kind == NodeKind::Text {
            let Some(t) = &p.text else { return Ok(()) };
            let glyphs = {
                let mut fonts = self.fonts.borrow_mut();
                let layout = fonts.layout(&LayoutRequest::for_node(t, &p.fills, p.w, p.h));
                if layout.loading {
                    return Ok(());
                }
                glyphs_path(&mut fonts, layout.runs.iter())
            };
            let form = Form::Path(glyphs, FillRule::NonZero);
            for s in &strokes {
                self.ring_stroke(&form, s, StrokeJoin::Miter, 4.0, &place);
            }
            return Ok(());
        }
        if let Some(ps) = n.path_shape().cloned() {
            for s in &strokes {
                self.path_stroke(&ps, s, p, &place);
            }
            return Ok(());
        }
        let Some(form) = form_of(n.geometry()) else {
            return Ok(());
        };
        for s in &strokes {
            self.ring_stroke(&form, s, StrokeJoin::Miter, 4.0, &place);
        }
        Ok(())
    }

    /// Обводка формы: по центру — как есть, внутри — двойной толщины с обрезкой
    /// формой, снаружи — двойной толщины с маской «всё, кроме формы».
    fn ring_stroke(
        &mut self,
        form: &Form,
        s: &ferrum_doc::Stroke,
        join: StrokeJoin,
        miter: f64,
        place: &Place,
    ) {
        let open = matches!(form, Form::Line(_));
        let align = if open { StrokeAlign::Center } else { s.align };
        let w = if align == StrokeAlign::Center {
            s.width
        } else {
            2.0 * s.width
        };
        let mut attrs = color_attr("stroke", s.color, 1.0);
        let _ = write!(attrs, " stroke-width=\"{}\"", num(w));
        match join {
            StrokeJoin::Miter => {
                if (miter - 4.0).abs() > 1e-9 {
                    let _ = write!(attrs, " stroke-miterlimit=\"{}\"", num(miter));
                }
            }
            StrokeJoin::Round => attrs.push_str(" stroke-linejoin=\"round\""),
            StrokeJoin::Bevel => attrs.push_str(" stroke-linejoin=\"bevel\""),
        }
        match align {
            StrokeAlign::Center => {
                let _ = write!(self.body, "{}{attrs}/>", form.open(place));
            }
            StrokeAlign::Inside => {
                let clip = self.fresh("clip");
                let local = Place {
                    g: place.g,
                    attr: String::new(),
                };
                let _ = write!(
                    self.defs,
                    "<clipPath id=\"{clip}\">{}/></clipPath>",
                    form.open(&local)
                );
                let _ = write!(
                    self.body,
                    "{}{attrs} clip-path=\"url(#{clip})\"/>",
                    form.open(place)
                );
            }
            StrokeAlign::Outside => {
                let mask = self.fresh("mask");
                let local = Place {
                    g: place.g,
                    attr: String::new(),
                };
                // Рамка маски — форма с запасом на толщину и острия стыков.
                let reach = w * miter.max(1.0) + 1.0;
                let b = place
                    .g
                    .transform_rect_bbox(form.path().bounding_box())
                    .inflate(reach, reach);
                let _ = write!(
                    self.defs,
                    "<mask id=\"{mask}\" maskUnits=\"userSpaceOnUse\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"><rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"white\"/>{}fill=\"black\"/></mask>",
                    form.open(&local) + " ",
                    x = num(b.x0),
                    y = num(b.y0),
                    w = num(b.width()),
                    h = num(b.height()),
                );
                let _ = write!(
                    self.body,
                    "{}{attrs} mask=\"url(#{mask})\"/>",
                    form.open(place)
                );
            }
        }
    }

    /// Обводка по контуру (вектор, результат булевой группы, линия с окончаниями,
    /// прямоугольник с соединением не «острым»): путь с соединением узла и
    /// плоскими концами, фигуры окончаний и круги в точках ветвления — заливкой
    /// цветом обводки. Полупрозрачная обводка с фигурами — в `<g opacity>`, чтобы
    /// перекрытия не темнели (как слой у рендера).
    fn path_stroke(&mut self, ps: &PathShape, s: &ferrum_doc::Stroke, p: &Props, place: &Place) {
        let style = &p.stroke_style;
        let w = vector::effective_width(ps, s);
        let extras = !ps.ends.is_empty() || !ps.junctions.is_empty();
        let alpha = s.color.a as f64 / 255.0;
        let layered = extras && alpha < 1.0;
        let color = if layered {
            Color { a: 255, ..s.color }
        } else {
            s.color
        };
        if layered {
            let _ = write!(self.body, "<g opacity=\"{}\">", num(alpha));
        }
        let stroke = ferrum_doc::Stroke {
            color,
            width: s.width,
            align: if ps.closed {
                s.align
            } else {
                StrokeAlign::Center
            },
        };
        let form = Form::Path(ps.stroke.clone(), FillRule::NonZero);
        if !ps.stroke.elements().is_empty() {
            self.ring_stroke(&form, &stroke, style.join, style.miter_limit, place);
        }
        let mut fill = BezPath::new();
        for j in &ps.junctions {
            fill.extend(kurbo::Circle::new(*j, w / 2.0).path_elements(PATH_TOLERANCE));
        }
        let mut lines = BezPath::new();
        for e in &ps.ends {
            let (f, l) = vector::cap_shapes(e, w);
            if f.area() < 0.0 {
                fill.extend(f.reverse_subpaths().iter());
            } else {
                fill.extend(f.iter());
            }
            if let Some(l) = l {
                lines.extend(l.iter());
            }
        }
        if !fill.elements().is_empty() {
            let form = Form::Path(fill, FillRule::NonZero);
            let _ = write!(
                self.body,
                "{}{}/>",
                form.open(place),
                color_attr("fill", color, 1.0)
            );
        }
        if !lines.elements().is_empty() {
            let form = Form::Path(lines, FillRule::NonZero);
            let _ = write!(
                self.body,
                "{}{} stroke-width=\"{}\"/>",
                form.open(place),
                color_attr("stroke", color, 1.0),
                num(w)
            );
        }
        if layered {
            self.body.push_str("</g>");
        }
    }

    fn finish(self, area: Rect) -> String {
        let (w, h) = (area.width(), area.height());
        let mut out = String::with_capacity(self.body.len() + self.defs.len() + 256);
        let _ = write!(
            out,
            "<svg width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" fill=\"none\" xmlns=\"http://www.w3.org/2000/svg\"",
            w = num(w),
            h = num(h)
        );
        if self.xlink {
            out.push_str(" xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
        }
        out.push_str(">\n");
        out.push_str(&self.body);
        if !self.defs.is_empty() {
            out.push_str("\n<defs>");
            out.push_str(&self.defs);
            out.push_str("</defs>");
        }
        out.push_str("\n</svg>\n");
        out
    }
}

fn form_of(g: Geometry) -> Option<Form> {
    match g {
        Geometry::Rect { rect, radius } => Some(Form::Rect { rect, radius }),
        Geometry::Ellipse(r) => Some(Form::Ellipse(r)),
        Geometry::Line(len) => Some(Form::Line(len)),
        Geometry::None | Geometry::Vector(_) => None,
    }
}

/// SVG узлов `ids` (как экспорт PNG: область — объединение областей узлов,
/// порядок — порядок отрисовки, потомки выбранных предков не повторяются).
pub(crate) fn export_svg(
    scene: &Scene,
    ids: &[ferrum_doc::NodeId],
    images: &dyn ImageBytes,
    fonts: &SharedFonts,
    opts: SvgOptions,
    budget: &ExportBudget,
) -> Result<String, ExportError> {
    let (items, area) = export_items(scene, ids)?;
    if !(area.width().is_finite() && area.height().is_finite())
        || area.width() <= 0.0 && area.height() <= 0.0
    {
        return Err(ExportError::Empty);
    }
    let root = Affine::translate(-area.origin().to_vec2());
    let mut w = Writer::new(scene, images, fonts, opts, budget, root);
    for (slot, _) in items {
        w.write_tree(slot)?;
        w.check()?;
    }
    let out = w.finish(area);
    budget.check(out.len())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short_and_never_negative_zero() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(0.5), "0.5");
        assert_eq!(num(2.0 / 3.0), "0.667");
        assert_eq!(num(-12.3456), "-12.346");
        assert_eq!(num(f64::NAN), "0");
    }

    #[test]
    fn base64_matches_rfc_4648() {
        for (raw, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(raw.as_bytes()), enc);
        }
    }

    #[test]
    fn escape_keeps_xml_well_formed() {
        assert_eq!(escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
        assert_eq!(escape("x\u{1}y\tz"), "xy\tz");
    }

    #[test]
    fn png_size_reads_the_header() {
        let mut px = vello_cpu::Pixmap::new(3, 2);
        px.data_as_u8_slice_mut().fill(255);
        assert_eq!(png_size(&px.into_png().unwrap()), Some((3, 2)));
        assert_eq!(png_size(b"GIF89a"), None);
    }
}
````

В `crates/ferrum-render/src/text_node.rs` заменить:

````rust
/// Контур глифов прогонов одним путём в координатах узла (заливка изображением
/// и обводка).
fn glyphs_path<'a>(fonts: &mut Fonts, runs: impl Iterator<Item = &'a GlyphRun>) -> BezPath {
    let mut out = BezPath::new();
    for run in runs {
````

на:

````rust
/// Контур глифов прогонов одним путём в координатах узла (заливка изображением
/// и обводка).
pub(crate) fn glyphs_path<'a>(
    fonts: &mut Fonts,
    runs: impl Iterator<Item = &'a GlyphRun>,
) -> BezPath {
    let mut out = BezPath::new();
    for run in runs {
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test svg`
Expected: созданы эталоны `crates/ferrum-render/tests/golden/{svg_frame_shapes,svg_vector_boolean,svg_image_fills}.svg` (фрейм с обрезкой, фигуры и обводки трёх положений; вектор с окончаниями и булева группа; картинки «Заполнить», «Вписать», «Замостить»), тесты — 5 passed

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-render 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `ferrum-render` — 143 passed (`svg` — 5 тестов, эталоны совпадают побайтно), остальные — как были

- [ ] **Step 6: Закоммитить**

```bash
git add Cargo.lock Cargo.toml crates/ferrum-render/Cargo.toml crates/ferrum-render/src/export.rs crates/ferrum-render/src/lib.rs crates/ferrum-render/src/svg.rs crates/ferrum-render/src/text_node.rs crates/ferrum-render/tests/svg.rs crates/ferrum-render/tests/golden/svg_frame_shapes.svg crates/ferrum-render/tests/golden/svg_image_fills.svg crates/ferrum-render/tests/golden/svg_vector_boolean.svg
git commit -m "feat(render): писатель SVG — фигуры, фреймы с обрезкой, обводки по положению, картинки

Renderer::export_svg (та же область и порядок, что у PNG): узлы — <g>, <rect>,
<ellipse>, <line>, <path>; фрейм — заливка, дети под <clipPath>, обводка над
ними; векторы и булевы группы — пути областей; текст пока контурами глифов.
Заливки — цвет и картинка (<pattern> с <image> data: PNG, байты один раз на
файл); обводка внутри — двойная толщина с <clipPath>, снаружи — с <mask>, по
контуру — stroke с соединением и пределом угла, окончания — залитые фигуры.
Прозрачность — <g opacity>, флажок «Атрибут id» — id из имён слоёв. Пределы:
256 МБ и 30 с (ExportBudget), обход без рекурсии. Область и порядок узлов
экспорта PNG вынесены в export::export_items. Эталоны tests/golden/svg_*.svg,
разбор — roxmltree =0.21.1 (dev).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 4: Текст в SVG: элементы `<text>` и «Текст кривыми»

Спека 11: `<text>` со шрифтом `font-family` и начертанием `font-weight`/`font-style`; флажок «Текст кривыми» — контуры глифов. Здесь — раскладка отдаёт текст прогона и кластеры глифов (у parley `Run::text_range()` — весь шейпинг, а не прогон кисти: кластеры берутся из `run.visual_clusters()` курсором по глифам строки), писатель — `<text>` на прогон с позициями глифов, `letter-spacing`, украшения — прямоугольниками; без шрифта — запасной Inter.

**Files:**
- Modify: `crates/ferrum-text/src/layout.rs`, `crates/ferrum-render/src/svg.rs`
- Test: `crates/ferrum-text/tests/layout.rs`, `crates/ferrum-render/tests/svg.rs`; эталон `crates/ferrum-render/tests/golden/svg_text_elements.svg`

**Interfaces:**
- Consumes: `SvgOptions::outline_text`, `collect_lines`.
- Produces: `GlyphRun::{text: String, letter_spacing: f32, glyph_text() -> &str}`, `PlacedGlyph::cluster: [u32; 2]` (байты в `GlyphRun::text`); `collect_lines(layout, text, …)`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-render/tests/svg.rs` заменить:

````rust
    assert_eq!(run(&mut r, &[id], &tight), Err(ExportError::TooBig));
}
````

на:

````rust
    assert_eq!(run(&mut r, &[id], &tight), Err(ExportError::TooBig));
}

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Реестр с загруженным вшитым Inter.
fn inter() -> ferrum_render::SharedFonts {
    let shared = ferrum_render::Fonts::shared();
    {
        let mut f = shared.borrow_mut();
        let files = [
            "inter/Inter-Variable.ttf",
            "inter/Inter-Italic-Variable.ttf",
        ];
        let faces = files
            .iter()
            .flat_map(|r| ferrum_text::read_faces(&font_file(r), r))
            .collect::<Vec<_>>();
        f.add_catalog(ferrum_text::SourceKind::Bundled, ferrum_text::group(faces));
        for r in files {
            f.load("Inter", r, font_file(r)).unwrap();
        }
    }
    shared
}

/// Текст «Привет <мир> & co» Inter Bold 24 с трекингом 1 px; « & co» — красным.
fn styled_text(p: &mut Page, family: &str) -> NodeId {
    let mut props = TextProps::default();
    props.style.size = 24.0;
    props.style.font = FontName::new(family, "Bold");
    props.style.letter_spacing = LetterSpacing::Px(1.0);
    let content = TextContent {
        runs: vec![
            TextRun {
                text: "Привет <мир>".into(),
                attrs: TextAttrs::default(),
            },
            TextRun {
                text: " & co\n".into(),
                attrs: TextAttrs {
                    fill: Some(Fill::solid(Color::rgba(255, 0, 0, 255))),
                    ..Default::default()
                },
            },
        ],
    };
    p.doc
        .transact(p.page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_props(
                id,
                &PropsPatch {
                    text: Some(props.clone()),
                    strokes: stroke(1.0, StrokeAlign::Outside),
                    ..rect(10.0, 10.0, 300.0, 40.0)
                },
            )?;
            tx.set_text_content(id, &content)?;
            Ok(vec![id])
        })
        .unwrap()[0]
}

#[test]
fn text_as_elements_or_outlines() {
    let fonts = inter();
    let mut p = Page::new();
    let id = styled_text(&mut p, "Inter");
    let scene = p.scene();
    let mut r = Renderer::new();
    r.set_fonts(fonts);
    let mut run = |outline_text: bool| {
        r.export_svg(
            &scene,
            &[id],
            &NoImageBytes,
            SvgOptions {
                outline_text,
                ids: false,
            },
            &ExportBudget::unlimited(),
        )
        .unwrap()
    };
    // Текстом: <text> со шрифтом, текст с экранированием читается обратно.
    let svg = run(false);
    let doc = Xml::parse(&svg).unwrap();
    let texts: Vec<_> = by_tag(&doc, "text")
        .into_iter()
        .filter(|n| !n.ancestors().any(|a| a.tag_name().name() == "defs"))
        .collect();
    let filled: Vec<_> = texts
        .iter()
        .filter(|t| t.attribute("fill").is_some())
        .collect();
    // Прогон — элемент: участок другого цвета и смена письменности делят строку.
    assert_eq!(filled.len(), 3, "по элементу на прогон");
    let t = filled[0];
    assert_eq!(t.attribute("font-family"), Some("Inter"));
    assert_eq!(t.attribute("font-weight"), Some("700"));
    assert_eq!(t.attribute("font-size"), Some("24"));
    assert_eq!(t.attribute("letter-spacing"), Some("1"));
    let spans: String = filled
        .iter()
        .flat_map(|t| t.children())
        .map(|s| s.text().unwrap_or_default())
        .collect();
    assert_eq!(spans, "Привет <мир> & co");
    assert_eq!(filled[1].attribute("fill"), Some("#FF0000"));
    // Внешняя обводка текста — маской с тем же <text>.
    assert_eq!(by_tag(&doc, "mask").len(), 3);
    golden_svg("svg_text_elements", &svg);
    // Кривыми: ни одного <text>, глифы — пути.
    let svg = run(true);
    let doc = Xml::parse(&svg).unwrap();
    assert!(by_tag(&doc, "text").is_empty());
    assert!(by_tag(&doc, "path")
        .iter()
        .any(|n| n.attribute("fill") == Some("#FF0000")));
}

/// Отсутствующий шрифт: текст разложен запасным Inter — так он и пишется.
#[test]
fn missing_font_is_written_with_the_fallback() {
    let fonts = inter();
    let mut p = Page::new();
    let id = styled_text(&mut p, "Нет Такого");
    let scene = p.scene();
    let mut r = Renderer::new();
    r.set_fonts(fonts);
    let svg = r
        .export_svg(
            &scene,
            &[id],
            &NoImageBytes,
            SvgOptions {
                outline_text: false,
                ids: false,
            },
            &ExportBudget::unlimited(),
        )
        .unwrap();
    assert!(svg.contains("font-family=\"Inter\"") && !svg.contains("Нет Такого"));
}
````

В `crates/ferrum-text/tests/layout.rs` заменить:

````rust
        "последний глиф перед многоточием — пробел"
    );
}
````

на:

````rust
        "последний глиф перед многоточием — пробел"
    );
}

/// Прогоны помнят свой текст (с регистром) и кластеры глифов — по ним экспорт
/// SVG пишет `<text>`, а PDF — текст для поиска и копирования; обрезка с
/// многоточием укорачивает и текст.
#[test]
fn runs_carry_their_text_and_glyph_clusters() {
    let mut fonts = fonts();
    let t = text("Привет, fi мир\nВторой", |p| {
        p.style.case = TextCase::Upper
    });
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    let all: String = l.runs.iter().map(|r| r.glyph_text()).collect();
    assert_eq!(all, "ПРИВЕТ, FI МИРВТОРОЙ");
    for r in &l.runs {
        let mut prev = 0;
        for g in &r.glyphs {
            assert!(g.cluster[0] <= g.cluster[1] && g.cluster[1] as usize <= r.text.len());
            assert!(g.cluster[0] >= prev, "кластеры по порядку");
            prev = g.cluster[0];
        }
    }
    let cut = text("ШШШШШШШШШШШШШШШШШШШШ", |p| {
        p.sizing = TextSizing::Fixed;
        p.truncate = true;
    });
    let l = lay(&mut fonts, &cut, 160.0, 30.0);
    let shown: String = l.runs.iter().map(|r| r.glyph_text()).collect();
    assert!(
        shown.ends_with('…') && shown.chars().count() < 20,
        "{shown}"
    );
    assert!(l.runs.iter().all(|r| r.letter_spacing == 0.0));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-text --test layout`
Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-render --test svg`
Expected: FAIL — раскладка не собирается — нет полей `GlyphRun::text`, `GlyphRun::letter_spacing`, `PlacedGlyph::cluster` и метода `GlyphRun::glyph_text` (9 ошибок); писатель SVG собирается, но 2 из 7 падают: `text_as_elements_or_outlines` — `по элементу на прогон` (`left: 0, right: 3`: текст пока только кривыми), `missing_font_is_written_with_the_fallback` — нет `font-family="Inter"`

- [ ] **Step 3: Реализация**

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
pub struct SvgOptions {
    /// «Текст кривыми»: текст — контурами глифов (по умолчанию, как в Figma), без
    /// него — элементы `<text>` со шрифтом (этап 5, задача 4).
    pub outline_text: bool,
    /// «Атрибут id»: у элементов — `id` из имён слоёв (по умолчанию нет).
````

на:

````rust
pub struct SvgOptions {
    /// «Текст кривыми»: текст — контурами глифов (по умолчанию, как в Figma), без
    /// него — элементы `<text>` со шрифтом (семейство, кегль, вес, курсив,
    /// трекинг): текст можно править, но вид зависит от шрифтов у зрителя.
    pub outline_text: bool,
    /// «Атрибут id»: у элементов — `id` из имён слоёв (по умолчанию нет).
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
#[derive(Debug, Clone)]
enum Form {
    Rect { rect: Rect, radius: f64 },
    Ellipse(Rect),
    Line(f64),
    Path(BezPath, FillRule),
}

impl Form {
    /// Открывающий тег без закрытия (`<rect … `): дальше — атрибуты краски и `/>`.
    fn open(&self, p: &Place) -> String {
````

на:

````rust
#[derive(Debug, Clone)]
enum Form {
    Rect {
        rect: Rect,
        radius: f64,
    },
    Ellipse(Rect),
    Line(f64),
    Path(BezPath, FillRule),
    /// Текст элементом `<text>`: атрибуты шрифта, строки `(x, y, текст)` на базовой
    /// линии и рамка (для маски внешней обводки).
    Text {
        font: String,
        spans: Vec<(f64, f64, String)>,
        bbox: Rect,
    },
}

impl Form {
    /// Элемент целиком с атрибутами `attrs` (краска, обрезка).
    fn el(&self, p: &Place, attrs: &str) -> String {
        let Form::Text { font, spans, .. } = self else {
            return format!("{}{attrs}/>", self.open(p));
        };
        let mut s = format!("<text{font}{}{attrs}>", p.attr);
        for (x, y, text) in spans {
            let at = p.g * kurbo::Point::new(*x, *y);
            let _ = write!(
                s,
                "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
                num(at.x),
                num(at.y),
                escape(text)
            );
        }
        s.push_str("</text>");
        s
    }

    /// Открывающий тег без закрытия (`<rect … `): дальше — атрибуты краски и `/>`.
    fn open(&self, p: &Place) -> String {
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
                s
            }
        }
    }
````

на:

````rust
                s
            }
            Form::Text { .. } => unreachable!("текст пишется целиком (`el`)"),
        }
    }
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
            }
            Form::Path(p, _) => p.clone(),
        }
    }
````

на:

````rust
            }
            Form::Path(p, _) => p.clone(),
            Form::Text { bbox, .. } => bbox.to_path(PATH_TOLERANCE),
        }
    }
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
                        let _ = write!(
                            self.defs,
                            "<clipPath id=\"{clip}\">{}/></clipPath>",
                            form.open(&place)
                        );
                        let _ = write!(self.body, "<g clip-path=\"url(#{clip})\">");
````

на:

````rust
                        let _ = write!(
                            self.defs,
                            "<clipPath id=\"{clip}\">{}</clipPath>",
                            form.el(&place, "")
                        );
                        let _ = write!(self.body, "<g clip-path=\"url(#{clip})\">");
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
                        if let Some(attr) = self.paint(f, area, &place) {
                            let form = Form::Path(r.path.clone(), r.rule);
                            let _ = write!(self.body, "{}{attr}/>", form.open(&place));
                        }
                    }
````

на:

````rust
                        if let Some(attr) = self.paint(f, area, &place) {
                            let form = Form::Path(r.path.clone(), r.rule);
                            self.body.push_str(&form.el(&place, &attr));
                        }
                    }
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
                for f in &p.fills {
                    if let Some(attr) = self.paint(f, area, &place) {
                        let _ = write!(self.body, "{}{attr}/>", form.open(&place));
                    }
                }
````

на:

````rust
                for f in &p.fills {
                    if let Some(attr) = self.paint(f, area, &place) {
                        self.body.push_str(&form.el(&place, &attr));
                    }
                }
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
        }
        let area = Rect::new(0.0, 0.0, p.w.max(0.0), p.h.max(0.0));
        for run in &layout.runs {
            let glyphs = glyphs_path(&mut fonts, std::iter::once(run));
            if glyphs.elements().is_empty() {
                continue;
            }
            let form = Form::Path(glyphs, FillRule::NonZero);
            let paints: Vec<&Fill> = match &run.paint {
                Some(f) => vec![f],
````

на:

````rust
        }
        let area = Rect::new(0.0, 0.0, p.w.max(0.0), p.h.max(0.0));
        let forms = text_forms(&mut fonts, &layout, self.opts.outline_text, area);
        for (run, form) in layout.runs.iter().zip(forms) {
            let Some(form) = form else { continue };
            let paints: Vec<&Fill> = match &run.paint {
                Some(f) => vec![f],
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
            for f in paints {
                if let Some(attr) = self.paint(f, area, place) {
                    let _ = write!(self.body, "{}{attr}/>", form.open(place));
                }
            }
````

на:

````rust
            for f in paints {
                if let Some(attr) = self.paint(f, area, place) {
                    self.body.push_str(&form.el(place, &attr));
                }
            }
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
                radius: 0.0,
            };
            let _ = write!(self.body, "{}{attr}/>", form.open(place));
        }
        Ok(())
````

на:

````rust
                radius: 0.0,
            };
            self.body.push_str(&form.el(place, &attr));
        }
        Ok(())
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
        if p.kind == NodeKind::Text {
            let Some(t) = &p.text else { return Ok(()) };
            let glyphs = {
                let mut fonts = self.fonts.borrow_mut();
                let layout = fonts.layout(&LayoutRequest::for_node(t, &p.fills, p.w, p.h));
````

на:

````rust
        if p.kind == NodeKind::Text {
            let Some(t) = &p.text else { return Ok(()) };
            let area = Rect::new(0.0, 0.0, p.w.max(0.0), p.h.max(0.0));
            let forms: Vec<Form> = {
                let mut fonts = self.fonts.borrow_mut();
                let layout = fonts.layout(&LayoutRequest::for_node(t, &p.fills, p.w, p.h));
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
                    return Ok(());
                }
                glyphs_path(&mut fonts, layout.runs.iter())
            };
            let form = Form::Path(glyphs, FillRule::NonZero);
            for s in &strokes {
                self.ring_stroke(&form, s, StrokeJoin::Miter, 4.0, &place);
            }
            return Ok(());
````

на:

````rust
                    return Ok(());
                }
                if self.opts.outline_text {
                    // Контуром — все глифы одним путём (как обводит рендер).
                    vec![Form::Path(
                        glyphs_path(&mut fonts, layout.runs.iter()),
                        FillRule::NonZero,
                    )]
                } else {
                    text_forms(&mut fonts, &layout, false, area)
                        .into_iter()
                        .flatten()
                        .collect()
                }
            };
            for s in &strokes {
                for form in &forms {
                    self.ring_stroke(form, s, StrokeJoin::Miter, 4.0, &place);
                }
            }
            return Ok(());
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
        match align {
            StrokeAlign::Center => {
                let _ = write!(self.body, "{}{attrs}/>", form.open(place));
            }
            StrokeAlign::Inside => {
````

на:

````rust
        match align {
            StrokeAlign::Center => {
                self.body.push_str(&form.el(place, &attrs));
            }
            StrokeAlign::Inside => {
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
                let _ = write!(
                    self.defs,
                    "<clipPath id=\"{clip}\">{}/></clipPath>",
                    form.open(&local)
                );
                let _ = write!(
                    self.body,
                    "{}{attrs} clip-path=\"url(#{clip})\"/>",
                    form.open(place)
                );
            }
            StrokeAlign::Outside => {
````

на:

````rust
                let _ = write!(
                    self.defs,
                    "<clipPath id=\"{clip}\">{}</clipPath>",
                    form.el(&local, "")
                );
                let attrs = format!("{attrs} clip-path=\"url(#{clip})\"");
                self.body.push_str(&form.el(place, &attrs));
            }
            StrokeAlign::Outside => {
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
                let _ = write!(
                    self.defs,
                    "<mask id=\"{mask}\" maskUnits=\"userSpaceOnUse\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"><rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"white\"/>{}fill=\"black\"/></mask>",
                    form.open(&local) + " ",
                    x = num(b.x0),
                    y = num(b.y0),
````

на:

````rust
                let _ = write!(
                    self.defs,
                    "<mask id=\"{mask}\" maskUnits=\"userSpaceOnUse\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"><rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"white\"/>{}</mask>",
                    form.el(&local, " fill=\"black\""),
                    x = num(b.x0),
                    y = num(b.y0),
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
                    h = num(b.height()),
                );
                let _ = write!(
                    self.body,
                    "{}{attrs} mask=\"url(#{mask})\"/>",
                    form.open(place)
                );
            }
        }
````

на:

````rust
                    h = num(b.height()),
                );
                let attrs = format!("{attrs} mask=\"url(#{mask})\"");
                self.body.push_str(&form.el(place, &attrs));
            }
        }
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
        if !fill.elements().is_empty() {
            let form = Form::Path(fill, FillRule::NonZero);
            let _ = write!(
                self.body,
                "{}{}/>",
                form.open(place),
                color_attr("fill", color, 1.0)
            );
        }
        if !lines.elements().is_empty() {
            let form = Form::Path(lines, FillRule::NonZero);
            let _ = write!(
                self.body,
                "{}{} stroke-width=\"{}\"/>",
                form.open(place),
                color_attr("stroke", color, 1.0),
                num(w)
            );
        }
        if layered {
````

на:

````rust
        if !fill.elements().is_empty() {
            let form = Form::Path(fill, FillRule::NonZero);
            self.body
                .push_str(&form.el(place, &color_attr("fill", color, 1.0)));
        }
        if !lines.elements().is_empty() {
            let form = Form::Path(lines, FillRule::NonZero);
            let attrs = format!(
                "{} stroke-width=\"{}\"",
                color_attr("stroke", color, 1.0),
                num(w)
            );
            self.body.push_str(&form.el(place, &attrs));
        }
        if layered {
````

В `crates/ferrum-render/src/svg.rs` заменить:

````rust
        out
    }
}

````

на:

````rust
        out
    }
}

/// Имя семейства для `font-family`: буквы, цифры, пробелы и дефисы — как есть,
/// иначе в кавычках.
fn font_family(name: &str) -> String {
    let plain = !name.is_empty()
        && name.starts_with(|c: char| c.is_alphabetic())
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '-');
    if plain {
        escape(name)
    } else {
        format!("'{}'", escape(&name.replace(['\\', '\''], "")))
    }
}

/// Форма каждого прогона раскладки: контуром глифов (`outline`) или элементом
/// `<text>` (шрифт, кегль, вес, курсив, ширина и трекинг прогона, строка от
/// первого глифа на базовой линии). Пустой прогон — `None`.
fn text_forms(
    fonts: &mut ferrum_text::Fonts,
    layout: &ferrum_text::TextLayout,
    outline: bool,
    area: Rect,
) -> Vec<Option<Form>> {
    layout
        .runs
        .iter()
        .map(|run| {
            if outline {
                let glyphs = glyphs_path(fonts, std::iter::once(run));
                return (!glyphs.elements().is_empty())
                    .then_some(Form::Path(glyphs, FillRule::NonZero));
            }
            let first = run.glyphs.first()?;
            let text = run.glyph_text();
            if text.trim().is_empty() {
                return None;
            }
            let f = &run.face;
            let mut font = format!(
                " xml:space=\"preserve\" style=\"white-space: pre\" font-family=\"{}\" font-size=\"{}\"",
                font_family(&f.family),
                num(run.size as f64)
            );
            if (f.weight - 400.0).abs() > 0.5 {
                let _ = write!(font, " font-weight=\"{}\"", num(f.weight as f64));
            }
            if f.italic || run.skew.is_some() {
                font.push_str(" font-style=\"italic\"");
            }
            if (f.stretch - 1.0).abs() > 1e-3 {
                let _ = write!(font, " font-stretch=\"{}%\"", num(f.stretch as f64 * 100.0));
            }
            if run.letter_spacing != 0.0 {
                let _ = write!(font, " letter-spacing=\"{}\"", num(run.letter_spacing as f64));
            }
            Some(Form::Text {
                font,
                spans: vec![(first.x as f64, first.y as f64, text.to_string())],
                bbox: area,
            })
        })
        .collect()
}

````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
    /// Ширина глифа (шаг по базовой линии) — по ней ставится многоточие.
    pub advance: f32,
}

````

на:

````rust
    /// Ширина глифа (шаг по базовой линии) — по ней ставится многоточие.
    pub advance: f32,
    /// Байты кластера глифа в `GlyphRun::text` (экспорт текста в SVG и PDF: какие
    /// символы он рисует).
    pub cluster: [u32; 2],
}

````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
    /// Синтетический наклон (курсив без своего файла), градусы.
    pub skew: Option<f32>,
    pub glyphs: Vec<PlacedGlyph>,
}

````

на:

````rust
    /// Синтетический наклон (курсив без своего файла), градусы.
    pub skew: Option<f32>,
    /// Трекинг участка в пикселях (экспорт `<text>` в SVG).
    pub letter_spacing: f32,
    /// Текст прогона (с учётом регистра), на который ссылаются кластеры глифов.
    pub text: String,
    pub glyphs: Vec<PlacedGlyph>,
}

impl GlyphRun {
    /// Символы, которые рисуют оставшиеся глифы (обрезка с многоточием снимает
    /// глифы с конца): от начала первого кластера до конца последнего.
    pub fn glyph_text(&self) -> &str {
        let lo = self.glyphs.iter().map(|g| g.cluster[0]).min().unwrap_or(0) as usize;
        let hi = self.glyphs.iter().map(|g| g.cluster[1]).max().unwrap_or(0) as usize;
        self.text.get(lo..hi.max(lo)).unwrap_or("")
    }
}

````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
            let first_caret = out.carets.len();
            let (first_run, first_deco) = (out.runs.len(), out.decorations.len());
            collect_lines(&layout, &styles, indent_x, y, &mut out);
            collect_carets(&layout, &map, indent_x, y, &mut out.carets);
            if let (Some(m), Some(line)) = (marker, out.lines.get(first_line).copied()) {
````

на:

````rust
            let first_caret = out.carets.len();
            let (first_run, first_deco) = (out.runs.len(), out.decorations.len());
            collect_lines(&layout, &text, &styles, indent_x, y, &mut out);
            collect_carets(&layout, &map, indent_x, y, &mut out.carets);
            if let (Some(m), Some(line)) = (marker, out.lines.get(first_line).copied()) {
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
        layout.break_all_lines(None);
        let mut tmp = TextLayout::default();
        collect_lines(&layout, std::slice::from_ref(&plain), 0.0, 0.0, &mut tmp);
        let Some(line) = tmp.lines.first() else {
            return;
````

на:

````rust
        layout.break_all_lines(None);
        let mut tmp = TextLayout::default();
        collect_lines(
            &layout,
            marker,
            std::slice::from_ref(&plain),
            0.0,
            0.0,
            &mut tmp,
        );
        let Some(line) = tmp.lines.first() else {
            return;
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
        layout.break_all_lines(None);
        let mut tmp = TextLayout::default();
        collect_lines(&layout, std::slice::from_ref(&style), 0.0, 0.0, &mut tmp);
        let width = layout.width() as f64;
        let mut run = tmp.runs.into_iter().next()?;
````

на:

````rust
        layout.break_all_lines(None);
        let mut tmp = TextLayout::default();
        collect_lines(
            &layout,
            text,
            std::slice::from_ref(&style),
            0.0,
            0.0,
            &mut tmp,
        );
        let width = layout.width() as f64;
        let mut run = tmp.runs.into_iter().next()?;
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
fn collect_lines(
    layout: &Layout<u32>,
    styles: &[SpanStyle],
    dx: f64,
````

на:

````rust
fn collect_lines(
    layout: &Layout<u32>,
    text: &str,
    styles: &[SpanStyle],
    dx: f64,
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
        let m = line.metrics();
        let mut line_color = None;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
````

на:

````rust
        let m = line.metrics();
        let mut line_color = None;
        // Кластеры текущего шейпинга: его текст, глифы по порядку и сколько взято.
        let mut shaped: Option<(Range<usize>, Vec<Range<usize>>, usize)> = None;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
            let color = span.color;
            line_color.get_or_insert(color);
            let glyphs: Vec<PlacedGlyph> = glyph_run
                .positioned_glyphs()
                .map(|g| PlacedGlyph {
                    id: g.id,
                    x: g.x + dx as f32,
                    y: g.y + dy as f32,
                    advance: g.advance,
                })
                .collect();
````

на:

````rust
            let color = span.color;
            line_color.get_or_insert(color);
            // Текст прогона и кластер каждого глифа. Прогоны одного шейпинга
            // (`Run`) идут на строке подряд в визуальном порядке, как и его
            // кластеры: прогон берёт следующие свои глифы из общего списка.
            let positioned: Vec<_> = glyph_run.positioned_glyphs().collect();
            let key = run.text_range();
            if shaped.as_ref().is_none_or(|(k, _, _)| *k != key) {
                let mut list: Vec<Range<usize>> = Vec::new();
                for c in run.visual_clusters() {
                    let r = c.text_range();
                    list.extend(std::iter::repeat_n(r, c.glyphs().count()));
                }
                shaped = Some((key, list, 0));
            }
            let (_, list, at) = shaped.as_mut().expect("задан выше");
            let mine: Vec<Range<usize>> = match list.get(*at..*at + positioned.len()) {
                Some(slice) => slice.to_vec(),
                // Так быть не должно: каждому глифу — весь текст шейпинга.
                None => vec![run.text_range(); positioned.len()],
            };
            *at += positioned.len();
            let lo = mine.iter().map(|r| r.start).min().unwrap_or(0);
            let hi = mine.iter().map(|r| r.end).max().unwrap_or(lo);
            let run_text = text.get(lo..hi).unwrap_or("").to_string();
            let clusters = mine
                .iter()
                .map(|r| [(r.start - lo) as u32, (r.end - lo) as u32]);
            let glyphs: Vec<PlacedGlyph> = positioned
                .into_iter()
                .zip(clusters)
                .map(|(g, cluster)| PlacedGlyph {
                    id: g.id,
                    x: g.x + dx as f32,
                    y: g.y + dy as f32,
                    advance: g.advance,
                    cluster,
                })
                .collect();
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
                paint: span.paint.clone(),
                skew: synthesis.skew(),
                glyphs,
            });
````

на:

````rust
                paint: span.paint.clone(),
                skew: synthesis.skew(),
                letter_spacing: letter_spacing(span.letter_spacing, span.size),
                text: run_text,
                glyphs,
            });
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test svg`
Expected: создан эталон `crates/ferrum-render/tests/golden/svg_text_elements.svg` (текст элементами `<text>`), прежние три не меняются, тесты — 7 passed

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-text -p ferrum-render 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `ferrum-text` и `ferrum-render` — 175 passed (`layout` — 18 тестов, `svg` — 7)

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-render/src/svg.rs crates/ferrum-render/tests/svg.rs crates/ferrum-text/src/layout.rs crates/ferrum-text/tests/layout.rs crates/ferrum-render/tests/golden/svg_text_elements.svg
git commit -m "feat(render): текст в SVG элементами <text> и флажок «Текст кривыми»

Прогон раскладки помнит свой текст (с регистром), трекинг и кластер каждого
глифа (GlyphRun::text, letter_spacing, PlacedGlyph::cluster, glyph_text —
символы оставшихся после обрезки глифов). Без «Текста кривыми» текст в SVG —
<text> на прогон: семейство, кегль, вес, курсив, ширина и трекинг, строка от
первого глифа на базовой линии с xml:space=preserve; заливки и обводки — как у
фигур (внутри — <clipPath> с тем же <text>, снаружи — <mask>). Отсутствующий
шрифт пишется запасным Inter, как на холсте. «Текст кривыми» (по умолчанию) —
контуры глифов.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5: «Копировать как SVG»

Спека 11: «Копировать как SVG» — тот же писатель в `text/plain` (и `image/svg+xml`, где разрешено). Здесь — команда `edit.copy_svg` без сочетания (как в Figma) и пункт контекстного меню холста, событие редактора `ExportSvg`, ядро пишет SVG выделения (текст кривыми, ≤ 64 МБ, часы экспорта) и шлёт `ClipboardSvg`, основной поток пишет `text/plain` и, где `ClipboardItem.supports('image/svg+xml')`, `image/svg+xml`.

**Files:**
- Modify: `crates/ferrum-editor/src/{commands,actions,editor}.rs`, `crates/ferrum-wasm/src/{core,protocol}.rs`, `crates/ferrum-wasm/src/web/mod.rs`, `apps/web/src/ui/{system-clipboard.ts,chrome.tsx}`
- Test: `crates/ferrum-editor/tests/{commands,keymap}.rs` (команд 54 → 55), `crates/ferrum-wasm/tests/{core,fonts,boolean}.rs`, `apps/web/e2e/{clipboard,app}.spec.ts`

**Interfaces:**
- Consumes: `Renderer::export_svg`, `ExportBudget`, `ImageBytes` для `ImageStore`.
- Produces: `CommandId::CopyAsSvg` (`edit.copy_svg`, без сочетания), `EditorEvent::ExportSvg(Vec<NodeId>)`, `CoreEvent::ClipboardSvg { text }`, `Core::set_export_clock(Option<Box<dyn Clock>>)`, `MAX_CLIPBOARD_SVG = 64 МБ`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 54, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 55, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
  // «Слой 1» 120 × 80, обводка внутри — размер не растёт.
  expect(size).toEqual([120, 80]);
});

````

на:

````ts
  // «Слой 1» 120 × 80, обводка внутри — размер не растёт.
  expect(size).toEqual([120, 80]);
});

test('копировать как SVG: текст и image/svg+xml из меню холста', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  const at = await appScreen(page, await live(page), 200, 40);
  await page.mouse.click(at.x, at.y, { button: 'right' });
  await page.getByRole('menu').getByRole('menuitem', { name: /Копировать как SVG/ }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Скопировано как SVG' })).toBeVisible();
  const got = await page.evaluate(async () => {
    const item = (await navigator.clipboard.read())[0];
    const text = await (await item.getType('text/plain')).text();
    const svg = item.types.includes('image/svg+xml') ? await (await item.getType('image/svg+xml')).text() : null;
    return { text, svg, types: item.types };
  });
  // «Слой 1» 120 × 80: разбирается как SVG того же размера.
  expect(got.text).toMatch(/^<svg width="120" height="80" viewBox="0 0 120 80"/);
  const size = await page.evaluate((t) => {
    const doc = new DOMParser().parseFromString(t, 'image/svg+xml');
    return doc.querySelector('parsererror') ? null : [doc.documentElement.getAttribute('width'), doc.documentElement.getAttribute('height')];
  }, got.text);
  expect(size).toEqual(['120', '80']);
  // Chromium записывает image/svg+xml, очистив разметку: размер на месте.
  if (got.svg !== null) expect(got.svg).toMatch(/<svg[^>]*width="120"[^>]*height="80"/);
});

````

В `crates/ferrum-editor/tests/commands.rs` заменить:

````rust
    assert!(events.contains(&EditorEvent::ExportPng(vec![b])));
    assert!(events.contains(&EditorEvent::PasteRequested));
}

````

на:

````rust
    assert!(events.contains(&EditorEvent::ExportPng(vec![b])));
    assert!(events.contains(&EditorEvent::PasteRequested));
    // «Копировать как SVG» — только командой (сочетания нет, как в Figma).
    h.ed.execute(CommandId::CopyAsSvg);
    assert_eq!(h.ed.take_events(), vec![EditorEvent::ExportSvg(vec![b])]);
}

````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    for c in COMMANDS {
        assert!(!c.name.is_empty() && CommandId::from_key(c.key) == Some(c.id));
        assert!(!map.bindings(c.id).is_empty(), "{:?}", c.id);
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 54);
}

````

на:

````rust
    for c in COMMANDS {
        assert!(!c.name.is_empty() && CommandId::from_key(c.key) == Some(c.id));
        // «Копировать как SVG» — без сочетания, как в Figma (меню «Правка»).
        assert_eq!(
            map.bindings(c.id).is_empty(),
            c.id == CommandId::CopyAsSvg,
            "{:?}",
            c.id
        );
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 55);
}

````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 54);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

на:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 55);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

В `crates/ferrum-wasm/tests/core.rs` заменить:

````rust
    let img = ferrum_render::decode_png(&png).expect("PNG не читается");
    assert_eq!((img.width(), img.height()), (120, 80));
}

````

на:

````rust
    let img = ferrum_render::decode_png(&png).expect("PNG не читается");
    assert_eq!((img.width(), img.height()), (120, 80));
}

/// «Копировать как SVG» (без сочетания, командой): тот же писатель, что у
/// экспорта; область — как у PNG.
#[test]
fn copy_as_svg_puts_the_selection_as_svg_text() {
    let mut core = demo(100);
    frame(&mut core);
    let at = screen(200.0, 40.0);
    core.handle(Input::PointerDown(pointer(at, 0.0)));
    core.handle(Input::PointerUp(pointer(at, 10.0)));
    core.take_events();
    core.command(Command::Execute {
        command: "edit.copy_svg".into(),
    });
    let text = core.take_events().into_iter().find_map(|e| match e {
        CoreEvent::ClipboardSvg { text } => Some(text),
        _ => None,
    });
    let text = text.expect("нет SVG");
    assert!(
        text.starts_with("<svg width=\"120\" height=\"80\""),
        "{text}"
    );
    assert!(text.trim_end().ends_with("</svg>"));
}

````

В `crates/ferrum-wasm/tests/fonts.rs` заменить:

````rust
        .iter()
        .any(|e| matches!(e, CoreEvent::ClipboardPng { .. })));
    // Превью: прежнее остаётся.
    c.command(Command::Snapshot { thumbnail: true });
````

на:

````rust
        .iter()
        .any(|e| matches!(e, CoreEvent::ClipboardPng { .. })));
    c.command(Command::Execute {
        command: "edit.copy_svg".into(),
    });
    let ev = c.take_events();
    assert_eq!(errors_of(&ev), [STILL_LOADING]);
    assert!(!ev
        .iter()
        .any(|e| matches!(e, CoreEvent::ClipboardSvg { .. })));
    // Превью: прежнее остаётся.
    c.command(Command::Snapshot { thumbnail: true });
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test core`
Expected: FAIL — не собирается: нет варианта `CoreEvent::ClipboardSvg`

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
    { separator: true },
    item('Копировать как PNG', 'edit.copy_png'),
    item('Выделить всё', 'edit.select_all', false),
    item('Показать всё', 'view.fit_all', false),
````

на:

````tsx
    { separator: true },
    item('Копировать как PNG', 'edit.copy_png'),
    item('Копировать как SVG', 'edit.copy_svg'),
    item('Выделить всё', 'edit.select_all', false),
    item('Показать всё', 'view.fit_all', false),
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
// Связь ядра с системным буфером обмена и выбором файлов: события `copy`, `cut`,
// `paste` документа, события ядра `clipboard`, `clipboardPng`, `pasteRequested`,
// `requestImage`. Разбор форматов — в clipboard.ts, байты и текст с меткой
// разбирает worker: основной поток только переносит их.
````

на:

````ts
// Связь ядра с системным буфером обмена и выбором файлов: события `copy`, `cut`,
// `paste` документа, события ядра `clipboard`, `clipboardPng`, `clipboardSvg`, `pasteRequested`,
// `requestImage`. Разбор форматов — в clipboard.ts, байты и текст с меткой
// разбирает worker: основной поток только переносит их.
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
    ? 'Не удалось записать в буфер обмена: браузер не дал доступа. Отмена вернёт вырезанные объекты'
    : 'Не удалось записать в буфер обмена: браузер не дал доступа. Повторите копирование';
}

````

на:

````ts
    ? 'Не удалось записать в буфер обмена: браузер не дал доступа. Отмена вернёт вырезанные объекты'
    : 'Не удалось записать в буфер обмена: браузер не дал доступа. Повторите копирование';
}

/** Умеет ли асинхронный буфер записывать `image/svg+xml` (Chromium 124+). */
function svgSupported(): boolean {
  const supports = (ClipboardItem as unknown as { supports?: (type: string) => boolean }).supports;
  try {
    return typeof supports === 'function' && supports('image/svg+xml');
  } catch {
    return false;
  }
}

````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
      .then(() => ed.store.toast('Скопировано как PNG'))
      .catch(() => toast('Не удалось скопировать картинку: браузер не дал доступа к буферу обмена'));
  };

````

на:

````ts
      .then(() => ed.store.toast('Скопировано как PNG'))
      .catch(() => toast('Не удалось скопировать картинку: браузер не дал доступа к буферу обмена'));
  };

  /** «Копировать как SVG»: `text/plain` всегда, `image/svg+xml` — где браузер умеет
   * его записать (`ClipboardItem.supports`); отказ асинхронного API — обычный текст
   * через `execCommand`. */
  const writeSvg = (text: string) => {
    const items: Record<string, Blob> = { 'text/plain': new Blob([text], { type: 'text/plain' }) };
    if (svgSupported()) items['image/svg+xml'] = new Blob([text], { type: 'image/svg+xml' });
    navigator.clipboard
      .write([new ClipboardItem(items)])
      .then(() => ed.store.toast('Скопировано как SVG'))
      .catch(() => writeText(text, false));
  };

````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
    if (e.type === 'clipboard') write(e);
    else if (e.type === 'clipboardPng') writePng(e.bytes);
    else if (e.type === 'pasteRequested') void readAsync(false).then(deliver);
    else if (e.type === 'requestImage') {
````

на:

````ts
    if (e.type === 'clipboard') write(e);
    else if (e.type === 'clipboardPng') writePng(e.bytes);
    else if (e.type === 'clipboardSvg') writeSvg(e.text);
    else if (e.type === 'pasteRequested') void readAsync(false).then(deliver);
    else if (e.type === 'requestImage') {
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
                if !ids.is_empty() {
                    self.out.events.push(EditorEvent::ExportPng(ids));
                }
            }
````

на:

````rust
                if !ids.is_empty() {
                    self.out.events.push(EditorEvent::ExportPng(ids));
                }
            }
            C::CopyAsSvg => {
                let ids = self.ordered_selection();
                if !ids.is_empty() {
                    self.out.events.push(EditorEvent::ExportSvg(ids));
                }
            }
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    Paste,
    CopyAsPng,
    Duplicate,
    Delete,
````

на:

````rust
    Paste,
    CopyAsPng,
    /// «Копировать как SVG» (подсистема 3, этап 5): без сочетания, как в Figma.
    CopyAsSvg,
    Duplicate,
    Delete,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        &["Mod+Shift+KeyC"],
    ),
    cmd(C::Duplicate, "edit.duplicate", "Дублировать", &["Mod+KeyD"]),
    cmd(
````

на:

````rust
        &["Mod+Shift+KeyC"],
    ),
    cmd(C::CopyAsSvg, "edit.copy_svg", "Копировать как SVG", &[]),
    cmd(C::Duplicate, "edit.duplicate", "Дублировать", &["Mod+KeyD"]),
    cmd(
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Скопировать как PNG: интерфейс экспортирует узлы и кладёт картинку в буфер.
    ExportPng(Vec<NodeId>),
    /// Выбрать файл изображения и вызвать `place_image`.
    RequestImage,
````

на:

````rust
    /// Скопировать как PNG: интерфейс экспортирует узлы и кладёт картинку в буфер.
    ExportPng(Vec<NodeId>),
    /// Скопировать как SVG: тот же писатель, что у экспорта SVG, — в буфер текстом.
    ExportSvg(Vec<NodeId>),
    /// Выбрать файл изображения и вызвать `place_image`.
    RequestImage,
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
use ferrum_editor::geometry::SelectionBox;
use ferrum_editor::{CommandId, Cursor, Editor, EditorEvent, Input, Platform, ToolKind};
use ferrum_render::{decode_png, FrameStats, OverlayTheme, RenderBackend, Renderer, Rgba};
use ferrum_scene::Scene;
use ferrum_text::{Fonts, SharedFonts};
````

на:

````rust
use ferrum_editor::geometry::SelectionBox;
use ferrum_editor::{CommandId, Cursor, Editor, EditorEvent, Input, Platform, ToolKind};
use ferrum_render::{
    decode_png, Clock, ExportBudget, ExportError, FrameStats, OverlayTheme, RenderBackend,
    Renderer, Rgba, SvgOptions,
};
use ferrum_scene::Scene;
use ferrum_text::{Fonts, SharedFonts};
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
/// Масштаб «Скопировать как PNG».
pub const COPY_PNG_SCALE: f64 = 1.0;

/// Значения выделения для живого состояния и ключ, при котором они посчитаны.
````

на:

````rust
/// Масштаб «Скопировать как PNG».
pub const COPY_PNG_SCALE: f64 = 1.0;
/// Наибольший SVG для буфера обмена (как предел вставки).
pub const MAX_CLIPBOARD_SVG: usize = 64 << 20;

/// Значения выделения для живого состояния и ключ, при котором они посчитаны.
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    /// Время ядра (мс) из `set_time`: начало ожидания шрифтов.
    now_ms: f64,
}

````

на:

````rust
    /// Время ядра (мс) из `set_time`: начало ожидания шрифтов.
    now_ms: f64,
    /// Часы пределов экспорта SVG и PDF (в worker'е — `performance.now()`).
    export_clock: Option<Box<dyn Clock>>,
}

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            text_state: text::TextState::default(),
            now_ms: 0.0,
        };
        // Таблица команд и пресеты нужны интерфейсу сразу, до документа.
````

на:

````rust
            text_state: text::TextState::default(),
            now_ms: 0.0,
            export_clock: ferrum_render::default_clock(),
        };
        // Таблица команд и пресеты нужны интерфейсу сразу, до документа.
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                EditorEvent::PasteRequested => self.events.push(CoreEvent::PasteRequested),
                EditorEvent::ExportPng(ids) => self.copy_png(&ids),
                EditorEvent::RequestImage => self.events.push(CoreEvent::RequestImage),
                EditorEvent::ToggleUi => self.events.push(CoreEvent::ToggleUi),
````

на:

````rust
                EditorEvent::PasteRequested => self.events.push(CoreEvent::PasteRequested),
                EditorEvent::ExportPng(ids) => self.copy_png(&ids),
                EditorEvent::ExportSvg(ids) => self.copy_svg(&ids),
                EditorEvent::RequestImage => self.events.push(CoreEvent::RequestImage),
                EditorEvent::ToggleUi => self.events.push(CoreEvent::ToggleUi),
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
    }

    /// Часы пределов экспорта (в wasm — `performance.now()`; без них — только
    /// предел байтов).
    pub fn set_export_clock(&mut self, clock: Option<Box<dyn Clock>>) {
        self.export_clock = clock;
    }

    /// «Копировать как SVG»: тот же писатель, что у экспорта (текст — кривыми,
    /// как в Figma), не больше `MAX_CLIPBOARD_SVG` байт.
    fn copy_svg(&mut self, ids: &[NodeId]) {
        if fonts::fonts_loading(self.editor.scene(), &self.fonts) {
            self.error(fonts::FONTS_LOADING);
            return;
        }
        let mut budget = ExportBudget::new(self.export_clock.as_deref());
        budget.max_bytes = MAX_CLIPBOARD_SVG;
        let result = self.renderer.export_svg(
            self.editor.scene(),
            ids,
            &self.editor.document().images,
            SvgOptions::default(),
            &budget,
        );
        match result {
            Ok(text) => self.events.push(CoreEvent::ClipboardSvg { text }),
            Err(ExportError::TooBig) => {
                self.error("Не удалось скопировать: SVG больше 64 МБ — экспортируйте его в файл")
            }
            Err(e) => self.error(format!("Не удалось скопировать: {e}")),
        }
    }

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        bytes: ByteBuf,
    },
    /// Пора прочитать системный буфер и прислать `Paste`.
    PasteRequested,
````

на:

````rust
        bytes: ByteBuf,
    },
    /// SVG выделения для буфера обмена («Копировать как SVG»): `text/plain` и, где
    /// браузер умеет, `image/svg+xml`.
    ClipboardSvg {
        text: String,
    },
    /// Пора прочитать системный буфер и прислать `Paste`.
    PasteRequested,
````

В `crates/ferrum-wasm/src/web/mod.rs` заменить:

````rust
        core.renderer_mut()
            .set_clock(PerformanceClock::new().map(|c| Box::new(c) as _));
        let forced = force.as_deref().and_then(Backend::parse);
        let order: Vec<Backend> = match forced {
````

на:

````rust
        core.renderer_mut()
            .set_clock(PerformanceClock::new().map(|c| Box::new(c) as _));
        core.set_export_clock(PerformanceClock::new().map(|c| Box::new(c) as _));
        let forced = force.as_deref().and_then(Backend::parse);
        let order: Vec<Backend> = match forced {
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npx playwright test e2e/clipboard.spec.ts e2e/app.spec.ts --project=chromium`

Expected: PASS: `ferrum-editor` и `ferrum-wasm` — 539 passed, 2 ignored (`keymap` — 55 команд, `core` — 27 тестов); модульные — 134 passed, сборка — ок; E2E `clipboard` и `app` в Chromium — 40 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts apps/web/e2e/clipboard.spec.ts apps/web/src/ui/chrome.tsx apps/web/src/ui/system-clipboard.ts crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/tests/commands.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/src/web/mod.rs crates/ferrum-wasm/tests/boolean.rs crates/ferrum-wasm/tests/core.rs crates/ferrum-wasm/tests/fonts.rs
git commit -m "feat: «Копировать как SVG» — текст и image/svg+xml в буфер обмена

Команда edit.copy_svg «Копировать как SVG» (без сочетания, как в Figma; пункт
контекстного меню холста): редактор сообщает узлы (EditorEvent::ExportSvg),
worker пишет SVG тем же писателем, что экспорт (текст кривыми), не больше 64 МБ,
и шлёт событие ClipboardSvg { text }; интерфейс кладёт text/plain и, где
браузер умеет (ClipboardItem.supports), image/svg+xml; отказ асинхронного
API — обычный текст через execCommand. Шрифты ещё грузятся — ошибка, как у PNG.
Часы пределов экспорта в worker'е — performance.now() (Core::set_export_clock).
Команд — 55.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6: Экспорт PDF через krilla

Спека 11: PDF — крейт `krilla` (векторные страницы, подмножества шрифтов, картинки); фрейм — страница его размера. Здесь — `ferrum_render::pdf`: страницы рисует тот же рисовальщик, что экран и PNG, через бэкенд `PdfBackend` (пути, обрезка, группы прозрачности, картинки с прямой альфой, плитки «Замостить» не больше 4096); текст сплошной краской — настоящим текстом (`fill_glyphs`, подмножество шрифта, вариативные оси, ToUnicode из кластеров), отказ krilla на шрифте — повтор файла с текстом контурами; сторона ≤ 14 400 pt.

**Files:**
- Create: `crates/ferrum-render/src/pdf.rs`
- Modify: `Cargo.toml`, `Cargo.lock`, `crates/ferrum-render/Cargo.toml`, `crates/ferrum-render/src/{backend,text_node,lib}.rs`
- Test: `crates/ferrum-render/tests/pdf.rs` (новый)

**Interfaces:**
- Consumes: `RenderBackend`, `draw` рисовальщика, `GlyphRun::cluster`, `ImageProvider`, `ExportBudget`.
- Produces: `Renderer::export_pdf(&Scene, pages, &dyn ImageProvider, &ExportBudget) -> Result<Vec<u8>, ExportError>` (страница — узел; в задаче 7 — `pages: &[Vec<NodeId>]`); `MAX_PDF_SIDE = 14 400`; `RenderBackend::fill_glyphs(&GlyphRun, to_device, Rgba) -> bool` (по умолчанию `false` — контуры).

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-render/tests/pdf.rs`:

````rust
//! Экспорт PDF через `krilla` (спека подсистемы 3, раздел 11): фрейм — страница
//! своего размера, текст — настоящим текстом со встроенным подмножеством шрифта,
//! картинки — изображениями PDF, пределы. Файл тест читает как есть: словари
//! страниц и шрифтов — открытым текстом, потоки — после распаковки Deflate.

mod common;

use std::cell::Cell;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_render::{
    decode_png, Clock, ExportBudget, ExportError, Fonts, NoImages, Pixmap, Renderer, SharedFonts,
    MAX_PDF_SIDE,
};

fn solid(r: u8, g: u8, b: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, 255))])
}

fn pdf(r: &mut Renderer, p: &mut Page, pages: &[NodeId]) -> Vec<u8> {
    let scene = p.scene();
    r.export_pdf(&scene, pages, &NoImages, &ExportBudget::unlimited())
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Все потоки файла, распакованные (несжатые — как есть).
fn streams(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = find(&bytes[at..], b"stream") {
        let start = at + i + b"stream".len();
        // «endstream» тоже содержит «stream»: пропускаем.
        if bytes[..at + i].ends_with(b"end") {
            at = start;
            continue;
        }
        let body = start + if bytes[start] == b'\r' { 2 } else { 1 };
        let Some(end) = find(&bytes[body..], b"endstream") else {
            break;
        };
        let raw = &bytes[body..body + end];
        let mut inflated = Vec::new();
        if flate2::read::ZlibDecoder::new(raw)
            .read_to_end(&mut inflated)
            .is_ok()
        {
            out.push(inflated);
        } else {
            out.push(raw.to_vec());
        }
        at = body + end + b"endstream".len();
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Размеры страниц по `/MediaBox[0 0 w h]` (`pdf-writer` пишет без пробелов).
fn media_boxes(bytes: &[u8]) -> Vec<(f64, f64)> {
    let s = text(bytes);
    s.match_indices("/MediaBox[")
        .map(|(i, m)| {
            let rest = &s[i + m.len()..];
            let nums: Vec<f64> = rest[..rest.find(']').unwrap()]
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            (nums[2] - nums[0], nums[3] - nums[1])
        })
        .collect()
}

#[test]
fn every_frame_is_a_page_of_its_own_size() {
    let mut p = Page::new();
    let a = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(255, 255, 255),
            ..rect(0.0, 0.0, 200.0, 120.0)
        },
    );
    p.add(
        Some(a),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0),
            corner_radius: Some(8.0),
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 2.0,
                align: StrokeAlign::Inside,
            }]),
            ..rect(150.0, 10.0, 80.0, 40.0)
        },
    );
    let b = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            opacity: Some(0.5),
            fills: solid(0, 0, 255),
            ..rect(300.0, 0.0, 393.0, 852.0)
        },
    );
    let bytes = pdf(&mut Renderer::new(), &mut p, &[a, b]);
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(text(&bytes).trim_end().ends_with("%%EOF"));
    assert_eq!(media_boxes(&bytes), [(200.0, 120.0), (393.0, 852.0)]);
    let content: Vec<String> = streams(&bytes).iter().map(|s| text(s)).collect();
    // Обрезка фрейма (`W n`), красная заливка, группа прозрачности второй страницы.
    assert!(content.iter().any(|c| c.contains("W\nn")), "обрезка фрейма");
    assert!(
        content.iter().any(|c| c.contains("1 0 0 rg")),
        "красная заливка"
    );
    assert!(text(&bytes).contains("/ca 0.5"), "прозрачность фрейма");
    // Ни одной картинки: всё — векторами.
    assert!(!text(&bytes).contains("/Subtype/Image"));
}

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn inter() -> SharedFonts {
    let shared = Fonts::shared();
    {
        let mut f = shared.borrow_mut();
        let files = [
            "inter/Inter-Variable.ttf",
            "inter/Inter-Italic-Variable.ttf",
        ];
        let faces = files
            .iter()
            .flat_map(|r| ferrum_text::read_faces(&font_file(r), r))
            .collect::<Vec<_>>();
        f.add_catalog(ferrum_text::SourceKind::Bundled, ferrum_text::group(faces));
        for r in files {
            f.load("Inter", r, font_file(r)).unwrap();
        }
    }
    shared
}

fn add_text(p: &mut Page, s: &str, style: &str, fills: Vec<Fill>, y: f64) -> NodeId {
    let mut props = TextProps::default();
    props.style.size = 24.0;
    props.style.font = FontName::new("Inter", style);
    p.doc
        .transact(p.page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_props(
                id,
                &PropsPatch {
                    text: Some(props.clone()),
                    fills: Some(fills.clone()),
                    ..rect(0.0, y, 400.0, 40.0)
                },
            )?;
            tx.set_text_content(id, &TextContent::plain(s))?;
            Ok(vec![id])
        })
        .unwrap()[0]
}

#[test]
fn text_is_real_text_with_font_subsets() {
    let mut p = Page::new();
    let black = vec![Fill::solid(Color::BLACK)];
    let regular = add_text(&mut p, "Привет, PDF", "Regular", black.clone(), 0.0);
    let bold = add_text(&mut p, "Жирный", "Bold", black, 50.0);
    let mut r = Renderer::new();
    r.set_fonts(inter());
    let bytes = pdf(&mut r, &mut p, &[regular, bold]);
    let raw = text(&bytes);
    // Шрифт встроен подмножеством (имя с префиксом «ABCDEF+»), по шрифту на вес.
    assert!(raw.contains("/FontFile2") || raw.contains("/FontFile3"));
    let subsets = raw.matches("/BaseFont/").count();
    assert!(subsets >= 2, "{subsets}");
    assert!(raw.contains("+Inter"), "подмножество Inter");
    let content: Vec<String> = streams(&bytes).iter().map(|s| text(s)).collect();
    assert!(
        content.iter().any(|c| c.contains("Tj") || c.contains("TJ")),
        "текстовые операторы"
    );
    // Текст для поиска и копирования — ToUnicode: «П» (U+041F) в UTF-16BE.
    assert!(content
        .iter()
        .any(|c| c.contains("<041F>") || c.contains("041F")));
    // Кривыми текст не нарисован: контуров глифов (`c` и `f`) почти нет.
    assert!(!content.iter().any(|c| c.matches(" c\n").count() > 50));
}

#[test]
fn image_fills_become_pdf_images_drawn_once_per_tile() {
    let mut p = Page::new();
    let mut px = Pixmap::new(2, 1);
    px.data_as_u8_slice_mut()
        .copy_from_slice(&[255, 0, 0, 255, 0, 0, 255, 255]);
    let png = px.into_png().unwrap();
    let hash = p.doc.images.add(png.clone());
    let id = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(vec![Fill {
                scale_mode: ScaleMode::Tile,
                ..Fill::image(hash.clone())
            }]),
            ..rect(0.0, 0.0, 40.0, 40.0)
        },
    );
    let scene = p.scene();
    let images: HashMap<String, Arc<Pixmap>> =
        HashMap::from([(hash, Arc::new(decode_png(&png).unwrap()))]);
    let bytes = Renderer::new()
        .export_pdf(&scene, &[id], &images, &ExportBudget::unlimited())
        .unwrap();
    let raw = text(&bytes);
    assert_eq!(raw.matches("/Subtype/Image").count(), 1, "картинка — одна");
    assert!(raw.contains("/Width 2") && raw.contains("/Height 1"));
    // Плитки 2×1 на 40×40 — 20 × 40 рисований одной картинки.
    let content: Vec<String> = streams(&bytes).iter().map(|s| text(s)).collect();
    let draws: usize = content.iter().map(|c| c.matches(" Do").count()).sum();
    assert_eq!(draws, 800);
}

#[test]
fn huge_frames_shrink_to_the_page_limit_and_errors_are_reported() {
    let mut p = Page::new();
    let big = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(0, 0, 0),
            ..rect(0.0, 0.0, 30_000.0, 300.0)
        },
    );
    let hidden = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            visible: Some(false),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let bytes = pdf(&mut Renderer::new(), &mut p, &[big]);
    assert_eq!(media_boxes(&bytes), [(MAX_PDF_SIDE, 144.0)]);
    let scene = p.scene();
    let mut r = Renderer::new();
    let run = |r: &mut Renderer, ids: &[NodeId], b: &ExportBudget| {
        r.export_pdf(&scene, ids, &NoImages, b)
    };
    assert_eq!(
        run(&mut r, &[hidden], &ExportBudget::unlimited()),
        Err(ExportError::Empty)
    );
    assert_eq!(
        run(&mut r, &[], &ExportBudget::unlimited()),
        Err(ExportError::NotFound)
    );
    /// Часы, которые всегда «после срока».
    struct Late(Cell<f64>);
    impl Clock for Late {
        fn now_ms(&self) -> f64 {
            self.0.set(self.0.get() + 1.0);
            self.0.get()
        }
    }
    let late = Late(Cell::new(10.0));
    let expired = ExportBudget {
        max_bytes: usize::MAX,
        clock: Some(&late),
        deadline_ms: 0.0,
    };
    assert_eq!(run(&mut r, &[big], &expired), Err(ExportError::Timeout));
    let small = ExportBudget {
        max_bytes: 100,
        ..ExportBudget::unlimited()
    };
    assert_eq!(run(&mut r, &[big], &small), Err(ExportError::TooBig));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-render --test pdf`
Expected: FAIL — не собирается: нет крейта `flate2` в dev-зависимостях, `ferrum_render::MAX_PDF_SIDE`, метода `Renderer::export_pdf` (5 ошибок)

- [ ] **Step 3: Реализация**

В `Cargo.lock` заменить:

````

[[package]]
name = "arrayvec"
version = "0.7.8"
````

на:

````

[[package]]
name = "arrayref"
version = "0.3.9"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "76a2e8124351fda1ef8aaaa3bbd7ebbcb486bbcd4225aca0aa0d84bb2db8fecb"

[[package]]
name = "arrayvec"
version = "0.7.8"
````

В `Cargo.lock` заменить:

````
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "f2032f911046de80f0a198e0901378627c33f59ea0ac00e363d481118bd70a53"

[[package]]
````

на:

````
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "f2032f911046de80f0a198e0901378627c33f59ea0ac00e363d481118bd70a53"

[[package]]
name = "base64"
version = "0.22.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "72b3254f16251a8381aa12e40e3c4d2f0199f8c6508fbecb9d91f575e0fbb8c6"

[[package]]
````

В `Cargo.lock` заменить:

````

[[package]]
name = "bytes"
version = "1.12.1"
````

на:

````

[[package]]
name = "byteorder-lite"
version = "0.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "8f1fe948ff07f4bd06c30984e69f5b4899c516a3ef74f34df92a2df2ab535495"

[[package]]
name = "bytes"
version = "1.12.1"
````

В `Cargo.lock` заменить:

````
dependencies = [
 "bytemuck",
]

[[package]]
name = "console_error_panic_hook"
````

на:

````
dependencies = [
 "bytemuck",
]

[[package]]
name = "color_quant"
version = "1.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3d7b894f5411737b7867f4827955924d7c254fc9f4d91a6aad6b097804b1018b"

[[package]]
name = "console_error_panic_hook"
````

В `Cargo.lock` заменить:

````
dependencies = [
 "serde",
 "skrifa",
]

````

на:

````
dependencies = [
 "serde",
 "skrifa 0.44.0",
]

````

В `Cargo.lock` заменить:

````
 "ferrum-scene",
 "ferrum-text",
 "kurbo",
 "parley",
 "roxmltree",
 "skrifa",
 "thiserror 2.0.21",
 "vello_common",
````

на:

````
 "ferrum-scene",
 "ferrum-text",
 "flate2",
 "krilla",
 "kurbo",
 "parley",
 "roxmltree",
 "skrifa 0.44.0",
 "thiserror 2.0.21",
 "vello_common",
````

В `Cargo.lock` заменить:

````
 "serde",
 "serde_json",
 "skrifa",
]

````

на:

````
 "serde",
 "serde_json",
 "skrifa 0.44.0",
]

````

В `Cargo.lock` заменить:

````

[[package]]
name = "fnv"
version = "1.0.7"
````

на:

````

[[package]]
name = "float-cmp"
version = "0.9.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "98de4bbd547a563b716d8dfa9aad1cb19bfab00f4fa09a6a4ed21dbcf44ce9c4"
dependencies = [
 "num-traits",
]

[[package]]
name = "fnv"
version = "1.0.7"
````

В `Cargo.lock` заменить:

````
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "77ce24cb58228fbb8aa041425bb1050850ac19177686ea6e0f41a70416f56fdb"

[[package]]
````

на:

````
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "77ce24cb58228fbb8aa041425bb1050850ac19177686ea6e0f41a70416f56fdb"

[[package]]
name = "font-types"
version = "0.11.3"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "5b38ad915f6dadd993ced50848a8291a543bd41ca62bc10740d5e64e2ab4cfd7"
dependencies = [
 "bytemuck",
]

[[package]]
````

В `Cargo.lock` заменить:

````
 "memmap2",
 "parlance",
 "read-fonts",
 "smallvec",
]
````

на:

````
 "memmap2",
 "parlance",
 "read-fonts 0.41.0",
 "smallvec",
]
````

В `Cargo.lock` заменить:

````

[[package]]
name = "glifo"
version = "0.4.0"
````

на:

````

[[package]]
name = "gif"
version = "0.14.2"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "ee8cfcc411d9adbbaba82fb72661cc1bcca13e8bba98b364e62b2dba8f960159"
dependencies = [
 "color_quant",
 "weezl",
]

[[package]]
name = "glifo"
version = "0.4.0"
````

В `Cargo.lock` заменить:

````
 "peniko",
 "png",
 "skrifa",
 "smallvec",
 "vello_common",
````

на:

````
 "peniko",
 "png",
 "skrifa 0.44.0",
 "smallvec",
 "vello_common",
````

В `Cargo.lock` заменить:

````
 "bitflags",
 "bytemuck",
 "read-fonts",
 "smallvec",
]
````

на:

````
 "bitflags",
 "bytemuck",
 "read-fonts 0.41.0",
 "smallvec",
]
````

В `Cargo.lock` заменить:

````

[[package]]
name = "indexmap"
version = "2.14.2"
````

на:

````

[[package]]
name = "image-webp"
version = "0.2.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "525e9ff3e1a4be2fbea1fdf0e98686a6d98b4d8f937e1bf7402245af1909e8c3"
dependencies = [
 "byteorder-lite",
 "quick-error 2.0.1",
]

[[package]]
name = "imagesize"
version = "0.14.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "09e54e57b4c48b40f7aec75635392b12b3421fa26fe8b4332e63138ed278459c"

[[package]]
name = "indexmap"
version = "2.14.2"
````

В `Cargo.lock` заменить:

````
dependencies = [
 "cpufeatures 0.2.17",
]

````

на:

````
dependencies = [
 "cpufeatures 0.2.17",
]

[[package]]
name = "krilla"
version = "0.8.2"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "27da593198b20eeba65caeb73c2bbeec3e53ab08fa549898312ce81c4fce5e33"
dependencies = [
 "base64",
 "bumpalo",
 "flate2",
 "float-cmp",
 "gif",
 "image-webp",
 "imagesize",
 "indexmap",
 "pdf-writer",
 "png",
 "rustc-hash 2.1.3",
 "siphasher",
 "skrifa 0.42.1",
 "smallvec",
 "subsetter",
 "tiny-skia-path",
 "xmp-writer",
 "yoke",
 "zune-jpeg",
]

````

В `Cargo.lock` заменить:

````
 "parley_data",
 "parley_emoji",
 "skrifa",
]

````

на:

````
 "parley_data",
 "parley_emoji",
 "skrifa 0.44.0",
]

[[package]]
name = "pdf-writer"
version = "0.15.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "f5e456864a7a304047bff84977dc6fb162bd956475d40ba50b2dcecaada7f753"
dependencies = [
 "bitflags",
 "itoa",
 "memchr",
 "ryu",
]

````

В `Cargo.lock` заменить:

````

[[package]]
name = "quick_cache"
version = "0.6.24"
````

на:

````

[[package]]
name = "quick-error"
version = "2.0.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "a993555f31e5a609f617c12db6250dedcac1b0a85076912c436e6fc9b2c8e6a3"

[[package]]
name = "quick_cache"
version = "0.6.24"
````

В `Cargo.lock` заменить:

````
[[package]]
name = "read-fonts"
version = "0.41.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
````

на:

````
[[package]]
name = "read-fonts"
version = "0.39.2"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "c4ed38b89c2c77ff968c524145ad65fb010f38af5c7a224b53b81d47ac2daa81"
dependencies = [
 "bytemuck",
 "font-types 0.11.3",
]

[[package]]
name = "read-fonts"
version = "0.41.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
````

В `Cargo.lock` заменить:

````
dependencies = [
 "bytemuck",
 "font-types",
 "once_cell",
]
````

на:

````
dependencies = [
 "bytemuck",
 "font-types 0.12.6",
 "once_cell",
]
````

В `Cargo.lock` заменить:

````
dependencies = [
 "fnv",
 "quick-error",
 "tempfile",
 "wait-timeout",
]

[[package]]
````

на:

````
dependencies = [
 "fnv",
 "quick-error 1.2.3",
 "tempfile",
 "wait-timeout",
]

[[package]]
name = "ryu"
version = "1.0.23"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "9774ba4a74de5f7b1c1451ed6cd5285a32eddb5cccb8cc655a4e50009e06477f"

[[package]]
````

В `Cargo.lock` заменить:

````
[[package]]
name = "skrifa"
version = "0.44.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
````

на:

````
[[package]]
name = "skrifa"
version = "0.42.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "0c34617370ae968efb7161bb2beb517d9084659aae19e24b89e3db25b46e4564"
dependencies = [
 "bytemuck",
 "read-fonts 0.39.2",
]

[[package]]
name = "skrifa"
version = "0.44.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
````

В `Cargo.lock` заменить:

````
dependencies = [
 "bytemuck",
 "read-fonts",
]

````

на:

````
dependencies = [
 "bytemuck",
 "read-fonts 0.41.0",
]

````

В `Cargo.lock` заменить:

````

[[package]]
name = "string_cache"
version = "0.8.9"
````

на:

````

[[package]]
name = "strict-num"
version = "0.1.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "6637bab7722d379c8b41ba849228d680cc12d0a45ba1fa2b48f2a30577a06731"

[[package]]
name = "string_cache"
version = "0.8.9"
````

В `Cargo.lock` заменить:

````
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "7da8b5736845d9f2fcb837ea5d9e2628564b3b043a70948a3f0b778838c5fb4f"

[[package]]
````

на:

````
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "7da8b5736845d9f2fcb837ea5d9e2628564b3b043a70948a3f0b778838c5fb4f"

[[package]]
name = "subsetter"
version = "0.2.6"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "38803281d1c23166c5ebcb455439a5d2afe711cc909cf88af72448c297756ad6"
dependencies = [
 "kurbo",
 "rustc-hash 2.1.3",
 "skrifa 0.42.1",
 "write-fonts",
]

[[package]]
````

В `Cargo.lock` заменить:

````
checksum = "1ad99c4c6d32803332c548b1af0540b357b3f5fc0be8f6c6bfe8b2e6ae784070"
dependencies = [
 "cfg-if",
]

[[package]]
````

на:

````
checksum = "1ad99c4c6d32803332c548b1af0540b357b3f5fc0be8f6c6bfe8b2e6ae784070"
dependencies = [
 "cfg-if",
]

[[package]]
name = "tiny-skia-path"
version = "0.12.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "edca365c3faccca67d06593c5980fa6c57687de727a03131735bb85f01fdeeb9"
dependencies = [
 "arrayref",
 "bytemuck",
 "strict-num",
]

[[package]]
````

В `Cargo.lock` заменить:

````

[[package]]
name = "wesl"
version = "0.4.0"
````

на:

````

[[package]]
name = "weezl"
version = "0.1.12"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "a28ac98ddc8b9274cb41bb4d9d4d5c425b6020c50c46f25559911905610b4a88"

[[package]]
name = "wesl"
version = "0.4.0"
````

В `Cargo.lock` заменить:

````

[[package]]
name = "writeable"
version = "0.6.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3ad82d2a33cdc9674dc7465672f271e096168fcdbe0f799d9e6db8c5892679dc"

[[package]]
````

на:

````

[[package]]
name = "write-fonts"
version = "0.48.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "cb731d4c4d93eacc69a1ad2f270f905788a98e4a3438267bcafbe08d3431c8d8"
dependencies = [
 "font-types 0.11.3",
 "indexmap",
 "kurbo",
 "log",
 "read-fonts 0.39.2",
]

[[package]]
name = "writeable"
version = "0.6.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3ad82d2a33cdc9674dc7465672f271e096168fcdbe0f799d9e6db8c5892679dc"

[[package]]
name = "xmp-writer"
version = "0.3.3"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "9440ea3e5aeabb0ac63af70daf835274065238cdd0cec83418f417eae38bacee"

[[package]]
````

В `Cargo.lock` заменить:

````
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "29666d0abbfad1e3dc4dcf6144730dd3a3ab225bbbdac83319345b1b44ccfc1b"
````

на:

````
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "29666d0abbfad1e3dc4dcf6144730dd3a3ab225bbbdac83319345b1b44ccfc1b"

[[package]]
name = "zune-core"
version = "0.5.3"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "d56377fd46368984a170bc5aac5567e52ca5da874caa60bea39fcbca78fb658b"

[[package]]
name = "zune-jpeg"
version = "0.5.15"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "27bc9d5b815bc103f142aa054f561d9187d191692ec7c2d1e2b4737f8dbd7296"
dependencies = [
 "zune-core",
]
````

В `Cargo.toml` заменить:

````toml
console_error_panic_hook = "=0.1.7"
wasm-bindgen-test = "=0.3.79"
# Проверка экспорта SVG в тестах: файл разбирается как XML (только dev-зависимость).
roxmltree = "=0.21.1"
````

на:

````toml
console_error_panic_hook = "=0.1.7"
wasm-bindgen-test = "=0.3.79"
# Экспорт PDF (спека подсистемы 3, раздел 11): векторные страницы, подмножества
# шрифтов, картинки. 0.8.2 — последняя (2026-06-04), MIT OR Apache-2.0, Rust 1.92,
# собирается под wasm32 без фич по умолчанию: `raster-images` нужен для картинок
# (png той же версии, что у vello_cpu), `simple-text` (rustybuzz) не нужен — глифы
# раскладывает parley. Тянет свой skrifa 0.42 (у нас 0.44) — цена версии.
krilla = { version = "=0.8.2", default-features = false, features = ["raster-images"] }
# Проверка экспорта SVG в тестах: файл разбирается как XML (только dev-зависимость).
roxmltree = "=0.21.1"
# Проверка экспорта PDF в тестах: потоки PDF сжаты Deflate (та же версия, что тянет krilla).
flate2 = "=1.1.10"
````

В `crates/ferrum-render/Cargo.toml` заменить:

````toml
skrifa.workspace = true
thiserror.workspace = true

[dev-dependencies]
roxmltree.workspace = true
````

на:

````toml
skrifa.workspace = true
thiserror.workspace = true
krilla.workspace = true

[dev-dependencies]
roxmltree.workspace = true
flate2.workspace = true
````

В `crates/ferrum-render/src/backend.rs` заменить:

````rust
    fn push_opacity_layer(&mut self, alpha: f32);
    fn pop_layer(&mut self);
    /// Рисует ли бэкенд изображения из `Pixmap`. Без этого рендер не использует
    /// растровый кэш фреймов (он хранится как изображения).
````

на:

````rust
    fn push_opacity_layer(&mut self, alpha: f32);
    fn pop_layer(&mut self);
    /// Глифы прогона текстом (PDF: шрифт с подмножеством, текст ищется и
    /// копируется) краской `color`; `to_device` — координаты узла → устройство.
    /// `false` — бэкенд так не умеет, рендер заливает контуры глифов.
    fn fill_glyphs(
        &mut self,
        _run: &ferrum_text::GlyphRun,
        _to_device: Affine,
        _color: Rgba,
    ) -> bool {
        false
    }
    /// Рисует ли бэкенд изображения из `Pixmap`. Без этого рендер не использует
    /// растровый кэш фреймов (он хранится как изображения).
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
mod image;
mod overlay;
mod svg;
mod text;
````

на:

````rust
mod image;
mod overlay;
mod pdf;
mod svg;
mod text;
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    VECTOR_HANDLE_RADIUS, VECTOR_MAX_POINTS_DRAWN, VECTOR_POINT_RADIUS,
};
pub use svg::{ImageBytes, NoImageBytes, SvgOptions};
pub use text::{Label, TextEngine, ONEST};
````

на:

````rust
    VECTOR_HANDLE_RADIUS, VECTOR_MAX_POINTS_DRAWN, VECTOR_POINT_RADIUS,
};
pub use pdf::MAX_PDF_SIDE;
pub use svg::{ImageBytes, NoImageBytes, SvgOptions};
pub use text::{Label, TextEngine, ONEST};
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    }

    /// Превью страницы для экрана файлов: всё содержимое (границы корней с полем
    /// `THUMB_PADDING` с каждой стороны), вписанное по центру в картинку
````

на:

````rust
    }

    /// Экспорт в PDF (спека подсистемы 3, раздел 11): каждый узел `pages` —
    /// страница размера его области (фрейм — своего размера, 1 px = 1 pt),
    /// векторная, с подмножествами шрифтов и картинками из `images`.
    pub fn export_pdf(
        &mut self,
        scene: &Scene,
        pages: &[NodeId],
        images: &dyn ImageProvider,
        budget: &ExportBudget,
    ) -> Result<Vec<u8>, ExportError> {
        pdf::export_pdf(scene, pages, images, &self.fonts, budget)
    }

    /// Превью страницы для экрана файлов: всё содержимое (границы корней с полем
    /// `THUMB_PADDING` с каждой стороны), вписанное по центру в картинку
````

Создать `crates/ferrum-render/src/pdf.rs`:

````rust
//! Экспорт в PDF через `krilla` (спека подсистемы 3, раздел 11): векторные
//! страницы, подмножества шрифтов, картинки. Узел — страница размера его области
//! (фрейм — своего размера, 1 px = 1 pt, как в Figma). Рисует тот же
//! рисовальщик, что экран и PNG (`Painter`), через свой бэкенд: заливки и
//! обводки — путями PDF, обрезка фреймов и обводок — обрезкой PDF, прозрачность
//! — группами прозрачности, картинки — изображениями PDF; текст сплошной краской
//! — настоящим текстом (шрифт с подмножеством глифов, текст ищется и
//! копируется), прочий (картинкой, обводки) — контурами глифов.
//!
//! Страница PDF не больше 14 400 pt по стороне (предел формата): большая область
//! уменьшается до него целиком.

use std::collections::HashMap;
use std::sync::Arc;

use ferrum_doc::NodeId;
use ferrum_scene::Scene;
use ferrum_text::{GlyphRun, SharedFonts};
use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Point as KPoint, Size as KSize, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill as KFill, FillRule, LineCap, LineJoin, Stroke as KStroke};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph, Tag};
use krilla::{Data, Document, SerializeSettings};
use kurbo::{Affine, BezPath, PathEl, Rect, Shape};
use skrifa::MetadataProvider;
use vello_cpu::peniko::{Extend, Fill};

use crate::backend::{ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::draw::{DrawContext, Painter};
use crate::export::{node_area, ExportBudget};
use crate::image::ImageProvider;
use crate::ExportError;

/// Наибольшая сторона страницы PDF, pt (предел формата без `UserUnit`).
pub const MAX_PDF_SIDE: f64 = 14_400.0;
/// Плиток картинки «Замостить» на одну заливку не больше (дальше — без плиток).
const MAX_TILES: usize = 4096;
/// Как часто бэкенд сверяется с часами (вызовов рисования).
const CHECK_EVERY: usize = 4096;

/// Текущая краска.
#[derive(Clone)]
enum Paint {
    Solid(Rgba),
    Image(ImagePaint),
}

/// Ключ шрифта: байты (по `Blob::id`), номер в коллекции, координаты осей.
type FontKey = (u64, u32, Vec<i16>);

/// Кэши одного документа: шрифты и картинки встраиваются один раз.
#[derive(Default)]
struct Cache {
    fonts: HashMap<FontKey, Option<Font>>,
    images: HashMap<ImageKey, Image>,
}

/// Бэкенд поверх поверхности страницы `krilla`. Матрица применяется к
/// координатам путей (обрезка в PDF живёт в стеке состояния, и снять матрицу,
/// не сняв обрезку, нельзя), у обводок и картинок — на время одного вызова.
struct PdfBackend<'s, 'p> {
    surface: &'s mut Surface<'p>,
    cache: &'s mut Cache,
    size: (u16, u16),
    transform: Affine,
    paint: Paint,
    /// Текст — настоящим текстом (иначе контурами).
    text: bool,
    budget: &'s ExportBudget<'s>,
    calls: usize,
    expired: bool,
}

fn finite(v: f64) -> f32 {
    if v.is_finite() {
        v.clamp(-1.0e9, 1.0e9) as f32
    } else {
        0.0
    }
}

fn transform(a: Affine) -> Transform {
    let c = a.as_coeffs();
    Transform::from_row(
        finite(c[0]),
        finite(c[1]),
        finite(c[2]),
        finite(c[3]),
        finite(c[4]),
        finite(c[5]),
    )
}

/// Путь `krilla` из пути `kurbo` с матрицей `t`; пустой — `None`.
fn path(p: &BezPath, t: Affine) -> Option<Path> {
    let mut b = PathBuilder::new();
    for el in p.elements() {
        match t * *el {
            PathEl::MoveTo(a) => b.move_to(finite(a.x), finite(a.y)),
            PathEl::LineTo(a) => b.line_to(finite(a.x), finite(a.y)),
            PathEl::QuadTo(a, c) => b.quad_to(finite(a.x), finite(a.y), finite(c.x), finite(c.y)),
            PathEl::CurveTo(a, c, d) => b.cubic_to(
                finite(a.x),
                finite(a.y),
                finite(c.x),
                finite(c.y),
                finite(d.x),
                finite(d.y),
            ),
            PathEl::ClosePath => b.close(),
        }
    }
    b.finish()
}

fn rgb_of(c: Rgba) -> (rgb::Color, NormalizedF32) {
    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let [r, g, b, a] = c.components;
    (
        rgb::Color::new(to8(r), to8(g), to8(b)),
        NormalizedF32::new(a.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE),
    )
}

fn rule(r: Fill) -> FillRule {
    match r {
        Fill::NonZero => FillRule::NonZero,
        Fill::EvenOdd => FillRule::EvenOdd,
    }
}

/// Картинка PDF из пикселей с премультипликацией (PDF ждёт прямую альфу).
fn image_of(pixmap: &vello_cpu::Pixmap) -> Image {
    let mut data = pixmap.data_as_u8_slice().to_vec();
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a == 0 {
            px.fill(0);
        } else if a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Image::from_rgba8(data, pixmap.width() as u32, pixmap.height() as u32)
}

/// Шрифт прогона для PDF: байты шрифта и положение на осях вариативного
/// шрифта — вес и ширина из начертания прогона, остальные оси — из его
/// нормализованных координат (без `avar`). `None` — шрифт не читается.
fn font_of(run: &GlyphRun) -> Option<Font> {
    let data = run.font.data.clone();
    let index = run.font.index;
    let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(data);
    let data = Data::from(bytes);
    if run.coords.iter().all(|c| *c == 0) {
        return Font::new(data, index);
    }
    let font = skrifa::FontRef::from_index(data.as_ref(), index).ok()?;
    let mut axes = Vec::new();
    for (i, axis) in font.axes().iter().enumerate() {
        let tag = axis.tag().to_be_bytes();
        let n = run.coords.get(i).copied().unwrap_or(0) as f32 / 16384.0;
        let value = match &tag {
            b"wght" => run.face.weight,
            b"wdth" => run.face.stretch * 100.0,
            _ => {
                let (min, def, max) = (axis.min_value(), axis.default_value(), axis.max_value());
                if n >= 0.0 {
                    def + n * (max - def)
                } else {
                    def + n * (def - min)
                }
            }
        };
        axes.push((Tag::new(&tag), value));
    }
    Font::new_variable(data, index, &axes)
}

impl PdfBackend<'_, '_> {
    fn tick(&mut self) -> bool {
        if self.expired {
            return false;
        }
        self.calls += 1;
        if self.calls.is_multiple_of(CHECK_EVERY) && self.budget.check(0).is_err() {
            self.expired = true;
            return false;
        }
        true
    }

    fn fill_solid(&mut self, p: &Path, c: Rgba, r: FillRule) {
        let (color, opacity) = rgb_of(c);
        self.surface.set_stroke(None);
        self.surface.set_fill(Some(KFill {
            paint: color.into(),
            opacity,
            rule: r,
        }));
        self.surface.draw_path(p);
    }

    /// Картинка по форме `p` (координаты страницы): обрезка формой, внутри —
    /// одна картинка или плитки.
    fn fill_image(&mut self, p: &Path, bounds: Rect, r: FillRule, img: &ImagePaint) {
        let key = img.key;
        let image = self
            .cache
            .images
            .entry(key)
            .or_insert_with(|| image_of(&img.pixmap))
            .clone();
        let (iw, ih) = (img.pixmap.width() as f64, img.pixmap.height() as f64);
        let place = self.transform * img.transform;
        let Some(size) = KSize::from_wh(iw as f32, ih as f32) else {
            return;
        };
        self.surface.push_clip_path(p, &r);
        let alpha = NormalizedF32::new(img.alpha.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE);
        let layered = img.alpha < 1.0;
        if layered {
            self.surface.push_opacity(alpha);
        }
        match img.extend {
            Extend::Repeat => {
                if place.determinant().abs() > 1e-12 {
                    let b = place.inverse().transform_rect_bbox(bounds);
                    let (i0, i1) = ((b.x0 / iw).floor() as i64, (b.x1 / iw).ceil() as i64);
                    let (j0, j1) = ((b.y0 / ih).floor() as i64, (b.y1 / ih).ceil() as i64);
                    let n = ((i1 - i0).max(0) as u64).saturating_mul((j1 - j0).max(0) as u64);
                    if n as usize <= MAX_TILES {
                        for j in j0..j1 {
                            for i in i0..i1 {
                                let t = place * Affine::translate((i as f64 * iw, j as f64 * ih));
                                self.surface.push_transform(&transform(t));
                                self.surface.draw_image(image.clone(), size);
                                self.surface.pop();
                            }
                        }
                    }
                }
            }
            _ => {
                self.surface.push_transform(&transform(place));
                self.surface.draw_image(image, size);
                self.surface.pop();
            }
        }
        if layered {
            self.surface.pop();
        }
        self.surface.pop();
    }
}

impl RenderBackend for PdfBackend<'_, '_> {
    fn begin_frame(&mut self, _width: u16, _height: u16) {}

    fn size(&self) -> (u16, u16) {
        self.size
    }

    fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn set_color(&mut self, color: Rgba) {
        self.paint = Paint::Solid(color);
    }

    fn set_image(&mut self, image: &ImagePaint) {
        self.paint = Paint::Image(image.clone());
    }

    fn fill_path(&mut self, p: &BezPath, r: Fill) {
        if !self.tick() {
            return;
        }
        let Some(kp) = path(p, self.transform) else {
            return;
        };
        match self.paint.clone() {
            Paint::Solid(c) => self.fill_solid(&kp, c, rule(r)),
            Paint::Image(img) => {
                let bounds = self.transform.transform_rect_bbox(p.bounding_box());
                self.fill_image(&kp, bounds, rule(r), &img);
            }
        }
    }

    fn fill_rect(&mut self, r: &Rect) {
        let mut p = BezPath::new();
        p.extend(r.path_elements(0.1));
        self.fill_path(&p, Fill::NonZero);
    }

    fn stroke_path(&mut self, p: &BezPath, stroke: &kurbo::Stroke) {
        if !self.tick() {
            return;
        }
        let Paint::Solid(c) = self.paint else {
            return;
        };
        let Some(kp) = path(p, Affine::IDENTITY) else {
            return;
        };
        let (color, opacity) = rgb_of(c);
        let line_join = match stroke.join {
            kurbo::Join::Miter => LineJoin::Miter,
            kurbo::Join::Round => LineJoin::Round,
            kurbo::Join::Bevel => LineJoin::Bevel,
        };
        let line_cap = match stroke.start_cap {
            kurbo::Cap::Butt => LineCap::Butt,
            kurbo::Cap::Round => LineCap::Round,
            kurbo::Cap::Square => LineCap::Square,
        };
        self.surface.push_transform(&transform(self.transform));
        self.surface.set_fill(None);
        self.surface.set_stroke(Some(KStroke {
            paint: color.into(),
            width: finite(stroke.width),
            miter_limit: finite(stroke.miter_limit),
            line_cap,
            line_join,
            opacity,
            dash: None,
        }));
        self.surface.draw_path(&kp);
        self.surface.set_stroke(None);
        self.surface.pop();
    }

    fn push_clip(&mut self, p: &BezPath) {
        // Пустая обрезка не рисует ничего — как прямоугольник нулевой площади.
        let kp = path(p, self.transform).unwrap_or_else(|| {
            let mut b = PathBuilder::new();
            b.move_to(0.0, 0.0);
            b.line_to(0.0, 0.0);
            b.close();
            b.finish().expect("непустой путь")
        });
        self.surface.push_clip_path(&kp, &FillRule::NonZero);
    }

    fn pop_clip(&mut self) {
        self.surface.pop();
    }

    fn push_opacity_layer(&mut self, alpha: f32) {
        self.surface
            .push_opacity(NormalizedF32::new(alpha.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE));
    }

    fn pop_layer(&mut self) {
        self.surface.pop();
    }

    fn fill_glyphs(&mut self, run: &GlyphRun, to_device: Affine, color: Rgba) -> bool {
        if !self.text {
            return false;
        }
        if !self.tick() {
            return true;
        }
        let Some(first) = run.glyphs.first().copied() else {
            return true;
        };
        let key: FontKey = (run.font.data.id(), run.font.index, run.coords.clone());
        let font = self
            .cache
            .fonts
            .entry(key)
            .or_insert_with(|| font_of(run))
            .clone();
        let Some(font) = font else {
            return false;
        };
        let size = run.size;
        if !(size.is_finite() && size > 0.0) {
            return true;
        }
        let glyphs: Vec<KrillaGlyph> = run
            .glyphs
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let next = run.glyphs.get(i + 1).map_or(g.x + g.advance, |n| n.x);
                KrillaGlyph::new(
                    GlyphId::new(g.id),
                    (next - g.x) / size,
                    0.0,
                    -(g.y - first.y) / size,
                    0.0,
                    g.cluster[0] as usize..g.cluster[1] as usize,
                    None,
                )
            })
            .collect();
        // Синтетический курсив — наклон вокруг базовой линии.
        let skew = run.skew.map_or(Affine::IDENTITY, |deg| {
            let by = first.y as f64;
            Affine::translate((0.0, by))
                * Affine::skew(-(deg as f64).to_radians().tan(), 0.0)
                * Affine::translate((0.0, -by))
        });
        let (rgb, opacity) = rgb_of(color);
        self.surface.push_transform(&transform(to_device * skew));
        self.surface.set_stroke(None);
        self.surface.set_fill(Some(KFill {
            paint: rgb.into(),
            opacity,
            rule: FillRule::NonZero,
        }));
        self.surface.draw_glyphs(
            KPoint::from_xy(first.x, first.y),
            &glyphs,
            font,
            &run.text,
            size,
            false,
        );
        self.surface.pop();
        true
    }
}

/// Страницы: узел — страница размера его области (не больше `MAX_PDF_SIDE`).
/// Скрытые и неизвестные узлы пропускаются; не осталось ни одного — ошибка.
pub(crate) fn export_pdf(
    scene: &Scene,
    pages: &[NodeId],
    images: &dyn ImageProvider,
    fonts: &SharedFonts,
    budget: &ExportBudget,
) -> Result<Vec<u8>, ExportError> {
    match build(scene, pages, images, fonts, budget, true) {
        // Шрифт, который `krilla` не смог встроить (повреждённые таблицы и т. п.):
        // текст — контурами глифов, как у SVG с «Текстом кривыми».
        Err(ExportError::Write(_)) => build(scene, pages, images, fonts, budget, false),
        other => other,
    }
}

fn build(
    scene: &Scene,
    pages: &[NodeId],
    images: &dyn ImageProvider,
    fonts: &SharedFonts,
    budget: &ExportBudget,
    text: bool,
) -> Result<Vec<u8>, ExportError> {
    let mut known = false;
    let mut list = Vec::new();
    for id in pages {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        known = true;
        if scene.at(slot).subtree_bounds.is_none() {
            continue;
        }
        let Some(area) = node_area(scene, slot) else {
            continue;
        };
        if !(area.width().is_finite() && area.height().is_finite())
            || area.width() < 0.5
            || area.height() < 0.5
        {
            continue;
        }
        list.push((slot, area));
    }
    if !known {
        return Err(ExportError::NotFound);
    }
    if list.is_empty() {
        return Err(ExportError::Empty);
    }
    let settings = SerializeSettings {
        // Тегированный PDF нужен для доступности, но без структуры документа теги
        // пусты: файл меньше и пишется быстрее.
        enable_tagging: false,
        ..SerializeSettings::default()
    };
    let mut doc = Document::new_with(settings);
    let mut cache = Cache::default();
    for (slot, area) in list {
        let k = (MAX_PDF_SIDE / area.width().max(area.height())).min(1.0);
        let (w, h) = (area.width() * k, area.height() * k);
        let size = KSize::from_wh(w as f32, h as f32).ok_or(ExportError::Empty)?;
        let mut page = doc.start_page_with(PageSettings::new(size));
        let mut surface = page.surface();
        let mut backend = PdfBackend {
            surface: &mut surface,
            cache: &mut cache,
            size: (w.min(u16::MAX as f64) as u16, h.min(u16::MAX as f64) as u16),
            transform: Affine::IDENTITY,
            paint: Paint::Solid(Rgba::new([0.0, 0.0, 0.0, 1.0])),
            text,
            budget,
            calls: 0,
            expired: false,
        };
        let ctx = DrawContext {
            scene,
            images,
            fonts,
            transform: Affine::scale(k) * Affine::translate(-area.origin().to_vec2()),
            scale: k,
            cull: None,
            lod: false,
            raster: false,
            retain: false,
        };
        let mut painter = Painter::with_raster_limit(0);
        painter.begin_frame();
        painter.draw(&mut backend, &ctx, &[slot]);
        let expired = backend.expired;
        surface.finish();
        page.finish();
        if expired {
            return Err(ExportError::Timeout);
        }
        budget.check(0)?;
    }
    let bytes = doc
        .finish()
        .map_err(|e| ExportError::Write(format!("{e:?}")))?;
    budget.check(bytes.len())?;
    Ok(bytes)
}
````

В `crates/ferrum-render/src/text_node.rs` заменить:

````rust
    };
    let Some(hash) = image else {
        backend.set_color(color(paint.color).multiply_alpha(a));
        for g in &run.glyphs {
            let path = fonts.outline(&run.font, g.id, &run.coords);
````

на:

````rust
    };
    let Some(hash) = image else {
        let c = color(paint.color).multiply_alpha(a);
        // Бэкенд с настоящим текстом (PDF) рисует прогон сам.
        if backend.fill_glyphs(run, to_device, c) {
            return true;
        }
        backend.set_color(c);
        for g in &run.glyphs {
            let path = fonts.outline(&run.font, g.id, &run.coords);
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-render 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: clippy wasm32 без замечаний (`krilla` собирается под wasm32); `ferrum-render` — 149 passed (`pdf` — новые тесты зелёные)

- [ ] **Step 5: Закоммитить**

```bash
git add Cargo.lock Cargo.toml crates/ferrum-render/Cargo.toml crates/ferrum-render/src/backend.rs crates/ferrum-render/src/lib.rs crates/ferrum-render/src/pdf.rs crates/ferrum-render/src/text_node.rs crates/ferrum-render/tests/pdf.rs
git commit -m "feat(render): экспорт PDF через krilla — страницы фреймов, текст со шрифтами, картинки

Renderer::export_pdf: узел — страница размера его области (фрейм — своего,
1 px = 1 pt), не больше 14 400 pt по стороне (большее уменьшается целиком).
Рисует тот же Painter, что экран и PNG, через PdfBackend поверх поверхности
krilla: пути, обрезка, группы прозрачности, картинки (прямая альфа, «Замостить»
— плитками, не больше 4096), обводки. Новый метод бэкенда fill_glyphs: прогон
сплошной краской — настоящим текстом (шрифт с подмножеством глифов, вес и
ширина вариативного шрифта — из начертания, ToUnicode из кластеров), прочее —
контурами. Шрифт, который krilla не встроил, — повтор с текстом контурами.
Пределы — ExportBudget (байты, время с проверкой каждые 4096 вызовов).
krilla =0.8.2 без фич по умолчанию + raster-images (23 новых крейта в
Cargo.lock, свой skrifa 0.42); flate2 =1.1.10 — dev, распаковка потоков в тестах.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 7: Протокол и ядро: экспорт SVG и PDF в worker'е

Спека 11: экспорт идёт в worker'е ядра, байты — передачей. Здесь — `ExportFormat` и поля `format`, `outlineText`, `svgIds` команды `Export`, событие `Exported` с типом файла, страницы PDF (`pdf_pages`: выделение — узлы верхнего уровня, иначе фреймы, иначе страница; порядок чтения), имена файлов (`file_name`), страница PDF — несколько узлов; интерфейс берёт тип скачивания из события.

**Files:**
- Modify: `crates/ferrum-wasm/src/protocol.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `crates/ferrum-render/src/{pdf,lib}.rs`, `apps/web/src/ui/app.tsx`
- Test: `crates/ferrum-wasm/tests/{panels,protocol,fonts}.rs`, `crates/ferrum-render/tests/pdf.rs`

**Interfaces:**
- Consumes: `Renderer::export_svg`, `Renderer::export_pdf`, `Core::set_export_clock`.
- Produces: `ExportFormat { Png (по умолчанию), Svg, Pdf }` (`extension()`, `mime()`); `Command::Export { scale, format?, outline_text?, svg_ids? }`; `CoreEvent::Exported { name, bytes, mime }`; `pdf_pages(scene, selection, all)`; `file_name(name, scale, format)`; `Renderer::export_pdf(…, pages: &[Vec<NodeId>], …)`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-render/tests/pdf.rs` заменить:

````rust
fn pdf(r: &mut Renderer, p: &mut Page, pages: &[NodeId]) -> Vec<u8> {
    let scene = p.scene();
    r.export_pdf(&scene, pages, &NoImages, &ExportBudget::unlimited())
        .unwrap()
}
````

на:

````rust
fn pdf(r: &mut Renderer, p: &mut Page, pages: &[NodeId]) -> Vec<u8> {
    let scene = p.scene();
    let pages: Vec<Vec<NodeId>> = pages.iter().map(|id| vec![*id]).collect();
    r.export_pdf(&scene, &pages, &NoImages, &ExportBudget::unlimited())
        .unwrap()
}
````

В `crates/ferrum-render/tests/pdf.rs` заменить:

````rust
        HashMap::from([(hash, Arc::new(decode_png(&png).unwrap()))]);
    let bytes = Renderer::new()
        .export_pdf(&scene, &[id], &images, &ExportBudget::unlimited())
        .unwrap();
    let raw = text(&bytes);
````

на:

````rust
        HashMap::from([(hash, Arc::new(decode_png(&png).unwrap()))]);
    let bytes = Renderer::new()
        .export_pdf(&scene, &[vec![id]], &images, &ExportBudget::unlimited())
        .unwrap();
    let raw = text(&bytes);
````

В `crates/ferrum-render/tests/pdf.rs` заменить:

````rust
    let mut r = Renderer::new();
    let run = |r: &mut Renderer, ids: &[NodeId], b: &ExportBudget| {
        r.export_pdf(&scene, ids, &NoImages, b)
    };
    assert_eq!(
````

на:

````rust
    let mut r = Renderer::new();
    let run = |r: &mut Renderer, ids: &[NodeId], b: &ExportBudget| {
        r.export_pdf(&scene, &[ids.to_vec()], &NoImages, b)
    };
    assert_eq!(
````

В `crates/ferrum-render/tests/pdf.rs` заменить:

````rust
        Err(ExportError::NotFound)
    );
    /// Часы, которые всегда «после срока».
    struct Late(Cell<f64>);
````

на:

````rust
        Err(ExportError::NotFound)
    );
    // Несколько узлов на странице — их общая область.
    let two = r
        .export_pdf(
            &scene,
            &[vec![big, hidden]],
            &NoImages,
            &ExportBudget::unlimited(),
        )
        .unwrap();
    assert_eq!(media_boxes(&two), [(MAX_PDF_SIDE, 144.0)]);
    /// Часы, которые всегда «после срока».
    struct Late(Cell<f64>);
````

В `crates/ferrum-wasm/tests/fonts.rs` заменить:

````rust
    c.command(Command::DemoText { texts: 100 });
    let loading = c.take_events();
    c.command(Command::Export { scale: 1.0 });
    let ev = c.take_events();
    assert_eq!(errors_of(&ev), [STILL_LOADING]);
````

на:

````rust
    c.command(Command::DemoText { texts: 100 });
    let loading = c.take_events();
    c.command(Command::Export {
        scale: 1.0,
        format: None,
        outline_text: None,
        svg_ids: None,
    });
    let ev = c.take_events();
    assert_eq!(errors_of(&ev), [STILL_LOADING]);
````

В `crates/ferrum-wasm/tests/fonts.rs` заменить:

````rust
    serve(&mut c, &loading);
    c.take_events();
    c.command(Command::Export { scale: 1.0 });
    let ev = c.take_events();
    assert!(errors_of(&ev).is_empty(), "{ev:?}");
````

на:

````rust
    serve(&mut c, &loading);
    c.take_events();
    c.command(Command::Export {
        scale: 1.0,
        format: None,
        outline_text: None,
        svg_ids: None,
    });
    let ev = c.take_events();
    assert!(errors_of(&ev).is_empty(), "{ev:?}");
````

В `crates/ferrum-wasm/tests/fonts.rs` заменить:

````rust
    serve(&mut c, &events);
    c.take_events();
    c.command(Command::Export { scale: 1.0 });
    let ev = c.take_events();
    let bytes = ev
````

на:

````rust
    serve(&mut c, &events);
    c.take_events();
    c.command(Command::Export {
        scale: 1.0,
        format: None,
        outline_text: None,
        svg_ids: None,
    });
    let ev = c.take_events();
    let bytes = ev
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
fn export_selection_and_page() {
    let mut core = demo(100);
    core.command(Command::Export { scale: 2.0 });
    let ev = core.take_events();
    let Some(CoreEvent::Exported { name, bytes }) =
        ev.iter().find(|e| matches!(e, CoreEvent::Exported { .. }))
    else {
````

на:

````rust
fn export_selection_and_page() {
    let mut core = demo(100);
    core.command(Command::Export {
        scale: 2.0,
        format: None,
        outline_text: None,
        svg_ids: None,
    });
    let ev = core.take_events();
    let Some(CoreEvent::Exported { name, bytes, .. }) =
        ev.iter().find(|e| matches!(e, CoreEvent::Exported { .. }))
    else {
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
    // 1440 × 1024 при 2× — 2880 × 2048 (IHDR: ширина и высота big-endian).
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 2880);
    core.command(Command::Export { scale: 4.0 });
    assert_eq!(
        errors(&core.take_events()),
````

на:

````rust
    // 1440 × 1024 при 2× — 2880 × 2048 (IHDR: ширина и высота big-endian).
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 2880);
    core.command(Command::Export {
        scale: 4.0,
        format: None,
        outline_text: None,
        svg_ids: None,
    });
    assert_eq!(
        errors(&core.take_events()),
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
    );
    core.command(Command::AddPage);
    core.command(Command::Export { scale: 1.0 });
    assert_eq!(
        errors(&core.take_events()),
````

на:

````rust
    );
    core.command(Command::AddPage);
    core.command(Command::Export {
        scale: 1.0,
        format: None,
        outline_text: None,
        svg_ids: None,
    });
    assert_eq!(
        errors(&core.take_events()),
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
    assert_eq!(document_steps(&core), steps + 1.0);
}
````

на:

````rust
    assert_eq!(document_steps(&core), steps + 1.0);
}

fn exported(ev: &[CoreEvent]) -> (String, Vec<u8>, String) {
    ev.iter()
        .find_map(|e| match e {
            CoreEvent::Exported { name, bytes, mime } => {
                Some((name.clone(), bytes.to_vec(), mime.clone()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("нет экспорта: {:?}", errors(ev)))
}

fn export(format: ExportFormat) -> Command {
    Command::Export {
        scale: 1.0,
        format: Some(format),
        outline_text: Some(false),
        svg_ids: Some(true),
    }
}

/// Экспорт SVG и PDF (подсистема 3, этап 5): без выделения SVG — вся страница,
/// PDF — фрейм верхнего уровня на страницу по порядку чтения; с выделением —
/// выделенные; имя — по слою или странице, тип — по формату.
#[test]
fn export_svg_and_pdf() {
    let mut core = demo(100);
    core.take_events();
    core.command(export(ExportFormat::Svg));
    let (name, bytes, mime) = exported(&core.take_events());
    assert_eq!(
        (name.as_str(), mime.as_str()),
        ("Страница 1.svg", "image/svg+xml")
    );
    let svg = String::from_utf8(bytes).unwrap();
    assert!(
        svg.starts_with("<svg width=\"") && svg.contains(" id=\""),
        "атрибут id"
    );
    // Фреймы верхнего уровня — по странице PDF (второй — из пресета).
    core.command(Command::AddFramePreset { index: 0 });
    core.command(Command::Select {
        ids: vec![],
        mode: SelectMode::Replace,
    });
    core.command(Command::LayersWindow {
        start: 0,
        count: 200,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    let frames = rows
        .iter()
        .filter(|r| r.depth == 0 && r.kind == NodeType::Frame)
        .count();
    assert!(frames > 1);
    core.command(export(ExportFormat::Pdf));
    let (name, bytes, mime) = exported(&core.take_events());
    assert_eq!(
        (name.as_str(), mime.as_str()),
        ("Страница 1.pdf", "application/pdf")
    );
    assert!(bytes.starts_with(b"%PDF-"));
    let raw = String::from_utf8_lossy(&bytes);
    assert_eq!(raw.matches("/Type/Page/").count(), frames);
    // Выделенный фрейм — одна страница его размера.
    let first = rows.iter().find(|r| r.depth == 0).unwrap();
    core.command(Command::Select {
        ids: vec![first.id.clone()],
        mode: SelectMode::Replace,
    });
    let w = summary(&core.take_events()).unwrap().w.value;
    core.command(export(ExportFormat::Pdf));
    let (name, bytes, _) = exported(&core.take_events());
    assert_eq!(name, format!("{}.pdf", first.name));
    let raw = String::from_utf8_lossy(&bytes);
    assert_eq!(raw.matches("/Type/Page/").count(), 1);
    assert!(raw.contains(&format!("/MediaBox[0 0 {w} ")), "{w}");
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        Command::AddFramePreset { index: 3 },
        Command::SetZoom { zoom: 2.0 },
        Command::Export { scale: 3.0 },
        Command::PasteImage {
            bytes: bytes(&[0x89]),
````

на:

````rust
        Command::AddFramePreset { index: 3 },
        Command::SetZoom { zoom: 2.0 },
        Command::Export {
            scale: 3.0,
            format: None,
            outline_text: None,
            svg_ids: None,
        },
        Command::PasteImage {
            bytes: bytes(&[0x89]),
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
            name: "Фон@2x.png".into(),
            bytes: bytes(b"png"),
        },
        CoreEvent::DocumentName {
````

на:

````rust
            name: "Фон@2x.png".into(),
            bytes: bytes(b"png"),
            mime: "image/png".into(),
        },
        CoreEvent::DocumentName {
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        json!({"type": "bind", "command": "tool.rect", "shortcut": "KeyQ"}),
        json!({"type": "export", "scale": "2"}),
        json!({"type": "addFramePreset", "index": -3}),
        json!({"type": "setSettings", "pixelSnap": 1}),
````

на:

````rust
        json!({"type": "bind", "command": "tool.rect", "shortcut": "KeyQ"}),
        json!({"type": "export", "scale": "2"}),
        json!({"type": "export", "scale": 1, "format": "gif"}),
        json!({"type": "export", "scale": 1, "format": "svg", "outlineText": "да"}),
        json!({"type": "addFramePreset", "index": -3}),
        json!({"type": "setSettings", "pixelSnap": 1}),
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test panels`
Expected: FAIL — не собирается: нет типа `ExportFormat`, полей `format`, `outline_text`, `svg_ids` команды `Command::Export` и поля `mime` события `Exported` (17 ошибок)

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/app.tsx` заменить:

````tsx
          }
          if (e.type === 'exported') {
            downloadBytes(e.name, e.bytes, 'image/png');
            store.toast(`Экспортировано: ${e.name}`);
          } else if (e.type === 'saved') {
````

на:

````tsx
          }
          if (e.type === 'exported') {
            downloadBytes(e.name, e.bytes, e.mime);
            store.toast(`Экспортировано: ${e.name}`);
          } else if (e.type === 'saved') {
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    }

    /// Экспорт в PDF (спека подсистемы 3, раздел 11): каждый узел `pages` —
    /// страница размера его области (фрейм — своего размера, 1 px = 1 pt),
    /// векторная, с подмножествами шрифтов и картинками из `images`.
    pub fn export_pdf(
        &mut self,
        scene: &Scene,
        pages: &[NodeId],
        images: &dyn ImageProvider,
        budget: &ExportBudget,
````

на:

````rust
    }

    /// Экспорт в PDF (спека подсистемы 3, раздел 11): каждый список узлов
    /// `pages` — страница размера их области (фрейм — своего размера, 1 px =
    /// 1 pt), векторная, с подмножествами шрифтов и картинками из `images`.
    pub fn export_pdf(
        &mut self,
        scene: &Scene,
        pages: &[Vec<NodeId>],
        images: &dyn ImageProvider,
        budget: &ExportBudget,
````

В `crates/ferrum-render/src/pdf.rs` заменить:

````rust
use crate::backend::{ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::draw::{DrawContext, Painter};
use crate::export::{node_area, ExportBudget};
use crate::image::ImageProvider;
use crate::ExportError;
````

на:

````rust
use crate::backend::{ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::draw::{DrawContext, Painter};
use crate::export::{export_items, ExportBudget};
use crate::image::ImageProvider;
use crate::ExportError;
````

В `crates/ferrum-render/src/pdf.rs` заменить:

````rust
}

/// Страницы: узел — страница размера его области (не больше `MAX_PDF_SIDE`).
/// Скрытые и неизвестные узлы пропускаются; не осталось ни одного — ошибка.
pub(crate) fn export_pdf(
    scene: &Scene,
    pages: &[NodeId],
    images: &dyn ImageProvider,
    fonts: &SharedFonts,
````

на:

````rust
}

/// Страницы: каждый список узлов — страница размера их общей области (как у
/// PNG; не больше `MAX_PDF_SIDE`). Скрытые и неизвестные узлы пропускаются; не
/// осталось ни одной страницы — ошибка.
pub(crate) fn export_pdf(
    scene: &Scene,
    pages: &[Vec<NodeId>],
    images: &dyn ImageProvider,
    fonts: &SharedFonts,
````

В `crates/ferrum-render/src/pdf.rs` заменить:

````rust
fn build(
    scene: &Scene,
    pages: &[NodeId],
    images: &dyn ImageProvider,
    fonts: &SharedFonts,
````

на:

````rust
fn build(
    scene: &Scene,
    pages: &[Vec<NodeId>],
    images: &dyn ImageProvider,
    fonts: &SharedFonts,
````

В `crates/ferrum-render/src/pdf.rs` заменить:

````rust
    text: bool,
) -> Result<Vec<u8>, ExportError> {
    let mut known = false;
    let mut list = Vec::new();
    for id in pages {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        known = true;
        if scene.at(slot).subtree_bounds.is_none() {
            continue;
        }
        let Some(area) = node_area(scene, slot) else {
            continue;
        };
        if !(area.width().is_finite() && area.height().is_finite())
            || area.width() < 0.5
            || area.height() < 0.5
        {
            continue;
        }
        list.push((slot, area));
    }
    if !known {
````

на:

````rust
    text: bool,
) -> Result<Vec<u8>, ExportError> {
    // Страница — узлы и их общая область (как у PNG): узлы, которых нет, и
    // страницы без видимого — пропускаются.
    let mut known = false;
    let mut list = Vec::new();
    for ids in pages {
        match export_items(scene, ids) {
            Ok((items, area)) => {
                known = true;
                if area.width().is_finite()
                    && area.height().is_finite()
                    && area.width() >= 0.5
                    && area.height() >= 0.5
                {
                    list.push((items.into_iter().map(|(s, _)| s).collect::<Vec<_>>(), area));
                }
            }
            Err(ExportError::NotFound) => {}
            Err(_) => known = true,
        }
    }
    if !known {
````

В `crates/ferrum-render/src/pdf.rs` заменить:

````rust
    let mut doc = Document::new_with(settings);
    let mut cache = Cache::default();
    for (slot, area) in list {
        let k = (MAX_PDF_SIDE / area.width().max(area.height())).min(1.0);
        let (w, h) = (area.width() * k, area.height() * k);
````

на:

````rust
    let mut doc = Document::new_with(settings);
    let mut cache = Cache::default();
    for (slots, area) in list {
        let k = (MAX_PDF_SIDE / area.width().max(area.height())).min(1.0);
        let (w, h) = (area.width() * k, area.height() * k);
````

В `crates/ferrum-render/src/pdf.rs` заменить:

````rust
        let mut painter = Painter::with_raster_limit(0);
        painter.begin_frame();
        painter.draw(&mut backend, &ctx, &[slot]);
        let expired = backend.expired;
        surface.finish();
````

на:

````rust
        let mut painter = Painter::with_raster_limit(0);
        painter.begin_frame();
        painter.draw(&mut backend, &ctx, &slots);
        let expired = backend.expired;
        surface.finish();
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
    PanelEdit, Phase, Shortcut, COMMANDS, MAX_NAME_CHARS,
};
use ferrum_render::decode_png;
use ferrum_scene::shape::COORD_LIMIT;
use serde_bytes::ByteBuf;
````

на:

````rust
    PanelEdit, Phase, Shortcut, COMMANDS, MAX_NAME_CHARS,
};
use ferrum_render::{decode_png, ExportBudget, SvgOptions};
use ferrum_scene::shape::COORD_LIMIT;
use serde_bytes::ByteBuf;
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
use super::Core;
use crate::protocol::{
    Align, Command, ConflictInfo, CoreEvent, DropPosition, Edit, EditPhase, Field, FillDto,
    KeymapEntry, PageInfo, PaintKind, PresetInfo, ScaleModeDto, SelectMode, StrokeDto,
    EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
````

на:

````rust
use super::Core;
use crate::protocol::{
    Align, Command, ConflictInfo, CoreEvent, DropPosition, Edit, EditPhase, ExportFormat, Field,
    FillDto, KeymapEntry, PageInfo, PaintKind, PresetInfo, ScaleModeDto, SelectMode, StrokeDto,
    EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust

/// Имя файла экспорта: без знаков, которые не любят файловые системы.
fn file_name(name: &str, scale: f64) -> String {
    let base: String = name
        .chars()
````

на:

````rust

/// Имя файла экспорта: без знаков, которые не любят файловые системы.
/// Страницы PDF (спека подсистемы 3, раздел 11): у выделения — каждый узел
/// верхнего уровня, без выделения — каждый фрейм верхнего уровня, а если
/// фреймов нет — вся страница одной; по порядку чтения (сверху вниз, слева
/// направо, как «Export frames to PDF» в Figma).
fn pdf_pages(
    scene: &ferrum_scene::Scene,
    selection: &[NodeId],
    all: &[NodeId],
) -> Vec<Vec<NodeId>> {
    let tops: Vec<NodeId> = if selection.is_empty() {
        all.iter()
            .copied()
            .filter(|id| {
                scene
                    .node(*id)
                    .is_some_and(|n| n.props.kind == ferrum_doc::NodeKind::Frame)
            })
            .collect()
    } else {
        ferrum_editor::geometry::top_level(scene, selection)
    };
    if tops.is_empty() {
        return vec![all.to_vec()];
    }
    let mut keyed: Vec<(f64, f64, NodeId)> = tops
        .into_iter()
        .map(|id| {
            let b = scene
                .node(id)
                .and_then(|n| n.subtree_bounds.or(n.world_bounds))
                .unwrap_or_default();
            (b.y0.round(), b.x0, id)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    keyed.into_iter().map(|(_, _, id)| vec![id]).collect()
}

fn file_name(name: &str, scale: f64, format: ExportFormat) -> String {
    let base: String = name
        .chars()
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
    let base = base.trim();
    let base = if base.is_empty() { "Ferrum" } else { base };
    format!("{base}@{scale}x.png")
}

````

на:

````rust
    let base = base.trim();
    let base = if base.is_empty() { "Ferrum" } else { base };
    match format {
        ExportFormat::Png => format!("{base}@{scale}x.png"),
        _ => format!("{base}.{}", format.extension()),
    }
}

````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
                }
            }
            Command::Export { scale } => self.export(scale),
            Command::PasteImage { bytes } => self.paste_image(bytes),
            Command::Bind {
````

на:

````rust
                }
            }
            Command::Export {
                scale,
                format,
                outline_text,
                svg_ids,
            } => {
                let svg = SvgOptions {
                    outline_text: outline_text.unwrap_or(true),
                    ids: svg_ids.unwrap_or(false),
                };
                self.export(scale, format.unwrap_or_default(), svg)
            }
            Command::PasteImage { bytes } => self.paste_image(bytes),
            Command::Bind {
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
    }

    fn export(&mut self, scale: f64) {
        if !EXPORT_SCALES.contains(&scale) {
            self.error("Масштаб экспорта — 1×, 2× или 3×");
````

на:

````rust
    }

    fn export(&mut self, scale: f64, format: ExportFormat, svg: SvgOptions) {
        if !EXPORT_SCALES.contains(&scale) {
            self.error("Масштаб экспорта — 1×, 2× или 3×");
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
                _ => "Выделение".to_string(),
            };
            (selection, name)
        };
        if ids.is_empty() {
````

на:

````rust
                _ => "Выделение".to_string(),
            };
            (selection.clone(), name)
        };
        if ids.is_empty() {
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
        }
        let before = self.images.placeholders_served();
        let provider = self.images.export_provider(&self.editor.document().images);
        let result = self
            .renderer
            .export_png_nodes(self.editor.scene(), &ids, scale, &provider);
        if self.images.placeholders_served() != before {
            self.error("Не удалось экспортировать: часть изображений не читается или не помещается в память");
````

на:

````rust
        }
        let before = self.images.placeholders_served();
        let budget = ExportBudget::new(self.export_clock.as_deref());
        let result = match format {
            ExportFormat::Png => {
                let provider = self.images.export_provider(&self.editor.document().images);
                self.renderer
                    .export_png_nodes(self.editor.scene(), &ids, scale, &provider)
            }
            ExportFormat::Svg => self
                .renderer
                .export_svg(
                    self.editor.scene(),
                    &ids,
                    &self.editor.document().images,
                    svg,
                    &budget,
                )
                .map(String::into_bytes),
            ExportFormat::Pdf => {
                let pages = pdf_pages(self.editor.scene(), &selection, &ids);
                let provider = self.images.export_provider(&self.editor.document().images);
                self.renderer
                    .export_pdf(self.editor.scene(), &pages, &provider, &budget)
            }
        };
        if self.images.placeholders_served() != before {
            self.error("Не удалось экспортировать: часть изображений не читается или не помещается в память");
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
        }
        match result {
            Ok(png) => self.events.push(CoreEvent::Exported {
                name: file_name(&name, scale),
                bytes: ByteBuf::from(png),
            }),
            Err(e) => self.error(format!("Не удалось экспортировать: {e}")),
````

на:

````rust
        }
        match result {
            Ok(bytes) => self.events.push(CoreEvent::Exported {
                name: file_name(&name, scale, format),
                bytes: ByteBuf::from(bytes),
                mime: format.mime().to_string(),
            }),
            Err(e) => self.error(format!("Не удалось экспортировать: {e}")),
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    /// Масштаб (K, подсистема 3, этап 5).
    Scale,
}

````

на:

````rust
    /// Масштаб (K, подсистема 3, этап 5).
    Scale,
}

/// Формат экспорта, как в разделе «Export» Figma (подсистема 3, этап 5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    #[default]
    Png,
    Svg,
    Pdf,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Svg => "svg",
            ExportFormat::Pdf => "pdf",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            ExportFormat::Png => "image/png",
            ExportFormat::Svg => "image/svg+xml",
            ExportFormat::Pdf => "application/pdf",
        }
    }
}

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        zoom: f64,
    },
    /// Экспорт PNG выделения (без выделения — всей страницы): ответ — `Exported`.
    Export {
        scale: f64,
    },
    /// Изображение из системного буфера: встаёт в центр видимой части.
````

на:

````rust
        zoom: f64,
    },
    /// Экспорт выделения (без выделения — всей страницы; PDF — каждого фрейма
    /// страницей): ответ — `Exported`. `scale` — у PNG (1, 2, 3), у SVG и PDF
    /// не читается; `outlineText` («Текст кривыми», по умолчанию да) и `svgIds`
    /// («Атрибут id») — у SVG.
    Export {
        scale: f64,
        #[serde(default)]
        format: Option<ExportFormat>,
        #[serde(default)]
        outline_text: Option<bool>,
        #[serde(default)]
        svg_ids: Option<bool>,
    },
    /// Изображение из системного буфера: встаёт в центр видимой части.
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        presets: Vec<PresetInfo>,
    },
    /// PNG экспорта и имя файла.
    Exported {
        name: String,
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Имя документа изменилось.
````

на:

````rust
        presets: Vec<PresetInfo>,
    },
    /// Файл экспорта: имя, байты и тип (`image/png`, `image/svg+xml`,
    /// `application/pdf`).
    Exported {
        name: String,
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
        mime: String,
    },
    /// Имя документа изменилось.
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test -p ferrum-render -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Expected: PASS: `ferrum-render` и `ferrum-wasm` — 314 passed, 2 ignored (`panels` — 31 тест); wasm собран, `npm run typecheck` без ошибок

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/src/ui/app.tsx crates/ferrum-render/src/lib.rs crates/ferrum-render/src/pdf.rs crates/ferrum-render/tests/pdf.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/fonts.rs crates/ferrum-wasm/tests/panels.rs crates/ferrum-wasm/tests/protocol.rs
git commit -m "feat(wasm): экспорт SVG и PDF в worker'е ядра — протокол, страницы, пределы

Команда Export получила format (png | svg | pdf, по умолчанию PNG — старые
команды те же), outlineText («Текст кривыми», по умолчанию да) и svgIds
(«Атрибут id»); событие Exported — mime. Worker пишет SVG тем же писателем и
PDF через krilla, байты уходят передачей, как у PNG. Страницы PDF: у
выделения — узлы верхнего уровня, без выделения — фреймы верхнего уровня, нет
фреймов — вся страница одной; по порядку чтения. Пределы — ExportBudget
(256 МБ, 30 с по performance.now()). Имена: «Слой.svg», «Страница 1.pdf».
Renderer::export_pdf принимает список узлов на страницу. Интерфейс скачивает
файл с типом из события.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 8: Интерфейс и десктоп: раздел «Экспорт»

Спека 11 и ядро §8.7: в Tauri — родным диалогом сохранения. Здесь — раздел «Экспорт» панели (формат списком, масштаб только у PNG, флажки SVG), сохранение в браузере — скачиванием, в десктопе — командой `export_save` (родной диалог из Rust, байты сырым телом, имя — заголовком ASCII-JSON, атомарная запись); E2E экспорта и модульные тесты моста Tauri.

**Files:**
- Modify: `apps/web/src/ui/{properties.tsx,app.tsx,tauri.ts}`, `apps/web/src/ui/styles/properties.css`, `apps/desktop/src-tauri/src/lib.rs`
- Test: `apps/web/e2e/export.spec.ts` (новый), `apps/web/unit/tauri.test.ts`

**Interfaces:**
- Consumes: `Command::Export { format, outlineText, svgIds }`, `CoreEvent::Exported { name, bytes, mime }`.
- Produces: TS `asciiJson(v)`, `saveExport(name, bytes) -> Promise<boolean>`; `raw()` всегда шлёт ASCII-JSON; Rust `export_save` (команда Tauri), `export_kind(name)`, `export_name(name)`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/e2e/export.spec.ts`:

````ts
// Экспорт SVG и PDF (подсистема 3, этап 5): формат в разделе «Экспорт» панели,
// у SVG — «Текст кривыми» и «Атрибут id», масштаб — только у PNG. SVG разбирается
// браузером как SVG того же размера, PDF — по страницам (`/MediaBox`) и шрифтам.
// Экспорт идёт в worker'е: основной поток только скачивает готовые байты.

import { expect, test, type Download, type Page } from '@playwright/test';
import { openApp } from './helpers';

test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

const selectRow = (page: Page, name: string) =>
  page.getByRole('tree', { name: 'Слои' }).getByRole('treeitem', { name, exact: true }).click();

async function bytesOf(d: Download): Promise<Buffer> {
  return Buffer.concat(await (await d.createReadStream()).toArray());
}

async function exportAs(page: Page, format: 'PNG' | 'SVG' | 'PDF'): Promise<Download> {
  await page.getByRole('combobox', { name: 'Формат экспорта' }).selectOption({ label: format });
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: /Экспортировать/ }).click();
  return download;
}

test('SVG выделенного фрейма: размер, «Текст кривыми» и «Атрибут id» @firefox', async ({ page }) => {
  await openApp(page, 100);
  await selectRow(page, 'Экран 0');
  await page.getByRole('combobox', { name: 'Формат экспорта' }).selectOption({ label: 'SVG' });
  // У SVG масштаба нет, есть свои настройки (как в Figma).
  await expect(page.getByRole('radiogroup', { name: 'Масштаб экспорта' })).toBeHidden();
  await expect(page.getByRole('checkbox', { name: 'Текст кривыми' })).toBeChecked();
  await page.getByRole('checkbox', { name: 'Атрибут id' }).check();
  const file = await exportAs(page, 'SVG');
  expect(file.suggestedFilename()).toBe('Экран 0.svg');
  const text = (await bytesOf(file)).toString('utf8');
  const parsed = await page.evaluate((t) => {
    const doc = new DOMParser().parseFromString(t, 'image/svg+xml');
    const root = doc.documentElement;
    return {
      error: !!doc.querySelector('parsererror'),
      size: [root.getAttribute('width'), root.getAttribute('height')],
      clip: doc.querySelectorAll('clipPath').length,
      ids: doc.querySelectorAll('[id="Экран_0"]').length,
    };
  }, text);
  expect(parsed).toEqual({ error: false, size: ['1440', '1024'], clip: parsed.clip, ids: 1 });
  expect(parsed.clip).toBeGreaterThan(0);
  await expect(page.getByRole('status').filter({ hasText: 'Экспортировано: Экран 0.svg' })).toBeVisible();
});

test('PDF без выделения — фрейм страницей своего размера; с выделением — выделенный', async ({ page }) => {
  await openApp(page, 100);
  let file = await exportAs(page, 'PDF');
  expect(file.suggestedFilename()).toBe('Страница 1.pdf');
  let pdf = (await bytesOf(file)).toString('latin1');
  expect(pdf.startsWith('%PDF-')).toBe(true);
  const pages = (s: string) => s.match(/\/Type\/Page\//g)?.length ?? 0;
  const frames = await page.evaluate(() => window.__ferrum!.ui!.layers.rows.filter((r) => r.depth === 0 && r.kind === 'frame').length);
  expect(pages(pdf)).toBe(frames);
  expect(pdf).toContain('/MediaBox[0 0 1440 1024]');
  await selectRow(page, 'Экран 0');
  file = await exportAs(page, 'PDF');
  expect(file.suggestedFilename()).toBe('Экран 0.pdf');
  pdf = (await bytesOf(file)).toString('latin1');
  expect(pages(pdf)).toBe(1);
  // Назад к PNG — снова масштаб и прежний файл.
  file = await exportAs(page, 'PNG');
  expect(file.suggestedFilename()).toBe('Экран 0@1x.png');
  await expect(page.getByRole('radiogroup', { name: 'Масштаб экспорта' })).toBeVisible();
});
````

В `apps/web/unit/tauri.test.ts` заменить:

````ts

import { expect, test } from '@playwright/test';
import { isTauri, TauriFileStore } from '../src/ui/tauri';

const ID = '0f8e2a06-1111-4222-8333-444455556666';
````

на:

````ts

import { expect, test } from '@playwright/test';
import { asciiJson, isTauri, saveExport, TauriFileStore } from '../src/ui/tauri';

const ID = '0f8e2a06-1111-4222-8333-444455556666';
````

В `apps/web/unit/tauri.test.ts` заменить:

````ts
  await expect(s.touch(ID)).rejects.toThrow(/^Неверный запрос к хранилищу файлов$/);
});
````

на:

````ts
  await expect(s.touch(ID)).rejects.toThrow(/^Неверный запрос к хранилищу файлов$/);
});

test('заголовок x-ferrum — JSON только из ASCII, имя читается обратно', () => {
  const args = { name: 'Экран «Главный» 😀.svg', id: '0a1b' };
  const s = asciiJson(args);
  expect([...s].every((c) => c.charCodeAt(0) < 0x7f)).toBe(true);
  expect(JSON.parse(s)).toEqual(args);
});

test('экспорт на десктопе — export_save: байты сырым телом, имя в заголовке', async () => {
  const calls = fakeIpc({ export_save: () => true });
  const bytes = new Uint8Array([1, 2, 3, 4, 5]).subarray(1, 4);
  expect(await saveExport('Экран.pdf', bytes)).toBe(true);
  expect(calls[0].cmd).toBe('export_save');
  expect(Array.from(calls[0].args as Uint8Array)).toEqual([2, 3, 4]);
  expect(JSON.parse(calls[0].headers!['x-ferrum'])).toEqual({ name: 'Экран.pdf' });
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run unit`
Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run build && npx playwright test e2e/export.spec.ts --project=chromium`
Expected: FAIL — модульные — `SyntaxError: The requested module '../src/ui/tauri' does not provide an export named 'asciiJson'`; E2E (стенд собран) — 2 failed: оба теста `export.spec.ts` — `locator.selectOption: Test timeout of 120000ms exceeded`, ожидание `getByRole('combobox', { name: 'Формат экспорта' })` (списка формата в панели нет)

- [ ] **Step 3: Реализация**

В `apps/desktop/src-tauri/src/lib.rs` заменить:

````rust
    session: Option<u64>,
    up_to: Option<u64>,
}

````

на:

````rust
    session: Option<u64>,
    up_to: Option<u64>,
}

#[derive(Deserialize)]
struct ExportArgs {
    name: String,
}

````

В `apps/desktop/src-tauri/src/lib.rs` заменить:

````rust
}

#[tauri::command(async)]
fn settings_get(desk: State<'_, Desk>) -> Res<Map<String, Value>> {
````

на:

````rust
}

/// Подпись фильтра и расширение файла экспорта по имени (`.png`, `.svg`, `.pdf`).
fn export_kind(name: &str) -> Option<(&'static str, &'static str)> {
    let (_, ext) = name.rsplit_once('.')?;
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some(("Изображение PNG", "png")),
        "svg" => Some(("Изображение SVG", "svg")),
        "pdf" => Some(("Документ PDF", "pdf")),
        _ => None,
    }
}

/// Имя файла экспорта для диалога: без разделителей пути и управляющих
/// символов, не длиннее 255 символов.
fn export_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') {
                '_'
            } else {
                c
            }
        })
        .take(255)
        .collect()
}

/// Экспорт PNG, SVG и PDF (подсистема 3, этап 5): родной диалог «Сохранить» с
/// именем из интерфейса и фильтром по типу; байты — сырым телом запроса, запись
/// атомарная. `false` — отказ в диалоге. Путь интерфейс не видит.
#[tauri::command(async)]
fn export_save(request: Request<'_>, app: AppHandle, window: WebviewWindow) -> Res<bool> {
    let (bytes, a): (&[u8], ExportArgs) = raw(&request)?;
    let name = export_name(&a.name);
    let (label, ext) = export_kind(&name).ok_or_else(|| BAD.to_string())?;
    let Some(picked) = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_file_name(&name)
        .add_filter(label, &[ext])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = picked.into_path().map_err(|_| NOT_A_PATH.to_string())?;
    ferrum_desktop_store::atomic_write(&path, bytes)
        .map_err(|e| format!("Не удалось сохранить файл: {}", io_error(&e)))?;
    Ok(true)
}

#[tauri::command(async)]
fn settings_get(desk: State<'_, Desk>) -> Res<Map<String, Value>> {
````

В `apps/desktop/src-tauri/src/lib.rs` заменить:

````rust
            files_save_dialog,
            files_copy_dialog,
            settings_get,
            settings_set,
````

на:

````rust
            files_save_dialog,
            files_copy_dialog,
            export_save,
            settings_get,
            settings_set,
````

В `apps/desktop/src-tauri/src/lib.rs` заменить:

````rust
        .expect("Ferrum не запустился");
}
````

на:

````rust
        .expect("Ferrum не запустился");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тип экспорта — только по расширению PNG, SVG и PDF; имя без путей.
    #[test]
    fn export_names_are_checked_and_cleaned() {
        assert_eq!(export_kind("Экран.svg"), Some(("Изображение SVG", "svg")));
        assert_eq!(export_kind("a@2x.PNG"), Some(("Изображение PNG", "png")));
        assert_eq!(export_kind("Отчёт.pdf").map(|k| k.1), Some("pdf"));
        assert_eq!(export_kind("файл.ferrum"), None);
        assert_eq!(export_kind("без расширения"), None);
        assert_eq!(export_name("../a/b\\c:d\u{0}.svg"), ".._a_b_c_d.svg");
        assert_eq!(export_name(&"я".repeat(400)).chars().count(), 255);
    }
}
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { importFromDisk, type FileMeta, type FileStore } from './file-store';
import { systemFonts } from './system-fonts';
import type { Backend } from '../wasm/ferrum_wasm';
import { restartText, type Restart } from './restart';
````

на:

````tsx
import { importFromDisk, type FileMeta, type FileStore } from './file-store';
import { systemFonts } from './system-fonts';
import { isTauri, saveExport } from './tauri';
import type { Backend } from '../wasm/ferrum_wasm';
import { restartText, type Restart } from './restart';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
          }
          if (e.type === 'exported') {
            downloadBytes(e.name, e.bytes, e.mime);
            store.toast(`Экспортировано: ${e.name}`);
          } else if (e.type === 'saved') {
            // «Сохранить копию на диск…»: байты готовит worker и передаёт без копии.
````

на:

````tsx
          }
          if (e.type === 'exported') {
            // Десктоп — родной диалог «Сохранить» (путь выбирает Rust), браузер — загрузка.
            if (isTauri()) {
              const name = e.name;
              saveExport(name, e.bytes)
                .then((saved) => saved && store.toast(`Экспортировано: ${name}`))
                .catch((err: unknown) => store.toast(err instanceof Error ? err.message : String(err), 'error'));
            } else {
              downloadBytes(e.name, e.bytes, e.mime);
              store.toast(`Экспортировано: ${e.name}`);
            }
          } else if (e.type === 'saved') {
            // «Сохранить копию на диск…»: байты готовит worker и передаёт без копии.
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx

import { createEffect, createSignal, For, Show } from 'solid-js';
import type { Edit, EditPhase, Field, Num } from '../wasm/ferrum_wasm';
import { Section } from './chrome';
import { useEditor } from './context';
````

на:

````tsx

import { createEffect, createSignal, For, Show } from 'solid-js';
import type { Edit, EditPhase, ExportFormat, Field, Num } from '../wasm/ferrum_wasm';
import { Section } from './chrome';
import { useEditor } from './context';
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
const SCALES = [1, 2, 3] as const;

function ExportSection() {
  const ed = useEditor();
  const [scale, setScale] = createSignal<number>(1);
  let group!: HTMLDivElement;
  // Группа переключателей: одна точка Tab (выбранный), стрелки и Home/End выбирают
````

на:

````tsx
const SCALES = [1, 2, 3] as const;

/** Форматы экспорта, как в разделе «Export» Figma (без JPG: его нет в ядре). */
const FORMATS: { id: ExportFormat; label: string }[] = [
  { id: 'png', label: 'PNG' },
  { id: 'svg', label: 'SVG' },
  { id: 'pdf', label: 'PDF' },
];

function ExportSection() {
  const ed = useEditor();
  const [scale, setScale] = createSignal<number>(1);
  const [format, setFormat] = createSignal<ExportFormat>('png');
  // Настройки SVG, как в Figma: «Текст кривыми» включён, «Атрибут id» — нет.
  const [outlineText, setOutlineText] = createSignal(true);
  const [svgIds, setSvgIds] = createSignal(false);
  let group!: HTMLDivElement;
  // Группа переключателей: одна точка Tab (выбранный), стрелки и Home/End выбирают
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
    return s.count === 1 ? `«${s.name ?? ''}»` : 'выделение';
  };
  return (
    <Section title="Экспорт">
      <div class="fe-export">
        <div ref={group} class="fe-segmented" role="radiogroup" aria-label="Масштаб экспорта" onKeyDown={onKey}>
          <For each={SCALES}>
            {(k) => (
              <button
                role="radio"
                aria-checked={scale() === k}
                tabindex={scale() === k ? 0 : -1}
                class="fe-segmented__item t-num"
                onClick={() => setScale(k)}
              >
                {k}×
              </button>
            )}
          </For>
        </div>
        <span class="fe-format t-num">PNG</span>
      </div>
      <button class="fe-btn fe-btn--soft fe-btn--wide" onClick={() => ed.send({ type: 'export', scale: scale() })}>
        <Icon name="download" />
        <span class="fe-ellipsis">Экспортировать {target()}</span>
````

на:

````tsx
    return s.count === 1 ? `«${s.name ?? ''}»` : 'выделение';
  };
  // Масштаб — у PNG; SVG и PDF векторные, в единицах документа (1 px = 1 pt).
  const raster = () => format() === 'png';
  const run = () =>
    ed.send({
      type: 'export',
      scale: raster() ? scale() : 1,
      format: format(),
      outlineText: format() === 'svg' ? outlineText() : undefined,
      svgIds: format() === 'svg' ? svgIds() : undefined,
    });
  return (
    <Section title="Экспорт">
      <div class="fe-export">
        <Show when={raster()}>
          <div ref={group} class="fe-segmented" role="radiogroup" aria-label="Масштаб экспорта" onKeyDown={onKey}>
            <For each={SCALES}>
              {(k) => (
                <button
                  role="radio"
                  aria-checked={scale() === k}
                  tabindex={scale() === k ? 0 : -1}
                  class="fe-segmented__item t-num"
                  onClick={() => setScale(k)}
                >
                  {k}×
                </button>
              )}
            </For>
          </div>
        </Show>
        <select
          class="fe-format t-num"
          aria-label="Формат экспорта"
          value={format()}
          onChange={(e) => setFormat(e.currentTarget.value as ExportFormat)}
        >
          <For each={FORMATS}>{(f) => <option value={f.id}>{f.label}</option>}</For>
        </select>
      </div>
      <Show when={format() === 'svg'}>
        <label class="fe-check">
          <input type="checkbox" checked={outlineText()} onChange={(e) => setOutlineText(e.currentTarget.checked)} />
          <span>Текст кривыми</span>
        </label>
        <label class="fe-check">
          <input type="checkbox" checked={svgIds()} onChange={(e) => setSvgIds(e.currentTarget.checked)} />
          <span>Атрибут id</span>
        </label>
      </Show>
      <button class="fe-btn fe-btn--soft fe-btn--wide" onClick={run}>
        <Icon name="download" />
        <span class="fe-ellipsis">Экспортировать {target()}</span>
````

В `apps/web/src/ui/styles/properties.css` заменить:

````css
  height: var(--row-height);
  padding: 0 var(--space-2);
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
  color: var(--ink);
}
.fe-scale {
````

на:

````css
  height: var(--row-height);
  padding: 0 var(--space-2);
  border: 0;
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
  color: var(--ink);
  font: inherit;
  cursor: pointer;
}
.fe-format:hover {
  box-shadow: inset 0 0 0 1px var(--line-control);
}
.fe-scale {
````

В `apps/web/src/ui/tauri.ts` заменить:

````ts
}

/** Сырое тело запроса и параметры заголовком `x-ferrum` (ASCII: UUID и числа). */
function raw<T>(cmd: string, bytes: ArrayBuffer, args: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, new Uint8Array(bytes), { 'x-ferrum': JSON.stringify(args) });
}

````

на:

````ts
}

/** JSON только из ASCII: не-ASCII символы — `\uXXXX` (заголовок HTTP — ASCII). */
export function asciiJson(v: unknown): string {
  return JSON.stringify(v).replace(/[\u007f-\uffff]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`);
}

/** Сырое тело запроса и параметры заголовком `x-ferrum` (ASCII: UUID, числа, имена
 * с `\uXXXX`). */
function raw<T>(cmd: string, bytes: ArrayBuffer, args: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, new Uint8Array(bytes), { 'x-ferrum': asciiJson(args) });
}

/** Экспорт (PNG, SVG, PDF) родным диалогом «Сохранить» (подсистема 3, этап 5): байты —
 * сырым телом, имя — заголовком; `false` — пользователь отказался. */
export function saveExport(name: string, bytes: Uint8Array): Promise<boolean> {
  const buf = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
  return raw<boolean>('export_save', buf, { name });
}

````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npx playwright test e2e/export.spec.ts e2e/app.spec.ts e2e/properties.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npx playwright test e2e/export.spec.ts --project=firefox`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR=$PWD/../../../target CARGO_INCREMENTAL=0 && cargo check --offline`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR=$PWD/../../../target CARGO_INCREMENTAL=0 && cargo clippy --offline --all-targets -- -D warnings`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR=$PWD/../../../target CARGO_INCREMENTAL=0 && cargo test --offline --lib export_names`

Expected: модульные — 136 passed, сборка — ок; E2E `export`, `app`, `properties` в Chromium — 34 passed; `export` в Firefox — 1 passed; десктоп: `cargo check` и `clippy` без замечаний, `export_names` — 1 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/desktop/src-tauri/src/lib.rs apps/web/e2e/export.spec.ts apps/web/src/ui/app.tsx apps/web/src/ui/properties.tsx apps/web/src/ui/styles/properties.css apps/web/src/ui/tauri.ts apps/web/unit/tauri.test.ts
git commit -m "feat(web,desktop): раздел «Экспорт» — PNG, SVG, PDF; в десктопе — родной диалог

Раздел «Экспорт» панели: формат (PNG, SVG, PDF — список, как в Figma), масштаб
1×/2×/3× — только у PNG, у SVG — «Текст кривыми» (по умолчанию да) и «Атрибут
id». Десктоп сохраняет экспорт родным диалогом «Сохранить» из Rust (команда
export_save: байты сырым телом, имя в заголовке ASCII-JSON, фильтр по типу,
атомарная запись, путь интерфейс не видит), браузер — загрузкой. Заголовок
x-ferrum теперь всегда ASCII (asciiJson: не-ASCII — \uXXXX). E2E export.spec.ts
(SVG разбирается браузером, PDF — страницы и размеры), модульные — asciiJson и
saveExport, desktop — имена и типы экспорта.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 9: Замеры, спека и финальная проверка

Спека 12: замеры с целями. Здесь — `render_report` (SVG 10 000 узлов, PDF со 100 текстами и вектором в 100 000 точек, PNG фрейма @3x справочно), `editor_report` (K на 10 000 узлах: кадр жеста, первое движение, запись, поле «Масштаб», по шагу); допуск упрощения больших векторов в PDF (`PDF_DETAIL_SCALE = 16`: точный контур 100 000 точек стоил 8,8 МБ и 1,4 с); спека получает «Сделано (этап 5)» в разделах 8, 11, 12, ядро §8.7 и «Отложено». Шаг «Проверить» — все ворота этапа.

**Files:**
- Modify: `crates/ferrum-render/examples/render_report.rs`, `crates/ferrum-editor/examples/editor_report.rs`, `crates/ferrum-render/src/pdf.rs`, `docs/superpowers/specs/{2026-10-29-ferrum-vectors-text-design,2026-10-23-ferrum-core-design}.md`

- [ ] **Step 1: Реализация**

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
    );
    boolean_child_drag(&mut report);

    println!("| Метрика | Значение | Цель | Итог |");
````

на:

````rust
    );
    boolean_child_drag(&mut report);
    scale_selection(&mut report);

    println!("| Метрика | Значение | Цель | Итог |");
````

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
/// перекрывающихся кругов в «Объединении», перетаскивание одного круга — кадр
/// жеста (превью сцены пересчитывает результат) и запись одним шагом.
fn boolean_child_drag(report: &mut Report) {
    let mut ed =
````

на:

````rust
/// перекрывающихся кругов в «Объединении», перетаскивание одного круга — кадр
/// жеста (превью сцены пересчитывает результат) и запись одним шагом.
/// Инструмент «Масштаб» (подсистема 3, этап 5) на большом выделении: 100 фреймов
/// по 100 узлов (10 000 узлов в поддереве, у фигур — обводки) — кадр жеста ручкой
/// (превью всего поддерева со свойствами), запись жеста и поле «Масштаб» панели —
/// одним шагом.
fn scale_selection(report: &mut Report) {
    use ferrum_editor::scale::Anchor;
    use ferrum_editor::{PanelEdit, Phase};
    const N: usize = 10_000;
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.transact(page, vec![], |p| fill(p, N).map(|_| vec![]))
        .unwrap();
    let mut ed = Editor::new(doc, Some(page), VIEWPORT, 1.0, Platform::Mac).unwrap();
    ed.execute(CommandId::SelectAll);
    ed.execute(CommandId::ToolScale);
    let sel = ed.selection().to_vec();
    let bounds = sel
        .iter()
        .filter_map(|id| ed.scene().node(*id).and_then(|n| n.world_bounds))
        .reduce(|a, b| a.union(b))
        .unwrap();
    let mut cam = Camera::new(VIEWPORT, 1.0);
    cam.fit(bounds, 48.0);
    ed.set_camera(cam);
    ed.take_events();
    let steps = ed.step_count();
    // Жест ручкой: правый нижний угол рамки выделения.
    let mut clock = Clock(0.0);
    let corner = screen(&ed, bounds.x1, bounds.y1);
    ed.handle(Input::PointerDown(clock.at(corner)));
    let kick = Point::new(corner.x + 4.0, corner.y + 4.0);
    let first = timed(&mut ed, Input::PointerMove(clock.at(kick)));
    let mut times = Vec::new();
    let mut end = corner;
    for i in 1..=MOVES {
        end = Point::new(
            corner.x + 4.0 + 3.0 * i as f64,
            corner.y + 4.0 + 2.0 * i as f64,
        );
        times.push(timed(&mut ed, Input::PointerMove(clock.at(end))));
    }
    report.gate_median(
        "Масштаб (K): 10 000 узлов — кадр жеста ручкой (превью поддерева со свойствами)",
        &times,
        50.0,
    );
    report.info(
        "Масштаб (K): 10 000 узлов — первое движение (базы поддерева)",
        ms(first),
    );
    let t = Instant::now();
    ed.handle(Input::PointerUp(clock.at(end)));
    let commit = t.elapsed();
    ed.take_events();
    report.gate(
        "Масштаб (K): 10 000 узлов — запись жеста (отпускание)",
        commit,
        500.0,
    );
    // Поле «Масштаб» панели: превью и запись.
    let preview: Vec<Duration> = (1..=10)
        .map(|i| {
            let t = Instant::now();
            ed.edit_selection(
                &PanelEdit::Scale {
                    factor: 1.0 + 0.05 * i as f64,
                    anchor: Anchor::TOP_LEFT,
                },
                Phase::Preview,
            );
            let d = t.elapsed();
            ed.take_events();
            d
        })
        .collect();
    report.gate_median(
        "Масштаб (K): 10 000 узлов — превью поля «Масштаб»",
        &preview,
        50.0,
    );
    let t = Instant::now();
    ed.edit_selection(
        &PanelEdit::Scale {
            factor: 0.5,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    let panel = t.elapsed();
    ed.take_events();
    report.gate(
        "Масштаб (K): 10 000 узлов — запись поля «Масштаб»",
        panel,
        500.0,
    );
    report.check(
        "Масштаб (K): жест и поле — по шагу",
        ed.step_count() == steps + 2,
        format!("{} шага", ed.step_count() - steps),
        "2 шага",
    );
}

fn boolean_child_drag(report: &mut Report) {
    let mut ed =
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
}

fn main() {
    let mut report = Report(Vec::new());
````

на:

````rust
}

/// Экспорт SVG и PDF (подсистема 3, этап 5): SVG страницы из 10 000 узлов
/// (фреймы по 100: фрейм + 99 фигур) и PDF страницы с текстом и вектором в
/// 100 000 точек — в worker'е, без замирания интерфейса; цель — секунда.
fn export_section(report: &mut Report) {
    use ferrum_render::{ExportBudget, SvgOptions};
    use std::sync::Arc;
    const SVG_NODES: usize = 10_000;
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.transact(page, vec![], |p| fill(p, SVG_NODES).map(|_| vec![]))
        .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    let roots: Vec<NodeId> = scene.roots().iter().map(|s| scene.at(*s).id).collect();
    let mut renderer = Renderer::new();
    let runs: Vec<Duration> = (0..3)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(
                renderer
                    .export_svg(
                        &scene,
                        &roots,
                        &doc.images,
                        SvgOptions::default(),
                        &ExportBudget::unlimited(),
                    )
                    .unwrap(),
            );
            t.elapsed()
        })
        .collect();
    let svg = renderer
        .export_svg(
            &scene,
            &roots,
            &doc.images,
            SvgOptions::default(),
            &ExportBudget::unlimited(),
        )
        .unwrap();
    report.gate(
        "Экспорт SVG страницы из 10 000 узлов (медиана)",
        median(runs),
        1000.0,
        "≤ 1000 мс",
    );
    report.info(
        "Экспорт SVG 10 000 узлов: размер файла",
        format!("{} КБ", svg.len() / 1024),
    );

    // PDF: фрейм с сотней текстов и вектором в 100 000 точек — страница.
    let fonts = inter_fonts();
    let net = ferrum_doc::fixture::wavy_network(100_000);
    let b = ferrum_scene::vector::network_bounds(&net).unwrap();
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let frame = doc
        .transact(page, vec![], |tx| {
            let frame = tx.create_node(None, None, NodeKind::Frame)?;
            tx.set_props(
                frame,
                &PropsPatch {
                    w: Some(b.width().max(1440.0)),
                    h: Some(b.height() + 2000.0),
                    ..Default::default()
                },
            )?;
            let v = tx.create_node(Some(frame), None, NodeKind::Vector)?;
            tx.set_props(
                v,
                &PropsPatch {
                    w: Some(b.width()),
                    h: Some(b.height()),
                    fills: Some(vec![ferrum_doc::Fill::solid(ferrum_doc::Color::SHAPE_GRAY)]),
                    vector: Some(Arc::new(net.clone())),
                    ..Default::default()
                },
            )?;
            for i in 0..100 {
                let t = tx.create_node(Some(frame), None, NodeKind::Text)?;
                tx.set_props(
                    t,
                    &PropsPatch {
                        y: Some(b.height() + 20.0 * i as f64),
                        w: Some(1400.0),
                        h: Some(20.0),
                        text: Some(ferrum_doc::TextProps {
                            sizing: ferrum_doc::TextSizing::Fixed,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                )?;
                tx.set_text_content(
                    t,
                    &ferrum_doc::TextContent::plain(&format!(
                        "Строка {i}: съешь же ещё этих мягких французских булок, да выпей чаю"
                    )),
                )?;
            }
            Ok(vec![frame])
        })
        .unwrap()[0];
    let scene = Scene::from_page(doc.page(page).unwrap());
    let mut renderer = Renderer::new();
    renderer.set_fonts(fonts);
    let runs: Vec<Duration> = (0..3)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(
                renderer
                    .export_pdf(
                        &scene,
                        &[vec![frame]],
                        &NoImages,
                        &ExportBudget::unlimited(),
                    )
                    .unwrap(),
            );
            t.elapsed()
        })
        .collect();
    let pdf = renderer
        .export_pdf(
            &scene,
            &[vec![frame]],
            &NoImages,
            &ExportBudget::unlimited(),
        )
        .unwrap();
    report.gate(
        "Экспорт PDF страницы: 100 текстов и вектор 100 000 точек (медиана)",
        median(runs),
        1000.0,
        "≤ 1000 мс",
    );
    report.info(
        "Экспорт PDF той же страницы: размер файла",
        format!("{} КБ", pdf.len() / 1024),
    );
}

fn main() {
    let mut report = Report(Vec::new());
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
    }

    text_section(&mut report);
    vector_section(&mut report);
````

на:

````rust
    }

    export_section(&mut report);
    text_section(&mut report);
    vector_section(&mut report);
````

В `crates/ferrum-render/src/pdf.rs` заменить:

````rust
/// Наибольшая сторона страницы PDF, pt (предел формата без `UserUnit`).
pub const MAX_PDF_SIDE: f64 = 14_400.0;
/// Плиток картинки «Замостить» на одну заливку не больше (дальше — без плиток).
const MAX_TILES: usize = 4096;
````

на:

````rust
/// Наибольшая сторона страницы PDF, pt (предел формата без `UserUnit`).
pub const MAX_PDF_SIDE: f64 = 14_400.0;
/// Масштаб, под который строятся контуры больших векторов в PDF (от 4096
/// элементов): упрощение до четверти пикселя при ×16 — не больше 1/64 pt, это
/// пиксель при увеличении 6400 % в просмотрщике. Точный контур сети в 100 000
/// точек — 8,8 МБ и 1,4 с, с этим допуском — 0,7 МБ и 0,1 с (`render_report`).
const PDF_DETAIL_SCALE: f64 = 16.0;
/// Плиток картинки «Замостить» на одну заливку не больше (дальше — без плиток).
const MAX_TILES: usize = 4096;
````

В `crates/ferrum-render/src/pdf.rs` заменить:

````rust
            fonts,
            transform: Affine::scale(k) * Affine::translate(-area.origin().to_vec2()),
            scale: k,
            cull: None,
            lod: false,
````

на:

````rust
            fonts,
            transform: Affine::scale(k) * Affine::translate(-area.origin().to_vec2()),
            // PDF приближают в просмотрщике: большие векторы упрощаются под ×16.
            scale: k * PDF_DETAIL_SCALE,
            cull: None,
            lod: false,
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
### 8.7 Экспорт

PNG фрейма или выделения в @1x/@2x/@3x тем же рендером (`vello_cpu`); размер пиксель в пиксель (фрейм 393×852 @3x → 1179×2556), сторона не больше 16384 px. Рисуется только поддерево узла: прозрачность и обрезка предков не применяются.

## 9. Интерфейс (apps/web)
````

на:

````markdown
### 8.7 Экспорт

PNG фрейма или выделения в @1x/@2x/@3x тем же рендером (`vello_cpu`); размер пиксель в пиксель (фрейм 393×852 @3x → 1179×2556), сторона не больше 16384 px. Рисуется только поддерево узла: прозрачность и обрезка предков не применяются. SVG и PDF — подсистема 3, этап 5 (спека `2026-10-29-ferrum-vectors-text-design.md`, раздел 11); в десктопе любой экспорт сохраняется родным диалогом.

## 9. Интерфейс (apps/web)
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
- **Tauri (часть 2, `apps/desktop`):** хранилище на Rust (`ferrum-desktop-store`): библиотека `library.json` в каталоге данных, новые файлы — в `~/Documents/Ferrum`, открытые с диска остаются на месте; запись атомарная (временный файл → `fsync` → переименование → `fsync` каталога); чужая запись — по поколению и времени изменения файла; журнал — файлы `journal/<id>/<сеанс>-<пачка>.bin`, пачки до номера снимка стираются не в одной транзакции с файлом, а после атомарной замены файла и записи списка (сбой между ними оставит пачку, уже вошедшую в файл, — её повтор безвреден: Loro пропускает известные обновления); превью; настройки в `settings.json`. Открытие читает метаданные, байты файла, сеанс и журнал одной командой под одной блокировкой библиотеки (`files_read_open`), байты забираются по частям сырыми ответами IPC; чтение, запись и пачки журнала окно шлёт по очереди — чтение после перезапуска ядра не встаёт посреди записи прежнего редактора. Ошибки ОС — по-русски («недостаточно места на диске», «нет доступа к файлу или папке», «диск доступен только для чтения», «файл или папка не найдены», прочие — «ошибка диска: » и текст ОС). Приложение одно на библиотеку (`library.lock`): второй экземпляр показывает родным окном «Ferrum уже запущен: библиотека файлов открыта в другом экземпляре» и выходит. Команды IPC, берущие библиотеку или диск, асинхронные (не в главном потоке окна), страница под политикой CSP (`default-src 'self'`, `font-src 'self' data:`, `connect-src 'self' ipc:`), окну выданы пустые разрешения ядра Tauri (`permissions: []`). Родные диалоги «Открыть с диска…», «Сохранить на диск…», «Сохранить копию на диск…» — из Rust (пути интерфейс не видит). В WKWebView страница не изолирована (`crossOriginIsolated === false`) — ядро работает на транспорте postMessage.
- **Сохранение (часть 1, без журнала; заменено частью 2):** полный снимок в хранилище по Ctrl/Cmd+S, через 2 с после последней правки, при уходе на экран файлов или к другому файлу (уход ждёт записи). Ошибка записи (квота и т. п.) — одно уведомление на русском, пока запись снова не удастся; документ остаётся открытым и изменённым, уход отменяется. `pagehide` — попытка сохранить без гарантии: вкладка может закрыться раньше, чем worker допишет. Цена полного снимка: на 100 000 узлах каждое отложенное сохранение держит worker ядра ≈ 0,45 с (холст замирает после паузы в правках) — уйдёт с журналом Loro.
- **Отложено после этапа 6:** быстрое открытие из кэша сцены (первый кадр до раскодирования Loro, правки — после фоновой загрузки): редактор держит загруженную страницу во всех путях (выделение, наведение, панели), а открытие 100 000 узлов уже укладывается в цель (≈ 0,95 с в worker'е при цели 2 с); обновление зеркала страницы по событиям Loro вместо полной перестройки при восстановлении (подсистема 4); SharedArrayBuffer в десктопе (WKWebView не даёт изоляции); вид после перезапуска редактора (страница, камера, выделение); экспорт PNG в десктопе родным диалогом; подпись, нотаризация, сборки Windows и Linux, автообновление и меню приложения десктопа; E2E в Safari/WebKit и Firefox в CI. Видимое пользователю из мелочей, оставленных на потом: без Web Locks сжатие журнала в новой вкладке стирает поздние пачки старой; перезапуск ядра на один круг IndexedDB может мигнуть плашкой остановки до уведомления; не ждём возврата контекста WebGL2 дольше двух попыток — медленное восстановление уходит на CPU; щелчок в панели в десятки миллисекунд между отказом GPU и остановкой в файл не попадает (уход спросит); журнал больше 512 МБ или повреждённый остаётся в хранилище и предупреждение повторяется при каждом открытии; пачка журнала только с картинками при восстановлении не применяется; файл раскладки с нестроковой платформой читается как без поля — без предупреждения; отвергнутое запасной записью значение настройки может вернуться при следующем запуске, две вкладки пишут один и тот же ключ — побеждает последняя; сборки Windows и Linux десктопа не проверены, «Удалить навсегда» в десктопе удаляет свой файл сразу, не в Корзину; закрытие окна десктопа не перехвачено (нет `CloseRequested` с записью перед выходом) — выход полагается на `pagehide` в WKWebView, как вкладка браузера: теряется до 2 с правок.

## 11. Обработка ошибок
````

на:

````markdown
- **Tauri (часть 2, `apps/desktop`):** хранилище на Rust (`ferrum-desktop-store`): библиотека `library.json` в каталоге данных, новые файлы — в `~/Documents/Ferrum`, открытые с диска остаются на месте; запись атомарная (временный файл → `fsync` → переименование → `fsync` каталога); чужая запись — по поколению и времени изменения файла; журнал — файлы `journal/<id>/<сеанс>-<пачка>.bin`, пачки до номера снимка стираются не в одной транзакции с файлом, а после атомарной замены файла и записи списка (сбой между ними оставит пачку, уже вошедшую в файл, — её повтор безвреден: Loro пропускает известные обновления); превью; настройки в `settings.json`. Открытие читает метаданные, байты файла, сеанс и журнал одной командой под одной блокировкой библиотеки (`files_read_open`), байты забираются по частям сырыми ответами IPC; чтение, запись и пачки журнала окно шлёт по очереди — чтение после перезапуска ядра не встаёт посреди записи прежнего редактора. Ошибки ОС — по-русски («недостаточно места на диске», «нет доступа к файлу или папке», «диск доступен только для чтения», «файл или папка не найдены», прочие — «ошибка диска: » и текст ОС). Приложение одно на библиотеку (`library.lock`): второй экземпляр показывает родным окном «Ferrum уже запущен: библиотека файлов открыта в другом экземпляре» и выходит. Команды IPC, берущие библиотеку или диск, асинхронные (не в главном потоке окна), страница под политикой CSP (`default-src 'self'`, `font-src 'self' data:`, `connect-src 'self' ipc:`), окну выданы пустые разрешения ядра Tauri (`permissions: []`). Родные диалоги «Открыть с диска…», «Сохранить на диск…», «Сохранить копию на диск…» — из Rust (пути интерфейс не видит). В WKWebView страница не изолирована (`crossOriginIsolated === false`) — ядро работает на транспорте postMessage.
- **Сохранение (часть 1, без журнала; заменено частью 2):** полный снимок в хранилище по Ctrl/Cmd+S, через 2 с после последней правки, при уходе на экран файлов или к другому файлу (уход ждёт записи). Ошибка записи (квота и т. п.) — одно уведомление на русском, пока запись снова не удастся; документ остаётся открытым и изменённым, уход отменяется. `pagehide` — попытка сохранить без гарантии: вкладка может закрыться раньше, чем worker допишет. Цена полного снимка: на 100 000 узлах каждое отложенное сохранение держит worker ядра ≈ 0,45 с (холст замирает после паузы в правках) — уйдёт с журналом Loro.
- **Отложено после этапа 6:** быстрое открытие из кэша сцены (первый кадр до раскодирования Loro, правки — после фоновой загрузки): редактор держит загруженную страницу во всех путях (выделение, наведение, панели), а открытие 100 000 узлов уже укладывается в цель (≈ 0,95 с в worker'е при цели 2 с); обновление зеркала страницы по событиям Loro вместо полной перестройки при восстановлении (подсистема 4); SharedArrayBuffer в десктопе (WKWebView не даёт изоляции); вид после перезапуска редактора (страница, камера, выделение); подпись, нотаризация, сборки Windows и Linux, автообновление и меню приложения десктопа; E2E в Safari/WebKit и Firefox в CI. Видимое пользователю из мелочей, оставленных на потом: без Web Locks сжатие журнала в новой вкладке стирает поздние пачки старой; перезапуск ядра на один круг IndexedDB может мигнуть плашкой остановки до уведомления; не ждём возврата контекста WebGL2 дольше двух попыток — медленное восстановление уходит на CPU; щелчок в панели в десятки миллисекунд между отказом GPU и остановкой в файл не попадает (уход спросит); журнал больше 512 МБ или повреждённый остаётся в хранилище и предупреждение повторяется при каждом открытии; пачка журнала только с картинками при восстановлении не применяется; файл раскладки с нестроковой платформой читается как без поля — без предупреждения; отвергнутое запасной записью значение настройки может вернуться при следующем запуске, две вкладки пишут один и тот же ключ — побеждает последняя; сборки Windows и Linux десктопа не проверены, «Удалить навсегда» в десктопе удаляет свой файл сразу, не в Корзину; закрытие окна десктопа не перехвачено (нет `CloseRequested` с записью перед выходом) — выход полагается на `pagehide` в WKWebView, как вкладка браузера: теряется до 2 с правок.

## 11. Обработка ошибок
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **Сделано (этап 3):** инструменты `pen` и `arrow` (команды раскладки `tool.pen` — P, `tool.arrow` — Shift+L, `object.flatten` — Ctrl/Cmd+E), курсоры `pen` и `bucket`, тип строки слоя `vector`; правка панели `strokeStyle { capStart?, capEnd?, join?, miterLimit? }` (предел вне 1…100 — ошибка, панель зажимает его теми же числами из `limits.ts`), сводка `strokeStyle { ends, corners, capStart?, capEnd?, join?, miterLimit }` (`undefined` — «Смешанное»). Сеть в протокол не идёт: правка вектора идёт вводом холста.
- **Сделано (этап 4):** тип строки слоя `boolean` с полем `booleanOp` (иконка операции), сводка выделения — `booleanOp` (у выделения только из булевых групп с одной операцией; список тулбара отмечает её); команды раскладки `object.boolean_union`, `object.boolean_subtract`, `object.boolean_intersect`, `object.boolean_exclude` (Alt+Shift+U/S/I/E), `object.outline_stroke` (Ctrl/Cmd+Alt+O) идут обычным `Execute`; подсказка кнопки тулбара — с сочетанием из действующей раскладки. Геометрия в протокол не идёт.
- **Сделано (этап 1):** `FontsAvailable { source: bundled | system, families: [{ family, faces: [{ style, weight?, italic?, stretch?, file, index? }] }] }` (у системных Chrome вес и курсив ядро угадывает по имени начертания), `LoadFont { family, file, bytes? }` — по одному файлу (без байтов — файл не получить), `FontRequest { family, source, files }` (вшитые выполняет worker ядра, системные — основной поток), `MissingFonts { list: [{ family, style, nodes }] }`, `ReplaceMissingFont { from: { family, style }, to: { family, style } }`; сверх списка — события `FontCatalog { families: [{ family, source, styles }] }` (выбор замены) и `Log { message }` (журнал ошибок), команды `DemoText { texts }` (замер) и `TestCreateText` (только тестовый режим страницы). Правки, которым нужны ещё не загруженные шрифты, ядро держит в очереди и выполняет по приходу байтов.

````

на:

````markdown
- **Сделано (этап 3):** инструменты `pen` и `arrow` (команды раскладки `tool.pen` — P, `tool.arrow` — Shift+L, `object.flatten` — Ctrl/Cmd+E), курсоры `pen` и `bucket`, тип строки слоя `vector`; правка панели `strokeStyle { capStart?, capEnd?, join?, miterLimit? }` (предел вне 1…100 — ошибка, панель зажимает его теми же числами из `limits.ts`), сводка `strokeStyle { ends, corners, capStart?, capEnd?, join?, miterLimit }` (`undefined` — «Смешанное»). Сеть в протокол не идёт: правка вектора идёт вводом холста.
- **Сделано (этап 4):** тип строки слоя `boolean` с полем `booleanOp` (иконка операции), сводка выделения — `booleanOp` (у выделения только из булевых групп с одной операцией; список тулбара отмечает её); команды раскладки `object.boolean_union`, `object.boolean_subtract`, `object.boolean_intersect`, `object.boolean_exclude` (Alt+Shift+U/S/I/E), `object.outline_stroke` (Ctrl/Cmd+Alt+O) идут обычным `Execute`; подсказка кнопки тулбара — с сочетанием из действующей раскладки. Геометрия в протокол не идёт.
- **Сделано (этап 5):** инструмент `scale` (команда раскладки `tool.scale` — K), правка панели `scale { factor, anchor: [x, y] }` (коэффициент 0,01…100, точка — 0, 0,5 или 1 по осям; вне — ошибка), команда раскладки `edit.copy_svg` без сочетания и событие `ClipboardSvg { text }`; `Export { scale, format?: png | svg | pdf, outlineText?, svgIds? }` (без формата — PNG, как раньше), `Exported { name, bytes, mime }`; геометрия по-прежнему в протокол не идёт.
- **Сделано (этап 1):** `FontsAvailable { source: bundled | system, families: [{ family, faces: [{ style, weight?, italic?, stretch?, file, index? }] }] }` (у системных Chrome вес и курсив ядро угадывает по имени начертания), `LoadFont { family, file, bytes? }` — по одному файлу (без байтов — файл не получить), `FontRequest { family, source, files }` (вшитые выполняет worker ядра, системные — основной поток), `MissingFonts { list: [{ family, style, nodes }] }`, `ReplaceMissingFont { from: { family, style }, to: { family, style } }`; сверх списка — события `FontCatalog { families: [{ family, source, styles }] }` (выбор замены) и `Log { message }` (журнал ошибок), команды `DemoText { texts }` (замер) и `TestCreateText` (только тестовый режим страницы). Правки, которым нужны ещё не загруженные шрифты, ядро держит в очереди и выполняет по приходу байтов.

````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **PDF:** крейт `krilla` (векторные страницы, подмножества шрифтов, картинки); фрейм — страница его размера.
- Экспорт идёт в worker'е ядра, байты — передачей (как PNG, ядро §8.7); в Tauri — родным диалогом сохранения.

## 12. Тестирование

- Как в ядре §12: модульные тесты крейтов (модель текста и разметка, отмена правок текста, раскладка с эталонами позиций глифов на вшитых шрифтах, векторная сеть, булевы операции — свойства и эталоны, писатель SVG — сравнение с эталоном и разбор), эталоны рендера (текст, LOD, векторы, обводки, стрелки) на `vello_cpu` и пиксели GPU против CPU, E2E (ввод, IME — синтетическими событиями композиции, буфер обмена, выбор шрифта, отсутствующий шрифт, перо, булевы, экспорт), замеры (раскладка и рисование 10 000 текстов, булевы на 1000 контуров).
- Совместимость: файл с текстом открывается в сборке подсистемы 1 только для чтения (тест на документе, собранном новой сборкой): `scripts/compat-old-build.sh [ref]` собирает `ferrum-doc` из `ref` (по умолчанию `main`) во временной копии и открывает им файл примера `text_fixture`; файл с вектором — `scripts/compat-old-build.sh <ref> vector_fixture` (сборка без векторов открывает его только для чтения); файл с булевой группой — `scripts/compat-old-build.sh <ref> boolean_fixture` (сборка без булевых групп скрывает группу с детьми и открывает файл только для чтения).
````

на:

````markdown
- **PDF:** крейт `krilla` (векторные страницы, подмножества шрифтов, картинки); фрейм — страница его размера.
- Экспорт идёт в worker'е ядра, байты — передачей (как PNG, ядро §8.7); в Tauri — родным диалогом сохранения.
- **Сделано (этап 5), Scale:** инструмент «Масштабирование» (K, команда `tool.scale`; в тулбаре — кнопка-список с «Перемещением», название не «Масштаб», чтобы не путать с зумом) — тот же инструмент выделения, у которого ручки масштабируют выделение всегда пропорционально (как в Figma: у обводки и кегля одно число; Shift ничего не меняет, Alt — от центра), зоны поворота нет, у одной линии — обычная рамка (масштаб по длине). Масштабируются толщина обводок, радиус скругления, кегль, интерлиньяж и трекинг в px (проценты и «Авто» — нет), интервал и отступ абзацев, разметка участков в px, радиусы точек вектора; окончания и стрелки строятся от толщины и растут с ней; предел острого угла — отношение, не меняется. Содержимое групп, булевых групп и фреймов масштабируется (у ресайза дети фрейма стоят на месте) — ребёнок фрейма получает масштаб в осях фрейма после его правки (`geometry::scale_patches_deep`, обход поддерева без рекурсии). Поворот и отражение узла сохраняются; у одного узла рамка — его повёрнутая, у нескольких — общая в мире. Без привязки к пиксельной сетке (дробные размеры, как в Figma). Текст «авто» сохраняет режим, рамка — по раскладке с новым кеглем (целыми вверх, как после правки текста; шрифт грузится — масштабированная рамка); значения стиля зажимаются в пределы секции «Текст». Инструмент после жеста остаётся K, Esc — «Перемещение»; наведение и измерения с Alt — как у V. Панель с K и выделением — секция «Масштаб» над «Положением»: точка рамки сеткой 3×3 (радиогруппа, стрелки; по умолчанию левый верхний угол — как у полей W и H) и поле в процентах (показывает 100 %, масштаб — от текущего размера; набор и перетаскивание подписи — превью и запись одним шагом); ядро отклоняет коэффициент вне 1 %…10 000 % и точку вне сетки (`Edit::Scale { factor, anchor }`). Жест и поле — одна функция `scale::scale_patches`, один шаг отмены. Замер (`editor_report`, нативно, release): 10 000 узлов (100 фреймов по 100) — кадр жеста ручкой ≈ 31,7 мс (цель ≤ 50), первое движение ≈ 38 мс, запись жеста ≈ 108 мс, превью поля ≈ 32 мс, запись поля ≈ 91 мс (цели ≤ 500).
- **Сделано (этап 5), SVG:** писатель — `ferrum-render::svg` (`Renderer::export_svg`), область и порядок — те же, что у PNG (`export::export_items`), единицы документа. Фрейм — заливка, дети в `<g clip-path>` (при «Обрезать содержимое»), обводка над ними вне обрезки; группа без прозрачности и id своего `<g>` не получает (как Figma); прозрачность узла — `<g opacity>`; скрытые слои и дети булевых групп не пишутся. Сдвиг запекается в координаты (`x`, `y`, `d`), поворот и отражение — `transform="matrix(…)"` (масштаба у узлов нет — толщина обводок не искажается). Заливки — цвет (`fill`, `fill-opacity`) и картинка (`<pattern>` с `<use>` картинки из `<defs>`: `data:` PNG, байты один раз на файл; «Вписать» — поля прозрачны, «Замостить» — узор размером с картинку; без байтов — заливка пропускается). Градиентов, режимов наложения и эффектов в модели нет — писать нечего. Обводка по центру — `stroke`; внутри — двойная толщина с `<clipPath>` формы, снаружи — двойная с `<mask>` (белый прямоугольник с запасом на острия, форма чёрным), как у Figma; обводка по контуру (вектор, булева группа, линия с окончаниями, прямоугольник с соединением не «острым») — путь со своим соединением и пределом, окончания (круг, ромб, треугольник, стрелка) и точки ветвления — залитые фигуры, полупрозрачная — в `<g opacity>`. Подчёркивание и зачёркивание — прямоугольники. «Текст кривыми» по умолчанию включён (как в Figma) — контуры глифов; без него — `<text>` на прогон раскладки (`xml:space="preserve"`, семейство, кегль, вес, курсив, ширина, трекинг, строка от первого глифа на базовой линии; прогон помнит свой текст и кластеры глифов — `GlyphRun::text`, `PlacedGlyph::cluster`), отсутствующий шрифт пишется запасным Inter, как на холсте. «Атрибут id» — id из имён слоёв (буквы, цифры, `-`, `_`, уникальные). Пределы: 256 МБ и 30 с (`ExportBudget`). «Копировать как SVG» (`edit.copy_svg`, без сочетания, как в Figma; пункт контекстного меню холста) — тот же писатель с текстом кривыми, не больше 64 МБ: `text/plain` и `image/svg+xml`, где умеет `ClipboardItem.supports` (Chromium записывает его, очистив разметку), иначе обычный текст через `execCommand`. Команд — 55. Замер (`render_report`): страница из 10 000 узлов — ≈ 40 мс, 2,6 МБ.
- **Сделано (этап 5), PDF:** `krilla 0.8.2` (2026-06-04, MIT OR Apache-2.0, Rust ≥ 1.92) без фич по умолчанию, с `raster-images`: собирается под `wasm32-unknown-unknown`; `simple-text` (rustybuzz) не нужен — глифы раскладывает parley. В `Cargo.lock` — 23 новых крейта (в том числе свой `skrifa 0.42`/`read-fonts` рядом с нашими 0.44); wasm ядра вырос с 10,05 до 12,33 МБ (gzip 3,42 → 3,98 МБ) вместе с писателем SVG. Страницы рисует тот же рисовальщик, что экран и PNG, через свой бэкенд (`pdf::PdfBackend`): пути, обрезка, группы прозрачности, картинки (прямая альфа; «Замостить» — плитками, не больше 4096), обводки текста. Текст сплошной краской — настоящим текстом: шрифт встроен подмножеством, вес и ширина вариативного шрифта — из начертания прогона, прочие оси — из нормализованных координат без `avar`, ToUnicode — из кластеров (новый метод бэкенда `fill_glyphs`); текст картинкой и обводки текста — контурами. Шрифт, который krilla не встроил, — весь файл заново с текстом контурами. Страница: у выделения — каждый узел верхнего уровня, без выделения — каждый фрейм верхнего уровня, нет фреймов — вся страница одной; порядок — чтения (сверху вниз, слева направо). Размер — область узла, 1 px = 1 pt, сторона не больше 14 400 pt (больше — уменьшается целиком). Большие векторы (от 4096 элементов) упрощаются с погрешностью не больше 1/64 pt (точный контур сети в 100 000 точек — 8,8 МБ и 1,4 с). Теги PDF выключены. Проверка вне тестов: файл открывается PDFKit macOS, текст извлекается, вес рисуется. Замер (`render_report`): страница со 100 текстами и вектором в 100 000 точек — ≈ 110 мс, 0,7 МБ.
- **Сделано (этап 5), экспорт:** раздел «Экспорт» панели — формат списком (PNG, SVG, PDF; JPG в ядре нет), масштаб 1×/2×/3× — только у PNG, у SVG — «Текст кривыми» и «Атрибут id». Без выделения PNG и SVG — вся страница одним файлом, PDF — по фрейму на страницу. Имена — «Слой@2x.png», «Слой.svg», «Страница 1.pdf». Экспорт — в worker'е ядра (основной поток только сохраняет готовые байты), шрифты ещё грузятся — ошибка, нечитаемые картинки PDF — ошибка (как у PNG). Десктоп сохраняет любой экспорт родным диалогом «Сохранить» из Rust (`export_save`: байты сырым телом IPC, имя — заголовком ASCII-JSON, фильтр по типу, атомарная запись; путь интерфейс не видит) — снято «экспорт PNG в десктопе родным диалогом» из отложенного ядра.

## 12. Тестирование

- Как в ядре §12: модульные тесты крейтов (модель текста и разметка, отмена правок текста, раскладка с эталонами позиций глифов на вшитых шрифтах, векторная сеть, булевы операции — свойства и эталоны, писатель SVG — сравнение с эталоном и разбор), эталоны рендера (текст, LOD, векторы, обводки, стрелки) на `vello_cpu` и пиксели GPU против CPU, E2E (ввод, IME — синтетическими событиями композиции, буфер обмена, выбор шрифта, отсутствующий шрифт, перо, булевы, экспорт), замеры (раскладка и рисование 10 000 текстов, булевы на 1000 контуров).
- **Сделано (этап 5):** писатель SVG — эталоны `crates/ferrum-render/tests/golden/svg_*.svg` (побайтно; пересоздать — `FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test svg`) и разбор XML (`roxmltree`, только тесты); PDF — словари страниц и шрифтов открытым текстом, потоки — после распаковки (`flate2`, только тесты); E2E `scale.spec.ts` и `export.spec.ts` (SVG разбирается браузером, PDF — страницы и размеры), «Копировать как SVG» — в `clipboard.spec.ts`; замеры — SVG 10 000 узлов, PDF с текстом и вектором в 100 000 точек, Scale на 10 000 узлах. Модель документа этапом 5 не менялась — проверки совместимости прежние.
- Совместимость: файл с текстом открывается в сборке подсистемы 1 только для чтения (тест на документе, собранном новой сборкой): `scripts/compat-old-build.sh [ref]` собирает `ferrum-doc` из `ref` (по умолчанию `main`) во временной копии и открывает им файл примера `text_fixture`; файл с вектором — `scripts/compat-old-build.sh <ref> vector_fixture` (сборка без векторов открывает его только для чтения); файл с булевой группой — `scripts/compat-old-build.sh <ref> boolean_fixture` (сборка без булевых групп скрывает группу с детьми и открывает файл только для чтения).
````

- [ ] **Step 2: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-render --example render_report | grep -E "FAIL|Экспорт"`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Масштаб"`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && ./scripts/compat-old-build.sh 0797cf7 boolean_fixture`

Run: `export CARGO_TARGET_DIR=$PWD/target CARGO_INCREMENTAL=0 && ./scripts/compat-old-build.sh main vector_fixture`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck && npm run unit && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target CARGO_INCREMENTAL=0 && npx playwright test e2e/export.spec.ts e2e/scale.spec.ts e2e/booleans.spec.ts e2e/vectors.spec.ts e2e/text.spec.ts e2e/roundtrip.spec.ts --project=firefox`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR=$PWD/../../../target CARGO_INCREMENTAL=0 && cargo check --offline`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR=$PWD/../../../target CARGO_INCREMENTAL=0 && cargo clippy --offline --all-targets -- -D warnings`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR=$PWD/../../../target CARGO_INCREMENTAL=0 && cargo test --offline --lib export_names`

Expected: всё PASS: `cargo test --workspace` — 937 passed, 2 ignored; `render_report` — SVG 10 000 узлов ≈ 37 мс (2624 КБ), PDF 100 текстов и вектор 100 000 точек ≈ 108 мс (729 КБ), цели ≤ 1000 мс, PNG фрейма @3x ≈ 97 мс (справочно), FAIL нет; `editor_report` — K на 10 000 узлах: кадр жеста ≈ 30 мс (≤ 50), первое движение ≈ 38 мс, запись жеста ≈ 111 мс (≤ 500), превью поля ≈ 31 мс (≤ 50), запись поля ≈ 88 мс (≤ 500), 2 шага, FAIL нет; совместимость — оба примера только для чтения; модульные — 136 passed; `e2e:ci` — 331 passed (7,2 мин); Firefox — 7 passed; десктоп — `cargo check`, `clippy` чисто, `export_names` — 1 passed

- [ ] **Step 3: Закоммитить**

```bash
git add crates/ferrum-editor/examples/editor_report.rs crates/ferrum-render/examples/render_report.rs crates/ferrum-render/src/pdf.rs docs/superpowers/specs/2026-10-23-ferrum-core-design.md docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md
git commit -m "perf: замеры экспорта SVG и PDF и Scale на 10 000 узлах, спека этапа 5

render_report: SVG страницы из 10 000 узлов (≈ 38 мс, 2,6 МБ) и PDF страницы
со 100 текстами и вектором в 100 000 точек (≈ 109 мс, 0,7 МБ), цели ≤ 1 с.
editor_report: инструмент K на 10 000 узлах — кадр жеста ≈ 32 мс (≤ 50),
запись ≈ 111 мс, превью поля ≈ 31 мс, запись поля ≈ 95 мс (≤ 500), по шагу.
Большие векторы в PDF упрощаются с погрешностью не больше 1/64 pt
(PDF_DETAIL_SCALE = 16): точный контур 100 000 точек — 8,8 МБ и 1,4 с.
Спека: «Сделано (этап 5)» в разделах 8, 11 и 12; спека ядра — экспорт в
десктопе родным диалогом снят из отложенного.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Ручная проверка**

1. Chrome: `#/new`, прямоугольник с обводкой и скруглением, текст; K, угловая ручка — обводка, радиус и кегль растут вместе, отпускание — один шаг; Alt — от центра; Esc — «Перемещение».
2. Секция «Масштаб»: точка рамки, 50 % — Enter; Ctrl/Cmd+Z.
3. Раздел «Экспорт»: PNG 2×, SVG с «Текстом кривыми» и без, «Атрибут id», PDF фрейма — открыть файлы в браузере и просмотрщике PDF (текст выделяется и копируется).
4. Контекстное меню холста — «Копировать как SVG», вставка в текстовый редактор и в Figma.
5. Десктоп (`cd apps/desktop && npx tauri dev`): экспорт PNG/SVG/PDF — родной диалог, отказ — без уведомления.
6. Firefox: K угловой ручкой, экспорт SVG.

---

## Что этап 5 сознательно не делает

- **Кадр жеста K в 16,7 мс на больших выделениях.** Масштаб поддерева из 10 000 узлов — ≈ 32 мс на движение (цель — 50 мс, а не кадр), первое движение ≈ 36 мс (базы поддерева), запись ≈ 111 мс (≤ 500); превью поля «Масштаб» — ≈ 31 мс.
- **Точные большие векторы в PDF.** Векторы из 4096 элементов и больше упрощаются с допуском до 1/64 pt (`PDF_DETAIL_SCALE = 16`): точный контур 100 000 точек — 8,8 МБ и 1,4 с против 0,7 МБ и 0,11 с.
- **`avar` вариативных шрифтов в PDF.** Оси, кроме веса и ширины, пишутся обратной нормализацией без таблицы `avar` — у шрифтов с нелинейными осями начертание в PDF может слегка отличаться от экрана.
- **Размер wasm.** `krilla` и писатель SVG увеличили wasm ядра на 2,3 МБ (10 053 050 → 12 333 408 Б) и на 0,56 МБ в gzip (3 423 011 → 3 982 651 Б); отложенной загрузки экспорта нет.
- **Градиенты, режимы наложения, эффекты** в экспорте — их нет в модели; SVG и PDF пишут только то, что модель умеет (цвет, картинки, прозрачность).
- **JPG, масштаб SVG/PDF, теги PDF (доступность), масштаб плитки «Замостить»** — не делаются.
- **Пиксели GPU против CPU** (проект `gpu`), замеры `@perf`, Safari/WebKit, запуск десктопа и живой родной диалог «Сохранить» — не запускались; десктоп проверен `cargo check`, `clippy` и юнит-тестом имён, замеры — только нативные.
- **Сторонние рендеры SVG и PDF** — SVG проверен разбором XML и `DOMParser` браузера, PDF — словарями и распакованными потоками; эталоны созданы на одной машине (macOS, arm64).
