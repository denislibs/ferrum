# Ferrum, подсистема 4, этап 5: комментарии — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Участники файла на сервере оставляют комментарии на холсте: метка у узла или точки, обсуждение с ответами, правкой, удалением и закрытием, упоминания через `@`, панель обсуждений, ссылка на обсуждение, живое обновление у всех открывших файл и письма тем, кто файл не открыл. Комментарии не попадают ни в документ, ни в историю, ни в экспорт.

**Architecture:** Протокол: виды REST, строгий разбор ответов и событий, кадр `comment` (27) на WebSocket файла этапа 3, `SYNC_PROTOCOL = 3`. Сервер: таблицы и функции plpgsql в `010_comments.sql` (номер изменений файла `comment_file.seq` берётся первым делом под блокировкой, роль сверяется внутри функции), маршруты `comment-routes.ts` через конвейер `api.ts`; рассылка событий проходами с одним запросом ролей, медленному — `gap`; письма — очередь `mail_outbox`, не больше одного раза. Ядро: рендер рисует метки готовыми картинками, редактор ведёт режим «Комментарий», раскладку, группы и жесты меток, ядро worker'а проверяет ввод хозяина. Клиент: комментарии живут в `sync.worker` (`CommentStore`: догоняние по `seq`, непрочитанное, метки ядру); основной поток рисует окно у метки и виртуальную панель.

**Tech Stack:** Bun 1.4.2, TypeScript 7.0.2, SolidJS 1.9.17, @playwright/test 1.64.0, PostgreSQL (plpgsql), nodemailer и Mailpit стенда; Rust: ferrum-render (vello), ferrum-editor, ferrum-wasm. Новая прямая зависимость ядра — `unicode-segmentation = "=1.13.3"` (уже была в `Cargo.lock` через `convert_case`, загрузки нет); новых пакетов npm нет.

**Спеки:** `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` — раздел 10, а также 6, 7.2, 9 («Сделано (этап 4)»), 13, 14; спека ядра `2026-10-23-ferrum-core-design.md` §8.6.

## Global Constraints

- Ветка `feat/sub4-stage5-comments` от 30be51d. Каталог сборки wasm — у каждой копии свой: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0` (в `apps/web` — `"$PWD/../../target"`); команды — из корня копии, если не сказано иное. Нужны Bun 1.4, Node 24, Docker с Compose 2.24+, `wasm-bindgen` 0.2.129.
- Новых пакетов npm нет; в ядре — только `unicode-segmentation` из `Cargo.lock`.
- Docker: только свои проекты compose — `ferrum` и `ferrum-test`. Порты: сервер 8090, Mailpit 8025; стенд тестов — 18090, 18025, 11025, 15432, 16379. Чужие контейнеры (`agent-dev-*`, `relay-*`) не трогать. `down -v` — только `ferrum-test` и `ferrum`. Никаких `docker system prune`. `bun test` сервера и `e2e:server` делят базу стенда: не запускать их одновременно.
- Порт 4173 может быть занят посторонним процессом: его не трогать, E2E тогда — через временный конфиг Playwright с другими портами (не коммитить).
- Основной поток тонкий: запросы, порядок событий, непрочитанное и метки — в `sync.worker`; основному потоку — изменившиеся обсуждения не чаще раза в 50 мс и открытое обсуждение. Панель на 1000 обсуждений — виртуальный список, живое изменение не пересобирает её.
- Цели: кадр с 1000 метками на странице (метки едут за узлом при своём перетаскивании и при чужом живом превью) — прибавка не больше 1 мс на 1920×1080 при 1× и 2×; список, создание и ответ на файле с 1000 обсуждений по 20 комментариев — один запрос к базе на обработчик, только индексы; рассылка события 50 соединениям файла — p50/p95 в выводе пробы; медленный получатель не растит буферы.
- Права и порядок — на сервере: каждый маршрут проходит конвейер `api.ts` (роль, предел), функция изменения сверяет роль сама после блокировки номера; id из запроса ограничены `file_id` маршрута (чужое — 404 без записи). События получают только соединения, чью роль подтвердил `hello`, после запроса ролей.
- Текст — только текст: длина в кодовых точках одной функцией у клиента и сервера, без управляющих и знаков направления, имена и тексты нигде не идут в HTML без экранирования, тема письма — одной строкой.
- Письма — не больше одного раза: строка очереди берётся на отправку и фиксируется до SMTP; найденная после сбоя не отправляется снова.
- Комментарии не в Loro, не шаг отмены, не в `own_change_counter`, не в превью и экспорте.
- Автономная сборка и десктоп не меняют поведения: кода комментариев в автономной сборке нет (`check-dist.mjs`), кнопка отключена; общий код без условий по флагу, через `extensions`.
- Тесты падают при откате правки, без ожиданий по времени и зависимости от порядка.
- Поведение — как в Figma; где спека молчит, решение помечено **(поведение)**.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

Спека, раздел 10 (дословно):

- Комментарии живут в Postgres, а не в Loro: они не шаг отмены, не часть документа и не попадают в экспорт.
- **Привязка:** щелчок над узлом привязывает метку к верхнему фрейму или узлу под точкой; `x`, `y` — смещение в координатах этого узла, поэтому метка едет вместе с ним. На пустом месте — мировые координаты страницы.
- **Узел удалён или скрыт (поведение):** метка остаётся там, где он был. Сервер хранит последние мировые координаты, клиент обновляет их при создании и при перемещении метки.
- **Рисует ядро:** круг с инициалами автора, счётчик ответов, отметка «не прочитано». Метки близко друг к другу собираются в группу с числом при маленьком масштабе. Попадание по метке проверяет ядро. Метки не экспортируются и не попадают в превью.
- **Режим:** инструмент «Комментарий» — клавиша C, кнопка в тулбаре, пункт в палитре и в меню «Вид». Shift+C показывает и скрывает метки. В режиме комментариев остальные инструменты не работают, камера работает.
- **Новый комментарий:** щелчок открывает поле. Enter — отправить, Shift+Enter — новая строка, Esc — отмена. Метку можно перетащить автору и владельцу файла.
- **Обсуждение:** окно рядом с меткой, закрывается щелчком мимо. Ответы, правка своего комментария, удаление своего; владелец файла удаляет любые. Удаление первого комментария удаляет обсуждение. Кнопка «Закрыть обсуждение», закрытое можно открыть снова.
- **Упоминания:** `@` открывает список людей с доступом к файлу, поиск по имени и email; упоминание без доступа не предлагается. В тексте хранится `@[userId]`, показывается имя. Текст показывается только как текст, без HTML, до 4000 символов.
- **Панель:** в режиме комментариев правая панель показывает обсуждения: по текущей странице или всем, сортировка по дате или непрочитанным, фильтр «Только мои», показ закрытых. Щелчок — переход на страницу, камера центрируется на метке, открывается окно.
- **Ссылка:** `#/file/<id>?comment=<threadId>` открывает файл на обсуждении; адрес из письма ведёт туда.
- **Права:** комментировать могут все с доступом к файлу, в том числе читатели.
- **API:** REST `GET/POST/PATCH/DELETE /files/:id/comments…`, ограничение — 60 комментариев в минуту на пользователя.
- **Живое обновление:** сервер шлёт `comment-event { thread, comment, kind }` по WebSocket файла, клиент обновляет панель и метки без перезагрузки.
- **Письма:** упоминание и ответ в обсуждении, где человек участвует, если он не открыл файл. Письма группируются за 5 минут, в письме есть ссылка на обсуждение. Настройка «Письма о комментариях» в настройках, по умолчанию включена. Очередь — таблица `mail_outbox` в Postgres, чтобы письмо не терялось при перезапуске.

## Проверка плана

План проверен до записи. Сначала отдельными пробами (`render_report`, `editor_report`, `comments-load.test.ts`, `comments-panel.spec.ts`) проверены главные риски.
- **Кадр.** 1000 меток на экране добавляют к спискам кадра 0,298 мс на 1920×1080 и 0,517 мс на 2× при цели 1 мс; 80 групп и 20 меток на мелком зуме — 0,044 и 0,102 мс. Первый кадр, пока картинки строятся по 8 за кадр, — 2,2–2,6 мс один раз (B-407).
- **Раскладка.** Перетаскивание 10 фреймов со 100 метками на странице из 100 000 узлов (1050 меток): зум 1 — 0,050 мс (чужое живое превью — 0,054), 0,25 — 0,085 мс, вся страница с 84 группами — 0,343 / 0,363 мс. Первая версия раскладывала и метки за краем холста — 0,42 мс; поэтому они отсекаются.
- **Сервер.** Файл с 1000 обсуждений по 20 комментариев: список целиком (2 страницы, 522 КБ) p50 98 мс, p95 110 мс — до сводки в строке обсуждения было 325 / 406 мс; `?after=` p95 3,7 мс; обсуждение p95 3,7 мс; новое p95 4,5 мс; ответ p95 4,9 мс; подсказка p95 4,3 мс. EXPLAIN — только индексы, полного просмотра нет (тест проверяет).
- **Рассылка.** 50 соединениям файла: от ответа REST до прихода p50 1,8 мс, p95 2,8 мс, процессор 2 %. Под враждебным человеком (5 сеансов, 40 потоков запросов, сокет, который не читает) — p50 5,9 мс, p95 46 мс, доставлено всё, принято ровно 60 его комментариев, около 52 000 ответов 429; без пакетной рассылки было p95 1565 мс. Медленный читатель — пик буфера 82 КБ, затем `gap`.
- **Порядок.** Один оператор с CTE читал бы снимком до ожидания блокировки номера: 25 одновременных ответов показали, что ответ с номером N должен видеть N−1, — поэтому изменение стало функцией plpgsql.
- **Панель.** 1000 обсуждений на стенде: открытие режима 204 мс, в DOM 11 строк, длинных задач при прокрутке в конец и живом изменении нет.

**Как проверялся план.**
1. Прототип этапа собран в отдельной копии: `plan-verify-s4s5`, ветка `verify/sub4-s5` от 30be51d, по коммиту на задачу.
2. Шаги этого документа сгенерированы из коммитов прототипа.
3. Скрипт, читающий сам этот документ, применил их к чистой копии от 30be51d (`plan-apply-s4s5`, ветка `verify/sub4-s5-apply`) по порядку задач, с коммитом после каждой.
4. После каждой из 13 задач дерево копии совпало с деревом соответствующего коммита прототипа; `git diff verify/sub4-s5 HEAD` пуст.

На этой копии выполнены все шаги «Убедиться, что тесты падают» и все шаги «Проверить», включая полный `e2e:server` (Chromium и Firefox) и `e2e:ci` задачи 13 (через временный конфиг на портах 4283/4284). Причины падений и значения в шагах «Проверить» записаны из этого прогона. У задачи 8 (проба нагрузки) и задачи 13 (документы) шага «падают» нет: они добавляют только пробу и документы. Красный шаг задачи 12 — её новый тест `config.test.ts`; E2E комментариев до реализации задачи 12 отдельно не запускались.

В первом прогоне шаг «Проверить» задачи 8 упал на одном прежнем тесте: `share.test.ts` «приглашение не теряется при сбое первого входа» взял письмо-приглашение вместо письма входа (`waitMail` считает письма до прихода приглашения). К комментариям тест не относится; отдельно он прошёл 5 раз из 5, повтор задач 8–12 — без падений. В финальном прогоне задачи 13 `e2e:ci` — 415 passed и 1 flaky (`booleans.spec.ts:46`, прошёл повтором). `e2e:server` — 181 passed, 11 skipped, 2 failed: `sync.spec.ts:429` @big (задержка 1094 мс при пределе 1000, машина под нагрузкой) и `files.spec.ts:355` в Firefox (после перехода по выключенной ссылке остаётся редактор с плашкой «Нет доступа» вместо экрана «Нет доступа»). Отдельно: `sync.spec.ts:429` — 3 из 3; `files.spec.ts:355` в Chromium — 3 из 3, в Firefox — 2 из 3, затем 6 из 6; на базе 30be51d в Firefox — 6 из 6. Всего на вершине этапа этот тест в Firefox упал 2 раза из 10, на базе — 0 из 6: связь с этапом не доказана и не исключена, перед PR его нужно прогнать повторно. Из-за падения ворота задачи 13 остановились до `cargo test --workspace` и проверки совместимости; они выполнены отдельно на том же дереве: 1381 passed, совместимость с `origin/main` подтверждена.

**Откат правок.** Прототип проверял откатом сервер и ядро. Сервер — 26 откатов (`NOTES-s4s5-server`, R1–R26): пойманы все, кроме R16 — без ожидания соседа порядок держит отбрасывание опоздавших (ожидание — оптимизация); вместе с отбрасыванием (R16b) пойман. Тема письма без `headerSafe` (R21) ловит модульный тест, настоящее письмо через Mailpit — нет: nodemailer кодирует тему сам. Ядро — 16 откатов: пойманы все; первая попытка отката «камера» не уронила тест, потому что проверка была лишней, — она убрана. Клиент и интерфейс прототип откатом не проверял; на копии применения проверено 13 откатов `unit`-тестами (`gen-s4s5/mutate.py`), пойманы 12:
- дыра в `seq`, старое событие, первый список по половине, своё новое — непрочитано, двигать может любой, открытое не отмечается прочитанным, кадр `comment` и новая роль не доходят до хранилища, метки будят сборку пачек, `?comment=` без проверки вида, окно списка без предела начала, длина не в кодовых точках — пойманы;
- **не пойман:** событие, пришедшее во время догоняния, применяется сразу, а не ждёт конца загрузки (`CommentStore.event` без `this.fetching`). Такое событие с номером больше известного + 1 запускает догоняние, которое уже идёт, и пропадает до следующего события или переподключения (метка и панель отстают); тест на это нужен.

**Не запускались:**
- `@perf`, проект `gpu`, Firefox и WebKit автономной сборки; прежние пропуски Firefox на стенде;
- `render_report` целиком (выходит с кодом 1 из-за прежних строк, не связанных с метками, под нагрузкой машины), проба меток в wasm в браузере и на GPU — замеры ядра нативные;
- 50 настоящих браузеров и длинные задачи основного потока при 50 участниках с комментариями;
- замер медленного читателя и рассылки на Linux (B-416); несколько процессов сервера (B-415), настоящий прокси, образ amd64, `docker compose up` проекта `ferrum`, десктоп;
- ручная проверка во встроенном браузере — только снимки E2E двух контекстов.

---

## Этапы подсистемы 4

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Сервер и вход (сделан) | Docker, миграции, Better Auth, вход | 3, 4, 5 |
| 2. Файлы на сервере (сделан) | команды, проекты, черновики, роли, «Поделиться» | 6 |
| 3. Синхронизация (сделан) | журнал, снимки, чужие правки, переподключение | 7, 8 |
| 4. Присутствие (сделан) | курсоры, выделения, живое превью, аватары, следование | 9 |
| **5. Комментарии** (этот план) | метки, обсуждения, упоминания, панель, письма | 10 |
| 6. Офлайн и версии | очередь правок, история версий | 11 |
| 7. Десктоп | вход через браузер, серверные файлы | 12 |

## Решения

Где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**. По справке Figma ничего не сверялось. Итог и замеры — в «Сделано (этап 5)» спеки (задача 13).

Расхождения со спекой:

1. **§10.1: модель шире.** `comment_file(file_id, seq)` — номер изменений комментариев файла (строку `file` держат журнал и сжатие); у `comment_thread` — `wx`, `wy`, `first_id`, `deleted_at` (мягкое удаление — надгробие для `?after=`) и сводка `replies`, `last_at`, `participants`: без сводки список 1000 обсуждений — 325 мс p50 / 406 мс p95, со сводкой — 98 / 110 мс. Таблицы `user_pref(comment_mail)` и `mail_outbox`. Комментарии удаляются сразу, обсуждения — мягко.
2. **Изменение — функция plpgsql, а не один оператор с CTE.** Оператор читает снимком своего начала, до ожидания блокировки номера: ответ с номером N мог не видеть изменения N−1, и клиент застрял бы с неверной сводкой. Обработчик по-прежнему делает один запрос (`SELECT comment_reply(…)`); 25 одновременных ответов проверены тестом. Отказы — исключения `CM404`/`CM403`/`CM409`, номер не тратится.
3. **§10.4: событие — кадр `comment` (27)** на WebSocket файла: JSON до 64 КБ, каждое поле проверено, лишнее поле — неверный кадр. Прежний клиент счёл бы такой кадр испорченным соединением, поэтому `SYNC_PROTOCOL = 3`, вкладка прежней версии получает `please-reload`. Вместо `{ thread, comment, kind }` — `{kind:'thread', seq, by, action, thread, comment, deleted}`, `{kind:'removed', seq, by, thread}`, `{kind:'gap', seq}`.
4. **Свой предел списка — 120 в минуту** (`LIMITS.commentList`): страница из 500 обсуждений стоит около 50 мс базы. Обсуждение и подсказка — общий `read`; создание и ответ — `LIMITS.comment` 60 в минуту на человека по всем вкладкам и сеансам; правка, удаление, закрытие, перенос и прочтение — `write`. Тело запроса комментария — до 32 КБ (4000 четырёхбайтных знаков не входят в общие 16 КБ).
5. **`PATCH …/:threadId` — ровно одно поле** (`resolved` или `pin`): у них разные права, событие называет одно действие.
6. Новая переменная окружения `COMMENT_MAIL_DELAY_MS` (по умолчанию 300 000, стенд тестов — 2000), опрос очереди не реже окна группировки.

Поведение:

7. **(поведение)** Права: комментируют, отвечают и читают все с доступом, читатели и пришедшие по ссылке тоже. Править — только свой комментарий, без срока. Удалить — свой, владелец файла — любой; удаление первого удаляет обсуждение у всех. Двигать метку — автор обсуждения и владелец.
8. **(поведение)** Упоминание заводится только для того, у кого есть доступ при отправке; остальные `@[id]` показываются «@неизвестный», без имени и письма. Упомянуть себя можно, письма себе нет. При правке новые упоминания проверяются заново, убранные из текста уходят; писем правка не шлёт.
9. **(поведение)** Автор удалённого аккаунта — «Удалённый пользователь», пустое имя — «Участник»; цвет метки — цвет курсора автора (этап 4), удалённый — серый.
10. **(поведение)** Метка — капля, остриё — точка привязки; привязка — к верхнему узлу страницы под точкой (вложенный фрейм — к верхнему), точка — в его координатах; узел скрыт или удалён — последняя мировая точка. Группы — в экранных пикселях (ближе 28 px), щелчок по группе вписывает её метки не меньше чем вдвое ближе. Метки за краем холста не раскладываются и не рисуются.
11. **(поведение)** Вне режима метки ловят нажатие только у «Перемещения» и «Масштаба»; Shift+C их прячет; вход в режим их показывает. Щелчок по чужой метке — открыть; своя протянутая метка сразу стоит на новом месте, отказ сервера возвращает её. Протяжка по пустому холсту в режиме — ничего. Курсор режима — прицел (B-409).
12. **(поведение)** Окно у метки справа; щелчок мимо закрывает окно, и тот же щелчок новый комментарий не ставит. Esc сначала закрывает окно или черновик, затем ядро выходит из режима. Удаление — второй щелчок подтверждает («Удалить обсуждение?» у первого комментария).
13. **(поведение)** Поле: в тексте `@Имя`, на сервер уходит `@[userId]` только для выбранных из подсказки; правка своего возвращает `@Имя`. Клавиши поля холсту не достаются.
14. **(поведение)** Непрочитанное считает клиент: чужое `create`/`reply` не в открытом окне; открытое обсуждение отмечается прочитанным сразу; своё — прочитано. Закрытые обсуждения скрыты и на холсте, и в панели, пока не включено «Показать закрытые» (кроме открытого).
15. **(поведение)** Ссылка `?comment=`: после первого списка — переход к обсуждению; закрытое включает «Показать закрытые»; нет — «Обсуждение не найдено или нет доступа». Ссылка, как `?page=`, после открытия файла из адреса уходит.
16. Рассылка — проходами: накопленное уходит с одним запросом ролей; пришедшее раньше соседа ждёт его до 50 мс; очередь файла — до 1024 событий, дальше всем `gap`. Медленный получатель (буфер больше 64 КБ) событие пропускает и получает `gap` с головой файла; очереди на соединение нет. Клиент применяет `seq = известный + 1`, старое пропускает, на дыру, `gap` и новое соединение догоняет `GET ?after=известный` страницами.
17. Письма: одна ждущая строка на человека и файл, новые комментарии сливаются в неё (до 20 ссылок и счётчик остальных); известный отказ SMTP (код ≥ 400 или соединение не открылось) — повтор до 5 раз с паузой; обрыв посреди DATA и неизвестное — `uncertain`, без повтора. Пределы: 50 ждущих строк и 30 писем в час на человека. Подсказка упоминаний: адрес виден и ищется только участнику файла (прямая роль или команда), пришедшему по ссылке — только имена.
18. Метки — готовые картинки (тело и плашка отдельно, ключ — вид, цвета, плотность), не больше 8 новых за кадр, кэш 256; без картинки — заливка формы. Так кадр с 1000 метками — 0,30 мс (1×) и 0,52 мс (2×).

Решения пользователя (взяты по умолчанию, могут быть изменены):

19. Закрыть обсуждение могут автор обсуждения, редактор и владелец; читатель — только своё.
20. Адреса в подсказке упоминаний — только участникам файла; пришедшим по ссылке — только имена.
21. Письма — не больше одного раза: сбой посреди отправки может потерять письмо, но не пришлёт его дважды.
22. Окно группировки 5 минут считается от первого комментария; предел — 30 писем в час и 50 файлов в очереди на человека.
23. Метка привязывается к верхнему узлу страницы (вложенный фрейм — к верхнему), а не к узлу под точкой.
24. В автономной сборке комментариев нет совсем (как в Figma без сети), кнопка отключена с подсказкой.
25. Протокол 3: открытые вкладки прежней версии после выкладки получают «обновите страницу».

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-render/src/pins.rs` (новый), `overlay.rs`, `lib.rs` | метки и группы готовыми картинками, попадание |
| `crates/ferrum-editor/src/comments.rs`, `tools/comment.rs` (новые), `editor.rs`, `commands.rs`, `menu.rs` | режим «Комментарий», Shift+C, раскладка, группы, жесты меток |
| `packages/protocol/src/comments.ts` (новый), `sync.ts`, `files.ts` | виды REST, строгий разбор, кадр `comment`, протокол 3 |
| `crates/ferrum-wasm/src/core/comments.rs` (новый), `protocol.rs`, `core.rs` | команды и события комментариев, проверка ввода хозяина |
| `apps/server/migrations/010_comments.sql` (новый), `src/comment-routes.ts` (новый), `api.ts`, `app.ts` | таблицы, функции изменения, маршруты |
| `apps/server/src/sync/hub.ts` | живые события, `gap` медленному |
| `apps/server/src/comment-mail.ts` (новый), `index.ts`, `config.ts` | очередь писем |
| `apps/web/src/core/comments-store.ts` (новый), `sync-client.ts`, `sync-link.ts`, `worker/sync.worker.ts` | комментарии в `sync.worker` |
| `apps/web/src/ui/server/comments.tsx`, `comments-logic.ts` (новые), `app.tsx`, `toolbar.tsx`, `router.ts`, `auth/account.tsx` | окно, поле, панель, ссылка, настройка писем |
| `apps/web/e2e-server/comments.spec.ts`, `comments-panel.spec.ts` (новые) | E2E двумя браузерами на стенде, проба панели |
| `docs/` | спека, запуск, бэклог B-407…B-418 |

---

### Task 1: Рендер — метки комментариев в оверлее

Спека §10.2, спека ядра §8.6; «Решения» 10, 18. Капля с инициалами цветом автора, счётчик ответов, отметка «не прочитано», открытая и закрытая метка, группа с числом, черновик; общая геометрия рисования и попадания; секция `render_report` с целью 1 мс.

**Files:**
- Create: `crates/ferrum-render/src/pins.rs`
- Modify: `crates/ferrum-editor/src/editor.rs`, `crates/ferrum-render/examples/render_report.rs`, `crates/ferrum-render/src/lib.rs`, `crates/ferrum-render/src/overlay.rs`
- Test: `crates/ferrum-render/tests/overlay.rs`

**Interfaces:**
- Produces: `PinMark { at: Point, kind: PinKind }`, `PinKind::{Pin { initials: Arc<str>, color: Rgba, replies: u32, unread: bool, resolved: bool, open: bool }, Cluster { count: u32, unread: bool }, Draft}`, `pin_contains(kind: &PinKind, anchor: Point, p: Point) -> bool`, `PIN_SIZE = 28.0`, `PIN_CLUSTER_SIZE = 32.0`, `PIN_BUILDS_PER_FRAME = 8`, `PIN_SPRITE_LIMIT = 256`, `PIN_REPLIES_SHOWN = 99`; `Overlay.comments: Arc<[PinMark]>`; `Renderer::pin_sprites() -> usize`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-render/tests/overlay.rs` заменить:

````rust
        assert_px(&img, (REMOTE_BOXES_MAX * 5) as u16, 15, [255, 255, 255], 0);
    }
}
````

на:

````rust
        assert_px(&img, (REMOTE_BOXES_MAX * 5) as u16, 15, [255, 255, 255], 0);
    }
}

/// Метки комментариев (спека подсистемы 4, раздел 10.2): капля с инициалами цветом автора,
/// плашка «не прочитано», группа с числом; картинками, построенными не больше нескольких за кадр.
mod comments {
    use super::common::*;
    use ferrum_render::{
        pin_contains, Camera, CpuBackend, ImagePaint, NoImages, Overlay, OverlayTheme, PinKind,
        PinMark, RenderBackend, Renderer, Rgba, PIN_BUILDS_PER_FRAME, PIN_SPRITE_LIMIT,
    };
    use ferrum_scene::Scene;
    use kurbo::{Affine, BezPath, Point, Rect, Stroke};
    use std::sync::Arc;
    use vello_cpu::peniko::Fill;

    const BLUE: [u8; 3] = [0x0d, 0x74, 0xce];

    fn pin(at: Point, initials: &str, replies: u32, unread: bool) -> PinMark {
        PinMark {
            at,
            kind: PinKind::Pin {
                initials: initials.into(),
                color: Rgba::from_rgba8(BLUE[0], BLUE[1], BLUE[2], 255),
                replies,
                unread,
                resolved: false,
                open: false,
            },
        }
    }

    fn overlay(marks: Vec<PinMark>) -> Overlay {
        Overlay {
            frame_labels: false,
            comments: Arc::from(marks),
            ..Default::default()
        }
    }

    fn frame(r: &mut Renderer, b: &mut impl RenderBackend, cam: &Camera, o: &Overlay) {
        r.render(b, &Scene::new(), cam, o, &NoImages, Some(BLACK));
    }

    /// Бэкенд без картинок: метка рисуется формой и глифами напрямую.
    struct Direct(CpuBackend);

    impl RenderBackend for Direct {
        fn begin_frame(&mut self, w: u16, h: u16) {
            self.0.begin_frame(w, h)
        }
        fn size(&self) -> (u16, u16) {
            self.0.size()
        }
        fn set_transform(&mut self, t: Affine) {
            self.0.set_transform(t)
        }
        fn set_color(&mut self, c: Rgba) {
            self.0.set_color(c)
        }
        fn set_image(&mut self, i: &ImagePaint) {
            self.0.set_image(i)
        }
        fn fill_path(&mut self, p: &BezPath, f: Fill) {
            self.0.fill_path(p, f)
        }
        fn fill_rect(&mut self, r: &Rect) {
            self.0.fill_rect(r)
        }
        fn stroke_path(&mut self, p: &BezPath, s: &Stroke) {
            self.0.stroke_path(p, s)
        }
        fn push_clip(&mut self, p: &BezPath) {
            self.0.push_clip(p)
        }
        fn pop_clip(&mut self) {
            self.0.pop_clip()
        }
        fn push_opacity_layer(&mut self, a: f32) {
            self.0.push_opacity_layer(a)
        }
        fn pop_layer(&mut self) {
            self.0.pop_layer()
        }
        fn supports_images(&self) -> bool {
            false
        }
    }

    /// Сколько пикселей прямоугольника почти белые.
    fn whites(img: &vello_cpu::Pixmap, r: (u16, u16, u16, u16)) -> usize {
        let mut n = 0;
        for y in r.1..r.3 {
            for x in r.0..r.2 {
                let p = px(img, x, y);
                if p[0] > 200 && p[1] > 200 && p[2] > 200 {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn pin_is_a_drop_over_its_point_in_author_colour_with_initials() {
        let mut cam = camera(200.0, 100.0);
        // Мир сдвинут: метка в мире (1040, 560) — на экране (40, 60).
        cam.set_center(Point::new(1100.0, 550.0));
        let o = overlay(vec![pin(Point::new(1040.0, 560.0), "АБ", 0, true)]);
        let mut r = Renderer::new();
        let mut cpu = CpuBackend::new(200, 100);
        frame(&mut r, &mut cpu, &cam, &o);
        let img = cpu.render();
        // Капля над остриём (40, 60): 28 × 28 вправо и вверх; левее и ниже — фон.
        assert_px(&img, 44, 56, BLUE, 30);
        assert_px(&img, 60, 55, BLUE, 30);
        assert_px(&img, 35, 50, [0, 0, 0], 0);
        assert_px(&img, 50, 65, [0, 0, 0], 0);
        // Инициалы белым посередине капли.
        assert!(whites(&img, (46, 38, 62, 52)) > 5, "инициалов не видно");
        // «Не прочитано» без ответов — точка цвета выделения в правом верхнем углу.
        let select = OverlayTheme::dark().select.to_rgba8().to_u8_array();
        assert_px(&img, 65, 35, [select[0], select[1], select[2]], 30);
        assert_eq!(r.pin_sprites(), 2, "капля и точка — две картинки");
    }

    #[test]
    fn cluster_is_a_dark_circle_with_count() {
        let cam = camera(200.0, 100.0);
        let o = overlay(vec![PinMark {
            at: Point::new(100.0, 50.0),
            kind: PinKind::Cluster {
                count: 12,
                unread: false,
            },
        }]);
        let img = render_overlay(&Scene::new(), &cam, &o, BLACK);
        assert_px(&img, 100, 37, [0x2c, 0x2a, 0x28], 6);
        assert!(whites(&img, (92, 44, 108, 56)) > 5, "числа не видно");
        // Белое кольцо по краю круга, за ним — фон.
        assert_px(&img, 100, 33, [255, 255, 255], 60);
        assert_px(&img, 100, 29, [0, 0, 0], 0);
    }

    #[test]
    fn hit_geometry_matches_drawing() {
        let pin = pin(Point::ZERO, "А", 0, false).kind;
        let at = Point::new(40.0, 60.0);
        assert!(pin_contains(&pin, at, Point::new(44.0, 56.0)));
        assert!(pin_contains(&pin, at, Point::new(67.0, 33.0)));
        assert!(!pin_contains(&pin, at, Point::new(39.0, 56.0)));
        assert!(!pin_contains(&pin, at, Point::new(44.0, 61.0)));
        let cluster = PinKind::Cluster {
            count: 3,
            unread: false,
        };
        assert!(pin_contains(&cluster, at, Point::new(55.0, 60.0)));
        assert!(!pin_contains(&cluster, at, Point::new(57.0, 60.0)));
    }

    #[test]
    fn off_screen_pins_are_not_drawn_and_build_nothing() {
        let cam = camera(100.0, 100.0);
        for at in [
            Point::new(5000.0, 50.0),
            Point::new(50.0, -5000.0),
            Point::new(f64::NAN, 50.0),
        ] {
            let mut r = Renderer::new();
            let mut cpu = CpuBackend::new(100, 100);
            frame(
                &mut r,
                &mut cpu,
                &cam,
                &overlay(vec![pin(at, "А", 3, true)]),
            );
            let img = cpu.render();
            for (x, y) in [(5, 5), (50, 50), (95, 95)] {
                assert_px(&img, x, y, [0, 0, 0], 0);
            }
            assert_eq!(r.pin_sprites(), 0);
        }
    }

    #[test]
    fn sprite_looks_like_direct_drawing_and_is_built_once() {
        let cam = camera(160.0, 80.0);
        let o = overlay(vec![
            pin(Point::new(20.0, 50.0), "ВК", 5, true),
            pin(Point::new(80.0, 50.0), "Г", 120, false),
            PinMark {
                at: Point::new(135.0, 40.0),
                kind: PinKind::Cluster {
                    count: 7,
                    unread: true,
                },
            },
        ]);
        let mut r = Renderer::new();
        let mut cpu = CpuBackend::new(160, 80);
        frame(&mut r, &mut cpu, &cam, &o);
        let sprite = cpu.render();
        let built = r.pin_sprites();
        assert_eq!(built, 6, "три тела и три плашки");
        frame(&mut r, &mut cpu, &cam, &o);
        assert_eq!(r.pin_sprites(), built, "те же метки — те же картинки");
        let mut direct = Direct(CpuBackend::new(160, 80));
        let mut r2 = Renderer::new();
        frame(&mut r2, &mut direct, &cam, &o);
        assert_eq!(r2.pin_sprites(), 0);
        let direct = direct.0.render();
        let mut worst = 0u8;
        for y in 0..80 {
            for x in 0..160 {
                let (a, b) = (px(&sprite, x, y), px(&direct, x, y));
                worst = worst.max((0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap());
            }
        }
        assert!(
            worst <= 8,
            "картинка метки отличается от прямого рисования на {worst}"
        );
    }

    #[test]
    fn sprites_are_built_a_few_per_frame_and_missing_ones_are_plain() {
        let cam = camera(400.0, 100.0);
        let marks: Vec<_> = (0..30)
            .map(|i| {
                pin(
                    Point::new(5.0 + i as f64 * 13.0, 60.0),
                    &format!("{i}"),
                    0,
                    false,
                )
            })
            .collect();
        let last = marks.last().unwrap().at;
        let o = overlay(marks);
        let mut r = Renderer::new();
        let mut cpu = CpuBackend::new(400, 100);
        frame(&mut r, &mut cpu, &cam, &o);
        assert_eq!(r.pin_sprites(), PIN_BUILDS_PER_FRAME);
        // Метка без картинки в этом кадре всё равно видна: заливкой цвета автора.
        let img = cpu.render();
        assert_px(&img, last.x as u16 + 4, last.y as u16 - 4, BLUE, 30);
        for _ in 0..3 {
            frame(&mut r, &mut cpu, &cam, &o);
        }
        assert_eq!(r.pin_sprites(), 30);
    }

    #[test]
    fn sprite_cache_is_bounded() {
        let cam = camera(800.0, 800.0);
        let marks: Vec<_> = (0..PIN_SPRITE_LIMIT + 100)
            .map(|i| {
                pin(
                    Point::new(5.0 + (i % 30) as f64 * 26.0, 30.0 + (i / 30) as f64 * 26.0),
                    &format!("{i}"),
                    0,
                    false,
                )
            })
            .collect();
        let o = overlay(marks);
        let mut r = Renderer::new();
        let mut cpu = CpuBackend::new(800, 800);
        for _ in 0..(PIN_SPRITE_LIMIT + 100) / PIN_BUILDS_PER_FRAME + 5 {
            frame(&mut r, &mut cpu, &cam, &o);
        }
        assert_eq!(r.pin_sprites(), PIN_SPRITE_LIMIT);
    }
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-render --test overlay`
Expected: FAIL — не собирается: нет `PinMark`, `PinKind`, `pin_contains`, `PIN_*`, поля `Overlay::comments` и `Renderer::pin_sprites`, 10 ошибок компиляции

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            active_guide: self.active_guide(),
            remote: self.remote_marks(),
        }
    }
````

на:

````rust
            active_guide: self.active_guide(),
            remote: self.remote_marks(),
            // Метки комментариев раскладывает редактор (этап 5, следующая задача).
            comments: Arc::from(Vec::new()),
        }
    }
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{
    Camera, CpuBackend, GuideMark, NoImages, Overlay, RemoteMark, Renderer, Rgba, RulersOverlay,
    TextEngine, LABEL_SIZE,
};
use ferrum_scene::Scene;
````

на:

````rust
use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{
    Camera, CpuBackend, GuideMark, NoImages, Overlay, PinKind, PinMark, RemoteMark, Renderer, Rgba,
    RulersOverlay, TextEngine, LABEL_SIZE,
};
use ferrum_scene::Scene;
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
}

/// Экспорт SVG и PDF (подсистема 3, этап 5): SVG страницы из 10 000 узлов
/// (фреймы по 100: фрейм + 99 фигур) и PDF страницы с текстом и вектором в
````

на:

````rust
}

/// Метки комментариев (спека подсистемы 4, раздел 10.2): 1000 обсуждений на странице. Зум 1 —
/// все 1000 меток на экране сеткой (худший случай: слипшихся нет, каждая — своя картинка и, у
/// трети, плашка счётчика), каждый кадр метки сдвигаются, как при перетаскивании их узлов.
/// Мелкий зум — те же метки, собранные редактором в группы: 80 кругов с числом и 20 одиночных.
/// Цель — прибавка к кадру не больше 1 мс, как у присутствия. Картинки строятся в прогреве
/// (не больше `PIN_BUILDS_PER_FRAME` за кадр); первый кадр без картинок — справочно.
fn comments_section(report: &mut Report, scene: &Scene, base: &Camera) {
    const RUNS: usize = 61;
    const WARM: usize = 40;
    let palette: [u32; 10] = [
        0xe5484d, 0xf76b15, 0x2d8a3e, 0x0d74ce, 0x8e4ec6, 0xd6409f, 0x12a594, 0x5b5bd6, 0xab6400,
        0x3e63dd,
    ];
    let initials = ["АС", "Б", "ВК", "ГЛ", "Д"];
    let pin = |i: usize, at: kurbo::Point| {
        let c = palette[i % palette.len()];
        PinMark {
            at,
            kind: PinKind::Pin {
                // 50 авторов: инициалы и цвет.
                initials: format!("{}{}", initials[i % 5], (i / 5) % 10).into(),
                color: Rgba::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, 255),
                replies: if i.is_multiple_of(3) {
                    (i % 17) as u32
                } else {
                    0
                },
                unread: i.is_multiple_of(4),
                resolved: false,
                open: i == 7,
            },
        }
    };
    for dpr in [1.0, 2.0] {
        let mut camera = Camera::new(VIEWPORT, dpr);
        camera.set_zoom(base.zoom());
        camera.set_center(base.center());
        let view = camera.visible_world_rect();
        let grid = |t: f64| -> Vec<PinMark> {
            (0..1000)
                .map(|i| {
                    let (col, row) = ((i % 40) as f64, (i / 40) as f64);
                    let at = kurbo::Point::new(
                        view.x0 + view.width() * (col + 0.2) / 40.0 + t,
                        view.y0 + view.height() * (row + 0.8) / 25.0 + t * 0.5,
                    );
                    pin(i, at)
                })
                .collect()
        };
        let clustered = |t: f64| -> Vec<PinMark> {
            let mut out: Vec<PinMark> = (0..80)
                .map(|i| PinMark {
                    at: kurbo::Point::new(
                        view.x0 + view.width() * ((i % 10) as f64 + 0.5) / 10.0 + t,
                        view.y0 + view.height() * ((i / 10) as f64 + 0.5) / 8.0,
                    ),
                    kind: PinKind::Cluster {
                        count: 6 + (i % 20) as u32,
                        unread: i % 3 == 0,
                    },
                })
                .collect();
            out.extend((0..20).map(|i| {
                pin(
                    i,
                    kurbo::Point::new(
                        view.x0 + view.width() * (i as f64 + 0.25) / 20.0 + t,
                        view.y0 + view.height() * 0.97,
                    ),
                )
            }));
            out
        };
        let (w, h) = camera.device_size();
        let mut backend = CpuBackend::new(w, h);
        for (name, marks) in [
            (
                "1000 меток на экране, зум 1",
                &grid as &dyn Fn(f64) -> Vec<PinMark>,
            ),
            ("1000 меток в 80 группах и 20 меток, мелкий зум", &clustered),
        ] {
            let mut renderer = Renderer::new();
            let plain = Overlay::default();
            let mut with = Overlay {
                comments: marks(0.0).into(),
                ..Default::default()
            };
            renderer.render(&mut backend, scene, &camera, &plain, &NoImages, None);
            // Первый кадр: картинок ещё нет, метки — простой заливкой (справочно).
            backend.reset(w, h);
            let t = Instant::now();
            renderer.render(&mut backend, scene, &camera, &with, &NoImages, None);
            let cold = t.elapsed();
            for i in 0..WARM {
                with.comments = marks(i as f64 * 0.37).into();
                backend.reset(w, h);
                renderer.render(&mut backend, scene, &camera, &with, &NoImages, None);
            }
            report.info(
                &format!("Метки комментариев: {name}, dpr {dpr}: картинок после прогрева"),
                format!("{}", renderer.pin_sprites()),
            );
            let (mut off, mut on) = (Vec::new(), Vec::new());
            for i in 0..RUNS {
                with.comments = marks(i as f64 * 0.37).into();
                for (overlay, runs) in [(&plain, &mut off), (&with, &mut on)] {
                    backend.reset(w, h);
                    let t = Instant::now();
                    renderer.render(&mut backend, scene, &camera, overlay, &NoImages, None);
                    runs.push(t.elapsed());
                }
            }
            let base_ms = median(off.clone());
            report.gate(
                &format!(
                    "Кадр с метками комментариев: {name}: прибавка, {}",
                    if dpr == 1.0 {
                        "1920×1080"
                    } else {
                        "3840×2160 (2×)"
                    }
                ),
                median(on).saturating_sub(base_ms),
                1.0,
                "≤ 1 мс",
            );
            report.info(
                &format!(
                    "Метки комментариев: {name}, dpr {dpr}: первый кадр без картинок, прибавка"
                ),
                ms(cold.saturating_sub(base_ms)),
            );
            // С растеризацией CPU — справочно: на GPU растеризация не на этом потоке.
            let empty = Scene::new();
            let (mut off, mut on) = (Vec::new(), Vec::new());
            for i in 0..RUNS {
                with.comments = marks(i as f64 * 0.37).into();
                for (overlay, runs) in [(&plain, &mut off), (&with, &mut on)] {
                    backend.reset(w, h);
                    let t = Instant::now();
                    renderer.render(&mut backend, &empty, &camera, overlay, &NoImages, None);
                    std::hint::black_box(backend.render());
                    runs.push(t.elapsed());
                }
            }
            report.info(
                &format!("Метки комментариев: {name}, dpr {dpr}: прибавка с растеризацией CPU"),
                ms(median(on).saturating_sub(median(off))),
            );
        }
    }
}

/// Экспорт SVG и PDF (подсистема 3, этап 5): SVG страницы из 10 000 узлов
/// (фреймы по 100: фрейм + 99 фигур) и PDF страницы с текстом и вектором в
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
    rulers_section(&mut report, &scene, &camera);
    presence_section(&mut report, &scene, &camera);

    // Вся страница при сильном отдалении: LOD и растровый кэш фреймов. Кэш
````

на:

````rust
    rulers_section(&mut report, &scene, &camera);
    presence_section(&mut report, &scene, &camera);
    comments_section(&mut report, &scene, &camera);

    // Вся страница при сильном отдалении: LOD и растровый кэш фреймов. Кэш
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
mod overlay;
mod pdf;
mod svg;
mod text;
````

на:

````rust
mod overlay;
mod pdf;
mod pins;
mod svg;
mod text;
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
};
pub use pdf::MAX_PDF_SIDE;
pub use svg::{ImageBytes, NoImageBytes, SvgOptions};
pub use text::{Label, TextEngine, ONEST};
````

на:

````rust
};
pub use pdf::MAX_PDF_SIDE;
pub use pins::{
    pin_contains, PinKind, PinMark, PIN_BUILDS_PER_FRAME, PIN_CLUSTER_SIZE, PIN_REPLIES_SHOWN,
    PIN_SIZE, PIN_SPRITE_LIMIT,
};
pub use svg::{ImageBytes, NoImageBytes, SvgOptions};
pub use text::{Label, TextEngine, ONEST};
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    /// Картинки чужих курсоров (`overlay::CursorSprites`).
    sprites: overlay::CursorSprites,
    /// Реестр шрифтов и раскладка текстовых узлов — общие с редактором
    /// (`set_fonts`); свой рендерер без реестра знает только вшитый Onest.
````

на:

````rust
    /// Картинки чужих курсоров (`overlay::CursorSprites`).
    sprites: overlay::CursorSprites,
    /// Картинки меток комментариев (`pins::PinSprites`).
    pins: pins::PinSprites,
    /// Реестр шрифтов и раскладка текстовых узлов — общие с редактором
    /// (`set_fonts`); свой рендерер без реестра знает только вшитый Onest.
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
            text: TextEngine::new(),
            sprites: Default::default(),
            fonts: Fonts::shared(),
        }
````

на:

````rust
            text: TextEngine::new(),
            sprites: Default::default(),
            pins: Default::default(),
            fonts: Fonts::shared(),
        }
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
            &mut self.text,
            &mut self.sprites,
            scene,
            camera,
````

на:

````rust
            &mut self.text,
            &mut self.sprites,
            &mut self.pins,
            scene,
            camera,
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    pub fn cursor_sprites(&self) -> usize {
        self.sprites.len()
    }

````

на:

````rust
    pub fn cursor_sprites(&self) -> usize {
        self.sprites.len()
    }

    /// Сколько картинок меток комментариев построено и хранится (для проверки кэша).
    pub fn pin_sprites(&self) -> usize {
        self.pins.len()
    }

````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
//! фреймов, направляющие, подсветка наведения, рамка выделения мышью, линии привязки,
//! рамка выделения с ручками, бейдж «W × H», измерения расстояний с Alt, линейки и
//! участники совместной работы: их курсоры с подписью и рамки выделения.
//! Толщины и размеры задаются в CSS-пикселях и умножаются на `devicePixelRatio`.

````

на:

````rust
//! фреймов, направляющие, подсветка наведения, рамка выделения мышью, линии привязки,
//! рамка выделения с ручками, бейдж «W × H», измерения расстояний с Alt, линейки и
//! участники совместной работы: их курсоры с подписью и рамки выделения; метки комментариев
//! (`pins`).
//! Толщины и размеры задаются в CSS-пикселях и умножаются на `devicePixelRatio`.

````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
use crate::backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::camera::Camera;
use crate::text::{Label, TextEngine};

````

на:

````rust
use crate::backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::camera::Camera;
use crate::pins::{draw_pins, PinMark, PinSprites};
use crate::text::{Label, TextEngine};

````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
    /// Другие участники файла на этой странице (спека подсистемы 4, раздел 9).
    pub remote: Arc<[RemoteMark]>,
}

````

на:

````rust
    /// Другие участники файла на этой странице (спека подсистемы 4, раздел 9).
    pub remote: Arc<[RemoteMark]>,
    /// Метки комментариев этой страницы, уже разложенные редактором (раздел 10.2): над
    /// выделением и измерениями, под чужими курсорами и линейками.
    pub comments: Arc<[PinMark]>,
}

````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
            active_guide: None,
            remote: Arc::from(Vec::new()),
        }
    }
````

на:

````rust
            active_guide: None,
            remote: Arc::from(Vec::new()),
            comments: Arc::from(Vec::new()),
        }
    }
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
    text: &mut TextEngine,
    sprites: &mut CursorSprites,
    scene: &Scene,
    camera: &Camera,
````

на:

````rust
    text: &mut TextEngine,
    sprites: &mut CursorSprites,
    pins: &mut PinSprites,
    scene: &Scene,
    camera: &Camera,
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
    remote_cursors(backend, text, sprites, view, dpr, &overlay.remote);
    if let Some(r) = &overlay.rulers {
````

на:

````rust
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
    draw_pins(
        backend,
        text,
        pins,
        view,
        dpr,
        theme.select,
        &overlay.comments,
    );
    remote_cursors(backend, text, sprites, view, dpr, &overlay.remote);
    if let Some(r) = &overlay.rulers {
````

Создать `crates/ferrum-render/src/pins.rs`:

````rust
//! Метки комментариев на холсте (спека подсистемы 4, раздел 10.2): капля с инициалами автора
//! цветом автора, счётчик ответов и отметка «не прочитано», открытая метка обведена цветом
//! выделения, закрытое обсуждение — серое; метки, слипшиеся на экране, — круг с числом. Где метки
//! стоят и какие слиплись, решает редактор (раскладка по сцене и камере); здесь — только как они
//! выглядят. Метки рисуются только на экране: превью и экспорт оверлеев не рисуют вовсе.
//!
//! Каждая часть метки (капля, группа, плашка счётчика) — готовая картинка, как чужой курсор
//! (`overlay::CursorSprites`): глифы инициалов и обводка у 1000 меток каждый кадр стоили бы
//! больше всей цели кадра. Картинок строится не больше `PIN_BUILDS_PER_FRAME` за кадр, хранится не
//! больше `PIN_SPRITE_LIMIT`: остальные метки кадра — простой заливкой формы без текста (дёшево),
//! следующие кадры — картинкой.

use std::collections::HashMap;
use std::sync::Arc;

use kurbo::{
    Affine, BezPath, Circle, Point, Rect, RoundedRect, RoundedRectRadii, Shape, Stroke, Vec2,
};
use vello_cpu::peniko::{Extend, Fill};
use vello_cpu::Pixmap;

use crate::backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::text::TextEngine;

/// Сторона капли метки — CSS-пиксели. Остриё капли (левый нижний угол) — точка метки.
pub const PIN_SIZE: f64 = 28.0;
/// Диаметр круга группы меток — CSS-пиксели; центр круга — точка группы.
pub const PIN_CLUSTER_SIZE: f64 = 32.0;
/// Столько картинок меток строится за кадр (около 0,1 мс каждая).
pub const PIN_BUILDS_PER_FRAME: usize = 8;
/// Столько картинок меток хранится: авторов на странице — десятки, у каждого 4 вида капли
/// (открыта, закрыта), плашек счётчика — до 200. Сверх предела метка рисуется простой заливкой.
pub const PIN_SPRITE_LIMIT: usize = 256;
/// Счётчик ответов больше этого показывается как «99+».
pub const PIN_REPLIES_SHOWN: u32 = 99;

/// Метка комментария или группа слипшихся меток в мировых координатах.
#[derive(Debug, Clone, PartialEq)]
pub struct PinMark {
    /// У метки — остриё капли, у группы — центр круга.
    pub at: Point,
    pub kind: PinKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PinKind {
    /// Обсуждение: инициалы и цвет автора, ответов, не прочитано, закрыто, открыто сейчас.
    Pin {
        initials: Arc<str>,
        color: Rgba,
        replies: u32,
        unread: bool,
        resolved: bool,
        open: bool,
    },
    /// Слипшиеся метки: сколько и есть ли среди них непрочитанные.
    Cluster { count: u32, unread: bool },
    /// Метка нового комментария, пока его пишут: капля цвета выделения без текста.
    Draft,
}

/// Попадает ли экранная точка `p` в метку с экранной точкой `anchor` (CSS-пиксели): капля —
/// квадрат над остриём, группа — круг. Одна геометрия у рисования и у попадания.
pub fn pin_contains(kind: &PinKind, anchor: Point, p: Point) -> bool {
    match kind {
        PinKind::Cluster { .. } => p.distance(anchor) <= PIN_CLUSTER_SIZE / 2.0,
        PinKind::Pin { .. } | PinKind::Draft => {
            let d = p - anchor;
            (0.0..=PIN_SIZE).contains(&d.x) && (-PIN_SIZE..=0.0).contains(&d.y)
        }
    }
}

/// Часть метки — одна картинка. Цвета — байтами (ключ кэша).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Part {
    Bubble {
        initials: Arc<str>,
        color: [u8; 4],
        resolved: bool,
        /// Открыта сейчас: кольцо цвета выделения.
        open: Option<[u8; 4]>,
    },
    Draft {
        color: [u8; 4],
    },
    Cluster {
        /// Не больше `PIN_REPLIES_SHOWN + 1` («99+»).
        count: u32,
    },
    /// Плашка в правом верхнем углу: число ответов (цвет выделения — есть непрочитанное),
    /// без ответов — точка «не прочитано».
    Badge {
        count: u32,
        unread: bool,
        accent: [u8; 4],
    },
}

/// Серый закрытого обсуждения и тёмный круг группы — нейтральные цвета дизайн-системы.
const RESOLVED: Rgba = Rgba::new([
    0x8f as f32 / 255.0,
    0x8a as f32 / 255.0,
    0x84 as f32 / 255.0,
    1.0,
]);
const CLUSTER: Rgba = Rgba::new([
    0x2c as f32 / 255.0,
    0x2a as f32 / 255.0,
    0x28 as f32 / 255.0,
    1.0,
]);
const BADGE: Rgba = Rgba::new([
    0x3a as f32 / 255.0,
    0x37 as f32 / 255.0,
    0x35 as f32 / 255.0,
    1.0,
]);
const WHITE: Rgba = Rgba::new([1.0, 1.0, 1.0, 1.0]);

fn rgba(c: [u8; 4]) -> Rgba {
    Rgba::from_rgba8(c[0], c[1], c[2], c[3])
}

/// Текст на заливке: тёмный на светлом, иначе белый (как у подписи чужого курсора).
fn ink_on(c: Rgba) -> Rgba {
    let [r, g, b, _] = c.components;
    if 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.6 {
        Rgba::from_rgba8(0x1a, 0x19, 0x18, 255)
    } else {
        WHITE
    }
}

fn count_text(n: u32) -> String {
    if n > PIN_REPLIES_SHOWN {
        format!("{PIN_REPLIES_SHOWN}+")
    } else {
        n.to_string()
    }
}

/// Части метки и где их начала относительно точки метки (пиксели устройства).
fn parts(kind: &PinKind, accent: [u8; 4], dpr: f64) -> [Option<(Part, Vec2)>; 2] {
    let cap = |n: u32| n.min(PIN_REPLIES_SHOWN + 1);
    match kind {
        PinKind::Pin {
            initials,
            color,
            replies,
            unread,
            resolved,
            open,
        } => {
            let body = Part::Bubble {
                initials: initials.clone(),
                color: color.to_rgba8().to_u8_array(),
                resolved: *resolved,
                // Цвет кольца нужен ключу только у открытой: у закрытых он не рисуется.
                open: open.then_some(accent),
            };
            let badge = (*replies > 0 || *unread).then(|| {
                (
                    Part::Badge {
                        count: cap(*replies),
                        unread: *unread,
                        accent,
                    },
                    Vec2::new((PIN_SIZE - 3.0) * dpr, -(PIN_SIZE - 3.0) * dpr),
                )
            });
            [Some((body, Vec2::ZERO)), badge]
        }
        PinKind::Cluster { count, unread } => {
            let r = PIN_CLUSTER_SIZE / 2.0 * 0.72 * dpr;
            let badge = unread.then(|| {
                (
                    Part::Badge {
                        count: 0,
                        unread: true,
                        accent,
                    },
                    Vec2::new(r, -r),
                )
            });
            [
                Some((Part::Cluster { count: cap(*count) }, Vec2::ZERO)),
                badge,
            ]
        }
        PinKind::Draft => [Some((Part::Draft { color: accent }, Vec2::ZERO)), None],
    }
}

/// Капля: квадрат над остриём `o` со скруглёнными углами, кроме левого нижнего.
fn bubble_path(o: Point, dpr: f64) -> BezPath {
    let s = PIN_SIZE * dpr;
    RoundedRect::from_rect(
        Rect::new(o.x, o.y - s, o.x + s, o.y),
        RoundedRectRadii::new(s / 2.0, s / 2.0, s / 2.0, 0.0),
    )
    .to_path(0.1)
}

/// Плашка счётчика с центром в `o`: её прямоугольник и подпись.
fn badge_shape(
    text: &mut TextEngine,
    count: u32,
    o: Point,
    dpr: f64,
) -> (BezPath, Option<Arc<crate::text::Label>>) {
    if count == 0 {
        return (Circle::new(o, 4.5 * dpr).to_path(0.1), None);
    }
    let label = text.label(&count_text(count), 9.0 * dpr, 600, 40.0 * dpr);
    let h = 14.0 * dpr;
    let w = label.as_ref().map_or(h, |l| (l.width + 8.0 * dpr).max(h));
    let r = Rect::from_center_size(o, (w, h));
    (RoundedRect::from_rect(r, h / 2.0).to_path(0.1), label)
}

/// Рисует часть с началом в `o` (пиксели устройства).
fn paint(backend: &mut impl RenderBackend, text: &mut TextEngine, part: &Part, o: Point, dpr: f64) {
    backend.set_transform(Affine::IDENTITY);
    match part {
        Part::Bubble {
            initials,
            color,
            resolved,
            open,
        } => {
            let path = bubble_path(o, dpr);
            let fill = if *resolved { RESOLVED } else { rgba(*color) };
            // Открытая метка — широкое кольцо цвета выделения под белым (как выбранная в Figma).
            // Обводка — до заливки: внутренняя половина штриха под ней.
            if let Some(ring) = open {
                backend.set_color(rgba(*ring));
                backend.stroke_path(&path, &Stroke::new(8.0 * dpr));
            }
            backend.set_color(WHITE);
            backend.stroke_path(&path, &Stroke::new(3.0 * dpr));
            backend.set_color(fill);
            backend.fill_path(&path, Fill::NonZero);
            let s = PIN_SIZE * dpr;
            if let Some(l) = text.label(initials, 11.0 * dpr, 600, s - 6.0 * dpr) {
                let base = o.y - s / 2.0 + (l.ascent - l.descent) / 2.0;
                l.draw(
                    backend,
                    Point::new(o.x + (s - l.width) / 2.0, base),
                    ink_on(fill),
                );
            }
        }
        Part::Draft { color } => {
            let path = bubble_path(o, dpr);
            backend.set_color(WHITE);
            backend.stroke_path(&path, &Stroke::new(3.0 * dpr));
            backend.set_color(rgba(*color));
            backend.fill_path(&path, Fill::NonZero);
        }
        Part::Cluster { count } => {
            let c = Circle::new(o, PIN_CLUSTER_SIZE / 2.0 * dpr).to_path(0.1);
            backend.set_color(WHITE);
            backend.stroke_path(&c, &Stroke::new(3.0 * dpr));
            backend.set_color(CLUSTER);
            backend.fill_path(&c, Fill::NonZero);
            if let Some(l) =
                text.label(&count_text(*count), 12.0 * dpr, 600, PIN_CLUSTER_SIZE * dpr)
            {
                let base = o.y + (l.ascent - l.descent) / 2.0;
                l.draw(backend, Point::new(o.x - l.width / 2.0, base), WHITE);
            }
        }
        Part::Badge {
            count,
            unread,
            accent,
        } => {
            let (path, label) = badge_shape(text, *count, o, dpr);
            let fill = if *unread { rgba(*accent) } else { BADGE };
            backend.set_transform(Affine::IDENTITY);
            backend.set_color(WHITE);
            backend.stroke_path(&path, &Stroke::new(3.0 * dpr));
            backend.set_color(fill);
            backend.fill_path(&path, Fill::NonZero);
            if let Some(l) = label {
                let base = o.y + (l.ascent - l.descent) / 2.0;
                l.draw(backend, Point::new(o.x - l.width / 2.0, base), ink_on(fill));
            }
        }
    }
}

/// Дешёвая замена картинки, пока её нет (предел построений за кадр или кэш полон): форма
/// одной заливкой, без обводки и текста. Плашки нет.
fn paint_plain(backend: &mut impl RenderBackend, part: &Part, o: Point, dpr: f64) {
    backend.set_transform(Affine::IDENTITY);
    match part {
        Part::Bubble {
            color, resolved, ..
        } => {
            backend.set_color(if *resolved { RESOLVED } else { rgba(*color) });
            backend.fill_path(&bubble_path(o, dpr), Fill::NonZero);
        }
        Part::Draft { color } => {
            backend.set_color(rgba(*color));
            backend.fill_path(&bubble_path(o, dpr), Fill::NonZero);
        }
        Part::Cluster { .. } => {
            backend.set_color(CLUSTER);
            backend.fill_path(
                &Circle::new(o, PIN_CLUSTER_SIZE / 2.0 * dpr).to_path(0.1),
                Fill::NonZero,
            );
        }
        Part::Badge { .. } => {}
    }
}

/// Границы части относительно её начала (пиксели устройства), с полем под обводку.
fn part_bounds(text: &mut TextEngine, part: &Part, dpr: f64) -> Rect {
    let m = 4.0 * dpr;
    match part {
        Part::Bubble { open: Some(_), .. } => {
            let s = PIN_SIZE * dpr;
            Rect::new(0.0, -s, s, 0.0).inflate(m + 2.0 * dpr, m + 2.0 * dpr)
        }
        Part::Bubble { .. } | Part::Draft { .. } => {
            let s = PIN_SIZE * dpr;
            Rect::new(0.0, -s, s, 0.0).inflate(m, m)
        }
        Part::Cluster { .. } => {
            let r = PIN_CLUSTER_SIZE / 2.0 * dpr;
            Rect::new(-r, -r, r, r).inflate(m, m)
        }
        Part::Badge { count, .. } => badge_shape(text, *count, Point::ZERO, dpr)
            .0
            .bounding_box()
            .inflate(m, m),
    }
}

#[derive(Clone)]
struct Sprite {
    key: ImageKey,
    pixmap: Arc<Pixmap>,
    /// Где в картинке начало части, пиксели устройства (целые).
    origin: Vec2,
    used: u64,
}

fn build(text: &mut TextEngine, part: &Part, dpr: f64) -> Sprite {
    let b = part_bounds(text, part, dpr);
    let origin = Vec2::new(-b.x0.floor(), -b.y0.floor());
    let (w, h) = (
        (b.x1 + origin.x).ceil().max(1.0) as u16,
        (b.y1 + origin.y).ceil().max(1.0) as u16,
    );
    let mut cpu = CpuBackend::new(w, h);
    paint(&mut cpu, text, part, origin.to_point(), dpr);
    Sprite {
        key: ImageKey::next_raster(),
        pixmap: Arc::new(cpu.render()),
        origin,
        used: 0,
    }
}

/// Картинки частей меток по виду и плотности пикселей.
#[derive(Default)]
pub(crate) struct PinSprites {
    map: HashMap<(Part, u32), Sprite>,
    frame: u64,
}

impl PinSprites {
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    fn get(&mut self, key: &(Part, u32)) -> Option<Sprite> {
        let frame = self.frame;
        self.map.get_mut(key).map(|s| {
            s.used = frame;
            s.clone()
        })
    }

    /// Есть ли место для новой картинки: давно не нужные уходят первыми, нужные в этом или
    /// прошлом кадре остаются — тогда сверх предела новая не строится.
    fn room(&mut self) -> bool {
        if self.map.len() >= PIN_SPRITE_LIMIT {
            let frame = self.frame;
            self.map.retain(|_, s| s.used + 1 >= frame);
        }
        self.map.len() < PIN_SPRITE_LIMIT
    }
}

/// Метки комментариев в порядке списка (последняя — сверху). Метка за краем экрана (с плашкой)
/// не рисуется и картинку не строит.
pub(crate) fn draw_pins(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    sprites: &mut PinSprites,
    view: Affine,
    dpr: f64,
    accent: Rgba,
    marks: &[PinMark],
) {
    if marks.is_empty() {
        return;
    }
    sprites.frame += 1;
    let accent = accent.to_rgba8().to_u8_array();
    let images = backend.supports_images();
    let (w, h) = backend.size();
    let reach = (PIN_SIZE + PIN_CLUSTER_SIZE) * dpr;
    let screen = Rect::new(0.0, 0.0, w as f64, h as f64).inflate(reach, reach);
    let scale = (dpr * 64.0).round() as u32;
    let mut builds = 0;
    for m in marks {
        let at = view * m.at;
        if !(at.x.is_finite() && at.y.is_finite() && screen.contains(at)) {
            continue;
        }
        // Картинка ложится пиксель в пиксель: точка метки — на целом пикселе.
        let at = Point::new(at.x.round(), at.y.round());
        for (part, offset) in parts(&m.kind, accent, dpr).into_iter().flatten() {
            let o = at + Vec2::new(offset.x.round(), offset.y.round());
            if !images {
                paint(backend, text, &part, o, dpr);
                continue;
            }
            let key = (part, scale);
            let mut sprite = sprites.get(&key);
            if sprite.is_none() && builds < PIN_BUILDS_PER_FRAME && sprites.room() {
                builds += 1;
                let s = build(text, &key.0, dpr);
                sprites.map.insert(
                    key.clone(),
                    Sprite {
                        used: sprites.frame,
                        ..s.clone()
                    },
                );
                sprite = Some(s);
            }
            match sprite {
                Some(s) => {
                    let origin = o - s.origin;
                    let (sw, sh) = (s.pixmap.width() as f64, s.pixmap.height() as f64);
                    backend.set_transform(Affine::IDENTITY);
                    backend.set_image(&ImagePaint {
                        key: s.key,
                        pixmap: s.pixmap,
                        transform: Affine::translate(origin.to_vec2()),
                        extend: Extend::Pad,
                        alpha: 1.0,
                    });
                    backend.fill_rect(&Rect::new(origin.x, origin.y, origin.x + sw, origin.y + sh));
                }
                None => paint_plain(backend, &key.0, o, dpr),
            }
        }
    }
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-render -p ferrum-editor`

Expected: PASS: fmt и clippy без замечаний; `ferrum-render` и `ferrum-editor` 779 passed

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/editor.rs crates/ferrum-render/examples/render_report.rs crates/ferrum-render/src/lib.rs crates/ferrum-render/src/overlay.rs crates/ferrum-render/src/pins.rs crates/ferrum-render/tests/overlay.rs
git commit -F - <<'EOF'
рендер: метки комментариев в оверлее

Overlay.comments — метки комментариев страницы в мировых координатах (PinMark:
капля с инициалами цветом автора, плашка счётчика ответов и «не прочитано»,
открытая — в кольце цвета выделения, закрытое обсуждение — серое; группа
слипшихся меток — тёмный круг с числом; черновик нового комментария).
Рисуются над выделением и измерениями, под чужими курсорами и линейками;
превью и экспорт оверлеев не рисуют. Каждая часть метки — готовая картинка,
как чужой курсор: строится не больше PIN_BUILDS_PER_FRAME (8) за кадр,
хранится не больше PIN_SPRITE_LIMIT (256); без картинки метка этого кадра —
простой заливкой формы. pin_contains — общая геометрия рисования и попадания.

render_report, «Кадр с метками комментариев» (прибавка к спискам кадра,
медиана 61 кадра, метки сдвигаются каждый кадр):
- 1000 меток на экране, зум 1: 0.298 мс (1920×1080), 0.517 мс (2×) — цель ≤ 1 мс;
- 80 групп и 20 меток, мелкий зум: 0.044 мс (1×), 0.102 мс (2×);
- первый кадр без картинок (раз): 2.2 мс (1×), 2.6 мс (2×) — справочно.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 2: Редактор — режим «Комментарий», Shift+C, раскладка и жесты меток

Спека §10.2, §10.3; «Решения» 10, 11. Метки не в документе и не в истории; раскладка каждый кадр по сцене (своё и чужое живое превью тоже), отсечение за краем холста, группы в экранных пикселях; команды доступны только после `set_comments_available(true)`. Проба `editor_report` с целью 1 мс. В автономном E2E число команд палитры 88 → 90.

**Files:**
- Create: `crates/ferrum-editor/src/comments.rs`, `crates/ferrum-editor/src/tools/comment.rs`
- Modify: `crates/ferrum-editor/examples/editor_report.rs`, `crates/ferrum-editor/src/actions.rs`, `crates/ferrum-editor/src/command_state.rs`, `crates/ferrum-editor/src/commands.rs`, `crates/ferrum-editor/src/editor.rs`, `crates/ferrum-editor/src/lib.rs`, `crates/ferrum-editor/src/menu.rs`, `crates/ferrum-editor/src/remote.rs`, `crates/ferrum-editor/src/tools/mod.rs`, `crates/ferrum-wasm/src/core.rs`, `crates/ferrum-wasm/src/protocol.rs`
- Test: `apps/web/e2e/app.spec.ts`, `crates/ferrum-editor/tests/command_table.rs`, `crates/ferrum-editor/tests/comments.rs` (новый), `crates/ferrum-editor/tests/keymap.rs`, `crates/ferrum-editor/tests/main_menu.rs`, `crates/ferrum-wasm/tests/boolean.rs`

**Interfaces:**
- Consumes: `PinMark`, `PinKind`, `pin_contains`, `PIN_SIZE` задачи 1.
- Produces: `PinAnchor { page, node: Option<NodeId>, local, world }`, `CommentPin { id: Arc<str>, anchor, initials, color, replies, unread, resolved, movable }`, `COMMENT_PINS_MAX = 10_000`, `PIN_CLUSTER_DISTANCE`, `PIN_CLUSTER_FIT_PADDING`; `ToolKind::Comment`; `CommandId::ToolComment` (`tool.comment`, C), `CommandId::ViewComments` (`view.comments`, Shift+C); `Editor::{set_comments_available, comments_available, comments_visible, set_comments_show_resolved, set_comment_pins, remove_comment_pins, comment_pin, comment_pin_count, set_comment_open, comment_open, set_comment_draft, comment_focus, comment_layout_for_tests}`; `EditorEvent::{CommentPlace { anchor, screen }, CommentOpened { id, screen }, CommentMoved { id, anchor }, CommentAnchor { screen, visible }, CommentsState { mode, visible, available }}`; в `ferrum-wasm` — `Tool::Comment`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/app.spec.ts` заменить:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 88, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

на:

````ts
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 90, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

````

В `crates/ferrum-editor/tests/command_table.rs` заменить:

````rust
            C::ViewRulers,
            C::ViewGuides,
            C::UiPixelSnap,
            C::UiThemeDark,
````

на:

````rust
            C::ViewRulers,
            C::ViewGuides,
            C::ViewComments,
            C::UiPixelSnap,
            C::UiThemeDark,
````

Создать `crates/ferrum-editor/tests/comments.rs`:

````rust
//! Метки комментариев (спека подсистемы 4, разделы 10.2 и 10.3): режим «Комментарий» (C),
//! Shift+C, привязка к верхнему узлу, раскладка за сценой (со своим и чужим превью), группы при
//! мелком зуме, попадание раньше выделения, перетаскивание своей метки, переход к обсуждению.
//! Метки — не документ: ни правок, ни шагов истории, ни `own_change_counter`.

mod common;

use std::sync::Arc;

use common::*;
use ferrum_doc::{NodeId, PageId, PropsPatch};
use ferrum_editor::*;
use ferrum_render::{PinKind, Rgba};
use kurbo::{Affine, Point, Rect, Vec2};

fn anchor(page: PageId, node: Option<NodeId>, local: (f64, f64), world: (f64, f64)) -> PinAnchor {
    PinAnchor {
        page,
        node,
        local: local.into(),
        world: world.into(),
    }
}

fn pin(id: &str, anchor: PinAnchor, movable: bool) -> CommentPin {
    CommentPin {
        id: Arc::from(id),
        anchor,
        initials: Arc::from("АБ"),
        color: Rgba::from_rgba8(0x0d, 0x74, 0xce, 255),
        replies: 0,
        unread: false,
        resolved: false,
        movable,
    }
}

/// Стенд с комментариями: файл на сервере, роль пришла.
fn available() -> H {
    let mut h = H::new();
    h.ed.set_comments_available(true);
    h.ed.take_events();
    h
}

/// Метка в мире без узла.
fn world_pin(h: &H, id: &str, x: f64, y: f64, movable: bool) -> CommentPin {
    pin(id, anchor(h.ed.page(), None, (x, y), (x, y)), movable)
}

/// Экранные точки меток раскладки (не групп).
fn pins_on_screen(h: &H) -> Vec<Point> {
    h.ed.comment_layout_for_tests()
        .into_iter()
        .filter(|(_, k)| matches!(k, PinKind::Pin { .. }))
        .map(|(p, _)| p)
        .collect()
}

fn clusters(h: &H) -> Vec<u32> {
    h.ed.comment_layout_for_tests()
        .into_iter()
        .filter_map(|(_, k)| match k {
            PinKind::Cluster { count, .. } => Some(count),
            _ => None,
        })
        .collect()
}

/// Щелчок по капле метки с остриём в экранной точке `tip`.
fn click_pin(h: &mut H, tip: Point) {
    h.click(tip.x + 10.0, tip.y - 10.0, none());
}

fn close_pt(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-6
}

#[test]
fn without_comments_tool_and_shift_c_are_disabled_and_keys_do_nothing() {
    let mut h = H::new();
    assert!(!h.ed.command_state(CommandId::ToolComment).enabled);
    assert!(!h.ed.command_state(CommandId::ViewComments).enabled);
    h.key("KeyC", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    h.key("KeyC", shift());
    assert!(h.ed.comments_visible(), "Shift+C молчит");
    h.ed.execute(CommandId::ToolComment);
    h.ed.set_tool(ToolKind::Comment);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move, "и тулбаром не включить");
    // Меток без комментариев не видно и нажатие их не ловит.
    let p = world_pin(&h, "t1", 100.0, 100.0, true);
    h.ed.set_comment_pins(vec![p], true);
    assert!(h.ed.overlay().comments.is_empty());

    h.ed.set_comments_available(true);
    let ev = h.ed.take_events();
    assert!(ev.contains(&EditorEvent::CommentsState {
        mode: false,
        visible: true,
        available: true
    }));
    assert!(ev
        .iter()
        .any(|e| matches!(e, EditorEvent::CommandStates(_))));
    assert!(h.ed.command_state(CommandId::ToolComment).enabled);
    assert_eq!(
        h.ed.command_state(CommandId::ViewComments).checked,
        Some(true)
    );
    assert_eq!(h.ed.overlay().comments.len(), 1);
    h.key("KeyC", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Comment);
    assert!(h
        .ed
        .take_events()
        .contains(&EditorEvent::ToolChanged(ToolKind::Comment)));
    // Комментариев не стало (роль отняли): режим кончается, меток нет.
    h.ed.set_comments_available(false);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert!(h.ed.overlay().comments.is_empty());
}

#[test]
fn comment_mode_disables_other_tools_but_not_the_camera() {
    let mut h = available();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyC", none());
    h.ed.take_events();
    // Щелчок по узлу — не выделение, а место нового комментария, привязанное к узлу.
    h.click(150.0, 120.0, none());
    assert!(h.sel().is_empty());
    let ev = h.ed.take_events();
    let place = ev.iter().find_map(|e| match e {
        EditorEvent::CommentPlace { anchor, screen } => Some((*anchor, *screen)),
        _ => None,
    });
    let (a, s) = place.expect("место нового комментария");
    assert_eq!(a.node, Some(r));
    assert!(close_pt(a.local, Point::new(50.0, 20.0)), "{:?}", a.local);
    assert!(close_pt(a.world, Point::new(150.0, 120.0)));
    assert!(close_pt(s, Point::new(150.0, 120.0)));
    // Протяжка по холсту — ни рамки выделения, ни нового комментария.
    h.drag((400.0, 400.0), (600.0, 600.0), none());
    assert!(h.sel().is_empty());
    assert!(h.ed.overlay().marquee.is_none());
    assert!(!h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentPlace { .. })));
    // Камера: колесо, колесо с Ctrl, щипок, средняя кнопка, Space.
    let c0 = h.ed.camera().center();
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(500.0, 400.0),
        delta: Vec2::new(0.0, 40.0),
        modifiers: none(),
    }));
    assert!(!close_pt(h.ed.camera().center(), c0), "колесо двигает вид");
    let z0 = h.ed.camera().zoom();
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(500.0, 400.0),
        delta: Vec2::new(0.0, -50.0),
        modifiers: ctrl(),
    }));
    assert!(h.ed.camera().zoom() > z0, "Ctrl+колесо — зум");
    let z1 = h.ed.camera().zoom();
    h.ev(Input::Pinch {
        point: Point::new(500.0, 400.0),
        scale: 0.5,
    });
    assert!(h.ed.camera().zoom() < z1, "щипок — зум");
    let c1 = h.ed.camera().center();
    let mut mid = h.pe(300.0, 300.0, none());
    mid.button = PointerButton::Middle;
    h.ev(Input::PointerDown(mid));
    mid.point = Point::new(350.0, 300.0);
    h.ev(Input::PointerMove(mid));
    h.ev(Input::PointerUp(mid));
    assert!(!close_pt(h.ed.camera().center(), c1), "средняя кнопка");
    let c2 = h.ed.camera().center();
    h.ev(Input::KeyDown(KeyEvent::new("Space", none())));
    h.drag((300.0, 300.0), (380.0, 300.0), none());
    h.ev(Input::KeyUp(KeyEvent::new("Space", none())));
    assert!(!close_pt(h.ed.camera().center(), c2), "Space");
    assert!(
        !h.ed
            .take_events()
            .iter()
            .any(|e| matches!(e, EditorEvent::CommentPlace { .. })),
        "камера — не новый комментарий"
    );
    assert_eq!(h.ed.tool_kind(), ToolKind::Comment);
    // Ничего не записано.
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    // Клавиша инструмента выводит из режима; Esc — к «Перемещению».
    h.key("KeyR", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
    h.key("KeyC", none());
    h.key("Escape", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
}

#[test]
fn pins_never_touch_document_history_or_scene_counter() {
    let mut h = available();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    h.ed.mark_journal();
    let before = h.state();
    let steps = h.ed.step_count();
    let can_undo = h.ed.document().can_undo();
    let own = h.ed.scene().own_change_counter();
    let page = h.ed.page();
    h.ed.set_comment_pins(
        vec![
            pin(
                "a",
                anchor(page, Some(r), (10.0, 10.0), (110.0, 110.0)),
                true,
            ),
            world_pin(&h, "b", 500.0, 500.0, true),
        ],
        true,
    );
    h.key("KeyC", none());
    h.click(160.0, 160.0, none());
    // Своя метка: перетаскивание, открытие, черновик, переход, скрытие, удаление.
    h.drag((520.0, 490.0), (620.0, 540.0), none());
    click_pin(&mut h, Point::new(110.0, 110.0));
    h.ed.set_comment_draft(Some(anchor(page, None, (5.0, 5.0), (5.0, 5.0))));
    h.ed.comment_focus("a");
    h.key("KeyC", shift());
    h.key("KeyC", shift());
    h.ed.remove_comment_pins(&["a", "b"]);
    assert_eq!(h.state(), before, "документ тот же");
    assert_eq!(h.ed.step_count(), steps, "шагов истории нет");
    assert_eq!(h.ed.document().can_undo(), can_undo);
    assert_eq!(h.ed.scene().own_change_counter(), own);
    assert_eq!(h.ed.take_journal().unwrap(), None, "в журнал ничего");
    h.assert_scene_clean();
}

#[test]
fn own_movable_pin_drags_with_preview_and_reports_on_release() {
    let mut h = available();
    let r = h.rect(None, 600.0, 500.0, 200.0, 200.0);
    let p = world_pin(&h, "mine", 300.0, 300.0, true);
    h.ed.set_comment_pins(vec![p], true);
    h.ed.take_events();
    // Тянем каплю «Перемещением»: метка едет, событие — только при отпускании.
    h.drag_hold((310.0, 290.0), (410.0, 340.0), none());
    assert_eq!(pins_on_screen(&h), vec![Point::new(400.0, 350.0)]);
    assert!(h.sel().is_empty(), "не выделение");
    assert!(!h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentMoved { .. })));
    // Esc возвращает метку, события нет.
    h.key("Escape", none());
    assert_eq!(pins_on_screen(&h), vec![Point::new(300.0, 300.0)]);
    h.up(410.0, 340.0, none());
    assert!(!h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentMoved { .. })));
    // Протянутая на узел — привязана к нему.
    h.drag((310.0, 290.0), (710.0, 590.0), none());
    let moved = h.ed.take_events().into_iter().find_map(|e| match e {
        EditorEvent::CommentMoved { id, anchor } => Some((id, anchor)),
        _ => None,
    });
    let (id, a) = moved.expect("CommentMoved");
    assert_eq!(&*id, "mine");
    assert_eq!(a.node, Some(r));
    assert!(close_pt(a.local, Point::new(100.0, 100.0)), "{:?}", a.local);
    assert!(close_pt(a.world, Point::new(700.0, 600.0)));
    assert_eq!(pins_on_screen(&h), vec![Point::new(700.0, 600.0)]);
}

#[test]
fn foreign_pin_does_not_move() {
    let mut h = available();
    let p = world_pin(&h, "theirs", 300.0, 300.0, false);
    h.ed.set_comment_pins(vec![p], true);
    h.ed.take_events();
    h.drag_hold((310.0, 290.0), (410.0, 340.0), none());
    assert_eq!(pins_on_screen(&h), vec![Point::new(300.0, 300.0)]);
    h.up(410.0, 340.0, none());
    let ev = h.ed.take_events();
    assert!(!ev
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentMoved { .. })));
    assert_eq!(
        h.ed.comment_pin("theirs").unwrap().anchor.world,
        Point::new(300.0, 300.0)
    );
}

#[test]
fn close_pins_cluster_at_small_zoom_and_cluster_click_zooms_in() {
    let mut h = available();
    let page = h.ed.page();
    let pins: Vec<_> = (0..10)
        .map(|i| {
            let x = 100.0 + i as f64 * 60.0;
            pin(
                &format!("p{i}"),
                anchor(page, None, (x, 300.0), (x, 300.0)),
                false,
            )
        })
        .collect();
    h.ed.set_comment_pins(pins, true);
    assert_eq!(pins_on_screen(&h).len(), 10, "зум 1 — каждая отдельно");
    assert!(clusters(&h).is_empty());
    for _ in 0..3 {
        h.ed.execute(CommandId::ZoomOut);
    }
    // Ряд в 67 px на экране — две группы, их круги не наползают друг на друга.
    assert_eq!(clusters(&h).iter().sum::<u32>(), 10);
    assert!(pins_on_screen(&h).is_empty());
    h.ed.execute(CommandId::ZoomOut);
    let zoom = h.ed.camera().zoom();
    assert!(zoom <= 0.0625);
    assert_eq!(clusters(&h), vec![10], "мелкий зум — одна группа из 10");
    assert!(pins_on_screen(&h).is_empty());
    assert_eq!(h.ed.overlay().comments.len(), 1);
    let (at, _) = h.ed.comment_layout_for_tests()[0].clone();
    h.click(at.x, at.y, none());
    assert!(
        h.ed.camera().zoom() >= zoom * 2.0,
        "щелчок по группе приближает"
    );
    assert!(
        clusters(&h).is_empty() && pins_on_screen(&h).len() == 10,
        "метки разошлись: {:?}",
        h.ed.comment_layout_for_tests()
    );
}

#[test]
fn shift_c_hides_and_shows_pins_and_hidden_pins_do_not_catch_clicks() {
    let mut h = available();
    let r = h.rect(None, 200.0, 200.0, 200.0, 200.0);
    let p = world_pin(&h, "a", 250.0, 300.0, false);
    h.ed.set_comment_pins(vec![p], true);
    h.ed.take_events();
    h.key("KeyC", shift());
    assert!(h.ed.overlay().comments.is_empty());
    assert_eq!(
        h.ed.command_state(CommandId::ViewComments).checked,
        Some(false)
    );
    assert!(h.ed.take_events().contains(&EditorEvent::CommentsState {
        mode: false,
        visible: false,
        available: true
    }));
    // Скрытая метка не ловит щелчок: выделяется узел под ней.
    click_pin(&mut h, Point::new(250.0, 300.0));
    assert_eq!(h.sel(), vec![r]);
    h.key("KeyC", shift());
    assert_eq!(h.ed.overlay().comments.len(), 1);
    // Вход в режим комментариев показывает скрытые метки.
    h.key("KeyC", shift());
    h.key("KeyC", none());
    assert!(h.ed.comments_visible());
}

#[test]
fn clicking_a_pin_opens_it_before_canvas_selection_and_anchor_follows_camera() {
    let mut h = available();
    let r = h.rect(None, 200.0, 200.0, 200.0, 200.0);
    let p = world_pin(&h, "a", 250.0, 300.0, false);
    h.ed.set_comment_pins(vec![p], true);
    h.ed.take_events();
    click_pin(&mut h, Point::new(250.0, 300.0));
    assert!(h.sel().is_empty(), "метка — раньше выделения");
    let ev = h.ed.take_events();
    assert!(ev.contains(&EditorEvent::CommentOpened {
        id: Arc::from("a"),
        screen: Point::new(250.0, 300.0)
    }));
    assert!(ev.contains(&EditorEvent::CommentAnchor {
        screen: Point::new(250.0, 300.0),
        visible: true
    }));
    assert_eq!(h.ed.comment_open(), Some("a"));
    // Камера сдвинулась — окно едет за меткой; за краем холста — «не видно».
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(500.0, 400.0),
        delta: Vec2::new(-30.0, 0.0),
        modifiers: none(),
    }));
    assert!(h.ed.take_events().contains(&EditorEvent::CommentAnchor {
        screen: Point::new(280.0, 300.0),
        visible: true
    }));
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(500.0, 400.0),
        delta: Vec2::new(2000.0, 0.0),
        modifiers: none(),
    }));
    assert!(h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentAnchor { visible: false, .. })));
    // Мимо метки — обычное выделение.
    h.ed.set_comment_open(None);
    h.ed.execute(CommandId::Zoom100);
    let mut cam = h.ed.camera().clone();
    cam.set_center(Point::new(500.0, 400.0));
    h.ed.set_camera(cam);
    h.click(350.0, 350.0, none());
    assert_eq!(h.sel(), vec![r]);
}

#[test]
fn focus_goes_to_the_pins_page_centres_camera_and_waits_for_unknown_pins() {
    let mut h = available();
    let first = h.ed.page();
    let second = h.ed.add_page("Вторая").unwrap();
    h.ed.set_comment_pins(
        vec![pin(
            "far",
            anchor(second, None, (5000.0, 4000.0), (5000.0, 4000.0)),
            false,
        )],
        true,
    );
    // Метка другой страницы здесь не рисуется.
    assert!(h.ed.overlay().comments.is_empty());
    h.ed.take_events();
    h.ed.comment_focus("far");
    assert_eq!(h.ed.page(), second);
    assert!(close_pt(h.ed.camera().center(), Point::new(5000.0, 4000.0)));
    assert_eq!(h.ed.comment_open(), Some("far"));
    let ev = h.ed.take_events();
    assert!(ev.contains(&EditorEvent::PageChanged(second)));
    assert!(ev.contains(&EditorEvent::CommentOpened {
        id: Arc::from("far"),
        screen: Point::new(500.0, 400.0)
    }));
    assert_eq!(h.ed.overlay().comments.len(), 1);
    // Обсуждения ещё нет (ссылка раньше списка): переход, когда метка придёт.
    h.ed.comment_focus("late");
    assert_eq!(h.ed.page(), second);
    h.ed.set_comment_pins(
        vec![pin(
            "late",
            anchor(first, None, (-300.0, 200.0), (-300.0, 200.0)),
            false,
        )],
        false,
    );
    assert_eq!(h.ed.page(), first);
    assert!(close_pt(h.ed.camera().center(), Point::new(-300.0, 200.0)));
    assert_eq!(h.ed.comment_open(), Some("late"));
}

#[test]
fn pin_of_hidden_or_deleted_node_stays_at_last_world_point() {
    let mut h = available();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    let page = h.ed.page();
    h.ed.set_comment_pins(
        vec![pin(
            "a",
            anchor(page, Some(r), (10.0, 20.0), (900.0, 700.0)),
            false,
        )],
        true,
    );
    assert_eq!(pins_on_screen(&h), vec![Point::new(110.0, 120.0)]);
    h.set(
        r,
        PropsPatch {
            x: Some(150.0),
            ..Default::default()
        },
    );
    assert_eq!(
        pins_on_screen(&h),
        vec![Point::new(160.0, 120.0)],
        "едет с узлом"
    );
    h.set(
        r,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    assert_eq!(pins_on_screen(&h), vec![Point::new(900.0, 700.0)], "скрыт");
    h.set(
        r,
        PropsPatch {
            visible: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(pins_on_screen(&h), vec![Point::new(160.0, 120.0)]);
    h.ed.select(&[r]);
    h.ed.execute(CommandId::Delete);
    assert_eq!(pins_on_screen(&h), vec![Point::new(900.0, 700.0)], "удалён");
}

#[test]
fn click_over_nested_frame_anchors_to_top_level_frame() {
    let mut h = available();
    let outer = h.frame(None, 50.0, 50.0, 500.0, 500.0);
    let inner = h.frame(Some(outer), 100.0, 100.0, 200.0, 200.0);
    let _leaf = h.rect(Some(inner), 10.0, 10.0, 50.0, 50.0);
    h.key("KeyC", none());
    h.ed.take_events();
    // Мир (170, 170) — над листом во вложенном фрейме.
    h.click(170.0, 170.0, none());
    let a =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::CommentPlace { anchor, .. } => Some(anchor),
                _ => None,
            })
            .unwrap();
    assert_eq!(a.node, Some(outer));
    assert!(close_pt(a.local, Point::new(120.0, 120.0)), "{:?}", a.local);
    // Пустое место — мир.
    h.click(800.0, 700.0, none());
    let a =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::CommentPlace { anchor, .. } => Some(anchor),
                _ => None,
            })
            .unwrap();
    assert_eq!(a.node, None);
    assert!(close_pt(a.local, Point::new(800.0, 700.0)));
}

#[test]
fn pin_follows_its_node_during_own_drag_and_remote_live_preview() {
    let mut h = available();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    let page = h.ed.page();
    h.ed.set_comment_pins(
        vec![pin(
            "a",
            anchor(page, Some(r), (10.0, 20.0), (110.0, 120.0)),
            false,
        )],
        true,
    );
    // Своё перетаскивание: превью жеста в сцене — метка с ним.
    h.drag_hold((250.0, 180.0), (350.0, 180.0), none());
    assert_eq!(pins_on_screen(&h), vec![Point::new(210.0, 120.0)]);
    h.key("Escape", none());
    h.up(350.0, 180.0, none());
    assert_eq!(pins_on_screen(&h), vec![Point::new(110.0, 120.0)]);
    // Чужое живое превью: слой сцены — метка с ним, документ тот же.
    let own = h.ed.scene().own_change_counter();
    h.ed.presence_join(1, "u-1", "Анна", peer_color(0x0d74ce));
    h.ed.presence_part(1, 1, 0, 1, vec![r]);
    let state = |dx: f64| PeerState {
        page: Some(page),
        view: Rect::new(0.0, 0.0, 1000.0, 800.0),
        cursor: None,
        selection: Some(PeerSelection {
            rev: 1,
            count: 1,
            parts: 1,
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
        }),
        gesture: Some(Gesture {
            kind: GestureKind::Move,
            transform: Affine::translate((dx, 0.0)),
            frame: Affine::IDENTITY,
            k: 1.0,
        }),
    };
    h.ed.presence_state(1, state(50.0));
    assert_eq!(pins_on_screen(&h), vec![Point::new(160.0, 120.0)]);
    h.ed.presence_state(1, state(80.0));
    assert_eq!(pins_on_screen(&h), vec![Point::new(190.0, 120.0)]);
    assert_eq!(h.ed.scene().own_change_counter(), own);
    h.ed.presence_left(1);
    assert_eq!(pins_on_screen(&h), vec![Point::new(110.0, 120.0)]);
}

#[test]
fn resolved_pins_are_hidden_unless_shown_or_open() {
    let mut h = available();
    let mut p = world_pin(&h, "done", 300.0, 300.0, false);
    p.resolved = true;
    h.ed.set_comment_pins(vec![p], true);
    assert!(pins_on_screen(&h).is_empty());
    h.ed.set_comments_show_resolved(true);
    assert_eq!(pins_on_screen(&h).len(), 1);
    h.ed.set_comments_show_resolved(false);
    h.ed.set_comment_open(Some("done"));
    assert_eq!(pins_on_screen(&h).len(), 1, "открытое видно и закрытым");
}

#[test]
fn pin_store_is_bounded_and_removal_closes_the_open_thread() {
    let mut h = available();
    let page = h.ed.page();
    let pins: Vec<_> = (0..COMMENT_PINS_MAX + 5)
        .map(|i| {
            pin(
                &format!("p{i}"),
                anchor(page, None, (0.0, 0.0), (0.0, 0.0)),
                false,
            )
        })
        .collect();
    h.ed.set_comment_pins(pins, true);
    assert_eq!(h.ed.comment_pin_count(), COMMENT_PINS_MAX);
    // Замена известной — и сверх предела.
    let mut known = pin("p0", anchor(page, None, (1.0, 1.0), (1.0, 1.0)), false);
    known.replies = 3;
    h.ed.set_comment_pins(vec![known], false);
    assert_eq!(h.ed.comment_pin("p0").unwrap().replies, 3);
    h.ed.set_comment_open(Some("p1"));
    h.ed.take_events();
    h.ed.remove_comment_pins(&["p1"]);
    assert_eq!(h.ed.comment_open(), None);
    assert!(h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentAnchor { visible: false, .. })));
}
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    (CommandId::ToolHand, &["KeyH"]),
    (CommandId::ToolScale, &["KeyK"]),
    (CommandId::HandHold, &["Space"]),
    (CommandId::ToolImage, &["Mod+Shift+KeyK"]),
````

на:

````rust
    (CommandId::ToolHand, &["KeyH"]),
    (CommandId::ToolScale, &["KeyK"]),
    (CommandId::ToolComment, &["KeyC"]),
    (CommandId::HandHold, &["Space"]),
    (CommandId::ToolImage, &["Mod+Shift+KeyK"]),
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    (CommandId::ViewRulers, &["Shift+KeyR"]),
    (CommandId::ViewGuides, &["Shift+KeyG"]),
    (CommandId::EditObject, &["Enter"]),
    (CommandId::TextBold, &["Mod+KeyB"]),
````

на:

````rust
    (CommandId::ViewRulers, &["Shift+KeyR"]),
    (CommandId::ViewGuides, &["Shift+KeyG"]),
    (CommandId::ViewComments, &["Shift+KeyC"]),
    (CommandId::EditObject, &["Enter"]),
    (CommandId::TextBold, &["Mod+KeyB"]),
````

В `crates/ferrum-editor/tests/keymap.rs` заменить:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 88);
}

````

на:

````rust
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 90);
}

````

В `crates/ferrum-editor/tests/main_menu.rs` заменить:

````rust
        assert_eq!(
            all.contains(&c.id),
            !only_keys || c.id == CommandId::ToolPen,
            "{}",
            c.key
````

на:

````rust
        assert_eq!(
            all.contains(&c.id),
            // «Перо» — и в меню «Вектор», «Комментарий» — и в меню «Вид» (спека подсистемы 4,
            // раздел 10.3).
            !only_keys || matches!(c.id, CommandId::ToolPen | CommandId::ToolComment),
            "{}",
            c.key
````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 88);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

на:

````rust
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 90);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test comments`
Expected: FAIL — не собирается: нет `ToolKind::Comment`, `CommandId::ViewComments`, событий `Comment*` и методов комментариев редактора, 76 ошибок компиляции

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
}

fn main() {
    let mut report = Report(Vec::new());
````

на:

````rust
}

/// Метки комментариев (спека подсистемы 4, раздел 10.2): 1000 обсуждений на странице из
/// 100 000 узлов — по 10 на каждом из первых 100 фреймов и 50 в мире. Каждый кадр, пока узлы
/// едут, редактор раскладывает метки заново (мировая точка по матрице узла в сцене, экран,
/// группы) — замер `Editor::overlay` после каждого движения своего перетаскивания 10 фреймов
/// (100 меток едут) и после каждого кадра участника, который тянет те же 10 фреймов (живое
/// превью), при зуме 1, 0,25 и «вся страница» (метки собраны в группы). Без меток та же
/// `overlay` стоит микросекунды: замер — прибавка к кадру. Цель — 1 мс, как у присутствия.
fn comments_probe(report: &mut Report, ed: &mut Editor, ids: &[NodeId], clock: &mut Clock) {
    use ferrum_editor::{peer_color, CommentPin, PinAnchor};
    use ferrum_render::{PinKind, Rgba};
    use std::sync::Arc;
    let page = ed.page();
    let frames: Vec<NodeId> = ids.iter().step_by(100).take(100).copied().collect();
    let mut pins = Vec::new();
    for (f, id) in frames.iter().enumerate() {
        let world = ed.scene().node(*id).unwrap().world;
        for k in 0..10 {
            let local = Point::new(60.0 + k as f64 * 130.0, 80.0 + (k % 3) as f64 * 300.0);
            pins.push(CommentPin {
                id: Arc::from(format!("t{f}-{k}")),
                anchor: PinAnchor {
                    page,
                    node: Some(*id),
                    local,
                    world: world * local,
                },
                initials: Arc::from(["АС", "Б", "ВК", "ГЛ", "Д"][(f + k) % 5]),
                color: peer_color([0xe5484d, 0x0d74ce, 0x2d8a3e][k % 3]),
                replies: (k % 4) as u32,
                unread: k % 3 == 0,
                resolved: false,
                movable: true,
            });
        }
    }
    for i in 0..50 {
        let at = Point::new(-500.0 - i as f64 * 97.0, -300.0 - (i % 7) as f64 * 113.0);
        pins.push(CommentPin {
            id: Arc::from(format!("w{i}")),
            anchor: PinAnchor {
                page,
                node: None,
                local: at,
                world: at,
            },
            initials: Arc::from("Е"),
            color: Rgba::from_rgba8(0x8e, 0x4e, 0xc6, 255),
            replies: 0,
            unread: false,
            resolved: false,
            movable: false,
        });
    }
    assert_eq!(pins.len(), 1050);
    ed.set_comments_available(true);
    ed.set_comment_pins(pins, true);
    ed.take_events();
    let dragged = &frames[..10];
    let saved = ed.camera().clone();
    let all = ed.scene().bounds().unwrap();
    for (zoom_label, zoom) in [
        ("зум 1", Some(1.0)),
        ("зум 0,25", Some(0.25)),
        ("вся страница", None),
    ] {
        let mut cam = saved.clone();
        match zoom {
            Some(z) => {
                cam.set_zoom(z);
                cam.set_center(Point::new(1600.0 * 2.0, 1024.0));
            }
            None => cam.fit(all, 0.0),
        }
        ed.set_camera(cam);
        let layout = ed.comment_layout_for_tests();
        let groups = layout
            .iter()
            .filter(|(_, k)| matches!(k, PinKind::Cluster { .. }))
            .count();
        report.info(
            &format!("Метки комментариев, {zoom_label}: меток / групп в раскладке"),
            format!("{} / {groups}", layout.len() - groups),
        );
        // Своё перетаскивание 10 фреймов: метки едут с ними каждый кадр.
        ed.select(dragged);
        let f0 = ed.scene().node(dragged[0]).unwrap().world_bounds.unwrap();
        let grab = ed
            .camera()
            .world_to_screen(Point::new(f0.x0 + 2.0, f0.y0 + 2.0));
        ed.handle(Input::PointerDown(clock.at(grab)));
        ed.handle(Input::PointerMove(
            clock.at(Point::new(grab.x + 4.0, grab.y)),
        ));
        let before = ed.overlay().comments.clone();
        let mut runs = Vec::new();
        for i in 1..=MOVES {
            let p = Point::new(grab.x + 4.0 + 1.7 * i as f64, grab.y + 0.37 * i as f64);
            ed.handle(Input::PointerMove(clock.at(p)));
            ed.take_events();
            let t = Instant::now();
            std::hint::black_box(ed.overlay());
            runs.push(t.elapsed());
        }
        let after = ed.overlay().comments.clone();
        report.gate_median(
            &format!("Метки комментариев за своим перетаскиванием 10 фреймов, {zoom_label}: раскладка кадра"),
            &runs,
            1.0,
        );
        report.check(
            &format!("Метки едут за своим перетаскиванием, {zoom_label}"),
            zoom.is_none() || before != after,
            format!("{} меток в оверлее", after.len()),
            "метки сдвинулись",
        );
        ed.handle(Input::KeyDown(KeyEvent::new("Escape", Modifiers::NONE)));
        ed.handle(Input::PointerUp(clock.at(grab)));
        ed.select(&[]);
        ed.take_events();
        // Чужое живое превью тех же 10 фреймов (1000 узлов — в пределах превью участника).
        ed.presence_join(41, "u-41", "Участник", peer_color(0x0d74ce));
        ed.presence_part(41, 1, 0, 1, dragged.to_vec());
        ed.presence_state(41, peer_move(page, dragged.len(), None));
        let before = ed.overlay().comments.clone();
        let mut runs = Vec::new();
        for i in 1..=MOVES {
            ed.presence_state(41, peer_move(page, dragged.len(), Some(i as f64 * 3.0)));
            ed.take_events();
            let t = Instant::now();
            std::hint::black_box(ed.overlay());
            runs.push(t.elapsed());
        }
        let after = ed.overlay().comments.clone();
        report.gate_median(
            &format!("Метки комментариев за чужим живым превью 10 фреймов, {zoom_label}: раскладка кадра"),
            &runs,
            1.0,
        );
        report.check(
            &format!("Метки едут за чужим превью, {zoom_label}"),
            zoom.is_none() || before != after,
            format!("{} меток в оверлее", after.len()),
            "метки сдвинулись",
        );
        ed.presence_left(41);
        ed.take_events();
    }
    ed.set_comment_pins(Vec::new(), true);
    ed.set_comments_available(false);
    ed.set_camera(saved);
    ed.take_events();
}

fn main() {
    let mut report = Report(Vec::new());
````

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
    // узлов (пересборка R-tree сцены), а не из-за числа участников.
    presence_frame_drag(&mut report, (8, 1500), 1, 16.0);
    ed.select(&children);

````

на:

````rust
    // узлов (пересборка R-tree сцены), а не из-за числа участников.
    presence_frame_drag(&mut report, (8, 1500), 1, 16.0);
    comments_probe(&mut report, &mut ed, &ids, &mut clock);
    ed.select(&children);

````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
            C::ToolArrow => self.set_tool(ToolKind::Arrow),
            C::ToolScale => self.set_tool(ToolKind::Scale),
            C::Flatten => self.flatten(),
            C::BooleanUnion => self.boolean(ferrum_doc::BooleanOp::Union),
````

на:

````rust
            C::ToolArrow => self.set_tool(ToolKind::Arrow),
            C::ToolScale => self.set_tool(ToolKind::Scale),
            C::ToolComment => self.set_tool(ToolKind::Comment),
            C::ViewComments => self.toggle_comments(),
            C::Flatten => self.flatten(),
            C::BooleanUnion => self.boolean(ferrum_doc::BooleanOp::Union),
````

В `crates/ferrum-editor/src/command_state.rs` заменить:

````rust
//! только изменившиеся команды и только после шага, который мог их изменить:
//! выделение, сцена, история, «только чтение», режим правки, настройки.
//! Инструмент и вид в ключ не входят — ни одно состояние от них пока не зависит.
//! Недоступная команда при вызове ничего не делает: ни шага, ни сообщения.

````

на:

````rust
//! только изменившиеся команды и только после шага, который мог их изменить:
//! выделение, сцена, история, «только чтение», режим правки, настройки.
//! Инструмент и вид в ключ не входят — ни одно состояние от них пока не зависит; от
//! комментариев зависят — есть ли они и видны ли метки.
//! Недоступная команда при вызове ничего не делает: ни шага, ни сообщения.

````

В `crates/ferrum-editor/src/command_state.rs` заменить:

````rust
    vector_edit: bool,
    settings: crate::editor::Settings,
}

````

на:

````rust
    vector_edit: bool,
    settings: crate::editor::Settings,
    /// Комментарии есть и метки видны.
    comments: (bool, bool),
}

````

В `crates/ferrum-editor/src/command_state.rs` заменить:

````rust
            vector_edit: self.vector_edit.is_some(),
            settings: self.settings.clone(),
        }
    }
````

на:

````rust
            vector_edit: self.vector_edit.is_some(),
            settings: self.settings.clone(),
            comments: (self.comments_available(), self.comments_visible()),
        }
    }
````

В `crates/ferrum-editor/src/command_state.rs` заменить:

````rust
            // Как в Figma, без линеек направляющих не видно — пункт серый.
            C::ViewGuides => (self.settings.rulers, checked(self.settings.guides)),
            C::EditObject => (edit && !text && !vector && f.editable, None),
            C::TextBold
````

на:

````rust
            // Как в Figma, без линеек направляющих не видно — пункт серый.
            C::ViewGuides => (self.settings.rulers, checked(self.settings.guides)),
            // Без комментариев (локальный файл, роль ещё не пришла) — серые, клавиши молчат.
            // Читателю можно: комментировать могут все с доступом (раздел 10.3).
            C::ToolComment => (self.comments_available(), None),
            C::ViewComments => (self.comments_available(), checked(self.comments_visible())),
            C::EditObject => (edit && !text && !vector && f.editable, None),
            C::TextBold
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// «Масштаб» (K, подсистема 3, этап 5).
    ToolScale,
    /// «Сплющить»: выделение — в один вектор.
    Flatten,
````

на:

````rust
    /// «Масштаб» (K, подсистема 3, этап 5).
    ToolScale,
    /// «Комментарий» (C, подсистема 4, этап 5): режим комментариев. Недоступна, пока хозяин не
    /// сказал, что комментарии есть (`Editor::set_comments_available`).
    ToolComment,
    /// «Сплющить»: выделение — в один вектор.
    Flatten,
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
    /// Направляющие (Shift+G, спека подсистемы 2, раздел 9): видны при линейках.
    ViewGuides,
    /// Войти в правку выделенного: текст — правка текста, вектор и фигура — правка
    /// вектора (как Enter).
````

на:

````rust
    /// Направляющие (Shift+G, спека подсистемы 2, раздел 9): видны при линейках.
    ViewGuides,
    /// Показать и скрыть метки комментариев (Shift+C, как в Figma).
    ViewComments,
    /// Войти в правку выделенного: текст — правка текста, вектор и фигура — правка
    /// вектора (как Enter).
````

В `crates/ferrum-editor/src/commands.rs` заменить:

````rust
        &["пропорционально", "размер", "scale", "resize"],
        &["KeyK"],
    ),
    cmd(
````

на:

````rust
        &["пропорционально", "размер", "scale", "resize"],
        &["KeyK"],
    ),
    // Как в Figma: «Comment» — C.
    cmd(
        C::ToolComment,
        "tool.comment",
        G::Tool,
        "Комментарий",
        &["обсуждение", "заметка", "отзыв", "comment", "feedback"],
        &["KeyC"],
    ),
    cmd(
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
    // Как в Figma: «Show comments» — Shift+C.
    cmd(
        C::ViewComments,
        "view.comments",
        G::View,
        "Комментарии",
        &[
            "метки",
            "обсуждения",
            "показать комментарии",
            "comments",
            "show comments",
        ],
        &["Shift+KeyC"],
    )
    .toggle(),
    // Enter — физическая клавиша правки (как Esc у жеста); в таблице — для меню
    // и подписи.
````

Создать `crates/ferrum-editor/src/comments.rs`:

````rust
//! Метки комментариев на холсте (спека подсистемы 4, разделы 10.2 и 10.3). Сами обсуждения
//! живут на сервере, интерфейс присылает метки (`set_comment_pins`), а редактор их раскладывает
//! по сцене и камере, собирает слипшиеся в группы, проверяет попадание и ведёт жесты меток:
//! открыть, перетащить свою, поставить новую в режиме «Комментарий». Метки не трогают ни
//! документ, ни историю, ни `own_change_counter` сцены: это не правка, а оверлей.
//!
//! Привязка: метка над узлом едет с верхним узлом страницы (ребёнком страницы) под точкой —
//! `x`, `y` в его координатах; на пустом месте — в мире. Каждый кадр мировая точка метки —
//! матрица узла в сцене (со своим превью жеста и слоем чужого живого превью) на `(x, y)`; узла
//! нет или он скрыт — последняя мировая точка с сервера `(wx, wy)` **(поведение)**.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use ferrum_doc::{NodeId, PageId};
use ferrum_render::{pin_contains, Camera, PinKind, PinMark, Rgba, MAX_ZOOM, PIN_SIZE};
use kurbo::{Point, Rect};

use crate::editor::{Editor, EditorEvent};
use crate::input::PointerEvent;
use crate::tools::{ToolKind, DRAG_THRESHOLD_PX, HIT_TOLERANCE_PX};

/// Меток держит редактор: больше — лишние не принимаются (страница из 1000 обсуждений — в
/// десять раз меньше).
pub const COMMENT_PINS_MAX: usize = 10_000;
/// Метки ближе этого на экране (CSS-пиксели, между точками меток) слипаются в группу: капли
/// перекрылись бы.
pub const PIN_CLUSTER_DISTANCE: f64 = PIN_SIZE;
/// Поле вокруг меток группы при щелчке по ней — CSS-пиксели: крайние метки не у края холста.
pub const PIN_CLUSTER_FIT_PADDING: f64 = 4.0 * PIN_SIZE;

/// Где метка: страница, верхний узел (или мир), точка в его координатах и в мире.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinAnchor {
    pub page: PageId,
    pub node: Option<NodeId>,
    pub local: Point,
    pub world: Point,
}

/// Метка обсуждения от интерфейса.
#[derive(Debug, Clone, PartialEq)]
pub struct CommentPin {
    pub id: Arc<str>,
    pub anchor: PinAnchor,
    /// Инициалы (до трёх графем) и цвет автора.
    pub initials: Arc<str>,
    pub color: Rgba,
    pub replies: u32,
    pub unread: bool,
    pub resolved: bool,
    /// Можно ли тащить (автор обсуждения или владелец файла — решает сервер).
    pub movable: bool,
}

/// Нажатие на метку, группу или холст в режиме комментариев — до отпускания.
#[derive(Debug, Clone)]
enum Press {
    Pin {
        id: Arc<str>,
        /// Экранная точка нажатия (порог) и мировая под ней: сдвиг метки — от неё, поэтому
        /// метка идёт за указателем и когда камера едет посреди жеста.
        start: Point,
        grab: Point,
        /// Мировая точка метки на нажатие и куда её тянут (после порога).
        from: Point,
        to: Option<Point>,
        movable: bool,
    },
    Cluster {
        members: Vec<Point>,
    },
    Place {
        start: Point,
        moved: bool,
    },
}

/// Что под точкой экрана: метка или группа.
#[derive(Debug, Clone)]
enum Target {
    Pin(Arc<str>),
    /// Мировые точки меток группы — для «вписать в экран».
    Cluster(Vec<Point>),
}

#[derive(Debug, Clone)]
struct Hit {
    /// Точка метки на экране, CSS-пиксели.
    at: Point,
    kind: PinKind,
    target: Target,
}

/// Раскладка меток для кадра и попадания.
#[derive(Debug, Default)]
pub(crate) struct Layout {
    marks: Arc<[PinMark]>,
    /// По порядку рисования: верхняя — последняя.
    hits: Vec<Hit>,
    /// Экранная точка открытой метки (или черновика) и видна ли она на холсте.
    anchor: Option<(Point, bool)>,
}

/// Ревизия меток, сцена (счётчик со всеми слоями превью, `generation`, `scene_epoch`), страница и
/// камера: раскладка та же, пока они те же.
type LayoutKey = (u64, u64, u64, u64, PageId, Camera);

pub(crate) struct Comments {
    /// Хозяин сказал, что комментарии есть (файл на сервере и роль пришла).
    available: bool,
    /// Shift+C: метки видны.
    visible: bool,
    show_resolved: bool,
    /// По идентификатору: порядок рисования стабилен от кадра к кадру.
    pins: BTreeMap<Arc<str>, CommentPin>,
    open: Option<Arc<str>>,
    draft: Option<PinAnchor>,
    press: Option<Press>,
    /// Над меткой без нажатия: курсор — стрелка, а не прицел.
    hover: bool,
    /// Перейти к обсуждению, когда его метка или страница дойдут.
    focus: Option<Arc<str>>,
    /// Растёт при всём, что видно у меток, кроме сцены и камеры.
    pub(crate) revision: u64,
    layout: RefCell<Option<(LayoutKey, Arc<Layout>)>>,
    sent_anchor: Option<(Point, bool)>,
    sent_state: Option<(bool, bool, bool)>,
}

impl Default for Comments {
    fn default() -> Self {
        Comments {
            available: false,
            visible: true,
            show_resolved: false,
            pins: BTreeMap::new(),
            open: None,
            draft: None,
            press: None,
            hover: false,
            focus: None,
            revision: 0,
            layout: RefCell::new(None),
            sent_anchor: None,
            sent_state: None,
        }
    }
}

/// Мировая точка метки: узел есть и виден — по его матрице в сцене, иначе последняя мировая.
fn pin_world(scene: &ferrum_scene::Scene, a: &PinAnchor) -> Point {
    if let Some(slot) = a.node.and_then(|id| scene.slot(id)) {
        if scene.is_visible(slot) {
            let p = scene.at(slot).world * a.local;
            if p.x.is_finite() && p.y.is_finite() {
                return p;
            }
        }
    }
    a.world
}

/// Группы меток на экране. Первый проход: метка ложится в первую группу, чья первая метка
/// ближе `PIN_CLUSTER_DISTANCE`, иначе начинает свою (сетка ячеек — O(n)). Дальше группы, чьи
/// середины ближе того же, сливаются (не больше `CLUSTER_PASSES` проходов): иначе ряд меток давал
/// бы цепочку перекрытых кругов. Сцепления «каждая с соседней» нет: строка из 1000 меток с шагом
/// меньше капли не становится одной группой через весь экран.
fn cluster(points: &[Point]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = (0..points.len()).map(|i| vec![i]).collect();
    let mut centers: Vec<Point> = points.to_vec();
    for _ in 0..CLUSTER_PASSES {
        let merged = seed_groups(&centers);
        if merged.len() == groups.len() {
            break;
        }
        let next: Vec<Vec<usize>> = merged
            .iter()
            .map(|m| m.iter().flat_map(|g| groups[*g].iter().copied()).collect())
            .collect();
        centers = next
            .iter()
            .map(|g| {
                let n = g.len() as f64;
                g.iter()
                    .fold(Point::ZERO, |a, i| a + points[*i].to_vec2() / n)
            })
            .collect();
        groups = next;
    }
    groups
}

/// Проходов слияния групп в `cluster`.
const CLUSTER_PASSES: usize = 4;

/// Один проход: точка — в группу первой точки ближе `PIN_CLUSTER_DISTANCE`, иначе своя.
fn seed_groups(points: &[Point]) -> Vec<Vec<usize>> {
    let cell = |p: Point| {
        (
            (p.x / PIN_CLUSTER_DISTANCE).floor() as i64,
            (p.y / PIN_CLUSTER_DISTANCE).floor() as i64,
        )
    };
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let (cx, cy) = cell(*p);
        let mut found = None;
        'look: for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = grid.get(&(cx + dx, cy + dy)) {
                    for g in list {
                        if points[groups[*g][0]].distance(*p) < PIN_CLUSTER_DISTANCE {
                            found = Some(*g);
                            break 'look;
                        }
                    }
                }
            }
        }
        match found {
            Some(g) => groups[g].push(i),
            None => {
                grid.entry((cx, cy)).or_default().push(groups.len());
                groups.push(vec![i]);
            }
        }
    }
    groups
}

impl Editor {
    fn comments_bump(&mut self) {
        self.comments.revision += 1;
    }

    /// Хозяин сказал, есть ли комментарии (файл на сервере, роль пришла). Без них метки не
    /// рисуются, команды «Комментарий» и Shift+C недоступны, режим комментариев кончается.
    pub fn set_comments_available(&mut self, on: bool) {
        if self.comments.available == on {
            return;
        }
        self.comments.available = on;
        if !on {
            self.comments.press = None;
            self.comments.open = None;
            self.comments.draft = None;
            self.comments.focus = None;
            if self.tool.kind() == ToolKind::Comment {
                self.set_tool(ToolKind::Move);
            }
        }
        self.comments_bump();
        self.emit();
    }

    pub fn comments_available(&self) -> bool {
        self.comments.available
    }

    /// Видны ли метки (Shift+C).
    pub fn comments_visible(&self) -> bool {
        self.comments.visible
    }

    pub(crate) fn toggle_comments(&mut self) {
        self.comments.visible = !self.comments.visible;
        if !self.comments.visible {
            self.comments.press = None;
            self.comments.hover = false;
        }
        self.comments_bump();
    }

    /// Вход в режим комментариев показывает метки **(поведение, как в Figma)**.
    pub(crate) fn comment_mode_entered(&mut self) {
        if !self.comments.visible {
            self.comments.visible = true;
            self.comments_bump();
        }
    }

    pub fn set_comments_show_resolved(&mut self, on: bool) {
        if self.comments.show_resolved != on {
            self.comments.show_resolved = on;
            self.comments_bump();
            self.emit();
        }
    }

    /// Метки: `replace` — весь набор, иначе добавить и заменить по идентификатору. Сверх
    /// `COMMENT_PINS_MAX` новые не принимаются.
    pub fn set_comment_pins(&mut self, pins: Vec<CommentPin>, replace: bool) {
        if replace {
            self.comments.pins.clear();
        }
        for p in pins {
            let known = self.comments.pins.contains_key(&p.id);
            if !known && self.comments.pins.len() >= COMMENT_PINS_MAX {
                continue;
            }
            self.comments.pins.insert(p.id.clone(), p);
        }
        self.forget_missing_pins();
        self.comments_bump();
        self.retry_comment_focus();
        self.emit();
    }

    pub fn remove_comment_pins(&mut self, ids: &[&str]) {
        let mut any = false;
        for id in ids {
            any |= self.comments.pins.remove(*id).is_some();
        }
        if any {
            self.forget_missing_pins();
            self.comments_bump();
            self.emit();
        }
    }

    /// Открытая и тянущаяся метки, которых больше нет, — закрыты, жест отменён.
    fn forget_missing_pins(&mut self) {
        let pins = &self.comments.pins;
        if self
            .comments
            .open
            .as_ref()
            .is_some_and(|id| !pins.contains_key(id))
        {
            self.comments.open = None;
        }
        if matches!(&self.comments.press, Some(Press::Pin { id, .. }) if !pins.contains_key(id)) {
            self.comments.press = None;
        }
    }

    pub fn comment_pin(&self, id: &str) -> Option<&CommentPin> {
        self.comments.pins.get(id)
    }

    pub fn comment_pin_count(&self) -> usize {
        self.comments.pins.len()
    }

    /// Открытое обсуждение: его метка выделена, за ней следуют события `CommentAnchor`.
    pub fn set_comment_open(&mut self, id: Option<&str>) {
        let id: Option<Arc<str>> = id.map(Arc::from);
        if self.comments.open != id {
            self.comments.open = id;
            self.comments_bump();
        }
        // Открыть другое — переход к прежнему, если он ещё ждал, не нужен.
        self.comments.focus = None;
        self.emit();
    }

    pub fn comment_open(&self) -> Option<&str> {
        self.comments.open.as_deref()
    }

    /// Черновик нового комментария: его метка, пока пишут текст.
    pub fn set_comment_draft(&mut self, draft: Option<PinAnchor>) {
        if self.comments.draft != draft {
            self.comments.draft = draft;
            self.comments_bump();
        }
        self.emit();
    }

    /// Перейти к обсуждению (панель, ссылка): страница метки, камера по центру метки (зум тот
    /// же), обсуждение открыто, событие `CommentOpened`. Метки или страницы ещё нет — переход,
    /// как только они дойдут.
    pub fn comment_focus(&mut self, id: &str) {
        self.comments.focus = Some(Arc::from(id));
        self.retry_comment_focus();
        self.emit();
    }

    pub(crate) fn retry_comment_focus(&mut self) {
        let Some(id) = self.comments.focus.clone() else {
            return;
        };
        let Some(page) = self.comments.pins.get(&id).map(|p| p.anchor.page) else {
            return;
        };
        if page != self.page {
            if self.doc.page(page).is_err() {
                return;
            }
            if self.set_page(page).is_err() {
                return;
            }
        }
        self.comments.focus = None;
        let pin = &self.comments.pins[&id];
        let world = pin_world(&self.scene, &pin.anchor);
        self.camera.set_center(world);
        if self.comments.open.as_ref() != Some(&id) {
            self.comments.open = Some(id.clone());
        }
        self.comments_bump();
        let screen = self.camera.world_to_screen(world);
        self.out
            .events
            .push(EditorEvent::CommentOpened { id, screen });
    }

    /// Ключ раскладки (`LayoutKey`).
    fn layout_key(&self) -> LayoutKey {
        (
            self.comments.revision,
            self.scene.change_counter(),
            self.scene.generation(),
            self.scene_epoch,
            self.page,
            self.camera.clone(),
        )
    }

    /// Раскладка меток этой страницы: из кэша, пока не сменились метки, сцена или камера.
    pub(crate) fn comment_layout(&self) -> Arc<Layout> {
        let key = self.layout_key();
        if let Some((k, l)) = &*self.comments.layout.borrow() {
            if *k == key {
                return l.clone();
            }
        }
        let l = Arc::new(self.build_comment_layout());
        *self.comments.layout.borrow_mut() = Some((key, l.clone()));
        l
    }

    fn build_comment_layout(&self) -> Layout {
        let c = &self.comments;
        if !c.available {
            return Layout::default();
        }
        let page = self.page;
        let cam = &self.camera;
        let dragging = match &c.press {
            Some(Press::Pin {
                id, to: Some(to), ..
            }) => Some((id.clone(), *to)),
            _ => None,
        };
        // Открытая и тянущаяся — отдельно, поверх групп.
        let mut singles: Vec<(Arc<str>, Point)> = Vec::new();
        let mut ids: Vec<&CommentPin> = Vec::new();
        let mut points: Vec<Point> = Vec::new();
        // Метки за краем холста не раскладываются и не рисуются (поле — на группу у края).
        let reach = Rect::from_origin_size(Point::ZERO, cam.viewport())
            .inflate(2.0 * PIN_SIZE, 2.0 * PIN_SIZE);
        if c.visible {
            for p in c.pins.values() {
                if p.anchor.page != page {
                    continue;
                }
                let open = c.open.as_ref() == Some(&p.id);
                if p.resolved && !c.show_resolved && !open {
                    continue;
                }
                let world = match &dragging {
                    Some((id, to)) if *id == p.id => *to,
                    _ => pin_world(&self.scene, &p.anchor),
                };
                if open || dragging.as_ref().is_some_and(|(id, _)| *id == p.id) {
                    singles.push((p.id.clone(), world));
                    continue;
                }
                let s = cam.world_to_screen(world);
                if !(s.x.is_finite() && s.y.is_finite() && reach.contains(s)) {
                    continue;
                }
                ids.push(p);
                points.push(s);
            }
        } else if let Some(open) = c.open.as_ref().and_then(|id| c.pins.get(id)) {
            // Метки скрыты (Shift+C), но обсуждение открыто: его метку видно, как в Figma.
            if open.anchor.page == page {
                singles.push((open.id.clone(), pin_world(&self.scene, &open.anchor)));
            }
        }
        let screens: Vec<Point> = points.clone();
        let mut marks: Vec<PinMark> = Vec::new();
        let mut hits: Vec<Hit> = Vec::new();
        let pin_kind = |p: &CommentPin, open: bool| PinKind::Pin {
            initials: p.initials.clone(),
            color: p.color,
            replies: p.replies,
            unread: p.unread,
            resolved: p.resolved,
            open,
        };
        for g in cluster(&screens) {
            if g.len() == 1 {
                let p = ids[g[0]];
                let kind = pin_kind(p, false);
                marks.push(PinMark {
                    at: cam.screen_to_world(screens[g[0]]),
                    kind: kind.clone(),
                });
                hits.push(Hit {
                    at: screens[g[0]],
                    kind,
                    target: Target::Pin(p.id.clone()),
                });
                continue;
            }
            let n = g.len() as f64;
            let center = g
                .iter()
                .fold(Point::ZERO, |a, i| a + screens[*i].to_vec2() / n);
            // Группа — кругом над серединой капель её меток.
            let center = center + kurbo::Vec2::new(PIN_SIZE / 2.0, -PIN_SIZE / 2.0);
            let kind = PinKind::Cluster {
                count: g.len() as u32,
                unread: g.iter().any(|i| ids[*i].unread),
            };
            marks.push(PinMark {
                at: cam.screen_to_world(center),
                kind: kind.clone(),
            });
            hits.push(Hit {
                at: center,
                kind,
                target: Target::Cluster(
                    g.iter().map(|i| cam.screen_to_world(screens[*i])).collect(),
                ),
            });
        }
        let viewport = Rect::from_origin_size(Point::ZERO, cam.viewport());
        let mut anchor = None;
        for (id, world) in singles {
            let p = &c.pins[&id];
            let open = c.open.as_ref() == Some(&id);
            let kind = pin_kind(p, open);
            let s = cam.world_to_screen(world);
            if open {
                anchor = Some((s, viewport.contains(s)));
            }
            marks.push(PinMark {
                at: world,
                kind: kind.clone(),
            });
            hits.push(Hit {
                at: s,
                kind,
                target: Target::Pin(id),
            });
        }
        if let Some(d) = c.draft.filter(|d| d.page == page) {
            let world = pin_world(&self.scene, &d);
            let s = cam.world_to_screen(world);
            anchor = Some((s, viewport.contains(s)));
            marks.push(PinMark {
                at: world,
                kind: PinKind::Draft,
            });
        }
        // Открытое обсуждение на другой странице — не видно.
        if anchor.is_none() && (c.open.is_some() || c.draft.is_some()) {
            anchor = Some((Point::ZERO, false));
        }
        Layout {
            marks: marks.into(),
            hits,
            anchor,
        }
    }

    /// Метки для оверлея.
    pub(crate) fn comment_marks(&self) -> Arc<[PinMark]> {
        self.comment_layout().marks.clone()
    }

    /// Что под экранной точкой: верхняя метка или группа.
    fn comment_hit(&self, p: Point) -> Option<Hit> {
        if !self.comments.available {
            return None;
        }
        let l = self.comment_layout();
        l.hits
            .iter()
            .rev()
            .find(|h| pin_contains(&h.kind, h.at, p))
            .cloned()
    }

    /// Метки перехватывают нажатие раньше выделения на холсте: в режиме комментариев — всегда,
    /// иначе — у «Перемещения» и «Масштаба», когда метки видны **(поведение)**: рисующий
    /// инструмент рисует и поверх метки. Рука (и удержание Space) метки не трогает.
    pub(crate) fn comment_press(&mut self, e: &PointerEvent) -> bool {
        let mode = self.tool.kind() == ToolKind::Comment;
        if !self.comments.available || (!mode && !self.tool.kind().is_select_like()) {
            return false;
        }
        let hit = self.comment_hit(e.point);
        let press = match hit {
            Some(Hit {
                target: Target::Pin(id),
                ..
            }) => {
                let p = &self.comments.pins[&id];
                Press::Pin {
                    from: pin_world(&self.scene, &p.anchor),
                    movable: p.movable,
                    id,
                    start: e.point,
                    grab: self.camera.screen_to_world(e.point),
                    to: None,
                }
            }
            Some(Hit {
                target: Target::Cluster(members),
                ..
            }) => Press::Cluster { members },
            None if mode => Press::Place {
                start: e.point,
                moved: false,
            },
            None => return false,
        };
        self.comments.press = Some(press);
        self.comments.hover = false;
        true
    }

    /// Движение с нажатой меткой: тянется своя (за порогом), остальное ждёт отпускания.
    /// `false` — нажатия на метку нет.
    pub(crate) fn comment_drag(&mut self, e: &PointerEvent) -> bool {
        let world = self.camera.screen_to_world(e.point);
        let Some(press) = &mut self.comments.press else {
            return false;
        };
        let mut moved = false;
        match press {
            Press::Pin {
                start,
                grab,
                from,
                to,
                movable,
                ..
            } => {
                let d = e.point - *start;
                if *movable && (to.is_some() || d.hypot() > DRAG_THRESHOLD_PX) {
                    let next = *from + (world - *grab);
                    if *to != Some(next) {
                        *to = Some(next);
                        moved = true;
                    }
                }
            }
            Press::Place { start, moved: m } => {
                *m |= (e.point - *start).hypot() > DRAG_THRESHOLD_PX;
            }
            Press::Cluster { .. } => {}
        }
        if moved {
            self.comments_bump();
        }
        true
    }

    /// Отпускание: щелчок по метке открывает её, протянутая метка встаёт на место (событие
    /// `CommentMoved`), щелчок по группе вписывает её метки в экран, щелчок по холсту в режиме
    /// комментариев — место нового (`CommentPlace`). `false` — нажатия на метку не было.
    pub(crate) fn comment_release(&mut self, e: &PointerEvent) -> bool {
        let Some(press) = self.comments.press.take() else {
            return false;
        };
        match press {
            Press::Pin { id, to: None, .. } => {
                let world = self
                    .comments
                    .pins
                    .get(&id)
                    .map(|p| pin_world(&self.scene, &p.anchor));
                if let Some(world) = world {
                    self.comments.open = Some(id.clone());
                    self.comments.focus = None;
                    let screen = self.camera.world_to_screen(world);
                    self.out
                        .events
                        .push(EditorEvent::CommentOpened { id, screen });
                }
            }
            Press::Pin {
                id, to: Some(to), ..
            } => {
                let anchor = self.anchor_at(to);
                // Сразу на новом месте, пока сервер не ответил; отказ — хозяин пришлёт метку
                // заново **(поведение)**.
                if let Some(p) = self.comments.pins.get_mut(&id) {
                    p.anchor = anchor;
                }
                self.out
                    .events
                    .push(EditorEvent::CommentMoved { id, anchor });
            }
            Press::Cluster { members, .. } => self.zoom_to_pins(&members),
            Press::Place { moved: false, .. } => {
                let anchor = self.anchor_at(self.camera.screen_to_world(e.point));
                self.out.events.push(EditorEvent::CommentPlace {
                    anchor,
                    screen: e.point,
                });
            }
            Press::Place { moved: true, .. } => {}
        }
        self.comments_bump();
        true
    }

    /// Жест метки отменён (Esc, потеря фокуса): протянутая метка возвращается, события нет.
    pub(crate) fn cancel_comment_press(&mut self) -> bool {
        if self.comments.press.take().is_some() {
            self.comments_bump();
            return true;
        }
        false
    }

    pub(crate) fn comment_pressed(&self) -> bool {
        self.comments.press.is_some()
    }

    /// Курсор у меток: тянется — «взята», над меткой — стрелка.
    pub(crate) fn comment_cursor(&self) -> Option<crate::tools::Cursor> {
        use crate::tools::Cursor;
        match &self.comments.press {
            Some(Press::Pin { to: Some(_), .. }) => Some(Cursor::Grabbing),
            Some(Press::Pin { .. } | Press::Cluster { .. }) => Some(Cursor::Default),
            _ if self.comments.hover => Some(Cursor::Default),
            _ => None,
        }
    }

    /// Наведение на метку (курсор): без нажатия, только где метки ловят нажатие.
    pub(crate) fn comment_hover(&mut self, e: &PointerEvent) {
        let kind = self.tool.kind();
        let catches =
            self.comments.available && (kind == ToolKind::Comment || kind.is_select_like());
        self.comments.hover = catches && self.comment_hit(e.point).is_some();
    }

    /// Привязка точки мира: верхний узел страницы под ней (по тому, что видно) и точка в его
    /// координатах; мимо узлов — мир. Вложенный фрейм — к верхнему **(поведение)**.
    pub(crate) fn anchor_at(&self, world: Point) -> PinAnchor {
        let tol = HIT_TOLERANCE_PX / self.camera.zoom();
        let top = self.scene.hit_test(world, tol).and_then(|id| {
            let mut slot = self.scene.slot(id)?;
            while let Some(p) = self.scene.at(slot).parent {
                slot = p;
            }
            Some(slot)
        });
        if let Some(slot) = top {
            let n = self.scene.at(slot);
            if n.world.determinant().abs() > 1e-12 {
                let local = n.world.inverse() * world;
                if local.x.is_finite() && local.y.is_finite() {
                    return PinAnchor {
                        page: self.page,
                        node: Some(n.id),
                        local,
                        world,
                    };
                }
            }
        }
        PinAnchor {
            page: self.page,
            node: None,
            local: world,
            world,
        }
    }

    /// Щелчок по группе: её метки — в экран с полем, но не меньше чем вдвое ближе: совпавшие
    /// метки иначе не разошлись бы ни от одного щелчка **(поведение)**.
    fn zoom_to_pins(&mut self, members: &[Point]) {
        let Some(b) = members
            .iter()
            .map(|p| Rect::from_points(*p, *p))
            .reduce(|a, b| a.union(b))
        else {
            return;
        };
        let before = self.camera.zoom();
        self.camera.fit(b, PIN_CLUSTER_FIT_PADDING);
        if self.camera.zoom() < before * 2.0 {
            self.camera.set_zoom((before * 2.0).min(MAX_ZOOM));
            self.camera.set_center(b.center());
        }
    }

    /// Состояние меток и точка открытой — интерфейсу, только при изменении (из `emit`).
    pub(crate) fn emit_comments(&mut self) {
        let state = (
            self.tool.kind() == ToolKind::Comment,
            self.comments.visible,
            self.comments.available,
        );
        if self.comments.sent_state != Some(state) {
            self.comments.sent_state = Some(state);
            self.out.events.push(EditorEvent::CommentsState {
                mode: state.0,
                visible: state.1,
                available: state.2,
            });
        }
        let anchor = if self.comments.open.is_some() || self.comments.draft.is_some() {
            self.comment_layout().anchor
        } else {
            None
        };
        if anchor != self.comments.sent_anchor {
            // Открытого не стало — последний раз «не видно» на прежнем месте.
            let (screen, visible) = anchor.unwrap_or((
                self.comments.sent_anchor.map_or(Point::ZERO, |a| a.0),
                false,
            ));
            self.comments.sent_anchor = anchor;
            self.out
                .events
                .push(EditorEvent::CommentAnchor { screen, visible });
        }
    }

    /// Вид меток для ключа кадра: ревизия; сцена и камера — уже в ключе.
    pub(crate) fn comments_revision(&self) -> u64 {
        self.comments.revision
    }

    /// Только для тестов и замеров: экранные точки и виды меток раскладки (группа — с числом).
    #[doc(hidden)]
    pub fn comment_layout_for_tests(&self) -> Vec<(Point, PinKind)> {
        self.comment_layout()
            .hits
            .iter()
            .map(|h| (h.at, h.kind.clone()))
            .collect()
    }
}
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
use crate::text_edit::{EnterAt, TextEdit};
use crate::tools::{
    sync, Ctx, Cursor, DrawTool, Flow, HandTool, Outbox, SelectTool, Selection, Shape, TextTool,
    Tool, ToolKind, ToolOverlay,
};
use crate::vector_edit::{Drag, VectorEdit, VectorTool};
````

на:

````rust
use crate::text_edit::{EnterAt, TextEdit};
use crate::tools::{
    sync, CommentTool, Ctx, Cursor, DrawTool, Flow, HandTool, Outbox, SelectTool, Selection, Shape,
    TextTool, Tool, ToolKind, ToolOverlay,
};
use crate::vector_edit::{Drag, VectorEdit, VectorTool};
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// холсте, уходом участника, сменой страницы.
    Following(Option<u32>),
}

````

на:

````rust
    /// холсте, уходом участника, сменой страницы.
    Following(Option<u32>),
    /// Комментарии (спека подсистемы 4, раздел 10.3). Щелчок по холсту в режиме комментариев:
    /// место новой метки и экранная точка щелчка (CSS-пиксели) — интерфейс открывает поле.
    CommentPlace {
        anchor: crate::comments::PinAnchor,
        screen: Point,
    },
    /// Обсуждение открыто щелчком по метке или переходом к нему: окно — у экранной точки метки.
    CommentOpened {
        id: Arc<str>,
        screen: Point,
    },
    /// Метку перетащили на новое место (только своя или владельцу файла).
    CommentMoved {
        id: Arc<str>,
        anchor: crate::comments::PinAnchor,
    },
    /// Экранная точка открытой метки (или черновика) сменилась: окно едет за ней. `visible` —
    /// метка на этой странице и в пределах холста.
    CommentAnchor {
        screen: Point,
        visible: bool,
    },
    /// Режим комментариев, видны ли метки (Shift+C), есть ли комментарии вообще.
    CommentsState {
        mode: bool,
        visible: bool,
        available: bool,
    },
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Другие участники: их курсоры, рамки, плавное движение.
    presence: u64,
    /// Подсказки привязки тянущейся линии: меняются без сдвига самой линии.
    guide_snap: Vec<kurbo::Line>,
````

на:

````rust
    /// Другие участники: их курсоры, рамки, плавное движение.
    presence: u64,
    /// Метки комментариев: набор, открытая, жест; сцена и камера — уже в ключе.
    comments: u64,
    /// Подсказки привязки тянущейся линии: меняются без сдвига самой линии.
    guide_snap: Vec<kurbo::Line>,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Указатель над холстом: курсор виден другим.
    pub(crate) pointer_inside: bool,
    /// Превью `SetGuides`: список владельца вместо его направляющих, пока жест идёт.
    pub(crate) guides_preview: Option<(ferrum_doc::GuideOwner, Vec<ferrum_doc::Guide>)>,
````

на:

````rust
    /// Указатель над холстом: курсор виден другим.
    pub(crate) pointer_inside: bool,
    /// Метки комментариев (спека подсистемы 4, раздел 10).
    pub(crate) comments: crate::comments::Comments,
    /// Превью `SetGuides`: список владельца вместо его направляющих, пока жест идёт.
    pub(crate) guides_preview: Option<(ferrum_doc::GuideOwner, Vec<ferrum_doc::Guide>)>,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                active_guide: None,
                presence: 0,
                guide_snap: Vec::new(),
                measures: Vec::new(),
````

на:

````rust
                active_guide: None,
                presence: 0,
                comments: 0,
                guide_snap: Vec::new(),
                measures: Vec::new(),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            guide_hover: None,
            presence: Default::default(),
            pointer_inside: false,
        })
````

на:

````rust
            guide_hover: None,
            presence: Default::default(),
            comments: Default::default(),
            pointer_inside: false,
        })
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        if self.pan.is_some() {
            Cursor::Grabbing
        } else if let Some(c) = self.guide_cursor() {
            c
````

на:

````rust
        if self.pan.is_some() {
            Cursor::Grabbing
        } else if let Some(c) = self.comment_cursor() {
            c
        } else if let Some(c) = self.guide_cursor() {
            c
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            active_guide: self.active_guide(),
            remote: self.remote_marks(),
            // Метки комментариев раскладывает редактор (этап 5, следующая задача).
            comments: Arc::from(Vec::new()),
        }
    }
````

на:

````rust
            active_guide: self.active_guide(),
            remote: self.remote_marks(),
            comments: self.comment_marks(),
        }
    }
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
                // Метки комментариев — раньше выделения на холсте и правки (`comment_press`:
                // рука и её удержание Space метки не трогают).
                if self.comment_press(&e) {
                    return;
                }
                // Правка текста: нажатие в тексте ставит курсор; мимо — правка
                // кончается, и нажатие достаётся инструменту.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            return;
        }
        // Над линейкой инструмента нет: ни наведения, ни его курсора.
        if self.ruler_hover_update(&e) {
````

на:

````rust
            return;
        }
        if self.comment_drag(&e) {
            return;
        }
        self.comment_hover(&e);
        // Над линейкой инструмента нет: ни наведения, ни его курсора.
        if self.ruler_hover_update(&e) {
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
        if e.button == PointerButton::Primary && self.comment_release(&e) {
            return;
        }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// местом мира, жест пересчитывается.
    fn refresh_gesture(&mut self) {
        if let (true, Some(e)) = (self.guide_drag.is_some(), self.last_pointer) {
            self.guide_drag_move(&e);
````

на:

````rust
    /// местом мира, жест пересчитывается.
    fn refresh_gesture(&mut self) {
        if let (true, Some(e)) = (self.comment_pressed(), self.last_pointer) {
            self.comment_drag(&e);
        }
        if let (true, Some(e)) = (self.guide_drag.is_some(), self.last_pointer) {
            self.guide_drag_move(&e);
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust

    fn key_down(&mut self, e: KeyEvent) {
        // Правка текста: раскладка клавиш не действует (спека подсистемы 3, 7).
        if self.text_edit.is_some() {
````

на:

````rust

    fn key_down(&mut self, e: KeyEvent) {
        // Нажатая метка: Esc возвращает протянутую на место, остальные клавиши ждут отпускания.
        if self.comment_pressed() {
            if e.code == "Escape" {
                self.cancel_comment_press();
            }
            return;
        }
        // Правка текста: раскладка клавиш не действует (спека подсистемы 3, 7).
        if self.text_edit.is_some() {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// не меняются.
    pub fn cancel_gesture(&mut self) {
        self.cancel_panel();
        self.cancel_guides();
````

на:

````rust
    /// не меняются.
    pub fn cancel_gesture(&mut self) {
        self.cancel_comment_press();
        self.cancel_panel();
        self.cancel_guides();
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Выбирает инструмент (тулбар, горячая клавиша). Жест текущего отменяется.
    pub fn set_tool(&mut self, kind: ToolKind) {
        self.cancel_gesture();
        self.end_text_edit();
````

на:

````rust
    /// Выбирает инструмент (тулбар, горячая клавиша). Жест текущего отменяется.
    pub fn set_tool(&mut self, kind: ToolKind) {
        // Без комментариев (локальный файл, роль не пришла) режима комментариев нет: ни
        // клавишей, ни тулбаром.
        if kind == ToolKind::Comment && !self.comments_available() {
            return;
        }
        self.cancel_gesture();
        self.end_text_edit();
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            ToolKind::Hand => Box::new(HandTool::new()),
            ToolKind::Text => Box::new(TextTool::new()),
            ToolKind::Pen => Box::new(SelectTool::new()),
            // Изображению нужны хэш и размер: интерфейс сначала выбирает файл.
````

на:

````rust
            ToolKind::Hand => Box::new(HandTool::new()),
            ToolKind::Text => Box::new(TextTool::new()),
            ToolKind::Comment => {
                self.comment_mode_entered();
                Box::new(CommentTool)
            }
            ToolKind::Pen => Box::new(SelectTool::new()),
            // Изображению нужны хэш и размер: интерфейс сначала выбирает файл.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.update_measures();
        self.refresh_guides();
        let now = Seen {
            selection: self.selection.version(),
````

на:

````rust
        self.update_measures();
        self.refresh_guides();
        self.emit_comments();
        let now = Seen {
            selection: self.selection.version(),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                active_guide: self.active_guide(),
                presence: self.presence_revision(),
                guide_snap: self
                    .guide_drag
````

на:

````rust
                active_guide: self.active_guide(),
                presence: self.presence_revision(),
                comments: self.comments_revision(),
                guide_snap: self
                    .guide_drag
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod command_state;
pub mod commands;
pub mod context_menu;
mod editor;
````

на:

````rust
pub mod command_state;
pub mod commands;
pub mod comments;
pub mod context_menu;
mod editor;
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub use command_state::CommandState;
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
pub use context_menu::{ContextTarget, MAX_SELECT_LAYER_ITEMS};
pub use editor::{Editor, EditorEvent, Settings, ThemeChoice, WHEEL_ZOOM_SPEED};
````

на:

````rust
pub use command_state::CommandState;
pub use commands::{Command, CommandGroup, CommandId, COMMANDS};
pub use comments::{CommentPin, PinAnchor, COMMENT_PINS_MAX, PIN_CLUSTER_DISTANCE};
pub use context_menu::{ContextTarget, MAX_SELECT_LAYER_ITEMS};
pub use editor::{Editor, EditorEvent, Settings, ThemeChoice, WHEEL_ZOOM_SPEED};
````

В `crates/ferrum-editor/src/menu.rs` заменить:

````rust
                I(C::ViewGuides),
                S,
                I(C::ToggleUi),
                S,
````

на:

````rust
                I(C::ViewGuides),
                S,
                // Спека подсистемы 4, раздел 10.3: режим комментариев — и в меню «Вид».
                I(C::ToolComment),
                I(C::ViewComments),
                S,
                I(C::ToggleUi),
                S,
````

В `crates/ferrum-editor/src/remote.rs` заменить:

````rust
            self.follow_peer();
        }
        self.emit();
        Ok(RemoteEffects {
````

на:

````rust
            self.follow_peer();
        }
        // Переход к обсуждению на странице, которой ещё не было, — тоже.
        self.retry_comment_focus();
        self.emit();
        Ok(RemoteEffects {
````

Создать `crates/ferrum-editor/src/tools/comment.rs`:

````rust
//! «Комментарий» (C, спека подсистемы 4, раздел 10.3): режим комментариев. Нажатия в нём
//! целиком у меток (`Editor::comment_press`: открыть, перетащить, поставить новую), поэтому сам
//! инструмент ничего не делает — пока он выбран, остальные инструменты не работают, наведения
//! и рамки выделения нет. Камера работает как обычно: колесо, щипок, Space и средняя кнопка,
//! клавиши зума — их редактор обрабатывает мимо инструмента.

use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay};
use crate::input::{Modifiers, PointerEvent};

#[derive(Default)]
pub struct CommentTool;

impl Tool for CommentTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Comment
    }

    fn pointer_down(&mut self, _: &mut Ctx, _: &PointerEvent) {}

    fn pointer_move(&mut self, cx: &mut Ctx, _: &PointerEvent) {
        *cx.hover = None;
    }

    fn pointer_up(&mut self, _: &mut Ctx, _: &PointerEvent) -> Flow {
        Flow::Stay
    }

    fn key(&mut self, _: &mut Ctx, _: Modifiers) {}

    fn cancel(&mut self, _: &mut Ctx) {}

    fn is_busy(&self) -> bool {
        false
    }

    /// Прицел — место новой метки **(поведение)**: своего курсора-«пузыря», как в Figma, у ядра
    /// нет, а интерфейс знает только CSS-курсоры.
    fn cursor(&self) -> Cursor {
        Cursor::Crosshair
    }

    fn overlay(&self) -> ToolOverlay {
        ToolOverlay::default()
    }
}
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
//! один шаг отмены; Esc убирает превью, и в истории ничего не остаётся.

mod draw;
mod hand;
````

на:

````rust
//! один шаг отмены; Esc убирает превью, и в истории ничего не остаётся.

mod comment;
mod draw;
mod hand;
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
mod text;

pub use draw::{DrawTool, Shape};
pub use hand::HandTool;
````

на:

````rust
mod text;

pub use comment::CommentTool;
pub use draw::{DrawTool, Shape};
pub use hand::HandTool;
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    /// «Масштаб» (K): ресайз выделения вместе с обводками, радиусами и текстом.
    Scale,
}

````

на:

````rust
    /// «Масштаб» (K): ресайз выделения вместе с обводками, радиусами и текстом.
    Scale,
    /// «Комментарий» (C, подсистема 4, этап 5): режим комментариев.
    Comment,
}

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        Tool::Arrow => ToolKind::Arrow,
        Tool::Scale => ToolKind::Scale,
    }
}
````

на:

````rust
        Tool::Arrow => ToolKind::Arrow,
        Tool::Scale => ToolKind::Scale,
        Tool::Comment => ToolKind::Comment,
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        ToolKind::Arrow => Tool::Arrow,
        ToolKind::Scale => Tool::Scale,
    }
}
````

на:

````rust
        ToolKind::Arrow => Tool::Arrow,
        ToolKind::Scale => Tool::Scale,
        ToolKind::Comment => Tool::Comment,
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                    self.events.push(CoreEvent::Following { participant })
                }
            }
        }
````

на:

````rust
                    self.events.push(CoreEvent::Following { participant })
                }
                // Комментарии: события протокола — следующая задача; без `CommentsAvailable`
                // редактор их и не шлёт, кроме состояния.
                EditorEvent::CommentPlace { .. }
                | EditorEvent::CommentOpened { .. }
                | EditorEvent::CommentMoved { .. }
                | EditorEvent::CommentAnchor { .. }
                | EditorEvent::CommentsState { .. } => {}
            }
        }
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
    /// Комментарий (C, подсистема 4, этап 5): только при `CommentsAvailable { on: true }`.
    Comment,
}

````

- [ ] **Step 4: Сгенерировать файлы**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm`
Expected: ядро собрано: `src/wasm`, в типах `Tool` появился `comment`

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm`

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npx playwright test e2e/app.spec.ts --project=chromium`

Expected: PASS: всё без замечаний; `ferrum-editor` и `ferrum-wasm` 834 passed; `e2e/app.spec.ts` 13 passed

- [ ] **Step 6: Закоммитить**

```bash
git add apps/web/e2e/app.spec.ts crates/ferrum-editor/examples/editor_report.rs crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/command_state.rs crates/ferrum-editor/src/commands.rs crates/ferrum-editor/src/comments.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/menu.rs crates/ferrum-editor/src/remote.rs crates/ferrum-editor/src/tools/comment.rs crates/ferrum-editor/src/tools/mod.rs crates/ferrum-editor/tests/command_table.rs crates/ferrum-editor/tests/comments.rs crates/ferrum-editor/tests/keymap.rs crates/ferrum-editor/tests/main_menu.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/boolean.rs
git commit -F - <<'EOF'
редактор: метки комментариев — режим «Комментарий», Shift+C, раскладка и жесты

comments.rs: метки обсуждений от хозяина (set_comment_pins / remove_comment_pins,
не больше COMMENT_PINS_MAX = 10 000), признак «комментарии есть»
(set_comments_available: без него ToolComment и ViewComments недоступны,
клавиши C и Shift+C молчат, SetTool не включает режим), видимость (Shift+C,
вход в режим показывает метки), показ закрытых, открытое обсуждение,
черновик, переход к обсуждению (comment_focus: страница метки, камера по
центру, CommentOpened; метки или страницы ещё нет — ждёт их прихода).

Привязка — к верхнему узлу страницы под точкой (вложенный фрейм — к
верхнему, поведение), x, y в его координатах; на пустом месте — мир. Каждый
кадр точка метки — матрица узла в сцене со своим превью и чужим живым
превью; узла нет или он скрыт — последняя мировая (поведение). Раскладка
кэшируется по ревизии меток, счётчику сцены со слоями, странице и камере;
метки за краем холста не раскладываются. Группы — в экранных пикселях:
ближе капли (28 px) слипаются, середины групп ближе того же сливаются;
щелчок по группе вписывает её метки, но не меньше чем вдвое ближе (поведение).

Попадание по метке — раньше выделения на холсте: в режиме «Комментарий»
всегда, иначе у «Перемещения» и «Масштаба» при видимых метках (поведение);
рука и Space метки не трогают. Щелчок — CommentOpened, своя метка тянется
(только превью, CommentMoved при отпускании, Esc возвращает), щелчок по
холсту в режиме — CommentPlace. ToolKind::Comment (CommentTool) глушит
остальные инструменты; камера работает. Метки не трогают документ,
историю, журнал и own_change_counter. События интерфейсу: CommentPlace,
CommentOpened, CommentMoved, CommentAnchor (только при изменении),
CommentsState. Ядро worker'а пока их не переводит (следующая задача);
protocol::Tool получил Comment.

editor_report, «раскладка кадра» (Editor::overlay после каждого движения,
медиана 60; 1050 меток: по 10 на 100 фреймах страницы из 100 000 узлов и 50
в мире; тянутся 10 фреймов — свои или чужое живое превью):
- зум 1 (13 меток на экране): 0.050 / 0.054 мс;
- зум 0,25 (98 меток): 0.085 / 0.088 мс;
- вся страница (84 группы): 0.343 / 0.363 мс — цель ≤ 1 мс.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3: Протокол — виды REST, событие `comment`, протокол 3

Спека §10, §7.2; «Решения» 3, 5. Пределы и нормализация текста — одна функция у клиента и сервера; строгий разбор запросов и ответов (лишнее поле — `null`); событие до 64 КБ. Вкладка протокола 2 получает `please-reload`.

**Files:**
- Create: `packages/protocol/src/comments.ts`
- Modify: `packages/protocol/src/files.ts`, `packages/protocol/src/index.ts`, `packages/protocol/src/sync.ts`
- Test: `apps/server/test/hub.test.ts`, `apps/server/test/presence.test.ts`, `packages/protocol/test/comments.test.ts` (новый), `packages/protocol/test/sync.test.ts`

**Interfaces:**
- Produces: `COMMENT_BODY_MAX = 4000`, `COMMENT_MENTIONS_MAX = 20`, `COMMENT_PAGE_MAX = 500`, `THREAD_COMMENTS_MAX = 500`, `MENTION_SUGGEST_MAX = 8`, `MENTION_QUERY_MAX = 64`, `THREAD_PARTICIPANTS_MAX = 50`, `COMMENT_EVENT_MAX_BYTES = 64 * 1024`; `commentLength`, `normalizeCommentBody`, `mentionIds`, `splitCommentBody`, `BodyPart`, `isCommentUserId`; `CommentAuthor`, `CommentDto`, `PinDto`, `ThreadDto`, `ThreadView`, `CommentListDto`, `ThreadDetailDto`, `CommentWriteDto`, `CommentAction`, `CommentEvent`, `MentionCandidate`, `MentionListDto`, `PrefsDto`; `parsePin`, `parseNewThread`, `parseCommentBody`, `parseThreadPatch`, `parsePrefs`, `parseMentionQuery`, `parseComment`, `parseThread`, `parseThreadView`, `parseCommentEvent`, `parseCommentList`, `parseThreadDetail`, `parseCommentWrite`, `parseMentionList`, `encodeCommentEvent`, `decodeCommentEvent`; `ServerFrame` `{t:'comment', event: CommentEvent}`; `SYNC_PROTOCOL = 3`; код ошибки `THREAD_FULL`.

- [ ] **Step 1: Написать тесты**

В `apps/server/test/hub.test.ts` заменить:

````ts
  await old.wait('please-reload');
  expect((await old.waitClosed()).code).toBe(SYNC_CLOSE.RELOAD);

  const huge = await connect(owner);
````

на:

````ts
  await old.wait('please-reload');
  expect((await old.waitClosed()).code).toBe(SYNC_CLOSE.RELOAD);
  // Вкладка прошлой сборки (протокол без кадра `comment`) тоже: кадра комментария она не разобрала бы.
  const stale = (await Client.open(s, owner, file))!;
  stale.hello(SYNC_PROTOCOL - 1);
  await stale.wait('please-reload');
  expect((await stale.waitClosed()).code).toBe(SYNC_CLOSE.RELOAD);

  const huge = await connect(owner);
````

В `apps/server/test/presence.test.ts` заменить:

````ts
  splitSelection,
  SYNC_CLOSE,
  syncPath,
  type PresenceItem,
````

на:

````ts
  splitSelection,
  SYNC_CLOSE,
  SYNC_PROTOCOL,
  syncPath,
  type PresenceItem,
````

В `apps/server/test/presence.test.ts` заменить:

````ts
  const mask = (p: Uint8Array): Buffer => Buffer.concat([Buffer.from([0x82, 0x80 | p.length]), Buffer.from([0, 0, 0, 0]), p]);
  await Bun.sleep(200);
  raw.write(mask(encodeClientFrame({ t: 'hello', protocol: 2, schema: 1 })));
  await Bun.sleep(300);
  raw.pause();
````

на:

````ts
  const mask = (p: Uint8Array): Buffer => Buffer.concat([Buffer.from([0x82, 0x80 | p.length]), Buffer.from([0, 0, 0, 0]), p]);
  await Bun.sleep(200);
  raw.write(mask(encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 })));
  await Bun.sleep(300);
  raw.pause();
````

Создать `packages/protocol/test/comments.test.ts`:

````ts
import { expect, test } from 'bun:test';
import {
  COMMENT_BODY_MAX,
  COMMENT_EVENT_MAX_BYTES,
  COMMENT_MENTIONS_MAX,
  commentLength,
  decodeCommentEvent,
  encodeCommentEvent,
  mentionIds,
  normalizeCommentBody,
  parseCommentBody,
  parseCommentEvent,
  parseCommentList,
  parseCommentWrite,
  parseMentionList,
  parseMentionQuery,
  parseNewThread,
  parsePrefs,
  parseThreadDetail,
  parseThreadPatch,
  splitCommentBody,
  THREAD_PARTICIPANTS_MAX,
  type CommentDto,
  type CommentEvent,
  type PinDto,
  type ThreadDto,
} from '../src/comments';
import { MAX_NAME_CHARS } from '../src/auth';

const T = '0b5c1d3e-7f40-4a8b-9c2d-1e2f3a4b5c6d';
const C = '1b5c1d3e-7f40-4a8b-9c2d-1e2f3a4b5c6d';
const U = 'Xk2PqR8sT0uVwYz1aB3cD5eF7gH9iJ4l';
const pin: PinDto = { page: '3@7910247476534078275', node: '12@7910247476534078275', x: 1.5, y: -2, wx: 100, wy: 200 };
const comment = (over: Partial<CommentDto> = {}): CommentDto => ({
  id: C,
  thread: T,
  author: { id: U, name: 'Анна' },
  body: 'Привет, @[' + U + ']',
  createdAt: 1_700_000_000_000,
  editedAt: null,
  mentions: { [U]: 'Анна' },
  ...over,
});
const thread = (over: Partial<ThreadDto> = {}): ThreadDto => ({
  id: T,
  pin,
  author: { id: U, name: 'Анна' },
  createdAt: 1_700_000_000_000,
  resolvedAt: null,
  resolvedBy: null,
  seq: 7,
  first: comment(),
  replies: 0,
  lastAt: 1_700_000_000_000,
  participants: [U],
  ...over,
});

test('длина — в кодовых точках, а не в UTF-16', () => {
  expect(commentLength('😀😀')).toBe(2);
  expect('😀😀'.length).toBe(4);
  const emoji = '😀'.repeat(COMMENT_BODY_MAX);
  expect(normalizeCommentBody(emoji)).toBe(emoji);
  expect(normalizeCommentBody(emoji + 'а')).toBeNull();
  expect(normalizeCommentBody('а'.repeat(COMMENT_BODY_MAX))).not.toBeNull();
});

test('текст: только текст — переводы строк, управляющие, направление, края', () => {
  expect(normalizeCommentBody('  a\r\nb\rc\n\td  ')).toBe('a\nb\nc\n\td');
  expect(normalizeCommentBody('a\u0000b\u0007c\u001bd\u007fe\u0085f\u009fg')).toBe('abcdefg');
  // RLO переворачивает показ: «exe.txt» выглядел бы иначе, чем есть.
  expect(normalizeCommentBody('file\u202etxt.exe\u2066x\u2069')).toBe('filetxt.exex');
  // Одиночный суррогат в базу не идёт.
  expect(normalizeCommentBody('a\ud800b')).toBe('a�b');
  for (const bad of ['', '   ', '\u202e', '\r\n', 1, null, undefined, {}, ['a']]) expect(normalizeCommentBody(bad)).toBeNull();
  // HTML не трогается: это текст, показывают его как текст.
  expect(normalizeCommentBody('<b>x</b>')).toBe('<b>x</b>');
  // Нормальный текст не меняется при повторе: сервер и клиент считают одно.
  const once = normalizeCommentBody(' x\r\ny\u202e ')!;
  expect(normalizeCommentBody(once)).toBe(once);
});

test('упоминания: без повторов, по порядку, только вид `@[id]`', () => {
  expect(mentionIds('@[b] и @[a] и снова @[b], @[] @[a b] @[x/y] @[' + 'z'.repeat(65) + ']')).toEqual(['b', 'a']);
  expect(splitCommentBody('Эй, @[a] и @[b]!', { a: 'Анна' })).toEqual([
    { text: 'Эй, ' },
    { mention: 'a', name: 'Анна' },
    { text: ' и ' },
    { mention: 'b', name: null },
    { text: '!' },
  ]);
  // Имя из прототипа — не имя: `@[constructor]` не показывает функцию.
  expect(splitCommentBody('@[constructor]', {})).toEqual([{ mention: 'constructor', name: null }]);
});

test('запросы: ровно нужные поля, предел упоминаний', () => {
  expect(parseNewThread({ pin, body: ' x ' })).toEqual({ pin, body: 'x' });
  expect(parseNewThread({ pin, body: 'x', extra: 1 })).toBeNull();
  expect(parseNewThread({ pin: { ...pin, page: 'root' }, body: 'x' })).toBeNull();
  expect(parseNewThread({ pin: { ...pin, node: '007@1' }, body: 'x' })).toBeNull();
  for (const v of [NaN, Infinity, 1e9 + 1, '1']) expect(parseNewThread({ pin: { ...pin, x: v }, body: 'x' }), String(v)).toBeNull();
  expect(parseNewThread({ pin: { ...pin, node: null }, body: 'x' })?.pin.node).toBeNull();
  expect(parseNewThread({ pin, body: '' })).toBeNull();
  const many = (n: number) => Array.from({ length: n }, (_, i) => `@[u${i}]`).join(' ');
  expect(parseCommentBody({ body: many(COMMENT_MENTIONS_MAX) })).not.toBeNull();
  expect(parseCommentBody({ body: many(COMMENT_MENTIONS_MAX + 1) })).toBeNull();
  // Повторы одного человека — одно упоминание.
  expect(parseCommentBody({ body: '@[a] '.repeat(100) })).not.toBeNull();
  expect(parseCommentBody({ body: 'x', pin })).toBeNull();
  expect(parseThreadPatch({ resolved: true })).toEqual({ resolved: true });
  expect(parseThreadPatch({ pin })).toEqual({ pin });
  expect(parseThreadPatch({ resolved: true, pin })).toBeNull();
  expect(parseThreadPatch({})).toBeNull();
  expect(parseThreadPatch({ resolved: 1 })).toBeNull();
  expect(parsePrefs({ commentMail: false })).toEqual({ commentMail: false });
  expect(parsePrefs({ commentMail: 'no' })).toBeNull();
  expect(parseMentionQuery('  ан ')).toBe('ан');
  expect(parseMentionQuery(null)).toBe('');
  expect(parseMentionQuery('a'.repeat(65))).toBeNull();
  expect(parseMentionQuery('a\u0000')).toBeNull();
});

test('событие туда и обратно; каждое поле проверено', () => {
  const events: CommentEvent[] = [
    { kind: 'thread', seq: 7, by: U, action: 'create', thread: thread(), comment: comment(), deleted: null },
    {
      kind: 'thread',
      seq: 7,
      by: U,
      action: 'delete',
      thread: thread({ resolvedAt: 5, resolvedBy: { id: null, name: '' } }),
      comment: null,
      deleted: C,
    },
    { kind: 'removed', seq: 9, by: U, thread: T },
    { kind: 'gap', seq: 0 },
  ];
  for (const e of events) expect(decodeCommentEvent(encodeCommentEvent(e))).toEqual(e);
  const good = events[0] as Extract<CommentEvent, { kind: 'thread' }>;
  const bad: unknown[] = [
    { ...good, extra: 1 },
    { ...good, action: 'hack' },
    { ...good, seq: -1 },
    { ...good, seq: 8 },
    { ...good, by: '' },
    { ...good, deleted: 'x' },
    { ...good, thread: { ...thread(), unread: true } },
    { ...good, thread: thread({ participants: Array(THREAD_PARTICIPANTS_MAX + 1).fill(U) }) },
    { ...good, thread: thread({ first: comment({ thread: C }) }) },
    { ...good, thread: thread({ pin: { ...pin, wx: Infinity } }) },
    { ...good, comment: comment({ body: '<b>\u202e</b>' }) },
    { ...good, comment: comment({ body: ' x' }) },
    { ...good, comment: comment({ thread: C }) },
    { ...good, comment: comment({ author: { id: U } as never }) },
    { ...good, comment: comment({ mentions: { [U]: 1 } as never }) },
    { ...good, comment: comment({ editedAt: 1.5 }) },
    { kind: 'removed', seq: 1, by: U, thread: 'nope' },
    { kind: 'gap', seq: 1, extra: true },
    { kind: 'other', seq: 1 },
    null,
    [],
  ];
  for (const b of bad) expect(parseCommentEvent(b), JSON.stringify(b)).toBeNull();
  // Не JSON, не UTF-8, пусто, больше предела.
  expect(decodeCommentEvent(new TextEncoder().encode('{'))).toBeNull();
  expect(decodeCommentEvent(new Uint8Array([0xff]))).toBeNull();
  expect(decodeCommentEvent(new Uint8Array(0))).toBeNull();
  expect(decodeCommentEvent(new Uint8Array(COMMENT_EVENT_MAX_BYTES + 1))).toBeNull();
});

test('id `__proto__` в упоминаниях — своё поле, а не прототип', () => {
  const raw = `{"kind":"thread","seq":7,"by":"${U}","action":"create","thread":${JSON.stringify(thread())},"comment":${JSON.stringify(
    comment({ mentions: {} }),
  ).replace('"mentions":{}', '"mentions":{"__proto__":"x"}')},"deleted":null}`;
  const e = decodeCommentEvent(new TextEncoder().encode(raw));
  expect(e?.kind).toBe('thread');
  const m = (e as Extract<CommentEvent, { kind: 'thread' }>).comment!.mentions;
  expect(Object.getPrototypeOf(m)).toBe(Object.prototype);
  expect(Object.hasOwn(m, '__proto__')).toBe(true);
});

test('худшее событие помещается в кадр: два предельных текста, 20 упоминаний с предельными именами, 50 участников', () => {
  const name = '𝔄'.repeat(MAX_NAME_CHARS);
  const ids = Array.from({ length: COMMENT_MENTIONS_MAX }, (_, i) => `${'u'.repeat(60)}${String(i).padStart(4, '0')}`);
  const mentions = Object.fromEntries(ids.map((id) => [id, name]));
  const prefix = ids.map((id) => `@[${id}]`).join('');
  // Текст из четырёхбайтных знаков: дороже в JSON не бывает (экранированные знаки — два байта на кодовую точку).
  const body = prefix + '𝔄'.repeat(COMMENT_BODY_MAX - commentLength(prefix));
  expect(commentLength(body)).toBe(COMMENT_BODY_MAX);
  const author = { id: ids[0], name };
  const big = comment({ body, mentions, author });
  const ev: CommentEvent = {
    kind: 'thread',
    seq: Number.MAX_SAFE_INTEGER,
    by: ids[0],
    action: 'reply',
    thread: thread({
      seq: Number.MAX_SAFE_INTEGER,
      author,
      resolvedBy: author,
      resolvedAt: 1,
      first: big,
      participants: Array.from({ length: THREAD_PARTICIPANTS_MAX }, () => ids[0]),
    }),
    comment: { ...big, id: C },
    deleted: null,
  };
  const bytes = encodeCommentEvent(ev);
  console.log('худшее событие комментария, байт:', bytes.length);
  expect(bytes.length).toBeLessThanOrEqual(COMMENT_EVENT_MAX_BYTES);
  expect(decodeCommentEvent(bytes)).toEqual(ev);
});

test('ответы REST разбираются строго', () => {
  const view = { ...thread(), unread: true };
  expect(parseCommentList({ seq: 3, threads: [view], removed: [T], more: false })).toEqual({
    seq: 3,
    threads: [view],
    removed: [T],
    more: false,
  });
  expect(parseCommentList({ seq: 3, threads: [thread()], removed: [], more: false })).toBeNull();
  expect(parseThreadDetail({ thread: view, comments: [comment()] })?.comments).toHaveLength(1);
  expect(parseThreadDetail({ thread: view, comments: [comment({ thread: C })] })).toBeNull();
  expect(parseCommentWrite({ seq: 7, thread: view, comment: null, removed: true })?.removed).toBe(true);
  expect(parseMentionList({ people: [{ id: U, name: 'А', email: null }] })).toEqual({ people: [{ id: U, name: 'А', email: null }] });
  expect(parseMentionList({ people: Array(9).fill({ id: U, name: 'А', email: null }) })).toBeNull();
});
````

В `packages/protocol/test/sync.test.ts` заменить:

````ts
    { t: 'please-reload' },
    { t: 'pong' },
  ];
  for (const f of frames) expect(parseServerFrame(encodeServerFrame(f))).toEqual(f);
});

````

на:

````ts
    { t: 'please-reload' },
    { t: 'pong' },
    { t: 'comment', event: { kind: 'gap', seq: 12 } },
    { t: 'comment', event: { kind: 'removed', seq: 13, by: 'u1', thread: '0b5c1d3e-7f40-4a8b-9c2d-1e2f3a4b5c6d' } },
  ];
  for (const f of frames) expect(parseServerFrame(encodeServerFrame(f))).toEqual(f);
});

test('кадр комментария: JSON строго, иначе — как обрыв', () => {
  const frame = (json: string) => new Uint8Array([27, ...new TextEncoder().encode(json)]);
  expect(parseServerFrame(frame('{"kind":"gap","seq":1}'))).toEqual({ t: 'comment', event: { kind: 'gap', seq: 1 } });
  for (const bad of ['', '{', '{"kind":"gap","seq":1,"x":1}', '{"kind":"gap","seq":"1"}', 'null', '[1]'])
    expect(parseServerFrame(frame(bad)), bad).toBeNull();
  // Кодирование не пишет того, что не прочтётся: событие больше предела — ошибка отправителя.
  const huge = { kind: 'removed' as const, seq: 1, by: 'x'.repeat(70_000), thread: '0b5c1d3e-7f40-4a8b-9c2d-1e2f3a4b5c6d' };
  expect(() => encodeServerFrame({ t: 'comment', event: huge })).toThrow(RangeError);
});

````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd packages/protocol && bun test test/comments.test.ts`
Expected: FAIL — нет модуля `../src/comments`: 0 pass, 1 fail (ошибка загрузки)

- [ ] **Step 3: Реализация**

Создать `packages/protocol/src/comments.ts`:

````ts
// Комментарии (спека подсистемы 4, раздел 10): пределы, текст, виды запросов и ответов REST и событие `comment` по
// WebSocket файла. Комментарии живут в Postgres, а не в Loro: не шаг отмены, не часть документа, не экспорт. Разбор не
// доверяет данным: неверное — `null`, лишние поля тоже неверны (событие приходит из сети, запрос — от чужого клиента).

import { isUuid } from './files';
import { isPageDoc } from './presence';

/** Длина текста — в кодовых точках (`commentLength`): одна функция у клиента и сервера, иначе поле ввода и сервер
 * спорили бы о смайликах. */
export const COMMENT_BODY_MAX = 4000;
/** Разных упоминаний в одном комментарии: больше — 400. Каждое — строка базы и, может быть, письмо. */
export const COMMENT_MENTIONS_MAX = 20;
/** Обсуждений в одной странице списка: больше `limit` урезается до этого. */
export const COMMENT_PAGE_MAX = 500;
/** Комментариев в обсуждении: ответ сверх — 409 `THREAD_FULL`. Обсуждение отдаётся целиком одним ответом. */
export const THREAD_COMMENTS_MAX = 500;
/** Подсказка упоминаний: столько людей и строка поиска не длиннее `MENTION_QUERY_MAX`. */
export const MENTION_SUGGEST_MAX = 8;
export const MENTION_QUERY_MAX = 64;
/** Участников обсуждения в `ThreadDto`: для фильтра «Только мои» и писем больше не нужно. */
export const THREAD_PARTICIPANTS_MAX = 50;
/** Событие `comment` на проводе — JSON не больше этого: два текста по 4000 кодовых точек, упоминания и участники с запасом
 * помещаются (`comments.test.ts` проверяет худший случай). */
export const COMMENT_EVENT_MAX_BYTES = 64 * 1024;
/** Координаты метки: как у присутствия, дальше мира ядра не бывает. */
const COORD_MAX = 1e9;
/** Имя человека: `MAX_NAME_CHARS` кодовых точек, по 4 байта, с запасом на будущее. */
const NAME_MAX_CHARS = 256;

const USER_ID_RE = /^[A-Za-z0-9_-]{1,64}$/;
/** Упоминание в тексте: `@[userId]`. Имя не хранится — оно показывается по текущему имени человека. */
const MENTION_RE = /@\[([A-Za-z0-9_-]{1,64})\]/g;
/** Управляющие C0 (кроме перевода строки и табуляции), DEL и C1. */
// eslint-disable-next-line no-control-regex
const CONTROLS_RE = /[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/g;
/** Направление текста (RLO и родня): переставляют показ чужого текста, в письме — тоже. */
const BIDI_RE = /[\u202a-\u202e\u2066-\u2069]/g;

export const isCommentUserId = (v: unknown): v is string => typeof v === 'string' && USER_ID_RE.test(v);

/** Длина в кодовых точках: `[...s].length`, а не `s.length` в UTF-16. */
export function commentLength(s: string): number {
  let n = 0;
  for (const _ of s) n++;
  return n;
}

/** Текст комментария: только текст. Переводы строк — `\n`, управляющие и знаки направления убираются, одиночные
 * суррогаты заменяются, края обрезаются. Пусто или длиннее `COMMENT_BODY_MAX` — `null`. Сервер хранит уже нормальный
 * текст: клиент показывает его как есть, без HTML. */
export function normalizeCommentBody(s: unknown): string | null {
  if (typeof s !== 'string') return null;
  // Предел до разбора: строка из запроса уже не больше JSON, но разбор чужого мегабайта не нужен и клиенту.
  if (s.length > COMMENT_BODY_MAX * 4) return null;
  const body = s.toWellFormed().replace(/\r\n?/g, '\n').replace(CONTROLS_RE, '').replace(BIDI_RE, '').trim();
  return body.length > 0 && commentLength(body) <= COMMENT_BODY_MAX ? body : null;
}

/** Люди, упомянутые в тексте: без повторов, по порядку появления. */
export function mentionIds(body: string): string[] {
  const out = new Set<string>();
  for (const m of body.matchAll(MENTION_RE)) out.add(m[1]);
  return [...out];
}

export type BodyPart = { text: string } | { mention: string; name: string | null };

/** Текст для показа: куски текста и упоминания. `names` — имена упомянутых с доступом (`CommentDto.mentions`); нет имени
 * — `name: null`, интерфейс показывает «@неизвестный»: ни имени, ни адреса человека без доступа. */
export function splitCommentBody(body: string, names: Record<string, string>): BodyPart[] {
  const out: BodyPart[] = [];
  let at = 0;
  for (const m of body.matchAll(MENTION_RE)) {
    if (m.index > at) out.push({ text: body.slice(at, m.index) });
    out.push({ mention: m[1], name: Object.hasOwn(names, m[1]) ? names[m[1]] : null });
    at = m.index + m[0].length;
  }
  if (at < body.length) out.push({ text: body.slice(at) });
  return out;
}

export interface CommentAuthor {
  /** `null` — учётная запись удалена: интерфейс пишет «Удалённый пользователь» **(поведение)**. */
  id: string | null;
  /** Пусто — «Участник», как у присутствия. */
  name: string;
}

export interface CommentDto {
  id: string;
  thread: string;
  author: CommentAuthor;
  body: string;
  createdAt: number;
  editedAt: number | null;
  /** Имена упомянутых, у кого был доступ к файлу при отправке: только им заведено упоминание (и письмо). */
  mentions: Record<string, string>;
}

/** Метка: `page`/`node` — `TreeID` Loro (`counter@peer`); `x`,`y` — в координатах узла или мира страницы (`node: null`);
 * `wx`,`wy` — последнее мировое положение: им метка держится, когда узла нет или он скрыт **(поведение)**. */
export interface PinDto {
  page: string;
  node: string | null;
  x: number;
  y: number;
  wx: number;
  wy: number;
}

export interface ThreadDto {
  id: string;
  pin: PinDto;
  author: CommentAuthor;
  createdAt: number;
  resolvedAt: number | null;
  resolvedBy: CommentAuthor | null;
  /** Номер последнего изменения обсуждения в счётчике файла: события и `?after=` сходятся по нему. */
  seq: number;
  first: CommentDto;
  replies: number;
  lastAt: number;
  /** Кто пишет в обсуждении и кого в нём упомянули, не больше `THREAD_PARTICIPANTS_MAX`. */
  participants: string[];
}

export interface ThreadView extends ThreadDto {
  /** Для того, кто спрашивает: есть чужой комментарий новее его прочтения. */
  unread: boolean;
}

/** `GET …/comments?after=N&limit=M`: обсуждения с `seq > N` по возрастанию `seq`. `after=0` — живые без надгробий;
 * иначе удалённые с тех пор — в `removed`. `more` — есть ещё: следующий запрос — `after=seq`. `seq` — номер, до которого
 * клиент теперь знает всё: при `more` — последнего в странице, иначе — голова файла. */
export interface CommentListDto {
  seq: number;
  threads: ThreadView[];
  removed: string[];
  more: boolean;
}

export interface ThreadDetailDto {
  thread: ThreadView;
  comments: CommentDto[];
}

/** Ответ изменения. `removed` — обсуждение удалено (удалили первый комментарий); `thread` тогда — его последнее состояние. */
export interface CommentWriteDto {
  seq: number;
  thread: ThreadView;
  comment: CommentDto | null;
  removed: boolean;
}

export type CommentAction = 'create' | 'reply' | 'edit' | 'delete' | 'resolve' | 'reopen' | 'move';
const ACTIONS: readonly CommentAction[] = ['create', 'reply', 'edit', 'delete', 'resolve', 'reopen', 'move'];

/** Событие по WebSocket файла. `by` — кто изменил. `deleted` — id удалённого комментария (`action: 'delete'`). `gap` —
 * сервер пропустил события этому соединению (не успевал читать): клиент догоняет `?after=известный`. */
export type CommentEvent =
  | {
      kind: 'thread';
      seq: number;
      by: string;
      action: CommentAction;
      thread: ThreadDto;
      comment: CommentDto | null;
      deleted: string | null;
    }
  | { kind: 'removed'; seq: number; by: string; thread: string }
  | { kind: 'gap'; seq: number };

export interface MentionCandidate {
  id: string;
  name: string;
  /** Только участнику файла (прямая роль или команда): опубликованная ссылка адресов не раздаёт **(поведение)**. */
  email: string | null;
}

export interface MentionListDto {
  people: MentionCandidate[];
}

export interface PrefsDto {
  commentMail: boolean;
}

// --- Разбор ---

type Obj = Record<string, unknown>;

/** Объект ровно с этими полями: лишнее и недостающее — неверно. `optional` — поля, которых может не быть. */
function exact(v: unknown, keys: readonly string[], optional: readonly string[] = []): Obj | null {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) return null;
  const o = v as Obj;
  for (const k of Object.keys(o)) if (!keys.includes(k) && !optional.includes(k)) return null;
  for (const k of keys) if (!Object.hasOwn(o, k)) return null;
  return o;
}

const isCoord = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v) && Math.abs(v) <= COORD_MAX;
const isCount = (v: unknown, max = Number.MAX_SAFE_INTEGER): v is number =>
  Number.isSafeInteger(v) && (v as number) >= 0 && (v as number) <= max;
const isTime = (v: unknown): v is number => isCount(v);
const isTreeId = (v: unknown): v is string => typeof v === 'string' && v.length <= 40 && isPageDoc(v);
const isName = (v: unknown): v is string => typeof v === 'string' && commentLength(v) <= NAME_MAX_CHARS;

export function parsePin(v: unknown): PinDto | null {
  const o = exact(v, ['page', 'node', 'x', 'y', 'wx', 'wy']);
  if (!o || !isTreeId(o.page) || !(o.node === null || isTreeId(o.node))) return null;
  if (!isCoord(o.x) || !isCoord(o.y) || !isCoord(o.wx) || !isCoord(o.wy)) return null;
  return { page: o.page, node: o.node, x: o.x, y: o.y, wx: o.wx, wy: o.wy };
}

/** Текст из запроса: нормальный и не больше `COMMENT_MENTIONS_MAX` разных упоминаний. */
function requestBody(v: unknown): string | null {
  const body = normalizeCommentBody(v);
  return body !== null && mentionIds(body).length <= COMMENT_MENTIONS_MAX ? body : null;
}

/** `POST …/comments`: новое обсуждение. */
export function parseNewThread(v: unknown): { pin: PinDto; body: string } | null {
  const o = exact(v, ['pin', 'body']);
  const pin = parsePin(o?.pin);
  const body = requestBody(o?.body);
  return pin && body !== null ? { pin, body } : null;
}

/** Ответ и правка: `{ body }`. */
export function parseCommentBody(v: unknown): { body: string } | null {
  const body = requestBody(exact(v, ['body'])?.body);
  return body !== null ? { body } : null;
}

/** `PATCH …/comments/:threadId`: ровно одно — закрыть/открыть или перенести метку. У них разные права, и одно событие
 * называет одно действие. */
export function parseThreadPatch(v: unknown): { resolved: boolean } | { pin: PinDto } | null {
  const o = exact(v, [], ['resolved', 'pin']);
  if (!o || Object.keys(o).length !== 1) return null;
  if ('resolved' in o) return typeof o.resolved === 'boolean' ? { resolved: o.resolved } : null;
  const pin = parsePin(o.pin);
  return pin ? { pin } : null;
}

export function parsePrefs(v: unknown): PrefsDto | null {
  const o = exact(v, ['commentMail']);
  return o && typeof o.commentMail === 'boolean' ? { commentMail: o.commentMail } : null;
}

function parseAuthor(v: unknown): CommentAuthor | null {
  const o = exact(v, ['id', 'name']);
  return o && (o.id === null || isCommentUserId(o.id)) && isName(o.name) ? { id: o.id, name: o.name } : null;
}

function parseMentionNames(v: unknown): Record<string, string> | null {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) return null;
  const entries = Object.entries(v as Obj);
  if (entries.length > COMMENT_MENTIONS_MAX) return null;
  for (const [id, name] of entries) if (!USER_ID_RE.test(id) || !isName(name)) return null;
  // `fromEntries` заводит свои поля: id `__proto__` не подменит прототип.
  return Object.fromEntries(entries) as Record<string, string>;
}

export function parseComment(v: unknown): CommentDto | null {
  const o = exact(v, ['id', 'thread', 'author', 'body', 'createdAt', 'editedAt', 'mentions']);
  if (!o || !isUuid(o.id) || !isUuid(o.thread) || !isTime(o.createdAt) || !(o.editedAt === null || isTime(o.editedAt))) return null;
  const author = parseAuthor(o.author);
  const mentions = parseMentionNames(o.mentions);
  // Текст с сервера уже нормальный: другой — подделка или сбой, показывать его нельзя.
  if (!author || !mentions || typeof o.body !== 'string' || normalizeCommentBody(o.body) !== o.body) return null;
  return { id: o.id, thread: o.thread, author, body: o.body, createdAt: o.createdAt, editedAt: o.editedAt, mentions };
}

const THREAD_KEYS = ['id', 'pin', 'author', 'createdAt', 'resolvedAt', 'resolvedBy', 'seq', 'first', 'replies', 'lastAt', 'participants'];

function threadFields(o: Obj): ThreadDto | null {
  if (!isUuid(o.id) || !isTime(o.createdAt) || !(o.resolvedAt === null || isTime(o.resolvedAt)) || !isCount(o.seq)) return null;
  if (!isCount(o.replies, THREAD_COMMENTS_MAX) || !isTime(o.lastAt)) return null;
  const pin = parsePin(o.pin);
  const author = parseAuthor(o.author);
  const resolvedBy = o.resolvedBy === null ? null : parseAuthor(o.resolvedBy);
  const first = parseComment(o.first);
  const p = o.participants;
  if (!pin || !author || (o.resolvedBy !== null && !resolvedBy) || !first || first.thread !== o.id) return null;
  if (!Array.isArray(p) || p.length > THREAD_PARTICIPANTS_MAX || !p.every(isCommentUserId)) return null;
  return {
    id: o.id,
    pin,
    author,
    createdAt: o.createdAt,
    resolvedAt: o.resolvedAt,
    resolvedBy,
    seq: o.seq,
    first,
    replies: o.replies,
    lastAt: o.lastAt,
    participants: [...p],
  };
}

export function parseThread(v: unknown): ThreadDto | null {
  const o = exact(v, THREAD_KEYS);
  return o ? threadFields(o) : null;
}

export function parseThreadView(v: unknown): ThreadView | null {
  const o = exact(v, [...THREAD_KEYS, 'unread']);
  const t = o && typeof o.unread === 'boolean' ? threadFields(o) : null;
  return t ? { ...t, unread: o!.unread as boolean } : null;
}

/** Событие с провода. Каждое поле проверено, лишние поля — неверно: старый клиент не примет полусмысленное. */
export function parseCommentEvent(v: unknown): CommentEvent | null {
  const kind = typeof v === 'object' && v !== null ? (v as Obj).kind : undefined;
  if (kind === 'gap') {
    const o = exact(v, ['kind', 'seq']);
    return o && isCount(o.seq) ? { kind: 'gap', seq: o.seq } : null;
  }
  if (kind === 'removed') {
    const o = exact(v, ['kind', 'seq', 'by', 'thread']);
    return o && isCount(o.seq) && isCommentUserId(o.by) && isUuid(o.thread)
      ? { kind: 'removed', seq: o.seq, by: o.by, thread: o.thread }
      : null;
  }
  if (kind !== 'thread') return null;
  const o = exact(v, ['kind', 'seq', 'by', 'action', 'thread', 'comment', 'deleted']);
  if (!o || !isCount(o.seq) || !isCommentUserId(o.by) || !ACTIONS.includes(o.action as CommentAction)) return null;
  const thread = parseThread(o.thread);
  const comment = o.comment === null ? null : parseComment(o.comment);
  if (!thread || thread.seq !== o.seq || (o.comment !== null && (!comment || comment.thread !== thread.id))) return null;
  if (!(o.deleted === null || isUuid(o.deleted))) return null;
  return { kind: 'thread', seq: o.seq, by: o.by, action: o.action as CommentAction, thread, comment, deleted: o.deleted };
}

const isArrayOf = <T>(v: unknown, of: (x: unknown) => T | null, max: number): T[] | null => {
  if (!Array.isArray(v) || v.length > max) return null;
  const out: T[] = [];
  for (const x of v) {
    const y = of(x);
    if (y === null) return null;
    out.push(y);
  }
  return out;
};

export function parseCommentList(v: unknown): CommentListDto | null {
  const o = exact(v, ['seq', 'threads', 'removed', 'more']);
  if (!o || !isCount(o.seq) || typeof o.more !== 'boolean') return null;
  const threads = isArrayOf(o.threads, parseThreadView, COMMENT_PAGE_MAX);
  const removed = isArrayOf(o.removed, (x) => (isUuid(x) ? x : null), COMMENT_PAGE_MAX);
  return threads && removed ? { seq: o.seq, threads, removed, more: o.more } : null;
}

export function parseThreadDetail(v: unknown): ThreadDetailDto | null {
  const o = exact(v, ['thread', 'comments']);
  const thread = parseThreadView(o?.thread);
  const comments = isArrayOf(o?.comments, parseComment, THREAD_COMMENTS_MAX);
  return thread && comments && comments.every((c) => c.thread === thread.id) ? { thread, comments } : null;
}

export function parseCommentWrite(v: unknown): CommentWriteDto | null {
  const o = exact(v, ['seq', 'thread', 'comment', 'removed']);
  if (!o || !isCount(o.seq) || typeof o.removed !== 'boolean') return null;
  const thread = parseThreadView(o.thread);
  const comment = o.comment === null ? null : parseComment(o.comment);
  return thread && (o.comment === null || comment) ? { seq: o.seq, thread, comment, removed: o.removed } : null;
}

export function parseMentionList(v: unknown): MentionListDto | null {
  const people = isArrayOf(
    exact(v, ['people'])?.people,
    (x) => {
      const o = exact(x, ['id', 'name', 'email']);
      return o && isCommentUserId(o.id) && isName(o.name) && (o.email === null || typeof o.email === 'string')
        ? { id: o.id, name: o.name, email: o.email }
        : null;
    },
    MENTION_SUGGEST_MAX,
  );
  return people ? { people } : null;
}

/** Строка поиска упоминаний: не длиннее `MENTION_QUERY_MAX`, без управляющих; пусто — все. */
export function parseMentionQuery(v: string | null): string | null {
  const q = (v ?? '').trim();
  // eslint-disable-next-line no-control-regex
  return commentLength(q) <= MENTION_QUERY_MAX && !/[\u0000-\u001f\u007f-\u009f]/.test(q) ? q : null;
}

// --- Кадр ---

const utf8 = new TextEncoder();
const fromUtf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/** Тело кадра `comment`: JSON события. Больше `COMMENT_EVENT_MAX_BYTES` — ошибка отправителя, а не кадр на проводе. */
export function encodeCommentEvent(ev: CommentEvent): Uint8Array {
  const b = utf8.encode(JSON.stringify(ev));
  if (b.length > COMMENT_EVENT_MAX_BYTES) throw new RangeError(`событие комментария: ${b.length} байт`);
  return b;
}

export function decodeCommentEvent(b: Uint8Array): CommentEvent | null {
  if (b.length === 0 || b.length > COMMENT_EVENT_MAX_BYTES) return null;
  try {
    return parseCommentEvent(JSON.parse(fromUtf8.decode(b)));
  } catch {
    return null;
  }
}
````

В `packages/protocol/src/files.ts` заменить:

````ts
   * был бы неполной копией; правки целы на сервере. */
  'INCOMPLETE',
] as const;
export type ApiErrorCode = (typeof API_ERROR_CODES)[number];
````

на:

````ts
   * был бы неполной копией; правки целы на сервере. */
  'INCOMPLETE',
  /** В обсуждении уже `THREAD_COMMENTS_MAX` комментариев: ответ не принят. */
  'THREAD_FULL',
] as const;
export type ApiErrorCode = (typeof API_ERROR_CODES)[number];
````

В `packages/protocol/src/index.ts` заменить:

````ts
export * from './sync';
export * from './presence';
````

на:

````ts
export * from './sync';
export * from './presence';
export * from './comments';
````

В `packages/protocol/src/sync.ts` заменить:

````ts
// Первый байт — вид кадра, числа — LE, строка — длина u8 и UTF-8. Документ — `root` или `TreeID` страницы.

import type { FileRole } from './files';
import {
````

на:

````ts
// Первый байт — вид кадра, числа — LE, строка — длина u8 и UTF-8. Документ — `root` или `TreeID` страницы.

import { decodeCommentEvent, encodeCommentEvent, type CommentEvent } from './comments';
import type { FileRole } from './files';
import {
````

В `packages/protocol/src/sync.ts` заменить:

````ts

/** Растёт при несовместимой смене кадров: старый клиент получает `please-reload`. 2 — присутствие (этап 4): кадров
 * `presence` прежний клиент не знает и счёл бы соединение испорченным. */
export const SYNC_PROTOCOL = 2;
/** Предел кадра по спеке, раздел 7.3. Вся полезная нагрузка — байты Loro, заголовок сверху не считается. */
export const MAX_SYNC_UPDATE_BYTES = 16 * 1024 * 1024;
````

на:

````ts

/** Растёт при несовместимой смене кадров: старый клиент получает `please-reload`. 2 — присутствие (этап 4): кадров
 * `presence` прежний клиент не знает и счёл бы соединение испорченным. 3 — комментарии (этап 5): кадр `comment`. */
export const SYNC_PROTOCOL = 3;
/** Предел кадра по спеке, раздел 7.3. Вся полезная нагрузка — байты Loro, заголовок сверху не считается. */
export const MAX_SYNC_UPDATE_BYTES = 16 * 1024 * 1024;
````

В `packages/protocol/src/sync.ts` заменить:

````ts
  | { t: 'pong' }
  /** Пачка присутствия других участников файла (`encodePresenceBatch`). */
  | { t: 'presence'; items: PresenceItem[] };

const KIND = {
````

на:

````ts
  | { t: 'pong' }
  /** Пачка присутствия других участников файла (`encodePresenceBatch`). */
  | { t: 'presence'; items: PresenceItem[] }
  /** Событие комментариев файла (спека, раздел 10.4): JSON не больше `COMMENT_EVENT_MAX_BYTES`, разбор строгий. */
  | { t: 'comment'; event: CommentEvent };

const KIND = {
````

В `packages/protocol/src/sync.ts` заменить:

````ts
  pong: 25,
  'presence-batch': 26,
} as const;

````

на:

````ts
  pong: 25,
  'presence-batch': 26,
  comment: 27,
} as const;

````

В `packages/protocol/src/sync.ts` заменить:

````ts
    case 'presence':
      throw new RangeError('пачку присутствия собирает encodePresenceBatch');
  }
}
````

на:

````ts
    case 'presence':
      throw new RangeError('пачку присутствия собирает encodePresenceBatch');
    case 'comment':
      return w.bytes(encodeCommentEvent(f.event)).done();
  }
}
````

В `packages/protocol/src/sync.ts` заменить:

````ts
      return items ? { t: 'presence', items } : null;
    }
    default:
      return null;
````

на:

````ts
      return items ? { t: 'presence', items } : null;
    }
    case KIND.comment: {
      const event = decodeCommentEvent(r.rest());
      return event ? { t: 'comment', event } : null;
    }
    default:
      return null;
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/sync-client.test.ts`

Expected: PASS: всё без замечаний; `test:server` — протокол 56 pass, сервер 516 pass; `sync-client.test.ts` 56 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/test/hub.test.ts apps/server/test/presence.test.ts packages/protocol/src/comments.ts packages/protocol/src/files.ts packages/protocol/src/index.ts packages/protocol/src/sync.ts packages/protocol/test/comments.test.ts packages/protocol/test/sync.test.ts
git commit -F - <<'EOF'
протокол: комментарии — виды REST и события, кадр comment (27), SYNC_PROTOCOL = 3

packages/protocol/src/comments.ts: пределы (4000 кодовых точек, 20 упоминаний, страница и обсуждение по 500,
подсказка 8), normalizeCommentBody и commentLength — общие для клиента и сервера, mentionIds/splitCommentBody,
DTO и строгий разбор запросов, ответов и события; кадр сервера comment — JSON не больше 64 КБ. Протокол 3:
вкладка прошлой сборки получает please-reload. Код ошибки THREAD_FULL.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4: Ядро worker'а — команды и события комментариев

Спека §10.2. Ввод хозяина проверяет ядро: координаты конечны и не больше 1e9, id обсуждения 1…64 байта `[A-Za-z0-9_-]`, `page`/`node` — `counter@peer` не черновика, цвет ≤ 0xFFFFFF, инициалы — первые 3 графемы, всего не больше 10 000 меток; неверная метка пропускается одна. Команды комментариев не берут удержание чужого превью.

**Files:**
- Create: `crates/ferrum-wasm/src/core/comments.rs`
- Modify: `Cargo.lock`, `Cargo.toml`, `crates/ferrum-wasm/Cargo.toml`, `crates/ferrum-wasm/src/core.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `crates/ferrum-wasm/src/core/presence.rs`, `crates/ferrum-wasm/src/protocol.rs`
- Test: `crates/ferrum-wasm/tests/comments.rs` (новый), `crates/ferrum-wasm/tests/protocol.rs`

**Interfaces:**
- Consumes: `Editor::*comment*` и события задачи 2.
- Produces: `Command::{CommentsAvailable { on }, CommentPins { pins: Vec<CommentPinDto>, replace }, CommentPinsRemove { ids }, CommentOpen { id: Option<String> }, CommentDraft { draft: Option<CommentDraftDto> }, CommentFocus { id }, CommentShowResolved { on }}`; `CommentPinDto { id, page, node: Option<String>, x, y, wx, wy, initials, color: u32, replies: u32, unread, resolved, movable }`, `CommentDraftDto { page, node, x, y, wx, wy }`; `CoreEvent::{CommentPlace { page, node, x, y, wx, wy, sx, sy }, CommentOpened { id, sx, sy }, CommentMoved { id, page, node, x, y, wx, wy }, CommentAnchor { sx, sy, visible }, CommentsState { mode, visible, available }}`; `COMMENT_PINS_MAX`, `COMMENT_ID_MAX_BYTES = 64`, `COMMENT_INITIALS_MAX = 3`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-wasm/tests/comments.rs`:

````rust
//! Комментарии в ядре worker'а (спека подсистемы 4, раздел 10): метки от интерфейса проверяются
//! ядром, события редактора уходят событиями протокола, без `CommentsAvailable` (отдельная сборка)
//! режима нет, команды комментариев не берут удержание чужого превью, метки не попадают ни в
//! документ и журнал, ни в превью файла.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, KeyEvent, Modifiers, Platform, PointerEvent};
use ferrum_render::CpuBackend;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_json::json;

/// Ядро; камера при открытии — центр в начале мира: экран (x, y) — мир (x − 400, y − 300).
fn core() -> Core {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.take_events();
    c
}

fn page(c: &Core) -> String {
    c.editor().page().to_string()
}

fn at(x: f64, y: f64, t: f64) -> PointerEvent {
    PointerEvent::new((x, y), Modifiers::NONE, t)
}

fn click(c: &mut Core, x: f64, y: f64, t: f64) {
    c.handle(Input::PointerDown(at(x, y, t)));
    c.handle(Input::PointerUp(at(x, y, t + 50.0)));
}

/// Прямоугольник инструментом от экрана (100, 100) до (200, 200); его идентификатор.
fn rect(c: &mut Core) -> String {
    c.command(Command::SetTool { tool: Tool::Rect });
    c.handle(Input::PointerDown(at(100.0, 100.0, 1000.0)));
    c.handle(Input::PointerMove(at(200.0, 200.0, 1100.0)));
    c.handle(Input::PointerUp(at(200.0, 200.0, 1200.0)));
    c.take_events();
    c.editor().selection()[0].to_string()
}

/// Метка в мире `(x, y)`.
fn pin(c: &Core, id: &str, x: f64, y: f64) -> CommentPinDto {
    CommentPinDto {
        id: id.into(),
        page: page(c),
        node: None,
        x,
        y,
        wx: x,
        wy: y,
        initials: "АБ".into(),
        color: 0x0d74ce,
        replies: 2,
        unread: true,
        resolved: false,
        movable: true,
    }
}

fn available(c: &mut Core) {
    c.command(Command::CommentsAvailable { on: true });
}

#[test]
fn standalone_build_has_no_comment_mode() {
    let mut c = core();
    c.command(Command::Execute {
        command: "tool.comment".into(),
    });
    c.command(Command::SetTool {
        tool: Tool::Comment,
    });
    c.handle(Input::KeyDown(KeyEvent::new("KeyC", Modifiers::NONE)));
    c.handle(Input::KeyDown(KeyEvent::new("KeyC", Modifiers::SHIFT)));
    let ev = c.take_events();
    assert!(
        !ev.iter().any(|e| matches!(e, CoreEvent::Tool { .. })),
        "{ev:?}"
    );
    assert_eq!(c.editor().tool_kind(), ferrum_editor::ToolKind::Move);
    assert!(c.editor().comments_visible());
    // Состояние команд: обе серые.
    let states = |c: &Core| {
        let s = c
            .editor()
            .command_state(ferrum_editor::CommandId::ToolComment);
        let v = c
            .editor()
            .command_state(ferrum_editor::CommandId::ViewComments);
        (s.enabled, v.enabled)
    };
    assert_eq!(states(&c), (false, false));
    available(&mut c);
    assert_eq!(states(&c), (true, true));
    let ev = c.take_events();
    assert!(ev.contains(&CoreEvent::CommentsState {
        mode: false,
        visible: true,
        available: true
    }));
    assert!(ev.iter().any(|e| matches!(
        e,
        CoreEvent::CommandStates { changed } if changed.iter().any(|s| s.key == "tool.comment" && s.enabled)
    )));
    c.handle(Input::KeyDown(KeyEvent::new("KeyC", Modifiers::NONE)));
    let ev = c.take_events();
    assert!(ev.contains(&CoreEvent::Tool {
        tool: Tool::Comment
    }));
    assert!(ev.contains(&CoreEvent::CommentsState {
        mode: true,
        visible: true,
        available: true
    }));
}

#[test]
fn host_input_is_validated_and_bad_pins_are_skipped_alone() {
    let mut c = core();
    available(&mut c);
    let good = pin(&c, "11111111-2222-3333-4444-555555555555", 10.0, 10.0);
    let mut bad = Vec::new();
    for f in [
        |p: &mut CommentPinDto| p.x = f64::NAN,
        |p: &mut CommentPinDto| p.wy = f64::INFINITY,
        |p: &mut CommentPinDto| p.y = 2e9,
        |p: &mut CommentPinDto| p.id = "x".repeat(65),
        |p: &mut CommentPinDto| p.id = "../a".into(),
        |p: &mut CommentPinDto| p.id = String::new(),
        |p: &mut CommentPinDto| p.page = "root".into(),
        |p: &mut CommentPinDto| p.page = "страница".into(),
        |p: &mut CommentPinDto| p.node = Some(format!("1@{}", u64::MAX)),
        |p: &mut CommentPinDto| p.node = Some("1@".into()),
        |p: &mut CommentPinDto| p.color = 0x1000000,
        |p: &mut CommentPinDto| p.initials = "e\u{301}".repeat(20),
    ] {
        let mut p = pin(&c, "bad", 0.0, 0.0);
        f(&mut p);
        bad.push(p);
    }
    let mut long = pin(&c, "long", 0.0, 0.0);
    // Четыре графемы (вторая — с диакритикой): остаются три.
    long.initials = "Ae\u{301}ВГ".into();
    let mut pins = bad;
    pins.push(good);
    pins.push(long);
    c.command(Command::CommentPins {
        pins,
        replace: true,
    });
    let ed = c.editor();
    assert_eq!(ed.comment_pin_count(), 2);
    assert!(ed.comment_pin("bad").is_none());
    assert_eq!(&*ed.comment_pin("long").unwrap().initials, "Ae\u{301}В");
    // Ни одной ошибки: неверное — тихо пропущено, как записи присутствия.
    assert!(!c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { .. })));
    // Сверх 10 000 меток лишние не принимаются.
    let many: Vec<_> = (0..COMMENT_PINS_MAX + 7)
        .map(|i| pin(&c, &format!("t{i}"), 0.0, 0.0))
        .collect();
    c.command(Command::CommentPins {
        pins: many,
        replace: true,
    });
    assert_eq!(c.editor().comment_pin_count(), COMMENT_PINS_MAX);
    // Неверные идентификаторы в остальных командах — ничего не делают.
    c.command(Command::CommentOpen {
        id: Some("t1".into()),
    });
    c.command(Command::CommentOpen {
        id: Some("a/b".into()),
    });
    assert_eq!(c.editor().comment_open(), Some("t1"));
    c.command(Command::CommentPinsRemove {
        ids: vec!["t1".into(), "y".repeat(100)],
    });
    assert_eq!(c.editor().comment_pin_count(), COMMENT_PINS_MAX - 1);
    c.command(Command::CommentDraft {
        draft: Some(CommentDraftDto {
            page: page(&c),
            node: None,
            x: f64::NAN,
            y: 0.0,
            wx: 0.0,
            wy: 0.0,
        }),
    });
    assert!(c
        .editor()
        .overlay()
        .comments
        .iter()
        .all(|m| !matches!(m.kind, ferrum_render::PinKind::Draft)));
}

#[test]
fn editor_events_become_protocol_events() {
    let mut c = core();
    available(&mut c);
    let r = rect(&mut c);
    c.command(Command::SetTool {
        tool: Tool::Comment,
    });
    c.take_events();
    // Щелчок по прямоугольнику (мир −300…−200) — место нового, привязанное к нему.
    click(&mut c, 150.0, 120.0, 5000.0);
    let place = c
        .take_events()
        .into_iter()
        .find(|e| matches!(e, CoreEvent::CommentPlace { .. }))
        .expect("CommentPlace");
    assert_eq!(
        place,
        CoreEvent::CommentPlace {
            page: page(&c),
            node: Some(r.clone()),
            x: 50.0,
            y: 20.0,
            wx: -250.0,
            wy: -180.0,
            sx: 150.0,
            sy: 120.0
        }
    );
    // Черновик — метка над точкой, пока пишут.
    c.command(Command::CommentDraft {
        draft: Some(CommentDraftDto {
            page: page(&c),
            node: Some(r.clone()),
            x: 50.0,
            y: 20.0,
            wx: -250.0,
            wy: -180.0,
        }),
    });
    assert!(c.take_events().contains(&CoreEvent::CommentAnchor {
        sx: 150.0,
        sy: 120.0,
        visible: true
    }));
    c.command(Command::CommentDraft { draft: None });
    // Своя метка в мире (0, 0) — экран (400, 300): щелчок открывает, протяжка двигает.
    let p = pin(&c, "mine", 0.0, 0.0);
    c.command(Command::CommentPins {
        pins: vec![p],
        replace: true,
    });
    c.take_events();
    click(&mut c, 410.0, 290.0, 6000.0);
    let ev = c.take_events();
    assert!(ev.contains(&CoreEvent::CommentOpened {
        id: "mine".into(),
        sx: 400.0,
        sy: 300.0
    }));
    c.handle(Input::PointerDown(at(410.0, 290.0, 7000.0)));
    c.handle(Input::PointerMove(at(450.0, 290.0, 7050.0)));
    c.handle(Input::PointerMove(at(510.0, 340.0, 7100.0)));
    c.handle(Input::PointerUp(at(510.0, 340.0, 7150.0)));
    let ev = c.take_events();
    assert!(
        ev.contains(&CoreEvent::CommentMoved {
            id: "mine".into(),
            page: page(&c),
            node: None,
            x: 100.0,
            y: 50.0,
            wx: 100.0,
            wy: 50.0
        }),
        "{ev:?}"
    );
    // Точка открытой метки за пачку — только последняя.
    let anchors = ev
        .iter()
        .filter(|e| matches!(e, CoreEvent::CommentAnchor { .. }))
        .count();
    assert!(anchors <= 1, "{ev:?}");
    assert!(ev.contains(&CoreEvent::CommentAnchor {
        sx: 500.0,
        sy: 350.0,
        visible: true
    }));
}

#[test]
fn comment_commands_do_not_take_the_presence_hold() {
    let mut c = core();
    available(&mut c);
    let r = rect(&mut c);
    let p = page(&c);
    // Участник тянет прямоугольник: слой чужого превью в сцене.
    c.command(Command::Presence {
        items: vec![
            PresenceItemDto::Join {
                participant: 1,
                user: "u-1".into(),
                name: "Анна".into(),
                color: 0x0d74ce,
            },
            PresenceItemDto::Part {
                participant: 1,
                rev: 1,
                index: 0,
                parts: 1,
                ids: vec![r.clone()],
            },
            PresenceItemDto::State {
                participant: 1,
                page: p.clone(),
                view: [0.0, 0.0, 800.0, 600.0],
                cursor: None,
                selection: Some(PresenceSelectionDto {
                    rev: 1,
                    count: 1,
                    parts: 1,
                    bounds: [0.0, 0.0, 1.0, 1.0],
                }),
                gesture: Some(PresenceGestureDto {
                    kind: GestureKindDto::Move,
                    transform: [1.0, 0.0, 0.0, 1.0, 40.0, 0.0],
                    frame: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    k: 1.0,
                }),
            },
        ],
    });
    assert!(c.editor().scene().has_remote_preview());
    let x = pin(&c, "a", 0.0, 0.0);
    for cmd in [
        Command::CommentPins {
            pins: vec![x],
            replace: true,
        },
        Command::CommentOpen {
            id: Some("a".into()),
        },
        Command::CommentDraft { draft: None },
        Command::CommentShowResolved { on: true },
        Command::CommentFocus { id: "a".into() },
        Command::CommentPinsRemove {
            ids: vec!["a".into()],
        },
        Command::CommentsAvailable { on: true },
    ] {
        let before = c.editor().scene().change_counter();
        c.command(cmd.clone());
        assert_eq!(
            c.editor().scene().change_counter(),
            before,
            "слой не снимался и не накладывался: {cmd:?}"
        );
    }
    // Для сравнения: команда с удержанием слой снимает и накладывает заново.
    let before = c.editor().scene().change_counter();
    c.command(Command::Execute {
        command: "view.zoom_in".into(),
    });
    assert_ne!(c.editor().scene().change_counter(), before);
}

#[test]
fn pins_stay_out_of_document_journal_and_file_thumbnail() {
    let mut c = core();
    c.command(Command::Demo { nodes: 300 });
    c.command(Command::FlushJournal);
    c.take_events();
    let thumb = |c: &mut Core| {
        c.command(Command::Thumbnail);
        c.take_events()
            .into_iter()
            .find_map(|e| match e {
                CoreEvent::Thumbnail { bytes, .. } => bytes,
                _ => None,
            })
            .expect("превью")
    };
    let plain = thumb(&mut c);
    let (w, h) = c.device_size();
    let mut b = CpuBackend::new(w, h);
    c.draw(&mut b);
    let without = b.render();
    available(&mut c);
    // Метки по всему видимому и по всей странице.
    let center = c.editor().camera().center();
    let mut pins = vec![pin(&c, "center", center.x, center.y)];
    pins.extend((0..50).map(|i| pin(&c, &format!("p{i}"), i as f64 * 300.0, i as f64 * 200.0)));
    c.command(Command::CommentPins {
        pins,
        replace: true,
    });
    c.command(Command::SetTool {
        tool: Tool::Comment,
    });
    click(&mut c, 30.0, 30.0, 9000.0);
    let ev = c.take_events();
    assert!(
        !ev.iter().any(|e| matches!(
            e,
            CoreEvent::Document { .. } | CoreEvent::Modified { modified: true }
        )),
        "{ev:?}"
    );
    // На экране метки есть, в превью файла — нет.
    let mut b = CpuBackend::new(w, h);
    c.draw(&mut b);
    let with = b.render();
    assert_ne!(with.data_as_u8_slice(), without.data_as_u8_slice());
    assert_eq!(thumb(&mut c), plain);
    // В журнал — ничего.
    c.command(Command::FlushJournal);
    let journal = c.take_events().into_iter().find_map(|e| match e {
        CoreEvent::Journal { bytes, .. } => Some(bytes),
        _ => None,
    });
    assert_eq!(journal, Some(None));
}

#[test]
fn comment_messages_have_typescript_field_names() {
    let pin = json!({
        "id": "t1", "page": "0@1", "x": 1.0, "y": 2.0, "wx": 3.0, "wy": 4.0,
        "initials": "АБ", "color": 255, "replies": 0, "unread": false, "resolved": false,
        "movable": true
    });
    let cmd: Command =
        serde_json::from_value(json!({"type": "commentPins", "pins": [pin], "replace": true}))
            .unwrap();
    assert!(
        matches!(cmd, Command::CommentPins { ref pins, replace: true } if pins[0].node.is_none())
    );
    for good in [
        json!({"type": "commentsAvailable", "on": true}),
        json!({"type": "commentPinsRemove", "ids": ["a"]}),
        json!({"type": "commentOpen"}),
        json!({"type": "commentOpen", "id": "a"}),
        json!({"type": "commentDraft"}),
        json!({"type": "commentDraft", "draft": {"page": "0@1", "node": "2@1", "x": 0, "y": 0, "wx": 0, "wy": 0}}),
        json!({"type": "commentFocus", "id": "a"}),
        json!({"type": "commentShowResolved", "on": false}),
        json!({"type": "setTool", "tool": "comment"}),
    ] {
        assert!(
            serde_json::from_value::<Command>(good.clone()).is_ok(),
            "{good}"
        );
    }
    // Лишние поля и неверные типы — ошибка разбора.
    for bad in [
        json!({"type": "commentsAvailable", "on": true, "x": 1}),
        json!({"type": "commentFocus"}),
        json!({"type": "commentPins", "pins": [{"id": "a"}], "replace": true}),
        json!({"type": "commentDraft", "draft": {"page": "0@1", "x": 0, "y": 0, "wx": 0, "wy": 0, "z": 1}}),
    ] {
        assert!(
            serde_json::from_value::<Command>(bad.clone()).is_err(),
            "{bad}"
        );
    }
    assert_eq!(
        serde_json::to_value(CoreEvent::CommentPlace {
            page: "0@1".into(),
            node: None,
            x: 1.0,
            y: 2.0,
            wx: 3.0,
            wy: 4.0,
            sx: 5.0,
            sy: 6.0
        })
        .unwrap(),
        json!({"type": "commentPlace", "page": "0@1", "node": null, "x": 1.0, "y": 2.0, "wx": 3.0, "wy": 4.0, "sx": 5.0, "sy": 6.0})
    );
    assert_eq!(
        serde_json::to_value(CoreEvent::CommentsState {
            mode: true,
            visible: false,
            available: true
        })
        .unwrap(),
        json!({"type": "commentsState", "mode": true, "visible": false, "available": true})
    );
    assert_eq!(
        serde_json::to_value(CoreEvent::CommentAnchor {
            sx: 1.0,
            sy: 2.0,
            visible: true
        })
        .unwrap(),
        json!({"type": "commentAnchor", "sx": 1.0, "sy": 2.0, "visible": true})
    );
    assert_eq!(
        serde_json::to_value(CoreEvent::CommentOpened {
            id: "a".into(),
            sx: 1.0,
            sy: 2.0
        })
        .unwrap()["type"],
        json!("commentOpened")
    );
    assert_eq!(
        serde_json::to_value(CoreEvent::CommentMoved {
            id: "a".into(),
            page: "0@1".into(),
            node: Some("2@1".into()),
            x: 1.0,
            y: 2.0,
            wx: 3.0,
            wy: 4.0
        })
        .unwrap()["node"],
        json!("2@1")
    );
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        },
        Command::Thumbnail,
    ]
}
````

на:

````rust
        },
        Command::Thumbnail,
        Command::CommentsAvailable { on: true },
        Command::CommentPins {
            pins: vec![CommentPinDto {
                id: "t1".into(),
                page: "0@1".into(),
                node: Some("2@1".into()),
                x: 1.0,
                y: 2.0,
                wx: 3.0,
                wy: 4.0,
                initials: "АБ".into(),
                color: 0x0d74ce,
                replies: 3,
                unread: true,
                resolved: false,
                movable: true,
            }],
            replace: false,
        },
        Command::CommentPinsRemove {
            ids: vec!["t1".into()],
        },
        Command::CommentOpen { id: None },
        Command::CommentDraft {
            draft: Some(CommentDraftDto {
                page: "0@1".into(),
                node: None,
                x: 1.0,
                y: 2.0,
                wx: 1.0,
                wy: 2.0,
            }),
        },
        Command::CommentFocus { id: "t1".into() },
        Command::CommentShowResolved { on: true },
        Command::SetTool {
            tool: Tool::Comment,
        },
    ]
}
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
            bytes: Some(bytes(&[1])),
            keep: false,
        },
    ]
````

на:

````rust
            bytes: Some(bytes(&[1])),
            keep: false,
        },
        CoreEvent::CommentPlace {
            page: "0@1".into(),
            node: Some("2@1".into()),
            x: 1.0,
            y: 2.0,
            wx: 3.0,
            wy: 4.0,
            sx: 5.0,
            sy: 6.0,
        },
        CoreEvent::CommentOpened {
            id: "t1".into(),
            sx: 1.0,
            sy: 2.0,
        },
        CoreEvent::CommentMoved {
            id: "t1".into(),
            page: "0@1".into(),
            node: None,
            x: 1.0,
            y: 2.0,
            wx: 1.0,
            wy: 2.0,
        },
        CoreEvent::CommentAnchor {
            sx: 1.0,
            sy: 2.0,
            visible: false,
        },
        CoreEvent::CommentsState {
            mode: true,
            visible: true,
            available: true,
        },
    ]
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test comments`
Expected: FAIL — не собирается: нет `CommentPinDto`, команд `Command::Comment*` и событий `CoreEvent::Comment*`, 52 ошибки компиляции

- [ ] **Step 3: Реализация**

В `Cargo.lock` заменить:

````
 "serde_json",
 "tsify",
 "vello_common",
 "vello_gpu",
````

на:

````
 "serde_json",
 "tsify",
 "unicode-segmentation",
 "vello_common",
 "vello_gpu",
````

В `Cargo.toml` заменить:

````toml
tsify = { version = "=0.5.8", default-features = false, features = ["js"] }
serde-wasm-bindgen = "=0.6.5"
serde_bytes = "=0.11.19"
console_error_panic_hook = "=0.1.7"
````

на:

````toml
tsify = { version = "=0.5.8", default-features = false, features = ["js"] }
serde-wasm-bindgen = "=0.6.5"
# Инициалы автора метки комментария — до трёх графем. Та же версия, что уже в Cargo.lock (её
# тянет convert_case).
unicode-segmentation = "=1.13.3"
serde_bytes = "=0.11.19"
console_error_panic_hook = "=0.1.7"
````

В `crates/ferrum-wasm/Cargo.toml` заменить:

````toml
serde_bytes.workspace = true
tsify.workspace = true
wasm-bindgen.workspace = true

````

на:

````toml
serde_bytes.workspace = true
tsify.workspace = true
unicode-segmentation.workspace = true
wasm-bindgen.workspace = true

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
//! декодируются).

mod fonts;
mod panels;
````

на:

````rust
//! декодируются).

mod comments;
mod fonts;
mod panels;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            | Command::Presence { .. }
            | Command::PresenceReset
            | Command::Demo { .. }
            | Command::DemoText { .. }
````

на:

````rust
            | Command::Presence { .. }
            | Command::PresenceReset
            // Метки и состояние комментариев документ не правят; переход к обсуждению
            // (`CommentFocus`) меняет страницу — он ждёт, как другие команды.
            | Command::CommentsAvailable { .. }
            | Command::CommentPins { .. }
            | Command::CommentPinsRemove { .. }
            | Command::CommentOpen { .. }
            | Command::CommentDraft { .. }
            | Command::CommentShowResolved { .. }
            | Command::Demo { .. }
            | Command::DemoText { .. }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                self.sync_note();
            }
            // Правка текста; ждущая шрифтов — в очередь.
            cmd if text::is_text_command(&cmd) => self.text_command(cmd),
````

на:

````rust
                self.sync_note();
            }
            cmd if comments::is_comment_command(&cmd) => self.comment_command(cmd),
            // Правка текста; ждущая шрифтов — в очередь.
            cmd if text::is_text_command(&cmd) => self.text_command(cmd),
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                    self.events.push(CoreEvent::Following { participant })
                }
                // Комментарии: события протокола — следующая задача; без `CommentsAvailable`
                // редактор их и не шлёт, кроме состояния.
                EditorEvent::CommentPlace { .. }
                | EditorEvent::CommentOpened { .. }
                | EditorEvent::CommentMoved { .. }
                | EditorEvent::CommentAnchor { .. }
                | EditorEvent::CommentsState { .. } => {}
            }
        }
````

на:

````rust
                    self.events.push(CoreEvent::Following { participant })
                }
                // Точка открытой метки меняется с каждым кадром камеры: в пачке важна последняя.
                e @ EditorEvent::CommentAnchor { .. } => {
                    self.events
                        .retain(|e| !matches!(e, CoreEvent::CommentAnchor { .. }));
                    self.events.extend(comments::comment_event(&e));
                }
                e @ (EditorEvent::CommentPlace { .. }
                | EditorEvent::CommentOpened { .. }
                | EditorEvent::CommentMoved { .. }
                | EditorEvent::CommentsState { .. }) => {
                    self.events.extend(comments::comment_event(&e))
                }
            }
        }
````

Создать `crates/ferrum-wasm/src/core/comments.rs`:

````rust
//! Комментарии в ядре (спека подсистемы 4, раздел 10): метки и состояние от интерфейса —
//! редактору, события редактора — интерфейсу. Сами обсуждения живут на сервере; метки приходят
//! из `comment-event` и списка через основной поток и не доверены: ядро проверяет всё само —
//! числа конечны и в пределах мира, идентификаторы обсуждений не длиннее
//! `COMMENT_ID_MAX_BYTES` и из `[A-Za-z0-9_-]`, страница и узел — `counter@peer` (не черновик),
//! инициалы — до `COMMENT_INITIALS_MAX` графем, меток — не больше `COMMENT_PINS_MAX` (лишние не
//! принимаются). Неверная метка пропускается целиком, остальные команды с неверным полем ничего
//! не делают: интерфейс — свой, неверное значение — ошибка или подделка, а не ввод человека.
//!
//! Команды комментариев сцену не правят и чужое живое превью не снимают (`presence::holds`).

use std::str::FromStr;
use std::sync::Arc;

use ferrum_doc::{NodeId, PageId};
use ferrum_editor::{peer_color, CommentPin, EditorEvent, PinAnchor};
use kurbo::Point;
use unicode_segmentation::UnicodeSegmentation;

use super::Core;
use crate::protocol::{
    Command, CommentDraftDto, CommentPinDto, CoreEvent, COMMENT_ID_MAX_BYTES, COMMENT_INITIALS_MAX,
    COMMENT_PINS_MAX,
};

const WORLD_MAX: f64 = 1e9;
/// Инициалы длиннее стольких байт не принимаются: одна «графема» из тысячи диакритик иначе
/// прошла бы предел графем.
const INITIALS_MAX_BYTES: usize = 32;

fn coord(v: f64) -> bool {
    v.is_finite() && v.abs() <= WORLD_MAX
}

/// Идентификатор обсуждения: UUID сервера или такой же короткий из `[A-Za-z0-9_-]`.
fn thread_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= COMMENT_ID_MAX_BYTES
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Узел или страница `counter@peer`: не длиннее 64 байт, и не черновик (пир `u64::MAX`).
fn tree_id(s: &str) -> bool {
    s.len() <= COMMENT_ID_MAX_BYTES
        && s.split_once('@')
            .is_some_and(|(_, peer)| peer.parse::<u64>().is_ok_and(|p| p != u64::MAX))
}

fn page(s: &str) -> Option<PageId> {
    tree_id(s).then(|| PageId::from_str(s).ok()).flatten()
}

fn node(s: &Option<String>) -> Option<Option<NodeId>> {
    match s {
        None => Some(None),
        Some(s) => tree_id(s)
            .then(|| NodeId::from_str(s).ok().map(Some))
            .flatten(),
    }
}

fn anchor(page_s: &str, node_s: &Option<String>, xy: [f64; 4]) -> Option<PinAnchor> {
    if !xy.iter().all(|v| coord(*v)) {
        return None;
    }
    Some(PinAnchor {
        page: page(page_s)?,
        node: node(node_s)?,
        local: Point::new(xy[0], xy[1]),
        world: Point::new(xy[2], xy[3]),
    })
}

/// Инициалы: не длиннее `INITIALS_MAX_BYTES`, первые `COMMENT_INITIALS_MAX` графем
/// **(поведение)**: длинное имя не прячет обсуждение, а укорачивается.
fn initials(s: &str) -> Option<Arc<str>> {
    if s.len() > INITIALS_MAX_BYTES {
        return None;
    }
    let short: String = s.graphemes(true).take(COMMENT_INITIALS_MAX).collect();
    Some(Arc::from(short))
}

/// Метка, если всё в ней верно.
pub fn comment_pin(p: &CommentPinDto) -> Option<CommentPin> {
    if !thread_id(&p.id) || p.color > 0xff_ffff {
        return None;
    }
    Some(CommentPin {
        id: Arc::from(p.id.as_str()),
        anchor: anchor(&p.page, &p.node, [p.x, p.y, p.wx, p.wy])?,
        initials: initials(&p.initials)?,
        color: peer_color(p.color),
        replies: p.replies,
        unread: p.unread,
        resolved: p.resolved,
        movable: p.movable,
    })
}

fn draft(d: &CommentDraftDto) -> Option<PinAnchor> {
    anchor(&d.page, &d.node, [d.x, d.y, d.wx, d.wy])
}

/// Команда комментариев: `true` — это она (и выполнена или отброшена проверкой).
pub(super) fn is_comment_command(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::CommentsAvailable { .. }
            | Command::CommentPins { .. }
            | Command::CommentPinsRemove { .. }
            | Command::CommentOpen { .. }
            | Command::CommentDraft { .. }
            | Command::CommentFocus { .. }
            | Command::CommentShowResolved { .. }
    )
}

impl Core {
    pub(super) fn comment_command(&mut self, cmd: Command) {
        match cmd {
            Command::CommentsAvailable { on } => self.editor.set_comments_available(on),
            Command::CommentPins { pins, replace } => {
                let pins: Vec<CommentPin> = pins
                    .iter()
                    .take(COMMENT_PINS_MAX)
                    .filter_map(comment_pin)
                    .collect();
                self.editor.set_comment_pins(pins, replace);
            }
            Command::CommentPinsRemove { ids } => {
                let ids: Vec<&str> = ids
                    .iter()
                    .take(COMMENT_PINS_MAX)
                    .map(String::as_str)
                    .filter(|id| thread_id(id))
                    .collect();
                self.editor.remove_comment_pins(&ids);
            }
            Command::CommentOpen { id } => match id {
                None => self.editor.set_comment_open(None),
                Some(id) if thread_id(&id) => self.editor.set_comment_open(Some(&id)),
                Some(_) => {}
            },
            Command::CommentDraft { draft: d } => match d {
                None => self.editor.set_comment_draft(None),
                Some(d) => {
                    if let Some(a) = draft(&d) {
                        self.editor.set_comment_draft(Some(a));
                    }
                }
            },
            Command::CommentFocus { id } => {
                if thread_id(&id) {
                    self.editor.comment_focus(&id);
                }
            }
            Command::CommentShowResolved { on } => self.editor.set_comments_show_resolved(on),
            _ => unreachable!("is_comment_command"),
        }
    }
}

/// Событие редактора о комментариях — событием протокола; `None` — не о комментариях.
pub(super) fn comment_event(e: &EditorEvent) -> Option<CoreEvent> {
    let place = |a: &PinAnchor| {
        (
            a.page.to_string(),
            a.node.map(|n| n.to_string()),
            a.local,
            a.world,
        )
    };
    Some(match e {
        EditorEvent::CommentPlace { anchor, screen } => {
            let (page, node, l, w) = place(anchor);
            CoreEvent::CommentPlace {
                page,
                node,
                x: l.x,
                y: l.y,
                wx: w.x,
                wy: w.y,
                sx: screen.x,
                sy: screen.y,
            }
        }
        EditorEvent::CommentOpened { id, screen } => CoreEvent::CommentOpened {
            id: id.to_string(),
            sx: screen.x,
            sy: screen.y,
        },
        EditorEvent::CommentMoved { id, anchor } => {
            let (page, node, l, w) = place(anchor);
            CoreEvent::CommentMoved {
                id: id.to_string(),
                page,
                node,
                x: l.x,
                y: l.y,
                wx: w.x,
                wy: w.y,
            }
        }
        EditorEvent::CommentAnchor { screen, visible } => CoreEvent::CommentAnchor {
            sx: screen.x,
            sy: screen.y,
            visible: *visible,
        },
        EditorEvent::CommentsState {
            mode,
            visible,
            available,
        } => CoreEvent::CommentsState {
            mode: *mode,
            visible: *visible,
            available: *available,
        },
        _ => return None,
    })
}
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            | Command::PresenceReset
            | Command::Follow { .. }
            | Command::TextEditStart { .. }
            | Command::TextEditEnd
````

на:

````rust
            | Command::PresenceReset
            | Command::Follow { .. }
            | Command::CommentsAvailable { .. }
            | Command::CommentPins { .. }
            | Command::CommentPinsRemove { .. }
            | Command::CommentOpen { .. }
            | Command::CommentDraft { .. }
            | Command::CommentFocus { .. }
            | Command::CommentShowResolved { .. }
            | Command::TextEditStart { .. }
            | Command::TextEditEnd
````

В `crates/ferrum-wasm/src/core/presence.rs` заменить:

````rust
/// Команды, которые сцену не правят и чужое превью снимать не должны: присутствие, синхронизация,
/// чтение пикселей, выбор контекстного меню (цель — по тому, что видно, как у нажатия; меняется
/// только выделение). Остальные идут с `presence_hold`: правка не прочтёт чужое превью.
pub(crate) fn holds(cmd: &Command) -> bool {
    !matches!(
````

на:

````rust
/// Команды, которые сцену не правят и чужое превью снимать не должны: присутствие, синхронизация,
/// чтение пикселей, выбор контекстного меню (цель — по тому, что видно, как у нажатия; меняется
/// только выделение), комментарии (метки — оверлей, не сцена; переход к обсуждению меняет только
/// страницу и камеру). Остальные идут с `presence_hold`: правка не прочтёт чужое превью.
pub(crate) fn holds(cmd: &Command) -> bool {
    !matches!(
````

В `crates/ferrum-wasm/src/core/presence.rs` заменить:

````rust
            | Command::ReadPresence
            | Command::ContextMenu { .. }
    )
}
````

на:

````rust
            | Command::ReadPresence
            | Command::ContextMenu { .. }
            | Command::CommentsAvailable { .. }
            | Command::CommentPins { .. }
            | Command::CommentPinsRemove { .. }
            | Command::CommentOpen { .. }
            | Command::CommentDraft { .. }
            | Command::CommentFocus { .. }
            | Command::CommentShowResolved { .. }
    )
}
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
pub const PRESENCE_PART_IDS_MAX: usize = 2048;

/// Выделение участника: версия, сколько узлов, частей и общая рамка.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
````

на:

````rust
pub const PRESENCE_PART_IDS_MAX: usize = 2048;

/// Комментарии (спека подсистемы 4, раздел 10): меток в одной команде и у ядра всего не больше —
/// лишние не принимаются; идентификатор обсуждения — не длиннее; инициалы — не больше графем.
pub const COMMENT_PINS_MAX: usize = ferrum_editor::COMMENT_PINS_MAX;
pub const COMMENT_ID_MAX_BYTES: usize = 64;
pub const COMMENT_INITIALS_MAX: usize = 3;

/// Метка обсуждения от интерфейса: страница и узел (`counter@peer`, без узла — мир), точка в
/// координатах узла (`x`, `y`) и последняя мировая (`wx`, `wy`), инициалы и цвет автора
/// `0xRRGGBB`, ответов, не прочитано, закрыто, можно ли тащить.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentPinDto {
    pub id: String,
    pub page: String,
    #[serde(default)]
    #[tsify(optional)]
    pub node: Option<String>,
    pub x: f64,
    pub y: f64,
    pub wx: f64,
    pub wy: f64,
    pub initials: String,
    pub color: u32,
    pub replies: u32,
    pub unread: bool,
    pub resolved: bool,
    pub movable: bool,
}

/// Метка нового комментария, пока его пишут: где она (как у `CommentPinDto`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentDraftDto {
    pub page: String,
    #[serde(default)]
    #[tsify(optional)]
    pub node: Option<String>,
    pub x: f64,
    pub y: f64,
    pub wx: f64,
    pub wy: f64,
}

/// Выделение участника: версия, сколько узлов, частей и общая рамка.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        #[tsify(type = "number | undefined")]
        participant: Option<u32>,
    },
    /// Отдать журналу правки с прошлой пачки: ответ — `Journal` (и без правок).
````

на:

````rust
        #[tsify(type = "number | undefined")]
        participant: Option<u32>,
    },
    /// Комментарии есть (файл на сервере, роль пришла) или нет: без них режима «Комментарий»,
    /// Shift+C и меток нет. Отдельная сборка его не шлёт.
    CommentsAvailable {
        on: bool,
    },
    /// Метки обсуждений: `replace` — весь набор, иначе добавить и заменить по `id`.
    CommentPins {
        pins: Vec<CommentPinDto>,
        replace: bool,
    },
    CommentPinsRemove {
        ids: Vec<String>,
    },
    /// Открытое обсуждение (его метка выделена, `CommentAnchor` следует за ней); без `id` — нет.
    CommentOpen {
        #[serde(default)]
        #[tsify(optional)]
        id: Option<String>,
    },
    /// Метка нового комментария, пока его пишут; без `draft` — нет.
    CommentDraft {
        #[serde(default)]
        #[tsify(optional)]
        draft: Option<CommentDraftDto>,
    },
    /// Перейти к обсуждению: страница метки, камера по центру метки, затем `CommentOpened`.
    /// Метки или страницы ещё нет — переход, когда они придут.
    CommentFocus {
        id: String,
    },
    /// Показывать закрытые обсуждения.
    CommentShowResolved {
        on: bool,
    },
    /// Отдать журналу правки с прошлой пачки: ответ — `Journal` (и без правок).
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        participant: Option<u32>,
    },
    /// Перед `Opened`: поверх файла применены пачки журнала — правки, которые не
    /// успели попасть в файл (документ изменён).
````

на:

````rust
        participant: Option<u32>,
    },
    /// Щелчок по холсту в режиме «Комментарий»: место нового (страница, узел или мир, точка в
    /// его координатах и мировая) и экранная точка щелчка `sx`, `sy` (CSS-пиксели холста).
    CommentPlace {
        page: String,
        #[tsify(type = "string | undefined")]
        node: Option<String>,
        x: f64,
        y: f64,
        wx: f64,
        wy: f64,
        sx: f64,
        sy: f64,
    },
    /// Обсуждение открыто щелчком по метке или `CommentFocus`: окно — у экранной точки метки.
    CommentOpened {
        id: String,
        sx: f64,
        sy: f64,
    },
    /// Метку перетащили (только `movable`): новое место — на сервер.
    CommentMoved {
        id: String,
        page: String,
        #[tsify(type = "string | undefined")]
        node: Option<String>,
        x: f64,
        y: f64,
        wx: f64,
        wy: f64,
    },
    /// Экранная точка открытой метки (или черновика) сменилась; `visible` — на этой странице и
    /// в пределах холста. Только при изменении.
    CommentAnchor {
        sx: f64,
        sy: f64,
        visible: bool,
    },
    /// Режим «Комментарий», видны ли метки (Shift+C), есть ли комментарии: при изменении.
    CommentsState {
        mode: bool,
        visible: bool,
        available: bool,
    },
    /// Перед `Opened`: поверх файла применены пачки журнала — правки, которые не
    /// успели попасть в файл (документ изменён).
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm`
Expected: ядро собрано: `src/wasm`, в типах появились команды и события комментариев

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm`

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Expected: PASS: всё без замечаний; `ferrum-wasm` 239 passed

- [ ] **Step 6: Закоммитить**

```bash
git add Cargo.lock Cargo.toml crates/ferrum-wasm/Cargo.toml crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/comments.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/core/presence.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/comments.rs crates/ferrum-wasm/tests/protocol.rs
git commit -F - <<'EOF'
ядро: команды и события комментариев, проверка ввода хозяина

protocol.rs (tsify): Command::CommentsAvailable { on }, CommentPins { pins,
replace }, CommentPinsRemove { ids }, CommentOpen { id? }, CommentDraft
{ draft? }, CommentFocus { id }, CommentShowResolved { on }; CommentPinDto
{ id, page, node?, x, y, wx, wy, initials, color, replies, unread, resolved,
movable }, CommentDraftDto { page, node?, x, y, wx, wy }; CoreEvent::
CommentPlace { page, node, x, y, wx, wy, sx, sy }, CommentOpened { id, sx,
sy }, CommentMoved { id, page, node, x, y, wx, wy }, CommentAnchor { sx, sy,
visible } (в пачке — последнее), CommentsState { mode, visible, available };
Tool::Comment.

core/comments.rs: ввод хозяина не доверен — координаты конечны и не больше
1e9 по модулю, идентификатор обсуждения — 1…64 байта [A-Za-z0-9_-], страница
и узел — counter@peer не длиннее 64 байт и не черновик, цвет ≤ 0xFFFFFF,
инициалы ≤ 32 байт и до трёх графем (длиннее — укорачиваются, поведение),
меток не больше 10 000 (лишние не принимаются). Неверная метка пропускается
одна, тихо, как запись присутствия; неверный идентификатор в остальных
командах — команда ничего не делает. Команды комментариев не берут
удержание чужого превью (presence::holds) и, кроме CommentFocus (меняет
страницу), не ждут шрифтов. Зависимость unicode-segmentation 1.13.3 — та же,
что уже была в Cargo.lock.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 5: Сервер — таблицы, функции изменения и маршруты REST

Спека §10.1, §10.3, §10.4; «Решения» 1, 2, 4, 5, 7, 8, 19, 20. Миграция `010_comments.sql`; изменение — функция базы, которая берёт блокировку номера файла и сверяет роль; чужие id — 404 без записи; пределы; подсказка упоминаний; настройка писем; строки очереди писем заводит изменение (отправка — задача 7). Все 11 маршрутов — в таблице `access.test.ts`.

**Files:**
- Create: `apps/server/migrations/010_comments.sql`, `apps/server/src/comment-routes.ts`
- Modify: `apps/server/src/api.ts`, `apps/server/src/app.ts`
- Test: `apps/server/test/access.test.ts`, `apps/server/test/api-stand.ts`, `apps/server/test/comments.test.ts` (новый), `apps/server/test/migrate.test.ts`

**Interfaces:**
- Consumes: виды и `parse*` задачи 3.
- Produces: `commentRoutes(deps: ApiDeps): Route[]`, `COMMENT_MAIL_DELAY_MS = 300 000`; `LIMITS.comment = { max: 60, windowSeconds: 60 }`, `LIMITS.commentList = { max: 120, windowSeconds: 60 }`; `PARAM_CHECK.threadId`, `PARAM_CHECK.commentId`; `Route.keepsAccess?: boolean`; `readJson(req, max?)`; `ApiDeps.comments?: { event(file, ev): void; openUsers(file): string[] }`, `ApiDeps.commentMailDelayMs?`, `AppDeps.commentMailDelayMs?`; маршруты `GET/POST /api/files/:id/comments`, `GET/PATCH /api/files/:id/comments/:threadId`, `POST …/:threadId/comments`, `PATCH/DELETE …/:threadId/comments/:commentId`, `POST …/:threadId/read`, `GET /api/files/:id/mentions?q=`, `GET/PUT /api/me/prefs`; функции базы `comment_create`, `comment_reply`, `comment_edit`, `comment_delete`, `comment_resolve`, `comment_move`.

- [ ] **Step 1: Написать тесты**

В `apps/server/test/access.test.ts` заменить:

````ts
  project: string;
  member: string;
}

````

на:

````ts
  project: string;
  member: string;
  /** Обсуждение владельца и его первый комментарий. */
  thread: string;
  comment: string;
}

````

В `apps/server/test/access.test.ts` заменить:

````ts
  const member = await s.pool.query<{ id: string }>('SELECT id FROM team_member WHERE team_id = $1 AND user_id = $2', [t, editor.id]);
  const project = await s.pool.query<{ id: string }>(`INSERT INTO project (team_id, name) VALUES ($1, 'Проект') RETURNING id`, [t]);
  return { file: f.id, entry: entry.rows[0].id, team: t, project: project.rows[0].id, member: member.rows[0].id };
}

````

на:

````ts
  const member = await s.pool.query<{ id: string }>('SELECT id FROM team_member WHERE team_id = $1 AND user_id = $2', [t, editor.id]);
  const project = await s.pool.query<{ id: string }>(`INSERT INTO project (team_id, name) VALUES ($1, 'Проект') RETURNING id`, [t]);
  const c = await s.pool.query<{ r: { thread: { id: string; first: { id: string } } } }>(
    `SELECT comment_create($1, $2, '1@1', NULL, 0, 0, 0, 0, 'Первый', '{}', '{}', 1000) AS r`,
    [f.id, owner.id],
  );
  const thread = c.rows[0].r.thread;
  return {
    file: f.id,
    entry: entry.rows[0].id,
    team: t,
    project: project.rows[0].id,
    member: member.rows[0].id,
    thread: thread.id,
    comment: thread.first.id,
  };
}

````

В `apps/server/test/access.test.ts` заменить:

````ts
const team = (w: World) => ({ teamId: w.team });
const project = (w: World) => ({ projectId: w.project });

const SAMPLES: Record<string, Sample> = {
````

на:

````ts
const team = (w: World) => ({ teamId: w.team });
const project = (w: World) => ({ projectId: w.project });
const thread = (w: World) => ({ id: w.file, threadId: w.thread });
const comment = (w: World) => ({ id: w.file, threadId: w.thread, commentId: w.comment });

const SAMPLES: Record<string, Sample> = {
````

В `apps/server/test/access.test.ts` заменить:

````ts
  'POST /api/files/:id/redeem': { params: file, body: { token: 'x'.repeat(43) }, allowed: stranger, status: 404 },
  'POST /api/files/:id/request-access': { params: file, allowed: stranger },
};

````

на:

````ts
  'POST /api/files/:id/redeem': { params: file, body: { token: 'x'.repeat(43) }, allowed: stranger, status: 404 },
  'POST /api/files/:id/request-access': { params: file, allowed: stranger },
  // Комментарии: каждый с доступом, и читатель тоже; правка — только своё, удаление — своё или владелец, закрытие — автор
  // или редактор (`comments.test.ts` — права каждого действия подробно).
  'GET /api/files/:id/comments': { params: file, allowed: viewer },
  'POST /api/files/:id/comments': {
    params: file,
    body: { pin: { page: '1@1', node: null, x: 0, y: 0, wx: 0, wy: 0 }, body: 'Новый' },
    allowed: viewer,
    status: 201,
  },
  'GET /api/files/:id/comments/:threadId': { params: thread, allowed: viewer },
  'PATCH /api/files/:id/comments/:threadId': { params: thread, body: { resolved: true }, allowed: editor, denied: [viewer] },
  'POST /api/files/:id/comments/:threadId/comments': { params: thread, body: { body: 'Ответ' }, allowed: viewer, status: 201 },
  'PATCH /api/files/:id/comments/:threadId/comments/:commentId': {
    params: comment,
    body: { body: 'Правка' },
    allowed: owner,
    denied: [viewer, editor, linked],
  },
  'DELETE /api/files/:id/comments/:threadId/comments/:commentId': { params: comment, allowed: owner, denied: [viewer, editor, linked] },
  'POST /api/files/:id/comments/:threadId/read': { params: thread, allowed: viewer },
  'GET /api/files/:id/mentions': { params: file, allowed: viewer },
  'GET /api/me/prefs': {},
  'PUT /api/me/prefs': { body: { commentMail: false } },
};

````

В `apps/server/test/access.test.ts` заменить:

````ts
      const w = await world();
      const path = pathOf(route.path, sample.params?.(w) ?? {});
      const before = await s.pool.query('SELECT name, version, trashed_at FROM file ORDER BY id');
      const res = await s.call(owner, route.method, path, sample.body, { ...headersOf(route, sample), origin: 'https://evil.example' });
      expect(res.status).toBe(403);
      expect(await res.json()).toEqual({ error: 'BAD_ORIGIN' });
      expect((await s.pool.query('SELECT name, version, trashed_at FROM file ORDER BY id')).rows).toEqual(before.rows);
      const none = await s.call(owner, route.method, path, sample.body, { ...headersOf(route, sample), origin: '' });
      expect(none.status).toBe(403);
````

на:

````ts
      const w = await world();
      const path = pathOf(route.path, sample.params?.(w) ?? {});
      // Файлы и комментарии: номер файла комментариев растёт с каждым их изменением.
      const state =
        'SELECT name, version, trashed_at, (SELECT seq FROM comment_file c WHERE c.file_id = f.id) AS comments FROM file f ORDER BY id';
      const before = await s.pool.query(state);
      const res = await s.call(owner, route.method, path, sample.body, { ...headersOf(route, sample), origin: 'https://evil.example' });
      expect(res.status).toBe(403);
      expect(await res.json()).toEqual({ error: 'BAD_ORIGIN' });
      expect((await s.pool.query(state)).rows).toEqual(before.rows);
      const none = await s.call(owner, route.method, path, sample.body, { ...headersOf(route, sample), origin: '' });
      expect(none.status).toBe(403);
````

В `apps/server/test/api-stand.ts` заменить:

````ts
  const reset = () =>
    pool.query(
      'TRUNCATE file, doc_state, doc_update, doc_peer, file_access, file_invite, share_link, file_link_user, file_recent, file_image, image, blob_trash, blob_pending, team, team_member, team_invite, project CASCADE',
    );

````

на:

````ts
  const reset = () =>
    pool.query(
      'TRUNCATE file, doc_state, doc_update, doc_peer, file_access, file_invite, share_link, file_link_user, file_recent, file_image, image, blob_trash, blob_pending, team, team_member, team_invite, project, comment_file, comment_thread, comment, comment_mention, comment_read, mail_outbox, user_pref CASCADE',
    );

````

Создать `apps/server/test/comments.test.ts`:

````ts
// Комментарии через REST (спека, раздел 10): права каждого действия, чужие id, текст, упоминания, прочтение, номер и
// `?after=`, пределы, удаление файла и человека, очередь писем. Ответы сервера разбирает строгий разбор клиента: что не
// примет клиент, не пропускает и тест.

import {
  COMMENT_BODY_MAX,
  COMMENT_PAGE_MAX,
  parseCommentList,
  parseCommentWrite,
  parseMentionList,
  parseThreadDetail,
  THREAD_COMMENTS_MAX,
  type CommentListDto,
  type CommentWriteDto,
  type PinDto,
  type ThreadDetailDto,
} from '@ferrum/protocol';
import { afterAll, beforeEach, expect, spyOn, test } from 'bun:test';
import { apiStand, type Person } from './api-stand';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('c-owner@example.com', 'Ольга');
const editor = await s.signIn('c-editor@example.com', 'Эдуард');
const viewer = await s.signIn('c-viewer@example.com', 'Вера');
const linked = await s.signIn('c-linked@example.com', 'Лев');
const member = await s.signIn('c-member@example.com', 'Мария');
const stranger = await s.signIn('c-stranger@example.com', 'Чужой');

beforeEach(async () => {
  await s.reset();
  // Пределы частоты — на человека и общие для всех тестов файла: каждый тест начинает с нуля.
  for (const key of (await s.valkey.send('KEYS', ['api:*'])) as string[]) await s.valkey.del(key);
});

const pin: PinDto = { page: '3@7910247476534078275', node: '12@7910247476534078275', x: 10, y: 20, wx: 110, wy: 220 };

/** Файл владельца: редактор и читатель напрямую, «пришедший» — по ссылке, участник — через команду проекта. */
async function world(): Promise<{ file: string; other: string }> {
  const team = await s.pool.query<{ id: string }>(`INSERT INTO team (name) VALUES ('Команда') RETURNING id`);
  await s.pool.query(`INSERT INTO team_member (team_id, user_id, role) VALUES ($1, $2, 'owner'), ($1, $3, 'member')`, [
    team.rows[0].id,
    owner.id,
    member.id,
  ]);
  const project = await s.pool.query<{ id: string }>(`INSERT INTO project (team_id, name) VALUES ($1, 'Проект') RETURNING id`, [
    team.rows[0].id,
  ]);
  const file = (await s.newFile(owner, 'Файл', project.rows[0].id)).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
    file,
    editor.id,
    viewer.id,
  ]);
  await s.pool.query(`INSERT INTO share_link (file_id, token_hash, nonce, role, enabled) VALUES ($1, $2, 'n', 'viewer', true)`, [
    file,
    'c'.repeat(64),
  ]);
  await s.pool.query('INSERT INTO file_link_user (file_id, user_id) VALUES ($1, $2)', [file, linked.id]);
  // Другой файл того же владельца, где у всех тех же людей тоже есть доступ: чужие id проверяются не только доступом.
  const other = (await s.newFile(owner, 'Другой')).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
    other,
    editor.id,
    viewer.id,
  ]);
  return { file, other };
}

const base = (file: string) => `/api/files/${file}/comments`;

async function write(res: Response, status = 201): Promise<CommentWriteDto> {
  const text = await res.text();
  expect(res.status, text).toBe(status);
  const dto = parseCommentWrite(JSON.parse(text));
  if (!dto) throw new Error(`ответ не разобран клиентом: ${text}`);
  return dto;
}

const create = async (who: Person, file: string, body = 'Привет', p: PinDto = pin) =>
  write(await s.call(who, 'POST', base(file), { pin: p, body }));
const reply = async (who: Person, file: string, thread: string, body = 'Ответ') =>
  write(await s.call(who, 'POST', `${base(file)}/${thread}/comments`, { body }));

async function list(who: Person, file: string, query = ''): Promise<CommentListDto> {
  const res = await s.call(who, 'GET', `${base(file)}${query}`);
  const text = await res.text();
  expect(res.status, text).toBe(200);
  const dto = parseCommentList(JSON.parse(text));
  if (!dto) throw new Error(`список не разобран клиентом: ${text}`);
  return dto;
}

async function detail(who: Person, file: string, thread: string): Promise<ThreadDetailDto> {
  const res = await s.call(who, 'GET', `${base(file)}/${thread}`);
  const text = await res.text();
  expect(res.status, text).toBe(200);
  const dto = parseThreadDetail(JSON.parse(text));
  if (!dto) throw new Error(`обсуждение не разобрано клиентом: ${text}`);
  return dto;
}

const rows = async (sql: string, params: unknown[] = []) => (await s.pool.query(sql, params)).rows;

test('обсуждение: создание, ответ, список и обсуждение целиком; читатель и пришедший по ссылке тоже пишут', async () => {
  const { file } = await world();
  const a = await create(viewer, file, 'Первый');
  expect(a.thread.first.body).toBe('Первый');
  expect(a.thread.author).toEqual({ id: viewer.id, name: 'Вера' });
  expect(a.thread.pin).toEqual(pin);
  expect(a.thread.replies).toBe(0);
  expect(a.thread.unread).toBe(false);
  expect(a.comment?.id).toBe(a.thread.first.id);
  const b = await reply(linked, file, a.thread.id, 'По ссылке');
  expect(b.seq).toBe(a.seq + 1);
  expect(b.thread.replies).toBe(1);
  expect(b.thread.participants).toEqual([viewer.id, linked.id]);
  expect(b.thread.lastAt).toBe(b.comment!.createdAt);
  const l = await list(owner, file);
  expect(l.seq).toBe(b.seq);
  expect(l.threads.map((t) => [t.id, t.seq, t.replies])).toEqual([[a.thread.id, b.seq, 1]]);
  const d = await detail(member, file, a.thread.id);
  expect(d.comments.map((c) => [c.body, c.author.name])).toEqual([
    ['Первый', 'Вера'],
    ['По ссылке', 'Лев'],
  ]);
  // Без пина — неверно.
  expect((await s.call(owner, 'POST', base(file), { body: 'x' })).status).toBe(400);
});

test('чужие id: обсуждение и комментарий другого файла через этот адрес — как несуществующие, без записи', async () => {
  const { file, other } = await world();
  const mine = await create(owner, file, 'Здесь');
  const theirs = await create(owner, other, 'Там');
  const reply2 = await reply(owner, other, theirs.thread.id);
  const t = theirs.thread.id;
  const c = reply2.comment!.id;
  const before = await rows('SELECT seq FROM comment_file ORDER BY file_id');
  const attempts: [string, string, unknown?][] = [
    ['GET', `${base(file)}/${t}`],
    ['POST', `${base(file)}/${t}/comments`, { body: 'x' }],
    ['PATCH', `${base(file)}/${t}`, { resolved: true }],
    ['PATCH', `${base(file)}/${t}`, { pin }],
    ['PATCH', `${base(file)}/${t}/comments/${c}`, { body: 'x' }],
    ['DELETE', `${base(file)}/${t}/comments/${c}`],
    ['DELETE', `${base(file)}/${t}/comments/${theirs.thread.first.id}`],
    ['POST', `${base(file)}/${t}/read`],
    // Своё обсуждение, чужой комментарий: тоже нет.
    ['PATCH', `${base(file)}/${mine.thread.id}/comments/${c}`, { body: 'x' }],
    ['DELETE', `${base(file)}/${mine.thread.id}/comments/${c}`],
  ];
  for (const [method, path, body] of attempts) {
    const res = await s.call(owner, method, path, body);
    expect(res.status, `${method} ${path}`).toBe(404);
    expect(await res.json()).toEqual({ error: 'NOT_FOUND' });
  }
  expect(await rows('SELECT seq FROM comment_file ORDER BY file_id')).toEqual(before);
  expect((await detail(owner, other, t)).comments.map((x) => x.body)).toEqual(['Там', 'Ответ']);
  expect(await rows('SELECT count(*)::int AS n FROM comment_read WHERE thread_id = $1 AND user_id <> $2', [t, owner.id])).toEqual([
    { n: 0 },
  ]);
  // Неверный вид id — 404 маршрута, до базы.
  expect((await s.call(owner, 'GET', `${base(file)}/not-a-uuid`)).status).toBe(404);
});

test('права: правка — только своё; удаление — своё или владелец; закрытие — автор или редактор; перенос — автор или владелец', async () => {
  const { file } = await world();
  const v = await create(viewer, file, 'Читатель');
  const r = await reply(editor, file, v.thread.id, 'Редактор');
  const path = (c: string) => `${base(file)}/${v.thread.id}/comments/${c}`;
  // Правка чужого — 403 и владельцу.
  for (const who of [owner, editor, linked])
    expect((await s.call(who, 'PATCH', path(v.comment!.id), { body: 'x' })).status, who.email).toBe(403);
  const edited = await write(await s.call(viewer, 'PATCH', path(v.comment!.id), { body: 'Правка' }), 200);
  expect(edited.comment?.body).toBe('Правка');
  expect(edited.comment?.editedAt).not.toBeNull();
  // Удаление чужого: читатель и редактор — нет, владелец — да.
  expect((await s.call(viewer, 'DELETE', path(r.comment!.id))).status).toBe(403);
  expect((await s.call(member, 'DELETE', path(r.comment!.id))).status).toBe(403);
  const del = await write(await s.call(owner, 'DELETE', path(r.comment!.id)), 200);
  expect(del.removed).toBe(false);
  expect(del.thread.replies).toBe(0);
  // Своё удаляет каждый.
  const mine = await reply(linked, file, v.thread.id, 'Моё');
  expect((await write(await s.call(linked, 'DELETE', path(mine.comment!.id)), 200)).thread.replies).toBe(0);
  // Закрыть: чужое обсуждение читателю и пришедшему по ссылке — нет; автору-читателю — да; редактору — да.
  const thread = `${base(file)}/${v.thread.id}`;
  expect((await s.call(linked, 'PATCH', thread, { resolved: true })).status).toBe(403);
  const resolved = await write(await s.call(viewer, 'PATCH', thread, { resolved: true }), 200);
  expect(resolved.thread.resolvedBy).toEqual({ id: viewer.id, name: 'Вера' });
  expect(resolved.thread.resolvedAt).not.toBeNull();
  const reopened = await write(await s.call(editor, 'PATCH', thread, { resolved: false }), 200);
  expect([reopened.thread.resolvedAt, reopened.thread.resolvedBy]).toEqual([null, null]);
  // Перенос: автор и владелец; редактор — нет.
  const moved = { ...pin, node: null, x: 5, y: 6, wx: 5, wy: 6 };
  expect((await s.call(editor, 'PATCH', thread, { pin: moved })).status).toBe(403);
  expect((await write(await s.call(viewer, 'PATCH', thread, { pin: moved }), 200)).thread.pin).toEqual(moved);
  expect((await write(await s.call(owner, 'PATCH', thread, { pin }), 200)).thread.pin).toEqual(pin);
  // Отказ не тратит номер.
  const seq = (await list(owner, file)).seq;
  expect((await s.call(editor, 'PATCH', thread, { pin: moved })).status).toBe(403);
  expect((await list(owner, file)).seq).toBe(seq);
});

test('удаление первого комментария удаляет обсуждение для всех: список, надгробие, обсуждение, ответ', async () => {
  const { file } = await world();
  const a = await create(editor, file, 'Первый');
  await reply(viewer, file, a.thread.id, 'Второй');
  const keep = await create(owner, file, 'Другое');
  const known = (await list(viewer, file)).seq;
  const gone = await write(await s.call(editor, 'DELETE', `${base(file)}/${a.thread.id}/comments/${a.thread.first.id}`), 200);
  expect(gone.removed).toBe(true);
  expect(gone.seq).toBe(known + 1);
  // Последнее состояние — с текстом: клиент закрывает окно обсуждения по нему.
  expect(gone.thread.first.body).toBe('Первый');
  expect((await list(viewer, file)).threads.map((t) => t.id)).toEqual([keep.thread.id]);
  const after = await list(viewer, file, `?after=${known}`);
  expect(after).toEqual({ seq: gone.seq, threads: [], removed: [a.thread.id], more: false });
  expect((await s.call(viewer, 'GET', `${base(file)}/${a.thread.id}`)).status).toBe(404);
  expect((await s.call(viewer, 'POST', `${base(file)}/${a.thread.id}/comments`, { body: 'x' })).status).toBe(404);
  // Текст удалённого не хранится.
  expect(await rows('SELECT count(*)::int AS n FROM comment WHERE thread_id = $1', [a.thread.id])).toEqual([{ n: 0 }]);
});

test('текст: только текст, предел в кодовых точках; лишние поля, пустое и сверх предела — 400', async () => {
  const { file } = await world();
  const t = await create(owner, file, '  <img src=x onerror=alert(1)>\r\n\u202eстрока\u0000  ');
  expect(t.thread.first.body).toBe('<img src=x onerror=alert(1)>\nстрока');
  // 4000 четырёхбайтных знаков — 16 000 байт текста: помещаются в запрос.
  const emoji = '😀'.repeat(COMMENT_BODY_MAX);
  expect((await reply(owner, file, t.thread.id, emoji)).comment?.body).toBe(emoji);
  for (const body of ['', '   ', '\u202e', 'а'.repeat(COMMENT_BODY_MAX + 1), 5])
    expect((await s.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body })).status).toBe(400);
  expect((await s.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: 'x', html: '<b>' })).status).toBe(400);
  const many = Array.from({ length: 21 }, (_, i) => `@[u${i}]`).join(' ');
  expect((await s.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: many })).status).toBe(400);
  expect((await s.call(owner, 'POST', base(file), { pin: { ...pin, x: 1e10 }, body: 'x' })).status).toBe(400);
});

test('упоминания: строка и имя — только у кого доступ при отправке; у остальных токен остаётся без имени', async () => {
  const { file } = await world();
  const body = `@[${editor.id}] @[${stranger.id}] @[${linked.id}] @[nobody] @[${editor.id}]`;
  const t = await create(viewer, file, body);
  expect(t.comment?.body).toBe(body);
  expect(t.comment?.mentions).toEqual({ [editor.id]: 'Эдуард', [linked.id]: 'Лев' });
  expect(await rows('SELECT user_id FROM comment_mention ORDER BY user_id')).toEqual(
    [editor.id, linked.id].sort().map((user_id) => ({ user_id })),
  );
  // Ни имени, ни адреса чужого — нигде в ответах.
  const all = JSON.stringify([await list(viewer, file), await detail(viewer, file, t.thread.id)]);
  expect(all).not.toContain('Чужой');
  expect(all).not.toContain('c-stranger@');
  expect(all).not.toContain('@example.com');
  // Правка: новое упоминание — с проверкой доступа сейчас; заведённое при отправке остаётся, пока токен в тексте; снятое из
  // текста — уходит.
  await s.pool.query('DELETE FROM file_link_user WHERE user_id = $1', [linked.id]);
  const path = `${base(file)}/${t.thread.id}/comments/${t.comment!.id}`;
  const e = await write(await s.call(viewer, 'PATCH', path, { body: `@[${linked.id}] @[${owner.id}] @[${stranger.id}]` }), 200);
  expect(e.comment?.mentions).toEqual({ [linked.id]: 'Лев', [owner.id]: 'Ольга' });
  // Упомянутые одним комментарием — по id: время у них одно.
  expect(e.thread.participants).toEqual([viewer.id, ...[linked.id, owner.id].sort()]);
  const e2 = await write(await s.call(viewer, 'PATCH', path, { body: `@[${owner.id}]` }), 200);
  expect(e2.comment?.mentions).toEqual({ [owner.id]: 'Ольга' });
  expect(e2.thread.participants).toEqual([viewer.id, owner.id]);
});

test('не прочитано — у каждого своё; открыл — прочитал; своё не бывает непрочитанным', async () => {
  const { file } = await world();
  const t = await create(owner, file, 'Вопрос');
  const unread = async (who: Person) => (await list(who, file)).threads[0].unread;
  expect(await unread(owner)).toBe(false);
  expect(await unread(viewer)).toBe(true);
  expect(await unread(editor)).toBe(true);
  expect((await s.call(viewer, 'POST', `${base(file)}/${t.thread.id}/read`)).status).toBe(200);
  expect(await unread(viewer)).toBe(false);
  expect(await unread(editor)).toBe(true);
  // Ответ читателя: у владельца снова не прочитано, у читателя — нет (ответил — прочитал).
  await reply(viewer, file, t.thread.id);
  expect(await unread(owner)).toBe(true);
  expect(await unread(viewer)).toBe(false);
  // Удалили чужой непрочитанный — прочитано.
  const r = await reply(editor, file, t.thread.id);
  expect(await unread(viewer)).toBe(true);
  await s.call(editor, 'DELETE', `${base(file)}/${t.thread.id}/comments/${r.comment!.id}`);
  expect(await unread(viewer)).toBe(false);
  expect((await detail(owner, file, t.thread.id)).thread.unread).toBe(true);
});

test('?after: изменения по номеру, страницы, `more` и номер страницы; неверное — 400', async () => {
  const { file } = await world();
  const made: CommentWriteDto[] = [];
  for (let i = 0; i < 5; i++) made.push(await create(owner, file, `т${i}`));
  expect(made.map((m) => m.seq)).toEqual([1, 2, 3, 4, 5]);
  const p1 = await list(viewer, file, '?limit=2');
  expect([p1.seq, p1.more, p1.threads.map((t) => t.first.body)]).toEqual([2, true, ['т0', 'т1']]);
  const p2 = await list(viewer, file, `?after=${p1.seq}&limit=2`);
  const p3 = await list(viewer, file, `?after=${p2.seq}&limit=2`);
  expect([p3.seq, p3.more, p3.threads.map((t) => t.first.body)]).toEqual([5, false, ['т4']]);
  // Изменение старого обсуждения поднимает его в конец.
  await reply(editor, file, made[0].thread.id);
  const changed = await list(viewer, file, '?after=5');
  expect([changed.seq, changed.threads.map((t) => [t.id, t.seq])]).toEqual([6, [[made[0].thread.id, 6]]]);
  // Ничего нового — голова файла и пусто.
  expect(await list(viewer, file, '?after=6')).toEqual({ seq: 6, threads: [], removed: [], more: false });
  for (const q of ['?after=-1', '?after=x', '?limit=0', '?limit=1.5', '?after=99999999999999999'])
    expect((await s.call(viewer, 'GET', `${base(file)}${q}`)).status, q).toBe(400);
  // Предел страницы урезается, а не отвергается.
  expect((await list(viewer, file, '?limit=100000')).threads).toHaveLength(5);
  // Файл без комментариев.
  const empty = (await s.newFile(owner, 'Пустой')).id;
  expect(await list(owner, empty)).toEqual({ seq: 0, threads: [], removed: [], more: false });
});

test('одновременные ответы: номера без пропусков, и ответ с номером N видит все изменения до N', async () => {
  const { file } = await world();
  const t = await create(owner, file);
  const people = [owner, editor, viewer, linked, member];
  const all = await Promise.all(Array.from({ length: 25 }, (_, i) => reply(people[i % 5], file, t.thread.id, `о${i}`)));
  const seqs = all.map((r) => r.seq).sort((a, b) => a - b);
  expect(seqs).toEqual(Array.from({ length: 25 }, (_, i) => t.seq + 1 + i));
  // Ответ несёт число ответов на момент своего номера: снимок до блокировки дал бы меньше.
  for (const r of all) expect(r.thread.replies, `seq ${r.seq}`).toBe(r.seq - t.seq);
  expect((await list(owner, file)).threads[0].replies).toBe(25);
  // Время комментариев растёт вместе с номером: прочтение по времени не пропускает соседа.
  const d = await detail(owner, file, t.thread.id);
  const bySeq = [...all].sort((a, b) => a.seq - b.seq).map((r) => r.comment!.id);
  expect(d.comments.slice(1).map((c) => c.id)).toEqual(bySeq);
});

test('предел комментариев — 60 в минуту на человека, общий для всех его сеансов и вкладок', async () => {
  const { file } = await world();
  const t = await create(editor, file);
  const second = await s.signIn(editor.email, 'Эдуард');
  expect(second.cookie).not.toBe(editor.cookie);
  // Первый — уже создан: 59 ответов вперемешку из двух сеансов.
  for (let i = 0; i < 59; i++) {
    const res = await s.call(i % 2 ? second : editor, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: `${i}` });
    expect(res.status, `ответ ${i}`).toBe(201);
  }
  for (const who of [editor, second]) {
    const res = await s.call(who, 'POST', base(file), { pin, body: 'ещё' });
    expect(res.status).toBe(429);
    expect(Number(res.headers.get('retry-after'))).toBeGreaterThan(0);
  }
  // Другой человек не задет; правка — в своём пределе.
  expect((await s.call(viewer, 'POST', base(file), { pin, body: 'я' })).status).toBe(201);
  expect((await s.call(editor, 'PATCH', `${base(file)}/${t.thread.id}/comments/${t.comment!.id}`, { body: 'правка' })).status).toBe(200);
  // Список — свой предел, 120 в минуту на человека: первая загрузка дорогая, догоняние дешёвое.
  for (let i = 0; i < 120; i++) expect((await s.call(i % 2 ? second : editor, 'GET', base(file))).status).toBe(200);
  expect((await s.call(editor, 'GET', base(file))).status).toBe(429);
  expect((await s.call(editor, 'GET', `${base(file)}/${t.thread.id}`)).status).toBe(200);
});

test('предел обсуждения: ответ сверх — 409 THREAD_FULL без записи; страница — не больше 500', async () => {
  const { file } = await world();
  const t = await create(owner, file);
  await s.pool.query(
    `INSERT INTO comment (id, thread_id, author_id, body, created_at)
     SELECT gen_random_uuid(), $1, $2, 'x', now() + make_interval(secs => i) FROM generate_series(1, $3) i`,
    [t.thread.id, owner.id, THREAD_COMMENTS_MAX - 2],
  );
  const last = await reply(viewer, file, t.thread.id, 'последний');
  expect(last.thread.replies).toBe(THREAD_COMMENTS_MAX - 1);
  const full = await s.call(viewer, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: 'лишний' });
  expect(full.status).toBe(409);
  expect(await full.json()).toEqual({ error: 'THREAD_FULL' });
  expect((await list(owner, file)).seq).toBe(last.seq);
  expect((await detail(owner, file, t.thread.id)).comments).toHaveLength(THREAD_COMMENTS_MAX);
  // Страница: 501 обсуждение — 500 и `more`.
  await s.pool.query(
    `INSERT INTO comment_thread (id, file_id, page_id, x, y, wx, wy, first_id, created_by, created_at, seq)
     SELECT gen_random_uuid(), $1, '1@1', 0, 0, 0, 0, gen_random_uuid(), $2, now(), 1000 + i FROM generate_series(1, 501) i`,
    [file, owner.id],
  );
  await s.pool.query(
    `INSERT INTO comment (id, thread_id, author_id, body, created_at) SELECT first_id, id, created_by, 'x', created_at FROM comment_thread
     WHERE file_id = $1 AND seq > 1000`,
    [file],
  );
  const page = await list(owner, file, `?after=${last.seq}`);
  expect([page.threads.length, page.more]).toEqual([COMMENT_PAGE_MAX, true]);
});

test('подсказка упоминаний: люди с доступом без себя; адрес — только участнику файла; поиск — буквы, а не шаблон', async () => {
  const { file } = await world();
  const people = async (who: Person, q = '') => {
    const res = await s.call(who, 'GET', `/api/files/${file}/mentions?q=${encodeURIComponent(q)}`);
    expect(res.status).toBe(200);
    return parseMentionList(await res.json())!.people;
  };
  const names = (p: { name: string }[]) => p.map((x) => x.name).sort();
  expect(names(await people(owner))).toEqual(['Вера', 'Лев', 'Мария', 'Эдуард']);
  expect(names(await people(viewer))).toEqual(['Лев', 'Мария', 'Ольга', 'Эдуард']);
  expect((await people(editor)).find((p) => p.name === 'Вера')?.email).toBe(viewer.email);
  // Пришедший по ссылке видит имена, адресов — нет, и по адресу не ищет.
  const viaLink = await people(linked);
  expect(names(viaLink)).toEqual(['Вера', 'Мария', 'Ольга', 'Эдуард']);
  expect(viaLink.every((p) => p.email === null)).toBe(true);
  expect(await people(linked, 'c-viewer')).toEqual([]);
  expect(names(await people(editor, 'c-viewer'))).toEqual(['Вера']);
  expect(names(await people(editor, 'ве'))).toEqual(['Вера']);
  // `%` и `_` — буквы.
  expect(await people(owner, '%')).toEqual([]);
  expect(await people(owner, '_')).toEqual([]);
  // Ссылку выключили — пришедшего по ней больше не предлагают.
  await s.pool.query('UPDATE share_link SET enabled = false WHERE file_id = $1', [file]);
  expect(names(await people(owner))).toEqual(['Вера', 'Мария', 'Эдуард']);
  expect((await s.call(owner, 'GET', `/api/files/${file}/mentions?q=${'a'.repeat(65)}`)).status).toBe(400);
  // Не больше 8.
  for (let i = 0; i < 9; i++) {
    const p = await s.signIn(`c-crowd-${i}@example.com`, `Толпа ${i}`);
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, p.id]);
  }
  expect(await people(owner, 'Толпа')).toHaveLength(8);
});

test('функции изменения сверяют роль сами: снятый доступ не пишет, даже минуя маршрут', async () => {
  const { file } = await world();
  const t = await create(owner, file);
  const before = await rows('SELECT seq FROM comment_file');
  // Маршрут проверил роль, а между проверкой и записью её сняли: запись видит это сама.
  const calls: [string, unknown[]][] = [
    ['comment_create($1, $2, $3, NULL, 0, 0, 0, 0, $4, $5, $6, 1000)', [file, stranger.id, pin.page, 'x', [], []]],
    ['comment_reply($1, $2, $3, $4, $5, $6, 1000, 500)', [file, stranger.id, t.thread.id, 'x', [], []]],
    ['comment_edit($1, $2, $3, $4, $5, $6)', [file, stranger.id, t.thread.id, t.comment!.id, 'x', []]],
    ['comment_delete($1, $2, $3, $4)', [file, stranger.id, t.thread.id, t.comment!.id]],
    ['comment_resolve($1, $2, $3, true)', [file, stranger.id, t.thread.id]],
    ['comment_move($1, $2, $3, $4, NULL, 0, 0, 0, 0)', [file, stranger.id, t.thread.id, pin.page]],
  ];
  for (const [sql, params] of calls) {
    const e = await s.pool.query(`SELECT ${sql}`, params).catch((x: unknown) => x as { code: string });
    expect((e as { code?: string }).code, sql).toBe('CM404');
  }
  expect(await rows('SELECT seq FROM comment_file')).toEqual(before);
  // Через маршрут: доступ сняли — 404.
  await s.pool.query('DELETE FROM file_access WHERE user_id = $1', [viewer.id]);
  expect((await s.call(viewer, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: 'x' })).status).toBe(404);
  expect((await s.call(viewer, 'GET', base(file))).status).toBe(404);
});

test('удаление файла убирает всё о комментариях; удалённый человек — автор без имени, его упоминания и прочтения уходят', async () => {
  const { file } = await world();
  const gone = await s.signIn(`c-gone-${crypto.randomUUID()}@example.com`, 'Уходящий');
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, gone.id]);
  const t = await create(gone, file, `Я и @[${owner.id}]`);
  const r = await reply(owner, file, t.thread.id, `@[${gone.id}] спасибо`);
  await s.call(owner, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: true });
  await s.call(gone, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: false });
  await s.call(gone, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: true });
  await s.call(gone, 'PUT', '/api/me/prefs', { commentMail: false });
  await s.pool.query('DELETE FROM "user" WHERE id = $1', [gone.id]);
  const d = await detail(owner, file, t.thread.id);
  expect(d.thread.author).toEqual({ id: null, name: '' });
  expect(d.thread.resolvedBy).toEqual({ id: null, name: '' });
  expect(d.comments[0].author).toEqual({ id: null, name: '' });
  expect(d.comments[1].mentions).toEqual({});
  expect(d.thread.participants).toEqual([owner.id]);
  for (const [table, column] of [
    ['comment_mention', 'user_id'],
    ['comment_read', 'user_id'],
    ['user_pref', 'user_id'],
    ['mail_outbox', 'recipient_id'],
  ])
    expect(await rows(`SELECT count(*)::int AS n FROM ${table} WHERE ${column} = $1`, [gone.id]), table).toEqual([{ n: 0 }]);
  // Комментарий удалённого никто не правит.
  expect((await s.call(owner, 'PATCH', `${base(file)}/${t.thread.id}/comments/${t.comment!.id}`, { body: 'x' })).status).toBe(403);
  expect(r.comment).not.toBeNull();
  // Удаление файла: ни строки.
  await s.pool.query('DELETE FROM file WHERE id = $1', [file]);
  for (const table of ['comment_file', 'comment_thread', 'comment', 'comment_mention', 'comment_read', 'mail_outbox'])
    expect(await rows(`SELECT count(*)::int AS n FROM ${table}`), table).toEqual([{ n: 0 }]);
});

test('настройка писем: по умолчанию включена, своя у каждого', async () => {
  expect(await (await s.call(viewer, 'GET', '/api/me/prefs')).json()).toEqual({ commentMail: true });
  expect((await s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: false })).status).toBe(200);
  expect(await (await s.call(viewer, 'GET', '/api/me/prefs')).json()).toEqual({ commentMail: false });
  expect(await (await s.call(editor, 'GET', '/api/me/prefs')).json()).toEqual({ commentMail: true });
  expect((await s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: 'нет' })).status).toBe(400);
  expect((await s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: true, x: 1 })).status).toBe(400);
});

const outbox = async () =>
  (
    await s.pool.query<{ recipient_id: string; comment_ids: string[]; extra: number; due_at: Date; first_at: Date; state: string }>(
      'SELECT recipient_id, comment_ids, extra, due_at, first_at, state FROM mail_outbox ORDER BY recipient_id',
    )
  ).rows;

test('очередь писем: упомянутые и участники ответа; не автор, не выключившие, не без доступа; слияние и предел', async () => {
  const { file } = await world();
  await s.call(member, 'PUT', '/api/me/prefs', { commentMail: false });
  // Создание: только упомянутые с доступом, кроме себя и выключивших.
  const t = await create(viewer, file, `@[${editor.id}] @[${viewer.id}] @[${member.id}] @[${stranger.id}]`);
  expect((await outbox()).map((o) => [o.recipient_id, o.comment_ids])).toEqual([[editor.id, [t.comment!.id]]]);
  const row = (await outbox())[0];
  // Срок — от первого и через 5 минут.
  expect(row.due_at.getTime() - row.first_at.getTime()).toBe(5 * 60 * 1000);
  // Ответ: участники обсуждения (автор первого и упомянутые), не автор ответа.
  const r = await reply(owner, file, t.thread.id, 'ответ');
  const rowsAfter = await outbox();
  const byUser = Object.fromEntries(rowsAfter.map((o) => [o.recipient_id, o.comment_ids]));
  expect(byUser).toEqual({ [editor.id]: [t.comment!.id, r.comment!.id], [viewer.id]: [r.comment!.id] });
  // Срок ждущей строки не сдвигается новыми комментариями.
  expect(rowsAfter.find((o) => o.recipient_id === editor.id)!.due_at).toEqual(row.due_at);
  // Слияние ограничено: 20 id и счётчик сверху.
  for (let i = 0; i < 25; i++) await reply(owner, file, t.thread.id, `${i}`);
  const merged = (await outbox()).find((o) => o.recipient_id === editor.id)!;
  expect([merged.comment_ids.length, merged.extra]).toEqual([20, 7]);
  // Потерявший доступ писем не получает.
  await s.pool.query('DELETE FROM file_access WHERE user_id = $1', [viewer.id]);
  await s.pool.query('DELETE FROM mail_outbox');
  await reply(owner, file, t.thread.id, 'после снятия');
  expect((await outbox()).map((o) => o.recipient_id)).toEqual([editor.id]);
});

test('очередь писем: ждущих строк у человека не больше 50 — по одной на файл', async () => {
  await world();
  const files: string[] = [];
  for (let i = 0; i < 52; i++) {
    const f = (await s.newFile(owner, `ф${i}`)).id;
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [f, viewer.id]);
    files.push(f);
  }
  for (const f of files)
    await s.pool.query(`SELECT comment_create($1, $2, '1@1', NULL, 0, 0, 0, 0, $3, $4, '{}', 1000)`, [
      f,
      owner.id,
      `@[${viewer.id}]`,
      [viewer.id],
    ]);
  expect((await outbox()).length).toBe(50);
  // В уже ждущую строку новое по-прежнему сливается.
  await s.pool.query(`SELECT comment_create($1, $2, '1@1', NULL, 0, 0, 0, 0, $3, $4, '{}', 1000)`, [
    files[0],
    owner.id,
    `@[${viewer.id}]`,
    [viewer.id],
  ]);
  expect((await outbox()).find((o) => o.comment_ids.length === 2)).toBeDefined();
});

test('запись комментариев и настроек не сверяет роли открытых соединений', async () => {
  const { file } = await world();
  const one = spyOn(s.hub, 'recheckFile');
  const all = spyOn(s.hub, 'recheckAll');
  try {
    const t = await create(owner, file);
    await reply(viewer, file, t.thread.id);
    await s.call(owner, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: true });
    await s.call(viewer, 'POST', `${base(file)}/${t.thread.id}/read`);
    await s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: false });
    expect([one.mock.calls.length, all.mock.calls.length]).toEqual([0, 0]);
    // Сверка по-прежнему идёт после записей, меняющих доступ.
    await s.call(owner, 'PATCH', `/api/files/${file}`, { name: 'Новое' });
    expect(one.mock.calls.length).toBe(1);
  } finally {
    one.mockRestore();
    all.mockRestore();
  }
});
````

В `apps/server/test/migrate.test.ts` заменить:

````ts
    '008_log_kept_bytes.sql',
    '009_content_by.sql',
  ]);
});
````

на:

````ts
    '008_log_kept_bytes.sql',
    '009_content_by.sql',
    '010_comments.sql',
  ]);
});
````

В `apps/server/test/migrate.test.ts` заменить:

````ts
    '008_log_kept_bytes.sql',
    '009_content_by.sql',
  ]);
  expect((await pool.query('SELECT email FROM "user"')).rows).toEqual([{ email: 'ivan@example.com' }]);
````

на:

````ts
    '008_log_kept_bytes.sql',
    '009_content_by.sql',
    '010_comments.sql',
  ]);
  expect((await pool.query('SELECT email FROM "user"')).rows).toEqual([{ email: 'ivan@example.com' }]);
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/comments.test.ts`
Expected: FAIL — 0 pass, 18 fail: `relation "comment_file" does not exist` — миграции и маршрутов ещё нет

- [ ] **Step 3: Реализация**

Создать `apps/server/migrations/010_comments.sql`:

````sql
-- Комментарии (спека подсистемы 4, раздел 10): обсуждения с меткой на холсте, комментарии, упоминания, прочтения, настройка
-- писем и очередь писем. Не в Loro: не шаг отмены, не документ, не экспорт.
--
-- Порядок и согласованность. Каждое изменение комментариев файла поднимает счётчик `comment_file.seq` первым же действием и
-- держит блокировку его строки до фиксации: изменения одного файла идут по одному, номер растёт в порядке фиксации, а у
-- обсуждения хранится номер его последнего изменения. Поэтому `?after=N` не пропускает изменений, а событие с номером N
-- несёт состояние со всеми изменениями до N. Изменения — функции ниже: в одном операторе с CTE всё читается снимком начала
-- оператора, то есть до ожидания блокировки, и ответ на ответ мог бы не видеть одновременного соседа с меньшим номером. В
-- функции каждый оператор после блокировки видит всё зафиксированное до неё. Для сервера это по-прежнему один запрос.
-- Отказ внутри функции (нет обсуждения, нет прав, предел) — исключение с кодом `CM404`/`CM403`/`CM409`: номер не тратится.

CREATE FUNCTION epoch_ms(t timestamptz) RETURNS bigint
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS $$ SELECT floor(extract(epoch FROM t) * 1000)::bigint $$;

-- Счётчик изменений комментариев файла: не в строке `file` — её держат журнал и сжатие.
CREATE TABLE comment_file (
  file_id uuid PRIMARY KEY REFERENCES file ON DELETE CASCADE,
  seq bigint NOT NULL
);

-- Обсуждение удаляется мягко (`deleted_at`): `?after=` отдаёт его надгробием. Комментарии удалённого — удаляются.
CREATE TABLE comment_thread (
  id uuid PRIMARY KEY,
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  page_id text NOT NULL,
  node_id text,
  x double precision NOT NULL,
  y double precision NOT NULL,
  wx double precision NOT NULL,
  wy double precision NOT NULL,
  -- Первый комментарий: его удаление удаляет обсуждение.
  first_id uuid NOT NULL,
  created_by text REFERENCES "user" ON DELETE SET NULL,
  created_at timestamptz NOT NULL,
  resolved_at timestamptz,
  resolved_by text REFERENCES "user" ON DELETE SET NULL,
  seq bigint NOT NULL,
  deleted_at timestamptz,
  -- Сводка для списка (`comment_thread_refresh`, под блокировкой изменения): иначе каждая строка списка собирала бы её из
  -- комментариев, и список 1000 обсуждений стоил бы в разы дороже.
  replies integer NOT NULL DEFAULT 0,
  last_at timestamptz NOT NULL DEFAULT now(),
  participants text[] NOT NULL DEFAULT '{}'
);
-- Список `?after=` по номеру; живые — для первой загрузки без надгробий.
CREATE UNIQUE INDEX comment_thread_seq_idx ON comment_thread (file_id, seq);
CREATE INDEX comment_thread_live_idx ON comment_thread (file_id, seq) WHERE deleted_at IS NULL;
-- Удаление человека обнуляет ссылки: без индексов — полный просмотр таблиц на каждое удаление.
CREATE INDEX comment_thread_created_by_idx ON comment_thread (created_by);
CREATE INDEX comment_thread_resolved_by_idx ON comment_thread (resolved_by);
-- Удалённый человек уходит из участников (`comment_user_gone`).
CREATE INDEX comment_thread_participants_idx ON comment_thread USING gin (participants);

-- Комментарий удаляется сразу, без надгробия: обсуждение приходит событием и `?after=` целиком.
CREATE TABLE comment (
  id uuid PRIMARY KEY,
  thread_id uuid NOT NULL REFERENCES comment_thread ON DELETE CASCADE,
  author_id text REFERENCES "user" ON DELETE SET NULL,
  -- Нормальный текст (`normalizeCommentBody`), до 4000 кодовых точек.
  body text NOT NULL,
  created_at timestamptz NOT NULL,
  edited_at timestamptz
);
CREATE INDEX comment_thread_idx ON comment (thread_id, created_at, id);
CREATE INDEX comment_author_idx ON comment (author_id);

-- Упоминание заводится только для того, у кого был доступ к файлу при отправке.
CREATE TABLE comment_mention (
  comment_id uuid NOT NULL REFERENCES comment ON DELETE CASCADE,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  PRIMARY KEY (comment_id, user_id)
);
CREATE INDEX comment_mention_user_idx ON comment_mention (user_id);

-- Прочтение: обсуждение не прочитано, пока есть чужой комментарий новее `read_at`.
CREATE TABLE comment_read (
  thread_id uuid NOT NULL REFERENCES comment_thread ON DELETE CASCADE,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  read_at timestamptz NOT NULL,
  PRIMARY KEY (thread_id, user_id)
);
CREATE INDEX comment_read_user_idx ON comment_read (user_id);

CREATE TABLE user_pref (
  user_id text PRIMARY KEY REFERENCES "user" ON DELETE CASCADE,
  comment_mail boolean NOT NULL DEFAULT true
);

-- Очередь писем о комментариях (раздел 10.4): письмо не теряется при перезапуске. Одна ждущая строка на получателя и файл:
-- новые комментарии сливаются в неё (не больше 20 id, остальное — счётчиком `extra`), срок — от первого (группировка).
-- `sending` — взято на отправку и зафиксировано до SMTP: строка, найденная в нём после сбоя, не отправляется снова
-- (`uncertain`) — письмо не больше одного раза.
CREATE TABLE mail_outbox (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  recipient_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  comment_ids uuid[] NOT NULL,
  extra integer NOT NULL DEFAULT 0,
  first_at timestamptz NOT NULL,
  due_at timestamptz NOT NULL,
  state text NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'sending', 'sent', 'skipped', 'dropped', 'uncertain')),
  attempts integer NOT NULL DEFAULT 0,
  sending_at timestamptz,
  done_at timestamptz
);
CREATE UNIQUE INDEX mail_outbox_pending_idx ON mail_outbox (recipient_id, file_id) WHERE state = 'pending';
CREATE INDEX mail_outbox_due_idx ON mail_outbox (due_at) WHERE state = 'pending';
CREATE INDEX mail_outbox_sending_idx ON mail_outbox (sending_at) WHERE state = 'sending';
-- Предел писем человеку в час и очистка старых строк.
CREATE INDEX mail_outbox_sent_idx ON mail_outbox (recipient_id, done_at) WHERE state = 'sent';
CREATE INDEX mail_outbox_done_idx ON mail_outbox (done_at) WHERE done_at IS NOT NULL;
CREATE INDEX mail_outbox_file_idx ON mail_outbox (file_id);

-- --- Виды для ответов: JSON прямо из базы, вид — `CommentDto`/`ThreadDto` пакета протокола. ---

CREATE FUNCTION comment_author(u text) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object('id', u, 'name', COALESCE((SELECT name FROM "user" WHERE id = u), ''))
$$;

CREATE FUNCTION comment_json(c comment) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object(
    'id', c.id, 'thread', c.thread_id, 'author', comment_author(c.author_id), 'body', c.body,
    'createdAt', epoch_ms(c.created_at), 'editedAt', epoch_ms(c.edited_at),
    'mentions', COALESCE((SELECT jsonb_object_agg(u.id, u.name) FROM comment_mention m JOIN "user" u ON u.id = m.user_id
                          WHERE m.comment_id = c.id), '{}'::jsonb))
$$;

-- Сводка обсуждения заново: число ответов, время последнего комментария и участники — авторы и упомянутые, по первому
-- появлению (автор комментария раньше упомянутых в нём, упомянутые одним комментарием — по id), не больше 50
-- (`THREAD_PARTICIPANTS_MAX`). Зовут функции изменения после своих записей.
CREATE FUNCTION comment_thread_refresh(p_thread uuid) RETURNS void
LANGUAGE sql AS $$
  UPDATE comment_thread t SET
    replies = GREATEST(a.n - 1, 0),
    last_at = COALESCE(a.last, t.created_at),
    participants = COALESCE((
      SELECT array_agg(p.u ORDER BY p.n) FROM (
        SELECT x.u, row_number() OVER (ORDER BY min(x.at), min(x.k), x.u COLLATE "C") AS n FROM (
          SELECT c.author_id AS u, c.created_at AS at, 0 AS k FROM comment c WHERE c.thread_id = p_thread AND c.author_id IS NOT NULL
          UNION ALL
          SELECT m.user_id, c.created_at, 1 FROM comment c JOIN comment_mention m ON m.comment_id = c.id WHERE c.thread_id = p_thread
        ) x GROUP BY x.u ORDER BY n LIMIT 50
      ) p), '{}')
  FROM (SELECT count(*) AS n, max(created_at) AS last FROM comment WHERE thread_id = p_thread) a
  WHERE t.id = p_thread;
$$;

-- Удалённый человек: его комментарии без автора, упоминания ушли (внешние ключи — раньше этого триггера) — сводки
-- обсуждений, где он участвовал, заново.
CREATE FUNCTION comment_user_gone() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM comment_thread_refresh(id) FROM comment_thread WHERE participants @> ARRAY[OLD.id];
  RETURN NULL;
END
$$;
CREATE TRIGGER comment_user_gone AFTER DELETE ON "user" FOR EACH ROW EXECUTE FUNCTION comment_user_gone();

CREATE FUNCTION comment_thread_json(t comment_thread) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object(
    'id', t.id,
    'pin', jsonb_build_object('page', t.page_id, 'node', t.node_id, 'x', t.x, 'y', t.y, 'wx', t.wx, 'wy', t.wy),
    'author', comment_author(t.created_by),
    'createdAt', epoch_ms(t.created_at),
    'resolvedAt', epoch_ms(t.resolved_at),
    'resolvedBy', CASE WHEN t.resolved_at IS NULL THEN NULL ELSE comment_author(t.resolved_by) END,
    'seq', t.seq,
    'first', (SELECT comment_json(c) FROM comment c WHERE c.id = t.first_id),
    'replies', t.replies,
    'lastAt', epoch_ms(t.last_at),
    'participants', to_jsonb(t.participants))
$$;

-- Не прочитано: есть чужой комментарий новее прочтения. Комментарий удалённого человека — чужой.
CREATE FUNCTION comment_unread(p_thread uuid, p_user text) RETURNS boolean
LANGUAGE sql STABLE AS $$
  SELECT EXISTS (
    SELECT 1 FROM comment c
    WHERE c.thread_id = p_thread AND c.author_id IS DISTINCT FROM p_user
      AND c.created_at > COALESCE((SELECT read_at FROM comment_read r WHERE r.thread_id = p_thread AND r.user_id = p_user), '-infinity'))
$$;

-- Ответ изменения: обсуждение, «не прочитано» для того, кто менял, и изменённый комментарий.
CREATE FUNCTION comment_result(p_thread uuid, p_user text, p_comment uuid, p_removed boolean, p_deleted uuid) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object(
    'seq', t.seq, 'thread', comment_thread_json(t), 'unread', comment_unread(t.id, p_user),
    'comment', (SELECT comment_json(c) FROM comment c WHERE c.id = p_comment),
    'removed', p_removed, 'deleted', p_deleted)
  FROM comment_thread t WHERE t.id = p_thread
$$;

-- --- Изменения ---

-- Следующий номер файла; блокирует строку счётчика до конца транзакции.
CREATE FUNCTION comment_next_seq(p_file uuid) RETURNS bigint
LANGUAGE sql AS $$
  INSERT INTO comment_file AS f (file_id, seq) VALUES (p_file, 1)
  ON CONFLICT (file_id) DO UPDATE SET seq = f.seq + 1
  RETURNING seq
$$;

-- Роль — здесь же, в транзакции изменения: снятие роли между проверкой маршрута и записью не пропускает запись.
CREATE FUNCTION comment_rank(p_file uuid, p_user text) RETURNS integer
LANGUAGE plpgsql AS $$
DECLARE
  v integer := COALESCE(file_role_rank(p_file, p_user), 0);
BEGIN
  IF v < 1 THEN
    RAISE EXCEPTION 'нет доступа к файлу' USING ERRCODE = 'CM404';
  END IF;
  RETURN v;
END
$$;

-- Упоминания комментария: только люди с доступом к файлу сейчас. Уже заведённые (правка) остаются, если токен в тексте.
CREATE FUNCTION comment_mentions(p_file uuid, p_comment uuid, p_users text[]) RETURNS void
LANGUAGE sql AS $$
  DELETE FROM comment_mention WHERE comment_id = p_comment AND user_id <> ALL (p_users);
  INSERT INTO comment_mention (comment_id, user_id)
  SELECT p_comment, u.id FROM "user" u WHERE u.id = ANY (p_users) AND COALESCE(file_role_rank(p_file, u.id), 0) >= 1
  ON CONFLICT DO NOTHING;
$$;

-- Письма о комментарии (раздел 10.4): упомянутые с доступом и, у ответа, участники обсуждения — кроме автора, тех, у кого
-- файл открыт сейчас (`p_open`, соединения сервера), выключивших письма и потерявших доступ. Строка на получателя и файл
-- одна: новое сливается в ждущую, не больше 20 id и счётчик сверху; ждущих строк у человека — не больше 50 (ещё файл
-- сверх этого — без письма). Срок — от первого: группировка за `p_delay_ms`.
CREATE FUNCTION comment_enqueue(p_file uuid, p_thread uuid, p_comment uuid, p_author text, p_reply boolean, p_open text[], p_delay_ms integer)
RETURNS void
LANGUAGE sql AS $$
  INSERT INTO mail_outbox AS o (recipient_id, file_id, comment_ids, first_at, due_at)
  SELECT r.u, p_file, ARRAY[p_comment], clock_timestamp(), clock_timestamp() + make_interval(secs => p_delay_ms / 1000.0)
  FROM (
    SELECT m.user_id AS u FROM comment_mention m WHERE m.comment_id = p_comment
    UNION
    SELECT c.author_id FROM comment c WHERE p_reply AND c.thread_id = p_thread AND c.author_id IS NOT NULL
    UNION
    SELECT m.user_id FROM comment c JOIN comment_mention m ON m.comment_id = c.id WHERE p_reply AND c.thread_id = p_thread
  ) r
  WHERE r.u <> p_author
    AND r.u <> ALL (p_open)
    AND COALESCE((SELECT comment_mail FROM user_pref WHERE user_id = r.u), true)
    AND COALESCE(file_role_rank(p_file, r.u), 0) >= 1
    AND (EXISTS (SELECT 1 FROM mail_outbox q WHERE q.recipient_id = r.u AND q.file_id = p_file AND q.state = 'pending')
         OR (SELECT count(*) FROM mail_outbox q WHERE q.recipient_id = r.u AND q.state = 'pending') < 50)
  ON CONFLICT (recipient_id, file_id) WHERE state = 'pending' DO UPDATE SET
    comment_ids = CASE WHEN cardinality(o.comment_ids) < 20 THEN o.comment_ids || EXCLUDED.comment_ids ELSE o.comment_ids END,
    extra = o.extra + CASE WHEN cardinality(o.comment_ids) < 20 THEN 0 ELSE 1 END;
$$;

CREATE FUNCTION comment_create(
  p_file uuid, p_user text, p_page text, p_node text, p_x float8, p_y float8, p_wx float8, p_wy float8,
  p_body text, p_mentions text[], p_open text[], p_delay_ms integer
) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_thread uuid := gen_random_uuid();
  v_comment uuid := gen_random_uuid();
  v_seq bigint;
  v_at timestamptz;
BEGIN
  PERFORM comment_rank(p_file, p_user);
  v_seq := comment_next_seq(p_file);
  -- После блокировки: время комментариев файла растёт в порядке номеров, прочтение по времени не пропустит соседа.
  v_at := clock_timestamp();
  INSERT INTO comment_thread (id, file_id, page_id, node_id, x, y, wx, wy, first_id, created_by, created_at, seq)
  VALUES (v_thread, p_file, p_page, p_node, p_x, p_y, p_wx, p_wy, v_comment, p_user, v_at, v_seq);
  INSERT INTO comment (id, thread_id, author_id, body, created_at) VALUES (v_comment, v_thread, p_user, p_body, v_at);
  PERFORM comment_mentions(p_file, v_comment, p_mentions);
  PERFORM comment_thread_refresh(v_thread);
  INSERT INTO comment_read (thread_id, user_id, read_at) VALUES (v_thread, p_user, v_at);
  PERFORM comment_enqueue(p_file, v_thread, v_comment, p_user, false, p_open, p_delay_ms);
  RETURN comment_result(v_thread, p_user, v_comment, false, NULL);
END
$$;

CREATE FUNCTION comment_reply(
  p_file uuid, p_user text, p_thread uuid, p_body text, p_mentions text[], p_open text[], p_delay_ms integer, p_max integer
) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_comment uuid := gen_random_uuid();
  v_seq bigint;
  v_at timestamptz;
BEGIN
  PERFORM comment_rank(p_file, p_user);
  v_seq := comment_next_seq(p_file);
  UPDATE comment_thread SET seq = v_seq WHERE id = p_thread AND file_id = p_file AND deleted_at IS NULL;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'нет обсуждения' USING ERRCODE = 'CM404';
  END IF;
  IF (SELECT count(*) FROM comment WHERE thread_id = p_thread) >= p_max THEN
    RAISE EXCEPTION 'обсуждение заполнено' USING ERRCODE = 'CM409';
  END IF;
  v_at := clock_timestamp();
  INSERT INTO comment (id, thread_id, author_id, body, created_at) VALUES (v_comment, p_thread, p_user, p_body, v_at);
  PERFORM comment_mentions(p_file, v_comment, p_mentions);
  PERFORM comment_thread_refresh(p_thread);
  INSERT INTO comment_read (thread_id, user_id, read_at) VALUES (p_thread, p_user, v_at)
  ON CONFLICT (thread_id, user_id) DO UPDATE SET read_at = GREATEST(comment_read.read_at, EXCLUDED.read_at);
  PERFORM comment_enqueue(p_file, p_thread, v_comment, p_user, true, p_open, p_delay_ms);
  RETURN comment_result(p_thread, p_user, v_comment, false, NULL);
END
$$;

-- Правка: только свой комментарий **(поведение, как в Figma: без срока)**. Новые упоминания — с проверкой доступа, писем
-- правка не шлёт.
CREATE FUNCTION comment_edit(p_file uuid, p_user text, p_thread uuid, p_comment uuid, p_body text, p_mentions text[]) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_seq bigint;
  v_author text;
BEGIN
  PERFORM comment_rank(p_file, p_user);
  v_seq := comment_next_seq(p_file);
  SELECT c.author_id INTO v_author FROM comment c JOIN comment_thread t ON t.id = c.thread_id
  WHERE c.id = p_comment AND c.thread_id = p_thread AND t.file_id = p_file AND t.deleted_at IS NULL;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'нет комментария' USING ERRCODE = 'CM404';
  END IF;
  IF v_author IS DISTINCT FROM p_user THEN
    RAISE EXCEPTION 'чужой комментарий' USING ERRCODE = 'CM403';
  END IF;
  UPDATE comment SET body = p_body, edited_at = clock_timestamp() WHERE id = p_comment;
  PERFORM comment_mentions(p_file, p_comment, p_mentions);
  UPDATE comment_thread SET seq = v_seq WHERE id = p_thread;
  PERFORM comment_thread_refresh(p_thread);
  RETURN comment_result(p_thread, p_user, p_comment, false, NULL);
END
$$;

-- Удаление: свой комментарий или любой — владельцу файла. Первый комментарий удаляет обсуждение для всех.
CREATE FUNCTION comment_delete(p_file uuid, p_user text, p_thread uuid, p_comment uuid) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_rank integer;
  v_seq bigint;
  v_author text;
  v_first uuid;
  v_result jsonb;
BEGIN
  v_rank := comment_rank(p_file, p_user);
  v_seq := comment_next_seq(p_file);
  SELECT c.author_id, t.first_id INTO v_author, v_first FROM comment c JOIN comment_thread t ON t.id = c.thread_id
  WHERE c.id = p_comment AND c.thread_id = p_thread AND t.file_id = p_file AND t.deleted_at IS NULL;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'нет комментария' USING ERRCODE = 'CM404';
  END IF;
  IF v_author IS DISTINCT FROM p_user AND v_rank < 3 THEN
    RAISE EXCEPTION 'чужой комментарий' USING ERRCODE = 'CM403';
  END IF;
  IF p_comment = v_first THEN
    UPDATE comment_thread SET seq = v_seq, deleted_at = clock_timestamp() WHERE id = p_thread;
    -- Последнее состояние — до удаления комментариев: ответ несёт обсуждение целиком.
    v_result := comment_result(p_thread, p_user, NULL, true, p_comment);
    DELETE FROM comment WHERE thread_id = p_thread;
    DELETE FROM comment_read WHERE thread_id = p_thread;
    RETURN v_result;
  END IF;
  DELETE FROM comment WHERE id = p_comment;
  UPDATE comment_thread SET seq = v_seq WHERE id = p_thread;
  PERFORM comment_thread_refresh(p_thread);
  RETURN comment_result(p_thread, p_user, NULL, false, p_comment);
END
$$;

-- Закрыть и открыть: автор обсуждения или редактор и выше **(поведение, как в Figma)**.
CREATE FUNCTION comment_resolve(p_file uuid, p_user text, p_thread uuid, p_resolved boolean) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_rank integer;
  v_seq bigint;
  v_author text;
BEGIN
  v_rank := comment_rank(p_file, p_user);
  v_seq := comment_next_seq(p_file);
  SELECT created_by INTO v_author FROM comment_thread WHERE id = p_thread AND file_id = p_file AND deleted_at IS NULL;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'нет обсуждения' USING ERRCODE = 'CM404';
  END IF;
  IF v_author IS DISTINCT FROM p_user AND v_rank < 2 THEN
    RAISE EXCEPTION 'не хватает роли' USING ERRCODE = 'CM403';
  END IF;
  UPDATE comment_thread SET
    seq = v_seq,
    resolved_at = CASE WHEN NOT p_resolved THEN NULL WHEN resolved_at IS NULL THEN clock_timestamp() ELSE resolved_at END,
    resolved_by = CASE WHEN NOT p_resolved THEN NULL WHEN resolved_at IS NULL THEN p_user ELSE resolved_by END
  WHERE id = p_thread;
  RETURN comment_result(p_thread, p_user, NULL, false, NULL);
END
$$;

-- Перенос метки: автор обсуждения или владелец файла (раздел 10.3).
CREATE FUNCTION comment_move(
  p_file uuid, p_user text, p_thread uuid, p_page text, p_node text, p_x float8, p_y float8, p_wx float8, p_wy float8
) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_rank integer;
  v_seq bigint;
  v_author text;
BEGIN
  v_rank := comment_rank(p_file, p_user);
  v_seq := comment_next_seq(p_file);
  SELECT created_by INTO v_author FROM comment_thread WHERE id = p_thread AND file_id = p_file AND deleted_at IS NULL;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'нет обсуждения' USING ERRCODE = 'CM404';
  END IF;
  IF v_author IS DISTINCT FROM p_user AND v_rank < 3 THEN
    RAISE EXCEPTION 'не хватает роли' USING ERRCODE = 'CM403';
  END IF;
  UPDATE comment_thread SET seq = v_seq, page_id = p_page, node_id = p_node, x = p_x, y = p_y, wx = p_wx, wy = p_wy
  WHERE id = p_thread;
  RETURN comment_result(p_thread, p_user, NULL, false, NULL);
END
$$;
````

В `apps/server/src/api.ts` заменить:

````ts
import { originAllowed } from './auth-routes';
import type { BlobStore, UploadPace } from './blob-store';
import type { Mailer } from './mail';
import { valkeyStorage } from './secondary-storage';
````

на:

````ts
import { originAllowed } from './auth-routes';
import type { BlobStore, UploadPace } from './blob-store';
import type { CommentEvent } from '@ferrum/protocol';
import type { Mailer } from './mail';
import { valkeyStorage } from './secondary-storage';
````

В `apps/server/src/api.ts` заменить:

````ts
  /** Попытки открыть файл по ссылке. */
  redeem: { max: 30, windowSeconds: 60 },
} as const;
export type LimitName = keyof typeof LIMITS;
````

на:

````ts
  /** Попытки открыть файл по ссылке. */
  redeem: { max: 30, windowSeconds: 60 },
  /** Новые комментарии и ответы (спека, раздел 10.4): на человека, общий для всех его вкладок и сеансов. */
  comment: { max: 60, windowSeconds: 60 },
  /** Список обсуждений файла: первая загрузка — 1000 обсуждений за две страницы по ~50 мс базы, догоняние — дёшево. Общий
   * `read` (1200 в минуту) дал бы одному человеку занять ядро базы. */
  commentList: { max: 120, windowSeconds: 60 },
} as const;
export type LimitName = keyof typeof LIMITS;
````

В `apps/server/src/api.ts` заменить:

````ts
   * документа, который видят люди, в файл не входит. */
  compactNow?: (file: string, user: string) => Promise<{ ok: true } | { ok: false; retryAfter: number } | { ok: false; incomplete: true }>;
}

````

на:

````ts
   * документа, который видят люди, в файл не входит. */
  compactNow?: (file: string, user: string) => Promise<{ ok: true } | { ok: false; retryAfter: number } | { ok: false; incomplete: true }>;
  /** Комментарии и соединения файла: событие после фиксации изменения и кто держит файл открытым (им письма не нужны).
   * Нет — событий нет, письма всем (тесты маршрутов без концентратора). */
  comments?: { event(file: string, ev: CommentEvent): void; openUsers(file: string): string[] };
  /** Группировка писем о комментариях; по умолчанию — `COMMENT_MAIL_DELAY_MS`. */
  commentMailDelayMs?: number;
}

````

В `apps/server/src/api.ts` заменить:

````ts
export interface Route {
  method: 'GET' | 'HEAD' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  /** `:id`, `:teamId`, `:projectId`, `:entryId` — UUID, `:hash` — blake3: неверный вид — 404. */
  path: string;
  access: Access;
````

на:

````ts
export interface Route {
  method: 'GET' | 'HEAD' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  /** `:id`, `:teamId`, `:projectId`, `:entryId`, `:threadId`, `:commentId` — UUID, `:hash` — blake3: неверный вид — 404. */
  path: string;
  access: Access;
````

В `apps/server/src/api.ts` заменить:

````ts
  /** Тело идёт потоком: обработчик следит за простоем сам, общего срока нет. */
  stream?: boolean;
  handle(ctx: Ctx): Promise<Response>;
}
````

на:

````ts
  /** Тело идёт потоком: обработчик следит за простоем сам, общего срока нет. */
  stream?: boolean;
  /** Удачная запись не меняет доступа к файлам (комментарии, настройки): открытые соединения роль не сверяют. */
  keepsAccess?: boolean;
  handle(ctx: Ctx): Promise<Response>;
}
````

В `apps/server/src/api.ts` заменить:

````ts
}

/** Разбирает JSON не больше `MAX_JSON_BYTES`; иначе и при мусоре — `null`, разбор протокола ответит 400. */
export async function readJson(req: Request): Promise<unknown> {
  if (!req.body) return null;
  const chunks: Uint8Array[] = [];
````

на:

````ts
}

/** Разбирает JSON не больше `max` байт; иначе и при мусоре — `null`, разбор протокола ответит 400. */
export async function readJson(req: Request, max = MAX_JSON_BYTES): Promise<unknown> {
  if (!req.body) return null;
  const chunks: Uint8Array[] = [];
````

В `apps/server/src/api.ts` заменить:

````ts
  for await (const chunk of req.body) {
    size += chunk.length;
    if (size > MAX_JSON_BYTES) return null;
    chunks.push(chunk);
  }
````

на:

````ts
  for await (const chunk of req.body) {
    size += chunk.length;
    if (size > max) return null;
    chunks.push(chunk);
  }
````

В `apps/server/src/api.ts` заменить:

````ts
  hash: isHash,
  doc: isSyncDoc,
};

````

на:

````ts
  hash: isHash,
  doc: isSyncDoc,
  threadId: isUuid,
  commentId: isUuid,
};

````

В `apps/server/src/app.ts` заменить:

````ts
import type { BlobStore, UploadPace } from './blob-store';
import { clientIp, proxyMatcher } from './client-ip';
import { fileRoutes, uploadGate } from './file-routes';
import { imageRoutes } from './image-routes';
````

на:

````ts
import type { BlobStore, UploadPace } from './blob-store';
import { clientIp, proxyMatcher } from './client-ip';
import { commentRoutes } from './comment-routes';
import { fileRoutes, uploadGate } from './file-routes';
import { imageRoutes } from './image-routes';
````

В `apps/server/src/app.ts` заменить:

````ts
  /** Тесты сокращают пределы и сроки API. */
  limits?: Partial<Limits>;
  apiDeadlineMs?: number;
  uploadIdleMs?: number;
````

на:

````ts
  /** Тесты сокращают пределы и сроки API. */
  limits?: Partial<Limits>;
  /** Группировка писем о комментариях: тесты сокращают. */
  commentMailDelayMs?: number;
  apiDeadlineMs?: number;
  uploadIdleMs?: number;
````

В `apps/server/src/app.ts` заменить:

````ts
 * запуски сливаются. */
function accessChanged(hub: SyncHub, route: Route, ctx: Ctx): void {
  if (NO_ACCESS_CHANGE.test(route.path) || route.path === '/api/files' || (route.method === 'POST' && route.path === '/api/teams')) return;
  const files = ctx.touched ?? (ctx.params.id ? [ctx.params.id] : null);
````

на:

````ts
 * запуски сливаются. */
function accessChanged(hub: SyncHub, route: Route, ctx: Ctx): void {
  if (route.keepsAccess) return;
  if (NO_ACCESS_CHANGE.test(route.path) || route.path === '/api/files' || (route.method === 'POST' && route.path === '/api/teams')) return;
  const files = ctx.touched ?? (ctx.params.id ? [ctx.params.id] : null);
````

В `apps/server/src/app.ts` заменить:

````ts
  const api = apiDeps(deps as AppDeps);
  const gate = uploadGate();
  return [...fileRoutes(api, gate), ...imageRoutes(api, gate), ...teamRoutes(api), ...shareRoutes(api)];
}

````

на:

````ts
  const api = apiDeps(deps as AppDeps);
  const gate = uploadGate();
  return [...fileRoutes(api, gate), ...imageRoutes(api, gate), ...teamRoutes(api), ...shareRoutes(api), ...commentRoutes(api)];
}

````

Создать `apps/server/src/comment-routes.ts`:

````ts
// Комментарии (спека подсистемы 4, раздел 10): обсуждения с меткой, ответы, правка и удаление, закрытие и перенос метки,
// прочтение, подсказка упоминаний и настройка писем. Права — по действующей роли: комментировать может каждый с доступом,
// и читатель тоже (раздел 10.3). Каждый обработчик — один запрос к базе: чтение — один `SELECT`, изменение — одна функция
// базы (`010_comments.sql`), которая сама сверяет роль, поднимает номер файла и ставит письма в очередь. id из адреса и тела
// не доверяются: всякий запрос ограничен `file_id` из адреса, чужое обсуждение неотличимо от несуществующего.

import {
  COMMENT_PAGE_MAX,
  MENTION_SUGGEST_MAX,
  mentionIds,
  parseCommentBody,
  parseMentionQuery,
  parseNewThread,
  parsePrefs,
  parseThreadPatch,
  THREAD_COMMENTS_MAX,
  type CommentAction,
  type CommentDto,
  type CommentEvent,
  type CommentListDto,
  type MentionCandidate,
  type ThreadDetailDto,
  type ThreadDto,
  type ThreadView,
} from '@ferrum/protocol';
import { apiError, ok, readJson, type ApiDeps, type Ctx, type Route } from './api';

/** Группировка писем о комментариях: первое ждёт столько, следующие в это окно сливаются в одно письмо (раздел 10.4). */
export const COMMENT_MAIL_DELAY_MS = 5 * 60 * 1000;
/** Тело запроса комментария: 4000 кодовых точек по 4 байта и метка не помещаются в общие 16 КБ. */
const COMMENT_JSON_BYTES = 32 * 1024;

/** Что функция изменения возвращает из базы. */
interface Changed {
  seq: number;
  thread: ThreadDto;
  unread: boolean;
  comment: CommentDto | null;
  removed: boolean;
  deleted: string | null;
}

/** Ошибки функций базы: отказ без записи, номер не потрачен. */
const REFUSALS: Record<string, () => Response> = {
  CM404: () => apiError(404, 'NOT_FOUND'),
  CM403: () => apiError(403, 'FORBIDDEN'),
  CM409: () => apiError(409, 'THREAD_FULL'),
};

function wholeParam(v: string | null, fallback: number): number | null {
  if (v === null) return fallback;
  if (!/^\d{1,16}$/.test(v)) return null;
  const n = Number(v);
  return Number.isSafeInteger(n) ? n : null;
}

/** `%`, `_` и `\` в строке поиска — буквы, а не шаблон: иначе `%` нашёл бы всех. */
const likeEscape = (s: string) => s.replace(/[\\%_]/g, (c) => `\\${c}`);

export function commentRoutes(deps: ApiDeps): Route[] {
  const { pool } = deps;
  const delay = deps.commentMailDelayMs ?? COMMENT_MAIL_DELAY_MS;

  /** Изменение: функция базы, ответ и событие соединениям файла (после фиксации — порядок по номеру держит концентратор). */
  async function change(ctx: Ctx, action: CommentAction, status: number, sql: string, params: unknown[]): Promise<Response> {
    let r: Changed;
    try {
      const { rows } = await pool.query<{ r: Changed }>(`SELECT ${sql} AS r`, params);
      r = rows[0].r;
    } catch (e) {
      const refusal = REFUSALS[(e as { code?: string }).code ?? ''];
      if (refusal) return refusal();
      throw e;
    }
    const file = ctx.params.id;
    const by = ctx.user.id;
    const event: CommentEvent = r.removed
      ? { kind: 'removed', seq: r.seq, by, thread: r.thread.id }
      : { kind: 'thread', seq: r.seq, by, action, thread: r.thread, comment: r.comment, deleted: r.deleted };
    deps.comments?.event(file, event);
    const view: ThreadView = { ...r.thread, unread: r.unread };
    return ok({ seq: r.seq, thread: view, comment: r.comment, removed: r.removed }, status);
  }

  /** Кто сейчас держит файл открытым: им письмо о комментарии не нужно, они видят его сразу. */
  const openUsers = (file: string): string[] => deps.comments?.openUsers(file) ?? [];

  const comments = (path: string) => `/api/files/:id/comments${path}`;

  return [
    {
      method: 'GET',
      path: comments(''),
      access: { kind: 'file', min: 'viewer' },
      limit: 'commentList',
      keepsAccess: true,
      handle: async (ctx) => {
        const after = wholeParam(ctx.url.searchParams.get('after'), 0);
        const limit = wholeParam(ctx.url.searchParams.get('limit'), COMMENT_PAGE_MAX);
        if (after === null || limit === null || limit === 0) return apiError(400, 'BAD_REQUEST');
        const count = Math.min(limit, COMMENT_PAGE_MAX);
        // Один снимок: голова файла и страница согласованы. Первая загрузка (`after=0`) — без надгробий.
        const { rows } = await pool.query<{ id: string | null; seq: string | null; view: ThreadView | null; head: string }>(
          `WITH h AS (SELECT COALESCE((SELECT seq FROM comment_file WHERE file_id = $1), 0) AS head)
           SELECT h.head, p.id, p.seq, p.view FROM h LEFT JOIN LATERAL (
             SELECT t.id, t.seq,
                    CASE WHEN t.deleted_at IS NULL
                      THEN comment_thread_json(t) || jsonb_build_object('unread', comment_unread(t.id, $2)) END AS view
             FROM comment_thread t
             WHERE t.file_id = $1 AND t.seq > $3 AND ($3 > 0 OR t.deleted_at IS NULL)
             ORDER BY t.seq LIMIT $4) p ON true
           ORDER BY p.seq`,
          [ctx.params.id, ctx.user.id, after, count + 1],
        );
        // Пустая страница — одна строка с головой файла и без обсуждения.
        const found = rows.filter((r) => r.id !== null);
        const more = found.length > count;
        const page = found.slice(0, count);
        const body: CommentListDto = {
          seq: Number(more ? page[page.length - 1].seq : rows[0].head),
          threads: page.flatMap((r) => (r.view ? [r.view] : [])),
          removed: page.flatMap((r) => (r.view ? [] : [r.id!])),
          more,
        };
        return ok(body);
      },
    },
    {
      method: 'POST',
      path: comments(''),
      access: { kind: 'file', min: 'viewer' },
      limit: 'comment',
      keepsAccess: true,
      handle: async (ctx) => {
        const body = parseNewThread(await readJson(ctx.req, COMMENT_JSON_BYTES));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { pin } = body;
        const file = ctx.params.id;
        return change(ctx, 'create', 201, 'comment_create($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)', [
          file,
          ctx.user.id,
          pin.page,
          pin.node,
          pin.x,
          pin.y,
          pin.wx,
          pin.wy,
          body.body,
          mentionIds(body.body),
          openUsers(file),
          delay,
        ]);
      },
    },
    {
      method: 'GET',
      path: comments('/:threadId'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'read',
      keepsAccess: true,
      handle: async (ctx) => {
        // Обсуждение другого файла через этот адрес — как несуществующее: условие на `file_id` из адреса.
        const { rows } = await pool.query<ThreadDetailDto>(
          `SELECT comment_thread_json(t) || jsonb_build_object('unread', comment_unread(t.id, $2)) AS thread,
                  (SELECT COALESCE(jsonb_agg(comment_json(c) ORDER BY c.id <> t.first_id, c.created_at, c.id), '[]'::jsonb)
                   FROM comment c WHERE c.thread_id = t.id) AS comments
           FROM comment_thread t WHERE t.id = $3 AND t.file_id = $1 AND t.deleted_at IS NULL`,
          [ctx.params.id, ctx.user.id, ctx.params.threadId],
        );
        return rows[0] ? ok(rows[0]) : apiError(404, 'NOT_FOUND');
      },
    },
    {
      method: 'PATCH',
      path: comments('/:threadId'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      keepsAccess: true,
      handle: async (ctx) => {
        const body = parseThreadPatch(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { id: file, threadId: thread } = ctx.params;
        if ('resolved' in body)
          return change(ctx, body.resolved ? 'resolve' : 'reopen', 200, 'comment_resolve($1, $2, $3, $4)', [
            file,
            ctx.user.id,
            thread,
            body.resolved,
          ]);
        const { pin } = body;
        return change(ctx, 'move', 200, 'comment_move($1, $2, $3, $4, $5, $6, $7, $8, $9)', [
          file,
          ctx.user.id,
          thread,
          pin.page,
          pin.node,
          pin.x,
          pin.y,
          pin.wx,
          pin.wy,
        ]);
      },
    },
    {
      method: 'POST',
      path: comments('/:threadId/comments'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'comment',
      keepsAccess: true,
      handle: async (ctx) => {
        const body = parseCommentBody(await readJson(ctx.req, COMMENT_JSON_BYTES));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const file = ctx.params.id;
        return change(ctx, 'reply', 201, 'comment_reply($1, $2, $3, $4, $5, $6, $7, $8)', [
          file,
          ctx.user.id,
          ctx.params.threadId,
          body.body,
          mentionIds(body.body),
          openUsers(file),
          delay,
          THREAD_COMMENTS_MAX,
        ]);
      },
    },
    {
      method: 'PATCH',
      path: comments('/:threadId/comments/:commentId'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      keepsAccess: true,
      handle: async (ctx) => {
        const body = parseCommentBody(await readJson(ctx.req, COMMENT_JSON_BYTES));
        if (!body) return apiError(400, 'BAD_REQUEST');
        return change(ctx, 'edit', 200, 'comment_edit($1, $2, $3, $4, $5, $6)', [
          ctx.params.id,
          ctx.user.id,
          ctx.params.threadId,
          ctx.params.commentId,
          body.body,
          mentionIds(body.body),
        ]);
      },
    },
    {
      method: 'DELETE',
      path: comments('/:threadId/comments/:commentId'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      keepsAccess: true,
      handle: (ctx) =>
        change(ctx, 'delete', 200, 'comment_delete($1, $2, $3, $4)', [
          ctx.params.id,
          ctx.user.id,
          ctx.params.threadId,
          ctx.params.commentId,
        ]),
    },
    {
      // Открыл обсуждение — прочитал **(поведение)**. Прочитано всё, что видно сейчас: время самого нового комментария, а не
      // `now()` — сосед, ещё не зафиксированный, останется непрочитанным.
      method: 'POST',
      path: comments('/:threadId/read'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      keepsAccess: true,
      handle: async (ctx) => {
        const { rowCount } = await pool.query(
          `INSERT INTO comment_read (thread_id, user_id, read_at)
           SELECT t.id, $2, COALESCE((SELECT max(created_at) FROM comment WHERE thread_id = t.id), now())
           FROM comment_thread t WHERE t.id = $3 AND t.file_id = $1 AND t.deleted_at IS NULL AND file_role_rank($1, $2) >= 1
           ON CONFLICT (thread_id, user_id) DO UPDATE SET read_at = GREATEST(comment_read.read_at, EXCLUDED.read_at)`,
          [ctx.params.id, ctx.user.id, ctx.params.threadId],
        );
        return rowCount ? ok({ ok: true }) : apiError(404, 'NOT_FOUND');
      },
    },
    {
      // Кого можно упомянуть: люди с доступом к файлу (прямой, через команду, по включённой ссылке), кроме себя.
      // **(поведение)** Адрес виден и ищется только участнику файла: опубликованная ссылка адресов не раздаёт, как `GET share`.
      method: 'GET',
      path: '/api/files/:id/mentions',
      access: { kind: 'file', min: 'viewer' },
      limit: 'read',
      keepsAccess: true,
      handle: async (ctx) => {
        const q = parseMentionQuery(ctx.url.searchParams.get('q'));
        if (q === null) return apiError(400, 'BAD_REQUEST');
        const { rows } = await pool.query<MentionCandidate>(
          `WITH me AS (SELECT file_member_rank($1, $2) >= 1 AS member),
                people AS (
                  SELECT user_id FROM file_access WHERE file_id = $1
                  UNION SELECT m.user_id FROM file f JOIN project p ON p.id = f.project_id JOIN team_member m ON m.team_id = p.team_id
                        WHERE f.id = $1
                  UNION SELECT lu.user_id FROM file_link_user lu JOIN share_link l ON l.file_id = lu.file_id AND l.enabled
                        WHERE lu.file_id = $1)
           SELECT u.id, u.name, CASE WHEN me.member THEN u.email END AS email
           FROM people p JOIN "user" u ON u.id = p.user_id, me
           WHERE u.id <> $2 AND file_role_rank($1, u.id) >= 1
             AND ($3 = '' OR u.name ILIKE '%' || $4 || '%' OR (me.member AND u.email ILIKE '%' || $4 || '%'))
           ORDER BY u.name ILIKE $4 || '%' DESC, lower(u.name), u.id
           LIMIT $5`,
          [ctx.params.id, ctx.user.id, q, likeEscape(q), MENTION_SUGGEST_MAX],
        );
        return ok({ people: rows });
      },
    },
    {
      method: 'GET',
      path: '/api/me/prefs',
      access: { kind: 'user' },
      limit: 'read',
      keepsAccess: true,
      handle: async (ctx) => {
        const { rows } = await pool.query<{ comment_mail: boolean }>('SELECT comment_mail FROM user_pref WHERE user_id = $1', [
          ctx.user.id,
        ]);
        return ok({ commentMail: rows[0]?.comment_mail ?? true });
      },
    },
    {
      method: 'PUT',
      path: '/api/me/prefs',
      access: { kind: 'user' },
      limit: 'write',
      keepsAccess: true,
      handle: async (ctx) => {
        const body = parsePrefs(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        await pool.query(
          `INSERT INTO user_pref (user_id, comment_mail) VALUES ($1, $2)
           ON CONFLICT (user_id) DO UPDATE SET comment_mail = EXCLUDED.comment_mail`,
          [ctx.user.id, body.commentMail],
        );
        return ok(body);
      },
    },
  ];
}
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `test:server` — протокол 56 pass, сервер 579 pass, 0 fail

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/migrations/010_comments.sql apps/server/src/api.ts apps/server/src/app.ts apps/server/src/comment-routes.ts apps/server/test/access.test.ts apps/server/test/api-stand.ts apps/server/test/comments.test.ts apps/server/test/migrate.test.ts
git commit -F - <<'EOF'
сервер: комментарии — таблицы, функции изменения и маршруты REST

Миграция 010_comments: comment_file (номер изменений файла), comment_thread (мягкое удаление, первый
комментарий, сводка для списка: число ответов, время последнего, участники), comment, comment_mention,
comment_read, user_pref, mail_outbox (одна ждущая строка на получателя и файл). Изменения — функции
базы: сверяют роль, первым делом поднимают номер файла под блокировкой строки (номер растёт в порядке
фиксации, ответ с номером N видит всё до N), отказ — CM404/CM403/CM409 без траты номера; письма
ставятся в очередь той же транзакцией. Удаление человека пересчитывает сводки (триггер).

comment-routes.ts: список ?after=&limit=, обсуждение, создание, ответ, правка, удаление, закрытие и
перенос метки, прочтение, подсказка упоминаний, настройка писем. Каждый обработчик — один запрос.
LIMITS.comment (60 в минуту на человека) и commentList (120), PARAM_CHECK threadId/commentId,
keepsAccess: записи комментариев не сверяют роли открытых соединений. Таблица прав и тесты
comments.test.ts.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 6: Сервер — живые события по WebSocket файла

Спека §10.4; «Решения» 16. Событие — только соединениям с подтверждённой ролью, после одного запроса ролей на проход; порядок по `seq`; медленному — пропуск и `gap`. Сокет без `hello` файл открытым не делает.

**Files:**
- Modify: `apps/server/src/app.ts`, `apps/server/src/sync/hub.ts`
- Test: `apps/server/test/comment-live.test.ts` (новый)

**Interfaces:**
- Consumes: `ApiDeps.comments` задачи 5, кадр задачи 3.
- Produces: `SyncHub.commentEvent(file: string, ev: CommentEvent): void`, `SyncHub.openUsers(file: string): string[]`, `SyncHub.commentSkipped`; `ConnData.confirmed`, `ConnData.commentGap`; `SyncLimits.commentHighWaterBytes = 64 * 1024`.

- [ ] **Step 1: Написать тесты**

Создать `apps/server/test/comment-live.test.ts`:

````ts
// Живые события комментариев (спека, раздел 10.4): событие `comment` по WebSocket файла — только соединениям с
// подтверждённой ролью и доступом сейчас, в порядке номеров; медленному сокету — пропуск и `gap`, без очереди. Клиент,
// применяющий события по номеру и догоняющий по REST при пропуске, приходит к тому же, что отдаёт REST.

import {
  encodeClientFrame,
  parseCommentList,
  parseCommentWrite,
  parseServerFrame,
  SYNC_PROTOCOL,
  syncPath,
  type CommentEvent,
  type CommentWriteDto,
  type PinDto,
  type ServerFrame,
  type ThreadDto,
} from '@ferrum/protocol';
import { afterAll, beforeEach, expect, test } from 'bun:test';
import { connect as tcp, type Socket } from 'node:net';
import { apiStand, type Person } from './api-stand';
import { Client } from './sync-client';

// Предел частоты комментариев проверяет `comments.test.ts`: здесь нужен поток изменений.
const s = await apiStand({ limits: { comment: { max: 100_000, windowSeconds: 60 }, write: { max: 100_000, windowSeconds: 60 } } });
afterAll(s.stop);
const owner = await s.signIn('l-owner@example.com', 'Ольга');
const editor = await s.signIn('l-editor@example.com', 'Эдуард');
const viewer = await s.signIn('l-viewer@example.com', 'Вера');

let file = '';
let other = '';
beforeEach(async () => {
  await s.reset();
  file = (await s.newFile(owner, 'Файл')).id;
  other = (await s.newFile(owner, 'Другой')).id;
  for (const f of [file, other])
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
      f,
      editor.id,
      viewer.id,
    ]);
});

const pin: PinDto = { page: '3@7910247476534078275', node: null, x: 1, y: 2, wx: 1, wy: 2 };
const base = (f: string) => `/api/files/${f}/comments`;

async function write(res: Response, status = 201): Promise<CommentWriteDto> {
  const text = await res.text();
  expect(res.status, text).toBe(status);
  return parseCommentWrite(JSON.parse(text))!;
}

/** Соединение с подтверждённой ролью. */
async function join(who: Person, f = file): Promise<Client> {
  const c = (await Client.open(s, who, f))!;
  c.hello();
  await c.wait('role');
  return c;
}

const events = (c: Client): CommentEvent[] => c.frames.flatMap((f) => (f.t === 'comment' ? [f.event] : []));
/** Обсуждение без «не прочитано» — как в событии: оно у каждого своё. */
function strip(v: CommentWriteDto['thread']): ThreadDto {
  const t: Partial<CommentWriteDto['thread']> = { ...v };
  delete t.unread;
  return t as ThreadDto;
}

/** Тишина: события, которые могли бы прийти, успели бы. */
const quiet = () => Bun.sleep(300);

test('событие — всем соединениям файла с подтверждённой ролью, и читателю, и вкладке автора; совпадает с ответом REST', async () => {
  const a = await join(owner);
  const a2 = await join(owner);
  const v = await join(viewer);
  const e = await join(editor);
  const elsewhere = await join(editor, other);
  const made = await write(await s.call(viewer, 'POST', base(file), { pin, body: 'Привет' }));
  for (const c of [a, a2, v, e]) {
    const f = await c.wait('comment');
    expect(f.event).toEqual({
      kind: 'thread',
      seq: made.seq,
      by: viewer.id,
      action: 'create',
      thread: strip(made.thread),
      comment: made.comment,
      deleted: null,
    });
  }
  const r = await write(await s.call(editor, 'POST', `${base(file)}/${made.thread.id}/comments`, { body: 'Ответ' }));
  expect((await v.wait('comment', (f) => f.event.seq === r.seq)).event).toMatchObject({ action: 'reply', comment: r.comment });
  const del = await write(await s.call(viewer, 'DELETE', `${base(file)}/${made.thread.id}/comments/${made.comment!.id}`), 200);
  expect((await e.wait('comment', (f) => f.event.seq === del.seq)).event).toEqual({
    kind: 'removed',
    seq: del.seq,
    by: viewer.id,
    thread: made.thread.id,
  });
  // Другой файл не слышит ничего.
  await quiet();
  expect(events(elsewhere)).toEqual([]);
});

test('без `hello` событий нет; после подтверждения роли — есть', async () => {
  const silent = (await Client.open(s, viewer, file))!;
  const a = await join(owner);
  await write(await s.call(owner, 'POST', base(file), { pin, body: 'раз' }));
  await a.wait('comment');
  await quiet();
  expect(events(silent)).toEqual([]);
  silent.hello();
  await silent.wait('role');
  const two = await write(await s.call(owner, 'POST', base(file), { pin, body: 'два' }));
  expect((await silent.wait('comment')).event.seq).toBe(two.seq);
});

test('снятый доступ: событие не уходит и до сверки ролей; вышедший — соединения закрыты', async () => {
  const v = await join(viewer);
  const e = await join(editor);
  // Доступ снят мимо маршрутов: сверка ролей ещё не прошла (раз в 30 с), соединение открыто.
  await s.pool.query('DELETE FROM file_access WHERE file_id = $1 AND user_id = $2', [file, viewer.id]);
  const made = await write(await s.call(owner, 'POST', base(file), { pin, body: `секрет @[${editor.id}]` }));
  await e.wait('comment', (f) => f.event.seq === made.seq);
  await quiet();
  expect(v.closed).toBeNull();
  expect(events(v)).toEqual([]);
  // Выход на этом устройстве: соединения сессии закрыты сразу, событий больше нет.
  const second = await s.signIn(editor.email, 'Эдуард');
  const tab = await join(second);
  expect((await s.call(second, 'POST', '/api/auth/sign-out', {})).status).toBe(200);
  await tab.waitClosed();
  await write(await s.call(owner, 'POST', base(file), { pin, body: 'после выхода' }));
  await quiet();
  expect(events(tab)).toEqual([]);
});

test('файл открыт — письма о комментарии нет; закрыт — есть', async () => {
  const v = await join(viewer);
  await write(await s.call(owner, 'POST', base(file), { pin, body: `@[${viewer.id}] @[${editor.id}]` }));
  const recipients = async () => (await s.pool.query<{ r: string }>('SELECT recipient_id AS r FROM mail_outbox')).rows.map((x) => x.r);
  expect(await recipients()).toEqual([editor.id]);
  v.close();
  await v.waitClosed();
  for (let i = 0; i < 50 && s.hub.openUsers(file).includes(viewer.id); i++) await Bun.sleep(10);
  await write(await s.call(owner, 'POST', base(file), { pin, body: `@[${viewer.id}]` }));
  expect((await recipients()).sort()).toEqual([editor.id, viewer.id].sort());
  // Неподтверждённое соединение файл открытым не делает.
  await s.pool.query('DELETE FROM mail_outbox');
  const silent = (await Client.open(s, editor, file))!;
  await write(await s.call(owner, 'POST', base(file), { pin, body: `@[${editor.id}]` }));
  expect(await recipients()).toEqual([editor.id]);
  silent.close();
});

/** Клиент по правилам протокола: событие с номером «известный + 1» применяется, больший номер или `gap` — догнать по REST
 * `?after=известный`, меньший — уже учтён. Итог должен совпасть с первой загрузкой. */
async function replay(who: Person, evs: CommentEvent[]) {
  let known = 0;
  const threads = new Map<string, ThreadDto>();
  let refetches = 0;
  const refetch = async () => {
    refetches++;
    for (let more = true; more;) {
      const l = parseCommentList(await (await s.call(who, 'GET', `${base(file)}?after=${known}`)).json())!;
      for (const t of l.threads) threads.set(t.id, strip(t));
      for (const id of l.removed) threads.delete(id);
      known = l.seq;
      more = l.more;
    }
  };
  for (const ev of evs) {
    if (ev.seq <= known) continue;
    if (ev.kind === 'gap' || ev.seq !== known + 1) {
      await refetch();
      continue;
    }
    if (ev.kind === 'thread') threads.set(ev.thread.id, ev.thread);
    else threads.delete(ev.thread);
    known = ev.seq;
  }
  return { known, threads, refetches };
}

test('порядок: события одного файла приходят по возрастанию номера; клиент по ним приходит к тому же, что REST', async () => {
  const c = await join(viewer);
  const t = await write(await s.call(owner, 'POST', base(file), { pin, body: 'начало' }));
  const people = [owner, editor, viewer];
  // Ответы, правки, закрытия и удаления разом, из разных рук.
  const work: Promise<unknown>[] = [];
  for (let i = 0; i < 40; i++) {
    const who = people[i % 3];
    work.push(
      (async () => {
        const r = await write(await s.call(who, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: `о${i}` }));
        if (i % 4 === 0) await s.call(who, 'PATCH', `${base(file)}/${t.thread.id}/comments/${r.comment!.id}`, { body: `правка ${i}` });
        if (i % 5 === 0) await s.call(who, 'DELETE', `${base(file)}/${t.thread.id}/comments/${r.comment!.id}`);
        if (i % 7 === 0) await s.call(owner, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: i % 2 === 0 });
      })(),
    );
  }
  for (let i = 0; i < 10; i++) work.push(s.call(people[i % 3], 'POST', base(file), { pin, body: `новое ${i}` }));
  await Promise.all(work);
  const head = parseCommentList(await (await s.call(owner, 'GET', base(file))).json())!;
  await c.wait('comment', (f) => f.event.seq === head.seq);
  const evs = events(c);
  const seqs = evs.map((e) => e.seq);
  // Строго по возрастанию: опоздавшее не шлётся после большего.
  expect(seqs).toEqual([...seqs].sort((a, b) => a - b));
  expect(new Set(seqs).size).toBe(seqs.length);
  const got = await replay(viewer, evs);
  console.log('события комментариев: получено', evs.length, 'из', head.seq, ', догоняний по REST', got.refetches);
  expect(got.known).toBe(head.seq);
  expect(new Map([...got.threads].sort())).toEqual(new Map(head.threads.map((x) => [x.id, strip(x)] as const).sort()));
});

/** Сокет, который не читает: настоящий TCP с WebSocket поверх, кадры разбираются вручную. */
async function rawSocket(who: Person): Promise<{ raw: Socket; frames: () => ServerFrame[] }> {
  const raw = tcp(Number(new URL(s.publicUrl).port), '127.0.0.1');
  const got: Buffer[] = [];
  raw.on('data', (b: Buffer) => got.push(b));
  raw.write(
    `GET ${syncPath(file, who.id)} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
      `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: ${s.publicUrl}\r\nCookie: ${who.cookie}\r\n\r\n`,
  );
  // Кадр — после ответа на открытие: раньше сервер его не читает.
  await Bun.sleep(200);
  const hello = encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 });
  raw.write(Buffer.concat([Buffer.from([0x82, 0x80 | hello.length, 0, 0, 0, 0]), hello]));
  const frames = (): ServerFrame[] => {
    const all = Buffer.concat(got);
    let at = all.indexOf('\r\n\r\n') + 4;
    const out: ServerFrame[] = [];
    while (at + 2 <= all.length) {
      const op = all[at] & 0x0f;
      let len = all[at + 1] & 0x7f;
      let head = 2;
      if (len === 126) {
        len = all.readUInt16BE(at + 2);
        head = 4;
      } else if (len === 127) {
        len = Number(all.readBigUInt64BE(at + 2));
        head = 10;
      }
      if (at + head + len > all.length) break;
      if (op === 2) {
        const f = parseServerFrame(new Uint8Array(all.subarray(at + head, at + head + len)));
        if (!f) throw new Error('неверный кадр сервера');
        out.push(f);
      }
      at += head + len;
    }
    return out;
  };
  return { raw, frames };
}

test('медленный читатель: события пропускаются без очереди, буфер сервера не растёт; после чтения — `gap` с головой файла', async () => {
  const { raw, frames } = await rawSocket(viewer);
  for (let i = 0; i < 100 && !frames().some((f) => f.t === 'role'); i++) await Bun.sleep(10);
  expect(frames().some((f) => f.t === 'role')).toBe(true);
  raw.pause();
  const fast = await join(editor);
  // Крупные события (первый комментарий и ответ — по 4000 четырёхбайтных знаков, ~32 КБ на событие): буфер ядра ОС
  // заполняется, дальше копит сервер — если не пропускает.
  const big = '😀'.repeat(4000);
  const t = await write(await s.call(owner, 'POST', base(file), { pin, body: big }));
  const skipped0 = s.hub.commentSkipped;
  let peak = 0;
  let last = t;
  for (let i = 0; i < 400 && s.hub.commentSkipped - skipped0 < 20; i++) {
    last = await write(await s.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: big }));
    peak = Math.max(peak, s.hub.bufferedBytes());
  }
  for (let i = 0; i < 20; i++) {
    last = await write(await s.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: big }));
    peak = Math.max(peak, s.hub.bufferedBytes());
  }
  await fast.wait('comment', (f) => f.event.seq === last.seq);
  console.log('медленный читатель комментариев', { skipped: s.hub.commentSkipped - skipped0, peakBufferedKb: peak / 1024 });
  expect(s.hub.commentSkipped - skipped0).toBeGreaterThanOrEqual(20);
  // Буфер — до порога и одного события (64 КБ кадр), сколько бы событий ни было.
  expect(peak).toBeLessThan(64 * 1024 + 64 * 1024 + 4096);
  // Быстрый получил всё подряд.
  const fastSeqs = events(fast).map((e) => e.seq);
  expect(fastSeqs.at(-1)).toBe(last.seq);
  expect(fastSeqs).toEqual(Array.from({ length: fastSeqs.length }, (_, i) => fastSeqs[0] + i));
  raw.resume();
  let gap: CommentEvent | undefined;
  for (let i = 0; i < 300 && !gap; i++) {
    await Bun.sleep(20);
    gap = frames()
      .flatMap((f) => (f.t === 'comment' ? [f.event] : []))
      .find((e) => e.kind === 'gap');
  }
  expect(gap).toEqual({ kind: 'gap', seq: last.seq });
  raw.destroy();
}, 120_000);
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/comment-live.test.ts`
Expected: FAIL — 0 pass, 6 fail: пять тестов ждут кадр `comment` и получают только `role` («нет кадра comment»), шестой — строка письма заводится и тому, у кого файл открыт (`openUsers` не передан)

- [ ] **Step 3: Реализация**

В `apps/server/src/app.ts` заменить:

````ts
    contentReplaced: (file) => hub.forgetFile(file),
    compactNow: (file, user) => (compactor ? compactor.manual(file, user) : Promise.resolve({ ok: true as const })),
  };
  const handleApi = apiHandler(apiRoutes(api), api);
````

на:

````ts
    contentReplaced: (file) => hub.forgetFile(file),
    compactNow: (file, user) => (compactor ? compactor.manual(file, user) : Promise.resolve({ ok: true as const })),
    comments: { event: (file, ev) => hub.commentEvent(file, ev), openUsers: (file) => hub.openUsers(file) },
  };
  const handleApi = apiHandler(apiRoutes(api), api);
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  SYNC_PROTOCOL,
  versionIncludes,
  type FileRole,
  type RevokeReason,
````

на:

````ts
  SYNC_PROTOCOL,
  versionIncludes,
  type CommentEvent,
  type FileRole,
  type RevokeReason,
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  /** Имя человека для присутствия: из базы при `hello`, не из кадров клиента. */
  name: string;
}

````

на:

````ts
  /** Имя человека для присутствия: из базы при `hello`, не из кадров клиента. */
  name: string;
  /** Роль подтверждена запросом `hello`: соединение получает события комментариев и считается «файл открыт» для писем. */
  confirmed: boolean;
  /** Пропущено событие комментариев (сокет не успевал): при освобождении — `gap`, клиент догонит по REST. */
  commentGap: boolean;
}

````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  log: Partial<LogLimits>;
  presence: Partial<PresenceLimits>;
}

````

на:

````ts
  log: Partial<LogLimits>;
  presence: Partial<PresenceLimits>;
  /** Буфер сокета, выше которого событие комментариев пропускается (потом — `gap`): очереди на соединение нет. */
  commentHighWaterBytes: number;
}

````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  log: {},
  presence: {},
};

/** Столько байт записей журнала за один запрос догоняния. */
const PUMP_BYTES = 8 * 1024 * 1024;
````

на:

````ts
  log: {},
  presence: {},
  commentHighWaterBytes: 64 * 1024,
};

/** Сколько событие комментариев ждёт соседа с меньшим номером, и сколько таких ждёт разом на файл. */
const COMMENT_REORDER_MS = 50;
const COMMENT_EARLY_MAX = 256;
/** Событий в очереди рассылки файла, пока идёт проход: дальше — `gap` всем. */
const COMMENT_QUEUE_MAX = 1024;
/** Столько байт записей журнала за один запрос догоняния. */
const PUMP_BYTES = 8 * 1024 * 1024;
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  private rechecking: Promise<void> | null = null;
  private recheckAgain = false;
  /** Замер §13.1: от прихода кадра до конца его рассылки, мс. */
  onTiming: ((ms: number) => void) | null = null;
````

на:

````ts
  private rechecking: Promise<void> | null = null;
  private recheckAgain = false;
  /** Комментарии (раздел 10.4): очередь рассылки каждого файла (рассылает один проход за раз, всё накопленное — одним
   * запросом ролей) и последний разосланный номер файла (событие с номером не больше уже не нужно: клиент, получивший
   * больший, догоняет по REST). */
  private readonly commentRooms = new Map<string, { queue: CommentEvent[]; busy: boolean; overflow: boolean }>();
  private readonly commentHeads = new Map<string, number>();
  /** Перестановка: последний поставленный в цепочку номер файла и пришедшие раньше своей очереди. Ответы REST о соседних
   * изменениях приходят сюда не по порядку фиксации; подождать соседа дешевле, чем каждому клиенту догонять по REST. */
  private readonly commentQueued = new Map<string, number>();
  private readonly commentEarly = new Map<string, { evs: CommentEvent[]; timer: Timer }>();
  /** Пропущено событий комментариев медленным соединениям: тесты и проба. */
  commentSkipped = 0;
  /** Замер §13.1: от прихода кадра до конца его рассылки, мс. */
  onTiming: ((ms: number) => void) | null = null;
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
      lagSince: 0,
      name: '',
    };
    if (!server.upgrade(req, { data })) return new Response(null, { status: 400 });
````

на:

````ts
      lagSince: 0,
      name: '',
      confirmed: false,
      commentGap: false,
    };
    if (!server.upgrade(req, { data })) return new Response(null, { status: 400 });
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
        ws.data.lagSince = 0;
        for (const w of ws.data.drained.splice(0)) w();
      });
    },
````

на:

````ts
        ws.data.lagSince = 0;
        for (const w of ws.data.drained.splice(0)) w();
        this.commentDrained(ws.data);
      });
    },
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
    if (set?.size === 0) {
      this.conns.delete(d.file);
      this.onLastLeft(d.file);
    }
````

на:

````ts
    if (set?.size === 0) {
      this.conns.delete(d.file);
      this.commentHeads.delete(d.file);
      this.commentQueued.delete(d.file);
      this.onLastLeft(d.file);
    }
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
          d.name = rows[0].name ?? '';
          this.send(d, { t: 'role', role, canShare: rows[0].member >= ROLE_RANK.editor, version: Number(rows[0].version) });
          // Присутствие файла — только после роли, подтверждённой этим запросом: снятый доступ его не увидит.
          this.presence.admit(d);
        })
````

на:

````ts
          d.name = rows[0].name ?? '';
          this.send(d, { t: 'role', role, canShare: rows[0].member >= ROLE_RANK.editor, version: Number(rows[0].version) });
          // Присутствие и комментарии файла — только после роли, подтверждённой этим запросом: снятый доступ их не увидит.
          d.confirmed = true;
          this.presence.admit(d);
        })
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  }

  /** Открыт ли файл у кого-нибудь. */
  isOpen(file: string): boolean {
````

на:

````ts
  }

  /** Событие комментариев файла (раздел 10.4): после фиксации изменения, из маршрута. Уходит соединениям с подтверждённой
   * ролью, у кого доступ есть сейчас: запрос ролей на проход рассылки — снятый доступ не получает события и до сверки
   * ролей. События файла рассылаются по возрастанию номера; пока идёт проход, следующие копятся и уходят следующим проходом
   * с одним запросом ролей на всех: медленная база не выстраивает очередь из запросов. Опоздавшее (номер не больше
   * разосланного) не шлётся — клиент, получивший больший номер с пропуском, догоняет по REST. Медленному сокету событие не
   * копится: пропуск и потом `gap`. */
  commentEvent(file: string, ev: CommentEvent): void {
    // Файл никто не держит открытым — слать некому, и порядок помнить незачем.
    if (this.stopped || !this.conns.has(file)) return;
    const queued = this.commentQueued.get(file);
    if (queued !== undefined && ev.seq > queued + 1) {
      // Раньше соседа: ждать его не дольше `COMMENT_REORDER_MS`, потом — как есть (клиент догонит по REST).
      let early = this.commentEarly.get(file);
      if (!early) {
        const timer = setTimeout(() => this.flushEarly(file, true), COMMENT_REORDER_MS);
        timer.unref?.();
        this.commentEarly.set(file, (early = { evs: [], timer }));
      }
      early.evs.push(ev);
      early.evs.sort((a, b) => a.seq - b.seq);
      if (early.evs.length >= COMMENT_EARLY_MAX) this.flushEarly(file, true);
      return;
    }
    this.queueComment(file, ev);
    this.flushEarly(file, false);
  }

  /** Ждущие своей очереди: подряд идущие — сейчас; `all` — все (срок вышел или их слишком много). */
  private flushEarly(file: string, all: boolean): void {
    const early = this.commentEarly.get(file);
    if (!early) return;
    while (early.evs.length > 0 && (all || early.evs[0].seq <= (this.commentQueued.get(file) ?? 0) + 1))
      this.queueComment(file, early.evs.shift()!);
    if (early.evs.length === 0) {
      clearTimeout(early.timer);
      this.commentEarly.delete(file);
    }
  }

  private queueComment(file: string, ev: CommentEvent): void {
    this.commentQueued.set(file, Math.max(ev.seq, this.commentQueued.get(file) ?? 0));
    let room = this.commentRooms.get(file);
    if (!room) this.commentRooms.set(file, (room = { queue: [], busy: false, overflow: false }));
    // Предел очереди файла (база зависла, событий поток): дальше — только последний номер, всем — `gap`.
    if (room.queue.length >= COMMENT_QUEUE_MAX) {
      room.overflow = true;
      room.queue = [];
    }
    room.queue.push(ev);
    if (room.busy) return;
    room.busy = true;
    const work = this.pumpComments(file, room);
    this.track(work);
  }

  private async pumpComments(file: string, room: { queue: CommentEvent[]; busy: boolean; overflow: boolean }): Promise<void> {
    try {
      while (room.queue.length > 0 && !this.stopped) {
        const batch = room.queue.splice(0);
        const overflow = room.overflow;
        room.overflow = false;
        await this.fanOutComments(file, batch, overflow).catch(report);
      }
    } finally {
      room.busy = false;
      if (room.queue.length === 0 && this.commentRooms.get(file) === room) this.commentRooms.delete(file);
    }
  }

  private async fanOutComments(file: string, batch: CommentEvent[], overflow: boolean): Promise<void> {
    const head = this.commentHeads.get(file) ?? 0;
    const evs = batch.filter((e) => e.seq > head).sort((a, b) => a.seq - b.seq);
    if (evs.length === 0) return;
    const targets = [...(this.conns.get(file) ?? [])].filter((d) => d.confirmed && !d.closed);
    if (targets.length === 0) return;
    const last = evs[evs.length - 1].seq;
    this.commentHeads.set(file, last);
    const users = [...new Set(targets.map((d) => d.user))];
    const { rows } = await this.pool.query<{ u: string; rank: number | null }>(
      'SELECT u, file_role_rank($1, u) AS rank FROM unnest($2::text[]) AS u',
      [file, users],
    );
    const allowed = new Set(rows.filter((r) => (r.rank ?? 0) > 0).map((r) => r.u));
    const frames = evs.map((ev) => encodeServerFrame({ t: 'comment', event: ev }));
    for (const d of targets) {
      if (d.closed || !allowed.has(d.user)) continue;
      // Очередь переполнялась — пропущено у всех: догнать по REST.
      if (overflow) d.commentGap = true;
      for (const [i, frame] of frames.entries()) {
        if (this.commentSlow(d)) {
          d.commentGap = true;
          this.commentSkipped += frames.length - i;
          break;
        }
        if (d.commentGap) {
          // Пропуск был: это событие и следующие клиент получит вместе с пропущенными, догоняя по REST.
          d.commentGap = false;
          this.send(d, { t: 'comment', event: { kind: 'gap', seq: last } });
          break;
        }
        this.sendBytes(d, frame);
      }
    }
  }

  private commentSlow(d: ConnData): boolean {
    return !!d.ws && d.ws.getBufferedAmount() > this.limits.commentHighWaterBytes;
  }

  /** Сокет освободился: пропустившему события — `gap` с последним номером файла. */
  private commentDrained(d: ConnData): void {
    if (!d.commentGap || d.closed || this.commentSlow(d)) return;
    d.commentGap = false;
    this.send(d, { t: 'comment', event: { kind: 'gap', seq: this.commentHeads.get(d.file) ?? 0 } });
  }

  /** Кто держит файл открытым: соединения с подтверждённой ролью. Им письма о комментариях не нужны. */
  openUsers(file: string): string[] {
    const out = new Set<string>();
    for (const d of this.conns.get(file) ?? []) if (d.confirmed && !d.closed) out.add(d.user);
    return [...out];
  }

  /** Открыт ли файл у кого-нибудь. */
  isOpen(file: string): boolean {
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  async stop(): Promise<void> {
    this.stopped = true;
    for (const t of this.timers) clearInterval(t);
    this.presence.stop();
````

на:

````ts
  async stop(): Promise<void> {
    this.stopped = true;
    for (const early of this.commentEarly.values()) clearTimeout(early.timer);
    this.commentEarly.clear();
    for (const t of this.timers) clearInterval(t);
    this.presence.stop();
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `test:server` — протокол 56 pass, сервер 585 pass, 0 fail

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/src/app.ts apps/server/src/sync/hub.ts apps/server/test/comment-live.test.ts
git commit -F - <<'EOF'
сервер: комментарии — живые события по WebSocket файла

SyncHub.commentEvent: событие comment уходит только соединениям с ролью, подтверждённой hello, и после
запроса ролей на проход рассылки (снятый доступ не получает его и до сверки ролей). Рассылка файла —
по одному проходу за раз: пока он идёт, события копятся и уходят следующим проходом с одним запросом
ролей на всех (медленная база не выстраивает очередь из запросов; не больше 1024 — дальше gap).
Пришедшее раньше соседа ждёт его до 50 мс, опоздавшее (номер не больше разосланного) не шлётся.
Медленному сокету (буфер > 64 КБ) событие пропускается без очереди, на drain или следующем событии —
gap с головой файла. openUsers: кому не слать писем. Маршруты получают оба через ApiDeps.comments.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 7: Сервер — отправка очереди писем `mail_outbox`

Спека §10.4; «Решения» 17, 21, 22. Группировка, повторная проверка при отправке (файл открыт, доступ снят, письма выключены, комментарии удалены), не больше одного раза после сбоя, тема одной строкой, HTML экранирован; одно настоящее письмо — через Mailpit стенда.

**Files:**
- Create: `apps/server/src/comment-mail.ts`
- Modify: `apps/server/src/app.ts`, `apps/server/src/index.ts`
- Test: `apps/server/test/api-stand.ts`, `apps/server/test/comment-mail.test.ts` (новый)

**Interfaces:**
- Consumes: строки `mail_outbox` задачи 5, `hub.openUsers` задачи 6.
- Produces: `CommentMailWorker` (`start()`, `tick()`, `stop()`, `counts`), `CommentMailOptions`, `COMMENT_MAIL_OPTIONS`, `knownSmtpFailure`, `headerSafe`, `commentMail`; `AppDeps.commentMail?: Partial<CommentMailOptions> | false`, `Running.commentMail: CommentMailWorker | null`.

- [ ] **Step 1: Написать тесты**

В `apps/server/test/api-stand.ts` заменить:

````ts
  const stop = async () => {
    server.compactor?.stop();
    await server.hub.stop();
    server.stop(true);
````

на:

````ts
  const stop = async () => {
    server.compactor?.stop();
    await server.commentMail?.stop();
    await server.hub.stop();
    server.stop(true);
````

Создать `apps/server/test/comment-mail.test.ts`:

````ts
// Письма о комментариях (спека, раздел 10.4): группировка, кому не слать, отказы SMTP, не больше одного раза при падении и
// перезапуске, безопасность письма. Почта — поддельная, но настоящая по поведению: отказывает по заказу, зависает, считает
// попытки. Последний тест шлёт настоящее письмо через nodemailer в Mailpit стенда и читает его через API Mailpit.

import { parseCommentWrite, type CommentWriteDto, type PinDto } from '@ferrum/protocol';
import { afterAll, afterEach, beforeEach, expect, test } from 'bun:test';
import { CommentMailWorker, type CommentMailOptions } from '../src/comment-mail';
import { smtpMailer, type Mail, type Mailer } from '../src/mail';
import { apiStand, type Person } from './api-stand';
import { Client } from './sync-client';

// Своя отправка стенда выключена: тесты гоняют свою, с поддельной почтой. Группировка — минута: «срок вышел» тест делает сам.
const s = await apiStand({ commentMail: false, commentMailDelayMs: 60_000 });
afterAll(s.stop);
const owner = await s.signIn('m-owner@example.com', 'Ольга');
const editor = await s.signIn('m-editor@example.com', 'Эдуард');
const viewer = await s.signIn('m-viewer@example.com', 'Вера');

/** Почта, которая отказывает по заказу и может зависнуть: каждое обращение — попытка. */
class FakeMail implements Mailer {
  readonly attempts: Mail[] = [];
  readonly delivered: Mail[] = [];
  readonly failures: unknown[] = [];
  hang: Promise<void> | null = null;
  async send(mail: Mail): Promise<void> {
    this.attempts.push(mail);
    if (this.hang) await this.hang;
    const e = this.failures.shift();
    if (e) throw e;
    this.delivered.push(mail);
  }
}

const workers: CommentMailWorker[] = [];
async function worker(mail: Mailer, opts: Partial<CommentMailOptions> = {}): Promise<CommentMailWorker> {
  const w = new CommentMailWorker(s.pool, mail, s.publicUrl, (f) => s.hub.openUsers(f), [], {
    pollMs: 3_600_000,
    retryMs: 0,
    ...opts,
  });
  await w.start();
  workers.push(w);
  return w;
}

let file = '';
beforeEach(async () => {
  await s.reset();
  for (const key of (await s.valkey.send('KEYS', ['api:*'])) as string[]) await s.valkey.del(key);
  file = (await s.newFile(owner, 'Макет')).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
    file,
    editor.id,
    viewer.id,
  ]);
});
afterEach(async () => {
  for (const w of workers.splice(0)) await w.stop();
});

const pin: PinDto = { page: '3@7910247476534078275', node: null, x: 1, y: 2, wx: 1, wy: 2 };
const base = (f: string) => `/api/files/${f}/comments`;
async function write(res: Response): Promise<CommentWriteDto> {
  const text = await res.text();
  expect(res.status, text).toBeLessThan(300);
  return parseCommentWrite(JSON.parse(text))!;
}
const create = async (who: Person, body: string, f = file) => write(await s.call(who, 'POST', base(f), { pin, body }));
const reply = async (who: Person, thread: string, body: string) =>
  write(await s.call(who, 'POST', `${base(file)}/${thread}/comments`, { body }));
/** Срок группировки вышел. */
const due = () => s.pool.query(`UPDATE mail_outbox SET due_at = now() - interval '1 second' WHERE state = 'pending'`);
const states = async () =>
  (await s.pool.query<{ state: string; attempts: number }>('SELECT state, attempts FROM mail_outbox ORDER BY id')).rows.map(
    (r) => `${r.state}/${r.attempts}`,
  );

test('группировка: комментарии за окно — одно письмо со ссылками на обсуждения; до срока — ничего; второй проход — ничего', async () => {
  const mail = new FakeMail();
  const w = await worker(mail);
  const a = await create(owner, `@[${viewer.id}] посмотри шапку`);
  const b = await create(editor, `И подвал, @[${viewer.id}]`);
  await reply(owner, a.thread.id, 'уточнение');
  await w.tick();
  expect(mail.attempts).toHaveLength(0);
  await due();
  await w.tick();
  await w.tick();
  expect(mail.delivered.map((m) => m.to)).toEqual([viewer.email]);
  // Ответ владельца в его же обсуждении — упомянутой там Вере; редактор в нём не участвует.
  const m = mail.delivered[0];
  expect(m.subject).toBe('Новые комментарии в «Макет»: 3');
  for (const t of [a.thread.id, b.thread.id]) expect(m.text).toContain(`${s.publicUrl}/#/file/${file}?comment=${t}`);
  expect(m.text).toContain('@Вера посмотри шапку');
  expect(await states()).toEqual(['sent/1']);
});

test('не шлётся: файл открыт сейчас, открыт после первого комментария, доступ снят, письма выключены, комментарии удалены', async () => {
  const mail = new FakeMail();
  const w = await worker(mail);
  const cases: [string, () => Promise<unknown>][] = [
    [
      'открыт сейчас',
      async () => {
        const c = (await Client.open(s, viewer, file))!;
        c.hello();
        await c.wait('role');
        return c;
      },
    ],
    ['открыт после', () => s.call(viewer, 'POST', `/api/files/${file}/touch`)],
    ['доступ снят', () => s.pool.query('DELETE FROM file_access WHERE user_id = $1', [viewer.id])],
    ['письма выключены', () => s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: false })],
    [
      'удалено',
      async () => {
        const t = (await s.pool.query<{ id: string; first_id: string }>('SELECT id, first_id FROM comment_thread')).rows[0];
        return s.call(owner, 'DELETE', `${base(file)}/${t.id}/comments/${t.first_id}`);
      },
    ],
  ];
  for (const [name, condition] of cases) {
    await s.pool.query('DELETE FROM comment_thread');
    await s.pool.query('DELETE FROM mail_outbox');
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer') ON CONFLICT DO NOTHING`, [
      file,
      viewer.id,
    ]);
    await s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: true });
    await create(owner, `@[${viewer.id}] ${name}`);
    expect(await states(), name).toEqual(['pending/0']);
    const held = await condition();
    await due();
    await w.tick();
    expect(await states(), name).toEqual(['skipped/1']);
    if (held instanceof Client) {
      held.close();
      await held.waitClosed();
    }
  }
  expect(mail.attempts).toHaveLength(0);
  expect(w.counts.skipped).toBe(cases.length);
});

test('отказ SMTP: повтор с паузой; после 5 попыток — брошено; обрыв посреди передачи и зависание — без повтора', async () => {
  const mail = new FakeMail();
  const w = await worker(mail, { sendTimeoutMs: 300 });
  await create(owner, `@[${viewer.id}] раз`);
  await due();
  // Сервер ответил кодом: письмо точно не ушло — повтор.
  mail.failures.push(Object.assign(new Error('451'), { responseCode: 451, command: 'DATA' }));
  await w.tick();
  expect(await states()).toEqual(['pending/1']);
  await due();
  await w.tick();
  expect(await states()).toEqual(['sent/2']);
  expect([mail.attempts.length, mail.delivered.length]).toEqual([2, 1]);

  // Соединение не открылось пять раз подряд — брошено.
  await s.pool.query('DELETE FROM mail_outbox');
  await create(owner, `@[${viewer.id}] два`);
  for (let i = 0; i < 5; i++) {
    mail.failures.push(Object.assign(new Error('refused'), { code: 'ESOCKET', command: 'CONN' }));
    await due();
    await w.tick();
  }
  expect(await states()).toEqual(['dropped/5']);
  await due();
  await w.tick();
  expect(mail.attempts).toHaveLength(2 + 5);

  // Обрыв посреди DATA без ответа сервера: могло дойти — повтора нет.
  await s.pool.query('DELETE FROM mail_outbox');
  await create(owner, `@[${viewer.id}] три`);
  await due();
  mail.failures.push(Object.assign(new Error('reset'), { code: 'ECONNECTION', command: 'DATA' }));
  await w.tick();
  await due();
  await w.tick();
  expect(await states()).toEqual(['uncertain/1']);

  // SMTP не ответил в срок: исход неизвестен, повтора нет, и поздний ответ отметку не меняет.
  await s.pool.query('DELETE FROM mail_outbox');
  await create(owner, `@[${viewer.id}] четыре`);
  await due();
  const { promise: hang, resolve: release } = Promise.withResolvers<void>();
  mail.hang = hang;
  const before = mail.attempts.length;
  await w.tick();
  expect(await states()).toEqual(['uncertain/1']);
  mail.hang = null;
  release();
  await Bun.sleep(50);
  await due();
  await w.tick();
  expect(await states()).toEqual(['uncertain/1']);
  expect(mail.attempts.length - before).toBe(1);
});

test('перезапуск посреди отправки и падение между отправкой и отметкой: второй раз письмо не уходит', async () => {
  const mail = new FakeMail();
  // Первый процесс: SMTP завис, строка взята.
  const { promise: hang, resolve: release } = Promise.withResolvers<void>();
  mail.hang = hang;
  const first = await worker(mail);
  await create(owner, `@[${viewer.id}] перезапуск`);
  await due();
  const flight = first.tick();
  for (let i = 0; i < 100 && mail.attempts.length === 0; i++) await Bun.sleep(10);
  expect(await states()).toEqual(['sending/1']);
  // Новый процесс на той же базе: взятое прошлым — исход неизвестен.
  const second = await worker(mail);
  await second.tick();
  expect(await states()).toEqual(['uncertain/1']);
  expect(second.counts.uncertain).toBe(1);
  // Прошлый процесс «очнулся» и дослал: его отметка строку не трогает, повтора нет.
  mail.hang = null;
  release();
  await flight;
  await due();
  await second.tick();
  expect(await states()).toEqual(['uncertain/1']);
  expect(mail.attempts).toHaveLength(1);

  // Письмо ушло, а отметка не записалась (база отказала): после перезапуска — не снова.
  await s.pool.query('DELETE FROM mail_outbox');
  await create(owner, `@[${viewer.id}] без отметки`);
  await due();
  await s.pool.query(`CREATE FUNCTION test_no_sent() RETURNS trigger LANGUAGE plpgsql AS $f$
    BEGIN IF NEW.state = 'sent' THEN RAISE EXCEPTION 'сбой теста'; END IF; RETURN NEW; END $f$`);
  await s.pool.query('CREATE TRIGGER test_no_sent BEFORE UPDATE ON mail_outbox FOR EACH ROW EXECUTE FUNCTION test_no_sent()');
  try {
    await second.tick();
  } finally {
    await s.pool.query('DROP FUNCTION test_no_sent CASCADE');
  }
  expect(mail.delivered).toHaveLength(2);
  expect(await states()).toEqual(['sending/1']);
  const third = await worker(mail);
  await due();
  await third.tick();
  expect(await states()).toEqual(['uncertain/1']);
  expect(mail.attempts).toHaveLength(2);

  // И без перезапуска: строка в `sending` дольше срока аренды — неизвестна, не снова.
  const leased = await worker(mail, { leaseMs: 60_000 });
  await s.pool.query(`UPDATE mail_outbox SET state = 'sending', sending_at = now() - interval '1 hour', done_at = NULL`);
  await leased.tick();
  expect(await states()).toEqual(['uncertain/1']);
  expect(mail.attempts).toHaveLength(2);
});

test('повтор после отказа сливается с новой строкой того же человека и файла: одно письмо со всем', async () => {
  const mail = new FakeMail();
  const w = await worker(mail);
  const a = await create(owner, `@[${viewer.id}] первое`);
  await due();
  const { promise: hang, resolve: release } = Promise.withResolvers<void>();
  mail.hang = hang;
  mail.failures.push(Object.assign(new Error('452'), { responseCode: 452, command: 'RCPT TO' }));
  const flight = w.tick();
  for (let i = 0; i < 100 && mail.attempts.length === 0; i++) await Bun.sleep(10);
  // Пока первое отправлялось, пришло второе: новая ждущая строка.
  const b = await create(editor, `@[${viewer.id}] второе`);
  expect(await states()).toEqual(['sending/1', 'pending/0']);
  mail.hang = null;
  release();
  await flight;
  expect(await states()).toEqual(['skipped/1', 'pending/1']);
  await due();
  await w.tick();
  expect(mail.delivered).toHaveLength(1);
  for (const t of [a.thread.id, b.thread.id]) expect(mail.delivered[0].text).toContain(`?comment=${t}`);
});

test('предел писем человеку в час: сверх — строки ждут', async () => {
  const mail = new FakeMail();
  const w = await worker(mail, { perHour: 2 });
  for (let i = 0; i < 3; i++) {
    const f = (await s.newFile(owner, `Ф${i}`)).id;
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [f, viewer.id]);
    await create(owner, `@[${viewer.id}]`, f);
  }
  await due();
  await w.tick();
  await w.tick();
  expect(mail.delivered).toHaveLength(2);
  expect((await states()).sort()).toEqual(['pending/0', 'sent/1', 'sent/1']);
});

test('письмо — текст как текст: HTML экранирован, тема одной строкой без чужих заголовков, упоминание без доступа — без имени', async () => {
  const mail = new FakeMail();
  const w = await worker(mail);
  const eve = await s.signIn(`m-eve-${crypto.randomUUID()}@example.com`, '<b>Ева</b> "q"');
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, eve.id]);
  await s.pool.query(`UPDATE file SET name = $2 WHERE id = $1`, [file, 'Макет\r\nBcc: evil@example.com\u202e']);
  const stranger = await s.signIn(`m-stranger-${crypto.randomUUID()}@example.com`, 'Чужой');
  await create(eve, `<script>alert(1)</script> & @[${viewer.id}] @[${stranger.id}]`);
  await due();
  await w.tick();
  const m = mail.delivered[0];
  expect(m.subject).toBe('<b>Ева</b> "q": комментарий в «Макет Bcc: evil@example.com»');
  expect(m.subject).not.toMatch(/[\r\n\u202e]/);
  expect(m.html).not.toContain('<script>');
  expect(m.html).not.toContain('<b>Ева');
  expect(m.html).toContain('&lt;script&gt;alert(1)&lt;/script&gt; &amp; @Вера @неизвестный');
  expect(m.html).toContain('&lt;b&gt;Ева&lt;/b&gt; &quot;q&quot;');
  expect(m.text).toContain('@неизвестный');
  expect(`${m.text}${m.html}`).not.toContain('Чужой');
});

test('настоящая почта: письмо через nodemailer приходит в Mailpit стенда, тема без чужих заголовков, HTML экранирован', async () => {
  const to = `m-real-${crypto.randomUUID()}@example.com`;
  const real = await s.signIn(to, 'Настоящий');
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, real.id]);
  await s.pool.query(`UPDATE file SET name = $2 WHERE id = $1`, [file, 'Макет\r\nBcc: evil@example.com']);
  const w = await worker(smtpMailer('smtp://127.0.0.1:11025', 'Ferrum <ferrum@localhost>'));
  const t = await create(owner, `<img src=x onerror=alert(1)> @[${real.id}]`);
  await due();
  await w.tick();
  expect(w.counts.sent).toBe(1);
  const api = 'http://127.0.0.1:18025/api/v1';
  let id = '';
  for (let i = 0; i < 100 && !id; i++) {
    const found = (await (await fetch(`${api}/search?query=${encodeURIComponent(`to:"${to}"`)}`)).json()) as {
      messages: { ID: string; Subject: string }[];
    };
    id = found.messages.find((x) => x.Subject.includes('комментарий'))?.ID ?? '';
    if (!id) await Bun.sleep(50);
  }
  expect(id).not.toBe('');
  const msg = (await (await fetch(`${api}/message/${id}`)).json()) as {
    Subject: string;
    To: { Address: string }[];
    Bcc: { Address: string }[] | null;
    HTML: string;
    Text: string;
  };
  expect(msg.To.map((x) => x.Address)).toEqual([to]);
  expect(msg.Bcc ?? []).toEqual([]);
  expect(msg.Subject).toBe('Ольга: комментарий в «Макет Bcc: evil@example.com»');
  expect(msg.HTML).toContain('&lt;img src=x onerror=alert(1)&gt; @Настоящий');
  expect(msg.HTML).not.toContain('<img');
  expect(msg.Text).toContain(`${s.publicUrl}/#/file/${file}?comment=${t.thread.id}`);
  const headers = (await (await fetch(`${api}/message/${id}/headers`)).json()) as Record<string, string[]>;
  expect(Object.keys(headers).map((k) => k.toLowerCase())).not.toContain('bcc');
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/comment-mail.test.ts`
Expected: FAIL — нет модуля `../src/comment-mail`: 0 pass, 1 fail (ошибка загрузки)

- [ ] **Step 3: Реализация**

В `apps/server/src/app.ts` заменить:

````ts
import type { BlobStore, UploadPace } from './blob-store';
import { clientIp, proxyMatcher } from './client-ip';
import { commentRoutes } from './comment-routes';
import { fileRoutes, uploadGate } from './file-routes';
````

на:

````ts
import type { BlobStore, UploadPace } from './blob-store';
import { clientIp, proxyMatcher } from './client-ip';
import { CommentMailWorker, type CommentMailOptions } from './comment-mail';
import { commentRoutes } from './comment-routes';
import { fileRoutes, uploadGate } from './file-routes';
````

В `apps/server/src/app.ts` заменить:

````ts
  /** Группировка писем о комментариях: тесты сокращают. */
  commentMailDelayMs?: number;
  apiDeadlineMs?: number;
  uploadIdleMs?: number;
````

на:

````ts
  /** Группировка писем о комментариях: тесты сокращают. */
  commentMailDelayMs?: number;
  /** Отправка писем о комментариях; `false` — без неё (тесты гоняют свою с поддельной почтой). */
  commentMail?: Partial<CommentMailOptions> | false;
  apiDeadlineMs?: number;
  uploadIdleMs?: number;
````

В `apps/server/src/app.ts` заменить:

````ts

/** Сервер и его WebSocket: остановка контейнера и тесты обращаются к обоим. */
export type Running = Bun.Server<ConnData> & { hub: SyncHub; compactor: Compactor | null };

export function startServer(deps: AppDeps, port: number): Running {
````

на:

````ts

/** Сервер и его WebSocket: остановка контейнера и тесты обращаются к обоим. */
export type Running = Bun.Server<ConnData> & { hub: SyncHub; compactor: Compactor | null; commentMail: CommentMailWorker | null };

export function startServer(deps: AppDeps, port: number): Running {
````

В `apps/server/src/app.ts` заменить:

````ts
  }
  const handleAuth = authRoutes({ ...deps, hub });
  const trusted = proxyMatcher(deps.trustedProxies);
  const withHeaders = responseHeaders(deps.publicUrl);
````

на:

````ts
  }
  const handleAuth = authRoutes({ ...deps, hub });
  const commentMail =
    deps.commentMail === false
      ? null
      : new CommentMailWorker(deps.pool, deps.mailer, deps.publicUrl, (file) => hub.openUsers(file), deps.allowedEmails, deps.commentMail);
  void commentMail?.start().catch((e: unknown) => console.error('Письма о комментариях:', e instanceof Error ? e.message : e));
  const trusted = proxyMatcher(deps.trustedProxies);
  const withHeaders = responseHeaders(deps.publicUrl);
````

В `apps/server/src/app.ts` заменить:

````ts
    websocket: hub.websocket,
  });
  return Object.assign(server, { hub, compactor });
}
````

на:

````ts
    websocket: hub.websocket,
  });
  return Object.assign(server, { hub, compactor, commentMail });
}
````

Создать `apps/server/src/comment-mail.ts`:

````ts
// Письма о комментариях (спека подсистемы 4, раздел 10.4): очередь `mail_outbox` в Postgres. Строку ставит в очередь та
// же транзакция, что пишет комментарий (`comment_enqueue`): одна ждущая строка на получателя и файл, срок — от первого
// комментария, за это время следующие сливаются в одно письмо. Здесь — отправка.
//
// Не больше одного раза: строка берётся на отправку (`sending`) и это фиксируется до SMTP. Удача — `sent`; известный отказ
// SMTP (сервер ответил кодом, соединение не открылось) — строка возвращается в очередь с паузой, после `MAX_ATTEMPTS` —
// `dropped`. Строка, оставшаяся в `sending` (процесс упал между отправкой и отметкой, отметка не записалась, отправка
// зависла дольше срока), не отправляется снова: `uncertain` и запись в журнал. Лишнее письмо хуже потерянного уведомления:
// комментарий всё равно виден в файле.
//
// Перед отправкой — снова: доступ есть, письма не выключены, файл не открыт сейчас и не открывался после первого
// комментария, комментарии ещё есть. Иначе — `skipped`.

import { splitCommentBody, type CommentDto } from '@ferrum/protocol';
import type { Pool } from 'pg';
import { emailAllowed } from './auth';
import { escapeHtml, mailFailure, type Mail, type Mailer } from './mail';

export interface CommentMailOptions {
  /** Как часто смотреть в очередь. Срок группировки (5 минут) — у строки, а не здесь. */
  pollMs: number;
  /** Строк за проход. */
  batch: number;
  /** Сколько ждать SMTP: дольше — исход неизвестен, строка `uncertain`. */
  sendTimeoutMs: number;
  /** Строка в `sending` дольше этого — исход неизвестен (`uncertain`). Больше `sendTimeoutMs`. */
  leaseMs: number;
  /** Писем одному человеку в час: сверх — строки ждут и копят комментарии дальше. */
  perHour: number;
  /** Попыток при известном отказе SMTP. */
  maxAttempts: number;
  /** Пауза перед повтором: `retryMs × номер попытки`. */
  retryMs: number;
  /** Сколько хранить отработанные строки: предел в час считается по ним. */
  keepMs: number;
}

export const COMMENT_MAIL_OPTIONS: CommentMailOptions = {
  pollMs: 15_000,
  batch: 20,
  sendTimeoutMs: 60_000,
  leaseMs: 5 * 60_000,
  perHour: 30,
  maxAttempts: 5,
  retryMs: 60_000,
  keepMs: 24 * 60 * 60_000,
};

/** Текст комментария в письме — начало, не больше этого. */
const EXCERPT_CHARS = 500;
/** Тема: без управляющих и переводов строк, не длиннее. */
const SUBJECT_CHARS = 150;

interface Claimed {
  id: string;
  recipient_id: string;
  file_id: string;
  extra: number;
  attempts: number;
}

interface Content {
  email: string;
  file_name: string;
  rank: number | null;
  wants: boolean;
  opened_after: boolean;
  items: CommentDto[] | null;
}

/** Отказ, после которого письмо точно не ушло: SMTP ответил кодом или соединение не открылось. Иначе (обрыв посреди
 * передачи, неизвестная ошибка) исход неизвестен — повтор мог бы прислать второе письмо. */
export function knownSmtpFailure(e: unknown): boolean {
  const { responseCode, command } = (typeof e === 'object' && e !== null ? e : {}) as { responseCode?: unknown; command?: unknown };
  return (typeof responseCode === 'number' && responseCode >= 400) || command === 'CONN';
}

/** Управляющие (CR, LF и прочие), разделители строк и знаки направления: в заголовке письма их быть не должно. */
// eslint-disable-next-line no-control-regex
const HEADER_UNSAFE_RE = /[\u0000-\u001f\u007f-\u009f\u2028\u2029\u202a-\u202e\u2066-\u2069]/g;

/** Строка заголовка: без `HEADER_UNSAFE_RE` и пробелов по краям, не длиннее предела. */
export function headerSafe(s: string, max = SUBJECT_CHARS): string {
  const clean = s.replace(HEADER_UNSAFE_RE, ' ').replace(/\s+/g, ' ').trim();
  const cps = [...clean];
  return cps.length > max ? `${cps.slice(0, max - 1).join('')}…` : clean;
}

const personName = (a: CommentDto['author']) => (a.id === null ? 'Удалённый пользователь' : a.name || 'Участник');

/** Текст комментария для письма: упоминания — именами (без доступа — «@неизвестный»), начало не длиннее предела. */
function excerpt(c: CommentDto): string {
  const text = splitCommentBody(c.body, c.mentions)
    .map((p) => ('text' in p ? p.text : `@${p.name ?? 'неизвестный'}`))
    .join('');
  const cps = [...text];
  return cps.length > EXCERPT_CHARS ? `${cps.slice(0, EXCERPT_CHARS - 1).join('')}…` : text;
}

/** Письмо о комментариях файла: каждый со ссылкой на своё обсуждение. Всё чужое в HTML экранировано, тема — одной строкой. */
export function commentMail(to: string, file: string, items: CommentDto[], extra: number, publicUrl: string, fileId: string): Mail {
  const link = (thread: string) => `${publicUrl}/#/file/${fileId}?comment=${thread}`;
  const fileName = headerSafe(file, 100);
  const total = items.length + extra;
  const subject = headerSafe(
    total === 1 ? `${personName(items[0].author)}: комментарий в «${fileName}»` : `Новые комментарии в «${fileName}»: ${total}`,
  );
  const more = extra > 0 ? `И ещё ${extra} — в файле.` : '';
  const footer = 'Письма о комментариях можно выключить в настройках Ferrum.';
  const text =
    [`Комментарии в файле «${fileName}»:`, ...items.map((c) => `${personName(c.author)}:\n${excerpt(c)}\n${link(c.thread)}`), more, footer]
      .filter(Boolean)
      .join('\n\n') + '\n';
  const html =
    `<p>Комментарии в файле «${escapeHtml(fileName)}»:</p>` +
    items
      .map(
        (c) =>
          `<p><b>${escapeHtml(personName(c.author))}</b><br>${escapeHtml(excerpt(c)).replace(/\n/g, '<br>')}<br>` +
          `<a href="${escapeHtml(link(c.thread))}">Открыть обсуждение</a></p>`,
      )
      .join('') +
    (more ? `<p>${escapeHtml(more)}</p>` : '') +
    `<p>${escapeHtml(footer)}</p>`;
  return { to, subject, text, html };
}

export class CommentMailWorker {
  private readonly opts: CommentMailOptions;
  private timer: Timer | null = null;
  private running: Promise<void> | null = null;
  private stopped = false;
  /** Счётчики для тестов и пробы. */
  readonly counts = { sent: 0, skipped: 0, retried: 0, dropped: 0, uncertain: 0 };

  constructor(
    private readonly pool: Pool,
    private readonly mailer: Mailer,
    private readonly publicUrl: string,
    /** Кто держит файл открытым сейчас (соединения с подтверждённой ролью). */
    private readonly openUsers: (file: string) => string[],
    private readonly allowedEmails: string[] = [],
    opts: Partial<CommentMailOptions> = {},
  ) {
    this.opts = { ...COMMENT_MAIL_OPTIONS, ...opts };
  }

  /** Запуск: строки, взятые прошлым процессом и не отмеченные, — исход неизвестен, снова не шлются. */
  async start(): Promise<void> {
    const { rows } = await this.pool.query<{ id: string }>(
      `UPDATE mail_outbox SET state = 'uncertain', done_at = now() WHERE state = 'sending' RETURNING id`,
    );
    this.noteUncertain(rows.length, 'остались от прошлого запуска');
    this.timer = setInterval(() => void this.tick().catch(report), this.opts.pollMs);
    this.timer.unref?.();
  }

  /** Один проход; идущий проход не запускается второй раз. Число отработанных строк. */
  tick(): Promise<void> {
    if (this.stopped) return Promise.resolve();
    this.running ??= this.round().finally(() => {
      this.running = null;
    });
    return this.running;
  }

  async stop(): Promise<void> {
    this.stopped = true;
    if (this.timer) clearInterval(this.timer);
    await this.running?.catch(() => {});
  }

  private noteUncertain(n: number, why: string): void {
    if (n === 0) return;
    this.counts.uncertain += n;
    console.error(`Письма о комментариях: исход ${n} неизвестен (${why}), повтора не будет`);
  }

  private async round(): Promise<void> {
    const { opts, pool } = this;
    const expired = await pool.query(
      `UPDATE mail_outbox SET state = 'uncertain', done_at = now()
       WHERE state = 'sending' AND sending_at < now() - make_interval(secs => $1 / 1000.0)`,
      [opts.leaseMs],
    );
    this.noteUncertain(expired.rowCount ?? 0, 'отправка дольше срока');
    await pool.query(`DELETE FROM mail_outbox WHERE done_at < now() - make_interval(secs => $1 / 1000.0)`, [opts.keepMs]);
    // Взять — и зафиксировать — до SMTP. Человеку, которому за час ушло (или уходит) `perHour` писем, строка ждёт дальше и
    // копит комментарии; в одном проходе ему — не больше остатка.
    const { rows } = await pool.query<Claimed>(
      `WITH recent AS (
         SELECT q.recipient_id AS r, (SELECT count(*) FROM mail_outbox x
                WHERE x.recipient_id = q.recipient_id AND x.state IN ('sent', 'sending')
                  AND COALESCE(x.done_at, x.sending_at) > now() - interval '1 hour') AS n
         FROM (SELECT DISTINCT recipient_id FROM mail_outbox WHERE state = 'pending' AND due_at <= now()) q),
       cand AS (
         SELECT q.id, q.recipient_id, q.due_at FROM mail_outbox q JOIN recent ON recent.r = q.recipient_id
         WHERE q.state = 'pending' AND q.due_at <= now() AND recent.n < $2
         ORDER BY q.due_at LIMIT $1 * 4 FOR UPDATE OF q SKIP LOCKED),
       ranked AS (
         SELECT c.id, c.due_at, row_number() OVER (PARTITION BY c.recipient_id ORDER BY c.due_at) + recent.n AS k
         FROM cand c JOIN recent ON recent.r = c.recipient_id)
       UPDATE mail_outbox o SET state = 'sending', sending_at = now(), attempts = o.attempts + 1
       WHERE o.id IN (SELECT id FROM ranked WHERE k <= $2 ORDER BY due_at LIMIT $1)
       RETURNING o.id, o.recipient_id, o.file_id, o.extra, o.attempts`,
      [opts.batch, opts.perHour],
    );
    for (const row of rows) {
      if (this.stopped) break;
      await this.deliver(row).catch(report);
    }
  }

  private async deliver(row: Claimed): Promise<void> {
    const { rows } = await this.pool.query<Content>(
      `SELECT u.email, f.name AS file_name, file_role_rank(o.file_id, o.recipient_id) AS rank,
              COALESCE((SELECT comment_mail FROM user_pref WHERE user_id = o.recipient_id), true) AS wants,
              EXISTS (SELECT 1 FROM file_recent r WHERE r.user_id = o.recipient_id AND r.file_id = o.file_id AND r.opened_at > o.first_at)
                AS opened_after,
              (SELECT jsonb_agg(comment_json(c) ORDER BY c.created_at, c.id)
               FROM comment c JOIN comment_thread t ON t.id = c.thread_id
               WHERE c.id = ANY (o.comment_ids) AND t.file_id = o.file_id AND t.deleted_at IS NULL) AS items
       FROM mail_outbox o JOIN "user" u ON u.id = o.recipient_id JOIN file f ON f.id = o.file_id
       WHERE o.id = $1`,
      [row.id],
    );
    const c = rows[0];
    // Файл или человек удалены — строка ушла вместе с ними.
    if (!c) return;
    const skip =
      (c.rank ?? 0) < 1 ||
      !c.wants ||
      c.opened_after ||
      !c.items?.length ||
      !emailAllowed(c.email, this.allowedEmails) ||
      this.openUsers(row.file_id).includes(row.recipient_id);
    if (skip) {
      await this.mark(row.id, 'skipped');
      this.counts.skipped++;
      return;
    }
    const mail = commentMail(c.email, c.file_name, c.items!, row.extra, this.publicUrl, row.file_id);
    let timer: Timer | undefined;
    const outcome = await Promise.race([
      this.mailer.send(mail).then(
        () => 'sent' as const,
        (e: unknown) => e,
      ),
      new Promise<'timeout'>((resolve) => {
        timer = setTimeout(resolve, this.opts.sendTimeoutMs, 'timeout');
      }),
    ]).finally(() => clearTimeout(timer));
    if (outcome === 'sent') {
      await this.mark(row.id, 'sent');
      this.counts.sent++;
      return;
    }
    if (outcome === 'timeout' || !knownSmtpFailure(outcome)) {
      if (outcome !== 'timeout') console.error('Письмо о комментариях: сбой отправки,', mailFailure(outcome));
      await this.mark(row.id, 'uncertain');
      this.noteUncertain(1, outcome === 'timeout' ? 'SMTP не ответил в срок' : 'обрыв посреди отправки');
      return;
    }
    console.error('Письмо о комментариях не принято:', mailFailure(outcome));
    if (row.attempts >= this.opts.maxAttempts) {
      await this.mark(row.id, 'dropped');
      this.counts.dropped++;
      console.error(`Письмо о комментариях брошено после ${row.attempts} попыток`);
      return;
    }
    await this.release(row.id, this.opts.retryMs * row.attempts);
    this.counts.retried++;
  }

  /** Отметка только своей строки в `sending`: строку, признанную `uncertain` (срок, новый процесс), она не трогает. */
  private mark(id: string, state: 'sent' | 'skipped' | 'dropped' | 'uncertain'): Promise<unknown> {
    return this.pool.query(`UPDATE mail_outbox SET state = $2, done_at = now() WHERE id = $1 AND state = 'sending'`, [id, state]);
  }

  /** Вернуть в очередь с паузой. Пока строка отправлялась, могла появиться новая ждущая на того же человека и файл: тогда
   * эта сливается в неё (старое — вперёд), а сама уходит. Одновременная вставка — повтор. */
  private async release(id: string, delayMs: number): Promise<void> {
    for (let attempt = 0; ; attempt++) {
      try {
        await this.pool.query(
          `WITH me AS (SELECT * FROM mail_outbox WHERE id = $1 AND state = 'sending'),
                merged AS (
                  UPDATE mail_outbox p SET
                    comment_ids = (me.comment_ids || p.comment_ids)[1:20],
                    extra = p.extra + me.extra + GREATEST(0, cardinality(me.comment_ids) + cardinality(p.comment_ids) - 20),
                    first_at = LEAST(p.first_at, me.first_at),
                    due_at = LEAST(p.due_at, now() + make_interval(secs => $2 / 1000.0)),
                    attempts = GREATEST(p.attempts, me.attempts)
                  FROM me WHERE p.recipient_id = me.recipient_id AND p.file_id = me.file_id AND p.state = 'pending'
                  RETURNING p.id)
           UPDATE mail_outbox o SET
             state = CASE WHEN EXISTS (SELECT 1 FROM merged) THEN 'skipped' ELSE 'pending' END,
             done_at = CASE WHEN EXISTS (SELECT 1 FROM merged) THEN now() END,
             due_at = now() + make_interval(secs => $2 / 1000.0),
             sending_at = NULL
           FROM me WHERE o.id = me.id`,
          [id, delayMs],
        );
        return;
      } catch (e) {
        if ((e as { code?: string }).code !== '23505' || attempt >= 2) throw e;
      }
    }
  }
}

function report(e: unknown): void {
  console.error('Письма о комментариях:', e instanceof Error ? e.message : e);
}
````

В `apps/server/src/index.ts` заменить:

````ts
  stopGc();
  server.compactor?.stop();
  // Принятое дописывается, соединения закрываются (клиенты подключатся заново): `server.stop()` без `true`
  // ждал бы их, и контейнер дождался бы SIGKILL.
  void server.hub
    .stop()
    .then(() => server.stop(true))
    .then(() => pool.end())
````

на:

````ts
  stopGc();
  server.compactor?.stop();
  // Письмо, взятое на отправку, дописывается: строка не останется «неизвестной» без нужды.
  const mails = server.commentMail?.stop() ?? Promise.resolve();
  // Принятое дописывается, соединения закрываются (клиенты подключатся заново): `server.stop()` без `true`
  // ждал бы их, и контейнер дождался бы SIGKILL.
  void Promise.all([server.hub.stop(), mails])
    .then(() => server.stop(true))
    .then(() => pool.end())
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `test:server` — протокол 56 pass, сервер 593 pass, 0 fail

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/src/app.ts apps/server/src/comment-mail.ts apps/server/src/index.ts apps/server/test/api-stand.ts apps/server/test/comment-mail.test.ts
git commit -F - <<'EOF'
сервер: письма о комментариях — отправка очереди mail_outbox

comment-mail.ts: CommentMailWorker берёт созревшие строки (группировка 5 минут — срок строки) и фиксирует
взятие до SMTP. Перед отправкой снова: доступ, настройка, файл открыт сейчас или открыт после первого
комментария, комментарии ещё есть — иначе skipped. Известный отказ SMTP (код ответа, соединение не
открылось) — повтор с паузой, после 5 попыток dropped; обрыв посреди передачи, зависание, строка от
прошлого процесса или дольше аренды — uncertain, без повтора: письмо не больше одного раза. Возврат в
очередь сливается с новой ждущей строкой. Не больше 30 писем человеку в час. Письмо: HTML экранирован,
тема одной строкой без управляющих и знаков направления, ссылки #/file/<id>?comment=<thread>.
Запуск и остановка — в startServer и index.ts; тесты с почтой, которая отказывает и зависает, перезапуск
посреди отправки и настоящее письмо через Mailpit стенда.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 8: Сервер — проба нагрузки

Цели раздела 13 для комментариев: 1000 обсуждений × 20 комментариев (задержки, EXPLAIN — только индексы), рассылка 50 соединениям, враждебный человек с сокетом, который не читает. Числа — в выводе теста; задача добавляет только пробу и её тест.

**Files:**
- Test: `apps/server/test/comments-load-probe.ts` (новый), `apps/server/test/comments-load.test.ts` (новый)

**Interfaces:**
- Consumes: маршруты задачи 5, рассылка задачи 6.
- Produces: `apps/server/test/comments-load-probe.ts` (режимы `big` и `fanout SECS [hostile]`), `comments-load.test.ts`.

- [ ] **Step 1: Написать пробу**

Создать `apps/server/test/comments-load-probe.ts`:

````ts
// Нагрузка комментариев (спека подсистемы 4, разделы 10 и 13.1). Печатает итоги в JSON последней строкой.
//   big                — файл с 1000 обсуждений по 20 комментариев: задержка списка (все страницы), обсуждения, нового
//                        обсуждения и ответа по REST; планы запросов (индексы).
//   fanout SECS [hostile] — 50 соединений одного файла (5 человек по 10 вкладок) в дочернем процессе, раз в 100 мс новый
//                        комментарий по REST: задержка от ответа REST до прихода события на каждое соединение (часы одного
//                        процесса). `hostile` — ещё процесс враждебного человека: 5 сеансов, 40 потоков создают комментарии и
//                        читают список из 1000 обсуждений без остановки, и его сокет, который не читает. Процессор, память и
//                        буферы — сервера (клиенты в других процессах).

import { encodeClientFrame, parseServerFrame, SYNC_PROTOCOL, syncPath, type PinDto } from '@ferrum/protocol';
import { connect } from 'node:net';
import type { Pool } from 'pg';
import { apiStand, type Person } from './api-stand';

const [mode, ...args] = process.argv.slice(2);
const pin: PinDto = { page: '3@7910247476534078275', node: null, x: 1, y: 2, wx: 1, wy: 2 };
const pct = (xs: number[], p: number) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length ? s[Math.min(s.length - 1, Math.ceil((s.length * p) / 100) - 1)] : -1;
};
const round = (v: number) => Math.round(v * 100) / 100;

if (mode === 'clients') {
  // Дочерний процесс: 50 соединений и автор комментариев.
  const [secs, origin, file, ...cookies] = args;
  const url = origin.replace('http', 'ws');
  const users = JSON.parse(cookies.pop()!) as string[];
  const sockets: WebSocket[] = [];
  const received = new Map<number, number[]>();
  let ready = 0;
  for (let i = 0; i < 50; i++) {
    const k = i % cookies.length;
    const ws = new WebSocket(url + syncPath(file, users[k]), { headers: { cookie: cookies[k], origin } } as never);
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => ws.send(encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 }));
    ws.onmessage = (e) => {
      const f = parseServerFrame(new Uint8Array(e.data as ArrayBuffer));
      if (f?.t === 'role' && ++ready === 50) console.log('ready');
      if (f?.t !== 'comment') return;
      const at = performance.now();
      let list = received.get(f.event.seq);
      if (!list) received.set(f.event.seq, (list = []));
      list.push(at);
    };
    sockets.push(ws);
  }
  while (ready < 50) await Bun.sleep(5);
  const fromResponse: number[] = [];
  const fromRequest: number[] = [];
  const rest: number[] = [];
  const sent: { seq: number; t0: number; t1: number }[] = [];
  const start = performance.now();
  let failed = 0;
  while (performance.now() - start < Number(secs) * 1000) {
    const t0 = performance.now();
    const res = await fetch(`${origin}/api/files/${file}/comments`, {
      method: 'POST',
      headers: { cookie: cookies[0], origin, 'content-type': 'application/json' },
      body: JSON.stringify({ pin, body: `замер ${t0}` }),
    });
    const t1 = performance.now();
    if (res.status !== 201) {
      failed++;
      await res.text();
    } else sent.push({ seq: ((await res.json()) as { seq: number }).seq, t0, t1 });
    rest.push(t1 - t0);
    const wait = 100 - (performance.now() - t0);
    if (wait > 0) await Bun.sleep(wait);
  }
  await Bun.sleep(1000);
  let delivered = 0;
  for (const s of sent) {
    for (const at of received.get(s.seq) ?? []) {
      delivered++;
      fromResponse.push(at - s.t1);
      fromRequest.push(at - s.t0);
    }
  }
  console.log(
    JSON.stringify({
      comments: sent.length,
      failed,
      deliveries: delivered,
      expected: sent.length * 50,
      restP50: round(pct(rest, 50)),
      restP95: round(pct(rest, 95)),
      fromResponseP50: round(pct(fromResponse, 50)),
      fromResponseP95: round(pct(fromResponse, 95)),
      fromResponseMax: round(pct(fromResponse, 100)),
      fromRequestP50: round(pct(fromRequest, 50)),
      fromRequestP95: round(pct(fromRequest, 95)),
    }),
  );
  setTimeout(() => process.exit(0), 10_000);
  await new Promise(() => {});
}

if (mode === 'hostile') {
  // Дочерний процесс враждебного: 40 потоков создают комментарии и читают большой список до сигнала.
  const [origin, file, thread, ...cookies] = args;
  const counts: Record<string, number> = {};
  let stop = false;
  process.on('SIGTERM', () => (stop = true));
  const note = (k: string) => (counts[k] = (counts[k] ?? 0) + 1);
  const big = '😀'.repeat(4000);
  const loops = Array.from({ length: 40 }, async (_, i) => {
    const cookie = cookies[i % cookies.length];
    const headers = { cookie, origin, 'content-type': 'application/json' };
    while (!stop) {
      const kind = i % 4;
      const res =
        kind === 0
          ? await fetch(`${origin}/api/files/${file}/comments`, { method: 'POST', headers, body: JSON.stringify({ pin, body: big }) })
          : kind === 1
            ? await fetch(`${origin}/api/files/${file}/comments/${thread}/comments`, {
                method: 'POST',
                headers,
                body: JSON.stringify({ body: big }),
              })
            : kind === 2
              ? await fetch(`${origin}/api/files/${file}/comments?limit=500`, { headers })
              : await fetch(`${origin}/api/files/${file}/comments/${thread}`, { headers });
      note(`${['create', 'reply', 'list', 'thread'][kind]}:${res.status}`);
      await res.arrayBuffer();
    }
  });
  console.log('started');
  await Promise.all(loops);
  console.log(JSON.stringify(counts));
  process.exit(0);
}

/** 1000 обсуждений по 20 комментариев, у каждого пятого — упоминание; прочтения у половины. */
async function seed(pool: Pool, file: string, people: Person[], threads = 1000, per = 20): Promise<void> {
  const ids = people.map((p) => p.id);
  await pool.query(
    `INSERT INTO comment_thread (id, file_id, page_id, node_id, x, y, wx, wy, first_id, created_by, created_at, seq)
     SELECT gen_random_uuid(), $1, '3@7910247476534078275', (i % 50) || '@7910247476534078275', i, i, i, i, gen_random_uuid(),
            ($2::text[])[1 + i % 5], now() - make_interval(secs => 1000000 - i * 100), i
     FROM generate_series(1, $3) i`,
    [file, ids, threads],
  );
  await pool.query(
    `INSERT INTO comment (id, thread_id, author_id, body, created_at)
     SELECT CASE WHEN k = 0 THEN t.first_id ELSE gen_random_uuid() END, t.id, ($2::text[])[1 + (k + t.seq::int) % 5],
            repeat('Текст комментария. ', 10) || CASE WHEN k % 5 = 0 THEN '@[' || ($2::text[])[1 + k % 5] || ']' ELSE '' END,
            t.created_at + make_interval(secs => k)
     FROM comment_thread t, generate_series(0, $3 - 1) k WHERE t.file_id = $1`,
    [file, ids, per],
  );
  await pool.query(
    `INSERT INTO comment_mention (comment_id, user_id)
     SELECT c.id, substring(c.body FROM '@\\[([A-Za-z0-9_-]+)\\]') FROM comment c JOIN comment_thread t ON t.id = c.thread_id
     WHERE t.file_id = $1 AND c.body LIKE '%@[%'`,
    [file],
  );
  await pool.query(
    `INSERT INTO comment_read (thread_id, user_id, read_at) SELECT id, $2, now() FROM comment_thread WHERE file_id = $1 AND seq % 2 = 0`,
    [file, ids[0]],
  );
  await pool.query('SELECT comment_thread_refresh(id) FROM comment_thread WHERE file_id = $1', [file]);
  await pool.query('INSERT INTO comment_file (file_id, seq) VALUES ($1, $2)', [file, threads]);
  await pool.query('ANALYZE comment_thread, comment, comment_mention, comment_read, comment_file');
}

async function timed(n: number, work: () => Promise<void>): Promise<{ p50: number; p95: number; max: number }> {
  const xs: number[] = [];
  for (let i = 0; i < n; i++) {
    const t0 = performance.now();
    await work();
    xs.push(performance.now() - t0);
  }
  return { p50: round(pct(xs, 50)), p95: round(pct(xs, 95)), max: round(pct(xs, 100)) };
}

const unlimited = { max: 1_000_000, windowSeconds: 60 };

if (mode === 'big') {
  const s = await apiStand({ commentMail: false, limits: { comment: unlimited, read: unlimited, write: unlimited } });
  const people: Person[] = [];
  for (let i = 0; i < 5; i++) people.push(await s.signIn(`load-${i}-${Date.now()}@example.com`, `Человек ${i}`));
  const file = (await s.newFile(people[0])).id;
  for (const p of people.slice(1))
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor')`, [file, p.id]);
  await seed(s.pool, file, people);
  const me = people[0];
  const get = async (path: string) => {
    const res = await s.call(me, 'GET', path);
    if (res.status !== 200) throw new Error(`${path}: ${res.status}`);
    return res.json() as Promise<{ seq: number; more: boolean; threads: { id: string }[] }>;
  };
  let bytes = 0;
  const listAll = await timed(20, async () => {
    let after = 0;
    for (let more = true; more;) {
      const res = await s.call(me, 'GET', `/api/files/${file}/comments?after=${after}`);
      const text = await res.text();
      bytes = text.length;
      const l = JSON.parse(text) as { seq: number; more: boolean };
      after = l.seq;
      more = l.more;
    }
  });
  const { rows: threadRows } = await s.pool.query<{ id: string }>('SELECT id FROM comment_thread WHERE file_id = $1 ORDER BY seq', [file]);
  const thread = threadRows[500].id;
  const listAfter = await timed(50, async () => void (await get(`/api/files/${file}/comments?after=990`)));
  const detail = await timed(50, async () => void (await get(`/api/files/${file}/comments/${thread}`)));
  const create = await timed(50, async () => {
    const res = await s.call(me, 'POST', `/api/files/${file}/comments`, { pin, body: `@[${people[1].id}] новое` });
    if (res.status !== 201) throw new Error(`create: ${res.status}`);
    await res.text();
  });
  let k = 0;
  const reply = await timed(50, async () => {
    const t = threadRows[(k++ * 37) % 1000].id;
    const res = await s.call(people[k % 5], 'POST', `/api/files/${file}/comments/${t}/comments`, { body: `@[${people[0].id}] ответ` });
    if (res.status !== 201) throw new Error(`reply: ${res.status}`);
    await res.text();
  });
  const mentions = await timed(50, async () => void (await get(`/api/files/${file}/mentions?q=чел`)));
  // Планы: запросы, из которых собраны функции вида, — с настоящими параметрами.
  const plan = async (sql: string, params: unknown[]) =>
    (await s.pool.query<{ 'QUERY PLAN': string }>(`EXPLAIN (ANALYZE, COSTS OFF) ${sql}`, params)).rows
      .map((r) => r['QUERY PLAN'])
      .join('\n');
  const plans = {
    list: await plan(
      `SELECT t.id FROM comment_thread t WHERE t.file_id = $1 AND t.seq > $2 AND ($2 > 0 OR t.deleted_at IS NULL) ORDER BY t.seq LIMIT 501`,
      [file, 0],
    ),
    listAfter: await plan(
      `SELECT t.id FROM comment_thread t WHERE t.file_id = $1 AND t.seq > $2 AND ($2 > 0 OR t.deleted_at IS NULL) ORDER BY t.seq LIMIT 501`,
      [file, 990],
    ),
    threadComments: await plan(`SELECT count(*), max(created_at) FROM comment WHERE thread_id = $1`, [thread]),
    unread: await plan(
      `SELECT EXISTS (SELECT 1 FROM comment c WHERE c.thread_id = $1 AND c.author_id IS DISTINCT FROM $2
         AND c.created_at > COALESCE((SELECT read_at FROM comment_read r WHERE r.thread_id = $1 AND r.user_id = $2), '-infinity'))`,
      [thread, me.id],
    ),
    mentions: await plan(`SELECT u.id FROM comment_mention m JOIN "user" u ON u.id = m.user_id WHERE m.comment_id = $1`, [
      (await s.pool.query<{ first_id: string }>('SELECT first_id FROM comment_thread WHERE id = $1', [thread])).rows[0].first_id,
    ]),
  };
  const indexes = Object.fromEntries(
    Object.entries(plans).map(([k, v]) => [
      k,
      [...v.matchAll(/(?:Index Scan|Index Only Scan|Bitmap Index Scan)(?: Backward)? (?:using|on) (\w+)/g)].map((m) => m[1]),
    ]),
  );
  const seqScans = Object.fromEntries(Object.entries(plans).map(([k, v]) => [k, /Seq Scan on (comment|comment_thread)\b/.test(v)]));
  console.error(Object.values(plans).join('\n---\n'));
  console.log(JSON.stringify({ listAllPages: listAll, listBytes: bytes, listAfter, detail, create, reply, mentions, indexes, seqScans }));
  await s.stop();
  process.exit(0);
}

if (mode === 'fanout') {
  const [secs, hostile = ''] = args;
  const s = await apiStand({
    commentMail: false,
    // 50 соединений — вкладки пяти человек; предел частоты автора замера снят, у враждебного — настоящий.
    syncLimits: { connectionsPerUser: 100, userOpensBurst: 1000 },
  });
  const people: Person[] = [];
  for (let i = 0; i < 5; i++) people.push(await s.signIn(`fan-${i}-${Date.now()}@example.com`, `Человек ${i}`));
  const file = (await s.newFile(people[0])).id;
  for (const p of people.slice(1))
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, p.id]);
  await s.valkey.set(`api:comment:${people[0].id}`, '-100000');
  await seed(s.pool, file, people);
  const thread = (await s.pool.query<{ id: string }>('SELECT id FROM comment_thread WHERE file_id = $1 ORDER BY seq LIMIT 1', [file]))
    .rows[0].id;
  let evil: Person[] = [];
  let raw: ReturnType<typeof connect> | null = null;
  if (hostile) {
    // Пять сеансов одного человека — как вкладки и устройства: предел частоты у них общий.
    const email = `evil-${Date.now()}@example.com`;
    for (let i = 0; i < 5; i++) evil.push(await s.signIn(email, 'Враждебный'));
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, evil[0].id]);
    raw = connect(Number(new URL(s.publicUrl).port), '127.0.0.1');
    raw.write(
      `GET ${syncPath(file, evil[0].id)} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
        `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: ${s.publicUrl}\r\nCookie: ${evil[0].cookie}\r\n\r\n`,
    );
    await Bun.sleep(200);
    const hello = encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 });
    raw.write(Buffer.concat([Buffer.from([0x82, 0x80 | hello.length, 0, 0, 0, 0]), hello]));
    await Bun.sleep(200);
    raw.pause();
  } else evil = [];
  Bun.gc(true);
  const rss0 = process.memoryUsage().rss;
  const heap0 = process.memoryUsage().heapUsed;
  const child = Bun.spawn(
    ['bun', import.meta.path, 'clients', secs, s.publicUrl, file, ...people.map((p) => p.cookie), JSON.stringify(people.map((p) => p.id))],
    { stdout: 'pipe', stderr: 'inherit' },
  );
  const reader = (child.stdout as ReadableStream<Uint8Array>).getReader();
  let text = '';
  const next = async (): Promise<string> => {
    for (;;) {
      const at = text.indexOf('\n');
      if (at >= 0) {
        const line = text.slice(0, at);
        text = text.slice(at + 1);
        return line;
      }
      const r = await reader.read();
      if (r.done) throw new Error('клиенты упали');
      text += new TextDecoder().decode(r.value);
    }
  };
  if ((await next()) !== 'ready') throw new Error('клиенты не готовы');
  let flood: Bun.Subprocess<'ignore', 'pipe', 'inherit'> | null = null;
  if (hostile) {
    flood = Bun.spawn(['bun', import.meta.path, 'hostile', s.publicUrl, file, thread, ...evil.map((p) => p.cookie)], {
      stdout: 'pipe',
      stderr: 'inherit',
    });
  }
  const cpu0 = process.cpuUsage();
  const t0 = performance.now();
  let peakBuffered = 0;
  let peakRss = 0;
  // Задержка цикла событий сервера: таймер на 10 мс.
  const lags: number[] = [];
  let tick = performance.now();
  const probe = setInterval(() => {
    const now = performance.now();
    lags.push(now - tick - 10);
    tick = now;
    peakBuffered = Math.max(peakBuffered, s.hub.bufferedBytes());
    peakRss = Math.max(peakRss, process.memoryUsage().rss);
  }, 10);
  const report = JSON.parse(await next()) as Record<string, number>;
  clearInterval(probe);
  const cpu = process.cpuUsage(cpu0);
  const wall = performance.now() - t0;
  let floodCounts: Record<string, number> = {};
  if (flood) {
    flood.kill('SIGTERM');
    const out = await new Response(flood.stdout).text();
    floodCounts = JSON.parse(out.trim().split('\n').at(-1) ?? '{}') as Record<string, number>;
  }
  Bun.gc(true);
  const accepted = (await s.pool.query<{ n: number }>('SELECT count(*)::int AS n FROM comment WHERE author_id = $1', [evil[0]?.id ?? '']))
    .rows[0].n;
  console.log(
    JSON.stringify({
      sockets: 50,
      hostile: !!hostile,
      ...report,
      serverCpuPercent: round(((cpu.user + cpu.system) / 1000 / wall) * 100),
      loopLagP95: round(pct(lags, 95)),
      loopLagMax: round(pct(lags, 100)),
      rssPeakGrowthMb: round((peakRss - rss0) / 1e6),
      heapGrowthMb: round((process.memoryUsage().heapUsed - heap0) / 1e6),
      peakBufferedKb: round(peakBuffered / 1024),
      commentSkipped: s.hub.commentSkipped,
      hostileAcceptedComments: accepted,
      hostile429: Object.entries(floodCounts)
        .filter(([k]) => k.endsWith(':429'))
        .reduce((n, [, v]) => n + v, 0),
      floodCounts,
    }),
  );
  child.kill();
  raw?.destroy();
  await s.stop();
  process.exit(0);
}
````

Создать `apps/server/test/comments-load.test.ts`:

````ts
// Нагрузка комментариев отдельным процессом (`comments-load-probe.ts`): большой файл, рассылка событий 50 соединениям, то же
// под потоком враждебного человека. Пределы — с запасом на чужую нагрузку машины; цифры печатаются.

import { fileURLToPath } from 'node:url';
import { expect, test } from 'bun:test';

type Stat = { p50: number; p95: number; max: number };

async function run(...args: string[]): Promise<Record<string, unknown>> {
  const p = Bun.spawn(['bun', fileURLToPath(new URL('./comments-load-probe.ts', import.meta.url)), ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [text, err] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  await p.exited;
  const line = text.trim().split('\n').at(-1) ?? '';
  try {
    return JSON.parse(line) as Record<string, unknown>;
  } catch {
    throw new Error(`проба упала: ${text}\n${err.slice(-2000)}`);
  }
}

test('1000 обсуждений по 20 комментариев: список, обсуждение, создание и ответ — по индексам и быстро', async () => {
  const r = await run('big');
  console.log('комментарии, большой файл (мс)', r);
  expect((r.listAllPages as Stat).p95).toBeLessThan(1000);
  for (const k of ['listAfter', 'detail', 'create', 'reply', 'mentions']) expect((r[k] as Stat).p95, k).toBeLessThan(150);
  expect(r.indexes).toEqual({
    list: ['comment_thread_live_idx'],
    listAfter: ['comment_thread_seq_idx'],
    threadComments: ['comment_thread_idx'],
    unread: ['comment_thread_idx', 'comment_read_pkey'],
    mentions: ['comment_mention_pkey', 'user_pkey'],
  });
  expect(Object.values(r.seqScans as Record<string, boolean>).some(Boolean)).toBe(false);
}, 180_000);

test('событие 50 соединениям одного файла: доходит до всех, от ответа REST до прихода — в пределах цели', async () => {
  const r = await run('fanout', '8');
  console.log('комментарии, рассылка 50 соединениям', r);
  expect(r.deliveries).toBe(r.expected);
  expect(r.fromResponseP95 as number).toBeLessThan(150);
  expect(r.heapGrowthMb as number).toBeLessThan(20);
}, 180_000);

test('враждебный человек (5 сеансов, 40 потоков) и сокет, который не читает: остальные получают всё, предел общий, память не растёт', async () => {
  const r = await run('fanout', '8', 'hostile');
  console.log('комментарии, враждебный поток', r);
  expect(r.deliveries).toBe(r.expected);
  // 60 комментариев в минуту на человека — на все его сеансы вместе.
  expect(r.hostileAcceptedComments as number).toBeLessThanOrEqual(60);
  expect(r.hostile429 as number).toBeGreaterThan(1000);
  expect(r.fromResponseP95 as number).toBeLessThan(500);
  expect(r.loopLagP95 as number).toBeLessThan(50);
  expect(r.heapGrowthMb as number).toBeLessThan(20);
}, 180_000);
````

- [ ] **Step 2: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `test:server` — протокол 56 pass, сервер 596 pass, 0 fail (проба печатает свои числа, около 2,5 минуты на весь прогон)

- [ ] **Step 3: Закоммитить**

```bash
git add apps/server/test/comments-load-probe.ts apps/server/test/comments-load.test.ts
git commit -F - <<'EOF'
сервер: комментарии — проба нагрузки

comments-load-probe.ts отдельным процессом: файл с 1000 обсуждений по 20 комментариев (задержка списка
всех страниц, обсуждения, создания, ответа, подсказки; планы запросов — индексы, без полного просмотра),
рассылка события 50 соединениям одного файла (от ответа REST до прихода, процессор, память, задержка
цикла событий) и то же под потоком враждебного человека: 5 сеансов, 40 потоков создают комментарии по
4000 знаков и читают большой список, его сокет не читает. comments-load.test.ts закрепляет пределы.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 9: Клиент — комментарии в `sync.worker`

Спека §10.4, §7.1; «Решения» 14, 16. `CommentStore`: запросы REST, порядок по `seq` и догоняние страницами, непрочитанное, метки ядру прямым портом, основному потоку — изменения не чаще раза в 50 мс и открытое обсуждение. Первый список отдаётся целиком (ссылка на обсуждение со второй страницы не считается ненайденной).

**Files:**
- Create: `apps/web/src/core/comments-store.ts`
- Modify: `apps/web/src/core/sync-client.ts`, `apps/web/src/core/sync-messages.ts`, `apps/web/src/ui/file-store.ts`, `apps/web/src/ui/server/server-store.ts`, `apps/web/src/worker/sync.worker.ts`
- Test: `apps/web/unit/comments-store.test.ts` (новый), `apps/web/unit/sync-client.test.ts`

**Interfaces:**
- Consumes: `ServerFrame` `comment` и виды задачи 3, команды ядра задачи 4.
- Produces: `CommentStore` (`connected(role)`, `event(ev)`, `request(id, req)`, `stop()`), `CommentsEnv`, `CommentPinForCore`, `CommentsCoreCommand`, `CommentRequest`, `CommentResult`, `CommentsToMain`, `COMMENTS_FLUSH_MS = 50`, `initialsOf`, `authorColor`, `viewOf`; `SyncEnv.comments?: { connected(role); event(ev) }`; `ToSync` `{t:'comments', id, req}`, `FromSync` `{t:'comments', m}`; `SyncEvents.comments?(m)`, `SyncHandle { stop(); comments?(req) }`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/unit/comments-store.test.ts`:

````ts
// Комментарии в `sync.worker`: порядок по `seq`, догоняние без дублей, непрочитанное, открытое обсуждение и метки ядру. Сервер —
// свой `fetch` с теми же видами ответов, что у настоящего (ответы проходят строгий разбор протокола), часы ручные.

import { expect, test } from '@playwright/test';
import type { CommentDto, CommentEvent, ThreadDto, ThreadView } from '@ferrum/protocol';
import { COMMENTS_FLUSH_MS, CommentStore, initialsOf, type CommentsCoreCommand, type CommentsToMain } from '../src/core/comments-store';

const ME = 'me';
const OTHER = 'other';
const uuid = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`;

function comment(thread: number, n: number, author = OTHER, body = `текст ${n}`): CommentDto {
  return {
    id: uuid(1000 + n),
    thread: uuid(thread),
    author: { id: author, name: author === ME ? 'Я Сам' : 'Иван Петров' },
    body,
    createdAt: n,
    editedAt: null,
    mentions: {},
  };
}

function thread(n: number, seq: number, extra: Partial<ThreadView> = {}): ThreadView {
  const first = comment(n, n * 100, extra.author?.id ?? OTHER);
  return {
    id: uuid(n),
    pin: { page: '1@1', node: null, x: n, y: n, wx: n, wy: n },
    author: first.author,
    createdAt: 1,
    resolvedAt: null,
    resolvedBy: null,
    seq,
    first,
    replies: 0,
    lastAt: 1,
    participants: [first.author.id!],
    unread: false,
    ...extra,
  };
}

function dto(t: ThreadView): ThreadDto {
  const { unread, ...rest } = t;
  void unread;
  return rest;
}

class Clock {
  private timers = new Map<number, () => void>();
  private next = 0;
  now = () => 0;
  setTimeout = (fn: () => void) => {
    const id = ++this.next;
    this.timers.set(id, fn);
    return id;
  };
  clearTimeout = (t: unknown) => void this.timers.delete(t as number);
  run(): void {
    const all = [...this.timers.values()];
    this.timers.clear();
    for (const fn of all) fn();
  }
}

interface Call {
  path: string;
  method: string;
  body: unknown;
}

function stand() {
  const clock = new Clock();
  const calls: Call[] = [];
  const core: CommentsCoreCommand[] = [];
  const main: CommentsToMain[] = [];
  /** Ответы по адресу: функция от запроса, очередь страниц списка. */
  const routes: ((c: Call) => { status: number; body: unknown } | null)[] = [];
  const gates: Promise<void>[] = [];
  const store = new CommentStore({
    user: ME,
    base: '/api/files/f',
    clock,
    core: (c) => core.push(c),
    post: (m) => main.push(m),
    fetch: async (path, init) => {
      const call = { path, method: init?.method ?? 'GET', body: init?.body ? JSON.parse(init.body as string) : undefined };
      calls.push(call);
      const gate = gates.shift();
      if (gate) await gate;
      for (const r of routes) {
        const hit = r(call);
        if (hit) return new Response(JSON.stringify(hit.body), { status: hit.status });
      }
      return new Response(JSON.stringify({ error: 'NOT_FOUND' }), { status: 404 });
    },
  });
  const settle = () => new Promise((r) => setTimeout(r, 0));
  const list = (after: number, body: { seq: number; threads: ThreadView[]; removed?: string[]; more?: boolean }) =>
    routes.push((c) =>
      c.path === `/api/files/f/comments?after=${after}&limit=500` ? { status: 200, body: { removed: [], more: false, ...body } } : null,
    );
  return { store, clock, calls, core, main, routes, gates, settle, list };
}

const ev = (
  seq: number,
  t: ThreadView,
  action: 'create' | 'reply' | 'edit' = 'reply',
  by = OTHER,
  c: CommentDto | null = null,
): CommentEvent => ({
  kind: 'thread',
  seq,
  by,
  action,
  thread: dto({ ...t, seq }),
  comment: c,
  deleted: null,
});

test('первая загрузка — страницами, ядру метки одним набором, панели — списком целиком', async () => {
  const s = stand();
  s.list(0, { seq: 2, threads: [thread(1, 1), thread(2, 2)], more: true });
  s.list(2, { seq: 5, threads: [thread(3, 3)] });
  s.store.connected('editor');
  await s.settle();
  expect(s.core.filter((c) => c.type === 'commentPins')).toEqual([
    expect.objectContaining({
      type: 'commentPins',
      replace: true,
      pins: expect.arrayContaining([expect.objectContaining({ id: uuid(3) })]),
    }),
  ]);
  expect(s.core[0]).toEqual({ type: 'commentsAvailable', on: true });
  s.clock.run();
  const out = s.main.filter((m) => m.t === 'threads');
  expect(out).toHaveLength(1);
  expect(out[0]).toMatchObject({ reset: true });
  expect((out[0] as { upserts: ThreadView[] }).upserts.map((t) => t.id).sort()).toEqual([uuid(1), uuid(2), uuid(3)]);
  expect(s.store.seq).toBe(5);
});

test('события: следующее применяется, старое пропускается, дыра — догоняние без дублей', async () => {
  const s = stand();
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  await s.settle();
  s.store.event(ev(2, thread(1, 1), 'reply'));
  expect(s.store.thread(uuid(1))?.seq).toBe(2);
  // Старое (уже учтено) — пропускается.
  s.store.event(ev(2, { ...thread(1, 1), replies: 99 }, 'reply'));
  expect(s.store.thread(uuid(1))?.replies).toBe(0);
  // Старое событие обсуждения, которого ещё нет у клиента, тоже пропускается: известный номер не откатывается назад.
  s.store.event(ev(1, thread(7, 1), 'create'));
  expect(s.store.thread(uuid(7))).toBeUndefined();
  expect(s.store.seq).toBe(2);
  // Дыра: 3 потеряно, пришло 4 — догоняние `after=2`; 4 придёт и списком, и ждущим событием, а применится один раз.
  s.list(2, { seq: 4, threads: [thread(2, 3), { ...thread(1, 4), replies: 1 }] });
  s.store.event(ev(4, { ...thread(1, 4), replies: 1 }));
  await s.settle();
  expect(s.calls.map((c) => c.path)).toContain('/api/files/f/comments?after=2&limit=500');
  expect(s.store.seq).toBe(4);
  expect(s.store.thread(uuid(2))).toBeTruthy();
  expect(s.store.thread(uuid(1))?.replies).toBe(1);
});

test('gap и новое соединение — догоняние от известного, а не весь список заново', async () => {
  const s = stand();
  s.list(0, { seq: 3, threads: [thread(1, 3)] });
  s.store.connected('viewer');
  await s.settle();
  s.list(3, { seq: 6, threads: [thread(2, 6)], removed: [uuid(1)] });
  s.store.event({ kind: 'gap', seq: 6 });
  await s.settle();
  expect(s.store.thread(uuid(1))).toBeUndefined();
  expect(s.core).toContainEqual({ type: 'commentPinsRemove', ids: [uuid(1)] });
  s.list(6, { seq: 6, threads: [] });
  s.store.connected('viewer');
  await s.settle();
  expect(s.calls.filter((c) => c.path.includes('after=0'))).toHaveLength(1);
  expect(s.calls.at(-1)!.path).toBe('/api/files/f/comments?after=6&limit=500');
});

test('события во время догоняния ждут и применяются после, по порядку', async () => {
  const s = stand();
  let open!: () => void;
  s.gates.push(new Promise<void>((r) => (open = r)));
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  s.store.event(ev(3, { ...thread(1, 1), replies: 2 }));
  s.store.event(ev(2, { ...thread(1, 1), replies: 1 }));
  open();
  await s.settle();
  expect(s.store.seq).toBe(3);
  expect(s.store.thread(uuid(1))?.replies).toBe(2);
  // Догоняния лишнего нет: 2 и 3 — подряд после 1.
  expect(s.calls.filter((c) => c.path.includes('comments?after'))).toHaveLength(1);
});

test('непрочитанное: чужой ответ — да, свой — нет, в открытом обсуждении — нет и отмечается прочитанным', async () => {
  const s = stand();
  s.list(0, { seq: 2, threads: [thread(1, 1), thread(2, 2)] });
  s.store.connected('editor');
  await s.settle();
  s.store.event(ev(3, thread(1, 1), 'reply', OTHER));
  expect(s.store.thread(uuid(1))?.unread).toBe(true);
  s.store.event(ev(4, thread(1, 1), 'reply', ME));
  expect(s.store.thread(uuid(1))?.unread).toBe(false);
  // Правка чужого не делает обсуждение новым.
  s.store.event(ev(5, thread(1, 1), 'edit', OTHER));
  expect(s.store.thread(uuid(1))?.unread).toBe(false);
  // Открытое обсуждение: подробности, затем чужой ответ — сразу в окно, прочитано, на сервер — `read`.
  s.routes.push((c) =>
    c.path === `/api/files/f/comments/${uuid(2)}`
      ? { status: 200, body: { thread: { ...thread(2, 2), unread: true }, comments: [comment(2, 200)] } }
      : null,
  );
  s.routes.push((c) => (c.path.endsWith('/read') ? { status: 200, body: { ok: true } } : null));
  await s.store.request(1, { k: 'open', thread: uuid(2) });
  expect(s.calls.some((c) => c.path === `/api/files/f/comments/${uuid(2)}/read` && c.method === 'POST')).toBe(true);
  const reply = comment(2, 201);
  s.store.event(ev(6, thread(2, 2), 'reply', OTHER, reply));
  await s.settle();
  expect(s.store.thread(uuid(2))?.unread).toBe(false);
  const detail = s.main.filter((m) => m.t === 'detail').at(-1) as Extract<CommentsToMain, { t: 'detail' }>;
  expect(detail.comments.map((c) => c.id)).toEqual([comment(2, 200).id, reply.id]);
  expect(s.calls.filter((c) => c.path.endsWith(`${uuid(2)}/read`))).toHaveLength(2);
});

test('удалённое обсуждение закрывает окно; удалённый комментарий уходит из окна', async () => {
  const s = stand();
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  await s.settle();
  s.clock.run();
  s.routes.push((c) =>
    c.path === `/api/files/f/comments/${uuid(1)}`
      ? { status: 200, body: { thread: thread(1, 1), comments: [comment(1, 100), comment(1, 101)] } }
      : null,
  );
  await s.store.request(1, { k: 'open', thread: uuid(1) });
  s.store.event({
    kind: 'thread',
    seq: 2,
    by: OTHER,
    action: 'delete',
    thread: dto(thread(1, 2)),
    comment: null,
    deleted: comment(1, 101).id,
  });
  let d = s.main.filter((m) => m.t === 'detail').at(-1) as Extract<CommentsToMain, { t: 'detail' }>;
  expect(d.comments.map((c) => c.id)).toEqual([comment(1, 100).id]);
  s.store.event({ kind: 'removed', seq: 3, by: OTHER, thread: uuid(1) });
  d = s.main.filter((m) => m.t === 'detail').at(-1) as Extract<CommentsToMain, { t: 'detail' }>;
  expect(d).toEqual({ t: 'detail', thread: uuid(1), view: null, comments: [] });
  s.clock.run();
  expect(s.main.filter((m) => m.t === 'threads').at(-1)).toMatchObject({ removed: [uuid(1)] });
});

test('двигать метку: автор и владелец; отказ сервера возвращает метку на место', async () => {
  const s = stand();
  const mine = thread(1, 1, { author: { id: ME, name: 'Я Сам' } });
  s.list(0, { seq: 2, threads: [mine, thread(2, 2)] });
  s.store.connected('editor');
  await s.settle();
  const pins = (s.core.find((c) => c.type === 'commentPins') as Extract<CommentsCoreCommand, { type: 'commentPins' }>).pins;
  expect(pins.find((p) => p.id === uuid(1))?.movable).toBe(true);
  expect(pins.find((p) => p.id === uuid(2))?.movable).toBe(false);
  s.list(2, { seq: 2, threads: [] });
  s.store.connected('owner');
  await s.settle();
  const again = (s.core.filter((c) => c.type === 'commentPins').at(-1) as Extract<CommentsCoreCommand, { type: 'commentPins' }>).pins;
  expect(again.every((p) => p.movable)).toBe(true);
  s.routes.unshift((c) => (c.method === 'PATCH' ? { status: 403, body: { error: 'FORBIDDEN' } } : null));
  const before = s.core.length;
  await s.store.request(2, { k: 'move', thread: uuid(2), pin: { page: '1@1', node: null, x: 50, y: 50, wx: 50, wy: 50 } });
  expect(s.main.at(-1)).toEqual({ t: 'result', id: 2, result: { ok: false, failure: 'FORBIDDEN' } });
  expect(s.core.slice(before)).toEqual([
    { type: 'commentPins', replace: false, pins: [expect.objectContaining({ id: uuid(2), x: 2, wx: 2 })] },
  ]);
});

test('панель получает изменения пачкой, не по событию', async () => {
  const s = stand();
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  await s.settle();
  s.clock.run();
  const before = s.main.filter((m) => m.t === 'threads').length;
  for (let i = 2; i < 12; i++) s.store.event(ev(i, thread(1, 1), 'edit'));
  expect(s.main.filter((m) => m.t === 'threads')).toHaveLength(before);
  s.clock.run();
  const out = s.main.filter((m) => m.t === 'threads');
  expect(out).toHaveLength(before + 1);
  expect(out.at(-1)).toMatchObject({ reset: false, upserts: [expect.objectContaining({ seq: 11 })] });
  expect(COMMENTS_FLUSH_MS).toBeGreaterThan(0);
});

test('сбой сети при загрузке — повтор по таймеру', async () => {
  const s = stand();
  s.routes.push((c) => (c.path.includes('after=0') && s.calls.length === 1 ? { status: 503, body: { error: 'UNAVAILABLE' } } : null));
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  await s.settle();
  expect(s.store.thread(uuid(1))).toBeUndefined();
  s.clock.run();
  await s.settle();
  expect(s.store.thread(uuid(1))).toBeTruthy();
});

test('инициалы — по первым буквам двух слов, удалённый — «?»', () => {
  expect(initialsOf({ id: 'a', name: 'иван петров сидоров' })).toBe('ИП');
  expect(initialsOf({ id: 'a', name: '  ' })).toBe('?');
  expect(initialsOf({ id: null, name: 'Иван' })).toBe('?');
});

test('первый список уходит панели только целиком: вторая страница ещё в пути — панели ничего', async () => {
  const s = stand();
  let open!: () => void;
  s.list(0, { seq: 1, threads: [thread(1, 1)], more: true });
  s.list(1, { seq: 2, threads: [thread(2, 2)] });
  s.gates.push(Promise.resolve(), new Promise<void>((r) => (open = r)));
  s.store.connected('editor');
  await s.settle();
  s.clock.run();
  expect(s.main.filter((m) => m.t === 'threads')).toEqual([]);
  open();
  await s.settle();
  s.clock.run();
  const out = s.main.filter((m) => m.t === 'threads');
  expect(out).toHaveLength(1);
  expect((out[0] as { upserts: ThreadView[] }).upserts.map((t) => t.id).sort()).toEqual([uuid(1), uuid(2)]);
});
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
  const access: AccessState[] = [];
  const participants: Participant[][] = [];
  let reloads = 0;
  const env: SyncEnv = {
````

на:

````ts
  const access: AccessState[] = [];
  const participants: Participant[][] = [];
  const comments: unknown[] = [];
  let reloads = 0;
  const env: SyncEnv = {
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
    reload: () => reloads++,
    participants: (list) => participants.push(list),
  };
  const client = new SyncClient(env);
````

на:

````ts
    reload: () => reloads++,
    participants: (list) => participants.push(list),
    comments: { connected: (role) => comments.push({ role }), event: (ev) => comments.push(ev) },
  };
  const client = new SyncClient(env);
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
    commands,
    participants,
    flushes,
    statuses,
````

на:

````ts
    commands,
    participants,
    comments,
    flushes,
    statuses,
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
  ws.dropped(SYNC_CLOSE.REVOKED);
  expect(s.participants.at(-1)).toEqual([]);
  expect(s.clock.timers.size).toBe(0);
});
````

на:

````ts
  ws.dropped(SYNC_CLOSE.REVOKED);
  expect(s.participants.at(-1)).toEqual([]);
  expect(s.clock.timers.size).toBe(0);
});

test('комментарии: роль соединения — догнать пропущенное, кадр `comment` — событие хранилищу', () => {
  const s = stand();
  const ws = s.live();
  expect(s.comments).toEqual([]);
  ws.receive({ t: 'role', role: 'viewer', canShare: false, version: 1 });
  ws.receive({ t: 'comment', event: { kind: 'gap', seq: 7 } });
  expect(s.comments).toEqual([{ role: 'viewer' }, { kind: 'gap', seq: 7 }]);
  // Переподключение: снова роль — снова догоняние.
  ws.dropped();
  const next = nextSocket(s);
  next.open();
  next.receive({ t: 'role', role: 'viewer', canShare: false, version: 1 });
  expect(s.comments.at(-1)).toEqual({ role: 'viewer' });
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/comments-store.test.ts unit/sync-client.test.ts`
Expected: FAIL — нет модуля `src/core/comments-store`: Playwright не загружает файл тестов, ни один тест не запущен

- [ ] **Step 3: Реализация**

Создать `apps/web/src/core/comments-store.ts`:

````ts
// Комментарии открытого файла со стороны клиента (спека подсистемы 4, раздел 10). Живут в `sync.worker`, рядом с сокетом файла:
// события `comment` и запросы REST сходятся здесь по `seq` файла, ядро получает метки, основной поток — только изменившиеся
// обсуждения (панель) и открытое обсуждение (окно). Логика без DOM: модульные тесты дают свой `fetch` и часы.
//
// Порядок. Событие с `seq = известный + 1` применяется; `seq ≤ известного` — уже учтено (пришло списком); дыра или `gap` —
// догоняние `GET ?after=известный` страницами. Пока идёт догоняние, события ждут и потом применяются по тому же правилу.
// Ответ своего изменения меняет обсуждение, но не `known`: его событие придёт и будет пропущено по `seq` обсуждения.

import {
  COMMENT_PAGE_MAX,
  parseCommentList,
  parseCommentWrite,
  parseMentionList,
  parseThreadDetail,
  PRESENCE_COLORS,
  presenceColor,
  type CommentAuthor,
  type CommentDto,
  type CommentEvent,
  type FileRole,
  type MentionCandidate,
  type PinDto,
  type ThreadDto,
  type ThreadView,
} from '@ferrum/protocol';
import type { Clock } from './server-files';

/** Метка обсуждения для ядра (`CommentPinDto`). */
export interface CommentPinForCore {
  id: string;
  page: string;
  node?: string;
  x: number;
  y: number;
  wx: number;
  wy: number;
  initials: string;
  color: number;
  replies: number;
  unread: boolean;
  resolved: boolean;
  movable: boolean;
}

export type CommentsCoreCommand =
  | { type: 'commentsAvailable'; on: boolean }
  | { type: 'commentPins'; pins: CommentPinForCore[]; replace: boolean }
  | { type: 'commentPinsRemove'; ids: string[] };

/** Запросы основного потока. `open` — какое обсуждение открыто в окне (его живые изменения идут туда, оно не «непрочитано»). */
export type CommentRequest =
  | { k: 'open'; thread: string | null }
  | { k: 'create'; pin: PinDto; body: string }
  | { k: 'reply'; thread: string; body: string }
  | { k: 'edit'; thread: string; comment: string; body: string }
  | { k: 'delete'; thread: string; comment: string }
  | { k: 'resolve'; thread: string; resolved: boolean }
  | { k: 'move'; thread: string; pin: PinDto }
  | { k: 'mentions'; q: string };

/** Ответ на запрос: `failure` — код ошибки API, `NETWORK` или `NOT_FOUND`. */
export type CommentResult = { ok: true; value?: unknown } | { ok: false; failure: string };

export type CommentsToMain =
  /** Изменившиеся обсуждения; `reset` — список целиком заново (новый файл или первая загрузка). */
  | { t: 'threads'; reset: boolean; upserts: ThreadView[]; removed: string[] }
  /** Открытое обсуждение: все его комментарии по порядку; `null` — его больше нет (удалено или нет доступа). */
  | { t: 'detail'; thread: string; view: ThreadView | null; comments: CommentDto[] }
  | { t: 'result'; id: number; result: CommentResult }
  /** Роль в файле и кто я: права кнопок окна. */
  | { t: 'me'; user: string; role: FileRole };

export interface CommentsEnv {
  user: string;
  /** Адрес API файла: `/api/files/<id>`. */
  base: string;
  fetch(path: string, init?: RequestInit): Promise<Response>;
  core(cmd: CommentsCoreCommand): void;
  post(m: CommentsToMain): void;
  clock: Clock;
}

/** Изменения панели уходят основному потоку не чаще: поток событий не перерисовывает панель на каждое. */
export const COMMENTS_FLUSH_MS = 50;
/** Сбой догоняния по сети — снова через столько. */
const RETRY_MS = 5000;

/** Инициалы автора для метки: первые буквы двух первых слов имени; без имени — «?». */
export function initialsOf(a: CommentAuthor): string {
  const words = a.name.trim().split(/\s+/).filter(Boolean);
  if (a.id === null || words.length === 0) return '?';
  return words
    .slice(0, 2)
    .map((w) => [...w][0] ?? '')
    .join('')
    .toLocaleUpperCase('ru');
}

/** Цвет автора — тот же, что у его курсора; удалённый — серый. */
export function authorColor(a: CommentAuthor): number {
  return a.id === null ? 0x8a8580 : parseInt(PRESENCE_COLORS[presenceColor(a.id)].slice(1), 16);
}

async function json(res: Response): Promise<unknown> {
  try {
    return (await res.json()) as unknown;
  } catch {
    return null;
  }
}

function failureOf(status: number, body: unknown): string {
  if (status >= 500) return 'UNAVAILABLE';
  const code = (body as { error?: unknown } | null)?.error;
  return typeof code === 'string' ? code : 'NETWORK';
}

export class CommentStore {
  private readonly threads = new Map<string, ThreadView>();
  private known = 0;
  private loaded = false;
  private fetching = false;
  private readonly waiting: CommentEvent[] = [];
  private role: FileRole | null = null;
  private open: string | null = null;
  private detail: { thread: string; comments: CommentDto[] } | null = null;
  /** Изменения для панели, ждущие отправки. */
  private upserts = new Map<string, ThreadView>();
  private removedOut = new Set<string>();
  private resetOut = false;
  private flushTimer: unknown = null;
  private retryTimer: unknown = null;
  private stopped = false;

  constructor(private readonly env: CommentsEnv) {}

  /** Роль подтверждена соединением (первое или новое): метки доступны, догоняем пропущенное. */
  connected(role: FileRole): void {
    if (this.stopped) return;
    const changed = this.role !== role;
    this.role = role;
    this.env.core({ type: 'commentsAvailable', on: true });
    this.env.post({ t: 'me', user: this.env.user, role });
    // Право двигать метку зависит от роли: метки заново.
    if (changed && this.loaded) this.env.core({ type: 'commentPins', pins: this.pins([...this.threads.values()]), replace: true });
    void this.catchUp();
  }

  stop(): void {
    this.stopped = true;
    if (this.flushTimer !== null) this.env.clock.clearTimeout(this.flushTimer);
    if (this.retryTimer !== null) this.env.clock.clearTimeout(this.retryTimer);
  }

  /** Событие сокета. */
  event(ev: CommentEvent): void {
    if (this.stopped) return;
    if (this.fetching || !this.loaded) {
      this.waiting.push(ev);
      return;
    }
    this.apply(ev);
  }

  private apply(ev: CommentEvent): void {
    if (ev.kind === 'gap' || ev.seq > this.known + 1) {
      void this.catchUp();
      return;
    }
    if (ev.seq <= this.known) return;
    this.known = ev.seq;
    if (ev.kind === 'removed') {
      this.remove(ev.thread);
      return;
    }
    const was = this.threads.get(ev.thread.id);
    if (was && was.seq >= ev.thread.seq) return;
    const fresh = ev.action === 'create' || ev.action === 'reply';
    // Непрочитано — для чужого нового комментария в обсуждении, которое не открыто; своё всегда прочитано.
    let unread = was?.unread ?? false;
    if (ev.by === this.env.user) unread = fresh ? false : unread;
    else if (fresh) unread = this.open !== ev.thread.id;
    this.upsert({ ...ev.thread, unread });
    if (this.detail?.thread === ev.thread.id) {
      const list = this.detail.comments;
      if (ev.deleted) this.detail.comments = list.filter((c) => c.id !== ev.deleted);
      if (ev.comment) {
        const at = list.findIndex((c) => c.id === ev.comment!.id);
        if (at >= 0) list[at] = ev.comment;
        else list.push(ev.comment);
      }
      this.postDetail();
      // Открытое обсуждение читается сразу: на других вкладках и после перезагрузки оно не «новое».
      if (fresh && ev.by !== this.env.user) void this.markRead(ev.thread.id);
    }
  }

  /** Догоняние `?after=known` страницами; ждущие события — после. Первый раз — весь список. */
  private async catchUp(): Promise<void> {
    if (this.fetching || this.stopped) return;
    this.fetching = true;
    const first = !this.loaded;
    try {
      for (;;) {
        const res = await this.env.fetch(`${this.env.base}/comments?after=${this.known}&limit=${COMMENT_PAGE_MAX}`);
        const body = await json(res);
        const list = res.ok ? parseCommentList(body) : null;
        if (!list) throw new Error(`список комментариев: ${res.status}`);
        if (first && this.known === 0) this.resetOut = true;
        for (const t of list.threads) {
          const was = this.threads.get(t.id);
          if (!was || was.seq <= t.seq) this.upsert(t);
        }
        for (const id of list.removed) this.remove(id);
        this.known = Math.max(this.known, list.seq);
        if (!list.more) break;
      }
      this.loaded = true;
    } catch {
      this.fetching = false;
      if (!this.stopped && this.retryTimer === null)
        this.retryTimer = this.env.clock.setTimeout(() => {
          this.retryTimer = null;
          void this.catchUp();
        }, RETRY_MS);
      return;
    }
    this.fetching = false;
    if (first) {
      this.env.core({ type: 'commentPins', pins: this.pins([...this.threads.values()]), replace: true });
      this.schedule();
    }
    const queued = this.waiting.splice(0).sort((a, b) => a.seq - b.seq);
    for (const ev of queued) {
      if (this.fetching) {
        this.waiting.push(ev);
        continue;
      }
      this.apply(ev);
    }
    // Открытое обсуждение могло измениться, пока не было связи.
    if (this.open && this.threads.has(this.open)) void this.loadDetail(this.open);
    else if (this.open) this.closeDetail(this.open);
  }

  private upsert(t: ThreadView): void {
    this.threads.set(t.id, t);
    this.upserts.set(t.id, t);
    this.removedOut.delete(t.id);
    if (this.loaded) this.env.core({ type: 'commentPins', pins: this.pins([t]), replace: false });
    this.schedule();
  }

  private remove(id: string): void {
    if (!this.threads.delete(id)) return;
    this.upserts.delete(id);
    this.removedOut.add(id);
    this.env.core({ type: 'commentPinsRemove', ids: [id] });
    if (this.open === id) this.closeDetail(id);
    this.schedule();
  }

  private pins(list: ThreadView[]): CommentPinForCore[] {
    return list.map((t) => ({
      id: t.id,
      page: t.pin.page,
      ...(t.pin.node ? { node: t.pin.node } : {}),
      x: t.pin.x,
      y: t.pin.y,
      wx: t.pin.wx,
      wy: t.pin.wy,
      initials: initialsOf(t.author),
      color: authorColor(t.author),
      replies: t.replies,
      unread: t.unread,
      resolved: t.resolvedAt !== null,
      movable: this.role === 'owner' || (t.author.id !== null && t.author.id === this.env.user),
    }));
  }

  private schedule(): void {
    if (this.flushTimer !== null || this.stopped) return;
    this.flushTimer = this.env.clock.setTimeout(() => {
      this.flushTimer = null;
      this.flush();
    }, COMMENTS_FLUSH_MS);
  }

  private flush(): void {
    // Первый список — только целиком: по половине панель показала бы не всё, а ссылка на обсуждение со второй страницы
    // сочла бы его ненайденным. Догрузится — `catchUp` позовёт снова.
    if (!this.loaded) return;
    const reset = this.resetOut;
    const upserts = reset ? [...this.threads.values()] : [...this.upserts.values()];
    const removed = reset ? [] : [...this.removedOut];
    this.upserts.clear();
    this.removedOut.clear();
    this.resetOut = false;
    if (reset || upserts.length > 0 || removed.length > 0) this.env.post({ t: 'threads', reset, upserts, removed });
  }

  /** Обсуждение — для тестов и проб. */
  thread(id: string): ThreadView | undefined {
    return this.threads.get(id);
  }

  get seq(): number {
    return this.known;
  }

  // --- Запросы основного потока ---

  async request(id: number, req: CommentRequest): Promise<void> {
    const result = await this.run(req).catch((): CommentResult => ({ ok: false, failure: 'NETWORK' }));
    this.env.post({ t: 'result', id, result });
  }

  private send(path: string, method: string, body?: unknown): Promise<Response> {
    return this.env.fetch(`${this.env.base}${path}`, {
      method,
      ...(body === undefined ? {} : { headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) }),
    });
  }

  private async write(path: string, method: string, body?: unknown): Promise<CommentResult> {
    const res = await this.send(path, method, body);
    const data = await json(res);
    if (!res.ok) return { ok: false, failure: failureOf(res.status, data) };
    const w = parseCommentWrite(data);
    if (!w) return { ok: false, failure: 'NETWORK' };
    if (w.removed) this.remove(w.thread.id);
    else {
      const was = this.threads.get(w.thread.id);
      if (!was || was.seq < w.thread.seq) this.upsert(w.thread);
      if (this.detail?.thread === w.thread.id && w.comment) {
        const list = this.detail.comments;
        const at = list.findIndex((c) => c.id === w.comment!.id);
        if (at >= 0) list[at] = w.comment;
        else list.push(w.comment);
        this.postDetail();
      }
    }
    return { ok: true, value: w };
  }

  private async run(req: CommentRequest): Promise<CommentResult> {
    switch (req.k) {
      case 'open':
        this.open = req.thread;
        if (req.thread === null) {
          this.detail = null;
          return { ok: true };
        }
        return this.loadDetail(req.thread);
      case 'create':
        return this.write('/comments', 'POST', { pin: req.pin, body: req.body });
      case 'reply':
        return this.write(`/comments/${req.thread}/comments`, 'POST', { body: req.body });
      case 'edit':
        return this.write(`/comments/${req.thread}/comments/${req.comment}`, 'PATCH', { body: req.body });
      case 'delete': {
        const r = await this.write(`/comments/${req.thread}/comments/${req.comment}`, 'DELETE');
        if (r.ok && this.detail?.thread === req.thread) {
          this.detail.comments = this.detail.comments.filter((c) => c.id !== req.comment);
          if (this.threads.has(req.thread)) this.postDetail();
        }
        return r;
      }
      case 'resolve':
        return this.write(`/comments/${req.thread}`, 'PATCH', { resolved: req.resolved });
      case 'move': {
        const r = await this.write(`/comments/${req.thread}`, 'PATCH', { pin: req.pin });
        // Ядро уже поставило метку на новое место: отказ сервера возвращает её назад.
        const t = this.threads.get(req.thread);
        if (!r.ok && t) this.env.core({ type: 'commentPins', pins: this.pins([t]), replace: false });
        return r;
      }
      case 'mentions': {
        const res = await this.env.fetch(`${this.env.base}/mentions?q=${encodeURIComponent(req.q)}`);
        const data = await json(res);
        if (!res.ok) return { ok: false, failure: failureOf(res.status, data) };
        const list = parseMentionList(data);
        return list ? { ok: true, value: list.people satisfies MentionCandidate[] } : { ok: false, failure: 'NETWORK' };
      }
    }
  }

  private async loadDetail(thread: string): Promise<CommentResult> {
    const res = await this.env.fetch(`${this.env.base}/comments/${thread}`);
    const data = await json(res);
    if (this.open !== thread) return { ok: true };
    if (!res.ok) {
      const failure = failureOf(res.status, data);
      if (failure === 'NOT_FOUND') this.closeDetail(thread);
      return { ok: false, failure };
    }
    const d = parseThreadDetail(data);
    if (!d) return { ok: false, failure: 'NETWORK' };
    this.detail = { thread, comments: d.comments };
    const was = this.threads.get(thread);
    // Список мог ещё не дойти (ссылка на обсуждение): обсуждение из ответа, прочитанное.
    if (!was || was.seq <= d.thread.seq) this.upsert({ ...d.thread, unread: false });
    else if (was.unread) this.upsert({ ...was, unread: false });
    this.postDetail();
    if (d.thread.unread || was?.unread) void this.markRead(thread);
    return { ok: true, value: d.thread };
  }

  private postDetail(): void {
    if (!this.detail) return;
    const view = this.threads.get(this.detail.thread) ?? null;
    this.env.post({ t: 'detail', thread: this.detail.thread, view, comments: [...this.detail.comments] });
  }

  private closeDetail(thread: string): void {
    if (this.detail?.thread === thread) this.detail = null;
    this.env.post({ t: 'detail', thread, view: null, comments: [] });
  }

  private async markRead(thread: string): Promise<void> {
    const res = await this.send(`/comments/${thread}/read`, 'POST').catch(() => null);
    if (!res?.ok) return;
    const t = this.threads.get(thread);
    if (t?.unread) this.upsert({ ...t, unread: false });
  }
}

/** Метка обсуждения — из `ThreadDto` без своего состояния (для проб и тестов). */
export const viewOf = (t: ThreadDto, unread = false): ThreadView => ({ ...t, unread });
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
  syncPath,
  type Affine6,
  type GestureKind,
  type PresenceItem,
````

на:

````ts
  syncPath,
  type Affine6,
  type CommentEvent,
  type FileRole,
  type GestureKind,
  type PresenceItem,
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
  type SyncRefusal,
} from '@ferrum/protocol';
import type { AccessState, Clock } from './server-files';

````

на:

````ts
  type SyncRefusal,
} from '@ferrum/protocol';
import type { CommentsCoreCommand } from './comments-store';
import type { AccessState, Clock } from './server-files';

````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
  | { type: 'presence'; items: PresenceItemForCore[] }
  /** Соединение заново: прежние участники уходят у ядра, своё состояние оно скажет снова. */
  | { type: 'presenceReset' };

/** Выделение и жест в кадре присутствия — те же поля, что у `PresenceSelection` и `PresenceGesture` протокола. */
````

на:

````ts
  | { type: 'presence'; items: PresenceItemForCore[] }
  /** Соединение заново: прежние участники уходят у ядра, своё состояние оно скажет снова. */
  | { type: 'presenceReset' }
  /** Метки комментариев (спека подсистемы 4, раздел 10.2): их ведёт `CommentStore`. */
  | CommentsCoreCommand;

/** Выделение и жест в кадре присутствия — те же поля, что у `PresenceSelection` и `PresenceGesture` протокола. */
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
  /** Участники файла для шапки: не чаще `PARTICIPANTS_MS` и только при изменении. */
  participants(list: Participant[]): void;
}

````

на:

````ts
  /** Участники файла для шапки: не чаще `PARTICIPANTS_MS` и только при изменении. */
  participants(list: Participant[]): void;
  /** Комментарии файла (спека подсистемы 4, раздел 10): роль подтверждена этим соединением — догнать пропущенное; событие сокета. */
  comments?: { connected(role: FileRole): void; event(ev: CommentEvent): void };
}

````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
          this.armHeartbeat();
        }
        break;
      case 'presence':
````

на:

````ts
          this.armHeartbeat();
        }
        // Новое соединение (или новая роль): события, пропущенные без связи, комментарии догоняют по `seq`.
        this.env.comments?.connected(f.role);
        break;
      case 'comment':
        this.env.comments?.event(f.event);
        break;
      case 'presence':
````

В `apps/web/src/core/sync-messages.ts` заменить:

````ts
import type { CoreCommand, CoreSyncEvent, Participant, SyncStatus } from './sync-client';
import type { AccessState } from './server-files';

export type ToSync = { t: 'start'; file: string; user: string; schema: number; port: MessagePort } | { t: 'stop' };

export type FromSync =
````

на:

````ts
import type { CoreCommand, CoreSyncEvent, Participant, SyncStatus } from './sync-client';
import type { AccessState } from './server-files';
import type { CommentRequest, CommentsToMain } from './comments-store';

export type ToSync =
  | { t: 'start'; file: string; user: string; schema: number; port: MessagePort }
  | { t: 'stop' }
  /** Запрос комментариев (спека подсистемы 4, раздел 10): ответ — `{ t: 'comments', m: { t: 'result', id } }`. */
  | { t: 'comments'; id: number; req: CommentRequest };

export type FromSync =
````

В `apps/web/src/core/sync-messages.ts` заменить:

````ts
  | { t: 'reload' }
  /** Участники файла для шапки (спека подсистемы 4, раздел 9): не чаще четырёх раз в секунду. */
  | { t: 'participants'; list: Participant[] };

/** Порт между worker'ом ядра и `sync.worker`. `applied` и `reset` — ядро выполнило `applyRemote` и `syncReset`. */
````

на:

````ts
  | { t: 'reload' }
  /** Участники файла для шапки (спека подсистемы 4, раздел 9): не чаще четырёх раз в секунду. */
  | { t: 'participants'; list: Participant[] }
  /** Комментарии: изменившиеся обсуждения, открытое обсуждение, ответы на запросы. */
  | { t: 'comments'; m: CommentsToMain };

/** Порт между worker'ом ядра и `sync.worker`. `applied` и `reset` — ядро выполнило `applyRemote` и `syncReset`. */
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
import type { FileMeta, FromIo, IoOp, IoResults, JournalMark, Section, ToIo } from '../core/file-list';
import type { AccessState } from '../core/server-files';
import type { Participant, SyncStatus } from '../core/sync-client';
import { nameFromFile } from '../core/file-list';
````

на:

````ts
import type { FileMeta, FromIo, IoOp, IoResults, JournalMark, Section, ToIo } from '../core/file-list';
import type { AccessState } from '../core/server-files';
import type { CommentRequest, CommentResult, CommentsToMain } from '../core/comments-store';
import type { Participant, SyncStatus } from '../core/sync-client';
import { nameFromFile } from '../core/file-list';
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  /** Участники файла для шапки (спека подсистемы 4, раздел 9). */
  participants(list: Participant[]): void;
}

````

на:

````ts
  /** Участники файла для шапки (спека подсистемы 4, раздел 9). */
  participants(list: Participant[]): void;
  /** Комментарии файла (спека подсистемы 4, раздел 10): только у файлов на сервере. */
  comments?(m: CommentsToMain): void;
}

/** Соединение файла: остановить; запросы комментариев — где они есть. */
export interface SyncHandle {
  stop(): void;
  comments?(req: CommentRequest): Promise<CommentResult>;
}

````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  readonly user?: string;
  /** Соединение файла (спека подсистемы 4, раздел 7): правки идут журналом сервера, файл целиком пишется только первый раз. */
  sync?(id: string, core: SyncCore, events: SyncEvents): { stop(): void };
  /** Сервер подтвердил правки: снять пачки журнала браузера. */
  cutJournal?(id: string, mark: JournalMark): Promise<void>;
````

на:

````ts
  readonly user?: string;
  /** Соединение файла (спека подсистемы 4, раздел 7): правки идут журналом сервера, файл целиком пишется только первый раз. */
  sync?(id: string, core: SyncCore, events: SyncEvents): SyncHandle;
  /** Сервер подтвердил правки: снять пачки журнала браузера. */
  cutJournal?(id: string, mark: JournalMark): Promise<void>;
````

В `apps/web/src/ui/server/server-store.ts` заменить:

````ts
import type { AccessState } from '../../core/server-files';
import type { FromServer, ServerOp, ServerResults, ToServer } from '../../core/server-messages';
import type { FromSync, ToSync } from '../../core/sync-messages';
import { IdbFileStore, type FileStore, type SyncCore, type SyncEvents } from '../file-store';
````

на:

````ts
import type { AccessState } from '../../core/server-files';
import type { FromServer, ServerOp, ServerResults, ToServer } from '../../core/server-messages';
import type { CommentRequest, CommentResult } from '../../core/comments-store';
import type { FromSync, ToSync } from '../../core/sync-messages';
import { IdbFileStore, type FileStore, type SyncCore, type SyncEvents } from '../file-store';
````

В `apps/web/src/ui/server/server-store.ts` заменить:

````ts
    const channel = new MessageChannel();
    core.connectSync(channel.port1);
    worker.onmessage = (e: MessageEvent<FromSync>) => {
      const m = e.data;
````

на:

````ts
    const channel = new MessageChannel();
    core.connectSync(channel.port1);
    /** Запросы комментариев в `sync.worker`: ответ приходит тем же порядком сообщений, по номеру. */
    const pending = new Map<number, (r: CommentResult) => void>();
    let nextId = 0;
    worker.onmessage = (e: MessageEvent<FromSync>) => {
      const m = e.data;
````

В `apps/web/src/ui/server/server-store.ts` заменить:

````ts
      else if (m.t === 'reload') events.reload();
      else if (m.t === 'participants') events.participants(m.list);
      else for (const w of this.watchers.get(file) ?? []) w(m.state);
    };
    worker.onerror = (e) => {
````

на:

````ts
      else if (m.t === 'reload') events.reload();
      else if (m.t === 'participants') events.participants(m.list);
      else if (m.t === 'comments') {
        if (m.m.t === 'result') {
          pending.get(m.m.id)?.(m.m.result);
          pending.delete(m.m.id);
        } else events.comments?.(m.m);
      } else for (const w of this.watchers.get(file) ?? []) w(m.state);
    };
    worker.onerror = (e) => {
````

В `apps/web/src/ui/server/server-store.ts` заменить:

````ts
        core.disconnectSync();
        worker.terminate();
      },
    };
  }
````

на:

````ts
        core.disconnectSync();
        worker.terminate();
        for (const done of pending.values()) done({ ok: false, failure: 'NETWORK' });
        pending.clear();
      },
      comments: (req: CommentRequest) =>
        new Promise<CommentResult>((resolve) => {
          const id = ++nextId;
          pending.set(id, resolve);
          worker.postMessage({ t: 'comments', id, req } satisfies ToSync);
        }),
    };
  }
````

В `apps/web/src/worker/sync.worker.ts` заменить:

````ts

import { asUser, ServerFiles, type JournalStore } from '../core/server-files';
import { SyncClient, type CoreCommand, type SocketLike } from '../core/sync-client';
import type { CoreToSync, FromSync, SyncToCore, ToSync } from '../core/sync-messages';
````

на:

````ts

import { asUser, ServerFiles, type JournalStore } from '../core/server-files';
import { CommentStore } from '../core/comments-store';
import { SyncClient, type CoreCommand, type SocketLike } from '../core/sync-client';
import type { CoreToSync, FromSync, SyncToCore, ToSync } from '../core/sync-messages';
````

В `apps/web/src/worker/sync.worker.ts` заменить:

````ts

let client: SyncClient | null = null;

function transferOf(c: CoreCommand): Transferable[] {
````

на:

````ts

let client: SyncClient | null = null;
let comments: CommentStore | null = null;

function transferOf(c: CoreCommand): Transferable[] {
````

В `apps/web/src/worker/sync.worker.ts` заменить:

````ts
  if (m.t === 'stop') {
    client?.stop();
    client = null;
    return;
  }
````

на:

````ts
  if (m.t === 'stop') {
    client?.stop();
    comments?.stop();
    client = comments = null;
    return;
  }
  if (m.t === 'comments') {
    if (comments) void comments.request(m.id, m.req);
    else post({ t: 'comments', m: { t: 'result', id: m.id, result: { ok: false, failure: 'NETWORK' } } });
    return;
  }
````

В `apps/web/src/worker/sync.worker.ts` заменить:

````ts
    },
  );
  client = new SyncClient({
    file: m.file,
````

на:

````ts
    },
  );
  const clock = {
    now: () => performance.now(),
    setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms),
    clearTimeout: (t: unknown) => clearTimeout(t as ReturnType<typeof setTimeout>),
  };
  const fetchAs = asUser(m.user, (input, init) => fetch(input, init));
  comments = new CommentStore({
    user: m.user,
    base: `/api/files/${m.file}`,
    fetch: (path, init) => fetchAs(path, init),
    core: (cmd) => toCore({ t: 'cmd', cmd }),
    post: (c) => post({ t: 'comments', m: c }),
    clock,
  });
  const store = comments;
  client = new SyncClient({
    file: m.file,
````

В `apps/web/src/worker/sync.worker.ts` заменить:

````ts
    reload: () => post({ t: 'reload' }),
    participants: (list) => post({ t: 'participants', list }),
  });
  port.onmessage = (ev: MessageEvent<CoreToSync>) => {
````

на:

````ts
    reload: () => post({ t: 'reload' }),
    participants: (list) => post({ t: 'participants', list }),
    comments: { connected: (role) => store.connected(role), event: (ev) => store.event(ev) },
  });
  port.onmessage = (ev: MessageEvent<CoreToSync>) => {
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Expected: PASS: всё без замечаний; `unit` 338 passed; обе сборки собираются

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/src/core/comments-store.ts apps/web/src/core/sync-client.ts apps/web/src/core/sync-messages.ts apps/web/src/ui/file-store.ts apps/web/src/ui/server/server-store.ts apps/web/src/worker/sync.worker.ts apps/web/unit/comments-store.test.ts apps/web/unit/sync-client.test.ts
git commit -F - <<'EOF'
клиент: комментарии в sync.worker — догоняние по seq, метки ядру, запросы окна

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 10: Клиент — метки комментариев не будят сборку пачек

Метки приходят с каждым комментарием файла; как присутствие, они правок не рождают, и сборка пачек ядра на них не просыпается.

**Files:**
- Modify: `apps/web/src/core/sync-link.ts`
- Test: `apps/web/unit/sync-link.test.ts`

**Interfaces:**
- Consumes: `CommentsCoreCommand` задачи 9.
- Produces: `QUIET` в `sync-link.ts` (`presence`, `presenceReset`, `commentsAvailable`, `commentPins`, `commentPinsRemove`).

- [ ] **Step 1: Написать тесты**

В `apps/web/unit/sync-link.test.ts` заменить:

````ts
  expect(port.posted).toEqual([{ t: 'reset' }, { t: 'applied' }]);
});
````

на:

````ts
  expect(port.posted).toEqual([{ t: 'reset' }, { t: 'applied' }]);
});

test('присутствие и метки комментариев не будят сборку пачек', () => {
  const s = stand();
  const port = new FakePort();
  s.link.connect(port);
  port.deliver({ t: 'flush', on: true });
  expect(s.flushes()).toBe(1);
  port.deliver({ t: 'cmd', cmd: { type: 'presenceReset' } });
  port.deliver({ t: 'cmd', cmd: { type: 'commentsAvailable', on: true } });
  port.deliver({ t: 'cmd', cmd: { type: 'commentPins', pins: [], replace: true } });
  port.deliver({ t: 'cmd', cmd: { type: 'commentPinsRemove', ids: ['a'] } });
  s.tick();
  expect(s.flushes()).toBe(1);
  expect(s.runs).toContain('commentPins');
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/sync-link.test.ts`
Expected: FAIL — 1 failed, 4 passed: «присутствие и метки комментариев не будят сборку пачек» — `commentPins` будит сборку

- [ ] **Step 3: Реализация**

В `apps/web/src/core/sync-link.ts` заменить:

````ts
  clearInterval(t: unknown): void;
}

export class SyncLink {
````

на:

````ts
  clearInterval(t: unknown): void;
}

/** Команды `sync.worker`, после которых собирать нечего. */
const QUIET = new Set<SyncToCore extends infer M ? (M extends { t: 'cmd'; cmd: { type: infer T } } ? T : never) : never>([
  'presence',
  'presenceReset',
  'commentsAvailable',
  'commentPins',
  'commentPinsRemove',
]);

export class SyncLink {
````

В `apps/web/src/core/sync-link.ts` заменить:

````ts
      return;
    }
    // Присутствие правок не рождает: сборка пачек на него не просыпается (оно приходит 30 раз в секунду).
    if (m.cmd.type !== 'presence' && m.cmd.type !== 'presenceReset') this.wanted = true;
    this.env.run(m.cmd);
    // Порядок порта: всё, что ядро пришлёт после этих ответов, — после команды.
````

на:

````ts
      return;
    }
    // Присутствие и метки комментариев правок не рождают: сборка пачек на них не просыпается (присутствие приходит 30 раз в
    // секунду, метки — с каждым комментарием файла).
    if (!QUIET.has(m.cmd.type)) this.wanted = true;
    this.env.run(m.cmd);
    // Порядок порта: всё, что ядро пришлёт после этих ответов, — после команды.
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Expected: PASS: всё без замечаний; `unit` 339 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/src/core/sync-link.ts apps/web/unit/sync-link.test.ts
git commit -F - <<'EOF'
клиент: метки комментариев не будят сборку пачек ядра

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 11: Интерфейс — окно у метки, поле с упоминаниями, панель, ссылка, настройка писем

Спека §10.3; «Решения» 12–15, 24. Окно у метки, поле (Enter, Shift+Enter, Esc, `@`), удаление со вторым щелчком, «Закрыть обсуждение»/«Открыть снова»; панель справа в режиме — виртуальный список с фильтрами; `?comment=`; выключатель «Письма о комментариях» в «Настройки → Аккаунт»; в автономной сборке кнопка отключена с подсказкой.

**Files:**
- Create: `apps/web/src/ui/server/comments-logic.ts`, `apps/web/src/ui/server/comments.tsx`
- Modify: `apps/web/src/ui/app.tsx`, `apps/web/src/ui/auth/account.tsx`, `apps/web/src/ui/extensions.ts`, `apps/web/src/ui/root.tsx`, `apps/web/src/ui/router.ts`, `apps/web/src/ui/server/server-api.ts`, `apps/web/src/ui/server/setup.tsx`, `apps/web/src/ui/styles/server.css`, `apps/web/src/ui/toolbar.tsx`
- Test: `apps/web/e2e/chrome.spec.ts`, `apps/web/unit/comments-logic.test.ts` (новый), `apps/web/unit/router.test.ts`

**Interfaces:**
- Consumes: `SyncHandle.comments`, `SyncEvents.comments` задачи 9; команды и события ядра задач 2 и 4; `/api/me/prefs` задачи 5.
- Produces: `CommentsUi`, `CommentsProps`, `extensions.comments`, `PANEL_ROW = 76`; `PanelFilter`, `DEFAULT_FILTER`, `panelThreads`, `listWindow`, `authorName`, `mentionAt`, `insertMention`, `storedBody`, `editableBody`, `bodyState`, `timeLabel`; маршрут `{kind:'file', comment?}`; `getPrefs`, `setPrefs`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e/chrome.spec.ts` заменить:

````ts
  await page.getByRole('button', { name: 'Перо', exact: true }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перо  P');
  await page.getByRole('button', { name: 'Комментарии — подсистема 4' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Комментарии  C — подсистема 4');
  // Открытая подсказка следует за раскладкой: V снят, Q назначен — текст меняется на месте.
  await move.hover();
````

на:

````ts
  await page.getByRole('button', { name: 'Перо', exact: true }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перо  P');
  await page.getByRole('button', { name: 'Комментарии — только в файлах на сервере' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Комментарии  C — только в файлах на сервере');
  // Открытая подсказка следует за раскладкой: V снят, Q назначен — текст меняется на месте.
  await move.hover();
````

Создать `apps/web/unit/comments-logic.test.ts`:

````ts
// Панель и поле комментариев: фильтры и порядок, окно виртуального списка, упоминания и длина текста (та же, что у сервера).

import { expect, test } from '@playwright/test';
import { COMMENT_BODY_MAX, normalizeCommentBody, type ThreadView } from '@ferrum/protocol';
import {
  authorName,
  bodyState,
  DEFAULT_FILTER,
  editableBody,
  insertMention,
  listWindow,
  mentionAt,
  panelThreads,
  storedBody,
} from '../src/ui/server/comments-logic';

const t = (id: string, o: Partial<ThreadView> & { page?: string }): ThreadView => {
  const author = o.author ?? { id: 'a', name: 'Анна' };
  return {
    id,
    pin: { page: o.page ?? '1@1', node: null, x: 0, y: 0, wx: 0, wy: 0 },
    author,
    createdAt: 0,
    resolvedAt: null,
    resolvedBy: null,
    seq: 1,
    first: { id: `${id}c`, thread: id, author, body: 'текст', createdAt: 0, editedAt: null, mentions: {} },
    replies: 0,
    lastAt: 0,
    participants: [author.id ?? ''],
    unread: false,
    ...o,
  };
};

test('панель: страница, закрытые скрыты по умолчанию, «только мои», новые сверху и непрочитанные первыми', () => {
  const list = [
    t('1', { lastAt: 10 }),
    t('2', { lastAt: 30, page: '2@1' }),
    t('3', { lastAt: 20, resolvedAt: 5 }),
    t('4', { lastAt: 5, unread: true, participants: ['a', 'me'] }),
  ];
  expect(panelThreads(list, DEFAULT_FILTER, '1@1', 'me').map((x) => x.id)).toEqual(['1', '4']);
  expect(panelThreads(list, { ...DEFAULT_FILTER, scope: 'all', resolved: true }, '1@1', 'me').map((x) => x.id)).toEqual([
    '2',
    '3',
    '1',
    '4',
  ]);
  expect(panelThreads(list, { ...DEFAULT_FILTER, sort: 'unread' }, '1@1', 'me').map((x) => x.id)).toEqual(['4', '1']);
  expect(panelThreads(list, { ...DEFAULT_FILTER, mine: true }, '1@1', 'me').map((x) => x.id)).toEqual(['4']);
});

test('окно списка: 1000 обсуждений — рисуются только видимые строки с запасом', () => {
  expect(listWindow(0, 600, 76, 1000)).toEqual({ start: 0, end: 12 });
  expect(listWindow(76 * 500, 600, 76, 1000)).toEqual({ start: 496, end: 512 });
  expect(listWindow(76 * 2000, 600, 76, 1000)).toEqual({ start: 1000, end: 1000 });
});

test('упоминание: набор после @, вставка имени, на сервер — @[id]; правка возвращает имена', () => {
  expect(mentionAt('привет @ан', 10)).toEqual({ start: 7, query: 'ан' });
  expect(mentionAt('mail@example', 12)).toBeNull();
  expect(mentionAt('@', 1)).toEqual({ start: 0, query: '' });
  const ins = insertMention('привет @ан', 7, 10, 'Анна Мария');
  expect(ins).toEqual({ text: 'привет @Анна Мария ', caret: 19 });
  const picked = [
    { id: 'u1', name: 'Анна' },
    { id: 'u2', name: 'Анна Мария' },
  ];
  expect(storedBody('@Анна Мария и @Анна, а @Борис нет', picked)).toBe('@[u2] и @[u1], а @Борис нет');
  expect(editableBody('@[u2] и @[zz]', { u2: 'Анна Мария' })).toEqual({
    text: '@Анна Мария и @[zz]',
    picked: [{ id: 'u2', name: 'Анна Мария' }],
  });
});

test('длина — в кодовых точках, как у сервера: 4000 эмодзи можно, 4001 — нет', () => {
  const max = '😀'.repeat(COMMENT_BODY_MAX);
  expect(bodyState(max)).toEqual({ ok: true, length: COMMENT_BODY_MAX, over: false });
  expect(normalizeCommentBody(max)).toBe(max);
  expect(bodyState(max + '😀').ok).toBe(false);
  expect(normalizeCommentBody(max + '😀')).toBeNull();
  expect(bodyState('   ').ok).toBe(false);
});

test('подпись автора: удалённый и без имени', () => {
  expect(authorName({ id: null, name: 'x' })).toBe('Удалённый пользователь');
  expect(authorName({ id: 'a', name: ' ' })).toBe('Участник');
  expect(authorName({ id: 'a', name: '<b>Ив</b>' })).toBe('<b>Ив</b>');
});
````

В `apps/web/unit/router.test.ts` заменить:

````ts
  expect(routeHash({ kind: 'file', id: 'abc', link: 'x' })).toBe('#/file/abc');
});
````

на:

````ts
  expect(routeHash({ kind: 'file', id: 'abc', link: 'x' })).toBe('#/file/abc');
});

test('ссылка на обсуждение: `?comment=` — только UUID', () => {
  const id = '0f8fad5b-d9cb-469f-a165-70867728950e';
  expect(parseRoute(`#/file/abc?comment=${id}`)).toEqual({ kind: 'file', id: 'abc', comment: id });
  expect(parseRoute('#/file/abc?comment=../x')).toEqual({ kind: 'file', id: 'abc' });
  expect(parseRoute('#/file/abc?comment=0F8FAD5B-D9CB-469F-A165-70867728950E')).toEqual({ kind: 'file', id: 'abc' });
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/comments-logic.test.ts unit/router.test.ts`
Expected: FAIL — нет модуля `src/ui/server/comments-logic`: Playwright не загружает файл тестов, ни один тест не запущен

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { buildContextMenu, type ContextTarget } from './context-menu';
import { EditorContext, type Editor } from './context';
import { ConfirmDialog } from './dialog';
import { Menu, type MenuItem, type MenuSearch } from './menu';
````

на:

````tsx
import { buildContextMenu, type ContextTarget } from './context-menu';
import { EditorContext, type Editor } from './context';
import { extensions } from './extensions';
import { ConfirmDialog } from './dialog';
import { Menu, type MenuItem, type MenuSearch } from './menu';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
import { ShortcutsPanel } from './shortcuts-panel';
import { downloadBytes, fileName } from './files';
import { importFromDisk, type FileMeta, type FileStore } from './file-store';
import { systemFonts } from './system-fonts';
import { isTauri, saveExport } from './tauri';
````

на:

````tsx
import { ShortcutsPanel } from './shortcuts-panel';
import { downloadBytes, fileName } from './files';
import { importFromDisk, type FileMeta, type FileStore, type SyncHandle } from './file-store';
import type { CommentsToMain } from '../core/comments-store';
import { systemFonts } from './system-fonts';
import { isTauri, saveExport } from './tauri';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  /** Страница из ссылки (`?page=`): открыть её, если она есть в файле. */
  page?: string;
}

````

на:

````tsx
  /** Страница из ссылки (`?page=`): открыть её, если она есть в файле. */
  page?: string;
  /** Обсуждение из ссылки (`?comment=`, письмо о комментарии): открыть его, когда комментарии загрузятся. */
  comment?: string;
}

````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  /** Сервер не принял правки: причина для плашки. */
  const [rejected, setRejected] = createSignal('');
  let syncHandle: { stop(): void } | undefined;
  /** Просьба о доступе из плашки «Нет доступа». */
  const [asked, setAsked] = createSignal(false);
````

на:

````tsx
  /** Сервер не принял правки: причина для плашки. */
  const [rejected, setRejected] = createSignal('');
  let syncHandle: SyncHandle | undefined;
  /** Комментарии из `sync.worker` (спека подсистемы 4, раздел 10): получатели — окно и панель. */
  const commentListeners = new Set<(m: CommentsToMain) => void>();
  const [commentsHost, setCommentsHost] = createSignal<HTMLElement>();
  const commentMode = () => store.state.tool === 'comment' && !!extensions.comments && !!props.files.sync;
  /** Просьба о доступе из плашки «Нет доступа». */
  const [asked, setAsked] = createSignal(false);
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
            },
            participants: (list) => store.setState('participants', reconcile(list, { key: 'id' })),
          });
        };
````

на:

````tsx
            },
            participants: (list) => store.setState('participants', reconcile(list, { key: 'id' })),
            comments: (m) => {
              for (const l of commentListeners) l(m);
            },
          });
        };
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
              <EditorContext.Provider value={ed()}>
                <Toolbar />
                <Show when={palette()}>
                  <QuickActions hidden={hidden()} recent={recent()} onClose={() => setPalette(false)} />
````

на:

````tsx
              <EditorContext.Provider value={ed()}>
                <Toolbar />
                <Show when={extensions.comments && props.files.sync ? extensions.comments : null}>
                  {(Comments) => {
                    const C = Comments();
                    return (
                      <C
                        editor={ed()}
                        canvas={canvas}
                        request={(req) => syncHandle?.comments?.(req) ?? Promise.resolve({ ok: false, failure: 'NETWORK' })}
                        subscribe={(l) => {
                          commentListeners.add(l);
                          return () => commentListeners.delete(l);
                        }}
                        panelHost={commentsHost}
                        mode={commentMode}
                        page={() => store.state.pages.current}
                        link={props.comment}
                        toast={(text) => store.toast(text)}
                      />
                    );
                  }}
                </Show>
                <Show when={palette()}>
                  <QuickActions hidden={hidden()} recent={recent()} onClose={() => setPalette(false)} />
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
                <Show when={!rightCollapsed()}>
                  <DesignHeader file={props.file} self={props.files.user} />
                  <PropertiesPanel theme={theme()} />
                  <ResizeEdge side="right" width={right()} onWidth={setRight} />
                </Show>
````

на:

````tsx
                <Show when={!rightCollapsed()}>
                  <DesignHeader file={props.file} self={props.files.user} />
                  <Show when={commentMode()} fallback={<PropertiesPanel theme={theme()} />}>
                    <div ref={setCommentsHost} class="fe-comments-host" />
                  </Show>
                  <ResizeEdge side="right" width={right()} onWidth={setRight} />
                </Show>
````

В `apps/web/src/ui/auth/account.tsx` заменить:

````tsx
// Человек в интерфейсе: кружок с инициалами, вкладка настроек и строка на экране файлов.

import { createSignal, Show } from 'solid-js';
import { avatarHue, initials } from './auth-logic';
import { currentUser, leave } from './session';
````

на:

````tsx
// Человек в интерфейсе: кружок с инициалами, вкладка настроек и строка на экране файлов.

import { createSignal, onMount, Show } from 'solid-js';
import { apiFailureText, getPrefs, setPrefs } from '../server/server-api';
import { avatarHue, initials } from './auth-logic';
import { currentUser, leave } from './session';
````

В `apps/web/src/ui/auth/account.tsx` заменить:

````tsx
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const out = async (everywhere: boolean) => {
    setBusy(true);
````

на:

````tsx
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  // «Письма о комментариях»: по умолчанию включены; пока ответа сервера нет — выключатель недоступен.
  const [mail, setMail] = createSignal<boolean | null>(null);
  onMount(() => {
    void getPrefs().then((r) => setMail(r.ok ? r.value.commentMail : null));
  });
  const toggleMail = async (on: boolean) => {
    setMail(on);
    const r = await setPrefs({ commentMail: on });
    if (!r.ok) {
      setMail(!on);
      setError(apiFailureText(r.failure));
    }
  };
  const out = async (everywhere: boolean) => {
    setBusy(true);
````

В `apps/web/src/ui/auth/account.tsx` заменить:

````tsx
        )}
      </Show>
      <div class="fe-account__actions">
        <button class="fe-btn fe-btn--secondary" disabled={busy()} onClick={() => void out(false)}>
````

на:

````tsx
        )}
      </Show>
      <label class="fe-account__pref">
        <input
          type="checkbox"
          checked={mail() ?? true}
          disabled={mail() === null}
          onChange={(e) => void toggleMail(e.currentTarget.checked)}
        />
        Письма о комментариях: упоминания и ответы, пока файл не открыт
      </label>
      <div class="fe-account__actions">
        <button class="fe-btn fe-btn--secondary" disabled={busy()} onClick={() => void out(false)}>
````

В `apps/web/src/ui/extensions.ts` заменить:

````ts
import type { Component, JSX } from 'solid-js';
import type { FileMeta, Section } from '../core/file-list';
import type { Participant } from '../core/sync-client';
import type { IconName } from './icons';
import type { MenuItem } from './menu';
````

на:

````ts
import type { Component, JSX } from 'solid-js';
import type { FileMeta, Section } from '../core/file-list';
import type { CommentRequest, CommentResult, CommentsToMain } from '../core/comments-store';
import type { Participant } from '../core/sync-client';
import type { Editor } from './context';
import type { IconName } from './icons';
import type { MenuItem } from './menu';
````

В `apps/web/src/ui/extensions.ts` заменить:

````ts
  currentPage: () => string;
  follow: (participant: number | null) => void;
}

````

на:

````ts
  currentPage: () => string;
  follow: (participant: number | null) => void;
}

/** Комментарии открытого файла (спека подсистемы 4, раздел 10): окно у метки и панель справа в режиме комментариев. */
export interface CommentsProps {
  editor: Editor;
  canvas: HTMLCanvasElement;
  /** Запрос в `sync.worker`; соединения ещё нет — `NETWORK`. */
  request(req: CommentRequest): Promise<CommentResult>;
  subscribe(listener: (m: CommentsToMain) => void): () => void;
  /** Где панель: правая колонка в режиме комментариев. */
  panelHost: () => HTMLElement | undefined;
  mode: () => boolean;
  page: () => string;
  /** Обсуждение из ссылки (`?comment=`). */
  link?: string;
  toast(text: string): void;
}

````

В `apps/web/src/ui/extensions.ts` заменить:

````ts
  /** Аватары участников открытого файла (спека подсистемы 4, раздел 9). */
  presenceBar?: Component<PresenceBarProps>;
} = {};

````

на:

````ts
  /** Аватары участников открытого файла (спека подсистемы 4, раздел 9). */
  presenceBar?: Component<PresenceBarProps>;
  /** Комментарии: только у файлов на сервере (в автономной сборке их нет, как в Figma без сети). */
  comments?: Component<CommentsProps>;
} = {};

````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
  /** `restart` — редактор запущен заново после сбоя; `backend` — бэкенд рендера
   * (после потери GPU — запасной, и для следующих перезапусков тоже). */
  | { kind: 'editor'; file: FileMeta; restart?: Restart; backend?: Backend; page?: string }
  /** Файл на диске без доступа (браузер перезапущен): вопрос, ответ — по щелчку. */
  | { kind: 'access'; file: FileMeta; answer: (ok: boolean) => void }
````

на:

````tsx
  /** `restart` — редактор запущен заново после сбоя; `backend` — бэкенд рендера
   * (после потери GPU — запасной, и для следующих перезапусков тоже). */
  | { kind: 'editor'; file: FileMeta; restart?: Restart; backend?: Backend; page?: string; comment?: string }
  /** Файл на диске без доступа (браузер перезапущен): вопрос, ответ — по щелчку. */
  | { kind: 'access'; file: FileMeta; answer: (ok: boolean) => void }
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx

  /** Открыть файл в редакторе: занят другой вкладкой — к экрану файлов. */
  async function edit(file: FileMeta, my: number, page?: string): Promise<void> {
    // Файл на диске: доступ к нему после перезапуска браузера снова разрешает
    // пользователь — щелчком в вопросе (запросу нужно действие пользователя).
````

на:

````tsx

  /** Открыть файл в редакторе: занят другой вкладкой — к экрану файлов. */
  async function edit(file: FileMeta, my: number, page?: string, comment?: string): Promise<void> {
    // Файл на диске: доступ к нему после перезапуска браузера снова разрешает
    // пользователь — щелчком в вопросе (запросу нужно действие пользователя).
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
    current = { kind: 'file', id: file.id };
    replace(current);
    setView({ kind: 'editor', file, page });
  }

````

на:

````tsx
    current = { kind: 'file', id: file.id };
    replace(current);
    setView({ kind: 'editor', file, page, comment });
  }

````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
        // Файл из корзины для правки не открывается: сначала восстановить.
        else if (file.trashedAt !== undefined) toFiles('Файл в корзине — восстановите его, чтобы открыть');
        else await edit(file, my, route.page);
      }
    } catch (e) {
````

на:

````tsx
        // Файл из корзины для правки не открывается: сначала восстановить.
        else if (file.trashedAt !== undefined) toFiles('Файл в корзине — восстановите его, чтобы открыть');
        else await edit(file, my, route.page, route.comment);
      }
    } catch (e) {
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
            backend={v.backend}
            page={v.page}
            onRestart={(r) => restart(v.file, r, v.backend)}
            navigate={navigate}
````

на:

````tsx
            backend={v.backend}
            page={v.page}
            comment={v.comment}
            onRestart={(r) => restart(v.file, r, v.backend)}
            navigate={navigate}
````

В `apps/web/src/ui/router.ts` заменить:

````ts
// - `#/` (и пустой) — экран файлов, «Недавние»; `#/drafts`, `#/trash` — другие разделы;
// - `#/file/<id>` — редактор с файлом из хранилища; `#/file/<id>?page=<pageId>` —
//   с этой страницей («Копировать ссылку на страницу»);
// - `#/new` — создать файл в хранилище и открыть его (адрес заменяется на `#/file/<id>`);
//   `#/new?project=<id>` — в этом проекте, где проекты есть;
````

на:

````ts
// - `#/` (и пустой) — экран файлов, «Недавние»; `#/drafts`, `#/trash` — другие разделы;
// - `#/file/<id>` — редактор с файлом из хранилища; `#/file/<id>?page=<pageId>` —
//   с этой страницей («Копировать ссылку на страницу»); `#/file/<id>?comment=<threadId>` — на обсуждении (ссылка из письма,
//   спека подсистемы 4, раздел 10.3);
// - `#/new` — создать файл в хранилище и открыть его (адрес заменяется на `#/file/<id>`);
//   `#/new?project=<id>` — в этом проекте, где проекты есть;
````

В `apps/web/src/ui/router.ts` заменить:

````ts

export type Route =
  { kind: 'files'; section: Section } | { kind: 'file'; id: string; page?: string; link?: string } | { kind: 'new'; project?: string };

const SECTION_PATH: Partial<Record<Section, string>> = { recent: '', drafts: 'drafts', trash: 'trash', shared: 'shared' };
````

на:

````ts

export type Route =
  | { kind: 'files'; section: Section }
  | { kind: 'file'; id: string; page?: string; link?: string; comment?: string }
  | { kind: 'new'; project?: string };

const SECTION_PATH: Partial<Record<Section, string>> = { recent: '', drafts: 'drafts', trash: 'trash', shared: 'shared' };
````

В `apps/web/src/ui/router.ts` заменить:

````ts
    const link = new URLSearchParams(query).get('link');
    if (link !== null && /^[A-Za-z0-9_-]{1,64}$/.test(link)) route.link = link;
    return route;
  }
````

на:

````ts
    const link = new URLSearchParams(query).get('link');
    if (link !== null && /^[A-Za-z0-9_-]{1,64}$/.test(link)) route.link = link;
    // Обсуждение — UUID сервера; другое отбрасывается: файл откроется как обычно.
    const comment = new URLSearchParams(query).get('comment');
    if (comment !== null && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(comment)) route.comment = comment;
    return route;
  }
````

Создать `apps/web/src/ui/server/comments-logic.ts`:

````ts
// Чистая логика панели и окна комментариев (спека подсистемы 4, раздел 10.3): фильтры и порядок панели, окно виртуального
// списка, упоминания в поле ввода. Без DOM: модульные тесты проверяют её отдельно.

import { commentLength, COMMENT_BODY_MAX, MENTION_QUERY_MAX, type CommentAuthor, type ThreadView } from '@ferrum/protocol';

export interface PanelFilter {
  /** `page` — только текущая страница. */
  scope: 'page' | 'all';
  /** `date` — новые сверху; `unread` — сначала непрочитанные, внутри — новые сверху. */
  sort: 'date' | 'unread';
  /** «Только мои»: начал, ответил или упомянут — участник обсуждения. */
  mine: boolean;
  /** Показ закрытых; по умолчанию скрыты, как в Figma **(поведение)**. */
  resolved: boolean;
}

export const DEFAULT_FILTER: PanelFilter = { scope: 'page', sort: 'date', mine: false, resolved: false };

/** Обсуждения панели по фильтру. Порядок устойчивый: при равном времени — по номеру. */
export function panelThreads(list: Iterable<ThreadView>, f: PanelFilter, page: string, me: string): ThreadView[] {
  const out: ThreadView[] = [];
  for (const t of list) {
    if (f.scope === 'page' && t.pin.page !== page) continue;
    if (!f.resolved && t.resolvedAt !== null) continue;
    if (f.mine && !t.participants.includes(me) && t.author.id !== me) continue;
    out.push(t);
  }
  out.sort((a, b) => {
    if (f.sort === 'unread' && a.unread !== b.unread) return a.unread ? -1 : 1;
    return b.lastAt - a.lastAt || b.seq - a.seq;
  });
  return out;
}

/** Окно виртуального списка: какие строки рисовать при прокрутке `top` и высоте `height`; запас — строк сверху и снизу. */
export function listWindow(top: number, height: number, row: number, total: number, spare = 4): { start: number; end: number } {
  const start = Math.min(total, Math.max(0, Math.floor(top / row) - spare));
  const end = Math.min(total, Math.ceil((top + height) / row) + spare);
  return { start, end: Math.max(start, end) };
}

/** Подпись автора: имя; без имени — «Участник»; удалённая учётная запись — «Удалённый пользователь» **(поведение)**. */
export function authorName(a: CommentAuthor | null): string {
  if (!a || a.id === null) return 'Удалённый пользователь';
  return a.name.trim() || 'Участник';
}

/** Упоминание, которое пишут сейчас: `@` и текст до каретки без пробелов. `null` — не пишут. */
export function mentionAt(text: string, caret: number): { start: number; query: string } | null {
  const before = text.slice(0, caret);
  const m = /(^|\s)@([^\s@]*)$/.exec(before);
  if (!m || m[2].length > MENTION_QUERY_MAX) return null;
  return { start: caret - m[2].length - 1, query: m[2] };
}

/** Выбранный человек вставляется как `@Имя ` на место набранного упоминания. */
export function insertMention(text: string, start: number, caret: number, name: string): { text: string; caret: number } {
  const label = `@${name} `;
  return { text: text.slice(0, start) + label + text.slice(caret), caret: start + label.length };
}

/** Текст для сервера: `@Имя` выбранных людей — `@[userId]`; остальное как есть. Длинные имена первыми: «Анна Мария» не станет
 * «Анна» с хвостом. Имя, которое человек стёр или изменил, остаётся текстом. */
export function storedBody(text: string, picked: { id: string; name: string }[]): string {
  const sorted = [...picked].sort((a, b) => b.name.length - a.name.length);
  let out = '';
  let i = 0;
  while (i < text.length) {
    const hit = text[i] === '@' ? sorted.find((p) => text.startsWith(`@${p.name}`, i)) : undefined;
    if (hit) {
      out += `@[${hit.id}]`;
      i += hit.name.length + 1;
    } else out += text[i++];
  }
  return out;
}

/** Поле правки своего комментария: `@[id]` — обратно в `@Имя` и выбранные. */
export function editableBody(body: string, names: Record<string, string>): { text: string; picked: { id: string; name: string }[] } {
  const picked: { id: string; name: string }[] = [];
  const text = body.replace(/@\[([A-Za-z0-9_-]{1,64})\]/g, (all, id: string) => {
    const name = names[id];
    if (!name) return all;
    if (!picked.some((p) => p.id === id)) picked.push({ id, name });
    return `@${name}`;
  });
  return { text, picked };
}

/** Можно ли отправить: не пусто и не длиннее предела — та же длина в кодовых точках, что считает сервер. */
export function bodyState(stored: string): { ok: boolean; length: number; over: boolean } {
  const length = commentLength(stored.trim());
  return { ok: length > 0 && length <= COMMENT_BODY_MAX, length, over: length > COMMENT_BODY_MAX };
}

/** Время для панели и окна: «только что», минуты, часы, иначе дата. */
export function timeLabel(at: number, now: number): string {
  const s = Math.max(0, Math.round((now - at) / 1000));
  if (s < 60) return 'только что';
  if (s < 3600) return `${Math.floor(s / 60)} мин назад`;
  if (s < 86_400) return `${Math.floor(s / 3600)} ч назад`;
  return new Date(at).toLocaleDateString('ru', { day: 'numeric', month: 'short' });
}
````

Создать `apps/web/src/ui/server/comments.tsx`:

````tsx
// Комментарии в редакторе (спека подсистемы 4, раздел 10.3): окно обсуждения у метки, поле нового комментария и панель обсуждений
// справа в режиме комментариев. Метки, их раскладку и попадание по ним ведёт ядро; данные — `sync.worker` (`CommentStore`).
// Здесь только то, что видно: основной поток получает изменившиеся обсуждения пачкой и не пересобирает панель целиком — строки
// по обсуждению, список виртуальный.
//
// Текст комментариев и имена — только текстом (`{…}` Solid), без `innerHTML`.

import { splitCommentBody, type CommentDto, type FileRole, type MentionCandidate, type PinDto, type ThreadView } from '@ferrum/protocol';
import { batch, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
import { Portal } from 'solid-js/web';
import type { CommentsToMain } from '../../core/comments-store';
import type { CommentsProps } from '../extensions';
import { apiFailureText, type ApiFailure } from './server-api';
import {
  authorName,
  bodyState,
  DEFAULT_FILTER,
  editableBody,
  insertMention,
  listWindow,
  mentionAt,
  panelThreads,
  storedBody,
  timeLabel,
  type PanelFilter,
} from './comments-logic';

/** Высота строки панели: список виртуальный, строки одной высоты. */
export const PANEL_ROW = 76;
/** Подсказка упоминаний ждёт паузы в наборе. */
const MENTION_DEBOUNCE_MS = 150;

const failText = (f: string) => apiFailureText(f as ApiFailure);

function Body(props: { comment: CommentDto }) {
  return (
    <p class="fe-comment__body">
      <For each={splitCommentBody(props.comment.body, props.comment.mentions)}>
        {(part) => ('mention' in part ? <span class="fe-comment__mention">@{part.name ?? 'неизвестный'}</span> : <>{part.text}</>)}
      </For>
    </p>
  );
}

interface ComposerProps {
  label: string;
  placeholder: string;
  initial?: { text: string; picked: { id: string; name: string }[] };
  submit: string;
  mentions(q: string): Promise<MentionCandidate[]>;
  onSubmit(body: string): Promise<boolean>;
  onCancel(): void;
}

/** Поле комментария: Enter — отправить, Shift+Enter — новая строка, Esc — отмена; `@` — список людей с доступом к файлу. */
function Composer(props: ComposerProps) {
  const [text, setText] = createSignal(props.initial?.text ?? '');
  const [busy, setBusy] = createSignal(false);
  const [people, setPeople] = createSignal<MentionCandidate[]>([]);
  const [active, setActive] = createSignal(0);
  let picked = [...(props.initial?.picked ?? [])];
  let field!: HTMLTextAreaElement;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let asked = 0;
  const state = createMemo(() => bodyState(storedBody(text(), picked)));
  onMount(() => {
    field.focus();
    field.setSelectionRange(field.value.length, field.value.length);
  });
  onCleanup(() => clearTimeout(timer));

  const lookup = () => {
    clearTimeout(timer);
    const at = mentionAt(field.value, field.selectionStart);
    if (!at) {
      setPeople([]);
      return;
    }
    const mine = ++asked;
    timer = setTimeout(() => {
      void props.mentions(at.query).then((list) => {
        if (mine !== asked) return;
        setPeople(list);
        setActive(0);
      });
    }, MENTION_DEBOUNCE_MS);
  };

  const pick = (p: MentionCandidate) => {
    const at = mentionAt(field.value, field.selectionStart);
    if (!at) return;
    const name = p.name.trim() || 'Участник';
    const next = insertMention(field.value, at.start, field.selectionStart, name);
    if (!picked.some((x) => x.id === p.id)) picked.push({ id: p.id, name });
    setText(next.text);
    field.value = next.text;
    field.setSelectionRange(next.caret, next.caret);
    setPeople([]);
    asked++;
  };

  const send = async () => {
    const body = storedBody(text(), picked).trim();
    if (!bodyState(body).ok || busy()) return;
    setBusy(true);
    const ok = await props.onSubmit(body);
    setBusy(false);
    if (ok) {
      setText('');
      picked = [];
      field.value = '';
    }
  };

  const onKey = (e: KeyboardEvent) => {
    // Клавиши поля — его: холст и команды их не получают.
    e.stopPropagation();
    const list = people();
    if (list.length > 0 && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault();
      setActive((i) => (i + (e.key === 'ArrowDown' ? 1 : list.length - 1)) % list.length);
      return;
    }
    if (list.length > 0 && (e.key === 'Enter' || e.key === 'Tab')) {
      e.preventDefault();
      pick(list[active()]);
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      if (list.length > 0) setPeople([]);
      else props.onCancel();
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void send();
    }
  };

  return (
    <div class="fe-composer">
      <textarea
        ref={field}
        class="fe-composer__field"
        aria-label={props.label}
        placeholder={props.placeholder}
        rows={2}
        value={text()}
        disabled={busy()}
        onInput={(e) => {
          setText(e.currentTarget.value);
          lookup();
        }}
        onKeyDown={onKey}
        onKeyUp={(e) => e.stopPropagation()}
      />
      <Show when={people().length > 0}>
        <ul class="fe-composer__people" role="listbox" aria-label="Упомянуть">
          <For each={people()}>
            {(p, i) => (
              <li
                role="option"
                aria-selected={i() === active()}
                class="fe-composer__person"
                classList={{ 'is-active': i() === active() }}
                onPointerDown={(e) => {
                  e.preventDefault();
                  pick(p);
                }}
              >
                <span class="t-strong">{p.name.trim() || 'Участник'}</span>
                <Show when={p.email}>{(m) => <span class="t-muted">{m()}</span>}</Show>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <div class="fe-composer__row">
        <Show when={state().length > 3800}>
          <span class="t-muted" classList={{ 'fe-composer__over': state().over }}>
            {state().length} / 4000
          </span>
        </Show>
        <button class="fe-btn fe-btn--primary" disabled={!state().ok || busy()} onClick={() => void send()}>
          {props.submit}
        </button>
      </div>
    </div>
  );
}

export function CommentsUi(props: CommentsProps) {
  const [threads, setThreads] = createStore<Record<string, ThreadView>>({});
  const [loaded, setLoaded] = createSignal(false);
  const [me, setMe] = createSignal<{ user: string; role: FileRole } | null>(null);
  const [open, setOpen] = createSignal<string | null>(null);
  const [detail, setDetail] = createSignal<{ view: ThreadView; comments: CommentDto[] } | null>(null);
  const [draft, setDraft] = createSignal<PinDto | null>(null);
  const [anchor, setAnchor] = createSignal({ x: 0, y: 0 });
  const [editing, setEditing] = createSignal<string | null>(null);
  const [confirmDelete, setConfirmDelete] = createSignal<string | null>(null);
  const [filter, setFilter] = createSignal<PanelFilter>(DEFAULT_FILTER);
  const [top, setTop] = createSignal(0);
  const [height, setHeight] = createSignal(600);
  const [now, setNow] = createSignal(Date.now());
  let link = props.link;
  let popover: HTMLDivElement | undefined;
  let panel: HTMLDivElement | undefined;
  /** Щелчок по холсту закрыл окно: место нового комментария из того же щелчка не ставится (как в Figma). */
  let swallowPlace = false;

  const tick = setInterval(() => setNow(Date.now()), 30_000);
  onCleanup(() => clearInterval(tick));

  const close = () => {
    batch(() => {
      setOpen(null);
      setDetail(null);
      setDraft(null);
      setEditing(null);
      setConfirmDelete(null);
    });
    props.editor.send({ type: 'commentOpen' });
    props.editor.send({ type: 'commentDraft' });
    void props.request({ k: 'open', thread: null });
  };

  const openThread = (id: string, sx?: number, sy?: number) => {
    batch(() => {
      setDraft(null);
      setOpen(id);
      setEditing(null);
      setConfirmDelete(null);
      setDetail(null);
      if (sx !== undefined && sy !== undefined) setAnchor({ x: sx, y: sy });
    });
    props.editor.send({ type: 'commentDraft' });
    props.editor.send({ type: 'commentOpen', id });
    void props.request({ k: 'open', thread: id }).then((r) => {
      if (!r.ok && open() === id) {
        props.toast(r.failure === 'NOT_FOUND' ? 'Обсуждение не найдено или нет доступа' : failText(r.failure));
        close();
      }
    });
  };

  const onMessage = (m: CommentsToMain) => {
    if (m.t === 'me') setMe({ user: m.user, role: m.role });
    else if (m.t === 'threads') {
      batch(() => {
        if (m.reset) {
          const all: Record<string, ThreadView> = {};
          for (const t of m.upserts) all[t.id] = t;
          setThreads(reconcile(all));
        } else {
          for (const t of m.upserts) setThreads(t.id, reconcile(t));
          for (const id of m.removed) setThreads(id, undefined!);
        }
        setLoaded(true);
      });
      // Ссылка на обсуждение (`?comment=`): после первого списка — страница, камера, окно.
      if (m.reset && link) {
        const id = link;
        link = undefined;
        const t = threads[id];
        if (!t) props.toast('Обсуждение не найдено или нет доступа');
        else {
          if (t.resolvedAt !== null) setResolved(true);
          props.editor.send({ type: 'commentFocus', id });
        }
      }
    } else if (m.t === 'detail') {
      if (m.thread !== open()) return;
      if (!m.view) {
        props.toast('Обсуждение удалено');
        close();
      } else setDetail({ view: m.view, comments: m.comments });
    }
  };
  onCleanup(props.subscribe(onMessage));

  const setResolved = (on: boolean) => {
    setFilter((f) => ({ ...f, resolved: on }));
    props.editor.send({ type: 'commentShowResolved', on });
  };

  onCleanup(
    props.editor.core.on((e) => {
      switch (e.type) {
        case 'commentPlace':
          if (swallowPlace) {
            swallowPlace = false;
            return;
          }
          batch(() => {
            setOpen(null);
            setDetail(null);
            setDraft({ page: e.page, node: e.node ?? null, x: e.x, y: e.y, wx: e.wx, wy: e.wy });
            setAnchor({ x: e.sx, y: e.sy });
          });
          props.editor.send({ type: 'commentOpen' });
          props.editor.send({ type: 'commentDraft', draft: { page: e.page, node: e.node, x: e.x, y: e.y, wx: e.wx, wy: e.wy } });
          void props.request({ k: 'open', thread: null });
          break;
        case 'commentOpened':
          swallowPlace = false;
          openThread(e.id, e.sx, e.sy);
          break;
        case 'commentAnchor':
          if (e.visible) setAnchor({ x: e.sx, y: e.sy });
          break;
        case 'commentMoved':
          void props
            .request({ k: 'move', thread: e.id, pin: { page: e.page, node: e.node ?? null, x: e.x, y: e.y, wx: e.wx, wy: e.wy } })
            .then((r) => {
              if (!r.ok) props.toast(failText(r.failure));
            });
          break;
        case 'tool':
          // Вышли из режима — новое не пишется (открытое обсуждение остаётся, как в Figma).
          if (e.tool !== 'comment' && draft()) close();
          break;
      }
    }),
  );

  // Щелчок мимо окна закрывает его; Esc закрывает окно раньше, чем ядро выйдет из режима.
  const onPointer = (e: PointerEvent) => {
    if (!open() && !draft()) return;
    const t = e.target as Node;
    if (popover?.contains(t) || panel?.contains(t)) return;
    swallowPlace = t === props.canvas;
    close();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || (!open() && !draft())) return;
    e.preventDefault();
    e.stopPropagation();
    close();
  };
  document.addEventListener('pointerdown', onPointer, true);
  window.addEventListener('keydown', onKey, true);
  onCleanup(() => {
    document.removeEventListener('pointerdown', onPointer, true);
    window.removeEventListener('keydown', onKey, true);
  });

  const mentions = async (q: string): Promise<MentionCandidate[]> => {
    const r = await props.request({ k: 'mentions', q });
    return r.ok ? ((r.value as MentionCandidate[]) ?? []) : [];
  };

  const write = async (req: Parameters<CommentsProps['request']>[0]): Promise<boolean> => {
    const r = await props.request(req);
    if (!r.ok) props.toast(r.failure === 'THREAD_FULL' ? 'В обсуждении слишком много ответов' : failText(r.failure));
    return r.ok;
  };

  const create = async (body: string) => {
    const pin = draft();
    if (!pin) return false;
    const r = await props.request({ k: 'create', pin, body });
    if (!r.ok) {
      props.toast(failText(r.failure));
      return false;
    }
    const id = (r.value as { thread: { id: string } }).thread.id;
    openThread(id);
    return true;
  };

  const canEdit = (c: CommentDto) => c.author.id !== null && c.author.id === me()?.user;
  const canDelete = (c: CommentDto) => canEdit(c) || me()?.role === 'owner';
  const canResolve = (t: ThreadView) => me()?.role === 'owner' || me()?.role === 'editor' || t.author.id === me()?.user;

  // Окно у метки: справа сверху от неё, в пределах холста.
  const position = createMemo(() => {
    const a = anchor();
    const left = (props.canvas.offsetLeft ?? 0) + a.x + 36;
    const top = Math.max(8, (props.canvas.offsetTop ?? 0) + a.y - 28);
    return { left: `${Math.min(left, Math.max(8, props.canvas.clientWidth - 328))}px`, top: `${top}px` };
  });

  const rows = createMemo(() => panelThreads(Object.values(threads), filter(), props.page(), me()?.user ?? ''));
  const win = createMemo(() => listWindow(top(), height(), PANEL_ROW, rows().length));

  return (
    <>
      <Show when={draft()}>
        <div ref={popover} class="fe-thread" role="dialog" aria-label="Новый комментарий" style={position()}>
          <Composer
            label="Комментарий"
            placeholder="Добавьте комментарий. @ — упомянуть"
            submit="Отправить"
            mentions={mentions}
            onSubmit={create}
            onCancel={close}
          />
        </div>
      </Show>
      <Show when={open()}>
        {(id) => (
          <div ref={popover} class="fe-thread" role="dialog" aria-label="Обсуждение" style={position()}>
            <div class="fe-thread__head">
              <span class="t-strong">Обсуждение</span>
              <Show when={detail()?.view}>
                {(v) => (
                  <Show when={canResolve(v())}>
                    <button
                      class="fe-btn fe-btn--ghost"
                      onClick={() => void write({ k: 'resolve', thread: id(), resolved: v().resolvedAt === null })}
                    >
                      {v().resolvedAt === null ? 'Закрыть обсуждение' : 'Открыть снова'}
                    </button>
                  </Show>
                )}
              </Show>
              <button class="fe-btn fe-btn--ghost" aria-label="Закрыть окно" onClick={close}>
                ×
              </button>
            </div>
            <Show when={detail()} fallback={<p class="t-muted fe-thread__loading">Загрузка…</p>}>
              {(d) => (
                <>
                  <Show when={d().view.resolvedAt !== null}>
                    <p class="t-muted fe-thread__resolved">Обсуждение закрыто: {authorName(d().view.resolvedBy)}</p>
                  </Show>
                  <ol class="fe-thread__list">
                    <For each={d().comments}>
                      {(c, i) => (
                        <li class="fe-comment">
                          <div class="fe-comment__meta">
                            <span class="t-strong">{authorName(c.author)}</span>
                            <span class="t-muted">
                              {timeLabel(c.createdAt, now())}
                              {c.editedAt !== null ? ' · изменено' : ''}
                            </span>
                          </div>
                          <Show
                            when={editing() === c.id}
                            fallback={
                              <>
                                <Body comment={c} />
                                <div class="fe-comment__actions">
                                  <Show when={canEdit(c)}>
                                    <button class="fe-link" onClick={() => setEditing(c.id)}>
                                      Изменить
                                    </button>
                                  </Show>
                                  <Show when={canDelete(c)}>
                                    <button
                                      class="fe-link"
                                      onClick={() => {
                                        if (confirmDelete() !== c.id) {
                                          setConfirmDelete(c.id);
                                          return;
                                        }
                                        setConfirmDelete(null);
                                        void write({ k: 'delete', thread: id(), comment: c.id });
                                      }}
                                    >
                                      {confirmDelete() === c.id ? (i() === 0 ? 'Удалить обсуждение?' : 'Удалить комментарий?') : 'Удалить'}
                                    </button>
                                  </Show>
                                </div>
                              </>
                            }
                          >
                            <Composer
                              label="Изменить комментарий"
                              placeholder=""
                              initial={editableBody(c.body, c.mentions)}
                              submit="Сохранить"
                              mentions={mentions}
                              onSubmit={async (body) => {
                                const ok = await write({ k: 'edit', thread: id(), comment: c.id, body });
                                if (ok) setEditing(null);
                                return ok;
                              }}
                              onCancel={() => setEditing(null)}
                            />
                          </Show>
                        </li>
                      )}
                    </For>
                  </ol>
                  <Show when={editing() === null}>
                    <Composer
                      label="Ответ"
                      placeholder="Ответить. @ — упомянуть"
                      submit="Ответить"
                      mentions={mentions}
                      onSubmit={(body) => write({ k: 'reply', thread: id(), body })}
                      onCancel={close}
                    />
                  </Show>
                </>
              )}
            </Show>
          </div>
        )}
      </Show>
      <Show when={props.mode() && props.panelHost()}>
        {(host) => (
          <Portal mount={host()}>
            <section ref={panel} class="fe-comments" aria-label="Комментарии">
              <div class="fe-comments__head">
                <span class="t-strong">Комментарии</span>
                <select
                  aria-label="Какие обсуждения"
                  value={filter().scope}
                  onChange={(e) => setFilter((f) => ({ ...f, scope: e.currentTarget.value as PanelFilter['scope'] }))}
                >
                  <option value="page">Эта страница</option>
                  <option value="all">Все страницы</option>
                </select>
                <select
                  aria-label="Порядок"
                  value={filter().sort}
                  onChange={(e) => setFilter((f) => ({ ...f, sort: e.currentTarget.value as PanelFilter['sort'] }))}
                >
                  <option value="date">По дате</option>
                  <option value="unread">Сначала непрочитанные</option>
                </select>
              </div>
              <div class="fe-comments__filters">
                <label>
                  <input
                    type="checkbox"
                    checked={filter().mine}
                    onChange={(e) => setFilter((f) => ({ ...f, mine: e.currentTarget.checked }))}
                  />
                  Только мои
                </label>
                <label>
                  <input type="checkbox" checked={filter().resolved} onChange={(e) => setResolved(e.currentTarget.checked)} />
                  Показать закрытые
                </label>
              </div>
              <div
                class="fe-comments__scroll"
                ref={(el) => {
                  queueMicrotask(() => setHeight(el.clientHeight || 600));
                }}
                onScroll={(e) => setTop(e.currentTarget.scrollTop)}
              >
                <Show when={loaded()} fallback={<p class="t-muted fe-comments__empty">Загрузка…</p>}>
                  <Show when={rows().length > 0} fallback={<p class="t-muted fe-comments__empty">Обсуждений нет</p>}>
                    <ul class="fe-comments__list" role="list" style={{ height: `${rows().length * PANEL_ROW}px` }}>
                      <For each={rows().slice(win().start, win().end)}>
                        {(t, i) => (
                          <li
                            class="fe-comments__row"
                            classList={{ 'is-unread': t.unread, 'is-resolved': t.resolvedAt !== null, 'is-open': open() === t.id }}
                            style={{ top: `${(win().start + i()) * PANEL_ROW}px`, height: `${PANEL_ROW}px` }}
                          >
                            <button class="fe-comments__item" onClick={() => props.editor.send({ type: 'commentFocus', id: t.id })}>
                              <span class="fe-comments__who">
                                <span class="t-strong">{authorName(t.author)}</span>
                                <span class="t-muted">{timeLabel(t.lastAt, now())}</span>
                                <Show when={t.unread}>
                                  <span class="fe-comments__dot" aria-label="Не прочитано" />
                                </Show>
                              </span>
                              <span class="fe-comments__text">
                                <Body comment={t.first} />
                              </span>
                              <span class="t-muted fe-comments__replies">
                                {t.replies > 0 ? `Ответов: ${t.replies}` : ''}
                                {t.resolvedAt !== null ? ' · закрыто' : ''}
                              </span>
                            </button>
                          </li>
                        )}
                      </For>
                    </ul>
                  </Show>
                </Show>
              </div>
            </section>
          </Portal>
        )}
      </Show>
    </>
  );
}
````

В `apps/web/src/ui/server/server-api.ts` заменить:

````ts
  isTeamList,
  isTeamMembers,
  type ApiErrorCode,
  type ProjectDto,
````

на:

````ts
  isTeamList,
  isTeamMembers,
  parsePrefs,
  type PrefsDto,
  type ApiErrorCode,
  type ProjectDto,
````

В `apps/web/src/ui/server/server-api.ts` заменить:

````ts
export const renameProject = (id: string, name: string) => send('PATCH', project(id), isProjectDto, { name });
export const deleteProject = (id: string) => send('DELETE', project(id), isOk);
````

на:

````ts
export const renameProject = (id: string, name: string) => send('PATCH', project(id), isProjectDto, { name });
export const deleteProject = (id: string) => send('DELETE', project(id), isOk);

const isPrefs = (v: unknown): v is PrefsDto => parsePrefs(v) !== null;
/** Настройка «Письма о комментариях» (спека подсистемы 4, раздел 10.4): на сервере, письма отправляет он. */
export const getPrefs = (): Promise<ApiResult<PrefsDto>> => call('/api/me/prefs', isPrefs);
export const setPrefs = (prefs: PrefsDto) => send('PUT', '/api/me/prefs', (v): v is unknown => isPrefs(v) || isOk(v), prefs);
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';
import { PresenceBar } from './presence-bar';
import { ShareButton } from './share-dialog';
````

на:

````tsx
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';
import { CommentsUi } from './comments';
import { PresenceBar } from './presence-bar';
import { ShareButton } from './share-dialog';
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
  extensions.shareButton = ShareButton;
  extensions.presenceBar = PresenceBar;
}
````

на:

````tsx
  extensions.shareButton = ShareButton;
  extensions.presenceBar = PresenceBar;
  extensions.comments = CommentsUi;
}
````

В `apps/web/src/ui/styles/server.css` заменить:

````css
  pointer-events: none;
}
````

на:

````css
  pointer-events: none;
}

/* Комментарии (спека подсистемы 4, раздел 10.3): окно обсуждения у метки и панель справа в режиме комментариев. */
.fe-account__pref {
  display: flex;
  gap: var(--space-2);
  align-items: center;
}

.fe-thread {
  position: absolute;
  z-index: 20;
  width: 320px;
  max-height: min(480px, calc(100% - 16px));
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  overflow: auto;
  background: var(--surface-raised);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-popover);
  color: var(--ink);
}

.fe-thread__head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.fe-thread__head > .t-strong {
  flex: 1;
}

.fe-thread__list {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.fe-comment__meta {
  display: flex;
  gap: var(--space-2);
  align-items: baseline;
}

.fe-comment__body {
  margin: var(--space-1) 0 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.fe-comment__mention {
  color: var(--select);
  font-weight: 500;
}

.fe-comment__actions {
  display: flex;
  gap: var(--space-3);
  margin-top: var(--space-1);
}

.fe-comment__actions .fe-link {
  background: none;
  border: 0;
  padding: 0;
  color: var(--ink-muted);
  cursor: pointer;
  font: inherit;
}

.fe-comment__actions .fe-link:hover {
  color: var(--ink);
}

.fe-composer {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.fe-composer__field {
  resize: vertical;
  min-height: 52px;
  padding: var(--space-2);
  border: 1px solid var(--line-control);
  border-radius: var(--radius-md);
  background: var(--surface);
  color: var(--ink);
  font: inherit;
}

.fe-composer__row {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: var(--space-2);
}

.fe-composer__over {
  color: var(--danger);
}

.fe-composer__people {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  z-index: 1;
  margin: var(--space-1) 0 0;
  padding: var(--space-1);
  list-style: none;
  background: var(--surface-raised);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-popover);
}

.fe-composer__person {
  display: flex;
  flex-direction: column;
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.fe-composer__person.is-active {
  background: var(--select-soft);
}

.fe-comments-host {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

/* Высота панели — от колонки, а не от списка: прокручивается только список. */
.fe-comments {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
}

.fe-comments__head,
.fe-comments__filters {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
  padding: var(--space-2) var(--space-3);
}

.fe-comments__head > .t-strong {
  flex: 1 0 100%;
}

.fe-comments__scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

.fe-comments__empty {
  padding: var(--space-3);
}

.fe-comments__list {
  position: relative;
  margin: 0;
  padding: 0;
  list-style: none;
}

.fe-comments__row {
  position: absolute;
  left: 0;
  right: 0;
  border-top: 1px solid var(--line);
}

.fe-comments__row.is-resolved {
  opacity: 0.6;
}

.fe-comments__row.is-open .fe-comments__item {
  background: var(--select-soft);
}

.fe-comments__item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
  height: 100%;
  padding: var(--space-2) var(--space-3);
  border: 0;
  background: none;
  color: inherit;
  text-align: left;
  font: inherit;
  cursor: pointer;
  overflow: hidden;
}

.fe-comments__item:hover {
  background: var(--surface-hover);
}

.fe-comments__who {
  display: flex;
  gap: var(--space-2);
  align-items: center;
}

.fe-comments__dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--select);
}

.fe-comments__text .fe-comment__body {
  margin: 0;
  display: -webkit-box;
  -webkit-line-clamp: 1;
  -webkit-box-orient: vertical;
  overflow: hidden;
  white-space: normal;
}
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
// булевы операции со стрелкой (как в Figma: операция выделенной булевой группы
// или последняя выбранная на кнопке, в списке — ещё «Сплющить» и «Контур
// обводки») и отключённые комментарии — с подсказкой, когда они появятся. Рука (H, удержание
// Space) работает с клавиатуры и в тулбар не выносится.

import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import type { BooleanOpDto, Tool } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
````

на:

````tsx
// булевы операции со стрелкой (как в Figma: операция выделенной булевой группы
// или последняя выбранная на кнопке, в списке — ещё «Сплющить» и «Контур
// обводки») и «Комментарий» (C, спека подсистемы 4, раздел 10.3): в файлах на сервере — инструмент, в автономной сборке —
// отключённая кнопка с объяснением (как в Figma без сети комментариев нет). Рука (H, удержание
// Space) работает с клавиатуры и в тулбар не выносится.

import { createEffect, createMemo, createSignal, onCleanup, onMount, Show } from 'solid-js';
import type { BooleanOpDto, Tool } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
  { op: 'intersect', name: 'Пересечение', command: 'object.boolean_intersect' },
  { op: 'exclude', name: 'Исключение', command: 'object.boolean_exclude' },
];

const LATER: { name: string; key: string; when: string; icon: IconName }[] = [
  { name: 'Комментарии', key: 'C', when: 'подсистема 4', icon: 'comment' },
];

````

на:

````tsx
  { op: 'intersect', name: 'Пересечение', command: 'object.boolean_intersect' },
  { op: 'exclude', name: 'Исключение', command: 'object.boolean_exclude' },
];

````

В `apps/web/src/ui/toolbar.tsx` заменить:

````tsx
      </div>
      <span class="fe-toolbar__sep" role="separator" />
      <For each={LATER}>
        {(t) => (
          <button
            class="fe-tool"
            aria-disabled="true"
            aria-label={`${t.name} — ${t.when}`}
            data-tip={`${t.name}  ${t.key} — ${t.when}`}
            data-tip-side="top"
          >
            <Icon name={t.icon} />
          </button>
        )}
      </For>
      <Show when={menu()}>
        {(m) => <Menu label={menuLabel(m().kind)} items={menuItems(m().kind)} anchor={m().anchor} above onClose={() => setMenu(null)} />}
````

на:

````tsx
      </div>
      <span class="fe-toolbar__sep" role="separator" />
      <Show
        when={enabled('tool.comment')}
        fallback={
          <button
            class="fe-tool"
            aria-disabled="true"
            aria-label="Комментарии — только в файлах на сервере"
            data-tip="Комментарии  C — только в файлах на сервере"
            data-tip-side="top"
          >
            <Icon name="comment" />
          </button>
        }
      >
        <button
          class="fe-tool"
          aria-pressed={tool() === 'comment'}
          aria-label="Комментарий"
          data-tip="Комментарий"
          data-command="tool.comment"
          data-tip-side="top"
          onClick={() => setTool('comment')}
        >
          <Icon name="comment" />
        </button>
      </Show>
      <Show when={menu()}>
        {(m) => <Menu label={menuLabel(m().kind)} items={menuItems(m().kind)} anchor={m().anchor} above onClose={() => setMenu(null)} />}
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npx playwright test e2e/chrome.spec.ts --project=chromium`

Expected: PASS: всё без замечаний; `unit` 345 passed; обе сборки собираются, в автономной нет кода комментариев; `e2e/chrome.spec.ts` 27 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e/chrome.spec.ts apps/web/src/ui/app.tsx apps/web/src/ui/auth/account.tsx apps/web/src/ui/extensions.ts apps/web/src/ui/root.tsx apps/web/src/ui/router.ts apps/web/src/ui/server/comments-logic.ts apps/web/src/ui/server/comments.tsx apps/web/src/ui/server/server-api.ts apps/web/src/ui/server/setup.tsx apps/web/src/ui/styles/server.css apps/web/src/ui/toolbar.tsx apps/web/unit/comments-logic.test.ts apps/web/unit/router.test.ts
git commit -F - <<'EOF'
интерфейс: комментарии — окно у метки, поле с упоминаниями, панель, ссылка на обсуждение, настройка писем

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 12: E2E — комментарии на стенде

Два браузера: метка, ответ, живое обновление, упоминание и письмо в Mailpit, права читателя, ссылка; проба панели с 1000 обсуждений. Стенду — `COMMENT_MAIL_DELAY_MS=2000`.

**Files:**
- Modify: `apps/server/src/config.ts`, `apps/server/src/index.ts`, `docker-compose.test.env`, `docker-compose.yml`
- Test: `apps/server/test/config.test.ts`, `apps/web/e2e-server/comments-panel.spec.ts` (новый), `apps/web/e2e-server/comments.spec.ts` (новый), `apps/web/e2e-server/stand.ts`

**Interfaces:**
- Consumes: всё выше.
- Produces: `Config.commentMailDelayMs`, переменная `COMMENT_MAIL_DELAY_MS`; в `e2e-server/stand.ts` — `mailsMatching`, `mailsNow`; E2E `e2e-server/comments.spec.ts`, `comments-panel.spec.ts`.

- [ ] **Step 1: Написать тесты**

В `apps/server/test/config.test.ts` заменить:

````ts
  expect(loadConfig({ ...ENV, FERRUM_PORT: '9000' }).port).toBe(9000);
});
````

на:

````ts
  expect(loadConfig({ ...ENV, FERRUM_PORT: '9000' }).port).toBe(9000);
});

test('COMMENT_MAIL_DELAY_MS: по умолчанию 5 минут, стенд сокращает, мусор — отказ', () => {
  expect(loadConfig(ENV).commentMailDelayMs).toBe(300_000);
  expect(loadConfig({ ...ENV, COMMENT_MAIL_DELAY_MS: '2000' }).commentMailDelayMs).toBe(2000);
  expect(() => loadConfig({ ...ENV, COMMENT_MAIL_DELAY_MS: '10' })).toThrow(/COMMENT_MAIL_DELAY_MS/);
  expect(() => loadConfig({ ...ENV, COMMENT_MAIL_DELAY_MS: 'x' })).toThrow(/COMMENT_MAIL_DELAY_MS/);
});
````

Создать `apps/web/e2e-server/comments-panel.spec.ts`:

````ts
// Панель комментариев с 1000 обсуждений (проба плана, спека подсистемы 4, раздел 10.3): список виртуальный — строк в DOM столько,
// сколько видно; живое изменение одного обсуждения не пересобирает остальные строки (тот же элемент DOM). Длинные задачи
// основного потока и время открытия панели печатаются для заметок, без проверок по времени.

import { expect, test, type Page } from '@playwright/test';
import { editorReady, fileId, testMode } from '../e2e/helpers';
import { compose, newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);

const THREADS = 1000;

const psql = (sql: string) => compose('exec', '-T', 'postgres', 'psql', '-U', 'ferrum', '-d', 'ferrum', '-v', 'ON_ERROR_STOP=1', '-c', sql);

const longTasks = (page: Page) =>
  page.evaluate(() => {
    const w = window as unknown as { __long?: number[] };
    w.__long = [];
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) w.__long!.push(Math.round(e.duration));
    }).observe({ type: 'longtask', buffered: false });
  });
const takeLong = (page: Page) => page.evaluate(() => (window as unknown as { __long: number[] }).__long.splice(0));

test('панель с 1000 обсуждений: строк в DOM — видимые, живое изменение не пересобирает остальные строки', async ({ page }) => {
  await testMode(page);
  await page.goto('/');
  await signIn(page, newEmail('pp'), 'Пётр Панель');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  await expect(page.locator('.fe-sync')).toHaveText('Синхронизировано', { timeout: 15_000 });
  const id = fileId(page);
  const user = await page.evaluate(async () => ((await (await fetch('/api/auth/session')).json()) as { user: { id: string } }).user.id);
  const pageId = await page.evaluate(() => (window.__ferrum!.ui as unknown as { pages: { current: string } }).pages.current);
  // 1000 обсуждений прямо в базе: через API их не набрать — 60 комментариев в минуту на человека.
  psql(
    `WITH t AS (
       INSERT INTO comment_thread (id, file_id, page_id, node_id, x, y, wx, wy, first_id, created_by, created_at, seq, replies, last_at, participants)
       SELECT gen_random_uuid(), '${id}', '${pageId}', NULL, (i % 40) * 60, (i / 40) * 60, (i % 40) * 60, (i / 40) * 60, gen_random_uuid(),
              '${user}', now() - make_interval(mins => i), i, 0, now() - make_interval(mins => i), ARRAY['${user}']
       FROM generate_series(1, ${THREADS}) i RETURNING id, first_id, created_at, seq)
     INSERT INTO comment (id, thread_id, author_id, body, created_at)
     SELECT first_id, id, '${user}', 'Обсуждение ' || seq, created_at FROM t;
     INSERT INTO comment_file (file_id, seq) VALUES ('${id}', ${THREADS}) ON CONFLICT (file_id) DO UPDATE SET seq = ${THREADS};`,
  );
  await page.reload();
  await editorReady(page);
  await expect(page.locator('.fe-sync')).toHaveText('Синхронизировано', { timeout: 15_000 });
  await longTasks(page);
  await expect(page.getByRole('button', { name: 'Комментарий', exact: true })).toBeVisible();
  await page.locator('canvas').focus();
  const t0 = Date.now();
  await page.keyboard.press('KeyC');
  const panel = page.getByRole('region', { name: 'Комментарии' });
  await expect(panel.locator('.fe-comments__row').first()).toContainText('Обсуждение 1');
  const openMs = Date.now() - t0;
  const rows = await panel.locator('.fe-comments__row').count();
  expect(rows).toBeLessThan(40);
  // Прокрутка в конец: те же десятки строк, последние обсуждения.
  const scroller = panel.locator('.fe-comments__scroll');
  await scroller.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(panel.locator('.fe-comments__row').last()).toContainText(`Обсуждение ${THREADS}`);
  expect(await panel.locator('.fe-comments__row').count()).toBeLessThan(40);
  await scroller.evaluate((el) => (el.scrollTop = 0));
  await expect(panel.locator('.fe-comments__row').first()).toContainText('Обсуждение 1');
  // Метка на строке, которая не изменится: после живого изменения другого обсуждения элемент тот же.
  await panel
    .locator('.fe-comments__row')
    .nth(1)
    .evaluate((el) => ((el as unknown as { __mark: number }).__mark = 1));
  const scrollLong = await takeLong(page);
  const list = (await page.evaluate(async (id) => (await fetch(`/api/files/${id}/comments?after=0&limit=3`)).json(), id)) as {
    threads: { id: string; first: { body: string } }[];
  };
  const target = list.threads.find((t) => t.first.body === 'Обсуждение 1')!;
  const res = await page.evaluate(
    async ({ id, thread }) =>
      (
        await fetch(`/api/files/${id}/comments/${thread}/comments`, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ body: 'живой ответ' }),
        })
      ).status,
    { id, thread: target.id },
  );
  expect(res).toBe(201);
  await expect(panel.locator('.fe-comments__row').first()).toContainText('Ответов: 1');
  expect(
    await panel
      .locator('.fe-comments__row')
      .nth(1)
      .evaluate((el) => (el as unknown as { __mark?: number }).__mark),
  ).toBe(1);
  const updateLong = await takeLong(page);
  console.log(
    `ПРОБА панели: ${THREADS} обсуждений, открытие ${openMs} мс, строк в DOM ${rows}, длинные задачи: прокрутка ${JSON.stringify(scrollLong)}, живое изменение ${JSON.stringify(updateLong)}`,
  );
});
````

Создать `apps/web/e2e-server/comments.spec.ts`:

````ts
// Комментарии на настоящем стенде (спека подсистемы 4, раздел 10): два браузера — метка и обсуждение видны другому сразу, ответ и
// закрытие тоже; упоминание приходит письмом (Mailpit), ссылка из письма открывает обсуждение; читатель комментирует, но чужое
// не удаляет; человек без доступа по ссылке ничего не получает. Окно группировки писем на стенде — 2 с (`COMMENT_MAIL_DELAY_MS`).

import { expect, test, type Browser, type Page } from '@playwright/test';
import { appScreen, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { mailsMatching, newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);

const COMMENT_MAIL = /комментари/;

const person = async (browser: Browser, email: string, name: string): Promise<Page> => {
  const context = await browser.newContext({ baseURL: STAND_URL, viewport: { width: 1280, height: 720 } });
  const page = await context.newPage();
  await testMode(page);
  await page.goto('/');
  await signIn(page, email, name);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  return page;
};

const synced = (page: Page) => expect(page.locator('.fe-sync')).toHaveText('Синхронизировано', { timeout: 15_000 });

const api = (page: Page, method: string, path: string, body?: unknown) =>
  page.evaluate(
    async ({ method, path, body }) => {
      const init: RequestInit = { method };
      if (body !== undefined) {
        init.headers = { 'content-type': 'application/json' };
        init.body = JSON.stringify(body);
      }
      const res = await fetch(path, init);
      return { status: res.status, body: (await res.json().catch(() => null)) as never };
    },
    { method, path, body },
  );

async function newFile(page: Page): Promise<string> {
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  await synced(page);
  return fileId(page);
}

const invite = async (owner: Page, id: string, email: string, role: 'editor' | 'viewer') =>
  expect((await api(owner, 'POST', `/api/files/${id}/share/invite`, { email, role })).status).toBe(200);

async function openFile(page: Page, id: string, query = ''): Promise<void> {
  await page.goto(`/#/file/${id}${query}`);
  await page.reload();
  await editorReady(page);
  await synced(page);
}

/** Режим комментариев: инструмент доступен, когда роль пришла (ядро получило `commentsAvailable`). */
async function commentMode(page: Page): Promise<void> {
  await expect(page.getByRole('button', { name: 'Комментарий', exact: true })).toBeVisible();
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyC');
  await expect(page.getByRole('region', { name: 'Комментарии' })).toBeVisible();
}

/** Метка в точке мира (x, y): её капля — квадрат над остриём, щелчок — в его середину. */
async function pinAt(page: Page, x: number, y: number): Promise<{ x: number; y: number }> {
  const p = await appScreen(page, await live(page), x, y);
  return { x: p.x + 14, y: p.y - 14 };
}

test('два браузера: комментарий с упоминанием, метка и обсуждение у другого сразу, ответ и закрытие', async ({ browser }) => {
  const [a, b] = [await person(browser, newEmail('ca'), 'Анна Смирнова'), await person(browser, newEmail('cb'), 'Борис')];
  const id = await newFile(a);
  const emailB = await b.evaluate(
    async () => ((await (await fetch('/api/auth/session')).json()) as { user: { email: string } }).user.email,
  );
  await invite(a, id, emailB, 'editor');
  await openFile(b, id);
  // Борис открыл файл: имя знают только у тех, кто открывал — подсказка упоминаний его находит.
  await commentMode(a);
  const at = await appScreen(a, await live(a), 40, 30);
  await a.mouse.click(at.x, at.y);
  const draft = a.getByRole('dialog', { name: 'Новый комментарий' });
  await expect(draft).toBeVisible();
  const field = draft.getByLabel('Комментарий');
  await field.pressSequentially('@Бор');
  await expect(a.getByRole('option', { name: /Борис/ })).toBeVisible();
  await field.press('Enter');
  await expect(field).toHaveValue('@Борис ');
  await field.pressSequentially('посмотри <b>это</b>');
  await field.press('Enter');
  // Окно нового обсуждения: текст как текст, упоминание — именем.
  const threadA = a.getByRole('dialog', { name: 'Обсуждение' });
  await expect(threadA.locator('.fe-comment__body')).toHaveText('@Борис посмотри <b>это</b>');
  await expect(threadA.locator('.fe-comment__body b')).toHaveCount(0);

  // У Бориса метка уже есть: щелчок по ней (вне режима комментариев) открывает обсуждение.
  const pin = await pinAt(b, 40, 30);
  await b.mouse.click(pin.x, pin.y);
  const threadB = b.getByRole('dialog', { name: 'Обсуждение' });
  await expect(threadB.locator('.fe-comment__body').first()).toHaveText('@Борис посмотри <b>это</b>');
  // Ответ — у Анны в открытом окне сразу.
  const reply = threadB.getByLabel('Ответ');
  await reply.fill('Готово');
  await reply.press('Enter');
  await expect(threadA.locator('.fe-comment__body')).toHaveText(['@Борис посмотри <b>это</b>', 'Готово']);
  // Закрытие обсуждения — у Анны сразу; в панели закрытые скрыты, «Показать закрытые» возвращает.
  await threadB.getByRole('button', { name: 'Закрыть обсуждение' }).click();
  await expect(threadA.getByRole('button', { name: 'Открыть снова' })).toBeVisible();
  await expect(threadA.getByText('Обсуждение закрыто: Борис')).toBeVisible();
  await a.keyboard.press('Escape');
  await expect(threadA).toHaveCount(0);
  const panel = a.getByRole('region', { name: 'Комментарии' });
  await expect(panel.getByText('Обсуждений нет')).toBeVisible();
  await panel.getByLabel('Показать закрытые').check();
  await expect(panel.locator('.fe-comments__row')).toHaveCount(1);
});

test('упоминание — письмо, ссылка из письма открывает обсуждение; читатель отвечает, но не удаляет чужое; без доступа — ничего', async ({
  browser,
}) => {
  const a = await person(browser, newEmail('ma'), 'Анна Смирнова');
  const emailV = newEmail('mv');
  const v = await person(browser, emailV, 'Вера Читатель');
  const id = await newFile(a);
  await invite(a, id, emailV, 'viewer');
  // Вера открыла файл раньше (её имя в подсказке) и ушла: упоминание — письмом.
  await openFile(v, id);
  await v.goto('/');
  await expect(v.getByRole('heading', { level: 1 })).toHaveText('Недавние');

  await commentMode(a);
  const at = await appScreen(a, await live(a), 80, 60);
  await a.mouse.click(at.x, at.y);
  const field = a.getByRole('dialog', { name: 'Новый комментарий' }).getByLabel('Комментарий');
  await field.pressSequentially('@Вер');
  await expect(a.getByRole('option', { name: /Вера Читатель/ })).toBeVisible();
  await field.press('Enter');
  await field.pressSequentially('глянь');
  await field.press('Enter');
  const threadA = a.getByRole('dialog', { name: 'Обсуждение' });
  await expect(threadA.locator('.fe-comment__body')).toHaveText('@Вера Читатель глянь');
  const thread = (await api(a, 'GET', `/api/files/${id}/comments?after=0&limit=10`)).body as { threads: { id: string }[] };
  const threadId = thread.threads[0].id;

  const [mail] = await mailsMatching(emailV, COMMENT_MAIL);
  expect(mail.subject).toContain('Анна Смирнова');
  expect(mail.text).toContain('@Вера Читатель глянь');
  const link = /http\S+#\/file\/\S+\?comment=\S+/.exec(mail.text)![0];
  expect(link).toBe(`${STAND_URL}/#/file/${id}?comment=${threadId}`);

  // Ссылка из письма: файл открывается на обсуждении (вкладка открывается по ссылке заново, как из почты).
  const fresh = await v.context().newPage();
  await testMode(fresh);
  await fresh.goto(link);
  await editorReady(fresh);
  await v.close();
  const vv = fresh;
  const threadV = vv.getByRole('dialog', { name: 'Обсуждение' });
  await expect(threadV.locator('.fe-comment__body')).toHaveText('@Вера Читатель глянь');
  // Читатель: отвечает, своё может удалить, чужое — нет.
  await expect(threadV.getByRole('button', { name: 'Удалить' })).toHaveCount(0);
  const reply = threadV.getByLabel('Ответ');
  await reply.fill('Вижу');
  await reply.press('Enter');
  await expect(threadV.locator('.fe-comment__body')).toHaveText(['@Вера Читатель глянь', 'Вижу']);
  await expect(threadV.getByRole('button', { name: 'Удалить' })).toHaveCount(1);
  await expect(threadA.locator('.fe-comment__body')).toHaveText(['@Вера Читатель глянь', 'Вижу']);
  // Сервер тоже не даёт: удалить чужой комментарий — 403.
  const first = ((await api(vv, 'GET', `/api/files/${id}/comments/${threadId}`)).body as { comments: { id: string }[] }).comments[0].id;
  expect((await api(vv, 'DELETE', `/api/files/${id}/comments/${threadId}/comments/${first}`)).status).toBe(403);

  // Без доступа: ссылка ведёт на «Нет доступа», а запрос обсуждения неотличим от несуществующего.
  const g = await person(browser, newEmail('mg'), 'Галина');
  const gg = await g.context().newPage();
  await gg.goto(link);
  await g.close();
  await expect(gg.getByRole('heading', { name: 'Нет доступа' })).toBeVisible();
  await expect(gg.getByText('глянь')).toHaveCount(0);
  expect((await api(gg, 'GET', `/api/files/${id}/comments/${threadId}`)).status).toBe(404);
  expect((await api(gg, 'GET', `/api/files/${id}/comments?after=0&limit=10`)).status).toBe(404);
});
````

В `apps/web/e2e-server/stand.ts` заменить:

````ts
  return Promise.all(found.map(async (m) => ((await (await fetch(`${MAILPIT}/message/${m.ID}`)).json()) as { Text: string }).Text));
}
````

на:

````ts
  return Promise.all(found.map(async (m) => ((await (await fetch(`${MAILPIT}/message/${m.ID}`)).json()) as { Text: string }).Text));
}

/** Письма на адрес, тема которых подходит: текст целиком, новые первыми; ждёт, пока их станет `count`. */
export async function mailsMatching(email: string, subject: RegExp, count = 1): Promise<{ subject: string; text: string }[]> {
  let found: MailSummary[] = [];
  await expect
    .poll(
      async () => {
        const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
        found = list.messages.filter((m) => subject.test(m.Subject) && m.To.some((t) => t.Address === email));
        return found.length;
      },
      { message: `письма ${subject} на ${email}`, timeout: 20_000 },
    )
    .toBeGreaterThanOrEqual(count);
  return Promise.all(
    found.map(async (m) => ({
      subject: m.Subject,
      text: ((await (await fetch(`${MAILPIT}/message/${m.ID}`)).json()) as { Text: string }).Text,
    })),
  );
}

/** Все письма на адрес с такой темой — сейчас, без ожидания. */
export async function mailsNow(email: string, subject: RegExp): Promise<number> {
  const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
  return list.messages.filter((m) => subject.test(m.Subject) && m.To.some((t) => t.Address === email)).length;
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/server && bun test test/config.test.ts`
Expected: FAIL — 5 pass, 1 fail: у `loadConfig` нет `commentMailDelayMs` (`Expected: 300000, Received: undefined`)

- [ ] **Step 3: Реализация**

В `apps/server/src/config.ts` заменить:

````ts
  /** Адреса обратных прокси (точные и IPv4 CIDR): только от них верим `X-Forwarded-For`. */
  trustedProxies: string[];
}

````

на:

````ts
  /** Адреса обратных прокси (точные и IPv4 CIDR): только от них верим `X-Forwarded-For`. */
  trustedProxies: string[];
  /** Окно группировки писем о комментариях (спека, раздел 10.4): 5 минут; стенд тестов сокращает. */
  commentMailDelayMs: number;
}

````

В `apps/server/src/config.ts` заменить:

````ts
    if (problem) problems.push(`TRUSTED_PROXIES: ${problem}`);
  }
  const config: Config = {
    port,
````

на:

````ts
    if (problem) problems.push(`TRUSTED_PROXIES: ${problem}`);
  }
  const delayRaw = env.COMMENT_MAIL_DELAY_MS?.trim();
  const commentMailDelayMs = delayRaw ? Number(delayRaw) : 5 * 60 * 1000;
  if (!Number.isSafeInteger(commentMailDelayMs) || commentMailDelayMs < 1000)
    problems.push('COMMENT_MAIL_DELAY_MS — целое число миллисекунд, не меньше 1000');
  const config: Config = {
    port,
````

В `apps/server/src/config.ts` заменить:

````ts
    webDist: env.FERRUM_WEB_DIST ?? '/app/web',
    trustedProxies,
  };
  if (problems.length > 0) throw new Error(`Настройки сервера: ${problems.join('; ')}`);
````

на:

````ts
    webDist: env.FERRUM_WEB_DIST ?? '/app/web',
    trustedProxies,
    commentMailDelayMs,
  };
  if (problems.length > 0) throw new Error(`Настройки сервера: ${problems.join('; ')}`);
````

В `apps/server/src/index.ts` заменить:

````ts
    secret: config.authSecret,
    compact: { init: { databaseUrl: config.databaseUrl, dataDir: config.dataDir } },
  },
  config.port,
````

на:

````ts
    secret: config.authSecret,
    compact: { init: { databaseUrl: config.databaseUrl, dataDir: config.dataDir } },
    commentMailDelayMs: config.commentMailDelayMs,
    // Очередь смотрится не реже окна группировки: короткое окно стенда не ждёт 15 с прохода.
    commentMail: { pollMs: Math.min(15_000, config.commentMailDelayMs) },
  },
  config.port,
````

В `docker-compose.test.env` заменить:

````
PUBLIC_URL=http://localhost:18090
COMPOSE_PROFILES=dev
````

на:

````
PUBLIC_URL=http://localhost:18090
COMPOSE_PROFILES=dev
# Письма о комментариях — через 2 с, а не через 5 минут: E2E читает их из Mailpit.
COMMENT_MAIL_DELAY_MS=2000
````

В `docker-compose.yml` заменить:

````yaml
      ALLOWED_EMAILS: ${ALLOWED_EMAILS:-}
      TRUSTED_PROXIES: ${TRUSTED_PROXIES:-}
    volumes:
      - ferrum-data:/data
````

на:

````yaml
      ALLOWED_EMAILS: ${ALLOWED_EMAILS:-}
      TRUSTED_PROXIES: ${TRUSTED_PROXIES:-}
      COMMENT_MAIL_DELAY_MS: ${COMMENT_MAIL_DELAY_MS:-300000}
    volumes:
      - ferrum-data:/data
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build:server && npm run e2e:server -- e2e-server/comments.spec.ts e2e-server/comments-panel.spec.ts`

Expected: PASS: всё без замечаний; `test:server` — протокол 56 pass, сервер 597 pass; `comments.spec.ts` и `comments-panel.spec.ts` на Chromium и Firefox — 6 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/src/config.ts apps/server/src/index.ts apps/server/test/config.test.ts apps/web/e2e-server/comments-panel.spec.ts apps/web/e2e-server/comments.spec.ts apps/web/e2e-server/stand.ts docker-compose.test.env docker-compose.yml
git commit -F - <<'EOF'
E2E: комментарии на стенде — два браузера, письмо об упоминании, права читателя, панель 1000 обсуждений

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 13: Документы и финальная проверка

«Сделано (этап 5)» в спеке, §8.6 спеки ядра, `docs/running.md` (комментарии и письма), бэклог B-407…B-418.

**Files:**
- Modify: `docs/backlog.md`, `docs/running.md`, `docs/superpowers/specs/2026-10-23-ferrum-core-design.md`, `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md`

- [ ] **Step 1: Реализация**

В `docs/backlog.md` заменить:

````markdown
| B-406 | Чужое живое превью артборда с 1500 потомками стоит 11,9 мс на пачку на странице из 12 000 узлов (5,8 мс на странице только с ним), в wasm ожидается 18–24 мс: цена растёт с размером страницы (вероятно, пересборка R-tree и рамок сцены), пределы превью её не ограничивают. Нужен дешёвый перенос поддерева в слое превью. | P2 | этап 4, финальная волна ядра |

## Код

````

на:

````markdown
| B-406 | Чужое живое превью артборда с 1500 потомками стоит 11,9 мс на пачку на странице из 12 000 узлов (5,8 мс на странице только с ним), в wasm ожидается 18–24 мс: цена растёт с размером страницы (вероятно, пересборка R-tree и рамок сцены), пределы превью её не ограничивают. Нужен дешёвый перенос поддерева в слое превью. | P2 | этап 4, финальная волна ядра |

## Комментарии (этап 5 подсистемы 4)

| № | Что | Пр. | Источник |
|---|---|---|---|
| B-407 | Первый кадр с 1000 новых меток — 2,2–2,6 мс: картинки строятся по 8 за кадр, остальные метки этого кадра рисуются заливкой. Один раз на открытие страницы. | P3 | этап 5, проба |
| B-408 | Группы меток считаются по видимым меткам: при панорамировании у края холста состав группы может меняться. | P3 | этап 5 |
| B-409 | Своего курсора режима комментариев нет (прицел): интерфейс знает только CSS-курсоры, нужен курсор-«пузырь». Видимость меток (Shift+C) не сохраняется между сеансами. | P3 | этап 5 |
| B-410 | Переход к обсуждению на странице, которой нет (удалена), ждёт до следующего `commentFocus`/`commentOpen`; ссылка на закрытое обсуждение включает «Показать закрытые». | P3 | этап 5 |
| B-411 | Надгробия удалённых обсуждений не чистятся (рост — только пределом 60 в минуту); число живых обсуждений в файле не ограничено (10 000 — около 1 с первой загрузки). Нужны чистка надгробий с ответом «начни с нуля» и предел или постраничная загрузка меток. | P2 | этап 5, проба сервера |
| B-412 | Прочтение не уходит в другие вкладки того же человека (события нет): там обсуждение «не прочитано» до следующего списка. | P3 | этап 5 |
| B-413 | Удаление человека пересчитывает сводки обсуждений, но не поднимает `seq`: открытые клиенты видят прежнего автора до следующего изменения. Правка с новым упоминанием письма не шлёт. | P3 | этап 5 |
| B-414 | Тела запросов, отвергнутых пределом частоты, Bun принимает целиком: под потоком 6,5 тыс. запросов в секунду с телами по 16 КБ пик RSS +200 МБ (живая куча +2 МБ). Общее для всех `POST`. | P2 | этап 5, проба враждебного человека |
| B-415 | Комментарии в одном процессе сервера: «файл открыт» для писем и порядок рассылки — в памяти процесса; при запуске отправки в `sending` считаются неизвестными (два процесса «съели» бы отправки друг друга). `docker stop` посреди письма (SMTP до 60 с) — письмо `uncertain`, повтора нет. | P3 | этап 5 |
| B-416 | Запрос ролей рассылки делит пул с REST: под враждебным потоком p95 доставки 46 мс; при меньшем пуле или медленной базе вырастет. Замер медленного читателя и рассылки — только macOS. | P3 | этап 5, проба |
| B-417 | Окно обсуждения: позиция — справа от метки в пределах холста, без переворота к левому краю; список подсказки упоминаний без виртуальности (до 8 строк). Панель: выпадающие списки фильтров — родные `select`, без стиля дизайн-системы. | P3 | этап 5 |
| B-418 | Письмо о комментариях не отправляется тому, кто открыл файл после первого комментария, даже если сам комментарий он не видел (окно закрыто, другая страница); Figma в этом случае тоже молчит. Тест «нет письма открывшему файл» на стенде E2E не повторён: проверяется в `comment-mail.test.ts` и `comment-live.test.ts`. | P3 | этап 5 |

## Код

````

В `docs/running.md` заменить:

````markdown

Прокси перед сервером должен пропускать тела до 512 МБ и не буферизовать их на диск без нужды (в nginx: `client_max_body_size 513m; proxy_request_buffering off;`; вариант не проверялся). Срок чтения тела у прокси не должен быть короче простоя загрузки: минута.

### Снять сессии человека
````

на:

````markdown

Прокси перед сервером должен пропускать тела до 512 МБ и не буферизовать их на диск без нужды (в nginx: `client_max_body_size 513m; proxy_request_buffering off;`; вариант не проверялся). Срок чтения тела у прокси не должен быть короче простоя загрузки: минута.

### Комментарии и письма о них

Комментарии лежат в Postgres (`comment_thread`, `comment` и рядом), не в файле: резервная копия базы нужна и ради них. Письма об упоминаниях и ответах идут через ту же почту, что и вход, из очереди `mail_outbox`: письмо не теряется при перезапуске, комментарии одного файла за 5 минут собираются в одно письмо (`COMMENT_MAIL_DELAY_MS`, миллисекунды, не меньше 1000). Тому, у кого файл открыт, письма нет; выключить письма можно в «Настройки → Аккаунт». Письмо, которое отправлялось в момент остановки сервера, не повторяется (строка `uncertain` и запись в журнале): лучше пропустить письмо, чем отправить дважды. Сервер — один процесс: два процесса считали бы чужие отправки неизвестными (B-415).

### Снять сессии человека
````

В `docs/running.md` заменить:

````markdown
```

Стенд тестов — свой проект compose `ferrum-test` с портами 18090 (сервер), 18025 и 11025 (Mailpit), 15432 (Postgres), 16379 (Valkey). `FERRUM_KEEP_STAND=1` оставляет его после E2E.
````

на:

````markdown
```

Стенд тестов — свой проект compose `ferrum-test` с портами 18090 (сервер), 18025 и 11025 (Mailpit), 15432 (Postgres), 16379 (Valkey); письма о комментариях на нём уходят через 2 с (`docker-compose.test.env`). `FERRUM_KEEP_STAND=1` оставляет его после E2E.
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
### 8.6 Оверлеи

В той же сцене vello поверх содержимого, в экранных пикселях: рамки выделения, ручки (8px), подсветка наведения, рамка выделения мышью, линии привязки, пиксельная сетка (зум ≥ 800%), названия фреймов, бейдж «W × H», измерения с Alt (раздел 7.3): отрезки в мире, которые редактор отдаёт в `Overlay.measurements` (расстояние со значением или направляющая), рисуются сплошной линией в 1 px (только видимая часть) с плашкой значения по середине видимой части, целиком на экране, и пунктиром в 1 px (штрих 4 px, промежуток 3 px; строятся только штрихи у экрана) — размеры экранные, от зума не зависят. Тонкие линии выравниваются по пикселям. Подписи — однострочный текст: раскладка `parley` со встроенным шрифтом Onest (SIL OFL), глифы — заливка контуров из `skrifa` через тот же трейт бэкенда (`glyph_run` в `vello_gpu` требует ресурсов рендерера с устройством). Бейдж пока набран Onest с цифрами одинаковой ширины. Цвета — из дизайн-системы (`select`, `on-select`, `component`, `ink-muted`; тёмная и светлая темы). Измерения — цвет `measure` = `danger` (#ff7a7a в тёмной, #c22c2c в светлой), текст их плашек — `on-select`; плашка — та же, что у бейджа «W × H». Совпадение цветов оверлеев с `theme.css` проверяет модульный тест интерфейса. Другие участники файла (спека подсистемы 4, раздел 9): курсор — готовая картинка стрелки с плашкой имени цветом участника (одна на имя, цвет и плотность, не больше 8 новых за кадр), рамки их выделения — цветом участника без ручек; рисуются под своими рамками, курсоры — поверх.

### 8.7 Экспорт
````

на:

````markdown
### 8.6 Оверлеи

В той же сцене vello поверх содержимого, в экранных пикселях: рамки выделения, ручки (8px), подсветка наведения, рамка выделения мышью, линии привязки, пиксельная сетка (зум ≥ 800%), названия фреймов, бейдж «W × H», измерения с Alt (раздел 7.3): отрезки в мире, которые редактор отдаёт в `Overlay.measurements` (расстояние со значением или направляющая), рисуются сплошной линией в 1 px (только видимая часть) с плашкой значения по середине видимой части, целиком на экране, и пунктиром в 1 px (штрих 4 px, промежуток 3 px; строятся только штрихи у экрана) — размеры экранные, от зума не зависят. Тонкие линии выравниваются по пикселям. Подписи — однострочный текст: раскладка `parley` со встроенным шрифтом Onest (SIL OFL), глифы — заливка контуров из `skrifa` через тот же трейт бэкенда (`glyph_run` в `vello_gpu` требует ресурсов рендерера с устройством). Бейдж пока набран Onest с цифрами одинаковой ширины. Цвета — из дизайн-системы (`select`, `on-select`, `component`, `ink-muted`; тёмная и светлая темы). Измерения — цвет `measure` = `danger` (#ff7a7a в тёмной, #c22c2c в светлой), текст их плашек — `on-select`; плашка — та же, что у бейджа «W × H». Совпадение цветов оверлеев с `theme.css` проверяет модульный тест интерфейса. Другие участники файла (спека подсистемы 4, раздел 9): курсор — готовая картинка стрелки с плашкой имени цветом участника (одна на имя, цвет и плотность, не больше 8 новых за кадр), рамки их выделения — цветом участника без ручек; рисуются под своими рамками, курсоры — поверх. Метки комментариев (спека подсистемы 4, раздел 10.2): капли с инициалами автора и группы с числом, готовыми картинками (не больше 8 новых за кадр), раскладку и попадание считает редактор; в превью и экспорт не попадают.

### 8.7 Экспорт
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown

- **Левая панель:** шапка файла («Назад к файлам», имя — двойной щелчок, Enter или F2 переименовывают и запись в хранилище тоже; меню: «Новый файл», «Открыть…» (импорт с диска в хранилище), «Сохранить» (Ctrl/Cmd+S), «Сохранить копию на диск…», «Все файлы», «Настройки…»; «Новый файл» и «Открыть…» не спрашивают — текущий файл сначала сохраняется в хранилище, затем открывается другой); страницы (сворачиваемые; добавить, переключить, переименовать, удалить — с подтверждением, потому что операции со страницами не отменяются); дерево слоёв — виртуализировано (основной поток запрашивает у worker'а только видимые строки с запасом, DOM-строки переиспользуются по месту), переименование на месте (одно поле для файла, страницы и слоя: Enter и уход фокуса применяют, Esc отменяет, фокус возвращается к строке; поле, убранное посреди набора, применяет набранное), перетаскивание для порядка и вложенности (над, под, внутрь фрейма или группы; положение на экране сохраняется; переносится взятое при нажатии, клавиши во время жеста не действуют, Esc отменяет), скрыть и заблокировать, поиск, наведение подсвечивает узел на холсте, выделение синхронно в обе стороны (выделение с холста раскрывает предков и прокручивает к строке).
- **Нижний плавающий тулбар по центру:** перемещение, фрейм с пресетами, фигуры (выпадающий список: прямоугольник, эллипс, линия, изображение; на кнопке — последняя выбранная). Перо, текст, комментарии — отключены с подсказкой, когда появятся.
- **Правая панель:** сверху зум (выпадающий список масштабов) и «Поделиться» (неактивна до подсистемы 4); вкладки «Дизайн» и «Прототип» (вторая неактивна). Секции «Дизайн»: Фрейм/Положение (пресет размера одним шагом, X/Y/W/H, поворот в `(-180, 180]`, отразить) → Вид (прозрачность, радиус, обрезка) → Заливка → Обводка → Экспорт (1×/2×/3×, PNG). Без выделения — фон страницы и экспорт страницы. Значения и «Смешанные» сводит worker; у группы X/Y/W/H — рамка детей, W/H масштабируют детей.
- **Поле числа:** выражения (`120+16`, `50%`, `*2`), у «Смешанных» `+N` — прибавка к каждому; Enter применяет, Esc откатывает, стрелки — шаг; перетаскивание подписи (Shift ×10, Alt ×0.1) — превью и один шаг отмены. Правка без изменений шага не создаёт.
````

на:

````markdown

- **Левая панель:** шапка файла («Назад к файлам», имя — двойной щелчок, Enter или F2 переименовывают и запись в хранилище тоже; меню: «Новый файл», «Открыть…» (импорт с диска в хранилище), «Сохранить» (Ctrl/Cmd+S), «Сохранить копию на диск…», «Все файлы», «Настройки…»; «Новый файл» и «Открыть…» не спрашивают — текущий файл сначала сохраняется в хранилище, затем открывается другой); страницы (сворачиваемые; добавить, переключить, переименовать, удалить — с подтверждением, потому что операции со страницами не отменяются); дерево слоёв — виртуализировано (основной поток запрашивает у worker'а только видимые строки с запасом, DOM-строки переиспользуются по месту), переименование на месте (одно поле для файла, страницы и слоя: Enter и уход фокуса применяют, Esc отменяет, фокус возвращается к строке; поле, убранное посреди набора, применяет набранное), перетаскивание для порядка и вложенности (над, под, внутрь фрейма или группы; положение на экране сохраняется; переносится взятое при нажатии, клавиши во время жеста не действуют, Esc отменяет), скрыть и заблокировать, поиск, наведение подсвечивает узел на холсте, выделение синхронно в обе стороны (выделение с холста раскрывает предков и прокручивает к строке).
- **Нижний плавающий тулбар по центру:** перемещение, фрейм с пресетами, фигуры (выпадающий список: прямоугольник, эллипс, линия, изображение; на кнопке — последняя выбранная). Перо и текст — подсистема 3, «Комментарий» — подсистема 4 (в автономной сборке отключён с подсказкой).
- **Правая панель:** сверху зум (выпадающий список масштабов) и «Поделиться» (неактивна до подсистемы 4); вкладки «Дизайн» и «Прототип» (вторая неактивна). Секции «Дизайн»: Фрейм/Положение (пресет размера одним шагом, X/Y/W/H, поворот в `(-180, 180]`, отразить) → Вид (прозрачность, радиус, обрезка) → Заливка → Обводка → Экспорт (1×/2×/3×, PNG). Без выделения — фон страницы и экспорт страницы. Значения и «Смешанные» сводит worker; у группы X/Y/W/H — рамка детей, W/H масштабируют детей.
- **Поле числа:** выражения (`120+16`, `50%`, `*2`), у «Смешанных» `+N` — прибавка к каждому; Enter применяет, Esc откатывает, стрелки — шаг; перетаскивание подписи (Shift ×10, Alt ×0.1) — превью и один шаг отмены. Правка без изменений шага не создаёт.
````

В `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` заменить:

````markdown
- **Письма:** упоминание и ответ в обсуждении, где человек участвует, если он не открыл файл. Письма группируются за 5 минут, в письме есть ссылка на обсуждение. Настройка «Письма о комментариях» в настройках, по умолчанию включена. Очередь — таблица `mail_outbox` в Postgres, чтобы письмо не терялось при перезапуске.

## 11. Офлайн и версии (этап 6)

````

на:

````markdown
- **Письма:** упоминание и ответ в обсуждении, где человек участвует, если он не открыл файл. Письма группируются за 5 минут, в письме есть ссылка на обсуждение. Настройка «Письма о комментариях» в настройках, по умолчанию включена. Очередь — таблица `mail_outbox` в Postgres, чтобы письмо не терялось при перезапуске.

- **Сделано (этап 5):**
  - **Протокол.** `packages/protocol/src/comments.ts`: виды REST, строгий разбор ответов и событий, длина текста в кодовых точках (`commentLength`, одна функция у клиента и сервера), нормализация (переводы строк, без управляющих и знаков направления), `@[userId]` — не больше 20 разных в комментарии. Событие — кадр `comment` (27) на WebSocket файла: JSON до 64 КБ, каждое поле проверено, лишние поля — неверный кадр. Прежний клиент такой кадр счёл бы испорченным соединением — **протокол 3**, прежняя вкладка получает `please-reload`.
  - **Расхождение с §10.1: модель.** Сверх спеки: `comment_file(file_id, seq)` — номер изменений комментариев файла (не строка `file`: её держат журнал и сжатие); у `comment_thread` — `wx`, `wy`, `first_id`, `deleted_at` (мягкое удаление: надгробие для `?after=`), сводка `replies`, `last_at`, `participants` (список 1000 обсуждений без неё — 325 мс, с ней — 98 мс); `user_pref(comment_mail)`; `mail_outbox` (раздел о письмах ниже). Комментарии удаляются сразу, обсуждения — мягко. Удаление человека: его комментарии остаются с автором «Удалённый пользователь» **(поведение)**, упоминания, прочтения и письма ему — уходят.
  - **Сервер** (`comment-routes.ts`, функции базы в `010_comments.sql`). `GET/POST/PATCH/DELETE /api/files/:id/comments…`, `…/:threadId/read`, `GET …/mentions?q=`, `GET/PUT /api/me/prefs`. Каждый маршрут — через конвейер `api.ts` (роль, предел), обработчик — один запрос к базе: изменение — функция plpgsql, которая сначала берёт блокировку номера файла и сверяет роль сама (снятая между проверкой конвейера и записью роль не пишет). **Расхождение с DESIGN: функция, а не один оператор с CTE:** оператор читает снимком своего начала, до ожидания блокировки, и ответ с номером N мог не видеть изменения N−1. Чужие id (обсуждение другого файла по маршруту этого) — 404 без записи. Пределы: 60 комментариев в минуту на человека по всем вкладкам и сеансам (Valkey), список — свой предел 120 в минуту (страница 500 обсуждений стоит ~50 мс базы), страница — до 500, обсуждение — до 500 комментариев (`THREAD_FULL`), подсказка — 8 человек, запрос до 64 символов.
  - **Права (поведение, как в Figma).** Комментируют, отвечают и читают все с доступом, читатели и пришедшие по ссылке тоже. Править — только свой комментарий, без срока. Удалить — свой, владелец файла — любой; удаление первого удаляет обсуждение у всех. Закрыть и открыть снова — автор обсуждения, редактор, владелец. Двигать метку — автор обсуждения и владелец. Упоминание заводится только для того, у кого есть доступ при отправке, остальные `@[id]` показываются «@неизвестный», без имени и письма. Подсказка упоминаний — люди с доступом к файлу; адрес виден и ищется только участнику файла (прямая роль или команда), пришедшему по ссылке — только имена: опубликованная ссылка не раздаёт адреса.
  - **Живое обновление.** Событие — только соединениям с подтверждённой `hello` ролью и после одного запроса ролей на проход (снятый доступ не получает событие и до сверки ролей). Порядок — по `seq`: проходы рассылки собирают накопленное, пришедшее раньше соседа ждёт его до 50 мс, очередь файла — до 1024 событий, дальше всем `gap`. Медленный читатель (буфер больше 64 КБ) событие пропускает и получает `gap` с головой файла, очереди на соединение нет. Клиент применяет событие `seq = известный + 1`, пропускает старое, на дыру, `gap` и новое соединение догоняет `GET ?after=известный` страницами — без дублей.
  - **Письма** (`comment-mail.ts`). Очередь — `mail_outbox`: одна ждущая строка на человека и файл, новые комментарии сливаются в неё (до 20 ссылок и счётчик остальных), срок — 5 минут от первого (`COMMENT_MAIL_DELAY_MS`, стенд тестов — 2 с). Кому: упомянутым и участникам обсуждения при ответе, кроме автора и тех, у кого файл открыт в момент комментария. При отправке снова: файл открыт сейчас или открыт после первого комментария, доступ снят, письма выключены, комментарии удалены — письма нет. **Не больше одного раза:** строка берётся на отправку и фиксируется до SMTP; найденная после сбоя в этом состоянии не отправляется снова (`uncertain`, в журнал), известный отказ SMTP — повтор до 5 раз. Пределы: 50 ждущих строк и 30 писем в час на человека. Тема — одной строкой без управляющих символов, HTML экранирован, ссылка — `#/file/<id>?comment=<threadId>`. Настройка «Письма о комментариях» — в «Аккаунте», по умолчанию включена.
  - **Ядро.** Рендер (`ferrum-render/src/pins.rs`): метка — капля с инициалами цветом автора (тот же, что у курсора), счётчик ответов, отметка «не прочитано», открытая и закрытая; группа — круг с числом; готовыми картинками, не больше 8 новых за кадр, кэш 256. Редактор (`ferrum-editor/src/comments.rs`, `tools/comment.rs`): инструмент «Комментарий» (C, тулбар, палитра, меню «Вид»), Shift+C — показ меток, в режиме комментариев остальные инструменты не работают, камера работает. Раскладка меток — каждый кадр по сцене (своё и чужое живое превью тоже), метки за краем холста не раскладываются, группы — в экранных пикселях. Метки не в документе, не в истории и не в `own_change_counter`, превью и экспорт их не видят. Ввод хозяина ядро проверяет само (координаты, id, инициалы, до 10 000 меток).
  - **(поведение)** Привязка — к верхнему узлу страницы под точкой (вложенный фрейм — к верхнему), точка — в его координатах; узел скрыт или удалён — последняя мировая точка. Остриё капли — точка метки. Щелчок по группе вписывает её метки не меньше чем вдвое ближе. Вне режима метки ловят нажатие у «Перемещения» и «Масштаба», Shift+C их прячет. Протяжка своей метки (автор, владелец) — сразу на новом месте, отказ сервера возвращает её. Закрытые обсуждения скрыты и на холсте, и в панели, пока не включено «Показать закрытые».
  - **Клиент и интерфейс.** Комментарии живут в `sync.worker` (`CommentStore`): запросы, порядок по `seq`, непрочитанное у каждого своё (открытое обсуждение читается сразу), метки ядру прямым портом, основному потоку — изменившиеся обсуждения не чаще раза в 50 мс и открытое обсуждение. Основной поток рисует окно у метки и панель: окно — комментарии, ответ, правка своего, удаление (второй щелчок подтверждает), «Закрыть обсуждение»/«Открыть снова»; поле — Enter отправить, Shift+Enter новая строка, Esc отмена, `@` — подсказка, на сервер уходит `@[userId]`; клавиши поля холсту не достаются. Щелчок мимо окна закрывает его, а новый комментарий тем же щелчком не ставится. Панель справа в режиме комментариев: эта страница или все, по дате или сначала непрочитанные, «Только мои», «Показать закрытые»; список виртуальный, живое изменение меняет только свою строку. Щелчок по строке — страница, камера на метке, окно. Ссылка `?comment=` открывает файл на обсуждении; нет такого или нет доступа — «Обсуждение не найдено или нет доступа», без доступа к файлу — экран «Нет доступа».
  - **(поведение)** Автономная сборка: комментариев нет, кнопка в тулбаре отключена с подсказкой «только в файлах на сервере» (как в Figma без сети), C и Shift+C ничего не делают, кода комментариев в сборке нет (`check-dist`, `FILES_MARKER`). Читатель и файл «только для просмотра» — комментарии есть.
  - **Замеры** (Apple Silicon, машина под чужой нагрузкой):
    - Кадр с 1000 метками на экране, прибавка к спискам кадра, цель 1 мс: 1920×1080 — 0,298 мс, 2× — 0,517 мс; 80 групп и 20 меток на мелком зуме — 0,044 и 0,102 мс; первый кадр, пока картинки строятся, — 2,2–2,6 мс один раз (B-407). Раскладка меток за кадр при перетаскивании 10 фреймов со 100 метками (страница 100 000 узлов, 1050 меток): зум 1 — 0,050 мс, своё/чужое превью одинаково, 0,25 — 0,085 мс, вся страница с 84 группами — 0,34–0,36 мс; без отсечения меток за краем было 0,42 мс.
    - Сервер, файл с 1000 обсуждений × 20 комментариев: список целиком (2 страницы, 522 КБ) p50 98 мс, p95 110 мс; `?after=` p95 3,7 мс; обсуждение p95 3,7 мс; новое p95 4,5 мс; ответ p95 4,9 мс; подсказка p95 4,3 мс. EXPLAIN — только индексы. Рассылка события 50 соединениям одного файла: от ответа REST до прихода p50 1,8 мс, p95 2,8 мс, процессор 2 %; под враждебным человеком (5 сеансов, 40 потоков запросов и сокет, который не читает) — p50 5,9 мс, p95 46 мс, доставлено всё, принято ровно 60 его комментариев, ~52 000 ответов 429, задержка цикла событий p95 3 мс; без пакетной рассылки было p95 1565 мс. Медленный читатель: пик буфера 82 КБ, затем `gap`.
    - Панель с 1000 обсуждений (`comments-panel.spec.ts`, стенд): открытие режима 204 мс, строк в DOM 11, длинных задач при прокрутке в конец и живом изменении нет.

## 11. Офлайн и версии (этап 6)

````

- [ ] **Step 2: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `npm run test:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build:server && npm run e2e:server`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test --workspace`

Run: `scripts/compat-old-build.sh origin/main guides_fixture`

Expected: PASS: всё без замечаний; протокол 56 pass, сервер 597 pass; `unit` 345 passed; `e2e:ci` 415 passed и 1 flaky (`booleans.spec.ts:46`, прошёл повтором); `e2e:server` 181 passed, 11 skipped (прежние пропуски Firefox), 2 failed — `sync.spec.ts:429` @big (задержка 1094 мс при пределе 1000 под нагрузкой машины) и `files.spec.ts:355` в Firefox (нет экрана «Нет доступа» после выключенной ссылки); оба отдельно прошли (см. «Проверка плана»); `cargo test --workspace` 1381 passed; совместимость с `origin/main` подтверждена

- [ ] **Step 3: Закоммитить**

```bash
git add docs/backlog.md docs/running.md docs/superpowers/specs/2026-10-23-ferrum-core-design.md docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md
git commit -F - <<'EOF'
docs: комментарии — итоговое поведение этапа 5, замеры, бэклог B-407…B-418

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

## Что этап 5 сознательно не делает

Офлайн-очередь и версии (этап 6), десктоп (этап 7). Реакции, вложения, форматирование текста и комментарии в автономной сборке. Чистку надгробий и предел числа обсуждений (B-411), прочтение в других вкладках (B-412), несколько процессов сервера (B-415), свой курсор режима (B-409).
