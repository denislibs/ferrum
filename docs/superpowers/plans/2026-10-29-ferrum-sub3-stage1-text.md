# Ferrum, подсистема 3, этап 1: текст — модель и рендер — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Текстовый узел `Text` в документе (содержимое — `LoroText` с разметкой, свойства в зеркале, отмена правок текста обратной дельтой, слияние ввода подряд, пределы), шрифты (десять вшитых семейств с манифестом, системные шрифты Chrome и десктопа, реестр 256 МБ с вытеснением, отсутствующий шрифт — запасной Inter и замена одним шагом), раскладка в новом крейте `ferrum-text` (`parley` со своей коллекцией `fontique`, абзацы, списки, украшения, обрезка с многоточием) и отрисовка глифов контурами `skrifa` (LOD полосами, ленивая раскладка в пределах кадра, экспорт PNG и превью с текстом). Инструмента T нет — тексты создаёт тестовая команда; замер «10 000 текстов» — в `render_report` и E2E `@perf`.

**Architecture:** `ferrum-doc` хранит содержимое текстового узла дочерним `LoroText` его карты (ключ `text`), стили — разметкой Loro с расширением «как в редакторах» (`config_text_style` ставит каждая страница), свойства текста — ключами карты; в зеркале свойств `Props::text: Option<Arc<TextData>>`, кэш сцены (версия 3) несёт и текст. Правка текста — дельта Quill: перед применением строится обратная дельта (`slice_delta`), она и есть шаг отмены (`Inverse::Text`), поэтому отмена переживает пересоздание узла под новым `TreeID`. Новый крейт `ferrum-fontmeta` читает метаданные файлов шрифтов (`skrifa`) для манифеста и десктопа; `ferrum-text` — реестр шрифтов (`Fonts`: каталоги источников, запросы байтов, загрузка, вытеснение, контуры глифов) и раскладка абзацами через `parley`. Один `SharedFonts` (`Rc<RefCell<Fonts>>`) на ядро: им раскладывает редактор (размер `AutoWidth`/`AutoHeight` пишется той же транзакцией) и рисует рендер (кэш раскладок по слоту, `version` узла и поколению реестра). Worker ядра сам качает вшитые файлы (`/fonts/manifest.json`, `/fonts/<семейство>/…`); системные шрифты Chrome — `queryLocalFonts` в основном потоке, `Blob` читает worker ввода-вывода, байты уходят ядру передачей; десктоп — сканирование каталогов в Rust, байты сырым ответом IPC.

**Tech Stack:** Rust 1.96 (новые крейты рабочего пространства `ferrum-text`, `ferrum-fontmeta`; внешних крейтов не добавляется — `parley 0.12.0`, `skrifa 0.44.0`, `serde_json 1.0.152` уже в `Cargo.lock`), Loro 1.16.2 (`LoroText`, `mark`/`unmark`, `apply_delta`, `slice_delta`, `config_text_style`), parley 0.12 / fontique 0.12 (`Collection { system_fonts: false }`, `register_fonts`, `unregister_font`, `set_text_indent`, `OverflowWrap::BreakWord`), vello_cpu 0.3; SolidJS 1.9.17, TypeScript 6.0.3, Vite 8.3.4, @playwright/test 1.64.0 (проекты `chromium`, `firefox`); десктоп — `apps/desktop/src-tauri` (tauri 2.12.1) + путь к `ferrum-fontmeta`. Шрифты — google/fonts на коммите `bd8f81ddb5c74d5c8897b36ad88b440266245103` (2026-10-09), SIL OFL 1.1.

**Спеки:** `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` — разделы 3, 4, 5, 6 и части разделов 8 и 12, нужные этапу 1; спека ядра `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` (ниже — «ядро §N»).

## Global Constraints

- Ветка `feat/sub3-text` от `main` (2710c85). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Каталог сборки — у каждой копии свой: `export CARGO_TARGET_DIR=$PWD/target` (в `apps/web` — `$PWD/../../target`); все команды — из корня копии, если не сказано иное.
- Корневой workspace: новые крейты `crates/ferrum-text` и `crates/ferrum-fontmeta`, новых внешних зависимостей нет — `Cargo.lock` меняется только записями этих двух крейтов. Десктоп (`apps/desktop/src-tauri`, свой `Cargo.lock`) получает путь к `ferrum-fontmeta`; `font-types` — `cargo update -p font-types --precise 0.12.6` (как в корне; новее — опубликованы после 2026-10-10).
- npm: новых пакетов нет (`package-lock.json` не меняется).
- Шрифты: Inter, Roboto, Open Sans, Montserrat, PT Sans, PT Serif, Noto Sans, JetBrains Mono, Playfair Display — файлы google/fonts на коммите выше (вариативные, где есть), сверка SHA-256, лицензия `OFL.txt` рядом; лежат в git (`apps/web/public/fonts`, ≈ 13 МБ). Onest — тот же файл, что уже вшит в wasm (`Onest-wght.ttf`, SHA-256 `966c5c29…`, файл переезжает из `ferrum-render` в `ferrum-text`), в `public/fonts` его нет.
- Основной поток тонкий (ядро §4.2): раскладка, реестр, сканирование документа на отсутствующие шрифты — в worker'е ядра; байты шрифтов проходят основной поток только передачей буферов (системные шрифты Chrome: `Blob` → worker ввода-вывода → `ArrayBuffer` передачей → ядро; десктоп — ответ IPC окну, затем передача ядру). Вшитые шрифты основной поток не видит вовсе.
- Недоверенный ввод (спека 8): текст тестовой команды ≤ 64 КБ, имя семейства и начертания ≤ 256 байт, путь файла ≤ 4096 байт, семейств в команде ≤ 4096, начертаний у семейства ≤ 1000, файл шрифта ≤ 64 МБ (`MAX_FONT_FILE`), текст узла ≤ 100 000 символов, участков со стилями ≤ 10 000 (превышение — отказ правки с сообщением, без шага). Неверное — `Error` на русском; неразборчивый файл шрифта — `Log` (журнал ошибок в консоли), не уведомление.
- Позиции в тексте — символы Unicode (скаляры): так считает `LoroText` без фичи `wasm` у Loro. Перевод из UTF-16 поля ввода — этап 2.
- `FORMAT_VERSION` не меняется: старая сборка видит узел `"text"` как неизвестный и открывает документ только для чтения (ядро §5.2; проверка — `scripts/compat-old-build.sh`). Версия таблицы кэша сцены — 3 (прежние сборки её пропускают и читают Loro).
- Тесты не проходят вхолостую: раскладка сверяется с метриками вшитого Inter, рендер — пикселями и эталоном, ядро — событиями и документом, E2E — пикселями экрана и сетевыми запросами. Тестовая команда `testCreateText` — только в тестовом режиме страницы (`init.test`), как `testPanic`.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Сначала прототип всего этапа собран в отдельной копии (`plan-verify-wt`, ветка `verify/sub3-text`) и прошёл ворота: `cargo fmt --check`, `clippy -D warnings` (нативно и `wasm32-unknown-unknown`), `cargo test --workspace`, `render_report` (release), wasm-сборка, `npm run typecheck`, модульные (118), E2E `chromium` целиком (`npm run e2e:ci` — 294 passed), `text.spec.ts` в Firefox, замер `@perf` «10 000 текстов», совместимость со сборкой `main` (`scripts/compat-old-build.sh`), тест сканирования шрифтов десктопа. Затем шаги этого документа сгенерированы из прототипа и применены скриптом к чистой копии `feat/sub3-text` в порядке задач: каждый фрагмент «заменить» найден ровно один раз, после каждой задачи — её ворота (команды в задачах), после последней дерево совпало с прототипом байт в байт. Проверенное прототипом API, отличающееся от ожиданий: `apply_delta` вставляет `Insert` с точной разметкой (без наследования слева) — для «как у соседа» нужна обычная `LoroText::insert`; `null` в атрибутах `Retain` снимает разметку; `get_or_create_container` у `LoroMap` устарел — `insert_container`; корневой стиль `parley` (16 px) подпирает строку, если его не задать стилем первого участка; вариативный вес — координатой `wght` (через `normalized_coords` прогона). Не запускались: проект Playwright `gpu` (пиксели WebGPU/WebGL2 против CPU с текстом), приложение Tauri с настоящими системными шрифтами (`fonts_list` на macOS проверен только тестом сканирования каталога), Safari/WebKit. Тексты ожидаемых результатов — с этого прогона.

---

## Этапы подсистемы 3

| Этап | Результат | Разделы спеки |
|---|---|---|
| **1. Текст: модель и рендер** (этот план) | узел `Text`, шрифты (вшитые и системные), раскладка, отрисовка, отсутствующие шрифты, текст в PNG и превью | 3, 4, 5, 6 |
| 2. Инструмент «Текст» и редактирование | T, правка на холсте, ввод и IME, курсор, буфер обмена с текстом, секция «Текст» | 7, 8 |
| 3–5 | векторы и перо; булевы операции; Scale и экспорт SVG/PDF | 9–11 |

## Решения, которых в спеке нет (на согласование)

Заполняя пробелы спеки, план принимает решения; поведение для пользователя — помечено **(поведение)**.

1. **(поведение)** Цвет текста по умолчанию — список `fills` узла (как «Заливка» текстового слоя в Figma; правится существующей секцией «Заливка» панели), а не отдельный ключ `fill` среди умолчаний стиля. Разметка `fill` — заливка участка. Новый текст — чёрный (`#000000ff`), Inter Regular 12, «Авто».
2. Текст узла всегда кончается переводом строки: на нём лежит разметка последнего абзаца (список, уровень), как в Quill. Раскладка его не рисует; вставлять после него и удалять его нельзя (`TextRange`).
3. Свойства текста пишутся в карту узла все (14 ключей), а не только отличные от умолчаний: умолчания текста могут ещё меняться, а запись явная. Строка кэша сцены текста несёт свойства и содержимое.
4. **(поведение)** Ресайз текстовой рамки (ручки, поля W/H, масштаб группы) переводит текст в `Fixed` тем же шагом. В Figma горизонтальный ресайз `AutoWidth` даёт `AutoHeight` — это этап 2 вместе с режимом размера в панели.
5. **(поведение)** «Отсутствующие шрифты (N)» — шрифты **текущей страницы** (шаг отмены у нас — по странице); замена — на текущей странице одним шагом. Секция — на панели «Дизайн» без выделения (как в Figma), выбор замены — `<select>` семейства (группы «Вшитые», «Системные») и начертания (по умолчанию то же имя, иначе Regular).
6. **(поведение)** Пункт «Показать системные шрифты» до появления выбора шрифта (этап 2) живёт в секции отсутствующих шрифтов (только Chrome/Edge и десктоп; Firefox и Safari — скрыт).
7. **(поведение)** Пока байты шрифта не пришли, текст не рисуется (как в Figma; без мелькания запасным шрифтом); отсутствующий шрифт рисуется запасным Inter сразу. Глифов, которых нет в шрифте, — пустые места: запасных семейств для отдельных символов (CJK и т. п.) на этапе 1 нет.
8. Обводки у текста на этапе 1 не рисуются (в Figma — есть; секция «Текст» и обводки текста — этап 2).
9. Текст, вылезающий за фиксированную рамку без обрезки, рисуется, но отсечение экрана и R-дерево — по рамке (часть глифов за рамкой у края экрана может пропасть до прокрутки).
10. Регистр `upper`/`lower`/`title` меняет текст до формирования глифов; `smallCaps` — функция `smcp` шрифта (у шрифта без неё — без эффекта). Отступ списка — 1,5 кегля первого символа абзаца на уровень, маркеры «•» и «N.» (нумерация сбрасывается не-списком и сменой уровня).
11. LOD: строка ниже 3 px устройства — полоса 60 % высоты строки цвета текста с прозрачностью 0,25; узел меньше 3 px — прямоугольник того же цвета; не разложенный ещё текст (предел времени кадра 4 мс на раскладки) — полосы по оценке строк (кегль × 1,2).
12. Слияние ввода подряд (спека 3) реализовано в модели (`Document::transact_merging` с меткой `TextMerge`), но вызывать его до этапа 2 некому — проверяется тестами.
13. Протокол (спека 8 — без полей): `FontsAvailable { source, families: [{ family, faces: [{ style, weight?, italic?, stretch?, file, index? }] }] }`, `LoadFont { family, file, bytes? }` — по одному файлу (без байтов — файл не получить), `FontRequest { family, source, files }`, `MissingFonts { list: [{ family, style, nodes }] }`, `ReplaceMissingFont { from: {family, style}, to: {family, style} }`. Сверх спеки: события `FontCatalog { families: [{ family, source, styles }] }` (выбор замены) и `Log { message }` (журнал ошибок), команды `DemoText { texts }` (замер) и `TestCreateText { … }` (только тестовый режим).
14. Буфер обмена: узел текста копируется с содержимым — поле `text` (JSON текста) в формате `web application/x-ferrum`, версия формата прежняя (старые сборки пропускают узел «text» как неизвестный).
15. Десктоп сканирует каталоги шрифтов при первом `fonts_list` (в потоке Tauri): каждый файл читается целиком один раз ради метаданных — на macOS это секунды при первом запуске.

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-doc/src/text.rs` (новый) | типы текста: `FontName`, `TextProps`, `TextStyle`, `TextAttrs`, `TextRun`, `TextContent`, `TextData`, перечисления, пределы |
| `crates/ferrum-doc/src/text_codec.rs` (новый) | текст в Loro: ключи свойств, разметка ↔ атрибуты, `LoroText` ↔ содержимое, обратная дельта, JSON для буфера |
| `crates/ferrum-doc/src/{types,codec,page,scene_cache,binary,container,changes,error,history,document,fixture,lib}.rs` | узел `Text`, правки текста и `Inverse::Text`, кэш сцены v3, слияние ввода, `create_with_props`, пример из текстов |
| `crates/ferrum-doc/examples/text_fixture.rs`, `scripts/compat-old-build.sh` (новые) | файл с текстом и проверка старой сборкой |
| `crates/ferrum-fontmeta/` (новый крейт) | семейство и начертания файла шрифта (`skrifa`) |
| `crates/ferrum-text/` (новый крейт) | реестр шрифтов, раскладка, контуры глифов, манифест вшитых (`examples/font_manifest.rs`) |
| `apps/web/scripts/fetch-fonts.mjs`, `apps/web/public/fonts/**` (новые) | вшитые шрифты, лицензии, манифест |
| `crates/ferrum-scene/src/{shape,scene}.rs` | форма и попадание текста — рамка; `Scene::iter` |
| `crates/ferrum-render/src/{text_node,draw,display,lib,text}.rs` | глифы, полосы LOD, ленивая раскладка, общий реестр у рендерера |
| `crates/ferrum-editor/src/text.rs` (новый), `{editor,lib,clipboard,ops,actions,panel,tools/select}.rs` | размер по раскладке, создание текста, отсутствующие шрифты и замена, ресайз → `Fixed`, копии с текстом |
| `crates/ferrum-wasm/src/core/fonts.rs` (новый), `{protocol,core,core/panels,layers}.rs` | команды и события шрифтов, очередь правок, ждущих шрифтов, `DemoText` |
| `apps/web/src/core/{fonts,limits,messages,file-list}.ts`, `src/worker/{core,io}.worker.ts` | манифест и вшитые файлы в worker'е ядра, байты системных шрифтов в worker'е ввода-вывода |
| `apps/web/src/ui/{system-fonts,missing-fonts}.ts(x)` (новые), `{boot,app,context,store,properties,layers,file-store,tauri}.ts(x)`, `styles/properties.css`, `src/debug.ts` | системные шрифты, «Отсутствующие шрифты (N)», иконка слоя |
| `apps/desktop/src-tauri/src/fonts.rs` (новый), `lib.rs`, `Cargo.toml` | сканирование шрифтов ОС, `fonts_list`, `fonts_read` |
| `apps/web/e2e/{text,perf}.spec.ts`, `apps/web/unit/{fonts,limits}.test.ts` | E2E и модульные тесты |
| `crates/ferrum-render/examples/render_report.rs` | замер «10 000 текстов» |
| `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` | спека: принятые решения этапа 1 |

---

### Task 1: Узел `Text`: модель, разметка, отмена правок текста, файл

Спека 3: новый вид узла `"text"`; содержимое — `LoroText` узла с разметкой Loro; свойства — в зеркале; правка текста записывается дельтой и откатывается обратной дельтой; пределы 100 000 символов и 10 000 участков. Задача даёт модель целиком, кроме слияния ввода и JSON (Task 2): типы (`text.rs`), чтение и запись (`text_codec.rs`, `codec.rs`), правки `Tx::insert_text`, `delete_text`, `format_text`, `set_text_content` с обратной дельтой (`Inverse::Text`), кэш сцены версии 3 с текстом. Чтобы рабочее пространство собиралось с новым `NodeKind::Text`, здесь же — форма текста в сцене (рамка), тип строки слоя и иконка, поле `text` у `Props` в буфере обмена.

Как устроена отмена: перед применением дельты `text_codec::invert` строит обратную по текущему тексту (`slice_delta` удаляемого и переразмечаемого), дельта применяется (`LoroText::apply_delta`; вставка «с разметкой слева» — обычной `LoroText::insert`: `apply_delta` вставляет ровно с указанной разметкой), содержимое зеркала перечитывается, обратная дельта уходит в журнал транзакции. Узел ищется по стабильному id, поэтому старый шаг отмены применяется и к узлу, пересозданному отменой удаления под новым `TreeID`.

**Files:**
- Create: `crates/ferrum-doc/src/text.rs`, `crates/ferrum-doc/src/text_codec.rs`
- Modify: `crates/ferrum-doc/src/types.rs` (`NodeKind` 86–131, `Props` 195–261, `PropsPatch` 263–341), `codec.rs` (`read_props` 80–110, `write_props` 180–201, `write_patch` 203–246), `page.rs` (`Inverse` 40–55, `wrap` 177–189, `replay` 375–385, `set_props` 481–495, `Tx` 812–851), `scene_cache.rs` (32–214), `binary.rs`, `container.rs:38`, `changes.rs:43–52`, `error.rs`, `lib.rs`, `document.rs` (тесты 620–636, 819–839)
- Modify: `crates/ferrum-scene/src/shape.rs` (`geometry` 83–99, `hit` 260–291), `crates/ferrum-scene/src/scene.rs` (`looks_different` 1000–1007), `crates/ferrum-wasm/src/layers.rs` (`NodeType` 38–63), `crates/ferrum-editor/src/clipboard.rs` (`props_from_json` 407–488, `full_patch` 510–530), `apps/web/src/ui/layers.tsx:21–28`
- Test: `crates/ferrum-doc/tests/text.rs` (новый), `crates/ferrum-doc/tests/container.rs` (`scene_cache_roundtrips_every_field`), `crates/ferrum-doc/tests/flip.rs` (`scene_cache_version_covers_flips`)

**Interfaces:**
- Consumes: `PageDoc`, `Tx`, `Document::transact`, `codec::write_props`/`read_props`, `scene_cache::{encode, decode}` этапов 1–6.
- Produces (ferrum-doc, публично): `NodeKind::Text`; `Props::text: Option<Arc<TextData>>`, `Props::text_props() -> Option<&TextProps>`; `PropsPatch::text: Option<TextProps>` (у не текста пропускается); типы `FontName { family, style }` (`FontName::new`, `FontName::inter_regular`), `TextSizing { AutoWidth, AutoHeight, Fixed }`, `AlignH { Left, Center, Right, Justify }`, `AlignV { Top, Middle, Bottom }`, `LineHeight { Auto, Px(f64), Percent(f64) }`, `LetterSpacing { Px, Percent }`, `Decoration { None, Underline, Strike }`, `TextCase { None, Upper, Lower, Title, SmallCaps }`, `ListKind { None, Bullet, Ordered }`, `TextStyle { font, size, line_height, letter_spacing, decoration, case }`, `TextProps { sizing, align_h, align_v, paragraph_spacing, paragraph_indent, truncate, max_lines: Option<u32>, style }`, `AttrKey` (`as_str`, `is_paragraph`, `ALL`), `TextAttrs` (все поля `Option`; `has`, `is_empty`, `font_name(&TextStyle)`), `TextRun { text, attrs }`, `TextContent { runs }` (`plain`, `text`, `char_len`, `styled_runs`, `fonts(&TextStyle)`, `run_ranges`), `TextData { props, content }`, константы `MAX_TEXT_CHARS = 100_000`, `MAX_TEXT_RUNS = 10_000`, `MAX_LIST_INDENT = 5`, `TEXT_COLOR`; `Tx::insert_text(id, at, &str, Option<&TextAttrs>)`, `Tx::delete_text(id, Range<usize>)`, `Tx::format_text(id, Range<usize>, set: &TextAttrs, clear: &[AttrKey])`, `Tx::set_text_content(id, &TextContent)`; ошибки `DocError::{NotText, TextTooLong, TooManyTextRuns, TextRange}`; `SCENE_CACHE_VERSION = 3`.
- Produces (другие крейты): `shape::geometry` текста — `Rect` без скругления, `shape::hit` — вся рамка; `NodeType::Text` (`"text"` в TypeScript).

- [ ] **Step 1: Написать падающие тесты модели текста**

Создать `crates/ferrum-doc/tests/text.rs`:

````rust
//! Текстовый узел (подсистема 3, этап 1): содержимое в `LoroText` с разметкой,
//! расширение разметки на вставку, свойства текста в зеркале, отмена правок
//! текста обратной дельтой, пределы, файл и кэш сцены.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;

/// Правка текста внутри транзакции (для таблиц правок в тестах).
type Edit = Box<dyn Fn(&Tx) -> DocResult<()>>;

fn bold() -> TextAttrs {
    TextAttrs {
        style: Some("Bold".into()),
        ..Default::default()
    }
}

/// Документ с одним текстовым узлом «Привет, мир» (без разметки).
fn doc_with_text() -> (Document, PageId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_text_content(id, &TextContent::plain("Привет, мир"))?;
            Ok(vec![id])
        })
        .unwrap()[0];
    (doc, page, id)
}

fn content(doc: &mut Document, page: PageId, id: NodeId) -> TextContent {
    doc.page(page)
        .unwrap()
        .props(id)
        .unwrap()
        .text
        .unwrap()
        .content
        .clone()
}

fn texts(c: &TextContent) -> Vec<(&str, &TextAttrs)> {
    c.runs.iter().map(|r| (r.text.as_str(), &r.attrs)).collect()
}

#[test]
fn new_text_node_has_figma_defaults() {
    let (mut doc, page, id) = doc_with_text();
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.kind, NodeKind::Text);
    assert_eq!(p.name, "Текст");
    assert_eq!(p.fills, vec![Fill::solid(Color::BLACK)]);
    let t = p.text.unwrap();
    assert_eq!(t.props, TextProps::default());
    assert_eq!(t.props.style.font, FontName::new("Inter", "Regular"));
    assert_eq!(t.props.style.size, 12.0);
    assert_eq!(t.content.text(), "Привет, мир\n");
    assert_eq!(t.content.char_len(), 12);
}

#[test]
fn marks_expand_to_the_right_and_paragraph_marks_do_not() {
    let (mut doc, page, id) = doc_with_text();
    doc.transact(page, vec![], |tx| {
        // «Привет» — жирным; вставка сразу за ним наследует жирный (как в редакторах).
        tx.format_text(id, 0..6, &bold(), &[])?;
        tx.insert_text(id, 6, "!", None)?;
        // Список — на переводе строки абзаца; вставка перед ним список не получает.
        let list = TextAttrs {
            list: Some(ListKind::Bullet),
            indent: Some(1),
            ..Default::default()
        };
        tx.format_text(id, 12..13, &list, &[])?;
        tx.insert_text(id, 12, "?", None)?;
        Ok(vec![])
    })
    .unwrap();
    let c = content(&mut doc, page, id);
    assert_eq!(c.text(), "Привет!, мир?\n");
    let runs = texts(&c);
    assert_eq!(runs[0], ("Привет!", &bold()));
    assert_eq!(runs[1].0, ", мир?");
    assert!(runs[1].1.is_empty());
    assert_eq!(runs[2].0, "\n");
    assert_eq!(runs[2].1.list, Some(ListKind::Bullet));
    assert_eq!(runs[2].1.indent, Some(1));
}

#[test]
fn explicit_insert_attrs_replace_inherited_ones() {
    let (mut doc, page, id) = doc_with_text();
    let red = TextAttrs {
        fill: Some(Fill::solid(Color::rgba(255, 0, 0, 255))),
        ..Default::default()
    };
    doc.transact(page, vec![], |tx| {
        tx.format_text(id, 0..6, &bold(), &[])?;
        // Явная разметка вставки — ровно она, без унаследованного жирного.
        tx.insert_text(id, 6, "X", Some(&red))?;
        Ok(vec![])
    })
    .unwrap();
    let c = content(&mut doc, page, id);
    assert_eq!(texts(&c)[1], ("X", &red));
}

#[test]
fn every_text_edit_undoes_and_redoes_exactly() {
    let (mut doc, page, id) = doc_with_text();
    let start = content(&mut doc, page, id);
    let size = TextAttrs {
        size: Some(24.0),
        decoration: Some(Decoration::Underline),
        ..Default::default()
    };
    let edits: Vec<Edit> = vec![
        Box::new(move |tx| tx.format_text(id, 0..6, &bold(), &[])),
        Box::new(move |tx| tx.format_text(id, 2..9, &size, &[])),
        Box::new(move |tx| tx.insert_text(id, 3, "ёж", None)),
        Box::new(move |tx| tx.delete_text(id, 1..8)),
        Box::new(move |tx| tx.format_text(id, 0..4, &TextAttrs::default(), &[AttrKey::Style])),
        Box::new(move |tx| tx.set_text_content(id, &TextContent::plain("Новый\nтекст"))),
    ];
    let mut states = vec![start];
    for edit in &edits {
        doc.transact(page, vec![id], |tx| {
            edit(tx)?;
            Ok(vec![id])
        })
        .unwrap();
        states.push(content(&mut doc, page, id));
    }
    assert_eq!(states.last().unwrap().text(), "Новый\nтекст\n");
    for i in (0..edits.len()).rev() {
        doc.undo().unwrap();
        assert_eq!(content(&mut doc, page, id), states[i], "отмена шага {i}");
    }
    for (i, s) in states.iter().enumerate().skip(1) {
        doc.redo().unwrap();
        assert_eq!(&content(&mut doc, page, id), s, "повтор шага {i}");
    }
}

#[test]
fn text_survives_delete_and_undo_and_old_text_steps_still_undo() {
    let (mut doc, page, id) = doc_with_text();
    doc.transact(page, vec![], |tx| {
        tx.format_text(id, 0..6, &bold(), &[])?;
        Ok(vec![])
    })
    .unwrap();
    let styled = content(&mut doc, page, id);
    doc.transact(page, vec![], |tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
    assert!(!doc.page(page).unwrap().contains(id));
    // Восстановленный узел — с тем же текстом и разметкой (новый `LoroText`).
    doc.undo().unwrap();
    assert_eq!(content(&mut doc, page, id), styled);
    // Отмена правки текста, записанной до удаления, — уже на новом контейнере.
    doc.undo().unwrap();
    let plain = content(&mut doc, page, id);
    assert_eq!(plain.runs.len(), 1);
    assert_eq!(plain.text(), "Привет, мир\n");
}

#[test]
fn text_props_patch_writes_only_text_nodes() {
    let (mut doc, page, id) = doc_with_text();
    let props = TextProps {
        sizing: TextSizing::Fixed,
        align_h: AlignH::Center,
        max_lines: Some(2),
        truncate: true,
        style: TextStyle {
            font: FontName::new("Roboto", "Semi Bold Italic"),
            line_height: LineHeight::Percent(150.0),
            letter_spacing: LetterSpacing::Px(-0.5),
            ..TextStyle::default()
        },
        ..TextProps::default()
    };
    let rect = doc
        .transact(page, vec![], |tx| {
            tx.set_props(
                id,
                &PropsPatch {
                    text: Some(props.clone()),
                    ..Default::default()
                },
            )?;
            let r = tx.create_node(None, None, NodeKind::Rect)?;
            // Патч со свойствами текста у прямоугольника их не создаёт.
            tx.set_props(
                r,
                &PropsPatch {
                    text: Some(props.clone()),
                    x: Some(5.0),
                    ..Default::default()
                },
            )?;
            Ok(vec![r])
        })
        .unwrap()[0];
    let p = doc.page(page).unwrap();
    assert_eq!(p.props(id).unwrap().text_props(), Some(&props));
    assert_eq!(p.props(rect).unwrap().text, None);
    assert_eq!(p.props(rect).unwrap().x, 5.0);
    // Зеркало совпадает с честным чтением из Loro.
    let reloaded = PageDoc::from_snapshot(&p.export_snapshot().unwrap()).unwrap();
    assert_eq!(reloaded.props(id).unwrap(), p.props(id).unwrap());
    assert_eq!(reloaded.props(rect).unwrap().text, None);
    doc.undo().unwrap();
    let p = doc.page(page).unwrap();
    assert_eq!(
        p.props(id).unwrap().text_props(),
        Some(&TextProps::default())
    );
}

#[test]
fn limits_refuse_the_edit_without_a_step() {
    let (mut doc, page, id) = doc_with_text();
    let before = content(&mut doc, page, id);
    let steps = doc.can_undo();
    let long = "я".repeat(MAX_TEXT_CHARS);
    let err = doc.transact(page, vec![], |tx| {
        tx.insert_text(id, 0, &long, None)?;
        Ok(vec![])
    });
    assert!(matches!(err, Err(DocError::TextTooLong)), "{err:?}");
    assert_eq!(content(&mut doc, page, id), before);
    // 10 001 участок со стилем одной правкой: буквы попеременно 10 и 11 px.
    let runs: Vec<TextRun> = (0..=MAX_TEXT_RUNS)
        .map(|i| TextRun {
            text: "a".into(),
            attrs: TextAttrs {
                size: Some(10.0 + (i % 2) as f64),
                ..Default::default()
            },
        })
        .collect();
    let ok = content(&mut doc, page, id);
    let err = doc.transact(page, vec![], |tx| {
        tx.set_text_content(id, &TextContent { runs })?;
        Ok(vec![])
    });
    assert!(matches!(err, Err(DocError::TooManyTextRuns)), "{err:?}");
    assert_eq!(content(&mut doc, page, id), ok);
    assert!(steps);
}

#[test]
fn positions_outside_the_text_are_refused() {
    let (mut doc, page, id) = doc_with_text();
    for bad in [
        Box::new(move |tx: &Tx| tx.insert_text(id, 12, "x", None)) as Edit,
        Box::new(move |tx: &Tx| tx.delete_text(id, 5..12)),
        Box::new(move |tx: &Tx| tx.format_text(id, 3..13, &bold(), &[])),
    ] {
        let err = doc.transact(page, vec![], |tx| {
            bad(tx)?;
            Ok(vec![])
        });
        assert!(matches!(err, Err(DocError::TextRange(..))), "{err:?}");
    }
    let rect = doc
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    let err = doc.transact(page, vec![], |tx| {
        tx.insert_text(rect, 0, "x", None)?;
        Ok(vec![])
    });
    assert!(matches!(err, Err(DocError::NotText(_))), "{err:?}");
}

#[test]
fn text_survives_file_roundtrip_with_and_without_scene_cache() {
    let (mut doc, page, id) = doc_with_text();
    let red = Fill::solid(Color::rgba(200, 10, 10, 255));
    doc.transact(page, vec![], |tx| {
        tx.format_text(
            id,
            0..6,
            &TextAttrs {
                font: Some("Roboto".into()),
                style: Some("Bold Italic".into()),
                size: Some(20.0),
                fill: Some(red.clone()),
                line_height: Some(LineHeight::Px(30.0)),
                letter_spacing: Some(LetterSpacing::Percent(5.0)),
                decoration: Some(Decoration::Strike),
                case: Some(TextCase::Upper),
                ..Default::default()
            },
            &[],
        )?;
        tx.format_text(
            id,
            11..12,
            &TextAttrs {
                list: Some(ListKind::Ordered),
                indent: Some(2),
                ..Default::default()
            },
            &[],
        )?;
        tx.set_props(
            id,
            &PropsPatch {
                w: Some(120.0),
                h: Some(40.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let props = doc.page(page).unwrap().props(id).unwrap();
    let bytes = encode(&doc).unwrap();
    // С кэшем сцены: зеркало из таблицы, без чтения Loro.
    let mut cached = decode(&bytes).unwrap().document;
    assert!(cached.page(page).unwrap().loaded_from_cache());
    assert_eq!(cached.page(page).unwrap().props(id).unwrap(), props);
    // Без кэша (секция с чужим id): зеркало из Loro — то же самое.
    let no_cache = container::testing::map_sections(&bytes, |kind, sid, _| {
        if kind == 5 {
            *sid = "не-страница".into();
        }
    });
    let mut fresh = decode(&no_cache).unwrap().document;
    assert!(!fresh.page(page).unwrap().loaded_from_cache());
    assert_eq!(fresh.page(page).unwrap().props(id).unwrap(), props);
    // Открытый файл правится: расширение разметки задано и у загруженной страницы.
    fresh
        .transact(page, vec![], |tx| {
            tx.insert_text(id, 6, "!", None)?;
            Ok(vec![])
        })
        .unwrap();
    let c = content(&mut fresh, page, id);
    assert_eq!(c.runs[0].text, "Привет!");
    assert_eq!(c.runs[0].attrs.fill, Some(red));
}

#[test]
fn text_edits_go_through_the_journal() {
    let (mut doc, page, id) = doc_with_text();
    let file = encode(&doc).unwrap();
    doc.mark_journal();
    doc.transact(page, vec![], |tx| {
        tx.insert_text(id, 0, "Ну, ", None)?;
        tx.format_text(id, 0..3, &bold(), &[])?;
        Ok(vec![])
    })
    .unwrap();
    let batch = doc.take_journal().unwrap().unwrap();
    let mut restored = decode(&file).unwrap().document;
    let replayed = restored.apply_journal(&[&batch]);
    assert_eq!(replayed.applied, 1, "{:?}", replayed.warnings);
    assert_eq!(
        content(&mut restored, page, id),
        content(&mut doc, page, id)
    );
}
````

В тесте всех полей кэша сцены — узел текста со свойствами и разметкой (у не текста те же свойства текста пропускаются):

В `crates/ferrum-doc/tests/container.rs` заменить:

````rust
        NodeKind::Ellipse,
        NodeKind::Line,
        NodeKind::Image,
    ];
    doc.transact(page, vec![], |p| {
        let mut parent = None;
````

на:

````rust
        NodeKind::Ellipse,
        NodeKind::Line,
        NodeKind::Image,
        NodeKind::Text,
    ];
    doc.transact(page, vec![], |p| {
        let mut parent = None;
````

В `crates/ferrum-doc/tests/container.rs` заменить:

````rust
                            align: StrokeAlign::Outside,
                        },
                    ]),
                },
            )?;
            // Вложенность через раз: и корень, и дети.
            if i % 2 == 0 {
                parent = Some(id);
````

на:

````rust
                            align: StrokeAlign::Outside,
                        },
                    ]),
                    // У не текста пропускается; у текста — не умолчания.
                    text: Some(TextProps {
                        sizing: TextSizing::Fixed,
                        align_h: AlignH::Right,
                        align_v: AlignV::Bottom,
                        paragraph_spacing: f,
                        paragraph_indent: 2.0 * f,
                        truncate: true,
                        max_lines: Some(3),
                        style: TextStyle {
                            font: FontName::new("PT Serif", "Bold Italic"),
                            size: 17.5,
                            line_height: LineHeight::Px(21.0),
                            letter_spacing: LetterSpacing::Px(-0.25),
                            decoration: Decoration::Strike,
                            case: TextCase::SmallCaps,
                        },
                    }),
                },
            )?;
            if kind == NodeKind::Text {
                p.set_text_content(id, &TextContent::plain("Строка\nвторая"))?;
                p.format_text(
                    id,
                    0..3,
                    &TextAttrs {
                        size: Some(30.0),
                        fill: Some(Fill::solid(Color::rgba(1, 2, 3, 255))),
                        ..Default::default()
                    },
                    &[],
                )?;
            }
            // Вложенность через раз: и корень, и дети.
            if i % 2 == 0 {
                parent = Some(id);
````

В `crates/ferrum-doc/tests/flip.rs` заменить:

````rust

#[test]
fn scene_cache_version_covers_flips() {
    assert_eq!(SCENE_CACHE_VERSION, 2, "таблица с флагами отражения");
    // Кэш прежней версии с честным хэшем снапшота игнорируется без предупреждений.
    let (doc, page, [f, ..]) = sample();
    let bytes = testing::map_sections(&encode(&doc).unwrap(), |kind, _, data| {
````

на:

````rust

#[test]
fn scene_cache_version_covers_flips() {
    assert_eq!(
        SCENE_CACHE_VERSION, 3,
        "таблица с флагами отражения (2) и текстом (3)"
    );
    // Кэш прежней версии с честным хэшем снапшота игнорируется без предупреждений.
    let (doc, page, [f, ..]) = sample();
    let bytes = testing::map_sections(&encode(&doc).unwrap(), |kind, _, data| {
````

Run: `cargo test -p ferrum-doc --test text`
Expected: FAIL — не собирается: нет `NodeKind::Text`, `TextContent`, `TextAttrs`, `Tx::set_text_content`.

- [ ] **Step 2: Типы текста**

Создать `crates/ferrum-doc/src/text.rs`:

````rust
//! Текстовый узел (спека подсистемы 3, раздел 3): свойства узла, стиль символов
//! по умолчанию и содержимое — участки текста с разметкой.
//!
//! Содержимое в документе — свой `LoroText` на узел (ключ `text` его карты),
//! стили — разметка Loro на диапазонах. Позиции — в символах Unicode (скалярах):
//! так считает `LoroText` в нативной и wasm-сборке без фичи `wasm` у Loro.
//! Текст всегда кончается переводом строки: на нём лежит разметка последнего
//! абзаца (список, отступ), как в Quill; раскладка этот перевод строки не рисует.

use crate::types::{Color, Fill};

/// Предел символов текста узла (спека 3).
pub const MAX_TEXT_CHARS: usize = 100_000;
/// Предел участков с разметкой (спека 3).
pub const MAX_TEXT_RUNS: usize = 10_000;

/// Шрифт: семейство и начертание, как `fontName` в Figma («Inter», «Semi Bold Italic»).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontName {
    pub family: String,
    pub style: String,
}

impl FontName {
    pub fn new(family: impl Into<String>, style: impl Into<String>) -> FontName {
        FontName {
            family: family.into(),
            style: style.into(),
        }
    }

    /// Шрифт нового текста и запасной для отсутствующих — Inter Regular (как в Figma).
    pub fn inter_regular() -> FontName {
        FontName::new("Inter", "Regular")
    }
}

/// Режим размера текстового узла.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextSizing {
    /// Ширина и высота — по тексту, строки рвутся только переводами строк.
    AutoWidth,
    /// Ширина задана, высота — по тексту.
    AutoHeight,
    /// Оба размера заданы; лишнее обрезается (с многоточием, если `truncate`).
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignH {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignV {
    Top,
    Middle,
    Bottom,
}

/// Интерлиньяж: `Auto` — по метрикам шрифта.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    Auto,
    Px(f64),
    Percent(f64),
}

/// Трекинг: в пикселях или процентах кегля.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LetterSpacing {
    Px(f64),
    Percent(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decoration {
    None,
    Underline,
    Strike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextCase {
    None,
    Upper,
    Lower,
    Title,
    SmallCaps,
}

/// Список абзаца — разметка на его переводе строки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    None,
    Bullet,
    Ordered,
}

/// Наибольший уровень вложенности списка.
pub const MAX_LIST_INDENT: u8 = 5;

/// Стиль символов узла по умолчанию: ключи разметки, кроме заливки — у текста её
/// задаёт список `fills` узла (как в Figma: «Заливка» текстового слоя — цвет текста).
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub font: FontName,
    pub size: f64,
    pub line_height: LineHeight,
    pub letter_spacing: LetterSpacing,
    pub decoration: Decoration,
    pub case: TextCase,
}

impl Default for TextStyle {
    /// Умолчания Figma: Inter Regular 12, интерлиньяж «Авто», трекинг 0.
    fn default() -> Self {
        TextStyle {
            font: FontName::inter_regular(),
            size: 12.0,
            line_height: LineHeight::Auto,
            letter_spacing: LetterSpacing::Percent(0.0),
            decoration: Decoration::None,
            case: TextCase::None,
        }
    }
}

/// Свойства текстового узла, кроме содержимого.
#[derive(Debug, Clone, PartialEq)]
pub struct TextProps {
    pub sizing: TextSizing,
    pub align_h: AlignH,
    pub align_v: AlignV,
    pub paragraph_spacing: f64,
    /// Отступ первой строки абзаца.
    pub paragraph_indent: f64,
    /// Обрезка с многоточием (только при `Fixed` или `max_lines`).
    pub truncate: bool,
    /// Наибольшее число строк при обрезке; `None` — сколько поместится.
    pub max_lines: Option<u32>,
    pub style: TextStyle,
}

impl Default for TextProps {
    fn default() -> Self {
        TextProps {
            sizing: TextSizing::AutoWidth,
            align_h: AlignH::Left,
            align_v: AlignV::Top,
            paragraph_spacing: 0.0,
            paragraph_indent: 0.0,
            truncate: false,
            max_lines: None,
            style: TextStyle::default(),
        }
    }
}

/// Ключ разметки участка (имя ключа в Loro — `as_str`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttrKey {
    Font,
    Style,
    Size,
    Fill,
    LineHeight,
    LetterSpacing,
    Decoration,
    Case,
    List,
    Indent,
}

impl AttrKey {
    pub const ALL: [AttrKey; 10] = [
        AttrKey::Font,
        AttrKey::Style,
        AttrKey::Size,
        AttrKey::Fill,
        AttrKey::LineHeight,
        AttrKey::LetterSpacing,
        AttrKey::Decoration,
        AttrKey::Case,
        AttrKey::List,
        AttrKey::Indent,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AttrKey::Font => "font",
            AttrKey::Style => "style",
            AttrKey::Size => "size",
            AttrKey::Fill => "fill",
            AttrKey::LineHeight => "lineHeight",
            AttrKey::LetterSpacing => "letterSpacing",
            AttrKey::Decoration => "decoration",
            AttrKey::Case => "case",
            AttrKey::List => "list",
            AttrKey::Indent => "indent",
        }
    }

    /// Разметка абзаца (лежит на переводе строки) — не расширяется на вставку
    /// рядом; разметка символов наследуется вставкой на границе слева.
    pub fn is_paragraph(self) -> bool {
        matches!(self, AttrKey::List | AttrKey::Indent)
    }
}

/// Разметка участка: `None` — как у узла (или «не задано» у абзаца).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextAttrs {
    pub font: Option<String>,
    pub style: Option<String>,
    pub size: Option<f64>,
    pub fill: Option<Fill>,
    pub line_height: Option<LineHeight>,
    pub letter_spacing: Option<LetterSpacing>,
    pub decoration: Option<Decoration>,
    pub case: Option<TextCase>,
    pub list: Option<ListKind>,
    pub indent: Option<u8>,
}

impl TextAttrs {
    pub fn is_empty(&self) -> bool {
        *self == TextAttrs::default()
    }

    /// Задан ли ключ.
    pub fn has(&self, key: AttrKey) -> bool {
        match key {
            AttrKey::Font => self.font.is_some(),
            AttrKey::Style => self.style.is_some(),
            AttrKey::Size => self.size.is_some(),
            AttrKey::Fill => self.fill.is_some(),
            AttrKey::LineHeight => self.line_height.is_some(),
            AttrKey::LetterSpacing => self.letter_spacing.is_some(),
            AttrKey::Decoration => self.decoration.is_some(),
            AttrKey::Case => self.case.is_some(),
            AttrKey::List => self.list.is_some(),
            AttrKey::Indent => self.indent.is_some(),
        }
    }

    /// Шрифт участка с учётом умолчания узла.
    pub fn font_name(&self, default: &TextStyle) -> FontName {
        FontName::new(
            self.font.as_deref().unwrap_or(&default.font.family),
            self.style.as_deref().unwrap_or(&default.font.style),
        )
    }
}

/// Участок текста с одной разметкой.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub attrs: TextAttrs,
}

/// Содержимое текстового узла: участки по порядку. Соседние участки с одной
/// разметкой слиты, пустых нет; текст кончается переводом строки.
#[derive(Debug, Clone, PartialEq)]
pub struct TextContent {
    pub runs: Vec<TextRun>,
}

impl Default for TextContent {
    fn default() -> Self {
        TextContent::plain("")
    }
}

impl TextContent {
    /// Текст без разметки; перевод строки в конце добавляется, если его нет.
    pub fn plain(text: &str) -> TextContent {
        let mut s = text.to_string();
        if !s.ends_with('\n') {
            s.push('\n');
        }
        TextContent {
            runs: vec![TextRun {
                text: s,
                attrs: TextAttrs::default(),
            }],
        }
    }

    /// Текст целиком (с последним переводом строки).
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Длина в символах Unicode.
    pub fn char_len(&self) -> usize {
        self.runs.iter().map(|r| r.text.chars().count()).sum()
    }

    /// Участки с разметкой (для предела `MAX_TEXT_RUNS`).
    pub fn styled_runs(&self) -> usize {
        self.runs.iter().filter(|r| !r.attrs.is_empty()).count()
    }

    /// Все шрифты содержимого с учётом умолчания узла, без повторов, по порядку.
    pub fn fonts(&self, default: &TextStyle) -> Vec<FontName> {
        let mut out: Vec<FontName> = Vec::new();
        for r in &self.runs {
            let f = r.attrs.font_name(default);
            if !out.contains(&f) {
                out.push(f);
            }
        }
        if out.is_empty() {
            out.push(default.font.clone());
        }
        out
    }

    /// Границы участков в символах, по порядку.
    pub fn run_ranges(&self) -> Vec<std::ops::Range<usize>> {
        let mut at = 0;
        self.runs
            .iter()
            .map(|r| {
                let n = r.text.chars().count();
                let range = at..at + n;
                at += n;
                range
            })
            .collect()
    }
}

/// Текст узла целиком: свойства и содержимое. В свойствах узла (`Props::text`)
/// лежит за `Arc`: копии свойств (зеркало, сцена, превью жеста) не копируют
/// сотни килобайт текста.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextData {
    pub props: TextProps,
    pub content: TextContent,
}

/// Цвет текста по умолчанию в Figma — чёрный.
pub const TEXT_COLOR: Color = Color::BLACK;
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust

use std::fmt;
use std::str::FromStr;

use loro::TreeID;

use crate::error::DocError;

/// Идентификатор узла. Обёртка над `TreeID` Loro: уникален и при совместной работе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
````

на:

````rust

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use loro::TreeID;

use crate::error::DocError;
use crate::text::{TextData, TextProps, TEXT_COLOR};

/// Идентификатор узла. Обёртка над `TreeID` Loro: уникален и при совместной работе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    Ellipse,
    Line,
    Image,
}

impl NodeKind {
````

на:

````rust
    Ellipse,
    Line,
    Image,
    Text,
}

impl NodeKind {
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            NodeKind::Ellipse => "ellipse",
            NodeKind::Line => "line",
            NodeKind::Image => "image",
        }
    }

````

на:

````rust
            NodeKind::Ellipse => "ellipse",
            NodeKind::Line => "line",
            NodeKind::Image => "image",
            NodeKind::Text => "text",
        }
    }

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            "ellipse" => NodeKind::Ellipse,
            "line" => NodeKind::Line,
            "image" => NodeKind::Image,
            _ => return None,
        })
    }
````

на:

````rust
            "ellipse" => NodeKind::Ellipse,
            "line" => NodeKind::Line,
            "image" => NodeKind::Image,
            "text" => NodeKind::Text,
            _ => return None,
        })
    }
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            NodeKind::Ellipse => "Эллипс",
            NodeKind::Line => "Линия",
            NodeKind::Image => "Изображение",
        }
    }
}
````

на:

````rust
            NodeKind::Ellipse => "Эллипс",
            NodeKind::Line => "Линия",
            NodeKind::Image => "Изображение",
            NodeKind::Text => "Текст",
        }
    }
}
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    pub flip_y: bool,
    pub fills: Vec<Fill>,
    pub strokes: Vec<Stroke>,
}

impl Props {
````

на:

````rust
    pub flip_y: bool,
    pub fills: Vec<Fill>,
    pub strokes: Vec<Stroke>,
    /// Текст и его свойства — только у `NodeKind::Text`.
    pub text: Option<Arc<TextData>>,
}

impl Props {
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
                }],
            ),
            NodeKind::Image => (100.0, 100.0, vec![], vec![]),
        };
        Props {
            kind,
````

на:

````rust
                }],
            ),
            NodeKind::Image => (100.0, 100.0, vec![], vec![]),
            // Размер пустого текста ставит редактор по раскладке; цвет — чёрный.
            NodeKind::Text => (0.0, 0.0, vec![Fill::solid(TEXT_COLOR)], vec![]),
        };
        Props {
            kind,
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            flip_y: false,
            fills,
            strokes,
        }
    }
}

````

на:

````rust
            flip_y: false,
            fills,
            strokes,
            text: (kind == NodeKind::Text).then(|| Arc::new(TextData::default())),
        }
    }

    /// Свойства текста (у не текста — `None`).
    pub fn text_props(&self) -> Option<&TextProps> {
        self.text.as_deref().map(|t| &t.props)
    }
}

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    pub flip_y: Option<bool>,
    pub fills: Option<Vec<Fill>>,
    pub strokes: Option<Vec<Stroke>>,
}

impl PropsPatch {
````

на:

````rust
    pub flip_y: Option<bool>,
    pub fills: Option<Vec<Fill>>,
    pub strokes: Option<Vec<Stroke>>,
    /// Свойства текста целиком (содержимое правится `Tx::insert_text` и соседями).
    /// У не текста пропускается.
    pub text: Option<TextProps>,
}

impl PropsPatch {
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            flip_y: self.flip_y.map(|_| before.flip_y),
            fills: self.fills.as_ref().map(|_| before.fills.clone()),
            strokes: self.strokes.as_ref().map(|_| before.strokes.clone()),
        }
    }

````

на:

````rust
            flip_y: self.flip_y.map(|_| before.flip_y),
            fills: self.fills.as_ref().map(|_| before.fills.clone()),
            strokes: self.strokes.as_ref().map(|_| before.strokes.clone()),
            text: match (&self.text, &before.text) {
                (Some(_), Some(t)) => Some(t.props.clone()),
                _ => None,
            },
        }
    }

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
        if let Some(v) = &self.strokes {
            p.strokes = v.clone();
        }
    }
}
````

на:

````rust
        if let Some(v) = &self.strokes {
            p.strokes = v.clone();
        }
        if let (Some(v), Some(t)) = (&self.text, &mut p.text) {
            Arc::make_mut(t).props = v.clone();
        }
    }
}
````

В `crates/ferrum-doc/src/error.rs` заменить:

````rust
    UnsupportedFormat(u32),
    #[error("Файл повреждён: {0}")]
    Corrupted(String),
    #[error("Ошибка Loro: {0}")]
    Loro(#[from] loro::LoroError),
    #[error("Ошибка экспорта Loro: {0}")]
````

на:

````rust
    UnsupportedFormat(u32),
    #[error("Файл повреждён: {0}")]
    Corrupted(String),
    #[error("Узел {0} — не текст")]
    NotText(String),
    #[error("Текст длиннее 100 000 символов — правка не применена")]
    TextTooLong,
    #[error("В тексте больше 10 000 участков со стилями — правка не применена")]
    TooManyTextRuns,
    #[error("Позиция {0} вне текста длиной {1}")]
    TextRange(usize, usize),
    #[error("Ошибка Loro: {0}")]
    Loro(#[from] loro::LoroError),
    #[error("Ошибка экспорта Loro: {0}")]
````

- [ ] **Step 3: Текст в Loro: свойства, разметка, содержимое, обратная дельта**

Создать `crates/ferrum-doc/src/text_codec.rs`:

````rust
//! Текст узла в Loro: свойства текста — ключи карты узла (как остальные
//! свойства), содержимое — дочерний `LoroText` с разметкой, правки — дельты Loro
//! (формат Quill) и обратные к ним для отмены.
//!
//! `FxHashMap` атрибутов дельты — тот же тип, что в `loro::TextDelta`; он
//! публично реэкспортируется `loro-internal` (это не внутренний API).

use loro::{
    Container, ExpandType, LoroMap, LoroText, LoroValue, StyleConfig, StyleConfigMap, TextDelta,
    ValueOrContainer,
};
use loro_internal::FxHashMap;

use crate::codec::{as_bool, as_f64, as_str, field, fill_value, obj, read_fill, value};
use crate::error::{DocError, DocResult};
use crate::text::*;

/// Ключ дочернего `LoroText` в карте текстового узла.
pub(crate) const KEY_TEXT: &str = "text";

pub(crate) mod key {
    pub const SIZING: &str = "sizing";
    pub const ALIGN_H: &str = "alignH";
    pub const ALIGN_V: &str = "alignV";
    pub const PARAGRAPH_SPACING: &str = "paragraphSpacing";
    pub const PARAGRAPH_INDENT: &str = "paragraphIndent";
    pub const TRUNCATE: &str = "truncate";
    pub const MAX_LINES: &str = "maxLines";
    // Умолчания стиля — те же ключи, что у разметки.
    pub const FONT: &str = "font";
    pub const STYLE: &str = "style";
    pub const SIZE: &str = "size";
    pub const LINE_HEIGHT: &str = "lineHeight";
    pub const LETTER_SPACING: &str = "letterSpacing";
    pub const DECORATION: &str = "decoration";
    pub const CASE: &str = "case";
}

/// Как разметка расширяется при вставке на границе (спека 3): символьная
/// наследуется вставкой слева (`After`, как в текстовых редакторах), разметка
/// абзаца на переводе строки не расширяется. Настройка не хранится в документе —
/// её ставит каждый `LoroDoc` страницы (`PageDoc::wrap`).
pub(crate) fn style_config() -> StyleConfigMap {
    let mut map = StyleConfigMap::new();
    for k in AttrKey::ALL {
        let expand = if k.is_paragraph() {
            ExpandType::None
        } else {
            ExpandType::After
        };
        map.insert(k.as_str().into(), StyleConfig { expand });
    }
    map
}

fn sizing_str(v: TextSizing) -> &'static str {
    match v {
        TextSizing::AutoWidth => "autoWidth",
        TextSizing::AutoHeight => "autoHeight",
        TextSizing::Fixed => "fixed",
    }
}

fn align_h_str(v: AlignH) -> &'static str {
    match v {
        AlignH::Left => "left",
        AlignH::Center => "center",
        AlignH::Right => "right",
        AlignH::Justify => "justify",
    }
}

fn align_v_str(v: AlignV) -> &'static str {
    match v {
        AlignV::Top => "top",
        AlignV::Middle => "middle",
        AlignV::Bottom => "bottom",
    }
}

fn decoration_str(v: Decoration) -> &'static str {
    match v {
        Decoration::None => "none",
        Decoration::Underline => "underline",
        Decoration::Strike => "strike",
    }
}

fn case_str(v: TextCase) -> &'static str {
    match v {
        TextCase::None => "none",
        TextCase::Upper => "upper",
        TextCase::Lower => "lower",
        TextCase::Title => "title",
        TextCase::SmallCaps => "smallCaps",
    }
}

fn list_str(v: ListKind) -> &'static str {
    match v {
        ListKind::None => "none",
        ListKind::Bullet => "bullet",
        ListKind::Ordered => "ordered",
    }
}

fn parse_sizing(s: &str) -> Option<TextSizing> {
    Some(match s {
        "autoWidth" => TextSizing::AutoWidth,
        "autoHeight" => TextSizing::AutoHeight,
        "fixed" => TextSizing::Fixed,
        _ => return None,
    })
}

fn parse_align_h(s: &str) -> Option<AlignH> {
    Some(match s {
        "left" => AlignH::Left,
        "center" => AlignH::Center,
        "right" => AlignH::Right,
        "justify" => AlignH::Justify,
        _ => return None,
    })
}

fn parse_align_v(s: &str) -> Option<AlignV> {
    Some(match s {
        "top" => AlignV::Top,
        "middle" => AlignV::Middle,
        "bottom" => AlignV::Bottom,
        _ => return None,
    })
}

fn parse_decoration(s: &str) -> Option<Decoration> {
    Some(match s {
        "none" => Decoration::None,
        "underline" => Decoration::Underline,
        "strike" => Decoration::Strike,
        _ => return None,
    })
}

fn parse_case(s: &str) -> Option<TextCase> {
    Some(match s {
        "none" => TextCase::None,
        "upper" => TextCase::Upper,
        "lower" => TextCase::Lower,
        "title" => TextCase::Title,
        "smallCaps" => TextCase::SmallCaps,
        _ => return None,
    })
}

fn parse_list(s: &str) -> Option<ListKind> {
    Some(match s {
        "none" => ListKind::None,
        "bullet" => ListKind::Bullet,
        "ordered" => ListKind::Ordered,
        _ => return None,
    })
}

/// `"auto"` или `{ unit: "px" | "percent", value }`.
fn line_height_value(v: LineHeight) -> LoroValue {
    match v {
        LineHeight::Auto => LoroValue::from("auto"),
        LineHeight::Px(px) => obj(vec![("unit", "px".into()), ("value", px.into())]),
        LineHeight::Percent(p) => obj(vec![("unit", "percent".into()), ("value", p.into())]),
    }
}

fn read_line_height(v: &LoroValue) -> Option<LineHeight> {
    if as_str(v).as_deref() == Some("auto") {
        return Some(LineHeight::Auto);
    }
    let value = field(v, "value")
        .and_then(as_f64)
        .filter(|x| x.is_finite())?;
    match field(v, "unit").and_then(as_str).as_deref() {
        Some("px") => Some(LineHeight::Px(value)),
        Some("percent") => Some(LineHeight::Percent(value)),
        _ => None,
    }
}

fn letter_spacing_value(v: LetterSpacing) -> LoroValue {
    match v {
        LetterSpacing::Px(px) => obj(vec![("unit", "px".into()), ("value", px.into())]),
        LetterSpacing::Percent(p) => obj(vec![("unit", "percent".into()), ("value", p.into())]),
    }
}

fn read_letter_spacing(v: &LoroValue) -> Option<LetterSpacing> {
    let value = field(v, "value")
        .and_then(as_f64)
        .filter(|x| x.is_finite())?;
    match field(v, "unit").and_then(as_str).as_deref() {
        Some("px") => Some(LetterSpacing::Px(value)),
        Some("percent") => Some(LetterSpacing::Percent(value)),
        _ => None,
    }
}

/// Свойства текста ключами карты узла; `None` — ключ удаляется (`maxLines`).
fn text_props_entries(p: &TextProps) -> Vec<(&'static str, Option<LoroValue>)> {
    let s = &p.style;
    vec![
        (key::SIZING, Some(sizing_str(p.sizing).into())),
        (key::ALIGN_H, Some(align_h_str(p.align_h).into())),
        (key::ALIGN_V, Some(align_v_str(p.align_v).into())),
        (key::PARAGRAPH_SPACING, Some(p.paragraph_spacing.into())),
        (key::PARAGRAPH_INDENT, Some(p.paragraph_indent.into())),
        (key::TRUNCATE, Some(p.truncate.into())),
        (key::MAX_LINES, p.max_lines.map(|n| (n as i64).into())),
        (key::FONT, Some(s.font.family.as_str().into())),
        (key::STYLE, Some(s.font.style.as_str().into())),
        (key::SIZE, Some(s.size.into())),
        (key::LINE_HEIGHT, Some(line_height_value(s.line_height))),
        (
            key::LETTER_SPACING,
            Some(letter_spacing_value(s.letter_spacing)),
        ),
        (key::DECORATION, Some(decoration_str(s.decoration).into())),
        (key::CASE, Some(case_str(s.case).into())),
    ]
}

/// Записывает свойства текста целиком. `maxLines` без значения — ключ удаляется.
pub(crate) fn write_text_props(map: &LoroMap, p: &TextProps) -> DocResult<()> {
    for (k, v) in text_props_entries(p) {
        match v {
            Some(v) => map.insert(k, v)?,
            None => map.delete(k)?,
        }
    }
    Ok(())
}

/// Свойства текста из карты узла; отсутствующие и неверные — умолчания.
pub(crate) fn read_text_props(map: &LoroMap) -> TextProps {
    read_text_props_with(|k| value(map, k))
}

fn read_text_props_with(get: impl Fn(&str) -> Option<LoroValue>) -> TextProps {
    let d = TextProps::default();
    let s = |k: &str| get(k).as_ref().and_then(as_str);
    let num = |k: &str| get(k).as_ref().and_then(as_f64).filter(|v| v.is_finite());
    TextProps {
        sizing: s(key::SIZING)
            .and_then(|v| parse_sizing(&v))
            .unwrap_or(d.sizing),
        align_h: s(key::ALIGN_H)
            .and_then(|v| parse_align_h(&v))
            .unwrap_or(d.align_h),
        align_v: s(key::ALIGN_V)
            .and_then(|v| parse_align_v(&v))
            .unwrap_or(d.align_v),
        paragraph_spacing: num(key::PARAGRAPH_SPACING).unwrap_or(d.paragraph_spacing),
        paragraph_indent: num(key::PARAGRAPH_INDENT).unwrap_or(d.paragraph_indent),
        truncate: get(key::TRUNCATE)
            .as_ref()
            .and_then(as_bool)
            .unwrap_or(d.truncate),
        max_lines: num(key::MAX_LINES)
            .filter(|v| *v >= 1.0)
            .map(|v| v.min(u32::MAX as f64) as u32),
        style: TextStyle {
            font: FontName::new(
                s(key::FONT).unwrap_or(d.style.font.family),
                s(key::STYLE).unwrap_or(d.style.font.style),
            ),
            size: num(key::SIZE).filter(|v| *v > 0.0).unwrap_or(d.style.size),
            line_height: get(key::LINE_HEIGHT)
                .as_ref()
                .and_then(read_line_height)
                .unwrap_or(d.style.line_height),
            letter_spacing: get(key::LETTER_SPACING)
                .as_ref()
                .and_then(read_letter_spacing)
                .unwrap_or(d.style.letter_spacing),
            decoration: s(key::DECORATION)
                .and_then(|v| parse_decoration(&v))
                .unwrap_or(d.style.decoration),
            case: s(key::CASE)
                .and_then(|v| parse_case(&v))
                .unwrap_or(d.style.case),
        },
    }
}

/// Разметка участка в атрибуты Loro (только заданные ключи).
pub(crate) fn attrs_to_loro(a: &TextAttrs) -> FxHashMap<String, LoroValue> {
    let mut m = FxHashMap::default();
    let mut put = |k: AttrKey, v: LoroValue| {
        m.insert(k.as_str().to_string(), v);
    };
    if let Some(v) = &a.font {
        put(AttrKey::Font, v.as_str().into());
    }
    if let Some(v) = &a.style {
        put(AttrKey::Style, v.as_str().into());
    }
    if let Some(v) = a.size {
        put(AttrKey::Size, v.into());
    }
    if let Some(v) = &a.fill {
        put(AttrKey::Fill, fill_value(v));
    }
    if let Some(v) = a.line_height {
        put(AttrKey::LineHeight, line_height_value(v));
    }
    if let Some(v) = a.letter_spacing {
        put(AttrKey::LetterSpacing, letter_spacing_value(v));
    }
    if let Some(v) = a.decoration {
        put(AttrKey::Decoration, decoration_str(v).into());
    }
    if let Some(v) = a.case {
        put(AttrKey::Case, case_str(v).into());
    }
    if let Some(v) = a.list {
        put(AttrKey::List, list_str(v).into());
    }
    if let Some(v) = a.indent {
        put(AttrKey::Indent, (v as i64).into());
    }
    m
}

/// Атрибуты Loro в разметку. Неизвестные ключи (из будущих версий) и неверные
/// значения пропускаются; `null` — ключ снят.
pub(crate) fn attrs_from_loro<'a>(
    pairs: impl Iterator<Item = (&'a str, &'a LoroValue)>,
) -> TextAttrs {
    let mut a = TextAttrs::default();
    for (k, v) in pairs {
        match k {
            "font" => a.font = as_str(v),
            "style" => a.style = as_str(v),
            "size" => a.size = as_f64(v).filter(|x| x.is_finite() && *x > 0.0),
            "fill" => a.fill = read_fill(v),
            "lineHeight" => a.line_height = read_line_height(v),
            "letterSpacing" => a.letter_spacing = read_letter_spacing(v),
            "decoration" => a.decoration = as_str(v).and_then(|s| parse_decoration(&s)),
            "case" => a.case = as_str(v).and_then(|s| parse_case(&s)),
            "list" => a.list = as_str(v).and_then(|s| parse_list(&s)),
            "indent" => {
                a.indent = as_f64(v)
                    .filter(|x| (0.0..=MAX_LIST_INDENT as f64).contains(x))
                    .map(|x| x as u8)
            }
            _ => {}
        }
    }
    a
}

/// Дочерний `LoroText` узла, если он есть.
pub(crate) fn existing_text(map: &LoroMap) -> Option<LoroText> {
    match map.get(KEY_TEXT)? {
        ValueOrContainer::Container(Container::Text(t)) => Some(t),
        _ => None,
    }
}

/// Дочерний `LoroText` узла; нет — создаётся пустым (обычный дочерний
/// контейнер: текст создаёт тот же, кто создаёт узел).
pub(crate) fn text_container(map: &LoroMap) -> DocResult<LoroText> {
    match existing_text(map) {
        Some(t) => Ok(t),
        None => Ok(map.insert_container(KEY_TEXT, LoroText::new())?),
    }
}

/// Содержимое из `LoroText`: участки по порядку, соседние с одной разметкой слиты.
/// Текст без перевода строки в конце (повреждённый или чужой) получает его.
pub(crate) fn read_content(text: &LoroText) -> TextContent {
    let mut runs: Vec<TextRun> = Vec::new();
    if let LoroValue::List(items) = text.get_richtext_value() {
        for item in items.iter() {
            let Some(s) = field(item, "insert").and_then(as_str) else {
                continue;
            };
            if s.is_empty() {
                continue;
            }
            let attrs = match field(item, "attributes") {
                Some(LoroValue::Map(m)) => attrs_from_loro(m.iter().map(|(k, v)| (k.as_str(), v))),
                _ => TextAttrs::default(),
            };
            push_run(&mut runs, s, attrs);
        }
    }
    let mut content = TextContent { runs };
    if !content.text().ends_with('\n') {
        push_run(&mut content.runs, "\n".to_string(), TextAttrs::default());
    }
    content
}

fn push_run(runs: &mut Vec<TextRun>, text: String, attrs: TextAttrs) {
    match runs.last_mut() {
        Some(last) if last.attrs == attrs => last.text.push_str(&text),
        _ => runs.push(TextRun { text, attrs }),
    }
}

/// Дельта, вставляющая содержимое в начало текста с точной разметкой каждого участка.
pub(crate) fn insert_delta(content: &TextContent) -> Vec<TextDelta> {
    content
        .runs
        .iter()
        .map(|r| TextDelta::Insert {
            insert: r.text.clone(),
            attributes: Some(attrs_to_loro(&r.attrs)),
        })
        .collect()
}

fn delta_len(s: &str) -> usize {
    s.chars().count()
}

/// Обратная дельта: применённая после `delta`, возвращает текст к нынешнему
/// состоянию (Quill `invert`). Позиции и стиль удаляемого и переразмечаемого
/// берутся из текущего текста, поэтому вызывать — до применения `delta`.
pub(crate) fn invert(text: &LoroText, delta: &[TextDelta]) -> DocResult<Vec<TextDelta>> {
    let len = text.len_unicode();
    let mut out = Vec::new();
    let mut at = 0usize;
    for op in delta {
        match op {
            TextDelta::Retain { retain, attributes } => {
                let end = at
                    .checked_add(*retain)
                    .filter(|e| *e <= len)
                    .ok_or(DocError::TextRange(at.saturating_add(*retain), len))?;
                match attributes {
                    Some(attrs) if !attrs.is_empty() => {
                        for piece in text.slice_delta(at, end, loro::cursor::PosType::Unicode)? {
                            let TextDelta::Insert {
                                insert,
                                attributes: before,
                            } = piece
                            else {
                                continue;
                            };
                            let mut prev = FxHashMap::default();
                            for k in attrs.keys() {
                                let old = before
                                    .as_ref()
                                    .and_then(|b| b.get(k))
                                    .cloned()
                                    .unwrap_or(LoroValue::Null);
                                prev.insert(k.clone(), old);
                            }
                            out.push(TextDelta::Retain {
                                retain: delta_len(&insert),
                                attributes: Some(prev),
                            });
                        }
                    }
                    _ => out.push(TextDelta::Retain {
                        retain: *retain,
                        attributes: None,
                    }),
                }
                at = end;
            }
            TextDelta::Insert { insert, .. } => out.push(TextDelta::Delete {
                delete: delta_len(insert),
            }),
            TextDelta::Delete { delete } => {
                let end = at
                    .checked_add(*delete)
                    .filter(|e| *e <= len)
                    .ok_or(DocError::TextRange(at.saturating_add(*delete), len))?;
                for piece in text.slice_delta(at, end, loro::cursor::PosType::Unicode)? {
                    if let TextDelta::Insert { insert, attributes } = piece {
                        out.push(TextDelta::Insert {
                            insert,
                            attributes: Some(attributes.unwrap_or_default()),
                        });
                    }
                }
                at = end;
            }
        }
    }
    Ok(out)
}
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
//! чтобы одновременные правки разных свойств одного узла не затирали друг друга.

use std::collections::HashMap;

use loro::{Container, LoroMap, LoroValue, ValueOrContainer};

use crate::error::{DocError, DocResult};
use crate::types::*;

pub(crate) mod key {
````

на:

````rust
//! чтобы одновременные правки разных свойств одного узла не затирали друг друга.

use std::collections::HashMap;
use std::sync::Arc;

use loro::{Container, LoroMap, LoroValue, ValueOrContainer};

use crate::error::{DocError, DocResult};
use crate::text::TextData;
use crate::text_codec;
use crate::types::*;

pub(crate) mod key {
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
    pub const STROKES: &str = "strokes";
}

fn value(map: &LoroMap, k: &str) -> Option<LoroValue> {
    match map.get(k)? {
        ValueOrContainer::Value(v) => Some(v),
        ValueOrContainer::Container(Container::List(l)) => Some(l.get_deep_value()),
````

на:

````rust
    pub const STROKES: &str = "strokes";
}

pub(crate) fn value(map: &LoroMap, k: &str) -> Option<LoroValue> {
    match map.get(k)? {
        ValueOrContainer::Value(v) => Some(v),
        ValueOrContainer::Container(Container::List(l)) => Some(l.get_deep_value()),
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
    }
}

fn as_f64(v: &LoroValue) -> Option<f64> {
    match v {
        LoroValue::Double(d) => Some(*d),
        LoroValue::I64(i) => Some(*i as f64),
````

на:

````rust
    }
}

pub(crate) fn as_f64(v: &LoroValue) -> Option<f64> {
    match v {
        LoroValue::Double(d) => Some(*d),
        LoroValue::I64(i) => Some(*i as f64),
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
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
````

на:

````rust
    }
}

pub(crate) fn as_bool(v: &LoroValue) -> Option<bool> {
    match v {
        LoroValue::Bool(b) => Some(*b),
        _ => None,
    }
}

pub(crate) fn as_str(v: &LoroValue) -> Option<String> {
    match v {
        LoroValue::String(s) => Some(s.to_string()),
        _ => None,
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
            Some(v) => read_list(&v, read_stroke),
            None => d.strokes,
        },
    })
}

````

на:

````rust
            Some(v) => read_list(&v, read_stroke),
            None => d.strokes,
        },
        text: (kind == NodeKind::Text).then(|| {
            Arc::new(TextData {
                props: text_codec::read_text_props(map),
                content: text_codec::existing_text(map)
                    .map(|t| text_codec::read_content(&t))
                    .unwrap_or_default(),
            })
        }),
    })
}

````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
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
````

на:

````rust
    }
}

pub(crate) fn field<'a>(v: &'a LoroValue, k: &str) -> Option<&'a LoroValue> {
    match v {
        LoroValue::Map(m) => m.get(k),
        _ => None,
    }
}

pub(crate) fn read_fill(v: &LoroValue) -> Option<Fill> {
    let kind = match field(v, "kind").and_then(as_str)?.as_str() {
        "solid" => FillKind::Solid,
        "image" => FillKind::Image,
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
        flip_y: p.flip_y.then_some(true),
        fills: Some(p.fills.clone()),
        strokes: Some(p.strokes.clone()),
    };
    write_patch(map, &patch)
}

pub(crate) fn write_patch(map: &LoroMap, p: &PropsPatch) -> DocResult<()> {
````

на:

````rust
        flip_y: p.flip_y.then_some(true),
        fills: Some(p.fills.clone()),
        strokes: Some(p.strokes.clone()),
        text: p.text.as_ref().map(|t| t.props.clone()),
    };
    write_patch(map, &patch)?;
    // Содержимое — в новый `LoroText` узла: узел только что создан (или
    // восстановлен отменой удаления под новым `TreeID`), контейнер пуст.
    if let Some(t) = &p.text {
        let text = text_codec::text_container(map)?;
        text.apply_delta(&text_codec::insert_delta(&t.content))?;
    }
    Ok(())
}

pub(crate) fn write_patch(map: &LoroMap, p: &PropsPatch) -> DocResult<()> {
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
            LoroValue::from(strokes.iter().map(stroke_value).collect::<Vec<_>>()),
        )?;
    }
    Ok(())
}

fn obj(fields: Vec<(&str, LoroValue)>) -> LoroValue {
    LoroValue::from(
        fields
            .into_iter()
````

на:

````rust
            LoroValue::from(strokes.iter().map(stroke_value).collect::<Vec<_>>()),
        )?;
    }
    // Вызывающий убирает `text` из патча узла не текста (`PageDoc::set_props`).
    if let Some(text) = &p.text {
        text_codec::write_text_props(map, text)?;
    }
    Ok(())
}

pub(crate) fn obj(fields: Vec<(&str, LoroValue)>) -> LoroValue {
    LoroValue::from(
        fields
            .into_iter()
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
    )
}

fn fill_value(f: &Fill) -> LoroValue {
    let mut fields = vec![
        (
            "kind",
````

на:

````rust
    )
}

pub(crate) fn fill_value(f: &Fill) -> LoroValue {
    let mut fields = vec![
        (
            "kind",
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
mod loro_ext;
mod page;
mod scene_cache;
mod types;

pub use changes::Changes;
````

на:

````rust
mod loro_ext;
mod page;
mod scene_cache;
mod text;
mod text_codec;
mod types;

pub use changes::Changes;
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
pub use journal::Replayed;
pub use page::{PageDoc, Tx};
pub use scene_cache::SceneRow;
pub use types::*;

#[doc(hidden)]
````

на:

````rust
pub use journal::Replayed;
pub use page::{PageDoc, Tx};
pub use scene_cache::SceneRow;
pub use text::*;
pub use types::*;

#[doc(hidden)]
````

- [ ] **Step 4: Правки текста на странице, отмена обратной дельтой**

Каждый `LoroDoc` страницы получает настройку расширения разметки (`wrap`); `set_props` не пишет свойства текста узлу не текста; правки текста — через `edit_text` с пределами и записью `Inverse::Text`. `Inverse::Props` хранит патч за `Box`: с `text` патч стал крупным, а clippy (`large_enum_variant`) требует ровных вариантов.

В `crates/ferrum-doc/src/page.rs` заменить:

````rust

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use loro::{
    ExportMode, LoroDoc, LoroTree, LoroValue, TreeID, TreeParentId, ValueOrContainer, VersionVector,
};

use crate::codec;
use crate::error::{DocError, DocResult};
use crate::loro_ext;
use crate::scene_cache::SceneRow;
use crate::types::*;

const TREE: &str = "nodes";
````

на:

````rust

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use loro::{
    ExportMode, LoroDoc, LoroText, LoroTree, LoroValue, TextDelta, TreeID, TreeParentId,
    ValueOrContainer, VersionVector,
};

use crate::codec;
use crate::error::{DocError, DocResult};
use crate::loro_ext;
use crate::scene_cache::SceneRow;
use crate::text::{AttrKey, TextAttrs, TextContent, MAX_TEXT_CHARS, MAX_TEXT_RUNS};
use crate::text_codec;
use crate::types::*;

const TREE: &str = "nodes";
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
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
````

на:

````rust
    Delete(NodeId),
    /// Отмена удаления: поддерево в порядке обхода, родители раньше детей.
    Restore(Vec<SavedNode>),
    /// Отмена изменения свойств: прежние значения изменённых полей. За `Box`:
    /// с текстом патч стал большим, а варианты журнала живут тысячами.
    Props(NodeId, Box<PropsPatch>),
    /// Отмена перемещения.
    Move {
        id: NodeId,
        parent: Option<NodeId>,
        index: usize,
    },
    /// Отмена правки текста: обратная дельта Loro (спека подсистемы 3, раздел 3).
    /// Применяется к текущему `LoroText` узла, найденному по стабильному id, —
    /// поэтому переживает удаление и восстановление узла под новым `TreeID`.
    Text(NodeId, Vec<TextDelta>),
}

/// Индекс стабильных ID и зеркало свойств живых узлов.
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    }

    fn wrap(doc: LoroDoc) -> PageDoc {
        let tree = doc.get_tree(TREE);
        tree.enable_fractional_index(0);
        PageDoc {
````

на:

````rust
    }

    fn wrap(doc: LoroDoc) -> PageDoc {
        // Расширение разметки текста на вставку не хранится в документе: его
        // задаёт каждый `LoroDoc` страницы (новый, из снапшота, из журнала).
        doc.config_text_style(text_codec::style_config());
        let tree = doc.get_tree(TREE);
        tree.enable_fractional_index(0);
        PageDoc {
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
            }),
            Inverse::Props(id, patch) => self.set_props(*id, patch),
            Inverse::Move { id, parent, index } => self.move_node(*id, *parent, *index),
        })
    }

````

на:

````rust
            }),
            Inverse::Props(id, patch) => self.set_props(*id, patch),
            Inverse::Move { id, parent, index } => self.move_node(*id, *parent, *index),
            Inverse::Text(id, delta) => self.apply_text_delta(*id, delta),
        })
    }

````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    pub(crate) fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        let tid = self.tid(id)?;
        let before = self.props(id)?;
        let mut after = before.clone();
        patch.apply_to(&mut after);
        if after == before {
````

на:

````rust
    pub(crate) fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        let tid = self.tid(id)?;
        let before = self.props(id)?;
        // Свойства текста есть только у текста: у остальных узлов их не пишем.
        let stripped;
        let patch = if before.text.is_none() && patch.text.is_some() {
            stripped = PropsPatch {
                text: None,
                ..patch.clone()
            };
            &stripped
        } else {
            patch
        };
        let mut after = before.clone();
        patch.apply_to(&mut after);
        if after == before {
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        if let Some(props) = self.state.borrow_mut().props.get_mut(&id) {
            patch.apply_to(props);
        }
        self.record(Inverse::Props(id, patch.previous(&before)));
        Ok(())
    }

    /// Позицию узла среди детей его родителя за O(log n) (см. `loro_ext`).
````

на:

````rust
        if let Some(props) = self.state.borrow_mut().props.get_mut(&id) {
            patch.apply_to(props);
        }
        self.record(Inverse::Props(id, Box::new(patch.previous(&before))));
        Ok(())
    }

    /// Применяет дельту к тексту узла и записывает обратную для отмены.
    pub(crate) fn apply_text_delta(&self, id: NodeId, delta: &[TextDelta]) -> DocResult<()> {
        if delta.is_empty() {
            return Ok(());
        }
        let mut grown: isize = 0;
        for op in delta {
            match op {
                TextDelta::Insert { insert, .. } => grown += insert.chars().count() as isize,
                TextDelta::Delete { delete } => grown -= *delete as isize,
                TextDelta::Retain { .. } => {}
            }
        }
        self.edit_text(id, grown, |text| {
            let inverse = text_codec::invert(text, delta)?;
            text.apply_delta(delta)?;
            Ok(inverse)
        })
    }

    /// Правка текста узла: `edit` меняет `LoroText` и возвращает обратную дельту.
    /// Пределы (спека 3): длина — до правки (`grown` — на сколько символов текст
    /// вырастет), участки со стилями — после (превышение откатывается обратной
    /// дельтой). Зеркало получает содержимое заново, отмена — обратную дельту.
    fn edit_text(
        &self,
        id: NodeId,
        grown: isize,
        edit: impl FnOnce(&LoroText) -> DocResult<Vec<TextDelta>>,
    ) -> DocResult<()> {
        let tid = self.tid(id)?;
        if self.props(id)?.text.is_none() {
            return Err(DocError::NotText(id.to_string()));
        }
        let text = text_codec::text_container(&self.tree.get_meta(tid)?)?;
        // Последний перевод строки — не символ пользователя: предел без него.
        if text.len_unicode() as isize + grown > MAX_TEXT_CHARS as isize + 1 {
            return Err(DocError::TextTooLong);
        }
        let inverse = edit(&text)?;
        let content = text_codec::read_content(&text);
        if content.styled_runs() > MAX_TEXT_RUNS {
            text.apply_delta(&inverse)?;
            return Err(DocError::TooManyTextRuns);
        }
        if let Some(props) = self.state.borrow_mut().props.get_mut(&id) {
            if let Some(t) = &mut props.text {
                Arc::make_mut(t).content = content;
            }
        }
        self.record(Inverse::Text(id, inverse));
        Ok(())
    }

    /// Длина текста узла в символах (с последним переводом строки).
    fn text_len(&self, id: NodeId) -> DocResult<usize> {
        match &self.props(id)?.text {
            Some(t) => Ok(t.content.char_len()),
            None => Err(DocError::NotText(id.to_string())),
        }
    }

    /// Вставка текста в позицию `at` (символы Unicode). `attrs`: `None` —
    /// разметка наследуется слева (расширение `After`), `Some` — ровно эта.
    /// После последнего перевода строки вставлять нельзя.
    pub(crate) fn insert_text(
        &self,
        id: NodeId,
        at: usize,
        s: &str,
        attrs: Option<&TextAttrs>,
    ) -> DocResult<()> {
        let len = self.text_len(id)?;
        if at >= len {
            return Err(DocError::TextRange(at, len));
        }
        if s.is_empty() {
            return Ok(());
        }
        let mut delta = Vec::new();
        if at > 0 {
            delta.push(TextDelta::Retain {
                retain: at,
                attributes: None,
            });
        }
        match attrs {
            // Дельта вставляет с точной разметкой (без унаследованной слева).
            Some(a) => {
                delta.push(TextDelta::Insert {
                    insert: s.to_string(),
                    attributes: Some(text_codec::attrs_to_loro(a)),
                });
                self.apply_text_delta(id, &delta)
            }
            // Обычная вставка Loro: разметка расширяется по `style_config`.
            None => {
                let n = s.chars().count();
                delta.push(TextDelta::Delete { delete: n });
                self.edit_text(id, n as isize, |text| {
                    text.insert(at, s)?;
                    Ok(delta)
                })
            }
        }
    }

    /// Удаление диапазона символов. Последний перевод строки не удаляется.
    pub(crate) fn delete_text(&self, id: NodeId, range: Range<usize>) -> DocResult<()> {
        let len = self.text_len(id)?;
        if range.start > range.end || range.end >= len {
            return Err(DocError::TextRange(range.end, len));
        }
        if range.is_empty() {
            return Ok(());
        }
        let mut delta = Vec::new();
        if range.start > 0 {
            delta.push(TextDelta::Retain {
                retain: range.start,
                attributes: None,
            });
        }
        delta.push(TextDelta::Delete {
            delete: range.len(),
        });
        self.apply_text_delta(id, &delta)
    }

    /// Разметка диапазона: ключи из `set` задаются, ключи из `clear` снимаются.
    pub(crate) fn format_text(
        &self,
        id: NodeId,
        range: Range<usize>,
        set: &TextAttrs,
        clear: &[AttrKey],
    ) -> DocResult<()> {
        let len = self.text_len(id)?;
        if range.start > range.end || range.end > len {
            return Err(DocError::TextRange(range.end, len));
        }
        let mut attrs = text_codec::attrs_to_loro(set);
        for k in clear {
            if !set.has(*k) {
                attrs.insert(k.as_str().to_string(), LoroValue::Null);
            }
        }
        if range.is_empty() || attrs.is_empty() {
            return Ok(());
        }
        let mut delta = Vec::new();
        if range.start > 0 {
            delta.push(TextDelta::Retain {
                retain: range.start,
                attributes: None,
            });
        }
        delta.push(TextDelta::Retain {
            retain: range.len(),
            attributes: Some(attrs),
        });
        self.apply_text_delta(id, &delta)
    }

    /// Заменяет содержимое целиком (вставка из буфера, тесты). Текст без
    /// перевода строки в конце его получает.
    pub(crate) fn set_text_content(&self, id: NodeId, content: &TextContent) -> DocResult<()> {
        let len = self.text_len(id)?;
        let mut content = content.clone();
        if !content.text().ends_with('\n') {
            content.runs.push(crate::text::TextRun {
                text: "\n".into(),
                attrs: TextAttrs::default(),
            });
        }
        if self.props(id)?.text.as_ref().map(|t| &t.content) == Some(&content) {
            return Ok(());
        }
        let mut delta = vec![TextDelta::Delete { delete: len }];
        delta.extend(text_codec::insert_delta(&content));
        self.apply_text_delta(id, &delta)
    }

    /// Позицию узла среди детей его родителя за O(log n) (см. `loro_ext`).
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    pub fn delete_nodes(&self, ids: &[NodeId]) -> DocResult<()> {
        self.page.delete_nodes(ids)
    }
}

impl std::ops::Deref for Tx<'_> {
````

на:

````rust
    pub fn delete_nodes(&self, ids: &[NodeId]) -> DocResult<()> {
        self.page.delete_nodes(ids)
    }

    /// Вставка в текст узла (позиции — символы Unicode). `attrs`: `None` —
    /// разметка наследуется слева, `Some` — ровно эта.
    pub fn insert_text(
        &self,
        id: NodeId,
        at: usize,
        text: &str,
        attrs: Option<&TextAttrs>,
    ) -> DocResult<()> {
        self.page.insert_text(id, at, text, attrs)
    }

    /// Удаление символов текста (последний перевод строки не удаляется).
    pub fn delete_text(&self, id: NodeId, range: Range<usize>) -> DocResult<()> {
        self.page.delete_text(id, range)
    }

    /// Разметка диапазона текста: задать ключи `set`, снять ключи `clear`.
    pub fn format_text(
        &self,
        id: NodeId,
        range: Range<usize>,
        set: &TextAttrs,
        clear: &[AttrKey],
    ) -> DocResult<()> {
        self.page.format_text(id, range, set, clear)
    }

    /// Содержимое текста целиком.
    pub fn set_text_content(&self, id: NodeId, content: &TextContent) -> DocResult<()> {
        self.page.set_text_content(id, content)
    }
}

impl std::ops::Deref for Tx<'_> {
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        let ops = vec![
            Inverse::Props(
                missing,
                PropsPatch {
                    x: Some(1.0),
                    ..Default::default()
                },
            ),
            Inverse::Delete(all[3]),
            Inverse::Props(
                all[1],
                PropsPatch {
                    x: Some(5.0),
                    ..Default::default()
                },
            ),
            Inverse::Move {
                id: all[2],
````

на:

````rust
        let ops = vec![
            Inverse::Props(
                missing,
                Box::new(PropsPatch {
                    x: Some(1.0),
                    ..Default::default()
                }),
            ),
            Inverse::Delete(all[3]),
            Inverse::Props(
                all[1],
                Box::new(PropsPatch {
                    x: Some(5.0),
                    ..Default::default()
                }),
            ),
            Inverse::Move {
                id: all[2],
````

В `crates/ferrum-doc/src/changes.rs` заменить:

````rust
                Inverse::Restore(nodes) => self.deleted.extend(nodes.iter().map(|n| n.id)),
                Inverse::Props(id, _) => self.props.push(*id),
                Inverse::Move { id, .. } => self.moved.push(*id),
            }
        }
    }
````

на:

````rust
                Inverse::Restore(nodes) => self.deleted.extend(nodes.iter().map(|n| n.id)),
                Inverse::Props(id, _) => self.props.push(*id),
                Inverse::Move { id, .. } => self.moved.push(*id),
                Inverse::Text(id, _) => self.props.push(*id),
            }
        }
    }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    fn missing_props_op() -> Inverse {
        Inverse::Props(
            missing_node(),
            PropsPatch {
                x: Some(1.0),
                ..Default::default()
            },
        )
    }

````

на:

````rust
    fn missing_props_op() -> Inverse {
        Inverse::Props(
            missing_node(),
            Box::new(PropsPatch {
                x: Some(1.0),
                ..Default::default()
            }),
        )
    }

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        };
        let err = rollback(
            &page,
            &[Inverse::Props(missing_node(), patch)],
            DocError::CyclicMove,
        );
        match err {
````

на:

````rust
        };
        let err = rollback(
            &page,
            &[Inverse::Props(missing_node(), Box::new(patch))],
            DocError::CyclicMove,
        );
        match err {
````

- [ ] **Step 5: Кэш сцены версии 3 с текстом**

В `crates/ferrum-doc/src/binary.rs` заменить:

````rust
    /// иначе кэш сцены молча разошёлся бы с документом.
    pub fn str16(&mut self, s: &str) -> Option<()> {
        self.u16(u16::try_from(s.len()).ok()?);
        self.bytes(s.as_bytes());
        Some(())
    }
````

на:

````rust
    /// иначе кэш сцены молча разошёлся бы с документом.
    pub fn str16(&mut self, s: &str) -> Option<()> {
        self.u16(u16::try_from(s.len()).ok()?);
        self.bytes(s.as_bytes());
        Some(())
    }

    /// Строка с длиной u32 — текст узла (до 100 000 символов, до 400 КБ).
    pub fn str32(&mut self, s: &str) -> Option<()> {
        self.u32(u32::try_from(s.len()).ok()?);
        self.bytes(s.as_bytes());
        Some(())
    }
````

В `crates/ferrum-doc/src/binary.rs` заменить:

````rust
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}
````

на:

````rust
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }

    pub fn str32(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
//! раскодируется в фоне. Таблица — производные данные: при несовпадении со снапшотом
//! она просто игнорируется.

use loro::TreeID;

use crate::binary::{Reader, Writer};
use crate::types::*;

/// Узел страницы в порядке обхода в глубину: родитель всегда раньше детей.
````

на:

````rust
//! раскодируется в фоне. Таблица — производные данные: при несовпадении со снапшотом
//! она просто игнорируется.

use std::sync::Arc;

use loro::TreeID;

use crate::binary::{Reader, Writer};
use crate::text::*;
use crate::types::*;

/// Узел страницы в порядке обхода в глубину: родитель всегда раньше детей.
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
        NodeKind::Ellipse => 3,
        NodeKind::Line => 4,
        NodeKind::Image => 5,
    }
}

````

на:

````rust
        NodeKind::Ellipse => 3,
        NodeKind::Line => 4,
        NodeKind::Image => 5,
        // Третья версия таблицы (`SCENE_CACHE_VERSION = 3`): у строки текста после
        // обводок — свойства текста и содержимое (`write_text`).
        NodeKind::Text => 6,
    }
}

````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
        NodeKind::Ellipse,
        NodeKind::Line,
        NodeKind::Image,
    ]
    .get(c as usize)
    .copied()
````

на:

````rust
        NodeKind::Ellipse,
        NodeKind::Line,
        NodeKind::Image,
        NodeKind::Text,
    ]
    .get(c as usize)
    .copied()
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
        w.str16(&p.name)?;
        w.u8(u8::try_from(p.fills.len()).ok()?);
        for f in &p.fills {
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
            w.str16(f.image_hash.as_deref().unwrap_or(""))?;
        }
        w.u8(u8::try_from(p.strokes.len()).ok()?);
        for s in &p.strokes {
````

на:

````rust
        w.str16(&p.name)?;
        w.u8(u8::try_from(p.fills.len()).ok()?);
        for f in &p.fills {
            write_fill(&mut w, f)?;
        }
        w.u8(u8::try_from(p.strokes.len()).ok()?);
        for s in &p.strokes {
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
                StrokeAlign::Outside => 2,
            });
        }
    }
    Some(w.buf)
}

/// `None` — таблица повреждена; вызывающий код тогда просто не использует кэш.
````

на:

````rust
                StrokeAlign::Outside => 2,
            });
        }
        if let Some(t) = &p.text {
            write_text(&mut w, t)?;
        }
    }
    Some(w.buf)
}

fn write_fill(w: &mut Writer, f: &Fill) -> Option<()> {
    w.u8(match f.kind {
        FillKind::Solid => 0,
        FillKind::Image => 1,
    });
    color(w, f.color);
    w.u8(match f.scale_mode {
        ScaleMode::Fill => 0,
        ScaleMode::Fit => 1,
        ScaleMode::Tile => 2,
    });
    w.f64(f.opacity);
    w.u8(f.visible as u8);
    w.str16(f.image_hash.as_deref().unwrap_or(""))
}

fn read_fill(r: &mut Reader) -> Option<Fill> {
    let kind = if r.u8()? == 1 {
        FillKind::Image
    } else {
        FillKind::Solid
    };
    let color = read_color(r)?;
    let scale_mode = match r.u8()? {
        1 => ScaleMode::Fit,
        2 => ScaleMode::Tile,
        _ => ScaleMode::Fill,
    };
    let opacity = r.f64()?;
    let visible = r.u8()? != 0;
    let hash = r.str16()?;
    Some(Fill {
        kind,
        color,
        image_hash: (!hash.is_empty()).then_some(hash),
        scale_mode,
        opacity,
        visible,
    })
}

/// Номер значения в списке всех значений перечисления (и обратно).
fn code<T: PartialEq + Copy>(all: &[T], v: T) -> u8 {
    all.iter().position(|x| *x == v).unwrap_or(0) as u8
}

fn decode_enum<T: Copy>(all: &[T], c: u8) -> Option<T> {
    all.get(c as usize).copied()
}

const SIZINGS: [TextSizing; 3] = [
    TextSizing::AutoWidth,
    TextSizing::AutoHeight,
    TextSizing::Fixed,
];
const ALIGNS_H: [AlignH; 4] = [AlignH::Left, AlignH::Center, AlignH::Right, AlignH::Justify];
const ALIGNS_V: [AlignV; 3] = [AlignV::Top, AlignV::Middle, AlignV::Bottom];
const DECORATIONS: [Decoration; 3] = [Decoration::None, Decoration::Underline, Decoration::Strike];
const CASES: [TextCase; 5] = [
    TextCase::None,
    TextCase::Upper,
    TextCase::Lower,
    TextCase::Title,
    TextCase::SmallCaps,
];
const LISTS: [ListKind; 3] = [ListKind::None, ListKind::Bullet, ListKind::Ordered];

fn write_line_height(w: &mut Writer, v: LineHeight) {
    let (tag, x) = match v {
        LineHeight::Auto => (0, 0.0),
        LineHeight::Px(x) => (1, x),
        LineHeight::Percent(x) => (2, x),
    };
    w.u8(tag);
    w.f64(x);
}

fn read_line_height(r: &mut Reader) -> Option<LineHeight> {
    let (tag, x) = (r.u8()?, r.f64()?);
    Some(match tag {
        0 => LineHeight::Auto,
        1 => LineHeight::Px(x),
        2 => LineHeight::Percent(x),
        _ => return None,
    })
}

fn write_letter_spacing(w: &mut Writer, v: LetterSpacing) {
    let (tag, x) = match v {
        LetterSpacing::Px(x) => (1, x),
        LetterSpacing::Percent(x) => (2, x),
    };
    w.u8(tag);
    w.f64(x);
}

fn read_letter_spacing(r: &mut Reader) -> Option<LetterSpacing> {
    let (tag, x) = (r.u8()?, r.f64()?);
    Some(match tag {
        1 => LetterSpacing::Px(x),
        2 => LetterSpacing::Percent(x),
        _ => return None,
    })
}

/// Свойства текста и содержимое: участки с маской заданных ключей разметки
/// (бит — номер ключа в `AttrKey::ALL`) и значениями по порядку ключей.
fn write_text(w: &mut Writer, t: &TextData) -> Option<()> {
    let p = &t.props;
    w.u8(code(&SIZINGS, p.sizing));
    w.u8(code(&ALIGNS_H, p.align_h));
    w.u8(code(&ALIGNS_V, p.align_v));
    w.f64(p.paragraph_spacing);
    w.f64(p.paragraph_indent);
    w.u8(p.truncate as u8);
    w.u32(p.max_lines.unwrap_or(0));
    w.str16(&p.style.font.family)?;
    w.str16(&p.style.font.style)?;
    w.f64(p.style.size);
    write_line_height(w, p.style.line_height);
    write_letter_spacing(w, p.style.letter_spacing);
    w.u8(code(&DECORATIONS, p.style.decoration));
    w.u8(code(&CASES, p.style.case));
    w.u32(u32::try_from(t.content.runs.len()).ok()?);
    for run in &t.content.runs {
        w.str32(&run.text)?;
        let a = &run.attrs;
        let mask = AttrKey::ALL
            .iter()
            .enumerate()
            .filter(|(_, k)| a.has(**k))
            .fold(0u16, |m, (i, _)| m | (1 << i));
        w.u16(mask);
        if let Some(v) = &a.font {
            w.str16(v)?;
        }
        if let Some(v) = &a.style {
            w.str16(v)?;
        }
        if let Some(v) = a.size {
            w.f64(v);
        }
        if let Some(v) = &a.fill {
            write_fill(w, v)?;
        }
        if let Some(v) = a.line_height {
            write_line_height(w, v);
        }
        if let Some(v) = a.letter_spacing {
            write_letter_spacing(w, v);
        }
        if let Some(v) = a.decoration {
            w.u8(code(&DECORATIONS, v));
        }
        if let Some(v) = a.case {
            w.u8(code(&CASES, v));
        }
        if let Some(v) = a.list {
            w.u8(code(&LISTS, v));
        }
        if let Some(v) = a.indent {
            w.u8(v);
        }
    }
    Some(())
}

fn read_text(r: &mut Reader) -> Option<TextData> {
    let sizing = decode_enum(&SIZINGS, r.u8()?)?;
    let align_h = decode_enum(&ALIGNS_H, r.u8()?)?;
    let align_v = decode_enum(&ALIGNS_V, r.u8()?)?;
    let paragraph_spacing = r.f64()?;
    let paragraph_indent = r.f64()?;
    let truncate = r.u8()? != 0;
    let max_lines = Some(r.u32()?).filter(|n| *n > 0);
    let font = FontName::new(r.str16()?, r.str16()?);
    let size = r.f64()?;
    let line_height = read_line_height(r)?;
    let letter_spacing = read_letter_spacing(r)?;
    let decoration = decode_enum(&DECORATIONS, r.u8()?)?;
    let case = decode_enum(&CASES, r.u8()?)?;
    let n = r.u32()? as usize;
    let mut runs = Vec::with_capacity(n.min(MAX_TEXT_RUNS * 2 + 1));
    for _ in 0..n {
        let text = r.str32()?;
        let mask = r.u16()?;
        let on = |k: AttrKey| {
            let i = AttrKey::ALL.iter().position(|x| *x == k).unwrap_or(0);
            mask & (1 << i) != 0
        };
        let mut a = TextAttrs::default();
        if on(AttrKey::Font) {
            a.font = Some(r.str16()?);
        }
        if on(AttrKey::Style) {
            a.style = Some(r.str16()?);
        }
        if on(AttrKey::Size) {
            a.size = Some(r.f64()?);
        }
        if on(AttrKey::Fill) {
            a.fill = Some(read_fill(r)?);
        }
        if on(AttrKey::LineHeight) {
            a.line_height = Some(read_line_height(r)?);
        }
        if on(AttrKey::LetterSpacing) {
            a.letter_spacing = Some(read_letter_spacing(r)?);
        }
        if on(AttrKey::Decoration) {
            a.decoration = Some(decode_enum(&DECORATIONS, r.u8()?)?);
        }
        if on(AttrKey::Case) {
            a.case = Some(decode_enum(&CASES, r.u8()?)?);
        }
        if on(AttrKey::List) {
            a.list = Some(decode_enum(&LISTS, r.u8()?)?);
        }
        if on(AttrKey::Indent) {
            a.indent = Some(r.u8()?);
        }
        runs.push(TextRun { text, attrs: a });
    }
    Some(TextData {
        props: TextProps {
            sizing,
            align_h,
            align_v,
            paragraph_spacing,
            paragraph_indent,
            truncate,
            max_lines,
            style: TextStyle {
                font,
                size,
                line_height,
                letter_spacing,
                decoration,
                case,
            },
        },
        content: TextContent { runs },
    })
}

/// `None` — таблица повреждена; вызывающий код тогда просто не использует кэш.
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
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
````

на:

````rust
        let name = r.str16()?;
        let mut fills = Vec::new();
        for _ in 0..r.u8()? {
            fills.push(read_fill(&mut r)?);
        }
        let mut strokes = Vec::new();
        for _ in 0..r.u8()? {
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
                align,
            });
        }
        let [x, y, w, h, rotation, opacity, corner_radius] = nums;
        rows.push(SceneRow {
            id,
````

на:

````rust
                align,
            });
        }
        let text = match kind {
            NodeKind::Text => Some(Arc::new(read_text(&mut r)?)),
            _ => None,
        };
        let [x, y, w, h, rotation, opacity, corner_radius] = nums;
        rows.push(SceneRow {
            id,
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
                flip_y: flags & FLAG_FLIP_Y != 0,
                fills,
                strokes,
            },
        });
    }
````

на:

````rust
                flip_y: flags & FLAG_FLIP_Y != 0,
                fills,
                strokes,
                text,
            },
        });
    }
````

В `crates/ferrum-doc/src/container.rs` заменить:

````rust
/// строки читались бы и сейчас: старая сборка, пересохранившая новый файл, пишет
/// таблицу без флагов рядом со снапшотом, где они есть, а зеркало страницы
/// берётся из таблицы — флаги тихо пропали бы и из правок.
pub const SCENE_CACHE_VERSION: u8 = 2;

const ROOT: u8 = 1;
const PAGE: u8 = 2;
````

на:

````rust
/// строки читались бы и сейчас: старая сборка, пересохранившая новый файл, пишет
/// таблицу без флагов рядом со снапшотом, где они есть, а зеркало страницы
/// берётся из таблицы — флаги тихо пропали бы и из правок.
pub const SCENE_CACHE_VERSION: u8 = 3;

const ROOT: u8 = 1;
const PAGE: u8 = 2;
````

- [ ] **Step 6: Текст в других крейтах: форма, строка слоя, буфер обмена**

Рамка текста — его форма и область попадания (спека 5); вид узла меняется и при правке текста (`looks_different`). В буфере обмена узел текста пока получает пустой текст — формат с содержимым появится в Task 6.

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
        },
        NodeKind::Ellipse => Geometry::Ellipse(rect),
        NodeKind::Line => Geometry::Line(p.w),
    }
}

````

на:

````rust
        },
        NodeKind::Ellipse => Geometry::Ellipse(rect),
        NodeKind::Line => Geometry::Line(p.w),
        // Рамка текста; попадание — по ней (спека подсистемы 3, раздел 5).
        NodeKind::Text => Geometry::Rect { rect, radius: 0.0 },
    }
}

````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
    }
    // Изображение рисуется на всём прямоугольнике, поэтому попадает в него, как фрейм,
    // даже без заливок: у свежесозданного изображения заливок нет, и без этого клик
    // по нему не попадал бы никуда.
    let filled =
        matches!(p.kind, NodeKind::Frame | NodeKind::Image) || p.fills.iter().any(|f| f.visible);
    if filled && geom.contains(local) {
        return true;
    }
````

на:

````rust
    }
    // Изображение рисуется на всём прямоугольнике, поэтому попадает в него, как фрейм,
    // даже без заливок: у свежесозданного изображения заливок нет, и без этого клик
    // по нему не попадал бы никуда. Текст ловит клик всей рамкой, как в Figma.
    let filled = matches!(p.kind, NodeKind::Frame | NodeKind::Image | NodeKind::Text)
        || p.fills.iter().any(|f| f.visible);
    if filled && geom.contains(local) {
        return true;
    }
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
        || a.corner_radius != b.corner_radius
        || a.fills != b.fills
        || a.strokes != b.strokes
}

fn aabb(r: Rect) -> AABB<[f64; 2]> {
````

на:

````rust
        || a.corner_radius != b.corner_radius
        || a.fills != b.fills
        || a.strokes != b.strokes
        || a.text != b.text
}

fn aabb(r: Rect) -> AABB<[f64; 2]> {
````

В `crates/ferrum-wasm/src/layers.rs` заменить:

````rust
    Ellipse,
    Line,
    Image,
}

impl From<NodeKind> for NodeType {
````

на:

````rust
    Ellipse,
    Line,
    Image,
    Text,
}

impl From<NodeKind> for NodeType {
````

В `crates/ferrum-wasm/src/layers.rs` заменить:

````rust
            NodeKind::Ellipse => NodeType::Ellipse,
            NodeKind::Line => NodeType::Line,
            NodeKind::Image => NodeType::Image,
        }
    }
}
````

на:

````rust
            NodeKind::Ellipse => NodeType::Ellipse,
            NodeKind::Line => NodeType::Line,
            NodeKind::Image => NodeType::Image,
            NodeKind::Text => NodeType::Text,
        }
    }
}
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
//! неизвестного типа (из будущих версий) пропускаются вместе с поддеревом.

use std::collections::HashSet;

use ferrum_doc::{
    Color, Fill, FillKind, NodeKind, Props, PropsPatch, ScaleMode, Stroke, StrokeAlign,
};
use ferrum_scene::shape;
use kurbo::Rect;
````

на:

````rust
//! неизвестного типа (из будущих версий) пропускаются вместе с поддеревом.

use std::collections::HashSet;
use std::sync::Arc;

use ferrum_doc::{
    Color, Fill, FillKind, NodeKind, Props, PropsPatch, ScaleMode, Stroke, StrokeAlign, TextData,
};
use ferrum_scene::shape;
use kurbo::Rect;
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
        flip_y: n.flip_y,
        fills,
        strokes,
    };
    // Нечисловые и огромные значения — к допустимым, как у строк кэша сцены.
    shape::sanitize(&mut props);
````

на:

````rust
        flip_y: n.flip_y,
        fills,
        strokes,
        // Текст копии — пустой до этапа с форматом текста в буфере (Task 6).
        text: (kind == NodeKind::Text).then(|| Arc::new(TextData::default())),
    };
    // Нечисловые и огромные значения — к допустимым, как у строк кэша сцены.
    shape::sanitize(&mut props);
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
        flip_y: p.flip_y.then_some(true),
        fills: Some(p.fills.clone()),
        strokes: Some(p.strokes.clone()),
    }
}
````

на:

````rust
        flip_y: p.flip_y.then_some(true),
        fills: Some(p.fills.clone()),
        strokes: Some(p.strokes.clone()),
        text: p.text.as_ref().map(|t| t.props.clone()),
    }
}
````

В `apps/web/src/ui/layers.tsx` заменить:

````tsx
  ellipse: 'ellipse',
  line: 'line',
  image: 'image',
};

/** Окно строк для прокрутки `scrollTop` и высоты `height`. */
````

на:

````tsx
  ellipse: 'ellipse',
  line: 'line',
  image: 'image',
  text: 'text',
};

/** Окно строк для прокрутки `scrollTop` и высоты `height`. */
````

- [ ] **Step 7: Проверить**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ferrum-doc -p ferrum-scene`
Expected: PASS, без предупреждений. В `ferrum-doc`: `text` — 10 тестов, `container` — 21, `flip` — 8, `invariants` — 1, остальные наборы — как прежде.

Run: `cd apps/web && npm run wasm && npm run typecheck`
Expected: PASS (тип `NodeType` с `"text"`, иконка строки слоя — `text`).

- [ ] **Step 8: Закоммитить**

```bash
git add crates/ferrum-doc crates/ferrum-scene crates/ferrum-wasm/src/layers.rs crates/ferrum-editor/src/clipboard.rs apps/web/src/ui/layers.tsx
git commit -m "feat(doc): узел Text — LoroText с разметкой, свойства текста в зеркале, отмена правок текста обратной дельтой, пределы, кэш сцены v3

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Слияние ввода, JSON текста, копии со всеми свойствами, совместимость

Спека 3: «ввод подряд в одном месте — один шаг (шаг рвётся паузой > 1 с, сменой позиции курсора или стиля)». Инструмента ввода до этапа 2 нет, поэтому модель даёт `Document::transact_merging` с меткой `TextMerge` (узел, позиция курсора до и после, отпечаток стиля, время): шаг продолжает верхний, пока стек не трогали (`History::last_merge`). Здесь же: текст в JSON для буфера обмена (`TextData::to_json`/`from_json` — те же ключи, что в карте узла), создание узла сразу со всеми свойствами и содержимым (`Tx::create_with_props` — копии и вставка, Task 6), пример страницы из текстов для замера (`fixture::fill_texts`), proptest с правками текста и проверка совместимости со сборкой без текста.

**Files:**
- Modify: `crates/ferrum-doc/src/history.rs` (1–57), `document.rs` (`transact` 329–367), `page.rs` (`create_node` 406–417, `Tx` 812–851), `text_codec.rs`, `lib.rs`, `fixture.rs`, `Cargo.toml`
- Create: `crates/ferrum-doc/examples/text_fixture.rs`, `scripts/compat-old-build.sh`
- Test: `crates/ferrum-doc/tests/text.rs` (слияние), `crates/ferrum-doc/tests/invariants.rs` (правки текста в proptest)

**Interfaces:**
- Consumes: Task 1 (`Tx::insert_text` и соседи, `TextData`, `text_codec`).
- Produces: `ferrum_doc::{TextMerge { node, before, after, style: u64, at_ms: f64 }, MERGE_PAUSE_MS = 1000.0}`, `TextMerge::continues(&next) -> bool`; `Document::transact_merging(page, selection, merge, f)`; `TextData::to_json() -> String`, `TextData::from_json(&str) -> DocResult<TextData>`; `Tx::create_with_props(parent, index, &Props) -> DocResult<NodeId>`; `fixture::fill_texts(&Tx, texts) -> DocResult<Vec<NodeId>>` (фреймы 1440 × 1024 по 100 текстов 120 × 40, Inter 14, `Fixed`); пример `cargo run -p ferrum-doc --example text_fixture -- <путь>`; `scripts/compat-old-build.sh [ref]`.

- [ ] **Step 1: Падающие тесты слияния и proptest с текстом**

В `crates/ferrum-doc/tests/text.rs` заменить:

````rust
//! Текстовый узел (подсистема 3, этап 1): содержимое в `LoroText` с разметкой,
//! расширение разметки на вставку, свойства текста в зеркале, отмена правок
//! текста обратной дельтой, пределы, файл и кэш сцены.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;
````

на:

````rust
//! Текстовый узел (подсистема 3, этап 1): содержимое в `LoroText` с разметкой,
//! расширение разметки на вставку, свойства текста в зеркале, отмена правок
//! текста обратной дельтой, слияние ввода подряд, пределы, файл и кэш сцены.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;
````

В `crates/ferrum-doc/tests/text.rs` заменить:

````rust
}

#[test]
fn limits_refuse_the_edit_without_a_step() {
    let (mut doc, page, id) = doc_with_text();
    let before = content(&mut doc, page, id);
````

на:

````rust
}

#[test]
fn typing_in_a_row_is_one_undo_step() {
    let (mut doc, page, id) = doc_with_text();
    let before = content(&mut doc, page, id);
    // Курсор с 11-го символа, правки через 300 мс.
    for (i, ch) in ["!", "!", "?"].into_iter().enumerate() {
        let caret = 11 + i;
        let merge = TextMerge {
            node: id,
            before: caret,
            after: caret + 1,
            style: 0,
            at_ms: 300.0 * i as f64,
        };
        doc.transact_merging(page, vec![id], merge, |tx| {
            tx.insert_text(id, caret, ch, None)?;
            Ok(vec![id])
        })
        .unwrap();
    }
    assert_eq!(content(&mut doc, page, id).text(), "Привет, мир!!?\n");
    doc.undo().unwrap();
    assert_eq!(content(&mut doc, page, id), before);
    assert!(
        !doc.can_undo() || {
            // Создание узла — свой шаг раньше ввода.
            doc.undo().unwrap();
            !doc.can_undo()
        }
    );
}

#[test]
fn pause_jump_or_style_change_starts_a_new_step() {
    for (gap, jump, style) in [(1500.0, 0, 0), (100.0, 3, 0), (100.0, 0, 7)] {
        let (mut doc, page, id) = doc_with_text();
        let first = TextMerge {
            node: id,
            before: 0,
            after: 1,
            style: 0,
            at_ms: 0.0,
        };
        doc.transact_merging(page, vec![], first, |tx| {
            tx.insert_text(id, 0, "а", None)?;
            Ok(vec![])
        })
        .unwrap();
        let after_first = content(&mut doc, page, id);
        let at = 1 + jump;
        let second = TextMerge {
            node: id,
            before: at,
            after: at + 1,
            style,
            at_ms: gap,
        };
        doc.transact_merging(page, vec![], second, |tx| {
            tx.insert_text(id, at, "б", None)?;
            Ok(vec![])
        })
        .unwrap();
        doc.undo().unwrap();
        assert_eq!(
            content(&mut doc, page, id),
            after_first,
            "{gap} {jump} {style}"
        );
    }
}

#[test]
fn undo_breaks_merging() {
    let (mut doc, page, id) = doc_with_text();
    let tag = |before: usize, at_ms: f64| TextMerge {
        node: id,
        before,
        after: before + 1,
        style: 0,
        at_ms,
    };
    doc.transact_merging(page, vec![], tag(0, 0.0), |tx| {
        tx.insert_text(id, 0, "а", None)?;
        Ok(vec![])
    })
    .unwrap();
    doc.undo().unwrap();
    let base = content(&mut doc, page, id);
    // После отмены та же позиция не сливается с отменённым шагом.
    doc.transact_merging(page, vec![], tag(1, 100.0), |tx| {
        tx.insert_text(id, 0, "б", None)?;
        Ok(vec![])
    })
    .unwrap();
    doc.undo().unwrap();
    assert_eq!(content(&mut doc, page, id), base);
}

#[test]
fn limits_refuse_the_edit_without_a_step() {
    let (mut doc, page, id) = doc_with_text();
    let before = content(&mut doc, page, id);
````

В proptest — правки текста (вставка буквы или перевода строки, удаление, кегль диапазона); их шаг есть, только если текст изменился. Заливка теста получает синий канал 1: чёрная заливка совпала бы с заливкой нового текста, и правка без изменений не дала бы шага, которого ждёт модель.

В `crates/ferrum-doc/tests/invariants.rs` заменить:

````rust
        node: usize,
        rgb: u8,
    },
    Undo,
    Redo,
}

fn edit_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<usize>(), 0u8..6).prop_map(|(parent, kind)| Op::Create { parent, kind }),
        (any::<usize>(), any::<usize>(), 0usize..5).prop_map(|(node, parent, index)| Op::Move {
            node,
            parent,
````

на:

````rust
        node: usize,
        rgb: u8,
    },
    /// Правки текста (подсистема 3): вставка буквы или перевода строки, удаление
    /// диапазона, кегль диапазона. Узел не текст — правки нет.
    TextInsert {
        node: usize,
        at: usize,
        ch: u8,
    },
    TextDelete {
        node: usize,
        a: usize,
        b: usize,
    },
    TextFormat {
        node: usize,
        a: usize,
        b: usize,
        size: u8,
    },
    Undo,
    Redo,
}

fn edit_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<usize>(), 0u8..7).prop_map(|(parent, kind)| Op::Create { parent, kind }),
        (any::<usize>(), any::<usize>(), 0usize..5).prop_map(|(node, parent, index)| Op::Move {
            node,
            parent,
````

В `crates/ferrum-doc/tests/invariants.rs` заменить:

````rust
        any::<usize>().prop_map(|node| Op::Delete { node }),
        (any::<usize>(), -1000i32..1000).prop_map(|(node, x)| Op::SetX { node, x }),
        (any::<usize>(), any::<u8>()).prop_map(|(node, rgb)| Op::Fill { node, rgb }),
    ]
}

````

на:

````rust
        any::<usize>().prop_map(|node| Op::Delete { node }),
        (any::<usize>(), -1000i32..1000).prop_map(|(node, x)| Op::SetX { node, x }),
        (any::<usize>(), any::<u8>()).prop_map(|(node, rgb)| Op::Fill { node, rgb }),
        (any::<usize>(), any::<usize>(), any::<u8>()).prop_map(|(node, at, ch)| Op::TextInsert {
            node,
            at,
            ch
        }),
        (any::<usize>(), any::<usize>(), any::<usize>()).prop_map(|(node, a, b)| Op::TextDelete {
            node,
            a,
            b
        }),
        (any::<usize>(), any::<usize>(), any::<usize>(), any::<u8>())
            .prop_map(|(node, a, b, size)| Op::TextFormat { node, a, b, size }),
    ]
}

````

В `crates/ferrum-doc/tests/invariants.rs` заменить:

````rust
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
````

на:

````rust
    ]
}

const KINDS: [NodeKind; 7] = [
    NodeKind::Frame,
    NodeKind::Group,
    NodeKind::Rect,
    NodeKind::Ellipse,
    NodeKind::Line,
    NodeKind::Image,
    NodeKind::Text,
];

/// Правка текста: шаг истории есть, только если текст изменился (кегль, уже
/// стоящий на диапазоне, — не правка).
fn is_text_op(op: &Op) -> bool {
    matches!(
        op,
        Op::TextInsert { .. } | Op::TextDelete { .. } | Op::TextFormat { .. }
    )
}

/// Длина текста узла (с последним переводом строки); не текст — `None`.
fn text_len(p: &Tx, n: NodeId) -> DocResult<Option<usize>> {
    Ok(p.props(n)?.text.map(|t| t.content.char_len()))
}

/// `pick(i)` — живой узел или корень (None), чтобы операции попадали в существующие узлы.
fn pick(alive: &[NodeId], i: usize) -> Option<NodeId> {
````

В `crates/ferrum-doc/tests/invariants.rs` заменить:

````rust
        | Op::Delete { node }
        | Op::SetX { node, .. }
        | Op::Fill { node, .. } => pick(alive, node).is_some(),
        Op::Undo | Op::Redo => false,
    }
}
````

на:

````rust
        | Op::Delete { node }
        | Op::SetX { node, .. }
        | Op::Fill { node, .. } => pick(alive, node).is_some(),
        Op::TextInsert { .. } | Op::TextDelete { .. } | Op::TextFormat { .. } => false,
        Op::Undo | Op::Redo => false,
    }
}
````

В `crates/ferrum-doc/tests/invariants.rs` заменить:

````rust
        }
        Op::Fill { node, rgb } => {
            if let Some(n) = pick(alive, node) {
                let fills = vec![Fill::solid(Color::rgba(rgb, rgb, 0, 255))];
                p.set_props(
                    n,
                    &PropsPatch {
````

на:

````rust
        }
        Op::Fill { node, rgb } => {
            if let Some(n) = pick(alive, node) {
                // Синий канал 1: такой заливки нет ни у одного узла по умолчанию (у
                // текста — чёрная), и правка всегда что-то меняет.
                let fills = vec![Fill::solid(Color::rgba(rgb, rgb, 1, 255))];
                p.set_props(
                    n,
                    &PropsPatch {
````

В `crates/ferrum-doc/tests/invariants.rs` заменить:

````rust
                        ..Default::default()
                    },
                )?;
            }
        }
        Op::Undo | Op::Redo => unreachable!("undo и redo не проходят через transact"),
````

на:

````rust
                        ..Default::default()
                    },
                )?;
            }
        }
        Op::TextInsert { node, at, ch } => {
            if let Some(n) = pick(alive, node) {
                if let Some(len) = text_len(p, n)? {
                    let s = ["а", "б", "x", " ", "\n"][ch as usize % 5];
                    p.insert_text(n, at % len, s, None)?;
                }
            }
        }
        Op::TextDelete { node, a, b } => {
            if let Some(n) = pick(alive, node) {
                if let Some(len) = text_len(p, n)? {
                    // Последний перевод строки не удаляется.
                    let body = len - 1;
                    if body > 0 {
                        let start = a % body;
                        p.delete_text(n, start..start + 1 + b % (body - start))?;
                    }
                }
            }
        }
        Op::TextFormat { node, a, b, size } => {
            if let Some(n) = pick(alive, node) {
                if let Some(len) = text_len(p, n)? {
                    let start = a % len;
                    let attrs = TextAttrs {
                        size: Some(10.0 + (size % 3) as f64),
                        ..Default::default()
                    };
                    p.format_text(n, start..start + 1 + b % (len - start), &attrs, &[])?;
                }
            }
        }
        Op::Undo | Op::Redo => unreachable!("undo и redo не проходят через transact"),
````

В `crates/ferrum-doc/tests/invariants.rs` заменить:

````rust
                    });
                    match result {
                        Ok(_) => {
                            if tried {
                                let after = snapshot(doc.page(page).unwrap());
                                m.past.push(replace(&mut m.cur, after));
                                m.future.clear();
                                applied += 1;
````

на:

````rust
                    });
                    match result {
                        Ok(_) => {
                            let after = snapshot(doc.page(page).unwrap());
                            let changed = if is_text_op(&edit) { after != m.cur } else { tried };
                            if changed {
                                m.past.push(replace(&mut m.cur, after));
                                m.future.clear();
                                applied += 1;
````

Run: `cargo test -p ferrum-doc --test text --test invariants`
Expected: FAIL — не собирается: нет `TextMerge`, `Document::transact_merging`.

- [ ] **Step 2: Слияние шагов отмены**

В `crates/ferrum-doc/src/history.rs` заменить:

````rust

use crate::page::Inverse;
use crate::types::{NodeId, PageId};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HistoryEntry {
````

на:

````rust

use crate::page::Inverse;
use crate::types::{NodeId, PageId};

/// Пауза, после которой ввод подряд начинает новый шаг отмены (как в Figma).
pub const MERGE_PAUSE_MS: f64 = 1000.0;

/// Метка правки текста, которая сливается с прошлым шагом отмены: ввод подряд в
/// одном месте — один шаг (спека подсистемы 3, раздел 3). Шаг рвут пауза больше
/// `MERGE_PAUSE_MS`, другая позиция курсора, другой узел или другой стиль.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMerge {
    pub node: NodeId,
    /// Курсор до и после правки, символы Unicode.
    pub before: usize,
    pub after: usize,
    /// Отпечаток стиля вставки (его считает редактор).
    pub style: u64,
    /// Время правки, мс (часы вызывающего).
    pub at_ms: f64,
}

impl TextMerge {
    /// Продолжает ли `next` эту правку.
    pub fn continues(&self, next: &TextMerge) -> bool {
        let pause = next.at_ms - self.at_ms;
        self.node == next.node
            && self.style == next.style
            && self.after == next.before
            && (0.0..=MERGE_PAUSE_MS).contains(&pause)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HistoryEntry {
````

В `crates/ferrum-doc/src/history.rs` заменить:

````rust
pub(crate) struct History {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl History {
````

на:

````rust
pub(crate) struct History {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    /// Метка верхнего шага undo, если его записала сливаемая правка текста и с
    /// тех пор стек не менялся; иначе `None` — следующей правке не с чем слиться.
    last_merge: Option<(PageId, TextMerge)>,
}

impl History {
````

В `crates/ferrum-doc/src/history.rs` заменить:

````rust
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

````

на:

````rust
    pub fn push(&mut self, entry: HistoryEntry) {
        self.undo.push(entry);
        self.redo.clear();
        self.last_merge = None;
    }

    /// Шаг сливаемой правки: продолжает верхний шаг (операции дописываются в
    /// конец — отмена применяет их с конца) или становится новым шагом.
    pub fn push_merging(&mut self, entry: HistoryEntry, merge: TextMerge) {
        let page = entry.page;
        let fits =
            matches!(&self.last_merge, Some((p, prev)) if *p == page && prev.continues(&merge));
        match self.undo.last_mut() {
            Some(top) if fits => {
                top.ops.extend(entry.ops);
                top.selection_after = entry.selection_after;
                self.redo.clear();
            }
            _ => self.push(entry),
        }
        self.last_merge = Some((page, merge));
    }

    pub fn push_redo(&mut self, entry: HistoryEntry) {
        self.redo.push(entry);
        self.last_merge = None;
    }

    pub fn push_undo_keep_redo(&mut self, entry: HistoryEntry) {
        self.undo.push(entry);
        self.last_merge = None;
    }

    pub fn pop_undo(&mut self) -> Option<HistoryEntry> {
        self.last_merge = None;
        self.undo.pop()
    }

    pub fn pop_redo(&mut self) -> Option<HistoryEntry> {
        self.last_merge = None;
        self.redo.pop()
    }

````

В `crates/ferrum-doc/src/history.rs` заменить:

````rust
    pub fn forget_page(&mut self, page: PageId) {
        self.undo.retain(|e| e.page != page);
        self.redo.retain(|e| e.page != page);
    }
}
````

на:

````rust
    pub fn forget_page(&mut self, page: PageId) {
        self.undo.retain(|e| e.page != page);
        self.redo.retain(|e| e.page != page);
        self.last_merge = None;
    }
}
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

use crate::changes::Changes;
use crate::error::{DocError, DocResult};
use crate::history::{History, HistoryEntry};
use crate::images::ImageStore;
use crate::journal::{self, BatchWriter, Replayed};
use crate::page::{Inverse, PageDoc, Tx};
````

на:

````rust

use crate::changes::Changes;
use crate::error::{DocError, DocResult};
use crate::history::{History, HistoryEntry, TextMerge};
use crate::images::ImageStore;
use crate::journal::{self, BatchWriter, Replayed};
use crate::page::{Inverse, PageDoc, Tx};
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    where
        F: FnOnce(&Tx) -> DocResult<Vec<NodeId>>,
    {
        // Сначала загрузка: страница с неизвестными узлами делает документ
        // доступным только для чтения.
        self.page(page)?;
````

на:

````rust
    where
        F: FnOnce(&Tx) -> DocResult<Vec<NodeId>>,
    {
        self.transact_with(page, selection_before, None, f)
    }

    /// Как `transact`, но шаг сливается с прошлым, если тот — продолжение этой
    /// правки текста (`TextMerge::continues`): ввод подряд — один шаг отмены.
    pub fn transact_merging<F>(
        &mut self,
        page: PageId,
        selection_before: Vec<NodeId>,
        merge: TextMerge,
        f: F,
    ) -> DocResult<Vec<NodeId>>
    where
        F: FnOnce(&Tx) -> DocResult<Vec<NodeId>>,
    {
        self.transact_with(page, selection_before, Some(merge), f)
    }

    fn transact_with<F>(
        &mut self,
        page: PageId,
        selection_before: Vec<NodeId>,
        merge: Option<TextMerge>,
        f: F,
    ) -> DocResult<Vec<NodeId>>
    where
        F: FnOnce(&Tx) -> DocResult<Vec<NodeId>>,
    {
        // Сначала загрузка: страница с неизвестными узлами делает документ
        // доступным только для чтения.
        self.page(page)?;
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
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
````

на:

````rust
        match result {
            Ok(selection_after) => {
                if !ops.is_empty() {
                    let entry = HistoryEntry {
                        page,
                        ops,
                        selection_before,
                        selection_after: selection_after.clone(),
                    };
                    match merge {
                        Some(m) => self.history.push_merging(entry, m),
                        None => self.history.push(entry),
                    }
                }
                Ok(selection_after)
            }
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
pub use changes::Changes;
pub use document::{Document, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use images::ImageStore;
pub use journal::Replayed;
pub use page::{PageDoc, Tx};
````

на:

````rust
pub use changes::Changes;
pub use document::{Document, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use history::{TextMerge, MERGE_PAUSE_MS};
pub use images::ImageStore;
pub use journal::Replayed;
pub use page::{PageDoc, Tx};
````

- [ ] **Step 3: JSON текста, узел со всеми свойствами, пример из текстов**

В `crates/ferrum-doc/Cargo.toml` заменить:

````toml
loro-internal.workspace = true
thiserror.workspace = true
blake3.workspace = true

[dev-dependencies]
criterion.workspace = true
````

на:

````toml
loro-internal.workspace = true
thiserror.workspace = true
blake3.workspace = true
# Текст узла в JSON для буфера обмена (`TextData::to_json`): значения Loro как есть.
serde_json.workspace = true

[dev-dependencies]
criterion.workspace = true
````

В `crates/ferrum-doc/src/text_codec.rs` заменить:

````rust
    }
}

/// Разметка участка в атрибуты Loro (только заданные ключи).
pub(crate) fn attrs_to_loro(a: &TextAttrs) -> FxHashMap<String, LoroValue> {
    let mut m = FxHashMap::default();
````

на:

````rust
    }
}

impl TextData {
    /// Текст в JSON для буфера обмена (`web application/x-ferrum`): свойства —
    /// те же ключи, что в карте узла, содержимое — дельта Quill с разметкой.
    pub fn to_json(&self) -> String {
        let props: std::collections::HashMap<String, LoroValue> = text_props_entries(&self.props)
            .into_iter()
            .filter_map(|(k, v)| Some((k.to_string(), v?)))
            .collect();
        let runs: Vec<LoroValue> = self
            .content
            .runs
            .iter()
            .map(|r| {
                obj(vec![
                    ("insert", r.text.as_str().into()),
                    (
                        "attributes",
                        LoroValue::from(
                            attrs_to_loro(&r.attrs)
                                .into_iter()
                                .collect::<std::collections::HashMap<_, _>>(),
                        ),
                    ),
                ])
            })
            .collect();
        let v = obj(vec![
            ("props", LoroValue::from(props)),
            ("runs", LoroValue::from(runs)),
        ]);
        serde_json::to_string(&v).unwrap_or_default()
    }

    /// Текст из JSON буфера обмена. Данные не доверенные: неизвестные ключи и
    /// неверные значения — умолчания, пределы — как у правки (спека 3).
    pub fn from_json(s: &str) -> DocResult<TextData> {
        let bad = || DocError::Corrupted("текст в буфере обмена".into());
        let v: LoroValue = serde_json::from_str(s).map_err(|_| bad())?;
        let props = match field(&v, "props") {
            Some(LoroValue::Map(m)) => read_text_props_with(|k| m.get(k).cloned()),
            _ => return Err(bad()),
        };
        let mut runs: Vec<TextRun> = Vec::new();
        let Some(LoroValue::List(items)) = field(&v, "runs") else {
            return Err(bad());
        };
        for item in items.iter() {
            let text = field(item, "insert").and_then(as_str).ok_or_else(bad)?;
            if text.is_empty() {
                continue;
            }
            let attrs = match field(item, "attributes") {
                Some(LoroValue::Map(m)) => attrs_from_loro(m.iter().map(|(k, v)| (k.as_str(), v))),
                _ => TextAttrs::default(),
            };
            push_run(&mut runs, text, attrs);
        }
        let mut content = TextContent { runs };
        if !content.text().ends_with('\n') {
            push_run(&mut content.runs, "\n".to_string(), TextAttrs::default());
        }
        if content.char_len() > MAX_TEXT_CHARS + 1 {
            return Err(DocError::TextTooLong);
        }
        if content.styled_runs() > MAX_TEXT_RUNS {
            return Err(DocError::TooManyTextRuns);
        }
        Ok(TextData { props, content })
    }
}

/// Разметка участка в атрибуты Loro (только заданные ключи).
pub(crate) fn attrs_to_loro(a: &TextAttrs) -> FxHashMap<String, LoroValue> {
    let mut m = FxHashMap::default();
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        let tid = self.create_tree_node(parent, index)?;
        let id = NodeId(tid);
        self.finish_create(id, tid, Props::defaults(kind))?;
        Ok(id)
    }

````

на:

````rust
        let tid = self.create_tree_node(parent, index)?;
        let id = NodeId(tid);
        self.finish_create(id, tid, Props::defaults(kind))?;
        Ok(id)
    }

    /// Узел сразу со всеми свойствами — копия или вставка; у текста и
    /// содержимое. `index = None` — поверх всех детей.
    pub(crate) fn create_with_props(
        &self,
        parent: Option<NodeId>,
        index: Option<usize>,
        props: &Props,
    ) -> DocResult<NodeId> {
        let tid = self.create_tree_node(parent, index)?;
        let id = NodeId(tid);
        self.finish_create(id, tid, props.clone())?;
        Ok(id)
    }

````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        self.page.set_props(id, patch)
    }

    /// См. [`Tx::move_nodes`]: перенос одного узла.
    pub fn move_node(&self, id: NodeId, parent: Option<NodeId>, index: usize) -> DocResult<()> {
        self.page.move_node(id, parent, index)
````

на:

````rust
        self.page.set_props(id, patch)
    }

    /// Узел сразу со всеми свойствами (копия, вставка; у текста — с содержимым).
    pub fn create_with_props(
        &self,
        parent: Option<NodeId>,
        index: Option<usize>,
        props: &Props,
    ) -> DocResult<NodeId> {
        self.page.create_with_props(parent, index, props)
    }

    /// См. [`Tx::move_nodes`]: перенос одного узла.
    pub fn move_node(&self, id: NodeId, parent: Option<NodeId>, index: usize) -> DocResult<()> {
        self.page.move_node(id, parent, index)
````

В `crates/ferrum-doc/src/fixture.rs` заменить:

````rust
    }
    Ok(ids)
}
````

на:

````rust
    }
    Ok(ids)
}

/// Наполняет страницу `texts` текстовыми узлами: фреймы 1440 × 1024 по 100 текстов
/// фиксированного размера 120 × 40 (Inter 14, две строки) — размеры записаны, и
/// открытие страницы раскладки не требует (спека подсистемы 3, раздел 5).
pub fn fill_texts(page: &Tx, texts: usize) -> DocResult<Vec<NodeId>> {
    use std::sync::Arc;

    use crate::{Props, TextContent, TextData, TextProps, TextSizing};

    let mut ids = Vec::with_capacity(texts);
    let frames = texts.div_ceil(100);
    let mut props = TextProps {
        sizing: TextSizing::Fixed,
        ..TextProps::default()
    };
    props.style.size = 14.0;
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
        for i in 0..100 {
            if ids.len() == texts {
                break;
            }
            let mut p = Props::defaults(NodeKind::Text);
            p.name = format!("Текст {i}");
            (p.x, p.y, p.w, p.h) = (
                (i % 10) as f64 * 140.0,
                (i / 10) as f64 * 100.0,
                120.0,
                40.0,
            );
            p.text = Some(Arc::new(TextData {
                props: props.clone(),
                content: TextContent::plain(&format!("Заголовок {i}\nЭкран {f}, подпись")),
            }));
            ids.push(page.create_with_props(Some(frame), None, &p)?);
        }
    }
    Ok(ids)
}
````

- [ ] **Step 4: Проверить**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ferrum-doc`
Expected: PASS: `text` — 13 тестов, `invariants` — 1 (64 сценария с правками текста; зеркало совпадает с честным чтением Loro и с открытием по кэшу сцены после каждого шага).

- [ ] **Step 5: Совместимость: файл с текстом в сборке без текста**

Создать `crates/ferrum-doc/examples/text_fixture.rs`:

````rust
//! Файл `.ferrum` с текстовым узлом — для проверки совместимости: сборка без
//! текста (подсистема 1) открывает его только для чтения (спека подсистемы 3,
//! раздел 3). `cargo run -p ferrum-doc --example text_fixture -- <путь>`; проверку
//! старой сборкой делает `scripts/compat-old-build.sh`.

use ferrum_doc::container::encode;
use ferrum_doc::{Document, NodeKind, PropsPatch, TextAttrs, TextContent};

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
        let text = tx.create_node(None, None, NodeKind::Text)?;
        tx.set_text_content(text, &TextContent::plain("Текст из новой сборки"))?;
        tx.format_text(
            text,
            0..5,
            &TextAttrs {
                style: Some("Bold".into()),
                ..Default::default()
            },
            &[],
        )?;
        Ok(vec![])
    })
    .unwrap();
    std::fs::write(&path, encode(&doc).unwrap()).unwrap();
    println!("{path}: прямоугольник и текст");
}
````

Создать `scripts/compat-old-build.sh`:

````sh
#!/bin/sh
# Совместимость (спека подсистемы 3, раздел 12): файл с текстом, собранный новой
# сборкой, открывается сборкой без текста (по умолчанию — `main`) только для
# чтения: прямоугольник виден, текст — узел неизвестного типа — скрыт, правка
# отвергается. Старая сборка собирается во временной копии репозитория.
#   scripts/compat-old-build.sh [ref]
set -eu
REF="${1:-main}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
FILE="$TMP/text.ferrum"
cleanup() {
  git -C "$ROOT" worktree remove --force "$TMP/old" >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT
(cd "$ROOT" && cargo run -q -p ferrum-doc --example text_fixture -- "$FILE")
git -C "$ROOT" worktree add --detach "$TMP/old" "$REF" >/dev/null
cat > "$TMP/old/crates/ferrum-doc/tests/compat_text.rs" <<'RS'
use ferrum_doc::container::decode;
use ferrum_doc::{DocError, NodeKind};

#[test]
fn file_with_text_opens_read_only() {
    let bytes = std::fs::read(std::env::var("FERRUM_TEXT_FILE").unwrap()).unwrap();
    let mut doc = decode(&bytes).unwrap().document;
    let page = doc.pages()[0].0;
    let walk = doc.page(page).unwrap().walk();
    assert_eq!(walk.len(), 1, "виден только прямоугольник");
    assert!(doc.is_read_only(), "текст скрыт — документ только для чтения");
    let err = doc.transact(page, vec![], |tx| Ok(vec![tx.create_node(None, None, NodeKind::Rect)?]));
    assert!(matches!(err, Err(DocError::ReadOnly)));
}
RS
(cd "$TMP/old" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}" FERRUM_TEXT_FILE="$FILE" \
  cargo test -q -p ferrum-doc --test compat_text)
echo "Совместимость с $REF: файл с текстом открыт только для чтения"
````

Run: `chmod +x scripts/compat-old-build.sh && ./scripts/compat-old-build.sh main`
Expected: `test result: ok. 1 passed` в копии `main` и последняя строка `Совместимость с main: файл с текстом открыт только для чтения`.

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-doc scripts/compat-old-build.sh Cargo.lock
git commit -m "feat(doc): ввод подряд — один шаг отмены, JSON текста для буфера, узел со всеми свойствами, пример из текстов; проверка совместимости со сборкой без текста

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Вшитые шрифты, `ferrum-fontmeta`, основа `ferrum-text`

Спека 4: десять вшитых семейств (SIL OFL), по возможности вариативные, файлами в сборке; список и метаданные — манифест, генерируемый из файлов. Скрипт `fetch-fonts.mjs` качает файлы google/fonts на закреплённом коммите (имена с `[opsz,wght]` переименовываются в `…-Variable.ttf` — без скобок в адресах) и сверяет SHA-256; `--check` — только сверка. Метаданные (семейство — имя 16, иначе 1; начертание статического — имя 17, иначе 2; вариативного — каждый именованный экземпляр `fvar` с весом из `wght`, шириной из `wdth`, курсивом из файла или `ital`) читает новый маленький крейт `ferrum-fontmeta` — им же пользуется десктоп (Task 10), без `parley` и Loro. Манифест пишет пример `ferrum-text`. Onest уже вшит в wasm (подписи оверлеев): файл переезжает в `ferrum-text`, `ferrum-render` берёт его оттуда.

**Files:**
- Create: `apps/web/scripts/fetch-fonts.mjs`; `apps/web/public/fonts/<семейство>/*.ttf`, `OFL.txt`, `manifest.json` (их создают скрипт и пример); `crates/ferrum-fontmeta/{Cargo.toml,src/lib.rs,tests/faces.rs}`; `crates/ferrum-text/{Cargo.toml,src/lib.rs,src/style_name.rs,examples/font_manifest.rs}`
- Move: `crates/ferrum-render/assets/fonts/{Onest-wght.ttf,OFL.txt}` → `crates/ferrum-text/assets/fonts/`
- Modify: `Cargo.toml` (members), `crates/ferrum-render/Cargo.toml`, `crates/ferrum-render/src/text.rs:24–25`

**Interfaces:**
- Produces: `ferrum_fontmeta::{read_faces(&[u8], file) -> Vec<(String, FaceInfo)>, group(..) -> Vec<FamilyInfo>, FaceInfo { style, weight: f32, italic, stretch: f32, file, index: u32 }, FamilyInfo { family, faces }, MAX_FONT_FILE = 64 МБ}` (serde, `camelCase` — это и формат манифеста, и `FontFamilyDto` протокола); `ferrum_text::{style_name::{style_key, guess_attrs}, ONEST}` и реэкспорт `ferrum_fontmeta`; файл `apps/web/public/fonts/manifest.json` (`{ source, license, families }`, семейства по имени).

- [ ] **Step 1: Скрипт шрифтов и файлы**

Создать `apps/web/scripts/fetch-fonts.mjs`:

````js
// Вшитые шрифты (спека подсистемы 3, раздел 4): скачивает файлы из репозитория
// google/fonts на закреплённом коммите, сверяет SHA-256 и кладёт их с лицензиями
// в public/fonts/<каталог>/. Файлы лежат в git: скрипт нужен, чтобы обновить их
// или проверить, что они те же (`--check`). Манифест (`manifest.json`) строит
// из файлов `cargo run -p ferrum-text --example font_manifest`.
//
// Все шрифты — SIL Open Font License 1.1 (OFL.txt рядом с файлами семейства).
// Onest в public/fonts нет: он уже вшит в wasm (crates/ferrum-text/assets) и
// регистрируется ядром без загрузки.

import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Коммит google/fonts от 2026-10-09. */
export const COMMIT = 'bd8f81ddb5c74d5c8897b36ad88b440266245103';
const BASE = `https://raw.githubusercontent.com/google/fonts/${COMMIT}/ofl/`;
const OUT = fileURLToPath(new URL('../public/fonts/', import.meta.url));

/** [путь в google/fonts, файл в public/fonts, SHA-256]. */
export const FILES = [
  ['inter/Inter[opsz,wght].ttf', 'inter/Inter-Variable.ttf', '29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031'],
  ['inter/Inter-Italic[opsz,wght].ttf', 'inter/Inter-Italic-Variable.ttf', 'acd98e64795781b2058f07b18475e0ecee2a0fe2b42a49e2f9e37d0d6bf66ce6'],
  ['inter/OFL.txt', 'inter/OFL.txt', '5b9321a4298cfeb6b34354164a1c3afc3db114569984c502b9b35d988fd58c57'],
  ['roboto/Roboto[wdth,wght].ttf', 'roboto/Roboto-Variable.ttf', 'd7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134'],
  ['roboto/Roboto-Italic[wdth,wght].ttf', 'roboto/Roboto-Italic-Variable.ttf', '9725a847af6b460ffca162ae66d20dad48b01876137947180b42d7dcd7887182'],
  ['roboto/OFL.txt', 'roboto/OFL.txt', '061402327a96aadb0bfb694a960ed289ecd38d383e396243831ab81feb109c41'],
  ['opensans/OpenSans[wdth,wght].ttf', 'opensans/OpenSans-Variable.ttf', '36643644f318a812aab2d2ed3bb98f8cf0872527f835fe9398d95fe6b9adb878'],
  ['opensans/OpenSans-Italic[wdth,wght].ttf', 'opensans/OpenSans-Italic-Variable.ttf', 'fe269381e992f32e135801740998544d6235061e37c93ec067ad2be3edd5b17b'],
  ['opensans/OFL.txt', 'opensans/OFL.txt', 'fbbbcfef55318de350562559b671360de6d597112ecc5c73881b05092db89602'],
  ['montserrat/Montserrat[wght].ttf', 'montserrat/Montserrat-Variable.ttf', '0f7b311b2f3279e4eef9b2f968bcdbab6e28f4daeb1f049f4f278a902bcd82f7'],
  ['montserrat/Montserrat-Italic[wght].ttf', 'montserrat/Montserrat-Italic-Variable.ttf', '51607f316bc020e59f03cbf51543eecffbea501c0b31d73e5b82927c5cca442c'],
  ['montserrat/OFL.txt', 'montserrat/OFL.txt', '8b7141c03fa4f8d44e6345d5d4931709290f0f67875e452e95ac1fd3a027802e'],
  ['ptsans/PT_Sans-Web-Regular.ttf', 'ptsans/PTSans-Regular.ttf', '9cc831490532009bae2b3ce0d39c62adfc889060beb421593bfd9d2396d0f10a'],
  ['ptsans/PT_Sans-Web-Bold.ttf', 'ptsans/PTSans-Bold.ttf', '3128bd5ecf01816e59a23d54c57a7a6b14615b07db53ff277c77376010265b05'],
  ['ptsans/PT_Sans-Web-Italic.ttf', 'ptsans/PTSans-Italic.ttf', '5a90fe2d0cd798700935240580bdcc12c0ffc9102c0c7163b3418e13bc21debd'],
  ['ptsans/PT_Sans-Web-BoldItalic.ttf', 'ptsans/PTSans-BoldItalic.ttf', '81ac221cdd02bccfa679c74adb122478e9d092e65a722e31ca11469961483785'],
  ['ptsans/OFL.txt', 'ptsans/OFL.txt', '2758cf7a872827f39661cf8cc24188113c030447aefb5ca7145993650076ca8c'],
  ['ptserif/PT_Serif-Web-Regular.ttf', 'ptserif/PTSerif-Regular.ttf', 'a4951fade06ff8f09b7673aa81ffb65a8cd409e24d3289a6dc670bc4dda2557a'],
  ['ptserif/PT_Serif-Web-Bold.ttf', 'ptserif/PTSerif-Bold.ttf', '038ba7336bd7ea14f12ad155bed51a4345cac5153275d521dec3ba04021c526e'],
  ['ptserif/PT_Serif-Web-Italic.ttf', 'ptserif/PTSerif-Italic.ttf', 'f57e95ff9dc85691a3b2e193f2028db36f6663939a46c0fc4f286d618b80b7ce'],
  ['ptserif/PT_Serif-Web-BoldItalic.ttf', 'ptserif/PTSerif-BoldItalic.ttf', 'f003788ba08981eb0988b3557a6f224a53dab49c20e283e8b74d5af3c466f8be'],
  ['ptserif/OFL.txt', 'ptserif/OFL.txt', 'ddf311c28ddf5a5ad9747649837346b67bed9d356789c3072bb27dbce49e514d'],
  ['notosans/NotoSans[wdth,wght].ttf', 'notosans/NotoSans-Variable.ttf', 'bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d'],
  ['notosans/NotoSans-Italic[wdth,wght].ttf', 'notosans/NotoSans-Italic-Variable.ttf', '58e6e0ebd1931b29a365aa2d3e2ee9a9e831a3af7cf3ad1462d4e72154f0b291'],
  ['notosans/OFL.txt', 'notosans/OFL.txt', 'cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a'],
  ['jetbrainsmono/JetBrainsMono[wght].ttf', 'jetbrainsmono/JetBrainsMono-Variable.ttf', '48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda'],
  ['jetbrainsmono/JetBrainsMono-Italic[wght].ttf', 'jetbrainsmono/JetBrainsMono-Italic-Variable.ttf', '85ae2a5cd3f56baf1ce1c21a851322c58e3d8fbe8e8ad4a4d090a820dd7fe558'],
  ['jetbrainsmono/OFL.txt', 'jetbrainsmono/OFL.txt', 'b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d'],
  ['playfairdisplay/PlayfairDisplay[wght].ttf', 'playfairdisplay/PlayfairDisplay-Variable.ttf', 'c40f2293766a503bc70cce9e512ef844a4ccb7cbcde792fe2ea31d191917d8d6'],
  ['playfairdisplay/PlayfairDisplay-Italic[wght].ttf', 'playfairdisplay/PlayfairDisplay-Italic-Variable.ttf', 'a5e26dc5e2e77fb2803a0bf02fd4f81ee136ec8dea863ccdb0c59a263b21378b'],
  ['playfairdisplay/OFL.txt', 'playfairdisplay/OFL.txt', '566be814f8e96e93dfa16101331557eb6b5467e9e03f627c0910fe93ca12300e'],
];

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const check = process.argv.includes('--check');
let failed = 0;
for (const [src, dst, hash] of FILES) {
  const path = OUT + dst;
  if (existsSync(path) && sha256(readFileSync(path)) === hash) continue;
  if (check) {
    console.error(`public/fonts/${dst}: нет или не совпадает SHA-256`);
    failed++;
    continue;
  }
  // Скобки и запятые в именах файлов google/fonts — в адресе экранируются.
  const url = BASE + src.split('/').map(encodeURIComponent).join('/');
  const res = await fetch(url);
  if (!res.ok) {
    console.error(`${url}: HTTP ${res.status}`);
    failed++;
    continue;
  }
  const bytes = Buffer.from(await res.arrayBuffer());
  if (sha256(bytes) !== hash) {
    console.error(`${url}: SHA-256 не совпал`);
    failed++;
    continue;
  }
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, bytes);
  console.log(`public/fonts/${dst} (${bytes.length} байт)`);
}
if (failed) process.exit(1);
````

Run: `node apps/web/scripts/fetch-fonts.mjs && node apps/web/scripts/fetch-fonts.mjs --check`
Expected: 31 строка `public/fonts/… (N байт)` (22 файла `.ttf` и 9 `OFL.txt`, ≈ 13 МБ), затем сверка без вывода, код 0.

- [ ] **Step 2: Заготовки крейтов, Onest — в `ferrum-text`, падающий тест метаданных**

Run: `mkdir -p crates/ferrum-text/assets && git mv crates/ferrum-render/assets/fonts crates/ferrum-text/assets/fonts`

Создать `crates/ferrum-text/Cargo.toml`:

````toml
[package]
name = "ferrum-text"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
ferrum-doc = { path = "../ferrum-doc" }
ferrum-fontmeta = { path = "../ferrum-fontmeta" }
kurbo.workspace = true
parley.workspace = true
skrifa.workspace = true
serde.workspace = true
serde_json.workspace = true
````

Создать `crates/ferrum-text/src/lib.rs`:

````rust
//! Текстовый движок (спека подсистемы 3, разделы 4–5): реестр шрифтов (вшитые,
//! системные, отсутствующие с запасным Inter), раскладка текстового узла через
//! `parley` с собственной коллекцией `fontique` без системного поиска и контуры
//! глифов из `skrifa`. О Loro и рендере не знает: получает `TextData` узла,
//! отдаёт строки и глифы.

pub mod style_name;

pub use ferrum_fontmeta::{group, read_faces, FaceInfo, FamilyInfo, MAX_FONT_FILE};

/// Onest (SIL OFL 1.1, `assets/fonts/OFL.txt`) вшит в wasm: им набраны подписи
/// оверлеев (`ferrum-render`), и в реестре он — вшитое семейство, готовое без
/// загрузки.
pub const ONEST: &[u8] = include_bytes!("../assets/fonts/Onest-wght.ttf");
````

Создать `crates/ferrum-text/src/style_name.rs`:

````rust
//! Имена начертаний. В документе шрифт — семейство и имя начертания, как
//! `fontName` в Figma («Semi Bold Italic»); у файлов те же начертания пишутся
//! по-разному («SemiBold Italic», «Semibold Italic»). Сравнение — по ключу без
//! пробелов, дефисов и регистра.

/// Ключ имени начертания: строчные буквы и цифры без разделителей.
pub fn style_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Вес и курсив по имени начертания, если о шрифте больше ничего не известно
/// (запасной Inter для отсутствующего шрифта): «Bold Italic» → 700 и курсив.
pub fn guess_attrs(name: &str) -> (f32, bool) {
    let key = style_key(name);
    let italic = key.contains("italic") || key.contains("oblique");
    // Сначала длинные: «extrabold» раньше «bold», «semibold» раньше «bold».
    const WEIGHTS: [(&str, f32); 14] = [
        ("hairline", 100.0),
        ("thin", 100.0),
        ("extralight", 200.0),
        ("ultralight", 200.0),
        ("semilight", 350.0),
        ("light", 300.0),
        ("medium", 500.0),
        ("semibold", 600.0),
        ("demibold", 600.0),
        ("extrabold", 800.0),
        ("ultrabold", 800.0),
        ("bold", 700.0),
        ("black", 900.0),
        ("heavy", 900.0),
    ];
    let weight = WEIGHTS
        .iter()
        .find(|(w, _)| key.contains(w))
        .map_or(400.0, |(_, v)| *v);
    (weight, italic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_spaces_and_case() {
        assert_eq!(style_key("Semi Bold Italic"), style_key("SemiBold Italic"));
        assert_eq!(style_key("Extra-Light"), "extralight");
    }

    #[test]
    fn guesses_figma_style_names() {
        assert_eq!(guess_attrs("Regular"), (400.0, false));
        assert_eq!(guess_attrs("Italic"), (400.0, true));
        assert_eq!(guess_attrs("Semi Bold Italic"), (600.0, true));
        assert_eq!(guess_attrs("Extra Bold"), (800.0, false));
        assert_eq!(guess_attrs("Bold"), (700.0, false));
        assert_eq!(guess_attrs("Extra Light"), (200.0, false));
        assert_eq!(guess_attrs("Black Italic"), (900.0, true));
    }
}
````

В `crates/ferrum-render/Cargo.toml` заменить:

````toml
[dependencies]
ferrum-doc = { path = "../ferrum-doc" }
ferrum-scene = { path = "../ferrum-scene" }
kurbo.workspace = true
vello_cpu.workspace = true
vello_gpu = { workspace = true, optional = true }
````

на:

````toml
[dependencies]
ferrum-doc = { path = "../ferrum-doc" }
ferrum-scene = { path = "../ferrum-scene" }
ferrum-text = { path = "../ferrum-text" }
kurbo.workspace = true
vello_cpu.workspace = true
vello_gpu = { workspace = true, optional = true }
````

В `crates/ferrum-render/src/text.rs` заменить:

````rust

use crate::backend::{RenderBackend, Rgba};

/// Onest (SIL Open Font License 1.1, лицензия — `assets/fonts/OFL.txt`).
pub const ONEST: &[u8] = include_bytes!("../assets/fonts/Onest-wght.ttf");

/// Сколько записей держать в каждом кэше подписей; при переполнении вытесняется
/// запись, которой не пользовались дольше всех.
````

на:

````rust

use crate::backend::{RenderBackend, Rgba};

/// Onest (SIL Open Font License 1.1) — тот же файл, что вшитое семейство
/// реестра шрифтов (`crates/ferrum-text/assets/fonts`).
pub use ferrum_text::ONEST;

/// Сколько записей держать в каждом кэше подписей; при переполнении вытесняется
/// запись, которой не пользовались дольше всех.
````

Создать `crates/ferrum-fontmeta/Cargo.toml`:

````toml
[package]
name = "ferrum-fontmeta"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

# Метаданные файлов шрифтов (семейство, начертания) — без раскладки и Loro:
# им пользуются ferrum-text (манифест вшитых шрифтов) и десктоп (системные шрифты).
[dependencies]
skrifa.workspace = true
serde.workspace = true
````

Создать `crates/ferrum-fontmeta/tests/faces.rs`:

````rust
//! Метаданные файлов шрифтов на вшитых файлах (`apps/web/public/fonts`):
//! семейство и начертания вариативного шрифта — именованные экземпляры, у
//! статического — подсемейство; неразборчивый файл — пустой список.

use std::path::PathBuf;

use ferrum_fontmeta::{group, read_faces};

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn variable_and_static_faces_are_named_like_figma() {
    let faces = group(read_faces(&font_file("inter/Inter-Variable.ttf"), "x.ttf"));
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].family, "Inter");
    let styles: Vec<&str> = faces[0].faces.iter().map(|f| f.style.as_str()).collect();
    assert!(
        styles.contains(&"Regular") && styles.contains(&"Bold"),
        "{styles:?}"
    );
    let bold = faces[0].faces.iter().find(|f| f.style == "Bold").unwrap();
    assert_eq!(
        (bold.weight, bold.italic, bold.file.as_str()),
        (700.0, false, "x.ttf")
    );
    let static_faces = group(read_faces(
        &font_file("ptsans/PTSans-BoldItalic.ttf"),
        "y.ttf",
    ));
    assert_eq!(static_faces[0].family, "PT Sans");
    assert_eq!(static_faces[0].faces[0].style, "Bold Italic");
    assert!(static_faces[0].faces[0].italic);
    // Мусор — пустой список, не паника.
    assert!(read_faces(b"not a font", "z.ttf").is_empty());
}
````

В `Cargo.toml` заменить:

````toml
    "crates/ferrum-render",
    "crates/ferrum-editor",
    "crates/ferrum-wasm",
]
# Десктоп — свой workspace со своим Cargo.lock (Tauri и сотни крейтов окна).
exclude = ["apps/desktop/src-tauri"]
````

на:

````toml
    "crates/ferrum-render",
    "crates/ferrum-editor",
    "crates/ferrum-wasm",
    "crates/ferrum-fontmeta",
    "crates/ferrum-text",
]
# Десктоп — свой workspace со своим Cargo.lock (Tauri и сотни крейтов окна).
exclude = ["apps/desktop/src-tauri"]
````

Run: `cargo test -p ferrum-fontmeta`
Expected: FAIL — cargo не находит библиотеку `ferrum_fontmeta`: у крейта ещё нет `src/lib.rs`.

- [ ] **Step 3: `ferrum-fontmeta`**

Создать `crates/ferrum-fontmeta/src/lib.rs`:

````rust
//! Метаданные файла шрифта: семейство и начертания (спека подсистемы 3, раздел 4).
//! Начертание статического шрифта — его подсемейство; у вариативного — каждый
//! именованный экземпляр (`fvar`), как их показывает Figma. Один формат описания
//! у манифеста вшитых шрифтов, системных шрифтов Chrome и десктопа.

use serde::{Deserialize, Serialize};
use skrifa::raw::types::NameId;
use skrifa::raw::{FileRef, TableProvider};
use skrifa::{FontRef, MetadataProvider, Tag};

/// Наибольший файл шрифта (спека 4): больше — не читается.
pub const MAX_FONT_FILE: usize = 64 * 1024 * 1024;

/// Начертание: имя («Semi Bold Italic»), вес 1–1000, курсив, ширина (1 — обычная)
/// и где его взять — файл и номер шрифта в коллекции (`.ttc`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceInfo {
    pub style: String,
    pub weight: f32,
    pub italic: bool,
    pub stretch: f32,
    pub file: String,
    #[serde(default)]
    pub index: u32,
}

/// Семейство и его начертания.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyInfo {
    pub family: String,
    pub faces: Vec<FaceInfo>,
}

fn name(font: &FontRef, id: NameId) -> Option<String> {
    let s: String = font
        .localized_strings(id)
        .english_or_first()?
        .chars()
        .collect();
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Начертания всех шрифтов файла (у коллекции — каждого) с именем семейства.
/// Неразборчивый файл — пустой список (вызывающий пишет сообщение в журнал).
pub fn read_faces(bytes: &[u8], file: &str) -> Vec<(String, FaceInfo)> {
    if bytes.len() > MAX_FONT_FILE {
        return Vec::new();
    }
    let fonts: Vec<(u32, FontRef)> = match FileRef::new(bytes) {
        Ok(FileRef::Font(f)) => vec![(0, f)],
        Ok(FileRef::Collection(c)) => c
            .iter()
            .enumerate()
            .filter_map(|(i, f)| Some((i as u32, f.ok()?)))
            .collect(),
        Err(_) => Vec::new(),
    };
    let mut out = Vec::new();
    for (index, font) in fonts {
        let Some(family) = name(&font, NameId::TYPOGRAPHIC_FAMILY_NAME)
            .or_else(|| name(&font, NameId::FAMILY_NAME))
        else {
            continue;
        };
        let attrs = font.attributes();
        let italic_file = !matches!(attrs.style, skrifa::attribute::Style::Normal);
        let axes = font.axes();
        let instances = font.named_instances();
        if instances.is_empty() {
            let style = name(&font, NameId::TYPOGRAPHIC_SUBFAMILY_NAME)
                .or_else(|| name(&font, NameId::SUBFAMILY_NAME))
                .unwrap_or_else(|| "Regular".to_string());
            out.push((
                family,
                FaceInfo {
                    style,
                    weight: attrs.weight.value(),
                    italic: italic_file,
                    stretch: attrs.stretch.ratio(),
                    file: file.to_string(),
                    index,
                },
            ));
            continue;
        }
        for inst in instances.iter() {
            let Some(style) = name(&font, inst.subfamily_name_id()) else {
                continue;
            };
            let coord = |tag: &[u8; 4]| {
                let axis = axes.get_by_tag(Tag::new(tag))?;
                inst.user_coords().nth(axis.index())
            };
            out.push((
                family.clone(),
                FaceInfo {
                    style,
                    weight: coord(b"wght").unwrap_or(attrs.weight.value()),
                    italic: italic_file || coord(b"ital").is_some_and(|v| v >= 1.0),
                    stretch: coord(b"wdth").map_or(attrs.stretch.ratio(), |w| w / 100.0),
                    file: file.to_string(),
                    index,
                },
            ));
        }
    }
    // Таблица `head` есть у любого разборчивого шрифта: без неё — мусор.
    out.retain(|_| FontRef::new(bytes).map_or(true, |f| f.head().is_ok()));
    out
}

/// Семейства из начертаний в порядке первого появления; повторы (то же имя
/// начертания в двух файлах) отбрасываются.
pub fn group(faces: impl IntoIterator<Item = (String, FaceInfo)>) -> Vec<FamilyInfo> {
    let mut out: Vec<FamilyInfo> = Vec::new();
    for (family, face) in faces {
        let entry = match out.iter_mut().position(|f| f.family == family) {
            Some(i) => &mut out[i],
            None => {
                out.push(FamilyInfo {
                    family,
                    faces: Vec::new(),
                });
                out.last_mut().expect("только что добавлено")
            }
        };
        if !entry.faces.iter().any(|f| f.style == face.style) {
            entry.faces.push(face);
        }
    }
    out
}
````

Run: `cargo test -p ferrum-fontmeta`
Expected: PASS: `faces` — 1 тест (Inter: «Regular» и «Bold» с весом 700 из `wght`; PT Sans Bold Italic — статическое начертание с курсивом; мусор — пустой список).

- [ ] **Step 4: Манифест вшитых шрифтов**

Создать `crates/ferrum-text/examples/font_manifest.rs`:

````rust
//! Манифест вшитых шрифтов (спека подсистемы 3, раздел 4): семейства и
//! начертания из файлов `apps/web/public/fonts/**.ttf` — теми же функциями, что
//! читают системные шрифты. Запуск после `node apps/web/scripts/fetch-fonts.mjs`:
//! `cargo run -p ferrum-text --example font_manifest` (с `--check` — сверить
//! записанный манифест с файлами, код выхода 1 при расхождении).

use std::path::{Path, PathBuf};

use ferrum_text::{group, read_faces, FamilyInfo};

fn ttf_files(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            ttf_files(&p, root, out);
        } else if p.extension().is_some_and(|x| x == "ttf") {
            out.push(p.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/public/fonts");
    let mut files = Vec::new();
    ttf_files(&root, &root, &mut files);
    let mut faces = Vec::new();
    for rel in &files {
        let name = rel.to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(root.join(rel)).unwrap();
        let found = read_faces(&bytes, &name);
        assert!(!found.is_empty(), "{name}: не шрифт");
        faces.extend(found);
    }
    let mut families: Vec<FamilyInfo> = group(faces);
    families.sort_by(|a, b| a.family.cmp(&b.family));
    let manifest = serde_json::json!({
        "source": "https://github.com/google/fonts (коммит в apps/web/scripts/fetch-fonts.mjs)",
        "license": "SIL Open Font License 1.1 (OFL.txt в каталоге семейства)",
        "families": families,
    });
    let text = serde_json::to_string_pretty(&manifest).unwrap() + "\n";
    let path = root.join("manifest.json");
    if std::env::args().any(|a| a == "--check") {
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        if old != text {
            eprintln!("{} устарел: запустите без --check", path.display());
            std::process::exit(1);
        }
        return;
    }
    std::fs::write(&path, text).unwrap();
    println!("{}: {} семейств", path.display(), families.len());
}
````

Run: `cargo run -p ferrum-text --example font_manifest && cargo run -p ferrum-text --example font_manifest -- --check`
Expected: `…/apps/web/public/fonts/manifest.json: 9 семейств`, затем сверка без вывода. В манифесте: Inter — 18 начертаний (два файла), Roboto — 36 (с узкими по `wdth`), PT Sans и PT Serif — по 4 статических.

- [ ] **Step 5: Проверить**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ferrum-fontmeta -p ferrum-text -p ferrum-render`
Expected: PASS; `ferrum-text` — 2 модульных теста имён начертаний, рендер — как прежде (подписи оверлеев — тем же Onest).

- [ ] **Step 6: Закоммитить**

```bash
git add Cargo.toml Cargo.lock apps/web/scripts/fetch-fonts.mjs apps/web/public/fonts crates/ferrum-fontmeta crates/ferrum-text crates/ferrum-render
git commit -m "feat(text): вшитые шрифты (google/fonts, OFL) со скриптом загрузки и манифестом, крейт метаданных ferrum-fontmeta, основа ferrum-text; Onest переехал в ferrum-text

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Реестр шрифтов и раскладка (`ferrum-text`)

Спека 4–5. `Fonts` — реестр: каталоги источников (`add_catalog`: вшитые из манифеста, системные; вшитое не подменяется системным с тем же именем), разрешение шрифта документа (`resolve`: готов, грузится — с запросом байтов всех файлов семейства один раз, отсутствует — Inter того же начертания, иначе Inter Regular; до списка вшитых — `Pending`, отсутствие не определяется), загрузка (`load`, файл ≤ 64 МБ, неразборчивый — ошибка и «отсутствует»), вытеснение сверх 256 МБ — семейства не текущей страницы, давно не нужные (`unregister_font`), кэш контуров глифов. Коллекция `fontique` без системных шрифтов: в wasm их искать негде, и вид текста не должен зависеть от машины.

Раскладка (`Fonts::layout`) — по абзацам: у каждого свой `parley::Layout` (отступ списка, маркер, отступ первой строки `set_text_indent`, интервал после). Стиль участка — свойства `parley` на его диапазоне; корневой стиль абзаца — стиль первого участка (иначе строку подпирал бы корневой 16 px `parley`). Кисть — номер участка: по нему прогон глифов получает цвет и начертание (для многоточия). Длинное слово переносится по буквам (`OverflowWrap::BreakWord`, как в Figma). Обрезка (`truncate`): строки, помещающиеся в рамку `Fixed` и не больше `max_lines`; у последней — «…» тем же шрифтом, лишние глифы снимаются. Выравнивание по вертикали — сдвигом всего в рамке `Fixed`.

**Files:**
- Create: `crates/ferrum-text/src/fonts.rs`, `crates/ferrum-text/src/layout.rs`
- Modify: `crates/ferrum-text/src/lib.rs`
- Test: `crates/ferrum-text/tests/layout.rs`

**Interfaces:**
- Consumes: `ferrum_doc::{TextData, TextAttrs, FontName, …}` (Task 1), `ferrum_fontmeta` (Task 3).
- Produces: `Fonts` (`new`, `shared() -> SharedFonts`, `generation`, `bundled_ready`, `add_catalog(SourceKind, Vec<FamilyInfo>)`, `families() -> Vec<(String, SourceKind, Vec<String>)>`, `set_in_use(families)`, `take_requests() -> Vec<FontRequest>`, `take_errors() -> Vec<String>`, `status(&FontName) -> FontStatus`, `resolve(&FontName) -> Resolved`, `load(family, file, Vec<u8>) -> Result<(), String>`, `load_failed(family, file)`, `memory`, `units_per_em(&FontData)`, `outline(&FontData, glyph, coords) -> Arc<BezPath>`, `layout(&LayoutRequest) -> TextLayout`, `set_memory_limit_for_tests`); `SharedFonts = Rc<RefCell<Fonts>>`; `SourceKind { System, Bundled, Embedded }`; `FontStatus { Ready, Loading, Missing, Pending }`; `FontRequest { family, source, files }`; `LayoutRequest { text, fills, width: Option<f64>, height: Option<f64> }` и `LayoutRequest::for_node(text, fills, w, h)`; `TextLayout { width, height, lines: Vec<LineBox>, runs: Vec<GlyphRun>, decorations: Vec<DecorationBox>, missing: Vec<FontName>, loading, truncated }`; `LineBox { rect, baseline, color }`; `GlyphRun { font, face, size, coords: Vec<i16>, color, skew, glyphs: Vec<PlacedGlyph { id, x, y }> }`; `text_color(&[Fill]) -> Color`; константы `MAX_FONT_MEMORY = 256 МБ`, `MAX_FAMILIES = 4096`, `MAX_FAMILY_NAME = 256`, `FALLBACK_FAMILY = "Inter"`, `LIST_INDENT_EM = 1.5`.

- [ ] **Step 1: Падающие тесты раскладки на вшитых шрифтах**

Создать `crates/ferrum-text/tests/layout.rs`:

````rust
//! Раскладка текста на вшитых шрифтах (файлы — `apps/web/public/fonts`):
//! вариативные начертания, перенос строк, абзацы и списки, украшения,
//! отсутствующий шрифт и загрузка, обрезка с многоточием, вытеснение.

use std::path::PathBuf;

use ferrum_doc::*;
use ferrum_text::*;

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Реестр со списком вшитых Inter и Roboto; Inter загружен, Roboto — нет.
fn fonts() -> Fonts {
    let mut fonts = Fonts::new();
    let mut faces = Vec::new();
    for rel in [
        "inter/Inter-Variable.ttf",
        "inter/Inter-Italic-Variable.ttf",
        "roboto/Roboto-Variable.ttf",
        "roboto/Roboto-Italic-Variable.ttf",
    ] {
        faces.extend(read_faces(&font_file(rel), rel));
    }
    fonts.add_catalog(SourceKind::Bundled, group(faces));
    for rel in [
        "inter/Inter-Variable.ttf",
        "inter/Inter-Italic-Variable.ttf",
    ] {
        fonts.load("Inter", rel, font_file(rel)).unwrap();
    }
    fonts.take_requests();
    fonts
}

fn text(s: &str, f: impl FnOnce(&mut TextProps)) -> TextData {
    let mut props = TextProps::default();
    props.style.size = 24.0;
    f(&mut props);
    TextData {
        props,
        content: TextContent::plain(s),
    }
}

fn black() -> Vec<Fill> {
    vec![Fill::solid(Color::BLACK)]
}

fn lay(fonts: &mut Fonts, t: &TextData, w: f64, h: f64) -> TextLayout {
    fonts.layout(&LayoutRequest::for_node(t, &black(), w, h))
}

#[test]
fn variable_weight_comes_from_the_style_name() {
    let mut fonts = fonts();
    let regular = lay(&mut fonts, &text("Ferrum", |_| {}), 0.0, 0.0);
    let bold = lay(
        &mut fonts,
        &text("Ferrum", |p| {
            p.style.font = FontName::new("Inter", "Semi Bold")
        }),
        0.0,
        0.0,
    );
    assert!(!regular.loading && !bold.loading);
    assert!(regular.missing.is_empty() && bold.missing.is_empty());
    assert!(
        bold.width > regular.width,
        "{} {}",
        bold.width,
        regular.width
    );
    // Вес вариативного шрифта — координатой оси wght, а не синтетическим жирным.
    assert!(
        regular.runs[0].coords.iter().all(|c| *c == 0)
            || regular.runs[0].coords != bold.runs[0].coords
    );
    assert_ne!(bold.runs[0].coords, regular.runs[0].coords);
    let g = bold.runs[0].glyphs[0];
    let path = fonts.outline(&bold.runs[0].font, g.id, &bold.runs[0].coords);
    assert!(!path.elements().is_empty());
}

#[test]
fn auto_width_keeps_one_line_per_paragraph() {
    let mut fonts = fonts();
    let one = lay(&mut fonts, &text("Привет, мир", |_| {}), 0.0, 0.0);
    assert_eq!(one.lines.len(), 1);
    assert!(one.width > 100.0 && one.width < 200.0, "{}", one.width);
    // «Авто» — по метрикам Inter: около 1.21 кегля.
    assert!((one.height - 24.0 * 1.21).abs() < 1.0, "{}", one.height);
    let two = lay(
        &mut fonts,
        &text("Привет\nмир", |p| p.paragraph_spacing = 10.0),
        0.0,
        0.0,
    );
    assert_eq!(two.lines.len(), 2);
    assert!(
        (two.height - (2.0 * one.height + 10.0)).abs() < 0.5,
        "{}",
        two.height
    );
    assert!(two.width < one.width);
    // Пустой абзац — строка своей высоты.
    let blank = lay(&mut fonts, &text("а\n\nб", |_| {}), 0.0, 0.0);
    assert_eq!(blank.lines.len(), 3);
}

#[test]
fn auto_height_wraps_at_the_box_width() {
    let mut fonts = fonts();
    let t = text(
        "Съешь же ещё этих мягких французских булок",
        |p| p.sizing = TextSizing::AutoHeight,
    );
    let l = lay(&mut fonts, &t, 150.0, 0.0);
    assert!(l.lines.len() >= 3, "{}", l.lines.len());
    for line in &l.lines {
        assert!(line.rect.x1 <= 150.0 + 0.5, "{:?}", line.rect);
    }
    // По центру: строки посередине рамки.
    let c = text("ab", |p| {
        p.sizing = TextSizing::AutoHeight;
        p.align_h = AlignH::Center;
    });
    let l = lay(&mut fonts, &c, 200.0, 0.0);
    let r = l.lines[0].rect;
    assert!(((r.x0 + r.x1) / 2.0 - 100.0).abs() < 1.0, "{r:?}");
}

#[test]
fn styles_colors_and_decorations_by_range() {
    let mut fonts = fonts();
    let mut t = text("Красный и подчёркнутый", |_| {});
    let red = Fill::solid(Color::rgba(255, 0, 0, 255));
    t.content = TextContent {
        runs: vec![
            TextRun {
                text: "Красный".into(),
                attrs: TextAttrs {
                    fill: Some(red),
                    size: Some(40.0),
                    ..Default::default()
                },
            },
            TextRun {
                text: " и ".into(),
                attrs: TextAttrs::default(),
            },
            TextRun {
                text: "подчёркнутый\n".into(),
                attrs: TextAttrs {
                    decoration: Some(Decoration::Underline),
                    ..Default::default()
                },
            },
        ],
    };
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert_eq!(l.runs[0].color, Color::rgba(255, 0, 0, 255));
    assert_eq!(l.runs[0].size, 40.0);
    assert_eq!(l.runs.last().unwrap().color, Color::BLACK);
    assert_eq!(l.decorations.len(), 1);
    let d = l.decorations[0].rect;
    // Под базовой линией, по ширине подчёркнутого участка.
    assert!(
        d.y0 > l.lines[0].baseline && d.y0 < l.lines[0].baseline + 6.0,
        "{d:?}"
    );
    assert!(d.width() > 100.0);
}

#[test]
fn lists_get_markers_and_numbers() {
    let mut fonts = fonts();
    let mut t = text("", |_| {});
    let item = |s: &str, list: ListKind| {
        [
            TextRun {
                text: s.into(),
                attrs: TextAttrs::default(),
            },
            TextRun {
                text: "\n".into(),
                attrs: TextAttrs {
                    list: Some(list),
                    ..Default::default()
                },
            },
        ]
    };
    t.content = TextContent {
        runs: [
            item("один", ListKind::Ordered),
            item("два", ListKind::Ordered),
            item("пункт", ListKind::Bullet),
        ]
        .concat(),
    };
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert_eq!(l.lines.len(), 3);
    // Текст сдвинут на ступень списка, маркеры — левее текста на тех же строках.
    for line in &l.lines {
        assert!(
            (line.rect.x0 - LIST_INDENT_EM * 24.0).abs() < 0.5,
            "{:?}",
            line.rect
        );
    }
    let left_of_text = l
        .runs
        .iter()
        .flat_map(|r| r.glyphs.iter())
        .filter(|g| (g.x as f64) < LIST_INDENT_EM * 24.0 - 0.5)
        .count();
    // «1.», «2.» — по два глифа, «•» — один.
    assert_eq!(left_of_text, 5);
}

#[test]
fn missing_font_falls_back_to_inter_of_the_same_style() {
    let mut fonts = fonts();
    let t = text("Ferrum", |p| {
        p.style.font = FontName::new("Comic Sans", "Bold")
    });
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert!(!l.loading);
    assert_eq!(l.missing, vec![FontName::new("Comic Sans", "Bold")]);
    let inter_bold = lay(
        &mut fonts,
        &text("Ferrum", |p| p.style.font = FontName::new("Inter", "Bold")),
        0.0,
        0.0,
    );
    assert_eq!(l.width, inter_bold.width);
    assert_eq!(
        fonts.status(&FontName::new("Comic Sans", "Bold")),
        FontStatus::Missing
    );
    // Начертания нет и у Inter — Inter Regular.
    let odd = lay(
        &mut fonts,
        &text("Ferrum", |p| {
            p.style.font = FontName::new("Roboto", "Wide Hairline")
        }),
        0.0,
        0.0,
    );
    assert_eq!(odd.missing.len(), 1);
    assert_eq!(
        odd.width,
        lay(&mut fonts, &text("Ferrum", |_| {}), 0.0, 0.0).width
    );
}

#[test]
fn unloaded_font_is_requested_once_and_relaid_after_load() {
    let mut fonts = fonts();
    let t = text("Ferrum", |p| p.style.font = FontName::new("Roboto", "Bold"));
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert!(l.loading && l.missing.is_empty());
    let reqs = fonts.take_requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].family, "Roboto");
    assert_eq!(reqs[0].source, SourceKind::Bundled);
    assert_eq!(reqs[0].files.len(), 2);
    lay(&mut fonts, &t, 0.0, 0.0);
    assert!(fonts.take_requests().is_empty(), "повторного запроса нет");
    let gen = fonts.generation();
    for f in &reqs[0].files {
        fonts.load("Roboto", f, font_file(f)).unwrap();
    }
    assert!(fonts.generation() > gen);
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert!(!l.loading && l.missing.is_empty());
    assert_eq!(
        fonts.status(&FontName::new("Roboto", "Bold")),
        FontStatus::Ready
    );
    // Неразборчивый файл — ошибка и «отсутствует».
    let mut fresh = self::fonts();
    lay(&mut fresh, &t, 0.0, 0.0);
    let err = fresh.load("Roboto", "roboto/Roboto-Variable.ttf", b"garbage".to_vec());
    assert!(err.unwrap_err().contains("не читается"));
    assert_eq!(
        fresh.status(&FontName::new("Roboto", "Bold")),
        FontStatus::Missing
    );
}

#[test]
fn before_the_bundled_list_nothing_is_missing() {
    let mut fonts = Fonts::new();
    let t = text("Ferrum", |_| {});
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert!(l.loading);
    assert!(l.missing.is_empty());
    assert_eq!(
        fonts.status(&FontName::inter_regular()),
        FontStatus::Pending
    );
    // Вшитый Onest готов без загрузки.
    assert_eq!(
        fonts.status(&FontName::new("Onest", "Regular")),
        FontStatus::Ready
    );
}

#[test]
fn fixed_box_truncates_with_an_ellipsis() {
    let mut fonts = fonts();
    let t = text(
        "Съешь же ещё этих мягких французских булок да выпей чаю",
        |p| {
            p.sizing = TextSizing::Fixed;
            p.truncate = true;
        },
    );
    let full = lay(
        &mut fonts,
        &TextData {
            props: TextProps {
                truncate: false,
                ..t.props.clone()
            },
            ..t.clone()
        },
        160.0,
        70.0,
    );
    assert!(full.lines.len() > 2 && !full.truncated);
    let l = lay(&mut fonts, &t, 160.0, 70.0);
    assert!(l.truncated);
    assert_eq!(l.lines.len(), 2);
    assert!(l.height <= 70.0);
    let last = l.lines[1];
    assert!(last.rect.x1 <= 160.0 + 0.5, "{:?}", last.rect);
    // Последний глиф строки — многоточие.
    let run = l.runs.last().unwrap();
    assert_eq!(run.glyphs.len(), 1);
    assert!((run.glyphs[0].y as f64 - last.baseline).abs() < 0.01);
    // Не больше `max_lines` строк.
    let one = TextData {
        props: TextProps {
            sizing: TextSizing::AutoHeight,
            max_lines: Some(1),
            ..t.props.clone()
        },
        ..t.clone()
    };
    assert_eq!(lay(&mut fonts, &one, 160.0, 0.0).lines.len(), 1);
}

#[test]
fn vertical_alignment_inside_a_fixed_box() {
    let mut fonts = fonts();
    let t = text("Ferrum", |p| {
        p.sizing = TextSizing::Fixed;
        p.align_v = AlignV::Bottom;
    });
    let l = lay(&mut fonts, &t, 200.0, 100.0);
    assert!(
        (l.lines[0].rect.y1 - 100.0).abs() < 0.5,
        "{:?}",
        l.lines[0].rect
    );
}

#[test]
fn eviction_unloads_fonts_not_on_the_page() {
    let mut fonts = fonts();
    fonts.set_memory_limit_for_tests(2_000_000);
    fonts.set_in_use(["Inter".to_string()]);
    let t = text("Ferrum", |p| {
        p.style.font = FontName::new("Roboto", "Regular")
    });
    lay(&mut fonts, &t, 0.0, 0.0);
    for f in fonts.take_requests()[0].files.clone() {
        fonts.load("Roboto", &f, font_file(&f)).unwrap();
    }
    // Inter (1.8 МБ) на странице, Roboto (1 МБ) — нет: вытеснен Roboto.
    assert!(fonts.memory() <= 2_000_000, "{}", fonts.memory());
    assert_eq!(fonts.status(&FontName::inter_regular()), FontStatus::Ready);
    assert_eq!(
        fonts.status(&FontName::new("Roboto", "Regular")),
        FontStatus::Loading
    );
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert!(l.loading);
    assert_eq!(
        fonts.take_requests().len(),
        1,
        "вытесненный запрашивается снова"
    );
}
````

Run: `cargo test -p ferrum-text --test layout`
Expected: FAIL — не собирается: нет `Fonts`, `LayoutRequest`, `SourceKind`.

- [ ] **Step 2: Реестр шрифтов**

Создать `crates/ferrum-text/src/fonts.rs`:

````rust
//! Реестр шрифтов ядра (спека подсистемы 3, раздел 4): семейство → начертания →
//! источник → загруженные байты. Шрифты регистрируются в собственной коллекции
//! `fontique` без системного поиска (в wasm его нет), байты живут в памяти
//! (`Blob` — `Arc`) с пределом `MAX_FONT_MEMORY` и вытеснением давно не
//! использованных семейств, которых нет на текущей странице.
//!
//! Шрифт документа (`FontName`) разрешается в одно из трёх: готов (файл
//! загружен), грузится (файл запрошен — `take_requests`), отсутствует (семейства
//! или начертания нет ни во вшитых, ни в системных) — тогда раскладка берёт
//! Inter того же начертания, иначе Inter Regular, а имя в документе не меняется.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use ferrum_doc::FontName;
use ferrum_fontmeta::{read_faces, FaceInfo, FamilyInfo, MAX_FONT_FILE};
use kurbo::BezPath;
use parley::fontique::{
    Blob, Collection, CollectionOptions, FamilyId, FontStyle, FontWeight, FontWidth, SourceCache,
};
use parley::{FontContext, LayoutContext};
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::style_name::{guess_attrs, style_key};
use crate::ONEST;

/// Имя файла вшитого Onest в реестре.
const ONEST_FILE: &str = "Onest-wght.ttf";

/// Предел памяти загруженных шрифтов (спека 4).
pub const MAX_FONT_MEMORY: usize = 256 * 1024 * 1024;
/// Предел семейств в реестре (спека 4).
pub const MAX_FAMILIES: usize = 4096;
/// Предел имени семейства в байтах (спека 8).
pub const MAX_FAMILY_NAME: usize = 256;
/// Запасное семейство для отсутствующих шрифтов.
pub const FALLBACK_FAMILY: &str = "Inter";
/// Сколько контуров глифов держать (в единицах шрифта); сверх — кэш сбрасывается.
const OUTLINE_CACHE: usize = 65_536;

/// Откуда семейство. Порядок — приоритет: вшитое не подменяется системным с
/// тем же именем (текст выглядит одинаково у всех).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceKind {
    System,
    Bundled,
    Embedded,
}

/// Разрешение шрифта документа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStatus {
    /// Файл загружен — можно раскладывать.
    Ready,
    /// Файл запрошен, байтов ещё нет.
    Loading,
    /// Ни семейства, ни начертания нет — раскладывается запасным.
    Missing,
    /// Список вшитых ещё не пришёл: отсутствие не проверить.
    Pending,
}

/// Запрос байтов семейства: worker ядра качает вшитые файлы сам, системные
/// просит у основного потока (`FontRequest` протокола).
#[derive(Debug, Clone, PartialEq)]
pub struct FontRequest {
    pub family: String,
    pub source: SourceKind,
    pub files: Vec<String>,
}

/// Чем раскладывать шрифт документа.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub status: FontStatus,
    /// Имя семейства в коллекции `fontique` (из файла шрифта).
    pub family: String,
    pub weight: f32,
    pub italic: bool,
    pub stretch: f32,
    /// Шрифт подменён запасным (отсутствует).
    pub substituted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileState {
    Idle,
    Requested,
    Loaded,
    Failed,
}

struct FileSlot {
    state: FileState,
    bytes: usize,
    /// Что зарегистрировано в коллекции — чтобы снять при вытеснении.
    registered: Vec<(FamilyId, FontWidth, FontStyle, FontWeight)>,
    /// Имя семейства в коллекции (по таблице `name` файла).
    collection_name: Option<String>,
}

struct Family {
    name: String,
    source: SourceKind,
    faces: Vec<FaceInfo>,
    files: BTreeMap<String, FileSlot>,
    last_used: u64,
}

type OutlineKey = (u64, u32, u32, Vec<i16>);

/// Один реестр на ядро: им раскладывает редактор (размер текста в транзакции
/// правки) и рисует рендер. Ядро однопоточное — `Rc<RefCell>`.
pub type SharedFonts = Rc<RefCell<Fonts>>;

/// Реестр, контексты `parley` и кэш контуров глифов.
pub struct Fonts {
    pub(crate) fcx: FontContext,
    pub(crate) lcx: LayoutContext<u32>,
    families: HashMap<String, Family>,
    bundled_ready: bool,
    generation: u64,
    requests: Vec<FontRequest>,
    memory: usize,
    limit: usize,
    tick: u64,
    in_use: HashSet<String>,
    outlines: HashMap<OutlineKey, Arc<BezPath>>,
    upem: HashMap<(u64, u32), f64>,
    errors: Vec<String>,
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts::new()
    }
}

fn family_key(name: &str) -> String {
    name.to_lowercase()
}

impl Fonts {
    /// Реестр с вшитым Onest.
    pub fn new() -> Fonts {
        let mut fonts = Fonts {
            fcx: FontContext {
                collection: Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts: false,
                }),
                source_cache: SourceCache::default(),
            },
            lcx: LayoutContext::new(),
            families: HashMap::new(),
            bundled_ready: false,
            generation: 0,
            requests: Vec::new(),
            memory: 0,
            limit: MAX_FONT_MEMORY,
            tick: 0,
            in_use: HashSet::new(),
            outlines: HashMap::new(),
            upem: HashMap::new(),
            errors: Vec::new(),
        };
        let embedded = ferrum_fontmeta::group(read_faces(ONEST, ONEST_FILE));
        fonts.add_catalog(SourceKind::Embedded, embedded);
        // Байты уже в памяти wasm: регистрация без запроса.
        let _ = fonts.register("Onest", ONEST_FILE, Blob::new(Arc::new(ONEST)));
        fonts
    }

    /// Номер состояния реестра: растёт при каждом изменении, от которого зависит
    /// раскладка (пришёл список шрифтов, загружен или вытеснен файл). Кэш
    /// раскладок сверяет его.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Пришёл ли список вшитых шрифтов (до него отсутствие не определить).
    pub fn bundled_ready(&self) -> bool {
        self.bundled_ready
    }

    /// Список семейств источника: манифест вшитых или системные шрифты. Семейство,
    /// уже известное из источника не ниже, не заменяется; лишние сверх
    /// `MAX_FAMILIES` и с длинным именем — пропуск с сообщением в журнале ошибок.
    pub fn add_catalog(&mut self, source: SourceKind, list: Vec<FamilyInfo>) {
        if source == SourceKind::Bundled {
            self.bundled_ready = true;
        }
        for info in list {
            if info.family.is_empty()
                || info.family.len() > MAX_FAMILY_NAME
                || info.faces.is_empty()
            {
                self.errors.push(format!(
                    "Шрифт пропущен: неверное имя семейства «{}»",
                    clip(&info.family)
                ));
                continue;
            }
            let key = family_key(&info.family);
            if let Some(old) = self.families.get(&key) {
                if old.source >= source {
                    continue;
                }
            } else if self.families.len() >= MAX_FAMILIES {
                self.errors.push(format!(
                    "Шрифт «{}» пропущен: семейств больше {MAX_FAMILIES}",
                    clip(&info.family)
                ));
                continue;
            }
            let files = info
                .faces
                .iter()
                .map(|f| {
                    (
                        f.file.clone(),
                        FileSlot {
                            state: FileState::Idle,
                            bytes: 0,
                            registered: Vec::new(),
                            collection_name: None,
                        },
                    )
                })
                .collect();
            self.families.insert(
                key,
                Family {
                    name: info.family,
                    source,
                    faces: info.faces,
                    files,
                    last_used: 0,
                },
            );
        }
        self.generation += 1;
    }

    /// Семейства и их начертания по имени, с источником.
    pub fn families(&self) -> Vec<(String, SourceKind, Vec<String>)> {
        let mut out: Vec<_> = self
            .families
            .values()
            .map(|f| {
                (
                    f.name.clone(),
                    f.source,
                    f.faces.iter().map(|x| x.style.clone()).collect(),
                )
            })
            .collect();
        out.sort();
        out
    }

    /// Семейства, нужные текущей странице: их не вытесняют.
    pub fn set_in_use(&mut self, families: impl IntoIterator<Item = String>) {
        self.in_use = families.into_iter().map(|f| family_key(&f)).collect();
    }

    /// Запросы байтов с прошлого вызова.
    pub fn take_requests(&mut self) -> Vec<FontRequest> {
        std::mem::take(&mut self.requests)
    }

    /// Сообщения для журнала ошибок (неразборчивые файлы, пропуски) с прошлого вызова.
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    /// Состояние шрифта документа без запроса байтов.
    pub fn status(&self, font: &FontName) -> FontStatus {
        match self.face(font) {
            Some((fam, face)) => match fam.files.get(&face.file).map(|f| f.state) {
                Some(FileState::Loaded) => FontStatus::Ready,
                Some(FileState::Failed) => FontStatus::Missing,
                _ => FontStatus::Loading,
            },
            None if !self.bundled_ready => FontStatus::Pending,
            None => FontStatus::Missing,
        }
    }

    fn face(&self, font: &FontName) -> Option<(&Family, &FaceInfo)> {
        let fam = self.families.get(&family_key(&font.family))?;
        let key = style_key(&font.style);
        let face = fam.faces.iter().find(|f| style_key(&f.style) == key)?;
        Some((fam, face))
    }

    /// Чем раскладывать шрифт: сам шрифт, если он есть (незагруженный — запрос
    /// байтов, `Loading`), иначе запасной Inter. Отметка использования — для
    /// вытеснения.
    pub fn resolve(&mut self, font: &FontName) -> Resolved {
        match self.status(font) {
            FontStatus::Missing => {
                let mut r = self.resolve_fallback(&font.style);
                r.substituted = true;
                r
            }
            FontStatus::Pending => Resolved {
                status: FontStatus::Pending,
                family: font.family.clone(),
                weight: 400.0,
                italic: false,
                stretch: 1.0,
                substituted: false,
            },
            _ => self.resolve_known(font),
        }
    }

    fn resolve_fallback(&mut self, style: &str) -> Resolved {
        let same = FontName::new(FALLBACK_FAMILY, style);
        let font = if self.face(&same).is_some() {
            same
        } else {
            FontName::new(FALLBACK_FAMILY, "Regular")
        };
        if self.face(&font).is_some() {
            return self.resolve_known(&font);
        }
        // Inter нет в списке (сборка без вшитых): начертание угадывается по имени.
        let (weight, italic) = guess_attrs(style);
        Resolved {
            status: FontStatus::Missing,
            family: FALLBACK_FAMILY.to_string(),
            weight,
            italic,
            stretch: 1.0,
            substituted: true,
        }
    }

    fn resolve_known(&mut self, font: &FontName) -> Resolved {
        self.tick += 1;
        let tick = self.tick;
        let key = family_key(&font.family);
        let style = style_key(&font.style);
        let Some(fam) = self.families.get_mut(&key) else {
            unreachable!("семейство проверено в status")
        };
        fam.last_used = tick;
        let face = fam
            .faces
            .iter()
            .find(|f| style_key(&f.style) == style)
            .cloned()
            .expect("начертание проверено в status");
        let slot = fam
            .files
            .get_mut(&face.file)
            .expect("файл из списка начертаний");
        let status = match slot.state {
            FileState::Loaded => FontStatus::Ready,
            FileState::Requested => FontStatus::Loading,
            FileState::Failed => FontStatus::Missing,
            FileState::Idle => {
                // Запрос — всем незагруженным файлам семейства сразу: курсив и
                // прямое начертание обычно нужны вместе.
                let files: Vec<String> = fam
                    .files
                    .iter_mut()
                    .filter(|(_, s)| s.state == FileState::Idle)
                    .map(|(name, s)| {
                        s.state = FileState::Requested;
                        name.clone()
                    })
                    .collect();
                self.requests.push(FontRequest {
                    family: fam.name.clone(),
                    source: fam.source,
                    files,
                });
                FontStatus::Loading
            }
        };
        let slot = &fam.files[&face.file];
        Resolved {
            status,
            family: slot
                .collection_name
                .clone()
                .unwrap_or_else(|| fam.name.clone()),
            weight: face.weight,
            italic: face.italic,
            stretch: face.stretch,
            substituted: false,
        }
    }

    /// Байты файла семейства (ответ на запрос). Ошибка — текст для журнала:
    /// файл не из списка, больше `MAX_FONT_FILE`, неразборчивый.
    pub fn load(&mut self, family: &str, file: &str, bytes: Vec<u8>) -> Result<(), String> {
        if bytes.len() > MAX_FONT_FILE {
            self.fail(family, file);
            return Err(format!(
                "Шрифт «{}» не загружен: файл больше 64 МБ",
                clip(family)
            ));
        }
        let result = self.register(family, file, Blob::new(Arc::new(bytes)));
        if result.is_err() {
            self.fail(family, file);
        }
        self.evict();
        result
    }

    /// Файл не удалось получить: шрифт считается отсутствующим.
    pub fn load_failed(&mut self, family: &str, file: &str) {
        self.fail(family, file);
    }

    fn fail(&mut self, family: &str, file: &str) {
        if let Some(slot) = self
            .families
            .get_mut(&family_key(family))
            .and_then(|f| f.files.get_mut(file))
        {
            slot.state = FileState::Failed;
            self.generation += 1;
        }
    }

    fn register(&mut self, family: &str, file: &str, blob: Blob<u8>) -> Result<(), String> {
        let key = family_key(family);
        let Some(fam) = self.families.get_mut(&key) else {
            return Err(format!(
                "Шрифт «{}» не загружен: нет в списке шрифтов",
                clip(family)
            ));
        };
        let Some(slot) = fam.files.get_mut(file) else {
            return Err(format!(
                "Шрифт «{}» не загружен: файла «{}» нет в списке",
                clip(family),
                clip(file)
            ));
        };
        if slot.state == FileState::Loaded {
            return Ok(());
        }
        let len = blob.as_ref().len();
        let added = self.fcx.collection.register_fonts(blob, None);
        if added.is_empty() {
            return Err(format!(
                "Шрифт «{}» не загружен: файл «{}» не читается",
                clip(family),
                clip(file)
            ));
        }
        slot.collection_name = added
            .first()
            .and_then(|(id, _)| self.fcx.collection.family_name(*id))
            .map(str::to_string);
        slot.registered = added
            .iter()
            .flat_map(|(id, fonts)| {
                fonts
                    .iter()
                    .map(move |f| (*id, f.width(), f.style(), f.weight()))
            })
            .collect();
        slot.state = FileState::Loaded;
        slot.bytes = len;
        self.memory += len;
        self.generation += 1;
        Ok(())
    }

    /// Вытеснение (спека 4): пока память шрифтов больше предела — семейства, не
    /// используемые на странице, начиная с давно не нужных. Вшитый Onest не
    /// вытесняется (его байты всё равно в wasm).
    fn evict(&mut self) {
        while self.memory > self.limit {
            let victim = self
                .families
                .iter()
                .filter(|(k, f)| {
                    f.source != SourceKind::Embedded
                        && !self.in_use.contains(*k)
                        && f.files.values().any(|s| s.state == FileState::Loaded)
                })
                .min_by_key(|(_, f)| f.last_used)
                .map(|(k, _)| k.clone());
            let Some(key) = victim else { break };
            let fam = self.families.get_mut(&key).expect("найдено выше");
            for slot in fam.files.values_mut() {
                if slot.state != FileState::Loaded {
                    continue;
                }
                for (id, w, s, wt) in slot.registered.drain(..) {
                    self.fcx.collection.unregister_font(id, w, s, wt);
                }
                self.memory -= slot.bytes;
                slot.bytes = 0;
                slot.state = FileState::Idle;
            }
            self.outlines.clear();
            self.generation += 1;
        }
    }

    /// Другой предел памяти — чтобы тест вытеснения не грузил 256 МБ шрифтов.
    #[doc(hidden)]
    pub fn set_memory_limit_for_tests(&mut self, bytes: usize) {
        self.limit = bytes;
        self.evict();
    }

    /// Занято байтами загруженных шрифтов.
    pub fn memory(&self) -> usize {
        self.memory
    }

    /// Новый общий реестр.
    pub fn shared() -> SharedFonts {
        Rc::new(RefCell::new(Fonts::new()))
    }

    /// Единиц на em шрифта, из кэша.
    pub fn units_per_em(&mut self, font: &parley::FontData) -> f64 {
        *self
            .upem
            .entry((font.data.id(), font.index))
            .or_insert_with(|| font_units_per_em(font))
    }

    /// Контур глифа в единицах шрифта (ось Y вверх), из кэша.
    pub fn outline(&mut self, font: &parley::FontData, glyph: u32, coords: &[i16]) -> Arc<BezPath> {
        let key = (font.data.id(), font.index, glyph, coords.to_vec());
        if let Some(p) = self.outlines.get(&key) {
            return p.clone();
        }
        if self.outlines.len() >= OUTLINE_CACHE {
            self.outlines.clear();
        }
        let path = Arc::new(glyph_outline(font, glyph, coords));
        self.outlines.insert(key, path.clone());
        path
    }
}

/// Контур глифа; повреждённый — пустой (кадр не роняется).
pub fn glyph_outline(font: &parley::FontData, glyph: u32, coords: &[i16]) -> BezPath {
    let mut pen = Pen(BezPath::new());
    let Ok(font_ref) = FontRef::from_index(font.data.as_ref(), font.index) else {
        return pen.0;
    };
    let location: Vec<NormalizedCoord> = coords
        .iter()
        .map(|c| NormalizedCoord::from_bits(*c))
        .collect();
    if let Some(outline) = font_ref.outline_glyphs().get(GlyphId::new(glyph)) {
        let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::new(&location));
        if outline.draw(settings, &mut pen).is_err() {
            pen.0 = BezPath::new();
        }
    }
    pen.0
}

/// Единиц на em шрифта.
pub fn font_units_per_em(font: &parley::FontData) -> f64 {
    FontRef::from_index(font.data.as_ref(), font.index)
        .map(|f| {
            f.metrics(Size::unscaled(), LocationRef::default())
                .units_per_em
                .max(1) as f64
        })
        .unwrap_or(1000.0)
}

fn clip(s: &str) -> String {
    let mut it = s.chars();
    let head: String = it.by_ref().take(64).collect();
    if it.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.quad_to((cx as f64, cy as f64), (x as f64, y as f64));
    }
    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.0.curve_to(
            (c1x as f64, c1y as f64),
            (c2x as f64, c2y as f64),
            (x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}
````

- [ ] **Step 3: Раскладка**

Создать `crates/ferrum-text/src/layout.rs`:

````rust
//! Раскладка текстового узла (спека подсистемы 3, раздел 5): содержимое,
//! свойства и ширина рамки → строки, глифы с позициями и украшения в
//! координатах узла (0, 0 — левый верхний угол рамки).
//!
//! Каждый абзац раскладывается отдельно: у абзаца свой отступ списка, маркер
//! («•», «1.»), отступ первой строки и интервал после. Стили участков —
//! диапазоны `parley`; кисть — номер цвета в палитре раскладки. Регистр
//! (`upper`, `lower`, `title`) меняет текст до формирования, капитель —
//! функция OpenType `smcp`.

use std::ops::Range;

use ferrum_doc::{
    AlignH, AlignV, Color, Decoration, Fill, FillKind, FontName, LetterSpacing, LineHeight,
    ListKind, TextAttrs, TextCase, TextData, TextSizing, MAX_LIST_INDENT,
};
use kurbo::Rect;
use parley::{
    Alignment, AlignmentOptions, FontFamily, FontStyle, FontWeight, FontWidth, IndentOptions,
    Layout, OverflowWrap, PositionedLayoutItem, StyleProperty,
};

use crate::fonts::{FontStatus, Fonts, Resolved};

/// Ширина ступени отступа списка в кеглях первого символа абзаца.
pub const LIST_INDENT_EM: f64 = 1.5;

/// Что раскладывать.
#[derive(Debug, Clone, Copy)]
pub struct LayoutRequest<'a> {
    pub text: &'a TextData,
    /// Заливки узла: цвет текста по умолчанию — верхняя видимая сплошная.
    pub fills: &'a [Fill],
    /// Ширина рамки: строки переносятся по ней (`AutoHeight`, `Fixed`); у
    /// `AutoWidth` — `None`, строки рвутся только переводами строк.
    pub width: Option<f64>,
    /// Высота рамки (`Fixed`): обрезка и выравнивание по вертикали; `None` —
    /// высота по тексту.
    pub height: Option<f64>,
}

impl<'a> LayoutRequest<'a> {
    /// Запрос по свойствам узла: размеры рамки берутся по режиму размера.
    pub fn for_node(text: &'a TextData, fills: &'a [Fill], w: f64, h: f64) -> LayoutRequest<'a> {
        let sizing = text.props.sizing;
        LayoutRequest {
            text,
            fills,
            width: (sizing != TextSizing::AutoWidth).then_some(w),
            height: (sizing == TextSizing::Fixed).then_some(h),
        }
    }
}

/// Глиф: номер в шрифте и позиция левого края на базовой линии.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

/// Начертание прогона в коллекции шрифтов (для многоточия тем же шрифтом).
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub family: String,
    pub weight: f32,
    pub italic: bool,
    pub stretch: f32,
}

/// Подряд идущие глифы одного шрифта, кегля и цвета.
#[derive(Debug, Clone)]
pub struct GlyphRun {
    pub font: parley::FontData,
    pub face: Face,
    pub size: f32,
    /// Нормализованные координаты вариативного шрифта (биты `F2Dot14`).
    pub coords: Vec<i16>,
    pub color: Color,
    /// Синтетический наклон (курсив без своего файла), градусы.
    pub skew: Option<f32>,
    pub glyphs: Vec<PlacedGlyph>,
}

/// Строка: прямоугольник (для LOD «полосой») и базовая линия.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineBox {
    pub rect: Rect,
    pub baseline: f64,
    /// Цвет полосы LOD — цвет первого участка строки.
    pub color: Color,
}

/// Подчёркивание или зачёркивание — прямоугольник цвета текста.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecorationBox {
    pub rect: Rect,
    pub color: Color,
}

/// Результат раскладки.
#[derive(Debug, Clone, Default)]
pub struct TextLayout {
    /// Размер содержимого: ширина самой длинной строки и высота всех строк с
    /// интервалами абзацев (у обрезанного — видимых).
    pub width: f64,
    pub height: f64,
    pub lines: Vec<LineBox>,
    pub runs: Vec<GlyphRun>,
    pub decorations: Vec<DecorationBox>,
    /// Шрифты, которых нет: разложены запасным (имена — как в документе).
    pub missing: Vec<FontName>,
    /// Какой-то шрифт ещё грузится или список вшитых не пришёл: раскладка не
    /// окончательная, рисовать её нельзя.
    pub loading: bool,
    /// Текст обрезан (обрезка с многоточием или по рамке).
    pub truncated: bool,
}

/// Стиль участка после разрешения шрифта.
#[derive(Debug, Clone)]
struct SpanStyle {
    resolved: Resolved,
    size: f64,
    line_height: LineHeight,
    letter_spacing: LetterSpacing,
    decoration: Decoration,
    case: TextCase,
    color: Color,
}

struct Paragraph<'a> {
    runs: Vec<(&'a str, &'a TextAttrs)>,
    /// Разметка перевода строки абзаца (список и его уровень).
    attrs: &'a TextAttrs,
}

/// Цвет текста по умолчанию: верхняя видимая сплошная заливка узла с её
/// прозрачностью; без такой — чёрный (градиенты и картинки — этап 2).
pub fn text_color(fills: &[Fill]) -> Color {
    fills
        .iter()
        .rev()
        .find(|f| f.visible && f.kind == FillKind::Solid)
        .map(|f| {
            let mut c = f.color;
            c.a = (c.a as f64 * f.opacity.clamp(0.0, 1.0)).round() as u8;
            c
        })
        .unwrap_or(Color::BLACK)
}

fn split_paragraphs(data: &TextData) -> Vec<Paragraph<'_>> {
    static PLAIN: std::sync::OnceLock<TextAttrs> = std::sync::OnceLock::new();
    let plain = PLAIN.get_or_init(TextAttrs::default);
    let mut out = Vec::new();
    let mut cur: Vec<(&str, &TextAttrs)> = Vec::new();
    for run in &data.content.runs {
        let mut rest = run.text.as_str();
        while let Some(i) = rest.find('\n') {
            if i > 0 {
                cur.push((&rest[..i], &run.attrs));
            }
            out.push(Paragraph {
                runs: std::mem::take(&mut cur),
                attrs: &run.attrs,
            });
            rest = &rest[i + 1..];
        }
        if !rest.is_empty() {
            cur.push((rest, &run.attrs));
        }
    }
    // Текст без перевода строки в конце (не должно быть) — последний абзац.
    if !cur.is_empty() {
        out.push(Paragraph {
            runs: cur,
            attrs: plain,
        });
    }
    out
}

fn apply_case(s: &str, case: TextCase, word_start: &mut bool) -> String {
    match case {
        TextCase::Upper => s.to_uppercase(),
        TextCase::Lower => s.to_lowercase(),
        TextCase::Title => s
            .chars()
            .flat_map(|c| {
                let out: Vec<char> = if *word_start && c.is_alphabetic() {
                    c.to_uppercase().collect()
                } else {
                    vec![c]
                };
                *word_start = !c.is_alphanumeric();
                out
            })
            .collect(),
        TextCase::None | TextCase::SmallCaps => s.to_string(),
    }
}

fn line_height(v: LineHeight) -> parley::LineHeight {
    match v {
        LineHeight::Auto => parley::LineHeight::MetricsRelative(1.0),
        LineHeight::Px(px) => parley::LineHeight::Absolute(px.max(0.0) as f32),
        LineHeight::Percent(p) => parley::LineHeight::FontSizeRelative((p / 100.0).max(0.0) as f32),
    }
}

fn letter_spacing(v: LetterSpacing, size: f64) -> f32 {
    match v {
        LetterSpacing::Px(px) => px as f32,
        LetterSpacing::Percent(p) => (size * p / 100.0) as f32,
    }
}

impl Fonts {
    /// Раскладывает текст узла.
    pub fn layout(&mut self, req: &LayoutRequest) -> TextLayout {
        let props = &req.text.props;
        let default_color = text_color(req.fills);
        let mut out = TextLayout::default();
        let mut y = 0.0f64;
        let mut counters = [0u32; MAX_LIST_INDENT as usize + 1];
        let paragraphs = split_paragraphs(req.text);
        for (pi, para) in paragraphs.iter().enumerate() {
            if pi > 0 {
                y += props.paragraph_spacing.max(0.0);
            }
            let styles: Vec<SpanStyle> = if para.runs.is_empty() {
                vec![self.span_style(para.attrs, req, default_color, &mut out)]
            } else {
                para.runs
                    .iter()
                    .map(|(_, a)| self.span_style(a, req, default_color, &mut out))
                    .collect()
            };
            let list = para.attrs.list.unwrap_or(ListKind::None);
            let level = para.attrs.indent.unwrap_or(0).min(MAX_LIST_INDENT) as usize;
            let marker = match list {
                ListKind::None => {
                    counters = [0; MAX_LIST_INDENT as usize + 1];
                    None
                }
                ListKind::Bullet => {
                    counters[level..].iter_mut().for_each(|c| *c = 0);
                    Some("•".to_string())
                }
                ListKind::Ordered => {
                    counters[level + 1..].iter_mut().for_each(|c| *c = 0);
                    counters[level] += 1;
                    Some(format!("{}.", counters[level]))
                }
            };
            let first = &styles[0];
            let indent_x = if marker.is_some() {
                (level as f64 + 1.0) * LIST_INDENT_EM * first.size
            } else {
                0.0
            };
            // Текст абзаца с учётом регистра и диапазоны участков в байтах.
            let mut text = String::new();
            let mut ranges: Vec<Range<usize>> = Vec::new();
            let mut word_start = true;
            for ((s, _), style) in para.runs.iter().zip(&styles) {
                let start = text.len();
                text.push_str(&apply_case(s, style.case, &mut word_start));
                ranges.push(start..text.len());
            }
            // Пустой абзац — строка высотой своего стиля: невидимый пробел нулевой ширины.
            if text.is_empty() {
                text.push('\u{200B}');
                ranges.clear();
                ranges.push(0..text.len());
            }
            // Кисть участка — его номер: по ней прогон узнаёт цвет и начертание.
            let mut builder = self.lcx.ranged_builder(&mut self.fcx, &text, 1.0, false);
            for (i, (range, style)) in ranges.iter().zip(&styles).enumerate() {
                push_style(&mut builder, style, i as u32, range.clone());
            }
            let mut layout: Layout<u32> = builder.build(&text);
            if props.paragraph_indent != 0.0 {
                layout.set_text_indent(props.paragraph_indent as f32, IndentOptions::default());
            }
            let avail = req.width.map(|w| (w - indent_x).max(0.0) as f32);
            layout.break_all_lines(avail);
            layout.align(
                match props.align_h {
                    AlignH::Left => Alignment::Left,
                    AlignH::Center => Alignment::Center,
                    AlignH::Right => Alignment::Right,
                    AlignH::Justify => Alignment::Justify,
                },
                AlignmentOptions::default(),
            );
            let first_line = out.lines.len();
            collect_lines(&layout, &styles, indent_x, y, &mut out);
            if let (Some(m), Some(line)) = (marker, out.lines.get(first_line).copied()) {
                let x = level as f64 * LIST_INDENT_EM * first.size;
                self.place_marker(&m, first, x, line.baseline, &mut out);
            }
            y += layout.height() as f64;
        }
        out.height = y;
        out.width = out.lines.iter().map(|l| l.rect.x1).fold(0.0, f64::max);
        self.truncate(req, &mut out);
        // Выравнивание по вертикали внутри рамки фиксированной высоты.
        if let Some(h) = req.height {
            let dy = match props.align_v {
                AlignV::Top => 0.0,
                AlignV::Middle => (h - out.height) / 2.0,
                AlignV::Bottom => h - out.height,
            };
            if dy != 0.0 {
                shift(&mut out, dy);
            }
        }
        out.missing.sort();
        out.missing.dedup();
        out
    }

    fn span_style(
        &mut self,
        a: &TextAttrs,
        req: &LayoutRequest,
        default_color: Color,
        out: &mut TextLayout,
    ) -> SpanStyle {
        let d = &req.text.props.style;
        let font = a.font_name(d);
        let resolved = self.resolve(&font);
        match resolved.status {
            FontStatus::Loading | FontStatus::Pending => out.loading = true,
            // Запасной Inter тоже может ещё грузиться.
            FontStatus::Missing if !resolved.substituted => out.loading = true,
            _ => {}
        }
        if resolved.substituted {
            out.missing.push(font);
        }
        SpanStyle {
            resolved,
            size: a.size.unwrap_or(d.size).clamp(0.1, 10_000.0),
            line_height: a.line_height.unwrap_or(d.line_height),
            letter_spacing: a.letter_spacing.unwrap_or(d.letter_spacing),
            decoration: a.decoration.unwrap_or(d.decoration),
            case: a.case.unwrap_or(d.case),
            color: match &a.fill {
                Some(f) if f.kind == FillKind::Solid => text_color(std::slice::from_ref(f)),
                _ => default_color,
            },
        }
    }

    /// Маркер списка: отдельная однострочная раскладка стилем первого участка.
    fn place_marker(
        &mut self,
        marker: &str,
        style: &SpanStyle,
        x: f64,
        baseline: f64,
        out: &mut TextLayout,
    ) {
        let mut builder = self.lcx.ranged_builder(&mut self.fcx, marker, 1.0, false);
        let plain = SpanStyle {
            decoration: Decoration::None,
            case: TextCase::None,
            ..style.clone()
        };
        push_style(&mut builder, &plain, 0, 0..marker.len());
        let mut layout: Layout<u32> = builder.build(marker);
        layout.break_all_lines(None);
        let mut tmp = TextLayout::default();
        collect_lines(&layout, std::slice::from_ref(&plain), 0.0, 0.0, &mut tmp);
        let Some(line) = tmp.lines.first() else {
            return;
        };
        let (dx, dy) = (x as f32, (baseline - line.baseline) as f32);
        for mut run in tmp.runs {
            for g in &mut run.glyphs {
                g.x += dx;
                g.y += dy;
            }
            out.runs.push(run);
        }
    }

    /// Обрезка (спека 3, `truncate`): у фиксированной рамки — строки, которые
    /// помещаются по высоте, и не больше `max_lines`; у последней видимой при
    /// продолжении — многоточие. Без `truncate` строки за рамкой остаются
    /// (их обрежет рамка при рисовании — как в Figma, текст виден за краем).
    fn truncate(&mut self, req: &LayoutRequest, out: &mut TextLayout) {
        let props = &req.text.props;
        if !props.truncate || out.lines.is_empty() {
            return;
        }
        let mut keep = out.lines.len();
        if let Some(h) = req.height {
            keep = out
                .lines
                .iter()
                .take_while(|l| l.rect.y1 <= h + 0.01)
                .count()
                .max(1);
        }
        if let Some(n) = props.max_lines {
            keep = keep.min(n as usize).max(1);
        }
        if keep >= out.lines.len() {
            return;
        }
        out.truncated = true;
        let last = out.lines[keep - 1];
        let cut_y = last.rect.y1;
        out.lines.truncate(keep);
        for run in &mut out.runs {
            run.glyphs.retain(|g| (g.y as f64) <= cut_y);
        }
        out.decorations.retain(|d| d.rect.y0 < cut_y);
        // Многоточие стилем последнего глифа строки — справа, в пределах рамки.
        let avail = req.width.unwrap_or(last.rect.x1);
        let on_line = |g: &PlacedGlyph| (g.y as f64 - last.baseline).abs() < 0.5;
        let Some(style_run) = out
            .runs
            .iter()
            .rev()
            .find(|r| r.glyphs.iter().any(on_line))
            .cloned()
        else {
            return;
        };
        let ellipsis = self.shape_ellipsis(&style_run);
        let Some(e_width) = ellipsis.as_ref().map(|(_, w)| *w) else {
            return;
        };
        // Глифы строки, не помещающиеся вместе с многоточием, снимаются.
        let limit = (avail - e_width) as f32;
        let mut end_x = last.rect.x0 as f32;
        for run in &mut out.runs {
            let advances: Vec<f32> = advances_of(run);
            let mut kept = Vec::with_capacity(run.glyphs.len());
            for (g, adv) in run.glyphs.iter().zip(advances) {
                if on_line(g) {
                    if g.x + adv > limit {
                        continue;
                    }
                    end_x = end_x.max(g.x + adv);
                }
                kept.push(*g);
            }
            run.glyphs = kept;
        }
        out.decorations.retain(|d| {
            (d.rect.y0 - last.baseline).abs() > last.rect.height() || d.rect.x0 < limit as f64
        });
        for d in &mut out.decorations {
            if d.rect.y0 >= last.rect.y0 && d.rect.y1 <= last.rect.y1 + 1.0 {
                d.rect.x1 = d.rect.x1.min(end_x as f64);
            }
        }
        if let Some((mut run, w)) = ellipsis {
            for g in &mut run.glyphs {
                g.x += end_x;
                g.y = last.baseline as f32;
            }
            out.runs.push(run);
            out.lines[keep - 1].rect.x1 = end_x as f64 + w;
        }
        out.height = cut_y;
        out.width = out.lines.iter().map(|l| l.rect.x1).fold(0.0, f64::max);
    }

    /// «…» тем же шрифтом и кеглем, что глифы `like`: глифы от нуля и ширина.
    fn shape_ellipsis(&mut self, like: &GlyphRun) -> Option<(GlyphRun, f64)> {
        let style = SpanStyle {
            resolved: Resolved {
                status: FontStatus::Ready,
                family: like.face.family.clone(),
                weight: like.face.weight,
                italic: like.face.italic,
                stretch: like.face.stretch,
                substituted: false,
            },
            size: like.size as f64,
            line_height: LineHeight::Auto,
            letter_spacing: LetterSpacing::Px(0.0),
            decoration: Decoration::None,
            case: TextCase::None,
            color: like.color,
        };
        let text = "…";
        let mut builder = self.lcx.ranged_builder(&mut self.fcx, text, 1.0, false);
        push_style(&mut builder, &style, 0, 0..text.len());
        let mut layout: Layout<u32> = builder.build(text);
        layout.break_all_lines(None);
        let mut tmp = TextLayout::default();
        collect_lines(&layout, std::slice::from_ref(&style), 0.0, 0.0, &mut tmp);
        let width = layout.width() as f64;
        let mut run = tmp.runs.into_iter().next()?;
        run.color = like.color;
        for g in &mut run.glyphs {
            g.y = 0.0;
        }
        Some((run, width))
    }
}

/// Ширины глифов прогона: расстояние до следующего (у последнего — оценка по кеглю).
fn advances_of(run: &GlyphRun) -> Vec<f32> {
    let n = run.glyphs.len();
    (0..n)
        .map(|i| match run.glyphs.get(i + 1) {
            Some(next) if (next.y - run.glyphs[i].y).abs() < 0.5 => {
                (next.x - run.glyphs[i].x).max(0.0)
            }
            _ => run.size * 0.6,
        })
        .collect()
}

/// Свойства `parley` стиля участка.
fn properties(s: &SpanStyle, brush: u32) -> Vec<StyleProperty<'_, u32>> {
    let mut out = vec![
        StyleProperty::FontFamily(FontFamily::named(&s.resolved.family)),
        StyleProperty::FontWeight(FontWeight::new(s.resolved.weight)),
        StyleProperty::FontStyle(if s.resolved.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        }),
        StyleProperty::FontWidth(FontWidth::from_ratio(s.resolved.stretch)),
        StyleProperty::FontSize(s.size as f32),
        StyleProperty::LineHeight(line_height(s.line_height)),
        StyleProperty::LetterSpacing(letter_spacing(s.letter_spacing, s.size)),
        StyleProperty::Brush(brush),
        // Слово длиннее рамки переносится по буквам, а не вылезает за неё (как в Figma).
        StyleProperty::OverflowWrap(OverflowWrap::BreakWord),
        StyleProperty::Underline(s.decoration == Decoration::Underline),
        StyleProperty::Strikethrough(s.decoration == Decoration::Strike),
    ];
    if s.case == TextCase::SmallCaps {
        out.push(StyleProperty::FontFeatures("\"smcp\" on".into()));
    }
    out
}

/// Стиль участка на диапазон. Корневой стиль абзаца — стиль первого участка:
/// иначе строку подпирал бы корневой стиль `parley` по умолчанию (16 px) и
/// мелкий текст получал бы лишний интерлиньяж.
fn push_style(
    builder: &mut parley::RangedBuilder<'_, u32>,
    s: &SpanStyle,
    brush: u32,
    range: Range<usize>,
) {
    if range.start == 0 {
        for prop in properties(s, brush) {
            builder.push_default(prop);
        }
    }
    for prop in properties(s, brush) {
        builder.push(prop, range.clone());
    }
}

/// Строки и глифы раскладки абзаца со сдвигом `(dx, dy)`.
fn collect_lines(
    layout: &Layout<u32>,
    styles: &[SpanStyle],
    dx: f64,
    dy: f64,
    out: &mut TextLayout,
) {
    for line in layout.lines() {
        let m = line.metrics();
        let mut line_color = None;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let style = glyph_run.style();
            let Some(span) = styles.get(style.brush as usize) else {
                continue;
            };
            let color = span.color;
            line_color.get_or_insert(color);
            let glyphs: Vec<PlacedGlyph> = glyph_run
                .positioned_glyphs()
                .map(|g| PlacedGlyph {
                    id: g.id,
                    x: g.x + dx as f32,
                    y: g.y + dy as f32,
                })
                .collect();
            let fm = run.font_metrics();
            let x0 = glyph_run.offset() as f64 + dx;
            let x1 = x0 + glyph_run.advance() as f64;
            let base = glyph_run.baseline() as f64 + dy;
            if style.underline.is_some() {
                let size = fm.underline_size.max(0.5) as f64;
                let top = base - fm.underline_offset as f64;
                out.decorations.push(DecorationBox {
                    rect: Rect::new(x0, top, x1, top + size),
                    color,
                });
            }
            if style.strikethrough.is_some() {
                let size = fm.strikethrough_size.max(0.5) as f64;
                let top = base - fm.strikethrough_offset as f64;
                out.decorations.push(DecorationBox {
                    rect: Rect::new(x0, top, x1, top + size),
                    color,
                });
            }
            if glyphs.is_empty() {
                continue;
            }
            let synthesis = run.synthesis();
            out.runs.push(GlyphRun {
                font: run.font().clone(),
                face: Face {
                    family: span.resolved.family.clone(),
                    weight: span.resolved.weight,
                    italic: span.resolved.italic,
                    stretch: span.resolved.stretch,
                },
                size: run.font_size(),
                coords: run
                    .normalized_coords()
                    .iter()
                    .map(|c| c.to_bits())
                    .collect(),
                color,
                skew: synthesis.skew(),
                glyphs,
            });
        }
        let x0 = m.offset as f64 + dx;
        out.lines.push(LineBox {
            rect: Rect::new(
                x0,
                m.block_min_coord as f64 + dy,
                x0 + m.advance as f64,
                m.block_max_coord as f64 + dy,
            ),
            baseline: m.baseline as f64 + dy,
            color: line_color.unwrap_or(Color::BLACK),
        });
    }
}

fn shift(out: &mut TextLayout, dy: f64) {
    for l in &mut out.lines {
        l.rect = l.rect + kurbo::Vec2::new(0.0, dy);
        l.baseline += dy;
    }
    for r in &mut out.runs {
        for g in &mut r.glyphs {
            g.y += dy as f32;
        }
    }
    for d in &mut out.decorations {
        d.rect = d.rect + kurbo::Vec2::new(0.0, dy);
    }
}
````

В `crates/ferrum-text/src/lib.rs` заменить:

````rust
//! глифов из `skrifa`. О Loro и рендере не знает: получает `TextData` узла,
//! отдаёт строки и глифы.

pub mod style_name;

pub use ferrum_fontmeta::{group, read_faces, FaceInfo, FamilyInfo, MAX_FONT_FILE};

/// Onest (SIL OFL 1.1, `assets/fonts/OFL.txt`) вшит в wasm: им набраны подписи
/// оверлеев (`ferrum-render`), и в реестре он — вшитое семейство, готовое без
````

на:

````rust
//! глифов из `skrifa`. О Loro и рендере не знает: получает `TextData` узла,
//! отдаёт строки и глифы.

mod fonts;
mod layout;
pub mod style_name;

pub use ferrum_fontmeta::{group, read_faces, FaceInfo, FamilyInfo, MAX_FONT_FILE};
pub use fonts::{
    font_units_per_em, glyph_outline, FontRequest, FontStatus, Fonts, Resolved, SharedFonts,
    SourceKind, FALLBACK_FAMILY, MAX_FAMILIES, MAX_FAMILY_NAME, MAX_FONT_MEMORY,
};
pub use layout::{
    text_color, DecorationBox, Face, GlyphRun, LayoutRequest, LineBox, PlacedGlyph, TextLayout,
    LIST_INDENT_EM,
};

/// Onest (SIL OFL 1.1, `assets/fonts/OFL.txt`) вшит в wasm: им набраны подписи
/// оверлеев (`ferrum-render`), и в реестре он — вшитое семейство, готовое без
````

- [ ] **Step 4: Проверить, в том числе сборку под wasm32**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ferrum-text && cargo check -p ferrum-text --target wasm32-unknown-unknown`
Expected: PASS: `layout` — 11 тестов (вес вариативного Inter — координатами `wght`, 24 px «Авто» ≈ 1,21 кегля, перенос по рамке, цвета и подчёркивание участков, списки с маркерами, запасной Inter для отсутствующего, запрос и загрузка, до списка вшитых — ничего не отсутствует, обрезка с многоточием, выравнивание по вертикали, вытеснение); сборка под wasm32 — без ошибок (`parley` без системных шрифтов).

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-text
git commit -m "feat(text): реестр шрифтов (каталоги, запросы, загрузка, запасной Inter, вытеснение 256 МБ) и раскладка абзацами через parley: списки, украшения, обрезка с многоточием

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Рендер текста: глифы, LOD, ленивая раскладка, экспорт и превью

Спека 6 и 5 («ленивая раскладка»). Рендерер получает общий реестр (`Renderer::set_fonts`; свой без него знает только Onest), рисовальщик держит раскладки текстов по слотам (годна, пока не сменились `version` узла и поколение реестра; давно не рисовавшиеся вытесняются вместе с контурами). Раскладка — при первом рисовании узла, в пределах 4 мс кадра (первая — всегда); что не уложилось — полосы по оценке строк и `needs_another_frame`. Экспорт и превью (`lod: false` у экспорта) раскладывают всё сразу. Раскладка шрифтом, который ещё грузится, не рисуется; когда байты придут, поколение реестра сменится и кадр перерисуется. Глифы — заливка контуров `skrifa` в единицах шрифта с матрицей `translate · skew · scale(k, −k)`, украшения — прямоугольники; строка ниже 3 px устройства — полосой. Растровый кэш фреймов работает как прежде: внутри растра — те же полосы и глифы.

**Files:**
- Create: `crates/ferrum-render/src/text_node.rs`
- Modify: `crates/ferrum-render/src/draw.rs` (`FrameStats` 51–75, `DrawContext` 77–94, `Painter` 141–222, `draw_own` 379–450, растр 545–554), `display.rs` (`lod_color` 76–94), `lib.rs` (`Renderer` 62–125, контексты 141, 278, 347)
- Test: `crates/ferrum-render/tests/text.rs` (новый), эталон `crates/ferrum-render/tests/golden/text.png`

**Interfaces:**
- Consumes: `ferrum_text::{Fonts, SharedFonts, LayoutRequest, TextLayout}` (Task 4), `NodeKind::Text`, `Props::text` (Task 1).
- Produces: `Renderer::set_fonts(SharedFonts)`, `Renderer::fonts() -> &SharedFonts`; `ferrum_render::{Fonts, SharedFonts}` (реэкспорт), `TEXT_LOD_PX = 3.0`, `TEXT_GREEK_ALPHA = 0.25`, `TEXT_LAYOUT_TIME_PER_FRAME = 4 мс`; `FrameStats::{text_layouts, text_glyphs, text_greeked}`; `DrawContext::fonts: &RefCell<Fonts>`.

- [ ] **Step 1: Падающие тесты рендера текста**

Создать `crates/ferrum-render/tests/text.rs`:

````rust
//! Текст (подсистема 3, этап 1): глифы контурами `skrifa`, украшения, полосы
//! LOD мелкого текста, ленивая раскладка с пределом времени кадра, шрифт,
//! который ещё грузится, экспорт PNG и превью с текстом.

mod common;

use std::path::PathBuf;

use common::*;
use ferrum_doc::*;
use ferrum_render::{Fonts, NoImages, Renderer, SharedFonts};
use ferrum_text::{group, read_faces, SourceKind};

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const INTER: [&str; 2] = [
    "inter/Inter-Variable.ttf",
    "inter/Inter-Italic-Variable.ttf",
];

/// Реестр со вшитым Inter; `load` — загрузить ли его байты.
fn fonts(load: bool) -> SharedFonts {
    let shared = Fonts::shared();
    {
        let mut f = shared.borrow_mut();
        let faces = INTER
            .iter()
            .flat_map(|r| read_faces(&font_file(r), r))
            .collect::<Vec<_>>();
        f.add_catalog(SourceKind::Bundled, group(faces));
        if load {
            for r in INTER {
                f.load("Inter", r, font_file(r)).unwrap();
            }
        }
    }
    shared
}

fn renderer(fonts: &SharedFonts) -> Renderer {
    let mut r = Renderer::new();
    r.set_fonts(fonts.clone());
    r
}

/// Текстовый узел с содержимым и свойствами.
fn add_text(
    p: &mut Page,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    content: TextContent,
    props: TextProps,
) -> NodeId {
    p.doc
        .transact(p.page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_props(
                id,
                &PropsPatch {
                    text: Some(props),
                    ..rect(x, y, w, h)
                },
            )?;
            tx.set_text_content(id, &content)?;
            Ok(vec![id])
        })
        .unwrap()[0]
}

fn props(size: f64, sizing: TextSizing) -> TextProps {
    TextProps {
        sizing,
        style: TextStyle {
            size,
            ..TextStyle::default()
        },
        ..TextProps::default()
    }
}

fn dark_pixels(img: &ferrum_render::Pixmap, x0: u16, y0: u16, x1: u16, y1: u16) -> usize {
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if px(img, x, y)[0] < 128 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn text_golden() {
    let fonts = fonts(true);
    let mut p = Page::new();
    let red = Fill::solid(Color::rgba(220, 30, 30, 255));
    let content = TextContent {
        runs: vec![
            TextRun {
                text: "Ferrum ".into(),
                attrs: TextAttrs {
                    style: Some("Bold".into()),
                    ..Default::default()
                },
            },
            TextRun {
                text: "текст".into(),
                attrs: TextAttrs {
                    fill: Some(red),
                    decoration: Some(Decoration::Underline),
                    ..Default::default()
                },
            },
            TextRun {
                text: "\nвторая строка\n".into(),
                attrs: TextAttrs::default(),
            },
        ],
    };
    add_text(
        &mut p,
        10.0,
        10.0,
        200.0,
        70.0,
        content,
        props(24.0, TextSizing::AutoHeight),
    );
    let img = ferrum_render_with(&fonts, &p.scene(), 220.0, 90.0);
    // Чёрный «F» в начале, красное «текст», белое поле справа от строк.
    assert!(dark_pixels(&img, 10, 14, 40, 38) > 40);
    let reds = (100..200)
        .flat_map(|x| (14..44).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let c = px(&img, x, y);
            c[0] > 180 && c[1] < 90
        })
        .count();
    assert!(reds > 40, "{reds}");
    assert_px(&img, 215, 20, [255, 255, 255], 0);
    golden("text", &img);
}

fn ferrum_render_with(
    fonts: &SharedFonts,
    scene: &ferrum_scene::Scene,
    w: f64,
    h: f64,
) -> ferrum_render::Pixmap {
    render_with(&mut renderer(fonts), scene, &camera(w, h), &NoImages)
}

#[test]
fn small_text_is_drawn_as_bars() {
    let fonts = fonts(true);
    let mut p = Page::new();
    let id = add_text(
        &mut p,
        0.0,
        0.0,
        400.0,
        200.0,
        TextContent::plain("Строка один\nСтрока два"),
        props(40.0, TextSizing::AutoWidth),
    );
    let scene = p.scene();
    let mut r = renderer(&fonts);
    let mut cam = camera(200.0, 100.0);
    // Зум 5 %: строка 48 px → 2.4 px на экране — полосой.
    cam.set_zoom(0.05);
    cam.set_center(kurbo::Point::new(100.0, 50.0));
    let (w, h) = cam.device_size();
    let mut backend = ferrum_render::CpuBackend::new(w, h);
    let stats = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
    assert_eq!((stats.text_greeked, stats.text_glyphs), (1, 0), "{stats:?}");
    let _ = id;
}

#[test]
fn layout_is_lazy_cached_and_time_limited() {
    let fonts = fonts(true);
    let mut p = Page::new();
    for i in 0..600 {
        add_text(
            &mut p,
            (i % 30) as f64 * 30.0,
            (i / 30) as f64 * 20.0,
            28.0,
            16.0,
            TextContent::plain(&format!("Текст {i}")),
            props(12.0, TextSizing::Fixed),
        );
    }
    let scene = p.scene();
    let mut r = renderer(&fonts);
    // Зум 20 %: строки по 3 px и меньше — полосы, глифов нет (кадр дешёвый).
    let mut cam = camera(200.0, 100.0);
    cam.set_zoom(0.2);
    cam.set_center(kurbo::Point::new(450.0, 200.0));
    let (w, h) = cam.device_size();
    let mut backend = ferrum_render::CpuBackend::new(w, h);
    let first = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
    // Первый кадр раскладывает не всё: предел времени, остальное — полосы по оценке.
    assert!(
        first.text_layouts >= 1 && first.text_layouts < 600,
        "{first:?}"
    );
    assert_eq!(first.text_greeked, 600, "{first:?}");
    assert!(first.needs_another_frame);
    let mut frames = 1;
    loop {
        let s = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
        frames += 1;
        if !s.needs_another_frame {
            assert_eq!(s.text_greeked, 600, "{s:?}");
            break;
        }
        assert!(frames < 1000, "раскладка не догнала");
    }
    // Разложенное не раскладывается заново.
    let again = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
    assert_eq!(again.text_layouts, 0);
}

#[test]
fn loading_font_draws_nothing_until_it_arrives() {
    let fonts = fonts(false);
    let mut p = Page::new();
    add_text(
        &mut p,
        10.0,
        10.0,
        0.0,
        0.0,
        TextContent::plain("Ferrum"),
        props(24.0, TextSizing::AutoWidth),
    );
    // Размер рамки ставит редактор; здесь — вручную, как в файле.
    let mut scene = p.scene();
    let ids = p.doc.page(p.page).unwrap().walk();
    p.edit(ids[0], rect(10.0, 10.0, 90.0, 30.0));
    p.update(&mut scene);
    let mut r = renderer(&fonts);
    let blank = render_with(&mut r, &scene, &camera(120.0, 50.0), &NoImages);
    assert_eq!(dark_pixels(&blank, 0, 0, 120, 50), 0);
    let reqs = fonts.borrow_mut().take_requests();
    assert_eq!(reqs.len(), 1);
    for f in &reqs[0].files {
        fonts.borrow_mut().load("Inter", f, font_file(f)).unwrap();
    }
    let drawn = render_with(&mut r, &scene, &camera(120.0, 50.0), &NoImages);
    assert!(dark_pixels(&drawn, 0, 0, 120, 50) > 50);
}

#[test]
fn png_export_and_thumbnail_include_text() {
    let fonts = fonts(true);
    let mut p = Page::new();
    let id = add_text(
        &mut p,
        0.0,
        0.0,
        120.0,
        30.0,
        TextContent::plain("Экспорт"),
        props(24.0, TextSizing::Fixed),
    );
    let scene = p.scene();
    let mut r = renderer(&fonts);
    let png = r.export_pixmap(&scene, id, 2.0, &NoImages).unwrap();
    assert_eq!((png.width(), png.height()), (240, 60));
    let inked = png.data().iter().filter(|c| c.a > 128).count();
    assert!(inked > 300, "{inked}");
    let thumb = r
        .thumbnail_png(&scene, 480, 320, Some(WHITE), &NoImages)
        .unwrap()
        .unwrap();
    let thumb = ferrum_render::Pixmap::from_png(std::io::Cursor::new(thumb)).unwrap();
    assert!(dark_pixels(&thumb, 0, 0, 480, 320) > 300);
}
````

Run: `cargo test -p ferrum-render --test text`
Expected: FAIL — не собирается: нет `Renderer::set_fonts`, `FrameStats::text_greeked`.

- [ ] **Step 2: Рисование текстового узла**

Создать `crates/ferrum-render/src/text_node.rs`:

````rust
//! Рисование текстового узла (спека подсистемы 3, раздел 6): глифы — заливка
//! контуров `skrifa` через трейт бэкенда (как подписи оверлеев), украшения —
//! прямоугольники, мелкий текст — полосы по строкам (LOD, как Figma при отдалении).

use std::time::Duration;

use ferrum_doc::{Props, TextSizing};
use ferrum_text::{text_color, Fonts, LineBox, TextLayout};
use kurbo::{Affine, Rect};
use vello_cpu::peniko::Fill;

use crate::backend::{RenderBackend, Rgba};
use crate::display::color;

/// Строка ниже стольких пикселей устройства рисуется полосой.
pub const TEXT_LOD_PX: f64 = 3.0;
/// Прозрачность полосы LOD (спека 6).
pub const TEXT_GREEK_ALPHA: f32 = 0.25;
/// Сколько времени за кадр тратить на раскладку текстов, ещё не разложенных:
/// остальные — полосами по оценке строк, раскладка догоняет в следующих кадрах.
/// Хотя бы одна раскладка за кадр делается всегда.
pub const TEXT_LAYOUT_TIME_PER_FRAME: Duration = Duration::from_millis(4);
/// Межстрочный интервал «Авто» для оценки строк без раскладки, в кеглях.
const ESTIMATED_LINE_EM: f64 = 1.2;

/// Полоса строки: середина строки высотой 60 % — как строка текста издалека.
fn greek_bar(r: Rect) -> Rect {
    let h = r.height();
    Rect::new(r.x0, r.y0 + 0.2 * h, r.x1, r.y1 - 0.2 * h)
}

/// Рисует строки полосами цвета текста с прозрачностью `TEXT_GREEK_ALPHA`.
pub(crate) fn paint_greek(
    backend: &mut impl RenderBackend,
    lines: &[LineBox],
    to_device: Affine,
    alpha: f32,
) {
    backend.set_transform(to_device);
    for l in lines {
        if l.rect.width() <= 0.0 {
            continue;
        }
        backend.set_color(color(l.color).multiply_alpha(TEXT_GREEK_ALPHA * alpha));
        backend.fill_rect(&greek_bar(l.rect));
    }
}

/// Строки без раскладки — для полос, пока текст не разложен: кегль × 1.2 на
/// строку, во всю ширину рамки; у `AutoWidth` — по строке на абзац.
pub(crate) fn estimate_lines(p: &Props) -> Vec<LineBox> {
    let Some(t) = &p.text else {
        return Vec::new();
    };
    let lh = (t.props.style.size * ESTIMATED_LINE_EM).max(0.1);
    let rows = match t.props.sizing {
        TextSizing::AutoWidth => t.content.text().matches('\n').count().max(1),
        _ => ((p.h / lh).floor() as usize).clamp(1, 10_000),
    };
    let c = text_color(&p.fills);
    (0..rows)
        .map(|i| LineBox {
            rect: Rect::new(0.0, i as f64 * lh, p.w, (i + 1) as f64 * lh),
            baseline: (i as f64 + 0.8) * lh,
            color: c,
        })
        .collect()
}

/// Глифы и украшения раскладки. `to_device` — локальные координаты узла →
/// пиксели устройства.
pub(crate) fn paint_glyphs(
    backend: &mut impl RenderBackend,
    fonts: &mut Fonts,
    layout: &TextLayout,
    to_device: Affine,
    alpha: f32,
) {
    for run in &layout.runs {
        let upem = fonts.units_per_em(&run.font);
        let k = run.size as f64 / upem;
        // Синтетический курсив: наклон контура вправо на угол `skew`.
        let skew = run.skew.map_or(Affine::IDENTITY, |deg| {
            Affine::skew(-(deg as f64).to_radians().tan(), 0.0)
        });
        backend.set_color(color(run.color).multiply_alpha(alpha));
        for g in &run.glyphs {
            let path = fonts.outline(&run.font, g.id, &run.coords);
            if path.elements().is_empty() {
                continue;
            }
            backend.set_transform(
                to_device
                    * Affine::translate((g.x as f64, g.y as f64))
                    * skew
                    * Affine::scale_non_uniform(k, -k),
            );
            backend.fill_path(&path, Fill::NonZero);
        }
    }
    backend.set_transform(to_device);
    for d in &layout.decorations {
        backend.set_color(color(d.color).multiply_alpha(alpha));
        backend.fill_rect(&d.rect);
    }
}

/// Цвет мелкого текстового узла (LOD-прямоугольник): цвет текста с той же
/// прозрачностью, что у полос.
pub(crate) fn lod_color(p: &Props) -> Rgba {
    color(text_color(&p.fills)).multiply_alpha(TEXT_GREEK_ALPHA)
}
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
/// Цвет узла для LOD-прямоугольника без построения списка: первая видимая заливка,
/// иначе первая обводка.
pub fn lod_color(p: &Props) -> Option<Rgba> {
    if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
        if let Some(f) = p.fills.iter().find(|f| f.visible && f.opacity > 0.0) {
            return Some(match f.kind {
````

на:

````rust
/// Цвет узла для LOD-прямоугольника без построения списка: первая видимая заливка,
/// иначе первая обводка.
pub fn lod_color(p: &Props) -> Option<Rgba> {
    if p.kind == NodeKind::Text {
        return Some(crate::text_node::lod_color(p));
    }
    if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
        if let Some(f) = p.fills.iter().find(|f| f.visible && f.opacity > 0.0) {
            return Some(match f.kind {
````

- [ ] **Step 3: Ленивая раскладка в рисовальщике, общий реестр у рендерера**

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
//! Сборка кадра: обход сцены с отсечением невидимого и LOD, растровый кэш
//! мелких фреймов верхнего уровня, воспроизведение списков отрисовки в бэкенд.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
````

на:

````rust
//! Сборка кадра: обход сцены с отсечением невидимого и LOD, растровый кэш
//! мелких фреймов верхнего уровня, воспроизведение списков отрисовки в бэкенд.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
use ferrum_doc::{NodeId, NodeKind, ScaleMode};
use ferrum_scene::shape::Geometry;
use ferrum_scene::{Scene, SceneNode, Slot};
use kurbo::{Affine, BezPath, Point, Rect};
use vello_cpu::peniko::{Extend, Fill};
use vello_cpu::Pixmap;
````

на:

````rust
use ferrum_doc::{NodeId, NodeKind, ScaleMode};
use ferrum_scene::shape::Geometry;
use ferrum_scene::{Scene, SceneNode, Slot};
use ferrum_text::{Fonts, LayoutRequest, TextLayout};
use kurbo::{Affine, BezPath, Point, Rect};
use vello_cpu::peniko::{Extend, Fill};
use vello_cpu::Pixmap;
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
use crate::clock::{default_clock, Clock};
use crate::display::{self, DisplayList, Item, Paint, IMAGE_PLACEHOLDER};
use crate::image::ImageProvider;

/// Поддерево меньше полупикселя не рисуется вовсе.
pub const LOD_SKIP_PX: f64 = 0.5;
````

на:

````rust
use crate::clock::{default_clock, Clock};
use crate::display::{self, DisplayList, Item, Paint, IMAGE_PLACEHOLDER};
use crate::image::ImageProvider;
use crate::text_node::{self, TEXT_LAYOUT_TIME_PER_FRAME, TEXT_LOD_PX};

/// Поддерево меньше полупикселя не рисуется вовсе.
pub const LOD_SKIP_PX: f64 = 0.5;
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
    pub raster_builds: usize,
    pub clips: usize,
    pub layers: usize,
    /// Кадр неполный: часть растров не построена (бюджет кадра исчерпан, показаны
    /// прежние растры или узлы) или вместо картинки нарисована заглушка. Холст без
    /// правок и движения камеры должен запросить ещё кадр, иначе так и останется
````

на:

````rust
    pub raster_builds: usize,
    pub clips: usize,
    pub layers: usize,
    /// Текстовые узлы: разложено за кадр, нарисовано глифами, полосами (LOD).
    pub text_layouts: usize,
    pub text_glyphs: usize,
    pub text_greeked: usize,
    /// Кадр неполный: часть растров не построена (бюджет кадра исчерпан, показаны
    /// прежние растры или узлы) или вместо картинки нарисована заглушка. Холст без
    /// правок и движения камеры должен запросить ещё кадр, иначе так и останется
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
pub struct DrawContext<'a> {
    pub scene: &'a Scene,
    pub images: &'a dyn ImageProvider,
    /// Мир → пиксели устройства.
    pub transform: Affine,
    /// Пикселей устройства на единицу мира.
````

на:

````rust
pub struct DrawContext<'a> {
    pub scene: &'a Scene,
    pub images: &'a dyn ImageProvider,
    /// Реестр шрифтов и раскладка текста.
    pub fonts: &'a RefCell<Fonts>,
    /// Мир → пиксели устройства.
    pub transform: Affine,
    /// Пикселей устройства на единицу мира.
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
    }
}

/// Кэши между кадрами: списки отрисовки по слотам сцены, растры фреймов, общий
/// буфер для контуров обрезки.
pub struct Painter {
    lists: Vec<Option<DisplayList>>,
    rasters: HashMap<Slot, Raster>,
    path: BezPath,
    frame: u64,
````

на:

````rust
    }
}

/// Раскладка текстового узла: годна, пока не изменились вид узла (`version`)
/// и реестр шрифтов (`generation`).
struct CachedText {
    id: NodeId,
    version: u64,
    generation: u64,
    layout: Arc<TextLayout>,
    last_used: u64,
}

/// Кэши между кадрами: списки отрисовки по слотам сцены, растры фреймов, общий
/// буфер для контуров обрезки.
pub struct Painter {
    lists: Vec<Option<DisplayList>>,
    /// Раскладки текстовых узлов по слотам (ленивые: при первом рисовании).
    texts: HashMap<Slot, CachedText>,
    text_time_spent: Duration,
    rasters: HashMap<Slot, Raster>,
    path: BezPath,
    frame: u64,
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
    pub fn with_raster_limit(bytes: usize) -> Painter {
        Painter {
            lists: Vec::new(),
            rasters: HashMap::new(),
            path: BezPath::new(),
            frame: 0,
````

на:

````rust
    pub fn with_raster_limit(bytes: usize) -> Painter {
        Painter {
            lists: Vec::new(),
            texts: HashMap::new(),
            text_time_spent: Duration::ZERO,
            rasters: HashMap::new(),
            path: BezPath::new(),
            frame: 0,
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        self.frame += 1;
        self.raster_budget = RASTER_BUILDS_PER_FRAME;
        self.raster_time_spent = Duration::ZERO;
        self.missing_image = false;
        self.raster_full = false;
        if self.frame.is_multiple_of(64) {
````

на:

````rust
        self.frame += 1;
        self.raster_budget = RASTER_BUILDS_PER_FRAME;
        self.raster_time_spent = Duration::ZERO;
        self.text_time_spent = Duration::ZERO;
        self.missing_image = false;
        self.raster_full = false;
        if self.frame.is_multiple_of(64) {
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        }
        self.rasters
            .retain(|_, r| r.last_used + EVICT_AFTER_FRAMES >= frame);
        self.raster_bytes = self.rasters.values().map(Raster::bytes).sum();
    }

````

на:

````rust
        }
        self.rasters
            .retain(|_, r| r.last_used + EVICT_AFTER_FRAMES >= frame);
        self.texts
            .retain(|_, t| t.last_used + EVICT_AFTER_FRAMES >= frame);
        self.raster_bytes = self.rasters.values().map(Raster::bytes).sum();
    }

````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
            }
            return None;
        }
        let i = slot as usize;
        let split = part != Part::All;
        backend.set_transform(ctx.transform * n.world);
````

на:

````rust
            }
            return None;
        }
        if n.props.kind == NodeKind::Text {
            if part != Part::Strokes {
                self.draw_text(backend, ctx, slot, n, alpha);
            }
            return None;
        }
        let i = slot as usize;
        let split = part != Part::All;
        backend.set_transform(ctx.transform * n.world);
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        self.play(backend, &list, alpha, ctx, part);
        self.lists[i] = Some(list);
        None
    }

    fn play(
````

на:

````rust
        self.play(backend, &list, alpha, ctx, part);
        self.lists[i] = Some(list);
        None
    }

    /// Текстовый узел: раскладка из кэша или новая (в пределах времени кадра на
    /// экране; при экспорте — всегда), затем глифы или полосы строк (LOD).
    /// Раскладка ещё грузящимися шрифтами не рисуется: кадр перерисуют, когда
    /// придут байты (поколение реестра).
    fn draw_text(
        &mut self,
        backend: &mut impl RenderBackend,
        ctx: &DrawContext,
        slot: Slot,
        n: &SceneNode,
        alpha: f32,
    ) {
        let Some(text) = &n.props.text else { return };
        let to_device = ctx.transform * n.world;
        let generation = ctx.fonts.borrow().generation();
        let frame = self.frame;
        let cached = self
            .texts
            .get_mut(&slot)
            .filter(|c| c.id == n.id && c.version == n.version && c.generation == generation)
            .map(|c| {
                c.last_used = frame;
                c.layout.clone()
            });
        let layout = match cached {
            Some(l) => Some(l),
            None if self.can_layout_text(ctx) => {
                let started = self.clock.as_ref().map(|c| c.now_ms());
                let req = LayoutRequest::for_node(text, &n.props.fills, n.props.w, n.props.h);
                let layout = Arc::new(ctx.fonts.borrow_mut().layout(&req));
                if let (Some(t), Some(c)) = (started, self.clock.as_ref()) {
                    let ms = (c.now_ms() - t).max(0.0);
                    self.text_time_spent += Duration::from_secs_f64(ms / 1000.0);
                }
                self.stats.text_layouts += 1;
                self.texts.insert(
                    slot,
                    CachedText {
                        id: n.id,
                        version: n.version,
                        generation,
                        layout: layout.clone(),
                        last_used: self.frame,
                    },
                );
                Some(layout)
            }
            None => {
                // Время кадра на раскладку вышло: пока — полосы по оценке строк.
                self.stats.needs_another_frame = true;
                None
            }
        };
        let Some(layout) = layout else {
            text_node::paint_greek(
                backend,
                &text_node::estimate_lines(&n.props),
                to_device,
                alpha,
            );
            self.stats.text_greeked += 1;
            return;
        };
        if layout.loading {
            return;
        }
        let line_px = layout
            .lines
            .iter()
            .map(|l| l.rect.height())
            .fold(0.0, f64::max)
            * ctx.scale;
        if ctx.lod && line_px < TEXT_LOD_PX {
            text_node::paint_greek(backend, &layout.lines, to_device, alpha);
            self.stats.text_greeked += 1;
        } else {
            text_node::paint_glyphs(
                backend,
                &mut ctx.fonts.borrow_mut(),
                &layout,
                to_device,
                alpha,
            );
            self.stats.text_glyphs += 1;
        }
    }

    /// Можно ли разложить ещё один текст в этом кадре: экспорт и превью (без LOD)
    /// раскладывают всё; на экране — первый всегда, дальше в пределах времени.
    fn can_layout_text(&self, ctx: &DrawContext) -> bool {
        if !ctx.lod || self.stats.text_layouts == 0 {
            return true;
        }
        match self.clock.as_ref() {
            Some(_) => self.text_time_spent < TEXT_LAYOUT_TIME_PER_FRAME,
            None => true,
        }
    }

    fn play(
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
            let inner = DrawContext {
                scene: ctx.scene,
                images: ctx.images,
                transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
                scale,
                cull: None,
````

на:

````rust
            let inner = DrawContext {
                scene: ctx.scene,
                images: ctx.images,
                fonts: ctx.fonts,
                transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
                scale,
                cull: None,
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
mod image;
mod overlay;
mod text;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::Scene;
````

на:

````rust
mod image;
mod overlay;
mod text;
mod text_node;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::Scene;
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    FrameStats, EVICT_AFTER_FRAMES, LOD_RECT_PX, LOD_SKIP_PX, RASTER_BUILDS_PER_FRAME,
    RASTER_BUILD_TIME_PER_FRAME, RASTER_CACHE_BYTES, RASTER_MAX_PX, RASTER_MAX_SCALE,
};
#[cfg(feature = "gpu")]
pub use gpu::{atlas_config, AtlasStats, GpuBackend, GpuDevice, GPU_ATLAS_PAGES, GPU_ATLAS_SIDE};
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
````

на:

````rust
    FrameStats, EVICT_AFTER_FRAMES, LOD_RECT_PX, LOD_SKIP_PX, RASTER_BUILDS_PER_FRAME,
    RASTER_BUILD_TIME_PER_FRAME, RASTER_CACHE_BYTES, RASTER_MAX_PX, RASTER_MAX_SCALE,
};
pub use ferrum_text::{Fonts, SharedFonts};
#[cfg(feature = "gpu")]
pub use gpu::{atlas_config, AtlasStats, GpuBackend, GpuDevice, GPU_ATLAS_PAGES, GPU_ATLAS_SIDE};
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    MEASURE_DASH, MEASURE_GAP, PIXEL_GRID_MIN_ZOOM,
};
pub use text::{Label, TextEngine, ONEST};
#[cfg(feature = "gpu")]
pub use vello_common::{multi_atlas::AtlasConfig, paint::ImageId};
pub use vello_cpu::Pixmap;
````

на:

````rust
    MEASURE_DASH, MEASURE_GAP, PIXEL_GRID_MIN_ZOOM,
};
pub use text::{Label, TextEngine, ONEST};
pub use text_node::{TEXT_GREEK_ALPHA, TEXT_LAYOUT_TIME_PER_FRAME, TEXT_LOD_PX};
#[cfg(feature = "gpu")]
pub use vello_common::{multi_atlas::AtlasConfig, paint::ImageId};
pub use vello_cpu::Pixmap;
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
}

/// Рендерер одного холста: кэши списков отрисовки, растров и шрифтов живут между кадрами.
#[derive(Default)]
pub struct Renderer {
    painter: Painter,
    text: TextEngine,
}

impl Renderer {
````

на:

````rust
}

/// Рендерер одного холста: кэши списков отрисовки, растров и шрифтов живут между кадрами.
pub struct Renderer {
    painter: Painter,
    text: TextEngine,
    /// Реестр шрифтов и раскладка текстовых узлов — общие с редактором
    /// (`set_fonts`); свой рендерер без реестра знает только вшитый Onest.
    fonts: SharedFonts,
}

impl Default for Renderer {
    fn default() -> Self {
        Renderer::with_raster_cache_bytes(RASTER_CACHE_BYTES)
    }
}

impl Renderer {
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
        Renderer {
            painter: Painter::with_raster_limit(bytes),
            text: TextEngine::new(),
        }
    }

    /// Предел времени на построение растров фреймов за кадр (по умолчанию
````

на:

````rust
        Renderer {
            painter: Painter::with_raster_limit(bytes),
            text: TextEngine::new(),
            fonts: Fonts::shared(),
        }
    }

    /// Общий реестр шрифтов ядра. Раскладки, посчитанные со старым реестром,
    /// устаревают по его поколению.
    pub fn set_fonts(&mut self, fonts: SharedFonts) {
        self.fonts = fonts;
    }

    pub fn fonts(&self) -> &SharedFonts {
        &self.fonts
    }

    /// Предел времени на построение растров фреймов за кадр (по умолчанию
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
        let ctx = DrawContext {
            scene,
            images,
            transform: camera.view_transform(),
            scale: camera.device_scale(),
            cull: Some(camera.visible_world_rect()),
````

на:

````rust
        let ctx = DrawContext {
            scene,
            images,
            fonts: &self.fonts,
            transform: camera.view_transform(),
            scale: camera.device_scale(),
            cull: Some(camera.visible_world_rect()),
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
        let ctx = DrawContext {
            scene,
            images,
            transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
            scale,
            cull: None,
````

на:

````rust
        let ctx = DrawContext {
            scene,
            images,
            fonts: &self.fonts,
            transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
            scale,
            cull: None,
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
        let ctx = DrawContext {
            scene,
            images,
            transform,
            scale,
            cull: None,
````

на:

````rust
        let ctx = DrawContext {
            scene,
            images,
            fonts: &self.fonts,
            transform,
            scale,
            cull: None,
````

- [ ] **Step 4: Эталон и проверка**

Run: `FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test text`
Expected: PASS; создан `crates/ferrum-render/tests/golden/text.png` 220 × 90: «Ferrum» жирным чёрным и «текст» красным с подчёркиванием в первой строке, «вторая строка» — во второй. Открыть файл и сверить глазами.

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ferrum-render`
Expected: PASS: `text` — 5 тестов (эталон, полосы при зуме 5 %, раскладка в пределах кадра догоняет и кэшируется, шрифт грузится — пусто, после загрузки — глифы, PNG @2x 240 × 60 и превью с текстом); прочие наборы — как прежде.

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-render
git commit -m "feat(render): текст — глифы контурами skrifa, украшения, полосы LOD, ленивая раскладка в пределах кадра, общий реестр шрифтов; текст в экспорте PNG и превью

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Текст в редакторе: размер по раскладке, отсутствующие шрифты, копии

Спека 5: «`autoWidth` меняет ширину узла, `autoHeight` — высоту: размер пишется в узел той же транзакцией, что и правка текста»; спека 4: «Отсутствующие шрифты (N)… замена одним действием (одна правка — один шаг отмены)». Редактор держит общий реестр (`Editor::set_fonts`). `fit_size` — размер рамки по раскладке; шрифт грузится — `TextPending`, правка не делается (ядро повторит её по приходу байтов, Task 7). `create_text` — текстовый узел одной транзакцией (создание, свойства, содержимое, размер) — для тестовой команды и стенда. `missing_fonts` и `replace_font` — по текущей странице; замена переписывает умолчание узла, если оно было `from`, и разметку участков так, чтобы шрифт `from` стал `to`, а вид остальных участков не изменился (участок с явным семейством без явного начертания иначе сменил бы начертание вслед за умолчанием); размер текстов с авторазмером — по новой раскладке, всё — одним шагом. Ресайз текста (ручки, поля, масштаб группы) переводит его в `Fixed`. Копирование, вставка и дублирование создают узел сразу со всеми свойствами и содержимым (`Tx::create_with_props`); в буфере обмена у узла текста — поле `text` с JSON текста.

**Files:**
- Create: `crates/ferrum-editor/src/text.rs`
- Modify: `crates/ferrum-editor/Cargo.toml`, `src/editor.rs` (`Editor` 122–153, `new` 157–214), `src/lib.rs`, `src/tools/select.rs` (`commit_patches` 784–792), `src/panel.rs` (запись правки 459–473), `src/clipboard.rs` (`JsonNode` 87–110, `encode` 192–261, `props_from_json` 407–488), `src/ops.rs` (`duplicate` 37–72), `src/actions.rs` (`paste` 556–600), `crates/ferrum-scene/src/scene.rs` (`len` 253–255)
- Test: `crates/ferrum-editor/tests/text.rs`

**Interfaces:**
- Consumes: `Fonts`, `LayoutRequest`, `FontStatus`, `SharedFonts` (Task 4); `Tx::create_with_props`, `TextData::{to_json, from_json}` (Task 2).
- Produces: `Editor::set_fonts(SharedFonts)`, `Editor::fonts()`, `Editor::create_text(at: Point, &TextContent, &TextProps, width, height) -> Result<Option<NodeId>, TextPending>`, `Editor::page_fonts() -> Vec<FontName>`, `Editor::missing_fonts() -> Vec<MissingFont { font, nodes }>`, `Editor::replace_font(&from, &to) -> Result<usize, TextPending>`; `ferrum_editor::{fit_size, MissingFont, TextPending}`; `Scene::iter() -> impl Iterator<Item = (Slot, &SceneNode)>` (живые узлы без черновиков).

- [ ] **Step 1: Падающие сценарные тесты**

Создать `crates/ferrum-editor/tests/text.rs`:

````rust
//! Текст в редакторе (подсистема 3, этап 1): размер по раскладке в той же
//! транзакции, ожидание шрифтов, ресайз переводит в `Fixed`, отсутствующие шрифты
//! и замена одним шагом, копирование и дублирование с содержимым.

mod common;

use std::path::PathBuf;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;
use ferrum_text::{group, read_faces, Fonts, SharedFonts, SourceKind};
use kurbo::Point;

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const FILES: [&str; 4] = [
    "inter/Inter-Variable.ttf",
    "inter/Inter-Italic-Variable.ttf",
    "roboto/Roboto-Variable.ttf",
    "roboto/Roboto-Italic-Variable.ttf",
];

/// Реестр со списком Inter и Roboto; загружены — те, что в `loaded`.
fn fonts(loaded: &[&str]) -> SharedFonts {
    let shared = Fonts::shared();
    {
        let mut f = shared.borrow_mut();
        let faces = FILES
            .iter()
            .flat_map(|r| read_faces(&font_file(r), r))
            .collect::<Vec<_>>();
        f.add_catalog(SourceKind::Bundled, group(faces));
        for rel in loaded {
            let family = if rel.starts_with("inter") {
                "Inter"
            } else {
                "Roboto"
            };
            f.load(family, rel, font_file(rel)).unwrap();
        }
        f.take_requests();
    }
    shared
}

fn h_with(fonts: &SharedFonts) -> H {
    let mut h = H::new();
    h.ed.set_fonts(fonts.clone());
    h
}

fn props(size: f64) -> TextProps {
    let mut p = TextProps::default();
    p.style.size = size;
    p
}

fn text_of(h: &mut H, id: NodeId) -> Props {
    h.ed.page_doc().props(id).unwrap()
}

#[test]
fn created_text_gets_its_size_in_the_same_step() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
    let id =
        h.ed.create_text(
            Point::new(10.0, 20.0),
            &TextContent::plain("Привет"),
            &props(24.0),
            0.0,
            0.0,
        )
        .unwrap()
        .unwrap();
    let p = text_of(&mut h, id);
    assert_eq!((p.x, p.y), (10.0, 20.0));
    assert!(p.w > 60.0 && p.w < 120.0, "{}", p.w);
    assert_eq!(p.h, (24.0f64 * 1.2109375).ceil());
    assert_eq!(h.ed.selection(), &[id]);
    assert_eq!(h.ed.step_count(), 1);
    h.ed.execute(CommandId::Undo);
    assert!(h.ed.page_doc().walk().is_empty());
    // Перенос по ширине рамки: высота по строкам.
    let mut tall = props(24.0);
    tall.sizing = TextSizing::AutoHeight;
    let id =
        h.ed.create_text(
            Point::ZERO,
            &TextContent::plain("Съешь же ещё этих мягких булок"),
            &tall,
            120.0,
            0.0,
        )
        .unwrap()
        .unwrap();
    let p = text_of(&mut h, id);
    assert_eq!(p.w, 120.0);
    assert!(p.h > 3.0 * 29.0, "{}", p.h);
}

#[test]
fn text_waits_for_fonts_and_changes_nothing_meanwhile() {
    let f = fonts(&[]);
    let mut h = h_with(&f);
    let r = h.ed.create_text(
        Point::ZERO,
        &TextContent::plain("Ferrum"),
        &props(12.0),
        0.0,
        0.0,
    );
    assert_eq!(r, Err(TextPending));
    assert!(h.ed.page_doc().walk().is_empty());
    let reqs = f.borrow_mut().take_requests();
    assert_eq!(reqs[0].family, "Inter");
    for file in &reqs[0].files {
        f.borrow_mut().load("Inter", file, font_file(file)).unwrap();
    }
    assert!(h
        .ed
        .create_text(
            Point::ZERO,
            &TextContent::plain("Ferrum"),
            &props(12.0),
            0.0,
            0.0
        )
        .unwrap()
        .is_some());
}

#[test]
fn resizing_text_makes_it_fixed_in_one_step() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
    let id =
        h.ed.create_text(
            Point::ZERO,
            &TextContent::plain("Ferrum"),
            &props(24.0),
            0.0,
            0.0,
        )
        .unwrap()
        .unwrap();
    let before = text_of(&mut h, id);
    let steps = h.ed.step_count();
    h.ed.edit_selection(
        &PanelEdit::Number {
            field: NumField::W,
            value: 300.0,
            relative: false,
        },
        Phase::Commit,
    );
    let after = text_of(&mut h, id);
    assert_eq!(after.w, 300.0);
    assert_eq!(after.text_props().unwrap().sizing, TextSizing::Fixed);
    assert_eq!(h.ed.step_count(), steps + 1);
    h.ed.execute(CommandId::Undo);
    assert_eq!(text_of(&mut h, id), before);
}

#[test]
fn missing_fonts_are_listed_and_replaced_in_one_step() {
    let f = fonts(&FILES);
    let mut h = h_with(&f);
    let mut comic = props(24.0);
    comic.style.font = FontName::new("Comic Sans", "Bold");
    let a =
        h.ed.create_text(Point::ZERO, &TextContent::plain("Первый"), &comic, 0.0, 0.0)
            .unwrap()
            .unwrap();
    let b =
        h.ed.create_text(
            Point::new(0.0, 100.0),
            &TextContent::plain("Второй"),
            &comic,
            0.0,
            0.0,
        )
        .unwrap()
        .unwrap();
    // Третий: Inter по умолчанию, одно слово — отсутствующим шрифтом.
    let c =
        h.ed.create_text(
            Point::new(0.0, 200.0),
            &TextContent::plain("Три слова"),
            &props(24.0),
            0.0,
            0.0,
        )
        .unwrap()
        .unwrap();
    h.ed.transact(|tx| {
        tx.format_text(
            c,
            0..3,
            &TextAttrs {
                font: Some("Comic Sans".into()),
                style: Some("Bold".into()),
                ..Default::default()
            },
            &[],
        )?;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(
        h.ed.missing_fonts(),
        vec![MissingFont {
            font: FontName::new("Comic Sans", "Bold"),
            nodes: 3,
        }]
    );
    let before: Vec<Props> = [a, b, c].iter().map(|id| text_of(&mut h, *id)).collect();
    let steps = h.ed.step_count();
    let roboto = FontName::new("Roboto", "Bold");
    assert_eq!(
        h.ed.replace_font(&FontName::new("Comic Sans", "Bold"), &roboto),
        Ok(3)
    );
    assert_eq!(h.ed.step_count(), steps + 1);
    assert!(h.ed.missing_fonts().is_empty());
    let pa = text_of(&mut h, a);
    assert_eq!(pa.text_props().unwrap().style.font, roboto);
    // Ширина — по раскладке Roboto Bold, а не прежняя.
    assert_ne!(pa.w, before[0].w);
    let pc = text_of(&mut h, c);
    let tc = pc.text.unwrap();
    assert_eq!(tc.props.style.font, FontName::inter_regular());
    assert_eq!(tc.content.runs[0].attrs.font_name(&tc.props.style), roboto);
    assert_eq!(
        tc.content.runs[1].attrs.font_name(&tc.props.style),
        FontName::inter_regular()
    );
    h.ed.execute(CommandId::Undo);
    let after_undo: Vec<Props> = [a, b, c].iter().map(|id| text_of(&mut h, *id)).collect();
    assert_eq!(after_undo, before);
}

#[test]
fn replacement_waits_for_the_new_font() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
    let mut comic = props(24.0);
    comic.style.font = FontName::new("Comic Sans", "Regular");
    h.ed.create_text(Point::ZERO, &TextContent::plain("Текст"), &comic, 0.0, 0.0)
        .unwrap();
    let r = h.ed.replace_font(
        &FontName::new("Comic Sans", "Regular"),
        &FontName::new("Roboto", "Regular"),
    );
    assert_eq!(r, Err(TextPending));
    assert_eq!(h.ed.missing_fonts().len(), 1);
}

#[test]
fn copy_paste_and_duplicate_keep_text_and_marks() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
    let id =
        h.ed.create_text(
            Point::new(5.0, 5.0),
            &TextContent::plain("Копия"),
            &props(20.0),
            0.0,
            0.0,
        )
        .unwrap()
        .unwrap();
    h.ed.transact(|tx| {
        tx.format_text(
            id,
            0..2,
            &TextAttrs {
                decoration: Some(Decoration::Underline),
                ..Default::default()
            },
            &[],
        )?;
        Ok(vec![id])
    })
    .unwrap();
    let original = text_of(&mut h, id).text.unwrap();
    h.ed.execute(CommandId::Copy);
    let bytes =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::Clipboard(b) => Some(b),
                _ => None,
            })
            .expect("копия в буфер");
    h.ed.paste(&bytes).unwrap();
    let pasted = h.ed.selection()[0];
    assert_ne!(pasted, id);
    assert_eq!(text_of(&mut h, pasted).text.unwrap(), original);
    h.ed.select(&[id]);
    h.ed.execute(CommandId::Duplicate);
    let dup = h.ed.selection()[0];
    assert_ne!(dup, id);
    assert_eq!(text_of(&mut h, dup).text.unwrap(), original);
}
````

Run: `cargo test -p ferrum-editor --test text`
Expected: FAIL — не собирается: нет `Editor::set_fonts`, `create_text`, `MissingFont`.

- [ ] **Step 2: Живые узлы сцены одним проходом**

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Счётчик изменений: растёт при каждом непустом `apply` и каждом превью,
````

на:

````rust

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Живые узлы документа (без черновиков жеста) в порядке слотов.
    pub fn iter(&self) -> impl Iterator<Item = (Slot, &SceneNode)> + '_ {
        self.nodes
            .chunks
            .iter()
            .flatten()
            .enumerate()
            .filter_map(|(i, n)| Some((i as Slot, n.as_ref()?)))
            .filter(|(s, _)| !self.draft_set.contains(s))
    }

    /// Счётчик изменений: растёт при каждом непустом `apply` и каждом превью,
````

- [ ] **Step 3: Текст в редакторе**

В `crates/ferrum-editor/Cargo.toml` заменить:

````toml
ferrum-doc = { path = "../ferrum-doc" }
ferrum-scene = { path = "../ferrum-scene" }
ferrum-render = { path = "../ferrum-render" }
blake3.workspace = true
kurbo.workspace = true
serde.workspace = true
````

на:

````toml
ferrum-doc = { path = "../ferrum-doc" }
ferrum-scene = { path = "../ferrum-scene" }
ferrum-render = { path = "../ferrum-render" }
ferrum-text = { path = "../ferrum-text" }
blake3.workspace = true
kurbo.workspace = true
serde.workspace = true
````

Создать `crates/ferrum-editor/src/text.rs`:

````rust
//! Текстовые узлы в редакторе (подсистема 3, этап 1): размер по раскладке в
//! той же транзакции, что и правка текста (спека 5), создание текста (тестовая
//! команда — инструмента T нет до этапа 2), отсутствующие шрифты и их замена
//! одним шагом (спека 4), ресайз текста рамкой.
//!
//! Раскладке нужны байты шрифтов. Если какой-то шрифт ещё грузится, правка не
//! делается: `TextPending` — ядро повторит её, когда шрифты придут.

use std::sync::Arc;

use ferrum_doc::{
    DocResult, FontName, NodeId, NodeKind, Props, PropsPatch, TextContent, TextData, TextProps,
    TextSizing, Tx,
};
use ferrum_text::{FontStatus, Fonts, LayoutRequest, SharedFonts};
use kurbo::Point;

use crate::editor::Editor;

/// Правка ждёт шрифтов: их байты запрошены (`Fonts::take_requests`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPending;

/// Правка одного текста при замене шрифта: новые свойства (если сменилось
/// умолчание), содержимое и размер по новой раскладке (у авторазмера).
struct FontEdit {
    id: NodeId,
    props: Option<TextProps>,
    content: TextContent,
    size: Option<(f64, f64)>,
}

/// Отсутствующий шрифт текущей страницы и сколько текстов им набрано.
#[derive(Debug, Clone, PartialEq)]
pub struct MissingFont {
    pub font: FontName,
    pub nodes: usize,
}

/// Размер рамки по раскладке: `AutoWidth` — ширина и высота текста, `AutoHeight`
/// — высота при ширине рамки, `Fixed` — без изменений (`None`). Шрифт грузится —
/// `Err(TextPending)`.
pub fn fit_size(fonts: &mut Fonts, p: &Props) -> Result<Option<(f64, f64)>, TextPending> {
    let Some(t) = &p.text else {
        return Ok(None);
    };
    if t.props.sizing == TextSizing::Fixed {
        return Ok(None);
    }
    let layout = fonts.layout(&LayoutRequest::for_node(t, &p.fills, p.w, p.h));
    if layout.loading {
        return Err(TextPending);
    }
    // Ширина и высота — целыми вверх, как у рамки текста в Figma.
    let h = layout.height.ceil();
    Ok(Some(match t.props.sizing {
        TextSizing::AutoWidth => (layout.width.ceil(), h),
        _ => (p.w, h),
    }))
}

/// Ресайз текста рамкой (ручки, поля W/H, масштаб группы) переводит его в
/// `Fixed`: размер больше не по тексту. Этап 2 добавит правила Figma для
/// горизонтального ресайза (`AutoHeight`) вместе с режимом размера в панели.
pub(crate) fn sizing_after_resize(before: &Props, patch: &PropsPatch) -> PropsPatch {
    let Some(t) = &before.text else {
        return patch.clone();
    };
    let resized = patch.w.is_some_and(|w| w != before.w) || patch.h.is_some_and(|h| h != before.h);
    if !resized || t.props.sizing == TextSizing::Fixed || patch.text.is_some() {
        return patch.clone();
    }
    PropsPatch {
        text: Some(TextProps {
            sizing: TextSizing::Fixed,
            ..t.props.clone()
        }),
        ..patch.clone()
    }
}

/// `set_props` с правилом ресайза текста (`sizing_after_resize`).
pub(crate) fn set_props_resizing(tx: &Tx, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
    let before = tx.props(id)?;
    tx.set_props(id, &sizing_after_resize(&before, patch))
}

/// Новый шрифт участков после замены `from` → `to`: участок, набранный `from`
/// (с учётом умолчания узла), получает `to`; остальные не меняют вида, даже
/// если умолчание узла сменилось.
fn replaced_content(
    content: &TextContent,
    old_default: &TextProps,
    new_default: &TextProps,
    from: &FontName,
    to: &FontName,
) -> TextContent {
    let mut out = content.clone();
    for run in &mut out.runs {
        let before = run.attrs.font_name(&old_default.style);
        let wanted = if &before == from { to.clone() } else { before };
        let after = run.attrs.font_name(&new_default.style);
        if after != wanted {
            run.attrs.font = Some(wanted.family.clone());
            run.attrs.style = Some(wanted.style.clone());
        }
        // Разметка, совпавшая с умолчанием узла, снимается: участок «как у узла».
        if run.attrs.font.as_deref() == Some(new_default.style.font.family.as_str())
            && run.attrs.style.as_deref() == Some(new_default.style.font.style.as_str())
        {
            run.attrs.font = None;
            run.attrs.style = None;
        }
    }
    // Соседние участки с одинаковой разметкой — один участок.
    let mut merged: Vec<ferrum_doc::TextRun> = Vec::with_capacity(out.runs.len());
    for run in out.runs {
        match merged.last_mut() {
            Some(last) if last.attrs == run.attrs => last.text.push_str(&run.text),
            _ => merged.push(run),
        }
    }
    TextContent { runs: merged }
}

impl Editor {
    /// Общий реестр шрифтов ядра (тот же у рендера).
    pub fn set_fonts(&mut self, fonts: SharedFonts) {
        self.fonts = fonts;
    }

    pub fn fonts(&self) -> &SharedFonts {
        &self.fonts
    }

    /// Текстовый узел в точке мира `at` (левый верхний угол) на странице, одним
    /// шагом отмены, с размером по раскладке; узел выделяется. Нужно тестам и
    /// стенду — инструмент T появится на этапе 2. `width` — ширина рамки для
    /// `AutoHeight` и `Fixed`, `height` — высота для `Fixed`.
    pub fn create_text(
        &mut self,
        at: Point,
        content: &TextContent,
        props: &TextProps,
        width: f64,
        height: f64,
    ) -> Result<Option<NodeId>, TextPending> {
        let mut p = Props::defaults(NodeKind::Text);
        (p.x, p.y, p.w, p.h) = (at.x, at.y, width.max(0.0), height.max(0.0));
        p.text = Some(Arc::new(TextData {
            props: props.clone(),
            content: content.clone(),
        }));
        if let Some((w, h)) = fit_size(&mut self.fonts.borrow_mut(), &p)? {
            (p.w, p.h) = (w, h);
        }
        let result = self.transact(|tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_props(
                id,
                &PropsPatch {
                    x: Some(p.x),
                    y: Some(p.y),
                    w: Some(p.w),
                    h: Some(p.h),
                    text: Some(props.clone()),
                    ..Default::default()
                },
            )?;
            tx.set_text_content(id, content)?;
            Ok(vec![id])
        });
        Ok(result.ok().and_then(|ids| ids.first().copied()))
    }

    /// Шрифты текстов текущей страницы, без повторов.
    pub fn page_fonts(&self) -> Vec<FontName> {
        let mut out: Vec<FontName> = Vec::new();
        for (_, n) in self.scene.iter() {
            if let Some(t) = &n.props.text {
                for f in t.content.fonts(&t.props.style) {
                    if !out.contains(&f) {
                        out.push(f);
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// Отсутствующие шрифты текущей страницы (спека 4): нет ни во вшитых, ни в
    /// системных. Пока список вшитых не пришёл — пусто.
    pub fn missing_fonts(&self) -> Vec<MissingFont> {
        let fonts = self.fonts.borrow();
        let mut out: Vec<MissingFont> = Vec::new();
        for (_, n) in self.scene.iter() {
            let Some(t) = &n.props.text else {
                continue;
            };
            let mut seen: Vec<FontName> = Vec::new();
            for f in t.content.fonts(&t.props.style) {
                if fonts.status(&f) != FontStatus::Missing || seen.contains(&f) {
                    continue;
                }
                match out.iter_mut().find(|m| m.font == f) {
                    Some(m) => m.nodes += 1,
                    None => out.push(MissingFont {
                        font: f.clone(),
                        nodes: 1,
                    }),
                }
                seen.push(f);
            }
        }
        out.sort_by(|a, b| a.font.cmp(&b.font));
        out
    }

    /// Замена шрифта `from` на `to` во всех текстах текущей страницы одним шагом
    /// отмены (спека 4); размер текстов с авторазмером — по новой раскладке в той
    /// же транзакции. Возвращает число изменённых узлов; новый шрифт ещё
    /// грузится — `Err(TextPending)` и ничего не меняется.
    pub fn replace_font(&mut self, from: &FontName, to: &FontName) -> Result<usize, TextPending> {
        let mut edits: Vec<FontEdit> = Vec::new();
        let mut pending = false;
        for (_, n) in self.scene.iter() {
            let Some(t) = &n.props.text else {
                continue;
            };
            if !t.content.fonts(&t.props.style).contains(from) {
                continue;
            }
            let mut props = t.props.clone();
            if &props.style.font == from {
                props.style.font = to.clone();
            }
            let content = replaced_content(&t.content, &t.props, &props, from, to);
            let mut p = n.props.clone();
            p.text = Some(Arc::new(TextData {
                props: props.clone(),
                content: content.clone(),
            }));
            let size = match fit_size(&mut self.fonts.borrow_mut(), &p) {
                Ok(s) => s,
                Err(TextPending) => {
                    pending = true;
                    None
                }
            };
            edits.push(FontEdit {
                id: n.id,
                props: (props != t.props).then_some(props),
                content,
                size,
            });
        }
        if pending {
            return Err(TextPending);
        }
        if edits.is_empty() {
            return Ok(0);
        }
        let count = edits.len();
        let selection = self.selection.to_vec();
        let _ = self.transact(|tx| {
            for e in &edits {
                tx.set_props(
                    e.id,
                    &PropsPatch {
                        text: e.props.clone(),
                        w: e.size.map(|s| s.0),
                        h: e.size.map(|s| s.1),
                        ..Default::default()
                    },
                )?;
                tx.set_text_content(e.id, &e.content)?;
            }
            Ok(selection)
        });
        Ok(count)
    }
}
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
use ferrum_doc::{Changes, DocError, DocResult, Document, NodeId, PageDoc, PageId, Tx};
use ferrum_render::{Camera, Measure, Overlay, OverlayTheme};
use ferrum_scene::Scene;
use kurbo::{Point, Size, Vec2};

use crate::commands::CommandId;
````

на:

````rust
use ferrum_doc::{Changes, DocError, DocResult, Document, NodeId, PageDoc, PageId, Tx};
use ferrum_render::{Camera, Measure, Overlay, OverlayTheme};
use ferrum_scene::Scene;
use ferrum_text::{Fonts, SharedFonts};
use kurbo::{Point, Size, Vec2};

use crate::commands::CommandId;
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Измерения для оверлея; пересчитываются в `emit`.
    measures: Vec<Measure>,
    measure_cache: Option<MeasureCache>,
}

impl Editor {
````

на:

````rust
    /// Измерения для оверлея; пересчитываются в `emit`.
    measures: Vec<Measure>,
    measure_cache: Option<MeasureCache>,
    /// Реестр шрифтов для размера текста по раскладке (общий с рендером ядра).
    pub(crate) fonts: SharedFonts,
}

impl Editor {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            alt: AltState::default(),
            measures: Vec::new(),
            measure_cache: None,
        })
    }

````

на:

````rust
            alt: AltState::default(),
            measures: Vec::new(),
            measure_cache: None,
            fonts: Fonts::shared(),
        })
    }

````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod presets;
pub mod selection;
pub mod snap;
pub mod tools;

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
````

на:

````rust
pub mod presets;
pub mod selection;
pub mod snap;
pub mod text;
pub mod tools;

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
    display_rotation, field_applies, panel_pose, DropTarget, NumField, PanelEdit, PanelPose, Phase,
};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

на:

````rust
    display_rotation, field_applies, panel_pose, DropTarget, NumField, PanelEdit, PanelPose, Phase,
};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use text::{fit_size, MissingFont, TextPending};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

- [ ] **Step 4: Ресайз текста — `Fixed`**

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
    let sel = cx.selection.to_vec();
    cx.commit(|tx, _| {
        for (id, patch) in &patches {
            tx.set_props(*id, patch)?;
        }
        Ok(sel)
    });
````

на:

````rust
    let sel = cx.selection.to_vec();
    cx.commit(|tx, _| {
        for (id, patch) in &patches {
            crate::text::set_props_resizing(tx, *id, patch)?;
        }
        Ok(sel)
    });
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
                    let selection = self.selection.to_vec();
                    self.ctx().commit(|tx, _| {
                        for (id, patch) in &p {
                            tx.set_props(*id, patch)?;
                        }
                        Ok(selection)
                    });
````

на:

````rust
                    let selection = self.selection.to_vec();
                    self.ctx().commit(|tx, _| {
                        for (id, patch) in &p {
                            crate::text::set_props_resizing(tx, *id, patch)?;
                        }
                        Ok(selection)
                    });
````

- [ ] **Step 5: Копии и буфер обмена с текстом**

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
    flip_y: bool,
    fills: Vec<JsonFill>,
    strokes: Vec<JsonStroke>,
}

fn is_false(v: &bool) -> bool {
````

на:

````rust
    flip_y: bool,
    fills: Vec<JsonFill>,
    strokes: Vec<JsonStroke>,
    /// Текст узла (`TextData::to_json`) — только у текста. Сборка без текста поле
    /// не знает и пропускает узел «text» как неизвестный (версия формата та же).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

fn is_false(v: &bool) -> bool {
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
                                align: align(s.align).to_string(),
                            })
                            .collect(),
                    }
                })
                .collect(),
````

на:

````rust
                                align: align(s.align).to_string(),
                            })
                            .collect(),
                        text: p.text.as_ref().map(|t| t.to_json()),
                    }
                })
                .collect(),
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
        flip_y: n.flip_y,
        fills,
        strokes,
        // Текст копии — пустой до этапа с форматом текста в буфере (Task 6).
        text: (kind == NodeKind::Text).then(|| Arc::new(TextData::default())),
    };
    // Нечисловые и огромные значения — к допустимым, как у строк кэша сцены.
    shape::sanitize(&mut props);
````

на:

````rust
        flip_y: n.flip_y,
        fills,
        strokes,
        // Текст — только у текстового узла; пределы проверяет `from_json`.
        text: match (kind, n.text.as_deref()) {
            (NodeKind::Text, Some(json)) => Some(Arc::new(
                TextData::from_json(json).map_err(|e| corrupted(e.to_string()))?,
            )),
            (NodeKind::Text, None) => Some(Arc::new(TextData::default())),
            _ => None,
        },
    };
    // Нечисловые и огромные значения — к допустимым, как у строк кэша сцены.
    shape::sanitize(&mut props);
````

В `crates/ferrum-editor/src/ops.rs` заменить:

````rust
use ferrum_scene::Scene;
use kurbo::{Affine, Vec2};

use crate::clipboard::full_patch;
use crate::geometry::{decompose_props, parent_world};

/// Сдвиг в мире, переведённый в координаты родителя узла (только линейная часть).
````

на:

````rust
use ferrum_scene::Scene;
use kurbo::{Affine, Vec2};

use crate::geometry::{decompose_props, parent_world};

/// Сдвиг в мире, переведённый в координаты родителя узла (только линейная часть).
````

В `crates/ferrum-editor/src/ops.rs` заменить:

````rust
        props.x += d.x;
        props.y += d.y;
        let index = scene.index(slot) as usize + 1;
        let root = tx.create_node(parent, Some(index), props.kind)?;
        tx.set_props(root, &full_patch(&props))?;
        // Поддерево — обходом без рекурсии: глубина не ограничена.
        let mut stack = vec![(id, root)];
        while let Some((src, dst)) = stack.pop() {
            for child in tx.children(Some(src)) {
                let p = tx.props(child)?;
                let copy = tx.create_node(Some(dst), None, p.kind)?;
                tx.set_props(copy, &full_patch(&p))?;
                stack.push((child, copy));
            }
        }
````

на:

````rust
        props.x += d.x;
        props.y += d.y;
        let index = scene.index(slot) as usize + 1;
        // Со всеми свойствами сразу: у текста — и содержимое.
        let root = tx.create_with_props(parent, Some(index), &props)?;
        // Поддерево — обходом без рекурсии: глубина не ограничена.
        let mut stack = vec![(id, root)];
        while let Some((src, dst)) = stack.pop() {
            for child in tx.children(Some(src)) {
                let p = tx.props(child)?;
                let copy = tx.create_with_props(Some(dst), None, &p)?;
                stack.push((child, copy));
            }
        }
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
                        target
                    }
                };
                let id = tx.create_node(parent, None, props.kind)?;
                tx.set_props(id, &full_patch(&props))?;
                if n.parent.is_none() {
                    roots.push(id);
                }
````

на:

````rust
                        target
                    }
                };
                let id = tx.create_with_props(parent, None, &props)?;
                if n.parent.is_none() {
                    roots.push(id);
                }
````

- [ ] **Step 6: Проверить**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ferrum-editor -p ferrum-scene`
Expected: PASS: `text` — 6 тестов (размер в том же шаге; ожидание шрифтов; ресайз → `Fixed` одним шагом; три текста с отсутствующим шрифтом — один пункт списка, замена одним шагом, ширина по Roboto Bold, отмена возвращает всё; замена ждёт Roboto; копия, вставка и дубликат — с текстом и разметкой); остальные наборы `ferrum-editor` (`clipboard`, `clipboard_format`, `property` и др.) и `ferrum-scene` — как прежде.

- [ ] **Step 7: Закоммитить**

```bash
git add crates/ferrum-editor crates/ferrum-scene Cargo.lock
git commit -m "feat(editor): текст — размер по раскладке в той же транзакции, создание текста, отсутствующие шрифты и замена одним шагом, ресайз в Fixed, копии и буфер обмена с текстом

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Ядро worker'а: команды и события шрифтов, ожидание шрифтов, пример из текстов

Спека 8 (часть этапа 1) и решения 13 выше. Ядро создаёт один реестр и отдаёт его редактору и рендереру (и рендереру эталонного кадра `ReadPixels`), переживает замену документа. `FontsAvailable` добавляет список источника (вшитые — из манифеста, системные — из основного потока; Chrome не сообщает вес — ядро угадывает его по имени начертания) и сразу запрашивает шрифты текущей страницы; `LoadFont` — байты одного файла; правки, вернувшие `TextPending` (`ReplaceMissingFont`, `TestCreateText`), ждут в очереди и повторяются по порядку после каждого изменения реестра. Запросы байтов, сообщения журнала, `MissingFonts` и `FontCatalog` уходят после команд, ввода и кадра — последние два только при изменении (пересчёт — при правке документа, смене страницы и поколения реестра, а не на каждом кадре). `DemoText { texts }` — страница из текстов для замера.

**Files:**
- Create: `crates/ferrum-wasm/src/core/fonts.rs`
- Modify: `crates/ferrum-wasm/Cargo.toml`, `src/protocol.rs` (константы 19–36, `Command` 180–369, `CoreEvent` 412–599), `src/core.rs` (`Core` 55–93, `new` 169–208, `replace_document` 240–262, `command` 512–563, `drain_editor` 582–652, `draw` 709–725, `draw_reference` 734–749), `src/core/panels.rs` (полный список команд ядра 466–483)
- Test: `crates/ferrum-wasm/tests/fonts.rs`

**Interfaces:**
- Consumes: `Editor::{set_fonts, create_text, replace_font, missing_fonts, page_fonts}`, `TextPending` (Task 6); `Fonts::{add_catalog, load, load_failed, take_requests, take_errors, families, set_in_use, resolve}` (Task 4); `fixture::fill_texts` (Task 2).
- Produces (протокол; TypeScript генерирует tsify): команды `fontsAvailable { source: 'bundled' | 'system'; families: FontFamilyDto[] }`, `loadFont { family; file; bytes?: Uint8Array }`, `replaceMissingFont { from: FontNameDto; to: FontNameDto }`, `testCreateText { x; y; text; font?; size?; sizing?: 'autoWidth' | 'autoHeight' | 'fixed'; width?; height? }`, `demoText { texts }`; события `fontRequest { family; source; files: string[] }`, `missingFonts { list: MissingFontDto[] }`, `fontCatalog { families: FontChoice[] }`, `log { message }`; типы `FontFamilyDto { family; faces: FontFaceDto[] }`, `FontFaceDto { style; weight?; italic?; stretch?; file; index? }`, `FontNameDto { family; style }`, `MissingFontDto { family; style; nodes }`, `FontChoice { family; source; styles }`; константы `MAX_TEXT_DATA_BYTES = 64 КБ`, `MAX_FONT_FAMILIES = 4096`, `MAX_FONT_FACES = 1000`, `MAX_DEMO_TEXTS = 100 000`; `Core::load_font_for_tests`.

- [ ] **Step 1: Падающие тесты ядра**

Создать `crates/ferrum-wasm/tests/fonts.rs`:

````rust
//! Шрифты и текст в ядре worker'а (подсистема 3, этап 1): список вшитых,
//! запрос и загрузка байтов, правка, ждущая шрифтов, отсутствующие шрифты и
//! замена одним шагом, пределы команд, пример из 10 000 текстов.
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use ferrum_editor::Platform;
use ferrum_render::CpuBackend;
use ferrum_text::read_faces;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const FILES: [&str; 4] = [
    "inter/Inter-Variable.ttf",
    "inter/Inter-Italic-Variable.ttf",
    "roboto/Roboto-Variable.ttf",
    "roboto/Roboto-Italic-Variable.ttf",
];

/// Манифест вшитых — как его отдаёт `font_manifest` (файлы относительно `fonts/`).
fn manifest() -> Vec<FontFamilyDto> {
    let faces = FILES
        .iter()
        .flat_map(|r| read_faces(&font_file(r), r))
        .collect::<Vec<_>>();
    ferrum_text::group(faces)
        .into_iter()
        .map(|f| FontFamilyDto {
            family: f.family,
            faces: f
                .faces
                .into_iter()
                .map(|x| FontFaceDto {
                    style: x.style,
                    weight: Some(x.weight as f64),
                    italic: Some(x.italic),
                    stretch: Some(x.stretch as f64),
                    file: x.file,
                    index: Some(x.index),
                })
                .collect(),
        })
        .collect()
}

fn core() -> Core {
    let mut c = Core::new(Platform::Mac, Size::new(400.0, 300.0), 1.0, Backend::Cpu);
    c.command(Command::Demo { nodes: 0 });
    c.command(Command::FontsAvailable {
        source: FontSource::Bundled,
        families: manifest(),
    });
    c.take_events();
    c
}

/// Отвечает на все запросы шрифтов из событий (как worker ядра по манифесту).
fn serve(c: &mut Core, events: &[CoreEvent]) -> usize {
    let mut n = 0;
    for e in events {
        if let CoreEvent::FontRequest {
            family,
            source,
            files,
        } = e
        {
            assert_eq!(*source, FontSource::Bundled);
            for f in files {
                c.load_font_for_tests(family, f, font_file(f));
                n += 1;
            }
        }
    }
    n
}

fn create(c: &mut Core, text: &str, font: Option<(&str, &str)>) {
    c.command(Command::TestCreateText {
        x: 20.0,
        y: 20.0,
        text: text.into(),
        font: font.map(|(f, s)| FontNameDto {
            family: f.into(),
            style: s.into(),
        }),
        size: Some(32.0),
        sizing: None,
        width: None,
        height: None,
    });
}

fn nodes(c: &Core) -> usize {
    c.editor().scene().len()
}

#[test]
fn text_waits_for_requested_fonts_then_appears_and_draws() {
    let mut c = core();
    create(&mut c, "Ferrum", None);
    assert_eq!(nodes(&c), 0, "шрифт ещё не загружен");
    let events = c.take_events();
    assert_eq!(serve(&mut c, &events), 2);
    assert_eq!(nodes(&c), 1);
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::Document { can_undo: true, .. })));
    // Текст на холсте: тёмные пиксели в его рамке (камера примера — зум 0.5).
    let (w, h) = c.device_size();
    let mut b = CpuBackend::new(w, h);
    c.draw(&mut b);
    let px = b.render();
    let dark = px.data().iter().filter(|p| p.r < 128 && p.a > 200).count();
    assert!(dark > 20, "{dark}");
}

#[test]
fn missing_fonts_are_reported_and_replaced_in_one_step() {
    let mut c = core();
    create(&mut c, "Ferrum", None);
    let events = c.take_events();
    serve(&mut c, &events);
    create(&mut c, "Comic", Some(("Comic Sans", "Bold")));
    let events = c.take_events();
    let missing = events.iter().rev().find_map(|e| match e {
        CoreEvent::MissingFonts { list } => Some(list.clone()),
        _ => None,
    });
    assert_eq!(
        missing,
        Some(vec![MissingFontDto {
            family: "Comic Sans".into(),
            style: "Bold".into(),
            nodes: 1,
        }])
    );
    c.command(Command::ReplaceMissingFont {
        from: FontNameDto {
            family: "Comic Sans".into(),
            style: "Bold".into(),
        },
        to: FontNameDto {
            family: "Roboto".into(),
            style: "Bold".into(),
        },
    });
    // Roboto ещё не загружен: замена ждёт его, запросив байты.
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::FontRequest { family, .. } if family == "Roboto")));
    serve(&mut c, &events);
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::MissingFonts { list } if list.is_empty())));
    c.command(Command::Execute {
        command: "edit.undo".into(),
    });
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::MissingFonts { list } if list.len() == 1)));
}

#[test]
fn catalog_and_logs_and_limits() {
    let mut c = Core::new(Platform::Mac, Size::new(400.0, 300.0), 1.0, Backend::Cpu);
    c.command(Command::FontsAvailable {
        source: FontSource::Bundled,
        families: manifest(),
    });
    let events = c.take_events();
    let catalog = events.iter().find_map(|e| match e {
        CoreEvent::FontCatalog { families } => Some(families.clone()),
        _ => None,
    });
    let catalog = catalog.expect("каталог");
    assert!(catalog
        .iter()
        .any(|f| f.family == "Inter" && f.styles.contains(&"Bold".to_string())));
    assert!(catalog.iter().any(|f| f.family == "Onest"));
    // Неразборчивые байты — сообщение в журнал, не уведомление.
    c.command(Command::LoadFont {
        family: "Inter".into(),
        file: FILES[0].into(),
        bytes: Some(ByteBuf::from(b"garbage".to_vec())),
    });
    let events = c.take_events();
    assert!(events.iter().any(|e| matches!(e, CoreEvent::Log { .. })));
    assert!(!events.iter().any(|e| matches!(e, CoreEvent::Error { .. })));
    // Пределы: длинное имя семейства, длинный текст.
    c.command(Command::FontsAvailable {
        source: FontSource::System,
        families: vec![FontFamilyDto {
            family: "x".repeat(300),
            faces: vec![],
        }],
    });
    c.command(Command::TestCreateText {
        x: 0.0,
        y: 0.0,
        text: "я".repeat(40_000),
        font: None,
        size: None,
        sizing: None,
        width: None,
        height: None,
    });
    let errors = c
        .take_events()
        .into_iter()
        .filter(|e| matches!(e, CoreEvent::Error { .. }))
        .count();
    assert_eq!(errors, 2);
}

#[test]
fn demo_with_ten_thousand_texts_opens_without_layout() {
    let mut c = core();
    let t = std::time::Instant::now();
    c.command(Command::DemoText { texts: 10_000 });
    let elapsed = t.elapsed();
    let events = c.take_events();
    let opened = events.iter().find_map(|e| match e {
        CoreEvent::Opened { nodes, .. } => Some(*nodes),
        _ => None,
    });
    assert_eq!(opened, Some(10_000 + 100));
    // Шрифты страницы запрошены сразу (а не при первом рисовании), и один раз.
    let requests = events
        .iter()
        .filter(|e| matches!(e, CoreEvent::FontRequest { family, .. } if family == "Inter"))
        .count();
    assert_eq!(requests, 1);
    eprintln!("пример из 10 000 текстов: {elapsed:?}");
}
````

Run: `cargo test -p ferrum-wasm --test fonts`
Expected: FAIL — не собирается: нет `Command::FontsAvailable`, `FontSource`, `Core::load_font_for_tests`.

- [ ] **Step 2: Протокол**

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
/// на экранах с плотностью 2 оно оставалось чётким.
pub const THUMB_WIDTH: u16 = 480;
pub const THUMB_HEIGHT: u16 = 320;
/// Наибольшее число пачек журнала изменений при открытии (этап 6): пачка пишется
/// не чаще раза в 2 с, а полный файл — хотя бы раз в 5 минут правок, так что столько
/// набирается, только если полное сохранение много часов не удаётся.
````

на:

````rust
/// на экранах с плотностью 2 оно оставалось чётким.
pub const THUMB_WIDTH: u16 = 480;
pub const THUMB_HEIGHT: u16 = 320;
/// Наибольший текст тестовой команды `TestCreateText` (спека подсистемы 3, раздел 8:
/// `data` ≤ 64 КБ за команду).
pub const MAX_TEXT_DATA_BYTES: usize = 64 * 1024;
/// Наибольшее число семейств в `FontsAvailable` и начертаний у семейства.
pub const MAX_FONT_FAMILIES: usize = 4096;
pub const MAX_FONT_FACES: usize = 1000;
/// Наибольший пример с текстами (`Demo { texts }`).
pub const MAX_DEMO_TEXTS: u32 = 100_000;
/// Наибольшее число пачек журнала изменений при открытии (этап 6): пачка пишется
/// не чаще раза в 2 с, а полный файл — хотя бы раз в 5 минут правок, так что столько
/// набирается, только если полное сохранение много часов не удаётся.
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    Cancel,
}

/// Правка выделения из панели свойств.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
````

на:

````rust
    Cancel,
}

/// Источник шрифтов: вшитые в сборку или системные.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum FontSource {
    Bundled,
    System,
}

/// Шрифт документа: семейство и начертание.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontNameDto {
    pub family: String,
    pub style: String,
}

/// Начертание из списка шрифтов: имя, вес, курсив, ширина (у системных шрифтов
/// Chrome их нет — ядро угадывает по имени) и файл (имя в манифесте вшитых или
/// postscript-имя системного) с номером шрифта в коллекции.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontFaceDto {
    pub style: String,
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub italic: Option<bool>,
    #[serde(default)]
    pub stretch: Option<f64>,
    pub file: String,
    #[serde(default)]
    pub index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontFamilyDto {
    pub family: String,
    pub faces: Vec<FontFaceDto>,
}

/// Отсутствующий шрифт текущей страницы и сколько текстов им набрано.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MissingFontDto {
    pub family: String,
    pub style: String,
    pub nodes: u32,
}

/// Семейство для выбора замены: источник и начертания.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct FontChoice {
    pub family: String,
    pub source: FontSource,
    pub styles: Vec<String>,
}

/// Режим размера текста тестовой команды.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum TextSizingDto {
    AutoWidth,
    AutoHeight,
    Fixed,
}

/// Правка выделения из панели свойств.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    /// Новый документ — страница-пример из `nodes` узлов (фреймы по 100 узлов).
    Demo {
        nodes: u32,
    },
    /// Открыть файл и применить поверх него пачки журнала изменений, записанные
    /// после него, по порядку записи (восстановление после сбоя, этап 6).
````

на:

````rust
    /// Новый документ — страница-пример из `nodes` узлов (фреймы по 100 узлов).
    Demo {
        nodes: u32,
    },
    /// Новый документ — страница-пример из `texts` текстовых узлов (фреймы по 100
    /// текстов фиксированного размера): замер «10 000 текстов» (спека подсистемы 3,
    /// раздел 5).
    DemoText {
        texts: u32,
    },
    /// Открыть файл и применить поверх него пачки журнала изменений, записанные
    /// после него, по порядку записи (восстановление после сбоя, этап 6).
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    },
    SetSettings {
        pixel_snap: bool,
    },
}

````

на:

````rust
    },
    SetSettings {
        pixel_snap: bool,
    },
    // ——— Подсистема 3, этап 1: шрифты ———
    /// Список шрифтов источника: вшитые (манифест сборки — его читает worker ядра)
    /// или системные (Chrome — `queryLocalFonts`, десктоп — сканирование каталогов).
    FontsAvailable {
        source: FontSource,
        families: Vec<FontFamilyDto>,
    },
    /// Байты файла шрифта в ответ на `FontRequest`; без байтов — файл получить не
    /// удалось (шрифт считается отсутствующим).
    LoadFont {
        family: String,
        file: String,
        #[serde(default)]
        #[tsify(type = "Uint8Array | undefined")]
        bytes: Option<ByteBuf>,
    },
    /// Заменить шрифт во всех текстах текущей страницы одним шагом отмены.
    ReplaceMissingFont {
        from: FontNameDto,
        to: FontNameDto,
    },
    /// Только тестовый режим (`init.test`; worker отвергает её без него): текстовый
    /// узел в точке мира — инструмента T до этапа 2 нет. Ждёт шрифтов, если они
    /// ещё грузятся.
    TestCreateText {
        x: f64,
        y: f64,
        text: String,
        #[serde(default)]
        font: Option<FontNameDto>,
        #[serde(default)]
        size: Option<f64>,
        #[serde(default)]
        sizing: Option<TextSizingDto>,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
    },
}

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    Modified {
        modified: bool,
    },
}
````

на:

````rust
    Modified {
        modified: bool,
    },
    // ——— Подсистема 3, этап 1: шрифты ———
    /// Ядру нужны байты файлов семейства: вшитые качает worker ядра, системные —
    /// основной поток (`LoadFont` на каждый файл).
    FontRequest {
        family: String,
        source: FontSource,
        files: Vec<String>,
    },
    /// Отсутствующие шрифты текущей страницы; приходит, когда список сменился.
    MissingFonts {
        list: Vec<MissingFontDto>,
    },
    /// Известные семейства для выбора замены; приходит, когда список сменился.
    FontCatalog {
        families: Vec<FontChoice>,
    },
    /// Сообщение для журнала ошибок (консоль), не для уведомления: неразборчивый
    /// файл шрифта, пропущенное семейство.
    Log {
        message: String,
    },
}
````

- [ ] **Step 3: Шрифты в ядре**

В `crates/ferrum-wasm/Cargo.toml` заменить:

````toml
ferrum-editor = { path = "../ferrum-editor" }
ferrum-render = { path = "../ferrum-render" }
ferrum-scene = { path = "../ferrum-scene" }
kurbo.workspace = true
serde.workspace = true
serde_bytes.workspace = true
````

на:

````toml
ferrum-editor = { path = "../ferrum-editor" }
ferrum-render = { path = "../ferrum-render" }
ferrum-scene = { path = "../ferrum-scene" }
ferrum-text = { path = "../ferrum-text" }
kurbo.workspace = true
serde.workspace = true
serde_bytes.workspace = true
````

Создать `crates/ferrum-wasm/src/core/fonts.rs`:

````rust
//! Шрифты и текст в ядре worker'а (подсистема 3, этап 1): списки шрифтов
//! источников, запросы и загрузка байтов, отсутствующие шрифты страницы и их
//! замена, тестовое создание текста. Правки текста, которым нужны ещё не
//! загруженные шрифты, ждут в очереди и выполняются, когда байты придут.
//!
//! Всё, что приходит из основного потока, не доверенное: имена и списки
//! ограничены (спека 8), неверное — `Error`, сбой разбора шрифта — `Log`.

use std::collections::VecDeque;

use ferrum_doc::{FontName, LetterSpacing, LineHeight, TextContent, TextProps, TextSizing};
use ferrum_editor::TextPending;
use ferrum_text::{FaceInfo, FamilyInfo, SourceKind, MAX_FAMILY_NAME, MAX_FONT_FILE};
use kurbo::Point;
use serde_bytes::ByteBuf;

use super::Core;
use crate::protocol::{
    Command, CoreEvent, FontChoice, FontFamilyDto, FontNameDto, FontSource, MissingFontDto,
    TextSizingDto, MAX_FONT_FACES, MAX_FONT_FAMILIES, MAX_TEXT_BYTES, MAX_TEXT_DATA_BYTES,
};

/// Что ядро уже сообщило об отсутствующих шрифтах и каталоге.
#[derive(Default)]
pub(crate) struct FontState {
    /// Правки, ждущие шрифтов, по порядку прихода.
    pub waiting: VecDeque<Command>,
    pub missing_sent: Option<Vec<MissingFontDto>>,
    pub catalog_sent: Option<Vec<FontChoice>>,
    /// Поколение реестра, при котором посчитаны отсутствующие (`None` — пересчитать).
    pub generation: Option<u64>,
}

fn source(s: FontSource) -> SourceKind {
    match s {
        FontSource::Bundled => SourceKind::Bundled,
        FontSource::System => SourceKind::System,
    }
}

fn source_dto(s: SourceKind) -> FontSource {
    match s {
        SourceKind::System => FontSource::System,
        // Вшитый в wasm Onest для интерфейса — тоже «вшитый».
        SourceKind::Bundled | SourceKind::Embedded => FontSource::Bundled,
    }
}

fn name_ok(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max
}

fn font_name(f: &FontNameDto) -> Result<FontName, String> {
    if !name_ok(&f.family, MAX_FAMILY_NAME) || !name_ok(&f.style, MAX_FAMILY_NAME) {
        return Err("Неверное имя шрифта".into());
    }
    Ok(FontName::new(f.family.clone(), f.style.clone()))
}

/// Семейства из команды: имена и списки в пределах, у начертаний без веса —
/// вес и курсив по имени (Chrome не сообщает их в `FontData`).
fn families(list: Vec<FontFamilyDto>) -> Result<Vec<FamilyInfo>, String> {
    if list.len() > MAX_FONT_FAMILIES {
        return Err(format!("Шрифтов больше {MAX_FONT_FAMILIES} семейств"));
    }
    list.into_iter()
        .map(|f| {
            if !name_ok(&f.family, MAX_FAMILY_NAME) || f.faces.len() > MAX_FONT_FACES {
                return Err("Неверный список шрифтов".to_string());
            }
            let faces = f
                .faces
                .into_iter()
                .map(|x| {
                    if !name_ok(&x.style, MAX_FAMILY_NAME) || !name_ok(&x.file, MAX_TEXT_BYTES) {
                        return Err("Неверный список шрифтов".to_string());
                    }
                    let (guess_w, guess_i) = ferrum_text::style_name::guess_attrs(&x.style);
                    let finite = |v: Option<f64>| v.filter(|v| v.is_finite());
                    Ok(FaceInfo {
                        weight: finite(x.weight).map_or(guess_w, |v| v.clamp(1.0, 1000.0) as f32),
                        italic: x.italic.unwrap_or(guess_i),
                        stretch: finite(x.stretch).map_or(1.0, |v| v.clamp(0.5, 2.0) as f32),
                        index: x.index.unwrap_or(0),
                        style: x.style,
                        file: x.file,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FamilyInfo {
                family: f.family,
                faces,
            })
        })
        .collect()
}

impl Core {
    /// Команды шрифтов и текста; остальные — не сюда.
    pub(crate) fn font_command(&mut self, cmd: Command) {
        match cmd {
            Command::FontsAvailable {
                source: s,
                families: list,
            } => match families(list) {
                Ok(list) => {
                    self.fonts.borrow_mut().add_catalog(source(s), list);
                    // Шрифты страницы, которые до списка были «неизвестны», — запросить.
                    self.preload_page_fonts();
                    self.after_fonts_changed();
                }
                Err(e) => self.error(e),
            },
            Command::LoadFont {
                family,
                file,
                bytes,
            } => {
                if !name_ok(&family, MAX_FAMILY_NAME) || !name_ok(&file, MAX_TEXT_BYTES) {
                    self.error("Неверная команда загрузки шрифта");
                    return;
                }
                let result = match bytes {
                    Some(b) if b.len() <= MAX_FONT_FILE => {
                        self.fonts.borrow_mut().load(&family, &file, b.into_vec())
                    }
                    Some(_) => {
                        self.fonts.borrow_mut().load_failed(&family, &file);
                        Err(format!("Шрифт «{family}» не загружен: файл больше 64 МБ"))
                    }
                    None => {
                        self.fonts.borrow_mut().load_failed(&family, &file);
                        Err(format!("Шрифт «{family}» не загружен: файл недоступен"))
                    }
                };
                if let Err(message) = result {
                    self.events.push(CoreEvent::Log { message });
                }
                self.after_fonts_changed();
            }
            cmd @ (Command::ReplaceMissingFont { .. } | Command::TestCreateText { .. }) => {
                self.text_command(cmd)
            }
            _ => {}
        }
    }

    /// Правка текста; ждущая шрифтов — в очередь (повтор — `after_fonts_changed`).
    fn text_command(&mut self, cmd: Command) {
        let result = match &cmd {
            Command::ReplaceMissingFont { from, to } => match (font_name(from), font_name(to)) {
                (Ok(from), Ok(to)) => self.editor.replace_font(&from, &to).map(|_| ()),
                (Err(e), _) | (_, Err(e)) => {
                    self.error(e);
                    return;
                }
            },
            Command::TestCreateText {
                x,
                y,
                text,
                font,
                size,
                sizing,
                width,
                height,
            } => {
                if text.len() > MAX_TEXT_DATA_BYTES {
                    self.error("Текст длиннее 64 КБ");
                    return;
                }
                let mut props = TextProps::default();
                if let Some(f) = font {
                    match font_name(f) {
                        Ok(f) => props.style.font = f,
                        Err(e) => {
                            self.error(e);
                            return;
                        }
                    }
                }
                if let Some(s) = size.filter(|s| s.is_finite() && *s > 0.0) {
                    props.style.size = s.min(10_000.0);
                }
                props.style.line_height = LineHeight::Auto;
                props.style.letter_spacing = LetterSpacing::Percent(0.0);
                props.sizing = match sizing.unwrap_or(TextSizingDto::AutoWidth) {
                    TextSizingDto::AutoWidth => TextSizing::AutoWidth,
                    TextSizingDto::AutoHeight => TextSizing::AutoHeight,
                    TextSizingDto::Fixed => TextSizing::Fixed,
                };
                let finite = |v: &Option<f64>| v.filter(|v| v.is_finite()).unwrap_or(0.0);
                if !(x.is_finite() && y.is_finite()) {
                    self.error("Неверная точка текста");
                    return;
                }
                self.editor
                    .create_text(
                        Point::new(*x, *y),
                        &TextContent::plain(text),
                        &props,
                        finite(width),
                        finite(height),
                    )
                    .map(|_| ())
            }
            _ => return,
        };
        if let Err(TextPending) = result {
            self.font_state.waiting.push_back(cmd);
        }
        self.push_font_requests();
    }

    /// Реестр изменился (пришёл список, загружен файл): кадр заново, правки,
    /// ждавшие шрифтов, — ещё раз по порядку, списки для интерфейса — если сменились.
    fn after_fonts_changed(&mut self) {
        self.dirty = true;
        let waiting: Vec<Command> = self.font_state.waiting.drain(..).collect();
        for cmd in waiting {
            self.text_command(cmd);
        }
        self.font_state.generation = None;
        self.push_font_requests();
    }

    /// Шрифты текущей страницы: на ней — не вытеснять, незагруженные — запросить
    /// сразу (а не при первом рисовании). Вызывается при смене страницы и документа.
    pub(crate) fn preload_page_fonts(&mut self) {
        let fonts = self.editor.page_fonts();
        {
            let mut reg = self.fonts.borrow_mut();
            reg.set_in_use(fonts.iter().map(|f| f.family.clone()));
            for f in &fonts {
                reg.resolve(f);
            }
        }
        self.font_state.generation = None;
        self.push_font_requests();
    }

    /// Запросы байтов и сообщения реестра — событиями; отсутствующие шрифты и
    /// каталог — если изменились.
    pub(crate) fn push_font_requests(&mut self) {
        let (requests, errors) = {
            let mut reg = self.fonts.borrow_mut();
            (reg.take_requests(), reg.take_errors())
        };
        for r in requests {
            self.events.push(CoreEvent::FontRequest {
                family: r.family,
                source: source_dto(r.source),
                files: r.files,
            });
        }
        for message in errors {
            self.events.push(CoreEvent::Log { message });
        }
        self.push_missing_fonts();
    }

    /// `MissingFonts` и `FontCatalog`, если сменились. Пересчёт — при правке
    /// документа, смене страницы и изменении реестра, а не на каждом кадре.
    pub(crate) fn push_missing_fonts(&mut self) {
        let generation = self.fonts.borrow().generation();
        if self.font_state.generation == Some(generation) {
            return;
        }
        self.font_state.generation = Some(generation);
        let list: Vec<MissingFontDto> = self
            .editor
            .missing_fonts()
            .into_iter()
            .map(|m| MissingFontDto {
                family: m.font.family,
                style: m.font.style,
                nodes: m.nodes as u32,
            })
            .collect();
        if self.font_state.missing_sent.as_ref() != Some(&list) {
            self.font_state.missing_sent = Some(list.clone());
            self.events.push(CoreEvent::MissingFonts { list });
        }
        let catalog: Vec<FontChoice> = self
            .fonts
            .borrow()
            .families()
            .into_iter()
            .map(|(family, s, styles)| FontChoice {
                family,
                source: source_dto(s),
                styles,
            })
            .collect();
        if self.font_state.catalog_sent.as_ref() != Some(&catalog) {
            self.font_state.catalog_sent = Some(catalog.clone());
            self.events
                .push(CoreEvent::FontCatalog { families: catalog });
        }
    }

    /// Документ правился: отсутствующие шрифты пересчитать при следующей пачке.
    pub(crate) fn fonts_document_changed(&mut self) {
        self.font_state.generation = None;
    }

    /// Байты для `LoadFont` тестов: `ByteBuf` из вектора.
    #[doc(hidden)]
    pub fn load_font_for_tests(&mut self, family: &str, file: &str, bytes: Vec<u8>) {
        self.command(Command::LoadFont {
            family: family.into(),
            file: file.into(),
            bytes: Some(ByteBuf::from(bytes)),
        });
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
//! (`FrameStats::needs_another_frame`: растры фреймов не догнали, картинки ещё
//! декодируются).

mod panels;

use std::collections::{HashSet, VecDeque};
````

на:

````rust
//! (`FrameStats::needs_another_frame`: растры фреймов не догнали, картинки ещё
//! декодируются).

mod fonts;
mod panels;

use std::collections::{HashSet, VecDeque};
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
use ferrum_editor::{CommandId, Cursor, Editor, EditorEvent, Input, Platform, ToolKind};
use ferrum_render::{decode_png, FrameStats, OverlayTheme, RenderBackend, Renderer, Rgba};
use ferrum_scene::Scene;
use kurbo::{Point, Size};
use serde_bytes::ByteBuf;

````

на:

````rust
use ferrum_editor::{CommandId, Cursor, Editor, EditorEvent, Input, Platform, ToolKind};
use ferrum_render::{decode_png, FrameStats, OverlayTheme, RenderBackend, Renderer, Rgba};
use ferrum_scene::Scene;
use ferrum_text::{Fonts, SharedFonts};
use kurbo::{Point, Size};
use serde_bytes::ByteBuf;

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
use crate::layers::Layers;
use crate::live::LiveState;
use crate::protocol::{
    Backend, Command, CoreEvent, CursorKind, Theme, Tool, MAX_DEMO_NODES, MAX_JOURNAL_BATCHES,
    SELECTION_IDS_MAX, THUMB_HEIGHT, THUMB_WIDTH,
};
use crate::summary::Summary;

````

на:

````rust
use crate::layers::Layers;
use crate::live::LiveState;
use crate::protocol::{
    Backend, Command, CoreEvent, CursorKind, Theme, Tool, MAX_DEMO_NODES, MAX_DEMO_TEXTS,
    MAX_JOURNAL_BATCHES, SELECTION_IDS_MAX, THUMB_HEIGHT, THUMB_WIDTH,
};
use crate::summary::Summary;

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    revision: u64,
    /// Номер последней непустой пачки журнала изменений (этап 6).
    journal_batch: u64,
}

/// Что из панелей пересчитать после пачки событий редактора.
````

на:

````rust
    revision: u64,
    /// Номер последней непустой пачки журнала изменений (этап 6).
    journal_batch: u64,
    /// Реестр шрифтов (подсистема 3): общий у редактора и рендера, переживает
    /// замену документа — загруженные шрифты не качаются заново.
    fonts: SharedFonts,
    font_state: fonts::FontState,
}

/// Что из панелей пересчитать после пачки событий редактора.
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
impl Core {
    /// Ядро с пустым документом для холста `viewport` (CSS-пиксели) и `dpr`.
    pub fn new(platform: Platform, viewport: Size, dpr: f64, backend: Backend) -> Core {
        let editor = Editor::new(Document::new(), None, viewport, dpr, platform)
            .expect("в новом документе есть страница");
        let mut core = Core {
            editor,
            renderer: Renderer::new(),
            images: DocImages::default(),
            backend,
            platform,
````

на:

````rust
impl Core {
    /// Ядро с пустым документом для холста `viewport` (CSS-пиксели) и `dpr`.
    pub fn new(platform: Platform, viewport: Size, dpr: f64, backend: Backend) -> Core {
        let mut editor = Editor::new(Document::new(), None, viewport, dpr, platform)
            .expect("в новом документе есть страница");
        let fonts = Fonts::shared();
        editor.set_fonts(fonts.clone());
        let mut renderer = Renderer::new();
        renderer.set_fonts(fonts.clone());
        let mut core = Core {
            editor,
            renderer,
            images: DocImages::default(),
            backend,
            platform,
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            modified_sent: Some(false),
            revision: 0,
            journal_batch: 0,
        };
        // Таблица команд и пресеты нужны интерфейсу сразу, до документа.
        core.push_keymap();
````

на:

````rust
            modified_sent: Some(false),
            revision: 0,
            journal_batch: 0,
            fonts,
            font_state: fonts::FontState::default(),
        };
        // Таблица команд и пресеты нужны интерфейсу сразу, до документа.
        core.push_keymap();
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            Editor::new(doc, None, viewport, dpr, self.platform).map_err(|e| e.to_string())?;
        editor.set_theme(overlay_theme(self.theme));
        editor.set_settings(self.editor.settings().clone());
        *editor.keymap_mut() = self.editor.keymap().clone();
        editor.take_events();
        self.editor = editor;
````

на:

````rust
            Editor::new(doc, None, viewport, dpr, self.platform).map_err(|e| e.to_string())?;
        editor.set_theme(overlay_theme(self.theme));
        editor.set_settings(self.editor.settings().clone());
        editor.set_fonts(self.fonts.clone());
        *editor.keymap_mut() = self.editor.keymap().clone();
        editor.take_events();
        self.editor = editor;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        self.modified_sent = None;
        self.revision += 1;
        self.dirty = true;
        Ok(())
    }

````

на:

````rust
        self.modified_sent = None;
        self.revision += 1;
        self.dirty = true;
        // Списки шрифтов — для нового документа; его шрифты — запросить сразу.
        self.font_state.missing_sent = None;
        self.preload_page_fonts();
        Ok(())
    }

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        self.push_opened(vec![]);
    }

    /// Разбирает записи кольцевого буфера и отдаёт события редактору по порядку.
    /// Возвращает число принятых записей.
    pub fn apply_input(&mut self, bytes: &[u8]) -> usize {
````

на:

````rust
        self.push_opened(vec![]);
    }

    /// Новый документ — страница-пример из `texts` текстов (фреймы по 100):
    /// замер открытия и панорамирования «10 000 текстов» (спека подсистемы 3, 5).
    pub fn demo_text(&mut self, texts: u32) {
        if texts > MAX_DEMO_TEXTS {
            self.error(format!("Пример не больше {MAX_DEMO_TEXTS} текстов"));
            return;
        }
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        if let Err(e) = doc.transact(page, vec![], |p| {
            fixture::fill_texts(p, texts as usize)?;
            Ok(vec![])
        }) {
            self.error(e.to_string());
            return;
        }
        doc.clear_history();
        if let Err(e) = self.replace_document(doc) {
            self.error(e);
            return;
        }
        self.editor.mark_journal();
        let mut cam = self.editor.camera().clone();
        cam.set_zoom(0.5);
        cam.set_center(Point::new(720.0, 512.0));
        self.editor.set_camera(cam);
        self.editor.take_events();
        self.push_opened(vec![]);
    }

    /// Разбирает записи кольцевого буфера и отдаёт события редактору по порядку.
    /// Возвращает число принятых записей.
    pub fn apply_input(&mut self, bytes: &[u8]) -> usize {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        match cmd {
            Command::Open { bytes } => self.open(&bytes),
            Command::Demo { nodes } => self.demo(nodes),
            Command::OpenWithJournal { bytes, journal } => self.open_with_journal(&bytes, &journal),
            Command::FlushJournal => self.flush_journal(true),
            Command::Execute { command } => match CommandId::from_key(&command) {
````

на:

````rust
        match cmd {
            Command::Open { bytes } => self.open(&bytes),
            Command::Demo { nodes } => self.demo(nodes),
            Command::DemoText { texts } => self.demo_text(texts),
            cmd @ (Command::FontsAvailable { .. }
            | Command::LoadFont { .. }
            | Command::ReplaceMissingFont { .. }
            | Command::TestCreateText { .. }) => self.font_command(cmd),
            Command::OpenWithJournal { bytes, journal } => self.open_with_journal(&bytes, &journal),
            Command::FlushJournal => self.flush_journal(true),
            Command::Execute { command } => match CommandId::from_key(&command) {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                EditorEvent::NeedsRedraw => self.dirty = true,
                EditorEvent::DocumentChanged => {
                    document = true;
                    self.touch();
                    self.ui.layers = true;
                    self.ui.properties = true;
````

на:

````rust
                EditorEvent::NeedsRedraw => self.dirty = true,
                EditorEvent::DocumentChanged => {
                    document = true;
                    self.fonts_document_changed();
                    self.touch();
                    self.ui.layers = true;
                    self.ui.properties = true;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                }
                EditorEvent::PageChanged(p) => {
                    self.events.push(CoreEvent::Page { id: p.to_string() });
                    self.background_preview = None;
                    self.layers.reset();
                    self.ui.layers = true;
````

на:

````rust
                }
                EditorEvent::PageChanged(p) => {
                    self.events.push(CoreEvent::Page { id: p.to_string() });
                    self.preload_page_fonts();
                    self.background_preview = None;
                    self.layers.reset();
                    self.ui.layers = true;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        }
        self.flush_ui();
        self.push_modified();
    }

    fn copy_png(&mut self, ids: &[NodeId]) {
````

на:

````rust
        }
        self.flush_ui();
        self.push_modified();
        self.push_font_requests();
    }

    fn copy_png(&mut self, ids: &[NodeId]) {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        self.update_selection(&overlay.selection);
        self.dirty = stats.needs_another_frame;
        self.frames += 1;
        stats
    }

````

на:

````rust
        self.update_selection(&overlay.selection);
        self.dirty = stats.needs_another_frame;
        self.frames += 1;
        // Раскладка при рисовании запрашивает незагруженные шрифты.
        self.push_font_requests();
        stats
    }

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        let mut backend = ferrum_render::CpuBackend::new(w, h);
        let mut renderer = Renderer::with_raster_cache_bytes(0);
        renderer.set_raster_build_time(None);
        let provider = self.images.export_provider(&self.editor.document().images);
        renderer.render(
            &mut backend,
````

на:

````rust
        let mut backend = ferrum_render::CpuBackend::new(w, h);
        let mut renderer = Renderer::with_raster_cache_bytes(0);
        renderer.set_raster_build_time(None);
        renderer.set_fonts(self.fonts.clone());
        let provider = self.images.export_provider(&self.editor.document().images);
        renderer.render(
            &mut backend,
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            | Command::Snapshot { .. }
            | Command::MarkSaved { .. }
            | Command::ReadPixels { .. }
            | Command::ReadMeasurements => {
                debug_assert!(false, "команда этапа 4 — в `Core::command`");
            }
        }
    }
````

на:

````rust
            | Command::Snapshot { .. }
            | Command::MarkSaved { .. }
            | Command::ReadPixels { .. }
            | Command::ReadMeasurements
            | Command::DemoText { .. }
            | Command::FontsAvailable { .. }
            | Command::LoadFont { .. }
            | Command::ReplaceMissingFont { .. }
            | Command::TestCreateText { .. } => {
                debug_assert!(false, "команда ядра — в `Core::command`");
            }
        }
    }
````

- [ ] **Step 4: Проверить**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings && cargo test -p ferrum-wasm`
Expected: PASS: `fonts` — 4 теста (текст ждёт два запрошенных файла Inter, затем появляется и рисуется; отсутствующий шрифт в событии, замена ждёт Roboto и одним шагом опустошает список, отмена возвращает; каталог с Inter и Onest, неразборчивые байты — `Log` без `Error`, пределы имени и текста — две `Error`; пример из 10 000 текстов — 10 100 узлов и один запрос Inter); остальные наборы — как прежде.

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-wasm Cargo.lock
git commit -m "feat(wasm): шрифты в ядре — списки источников, запросы и загрузка байтов, правки, ждущие шрифтов, отсутствующие шрифты и каталог для замены; пример из текстов

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Worker ядра: манифест и вшитые шрифты, тестовая команда текста

Спека 4: «worker ядра запрашивает файл семейства при первом использовании; кэш — HTTP-кэш браузера и память ядра». После запуска ядра worker читает `/fonts/manifest.json` (недоступен — вшитых нет, предупреждение в консоли) и шлёт `fontsAvailable { source: 'bundled' }`; запросы вшитых (`fontRequest` с `source: 'bundled'`) он выполняет сам — `fetch` файла из `/fonts/` и `loadFont` (неудача — `loadFont` без байтов), в основной поток такие события не уходят; `log` — в консоль worker'а. Путь файла из манифеста проверяется (`isFontPath`: относительный, латиница, без `..`). `testCreateText` без тестового режима страницы — «Неверное сообщение для ядра», как `testPanic`. `loadFont` проверяется как остальные команды с байтами (≤ 64 МБ, `Uint8Array` не в общей памяти) и передаёт буфер. `?texts=N` у стенда и нового файла — страница из N текстов.

**Files:**
- Create: `apps/web/src/core/fonts.ts`
- Modify: `apps/web/src/core/limits.ts`, `src/core/messages.ts` (`MAX_COMMAND_BYTES` 43–50, `commandSizeError` 61–72, `prepareCommand` 132–145), `src/worker/core.worker.ts` (`sendEvents` 175–194, `handle` 251–324, запуск 387–388), `tsconfig.worker.json`, `src/debug.ts:51`, `src/ui/boot.ts:186`
- Test: `apps/web/unit/fonts.test.ts` (новый), `apps/web/unit/limits.test.ts`, `apps/web/e2e/text.spec.ts` (новый)

**Interfaces:**
- Consumes: протокол Task 7 (типы `FontFamilyDto`, команды `fontsAvailable`, `loadFont`, `testCreateText`, `demoText`, события `fontRequest`, `log`).
- Produces: `src/core/limits.ts`: `MAX_FONT_BYTES`, `MAX_FAMILY_NAME`, `MAX_FONT_FAMILIES`, `MAX_FONT_FACES`; `src/core/fonts.ts`: `isFontPath`, `manifestFamilies(json) -> FontFamilyDto[]`, `LocalFont`, `localFamilies(fonts) -> FontFamilyDto[]`; `MAX_COMMAND_BYTES.loadFont`; адреса `/fonts/manifest.json` и `/fonts/<file>`.

- [ ] **Step 1: Падающие модульные тесты и E2E**

Создать `apps/web/unit/fonts.test.ts`:

````ts
// Шрифты на стороне TypeScript (подсистема 3): манифест вшитых, системные шрифты
// Chrome в семейства, адрес файла.

import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { isFontPath, localFamilies, manifestFamilies } from '../src/core/fonts';

test('манифест сборки читается целиком, мусор пропускается', () => {
  const manifest = JSON.parse(readFileSync(new URL('../public/fonts/manifest.json', import.meta.url), 'utf8'));
  const families = manifestFamilies(manifest);
  expect(families.map((f) => f.family)).toEqual(
    ['Inter', 'JetBrains Mono', 'Montserrat', 'Noto Sans', 'Open Sans', 'PT Sans', 'PT Serif', 'Playfair Display', 'Roboto'],
  );
  const inter = families.find((f) => f.family === 'Inter')!;
  expect(inter.faces.find((f) => f.style === 'Bold')).toMatchObject({ weight: 700, italic: false, file: 'inter/Inter-Variable.ttf' });
  expect(manifestFamilies({ families: [{ family: 'X', faces: [{ style: 'R', file: '../../etc/passwd.ttf' }] }] })).toEqual([]);
  expect(manifestFamilies(null)).toEqual([]);
});

test('путь файла шрифта — только внутри public/fonts', () => {
  expect(isFontPath('inter/Inter-Variable.ttf')).toBe(true);
  for (const bad of ['../x.ttf', '/abs.ttf', 'inter/x.woff2', 'a//b.ttf', 'http://x/y.ttf', 'inter/../x.ttf']) {
    expect(isFontPath(bad), bad).toBe(false);
  }
});

test('системные шрифты Chrome группируются по семействам', () => {
  const fams = localFamilies([
    { family: 'Helvetica Neue', style: 'Regular', postscriptName: 'HelveticaNeue' },
    { family: 'Helvetica Neue', style: 'Bold', postscriptName: 'HelveticaNeue-Bold' },
    { family: 'Helvetica Neue', style: 'Bold', postscriptName: 'HelveticaNeue-Bold2' },
    { family: '', style: 'Regular', postscriptName: 'Nameless' },
  ]);
  expect(fams).toEqual([
    {
      family: 'Helvetica Neue',
      faces: [
        { style: 'Regular', file: 'HelveticaNeue' },
        { style: 'Bold', file: 'HelveticaNeue-Bold' },
      ],
    },
  ]);
});
````

В `apps/web/unit/limits.test.ts` заменить:

````ts
import { expect, test } from '@playwright/test';
import {
  MAX_DATA_BYTES,
  MAX_IMAGE_SIDE,
  MAX_JOURNAL_ENTRIES,
  MAX_KEYMAP_BYTES,
````

на:

````ts
import { expect, test } from '@playwright/test';
import {
  MAX_DATA_BYTES,
  MAX_FAMILY_NAME,
  MAX_FONT_BYTES,
  MAX_FONT_FACES,
  MAX_FONT_FAMILIES,
  MAX_IMAGE_SIDE,
  MAX_JOURNAL_ENTRIES,
  MAX_KEYMAP_BYTES,
````

В `apps/web/unit/limits.test.ts` заменить:

````ts
  expect(rustConst(`${CRATES}ferrum-editor/src/keymap.rs`, 'MAX_KEYMAP_JSON')).toBe(MAX_KEYMAP_BYTES);
  expect(rustConst(`${CRATES}ferrum-editor/src/pages.rs`, 'MAX_NAME_CHARS')).toBe(MAX_NAME_CHARS);
  expect(rustConst(`${CRATES}ferrum-wasm/src/protocol.rs`, 'MAX_JOURNAL_BATCHES')).toBe(MAX_JOURNAL_ENTRIES);
});

test('theme-init.js читает настройки с тем же пределом', () => {
````

на:

````ts
  expect(rustConst(`${CRATES}ferrum-editor/src/keymap.rs`, 'MAX_KEYMAP_JSON')).toBe(MAX_KEYMAP_BYTES);
  expect(rustConst(`${CRATES}ferrum-editor/src/pages.rs`, 'MAX_NAME_CHARS')).toBe(MAX_NAME_CHARS);
  expect(rustConst(`${CRATES}ferrum-wasm/src/protocol.rs`, 'MAX_JOURNAL_BATCHES')).toBe(MAX_JOURNAL_ENTRIES);
  // Шрифты (подсистема 3): файл, имя семейства, размер списка.
  expect(rustConst(`${CRATES}ferrum-fontmeta/src/lib.rs`, 'MAX_FONT_FILE')).toBe(MAX_FONT_BYTES);
  expect(rustConst(`${CRATES}ferrum-text/src/fonts.rs`, 'MAX_FAMILY_NAME')).toBe(MAX_FAMILY_NAME);
  expect(rustConst(`${CRATES}ferrum-wasm/src/protocol.rs`, 'MAX_FONT_FAMILIES')).toBe(MAX_FONT_FAMILIES);
  expect(rustConst(`${CRATES}ferrum-wasm/src/protocol.rs`, 'MAX_FONT_FACES')).toBe(MAX_FONT_FACES);
});

test('theme-init.js читает настройки с тем же пределом', () => {
````

Создать `apps/web/e2e/text.spec.ts`:

````ts
// Текст (подсистема 3, этап 1): текстовый узел рисуется вшитым Inter (файл
// качает worker ядра по манифесту), отсутствующий шрифт — в списке
// «Отсутствующие шрифты (N)» с заменой одним шагом отмены, тестовая команда
// создания текста — только в тестовом режиме. Firefox (@firefox): текст рисуется,
// «Показать системные шрифты» нет.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

/** Текст в точке мира; ждёт узел в сцене (шрифты грузятся асинхронно). */
async function createText(page: Page, x: number, y: number, text: string, font?: { family: string; style: string }): Promise<void> {
  const before = (await live(page)).nodes;
  await page.evaluate(
    ({ x, y, text, font }) => window.__ferrum!.core!.send({ type: 'testCreateText', x, y, text, size: 64, font }),
    { x, y, text, font },
  );
  await liveWhen(page, `l.nodes === ${before + 1} && l.busy === 0`);
}

/** Тёмных пикселей в квадрате экрана (текст чёрный на белом фрейме примера). */
async function darkPixels(page: Page, at: { x: number; y: number }, size: number): Promise<number> {
  const png = (await page.screenshot({ clip: { x: at.x, y: at.y, width: size, height: size } })).toString('base64');
  return page.evaluate(async (png) => {
    const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    const ctx = c.getContext('2d')!;
    ctx.drawImage(bmp, 0, 0);
    const d = ctx.getImageData(0, 0, bmp.width, bmp.height).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] < 100 && d[i + 1] < 100 && d[i + 2] < 100) n++;
    return n;
  }, png);
}

test('текст рисуется вшитым Inter, загруженным worker\'ом ядра @firefox', async ({ page }) => {
  const fonts: string[] = [];
  page.on('request', (r) => {
    if (r.url().includes('/fonts/')) fonts.push(new URL(r.url()).pathname);
  });
  await openApp(page, 0);
  // Белый фон страницы: до текста в этом месте нет тёмных пикселей.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setPageBackground', color: '#ffffff', phase: 'commit' }));
  const l = await live(page);
  const at = await appScreen(page, l, 100, 300);
  await expect.poll(() => darkPixels(page, at, 120)).toBe(0);
  await createText(page, 100, 300, 'Ferrum');
  await expect.poll(() => darkPixels(page, at, 120), { timeout: 10_000 }).toBeGreaterThan(200);
  expect(fonts).toContain('/fonts/manifest.json');
  expect(fonts).toContain('/fonts/inter/Inter-Variable.ttf');
  // Строка слоя — «Текст» с иконкой текста.
  await expect(page.getByRole('treeitem', { name: 'Текст', exact: true })).toBeVisible();
  // Системные шрифты в Firefox не предлагаются.
  if (test.info().project.name === 'firefox') {
    await page.evaluate(() => window.__ferrum!.core!.send({ type: 'select', ids: [], mode: 'replace' }));
    await expect(page.getByRole('button', { name: 'Показать системные шрифты' })).toHaveCount(0);
  }
});
````

Run: `cd apps/web && npm run wasm && npm run unit`
Expected: FAIL — нет модуля `src/core/fonts`, нет `MAX_FONT_BYTES` в `limits.ts`.

- [ ] **Step 2: Пределы, конверт команды с байтами шрифта, разбор манифеста**

В `apps/web/src/core/limits.ts` заменить:

````ts
export const MAX_NAME_CHARS = 256;
/** Пачек журнала изменений при открытии файла — как `MAX_JOURNAL_BATCHES` ядра. */
export const MAX_JOURNAL_ENTRIES = 100_000;

/** Мегабайты с округлением вверх: «предел 64 МБ, получено 64 МБ» сбивало бы с толку. */
export const mb = (bytes: number): number => Math.ceil(bytes / MIB);
````

на:

````ts
export const MAX_NAME_CHARS = 256;
/** Пачек журнала изменений при открытии файла — как `MAX_JOURNAL_BATCHES` ядра. */
export const MAX_JOURNAL_ENTRIES = 100_000;
/** Файл шрифта — как `ferrum_fontmeta::MAX_FONT_FILE` ядра (спека подсистемы 3, раздел 4). */
export const MAX_FONT_BYTES = 64 * MIB;
/** Имя семейства и начертания в байтах UTF-8 — как `MAX_FAMILY_NAME` ядра. */
export const MAX_FAMILY_NAME = 256;
/** Семейств в списке шрифтов и начертаний у семейства — как `MAX_FONT_FAMILIES`, `MAX_FONT_FACES`. */
export const MAX_FONT_FAMILIES = 4096;
export const MAX_FONT_FACES = 1000;

/** Мегабайты с округлением вверх: «предел 64 МБ, получено 64 МБ» сбивало бы с толку. */
export const mb = (bytes: number): number => Math.ceil(bytes / MIB);
````

В `apps/web/src/core/messages.ts` заменить:

````ts

import type { Command, CoreEvent } from '../wasm/ferrum_wasm';
import { MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from './clipboard-text';
import { MAX_DATA_BYTES, MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES, mb, tooLarge } from './limits';

export type ToWorker =
  | {
````

на:

````ts

import type { Command, CoreEvent } from '../wasm/ferrum_wasm';
import { MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from './clipboard-text';
import { MAX_DATA_BYTES, MAX_FONT_BYTES, MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES, mb, tooLarge } from './limits';

export type ToWorker =
  | {
````

В `apps/web/src/core/messages.ts` заменить:

````ts
  paste: MAX_DATA_BYTES,
  placeImage: MAX_DATA_BYTES,
  pasteImage: MAX_DATA_BYTES,
} as const;

/** Байты команды не того вида (не массив байтов) — текст без технических имён. */
````

на:

````ts
  paste: MAX_DATA_BYTES,
  placeImage: MAX_DATA_BYTES,
  pasteImage: MAX_DATA_BYTES,
  /** Файл шрифта (`loadFont`; без байтов — файл получить не удалось). */
  loadFont: MAX_FONT_BYTES,
} as const;

/** Байты команды не того вида (не массив байтов) — текст без технических имён. */
````

В `apps/web/src/core/messages.ts` заменить:

````ts
export function commandSizeError(cmd: Command): string | null {
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type === 'openWithJournal') return journalCommandError(cmd);
  if (cmd.type !== 'open' && cmd.type !== 'paste' && cmd.type !== 'placeImage' && cmd.type !== 'pasteImage') return null;
  const bytes = (cmd as { bytes: unknown }).bytes;
  const kind = bytesKindError(bytes);
  if (kind) return kind;
````

на:

````ts
export function commandSizeError(cmd: Command): string | null {
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type === 'openWithJournal') return journalCommandError(cmd);
  if (cmd.type === 'loadFont' && (cmd as { bytes?: unknown }).bytes === undefined) return null;
  if (
    cmd.type !== 'open' &&
    cmd.type !== 'paste' &&
    cmd.type !== 'placeImage' &&
    cmd.type !== 'pasteImage' &&
    cmd.type !== 'loadFont'
  )
    return null;
  const bytes = (cmd as { bytes: unknown }).bytes;
  const kind = bytesKindError(bytes);
  if (kind) return kind;
````

В `apps/web/src/core/messages.ts` заменить:

````ts
export function prepareCommand(cmd: Command): { cmd: Command; transfer: Transferable[] } | { error: string } {
  const error = commandSizeError(cmd);
  if (error) return { error };
  if (typeof cmd !== 'object' || cmd === null || !('bytes' in cmd)) return { cmd, transfer: [] };
  const own = (b: Uint8Array) => (b.byteOffset !== 0 || b.byteLength !== b.buffer.byteLength ? b.slice() : b);
  const bytes = own(cmd.bytes as unknown as Uint8Array);
  if (cmd.type === 'openWithJournal') {
````

на:

````ts
export function prepareCommand(cmd: Command): { cmd: Command; transfer: Transferable[] } | { error: string } {
  const error = commandSizeError(cmd);
  if (error) return { error };
  if (typeof cmd !== 'object' || cmd === null || !('bytes' in cmd) || cmd.bytes === undefined) return { cmd, transfer: [] };
  const own = (b: Uint8Array) => (b.byteOffset !== 0 || b.byteLength !== b.buffer.byteLength ? b.slice() : b);
  const bytes = own(cmd.bytes as unknown as Uint8Array);
  if (cmd.type === 'openWithJournal') {
````

Создать `apps/web/src/core/fonts.ts`:

````ts
// Шрифты на стороне TypeScript (спека подсистемы 3, раздел 4): разбор манифеста
// вшитых шрифтов, адрес файла вшитого шрифта и список системных шрифтов Chrome
// (`queryLocalFonts`) в виде команды `fontsAvailable`. Чистая логика — для
// модульных тестов; данные не доверенные и ограничены как в ядре.

import type { FontFamilyDto } from '../wasm/ferrum_wasm';
import { MAX_FAMILY_NAME, MAX_FONT_FACES, MAX_FONT_FAMILIES } from './limits';

const utf8 = new TextEncoder();

function isName(v: unknown): v is string {
  return typeof v === 'string' && v.length > 0 && v.length <= MAX_FAMILY_NAME && utf8.encode(v).length <= MAX_FAMILY_NAME;
}

/** Путь файла вшитого шрифта: относительный, из латиницы, цифр, `-_.` и `/`, без `..`. */
export function isFontPath(v: unknown): v is string {
  return typeof v === 'string' && v.length <= 256 && /^[A-Za-z0-9_-]+(\/[A-Za-z0-9_.-]+)*\.(ttf|otf)$/.test(v) && !v.includes('..');
}

/** Семейства манифеста (`public/fonts/manifest.json`), пригодные для ядра;
 * неверные записи пропускаются. */
export function manifestFamilies(json: unknown): FontFamilyDto[] {
  const list = (json as { families?: unknown })?.families;
  if (!Array.isArray(list)) return [];
  const out: FontFamilyDto[] = [];
  for (const f of list.slice(0, MAX_FONT_FAMILIES)) {
    if (!isName(f?.family) || !Array.isArray(f.faces)) continue;
    const faces = (f.faces as unknown[])
      .slice(0, MAX_FONT_FACES)
      .filter((x): x is Record<string, unknown> => typeof x === 'object' && x !== null)
      .filter((x) => isName(x.style) && isFontPath(x.file))
      .map((x) => ({
        style: x.style as string,
        weight: typeof x.weight === 'number' ? x.weight : undefined,
        italic: typeof x.italic === 'boolean' ? x.italic : undefined,
        stretch: typeof x.stretch === 'number' ? x.stretch : undefined,
        file: x.file as string,
        index: typeof x.index === 'number' ? x.index : undefined,
      }));
    if (faces.length) out.push({ family: f.family, faces });
  }
  return out;
}

/** Метаданные системного шрифта Chrome (`FontData` без байтов). */
export interface LocalFont {
  family: string;
  style: string;
  postscriptName: string;
}

/** Системные шрифты Chrome в семейства для `fontsAvailable`: файл начертания —
 * postscript-имя (по нему основной поток найдёт `FontData` за байтами). Вес и
 * курсив Chrome не сообщает — их угадывает ядро по имени начертания. */
export function localFamilies(fonts: readonly LocalFont[]): FontFamilyDto[] {
  const byFamily = new Map<string, FontFamilyDto>();
  for (const f of fonts) {
    if (!isName(f.family) || !isName(f.style) || !isName(f.postscriptName)) continue;
    let fam = byFamily.get(f.family);
    if (!fam) {
      if (byFamily.size >= MAX_FONT_FAMILIES) continue;
      fam = { family: f.family, faces: [] };
      byFamily.set(f.family, fam);
    }
    if (fam.faces.length >= MAX_FONT_FACES || fam.faces.some((x) => x.style === f.style)) continue;
    fam.faces.push({ style: f.style, file: f.postscriptName });
  }
  return [...byFamily.values()];
}
````

- [ ] **Step 3: Worker ядра: манифест, вшитые файлы, тестовая команда**

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  type ToWorker,
} from '../core/messages';
import { imageSize } from '../core/image-size';
import { addSaturating, drainRing, holdInput, writeLive as writeLiveSlots, type Held } from '../core/shared';

const scope = self as unknown as DedicatedWorkerGlobalScope;
````

на:

````ts
  type ToWorker,
} from '../core/messages';
import { imageSize } from '../core/image-size';
import { isFontPath, manifestFamilies } from '../core/fonts';
import { addSaturating, drainRing, holdInput, writeLive as writeLiveSlots, type Held } from '../core/shared';

const scope = self as unknown as DedicatedWorkerGlobalScope;
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  return x;
}

/** Байты событий уходят без копии: их буферы передаются основному потоку. */
function sendEvents(): void {
  if (!core) return;
  let events: CoreEvent[] = core.takeEvents();
  if (events.length === 0) return;
  for (const e of events) if (e.type === 'surfaceLost') void recoverSurface(e.backend, e.message);
  if (events.some((e) => e.type === 'clipboard')) {
````

на:

````ts
  return x;
}

/** Адрес файла вшитых шрифтов: `public/fonts` в корне сайта (Vite `base` — `/`,
 * в Tauri — тот же корень `tauri://localhost/`). */
function fontUrl(file: string): string {
  return new URL(`/fonts/${file}`, self.location.origin).href;
}

/** Список вшитых шрифтов — манифест сборки; без него вшитых нет (и ядро не
 * считает шрифты отсутствующими: список не пришёл). */
async function loadManifest(): Promise<void> {
  let families: ReturnType<typeof manifestFamilies> = [];
  try {
    const res = await fetch(fontUrl('manifest.json'));
    if (res.ok) families = manifestFamilies(await res.json());
    else console.warn(`ferrum: манифест шрифтов недоступен (HTTP ${res.status})`);
  } catch (e) {
    console.warn('ferrum: манифест шрифтов недоступен', e);
  }
  if (!core) return;
  core.command({ type: 'fontsAvailable', source: 'bundled', families });
  sendEvents();
}

/** Байты вшитого семейства: worker качает файлы сам (основной поток их не
 * видит), кэш — HTTP-кэш браузера. Неудача — `loadFont` без байтов. */
async function loadBundled(family: string, files: string[]): Promise<void> {
  await Promise.all(
    files.map(async (file) => {
      let bytes: Uint8Array | undefined;
      try {
        if (!isFontPath(file)) throw new Error('неверный путь');
        const res = await fetch(fontUrl(file));
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        bytes = new Uint8Array(await res.arrayBuffer());
      } catch (e) {
        console.warn(`ferrum: шрифт ${family} (${file}) не загружен`, e);
      }
      if (!core) return;
      try {
        core.command({ type: 'loadFont', family, file, bytes });
        sendEvents();
      } catch (e) {
        fatal(e);
      }
    }),
  );
}

/** Байты событий уходят без копии: их буферы передаются основному потоку. */
function sendEvents(): void {
  if (!core) return;
  let events: CoreEvent[] = core.takeEvents();
  if (events.length === 0) return;
  // Вшитые шрифты качает этот worker; основному потоку — только системные.
  for (const e of events) if (e.type === 'fontRequest' && e.source === 'bundled') void loadBundled(e.family, e.files);
  events = events.filter((e) => !(e.type === 'fontRequest' && e.source === 'bundled'));
  for (const e of events) if (e.type === 'log') console.warn('ferrum:', e.message);
  if (events.length === 0) return;
  for (const e of events) if (e.type === 'surfaceLost') void recoverSurface(e.backend, e.message);
  if (events.some((e) => e.type === 'clipboard')) {
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
          return;
        }
        const cmd = msg.cmd;
        // Команда не доверенная: не объект — её отвергнет разбор в ядре.
        const image = typeof cmd === 'object' && cmd !== null && (cmd.type === 'pasteImage' || cmd.type === 'placeImage');
        if (image && !commandSizeError(cmd) && imageSize(cmd.bytes)?.type !== 'png') {
````

на:

````ts
          return;
        }
        const cmd = msg.cmd;
        // Тестовое создание текста — только в тестовом режиме страницы.
        if (typeof cmd === 'object' && cmd !== null && cmd.type === 'testCreateText' && !testHooks) {
          postError('Неверное сообщение для ядра');
          return;
        }
        // Команда не доверенная: не объект — её отвергнет разбор в ядре.
        const image = typeof cmd === 'object' && cmd !== null && (cmd.type === 'pasteImage' || cmd.type === 'placeImage');
        if (image && !commandSizeError(cmd) && imageSize(cmd.bytes)?.type !== 'png') {
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
    return;
  }
  for (const m of early.splice(0)) handle(m);
  if (sab && waitAsync) void sabLoop();
};
````

на:

````ts
    return;
  }
  for (const m of early.splice(0)) handle(m);
  void loadManifest();
  if (sab && waitAsync) void sabLoop();
};
````

В `apps/web/tsconfig.worker.json` заменить:

````json
  "compilerOptions": {
    "lib": ["ES2024", "WebWorker"]
  },
  "include": ["src/worker/**/*.ts", "src/core/layout.gen.ts", "src/core/messages.ts", "src/core/image-size.ts", "src/core/clipboard-text.ts"],
  "exclude": []
}
````

на:

````json
  "compilerOptions": {
    "lib": ["ES2024", "WebWorker"]
  },
  "include": ["src/worker/**/*.ts", "src/core/layout.gen.ts", "src/core/messages.ts", "src/core/image-size.ts", "src/core/clipboard-text.ts", "src/core/fonts.ts", "src/core/limits.ts"],
  "exclude": []
}
````

- [ ] **Step 4: Страница из текстов у стенда и нового файла**

В `apps/web/src/debug.ts` заменить:

````ts
    if (e.type === 'pasteRequested' && clipboard) core.send({ type: 'paste', bytes: clipboard.slice() });
  });
  startHud(hud, core);
  core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 1000) });
  canvas.focus();
}

````

на:

````ts
    if (e.type === 'pasteRequested' && clipboard) core.send({ type: 'paste', bytes: clipboard.slice() });
  });
  startHud(hud, core);
  // `?texts=N` — страница из N текстов (замер «10 000 текстов», подсистема 3).
  if (params.has('texts')) core.send({ type: 'demoText', texts: Number(params.get('texts')) });
  else core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 1000) });
  canvas.focus();
}

````

В `apps/web/src/ui/boot.ts` заменить:

````ts
    // Правки после файла (сбой, закрытая вкладка): ядро применит их поверх него.
    core.send({ type: 'openWithJournal', bytes: new Uint8Array(doc.bytes), journal: doc.journal.map((b) => new Uint8Array(b)) });
  } else if (doc) core.send({ type: 'open', bytes: new Uint8Array(doc.bytes) });
  else core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 0) });
  return editor;
}
````

на:

````ts
    // Правки после файла (сбой, закрытая вкладка): ядро применит их поверх него.
    core.send({ type: 'openWithJournal', bytes: new Uint8Array(doc.bytes), journal: doc.journal.map((b) => new Uint8Array(b)) });
  } else if (doc) core.send({ type: 'open', bytes: new Uint8Array(doc.bytes) });
  else if (params.has('texts')) core.send({ type: 'demoText', texts: Number(params.get('texts')) });
  else core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 0) });
  return editor;
}
````

- [ ] **Step 5: Проверить**

Run: `cd apps/web && npm run typecheck && npm run unit && npm run build && npx playwright test e2e/text.spec.ts --project=chromium`
Expected: PASS: модульные — 117 (новые: манифест сборки читается целиком — девять семейств, путь шрифта только внутри `public/fonts`, системные шрифты группируются по семействам; пределы шрифтов совпадают с константами Rust); E2E — 1: до текста на белом фоне нет тёмных пикселей, после — больше 200, worker запросил `/fonts/manifest.json` и `/fonts/inter/Inter-Variable.ttf`, в слоях — строка «Текст».

- [ ] **Step 6: Закоммитить**

```bash
git add apps/web/src apps/web/tsconfig.worker.json apps/web/unit apps/web/e2e/text.spec.ts
git commit -m "feat(web): worker ядра читает манифест и качает вшитые шрифты сам; loadFont с передачей байтов, тестовая команда текста только в тестовом режиме, ?texts=N

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Системные шрифты и «Отсутствующие шрифты (N)»

Спека 4. Chrome и Edge: `queryLocalFonts()` — по щелчку «Показать системные шрифты» (разрешение браузера) или при запуске, если разрешение уже выдано (`navigator.permissions.query({ name: 'local-fonts' })`); метаданные `FontData` (семейство, начертание, postscript-имя — файл начертания) группируются в основном потоке и уходят ядру `fontsAvailable { source: 'system' }`, объекты `FontData` остаются в основном потоке (`Map` по postscript-имени). На `fontRequest` системного семейства основной поток берёт `FontData.blob()` (ссылка, без чтения), worker ввода-вывода читает байты (`op: 'fontBytes'`, ≤ 64 МБ по `Blob.size`) и возвращает их передачей, основной поток передаёт их ядру `loadFont`. Десктоп — команды IPC `fonts_list` и `fonts_read` (Task 10). Firefox и Safari — `NO_SYSTEM_FONTS`, пункта нет. Секция «Отсутствующие шрифты (N)» — на панели «Дизайн» без выделения, над фоном страницы: строка шрифта с числом текстов, выбор семейства (группы «Вшитые», «Системные») и начертания, «Заменить» → `replaceMissingFont`.

**Files:**
- Create: `apps/web/src/ui/system-fonts.ts`, `apps/web/src/ui/missing-fonts.tsx`
- Modify: `apps/web/src/core/fonts.ts`, `src/core/file-list.ts` (`IoOp` 111–159, `IoResults` 164–190, `requestError` 254–315), `src/worker/io.worker.ts` (`handle` 356–406), `src/ui/file-store.ts` (`FileStore` 18–66, `IdbFileStore` 79–140), `src/ui/context.ts` (`Editor` 10–22), `src/ui/boot.ts` (`BootOptions` 78–88, редактор 160–175), `src/ui/app.tsx:241–244`, `src/ui/tauri.ts:48`, `src/ui/store.ts`, `src/ui/properties.tsx:49–60`, `src/ui/styles/properties.css`
- Test: `apps/web/unit/fonts.test.ts`, `apps/web/e2e/text.spec.ts`

**Interfaces:**
- Consumes: протокол Task 7 (`fontRequest` с `source: 'system'`, `missingFonts`, `fontCatalog`, `replaceMissingFont`, `fontsAvailable`), `localFamilies` (Task 8).
- Produces: `SystemFonts { supported; granted(); list(); read(file) }`, `chromeFonts(readBlob)`, `tauriFonts()`, `NO_SYSTEM_FONTS`, `systemFonts(readBlob)`, `installSystemFonts(ed, src) -> { show, dispose }`; `Editor.showSystemFonts()`; `BootOptions.systemFonts`; `FileStore.fontBytes?(blob)`, `IdbFileStore.fontBytes`; запрос ввода-вывода `{ op: 'fontBytes'; blob: Blob }` → `ArrayBuffer`; `tauri.ts`: `systemFontList()`, `systemFontBytes(file)`; состояние `UiState.missingFonts`, `fontCatalog`, `systemFonts { supported, shown }`; `MissingFontsSection`; `src/core/fonts.ts`: `styleKey`, `defaultStyle(choice, wanted)`.

- [ ] **Step 1: Падающие тесты**

В `apps/web/unit/fonts.test.ts` заменить:

````ts
// Шрифты на стороне TypeScript (подсистема 3): манифест вшитых, системные шрифты
// Chrome в семейства, адрес файла.

import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { isFontPath, localFamilies, manifestFamilies } from '../src/core/fonts';

test('манифест сборки читается целиком, мусор пропускается', () => {
  const manifest = JSON.parse(readFileSync(new URL('../public/fonts/manifest.json', import.meta.url), 'utf8'));
````

на:

````ts
// Шрифты на стороне TypeScript (подсистема 3): манифест вшитых, системные шрифты
// Chrome в семейства, адрес файла и начертание замены по умолчанию.

import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { defaultStyle, isFontPath, localFamilies, manifestFamilies, styleKey } from '../src/core/fonts';

test('манифест сборки читается целиком, мусор пропускается', () => {
  const manifest = JSON.parse(readFileSync(new URL('../public/fonts/manifest.json', import.meta.url), 'utf8'));
````

В `apps/web/unit/fonts.test.ts` заменить:

````ts
    },
  ]);
});
````

на:

````ts
    },
  ]);
});

test('начертание замены: то же, иначе Regular, иначе первое', () => {
  expect(styleKey('Semi Bold Italic')).toBe(styleKey('SemiBold Italic'));
  const roboto = { family: 'Roboto', source: 'bundled' as const, styles: ['Thin', 'Regular', 'SemiBold Italic'] };
  expect(defaultStyle(roboto, 'Semi Bold Italic')).toBe('SemiBold Italic');
  expect(defaultStyle(roboto, 'Wide')).toBe('Regular');
  expect(defaultStyle({ ...roboto, styles: ['Thin'] }, 'Wide')).toBe('Thin');
  expect(defaultStyle(undefined, 'Bold')).toBe('');
});
````

В `apps/web/e2e/text.spec.ts` заменить:

````ts
    await expect(page.getByRole('button', { name: 'Показать системные шрифты' })).toHaveCount(0);
  }
});
````

на:

````ts
    await expect(page.getByRole('button', { name: 'Показать системные шрифты' })).toHaveCount(0);
  }
});

test('отсутствующий шрифт: список и замена одним шагом отмены', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Comic', { family: 'Comic Sans', style: 'Bold' });
  // Без выделения — панель «Дизайн» показывает отсутствующие шрифты.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'select', ids: [], mode: 'replace' }));
  const section = page.getByRole('region', { name: 'Отсутствующие шрифты (1)' });
  await expect(section).toBeVisible();
  const group = section.getByRole('group', { name: 'Comic Sans Bold' });
  await group.getByRole('combobox', { name: 'Замена для Comic Sans Bold: семейство' }).selectOption('Roboto');
  await expect(group.getByRole('combobox', { name: 'Замена для Comic Sans Bold: начертание' })).toHaveValue('Bold');
  const steps = (await live(page)).steps;
  await group.getByRole('button', { name: 'Заменить' }).click();
  // Замена ждёт загрузки Roboto, затем — один шаг, и список пуст.
  await liveWhen(page, `l.steps === ${steps + 1}`);
  await expect(section).toHaveCount(0);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await expect(page.getByRole('region', { name: 'Отсутствующие шрифты (1)' })).toBeVisible();
});

test('тестовая команда создания текста вне тестового режима отвергается', async ({ page }) => {
  await page.addInitScript(() => ((window as { __ferrumPerf?: boolean }).__ferrumPerf = true));
  await page.goto('/?nodes=0#/new');
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true, null, { timeout: 90_000 });
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'testCreateText', x: 0, y: 0, text: 'x' }));
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.toasts.map((t) => t.text))).toContain('Неверное сообщение для ядра');
  expect((await live(page)).nodes).toBe(0);
});

test('системные шрифты Chrome: «Показать системные шрифты», байты через worker ввода-вывода', async ({ page, context }, info) => {
  test.skip(info.project.name !== 'chromium', 'queryLocalFonts — только Chrome и Edge');
  await openApp(page, 0);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setPageBackground', color: '#ffffff', phase: 'commit' }));
  // Разрешение после запуска: список при запуске не строился, нужен щелчок.
  await context.grantPermissions(['local-fonts']);
  const family = await page.evaluate(async () => {
    const all = await (window as unknown as { queryLocalFonts: () => Promise<{ family: string; style: string }[]> }).queryLocalFonts();
    const bundled = ['Inter', 'Onest', 'Roboto', 'Open Sans', 'Montserrat', 'PT Sans', 'PT Serif', 'Noto Sans', 'JetBrains Mono', 'Playfair Display'];
    return all.find((f) => f.style === 'Regular' && !bundled.includes(f.family))?.family ?? null;
  });
  test.skip(family === null, 'в системе нет шрифтов, кроме вшитых');
  const l = await live(page);
  const at = await appScreen(page, l, 100, 300);
  await createText(page, 100, 300, 'Ferrum', { family: family!, style: 'Regular' });
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'select', ids: [], mode: 'replace' }));
  await expect(page.getByRole('region', { name: 'Отсутствующие шрифты (1)' })).toBeVisible();
  await page.getByRole('button', { name: 'Показать системные шрифты' }).click();
  // Шрифт нашёлся: список пуст, текст нарисован системным шрифтом (байты пришли).
  await expect(page.getByRole('region', { name: /Отсутствующие шрифты/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Показать системные шрифты' })).toHaveCount(0);
  await expect.poll(() => darkPixels(page, at, 120), { timeout: 10_000 }).toBeGreaterThan(200);
});
````

Run: `cd apps/web && npm run unit`
Expected: FAIL — нет `defaultStyle`, `styleKey` в `src/core/fonts.ts`.

- [ ] **Step 2: Байты системного шрифта — worker ввода-вывода**

В `apps/web/src/core/file-list.ts` заменить:

````ts
// чистые функции, общие для него и модульных тестов. Всё, что пересекает границу
// потоков, не доверенное: проверки — `requestError`.

import { MAX_NAME_CHARS, MAX_OPEN_BYTES, MAX_SETTINGS_BYTES, tooLarge } from './limits';

/** Поля настроек пользователя в хранилище `settings` — по ключу на поле: вкладки
 * пишут только свои изменения и не затирают чужие. Значения проверяет интерфейс. */
````

на:

````ts
// чистые функции, общие для него и модульных тестов. Всё, что пересекает границу
// потоков, не доверенное: проверки — `requestError`.

import { MAX_FONT_BYTES, MAX_NAME_CHARS, MAX_OPEN_BYTES, MAX_SETTINGS_BYTES, tooLarge } from './limits';

/** Поля настроек пользователя в хранилище `settings` — по ключу на поле: вкладки
 * пишут только свои изменения и не затирают чужие. Значения проверяет интерфейс. */
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
  | { op: 'delete'; file: string }
  | { op: 'emptyTrash' }
  | { op: 'thumb'; file: string }
  /** Тесты: засеять `count` пустых записей (только в тестовом режиме). */
  | { op: 'seed'; count: number }
  /** Тесты: следующие записи содержимого отказывают, как при нехватке места. */
````

на:

````ts
  | { op: 'delete'; file: string }
  | { op: 'emptyTrash' }
  | { op: 'thumb'; file: string }
  /** Байты системного шрифта Chrome (`FontData.blob()`): `Blob` приходит ссылкой,
   * читает его этот worker, байты уходят передачей (спека подсистемы 3, раздел 4). */
  | { op: 'fontBytes'; blob: Blob }
  /** Тесты: засеять `count` пустых записей (только в тестовом режиме). */
  | { op: 'seed'; count: number }
  /** Тесты: следующие записи содержимого отказывают, как при нехватке места. */
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
  delete: null;
  emptyTrash: null;
  thumb: Blob | null;
  seed: null;
  failWrites: null;
  holdSettings: null;
````

на:

````ts
  delete: null;
  emptyTrash: null;
  thumb: Blob | null;
  fontBytes: ArrayBuffer;
  seed: null;
  failWrites: null;
  holdSettings: null;
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
      return null;
    case 'setSettings':
      return settingsPatchError(m.patch);
    case 'seed':
      return isCount(m.count, MAX_SEED) ? null : BAD;
    case 'failWrites':
````

на:

````ts
      return null;
    case 'setSettings':
      return settingsPatchError(m.patch);
    case 'fontBytes':
      return typeof Blob === 'function' && m.blob instanceof Blob ? bytesError(m.blob, MAX_FONT_BYTES) : BAD;
    case 'seed':
      return isCount(m.count, MAX_SEED) ? null : BAD;
    case 'failWrites':
````

В `apps/web/src/worker/io.worker.ts` заменить:

````ts
      const blob = (await request(d.transaction('thumbs').objectStore('thumbs').get(r.file))) as unknown;
      return { value: blob instanceof Blob ? blob : null };
    }
    case 'setSettings':
      // Тесты: запись «потеряна» — ответа нет (вне очереди правок: её не держит).
      if (holdSettings) return new Promise<never>(() => {});
````

на:

````ts
      const blob = (await request(d.transaction('thumbs').objectStore('thumbs').get(r.file))) as unknown;
      return { value: blob instanceof Blob ? blob : null };
    }
    case 'fontBytes': {
      // Байты системного шрифта (вне очереди правок: база не нужна). Размер проверен
      // по `Blob` в `requestError`; основному потоку — передачей.
      const buf = await r.blob.arrayBuffer();
      return { value: buf, transfer: [buf] };
    }
    case 'setSettings':
      // Тесты: запись «потеряна» — ответа нет (вне очереди правок: её не держит).
      if (holdSettings) return new Promise<never>(() => {});
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  onChange(listener: () => void): () => void;
  /** Файлы на диске (File System Access, Tauri); нет — пунктов «с диска» нет. */
  readonly disk?: DiskFiles;
  dispose(): void;
}

````

на:

````ts
  onChange(listener: () => void): () => void;
  /** Файлы на диске (File System Access, Tauri); нет — пунктов «с диска» нет. */
  readonly disk?: DiskFiles;
  /** Байты системного шрифта Chrome (`FontData.blob()`) — читает worker
   * ввода-вывода; нет — системных шрифтов браузера нет. */
  fontBytes?(blob: Blob): Promise<ArrayBuffer>;
  dispose(): void;
}

````

В `apps/web/src/ui/file-store.ts` заменить:

````ts

  list(section: Section, query: string, start: number, count: number) {
    return this.call({ op: 'list', section, query, start, count });
  }
  get(file: string) {
    return this.call({ op: 'get', file });
````

на:

````ts

  list(section: Section, query: string, start: number, count: number) {
    return this.call({ op: 'list', section, query, start, count });
  }
  /** Байты системного шрифта Chrome: `Blob` читает worker ввода-вывода. */
  fontBytes(blob: Blob) {
    return this.call({ op: 'fontBytes', blob });
  }
  get(file: string) {
    return this.call({ op: 'get', file });
````

В `apps/web/src/ui/tauri.ts` заменить:

````ts
// передаёт: их дают только родные диалоги в Rust. Окно одно: изменения списка
// интерфейс узнаёт от своих же вызовов, `BroadcastChannel` не нужен.

import type { FileMeta, IoResults, JournalMark, Section } from '../core/file-list';
import type { DiskFiles } from './disk';
import type { FileStore } from './file-store';
````

на:

````ts
// передаёт: их дают только родные диалоги в Rust. Окно одно: изменения списка
// интерфейс узнаёт от своих же вызовов, `BroadcastChannel` не нужен.

import type { FontFamilyDto } from '../wasm/ferrum_wasm';
import type { FileMeta, IoResults, JournalMark, Section } from '../core/file-list';
import type { DiskFiles } from './disk';
import type { FileStore } from './file-store';
````

В `apps/web/src/ui/tauri.ts` заменить:

````ts
  if (message.startsWith('state not managed')) return 'Хранилище файлов не открыто';
  if (/^invalid args |^command \S+ not found/.test(message)) return 'Неверный запрос к хранилищу файлов';
  return message;
}

/** Сырое тело запроса и параметры заголовком `x-ferrum` (ASCII: UUID и числа). */
````

на:

````ts
  if (message.startsWith('state not managed')) return 'Хранилище файлов не открыто';
  if (/^invalid args |^command \S+ not found/.test(message)) return 'Неверный запрос к хранилищу файлов';
  return message;
}

/** Системные шрифты десктопа (спека подсистемы 3, раздел 4): семейства из
 * каталогов шрифтов; файл начертания — номер файла в списке Rust. */
export function systemFontList(): Promise<FontFamilyDto[]> {
  return invoke<FontFamilyDto[]>('fonts_list');
}

/** Байты файла системного шрифта — сырым ответом IPC. */
export function systemFontBytes(file: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>('fonts_read', { id: Number(file) });
}

/** Сырое тело запроса и параметры заголовком `x-ferrum` (ASCII: UUID и числа). */
````

- [ ] **Step 3: Системные шрифты в редакторе**

Создать `apps/web/src/ui/system-fonts.ts`:

````ts
// Системные шрифты (спека подсистемы 3, раздел 4). Chrome и Edge —
// `queryLocalFonts()`: список (метаданные `FontData` без байтов) строится здесь,
// в основном потоке, по явному действию пользователя или при запуске, если
// разрешение уже выдано; байты файла (`FontData.blob()`) читает worker
// ввода-вывода и отдаёт передачей. Десктоп — сканирование каталогов шрифтов в
// Rust, байты сырым ответом IPC. Firefox и Safari — только вшитые.

import type { FontFamilyDto } from '../wasm/ferrum_wasm';
import { localFamilies, type LocalFont } from '../core/fonts';
import { MAX_FONT_BYTES, tooLarge } from '../core/limits';
import type { Editor } from './context';
import { isTauri, systemFontBytes, systemFontList } from './tauri';

/** `FontData` Chrome (тип ещё не во всех lib.dom). */
interface FontData extends LocalFont {
  blob(): Promise<Blob>;
}

type QueryLocalFonts = () => Promise<FontData[]>;

export interface SystemFonts {
  /** Есть ли системные шрифты вообще (иначе пункт «Показать системные шрифты» скрыт). */
  readonly supported: boolean;
  /** Разрешение уже есть: список строится без вопроса. */
  granted(): Promise<boolean>;
  /** Список семейств (в Chrome — по действию пользователя: просит разрешение). */
  list(): Promise<FontFamilyDto[]>;
  /** Байты файла начертания (postscript-имя в Chrome, номер файла в десктопе). */
  read(file: string): Promise<ArrayBuffer>;
}

/** Chrome и Edge: `queryLocalFonts`; байты — через worker ввода-вывода. */
export function chromeFonts(readBlob: (blob: Blob) => Promise<ArrayBuffer>): SystemFonts {
  const query = (window as unknown as { queryLocalFonts?: QueryLocalFonts }).queryLocalFonts;
  const byName = new Map<string, FontData>();
  return {
    supported: typeof query === 'function',
    async granted() {
      try {
        const p = await navigator.permissions.query({ name: 'local-fonts' as PermissionName });
        return p.state === 'granted';
      } catch {
        return false;
      }
    },
    async list() {
      if (!query) return [];
      const fonts = await query.call(window);
      byName.clear();
      for (const f of fonts) byName.set(f.postscriptName, f);
      return localFamilies(fonts);
    },
    async read(file) {
      const f = byName.get(file);
      if (!f) throw new Error('шрифта нет в списке');
      const blob = await f.blob();
      if (blob.size > MAX_FONT_BYTES) throw new Error(tooLarge(MAX_FONT_BYTES, blob.size));
      return readBlob(blob);
    },
  };
}

/** Десктоп: каталоги шрифтов сканирует Rust; разрешения не нужно. */
export function tauriFonts(): SystemFonts {
  return {
    supported: true,
    granted: async () => true,
    list: () => systemFontList(),
    read: (file) => systemFontBytes(file),
  };
}

export const NO_SYSTEM_FONTS: SystemFonts = {
  supported: false,
  granted: async () => false,
  list: async () => [],
  read: async () => {
    throw new Error('системных шрифтов нет');
  },
};

/** Источник системных шрифтов этой платформы. */
export function systemFonts(readBlob: (blob: Blob) => Promise<ArrayBuffer>): SystemFonts {
  if (isTauri()) return tauriFonts();
  const chrome = chromeFonts(readBlob);
  return chrome.supported ? chrome : NO_SYSTEM_FONTS;
}

/** Подключает системные шрифты к ядру: список при запуске (если разрешение уже
 * есть), ответы на `fontRequest` системных семейств. Возвращает «показать
 * системные шрифты» (по щелчку пользователя) и отписку. */
export function installSystemFonts(
  ed: Pick<Editor, 'core' | 'store' | 'send'>,
  src: SystemFonts,
): { show: () => Promise<void>; dispose: () => void } {
  let shown = false;
  const show = async () => {
    try {
      const families = await src.list();
      shown = true;
      ed.store.setState('systemFonts', 'shown', true);
      ed.send({ type: 'fontsAvailable', source: 'system', families });
    } catch (e) {
      ed.store.toast(`Системные шрифты недоступны: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  };
  ed.store.setState('systemFonts', { supported: src.supported, shown: false });
  if (src.supported) {
    void src.granted().then(async (ok) => {
      if (ok && !shown) await show();
    });
  }
  const off = ed.core.on((e) => {
    if (e.type !== 'fontRequest' || e.source !== 'system') return;
    for (const file of e.files) {
      src
        .read(file)
        .then((buf) => ed.send({ type: 'loadFont', family: e.family, file, bytes: new Uint8Array(buf) }))
        .catch((err: unknown) => {
          console.warn(`ferrum: системный шрифт ${e.family} не прочитан`, err);
          ed.send({ type: 'loadFont', family: e.family, file });
        });
    }
  });
  return { show, dispose: off };
}
````

В `apps/web/src/ui/context.ts` заменить:

````ts
  run(command: string): void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
  /** Остановить ядро (уход с редактора). */
  dispose(): void;
}
````

на:

````ts
  run(command: string): void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
  /** «Показать системные шрифты» (Chrome — с разрешения браузера). */
  showSystemFonts(): void;
  /** Остановить ядро (уход с редактора). */
  dispose(): void;
}
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
import { forwardKey } from './keys';
import { createScheduler, guardLayout } from './scheduler';
import { loadLayoutMap, type LayoutMap } from './shortcuts';
import type { UiStore } from './store';

/** Сколько заставка держится с открытия страницы, даже если ядро готово раньше:
````

на:

````ts
import { forwardKey } from './keys';
import { createScheduler, guardLayout } from './scheduler';
import { loadLayoutMap, type LayoutMap } from './shortcuts';
import { installSystemFonts, NO_SYSTEM_FONTS, type SystemFonts } from './system-fonts';
import type { UiStore } from './store';

/** Сколько заставка держится с открытия страницы, даже если ядро готово раньше:
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
  /** Байты файла и пачки журнала после них (читаются, пока запускается ядро);
   * `null` — новый файл. */
  document?: Promise<{ bytes: ArrayBuffer; journal: ArrayBuffer[] } | null>;
}

/** Тестовый режим страницы (`__ferrumTest` или `?debug`). */
````

на:

````ts
  /** Байты файла и пачки журнала после них (читаются, пока запускается ядро);
   * `null` — новый файл. */
  document?: Promise<{ bytes: ArrayBuffer; journal: ArrayBuffer[] } | null>;
  /** Системные шрифты платформы (подсистема 3); нет — только вшитые. */
  systemFonts?: SystemFonts;
}

/** Тестовый режим страницы (`__ferrumTest` или `?debug`). */
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
  const refreshLayout = () => void loadLayoutMap().then((m) => setLayout(() => m));
  refreshLayout();
  (navigator as Navigator & { keyboard?: EventTarget }).keyboard?.addEventListener?.('layoutchange', refreshLayout);
  const editor: Editor = {
    core,
    store,
````

на:

````ts
  const refreshLayout = () => void loadLayoutMap().then((m) => setLayout(() => m));
  refreshLayout();
  (navigator as Navigator & { keyboard?: EventTarget }).keyboard?.addEventListener?.('layoutchange', refreshLayout);
  const fonts = installSystemFonts({ core, store, send: (cmd) => core.send(cmd) }, opts.systemFonts ?? NO_SYSTEM_FONTS);
  const editor: Editor = {
    core,
    store,
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
    send: (cmd) => core.send(cmd),
    run: (command) => core.send({ type: 'execute', command }),
    layout,
    // Ядро и защита чтений layout уходят вместе с редактором: экрану файлов они не нужны.
    dispose: () => {
      unguard();
      core.dispose();
    },
  };
````

на:

````ts
    send: (cmd) => core.send(cmd),
    run: (command) => core.send({ type: 'execute', command }),
    layout,
    showSystemFonts: () => void fonts.show(),
    // Ядро и защита чтений layout уходят вместе с редактором: экрану файлов они не нужны.
    dispose: () => {
      unguard();
      fonts.dispose();
      core.dispose();
    },
  };
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { SettingsDialog } from './settings-dialog';
import { downloadBytes, fileName } from './files';
import { importFromDisk, type FileMeta, type FileStore } from './file-store';
import type { Backend } from '../wasm/ferrum_wasm';
import { restartText, type Restart } from './restart';
import type { Route } from './router';
````

на:

````tsx
import { SettingsDialog } from './settings-dialog';
import { downloadBytes, fileName } from './files';
import { importFromDisk, type FileMeta, type FileStore } from './file-store';
import { systemFonts } from './system-fonts';
import type { Backend } from '../wasm/ferrum_wasm';
import { restartText, type Restart } from './restart';
import type { Route } from './router';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
    boot(canvas, store, {
      document: bytes,
      backend: props.backend,
      // До документа: тема (первый кадр сразу в цвете темы), привязка и сохранённая раскладка.
      beforeDocument: (ed) => {
        ed.send({ type: 'setTheme', theme: theme() });
````

на:

````tsx
    boot(canvas, store, {
      document: bytes,
      backend: props.backend,
      systemFonts: systemFonts((blob) => props.files.fontBytes?.(blob) ?? Promise.reject(new Error('нет хранилища'))),
      // До документа: тема (первый кадр сразу в цвете темы), привязка и сохранённая раскладка.
      beforeDocument: (ed) => {
        ed.send({ type: 'setTheme', theme: theme() });
````

- [ ] **Step 4: «Отсутствующие шрифты (N)»**

В `apps/web/src/core/fonts.ts` заменить:

````ts
// (`queryLocalFonts`) в виде команды `fontsAvailable`. Чистая логика — для
// модульных тестов; данные не доверенные и ограничены как в ядре.

import type { FontFamilyDto } from '../wasm/ferrum_wasm';
import { MAX_FAMILY_NAME, MAX_FONT_FACES, MAX_FONT_FAMILIES } from './limits';

const utf8 = new TextEncoder();
````

на:

````ts
// (`queryLocalFonts`) в виде команды `fontsAvailable`. Чистая логика — для
// модульных тестов; данные не доверенные и ограничены как в ядре.

import type { FontChoice, FontFamilyDto } from '../wasm/ferrum_wasm';
import { MAX_FAMILY_NAME, MAX_FONT_FACES, MAX_FONT_FAMILIES } from './limits';

const utf8 = new TextEncoder();
````

В `apps/web/src/core/fonts.ts` заменить:

````ts
  }
  return [...byFamily.values()];
}
````

на:

````ts
  }
  return [...byFamily.values()];
}

/** Ключ начертания как в ядре (`style_key`): без разделителей и регистра. */
export function styleKey(s: string): string {
  return [...s].filter((c) => /[\p{L}\p{N}]/u.test(c)).join('').toLowerCase();
}

/** Начертание замены по умолчанию: то же имя, иначе Regular, иначе первое. */
export function defaultStyle(choice: FontChoice | undefined, wanted: string): string {
  if (!choice || choice.styles.length === 0) return '';
  return (
    choice.styles.find((s) => styleKey(s) === styleKey(wanted)) ??
    choice.styles.find((s) => styleKey(s) === 'regular') ??
    choice.styles[0]
  );
}
````

В `apps/web/src/ui/store.ts` заменить:

````ts
import type {
  ConflictInfo,
  CoreEvent,
  KeymapEntry,
  LayerRow,
  PageInfo,
  PresetInfo,
````

на:

````ts
import type {
  ConflictInfo,
  CoreEvent,
  FontChoice,
  KeymapEntry,
  MissingFontDto,
  LayerRow,
  PageInfo,
  PresetInfo,
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  toasts: Toast[];
  /** Сочетание занято другими командами: настройки предлагают заменить. */
  bindConflict: { command: string; shortcut: string; commands: string[] } | null;
}

const initial: UiState = {
````

на:

````ts
  toasts: Toast[];
  /** Сочетание занято другими командами: настройки предлагают заменить. */
  bindConflict: { command: string; shortcut: string; commands: string[] } | null;
  /** Отсутствующие шрифты текущей страницы (подсистема 3) и семейства для замены. */
  missingFonts: MissingFontDto[];
  fontCatalog: FontChoice[];
  /** Системные шрифты: есть ли на платформе и показаны ли уже. */
  systemFonts: { supported: boolean; shown: boolean };
}

const initial: UiState = {
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  docSeq: 0,
  toasts: [],
  bindConflict: null,
};

/** Сколько уведомлений видно одновременно и сколько секунд живёт каждое. */
````

на:

````ts
  docSeq: 0,
  toasts: [],
  bindConflict: null,
  missingFonts: [],
  fontCatalog: [],
  systemFonts: { supported: false, shown: false },
};

/** Сколько уведомлений видно одновременно и сколько секунд живёт каждое. */
````

В `apps/web/src/ui/store.ts` заменить:

````ts
      case 'toggleUi':
        setState('uiHidden', (h) => !h);
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
        if (quiet && e.message.startsWith('Не удалось импортировать раскладку')) {
````

на:

````ts
      case 'toggleUi':
        setState('uiHidden', (h) => !h);
        break;
      case 'missingFonts':
        setState('missingFonts', reconcile(e.list));
        break;
      case 'fontCatalog':
        setState('fontCatalog', e.families);
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
        if (quiet && e.message.startsWith('Не удалось импортировать раскладку')) {
````

Создать `apps/web/src/ui/missing-fonts.tsx`:

````tsx
// «Отсутствующие шрифты (N)» (спека подсистемы 3, раздел 4): шрифты текущей
// страницы, которых нет ни во вшитых, ни в системных, с заменой одним действием
// (одна правка — один шаг отмены). Список и каталог семейств присылает ядро;
// панель показывает их и шлёт `replaceMissingFont`. Как в Figma — на панели
// «Дизайн» без выделения.

import { createMemo, createSignal, For, Show } from 'solid-js';
import type { FontChoice, MissingFontDto } from '../wasm/ferrum_wasm';
import { defaultStyle } from '../core/fonts';
import { Section } from './chrome';
import { useEditor } from './context';

function MissingRow(props: { font: MissingFontDto; catalog: FontChoice[] }) {
  const ed = useEditor();
  const [family, setFamily] = createSignal('Inter');
  const choice = () => props.catalog.find((c) => c.family === family());
  const [style, setStyle] = createSignal<string | null>(null);
  const effectiveStyle = () => style() ?? defaultStyle(choice(), props.font.style);
  const bundled = () => props.catalog.filter((c) => c.source === 'bundled');
  const system = () => props.catalog.filter((c) => c.source === 'system');
  const name = () => `${props.font.family} ${props.font.style}`;
  return (
    <div class="fe-missing" role="group" aria-label={name()}>
      <div class="fe-missing__name">
        <span class="fe-ellipsis">{name()}</span>
        <span class="t-muted t-num">{props.font.nodes}</span>
      </div>
      <div class="fe-missing__pick">
        <select
          class="fe-select fe-missing__family"
          aria-label={`Замена для ${name()}: семейство`}
          value={family()}
          onChange={(e) => {
            setFamily(e.currentTarget.value);
            setStyle(null);
          }}
        >
          <optgroup label="Вшитые">
            <For each={bundled()}>{(c) => <option value={c.family}>{c.family}</option>}</For>
          </optgroup>
          <Show when={system().length > 0}>
            <optgroup label="Системные">
              <For each={system()}>{(c) => <option value={c.family}>{c.family}</option>}</For>
            </optgroup>
          </Show>
        </select>
        <select
          class="fe-select fe-missing__style"
          aria-label={`Замена для ${name()}: начертание`}
          value={effectiveStyle()}
          onChange={(e) => setStyle(e.currentTarget.value)}
        >
          <For each={choice()?.styles ?? []}>{(s) => <option value={s}>{s}</option>}</For>
        </select>
      </div>
      <button
        class="fe-btn fe-btn--soft fe-btn--wide"
        disabled={!choice() || !effectiveStyle()}
        onClick={() =>
          ed.send({
            type: 'replaceMissingFont',
            from: { family: props.font.family, style: props.font.style },
            to: { family: family(), style: effectiveStyle() },
          })
        }
      >
        Заменить
      </button>
    </div>
  );
}

export function MissingFontsSection() {
  const ed = useEditor();
  const list = () => ed.store.state.missingFonts;
  const catalog = createMemo(() => [...ed.store.state.fontCatalog].sort((a, b) => a.family.localeCompare(b.family)));
  const sys = () => ed.store.state.systemFonts;
  return (
    <Show when={list().length > 0}>
      <Section title={`Отсутствующие шрифты (${list().length})`}>
        <For each={list()}>{(f) => <MissingRow font={f} catalog={catalog()} />}</For>
        <Show when={sys().supported && !sys().shown}>
          <button class="fe-btn fe-btn--soft fe-btn--wide" onClick={() => ed.showSystemFonts()}>
            Показать системные шрифты
          </button>
        </Show>
      </Section>
    </Show>
  );
}
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
import { Menu, type MenuItem } from './menu';
import { NumberField, type EditTarget } from './number-field';
import type { FieldInput } from './number';
import { FillsSection, PageSection, StrokesSection } from './paints';

/** Правка; `target` — кого правят с начала жеста (без него — текущее выделение). */
````

на:

````tsx
import { Menu, type MenuItem } from './menu';
import { NumberField, type EditTarget } from './number-field';
import type { FieldInput } from './number';
import { MissingFontsSection } from './missing-fonts';
import { FillsSection, PageSection, StrokesSection } from './paints';

/** Правка; `target` — кого правят с начала жеста (без него — текущее выделение). */
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx

  return (
    <div class="fe-props" id="panel-design" role="tabpanel" aria-labelledby="tab-design" tabindex="-1">
      <Show when={count() > 0} fallback={<PageSection theme={props.theme} />}>
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
        <FillsSection />
````

на:

````tsx

  return (
    <div class="fe-props" id="panel-design" role="tabpanel" aria-labelledby="tab-design" tabindex="-1">
      <Show
        when={count() > 0}
        fallback={
          <>
            <MissingFontsSection />
            <PageSection theme={props.theme} />
          </>
        }
      >
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
        <FillsSection />
````

В `apps/web/src/ui/styles/properties.css` заменить:

````css
.fe-picker .fe-field__label {
  cursor: default;
}
````

на:

````css
.fe-picker .fe-field__label {
  cursor: default;
}

/* Отсутствующие шрифты: имя и число слоёв, выбор семейства и начертания, «Заменить». */
.fe-missing {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding-bottom: var(--space-2);
}
.fe-missing__name {
  display: flex;
  justify-content: space-between;
  gap: var(--space-2);
  min-width: 0;
}
.fe-missing__pick {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-2);
}
.fe-missing__pick .fe-select {
  width: 100%;
  font: inherit;
}
````

- [ ] **Step 5: Проверить**

Run: `cd apps/web && npm run typecheck && npm run unit && npm run build && npx playwright test e2e/text.spec.ts`
Expected: PASS: модульные — 118; E2E — 5: в `chromium` 4 (вшитый Inter; отсутствующий «Comic Sans Bold» — секция «Отсутствующие шрифты (1)», выбор Roboto даёт начертание Bold, «Заменить» — один шаг, после загрузки Roboto секции нет, Ctrl/Cmd+Z возвращает; без тестового режима `testCreateText` — «Неверное сообщение для ядра» и узла нет; системный шрифт: разрешение `local-fonts` выдано после запуска, текст шрифтом системы — секция (1), «Показать системные шрифты» — список пришёл, секции и кнопки нет, текст нарисован байтами, прочитанными worker'ом ввода-вывода), в `firefox` 1 (Inter рисуется, кнопки системных шрифтов нет). Без установленного Firefox для Playwright — 4 и предупреждение конфигурации.

Ручная проверка (Chrome): `npm run dev`, открыть `/?debug#/new` (тестовый режим: ядро в `window.__ferrum`), в консоли `__ferrum.core.send({ type: 'testCreateText', x: 0, y: 0, text: 'Проверка', font: { family: 'Helvetica', style: 'Bold' } })`, снять выделение — секция «Отсутствующие шрифты (1)»; «Показать системные шрифты» — запрос разрешения браузера; после согласия секция пропадает, текст — Helvetica Bold.

- [ ] **Step 6: Закоммитить**

```bash
git add apps/web/src apps/web/unit apps/web/e2e/text.spec.ts
git commit -m "feat(web): системные шрифты Chrome (queryLocalFonts, байты через worker ввода-вывода) и десктопа, «Отсутствующие шрифты (N)» с заменой одним шагом

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Десктоп: системные шрифты из каталогов ОС

Спека 4: «Tauri — сканирование системных каталогов шрифтов в Rust (`/Library/Fonts`, `~/Library/Fonts`, `/System/Library/Fonts`; на Windows/Linux — их каталоги), метаданные — `skrifa`; байты — сырым ответом IPC». Сканирование — при первом `fonts_list`, в потоке Tauri (`async`-команда, как остальные, что трогают диск), результат держится до выхода: файлы `.ttf`/`.otf`/`.ttc`/`.otc` до 20 000, больше 64 МБ и неразборчивые — пропуск с сообщением в журнале (stderr). Интерфейс путей не видит: файл начертания — номер файла в списке процесса; `fonts_read` отдаёт байты по номеру. Метаданные — `ferrum-fontmeta`, как у манифеста вшитых (одни имена начертаний).

**Files:**
- Create: `apps/desktop/src-tauri/src/fonts.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs` (`Desk` 32–38, `open_desk` — поля, команды до `smoke`, `generate_handler!` 427–450), `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/Cargo.lock`

**Interfaces:**
- Consumes: `ferrum_fontmeta::{read_faces, group, FamilyInfo, MAX_FONT_FILE}` (Task 3); вызовы `systemFontList`/`systemFontBytes` интерфейса (Task 9).
- Produces: команды IPC `fonts_list() -> FamilyInfo[]` (JSON, `camelCase`, `file` — номер файла строкой) и `fonts_read({ id: number }) -> ArrayBuffer` (сырой ответ); `fonts::{SystemFonts::scan, SystemFonts::read, font_dirs, MAX_FONT_FILES}`.

- [ ] **Step 1: Падающий тест сканирования**

Создать `apps/desktop/src-tauri/src/fonts.rs`:

````rust
//! Системные шрифты десктопа (спека подсистемы 3, раздел 4): каталоги шрифтов
//! ОС сканируются в Rust, метаданные — `skrifa` (`ferrum-fontmeta`, как у
//! манифеста вшитых), байты — сырым ответом IPC. Интерфейс путей не видит: файл
//! начертания — номер файла в списке этого процесса.

use std::path::{Path, PathBuf};

use ferrum_fontmeta::{group, read_faces, FamilyInfo, MAX_FONT_FILE};

/// Наибольшее число файлов шрифтов при сканировании (страховка от огромных каталогов).
pub const MAX_FONT_FILES: usize = 20_000;

/// Список системных шрифтов: файлы и семейства (номер файла — `FaceInfo::file`).
#[derive(Default)]
pub struct SystemFonts {
    files: Vec<PathBuf>,
    pub families: Vec<FamilyInfo>,
    /// Пропущенные файлы (неразборчивые, больше 64 МБ) — для журнала ошибок.
    pub skipped: Vec<String>,
}

/// Каталоги шрифтов ОС.
pub fn font_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs = Vec::new();
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Some(h) = &home {
            dirs.push(h.join("Library/Fonts"));
        }
    } else if cfg!(target_os = "windows") {
        if let Some(w) = std::env::var_os("WINDIR") {
            dirs.push(PathBuf::from(w).join("Fonts"));
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(l).join("Microsoft/Windows/Fonts"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(h) = &home {
            dirs.push(h.join(".local/share/fonts"));
            dirs.push(h.join(".fonts"));
        }
    }
    dirs
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| Some(e.ok()?.path())).collect();
    entries.sort();
    for p in entries {
        if out.len() >= MAX_FONT_FILES {
            return;
        }
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "otc"
            )
        }) {
            out.push(p);
        }
    }
}

impl SystemFonts {
    /// Сканирует каталоги: каждый файл читается один раз ради метаданных.
    pub fn scan(dirs: &[PathBuf]) -> SystemFonts {
        let mut paths = Vec::new();
        for d in dirs {
            collect(d, &mut paths);
        }
        let mut out = SystemFonts::default();
        let mut faces = Vec::new();
        for path in paths {
            let shown = path.display().to_string();
            match std::fs::metadata(&path) {
                Ok(m) if m.len() as usize > MAX_FONT_FILE => {
                    out.skipped
                        .push(format!("Шрифт пропущен: {shown} больше 64 МБ"));
                    continue;
                }
                Err(_) => continue,
                _ => {}
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let id = out.files.len().to_string();
            let found = read_faces(&bytes, &id);
            if found.is_empty() {
                out.skipped
                    .push(format!("Шрифт пропущен: {shown} не читается"));
                continue;
            }
            out.files.push(path);
            faces.extend(found);
        }
        out.families = group(faces);
        out
    }

    /// Байты файла по номеру из списка.
    pub fn read(&self, id: usize) -> Result<Vec<u8>, String> {
        let path = self.files.get(id).ok_or("Неверный номер файла шрифта")?;
        std::fs::read(path).map_err(|e| format!("Не удалось прочитать шрифт: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_fonts_skips_garbage_and_reads_by_id() {
        let dir = std::env::temp_dir().join(format!("ferrum-fonts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/public/fonts");
        std::fs::copy(web.join("ptsans/PTSans-Regular.ttf"), dir.join("a.ttf")).unwrap();
        std::fs::copy(web.join("ptsans/PTSans-Bold.ttf"), dir.join("nested/b.TTF")).unwrap();
        std::fs::write(dir.join("broken.otf"), b"garbage").unwrap();
        std::fs::write(dir.join("readme.txt"), b"not a font").unwrap();
        let fonts = SystemFonts::scan(std::slice::from_ref(&dir));
        assert_eq!(fonts.families.len(), 1);
        assert_eq!(fonts.families[0].family, "PT Sans");
        let styles: Vec<&str> = fonts.families[0]
            .faces
            .iter()
            .map(|f| f.style.as_str())
            .collect();
        assert_eq!(styles, vec!["Regular", "Bold"]);
        assert_eq!(fonts.skipped.len(), 1, "{:?}", fonts.skipped);
        let id: usize = fonts.families[0].faces[1].file.parse().unwrap();
        assert_eq!(
            fonts.read(id).unwrap().len(),
            std::fs::metadata(dir.join("nested/b.TTF")).unwrap().len() as usize
        );
        assert!(fonts.read(99).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
````

В `apps/desktop/src-tauri/Cargo.toml` заменить:

````toml

[dependencies]
ferrum-desktop-store = { path = "store" }
tauri = { version = "=2.12.1", features = [] }
tauri-plugin-dialog = "=2.8.1"
serde = { version = "=1.0.229", features = ["derive"] }
````

на:

````toml

[dependencies]
ferrum-desktop-store = { path = "store" }
# Метаданные системных шрифтов — тот же разбор, что у манифеста вшитых.
ferrum-fontmeta = { path = "../../../crates/ferrum-fontmeta" }
tauri = { version = "=2.12.1", features = [] }
tauri-plugin-dialog = "=2.8.1"
serde = { version = "=1.0.229", features = ["derive"] }
````

Run: `cd apps/desktop/src-tauri && cargo fetch && cargo update -p font-types --precise 0.12.6 && cargo test --lib fonts`
Expected: `running 0 tests` — модуль ещё не подключён (`mod fonts` — в Step 2), тест не запускается.

- [ ] **Step 2: Команды IPC**

В `apps/desktop/src-tauri/src/lib.rs` заменить:

````rust
//! приложение печатает его, убирает свой временный каталог и выходит (код 0 — ядро
//! запустилось и круг прошёл), без ответа за 90 с — выход с кодом 1.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use ferrum_desktop_store::{io_error, FileMeta, Library, ListResult, ReadParts, Section, Settings};
use serde::{Deserialize, Serialize};
````

на:

````rust
//! приложение печатает его, убирает свой временный каталог и выходит (код 0 — ядро
//! запустилось и круг прошёл), без ответа за 90 с — выход с кодом 1.

mod fonts;

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use ferrum_fontmeta::FamilyInfo;

use ferrum_desktop_store::{io_error, FileMeta, Library, ListResult, ReadParts, Section, Settings};
use serde::{Deserialize, Serialize};
````

В `apps/desktop/src-tauri/src/lib.rs` заменить:

````rust
    settings: Mutex<Settings>,
    /// Части чтений файла, ещё не забранные интерфейсом (`files_read_part`).
    parts: Mutex<ReadParts>,
}

type Res<T> = Result<T, String>;
````

на:

````rust
    settings: Mutex<Settings>,
    /// Части чтений файла, ещё не забранные интерфейсом (`files_read_part`).
    parts: Mutex<ReadParts>,
    /// Системные шрифты (подсистема 3): сканируются при первом `fonts_list`.
    fonts: OnceLock<fonts::SystemFonts>,
}

type Res<T> = Result<T, String>;
````

В `apps/desktop/src-tauri/src/lib.rs` заменить:

````rust
    desk.settings().set(patch)
}

fn smoke() -> bool {
    std::env::var("FERRUM_SMOKE").is_ok_and(|v| v == "1")
}
````

на:

````rust
    desk.settings().set(patch)
}

/// Системные шрифты: семейства и начертания (файл — номер в списке процесса).
/// Первый вызов сканирует каталоги шрифтов ОС — в потоке Tauri, не в окне.
#[tauri::command(async)]
fn fonts_list(desk: State<'_, Desk>) -> Res<Vec<FamilyInfo>> {
    let fonts = desk.fonts.get_or_init(|| {
        let f = fonts::SystemFonts::scan(&fonts::font_dirs());
        for s in &f.skipped {
            eprintln!("{s}");
        }
        f
    });
    Ok(fonts.families.clone())
}

/// Байты файла системного шрифта сырым ответом (без JSON и копий в JS).
#[tauri::command(async)]
fn fonts_read(id: usize, desk: State<'_, Desk>) -> Res<Response> {
    let fonts = desk
        .fonts
        .get()
        .ok_or("Список системных шрифтов ещё не построен")?;
    Ok(Response::new(fonts.read(id)?))
}

fn smoke() -> bool {
    std::env::var("FERRUM_SMOKE").is_ok_and(|v| v == "1")
}
````

В `apps/desktop/src-tauri/src/lib.rs` заменить:

````rust
        lib: Mutex::new(lib),
        settings: Mutex::new(settings),
        parts: Mutex::new(ReadParts::default()),
    })
}

````

на:

````rust
        lib: Mutex::new(lib),
        settings: Mutex::new(settings),
        parts: Mutex::new(ReadParts::default()),
        fonts: OnceLock::new(),
    })
}

````

В `apps/desktop/src-tauri/src/lib.rs` заменить:

````rust
            files_copy_dialog,
            settings_get,
            settings_set,
            smoke_mode,
            smoke_done,
        ])
````

на:

````rust
            files_copy_dialog,
            settings_get,
            settings_set,
            fonts_list,
            fonts_read,
            smoke_mode,
            smoke_done,
        ])
````

- [ ] **Step 3: Проверить**

Run: `cd apps/desktop/src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib fonts && cargo test -p ferrum-desktop-store`
Expected: PASS: `fonts::tests::scans_fonts_skips_garbage_and_reads_by_id` — копии вшитых PT Sans Regular и Bold во временном каталоге (одна во вложенном, с расширением в верхнем регистре) — одно семейство «PT Sans» с двумя начертаниями, испорченный `.otf` — одно сообщение о пропуске, `.txt` не читается вовсе, `read` по номеру — байты файла, чужой номер — ошибка; хранилище — как прежде (9). В `Cargo.lock` десктопа — `ferrum-fontmeta`, `skrifa 0.44.0`, `read-fonts 0.41.0`, `font-types 0.12.6`, `bytemuck 1.25.2`.

Run: `cd apps/web && npm run build && cd ../desktop/src-tauri && cargo build && FERRUM_SMOKE=1 ./target/debug/ferrum-desktop`
Expected: `smoke: {…"ready":true…}`, код выхода 0 (команды шрифтов не мешают запуску).

Ручная проверка (macOS): в Chrome сделать файл с текстом шрифтом Helvetica (ручная проверка Task 9) и «Сохранить на диск…»; в десктопе (`cd apps/desktop && npx tauri dev`) — «Открыть с диска…» этот файл: секции «Отсутствующие шрифты» нет, текст — Helvetica (байты пришли ответом `fonts_read`).

- [ ] **Step 4: Закоммитить**

```bash
git add apps/desktop/src-tauri
git commit -m "feat(desktop): системные шрифты — сканирование каталогов ОС в Rust (метаданные ferrum-fontmeta), fonts_list и fonts_read сырым ответом IPC

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Замеры: 10 000 текстов

Спека 5: «10 000 текстовых слоёв на странице открываются быстрее 2 с, панорамирование по ним — 60 fps (замер в плане этапа 1)». Два замера: `render_report` (нативно, release) — открытие файла из 10 000 текстов до сцены (по кэшу сцены: раскладка не нужна, ≤ 1000 мс нативно — wasm ожидается в 1,5–2 раза медленнее), первый кадр с ленивой раскладкой (≤ 16,7 мс), p95 кадра при панорамировании на 100 % (≤ 16,7 мс), вся страница издалека (≤ 8 мс); E2E `@perf` в браузере (`vello_cpu`) — стенд `?texts=10000`: от перехода до первого кадра < 2000 мс, 3 с панорамирования колесом — p95 времени кадра worker'а ≤ 16,7 мс.

**Files:**
- Modify: `crates/ferrum-render/examples/render_report.rs` (перед экспортом @3x и `main` 165), `apps/web/e2e/perf.spec.ts` (в конец)

**Interfaces:**
- Consumes: `fixture::fill_texts` (Task 2), `Renderer::set_fonts` (Task 5), `DemoText` и `?texts=` (Tasks 7, 8).
- Produces: строки отчёта «Текст: …» и тест «10 000 текстов: открытие и панорамирование @perf» (`test-results/chromium/perf-text.json`).

- [ ] **Step 1: Замер в `render_report`**

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
        })
        .collect();
    (median(runs), stats)
}

fn main() {
````

на:

````rust
        })
        .collect();
    (median(runs), stats)
}

/// Вшитый Inter из `apps/web/public/fonts` в общем реестре — как его грузит ядро.
fn inter_fonts() -> ferrum_render::SharedFonts {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/public/fonts");
    let fonts = ferrum_render::Fonts::shared();
    let mut f = fonts.borrow_mut();
    let files = [
        "inter/Inter-Variable.ttf",
        "inter/Inter-Italic-Variable.ttf",
    ];
    let mut faces = Vec::new();
    for rel in files {
        faces.extend(ferrum_text::read_faces(
            &std::fs::read(dir.join(rel)).unwrap(),
            rel,
        ));
    }
    f.add_catalog(ferrum_text::SourceKind::Bundled, ferrum_text::group(faces));
    for rel in files {
        f.load("Inter", rel, std::fs::read(dir.join(rel)).unwrap())
            .unwrap();
    }
    drop(f);
    fonts
}

/// Текст (подсистема 3, этап 1): страница из 10 000 текстов открывается без
/// раскладки (размеры — из документа), раскладка ленивая (при первом рисовании,
/// в пределах времени кадра), панорамирование по разложенным — 60 fps.
fn text_section(report: &mut Report) {
    const TEXTS: usize = 10_000;
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.transact(page, vec![], |p| {
        ferrum_doc::fixture::fill_texts(p, TEXTS).map(|_| vec![])
    })
    .unwrap();
    let bytes = encode(&doc).unwrap();
    let open = median(
        (0..3)
            .map(|_| {
                let t = Instant::now();
                let mut d = decode(&bytes).unwrap().document;
                let scene = Scene::from_page(d.page(page).unwrap());
                std::hint::black_box(scene.len());
                t.elapsed()
            })
            .collect(),
    );
    report.gate(
        "Текст: открытие страницы из 10 000 текстов (файл → сцена)",
        open,
        1000.0,
        "≤ 1000 мс (wasm ≤ 2 с)",
    );
    let mut d = decode(&bytes).unwrap().document;
    let scene = Scene::from_page(d.page(page).unwrap());
    let mut renderer = Renderer::new();
    renderer.set_fonts(inter_fonts());
    let mut camera = Camera::new(VIEWPORT, 1.0);
    camera.set_center(Point::new(720.0, 512.0));
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    // Прогрев: ленивая раскладка видимых текстов догоняет за несколько кадров.
    let mut warm = 0;
    let mut first = Duration::ZERO;
    for i in 0..WARMUP_CAP {
        backend.reset(w, h);
        let t = Instant::now();
        let s = renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
        if i == 0 {
            first = t.elapsed();
        }
        warm = i + 1;
        if !s.needs_another_frame {
            break;
        }
    }
    report.gate(
        "Текст: первый кадр (раскладка в пределах кадра)",
        first,
        16.7,
        "≤ 16.7 мс",
    );
    report.info(
        "Текст: кадров до полной раскладки видимого",
        warm.to_string(),
    );
    // Панорамирование по странице: каждый кадр — сдвиг на 40 px; новые тексты
    // раскладываются на лету.
    let mut times = Vec::new();
    for i in 0..240 {
        camera.set_center(Point::new(
            720.0 + i as f64 * 40.0,
            512.0 + (i % 60) as f64 * 10.0,
        ));
        backend.reset(w, h);
        let t = Instant::now();
        renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
        times.push(t.elapsed());
    }
    times.sort();
    let p95 = times[times.len() * 95 / 100];
    report.gate(
        "Текст: панорамирование, p95 кадра (зум 100 %)",
        p95,
        16.7,
        "≤ 16.7 мс",
    );
    // Вся страница издалека: полосы LOD и растры фреймов.
    camera.set_zoom(0.05);
    camera.set_center(Point::new(40_000.0, 1_500.0));
    let _ = warm_up(&mut renderer, &mut backend, &scene, &camera);
    let (whole, stats) = frame(&mut renderer, &mut backend, &scene, &camera);
    report.info(
        "Текст: вся страница — растров / глифами / полосами",
        format!(
            "{} / {} / {}",
            stats.rasters, stats.text_glyphs, stats.text_greeked
        ),
    );
    report.gate(
        "Текст: вся страница с LOD, сборка кадра",
        whole,
        8.0,
        "≤ 8 мс",
    );
}

fn main() {
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
        );
    }

    // Экспорт фрейма @3x — справочно.
    let first_frame = scene.at(scene.roots()[0]).id;
    let t = Instant::now();
````

на:

````rust
        );
    }

    text_section(&mut report);

    // Экспорт фрейма @3x — справочно.
    let first_frame = scene.at(scene.roots()[0]).id;
    let t = Instant::now();
````

Run: `cargo run --release -p ferrum-render --example render_report`
Expected: все строки PASS; строки текста (Apple M1 Pro): открытие 10 000 текстов ≈ 45 мс, первый кадр ≈ 4,5 мс, кадров до полной раскладки видимого — 1, p95 панорамирования ≈ 3–4 мс, вся страница — 50 растров фреймов, сборка ≈ 0,01 мс.

- [ ] **Step 2: Замер в браузере**

В `apps/web/e2e/perf.spec.ts` заменить:

````ts
  // Рамка выделения во время жеста доходила до интерфейса из общей памяти.
  expect(liveX.size, 'живые значения жеста в панели').toBeGreaterThan(10);
});
````

на:

````ts
  // Рамка выделения во время жеста доходила до интерфейса из общей памяти.
  expect(liveX.size, 'живые значения жеста в панели').toBeGreaterThan(10);
});

// --- Текст (подсистема 3, этап 1) ---------------------------------------------
//
// Страница из 10 000 текстов: открывается быстрее 2 с (размеры — из документа,
// раскладка не нужна), панорамирование по ней — 60 fps (p95 кадра worker'а
// vello_cpu ≤ 16,7 мс): раскладка ленивая — при первом рисовании видимых и в
// пределах времени кадра.

const TEXTS = 10_000;

test('10 000 текстов: открытие и панорамирование @perf', async ({ page }, info) => {
  test.skip(info.project.name !== 'chromium', 'замер текста — в проекте chromium (vello_cpu)');
  const t0 = Date.now();
  await gotoDebug(page, `/debug.html?texts=${TEXTS}&backend=cpu`);
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 120_000 });
  await page.waitForFunction(() => (window.__ferrum!.core!.live?.frames ?? 0) > 0, null, { timeout: 60_000 });
  const openMs = Date.now() - t0;
  // Шрифт страницы (Inter) пришёл: кадры с глифами, а не пустые.
  await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'fontCatalog'), null, { timeout: 30_000 });
  await page.waitForTimeout(500);
  await page.evaluate(() => {
    const w = window as unknown as { __frameMs: number[] };
    w.__frameMs = [];
    let seen = window.__ferrum!.core!.live!.frames;
    const tick = () => {
      const l = window.__ferrum!.core!.live!;
      if (l.frames !== seen) {
        seen = l.frames;
        w.__frameMs.push(l.frameMs);
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
  const l = await live(page);
  await page.mouse.move(l.viewportW / 2, l.viewportH / 2);
  await during(page, 3000, (i) => page.mouse.wheel(i % 120 < 60 ? 15 : -15, i % 40 < 20 ? 10 : -10));
  const frames = await page.evaluate(() => [...(window as unknown as { __frameMs: number[] }).__frameMs].sort((a, b) => a - b));
  const p95 = frames[Math.min(frames.length - 1, Math.round((frames.length - 1) * 0.95))] ?? 0;
  const out = path.join(info.project.outputDir, 'perf-text.json');
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, JSON.stringify({ openMs, frames: frames.length, p95 }, null, 2));
  console.log(`perf text: открытие ${openMs} мс, кадров ${frames.length}, p95 ${p95.toFixed(2)} мс`);
  expect((await live(page)).nodes).toBe(TEXTS + TEXTS / 100);
  expect(openMs, 'открытие 10 000 текстов').toBeLessThan(2000);
  expect(frames.length, 'worker рисовал кадры').toBeGreaterThan(30);
  expect(p95, 'p95 кадра worker\'а').toBeLessThanOrEqual(WORKER_P95_MS);
});
````

Run: `cd apps/web && npm run build && npx playwright test e2e/perf.spec.ts --project=chromium -g "10 000 текстов"`
Expected: PASS, в выводе `perf text: открытие ≈ 500 мс, кадров ≈ 60, p95 ≈ 12 мс` (vello_cpu, headless Chromium на той же машине).

- [ ] **Step 3: Закоммитить**

```bash
git add crates/ferrum-render/examples/render_report.rs apps/web/e2e/perf.spec.ts
git commit -m "test(render,web): замер 10 000 текстов — открытие без раскладки, ленивая раскладка в пределах кадра, панорамирование 60 fps

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Спека и финальная проверка

**Files:**
- Modify: `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md`

**Interfaces:**
- Consumes: всё этапа 1 (Tasks 1–11).
- Produces: спека с решениями этапа 1.

- [ ] **Step 1: Спека — решения этапа 1**

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **История отмены** (ядро §5.5) записывает правку текста дельтой Loro (`diff` между версиями контейнера) и откатывает обратной дельтой; ввод подряд в одном месте — один шаг (как в Figma: шаг рвётся паузой > 1 с, сменой позиции курсора или стиля).
````

на:

````markdown
- **История отмены** (ядро §5.5) записывает правку текста дельтой Loro и откатывает обратной дельтой — её строит модель перед применением правки из текущего текста (`slice_delta` удаляемого и переразмечаемого), поэтому шаг переживает пересоздание узла отменой удаления под новым `TreeID`; ввод подряд в одном месте — один шаг (как в Figma: шаг рвётся паузой > 1 с, сменой позиции курсора или стиля): `Document::transact_merging` с меткой `TextMerge`.
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **Журнал** (ядро §10): правки текста — обычные обновления Loro, журнал их пишет без изменений.
````

на:

````markdown
- **Журнал** (ядро §10): правки текста — обычные обновления Loro, журнал их пишет без изменений.
- **Сделано (этап 1):** текст узла всегда кончается переводом строки (на нём — разметка последнего абзаца, как в Quill; раскладка его не рисует, удалить его и вставить после него нельзя); позиции — символы Unicode (так считает `LoroText` без фичи `wasm` у Loro; перевод из UTF-16 поля ввода — этап 2). Цвет текста по умолчанию — список `fills` узла (как «Заливка» текстового слоя в Figma), разметка `fill` — заливка участка; остальные умолчания стиля и свойства текста пишутся ключами карты узла все. Расширение разметки: символьная — `After`, абзацная (`list`, `indent`) — `None`; настройку ставит каждый `LoroDoc` страницы. Кэш сцены — версия 3 (строка текста несёт свойства и содержимое). Узел текста копируется в буфер обмена с содержимым (поле `text` с JSON текста; версия формата прежняя). Совместимость со сборкой без текста проверяет `scripts/compat-old-build.sh`.
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **Отсутствующий шрифт:** текст раскладывается запасным шрифтом (Inter того же начертания, иначе Inter Regular), имя в документе не меняется, правки текста без смены шрифта его сохраняют. Узел помечается как «шрифт отсутствует»; в панели свойств — «Отсутствующие шрифты (N)» со списком и заменой одним действием (одна правка — один шаг отмены). Шрифт появился (разрешили системные, установили в Tauri) — пометка снимается, текст раскладывается заново.
````

на:

````markdown
- **Отсутствующий шрифт:** текст раскладывается запасным шрифтом (Inter того же начертания, иначе Inter Regular), имя в документе не меняется, правки текста без смены шрифта его сохраняют. Узел помечается как «шрифт отсутствует»; в панели свойств — «Отсутствующие шрифты (N)» со списком и заменой одним действием (одна правка — один шаг отмены). Шрифт появился (разрешили системные, установили в Tauri) — пометка снимается, текст раскладывается заново.
- **Сделано (этап 1):** вшитые файлы — google/fonts на закреплённом коммите (`apps/web/scripts/fetch-fonts.mjs` с SHA-256, `OFL.txt` рядом), манифест — `cargo run -p ferrum-text --example font_manifest`; Onest вшит в wasm и в `public/fonts` не лежит. Вшитые файлы worker ядра качает сам (`/fonts/…`); до списка вшитых отсутствие не определяется. Пока байты шрифта не пришли, текст не рисуется (без мелькания запасным). «Отсутствующие шрифты (N)» — шрифты текущей страницы, на панели «Дизайн» без выделения; замена — на текущей странице, выбор `<select>` семейства и начертания; там же до выбора шрифта этапа 2 — «Показать системные шрифты». Метаданные файлов — крейт `ferrum-fontmeta` (и для манифеста, и для десктопа); десктоп сканирует каталоги при первом `fonts_list`. Запасных шрифтов для отдельных символов нет.
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **Ленивая раскладка:** сцена (ядро §6) знает границы текста из документа, поэтому открытие страницы не раскладывает тексты; раскладка считается при первом рисовании узла на экране и при правке. Ориентир: 10 000 текстовых слоёв на странице открываются быстрее 2 с, панорамирование по ним — 60 fps (замер в плане этапа 1).
````

на:

````markdown
- **Ленивая раскладка:** сцена (ядро §6) знает границы текста из документа, поэтому открытие страницы не раскладывает тексты; раскладка считается при первом рисовании узла на экране и при правке. Ориентир: 10 000 текстовых слоёв на странице открываются быстрее 2 с, панорамирование по ним — 60 fps (замер в плане этапа 1).
- **Сделано (этап 1):** раскладка — по абзацам (свой `parley::Layout` на абзац: отступ списка — 1,5 кегля на уровень, маркеры «•» и «N.», отступ первой строки, интервал), длинное слово переносится по буквам; кэш раскладок — у рисовальщика по слоту (`version` узла и поколение реестра), раскладки за кадр — в пределах 4 мс, остальное — полосами по оценке строк. Замер (`render_report`, нативно, release, M1 Pro): открытие файла из 10 000 текстов до сцены ≈ 45 мс, первый кадр ≈ 4,5 мс, p95 панорамирования ≈ 3–4 мс; в браузере (`@perf`, vello_cpu): до первого кадра ≈ 0,5 с, p95 кадра ≈ 12 мс. Ресайз рамки текста на этапе 1 переводит его в `fixed` (правила Figma для горизонтального ресайза — этап 2).
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **LOD:** строка ниже 3 px на экране рисуется полосой цвета текста с прозрачностью 0.25 по прямоугольнику строки (как Figma при отдалении); растровый кэш фреймов верхнего уровня (ядро §8.4) работает и с текстом.
````

на:

````markdown
- **LOD:** строка ниже 3 px на экране рисуется полосой цвета текста с прозрачностью 0.25 по прямоугольнику строки (как Figma при отдалении; полоса — 60 % высоты строки, узел меньше 3 px — прямоугольник того же цвета); растровый кэш фреймов верхнего уровня (ядро §8.4) работает и с текстом. Обводки текста на этапе 1 не рисуются.
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- Пределы (ядро §4.3, недоверенный ввод): `data` ≤ 64 КБ за команду, имя семейства ≤ 256 байт, байты шрифта ≤ 64 МБ, `patch` — только известные ключи с проверкой значений.
````

на:

````markdown
- Пределы (ядро §4.3, недоверенный ввод): `data` ≤ 64 КБ за команду, имя семейства ≤ 256 байт, байты шрифта ≤ 64 МБ, `patch` — только известные ключи с проверкой значений.
- **Сделано (этап 1):** `FontsAvailable { source: bundled | system, families: [{ family, faces: [{ style, weight?, italic?, stretch?, file, index? }] }] }` (у системных Chrome вес и курсив ядро угадывает по имени начертания), `LoadFont { family, file, bytes? }` — по одному файлу (без байтов — файл не получить), `FontRequest { family, source, files }` (вшитые выполняет worker ядра, системные — основной поток), `MissingFonts { list: [{ family, style, nodes }] }`, `ReplaceMissingFont { from: { family, style }, to: { family, style } }`; сверх списка — события `FontCatalog { families: [{ family, source, styles }] }` (выбор замены) и `Log { message }` (журнал ошибок), команды `DemoText { texts }` (замер) и `TestCreateText` (только тестовый режим страницы). Правки, которым нужны ещё не загруженные шрифты, ядро держит в очереди и выполняет по приходу байтов.
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- Совместимость: файл с текстом открывается в сборке подсистемы 1 только для чтения (тест на документе, собранном новой сборкой).
````

на:

````markdown
- Совместимость: файл с текстом открывается в сборке подсистемы 1 только для чтения (тест на документе, собранном новой сборкой): `scripts/compat-old-build.sh [ref]` собирает `ferrum-doc` из `ref` (по умолчанию `main`) во временной копии и открывает им файл примера `text_fixture`.
````

- [ ] **Step 2: Полная проверка**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings && cargo test --workspace`
Expected: PASS, без предупреждений.

Run: `cargo run --release -p ferrum-render --example render_report && ./scripts/compat-old-build.sh main`
Expected: все строки PASS; «Совместимость с main: файл с текстом открыт только для чтения».

Run: `cd apps/web && npm run wasm && npm run typecheck && npm run unit && npm run build && npm run e2e:ci && npx playwright test e2e/text.spec.ts --project=firefox`
Expected: модульные — 118; `e2e:ci` — 295 passed (294 прежних и новых плюс системные шрифты); Firefox — 1.

Run: `cd apps/desktop/src-tauri && cargo test --lib fonts && cargo test -p ferrum-desktop-store`
Expected: PASS.

- [ ] **Step 3: Ручная проверка**

1. Chrome: `/?debug#/new`, тексты командой `testCreateText` разными шрифтами и размерами; зум до 5 % — полосы вместо букв; экспорт PNG выделенного текста @2x — текст в файле; превью файла на экране файлов — с текстом.
2. Отсутствующий шрифт и замена — по шагу ручной проверки Task 9; Ctrl/Cmd+Z возвращает прежний шрифт и ширину.
3. Firefox: тот же текст вшитым шрифтом рисуется; «Показать системные шрифты» нет.

- [ ] **Step 4: Закоммитить**

```bash
git add docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md
git commit -m "docs: спека подсистемы 3 — решения этапа 1: цвет текста — заливки узла, перевод строки в конце, протокол шрифтов, ресайз в Fixed, отсутствующие шрифты страницы, LOD и ленивая раскладка

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Что этап 1 сознательно не делает

- **Инструмент T, правка на холсте, ввод, IME, курсор и выделение текста, секция «Текст» в свойствах, буфер обмена фрагментов текста** — этап 2 (спека 7, 8). Текстовые узлы до него создаёт тестовая команда, а приходят — из файлов и буфера обмена (узлы целиком).
- **Слияние ввода подряд вызывается только тестами**: `transact_merging` ждёт поля ввода этапа 2.
- **Заливки текста градиентом и изображением, обводки текста** — этап 2 (спека 6: на этапе 1 только цвет).
- **Запасные шрифты для отдельных символов** (CJK, эмодзи, редкие письменности): нет глифа — пустое место. Каталог Google Fonts — подсистема 4.
- **Правила ресайза Figma для текста** (горизонтальный ресайз `AutoWidth` → `AutoHeight`): пока любой ресайз — `Fixed`.
- **Отсутствующие шрифты других страниц**: список и замена — по текущей странице.
- **Пиксели текста на GPU против `vello_cpu`** (проект `gpu`) и проверка десктопа с настоящими системными шрифтами — вручную (Task 10, Step 3; Task 12, Step 3).
