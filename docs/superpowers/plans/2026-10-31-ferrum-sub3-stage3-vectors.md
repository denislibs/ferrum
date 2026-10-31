# Ferrum, подсистема 3, этап 3: векторные сети и перо — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Векторы как в Figma: узел `Vector` с векторной сетью (точки с режимом ручек и радиусом, сегменты с ручками, области-петли с правилом заливки и своими заливками) значением в документе, перо (щелчок — точка, протяжка — ручки, замыкание, соединение контуров, Shift — 45°, Alt — разрыв зеркальности, один путь — один шаг), режим правки (выделение точек, ручек и сегментов щелчком, Shift и рамкой, перемещение с привязкой к точкам и пиксельной сетке, точка на сегменте, изгиб, удаление с сохранением формы и с разрывом, «Заливка области»), фигуры становятся векторами при правке, «Сплющить», окончания и соединения обводки в модели, рендере и секции «Обводка», инструмент «Стрелка»; сеть из 100 000 точек рисуется и правится в бюджете кадра.

**Architecture:** Модель (`ferrum-doc`) хранит сеть двоичным значением ключа `vector` (проверка недоверенных данных — `VectorNetwork::decode`), стиль обводки — четырьмя ключами узла; кэш сцены — версия 4. Геометрия (`ferrum-scene::vector`) строит из сети цепочки и петли для обводки, контуры областей, открытые концы с окончаниями, растягивает сеть под рамку узла (`fit`) и проверяет попадание; рендер заливает области, а обводку по контуру — заранее построенным контуром `kurbo::stroke` с фигурами окончаний (кэш по версии узла, у больших сетей — упрощение под масштаб). Редактор (`ferrum-editor`) держит сеанс правки вектора (`VectorEdit`: сеть в координатах формы, матрица формы → мир, выделение, инструмент внутри правки, путь пера, жест) и пишет каждую правку одной транзакцией через `vector_ops::reframe` (рамка и положение — по границам сети, с поворотом и отражением); чистые правки сети — `vector_ops`. Ядро worker'а (`ferrum-wasm`) получает инструменты `pen` и `arrow`, правку стиля обводки с проверкой и его сводку; интерфейс — кнопки тулбара и строки секции «Обводка» с пределами из `limits.ts`. Основной поток ничего не считает: ввод холста уходит в ядро как раньше.

**Tech Stack:** Rust 1.96 (внешних крейтов не добавляется: `kurbo 0.13.1` — `stroke`, `flatten`, `ParamCurveNearest`, `CubicBez::subsegment`; `loro 1.16.2` — `LoroValue::Binary`; `vello_cpu 0.3.0`), SolidJS 1.9.17, TypeScript 6.0.3, @playwright/test 1.64.0 (`chromium`, `firefox`).

**Спеки:** `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` — раздел 9 и части 8 и 12, нужные этапу 3 (булевы операции — раздел 10, Scale и экспорт — раздел 11 — не входят); спека ядра `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` («ядро §N»).

## Global Constraints

- Ветка `feat/sub3-vectors` от `main` (6a9e200, этапы 1 и 2 подсистемы 3 слиты). Rust toolchain `1.96.0` из `rust-toolchain.toml`. Каталог сборки — у каждой копии свой: `export CARGO_TARGET_DIR=$PWD/target` (в `apps/web` — `$PWD/../../target`); команды — из корня копии, если не сказано иное.
- Новых внешних зависимостей нет: `Cargo.lock` и `package-lock.json` не меняются.
- Основной поток тонкий (ядро §4.2): сеть, попадание, привязки, грани, упрощение — в worker'е ядра. В основном потоке — кнопки тулбара и строки секции «Обводка»; чтения layout — только в фазе `measure` планировщика (E2E проверяют `__ferrumWarnings`).
- Недоверенный ввод (спека 8, ядро §4.3): сеть из файла и буфера обмена проверяется целиком (`VectorNetwork::decode`: конечные числа ≤ 1e9, индексы в сети, сегмент не в себя, петли областей замкнуты, ≤ 100 000 точек, ≤ 200 000 сегментов, ≤ 10 000 петель, заливки областей — только цвет, длины сверяются с остатком байтов до выделения памяти); правка стиля обводки из панели — только известные ключи (`deny_unknown_fields`), предел острого угла вне 1…100 — `Error` на русском без правки (ядро не зажимает); панель зажимает значение теми же числами из `limits.ts`, тест сверяет их с Rust.
- Одна правка — один шаг отмены: путь пера (до Esc/Enter, замыкания или соединения), жест в правке (точки, ручка, изгиб) — при отпускании, удаление, «Заливка области», сдвиг стрелкой, «Сплющить», превращение фигуры в вектор — тем же шагом, что первая правка. Превью жеста и пути — только в сцене (превью узла или черновик).
- Поза родителя: перо и правка учитывают поворот и отражение родителя и самого узла (точки — под курсором в мире); запись не сдвигает узел на экране (`reframe`).
- Тесты не проходят вхолостую: модель — двоичная форма и перезагрузка из Loro и кэша; геометрия — точки, концы, попадание; рендер — пиксели каждого окончания, соединения, положения обводки и правила заливки, затем эталоны; редактор — документ, шаги отмены, мировые координаты точек; ядро — события; E2E — сводка панели, строки слоёв, шаги и пиксели экрана.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Сначала прототип всего этапа собран в отдельной копии (`plan-verify-wt3`, ветка `verify/sub3-vectors`, по коммиту на задачу) и прошёл ворота: `cargo fmt --check`, `clippy -D warnings` (нативно и `wasm32-unknown-unknown`), `cargo test --workspace` (788 тестов), `render_report` и `editor_report` (release, все строки PASS; сеть 100 000 точек — первый кадр ≈ 7–9 мс, кадр ≈ 1,5 мс, правка точки ≈ 12–13 мс, кадр правки с оверлеем ≈ 4 мс, кадр жеста перетаскивания точки ≈ 3 мс, запись ≈ 6 мс), wasm-сборка, `npm run typecheck`, модульные (134), E2E `chromium` целиком (`e2e:ci` — 322 теста), `vectors`, `text`, `text-edit`, `roundtrip` в Firefox (6 passed), совместимость: `scripts/compat-old-build.sh main vector_fixture` и прежняя `scripts/compat-old-build.sh` (текст против 2710c85). Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом к чистой копии `feat/sub3-vectors` (`plan-apply-wt3`) в порядке задач: тесты шага 1 — и проверка, что команда шага 2 действительно падает (не собирается или падает, как написано), затем реализация — каждый фрагмент «заменить» найден ровно один раз, эталоны — командой шага генерации, после каждой задачи — все команды её шага «Проверить» (включая wasm-сборку и `typecheck` задач 1, 5, 8, E2E задачи 9 и полный `e2e:ci` и Firefox задачи 10); после последней задачи дерево совпало с прототипом байт в байт (и эталоны PNG). Порт 4173 на машине проверки занят посторонним процессом, поэтому E2E шли через временный конфиг Playwright с портами 4273/4274 (и временной правкой портов в `core.spec.ts` и `settings.spec.ts`), с `--retries=1`: нестабильными (прошли со второго раза или отдельно) были `text-edit.spec.ts` «фрагмент со стилями вставляется в другой текст», `files.spec.ts` «1000 записей… без длинных задач» и `picker.spec.ts` «обводка: ширина жестом…» — все трое вне этапа 3 и проходят отдельно. Найденное прототипом и вошедшее в шаги: без упрощения сеть из 100 000 точек давала кадр ≈ 30 мс, первую постройку контура обводки ≈ 250 мс и правку ≈ 270 мс (`kurbo::stroke` на 100 000 отрезков даёт 800 000 элементов) — отсюда упрощение контуров под масштаб, выступ обводки без контуров и связи точек одним массивом (задача 10); оверлей правки рисовал каждый сегмент своим подпутём, и упрощение не работало (кадр ≈ 50 мс); двойной щелчок по фигуре в группе сначала идёт на уровень глубже (иначе ломался `selection::double_click_goes_deeper…`); тест живых значений панели нажимал второй раз в пределах двойного щелчка — теперь это правка прямоугольника. Не запускались: проект Playwright `gpu` (пиксели WebGPU/WebGL2 против CPU с векторами и оверлеем правки), Safari/WebKit, приложение Tauri, замеры в wasm (только нативные отчёты), ручная проверка (задача 10). Тексты ожидаемых результатов — с этого прогона.

---

## Этапы подсистемы 3

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Текст: модель и рендер (слит) | узел `Text`, шрифты, раскладка, отрисовка, отсутствующие шрифты | 3, 4, 5, 6 |
| 2. Инструмент «Текст» и правка (слит) | T, правка на холсте, ввод и IME, курсор, буфер обмена, секция «Текст» | 7, 8 |
| **3. Векторные сети и перо** (этот план) | узел `Vector`, перо, режим правки, «Сплющить», окончания и соединения, стрелка | 9 |
| 4–5 | булевы операции; Scale и экспорт SVG/PDF | 10, 11 |

## Решения, которых в спеке нет (на согласование)

Пользователь просил не останавливаться на вопросах: где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**.

1. Сеть — одно значение `vector` узла (двоичная форма версии 1), а не дерево контейнеров Loro: правка сети заменяет значение целиком, как список заливок (совместная правка двух людей в одном векторе — «последний пишет», подсистема 4 решит иначе, если понадобится). Неверная сеть из файла скрывает узел и делает документ только для чтения — как узел неизвестного типа (`DocError::InvalidVector`). Правка сверх 100 000 точек — отказ с сообщением «Вектор длиннее 100 000 точек — правка не применена».
2. Рамка вектора (`w × h`) — точные границы сети (с экстремумами кривых); ресайз узла растягивает сеть под рамку (`ferrum_scene::vector::fit`), поэтому все прежние пути ресайза (ручки, панель W/H, масштаб группы) работают без изменений. Правка в сеансе идёт в координатах формы узла, запись сдвигает сеть к началу её границ и пересчитывает `x`, `y`, `w`, `h` с поворотом и отражением (`vector_ops::reframe`).
3. **(поведение)** Стиль обводки — один на узел, как в Figma (`strokeCap`, `strokeJoin`, `strokeMiterLimit`): ключи `strokeCapStart`, `strokeCapEnd`, `strokeJoin`, `strokeMiterLimit` (1…100, по умолчанию 4, нейтральные не пишутся). «Начало» — открытый конец, который у своего сегмента начало (первая точка пути пера), «конец» — остальные открытые концы (у линии — `(0, 0)` и `(w, 0)`). Поточечных окончаний (`VectorVertex.strokeCap` в Figma) нет.
4. **(поведение)** Окончания-фигуры (размеры — как на глаз в Figma): «Стрелка» — две линии толщины обводки длиной 3w + 6 под 30° назад, «Треугольник» — залитый, остриё на w за концом, длина 3w + 6, «Круг» и «Ромб» — радиус 1,5w + 1,5; «Круглое» и «Квадратное» — на полтолщины. В точках, где сходится больше двух сегментов, — круг толщины обводки (ветви сети не обрываются зарубкой). Полупрозрачная обводка с окончаниями рисуется слоем: перекрытия не темнее.
5. **(поведение)** Обводка внутри и снаружи — только у замкнутого пути (нет открытых концов): двойная толщина с обрезкой по контуру или по его внешности; у открытого пути положение игнорируется — по центру (как в Figma). Соединение и предел угла действуют и на прямоугольник (у скруглённого и эллипса углов нет); «острое» у прямоугольника рисуется прежними кольцами, остальные — обводкой по контуру. Окончания линии — тоже по контуру.
6. **(поведение)** Перо: путь до конца — черновик сцены (в документе и слоях его нет); Esc или Enter заканчивают путь одним шагом — у инструмента «Перо» с возвратом к «Перемещению» и выделенным вектором, в правке вектора — к выделению точек (повторный Esc — выход из правки). Путь из одной точки без сегментов исчезает без шага. Ctrl/Cmd+Z во время пути убирает его последнюю точку, а не шаг истории. Щелчок по первой точке пути замыкает его (петля — область с заливками узла; у нового вектора заливок нет, как в Figma), щелчок по точке другого контура той же сети — соединяет и заканчивает путь, щелчок по точке без начатого пути — продолжает от неё, щелчок по сегменту без начатого пути — ставит на нём точку. Протяжка: исходящая ручка — к курсору, входящая — зеркально (`mirrorAll`); Alt во время протяжки — входящая остаётся, точка `free`. Новые точки — по пиксельной сетке (если включена); Shift — шаг 45° от прежней точки с длиной проекции. Перо из тулбара в документе только для чтения ничего не делает.
7. **(поведение)** Вход в правку — двойной щелчок (как в Figma, первый двойной щелчок по узлу в группе или фрейме сначала идёт на уровень глубже) и Enter по единственному выделенному; заблокированный узел и документ только для чтения — нет. Фигура (прямоугольник, эллипс, линия) становится вектором только при первой правке, тем же шагом: новый узел на её месте (та же позиция в родителе, имя, заливки, обводки, поза; скругление — дугами сети); без правки Esc оставляет фигуру фигурой. Выход — Esc, Enter, другой инструмент, выделение из слоёв, другая страница, щелчок по другому узлу (он выделяется, как обычным щелчком).
8. **(поведение)** Инструменты внутри правки: V — выделение, P — перо, B — «Заливка области» (повторное B или Esc — назад к выделению). Выделение: щелчок по точке, ручке или сегменту, Shift — добавить и убрать, рамка с пустого места (Shift — добавить). Протяжка точки или сегмента двигает выделенные точки (Shift — по оси), привязка — к точкам сети ближе 4 экранных пикселей по каждой оси (красные линии) и к целым пикселям мира по осям без привязки к точке. Ручки видны у выделенных точек, концов выделенных сегментов и их соседей; протяжка ручки — с зеркальностью точки (`mirrorAngle` — направление, `mirrorAll` — и длина; у точки без ручек появляется `mirrorAll`), Alt — разрыв (`free`). Ctrl/Cmd + протяжка сегмента — изгиб: точка кривой при параметре щелчка уходит под курсор. Стрелки (Shift — на 10) сдвигают выделенные точки, Ctrl/Cmd+A выделяет все. Команды узлов (копировать, группировать и т. п.) заканчивают правку и выполняются как обычно; вид и отмена — нет.
9. **(поведение)** Удаление: Delete/Backspace — с сохранением формы (точка с двумя сегментами — один сегмент, приближающий оба наименьшими квадратами с теми же касательными в концах; два отрезка — отрезок; конец пути — уходит со своим сегментом; точка ветвления — с сегментами), Shift+Delete — с разрывом (точки с сегментами); выделены только сегменты — уходят сегменты и оставшиеся без сегментов точки; петли с удалённым сегментом перестают быть областями; удалены все точки — удаляется узел.
10. **(поведение)** «Заливка области» — самая маленькая грань под щелчком (грани плоского графа сегментов по углам касательных; пересечения сегментов не ищутся — как сеть без самопересечений). Залитая грань (петля какой-то области) — заливка снимается, иначе — новая область с заливками узла, а у узла без видимых заливок — серая `#d9d9d9` (как у новой фигуры). Заливки областей — только цвет.
11. **(поведение)** «Сплющить» (Ctrl/Cmd+E): векторы, прямоугольники, эллипсы, линии и группы только из них (группа с текстом, изображением или фреймом пропускается; текст — контуром глифов на этапе 4 вместе с «Контуром обводки»); заблокированные пропускаются; нечего сплющить — сообщение без шага. Один узел — вектор той же позы на его месте; несколько — одна сеть в координатах родителя верхнего узла, без поворота, на месте верхнего, с его именем, заливками, обводками и стилем обводки, а области остальных узлов хранят их заливки (если отличаются и все цветом).
12. **(поведение)** Инструмент «Стрелка» (Shift+L) — линия с `arrowLines` на конце, в меню фигур тулбара после линии; после рисования — «Перемещение». Перо — своя кнопка тулбара (P) вместо отключённой; комментарии остаются отключёнными.
13. **(поведение)** Секция «Обводка»: под списком обводок — «Окончание в начале» и «Окончание в конце» (только если у выделенных есть открытые концы), «Соединение» (если есть углы) и «Предел острого угла» (у соединения «Острое» или «Смешанное»); «Смешанное» — пустой пункт списка. Каждая смена — одна команда и один шаг; поле предела — с превью при перетаскивании подписи, как все поля чисел.
14. Большие сети рисуются упрощёнными: от 4096 элементов контура — ломаной с точностью в четверть пикселя устройства для масштаба-степени двойки (перестраивается при смене этой степени), отбрасываются точки ближе допуска к прямой от последней оставленной (Реуманн — Виткам). Оверлей правки упрощает сеть так же, а при > 5000 точек на экране рисует только выделенные. Точный путь остаётся для попадания и правки.
15. Радиус скругления точки (`cornerRadius` вершины в Figma) хранится и проверяется, но не рисуется и не правится — позже (см. «Что этап 3 сознательно не делает»).
16. Протокол: `Tool` — `pen`, `arrow`; `CursorKind` — `pen` (CSS `crosshair`), `bucket` (`cell`); `NodeType` — `vector` (иконка пера); `Edit::StrokeStyle { capStart?, capEnd?, join?, miterLimit? }`; `Summary.strokeStyle { ends, corners, capStart?, capEnd?, join?, miterLimit }` (`undefined` — «Смешанное»). Команды раскладки `tool.pen` (P), `tool.arrow` (Shift+L), `object.flatten` (Ctrl/Cmd+E) — всего 48. Сеть в протокол не идёт: правка вектора — ввод холста.
17. Совместимость: файл с вектором открывается сборкой без векторов только для чтения — `scripts/compat-old-build.sh <ref> vector_fixture` (скрипт получил второй аргумент — пример; по умолчанию прежний `text_fixture`).

## Как план закрывает повторявшиеся дефекты этапа 2

- **Поворот и отражение родителя.** Перо ставит позу нового вектора через `decompose_with` (как `DrawTool`), правка работает в координатах формы через мировую матрицу узла, запись — `reframe`; тесты `pen_honours_parent_rotation_and_flip`, `rotated_and_flipped_vector_moves_points_under_the_cursor`, `reframe_keeps_points_in_place_under_rotation_and_flip`.
- **Превью и запись — один шаг.** Каждый жест — превью сцены (у фигуры — черновик поверх скрытой фигуры) и одна транзакция при отпускании (`commit_vector_work`); тесты сверяют число шагов, отмену и повтор (`assert_one_step`) и чистую сцену после жеста (`assert_scene_clean`).
- **Чтения layout вне `measure`.** Новый код основного потока — только JSX и команды; E2E векторов проверяют `__ferrumWarnings` после каждого теста.
- **Клавиши.** Esc, Enter, Delete, B правки — по физическим `code` и без модификаторов (кроме Shift у удаления), остальное — раскладкой: AltGr и IME правку не трогают (в правке вектора нет текстового ввода).
- **Недоверенные значения отклоняются, а не зажимаются** (предел угла, сеть), панель зажимает из одного `limits.ts` (тест сверяет с Rust).
- **Полная замена при совместной правке** — сознательное решение 1 (сеть — значение), стиль обводки — отдельными ключами.
- **Пиксели на каждый вариант краски и обводки:** каждое из 7 окончаний, 3 соединения и предел угла, 3 положения обводки на замкнутом пути, 2 правила заливки, своя заливка области, круглое соединение прямоугольника, стрелка линии, полупрозрачная обводка с окончаниями, оверлей правки.

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-doc/src/vector.rs` (новый), `{types,codec,page,scene_cache,error,binary,container,lib,fixture}.rs`, `examples/vector_fixture.rs` (новый) | сеть, двоичная форма и проверка, стиль обводки, ключи узла, кэш сцены v4, пример для совместимости, сеть для замеров |
| `crates/ferrum-scene/src/vector.rs` (новый), `shape.rs`, `scene.rs` | цепочки и петли, концы, растяжение под рамку, контур обводки с окончаниями, границы, попадание, упрощение |
| `crates/ferrum-render/src/{display,draw,overlay,lib}.rs` | области, обводка по контуру, контуры по масштабу, оверлей правки, подсветка вектора |
| `crates/ferrum-editor/src/vector_ops.rs` (новый) | чистые правки сети |
| `crates/ferrum-editor/src/vector_edit.rs` (новый) | сеанс правки, перо, запись, оверлей, клавиши |
| `crates/ferrum-editor/src/vector_select.rs` (новый) | выделение, перемещение, привязки, изгиб, удаление, заливка области, команды в правке |
| `crates/ferrum-editor/src/flatten.rs` (новый), `{editor,actions,commands,panel,clipboard,lib}.rs`, `tools/{mod,draw,select}.rs` | маршрутизация в правку, «Сплющить», «Стрелка», стиль обводки из панели, буфер обмена |
| `crates/ferrum-wasm/src/{protocol,core,summary,layers}.rs`, `core/panels.rs` | инструменты, курсоры, правка и сводка стиля обводки |
| `apps/web/src/ui/{toolbar,paints,icons,layers}.tsx`, `src/core/{core,limits}.ts` | кнопки, строки секции «Обводка», пределы, курсоры |
| `apps/web/{e2e,unit}/…`, `scripts/compat-old-build.sh` | E2E векторов, пределы, совместимость |
| `crates/{ferrum-render,ferrum-editor}/examples/*_report.rs`, спека | замеры 100 000 точек; решения этапа 3 |

---

### Task 1: Модель: узел `Vector` и стиль обводки

Спека 9: сеть хранится значением в узле, ≤ 100 000 точек, старые сборки открывают её только для чтения, узел копируется в буфер обмена. Здесь — типы сети (`VectorPoint`, `VectorSegment`, `VectorRegion`, `HandleMode`, `FillRule`), двоичная форма с проверкой (её же читают кэш сцены и буфер обмена — base64), стиль обводки (`StrokeCap`, `StrokeJoin`, `StrokeStyle`) в `Props` и `PropsPatch`, ключи узла, кэш сцены версии 4, неверная сеть из Loro — как неизвестный тип, отказ правки сверх предела. Чтобы рабочее пространство собиралось, сцена пока считает вектор рамкой, слой — иконкой пера.

**Files:**
- Create: `crates/ferrum-doc/src/vector.rs`, `crates/ferrum-doc/examples/vector_fixture.rs`
- Modify: `crates/ferrum-doc/src/{types,codec,page,scene_cache,error,binary,container,lib}.rs`, `crates/ferrum-editor/src/clipboard.rs`, `crates/ferrum-scene/src/shape.rs` (временно), `crates/ferrum-wasm/src/layers.rs`, `apps/web/src/ui/layers.tsx`, `scripts/compat-old-build.sh`
- Test: `crates/ferrum-doc/tests/vector.rs` (новый), `crates/ferrum-doc/tests/{container,flip}.rs`, `crates/ferrum-editor/tests/clipboard_format.rs`

**Interfaces:**
- Produces: `VectorNetwork { points, segments, regions }` (`degrees`, `loop_points(&[u32]) -> Option<Vec<u32>>`, `validate`, `encode`, `decode(&[u8]) -> Result<_, VectorError>`, `to_base64`, `from_base64`), `VectorPoint { x, y, handle_mode, corner_radius }` (`new`), `VectorSegment { start, end, tangent_start: [f64; 2], tangent_end }` (`line`, `is_straight`), `VectorRegion { rule, loops, fills: Option<Vec<Fill>> }`, `HandleMode`, `FillRule`, `VectorError`, `MAX_VECTOR_POINTS`; `StrokeCap` (`ALL`, `as_str`, `parse`), `StrokeJoin`, `StrokeStyle { cap_start, cap_end, join, miter_limit }`, `DEFAULT_MITER_LIMIT`, `MITER_LIMIT_RANGE`; `NodeKind::Vector`; `Props::{stroke_style, vector: Option<Arc<VectorNetwork>>}`; `PropsPatch::{stroke_cap_start, stroke_cap_end, stroke_join, stroke_miter_limit, vector}`; `DocError::{InvalidVector, VectorTooLarge}`; `PageDoc::force_vector_bytes_for_tests`; `NodeType::Vector`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-doc/tests/container.rs` заменить:

````rust
        NodeKind::Image,
        NodeKind::Text,
    ];
    doc.transact(page, vec![], |p| {
````

на:

````rust
        NodeKind::Image,
        NodeKind::Text,
        NodeKind::Vector,
    ];
    doc.transact(page, vec![], |p| {
````

В `crates/ferrum-doc/tests/container.rs` заменить:

````rust
                    // Содержимое — ниже, своими правками.
                    content: None,
                },
            )?;
````

на:

````rust
                    // Содержимое — ниже, своими правками.
                    content: None,
                    stroke_cap_start: Some(StrokeCap::ALL[i % 7]),
                    stroke_cap_end: Some(StrokeCap::ALL[(i + 3) % 7]),
                    stroke_join: Some(StrokeJoin::ALL[i % 3]),
                    stroke_miter_limit: Some(1.5 + f),
                    // У не вектора пропускается.
                    vector: Some(std::sync::Arc::new(VectorNetwork {
                        points: vec![VectorPoint::new(0.0, 0.0), VectorPoint::new(f, 7.0)],
                        segments: vec![VectorSegment::line(0, 1)],
                        regions: vec![],
                    })),
                },
            )?;
````

В `crates/ferrum-doc/tests/flip.rs` заменить:

````rust
fn scene_cache_version_covers_flips() {
    assert_eq!(
        SCENE_CACHE_VERSION, 3,
        "таблица с флагами отражения (2) и текстом (3)"
    );
    // Кэш прежней версии с честным хэшем снапшота игнорируется без предупреждений.
````

на:

````rust
fn scene_cache_version_covers_flips() {
    assert_eq!(
        SCENE_CACHE_VERSION, 4,
        "таблица с флагами отражения (2), текстом (3), стилем обводки и сетью (4)"
    );
    // Кэш прежней версии с честным хэшем снапшота игнорируется без предупреждений.
````

Создать `crates/ferrum-doc/tests/vector.rs`:

````rust
//! Узел `Vector` и стиль обводки (подсистема 3, этап 3): двоичная форма сети с
//! проверкой не доверенных данных, запись в документ, зеркало против честной
//! перезагрузки из Loro и из кэша сцены, отмена, пределы, неверная сеть в файле.

use std::sync::Arc;

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;

/// Треугольник: три точки, три сегмента (один — кривая), область с петлёй.
fn triangle() -> VectorNetwork {
    VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint {
                handle_mode: HandleMode::MirrorAll,
                corner_radius: 2.5,
                ..VectorPoint::new(100.0, 0.0)
            },
            VectorPoint::new(50.0, 80.0),
        ],
        segments: vec![
            VectorSegment::line(0, 1),
            VectorSegment {
                start: 1,
                end: 2,
                tangent_start: [10.0, 20.0],
                tangent_end: [-5.0, 0.0],
            },
            VectorSegment::line(2, 0),
        ],
        regions: vec![VectorRegion {
            rule: FillRule::EvenOdd,
            loops: vec![vec![0, 1, 2]],
            fills: Some(vec![Fill::solid(Color::rgba(255, 0, 0, 255))]),
        }],
    }
}

fn doc_with_vector() -> (Document, PageId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            let id = tx.create_with_props(
                None,
                None,
                &Props {
                    w: 100.0,
                    h: 80.0,
                    stroke_style: StrokeStyle {
                        cap_start: StrokeCap::Round,
                        cap_end: StrokeCap::ArrowTriangle,
                        join: StrokeJoin::Bevel,
                        miter_limit: 8.0,
                    },
                    vector: Some(Arc::new(triangle())),
                    ..Props::defaults(NodeKind::Vector)
                },
            )?;
            Ok(vec![id])
        })
        .unwrap()[0];
    (doc, page, id)
}

#[test]
fn network_round_trips_through_binary_and_base64() {
    let net = triangle();
    assert_eq!(VectorNetwork::decode(&net.encode()), Ok(net.clone()));
    assert_eq!(
        VectorNetwork::from_base64(&net.to_base64()),
        Ok(net.clone())
    );
    assert_eq!(net.loop_points(&[0, 1, 2]), Some(vec![0, 1, 2]));
    // Петля в обратном порядке сегментов тоже замкнута.
    assert_eq!(net.loop_points(&[2, 1, 0]), Some(vec![0, 2, 1]));
    assert_eq!(net.degrees(), vec![2, 2, 2]);
}

#[test]
fn untrusted_network_is_rejected() {
    let bad = |f: &dyn Fn(&mut VectorNetwork)| {
        let mut n = triangle();
        f(&mut n);
        VectorNetwork::decode(&n.encode()).is_err()
    };
    assert!(bad(&|n| n.points[0].x = f64::NAN));
    assert!(bad(&|n| n.points[0].y = 1e12));
    assert!(bad(&|n| n.points[1].corner_radius = -1.0));
    assert!(bad(&|n| n.segments[0].end = 9));
    assert!(bad(&|n| n.segments[0].end = 0));
    assert!(bad(&|n| n.segments[1].tangent_end = [f64::INFINITY, 0.0]));
    // Петля не замкнута: сегментов 0 и 1 мало для цикла.
    assert!(bad(&|n| n.regions[0].loops = vec![vec![0, 1]]));
    assert!(bad(&|n| n.regions[0].loops = vec![vec![]]));
    assert!(bad(&|n| n.regions[0].fills = Some(vec![Fill::image("ab")])));
    // Обрезанная и удлинённая форма, чужая версия.
    let bytes = triangle().encode();
    assert!(VectorNetwork::decode(&bytes[..bytes.len() - 1]).is_err());
    let mut long = bytes.clone();
    long.push(0);
    assert!(VectorNetwork::decode(&long).is_err());
    let mut version = bytes;
    version[0] = 2;
    assert!(VectorNetwork::decode(&version).is_err());
    // Заявленное число точек больше остатка байтов — отказ без выделения памяти.
    let mut huge = vec![1u8];
    huge.extend_from_slice(&(MAX_VECTOR_POINTS as u32).to_le_bytes());
    assert!(VectorNetwork::decode(&huge).is_err());
    let mut over = vec![1u8];
    over.extend_from_slice(&(MAX_VECTOR_POINTS as u32 + 1).to_le_bytes());
    assert_eq!(
        VectorNetwork::decode(&over),
        Err(VectorError::TooManyPoints)
    );
}

#[test]
fn vector_and_stroke_style_survive_reload_from_loro_and_cache() {
    let (mut doc, page, id) = doc_with_vector();
    let before = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(before.kind, NodeKind::Vector);
    assert_eq!(before.vector.as_deref(), Some(&triangle()));
    // Без кэша — из Loro.
    let snapshot = doc.page(page).unwrap().export_snapshot().unwrap();
    let loro = PageDoc::from_snapshot(&snapshot).unwrap();
    assert_eq!(loro.props(id).unwrap(), before);
    // Файл целиком — из кэша сцены.
    let mut opened = decode(&encode(&doc).unwrap()).unwrap().document;
    let p = opened.page(page).unwrap();
    assert!(p.loaded_from_cache());
    assert_eq!(p.props(id).unwrap(), before);
    assert!(!opened.is_read_only());
}

#[test]
fn neutral_stroke_style_is_not_written_and_parses_back() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    let p = doc.page(page).unwrap();
    let snapshot = p.export_snapshot().unwrap();
    let props = PageDoc::from_snapshot(&snapshot)
        .unwrap()
        .props(id)
        .unwrap();
    assert_eq!(props.stroke_style, StrokeStyle::default());
    assert!(props.vector.is_none());
}

#[test]
fn vector_edit_is_one_undo_step_and_foreign_vector_is_ignored() {
    let (mut doc, page, id) = doc_with_vector();
    let mut moved = triangle();
    moved.points[2].y = 90.0;
    doc.transact(page, vec![id], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                vector: Some(Arc::new(moved.clone())),
                h: Some(90.0),
                stroke_join: Some(StrokeJoin::Round),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    })
    .unwrap();
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.vector.as_deref(), Some(&moved));
    assert_eq!(p.stroke_style.join, StrokeJoin::Round);
    doc.undo().unwrap();
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.vector.as_deref(), Some(&triangle()));
    assert_eq!(p.stroke_style.join, StrokeJoin::Bevel);
    // Сеть в патче прямоугольника пропускается, остальное пишется.
    let rect = doc
        .transact(page, vec![], |tx| {
            let r = tx.create_node(None, None, NodeKind::Rect)?;
            tx.set_props(
                r,
                &PropsPatch {
                    vector: Some(Arc::new(triangle())),
                    stroke_cap_end: Some(StrokeCap::Circle),
                    ..Default::default()
                },
            )?;
            Ok(vec![r])
        })
        .unwrap()[0];
    let r = doc.page(page).unwrap().props(rect).unwrap();
    assert!(r.vector.is_none());
    assert_eq!(r.stroke_style.cap_end, StrokeCap::Circle);
}

#[test]
fn over_limit_network_is_refused_without_a_step() {
    let (mut doc, page, id) = doc_with_vector();
    let big = VectorNetwork {
        points: (0..=MAX_VECTOR_POINTS)
            .map(|i| VectorPoint::new(i as f64, 0.0))
            .collect(),
        ..Default::default()
    };
    let err = doc.transact(page, vec![id], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                vector: Some(Arc::new(big.clone())),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    });
    assert!(matches!(err, Err(DocError::VectorTooLarge)));
    let created = doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_with_props(
            None,
            None,
            &Props {
                vector: Some(Arc::new(big.clone())),
                ..Props::defaults(NodeKind::Vector)
            },
        )?])
    });
    assert!(matches!(created, Err(DocError::VectorTooLarge)));
    // Остался только шаг создания исходного вектора.
    doc.undo().unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn invalid_network_in_file_hides_the_node_and_opens_read_only() {
    let (mut doc, page, id) = doc_with_vector();
    let p = doc.page(page).unwrap();
    // Петля области ссылается на сегмент, которого нет.
    let mut broken = triangle();
    broken.regions[0].loops = vec![vec![0, 1, 7]];
    for bytes in [vec![1, 2, 3], broken.encode()] {
        p.force_vector_bytes_for_tests(id, &bytes);
        // Снапшот без кэша сцены — путь чтения из Loro (как файл чужой сборки).
        let reopened = PageDoc::from_snapshot(&p.export_snapshot().unwrap()).unwrap();
        assert!(
            reopened.has_unknown_nodes(),
            "документ откроется только для чтения"
        );
        assert!(reopened.walk().is_empty(), "узел с неверной сетью скрыт");
    }
}
````

В `crates/ferrum-editor/tests/clipboard_format.rs` заменить:

````rust
    assert_eq!((p.nodes[1].props.w, p.nodes[1].props.h), (10.0, 10.0));
}
````

на:

````rust
    assert_eq!((p.nodes[1].props.w, p.nodes[1].props.h), (10.0, 10.0));
}

#[test]
fn vector_and_stroke_style_round_trip_and_are_checked() {
    use ferrum_doc::{StrokeCap, StrokeJoin, VectorNetwork, VectorPoint, VectorSegment};
    use std::sync::Arc;
    let mut v = Props::defaults(NodeKind::Vector);
    v.w = 30.0;
    v.stroke_style.cap_end = StrokeCap::ArrowLines;
    v.stroke_style.join = StrokeJoin::Round;
    v.stroke_style.miter_limit = 10.0;
    v.vector = Some(Arc::new(VectorNetwork {
        points: vec![VectorPoint::new(0.0, 0.0), VectorPoint::new(30.0, 0.0)],
        segments: vec![VectorSegment::line(0, 1)],
        regions: vec![],
    }));
    let p = ClipPayload {
        nodes: vec![ClipNode {
            parent: None,
            props: v,
        }],
        images: vec![],
        bounds: Rect::new(0.0, 0.0, 30.0, 0.0),
    };
    let bytes = p.encode();
    let json = json_of(&bytes);
    assert!(json.contains("\"strokeCapEnd\":\"arrowLines\""), "{json}");
    assert!(
        !json.contains("strokeCapStart"),
        "нейтральное не пишется: {json}"
    );
    assert_eq!(ClipPayload::decode(&bytes).unwrap(), p);
    // Чужие данные: неизвестное окончание, предел вне диапазона, битая сеть.
    for (from, to) in [
        ("\"arrowLines\"", "\"arrow\""),
        ("\"strokeMiterLimit\":10.0", "\"strokeMiterLimit\":1000.0"),
        ("\"vector\":\"", "\"vector\":\"AAAA"),
    ] {
        let bad = json.replace(from, to);
        assert_ne!(bad, json);
        assert!(
            matches!(
                ClipPayload::decode(&with_json(&bad)),
                Err(ClipboardError::Corrupted(_))
            ),
            "{to}"
        );
    }
    // Узел без поля сети — пустой вектор, а не ошибка.
    let empty = ClipPayload::decode(&with_json(&doc_json(
        &[node_json(None, "vector")],
        &[],
        "[0.0,0.0,10.0,10.0]",
    )))
    .unwrap();
    assert_eq!(
        empty.nodes[0].props.vector.as_deref(),
        Some(&VectorNetwork::default())
    );
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-doc --test vector`
Expected: FAIL — не собирается: нет `VectorNetwork`, `VectorPoint`, `StrokeCap`, `StrokeStyle`, `NodeKind::Vector`, полей `Props::vector` и `PropsPatch::stroke_cap_end`

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/layers.tsx` заменить:

````tsx
  image: 'image',
  text: 'text',
};

````

на:

````tsx
  image: 'image',
  text: 'text',
  vector: 'pen',
};

````

Создать `crates/ferrum-doc/examples/vector_fixture.rs`:

````rust
//! Файл `.ferrum` с векторным узлом — для проверки совместимости: сборка без
//! векторов (этап 2 подсистемы 3, `main` до этапа 3) открывает его только для
//! чтения (спека подсистемы 3, раздел 9). `cargo run -p ferrum-doc --example
//! vector_fixture -- <путь>`; проверку старой сборкой делает
//! `scripts/compat-old-build.sh <ref> vector_fixture`.

use std::sync::Arc;

use ferrum_doc::container::encode;
use ferrum_doc::{
    Document, NodeKind, Props, PropsPatch, StrokeCap, VectorNetwork, VectorPoint, VectorSegment,
};

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
        let mut arrow = Props {
            name: "Стрелка".into(),
            y: 150.0,
            w: 120.0,
            vector: Some(Arc::new(VectorNetwork {
                points: vec![VectorPoint::new(0.0, 0.0), VectorPoint::new(120.0, 0.0)],
                segments: vec![VectorSegment::line(0, 1)],
                regions: vec![],
            })),
            ..Props::defaults(NodeKind::Vector)
        };
        arrow.stroke_style.cap_end = StrokeCap::ArrowLines;
        tx.create_with_props(None, None, &arrow)?;
        Ok(vec![])
    })
    .unwrap();
    std::fs::write(&path, encode(&doc).unwrap()).unwrap();
    println!("{path}: прямоугольник и вектор");
}
````

В `crates/ferrum-doc/src/binary.rs` заменить:

````rust
    }

    pub fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
````

на:

````rust
    }

    /// Сколько байт ещё не прочитано.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
use crate::text_codec;
use crate::types::*;

pub(crate) mod key {
````

на:

````rust
use crate::text_codec;
use crate::types::*;
use crate::vector::{StrokeCap, StrokeJoin, StrokeStyle, VectorNetwork};

pub(crate) mod key {
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
    pub const FILLS: &str = "fills";
    pub const STROKES: &str = "strokes";
}

````

на:

````rust
    pub const FILLS: &str = "fills";
    pub const STROKES: &str = "strokes";
    pub const STROKE_CAP_START: &str = "strokeCapStart";
    pub const STROKE_CAP_END: &str = "strokeCapEnd";
    pub const STROKE_JOIN: &str = "strokeJoin";
    pub const STROKE_MITER_LIMIT: &str = "strokeMiterLimit";
    /// Сеть узла `vector` — двоичное значение (`VectorNetwork::encode`).
    pub const VECTOR: &str = "vector";
}

````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
fn bool_or(map: &LoroMap, k: &str, default: bool) -> bool {
    value(map, k).as_ref().and_then(as_bool).unwrap_or(default)
}

````

на:

````rust
fn bool_or(map: &LoroMap, k: &str, default: bool) -> bool {
    value(map, k).as_ref().and_then(as_bool).unwrap_or(default)
}

/// Стиль обводки: неизвестное окончание или соединение (из будущей сборки) —
/// умолчание, предел острого угла — в допустимых пределах.
fn read_stroke_style(map: &LoroMap) -> StrokeStyle {
    let d = StrokeStyle::default();
    let s = |k| value(map, k).as_ref().and_then(as_str);
    let limit = f64_or(map, key::STROKE_MITER_LIMIT, d.miter_limit);
    StrokeStyle {
        cap_start: s(key::STROKE_CAP_START)
            .and_then(|v| StrokeCap::parse(&v))
            .unwrap_or(d.cap_start),
        cap_end: s(key::STROKE_CAP_END)
            .and_then(|v| StrokeCap::parse(&v))
            .unwrap_or(d.cap_end),
        join: s(key::STROKE_JOIN)
            .and_then(|v| StrokeJoin::parse(&v))
            .unwrap_or(d.join),
        miter_limit: if limit.is_finite() {
            limit.clamp(
                *crate::MITER_LIMIT_RANGE.start(),
                *crate::MITER_LIMIT_RANGE.end(),
            )
        } else {
            d.miter_limit
        },
    }
}

/// Сеть узла `vector`; нет ключа — пустая сеть. Неверная — ошибка: узел
/// скрывается, документ — только для чтения (`PageDoc::rebuild_from_loro`).
fn read_vector(map: &LoroMap, id: &NodeId) -> DocResult<VectorNetwork> {
    match value(map, key::VECTOR) {
        None => Ok(VectorNetwork::default()),
        Some(LoroValue::Binary(b)) => {
            VectorNetwork::decode(&b).map_err(|e| DocError::InvalidVector(id.to_string(), e))
        }
        Some(_) => Err(DocError::InvalidVector(
            id.to_string(),
            crate::vector::VectorError::Invalid("не двоичное значение"),
        )),
    }
}

````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
            None => d.strokes,
        },
        text: (kind == NodeKind::Text).then(|| {
            Arc::new(TextData {
````

на:

````rust
            None => d.strokes,
        },
        stroke_style: read_stroke_style(map),
        text: (kind == NodeKind::Text).then(|| {
            Arc::new(TextData {
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
            })
        }),
    })
}
````

на:

````rust
            })
        }),
        vector: match kind {
            NodeKind::Vector => Some(Arc::new(read_vector(map, id)?)),
            _ => None,
        },
    })
}
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
        // Содержимое пишется ниже, в новый контейнер.
        content: None,
    };
    write_patch(map, &patch)?;
````

на:

````rust
        // Содержимое пишется ниже, в новый контейнер.
        content: None,
        stroke_cap_start: Some(p.stroke_style.cap_start).filter(|v| *v != StrokeCap::None),
        stroke_cap_end: Some(p.stroke_style.cap_end).filter(|v| *v != StrokeCap::None),
        stroke_join: Some(p.stroke_style.join).filter(|v| *v != StrokeJoin::Miter),
        stroke_miter_limit: neutral(p.stroke_style.miter_limit, crate::DEFAULT_MITER_LIMIT),
        vector: p.vector.clone(),
    };
    write_patch(map, &patch)?;
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
        )?;
    }
    // Вызывающий убирает `text` из патча узла не текста (`PageDoc::set_props`).
    if let Some(text) = &p.text {
        text_codec::write_text_props(map, text)?;
    }
    Ok(())
````

на:

````rust
        )?;
    }
    for (k, v) in [
        (
            key::STROKE_CAP_START,
            p.stroke_cap_start.map(StrokeCap::as_str),
        ),
        (key::STROKE_CAP_END, p.stroke_cap_end.map(StrokeCap::as_str)),
        (key::STROKE_JOIN, p.stroke_join.map(StrokeJoin::as_str)),
    ] {
        if let Some(v) = v {
            map.insert(k, v)?;
        }
    }
    if let Some(v) = p.stroke_miter_limit {
        map.insert(key::STROKE_MITER_LIMIT, v)?;
    }
    // Вызывающий убирает `text` и `vector` из патча узла другого типа
    // (`PageDoc::set_props`).
    if let Some(text) = &p.text {
        text_codec::write_text_props(map, text)?;
    }
    if let Some(v) = &p.vector {
        map.insert(key::VECTOR, LoroValue::Binary(v.encode().into()))?;
    }
    Ok(())
````

В `crates/ferrum-doc/src/container.rs` заменить:

````rust
/// таблицу без флагов рядом со снапшотом, где они есть, а зеркало страницы
/// берётся из таблицы — флаги тихо пропали бы и из правок.
pub const SCENE_CACHE_VERSION: u8 = 3;

const ROOT: u8 = 1;
````

на:

````rust
/// таблицу без флагов рядом со снапшотом, где они есть, а зеркало страницы
/// берётся из таблицы — флаги тихо пропали бы и из правок.
/// 3 — текст, 4 — стиль обводки и векторная сеть.
pub const SCENE_CACHE_VERSION: u8 = 4;

const ROOT: u8 = 1;
````

В `crates/ferrum-doc/src/error.rs` заменить:

````rust
    #[error("Узел {0} хранит неизвестный тип «{1}»")]
    UnknownKind(String, String),
    #[error("Нельзя переместить узел внутрь самого себя")]
    CyclicMove,
````

на:

````rust
    #[error("Узел {0} хранит неизвестный тип «{1}»")]
    UnknownKind(String, String),
    /// Сеть узла `vector` из файла не прошла проверку: узел скрывается, как узел
    /// неизвестного типа, и документ открывается только для чтения.
    #[error("Узел {0}: {1}")]
    InvalidVector(String, crate::vector::VectorError),
    #[error("Вектор длиннее 100 000 точек — правка не применена")]
    VectorTooLarge,
    #[error("Нельзя переместить узел внутрь самого себя")]
    CyclicMove,
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
mod text_codec;
mod types;

pub use changes::Changes;
````

на:

````rust
mod text_codec;
mod types;
mod vector;

pub use changes::Changes;
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
pub use text::*;
pub use types::*;

#[doc(hidden)]
````

на:

````rust
pub use text::*;
pub use types::*;
pub use vector::*;

#[doc(hidden)]
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
use crate::text_codec;
use crate::types::*;

const TREE: &str = "nodes";
const KEY_ID: &str = "id";

/// Узел, сохранённый перед удалением, чтобы восстановить его отменой.
````

на:

````rust
use crate::text_codec;
use crate::types::*;
use crate::vector::{VectorError, VectorNetwork};

const TREE: &str = "nodes";
const KEY_ID: &str = "id";

/// Сеть, которую пишет правка: сверх 100 000 точек — отказ с сообщением, иначе
/// неверная сеть (ошибка редактора) не попадает в документ.
fn check_vector(v: &VectorNetwork) -> DocResult<()> {
    match v.validate() {
        Ok(()) => Ok(()),
        Err(VectorError::TooManyPoints) => Err(DocError::VectorTooLarge),
        Err(e) => Err(DocError::InvalidVector("—".into(), e)),
    }
}

/// Узел, сохранённый перед удалением, чтобы восстановить его отменой.
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
            let props = match codec::read_props(&self.tree.get_meta(tid)?, &id) {
                Ok(props) => props,
                Err(DocError::UnknownKind(..)) => {
                    unknown = true;
                    self.collect_hidden_refs(tid, &mut hidden_refs)?;
````

на:

````rust
            let props = match codec::read_props(&self.tree.get_meta(tid)?, &id) {
                Ok(props) => props,
                // Неверная сеть вектора (чужая сборка, повреждение) — как
                // неизвестный тип: узел скрыт, документ только для чтения.
                Err(DocError::UnknownKind(..) | DocError::InvalidVector(..)) => {
                    unknown = true;
                    self.collect_hidden_refs(tid, &mut hidden_refs)?;
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
            .expect("текст узла");
        t.insert(0, text).expect("вставка в Loro");
        self.doc.commit();
    }
````

на:

````rust
            .expect("текст узла");
        t.insert(0, text).expect("вставка в Loro");
        self.doc.commit();
    }

    /// Только для тестов: значение `vector` узла в Loro как есть (как в файле
    /// чужой сборки) — без проверки и без зеркала.
    #[doc(hidden)]
    pub fn force_vector_bytes_for_tests(&self, id: NodeId, bytes: &[u8]) {
        let tid = self.tid(id).expect("узел есть");
        let meta = self.tree.get_meta(tid).expect("карта узла");
        meta.insert(codec::key::VECTOR, LoroValue::Binary(bytes.to_vec().into()))
            .expect("запись в Loro");
        self.doc.commit();
    }
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        props: &Props,
    ) -> DocResult<NodeId> {
        // Пределы текста — до создания узла: отказ не оставляет ни узла, ни шага.
        if let Some(t) = &props.text {
            t.content.check_limits()?;
        }
        let tid = self.create_tree_node(parent, index)?;
````

на:

````rust
        props: &Props,
    ) -> DocResult<NodeId> {
        // Пределы текста и сети — до создания узла: отказ не оставляет ни узла,
        // ни шага.
        if let Some(t) = &props.text {
            t.content.check_limits()?;
        }
        if let Some(v) = &props.vector {
            check_vector(v)?;
        }
        let tid = self.create_tree_node(parent, index)?;
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        let tid = self.tid(id)?;
        let before = self.props(id)?;
        // Свойства текста есть только у текста: у остальных узлов их не пишем.
        let stripped;
        let patch = if before.text.is_none() && patch.text.is_some() {
            stripped = PropsPatch {
                text: None,
                ..patch.clone()
            };
````

на:

````rust
        let tid = self.tid(id)?;
        let before = self.props(id)?;
        // Свойства текста есть только у текста, сеть — только у вектора: у
        // остальных узлов их не пишем.
        let stripped;
        let foreign_text = before.text.is_none() && patch.text.is_some();
        let foreign_vector = before.vector.is_none() && patch.vector.is_some();
        let patch = if foreign_text || foreign_vector {
            stripped = PropsPatch {
                text: if foreign_text {
                    None
                } else {
                    patch.text.clone()
                },
                vector: if foreign_vector {
                    None
                } else {
                    patch.vector.clone()
                },
                ..patch.clone()
            };
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
            patch
        };
        let mut after = before.clone();
        patch.apply_to(&mut after);
````

на:

````rust
            patch
        };
        if let Some(v) = &patch.vector {
            check_vector(v)?;
        }

        let mut after = before.clone();
        patch.apply_to(&mut after);
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
use crate::text::*;
use crate::types::*;

/// Узел страницы в порядке обхода в глубину: родитель всегда раньше детей.
````

на:

````rust
use crate::text::*;
use crate::types::*;
use crate::vector::*;

/// Узел страницы в порядке обхода в глубину: родитель всегда раньше детей.
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
        // обводок — свойства текста и содержимое (`write_text`).
        NodeKind::Text => 6,
    }
}
````

на:

````rust
        // обводок — свойства текста и содержимое (`write_text`).
        NodeKind::Text => 6,
        // Четвёртая версия (`SCENE_CACHE_VERSION = 4`): у каждой строки после
        // обводок — стиль обводки, у вектора после него — сеть (`write_vector`).
        NodeKind::Vector => 7,
    }
}
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
        NodeKind::Image,
        NodeKind::Text,
    ]
    .get(c as usize)
````

на:

````rust
        NodeKind::Image,
        NodeKind::Text,
        NodeKind::Vector,
    ]
    .get(c as usize)
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
            });
        }
        if let Some(t) = &p.text {
            write_text(&mut w, t)?;
        }
    }
````

на:

````rust
            });
        }
        let s = &p.stroke_style;
        w.u8(code(&StrokeCap::ALL, s.cap_start));
        w.u8(code(&StrokeCap::ALL, s.cap_end));
        w.u8(code(&StrokeJoin::ALL, s.join));
        w.f64(s.miter_limit);
        if let Some(t) = &p.text {
            write_text(&mut w, t)?;
        }
        if let Some(v) = &p.vector {
            let bytes = v.encode();
            w.u32(u32::try_from(bytes.len()).ok()?);
            w.bytes(&bytes);
        }
    }
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
}

fn write_fill(w: &mut Writer, f: &Fill) -> Option<()> {
    w.u8(match f.kind {
        FillKind::Solid => 0,
````

на:

````rust
}

pub(crate) fn write_fill(w: &mut Writer, f: &Fill) -> Option<()> {
    w.u8(match f.kind {
        FillKind::Solid => 0,
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
}

fn read_fill(r: &mut Reader) -> Option<Fill> {
    let kind = if r.u8()? == 1 {
        FillKind::Image
````

на:

````rust
}

pub(crate) fn read_fill(r: &mut Reader) -> Option<Fill> {
    let kind = if r.u8()? == 1 {
        FillKind::Image
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
            });
        }
        let text = match kind {
            NodeKind::Text => Some(Arc::new(read_text(&mut r)?)),
            _ => None,
        };
````

на:

````rust
            });
        }
        let stroke_style = StrokeStyle {
            cap_start: decode_enum(&StrokeCap::ALL, r.u8()?)?,
            cap_end: decode_enum(&StrokeCap::ALL, r.u8()?)?,
            join: decode_enum(&StrokeJoin::ALL, r.u8()?)?,
            miter_limit: Some(r.f64()?).filter(|v| MITER_LIMIT_RANGE.contains(v))?,
        };
        let text = match kind {
            NodeKind::Text => Some(Arc::new(read_text(&mut r)?)),
            _ => None,
        };
        // Сеть из файла — те же проверки, что у читателя Loro; неверная —
        // кэш отбрасывается, и страница читается из Loro (узел там скрывается).
        let vector = match kind {
            NodeKind::Vector => {
                let n = r.u32()? as usize;
                Some(Arc::new(VectorNetwork::decode(r.take(n)?).ok()?))
            }
            _ => None,
        };
````

В `crates/ferrum-doc/src/scene_cache.rs` заменить:

````rust
                fills,
                strokes,
                text,
            },
        });
````

на:

````rust
                fills,
                strokes,
                stroke_style,
                text,
                vector,
            },
        });
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
use crate::error::DocError;
use crate::text::{TextContent, TextData, TextProps, TEXT_COLOR};

/// Идентификатор узла. Обёртка над `TreeID` Loro: уникален и при совместной работе.
````

на:

````rust
use crate::error::DocError;
use crate::text::{TextContent, TextData, TextProps, TEXT_COLOR};
use crate::vector::{StrokeCap, StrokeJoin, StrokeStyle, VectorNetwork};

/// Идентификатор узла. Обёртка над `TreeID` Loro: уникален и при совместной работе.
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    Image,
    Text,
}

````

на:

````rust
    Image,
    Text,
    /// Векторная сеть (спека подсистемы 3, раздел 9).
    Vector,
}

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            NodeKind::Image => "image",
            NodeKind::Text => "text",
        }
    }
````

на:

````rust
            NodeKind::Image => "image",
            NodeKind::Text => "text",
            NodeKind::Vector => "vector",
        }
    }
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            "image" => NodeKind::Image,
            "text" => NodeKind::Text,
            _ => return None,
        })
````

на:

````rust
            "image" => NodeKind::Image,
            "text" => NodeKind::Text,
            "vector" => NodeKind::Vector,
            _ => return None,
        })
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            NodeKind::Image => "Изображение",
            NodeKind::Text => "Текст",
        }
    }
````

на:

````rust
            NodeKind::Image => "Изображение",
            NodeKind::Text => "Текст",
            NodeKind::Vector => "Вектор",
        }
    }
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    pub fills: Vec<Fill>,
    pub strokes: Vec<Stroke>,
    /// Текст и его свойства — только у `NodeKind::Text`.
    pub text: Option<Arc<TextData>>,
}

````

на:

````rust
    pub fills: Vec<Fill>,
    pub strokes: Vec<Stroke>,
    /// Окончания, соединение и предел острого угла — общие для всех обводок узла
    /// (как в Figma). Нейтральные — без окончаний, острый стык, предел 4.
    pub stroke_style: StrokeStyle,
    /// Текст и его свойства — только у `NodeKind::Text`.
    pub text: Option<Arc<TextData>>,
    /// Сеть — только у `NodeKind::Vector`. `Arc`: правка свойств узла не копирует
    /// 100 000 точек, а сравнение одной и той же сети — по указателю.
    pub vector: Option<Arc<VectorNetwork>>,
}

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            // Размер пустого текста ставит редактор по раскладке; цвет — чёрный.
            NodeKind::Text => (0.0, 0.0, vec![Fill::solid(TEXT_COLOR)], vec![]),
        };
        Props {
````

на:

````rust
            // Размер пустого текста ставит редактор по раскладке; цвет — чёрный.
            NodeKind::Text => (0.0, 0.0, vec![Fill::solid(TEXT_COLOR)], vec![]),
            // Как у пера Figma: без заливки, чёрная обводка 1 по центру.
            NodeKind::Vector => (
                0.0,
                0.0,
                vec![],
                vec![Stroke {
                    color: Color::BLACK,
                    width: 1.0,
                    align: StrokeAlign::Center,
                }],
            ),
        };
        Props {
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            fills,
            strokes,
            text: (kind == NodeKind::Text).then(|| Arc::new(TextData::default())),
        }
    }
````

на:

````rust
            fills,
            strokes,
            stroke_style: StrokeStyle::default(),
            text: (kind == NodeKind::Text).then(|| Arc::new(TextData::default())),
            vector: (kind == NodeKind::Vector).then(|| Arc::new(VectorNetwork::default())),
        }
    }
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
    /// панели. У не текста пропускается.
    pub content: Option<TextContent>,
}

````

на:

````rust
    /// панели. У не текста пропускается.
    pub content: Option<TextContent>,
    /// Окончания обводки в начале и в конце открытых путей, соединение и предел
    /// острого угла — ключами узла по отдельности (совместная правка не затирает
    /// соседний ключ).
    pub stroke_cap_start: Option<StrokeCap>,
    pub stroke_cap_end: Option<StrokeCap>,
    pub stroke_join: Option<StrokeJoin>,
    pub stroke_miter_limit: Option<f64>,
    /// Сеть целиком (значение узла заменяется). У не вектора пропускается.
    pub vector: Option<Arc<VectorNetwork>>,
}

````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            content: match (&self.content, &before.text) {
                (Some(_), Some(t)) => Some(t.content.clone()),
                _ => None,
            },
````

на:

````rust
            content: match (&self.content, &before.text) {
                (Some(_), Some(t)) => Some(t.content.clone()),
                _ => None,
            },
            stroke_cap_start: self.stroke_cap_start.map(|_| before.stroke_style.cap_start),
            stroke_cap_end: self.stroke_cap_end.map(|_| before.stroke_style.cap_end),
            stroke_join: self.stroke_join.map(|_| before.stroke_style.join),
            stroke_miter_limit: self
                .stroke_miter_limit
                .map(|_| before.stroke_style.miter_limit),
            vector: match (&self.vector, &before.vector) {
                (Some(_), Some(v)) => Some(v.clone()),
                _ => None,
            },
````

В `crates/ferrum-doc/src/types.rs` заменить:

````rust
            p.strokes = v.clone();
        }
        if let Some(t) = &mut p.text {
            if self.text.is_some() || self.content.is_some() {
````

на:

````rust
            p.strokes = v.clone();
        }
        let s = &mut p.stroke_style;
        if let Some(v) = self.stroke_cap_start {
            s.cap_start = v;
        }
        if let Some(v) = self.stroke_cap_end {
            s.cap_end = v;
        }
        if let Some(v) = self.stroke_join {
            s.join = v;
        }
        if let Some(v) = self.stroke_miter_limit {
            s.miter_limit = v;
        }
        if let (Some(v), Some(dst)) = (&self.vector, &mut p.vector) {
            *dst = v.clone();
        }

        if let Some(t) = &mut p.text {
            if self.text.is_some() || self.content.is_some() {
````

Создать `crates/ferrum-doc/src/vector.rs`:

````rust
//! Векторная сеть (спека подсистемы 3, раздел 9): точки, сегменты между ними и
//! области — петли сегментов со своим правилом заливки и, если заданы, своими
//! заливками. Как в Figma, сеть не обязана быть набором контуров: в точке
//! сходится сколько угодно сегментов.
//!
//! В документе сеть — одно значение узла (ключ `vector`, двоичная форма ниже), а
//! не дерево контейнеров Loro: правка сети заменяет значение целиком (как список
//! заливок), а 100 000 точек в контейнерах раздули бы файл в разы. Координаты —
//! в осях узла; рамка узла (`w × h`) — границы сети, а ресайз узла растягивает
//! сеть под рамку (`ferrum_scene::vector`).
//!
//! Данные из файла и буфера обмена не доверенные: `decode` проверяет пределы,
//! числа, индексы и то, что каждая петля области — замкнутый цикл сегментов.

use crate::binary::{Reader, Writer};
use crate::scene_cache::{read_fill, write_fill};
use crate::types::{Fill, FillKind};

/// Предел точек сети (спека 9).
pub const MAX_VECTOR_POINTS: usize = 100_000;
/// Предел сегментов: в сети точка может соединяться с любым числом соседей, но
/// больше двух сегментов на точку в среднем не бывает ни у контуров, ни у сеток.
pub const MAX_VECTOR_SEGMENTS: usize = 2 * MAX_VECTOR_POINTS;
/// Предел областей и петель (суммарно по сети).
pub const MAX_VECTOR_LOOPS: usize = 10_000;
/// Предел заливок одной области (как у узла).
pub const MAX_REGION_FILLS: usize = 64;
/// Предел координат и ручек (как у узлов сцены, `ferrum_scene::shape::COORD_LIMIT`).
pub const MAX_VECTOR_COORD: f64 = 1.0e9;
/// Версия двоичной формы.
const FORMAT: u8 = 1;

/// Режим ручек точки, как в Figma: без ручек, зеркально по углу (длины свои),
/// зеркально по углу и длине, независимые.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleMode {
    None,
    MirrorAngle,
    MirrorAll,
    Free,
}

const HANDLE_MODES: [HandleMode; 4] = [
    HandleMode::None,
    HandleMode::MirrorAngle,
    HandleMode::MirrorAll,
    HandleMode::Free,
];

/// Правило заливки области.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorPoint {
    pub x: f64,
    pub y: f64,
    pub handle_mode: HandleMode,
    /// Радиус скругления угла в этой точке (хранится; рисование — позже, раздел
    /// «Что этап 3 не делает» плана).
    pub corner_radius: f64,
}

impl VectorPoint {
    pub fn new(x: f64, y: f64) -> VectorPoint {
        VectorPoint {
            x,
            y,
            handle_mode: HandleMode::None,
            corner_radius: 0.0,
        }
    }
}

/// Сегмент от точки `start` к точке `end`. Ручки — смещения от своих точек:
/// нулевые обе — отрезок, иначе кубическая кривая
/// `start → start + tangent_start → end + tangent_end → end`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorSegment {
    pub start: u32,
    pub end: u32,
    pub tangent_start: [f64; 2],
    pub tangent_end: [f64; 2],
}

impl VectorSegment {
    pub fn line(start: u32, end: u32) -> VectorSegment {
        VectorSegment {
            start,
            end,
            tangent_start: [0.0; 2],
            tangent_end: [0.0; 2],
        }
    }

    /// Прямой ли сегмент (обе ручки нулевые).
    pub fn is_straight(&self) -> bool {
        self.tangent_start == [0.0; 2] && self.tangent_end == [0.0; 2]
    }
}

/// Область: петли (каждая — цикл сегментов по индексам) с правилом заливки.
/// `fills = None` — заливки узла (как `VectorRegion.fills` в Figma).
#[derive(Debug, Clone, PartialEq)]
pub struct VectorRegion {
    pub rule: FillRule,
    pub loops: Vec<Vec<u32>>,
    pub fills: Option<Vec<Fill>>,
}

/// Сеть узла `Vector`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VectorNetwork {
    pub points: Vec<VectorPoint>,
    pub segments: Vec<VectorSegment>,
    pub regions: Vec<VectorRegion>,
}

/// Почему сеть не принята.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VectorError {
    #[error("в векторе больше 100 000 точек")]
    TooManyPoints,
    #[error("в векторе слишком много сегментов или областей")]
    TooLarge,
    #[error("векторная сеть повреждена: {0}")]
    Invalid(&'static str),
}

fn finite_coord(v: f64) -> bool {
    v.is_finite() && v.abs() <= MAX_VECTOR_COORD
}

impl VectorNetwork {
    /// Степень точки: сколько сегментов в ней сходится.
    pub fn degrees(&self) -> Vec<u32> {
        let mut d = vec![0u32; self.points.len()];
        for s in &self.segments {
            d[s.start as usize] += 1;
            d[s.end as usize] += 1;
        }
        d
    }

    /// Точки петли по порядку обхода (первая не повторяется в конце); `None` —
    /// сегменты не образуют замкнутый цикл.
    pub fn loop_points(&self, segs: &[u32]) -> Option<Vec<u32>> {
        let first = self.segments.get(*segs.first()? as usize)?;
        // Направление первого сегмента — то, при котором второй к нему примыкает.
        'orient: for (from, to) in [(first.start, first.end), (first.end, first.start)] {
            let mut out = vec![from];
            let mut cur = to;
            for &i in &segs[1..] {
                let s = self.segments.get(i as usize)?;
                out.push(cur);
                cur = if s.start == cur {
                    s.end
                } else if s.end == cur {
                    s.start
                } else {
                    continue 'orient;
                };
            }
            if cur == from && out.len() >= 2 {
                return Some(out);
            }
        }
        None
    }

    /// Проверка пределов и связности: числа конечные, индексы в сети, петли
    /// областей замкнуты, заливки областей — только цвет.
    pub fn validate(&self) -> Result<(), VectorError> {
        if self.points.len() > MAX_VECTOR_POINTS {
            return Err(VectorError::TooManyPoints);
        }
        let loops: usize = self.regions.iter().map(|r| r.loops.len()).sum();
        if self.segments.len() > MAX_VECTOR_SEGMENTS || loops > MAX_VECTOR_LOOPS {
            return Err(VectorError::TooLarge);
        }
        for p in &self.points {
            if !finite_coord(p.x) || !finite_coord(p.y) {
                return Err(VectorError::Invalid("координата точки"));
            }
            if !(p.corner_radius.is_finite() && (0.0..=MAX_VECTOR_COORD).contains(&p.corner_radius))
            {
                return Err(VectorError::Invalid("радиус точки"));
            }
        }
        let n = self.points.len() as u32;
        for s in &self.segments {
            if s.start >= n || s.end >= n || s.start == s.end {
                return Err(VectorError::Invalid("концы сегмента"));
            }
            if !s
                .tangent_start
                .iter()
                .chain(&s.tangent_end)
                .all(|v| finite_coord(*v))
            {
                return Err(VectorError::Invalid("ручка сегмента"));
            }
        }
        for r in &self.regions {
            if r.loops.is_empty() {
                return Err(VectorError::Invalid("пустая область"));
            }
            for l in &r.loops {
                if l.len() > self.segments.len() || self.loop_points(l).is_none() {
                    return Err(VectorError::Invalid("петля области не замкнута"));
                }
            }
            if let Some(fills) = &r.fills {
                if fills.len() > MAX_REGION_FILLS {
                    return Err(VectorError::TooLarge);
                }
                if fills
                    .iter()
                    .any(|f| f.kind != FillKind::Solid || !(0.0..=1.0).contains(&f.opacity))
                {
                    return Err(VectorError::Invalid("заливка области"));
                }
            }
        }
        Ok(())
    }

    /// Двоичная форма для документа и кэша сцены.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(FORMAT);
        w.u32(self.points.len() as u32);
        for p in &self.points {
            w.f64(p.x);
            w.f64(p.y);
            w.u8(HANDLE_MODES
                .iter()
                .position(|m| *m == p.handle_mode)
                .unwrap_or(0) as u8);
            w.f64(p.corner_radius);
        }
        w.u32(self.segments.len() as u32);
        for s in &self.segments {
            w.u32(s.start);
            w.u32(s.end);
            for v in s.tangent_start.iter().chain(&s.tangent_end) {
                w.f64(*v);
            }
        }
        w.u32(self.regions.len() as u32);
        for r in &self.regions {
            w.u8(match r.rule {
                FillRule::NonZero => 0,
                FillRule::EvenOdd => 1,
            });
            w.u32(r.loops.len() as u32);
            for l in &r.loops {
                w.u32(l.len() as u32);
                for i in l {
                    w.u32(*i);
                }
            }
            match &r.fills {
                None => w.u8(0),
                Some(fills) => {
                    w.u8(1);
                    w.u8(fills.len().min(u8::MAX as usize) as u8);
                    for f in fills.iter().take(u8::MAX as usize) {
                        // Хэша у заливки области нет: строка пустая.
                        let _ = write_fill(&mut w, f);
                    }
                }
            }
        }
        w.buf
    }

    /// Сеть из двоичной формы — не доверенной: длины сверяются с остатком байтов
    /// до выделения памяти, затем `validate`.
    pub fn decode(bytes: &[u8]) -> Result<VectorNetwork, VectorError> {
        let bad = VectorError::Invalid("двоичная форма");
        let mut r = Reader::new(bytes);
        if r.u8() != Some(FORMAT) {
            return Err(VectorError::Invalid("версия формы"));
        }
        let left = |r: &Reader, per: usize| r.remaining() / per;
        let n = r.u32().ok_or(bad.clone())? as usize;
        if n > MAX_VECTOR_POINTS {
            return Err(VectorError::TooManyPoints);
        }
        if n > left(&r, 25) {
            return Err(bad);
        }
        let mut points = Vec::with_capacity(n);
        for _ in 0..n {
            let (x, y) = (r.f64().ok_or(bad.clone())?, r.f64().ok_or(bad.clone())?);
            let handle_mode = *HANDLE_MODES
                .get(r.u8().ok_or(bad.clone())? as usize)
                .ok_or(bad.clone())?;
            let corner_radius = r.f64().ok_or(bad.clone())?;
            points.push(VectorPoint {
                x,
                y,
                handle_mode,
                corner_radius,
            });
        }
        let n = r.u32().ok_or(bad.clone())? as usize;
        if n > MAX_VECTOR_SEGMENTS {
            return Err(VectorError::TooLarge);
        }
        if n > left(&r, 40) {
            return Err(bad);
        }
        let mut segments = Vec::with_capacity(n);
        for _ in 0..n {
            let (start, end) = (r.u32().ok_or(bad.clone())?, r.u32().ok_or(bad.clone())?);
            let mut t = [0.0; 4];
            for v in &mut t {
                *v = r.f64().ok_or(bad.clone())?;
            }
            segments.push(VectorSegment {
                start,
                end,
                tangent_start: [t[0], t[1]],
                tangent_end: [t[2], t[3]],
            });
        }
        let n = r.u32().ok_or(bad.clone())? as usize;
        if n > MAX_VECTOR_LOOPS || n > left(&r, 6) {
            return Err(VectorError::TooLarge);
        }
        let mut regions = Vec::with_capacity(n);
        let mut loops_total = 0usize;
        for _ in 0..n {
            let rule = match r.u8().ok_or(bad.clone())? {
                0 => FillRule::NonZero,
                1 => FillRule::EvenOdd,
                _ => return Err(bad),
            };
            let k = r.u32().ok_or(bad.clone())? as usize;
            loops_total += k;
            if loops_total > MAX_VECTOR_LOOPS || k > left(&r, 4) {
                return Err(VectorError::TooLarge);
            }
            let mut loops = Vec::with_capacity(k);
            for _ in 0..k {
                let len = r.u32().ok_or(bad.clone())? as usize;
                if len > MAX_VECTOR_SEGMENTS || len > left(&r, 4) {
                    return Err(VectorError::TooLarge);
                }
                let mut l = Vec::with_capacity(len);
                for _ in 0..len {
                    l.push(r.u32().ok_or(bad.clone())?);
                }
                loops.push(l);
            }
            let fills = match r.u8().ok_or(bad.clone())? {
                0 => None,
                1 => {
                    let m = r.u8().ok_or(bad.clone())? as usize;
                    let mut fills = Vec::with_capacity(m);
                    for _ in 0..m {
                        fills.push(read_fill(&mut r).ok_or(bad.clone())?);
                    }
                    Some(fills)
                }
                _ => return Err(bad),
            };
            regions.push(VectorRegion { rule, loops, fills });
        }
        if r.remaining() != 0 {
            return Err(bad);
        }
        let net = VectorNetwork {
            points,
            segments,
            regions,
        };
        net.validate()?;
        Ok(net)
    }

    /// Строка для буфера обмена (base64 двоичной формы).
    pub fn to_base64(&self) -> String {
        base64_encode(&self.encode())
    }

    /// Сеть из строки буфера обмена (не доверенной).
    pub fn from_base64(s: &str) -> Result<VectorNetwork, VectorError> {
        let bytes = base64_decode(s).ok_or(VectorError::Invalid("base64"))?;
        VectorNetwork::decode(&bytes)
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.as_bytes();
    if !s.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for chunk in s.chunks(4) {
        let mut n = 0u32;
        let mut pad = 0;
        for (i, &c) in chunk.iter().enumerate() {
            let v = match c {
                b'=' if i >= 2 => {
                    pad += 1;
                    0
                }
                _ if pad > 0 => return None,
                _ => B64.iter().position(|x| *x == c)? as u32,
            };
            n = n << 6 | v;
        }
        let b = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&b[..3 - pad]);
    }
    Some(out)
}

/// Окончание открытого конца обводки (как `strokeCap` в Figma).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrokeCap {
    None,
    Round,
    Square,
    ArrowLines,
    ArrowTriangle,
    Circle,
    Diamond,
}

impl StrokeCap {
    pub const ALL: [StrokeCap; 7] = [
        StrokeCap::None,
        StrokeCap::Round,
        StrokeCap::Square,
        StrokeCap::ArrowLines,
        StrokeCap::ArrowTriangle,
        StrokeCap::Circle,
        StrokeCap::Diamond,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            StrokeCap::None => "none",
            StrokeCap::Round => "round",
            StrokeCap::Square => "square",
            StrokeCap::ArrowLines => "arrowLines",
            StrokeCap::ArrowTriangle => "arrowTriangle",
            StrokeCap::Circle => "circle",
            StrokeCap::Diamond => "diamond",
        }
    }

    pub fn parse(s: &str) -> Option<StrokeCap> {
        StrokeCap::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

/// Соединение сегментов обводки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrokeJoin {
    Miter,
    Round,
    Bevel,
}

impl StrokeJoin {
    pub const ALL: [StrokeJoin; 3] = [StrokeJoin::Miter, StrokeJoin::Round, StrokeJoin::Bevel];

    pub fn as_str(self) -> &'static str {
        match self {
            StrokeJoin::Miter => "miter",
            StrokeJoin::Round => "round",
            StrokeJoin::Bevel => "bevel",
        }
    }

    pub fn parse(s: &str) -> Option<StrokeJoin> {
        StrokeJoin::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

/// Предел острого угла по умолчанию (как у `kurbo::Stroke` и SVG).
pub const DEFAULT_MITER_LIMIT: f64 = 4.0;
/// Допустимый предел острого угла (как в Figma: угол от ~0,01° до 180°).
pub const MITER_LIMIT_RANGE: std::ops::RangeInclusive<f64> = 1.0..=100.0;

/// Стиль обводки узла: окончания начала и конца открытых путей, соединение и
/// предел острого угла. Как в Figma, общий для всех обводок узла.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    pub cap_start: StrokeCap,
    pub cap_end: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f64,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        StrokeStyle {
            cap_start: StrokeCap::None,
            cap_end: StrokeCap::None,
            join: StrokeJoin::Miter,
            miter_limit: DEFAULT_MITER_LIMIT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_every_tail() {
        for n in 0..10u8 {
            let bytes: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
            assert_eq!(base64_decode(&base64_encode(&bytes)), Some(bytes));
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_decode("TQ=="), Some(b"M".to_vec()));
        assert_eq!(base64_decode("T=Q="), None);
        assert_eq!(base64_decode("TQ="), None);
    }
}
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
//! с `false`. Версия формата поэтому не менялась: старая сборка, получив копию с
//! отражением, вставит её неотражённой, но без ошибки и без небезопасных данных.
//!
//! Вставка не доверяет данным: размеры и количества ограничены, длины проверяются
````

на:

````rust
//! с `false`. Версия формата поэтому не менялась: старая сборка, получив копию с
//! отражением, вставит её неотражённой, но без ошибки и без небезопасных данных.
//! Так же добавлены стиль обводки (`strokeCapStart`, `strokeCapEnd`, `strokeJoin`,
//! `strokeMiterLimit` — только не нейтральные) и сеть вектора (`vector` — base64
//! двоичной формы `VectorNetwork`): сборка без векторов пропускает узел «vector»
//! как неизвестный.
//!
//! Вставка не доверяет данным: размеры и количества ограничены, длины проверяются
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust

use ferrum_doc::{
    Color, Fill, FillKind, NodeKind, Props, PropsPatch, ScaleMode, Stroke, StrokeAlign, TextData,
};
use ferrum_scene::shape;
````

на:

````rust

use ferrum_doc::{
    Color, Fill, FillKind, NodeKind, Props, PropsPatch, ScaleMode, Stroke, StrokeAlign, StrokeCap,
    StrokeJoin, StrokeStyle, TextData, VectorNetwork, DEFAULT_MITER_LIMIT, MITER_LIMIT_RANGE,
};
use ferrum_scene::shape;
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

````

на:

````rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stroke_cap_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stroke_cap_end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stroke_join: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stroke_miter_limit: Option<f64>,
    /// Сеть вектора: base64 двоичной формы — только у вектора.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vector: Option<String>,
}

````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
                            .collect(),
                        text: p.text.as_ref().map(|t| t.to_json()),
                    }
                })
````

на:

````rust
                            .collect(),
                        text: p.text.as_ref().map(|t| t.to_json()),
                        stroke_cap_start: Some(p.stroke_style.cap_start)
                            .filter(|c| *c != StrokeCap::None)
                            .map(|c| c.as_str().to_string()),
                        stroke_cap_end: Some(p.stroke_style.cap_end)
                            .filter(|c| *c != StrokeCap::None)
                            .map(|c| c.as_str().to_string()),
                        stroke_join: Some(p.stroke_style.join)
                            .filter(|j| *j != StrokeJoin::Miter)
                            .map(|j| j.as_str().to_string()),
                        stroke_miter_limit: Some(p.stroke_style.miter_limit)
                            .filter(|m| *m != DEFAULT_MITER_LIMIT),
                        vector: p.vector.as_ref().map(|v| v.to_base64()),
                    }
                })
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
        });
    }
    let mut props = Props {
        kind,
````

на:

````rust
        });
    }
    let cap = |s: Option<String>| match s {
        None => Ok(StrokeCap::None),
        Some(s) => StrokeCap::parse(&s).ok_or_else(|| corrupted("окончание обводки")),
    };
    let stroke_style = StrokeStyle {
        cap_start: cap(n.stroke_cap_start)?,
        cap_end: cap(n.stroke_cap_end)?,
        join: match n.stroke_join {
            None => StrokeJoin::Miter,
            Some(s) => StrokeJoin::parse(&s).ok_or_else(|| corrupted("соединение обводки"))?,
        },
        miter_limit: match n.stroke_miter_limit {
            None => DEFAULT_MITER_LIMIT,
            Some(v) if MITER_LIMIT_RANGE.contains(&v) => v,
            Some(_) => return Err(corrupted("предел острого угла")),
        },
    };
    let mut props = Props {
        kind,
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
            )),
            (NodeKind::Text, None) => Some(Arc::new(TextData::default())),
            _ => None,
        },
````

на:

````rust
            )),
            (NodeKind::Text, None) => Some(Arc::new(TextData::default())),
            _ => None,
        },
        stroke_style,
        // Сеть — только у вектора; пределы и связность проверяет `from_base64`.
        vector: match (kind, n.vector.as_deref()) {
            (NodeKind::Vector, Some(s)) => Some(Arc::new(
                VectorNetwork::from_base64(s).map_err(|e| corrupted(e.to_string()))?,
            )),
            (NodeKind::Vector, None) => Some(Arc::new(VectorNetwork::default())),
            _ => None,
        },
````

В `crates/ferrum-editor/src/clipboard.rs` заменить:

````rust
        text: p.text.as_ref().map(|t| t.props.clone()),
        content: None,
    }
}
````

на:

````rust
        text: p.text.as_ref().map(|t| t.props.clone()),
        content: None,
        // Нейтральные значения у нового узла уже есть: не пишутся.
        stroke_cap_start: Some(p.stroke_style.cap_start).filter(|c| *c != StrokeCap::None),
        stroke_cap_end: Some(p.stroke_style.cap_end).filter(|c| *c != StrokeCap::None),
        stroke_join: Some(p.stroke_style.join).filter(|j| *j != StrokeJoin::Miter),
        stroke_miter_limit: Some(p.stroke_style.miter_limit).filter(|m| *m != DEFAULT_MITER_LIMIT),
        vector: p.vector.clone(),
    }
}
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
        // Рамка текста; попадание — по ней (спека подсистемы 3, раздел 5).
        NodeKind::Text => Geometry::Rect { rect, radius: 0.0 },
    }
}
````

на:

````rust
        // Рамка текста; попадание — по ней (спека подсистемы 3, раздел 5).
        NodeKind::Text => Geometry::Rect { rect, radius: 0.0 },
        // Пока сеть не рисуется (её геометрия — следующая задача), вектор — рамка.
        NodeKind::Vector => Geometry::Rect { rect, radius: 0.0 },
    }
}
````

В `crates/ferrum-wasm/src/layers.rs` заменить:

````rust
    Image,
    Text,
}

````

на:

````rust
    Image,
    Text,
    Vector,
}

````

В `crates/ferrum-wasm/src/layers.rs` заменить:

````rust
            NodeKind::Image => NodeType::Image,
            NodeKind::Text => NodeType::Text,
        }
    }
````

на:

````rust
            NodeKind::Image => NodeType::Image,
            NodeKind::Text => NodeType::Text,
            NodeKind::Vector => NodeType::Vector,
        }
    }
````

В `scripts/compat-old-build.sh` заменить:

````
#!/bin/sh
# Совместимость (спека подсистемы 3, раздел 12): файл с текстом, собранный новой
# сборкой, открывается сборкой без текста (по умолчанию — база ветки, 2710c85) только для
# чтения: прямоугольник виден, текст — узел неизвестного типа — скрыт, правка
# отвергается. Старая сборка собирается во временной копии репозитория.
#   scripts/compat-old-build.sh [ref]
set -eu
REF="${1:-2710c85}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
FILE="$TMP/text.ferrum"
cleanup() {
  git -C "$ROOT" worktree remove --force "$TMP/old" >/dev/null 2>&1 || true
````

на:

````
#!/bin/sh
# Совместимость (спека подсистемы 3, разделы 3, 9 и 12): файл с узлом нового вида,
# собранный новой сборкой, открывается сборкой без этого вида только для чтения:
# прямоугольник виден, новый узел — узел неизвестного типа — скрыт, правка
# отвергается. Старая сборка собирается во временной копии репозитория.
#   scripts/compat-old-build.sh [ref] [fixture]
# fixture — пример `ferrum-doc`: `text_fixture` (по умолчанию; ref — сборка без
# текста, 2710c85) или `vector_fixture` (ref — сборка без векторов, например main
# до этапа 3).
set -eu
REF="${1:-2710c85}"
FIXTURE="${2:-text_fixture}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
FILE="$TMP/fixture.ferrum"
cleanup() {
  git -C "$ROOT" worktree remove --force "$TMP/old" >/dev/null 2>&1 || true
````

В `scripts/compat-old-build.sh` заменить:

````
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

на:

````
}
trap cleanup EXIT
(cd "$ROOT" && cargo run -q -p ferrum-doc --example "$FIXTURE" -- "$FILE")
git -C "$ROOT" worktree add --detach "$TMP/old" "$REF" >/dev/null
cat > "$TMP/old/crates/ferrum-doc/tests/compat_fixture.rs" <<'RS'
use ferrum_doc::container::decode;
use ferrum_doc::{DocError, NodeKind};

#[test]
fn file_with_new_kind_opens_read_only() {
    let bytes = std::fs::read(std::env::var("FERRUM_FIXTURE_FILE").unwrap()).unwrap();
    let mut doc = decode(&bytes).unwrap().document;
    let page = doc.pages()[0].0;
    let walk = doc.page(page).unwrap().walk();
    assert_eq!(walk.len(), 1, "виден только прямоугольник");
    assert!(doc.is_read_only(), "новый узел скрыт — документ только для чтения");
    let err = doc.transact(page, vec![], |tx| Ok(vec![tx.create_node(None, None, NodeKind::Rect)?]));
    assert!(matches!(err, Err(DocError::ReadOnly)));
}
RS
(cd "$TMP/old" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}" FERRUM_FIXTURE_FILE="$FILE" \
  cargo test -q -p ferrum-doc --test compat_fixture)
echo "Совместимость с $REF ($FIXTURE): файл открыт только для чтения"
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-doc -p ferrum-scene -p ferrum-editor 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR=$PWD/target && ./scripts/compat-old-build.sh main vector_fixture`

Run: `export CARGO_TARGET_DIR=$PWD/target && ./scripts/compat-old-build.sh`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npm run wasm && npm run typecheck`

Expected: PASS: fmt и clippy без замечаний; `vector` — 7 тестов, `clipboard_format` — 20, остальные тесты `ferrum-doc`, `ferrum-scene`, `ferrum-editor` — как были; «Совместимость с main (vector_fixture): файл открыт только для чтения» и «Совместимость с 2710c85 (text_fixture): …»; `npm run typecheck` без ошибок

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/src/ui/layers.tsx crates/ferrum-doc/examples/vector_fixture.rs crates/ferrum-doc/src/binary.rs crates/ferrum-doc/src/codec.rs crates/ferrum-doc/src/container.rs crates/ferrum-doc/src/error.rs crates/ferrum-doc/src/lib.rs crates/ferrum-doc/src/page.rs crates/ferrum-doc/src/scene_cache.rs crates/ferrum-doc/src/types.rs crates/ferrum-doc/src/vector.rs crates/ferrum-doc/tests/container.rs crates/ferrum-doc/tests/flip.rs crates/ferrum-doc/tests/vector.rs crates/ferrum-editor/src/clipboard.rs crates/ferrum-editor/tests/clipboard_format.rs crates/ferrum-scene/src/shape.rs crates/ferrum-wasm/src/layers.rs scripts/compat-old-build.sh
git commit -m "feat(doc): узел Vector — сеть точек, сегментов и областей значением узла; стиль обводки (окончания, соединение, предел угла); кэш сцены v4, буфер обмена, совместимость

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2: Геометрия сети и обводка по контуру

Спека 9: геометрия — `kurbo`, попадание — по пути (допуск обводки и залитые области). Сеть разбирается на цепочки (через точки с двумя сегментами путь идёт дальше — там соединение) и петли; концы — точки с одним сегментом, их окончание — по тому, начало это сегмента или конец; ветвления — точки с тремя и больше. Границы — точные (экстремумы кривых), сеть растягивается под рамку. Обводка по контуру — у вектора, линии с окончаниями и прямоугольника с соединением не «острым»: контур `kurbo::stroke` (плоские концы) плюс фигуры окончаний и круги ветвлений; внутри и снаружи — двойная толщина с обрезкой (её делает рендер). Попадание: области по их правилу и заливкам, полоса обводки не уже допуска, фигуры окончаний. Сцена считает выступ обводки по стилю и сравнивает сеть по указателю.

**Files:**
- Create: `crates/ferrum-scene/src/vector.rs`
- Modify: `crates/ferrum-scene/src/{shape,scene,lib}.rs`
- Test: `crates/ferrum-scene/tests/vector.rs` (новый)

**Interfaces:**
- Consumes: типы задачи 1.
- Produces: `ferrum_scene::vector::{segment_curve, network_bounds, fit, network_shape, path_shape, uses_path_stroke, stroke_outline, stroke_clip, effective_width, path_stroke_outset, cap_shapes, cap_extent, arrow_length, dot_radius, hit_path, region_contains, distance_to, nearest_segment}`, `PathShape { stroke, regions, ends, junctions, closed }`, `End { point, dir, cap }`, `RegionPath { path, rule, fills }`, `StrokeClip`; `Geometry::Vector(Rect)`; `shape::stroke_outset(&Props)`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-scene/tests/vector.rs`:

````rust
//! Геометрия векторной сети и обводки по контуру (подсистема 3, этап 3): цепочки
//! и петли, концы с окончаниями, растяжение под рамку, границы с экстремумами
//! кривых, попадание по обводке и областям, сцена с вектором.

use std::sync::Arc;

use ferrum_doc::*;
use ferrum_scene::shape;
use ferrum_scene::vector::*;
use ferrum_scene::Scene;
use kurbo::{PathEl, Point, Rect, Shape, Vec2};

fn net(points: &[(f64, f64)], segs: &[(u32, u32)]) -> VectorNetwork {
    VectorNetwork {
        points: points
            .iter()
            .map(|&(x, y)| VectorPoint::new(x, y))
            .collect(),
        segments: segs
            .iter()
            .map(|&(a, b)| VectorSegment::line(a, b))
            .collect(),
        regions: vec![],
    }
}

/// Вектор с рамкой по границам сети (как его пишет редактор).
fn vector(n: VectorNetwork) -> Props {
    let b = network_bounds(&n).unwrap_or(Rect::ZERO);
    Props {
        w: b.width(),
        h: b.height(),
        vector: Some(Arc::new(n)),
        ..Props::defaults(NodeKind::Vector)
    }
}

fn subpaths(shape: &PathShape) -> (usize, usize) {
    let els = shape.stroke.elements();
    let moves = els
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count();
    let closes = els
        .iter()
        .filter(|e| matches!(e, PathEl::ClosePath))
        .count();
    (moves, closes)
}

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-9
}

#[test]
fn open_polyline_is_one_chain_with_capped_ends() {
    let mut p = vector(net(
        &[(0.0, 0.0), (50.0, 0.0), (50.0, 40.0)],
        &[(0, 1), (1, 2)],
    ));
    p.stroke_style.cap_start = StrokeCap::Round;
    p.stroke_style.cap_end = StrokeCap::ArrowLines;
    let s = network_shape(&p);
    assert_eq!(
        subpaths(&s),
        (1, 0),
        "одна открытая цепочка: соединение в средней точке"
    );
    assert!(!s.closed);
    assert_eq!(s.ends.len(), 2);
    let start = s.ends.iter().find(|e| e.cap == StrokeCap::Round).unwrap();
    let end = s
        .ends
        .iter()
        .find(|e| e.cap == StrokeCap::ArrowLines)
        .unwrap();
    assert!(near(start.point, Point::new(0.0, 0.0)));
    assert!(
        (start.dir - Vec2::new(-1.0, 0.0)).hypot() < 1e-9,
        "наружу — против хода"
    );
    assert!(near(end.point, Point::new(50.0, 40.0)));
    assert!(
        (end.dir - Vec2::new(0.0, 1.0)).hypot() < 1e-9,
        "наружу — по ходу"
    );
}

#[test]
fn cycle_is_closed_and_junction_is_marked() {
    let tri = vector(net(
        &[(0.0, 0.0), (40.0, 0.0), (20.0, 30.0)],
        &[(0, 1), (1, 2), (2, 0)],
    ));
    let s = network_shape(&tri);
    assert!(s.closed && s.ends.is_empty());
    assert_eq!(subpaths(&s), (1, 1));
    // Звезда: три луча из центра — точка ветвления и три конца.
    let star = vector(net(
        &[(20.0, 20.0), (0.0, 0.0), (40.0, 0.0), (20.0, 40.0)],
        &[(0, 1), (0, 2), (0, 3)],
    ));
    let s = network_shape(&star);
    assert_eq!(s.junctions, vec![Point::new(20.0, 20.0)]);
    assert_eq!(s.ends.len(), 3);
    assert_eq!(subpaths(&s).0, 3);
}

#[test]
fn network_is_stretched_into_the_frame_and_curve_extrema_count() {
    // Сеть с рамкой (10, 10)–(110, 60) в узле 200 × 100: вдвое по обеим осям.
    let mut p = vector(net(&[(10.0, 10.0), (110.0, 60.0)], &[(0, 1)]));
    p.w = 200.0;
    p.h = 100.0;
    let f = fit(&p);
    assert!(near(f * Point::new(10.0, 10.0), Point::ZERO));
    assert!(near(f * Point::new(110.0, 60.0), Point::new(200.0, 100.0)));
    // Горизонтальный отрезок по высоте не растягивается (делить не на что).
    let flat = vector(net(&[(0.0, 5.0), (30.0, 5.0)], &[(0, 1)]));
    assert!(near(
        fit(&flat) * Point::new(30.0, 5.0),
        Point::new(30.0, 0.0)
    ));
    // Дуга выше своих точек: границы — по кривой, а не по точкам.
    let mut arc = net(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1)]);
    arc.segments[0].tangent_start = [0.0, -40.0];
    arc.segments[0].tangent_end = [0.0, -40.0];
    let b = network_bounds(&arc).unwrap();
    assert!(
        (b.y0 + 30.0).abs() < 1e-9,
        "вершина кубической дуги — 3/4 ручки: {b:?}"
    );
}

#[test]
fn stroke_and_region_hits() {
    let mut p = vector(net(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1)]));
    p.strokes[0].width = 2.0;
    let s = network_shape(&p);
    assert!(hit_path(&p, &s, Point::new(50.0, 3.0), 4.0), "допуск 4 px");
    assert!(!hit_path(&p, &s, Point::new(50.0, 6.0), 4.0));
    // Стрелка на конце ловит щелчок за точкой конца.
    p.stroke_style.cap_end = StrokeCap::ArrowTriangle;
    let s = network_shape(&p);
    assert!(hit_path(&p, &s, Point::new(106.0, 0.0), 1.0));
    // Квадрат с дыркой по even-odd: дырка не ловит, кольцо ловит.
    let mut sq = net(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
            (40.0, 40.0),
            (60.0, 40.0),
            (60.0, 60.0),
            (40.0, 60.0),
        ],
        &[
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
        ],
    );
    sq.regions = vec![VectorRegion {
        rule: FillRule::EvenOdd,
        loops: vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]],
        fills: None,
    }];
    let mut q = vector(sq);
    q.strokes.clear();
    let s = network_shape(&q);
    assert!(
        !hit_path(&q, &s, Point::new(20.0, 20.0), 1.0),
        "без заливок узла область пуста"
    );
    q.fills = vec![Fill::solid(Color::BLACK)];
    assert!(hit_path(&q, &s, Point::new(20.0, 20.0), 1.0));
    assert!(
        !hit_path(&q, &s, Point::new(50.0, 50.0), 1.0),
        "дырка even-odd"
    );
    // Своя заливка области — даже без заливок узла.
    let mut own = q.clone();
    own.fills.clear();
    Arc::make_mut(own.vector.as_mut().unwrap()).regions[0].fills =
        Some(vec![Fill::solid(Color::WHITE)]);
    let s = network_shape(&own);
    assert!(hit_path(&own, &s, Point::new(20.0, 20.0), 1.0));
}

#[test]
fn outline_covers_caps_and_inside_stroke_doubles_width() {
    let mut p = vector(net(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1)]));
    p.strokes[0].width = 4.0;
    p.stroke_style.cap_end = StrokeCap::Circle;
    let s = network_shape(&p);
    let out = stroke_outline(&s, &p.strokes[0], &p.stroke_style);
    assert!(out.contains(Point::new(50.0, 1.5)));
    assert!(!out.contains(Point::new(50.0, 2.5)));
    // Круг на конце радиуса 1,5w + 1,5 = 7,5.
    assert!(out.contains(Point::new(100.0, 7.0)));
    assert!(
        !out.contains(Point::new(-1.0, 0.0)),
        "в начале окончания нет — плоский конец"
    );
    // Замкнутый контур внутри: двойная толщина, обрезку делает рендер.
    let mut tri = vector(net(
        &[(0.0, 0.0), (40.0, 0.0), (20.0, 30.0)],
        &[(0, 1), (1, 2), (2, 0)],
    ));
    tri.strokes[0].align = StrokeAlign::Inside;
    let s = network_shape(&tri);
    assert_eq!(effective_width(&s, &tri.strokes[0]), 2.0);
    assert_eq!(stroke_clip(&s, &tri.strokes[0]), StrokeClip::Inside);
    // Открытый путь «внутри» не бывает: по центру, без обрезки.
    let open = network_shape(&p);
    assert_eq!(stroke_clip(&open, &p.strokes[0]), StrokeClip::None);
}

#[test]
fn line_caps_and_rect_joins_switch_to_path_stroke() {
    let mut line = Props {
        w: 100.0,
        ..Props::defaults(NodeKind::Line)
    };
    assert!(!uses_path_stroke(&line));
    assert_eq!(shape::stroke_outset(&line), 0.5);
    line.stroke_style.cap_end = StrokeCap::ArrowLines;
    assert!(uses_path_stroke(&line));
    assert_eq!(shape::stroke_outset(&line), arrow_length(1.0) + 1.0);
    assert!(
        shape::hit(&line, Point::new(104.0, 0.0), 0.5),
        "стрелка за концом"
    );
    let mut rect = Props::defaults(NodeKind::Rect);
    rect.strokes = vec![Stroke {
        color: Color::BLACK,
        width: 10.0,
        align: StrokeAlign::Center,
    }];
    rect.stroke_style.join = StrokeJoin::Round;
    assert!(uses_path_stroke(&rect));
    assert_eq!(
        shape::stroke_outset(&rect),
        5.0,
        "круглый стык не выступает за полтолщины"
    );
    rect.corner_radius = 4.0;
    assert!(!uses_path_stroke(&rect), "у скруглённого углов нет");
}

#[test]
fn scene_bounds_and_hit_test_follow_the_network() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut p = vector(net(&[(0.0, 0.0), (100.0, 50.0)], &[(0, 1)]));
    p.x = 10.0;
    p.y = 20.0;
    p.stroke_style.cap_end = StrokeCap::Diamond;
    let id = doc
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_with_props(None, None, &p)?])
        })
        .unwrap()[0];
    let scene = Scene::from_page(doc.page(page).unwrap());
    let n = scene.node(id).unwrap();
    assert_eq!(n.local_bounds, Some(Rect::new(0.0, 0.0, 100.0, 50.0)));
    let wb = n.world_bounds.unwrap();
    let r = dot_radius(1.0);
    assert!(
        wb.x1 >= 110.0 + r - 1e-9 && wb.y1 >= 70.0 + r - 1e-9,
        "{wb:?}"
    );
    // Середина отрезка — попадание, угол рамки вдали от отрезка — нет.
    assert_eq!(scene.hit_test(Point::new(60.0, 45.0), 1.0), Some(id));
    assert_eq!(scene.hit_test(Point::new(100.0, 25.0), 1.0), None);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-scene --test vector`
Expected: FAIL — не собирается: нет модуля `ferrum_scene::vector` (`network_shape`, `fit`, `hit_path`, `stroke_outline`…)

- [ ] **Step 3: Реализация**

В `crates/ferrum-scene/src/lib.rs` заменить:

````rust
mod scene;
pub mod shape;

pub use scene::{testing, Draft, Scene, SceneNode, Slot};
````

на:

````rust
mod scene;
pub mod shape;
pub mod vector;

pub use scene::{testing, Draft, Scene, SceneNode, Slot};
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
            _ => geom.bounds(),
        };
        let outset = geom.stroke_outset(&n.props.strokes);
        let world_bounds =
            local_bounds.map(|b| n.world.transform_rect_bbox(b.inflate(outset, outset)));
````

на:

````rust
            _ => geom.bounds(),
        };
        let outset = shape::stroke_outset(&n.props);
        let world_bounds =
            local_bounds.map(|b| n.world.transform_rect_bbox(b.inflate(outset, outset)));
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
        || a.fills != b.fills
        || a.strokes != b.strokes
        || a.text != b.text
}

````

на:

````rust
        || a.fills != b.fills
        || a.strokes != b.strokes
        || a.stroke_style != b.stroke_style
        || a.text != b.text
        || match (&a.vector, &b.vector) {
            // Та же сеть (правка других свойств) — без сравнения точек.
            (Some(x), Some(y)) => !Arc::ptr_eq(x, y) && x != y,
            (x, y) => x.is_some() != y.is_some(),
        }
}

````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
//! кольца обводок и точная проверка попадания. Рендер и hit-testing берут форму
//! отсюда, поэтому то, что нарисовано, и то, во что попадает клик, совпадает.

use ferrum_doc::{NodeKind, Props, Stroke, StrokeAlign};
use kurbo::{Affine, BezPath, Ellipse, Line, ParamCurveNearest, Point, Rect, RoundedRect, Shape};

/// Точность аппроксимации дуг кривыми Безье в локальных единицах. Контур строится
````

на:

````rust
//! кольца обводок и точная проверка попадания. Рендер и hit-testing берут форму
//! отсюда, поэтому то, что нарисовано, и то, во что попадает клик, совпадает.
//! Сеть вектора и обводка по контуру (окончания, соединения) — `crate::vector`.

use ferrum_doc::{NodeKind, Props, Stroke, StrokeAlign, MITER_LIMIT_RANGE};
use kurbo::{Affine, BezPath, Ellipse, Line, ParamCurveNearest, Point, Rect, RoundedRect, Shape};

use crate::vector;

/// Точность аппроксимации дуг кривыми Безье в локальных единицах. Контур строится
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
        s.width = finite(s.width, 0.0).clamp(0.0, COORD_LIMIT);
    }
}

````

на:

````rust
        s.width = finite(s.width, 0.0).clamp(0.0, COORD_LIMIT);
    }
    let (lo, hi) = (*MITER_LIMIT_RANGE.start(), *MITER_LIMIT_RANGE.end());
    p.stroke_style.miter_limit = finite(p.stroke_style.miter_limit, 4.0).clamp(lo, hi);
}

````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
    /// Линия от `(0, 0)` до `(w, 0)`.
    Line(f64),
}

````

на:

````rust
    /// Линия от `(0, 0)` до `(w, 0)`.
    Line(f64),
    /// Векторная сеть, растянутая в рамку `(0, 0, w, h)`; контуры —
    /// `crate::vector::network_shape` по свойствам узла.
    Vector(Rect),
}

````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
        // Рамка текста; попадание — по ней (спека подсистемы 3, раздел 5).
        NodeKind::Text => Geometry::Rect { rect, radius: 0.0 },
        // Пока сеть не рисуется (её геометрия — следующая задача), вектор — рамка.
        NodeKind::Vector => Geometry::Rect { rect, radius: 0.0 },
    }
}
````

на:

````rust
        // Рамка текста; попадание — по ней (спека подсистемы 3, раздел 5).
        NodeKind::Text => Geometry::Rect { rect, radius: 0.0 },
        NodeKind::Vector => Geometry::Vector(rect),
    }
}
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
        match *self {
            Geometry::None => None,
            Geometry::Rect { rect, .. } | Geometry::Ellipse(rect) => Some(rect),
            Geometry::Line(len) => Some(Rect::new(0.0, 0.0, len, 0.0)),
        }
````

на:

````rust
        match *self {
            Geometry::None => None,
            Geometry::Rect { rect, .. } | Geometry::Ellipse(rect) | Geometry::Vector(rect) => {
                Some(rect)
            }
            Geometry::Line(len) => Some(Rect::new(0.0, 0.0, len, 0.0)),
        }
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
    }

    /// Контур заливки; у линии и группы заливки нет.
    pub fn fill_path(&self) -> Option<BezPath> {
        let mut path = BezPath::new();
````

на:

````rust
    }

    /// Контур заливки; у линии и группы заливки нет, у вектора она — по областям
    /// (`crate::vector`).
    pub fn fill_path(&self) -> Option<BezPath> {
        let mut path = BezPath::new();
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
                true
            }
            Geometry::None | Geometry::Line(_) => false,
        }
    }
````

на:

````rust
                true
            }
            Geometry::None | Geometry::Line(_) | Geometry::Vector(_) => false,
        }
    }
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
                    .then(|| Offset::Ellipse(Ellipse::from_rect(r)))
            }
            Geometry::None | Geometry::Line(_) => None,
        }
    }
````

на:

````rust
                    .then(|| Offset::Ellipse(Ellipse::from_rect(r)))
            }
            Geometry::None | Geometry::Line(_) | Geometry::Vector(_) => None,
        }
    }
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
    }

    /// На сколько обводки выступают за форму наружу.
    pub fn stroke_outset(&self, strokes: &[Stroke]) -> f64 {
        strokes
````

на:

````rust
    }

    /// На сколько обводки-кольца выступают за форму наружу (обводка по контуру —
    /// `stroke_outset` по свойствам узла).
    pub fn stroke_outset(&self, strokes: &[Stroke]) -> f64 {
        strokes
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
            })
            .fold(0.0, f64::max)
    }
}
````

на:

````rust
            })
            .fold(0.0, f64::max)
    }
}

/// На сколько обводки узла выступают за его форму: кольца или обводка по контуру
/// с соединениями и окончаниями.
pub fn stroke_outset(p: &Props) -> f64 {
    match vector::path_shape(p) {
        Some(shape) => vector::path_stroke_outset(p, &shape),
        None => geometry(p).stroke_outset(&p.strokes),
    }
}
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
pub fn hit(p: &Props, local: Point, tolerance: f64) -> bool {
    let geom = geometry(p);
    if let Geometry::Line(len) = geom {
        let half = p.strokes.iter().map(|s| s.width / 2.0).fold(0.0, f64::max);
````

на:

````rust
pub fn hit(p: &Props, local: Point, tolerance: f64) -> bool {
    let geom = geometry(p);
    if let Some(shape) = vector::path_shape(p) {
        // Вектор заливается по областям (`hit_path`), фигура — по своей форме.
        let filled = matches!(p.kind, NodeKind::Frame | NodeKind::Image)
            || p.fills.iter().any(|f| f.visible);
        if p.kind != NodeKind::Vector && p.kind != NodeKind::Line && filled && geom.contains(local)
        {
            return true;
        }
        return vector::hit_path(p, &shape, local, tolerance);
    }
    if let Geometry::Line(len) = geom {
        let half = p.strokes.iter().map(|s| s.width / 2.0).fold(0.0, f64::max);
````

Создать `crates/ferrum-scene/src/vector.rs`:

````rust
//! Геометрия векторной сети и обводки по контуру (спека подсистемы 3, раздел 9):
//! контуры для заливки областей и для обводки, открытые концы с их окончаниями,
//! границы, попадание. Рендер и hit-testing берут форму отсюда, поэтому то, что
//! нарисовано, и то, во что попадает клик, совпадает.
//!
//! Сеть хранится в своих координатах; рамка узла `w × h` — её границы, поэтому
//! сеть растягивается под рамку (`fit`): ресайз вектора, как в Figma, меняет его
//! геометрию без переписывания точек.
//!
//! Обводка по контуру — у вектора всегда, у линии с окончаниями и у фигур с
//! углами при соединении не «острым» (`uses_path_stroke`): контур обводится
//! `kurbo::stroke` с плоскими концами, а окончания открытых концов дорисовываются
//! фигурами (`cap_shapes`). Стыки в точках, где сходится больше двух сегментов,
//! закрываются кругом (как в Figma, ветви сети не обрываются зарубкой).

use ferrum_doc::{
    Fill, FillRule, NodeKind, Props, Stroke, StrokeAlign, StrokeCap, StrokeJoin, StrokeStyle,
    VectorNetwork, VectorSegment,
};
use kurbo::{
    Affine, BezPath, Cap, Circle, CubicBez, Join, Line, ParamCurve, ParamCurveDeriv,
    ParamCurveNearest, PathSeg, Point, Rect, Shape, StrokeOpts, Vec2,
};

use crate::shape::PATH_TOLERANCE;

/// Кривая сегмента в координатах сети.
pub fn segment_curve(net: &VectorNetwork, s: &VectorSegment) -> PathSeg {
    let p = |i: u32| {
        let v = &net.points[i as usize];
        Point::new(v.x, v.y)
    };
    let (a, b) = (p(s.start), p(s.end));
    if s.is_straight() {
        PathSeg::Line(Line::new(a, b))
    } else {
        PathSeg::Cubic(CubicBez::new(
            a,
            a + Vec2::new(s.tangent_start[0], s.tangent_start[1]),
            b + Vec2::new(s.tangent_end[0], s.tangent_end[1]),
            b,
        ))
    }
}

/// Точные границы сети в её координатах: кривые сегментов (с экстремумами) и
/// отдельные точки. `None` — точек нет.
pub fn network_bounds(net: &VectorNetwork) -> Option<Rect> {
    let mut r: Option<Rect> = None;
    let mut add = |b: Rect| r = Some(r.map_or(b, |a| a.union(b)));
    for p in &net.points {
        add(Rect::from_points((p.x, p.y), (p.x, p.y)));
    }
    for s in &net.segments {
        if !s.is_straight() {
            add(segment_curve(net, s).bounding_box());
        }
    }
    r
}

/// Сеть → координаты формы узла: границы сети растягиваются в `(0, 0, w, h)`.
/// Вырожденная по оси сеть (отрезок по горизонтали) по этой оси не растягивается.
pub fn fit(p: &Props) -> Affine {
    let Some(net) = &p.vector else {
        return Affine::IDENTITY;
    };
    let Some(b) = network_bounds(net) else {
        return Affine::IDENTITY;
    };
    let sx = if b.width() > 0.0 {
        p.w / b.width()
    } else {
        1.0
    };
    let sy = if b.height() > 0.0 {
        p.h / b.height()
    } else {
        1.0
    };
    Affine::scale_non_uniform(sx, sy) * Affine::translate((-b.x0, -b.y0))
}

/// Открытый конец пути: точка, направление наружу и какое окончание у него.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct End {
    pub point: Point,
    /// Единичный вектор от пути наружу.
    pub dir: Vec2,
    pub cap: StrokeCap,
}

/// Область для заливки: контур петель, правило и свои заливки (`None` — узла).
#[derive(Debug, Clone, PartialEq)]
pub struct RegionPath {
    pub path: BezPath,
    pub rule: FillRule,
    pub fills: Option<Vec<Fill>>,
}

/// Контуры формы для обводки по контуру — в координатах формы узла.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathShape {
    /// Цепочки сегментов: через точки с двумя сегментами путь идёт дальше (там —
    /// соединение), цикл таких точек — замкнутый подпуть.
    pub stroke: BezPath,
    pub regions: Vec<RegionPath>,
    /// Концы в точках с одним сегментом.
    pub ends: Vec<End>,
    /// Точки, где сходится больше двух сегментов.
    pub junctions: Vec<Point>,
    /// Путь замкнут (нет концов): обводка внутри и снаружи — по нему.
    pub closed: bool,
}

/// Обводится ли узел по контуру (иначе — кольцами `shape::Geometry`).
pub fn uses_path_stroke(p: &Props) -> bool {
    let s = &p.stroke_style;
    match p.kind {
        NodeKind::Vector => true,
        NodeKind::Line => s.cap_start != StrokeCap::None || s.cap_end != StrokeCap::None,
        // Углы есть только у прямоугольника без скругления.
        NodeKind::Rect | NodeKind::Frame | NodeKind::Image => {
            s.join != StrokeJoin::Miter && p.corner_radius <= 0.0
        }
        _ => false,
    }
}

/// Направление кривой в начале (`at_end = false`) или в конце, единичное; у
/// вырожденной кривой — по хорде.
fn tangent(seg: PathSeg, at_end: bool) -> Vec2 {
    let v = match seg {
        PathSeg::Cubic(c) => {
            let d = if at_end { c.p3 - c.p2 } else { c.p1 - c.p0 };
            if d.hypot2() > 1e-18 {
                d
            } else {
                let q = c.deriv().eval(if at_end { 1.0 } else { 0.0 });
                if q.to_vec2().hypot2() > 1e-18 {
                    q.to_vec2()
                } else {
                    c.p3 - c.p0
                }
            }
        }
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.p2 - q.p0,
    };
    let n = v.hypot();
    if n > 0.0 {
        v / n
    } else {
        Vec2::new(1.0, 0.0)
    }
}

fn reversed(seg: PathSeg) -> PathSeg {
    match seg {
        PathSeg::Cubic(c) => PathSeg::Cubic(CubicBez::new(c.p3, c.p2, c.p1, c.p0)),
        PathSeg::Line(l) => PathSeg::Line(Line::new(l.p1, l.p0)),
        PathSeg::Quad(q) => PathSeg::Quad(kurbo::QuadBez::new(q.p2, q.p1, q.p0)),
    }
}

fn push_seg(path: &mut BezPath, seg: PathSeg) {
    match seg {
        PathSeg::Line(l) => path.line_to(l.p1),
        PathSeg::Cubic(c) => path.curve_to(c.p1, c.p2, c.p3),
        PathSeg::Quad(q) => path.quad_to(q.p1, q.p2),
    }
}

/// Контуры сети в координатах формы узла (`fit` применён).
pub fn network_shape(p: &Props) -> PathShape {
    let Some(net) = &p.vector else {
        return PathShape::default();
    };
    let fit = fit(p);
    let curve = |i: usize| fit_seg(fit, segment_curve(net, &net.segments[i]));
    let n = net.points.len();
    // Сегменты каждой точки.
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, s) in net.segments.iter().enumerate() {
        incident[s.start as usize].push(i);
        incident[s.end as usize].push(i);
    }
    let mut used = vec![false; net.segments.len()];
    let mut stroke = BezPath::new();
    // Цепочка от точки `from` по сегменту `first` до точки, где путь ветвится,
    // кончается или замыкается.
    let walk = |from: usize, first: usize, used: &mut Vec<bool>, stroke: &mut BezPath| {
        let mut at = from;
        let mut seg = first;
        let start = fit * point(net, from);
        stroke.move_to(start);
        loop {
            used[seg] = true;
            let s = &net.segments[seg];
            let forward = s.start as usize == at;
            let c = if forward {
                curve(seg)
            } else {
                reversed(curve(seg))
            };
            push_seg(stroke, c);
            at = if forward { s.end } else { s.start } as usize;
            if at == from && incident[at].len() == 2 {
                stroke.close_path();
                return;
            }
            if incident[at].len() != 2 {
                return;
            }
            match incident[at].iter().find(|&&i| !used[i]) {
                Some(&next) => seg = next,
                None => return,
            }
        }
    };
    for (v, segs) in incident.iter().enumerate() {
        if segs.len() == 2 {
            continue;
        }
        for &s in segs {
            if !used[s] {
                walk(v, s, &mut used, &mut stroke);
            }
        }
    }
    // Остались только циклы из точек с двумя сегментами.
    for s in 0..net.segments.len() {
        if !used[s] {
            walk(net.segments[s].start as usize, s, &mut used, &mut stroke);
        }
    }
    let mut ends = Vec::new();
    let mut junctions = Vec::new();
    for (v, segs) in incident.iter().enumerate() {
        match segs[..] {
            [s] => {
                let seg = &net.segments[s];
                let is_start = seg.start as usize == v;
                let c = curve(s);
                // Наружу: против хода кривой в её начале, по ходу — в конце.
                let dir = if is_start {
                    -tangent(c, false)
                } else {
                    tangent(c, true)
                };
                ends.push(End {
                    point: fit * point(net, v),
                    dir,
                    cap: if is_start {
                        p.stroke_style.cap_start
                    } else {
                        p.stroke_style.cap_end
                    },
                });
            }
            [_, _, _, ..] => junctions.push(fit * point(net, v)),
            _ => {}
        }
    }
    let regions = net
        .regions
        .iter()
        .map(|r| {
            let mut path = BezPath::new();
            for l in &r.loops {
                let Some(pts) = net.loop_points(l) else {
                    continue;
                };
                path.move_to(fit * point(net, pts[0] as usize));
                for (k, &si) in l.iter().enumerate() {
                    let s = &net.segments[si as usize];
                    let c = curve(si as usize);
                    let forward = s.start == pts[k];
                    push_seg(&mut path, if forward { c } else { reversed(c) });
                }
                path.close_path();
            }
            RegionPath {
                path,
                rule: r.rule,
                fills: r.fills.clone(),
            }
        })
        .collect();
    let closed = ends.is_empty() && !net.segments.is_empty();
    PathShape {
        stroke,
        regions,
        ends,
        junctions,
        closed,
    }
}

fn point(net: &VectorNetwork, i: usize) -> Point {
    Point::new(net.points[i].x, net.points[i].y)
}

fn fit_seg(a: Affine, s: PathSeg) -> PathSeg {
    match s {
        PathSeg::Line(l) => PathSeg::Line(Line::new(a * l.p0, a * l.p1)),
        PathSeg::Cubic(c) => PathSeg::Cubic(CubicBez::new(a * c.p0, a * c.p1, a * c.p2, a * c.p3)),
        PathSeg::Quad(q) => PathSeg::Quad(kurbo::QuadBez::new(a * q.p0, a * q.p1, a * q.p2)),
    }
}

/// Контуры узла для обводки по контуру: сеть вектора, отрезок линии, рамка
/// прямоугольника. `None` — узел обводится кольцами.
pub fn path_shape(p: &Props) -> Option<PathShape> {
    if !uses_path_stroke(p) {
        return None;
    }
    let (w, h) = (p.w.max(0.0), p.h.max(0.0));
    Some(match p.kind {
        NodeKind::Vector => network_shape(p),
        NodeKind::Line => {
            let mut stroke = BezPath::new();
            stroke.move_to((0.0, 0.0));
            stroke.line_to((w, 0.0));
            PathShape {
                stroke,
                ends: vec![
                    End {
                        point: Point::ZERO,
                        dir: Vec2::new(-1.0, 0.0),
                        cap: p.stroke_style.cap_start,
                    },
                    End {
                        point: Point::new(w, 0.0),
                        dir: Vec2::new(1.0, 0.0),
                        cap: p.stroke_style.cap_end,
                    },
                ],
                ..Default::default()
            }
        }
        _ => PathShape {
            stroke: Rect::new(0.0, 0.0, w, h).to_path(PATH_TOLERANCE),
            closed: true,
            ..Default::default()
        },
    })
}

/// Длина плеча стрелки и треугольника вдоль пути для толщины `w`.
pub fn arrow_length(w: f64) -> f64 {
    3.0 * w + 6.0
}

/// Радиус круга и ромба на конце для толщины `w`.
pub fn dot_radius(w: f64) -> f64 {
    1.5 * w + 1.5
}

/// Насколько окончание выступает за точку конца (для границ узла).
pub fn cap_extent(cap: StrokeCap, w: f64) -> f64 {
    match cap {
        StrokeCap::None => w / 2.0,
        StrokeCap::Round | StrokeCap::Square => w / 2.0 * std::f64::consts::SQRT_2,
        StrokeCap::ArrowLines | StrokeCap::ArrowTriangle => arrow_length(w) + w,
        StrokeCap::Circle | StrokeCap::Diamond => dot_radius(w),
    }
}

/// Фигуры окончания конца: заливаемые контуры и обводимые линии (стрелка из
/// двух линий — ломаная через конец, обводится той же толщиной с острым стыком).
pub fn cap_shapes(end: &End, w: f64) -> (BezPath, Option<BezPath>) {
    let (p, d) = (end.point, end.dir);
    let nrm = Vec2::new(-d.y, d.x);
    let mut fill = BezPath::new();
    let mut lines = None;
    match end.cap {
        StrokeCap::None => {}
        StrokeCap::Round => fill.extend(Circle::new(p, w / 2.0).path_elements(PATH_TOLERANCE)),
        StrokeCap::Square => {
            let h = w / 2.0;
            fill.move_to(p + nrm * h);
            fill.line_to(p + nrm * h + d * h);
            fill.line_to(p - nrm * h + d * h);
            fill.line_to(p - nrm * h);
            fill.close_path();
        }
        StrokeCap::ArrowLines => {
            let len = arrow_length(w);
            let back = -d * len * (30f64.to_radians().cos());
            let side = nrm * len * (30f64.to_radians().sin());
            let mut l = BezPath::new();
            l.move_to(p + back + side);
            l.line_to(p);
            l.line_to(p + back - side);
            lines = Some(l);
        }
        StrokeCap::ArrowTriangle => {
            // Остриё выступает на толщину: плоский конец пути целиком под треугольником.
            let len = arrow_length(w);
            let tip = p + d * w;
            let base = tip - d * len;
            let half = len * 30f64.to_radians().tan();
            fill.move_to(tip);
            fill.line_to(base + nrm * half);
            fill.line_to(base - nrm * half);
            fill.close_path();
        }
        StrokeCap::Circle => {
            fill.extend(Circle::new(p, dot_radius(w)).path_elements(PATH_TOLERANCE));
        }
        StrokeCap::Diamond => {
            let r = dot_radius(w);
            fill.move_to(p + d * r);
            fill.line_to(p + nrm * r);
            fill.line_to(p - d * r);
            fill.line_to(p - nrm * r);
            fill.close_path();
        }
    }
    (fill, lines)
}

/// Толщина, которой обводится контур: внутри и снаружи замкнутого — двойная с
/// обрезкой (`StrokeClip`), иначе — как есть.
pub fn effective_width(shape: &PathShape, s: &Stroke) -> f64 {
    if shape.closed && s.align != StrokeAlign::Center {
        2.0 * s.width
    } else {
        s.width
    }
}

/// Обрезка обводки замкнутого контура.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeClip {
    None,
    /// Только внутри контура.
    Inside,
    /// Только снаружи контура.
    Outside,
}

pub fn stroke_clip(shape: &PathShape, s: &Stroke) -> StrokeClip {
    match (shape.closed, s.align) {
        (true, StrokeAlign::Inside) => StrokeClip::Inside,
        (true, StrokeAlign::Outside) => StrokeClip::Outside,
        _ => StrokeClip::None,
    }
}

fn kurbo_join(j: StrokeJoin) -> Join {
    match j {
        StrokeJoin::Miter => Join::Miter,
        StrokeJoin::Round => Join::Round,
        StrokeJoin::Bevel => Join::Bevel,
    }
}

/// Контур обводки как заливка (правило ненулевое): сам путь с соединением узла и
/// плоскими концами, круги в точках ветвления и фигуры окончаний открытых концов.
/// Кэшируется рендером по версии узла: на кадр — только заливка.
pub fn stroke_outline(shape: &PathShape, s: &Stroke, style: &StrokeStyle) -> BezPath {
    let w = effective_width(shape, s);
    let mut out = BezPath::new();
    if w <= 0.0 {
        return out;
    }
    let k = kurbo::Stroke::new(w)
        .with_join(kurbo_join(style.join))
        .with_miter_limit(style.miter_limit)
        .with_caps(Cap::Butt);
    out.extend(
        kurbo::stroke(
            shape.stroke.iter(),
            &k,
            &StrokeOpts::default(),
            PATH_TOLERANCE,
        )
        .iter(),
    );
    for j in &shape.junctions {
        out.extend(Circle::new(*j, w / 2.0).path_elements(PATH_TOLERANCE));
    }
    let arms = kurbo::Stroke::new(w)
        .with_join(Join::Miter)
        .with_caps(Cap::Butt);
    for e in &shape.ends {
        let (fill, lines) = cap_shapes(e, w);
        // Фигуры — по часовой, как и контур: при ненулевом правиле не вырезают друг друга.
        if fill.area() < 0.0 {
            out.extend(fill.reverse_subpaths().iter());
        } else {
            out.extend(fill.iter());
        }
        if let Some(l) = lines {
            out.extend(
                kurbo::stroke(l.iter(), &arms, &StrokeOpts::default(), PATH_TOLERANCE).iter(),
            );
        }
    }
    out
}

/// На сколько обводки узла по контуру выступают за рамку формы.
pub fn path_stroke_outset(p: &Props, shape: &PathShape) -> f64 {
    let style = &p.stroke_style;
    p.strokes
        .iter()
        .filter(|s| s.width > 0.0)
        .map(|s| {
            let w = effective_width(shape, s);
            let join = if style.join == StrokeJoin::Miter {
                w / 2.0 * style.miter_limit.max(1.0)
            } else {
                w / 2.0
            };
            let caps = shape
                .ends
                .iter()
                .map(|e| cap_extent(e.cap, w))
                .fold(0.0, f64::max);
            join.max(caps)
        })
        .fold(0.0, f64::max)
}

/// Расстояние от точки до кривой — с отсевом сегментов по рамке: на сети в
/// 100 000 точек точный поиск идёт только у сегментов рядом с точкой.
pub fn distance_to(path: &BezPath, pt: Point, within: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for seg in path.segments() {
        let b = seg.bounding_box().inflate(within, within);
        if !b.contains(pt) {
            continue;
        }
        let d = seg.nearest(pt, 1e-6).distance_sq.sqrt();
        best = Some(best.map_or(d, |x: f64| x.min(d)));
    }
    best
}

/// Лежит ли точка в области по её правилу.
pub fn region_contains(r: &RegionPath, pt: Point) -> bool {
    let w = r.path.winding(pt);
    match r.rule {
        FillRule::NonZero => w != 0,
        FillRule::EvenOdd => w % 2 != 0,
    }
}

/// Попадание в узел с обводкой по контуру (`local` — в координатах формы):
/// залитая область, полоса обводки не уже допуска, фигуры окончаний.
pub fn hit_path(p: &Props, shape: &PathShape, local: Point, tolerance: f64) -> bool {
    let node_filled = p.fills.iter().any(|f| f.visible && f.opacity > 0.0);
    if p.kind == NodeKind::Vector {
        for r in &shape.regions {
            let filled = match &r.fills {
                Some(f) => f.iter().any(|f| f.visible && f.opacity > 0.0),
                None => node_filled,
            };
            if filled && region_contains(r, local) {
                return true;
            }
        }
    }
    for s in p.strokes.iter().filter(|s| s.width > 0.0) {
        let w = effective_width(shape, s);
        let half = (w / 2.0).max(tolerance);
        if distance_to(&shape.stroke, local, half).is_some_and(|d| d <= half) {
            return true;
        }
        for e in &shape.ends {
            if e.cap != StrokeCap::None
                && (local - e.point).hypot() <= cap_extent(e.cap, w).max(tolerance)
            {
                return true;
            }
        }
    }
    false
}

/// Сегмент сети под точкой (в координатах сети) и параметр на нём — для
/// добавления точки щелчком и изгиба: ближайший в пределах `within`.
pub fn nearest_segment(net: &VectorNetwork, pt: Point, within: f64) -> Option<(usize, f64, f64)> {
    let mut best: Option<(usize, f64, f64)> = None;
    for (i, s) in net.segments.iter().enumerate() {
        let c = segment_curve(net, s);
        if !c.bounding_box().inflate(within, within).contains(pt) {
            continue;
        }
        let n = c.nearest(pt, 1e-9);
        let d = n.distance_sq.sqrt();
        if d <= within && best.is_none_or(|b| d < b.2) {
            best = Some((i, n.t, d));
        }
    }
    best
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-scene -p ferrum-render 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `vector` сцены — 7 тестов, тесты рендера — без изменений

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-scene/src/lib.rs crates/ferrum-scene/src/scene.rs crates/ferrum-scene/src/shape.rs crates/ferrum-scene/src/vector.rs crates/ferrum-scene/tests/vector.rs
git commit -m "feat(scene): геометрия векторной сети — цепочки, петли, концы с окончаниями, растяжение под рамку; обводка по контуру, границы и попадание

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 3: Рендер векторов, окончаний и соединений

Спека 9 и 12: области рисуются по правилу заливки своими заливками (или заливками узла; изображение — по рамке узла с обрезкой областью), обводка по контуру — заливкой контура (строится один раз на версию узла и живёт в кэше списка), внутри и снаружи — с обрезкой по пути или его внешности. Эталоны и пиксели — на каждое окончание, соединение, положение обводки, правило заливки, свою заливку области, круглое соединение прямоугольника, стрелку линии и полупрозрачную обводку с окончаниями. Подсветка наведения вектора — его сетью.

**Files:**
- Modify: `crates/ferrum-render/src/{display,draw,overlay}.rs`
- Test: `crates/ferrum-render/tests/vector.rs` (новый), эталоны `crates/ferrum-render/tests/golden/{vector_regions,stroke_caps,stroke_joins}.png` (новые)

**Interfaces:**
- Consumes: `ferrum_scene::vector` задачи 2.
- Produces: `Item::{Region { region, paint }, PathStroke { stroke, color, clip }}`, `Paths::{inside, outside}`, `DisplayList::path: Option<Arc<PathSource>>`, `PathSource { shape, style }`, `display::exterior(path, margin)`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-render/tests/vector.rs`:

````rust
//! Рендер векторов и обводки по контуру (подсистема 3, этап 3): области с
//! правилами заливки и своими заливками, каждое окончание, каждое соединение и
//! предел острого угла, обводка внутри, по центру и снаружи замкнутого пути,
//! стрелка у линии, полупрозрачная обводка с окончаниями. Каждый вариант —
//! пиксели, затем эталон. Пересоздать эталоны:
//! `FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test vector`.

mod common;

use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_scene::vector::network_bounds;

/// Вектор по точкам в мировых координатах: рамка узла — границы сети.
fn add_vector(
    p: &mut Page,
    points: &[(f64, f64)],
    segs: &[(u32, u32)],
    regions: Vec<VectorRegion>,
    patch: PropsPatch,
) -> NodeId {
    let net = VectorNetwork {
        points: points
            .iter()
            .map(|&(x, y)| VectorPoint::new(x, y))
            .collect(),
        segments: segs
            .iter()
            .map(|&(a, b)| VectorSegment::line(a, b))
            .collect(),
        regions,
    };
    let b = network_bounds(&net).unwrap();
    p.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            x: Some(b.x0),
            y: Some(b.y0),
            w: Some(b.width()),
            h: Some(b.height()),
            vector: Some(Arc::new(net)),
            ..patch
        },
    )
}

fn black(width: f64, align: StrokeAlign) -> Option<Vec<Stroke>> {
    Some(vec![Stroke {
        color: Color::BLACK,
        width,
        align,
    }])
}

fn solid(r: u8, g: u8, b: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, 255))])
}

/// Точки и сегменты сети.
type Net = (Vec<(f64, f64)>, Vec<(u32, u32)>);

/// Квадрат 80 × 80 от `(x, y)` и квадрат-дырка внутри; `same_dir` — дырка
/// обходится в ту же сторону (иначе — в обратную).
fn square_with_hole(x: f64, y: f64, same_dir: bool) -> Net {
    let pts = vec![
        (x, y),
        (x + 80.0, y),
        (x + 80.0, y + 80.0),
        (x, y + 80.0),
        (x + 30.0, y + 30.0),
        (x + 50.0, y + 30.0),
        (x + 50.0, y + 50.0),
        (x + 30.0, y + 50.0),
    ];
    let hole = if same_dir {
        [(4, 5), (5, 6), (6, 7), (7, 4)]
    } else {
        [(4, 7), (7, 6), (6, 5), (5, 4)]
    };
    let mut segs = vec![(0, 1), (1, 2), (2, 3), (3, 0)];
    segs.extend(hole);
    (pts, segs)
}

#[test]
fn vector_regions() {
    let mut p = Page::new();
    // Even-odd: дырка пустая.
    let (pts, segs) = square_with_hole(10.0, 10.0, true);
    add_vector(
        &mut p,
        &pts,
        &segs,
        vec![VectorRegion {
            rule: FillRule::EvenOdd,
            loops: vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]],
            fills: None,
        }],
        PropsPatch {
            fills: solid(255, 0, 0),
            strokes: Some(vec![]),
            ..Default::default()
        },
    );
    // Non-zero, петли в одну сторону: дырка залита.
    let (pts, segs) = square_with_hole(100.0, 10.0, true);
    add_vector(
        &mut p,
        &pts,
        &segs,
        vec![VectorRegion {
            rule: FillRule::NonZero,
            loops: vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]],
            fills: None,
        }],
        PropsPatch {
            fills: solid(255, 0, 0),
            strokes: Some(vec![]),
            ..Default::default()
        },
    );
    // Своя заливка области (синяя) у узла без заливок, обводка по центру.
    add_vector(
        &mut p,
        &[(190.0, 90.0), (250.0, 90.0), (220.0, 10.0)],
        &[(0, 1), (1, 2), (2, 0)],
        vec![VectorRegion {
            rule: FillRule::NonZero,
            loops: vec![vec![0, 1, 2]],
            fills: solid(0, 0, 255),
        }],
        PropsPatch {
            strokes: black(2.0, StrokeAlign::Center),
            ..Default::default()
        },
    );
    let img = render(&p.scene(), &camera(260.0, 100.0));
    assert_px(&img, 20, 20, [255, 0, 0], 0);
    assert_px(&img, 50, 50, [255, 255, 255], 0);
    assert_px(&img, 110, 20, [255, 0, 0], 0);
    assert_px(&img, 140, 50, [255, 0, 0], 0);
    assert_px(&img, 220, 70, [0, 0, 255], 0);
    assert_px(&img, 220, 90, [0, 0, 0], 40);
    assert_px(&img, 220, 94, [255, 255, 255], 0);
    golden("vector_regions", &img);
}

#[test]
fn stroke_caps() {
    let mut p = Page::new();
    // Окончание на обоих концах отрезка (40, y)–(200, y) толщиной 4.
    for (i, cap) in StrokeCap::ALL.into_iter().enumerate() {
        let y = 20.0 + 30.0 * i as f64;
        add_vector(
            &mut p,
            &[(40.0, y), (200.0, y)],
            &[(0, 1)],
            vec![],
            PropsPatch {
                strokes: black(4.0, StrokeAlign::Center),
                stroke_cap_start: Some(cap),
                stroke_cap_end: Some(cap),
                ..Default::default()
            },
        );
    }
    // Полупрозрачная обводка с кругами на концах: перекрытие не темнее.
    add_vector(
        &mut p,
        &[(40.0, 230.0), (200.0, 230.0)],
        &[(0, 1)],
        vec![],
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::rgba(0, 0, 0, 128),
                width: 4.0,
                align: StrokeAlign::Center,
            }]),
            stroke_cap_end: Some(StrokeCap::Circle),
            ..Default::default()
        },
    );
    let img = render(&p.scene(), &camera(240.0, 250.0));
    let row = |i: usize| 20 + 30 * i as u16;
    // None: плоский конец.
    assert_px(&img, 198, row(0), [0, 0, 0], 0);
    assert_px(&img, 202, row(0), [255, 255, 255], 0);
    assert_px(&img, 38, row(0), [255, 255, 255], 0);
    // Round: полукруг радиуса 2 за концом, угол квадрата пуст.
    assert_px(&img, 201, row(1), [0, 0, 0], 60);
    assert_px(&img, 203, row(1), [255, 255, 255], 0);
    assert_px(&img, 201, row(1) + 1, [255, 255, 255], 120);
    // Square: квадрат на 2 за концом, угол залит.
    assert_px(&img, 201, row(2) + 1, [0, 0, 0], 0);
    assert_px(&img, 203, row(2), [255, 255, 255], 0);
    // ArrowLines: плечо длиной 18 под 30° назад.
    assert_px(&img, 192, row(3) + 4, [0, 0, 0], 60);
    assert_px(&img, 38, row(3) + 4, [255, 255, 255], 0);
    assert_px(&img, 48, row(3) + 4, [0, 0, 0], 60);
    // ArrowTriangle: остриё на 204, у основания шире линии.
    assert_px(&img, 195, row(4) + 3, [0, 0, 0], 0);
    assert_px(&img, 206, row(4), [255, 255, 255], 0);
    assert_px(&img, 34, row(4), [255, 255, 255], 0);
    assert_px(&img, 38, row(4), [0, 0, 0], 60);
    // Circle: радиус 7,5; Diamond: тот же радиус ромбом — угол пуст.
    assert_px(&img, 200, row(5) + 6, [0, 0, 0], 0);
    assert_px(&img, 204, row(5) + 4, [0, 0, 0], 0);
    assert_px(&img, 200, row(5) + 9, [255, 255, 255], 0);
    assert_px(&img, 200, row(6) + 5, [0, 0, 0], 0);
    assert_px(&img, 204, row(6) + 4, [255, 255, 255], 0);
    // Полупрозрачная: линия и круг одного тона, перекрытия нет.
    let line = px(&img, 150, 230);
    let joint = px(&img, 198, 230);
    let dot = px(&img, 200, 236);
    assert!(line[0].abs_diff(127) <= 2, "{line:?}");
    assert!(line[0].abs_diff(joint[0]) <= 2, "{line:?} {joint:?}");
    assert!(line[0].abs_diff(dot[0]) <= 2, "{line:?} {dot:?}");
    golden("stroke_caps", &img);
}

#[test]
fn stroke_joins_and_closed_alignment() {
    let mut p = Page::new();
    // «V» толщиной 10: острый, круглый, скошенный и острый с пределом 1,5.
    let joins = [
        (10.0, StrokeJoin::Miter, DEFAULT_MITER_LIMIT),
        (80.0, StrokeJoin::Round, DEFAULT_MITER_LIMIT),
        (150.0, StrokeJoin::Bevel, DEFAULT_MITER_LIMIT),
        (220.0, StrokeJoin::Miter, 1.5),
    ];
    for (x, join, limit) in joins {
        add_vector(
            &mut p,
            &[(x, 20.0), (x + 30.0, 90.0), (x + 60.0, 20.0)],
            &[(0, 1), (1, 2)],
            vec![],
            PropsPatch {
                strokes: black(10.0, StrokeAlign::Center),
                stroke_join: Some(join),
                stroke_miter_limit: Some(limit),
                ..Default::default()
            },
        );
    }
    // Замкнутый квадрат 30 × 30 с обводкой 6 внутри, по центру и снаружи — те же
    // полосы, что у прямоугольника (`golden::stroke_alignment`).
    for (x, align) in [
        (15.0, StrokeAlign::Inside),
        (65.0, StrokeAlign::Center),
        (115.0, StrokeAlign::Outside),
    ] {
        add_vector(
            &mut p,
            &[(x, 130.0), (x + 30.0, 130.0), (x + 30.0, 160.0), (x, 160.0)],
            &[(0, 1), (1, 2), (2, 3), (3, 0)],
            vec![VectorRegion {
                rule: FillRule::NonZero,
                loops: vec![vec![0, 1, 2, 3]],
                fills: None,
            }],
            PropsPatch {
                fills: solid(200, 200, 200),
                strokes: black(6.0, align),
                ..Default::default()
            },
        );
    }
    // Прямоугольник с круглым соединением: угол скруглён.
    p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(vec![]),
            strokes: black(10.0, StrokeAlign::Center),
            stroke_join: Some(StrokeJoin::Round),
            ..rect(180.0, 130.0, 40.0, 30.0)
        },
    );
    // Линия со стрелкой на конце (инструмент «Стрелка»).
    p.add(
        None,
        NodeKind::Line,
        PropsPatch {
            strokes: black(2.0, StrokeAlign::Center),
            stroke_cap_end: Some(StrokeCap::ArrowLines),
            ..rect(240.0, 145.0, 50.0, 0.0)
        },
    );
    let img = render(&p.scene(), &camera(300.0, 180.0));
    // Остриё «V»: острый стык уходит до 102,7, круглый — до 95, скошенный — до 92.
    assert_px(&img, 40, 99, [0, 0, 0], 0);
    assert_px(&img, 110, 99, [255, 255, 255], 0);
    assert_px(&img, 110, 93, [0, 0, 0], 0);
    assert_px(&img, 180, 93, [255, 255, 255], 0);
    assert_px(&img, 180, 90, [0, 0, 0], 0);
    // Предел 1,5 < 2,54: острый стык становится скошенным.
    assert_px(&img, 250, 99, [255, 255, 255], 0);
    assert_px(&img, 250, 93, [255, 255, 255], 0);
    // Квадрат: внутри — пиксели 15..=20, по центру — 62..=67, снаружи — 109..=114.
    assert_px(&img, 14, 145, [255, 255, 255], 0);
    assert_px(&img, 15, 145, [0, 0, 0], 0);
    assert_px(&img, 20, 145, [0, 0, 0], 0);
    assert_px(&img, 21, 145, [200, 200, 200], 0);
    assert_px(&img, 61, 145, [255, 255, 255], 0);
    assert_px(&img, 62, 145, [0, 0, 0], 0);
    assert_px(&img, 67, 145, [0, 0, 0], 0);
    assert_px(&img, 68, 145, [200, 200, 200], 0);
    assert_px(&img, 108, 145, [255, 255, 255], 0);
    assert_px(&img, 109, 145, [0, 0, 0], 0);
    assert_px(&img, 114, 145, [0, 0, 0], 0);
    assert_px(&img, 115, 145, [200, 200, 200], 0);
    // Круглый угол прямоугольника: у острого (175, 125) было бы чёрным.
    assert_px(&img, 175, 125, [255, 255, 255], 0);
    assert_px(&img, 200, 126, [0, 0, 0], 0);
    // Стрелка: плечо под концом линии (290, 145).
    assert_px(&img, 284, 148, [0, 0, 0], 90);
    assert_px(&img, 284, 152, [255, 255, 255], 0);
    golden("stroke_joins", &img);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-render --test vector`
Expected: FAIL — тесты собираются, но падают: вектор не рисуется — `пиксель (20, 20) = [255, 255, 255, 255], ожидался [255, 0, 0]` (и эталонов ещё нет)

- [ ] **Step 3: Реализация**

В `crates/ferrum-render/src/display.rs` заменить:

````rust
//! узлов выбрасываются (`Painter::evict`), а описание остаётся.

use ferrum_doc::{Color, FillKind, NodeKind, Props, ScaleMode, Stroke, StrokeAlign};
use ferrum_scene::shape::{self, Geometry};
use ferrum_scene::SceneNode;
use kurbo::BezPath;

use crate::backend::Rgba;
````

на:

````rust
//! узлов выбрасываются (`Painter::evict`), а описание остаётся.

use std::sync::Arc;

use ferrum_doc::{
    Color, Fill, FillKind, NodeKind, Props, ScaleMode, Stroke, StrokeAlign, StrokeStyle,
};
use ferrum_scene::shape::{self, Geometry};
use ferrum_scene::vector::{self, PathShape, StrokeClip};
use ferrum_scene::SceneNode;
use kurbo::{BezPath, Shape};

use crate::backend::Rgba;
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
        width: f64,
    },
}

````

на:

````rust
        width: f64,
    },
    /// Заливка области вектора номер `region` (своей заливкой или заливкой узла).
    Region { region: usize, paint: Paint },
    /// Обводка по контуру (вектор, линия с окончаниями, фигура с соединением не
    /// «острым»): контур обводки заливается, внутри и снаружи замкнутого пути —
    /// с обрезкой по нему.
    PathStroke {
        stroke: Stroke,
        color: Rgba,
        clip: StrokeClip,
    },
}

````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
    /// Кольцо обводки или сжатая форма; `None` — элементу хватает `shape`.
    pub items: Vec<Option<BezPath>>,
}

````

на:

````rust
    /// Кольцо обводки или сжатая форма; `None` — элементу хватает `shape`.
    pub items: Vec<Option<BezPath>>,
    /// Обрезка обводок по контуру: внутренность замкнутого пути и его внешность.
    pub inside: Option<BezPath>,
    pub outside: Option<BezPath>,
}

````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
    /// Номер кадра, в котором узел последний раз рисовался полностью.
    pub last_used: u64,
}

````

на:

````rust
    /// Номер кадра, в котором узел последний раз рисовался полностью.
    pub last_used: u64,
    /// Контуры для обводки по контуру и областей вектора (`None` — кольца).
    /// Держится вместе со списком: по нему контуры строятся заново после
    /// вытеснения, без свойств узла.
    pub path: Option<Arc<PathSource>>,
}

/// Контуры узла с обводкой по контуру и стиль обводки (соединение, предел угла).
#[derive(Debug, Clone, PartialEq)]
pub struct PathSource {
    pub shape: PathShape,
    pub style: StrokeStyle,
}

````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
        return Some(crate::text_node::lod_color(p));
    }
    if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
        if let Some(f) = p.fills.iter().find(|f| f.visible && f.opacity > 0.0) {
            return Some(match f.kind {
                FillKind::Solid => color(f.color).multiply_alpha(f.opacity as f32),
                FillKind::Image => IMAGE_PLACEHOLDER.multiply_alpha(f.opacity as f32),
            });
        }
    }
````

на:

````rust
        return Some(crate::text_node::lod_color(p));
    }
    if p.kind == NodeKind::Vector {
        // Заливка первой области (своя или узла), иначе обводка.
        let net = p.vector.as_deref()?;
        let fill = net
            .regions
            .iter()
            .flat_map(|r| r.fills.as_deref().unwrap_or(&p.fills))
            .find(|f| f.visible && f.opacity > 0.0);
        if let Some(f) = fill {
            return Some(paint_lod(f));
        }
        return p
            .strokes
            .iter()
            .find(|s| s.width > 0.0 && s.color.a > 0)
            .map(|s| color(s.color));
    }
    if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
        if let Some(f) = p.fills.iter().find(|f| f.visible && f.opacity > 0.0) {
            return Some(paint_lod(f));
        }
    }
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
}

impl DisplayList {
    pub fn build(node: &SceneNode, split: bool) -> DisplayList {
        let p: &Props = &node.props;
        let geometry = shape::geometry(p);
        let mut items = Vec::new();
        // У линии и группы своей заливки нет.
        let fillable = !matches!(p.kind, NodeKind::Line | NodeKind::Group);
        if fillable {
            for f in p.fills.iter().filter(|f| f.visible && f.opacity > 0.0) {
                let paint = match (f.kind, &f.image_hash) {
                    (FillKind::Solid, _) => {
                        Paint::Solid(color(f.color).multiply_alpha(f.opacity as f32))
                    }
                    (FillKind::Image, Some(hash)) => Paint::Image {
                        hash: hash.clone(),
                        mode: f.scale_mode,
                        alpha: f.opacity as f32,
                    },
                    (FillKind::Image, None) => continue,
                };
                items.push(Item::Fill(paint));
            }
        }
        if p.kind != NodeKind::Group {
            for s in p.strokes.iter().filter(|s| s.width > 0.0 && s.color.a > 0) {
                items.push(Item::Stroke(s.clone(), color(s.color)));
            }
        }
````

на:

````rust
}

fn paint_lod(f: &Fill) -> Rgba {
    match f.kind {
        FillKind::Solid => color(f.color).multiply_alpha(f.opacity as f32),
        FillKind::Image => IMAGE_PLACEHOLDER.multiply_alpha(f.opacity as f32),
    }
}

/// Краска заливки; `None` — невидимая или изображение без хэша.
fn paint_of(f: &Fill) -> Option<Paint> {
    if !f.visible || f.opacity <= 0.0 {
        return None;
    }
    match (f.kind, &f.image_hash) {
        (FillKind::Solid, _) => Some(Paint::Solid(
            color(f.color).multiply_alpha(f.opacity as f32),
        )),
        (FillKind::Image, Some(hash)) => Some(Paint::Image {
            hash: hash.clone(),
            mode: f.scale_mode,
            alpha: f.opacity as f32,
        }),
        (FillKind::Image, None) => None,
    }
}

impl DisplayList {
    pub fn build(node: &SceneNode, split: bool) -> DisplayList {
        let p: &Props = &node.props;
        let geometry = shape::geometry(p);
        let path_shape = vector::path_shape(p);
        let mut items = Vec::new();
        if let (NodeKind::Vector, Some(ps)) = (p.kind, &path_shape) {
            // Области снизу вверх, у каждой — её заливки (или заливки узла).
            for (region, r) in ps.regions.iter().enumerate() {
                for f in r.fills.as_deref().unwrap_or(&p.fills) {
                    if let Some(paint) = paint_of(f) {
                        items.push(Item::Region { region, paint });
                    }
                }
            }
        } else if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
            // У линии и группы своей заливки нет.
            items.extend(p.fills.iter().filter_map(paint_of).map(Item::Fill));
        }
        if p.kind != NodeKind::Group {
            for s in p.strokes.iter().filter(|s| s.width > 0.0 && s.color.a > 0) {
                items.push(match &path_shape {
                    Some(ps) => Item::PathStroke {
                        stroke: s.clone(),
                        color: color(s.color),
                        clip: vector::stroke_clip(ps, s),
                    },
                    None => Item::Stroke(s.clone(), color(s.color)),
                });
            }
        }
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
            split,
            last_used: 0,
        }
    }
````

на:

````rust
            split,
            last_used: 0,
            path: path_shape.map(|shape| {
                Arc::new(PathSource {
                    shape,
                    style: p.stroke_style,
                })
            }),
        }
    }
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
        let geom = self.geometry;
        let items = &self.items;
        self.paths.get_or_insert_with(|| {
            let mut shape = BezPath::new();
            geom.fill_path_into(&mut shape);
            let items = items
                .iter()
                .map(|item| match item {
````

на:

````rust
        let geom = self.geometry;
        let items = &self.items;
        let list = &self.items;
        let ps = self.path.as_deref().map(|p| &p.shape);
        let style = self.path.as_deref().map(|p| p.style).unwrap_or_default();
        self.paths.get_or_insert_with(|| {
            let mut shape = BezPath::new();
            geom.fill_path_into(&mut shape);
            let items: Vec<Option<BezPath>> = items
                .iter()
                .map(|item| match item {
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
                    Item::Stroke(s, _) => geom.stroke_ring(s),
                    Item::FilledInsideStroke { width, .. } => geom.inset_path(*width),
                })
                .collect();
            Paths { shape, items }
        })
    }
}
````

на:

````rust
                    Item::Stroke(s, _) => geom.stroke_ring(s),
                    Item::FilledInsideStroke { width, .. } => geom.inset_path(*width),
                    Item::Region { region, .. } => ps.map(|ps| ps.regions[*region].path.clone()),
                    Item::PathStroke { stroke, .. } => {
                        ps.map(|ps| vector::stroke_outline(ps, stroke, &style))
                    }
                })
                .collect();
            let wants = |c: StrokeClip| {
                list.iter()
                    .any(|i| matches!(i, Item::PathStroke { clip, .. } if *clip == c))
            };
            let inside = ps
                .filter(|_| wants(StrokeClip::Inside))
                .map(|ps| ps.stroke.clone());
            // Запас рамки внешности — шире самой толстой обводки.
            let margin = list
                .iter()
                .filter_map(|i| match i {
                    Item::PathStroke { stroke, .. } => Some(2.0 * stroke.width + 1.0),
                    _ => None,
                })
                .fold(1.0, f64::max);
            let outside = ps
                .filter(|_| wants(StrokeClip::Outside))
                .map(|ps| exterior(&ps.stroke, margin));
            Paths {
                shape,
                items,
                inside,
                outside,
            }
        })
    }
}

/// Внешность замкнутого пути: рамка с запасом `margin` и сам путь с обходом
/// против неё — при ненулевом правиле обрезки путь вырезается (как у текста
/// снаружи).
pub fn exterior(path: &BezPath, margin: f64) -> BezPath {
    let r = path.bounding_box().inflate(margin, margin);
    let mut out = BezPath::new();
    out.move_to((r.x0, r.y0));
    out.line_to((r.x1, r.y0));
    out.line_to((r.x1, r.y1));
    out.line_to((r.x0, r.y1));
    out.close_path();
    // Рамка выше обходится с положительной площадью; путь — в обратную сторону.
    if path.area() > 0.0 {
        out.extend(path.reverse_subpaths().iter());
    } else {
        out.extend(path.iter());
    }
    out
}
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
use std::time::Duration;

use ferrum_doc::{NodeId, NodeKind, ScaleMode};
use ferrum_scene::shape::Geometry;
use ferrum_scene::{Scene, SceneNode, Slot};
use ferrum_text::{Fonts, LayoutRequest, TextLayout};
````

на:

````rust
use std::time::Duration;

use ferrum_doc::{FillRule, NodeId, NodeKind, ScaleMode};
use ferrum_scene::shape::Geometry;
use ferrum_scene::vector::StrokeClip;
use ferrum_scene::{Scene, SceneNode, Slot};
use ferrum_text::{Fonts, LayoutRequest, TextLayout};
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        clip: bool,
        layer: bool,
        stroke: Option<(Slot, Option<DisplayList>)>,
    },
}
````

на:

````rust
        clip: bool,
        layer: bool,
        stroke: Option<(Slot, Option<Box<DisplayList>>)>,
    },
}
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
                    if let Some((slot, held)) = stroke {
                        let n = scene.at(slot);
                        self.draw_own(backend, ctx, slot, n, 1.0, Part::Strokes, held);
                    }
````

на:

````rust
                    if let Some((slot, held)) = stroke {
                        let n = scene.at(slot);
                        let held = held.map(|b| *b);
                        self.draw_own(backend, ctx, slot, n, 1.0, Part::Strokes, held);
                    }
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
                    self.stats.clips += 1;
                }
                let stroke = (split && !n.props.strokes.is_empty()).then_some((slot, held));
                stack.push(Visit::Exit {
                    clip,
````

на:

````rust
                    self.stats.clips += 1;
                }
                let stroke =
                    (split && !n.props.strokes.is_empty()).then_some((slot, held.map(Box::new)));
                stack.push(Visit::Exit {
                    clip,
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        layered: false,
    };
    let single_paint =
        list.items.len() == 1 && !matches!(list.items[0], Item::FilledInsideStroke { .. });
    if alpha < 1.0 && !single_paint && !list.items.is_empty() {
        backend.push_opacity_layer(alpha);
````

на:

````rust
        layered: false,
    };
    // Одна краска — одна заливка; у обводки по контуру с окончаниями фигуры
    // перекрывают путь, поэтому прозрачность умножать в краску нельзя.
    let single_paint = list.items.len() == 1
        && matches!(
            list.items[0],
            Item::Fill(_) | Item::Stroke(..) | Item::Region { .. }
        );
    if alpha < 1.0 && !single_paint && !list.items.is_empty() {
        backend.push_opacity_layer(alpha);
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        .expect("контуры построены до воспроизведения");
    for (item, item_path) in list.items.iter().zip(&paths.items) {
        let is_fill = matches!(item, Item::Fill(_));
        match part {
            Part::Fills if !is_fill => continue,
````

на:

````rust
        .expect("контуры построены до воспроизведения");
    for (item, item_path) in list.items.iter().zip(&paths.items) {
        let is_fill = matches!(item, Item::Fill(_) | Item::Region { .. });
        match part {
            Part::Fills if !is_fill => continue,
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
                    backend.set_color(c.multiply_alpha(alpha));
                    backend.fill_path(ring, Fill::EvenOdd);
                }
            }
````

на:

````rust
                    backend.set_color(c.multiply_alpha(alpha));
                    backend.fill_path(ring, Fill::EvenOdd);
                }
            }
            Item::Region { region, paint } => {
                let (Some(path), Some(ps)) = (item_path, list.path.as_deref().map(|p| &p.shape))
                else {
                    continue;
                };
                let rule = match ps.regions[*region].rule {
                    FillRule::NonZero => Fill::NonZero,
                    FillRule::EvenOdd => Fill::EvenOdd,
                };
                match paint {
                    Paint::Solid(c) => {
                        backend.set_color(c.multiply_alpha(alpha));
                        backend.fill_path(path, rule);
                    }
                    Paint::Image {
                        hash,
                        mode,
                        alpha: a,
                    } => {
                        // Изображение — по рамке узла, как у фигур; область его обрезает.
                        let Some(area) = geom.bounds() else { continue };
                        if !(area.width() > 0.0 && area.height() > 0.0) {
                            continue;
                        }
                        let Some(pixmap) = images.image(hash) else {
                            info.missing_image = true;
                            backend.set_color(IMAGE_PLACEHOLDER.multiply_alpha(a * alpha));
                            backend.fill_path(path, rule);
                            continue;
                        };
                        let (iw, ih) = (pixmap.width() as f64, pixmap.height() as f64);
                        let (transform, extend, fit) = place_image(*mode, area, iw, ih);
                        backend.set_image(&ImagePaint {
                            key: ImageKey::for_hash(hash),
                            pixmap,
                            transform,
                            extend,
                            alpha: a * alpha,
                        });
                        match fit {
                            Some(rect) => {
                                backend.push_clip(path);
                                backend.fill_rect(&rect);
                                backend.pop_clip();
                            }
                            None => backend.fill_path(path, rule),
                        }
                    }
                }
            }
            Item::PathStroke { color, clip, .. } => {
                let Some(outline) = item_path else { continue };
                let clip_path = match clip {
                    StrokeClip::None => None,
                    StrokeClip::Inside => paths.inside.as_ref(),
                    StrokeClip::Outside => paths.outside.as_ref(),
                };
                // Окончания и стыки перекрывают путь: полупрозрачная обводка —
                // слоем, иначе перекрытия темнее.
                let a = color.components[3] * alpha;
                let layered = a < 1.0;
                if layered {
                    backend.push_opacity_layer(a);
                }
                if let Some(c) = clip_path {
                    backend.push_clip(c);
                }
                backend.set_color(if layered {
                    color.with_alpha(1.0)
                } else {
                    *color
                });
                backend.fill_path(outline, Fill::NonZero);
                if clip_path.is_some() {
                    backend.pop_clip();
                }
                if layered {
                    backend.pop_layer();
                    info.layered = true;
                }
            }
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
        }
        Geometry::None => path = to_device * n.local_bounds?.to_path(0.1),
        geom => {
            geom.fill_path_into(&mut path);
````

на:

````rust
        }
        Geometry::None => path = to_device * n.local_bounds?.to_path(0.1),
        // Вектор подсвечивается своей сетью, а не рамкой.
        Geometry::Vector(_) => {
            path = ferrum_scene::vector::network_shape(&n.props).stroke;
            path.apply_affine(to_device);
        }
        geom => {
            geom.fill_path_into(&mut path);
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `export CARGO_TARGET_DIR=$PWD/target && FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test vector`
Expected: созданы эталоны `crates/ferrum-render/tests/golden/{vector_regions,stroke_caps,stroke_joins}.png` (красные квадраты с дыркой и без, синий треугольник; семь окончаний и полупрозрачная линия с кругом; четыре «V», три квадрата, скруглённый угол и стрелка)

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-render 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `vector` рендера — 3 теста, эталоны прежние совпадают

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-render/src/display.rs crates/ferrum-render/src/draw.rs crates/ferrum-render/src/overlay.rs crates/ferrum-render/tests/vector.rs crates/ferrum-render/tests/golden/stroke_caps.png crates/ferrum-render/tests/golden/stroke_joins.png crates/ferrum-render/tests/golden/vector_regions.png
git commit -m "feat(render): векторы — области по правилам заливки и своими заливками, обводка по контуру с окончаниями, соединениями и пределом угла, внутри и снаружи замкнутого пути; эталоны

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 4: Правки сети без документа

Спека 9: всё, что режим правки и перо делают с сетью, — чистыми функциями (тестируются без редактора): фигура → сеть той же геометрии (прямоугольник — со скруглением дугами, эллипс — четыре кривые `mirrorAll`, линия), сеть под аффинным преобразованием, сеть → правка узла (`reframe`), удаление точек и сегментов с перенумерацией и разрывом петель (`compact`), точка на сегменте (де Кастельжо, петли получают оба куска по ходу обхода), удаление с сохранением формы (наименьшие квадраты с теми же касательными, параметр — по длине хорды) и с разрывом, ручки с зеркальностью, изгиб, грани плоского графа и «Заливка области», слияние сетей.

**Files:**
- Create: `crates/ferrum-editor/src/vector_ops.rs`
- Modify: `crates/ferrum-editor/src/lib.rs`
- Test: `crates/ferrum-editor/tests/vector_ops.rs` (новый)

**Interfaces:**
- Consumes: задачи 1–2.
- Produces: `ferrum_editor::vector_ops::{shape_network(&Props) -> Option<VectorNetwork>, transform_network, reframe(&Props, VectorNetwork) -> PropsPatch, compact, add_point(&VectorNetwork, seg, t) -> (VectorNetwork, u32), delete_points_keep_shape, delete_points_break, delete_segments, move_points, set_handle(net, seg, HandleEnd, Vec2, break_mirror), bend(net, seg, t, Vec2), faces, loop_path, toggle_fill(net, Point, node_has_fills) -> Option<VectorNetwork>, merge_networks(&[(VectorNetwork, Affine)])}`, `HandleEnd { Start, End }`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/vector_ops.rs`:

````rust
//! Правки векторной сети без документа (подсистема 3, этап 3): фигура → сеть,
//! рамка после правки, точка на сегменте, удаление с сохранением формы и с
//! разрывом, ручки и зеркальность, изгиб, грани и «Заливка области», слияние.

use std::sync::Arc;

use ferrum_doc::*;
use ferrum_editor::vector_ops::*;
use ferrum_scene::shape::local_transform;
use ferrum_scene::vector::{fit, network_bounds, segment_curve};
use kurbo::{Affine, ParamCurve, Point, Rect, Vec2};

fn net(points: &[(f64, f64)], segs: &[(u32, u32)]) -> VectorNetwork {
    VectorNetwork {
        points: points
            .iter()
            .map(|&(x, y)| VectorPoint::new(x, y))
            .collect(),
        segments: segs
            .iter()
            .map(|&(a, b)| VectorSegment::line(a, b))
            .collect(),
        regions: vec![],
    }
}

fn near(a: Point, b: Point, eps: f64) -> bool {
    (a - b).hypot() <= eps
}

/// Наибольшее отклонение кривых сети `b` от окружности радиуса `r` с центром `c`.
fn circle_error(n: &VectorNetwork, c: Point, r: f64) -> f64 {
    let mut worst: f64 = 0.0;
    for s in &n.segments {
        let curve = segment_curve(n, s);
        for i in 0..=32 {
            let p = curve.eval(i as f64 / 32.0);
            worst = worst.max(((p - c).hypot() - r).abs());
        }
    }
    worst
}

fn props(kind: NodeKind, w: f64, h: f64) -> Props {
    Props {
        w,
        h,
        ..Props::defaults(kind)
    }
}

#[test]
fn shapes_become_networks_of_the_same_geometry() {
    let rect = shape_network(&props(NodeKind::Rect, 100.0, 60.0)).unwrap();
    assert_eq!(rect.points.len(), 4);
    assert_eq!(rect.regions[0].loops, vec![vec![0, 1, 2, 3]]);
    assert!(rect.validate().is_ok());
    let rounded = shape_network(&Props {
        corner_radius: 10.0,
        ..props(NodeKind::Rect, 100.0, 60.0)
    })
    .unwrap();
    assert_eq!((rounded.points.len(), rounded.segments.len()), (8, 8));
    assert_eq!(
        network_bounds(&rounded),
        Some(Rect::new(0.0, 0.0, 100.0, 60.0))
    );
    assert!(rounded.validate().is_ok());
    let circle = shape_network(&props(NodeKind::Ellipse, 200.0, 200.0)).unwrap();
    assert_eq!(circle.points[0].handle_mode, HandleMode::MirrorAll);
    assert!(circle_error(&circle, Point::new(100.0, 100.0), 100.0) < 0.03);
    assert_eq!(
        network_bounds(&circle),
        Some(Rect::new(0.0, 0.0, 200.0, 200.0))
    );
    let line = shape_network(&props(NodeKind::Line, 80.0, 0.0)).unwrap();
    assert_eq!((line.points.len(), line.regions.len()), (2, 0));
    assert!(shape_network(&props(NodeKind::Text, 10.0, 10.0)).is_none());
    // Вектор, растянутый ресайзом, — в координатах своей рамки.
    let mut v = props(NodeKind::Vector, 200.0, 100.0);
    v.vector = Some(Arc::new(net(&[(0.0, 0.0), (100.0, 50.0)], &[(0, 1)])));
    let n = shape_network(&v).unwrap();
    assert!(near(
        Point::new(n.points[1].x, n.points[1].y),
        Point::new(200.0, 100.0),
        1e-9
    ));
}

#[test]
fn reframe_keeps_points_in_place_under_rotation_and_flip() {
    let mut p = props(NodeKind::Vector, 100.0, 50.0);
    p.x = 30.0;
    p.y = -20.0;
    p.rotation = 35.0;
    p.flip_x = true;
    p.flip_y = true;
    p.vector = Some(Arc::new(net(&[(0.0, 0.0), (100.0, 50.0)], &[(0, 1)])));
    // Новая сеть вылезает за прежнюю рамку влево и вниз.
    let grown = net(
        &[(-40.0, 0.0), (100.0, 50.0), (60.0, 90.0)],
        &[(0, 1), (1, 2)],
    );
    let world_before: Vec<Point> = grown
        .points
        .iter()
        .map(|q| local_transform(&p) * Point::new(q.x, q.y))
        .collect();
    let patch = reframe(&p, grown);
    let mut after = p.clone();
    patch.apply_to(&mut after);
    assert_eq!((after.w, after.h), (140.0, 90.0));
    let n = after.vector.as_deref().unwrap();
    assert_eq!(network_bounds(n).unwrap().origin(), Point::ZERO);
    let m = local_transform(&after) * fit(&after);
    for (q, w) in n.points.iter().zip(&world_before) {
        assert!(near(m * Point::new(q.x, q.y), *w, 1e-9), "{q:?}");
    }
}

#[test]
fn add_point_splits_the_curve_without_changing_it() {
    let circle = shape_network(&props(NodeKind::Ellipse, 100.0, 100.0)).unwrap();
    let before = segment_curve(&circle, &circle.segments[1]);
    let (n, p) = add_point(&circle, 1, 0.25);
    assert_eq!(p, 4);
    assert_eq!(n.segments.len(), 5);
    assert_eq!(n.points[4].handle_mode, HandleMode::MirrorAngle);
    let at = before.eval(0.25);
    assert!(near(Point::new(n.points[4].x, n.points[4].y), at, 1e-9));
    // Первый кусок повторяет начало кривой, второй — конец.
    assert!(near(
        segment_curve(&n, &n.segments[1]).eval(0.5),
        before.eval(0.125),
        1e-9
    ));
    assert!(near(
        segment_curve(&n, &n.segments[4]).eval(0.5),
        before.eval(0.625),
        1e-9
    ));
    // Петля области — из пяти сегментов и по-прежнему замкнута.
    assert_eq!(n.regions[0].loops[0], vec![0, 1, 4, 2, 3]);
    assert!(n.validate().is_ok());
    // Точка на отрезке — без ручек.
    let (l, _) = add_point(&net(&[(0.0, 0.0), (10.0, 0.0)], &[(0, 1)]), 0, 0.5);
    assert_eq!(l.points[2], VectorPoint::new(5.0, 0.0));
}

#[test]
fn delete_keeps_shape_or_breaks() {
    let circle = shape_network(&props(NodeKind::Ellipse, 200.0, 200.0)).unwrap();
    let kept = delete_points_keep_shape(&circle, &[1]);
    assert_eq!((kept.points.len(), kept.segments.len()), (3, 3));
    assert!(kept.validate().is_ok());
    assert_eq!(kept.regions[0].loops[0].len(), 3, "область осталась");
    let err = circle_error(&kept, Point::new(100.0, 100.0), 100.0);
    assert!(
        err < 4.0,
        "одна кривая вместо двух — почти та же дуга: {err}"
    );
    // Угол между отрезками — отрезок.
    let corner = net(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], &[(0, 1), (1, 2)]);
    let line = delete_points_keep_shape(&corner, &[1]);
    assert_eq!(line.segments, vec![VectorSegment::line(0, 1)]);
    // Конец пути уходит со своим сегментом; несколько точек сразу.
    let path = net(
        &[(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)],
        &[(0, 1), (1, 2), (2, 3)],
    );
    let short = delete_points_keep_shape(&path, &[3, 1]);
    assert_eq!(short.points.len(), 2);
    assert_eq!(short.segments, vec![VectorSegment::line(0, 1)]);
    // С разрывом: точка уходит с обоими сегментами, петля области — тоже.
    let broken = delete_points_break(&circle, &[1]);
    assert_eq!((broken.points.len(), broken.segments.len()), (3, 2));
    assert!(broken.regions.is_empty());
    // Сегмент: концы остаются, пока у них есть другие сегменты.
    let gone = delete_segments(&path, &[0]);
    assert_eq!(gone.points.len(), 3, "точка 0 без сегментов ушла");
    assert_eq!(gone.segments.len(), 2);
}

#[test]
fn handles_mirror_by_mode_and_alt_breaks() {
    let circle = shape_network(&props(NodeKind::Ellipse, 100.0, 100.0)).unwrap();
    // Точка 1 (справа): сегмент 0 приходит в неё концом, сегмент 1 выходит началом.
    let n = set_handle(&circle, 1, HandleEnd::Start, Vec2::new(10.0, 40.0), false);
    let other = n.segments[0].tangent_end;
    assert!((Vec2::new(other[0], other[1]) + Vec2::new(10.0, 40.0)).hypot() < 1e-9);
    let mut angle = circle.clone();
    angle.points[1].handle_mode = HandleMode::MirrorAngle;
    let own = Vec2::new(
        circle.segments[0].tangent_end[0],
        circle.segments[0].tangent_end[1],
    )
    .hypot();
    let n = set_handle(&angle, 1, HandleEnd::Start, Vec2::new(0.0, 80.0), false);
    let t = n.segments[0].tangent_end;
    assert!(
        (Vec2::new(t[0], t[1]) - Vec2::new(0.0, -own)).hypot() < 1e-9,
        "длина своя"
    );
    let n = set_handle(&circle, 1, HandleEnd::Start, Vec2::new(10.0, 40.0), true);
    assert_eq!(n.points[1].handle_mode, HandleMode::Free);
    assert_eq!(n.segments[0].tangent_end, circle.segments[0].tangent_end);
}

#[test]
fn bend_moves_the_curve_point_under_the_cursor() {
    let n = net(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1)]);
    for t in [0.3, 0.5, 0.8] {
        let d = Vec2::new(5.0, 30.0);
        let b = bend(&n, 0, t, d);
        let before = segment_curve(&n, &n.segments[0]).eval(t);
        let after = segment_curve(&b, &b.segments[0]).eval(t);
        assert!(near(after, before + d, 1e-9), "t = {t}");
        assert_eq!((b.points[0], b.points[1]), (n.points[0], n.points[1]));
    }
}

#[test]
fn faces_and_paint_bucket() {
    // Квадрат с диагональю: две треугольные грани, внешняя не входит.
    let sq = net(
        &[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)],
        &[(0, 1), (1, 2), (2, 3), (3, 0), (0, 2)],
    );
    let f = faces(&sq);
    assert_eq!(f.len(), 2, "{f:?}");
    let filled = toggle_fill(&sq, Point::new(70.0, 30.0), false).unwrap();
    assert_eq!(filled.regions.len(), 1);
    let mut loop_ = filled.regions[0].loops[0].clone();
    loop_.sort();
    assert_eq!(loop_, vec![0, 1, 4], "правый верхний треугольник");
    assert_eq!(
        filled.regions[0].fills,
        Some(vec![Fill::solid(Color::SHAPE_GRAY)])
    );
    assert!(filled.validate().is_ok());
    // Узел с заливками: область берёт их (`None`).
    let own = toggle_fill(&sq, Point::new(30.0, 70.0), true).unwrap();
    assert_eq!(own.regions[0].fills, None);
    // Повторный щелчок снимает заливку; мимо граней — ничего.
    let cleared = toggle_fill(&filled, Point::new(70.0, 30.0), false).unwrap();
    assert!(cleared.regions.is_empty());
    assert!(toggle_fill(&sq, Point::new(150.0, 50.0), false).is_none());
    // Открытый путь граней не имеет.
    assert!(faces(&net(&[(0.0, 0.0), (10.0, 0.0)], &[(0, 1)])).is_empty());
}

#[test]
fn merge_offsets_indices_and_transforms_handles() {
    let a = shape_network(&props(NodeKind::Rect, 10.0, 10.0)).unwrap();
    let b = shape_network(&props(NodeKind::Ellipse, 20.0, 10.0)).unwrap();
    let m = merge_networks(&[
        (a, Affine::IDENTITY),
        (
            b.clone(),
            Affine::translate((100.0, 0.0)) * Affine::scale(2.0),
        ),
    ]);
    assert_eq!(
        (m.points.len(), m.segments.len(), m.regions.len()),
        (8, 8, 2)
    );
    assert_eq!(m.regions[1].loops[0], vec![4, 5, 6, 7]);
    assert_eq!(m.segments[4].start, 4);
    assert_eq!(m.points[4].x, 100.0 + 2.0 * b.points[0].x);
    assert_eq!(
        m.segments[4].tangent_start[0],
        2.0 * b.segments[0].tangent_start[0]
    );
    assert!(m.validate().is_ok());
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor --test vector_ops`
Expected: FAIL — не собирается: нет модуля `ferrum_editor::vector_ops`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
mod text_style;
pub mod tools;

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
````

на:

````rust
mod text_style;
pub mod tools;
pub mod vector_ops;

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
````

Создать `crates/ferrum-editor/src/vector_ops.rs`:

````rust
//! Правки векторной сети без документа (спека подсистемы 3, раздел 9): фигура →
//! сеть, сеть → рамка узла, добавление точки на сегменте, удаление с сохранением
//! формы и с разрывом, ручки с зеркальностью, изгиб сегмента, грани для
//! «Заливки области», сплющивание нескольких узлов. Все функции чистые: на входе
//! сеть и параметры, на выходе новая сеть — режим правки и перо записывают её
//! одной транзакцией.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use ferrum_doc::{
    Color, Fill, FillRule, HandleMode, NodeKind, Props, PropsPatch, VectorNetwork, VectorPoint,
    VectorRegion, VectorSegment,
};
use ferrum_scene::vector::{fit, network_bounds, segment_curve};
use kurbo::{Affine, BezPath, CubicBez, ParamCurve, PathSeg, Point, Shape, Vec2};

/// Доля радиуса для ручек дуги в четверть окружности.
const KAPPA: f64 = 0.552_284_749_830_793_4;

fn v2(t: [f64; 2]) -> Vec2 {
    Vec2::new(t[0], t[1])
}

fn arr(v: Vec2) -> [f64; 2] {
    [v.x, v.y]
}

fn pt(net: &VectorNetwork, i: u32) -> Point {
    let p = &net.points[i as usize];
    Point::new(p.x, p.y)
}

/// Сеть узла в координатах его формы (`0..w × 0..h`, до отражения): у вектора —
/// растянутая под рамку, у прямоугольника, эллипса и линии — их контур (как в
/// Figma при входе в правку фигуры). `None` — у узла нет контура (группа, текст,
/// изображение, фрейм).
pub fn shape_network(p: &Props) -> Option<VectorNetwork> {
    let (w, h) = (p.w.max(0.0), p.h.max(0.0));
    let closed = |points: Vec<VectorPoint>, segments: Vec<VectorSegment>| {
        let loop_ = (0..segments.len() as u32).collect();
        VectorNetwork {
            points,
            segments,
            regions: vec![VectorRegion {
                rule: FillRule::NonZero,
                loops: vec![loop_],
                fills: None,
            }],
        }
    };
    match p.kind {
        NodeKind::Vector => {
            let net = p.vector.as_deref()?;
            Some(transform_network(net, fit(p)))
        }
        NodeKind::Line => Some(VectorNetwork {
            points: vec![VectorPoint::new(0.0, 0.0), VectorPoint::new(w, 0.0)],
            segments: vec![VectorSegment::line(0, 1)],
            regions: vec![],
        }),
        NodeKind::Rect => {
            let r = p.corner_radius.max(0.0).min(w / 2.0).min(h / 2.0);
            if r <= 0.0 {
                let pts = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
                return Some(closed(
                    pts.iter().map(|&(x, y)| VectorPoint::new(x, y)).collect(),
                    (0..4)
                        .map(|i| VectorSegment::line(i, (i + 1) % 4))
                        .collect(),
                ));
            }
            // Скругление — дугами: по две точки на угол, между ними кривая.
            let k = r * KAPPA;
            let pts = [
                (r, 0.0),
                (w - r, 0.0),
                (w, r),
                (w, h - r),
                (w - r, h),
                (r, h),
                (0.0, h - r),
                (0.0, r),
            ];
            let arcs = [
                ([k, 0.0], [0.0, -k]),
                ([0.0, k], [k, 0.0]),
                ([-k, 0.0], [0.0, k]),
                ([0.0, -k], [-k, 0.0]),
            ];
            let mut segments = Vec::new();
            for c in 0..4u32 {
                segments.push(VectorSegment::line(2 * c, 2 * c + 1));
                let (ts, te) = arcs[c as usize];
                segments.push(VectorSegment {
                    start: 2 * c + 1,
                    end: (2 * c + 2) % 8,
                    tangent_start: ts,
                    tangent_end: te,
                });
            }
            Some(closed(
                pts.iter().map(|&(x, y)| VectorPoint::new(x, y)).collect(),
                segments,
            ))
        }
        NodeKind::Ellipse => {
            let (rx, ry) = (w / 2.0, h / 2.0);
            let (kx, ky) = (rx * KAPPA, ry * KAPPA);
            let point = |x, y| VectorPoint {
                handle_mode: HandleMode::MirrorAll,
                ..VectorPoint::new(x, y)
            };
            // Сверху по часовой: верх, право, низ, лево.
            let points = vec![point(rx, 0.0), point(w, ry), point(rx, h), point(0.0, ry)];
            let t = [
                ([kx, 0.0], [0.0, -ky]),
                ([0.0, ky], [kx, 0.0]),
                ([-kx, 0.0], [0.0, ky]),
                ([0.0, -ky], [-kx, 0.0]),
            ];
            let segments = (0..4u32)
                .map(|i| VectorSegment {
                    start: i,
                    end: (i + 1) % 4,
                    tangent_start: t[i as usize].0,
                    tangent_end: t[i as usize].1,
                })
                .collect();
            Some(closed(points, segments))
        }
        _ => None,
    }
}

/// Сеть под аффинным преобразованием: точки — целиком, ручки — линейной частью.
pub fn transform_network(net: &VectorNetwork, a: Affine) -> VectorNetwork {
    let c = a.as_coeffs();
    let lin = |t: [f64; 2]| [c[0] * t[0] + c[2] * t[1], c[1] * t[0] + c[3] * t[1]];
    VectorNetwork {
        points: net
            .points
            .iter()
            .map(|p| {
                let q = a * Point::new(p.x, p.y);
                VectorPoint {
                    x: q.x,
                    y: q.y,
                    ..*p
                }
            })
            .collect(),
        segments: net
            .segments
            .iter()
            .map(|s| VectorSegment {
                tangent_start: lin(s.tangent_start),
                tangent_end: lin(s.tangent_end),
                ..*s
            })
            .collect(),
        regions: net.regions.clone(),
    }
}

/// Новая сеть узла `p` в координатах его формы → правка узла: сеть сдвигается к
/// началу своих границ, рамка узла — по ним, положение пересчитывается с учётом
/// поворота и отражения так, что на экране ничего не сдвигается.
pub fn reframe(p: &Props, net: VectorNetwork) -> PropsPatch {
    let b = network_bounds(&net).unwrap_or(kurbo::Rect::ZERO);
    let m = b.origin();
    let (nw, nh) = (b.width(), b.height());
    let d = Vec2::new(
        if p.flip_x { p.w - m.x - nw } else { m.x },
        if p.flip_y { p.h - m.y - nh } else { m.y },
    );
    let r = Affine::rotate(-p.rotation.to_radians()) * d.to_point();
    let shifted = transform_network(&net, Affine::translate(-m.to_vec2()));
    PropsPatch {
        x: Some(p.x + r.x),
        y: Some(p.y + r.y),
        w: Some(nw),
        h: Some(nh),
        vector: Some(Arc::new(shifted)),
        ..Default::default()
    }
}

/// Убирает точки `drop_points` и сегменты `drop_segs`, перенумеровывая остальное.
/// Петли, в которых был удалённый сегмент, разорваны — уходят (область без
/// петель — тоже).
pub fn compact(
    net: &VectorNetwork,
    drop_points: &HashSet<u32>,
    drop_segs: &HashSet<u32>,
) -> VectorNetwork {
    let mut point_map = vec![u32::MAX; net.points.len()];
    let mut points = Vec::new();
    for (i, p) in net.points.iter().enumerate() {
        if !drop_points.contains(&(i as u32)) {
            point_map[i] = points.len() as u32;
            points.push(*p);
        }
    }
    let mut seg_map = vec![u32::MAX; net.segments.len()];
    let mut segments = Vec::new();
    for (i, s) in net.segments.iter().enumerate() {
        let (a, b) = (point_map[s.start as usize], point_map[s.end as usize]);
        if drop_segs.contains(&(i as u32)) || a == u32::MAX || b == u32::MAX {
            continue;
        }
        seg_map[i] = segments.len() as u32;
        segments.push(VectorSegment {
            start: a,
            end: b,
            ..*s
        });
    }
    let regions = net
        .regions
        .iter()
        .filter_map(|r| {
            let loops: Vec<Vec<u32>> = r
                .loops
                .iter()
                .filter(|l| l.iter().all(|&s| seg_map[s as usize] != u32::MAX))
                .map(|l| l.iter().map(|&s| seg_map[s as usize]).collect())
                .collect();
            (!loops.is_empty()).then(|| VectorRegion { loops, ..r.clone() })
        })
        .collect();
    VectorNetwork {
        points,
        segments,
        regions,
    }
}

/// Точка на сегменте `seg` при параметре `t`: сегмент делится в этой точке
/// (кривая — по де Кастельжо, форма та же), петли областей получают оба
/// куска в порядке обхода. Возвращает сеть и номер новой точки.
pub fn add_point(net: &VectorNetwork, seg: usize, t: f64) -> (VectorNetwork, u32) {
    let mut out = net.clone();
    let s = net.segments[seg];
    let t = t.clamp(0.0, 1.0);
    let curve = segment_curve(net, &s);
    let at = curve.eval(t);
    let new = out.points.len() as u32;
    let (first, second, mode) = match curve {
        PathSeg::Cubic(c) => {
            let (a, b) = (c.subsegment(0.0..t), c.subsegment(t..1.0));
            (
                VectorSegment {
                    start: s.start,
                    end: new,
                    tangent_start: arr(a.p1 - a.p0),
                    tangent_end: arr(a.p2 - a.p3),
                },
                VectorSegment {
                    start: new,
                    end: s.end,
                    tangent_start: arr(b.p1 - b.p0),
                    tangent_end: arr(b.p2 - b.p3),
                },
                HandleMode::MirrorAngle,
            )
        }
        _ => (
            VectorSegment::line(s.start, new),
            VectorSegment::line(new, s.end),
            HandleMode::None,
        ),
    };
    out.points.push(VectorPoint {
        handle_mode: mode,
        ..VectorPoint::new(at.x, at.y)
    });
    out.segments[seg] = first;
    let second_i = out.segments.len() as u32;
    out.segments.push(second);
    for r in &mut out.regions {
        for l in &mut r.loops {
            let Some(k) = l.iter().position(|&x| x as usize == seg) else {
                continue;
            };
            let Some(order) = net.loop_points(l) else {
                continue;
            };
            // Петля проходит сегмент от его начала — второй кусок после первого.
            if order[k] == s.start {
                l.insert(k + 1, second_i);
            } else {
                l.insert(k, second_i);
            }
        }
    }
    (out, new)
}

/// Сегменты точки.
fn incident(net: &VectorNetwork, p: u32) -> Vec<usize> {
    net.segments
        .iter()
        .enumerate()
        .filter(|(_, s)| s.start == p || s.end == p)
        .map(|(i, _)| i)
        .collect()
}

/// Сегмент `seg` как кривая от точки `from` (развёрнутая, если сегмент идёт к ней).
fn curve_from(net: &VectorNetwork, seg: usize, from: u32) -> CubicBez {
    let c = match segment_curve(net, &net.segments[seg]) {
        PathSeg::Cubic(c) => c,
        PathSeg::Line(l) => CubicBez::new(l.p0, l.p0, l.p1, l.p1),
        PathSeg::Quad(q) => q.raise(),
    };
    if net.segments[seg].start == from {
        c
    } else {
        CubicBez::new(c.p3, c.p2, c.p1, c.p0)
    }
}

/// Одна кубическая кривая вместо двух `a` (от x к p) и `b` (от p к y): концы и
/// направления касательных в x и y те же, длины ручек — наименьшими квадратами
/// по точкам обеих кривых (параметр — по длине хорды, как у Шнайдера).
fn merge_curves(a: CubicBez, b: CubicBez) -> CubicBez {
    let (x, y) = (a.p0, b.p3);
    let dir = |v: Vec2, fallback: Vec2| {
        let v = if v.hypot2() > 1e-18 { v } else { fallback };
        let n = v.hypot();
        if n > 0.0 {
            v / n
        } else {
            Vec2::ZERO
        }
    };
    let t1 = dir(a.p1 - a.p0, a.p3 - a.p0);
    let t2 = dir(b.p2 - b.p3, b.p0 - b.p3);
    // Точки обеих кривых и их параметры по накопленной длине хорды.
    let mut samples = Vec::new();
    for (c, k) in [(a, 0), (b, 1)] {
        for i in 0..=16 {
            if k == 1 && i == 0 {
                continue;
            }
            samples.push(c.eval(i as f64 / 16.0));
        }
    }
    let mut len = vec![0.0];
    for w in samples.windows(2) {
        len.push(len.last().unwrap() + (w[1] - w[0]).hypot());
    }
    let total = *len.last().unwrap();
    if total <= 0.0 {
        return CubicBez::new(x, x, y, y);
    }
    let (mut c11, mut c12, mut c22, mut x1, mut x2) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (q, l) in samples.iter().zip(&len) {
        let u = l / total;
        let (b0, b1, b2, b3) = (
            (1.0 - u).powi(3),
            3.0 * u * (1.0 - u).powi(2),
            3.0 * u * u * (1.0 - u),
            u.powi(3),
        );
        let a1 = t1 * b1;
        let a2 = t2 * b2;
        let rest = q.to_vec2() - (x.to_vec2() * (b0 + b1) + y.to_vec2() * (b2 + b3));
        c11 += a1.dot(a1);
        c12 += a1.dot(a2);
        c22 += a2.dot(a2);
        x1 += a1.dot(rest);
        x2 += a2.dot(rest);
    }
    let det = c11 * c22 - c12 * c12;
    let chord = (y - x).hypot();
    let (mut l1, mut l2) = if det.abs() > 1e-12 {
        ((x1 * c22 - x2 * c12) / det, (c11 * x2 - c12 * x1) / det)
    } else {
        (chord / 3.0, chord / 3.0)
    };
    // Отрицательная длина — петля: тогда треть хорды, как у Шнайдера.
    if l1 < 1e-9 || l2 < 1e-9 {
        l1 = chord / 3.0;
        l2 = chord / 3.0;
    }
    CubicBez::new(x, x + t1 * l1, y + t2 * l2, y)
}

/// Удаление точек с сохранением формы (Delete): у точки с двумя сегментами они
/// сливаются в один, приближающий оба (два отрезка — отрезок); конец пути
/// уходит со своим сегментом; у остальных точек уходят и их сегменты.
pub fn delete_points_keep_shape(net: &VectorNetwork, points: &[u32]) -> VectorNetwork {
    let mut cur = net.clone();
    // По одной, от больших номеров к меньшим: удаление точки сдвигает номера
    // только после неё, поэтому оставшиеся в очереди номера верны.
    let mut queue: Vec<u32> = points.to_vec();
    queue.sort_unstable();
    queue.dedup();
    while let Some(p) = queue.pop() {
        if p as usize >= cur.points.len() {
            continue;
        }
        let segs = incident(&cur, p);
        if let [a, b] = segs[..] {
            let ca = curve_from(&cur, a, p);
            let cb = curve_from(&cur, b, p);
            let other = |s: usize| {
                let s = &cur.segments[s];
                if s.start == p {
                    s.end
                } else {
                    s.start
                }
            };
            let (xi, yi) = (other(a), other(b));
            if xi != yi {
                // Обе прямые — прямая; иначе — приближение двух кривых одной.
                let both_straight = cur.segments[a].is_straight() && cur.segments[b].is_straight();
                let merged = if both_straight {
                    VectorSegment::line(xi, yi)
                } else {
                    // Кривая от x через p к y.
                    let a_rev = CubicBez::new(ca.p3, ca.p2, ca.p1, ca.p0);
                    let m = merge_curves(a_rev, cb);
                    VectorSegment {
                        start: xi,
                        end: yi,
                        tangent_start: arr(m.p1 - m.p0),
                        tangent_end: arr(m.p2 - m.p3),
                    }
                };
                cur.segments[a] = merged;
                // Петли: сегмент `b` уходит, `a` его заменяет.
                for r in &mut cur.regions {
                    for l in &mut r.loops {
                        l.retain(|&s| s as usize != b);
                    }
                }
                let drop_p = HashSet::from([p]);
                let drop_s = HashSet::from([b as u32]);
                cur = compact(&cur, &drop_p, &drop_s);
                continue;
            }
        }
        // Конец пути, ветвление или одиночная точка: точка уходит с сегментами.
        let drop_s: HashSet<u32> = segs.iter().map(|&s| s as u32).collect();
        cur = compact(&cur, &HashSet::from([p]), &drop_s);
    }
    cur
}

/// Удаление с разрывом (Shift+Delete): точки уходят вместе со своими сегментами.
pub fn delete_points_break(net: &VectorNetwork, points: &[u32]) -> VectorNetwork {
    let drop_p: HashSet<u32> = points.iter().copied().collect();
    let drop_s: HashSet<u32> = net
        .segments
        .iter()
        .enumerate()
        .filter(|(_, s)| drop_p.contains(&s.start) || drop_p.contains(&s.end))
        .map(|(i, _)| i as u32)
        .collect();
    compact(net, &drop_p, &drop_s)
}

/// Удаление сегментов: точки остаются, кроме тех, у которых не осталось ни
/// одного сегмента (как в Figma — висящих точек не бывает).
pub fn delete_segments(net: &VectorNetwork, segs: &[u32]) -> VectorNetwork {
    let drop_s: HashSet<u32> = segs.iter().copied().collect();
    let mut degree = vec![0u32; net.points.len()];
    let mut touched = HashSet::new();
    for (i, s) in net.segments.iter().enumerate() {
        if drop_s.contains(&(i as u32)) {
            touched.insert(s.start);
            touched.insert(s.end);
        } else {
            degree[s.start as usize] += 1;
            degree[s.end as usize] += 1;
        }
    }
    let drop_p: HashSet<u32> = touched
        .into_iter()
        .filter(|&p| degree[p as usize] == 0)
        .collect();
    compact(net, &drop_p, &drop_s)
}

/// Сдвиг точек (ручки — смещения от точек — едут вместе с ними).
pub fn move_points(net: &VectorNetwork, points: &[u32], d: Vec2) -> VectorNetwork {
    let mut out = net.clone();
    for &i in points {
        let p = &mut out.points[i as usize];
        p.x += d.x;
        p.y += d.y;
    }
    out
}

/// Какой конец сегмента: ручка у начала или у конца.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleEnd {
    Start,
    End,
}

/// Ручка сегмента `seg` у его конца `end` становится `tangent` (смещение от
/// точки). Зеркальность точки (`MirrorAngle`, `MirrorAll`) поворачивает ручку
/// второго сегмента точки в противоположную сторону (у `MirrorAll` — и той же
/// длины); `break_mirror` (Alt) делает точку независимой (`Free`).
pub fn set_handle(
    net: &VectorNetwork,
    seg: usize,
    end: HandleEnd,
    tangent: Vec2,
    break_mirror: bool,
) -> VectorNetwork {
    let mut out = net.clone();
    let s = &mut out.segments[seg];
    let p = match end {
        HandleEnd::Start => {
            s.tangent_start = arr(tangent);
            s.start
        }
        HandleEnd::End => {
            s.tangent_end = arr(tangent);
            s.end
        }
    };
    let mode = out.points[p as usize].handle_mode;
    if break_mirror {
        out.points[p as usize].handle_mode = HandleMode::Free;
        return out;
    }
    if mode == HandleMode::None {
        // Точка без ручек получила ручку: дальше — зеркально, как у пера Figma.
        out.points[p as usize].handle_mode = HandleMode::MirrorAll;
    }
    let mode = out.points[p as usize].handle_mode;
    if !matches!(mode, HandleMode::MirrorAngle | HandleMode::MirrorAll) {
        return out;
    }
    let segs = incident(&out, p);
    let Some(&other) = segs.iter().find(|&&i| i != seg) else {
        return out;
    };
    if segs.len() != 2 {
        return out;
    }
    let o = &mut out.segments[other];
    let slot = if o.start == p {
        &mut o.tangent_start
    } else {
        &mut o.tangent_end
    };
    let len = tangent.hypot();
    let own = v2(*slot).hypot();
    if len > 0.0 {
        let l = if mode == HandleMode::MirrorAll || own == 0.0 {
            len
        } else {
            own
        };
        *slot = arr(-tangent / len * l);
    }
    out
}

/// Изгиб (Ctrl/Cmd + протяжка сегмента): точка сегмента при параметре `t`
/// уходит на `d`, обе ручки сдвигаются на `d / (3t(1 − t))` — так точка кривой
/// при этом параметре попадает ровно под курсор. Концы не двигаются.
pub fn bend(net: &VectorNetwork, seg: usize, t: f64, d: Vec2) -> VectorNetwork {
    let mut out = net.clone();
    let t = t.clamp(0.1, 0.9);
    let k = d / (3.0 * t * (1.0 - t));
    let s = &mut out.segments[seg];
    if s.is_straight() {
        // Отрезок — кубической кривой с ручками в трети: параметр тот же, что у
        // отрезка (`t` пришёл из попадания по отрезку).
        let third = (pt(net, s.end) - pt(net, s.start)) / 3.0;
        s.tangent_start = arr(third);
        s.tangent_end = arr(-third);
    }
    s.tangent_start = arr(v2(s.tangent_start) + k);
    s.tangent_end = arr(v2(s.tangent_end) + k);
    out
}

/// Грани сети как плоского графа (сегменты без пересечений): петли сегментов,
/// ограничивающие внутренние области; внешняя грань каждой связной части не
/// входит. Обход: из полуребра u→v к следующему по углу касательной в v.
pub fn faces(net: &VectorNetwork) -> Vec<Vec<u32>> {
    let n = net.segments.len();
    // Полуребро 2i — сегмент i от начала, 2i+1 — от конца.
    let from = |h: usize| {
        let s = &net.segments[h / 2];
        if h.is_multiple_of(2) {
            s.start
        } else {
            s.end
        }
    };
    let angle = |h: usize| {
        let s = &net.segments[h / 2];
        let c = segment_curve(net, s);
        let (p, q) = match c {
            PathSeg::Cubic(c) if h.is_multiple_of(2) => {
                (c.p0, if c.p1 != c.p0 { c.p1 } else { c.p2 })
            }
            PathSeg::Cubic(c) => (c.p3, if c.p2 != c.p3 { c.p2 } else { c.p1 }),
            PathSeg::Line(l) if h.is_multiple_of(2) => (l.p0, l.p1),
            PathSeg::Line(l) => (l.p1, l.p0),
            PathSeg::Quad(q) => (q.p0, q.p1),
        };
        (q - p).atan2()
    };
    let mut out_of: HashMap<u32, Vec<usize>> = HashMap::new();
    for h in 0..2 * n {
        out_of.entry(from(h)).or_default().push(h);
    }
    for list in out_of.values_mut() {
        list.sort_by(|&a, &b| angle(a).total_cmp(&angle(b)));
    }
    let mut seen = vec![false; 2 * n];
    let mut result = Vec::new();
    for start in 0..2 * n {
        if seen[start] {
            continue;
        }
        let mut face = Vec::new();
        let mut h = start;
        loop {
            seen[h] = true;
            face.push(h);
            let twin = h ^ 1;
            let list = &out_of[&from(twin)];
            let k = list.iter().position(|&x| x == twin).unwrap_or(0);
            h = list[(k + 1) % list.len()];
            if h == start || seen[h] || face.len() > 2 * n {
                break;
            }
        }
        if h != start {
            continue;
        }
        let path = face_path(net, &face);
        // Внутренние грани обходятся против часовой на экране (площадь kurbo < 0).
        if path.area() < -1e-9 {
            result.push(face.iter().map(|h| (h / 2) as u32).collect());
        }
    }
    result
}

/// Контур грани по полурёбрам.
fn face_path(net: &VectorNetwork, face: &[usize]) -> BezPath {
    let mut path = BezPath::new();
    for (k, &h) in face.iter().enumerate() {
        let c = curve_from(
            net,
            h / 2,
            if h.is_multiple_of(2) {
                net.segments[h / 2].start
            } else {
                net.segments[h / 2].end
            },
        );
        if k == 0 {
            path.move_to(c.p0);
        }
        path.curve_to(c.p1, c.p2, c.p3);
    }
    path.close_path();
    path
}

/// Контур петли сегментов (в порядке обхода) — для попадания в грань.
pub fn loop_path(net: &VectorNetwork, l: &[u32]) -> Option<BezPath> {
    let order = net.loop_points(l)?;
    let mut path = BezPath::new();
    path.move_to(pt(net, order[0]));
    for (k, &s) in l.iter().enumerate() {
        let c = curve_from(net, s as usize, order[k]);
        path.curve_to(c.p1, c.p2, c.p3);
    }
    path.close_path();
    Some(path)
}

/// «Заливка области» (B) в точке `at` (координаты сети): самая маленькая грань
/// под точкой. Залита петлёй какой-то области — заливка снимается (петля
/// уходит), иначе — новая область с заливками узла (у узла без заливок — серая,
/// как у новой фигуры). `None` — под точкой нет замкнутой грани.
pub fn toggle_fill(net: &VectorNetwork, at: Point, node_has_fills: bool) -> Option<VectorNetwork> {
    let face = faces(net)
        .into_iter()
        .filter_map(|f| {
            let path = loop_path(net, &f)?;
            (path.winding(at) != 0).then(|| (path.area().abs(), f))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))?
        .1;
    let key: HashSet<u32> = face.iter().copied().collect();
    let mut out = net.clone();
    let mut removed = false;
    for r in &mut out.regions {
        let before = r.loops.len();
        r.loops
            .retain(|l| l.iter().copied().collect::<HashSet<u32>>() != key);
        removed |= r.loops.len() != before;
    }
    if removed {
        out.regions.retain(|r| !r.loops.is_empty());
        return Some(out);
    }
    out.regions.push(VectorRegion {
        rule: FillRule::NonZero,
        loops: vec![face],
        fills: (!node_has_fills).then(|| vec![Fill::solid(Color::SHAPE_GRAY)]),
    });
    Some(out)
}

/// Сплющивание: сети узлов (каждая — в координатах формы своего узла, с
/// матрицей формы → общие координаты) в одну сеть общих координат.
pub fn merge_networks(parts: &[(VectorNetwork, Affine)]) -> VectorNetwork {
    let mut out = VectorNetwork::default();
    for (net, a) in parts {
        let t = transform_network(net, *a);
        let (po, so) = (out.points.len() as u32, out.segments.len() as u32);
        out.points.extend(t.points);
        out.segments
            .extend(t.segments.into_iter().map(|s| VectorSegment {
                start: s.start + po,
                end: s.end + po,
                ..s
            }));
        out.regions.extend(t.regions.into_iter().map(|r| {
            VectorRegion {
                loops: r
                    .loops
                    .into_iter()
                    .map(|l| l.into_iter().map(|i| i + so).collect())
                    .collect(),
                ..r
            }
        }));
    }
    out
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `vector_ops` — 8 тестов

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/vector_ops.rs crates/ferrum-editor/tests/vector_ops.rs
git commit -m "feat(editor): правки векторной сети — фигура в сеть, рамка по сети, точка на сегменте, удаление с сохранением формы и с разрывом, ручки, изгиб, грани и заливка области, слияние

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 5: Сеанс правки вектора и перо

Спека 9: перо (P) — щелчок, протяжка, замыкание, соединение, Shift, Alt, Esc/Enter. Сеанс правки (`VectorEdit`) — общий для пера и режима правки: сеть в координатах формы, матрица формы → мир, выделение, инструмент внутри правки, путь пера, жест; показ — превью узла (у фигуры — черновик-вектор поверх скрытой фигуры, у нового пути — черновик), запись — одна транзакция (`commit_vector_work`: у фигуры — новый вектор на её месте тем же шагом), сверка с узлом после записи, отмены и правки из панели (`resync_vector_edit`). Перо из тулбара начинает сеанс без узла: первый щелчок задаёт родителя (самый глубокий фрейм под точкой) и позу (компенсация поворота и отражения родителя). Оверлей правки рисует `ferrum-render` (сеть, точки, ручки, резинка пера). Инструмент `Pen`, курсоры `Pen` и `Bucket` — и в протоколе (иначе ядро не собирается).

**Files:**
- Create: `crates/ferrum-editor/src/vector_edit.rs`
- Modify: `crates/ferrum-editor/src/{editor,actions,commands,lib,vector_ops}.rs`, `tools/mod.rs`, `crates/ferrum-render/src/{overlay,lib}.rs`, `crates/ferrum-wasm/src/{protocol,core}.rs`, `apps/web/src/core/core.ts`
- Test: `crates/ferrum-editor/tests/pen.rs` (новый), `crates/ferrum-editor/tests/keymap.rs`, `crates/ferrum-render/tests/vector.rs`, эталон `crates/ferrum-render/tests/golden/vector_edit.png` (новый), `crates/ferrum-wasm/tests/{panels,protocol}.rs`

**Interfaces:**
- Consumes: `vector_ops` задачи 4, `ferrum_scene::vector::nearest_segment`, `geometry::decompose_with`, `selection::drop_frame_at`.
- Produces: `Editor::{start_vector_edit(id) -> bool, end_vector_edit, is_editing_vector, vector_tool, vector_selection, vector_edit_node, vector_work}`; `VectorTool { Select, Pen, Bucket }`, `VectorSelection { points, segments, handles }`; внутри крейта — `VectorEdit`, `Drag`, `commit_vector_work`, `show_vector_work`, `resync_vector_edit`, `finish_pen_path`, `vector_point_at`, `vector_segment_at`, `vector_pointer_{down,move,up}`, `vector_key`, `vector_escape`, `cancel_vector_drag`, `vector_overlay`, `as_vector`, `replace_with_vector`; `ToolKind::Pen`, `Cursor::{Pen, Bucket}`, `CommandId::ToolPen` (`tool.pen`, P), `Outbox::edit_vector`; `VectorEditOverlay { transform, net, points, segments, handles, selected_handles, rubber }`, `Overlay::vector_edit`; `Tool::Pen`, `CursorKind::{Pen, Bucket}`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 45);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 46);
}

````

Создать `crates/ferrum-editor/tests/pen.rs`:

````rust
//! Перо (подсистема 3, этап 3): точки щелчком, ручки протяжкой (зеркально, Alt —
//! разрыв), замыкание по первой точке, соединение с точкой другого контура,
//! Shift — 45°, Esc/Enter — конец пути одним шагом, Ctrl/Cmd+Z — последняя точка,
//! поворот и отражение родителя, перо в правке фигуры.

mod common;

use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;
use ferrum_scene::vector::fit;
use kurbo::Point;

/// Точки вектора в мировых координатах.
fn world_points(h: &mut H, id: NodeId) -> Vec<Point> {
    let n = h.ed.scene().node(id).unwrap().clone();
    let net = n.props.vector.as_deref().unwrap().clone();
    let m = n.world * fit(&n.props);
    net.points
        .iter()
        .map(|p| m * Point::new(p.x, p.y))
        .collect()
}

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-6
}

fn only_vector(h: &mut H) -> NodeId {
    let all = h.children(None);
    let v: Vec<NodeId> = all
        .into_iter()
        .filter(|id| h.props(*id).kind == NodeKind::Vector)
        .collect();
    assert_eq!(v.len(), 1, "один вектор");
    v[0]
}

#[test]
fn clicks_make_an_open_path_in_one_step() {
    let mut h = H::new();
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyP", Modifiers::NONE);
    assert_eq!(h.ed.tool_kind(), ToolKind::Pen);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 100.0, Modifiers::NONE);
    h.click(200.0, 180.0, Modifiers::NONE);
    // До конца пути в документе ничего нет: путь — черновик сцены.
    assert!(h.children(None).is_empty());
    h.key("Enter", Modifiers::NONE);
    let id = only_vector(&mut h);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert_eq!(h.sel(), vec![id]);
    let p = h.props(id);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 100.0, 80.0));
    let net = p.vector.as_deref().unwrap();
    assert_eq!(
        (net.points.len(), net.segments.len(), net.regions.len()),
        (3, 2, 0)
    );
    let w = world_points(&mut h, id);
    assert!(near(w[2], Point::new(200.0, 180.0)));
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn drag_pulls_mirrored_handles_and_alt_breaks_them() {
    let mut h = H::new();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    // Протяжка из (200, 100) на (+30, +40): исходящая ручка к курсору,
    // входящая — зеркально.
    h.drag((200.0, 100.0), (230.0, 140.0), Modifiers::NONE);
    h.click(300.0, 100.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    let id = only_vector(&mut h);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!(net.points[1].handle_mode, HandleMode::MirrorAll);
    assert_eq!(net.segments[0].tangent_end, [-30.0, -40.0]);
    assert_eq!(net.segments[1].tangent_start, [30.0, 40.0]);
    // Alt посреди протяжки: входящая ручка остаётся как была.
    let mut h = H::new();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.down(200.0, 100.0, Modifiers::NONE);
    h.mv(210.0, 120.0, Modifiers::NONE);
    h.mv(240.0, 100.0, Modifiers::ALT);
    h.up(240.0, 100.0, Modifiers::ALT);
    h.click(300.0, 100.0, Modifiers::NONE);
    h.key("Enter", Modifiers::NONE);
    let id = only_vector(&mut h);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!(net.points[1].handle_mode, HandleMode::Free);
    assert_eq!(net.segments[0].tangent_end, [-10.0, -20.0]);
    assert_eq!(net.segments[1].tangent_start, [40.0, 0.0]);
}

#[test]
fn click_on_the_first_point_closes_the_path_into_a_region() {
    let mut h = H::new();
    let steps = h.ed.step_count();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 100.0, Modifiers::NONE);
    h.click(150.0, 180.0, Modifiers::NONE);
    // Попадание в точку — в радиусе 6 px.
    h.click(103.0, 102.0, Modifiers::NONE);
    let id = only_vector(&mut h);
    assert_eq!(
        h.ed.step_count(),
        steps + 1,
        "замыкание — конец пути, один шаг"
    );
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!((net.points.len(), net.segments.len()), (3, 3));
    assert_eq!(net.regions.len(), 1);
    assert_eq!(net.regions[0].loops, vec![vec![0, 1, 2]]);
    assert!(net.validate().is_ok());
}

#[test]
fn shift_snaps_to_45_degrees_and_esc_drops_a_lone_point() {
    let mut h = H::new();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 130.0, Modifiers::SHIFT);
    h.key("Enter", Modifiers::NONE);
    let id = only_vector(&mut h);
    let w = world_points(&mut h, id);
    // 16,7° — к горизонтали; длина — проекция на неё.
    assert!(near(w[1], Point::new(200.0, 100.0)), "{w:?}");
    // Одна точка без сегментов — не вектор: ни узла, ни шага.
    let mut h = H::new();
    let steps = h.ed.step_count();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    assert!(h.children(None).is_empty());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    h.assert_scene_clean();
}

#[test]
fn undo_during_a_path_removes_its_last_point() {
    let mut h = H::new();
    let steps = h.ed.step_count();
    h.key("KeyP", Modifiers::NONE);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 100.0, Modifiers::NONE);
    h.click(300.0, 100.0, Modifiers::NONE);
    h.key("KeyZ", ctrl());
    assert_eq!(h.ed.step_count(), steps);
    h.key("Enter", Modifiers::NONE);
    let id = only_vector(&mut h);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!((net.points.len(), net.segments.len()), (2, 1));
    assert_eq!(h.ed.step_count(), steps + 1);
}

#[test]
fn pen_honours_parent_rotation_and_flip() {
    let mut h = H::new();
    let f = h.frame(None, 50.0, 50.0, 600.0, 500.0);
    h.set(
        f,
        PropsPatch {
            rotation: Some(30.0),
            flip_x: Some(true),
            ..Default::default()
        },
    );
    let a = h.ed.scene().node(f).unwrap().world * Point::new(100.0, 100.0);
    let b = h.ed.scene().node(f).unwrap().world * Point::new(300.0, 200.0);
    h.ed.set_settings(Settings {
        pixel_snap: false,
        ..h.ed.settings().clone()
    });
    h.key("KeyP", Modifiers::NONE);
    h.click(a.x, a.y, Modifiers::NONE);
    h.click(b.x, b.y, Modifiers::NONE);
    h.key("Enter", Modifiers::NONE);
    let id = h.children(Some(f))[0];
    assert_eq!(h.parent(id), Some(f), "вектор — в фрейме под точкой");
    let w = world_points(&mut h, id);
    assert!(near(w[0], a) && near(w[1], b), "{w:?}");
}

#[test]
fn pen_in_edit_mode_joins_contours_and_adds_points_on_segments() {
    let mut h = H::new();
    // Два отдельных отрезка в одном векторе.
    let net = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(0.0, 100.0),
            VectorPoint::new(100.0, 100.0),
        ],
        segments: vec![VectorSegment::line(0, 1), VectorSegment::line(2, 3)],
        regions: vec![],
    };
    let id = h.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(net)),
            ..patch(100.0, 100.0, 100.0, 100.0)
        },
    );
    assert!(h.ed.start_vector_edit(id));
    let steps = h.ed.step_count();
    h.key("KeyP", Modifiers::NONE);
    assert_eq!(h.ed.vector_tool(), Some(VectorTool::Pen));
    // От конца первого отрезка — к концу второго: сеть соединилась.
    h.click(200.0, 100.0, Modifiers::NONE);
    h.click(200.0, 200.0, Modifiers::NONE);
    assert_eq!(h.ed.step_count(), steps + 1);
    assert!(h.ed.is_editing_vector(), "перо в правке остаётся в ней");
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!(net.segments.len(), 3);
    assert_eq!(net.segments[2], VectorSegment::line(1, 3));
    // Щелчок пером по сегменту без пути — новая точка на нём.
    h.click(150.0, 100.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    let net = h.props(id).vector.as_deref().unwrap().clone();
    assert_eq!((net.points.len(), net.segments.len()), (5, 4));
    assert_eq!(net.points[4], VectorPoint::new(50.0, 0.0));
    assert_eq!(h.ed.step_count(), steps + 2);
}

#[test]
fn pen_on_a_rectangle_turns_it_into_a_vector_in_one_step() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    h.set(
        r,
        PropsPatch {
            name: Some("Кнопка".into()),
            ..Default::default()
        },
    );
    let before = h.state();
    let steps = h.ed.step_count();
    assert!(h.ed.start_vector_edit(r));
    h.key("KeyP", Modifiers::NONE);
    // Точка на верхней стороне.
    h.click(150.0, 100.0, Modifiers::NONE);
    h.key("Escape", Modifiers::NONE);
    let id = only_vector(&mut h);
    assert_ne!(id, r);
    let p = h.props(id);
    assert_eq!(p.name, "Кнопка");
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 200.0, 100.0));
    assert_eq!(p.fills, Props::defaults(NodeKind::Rect).fills);
    let net = p.vector.as_deref().unwrap();
    assert_eq!(net.points.len(), 5);
    assert_eq!(net.regions[0].loops[0].len(), 5);
    h.ed.end_vector_edit();
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}
````

В `crates/ferrum-render/tests/vector.rs` заменить:

````rust
    golden("stroke_joins", &img);
}
````

на:

````rust
    golden("stroke_joins", &img);
}

#[test]
fn vector_edit_overlay() {
    use ferrum_render::{Overlay, VectorEditOverlay};
    use kurbo::{Affine, Point};
    let scene = Page::new().scene();
    // Треугольник: сегмент 1 — кривая; выделена точка 1, её ручка видна и выделена.
    let mut net = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(50.0, 80.0),
        ],
        segments: vec![
            VectorSegment::line(0, 1),
            VectorSegment::line(1, 2),
            VectorSegment::line(2, 0),
        ],
        regions: vec![],
    };
    net.segments[1].tangent_start = [30.0, 30.0];
    let overlay = Overlay {
        frame_labels: false,
        pixel_grid: false,
        vector_edit: Some(VectorEditOverlay {
            transform: Affine::translate((20.0, 20.0)),
            net: Arc::new(net),
            points: vec![1],
            segments: vec![0],
            handles: vec![(1, true)],
            selected_handles: vec![],
            rubber: Some((Point::new(50.0, 80.0), Point::new(130.0, 90.0))),
        }),
        ..Default::default()
    };
    let img = render_overlay(&scene, &camera(180.0, 120.0), &overlay, WHITE);
    let select = [0x4a, 0xa3, 0xff];
    // Невыделенная точка (20, 20) — белая середина, выделенная (120, 20) — залита.
    assert_px(&img, 20, 20, [255, 255, 255], 30);
    assert_px(&img, 120, 20, select, 30);
    // Выделенный сегмент — 2 px по верхней стороне, середина его видна.
    assert_px(&img, 70, 20, select, 30);
    // Ручка точки 1: кружок в (150, 50) — белая середина, линия к нему.
    assert_px(&img, 150, 50, [255, 255, 255], 40);
    assert_px(&img, 135, 35, select, 90);
    // Резинка пера от (70, 100) к (150, 110).
    assert_px(&img, 110, 105, select, 90);
    // Внутри треугольника — фон.
    assert_px(&img, 70, 50, [255, 255, 255], 0);
    golden("vector_edit", &img);
}
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
        "без английского текста разбора"
    );
    core.command(Command::Bind {
        command: "tool.pen".into(),
        shortcut: "KeyP".into(),
        replace: true,
    });
````

на:

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

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        json!({"type": "demo", "nodes": -1}),
        json!({"type": "demo", "nodes": 1, "extra": 1}),
        json!({"type": "setTool", "tool": "pen"}),
        json!({"tool": "rect"}),
        json!("save"),
````

на:

````rust
        json!({"type": "demo", "nodes": -1}),
        json!({"type": "demo", "nodes": 1, "extra": 1}),
        json!({"type": "setTool", "tool": "scale"}),
        json!({"tool": "rect"}),
        json!("save"),
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor --test pen`
Expected: FAIL — не собирается: нет `Editor::start_vector_edit`, `VectorTool`, `ToolKind::Pen`

- [ ] **Step 3: Реализация**

В `apps/web/src/core/core.ts` заменить:

````ts
    case 'text':
      return 'text';
    case 'resize': {
      const a = ((angle % 180) + 180) % 180;
````

на:

````ts
    case 'text':
      return 'text';
    case 'pen':
      return 'crosshair';
    case 'bucket':
      return 'cell';
    case 'resize': {
      const a = ((angle % 180) + 180) % 180;
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::ToolHand => self.set_tool(ToolKind::Hand),
            C::ToolText => self.set_tool(ToolKind::Text),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
````

на:

````rust
            C::ToolHand => self.set_tool(ToolKind::Hand),
            C::ToolText => self.set_tool(ToolKind::Text),
            C::ToolPen => self.set_tool(ToolKind::Pen),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// «Текст» (подсистема 3).
    ToolText,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
````

на:

````rust
    /// «Текст» (подсистема 3).
    ToolText,
    /// «Перо» (подсистема 3, этап 3).
    ToolPen,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    cmd(C::ToolHand, "tool.hand", "Рука", &["KeyH"]),
    cmd(C::ToolText, "tool.text", "Текст", &["KeyT"]),
    cmd(
        C::HandHold,
````

на:

````rust
    cmd(C::ToolHand, "tool.hand", "Рука", &["KeyH"]),
    cmd(C::ToolText, "tool.text", "Текст", &["KeyT"]),
    cmd(C::ToolPen, "tool.pen", "Перо", &["KeyP"]),
    cmd(
        C::HandHold,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust

use ferrum_doc::{Changes, DocError, DocResult, Document, NodeId, PageDoc, PageId, Tx};
use ferrum_render::{Camera, Measure, Overlay, OverlayTheme, TextEditOverlay};
use ferrum_scene::Scene;
use ferrum_text::{Fonts, SharedFonts, TextLayout};
````

на:

````rust

use ferrum_doc::{Changes, DocError, DocResult, Document, NodeId, PageDoc, PageId, Tx};
use ferrum_render::{Camera, Measure, Overlay, OverlayTheme, TextEditOverlay, VectorEditOverlay};
use ferrum_scene::Scene;
use ferrum_text::{Fonts, SharedFonts, TextLayout};
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    Tool, ToolKind, ToolOverlay,
};

/// Раскладка правимого текста и её ключ: узел, версия узла, поколение шрифтов.
````

на:

````rust
    Tool, ToolKind, ToolOverlay,
};
use crate::vector_edit::{VectorEdit, VectorTool};

/// Раскладка правимого текста и её ключ: узел, версия узла, поколение шрифтов.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Правка текста: курсор (с миганием), выделение, превью IME.
    text: Option<TextEditOverlay>,
}

````

на:

````rust
    /// Правка текста: курсор (с миганием), выделение, превью IME.
    text: Option<TextEditOverlay>,
    /// Правка вектора: сеть, выделенные точки, ручки, резинка пера.
    vector: Option<VectorEditOverlay>,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Указатель над правимым текстом: курсор — текстовый.
    pub(crate) text_hover: bool,
}

````

на:

````rust
    /// Указатель над правимым текстом: курсор — текстовый.
    pub(crate) text_hover: bool,
    /// Правка вектора и перо (спека подсистемы 3, раздел 9).
    pub(crate) vector_edit: Option<VectorEdit>,
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                measures: Vec::new(),
                text: None,
            },
        };
````

на:

````rust
                measures: Vec::new(),
                text: None,
                vector: None,
            },
        };
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            now_ms: 0.0,
            text_hover: false,
        })
    }
````

на:

````rust
            now_ms: 0.0,
            text_hover: false,
            vector_edit: None,
        })
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    }

    pub fn tool_kind(&self) -> ToolKind {
        self.tool.kind()
    }

````

на:

````rust
    }

    /// Инструмент для интерфейса: перо в правке вектора — «Перо».
    pub fn tool_kind(&self) -> ToolKind {
        if self.vector_tool() == Some(VectorTool::Pen) {
            ToolKind::Pen
        } else {
            self.tool.kind()
        }
    }

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Идёт ли жест (кнопка нажата).
    pub fn is_busy(&self) -> bool {
        self.tool.is_busy() || self.pan.is_some()
    }

````

на:

````rust
    /// Идёт ли жест (кнопка нажата).
    pub fn is_busy(&self) -> bool {
        self.tool.is_busy()
            || self.pan.is_some()
            || self.vector_edit.as_ref().is_some_and(|v| v.drag.is_some())
    }

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                Cursor::Default
            }
        } else {
            self.tool.cursor()
````

на:

````rust
                Cursor::Default
            }
        } else if let Some(c) = self.vector_cursor() {
            c
        } else {
            self.tool.cursor()
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Описание оверлея для рендера.
    pub fn overlay(&self) -> Overlay {
        let t = self.tool.overlay();
        // Правка текста: вместо рамки с ручками — рамка текста и курсор.
        let text = self.text_overlay().map(|o| o.overlay);
        let editing = text.is_some();
        Overlay {
            theme: self.theme.clone(),
````

на:

````rust
    /// Описание оверлея для рендера.
    pub fn overlay(&self) -> Overlay {
        let mut t = self.tool.overlay();
        // Правка текста: вместо рамки с ручками — рамка текста и курсор.
        let text = self.text_overlay().map(|o| o.overlay);
        // Правка вектора: сеть с точками вместо рамки; рамка выделения точек и
        // привязки — её.
        let vector = self.vector_overlay();
        if let Some(v) = &self.vector_edit {
            t.snap_lines = v.snap_lines.clone();
        }
        let editing = text.is_some() || self.vector_edit.is_some();
        Overlay {
            theme: self.theme.clone(),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            measurements: self.measures.clone(),
            text_edit: text,
        }
    }
````

на:

````rust
            measurements: self.measures.clone(),
            text_edit: text,
            vector_edit: vector,
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
                // Правка вектора и перо — так же.
                if self.vector_edit.is_some() && self.vector_pointer_down(&e) {
                    return;
                }
                // Нажатие без отпускания предыдущего (событие потеряно) — прежний
                // жест отменяется, а не записывается.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                    self.start_text_edit(id, EnterAt::Point(world));
                }
            }
        }
````

на:

````rust
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
        if self.vector_edit.is_some() {
            self.vector_pointer_move(&e);
            return;
        }
        let (tool, mut cx) = self.split();
        tool.pointer_move(&mut cx, &e);
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        if self.text_edit.as_ref().is_some_and(|t| t.drag.is_some()) {
            self.text_pointer_up(&e);
            return;
        }
````

на:

````rust
        if self.text_edit.as_ref().is_some_and(|t| t.drag.is_some()) {
            self.text_pointer_up(&e);
            return;
        }
        if self.vector_edit.is_some() {
            self.vector_pointer_up(&e);
            return;
        }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            return;
        }
        if self.is_busy() {
            // Во время жеста клавиши только меняют модификаторы; Esc отменяет жест.
````

на:

````rust
            return;
        }
        // Правка вектора: свои клавиши (Esc, Enter, Delete, B), остальное — по
        // раскладке (её команды правка перехватывает в `run`).
        if self.vector_edit.is_some() && self.vector_key(&e) {
            return;
        }
        if self.is_busy() {
            // Во время жеста клавиши только меняют модификаторы; Esc отменяет жест.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    pub fn cancel_gesture(&mut self) {
        self.cancel_panel();
        if let Some(mut hand) = self.pan.take() {
            hand.cancel(&mut self.ctx());
````

на:

````rust
    pub fn cancel_gesture(&mut self) {
        self.cancel_panel();
        self.cancel_vector_drag();
        if let Some(mut hand) = self.pan.take() {
            hand.cancel(&mut self.ctx());
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.cancel_gesture();
        self.end_text_edit();
        self.held = None;
        self.hover = None;
````

на:

````rust
        self.cancel_gesture();
        self.end_text_edit();
        // Перо в правке — перо внутри неё; «Перемещение» в правке узла —
        // выделение точек. Остальные инструменты правку заканчивают.
        if kind == ToolKind::Pen {
            self.tool = Box::new(SelectTool::new());
            self.held = None;
            self.hover = None;
            self.start_pen();
            self.emit();
            return;
        }
        if kind == ToolKind::Move
            && self
                .vector_edit
                .as_ref()
                .is_some_and(|v| !v.from_pen_tool && v.id.is_some())
        {
            self.finish_pen_path();
            if let Some(v) = &mut self.vector_edit {
                v.tool = VectorTool::Select;
            }
            self.emit();
            return;
        }
        if let Some(id) = self.finish_vector_edit() {
            if self.scene.slot(id).is_some() {
                self.selection.set(vec![id]);
            }
        }
        self.held = None;
        self.hover = None;
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            ToolKind::Hand => Box::new(HandTool::new()),
            ToolKind::Text => Box::new(TextTool::new()),
            // Изображению нужны хэш и размер: интерфейс сначала выбирает файл.
            ToolKind::Image => {
````

на:

````rust
            ToolKind::Hand => Box::new(HandTool::new()),
            ToolKind::Text => Box::new(TextTool::new()),
            ToolKind::Pen => Box::new(SelectTool::new()),
            // Изображению нужны хэш и размер: интерфейс сначала выбирает файл.
            ToolKind::Image => {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.cancel_gesture();
        let sel = self.alive(ids);
        // Другое выделение из панели слоёв заканчивает правку текста.
        if self.text_edit.as_ref().is_some_and(|t| sel != [t.id]) {
            self.end_text_edit();
        }
        self.selection.set(sel);
````

на:

````rust
        self.cancel_gesture();
        let sel = self.alive(ids);
        // Другое выделение из панели слоёв заканчивает правку текста и вектора.
        if self.text_edit.as_ref().is_some_and(|t| sel != [t.id]) {
            self.end_text_edit();
        }
        if self.vector_edit.is_some() && self.vector_edit_node().is_none_or(|id| sel != [id]) {
            self.finish_vector_edit();
        }
        self.selection.set(sel);
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.cancel_gesture();
        self.finish_text_edit();
        // Изменения всех страниц уже в сценах или не нужны: новая сцена строится
        // по документу целиком.
````

на:

````rust
        self.cancel_gesture();
        self.finish_text_edit();
        self.finish_vector_edit();
        // Изменения всех страниц уже в сценах или не нужны: новая сцена строится
        // по документу целиком.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust

    pub(crate) fn undo_redo(&mut self, redo: bool) {
        // Жест держит превью и черновики на узлах, которые отмена может убрать.
        self.cancel_gesture();
````

на:

````rust

    pub(crate) fn undo_redo(&mut self, redo: bool) {
        // Путь пера: отмена убирает его последнюю точку, а не шаг истории.
        if !redo && self.pen_undo_point() {
            return;
        }
        // Незаписанный путь пера записывается раньше отмены: иначе он остался
        // бы поверх отменённого документа.
        self.finish_pen_path();
        // Жест держит превью и черновики на узлах, которые отмена может убрать.
        self.cancel_gesture();
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        // за концом текста (его укоротили) — к концу.
        self.clamp_text_edit();
        self.update_measures();
        let now = Seen {
            selection: self.selection.version(),
            tool: self.tool.kind(),
            cursor: self.cursor(),
            frame: FrameKey {
````

на:

````rust
        // за концом текста (его укоротили) — к концу.
        self.clamp_text_edit();
        // Правимый вектор изменился (запись, отмена, панель) — сеть заново.
        self.resync_vector_edit();
        self.update_measures();
        let now = Seen {
            selection: self.selection.version(),
            tool: self.tool_kind(),
            cursor: self.cursor(),
            frame: FrameKey {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                measures: self.measures.clone(),
                text: self.text_overlay().map(|o| o.overlay),
            },
        };
````

на:

````rust
                measures: self.measures.clone(),
                text: self.text_overlay().map(|o| o.overlay),
                vector: self.vector_overlay(),
            },
        };
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
mod text_style;
pub mod tools;
pub mod vector_ops;

````

на:

````rust
mod text_style;
pub mod tools;
pub mod vector_edit;
pub mod vector_ops;

````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub use text_style::{Mixed, TextPropsPatch, TextStylePatch, TextStyleSummary};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

на:

````rust
pub use text_style::{Mixed, TextPropsPatch, TextStylePatch, TextStyleSummary};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
pub use vector_edit::{VectorSelection, VectorTool};
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    /// «Текст» (T): новый текст щелчком или рамкой.
    Text,
}

````

на:

````rust
    /// «Текст» (T): новый текст щелчком или рамкой.
    Text,
    /// «Перо» (P): векторные пути (сеанс правки вектора, `vector_edit`).
    Pen,
}

````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    /// Над текстом в правке и у инструмента «Текст» (I-образный).
    Text,
}

````

на:

````rust
    /// Над текстом в правке и у инструмента «Текст» (I-образный).
    Text,
    /// Перо (в правке вектора и у инструмента «Перо»).
    Pen,
    /// «Заливка области» в правке вектора.
    Bucket,
}

````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    /// Шагов истории, созданных редактором (транзакции с изменениями).
    pub steps: u64,
    next_draft: u32,
    /// Изменения, применённые к сцене текущей страницы с прошлого
    /// `Editor::take_applied` (кэши потребителя, например имён слоёв, обновляются
````

на:

````rust
    /// Шагов истории, созданных редактором (транзакции с изменениями).
    pub steps: u64,
    pub(crate) next_draft: u32,
    /// Изменения, применённые к сцене текущей страницы с прошлого
    /// `Editor::take_applied` (кэши потребителя, например имён слоёв, обновляются
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    /// Инструмент «Текст» закончил: новый текст (родитель и свойства) в правку.
    pub new_text: Option<(Option<NodeId>, Props)>,
    /// Действие правки текста (клавиша удаления, «Вырезать», вставка фрагмента)
    /// не применено: шрифты ещё грузятся. Ядро повторит то же действие, когда
````

на:

````rust
    /// Инструмент «Текст» закончил: новый текст (родитель и свойства) в правку.
    pub new_text: Option<(Option<NodeId>, Props)>,
    /// Двойной щелчок по вектору или фигуре: войти в правку вектора.
    pub edit_vector: Option<NodeId>,
    /// Действие правки текста (клавиша удаления, «Вырезать», вставка фрагмента)
    /// не применено: шрифты ещё грузятся. Ядро повторит то же действие, когда
````

Создать `crates/ferrum-editor/src/vector_edit.rs`:

````rust
//! Правка векторной сети на холсте (спека подсистемы 3, раздел 9): сеанс правки
//! узла (вектора или фигуры, которая станет вектором при первой правке) и перо.
//!
//! Сеанс держит сеть в координатах формы узла (`work`) и матрицу формы → мир
//! (`world`). Пока идёт жест или путь пера, сеть живёт только в превью сцены
//! (у нового вектора — черновиком); запись — одна транзакция: рамка узла и
//! положение пересчитываются по границам сети (`vector_ops::reframe`), поэтому
//! на экране ничего не сдвигается. После записи сеанс перечитывает узел:
//! номера точек при этом не меняются.
//!
//! Перо (P): щелчок — точка, протяжка — ручки (зеркально; Alt — разорвать),
//! щелчок по первой точке пути — замкнуть (петля становится областью), по
//! другой точке той же сети — соединить, Shift — шаг 45°; Esc и Enter —
//! закончить путь. Один путь — один шаг отмены; Ctrl/Cmd+Z во время пути
//! убирает последнюю точку.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use ferrum_doc::{
    DocResult, FillRule, HandleMode, NodeId, NodeKind, Props, PropsPatch, Tx, VectorNetwork,
    VectorPoint, VectorRegion, VectorSegment,
};
use ferrum_render::VectorEditOverlay;
use ferrum_scene::shape::local_transform;
use ferrum_scene::Draft;
use kurbo::{Affine, Line, Point, Vec2};

use crate::editor::{Editor, EditorEvent};
use crate::geometry::{decompose_with, snap_angle};
use crate::input::{KeyEvent, Modifiers, PointerEvent};
use crate::selection::{drop_frame_at, locked};
use crate::tools::{sync, Cursor, ToolKind, DRAG_THRESHOLD_PX};
use crate::vector_ops::{reframe, shape_network, HandleEnd};

/// Радиус попадания в точку и ручку, экранные пиксели.
pub const POINT_HIT_PX: f64 = 6.0;
/// Допуск попадания в сегмент, экранные пиксели.
pub const SEGMENT_HIT_PX: f64 = 4.0;

/// Инструмент внутри правки вектора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorTool {
    /// Выделение и перемещение точек, ручек и сегментов.
    Select,
    /// Перо: новые точки и пути.
    Pen,
    /// «Заливка области» (B): щелчок по замкнутой области.
    Bucket,
}

/// Выделенное в сети.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VectorSelection {
    pub points: BTreeSet<u32>,
    pub segments: BTreeSet<u32>,
    /// Ручки: сегмент и его конец.
    pub handles: BTreeSet<(u32, HandleEnd)>,
}

impl VectorSelection {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty() && self.segments.is_empty() && self.handles.is_empty()
    }

    /// Точки, которые двигает перетаскивание: выделенные и концы выделенных сегментов.
    pub fn moving_points(&self, net: &VectorNetwork) -> Vec<u32> {
        let mut out: BTreeSet<u32> = self.points.clone();
        for &s in &self.segments {
            if let Some(seg) = net.segments.get(s as usize) {
                out.insert(seg.start);
                out.insert(seg.end);
            }
        }
        out.into_iter().collect()
    }

    /// Убирает номера за пределами сети (после отмены или чужой правки).
    fn clamp(&mut self, net: &VectorNetwork) {
        let (p, s) = (net.points.len() as u32, net.segments.len() as u32);
        self.points.retain(|&i| i < p);
        self.segments.retain(|&i| i < s);
        self.handles.retain(|&(i, _)| i < s);
    }
}

/// Новый вектор пера, которого ещё нет в документе.
#[derive(Debug, Clone)]
pub(crate) struct NewVector {
    pub parent: Option<NodeId>,
}

/// Путь пера, который сейчас рисуется.
#[derive(Debug, Clone, Default)]
pub(crate) struct PenPath {
    /// Последняя точка пути; `None` — следующий щелчок начинает новый путь.
    pub last: Option<u32>,
    /// Первая точка пути: щелчок по ней замыкает путь.
    pub first: Option<u32>,
    /// Сегменты пути по порядку — петля области при замыкании.
    pub chain: Vec<u32>,
    /// Ручка, вытянутая из последней точки протяжкой: начальная ручка
    /// следующего сегмента.
    pub pending_out: Option<Vec2>,
    /// В пути есть незаписанные точки.
    pub dirty: bool,
}

/// Перетаскивание в правке.
#[derive(Debug, Clone)]
pub(crate) enum Drag {
    /// Протяжка пера: ручки только что поставленной точки.
    PenHandle {
        point: u32,
        /// Сегмент, который пришёл в точку (его конечная ручка — зеркальная).
        incoming: Option<usize>,
        origin: VectorNetwork,
        start_screen: Point,
        /// Alt был нажат: зеркальность разорвана, входящая ручка не меняется.
        broken: bool,
    },
}

/// Сеанс правки вектора.
#[derive(Debug, Clone)]
pub(crate) struct VectorEdit {
    /// Узел; `None` — новый вектор пера ещё не в документе (`new`).
    pub id: Option<NodeId>,
    pub new: Option<NewVector>,
    /// Черновик сцены: новый вектор пера или сеть фигуры, которая ещё не стала
    /// вектором (сама фигура на время превью скрыта).
    pub draft: NodeId,
    /// Свойства узла на момент последней записи (у нового — начальные): по ним
    /// `reframe` пересчитывает рамку.
    pub base: Props,
    /// Матрица формы узла → мир.
    pub world: Affine,
    /// Сеть в координатах формы с незаписанными правками жеста или пути.
    pub work: VectorNetwork,
    /// Версия узла в сцене, с которой сеанс сверился.
    pub version: u64,
    pub sel: VectorSelection,
    pub tool: VectorTool,
    /// Сеанс начат инструментом «Перо»: конец пути — конец правки.
    pub from_pen_tool: bool,
    pub pen: PenPath,
    pub drag: Option<Drag>,
    /// Последняя точка указателя в мире (резинка пера).
    pub pointer: Option<Point>,
    /// Модификаторы последнего события (Shift — шаг 45° резинки).
    pub mods: Modifiers,
    pub snap_lines: Vec<Line>,
}

impl VectorEdit {
    fn to_shape(&self, world: Point) -> Point {
        self.world.inverse() * world
    }

    fn to_world(&self, p: Point) -> Point {
        self.world * p
    }
}

fn point_of(net: &VectorNetwork, i: u32) -> Point {
    let p = &net.points[i as usize];
    Point::new(p.x, p.y)
}

/// Точка `p` с шагом 45° от `from` (Shift).
fn snap45(from: Point, p: Point) -> Point {
    let v = p - from;
    let ang = snap_angle(v.atan2().to_degrees(), 45.0).to_radians();
    let len = v.hypot();
    from + Vec2::new(ang.cos(), ang.sin()) * len * (v.atan2() - ang).cos()
}

impl Editor {
    /// Идёт ли правка вектора.
    pub fn is_editing_vector(&self) -> bool {
        self.vector_edit.is_some()
    }

    /// Инструмент правки вектора (`None` — правки нет).
    pub fn vector_tool(&self) -> Option<VectorTool> {
        self.vector_edit.as_ref().map(|v| v.tool)
    }

    /// Выделенное в правке вектора.
    pub fn vector_selection(&self) -> Option<&VectorSelection> {
        self.vector_edit.as_ref().map(|v| &v.sel)
    }

    /// Узел правки (у нового пути пера — `None`, пока он не записан).
    pub fn vector_edit_node(&self) -> Option<NodeId> {
        self.vector_edit.as_ref().and_then(|v| v.id)
    }

    /// Сеть правки в координатах формы — с незаписанным жестом и путём пера.
    pub fn vector_work(&self) -> Option<&VectorNetwork> {
        self.vector_edit.as_ref().map(|v| &v.work)
    }

    /// Входит в правку вектора или фигуры (прямоугольник, эллипс, линия),
    /// которая станет вектором при первой правке. `false` — не тот узел,
    /// заблокирован или документ только для чтения.
    pub fn start_vector_edit(&mut self, id: NodeId) -> bool {
        self.cancel_gesture();
        self.end_text_edit();
        self.finish_vector_edit();
        if self.doc.is_read_only() {
            return false;
        }
        let Some(slot) = self.scene.slot(id) else {
            return false;
        };
        let n = self.scene.at(slot);
        if self.scene.is_draft(id)
            || locked(&self.scene, slot)
            || !matches!(
                n.props.kind,
                NodeKind::Vector | NodeKind::Rect | NodeKind::Ellipse | NodeKind::Line
            )
        {
            return false;
        }
        let Some(net) = shape_network(&n.props) else {
            return false;
        };
        let (props, world, version) = (n.props.clone(), n.world, n.version);
        let draft = crate::tools::draft_id(&mut self.out.next_draft);
        self.selection.set(vec![id]);
        self.hover = None;
        self.vector_edit = Some(VectorEdit {
            id: Some(id),
            new: None,
            draft,
            base: props,
            world,
            work: net,
            version,
            sel: VectorSelection::default(),
            tool: VectorTool::Select,
            from_pen_tool: false,
            pen: PenPath::default(),
            drag: None,
            pointer: None,
            mods: Modifiers::NONE,
            snap_lines: Vec::new(),
        });
        self.emit();
        true
    }

    /// Инструмент «Перо» вне правки: сеанс без узла, первый щелчок начнёт новый
    /// вектор. В правке — перо внутри неё.
    pub(crate) fn start_pen(&mut self) {
        if let Some(v) = &mut self.vector_edit {
            v.tool = VectorTool::Pen;
            v.drag = None;
            return;
        }
        if self.doc.is_read_only() {
            return;
        }
        let draft = crate::tools::draft_id(&mut self.out.next_draft);
        self.vector_edit = Some(VectorEdit {
            id: None,
            new: None,
            draft,
            base: Props::defaults(NodeKind::Vector),
            world: Affine::IDENTITY,
            work: VectorNetwork::default(),
            version: 0,
            sel: VectorSelection::default(),
            tool: VectorTool::Pen,
            from_pen_tool: true,
            pen: PenPath::default(),
            drag: None,
            pointer: None,
            mods: Modifiers::NONE,
            snap_lines: Vec::new(),
        });
    }

    /// Выход из правки (Esc, Enter, другой инструмент): незаконченный путь пера
    /// записывается, узел остаётся выделенным.
    pub fn end_vector_edit(&mut self) {
        let Some(id) = self.finish_vector_edit() else {
            self.emit();
            return;
        };
        if self.scene.slot(id).is_some() {
            self.selection.set(vec![id]);
        }
        self.emit();
    }

    /// Заканчивает сеанс: путь пера записывается, жест отменяется. Возвращает
    /// узел правки (после записи — и новый вектор).
    pub(crate) fn finish_vector_edit(&mut self) -> Option<NodeId> {
        self.vector_edit.as_ref()?;
        if let Some(v) = &mut self.vector_edit {
            v.drag = None;
        }
        self.finish_pen_path();
        let v = self.vector_edit.take()?;
        self.scene.clear_preview();
        v.id
    }

    /// Матрица формы и сеть узла правки — заново из сцены (после записи,
    /// отмены, правки из панели). Узел пропал или стал не тем — правка кончается.
    pub(crate) fn resync_vector_edit(&mut self) {
        let Some(v) = &self.vector_edit else {
            return;
        };
        if v.drag.is_some() || v.pen.dirty {
            return;
        }
        let Some(id) = v.id else {
            return;
        };
        let Some(n) = self.scene.node(id) else {
            self.vector_edit = None;
            return;
        };
        if n.version == v.version && n.world == v.world {
            return;
        }
        let Some(net) = shape_network(&n.props) else {
            self.vector_edit = None;
            return;
        };
        let (props, world, version) = (n.props.clone(), n.world, n.version);
        let v = self.vector_edit.as_mut().expect("проверено выше");
        v.base = props;
        v.world = world;
        v.version = version;
        v.work = net;
        v.sel.clamp(&v.work);
        if v.pen
            .last
            .is_some_and(|l| l as usize >= v.work.points.len())
        {
            v.pen = PenPath::default();
        }
    }

    /// Показывает рабочую сеть: превью узла или черновик нового вектора.
    pub(crate) fn show_vector_work(&mut self) {
        let Some(v) = &self.vector_edit else {
            return;
        };
        let patch = reframe(&v.base, v.work.clone());
        match (&v.id, &v.new) {
            (Some(id), _) if v.base.kind == NodeKind::Vector => {
                self.scene.set_preview(&[(*id, patch)]);
            }
            (Some(id), _) => {
                // Фигура ещё не вектор: превью — её сеть черновиком поверх, а
                // сама фигура на время превью скрыта.
                let mut props = v.base.clone();
                as_vector(&mut props);
                patch.apply_to(&mut props);
                let draft = v.draft;
                let parent = self
                    .scene
                    .node(*id)
                    .and_then(|n| n.parent)
                    .map(|s| self.scene.at(s).id);
                self.scene.set_preview(&[(
                    *id,
                    PropsPatch {
                        visible: Some(false),
                        ..Default::default()
                    },
                )]);
                self.scene.set_drafts(&[Draft {
                    id: draft,
                    parent,
                    props,
                }]);
            }
            (None, Some(n)) => {
                let mut props = v.base.clone();
                patch.apply_to(&mut props);
                let draft = Draft {
                    id: v.draft,
                    parent: n.parent,
                    props,
                };
                self.scene.set_drafts(&[draft]);
            }
            (None, None) => {}
        }
    }

    /// Записывает рабочую сеть одной транзакцией. Фигура при этом становится
    /// вектором (новый узел на её месте, та же рамка, имя, заливки и обводки),
    /// новый вектор пера — создаётся. `false` — отказ документа (сообщение ушло).
    pub(crate) fn commit_vector_work(&mut self) -> bool {
        let Some(v) = &self.vector_edit else {
            return false;
        };
        let patch = reframe(&v.base, v.work.clone());
        let (id, new, base) = (v.id, v.new.clone(), v.base.clone());
        self.scene.clear_preview();
        let page = self.page;
        let result = self.doc.transact(
            page,
            self.selection.to_vec(),
            |tx| -> DocResult<Vec<NodeId>> {
                match (id, &new) {
                    (Some(id), _) if base.kind == NodeKind::Vector => {
                        tx.set_props(id, &patch)?;
                        Ok(vec![id])
                    }
                    (Some(id), _) => Ok(vec![replace_with_vector(tx, id, &base, &patch)?]),
                    (None, Some(n)) => {
                        let mut props = base.clone();
                        patch.apply_to(&mut props);
                        Ok(vec![tx.create_with_props(n.parent, None, &props)?])
                    }
                    (None, None) => Ok(vec![]),
                }
            },
        );
        let changed = sync(&mut self.doc, page, &mut self.scene, &mut self.out);
        match result {
            Ok(sel) => {
                if changed {
                    self.out.steps += 1;
                }
                let node = sel.first().copied();
                self.selection.set(sel);
                if let Some(v) = &mut self.vector_edit {
                    v.id = node.or(v.id);
                    v.new = None;
                    v.pen.dirty = false;
                    // Сверка с новой рамкой — заново.
                    v.version = u64::MAX;
                }
                self.resync_vector_edit();
                true
            }
            Err(e) => {
                self.out.events.push(EditorEvent::Error(e.to_string()));
                // Рабочая сеть — снова как в документе.
                if let Some(v) = &mut self.vector_edit {
                    v.pen = PenPath::default();
                    v.version = u64::MAX;
                    if v.id.is_none() {
                        v.work = VectorNetwork::default();
                        v.new = None;
                    }
                }
                self.resync_vector_edit();
                false
            }
        }
    }

    /// Точка сети под экранной точкой (верхняя — ближайшая).
    pub(crate) fn vector_point_at(&self, screen: Point) -> Option<u32> {
        let v = self.vector_edit.as_ref()?;
        let to_screen = self.camera.world_to_screen_transform() * v.world;
        let mut best: Option<(u32, f64)> = None;
        for (i, p) in v.work.points.iter().enumerate() {
            let d = (to_screen * Point::new(p.x, p.y) - screen).hypot();
            if d <= POINT_HIT_PX && best.is_none_or(|b| d < b.1) {
                best = Some((i as u32, d));
            }
        }
        best.map(|b| b.0)
    }

    /// Сегмент под экранной точкой и параметр на нём.
    pub(crate) fn vector_segment_at(&self, screen: Point) -> Option<(usize, f64)> {
        let v = self.vector_edit.as_ref()?;
        let world = self.camera.screen_to_world(screen);
        let local = v.to_shape(world);
        // Допуск — в координатах формы (масштаба у узлов нет, только камера).
        let tol = SEGMENT_HIT_PX / self.camera.zoom();
        ferrum_scene::vector::nearest_segment(&v.work, local, tol).map(|(s, t, _)| (s, t))
    }

    /// Нажатие пера.
    pub(crate) fn pen_pointer_down(&mut self, e: &PointerEvent) {
        let world_raw = self.camera.screen_to_world(e.point);
        let hit_point = self.vector_point_at(e.point);
        let hit_segment = self.vector_segment_at(e.point);
        // Новый вектор: родитель — самый глубокий фрейм под точкой, поза — как
        // у фигур (поворот и отражение родителя компенсируются).
        if self
            .vector_edit
            .as_ref()
            .is_some_and(|v| v.id.is_none() && v.new.is_none())
        {
            let mut world = world_raw;
            if self.settings.pixel_snap {
                world = Point::new(world.x.round(), world.y.round());
            }
            let tol = crate::tools::HIT_TOLERANCE_PX / self.camera.zoom();
            let parent = drop_frame_at(&self.scene, world, tol, &HashSet::new());
            let parent_world = parent
                .and_then(|p| self.scene.node(p))
                .map_or(Affine::IDENTITY, |n| n.world);
            let pose = decompose_with(
                parent_world.inverse() * Affine::translate(world.to_vec2()),
                0.0,
                0.0,
                0.0,
            );
            let mut base = Props::defaults(NodeKind::Vector);
            base.x = pose.x;
            base.y = pose.y;
            base.rotation = pose.rotation;
            (base.flip_x, base.flip_y) = (pose.flip_x, pose.flip_y);
            let v = self.vector_edit.as_mut().expect("сеанс пера");
            v.world = parent_world * local_transform(&base);
            v.base = base;
            v.new = Some(NewVector { parent });
        }
        let pixel_snap = self.settings.pixel_snap;
        let v = self.vector_edit.as_mut().expect("сеанс пера");
        v.mods = e.modifiers;
        let mut net = v.work.clone();
        let from_last = v.pen.last;
        // Щелчок по точке сети: замкнуть, соединить или продолжить от неё.
        if let Some(p) = hit_point {
            match from_last {
                Some(last) if last == p => {}
                Some(last) => {
                    let seg = net.segments.len() as u32;
                    let mut s = VectorSegment::line(last, p);
                    if let Some(t) = v.pen.pending_out.take() {
                        s.tangent_start = [t.x, t.y];
                    }
                    net.segments.push(s);
                    v.pen.chain.push(seg);
                    let closes = v.pen.first == Some(p);
                    if closes && v.pen.chain.len() >= 2 {
                        net.regions.push(VectorRegion {
                            rule: FillRule::NonZero,
                            loops: vec![v.pen.chain.clone()],
                            fills: None,
                        });
                    }
                    v.work = net;
                    v.pen.dirty = true;
                    v.sel = VectorSelection {
                        points: BTreeSet::from([p]),
                        ..Default::default()
                    };
                    // Протяжка с замыкающего щелчка тянет его входящую ручку.
                    v.drag = Some(Drag::PenHandle {
                        point: p,
                        incoming: Some(seg as usize),
                        origin: v.work.clone(),
                        start_screen: e.point,
                        broken: e.modifiers.alt,
                    });
                    v.pen.last = None;
                    v.pen.first = None;
                    v.pen.chain.clear();
                    self.show_vector_work();
                    return;
                }
                None => {
                    // Продолжить путь от существующей точки.
                    v.pen.last = Some(p);
                    v.pen.first = Some(p);
                    v.pen.chain.clear();
                    v.sel = VectorSelection {
                        points: BTreeSet::from([p]),
                        ..Default::default()
                    };
                    return;
                }
            }
            return;
        }
        // Щелчок пером по сегменту без пути — новая точка на нём.
        if let (None, Some((seg, t))) = (from_last, hit_segment) {
            let (n2, p) = crate::vector_ops::add_point(&net, seg, t);
            v.work = n2;
            v.pen.dirty = true;
            v.sel = VectorSelection {
                points: BTreeSet::from([p]),
                ..Default::default()
            };
            self.show_vector_work();
            return;
        }
        let mut world = world_raw;
        if let (Some(l), true) = (from_last, e.modifiers.shift) {
            world = snap45(v.to_world(point_of(&net, l)), world);
        }
        if pixel_snap {
            world = Point::new(world.x.round(), world.y.round());
        }
        let local = v.to_shape(world);
        let p = net.points.len() as u32;
        net.points.push(VectorPoint::new(local.x, local.y));
        let mut incoming = None;
        if let Some(last) = from_last {
            let mut s = VectorSegment::line(last, p);
            if let Some(t) = v.pen.pending_out.take() {
                s.tangent_start = [t.x, t.y];
            }
            incoming = Some(net.segments.len());
            v.pen.chain.push(net.segments.len() as u32);
            net.segments.push(s);
        } else {
            v.pen.first = Some(p);
            v.pen.chain.clear();
        }
        v.pen.last = Some(p);
        v.pen.pending_out = None;
        v.pen.dirty = true;
        v.work = net;
        v.sel = VectorSelection {
            points: BTreeSet::from([p]),
            ..Default::default()
        };
        v.drag = Some(Drag::PenHandle {
            point: p,
            incoming,
            origin: v.work.clone(),
            start_screen: e.point,
            broken: e.modifiers.alt,
        });
        self.show_vector_work();
    }

    /// Протяжка пера: ручки точки — к курсору (исходящая) и зеркально
    /// (входящая); Alt разрывает зеркальность.
    pub(crate) fn pen_pointer_move(&mut self, e: &PointerEvent) {
        let world = self.camera.screen_to_world(e.point);
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        v.pointer = Some(world);
        v.mods = e.modifiers;
        let Some(Drag::PenHandle {
            point,
            incoming,
            origin,
            start_screen,
            broken,
        }) = v.drag.clone()
        else {
            return;
        };
        if (e.point - start_screen).hypot() < DRAG_THRESHOLD_PX {
            return;
        }
        let broken = broken || e.modifiers.alt;
        let anchor = point_of(&origin, point);
        let d = v.to_shape(world) - anchor;
        let mut net = origin.clone();
        net.points[point as usize].handle_mode = if broken {
            HandleMode::Free
        } else {
            HandleMode::MirrorAll
        };
        if let Some(seg) = incoming {
            if broken {
                // После Alt входящая ручка остаётся той, что была на момент Alt.
                net.segments[seg] = v.work.segments[seg];
            } else {
                let s = &mut net.segments[seg];
                if s.end == point {
                    s.tangent_end = [-d.x, -d.y];
                } else {
                    s.tangent_start = [-d.x, -d.y];
                }
            }
        }
        // Замыкающий щелчок не продолжает путь: исходящей ручки нет.
        if v.pen.last == Some(point) {
            v.pen.pending_out = Some(d);
        }
        v.work = net;
        v.drag = Some(Drag::PenHandle {
            point,
            incoming,
            origin,
            start_screen,
            broken,
        });
        self.show_vector_work();
    }

    /// Отпускание пера: протяжка ручек кончается, путь продолжается.
    pub(crate) fn pen_pointer_up(&mut self) {
        let closed = self.vector_edit.as_ref().is_some_and(|v| {
            v.pen.last.is_none() && v.pen.dirty && matches!(v.drag, Some(Drag::PenHandle { .. }))
        });
        if let Some(v) = self.vector_edit.as_mut() {
            v.drag = None;
        }
        // Замкнули или соединили — путь закончен.
        if closed {
            self.finish_pen_path();
            if self.vector_edit.as_ref().is_some_and(|v| v.from_pen_tool) {
                self.exit_pen_tool();
            }
        }
    }

    /// Записывает незаписанный путь пера одним шагом. Путь из одной точки без
    /// сегментов не записывается (у нового вектора — исчезает).
    pub(crate) fn finish_pen_path(&mut self) {
        let Some(v) = &mut self.vector_edit else {
            return;
        };
        v.pen.last = None;
        v.pen.first = None;
        v.pen.chain.clear();
        v.pen.pending_out = None;
        if !v.pen.dirty {
            return;
        }
        if v.id.is_none() && v.work.segments.is_empty() {
            v.pen.dirty = false;
            v.work = VectorNetwork::default();
            v.new = None;
            self.scene.clear_preview();
            return;
        }
        self.commit_vector_work();
    }

    /// Перо вне правки закончило путь: «Перемещение», новый вектор выделен.
    pub(crate) fn exit_pen_tool(&mut self) {
        let id = self.finish_vector_edit();
        self.set_tool(ToolKind::Move);
        if let Some(id) = id.filter(|id| self.scene.slot(*id).is_some()) {
            self.selection.set(vec![id]);
        }
    }

    /// Ctrl/Cmd+Z во время пути пера: убрать последнюю точку пути. `false` —
    /// путь пуст, отмена — обычная.
    pub(crate) fn pen_undo_point(&mut self) -> bool {
        let Some(v) = &mut self.vector_edit else {
            return false;
        };
        let Some(last) = v.pen.last.filter(|_| v.pen.dirty) else {
            return false;
        };
        // Новая точка пути — последняя в сети, её сегмент — последний.
        if last as usize + 1 != v.work.points.len() {
            return false;
        }
        let seg = v.pen.chain.pop();
        // Предыдущая точка пути — другой конец убранного сегмента.
        let prev = seg.map(|s| {
            let s = v.work.segments[s as usize];
            if s.end == last {
                s.start
            } else {
                s.end
            }
        });
        if let Some(s) = seg {
            v.work.segments.truncate(s as usize);
        }
        v.work.points.pop();
        v.pen.last = prev;
        v.pen.pending_out = None;
        v.sel = VectorSelection {
            points: prev.into_iter().collect(),
            ..Default::default()
        };
        if prev.is_none() {
            v.pen.first = None;
        }
        if v.id.is_none() && v.work.points.is_empty() {
            v.pen = PenPath::default();
            v.new = None;
            self.scene.clear_preview();
            return true;
        }
        self.show_vector_work();
        true
    }

    /// Нажатие в правке. `false` — нажатие мимо: правка кончилась, его
    /// обработает инструмент.
    pub(crate) fn vector_pointer_down(&mut self, e: &PointerEvent) -> bool {
        match self.vector_edit.as_ref().map(|v| v.tool) {
            Some(VectorTool::Pen) => {
                self.pen_pointer_down(e);
                true
            }
            _ => {
                self.end_vector_edit();
                false
            }
        }
    }

    /// Движение указателя в правке (с кнопкой и без).
    pub(crate) fn vector_pointer_move(&mut self, e: &PointerEvent) {
        if self.vector_tool() == Some(VectorTool::Pen) {
            self.pen_pointer_move(e);
        }
    }

    /// Отпускание кнопки в правке.
    pub(crate) fn vector_pointer_up(&mut self, _e: &PointerEvent) {
        if self.vector_tool() == Some(VectorTool::Pen) {
            self.pen_pointer_up();
        }
    }

    /// Клавиша в правке. `false` — не её: дальше по раскладке.
    pub(crate) fn vector_key(&mut self, e: &KeyEvent) -> bool {
        let plain = e.modifiers == Modifiers::NONE || e.modifiers == Modifiers::SHIFT;
        match e.code.as_str() {
            "Escape" | "Enter" | "NumpadEnter" if plain && !e.repeat => {
                self.vector_escape();
                true
            }
            _ => false,
        }
    }

    /// Esc и Enter в правке: путь пера закончить (перо из инструмента — выйти
    /// к «Перемещению»), иначе — выйти из правки.
    pub(crate) fn vector_escape(&mut self) {
        let Some(v) = &self.vector_edit else {
            return;
        };
        if v.from_pen_tool {
            self.finish_pen_path();
            self.exit_pen_tool();
            return;
        }
        if v.tool == VectorTool::Pen && (v.pen.last.is_some() || v.pen.dirty) {
            self.finish_pen_path();
            if let Some(v) = &mut self.vector_edit {
                v.tool = VectorTool::Select;
            }
            return;
        }
        self.end_vector_edit();
    }

    /// Отменяет жест правки (Esc при нажатой кнопке, потеря фокуса): рабочая
    /// сеть — снова как до жеста. Протяжка пера оставляет поставленную точку.
    pub(crate) fn cancel_vector_drag(&mut self) {
        let Some(v) = &mut self.vector_edit else {
            return;
        };
        v.drag = None;
        v.snap_lines.clear();
    }

    /// Курсор над холстом в правке.
    pub(crate) fn vector_cursor(&self) -> Option<Cursor> {
        let v = self.vector_edit.as_ref()?;
        Some(match v.tool {
            VectorTool::Pen => Cursor::Pen,
            VectorTool::Bucket => Cursor::Bucket,
            VectorTool::Select => Cursor::Default,
        })
    }

    /// Оверлей правки: сеть, точки, ручки выделенного, резинка пера.
    pub(crate) fn vector_overlay(&self) -> Option<VectorEditOverlay> {
        let v = self.vector_edit.as_ref()?;
        if v.work.points.is_empty() {
            return None;
        }
        // Ручки — у выделенных точек, концов выделенных сегментов и соседей
        // выделенных точек (как в Figma).
        let mut handles = BTreeSet::new();
        let around: BTreeSet<u32> = v.sel.moving_points(&v.work).into_iter().collect();
        for (i, s) in v.work.segments.iter().enumerate() {
            if around.contains(&s.start) && s.tangent_start != [0.0; 2] {
                handles.insert((i as u32, HandleEnd::Start));
            }
            if around.contains(&s.end) && s.tangent_end != [0.0; 2] {
                handles.insert((i as u32, HandleEnd::End));
            }
        }
        let rubber = match (v.tool, v.pen.last, v.pointer, &v.drag) {
            (VectorTool::Pen, Some(last), Some(world), None) => {
                let from = point_of(&v.work, last);
                let mut to = world;
                if v.mods.shift {
                    to = snap45(v.to_world(from), to);
                }
                Some((from, v.to_shape(to)))
            }
            _ => None,
        };
        Some(VectorEditOverlay {
            transform: v.world,
            net: Arc::new(v.work.clone()),
            points: v.sel.points.iter().copied().collect(),
            segments: v.sel.segments.iter().copied().collect(),
            handles: handles
                .into_iter()
                .map(|(s, e)| (s, e == HandleEnd::Start))
                .collect(),
            selected_handles: v
                .sel
                .handles
                .iter()
                .map(|&(s, e)| (s, e == HandleEnd::Start))
                .collect(),
            rubber,
        })
    }
}

/// Свойства фигуры как свойства вектора той же рамки: тип, сеть, скругление
/// уже в сети.
pub(crate) fn as_vector(p: &mut Props) {
    p.kind = NodeKind::Vector;
    p.corner_radius = 0.0;
    p.clip_content = false;
    p.text = None;
    p.vector = Some(Arc::new(VectorNetwork::default()));
}

/// Заменяет фигуру `id` вектором на её месте (та же позиция в родителе, имя,
/// заливки, обводки, поза) с правкой `patch`. Возвращает новый узел.
pub(crate) fn replace_with_vector(
    tx: &Tx,
    id: NodeId,
    base: &Props,
    patch: &PropsPatch,
) -> DocResult<NodeId> {
    let (parent, index) = tx.placement(id).unwrap_or((None, None));
    let mut props = base.clone();
    as_vector(&mut props);
    patch.apply_to(&mut props);
    let new = tx.create_with_props(parent, index, &props)?;
    tx.delete_nodes(&[id])?;
    Ok(new)
}
````

В `crates/ferrum-editor/src/vector_ops.rs` заменить:

````rust

/// Какой конец сегмента: ручка у начала или у конца.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleEnd {
    Start,
````

на:

````rust

/// Какой конец сегмента: ручка у начала или у конца.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HandleEnd {
    Start,
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use overlay::{
    format_size, Measure, MeasureKind, Overlay, OverlayTheme, TextEditOverlay, HANDLE_SIZE,
    LABEL_SIZE, MEASURE_DASH, MEASURE_GAP, PIXEL_GRID_MIN_ZOOM, TEXT_SELECTION_ALPHA,
};
pub use text::{Label, TextEngine, ONEST};
````

на:

````rust
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use overlay::{
    format_size, Measure, MeasureKind, Overlay, OverlayTheme, TextEditOverlay, VectorEditOverlay,
    HANDLE_SIZE, LABEL_SIZE, MEASURE_DASH, MEASURE_GAP, PIXEL_GRID_MIN_ZOOM, TEXT_SELECTION_ALPHA,
    VECTOR_HANDLE_RADIUS, VECTOR_MAX_POINTS_DRAWN, VECTOR_POINT_RADIUS,
};
pub use text::{Label, TextEngine, ONEST};
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
pub const TEXT_SELECTION_ALPHA: f32 = 0.25;

/// Что показать поверх содержимого. Заполняет редактор (этап 3).
#[derive(Debug, Clone)]
````

на:

````rust
pub const TEXT_SELECTION_ALPHA: f32 = 0.25;

/// Радиус точки сети в правке вектора и ручки — CSS-пиксели.
pub const VECTOR_POINT_RADIUS: f64 = 3.5;
pub const VECTOR_HANDLE_RADIUS: f64 = 2.5;
/// Больше стольких точек на экране не рисуются невыделенные: сеть в 100 000
/// точек при отдалении была бы сплошной кашей кружков (и дорогим кадром).
pub const VECTOR_MAX_POINTS_DRAWN: usize = 5000;

/// Правка вектора (подсистема 3, этап 3): сеть, точки, ручки и резинка пера.
/// Всё — в координатах формы узла.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorEditOverlay {
    /// Координаты формы → мир.
    pub transform: Affine,
    pub net: std::sync::Arc<ferrum_doc::VectorNetwork>,
    /// Выделенные точки и сегменты.
    pub points: Vec<u32>,
    pub segments: Vec<u32>,
    /// Видимые ручки: сегмент и `true` — у его начала.
    pub handles: Vec<(u32, bool)>,
    pub selected_handles: Vec<(u32, bool)>,
    /// Резинка пера: от последней точки пути к курсору.
    pub rubber: Option<(Point, Point)>,
}

/// Что показать поверх содержимого. Заполняет редактор (этап 3).
#[derive(Debug, Clone)]
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
    /// Правка текста: курсор и выделение внутри текстового узла.
    pub text_edit: Option<TextEditOverlay>,
}

````

на:

````rust
    /// Правка текста: курсор и выделение внутри текстового узла.
    pub text_edit: Option<TextEditOverlay>,
    /// Правка вектора: сеть, точки и ручки.
    pub vector_edit: Option<VectorEditOverlay>,
}

````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
            measurements: Vec::new(),
            text_edit: None,
        }
    }
````

на:

````rust
            measurements: Vec::new(),
            text_edit: None,
            vector_edit: None,
        }
    }
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
        text_edit(backend, view, dpr, theme, t);
    }
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
````

на:

````rust
        text_edit(backend, view, dpr, theme, t);
    }
    if let Some(v) = &overlay.vector_edit {
        vector_edit(backend, view, dpr, theme, v);
    }
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
        backend.stroke_path(&path, &Stroke::new(dpr));
    }
}

````

на:

````rust
        backend.stroke_path(&path, &Stroke::new(dpr));
    }
}

/// Правка вектора: все сегменты тонкой линией, выделенные — толще, ручки
/// выделенного — линией с кружком, точки — кружками (выделенные залиты).
/// Рисуется только видимое на экране.
fn vector_edit(
    backend: &mut impl RenderBackend,
    view: Affine,
    dpr: f64,
    theme: &OverlayTheme,
    v: &VectorEditOverlay,
) {
    let to_device = view * v.transform;
    let (w, h) = backend.size();
    let screen = Rect::new(0.0, 0.0, w as f64, h as f64).inflate(8.0 * dpr, 8.0 * dpr);
    let net = &*v.net;
    let pt = |i: u32| {
        let p = &net.points[i as usize];
        Point::new(p.x, p.y)
    };
    let seg_path = |i: usize, path: &mut BezPath| {
        let s = &net.segments[i];
        let (a, b) = (to_device * pt(s.start), to_device * pt(s.end));
        let ta =
            to_device * (pt(s.start) + kurbo::Vec2::new(s.tangent_start[0], s.tangent_start[1]));
        let tb = to_device * (pt(s.end) + kurbo::Vec2::new(s.tangent_end[0], s.tangent_end[1]));
        let bb = Rect::from_points(a, b).union_pt(ta).union_pt(tb);
        if bb.x1 < screen.x0 || bb.x0 > screen.x1 || bb.y1 < screen.y0 || bb.y0 > screen.y1 {
            return;
        }
        path.move_to(a);
        if s.is_straight() {
            path.line_to(b);
        } else {
            path.curve_to(ta, tb, b);
        }
    };
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    let mut all = BezPath::new();
    for i in 0..net.segments.len() {
        seg_path(i, &mut all);
    }
    backend.stroke_path(&all, &Stroke::new(dpr));
    if !v.segments.is_empty() {
        let mut sel = BezPath::new();
        for &i in &v.segments {
            if (i as usize) < net.segments.len() {
                seg_path(i as usize, &mut sel);
            }
        }
        backend.stroke_path(&sel, &Stroke::new(2.0 * dpr));
    }
    if let Some((a, b)) = v.rubber {
        let mut r = BezPath::new();
        r.move_to(to_device * a);
        r.line_to(to_device * b);
        backend.stroke_path(&r, &Stroke::new(dpr));
    }
    // Ручки: линия от точки и кружок на конце.
    let handle_r = VECTOR_HANDLE_RADIUS * dpr;
    for &(i, at_start) in &v.handles {
        let Some(s) = net.segments.get(i as usize) else {
            continue;
        };
        let (p, t) = if at_start {
            (pt(s.start), s.tangent_start)
        } else {
            (pt(s.end), s.tangent_end)
        };
        let a = to_device * p;
        let b = to_device * (p + kurbo::Vec2::new(t[0], t[1]));
        let mut line = BezPath::new();
        line.move_to(a);
        line.line_to(b);
        backend.set_transform(Affine::IDENTITY);
        backend.set_color(theme.select);
        backend.stroke_path(&line, &Stroke::new(dpr));
        let circle = kurbo::Circle::new(b, handle_r).to_path(0.1);
        let selected = v.selected_handles.contains(&(i, at_start));
        backend.set_color(if selected {
            theme.select
        } else {
            theme.handle_fill
        });
        backend.fill_path(&circle, Fill::NonZero);
        backend.set_color(theme.select);
        backend.stroke_path(&circle, &Stroke::new(dpr));
    }
    // Точки: видимые; при толпе на экране — только выделенные.
    let visible: Vec<u32> = (0..net.points.len() as u32)
        .filter(|&i| screen.contains(to_device * pt(i)))
        .collect();
    let crowd = visible.len() > VECTOR_MAX_POINTS_DRAWN;
    let r = VECTOR_POINT_RADIUS * dpr;
    let mut filled = BezPath::new();
    let mut hollow = BezPath::new();
    for i in visible {
        let selected = v.points.binary_search(&i).is_ok();
        if crowd && !selected {
            continue;
        }
        let c = kurbo::Circle::new(to_device * pt(i), r);
        if selected {
            filled.extend(c.path_elements(0.1));
        } else {
            hollow.extend(c.path_elements(0.1));
        }
    }
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.handle_fill);
    backend.fill_path(&hollow, Fill::NonZero);
    backend.set_color(theme.select);
    backend.stroke_path(&hollow, &Stroke::new(dpr));
    backend.fill_path(&filled, Fill::NonZero);
    backend.set_color(theme.handle_fill);
    backend.stroke_path(&filled, &Stroke::new(dpr));
}

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        Tool::Image => ToolKind::Image,
        Tool::Text => ToolKind::Text,
    }
}
````

на:

````rust
        Tool::Image => ToolKind::Image,
        Tool::Text => ToolKind::Text,
        Tool::Pen => ToolKind::Pen,
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        ToolKind::Image => Tool::Image,
        ToolKind::Text => Tool::Text,
    }
}
````

на:

````rust
        ToolKind::Image => Tool::Image,
        ToolKind::Text => Tool::Text,
        ToolKind::Pen => Tool::Pen,
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        Cursor::Rotate { angle } => (CursorKind::Rotate, angle),
        Cursor::Text => (CursorKind::Text, 0.0),
    }
}
````

на:

````rust
        Cursor::Rotate { angle } => (CursorKind::Rotate, angle),
        Cursor::Text => (CursorKind::Text, 0.0),
        Cursor::Pen => (CursorKind::Pen, 0.0),
        Cursor::Bucket => (CursorKind::Bucket, 0.0),
    }
}
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    Image,
    Text,
}

````

на:

````rust
    Image,
    Text,
    /// Перо (подсистема 3, этап 3).
    Pen,
}

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    /// Текст в правке и инструмент «Текст».
    Text,
}

````

на:

````rust
    /// Текст в правке и инструмент «Текст».
    Text,
    /// Перо и «Заливка области» в правке вектора.
    Pen,
    Bucket,
}

````

- [ ] **Step 4: Сгенерировать файлы**

Run: `export CARGO_TARGET_DIR=$PWD/target && FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test vector vector_edit`
Expected: создан эталон `crates/ferrum-render/tests/golden/vector_edit.png` (треугольник сети, залитая выделенная точка, ручка с кружком, резинка пера)

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npm run wasm && npm run typecheck`

Expected: PASS: `pen` — 8 тестов, `keymap` — 46 команд, `vector` рендера — 4 теста (с `vector_edit`), все тесты рабочего пространства зелёные; clippy wasm32 без замечаний; `npm run typecheck` без ошибок

- [ ] **Step 6: Закоммитить**

```bash
git add apps/web/src/core/core.ts crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/tools/mod.rs crates/ferrum-editor/src/vector_edit.rs crates/ferrum-editor/src/vector_ops.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/pen.rs crates/ferrum-render/src/lib.rs crates/ferrum-render/src/overlay.rs crates/ferrum-render/tests/vector.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/panels.rs crates/ferrum-wasm/tests/protocol.rs crates/ferrum-render/tests/golden/vector_edit.png
git commit -m "feat(editor): перо и сеанс правки вектора — точки, ручки с зеркальностью, замыкание и соединение, 45°, один шаг на путь; фигура становится вектором при первой правке; оверлей правки

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 6: Режим правки: выделение, перемещение, изгиб, удаление, заливка области

Спека 9: выделение точек, ручек и сегментов (рамкой и Shift), перемещение с привязками к точкам и пиксельной сетке, изгиб Ctrl/Cmd + протяжкой, удаление с сохранением формы (Delete) и с разрывом (Shift+Delete), «Заливка области» (B), выход Esc/Enter. Вход двойным щелчком и Enter, щелчок по другому узлу — выход и обычный щелчок. Команды раскладки в правке перехватывает `vector_command` (удаление, все точки, сдвиг, Esc), остальные правки узлов сначала заканчивают правку.

**Files:**
- Create: `crates/ferrum-editor/src/vector_select.rs`
- Modify: `crates/ferrum-editor/src/{vector_edit,editor,actions,lib}.rs`, `tools/select.rs`
- Test: `crates/ferrum-editor/tests/vector_edit.rs` (новый)

**Interfaces:**
- Consumes: сеанс задачи 5, `vector_ops` задачи 4, `snap::SNAP_THRESHOLD_PX`.
- Produces: `Drag::{Points, Handle, Bend, Marquee}`; внутри крейта — `vector_select_{down,move,up}`, `bucket_click`, `vector_delete(break_path)`, `vector_command(CommandId) -> bool`; двойной щелчок `SelectTool` → `Outbox::edit_vector`; Enter по выделенному — `start_vector_edit`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/vector_edit.rs`:

````rust
//! Режим правки вектора (подсистема 3, этап 3): вход двойным щелчком и Enter,
//! выход Esc и щелчком по другому узлу, выделение точек щелчком, Shift и рамкой,
//! перемещение с привязками, ручки, изгиб, удаление с сохранением формы и с
//! разрывом, «Заливка области», фигура становится вектором при первой правке,
//! отмена в правке, повёрнутый и отражённый узел.

mod common;

use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;
use ferrum_scene::vector::{fit, segment_curve};
use kurbo::{ParamCurve, Point};

/// Квадрат 100 × 100 в (100, 100): четыре точки по часовой от левого верхнего.
fn square(h: &mut H, closed_region: bool) -> NodeId {
    let net = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(100.0, 100.0),
            VectorPoint::new(0.0, 100.0),
        ],
        segments: (0..4)
            .map(|i| VectorSegment::line(i, (i + 1) % 4))
            .collect(),
        regions: if closed_region {
            vec![VectorRegion {
                rule: FillRule::NonZero,
                loops: vec![vec![0, 1, 2, 3]],
                fills: None,
            }]
        } else {
            vec![]
        },
    };
    h.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(net)),
            ..patch(100.0, 100.0, 100.0, 100.0)
        },
    )
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

fn net_of(h: &mut H, id: NodeId) -> VectorNetwork {
    h.props(id).vector.as_deref().unwrap().clone()
}

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-6
}

fn points(h: &H) -> Vec<u32> {
    h.ed.vector_selection()
        .unwrap()
        .points
        .iter()
        .copied()
        .collect()
}

#[test]
fn double_click_and_enter_enter_edit_mode_esc_leaves_it() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.dbl(150.0, 100.0);
    assert!(h.ed.is_editing_vector());
    assert_eq!(h.ed.vector_edit_node(), Some(id));
    h.key("Escape", Modifiers::NONE);
    assert!(!h.ed.is_editing_vector());
    assert_eq!(h.sel(), vec![id]);
    h.key("Enter", Modifiers::NONE);
    assert!(h.ed.is_editing_vector());
    h.key("Enter", Modifiers::NONE);
    assert!(!h.ed.is_editing_vector());
    // Заблокированный — нет.
    h.set(
        id,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    h.ed.select(&[id]);
    h.key("Enter", Modifiers::NONE);
    assert!(!h.ed.is_editing_vector());
}

#[test]
fn drag_point_is_one_step_with_pixel_and_point_snapping() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    let before = h.state();
    let steps = h.ed.step_count();
    // Правая нижняя (200, 200) → (230.4, 240.6): пиксельная сетка округляет.
    h.drag((200.0, 200.0), (230.4, 240.6), Modifiers::NONE);
    assert_eq!(points(&h), vec![2]);
    let w = world_points(&mut h, id);
    assert!(near(w[2], Point::new(230.0, 241.0)), "{w:?}");
    assert!(near(w[0], Point::new(100.0, 100.0)), "остальные на месте");
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
    // К точке сети ближе 4 px по x — привязка к ней, а не к сетке.
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    h.drag((200.0, 200.0), (102.5, 250.0), Modifiers::NONE);
    let w = world_points(&mut h, id);
    assert!(near(w[2], Point::new(100.0, 250.0)), "{w:?}");
}

#[test]
fn shift_click_and_marquee_select_points() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    h.click(100.0, 100.0, Modifiers::NONE);
    h.click(200.0, 100.0, Modifiers::SHIFT);
    assert_eq!(points(&h), vec![0, 1]);
    h.click(100.0, 100.0, Modifiers::SHIFT);
    assert_eq!(points(&h), vec![1]);
    // Рамка с пустого места внутри квадрата захватывает нижние точки.
    h.drag((90.0, 150.0), (210.0, 210.0), Modifiers::NONE);
    assert_eq!(points(&h), vec![2, 3]);
    // Стрелка сдвигает выделенные точки на пиксель (шаг отмены).
    let steps = h.ed.step_count();
    h.key("ArrowDown", Modifiers::NONE);
    assert_eq!(h.ed.step_count(), steps + 1);
    let w = world_points(&mut h, id);
    assert!(near(w[3], Point::new(100.0, 201.0)), "{w:?}");
}

#[test]
fn handles_mirror_and_ctrl_drag_bends_a_segment() {
    let mut h = H::new();
    let e = h.add(None, NodeKind::Ellipse, patch(100.0, 100.0, 200.0, 200.0));
    h.ed.start_vector_edit(e);
    // Правая точка (300, 200); её ручки — вверх и вниз на 200·κ/2.
    h.click(300.0, 200.0, Modifiers::NONE);
    let k = 100.0 * 0.552_284_749_830_793_4;
    h.drag((300.0, 200.0 + k), (330.0, 200.0 + k), Modifiers::NONE);
    let id = h.ed.vector_edit_node().unwrap();
    assert_ne!(id, e, "эллипс стал вектором");
    let net = net_of(&mut h, id);
    // Ручки у точки 1: выходящая к курсору, входящая — зеркально той же длины.
    let out = net.segments[1].tangent_start;
    let inc = net.segments[0].tangent_end;
    assert!(
        (out[0] - 30.0).abs() < 1e-6 && (out[1] - k).abs() < 1e-6,
        "{out:?}"
    );
    assert!(
        (inc[0] + 30.0).abs() < 1e-6 && (inc[1] + k).abs() < 1e-6,
        "{inc:?}"
    );
    // Изгиб: точка нижнего отрезка квадрата уходит под курсор.
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    h.drag((150.0, 200.0), (150.0, 240.0), ctrl());
    let net = net_of(&mut h, id);
    let n = h.ed.scene().node(id).unwrap().clone();
    let m = n.world * fit(&n.props);
    let mid = m * segment_curve(&net, &net.segments[2]).eval(0.5);
    assert!(near(mid, Point::new(150.0, 240.0)), "{mid:?}");
}

#[test]
fn delete_keeps_shape_shift_delete_breaks_and_empty_vector_goes() {
    let mut h = H::new();
    let id = square(&mut h, true);
    h.ed.start_vector_edit(id);
    h.click(200.0, 100.0, Modifiers::NONE);
    h.key("Delete", Modifiers::NONE);
    let net = net_of(&mut h, id);
    assert_eq!((net.points.len(), net.segments.len()), (3, 3));
    assert_eq!(net.regions.len(), 1, "треугольник остался замкнутым");
    // С разрывом: сегменты точки уходят, область — тоже.
    h.click(100.0, 100.0, Modifiers::NONE);
    h.key("Delete", Modifiers::SHIFT);
    let net = net_of(&mut h, id);
    assert_eq!((net.points.len(), net.segments.len()), (2, 1));
    assert!(net.regions.is_empty());
    // Всё выделено и удалено — узла нет, правки нет.
    h.key("KeyA", ctrl());
    h.key("Delete", Modifiers::NONE);
    assert!(h.children(None).is_empty());
    assert!(!h.ed.is_editing_vector());
}

#[test]
fn paint_bucket_toggles_a_closed_area() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    h.key("KeyB", Modifiers::NONE);
    assert_eq!(h.ed.vector_tool(), Some(VectorTool::Bucket));
    let steps = h.ed.step_count();
    h.click(150.0, 150.0, Modifiers::NONE);
    let net = net_of(&mut h, id);
    assert_eq!(net.regions.len(), 1);
    assert_eq!(
        net.regions[0].fills,
        Some(vec![Fill::solid(Color::SHAPE_GRAY)])
    );
    assert_eq!(h.ed.step_count(), steps + 1);
    h.click(150.0, 150.0, Modifiers::NONE);
    assert!(net_of(&mut h, id).regions.is_empty());
    // Мимо замкнутой области — ничего.
    h.click(400.0, 400.0, Modifiers::NONE);
    assert_eq!(h.ed.step_count(), steps + 2);
    h.key("Escape", Modifiers::NONE);
    assert_eq!(h.ed.vector_tool(), Some(VectorTool::Select));
}

#[test]
fn shape_becomes_a_vector_only_when_edited() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let steps = h.ed.step_count();
    h.dbl(150.0, 150.0);
    assert!(h.ed.is_editing_vector());
    h.key("Escape", Modifiers::NONE);
    assert_eq!(
        h.props(r).kind,
        NodeKind::Rect,
        "без правки — прямоугольник"
    );
    assert_eq!(h.ed.step_count(), steps);
    let before = h.state();
    h.key("Enter", Modifiers::NONE);
    // Превью до отпускания: прямоугольник ещё в документе.
    h.drag_hold((200.0, 200.0), (220.0, 230.0), Modifiers::NONE);
    assert_eq!(h.props(r).kind, NodeKind::Rect);
    h.up(220.0, 230.0, Modifiers::NONE);
    let v = h.ed.vector_edit_node().unwrap();
    assert_eq!(h.props(v).kind, NodeKind::Vector);
    let w = world_points(&mut h, v);
    assert!(near(w[2], Point::new(220.0, 230.0)));
    h.ed.end_vector_edit();
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn click_on_another_node_leaves_edit_mode_and_selects_it() {
    let mut h = H::new();
    let id = square(&mut h, false);
    let other = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.start_vector_edit(id);
    h.click(420.0, 420.0, Modifiers::NONE);
    assert!(!h.ed.is_editing_vector());
    assert_eq!(h.sel(), vec![other]);
}

#[test]
fn undo_in_edit_mode_restores_the_network() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.ed.start_vector_edit(id);
    h.drag((200.0, 200.0), (250.0, 250.0), Modifiers::NONE);
    h.key("KeyZ", ctrl());
    assert!(h.ed.is_editing_vector());
    let work = h.ed.vector_work().unwrap().clone();
    assert_eq!(work.points[2], VectorPoint::new(100.0, 100.0));
    h.key("KeyY", ctrl());
    assert_eq!(
        h.ed.vector_work().unwrap().points[2],
        VectorPoint::new(150.0, 150.0)
    );
}

#[test]
fn rotated_and_flipped_vector_moves_points_under_the_cursor() {
    let mut h = H::new();
    let id = square(&mut h, false);
    h.set(
        id,
        PropsPatch {
            rotation: Some(30.0),
            flip_x: Some(true),
            ..Default::default()
        },
    );
    h.ed.set_settings(Settings {
        pixel_snap: false,
        snapping: false,
        ..h.ed.settings().clone()
    });
    let w = world_points(&mut h, id);
    h.ed.start_vector_edit(id);
    let target = Point::new(w[2].x + 25.0, w[2].y - 10.0);
    h.drag((w[2].x, w[2].y), (target.x, target.y), Modifiers::NONE);
    let after = world_points(&mut h, id);
    assert!(near(after[2], target), "{after:?}");
    assert!(near(after[0], w[0]), "остальные на месте");
    let p = h.props(id);
    assert!(
        p.flip_x && (p.rotation - 30.0).abs() < 1e-9,
        "поза узла сохранена"
    );
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor --test vector_edit`
Expected: FAIL — тесты собираются, но падают: двойной щелчок по вектору не входит в правку, протяжка точки в правке её заканчивает (`assertion failed: h.ed.is_editing_vector()`)

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
    pub(crate) fn run(&mut self, cmd: CommandId) {
        use CommandId as C;
        match cmd {
            C::ToolMove => self.set_tool(ToolKind::Move),
````

на:

````rust
    pub(crate) fn run(&mut self, cmd: CommandId) {
        use CommandId as C;
        // Правка вектора перехватывает свои команды (удаление, сдвиг, Esc…).
        if self.vector_edit.is_some() && self.vector_command(cmd) {
            return;
        }
        match cmd {
            C::ToolMove => self.set_tool(ToolKind::Move),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    Tool, ToolKind, ToolOverlay,
};
use crate::vector_edit::{VectorEdit, VectorTool};

/// Раскладка правимого текста и её ключ: узел, версия узла, поколение шрифтов.
````

на:

````rust
    Tool, ToolKind, ToolOverlay,
};
use crate::vector_edit::{Drag, VectorEdit, VectorTool};

/// Раскладка правимого текста и её ключ: узел, версия узла, поколение шрифтов.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        let vector = self.vector_overlay();
        if let Some(v) = &self.vector_edit {
            t.snap_lines = v.snap_lines.clone();
        }
````

на:

````rust
        let vector = self.vector_overlay();
        if let Some(v) = &self.vector_edit {
            if let Some(Drag::Marquee { rect, .. }) = &v.drag {
                t.marquee = *rect;
            }
            t.snap_lines = v.snap_lines.clone();
        }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            if let [id] = self.selection[..] {
                if self.start_text_edit(id, EnterAt::All) {
                    return;
                }
````

на:

````rust
            if let [id] = self.selection[..] {
                if self.start_text_edit(id, EnterAt::All) {
                    return;
                }
                // Enter по вектору или фигуре — правка вектора (как в Figma).
                if self.start_vector_edit(id) {
                    return;
                }
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod vector_edit;
pub mod vector_ops;

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
````

на:

````rust
pub mod vector_edit;
pub mod vector_ops;
mod vector_select;

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
                    return;
                }
            }
        }
````

на:

````rust
                    return;
                }
                // Двойной щелчок по вектору или фигуре — правка вектора; но
                // сначала, как в Figma, — на уровень глубже (в группу или фрейм).
                let same_level = Self::pick_at(cx, &cache.parents, e.point, e.modifiers, false)
                    == Some(id)
                    || cx.selection.contains(&id);
                let shape = same_level
                    && cx.scene.slot(id).is_some_and(|s| {
                        matches!(
                            cx.scene.at(s).props.kind,
                            NodeKind::Vector | NodeKind::Rect | NodeKind::Ellipse | NodeKind::Line
                        ) && !locked(cx.scene, s)
                    });
                if shape {
                    cx.selection.set(vec![id]);
                    cx.out.edit_vector = Some(id);
                    self.state = State::Idle;
                    return;
                }
            }
        }
````

В `crates/ferrum-editor/src/vector_edit.rs` заменить:

````rust
use ferrum_scene::shape::local_transform;
use ferrum_scene::Draft;
use kurbo::{Affine, Line, Point, Vec2};

use crate::editor::{Editor, EditorEvent};
````

на:

````rust
use ferrum_scene::shape::local_transform;
use ferrum_scene::Draft;
use kurbo::{Affine, Line, Point, Rect, Vec2};

use crate::editor::{Editor, EditorEvent};
````

В `crates/ferrum-editor/src/vector_edit.rs` заменить:

````rust
#[derive(Debug, Clone)]
pub(crate) enum Drag {
    /// Протяжка пера: ручки только что поставленной точки.
    PenHandle {
````

на:

````rust
#[derive(Debug, Clone)]
pub(crate) enum Drag {
    /// Точки (и концы выделенных сегментов): `anchor` — точка под курсором в
    /// мире, по ней — привязки.
    Points {
        start_screen: Point,
        start_world: Point,
        anchor: Point,
        origin: VectorNetwork,
        moved: bool,
    },
    /// Ручка сегмента.
    Handle {
        seg: usize,
        end: HandleEnd,
        origin: VectorNetwork,
    },
    /// Изгиб сегмента (Ctrl/Cmd).
    Bend {
        seg: usize,
        t: f64,
        start_world: Point,
        origin: VectorNetwork,
    },
    /// Рамка выделения точек.
    Marquee {
        start_world: Point,
        base: VectorSelection,
        rect: Option<Rect>,
    },
    /// Протяжка пера: ручки только что поставленной точки.
    PenHandle {
````

В `crates/ferrum-editor/src/vector_edit.rs` заменить:

````rust

impl VectorEdit {
    fn to_shape(&self, world: Point) -> Point {
        self.world.inverse() * world
    }

    fn to_world(&self, p: Point) -> Point {
        self.world * p
    }
}

fn point_of(net: &VectorNetwork, i: u32) -> Point {
    let p = &net.points[i as usize];
    Point::new(p.x, p.y)
````

на:

````rust

impl VectorEdit {
    pub(crate) fn to_shape(&self, world: Point) -> Point {
        self.world.inverse() * world
    }

    pub(crate) fn to_world(&self, p: Point) -> Point {
        self.world * p
    }

    /// Линейная часть мир → форма (для сдвигов).
    pub(crate) fn vec_to_shape(&self, v: Vec2) -> Vec2 {
        let inv = self.world.inverse();
        (inv * v.to_point()) - (inv * Point::ZERO)
    }
}

pub(crate) fn point_of(net: &VectorNetwork, i: u32) -> Point {
    let p = &net.points[i as usize];
    Point::new(p.x, p.y)
````

В `crates/ferrum-editor/src/vector_edit.rs` заменить:

````rust
                true
            }
            _ => {
                self.end_vector_edit();
                false
            }
        }
    }
````

на:

````rust
                true
            }
            Some(VectorTool::Bucket) => {
                self.bucket_click(e);
                true
            }
            Some(VectorTool::Select) => self.vector_select_down(e),
            None => false,
        }
    }
````

В `crates/ferrum-editor/src/vector_edit.rs` заменить:

````rust
    /// Движение указателя в правке (с кнопкой и без).
    pub(crate) fn vector_pointer_move(&mut self, e: &PointerEvent) {
        if self.vector_tool() == Some(VectorTool::Pen) {
            self.pen_pointer_move(e);
        }
    }

    /// Отпускание кнопки в правке.
    pub(crate) fn vector_pointer_up(&mut self, _e: &PointerEvent) {
        if self.vector_tool() == Some(VectorTool::Pen) {
            self.pen_pointer_up();
        }
    }
````

на:

````rust
    /// Движение указателя в правке (с кнопкой и без).
    pub(crate) fn vector_pointer_move(&mut self, e: &PointerEvent) {
        match self.vector_tool() {
            Some(VectorTool::Pen) => self.pen_pointer_move(e),
            Some(VectorTool::Select) => self.vector_select_move(e),
            _ => {}
        }
    }

    /// Отпускание кнопки в правке.
    pub(crate) fn vector_pointer_up(&mut self, e: &PointerEvent) {
        match self.vector_tool() {
            Some(VectorTool::Pen) => self.pen_pointer_up(),
            Some(VectorTool::Select) => self.vector_select_up(e),
            _ => {}
        }
    }
````

В `crates/ferrum-editor/src/vector_edit.rs` заменить:

````rust
    pub(crate) fn vector_key(&mut self, e: &KeyEvent) -> bool {
        let plain = e.modifiers == Modifiers::NONE || e.modifiers == Modifiers::SHIFT;
        match e.code.as_str() {
            "Escape" | "Enter" | "NumpadEnter" if plain && !e.repeat => {
                self.vector_escape();
                true
            }
````

на:

````rust
    pub(crate) fn vector_key(&mut self, e: &KeyEvent) -> bool {
        let plain = e.modifiers == Modifiers::NONE || e.modifiers == Modifiers::SHIFT;
        // Во время жеста — только модификаторы; Esc отменяет жест.
        if self.vector_edit.as_ref().is_some_and(|v| v.drag.is_some()) {
            if e.code == "Escape" {
                self.cancel_vector_drag();
            }
            return true;
        }
        match e.code.as_str() {
            "Escape" | "Enter" | "NumpadEnter" if plain && !e.repeat => {
                self.vector_escape();
                true
            }
            // Shift+Delete — удаление с разрывом (без Shift — команда раскладки).
            "Delete" | "Backspace" if e.modifiers == Modifiers::SHIFT => {
                self.vector_delete(true);
                true
            }
            // «Заливка области»: B включает и выключает её.
            "KeyB" if e.modifiers == Modifiers::NONE && !e.repeat => {
                // У нового пути пера областей ещё нет.
                if self.vector_edit_node().is_some() {
                    self.finish_pen_path();
                    if let Some(v) = &mut self.vector_edit {
                        v.tool = if v.tool == VectorTool::Bucket {
                            VectorTool::Select
                        } else {
                            VectorTool::Bucket
                        };
                    }
                }
                true
            }
````

В `crates/ferrum-editor/src/vector_edit.rs` заменить:

````rust
            return;
        }
        if v.tool == VectorTool::Pen && (v.pen.last.is_some() || v.pen.dirty) {
            self.finish_pen_path();
            if let Some(v) = &mut self.vector_edit {
````

на:

````rust
            return;
        }
        if v.tool != VectorTool::Select {
            // Перо и заливка области — к выделению точек (путь пера записан).
            self.finish_pen_path();
            if let Some(v) = &mut self.vector_edit {
````

В `crates/ferrum-editor/src/vector_edit.rs` заменить:

````rust
            return;
        };
        v.drag = None;
        v.snap_lines.clear();
    }

````

на:

````rust
            return;
        };
        match v.drag.take() {
            Some(Drag::Points { origin, .. })
            | Some(Drag::Handle { origin, .. })
            | Some(Drag::Bend { origin, .. }) => {
                v.work = origin;
                v.snap_lines.clear();
                self.scene.clear_preview();
            }
            Some(Drag::Marquee { base, .. }) => v.sel = base,
            Some(Drag::PenHandle { .. }) | None => {}
        }
    }

````

Создать `crates/ferrum-editor/src/vector_select.rs`:

````rust
//! Режим правки вектора, инструмент выделения (спека подсистемы 3, раздел 9):
//! точки, ручки и сегменты щелчком (Shift — добавить и убрать) и рамкой,
//! перемещение с привязкой к точкам сети и пиксельной сетке, ручки с
//! зеркальностью (Alt — разорвать), изгиб сегмента (Ctrl/Cmd + протяжка),
//! удаление с сохранением формы (Delete) и с разрывом (Shift+Delete), сдвиг
//! стрелками, «Заливка области» (B). Каждая правка — один шаг отмены.

use std::collections::BTreeSet;

use ferrum_doc::NodeId;
use kurbo::{Line, Point, Rect, Vec2};

use crate::commands::CommandId;
use crate::editor::{Editor, EditorEvent};
use crate::input::PointerEvent;
use crate::snap::SNAP_THRESHOLD_PX;
use crate::tools::{sync, DRAG_THRESHOLD_PX};
use crate::vector_edit::{point_of, Drag, VectorSelection, POINT_HIT_PX};
use crate::vector_ops::{
    bend, delete_points_break, delete_points_keep_shape, delete_segments, move_points, set_handle,
    toggle_fill, HandleEnd,
};

impl Editor {
    /// Видимая ручка под экранной точкой: у выделенных точек, концов выделенных
    /// сегментов и выделенных ручек (те же, что рисует оверлей).
    fn vector_handle_at(&self, screen: Point) -> Option<(usize, HandleEnd)> {
        let overlay = self.vector_overlay()?;
        let v = self.vector_edit.as_ref()?;
        let to_screen = self.camera.world_to_screen_transform() * v.world;
        overlay.handles.iter().find_map(|&(s, at_start)| {
            let seg = v.work.segments.get(s as usize)?;
            let (p, t) = if at_start {
                (seg.start, seg.tangent_start)
            } else {
                (seg.end, seg.tangent_end)
            };
            let h = point_of(&v.work, p) + Vec2::new(t[0], t[1]);
            ((to_screen * h - screen).hypot() <= POINT_HIT_PX).then_some((
                s as usize,
                if at_start {
                    HandleEnd::Start
                } else {
                    HandleEnd::End
                },
            ))
        })
    }

    /// Нажатие инструментом выделения в правке. `false` — нажатие по другому
    /// узлу: правка кончилась, нажатие — обычное.
    pub(crate) fn vector_select_down(&mut self, e: &PointerEvent) -> bool {
        let world = self.camera.screen_to_world(e.point);
        let primary = e.modifiers.primary(self.settings.platform);
        let shift = e.modifiers.shift;
        if let Some((seg, end)) = self.vector_handle_at(e.point) {
            let v = self.vector_edit.as_mut().expect("правка идёт");
            v.sel.handles = BTreeSet::from([(seg as u32, end)]);
            v.drag = Some(Drag::Handle {
                seg,
                end,
                origin: v.work.clone(),
            });
            return true;
        }
        if let Some(p) = self.vector_point_at(e.point) {
            let v = self.vector_edit.as_mut().expect("правка идёт");
            let anchor = v.to_world(point_of(&v.work, p));
            if shift && v.sel.points.contains(&p) {
                v.sel.points.remove(&p);
                return true;
            }
            if shift {
                v.sel.points.insert(p);
            } else if !v.sel.points.contains(&p) {
                v.sel = VectorSelection {
                    points: BTreeSet::from([p]),
                    ..Default::default()
                };
            }
            v.sel.handles.clear();
            v.drag = Some(Drag::Points {
                start_screen: e.point,
                start_world: world,
                anchor,
                origin: v.work.clone(),
                moved: false,
            });
            return true;
        }
        if let Some((seg, t)) = self.vector_segment_at(e.point) {
            let v = self.vector_edit.as_mut().expect("правка идёт");
            if primary {
                v.drag = Some(Drag::Bend {
                    seg,
                    t,
                    start_world: world,
                    origin: v.work.clone(),
                });
                return true;
            }
            let s = seg as u32;
            if shift && v.sel.segments.contains(&s) {
                v.sel.segments.remove(&s);
                return true;
            }
            if shift {
                v.sel.segments.insert(s);
            } else if !v.sel.segments.contains(&s) {
                v.sel = VectorSelection {
                    segments: BTreeSet::from([s]),
                    ..Default::default()
                };
            }
            v.sel.handles.clear();
            v.drag = Some(Drag::Points {
                start_screen: e.point,
                start_world: world,
                anchor: world,
                origin: v.work.clone(),
                moved: false,
            });
            return true;
        }
        // Другой узел под точкой — выход из правки, щелчок как обычный.
        let tol = crate::tools::HIT_TOLERANCE_PX / self.camera.zoom();
        let edited = self.vector_edit_node();
        if let Some(hit) = self.scene.hit_test(world, tol) {
            if Some(hit) != edited && !self.scene.is_draft(hit) {
                self.end_vector_edit();
                self.selection.set(Vec::new());
                return false;
            }
        }
        let v = self.vector_edit.as_mut().expect("правка идёт");
        let base = if shift {
            v.sel.clone()
        } else {
            VectorSelection::default()
        };
        v.sel = base.clone();
        v.drag = Some(Drag::Marquee {
            start_world: world,
            base,
            rect: None,
        });
        true
    }

    /// Протяжка инструментом выделения в правке.
    pub(crate) fn vector_select_move(&mut self, e: &PointerEvent) {
        let world = self.camera.screen_to_world(e.point);
        let zoom = self.camera.zoom();
        let pixel_snap = self.settings.pixel_snap;
        let snapping = self.settings.snapping;
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        let Some(drag) = v.drag.clone() else {
            return;
        };
        match drag {
            Drag::Points {
                start_screen,
                start_world,
                anchor,
                origin,
                moved,
            } => {
                if !moved && (e.point - start_screen).hypot() < DRAG_THRESHOLD_PX {
                    return;
                }
                let moving = v.sel.moving_points(&origin);
                let mut target = anchor + (world - start_world);
                if e.modifiers.shift {
                    // Shift — по оси, вдоль которой сдвиг больше.
                    let d = target - anchor;
                    target = if d.x.abs() >= d.y.abs() {
                        Point::new(target.x, anchor.y)
                    } else {
                        Point::new(anchor.x, target.y)
                    };
                }
                let mut lines = Vec::new();
                let (mut sx, mut sy) = (false, false);
                if snapping {
                    // Привязка к точкам сети, которые не двигаются: по каждой оси —
                    // ближайшая в пределах порога.
                    let moving_set: BTreeSet<u32> = moving.iter().copied().collect();
                    let threshold = SNAP_THRESHOLD_PX / zoom;
                    let mut bx: Option<(f64, Point)> = None;
                    let mut by: Option<(f64, Point)> = None;
                    for i in 0..origin.points.len() as u32 {
                        if moving_set.contains(&i) {
                            continue;
                        }
                        let q = v.to_world(point_of(&origin, i));
                        let (dx, dy) = ((q.x - target.x).abs(), (q.y - target.y).abs());
                        if dx <= threshold && bx.is_none_or(|b| dx < b.0) {
                            bx = Some((dx, q));
                        }
                        if dy <= threshold && by.is_none_or(|b| dy < b.0) {
                            by = Some((dy, q));
                        }
                    }
                    if let Some((_, q)) = bx {
                        target.x = q.x;
                        sx = true;
                        lines.push(Line::new((q.x, q.y), (q.x, target.y)));
                    }
                    if let Some((_, q)) = by {
                        target.y = q.y;
                        sy = true;
                        lines.push(Line::new((q.x, q.y), (target.x, q.y)));
                    }
                }
                // Пиксельная сетка — по осям без привязки к точке.
                if pixel_snap {
                    if !sx {
                        target.x = target.x.round();
                    }
                    if !sy {
                        target.y = target.y.round();
                    }
                }
                let d = v.to_shape(target) - v.to_shape(anchor);
                v.work = move_points(&origin, &moving, d);
                v.snap_lines = lines;
                v.drag = Some(Drag::Points {
                    start_screen,
                    start_world,
                    anchor,
                    origin,
                    moved: true,
                });
            }
            Drag::Handle { seg, end, origin } => {
                let s = &origin.segments[seg];
                let p = point_of(
                    &origin,
                    if end == HandleEnd::Start {
                        s.start
                    } else {
                        s.end
                    },
                );
                let t = v.to_shape(world) - p;
                v.work = set_handle(&origin, seg, end, t, e.modifiers.alt);
            }
            Drag::Bend {
                seg,
                t,
                start_world,
                origin,
            } => {
                let d = v.to_shape(world) - v.to_shape(start_world);
                v.work = bend(&origin, seg, t, d);
            }
            Drag::Marquee {
                start_world, base, ..
            } => {
                let rect = Rect::from_points(start_world, world);
                let mut sel = base.clone();
                for (i, p) in v.work.points.iter().enumerate() {
                    if rect.contains(v.to_world(Point::new(p.x, p.y))) {
                        sel.points.insert(i as u32);
                    }
                }
                v.sel = sel;
                v.drag = Some(Drag::Marquee {
                    start_world,
                    base,
                    rect: Some(rect),
                });
                return;
            }
            Drag::PenHandle { .. } => return,
        }
        self.show_vector_work();
    }

    /// Отпускание инструментом выделения: правка жеста — одним шагом.
    pub(crate) fn vector_select_up(&mut self, _e: &PointerEvent) {
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        let drag = v.drag.take();
        v.snap_lines.clear();
        let changed = match &drag {
            Some(Drag::Points { origin, .. })
            | Some(Drag::Handle { origin, .. })
            | Some(Drag::Bend { origin, .. }) => *origin != v.work,
            _ => false,
        };
        if changed {
            self.commit_vector_work();
        } else if matches!(
            drag,
            Some(Drag::Points { .. }) | Some(Drag::Handle { .. }) | Some(Drag::Bend { .. })
        ) {
            self.scene.clear_preview();
        }
    }

    /// «Заливка области» в точке нажатия: заливка грани включается или снимается.
    pub(crate) fn bucket_click(&mut self, e: &PointerEvent) {
        let world = self.camera.screen_to_world(e.point);
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        let filled = v.base.fills.iter().any(|f| f.visible);
        let at = v.to_shape(world);
        if let Some(net) = toggle_fill(&v.work, at, filled) {
            v.work = net;
            self.commit_vector_work();
        }
    }

    /// Удаление выделенного в правке: точки с сохранением формы (или с
    /// разрывом), иначе сегменты. Пустая сеть — узел удаляется (как в Figma).
    pub(crate) fn vector_delete(&mut self, break_path: bool) {
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        if v.sel.is_empty() || v.id.is_none() {
            return;
        }
        let net = if !v.sel.points.is_empty() {
            let pts: Vec<u32> = v.sel.points.iter().copied().collect();
            if break_path {
                delete_points_break(&v.work, &pts)
            } else {
                delete_points_keep_shape(&v.work, &pts)
            }
        } else if !v.sel.segments.is_empty() {
            let segs: Vec<u32> = v.sel.segments.iter().copied().collect();
            delete_segments(&v.work, &segs)
        } else {
            return;
        };
        v.sel = VectorSelection::default();
        if net.points.is_empty() {
            let id = v.id.expect("проверено выше");
            self.vector_edit = None;
            self.delete_node(id);
            return;
        }
        v.work = net;
        self.commit_vector_work();
    }

    /// Удаляет узел одним шагом (сеть правки опустела).
    fn delete_node(&mut self, id: NodeId) {
        self.scene.clear_preview();
        let page = self.page;
        let result = self.doc.transact(page, vec![id], |tx| {
            tx.delete_nodes(&[id])?;
            Ok(vec![])
        });
        let changed = sync(&mut self.doc, page, &mut self.scene, &mut self.out);
        match result {
            Ok(_) => {
                if changed {
                    self.out.steps += 1;
                }
                self.selection.clear();
            }
            Err(e) => self.out.events.push(EditorEvent::Error(e.to_string())),
        }
    }

    /// Сдвиг выделенных точек стрелками на `d` в мире.
    fn vector_nudge(&mut self, d: Vec2) {
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        let moving = v.sel.moving_points(&v.work);
        if moving.is_empty() {
            return;
        }
        let local = v.vec_to_shape(d);
        v.work = move_points(&v.work, &moving, local);
        self.commit_vector_work();
    }

    /// Команда раскладки во время правки вектора. `true` — правка её выполнила;
    /// `false` — команда обычная (вид, отмена и повтор работают в правке, а
    /// правки выделения узлов сначала заканчивают её).
    pub(crate) fn vector_command(&mut self, cmd: CommandId) -> bool {
        use CommandId as C;
        match cmd {
            C::Delete => self.vector_delete(false),
            C::SelectAll => {
                if let Some(v) = &mut self.vector_edit {
                    v.sel = VectorSelection {
                        points: (0..v.work.points.len() as u32).collect(),
                        ..Default::default()
                    };
                }
            }
            C::Escape => self.vector_escape(),
            C::NudgeLeft => self.vector_nudge(Vec2::new(-1.0, 0.0)),
            C::NudgeRight => self.vector_nudge(Vec2::new(1.0, 0.0)),
            C::NudgeUp => self.vector_nudge(Vec2::new(0.0, -1.0)),
            C::NudgeDown => self.vector_nudge(Vec2::new(0.0, 1.0)),
            C::NudgeLeftBig => self.vector_nudge(Vec2::new(-10.0, 0.0)),
            C::NudgeRightBig => self.vector_nudge(Vec2::new(10.0, 0.0)),
            C::NudgeUpBig => self.vector_nudge(Vec2::new(0.0, -10.0)),
            C::NudgeDownBig => self.vector_nudge(Vec2::new(0.0, 10.0)),
            // Вид, отмена, инструменты пера и выделения — как обычно.
            C::Undo
            | C::Redo
            | C::ToolPen
            | C::ToolMove
            | C::HandHold
            | C::ToolHand
            | C::ZoomToFit
            | C::ZoomToSelection
            | C::Zoom100
            | C::ZoomIn
            | C::ZoomOut
            | C::ToggleUi
            | C::Save => return false,
            // Остальное — правки узлов: правка вектора кончается.
            _ => {
                self.end_vector_edit();
                return false;
            }
        }
        self.emit();
        true
    }
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `vector_edit` — 10 тестов, `selection` (двойной щелчок в группе — на уровень глубже) и `pen` — зелёные

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/tools/select.rs crates/ferrum-editor/src/vector_edit.rs crates/ferrum-editor/src/vector_select.rs crates/ferrum-editor/tests/vector_edit.rs
git commit -m "feat(editor): режим правки вектора — выделение точек, сегментов и ручек, рамка, перемещение с привязками, изгиб, удаление с сохранением формы и с разрывом, заливка области; вход двойным щелчком и Enter

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 7: «Сплющить», «Стрелка» и стиль обводки из панели

Спека 9: «Сплющить» (Ctrl/Cmd+E) — выделение в один вектор, инструмент «Стрелка» (Shift+L) — линия с `arrowLines` на конце, окончания и соединения правятся из секции «Обводка» (здесь — правка панели `PanelEdit::StrokeStyle` с превью и одним шагом).

**Files:**
- Create: `crates/ferrum-editor/src/flatten.rs`
- Modify: `crates/ferrum-editor/src/{actions,commands,editor,panel,lib}.rs`, `tools/{draw,mod}.rs`, `crates/ferrum-wasm/src/{protocol,core}.rs`
- Test: `crates/ferrum-editor/tests/flatten.rs` (новый), `crates/ferrum-editor/tests/keymap.rs`

**Interfaces:**
- Consumes: `vector_ops::{shape_network, merge_networks, reframe}`, `vector_edit::{as_vector, replace_with_vector}`.
- Produces: `CommandId::{ToolArrow, Flatten}` (`tool.arrow` — Shift+L, `object.flatten` — Mod+E), `ToolKind::Arrow`, `Shape::Arrow`, `PanelEdit::StrokeStyle { cap_start, cap_end, join, miter_limit }`, `Editor::ordered_selection` (pub(crate)), `Tool::Arrow`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/flatten.rs`:

````rust
//! «Сплющить», «Стрелка» и стиль обводки из панели (подсистема 3, этап 3).

mod common;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;
use ferrum_scene::vector::fit;
use kurbo::Point;

fn world_points(h: &mut H, id: NodeId) -> Vec<Point> {
    let n = h.ed.scene().node(id).unwrap().clone();
    let net = n.props.vector.as_deref().unwrap().clone();
    let m = n.world * fit(&n.props);
    net.points
        .iter()
        .map(|p| m * Point::new(p.x, p.y))
        .collect()
}

fn has(points: &[Point], p: Point) -> bool {
    points.iter().any(|q| (*q - p).hypot() < 1e-6)
}

#[test]
fn several_shapes_become_one_vector_in_one_step() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    let b = h.rect(None, 300.0, 100.0, 50.0, 50.0);
    h.set(
        a,
        PropsPatch {
            fills: Some(vec![Fill::solid(Color::rgba(255, 0, 0, 255))]),
            ..Default::default()
        },
    );
    h.set(
        b,
        PropsPatch {
            rotation: Some(90.0),
            name: Some("Верхний".into()),
            ..Default::default()
        },
    );
    let corner_b = h.ed.scene().node(b).unwrap().world * Point::new(50.0, 50.0);
    h.ed.select(&[a, b]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyE", ctrl());
    let all = h.children(None);
    assert_eq!(all.len(), 1);
    let v = all[0];
    assert_eq!(h.sel(), vec![v]);
    let p = h.props(v);
    assert_eq!(p.kind, NodeKind::Vector);
    assert_eq!(p.name, "Верхний", "свойства верхнего узла");
    assert_eq!(p.rotation, 0.0);
    let net = p.vector.as_deref().unwrap();
    assert_eq!((net.points.len(), net.regions.len()), (8, 2));
    // Нижний сохранил свою заливку в своей области; верхний — заливки узла.
    assert_eq!(
        net.regions[0].fills,
        Some(vec![Fill::solid(Color::rgba(255, 0, 0, 255))])
    );
    assert_eq!(net.regions[1].fills, None);
    let w = world_points(&mut h, v);
    assert!(
        has(&w, Point::new(100.0, 100.0)) && has(&w, corner_b),
        "{w:?}"
    );
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn one_shape_keeps_its_pose_and_groups_flatten_whole() {
    let mut h = H::new();
    let e = h.add(None, NodeKind::Ellipse, patch(100.0, 100.0, 80.0, 40.0));
    h.set(
        e,
        PropsPatch {
            rotation: Some(30.0),
            flip_y: Some(true),
            ..Default::default()
        },
    );
    h.ed.select(&[e]);
    h.key("KeyE", ctrl());
    let v = h.sel()[0];
    let p = h.props(v);
    assert_eq!(
        (p.kind, p.rotation, p.flip_y),
        (NodeKind::Vector, 30.0, true)
    );
    assert_eq!((p.w, p.h), (80.0, 40.0));
    // Группа из прямоугольника и линии — вектор, группы нет.
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let l = h.add(None, NodeKind::Line, patch(20.0, 5.0, 30.0, 0.0));
    h.ed.select(&[r, l]);
    h.key("KeyG", ctrl());
    h.key("KeyE", ctrl());
    let all = h.children(None);
    assert_eq!(all.len(), 1);
    assert_eq!(h.props(all[0]).kind, NodeKind::Vector);
    let net = h.props(all[0]).vector.as_deref().unwrap().clone();
    assert_eq!((net.points.len(), net.segments.len()), (6, 5));
}

#[test]
fn nothing_to_flatten_is_an_error_without_a_step() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 100.0, 100.0);
    h.ed.select(&[f]);
    let steps = h.ed.step_count();
    h.ed.take_events();
    h.key("KeyE", ctrl());
    assert_eq!(h.ed.step_count(), steps);
    assert!(h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::Error(m) if m.starts_with("Нечего сплющить"))));
}

#[test]
fn arrow_tool_draws_a_line_with_arrow_lines_at_its_end() {
    let mut h = H::new();
    h.key("KeyL", Modifiers::SHIFT);
    assert_eq!(h.ed.tool_kind(), ToolKind::Arrow);
    h.drag((100.0, 100.0), (300.0, 100.0), Modifiers::NONE);
    let id = h.sel()[0];
    let p = h.props(id);
    assert_eq!((p.kind, p.w), (NodeKind::Line, 200.0));
    assert_eq!(p.stroke_style.cap_end, StrokeCap::ArrowLines);
    assert_eq!(p.stroke_style.cap_start, StrokeCap::None);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
}

#[test]
fn stroke_style_from_the_panel_is_one_step_and_skips_groups() {
    let mut h = H::new();
    let l = h.add(None, NodeKind::Line, patch(0.0, 0.0, 100.0, 0.0));
    let r = h.rect(None, 200.0, 0.0, 50.0, 50.0);
    h.ed.select(&[l, r]);
    let before = h.state();
    let steps = h.ed.step_count();
    let edit = PanelEdit::StrokeStyle {
        cap_start: Some(StrokeCap::Round),
        cap_end: None,
        join: Some(StrokeJoin::Bevel),
        miter_limit: Some(10.0),
    };
    h.ed.edit_selection(&edit, Phase::Preview);
    assert_eq!(
        h.props(l).stroke_style.cap_start,
        StrokeCap::None,
        "превью — только в сцене"
    );
    h.ed.edit_selection(&edit, Phase::Commit);
    let s = h.props(l).stroke_style;
    assert_eq!(
        (s.cap_start, s.cap_end, s.join, s.miter_limit),
        (StrokeCap::Round, StrokeCap::None, StrokeJoin::Bevel, 10.0)
    );
    assert_eq!(h.props(r).stroke_style.join, StrokeJoin::Bevel);
    h.assert_one_step(steps, &before);
}
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 46);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 48);
}

````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-editor --test flatten`
Expected: FAIL — не собирается: нет `PanelEdit::StrokeStyle`, `ToolKind::Arrow`

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::ToolText => self.set_tool(ToolKind::Text),
            C::ToolPen => self.set_tool(ToolKind::Pen),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
````

на:

````rust
            C::ToolText => self.set_tool(ToolKind::Text),
            C::ToolPen => self.set_tool(ToolKind::Pen),
            C::ToolArrow => self.set_tool(ToolKind::Arrow),
            C::Flatten => self.flatten(),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust

    /// Выделение верхнего уровня в порядке отрисовки, снизу вверх.
    fn ordered_selection(&self) -> Vec<NodeId> {
        let mut ids: Vec<(Vec<u32>, NodeId)> = top_level(&self.scene, &self.selection)
            .into_iter()
````

на:

````rust

    /// Выделение верхнего уровня в порядке отрисовки, снизу вверх.
    pub(crate) fn ordered_selection(&self) -> Vec<NodeId> {
        let mut ids: Vec<(Vec<u32>, NodeId)> = top_level(&self.scene, &self.selection)
            .into_iter()
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// «Перо» (подсистема 3, этап 3).
    ToolPen,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
````

на:

````rust
    /// «Перо» (подсистема 3, этап 3).
    ToolPen,
    /// «Стрелка»: линия со стрелкой на конце.
    ToolArrow,
    /// «Сплющить»: выделение — в один вектор.
    Flatten,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    cmd(C::ToolText, "tool.text", "Текст", &["KeyT"]),
    cmd(C::ToolPen, "tool.pen", "Перо", &["KeyP"]),
    cmd(
        C::HandHold,
````

на:

````rust
    cmd(C::ToolText, "tool.text", "Текст", &["KeyT"]),
    cmd(C::ToolPen, "tool.pen", "Перо", &["KeyP"]),
    cmd(C::ToolArrow, "tool.arrow", "Стрелка", &["Shift+KeyL"]),
    cmd(
        C::HandHold,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        &["Mod+Shift+KeyG"],
    ),
    cmd(
        C::FrameSelection,
````

на:

````rust
        &["Mod+Shift+KeyG"],
    ),
    cmd(C::Flatten, "object.flatten", "Сплющить", &["Mod+KeyE"]),
    cmd(
        C::FrameSelection,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            ToolKind::Ellipse => Box::new(DrawTool::new(Shape::Ellipse)),
            ToolKind::Line => Box::new(DrawTool::new(Shape::Line)),
            ToolKind::Hand => Box::new(HandTool::new()),
            ToolKind::Text => Box::new(TextTool::new()),
````

на:

````rust
            ToolKind::Ellipse => Box::new(DrawTool::new(Shape::Ellipse)),
            ToolKind::Line => Box::new(DrawTool::new(Shape::Line)),
            ToolKind::Arrow => Box::new(DrawTool::new(Shape::Arrow)),
            ToolKind::Hand => Box::new(HandTool::new()),
            ToolKind::Text => Box::new(TextTool::new()),
````

Создать `crates/ferrum-editor/src/flatten.rs`:

````rust
//! «Сплющить» (Ctrl/Cmd+E, спека подсистемы 3, раздел 9): выделенные векторы и
//! фигуры (и такие же узлы в выделенных группах) — один вектор одним шагом
//! отмены. Один узел сохраняет свою позу (поворот, отражение), несколько —
//! сливаются в координатах родителя верхнего из них; вектор встаёт на место
//! верхнего и берёт его свойства (имя, заливки, обводки), а области других
//! узлов сохраняют свои заливки цветом.

use crate::editor::{Editor, EditorEvent};
use crate::geometry::parent_world;
use crate::selection::locked;
use crate::vector_edit::{as_vector, replace_with_vector};
use crate::vector_ops::{merge_networks, reframe, shape_network};
use ferrum_doc::{FillKind, NodeId, NodeKind, Props};

/// Сплющивается ли узел этого типа.
fn flattenable(k: NodeKind) -> bool {
    matches!(
        k,
        NodeKind::Vector | NodeKind::Rect | NodeKind::Ellipse | NodeKind::Line
    )
}

impl Editor {
    /// Сплющивает выделение в один вектор.
    pub(crate) fn flatten(&mut self) {
        self.finish_vector_edit();
        let ids = self.ordered_selection();
        // Узлы-источники снизу вверх: выделенные фигуры и фигуры в выделенных группах.
        let mut leaves: Vec<NodeId> = Vec::new();
        let mut removed: Vec<NodeId> = Vec::new();
        for id in &ids {
            let Some(slot) = self.scene.slot(*id) else {
                continue;
            };
            if locked(&self.scene, slot) {
                continue;
            }
            let kind = self.scene.at(slot).props.kind;
            if flattenable(kind) {
                leaves.push(*id);
                removed.push(*id);
            } else if kind == NodeKind::Group {
                let mut inner = Vec::new();
                let mut stack = vec![slot];
                let mut foreign = false;
                while let Some(s) = stack.pop() {
                    let n = self.scene.at(s);
                    match n.props.kind {
                        NodeKind::Group => stack.extend(n.children.iter().rev()),
                        k if flattenable(k) => inner.push((self.scene.order_key(s), n.id)),
                        _ => foreign = true,
                    }
                }
                // Группу с текстом, изображением или фреймом сплющить целиком нельзя:
                // их пришлось бы потерять вместе с группой.
                if foreign {
                    continue;
                }
                inner.sort_by(|a, b| a.0.cmp(&b.0));
                leaves.extend(inner.into_iter().map(|(_, id)| id));
                removed.push(*id);
            }
        }
        if leaves.is_empty() {
            self.out.events.push(EditorEvent::Error(
                "Нечего сплющить: сплющиваются векторы, прямоугольники, эллипсы, линии и их группы"
                    .into(),
            ));
            return;
        }
        let top = *removed.last().expect("есть узлы");
        let top_slot = self.scene.slot(top).expect("в сцене");
        // Один узел — вектор той же позы на его месте.
        if let ([one], [only]) = (&leaves[..], &removed[..]) {
            if one == only {
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
                    .commit(|tx, _| Ok(vec![replace_with_vector(tx, id, &props, &patch)?]));
                return;
            }
        }
        // Несколько — в координатах родителя верхнего узла.
        let pw = parent_world(&self.scene, top_slot);
        let inv = pw.inverse();
        let top_leaf = self
            .scene
            .node(*leaves.last().expect("есть узлы"))
            .expect("в сцене")
            .props
            .clone();
        let mut parts = Vec::new();
        for id in &leaves {
            let n = self.scene.node(*id).expect("в сцене");
            let Some(mut net) = shape_network(&n.props) else {
                continue;
            };
            // Заливки узла, отличные от заливок результата, остаются у его областей.
            let own = n.props.fills != top_leaf.fills
                && n.props.fills.iter().all(|f| f.kind == FillKind::Solid);
            for r in &mut net.regions {
                if r.fills.is_none() && own {
                    r.fills = Some(n.props.fills.clone());
                }
            }
            parts.push((net, inv * n.world));
        }
        let merged = merge_networks(&parts);
        let mut base = Props {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            rotation: 0.0,
            flip_x: false,
            flip_y: false,
            ..top_leaf
        };
        as_vector(&mut base);
        base.name = self.scene.at(top_slot).props.name.clone();
        let patch = reframe(&base, merged);
        patch.apply_to(&mut base);
        let (parent, index) = (
            self.scene.at(top_slot).parent.map(|p| self.scene.at(p).id),
            self.scene.index(top_slot) as usize + 1,
        );
        self.ctx().commit(|tx, _| {
            let v = tx.create_with_props(parent, Some(index), &base)?;
            tx.delete_nodes(&removed)?;
            Ok(vec![v])
        });
    }
}
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod commands;
mod editor;
pub mod geometry;
pub mod input;
````

на:

````rust
pub mod commands;
mod editor;
mod flatten;
pub mod geometry;
pub mod input;
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
use std::collections::HashSet;

use ferrum_doc::{Fill, NodeId, NodeKind, Props, PropsPatch, Stroke};
use ferrum_scene::shape::COORD_LIMIT;
use ferrum_scene::{Scene, Slot};
````

на:

````rust
use std::collections::HashSet;

use ferrum_doc::{Fill, NodeId, NodeKind, Props, PropsPatch, Stroke, StrokeCap, StrokeJoin};
use ferrum_scene::shape::COORD_LIMIT;
use ferrum_scene::{Scene, Slot};
````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
    /// Ширина и высота сразу (пресет фрейма) — одним шагом.
    Size { w: f64, h: f64 },
}

````

на:

````rust
    /// Ширина и высота сразу (пресет фрейма) — одним шагом.
    Size { w: f64, h: f64 },
    /// Стиль обводки: окончания начала и конца, соединение, предел острого
    /// угла (`None` — не трогать). У групп и текста стиля нет — пропускаются.
    StrokeStyle {
        cap_start: Option<StrokeCap>,
        cap_end: Option<StrokeCap>,
        join: Option<StrokeJoin>,
        miter_limit: Option<f64>,
    },
}

````

В `crates/ferrum-editor/src/panel.rs` заменить:

````rust
                }
            }
            PanelEdit::Strokes(strokes) => {
                if kind != NodeKind::Group {
````

на:

````rust
                }
            }
            PanelEdit::StrokeStyle {
                cap_start,
                cap_end,
                join,
                miter_limit,
            } => {
                if kind != NodeKind::Group && kind != NodeKind::Text {
                    out.push((
                        t.id,
                        PropsPatch {
                            stroke_cap_start: *cap_start,
                            stroke_cap_end: *cap_end,
                            stroke_join: *join,
                            stroke_miter_limit: *miter_limit,
                            ..Default::default()
                        },
                    ));
                }
            }
            PanelEdit::Strokes(strokes) => {
                if kind != NodeKind::Group {
````

В `crates/ferrum-editor/src/tools/draw.rs` заменить:

````rust
//! Рисование фигур: фрейм (F, A), прямоугольник (R), эллипс (O), линия (L) и
//! размещение изображения (Ctrl/Cmd+Shift+K). Пока кнопка нажата, фигура — черновик
//! сцены; при отпускании она создаётся одной транзакцией и выделяется, а редактор
````

на:

````rust
//! Рисование фигур: фрейм (F, A), прямоугольник (R), эллипс (O), линия (L),
//! стрелка (Shift+L — линия со стрелкой на конце, как в Figma) и
//! размещение изображения (Ctrl/Cmd+Shift+K). Пока кнопка нажата, фигура — черновик
//! сцены; при отпускании она создаётся одной транзакцией и выделяется, а редактор
````

В `crates/ferrum-editor/src/tools/draw.rs` заменить:

````rust
use std::collections::HashSet;

use ferrum_doc::{Fill, NodeId, NodeKind, Props};
use ferrum_scene::Draft;
use kurbo::{Affine, Point, Rect, Vec2};
````

на:

````rust
use std::collections::HashSet;

use ferrum_doc::{Fill, NodeId, NodeKind, Props, StrokeCap};
use ferrum_scene::Draft;
use kurbo::{Affine, Point, Rect, Vec2};
````

В `crates/ferrum-editor/src/tools/draw.rs` заменить:

````rust
    Ellipse,
    Line,
    /// Изображение по хэшу байтов (байты кладёт в документ интерфейс) и его
    /// исходный размер: клик ставит его в этом размере, перетаскивание сохраняет
````

на:

````rust
    Ellipse,
    Line,
    /// Линия с окончанием `arrowLines` на конце.
    Arrow,
    /// Изображение по хэшу байтов (байты кладёт в документ интерфейс) и его
    /// исходный размер: клик ставит его в этом размере, перетаскивание сохраняет
````

В `crates/ferrum-editor/src/tools/draw.rs` заменить:

````rust
            Shape::Rect => NodeKind::Rect,
            Shape::Ellipse => NodeKind::Ellipse,
            Shape::Line => NodeKind::Line,
            Shape::Image { .. } => NodeKind::Image,
        }
````

на:

````rust
            Shape::Rect => NodeKind::Rect,
            Shape::Ellipse => NodeKind::Ellipse,
            Shape::Line | Shape::Arrow => NodeKind::Line,
            Shape::Image { .. } => NodeKind::Image,
        }
````

В `crates/ferrum-editor/src/tools/draw.rs` заменить:

````rust
        if let Shape::Image { hash, .. } = &self.shape {
            props.fills = vec![Fill::image(hash.clone())];
        }
        let inv = start.parent_world.inverse();
````

на:

````rust
        if let Shape::Image { hash, .. } = &self.shape {
            props.fills = vec![Fill::image(hash.clone())];
        }
        if self.shape == Shape::Arrow {
            props.stroke_style.cap_end = StrokeCap::ArrowLines;
        }
        let inv = start.parent_world.inverse();
````

В `crates/ferrum-editor/src/tools/draw.rs` заменить:

````rust
        let (w, h) = match &self.shape {
            Shape::Image { width, height, .. } => (*width, *height),
            Shape::Line => (CLICK_SIZE, 0.0),
            _ => (CLICK_SIZE, CLICK_SIZE),
        };
````

на:

````rust
        let (w, h) = match &self.shape {
            Shape::Image { width, height, .. } => (*width, *height),
            Shape::Line | Shape::Arrow => (CLICK_SIZE, 0.0),
            _ => (CLICK_SIZE, CLICK_SIZE),
        };
````

В `crates/ferrum-editor/src/tools/draw.rs` заменить:

````rust
            Shape::Ellipse => ToolKind::Ellipse,
            Shape::Line => ToolKind::Line,
            Shape::Image { .. } => ToolKind::Image,
        }
````

на:

````rust
            Shape::Ellipse => ToolKind::Ellipse,
            Shape::Line => ToolKind::Line,
            Shape::Arrow => ToolKind::Arrow,
            Shape::Image { .. } => ToolKind::Image,
        }
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    /// «Перо» (P): векторные пути (сеанс правки вектора, `vector_edit`).
    Pen,
}

````

на:

````rust
    /// «Перо» (P): векторные пути (сеанс правки вектора, `vector_edit`).
    Pen,
    /// «Стрелка» (Shift+L): линия со стрелкой на конце.
    Arrow,
}

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        Tool::Text => ToolKind::Text,
        Tool::Pen => ToolKind::Pen,
    }
}
````

на:

````rust
        Tool::Text => ToolKind::Text,
        Tool::Pen => ToolKind::Pen,
        Tool::Arrow => ToolKind::Arrow,
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        ToolKind::Text => Tool::Text,
        ToolKind::Pen => Tool::Pen,
    }
}
````

на:

````rust
        ToolKind::Text => Tool::Text,
        ToolKind::Pen => Tool::Pen,
        ToolKind::Arrow => Tool::Arrow,
    }
}
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    /// Перо (подсистема 3, этап 3).
    Pen,
}

````

на:

````rust
    /// Перо (подсистема 3, этап 3).
    Pen,
    /// Стрелка: линия со стрелкой на конце.
    Arrow,
}

````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Expected: PASS: `flatten` — 5 тестов, `keymap` — 48 команд; все тесты рабочего пространства зелёные

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/flatten.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/panel.rs crates/ferrum-editor/src/tools/draw.rs crates/ferrum-editor/src/tools/mod.rs crates/ferrum-editor/tests/flatten.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/protocol.rs
git commit -m "feat(editor): «Сплющить» (Ctrl/Cmd+E), инструмент «Стрелка» (Shift+L), стиль обводки из панели

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 8: Протокол и ядро: стиль обводки

Спека 8 и ядро §4.3: правка окончаний, соединения и предела угла из панели и сводка для неё; недоверенный ввод — известные значения перечислений, предел 1…100 (вне — ошибка «Предел острого угла — от 1 до 100»). Сводка показывает окончания только при открытых концах (линия, открытый путь вектора), соединение — при углах.

**Files:**
- Modify: `crates/ferrum-wasm/src/{protocol,summary}.rs`, `crates/ferrum-wasm/src/core/panels.rs`
- Test: `crates/ferrum-wasm/tests/vector.rs` (новый)

**Interfaces:**
- Consumes: `PanelEdit::StrokeStyle` задачи 7.
- Produces: `StrokeCapDto`, `StrokeJoinDto`, `StrokeStyleDto { ends, corners, cap_start, cap_end, join, miter_limit: Num }`, `Summary::stroke_style`, `Edit::StrokeStyle { cap_start, cap_end, join, miter_limit }`, `summary::{cap_dto, cap_of, join_dto, join_of}`; TypeScript — те же типы (tsify).

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-wasm/tests/vector.rs`:

````rust
//! Векторы в ядре worker'а (подсистема 3, этап 3): инструмент «Перо» и курсор,
//! строка слоя вектора, сводка стиля обводки, правка окончаний и соединения из
//! панели с проверкой недоверенного ввода.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
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

fn click(c: &mut Core, x: f64, y: f64, t: f64) {
    let e = PointerEvent {
        point: (x, y).into(),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t,
    };
    c.handle(Input::PointerDown(e));
    c.handle(Input::PointerUp(e));
}

fn key(c: &mut Core, code: &str) {
    c.handle(Input::KeyDown(KeyEvent::new(code, Modifiers::NONE)));
    c.handle(Input::KeyUp(KeyEvent::new(code, Modifiers::NONE)));
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

fn style(edit: Edit) -> Command {
    Command::EditSelection {
        edit,
        phase: EditPhase::Commit,
        ids: None,
        selection_version: None,
    }
}

/// Открытый путь пером из трёх точек, выделен.
fn pen_path(c: &mut Core) {
    c.command(Command::SetTool { tool: Tool::Pen });
    click(c, 100.0, 100.0, 1000.0);
    click(c, 200.0, 100.0, 2000.0);
    click(c, 200.0, 200.0, 3000.0);
    key(c, "Enter");
}

#[test]
fn pen_makes_a_vector_layer_and_its_stroke_style_summary() {
    let mut c = core();
    c.command(Command::SetTool { tool: Tool::Pen });
    let ev = c.take_events();
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Tool { tool: Tool::Pen })));
    assert!(ev.iter().any(|e| matches!(
        e,
        CoreEvent::Cursor {
            cursor: CursorKind::Pen,
            ..
        }
    )));
    pen_path(&mut c);
    let ev = c.take_events();
    let s = summary(&ev).expect("сводка");
    assert_eq!(s.kind, Some(NodeType::Vector));
    let st = s.stroke_style.expect("стиль обводки");
    assert!(st.ends && st.corners);
    assert_eq!(st.cap_start, Some(StrokeCapDto::None));
    assert_eq!(st.join, Some(StrokeJoinDto::Miter));
    assert_eq!(st.miter_limit.value, 4.0);
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Tool { tool: Tool::Move })));
}

#[test]
fn stroke_style_edit_applies_and_bad_values_are_refused() {
    let mut c = core();
    pen_path(&mut c);
    c.take_events();
    c.command(style(Edit::StrokeStyle {
        cap_start: None,
        cap_end: Some(StrokeCapDto::ArrowTriangle),
        join: Some(StrokeJoinDto::Round),
        miter_limit: None,
    }));
    let st = summary(&c.take_events()).unwrap().stroke_style.unwrap();
    assert_eq!(st.cap_end, Some(StrokeCapDto::ArrowTriangle));
    assert_eq!(st.join, Some(StrokeJoinDto::Round));
    for bad in [0.5, 101.0, f64::NAN] {
        c.command(style(Edit::StrokeStyle {
            cap_start: None,
            cap_end: None,
            join: None,
            miter_limit: Some(bad),
        }));
        let ev = c.take_events();
        assert_eq!(
            errors(&ev),
            ["Неверная правка: Предел острого угла — от 1 до 100"],
            "{bad}"
        );
    }
}

#[test]
fn stroke_style_protocol_shape() {
    let json = serde_json::json!({
        "type": "editSelection",
        "edit": {"type": "strokeStyle", "capEnd": "arrowLines", "miterLimit": 8},
        "phase": "commit",
        "ids": null,
        "selectionVersion": null
    });
    let cmd: Command = serde_json::from_value(json).unwrap();
    assert!(matches!(
        cmd,
        Command::EditSelection {
            edit: Edit::StrokeStyle {
                cap_end: Some(StrokeCapDto::ArrowLines),
                cap_start: None,
                ..
            },
            ..
        }
    ));
    // Неизвестное окончание и лишнее поле — ошибка разбора.
    for bad in [
        serde_json::json!({"type": "strokeStyle", "capEnd": "arrow"}),
        serde_json::json!({"type": "strokeStyle", "dash": 2}),
    ] {
        assert!(
            serde_json::from_value::<Edit>(bad.clone()).is_err(),
            "{bad}"
        );
    }
    for tool in ["pen", "arrow"] {
        let t: Tool = serde_json::from_value(serde_json::json!(tool)).unwrap();
        assert_eq!(serde_json::to_value(t).unwrap(), serde_json::json!(tool));
    }
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-wasm --test vector`
Expected: FAIL — не собирается: нет `Edit::StrokeStyle`, `StrokeCapDto`, `StrokeJoinDto`, поля `Summary::stroke_style`

- [ ] **Step 3: Реализация**

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
    EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{summarize, MAX_PAINTS};

/// Наибольший размер картинки из буфера обмена (`PasteImage`).
````

на:

````rust
    EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{cap_of, join_of, summarize, MAX_PAINTS};

/// Наибольший размер картинки из буфера обмена (`PasteImage`).
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            }
            PanelEdit::Size { w: *w, h: *h }
        }
    })
````

на:

````rust
            }
            PanelEdit::Size { w: *w, h: *h }
        }
        Edit::StrokeStyle {
            cap_start,
            cap_end,
            join,
            miter_limit,
        } => {
            // Недоверенный ввод: предел вне диапазона — отказ, а не подгонка
            // (панель зажимает значения сама, `limits.ts`).
            if let Some(m) = miter_limit {
                if !(m.is_finite() && ferrum_doc::MITER_LIMIT_RANGE.contains(m)) {
                    return Err("Предел острого угла — от 1 до 100".into());
                }
            }
            PanelEdit::StrokeStyle {
                cap_start: cap_start.map(cap_of),
                cap_end: cap_end.map(cap_of),
                join: join.map(join_of),
                miter_limit: *miter_limit,
            }
        }
    })
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
pub use crate::layers::{LayerRow, NodeType};
pub use crate::summary::{
    Align, FillDto, Flag, Num, PaintKind, Paints, ScaleModeDto, StrokeDto, Summary,
};

````

на:

````rust
pub use crate::layers::{LayerRow, NodeType};
pub use crate::summary::{
    Align, FillDto, Flag, Num, PaintKind, Paints, ScaleModeDto, StrokeCapDto, StrokeDto,
    StrokeJoinDto, StrokeStyleDto, Summary,
};

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        w: f64,
        h: f64,
    },
}
````

на:

````rust
        w: f64,
        h: f64,
    },
    /// Окончания, соединение и предел острого угла; без поля — не трогать.
    StrokeStyle {
        #[serde(default)]
        cap_start: Option<StrokeCapDto>,
        #[serde(default)]
        cap_end: Option<StrokeCapDto>,
        #[serde(default)]
        join: Option<StrokeJoinDto>,
        #[serde(default)]
        miter_limit: Option<f64>,
    },
}
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
//! выделения или документа, а не в каждом кадре: O(выделенных).

use ferrum_doc::{Fill, FillKind, NodeId, NodeKind, ScaleMode, Stroke, StrokeAlign};
use ferrum_editor::{field_applies, panel_pose, NumField};
use ferrum_scene::Scene;
````

на:

````rust
//! выделения или документа, а не в каждом кадре: O(выделенных).

use ferrum_doc::{
    Fill, FillKind, NodeId, NodeKind, ScaleMode, Stroke, StrokeAlign, StrokeCap, StrokeJoin,
};
use ferrum_editor::{field_applies, panel_pose, NumField};
use ferrum_scene::Scene;
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
}

/// Заливки или обводки выделения: список, если у всех одинаковый.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

на:

````rust
}

/// Окончание открытого конца обводки (спека подсистемы 3, раздел 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum StrokeCapDto {
    None,
    Round,
    Square,
    ArrowLines,
    ArrowTriangle,
    Circle,
    Diamond,
}

/// Соединение сегментов обводки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum StrokeJoinDto {
    Miter,
    Round,
    Bevel,
}

pub fn cap_dto(c: StrokeCap) -> StrokeCapDto {
    match c {
        StrokeCap::None => StrokeCapDto::None,
        StrokeCap::Round => StrokeCapDto::Round,
        StrokeCap::Square => StrokeCapDto::Square,
        StrokeCap::ArrowLines => StrokeCapDto::ArrowLines,
        StrokeCap::ArrowTriangle => StrokeCapDto::ArrowTriangle,
        StrokeCap::Circle => StrokeCapDto::Circle,
        StrokeCap::Diamond => StrokeCapDto::Diamond,
    }
}

pub fn cap_of(c: StrokeCapDto) -> StrokeCap {
    match c {
        StrokeCapDto::None => StrokeCap::None,
        StrokeCapDto::Round => StrokeCap::Round,
        StrokeCapDto::Square => StrokeCap::Square,
        StrokeCapDto::ArrowLines => StrokeCap::ArrowLines,
        StrokeCapDto::ArrowTriangle => StrokeCap::ArrowTriangle,
        StrokeCapDto::Circle => StrokeCap::Circle,
        StrokeCapDto::Diamond => StrokeCap::Diamond,
    }
}

pub fn join_dto(j: StrokeJoin) -> StrokeJoinDto {
    match j {
        StrokeJoin::Miter => StrokeJoinDto::Miter,
        StrokeJoin::Round => StrokeJoinDto::Round,
        StrokeJoin::Bevel => StrokeJoinDto::Bevel,
    }
}

pub fn join_of(j: StrokeJoinDto) -> StrokeJoin {
    match j {
        StrokeJoinDto::Miter => StrokeJoin::Miter,
        StrokeJoinDto::Round => StrokeJoin::Round,
        StrokeJoinDto::Bevel => StrokeJoin::Bevel,
    }
}

/// Стиль обводки выделения: значение поля — общее у всех, `None` — «Смешанные».
/// Окончания показываются, только если у кого-то есть открытые концы (линия,
/// открытый путь вектора), соединение — если есть углы.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct StrokeStyleDto {
    pub ends: bool,
    pub corners: bool,
    pub cap_start: Option<StrokeCapDto>,
    pub cap_end: Option<StrokeCapDto>,
    pub join: Option<StrokeJoinDto>,
    pub miter_limit: Num,
}

/// Заливки или обводки выделения: список, если у всех одинаковый.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
    pub fills: Option<Paints<FillDto>>,
    pub strokes: Option<Paints<StrokeDto>>,
}

````

на:

````rust
    pub fills: Option<Paints<FillDto>>,
    pub strokes: Option<Paints<StrokeDto>>,
    /// Окончания и соединения обводки (у групп и текста их нет).
    pub stroke_style: Option<StrokeStyleDto>,
}

````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
            fills: None,
            strokes: None,
        }
    }
````

на:

````rust
            fills: None,
            strokes: None,
            stroke_style: None,
        }
    }
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
    let mut fills: Acc<&[Fill]> = Acc::default();
    let mut strokes: Acc<&[Stroke]> = Acc::default();
    let mut name = None;
    for id in ids {
````

на:

````rust
    let mut fills: Acc<&[Fill]> = Acc::default();
    let mut strokes: Acc<&[Stroke]> = Acc::default();
    let mut cap_start: Acc<StrokeCap> = Acc::default();
    let mut cap_end: Acc<StrokeCap> = Acc::default();
    let mut join: Acc<StrokeJoin> = Acc::default();
    let mut miter: Acc<f64> = Acc::default();
    let (mut ends, mut corners) = (false, false);
    let mut name = None;
    for id in ids {
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
        if p.kind != NodeKind::Group {
            strokes.add_by(&p.strokes, |a, b| strokes_eq(a, b));
        }
        if count == 1 {
````

на:

````rust
        if p.kind != NodeKind::Group {
            strokes.add_by(&p.strokes, |a, b| strokes_eq(a, b));
        }
        if !matches!(p.kind, NodeKind::Group | NodeKind::Text) {
            let s = &p.stroke_style;
            join.add(s.join);
            miter.add_num(s.miter_limit);
            match p.kind {
                NodeKind::Line => {
                    ends = true;
                    cap_start.add(s.cap_start);
                    cap_end.add(s.cap_end);
                }
                NodeKind::Vector => {
                    let shape = ferrum_scene::vector::network_shape(p);
                    corners = true;
                    if !shape.ends.is_empty() {
                        ends = true;
                        cap_start.add(s.cap_start);
                        cap_end.add(s.cap_end);
                    }
                }
                NodeKind::Ellipse => {}
                _ => corners |= p.corner_radius <= 0.0,
            }
        }
        if count == 1 {
````

В `crates/ferrum-wasm/src/summary.rs` заменить:

````rust
            }
        }),
    }
}
````

на:

````rust
            }
        }),
        stroke_style: join.first.map(|j| StrokeStyleDto {
            ends,
            corners,
            cap_start: cap_start.first.filter(|_| !cap_start.mixed).map(cap_dto),
            cap_end: cap_end.first.filter(|_| !cap_end.mixed).map(cap_dto),
            join: (!join.mixed).then(|| join_dto(j)),
            miter_limit: miter.num().unwrap_or(Num {
                value: ferrum_doc::DEFAULT_MITER_LIMIT,
                mixed: false,
            }),
        }),
    }
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-wasm 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npm run wasm && npm run typecheck`

Expected: PASS: `vector` ядра — 3 теста; wasm собран, `npm run typecheck` без ошибок (новые типы tsify)

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/src/summary.rs crates/ferrum-wasm/tests/vector.rs
git commit -m "feat(wasm): правка окончаний и соединения обводки из панели с проверкой ввода, сводка стиля обводки, инструменты «Перо» и «Стрелка» в протоколе

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 9: Интерфейс: перо и стрелка в тулбаре, строки секции «Обводка»

Кнопка «Перо» вместо отключённой, «Стрелка» в меню фигур, строки окончаний, соединения и предела угла под обводками (значение предела зажимается `MIN_MITER_LIMIT`…`MAX_MITER_LIMIT` из `limits.ts`, тест сверяет их с `MITER_LIMIT_RANGE` ядра). E2E: перо одним шагом и отмена (и Firefox), правка вектора двойным щелчком, «Сплющить», окончание «Стрелка» и соединение в секции, инструмент «Стрелка». Прежние E2E, которые знали отключённое перо, четыре фигуры, 45 команд и «неизвестную» команду `tool.pen`, правятся; двойной щелчок по прямоугольнику теперь входит в его правку, поэтому тест живых значений ждёт порога двойного щелчка.

**Files:**
- Modify: `apps/web/src/ui/{toolbar,paints,icons}.tsx`, `apps/web/src/core/limits.ts`
- Test: `apps/web/e2e/vectors.spec.ts` (новый), `apps/web/e2e/{app,chrome,properties,settings}.spec.ts`, `apps/web/unit/limits.test.ts`

**Interfaces:**
- Consumes: типы TypeScript задачи 8 (`StrokeCapDto`, `StrokeJoinDto`, `Summary.strokeStyle`, `Edit` с `strokeStyle`), `Tool` с `pen` и `arrow`.
- Produces: `MIN_MITER_LIMIT`, `MAX_MITER_LIMIT`; иконка `arrow`; подписи полей «Окончание в начале», «Окончание в конце», «Соединение», «Предел острого угла».

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 45, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 48, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  await toolbar.getByRole('button', { name: 'Фигуры' }).click();
  const menu = page.getByRole('menu', { name: 'Фигуры' });
  await expect(menu.getByRole('menuitemcheckbox')).toHaveCount(4);
  await menu.getByRole('menuitemcheckbox', { name: /Эллипс/ }).click();
  await expect.poll(() => ui(page, (s) => s.tool)).toBe('ellipse');
````

на:

````ts
  await toolbar.getByRole('button', { name: 'Фигуры' }).click();
  const menu = page.getByRole('menu', { name: 'Фигуры' });
  // Прямоугольник, эллипс, линия, стрелка, изображение.
  await expect(menu.getByRole('menuitemcheckbox')).toHaveCount(5);
  await menu.getByRole('menuitemcheckbox', { name: /Эллипс/ }).click();
  await expect.poll(() => ui(page, (s) => s.tool)).toBe('ellipse');
````

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  await move.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  V');
  await page.getByRole('button', { name: 'Перо — подсистема 3' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перо  P — подсистема 3');
  // Открытая подсказка следует за раскладкой: V снят, Q назначен — текст меняется на месте.
  await move.hover();
````

на:

````ts
  await move.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  V');
  await page.getByRole('button', { name: 'Перо', exact: true }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перо  P');
  await page.getByRole('button', { name: 'Комментарии — подсистема 4' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Комментарии  C — подсистема 4');
  // Открытая подсказка следует за раскладкой: V снят, Q назначен — текст меняется на месте.
  await move.hover();
````

В `apps/web/e2e/properties.spec.ts` заменить:

````ts
  const x = field(page, 'X');
  await expect(x).toHaveValue('140');
  await page.mouse.down();
  await page.mouse.move(at.x + 30, at.y, { steps: 6 });
````

на:

````ts
  const x = field(page, 'X');
  await expect(x).toHaveValue('140');
  // Второе нажатие позже порога двойного щелчка (500 мс): двойной щелчок по
  // прямоугольнику — его правка как вектора.
  await page.waitForTimeout(600);
  await page.mouse.down();
  await page.mouse.move(at.x + 30, at.y, { steps: 6 });
````

В `apps/web/e2e/settings.spec.ts` заменить:

````ts
  const mod = await page.evaluate(() => (navigator.platform.toLowerCase().startsWith('mac') ? 'Meta' : 'Ctrl'));
  const dialog = await openSettings(page);
  const json = JSON.stringify({ format: 'ferrum-keymap', version: 1, overrides: { 'edit.duplicate': [`${mod}+KeyW`], 'tool.pen': ['KeyP'] } });
  await dialog.locator('input[type=file]').setInputFiles({ name: 'keys.json', mimeType: 'application/json', buffer: Buffer.from(json) });
  await expect(page.getByRole('status').filter({ hasText: /Раскладка импортирована, пропущено: 1/ })).toBeVisible();
````

на:

````ts
  const mod = await page.evaluate(() => (navigator.platform.toLowerCase().startsWith('mac') ? 'Meta' : 'Ctrl'));
  const dialog = await openSettings(page);
  const json = JSON.stringify({ format: 'ferrum-keymap', version: 1, overrides: { 'edit.duplicate': [`${mod}+KeyW`], 'tool.scale': ['KeyK'] } });
  await dialog.locator('input[type=file]').setInputFiles({ name: 'keys.json', mimeType: 'application/json', buffer: Buffer.from(json) });
  await expect(page.getByRole('status').filter({ hasText: /Раскладка импортирована, пропущено: 1/ })).toBeVisible();
````

Создать `apps/web/e2e/vectors.spec.ts`:

````ts
// Векторы (подсистема 3, этап 3): перо на холсте одним шагом, правка вектора
// двойным щелчком (точка тянется, Esc — выход), «Сплющить» двух прямоугольников,
// окончания и соединение в секции «Обводка», инструмент «Стрелка». Документ тест
// читает сводкой панели свойств и строками слоёв, вид — пикселями экрана.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, live, liveWhen, openApp, pixel } from './helpers';

// Чтений layout вне фазы measure планировщика не было (тестовый режим их копит).
test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

const summary = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)));
const tool = (page: Page) => page.evaluate(() => window.__ferrum!.ui!.tool);
const rows = (page: Page) => page.getByRole('tree', { name: 'Слои' }).getByRole('treeitem');

/** Пустой файл на белом фоне при зуме 100 % (пиксель экрана — единица мира), фокус на холсте. */
async function blank(page: Page): Promise<void> {
  await openApp(page, 0);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setPageBackground', color: '#ffffff', phase: 'commit' }));
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+Digit0');
  await liveWhen(page, 'l.zoom === 1');
}

/** Тёмных пикселей в прямоугольнике экрана (обводки чёрные на белом фоне). */
async function darkIn(page: Page, at: { x: number; y: number }, w: number, h: number): Promise<number> {
  const png = (await page.screenshot({ clip: { x: at.x, y: at.y, width: w, height: h } })).toString('base64');
  return page.evaluate(async (png) => {
    const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    const ctx = c.getContext('2d')!;
    ctx.drawImage(bmp, 0, 0);
    const d = ctx.getImageData(0, 0, bmp.width, bmp.height).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] < 160) n++;
    return n;
  }, png);
}

/** Щелчки пером по точкам мира и Enter; ждёт записанный вектор. */
async function penPath(page: Page, points: [number, number][]): Promise<void> {
  const before = await live(page);
  await page.keyboard.press('KeyP');
  await expect.poll(() => tool(page)).toBe('pen');
  for (const [x, y] of points) {
    const at = await appScreen(page, before, x, y);
    await page.mouse.click(at.x, at.y);
  }
  await page.keyboard.press('Enter');
  await liveWhen(page, `l.nodes === ${before.nodes + 1} && l.steps === ${before.steps + 1}`);
}

test('перо: щелчки и Enter — вектор одним шагом, отмена убирает @firefox', async ({ page }) => {
  await blank(page);
  const l = await live(page);
  await penPath(page, [
    [l.cameraX - 100, l.cameraY - 50],
    [l.cameraX + 100, l.cameraY - 50],
    [l.cameraX + 100, l.cameraY + 50],
  ]);
  await expect.poll(() => tool(page)).toBe('move');
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toHaveAccessibleName('Вектор');
  const s = await summary(page);
  expect([s.kind, s.w.value, s.h.value]).toEqual(['vector', 200, 100]);
  // Отрезок нарисован (под рамкой выделения тоже): у верхней стороны вдали от ручек
  // есть тёмные пиксели обводки и рамки.
  const mid = await appScreen(page, l, l.cameraX - 53, l.cameraY - 53);
  await expect.poll(() => darkIn(page, mid, 6, 6)).toBeGreaterThan(0);
  await page.keyboard.press('ControlOrMeta+z');
  await liveWhen(page, `l.nodes === ${l.nodes}`);
});

test('правка вектора: двойной щелчок, точка тянется одним шагом, Esc — выход', async ({ page }) => {
  await blank(page);
  const l = await live(page);
  const [x0, y0] = [l.cameraX - 100, l.cameraY - 50];
  await penPath(page, [
    [x0, y0],
    [x0 + 200, y0],
  ]);
  const mid = await appScreen(page, l, x0 + 100, y0);
  await page.mouse.dblclick(mid.x, mid.y);
  // Правый конец (x0 + 200, y0) — вниз на 80.
  const before = await live(page);
  const end = await appScreen(page, l, x0 + 200, y0);
  await drag(page, end, 0, 80);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect.poll(async () => (await summary(page)).h.value).toBe(80);
  await page.keyboard.press('Escape');
  // Вне правки Delete удаляет узел целиком (в правке — точки).
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes}`);
});

test('«Сплющить»: два прямоугольника — один вектор', async ({ page }) => {
  await blank(page);
  const l = await live(page);
  for (const dx of [-150, 50]) {
    await page.keyboard.press('KeyR');
    await drag(page, await appScreen(page, l, l.cameraX + dx, l.cameraY - 40), 100, 80);
    await liveWhen(page, `l.busy === 0`);
  }
  await expect(rows(page)).toHaveCount(2);
  await page.keyboard.press('ControlOrMeta+a');
  const before = await live(page);
  await page.keyboard.press('ControlOrMeta+e');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect(rows(page)).toHaveCount(1);
  const s = await summary(page);
  expect([s.kind, s.w.value, s.h.value]).toEqual(['vector', 300, 80]);
  // Обе заливки на месте: серый внутри каждого прямоугольника.
  for (const dx of [-100, 100]) {
    const at = await appScreen(page, l, l.cameraX + dx, l.cameraY);
    await expect.poll(async () => (await pixel(page, at.x, at.y))[0]).toBe(217);
  }
});

test('«Обводка»: окончание «Стрелка» у линии, соединение; инструмент «Стрелка»', async ({ page }) => {
  await blank(page);
  const l = await live(page);
  await page.keyboard.press('KeyL');
  const from = await appScreen(page, l, l.cameraX - 100, l.cameraY);
  await drag(page, from, 200, 0);
  await liveWhen(page, `l.busy === 0 && l.nodes === ${l.nodes + 1}`);
  const end = page.getByRole('combobox', { name: 'Окончание в конце' });
  await expect(end).toHaveValue('none');
  // Под концом линии пусто (ручка конца — правее): плечо стрелки появится там.
  const box = await appScreen(page, l, l.cameraX + 100 - 10, l.cameraY + 2);
  expect(await darkIn(page, box, 4, 5)).toBe(0);
  // Соединения у линии нет (углов нет).
  await expect(page.getByRole('combobox', { name: 'Соединение' })).toHaveCount(0);
  const before = await live(page);
  await end.selectOption('arrowLines');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect.poll(async () => (await summary(page)).strokeStyle.capEnd).toBe('arrowLines');
  // Плечо стрелки длиной 9 под 30° назад: (−7,8; +4,5) от конца линии.
  await expect.poll(() => darkIn(page, box, 4, 5)).toBeGreaterThan(0);
  // «Стрелка» (Shift+L) рисует линию со стрелкой сразу.
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+KeyL');
  await expect.poll(() => tool(page)).toBe('arrow');
  await drag(page, await appScreen(page, l, l.cameraX - 100, l.cameraY + 100), 200, 0);
  await liveWhen(page, `l.busy === 0 && l.nodes === ${l.nodes + 2}`);
  await expect.poll(async () => (await summary(page)).strokeStyle.capEnd).toBe('arrowLines');
  // Прямоугольник: соединение есть, окончаний нет; «Круглое» прячет предел угла.
  await page.keyboard.press('KeyR');
  await drag(page, await appScreen(page, l, l.cameraX - 50, l.cameraY + 150), 100, 60);
  await liveWhen(page, `l.busy === 0 && l.nodes === ${l.nodes + 3}`);
  await expect(page.getByRole('combobox', { name: 'Окончание в конце' })).toHaveCount(0);
  await expect(page.getByRole('combobox', { name: 'Соединение' })).toHaveCount(0);
  // У прямоугольника без обводки строк стиля нет: добавить обводку.
  await page.getByRole('button', { name: 'Добавить обводку' }).click();
  const join = page.getByRole('combobox', { name: 'Соединение' });
  await expect(join).toHaveValue('miter');
  await expect(page.getByRole('textbox', { name: 'Предел острого угла' })).toBeVisible();
  await join.selectOption('round');
  await expect.poll(async () => (await summary(page)).strokeStyle.join).toBe('round');
  await expect(page.getByRole('textbox', { name: 'Предел острого угла' })).toHaveCount(0);
});

test('перо в тулбаре и подсказка', async ({ page }) => {
  await blank(page);
  const pen = page.getByRole('button', { name: 'Перо', exact: true });
  await pen.click();
  await expect(pen).toHaveAttribute('aria-pressed', 'true');
  await page.locator('canvas').focus();
  await page.keyboard.press('Escape');
  await expect(pen).toHaveAttribute('aria-pressed', 'false');
});
````

В `apps/web/unit/limits.test.ts` заменить:

````ts
  MAX_LETTER_SPACING_PERCENT,
  MAX_LINE_HEIGHT_PERCENT,
  MAX_NAME_CHARS,
  MIN_FONT_SIZE,
  MAX_SETTINGS_BYTES,
````

на:

````ts
  MAX_LETTER_SPACING_PERCENT,
  MAX_LINE_HEIGHT_PERCENT,
  MAX_MITER_LIMIT,
  MAX_NAME_CHARS,
  MIN_MITER_LIMIT,
  MIN_FONT_SIZE,
  MAX_SETTINGS_BYTES,
````

В `apps/web/unit/limits.test.ts` заменить:

````ts
  expect(text).toContain(`finite_in(d.value, 0.0, ${MAX_LINE_HEIGHT_PERCENT}.0`.replace(/(\d)(?=(\d{3})+\.)/g, '$1_'));
  expect(text).toContain(`finite_in(d.value, -${MAX_LETTER_SPACING_PERCENT}.0, ${MAX_LETTER_SPACING_PERCENT}.0`);
  // Десктоп отдаёт список системных шрифтов уже в этих пределах.
  const desktop = '../../desktop/src-tauri/src/fonts.rs';
````

на:

````ts
  expect(text).toContain(`finite_in(d.value, 0.0, ${MAX_LINE_HEIGHT_PERCENT}.0`.replace(/(\d)(?=(\d{3})+\.)/g, '$1_'));
  expect(text).toContain(`finite_in(d.value, -${MAX_LETTER_SPACING_PERCENT}.0, ${MAX_LETTER_SPACING_PERCENT}.0`);
  // Предел острого угла обводки (подсистема 3, этап 3).
  expect(read(`${CRATES}ferrum-doc/src/vector.rs`)).toContain(
    `MITER_LIMIT_RANGE: std::ops::RangeInclusive<f64> = ${MIN_MITER_LIMIT}.0..=${MAX_MITER_LIMIT}.0;`,
  );
  // Десктоп отдаёт список системных шрифтов уже в этих пределах.
  const desktop = '../../desktop/src-tauri/src/fonts.rs';
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npx playwright test -c playwright.unit.config.ts unit/limits.test.ts`
Expected: FAIL — модуль не собирается: нет экспортов `MIN_MITER_LIMIT`, `MAX_MITER_LIMIT` в `src/core/limits.ts`

- [ ] **Step 3: Реализация**

В `apps/web/src/core/limits.ts` заменить:

````ts
export const MAX_LETTER_SPACING_PERCENT = 1000;

/** Мегабайты с округлением вверх: «предел 64 МБ, получено 64 МБ» сбивало бы с толку. */
export const mb = (bytes: number): number => Math.ceil(bytes / MIB);
````

на:

````ts
export const MAX_LETTER_SPACING_PERCENT = 1000;

/** Предел острого угла обводки — как `MITER_LIMIT_RANGE` ядра (`ferrum-doc`): значение вне
 * пределов ядро отклоняет ошибкой, поэтому секция «Обводка» зажимает поле этими числами. */
export const MIN_MITER_LIMIT = 1;
export const MAX_MITER_LIMIT = 100;

/** Мегабайты с округлением вверх: «предел 64 МБ, получено 64 МБ» сбивало бы с толку. */
export const mb = (bytes: number): number => Math.ceil(bytes / MIB);
````

В `apps/web/src/ui/icons.tsx` заменить:

````tsx
  ellipse: '<circle cx="12" cy="12" r="10"/>',
  line: '<path d="M22 2 2 22"/>',
  image: '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
  pen: '<path d="m18 13-.654-4.736a4 4 0 00-2.756-3.266l-9.163-2.9a2 2 0 00-2.017.493l-.82.819a2 2 0 00-.492 2.017l2.9 9.163a4 4 0 003.266 2.756L13 18"/><path d="M18.648 12.352a1.205 1.205 0 011.704 0l1.296 1.296a1.205 1.205 0 010 1.704l-6.296 6.296a1.205 1.205 0 01-1.704 0l-1.296-1.296a1.205 1.205 0 010-1.704z"/><path d="m3 3 6.586 6.586"/><circle cx="11" cy="11" r="2"/>',
````

на:

````tsx
  ellipse: '<circle cx="12" cy="12" r="10"/>',
  line: '<path d="M22 2 2 22"/>',
  arrow: '<path d="M7 17 17 7"/><path d="M7 7h10v10"/>',
  image: '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
  pen: '<path d="m18 13-.654-4.736a4 4 0 00-2.756-3.266l-9.163-2.9a2 2 0 00-2.017.493l-.82.819a2 2 0 00-.492 2.017l2.9 9.163a4 4 0 003.266 2.756L13 18"/><path d="M18.648 12.352a1.205 1.205 0 011.704 0l1.296 1.296a1.205 1.205 0 010 1.704l-6.296 6.296a1.205 1.205 0 01-1.704 0l-1.296-1.296a1.205 1.205 0 010-1.704z"/><path d="m3 3 6.586 6.586"/><circle cx="11" cy="11" r="2"/>',
````

В `apps/web/src/ui/paints.tsx` заменить:

````tsx
// `fills`/`strokes`), палитра — превью во время жеста и один шаг при отпускании.
// Заливка изображением показывается, но не правится (картинку меняет этап 6).
//
// Правка строится от списка и выделения на её начало (`begin`): жест палитры или
````

на:

````tsx
// `fills`/`strokes`), палитра — превью во время жеста и один шаг при отпускании.
// Заливка изображением показывается, но не правится (картинку меняет этап 6).
// Под обводками — окончания открытых концов, соединение и предел острого угла
// (подсистема 3, этап 3): одна правка — одна команда `strokeStyle`.
//
// Правка строится от списка и выделения на её начало (`begin`): жест палитры или
````

В `apps/web/src/ui/paints.tsx` заменить:

````tsx

import { createEffect, createSignal, For, Index, on, Show, untrack } from 'solid-js';
import type { Align, EditPhase, FillDto, StrokeDto } from '../wasm/ferrum_wasm';
import { Section } from './chrome';
import { cssColor, hexHasAlpha, paintOpacity, parseHex, parseOpacityText, toHex, toHex6, type Rgb } from './colour';
````

на:

````tsx

import { createEffect, createSignal, For, Index, on, Show, untrack } from 'solid-js';
import type { Align, EditPhase, FillDto, StrokeCapDto, StrokeDto, StrokeJoinDto } from '../wasm/ferrum_wasm';
import { MAX_MITER_LIMIT, MIN_MITER_LIMIT } from '../core/limits';
import { Section } from './chrome';
import { cssColor, hexHasAlpha, paintOpacity, parseHex, parseOpacityText, toHex, toHex6, type Rgb } from './colour';
````

В `apps/web/src/ui/paints.tsx` заменить:

````tsx

const SCALE_MODES = { fill: 'Заполнить', fit: 'Вписать', tile: 'Плитка' } as const;

/** Цвет из `#rrggbbaa` и прозрачность для панели. */
````

на:

````tsx

const SCALE_MODES = { fill: 'Заполнить', fit: 'Вписать', tile: 'Плитка' } as const;

const CAPS: { value: StrokeCapDto; label: string }[] = [
  { value: 'none', label: 'Нет' },
  { value: 'round', label: 'Круглое' },
  { value: 'square', label: 'Квадратное' },
  { value: 'arrowLines', label: 'Стрелка' },
  { value: 'arrowTriangle', label: 'Треугольник' },
  { value: 'circle', label: 'Круг' },
  { value: 'diamond', label: 'Ромб' },
];

const JOINS: { value: StrokeJoinDto; label: string }[] = [
  { value: 'miter', label: 'Острое' },
  { value: 'round', label: 'Круглое' },
  { value: 'bevel', label: 'Скошенное' },
];

/** Окончания, соединение и предел острого угла — под списком обводок. «Смешанные» —
 * пустой пункт списка. */
function StrokeStyleRows() {
  const ed = useEditor();
  const style = () => ed.store.state.summary?.strokeStyle;
  const send = (edit: {
    capStart?: StrokeCapDto;
    capEnd?: StrokeCapDto;
    join?: StrokeJoinDto;
    miterLimit?: number;
  }) =>
    sendEdit(
      ed.send,
      { type: 'strokeStyle', capStart: edit.capStart, capEnd: edit.capEnd, join: edit.join, miterLimit: edit.miterLimit },
      'commit',
      captureEdit(ed).target,
    );
  return (
    <Show when={style()}>
      {(s) => (
        <>
          <Show when={s().ends}>
            <div class="fe-grid2">
              <label class="fe-field">
                <select
                  class="fe-native-select"
                  aria-label="Окончание в начале"
                  value={s().capStart ?? ''}
                  onChange={(e) => send({ capStart: e.currentTarget.value as StrokeCapDto })}
                >
                  <Show when={!s().capStart}>
                    <option value="">Смешанное</option>
                  </Show>
                  <For each={CAPS}>{(c) => <option value={c.value}>{c.label}</option>}</For>
                </select>
              </label>
              <label class="fe-field">
                <select
                  class="fe-native-select"
                  aria-label="Окончание в конце"
                  value={s().capEnd ?? ''}
                  onChange={(e) => send({ capEnd: e.currentTarget.value as StrokeCapDto })}
                >
                  <Show when={!s().capEnd}>
                    <option value="">Смешанное</option>
                  </Show>
                  <For each={CAPS}>{(c) => <option value={c.value}>{c.label}</option>}</For>
                </select>
              </label>
            </div>
          </Show>
          <Show when={s().corners}>
            <div class="fe-grid2">
              <label class="fe-field">
                <select
                  class="fe-native-select"
                  aria-label="Соединение"
                  value={s().join ?? ''}
                  onChange={(e) => send({ join: e.currentTarget.value as StrokeJoinDto })}
                >
                  <Show when={!s().join}>
                    <option value="">Смешанное</option>
                  </Show>
                  <For each={JOINS}>{(j) => <option value={j.value}>{j.label}</option>}</For>
                </select>
              </label>
              <Show when={s().join !== 'round' && s().join !== 'bevel'}>
                <NumberField
                  label="∠"
                  name="Предел острого угла"
                  value={s().miterLimit.value}
                  mixed={s().miterLimit.mixed}
                  min={MIN_MITER_LIMIT}
                  max={MAX_MITER_LIMIT}
                  onEdit={(input, phase, target) => {
                    const cur = s().miterLimit.value;
                    const raw = input.kind === 'set' ? input.value : cur + input.delta;
                    const miterLimit = Math.min(MAX_MITER_LIMIT, Math.max(MIN_MITER_LIMIT, raw));
                    sendEdit(ed.send, { type: 'strokeStyle', miterLimit, capStart: undefined, capEnd: undefined, join: undefined }, phase, target);
                  }}
                />
              </Show>
            </div>
          </Show>
        </>
      )}
    </Show>
  );
}

/** Цвет из `#rrggbbaa` и прозрачность для панели. */
````

В `apps/web/src/ui/paints.tsx` заменить:

````tsx
            }}
          </For>
          <Show when={picker()}>
            {(pk) => {
````

на:

````tsx
            }}
          </For>
          <Show when={p().mixed || list().length > 0}>
            <StrokeStyleRows />
          </Show>
          <Show when={picker()}>
            {(pk) => {
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
// Плавающий тулбар внизу по центру холста (раскладка UI3): перемещение, фрейм с
// пресетами, фигуры (последняя выбранная на кнопке), отключённые перо, текст и
// комментарии — с подсказкой, когда они появятся. Рука (H, удержание Space)
// работает с клавиатуры и в тулбар не выносится.

import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
````

на:

````tsx
// Плавающий тулбар внизу по центру холста (раскладка UI3): перемещение, фрейм с
// пресетами, фигуры со стрелкой (последняя выбранная на кнопке), перо, текст и
// отключённые комментарии — с подсказкой, когда они появятся. Рука (H, удержание
// Space) работает с клавиатуры и в тулбар не выносится.

import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
import { roving } from './roving';

type Shape = 'rect' | 'ellipse' | 'line' | 'image';

const SHAPES: { tool: Shape; name: string; command: string; icon: IconName }[] = [
````

на:

````tsx
import { roving } from './roving';

type Shape = 'rect' | 'ellipse' | 'line' | 'arrow' | 'image';

const SHAPES: { tool: Shape; name: string; command: string; icon: IconName }[] = [
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
  { tool: 'ellipse', name: 'Эллипс', command: 'tool.ellipse', icon: 'ellipse' },
  { tool: 'line', name: 'Линия', command: 'tool.line', icon: 'line' },
  { tool: 'image', name: 'Изображение', command: 'tool.image', icon: 'image' },
];

const LATER: { name: string; key: string; when: string; icon: IconName }[] = [
  { name: 'Перо', key: 'P', when: 'подсистема 3', icon: 'pen' },
  { name: 'Комментарии', key: 'C', when: 'подсистема 4', icon: 'comment' },
];
````

на:

````tsx
  { tool: 'ellipse', name: 'Эллипс', command: 'tool.ellipse', icon: 'ellipse' },
  { tool: 'line', name: 'Линия', command: 'tool.line', icon: 'line' },
  { tool: 'arrow', name: 'Стрелка', command: 'tool.arrow', icon: 'arrow' },
  { tool: 'image', name: 'Изображение', command: 'tool.image', icon: 'image' },
];

const LATER: { name: string; key: string; when: string; icon: IconName }[] = [
  { name: 'Комментарии', key: 'C', when: 'подсистема 4', icon: 'comment' },
];
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
      <button
        class="fe-tool"
        aria-pressed={tool() === 'text'}
        aria-label="Текст"
````

на:

````tsx
      <button
        class="fe-tool"
        aria-pressed={tool() === 'pen'}
        aria-label="Перо"
        data-tip="Перо"
        data-command="tool.pen"
        data-tip-side="top"
        onClick={() => setTool('pen')}
      >
        <Icon name="pen" />
      </button>
      <button
        class="fe-tool"
        aria-pressed={tool() === 'text'}
        aria-label="Текст"
````

- [ ] **Step 4: Проверить**

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npm run wasm && npm run typecheck && npm run unit && npm run build`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npx playwright test e2e/vectors.spec.ts e2e/app.spec.ts e2e/chrome.spec.ts e2e/properties.spec.ts e2e/settings.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npx playwright test e2e/vectors.spec.ts --project=firefox`

Expected: модульные — 134 passed; E2E `vectors` (5), `app`, `chrome`, `properties`, `settings` в Chromium — passed; Firefox — 1 passed (тест с @firefox)

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts apps/web/e2e/chrome.spec.ts apps/web/e2e/properties.spec.ts apps/web/e2e/settings.spec.ts apps/web/e2e/vectors.spec.ts apps/web/src/core/limits.ts apps/web/src/ui/icons.tsx apps/web/src/ui/paints.tsx apps/web/src/ui/toolbar.tsx apps/web/unit/limits.test.ts
git commit -m "feat(web): перо и стрелка в тулбаре, окончания, соединение и предел угла в секции «Обводка»; E2E векторов

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 10: Вектор из 100 000 точек в бюджете кадра, спека и финальная проверка

Спека 12 и задача этапа: сеть из 100 000 точек рисуется и правится в бюджете кадра. Без упрощения кадр такой сети стоил ≈ 30 мс (заливка контура обводки в 800 000 элементов), первая постройка контура — ≈ 250 мс, правка — ≈ 270 мс. Поэтому: контуры больших сетей упрощаются под масштаб (`simplify`, кэш списка помнит точность `detail`), выступ обводки считается по степеням точек без построения контуров, связи точек — одним массивом (`Incidence`), оверлей правки продолжает путь без лишних подпутей и упрощает его. Замеры — в `render_report` (первый кадр, кадр, правка точки, кадр правки с оверлеем) и `editor_report` (кадр жеста перетаскивания точки, запись, один шаг). Спека получает решения этапа 3.

**Files:**
- Modify: `crates/ferrum-scene/src/{vector,shape}.rs`, `crates/ferrum-render/src/{display,draw,overlay}.rs`, `crates/ferrum-doc/src/fixture.rs`, `crates/ferrum-render/examples/render_report.rs`, `crates/ferrum-editor/examples/editor_report.rs`, `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md`
- Test: `crates/ferrum-scene/tests/vector.rs`

**Interfaces:**
- Produces: `ferrum_scene::vector::{simplify(&PathShape, tol), simplify_path(&BezPath, tol), SIMPLIFY_MIN_ELEMENTS}`, `path_stroke_outset(&Props)` (без контуров), `DisplayList::{paths_at(scale), detail}`, `display::LOD_TOLERANCE_PX`, `ferrum_doc::fixture::wavy_network(n)`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-scene/tests/vector.rs` заменить:

````rust
    assert_eq!(scene.hit_test(Point::new(100.0, 25.0), 1.0), None);
}
````

на:

````rust
    assert_eq!(scene.hit_test(Point::new(100.0, 25.0), 1.0), None);
}

#[test]
fn big_networks_are_simplified_to_the_tolerance_small_ones_are_not() {
    let small = network_shape(&vector(net(
        &[(0.0, 0.0), (50.0, 0.1), (100.0, 0.0)],
        &[(0, 1), (1, 2)],
    )));
    assert_eq!(simplify(&small, 1.0), small, "маленький контур — как есть");
    let p = vector(ferrum_doc::fixture::wavy_network(100_000));
    let shape = network_shape(&p);
    let s = simplify(&shape, 0.25);
    let (before, after) = (shape.stroke.elements().len(), s.stroke.elements().len());
    assert!(after * 20 < before, "{before} → {after}");
    assert!(s.closed && s.regions.len() == 1);
    // Каждая точка упрощённого пути — на исходном контуре (не дальше допуска).
    for el in s.stroke.elements().iter().step_by(50) {
        if let PathEl::LineTo(q) | PathEl::MoveTo(q) = el {
            let d = distance_to(&shape.stroke, *q, 1.0).unwrap();
            assert!(d <= 0.25, "{q:?}: {d}");
        }
    }
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test -p ferrum-scene --test vector`
Expected: FAIL — не собирается: нет `ferrum_scene::vector::simplify` и `ferrum_doc::fixture::wavy_network`

- [ ] **Step 3: Реализация**

В `crates/ferrum-doc/src/fixture.rs` заменить:

````rust
            ids.push(page.create_with_props(Some(frame), None, &p)?);
        }
    }
    Ok(ids)
}
````

на:

````rust
            ids.push(page.create_with_props(Some(frame), None, &p)?);
        }
    }
    Ok(ids)
}

/// Сеть для замеров (подсистема 3, этап 3): замкнутый волнистый контур из `n`
/// точек по эллипсу 2000 × 1000 (как обведённая карта или скан), каждый
/// четвёртый сегмент — кривая, одна область по всей петле.
pub fn wavy_network(n: usize) -> crate::VectorNetwork {
    use crate::{FillRule, VectorNetwork, VectorPoint, VectorRegion, VectorSegment};
    let points = (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            let r = 1.0 + 0.01 * (i as f64 * 0.005).sin();
            VectorPoint::new(1000.0 + 1000.0 * r * a.cos(), 500.0 + 500.0 * r * a.sin())
        })
        .collect();
    let segments = (0..n as u32)
        .map(|i| {
            let mut s = VectorSegment::line(i, (i + 1) % n as u32);
            if i % 4 == 0 {
                s.tangent_start = [0.01, -0.01];
                s.tangent_end = [-0.01, 0.01];
            }
            s
        })
        .collect();
    VectorNetwork {
        points,
        segments,
        regions: vec![VectorRegion {
            rule: FillRule::NonZero,
            loops: vec![(0..n as u32).collect()],
            fills: None,
        }],
    }
}
````

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust

    typing(&mut report);

    println!("| Метрика | Значение | Цель | Итог |");
````

на:

````rust

    typing(&mut report);
    vector_edit(&mut report);

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

/// Правка вектора из 100 000 точек (подсистема 3, этап 3): перетаскивание точки в
/// режиме правки — кадр жеста (превью сцены с новой рамкой) и запись одним шагом.
fn vector_edit(report: &mut Report) {
    use std::sync::Arc;
    let mut ed =
        Editor::new(Document::new(), None, VIEWPORT, 1.0, Platform::Mac).expect("страница");
    let net = ferrum_doc::fixture::wavy_network(100_000);
    let b = ferrum_scene::vector::network_bounds(&net).unwrap();
    let id = ed
        .transact(|tx| {
            let id = tx.create_node(None, None, ferrum_doc::NodeKind::Vector)?;
            tx.set_props(
                id,
                &PropsPatch {
                    w: Some(b.width()),
                    h: Some(b.height()),
                    vector: Some(Arc::new(net.clone())),
                    ..Default::default()
                },
            )?;
            Ok(vec![id])
        })
        .unwrap()[0];
    let mut cam = Camera::new(VIEWPORT, 1.0);
    cam.fit(kurbo::Rect::new(0.0, 0.0, b.width(), b.height()), 48.0);
    ed.set_camera(cam);
    ed.start_vector_edit(id);
    ed.take_events();
    let steps = ed.step_count();
    // Точка на правом краю эллипса (точка 0 сети — справа, в середине по высоте).
    let n = ed.scene().node(id).unwrap().clone();
    let m = n.world * ferrum_scene::vector::fit(&n.props);
    let p0 = net.points[0];
    let from = ed.camera().world_to_screen(m * Point::new(p0.x, p0.y));
    let mut clock = Clock(1000.0);
    timed(&mut ed, Input::PointerDown(clock.at(from)));
    let mut moves = Vec::new();
    for i in 1..=MOVES {
        let e = clock.at(Point::new(from.x + i as f64, from.y + 0.5 * i as f64));
        moves.push(timed(&mut ed, Input::PointerMove(e)));
    }
    let end = Point::new(from.x + MOVES as f64, from.y + 0.5 * MOVES as f64);
    let commit = timed(&mut ed, Input::PointerUp(clock.at(end)));
    report.gate_median(
        "Вектор 100 000 точек: перетаскивание точки, кадр жеста",
        &moves,
        16.0,
    );
    report.gate(
        "Вектор 100 000 точек: отпускание — запись одним шагом",
        commit,
        50.0,
    );
    report.check(
        "Перетаскивание точки — один шаг отмены",
        ed.step_count() == steps + 1,
        format!("{}", ed.step_count() - steps),
        "1",
    );
}

````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
        8.0,
        "≤ 8 мс",
    );
}
````

на:

````rust
        8.0,
        "≤ 8 мс",
    );
}

/// Векторы (подсистема 3, этап 3): сеть из 100 000 точек с областью и обводкой
/// рисуется в бюджете кадра; правка одной точки — новый контур и кадр.
fn vector_section(report: &mut Report) {
    use std::sync::Arc;
    const POINTS: usize = 100_000;
    let net = ferrum_doc::fixture::wavy_network(POINTS);
    let b = ferrum_scene::vector::network_bounds(&net).unwrap();
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Vector)?;
            tx.set_props(
                id,
                &PropsPatch {
                    w: Some(b.width()),
                    h: Some(b.height()),
                    fills: Some(vec![ferrum_doc::Fill::solid(ferrum_doc::Color::SHAPE_GRAY)]),
                    vector: Some(Arc::new(net.clone())),
                    ..Default::default()
                },
            )?;
            Ok(vec![id])
        })
        .unwrap()[0];
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let mut camera = Camera::new(VIEWPORT, 1.0);
    camera.fit(kurbo::Rect::new(0.0, 0.0, b.width(), b.height()), 48.0);
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    let mut renderer = Renderer::new();
    let t = Instant::now();
    renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
    report.gate(
        "Вектор 100 000 точек: первый кадр (контуры и обводка строятся)",
        t.elapsed(),
        16.7,
        "≤ 16.7 мс",
    );
    let (steady, _) = frame(&mut renderer, &mut backend, &scene, &camera);
    report.gate(
        "Вектор 100 000 точек: кадр (контуры в кэше)",
        steady,
        16.7,
        "≤ 16.7 мс",
    );
    // Правка одной точки: документ, сцена и кадр с новым контуром.
    let mut edited = net.clone();
    edited.points[POINTS / 3].x += 15.0;
    let edited = Arc::new(edited);
    let t = Instant::now();
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                vector: Some(edited.clone()),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    let changes = doc.take_changes(page);
    scene.apply(&changes, doc.page(page).unwrap());
    let (w, h) = camera.device_size();
    backend.reset(w, h);
    renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
    report.gate(
        "Вектор 100 000 точек: правка точки — запись, сцена и кадр",
        t.elapsed(),
        16.7,
        "≤ 16.7 мс",
    );
    // Режим правки: сеть тонкой линией, выделенная точка, ручки.
    let overlay = Overlay {
        vector_edit: Some(ferrum_render::VectorEditOverlay {
            transform: scene.node(id).unwrap().world
                * ferrum_scene::vector::fit(&scene.node(id).unwrap().props),
            net: edited.clone(),
            points: vec![POINTS as u32 / 3],
            segments: vec![],
            handles: vec![],
            selected_handles: vec![],
            rubber: None,
        }),
        ..Default::default()
    };
    let runs = (0..7)
        .map(|_| {
            let t = Instant::now();
            renderer.render(&mut backend, &scene, &camera, &overlay, &NoImages, None);
            t.elapsed()
        })
        .collect();
    report.gate(
        "Вектор 100 000 точек: кадр правки (сеть и точки в оверлее)",
        median(runs),
        16.7,
        "≤ 16.7 мс",
    );
}
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust

    text_section(&mut report);

    // Экспорт фрейма @3x — справочно.
````

на:

````rust

    text_section(&mut report);
    vector_section(&mut report);

    // Экспорт фрейма @3x — справочно.
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
    /// Номер кадра, в котором узел последний раз рисовался полностью.
    pub last_used: u64,
    /// Контуры для обводки по контуру и областей вектора (`None` — кольца).
    /// Держится вместе со списком: по нему контуры строятся заново после
````

на:

````rust
    /// Номер кадра, в котором узел последний раз рисовался полностью.
    pub last_used: u64,
    /// Точность, с которой построены контуры (`paths_at`): у больших векторов —
    /// по масштабу кадра, у остальных — 0 (точно).
    pub detail: f64,
    /// Контуры для обводки по контуру и областей вектора (`None` — кольца).
    /// Держится вместе со списком: по нему контуры строятся заново после
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
    pub style: StrokeStyle,
}

pub fn color(c: Color) -> Rgba {
````

на:

````rust
    pub style: StrokeStyle,
}

/// Точность упрощённых контуров больших векторов — в пикселях устройства.
pub const LOD_TOLERANCE_PX: f64 = 0.25;

pub fn color(c: Color) -> Rgba {
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
            split,
            last_used: 0,
            path: path_shape.map(|shape| {
                Arc::new(PathSource {
````

на:

````rust
            split,
            last_used: 0,
            detail: 0.0,
            path: path_shape.map(|shape| {
                Arc::new(PathSource {
````

В `crates/ferrum-render/src/display.rs` заменить:

````rust
    }

    /// Контуры, построенные при первом обращении.
    pub fn paths(&mut self) -> &Paths {
        let geom = self.geometry;
        let items = &self.items;
        let list = &self.items;
        let ps = self.path.as_deref().map(|p| &p.shape);
        let style = self.path.as_deref().map(|p| p.style).unwrap_or_default();
        self.paths.get_or_insert_with(|| {
````

на:

````rust
    }

    /// Контуры для кадра с масштабом `scale` (пикселей устройства на единицу мира):
    /// у большого вектора — упрощённые до четверти пикселя ближайшего сверху
    /// масштаба-степени двойки (перестраиваются, когда масштаб уходит за неё), у
    /// остальных — точные и одни на все масштабы. `scale = 0` — точные.
    pub fn paths_at(&mut self, scale: f64) -> &Paths {
        let detail = match self.path.as_deref() {
            Some(p)
                if scale > 0.0
                    && p.shape.stroke.elements().len() >= vector::SIMPLIFY_MIN_ELEMENTS =>
            {
                LOD_TOLERANCE_PX / 2f64.powf(scale.log2().ceil())
            }
            _ => 0.0,
        };
        if self.detail != detail {
            self.paths = None;
            self.detail = detail;
        }
        let geom = self.geometry;
        let items = &self.items;
        let list = &self.items;
        let simplified = self
            .path
            .as_deref()
            .filter(|_| detail > 0.0)
            .map(|p| vector::simplify(&p.shape, detail));
        let ps = simplified
            .as_ref()
            .or(self.path.as_deref().map(|p| &p.shape));
        let style = self.path.as_deref().map(|p| p.style).unwrap_or_default();
        self.paths.get_or_insert_with(|| {
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        }
        if let Some(mut list) = held {
            list.paths();
            self.play(backend, &list, alpha, ctx, part);
            return None;
````

на:

````rust
        }
        if let Some(mut list) = held {
            list.paths_at(ctx.scale);
            self.play(backend, &list, alpha, ctx, part);
            return None;
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
            }
            let mut list = DisplayList::build(n, split);
            list.paths();
            self.play(backend, &list, alpha, ctx, part);
            if ctx.retain {
````

на:

````rust
            }
            let mut list = DisplayList::build(n, split);
            list.paths_at(ctx.scale);
            self.play(backend, &list, alpha, ctx, part);
            if ctx.retain {
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        let mut list = self.lists[i].take().expect("проверено выше");
        list.last_used = self.frame;
        list.paths();
        self.play(backend, &list, alpha, ctx, part);
        self.lists[i] = Some(list);
````

на:

````rust
        let mut list = self.lists[i].take().expect("проверено выше");
        list.last_used = self.frame;
        list.paths_at(ctx.scale);
        self.play(backend, &list, alpha, ctx, part);
        self.lists[i] = Some(list);
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
        Point::new(p.x, p.y)
    };
    let seg_path = |i: usize, path: &mut BezPath| {
        let s = &net.segments[i];
        let (a, b) = (to_device * pt(s.start), to_device * pt(s.end));
````

на:

````rust
        Point::new(p.x, p.y)
    };
    // Сегменты по порядку: если сегмент начинается там, где кончился прежний, путь
    // продолжается без нового подпути (так большую сеть можно упростить целиком).
    let seg_path = |i: usize, path: &mut BezPath, last: &mut Option<Point>| {
        let s = &net.segments[i];
        let (a, b) = (to_device * pt(s.start), to_device * pt(s.end));
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
        let bb = Rect::from_points(a, b).union_pt(ta).union_pt(tb);
        if bb.x1 < screen.x0 || bb.x0 > screen.x1 || bb.y1 < screen.y0 || bb.y0 > screen.y1 {
            return;
        }
        path.move_to(a);
        if s.is_straight() {
            path.line_to(b);
````

на:

````rust
        let bb = Rect::from_points(a, b).union_pt(ta).union_pt(tb);
        if bb.x1 < screen.x0 || bb.x0 > screen.x1 || bb.y1 < screen.y0 || bb.y0 > screen.y1 {
            *last = None;
            return;
        }
        if *last != Some(a) {
            path.move_to(a);
        }
        *last = Some(b);
        if s.is_straight() {
            path.line_to(b);
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
    backend.set_color(theme.select);
    let mut all = BezPath::new();
    for i in 0..net.segments.len() {
        seg_path(i, &mut all);
    }
    backend.stroke_path(&all, &Stroke::new(dpr));
    if !v.segments.is_empty() {
        let mut sel = BezPath::new();
        for &i in &v.segments {
            if (i as usize) < net.segments.len() {
                seg_path(i as usize, &mut sel);
            }
        }
````

на:

````rust
    backend.set_color(theme.select);
    let mut all = BezPath::new();
    let mut last = None;
    for i in 0..net.segments.len() {
        seg_path(i, &mut all, &mut last);
    }
    // Большая сеть — ломаной с точностью в четверть пикселя: тысячи сегментов
    // меньше пикселя обводить по одному незачем.
    if all.elements().len() > ferrum_scene::vector::SIMPLIFY_MIN_ELEMENTS {
        all = ferrum_scene::vector::simplify_path(&all, 0.25 * dpr);
    }
    backend.stroke_path(&all, &Stroke::new(dpr));
    if !v.segments.is_empty() {
        let mut sel = BezPath::new();
        let mut last = None;
        for &i in &v.segments {
            if (i as usize) < net.segments.len() {
                seg_path(i as usize, &mut sel, &mut last);
            }
        }
````

В `crates/ferrum-scene/src/shape.rs` заменить:

````rust
/// с соединениями и окончаниями.
pub fn stroke_outset(p: &Props) -> f64 {
    match vector::path_shape(p) {
        Some(shape) => vector::path_stroke_outset(p, &shape),
        None => geometry(p).stroke_outset(&p.strokes),
    }
}
````

на:

````rust
/// с соединениями и окончаниями.
pub fn stroke_outset(p: &Props) -> f64 {
    if vector::uses_path_stroke(p) {
        vector::path_stroke_outset(p)
    } else {
        geometry(p).stroke_outset(&p.strokes)
    }
}
````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
use kurbo::{
    Affine, BezPath, Cap, Circle, CubicBez, Join, Line, ParamCurve, ParamCurveDeriv,
    ParamCurveNearest, PathSeg, Point, Rect, Shape, StrokeOpts, Vec2,
};

````

на:

````rust
use kurbo::{
    Affine, BezPath, Cap, Circle, CubicBez, Join, Line, ParamCurve, ParamCurveDeriv,
    ParamCurveNearest, PathEl, PathSeg, Point, Rect, Shape, StrokeOpts, Vec2,
};

````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
    let curve = |i: usize| fit_seg(fit, segment_curve(net, &net.segments[i]));
    let n = net.points.len();
    // Сегменты каждой точки.
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, s) in net.segments.iter().enumerate() {
        incident[s.start as usize].push(i);
        incident[s.end as usize].push(i);
    }
    let mut used = vec![false; net.segments.len()];
    let mut stroke = BezPath::new();
````

на:

````rust
    let curve = |i: usize| fit_seg(fit, segment_curve(net, &net.segments[i]));
    let n = net.points.len();
    let incident = Incidence::new(net);
    let mut used = vec![false; net.segments.len()];
    let mut stroke = BezPath::new();
````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
            push_seg(stroke, c);
            at = if forward { s.end } else { s.start } as usize;
            if at == from && incident[at].len() == 2 {
                stroke.close_path();
                return;
            }
            if incident[at].len() != 2 {
                return;
            }
            match incident[at].iter().find(|&&i| !used[i]) {
                Some(&next) => seg = next,
                None => return,
            }
        }
    };
    for (v, segs) in incident.iter().enumerate() {
        if segs.len() == 2 {
            continue;
        }
        for &s in segs {
            if !used[s] {
                walk(v, s, &mut used, &mut stroke);
            }
        }
````

на:

````rust
            push_seg(stroke, c);
            at = if forward { s.end } else { s.start } as usize;
            if at == from && incident.of(at).len() == 2 {
                stroke.close_path();
                return;
            }
            if incident.of(at).len() != 2 {
                return;
            }
            match incident
                .of(at)
                .iter()
                .map(|&i| i as usize)
                .find(|&i| !used[i])
            {
                Some(next) => seg = next,
                None => return,
            }
        }
    };
    for v in 0..n {
        let segs = incident.of(v);
        if segs.len() == 2 {
            continue;
        }
        for &s in segs {
            if !used[s as usize] {
                walk(v, s as usize, &mut used, &mut stroke);
            }
        }
````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
    let mut ends = Vec::new();
    let mut junctions = Vec::new();
    for (v, segs) in incident.iter().enumerate() {
        match segs[..] {
            [s] => {
                let seg = &net.segments[s];
                let is_start = seg.start as usize == v;
````

на:

````rust
    let mut ends = Vec::new();
    let mut junctions = Vec::new();
    for v in 0..n {
        match *incident.of(v) {
            [s] => {
                let s = s as usize;
                let seg = &net.segments[s];
                let is_start = seg.start as usize == v;
````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
        junctions,
        closed,
    }
}
````

на:

````rust
        junctions,
        closed,
    }
}

/// Сегменты каждой точки одним массивом (смещения по точкам): без вектора на
/// точку — на сети в 100 000 точек это миллисекунды выделений памяти.
struct Incidence {
    offsets: Vec<u32>,
    segs: Vec<u32>,
}

impl Incidence {
    fn new(net: &VectorNetwork) -> Incidence {
        let n = net.points.len();
        let mut offsets = vec![0u32; n + 1];
        for s in &net.segments {
            offsets[s.start as usize + 1] += 1;
            offsets[s.end as usize + 1] += 1;
        }
        for i in 0..n {
            offsets[i + 1] += offsets[i];
        }
        let mut fill = offsets.clone();
        let mut segs = vec![0u32; offsets[n] as usize];
        for (i, s) in net.segments.iter().enumerate() {
            for v in [s.start, s.end] {
                segs[fill[v as usize] as usize] = i as u32;
                fill[v as usize] += 1;
            }
        }
        Incidence { offsets, segs }
    }

    fn of(&self, v: usize) -> &[u32] {
        &self.segs[self.offsets[v] as usize..self.offsets[v + 1] as usize]
    }
}
````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
}

/// На сколько обводки узла по контуру выступают за рамку формы.
pub fn path_stroke_outset(p: &Props, shape: &PathShape) -> f64 {
    let style = &p.stroke_style;
    p.strokes
        .iter()
        .filter(|s| s.width > 0.0)
        .map(|s| {
            let w = effective_width(shape, s);
            let join = if style.join == StrokeJoin::Miter {
                w / 2.0 * style.miter_limit.max(1.0)
````

на:

````rust
}

/// Окончания открытых концов узла и замкнут ли путь — без построения контуров
/// (границы узла пересчитываются на каждую правку и кадр жеста).
fn end_caps(p: &Props) -> (Vec<StrokeCap>, bool) {
    let s = &p.stroke_style;
    match p.kind {
        NodeKind::Vector => {
            let Some(net) = &p.vector else {
                return (vec![], false);
            };
            let deg = net.degrees();
            let (mut start, mut end) = (false, false);
            for seg in &net.segments {
                start |= deg[seg.start as usize] == 1;
                end |= deg[seg.end as usize] == 1;
            }
            let caps = [(start, s.cap_start), (end, s.cap_end)]
                .into_iter()
                .filter_map(|(on, c)| on.then_some(c))
                .collect::<Vec<_>>();
            let closed = caps.is_empty() && !net.segments.is_empty();
            (caps, closed)
        }
        NodeKind::Line => (vec![s.cap_start, s.cap_end], false),
        _ => (vec![], true),
    }
}

/// На сколько обводки узла по контуру выступают за рамку формы.
pub fn path_stroke_outset(p: &Props) -> f64 {
    let style = &p.stroke_style;
    let (caps, closed) = end_caps(p);
    p.strokes
        .iter()
        .filter(|s| s.width > 0.0)
        .map(|s| {
            let w = if closed && s.align != StrokeAlign::Center {
                2.0 * s.width
            } else {
                s.width
            };
            let join = if style.join == StrokeJoin::Miter {
                w / 2.0 * style.miter_limit.max(1.0)
````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
                w / 2.0
            };
            let caps = shape
                .ends
                .iter()
                .map(|e| cap_extent(e.cap, w))
                .fold(0.0, f64::max);
            join.max(caps)
        })
        .fold(0.0, f64::max)
````

на:

````rust
                w / 2.0
            };
            let cap = caps.iter().map(|c| cap_extent(*c, w)).fold(0.0, f64::max);
            join.max(cap)
        })
        .fold(0.0, f64::max)
````

В `crates/ferrum-scene/src/vector.rs` заменить:

````rust
        }
    }
    best
}
````

на:

````rust
        }
    }
    best
}

/// С какого числа элементов контуры упрощаются под масштаб (`simplify`): меньшие
/// рисуются точно на любом зуме.
pub const SIMPLIFY_MIN_ELEMENTS: usize = 4096;

/// Контуры с точностью `tol` (в координатах формы): кривые — ломаными, точки ближе
/// `tol` к последней оставленной — пропускаются (концы подпутей остаются). Сеть в
/// 100 000 точек при отдалении — тысячи отрезков меньше пикселя; рисовать и обводить
/// их все — сотни миллисекунд на кадр. Небольшие контуры возвращаются как есть.
pub fn simplify(shape: &PathShape, tol: f64) -> PathShape {
    let size: usize = shape.stroke.elements().len()
        + shape
            .regions
            .iter()
            .map(|r| r.path.elements().len())
            .sum::<usize>();
    if size < SIMPLIFY_MIN_ELEMENTS || tol.is_nan() || tol <= 0.0 {
        return shape.clone();
    }
    PathShape {
        stroke: simplify_path(&shape.stroke, tol),
        regions: shape
            .regions
            .iter()
            .map(|r| RegionPath {
                path: simplify_path(&r.path, tol),
                ..r.clone()
            })
            .collect(),
        ..shape.clone()
    }
}

/// Ломаная подпути с отбросом точек, лежащих ближе `tol` к прямой от последней
/// оставленной точки (Реуманн — Виткам, за один проход); точки, где путь
/// поворачивает назад, остаются.
#[derive(Default)]
struct Thinner {
    anchor: Point,
    dir: Option<Vec2>,
    /// Последняя пропущенная точка: оставляется, когда следующая уходит от прямой.
    prev: Option<Point>,
    along: f64,
}

impl Thinner {
    fn start(&mut self, out: &mut BezPath, p: Point) {
        out.move_to(p);
        *self = Thinner {
            anchor: p,
            ..Default::default()
        };
    }

    fn push(&mut self, out: &mut BezPath, p: Point, tol: f64) {
        let v = p - self.anchor;
        let Some(dir) = self.dir else {
            if v.hypot() >= tol {
                self.dir = Some(v / v.hypot());
                self.along = v.hypot();
            }
            self.prev = Some(p);
            return;
        };
        let along = v.dot(dir);
        if v.cross(dir).abs() <= tol && along >= self.along - tol {
            self.prev = Some(p);
            self.along = self.along.max(along);
            return;
        }
        let q = self.prev.unwrap_or(p);
        out.line_to(q);
        self.anchor = q;
        let w = p - q;
        let n = w.hypot();
        self.dir = (n > 0.0).then(|| w / n);
        self.along = n;
        self.prev = Some(p);
    }

    fn finish(&mut self, out: &mut BezPath) {
        if let Some(q) = self.prev.take() {
            out.line_to(q);
        }
    }
}

/// Ломаная пути с точностью `tol` (см. `simplify`) — и для оверлея правки.
pub fn simplify_path(path: &BezPath, tol: f64) -> BezPath {
    let mut out = BezPath::new();
    let mut t = Thinner::default();
    let mut last = Point::ZERO;
    for el in path.iter() {
        match el {
            PathEl::MoveTo(p) => {
                t.finish(&mut out);
                t.start(&mut out, p);
                last = p;
            }
            PathEl::LineTo(p) => {
                t.push(&mut out, p, tol);
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                // Кривая, чьи ручки ближе `tol` к хорде, — отрезок: дробить её
                // незачем (на сети в 100 000 точек таких — десятки тысяч).
                let chord = Line::new(last, p);
                let near = |q: Point| chord.nearest(q, 1e-9).distance_sq <= tol * tol;
                if near(a) && near(b) {
                    t.push(&mut out, p, tol);
                } else {
                    kurbo::flatten([PathEl::MoveTo(last), PathEl::CurveTo(a, b, p)], tol, |e| {
                        if let PathEl::LineTo(q) = e {
                            t.push(&mut out, q, tol);
                        }
                    });
                }
                last = p;
            }
            PathEl::QuadTo(a, p) => {
                kurbo::flatten([PathEl::MoveTo(last), PathEl::QuadTo(a, p)], tol, |e| {
                    if let PathEl::LineTo(q) = e {
                        t.push(&mut out, q, tol);
                    }
                });
                last = p;
            }
            PathEl::ClosePath => {
                t.finish(&mut out);
                out.close_path();
            }
        }
    }
    t.finish(&mut out);
    out
}
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- Пределы (ядро §4.3, недоверенный ввод): `data` ≤ 64 КБ за команду, имя семейства ≤ 256 байт, байты шрифта ≤ 64 МБ, `patch` — только известные ключи с проверкой значений.
- **Сделано (этап 2):** `TextEditStart { id }`, `TextEditEnd`, `TextInput { kind, data? }` (`insert`, `paragraph`, `paste` — вне правки новый текст, `compose`, `commit`, `delete*`), `TextKey { key, mods }` (`key` — `KeyboardEvent.code`), `SetTextStyle { patch, phase, ids? }` и `SetTextProps { patch, phase, ids? }` (диапазон — выделение правки в ядре, поля `range` нет; превью — фазой, как у полей панели); события `TextCaret { editing, rect?, selection?, draft }`, `TextStyleSummary { summary? }` (перечисления — строками), сверх списка — `PageFonts { families }` (группа «В документе»); `Clipboard` получил `plain` (обычный текст фрагмента). Курсор `text` — у инструмента T и над текстом в правке. Очередь правок, ждущих шрифтов: пока она не пуста, в неё же по порядку встают и все следующие команды, меняющие документ, выделение, камеру или правку (`Paste`, `Execute` — «Вырезать», отмена, — `Select`, `SetTool`, `SetPage` и т. д.), и ввод из кольца (щелчки, клавиши; сдвиги указателя подряд — последний), и выполняются по приходу шрифтов — щелчок не сдвигает курсор раньше ждущего набора, щелчок мимо заканчивает правку после него. Сразу выполняются загрузка шрифтов, замена документа (очередь сбрасывается), снимок и журнал, чтения для тестов, тема и вид панели слоёв. «Вырезать», Backspace и вставка фрагмента, сами упёршиеся в незагруженный шрифт, тоже встают в очередь (без ошибки). Очередь — не больше 4096 записей (сверх — сбрасывается с ошибкой «Шрифты загружаются слишком долго…»); ответа на запрос шрифта ждут не дольше 15 с, затем запрошенные шрифты считаются не полученными (как `LoadFont` без байтов), очередь выполняется запасным шрифтом, поздние байты всё равно принимаются.
- **Сделано (этап 1):** `FontsAvailable { source: bundled | system, families: [{ family, faces: [{ style, weight?, italic?, stretch?, file, index? }] }] }` (у системных Chrome вес и курсив ядро угадывает по имени начертания), `LoadFont { family, file, bytes? }` — по одному файлу (без байтов — файл не получить), `FontRequest { family, source, files }` (вшитые выполняет worker ядра, системные — основной поток), `MissingFonts { list: [{ family, style, nodes }] }`, `ReplaceMissingFont { from: { family, style }, to: { family, style } }`; сверх списка — события `FontCatalog { families: [{ family, source, styles }] }` (выбор замены) и `Log { message }` (журнал ошибок), команды `DemoText { texts }` (замер) и `TestCreateText` (только тестовый режим страницы). Правки, которым нужны ещё не загруженные шрифты, ядро держит в очереди и выполняет по приходу байтов.

````

на:

````markdown
- Пределы (ядро §4.3, недоверенный ввод): `data` ≤ 64 КБ за команду, имя семейства ≤ 256 байт, байты шрифта ≤ 64 МБ, `patch` — только известные ключи с проверкой значений.
- **Сделано (этап 2):** `TextEditStart { id }`, `TextEditEnd`, `TextInput { kind, data? }` (`insert`, `paragraph`, `paste` — вне правки новый текст, `compose`, `commit`, `delete*`), `TextKey { key, mods }` (`key` — `KeyboardEvent.code`), `SetTextStyle { patch, phase, ids? }` и `SetTextProps { patch, phase, ids? }` (диапазон — выделение правки в ядре, поля `range` нет; превью — фазой, как у полей панели); события `TextCaret { editing, rect?, selection?, draft }`, `TextStyleSummary { summary? }` (перечисления — строками), сверх списка — `PageFonts { families }` (группа «В документе»); `Clipboard` получил `plain` (обычный текст фрагмента). Курсор `text` — у инструмента T и над текстом в правке. Очередь правок, ждущих шрифтов: пока она не пуста, в неё же по порядку встают и все следующие команды, меняющие документ, выделение, камеру или правку (`Paste`, `Execute` — «Вырезать», отмена, — `Select`, `SetTool`, `SetPage` и т. д.), и ввод из кольца (щелчки, клавиши; сдвиги указателя подряд — последний), и выполняются по приходу шрифтов — щелчок не сдвигает курсор раньше ждущего набора, щелчок мимо заканчивает правку после него. Сразу выполняются загрузка шрифтов, замена документа (очередь сбрасывается), снимок и журнал, чтения для тестов, тема и вид панели слоёв. «Вырезать», Backspace и вставка фрагмента, сами упёршиеся в незагруженный шрифт, тоже встают в очередь (без ошибки). Очередь — не больше 4096 записей (сверх — сбрасывается с ошибкой «Шрифты загружаются слишком долго…»); ответа на запрос шрифта ждут не дольше 15 с, затем запрошенные шрифты считаются не полученными (как `LoadFont` без байтов), очередь выполняется запасным шрифтом, поздние байты всё равно принимаются.
- **Сделано (этап 3):** инструменты `pen` и `arrow` (команды раскладки `tool.pen` — P, `tool.arrow` — Shift+L, `object.flatten` — Ctrl/Cmd+E), курсоры `pen` и `bucket`, тип строки слоя `vector`; правка панели `strokeStyle { capStart?, capEnd?, join?, miterLimit? }` (предел вне 1…100 — ошибка, панель зажимает его теми же числами из `limits.ts`), сводка `strokeStyle { ends, corners, capStart?, capEnd?, join?, miterLimit }` (`undefined` — «Смешанное»). Сеть в протокол не идёт: правка вектора идёт вводом холста.
- **Сделано (этап 1):** `FontsAvailable { source: bundled | system, families: [{ family, faces: [{ style, weight?, italic?, stretch?, file, index? }] }] }` (у системных Chrome вес и курсив ядро угадывает по имени начертания), `LoadFont { family, file, bytes? }` — по одному файлу (без байтов — файл не получить), `FontRequest { family, source, files }` (вшитые выполняет worker ядра, системные — основной поток), `MissingFonts { list: [{ family, style, nodes }] }`, `ReplaceMissingFont { from: { family, style }, to: { family, style } }`; сверх списка — события `FontCatalog { families: [{ family, source, styles }] }` (выбор замены) и `Log { message }` (журнал ошибок), команды `DemoText { texts }` (замер) и `TestCreateText` (только тестовый режим страницы). Правки, которым нужны ещё не загруженные шрифты, ядро держит в очереди и выполняет по приходу байтов.

````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown
- Фигуры (прямоугольник, эллипс, линия, многоугольник позже) двойным щелчком переходят в правку как вектор: узел заменяется `Vector` той же геометрии одним шагом отмены; «Сплющить» (Ctrl/Cmd+E) — то же для выделения, несколько узлов — в один `Vector`.
- Обводка: окончания (`none` | `round` | `square` | `arrowLines` | `arrowTriangle` | `circle` | `diamond`) на каждом открытом конце, соединения (`miter` | `round` | `bevel`, предел острого угла), положение (`inside` | `center` | `outside` — для замкнутых). Инструмент «Стрелка» (Shift+L) — линия с `arrowLines` на конце.

## 10. Булевы операции (этап 4)
````

на:

````markdown
- Фигуры (прямоугольник, эллипс, линия, многоугольник позже) двойным щелчком переходят в правку как вектор: узел заменяется `Vector` той же геометрии одним шагом отмены; «Сплющить» (Ctrl/Cmd+E) — то же для выделения, несколько узлов — в один `Vector`.
- Обводка: окончания (`none` | `round` | `square` | `arrowLines` | `arrowTriangle` | `circle` | `diamond`) на каждом открытом конце, соединения (`miter` | `round` | `bevel`, предел острого угла), положение (`inside` | `center` | `outside` — для замкнутых). Инструмент «Стрелка» (Shift+L) — линия с `arrowLines` на конце.
- **Сделано (этап 3):** сеть — значение `vector` узла (двоичная форма версии 1: точки, сегменты с ручками, области с петлями, правилом и необязательными заливками — только цветом); данные из файла и буфера обмена проверяются (числа, индексы, замкнутость петель, ≤ 100 000 точек, ≤ 200 000 сегментов, ≤ 10 000 петель); неверная сеть из файла скрывает узел и открывает документ только для чтения, как неизвестный тип; правка сверх 100 000 точек — отказ «Вектор длиннее 100 000 точек». Рамка узла — границы сети (с экстремумами кривых), ресайз растягивает сеть под рамку; запись правки пересчитывает рамку и положение с поворотом и отражением так, что на экране ничего не сдвигается. Кэш сцены — версия 4 (стиль обводки у каждой строки, сеть у вектора); буфер обмена — поле `vector` (base64 двоичной формы) и необязательные поля стиля обводки, версия формата прежняя. Стиль обводки — ключи узла `strokeCapStart`, `strokeCapEnd`, `strokeJoin`, `strokeMiterLimit` (1…100, по умолчанию 4), общие для всех обводок узла, как в Figma: «начало» — открытый конец, который у своего сегмента начало (первая точка пера), «конец» — остальные. Обводка по контуру — у вектора, у линии с окончаниями и у прямоугольника без скругления с соединением не «острым»: путь с плоскими концами, окончания — фигуры (стрелка линиями: плечи 3w + 6 под 30°; треугольник с остриём на w за концом; круг и ромб радиуса 1,5w + 1,5), в точках ветвления — круг; внутри и снаружи — только у замкнутого пути (у открытого — по центру); полупрозрачная обводка с окончаниями рисуется слоем. Радиус скругления точки хранится, но не рисуется и не правится (позже). Перо: путь до конца — черновик сцены, один путь — один шаг отмены, Ctrl/Cmd+Z во время пути убирает последнюю точку; щелчок по первой точке замыкает путь (петля — область с заливками узла), по точке другого контура — соединяет и заканчивает путь, по сегменту без начатого пути — ставит на нём точку; Esc и Enter заканчивают путь — у инструмента «Перо» с возвратом к «Перемещению» (вектор выделен), в правке — к выделению точек. Правка: вход двойным щелчком (сначала, как в Figma, — на уровень глубже в группу или фрейм) и Enter; фигура становится вектором при первой правке тем же шагом (новый узел на её месте с её именем, заливками и обводками), без правки остаётся фигурой; щелчок по другому узлу — выход и обычный щелчок; V — выделение, P — перо, B — «Заливка области», стрелки — сдвиг точек, Ctrl/Cmd+A — все точки; удаление всех точек удаляет узел; привязка — к точкам сети (4 px) и к пиксельной сетке по осям без привязки к точке. «Заливка области» — самая маленькая грань под щелчком (грани плоского графа сегментов без учёта пересечений): залитая — снимается, новая — с заливками узла, у узла без заливок — серая. «Сплющить» — векторы, прямоугольники, эллипсы, линии и группы из них (группа с текстом, изображением или фреймом пропускается, текст — этап 4); один узел сохраняет позу, несколько — сливаются в координатах родителя верхнего и берут его свойства, области остальных хранят их заливки цветом. Большие сети (от 4096 элементов контура) рисуются упрощёнными под масштаб (четверть пикселя, степень двойки), оверлей правки при > 5000 точек на экране рисует только выделенные. Замер (`render_report`, `editor_report`, нативно, release, M1 Pro), сеть 100 000 точек: первый кадр ≈ 7 мс, кадр ≈ 1,5 мс, правка точки (запись, сцена, кадр) ≈ 13 мс, кадр правки с оверлеем ≈ 4 мс, кадр жеста перетаскивания точки в ядре ≈ 3 мс, запись ≈ 6 мс.

## 10. Булевы операции (этап 4)
````

В `docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md` заменить:

````markdown

- Как в ядре §12: модульные тесты крейтов (модель текста и разметка, отмена правок текста, раскладка с эталонами позиций глифов на вшитых шрифтах, векторная сеть, булевы операции — свойства и эталоны, писатель SVG — сравнение с эталоном и разбор), эталоны рендера (текст, LOD, векторы, обводки, стрелки) на `vello_cpu` и пиксели GPU против CPU, E2E (ввод, IME — синтетическими событиями композиции, буфер обмена, выбор шрифта, отсутствующий шрифт, перо, булевы, экспорт), замеры (раскладка и рисование 10 000 текстов, булевы на 1000 контуров).
- Совместимость: файл с текстом открывается в сборке подсистемы 1 только для чтения (тест на документе, собранном новой сборкой): `scripts/compat-old-build.sh [ref]` собирает `ferrum-doc` из `ref` (по умолчанию `main`) во временной копии и открывает им файл примера `text_fixture`.
````

на:

````markdown

- Как в ядре §12: модульные тесты крейтов (модель текста и разметка, отмена правок текста, раскладка с эталонами позиций глифов на вшитых шрифтах, векторная сеть, булевы операции — свойства и эталоны, писатель SVG — сравнение с эталоном и разбор), эталоны рендера (текст, LOD, векторы, обводки, стрелки) на `vello_cpu` и пиксели GPU против CPU, E2E (ввод, IME — синтетическими событиями композиции, буфер обмена, выбор шрифта, отсутствующий шрифт, перо, булевы, экспорт), замеры (раскладка и рисование 10 000 текстов, булевы на 1000 контуров).
- Совместимость: файл с текстом открывается в сборке подсистемы 1 только для чтения (тест на документе, собранном новой сборкой): `scripts/compat-old-build.sh [ref]` собирает `ferrum-doc` из `ref` (по умолчанию `main`) во временной копии и открывает им файл примера `text_fixture`; файл с вектором — `scripts/compat-old-build.sh <ref> vector_fixture` (сборка без векторов открывает его только для чтения).
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo clippy -p ferrum-wasm --target wasm32-unknown-unknown -- -D warnings`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo run --release -p ferrum-render --example render_report | grep -E "FAIL|Вектор"`

Run: `export CARGO_TARGET_DIR=$PWD/target && cargo run --release -p ferrum-editor --example editor_report | grep -E "FAIL|Вектор|Перетаскивание точки"`

Run: `export CARGO_TARGET_DIR=$PWD/target && ./scripts/compat-old-build.sh main vector_fixture`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npm run wasm && npm run typecheck && npm run unit && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR=$PWD/../../target && npx playwright test e2e/vectors.spec.ts e2e/text.spec.ts e2e/text-edit.spec.ts --project=firefox`

Expected: всё PASS; `render_report` — четыре строки «Вектор 100 000 точек» PASS (≈ 7 / 1,5 / 13 / 4 мс), `editor_report` — кадр жеста ≈ 3 мс, запись ≈ 6 мс, один шаг; совместимость — только для чтения; `e2e:ci` — все passed; Firefox — passed

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-doc/src/fixture.rs crates/ferrum-editor/examples/editor_report.rs crates/ferrum-render/examples/render_report.rs crates/ferrum-render/src/display.rs crates/ferrum-render/src/draw.rs crates/ferrum-render/src/overlay.rs crates/ferrum-scene/src/shape.rs crates/ferrum-scene/src/vector.rs crates/ferrum-scene/tests/vector.rs docs/superpowers/specs/2026-10-29-ferrum-vectors-text-design.md
git commit -m "perf: вектор из 100 000 точек в бюджете кадра — упрощение контуров под масштаб, оверлей правки, замеры; спека — сделанное в этапе 3

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Ручная проверка**

1. Chrome: `#/new`, P — щелчки, протяжка с ручками, Alt посреди протяжки, Shift; щелчок по первой точке — замкнуть; Enter — вектор выделен, «Перемещение»; Ctrl/Cmd+Z во время пути.
2. Двойной щелчок по вектору: точки и ручки, рамка, Shift, перетаскивание с красными линиями привязки, Ctrl/Cmd + протяжка сегмента, Delete и Shift+Delete, B — заливка грани и снятие, Esc.
3. Прямоугольник со скруглением: двойной щелчок, Esc — остался прямоугольником; двойной щелчок и протяжка угла — вектор, отмена — прямоугольник.
4. Несколько фигур и группа — Ctrl/Cmd+E; линия — «Окончание в конце» → «Стрелка», «Треугольник», «Ромб»; прямоугольник с обводкой — «Соединение» «Круглое»; Shift+L — стрелка.
5. Firefox: перо и правка точки.

---

## Что этап 3 сознательно не делает

- **Радиус скругления точки сети** (вершины в Figma) — хранится и проверяется, но не рисуется и не правится; скругление прямоугольника при превращении в вектор — дугами сети.
- **Поточечные окончания** (`VectorVertex.strokeCap`) — окончание одно для начала и одно для конца на узел.
- **Пересечения сегментов в «Заливке области»** — грани ищутся без деления сегментов в точках пересечения (сеть с самопересечениями даёт грани по обходу, а не по картинке); булевы операции — этап 4.
- **Сплющивание текста, изображений и фреймов** — этап 4 (вместе с «Контуром обводки»).
- **Копирование и вставка точек в правке**, смена режима ручек двойным щелчком по точке, отражение выделенных точек — позже; команды узлов в правке заканчивают её.
- **Совместная правка одного вектора** — сеть заменяется целиком (решение 1).
- **Пиксели GPU против CPU для векторов и оверлея правки** (проект `gpu`), Safari, Tauri — вручную.
- **Замер векторов в wasm** — только нативные `render_report` и `editor_report`.
