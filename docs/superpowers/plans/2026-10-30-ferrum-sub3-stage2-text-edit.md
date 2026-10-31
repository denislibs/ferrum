# Ferrum, подсистема 3, этап 2: инструмент «Текст» и правка — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Текст правится на холсте как в Figma: инструмент T (щелчок — «авто по ширине», протяжка — фиксированный размер, пустой новый текст исчезает без шага), вход двойным щелчком и Enter, курсор и выделение в ядре (позиции Loro, мигание 1 Гц, выделение `select` 25 %), мышь (протяжка, слово, абзац, Shift+щелчок) и клавиши (кластеры, слова, строки, документ), ввод через скрытое поле основного потока (`beforeinput`, IME — превью подчёркнуто, поле под курсором), ввод подряд — один шаг отмены, буфер обмена фрагментов со стилями и обычного текста, сочетания форматирования, секция «Текст» панели свойств (выбор шрифта с поиском, группами и виртуальным списком, все поля стиля и абзаца, «Смешанные», превью до отпускания), правила ресайза текста Figma, заливки изображением и обводки текста.

**Architecture:** Модель (`ferrum-doc`) получает фрагменты содержимого, вставку участков со своей разметкой и содержимое в патче свойств (`PropsPatch::content` — одна правка со свойствами и превью сцены). Раскладка (`ferrum-text`) отдаёт строки правки (`CaretLine`: позиции курсора по кластерам `parley`) и по ним — попадание, прямоугольники курсора и выделения, ход по строкам; слова и абзацы — по тексту. Редактор (`ferrum-editor`) держит правку (`TextEdit`: якорь, курсор, черновик нового текста, стиль следующего ввода, составление IME, метка склейки) и превращает ввод, клавиши, буфер обмена и правки стиля в транзакции с размером рамки по раскладке; оверлей правки рисует `ferrum-render`. Ядро worker'а (`ferrum-wasm`) принимает команды `TextEditStart`, `TextEditEnd`, `TextInput`, `TextKey`, `SetTextStyle`, `SetTextProps` с проверкой недоверенного ввода и шлёт `TextCaret`, `TextStyleSummary`, `PageFonts`; время ядра (`set_time`) даёт мигание и склейку шагов. Основной поток — только поле `<textarea>` без текста документа и секция панели; правка текста идёт после ввода, положенного в кольцо раньше неё.

**Tech Stack:** Rust 1.96 (внешних крейтов не добавляется: `parley 0.12.0` — кластеры строк `Line::items`, `Run::visual_clusters`, `Cluster::text_range`/`advance`; `loro 1.16.2`; `kurbo`), SolidJS 1.9.17, TypeScript 6.0.3, @playwright/test 1.64.0 (`chromium`, `firefox`).

**Спеки:** `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` — разделы 7 и 8 и части 3, 5, 6, 12, нужные этапу 2; спека ядра `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` («ядро §N»).

## Global Constraints

- Ветка `feat/sub3-text-edit` от `main` (4e11f7f, этап 1 подсистемы 3 слит). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Каталог сборки — у каждой копии свой: `export CARGO_TARGET_DIR=$PWD/target` (в `apps/web` — `$PWD/../../target`); команды — из корня копии, если не сказано иное.
- Новых внешних зависимостей нет: `Cargo.lock` и `package-lock.json` не меняются.
- Основной поток тонкий (ядро §4.2): раскладка, позиции курсора, слова, стиль и сводка — в worker'е ядра. В основном потоке — скрытое поле (без текста документа: после каждого события оно пустое), положение поля по прямоугольнику курсора из ядра, секция панели; чтения layout — только в фазе `measure` планировщика; предпросмотр вшитых шрифтов в выборе шрифта — `FontFace` браузера.
- Недоверенный ввод (спека 8): `TextInput.data` ≤ 64 КБ (UTF-8; проверяют обе стороны), `TextKey.key` ≤ 64 байт, семейство и начертание ≤ 256 байт, размер шрифта 1…10 000, интерлиньяж и трекинг — конечные числа в пределах, ключи правки стиля и свойств — только известные (`deny_unknown_fields`), идентификаторы — как у панели (≤ 1000, ≤ 64 байт). Неверное — `Error` на русском, документ не меняется. Текст сверх пределов в файле (≤ 100 000 символов, ≤ 10 000 участков) открывает документ только для чтения.
- Позиции текста — символы Unicode (как `LoroText` без фичи `wasm`). Поле ввода позиций не передаёт (поле пустое), поэтому перевода из UTF-16 нет вовсе: ядро получает строки и само считает символы.
- Одна правка — один шаг отмены; ввод подряд склеивается (`Document::transact_merging`, `TextMerge`): шаг рвут пауза > 1 с, другая позиция курсора, другой вид правки и другой стиль. Превью (IME, поля панели) — только в сцене до записи.
- Тесты не проходят вхолостую: раскладка — на вшитом Inter, рендер — пиксели и эталоны, редактор — документ и шаги отмены, ядро — события, E2E — содержимое через копию фрагмента (журнал тестового режима), шаги и пиксели экрана.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Сначала прототип всего этапа собран в отдельной копии (`plan-verify-wt2`, ветка `verify/sub3-text-edit`, по коммиту на задачу) и прошёл ворота: `cargo fmt --check`, `clippy -D warnings` (нативно и `wasm32-unknown-unknown`), `cargo test --workspace` (687 тестов), `render_report` (release, все строки PASS), `editor_report` (release, ввод в текст из 10 000 символов ≈ 7,3 мс, ход курсора ≈ 0,02 мс), wasm-сборка, `npm run typecheck`, модульные (128), E2E `chromium` целиком (`npm run e2e:ci` — 307 passed), `text.spec.ts` и `text-edit.spec.ts` в Firefox (3 passed), совместимость со сборкой `main` (`scripts/compat-old-build.sh`). Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом к чистой копии `feat/sub3-text-edit` в порядке задач: каждый фрагмент «заменить» найден ровно один раз, после каждой задачи — её ворота (команды шага «Проверить», включая E2E задач 10 и 11 и Firefox), эталоны — командой шага генерации; после последней задачи дерево совпало с прототипом байт в байт (и эталоны PNG). Найденное прототипом и вошедшее в шаги: `Run::visual_clusters` у прогона строки (`Line::items`) перебирает только кластеры этой строки; двойной щелчок по мелкому тексту попадал в ручки рамки — проверка правки идёт раньше ручек; действие `mousedown` по умолчанию уводило фокус с поля на холст — гасится, пока идёт правка; набор сразу после щелчка обгонял щелчок (команды — сообщениями, указатель — кольцом) — отсюда `Transport.flush` и вычитка кольца; повтор очереди шрифтов переставлял стиль и набор — очередь держит порядок; в замере рендера текста без часов вторая раскладка в кадре под нагрузкой уходила в полосы — тест отключает часы. Не запускались: проект Playwright `gpu` (пиксели WebGPU/WebGL2 против CPU с оверлеем правки и обводками текста), Safari/WebKit, приложение Tauri, настоящий IME ОС (проверен синтетическими событиями составления), замер ввода в wasm (только нативный `editor_report`), ручная проверка (Task 13). Тексты ожидаемых результатов — с этого прогона.

---

## Этапы подсистемы 3

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Текст: модель и рендер (слит) | узел `Text`, шрифты, раскладка, отрисовка, отсутствующие шрифты | 3, 4, 5, 6 |
| **2. Инструмент «Текст» и правка** (этот план) | T, правка на холсте, ввод и IME, курсор, буфер обмена, секция «Текст» | 7, 8 |
| 3–5 | векторы и перо; булевы операции; Scale и экспорт SVG/PDF | 9–11 |

## Решения, которых в спеке нет (на согласование)

Пользователь просил не останавливаться на вопросах: где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**.

1. **(поведение)** Новый текст инструментом T — черновик в сцене до первого символа: в документе и в слоях его нет, шага нет. Первый символ создаёт узел той же транзакцией, что вставляет символ; дальнейший набор склеивается с ней. Esc или щелчок мимо у пустого черновика — он просто исчезает. Стиль, выбранный в панели до набора, — стиль нового узла.
2. **(поведение)** Опустевший существующий текст при выходе удаляется той же склейкой, что его опустошила (Backspace подряд и удаление — один шаг); после посторонней правки — отдельным шагом.
3. **(поведение)** Щелчок T ставит левый верхний угол рамки в точку щелчка; протяжка меньше 1 px — как щелчок. Текст встаёт в самый глубокий фрейм под точкой. После T инструмент — «Перемещение».
4. **(поведение)** Двойной щелчок по тексту ставит курсор в точку (и над ручками рамки мелкого текста); Enter по единственному выделенному тексту — выделяет весь текст. Esc — выход с выделенным узлом; щелчок мимо текста — выход, и щелчок действует как обычно; выбор другого инструмента или выделения в слоях — выход.
5. **(поведение)** Позиции курсора — границы кластеров раскладки (графемы); позиция на переносе строки по умолчанию — в начале нижней строки, после End и щелчка за концом строки — в конце верхней (`upstream`). Вверх с первой строки — в начало текста, вниз с последней — в конец (как в macOS). Слово для Alt/Option(Ctrl)+стрелок и двойного щелчка — буквы, цифры и `_`; двойной щелчок по пробелам или знакам выделяет их серию.
6. **(поведение)** Клавиши: на macOS Option — слово, Cmd+←/→ — строка, Cmd+↑/↓ — текст, Option+↑/↓ — абзац; на Windows и Linux Ctrl+←/→ — слово, Ctrl+Home/End — текст, Ctrl+↑/↓ — абзац; Home/End — строка; PageUp/PageDown — начало и конец текста. Удаление: Option(Ctrl)+Backspace/Delete — слово, Cmd+Backspace/Delete (macOS) — до начала и конца строки.
7. **(поведение)** Стиль набора — символа слева (в начале абзаца — справа); набор поверх выделения — стиль первого выделенного; Enter внутри пункта списка — новый пункт того же списка; Backspace в начале пункта снимает список. Перевод строки — всегда новый абзац (мягкого переноса строки в модели нет; Shift+Enter — тоже абзац).
8. **(поведение)** Ctrl/Cmd+B переключает обычный и жирный (`Regular` ↔ `Bold`, курсив сохраняется), Ctrl/Cmd+I — курсив; начертания нет в семействе — ничего. У курсора без выделения B, I, U, Shift+X задают стиль следующего ввода (документ не меняется). Tab и Shift+Tab — уровень пункта списка (вне списка — ничего). Остальные команды раскладки в правке не действуют; Ctrl/Cmd+Z и Shift+Z (Ctrl+Y) — отмена и повтор в правке, правка продолжается.
9. **(поведение)** Буфер обмена: копия в правке — `text/plain` и наш формат (копия одного текстового узла с выделенным фрагментом; стиль участков — все ключи, а при вставке совпавшие с умолчаниями цели снимаются). В Firefox (асинхронный буфер без своих форматов) — только обычный текст. Фрагмент вне правки вставляется текстом «авто по ширине». Копия с не-текстами во время правки — выход из правки и вставка узлами. Обычный текст вне правки — новый текст в центре видимой части (или выделенного фрейма); длиннее 64 КБ не вставляется (сообщение).
10. **(поведение)** Ресайз по правилам Figma: у «авто по ширине» и «авто по высоте» только ширина (стороны, поле W) — «авто по высоте» с высотой по тексту (перенос строк виден уже в превью жеста), высота, угол и масштаб группы — «фиксированный». Возврат к «авто» из панели подгоняет рамку. «Авто по ширине» с выравниванием по центру растёт в обе стороны, справа — влево.
11. **(поведение)** Секция «Текст» — между «Видом» и «Заливкой»; при наборе нового текста (черновик, выделения нет) панель показывает только её. Выбор шрифта — раскрывающийся список в секции (поиск, группы «В документе», «Вшитые», «Системные», виртуальный список по 28 px; название — своим шрифтом), а не всплывающее окно; «Показать системные шрифты» перенесено туда из отсутствующих шрифтов. Смена семейства сохраняет имя начертания, если оно есть, иначе Regular. Интерлиньяж и трекинг — текстовые поля («Авто», «150%», «20»), применяются Enter или уходом; размер, интервал и отступ абзаца — поля чисел с перетаскиванием (превью до отпускания). Разные значения — «Смешанные» (слово полей панели этапа 5).
12. **(поведение)** Правка из панели в правке текста меняет выделенный диапазон (у курсора — стиль следующего ввода; список — абзац курсора), вне правки — весь текст выделенных узлов: умолчание узла получает значение, разметка этого ключа снимается. Значение, равное умолчанию узла, снимает разметку участка.
13. **(поведение)** Заливки текста рисуются по порядку, изображение — по рамке узла (режимы как у фигур); заливка участка из разметки (цвет или изображение) заменяет заливки узла на участке. Обводки — по контуру глифов: снаружи — двойной толщины под заливкой, по центру — над ней, внутри — двойной толщины с обрезкой по глифам. Текст без видимых заливок глифов не заливает (этап 1 рисовал его чёрным). Градиентов в модели Ferrum нет.
14. Склейка шагов — метками `TextMerge` с отпечатком вида правки и стиля; новый сеанс правки начинает новый шаг (`Document::break_text_merge`). Время — часы ядра (`Core::set_time`: worker передаёт `performance.now()` в начале кадра и перед каждой командой), мигание — по нему же: кадр нужен, только когда курсор гаснет или загорается.
15. Порядок ввода и команд: команды текста идут сообщениями, указатель — кольцом общей памяти. Основной поток перед командой текста досылает ждущий ввод (`Transport.flush`), worker перед ней вычитывает кольцо — щелчок в тексте и набор следом не меняются местами. Правки, ждущие шрифтов (смена шрифта), держат за собой все следующие команды текста.
16. Протокол (спека 8 — без полей): `TextInput { kind: insert | paragraph | paste | compose | commit | deleteBackward | deleteForward | deleteWordBackward | deleteWordForward | deleteLineBackward | deleteLineForward, data? }`, `TextKey { key: KeyboardEvent.code, mods: { shift, alt, ctrl, meta } }`, `TextEditStart { id }`, `TextEditEnd`, `SetTextStyle { patch: { family?, style?, size?, lineHeight?, letterSpacing?, decoration?, case?, list? }, phase, ids? }`, `SetTextProps { patch: { sizing?, alignH?, alignV?, paragraphSpacing?, paragraphIndent? }, phase, ids? }`; поля `range` у `SetTextStyle` нет — диапазон держит ядро (выделение правки). События: `TextCaret { editing, rect: [x, y, w, h]?, selection: [якорь, курсор]?, draft }`, `TextStyleSummary { summary? }` (перечисления — строками), сверх спеки — `PageFonts { families }`; `Clipboard` получил `plain`; курсор `text`; инструмент `text` (команда раскладки `tool.text`, T).
17. Отложенное с этапа 1, взятое сюда: NaN-безопасные числа в раскладке; текст сверх пределов в файле (путь Loro) — только чтение; правила ресайза Figma и режим размера в панели; перенос «авто по ширине» в превью ресайза; обводки текста и заливки участков не только цветом; повторное `set_text_content` с тем же содержимым, разбитым иначе, — без шага. Перевод UTF-16 ↔ символы не нужен (решение 15 Global Constraints).

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-doc/src/{text,types,page,document,history,codec}.rs` | фрагменты и разметка позиции, `normalize`, `PropsPatch::content`, `Tx::insert_runs`, текст сверх пределов в файле, `break_text_merge` |
| `crates/ferrum-text/src/caret.rs` (новый), `layout.rs`, `lib.rs` | строки правки `CaretLine`, `Caret`, попадание, курсор, выделение, ход; слова и абзацы; NaN; краска участка `GlyphRun::paint` |
| `crates/ferrum-render/src/{overlay,text_node,draw,lib}.rs` | оверлей правки текста; заливки и обводки текста |
| `crates/ferrum-editor/src/text_edit.rs` (новый) | правка: вход, выход, курсор, мышь, ход клавишами, мигание, оверлей, состояние для ядра |
| `crates/ferrum-editor/src/text_input.rs` (новый) | ввод, удаление, склейка, IME, черновик T, пустой текст, отмена в правке |
| `crates/ferrum-editor/src/text_clipboard.rs` (новый) | копия фрагмента, вставка со стилями, обычный текст вне правки |
| `crates/ferrum-editor/src/text_style.rs` (новый) | сводка стиля, правка стиля и свойств, сочетания форматирования и списков |
| `crates/ferrum-editor/src/tools/text.rs` (новый), `{editor,text,panel,actions,commands,lib}.rs`, `tools/{mod,select}.rs` | инструмент T, маршрутизация ввода в правку, правила ресайза |
| `crates/ferrum-wasm/src/core/text.rs` (новый), `{protocol,core,core/fonts,core/panels,web/mod}.rs` | команды и события правки текста, пределы, очередь, время |
| `apps/web/src/core/text-keys.ts` (новый), `{core,messages,transport}.ts`, `worker/core.worker.ts` | клавиши поля, предел ввода, порядок ввода и команд текста |
| `apps/web/src/ui/text-input.ts` (новый), `{system-clipboard,store,boot,toolbar,app}.ts(x)`, `styles/app.css` | поле правки, буфер обмена в правке, инструмент T |
| `apps/web/src/ui/{text-panel.ts,font-picker.tsx,text-section.tsx}` (новые), `{properties,missing-fonts,icons}.tsx`, `styles/properties.css` | секция «Текст» и выбор шрифта |
| `apps/web/{e2e,unit}/…` | E2E правки, панели; модульные клавиш и панели |
| `crates/ferrum-editor/examples/editor_report.rs`, спека | замер ввода; решения этапа 2 |

---

### Task 1: Модель: правки текста для редактора

Спека 3 и 7: редактору нужны фрагмент содержимого (копия в буфер), разметка символа в позиции (стиль набора), вставка участков с их разметкой (вставка со стилями) и замена содержимого вместе со свойствами одной правкой — это же поле патча показывает превью IME и панели в сцене. Здесь же два хвоста этапа 1: то же содержимое, разбитое на участки иначе, не даёт пустого шага, а текст сверх пределов, прочитанный из Loro (файл чужой сборки), делает документ только для чтения.

**Files:**
- Modify: `crates/ferrum-doc/src/text.rs`, `types.rs` (`PropsPatch`), `page.rs` (`set_props`, `insert_runs`, `set_text_content`, загрузка), `document.rs` (`page`, журнал), `codec.rs` (`write_props`), `crates/ferrum-editor/src/clipboard.rs` (`full_patch`)
- Test: `crates/ferrum-doc/tests/text_edit.rs` (новый), `crates/ferrum-doc/tests/container.rs`

**Interfaces:**
- Consumes: `Tx`, `TextContent`, `TextRun`, `TextAttrs`, `text_codec::attrs_to_loro` этапа 1.
- Produces: `TextContent::slice(Range<usize>) -> Vec<TextRun>`, `TextContent::attrs_at(usize) -> &TextAttrs`, `TextContent::normalize(&mut self)`; `PropsPatch::content: Option<TextContent>` (в документе — `set_text_content` той же транзакцией; `previous` и `apply_to` — с содержимым); `Tx::insert_runs(id, at, &[TextRun])`; `PageDoc::has_oversized_text()`, `PageDoc::force_text_for_tests(id, &str)` (только тесты).

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-doc/tests/container.rs` заменить:

````rust
                        },
                    }),
                },
            )?;
````

на:

````rust
                        },
                    }),
                    // Содержимое — ниже, своими правками.
                    content: None,
                },
            )?;
````

Создать `crates/ferrum-doc/tests/text_edit.rs`:

````rust
//! Правки текста для редактора (подсистема 3, этап 2): фрагменты содержимого,
//! вставка участков со своей разметкой (буфер обмена), содержимое в патче
//! свойств (превью сцены и правка панели одним шагом), сравнение содержимого без
//! пустых шагов и текст сверх пределов в файле.

use ferrum_doc::*;

fn bold() -> TextAttrs {
    TextAttrs {
        style: Some("Bold".into()),
        ..Default::default()
    }
}

/// Документ с текстом «Привет, мир», где «Привет» — жирным.
fn doc_with_text() -> (Document, PageId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_text_content(id, &TextContent::plain("Привет, мир"))?;
            tx.format_text(id, 0..6, &bold(), &[])?;
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

#[test]
fn slice_keeps_run_attrs_and_attrs_at_reads_one_char() {
    let (mut doc, page, id) = doc_with_text();
    let c = content(&mut doc, page, id);
    let part = c.slice(4..9);
    assert_eq!(part.len(), 2);
    assert_eq!((part[0].text.as_str(), &part[0].attrs), ("ет", &bold()));
    assert_eq!(part[1].text, ", м");
    assert!(part[1].attrs.is_empty());
    // За концом — обрезается, пустой диапазон — пусто.
    assert_eq!(c.slice(10..50)[0].text, "р\n");
    assert!(c.slice(3..3).is_empty());
    assert_eq!(c.attrs_at(0), &bold());
    assert!(c.attrs_at(6).is_empty());
    // За концом — разметка перевода строки.
    assert!(c.attrs_at(1000).is_empty());
}

#[test]
fn normalize_merges_equal_neighbours_and_drops_empty_runs() {
    let mut c = TextContent {
        runs: vec![
            TextRun {
                text: "а".into(),
                attrs: bold(),
            },
            TextRun {
                text: String::new(),
                attrs: TextAttrs::default(),
            },
            TextRun {
                text: "б".into(),
                attrs: bold(),
            },
            TextRun {
                text: "\n".into(),
                attrs: TextAttrs::default(),
            },
        ],
    };
    c.normalize();
    assert_eq!(c.runs.len(), 2);
    assert_eq!(c.runs[0].text, "аб");
}

#[test]
fn insert_runs_keeps_exact_attrs_and_undoes_in_one_step() {
    let (mut doc, page, id) = doc_with_text();
    let red = TextAttrs {
        fill: Some(Fill::solid(Color::rgba(255, 0, 0, 255))),
        ..Default::default()
    };
    let runs = vec![
        TextRun {
            text: "А".into(),
            attrs: red.clone(),
        },
        TextRun {
            text: "Б".into(),
            attrs: TextAttrs::default(),
        },
    ];
    // Вставка сразу за жирным «Привет»: участки — со своей разметкой, не с жирной.
    doc.transact(page, vec![], |tx| {
        tx.insert_runs(id, 6, &runs)?;
        Ok(vec![])
    })
    .unwrap();
    let c = content(&mut doc, page, id);
    assert_eq!(c.text(), "ПриветАБ, мир\n");
    assert_eq!(c.attrs_at(6), &red);
    assert!(c.attrs_at(7).is_empty());
    // За последним переводом строки — нельзя; пустые участки — правки нет.
    let err = doc
        .transact(page, vec![], |tx| {
            tx.insert_runs(id, 14, &runs)?;
            Ok(vec![])
        })
        .unwrap_err();
    assert!(matches!(err, DocError::TextRange(14, 14)));
    doc.undo().unwrap();
    assert_eq!(content(&mut doc, page, id).text(), "Привет, мир\n");
}

#[test]
fn content_in_patch_is_one_undo_step_with_props() {
    let (mut doc, page, id) = doc_with_text();
    let before = doc.page(page).unwrap().props(id).unwrap();
    let mut next = TextContent::plain("Новый");
    next.runs[0].attrs = bold();
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                content: Some(next.clone()),
                w: Some(77.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let after = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(after.w, 77.0);
    assert_eq!(after.text.as_ref().unwrap().content.text(), "Новый\n");
    // Патч применяется к свойствам так же, как в документе (превью сцены).
    let mut preview = before.clone();
    PropsPatch {
        content: Some(next),
        ..Default::default()
    }
    .apply_to(&mut preview);
    assert_eq!(
        preview.text.as_ref().unwrap().content.text(),
        after.text.as_ref().unwrap().content.text()
    );
    // Один шаг: его отмена возвращает и свойства, и содержимое; дальше — только
    // создание узла.
    doc.undo().unwrap();
    assert_eq!(doc.page(page).unwrap().props(id).unwrap(), before);
    doc.undo().unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn same_content_split_differently_makes_no_step() {
    let (mut doc, page, id) = doc_with_text();
    // «Привет» жирным — двумя участками той же разметки и пустой участок.
    let mut split = content(&mut doc, page, id);
    split.runs.remove(0);
    for (i, part) in ["ет", "", "Прив"].into_iter().enumerate() {
        split.runs.insert(
            0,
            TextRun {
                text: part.into(),
                attrs: if i == 1 { TextAttrs::default() } else { bold() },
            },
        );
    }
    assert_eq!(split.text(), "Привет, мир\n");
    doc.transact(page, vec![], |tx| {
        tx.set_text_content(id, &split)?;
        Ok(vec![])
    })
    .unwrap();
    // Шаг только один — создание: после его отмены отменять нечего.
    doc.undo().unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn oversized_text_in_file_makes_page_read_only() {
    let (mut doc, page, id) = doc_with_text();
    let p = doc.page(page).unwrap();
    assert!(!p.has_oversized_text());
    p.force_text_for_tests(id, &"я".repeat(MAX_TEXT_CHARS + 1));
    // Снапшот без кэша сцены — путь чтения из Loro.
    let snapshot = p.export_snapshot().unwrap();
    let reopened = PageDoc::from_snapshot(&snapshot).unwrap();
    assert!(reopened.has_oversized_text());
    let ok = PageDoc::from_snapshot(&PageDoc::new().export_snapshot().unwrap()).unwrap();
    assert!(!ok.has_oversized_text());
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-doc --test text_edit`
Expected: FAIL — не собирается: нет `TextContent::slice`, `TextContent::attrs_at`, `TextContent::normalize`, `Tx::insert_runs`, поля `PropsPatch::content`, `PageDoc::has_oversized_text`

- [ ] **Step 3: Реализация**

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
        strokes: Some(p.strokes.clone()),
        text: p.text.as_ref().map(|t| t.props.clone()),
    };
    write_patch(map, &patch)?;
````

на:

````rust
        strokes: Some(p.strokes.clone()),
        text: p.text.as_ref().map(|t| t.props.clone()),
        // Содержимое пишется ниже, в новый контейнер.
        content: None,
    };
    write_patch(map, &patch)?;
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            let page = PageDoc::load(&u.snapshot, u.scene.as_deref())?;
            // Узлы неизвестного типа скрыты от модели; правка могла бы их потерять.
            if page.has_unknown_nodes() {
                self.read_only = true;
            }
````

на:

````rust
            let page = PageDoc::load(&u.snapshot, u.scene.as_deref())?;
            // Узлы неизвестного типа скрыты от модели; правка могла бы их потерять.
            if page.has_unknown_nodes() || page.has_oversized_text() {
                self.read_only = true;
            }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
                Err(_) => lost = true,
            }
            if page.has_unknown_nodes() {
                self.read_only = true;
            }
````

на:

````rust
                Err(_) => lost = true,
            }
            if page.has_unknown_nodes() || page.has_oversized_text() {
                self.read_only = true;
            }
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
use crate::loro_ext;
use crate::scene_cache::SceneRow;
use crate::text::{AttrKey, TextAttrs, TextContent, TextData, MAX_TEXT_CHARS, MAX_TEXT_RUNS};
use crate::text_codec;
use crate::types::*;
````

на:

````rust
use crate::loro_ext;
use crate::scene_cache::SceneRow;
use crate::text::{
    AttrKey, TextAttrs, TextContent, TextData, TextRun, MAX_TEXT_CHARS, MAX_TEXT_RUNS,
};
use crate::text_codec;
use crate::types::*;
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    /// При загрузке встретились узлы неизвестного типа (см. `rebuild_from_loro`).
    unknown_nodes: Cell<bool>,
    /// Изображения, на которые ссылаются скрытые узлы неизвестного типа и их
    /// потомки. Зеркало их не видит, но при сохранении эти байты терять нельзя.
````

на:

````rust
    /// При загрузке встретились узлы неизвестного типа (см. `rebuild_from_loro`).
    unknown_nodes: Cell<bool>,
    /// Есть текст сверх пределов (спека 3): из файла чужой или старой сборки.
    /// Правка такого текста нарушила бы пределы, поэтому документ — только для чтения.
    oversized_text: Cell<bool>,
    /// Изображения, на которые ссылаются скрытые узлы неизвестного типа и их
    /// потомки. Зеркало их не видит, но при сохранении эти байты терять нельзя.
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
            journal: RefCell::default(),
            unknown_nodes: Cell::default(),
            hidden_refs: RefCell::default(),
            from_cache: Cell::default(),
````

на:

````rust
            journal: RefCell::default(),
            unknown_nodes: Cell::default(),
            oversized_text: Cell::default(),
            hidden_refs: RefCell::default(),
            from_cache: Cell::default(),
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        let mut state = State::default();
        let mut unknown = false;
        let mut hidden_refs = Vec::new();
        let mut stack = self.tree.children(TreeParentId::Root).unwrap_or_default();
````

на:

````rust
        let mut state = State::default();
        let mut unknown = false;
        let mut oversized = false;
        let mut hidden_refs = Vec::new();
        let mut stack = self.tree.children(TreeParentId::Root).unwrap_or_default();
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
                Err(e) => return Err(e),
            };
            state.insert(id, tid, props);
            stack.extend(
````

на:

````rust
                Err(e) => return Err(e),
            };
            oversized |= props
                .text
                .as_ref()
                .is_some_and(|t| t.content.check_limits().is_err());
            state.insert(id, tid, props);
            stack.extend(
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        *self.state.borrow_mut() = state;
        self.unknown_nodes.set(unknown);
        hidden_refs.sort();
        hidden_refs.dedup();
````

на:

````rust
        *self.state.borrow_mut() = state;
        self.unknown_nodes.set(unknown);
        self.oversized_text.set(oversized);
        hidden_refs.sort();
        hidden_refs.dedup();
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    pub fn has_unknown_nodes(&self) -> bool {
        self.unknown_nodes.get()
    }

````

на:

````rust
    pub fn has_unknown_nodes(&self) -> bool {
        self.unknown_nodes.get()
    }

    /// Есть ли на странице текст сверх пределов (`MAX_TEXT_CHARS`, `MAX_TEXT_RUNS`).
    pub fn has_oversized_text(&self) -> bool {
        self.oversized_text.get()
    }

    /// Только для тестов: текст узла в Loro без проверки пределов (как в файле
    /// чужой сборки).
    #[doc(hidden)]
    pub fn force_text_for_tests(&self, id: NodeId, text: &str) {
        let tid = self.tid(id).expect("узел есть");
        let t = text_codec::text_container(&self.tree.get_meta(tid).expect("карта узла"))
            .expect("текст узла");
        t.insert(0, text).expect("вставка в Loro");
        self.doc.commit();
    }

````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    /// записывает обратную операцию: транзакция без изменений шага не создаёт.
    pub(crate) fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        let tid = self.tid(id)?;
        let before = self.props(id)?;
````

на:

````rust
    /// записывает обратную операцию: транзакция без изменений шага не создаёт.
    pub(crate) fn set_props(&self, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
        // Содержимое текста — своей правкой (обратная дельта в журнале отмены),
        // остальное — патчем свойств. У не текста содержимое пропускается.
        if let Some(content) = &patch.content {
            if self.props(id)?.text.is_some() {
                self.set_text_content(id, content)?;
            }
            return self.set_props(
                id,
                &PropsPatch {
                    content: None,
                    ..patch.clone()
                },
            );
        }
        let tid = self.tid(id)?;
        let before = self.props(id)?;
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    }

    /// Удаление диапазона символов. Последний перевод строки не удаляется.
    pub(crate) fn delete_text(&self, id: NodeId, range: Range<usize>) -> DocResult<()> {
````

на:

````rust
    }

    /// Вставка участков с точной разметкой каждого (вставка из буфера обмена со
    /// стилями) в позицию `at`; после последнего перевода строки — нельзя.
    pub(crate) fn insert_runs(&self, id: NodeId, at: usize, runs: &[TextRun]) -> DocResult<()> {
        let len = self.text_len(id)?;
        if at >= len {
            return Err(DocError::TextRange(at, len));
        }
        let mut delta = Vec::new();
        if at > 0 {
            delta.push(TextDelta::Retain {
                retain: at,
                attributes: None,
            });
        }
        for r in runs.iter().filter(|r| !r.text.is_empty()) {
            delta.push(TextDelta::Insert {
                insert: r.text.clone(),
                attributes: Some(text_codec::attrs_to_loro(&r.attrs)),
            });
        }
        if delta.len() == usize::from(at > 0) {
            return Ok(());
        }
        self.apply_text_delta(id, &delta)
    }

    /// Удаление диапазона символов. Последний перевод строки не удаляется.
    pub(crate) fn delete_text(&self, id: NodeId, range: Range<usize>) -> DocResult<()> {
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
            });
        }
        if self.props(id)?.text.as_ref().map(|t| &t.content) == Some(&content) {
            return Ok(());
````

на:

````rust
            });
        }
        // Сравнение — со слитыми участками: иначе то же содержимое, разбитое
        // по-другому, давало бы пустой шаг отмены.
        content.normalize();
        if self.props(id)?.text.as_ref().map(|t| &t.content) == Some(&content) {
            return Ok(());
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        self.page.set_text_content(id, content)
    }
}

````

на:

````rust
        self.page.set_text_content(id, content)
    }

    /// Вставка участков с их разметкой в позицию `at` (символы Unicode).
    pub fn insert_runs(&self, id: NodeId, at: usize, runs: &[TextRun]) -> DocResult<()> {
        self.page.insert_runs(id, at, runs)
    }
}

````

В `crates/ferrum-doc/src/text.rs` заменить:

````rust
    }

    /// Границы участков в символах, по порядку.
    pub fn run_ranges(&self) -> Vec<std::ops::Range<usize>> {
````

на:

````rust
    }

    /// Участки диапазона символов `range` (фрагмент: перевод строки в конце не
    /// добавляется). Диапазон за концом текста обрезается.
    pub fn slice(&self, range: std::ops::Range<usize>) -> Vec<TextRun> {
        let mut out: Vec<TextRun> = Vec::new();
        let mut at = 0;
        for r in &self.runs {
            let n = r.text.chars().count();
            let (lo, hi) = (range.start.max(at), range.end.min(at + n));
            if lo < hi {
                let text: String = r.text.chars().skip(lo - at).take(hi - lo).collect();
                match out.last_mut() {
                    Some(last) if last.attrs == r.attrs => last.text.push_str(&text),
                    _ => out.push(TextRun {
                        text,
                        attrs: r.attrs.clone(),
                    }),
                }
            }
            at += n;
            if at >= range.end {
                break;
            }
        }
        out
    }

    /// Разметка символа в позиции `at`; за концом — разметка последнего символа
    /// (перевода строки).
    pub fn attrs_at(&self, at: usize) -> &TextAttrs {
        static PLAIN: std::sync::OnceLock<TextAttrs> = std::sync::OnceLock::new();
        let mut pos = 0;
        for r in &self.runs {
            let n = r.text.chars().count();
            if at < pos + n {
                return &r.attrs;
            }
            pos += n;
        }
        self.runs
            .last()
            .map(|r| &r.attrs)
            .unwrap_or_else(|| PLAIN.get_or_init(TextAttrs::default))
    }

    /// Соседние участки с одной разметкой сливаются, пустые выбрасываются: так
    /// содержимое из буфера обмена или панели сравнивается с документом.
    pub fn normalize(&mut self) {
        let mut out: Vec<TextRun> = Vec::with_capacity(self.runs.len());
        for r in std::mem::take(&mut self.runs) {
            if r.text.is_empty() {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.attrs == r.attrs => last.text.push_str(&r.text),
                _ => out.push(r),
            }
        }
        self.runs = out;
    }

    /// Границы участков в символах, по порядку.
    pub fn run_ranges(&self) -> Vec<std::ops::Range<usize>> {
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust

use crate::error::DocError;
use crate::text::{TextData, TextProps, TEXT_COLOR};

/// Идентификатор узла. Обёртка над `TreeID` Loro: уникален и при совместной работе.
````

на:

````rust

use crate::error::DocError;
use crate::text::{TextContent, TextData, TextProps, TEXT_COLOR};

/// Идентификатор узла. Обёртка над `TreeID` Loro: уникален и при совместной работе.
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    /// У не текста пропускается.
    pub text: Option<TextProps>,
}

````

на:

````rust
    /// У не текста пропускается.
    pub text: Option<TextProps>,
    /// Содержимое текста целиком (`Tx::set_text_content` той же правкой): превью
    /// сцены показывает составляемый текст IME и стиль участка до отпускания поля
    /// панели. У не текста пропускается.
    pub content: Option<TextContent>,
}

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            text: match (&self.text, &before.text) {
                (Some(_), Some(t)) => Some(t.props.clone()),
                _ => None,
            },
````

на:

````rust
            text: match (&self.text, &before.text) {
                (Some(_), Some(t)) => Some(t.props.clone()),
                _ => None,
            },
            content: match (&self.content, &before.text) {
                (Some(_), Some(t)) => Some(t.content.clone()),
                _ => None,
            },
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            p.strokes = v.clone();
        }
        if let (Some(v), Some(t)) = (&self.text, &mut p.text) {
            Arc::make_mut(t).props = v.clone();
        }
    }
````

на:

````rust
            p.strokes = v.clone();
        }
        if let Some(t) = &mut p.text {
            if self.text.is_some() || self.content.is_some() {
                // Новый `Arc`: общий с зеркалом `TextData` не копируется дважды.
                *t = Arc::new(TextData {
                    props: self.text.clone().unwrap_or_else(|| t.props.clone()),
                    content: self.content.clone().unwrap_or_else(|| t.content.clone()),
                });
            }
        }
    }
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
        strokes: Some(p.strokes.clone()),
        text: p.text.as_ref().map(|t| t.props.clone()),
    }
}
````

на:

````rust
        strokes: Some(p.strokes.clone()),
        text: p.text.as_ref().map(|t| t.props.clone()),
        content: None,
    }
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-doc -p ferrum-scene 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: fmt и clippy без замечаний, `text_edit` — 6 тестов, остальные тесты `ferrum-doc` и `ferrum-scene` — как были

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-doc/src/codec.rs crates/ferrum-doc/src/document.rs crates/ferrum-doc/src/page.rs crates/ferrum-doc/src/text.rs crates/ferrum-doc/src/types.rs crates/ferrum-doc/tests/container.rs crates/ferrum-doc/tests/text_edit.rs crates/ferrum-editor/src/clipboard.rs
git commit -m "feat(doc): правки текста для редактора — фрагменты, вставка участков со своей разметкой, содержимое в патче свойств, текст сверх пределов в файле — только чтение

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2: Раскладка: курсор, попадание, слова

Спека 7: курсор и выделение рисует ядро по раскладке. Раскладка этапа 1 отдаёт только глифы, поэтому каждая строка абзаца получает строку правки `CaretLine`: символы строки (позиции содержимого узла), верх, низ и базовую линию и позиции курсора `(символ, x)` на границах кластеров `parley` (`Line::items` → `Run::visual_clusters` → `Cluster::text_range`, `advance`). Регистр меняет текст до формирования, поэтому он применяется посимвольно, и у каждого символа содержимого известны его байты в тексте раскладки (`ß` → `SS` — одна позиция). Пустой абзац — одна позиция. Сдвиги выравнивания «авто по ширине» и по вертикали двигают и строки правки; обрезка их не трогает. По ним — попадание по точке, отрезок курсора, прямоугольники выделения (за концом абзаца — полоска в четверть строки), соседние позиции, ход по строкам с удержанием x, границы строки; слова и абзацы считаются по тексту. Отложенное с этапа 1: NaN и бесконечность из файла (размер, отступ, интерлиньяж, трекинг) раскладка заменяет умолчаниями.

**Files:**
- Create: `crates/ferrum-text/src/caret.rs`
- Modify: `crates/ferrum-text/src/layout.rs`, `crates/ferrum-text/src/lib.rs`
- Test: `crates/ferrum-text/tests/caret.rs` (новый)

**Interfaces:**
- Produces: `CaretLine { start, end, top, bottom, baseline, stops: Vec<(usize, f64)>, hard }` (`x_of`, `nearest`); `TextLayout::carets`; `Caret { index, upstream }` (`Caret::at`); `TextLayout::{caret_line, caret_rect, hit, selection_rects, next_stop, prev_stop, vertical, line_bounds}`; `word_range`, `next_word_end`, `prev_word_start`, `paragraph_range` (над `&[char]`).

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-text/tests/caret.rs`:

````rust
//! Курсор по раскладке на вшитом Inter (подсистема 3, этап 2): позиции между
//! кластерами, попадание по точке, перенос строк и абзацы, выделение, регистр,
//! выравнивание и защита от NaN из файла.

use std::path::PathBuf;

use ferrum_doc::*;
use ferrum_text::*;
use kurbo::Point;

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Реестр с загруженным вшитым Inter.
fn fonts() -> Fonts {
    let mut fonts = Fonts::new();
    let rel = "inter/Inter-Variable.ttf";
    fonts.add_catalog(SourceKind::Bundled, group(read_faces(&font_file(rel), rel)));
    fonts.load("Inter", rel, font_file(rel)).unwrap();
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

fn lay(fonts: &mut Fonts, t: &TextData, w: f64) -> TextLayout {
    let fills = vec![Fill::solid(Color::BLACK)];
    let l = fonts.layout(&LayoutRequest::for_node(t, &fills, w, 0.0));
    assert!(!l.loading);
    l
}

#[test]
fn one_line_has_a_stop_per_cluster_from_zero_to_width() {
    let mut fonts = fonts();
    let l = lay(&mut fonts, &text("Ferrum", |_| {}), 0.0);
    assert_eq!(l.carets.len(), 1);
    let c = &l.carets[0];
    assert_eq!((c.start, c.end, c.hard), (0, 6, true));
    let idx: Vec<usize> = c.stops.iter().map(|s| s.0).collect();
    assert_eq!(idx, vec![0, 1, 2, 3, 4, 5, 6]);
    assert!(c.stops.windows(2).all(|w| w[1].1 > w[0].1), "{:?}", c.stops);
    assert!(c.stops[0].1.abs() < 1e-6);
    assert!(
        (c.stops[6].1 - l.width).abs() < 0.5,
        "{:?} {}",
        c.stops,
        l.width
    );
    assert!(c.top < c.baseline && c.baseline < c.bottom);
}

#[test]
fn hit_picks_the_nearest_boundary_and_clamps_outside() {
    let mut fonts = fonts();
    let l = lay(&mut fonts, &text("Ferrum", |_| {}), 0.0);
    let c = &l.carets[0];
    let mid = |i: usize| (c.stops[i].1 + c.stops[i + 1].1) / 2.0;
    let y = c.baseline;
    assert_eq!(l.hit(Point::new(mid(2) - 0.5, y)).index, 2);
    assert_eq!(l.hit(Point::new(mid(2) + 0.5, y)).index, 3);
    assert_eq!(l.hit(Point::new(-50.0, -50.0)).index, 0);
    assert_eq!(l.hit(Point::new(1e4, 1e4)).index, 6);
    let (x, top, bottom) = l.caret_rect(Caret::at(3)).unwrap();
    assert_eq!((x, top, bottom), (c.stops[3].1, c.top, c.bottom));
}

#[test]
fn paragraphs_are_lines_and_stops_cross_the_newline() {
    let mut fonts = fonts();
    let l = lay(&mut fonts, &text("ab\ncd", |_| {}), 0.0);
    assert_eq!(l.carets.len(), 2);
    assert_eq!((l.carets[0].start, l.carets[0].end), (0, 2));
    assert_eq!((l.carets[1].start, l.carets[1].end), (3, 5));
    assert!(l.carets[1].top >= l.carets[0].bottom - 1e-6);
    // Ход по кластерам: из конца первого абзаца — в начало второго.
    assert_eq!(l.next_stop(2), 3);
    assert_eq!(l.prev_stop(3), 2);
    assert_eq!(l.next_stop(5), 5);
    assert_eq!(l.prev_stop(0), 0);
    // Выделение через перевод строки — полоска за концом первой строки.
    let r = l.selection_rects(1..4);
    assert_eq!(r.len(), 2);
    assert!(r[0].x1 > l.carets[0].stops.last().unwrap().1);
    assert_eq!(r[1].x0, l.carets[1].stops[0].1);
    // Вверх с первой строки и вниз с последней — нет строки.
    assert!(l.vertical(Caret::at(1), 10.0, -1).is_none());
    assert_eq!(l.vertical(Caret::at(1), 0.0, 1).unwrap().index, 3);
}

#[test]
fn soft_wrap_position_belongs_to_the_next_line_unless_upstream() {
    let mut fonts = fonts();
    let t = text("один два три", |p| {
        p.sizing = TextSizing::AutoHeight
    });
    // Рамка уже двух слов: каждое слово — своя строка.
    let l = lay(&mut fonts, &t, 70.0);
    assert!(l.carets.len() >= 2, "{:?}", l.carets);
    let (first, second) = (&l.carets[0], &l.carets[1]);
    assert_eq!(first.end, second.start);
    assert!(!first.hard);
    let wrap = first.end;
    assert_eq!(l.caret_line(Caret::at(wrap)), Some(1));
    assert_eq!(
        l.caret_line(Caret {
            index: wrap,
            upstream: true
        }),
        Some(0)
    );
    let (home, end) = l.line_bounds(Caret::at(1)).unwrap();
    assert_eq!(home.index, 0);
    assert_eq!((end.index, end.upstream), (wrap, true));
    // Щелчок за концом первой строки — её конец, курсор остаётся на ней.
    let hit = l.hit(Point::new(1e3, first.baseline));
    assert_eq!((hit.index, hit.upstream), (wrap, true));
}

#[test]
fn empty_paragraph_has_one_stop() {
    let mut fonts = fonts();
    let l = lay(&mut fonts, &text("а\n\nб", |_| {}), 0.0);
    assert_eq!(l.carets.len(), 3);
    let e = &l.carets[1];
    assert_eq!((e.start, e.end, e.stops.len()), (2, 2, 1));
    assert!(e.bottom - e.top > 10.0);
    // Пустой текст — одна строка с одной позицией.
    let empty = lay(&mut fonts, &text("", |_| {}), 0.0);
    assert_eq!(empty.carets.len(), 1);
    assert_eq!(empty.carets[0].stops, vec![(0, 0.0)]);
}

#[test]
fn case_transform_keeps_positions_of_document_chars() {
    let mut fonts = fonts();
    // «ß» в верхнем регистре — «SS»: два глифа, одна позиция содержимого.
    let l = lay(
        &mut fonts,
        &text("aßb", |p| p.style.case = TextCase::Upper),
        0.0,
    );
    let idx: Vec<usize> = l.carets[0].stops.iter().map(|s| s.0).collect();
    assert_eq!(idx, vec![0, 1, 2, 3]);
}

#[test]
fn centered_auto_width_paragraphs_shift_carets_with_lines() {
    let mut fonts = fonts();
    let l = lay(
        &mut fonts,
        &text("a\nширокая строка", |p| {
            p.align_h = AlignH::Center
        }),
        0.0,
    );
    let short = &l.carets[0];
    assert!((short.stops[0].1 - l.lines[0].rect.x0).abs() < 0.5);
    assert!(short.stops[0].1 > 10.0);
}

#[test]
fn nan_from_a_file_does_not_reach_the_layout() {
    let mut fonts = fonts();
    let t = text("Ferrum", |p| {
        p.style.size = f64::NAN;
        p.paragraph_indent = f64::NAN;
        p.style.letter_spacing = LetterSpacing::Px(f64::INFINITY);
    });
    let l = lay(&mut fonts, &t, 0.0);
    assert!(l.width.is_finite() && l.width > 0.0 && l.height.is_finite());
    assert!(l
        .carets
        .iter()
        .flat_map(|c| c.stops.iter())
        .all(|s| s.1.is_finite()));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-text --test caret`
Expected: FAIL — не собирается: нет `ferrum_text::Caret`, поля `TextLayout::carets`, `word_range` и соседей

- [ ] **Step 3: Реализация**

Создать `crates/ferrum-text/src/caret.rs`:

````rust
//! Курсор и выделение по раскладке (спека подсистемы 3, раздел 7): позиция по
//! точке, прямоугольник курсора и выделения, ход по кластерам, строкам и словам.
//! Позиции — символы Unicode содержимого узла, как у `LoroText`; координаты —
//! узла (0, 0 — левый верхний угол рамки). Текст справа налево не поддерживается
//! (спека 1: не входит).
//!
//! Позиция на переносе строки — одна и та же для конца верхней строки и начала
//! нижней; `upstream` выбирает верхнюю (курсор после щелчка за концом строки и
//! после End), иначе — нижнюю.

use std::ops::Range;

use kurbo::{Point, Rect};

use crate::layout::{CaretLine, TextLayout};

/// Позиция курсора: символ и сторона переноса строки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Caret {
    pub index: usize,
    /// На переносе строки — в конце верхней строки, а не в начале нижней.
    pub upstream: bool,
}

impl Caret {
    pub fn at(index: usize) -> Caret {
        Caret {
            index,
            upstream: false,
        }
    }
}

/// Сколько добавить к выделению за концом абзаца (перевод строки виден как
/// узкая полоска, как в Figma), в долях высоты строки.
const NEWLINE_SELECTION: f64 = 0.25;

impl CaretLine {
    /// x позиции `index` на строке (за её пределами — ближайший край).
    pub fn x_of(&self, index: usize) -> f64 {
        match self.stops.binary_search_by_key(&index, |s| s.0) {
            Ok(i) => self.stops[i].1,
            Err(0) => self.stops.first().map_or(0.0, |s| s.1),
            // Внутри кластера (несколько символов) — его левый край.
            Err(i) => self.stops[i - 1].1,
        }
    }

    /// Ближайшая к `x` позиция курсора на строке.
    pub fn nearest(&self, x: f64) -> usize {
        let mut best = self.start;
        let mut dist = f64::INFINITY;
        for (c, sx) in &self.stops {
            let d = (sx - x).abs();
            if d < dist {
                (best, dist) = (*c, d);
            }
        }
        best
    }
}

impl TextLayout {
    /// Номер строки позиции: `index` на переносе с `upstream` — верхняя строка.
    pub fn caret_line(&self, caret: Caret) -> Option<usize> {
        let n = self.carets.len();
        if n == 0 {
            return None;
        }
        for (i, l) in self.carets.iter().enumerate() {
            if caret.index < l.end
                || (caret.index == l.end && (l.hard || caret.upstream || i + 1 == n))
            {
                return Some(i);
            }
        }
        Some(n - 1)
    }

    /// Отрезок курсора: x, верх и низ строки.
    pub fn caret_rect(&self, caret: Caret) -> Option<(f64, f64, f64)> {
        let l = &self.carets[self.caret_line(caret)?];
        Some((l.x_of(caret.index), l.top, l.bottom))
    }

    /// Позиция по точке узла: строка — по высоте (выше первой — первая, ниже
    /// последней — последняя), позиция — ближайшая по x.
    pub fn hit(&self, p: Point) -> Caret {
        let n = self.carets.len();
        let Some(i) = self
            .carets
            .iter()
            .position(|l| p.y < l.bottom)
            .or(n.checked_sub(1))
        else {
            return Caret::default();
        };
        let l = &self.carets[i];
        let index = l.nearest(p.x);
        Caret {
            index,
            upstream: index == l.end && !l.hard && i + 1 < n,
        }
    }

    /// Прямоугольники выделения диапазона — по строке на строку.
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Rect> {
        let mut out = Vec::new();
        if range.is_empty() {
            return out;
        }
        for l in &self.carets {
            if l.end < range.start || l.start >= range.end {
                continue;
            }
            // Переход строки переносом принадлежит нижней строке.
            if l.end == range.start && !l.hard {
                continue;
            }
            let x0 = l.x_of(range.start.max(l.start));
            let mut x1 = l.x_of(range.end.min(l.end));
            if l.hard && range.end > l.end {
                x1 += (l.bottom - l.top) * NEWLINE_SELECTION;
            }
            if x1 > x0 {
                out.push(Rect::new(x0, l.top, x1, l.bottom));
            }
        }
        out
    }

    /// Соседняя позиция курсора (граница кластера) после `index`; в конце —
    /// `index`.
    pub fn next_stop(&self, index: usize) -> usize {
        self.carets
            .iter()
            .flat_map(|l| l.stops.iter())
            .map(|s| s.0)
            .find(|c| *c > index)
            .unwrap_or(index)
    }

    /// Соседняя позиция курсора до `index`; в начале — `index`.
    pub fn prev_stop(&self, index: usize) -> usize {
        self.carets
            .iter()
            .rev()
            .flat_map(|l| l.stops.iter().rev())
            .map(|s| s.0)
            .find(|c| *c < index)
            .unwrap_or(index)
    }

    /// Позиция на строке выше (`delta < 0`) или ниже, ближайшая к `x`; за первой
    /// или последней строкой — `None`.
    pub fn vertical(&self, caret: Caret, x: f64, delta: isize) -> Option<Caret> {
        let i = self.caret_line(caret)? as isize + delta;
        if i < 0 || i as usize >= self.carets.len() {
            return None;
        }
        let l = &self.carets[i as usize];
        let index = l.nearest(x);
        Some(Caret {
            index,
            upstream: index == l.end && !l.hard,
        })
    }

    /// Начало и конец видимой строки позиции (Home/End, Cmd/Ctrl+←/→); у конца
    /// перенесённой строки — `upstream`.
    pub fn line_bounds(&self, caret: Caret) -> Option<(Caret, Caret)> {
        let l = &self.carets[self.caret_line(caret)?];
        Some((
            Caret::at(l.start),
            Caret {
                index: l.end,
                upstream: !l.hard,
            },
        ))
    }
}

/// Буква слова: буквы, цифры и подчёркивание.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Класс символа для выделения двойным щелчком: слово, пробелы, остальное.
fn class(c: char) -> u8 {
    if is_word(c) {
        0
    } else if c.is_whitespace() && c != '\n' {
        1
    } else {
        2
    }
}

/// Слово под позицией (двойной щелчок): подряд идущие символы одного класса —
/// буквы, пробелы или знаки; перевод строки — пусто.
pub fn word_range(text: &[char], at: usize) -> Range<usize> {
    let len = text.len();
    if len == 0 {
        return 0..0;
    }
    // Справа от конца слова (щелчок за последней буквой) — то же слово.
    let mut i = at.min(len - 1);
    if (text[i] == '\n' || !is_word(text[i])) && i > 0 && is_word(text[i - 1]) {
        i -= 1;
    }
    if text[i] == '\n' {
        return at..at;
    }
    let k = class(text[i]);
    let mut a = i;
    while a > 0 && text[a - 1] != '\n' && class(text[a - 1]) == k {
        a -= 1;
    }
    let mut b = i + 1;
    while b < len && text[b] != '\n' && class(text[b]) == k {
        b += 1;
    }
    a..b
}

/// Конец следующего слова (Alt/Option+→ на macOS, Ctrl+→ на остальных).
pub fn next_word_end(text: &[char], at: usize) -> usize {
    let len = text.len();
    let mut i = at.min(len);
    while i < len && !is_word(text[i]) {
        i += 1;
    }
    while i < len && is_word(text[i]) {
        i += 1;
    }
    i
}

/// Начало предыдущего слова (Alt/Option+←, Ctrl+←).
pub fn prev_word_start(text: &[char], at: usize) -> usize {
    let mut i = at.min(text.len());
    while i > 0 && !is_word(text[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word(text[i - 1]) {
        i -= 1;
    }
    i
}

/// Абзац позиции без его перевода строки (тройной щелчок, Alt+↑/↓ на macOS).
pub fn paragraph_range(text: &[char], at: usize) -> Range<usize> {
    let len = text.len();
    let at = at.min(len);
    let mut a = at;
    while a > 0 && text[a - 1] != '\n' {
        a -= 1;
    }
    let mut b = at;
    while b < len && text[b] != '\n' {
        b += 1;
    }
    a..b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn words_and_paragraphs() {
        let t = chars("Привет, мир!\nвторой  абзац\n");
        assert_eq!(word_range(&t, 2), 0..6);
        // Щелчок сразу за словом — то же слово; по пробелам — пробелы.
        assert_eq!(word_range(&t, 6), 0..6);
        assert_eq!(word_range(&t, 20), 19..21);
        assert_eq!(next_word_end(&t, 0), 6);
        assert_eq!(next_word_end(&t, 6), 11);
        assert_eq!(prev_word_start(&t, 11), 8);
        assert_eq!(prev_word_start(&t, 8), 0);
        assert_eq!(paragraph_range(&t, 15), 13..26);
        assert_eq!(paragraph_range(&t, 12), 0..12);
        // Пустой абзац — пустой диапазон в его начале.
        let e = chars("а\n\nб\n");
        assert_eq!(paragraph_range(&e, 2), 2..2);
        assert_eq!(word_range(&e, 2), 2..2);
    }
}
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
}

/// Результат раскладки.
#[derive(Debug, Clone, Default)]
````

на:

````rust
}

/// Строка для правки текста: какие символы на ней, где она по вертикали и где
/// между кластерами может стоять курсор (спека 7). Позиции — символы Unicode
/// содержимого узла.
#[derive(Debug, Clone, PartialEq)]
pub struct CaretLine {
    /// Позиция первого символа строки.
    pub start: usize,
    /// Позиция за последним символом строки; у последней строки абзаца — позиция
    /// его перевода строки.
    pub end: usize,
    pub top: f64,
    pub bottom: f64,
    pub baseline: f64,
    /// Позиции курсора по возрастанию символа: `(символ, x)`, от `start` до `end`.
    pub stops: Vec<(usize, f64)>,
    /// Строка кончается абзацем, а не переносом.
    pub hard: bool,
}

/// Результат раскладки.
#[derive(Debug, Clone, Default)]
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
    /// Текст обрезан (обрезка с многоточием или по рамке).
    pub truncated: bool,
}

````

на:

````rust
    /// Текст обрезан (обрезка с многоточием или по рамке).
    pub truncated: bool,
    /// Строки для правки (курсор, выделение, попадание) — все, и у обрезанного.
    pub carets: Vec<CaretLine>,
}

````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
struct ParaSpan {
    lines: Range<usize>,
    runs: Range<usize>,
    decos: Range<usize>,
````

на:

````rust
struct ParaSpan {
    lines: Range<usize>,
    carets: Range<usize>,
    runs: Range<usize>,
    decos: Range<usize>,
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
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
````

на:

````rust
}

/// Символ с учётом регистра — по одному: так каждый символ содержимого знает
/// свои байты в тексте раскладки (курсор и попадание, спека 7).
fn push_case(out: &mut String, c: char, case: TextCase, word_start: &mut bool) {
    match case {
        TextCase::Upper => out.extend(c.to_uppercase()),
        TextCase::Lower => out.extend(c.to_lowercase()),
        TextCase::Title => {
            if *word_start && c.is_alphabetic() {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            *word_start = !c.is_alphanumeric();
        }
        TextCase::None | TextCase::SmallCaps => out.push(c),
    }
}

/// Число из документа без NaN и бесконечности: иначе — `default` (файл чужой
/// сборки или испорченный кэш не должен довести NaN до `parley`).
fn finite_or(v: f64, default: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        default
    }
}

/// Байты текста абзаца → символы содержимого узла.
struct CharMap {
    /// Начало байтов каждого символа абзаца и его позиция в содержимом.
    starts: Vec<(usize, usize)>,
    /// Позиция перевода строки абзаца (за последним символом).
    end: usize,
    text_len: usize,
}

impl CharMap {
    fn char_at(&self, byte: usize) -> usize {
        if byte >= self.text_len {
            return self.end;
        }
        match self.starts.partition_point(|(b, _)| *b <= byte) {
            0 => self.starts.first().map_or(self.end, |s| s.1),
            i => self.starts[i - 1].1,
        }
    }
}
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
    match v {
        LineHeight::Auto => parley::LineHeight::MetricsRelative(1.0),
        LineHeight::Px(px) => parley::LineHeight::Absolute(px.max(0.0) as f32),
        LineHeight::Percent(p) => parley::LineHeight::FontSizeRelative((p / 100.0).max(0.0) as f32),
    }
}
````

на:

````rust
    match v {
        LineHeight::Auto => parley::LineHeight::MetricsRelative(1.0),
        LineHeight::Px(px) => parley::LineHeight::Absolute(finite_or(px, 0.0).max(0.0) as f32),
        LineHeight::Percent(p) => {
            parley::LineHeight::FontSizeRelative((finite_or(p, 100.0) / 100.0).max(0.0) as f32)
        }
    }
}
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
fn letter_spacing(v: LetterSpacing, size: f64) -> f32 {
    match v {
        LetterSpacing::Px(px) => px as f32,
        LetterSpacing::Percent(p) => (size * p / 100.0) as f32,
    }
}
````

на:

````rust
fn letter_spacing(v: LetterSpacing, size: f64) -> f32 {
    match v {
        LetterSpacing::Px(px) => finite_or(px, 0.0) as f32,
        LetterSpacing::Percent(p) => finite_or(size * p / 100.0, 0.0) as f32,
    }
}
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
        let mut counters = [0u32; MAX_LIST_INDENT as usize + 1];
        let paragraphs = split_paragraphs(req.text);
        for (pi, para) in paragraphs.iter().enumerate() {
            if pi > 0 {
````

на:

````rust
        let mut counters = [0u32; MAX_LIST_INDENT as usize + 1];
        let paragraphs = split_paragraphs(req.text);
        // Позиция первого символа абзаца в содержимом узла.
        let mut para_start = 0usize;
        for (pi, para) in paragraphs.iter().enumerate() {
            if pi > 0 {
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
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
````

на:

````rust
                0.0
            };
            // Текст абзаца с учётом регистра, диапазоны участков в байтах и
            // символы содержимого для курсора.
            let mut text = String::new();
            let mut ranges: Vec<Range<usize>> = Vec::new();
            let mut starts: Vec<(usize, usize)> = Vec::new();
            let mut word_start = true;
            let mut ci = para_start;
            for ((s, _), style) in para.runs.iter().zip(&styles) {
                let start = text.len();
                for c in s.chars() {
                    starts.push((text.len(), ci));
                    push_case(&mut text, c, style.case, &mut word_start);
                    ci += 1;
                }
                ranges.push(start..text.len());
            }
            let map = CharMap {
                starts,
                end: ci,
                text_len: text.len(),
            };
            para_start = ci + 1;
            // Пустой абзац — строка высотой своего стиля: невидимый пробел нулевой ширины.
            if text.is_empty() {
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
            }
            let mut layout: Layout<u32> = builder.build(&text);
            if props.paragraph_indent != 0.0 {
                layout.set_text_indent(props.paragraph_indent as f32, IndentOptions::default());
            }
            let avail = req.width.map(|w| (w - indent_x).max(0.0) as f32);
````

на:

````rust
            }
            let mut layout: Layout<u32> = builder.build(&text);
            let indent = finite_or(props.paragraph_indent, 0.0);
            if indent != 0.0 {
                layout.set_text_indent(indent as f32, IndentOptions::default());
            }
            let avail = req.width.map(|w| (w - indent_x).max(0.0) as f32);
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
            );
            let first_line = out.lines.len();
            let (first_run, first_deco) = (out.runs.len(), out.decorations.len());
            collect_lines(&layout, &styles, indent_x, y, &mut out);
            if let (Some(m), Some(line)) = (marker, out.lines.get(first_line).copied()) {
                let x = level as f64 * LIST_INDENT_EM * first.size;
````

на:

````rust
            );
            let first_line = out.lines.len();
            let first_caret = out.carets.len();
            let (first_run, first_deco) = (out.runs.len(), out.decorations.len());
            collect_lines(&layout, &styles, indent_x, y, &mut out);
            collect_carets(&layout, &map, indent_x, y, &mut out.carets);
            if let (Some(m), Some(line)) = (marker, out.lines.get(first_line).copied()) {
                let x = level as f64 * LIST_INDENT_EM * first.size;
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
                spans.push(ParaSpan {
                    lines: first_line..out.lines.len(),
                    runs: first_run..out.runs.len(),
                    decos: first_deco..out.decorations.len(),
````

на:

````rust
                spans.push(ParaSpan {
                    lines: first_line..out.lines.len(),
                    carets: first_caret..out.carets.len(),
                    runs: first_run..out.runs.len(),
                    decos: first_deco..out.decorations.len(),
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
            for ParaSpan {
                lines,
                runs,
                decos,
````

на:

````rust
            for ParaSpan {
                lines,
                carets,
                runs,
                decos,
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
                for l in &mut out.lines[lines] {
                    l.rect = l.rect + kurbo::Vec2::new(dx, 0.0);
                }
                for r in &mut out.runs[runs] {
````

на:

````rust
                for l in &mut out.lines[lines] {
                    l.rect = l.rect + kurbo::Vec2::new(dx, 0.0);
                }
                for c in &mut out.carets[carets] {
                    for s in &mut c.stops {
                        s.1 += dx;
                    }
                }
                for r in &mut out.runs[runs] {
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
        SpanStyle {
            resolved,
            size: a.size.unwrap_or(d.size).clamp(0.1, 10_000.0),
            line_height: a.line_height.unwrap_or(d.line_height),
            letter_spacing: a.letter_spacing.unwrap_or(d.letter_spacing),
````

на:

````rust
        SpanStyle {
            resolved,
            size: finite_or(a.size.unwrap_or(d.size), 12.0).clamp(0.1, 10_000.0),
            line_height: a.line_height.unwrap_or(d.line_height),
            letter_spacing: a.letter_spacing.unwrap_or(d.letter_spacing),
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
}

fn shift(out: &mut TextLayout, dy: f64) {
    for l in &mut out.lines {
        l.rect = l.rect + kurbo::Vec2::new(0.0, dy);
````

на:

````rust
}

/// Строки абзаца для правки: символы, вертикаль и позиции курсора по кластерам
/// (`parley`), со сдвигом `(dx, dy)`.
fn collect_carets(layout: &Layout<u32>, map: &CharMap, dx: f64, dy: f64, out: &mut Vec<CaretLine>) {
    let count = layout.lines().count();
    for (i, line) in layout.lines().enumerate() {
        let m = line.metrics();
        let range = line.text_range();
        let (start, end) = (map.char_at(range.start), map.char_at(range.end));
        let mut stops: Vec<(usize, f64)> = Vec::new();
        let mut tail = None;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(gr) = item else {
                continue;
            };
            let mut x = gr.offset() as f64 + dx;
            for cl in gr.run().visual_clusters() {
                let r = cl.text_range();
                stops.push((map.char_at(r.start), x));
                x += cl.advance() as f64;
                tail = Some((map.char_at(r.end), x));
            }
        }
        let line_x = m.offset as f64 + dx;
        if stops.is_empty() {
            stops.push((start, line_x));
        }
        stops.push(match tail {
            Some((c, x)) => (c.max(end), x),
            None => (end, line_x),
        });
        // Кластер из нескольких символов (и пустой абзац) — одна позиция.
        stops.sort_by_key(|s| s.0);
        stops.dedup_by_key(|s| s.0);
        out.push(CaretLine {
            start,
            end,
            top: m.block_min_coord as f64 + dy,
            bottom: m.block_max_coord as f64 + dy,
            baseline: m.baseline as f64 + dy,
            stops,
            hard: i + 1 == count,
        });
    }
}

fn shift(out: &mut TextLayout, dy: f64) {
    for c in &mut out.carets {
        c.top += dy;
        c.bottom += dy;
        c.baseline += dy;
    }
    for l in &mut out.lines {
        l.rect = l.rect + kurbo::Vec2::new(0.0, dy);
````

В `crates/ferrum-text/src/lib.rs` заменить:

````rust
//! отдаёт строки и глифы.

mod fonts;
mod layout;
pub mod style_name;

pub use ferrum_fontmeta::{group, read_faces, FaceInfo, FamilyInfo, MAX_FONT_FILE};
pub use fonts::{
````

на:

````rust
//! отдаёт строки и глифы.

mod caret;
mod fonts;
mod layout;
pub mod style_name;

pub use caret::{next_word_end, paragraph_range, prev_word_start, word_range, Caret};
pub use ferrum_fontmeta::{group, read_faces, FaceInfo, FamilyInfo, MAX_FONT_FILE};
pub use fonts::{
````

В `crates/ferrum-text/src/lib.rs` заменить:

````rust
};
pub use layout::{
    text_color, DecorationBox, Face, GlyphRun, LayoutRequest, LineBox, PlacedGlyph, TextLayout,
    LIST_INDENT_EM,
};

````

на:

````rust
};
pub use layout::{
    text_color, CaretLine, DecorationBox, Face, GlyphRun, LayoutRequest, LineBox, PlacedGlyph,
    TextLayout, LIST_INDENT_EM,
};

````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-text -p ferrum-render 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo check -p ferrum-text --target wasm32-unknown-unknown`

Expected: PASS: `caret` — 8 тестов, юнит-тесты `ferrum-text` — 3 (слова и абзацы), `ferrum-render` — без изменений; wasm-сборка крейта собирается

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-text/src/caret.rs crates/ferrum-text/src/layout.rs crates/ferrum-text/src/lib.rs crates/ferrum-text/tests/caret.rs
git commit -m "feat(text): курсор по раскладке — позиции между кластерами, попадание по точке, строки, слова и абзацы, выделение; NaN из файла не доходит до parley

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3: Оверлей правки текста

Спека 7 и ядро §8.6: курсор 1 px цвета `select`, выделение — заливка `select` 25 %, составляемый текст IME — подчёркнут; вместо рамки с ручками — тонкая рамка текста (как в Figma). Оверлей получает геометрию в координатах узла и матрицу узла, рендер только рисует; редактор заполнит его в Task 4 (здесь — `None`).

**Files:**
- Modify: `crates/ferrum-render/src/overlay.rs`, `lib.rs`, `crates/ferrum-editor/src/editor.rs` (`overlay`: `text_edit: None`)
- Test: `crates/ferrum-render/tests/golden.rs`, эталон `tests/golden/text_edit.png`

**Interfaces:**
- Produces: `TextEditOverlay { transform: Affine, frame: Rect, selection: Vec<Rect>, caret: Option<(f64, f64, f64)>, composition: Vec<(f64, f64, f64)> }`, `Overlay::text_edit: Option<TextEditOverlay>`, `TEXT_SELECTION_ALPHA = 0.25`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-render/tests/golden.rs` заменить:

````rust
use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch, ScaleMode, Stroke, StrokeAlign};
use ferrum_render::{Camera, Measure, MeasureKind, Overlay, OverlayTheme, Pixmap, Renderer, Rgba};
use kurbo::{Line, Point, Rect, Size};
use vello_cpu::color::PremulRgba8;

````

на:

````rust
use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch, ScaleMode, Stroke, StrokeAlign};
use ferrum_render::{
    Camera, Measure, MeasureKind, Overlay, OverlayTheme, Pixmap, Renderer, Rgba, TextEditOverlay,
};
use kurbo::{Affine, Line, Point, Rect, Size};
use vello_cpu::color::PremulRgba8;

````

В `crates/ferrum-render/tests/golden.rs` заменить:

````rust
    golden("mirroring", &img);
}
````

на:

````rust
    golden("mirroring", &img);
}

#[test]
fn text_edit() {
    let scene = Page::new().scene();
    let overlay = Overlay {
        frame_labels: false,
        pixel_grid: false,
        text_edit: Some(TextEditOverlay {
            transform: Affine::translate((20.0, 30.0)),
            frame: Rect::new(0.0, 0.0, 120.0, 40.0),
            selection: vec![Rect::new(10.0, 4.0, 50.0, 24.0)],
            caret: Some((70.0, 4.0, 24.0)),
            composition: vec![(80.0, 110.0, 22.0)],
        }),
        ..Default::default()
    };
    let img = render_overlay(&scene, &camera(160.0, 80.0), &overlay, WHITE);
    let select = [0x4a, 0xa3, 0xff];
    // Выделение — `select` 25 % поверх белого.
    assert_px(&img, 40, 45, [210, 232, 255], 2);
    // Курсор в 1 px: столбец x = 90 от y = 34 до 54, соседние столбцы — фон.
    assert_px(&img, 90, 44, select, 2);
    assert_px(&img, 89, 44, [255, 255, 255], 0);
    assert_px(&img, 91, 44, [255, 255, 255], 0);
    // Подчёркивание IME — строка y = 52 от x = 100 до 130.
    assert_px(&img, 115, 52, select, 2);
    assert_px(&img, 115, 53, [255, 255, 255], 0);
    // Рамка текста — тонкий контур, внутри — фон.
    assert_px(&img, 20, 60, select, 2);
    assert_px(&img, 25, 60, [255, 255, 255], 0);
    golden("text_edit", &img);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-render --test golden text_edit`
Expected: FAIL — не собирается: нет `TextEditOverlay` и поля `Overlay::text_edit`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            pixel_grid: self.settings.pixel_grid,
            measurements: self.measures.clone(),
        }
    }
````

на:

````rust
            pixel_grid: self.settings.pixel_grid,
            measurements: self.measures.clone(),
            text_edit: None,
        }
    }
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use overlay::{
    format_size, Measure, MeasureKind, Overlay, OverlayTheme, HANDLE_SIZE, LABEL_SIZE,
    MEASURE_DASH, MEASURE_GAP, PIXEL_GRID_MIN_ZOOM,
};
pub use text::{Label, TextEngine, ONEST};
````

на:

````rust
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use overlay::{
    format_size, Measure, MeasureKind, Overlay, OverlayTheme, TextEditOverlay, HANDLE_SIZE,
    LABEL_SIZE, MEASURE_DASH, MEASURE_GAP, PIXEL_GRID_MIN_ZOOM, TEXT_SELECTION_ALPHA,
};
pub use text::{Label, TextEngine, ONEST};
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

/// Правка текста на холсте (подсистема 3, этап 2): рамка текста, выделение,
/// курсор и подчёркивание составляемого текста IME. Всё — в координатах узла.
#[derive(Debug, Clone, PartialEq)]
pub struct TextEditOverlay {
    /// Координаты узла → мир.
    pub transform: Affine,
    /// Рамка узла: тонкий контур цвета выделения вместо ручек (как в Figma).
    pub frame: Rect,
    /// Выделенные участки строк: заливка `select` 25 %.
    pub selection: Vec<Rect>,
    /// Курсор: x, верх и низ строки; `None` — погашен миганием или есть выделение.
    pub caret: Option<(f64, f64, f64)>,
    /// Подчёркивание составляемого IME текста: `(x0, x1, y)` по строкам.
    pub composition: Vec<(f64, f64, f64)>,
}

/// Прозрачность заливки выделения текста (спека подсистемы 3, раздел 7).
pub const TEXT_SELECTION_ALPHA: f32 = 0.25;

/// Что показать поверх содержимого. Заполняет редактор (этап 3).
#[derive(Debug, Clone)]
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
    /// Измерения расстояний с Alt (раздел 8.6 спеки).
    pub measurements: Vec<Measure>,
}

````

на:

````rust
    /// Измерения расстояний с Alt (раздел 8.6 спеки).
    pub measurements: Vec<Measure>,
    /// Правка текста: курсор и выделение внутри текстового узла.
    pub text_edit: Option<TextEditOverlay>,
}

````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
            pixel_grid: true,
            measurements: Vec::new(),
        }
    }
````

на:

````rust
            pixel_grid: true,
            measurements: Vec::new(),
            text_edit: None,
        }
    }
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
        backend.stroke_path(&path, &hairline);
    }
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
}

````

на:

````rust
        backend.stroke_path(&path, &hairline);
    }
    if let Some(t) = &overlay.text_edit {
        text_edit(backend, view, dpr, theme, t);
    }
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
}

/// Правка текста: контур рамки, выделение, подчёркивание IME и курсор в 1 CSS px.
fn text_edit(
    backend: &mut impl RenderBackend,
    view: Affine,
    dpr: f64,
    theme: &OverlayTheme,
    t: &TextEditOverlay,
) {
    let to_device = view * t.transform;
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    let mut frame = BezPath::new();
    let corners = [
        Point::new(t.frame.x0, t.frame.y0),
        Point::new(t.frame.x1, t.frame.y0),
        Point::new(t.frame.x1, t.frame.y1),
        Point::new(t.frame.x0, t.frame.y1),
    ];
    for (i, c) in corners.iter().enumerate() {
        let p = crisp_point(to_device * *c, dpr);
        if i == 0 {
            frame.move_to(p);
        } else {
            frame.line_to(p);
        }
    }
    frame.close_path();
    backend.stroke_path(&frame, &Stroke::new(dpr));
    if !t.selection.is_empty() {
        backend.set_transform(to_device);
        backend.set_color(theme.select.multiply_alpha(TEXT_SELECTION_ALPHA));
        for r in &t.selection {
            backend.fill_rect(r);
        }
    }
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    for &(x0, x1, y) in &t.composition {
        let a = crisp_point(to_device * Point::new(x0, y), dpr);
        let b = crisp_point(to_device * Point::new(x1, y), dpr);
        let mut path = BezPath::new();
        path.move_to(a);
        path.line_to(b);
        backend.stroke_path(&path, &Stroke::new(dpr));
    }
    if let Some((x, top, bottom)) = t.caret {
        let a = crisp_point(to_device * Point::new(x, top), dpr);
        let b = crisp_point(to_device * Point::new(x, bottom), dpr);
        let mut path = BezPath::new();
        path.move_to(a);
        path.line_to(b);
        backend.stroke_path(&path, &Stroke::new(dpr));
    }
}

````

- [ ] **Step 4: Сгенерировать файлы**

Run: `export CARGO_TARGET_DIR=$PWD/target && FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test golden text_edit`
Expected: создан эталон `crates/ferrum-render/tests/golden/text_edit.png` (рамка, синяя полоса выделения, курсор и подчёркивание)

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-render 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `golden` — 16 тестов (с `text_edit`)

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-editor/src/editor.rs crates/ferrum-render/src/lib.rs crates/ferrum-render/src/overlay.rs crates/ferrum-render/tests/golden.rs crates/ferrum-render/tests/golden/text_edit.png
git commit -m "feat(render): оверлей правки текста — рамка, выделение select 25 %, курсор в 1 px, подчёркивание IME; эталон text_edit

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 4: Правка текста на холсте: вход, выход, курсор, мышь, клавиши

Спека 7: правку держит редактор (`Editor::text_edit`). Вход — двойной щелчок по тексту (курсор в точку; над ручками рамки мелкого текста — тоже правка) или Enter по единственному выделенному тексту (весь текст); выход — Esc (узел выделен), щелчок мимо текста (щелчок достаётся инструменту), выбор инструмента, выделение из слоёв, смена страницы, удаление узла. Пока идёт правка, раскладка клавиш не действует: `KeyDown` из кольца уходит в `text_key`. Мышь: щелчок — курсор, протяжка — выделение, двойной — слово, тройной — абзац (протяжка после них — словами и абзацами), Shift+щелчок — расширить. Курсор мигает 1 Гц от последнего действия по времени редактора `set_time`; оверлей правки входит в ключ кадра, поэтому кадр нужен только при смене. Ядру — `text_edit_info` (прямоугольник курсора на экране и когда курсор погашен). Ядро пока показывает текстовый курсор как обычный (протокол — Task 9).

**Files:**
- Create: `crates/ferrum-editor/src/text_edit.rs`
- Modify: `crates/ferrum-editor/src/editor.rs`, `lib.rs`, `tools/mod.rs` (`Cursor::Text`, `Outbox::edit_text`), `tools/select.rs` (двойной щелчок), `crates/ferrum-wasm/src/core.rs` (`Cursor::Text`)
- Test: `crates/ferrum-editor/tests/text_edit.rs` (новый), `tests/common/text.rs` (новый), `tests/common/mod.rs`

**Interfaces:**
- Consumes: `TextLayout::{hit, caret_rect, selection_rects, vertical, line_bounds, next_stop, prev_stop}`, `word_range` и соседи (Task 2); `TextEditOverlay` (Task 3).
- Produces: `EnterAt { All, Point(Point) }`, `TextEditInfo { id, anchor, focus, caret: Rect, composing }`, `CARET_BLINK_MS = 500`; `Editor::{is_editing_text, text_edit_info, set_time, start_text_edit(id, EnterAt) -> bool, end_text_edit, text_key(code, Modifiers) -> bool}`; внутри крейта — `TextEdit`, `edit_content`, `edit_layout`, `text_overlay`, `clamp_text_edit`, `finish_text_edit`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-editor/tests/common/mod.rs` заменить:

````rust
//! при которой экранные координаты совпадают с мировыми.
#![allow(dead_code)]

use ferrum_doc::{NodeId, NodeKind, Props, PropsPatch};
````

на:

````rust
//! при которой экранные координаты совпадают с мировыми.
#![allow(dead_code)]

pub mod text;

use ferrum_doc::{NodeId, NodeKind, Props, PropsPatch};
````

Создать `crates/ferrum-editor/tests/common/text.rs`:

````rust
//! Текст в сценарных тестах (подсистема 3, этап 2): реестр с вшитыми Inter и
//! Roboto, текст в точке мира, раскладка правимого узла, состояние правки.
#![allow(dead_code)]

use std::path::PathBuf;

use ferrum_doc::{NodeId, TextContent, TextProps};
use ferrum_editor::TextEditInfo;
use ferrum_text::{group, read_faces, Fonts, LayoutRequest, SharedFonts, SourceKind, TextLayout};
use kurbo::Point;

use super::H;

pub fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const FILES: [(&str, &str); 4] = [
    ("Inter", "inter/Inter-Variable.ttf"),
    ("Inter", "inter/Inter-Italic-Variable.ttf"),
    ("Roboto", "roboto/Roboto-Variable.ttf"),
    ("Roboto", "roboto/Roboto-Italic-Variable.ttf"),
];

/// Реестр со списком вшитых Inter и Roboto, оба загружены.
pub fn loaded_fonts() -> SharedFonts {
    let shared = Fonts::shared();
    {
        let mut f = shared.borrow_mut();
        let faces = FILES
            .iter()
            .flat_map(|(_, r)| read_faces(&font_file(r), r))
            .collect::<Vec<_>>();
        f.add_catalog(SourceKind::Bundled, group(faces));
        for (family, rel) in FILES {
            f.load(family, rel, font_file(rel)).unwrap();
        }
        f.take_requests();
    }
    shared
}

/// Стенд с загруженными шрифтами (экран совпадает с миром: зум 1).
pub fn text_h() -> H {
    let mut h = H::new();
    h.ed.set_fonts(loaded_fonts());
    h
}

/// Текст «авто по ширине» кеглем `size` в точке мира; выделен.
pub fn add_text(h: &mut H, x: f64, y: f64, s: &str, size: f64) -> NodeId {
    let mut props = TextProps::default();
    props.style.size = size;
    h.ed.create_text(Point::new(x, y), &TextContent::plain(s), &props, 0.0, 0.0)
        .unwrap()
}

/// Раскладка узла по свойствам сцены.
pub fn layout_of(h: &H, id: NodeId) -> TextLayout {
    let n = h.ed.scene().node(id).unwrap();
    let t = n.props.text.as_ref().unwrap();
    h.ed.fonts().borrow_mut().layout(&LayoutRequest::for_node(
        t,
        &n.props.fills,
        n.props.w,
        n.props.h,
    ))
}

/// Точка мира позиции `index` на первой строке её текста (середина строки).
pub fn at_index(h: &H, id: NodeId, index: usize) -> (f64, f64) {
    let l = layout_of(h, id);
    let line = l
        .carets
        .iter()
        .find(|c| c.start <= index && index <= c.end)
        .unwrap();
    let n = h.ed.scene().node(id).unwrap();
    let p = n.world * Point::new(line.x_of(index), (line.top + line.bottom) / 2.0);
    (p.x, p.y)
}

pub fn info(h: &H) -> TextEditInfo {
    h.ed.text_edit_info().expect("идёт правка текста")
}

/// Выделение правки: (якорь, курсор).
pub fn sel_range(h: &H) -> (usize, usize) {
    let i = info(h);
    (i.anchor, i.focus)
}

/// Текст узла в документе (с последним переводом строки).
pub fn text_of(h: &mut H, id: NodeId) -> String {
    h.ed.page_doc()
        .props(id)
        .unwrap()
        .text
        .unwrap()
        .content
        .text()
}
````

Создать `crates/ferrum-editor/tests/text_edit.rs`:

````rust
//! Правка текста на холсте (подсистема 3, этап 2): вход двойным щелчком и Enter,
//! выход Esc и щелчком мимо, курсор и выделение мышью и клавишами, мигание,
//! раскладка клавиш в правке не действует.

mod common;

use common::text::*;
use common::*;
use ferrum_editor::*;
use kurbo::Point;

#[test]
fn double_click_enters_editing_with_caret_at_the_point() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет мир", 24.0);
    h.ed.select(&[]);
    let (x, y) = at_index(&h, id, 3);
    h.dbl(x + 1.0, y);
    assert!(h.ed.is_editing_text());
    assert_eq!(sel_range(&h), (3, 3));
    assert_eq!(h.sel(), vec![id]);
    // Вместо рамки с ручками — рамка текста и курсор.
    let o = h.ed.overlay();
    assert!(o.selection.is_empty());
    let t = o.text_edit.unwrap();
    let (cx, top, bottom) = t.caret.unwrap();
    assert!((t.transform * Point::new(cx, 0.0)).x - x < 1e-6);
    assert!(bottom - top > 20.0);
    assert!(t.selection.is_empty());
    assert_eq!(h.ed.step_count(), 1, "вход в правку — не шаг истории");
}

#[test]
fn enter_selects_all_and_escape_exits_with_the_node_selected() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет мир", 24.0);
    h.key("Enter", none());
    assert_eq!(sel_range(&h), (0, 10));
    assert!(!h.ed.overlay().text_edit.unwrap().selection.is_empty());
    h.key("Escape", none());
    assert!(!h.ed.is_editing_text());
    assert_eq!(h.sel(), vec![id]);
    // Enter по не тексту ничего не делает.
    let r = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.select(&[r]);
    h.key("Enter", none());
    assert!(!h.ed.is_editing_text());
}

#[test]
fn arrows_move_by_cluster_word_line_and_document() {
    let mut h = text_h();
    add_text(&mut h, 100.0, 100.0, "один два\nтри", 24.0);
    h.key("Enter", none());
    // Схлопнуть выделение к началу, затем по кластерам.
    h.key("ArrowLeft", none());
    assert_eq!(sel_range(&h), (0, 0));
    h.key("ArrowRight", none());
    assert_eq!(sel_range(&h), (1, 1));
    h.key("ArrowRight", shift());
    h.key("ArrowRight", shift());
    assert_eq!(sel_range(&h), (1, 3));
    // Без Shift выделение схлопывается к своему краю.
    h.key("ArrowRight", none());
    assert_eq!(sel_range(&h), (3, 3));
    // Ctrl+→ (Windows) — конец слова, Ctrl+← — начало.
    h.key("ArrowRight", ctrl());
    assert_eq!(sel_range(&h), (4, 4));
    h.key("ArrowRight", ctrl());
    assert_eq!(sel_range(&h), (8, 8));
    h.key("ArrowLeft", ctrl());
    assert_eq!(sel_range(&h), (5, 5));
    // Home/End — строка, Ctrl+End — конец текста, ↓ — следующая строка.
    h.key("End", none());
    assert_eq!(sel_range(&h), (8, 8));
    h.key("Home", none());
    assert_eq!(sel_range(&h), (0, 0));
    h.key("ArrowDown", none());
    assert_eq!(sel_range(&h), (9, 9));
    h.key("ArrowDown", none());
    assert_eq!(sel_range(&h), (12, 12), "ниже последней строки — конец");
    h.key("Home", ctrl().with_shift());
    assert_eq!(sel_range(&h), (12, 0));
    h.key("KeyA", ctrl());
    assert_eq!(sel_range(&h), (0, 12));
}

#[test]
fn mac_uses_option_for_words_and_command_for_lines() {
    let mut h = H::with_platform(Platform::Mac);
    h.ed.set_fonts(loaded_fonts());
    add_text(&mut h, 100.0, 100.0, "один два\nтри", 24.0);
    h.key("Enter", none());
    h.key("ArrowLeft", none());
    h.key("ArrowRight", Modifiers::ALT);
    assert_eq!(sel_range(&h), (4, 4));
    h.key("ArrowRight", Modifiers::META);
    assert_eq!(sel_range(&h), (8, 8));
    h.key("ArrowDown", Modifiers::META);
    assert_eq!(sel_range(&h), (12, 12));
    h.key("ArrowUp", Modifiers::ALT);
    assert_eq!(sel_range(&h), (9, 9));
}

#[test]
fn mouse_selects_by_drag_word_paragraph_and_shift_click() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "один два\nтри", 24.0);
    h.key("Enter", none());
    let (x1, y) = at_index(&h, id, 1);
    let (x3, _) = at_index(&h, id, 3);
    // Протяжка — от позиции до позиции.
    h.drag((x1, y), (x3, y), none());
    assert_eq!(sel_range(&h), (1, 3));
    // Двойной щелчок — слово, тройной — абзац.
    let (x6, _) = at_index(&h, id, 6);
    h.dbl(x6, y);
    assert_eq!(sel_range(&h), (5, 8));
    h.t -= 550.0;
    h.click(x6, y, none());
    assert_eq!(sel_range(&h), (0, 8));
    // Shift+щелчок расширяет от якоря.
    h.click(x1, y, none());
    let (x10, y2) = at_index(&h, id, 10);
    h.click(x10, y2, shift());
    assert_eq!(sel_range(&h), (1, 10));
    assert_eq!(h.ed.step_count(), 1);
}

#[test]
fn click_outside_ends_editing_and_acts_as_a_normal_click() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    let r = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.select(&[id]);
    h.key("Enter", none());
    h.click(420.0, 420.0, none());
    assert!(!h.ed.is_editing_text());
    assert_eq!(h.sel(), vec![r]);
}

#[test]
fn keymap_does_not_act_while_editing() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.key("Enter", none());
    h.key("KeyR", none());
    h.key("Delete", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert!(h.ed.scene().node(id).is_some());
    assert!(h.ed.is_editing_text());
    // Выбор инструмента извне заканчивает правку.
    h.ed.set_tool(ToolKind::Rect);
    assert!(!h.ed.is_editing_text());
}

#[test]
fn caret_blinks_at_one_hertz_and_lights_up_after_a_key() {
    let mut h = text_h();
    add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.ed.set_time(10_000.0);
    h.key("Enter", none());
    h.key("ArrowLeft", none());
    let caret = |h: &H| h.ed.overlay().text_edit.unwrap().caret.is_some();
    assert!(caret(&h));
    h.ed.take_events();
    h.ed.set_time(10_600.0);
    assert!(!caret(&h));
    assert!(h.ed.take_events().contains(&EditorEvent::NeedsRedraw));
    h.ed.set_time(11_100.0);
    assert!(caret(&h));
    h.ed.set_time(11_600.0);
    assert!(!caret(&h));
    h.key("ArrowRight", none());
    assert!(caret(&h));
    // Поле ввода получает прямоугольник курсора и когда он погашен.
    h.ed.set_time(12_200.0);
    assert!(info(&h).caret.height() > 20.0);
}

#[test]
fn caret_rect_follows_the_camera() {
    let mut h = text_h();
    add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.key("Enter", none());
    let one = info(&h).caret;
    let mut cam = h.ed.camera().clone();
    cam.set_zoom(2.0);
    h.ed.set_camera(cam);
    let two = info(&h).caret;
    assert!((two.height() - 2.0 * one.height()).abs() < 1e-6);
}

#[test]
fn double_click_on_small_text_beats_its_handles() {
    let mut h = text_h();
    // Кегль 12: рамка 15 px высотой — точка у края в зоне ручки стороны.
    let id = add_text(&mut h, 100.0, 300.0, "Привет", 12.0);
    h.ed.select(&[]);
    h.dbl(105.0, 305.0);
    assert!(h.ed.is_editing_text());
    assert_eq!(h.sel(), vec![id]);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor --test text_edit`
Expected: FAIL — не собирается: нет `Editor::text_edit_info`, `start_text_edit`, `text_key`, `set_time`, `CARET_BLINK_MS`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
//! `camera()` и `overlay()` по ссылке.

use std::collections::HashSet;

use ferrum_doc::{Changes, DocError, DocResult, Document, NodeId, PageDoc, PageId, Tx};
use ferrum_render::{Camera, Measure, Overlay, OverlayTheme};
use ferrum_scene::Scene;
use ferrum_text::{Fonts, SharedFonts};
use kurbo::{Point, Size, Vec2};

````

на:

````rust
//! `camera()` и `overlay()` по ссылке.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use ferrum_doc::{Changes, DocError, DocResult, Document, NodeId, PageDoc, PageId, Tx};
use ferrum_render::{Camera, Measure, Overlay, OverlayTheme, TextEditOverlay};
use ferrum_scene::Scene;
use ferrum_text::{Fonts, SharedFonts, TextLayout};
use kurbo::{Point, Size, Vec2};

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
use crate::measure::{measure, AltState, MeasureCache};
use crate::panel::PanelGesture;
use crate::tools::{
    sync, Ctx, Cursor, DrawTool, Flow, HandTool, Outbox, SelectTool, Selection, Shape, Tool,
    ToolKind, ToolOverlay,
};

/// Множитель зума колесом с Ctrl/Cmd и щипком: `exp(-deltaY · k)`.
````

на:

````rust
use crate::measure::{measure, AltState, MeasureCache};
use crate::panel::PanelGesture;
use crate::text_edit::{EnterAt, TextEdit};
use crate::tools::{
    sync, Ctx, Cursor, DrawTool, Flow, HandTool, Outbox, SelectTool, Selection, Shape, Tool,
    ToolKind, ToolOverlay,
};

/// Раскладка правимого текста и её ключ: узел, версия узла, поколение шрифтов.
pub(crate) type EditLayout = ((NodeId, u64, u64), Arc<TextLayout>);

/// Множитель зума колесом с Ctrl/Cmd и щипком: `exp(-deltaY · k)`.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Измерения с Alt: кадр нужен, когда они изменились, и не нужен, пока те же.
    measures: Vec<Measure>,
}

````

на:

````rust
    /// Измерения с Alt: кадр нужен, когда они изменились, и не нужен, пока те же.
    measures: Vec<Measure>,
    /// Правка текста: курсор (с миганием), выделение, превью IME.
    text: Option<TextEditOverlay>,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Реестр шрифтов для размера текста по раскладке (общий с рендером ядра).
    pub(crate) fonts: SharedFonts,
}

````

на:

````rust
    /// Реестр шрифтов для размера текста по раскладке (общий с рендером ядра).
    pub(crate) fonts: SharedFonts,
    /// Правка текста на холсте (спека подсистемы 3, раздел 7).
    pub(crate) text_edit: Option<TextEdit>,
    /// Раскладка правимого текста по ключу (узел, версия узла, поколение шрифтов).
    pub(crate) edit_layout_cache: RefCell<Option<EditLayout>>,
    /// Время ядра, мс (`set_time`): мигание курсора и склейка ввода.
    pub(crate) now_ms: f64,
    /// Указатель над правимым текстом: курсор — текстовый.
    pub(crate) text_hover: bool,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                pixel_grid: true,
                measures: Vec::new(),
            },
        };
````

на:

````rust
                pixel_grid: true,
                measures: Vec::new(),
                text: None,
            },
        };
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            measure_cache: None,
            fonts: Fonts::shared(),
        })
    }
````

на:

````rust
            measure_cache: None,
            fonts: Fonts::shared(),
            text_edit: None,
            edit_layout_cache: RefCell::new(None),
            now_ms: 0.0,
            text_hover: false,
        })
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        if self.pan.is_some() {
            Cursor::Grabbing
        } else {
            self.tool.cursor()
````

на:

````rust
        if self.pan.is_some() {
            Cursor::Grabbing
        } else if self.text_edit.is_some() && self.text_hover {
            Cursor::Text
        } else {
            self.tool.cursor()
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    pub fn overlay(&self) -> Overlay {
        let t = self.tool.overlay();
        Overlay {
            theme: self.theme.clone(),
            selection: t.selection.unwrap_or_else(|| self.selection.to_vec()),
            hover: if self.is_busy() { None } else { self.hover },
            marquee: t.marquee,
            snap_lines: t.snap_lines,
````

на:

````rust
    pub fn overlay(&self) -> Overlay {
        let t = self.tool.overlay();
        // Правка текста: вместо рамки с ручками — рамка текста и курсор.
        let text = self.text_overlay().map(|o| o.overlay);
        let editing = text.is_some();
        Overlay {
            theme: self.theme.clone(),
            selection: if editing {
                Vec::new()
            } else {
                t.selection.unwrap_or_else(|| self.selection.to_vec())
            },
            hover: if self.is_busy() || editing {
                None
            } else {
                self.hover
            },
            marquee: t.marquee,
            snap_lines: t.snap_lines,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            pixel_grid: self.settings.pixel_grid,
            measurements: self.measures.clone(),
            text_edit: None,
        }
    }
````

на:

````rust
            pixel_grid: self.settings.pixel_grid,
            measurements: self.measures.clone(),
            text_edit: text,
        }
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                    return;
                }
                // Нажатие без отпускания предыдущего (событие потеряно) — прежний
                // жест отменяется, а не записывается.
````

на:

````rust
                    return;
                }
                // Правка текста: нажатие в тексте ставит курсор; мимо — правка
                // кончается, и нажатие достаётся инструменту.
                if self.text_edit.is_some() && self.text_pointer_down(&e) {
                    return;
                }
                // Нажатие без отпускания предыдущего (событие потеряно) — прежний
                // жест отменяется, а не записывается.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                let (tool, mut cx) = self.split();
                tool.pointer_down(&mut cx, &e);
            }
        }
````

на:

````rust
                let (tool, mut cx) = self.split();
                tool.pointer_down(&mut cx, &e);
                // Двойной щелчок по тексту: инструмент просит войти в правку.
                if let Some((id, world)) = self.out.edit_text.take() {
                    if self.tool.is_busy() {
                        let (tool, mut cx) = self.split();
                        tool.cancel(&mut cx);
                    }
                    self.start_text_edit(id, EnterAt::Point(world));
                }
            }
        }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            return;
        }
        let (tool, mut cx) = self.split();
        tool.pointer_move(&mut cx, &e);
````

на:

````rust
            return;
        }
        if let Some(t) = &self.text_edit {
            if t.drag.is_some() {
                self.text_pointer_move(&e);
                return;
            }
            self.text_hover = self.over_edited_text(e.point);
        }
        let (tool, mut cx) = self.split();
        tool.pointer_move(&mut cx, &e);
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                hand.pointer_up(&mut self.ctx(), &e);
            }
            return;
        }
````

на:

````rust
                hand.pointer_up(&mut self.ctx(), &e);
            }
            return;
        }
        if self.text_edit.as_ref().is_some_and(|t| t.drag.is_some()) {
            self.text_pointer_up(&e);
            return;
        }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust

    fn key_down(&mut self, e: KeyEvent) {
        if self.is_busy() {
            // Во время жеста клавиши только меняют модификаторы; Esc отменяет жест.
````

на:

````rust

    fn key_down(&mut self, e: KeyEvent) {
        // Правка текста: раскладка клавиш не действует (спека подсистемы 3, 7).
        if self.text_edit.is_some() {
            self.text_key(&e.code, e.modifiers);
            return;
        }
        if self.is_busy() {
            // Во время жеста клавиши только меняют модификаторы; Esc отменяет жест.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            }
            return;
        }
        let Some(cmd) = self.keymap.lookup(&e) else {
````

на:

````rust
            }
            return;
        }
        // Enter по единственному выделенному тексту — правка всего текста (как в
        // Figma). Физическая клавиша, как Esc у жеста: не команда раскладки.
        if (e.code == "Enter" || e.code == "NumpadEnter") && e.modifiers == Modifiers::NONE {
            if let [id] = self.selection[..] {
                if self.start_text_edit(id, EnterAt::All) {
                    return;
                }
            }
        }
        let Some(cmd) = self.keymap.lookup(&e) else {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    pub fn set_tool(&mut self, kind: ToolKind) {
        self.cancel_gesture();
        self.held = None;
        self.hover = None;
````

на:

````rust
    pub fn set_tool(&mut self, kind: ToolKind) {
        self.cancel_gesture();
        self.end_text_edit();
        self.held = None;
        self.hover = None;
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.cancel_gesture();
        let sel = self.alive(ids);
        self.selection.set(sel);
        self.emit();
````

на:

````rust
        self.cancel_gesture();
        let sel = self.alive(ids);
        // Другое выделение из панели слоёв заканчивает правку текста.
        if self.text_edit.as_ref().is_some_and(|t| sel != [t.id]) {
            self.end_text_edit();
        }
        self.selection.set(sel);
        self.emit();
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    pub fn set_page(&mut self, page: PageId) -> DocResult<()> {
        self.cancel_gesture();
        // Изменения всех страниц уже в сценах или не нужны: новая сцена строится
        // по документу целиком.
````

на:

````rust
    pub fn set_page(&mut self, page: PageId) -> DocResult<()> {
        self.cancel_gesture();
        self.finish_text_edit();
        // Изменения всех страниц уже в сценах или не нужны: новая сцена строится
        // по документу целиком.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            self.hover = None;
        }
        self.update_measures();
        let now = Seen {
````

на:

````rust
            self.hover = None;
        }
        // Правимый текст удалён (отмена, чужая правка): правка кончается; курсор
        // за концом текста (его укоротили) — к концу.
        self.clamp_text_edit();
        self.update_measures();
        let now = Seen {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                pixel_grid: self.settings.pixel_grid,
                measures: self.measures.clone(),
            },
        };
````

на:

````rust
                pixel_grid: self.settings.pixel_grid,
                measures: self.measures.clone(),
                text: self.text_overlay().map(|o| o.overlay),
            },
        };
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod snap;
pub mod text;
pub mod tools;

````

на:

````rust
pub mod snap;
pub mod text;
pub mod text_edit;
pub mod tools;

````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use text::{fit_size, MissingFont, TextError, TextPending};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

на:

````rust
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use text::{fit_size, MissingFont, TextError, TextPending};
pub use text_edit::{EnterAt, TextEditInfo, CARET_BLINK_MS};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

Создать `crates/ferrum-editor/src/text_edit.rs`:

````rust
//! Правка текста на холсте (спека подсистемы 3, раздел 7): вход и выход,
//! курсор и выделение (их держит ядро; позиции — символы Unicode, как у
//! `LoroText`), выделение мышью (щелчок, протяжка, двойной — слово, тройной —
//! абзац, Shift — расширить) и ход клавишами по кластерам, словам, строкам и
//! документу. Геометрию даёт раскладка узла (`ferrum_text::TextLayout`).
//!
//! Пока идёт правка, раскладка клавиш не действует: клавиши приходят сюда
//! (`text_key`), а набор — командами ввода (`text_input`, модуль `text_input`).

use std::sync::Arc;

use ferrum_doc::{NodeId, NodeKind, TextAttrs, TextContent};
use ferrum_render::TextEditOverlay;
use ferrum_text::{
    next_word_end, paragraph_range, prev_word_start, word_range, Caret, LayoutRequest, TextLayout,
};
use kurbo::{Point, Rect};

use crate::editor::Editor;
use crate::input::{Modifiers, Platform, PointerEvent};
use crate::tools::select::{DOUBLE_CLICK_MS, DOUBLE_CLICK_PX};

/// Полупериод мигания курсора: 1 Гц — полсекунды виден, полсекунды нет.
pub const CARET_BLINK_MS: f64 = 500.0;

/// Как войти в правку текста.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnterAt {
    /// Выделить весь текст (Enter по выделенному тексту, команда).
    All,
    /// Курсор в точку мира (двойной щелчок).
    Point(Point),
}

/// Чем расширяется выделение протяжкой: символами, словами или абзацами.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unit {
    Char,
    Word,
    Paragraph,
}

/// Состояние правки текста.
#[derive(Debug, Clone)]
pub(crate) struct TextEdit {
    pub id: NodeId,
    /// Начало выделения (неподвижный конец) и курсор.
    pub anchor: usize,
    pub focus: Caret,
    /// x для хода вверх и вниз: держится, пока жмут ↑/↓.
    pub goal_x: Option<f64>,
    /// Стиль следующего ввода, заданный при курсоре без выделения (панель,
    /// Ctrl/Cmd+B). Сбрасывается сменой позиции.
    pub caret_attrs: Option<TextAttrs>,
    /// Составляемый текст IME (превью — в сцене, в документе его нет).
    pub composition: Option<String>,
    /// Протяжка мышью: якорный диапазон и единица выделения.
    pub drag: Option<(std::ops::Range<usize>, Unit)>,
    /// Последний щелчок: время отпускания, экранная точка и кратность (1–3).
    pub clicks: Option<(f64, Point, u8)>,
    /// С какого момента курсор горит (мигание начинается заново после действия).
    pub blink_from: f64,
}

impl TextEdit {
    pub fn new(id: NodeId, anchor: usize, focus: usize, now: f64) -> TextEdit {
        TextEdit {
            id,
            anchor,
            focus: Caret::at(focus),
            goal_x: None,
            caret_attrs: None,
            composition: None,
            drag: None,
            clicks: None,
            blink_from: now,
        }
    }

    /// Выделенный диапазон (пустой — курсор).
    pub fn range(&self) -> std::ops::Range<usize> {
        let (a, b) = (self.anchor, self.focus.index);
        a.min(b)..a.max(b)
    }
}

/// Что видит ядро о правке: узел, выделение и курсор на экране (поле ввода
/// встаёт под курсор, окно IME — у текста).
#[derive(Debug, Clone, PartialEq)]
pub struct TextEditInfo {
    pub id: NodeId,
    pub anchor: usize,
    pub focus: usize,
    /// Прямоугольник курсора в CSS-пикселях холста.
    pub caret: Rect,
    pub composing: bool,
}

/// Последний допустимый символ курсора: перед последним переводом строки.
fn last_index(content: &TextContent) -> usize {
    content.char_len().saturating_sub(1)
}

impl Editor {
    /// Идёт ли правка текста.
    pub fn is_editing_text(&self) -> bool {
        self.text_edit.is_some()
    }

    /// Состояние правки для ядра (`None` — правки нет).
    pub fn text_edit_info(&self) -> Option<TextEditInfo> {
        let t = self.text_edit.as_ref()?;
        let caret = self.text_overlay().and_then(|o| {
            let (x, top, bottom) = o.caret_geometry?;
            let to_screen = self.camera.world_to_screen_transform() * o.overlay.transform;
            let a = to_screen * Point::new(x, top);
            let b = to_screen * Point::new(x, bottom);
            Some(Rect::from_points(a, b))
        });
        Some(TextEditInfo {
            id: t.id,
            anchor: t.anchor,
            focus: t.focus.index,
            caret: caret.unwrap_or(Rect::ZERO),
            composing: t.composition.is_some(),
        })
    }

    /// Текущее время ядра (мс): мигание курсора и склейка ввода в один шаг.
    pub fn set_time(&mut self, now_ms: f64) {
        if now_ms.is_finite() {
            self.now_ms = now_ms;
            if self.text_edit.is_some() {
                self.emit();
            }
        }
    }

    /// Горит ли курсор сейчас (мигание 1 Гц от последнего действия).
    pub(crate) fn caret_on(&self) -> bool {
        let Some(t) = &self.text_edit else {
            return false;
        };
        let k = ((self.now_ms - t.blink_from).max(0.0) / CARET_BLINK_MS).floor() as u64;
        k.is_multiple_of(2)
    }

    /// Содержимое правимого узла в документе (без превью IME).
    pub(crate) fn edit_content(&self) -> Option<TextContent> {
        let t = self.text_edit.as_ref()?;
        let n = self.scene.node(t.id)?;
        let doc = self.doc.loaded_page(self.page)?.props(n.id).ok()?;
        doc.text.as_ref().map(|x| x.content.clone())
    }

    /// Раскладка правимого узла по свойствам сцены (кэш — по версии узла и
    /// поколению реестра шрифтов).
    pub(crate) fn edit_layout(&self) -> Option<Arc<TextLayout>> {
        let t = self.text_edit.as_ref()?;
        let n = self.scene.node(t.id)?;
        let text = n.props.text.as_ref()?;
        let generation = self.fonts.borrow().generation();
        let key = (t.id, n.version, generation);
        if let Some((k, l)) = self.edit_layout_cache.borrow().as_ref() {
            if *k == key {
                return Some(l.clone());
            }
        }
        let req = LayoutRequest::for_node(text, &n.props.fills, n.props.w, n.props.h);
        let layout = Arc::new(self.fonts.borrow_mut().layout(&req));
        *self.edit_layout_cache.borrow_mut() = Some((key, layout.clone()));
        Some(layout)
    }

    /// Оверлей правки: рамка, выделение, курсор (если горит) и подчёркивание IME.
    pub(crate) fn text_overlay(&self) -> Option<EditOverlay> {
        let t = self.text_edit.as_ref()?;
        let n = self.scene.node(t.id)?;
        let layout = self.edit_layout()?;
        let comp = t.composition.as_ref().map_or(0, |c| c.chars().count());
        let range = t.range();
        let mut overlay = TextEditOverlay {
            transform: n.world,
            frame: Rect::new(0.0, 0.0, n.props.w, n.props.h),
            selection: Vec::new(),
            caret: None,
            composition: Vec::new(),
        };
        // Пока идёт составление, выделение уже заменено превью: курсор — за ним.
        let caret = if comp > 0 {
            let r = range.start..range.start + comp;
            overlay.composition = layout
                .selection_rects(r.clone())
                .into_iter()
                .map(|q| (q.x0, q.x1, q.y1 - 1.0))
                .collect();
            Caret::at(r.end)
        } else {
            overlay.selection = layout.selection_rects(range.clone());
            t.focus
        };
        let geometry = layout.caret_rect(caret);
        if (range.is_empty() || comp > 0) && self.caret_on() {
            overlay.caret = geometry;
        }
        Some(EditOverlay {
            overlay,
            caret_geometry: geometry,
        })
    }

    /// Входит в правку текста `id` (выделенного или нет): узел выделяется,
    /// жест отменяется. `false` — не текст, заблокирован или его нет.
    pub fn start_text_edit(&mut self, id: NodeId, at: EnterAt) -> bool {
        self.cancel_gesture();
        self.finish_text_edit();
        let Some(slot) = self.scene.slot(id) else {
            return false;
        };
        if self.scene.is_draft(id)
            || self.scene.at(slot).props.kind != NodeKind::Text
            || crate::selection::locked(&self.scene, slot)
        {
            return false;
        }
        let Some(content) = self
            .scene
            .at(slot)
            .props
            .text
            .as_ref()
            .map(|t| t.content.clone())
        else {
            return false;
        };
        let last = last_index(&content);
        self.selection.set(vec![id]);
        self.hover = None;
        self.text_edit = Some(TextEdit::new(id, 0, last, self.now_ms));
        match at {
            EnterAt::All => {}
            EnterAt::Point(world) => {
                let c = self.hit_text(world).unwrap_or(Caret::at(last));
                let t = self.text_edit.as_mut().expect("задана выше");
                (t.anchor, t.focus) = (c.index, c);
            }
        }
        self.emit();
        true
    }

    /// Выход из правки (Esc, команда): узел остаётся выделенным.
    pub fn end_text_edit(&mut self) {
        let Some(id) = self.text_edit.as_ref().map(|t| t.id) else {
            return;
        };
        self.finish_text_edit();
        if self.scene.slot(id).is_some() {
            self.selection.set(vec![id]);
        }
        self.emit();
    }

    /// Позиция по точке мира в правимом узле (или ближайшая к ней).
    pub(crate) fn hit_text(&self, world: Point) -> Option<Caret> {
        let t = self.text_edit.as_ref()?;
        let n = self.scene.node(t.id)?;
        let local = n.world.inverse() * world;
        let layout = self.edit_layout()?;
        let content = self.edit_content()?;
        let mut c = layout.hit(local);
        c.index = c.index.min(last_index(&content));
        Some(c)
    }

    /// Точка мира внутри рамки правимого узла (с допуском в 4 px экрана).
    fn inside_edited(&self, world: Point) -> bool {
        let Some(n) = self.text_edit.as_ref().and_then(|t| self.scene.node(t.id)) else {
            return false;
        };
        let tol = crate::tools::HIT_TOLERANCE_PX / self.camera.zoom();
        let local = n.world.inverse() * world;
        Rect::new(0.0, 0.0, n.props.w, n.props.h)
            .inflate(tol, tol)
            .contains(local)
    }

    /// Нажатие во время правки. `false` — мимо текста: правка закончена, нажатие
    /// обработает инструмент.
    pub(crate) fn text_pointer_down(&mut self, e: &PointerEvent) -> bool {
        let world = self.camera.screen_to_world(e.point);
        if !self.inside_edited(world) {
            self.end_text_edit();
            return false;
        }
        let Some(caret) = self.hit_text(world) else {
            return true;
        };
        let content = self.edit_content().unwrap_or_default();
        let chars: Vec<char> = content.text().chars().collect();
        let now = self.now_ms;
        let t = self.text_edit.as_mut().expect("правка идёт");
        let count = match t.clicks {
            Some((at, p, n))
                if e.time_ms - at <= DOUBLE_CLICK_MS
                    && (e.point - p).hypot() <= DOUBLE_CLICK_PX
                    && !e.modifiers.shift =>
            {
                n % 3 + 1
            }
            _ => 1,
        };
        t.clicks = Some((e.time_ms, e.point, count));
        t.goal_x = None;
        t.caret_attrs = None;
        t.blink_from = now;
        if e.modifiers.shift && count == 1 {
            // Shift+щелчок расширяет выделение от прежнего якоря.
            t.focus = caret;
            t.drag = Some((t.anchor..t.anchor, Unit::Char));
        } else {
            let (range, unit) = match count {
                2 => (word_range(&chars, caret.index), Unit::Word),
                3 => (paragraph_range(&chars, caret.index), Unit::Paragraph),
                _ => (caret.index..caret.index, Unit::Char),
            };
            t.anchor = range.start;
            t.focus = if range.is_empty() {
                caret
            } else {
                Caret::at(range.end)
            };
            t.drag = Some((range, unit));
        }
        true
    }

    /// Движение с нажатой кнопкой во время правки: выделение до точки (словами
    /// или абзацами — после двойного и тройного щелчка).
    pub(crate) fn text_pointer_move(&mut self, e: &PointerEvent) {
        let Some((base, unit)) = self.text_edit.as_ref().and_then(|t| t.drag.clone()) else {
            return;
        };
        let world = self.camera.screen_to_world(e.point);
        let Some(caret) = self.hit_text(world) else {
            return;
        };
        let content = self.edit_content().unwrap_or_default();
        let chars: Vec<char> = content.text().chars().collect();
        let extent = match unit {
            Unit::Char => caret.index..caret.index,
            Unit::Word => word_range(&chars, caret.index),
            Unit::Paragraph => paragraph_range(&chars, caret.index),
        };
        let now = self.now_ms;
        let t = self.text_edit.as_mut().expect("правка идёт");
        if extent.start < base.start {
            t.anchor = base.end;
            t.focus = Caret::at(extent.start);
        } else {
            t.anchor = base.start;
            t.focus = if unit == Unit::Char {
                caret
            } else {
                Caret::at(extent.end.max(base.end))
            };
        }
        t.blink_from = now;
    }

    /// Экранная точка над правимым текстом (курсор — текстовый).
    pub(crate) fn over_edited_text(&self, screen: Point) -> bool {
        self.inside_edited(self.camera.screen_to_world(screen))
    }

    /// Правимый узел пропал — правка кончается (черновик — в сцене, его не
    /// трогаем); курсор и якорь — не дальше конца текста.
    pub(crate) fn clamp_text_edit(&mut self) {
        let Some(t) = &self.text_edit else {
            return;
        };
        if self.scene.slot(t.id).is_none() {
            self.text_edit = None;
            self.text_hover = false;
            return;
        }
        let Some(content) = self.edit_content() else {
            return;
        };
        let last = last_index(&content);
        let t = self.text_edit.as_mut().expect("проверено выше");
        if t.anchor > last || t.focus.index > last {
            t.anchor = t.anchor.min(last);
            t.focus = Caret::at(t.focus.index.min(last));
        }
    }

    /// Отпускание: протяжка кончается; следующий щелчок считается двойным от
    /// этого момента (как у «Перемещения»).
    pub(crate) fn text_pointer_up(&mut self, e: &PointerEvent) {
        if let Some(t) = self.text_edit.as_mut() {
            t.drag = None;
            if let Some(c) = t.clicks.as_mut() {
                c.0 = e.time_ms;
            }
        }
    }

    /// Клавиша во время правки текста. Возвращает, обработана ли она.
    pub fn text_key(&mut self, code: &str, mods: Modifiers) -> bool {
        if self.text_edit.is_none() {
            return false;
        }
        let handled = self.text_navigation(code, mods) || self.text_command_key(code, mods);
        if handled {
            if let Some(t) = self.text_edit.as_mut() {
                t.blink_from = self.now_ms;
            }
        }
        self.emit();
        handled
    }

    /// Клавиши правки, кроме хода курсора: Esc — выход (`text_input` добавляет
    /// удаление, отмену, списки и форматирование).
    fn text_command_key(&mut self, code: &str, _mods: Modifiers) -> bool {
        match code {
            "Escape" => {
                self.end_text_edit();
                true
            }
            _ => false,
        }
    }

    /// Заканчивает правку без выделения узла: превью IME убирается.
    pub(crate) fn finish_text_edit(&mut self) {
        if self.text_edit.take().is_some() {
            self.scene.clear_preview();
        }
    }

    /// Ход курсора и выделение клавишами (см. описание модуля).
    fn text_navigation(&mut self, code: &str, mods: Modifiers) -> bool {
        let platform = self.settings.platform;
        let mac = platform == Platform::Mac;
        let primary = mods.primary(platform);
        let word = if mac { mods.alt } else { mods.ctrl };
        let extend = mods.shift;
        let Some(content) = self.edit_content() else {
            return false;
        };
        let Some(layout) = self.edit_layout() else {
            return false;
        };
        let last = last_index(&content);
        let chars: Vec<char> = content.text().chars().collect();
        let t = self.text_edit.as_ref().expect("правка идёт");
        let (range, focus) = (t.range(), t.focus);
        let x_now = || {
            t.goal_x
                .or_else(|| layout.caret_rect(focus).map(|r| r.0))
                .unwrap_or(0.0)
        };
        let mut goal = None;
        let target: Caret = match code {
            "ArrowLeft" | "ArrowRight" => {
                let right = code == "ArrowRight";
                if mac && primary {
                    let (a, b) = layout.line_bounds(focus).unwrap_or_default();
                    if right {
                        b
                    } else {
                        a
                    }
                } else if word {
                    Caret::at(if right {
                        next_word_end(&chars, focus.index)
                    } else {
                        prev_word_start(&chars, focus.index)
                    })
                } else if !extend && !range.is_empty() {
                    // Выделение схлопывается к своему краю.
                    Caret::at(if right { range.end } else { range.start })
                } else {
                    Caret::at(if right {
                        layout.next_stop(focus.index)
                    } else {
                        layout.prev_stop(focus.index)
                    })
                }
            }
            "ArrowUp" | "ArrowDown" => {
                let down = code == "ArrowDown";
                if mac && primary {
                    Caret::at(if down { last } else { 0 })
                } else if (mac && mods.alt) || (!mac && mods.ctrl) {
                    // Абзац: к началу текущего (или предыдущего) и к концу.
                    let p = paragraph_range(&chars, focus.index);
                    if down {
                        let end = if focus.index >= p.end {
                            paragraph_range(&chars, (p.end + 1).min(last)).end
                        } else {
                            p.end
                        };
                        Caret::at(end)
                    } else if focus.index > p.start {
                        Caret::at(p.start)
                    } else {
                        Caret::at(paragraph_range(&chars, p.start.saturating_sub(1)).start)
                    }
                } else {
                    let x = x_now();
                    goal = Some(x);
                    let from = if !extend && !range.is_empty() {
                        Caret::at(if down { range.end } else { range.start })
                    } else {
                        focus
                    };
                    layout
                        .vertical(from, x, if down { 1 } else { -1 })
                        .unwrap_or(Caret::at(if down { last } else { 0 }))
                }
            }
            "Home" | "End" => {
                let end = code == "End";
                if !mac && mods.ctrl {
                    Caret::at(if end { last } else { 0 })
                } else {
                    let (a, b) = layout.line_bounds(focus).unwrap_or_default();
                    if end {
                        b
                    } else {
                        a
                    }
                }
            }
            "PageUp" => Caret::at(0),
            "PageDown" => Caret::at(last),
            "KeyA" if primary && !mods.alt && !mods.shift => {
                let t = self.text_edit.as_mut().expect("правка идёт");
                t.anchor = 0;
                t.focus = Caret::at(last);
                t.goal_x = None;
                t.caret_attrs = None;
                return true;
            }
            _ => return false,
        };
        let target = Caret {
            index: target.index.min(last),
            upstream: target.upstream && target.index <= last,
        };
        let t = self.text_edit.as_mut().expect("правка идёт");
        if !extend {
            t.anchor = target.index;
        }
        t.focus = target;
        t.goal_x = goal;
        t.caret_attrs = None;
        true
    }
}

/// Оверлей правки и геометрия курсора (для поля ввода — и когда курсор погашен).
pub(crate) struct EditOverlay {
    pub overlay: TextEditOverlay,
    pub caret_geometry: Option<(f64, f64, f64)>,
}
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
mod draw;
mod hand;
mod select;

pub use draw::{DrawTool, Shape};
````

на:

````rust
mod draw;
mod hand;
pub(crate) mod select;

pub use draw::{DrawTool, Shape};
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
        angle: f64,
    },
}

````

на:

````rust
        angle: f64,
    },
    /// Над текстом в правке и у инструмента «Текст» (I-образный).
    Text,
}

````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    /// старые отбрасываются, и потребитель видит пропуск по метке сцены.
    pub applied: Vec<Changes>,
}

````

на:

````rust
    /// старые отбрасываются, и потребитель видит пропуск по метке сцены.
    pub applied: Vec<Changes>,
    /// Двойной щелчок по тексту: войти в его правку с курсором в точке мира.
    pub edit_text: Option<(NodeId, Point)>,
}

````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
        let cache = self.sel_cache(cx);
        let sbox = cache.sbox;
        let target = if let Some((hit, _)) = Self::box_hit(cx, sbox, e.point) {
            match hit {
````

на:

````rust
        let cache = self.sel_cache(cx);
        let sbox = cache.sbox;
        // Двойной щелчок по тексту — его правка (спека подсистемы 3, раздел 7),
        // даже над ручками рамки мелкого текста.
        if double && !shift {
            if let Some(id) = Self::pick_at(cx, &cache.parents, e.point, e.modifiers, true) {
                let text = cx.scene.slot(id).is_some_and(|s| {
                    cx.scene.at(s).props.kind == NodeKind::Text && !locked(cx.scene, s)
                });
                if text {
                    cx.selection.set(vec![id]);
                    cx.out.edit_text = Some((id, world));
                    self.state = State::Idle;
                    return;
                }
            }
        }
        let target = if let Some((hit, _)) = Self::box_hit(cx, sbox, e.point) {
            match hit {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        Cursor::Resize { angle } => (CursorKind::Resize, angle),
        Cursor::Rotate { angle } => (CursorKind::Rotate, angle),
    }
}
````

на:

````rust
        Cursor::Resize { angle } => (CursorKind::Resize, angle),
        Cursor::Rotate { angle } => (CursorKind::Rotate, angle),
        // Текстовый курсор появится в протоколе вместе с командами текста.
        Cursor::Text => (CursorKind::Default, 0.0),
    }
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Expected: PASS: `text_edit` — 10 тестов (с двойным щелчком по мелкому тексту), прежние тесты редактора и ядра — без изменений; clippy wasm32 без замечаний

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/text_edit.rs crates/ferrum-editor/src/tools/mod.rs crates/ferrum-editor/src/tools/select.rs crates/ferrum-editor/tests/common/mod.rs crates/ferrum-editor/tests/common/text.rs crates/ferrum-editor/tests/text_edit.rs crates/ferrum-wasm/src/core.rs
git commit -m "feat(editor): правка текста на холсте — вход двойным щелчком и Enter, выход Esc и щелчком мимо, курсор и выделение мышью и клавишами, мигание 1 Гц

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5: Ввод: набор, удаление, склейка шагов, IME, инструмент T

Спека 7 и 3: `text_input(kind, data)` — набор и вставка обычного текста (поверх выделения), абзац (с пунктом списка), удаление по кластеру, слову и строке (Backspace в начале пункта снимает список), составление IME (превью — содержимое в патче сцены, документ не меняется; конец — набор). Каждая правка — одна транзакция с размером рамки (`fit_patch`: «авто по ширине» с выравниванием по центру или справа растёт от выравнивания); новое содержимое считается до транзакции, поэтому шрифт, который ещё грузится, даёт `Pending` без изменений. Ввод подряд — один шаг: метка `TextMerge` (вид правки и стиль — отпечатком), новый сеанс — `Document::break_text_merge`. Инструмент T (клавиша T, команда `tool.text`) ставит черновик и правку в нём; первый символ создаёт узел той же склейкой; пустой текст при выходе исчезает без своего шага. Отмена и повтор в правке (Ctrl/Cmd+Z, Shift+Z, Ctrl+Y) не выходят из неё.

**Files:**
- Create: `crates/ferrum-editor/src/text_input.rs`, `crates/ferrum-editor/src/tools/text.rs`
- Modify: `crates/ferrum-doc/src/history.rs`, `document.rs` (`break_text_merge`), `crates/ferrum-editor/src/{text_edit,text,editor,actions,commands,lib}.rs`, `tools/mod.rs`, `crates/ferrum-wasm/src/{protocol,core}.rs` (`Tool::Text`), `apps/web/e2e/app.spec.ts` (45 команд)
- Test: `crates/ferrum-editor/tests/text_input.rs` (новый), `tests/keymap.rs`

**Interfaces:**
- Consumes: Task 1 (`insert_runs`, `slice`, `attrs_at`, `PropsPatch::content`), Task 4.
- Produces: `TextInputKind { Insert, Paragraph, Paste, Compose, Commit, DeleteBackward, DeleteForward, DeleteWordBackward, DeleteWordForward, DeleteLineBackward, DeleteLineForward }`; `Editor::text_input(kind, &str) -> Result<(), TextError>`; `ToolKind::Text`, `TextTool`, `CommandId::ToolText` (`tool.text`, KeyT); `TextEditInfo::draft`; `Document::break_text_merge()`; внутри крейта — `text::fit_patch(fonts, before, after) -> Result<PropsPatch, TextPending>`, `text_input::{splice, char_at, char_attrs, typing_attrs, clean_text}`, `Editor::{apply_text_edit, start_draft_text, restore_draft, text_edit_key}`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 44, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 45, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 44);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 45);
}

````

Создать `crates/ferrum-editor/tests/text_input.rs`:

````rust
//! Ввод в текст (подсистема 3, этап 2): набор и удаление, склейка ввода в один
//! шаг (пауза, позиция, стиль), абзац со списком, рост рамки «авто», IME —
//! превью без документа, инструмент T, пустой текст при выходе, отмена в правке.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn type_text(h: &mut H, s: &str) {
    for c in s.chars() {
        h.ed.set_time(h.t);
        h.t += 100.0;
        h.ed.text_input(TextInputKind::Insert, &c.to_string())
            .unwrap();
    }
}

#[test]
fn typing_inserts_at_the_caret_and_merges_into_one_step() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    let steps = h.ed.step_count();
    let w0 = h.props(id).w;
    h.key("Enter", none());
    h.key("End", none());
    type_text(&mut h, ", мир");
    assert_eq!(text_of(&mut h, id), "Привет, мир\n");
    assert_eq!(sel_range(&h), (11, 11));
    assert_eq!(h.ed.step_count(), steps + 1, "ввод подряд — один шаг");
    // «Авто по ширине»: рамка выросла той же правкой.
    assert!(h.props(id).w > w0 + 30.0);
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, id), "Привет\n");
    assert_eq!(h.props(id).w, w0);
    assert!(h.ed.is_editing_text(), "отмена в правке не выходит из неё");
}

#[test]
fn pause_caret_move_and_style_break_the_step() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "", 24.0);
    let steps = h.ed.step_count();
    h.key("Enter", none());
    type_text(&mut h, "ab");
    h.t += 1500.0;
    type_text(&mut h, "c");
    assert_eq!(
        h.ed.step_count(),
        steps + 2,
        "пауза больше секунды — новый шаг"
    );
    h.key("ArrowLeft", none());
    type_text(&mut h, "x");
    assert_eq!(text_of(&mut h, id), "abxc\n");
    assert_eq!(h.ed.step_count(), steps + 3, "другая позиция — новый шаг");
    // Удаление после набора — тоже новый шаг; Backspace подряд — один.
    h.ed.text_input(TextInputKind::DeleteBackward, "").unwrap();
    h.ed.text_input(TextInputKind::DeleteBackward, "").unwrap();
    assert_eq!(text_of(&mut h, id), "ac\n");
    assert_eq!(h.ed.step_count(), steps + 4);
}

#[test]
fn delete_by_cluster_word_line_and_selection() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "один два три", 24.0);
    h.key("Enter", none());
    h.key("End", none());
    h.key("Backspace", ctrl());
    assert_eq!(text_of(&mut h, id), "один два \n");
    h.key("Home", none());
    h.key("Delete", none());
    assert_eq!(text_of(&mut h, id), "дин два \n");
    h.key("ArrowRight", shift());
    h.key("ArrowRight", shift());
    h.key("Backspace", none());
    assert_eq!(text_of(&mut h, id), "н два \n");
    // Последний перевод строки не удаляется.
    h.key("End", ctrl());
    h.key("Delete", none());
    assert_eq!(text_of(&mut h, id), "н два \n");
    // Набор поверх выделения заменяет его.
    h.key("KeyA", ctrl());
    type_text(&mut h, "Ок");
    assert_eq!(text_of(&mut h, id), "Ок\n");
}

#[test]
fn typing_takes_the_style_on_the_left_and_paragraph_keeps_the_list() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    let bold = TextAttrs {
        style: Some("Bold".into()),
        ..Default::default()
    };
    let list = TextAttrs {
        list: Some(ListKind::Bullet),
        ..Default::default()
    };
    h.ed.transact(|tx| {
        tx.format_text(id, 0..1, &bold, &[])?;
        tx.format_text(id, 2..3, &list, &[])?;
        Ok(vec![id])
    })
    .unwrap();
    h.key("Enter", none());
    h.key("Home", none());
    h.key("ArrowRight", none());
    type_text(&mut h, "x");
    h.ed.text_input(TextInputKind::Paragraph, "").unwrap();
    let c = h.props(id).text.unwrap().content.clone();
    assert_eq!(c.text(), "ax\nb\n");
    assert_eq!(c.attrs_at(1).style.as_deref(), Some("Bold"));
    assert_eq!(
        c.attrs_at(2).list,
        Some(ListKind::Bullet),
        "новый пункт списка"
    );
    assert_eq!(c.attrs_at(4).list, Some(ListKind::Bullet));
    // Backspace в начале пункта снимает список, а не склеивает абзацы.
    h.key("Home", none());
    h.key("Backspace", none());
    let c = h.props(id).text.unwrap().content.clone();
    assert_eq!(c.text(), "ax\nb\n");
    assert_eq!(c.attrs_at(4).list, None);
}

#[test]
fn centered_auto_width_grows_both_ways() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    let mut props = h.props(id).text.unwrap().props.clone();
    props.align_h = AlignH::Center;
    h.set(
        id,
        PropsPatch {
            text: Some(props),
            ..Default::default()
        },
    );
    let before = h.props(id);
    h.key("Enter", none());
    h.key("End", none());
    type_text(&mut h, "cdef");
    let after = h.props(id);
    let grown = after.w - before.w;
    assert!(grown > 20.0);
    assert!((after.x - (before.x - grown / 2.0)).abs() < 1e-6);
}

#[test]
fn composition_is_a_preview_until_commit() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    let steps = h.ed.step_count();
    h.key("Enter", none());
    h.key("End", none());
    h.ed.text_input(TextInputKind::Compose, "к").unwrap();
    h.ed.text_input(TextInputKind::Compose, "ку").unwrap();
    assert_eq!(text_of(&mut h, id), "ab\n", "в документе составления нет");
    let scene_text =
        h.ed.scene()
            .node(id)
            .unwrap()
            .props
            .text
            .as_ref()
            .unwrap()
            .content
            .text();
    assert_eq!(scene_text, "abку\n");
    let o = h.ed.overlay().text_edit.unwrap();
    assert_eq!(o.composition.len(), 1);
    assert!(info(&h).composing);
    h.ed.text_input(TextInputKind::Commit, "кур").unwrap();
    assert_eq!(text_of(&mut h, id), "abкур\n");
    assert!(!h.ed.scene().has_preview());
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(sel_range(&h), (5, 5));
    // Отменённое составление (пустой текст) ничего не оставляет.
    h.ed.text_input(TextInputKind::Compose, "x").unwrap();
    h.ed.text_input(TextInputKind::Compose, "").unwrap();
    assert!(!h.ed.scene().has_preview());
    assert_eq!(text_of(&mut h, id), "abкур\n");
}

#[test]
fn text_tool_click_makes_a_draft_until_the_first_char() {
    let mut h = text_h();
    h.key("KeyT", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Text);
    h.click(200.0, 150.0, none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert!(h.ed.is_editing_text() && info(&h).draft);
    assert!(h.ed.page_doc().walk().is_empty(), "черновик не в документе");
    assert_eq!(h.ed.step_count(), 0);
    assert!(h.ed.overlay().text_edit.unwrap().caret.is_some());
    type_text(&mut h, "Hi");
    let id = info(&h).id;
    assert!(!info(&h).draft);
    assert_eq!(text_of(&mut h, id), "Hi\n");
    let p = h.props(id);
    assert_eq!((p.x, p.y), (200.0, 150.0));
    assert_eq!(p.text.unwrap().props.sizing, TextSizing::AutoWidth);
    assert_eq!(h.ed.step_count(), 1, "создание и набор — один шаг");
    h.key("Escape", none());
    assert_eq!(h.sel(), vec![id]);
    h.key("KeyZ", ctrl());
    assert!(h.ed.page_doc().walk().is_empty());
}

#[test]
fn empty_new_text_vanishes_without_a_step() {
    let mut h = text_h();
    h.key("KeyT", none());
    h.click(200.0, 150.0, none());
    h.key("Escape", none());
    assert!(!h.ed.is_editing_text());
    assert!(h.ed.page_doc().walk().is_empty());
    assert!(h.ed.scene().is_empty());
    assert_eq!(h.ed.step_count(), 0);
    // Протяжка — фиксированный размер по рамке.
    h.key("KeyT", none());
    h.drag((100.0, 100.0), (300.0, 160.0), none());
    type_text(&mut h, "x");
    let id = info(&h).id;
    let p = h.props(id);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 200.0, 60.0));
    assert_eq!(p.text.unwrap().props.sizing, TextSizing::Fixed);
}

#[test]
fn emptied_text_is_removed_on_exit_with_the_same_step() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    let steps = h.ed.step_count();
    h.key("Enter", none());
    h.key("Backspace", none());
    h.key("Escape", none());
    assert!(h.ed.scene().node(id).is_none());
    assert!(h.sel().is_empty());
    assert_eq!(h.ed.step_count(), steps + 1);
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, id), "ab\n");
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor --test text_input`
Expected: FAIL — не собирается: нет `TextInputKind` и `Editor::text_input`

- [ ] **Step 3: Реализация**

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            Err(e) => Err(rollback(p, &ops, e)),
        }
    }

````

на:

````rust
            Err(e) => Err(rollback(p, &ops, e)),
        }
    }

    /// Следующая `transact_merging` не сольётся с прошлой правкой, даже если та её
    /// продолжает: редактор начал новый сеанс правки текста.
    pub fn break_text_merge(&mut self) {
        self.history.break_merge();
    }

````

В `crates/ferrum-doc/src/history.rs` заменить:

````rust
    }

    pub fn push_redo(&mut self, entry: HistoryEntry) {
        self.redo.push(entry);
````

на:

````rust
    }

    /// Следующая сливаемая правка начинает новый шаг (новый сеанс правки текста).
    pub fn break_merge(&mut self) {
        self.last_merge = None;
    }

    pub fn push_redo(&mut self, entry: HistoryEntry) {
        self.redo.push(entry);
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::ToolLine => self.set_tool(ToolKind::Line),
            C::ToolHand => self.set_tool(ToolKind::Hand),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
````

на:

````rust
            C::ToolLine => self.set_tool(ToolKind::Line),
            C::ToolHand => self.set_tool(ToolKind::Hand),
            C::ToolText => self.set_tool(ToolKind::Text),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    ToolLine,
    ToolHand,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
````

на:

````rust
    ToolLine,
    ToolHand,
    /// «Текст» (подсистема 3).
    ToolText,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    cmd(C::ToolLine, "tool.line", "Линия", &["KeyL"]),
    cmd(C::ToolHand, "tool.hand", "Рука", &["KeyH"]),
    cmd(
        C::HandHold,
````

на:

````rust
    cmd(C::ToolLine, "tool.line", "Линия", &["KeyL"]),
    cmd(C::ToolHand, "tool.hand", "Рука", &["KeyH"]),
    cmd(C::ToolText, "tool.text", "Текст", &["KeyT"]),
    cmd(
        C::HandHold,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
use crate::text_edit::{EnterAt, TextEdit};
use crate::tools::{
    sync, Ctx, Cursor, DrawTool, Flow, HandTool, Outbox, SelectTool, Selection, Shape, Tool,
    ToolKind, ToolOverlay,
};

````

на:

````rust
use crate::text_edit::{EnterAt, TextEdit};
use crate::tools::{
    sync, Ctx, Cursor, DrawTool, Flow, HandTool, Outbox, SelectTool, Selection, Shape, TextTool,
    Tool, ToolKind, ToolOverlay,
};

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            self.set_tool(ToolKind::Move);
        }
    }

````

на:

````rust
            self.set_tool(ToolKind::Move);
        }
        // Инструмент «Текст» поставил новый текст: правка с курсором в нём.
        if let Some((parent, props)) = self.out.new_text.take() {
            self.start_draft_text(parent, props);
        }
    }

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            ToolKind::Line => Box::new(DrawTool::new(Shape::Line)),
            ToolKind::Hand => Box::new(HandTool::new()),
            // Изображению нужны хэш и размер: интерфейс сначала выбирает файл.
            ToolKind::Image => {
````

на:

````rust
            ToolKind::Line => Box::new(DrawTool::new(Shape::Line)),
            ToolKind::Hand => Box::new(HandTool::new()),
            ToolKind::Text => Box::new(TextTool::new()),
            // Изображению нужны хэш и размер: интерфейс сначала выбирает файл.
            ToolKind::Image => {
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod text;
pub mod text_edit;
pub mod tools;

````

на:

````rust
pub mod text;
pub mod text_edit;
mod text_input;
pub mod tools;

````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub use text::{fit_size, MissingFont, TextError, TextPending};
pub use text_edit::{EnterAt, TextEditInfo, CARET_BLINK_MS};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

на:

````rust
pub use text::{fit_size, MissingFont, TextError, TextPending};
pub use text_edit::{EnterAt, TextEditInfo, CARET_BLINK_MS};
pub use text_input::TextInputKind;
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

В `crates/ferrum-editor/src/text.rs` заменить:

````rust
        _ => (p.w, h),
    }))
}

````

на:

````rust
        _ => (p.w, h),
    }))
}

/// Размер и положение рамки после правки текста `before` → `after` (у
/// `AutoWidth` и `AutoHeight` — по раскладке `after`; `Fixed` — без изменений).
/// Текст «авто по ширине» растёт от своего выравнивания: по центру — в обе
/// стороны, справа — влево (как в Figma). Пустой патч — менять нечего.
pub(crate) fn fit_patch(
    fonts: &mut Fonts,
    before: &Props,
    after: &Props,
) -> Result<PropsPatch, TextPending> {
    let Some((w, h)) = fit_size(fonts, after)? else {
        return Ok(PropsPatch::default());
    };
    let mut patch = PropsPatch::default();
    if w != before.w {
        patch.w = Some(w);
        let k = match after.text_props().map(|t| (t.sizing, t.align_h)) {
            Some((TextSizing::AutoWidth, ferrum_doc::AlignH::Center)) => 0.5,
            Some((TextSizing::AutoWidth, ferrum_doc::AlignH::Right)) => 1.0,
            _ => 0.0,
        };
        if k > 0.0 {
            // Сдвиг вдоль оси x узла (с его поворотом) в координатах родителя.
            let v = kurbo::Affine::rotate(-before.rotation.to_radians())
                * kurbo::Point::new(-(w - before.w) * k, 0.0);
            patch.x = Some(before.x + v.x);
            patch.y = Some(before.y + v.y);
        }
    }
    if h != before.h {
        patch.h = Some(h);
    }
    Ok(patch)
}

````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
use std::sync::Arc;

use ferrum_doc::{NodeId, NodeKind, TextAttrs, TextContent};
use ferrum_render::TextEditOverlay;
use ferrum_text::{
````

на:

````rust
use std::sync::Arc;

use ferrum_doc::{NodeId, NodeKind, Props, TextAttrs, TextContent, TextMerge};
use ferrum_render::TextEditOverlay;
use ferrum_text::{
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
}

/// Состояние правки текста.
#[derive(Debug, Clone)]
pub(crate) struct TextEdit {
    pub id: NodeId,
    /// Начало выделения (неподвижный конец) и курсор.
    pub anchor: usize,
````

на:

````rust
}

/// Узел черновика инструмента T: в документе его нет, пока не набран первый
/// символ (пустой новый текст при выходе исчезает без шага отмены).
#[derive(Debug, Clone)]
pub(crate) struct DraftText {
    pub parent: Option<NodeId>,
    pub props: Props,
}

/// Состояние правки текста.
#[derive(Debug, Clone)]
pub(crate) struct TextEdit {
    pub id: NodeId,
    /// Новый текст инструмента T, ещё не в документе.
    pub draft: Option<DraftText>,
    /// Начало выделения (неподвижный конец) и курсор.
    pub anchor: usize,
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
    /// С какого момента курсор горит (мигание начинается заново после действия).
    pub blink_from: f64,
}

````

на:

````rust
    /// С какого момента курсор горит (мигание начинается заново после действия).
    pub blink_from: f64,
    /// Метка последней правки ввода: продолжение склеивается в один шаг.
    pub merge: Option<TextMerge>,
    /// Шагов редактора на момент `merge`: другой шаг между — склейки нет.
    pub merge_steps: u64,
}

````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
        TextEdit {
            id,
            anchor,
            focus: Caret::at(focus),
````

на:

````rust
        TextEdit {
            id,
            draft: None,
            anchor,
            focus: Caret::at(focus),
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
            clicks: None,
            blink_from: now,
        }
    }
````

на:

````rust
            clicks: None,
            blink_from: now,
            merge: None,
            merge_steps: 0,
        }
    }
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
pub struct TextEditInfo {
    pub id: NodeId,
    pub anchor: usize,
    pub focus: usize,
````

на:

````rust
pub struct TextEditInfo {
    pub id: NodeId,
    /// Узел ещё черновик (новый текст до первого символа).
    pub draft: bool,
    pub anchor: usize,
    pub focus: usize,
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
        Some(TextEditInfo {
            id: t.id,
            anchor: t.anchor,
            focus: t.focus.index,
````

на:

````rust
        Some(TextEditInfo {
            id: t.id,
            draft: t.draft.is_some(),
            anchor: t.anchor,
            focus: t.focus.index,
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
    }

    /// Содержимое правимого узла в документе (без превью IME).
    pub(crate) fn edit_content(&self) -> Option<TextContent> {
        let t = self.text_edit.as_ref()?;
        let n = self.scene.node(t.id)?;
        let doc = self.doc.loaded_page(self.page)?.props(n.id).ok()?;
````

на:

````rust
    }

    /// Содержимое правимого узла в документе (без превью IME); у черновика —
    /// его собственное.
    pub(crate) fn edit_content(&self) -> Option<TextContent> {
        let t = self.text_edit.as_ref()?;
        if let Some(d) = &t.draft {
            return d.props.text.as_ref().map(|x| x.content.clone());
        }
        let n = self.scene.node(t.id)?;
        let doc = self.doc.loaded_page(self.page)?.props(n.id).ok()?;
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
    }

    /// Выход из правки (Esc, команда): узел остаётся выделенным.
    pub fn end_text_edit(&mut self) {
        let Some(id) = self.text_edit.as_ref().map(|t| t.id) else {
````

на:

````rust
    }

    /// Выход из правки (Esc, команда): узел остаётся выделенным; пустой текст
    /// удаляется (`finish_text_edit`).
    pub fn end_text_edit(&mut self) {
        let Some(id) = self.text_edit.as_ref().map(|t| t.id) else {
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
        };
        self.finish_text_edit();
        if self.scene.slot(id).is_some() {
            self.selection.set(vec![id]);
        }
````

на:

````rust
        };
        self.finish_text_edit();
        if self.scene.slot(id).is_some() && !self.scene.is_draft(id) {
            self.selection.set(vec![id]);
        }
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
        };
        if self.scene.slot(t.id).is_none() {
            self.text_edit = None;
            self.text_hover = false;
            return;
        }
````

на:

````rust
        };
        if self.scene.slot(t.id).is_none() {
            if t.draft.is_some() {
                // Черновик убран вместе с превью чужого жеста — вернуть.
                self.restore_draft();
            } else {
                self.text_edit = None;
                self.text_hover = false;
            }
            return;
        }
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
    }

    /// Клавиши правки, кроме хода курсора: Esc — выход (`text_input` добавляет
    /// удаление, отмену, списки и форматирование).
    fn text_command_key(&mut self, code: &str, _mods: Modifiers) -> bool {
        match code {
            "Escape" => {
````

на:

````rust
    }

    /// Клавиши правки, кроме хода курсора: Esc — выход, удаление и отмена —
    /// `text_edit_key` (модуль `text_input`).
    fn text_command_key(&mut self, code: &str, mods: Modifiers) -> bool {
        match code {
            "Escape" => {
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
                true
            }
            _ => false,
        }
    }

    /// Заканчивает правку без выделения узла: превью IME убирается.
    pub(crate) fn finish_text_edit(&mut self) {
        if self.text_edit.take().is_some() {
            self.scene.clear_preview();
        }
    }
````

на:

````rust
                true
            }
            _ => self.text_edit_key(code, mods),
        }
    }
````

Создать `crates/ferrum-editor/src/text_input.rs`:

````rust
//! Ввод в текст (спека подсистемы 3, раздел 7): набор, абзац, удаление по
//! кластерам, словам и строкам, вставка обычного текста, составление IME
//! (превью в сцене подчёркнуто, в документе его нет), отмена и повтор в правке,
//! новый текст инструментом T.
//!
//! Каждая правка — одна транзакция вместе с размером рамки по раскладке
//! (`AutoWidth`, `AutoHeight`); ввод подряд — один шаг отмены (`transact_merging`):
//! шаг рвут пауза больше секунды, другая позиция курсора и другой стиль.
//! Пустой текст при выходе из правки удаляется без своего шага: новый (черновик
//! инструмента T) в документ так и не попадает, а опустевший — удаляется той же
//! склейкой, что опустошила его.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use ferrum_doc::{
    DocResult, NodeId, Props, PropsPatch, TextAttrs, TextContent, TextData, TextMerge, TextRun, Tx,
};
use ferrum_scene::Draft;
use ferrum_text::{next_word_end, prev_word_start, Caret};

use crate::editor::{Editor, EditorEvent};
use crate::input::{Modifiers, Platform};
use crate::text::{fit_patch, TextError};
use crate::text_edit::{DraftText, TextEdit};
use crate::tools::sync;

/// Вид ввода из поля (`InputEvent.inputType` и события составления).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputKind {
    /// Набор (`insertText`, `insertReplacementText`).
    Insert,
    /// Новый абзац (`insertParagraph`, `insertLineBreak`).
    Paragraph,
    /// Вставка обычного текста (`insertFromPaste`, событие `paste` без данных Ferrum).
    Paste,
    /// Составление IME (`compositionupdate`): только превью.
    Compose,
    /// Конец составления (`compositionend`): текст — в документ.
    Commit,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteLineBackward,
    DeleteLineForward,
}

/// Содержимое с заменой диапазона участками.
pub(crate) fn splice(c: &TextContent, range: Range<usize>, runs: &[TextRun]) -> TextContent {
    let len = c.char_len();
    let mut out = TextContent {
        runs: c.slice(0..range.start),
    };
    out.runs.extend(runs.iter().cloned());
    out.runs.extend(c.slice(range.end.min(len)..len));
    out.normalize();
    out
}

/// Символ в позиции (`None` — за концом).
pub(crate) fn char_at(c: &TextContent, at: usize) -> Option<char> {
    let mut pos = 0;
    for r in &c.runs {
        let n = r.text.chars().count();
        if at < pos + n {
            return r.text.chars().nth(at - pos);
        }
        pos += n;
    }
    None
}

/// Разметка символов без абзацной (список и уровень — на переводе строки).
pub(crate) fn char_attrs(a: &TextAttrs) -> TextAttrs {
    TextAttrs {
        list: None,
        indent: None,
        ..a.clone()
    }
}

/// Стиль набора: заданный для курсора, иначе стиль заменяемого (первого
/// выделенного) символа, иначе символа слева, а в начале абзаца — справа.
pub(crate) fn typing_attrs(
    c: &TextContent,
    range: Range<usize>,
    caret: Option<&TextAttrs>,
) -> TextAttrs {
    if let Some(a) = caret {
        return char_attrs(a);
    }
    let at = range.start;
    let src = if !range.is_empty() {
        at
    } else if at > 0 && char_at(c, at - 1) != Some('\n') {
        at - 1
    } else {
        at
    };
    char_attrs(c.attrs_at(src))
}

/// Отпечаток стиля для склейки шагов ввода: вид правки и разметка.
fn style_print(kind: u8, attrs: &TextAttrs) -> u64 {
    let mut h = DefaultHasher::new();
    kind.hash(&mut h);
    format!("{attrs:?}").hash(&mut h);
    h.finish()
}

/// Текст из буфера обмена: переводы строк Windows и старого macOS — `\n`,
/// прочие управляющие символы (кроме табуляции) выбрасываются.
pub(crate) fn clean_text(s: &str) -> String {
    s.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect()
}

/// Правка не черновика: черновик создаётся своим шагом.
fn draft_free(t: &TextEdit) -> bool {
    t.draft.is_none()
}

impl Editor {
    /// Ввод из поля во время правки текста. Шрифты ещё грузятся —
    /// `Err(TextError::Pending)`, ничего не изменено (ядро повторит).
    pub fn text_input(&mut self, kind: TextInputKind, data: &str) -> Result<(), TextError> {
        if self.text_edit.is_none() {
            return Ok(());
        }
        let r = match kind {
            TextInputKind::Insert | TextInputKind::Commit | TextInputKind::Paste => {
                if kind == TextInputKind::Commit {
                    self.clear_composition();
                }
                let text = clean_text(data);
                if text.is_empty() {
                    Ok(())
                } else {
                    self.replace_selection(&text, false)
                }
            }
            TextInputKind::Paragraph => self.replace_selection("\n", true),
            TextInputKind::Compose => {
                self.compose(data);
                Ok(())
            }
            TextInputKind::DeleteBackward
            | TextInputKind::DeleteForward
            | TextInputKind::DeleteWordBackward
            | TextInputKind::DeleteWordForward
            | TextInputKind::DeleteLineBackward
            | TextInputKind::DeleteLineForward => self.delete(kind),
        };
        if let Some(t) = self.text_edit.as_mut() {
            t.blink_from = self.now_ms;
        }
        self.emit();
        r
    }

    /// Свойства правимого узла в документе (у черновика — его собственные).
    fn edit_doc_props(&mut self) -> Option<Props> {
        let t = self.text_edit.as_ref()?;
        if let Some(d) = &t.draft {
            return Some(d.props.clone());
        }
        let id = t.id;
        self.doc.page(self.page).ok()?.props(id).ok()
    }

    /// Заменяет выделение текстом `text` одной правкой (набор и вставка подряд —
    /// один шаг). `paragraph` — новый абзац: перевод строки получает список и
    /// уровень текущего абзаца.
    fn replace_selection(&mut self, text: &str, paragraph: bool) -> Result<(), TextError> {
        let Some(before) = self.edit_doc_props() else {
            return Ok(());
        };
        let Some(content) = before.text.as_ref().map(|t| t.content.clone()) else {
            return Ok(());
        };
        let t = self.text_edit.as_ref().expect("правка идёт");
        let range = t.range();
        let mut attrs = typing_attrs(&content, range.clone(), t.caret_attrs.as_ref());
        if paragraph {
            let end = (range.end..content.char_len())
                .find(|i| char_at(&content, *i) == Some('\n'))
                .unwrap_or(content.char_len().saturating_sub(1));
            let p = content.attrs_at(end);
            attrs.list = p.list;
            attrs.indent = p.indent;
        }
        let runs = vec![TextRun {
            text: text.to_string(),
            attrs: attrs.clone(),
        }];
        let next = splice(&content, range.clone(), &runs);
        next.check_limits()?;
        let added = text.chars().count();
        let caret = range.start + added;
        let merge_kind = if paragraph { 2 } else { 1 };
        self.apply_text_edit(
            &before,
            next,
            range.start,
            caret,
            style_print(merge_kind, &attrs),
            |tx, id| {
                if !range.is_empty() {
                    tx.delete_text(id, range.clone())?;
                }
                tx.insert_runs(id, range.start, &runs)
            },
        )
    }

    /// Удаление выделения или куска перед курсором (за ним) по кластеру, слову
    /// или строке.
    fn delete(&mut self, kind: TextInputKind) -> Result<(), TextError> {
        let Some(before) = self.edit_doc_props() else {
            return Ok(());
        };
        let Some(content) = before.text.as_ref().map(|t| t.content.clone()) else {
            return Ok(());
        };
        let last = content.char_len().saturating_sub(1);
        let t = self.text_edit.as_ref().expect("правка идёт");
        let mut range = t.range();
        let focus = t.focus;
        if range.is_empty() {
            let at = focus.index;
            // Backspace в начале пункта списка снимает список, а не склеивает абзацы.
            if kind == TextInputKind::DeleteBackward
                && (at == 0 || char_at(&content, at - 1) == Some('\n'))
            {
                let end = (at..content.char_len())
                    .find(|i| char_at(&content, *i) == Some('\n'))
                    .unwrap_or(last);
                let p = content.attrs_at(end);
                if p.list.is_some_and(|l| l != ferrum_doc::ListKind::None) {
                    return self.unlist(&before, end);
                }
            }
            let Some(layout) = self.edit_layout() else {
                return Ok(());
            };
            let chars: Vec<char> = content.text().chars().collect();
            range = match kind {
                TextInputKind::DeleteBackward => layout.prev_stop(at)..at,
                TextInputKind::DeleteForward => at..layout.next_stop(at).min(last),
                TextInputKind::DeleteWordBackward => prev_word_start(&chars, at)..at,
                TextInputKind::DeleteWordForward => at..next_word_end(&chars, at).min(last),
                TextInputKind::DeleteLineBackward => {
                    let (a, _) = layout.line_bounds(focus).unwrap_or_default();
                    // В начале строки — как Backspace (склейка с прошлой).
                    if a.index == at {
                        layout.prev_stop(at)..at
                    } else {
                        a.index..at
                    }
                }
                TextInputKind::DeleteLineForward => {
                    let (_, b) = layout.line_bounds(focus).unwrap_or_default();
                    if b.index == at {
                        at..layout.next_stop(at).min(last)
                    } else {
                        at..b.index.min(last)
                    }
                }
                _ => return Ok(()),
            };
        }
        if range.is_empty() {
            return Ok(());
        }
        let next = splice(&content, range.clone(), &[]);
        let backward = matches!(
            kind,
            TextInputKind::DeleteBackward
                | TextInputKind::DeleteWordBackward
                | TextInputKind::DeleteLineBackward
        );
        let (from, caret) = if backward {
            (focus.index.max(range.end), range.start)
        } else {
            (range.start, range.start)
        };
        self.apply_text_edit(
            &before,
            next,
            from,
            caret,
            style_print(if backward { 3 } else { 4 }, &TextAttrs::default()),
            |tx, id| tx.delete_text(id, range.clone()),
        )
    }

    /// Снимает список с абзаца, кончающегося переводом строки `newline`.
    fn unlist(&mut self, before: &Props, newline: usize) -> Result<(), TextError> {
        let content = before.text.as_ref().expect("текст").content.clone();
        let mut next = content.clone();
        let mut runs = next.slice(newline..newline + 1);
        for r in &mut runs {
            r.attrs.list = None;
            r.attrs.indent = None;
        }
        next = splice(&next, newline..newline + 1, &runs);
        let caret = self.text_edit.as_ref().expect("правка идёт").focus.index;
        self.apply_text_edit(before, next, caret, caret, 0, |tx, id| {
            tx.format_text(
                id,
                newline..newline + 1,
                &TextAttrs::default(),
                &[ferrum_doc::AttrKey::List, ferrum_doc::AttrKey::Indent],
            )
        })
    }

    /// Правка текста одной транзакцией: `edit` меняет содержимое (`next` — каким
    /// оно станет: по нему считается размер рамки до транзакции), размер — той же
    /// транзакцией. Ввод подряд склеивается в один шаг. Черновик инструмента T
    /// становится узлом документа с первой правкой.
    pub(crate) fn apply_text_edit(
        &mut self,
        before: &Props,
        next: TextContent,
        from: usize,
        caret: usize,
        style: u64,
        edit: impl FnOnce(&Tx, NodeId) -> DocResult<()>,
    ) -> Result<(), TextError> {
        let mut after = before.clone();
        let text = before.text.as_ref().expect("текст");
        after.text = Some(Arc::new(TextData {
            props: text.props.clone(),
            content: next.clone(),
        }));
        let fit = fit_patch(&mut self.fonts.borrow_mut(), before, &after)?;
        let t = self.text_edit.as_ref().expect("правка идёт");
        let id = t.id;
        // Метка склейки — по узлу правки; у нового текста — по черновику до конца
        // сеанса (id узла появится только в транзакции).
        let merge = TextMerge {
            node: t.merge.map_or(id, |m| m.node),
            before: from,
            after: caret,
            style,
            at_ms: self.now_ms,
        };
        // Склейка — только с собственной прошлой правкой этого же сеанса правки.
        let merged = t.merge_steps == self.out.steps
            && t.merge.is_some_and(|prev| prev.continues(&merge))
            && draft_free(t);
        if !merged {
            self.doc.break_text_merge();
        }
        let draft = t.draft.clone();
        self.scene.clear_preview();
        let selection = self.selection.to_vec();
        let page = self.page;
        let result = match &draft {
            // Первый символ нового текста: узел создаётся той же правкой.
            Some(d) => self
                .doc
                .transact_merging(page, selection, merge, |tx| {
                    let mut props = after.clone();
                    fit.apply_to(&mut props);
                    let id = tx.create_with_props(d.parent, None, &props)?;
                    Ok(vec![id])
                })
                .map(|ids| ids[0]),
            None => self
                .doc
                .transact_merging(page, selection, merge, |tx| {
                    edit(tx, id)?;
                    if fit != PropsPatch::default() {
                        tx.set_props(id, &fit)?;
                    }
                    Ok(vec![id])
                })
                .map(|_| id),
        };
        let changed = sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        let id = match result {
            Ok(id) => id,
            Err(e) => {
                self.out.events.push(EditorEvent::Error(e.to_string()));
                return Err(TextError::Doc(e));
            }
        };
        if changed && !merged {
            self.out.steps += 1;
        }
        self.selection.set(vec![id]);
        let steps = self.out.steps;
        let t = self.text_edit.as_mut().expect("правка идёт");
        t.id = id;
        t.draft = None;
        t.anchor = caret;
        t.focus = Caret::at(caret);
        t.goal_x = None;
        t.merge = Some(merge);
        t.merge_steps = steps;
        Ok(())
    }

    /// Превью составляемого IME текста: в сцене (подчёркнуто оверлеем), не в
    /// документе. Пустое — превью убирается.
    fn compose(&mut self, data: &str) {
        let Some(before) = self.edit_doc_props() else {
            return;
        };
        let text = clean_text(data).replace('\n', " ");
        let t = self.text_edit.as_mut().expect("правка идёт");
        if text.is_empty() {
            t.composition = None;
            self.scene.clear_preview();
            self.restore_draft();
            return;
        }
        t.composition = Some(text.clone());
        let range = t.range();
        let Some(content) = before.text.as_ref().map(|x| x.content.clone()) else {
            return;
        };
        let attrs = typing_attrs(&content, range.clone(), t.caret_attrs.as_ref());
        let next = splice(&content, range, &[TextRun { text, attrs }]);
        let mut after = before.clone();
        let td = before.text.as_ref().expect("текст");
        after.text = Some(Arc::new(TextData {
            props: td.props.clone(),
            content: next.clone(),
        }));
        // Шрифт ещё грузится — превью без нового размера.
        let fit = fit_patch(&mut self.fonts.borrow_mut(), &before, &after).unwrap_or_default();
        let id = t.id;
        self.scene.set_preview(&[(
            id,
            PropsPatch {
                content: Some(next),
                ..fit
            },
        )]);
    }

    /// Составление кончилось: превью убирается (текст придёт вводом `Commit`).
    fn clear_composition(&mut self) {
        if let Some(t) = self.text_edit.as_mut() {
            if t.composition.take().is_some() {
                self.scene.clear_preview();
                self.restore_draft();
            }
        }
    }

    /// Черновик инструмента T снова в сцене (`clear_preview` убирает и черновики).
    pub(crate) fn restore_draft(&mut self) {
        if let Some(t) = &self.text_edit {
            if let Some(d) = &t.draft {
                self.scene.set_drafts(&[Draft {
                    id: t.id,
                    parent: d.parent,
                    props: d.props.clone(),
                }]);
            }
        }
    }

    /// Новый текст инструментом T: черновик в сцене и правка с курсором в нём.
    /// В документ он попадёт с первым символом.
    pub(crate) fn start_draft_text(&mut self, parent: Option<NodeId>, props: Props) {
        self.finish_text_edit();
        let id = self.ctx().draft_id();
        let mut props = props;
        // Размер пустой строки (у «авто») — по раскладке; шрифт грузится — как есть.
        if let Ok(patch) = fit_patch(&mut self.fonts.borrow_mut(), &props, &props) {
            patch.apply_to(&mut props);
        }
        self.scene.set_drafts(&[Draft {
            id,
            parent,
            props: props.clone(),
        }]);
        self.selection.clear();
        let mut t = TextEdit::new(id, 0, 0, self.now_ms);
        t.draft = Some(DraftText { parent, props });
        self.text_edit = Some(t);
        self.emit();
    }

    /// Заканчивает правку: превью и черновик убираются; опустевший текст
    /// удаляется склейкой с правкой, что его опустошила (без своего шага).
    pub(crate) fn finish_text_edit(&mut self) {
        let Some(t) = self.text_edit.take() else {
            return;
        };
        self.text_hover = false;
        self.scene.clear_preview();
        if t.draft.is_some() {
            return;
        }
        let empty = self
            .doc
            .page(self.page)
            .ok()
            .and_then(|p| p.props(t.id).ok())
            .and_then(|p| p.text.map(|x| x.content.char_len() <= 1))
            .unwrap_or(false);
        if !empty {
            return;
        }
        let page = self.page;
        let id = t.id;
        let result = match t.merge.filter(|_| t.merge_steps == self.out.steps) {
            Some(m) => self.doc.transact_merging(
                page,
                vec![id],
                TextMerge {
                    before: m.after,
                    ..m
                },
                |tx| {
                    tx.delete_nodes(&[id])?;
                    Ok(vec![])
                },
            ),
            None => {
                self.out.steps += 1;
                self.doc.transact(page, vec![id], |tx| {
                    tx.delete_nodes(&[id])?;
                    Ok(vec![])
                })
            }
        };
        sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        match result {
            Ok(_) => self.selection.clear(),
            Err(e) => self.out.events.push(EditorEvent::Error(e.to_string())),
        }
    }

    /// Отмена и повтор в правке: правка продолжается, если текст остался.
    pub(crate) fn text_undo_redo(&mut self, redo: bool) {
        let Some(id) = self.text_edit.as_ref().map(|t| t.id) else {
            return;
        };
        let draft = self.text_edit.as_ref().is_some_and(|t| t.draft.is_some());
        if draft {
            // Черновик ещё не в документе: отмена — к прошлым правкам.
            self.finish_text_edit();
            self.undo_redo(redo);
            return;
        }
        // Превью IME — не правка: убрать до отмены.
        self.clear_composition();
        let edit = self.text_edit.take();
        self.undo_redo(redo);
        if self.scene.slot(id).is_some() {
            self.text_edit = edit;
            self.selection.set(vec![id]);
            if let Some(t) = self.text_edit.as_mut() {
                t.merge = None;
            }
            self.clamp_text_edit();
        }
    }

    /// Клавиши удаления и отмены в правке (`text_key`).
    pub(crate) fn text_edit_key(&mut self, code: &str, mods: Modifiers) -> bool {
        let platform = self.settings.platform;
        let mac = platform == Platform::Mac;
        let primary = mods.primary(platform);
        match code {
            "Backspace" | "Delete" => {
                let back = code == "Backspace";
                let kind = if mac && mods.meta {
                    if back {
                        TextInputKind::DeleteLineBackward
                    } else {
                        TextInputKind::DeleteLineForward
                    }
                } else if (mac && mods.alt) || (!mac && mods.ctrl) {
                    if back {
                        TextInputKind::DeleteWordBackward
                    } else {
                        TextInputKind::DeleteWordForward
                    }
                } else if back {
                    TextInputKind::DeleteBackward
                } else {
                    TextInputKind::DeleteForward
                };
                if let Err(TextError::Doc(_)) = self.text_input(kind, "") {
                    // Сообщение уже отправлено событием.
                }
                true
            }
            "KeyZ" if primary && !mods.alt => {
                self.text_undo_redo(mods.shift);
                true
            }
            "KeyY" if !mac && mods.ctrl && !mods.shift && !mods.alt => {
                self.text_undo_redo(true);
                true
            }
            _ => false,
        }
    }
}
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
mod hand;
pub(crate) mod select;

pub use draw::{DrawTool, Shape};
pub use hand::HandTool;
pub use select::SelectTool;

use ferrum_doc::{Changes, DocResult, Document, NodeId, PageId, Tx};
use ferrum_render::Camera;
use ferrum_scene::Scene;
````

на:

````rust
mod hand;
pub(crate) mod select;
mod text;

pub use draw::{DrawTool, Shape};
pub use hand::HandTool;
pub use select::SelectTool;
pub use text::TextTool;

use ferrum_doc::{Changes, DocResult, Document, NodeId, PageId, Props, Tx};
use ferrum_render::Camera;
use ferrum_scene::Scene;
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    Hand,
    Image,
}

````

на:

````rust
    Hand,
    Image,
    /// «Текст» (T): новый текст щелчком или рамкой.
    Text,
}

````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    /// Двойной щелчок по тексту: войти в его правку с курсором в точке мира.
    pub edit_text: Option<(NodeId, Point)>,
}

````

на:

````rust
    /// Двойной щелчок по тексту: войти в его правку с курсором в точке мира.
    pub edit_text: Option<(NodeId, Point)>,
    /// Инструмент «Текст» закончил: новый текст (родитель и свойства) в правку.
    pub new_text: Option<(Option<NodeId>, Props)>,
}

````

Создать `crates/ferrum-editor/src/tools/text.rs`:

````rust
//! Инструмент «Текст» (T, спека подсистемы 3, раздел 7): щелчок — новый текст
//! «авто по ширине» с курсором, протяжка — текст фиксированного размера по рамке.
//! Текст встаёт в самый глубокий фрейм под точкой начала. В документ он попадёт
//! с первым символом: до того — черновик правки (`Editor::start_draft_text`).

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind, Props, TextSizing};
use kurbo::{Affine, Point, Rect};

use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay, DRAG_THRESHOLD_PX};
use crate::input::{Modifiers, PointerEvent};
use crate::selection::drop_frame_at;

/// Рамка меньше стольких пикселей мира по любой оси — это щелчок.
const MIN_FIXED: f64 = 1.0;

#[derive(Debug, Clone, Copy)]
struct Start {
    screen: Point,
    world: Point,
    parent: Option<NodeId>,
    parent_world: Affine,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Idle,
    Pressed(Start),
    Drawing(Start, Rect),
}

pub struct TextTool {
    state: State,
}

impl Default for TextTool {
    fn default() -> Self {
        TextTool::new()
    }
}

impl TextTool {
    pub fn new() -> TextTool {
        TextTool { state: State::Idle }
    }

    fn snap(cx: &Ctx, p: Point) -> Point {
        if cx.settings.pixel_snap {
            Point::new(p.x.round(), p.y.round())
        } else {
            p
        }
    }

    /// Свойства нового текста: левый верхний угол (в осях родителя) и размер
    /// рамки; без рамки — «авто по ширине».
    fn props(start: &Start, rect: Option<Rect>) -> Props {
        let mut p = Props::defaults(NodeKind::Text);
        let origin = rect.map_or(start.world, |r| r.origin());
        let local = start.parent_world.inverse() * origin;
        (p.x, p.y) = (local.x, local.y);
        if let (Some(r), Some(t)) = (rect, p.text.as_mut()) {
            let t = std::sync::Arc::make_mut(t);
            t.props.sizing = TextSizing::Fixed;
            (p.w, p.h) = (r.width(), r.height());
        }
        p
    }
}

impl Tool for TextTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Text
    }

    fn pointer_down(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        let world = Self::snap(cx, cx.to_world(e.point));
        let parent = drop_frame_at(cx.scene, world, cx.tolerance(), &HashSet::new());
        let parent_world = parent
            .and_then(|p| cx.scene.node(p))
            .map_or(Affine::IDENTITY, |n| n.world);
        *cx.hover = None;
        self.state = State::Pressed(Start {
            screen: e.point,
            world,
            parent,
            parent_world,
        });
    }

    fn pointer_move(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        let start = match self.state {
            State::Idle => return,
            State::Pressed(s) => {
                if (e.point - s.screen).hypot() < DRAG_THRESHOLD_PX {
                    return;
                }
                s
            }
            State::Drawing(s, _) => s,
        };
        let p = Self::snap(cx, cx.to_world(e.point));
        self.state = State::Drawing(start, Rect::from_points(start.world, p));
    }

    fn pointer_up(&mut self, cx: &mut Ctx, e: &PointerEvent) -> Flow {
        self.pointer_move(cx, e);
        let state = std::mem::replace(&mut self.state, State::Idle);
        let (start, rect) = match state {
            State::Idle => return Flow::Stay,
            State::Pressed(s) => (s, None),
            State::Drawing(s, r) => (
                s,
                (r.width() >= MIN_FIXED && r.height() >= MIN_FIXED).then_some(r),
            ),
        };
        cx.out.new_text = Some((start.parent, Self::props(&start, rect)));
        Flow::Finished
    }

    fn key(&mut self, _cx: &mut Ctx, _modifiers: Modifiers) {}

    fn cancel(&mut self, _cx: &mut Ctx) {
        self.state = State::Idle;
    }

    fn is_busy(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    fn cursor(&self) -> Cursor {
        Cursor::Text
    }

    fn overlay(&self) -> ToolOverlay {
        ToolOverlay {
            marquee: match self.state {
                State::Drawing(_, r) => Some(r),
                _ => None,
            },
            ..Default::default()
        }
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        Tool::Hand => ToolKind::Hand,
        Tool::Image => ToolKind::Image,
    }
}
````

на:

````rust
        Tool::Hand => ToolKind::Hand,
        Tool::Image => ToolKind::Image,
        Tool::Text => ToolKind::Text,
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        ToolKind::Hand => Tool::Hand,
        ToolKind::Image => Tool::Image,
    }
}
````

на:

````rust
        ToolKind::Hand => Tool::Hand,
        ToolKind::Image => Tool::Image,
        ToolKind::Text => Tool::Text,
    }
}
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    Hand,
    Image,
}

````

на:

````rust
    Hand,
    Image,
    Text,
}

````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `text_input` — 9 тестов, `keymap` — 45 команд, все тесты рабочего пространства зелёные

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts crates/ferrum-doc/src/document.rs crates/ferrum-doc/src/history.rs crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/text.rs crates/ferrum-editor/src/text_edit.rs crates/ferrum-editor/src/text_input.rs crates/ferrum-editor/src/tools/mod.rs crates/ferrum-editor/src/tools/text.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/text_input.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/protocol.rs
git commit -m "feat(editor): ввод в текст — набор, абзац, удаление, склейка ввода в один шаг, IME превью, инструмент T с черновиком, пустой текст при выходе без шага, отмена в правке

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6: Буфер обмена в правке текста

Спека 7: копия в правке — наш формат (копия одного текстового узла с выделенным фрагментом: формат и версия прежние, стиль участков — все ключи) и обычный текст (`EditorEvent::ClipboardText`, в протоколе — поле `plain` события `Clipboard`); вырезание — копия и удаление одним шагом. Вставка нашего формата в правке — участки текстов со стилем, переведённым к умолчаниям правимого узла; иное — выход из правки и вставка узлами. Обычный текст: в правке — набор, вне её — новый текст в центре видимой части.

**Files:**
- Create: `crates/ferrum-editor/src/text_clipboard.rs`
- Modify: `crates/ferrum-editor/src/{actions,editor,lib}.rs`, `crates/ferrum-wasm/src/{protocol,core}.rs`, `crates/ferrum-wasm/tests/{protocol,web}.rs`
- Test: `crates/ferrum-editor/tests/text_clipboard.rs` (новый)

**Interfaces:**
- Produces: `EditorEvent::ClipboardText { bytes, plain }`, `Editor::paste_plain(&str) -> Result<(), TextError>`, `CoreEvent::Clipboard { bytes, plain: Option<String> }`; `Editor::paste` в правке вставляет тексты в текст; `actions::paste_target` — `pub(crate)`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/text_clipboard.rs`:

````rust
//! Буфер обмена в правке текста (подсистема 3, этап 2): копия фрагмента — наш
//! формат и обычный текст, вставка со стилями в другой текст, вырезание одним
//! шагом, обычный текст вне правки — новый текстовый узел.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::clipboard::ClipPayload;
use ferrum_editor::*;

/// Байты и обычный текст последней копии фрагмента.
fn take_text_clip(h: &mut H) -> Option<(Vec<u8>, String)> {
    h.ed.take_events().into_iter().find_map(|e| match e {
        EditorEvent::ClipboardText { bytes, plain } => Some((bytes, plain)),
        _ => None,
    })
}

/// «Привет» кеглем 24, «При» — жирным; правка, выделено «ивет» (2..6).
fn bold_text(h: &mut H) -> NodeId {
    let id = add_text(h, 100.0, 100.0, "Привет", 24.0);
    h.ed.transact(|tx| {
        tx.format_text(
            id,
            0..3,
            &TextAttrs {
                style: Some("Bold".into()),
                ..Default::default()
            },
            &[],
        )?;
        Ok(vec![id])
    })
    .unwrap();
    h.key("Enter", none());
    h.key("Home", none());
    h.key("ArrowRight", none());
    h.key("ArrowRight", none());
    h.key("End", shift());
    id
}

#[test]
fn copy_puts_a_fragment_with_resolved_style_and_plain_text() {
    let mut h = text_h();
    bold_text(&mut h);
    h.ed.take_events();
    h.ed.execute(CommandId::Copy);
    let (bytes, plain) = take_text_clip(&mut h).expect("копия фрагмента");
    assert_eq!(plain, "ивет");
    let p = ClipPayload::decode(&bytes).unwrap();
    assert_eq!(p.nodes.len(), 1);
    let t = p.nodes[0].props.text.as_ref().unwrap();
    assert_eq!(t.content.text(), "ивет\n");
    assert_eq!(t.props.sizing, TextSizing::AutoWidth);
    let first = &t.content.runs[0].attrs;
    assert_eq!(first.style.as_deref(), Some("Bold"));
    assert_eq!(first.size, Some(24.0), "стиль — все ключи, не от умолчаний");
    assert_eq!(first.font.as_deref(), Some("Inter"));
    // Без выделения копировать нечего.
    h.key("ArrowRight", none());
    h.ed.execute(CommandId::Copy);
    assert!(take_text_clip(&mut h).is_none());
}

#[test]
fn fragment_pastes_with_its_style_into_another_text_in_one_step() {
    let mut h = text_h();
    bold_text(&mut h);
    h.ed.execute(CommandId::Copy);
    let (bytes, _) = take_text_clip(&mut h).unwrap();
    h.key("Escape", none());
    let other = add_text(&mut h, 100.0, 300.0, "ab", 12.0);
    h.key("Enter", none());
    h.key("End", none());
    let steps = h.ed.step_count();
    h.ed.paste(&bytes).unwrap();
    assert_eq!(text_of(&mut h, other), "abивет\n");
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(sel_range(&h), (6, 6));
    let c = h.props(other).text.unwrap().content.clone();
    // Кегль источника (24) отличается от умолчания цели (12) — остаётся разметкой.
    assert_eq!(c.attrs_at(2).size, Some(24.0));
    assert_eq!(c.attrs_at(2).style.as_deref(), Some("Bold"));
    assert_eq!(c.attrs_at(4).style, None, "обычное начертание — как у узла");
    // Шрифт совпал с умолчанием цели — разметки нет.
    assert_eq!(c.attrs_at(2).font, None);
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, other), "ab\n");
}

#[test]
fn cut_removes_the_fragment_and_undo_brings_it_back() {
    let mut h = text_h();
    let id = bold_text(&mut h);
    h.ed.execute(CommandId::Cut);
    assert_eq!(take_text_clip(&mut h).unwrap().1, "ивет");
    assert_eq!(text_of(&mut h, id), "Пр\n");
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, id), "Привет\n");
}

#[test]
fn non_text_payload_ends_editing_and_pastes_nodes() {
    let mut h = text_h();
    let r = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.select(&[r]);
    let bytes = h.ed.copy().unwrap();
    add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    h.key("Enter", none());
    h.ed.paste(&bytes).unwrap();
    assert!(!h.ed.is_editing_text());
    assert_eq!(h.ed.page_doc().walk().len(), 3);
}

#[test]
fn plain_text_outside_editing_makes_a_text_node_in_the_view_center() {
    let mut h = text_h();
    h.ed.paste_plain("Строка\r\nвторая").unwrap();
    let id = h.sel()[0];
    let p = h.props(id);
    assert_eq!(p.kind, NodeKind::Text);
    assert_eq!(p.text.as_ref().unwrap().content.text(), "Строка\nвторая\n");
    assert_eq!(p.text.unwrap().props.sizing, TextSizing::AutoWidth);
    let c = h.ed.camera().center();
    assert!((p.x + p.w / 2.0 - c.x).abs() <= 1.0 && (p.y + p.h / 2.0 - c.y).abs() <= 1.0);
    assert_eq!(h.ed.step_count(), 1);
    // В правке — в позицию курсора.
    h.key("Enter", none());
    h.key("Home", ctrl());
    h.ed.paste_plain("> ").unwrap();
    assert_eq!(text_of(&mut h, id), "> Строка\nвторая\n");
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        },
        CoreEvent::Page { id: "p1".into() },
        CoreEvent::Clipboard { bytes: bytes(b"x") },
        CoreEvent::ClipboardPng { bytes: bytes(b"p") },
        CoreEvent::PasteRequested,
````

на:

````rust
        },
        CoreEvent::Page { id: "p1".into() },
        CoreEvent::Clipboard {
            bytes: bytes(b"x"),
            plain: Some("Привет".into()),
        },
        CoreEvent::ClipboardPng { bytes: bytes(b"p") },
        CoreEvent::PasteRequested,
````

В `crates/ferrum-wasm/tests/web.rs` заменить:

````rust
        CoreEvent::Clipboard {
            bytes: ByteBuf::from(vec![7, 8]),
        },
        CoreEvent::Document {
````

на:

````rust
        CoreEvent::Clipboard {
            bytes: ByteBuf::from(vec![7, 8]),
            plain: None,
        },
        CoreEvent::Document {
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor --test text_clipboard`
Expected: FAIL — не собирается: нет `EditorEvent::ClipboardText` и `Editor::paste_plain`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::Undo => self.undo_redo(false),
            C::Redo => self.undo_redo(true),
            C::Copy => {
                if let Some(bytes) = self.copy() {
````

на:

````rust
            C::Undo => self.undo_redo(false),
            C::Redo => self.undo_redo(true),
            // В правке текста — фрагмент текста и обычный текст.
            C::Copy if self.text_edit.is_some() => {
                if let Some((bytes, plain)) = self.copy_text() {
                    self.out
                        .events
                        .push(EditorEvent::ClipboardText { bytes, plain });
                }
            }
            C::Cut if self.text_edit.is_some() => {
                if let Some((bytes, plain)) = self.copy_text() {
                    self.out
                        .events
                        .push(EditorEvent::ClipboardText { bytes, plain });
                    let _ = self.text_input(crate::TextInputKind::DeleteBackward, "");
                }
            }
            C::Copy => {
                if let Some(bytes) = self.copy() {
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
    /// Куда вставлять: в единственный выделенный фрейм, иначе к родителю
    /// выделения, иначе в корень. Родитель, его мировая матрица и рамка фрейма.
    fn paste_target(&self) -> (Option<NodeId>, Affine, Option<Rect>) {
        let candidate = match self.selection[..] {
            [one]
````

на:

````rust
    /// Куда вставлять: в единственный выделенный фрейм, иначе к родителю
    /// выделения, иначе в корень. Родитель, его мировая матрица и рамка фрейма.
    pub(crate) fn paste_target(&self) -> (Option<NodeId>, Affine, Option<Rect>) {
        let candidate = match self.selection[..] {
            [one]
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            }
        };
        let (target, target_world, target_box) = self.paste_target();
        let offset = self.paste_offset(payload.bounds, target_box);
````

на:

````rust
            }
        };
        // В правке текста тексты вставляются в него; иное — узлами после правки.
        if self.text_edit.is_some() {
            if self.paste_into_text(&payload) {
                return Ok(());
            }
            self.end_text_edit();
        }
        let (target, target_world, target_box) = self.paste_target();
        let offset = self.paste_offset(payload.bounds, target_box);
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Готовы байты для системного буфера обмена (формат `web application/x-ferrum`).
    Clipboard(Vec<u8>),
    /// Интерфейсу пора прочитать системный буфер и вызвать `paste`.
    PasteRequested,
````

на:

````rust
    /// Готовы байты для системного буфера обмена (формат `web application/x-ferrum`).
    Clipboard(Vec<u8>),
    /// Копия фрагмента текста в правке: наш формат и обычный текст (`text/plain`).
    ClipboardText {
        bytes: Vec<u8>,
        plain: String,
    },
    /// Интерфейсу пора прочитать системный буфер и вызвать `paste`.
    PasteRequested,
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod snap;
pub mod text;
pub mod text_edit;
mod text_input;
````

на:

````rust
pub mod snap;
pub mod text;
mod text_clipboard;
pub mod text_edit;
mod text_input;
````

Создать `crates/ferrum-editor/src/text_clipboard.rs`:

````rust
//! Буфер обмена в правке текста (спека подсистемы 3, раздел 7): копирование кладёт
//! обычный текст и наш формат (`web application/x-ferrum`) — копию из одного
//! текстового узла с выделенным фрагментом («фрагмент текста»: формат тот же,
//! версия прежняя). Вставка в текст — участки со стилями из нашего формата
//! (стиль переводится из умолчаний исходного узла в умолчания правимого), иначе
//! обычный текст в стиле позиции курсора. Обычный текст вне правки — новый
//! текстовый узел в центре видимой части (как в Figma).

use std::sync::Arc;

use ferrum_doc::{
    Fill, NodeKind, Props, TextAttrs, TextContent, TextData, TextRun, TextSizing, TextStyle,
};
use ferrum_text::text_color;
use kurbo::{Point, Vec2};

use crate::clipboard::{encodable, ClipNode, ClipPayload};
use crate::editor::{Editor, EditorEvent};
use crate::geometry::world_box;
use crate::text::{fit_patch, fit_size, TextError};
use crate::text_input::{clean_text, splice};

/// Заливка текста по умолчанию для разметки участка: сплошная цвета текста узла.
fn default_fill(fills: &[Fill]) -> Fill {
    Fill::solid(text_color(fills))
}

/// Разметка участка со всеми ключами стиля символов: так фрагмент не зависит от
/// умолчаний исходного узла.
fn resolved(a: &TextAttrs, d: &TextStyle, fill: &Fill) -> TextAttrs {
    TextAttrs {
        font: Some(a.font.clone().unwrap_or_else(|| d.font.family.clone())),
        style: Some(a.style.clone().unwrap_or_else(|| d.font.style.clone())),
        size: Some(a.size.unwrap_or(d.size)),
        fill: Some(a.fill.clone().unwrap_or_else(|| fill.clone())),
        line_height: Some(a.line_height.unwrap_or(d.line_height)),
        letter_spacing: Some(a.letter_spacing.unwrap_or(d.letter_spacing)),
        decoration: Some(a.decoration.unwrap_or(d.decoration)),
        case: Some(a.case.unwrap_or(d.case)),
        list: a.list,
        indent: a.indent,
    }
}

/// Обратно: ключи, совпавшие с умолчаниями узла, снимаются.
fn relative(a: &TextAttrs, d: &TextStyle, fill: &Fill) -> TextAttrs {
    let keep = |same: bool| !same;
    TextAttrs {
        font: a.font.clone().filter(|f| keep(*f == d.font.family)),
        style: a.style.clone().filter(|s| keep(*s == d.font.style)),
        size: a.size.filter(|s| keep(*s == d.size)),
        fill: a.fill.clone().filter(|f| keep(f == fill)),
        line_height: a.line_height.filter(|v| keep(*v == d.line_height)),
        letter_spacing: a.letter_spacing.filter(|v| keep(*v == d.letter_spacing)),
        decoration: a.decoration.filter(|v| keep(*v == d.decoration)),
        case: a.case.filter(|v| keep(*v == d.case)),
        list: a.list,
        indent: a.indent,
    }
}

/// Участки текстовых узлов копии подряд, стиль — в осях `target`: у каждого
/// узла последний перевод строки — между узлами, у последнего — отбрасывается.
/// `None` — в копии есть не только тексты.
fn payload_runs(payload: &ClipPayload, target: &Props) -> Option<Vec<TextRun>> {
    let t = target.text.as_ref()?;
    let target_fill = default_fill(&target.fills);
    let mut out: Vec<TextRun> = Vec::new();
    let roots: Vec<&ClipNode> = payload
        .nodes
        .iter()
        .filter(|n| n.parent.is_none())
        .collect();
    if payload.nodes.len() != roots.len() {
        return None;
    }
    for (i, n) in roots.iter().enumerate() {
        let src = n
            .props
            .text
            .as_ref()
            .filter(|_| n.props.kind == NodeKind::Text)?;
        let fill = default_fill(&n.props.fills);
        let len = src.content.char_len();
        let take = if i + 1 == roots.len() {
            len.saturating_sub(1)
        } else {
            len
        };
        for r in src.content.slice(0..take) {
            let full = resolved(&r.attrs, &src.props.style, &fill);
            out.push(TextRun {
                text: r.text,
                attrs: relative(&full, &t.props.style, &target_fill),
            });
        }
    }
    Some(out)
}

impl Editor {
    /// Копия выделенного фрагмента правимого текста: байты нашего формата и
    /// обычный текст. `None` — выделения нет.
    pub(crate) fn copy_text(&mut self) -> Option<(Vec<u8>, String)> {
        let t = self.text_edit.as_ref()?;
        let range = t.range();
        if range.is_empty() || t.draft.is_some() {
            return None;
        }
        let id = t.id;
        let slot = self.scene.slot(id)?;
        let mut props = self.doc.page(self.page).ok()?.props(id).ok()?;
        let text = props.text.clone()?;
        let fill = default_fill(&props.fills);
        let runs: Vec<TextRun> = text
            .content
            .slice(range)
            .into_iter()
            .map(|r| TextRun {
                attrs: resolved(&r.attrs, &text.props.style, &fill),
                text: r.text,
            })
            .collect();
        let plain: String = runs.iter().map(|r| r.text.as_str()).collect();
        let mut content = TextContent { runs };
        content.normalize();
        // Фрагмент вне правки вставится текстом «авто по ширине» в точке исходного.
        let before = props.clone();
        let mut tp = text.props.clone();
        tp.sizing = TextSizing::AutoWidth;
        props.text = Some(Arc::new(TextData {
            props: tp,
            content: TextContent {
                runs: {
                    let mut r = content.runs.clone();
                    r.push(TextRun {
                        text: "\n".into(),
                        attrs: TextAttrs::default(),
                    });
                    r
                },
            },
        }));
        if let Ok(fit) = fit_patch(&mut self.fonts.borrow_mut(), &before, &props) {
            fit.apply_to(&mut props);
        }
        let n = self.scene.at(slot);
        let origin = n.world * Point::ZERO;
        (props.x, props.y) = (origin.x, origin.y);
        let bounds = world_box(&self.scene, slot).unwrap_or_default();
        let payload = ClipPayload {
            nodes: vec![ClipNode {
                parent: None,
                props,
            }],
            images: Vec::new(),
            bounds,
        };
        if let Err(e) = encodable(&payload) {
            self.out.events.push(EditorEvent::Error(e.to_string()));
            return None;
        }
        Some((payload.encode(), plain))
    }

    /// Вставка нашего формата в правимый текст. `false` — в копии не только
    /// тексты: правка кончается, копия вставляется узлами.
    pub(crate) fn paste_into_text(&mut self, payload: &ClipPayload) -> bool {
        let Some(t) = self.text_edit.as_ref() else {
            return false;
        };
        let id = t.id;
        let range = t.range();
        let before = match &t.draft {
            Some(d) => d.props.clone(),
            None => match self.doc.page(self.page).ok().and_then(|p| p.props(id).ok()) {
                Some(p) => p,
                None => return false,
            },
        };
        let Some(runs) = payload_runs(payload, &before) else {
            return false;
        };
        let Some(content) = before.text.as_ref().map(|x| x.content.clone()) else {
            return false;
        };
        let next = splice(&content, range.clone(), &runs);
        if let Err(e) = next.check_limits() {
            self.out.events.push(EditorEvent::Error(e.to_string()));
            return true;
        }
        let caret = range.start + runs.iter().map(|r| r.text.chars().count()).sum::<usize>();
        let r = self.apply_text_edit(&before, next, range.start, caret, 5, |tx, id| {
            if !range.is_empty() {
                tx.delete_text(id, range.clone())?;
            }
            tx.insert_runs(id, range.start, &runs)
        });
        if let Err(TextError::Pending) = r {
            self.out
                .events
                .push(EditorEvent::Error(TextError::Pending.to_string()));
        }
        self.emit();
        true
    }

    /// Обычный текст из буфера обмена: в правке — в позицию курсора, вне её —
    /// новым текстом «авто по ширине» в центре видимой части (или выделенного
    /// фрейма), одним шагом.
    pub fn paste_plain(&mut self, text: &str) -> Result<(), TextError> {
        if self.text_edit.is_some() {
            return self.text_input(crate::TextInputKind::Paste, text);
        }
        let text = clean_text(text);
        if text.is_empty() {
            return Ok(());
        }
        let content = TextContent::plain(&text);
        content.check_limits()?;
        let mut p = Props::defaults(NodeKind::Text);
        p.text = Some(Arc::new(TextData {
            props: Default::default(),
            content,
        }));
        if let Some((w, h)) = fit_size(&mut self.fonts.borrow_mut(), &p)? {
            (p.w, p.h) = (w, h);
        }
        let (target, target_world, target_box) = self.paste_target();
        let center = target_box.map_or(self.view_center(), |b| b.center());
        let origin = target_world.inverse() * (center - Vec2::new(p.w / 2.0, p.h / 2.0));
        (p.x, p.y) = (origin.x.round(), origin.y.round());
        self.cancel_gesture();
        // Отказ документа (только чтение) `commit` сам отдаёт событием `Error`.
        self.ctx()
            .commit(|tx, _| Ok(vec![tx.create_with_props(target, None, &p)?]));
        self.emit();
        Ok(())
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                EditorEvent::Clipboard(bytes) => self.events.push(CoreEvent::Clipboard {
                    bytes: ByteBuf::from(bytes),
                }),
                EditorEvent::PasteRequested => self.events.push(CoreEvent::PasteRequested),
                EditorEvent::ExportPng(ids) => self.copy_png(&ids),
````

на:

````rust
                EditorEvent::Clipboard(bytes) => self.events.push(CoreEvent::Clipboard {
                    bytes: ByteBuf::from(bytes),
                    plain: None,
                }),
                EditorEvent::ClipboardText { bytes, plain } => {
                    self.events.push(CoreEvent::Clipboard {
                        bytes: ByteBuf::from(bytes),
                        plain: Some(plain),
                    })
                }
                EditorEvent::PasteRequested => self.events.push(CoreEvent::PasteRequested),
                EditorEvent::ExportPng(ids) => self.copy_png(&ids),
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        id: String,
    },
    /// Байты для системного буфера обмена (`web application/x-ferrum`).
    Clipboard {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// PNG выделения для буфера обмена («Скопировать как PNG»).
````

на:

````rust
        id: String,
    },
    /// Байты для системного буфера обмена (`web application/x-ferrum`); у
    /// фрагмента текста в правке — и обычный текст для `text/plain`.
    Clipboard {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
        #[serde(default)]
        #[tsify(type = "string | undefined")]
        plain: Option<String>,
    },
    /// PNG выделения для буфера обмена («Скопировать как PNG»).
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `text_clipboard` — 5 тестов; `ferrum-wasm` (`protocol` с полем `plain`) — зелёный

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/text_clipboard.rs crates/ferrum-editor/tests/text_clipboard.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/protocol.rs crates/ferrum-wasm/tests/web.rs
git commit -m "feat(editor): буфер обмена в правке текста — фрагмент со стилями и обычный текст, вставка со стилями, вырезание, обычный текст вне правки — новый текст

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 7: Ресайз текста по правилам Figma

Отложено с этапа 1 (решение 4 и замечание M2 финального ревью): ресайз «авто» переводил текст в `Fixed`, а превью не переносило строки. Теперь режим считает `sizing_after_resize` (только ширина — `AutoHeight`, иначе — `Fixed`), а `resize_texts` применяет его к правкам жеста холста и панели — и к превью, и к записи; у «авто по высоте» высота — по раскладке при новой ширине. Контекст инструмента получает реестр шрифтов.

**Files:**
- Modify: `crates/ferrum-editor/src/text.rs`, `tools/select.rs`, `panel.rs`, `tools/mod.rs` (`Ctx::fonts`), `editor.rs`
- Test: `crates/ferrum-editor/tests/text_resize.rs` (новый), `crates/ferrum-editor/tests/text.rs` (тест этапа 1 — ширина даёт «авто по высоте»)

**Interfaces:**
- Produces (внутри крейта): `text::sizing_after_resize(TextSizing, bool, bool) -> Option<TextSizing>`, `text::resize_texts(&mut Fonts, bases, &mut [(NodeId, PropsPatch)])`; `set_props_resizing` удалён.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-editor/tests/text.rs` заменить:

````rust

#[test]
fn resizing_text_makes_it_fixed_in_one_step() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
````

на:

````rust

#[test]
fn resizing_text_width_makes_it_auto_height_in_one_step() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
````

В `crates/ferrum-editor/tests/text.rs` заменить:

````rust
    let after = text_of(&mut h, id);
    assert_eq!(after.w, 300.0);
    assert_eq!(after.text_props().unwrap().sizing, TextSizing::Fixed);
    assert_eq!(h.ed.step_count(), steps + 1);
    h.ed.execute(CommandId::Undo);
````

на:

````rust
    let after = text_of(&mut h, id);
    assert_eq!(after.w, 300.0);
    // Только ширина — «авто по высоте» (правила Figma, этап 2); высота по тексту.
    assert_eq!(after.text_props().unwrap().sizing, TextSizing::AutoHeight);
    assert_eq!(after.h, before.h);
    assert_eq!(h.ed.step_count(), steps + 1);
    h.ed.execute(CommandId::Undo);
````

Создать `crates/ferrum-editor/tests/text_resize.rs`:

````rust
//! Ресайз текста по правилам Figma (подсистема 3, этап 2): у «авто» ширина —
//! «авто по высоте» с переносом уже в превью, высота и угол — фиксированный
//! размер; поля W и H панели — так же.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn sizing(h: &mut H, id: NodeId) -> TextSizing {
    h.props(id).text.unwrap().props.sizing
}

#[test]
fn side_handle_turns_auto_width_into_auto_height_and_rewraps_in_preview() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет большой мир", 24.0);
    let p = h.props(id);
    let (right, mid) = (100.0 + p.w, 100.0 + p.h / 2.0);
    let steps = h.ed.step_count();
    // Правая сторона — вдвое уже: в превью строки уже переносятся.
    h.drag_hold((right, mid), (100.0 + p.w / 2.0, mid), none());
    let n = h.ed.scene().node(id).unwrap();
    assert_eq!(n.props.text_props().unwrap().sizing, TextSizing::AutoHeight);
    assert!(
        n.props.h > 1.5 * p.h,
        "превью перенесло строки: {}",
        n.props.h
    );
    h.up(100.0 + p.w / 2.0, mid, none());
    let after = h.props(id);
    assert_eq!(sizing(&mut h, id), TextSizing::AutoHeight);
    assert!((after.w - p.w / 2.0).abs() <= 1.0);
    assert!(after.h > 1.5 * p.h);
    assert_eq!(h.ed.step_count(), steps + 1);
    // «Авто по высоте» по ширине остаётся собой, высота — по тексту.
    let wide = h.props(id);
    h.drag(
        (100.0 + wide.w, 100.0 + wide.h / 2.0),
        (100.0 + 3.0 * wide.w, 100.0 + wide.h / 2.0),
        none(),
    );
    assert_eq!(sizing(&mut h, id), TextSizing::AutoHeight);
    assert_eq!(h.props(id).h, p.h);
}

#[test]
fn bottom_and_corner_handles_make_text_fixed() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    let p = h.props(id);
    h.drag(
        (100.0 + p.w / 2.0, 100.0 + p.h),
        (100.0 + p.w / 2.0, 200.0),
        none(),
    );
    assert_eq!(sizing(&mut h, id), TextSizing::Fixed);
    assert_eq!(h.props(id).h, 100.0);
    h.ed.execute(CommandId::Undo);
    let p = h.props(id);
    h.drag((100.0 + p.w, 100.0 + p.h), (300.0, 300.0), none());
    assert_eq!(sizing(&mut h, id), TextSizing::Fixed);
}

#[test]
fn panel_width_is_auto_height_and_height_is_fixed() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет большой мир", 24.0);
    let p = h.props(id);
    let w = |value| PanelEdit::Number {
        field: NumField::W,
        value,
        relative: false,
    };
    // Превью поля W переносит строки так же, как запись.
    h.ed.edit_selection(&w(p.w / 2.0), Phase::Preview);
    assert!(h.ed.scene().node(id).unwrap().props.h > 1.5 * p.h);
    h.ed.edit_selection(&w(p.w / 2.0), Phase::Commit);
    assert_eq!(sizing(&mut h, id), TextSizing::AutoHeight);
    h.ed.edit_selection(
        &PanelEdit::Number {
            field: NumField::H,
            value: 300.0,
            relative: false,
        },
        Phase::Commit,
    );
    assert_eq!(sizing(&mut h, id), TextSizing::Fixed);
    assert_eq!(h.props(id).h, 300.0);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor --test text_resize`
Expected: FAIL — тесты собираются, но падают: ресайз ширины даёт `Fixed`, а не `AutoHeight`, превью не переносит строки

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            out,
            tool,
            ..
        } = self;
````

на:

````rust
            out,
            tool,
            fonts,
            ..
        } = self;
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                settings,
                out,
            },
        )
````

на:

````rust
                settings,
                out,
                fonts,
            },
        )
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
use crate::ops::reposition;
use crate::pages::clean_name;

/// Числовое поле панели свойств.
````

на:

````rust
use crate::ops::reposition;
use crate::pages::clean_name;
use crate::text::resize_texts;

/// Числовое поле панели свойств.
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
}

/// Куда положить слои, перетащенные в панели слоёв (строки сверху вниз — от
/// переднего плана к заднему).
````

на:

````rust
}

/// Свойства узлов жеста панели на его начало (выделенные и дети групп).
fn gesture_bases(g: &PanelGesture) -> impl Iterator<Item = (NodeId, &Props)> {
    g.targets.iter().flat_map(|t| {
        std::iter::once((t.id, &t.props)).chain(t.bases.iter().map(|b| (b.id, &b.props)))
    })
}

/// Куда положить слои, перетащенные в панели слоёв (строки сверху вниз — от
/// переднего плана к заднему).
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
                    self.panel = Some(capture(&self.scene, &targets(self), deep));
                }
                let p = patches(self.panel.as_ref().expect("задан выше"), edit);
                self.scene.set_preview(&p);
            }
````

на:

````rust
                    self.panel = Some(capture(&self.scene, &targets(self), deep));
                }
                let g = self.panel.as_ref().expect("задан выше");
                let mut p = patches(g, edit);
                resize_texts(&mut self.fonts.borrow_mut(), gesture_bases(g), &mut p);
                self.scene.set_preview(&p);
            }
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
                    }
                };
                let p = patches(&g, edit);
                self.scene.clear_preview();
                if !p.is_empty() {
````

на:

````rust
                    }
                };
                let mut p = patches(&g, edit);
                resize_texts(&mut self.fonts.borrow_mut(), gesture_bases(&g), &mut p);
                self.scene.clear_preview();
                if !p.is_empty() {
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
                    self.ctx().commit(|tx, _| {
                        for (id, patch) in &p {
                            crate::text::set_props_resizing(tx, *id, patch)?;
                        }
                        Ok(selection)
````

на:

````rust
                    self.ctx().commit(|tx, _| {
                        for (id, patch) in &p {
                            tx.set_props(*id, patch)?;
                        }
                        Ok(selection)
````

В `crates/ferrum-editor/src/text.rs` заменить:

````rust
//! делается: `TextPending` — ядро повторит её, когда шрифты придут.

use std::sync::Arc;

use ferrum_doc::{
    DocError, DocResult, FontName, NodeId, NodeKind, Props, PropsPatch, TextContent, TextData,
    TextProps, TextSizing, Tx,
};
use ferrum_text::{FontStatus, Fonts, LayoutRequest, SharedFonts};
````

на:

````rust
//! делается: `TextPending` — ядро повторит её, когда шрифты придут.

use std::collections::HashMap;
use std::sync::Arc;

use ferrum_doc::{
    DocError, FontName, NodeId, NodeKind, Props, PropsPatch, TextContent, TextData, TextProps,
    TextSizing,
};
use ferrum_text::{FontStatus, Fonts, LayoutRequest, SharedFonts};
````

В `crates/ferrum-editor/src/text.rs` заменить:

````rust
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

````

на:

````rust
}

/// Режим размера текста после ресайза рамки — правила Figma: у «авто» только
/// ширина (стороны слева и справа, поле W) — «авто по высоте» с новой шириной,
/// иначе (высота, угол, масштаб группы) — фиксированный. `None` — размер не менялся.
pub(crate) fn sizing_after_resize(
    sizing: TextSizing,
    width_changed: bool,
    height_changed: bool,
) -> Option<TextSizing> {
    match (sizing, width_changed, height_changed) {
        (_, false, false) | (TextSizing::Fixed, _, _) => None,
        (_, true, false) => Some(TextSizing::AutoHeight),
        _ => Some(TextSizing::Fixed),
    }
}

/// Правила ресайза к правкам текстов жеста или панели (и к превью, и к записи):
/// режим размера по `sizing_after_resize`, у «авто по высоте» — высота по
/// раскладке при новой ширине (превью переносит строки сразу). `bases` —
/// свойства узлов на начало жеста.
pub(crate) fn resize_texts<'a>(
    fonts: &mut Fonts,
    bases: impl Iterator<Item = (NodeId, &'a Props)>,
    patches: &mut [(NodeId, PropsPatch)],
) {
    let texts: HashMap<NodeId, &Props> = bases.filter(|(_, p)| p.text.is_some()).collect();
    if texts.is_empty() {
        return;
    }
    for (id, patch) in patches.iter_mut() {
        let Some(before) = texts.get(id) else {
            continue;
        };
        let Some(t) = before.text_props() else {
            continue;
        };
        if patch.text.is_some() {
            continue;
        }
        let dw = patch.w.is_some_and(|w| w != before.w);
        let dh = patch.h.is_some_and(|h| h != before.h);
        let Some(sizing) = sizing_after_resize(t.sizing, dw, dh) else {
            continue;
        };
        patch.text = Some(TextProps {
            sizing,
            ..t.clone()
        });
        if sizing == TextSizing::AutoHeight {
            let mut p = (*before).clone();
            patch.apply_to(&mut p);
            // Шрифт ещё грузится — высота прежняя, поправит следующая правка.
            if let Ok(Some((_, h))) = fit_size(fonts, &p) {
                patch.h = Some(h);
            }
        }
    }
}

````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
use ferrum_render::Camera;
use ferrum_scene::Scene;
use kurbo::{Line, Point, Rect};

````

на:

````rust
use ferrum_render::Camera;
use ferrum_scene::Scene;
use ferrum_text::SharedFonts;
use kurbo::{Line, Point, Rect};

````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    pub(crate) settings: &'a Settings,
    pub(crate) out: &'a mut Outbox,
}

````

на:

````rust
    pub(crate) settings: &'a Settings,
    pub(crate) out: &'a mut Outbox,
    /// Реестр шрифтов: ресайз текста считает высоту по раскладке.
    pub(crate) fonts: &'a SharedFonts,
}

````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
};
use crate::snap::{Snapper, SNAP_THRESHOLD_PX};

/// Двойной клик: не дольше 500 мс и не дальше 4 px от первого.
````

на:

````rust
};
use crate::snap::{Snapper, SNAP_THRESHOLD_PX};
use crate::text::resize_texts;

/// Двойной клик: не дольше 500 мс и не дальше 4 px от первого.
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
                r.transform =
                    r.sbox.to_world * rect_map(r.sbox.rect, new) * r.sbox.to_world.inverse();
                let patches = scale_patches_in(
                    &r.bases,
                    r.transform,
                    r.sbox.to_world,
                    cx.settings.pixel_snap.into(),
                );
                cx.scene.set_preview(&patches);
````

на:

````rust
                r.transform =
                    r.sbox.to_world * rect_map(r.sbox.rect, new) * r.sbox.to_world.inverse();
                let mut patches = scale_patches_in(
                    &r.bases,
                    r.transform,
                    r.sbox.to_world,
                    cx.settings.pixel_snap.into(),
                );
                resize_texts(
                    &mut cx.fonts.borrow_mut(),
                    r.bases.iter().map(|b| (b.id, &b.props)),
                    &mut patches,
                );
                cx.scene.set_preview(&patches);
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
            State::Marquee(_) => {}
            State::Resizing(r) => {
                let patches = scale_patches_in(
                    &r.bases,
                    r.transform,
                    r.sbox.to_world,
                    cx.settings.pixel_snap.into(),
                );
                commit_patches(cx, patches);
````

на:

````rust
            State::Marquee(_) => {}
            State::Resizing(r) => {
                let mut patches = scale_patches_in(
                    &r.bases,
                    r.transform,
                    r.sbox.to_world,
                    cx.settings.pixel_snap.into(),
                );
                resize_texts(
                    &mut cx.fonts.borrow_mut(),
                    r.bases.iter().map(|b| (b.id, &b.props)),
                    &mut patches,
                );
                commit_patches(cx, patches);
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
    cx.commit(|tx, _| {
        for (id, patch) in &patches {
            crate::text::set_props_resizing(tx, *id, patch)?;
        }
        Ok(sel)
````

на:

````rust
    cx.commit(|tx, _| {
        for (id, patch) in &patches {
            tx.set_props(*id, patch)?;
        }
        Ok(sel)
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `text_resize` — 3 теста; `text` (этап 1) — ширина поля W даёт «авто по высоте»

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/panel.rs crates/ferrum-editor/src/text.rs crates/ferrum-editor/src/tools/mod.rs crates/ferrum-editor/src/tools/select.rs crates/ferrum-editor/tests/text.rs crates/ferrum-editor/tests/text_resize.rs
git commit -m "feat(editor): ресайз текста по правилам Figma — ширина «авто» даёт «авто по высоте» с переносом уже в превью, высота и угол — фиксированный размер

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 8: Стиль текста из панели и сочетаний

Спека 7: сводка стиля — по выделенному диапазону в правке (у курсора — стиль следующего ввода), иначе по всем символам выделенных текстов, с «Смешанными»; правка — так же: диапазон — разметкой (`format_text`, позиции Loro живы), у курсора — стиль следующего ввода, узлы целиком — умолчанием узла и снятием разметки ключа; список — переводам строк задетых абзацев. Размер рамки — той же транзакцией, превью — фазой (`Phase`), отмена — `Cancel`; у черновика — его свойства без документа. Сочетания в правке: Ctrl/Cmd+B, I, U, Shift+X, Alt+Ctrl/Cmd+L/T/R/J, Tab и Shift+Tab.

**Files:**
- Create: `crates/ferrum-editor/src/text_style.rs`
- Modify: `crates/ferrum-editor/src/text_edit.rs`, `lib.rs`
- Test: `crates/ferrum-editor/tests/text_style.rs` (новый)

**Interfaces:**
- Produces: `TextStylePatch { font, size, line_height, letter_spacing, decoration, case, list, indent_by }`, `TextPropsPatch { sizing, align_h, align_v, paragraph_spacing, paragraph_indent, truncate, max_lines }`, `Mixed<T> { value, mixed }`, `TextStyleSummary { family, style, size, line_height, letter_spacing, decoration, case, list, sizing, align_h, align_v, paragraph_spacing, paragraph_indent, range }`; `Editor::{text_style_summary, edit_text_style(&patch, Phase, Option<&[NodeId]>), edit_text_props(...)}` → `Result<(), TextError>`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/text_style.rs`:

````rust
//! Стиль текста из панели и сочетаний (подсистема 3, этап 2): сводка со
//! «Смешанным», правка всего текста узлов и диапазона в правке, превью до
//! отпускания, стиль следующего ввода у курсора, жирный, подчёркивание,
//! выравнивание, списки и режим размера.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn content(h: &mut H, id: NodeId) -> TextContent {
    h.props(id).text.unwrap().content.clone()
}

fn size(v: f64) -> TextStylePatch {
    TextStylePatch {
        size: Some(v),
        ..Default::default()
    }
}

/// «Привет мир», «При» — жирным.
fn mixed_text(h: &mut H) -> NodeId {
    let id = add_text(h, 100.0, 100.0, "Привет мир", 24.0);
    h.ed.transact(|tx| {
        tx.format_text(
            id,
            0..3,
            &TextAttrs {
                style: Some("Bold".into()),
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
fn summary_marks_mixed_values() {
    let mut h = text_h();
    mixed_text(&mut h);
    let s = h.ed.text_style_summary().unwrap();
    assert_eq!(s.family.value, "Inter");
    assert!(!s.family.mixed);
    assert!(s.style.mixed, "жирный и обычный — «Смешанное»");
    assert_eq!(s.size.value, 24.0);
    assert!(!s.size.mixed);
    assert!(!s.range);
    // В правке — только выделенный диапазон.
    h.key("Enter", none());
    h.key("Home", none());
    h.key("ArrowRight", shift());
    let s = h.ed.text_style_summary().unwrap();
    assert!(s.range && !s.style.mixed);
    assert_eq!(s.style.value, "Bold");
    // Ничего не выделено (не текст) — сводки нет.
    h.key("Escape", none());
    h.ed.select(&[]);
    assert!(h.ed.text_style_summary().is_none());
}

#[test]
fn whole_text_edit_sets_the_default_and_clears_marks_in_one_step() {
    let mut h = text_h();
    let id = mixed_text(&mut h);
    let w0 = h.props(id).w;
    let steps = h.ed.step_count();
    let regular = TextStylePatch {
        font: Some(FontName::new("Inter", "Regular")),
        size: Some(32.0),
        ..Default::default()
    };
    h.ed.edit_text_style(&regular, Phase::Commit, None).unwrap();
    let p = h.props(id);
    let t = p.text.as_ref().unwrap();
    assert_eq!(t.props.style.size, 32.0);
    assert_eq!(t.content.runs.len(), 1, "разметка жирного снята");
    assert!(p.w > w0, "«авто по ширине» выросла той же правкой");
    assert_eq!(h.ed.step_count(), steps + 1);
    h.ed.execute(CommandId::Undo);
    assert_eq!(
        content(&mut h, id).attrs_at(0).style.as_deref(),
        Some("Bold")
    );
    assert_eq!(h.props(id).w, w0);
}

#[test]
fn range_edit_marks_only_the_selection_with_preview_until_commit() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет мир", 24.0);
    h.key("Enter", none());
    h.key("Home", none());
    for _ in 0..3 {
        h.key("ArrowRight", shift());
    }
    let steps = h.ed.step_count();
    h.ed.edit_text_style(&size(40.0), Phase::Preview, None)
        .unwrap();
    assert_eq!(
        content(&mut h, id).attrs_at(0).size,
        None,
        "в документе — нет"
    );
    let scene = h.ed.scene().node(id).unwrap().props.text.clone().unwrap();
    assert_eq!(scene.content.attrs_at(0).size, Some(40.0));
    h.ed.edit_text_style(&size(40.0), Phase::Cancel, None)
        .unwrap();
    assert!(!h.ed.scene().has_preview());
    h.ed.edit_text_style(&size(40.0), Phase::Commit, None)
        .unwrap();
    let c = content(&mut h, id);
    assert_eq!(c.attrs_at(2).size, Some(40.0));
    assert_eq!(c.attrs_at(3).size, None);
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(sel_range(&h), (0, 3), "выделение остаётся");
    // Значение, равное умолчанию узла, снимает разметку.
    h.ed.edit_text_style(&size(24.0), Phase::Commit, None)
        .unwrap();
    assert_eq!(content(&mut h, id).runs.len(), 1);
}

#[test]
fn caret_style_applies_to_the_next_typing() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    h.key("Enter", none());
    h.key("End", none());
    let steps = h.ed.step_count();
    h.key("KeyB", ctrl());
    assert_eq!(h.ed.step_count(), steps, "у курсора документ не меняется");
    assert_eq!(h.ed.text_style_summary().unwrap().style.value, "Bold");
    h.ed.text_input(TextInputKind::Insert, "c").unwrap();
    let c = content(&mut h, id);
    assert_eq!(c.attrs_at(2).style.as_deref(), Some("Bold"));
    assert_eq!(c.attrs_at(1).style, None);
}

#[test]
fn shortcuts_toggle_bold_underline_strike_and_align() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.key("Enter", none());
    h.key("KeyB", ctrl());
    assert_eq!(
        content(&mut h, id).attrs_at(0).style.as_deref(),
        Some("Bold")
    );
    h.key("KeyB", ctrl());
    assert_eq!(
        content(&mut h, id).runs.len(),
        1,
        "снова обычный — как у узла"
    );
    h.key("KeyI", ctrl());
    assert_eq!(
        content(&mut h, id).attrs_at(0).style.as_deref(),
        Some("Italic")
    );
    h.key("KeyU", ctrl());
    assert_eq!(
        content(&mut h, id).attrs_at(0).decoration,
        Some(Decoration::Underline)
    );
    h.key("KeyX", ctrl().with_shift());
    assert_eq!(
        content(&mut h, id).attrs_at(0).decoration,
        Some(Decoration::Strike)
    );
    h.key("KeyT", ctrl().with_alt());
    assert_eq!(h.props(id).text.unwrap().props.align_h, AlignH::Center);
    assert!(h.ed.is_editing_text());
}

#[test]
fn lists_apply_to_paragraphs_and_tab_changes_the_level() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "один\nдва\nтри", 24.0);
    h.key("Enter", none());
    h.key("Home", ctrl());
    h.key("ArrowDown", shift());
    h.ed.edit_text_style(
        &TextStylePatch {
            list: Some(ListKind::Bullet),
            ..Default::default()
        },
        Phase::Commit,
        None,
    )
    .unwrap();
    let c = content(&mut h, id);
    assert_eq!(c.attrs_at(4).list, Some(ListKind::Bullet));
    assert_eq!(c.attrs_at(8).list, Some(ListKind::Bullet));
    assert_eq!(c.attrs_at(12).list, None, "третий абзац не задет");
    h.key("Tab", none());
    assert_eq!(content(&mut h, id).attrs_at(4).indent, Some(1));
    h.key("Tab", shift());
    assert_eq!(content(&mut h, id).attrs_at(4).indent, None);
    let s = h.ed.text_style_summary().unwrap();
    assert_eq!(s.list.value, ListKind::Bullet);
}

#[test]
fn sizing_mode_back_to_auto_refits_the_frame() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    let w0 = h.props(id).w;
    h.ed.edit_selection(&PanelEdit::Size { w: 400.0, h: 200.0 }, Phase::Commit);
    assert_eq!(h.props(id).w, 400.0);
    h.ed.edit_text_props(
        &TextPropsPatch {
            sizing: Some(TextSizing::AutoWidth),
            ..Default::default()
        },
        Phase::Commit,
        None,
    )
    .unwrap();
    let p = h.props(id);
    assert_eq!(p.text.unwrap().props.sizing, TextSizing::AutoWidth);
    assert_eq!(p.w, w0);
}

#[test]
fn new_text_takes_the_style_chosen_before_typing() {
    let mut h = text_h();
    h.key("KeyT", none());
    h.click(200.0, 150.0, none());
    h.ed.edit_text_style(&size(40.0), Phase::Commit, None)
        .unwrap();
    assert_eq!(h.ed.step_count(), 0, "черновик — без документа");
    assert_eq!(h.ed.text_style_summary().unwrap().size.value, 40.0);
    h.ed.text_input(TextInputKind::Insert, "x").unwrap();
    let id = info(&h).id;
    assert_eq!(h.props(id).text.unwrap().props.style.size, 40.0);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor --test text_style`
Expected: FAIL — не собирается: нет `TextStylePatch`, `TextPropsPatch`, `Editor::edit_text_style`, `text_style_summary`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod text_edit;
mod text_input;
pub mod tools;

````

на:

````rust
pub mod text_edit;
mod text_input;
mod text_style;
pub mod tools;

````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub use text_edit::{EnterAt, TextEditInfo, CARET_BLINK_MS};
pub use text_input::TextInputKind;
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

на:

````rust
pub use text_edit::{EnterAt, TextEditInfo, CARET_BLINK_MS};
pub use text_input::TextInputKind;
pub use text_style::{Mixed, TextPropsPatch, TextStylePatch, TextStyleSummary};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust

    /// Клавиши правки, кроме хода курсора: Esc — выход, удаление и отмена —
    /// `text_edit_key` (модуль `text_input`).
    fn text_command_key(&mut self, code: &str, mods: Modifiers) -> bool {
        match code {
````

на:

````rust

    /// Клавиши правки, кроме хода курсора: Esc — выход, удаление и отмена —
    /// `text_edit_key` (модуль `text_input`), форматирование и списки —
    /// `text_format_key` (модуль `text_style`).
    fn text_command_key(&mut self, code: &str, mods: Modifiers) -> bool {
        match code {
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
                true
            }
            _ => self.text_edit_key(code, mods),
        }
    }
````

на:

````rust
                true
            }
            _ => self.text_edit_key(code, mods) || self.text_format_key(code, mods),
        }
    }
````

Создать `crates/ferrum-editor/src/text_style.rs`:

````rust
//! Стиль текста из панели «Текст» и сочетаний (спека подсистемы 3, раздел 7):
//! сводка со «Смешанным» (выделенный диапазон в правке, иначе все символы
//! выделенных текстов) и правка — диапазона в правке (у курсора без выделения —
//! стиль следующего ввода), иначе всего текста узлов. Одна правка — один шаг,
//! превью до отпускания (`Phase`, как у полей панели). Размер рамки «авто» —
//! той же правкой.
//!
//! Сочетания в правке: Ctrl/Cmd+B, I, U, Shift+Ctrl/Cmd+X — жирный, курсив,
//! подчёркивание, зачёркивание; Alt+Ctrl/Cmd+L, T, R, J — выравнивание; Tab и
//! Shift+Tab — уровень списка.

use std::ops::Range;
use std::sync::Arc;

use ferrum_doc::{
    AlignH, AlignV, AttrKey, Decoration, DocResult, FontName, LetterSpacing, LineHeight, ListKind,
    NodeId, NodeKind, Props, PropsPatch, TextAttrs, TextCase, TextContent, TextData, TextProps,
    TextSizing, TextStyle, Tx, MAX_LIST_INDENT,
};
use ferrum_text::{style_name::guess_attrs, FontStatus};

use crate::editor::{Editor, EditorEvent};
use crate::input::Modifiers;
use crate::panel::{PanelGesture, Phase};
use crate::text::{fit_patch, TextError};
use crate::text_input::{char_attrs, splice, typing_attrs};

/// Правка стиля символов (и списка абзацев); `None` — не трогать.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextStylePatch {
    pub font: Option<FontName>,
    pub size: Option<f64>,
    pub line_height: Option<LineHeight>,
    pub letter_spacing: Option<LetterSpacing>,
    pub decoration: Option<Decoration>,
    pub case: Option<TextCase>,
    pub list: Option<ListKind>,
    /// Уровень списка: прибавка (Tab — +1, Shift+Tab — −1).
    pub indent_by: Option<i8>,
}

/// Правка свойств текстового узла; `None` — не трогать.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextPropsPatch {
    pub sizing: Option<TextSizing>,
    pub align_h: Option<AlignH>,
    pub align_v: Option<AlignV>,
    pub paragraph_spacing: Option<f64>,
    pub paragraph_indent: Option<f64>,
    pub truncate: Option<bool>,
    pub max_lines: Option<Option<u32>>,
}

/// Значение для панели и признак «Смешанное».
#[derive(Debug, Clone, PartialEq)]
pub struct Mixed<T> {
    pub value: T,
    pub mixed: bool,
}

/// Сводка стиля для секции «Текст» (значение первого символа и «Смешанное»).
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyleSummary {
    pub family: Mixed<String>,
    pub style: Mixed<String>,
    pub size: Mixed<f64>,
    pub line_height: Mixed<LineHeight>,
    pub letter_spacing: Mixed<LetterSpacing>,
    pub decoration: Mixed<Decoration>,
    pub case: Mixed<TextCase>,
    pub list: Mixed<ListKind>,
    pub sizing: Mixed<TextSizing>,
    pub align_h: Mixed<AlignH>,
    pub align_v: Mixed<AlignV>,
    pub paragraph_spacing: Mixed<f64>,
    pub paragraph_indent: Mixed<f64>,
    /// Правится диапазон (идёт правка текста), а не узлы целиком.
    pub range: bool,
}

/// Накопитель «первое значение и было ли отличие».
struct Acc<T>(Option<Mixed<T>>);

impl<T: PartialEq + Clone> Acc<T> {
    fn add(&mut self, v: T) {
        match &mut self.0 {
            None => {
                self.0 = Some(Mixed {
                    value: v,
                    mixed: false,
                })
            }
            Some(m) => {
                if m.value != v {
                    m.mixed = true;
                }
            }
        }
    }

    fn get(self, default: T) -> Mixed<T> {
        self.0.unwrap_or(Mixed {
            value: default,
            mixed: false,
        })
    }
}

/// Ключи разметки символов, которые меняет правка.
fn char_keys(p: &TextStylePatch) -> Vec<AttrKey> {
    let mut k = Vec::new();
    if p.font.is_some() {
        k.extend([AttrKey::Font, AttrKey::Style]);
    }
    for (on, key) in [
        (p.size.is_some(), AttrKey::Size),
        (p.line_height.is_some(), AttrKey::LineHeight),
        (p.letter_spacing.is_some(), AttrKey::LetterSpacing),
        (p.decoration.is_some(), AttrKey::Decoration),
        (p.case.is_some(), AttrKey::Case),
    ] {
        if on {
            k.push(key);
        }
    }
    k
}

/// Стиль по умолчанию узла после правки.
fn apply_style(s: &TextStyle, p: &TextStylePatch) -> TextStyle {
    TextStyle {
        font: p.font.clone().unwrap_or_else(|| s.font.clone()),
        size: p.size.unwrap_or(s.size),
        line_height: p.line_height.unwrap_or(s.line_height),
        letter_spacing: p.letter_spacing.unwrap_or(s.letter_spacing),
        decoration: p.decoration.unwrap_or(s.decoration),
        case: p.case.unwrap_or(s.case),
    }
}

/// Разметка участка после правки: ключи правки задаются, а совпавшие с
/// умолчанием узла — снимаются (участок «как у узла»).
fn apply_attrs(a: &TextAttrs, p: &TextStylePatch, d: &TextStyle) -> TextAttrs {
    let mut out = a.clone();
    if let Some(f) = &p.font {
        let same = *f == d.font;
        out.font = (!same).then(|| f.family.clone());
        out.style = (!same).then(|| f.style.clone());
    }
    if let Some(v) = p.size {
        out.size = (v != d.size).then_some(v);
    }
    if let Some(v) = p.line_height {
        out.line_height = (v != d.line_height).then_some(v);
    }
    if let Some(v) = p.letter_spacing {
        out.letter_spacing = (v != d.letter_spacing).then_some(v);
    }
    if let Some(v) = p.decoration {
        out.decoration = (v != d.decoration).then_some(v);
    }
    if let Some(v) = p.case {
        out.case = (v != d.case).then_some(v);
    }
    out
}

/// Разметка перевода строки абзаца после правки списка.
fn apply_list(a: &TextAttrs, p: &TextStylePatch) -> TextAttrs {
    let mut out = a.clone();
    if let Some(l) = p.list {
        if l == ListKind::None {
            out.list = None;
            out.indent = None;
        } else {
            out.list = Some(l);
        }
    }
    if let Some(by) = p.indent_by {
        if out.list.is_some_and(|l| l != ListKind::None) {
            let level = out.indent.unwrap_or(0) as i16 + by as i16;
            let level = level.clamp(0, MAX_LIST_INDENT as i16) as u8;
            out.indent = (level > 0).then_some(level);
        }
    }
    out
}

/// Позиции переводов строк абзацев, которых касается диапазон.
fn paragraph_ends(c: &TextContent, range: Range<usize>) -> Vec<usize> {
    let text: Vec<char> = c.text().chars().collect();
    let mut start = range.start.min(text.len());
    while start > 0 && text[start - 1] != '\n' {
        start -= 1;
    }
    let end = range.end.max(range.start);
    (start..text.len())
        .filter(|i| text[*i] == '\n')
        .take_while(|i| {
            let para_start = (0..*i)
                .rev()
                .find(|j| text[*j] == '\n')
                .map_or(0, |j| j + 1);
            para_start <= end
        })
        .collect()
}

/// Содержимое после правки стиля диапазона `range` (абзацная — у абзацев).
fn restyle(c: &TextContent, range: Range<usize>, p: &TextStylePatch, d: &TextStyle) -> TextContent {
    let mut out = c.clone();
    if !char_keys(p).is_empty() && !range.is_empty() {
        let runs: Vec<_> = c
            .slice(range.clone())
            .into_iter()
            .map(|mut r| {
                r.attrs = apply_attrs(&r.attrs, p, d);
                r
            })
            .collect();
        out = splice(&out, range.clone(), &runs);
    }
    if p.list.is_some() || p.indent_by.is_some() {
        for at in paragraph_ends(c, range) {
            let mut runs = out.slice(at..at + 1);
            for r in &mut runs {
                r.attrs = apply_list(&r.attrs, p);
            }
            out = splice(&out, at..at + 1, &runs);
        }
    }
    out
}

/// Операции документа, дающие `restyle`.
fn restyle_ops(
    tx: &Tx,
    id: NodeId,
    before: &TextContent,
    range: Range<usize>,
    p: &TextStylePatch,
    d: &TextStyle,
) -> DocResult<()> {
    if !char_keys(p).is_empty() && !range.is_empty() {
        let set = apply_attrs(&TextAttrs::default(), p, d);
        let clear: Vec<AttrKey> = char_keys(p).into_iter().filter(|k| !set.has(*k)).collect();
        tx.format_text(id, range.clone(), &set, &clear)?;
    }
    if p.list.is_some() || p.indent_by.is_some() {
        for at in paragraph_ends(before, range) {
            let a = apply_list(before.attrs_at(at), p);
            let mut clear = Vec::new();
            if a.list.is_none() {
                clear.push(AttrKey::List);
            }
            if a.indent.is_none() {
                clear.push(AttrKey::Indent);
            }
            let set = TextAttrs {
                list: a.list,
                indent: a.indent,
                ..Default::default()
            };
            tx.format_text(id, at..at + 1, &set, &clear)?;
        }
    }
    Ok(())
}

/// Имя начертания по весу и курсиву (как у Figma: «Semi Bold Italic»).
fn style_name(weight: f32, italic: bool) -> String {
    let w = match weight.round() as i32 {
        ..=150 => "Thin",
        151..=250 => "Extra Light",
        251..=350 => "Light",
        351..=450 => "Regular",
        451..=550 => "Medium",
        551..=650 => "Semi Bold",
        651..=750 => "Bold",
        751..=850 => "Extra Bold",
        _ => "Black",
    };
    match (w, italic) {
        ("Regular", true) => "Italic".into(),
        (w, true) => format!("{w} Italic"),
        (w, false) => w.into(),
    }
}

impl Editor {
    /// Узлы правки стиля: правимый текст, иначе выделенные тексты (`ids` —
    /// адресат с начала жеста панели).
    fn style_targets(&self, ids: Option<&[NodeId]>) -> Vec<NodeId> {
        if let Some(t) = &self.text_edit {
            return vec![t.id];
        }
        let ids = match ids {
            Some(ids) => self.alive(ids),
            None => self.selection.to_vec(),
        };
        ids.into_iter()
            .filter(|id| {
                self.scene
                    .slot(*id)
                    .is_some_and(|s| self.scene.at(s).props.kind == NodeKind::Text)
            })
            .collect()
    }

    /// Свойства узла в документе (у черновика — его собственные).
    fn doc_props(&mut self, id: NodeId) -> Option<Props> {
        if let Some(t) = &self.text_edit {
            if t.id == id {
                if let Some(d) = &t.draft {
                    return Some(d.props.clone());
                }
            }
        }
        self.doc.page(self.page).ok()?.props(id).ok()
    }

    /// Сводка стиля для секции «Текст»; `None` — текстов нет.
    pub fn text_style_summary(&self) -> Option<TextStyleSummary> {
        let mut family = Acc(None);
        let mut style = Acc(None);
        let mut size = Acc(None);
        let mut lh = Acc(None);
        let mut ls = Acc(None);
        let mut deco = Acc(None);
        let mut case = Acc(None);
        let mut list = Acc(None);
        let mut sizing = Acc(None);
        let mut ah = Acc(None);
        let mut av = Acc(None);
        let mut ps = Acc(None);
        let mut pi = Acc(None);
        let mut add = |a: &TextAttrs, d: &TextStyle| {
            let f = a.font_name(d);
            family.add(f.family);
            style.add(f.style);
            size.add(a.size.unwrap_or(d.size));
            lh.add(a.line_height.unwrap_or(d.line_height));
            ls.add(a.letter_spacing.unwrap_or(d.letter_spacing));
            deco.add(a.decoration.unwrap_or(d.decoration));
            case.add(a.case.unwrap_or(d.case));
        };
        let edit = self.text_edit.as_ref();
        let ids: Vec<NodeId> = match edit {
            Some(t) => vec![t.id],
            None => self.style_targets(None),
        };
        if ids.is_empty() {
            return None;
        }
        for id in &ids {
            let props = match edit.and_then(|t| t.draft.as_ref()) {
                Some(d) => &d.props,
                None => &self.scene.node(*id)?.props,
            };
            let t = props.text.as_ref()?;
            let d = &t.props.style;
            let c = &t.content;
            sizing.add(t.props.sizing);
            ah.add(t.props.align_h);
            av.add(t.props.align_v);
            ps.add(t.props.paragraph_spacing);
            pi.add(t.props.paragraph_indent);
            let range = match edit {
                Some(e) => e.range(),
                None => 0..c.char_len(),
            };
            if range.is_empty() {
                // Курсор: стиль следующего ввода.
                let caret = edit.and_then(|e| e.caret_attrs.as_ref());
                add(&typing_attrs(c, range.clone(), caret), d);
            } else {
                let mut any = false;
                for r in c.slice(range.clone()) {
                    if r.text.chars().any(|ch| ch != '\n') {
                        add(&r.attrs, d);
                        any = true;
                    }
                }
                if !any {
                    add(&char_attrs(c.attrs_at(range.start)), d);
                }
            }
            for at in paragraph_ends(c, range) {
                list.add(c.attrs_at(at).list.unwrap_or(ListKind::None));
            }
        }
        let d = TextStyle::default();
        let tp = TextProps::default();
        Some(TextStyleSummary {
            family: family.get(d.font.family.clone()),
            style: style.get(d.font.style.clone()),
            size: size.get(d.size),
            line_height: lh.get(d.line_height),
            letter_spacing: ls.get(d.letter_spacing),
            decoration: deco.get(d.decoration),
            case: case.get(d.case),
            list: list.get(ListKind::None),
            sizing: sizing.get(tp.sizing),
            align_h: ah.get(tp.align_h),
            align_v: av.get(tp.align_v),
            paragraph_spacing: ps.get(0.0),
            paragraph_indent: pi.get(0.0),
            range: edit.is_some(),
        })
    }

    /// Правка стиля текста (панель «Текст», сочетания): в правке — выделенного
    /// диапазона (у курсора — стиль следующего ввода), иначе — всего текста
    /// узлов. Превью — до `Commit`, одна запись — один шаг. Шрифт ещё грузится
    /// — `Err(Pending)` при записи (ядро повторит), превью пропускается.
    pub fn edit_text_style(
        &mut self,
        patch: &TextStylePatch,
        phase: Phase,
        ids: Option<&[NodeId]>,
    ) -> Result<(), TextError> {
        if phase == Phase::Cancel {
            self.cancel_text_preview();
            self.emit();
            return Ok(());
        }
        // Курсор без выделения: стиль следующего ввода, документ не меняется.
        let caret_only = self
            .text_edit
            .as_ref()
            .is_some_and(|t| t.range().is_empty() && t.draft.is_none());
        if caret_only && !char_keys(patch).is_empty() && phase == Phase::Commit {
            let content = self.edit_content().unwrap_or_default();
            let t = self.text_edit.as_mut().expect("правка идёт");
            let base = typing_attrs(&content, t.range(), t.caret_attrs.as_ref());
            let d = self.edit_props_style().unwrap_or_default();
            let t = self.text_edit.as_mut().expect("правка идёт");
            t.caret_attrs = Some(apply_attrs(&base, patch, &d));
            if patch.list.is_none() && patch.indent_by.is_none() {
                self.emit();
                return Ok(());
            }
        }
        let mut edits: Vec<(NodeId, Props, Props, Option<Range<usize>>)> = Vec::new();
        for id in self.style_targets(ids) {
            let Some(before) = self.doc_props(id) else {
                continue;
            };
            let Some(t) = before.text.as_ref() else {
                continue;
            };
            let edit = self.text_edit.as_ref().filter(|e| e.id == id);
            let (props, content, range) = match edit {
                // Новый текст (черновик): стиль узла целиком, без документа.
                Some(e) if e.draft.is_some() => {
                    let mut p = t.props.clone();
                    p.style = apply_style(&p.style, patch);
                    let c = restyle(
                        &t.content,
                        0..t.content.char_len(),
                        &TextStylePatch {
                            font: None,
                            size: None,
                            line_height: None,
                            letter_spacing: None,
                            decoration: None,
                            case: None,
                            ..patch.clone()
                        },
                        &p.style,
                    );
                    (p, c, None)
                }
                Some(e) => {
                    let r = e.range();
                    let char_patch = if r.is_empty() {
                        TextStylePatch {
                            list: patch.list,
                            indent_by: patch.indent_by,
                            ..Default::default()
                        }
                    } else {
                        patch.clone()
                    };
                    let c = restyle(&t.content, r.clone(), &char_patch, &t.props.style);
                    (t.props.clone(), c, Some(r))
                }
                None => {
                    // Узел целиком: умолчание узла, а разметка этих ключей снимается.
                    let mut p = t.props.clone();
                    p.style = apply_style(&p.style, patch);
                    let all = 0..t.content.char_len();
                    let c = restyle(&t.content, all, patch, &p.style);
                    (p, c, None)
                }
            };
            let mut after = before.clone();
            after.text = Some(Arc::new(TextData { props, content }));
            edits.push((id, before, after, range));
        }
        self.apply_text_changes(edits, phase, patch)
    }

    /// Правка свойств текстовых узлов (режим размера, выравнивание, интервалы).
    pub fn edit_text_props(
        &mut self,
        patch: &TextPropsPatch,
        phase: Phase,
        ids: Option<&[NodeId]>,
    ) -> Result<(), TextError> {
        if phase == Phase::Cancel {
            self.cancel_text_preview();
            self.emit();
            return Ok(());
        }
        let mut edits = Vec::new();
        for id in self.style_targets(ids) {
            let Some(before) = self.doc_props(id) else {
                continue;
            };
            let Some(t) = before.text.as_ref() else {
                continue;
            };
            let mut p = t.props.clone();
            if let Some(v) = patch.sizing {
                p.sizing = v;
            }
            if let Some(v) = patch.align_h {
                p.align_h = v;
            }
            if let Some(v) = patch.align_v {
                p.align_v = v;
            }
            if let Some(v) = patch.paragraph_spacing {
                p.paragraph_spacing = v;
            }
            if let Some(v) = patch.paragraph_indent {
                p.paragraph_indent = v;
            }
            if let Some(v) = patch.truncate {
                p.truncate = v;
            }
            if let Some(v) = patch.max_lines {
                p.max_lines = v;
            }
            let mut after = before.clone();
            after.text = Some(Arc::new(TextData {
                props: p,
                content: t.content.clone(),
            }));
            edits.push((id, before, after, None));
        }
        self.apply_text_changes(edits, phase, &TextStylePatch::default())
    }

    fn edit_props_style(&self) -> Option<TextStyle> {
        let t = self.text_edit.as_ref()?;
        match &t.draft {
            Some(d) => d.props.text.as_ref().map(|x| x.props.style.clone()),
            None => self
                .scene
                .node(t.id)?
                .props
                .text
                .as_ref()
                .map(|x| x.props.style.clone()),
        }
    }

    /// Превью или запись правок текстов: свойства, содержимое и размер рамки.
    fn apply_text_changes(
        &mut self,
        edits: Vec<(NodeId, Props, Props, Option<Range<usize>>)>,
        phase: Phase,
        patch: &TextStylePatch,
    ) -> Result<(), TextError> {
        let mut patches: Vec<(NodeId, PropsPatch, Props, Option<Range<usize>>)> = Vec::new();
        let mut pending = false;
        for (id, before, after, range) in edits {
            let fit = match fit_patch(&mut self.fonts.borrow_mut(), &before, &after) {
                Ok(f) => f,
                Err(_) => {
                    pending = true;
                    PropsPatch::default()
                }
            };
            let (bt, at) = (before.text.as_ref(), after.text.as_ref());
            let p = PropsPatch {
                text: (bt.map(|t| &t.props) != at.map(|t| &t.props))
                    .then(|| at.map(|t| t.props.clone()))
                    .flatten(),
                content: (bt.map(|t| &t.content) != at.map(|t| &t.content))
                    .then(|| at.map(|t| t.content.clone()))
                    .flatten(),
                ..fit
            };
            if p != PropsPatch::default() {
                patches.push((id, p, before, range));
            }
        }
        // Черновик правится сам по себе, без документа и превью.
        if let Some(t) = self.text_edit.as_mut() {
            if let Some(d) = t.draft.as_mut() {
                if let Some((_, p, _, _)) = patches.iter().find(|x| x.0 == t.id) {
                    if phase == Phase::Commit {
                        p.apply_to(&mut d.props);
                    }
                }
                let mut preview = d.props.clone();
                if phase == Phase::Preview {
                    if let Some((_, p, _, _)) = patches.iter().find(|x| x.0 == t.id) {
                        p.apply_to(&mut preview);
                    }
                }
                let (id, parent) = (t.id, d.parent);
                self.scene.set_drafts(&[ferrum_scene::Draft {
                    id,
                    parent,
                    props: preview,
                }]);
                self.emit();
                return Ok(());
            }
        }
        match phase {
            Phase::Cancel => unreachable!("отмена — выше"),
            Phase::Preview => {
                if pending {
                    return Ok(());
                }
                let list: Vec<(NodeId, PropsPatch)> = patches
                    .iter()
                    .map(|(id, p, _, _)| (*id, p.clone()))
                    .collect();
                self.panel = Some(PanelGesture::default());
                self.scene.set_preview(&list);
                self.emit();
                Ok(())
            }
            Phase::Commit => {
                self.cancel_text_preview();
                if pending {
                    return Err(TextError::Pending);
                }
                if patches.is_empty() {
                    self.emit();
                    return Ok(());
                }
                let selection = self.selection.to_vec();
                let style = patch.clone();
                // Отказ документа `commit` сам отдаёт событием `Error`.
                self.ctx().commit(|tx, _| {
                    for (id, p, before, range) in &patches {
                        let t = before.text.as_ref().expect("текст");
                        match range {
                            // Диапазон в правке — разметкой (позиции Loro живы у
                            // других участников), без замены содержимого.
                            Some(r) => {
                                restyle_ops(
                                    tx,
                                    *id,
                                    &t.content,
                                    r.clone(),
                                    &style,
                                    &t.props.style,
                                )?;
                                tx.set_props(
                                    *id,
                                    &PropsPatch {
                                        content: None,
                                        ..p.clone()
                                    },
                                )?;
                            }
                            None if p.content.is_some() && p.text.is_some() => {
                                let d = &p.text.as_ref().expect("проверено").style;
                                let all = 0..t.content.char_len();
                                tx.set_props(
                                    *id,
                                    &PropsPatch {
                                        content: None,
                                        ..p.clone()
                                    },
                                )?;
                                restyle_ops(tx, *id, &t.content, all, &style, d)?;
                            }
                            None => tx.set_props(*id, p)?,
                        }
                    }
                    Ok(selection)
                });
                // Ввод после правки стиля — новый шаг.
                if let Some(t) = self.text_edit.as_mut() {
                    t.merge = None;
                }
                self.emit();
                Ok(())
            }
        }
    }

    /// Убирает превью правки стиля (и черновик возвращает в сцену).
    fn cancel_text_preview(&mut self) {
        if self.panel.take().is_some() {
            self.scene.clear_preview();
        }
        self.restore_draft();
    }

    /// Сочетания форматирования в правке. Возвращает, обработано ли.
    pub(crate) fn text_format_key(&mut self, code: &str, mods: Modifiers) -> bool {
        let platform = self.settings.platform;
        let primary = mods.primary(platform);
        let result = match code {
            "KeyB" | "KeyI" if primary && !mods.alt && !mods.shift => {
                let Some(s) = self.text_style_summary() else {
                    return true;
                };
                let (w, italic) = guess_attrs(&s.style.value);
                let (w, italic) = if code == "KeyB" {
                    (if w >= 600.0 { 400.0 } else { 700.0 }, italic)
                } else {
                    (w, !italic)
                };
                let font = FontName::new(s.family.value.clone(), style_name(w, italic));
                // Начертания нет в семействе — ничего (как в Figma).
                if self.fonts.borrow().status(&font) == FontStatus::Missing {
                    return true;
                }
                self.edit_text_style(
                    &TextStylePatch {
                        font: Some(font),
                        ..Default::default()
                    },
                    Phase::Commit,
                    None,
                )
            }
            "KeyU" | "KeyX" if primary && !mods.alt && (code == "KeyU") != mods.shift => {
                let want = if code == "KeyU" {
                    Decoration::Underline
                } else {
                    Decoration::Strike
                };
                let on = self
                    .text_style_summary()
                    .is_some_and(|s| s.decoration.value == want && !s.decoration.mixed);
                self.edit_text_style(
                    &TextStylePatch {
                        decoration: Some(if on { Decoration::None } else { want }),
                        ..Default::default()
                    },
                    Phase::Commit,
                    None,
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
                self.edit_text_style(
                    &TextStylePatch {
                        indent_by: Some(if mods.shift { -1 } else { 1 }),
                        ..Default::default()
                    },
                    Phase::Commit,
                    None,
                )
            }
            _ => return false,
        };
        if let Err(e) = result {
            self.out.events.push(EditorEvent::Error(e.to_string()));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_follow_figma() {
        assert_eq!(style_name(400.0, false), "Regular");
        assert_eq!(style_name(400.0, true), "Italic");
        assert_eq!(style_name(700.0, true), "Bold Italic");
        assert_eq!(style_name(600.0, false), "Semi Bold");
    }

    #[test]
    fn paragraphs_touched_by_a_range() {
        let c = TextContent::plain("аб\nвг\nде");
        assert_eq!(paragraph_ends(&c, 1..1), vec![2]);
        assert_eq!(paragraph_ends(&c, 1..4), vec![2, 5]);
        assert_eq!(paragraph_ends(&c, 6..8), vec![8]);
    }
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `text_style` — 8 тестов, юнит-тесты `text_style` — 2

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/text_edit.rs crates/ferrum-editor/src/text_style.rs crates/ferrum-editor/tests/text_style.rs
git commit -m "feat(editor): стиль текста из панели и сочетаний — сводка со «Смешанным», правка диапазона и узлов целиком с превью, стиль у курсора, жирный, курсив, подчёркивание, выравнивание, списки, режим размера

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 9: Протокол и ядро worker'а

Спека 8: команды правки текста с проверкой недоверенного ввода (`core/text.rs`), события `TextCaret` (когда сменились правка, прямоугольник или выделение) и `TextStyleSummary` (после правок, выделения и хода курсора), `PageFonts` — вместе с отсутствующими шрифтами (тот же ключ пересчёта). Команды текста, пришедшие, пока в очереди ждут шрифтов, встают за ними; повтор очереди держит порядок. Время ядра: `WasmCore` передаёт `performance.now()` в начале кадра и перед командой (`Core::set_time`). Курсор `text`; TypeScript — новый вид курсора.

**Files:**
- Create: `crates/ferrum-wasm/src/core/text.rs`
- Modify: `crates/ferrum-wasm/src/{protocol,core,core/fonts,core/panels,web/mod}.rs`, `apps/web/src/core/core.ts` (`cssCursor`)
- Test: `crates/ferrum-wasm/tests/text_edit.rs` (новый)

**Interfaces:**
- Consumes: Tasks 4–8.
- Produces (протокол, TypeScript через tsify): `Command::{TextEditStart { id }, TextEditEnd, TextInput { kind: TextInputKindDto, data? }, TextKey { key, mods: KeyMods }, SetTextStyle { patch: TextStylePatchDto, phase, ids? }, SetTextProps { patch: TextPropsPatchDto, phase, ids? }}`; `CoreEvent::{TextCaret { editing, rect?, selection?, draft }, TextStyleSummary { summary?: TextStyleDto }, PageFonts { families }}`; `SpacingDto { unit: auto | px | percent, value }`, `MixedString`, `MixedSpacing`; `CursorKind::Text`; `Core::set_time(f64)`; `MAX_KEY_CODE_BYTES = 64`, `MAX_FONT_SIZE = 10 000`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-wasm/tests/text_edit.rs`:

````rust
//! Правка текста в ядре worker'а (подсистема 3, этап 2): команды ввода, клавиш,
//! стиля и свойств, события курсора и сводки стиля, пределы недоверенного ввода,
//! очередь правок, ждущих шрифтов, мигание курсора.
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use ferrum_editor::Platform;
use ferrum_text::read_faces;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;

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

/// Ядро со списком вшитых Inter и Roboto; Inter загружен, на странице — текст
/// «Привет» (выделен).
fn core() -> (Core, String) {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(400.0, 300.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::Demo { nodes: 0 });
    let faces = FILES
        .iter()
        .flat_map(|r| read_faces(&font_file(r), r))
        .collect::<Vec<_>>();
    let families = ferrum_text::group(faces)
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
        .collect();
    c.command(Command::FontsAvailable {
        source: FontSource::Bundled,
        families,
    });
    for f in &FILES[..2] {
        c.load_font_for_tests("Inter", f, font_file(f));
    }
    c.command(Command::TestCreateText {
        x: 20.0,
        y: 20.0,
        text: "Привет".into(),
        font: None,
        size: Some(24.0),
        sizing: None,
        width: None,
        height: None,
    });
    let id = c.editor().selection()[0].to_string();
    c.take_events();
    (c, id)
}

/// Последнее `TextCaret` пачки: правка, выделение, прямоугольник.
type CaretState = (bool, Option<[u32; 2]>, Option<[f64; 4]>);

fn caret(events: &[CoreEvent]) -> Option<CaretState> {
    events.iter().rev().find_map(|e| match e {
        CoreEvent::TextCaret {
            editing,
            selection,
            rect,
            ..
        } => Some((*editing, *selection, *rect)),
        _ => None,
    })
}

fn summary(events: &[CoreEvent]) -> Option<Option<TextStyleDto>> {
    events.iter().rev().find_map(|e| match e {
        CoreEvent::TextStyleSummary { summary } => Some(summary.clone()),
        _ => None,
    })
}

fn text(c: &Core) -> String {
    let id = c.editor().selection()[0];
    c.editor()
        .scene()
        .node(id)
        .unwrap()
        .props
        .text
        .as_ref()
        .unwrap()
        .content
        .text()
}

fn input(c: &mut Core, kind: TextInputKindDto, data: &str) {
    c.command(Command::TextInput {
        kind,
        data: Some(data.into()),
    });
}

fn key(c: &mut Core, key: &str, mods: KeyMods) {
    c.command(Command::TextKey {
        key: key.into(),
        mods,
    });
}

#[test]
fn edit_start_input_and_end_report_the_caret() {
    let (mut c, id) = core();
    c.command(Command::TextEditStart { id });
    let (editing, sel, rect) = caret(&c.take_events()).unwrap();
    assert!(editing);
    assert_eq!(sel, Some([0, 6]));
    let r = rect.unwrap();
    // Камера примера — зум 0.5: строка 29 px мира — 14,5 px экрана.
    assert!(r[3] > 12.0, "высота курсора — строка: {r:?}");
    key(&mut c, "End", KeyMods::default());
    input(&mut c, TextInputKindDto::Insert, "!");
    let events = c.take_events();
    assert_eq!(caret(&events).unwrap().1, Some([7, 7]));
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::Document { .. })));
    assert_eq!(text(&c), "Привет!\n");
    key(&mut c, "Escape", KeyMods::default());
    let (editing, sel, _) = caret(&c.take_events()).unwrap();
    assert!(!editing && sel.is_none());
}

#[test]
fn untrusted_text_commands_are_bounded() {
    let (mut c, id) = core();
    c.command(Command::TextEditStart { id });
    c.take_events();
    input(&mut c, TextInputKindDto::Insert, &"я".repeat(40_000));
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { message } if message.contains("64 КБ"))));
    assert_eq!(text(&c), "Привет\n");
    key(&mut c, &"K".repeat(100), KeyMods::default());
    assert!(c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { .. })));
    // Неизвестный ключ правки стиля не разбирается.
    let bad = serde_json::json!({
        "type": "setTextStyle", "phase": "commit", "patch": {"size": 12, "weight": 700}
    });
    assert!(serde_json::from_value::<Command>(bad).is_err());
    // Не число — ошибка, документ тот же.
    c.command(Command::SetTextStyle {
        patch: TextStylePatchDto {
            size: Some(f64::NAN),
            ..Default::default()
        },
        phase: EditPhase::Commit,
        ids: None,
    });
    assert!(c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { message } if message.contains("не число"))));
}

#[test]
fn style_and_props_commands_edit_and_report_the_summary() {
    let (mut c, _) = core();
    c.command(Command::SetTextStyle {
        patch: TextStylePatchDto {
            size: Some(40.0),
            line_height: Some(SpacingDto {
                unit: SpacingUnit::Percent,
                value: 150.0,
            }),
            ..Default::default()
        },
        phase: EditPhase::Commit,
        ids: None,
    });
    let s = summary(&c.take_events()).unwrap().unwrap();
    assert_eq!(s.size.value, 40.0);
    assert_eq!(s.line_height.value.unit, SpacingUnit::Percent);
    assert!(!s.range);
    c.command(Command::SetTextProps {
        patch: TextPropsPatchDto {
            align_h: Some(AlignHDto::Right),
            ..Default::default()
        },
        phase: EditPhase::Commit,
        ids: None,
    });
    let s = summary(&c.take_events()).unwrap().unwrap();
    assert_eq!(s.align_h.value, "right");
    // Без выделения сводки нет.
    c.command(Command::Select {
        ids: vec![],
        mode: SelectMode::Replace,
    });
    assert_eq!(summary(&c.take_events()), Some(None));
}

#[test]
fn style_waiting_for_a_font_keeps_the_order_of_typing() {
    let (mut c, id) = core();
    c.command(Command::TextEditStart { id });
    c.command(Command::SetTextStyle {
        patch: TextStylePatchDto {
            family: Some("Roboto".into()),
            style: Some("Regular".into()),
            ..Default::default()
        },
        phase: EditPhase::Commit,
        ids: None,
    });
    // Roboto ещё не загружен: правка ждёт, набор за ней — тоже.
    input(&mut c, TextInputKindDto::Insert, "!");
    assert_eq!(text(&c), "Привет\n");
    let events = c.take_events();
    let req = events
        .iter()
        .find_map(|e| match e {
            CoreEvent::FontRequest { family, files, .. } if family == "Roboto" => {
                Some(files.clone())
            }
            _ => None,
        })
        .expect("запрос Roboto");
    for f in req {
        c.load_font_for_tests("Roboto", &f, font_file(&f));
    }
    assert_eq!(
        text(&c),
        "!\n",
        "сначала стиль, затем набор поверх выделения"
    );
    let s = summary(&c.take_events()).unwrap().unwrap();
    assert_eq!(s.family.value, "Roboto");
}

#[test]
fn plain_paste_outside_editing_creates_text() {
    let (mut c, _) = core();
    let before = c.editor().scene().len();
    input(&mut c, TextInputKindDto::Paste, "Из буфера");
    assert_eq!(c.editor().scene().len(), before + 1);
    assert_eq!(text(&c), "Из буфера\n");
}

#[test]
fn caret_blink_requests_frames() {
    let (mut c, id) = core();
    c.set_time(1000.0);
    c.command(Command::TextEditStart { id });
    key(&mut c, "End", KeyMods::default());
    let mut b = ferrum_render::CpuBackend::new(400, 300);
    c.draw(&mut b);
    assert!(!c.wants_frame());
    c.set_time(1100.0);
    assert!(!c.wants_frame(), "курсор ещё горит — кадр не нужен");
    c.set_time(1600.0);
    assert!(c.wants_frame(), "курсор погас — нужен кадр");
}

#[test]
fn text_tool_and_editing_show_the_text_cursor() {
    let (mut c, _) = core();
    c.command(Command::SetTool { tool: Tool::Text });
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::Tool { tool: Tool::Text })));
    assert!(events.iter().any(|e| matches!(
        e,
        CoreEvent::Cursor {
            cursor: CursorKind::Text,
            ..
        }
    )));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-wasm --test text_edit`
Expected: FAIL — не собирается: нет `Command::TextInput`, `TextKey`, `SetTextStyle`, событий `TextCaret`, `TextStyleSummary`, `Core::set_time`

- [ ] **Step 3: Реализация**

В `apps/web/src/core/core.ts` заменить:

````ts
    case 'rotate':
      return 'alias';
    case 'resize': {
      const a = ((angle % 180) + 180) % 180;
````

на:

````ts
    case 'rotate':
      return 'alias';
    case 'text':
      return 'text';
    case 'resize': {
      const a = ((angle % 180) + 180) % 180;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
mod fonts;
mod panels;

use std::collections::{HashSet, VecDeque};
````

на:

````rust
mod fonts;
mod panels;
mod text;

use std::collections::{HashSet, VecDeque};
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    fonts: SharedFonts,
    font_state: fonts::FontState,
}

````

на:

````rust
    fonts: SharedFonts,
    font_state: fonts::FontState,
    /// Что ушло полю ввода и секции «Текст» (подсистема 3, этап 2).
    text_state: text::TextState,
}

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        Cursor::Resize { angle } => (CursorKind::Resize, angle),
        Cursor::Rotate { angle } => (CursorKind::Rotate, angle),
        // Текстовый курсор появится в протоколе вместе с командами текста.
        Cursor::Text => (CursorKind::Default, 0.0),
    }
}
````

на:

````rust
        Cursor::Resize { angle } => (CursorKind::Resize, angle),
        Cursor::Rotate { angle } => (CursorKind::Rotate, angle),
        Cursor::Text => (CursorKind::Text, 0.0),
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            fonts,
            font_state: fonts::FontState::default(),
        };
        // Таблица команд и пресеты нужны интерфейсу сразу, до документа.
````

на:

````rust
            fonts,
            font_state: fonts::FontState::default(),
            text_state: text::TextState::default(),
        };
        // Таблица команд и пресеты нужны интерфейсу сразу, до документа.
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        // Ждавшие шрифтов правки относились к прошлому документу.
        self.font_state.waiting.clear();
        self.preload_page_fonts();
        Ok(())
````

на:

````rust
        // Ждавшие шрифтов правки относились к прошлому документу.
        self.font_state.waiting.clear();
        // Поле ввода и секция «Текст» узнают о новом документе заново.
        self.text_state = text::TextState::default();
        self.preload_page_fonts();
        Ok(())
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            Command::OpenWithJournal { bytes, journal } => self.open_with_journal(&bytes, &journal),
            Command::FlushJournal => self.flush_journal(true),
            Command::Execute { command } => match CommandId::from_key(&command) {
                Some(id) => self.editor.execute(id),
````

на:

````rust
            Command::OpenWithJournal { bytes, journal } => self.open_with_journal(&bytes, &journal),
            Command::FlushJournal => self.flush_journal(true),
            // Правка текста: после ждущих шрифтов правок — по порядку.
            cmd if text::is_text_command(&cmd) => {
                if self.font_state.waiting.is_empty() {
                    self.text_command(cmd);
                } else {
                    self.font_state.waiting.push_back(cmd);
                }
            }
            Command::Execute { command } => match CommandId::from_key(&command) {
                Some(id) => self.editor.execute(id),
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            self.push_document();
        }
        self.flush_ui();
        self.push_modified();
        self.push_font_requests();
````

на:

````rust
            self.push_document();
        }
        let properties = self.ui.properties;
        self.flush_ui();
        self.push_text_state(properties);
        self.push_modified();
        self.push_font_requests();
````

В `crates/ferrum-wasm/src/core/fonts.rs` заменить:

````rust
    pub waiting: VecDeque<Command>,
    pub missing_sent: Option<Vec<MissingFontDto>>,
    pub catalog_sent: Option<Vec<FontChoice>>,
    /// При чём посчитаны отсутствующие: страница, экземпляр сцены, ревизия
````

на:

````rust
    pub waiting: VecDeque<Command>,
    pub missing_sent: Option<Vec<MissingFontDto>>,
    /// Семейства текстов страницы, что ушли последними (`PageFonts`).
    pub page_sent: Option<Vec<String>>,
    pub catalog_sent: Option<Vec<FontChoice>>,
    /// При чём посчитаны отсутствующие: страница, экземпляр сцены, ревизия
````

В `crates/ferrum-wasm/src/core/fonts.rs` заменить:

````rust

    /// Правка текста; ждущая шрифтов — в очередь (повтор — `after_fonts_changed`).
    fn text_command(&mut self, cmd: Command) {
        let result = match &cmd {
            Command::ReplaceMissingFont { from, to } => match (font_name(from), font_name(to)) {
````

на:

````rust

    /// Правка текста; ждущая шрифтов — в очередь (повтор — `after_fonts_changed`).
    pub(crate) fn text_command(&mut self, cmd: Command) {
        let result = match &cmd {
            Command::ReplaceMissingFont { from, to } => match (font_name(from), font_name(to)) {
````

В `crates/ferrum-wasm/src/core/fonts.rs` заменить:

````rust
                }
            },
            Command::TestCreateText {
                x,
````

на:

````rust
                }
            },
            c if super::text::is_text_command(c) => self.run_text(c),
            Command::TestCreateText {
                x,
````

В `crates/ferrum-wasm/src/core/fonts.rs` заменить:

````rust
        let waiting: Vec<Command> = self.font_state.waiting.drain(..).collect();
        for cmd in waiting {
            self.text_command(cmd);
        }
        self.push_font_requests();
````

на:

````rust
        let waiting: Vec<Command> = self.font_state.waiting.drain(..).collect();
        for cmd in waiting {
            // Правка снова ждёт — следующие за ней тоже ждут: порядок команд текста
            // (стиль, затем набор) сохраняется.
            if self.font_state.waiting.is_empty() {
                self.text_command(cmd);
            } else {
                self.font_state.waiting.push_back(cmd);
            }
        }
        self.push_font_requests();
````

В `crates/ferrum-wasm/src/core/fonts.rs` заменить:

````rust
    fn push_missing_list(&mut self) {
        self.font_state.recomputed.0 += 1;
        let list: Vec<MissingFontDto> = self
            .editor
````

на:

````rust
    fn push_missing_list(&mut self) {
        self.font_state.recomputed.0 += 1;
        // Семейства страницы — для группы «В документе» выбора шрифта.
        let mut families: Vec<String> = self
            .editor
            .page_fonts()
            .into_iter()
            .map(|f| f.family)
            .collect();
        families.dedup();
        if self.font_state.page_sent.as_ref() != Some(&families) {
            self.font_state.page_sent = Some(families.clone());
            self.events.push(CoreEvent::PageFonts { families });
        }
        let list: Vec<MissingFontDto> = self
            .editor
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            | Command::LoadFont { .. }
            | Command::ReplaceMissingFont { .. }
            | Command::TestCreateText { .. } => {
                debug_assert!(false, "команда ядра — в `Core::command`");
            }
````

на:

````rust
            | Command::LoadFont { .. }
            | Command::ReplaceMissingFont { .. }
            | Command::TestCreateText { .. }
            | Command::TextEditStart { .. }
            | Command::TextEditEnd
            | Command::TextInput { .. }
            | Command::TextKey { .. }
            | Command::SetTextStyle { .. }
            | Command::SetTextProps { .. } => {
                debug_assert!(false, "команда ядра — в `Core::command`");
            }
````

Создать `crates/ferrum-wasm/src/core/text.rs`:

````rust
//! Правка текста в ядре worker'а (подсистема 3, этап 2): команды ввода, клавиш,
//! стиля и свойств текста и события для поля ввода (`TextCaret`) и секции «Текст»
//! (`TextStyleSummary`). Всё из основного потока — недоверенное (спека 8): текст
//! ввода ≤ 64 КБ, клавиша ≤ 64 байт, имена шрифтов ≤ 256 байт, числа конечны и в
//! пределах, неизвестные ключи правки отвергает разбор (`deny_unknown_fields`).
//! Правка, которой нужны ещё не загруженные шрифты, ждёт в общей очереди правок
//! шрифтов (порядок команд текста сохраняется).

use ferrum_doc::{
    AlignH, AlignV, Decoration, FontName, LetterSpacing, LineHeight, ListKind, NodeId, TextCase,
    TextSizing,
};
use ferrum_editor::{
    EnterAt, Modifiers, Phase, TextError, TextInputKind, TextPropsPatch, TextStylePatch,
    TextStyleSummary,
};
use ferrum_text::MAX_FAMILY_NAME;

use super::Core;
use crate::protocol::{
    AlignHDto, AlignVDto, Command, CoreEvent, DecorationDto, EditPhase, ListKindDto, MixedSpacing,
    MixedString, Num, SpacingDto, SpacingUnit, TextCaseDto, TextInputKindDto, TextPropsPatchDto,
    TextSizingDto, TextStyleDto, TextStylePatchDto, MAX_FONT_SIZE, MAX_KEY_CODE_BYTES,
    MAX_TEXT_DATA_BYTES, SELECTION_IDS_MAX,
};

/// Что последним ушло полю ввода и панели.
#[derive(Default)]
pub(crate) struct TextState {
    pub caret_sent: Option<CoreEvent>,
    pub style_sent: Option<Option<TextStyleDto>>,
}

fn kind(k: TextInputKindDto) -> TextInputKind {
    match k {
        TextInputKindDto::Insert => TextInputKind::Insert,
        TextInputKindDto::Paragraph => TextInputKind::Paragraph,
        TextInputKindDto::Paste => TextInputKind::Paste,
        TextInputKindDto::Compose => TextInputKind::Compose,
        TextInputKindDto::Commit => TextInputKind::Commit,
        TextInputKindDto::DeleteBackward => TextInputKind::DeleteBackward,
        TextInputKindDto::DeleteForward => TextInputKind::DeleteForward,
        TextInputKindDto::DeleteWordBackward => TextInputKind::DeleteWordBackward,
        TextInputKindDto::DeleteWordForward => TextInputKind::DeleteWordForward,
        TextInputKindDto::DeleteLineBackward => TextInputKind::DeleteLineBackward,
        TextInputKindDto::DeleteLineForward => TextInputKind::DeleteLineForward,
    }
}

fn phase(p: EditPhase) -> Phase {
    match p {
        EditPhase::Preview => Phase::Preview,
        EditPhase::Commit => Phase::Commit,
        EditPhase::Cancel => Phase::Cancel,
    }
}

fn finite_in(v: f64, lo: f64, hi: f64, what: &str) -> Result<f64, String> {
    if v.is_finite() {
        Ok(v.clamp(lo, hi))
    } else {
        Err(format!("{what} — не число"))
    }
}

fn line_height(d: SpacingDto) -> Result<LineHeight, String> {
    Ok(match d.unit {
        SpacingUnit::Auto => LineHeight::Auto,
        SpacingUnit::Px => LineHeight::Px(finite_in(d.value, 0.0, MAX_FONT_SIZE, "Интерлиньяж")?),
        SpacingUnit::Percent => {
            LineHeight::Percent(finite_in(d.value, 0.0, 10_000.0, "Интерлиньяж")?)
        }
    })
}

fn letter_spacing(d: SpacingDto) -> Result<LetterSpacing, String> {
    Ok(match d.unit {
        SpacingUnit::Auto => return Err("Трекинг не бывает «авто»".into()),
        SpacingUnit::Px => LetterSpacing::Px(finite_in(
            d.value,
            -MAX_FONT_SIZE,
            MAX_FONT_SIZE,
            "Трекинг",
        )?),
        SpacingUnit::Percent => {
            LetterSpacing::Percent(finite_in(d.value, -1000.0, 1000.0, "Трекинг")?)
        }
    })
}

/// Правка стиля из команды: имена и числа проверены.
fn style_patch(d: &TextStylePatchDto) -> Result<TextStylePatch, String> {
    let name = |s: &str| !s.is_empty() && s.len() <= MAX_FAMILY_NAME;
    let font = match (&d.family, &d.style) {
        (None, None) => None,
        (Some(f), s) => {
            let s = s.as_deref().unwrap_or("Regular");
            if !name(f) || !name(s) {
                return Err("Неверное имя шрифта".into());
            }
            Some(FontName::new(f.clone(), s))
        }
        (None, Some(_)) => return Err("Начертание без семейства".into()),
    };
    Ok(TextStylePatch {
        font,
        size: d
            .size
            .map(|v| finite_in(v, 1.0, MAX_FONT_SIZE, "Размер шрифта"))
            .transpose()?,
        line_height: d.line_height.map(line_height).transpose()?,
        letter_spacing: d.letter_spacing.map(letter_spacing).transpose()?,
        decoration: d.decoration.map(|v| match v {
            DecorationDto::None => Decoration::None,
            DecorationDto::Underline => Decoration::Underline,
            DecorationDto::Strike => Decoration::Strike,
        }),
        case: d.case.map(|v| match v {
            TextCaseDto::None => TextCase::None,
            TextCaseDto::Upper => TextCase::Upper,
            TextCaseDto::Lower => TextCase::Lower,
            TextCaseDto::Title => TextCase::Title,
            TextCaseDto::SmallCaps => TextCase::SmallCaps,
        }),
        list: d.list.map(|v| match v {
            ListKindDto::None => ListKind::None,
            ListKindDto::Bullet => ListKind::Bullet,
            ListKindDto::Ordered => ListKind::Ordered,
        }),
        indent_by: None,
    })
}

fn props_patch(d: &TextPropsPatchDto) -> Result<TextPropsPatch, String> {
    Ok(TextPropsPatch {
        sizing: d.sizing.map(|v| match v {
            TextSizingDto::AutoWidth => TextSizing::AutoWidth,
            TextSizingDto::AutoHeight => TextSizing::AutoHeight,
            TextSizingDto::Fixed => TextSizing::Fixed,
        }),
        align_h: d.align_h.map(|v| match v {
            AlignHDto::Left => AlignH::Left,
            AlignHDto::Center => AlignH::Center,
            AlignHDto::Right => AlignH::Right,
            AlignHDto::Justify => AlignH::Justify,
        }),
        align_v: d.align_v.map(|v| match v {
            AlignVDto::Top => AlignV::Top,
            AlignVDto::Middle => AlignV::Middle,
            AlignVDto::Bottom => AlignV::Bottom,
        }),
        paragraph_spacing: d
            .paragraph_spacing
            .map(|v| finite_in(v, 0.0, MAX_FONT_SIZE, "Интервал абзацев"))
            .transpose()?,
        paragraph_indent: d
            .paragraph_indent
            .map(|v| finite_in(v, -MAX_FONT_SIZE, MAX_FONT_SIZE, "Отступ абзаца"))
            .transpose()?,
        truncate: None,
        max_lines: None,
    })
}

fn spacing_dto_lh(v: LineHeight) -> SpacingDto {
    match v {
        LineHeight::Auto => SpacingDto {
            unit: SpacingUnit::Auto,
            value: 0.0,
        },
        LineHeight::Px(value) => SpacingDto {
            unit: SpacingUnit::Px,
            value,
        },
        LineHeight::Percent(value) => SpacingDto {
            unit: SpacingUnit::Percent,
            value,
        },
    }
}

fn spacing_dto_ls(v: LetterSpacing) -> SpacingDto {
    match v {
        LetterSpacing::Px(value) => SpacingDto {
            unit: SpacingUnit::Px,
            value,
        },
        LetterSpacing::Percent(value) => SpacingDto {
            unit: SpacingUnit::Percent,
            value,
        },
    }
}

fn mixed(value: &str, mixed: bool) -> MixedString {
    MixedString {
        value: value.to_string(),
        mixed,
    }
}

/// Сводка редактора → событие (перечисления — строками протокола).
pub(crate) fn style_dto(s: &TextStyleSummary) -> TextStyleDto {
    let deco = match s.decoration.value {
        Decoration::None => "none",
        Decoration::Underline => "underline",
        Decoration::Strike => "strike",
    };
    let case = match s.case.value {
        TextCase::None => "none",
        TextCase::Upper => "upper",
        TextCase::Lower => "lower",
        TextCase::Title => "title",
        TextCase::SmallCaps => "smallCaps",
    };
    let list = match s.list.value {
        ListKind::None => "none",
        ListKind::Bullet => "bullet",
        ListKind::Ordered => "ordered",
    };
    let sizing = match s.sizing.value {
        TextSizing::AutoWidth => "autoWidth",
        TextSizing::AutoHeight => "autoHeight",
        TextSizing::Fixed => "fixed",
    };
    let ah = match s.align_h.value {
        AlignH::Left => "left",
        AlignH::Center => "center",
        AlignH::Right => "right",
        AlignH::Justify => "justify",
    };
    let av = match s.align_v.value {
        AlignV::Top => "top",
        AlignV::Middle => "middle",
        AlignV::Bottom => "bottom",
    };
    TextStyleDto {
        family: mixed(&s.family.value, s.family.mixed),
        style: mixed(&s.style.value, s.style.mixed),
        size: Num {
            value: s.size.value,
            mixed: s.size.mixed,
        },
        line_height: MixedSpacing {
            value: spacing_dto_lh(s.line_height.value),
            mixed: s.line_height.mixed,
        },
        letter_spacing: MixedSpacing {
            value: spacing_dto_ls(s.letter_spacing.value),
            mixed: s.letter_spacing.mixed,
        },
        decoration: mixed(deco, s.decoration.mixed),
        case: mixed(case, s.case.mixed),
        list: mixed(list, s.list.mixed),
        sizing: mixed(sizing, s.sizing.mixed),
        align_h: mixed(ah, s.align_h.mixed),
        align_v: mixed(av, s.align_v.mixed),
        paragraph_spacing: Num {
            value: s.paragraph_spacing.value,
            mixed: s.paragraph_spacing.mixed,
        },
        paragraph_indent: Num {
            value: s.paragraph_indent.value,
            mixed: s.paragraph_indent.mixed,
        },
        range: s.range,
    }
}

/// Команда правки текста?
pub(crate) fn is_text_command(c: &Command) -> bool {
    matches!(
        c,
        Command::TextEditStart { .. }
            | Command::TextEditEnd
            | Command::TextInput { .. }
            | Command::TextKey { .. }
            | Command::SetTextStyle { .. }
            | Command::SetTextProps { .. }
    )
}

impl Core {
    /// Время ядра (мс): мигание курсора и склейка ввода в один шаг. Worker
    /// передаёт `performance.now()` в начале кадра и перед командой.
    pub fn set_time(&mut self, now_ms: f64) {
        self.editor.set_time(now_ms);
        if self.editor.is_editing_text() {
            self.drain_editor();
        }
    }

    fn ids(&mut self, list: &Option<Vec<String>>) -> Option<Option<Vec<NodeId>>> {
        match list {
            None => Some(None),
            Some(l) => {
                if l.len() > SELECTION_IDS_MAX {
                    self.error(format!("Не больше {SELECTION_IDS_MAX} слоёв в команде"));
                    return None;
                }
                let mut out = Vec::with_capacity(l.len());
                for s in l {
                    match s.parse::<NodeId>() {
                        Ok(id) if s.len() <= 64 => out.push(id),
                        _ => {
                            self.error("Неверный идентификатор слоя");
                            return None;
                        }
                    }
                }
                Some(Some(out))
            }
        }
    }

    /// Команда правки текста. `Err(Pending)` — ждать шрифтов (очередь —
    /// `text_command` в модуле шрифтов); неверное — `Error`, `Ok`.
    pub(crate) fn run_text(&mut self, cmd: &Command) -> Result<(), TextError> {
        match cmd {
            Command::TextEditStart { id } => {
                let Some(Some(ids)) = self.ids(&Some(vec![id.clone()])) else {
                    return Ok(());
                };
                if !self.editor.start_text_edit(ids[0], EnterAt::All) {
                    self.error("Это не текст или он заблокирован");
                }
                Ok(())
            }
            Command::TextEditEnd => {
                self.editor.end_text_edit();
                Ok(())
            }
            Command::TextInput { kind: k, data } => {
                let data = data.as_deref().unwrap_or("");
                if data.len() > MAX_TEXT_DATA_BYTES {
                    self.error("Текст ввода длиннее 64 КБ");
                    return Ok(());
                }
                if *k == TextInputKindDto::Paste && !self.editor.is_editing_text() {
                    return self.editor.paste_plain(data);
                }
                self.editor.text_input(kind(*k), data)
            }
            Command::TextKey { key, mods } => {
                if key.len() > MAX_KEY_CODE_BYTES {
                    self.error("Неверная клавиша");
                    return Ok(());
                }
                let m = Modifiers {
                    shift: mods.shift,
                    alt: mods.alt,
                    ctrl: mods.ctrl,
                    meta: mods.meta,
                };
                self.editor.text_key(key, m);
                Ok(())
            }
            Command::SetTextStyle {
                patch,
                phase: p,
                ids,
            } => {
                let Some(ids) = self.ids(ids) else {
                    return Ok(());
                };
                match style_patch(patch) {
                    Ok(patch) => self
                        .editor
                        .edit_text_style(&patch, phase(*p), ids.as_deref()),
                    Err(m) => {
                        self.editor.edit_text_style(
                            &TextStylePatch::default(),
                            Phase::Cancel,
                            None,
                        )?;
                        self.error(format!("Неверная правка: {m}"));
                        Ok(())
                    }
                }
            }
            Command::SetTextProps {
                patch,
                phase: p,
                ids,
            } => {
                let Some(ids) = self.ids(ids) else {
                    return Ok(());
                };
                match props_patch(patch) {
                    Ok(patch) => self
                        .editor
                        .edit_text_props(&patch, phase(*p), ids.as_deref()),
                    Err(m) => {
                        self.error(format!("Неверная правка: {m}"));
                        Ok(())
                    }
                }
            }
            _ => Ok(()),
        }
    }

    /// `TextCaret` и `TextStyleSummary`, если сменились (после пачки событий).
    pub(crate) fn push_text_state(&mut self, properties: bool) {
        let caret = match self.editor.text_edit_info() {
            Some(i) => CoreEvent::TextCaret {
                editing: true,
                rect: Some([i.caret.x0, i.caret.y0, i.caret.width(), i.caret.height()]),
                selection: Some([i.anchor as u32, i.focus as u32]),
                draft: i.draft,
            },
            None => CoreEvent::TextCaret {
                editing: false,
                rect: None,
                selection: None,
                draft: false,
            },
        };
        let caret_changed = self.text_state.caret_sent.as_ref() != Some(&caret);
        if caret_changed {
            self.text_state.caret_sent = Some(caret.clone());
            self.events
                .retain(|e| !matches!(e, CoreEvent::TextCaret { .. }));
            self.events.push(caret);
        }
        if properties || caret_changed {
            let summary = self.editor.text_style_summary().map(|s| style_dto(&s));
            if self.text_state.style_sent.as_ref() != Some(&summary) {
                self.text_state.style_sent = Some(summary.clone());
                self.events
                    .retain(|e| !matches!(e, CoreEvent::TextStyleSummary { .. }));
                self.events.push(CoreEvent::TextStyleSummary { summary });
            }
        }
    }
}
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    Resize,
    Rotate,
}

````

на:

````rust
    Resize,
    Rotate,
    /// Текст в правке и инструмент «Текст».
    Text,
}

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    AutoHeight,
    Fixed,
}

````

на:

````rust
    AutoHeight,
    Fixed,
}

/// Наибольшая клавиша `TextKey` (`KeyboardEvent.code`) в байтах.
pub const MAX_KEY_CODE_BYTES: usize = 64;
/// Пределы чисел стиля текста из панели (недоверенный ввод).
pub const MAX_FONT_SIZE: f64 = 10_000.0;

/// Вид ввода в текст (`InputEvent.inputType` и составление IME).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum TextInputKindDto {
    Insert,
    Paragraph,
    Paste,
    Compose,
    Commit,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteLineBackward,
    DeleteLineForward,
}

/// Модификаторы клавиши `TextKey`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyMods {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub meta: bool,
}

/// Единица интерлиньяжа и трекинга.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum SpacingUnit {
    /// Только интерлиньяж: по метрикам шрифта (значение не читается).
    Auto,
    Px,
    Percent,
}

/// Интерлиньяж или трекинг: единица и значение.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpacingDto {
    pub unit: SpacingUnit,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum DecorationDto {
    None,
    Underline,
    Strike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum TextCaseDto {
    None,
    Upper,
    Lower,
    Title,
    SmallCaps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum ListKindDto {
    None,
    Bullet,
    Ordered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum AlignHDto {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum AlignVDto {
    Top,
    Middle,
    Bottom,
}

/// Правка стиля символов из секции «Текст»: только известные ключи. Семейство
/// без начертания — начертание то же (если есть в семействе), иначе Regular.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextStylePatchDto {
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(default)]
    pub line_height: Option<SpacingDto>,
    #[serde(default)]
    pub letter_spacing: Option<SpacingDto>,
    #[serde(default)]
    pub decoration: Option<DecorationDto>,
    #[serde(default)]
    pub case: Option<TextCaseDto>,
    #[serde(default)]
    pub list: Option<ListKindDto>,
}

/// Правка свойств текстовых узлов из секции «Текст».
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextPropsPatchDto {
    #[serde(default)]
    pub sizing: Option<TextSizingDto>,
    #[serde(default)]
    pub align_h: Option<AlignHDto>,
    #[serde(default)]
    pub align_v: Option<AlignVDto>,
    #[serde(default)]
    pub paragraph_spacing: Option<f64>,
    #[serde(default)]
    pub paragraph_indent: Option<f64>,
}

/// Значение для панели и признак «Смешанное».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MixedString {
    pub value: String,
    pub mixed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MixedSpacing {
    pub value: SpacingDto,
    pub mixed: bool,
}

/// Сводка стиля для секции «Текст».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct TextStyleDto {
    pub family: MixedString,
    pub style: MixedString,
    pub size: Num,
    pub line_height: MixedSpacing,
    pub letter_spacing: MixedSpacing,
    /// Перечисления — строкой: `decoration` (`none`…), `case`, `list`,
    /// `sizing`, `alignH`, `alignV`.
    pub decoration: MixedString,
    pub case: MixedString,
    pub list: MixedString,
    pub sizing: MixedString,
    pub align_h: MixedString,
    pub align_v: MixedString,
    pub paragraph_spacing: Num,
    pub paragraph_indent: Num,
    /// Правится выделенный диапазон (идёт правка текста).
    pub range: bool,
}

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        height: Option<f64>,
    },
}

````

на:

````rust
        height: Option<f64>,
    },
    // ——— Подсистема 3, этап 2: правка текста ———
    /// Войти в правку текста (весь текст выделен).
    TextEditStart {
        id: String,
    },
    /// Выйти из правки (как Esc).
    TextEditEnd,
    /// Ввод из поля: набор, абзац, удаление, вставка обычного текста (вне правки
    /// — новый текст), составление IME. `data` — не больше 64 КБ.
    TextInput {
        kind: TextInputKindDto,
        #[serde(default)]
        data: Option<String>,
    },
    /// Клавиша в правке: ход курсора, удаление, отмена, форматирование
    /// (`key` — `KeyboardEvent.code`).
    TextKey {
        key: String,
        #[serde(default)]
        mods: KeyMods,
    },
    /// Стиль символов: в правке — выделенного диапазона (у курсора — следующего
    /// ввода), иначе — всего текста узлов `ids` (без них — выделения).
    SetTextStyle {
        patch: TextStylePatchDto,
        phase: EditPhase,
        #[serde(default)]
        ids: Option<Vec<String>>,
    },
    /// Свойства текстовых узлов (режим размера, выравнивание, интервалы абзацев).
    SetTextProps {
        patch: TextPropsPatchDto,
        phase: EditPhase,
        #[serde(default)]
        ids: Option<Vec<String>>,
    },
}

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        message: String,
    },
}
````

на:

````rust
        message: String,
    },
    // ——— Подсистема 3, этап 2: правка текста ———
    /// Правка текста: идёт ли, прямоугольник курсора в CSS-пикселях холста
    /// `[x, y, w, h]` (поле ввода и окно IME встают под него) и выделение
    /// `[якорь, курсор]` в символах. Приходит, когда что-то из этого сменилось.
    TextCaret {
        editing: bool,
        rect: Option<[f64; 4]>,
        selection: Option<[u32; 2]>,
        /// Новый текст (инструмент T) ещё не в документе.
        draft: bool,
    },
    /// Стиль для секции «Текст»; `None` — текстов не выделено.
    TextStyleSummary {
        summary: Option<TextStyleDto>,
    },
    /// Семейства текстов текущей страницы (группа «В документе» выбора шрифта).
    PageFonts {
        families: Vec<String>,
    },
}
````

В `crates/ferrum-wasm/src/web/mod.rs` заменить:

````rust
    /// Команда основного потока. Объект не той формы — событие `error`, не паника.
    pub fn command(&mut self, #[wasm_bindgen(unchecked_param_type = "Command")] cmd: JsValue) {
        match parse_command(cmd) {
            Ok(c) => self.core.command(c),
````

на:

````rust
    /// Команда основного потока. Объект не той формы — событие `error`, не паника.
    pub fn command(&mut self, #[wasm_bindgen(unchecked_param_type = "Command")] cmd: JsValue) {
        // Время — для склейки ввода в один шаг и мигания курсора.
        let now = self.now();
        self.core.set_time(now);
        match parse_command(cmd) {
            Ok(c) => self.core.command(c),
````

В `crates/ferrum-wasm/src/web/mod.rs` заменить:

````rust
    /// Начало кадра после ввода: рисует кадр, если он нужен. `true` — нарисован.
    pub fn frame(&mut self) -> bool {
        if let Some(r) = self.surface.poll_readback() {
            self.awaiting = false;
````

на:

````rust
    /// Начало кадра после ввода: рисует кадр, если он нужен. `true` — нарисован.
    pub fn frame(&mut self) -> bool {
        // Мигание курсора правки текста: кадр нужен, когда курсор гаснет и горит.
        let now = self.now();
        self.core.set_time(now);
        if let Some(r) = self.surface.poll_readback() {
            self.awaiting = false;
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npm run wasm && npm run typecheck`

Expected: PASS: `text_edit` ядра — 7 тестов; wasm собран, `npm run typecheck` без ошибок (типы tsify с новыми командами и событиями)

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/src/core/core.ts crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/fonts.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/core/text.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/src/web/mod.rs crates/ferrum-wasm/tests/text_edit.rs
git commit -m "feat(wasm): протокол правки текста — TextEditStart, TextEditEnd, TextInput, TextKey, SetTextStyle, SetTextProps, события TextCaret, TextStyleSummary, PageFonts; пределы ввода, очередь ждущих шрифтов, мигание по времени ядра

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 10: Поле правки в интерфейсе, инструмент T, буфер обмена

Спека 7 и ядро §4.2: скрытое `<textarea>` в сцене холста (`text-input.ts`) получает фокус, пока идёт правка (событие `textCaret`), и стоит под курсором; текста документа в нём нет. `keydown` — решение `textKeyAction` (ход курсора, удаление, отмена, форматирование — `textKey`; набор и буфер обмена — браузеру; прочие сочетания раскладки — гаснут), `beforeinput` — `textInput` по `inputType`, составление IME — `compose` и `commit`. Нажатие на холст в правке оставляет фокус у поля. Буфер обмена: копия и вырезание в поле — команды ядра, запись — обычный текст и наш формат; вставка — наш формат или обычный текст (вне правки — новый текст). Порядок: перед командой текста — ждущий ввод (`Transport.flush`, вычитка кольца в worker'е). Кнопка «Текст» в тулбаре.

**Files:**
- Create: `apps/web/src/core/text-keys.ts`, `apps/web/src/ui/text-input.ts`
- Modify: `apps/web/src/core/{core,messages,transport}.ts`, `src/worker/core.worker.ts`, `src/ui/{system-clipboard,store,boot,toolbar,app}.ts(x)`, `src/window.d.ts`, `src/ui/styles/app.css`
- Test: `apps/web/unit/text-keys.test.ts`, `apps/web/e2e/text-edit.spec.ts` (новые)

**Interfaces:**
- Consumes: протокол Task 9.
- Produces: `textKeyAction(e, platform) -> 'core' | 'browser' | 'block'`, `inputKind(inputType)`, `MAX_TEXT_INPUT_BYTES`, `utf8Length`; `isTextCommand(cmd)`; `Transport.flush()`; `Core.keyTarget`; `installTextInput(ed, stage, canvas)`, `isTextField`; состояние `text { editing, draft }`, `textStyle`, `pageFonts`; журнал тестового режима: у `clipboard` — `text` (обычный текст фрагмента).

- [ ] **Step 1: Написать тесты**

Создать `apps/web/e2e/text-edit.spec.ts`:

````ts
// Правка текста на холсте (подсистема 3, этап 2): инструмент T, набор в скрытое
// поле, ход курсора и удаление, IME синтетическими событиями составления, отмена
// ввода подряд одним шагом, клавиши в правке не уходят командам раскладки, буфер
// обмена (фрагмент со стилями, обычный текст вне правки). Текст документа тест
// читает копией фрагмента (журнал событий тестового режима хранит её обычный текст).

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

const ui = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!)));
const clipTexts = (page: Page) =>
  page.evaluate(() => window.__ferrum!.uiEvents!.filter((e) => e.type === 'clipboard' && e.text !== undefined).map((e) => e.text!));

/** Новый текст инструментом T в точке мира; ждёт начала правки. */
async function newText(page: Page, x: number, y: number): Promise<void> {
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyT');
  await expect.poll(async () => (await ui(page)).tool).toBe('text');
  const at = await appScreen(page, await live(page), x, y);
  await page.mouse.click(at.x, at.y);
  await expect.poll(async () => (await ui(page)).text.editing).toBe(true);
  await expect(page.locator('.fe-text-input')).toBeFocused();
}

/** Текст правимого узла: выделить всё и скопировать фрагмент. */
async function copyAll(page: Page): Promise<string> {
  const before = (await clipTexts(page)).length;
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.press('ControlOrMeta+c');
  await expect.poll(async () => (await clipTexts(page)).length).toBe(before + 1);
  return (await clipTexts(page))[before];
}

/** Тёмных пикселей в квадрате экрана (текст чёрный на белом фоне страницы). */
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

test('инструмент T: щелчок, набор, Esc — текст на холсте одним шагом @firefox', async ({ page }) => {
  await openApp(page, 0);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setPageBackground', color: '#ffffff', phase: 'commit' }));
  const l = await live(page);
  await newText(page, 100, 300);
  // Черновик: в документе (и в слоях) узла ещё нет, шага тоже.
  expect((await ui(page)).text.draft).toBe(true);
  expect((await ui(page)).layers.total).toBe(0);
  expect((await live(page)).steps).toBe(l.steps);
  await page.keyboard.type('Привет, мир');
  await liveWhen(page, `l.nodes === ${l.nodes + 1} && l.steps === ${l.steps + 1}`);
  expect(await copyAll(page)).toBe('Привет, мир');
  // Поле ввода не держит текста документа.
  expect(await page.locator('.fe-text-input').inputValue()).toBe('');
  await page.keyboard.press('Escape');
  await expect.poll(async () => (await ui(page)).text.editing).toBe(false);
  expect((await ui(page)).selectionCount).toBe(1);
  expect((await live(page)).steps).toBe(l.steps + 1);
  await expect(page.getByRole('treeitem', { name: 'Текст', exact: true })).toBeVisible();
  const at = await appScreen(page, await live(page), 100, 300);
  await expect.poll(() => darkPixels(page, at, 60), { timeout: 10_000 }).toBeGreaterThan(20);
});

test('стрелки, Backspace и выделение Shift — в позиции курсора @firefox', async ({ page }) => {
  await openApp(page, 0);
  await newText(page, 100, 300);
  await page.keyboard.type('abc');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.type('X');
  await page.keyboard.press('End');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Home');
  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.type('Y');
  expect(await copyAll(page)).toBe('YXb');
});

test('IME: составление — превью без шага, конец составления — текст', async ({ page }) => {
  await openApp(page, 0);
  const steps = (await live(page)).steps + 1;
  await newText(page, 100, 300);
  await page.keyboard.type('a');
  await liveWhen(page, `l.steps === ${steps}`);
  const compose = (type: string, data: string) =>
    page.evaluate(
      ({ type, data }) => document.querySelector('.fe-text-input')!.dispatchEvent(new CompositionEvent(type, { data, bubbles: true })),
      { type, data },
    );
  await compose('compositionstart', '');
  await compose('compositionupdate', 'к');
  await compose('compositionupdate', 'ко');
  await expect.poll(async () => (await ui(page)).text.editing).toBe(true);
  // Превью — не правка: шагов не прибавилось.
  await page.waitForTimeout(100);
  expect((await live(page)).steps).toBe(steps);
  await compose('compositionend', 'кот');
  await liveWhen(page, `l.steps >= ${steps}`);
  expect(await copyAll(page)).toBe('aкот');
});

test('ввод подряд — один шаг: Ctrl/Cmd+Z в правке убирает новый текст целиком', async ({ page }) => {
  await openApp(page, 0);
  const l = await live(page);
  await newText(page, 100, 300);
  await page.keyboard.type('abc');
  await liveWhen(page, `l.nodes === ${l.nodes + 1}`);
  await page.keyboard.press('ControlOrMeta+z');
  await liveWhen(page, `l.nodes === ${l.nodes} && l.canUndo === 0`);
  await expect.poll(async () => (await ui(page)).text.editing).toBe(false);
});

test('клавиши в правке не уходят командам раскладки', async ({ page }) => {
  await openApp(page, 0);
  const l = await live(page);
  await newText(page, 100, 300);
  // R — буква, а не «Прямоугольник»; Delete — не удаление узла.
  await page.keyboard.type('rr');
  await page.keyboard.press('Delete');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('Delete');
  expect((await ui(page)).tool).toBe('move');
  expect(await copyAll(page)).toBe('r');
  await page.keyboard.press('Escape');
  await expect.poll(async () => (await ui(page)).text.editing).toBe(false);
  // Вне правки Delete удаляет выделенный текст.
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes}`);
});

test('двойной щелчок по тексту — правка; щелчок мимо — выход', async ({ page }) => {
  await openApp(page, 0);
  await newText(page, 100, 300);
  await page.keyboard.type('Привет');
  await page.keyboard.press('Escape');
  await expect.poll(async () => (await ui(page)).text.editing).toBe(false);
  const empty = await appScreen(page, await live(page), 400, 600);
  await page.mouse.click(empty.x, empty.y);
  await expect.poll(async () => (await ui(page)).selectionCount).toBe(0);
  const at = await appScreen(page, await live(page), 105, 305);
  await page.mouse.dblclick(at.x, at.y);
  await expect.poll(async () => (await ui(page)).text.editing).toBe(true);
  await expect(page.locator('.fe-text-input')).toBeFocused();
  // Щелчок по тексту в правке оставляет фокус у поля ввода.
  await page.mouse.click(at.x, at.y);
  await expect(page.locator('.fe-text-input')).toBeFocused();
  await page.keyboard.type('!');
  await page.mouse.click(empty.x, empty.y);
  await expect.poll(async () => (await ui(page)).text.editing).toBe(false);
});

test.describe('буфер обмена', () => {
  test.use({ permissions: ['clipboard-read', 'clipboard-write'] });

  test('фрагмент со стилями вставляется в другой текст', async ({ page }) => {
    await openApp(page, 0);
    await newText(page, 100, 300);
    await page.keyboard.type('Жирный');
    await page.keyboard.press('ControlOrMeta+a');
    await page.keyboard.press('ControlOrMeta+b');
    await expect.poll(async () => (await ui(page)).textStyle?.style.value).toBe('Bold');
    await page.keyboard.press('ControlOrMeta+c');
    // В буфере — и обычный текст, и наш формат со стилями.
    await expect.poll(async () => page.evaluate(async () => (await navigator.clipboard.read())[0]?.types ?? [])).toContain('text/plain');
    expect(await page.evaluate(async () => (await navigator.clipboard.read())[0].types)).toContain('web application/x-ferrum');
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Жирный');
    await page.keyboard.press('Escape');
    await newText(page, 100, 500);
    await page.keyboard.press('ControlOrMeta+v');
    // Свой формат читается асинхронно: новый текст появится, когда вставка дойдёт.
    await expect.poll(async () => (await ui(page)).text.draft).toBe(false);
    expect(await copyAll(page)).toBe('Жирный');
    expect((await ui(page)).textStyle?.style.value).toBe('Bold');
  });

  test('обычный текст вне правки — новый текстовый узел', async ({ page }) => {
    await openApp(page, 0);
    const l = await live(page);
    await page.locator('canvas').focus();
    await page.evaluate(() => {
      const dt = new DataTransfer();
      dt.setData('text/plain', 'Из буфера');
      document.querySelector('canvas')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
    });
    await liveWhen(page, `l.nodes === ${l.nodes + 1} && l.selectionCount === 1`);
    expect((await ui(page)).summary.kind).toBe('text');
  });
});
````

Создать `apps/web/unit/text-keys.test.ts`:

````ts
// Клавиши поля правки текста: ядру, браузеру или погасить; виды `beforeinput`;
// длина в UTF-8 для предела команды.

import { expect, test } from '@playwright/test';
import { inputKind, textKeyAction, utf8Length } from '../src/core/text-keys';

const key = (code: string, mods: Partial<Record<'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'isComposing', boolean>> = {}, k = '') => ({
  code,
  key: k,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  isComposing: false,
  ...mods,
});

test('ход курсора, удаление и форматирование — ядру', () => {
  expect(textKeyAction(key('ArrowLeft', { shiftKey: true }), 'windows')).toBe('core');
  expect(textKeyAction(key('Backspace', { altKey: true }), 'mac')).toBe('core');
  expect(textKeyAction(key('KeyB', { metaKey: true }), 'mac')).toBe('core');
  expect(textKeyAction(key('KeyX', { ctrlKey: true, shiftKey: true }), 'windows')).toBe('core');
  expect(textKeyAction(key('KeyT', { metaKey: true, altKey: true }), 'mac')).toBe('core');
  expect(textKeyAction(key('KeyY', { ctrlKey: true }), 'linux')).toBe('core');
});

test('набор, буфер обмена и составление — браузеру; команды раскладки гаснут', () => {
  expect(textKeyAction(key('KeyR', {}, 'r'), 'windows')).toBe('browser');
  expect(textKeyAction(key('KeyC', { ctrlKey: true }), 'windows')).toBe('browser');
  expect(textKeyAction(key('ArrowLeft', { isComposing: true }), 'mac')).toBe('browser');
  expect(textKeyAction(key('KeyD', { metaKey: true }), 'mac')).toBe('block');
  expect(textKeyAction(key('KeyG', { ctrlKey: true }), 'windows')).toBe('block');
  // Перезагрузка страницы остаётся браузеру.
  expect(textKeyAction(key('KeyR', { ctrlKey: true }), 'windows')).toBe('browser');
});

test('виды beforeinput', () => {
  expect(inputKind('insertText')).toBe('insert');
  expect(inputKind('insertLineBreak')).toBe('paragraph');
  expect(inputKind('deleteWordBackward')).toBe('deleteWordBackward');
  expect(inputKind('deleteSoftLineBackward')).toBe('deleteLineBackward');
  expect(inputKind('insertCompositionText')).toBeNull();
  expect(inputKind('insertFromPaste')).toBeNull();
});

test('длина в UTF-8', () => {
  expect(utf8Length('abc')).toBe(3);
  expect(utf8Length('я')).toBe(2);
  expect(utf8Length('€')).toBe(3);
  expect(utf8Length('😀')).toBe(4);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npx playwright test -c playwright.unit.config.ts unit/text-keys.test.ts`
Expected: FAIL — модуль `src/core/text-keys.ts` не найден

- [ ] **Step 3: Реализация**

В `apps/web/src/core/core.ts` заменить:

````ts
import * as enc from './input';
import { BACKENDS, Kind, LIVE_NAMES } from './layout.gen';
import { prepareCommand, type FromWorker } from './messages';
import { createTransport, type Transport, type TransportMode } from './transport';

````

на:

````ts
import * as enc from './input';
import { BACKENDS, Kind, LIVE_NAMES } from './layout.gen';
import { isTextCommand, prepareCommand, type FromWorker } from './messages';
import { createTransport, type Transport, type TransportMode } from './transport';

````

В `apps/web/src/core/core.ts` заменить:

````ts
  /** Ожидающие ответа `encodeClipboard` — по порядку запросов. */
  private readonly encodes: ((text: string | null) => void)[] = [];

  private constructor(
````

на:

````ts
  /** Ожидающие ответа `encodeClipboard` — по порядку запросов. */
  private readonly encodes: ((text: string | null) => void)[] = [];
  /** Кто получает клавиши после нажатия на холсте: поле правки текста, пока идёт
   * правка (иначе — сам холст). */
  keyTarget: HTMLElement | null = null;

  private constructor(
````

В `apps/web/src/core/core.ts` заменить:

````ts
      return;
    }
    const error = this.transport.post({ t: 'cmd', cmd: prepared.cmd }, prepared.transfer);
    if (error) this.emit({ type: 'error', message: error });
````

на:

````ts
      return;
    }
    // Щелчок в тексте и набор следом: ввод из кольца — раньше команды.
    if (isTextCommand(cmd)) this.transport.flush();
    const error = this.transport.post({ t: 'cmd', cmd: prepared.cmd }, prepared.transfer);
    if (error) this.emit({ type: 'error', message: error });
````

В `apps/web/src/core/core.ts` заменить:

````ts
    this.add(c, 'pointerdown', (e) => {
      c.setPointerCapture(e.pointerId);
      c.focus();
      // Кнопки «назад» и «вперёд» (3, 4) холсту не нужны.
      if (e.button > 2) return;
      const [x, y] = xy(e);
      enc.pointer(sink, Kind.POINTER_DOWN, e, x, y);
    });
    this.add(c, 'pointermove', (e) => {
````

на:

````ts
    this.add(c, 'pointerdown', (e) => {
      c.setPointerCapture(e.pointerId);
      // В правке текста клавиши остаются у поля ввода (щелчок по тексту — курсор).
      (this.keyTarget ?? c).focus({ preventScroll: true });
      // Кнопки «назад» и «вперёд» (3, 4) холсту не нужны.
      if (e.button > 2) return;
      const [x, y] = xy(e);
      enc.pointer(sink, Kind.POINTER_DOWN, e, x, y);
    });
    // В правке текста нажатие на холсте не уводит фокус у поля ввода (действие
    // `mousedown` по умолчанию отдало бы его холсту).
    this.add(c, 'mousedown', (e) => {
      if (this.keyTarget) e.preventDefault();
    });
    this.add(c, 'pointermove', (e) => {
````

В `apps/web/src/core/messages.ts` заменить:

````ts
import { MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from './clipboard-text';
import { MAX_DATA_BYTES, MAX_FONT_BYTES, MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES, mb, tooLarge } from './limits';

export type ToWorker =
````

на:

````ts
import { MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from './clipboard-text';
import { MAX_DATA_BYTES, MAX_FONT_BYTES, MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES, mb, tooLarge } from './limits';
import { MAX_TEXT_INPUT_BYTES, utf8Length } from './text-keys';

export type ToWorker =
````

В `apps/web/src/core/messages.ts` заменить:

````ts
export function commandSizeError(cmd: Command): string | null {
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type === 'openWithJournal') return journalCommandError(cmd);
  if (cmd.type === 'loadFont' && (cmd as { bytes?: unknown }).bytes === undefined) return null;
````

на:

````ts
export function commandSizeError(cmd: Command): string | null {
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type === 'textInput') {
    const data = (cmd as { data?: unknown }).data;
    // Каждая буква — 1–4 байта: длина в символах × 4 укладывается — считать незачем.
    if (typeof data === 'string' && data.length * 4 > MAX_TEXT_INPUT_BYTES && utf8Length(data) > MAX_TEXT_INPUT_BYTES) {
      return 'Слишком длинный текст: за раз — не больше 64 КБ';
    }
    return null;
  }
  if (cmd.type === 'openWithJournal') return journalCommandError(cmd);
  if (cmd.type === 'loadFont' && (cmd as { bytes?: unknown }).bytes === undefined) return null;
````

В `apps/web/src/core/messages.ts` заменить:

````ts
}

/** Виды сообщений worker'у: остальное — неверный конверт. */
export const TO_WORKER_KINDS: ReadonlySet<string> = new Set(['init', 'cmd', 'pasteText', 'encodeClipboard', 'tick', 'testPanic']);
````

на:

````ts
}

/** Команды правки текста: идут после ввода, положенного в кольцо до них
 * (щелчок в тексте, затем набор), — основной поток досылает ввод, worker вычитывает
 * кольцо перед такой командой. */
const TEXT_COMMANDS: ReadonlySet<string> = new Set(['textEditStart', 'textEditEnd', 'textInput', 'textKey', 'setTextStyle', 'setTextProps']);

export function isTextCommand(cmd: Command): boolean {
  return typeof cmd === 'object' && cmd !== null && TEXT_COMMANDS.has(cmd.type);
}

/** Виды сообщений worker'у: остальное — неверный конверт. */
export const TO_WORKER_KINDS: ReadonlySet<string> = new Set(['init', 'cmd', 'pasteText', 'encodeClipboard', 'tick', 'testPanic']);
````

Создать `apps/web/src/core/text-keys.ts`:

````ts
// Клавиши в поле правки текста (спека подсистемы 3, раздел 7): что отдать ядру
// командой `TextKey` (ход курсора, удаление, отмена, форматирование), что
// оставить браузеру (набор и IME идут событиями `beforeinput` и составления,
// Ctrl/Cmd+C, X, V — событиями буфера обмена) и что погасить: остальные команды
// раскладки в правке текста не действуют.

// Модуль без DOM: его берёт и worker ядра (предел команды ввода в `messages.ts`).

/** Платформа (как `Platform` в `core.ts`, без зависимости от DOM). */
type Platform = 'mac' | 'windows' | 'linux';

export type TextKeyAction = 'core' | 'browser' | 'block';

/** Ход курсора и удаление — ядро. */
const NAV = new Set([
  'ArrowLeft',
  'ArrowRight',
  'ArrowUp',
  'ArrowDown',
  'Home',
  'End',
  'PageUp',
  'PageDown',
  'Backspace',
  'Delete',
  'Escape',
  'Tab',
]);

/** Сочетания с главной клавишей, которые обрабатывает ядро в правке. */
const CORE_MOD = new Set(['KeyZ', 'KeyA', 'KeyB', 'KeyI', 'KeyU']);
/** С Alt и главной клавишей — выравнивание. */
const ALIGN = new Set(['KeyL', 'KeyT', 'KeyR', 'KeyJ']);
/** Буфер обмена — событиями браузера. */
const CLIPBOARD = new Set(['KeyC', 'KeyX', 'KeyV']);

/** Поля `KeyboardEvent`, нужные решению. */
export interface KeyLike {
  code: string;
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  isComposing: boolean;
}

/** Сочетания, которые оставляем браузеру и в правке (перезагрузка, инструменты). */
function browserKey(e: KeyLike): boolean {
  if (/^F\d+$/.test(e.key)) return true;
  const mod = e.metaKey || e.ctrlKey;
  return mod && (e.code === 'KeyR' || e.code === 'KeyW' || e.code === 'KeyT' || (e.shiftKey && e.code === 'KeyI'));
}

export function textKeyAction(e: KeyLike, platform: Platform): TextKeyAction {
  // Составление IME: клавиши — его (Enter подтверждает, Esc отменяет).
  if (e.isComposing || e.key === 'Process' || e.key === 'Dead') return 'browser';
  const primary = platform === 'mac' ? e.metaKey : e.ctrlKey;
  if (NAV.has(e.code)) return 'core';
  if (primary && e.altKey && ALIGN.has(e.code)) return 'core';
  if (browserKey(e)) return 'browser';
  // Ctrl+Y — повтор на Windows и Linux.
  if (platform !== 'mac' && e.ctrlKey && !e.altKey && !e.shiftKey && e.code === 'KeyY') return 'core';
  if (primary && !e.altKey) {
    if (CORE_MOD.has(e.code)) return 'core';
    if (e.code === 'KeyX' && e.shiftKey) return 'core';
    if (CLIPBOARD.has(e.code) && !e.shiftKey) return 'browser';
    // Сохранить — обработчик редактора; остальные команды в правке не действуют.
    if (e.code === 'KeyS') return 'browser';
    return 'block';
  }
  return 'browser';
}

/** Вид ввода `beforeinput` → вид команды `TextInput`; `null` — не наш (или его
 * ведут события составления и буфера обмена). */
export function inputKind(
  inputType: string,
):
  | 'insert'
  | 'paragraph'
  | 'deleteBackward'
  | 'deleteForward'
  | 'deleteWordBackward'
  | 'deleteWordForward'
  | 'deleteLineBackward'
  | 'deleteLineForward'
  | null {
  switch (inputType) {
    case 'insertText':
    case 'insertReplacementText':
      return 'insert';
    case 'insertLineBreak':
    case 'insertParagraph':
      return 'paragraph';
    case 'deleteContentBackward':
      return 'deleteBackward';
    case 'deleteContentForward':
      return 'deleteForward';
    case 'deleteWordBackward':
      return 'deleteWordBackward';
    case 'deleteWordForward':
      return 'deleteWordForward';
    case 'deleteSoftLineBackward':
    case 'deleteHardLineBackward':
      return 'deleteLineBackward';
    case 'deleteSoftLineForward':
    case 'deleteHardLineForward':
      return 'deleteLineForward';
    default:
      return null;
  }
}

/** Предел текста одной команды ввода (спека 8): 64 КБ в UTF-8. */
export const MAX_TEXT_INPUT_BYTES = 64 * 1024;

/** Длина строки в UTF-8 без кодирования (суррогатная пара — 4 байта). */
export function utf8Length(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
      n += 4;
      i++;
    } else n += 3;
  }
  return n;
}
````

В `apps/web/src/core/transport.ts` заменить:

````ts
  /** Кадр основного потока (`requestAnimationFrame`): разбудить worker. */
  tick(): void;
  /** Последнее живое состояние или `null`, если его ещё не было. */
  live(): Float64Array | null;
````

на:

````ts
  /** Кадр основного потока (`requestAnimationFrame`): разбудить worker. */
  tick(): void;
  /** Ввод, ждущий отправки, — worker'у сейчас, до следующего сообщения (команды
   * правки текста идут после щелчков, положенных раньше них). */
  flush(): void;
  /** Последнее живое состояние или `null`, если его ещё не было. */
  live(): Float64Array | null;
````

В `apps/web/src/core/transport.ts` заменить:

````ts
  }

  tick(): void {
    if (this.pendingCount > 0) this.flushPending();
````

на:

````ts
  }

  flush(): void {
    if (this.pendingCount > 0) this.flushPending();
  }

  tick(): void {
    if (this.pendingCount > 0) this.flushPending();
````

В `apps/web/src/core/transport.ts` заменить:

````ts
  };

  tick(): void {
    if (this.inFlight) return;
````

на:

````ts
  };

  flush(): void {
    if (this.count === 0) return;
    // Кадр вне очереди: ответ `live` снимет и признак ожидания прошлого кадра.
    const input = this.buf.slice(0, this.count * RECORD_SIZE);
    this.count = 0;
    this.inFlight = true;
    this.worker.postMessage({ t: 'tick', input } satisfies ToWorker, [input]);
  }

  tick(): void {
    if (this.inFlight) return;
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { clipboardAction, eventShortcut } from './shortcuts';
import { installClipboard } from './system-clipboard';
import { createUiStore } from './store';
import { Toasts } from './toasts';
````

на:

````tsx
import { clipboardAction, eventShortcut } from './shortcuts';
import { installClipboard } from './system-clipboard';
import { installTextInput } from './text-input';
import { createUiStore } from './store';
import { Toasts } from './toasts';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        setEditor(ed);
        canvas.focus();
        uninstall = installClipboard(ed);
        saver = createSaver(ed, props.files, id, version, stored, { ...testIntervals(), journalSession });
        // Ctrl/Cmd+S, пока фокус в поле панели (клавиша не ушла ядру): тоже
````

на:

````tsx
        setEditor(ed);
        canvas.focus();
        const offClipboard = installClipboard(ed);
        // Поле правки текста — в сцене холста (под курсором текста).
        const offText = installTextInput(ed, canvas.parentElement ?? document.body, canvas);
        uninstall = () => {
          offText();
          offClipboard();
        };
        saver = createSaver(ed, props.files, id, version, stored, { ...testIntervals(), journalSession });
        // Ctrl/Cmd+S, пока фокус в поле панели (клавиша не ушла ядру): тоже
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
}

/** Запись журнала: тип и размер, но не сами данные (картинки и буфер обмена — до 64 МБ). */
function note(e: CoreEvent): { type: string; size?: number; message?: string } {
  const n: { type: string; size?: number; message?: string } = { type: e.type };
  if ('bytes' in e && e.bytes instanceof Uint8Array) n.size = e.bytes.length;
  if (e.type === 'error') n.message = e.message;
  return n;
}
````

на:

````ts
}

/** Запись журнала: тип и размер, но не сами данные (картинки и буфер обмена — до 64 МБ).
 * У копии фрагмента текста — её обычный текст (не больше 64 КБ). */
function note(e: CoreEvent): { type: string; size?: number; message?: string; text?: string } {
  const n: { type: string; size?: number; message?: string; text?: string } = { type: e.type };
  if ('bytes' in e && e.bytes instanceof Uint8Array) n.size = e.bytes.length;
  if (e.type === 'error') n.message = e.message;
  if (e.type === 'clipboard' && typeof e.plain === 'string') n.text = e.plain;
  return n;
}
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
    },
  });
  const uiEvents: { type: string; size?: number; message?: string }[] = [];
  const isClipboardKey = opts.isClipboardKey ?? (() => false);
  let unguard = () => {};
````

на:

````ts
    },
  });
  const uiEvents: { type: string; size?: number; message?: string; text?: string }[] = [];
  const isClipboardKey = opts.isClipboardKey ?? (() => false);
  let unguard = () => {};
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  FontChoice,
  KeymapEntry,
  MissingFontDto,
  LayerRow,
````

на:

````ts
  FontChoice,
  KeymapEntry,
  TextStyleDto,
  MissingFontDto,
  LayerRow,
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  /** Системные шрифты: есть ли на платформе и показаны ли уже. */
  systemFonts: { supported: boolean; shown: boolean };
}

````

на:

````ts
  /** Системные шрифты: есть ли на платформе и показаны ли уже. */
  systemFonts: { supported: boolean; shown: boolean };
  /** Правка текста на холсте (подсистема 3, этап 2): идёт ли и новый ли текст. */
  text: { editing: boolean; draft: boolean };
  /** Стиль для секции «Текст»; `null` — текстов не выделено. */
  textStyle: TextStyleDto | null;
  /** Семейства текстов текущей страницы (группа «В документе» выбора шрифта). */
  pageFonts: string[];
}

````

В `apps/web/src/ui/store.ts` заменить:

````ts
  fontCatalog: [],
  systemFonts: { supported: false, shown: false },
};

````

на:

````ts
  fontCatalog: [],
  systemFonts: { supported: false, shown: false },
  text: { editing: false, draft: false },
  textStyle: null,
  pageFonts: [],
};

````

В `apps/web/src/ui/store.ts` заменить:

````ts
        setState('fontCatalog', e.families);
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
````

на:

````ts
        setState('fontCatalog', e.families);
        break;
      case 'textCaret':
        setState('text', { editing: e.editing, draft: e.draft });
        break;
      case 'textStyleSummary':
        setState('textStyle', e.summary ? reconcile(e.summary) : null);
        break;
      case 'pageFonts':
        setState('pageFonts', e.families);
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
````

В `apps/web/src/ui/styles/app.css` заменить:

````css
  text-align: center;
}
````

на:

````css
  text-align: center;
}

/* Поле правки текста: невидимое, под курсором текста на холсте (окно IME
   появляется рядом с текстом). Текста документа в нём нет. */
.fe-text-input {
  position: absolute;
  top: 0;
  left: 0;
  width: 1px;
  height: 1px;
  padding: 0;
  border: 0;
  margin: 0;
  opacity: 0;
  resize: none;
  overflow: hidden;
  white-space: pre;
  pointer-events: none;
  caret-color: transparent;
  color: transparent;
  background: transparent;
  outline: none;
}
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
import { fileBytes, IMAGE_FILE, pickFile } from './files';
import { editableTarget, modalOpen } from './keys';

/** Событие из поля ввода, из диалога (и немодального — палитры), из меню или при
````

на:

````ts
import { fileBytes, IMAGE_FILE, pickFile } from './files';
import { editableTarget, modalOpen } from './keys';
import { isTextField } from './text-input';
import { MAX_TEXT_INPUT_BYTES, utf8Length } from '../core/text-keys';

/** Событие из поля ввода, из диалога (и немодального — палитры), из меню или при
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
  };

  const write = (e: { bytes: Uint8Array } & ClipboardExtra) => {
    const cut = e.cut === true;
    if (!supportsCustomFormat()) {
      if (typeof e.text === 'string') writeText(e.text, cut);
````

на:

````ts
  };

  /** Фрагмент текста в правке: обычный текст и (где можно) наш формат со стилями.
   * Без своего формата — только обычный текст (запасной путь `execCommand`). */
  const writeFragment = (bytes: Uint8Array, plain: string, cut: boolean) => {
    if (!supportsCustomFormat() || typeof ClipboardItem === 'undefined') {
      writeText(plain, cut);
      return;
    }
    navigator.clipboard
      .write([
        new ClipboardItem({
          'text/plain': new Blob([plain], { type: 'text/plain' }),
          [FERRUM_MIME]: new Blob([bytes as BlobPart], { type: FERRUM_MIME }),
        }),
      ])
      .catch(() => writeText(plain, cut));
  };

  const write = (e: { bytes: Uint8Array; plain?: string } & ClipboardExtra) => {
    const cut = e.cut === true;
    if (typeof e.plain === 'string') {
      writeFragment(e.bytes, e.plain, cut);
      return;
    }
    if (!supportsCustomFormat()) {
      if (typeof e.text === 'string') writeText(e.text, cut);
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
      return;
    }
    if (textTarget(e.target)) return;
    e.preventDefault();
````

на:

````ts
      return;
    }
    // Поле правки текста: копия фрагмента — от ядра (в поле текста документа нет).
    if (isTextField(e.target) && !modalOpen()) {
      e.preventDefault();
      ed.run(command);
      return;
    }
    if (textTarget(e.target)) return;
    e.preventDefault();
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
  const copy = onCopy('edit.copy');
  const cut = onCopy('edit.cut');
  const paste = (e: ClipboardEvent) => {
    if (textTarget(e.target)) return;
    e.preventDefault();
    const sync = fromDataTransfer(e.clipboardData);
````

на:

````ts
  const copy = onCopy('edit.copy');
  const cut = onCopy('edit.cut');
  /** Обычный текст другого приложения: в правке — в позицию курсора, вне её —
   * новым текстом (ядро). Больше предела команды — отказ сразу. */
  const pastePlain = (text: string) => {
    if (utf8Length(text) > MAX_TEXT_INPUT_BYTES) {
      toast('Слишком длинный текст: за раз — не больше 64 КБ');
      return;
    }
    ed.send({ type: 'textInput', kind: 'paste', data: text });
  };
  const paste = (e: ClipboardEvent) => {
    if (textTarget(e.target) && !(isTextField(e.target) && !modalOpen())) return;
    e.preventDefault();
    const sync = fromDataTransfer(e.clipboardData);
````

В `apps/web/src/ui/system-clipboard.ts` заменить:

````ts
    // Свой формат асинхронного API в событии не виден: событие пустое — читаем его.
    else if (eventIsEmpty(e.clipboardData)) void readAsync(true).then(deliver);
  };
  document.addEventListener('copy', copy);
````

на:

````ts
    // Свой формат асинхронного API в событии не виден: событие пустое — читаем его.
    else if (eventIsEmpty(e.clipboardData)) void readAsync(true).then(deliver);
    else {
      const text = e.clipboardData?.getData('text/plain') ?? '';
      if (text !== '') {
        // Свой формат (стили) — в асинхронном API рядом с обычным текстом.
        if (supportsCustomFormat()) {
          void readAsync(true).then((d) => (d?.kind === 'ferrum' ? deliver(d) : pastePlain(text)));
        } else pastePlain(text);
      }
    }
  };
  document.addEventListener('copy', copy);
````

Создать `apps/web/src/ui/text-input.ts`:

````ts
// Поле ввода правки текста (спека подсистемы 3, раздел 7): скрытый `<textarea>`
// в основном потоке получает фокус на время правки. Текста документа в нём нет:
// набор (`beforeinput`), составление IME (`composition*`) и клавиши хода курсора
// уходят ядру командами, ядро отвечает прямоугольником курсора (`textCaret`) —
// поле встаёт под курсор, и окно IME появляется у текста. Буфер обмена в правке
// — `system-clipboard.ts` (событие с полем как целью).

import type { Editor } from './context';
import { inputKind, textKeyAction } from '../core/text-keys';

/** Поле правки текста — его узнаёт `system-clipboard.ts`. */
export const TEXT_FIELD_CLASS = 'fe-text-input';

export function isTextField(t: EventTarget | null): boolean {
  return t instanceof HTMLTextAreaElement && t.classList.contains(TEXT_FIELD_CLASS);
}

export function installTextInput(ed: Editor, stage: HTMLElement, canvas: HTMLCanvasElement): () => void {
  const field = document.createElement('textarea');
  field.className = TEXT_FIELD_CLASS;
  field.setAttribute('aria-label', 'Правка текста');
  field.setAttribute('autocomplete', 'off');
  field.setAttribute('autocapitalize', 'off');
  field.setAttribute('spellcheck', 'false');
  field.tabIndex = -1;
  stage.appendChild(field);
  let editing = false;
  let composing = false;

  const send = ed.send.bind(ed);
  const clear = () => {
    if (!composing && field.value !== '') field.value = '';
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const action = textKeyAction(e, ed.platform);
    if (action === 'browser') return;
    e.preventDefault();
    // Клавиша обработана здесь: обработчики окна (горячие клавиши ядра) её не видят.
    e.stopPropagation();
    if (action === 'core') {
      send({
        type: 'textKey',
        key: e.code,
        mods: { shift: e.shiftKey, alt: e.altKey, ctrl: e.ctrlKey, meta: e.metaKey },
      });
    }
  };
  const onBeforeInput = (e: InputEvent) => {
    // Составление IME браузер не даёт отменить: его ведут события составления.
    if (e.inputType === 'insertCompositionText' || e.isComposing) return;
    e.preventDefault();
    if (e.inputType === 'historyUndo' || e.inputType === 'historyRedo') {
      send({ type: 'textKey', key: 'KeyZ', mods: { shift: e.inputType === 'historyRedo', ctrl: ed.platform !== 'mac', meta: ed.platform === 'mac', alt: false } });
      return;
    }
    const kind = inputKind(e.inputType);
    if (!kind) return;
    send({ type: 'textInput', kind, data: e.data ?? undefined });
  };
  const onCompositionStart = () => {
    composing = true;
  };
  const onCompositionUpdate = (e: CompositionEvent) => {
    send({ type: 'textInput', kind: 'compose', data: e.data });
  };
  const onCompositionEnd = (e: CompositionEvent) => {
    composing = false;
    send({ type: 'textInput', kind: 'commit', data: e.data });
    field.value = '';
  };
  const onInput = () => clear();
  field.addEventListener('keydown', onKeyDown);
  field.addEventListener('beforeinput', onBeforeInput);
  field.addEventListener('compositionstart', onCompositionStart);
  field.addEventListener('compositionupdate', onCompositionUpdate);
  field.addEventListener('compositionend', onCompositionEnd);
  field.addEventListener('input', onInput);

  const off = ed.core.on((e) => {
    if (e.type !== 'textCaret') return;
    if (e.editing && e.rect) {
      // Поле — под курсором (CSS-пиксели холста; холст — в начале сцены).
      const [x, y, , h] = e.rect;
      field.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
      field.style.height = `${Math.max(1, Math.round(h))}px`;
      ed.core.keyTarget = field;
      // Фокус — полю, если он не в панели (правка шрифта из панели не прерывает правку).
      const active = document.activeElement;
      if (!editing || active === canvas || active === document.body || active === null) {
        field.focus({ preventScroll: true });
      }
      editing = true;
    } else if (editing) {
      editing = false;
      ed.core.keyTarget = null;
      composing = false;
      field.value = '';
      if (document.activeElement === field) canvas.focus({ preventScroll: true });
    }
  });

  return () => {
    off();
    ed.core.keyTarget = null;
    field.removeEventListener('keydown', onKeyDown);
    field.removeEventListener('beforeinput', onBeforeInput);
    field.removeEventListener('compositionstart', onCompositionStart);
    field.removeEventListener('compositionupdate', onCompositionUpdate);
    field.removeEventListener('compositionend', onCompositionEnd);
    field.removeEventListener('input', onInput);
    field.remove();
  };
}
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
const LATER: { name: string; key: string; when: string; icon: IconName }[] = [
  { name: 'Перо', key: 'P', when: 'подсистема 3', icon: 'pen' },
  { name: 'Текст', key: 'T', when: 'подсистема 3', icon: 'text' },
  { name: 'Комментарии', key: 'C', when: 'подсистема 4', icon: 'comment' },
];
````

на:

````tsx
const LATER: { name: string; key: string; when: string; icon: IconName }[] = [
  { name: 'Перо', key: 'P', when: 'подсистема 3', icon: 'pen' },
  { name: 'Комментарии', key: 'C', when: 'подсистема 4', icon: 'comment' },
];
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
        </button>
      </div>
      <span class="fe-toolbar__sep" role="separator" />
      <For each={LATER}>
````

на:

````tsx
        </button>
      </div>
      <button
        class="fe-tool"
        aria-pressed={tool() === 'text'}
        aria-label="Текст"
        data-tip="Текст"
        data-command="tool.text"
        data-tip-side="top"
        onClick={() => setTool('text')}
      >
        <Icon name="text" />
      </button>
      <span class="fe-toolbar__sep" role="separator" />
      <For each={LATER}>
````

В `apps/web/src/window.d.ts` заменить:

````ts
      scheduler?: Scheduler;
      events: CoreEvent[];
      /** Редактор, тестовый режим: тип, размер байтов и текст ошибки каждого события. */
      uiEvents?: { type: string; size?: number; message?: string }[];
      /** Документ открыт (событие `opened`). */
      ready: boolean;
````

на:

````ts
      scheduler?: Scheduler;
      events: CoreEvent[];
      /** Редактор, тестовый режим: тип, размер байтов и текст ошибки каждого события
       * (у копии фрагмента текста — её обычный текст). */
      uiEvents?: { type: string; size?: number; message?: string; text?: string }[];
      /** Документ открыт (событие `opened`). */
      ready: boolean;
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  commandSizeError,
  copyTooLarge,
  isToWorker,
  pasteTextError,
````

на:

````ts
  commandSizeError,
  copyTooLarge,
  isTextCommand,
  isToWorker,
  pasteTextError,
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
    postError(error);
    return;
  }
  core!.command(cmd.cmd);
````

на:

````ts
    postError(error);
    return;
  }
  // Правка текста — после ввода, положенного в кольцо до неё (щелчок, затем набор).
  if (sab && isTextCommand(cmd.cmd)) {
    const input = drain();
    if (input.byteLength > 0) core!.input(input);
  }
  core!.command(cmd.cmd);
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npx playwright test e2e/text-edit.spec.ts e2e/app.spec.ts e2e/clipboard.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npx playwright test e2e/text-edit.spec.ts --project=firefox`

Expected: модульные — 125 passed; E2E `text-edit`, `app`, `clipboard` в Chromium — passed; Firefox — 2 passed (тесты с @firefox)

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/text-edit.spec.ts apps/web/src/core/core.ts apps/web/src/core/messages.ts apps/web/src/core/text-keys.ts apps/web/src/core/transport.ts apps/web/src/ui/app.tsx apps/web/src/ui/boot.ts apps/web/src/ui/store.ts apps/web/src/ui/styles/app.css apps/web/src/ui/system-clipboard.ts apps/web/src/ui/text-input.ts apps/web/src/ui/toolbar.tsx apps/web/src/window.d.ts apps/web/src/worker/core.worker.ts apps/web/unit/text-keys.test.ts
git commit -m "feat(web): поле правки текста — скрытое поле под курсором, набор и IME событиями, клавиши хода курсора ядру, буфер обмена фрагментов и обычного текста, инструмент T в тулбаре, ввод раньше команд текста

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 11: Секция «Текст» и выбор шрифта

Спека 7: секция показывает сводку ядра и шлёт `setTextStyle`/`setTextProps` (в правке — без id: диапазон держит ядро; вне её — выделенные на начало правки). Выбор шрифта — поиск, группы, виртуальный список (строки 28 px, окно по прокрутке, чтение прокрутки — в фазе `measure`), название — своим шрифтом (вшитые — `FontFace` с файла сборки), «Показать системные шрифты». Начертание, размер, интерлиньяж, трекинг, интервал и отступ абзаца, выравнивание, режим размера, регистр, подчёркивание и зачёркивание, списки.

**Files:**
- Create: `apps/web/src/ui/text-panel.ts`, `font-picker.tsx`, `text-section.tsx`
- Modify: `apps/web/src/ui/properties.tsx`, `missing-fonts.tsx`, `icons.tsx`, `styles/properties.css`, `apps/web/e2e/text.spec.ts` (системные шрифты — из выбора шрифта)
- Test: `apps/web/unit/text-panel.test.ts`, `apps/web/e2e/text-panel.spec.ts` (новые)

**Interfaces:**
- Produces: `parseSpacing`, `formatSpacing`, `fontRows(catalog, pageFonts, query) -> FontRow[]`, `styleFor`, `rowWindow`; компоненты `FontPicker`, `TextSection`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/e2e/text-panel.spec.ts`:

````ts
// Секция «Текст» панели свойств (подсистема 3, этап 2): сводка выделенных
// текстов со «Смешанными», размер полем — один шаг, выбор шрифта с поиском и
// группами, начертание, выравнивание и режим размера, правка выделенного
// диапазона в правке текста.

import { expect, test, type Page } from '@playwright/test';
import { live, liveWhen, openApp } from './helpers';

const ui = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!)));

/** Текст тестовой командой; ждёт узел (шрифт грузится асинхронно). */
async function createText(page: Page, x: number, y: number, text: string, size: number): Promise<void> {
  const before = (await live(page)).nodes;
  await page.evaluate(
    ({ x, y, text, size }) => window.__ferrum!.core!.send({ type: 'testCreateText', x, y, text, size }),
    { x, y, text, size },
  );
  await liveWhen(page, `l.nodes === ${before + 1} && l.busy === 0`);
}

const section = (page: Page) => page.getByRole('region', { name: 'Текст' });

test('сводка, размер полем одним шагом, «Смешанные» у разных', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Первый', 24);
  await expect(section(page)).toBeVisible();
  await expect(section(page).getByRole('button', { name: 'Шрифт', exact: true })).toHaveText('Inter');
  const size = section(page).getByRole('textbox', { name: 'Размер шрифта' });
  await expect(size).toHaveValue('24');
  const steps = (await live(page)).steps;
  await size.fill('40');
  await size.press('Enter');
  await liveWhen(page, `l.steps === ${steps + 1}`);
  await expect.poll(async () => (await ui(page)).textStyle.size.value).toBe(40);
  // Второй текст другого размера: вместе — «Смешанные» (поле без фокуса).
  await page.locator('canvas').focus();
  await createText(page, 0, 200, 'Второй', 12);
  const ids = (await ui(page)).layers.rows.map((r: { id: string }) => r.id);
  await page.evaluate((ids) => window.__ferrum!.core!.send({ type: 'select', ids, mode: 'replace' }), ids);
  await expect(size).toHaveValue('Смешанные');
  await page.keyboard.press('ControlOrMeta+z');
  await liveWhen(page, `l.steps === ${steps + 2}`);
});

test('выбор шрифта: поиск, группы, начертание того же имени', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Шрифт', 24);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setTextStyle', patch: { family: 'Inter', style: 'Bold' }, phase: 'commit' }));
  await expect.poll(async () => (await ui(page)).textStyle.style.value).toBe('Bold');
  await section(page).getByRole('button', { name: 'Шрифт', exact: true }).click();
  const list = section(page).getByRole('listbox', { name: 'Шрифты' });
  // Inter — и «В документе», и во «Вшитых»: обе строки отмечены текущими.
  await expect(list.getByRole('option', { name: 'Inter' })).toHaveCount(2);
  await expect(list.getByRole('option', { name: 'Inter' }).first()).toHaveAttribute('aria-selected', 'true');
  await section(page).getByRole('searchbox', { name: 'Поиск шрифта' }).fill('robo');
  await expect(list.getByRole('option')).toHaveCount(1);
  await list.getByRole('option', { name: 'Roboto' }).click();
  // Roboto качает worker ядра; начертание — то же имя (Bold).
  await expect.poll(async () => (await ui(page)).textStyle.family.value, { timeout: 10_000 }).toBe('Roboto');
  expect((await ui(page)).textStyle.style.value).toBe('Bold');
  await expect(section(page).getByRole('combobox', { name: 'Начертание' })).toHaveValue('Bold');
  // Группы списка — вшитые семейства, без поиска их десять.
  await section(page).getByRole('button', { name: 'Шрифт', exact: true }).click();
  await expect(list.getByRole('option')).not.toHaveCount(0);
});

test('выравнивание, режим размера и подчёркивание — кнопками', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Кнопки', 24);
  await section(page).getByRole('radio', { name: 'По центру' }).click();
  await expect.poll(async () => (await ui(page)).textStyle.alignH.value).toBe('center');
  await section(page).getByRole('radio', { name: 'Фиксированный размер' }).click();
  await expect.poll(async () => (await ui(page)).textStyle.sizing.value).toBe('fixed');
  await section(page).getByRole('button', { name: 'Подчёркнутый' }).click();
  await expect(section(page).getByRole('button', { name: 'Подчёркнутый' })).toHaveAttribute('aria-pressed', 'true');
  await section(page).getByRole('combobox', { name: 'Регистр' }).selectOption('upper');
  await expect.poll(async () => (await ui(page)).textStyle.case.value).toBe('upper');
});

test('в правке текста панель правит выделенный диапазон', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Диапазон', 24);
  await page.locator('canvas').focus();
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await ui(page)).text.editing).toBe(true);
  await page.keyboard.press('Home');
  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.press('Shift+ArrowRight');
  await expect.poll(async () => (await ui(page)).textStyle.range).toBe(true);
  const size = section(page).getByRole('textbox', { name: 'Размер шрифта' });
  await size.fill('48');
  await size.press('Enter');
  await expect.poll(async () => (await ui(page)).textStyle.size.value).toBe(48);
  // Правка из панели не выходит из правки текста; весь текст — «Смешанные».
  expect((await ui(page)).text.editing).toBe(true);
  await page.locator('.fe-text-input').focus();
  await page.keyboard.press('ControlOrMeta+a');
  await expect.poll(async () => (await ui(page)).textStyle.size.mixed).toBe(true);
});
````

В `apps/web/e2e/text.spec.ts` заменить:

````ts
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'select', ids: [], mode: 'replace' }));
  await expect(page.getByRole('region', { name: 'Отсутствующие шрифты (1)' })).toBeVisible();
  await page.getByRole('button', { name: 'Показать системные шрифты' }).click();
  // Шрифт нашёлся: список пуст, текст нарисован системным шрифтом (байты пришли).
  await expect(page.getByRole('region', { name: /Отсутствующие шрифты/ })).toHaveCount(0);
````

на:

````ts
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'select', ids: [], mode: 'replace' }));
  await expect(page.getByRole('region', { name: 'Отсутствующие шрифты (1)' })).toBeVisible();
  // «Показать системные шрифты» — в выборе шрифта секции «Текст» (этап 2).
  const id = await page.evaluate(() => window.__ferrum!.ui!.layers.rows[0].id);
  await page.evaluate((id) => window.__ferrum!.core!.send({ type: 'select', ids: [id], mode: 'replace' }), id);
  await page.getByRole('button', { name: 'Шрифт', exact: true }).click();
  await page.getByRole('button', { name: 'Показать системные шрифты' }).click();
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'select', ids: [], mode: 'replace' }));
  // Шрифт нашёлся: список пуст, текст нарисован системным шрифтом (байты пришли).
  await expect(page.getByRole('region', { name: /Отсутствующие шрифты/ })).toHaveCount(0);
````

Создать `apps/web/unit/text-panel.test.ts`:

````ts
// Секция «Текст»: разбор интерлиньяжа и трекинга, группы выбора шрифта и поиск,
// начертание при смене семейства, окно виртуального списка.

import { expect, test } from '@playwright/test';
import { fontRows, formatSpacing, parseSpacing, rowWindow, styleFor } from '../src/ui/text-panel';

test('интерлиньяж и трекинг: авто, проценты, пиксели', () => {
  expect(parseSpacing('Авто', true)).toEqual({ unit: 'auto', value: 0 });
  expect(parseSpacing('', true)).toEqual({ unit: 'auto', value: 0 });
  expect(parseSpacing('', false)).toBeNull();
  expect(parseSpacing('150%', true)).toEqual({ unit: 'percent', value: 150 });
  expect(parseSpacing('-0,5', false)).toEqual({ unit: 'px', value: -0.5 });
  expect(parseSpacing('20px', true)).toEqual({ unit: 'px', value: 20 });
  expect(parseSpacing('abc', true)).toBeNull();
  expect(formatSpacing({ unit: 'percent', value: 120 })).toBe('120%');
  expect(formatSpacing({ unit: 'auto', value: 0 })).toBe('Авто');
});

const catalog = [
  { family: 'Roboto', source: 'bundled' as const, styles: ['Regular', 'Bold'] },
  { family: 'Inter', source: 'bundled' as const, styles: ['Regular', 'Bold', 'Italic'] },
  { family: 'Arial', source: 'system' as const, styles: ['Regular'] },
];

test('группы шрифтов и поиск', () => {
  const rows = fontRows(catalog, ['Comic Sans', 'Inter'], '');
  expect(rows.map((r) => (r.kind === 'heading' ? `#${r.label}` : r.family))).toEqual([
    '#В документе',
    'Comic Sans',
    'Inter',
    '#Вшитые',
    'Inter',
    'Roboto',
    '#Системные',
    'Arial',
  ]);
  const found = fontRows(catalog, [], 'rob');
  expect(found).toEqual([{ kind: 'heading', label: 'Вшитые' }, { kind: 'font', family: 'Roboto', source: 'bundled' }]);
  expect(fontRows(catalog, [], 'zzz')).toEqual([]);
});

test('начертание при смене семейства и окно списка', () => {
  expect(styleFor(catalog, 'Roboto', 'Bold')).toBe('Bold');
  expect(styleFor(catalog, 'Roboto', 'Italic')).toBe('Regular');
  expect(styleFor(catalog, 'Нет такого', 'Bold')).toBe('Regular');
  expect(rowWindow(0, 280, 28, 100)).toEqual({ start: 0, end: 22 });
  expect(rowWindow(28 * 50, 280, 28, 100)).toEqual({ start: 44, end: 66 });
  expect(rowWindow(28 * 95, 280, 28, 100).end).toBe(100);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npx playwright test -c playwright.unit.config.ts unit/text-panel.test.ts`
Expected: FAIL — модуль `src/ui/text-panel.ts` не найден

- [ ] **Step 3: Реализация**

Создать `apps/web/src/ui/font-picker.tsx`:

````tsx
// Выбор шрифта секции «Текст» (спека подсистемы 3, раздел 7): поиск, группы
// «В документе» / «Вшитые» / «Системные», виртуальный список (тысячи системных
// семейств — в DOM только видимые строки), название — своим шрифтом. Вшитые для
// предпросмотра грузит браузер (`FontFace` с файла сборки — его же HTTP-кэш, что у
// worker'а ядра), системные Chrome и десктопа — по имени. «Показать системные
// шрифты» — здесь же.

import { createMemo, createSignal, For, onMount, Show } from 'solid-js';
import { manifestFamilies } from '../core/fonts';
import { useEditor } from './context';
import { fontRows, rowWindow, type FontRow } from './text-panel';

const ROW = 28;
const LIST_HEIGHT = 280;
/** Имя `FontFace` предпросмотра вшитого семейства (своё — не путать с шрифтами интерфейса). */
const PREVIEW = 'ferrum-preview-';

/** Файл обычного начертания каждого вшитого семейства (манифест сборки). */
let manifest: Promise<Map<string, string>> | null = null;
const previewed = new Set<string>();

function bundledFiles(): Promise<Map<string, string>> {
  manifest ??= fetch(new URL('/fonts/manifest.json', location.origin).href)
    .then((r) => (r.ok ? r.json() : { families: [] }))
    .then((json: unknown) => {
      const map = new Map<string, string>();
      for (const f of manifestFamilies(json)) {
        const face = f.faces.find((x) => /regular/i.test(x.style)) ?? f.faces[0];
        if (face) map.set(f.family, face.file);
      }
      return map;
    })
    .catch(() => new Map());
  return manifest;
}

/** CSS-семейство предпросмотра названия; вшитое — догружается при первом показе. */
function previewFamily(family: string, source: 'bundled' | 'system'): string {
  const quoted = `"${family.replace(/["\\]/g, '')}"`;
  if (source !== 'bundled') return `${quoted}, var(--font-sans)`;
  if (!previewed.has(family)) {
    previewed.add(family);
    void bundledFiles().then((files) => {
      const file = files.get(family);
      if (!file) return;
      const face = new FontFace(PREVIEW + family, `url(/fonts/${file})`);
      face.load().then(
        (f) => document.fonts.add(f),
        () => {},
      );
    });
  }
  return `"${PREVIEW}${family.replace(/["\\]/g, '')}", var(--font-sans)`;
}

export function FontPicker(props: { current: string; onPick: (family: string) => void; onClose: () => void }) {
  const ed = useEditor();
  const [query, setQuery] = createSignal('');
  const [top, setTop] = createSignal(0);
  const rows = createMemo(() => fontRows(ed.store.state.fontCatalog, ed.store.state.pageFonts, query()));
  const visible = createMemo(() => {
    const w = rowWindow(top(), LIST_HEIGHT, ROW, rows().length);
    return rows()
      .slice(w.start, w.end)
      .map((r, i) => ({ row: r, index: w.start + i }));
  });
  const sourceOf = (family: string): 'bundled' | 'system' =>
    ed.store.state.fontCatalog.find((c) => c.family === family)?.source ?? 'system';
  const sys = () => ed.store.state.systemFonts;
  let list!: HTMLDivElement;
  let search!: HTMLInputElement;
  onMount(() => search.focus());
  // Прокрутка читается в фазе measure (защита layout тестового режима).
  const onScroll = ed.core.costed(() => ed.scheduler.measure(() => setTop(list.scrollTop)));
  const first = () => rows().find((r): r is FontRow & { kind: 'font' } => r.kind === 'font');
  return (
    <div class="fe-fonts" role="group" aria-label="Выбор шрифта">
      <input
        ref={search}
        class="fe-input fe-fonts__search"
        type="search"
        aria-label="Поиск шрифта"
        placeholder="Поиск шрифта"
        value={query()}
        onInput={(e) => {
          setQuery(e.currentTarget.value);
          setTop(0);
          list.scrollTop = 0;
        }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            props.onClose();
          } else if (e.key === 'Enter') {
            e.preventDefault();
            const f = first();
            if (f) props.onPick(f.family);
          }
        }}
      />
      <div ref={list} class="fe-fonts__list" role="listbox" aria-label="Шрифты" style={{ height: `${LIST_HEIGHT}px` }} onScroll={onScroll}>
        <div style={{ height: `${rows().length * ROW}px`, position: 'relative' }}>
          <For each={visible()}>
            {(v) =>
              v.row.kind === 'heading' ? (
                <div class="fe-fonts__heading t-muted" role="presentation" style={{ top: `${v.index * ROW}px` }}>
                  {v.row.label}
                </div>
              ) : (
                <div
                  class="fe-fonts__row"
                  role="option"
                  aria-selected={v.row.family === props.current}
                  style={{
                    top: `${v.index * ROW}px`,
                    'font-family': previewFamily(v.row.family, v.row.source === 'document' ? sourceOf(v.row.family) : v.row.source),
                  }}
                  onClick={() => v.row.kind === 'font' && props.onPick(v.row.family)}
                >
                  {v.row.family}
                </div>
              )
            }
          </For>
        </div>
      </div>
      <Show when={rows().length === 0}>
        <p class="t-muted fe-fonts__empty">Ничего не найдено</p>
      </Show>
      <Show when={sys().supported && !sys().shown}>
        <button class="fe-btn fe-btn--soft fe-btn--wide" onClick={() => ed.showSystemFonts()}>
          Показать системные шрифты
        </button>
      </Show>
    </div>
  );
}
````

В `apps/web/src/ui/icons.tsx` заменить:

````tsx
  grid: '<rect width="7" height="7" x="3" y="3" rx="1"/><rect width="7" height="7" x="14" y="3" rx="1"/><rect width="7" height="7" x="14" y="14" rx="1"/><rect width="7" height="7" x="3" y="14" rx="1"/>',
  list: '<path d="M3 5h.01"/><path d="M3 12h.01"/><path d="M3 19h.01"/><path d="M8 5h13"/><path d="M8 12h13"/><path d="M8 19h13"/>',
  keyboard: '<path d="M10 8h.01"/><path d="M12 12h.01"/><path d="M14 8h.01"/><path d="M16 12h.01"/><path d="M18 8h.01"/><path d="M6 8h.01"/><path d="M7 16h10"/><path d="M8 12h.01"/><rect width="20" height="16" x="2" y="4" rx="2"/>',
} as const;
````

на:

````tsx
  grid: '<rect width="7" height="7" x="3" y="3" rx="1"/><rect width="7" height="7" x="14" y="3" rx="1"/><rect width="7" height="7" x="14" y="14" rx="1"/><rect width="7" height="7" x="3" y="14" rx="1"/>',
  list: '<path d="M3 5h.01"/><path d="M3 12h.01"/><path d="M3 19h.01"/><path d="M8 5h13"/><path d="M8 12h13"/><path d="M8 19h13"/>',
  alignLeft: '<path d="M21 5H3"/><path d="M15 12H3"/><path d="M17 19H3"/>',
  alignCenter: '<path d="M21 5H3"/><path d="M17 12H7"/><path d="M19 19H5"/>',
  alignRight: '<path d="M21 5H3"/><path d="M21 12H9"/><path d="M21 19H7"/>',
  alignJustify: '<path d="M3 5h18"/><path d="M3 12h18"/><path d="M3 19h18"/>',
  alignTop: '<path d="M5 3h14"/><path d="m18 13-6-6-6 6"/><path d="M12 7v14"/>',
  alignMiddle: '<path d="M3 12h18"/><path d="M12 3v6"/><path d="M12 15v6"/>',
  alignBottom: '<path d="M12 17V3"/><path d="m6 11 6 6 6-6"/><path d="M19 21H5"/>',
  underline: '<path d="M6 4v6a6 6 0 0 0 12 0V4"/><line x1="4" x2="20" y1="20" y2="20"/>',
  strike: '<path d="M16 4H9a3 3 0 0 0-2.83 4"/><path d="M14 12a4 4 0 0 1 0 8H6"/><line x1="4" x2="20" y1="12" y2="12"/>',
  listOrdered: '<path d="M11 5h10"/><path d="M11 12h10"/><path d="M11 19h10"/><path d="M4 4h1v5"/><path d="M4 9h2"/><path d="M6.5 20H3.4c0-1 2.6-1.925 2.6-3.5a1.5 1.5 0 0 0-2.6-1.02"/>',
  autoWidth: '<path d="m18 8 4 4-4 4"/><path d="M2 12h20"/><path d="m6 8-4 4 4 4"/>',
  autoHeight: '<path d="M12 2v20"/><path d="m8 18 4 4 4-4"/><path d="m8 6 4-4 4 4"/>',
  keyboard: '<path d="M10 8h.01"/><path d="M12 12h.01"/><path d="M14 8h.01"/><path d="M16 12h.01"/><path d="M18 8h.01"/><path d="M6 8h.01"/><path d="M7 16h10"/><path d="M8 12h.01"/><rect width="20" height="16" x="2" y="4" rx="2"/>',
} as const;
````

В `apps/web/src/ui/missing-fonts.tsx` заменить:

````tsx
// (одна правка — один шаг отмены). Список и каталог семейств присылает ядро;
// панель показывает их и шлёт `replaceMissingFont`. Как в Figma — на панели
// «Дизайн» без выделения.

import { createMemo, createSignal, For, Show } from 'solid-js';
````

на:

````tsx
// (одна правка — один шаг отмены). Список и каталог семейств присылает ядро;
// панель показывает их и шлёт `replaceMissingFont`. Как в Figma — на панели
// «Дизайн» без выделения. «Показать системные шрифты» — в выборе шрифта секции
// «Текст» (этап 2).

import { createMemo, createSignal, For, Show } from 'solid-js';
````

В `apps/web/src/ui/missing-fonts.tsx` заменить:

````tsx
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
````

на:

````tsx
  const list = () => ed.store.state.missingFonts;
  const catalog = createMemo(() => [...ed.store.state.fontCatalog].sort((a, b) => a.family.localeCompare(b.family)));
  return (
    <Show when={list().length > 0}>
      <Section title={`Отсутствующие шрифты (${list().length})`}>
        <For each={list()}>{(f) => <MissingRow font={f} catalog={catalog()} />}</For>
      </Section>
    </Show>
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
import { MissingFontsSection } from './missing-fonts';
import { FillsSection, PageSection, StrokesSection } from './paints';

/** Правка; `target` — кого правят с начала жеста (без него — текущее выделение). */
````

на:

````tsx
import { MissingFontsSection } from './missing-fonts';
import { FillsSection, PageSection, StrokesSection } from './paints';
import { TextSection } from './text-section';

/** Правка; `target` — кого правят с начала жеста (без него — текущее выделение). */
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
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
        <StrokesSection />
````

на:

````tsx
        when={count() > 0}
        fallback={
          <Show
            when={ed.store.state.text.editing && ed.store.state.textStyle}
            fallback={
              <>
                <MissingFontsSection />
                <PageSection theme={props.theme} />
              </>
            }
          >
            {/* Новый текст (инструмент T) до первого символа: стиль — заранее. */}
            <TextSection />
          </Show>
        }
      >
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
        <Show when={ed.store.state.textStyle}>
          <TextSection />
        </Show>
        <FillsSection />
        <StrokesSection />
````

В `apps/web/src/ui/styles/properties.css` заменить:

````css
  font: inherit;
}
````

на:

````css
  font: inherit;
}

/* Секция «Текст» (подсистема 3, этап 2): шрифт, переключатели с иконками,
   выбор шрифта с поиском и виртуальным списком. */
.fe-text__family {
  width: 100%;
  margin-bottom: var(--space-2);
}
.fe-text__row {
  margin-top: var(--space-2);
  flex-wrap: wrap;
}
.fe-segmented--icons .fe-segmented__item {
  display: grid;
  place-items: center;
  min-width: 28px;
}
.fe-fonts {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin-bottom: var(--space-2);
}
.fe-fonts__list {
  overflow: auto;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}
.fe-fonts__heading,
.fe-fonts__row {
  position: absolute;
  left: 0;
  right: 0;
  height: 28px;
  padding: 0 var(--space-2);
  display: flex;
  align-items: center;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.fe-fonts__heading {
  font-size: 11px;
}
.fe-fonts__row {
  cursor: default;
  font-size: 13px;
}
.fe-fonts__row:hover,
.fe-fonts__row[aria-selected='true'] {
  background: var(--surface-hover);
}
.fe-fonts__empty {
  margin: 0;
}
````

Создать `apps/web/src/ui/text-panel.ts`:

````ts
// Секция «Текст» панели свойств (спека подсистемы 3, раздел 7) — чистая логика:
// разбор и вид интерлиньяжа и трекинга, группы и поиск выбора шрифта,
// начертание при смене семейства. Сводку стиля готовит ядро.

import type { FontChoice, SpacingDto } from '../wasm/ferrum_wasm';
import { defaultStyle } from '../core/fonts';
import { formatNumber } from './number';

/** Интерлиньяж или трекинг в поле: «Авто», «150%», «20». Пусто или «авто» —
 * авто (только интерлиньяж); число без знака — пиксели. `null` — не разобрать. */
export function parseSpacing(text: string, allowAuto: boolean): SpacingDto | null {
  const t = text.trim().toLowerCase().replace(',', '.');
  if (allowAuto && (t === '' || t === 'авто' || t === 'auto')) return { unit: 'auto', value: 0 };
  const m = /^(-?\d+(?:\.\d+)?)\s*(%|px)?$/.exec(t);
  if (!m) return null;
  const value = Number(m[1]);
  if (!Number.isFinite(value)) return null;
  return { unit: m[2] === '%' ? 'percent' : 'px', value };
}

export function formatSpacing(v: SpacingDto): string {
  switch (v.unit) {
    case 'auto':
      return 'Авто';
    case 'percent':
      return `${formatNumber(v.value)}%`;
    case 'px':
      return formatNumber(v.value);
  }
}

/** Строка выбора шрифта: заголовок группы или семейство. */
export type FontRow =
  | { kind: 'heading'; label: string }
  | { kind: 'font'; family: string; source: 'bundled' | 'system' | 'document' };

/** Группы «В документе» / «Вшитые» / «Системные» с поиском по имени (без регистра). */
export function fontRows(catalog: readonly FontChoice[], pageFonts: readonly string[], query: string): FontRow[] {
  const q = query.trim().toLowerCase();
  const hit = (f: string) => q === '' || f.toLowerCase().includes(q);
  const sorted = [...catalog].sort((a, b) => a.family.localeCompare(b.family));
  const groups: [string, FontRow[]][] = [
    ['В документе', [...pageFonts].sort((a, b) => a.localeCompare(b)).filter(hit).map((family) => ({ kind: 'font', family, source: 'document' }))],
    ['Вшитые', sorted.filter((c) => c.source === 'bundled' && hit(c.family)).map((c) => ({ kind: 'font', family: c.family, source: 'bundled' }))],
    ['Системные', sorted.filter((c) => c.source === 'system' && hit(c.family)).map((c) => ({ kind: 'font', family: c.family, source: 'system' }))],
  ];
  const out: FontRow[] = [];
  for (const [label, rows] of groups) {
    if (rows.length === 0) continue;
    out.push({ kind: 'heading', label }, ...rows);
  }
  return out;
}

/** Начертание при смене семейства: то же имя, иначе Regular, иначе первое. */
export function styleFor(catalog: readonly FontChoice[], family: string, current: string): string {
  return defaultStyle(
    catalog.find((c) => c.family === family),
    current,
  ) || 'Regular';
}

/** Окно строк виртуального списка: первая и сколько (с запасом сверху и снизу). */
export function rowWindow(scrollTop: number, height: number, row: number, total: number): { start: number; end: number } {
  const overscan = 6;
  const start = Math.max(0, Math.floor(scrollTop / row) - overscan);
  return { start, end: Math.min(total, start + Math.ceil(height / row) + 2 * overscan) };
}
````

Создать `apps/web/src/ui/text-section.tsx`:

````tsx
// Секция «Текст» панели свойств (спека подсистемы 3, раздел 7): шрифт (выбор с
// поиском), начертание, размер, интерлиньяж, трекинг, интервал и отступ абзаца,
// выравнивание по горизонтали и вертикали, режим размера, регистр,
// подчёркивание и зачёркивание, списки. Сводку («Смешанные» — у разных значений)
// готовит ядро: в правке текста — по выделенному диапазону, иначе — по всем
// символам выделенных текстов; правка — так же (`setTextStyle`, `setTextProps`).
// Поля чисел — превью до отпускания, одна правка — один шаг.

import { createEffect, createSignal, For, Show } from 'solid-js';
import type { EditPhase, SpacingDto, TextPropsPatchDto, TextStylePatchDto } from '../wasm/ferrum_wasm';
import { Section } from './chrome';
import { useEditor, type Editor } from './context';
import { FontPicker } from './font-picker';
import { Caret, Icon, type IconName } from './icons';
import { captureEdit, NumberField, type EditTarget } from './number-field';
import type { FieldInput } from './number';
import { formatSpacing, parseSpacing, styleFor } from './text-panel';

const MIXED = 'Смешанные';

/** Адресат правки: в правке текста — диапазон ядра (без id), иначе — выделенные на начало. */
function target(ed: Editor, t?: EditTarget): { ids: string[] | undefined } {
  if (ed.store.state.text.editing) return { ids: undefined };
  return { ids: (t ?? captureEdit(ed).target).ids };
}

function sendStyle(ed: Editor, patch: TextStylePatchDto, phase: EditPhase, t?: EditTarget) {
  ed.send({ type: 'setTextStyle', patch, phase, ...target(ed, t) });
}

function sendProps(ed: Editor, patch: TextPropsPatchDto, phase: EditPhase, t?: EditTarget) {
  ed.send({ type: 'setTextProps', patch, phase, ...target(ed, t) });
}

/** Число поля: набранное значение или прибавка к показанному (у «Смешанных»). */
function value(input: FieldInput, shown: number): number {
  return input.kind === 'set' ? input.value : shown + input.delta;
}

/** Поле интерлиньяжа или трекинга: «Авто», «150%», «20» — Enter или уход применяют,
 * Esc возвращает показанное. */
function SpacingField(props: {
  label: string;
  value: SpacingDto;
  mixed: boolean;
  auto: boolean;
  onCommit: (v: SpacingDto) => void;
}) {
  let input!: HTMLInputElement;
  const shown = () => (props.mixed ? '' : formatSpacing(props.value));
  createEffect(() => {
    if (document.activeElement !== input) input.value = shown();
  });
  const commit = () => {
    const v = parseSpacing(input.value, props.auto);
    if (v && (props.mixed || v.unit !== props.value.unit || v.value !== props.value.value)) props.onCommit(v);
    input.value = shown();
  };
  return (
    <label class="fe-field fe-num">
      <span class="fe-field__label">{props.label}</span>
      <input
        ref={input}
        class="fe-input t-num"
        aria-label={props.label === 'LH' ? 'Интерлиньяж' : 'Трекинг'}
        placeholder={props.mixed ? MIXED : ''}
        onFocus={(e) => e.currentTarget.select()}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            commit();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            input.value = shown();
            input.blur();
          }
        }}
      />
    </label>
  );
}

/** Ряд переключателей с одним выбором (выравнивание, режим размера). */
function Choice<T extends string>(props: {
  label: string;
  value: T;
  mixed: boolean;
  items: { value: T; name: string; icon: IconName }[];
  onPick: (v: T) => void;
}) {
  return (
    <div class="fe-segmented fe-segmented--icons" role="radiogroup" aria-label={props.label}>
      <For each={props.items}>
        {(it) => (
          <button
            role="radio"
            class="fe-segmented__item"
            aria-checked={!props.mixed && props.value === it.value}
            aria-label={it.name}
            data-tip={it.name}
            onClick={() => props.onPick(it.value)}
          >
            <Icon name={it.icon} />
          </button>
        )}
      </For>
    </div>
  );
}

export function TextSection() {
  const ed = useEditor();
  const s = () => ed.store.state.textStyle!;
  const [picking, setPicking] = createSignal(false);
  const styles = () => ed.store.state.fontCatalog.find((c) => c.family === s().family.value)?.styles ?? [];
  const pickFamily = (family: string) => {
    setPicking(false);
    sendStyle(ed, { family, style: styleFor(ed.store.state.fontCatalog, family, s().style.value) }, 'commit');
  };
  const toggle = (key: 'decoration' | 'list', on: string, off: string) => {
    const cur = s()[key];
    const next = !cur.mixed && cur.value === on ? off : on;
    sendStyle(ed, { [key]: next } as TextStylePatchDto, 'commit');
  };
  const pressed = (key: 'decoration' | 'list', v: string) => !s()[key].mixed && s()[key].value === v;
  return (
    <Section title="Текст">
      <button
        class="fe-select fe-text__family"
        aria-haspopup="listbox"
        aria-expanded={picking()}
        aria-label="Шрифт"
        onClick={() => setPicking((p) => !p)}
      >
        <span class="fe-ellipsis" classList={{ 't-muted': s().family.mixed }}>
          {s().family.mixed ? MIXED : s().family.value}
        </span>
        <Caret />
      </button>
      <Show when={picking()}>
        <FontPicker current={s().family.value} onPick={pickFamily} onClose={() => setPicking(false)} />
      </Show>
      <div class="fe-grid2">
        <select
          class="fe-select"
          aria-label="Начертание"
          value={s().style.mixed ? '' : s().style.value}
          onChange={(e) =>
            !s().family.mixed && sendStyle(ed, { family: s().family.value, style: e.currentTarget.value }, 'commit')
          }
        >
          <Show when={s().style.mixed}>
            <option value="" disabled>
              {MIXED}
            </option>
          </Show>
          <For each={styles()}>{(st) => <option value={st}>{st}</option>}</For>
        </select>
        <NumberField
          label="Aa"
          name="Размер шрифта"
          value={s().size.value}
          mixed={s().size.mixed}
          min={1}
          max={10000}
          onEdit={(input, phase, t) => sendStyle(ed, { size: value(input, s().size.value) }, phase, t)}
        />
        <SpacingField
          label="LH"
          value={s().lineHeight.value}
          mixed={s().lineHeight.mixed}
          auto
          onCommit={(v) => sendStyle(ed, { lineHeight: v }, 'commit')}
        />
        <SpacingField
          label="LS"
          value={s().letterSpacing.value}
          mixed={s().letterSpacing.mixed}
          auto={false}
          onCommit={(v) => sendStyle(ed, { letterSpacing: v }, 'commit')}
        />
        <NumberField
          label="¶"
          name="Интервал абзацев"
          value={s().paragraphSpacing.value}
          mixed={s().paragraphSpacing.mixed}
          min={0}
          onEdit={(input, phase, t) => sendProps(ed, { paragraphSpacing: value(input, s().paragraphSpacing.value) }, phase, t)}
        />
        <NumberField
          label="⇥"
          name="Отступ абзаца"
          value={s().paragraphIndent.value}
          mixed={s().paragraphIndent.mixed}
          onEdit={(input, phase, t) => sendProps(ed, { paragraphIndent: value(input, s().paragraphIndent.value) }, phase, t)}
        />
      </div>
      <div class="fe-row fe-text__row">
        <Choice
          label="Выравнивание по горизонтали"
          value={s().alignH.value}
          mixed={s().alignH.mixed}
          items={[
            { value: 'left', name: 'По левому краю', icon: 'alignLeft' },
            { value: 'center', name: 'По центру', icon: 'alignCenter' },
            { value: 'right', name: 'По правому краю', icon: 'alignRight' },
            { value: 'justify', name: 'По ширине', icon: 'alignJustify' },
          ]}
          onPick={(v) => sendProps(ed, { alignH: v as TextPropsPatchDto['alignH'] }, 'commit')}
        />
        <Choice
          label="Выравнивание по вертикали"
          value={s().alignV.value}
          mixed={s().alignV.mixed}
          items={[
            { value: 'top', name: 'По верху', icon: 'alignTop' },
            { value: 'middle', name: 'По середине', icon: 'alignMiddle' },
            { value: 'bottom', name: 'По низу', icon: 'alignBottom' },
          ]}
          onPick={(v) => sendProps(ed, { alignV: v as TextPropsPatchDto['alignV'] }, 'commit')}
        />
      </div>
      <div class="fe-row fe-text__row">
        <Choice
          label="Размер рамки"
          value={s().sizing.value}
          mixed={s().sizing.mixed}
          items={[
            { value: 'autoWidth', name: 'Авто по ширине', icon: 'autoWidth' },
            { value: 'autoHeight', name: 'Авто по высоте', icon: 'autoHeight' },
            { value: 'fixed', name: 'Фиксированный размер', icon: 'rect' },
          ]}
          onPick={(v) => sendProps(ed, { sizing: v as TextPropsPatchDto['sizing'] }, 'commit')}
        />
        <select
          class="fe-select fe-row__grow"
          aria-label="Регистр"
          value={s().case.mixed ? '' : s().case.value}
          onChange={(e) => sendStyle(ed, { case: e.currentTarget.value as TextStylePatchDto['case'] }, 'commit')}
        >
          <Show when={s().case.mixed}>
            <option value="" disabled>
              {MIXED}
            </option>
          </Show>
          <option value="none">Как есть</option>
          <option value="upper">ВЕРХНИЙ</option>
          <option value="lower">нижний</option>
          <option value="title">Каждое Слово</option>
          <option value="smallCaps">Капитель</option>
        </select>
      </div>
      <div class="fe-row fe-text__row">
        <button
          class="fe-tool fe-tool--small"
          aria-pressed={pressed('decoration', 'underline')}
          aria-label="Подчёркнутый"
          data-tip="Подчёркнутый"
          onClick={() => toggle('decoration', 'underline', 'none')}
        >
          <Icon name="underline" />
        </button>
        <button
          class="fe-tool fe-tool--small"
          aria-pressed={pressed('decoration', 'strike')}
          aria-label="Зачёркнутый"
          data-tip="Зачёркнутый"
          onClick={() => toggle('decoration', 'strike', 'none')}
        >
          <Icon name="strike" />
        </button>
        <span class="fe-toolbar__sep" role="separator" />
        <button
          class="fe-tool fe-tool--small"
          aria-pressed={pressed('list', 'bullet')}
          aria-label="Маркированный список"
          data-tip="Маркированный список"
          onClick={() => toggle('list', 'bullet', 'none')}
        >
          <Icon name="list" />
        </button>
        <button
          class="fe-tool fe-tool--small"
          aria-pressed={pressed('list', 'ordered')}
          aria-label="Нумерованный список"
          data-tip="Нумерованный список"
          onClick={() => toggle('list', 'ordered', 'none')}
        >
          <Icon name="listOrdered" />
        </button>
      </div>
    </Section>
  );
}
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npx playwright test e2e/text-panel.spec.ts e2e/text.spec.ts e2e/properties.spec.ts --project=chromium`

Expected: модульные — 128 passed; E2E `text-panel` (4), `text` (4), `properties` — passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/text-panel.spec.ts apps/web/e2e/text.spec.ts apps/web/src/ui/font-picker.tsx apps/web/src/ui/icons.tsx apps/web/src/ui/missing-fonts.tsx apps/web/src/ui/properties.tsx apps/web/src/ui/styles/properties.css apps/web/src/ui/text-panel.ts apps/web/src/ui/text-section.tsx apps/web/unit/text-panel.test.ts
git commit -m "feat(web): секция «Текст» — выбор шрифта с поиском, группами и виртуальным списком, начертание, размер, интерлиньяж, трекинг, абзацы, выравнивание, режим размера, регистр, украшения, списки; «Показать системные шрифты» — в выборе шрифта

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 12: Заливки и обводки текста

Спека 6 (этап 2 вместе с секцией «Текст»): заливки узла по порядку (изображение — по рамке узла), заливка участка из разметки любого вида, обводки по контуру глифов. Раскладка запоминает краску участка (`GlyphRun::paint`), рисование — `paint_glyphs` со свойствами узла и картинками; незагруженная картинка — заглушка и неполный кадр.

**Files:**
- Modify: `crates/ferrum-text/src/layout.rs`, `crates/ferrum-render/src/text_node.rs`, `draw.rs`
- Test: `crates/ferrum-render/tests/text.rs`, эталон `tests/golden/text_paints.png`

**Interfaces:**
- Produces: `GlyphRun::paint: Option<Fill>`; `text_node::paint_glyphs(backend, fonts, layout, props, to_device, alpha, images) -> bool`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-render/tests/text.rs` заменить:

````rust

use std::cell::Cell;
use std::path::PathBuf;

use common::*;
````

на:

````rust

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use common::*;
````

В `crates/ferrum-render/tests/text.rs` заменить:

````rust
    assert!(!second.needs_another_frame, "{second:?}");
}
````

на:

````rust
    assert!(!second.needs_another_frame, "{second:?}");
}

/// Картинка 16×8 из полос: красная слева, синяя справа.
fn two_tone() -> Arc<ferrum_render::Pixmap> {
    let mut img = ferrum_render::Pixmap::new(16, 8);
    for y in 0..8 {
        for x in 0..16u16 {
            let (r, b) = if x < 8 { (255, 0) } else { (0, 255) };
            img.set_pixel(x, y, vello_cpu::color::PremulRgba8 { r, g: 0, b, a: 255 });
        }
    }
    Arc::new(img)
}

#[test]
fn image_fill_and_strokes_paint_glyphs() {
    let fonts = fonts(true);
    let mut p = Page::new();
    let text = TextContent::plain("IIII");
    let bold = props(48.0, TextSizing::Fixed);
    let mut bold = bold;
    bold.style.font = FontName::new("Inter", "Black");
    // Заливка изображением по рамке узла: слева красное, справа синее.
    let a = add_text(&mut p, 10.0, 10.0, 100.0, 60.0, text.clone(), bold.clone());
    p.edit(
        a,
        PropsPatch {
            fills: Some(vec![Fill::image("two-tone")]),
            ..Default::default()
        },
    );
    // Без заливки, с обводкой по центру зелёного цвета в 3 px.
    let b = add_text(&mut p, 130.0, 10.0, 100.0, 60.0, text, bold);
    p.edit(
        b,
        PropsPatch {
            fills: Some(vec![]),
            strokes: Some(vec![Stroke {
                color: Color::rgba(0, 160, 0, 255),
                width: 3.0,
                align: StrokeAlign::Center,
            }]),
            ..Default::default()
        },
    );
    let images: HashMap<String, Arc<ferrum_render::Pixmap>> =
        [("two-tone".to_string(), two_tone())].into();
    // Без часов: оба текста раскладываются в этом же кадре при любой нагрузке.
    let mut r = renderer(&fonts);
    r.set_clock(None);
    let img = render_with(&mut r, &p.scene(), &camera(240.0, 80.0), &images);
    let count = |x0: u16, x1: u16, f: &dyn Fn([u8; 4]) -> bool| {
        (x0..x1)
            .flat_map(|x| (10..70).map(move |y| (x, y)))
            .filter(|&(x, y)| f(px(&img, x, y)))
            .count()
    };
    let red = |c: [u8; 4]| c[0] > 200 && c[2] < 60;
    let blue = |c: [u8; 4]| c[2] > 200 && c[0] < 60;
    let green = |c: [u8; 4]| c[1] > 120 && c[0] < 60 && c[2] < 60;
    // Глифы левой половины рамки — красные, правой — синие; между глифами — фон.
    assert!(
        count(10, 60, &red) > 100,
        "красных: {}",
        count(10, 60, &red)
    );
    assert!(
        count(60, 110, &blue) > 20,
        "синих: {}",
        count(60, 110, &blue)
    );
    assert_eq!(count(10, 60, &blue), 0);
    // Обводка без заливки: зелёный контур, середина буквы — фон.
    assert!(
        count(130, 230, &green) > 100,
        "зелёных: {}",
        count(130, 230, &green)
    );
    golden("text_paints", &img);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-render --test text image_fill_and_strokes`
Expected: FAIL — тест падает: глифы залиты чёрным (заливка изображением и обводки текста не рисуются) — `красных: 0`

- [ ] **Step 3: Реализация**

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
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
````

на:

````rust
            self.stats.text_greeked += 1;
        } else {
            let ready = text_node::paint_glyphs(
                backend,
                &mut ctx.fonts.borrow_mut(),
                &layout,
                &n.props,
                to_device,
                alpha,
                ctx.images,
            );
            self.missing_image |= !ready;
            self.stats.text_glyphs += 1;
        }
````

В `crates/ferrum-render/src/text_node.rs` заменить:

````rust
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
````

на:

````rust
//! контуров `skrifa` через трейт бэкенда (как подписи оверлеев), украшения —
//! прямоугольники, мелкий текст — полосы по строкам (LOD, как Figma при отдалении).
//! Этап 2: заливки узла по порядку (цвет и изображение по рамке узла), заливка
//! участка из разметки, обводки текста — по контуру глифов (снаружи — под
//! заливкой, по центру и внутри — над ней, внутри — с обрезкой по глифам).

use std::time::Duration;

use ferrum_doc::{Fill as Paint, FillKind, Props, StrokeAlign, TextSizing};
use ferrum_text::{text_color, Fonts, GlyphRun, LineBox, TextLayout};
use kurbo::{Affine, BezPath, Rect, Stroke};
use vello_cpu::peniko::Fill;

use crate::backend::{ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::display::{color, IMAGE_PLACEHOLDER};
use crate::draw::place_image;
use crate::image::ImageProvider;

/// Строка ниже стольких пикселей устройства рисуется полосой.
````

В `crates/ferrum-render/src/text_node.rs` заменить:

````rust
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
````

на:

````rust
}

/// Матрица глифа: позиция на базовой линии, синтетический наклон, масштаб из
/// единиц шрифта (ось y шрифта — вверх).
fn glyph_transform(fonts: &mut Fonts, run: &GlyphRun, x: f32, y: f32) -> Affine {
    let k = run.size as f64 / fonts.units_per_em(&run.font);
    // Синтетический курсив: наклон контура вправо на угол `skew`.
    let skew = run.skew.map_or(Affine::IDENTITY, |deg| {
        Affine::skew(-(deg as f64).to_radians().tan(), 0.0)
    });
    Affine::translate((x as f64, y as f64)) * skew * Affine::scale_non_uniform(k, -k)
}

/// Контур глифов прогонов одним путём в координатах узла (заливка изображением
/// и обводка).
fn glyphs_path<'a>(fonts: &mut Fonts, runs: impl Iterator<Item = &'a GlyphRun>) -> BezPath {
    let mut out = BezPath::new();
    for run in runs {
        for g in &run.glyphs {
            let path = fonts.outline(&run.font, g.id, &run.coords);
            let t = glyph_transform(fonts, run, g.x, g.y);
            out.extend(path.elements().iter().map(|e| t * *e));
        }
    }
    out
}

/// Заливка глифов прогона краской `paint`. Изображение — по рамке узла `area`;
/// `false` — картинки ещё нет (нарисована заглушка).
#[allow(clippy::too_many_arguments)]
fn fill_run(
    backend: &mut impl RenderBackend,
    fonts: &mut Fonts,
    run: &GlyphRun,
    paint: &Paint,
    area: Rect,
    to_device: Affine,
    alpha: f32,
    images: &dyn ImageProvider,
) -> bool {
    let a = alpha * paint.opacity.clamp(0.0, 1.0) as f32;
    let image = match (paint.kind, &paint.image_hash) {
        (FillKind::Image, Some(h)) => Some(h),
        (FillKind::Image, None) => return true,
        (FillKind::Solid, _) => None,
    };
    let Some(hash) = image else {
        backend.set_color(color(paint.color).multiply_alpha(a));
        for g in &run.glyphs {
            let path = fonts.outline(&run.font, g.id, &run.coords);
````

В `crates/ferrum-render/src/text_node.rs` заменить:

````rust
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
````

на:

````rust
                continue;
            }
            backend.set_transform(to_device * glyph_transform(fonts, run, g.x, g.y));
            backend.fill_path(&path, Fill::NonZero);
        }
        return true;
    };
    let path = glyphs_path(fonts, std::iter::once(run));
    backend.set_transform(to_device);
    if !(area.width() > 0.0 && area.height() > 0.0) {
        return true;
    }
    let Some(pixmap) = images.image(hash) else {
        backend.set_color(IMAGE_PLACEHOLDER.multiply_alpha(a));
        backend.fill_path(&path, Fill::NonZero);
        return false;
    };
    let (iw, ih) = (pixmap.width() as f64, pixmap.height() as f64);
    let (transform, extend, _) = place_image(paint.scale_mode, area, iw, ih);
    backend.set_image(&ImagePaint {
        key: ImageKey::for_hash(hash),
        pixmap,
        transform,
        extend,
        alpha: a,
    });
    backend.fill_path(&path, Fill::NonZero);
    true
}

/// Глифы, украшения и обводки раскладки текста узла `props`. `to_device` —
/// локальные координаты узла → пиксели устройства. `false` — какая-то картинка
/// заливки ещё не готова (кадр неполный).
pub(crate) fn paint_glyphs(
    backend: &mut impl RenderBackend,
    fonts: &mut Fonts,
    layout: &TextLayout,
    props: &Props,
    to_device: Affine,
    alpha: f32,
    images: &dyn ImageProvider,
) -> bool {
    let area = Rect::new(0.0, 0.0, props.w, props.h);
    let fills: Vec<&Paint> = props
        .fills
        .iter()
        .filter(|f| f.visible && f.opacity > 0.0)
        .collect();
    let strokes: Vec<_> = props
        .strokes
        .iter()
        .filter(|s| s.width > 0.0 && s.color.a > 0)
        .collect();
    let outline = (!strokes.is_empty()).then(|| glyphs_path(fonts, layout.runs.iter()));
    let stroke = |backend: &mut _, outside: bool| {
        let Some(path) = &outline else { return };
        for s in &strokes {
            if (s.align == StrokeAlign::Outside) != outside {
                continue;
            }
            backend_stroke(backend, path, s, to_device, alpha);
        }
    };
    // Снаружи — под заливкой: заливка закрывает внутреннюю половину.
    stroke(backend, true);
    let mut ready = true;
    for run in &layout.runs {
        match &run.paint {
            Some(p) => ready &= fill_run(backend, fonts, run, p, area, to_device, alpha, images),
            None => {
                for f in &fills {
                    ready &= fill_run(backend, fonts, run, f, area, to_device, alpha, images);
                }
            }
        }
    }
````

В `crates/ferrum-render/src/text_node.rs` заменить:

````rust
        backend.set_color(color(d.color).multiply_alpha(alpha));
        backend.fill_rect(&d.rect);
    }
}
````

на:

````rust
        backend.set_color(color(d.color).multiply_alpha(alpha));
        backend.fill_rect(&d.rect);
    }
    stroke(backend, false);
    ready
}

/// Обводка контура глифов: по центру — как есть; внутри — двойной толщины с
/// обрезкой по глифам; снаружи — двойной толщины (рисуется под заливкой).
fn backend_stroke(
    backend: &mut impl RenderBackend,
    path: &BezPath,
    s: &ferrum_doc::Stroke,
    to_device: Affine,
    alpha: f32,
) {
    backend.set_transform(to_device);
    backend.set_color(color(s.color).multiply_alpha(alpha));
    match s.align {
        StrokeAlign::Center => backend.stroke_path(path, &Stroke::new(s.width)),
        StrokeAlign::Outside => backend.stroke_path(path, &Stroke::new(2.0 * s.width)),
        StrokeAlign::Inside => {
            backend.push_clip(path);
            backend.stroke_path(path, &Stroke::new(2.0 * s.width));
            backend.pop_clip();
        }
    }
}
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
    pub coords: Vec<i16>,
    pub color: Color,
    /// Синтетический наклон (курсив без своего файла), градусы.
    pub skew: Option<f32>,
````

на:

````rust
    pub coords: Vec<i16>,
    pub color: Color,
    /// Заливка участка (разметка `fill`): цвет или изображение; `None` — заливки
    /// узла (спека 6: заливки диапазонов — этап 2).
    pub paint: Option<Fill>,
    /// Синтетический наклон (курсив без своего файла), градусы.
    pub skew: Option<f32>,
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
    case: TextCase,
    color: Color,
}

````

на:

````rust
    case: TextCase,
    color: Color,
    /// Заливка участка из разметки (любого вида).
    paint: Option<Fill>,
}

````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
                _ => default_color,
            },
        }
    }
````

на:

````rust
                _ => default_color,
            },
            paint: a.fill.clone(),
        }
    }
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
            case: TextCase::None,
            color: like.color,
        };
        let text = "…";
````

на:

````rust
            case: TextCase::None,
            color: like.color,
            paint: like.paint.clone(),
        };
        let text = "…";
````

В `crates/ferrum-text/src/layout.rs` заменить:

````rust
                    .collect(),
                color,
                skew: synthesis.skew(),
                glyphs,
````

на:

````rust
                    .collect(),
                color,
                paint: span.paint.clone(),
                skew: synthesis.skew(),
                glyphs,
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `export CARGO_TARGET_DIR=$PWD/target && FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test text image_fill_and_strokes`
Expected: создан эталон `crates/ferrum-render/tests/golden/text_paints.png` (красно-синие буквы изображением и зелёный контур)

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-text -p ferrum-render 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `text` рендера — 8 тестов (с `image_fill_and_strokes_paint_glyphs`), эталоны прежние совпадают

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-render/src/draw.rs crates/ferrum-render/src/text_node.rs crates/ferrum-render/tests/text.rs crates/ferrum-text/src/layout.rs crates/ferrum-render/tests/golden/text_paints.png
git commit -m "feat(render): заливки и обводки текста — заливки узла по порядку и изображение по рамке, заливка участка из разметки, обводки по контуру глифов (снаружи, по центру, внутри); эталон text_paints

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 13: Замер ввода, спека и финальная проверка

Замер `editor_report`: символ в середину текста из 10 000 символов «авто по высоте» (правка, раскладка для рамки и курсора — одна транзакция) и ход курсора стрелкой; цели — 16 мс и 1 мс нативно (в wasm ожидается вдвое медленнее). Спека получает решения этапа 2.

**Files:**
- Modify: `crates/ferrum-editor/examples/editor_report.rs`, `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md`

**Interfaces:**
- Consumes: всё этапа 2.

- [ ] **Step 1: Реализация**

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust

use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, NodeId, PropsPatch};
use ferrum_editor::snap::SNAP_THRESHOLD_PX as SNAP_PX;
use ferrum_editor::{CommandId, Editor, Input, KeyEvent, Modifiers, Platform, PointerEvent};
````

на:

````rust

use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, NodeId, PropsPatch, TextContent, TextProps, TextSizing};
use ferrum_editor::snap::SNAP_THRESHOLD_PX as SNAP_PX;
use ferrum_editor::{CommandId, Editor, Input, KeyEvent, Modifiers, Platform, PointerEvent};
````

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
    );

    println!("| Метрика | Значение | Цель | Итог |");
    println!("|---|---|---|---|");
````

на:

````rust
    );

    typing(&mut report);

    println!("| Метрика | Значение | Цель | Итог |");
    println!("|---|---|---|---|");
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

/// Шрифт вшитого Inter для замера ввода (файл сборки интерфейса).
fn inter() -> ferrum_text::SharedFonts {
    let rel = "inter/Inter-Variable.ttf";
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    let bytes = std::fs::read(&path).expect("вшитый Inter");
    let fonts = ferrum_text::Fonts::shared();
    {
        let mut f = fonts.borrow_mut();
        f.add_catalog(
            ferrum_text::SourceKind::Bundled,
            ferrum_text::group(ferrum_text::read_faces(&bytes, rel)),
        );
        f.load("Inter", rel, bytes).unwrap();
        f.take_requests();
    }
    fonts
}

/// Ввод в текст из 10 000 символов (подсистема 3, этап 2): символ в середину
/// абзацного текста «авто по высоте» — правка, раскладка для размера рамки и
/// курсора одной транзакцией; ход курсора стрелкой.
fn typing(report: &mut Report) {
    let mut ed =
        Editor::new(Document::new(), None, VIEWPORT, 1.0, Platform::Mac).expect("страница");
    ed.set_fonts(inter());
    let words = "Съешь же ещё этих мягких французских булок, да выпей чаю. ";
    let text: String = words.chars().cycle().take(10_000).collect();
    let mut props = TextProps {
        sizing: TextSizing::AutoHeight,
        ..Default::default()
    };
    props.style.size = 14.0;
    let id = ed
        .create_text(
            Point::new(0.0, 0.0),
            &TextContent::plain(&text),
            &props,
            600.0,
            0.0,
        )
        .expect("текст");
    ed.start_text_edit(id, ferrum_editor::EnterAt::All);
    ed.text_key("ArrowLeft", Modifiers::NONE);
    for _ in 0..5000 {
        ed.text_key("ArrowRight", Modifiers::NONE);
    }
    let mut chars = Vec::new();
    let mut now = 10_000.0;
    for c in "Ферум".chars().cycle().take(40) {
        now += 50.0;
        ed.set_time(now);
        let t = Instant::now();
        ed.text_input(ferrum_editor::TextInputKind::Insert, &c.to_string())
            .expect("ввод");
        chars.push(t.elapsed());
    }
    let mut moves = Vec::new();
    for _ in 0..40 {
        let t = Instant::now();
        ed.text_key("ArrowLeft", Modifiers::NONE);
        moves.push(t.elapsed());
    }
    ed.take_events();
    report.gate_median(
        "Символ в текст из 10 000 символов: правка и размер рамки",
        &chars,
        16.0,
    );
    report.gate_median(
        "Ход курсора стрелкой в тексте из 10 000 символов",
        &moves,
        1.0,
    );
    report.check(
        "Ввод подряд — один шаг отмены",
        ed.step_count() == 2,
        format!("{}", ed.step_count()),
        "2",
    );
}
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **Списки** — атрибут абзаца (`list`: `none` | `bullet` | `ordered`, `indent` 0–5) разметкой на символе перевода строки абзаца, как в Quill/Figma.
- **История отмены** (ядро §5.5) записывает правку текста дельтой Loro и откатывает обратной дельтой — её строит модель перед применением правки из текущего текста (`slice_delta` удаляемого и переразмечаемого), поэтому шаг переживает пересоздание узла отменой удаления под новым `TreeID`; ввод подряд в одном месте — один шаг (как в Figma: шаг рвётся паузой > 1 с, сменой позиции курсора или стиля): `Document::transact_merging` с меткой `TextMerge`.
- **Пределы:** текст узла ≤ 100 000 символов, диапазонов разметки ≤ 10 000; превышение — отказ правки с уведомлением.
- **Журнал** (ядро §10): правки текста — обычные обновления Loro, журнал их пишет без изменений.
- **Сделано (этап 1):** текст узла всегда кончается переводом строки (на нём — разметка последнего абзаца, как в Quill; раскладка его не рисует, удалить его и вставить после него нельзя); позиции — символы Unicode (так считает `LoroText` без фичи `wasm` у Loro; перевод из UTF-16 поля ввода — этап 2). Цвет текста по умолчанию — список `fills` узла (как «Заливка» текстового слоя в Figma), разметка `fill` — заливка участка; остальные умолчания стиля и свойства текста пишутся ключами карты узла все. Расширение разметки: символьная — `After`, абзацная (`list`, `indent`) — `None`; настройку ставит каждый `LoroDoc` страницы. Кэш сцены — версия 3 (строка текста несёт свойства и содержимое). Узел текста копируется в буфер обмена с содержимым (поле `text` с JSON текста; версия формата прежняя). Совместимость со сборкой без текста проверяет `scripts/compat-old-build.sh`.
````

на:

````markdown
- **Списки** — атрибут абзаца (`list`: `none` | `bullet` | `ordered`, `indent` 0–5) разметкой на символе перевода строки абзаца, как в Quill/Figma.
- **История отмены** (ядро §5.5) записывает правку текста дельтой Loro и откатывает обратной дельтой — её строит модель перед применением правки из текущего текста (`slice_delta` удаляемого и переразмечаемого), поэтому шаг переживает пересоздание узла отменой удаления под новым `TreeID`; ввод подряд в одном месте — один шаг (как в Figma: шаг рвётся паузой > 1 с, сменой позиции курсора или стиля): `Document::transact_merging` с меткой `TextMerge`.
- **Пределы:** текст узла ≤ 100 000 символов, диапазонов разметки ≤ 10 000; превышение — отказ правки с уведомлением. Текст сверх пределов в файле (чужая сборка) открывает документ только для чтения (этап 2).
- **Журнал** (ядро §10): правки текста — обычные обновления Loro, журнал их пишет без изменений.
- **Сделано (этап 1):** текст узла всегда кончается переводом строки (на нём — разметка последнего абзаца, как в Quill; раскладка его не рисует, удалить его и вставить после него нельзя); позиции — символы Unicode (так считает `LoroText` без фичи `wasm` у Loro; перевод из UTF-16 поля ввода — этап 2). Цвет текста по умолчанию — список `fills` узла (как «Заливка» текстового слоя в Figma), разметка `fill` — заливка участка; остальные умолчания стиля и свойства текста пишутся ключами карты узла все. Расширение разметки: символьная — `After`, абзацная (`list`, `indent`) — `None`; настройку ставит каждый `LoroDoc` страницы. Кэш сцены — версия 3 (строка текста несёт свойства и содержимое). Узел текста копируется в буфер обмена с содержимым (поле `text` с JSON текста; версия формата прежняя). Совместимость со сборкой без текста проверяет `scripts/compat-old-build.sh`.
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **Ленивая раскладка:** сцена (ядро §6) знает границы текста из документа, поэтому открытие страницы не раскладывает тексты; раскладка считается при первом рисовании узла на экране и при правке. Ориентир: 10 000 текстовых слоёв на странице открываются быстрее 2 с, панорамирование по ним — 60 fps (замер в плане этапа 1).
- **Сделано (этап 1):** раскладка — по абзацам (свой `parley::Layout` на абзац: отступ списка — 1,5 кегля на уровень, маркеры «•» и «N.», отступ первой строки, интервал), длинное слово переносится по буквам; кэш раскладок — у рисовальщика по слоту (`version` узла и поколение реестра), раскладки за кадр — в пределах 4 мс, остальное — полосами по оценке строк. Замер (`render_report`, нативно, release, M1 Pro): открытие файла из 10 000 текстов до сцены ≈ 45 мс, первый кадр ≈ 4,5 мс, p95 панорамирования ≈ 3–4 мс; в браузере (`@perf`, vello_cpu): до первого кадра ≈ 0,5 с, p95 кадра ≈ 12 мс. Ресайз рамки текста на этапе 1 переводит его в `fixed` (правила Figma для горизонтального ресайза — этап 2).
- **Попадание** (hit-testing, ядро §7.4) по тексту — по прямоугольнику узла; позиция курсора по точке — по раскладке (этап 2).

## 6. Отрисовка
````

на:

````markdown
- **Ленивая раскладка:** сцена (ядро §6) знает границы текста из документа, поэтому открытие страницы не раскладывает тексты; раскладка считается при первом рисовании узла на экране и при правке. Ориентир: 10 000 текстовых слоёв на странице открываются быстрее 2 с, панорамирование по ним — 60 fps (замер в плане этапа 1).
- **Сделано (этап 1):** раскладка — по абзацам (свой `parley::Layout` на абзац: отступ списка — 1,5 кегля на уровень, маркеры «•» и «N.», отступ первой строки, интервал), длинное слово переносится по буквам; кэш раскладок — у рисовальщика по слоту (`version` узла и поколение реестра), раскладки за кадр — в пределах 4 мс, остальное — полосами по оценке строк. Замер (`render_report`, нативно, release, M1 Pro): открытие файла из 10 000 текстов до сцены ≈ 45 мс, первый кадр ≈ 4,5 мс, p95 панорамирования ≈ 3–4 мс; в браузере (`@perf`, vello_cpu): до первого кадра ≈ 0,5 с, p95 кадра ≈ 12 мс. Ресайз рамки текста на этапе 1 переводит его в `fixed` (правила Figma для горизонтального ресайза — этап 2).
- **Попадание** (hit-testing, ядро §7.4) по тексту — по прямоугольнику узла; позиция курсора по точке — по раскладке (этап 2: `TextLayout::hit`, строки правки `carets` — и у обрезанного текста; NaN и бесконечность из файла раскладка заменяет умолчаниями).

## 6. Отрисовка
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- Подчёркивание и зачёркивание — по метрикам шрифта; заливки диапазонов — цвет, градиент и изображение не входят в этап 1 (только цвет; остальные — этап 2 вместе с секцией «Текст»).
- Экспорт PNG, превью файлов (ядро §10) и буфер обмена PNG включают текст.

## 7. Инструмент «Текст» и правка (этап 2)
````

на:

````markdown
- Подчёркивание и зачёркивание — по метрикам шрифта; заливки диапазонов — цвет, градиент и изображение не входят в этап 1 (только цвет; остальные — этап 2 вместе с секцией «Текст»).
- Экспорт PNG, превью файлов (ядро §10) и буфер обмена PNG включают текст.
- **Сделано (этап 2):** заливки узла рисуются по порядку (изображение — по рамке узла, режим заполнения как у фигур), заливка участка из разметки заменяет их на участке; обводки — по контуру глифов: снаружи — под заливкой двойной толщины, по центру — над ней, внутри — двойной толщины с обрезкой по глифам. Узел текста без видимых заливок глифов не заливает (как в Figma). Градиентов в модели нет.

## 7. Инструмент «Текст» и правка (этап 2)
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- **Секция «Текст» в свойствах:** выбор шрифта (поиск, предпросмотр названием в своём шрифте, группы «Вшитые» / «Системные» / «В документе», виртуальный список), начертание, размер, интерлиньяж, трекинг, отступ абзаца, выравнивание (по горизонтали и вертикали), режим размера, регистр, подчёркивание и зачёркивание, списки; смешанные значения при выделении нескольких диапазонов или узлов показываются как «Смешанное» (ядро §9). Правка из панели при выделенном диапазоне меняет только его, при выделенном узле — весь текст. Одна правка — один шаг отмены, превью до отпускания (как в этапе 5 ядра).
- Инварианты интерфейса этапа 5 ядра действуют: поле ввода не переписывается событиями ядра во время правки, клавиши не утекают на холст из панелей, модальные окна держат клавиатуру.

## 8. Протокол (дополнения к ядру §4.3)
````

на:

````markdown
- **Секция «Текст» в свойствах:** выбор шрифта (поиск, предпросмотр названием в своём шрифте, группы «Вшитые» / «Системные» / «В документе», виртуальный список), начертание, размер, интерлиньяж, трекинг, отступ абзаца, выравнивание (по горизонтали и вертикали), режим размера, регистр, подчёркивание и зачёркивание, списки; смешанные значения при выделении нескольких диапазонов или узлов показываются как «Смешанное» (ядро §9). Правка из панели при выделенном диапазоне меняет только его, при выделенном узле — весь текст. Одна правка — один шаг отмены, превью до отпускания (как в этапе 5 ядра).
- Инварианты интерфейса этапа 5 ядра действуют: поле ввода не переписывается событиями ядра во время правки, клавиши не утекают на холст из панелей, модальные окна держат клавиатуру.
- **Сделано (этап 2):** новый текст инструментом T — черновик до первого символа (в документ и слои не попадает; создание и набор — один шаг, пустой — исчезает без шага); опустевший текст удаляется при выходе склейкой с правкой, что его опустошила. Двойной щелчок ставит курсор в точку (и над ручками рамки мелкого текста), Enter — выделяет весь текст. Позиции курсора — границы кластеров раскладки (`CaretLine` в `ferrum-text`), позиция на переносе строки — с признаком верхней строки (`upstream`). Слово — буквы, цифры и `_`. Набор берёт стиль символа слева (в начале абзаца — справа), поверх выделения — стиль первого выделенного; Backspace в начале пункта списка снимает список. Шаг ввода рвут пауза > 1 с, другая позиция, другой вид правки (набор, удаление вперёд или назад, абзац) и стиль. Ctrl/Cmd+B, I, U, Shift+Ctrl/Cmd+X у курсора без выделения задают стиль следующего ввода; Tab и Shift+Tab — уровень списка (вне списка — ничего). Ввод из кольца событий доходит до ядра раньше команды текста (основной поток досылает его, worker вычитывает кольцо). Обычный текст длиннее 64 КБ не вставляется (сообщение). В Firefox копия фрагмента — только обычный текст (своего формата асинхронный буфер там не умеет). Фрагмент вне правки вставляется текстом «авто по ширине». Ресайз текста по правилам Figma: у «авто» только ширина — «авто по высоте» (перенос уже в превью), высота и угол — фиксированный. Выбор шрифта — раскрывающийся список в секции (не всплывающее окно); предпросмотр вшитых — `FontFace` из `/fonts/…` в основном потоке. Замер (`editor_report`, нативно, release, M1 Pro): символ в текст из 10 000 символов — ≈ 7 мс (полная раскладка), ход курсора — ≈ 0,02 мс.

## 8. Протокол (дополнения к ядру §4.3)
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- События: `TextCaret { rect, selection }` (для поля ввода), `TextStyleSummary` (смешанные значения для панели), `FontRequest { family }` (ядру нужны байты), `MissingFonts { list }`.
- Пределы (ядро §4.3, недоверенный ввод): `data` ≤ 64 КБ за команду, имя семейства ≤ 256 байт, байты шрифта ≤ 64 МБ, `patch` — только известные ключи с проверкой значений.
- **Сделано (этап 1):** `FontsAvailable { source: bundled | system, families: [{ family, faces: [{ style, weight?, italic?, stretch?, file, index? }] }] }` (у системных Chrome вес и курсив ядро угадывает по имени начертания), `LoadFont { family, file, bytes? }` — по одному файлу (без байтов — файл не получить), `FontRequest { family, source, files }` (вшитые выполняет worker ядра, системные — основной поток), `MissingFonts { list: [{ family, style, nodes }] }`, `ReplaceMissingFont { from: { family, style }, to: { family, style } }`; сверх списка — события `FontCatalog { families: [{ family, source, styles }] }` (выбор замены) и `Log { message }` (журнал ошибок), команды `DemoText { texts }` (замер) и `TestCreateText` (только тестовый режим страницы). Правки, которым нужны ещё не загруженные шрифты, ядро держит в очереди и выполняет по приходу байтов.

````

на:

````markdown
- События: `TextCaret { rect, selection }` (для поля ввода), `TextStyleSummary` (смешанные значения для панели), `FontRequest { family }` (ядру нужны байты), `MissingFonts { list }`.
- Пределы (ядро §4.3, недоверенный ввод): `data` ≤ 64 КБ за команду, имя семейства ≤ 256 байт, байты шрифта ≤ 64 МБ, `patch` — только известные ключи с проверкой значений.
- **Сделано (этап 2):** `TextEditStart { id }`, `TextEditEnd`, `TextInput { kind, data? }` (`insert`, `paragraph`, `paste` — вне правки новый текст, `compose`, `commit`, `delete*`), `TextKey { key, mods }` (`key` — `KeyboardEvent.code`), `SetTextStyle { patch, phase, ids? }` и `SetTextProps { patch, phase, ids? }` (диапазон — выделение правки в ядре, поля `range` нет; превью — фазой, как у полей панели); события `TextCaret { editing, rect?, selection?, draft }`, `TextStyleSummary { summary? }` (перечисления — строками), сверх списка — `PageFonts { families }` (группа «В документе»); `Clipboard` получил `plain` (обычный текст фрагмента). Курсор `text` — у инструмента T и над текстом в правке.
- **Сделано (этап 1):** `FontsAvailable { source: bundled | system, families: [{ family, faces: [{ style, weight?, italic?, stretch?, file, index? }] }] }` (у системных Chrome вес и курсив ядро угадывает по имени начертания), `LoadFont { family, file, bytes? }` — по одному файлу (без байтов — файл не получить), `FontRequest { family, source, files }` (вшитые выполняет worker ядра, системные — основной поток), `MissingFonts { list: [{ family, style, nodes }] }`, `ReplaceMissingFont { from: { family, style }, to: { family, style } }`; сверх списка — события `FontCatalog { families: [{ family, source, styles }] }` (выбор замены) и `Log { message }` (журнал ошибок), команды `DemoText { texts }` (замер) и `TestCreateText` (только тестовый режим страницы). Правки, которым нужны ещё не загруженные шрифты, ядро держит в очереди и выполняет по приходу байтов.

````

- [ ] **Step 2: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo run --release -p ferrum-editor --example editor_report | tail -8`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo run --release -p ferrum-render --example render_report | grep -E "FAIL|Текст"`

Run: `export CARGO_TARGET_DIR=$PWD/target && ./scripts/compat-old-build.sh main`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npm run wasm && npm run typecheck && npm run unit && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npx playwright test e2e/text.spec.ts e2e/text-edit.spec.ts --project=firefox`

Expected: всё PASS; `editor_report` — строки «Символ в текст из 10 000 символов» (≈ 7 мс, цель ≤ 16 мс) и «Ход курсора» PASS; `compat-old-build.sh` — «Совместимость с main: файл с текстом открыт только для чтения»; `e2e:ci` — все passed; Firefox — 3 passed

- [ ] **Step 3: Закоммитить**

```bash
git add crates/ferrum-editor/examples/editor_report.rs docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md
git commit -m "docs,bench: спека подсистемы 3 — решения этапа 2 (правка текста, протокол, заливки и обводки текста); editor_report — ввод в текст из 10 000 символов

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Ручная проверка**

1. Chrome: `#/new`, T — щелчок, набор кириллицы и латиницы, стрелки с Shift и Option/Ctrl, двойной и тройной щелчок, Esc; T — протяжка рамкой, набор переносится по ширине; IME (японская раскладка macOS) — окно кандидатов у курсора, превью подчёркнуто.
2. Секция «Текст»: выбор шрифта с поиском, системные шрифты (разрешение браузера), начертание, размер перетаскиванием подписи — превью и один шаг; диапазон в правке — жирным (Cmd+B), отмена.
3. Копия фрагмента в другое приложение (обычный текст) и обратно; вставка картинкой заливки текста (через буфер обмена узла с заливкой) и обводка из секции «Обводка».
4. Firefox: набор, IME, копия — обычным текстом.

---

## Что этап 2 сознательно не делает

- **Раскладка правимого текста — целиком на каждую правку** (≈ 7 мс нативно на 10 000 символов; инкрементальная раскладка абзаца — позже, если замер в wasm упрётся в кадр).
- **Имя слоя по содержимому** (Figma называет текстовый слой его текстом): имя — «Текст», как на этапе 1.
- **Заливка участка из секции «Заливка»** (в правке — для выделенного диапазона): секция правит заливки узла; заливки участков приходят из буфера обмена и файлов.
- **Текст справа налево, вертикальный текст, мягкий перенос строки (Shift+Enter)**; `truncate` и `maxLines` в панели (только модель и раскладка этапа 1).
- **Подгонка «авто» рамок, когда отсутствующий шрифт появился** (M4 финального ревью этапа 1): нужна запись в документ вне истории отмены — следующий этап.
- **Глубокая копия содержимого в `PropsPatch::apply_to`** при смене только свойств (M8): `TextContent` за `Arc` — отдельная правка модели.
- **Пиксели GPU против CPU для оверлея правки и обводок текста** (проект `gpu`) и Safari — вручную.
