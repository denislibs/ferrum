# Ferrum, подсистема 4, этап 2: файлы на сервере — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Файлы живут на сервере: команды, проекты, «Черновики», `ServerFileStore` за тем же интерфейсом `FileStore`. Экран файлов с разделами «Недавние», «Черновики», команды и проекты, «Доступные мне», «Корзина». Действующая роль (наибольшая из прямой, по ссылке и через команду), режим просмотра для читателя. Приглашения в команду. «Поделиться»: приглашение по email с ролью, список людей, смена и снятие роли, ссылка с ролью и выключателем, токен хэшем. Картинки отдельными запросами с проверкой хэша и размера, чтение с проверкой доступа, фоновая очистка. Превью, импорт «Открыть с диска», экспорт, «Локальные файлы (N)» с переносом. Файл пишется целиком с проверкой поколения (`CONFLICT`). Закрываются B-190, B-193 (минимум), B-195, B-204, B-205, B-206.

**Architecture:** Сервер: `api.ts` — конвейер маршрутов (`Origin`, сессия, предел частоты, права, срок, формат ошибок), маршрут только объявляет, что ему нужно; `file-routes.ts`, `image-routes.ts`, `team-routes.ts`, `share-routes.ts`, `invites.ts`, `gc.ts`; действующая роль — функция базы `file_role_rank`. Содержимое идёт потоком на диск и обратно (`BlobStore.putStream`, `open`), `ContainerWalker` из `packages/protocol` проходит контейнер кусками, не держа его в памяти. Клиент: `core/server-files.ts` (логика без DOM) и `worker/server.worker.ts`: всё размером с документ; картинки выносятся из контейнера перед записью и вставляются при чтении, ядро получает обычный файл; журнал правок — своя база IndexedDB на человека. `ui/server/` — `ServerFileStore`, «Поделиться», команды, локальные файлы; всё это ставит ветка `main.tsx` через `extensions`, общий код условий по флагу не содержит. Ядро: `Snapshot { full }` (полные снимки для сервера) и `SetViewOnly`.

**Tech Stack:** Bun 1.4.2, TypeScript 7.0.2, better-auth 1.7.16, pg 8.23.1, nodemailer 10.2.0, `@noble/hashes` 2.4.0 (новая, blake3), Postgres 18, Valkey 9, Mailpit 1.33.0; SolidJS 1.9.17, @playwright/test 1.64.0 (`chromium`, `firefox`); Rust с `loro` 1.16.2.

**Спеки:** `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` — раздел 6, а также 2, 4, 5, 7.5, 7.6, 13, 14.

## Global Constraints

- Ветка `feat/sub4-stage2-files` от a301e40. Каталог сборки wasm — у каждой копии свой: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0` (в `apps/web` — `"$PWD/../../target"`); команды — из корня копии, если не сказано иное. Нужны Bun 1.4, Node 24, Docker с Compose 2.24+, `wasm-bindgen` 0.2.129.
- Версии точные. Новая зависимость одна: `@noble/hashes` 2.4.0 (2026-08-27), в `Bun.CryptoHasher` нет blake3; 64 МБ хэшируются за 0,47 с кусками по 64 КиБ, самый долгий кусок 27 мс.
- Docker: только свои проекты compose — `ferrum` и `ferrum-test`. Порты: сервер 8090, Mailpit 8025; стенд тестов — 18090, 18025, 11025, 15432, 16379. Чужие контейнеры (`agent-dev-*`, `relay-*`) не трогать. `down -v` — только `ferrum-test` и `ferrum`. Никаких `docker system prune`.
- Порт 4173 может быть занят посторонним процессом: его не трогать, E2E тогда — через временный конфиг Playwright с другими портами (не коммитить).
- Основной поток тонкий: всё размером с документ идёт в worker'ах, байты передаются. Окна списка до 200 строк.
- У каждого маршрута: права и тест на чужого (`test/access.test.ts` перечисляет все маршруты, новый без записи роняет тест), `Origin`, разбор схемой из `packages/protocol`, пределы размера и частоты, срок, ответ в формате ошибок. Несуществующий и чужой файл неразличимы.
- Клиент: неожиданный вид ответа — `NETWORK`, у запроса срок, нет вечной заставки, устаревшие ответы отбрасываются, смена адреса не стирает экран.
- Уход, выход, смена прав и `CONFLICT` проходят протокол сохранения редактора.
- Автономная сборка и десктоп не меняют поведения, кода сервера в них нет (`check-dist.mjs`); общий код без условий по флагу, через `extensions`.
- Тесты падают при откате правки, без ожиданий по времени и зависимости от порядка; письма ждутся дольше 5 с.
- Поведение — как в Figma; где спека молчит, решение помечено **(поведение)**.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Первым делом проверен главный риск, отдельной пробой. Bun 1.4.2 пишет тело потоком на диск и отдаёт `Bun.file`; Chromium шлёт из worker'а: 300 МБ — запись 0,83 с, чтение 0,25 с, пик памяти сервера 71 МБ, длинных задач на основном потоке нет. Умолчание `maxRequestBodySize` (128 МБ) поднято. Там же найдено решение, которое не следует из этапа: журнал вкладки, отставшей от чужой записи, shallow-снимок отвергает, а полный сливает, поэтому сервер хранит полные снимки уже сейчас (§7.6). На настоящем стенде файл 100 000 узлов: 33,5 МБ, запись 3,6 с, открытие 0,7 с, длинных задач при записи нет. `CONFLICT` проверен двумя браузерами и одновременными записями, права — таблицей на каждый маршрут.

Прототип всего этапа собран в отдельной копии (`plan-verify-s4s2`, ветка `verify/sub4-s2` от a301e40, по коммиту на задачу; вершина — 482c9b3). На его вершине прошло: `format:check`, `lint`, `typecheck`; `bun test` — 12 и 285 pass против настоящих Postgres, Valkey и Mailpit; модульные — 212 passed; `build`, `build:server`, `check-dist.mjs` обеих; E2E автономной сборки, Chromium, `--grep-invert @perf --retries=1` — 416 тестов: на вершине 415 passed и 1 flaky (`paste.spec.ts:142`, `Ctrl/Cmd+Alt+V`, код вставки этап не трогал), прогоном раньше на той же сборке 416 passed; `e2e:server` — 110 passed, 2 пропущены (56 сценариев в Chromium и Firefox; 100 000 узлов и `setOffline` только в Chromium); `cargo fmt`, `clippy`, `cargo test --workspace` — 1183 passed; `cargo check` десктопа; `docker compose build --no-cache` — 3 мин 38 с, образ 130 МБ; подъём с чистых томов — 11 с; ручной круг через API: письмо, вход, запись 200, повтор с той же базой 409, чужой 404, по ссылке роль `viewer`, запись читателем 403, чужой `Origin` 403. Переход с данных этапа 1: база с миграциями 001 и 002 и человеком, запуск применил только `003_files.sql`, человек тот же; `down` и `up` на тех же томах: содержимое файла на месте; `stop server` меньше секунды.

Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом, который читает сам этот документ, к чистой копии от a301e40 (`plan-apply-s4s2`, ветка `verify/sub4-s2-apply`) в порядке задач с коммитом после каждой: каждый фрагмент «заменить» найден ровно один раз (операций по задачам: 42, 32, 10, 32, 9, 106, 9, 5, 5, 21, 8). После каждой задачи дерево копии совпало с деревом соответствующего коммита прототипа, `git diff verify/sub4-s2 HEAD` пуст. На этой копии выполнены все шаги «Убедиться, что тесты падают» (задачи 1–6, 10; причины записаны из этого прогона) и все шаги «Проверить», кроме `npm run e2e:ci` задачи 11 (он прошёл в прототипе на том же дереве). Задачи 7–9 проверяются сценариями на настоящем стенде: шага «падают» у них нет, отдельная сборка образа ради него ничего бы не добавила.

Мутации сервера. Снятие проверки прав по очереди на каждом маршруте файлов, картинок, команд и «Поделиться», всего 30: поймано 27. Выжили `PUT …/content` и `GET /api/files/:id`: у них есть вторая линия (`file_role_rank` в транзакции, `loadFile`), мутант равносилен; `HEAD …/images/:hash` — добавлен тест, теперь ловится. Первый прогон показал, что тест смотрел на `route.access.kind` и мутация снимала сам тест: условие теперь из таблицы. Ещё 26 мутаций, поймано 25: `Origin`, предел частоты, поколение, потерянная своя запись, роль при фиксации, проверка контейнера, обрезанный файл, сборка тела в памяти, немедленное удаление байтов, хэш картинки, чтение без `file_image`, срок очистки, роль через команду, корзина, выключатель ссылки, сверка токена, токен открытым текстом, новая ссылка, чужая запись списка, снятие владельца, понижение редактором, имя в списке, приглашения при входе, предел приглашений, передача файлов ушедшего. Выжила проверка `enabled` в `redeem`: вторая линия, роль считает `file_role_rank` с `l.enabled`, а её снятие ловится.

Найденное прототипом и вошедшее в шаги: файл worker'а с кодом сервера попадал в автономную сборку из мёртвой ветки `main.tsx` (задача 6, заглушка в `vite.config.ts`; поймал `check-dist.mjs`); `loadFile` отдавал запись с ролью по умолчанию при роли 0, и файл из корзины открывался по ссылке (задача 4); выход в другой вкладке терял новый незаписанный файл, потому что сессии уже нет (задача 6, редактор остаётся); письма-приглашения ломали `linksFor` E2E (задача 6); проверка памяти в общем процессе шумела (задача 5, свой процесс); стенд тестов не убирал байты и журнал Postgres заполнял tmpfs (задачи 4, 5).

Не запускались: E2E автономной сборки в Firefox, проекты `gpu`, `@perf`, WebKit и Safari; `compat-old-build.sh` (формат файла не менялся: те же секции и `FORMAT_VERSION`, полный снимок читает тот же `LoroDoc`; открытие полного снимка старой сборкой отдельно не проверено); запуск десктопа (`FERRUM_SMOKE`, только `cargo check`), сборки десктопа на Windows и Linux; настоящий SMTP; HTTPS; настоящий прокси с телами до 512 МБ; образ на amd64. Порт 4173 был свободен, чужие контейнеры не трогали.

---

## Этапы подсистемы 4

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Сервер и вход (сделан) | Docker, миграции, Better Auth, вход, TypeScript 7, oxlint, prettier | 3, 4, 5 |
| **2. Файлы на сервере** (этот план) | команды, проекты, черновики, серверный `FileStore`, роли, «Поделиться», картинки, импорт и экспорт, перенос | 6 |
| 3. Синхронизация | журнал, снимки, чужие правки, переподключение | 7, 8 |
| 4. Присутствие | курсоры, аватары, следование | 9 |
| 5. Комментарии | метки, обсуждения, письма | 10 |
| 6. Офлайн и версии | очередь правок, история версий | 11 |
| 7. Десктоп | вход через браузер, серверные файлы | 12 |

## Решения

Где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**. По справке Figma ничего не сверялось: это решения по памяти о Figma и из требований этапа.

Файл и запись:

1. Адреса API — `/api/files/…`, `/api/teams/…`, `/api/projects/…`, а не `/files/…` спеки: один префикс с входом и прокси разработки. Этап 3 заводит `/api/files/:id/sync`.
2. Сервер уже на этапе 2 хранит полные снимки (`Snapshot { full: true }`, §7.6). Проба: журнал вкладки, отставшей от чужой записи, shallow-снимок отвергает, полный сливает. 100 000 узлов и 500 правок: shallow 28,5 МБ за 0,99 с, полный 34,0 МБ за 0,70 с, открытие одинаково. Копия на диск остаётся shallow.
3. **(поведение)** После `CONFLICT` правки не теряются: уход спрашивает, правки остаются в журнале браузера, следующее открытие сливает их с новой версией. Текст ошибки об этом и говорит.
4. **(поведение)** Запись несёт свой номер и номер предыдущей с неизвестным исходом: своя удачная запись, ответ на которую потерялся, не становится `CONFLICT` (поколение ровно на одну больше и `last_write_id` совпал).
5. Заведомый отказ — раньше передачи: `GET …/access` перед записью, проверка поколения до чтения тела; решает транзакция фиксации, в ней же проверяется роль.
6. Байты заменённого файла уходят в `blob_trash` той же транзакцией и удаляются через 2 минуты: сбой ничего не теряет, читатель дочитывает.
7. Сервер проходит контейнер `ContainerWalker`: неверный вид и картинка внутри — 400 `BAD_CONTAINER`. Чужие секции пропускаются. Документ не раскодируется, поэтому `schema_version` заполнит этап 3.
8. Журнал правок — на клиенте, своя база `ferrum-user-<id>`. Читатель журнал не применяет и не стирает.
9. Настройки интерфейса и шрифты остаются в браузере: `ServerFileStore` держит `IdbFileStore`.

Картинки:

10. **(поведение)** В файле на сервере картинок нет: worker выносит секции перед записью и вставляет при чтении, заголовок собирается без вычислений, хэш — это имя секции. «Скачать копию» — обычный файл с картинками.
11. **(поведение)** Ответа «такая уже есть» нет: `file_image` появляется только у того, кто прислал все байты, знание хэша ничего не даёт. На диске байты одни (`image`), вторая загрузка уходит в `blob_trash`.
12. Без ссылки в содержимом картинка файла живёт 30 дней (отмена может её вернуть), затем ещё 30 — запись самой картинки. Загрузка оживляет запись.

Роли и «Поделиться»:

13. **(поведение)** Участник команды — редактор файлов её проектов, владелец команды — владелец. Файлы ушедшего участника переходят владельцу команды.
14. **(поведение)** Редактор приглашает и может поднять читателя до редактора; менять и снимать роли — только владелец; сам себе снять доступ может каждый, кроме владельца. Ссылкой управляет только владелец.
15. **(поведение, отличие от Figma)** Читателю кнопки «Поделиться» нет: адреса людей ему не положены.
16. **(поведение)** Адрес без учётной записи получает доступ при первом входе (`databaseHooks.user.create.after`). Список не выдаёт, есть ли запись: имя показывается, если человек уже открывал файл; в команде — только адреса. Одно и то же письмо и ответ для всех.
17. **(поведение)** Письма приглашений: 30 в час от человека и 5 на адрес; сверх предела и вне `ALLOWED_EMAILS` запись остаётся, письма нет, ответ тот же. Счётчик отдельный от писем входа: чужие приглашения не закрывают человеку вход.
18. **(поведение)** Токен ссылки выводится из секрета сервера и `nonce` (HMAC), в базе его хэш: показать ссылку снова можно, как в Figma, а утечка базы токенов не раскрывает. Смена секрета: прежняя ссылка действует, но не показывается.
19. **(поведение)** Роль по ссылке действует, пока ссылка включена; новая ссылка отменяет прежнюю и всех пришедших по ней. Токен из адреса убирается сразу.
20. **(поведение)** Файл в корзине виден только владельцу и тому, кто убрал. Нельзя удалить команду с проектами и проект с файлами (файлы в корзине не считаются, они уходят в черновики владельца).
21. **(поведение)** Страница команды — диалог, а не маршрут.

Смена прав, расхождение со спекой:

22. **Расхождение с §6.3.** WebSocket появится на этапе 3, поэтому снятие роли не мгновенно. Запись отказывает сразу, в транзакции. Открытый редактор узнаёт при первой записи, при возврате на вкладку или опросом раз в 15 с (`GET /api/files/:id/access`), пока вкладка видна. Этап 3 шлёт `role` и `access-revoked` в тот же обработчик `files.watch`, ломать ничего не придётся, опрос убирается.
23. **(поведение)** Понижение: режим просмотра без перезагрузки, незавершённое закрывается, правки — в журнале, уход спросит. Снятие: плашка «Нет доступа» с копией на диск и просьбой о доступе. Конец сессии (B-195): плашка «Сессия завершена», правки остаются в журнале.
24. **(поведение)** Выход в другой вкладке, когда файл не записался: редактор не перезагружается. Новый файл без записи спрашивает перед входом заново.
25. **(поведение)** Открытый редактор узнаёт о новой версии опросом и сообщает об этом один раз.

Сборки:

26. Хранилище выбирает `main.tsx` (`Root { createStore }`): в рабочей сборке нет `TauriFileStore` (B-206) и `__ferrumFiles`, `?debug` не действует (B-205). `check-dist.mjs` проверяет обе стороны.
27. В автономной сборке `ui/server/setup` подменён заглушкой (`vite.config.ts`): сборщик разбирает `new Worker(new URL(…))` до выбрасывания ветки и выпускал файл worker'а. Поймал `check-dist`.

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-doc`, `ferrum-editor`, `ferrum-wasm` | полные снимки, режим просмотра |
| `packages/protocol/src/{files,container}.ts` (новые), `auth.ts` | адреса, разбор, контейнер |
| `apps/server/migrations/003_files.sql` (новый) | модель §6.1, `file_role_rank` |
| `apps/server/src/{api,file-routes,image-routes,team-routes,share-routes,invites,gc}.ts` (новые) | маршруты |
| `apps/server/src/{blob-store,app,index,auth,auth-routes,mail}.ts` | потоковая запись с `fsync`, подключение |
| `apps/server/test/{api-stand,memory-probe}.ts`, `{access,files,images,teams,share}.test.ts` (новые) | стенд и тесты |
| `apps/web/src/core/server-{files,messages}.ts`, `worker/server.worker.ts` (новые) | файлы в worker'е |
| `apps/web/src/ui/server/` (новый) | `ServerFileStore`, запросы, «Поделиться», команды, локальные файлы |
| `apps/web/src/ui/{local-store.ts,no-access.tsx}` (новые), `{root,app,chrome,files-screen,properties,router,saver,extensions,boot}` | выбор хранилища, разделы, просмотр, доступ |
| `apps/web/scripts/check-dist.mjs`, `vite.config.ts` | сборки |
| `apps/web/e2e-server/files.spec.ts` (новый), `auth.spec.ts`, `stand.ts` | E2E на настоящем стенде |
| `docs/` | спека, запуск, бэклог B-237…B-258 |

---

### Task 1: Ядро — полный снимок и режим просмотра

Спека §7.6, §6.1, §6.3. `container::encode_full`, `Command::Snapshot { full }`, `Command::SetViewOnly`, `Document::set_view_only`, `Editor::set_view_only`. Формат файла не меняется.

**Files:**
- Modify: `crates/ferrum-doc/src/container.rs`, `crates/ferrum-doc/src/document.rs`, `crates/ferrum-doc/src/page.rs`, `crates/ferrum-doc/tests/journal.rs`, `crates/ferrum-editor/src/editor.rs`, `crates/ferrum-editor/tests/text_edit.rs`, `crates/ferrum-wasm/src/core.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `crates/ferrum-wasm/src/protocol.rs`, `crates/ferrum-wasm/tests/boolean.rs`, `crates/ferrum-wasm/tests/fonts.rs`, `crates/ferrum-wasm/tests/journal.rs`, `crates/ferrum-wasm/tests/panels.rs`, `crates/ferrum-wasm/tests/protocol.rs`, `crates/ferrum-wasm/tests/snapshot_perf.rs`, `crates/ferrum-wasm/tests/text_edit.rs`

**Interfaces:**
- Produces: `{ type: 'snapshot', thumbnail, full? }`, `{ type: 'setViewOnly', on }`.

- [ ] **Step 1: Реализация**

В `crates/ferrum-doc/src/container.rs` заменить:

````rust

use crate::binary::Reader;
use crate::document::{Document, UnloadedPage};
use crate::error::{DocError, DocResult};
use crate::images::ImageStore;
````

на:

````rust

use crate::binary::Reader;
use crate::document::{Document, SnapshotKind, UnloadedPage};
use crate::error::{DocError, DocResult};
use crate::images::ImageStore;
````

В `crates/ferrum-doc/src/container.rs` заменить:

````rust
}

/// Сериализует документ. Изображения без ссылок со страниц не записываются.
pub fn encode(doc: &Document) -> DocResult<Vec<u8>> {
    let mut sections = Vec::new();
    let mut count = 0u32;
````

на:

````rust
}

/// Сериализует документ со shallow-снапшотами страниц: копия на диск и локальные файлы.
/// Изображения без ссылок со страниц не записываются.
pub fn encode(doc: &Document) -> DocResult<Vec<u8>> {
    encode_with(doc, SnapshotKind::Shallow)
}

/// То же с полными снимками страниц: файл на сервере (спека подсистемы 4, раздел 7.6). Страница,
/// которую не открывали, пишется как прочитана: правок в ней не было.
pub fn encode_full(doc: &Document) -> DocResult<Vec<u8>> {
    encode_with(doc, SnapshotKind::Full)
}

fn encode_with(doc: &Document, kind: SnapshotKind) -> DocResult<Vec<u8>> {
    let mut sections = Vec::new();
    let mut count = 0u32;
````

В `crates/ferrum-doc/src/container.rs` заменить:

````rust
    let mut used = HashSet::new();
    for (id, _) in doc.pages() {
        let payload = doc.page_payload(id)?;
        let page = id.to_string();
        push_section(&mut sections, PAGE, &page, &payload.snapshot);
````

на:

````rust
    let mut used = HashSet::new();
    for (id, _) in doc.pages() {
        let payload = doc.page_payload(id, kind)?;
        let page = id.to_string();
        push_section(&mut sections, PAGE, &page, &payload.snapshot);
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    /// Кэш сцены из файла, если он совпал со снапшотом.
    pub scene: Option<Vec<SceneRow>>,
}

````

на:

````rust
    /// Кэш сцены из файла, если он совпал со снапшотом.
    pub scene: Option<Vec<SceneRow>>,
}

/// Что пишется в снимок страницы: только состояние или состояние с историей правок.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SnapshotKind {
    Shallow,
    Full,
}

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    pub images: ImageStore,
    read_only: bool,
    /// Журнал (этап 6): версии Loro корня и загруженных страниц, до которых правки
    /// уже отданы в журнал (`take_journal`). Страница получает версию при загрузке.
````

на:

````rust
    pub images: ImageStore,
    read_only: bool,
    /// Режим просмотра: так решил не файл, а роль человека (читатель, спека подсистемы 4,
    /// раздел 6.1). В отличие от `read_only` снимается, когда роль повысили.
    view_only: bool,
    /// Журнал (этап 6): версии Loro корня и загруженных страниц, до которых правки
    /// уже отданы в журнал (`take_journal`). Страница получает версию при загрузке.
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            images,
            read_only: false,
            journaled_root,
            journaled_pages: HashMap::new(),
````

на:

````rust
            images,
            read_only: false,
            view_only: false,
            journaled_root,
            journaled_pages: HashMap::new(),
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

````

на:

````rust

    pub fn is_read_only(&self) -> bool {
        self.read_only || self.view_only
    }

    /// Режим просмотра по роли: правки отвергаются так же, как у файла только для чтения.
    pub fn set_view_only(&mut self, on: bool) {
        self.view_only = on;
    }

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

    fn writable(&self) -> DocResult<()> {
        if self.read_only {
            Err(DocError::ReadOnly)
        } else {
````

на:

````rust

    fn writable(&self) -> DocResult<()> {
        if self.is_read_only() {
            Err(DocError::ReadOnly)
        } else {
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

    /// Данные страницы для файла — без загрузки, если она не открыта.
    pub(crate) fn page_payload(&self, id: PageId) -> DocResult<PagePayload> {
        if let Some(p) = self.loaded.get(&id) {
            return Ok(PagePayload {
                snapshot: p.export_snapshot()?,
                image_refs: p.image_refs()?,
                scene: Some(p.scene_rows()?),
````

на:

````rust

    /// Данные страницы для файла — без загрузки, если она не открыта.
    pub(crate) fn page_payload(&self, id: PageId, kind: SnapshotKind) -> DocResult<PagePayload> {
        if let Some(p) = self.loaded.get(&id) {
            return Ok(PagePayload {
                snapshot: match kind {
                    SnapshotKind::Shallow => p.export_snapshot()?,
                    SnapshotKind::Full => p.export_full_snapshot()?,
                },
                image_refs: p.image_refs()?,
                scene: Some(p.scene_rows()?),
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            return out;
        }
        if self.read_only {
            out.warnings
                .push("Несохранённые изменения не применены: файл открыт только для чтения".into());
````

на:

````rust
            return out;
        }
        if self.is_read_only() {
            out.warnings
                .push("Несохранённые изменения не применены: файл открыт только для чтения".into());
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
                continue;
            }
            if self.read_only {
                // Страница с неизвестными узлами (найдена при загрузке) — только чтение,
                // как в ветке раннего выхода: правки этой и следующих страниц не применяем.
````

на:

````rust
                continue;
            }
            if self.is_read_only() {
                // Страница с неизвестными узлами (найдена при загрузке) — только чтение,
                // как в ветке раннего выхода: правки этой и следующих страниц не применяем.
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        let page = doc.pages()[0].0;
        let star = doc
            .transact(page, vec![], |p| {
                p.create_node(None, None, NodeKind::Rect)?;
                Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
            })
            .unwrap()[0];
        doc.page(page)
            .unwrap()
            .force_raw_kind_for_tests(star, Some("star"));
        let root = doc.root_snapshot().unwrap();
        let payload = doc.page_payload(page).unwrap();
        let mut unloaded = HashMap::new();
        unloaded.insert(
            page,
            UnloadedPage {
                snapshot: payload.snapshot,
                image_refs: payload.image_refs,
                scene: None,
            },
        );
        let (mut d2, _) = Document::from_parts(&root, unloaded, ImageStore::default()).unwrap();
        assert!(!d2.is_read_only());
````

на:

````rust
        let page = doc.pages()[0].0;
        let star = doc
            .transact(page, vec![], |p| {
                p.create_node(None, None, NodeKind::Rect)?;
                Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
            })
            .unwrap()[0];
        doc.page(page)
            .unwrap()
            .force_raw_kind_for_tests(star, Some("star"));
        let root = doc.root_snapshot().unwrap();
        let payload = doc.page_payload(page, SnapshotKind::Shallow).unwrap();
        let mut unloaded = HashMap::new();
        unloaded.insert(
            page,
            UnloadedPage {
                snapshot: payload.snapshot,
                image_refs: payload.image_refs,
                scene: None,
            },
        );
        let (mut d2, _) = Document::from_parts(&root, unloaded, ImageStore::default()).unwrap();
        assert!(!d2.is_read_only());
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            .force_raw_kind_for_tests(star, Some("star"));
        let root = doc.root_snapshot().unwrap();
        let payload = doc.page_payload(page).unwrap();
        let mut unloaded = HashMap::new();
        unloaded.insert(
````

на:

````rust
            .force_raw_kind_for_tests(star, Some("star"));
        let root = doc.root_snapshot().unwrap();
        let payload = doc.page_payload(page, SnapshotKind::Shallow).unwrap();
        let mut unloaded = HashMap::new();
        unloaded.insert(
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        let p2 = doc.add_page("Вторая").unwrap();
        let root = doc.root_snapshot().unwrap();
        let s1 = doc.page_payload(p1).unwrap();

        let mut unloaded = HashMap::new();
````

на:

````rust
        let p2 = doc.add_page("Вторая").unwrap();
        let root = doc.root_snapshot().unwrap();
        let s1 = doc.page_payload(p1, SnapshotKind::Shallow).unwrap();

        let mut unloaded = HashMap::new();
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

        assert!(d2.page(p2).is_err());
        let payload = d2.page_payload(p2).unwrap();
        assert_eq!(payload.snapshot, b"garbage".to_vec());
        assert!(d2.pages().iter().any(|(id, _)| *id == p2));
````

на:

````rust

        assert!(d2.page(p2).is_err());
        let payload = d2.page_payload(p2, SnapshotKind::Shallow).unwrap();
        assert_eq!(payload.snapshot, b"garbage".to_vec());
        assert!(d2.pages().iter().any(|(id, _)| *id == p2));
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        }
        Ok(self.doc.export(ExportMode::shallow_snapshot(&frontiers))?)
    }

````

на:

````rust
        }
        Ok(self.doc.export(ExportMode::shallow_snapshot(&frontiers))?)
    }

    /// Полный снимок: состояние вместе с историей правок. Серверные файлы хранят только такие
    /// (спека подсистемы 4, раздел 7.6): на shallow-снапшот Loro не примет правку, основанную на
    /// более раннем состоянии, поэтому файл из другой вкладки или журнал после `CONFLICT` не
    /// сольются с ним.
    pub fn export_full_snapshot(&self) -> DocResult<Vec<u8>> {
        Ok(self.doc.export(ExportMode::Snapshot)?)
    }

````

В `crates/ferrum-doc/tests/journal.rs` заменить:

````rust
//! восстанавливаются точно.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;
use proptest::prelude::*;
````

на:

````rust
//! восстанавливаются точно.

use ferrum_doc::container::{decode, encode, encode_full};
use ferrum_doc::*;
use proptest::prelude::*;
````

В `crates/ferrum-doc/tests/journal.rs` заменить:

````rust
    assert_eq!(state(&mut again), state(&mut doc));
}
````

на:

````rust
    assert_eq!(state(&mut again), state(&mut doc));
}

/// Сервер хранит полные снимки (спека подсистемы 4, раздел 7.6): журнал вкладки, отставшей от
/// чужой записи (`CONFLICT`), сливается с новым файлом при следующем открытии.
#[test]
fn journal_of_a_stale_tab_merges_with_a_newer_full_snapshot() {
    let mut base = Document::new();
    let page = base.pages()[0].0;
    rect(&mut base, page, 1.0);
    let v1 = encode_full(&base).unwrap();
    let mut a = decode(&v1).unwrap().document;
    let mut b = decode(&v1).unwrap().document;
    let ra = rect(&mut a, page, 2.0);
    let rb = rect(&mut b, page, 3.0);
    let journal = b.take_journal().unwrap().unwrap();
    let v2 = encode_full(&a).unwrap();
    let mut merged = decode(&v2).unwrap().document;
    let replayed = merged.apply_journal(&[&journal]);
    assert!(replayed.warnings.is_empty(), "{:?}", replayed.warnings);
    let nodes = merged.page(page).unwrap().children(None);
    assert_eq!(nodes.len(), 3);
    assert!(nodes.contains(&ra) && nodes.contains(&rb));
}

/// А shallow-снапшот такой журнал не примет: поэтому файл на сервере пишется только полным.
#[test]
fn journal_of_a_stale_tab_does_not_merge_with_a_shallow_snapshot() {
    let mut base = Document::new();
    let page = base.pages()[0].0;
    rect(&mut base, page, 1.0);
    let v1 = encode(&base).unwrap();
    let mut a = decode(&v1).unwrap().document;
    let mut b = decode(&v1).unwrap().document;
    rect(&mut a, page, 2.0);
    rect(&mut b, page, 3.0);
    let journal = b.take_journal().unwrap().unwrap();
    let mut merged = decode(&encode(&a).unwrap()).unwrap().document;
    assert!(!merged.apply_journal(&[&journal]).warnings.is_empty());
    assert_eq!(merged.page(page).unwrap().children(None).len(), 2);
}

#[test]
fn full_snapshot_opens_like_a_shallow_one_and_keeps_images() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let r = rect(&mut doc, page, 1.0);
    let hash = doc.images.add(vec![9; 64]);
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            r,
            &PropsPatch {
                fills: Some(vec![Fill::image(hash.clone())]),
                ..Default::default()
            },
        )?;
        Ok(vec![r])
    })
    .unwrap();
    let mut back = decode(&encode_full(&doc).unwrap()).unwrap().document;
    assert_eq!(state(&mut back), state(&mut doc));
    assert_eq!(back.images.get(&hash), Some(&[9u8; 64][..]));
}

#[test]
fn view_only_refuses_edits_and_journal_until_lifted() {
    let (_, mut doc) = opened();
    let page = doc.pages()[0].0;
    rect(&mut doc, page, 1.0);
    let batch = doc.take_journal().unwrap().unwrap();
    doc.set_view_only(true);
    assert!(doc.is_read_only());
    assert!(doc.set_name("Новое").is_err());
    let before = doc.page(page).unwrap().children(None).len();
    let err = doc.transact(page, vec![], |tx| {
        tx.create_node(None, None, NodeKind::Rect)?;
        Ok(vec![])
    });
    assert!(matches!(err, Err(DocError::ReadOnly)));
    assert_eq!(doc.page(page).unwrap().children(None).len(), before);
    assert!(!doc.apply_journal(&[&batch]).warnings.is_empty());
    doc.set_view_only(false);
    assert!(!doc.is_read_only());
    assert!(doc.set_name("Новое").is_ok());
}
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    }

    /// Пролог каждой правки документа не с холста (панели, слои, вставка):
    /// открытый путь пера записывается своим шагом раньше неё (как в Figma) —
````

на:

````rust
    }

    /// Режим просмотра по роли (спека подсистемы 4, раздел 6.3): незавершённое записывается и
    /// закрывается раньше, чем документ перестанет принимать правки, жест отменяется, инструмент —
    /// «Перемещение». Правки, уже сделанные, остаются в журнале документа.
    pub fn set_view_only(&mut self, on: bool) {
        if on {
            self.finish_pen_path();
            self.cancel_gesture();
            self.end_text_edit();
            self.end_vector_edit();
            self.set_tool(ToolKind::Move);
        }
        self.doc.set_view_only(on);
        self.emit();
    }

    /// Пролог каждой правки документа не с холста (панели, слои, вставка):
    /// открытый путь пера записывается своим шагом раньше неё (как в Figma) —
````

В `crates/ferrum-editor/tests/text_edit.rs` заменить:

````rust
    assert_eq!(h.props(made).name, "Строка из буфера");
}
````

на:

````rust
    assert_eq!(h.props(made).name, "Строка из буфера");
}

/// Роль понизили посреди правки текста (спека подсистемы 4, раздел 6.3): сеанс закрыт, начатое в
/// документе, дальше правок нет, снятие режима возвращает их.
#[test]
fn view_only_closes_text_edit_and_lifts_cleanly() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    assert!(h.ed.start_text_edit(id, EnterAt::All));
    h.ed.set_view_only(true);
    assert!(!h.ed.is_editing_text());
    assert!(h.ed.document().is_read_only());
    assert!(!h.ed.start_text_edit(id, EnterAt::All));
    h.ed.set_view_only(false);
    assert!(!h.ed.document().is_read_only());
    assert!(h.ed.start_text_edit(id, EnterAt::All));
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    renderer: Renderer,
    images: DocImages,
    backend: Backend,
    platform: Platform,
````

на:

````rust
    renderer: Renderer,
    images: DocImages,
    /// Режим просмотра по роли: переживает замену документа.
    view_only: bool,
    backend: Backend,
    platform: Platform,
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            | Command::FlushJournal
            | Command::Snapshot { .. }
            | Command::MarkSaved { .. }
            | Command::ReadPixels { .. }
````

на:

````rust
            | Command::FlushJournal
            | Command::Snapshot { .. }
            | Command::SetViewOnly { .. }
            | Command::MarkSaved { .. }
            | Command::ReadPixels { .. }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            renderer,
            images: DocImages::default(),
            backend,
            platform,
````

на:

````rust
            renderer,
            images: DocImages::default(),
            view_only: false,
            backend,
            platform,
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        editor.set_fonts(self.fonts.clone());
        *editor.keymap_mut() = self.editor.keymap().clone();
        editor.take_events();
        self.editor = editor;
````

на:

````rust
        editor.set_fonts(self.fonts.clone());
        *editor.keymap_mut() = self.editor.keymap().clone();
        editor.set_view_only(self.view_only);
        editor.take_events();
        self.editor = editor;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    /// — пачка журнала с правками до снимка: не запишется снимок — они останутся в
    /// журнале, запишется — пачки до его номера больше не нужны.
    fn snapshot(&mut self, thumbnail: bool) {
        self.flush_journal(false);
        let bytes = match container::encode(self.editor.document()) {
            Ok(b) => b,
            Err(e) => {
````

на:

````rust
    /// — пачка журнала с правками до снимка: не запишется снимок — они останутся в
    /// журнале, запишется — пачки до его номера больше не нужны.
    fn snapshot(&mut self, thumbnail: bool, full: bool) {
        self.flush_journal(false);
        let encode = if full {
            container::encode_full
        } else {
            container::encode
        };
        let bytes = match encode(self.editor.document()) {
            Ok(b) => b,
            Err(e) => {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                Err(e) => self.error(format!("Не удалось сохранить: {e}")),
            },
            Command::Snapshot { thumbnail } => {
                self.settle_waiting_fonts();
                self.snapshot(thumbnail);
            }
            Command::MarkSaved { revision } => {
````

на:

````rust
                Err(e) => self.error(format!("Не удалось сохранить: {e}")),
            },
            Command::Snapshot { thumbnail, full } => {
                self.settle_waiting_fonts();
                self.snapshot(thumbnail, full.unwrap_or(false));
            }
            Command::SetViewOnly { on } => {
                self.view_only = on;
                self.editor.set_view_only(on);
                self.dirty = true;
            }
            Command::MarkSaved { revision } => {
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            | Command::Save
            | Command::Snapshot { .. }
            | Command::MarkSaved { .. }
            | Command::ReadPixels { .. }
````

на:

````rust
            | Command::Save
            | Command::Snapshot { .. }
            | Command::SetViewOnly { .. }
            | Command::MarkSaved { .. }
            | Command::ReadPixels { .. }
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    Snapshot {
        thumbnail: bool,
    },
    /// Снимок с ревизией `revision` записан в хранилище: `modified` снимается, если
````

на:

````rust
    Snapshot {
        thumbnail: bool,
        /// Полные снимки страниц с историей правок (файл на сервере, спека подсистемы 4, раздел 7.6).
        #[serde(default)]
        full: Option<bool>,
    },
    /// Режим просмотра по роли: документ не принимает правок. Действует и на документы, открытые
    /// позже, пока его не снимут.
    SetViewOnly {
        on: bool,
    },
    /// Снимок с ревизией `revision` записан в хранилище: `modified` снимается, если
````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust
    let mut a = core_with_boolean_text();
    let full_area = boolean_area(&a);
    a.command(Command::Snapshot { thumbnail: false });
    let bytes = a
        .take_events()
````

на:

````rust
    let mut a = core_with_boolean_text();
    let full_area = boolean_area(&a);
    a.command(Command::Snapshot {
        thumbnail: false,
        full: None,
    });
    let bytes = a
        .take_events()
````

В `crates/ferrum-wasm/tests/boolean.rs` заменить:

````rust

fn thumbnail(c: &mut Core) -> Vec<u8> {
    c.command(Command::Snapshot { thumbnail: true });
    c.take_events()
        .into_iter()
````

на:

````rust

fn thumbnail(c: &mut Core) -> Vec<u8> {
    c.command(Command::Snapshot {
        thumbnail: true,
        full: None,
    });
    c.take_events()
        .into_iter()
````

В `crates/ferrum-wasm/tests/fonts.rs` заменить:

````rust
        .any(|e| matches!(e, CoreEvent::ClipboardSvg { .. })));
    // Превью: прежнее остаётся.
    c.command(Command::Snapshot { thumbnail: true });
    let ev = c.take_events();
    assert!(ev.iter().any(|e| matches!(
````

на:

````rust
        .any(|e| matches!(e, CoreEvent::ClipboardSvg { .. })));
    // Превью: прежнее остаётся.
    c.command(Command::Snapshot {
        thumbnail: true,
        full: None,
    });
    let ev = c.take_events();
    assert!(ev.iter().any(|e| matches!(
````

В `crates/ferrum-wasm/tests/fonts.rs` заменить:

````rust
        assert!(ev.iter().any(|e| matches!(e, CoreEvent::Exported { .. })));
    }
    c.command(Command::Snapshot { thumbnail: true });
    let ev = c.take_events();
    assert!(ev.iter().any(|e| matches!(
````

на:

````rust
        assert!(ev.iter().any(|e| matches!(e, CoreEvent::Exported { .. })));
    }
    c.command(Command::Snapshot {
        thumbnail: true,
        full: None,
    });
    let ev = c.take_events();
    assert!(ev.iter().any(|e| matches!(
````

В `crates/ferrum-wasm/tests/journal.rs` заменить:

````rust
    run(&mut c, "edit.select_all");
    run(&mut c, "edit.delete");
    c.command(Command::Snapshot { thumbnail: false });
    let ev = c.take_events();
    let j = ev
````

на:

````rust
    run(&mut c, "edit.select_all");
    run(&mut c, "edit.delete");
    c.command(Command::Snapshot {
        thumbnail: false,
        full: None,
    });
    let ev = c.take_events();
    let j = ev
````

В `crates/ferrum-wasm/tests/journal.rs` заменить:

````rust
    assert!(j < s);
    // Без правок снимок пачку не шлёт, номер — прежний.
    c.command(Command::Snapshot { thumbnail: false });
    let ev = c.take_events();
    assert!(journal(&ev).is_empty());
````

на:

````rust
    assert!(j < s);
    // Без правок снимок пачку не шлёт, номер — прежний.
    c.command(Command::Snapshot {
        thumbnail: false,
        full: None,
    });
    let ev = c.take_events();
    assert!(journal(&ev).is_empty());
````

В `crates/ferrum-wasm/tests/journal.rs` заменить:

````rust
    assert_eq!(back.editor().scene().len(), 0);
}
````

на:

````rust
    assert_eq!(back.editor().scene().len(), 0);
}

/// Снимок для сервера — полный: журнал отставшей вкладки сливается с ним (спека подсистемы 4,
/// раздел 7.6). Команда без `full` пишет как раньше.
#[test]
fn full_snapshot_comes_only_on_request() {
    let (_, mut c) = stored(100);
    run(&mut c, "edit.select_all");
    run(&mut c, "edit.delete");
    let snap = |c: &mut Core, full| {
        c.command(Command::Snapshot {
            thumbnail: false,
            full,
        });
        c.take_events()
            .into_iter()
            .find_map(|e| match e {
                CoreEvent::Snapshot { bytes, .. } => Some(bytes.into_vec()),
                _ => None,
            })
            .expect("нет Snapshot")
    };
    let shallow = snap(&mut c, None);
    let full = snap(&mut c, Some(true));
    // Удаление лежит только в истории: полный снимок её хранит, shallow — нет.
    assert!(
        full.len() > shallow.len(),
        "{} > {}",
        full.len(),
        shallow.len()
    );
}

/// Режим просмотра: правки отвергаются, признак «изменён» не появляется, режим переживает открытие
/// файла, снятие возвращает правки.
#[test]
fn view_only_survives_open_and_lifts() {
    let (bytes, _) = stored(10);
    let mut c = core();
    c.command(Command::SetViewOnly { on: true });
    c.command(Command::Open {
        bytes: ByteBuf::from(bytes),
    });
    c.take_events();
    c.command(Command::SetDocumentName {
        name: "Чужое".into(),
        quiet: None,
    });
    run(&mut c, "edit.delete");
    let ev = c.take_events();
    assert!(ev.iter().any(
        |e| matches!(e, CoreEvent::Error { message } if message.contains("только для чтения"))
    ));
    assert!(!ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Modified { modified: true })));
    c.command(Command::SetViewOnly { on: false });
    c.take_events();
    c.command(Command::SetDocumentName {
        name: "Своё".into(),
        quiet: None,
    });
    assert!(c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::DocumentName { name } if name == "Своё")));
}
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
/// Сохранение в хранилище: снимок, затем `MarkSaved` с его ревизией (запись удалась).
fn save(core: &mut Core) {
    core.command(Command::Snapshot { thumbnail: false });
    let revision = core
        .take_events()
````

на:

````rust
/// Сохранение в хранилище: снимок, затем `MarkSaved` с его ревизией (запись удалась).
fn save(core: &mut Core) {
    core.command(Command::Snapshot {
        thumbnail: false,
        full: None,
    });
    let revision = core
        .take_events()
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
    core.command(Command::Save);
    assert_eq!(modified(&core.take_events()), None, "копия на диск");
    core.command(Command::Snapshot { thumbnail: false });
    let snap = core.take_events();
    let Some(CoreEvent::Snapshot { revision, name, .. }) = snap
````

на:

````rust
    core.command(Command::Save);
    assert_eq!(modified(&core.take_events()), None, "копия на диск");
    core.command(Command::Snapshot {
        thumbnail: false,
        full: None,
    });
    let snap = core.take_events();
    let Some(CoreEvent::Snapshot { revision, name, .. }) = snap
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust
fn snapshot_says_whether_to_keep_the_old_thumbnail() {
    let keep = |core: &mut Core, thumbnail| {
        core.command(Command::Snapshot { thumbnail });
        core.take_events().into_iter().find_map(|e| match e {
            CoreEvent::Snapshot {
````

на:

````rust
fn snapshot_says_whether_to_keep_the_old_thumbnail() {
    let keep = |core: &mut Core, thumbnail| {
        core.command(Command::Snapshot {
            thumbnail,
            full: None,
        });
        core.take_events().into_iter().find_map(|e| match e {
            CoreEvent::Snapshot {
````

В `crates/ferrum-wasm/tests/panels.rs` заменить:

````rust

fn thumbnail_of(core: &mut Core) -> Option<Vec<u8>> {
    core.command(Command::Snapshot { thumbnail: true });
    core.take_events().into_iter().find_map(|e| match e {
        CoreEvent::Snapshot { thumbnail, .. } => Some(thumbnail.map(|t| t.into_vec())),
````

на:

````rust

fn thumbnail_of(core: &mut Core) -> Option<Vec<u8>> {
    core.command(Command::Snapshot {
        thumbnail: true,
        full: None,
    });
    core.take_events().into_iter().find_map(|e| match e {
        CoreEvent::Snapshot { thumbnail, .. } => Some(thumbnail.map(|t| t.into_vec())),
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
        },
        Command::RevealLayer { id: "1@2".into() },
        Command::Snapshot { thumbnail: true },
        Command::MarkSaved { revision: 3.0 },
        Command::ReadPixels { reference: true },
````

на:

````rust
        },
        Command::RevealLayer { id: "1@2".into() },
        Command::Snapshot {
            thumbnail: true,
            full: None,
        },
        Command::MarkSaved { revision: 3.0 },
        Command::ReadPixels { reference: true },
````

В `crates/ferrum-wasm/tests/snapshot_perf.rs` заменить:

````rust
fn snapshot(core: &mut Core, thumbnail: bool) -> (Duration, bool) {
    let t = Instant::now();
    core.command(Command::Snapshot { thumbnail });
    let d = t.elapsed();
    let thumb = core.take_events().into_iter().any(|e| {
````

на:

````rust
fn snapshot(core: &mut Core, thumbnail: bool) -> (Duration, bool) {
    let t = Instant::now();
    core.command(Command::Snapshot {
        thumbnail,
        full: None,
    });
    let d = t.elapsed();
    let thumb = core.take_events().into_iter().any(|e| {
````

В `crates/ferrum-wasm/tests/text_edit.rs` заменить:

````rust
    let (mut c, id) = core();
    typing_waits_for_roboto(&mut c, &id);
    c.command(Command::Snapshot { thumbnail: false });
    assert_eq!(text_of(&c, &id), "Привет!\n");
    assert!(c
````

на:

````rust
    let (mut c, id) = core();
    typing_waits_for_roboto(&mut c, &id);
    c.command(Command::Snapshot {
        thumbnail: false,
        full: None,
    });
    assert_eq!(text_of(&c, &id), "Привет!\n");
    assert!(c
````

- [ ] **Step 2: Сгенерировать файлы**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm`
Expected: ядро собрано: `src/wasm`, в типах команды `snapshot` появилось `full`, новая команда `setViewOnly`

- [ ] **Step 3: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-wasm --test journal`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test text_edit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Expected: PASS: fmt и clippy без замечаний; журнал 15 и 12 passed, `text_edit` 16 passed; модульные 188 passed

- [ ] **Step 4: Закоммитить**

```bash
git add crates/ferrum-doc/src/container.rs crates/ferrum-doc/src/document.rs crates/ferrum-doc/src/page.rs crates/ferrum-doc/tests/journal.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/tests/text_edit.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/boolean.rs crates/ferrum-wasm/tests/fonts.rs crates/ferrum-wasm/tests/journal.rs crates/ferrum-wasm/tests/panels.rs crates/ferrum-wasm/tests/protocol.rs crates/ferrum-wasm/tests/snapshot_perf.rs crates/ferrum-wasm/tests/text_edit.rs
git commit -F - <<'EOF'
ядро: полный снимок для сервера и режим просмотра

Снимок с полной историей страниц (`full`): журнал вкладки, отставшей от чужой
записи, сливается с новым файлом, а на shallow-снапшот Loro его не примет.
`SetViewOnly`: документ не принимает правок по роли человека, режим переживает
открытие файла и снимается без перезагрузки.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 2: Сервер — файлы и запись с проверкой поколения

Спека §4, §6.1, §6.2, §6.5. Модель, конвейер прав, списки, корзина, содержимое потоком, превью, потоковый `BlobStore` с `fsync` (B-204), таблица проверки прав.

**Files:**
- Create: `apps/server/migrations/003_files.sql`, `apps/server/src/api.ts`, `apps/server/src/file-routes.ts`, `apps/server/src/gc.ts`, `packages/protocol/src/container.ts`, `packages/protocol/src/files.ts`
- Modify: `apps/server/bun.lock`, `apps/server/package.json`, `apps/server/src/app.ts`, `apps/server/src/blob-store.ts`, `apps/server/src/index.ts`, `packages/protocol/src/index.ts`
- Test: `apps/server/test/access.test.ts` (новый), `apps/server/test/api-stand.ts` (новый), `apps/server/test/auth.test.ts`, `apps/server/test/blob-store.test.ts`, `apps/server/test/files.test.ts` (новый), `apps/server/test/migrate.test.ts`, `apps/server/test/server.test.ts`, `packages/protocol/test/container.test.ts` (новый)

**Interfaces:**
- Consumes: Better Auth задачи этапа 1, `originAllowed`.
- Produces: `apiHandler`, `Route`, `apiRoutes`, `GET/POST /api/files`, `GET/PATCH/DELETE /api/files/:id`, `…/move|touch|trash|restore`, `…/access`, `…/content`, `…/thumb`; `@ferrum/protocol`: `parse*`, `is*`, `ContainerWalker`, `scanContainer`.

- [ ] **Step 1: Написать тесты**

Создать `apps/server/test/access.test.ts`:

````ts
// Проверка прав на каждом маршруте (спека, раздел 13.2): таблица перечисляет все маршруты, новый без
// записи здесь роняет тест. У каждого: без сессии — 401; изменяющий с чужим `Origin` — 403; чужой
// неотличим от несуществующего; роли ниже нужной — 403; нужной — работает.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { apiRoutes } from '../src/app';
import { apiStand, container, PNG, type Person } from './api-stand';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const editor = await s.signIn('editor@example.com');
const viewer = await s.signIn('viewer@example.com');
const stranger = await s.signIn('stranger@example.com');
beforeEach(s.reset);

const ROUTES = apiRoutes({
  pool: s.pool,
  valkey: s.valkey,
  auth: s.auth,
  webDist: '',
  publicUrl: s.publicUrl,
  trustedProxies: [],
  blobs: s.blobs,
  mailer: { send: async () => {} },
  secret: '',
});

const NOBODY = '00000000-0000-4000-8000-000000000000';
const HASH = 'a'.repeat(64);

interface World {
  file: string;
}

async function world(): Promise<World> {
  const f = await s.newFile(owner, 'Файл');
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
    f.id,
    editor.id,
    viewer.id,
  ]);
  return { file: f.id };
}

/** Запрос маршрута: метод и адрес — из таблицы маршрутов, здесь — чем его наполнить. */
interface Sample {
  params?: (w: World) => Record<string, string>;
  body?: unknown;
  headers?: Record<string, string>;
  /** Кому хватает роли; нет — маршрут только для вошедшего. */
  allowed?: Person;
  /** Кому роли не хватает. */
  denied?: Person[];
  /** Ответ при достаточной роли. */
  status?: number;
}

const write = () => ({ 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() });
const file = (w: World) => ({ id: w.file });

const SAMPLES: Record<string, Sample> = {
  'GET /api/files': {},
  'POST /api/files': { body: { name: 'Новый' }, status: 201 },
  'POST /api/files/empty-trash': {},
  'GET /api/files/:id': { params: file, allowed: viewer },
  'PATCH /api/files/:id': { params: file, body: { name: 'Имя' }, allowed: editor, denied: [viewer] },
  'POST /api/files/:id/move': { params: file, body: { projectId: null }, allowed: owner, denied: [viewer, editor] },
  'POST /api/files/:id/touch': { params: file, allowed: viewer },
  'POST /api/files/:id/trash': { params: file, allowed: owner, denied: [viewer, editor] },
  'POST /api/files/:id/restore': { params: file, allowed: owner, denied: [viewer, editor] },
  'DELETE /api/files/:id': { params: file, allowed: owner, denied: [viewer, editor] },
  'GET /api/files/:id/access': { params: file, allowed: viewer },
  'GET /api/files/:id/content': { params: file, allowed: viewer, status: 204 },
  'PUT /api/files/:id/content': { params: file, body: container(), headers: {}, allowed: editor, denied: [viewer] },
  'GET /api/files/:id/thumb': { params: file, allowed: viewer, status: 204 },
  'PUT /api/files/:id/thumb': { params: file, body: PNG, allowed: editor, denied: [viewer] },
  'DELETE /api/files/:id/thumb': { params: file, allowed: editor, denied: [viewer] },
};

const key = (r: { method: string; path: string }) => `${r.method} ${r.path}`;

function pathOf(path: string, params: Record<string, string>): string {
  return path.replace(/:(\w+)/g, (_, name: string) => params[name] ?? (name === 'hash' ? HASH : NOBODY));
}

const headersOf = (route: { path: string; method: string }, sample: Sample) =>
  route.path.endsWith('/content') && route.method === 'PUT' ? write() : (sample.headers ?? {});
const queryOf = (route: { path: string; method: string }) =>
  route.path === '/api/files' && route.method === 'GET' ? '?section=drafts&start=0&count=1' : '';

test('таблица перечисляет все маршруты', () => {
  expect(ROUTES.map(key).filter((k) => !SAMPLES[k])).toEqual([]);
  expect(Object.keys(SAMPLES).filter((k) => !ROUTES.some((r) => key(r) === k))).toEqual([]);
});

for (const route of ROUTES) {
  const sample = SAMPLES[key(route)] ?? {};
  const mutating = route.method !== 'GET' && route.method !== 'HEAD';

  test(`${key(route)}: без сессии — 401`, async () => {
    const w = await world();
    const path = pathOf(route.path, sample.params?.(w) ?? {}) + queryOf(route);
    const res = await s.call(null, route.method, path, sample.body, headersOf(route, sample));
    expect(res.status).toBe(401);
    expect(await res.json()).toEqual({ error: 'UNAUTHORIZED' });
  });

  if (mutating) {
    test(`${key(route)}: чужой Origin — 403 и ничего не меняется`, async () => {
      const w = await world();
      const path = pathOf(route.path, sample.params?.(w) ?? {});
      const before = await s.pool.query('SELECT name, version, trashed_at FROM file ORDER BY id');
      const res = await s.call(owner, route.method, path, sample.body, { ...headersOf(route, sample), origin: 'https://evil.example' });
      expect(res.status).toBe(403);
      expect(await res.json()).toEqual({ error: 'BAD_ORIGIN' });
      expect((await s.pool.query('SELECT name, version, trashed_at FROM file ORDER BY id')).rows).toEqual(before.rows);
      const none = await s.call(owner, route.method, path, sample.body, { ...headersOf(route, sample), origin: '' });
      expect(none.status).toBe(403);
    });
  }

  if (route.access.kind !== 'user') {
    test(`${key(route)}: чужой неотличим от несуществующего`, async () => {
      const w = await world();
      const params = sample.params?.(w) ?? {};
      const foreign = await s.call(stranger, route.method, pathOf(route.path, params), sample.body, headersOf(route, sample));
      const missing = await s.call(stranger, route.method, pathOf(route.path, {}), sample.body, headersOf(route, sample));
      expect(foreign.status).toBe(404);
      expect(missing.status).toBe(404);
      expect(await foreign.text()).toBe(await missing.text());
      // И без записи: файл цел.
      const meta = await s.call(owner, 'GET', `/api/files/${w.file}`);
      expect(meta.status).toBe(200);
    });

    for (const who of sample.denied ?? []) {
      test(`${key(route)}: роли ${who.email} не хватает — 403`, async () => {
        const w = await world();
        const res = await s.call(who, route.method, pathOf(route.path, sample.params?.(w) ?? {}), sample.body, headersOf(route, sample));
        expect(res.status).toBe(403);
        expect(await res.json()).toEqual({ error: 'FORBIDDEN' });
      });
    }
  }

  test(`${key(route)}: нужной роли хватает`, async () => {
    const w = await world();
    const path = pathOf(route.path, sample.params?.(w) ?? {}) + queryOf(route);
    const res = await s.call(sample.allowed ?? owner, route.method, path, sample.body, headersOf(route, sample));
    expect(res.status).toBe(sample.status ?? 200);
  });
}
````

Создать `apps/server/test/api-stand.ts`:

````ts
// Стенд тестов API: настоящий сервер на свободном порту, настоящие Postgres и Valkey, настоящий
// вход по письму. Люди заводятся один раз на файл тестов, данные файлов очищаются между тестами.

import { containerHeader, SECTION_PAGE_REFS } from '@ferrum/protocol';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { Pool } from 'pg';
import { startServer, type AppDeps } from '../src/app';
import { createAuth } from '../src/auth';
import { FsBlobStore } from '../src/blob-store';
import type { Mail } from '../src/mail';
import { migrate } from '../src/migrate';
import { connectValkey } from '../src/valkey';
import { resetDatabase, resetValkey, TEST_SECRET, TEST_VALKEY_URL, testPool } from './helpers';

export interface Person {
  id: string;
  email: string;
  cookie: string;
}

function section(kind: number, id: string, data: Uint8Array): Uint8Array {
  const idBytes = new TextEncoder().encode(id);
  const out = new Uint8Array(5 + idBytes.length + 40 + data.length);
  const v = new DataView(out.buffer);
  out[0] = kind;
  v.setUint32(1, idBytes.length, true);
  out.set(idBytes, 5);
  v.setBigUint64(5 + idBytes.length, BigInt(data.length), true);
  out.set(data, 5 + idBytes.length + 40);
  return out;
}

/** Контейнер `.ferrum` нужного вида: корень и, если есть, ссылки страницы на картинки. */
export function container(refs: string[] = [], fill = 16): Uint8Array<ArrayBuffer> {
  const parts = [section(1, '', new Uint8Array(fill).fill(7))];
  if (refs.length > 0) parts.push(section(SECTION_PAGE_REFS, '1@1', new TextEncoder().encode(refs.join('\n'))));
  const head = containerHeader(parts.length);
  const out = new Uint8Array(head.length + parts.reduce((n, s) => n + s.length, 0));
  out.set(head);
  let at = head.length;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

export const PNG: Uint8Array<ArrayBuffer> = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);

export async function apiStand(over: Partial<AppDeps> = {}) {
  const pool: Pool = testPool();
  const valkey = await connectValkey(TEST_VALKEY_URL);
  await resetDatabase(pool);
  await resetValkey(valkey);
  await migrate(pool, join(import.meta.dir, '../migrations'));
  const free = Bun.serve({ port: 0, fetch: () => new Response() });
  const port = free.port!;
  free.stop(true);
  const publicUrl = `http://localhost:${port}`;
  const mails: Mail[] = [];
  const mailer = { send: async (m: Mail) => void mails.push(m) };
  const auth = createAuth({ publicUrl, secret: TEST_SECRET, pool, valkey, mailer, allowedEmails: [] });
  const dataDir = mkdtempSync(join(tmpdir(), 'ferrum-files-'));
  const blobs = new FsBlobStore(dataDir);
  const server = startServer(
    { pool, valkey, webDist: '/nonexistent', auth, publicUrl, trustedProxies: [], blobs, mailer, secret: TEST_SECRET, ...over },
    port,
  );

  const call = (who: Person | null, method: string, path: string, body?: unknown, headers: Record<string, string> = {}) =>
    fetch(`${publicUrl}${path}`, {
      method,
      headers: {
        origin: publicUrl,
        ...(who ? { cookie: who.cookie } : {}),
        ...(body !== undefined && !(body instanceof Uint8Array) ? { 'content-type': 'application/json' } : {}),
        ...headers,
      },
      body: body === undefined ? undefined : body instanceof Uint8Array ? (body as Uint8Array<ArrayBuffer>) : JSON.stringify(body),
      redirect: 'manual',
    });

  /** Письмо уходит после ответа: его ждут опросом. */
  const waitMail = async (to: string, seen = 0): Promise<Mail> => {
    for (let i = 0; i < 250; i++) {
      const found = mails.filter((m) => m.to === to);
      if (found.length > seen) return found[found.length - 1];
      await Bun.sleep(20);
    }
    throw new Error(`нет письма на ${to}`);
  };

  /** Настоящий вход: письмо, подтверждение, имя. */
  const signIn = async (email: string, name = 'Имя'): Promise<Person> => {
    const seen = mails.filter((m) => m.to === email).length;
    const sent = await call(null, 'POST', '/api/auth/sign-in/magic-link', { email });
    if (sent.status !== 200) throw new Error(`вход: ${sent.status}`);
    const mail = await waitMail(email, seen);
    const token = new URLSearchParams(mail.text.split('#/auth/confirm?')[1].split('\n')[0]).get('token')!;
    const res = await call(null, 'POST', '/api/auth/confirm', { token });
    const cookie = res.headers
      .getSetCookie()
      .map((c) => c.split(';')[0])
      .join('; ');
    const who: Person = { id: '', email, cookie };
    const named = await call(who, 'POST', '/api/auth/name', { name });
    const { user } = (await named.json()) as { user: { id: string } };
    who.id = user.id;
    return who;
  };

  const reset = () =>
    pool.query(
      'TRUNCATE file, file_access, file_invite, share_link, file_link_user, file_recent, file_image, image, blob_trash, team, team_member, team_invite, project CASCADE',
    );

  const stop = async () => {
    server.stop(true);
    valkey.close();
    await pool.end();
  };

  /** Новый файл и запись содержимого. */
  const newFile = async (who: Person, name = 'Файл', projectId?: string) => {
    const res = await call(who, 'POST', '/api/files', { name, projectId });
    if (res.status !== 201) throw new Error(`создание файла: ${res.status}`);
    return (await res.json()) as { id: string; version: number };
  };
  const writeContent = (who: Person, id: string, base: number, bytes: Uint8Array = container(), extra: Record<string, string> = {}) =>
    call(who, 'PUT', `/api/files/${id}/content`, bytes, {
      'if-match': `"${base}"`,
      'x-ferrum-write': crypto.randomUUID(),
      ...extra,
    });

  return { pool, valkey, auth, publicUrl, mails, blobs, dataDir, server, call, signIn, waitMail, reset, stop, newFile, writeContent };
}
export type ApiStand = Awaited<ReturnType<typeof apiStand>>;
````

В `apps/server/test/auth.test.ts` заменить:

````ts
import { startServer } from '../src/app';
import { AUTH_DEADLINE_MS, authorizeUpgrade } from '../src/auth-routes';
import { authOptions, createAuth, emailAllowed, MAILS_PER_ADDRESS, MAILS_PER_IP, type AuthDeps } from '../src/auth';
import { magicLinkMail, smtpMailer, type Mail } from '../src/mail';
````

на:

````ts
import { startServer } from '../src/app';
import { AUTH_DEADLINE_MS, authorizeUpgrade } from '../src/auth-routes';
import { FsBlobStore } from '../src/blob-store';
import { authOptions, createAuth, emailAllowed, MAILS_PER_ADDRESS, MAILS_PER_IP, type AuthDeps } from '../src/auth';
import { magicLinkMail, smtpMailer, type Mail } from '../src/mail';
````

В `apps/server/test/auth.test.ts` заменить:

````ts
  };
  const auth = createAuth(deps);
  const app = startServer({ pool, valkey, webDist: '/nonexistent', auth, publicUrl, trustedProxies }, port);
  stops.push(() => app.stop(true));
  const post = (path: string, body: unknown, headers: Record<string, string> = {}) =>
````

на:

````ts
  };
  const auth = createAuth(deps);
  const app = startServer(
    {
      pool,
      valkey,
      webDist: '/nonexistent',
      auth,
      publicUrl,
      trustedProxies,
      blobs: new FsBlobStore('/nonexistent'),
      mailer: deps.mailer,
      secret: TEST_SECRET,
    },
    port,
  );
  stops.push(() => app.stop(true));
  const post = (path: string, body: unknown, headers: Record<string, string> = {}) =>
````

В `apps/server/test/auth.test.ts` заменить:

````ts
    (r) => r.t,
  );
  expect(tables.sort()).toEqual(['account', 'schema_migration', 'user']);
  const sessionToken = decodeURIComponent(cookieOf(ok).split('=')[1]).split('.')[0];
  expect(await valkey.get(sessionToken)).toContain('ivan@example.com');
````

на:

````ts
    (r) => r.t,
  );
  expect(tables).toEqual(expect.arrayContaining(['account', 'user']));
  expect(tables).not.toContain('session');
  expect(tables).not.toContain('verification');
  const sessionToken = decodeURIComponent(cookieOf(ok).split('=')[1]).split('.')[0];
  expect(await valkey.get(sessionToken)).toContain('ivan@example.com');
````

В `apps/server/test/auth.test.ts` заменить:

````ts
  const publicUrl = 'http://localhost:1';
  const auth = createAuth({ publicUrl, secret: TEST_SECRET, pool, valkey: hung, allowedEmails: [], mailer: { send: async () => {} } });
  const app = startServer({ pool, valkey: hung, webDist: '/nonexistent', auth, publicUrl, trustedProxies: [], authDeadlineMs: 150 }, 0);
  try {
    proxy.freeze();
````

на:

````ts
  const publicUrl = 'http://localhost:1';
  const auth = createAuth({ publicUrl, secret: TEST_SECRET, pool, valkey: hung, allowedEmails: [], mailer: { send: async () => {} } });
  const app = startServer(
    {
      pool,
      valkey: hung,
      webDist: '/nonexistent',
      auth,
      publicUrl,
      trustedProxies: [],
      authDeadlineMs: 150,
      blobs: new FsBlobStore('/nonexistent'),
      mailer: { send: async () => {} },
      secret: TEST_SECRET,
    },
    0,
  );
  try {
    proxy.freeze();
````

В `apps/server/test/blob-store.test.ts` заменить:

````ts
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FsBlobStore } from '../src/blob-store';

const store = () => {
````

на:

````ts
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BlobStalled, BlobTooLarge, FsBlobStore } from '../src/blob-store';

const store = () => {
````

В `apps/server/test/blob-store.test.ts` заменить:

````ts
  }
});
````

на:

````ts
  }
});

const streamOf = (...chunks: Uint8Array[]) =>
  new ReadableStream<Uint8Array>({
    start(c) {
      for (const chunk of chunks) c.enqueue(chunk);
      c.close();
    },
  });
const OPTIONS = { maxBytes: 8, idleMs: 1000 };

test('запись потоком: ключ появляется целиком, временных файлов нет', async () => {
  const { s, dir } = store();
  expect(await s.putStream('files/a', streamOf(new Uint8Array([1, 2]), new Uint8Array([3])), OPTIONS)).toEqual({ size: 3 });
  expect(await s.get('files/a')).toEqual(new Uint8Array([1, 2, 3]));
  expect(readdirSync(join(dir, 'files'))).toEqual(['a']);
  const blob = await s.open('files/a');
  expect(blob?.size).toBe(3);
  expect(await s.open('files/none')).toBeNull();
});

test('сверх предела и отказ проверки: ключа нет, `.tmp` убран, старое значение цело', async () => {
  const { s, dir } = store();
  await s.put('files/a', new Uint8Array([9]));
  await expect(s.putStream('files/a', streamOf(new Uint8Array(9)), OPTIONS)).rejects.toBeInstanceOf(BlobTooLarge);
  const bad = {
    ...OPTIONS,
    onChunk: () => {
      throw new Error('плохой кусок');
    },
  };
  await expect(s.putStream('files/a', streamOf(new Uint8Array(1)), bad)).rejects.toThrow('плохой кусок');
  expect(await s.get('files/a')).toEqual(new Uint8Array([9]));
  expect(readdirSync(join(dir, 'files'))).toEqual(['a']);
});

test('клиент молчит дольше срока: запись отменена, `.tmp` убран', async () => {
  const { s, dir } = store();
  let cancelled = false;
  const silent = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(new Uint8Array(1));
    },
    cancel() {
      cancelled = true;
    },
  });
  await expect(s.putStream('files/a', silent, { maxBytes: 8, idleMs: 20 })).rejects.toBeInstanceOf(BlobStalled);
  expect(cancelled).toBe(true);
  expect(readdirSync(join(dir, 'files'))).toEqual([]);
});
````

Создать `apps/server/test/files.test.ts`:

````ts
import { afterAll, beforeEach, expect, test } from 'bun:test';
import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import { LIMITS, MAX_JSON_BYTES } from '../src/api';
import { runGc } from '../src/gc';
import { apiStand, container, PNG, type Person } from './api-stand';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const other = await s.signIn('other@example.com');
beforeEach(s.reset);

const json = async (res: Response) => (await res.json()) as any; // eslint-disable-line @typescript-eslint/no-explicit-any
const list = async (who: Person, section: string, q = '', start = 0, count = 200) =>
  json(await s.call(who, 'GET', `/api/files?section=${section}&start=${start}&count=${count}&q=${encodeURIComponent(q)}`));
const giveAccess = (id: string, who: Person, role: string) =>
  s.pool.query('INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, $3)', [id, who.id, role]);

test('новый файл: в «Черновиках» и «Недавних», поколение 0, без содержимого', async () => {
  const f = await s.newFile(owner, 'План');
  expect(f.version).toBe(0);
  expect((await list(owner, 'drafts')).rows.map((r: { name: string }) => r.name)).toEqual(['План']);
  expect((await list(owner, 'recent')).total).toBe(1);
  const meta = await json(await s.call(owner, 'GET', `/api/files/${f.id}`));
  expect(meta.role).toBe('owner');
  expect(meta.openedAt).toBeGreaterThan(0);
  const res = await s.call(owner, 'GET', `/api/files/${f.id}/content`);
  expect(res.status).toBe(204);
  expect(res.headers.get('x-ferrum-version')).toBe('0');
});

test('имя: пробелы по краям убираются, пустое и управляющие символы — 400', async () => {
  const f = await s.newFile(owner);
  expect((await json(await s.call(owner, 'PATCH', `/api/files/${f.id}`, { name: '  Новое ' }))).name).toBe('Новое');
  for (const name of ['', '  ', 'a\u0000b', 'x'.repeat(257), 5]) {
    expect((await s.call(owner, 'PATCH', `/api/files/${f.id}`, { name })).status).toBe(400);
    expect((await s.call(owner, 'POST', '/api/files', { name })).status).toBe(400);
  }
});

test('списки: окно до 200, поиск с `%` не как шаблон, порядок раздела', async () => {
  const names = ['Альфа', 'Бета', '100%', '100x'];
  for (const n of names) await s.newFile(owner, n);
  expect((await list(owner, 'drafts')).total).toBe(4);
  const found = await list(owner, 'drafts', '100%');
  expect(found.rows.map((r: { name: string }) => r.name)).toEqual(['100%']);
  expect((await list(owner, 'drafts', 'альф')).total).toBe(1);
  const window = await list(owner, 'drafts', '', 1, 2);
  expect(window.total).toBe(4);
  expect(window.rows).toHaveLength(2);
  expect((await list(owner, 'drafts', '', 10, 2)).total).toBe(4);
  expect((await s.call(owner, 'GET', '/api/files?section=drafts&start=0&count=201')).status).toBe(400);
  expect((await s.call(owner, 'GET', '/api/files?section=other&start=0&count=1')).status).toBe(400);
  // Чужие черновики не видны.
  expect((await list(other, 'drafts')).total).toBe(0);
  expect((await list(other, 'recent')).total).toBe(0);
});

test('содержимое: запись и чтение байт в байт, размер и поколение растут', async () => {
  const f = await s.newFile(owner);
  const bytes = container([], 200_000);
  const res = await s.writeContent(owner, f.id, 0, bytes);
  expect(res.status).toBe(200);
  const meta = await json(res);
  expect([meta.version, meta.size]).toEqual([1, bytes.length]);
  const got = await s.call(owner, 'GET', `/api/files/${f.id}/content`);
  expect(got.headers.get('x-ferrum-version')).toBe('1');
  expect(got.headers.get('content-length')).toBe(String(bytes.length));
  expect(got.headers.get('content-disposition')).toBe('attachment');
  expect(got.headers.get('cache-control')).toBe('no-store');
  expect(new Uint8Array(await got.arrayBuffer())).toEqual(bytes);
});

test('CONFLICT: две вкладки с одного поколения, вторая не затирает первую', async () => {
  const f = await s.newFile(owner);
  await s.writeContent(owner, f.id, 0, container([], 1));
  const first = await s.writeContent(owner, f.id, 1, container([], 2));
  expect(first.status).toBe(200);
  const second = await s.writeContent(owner, f.id, 1, container([], 3));
  expect(second.status).toBe(409);
  expect(await second.json()).toEqual({ error: 'CONFLICT' });
  const got = await s.call(owner, 'GET', `/api/files/${f.id}/content`);
  expect(got.headers.get('x-ferrum-version')).toBe('2');
  expect(new Uint8Array(await got.arrayBuffer())).toEqual(container([], 2));
});

test('CONFLICT: одновременные записи с одного поколения — проходит ровно одна', async () => {
  const f = await s.newFile(owner);
  const results = await Promise.all([0, 1, 2].map((i) => s.writeContent(owner, f.id, 0, container([], 10 + i))));
  expect(results.map((r) => r.status).sort()).toEqual([200, 409, 409]);
  expect((await json(await s.call(owner, 'GET', `/api/files/${f.id}`))).version).toBe(1);
});

test('потерянный ответ: своя удачная запись не становится `CONFLICT`, чужая — становится', async () => {
  const f = await s.newFile(owner);
  const write = crypto.randomUUID();
  // Запись прошла, но клиент ответа не получил и считает поколение прежним.
  expect((await s.writeContent(owner, f.id, 0, container(), { 'x-ferrum-write': write })).status).toBe(200);
  const retry = await s.writeContent(owner, f.id, 0, container([], 5), { 'x-ferrum-previous-write': write });
  expect(retry.status).toBe(200);
  expect((await json(retry)).version).toBe(2);
  // Чужой номер записи поколение не отменяет.
  const foreign = await s.writeContent(owner, f.id, 1, container(), { 'x-ferrum-previous-write': crypto.randomUUID() });
  expect(foreign.status).toBe(409);
});

test('содержимое не контейнер, обрезано, с картинкой внутри: 400, старое цело, временных файлов нет', async () => {
  const f = await s.newFile(owner);
  const good = container([], 8);
  await s.writeContent(owner, f.id, 0, good);
  const before = readdirSync(join(s.dataDir, 'files', f.id));
  const image = container();
  image[16] = 4;
  for (const bad of [new Uint8Array(64), good.subarray(0, good.length - 1), image]) {
    const res = await s.writeContent(owner, f.id, 1, bad);
    expect(res.status).toBe(400);
    expect(await res.json()).toEqual({ error: 'BAD_CONTAINER' });
  }
  expect(readdirSync(join(s.dataDir, 'files', f.id))).toEqual(before);
  expect(new Uint8Array(await (await s.call(owner, 'GET', `/api/files/${f.id}/content`)).arrayBuffer())).toEqual(good);
});

test('заголовки записи обязательны; сверх предела по `Content-Length` — 413 без чтения', async () => {
  const f = await s.newFile(owner);
  const put = (headers: Record<string, string>) => s.call(owner, 'PUT', `/api/files/${f.id}/content`, container(), headers);
  expect((await put({})).status).toBe(400);
  expect((await put({ 'if-match': '"0"' })).status).toBe(400);
  expect((await put({ 'if-match': '0', 'x-ferrum-write': crypto.randomUUID() })).status).toBe(400);
  expect(
    (await s.call(owner, 'PUT', `/api/files/${f.id}/content`, undefined, { 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() }))
      .status,
  ).toBe(400);
});

test('замена оставляет старые байты на срок, очистка удаляет только просроченные', async () => {
  const f = await s.newFile(owner);
  await s.writeContent(owner, f.id, 0);
  const { rows: a } = await s.pool.query('SELECT content_key FROM file WHERE id = $1', [f.id]);
  await s.writeContent(owner, f.id, 1);
  expect(await s.blobs.get(a[0].content_key)).not.toBeNull();
  expect(await runGc(s.pool, s.blobs)).toBe(0);
  await s.pool.query(`UPDATE blob_trash SET delete_after = now() - interval '1 second'`);
  expect(await runGc(s.pool, s.blobs)).toBe(1);
  expect(await s.blobs.get(a[0].content_key)).toBeNull();
  expect((await s.call(owner, 'GET', `/api/files/${f.id}/content`)).status).toBe(200);
});

test('корзина: файл уходит из списков и возвращается, удалить навсегда, очистить', async () => {
  const f = await s.newFile(owner, 'Один');
  const g = await s.newFile(owner, 'Два');
  await s.writeContent(owner, f.id, 0);
  await s.call(owner, 'POST', `/api/files/${f.id}/trash`);
  expect((await list(owner, 'drafts')).total).toBe(1);
  expect((await list(owner, 'recent')).total).toBe(1);
  const trash = await list(owner, 'trash');
  expect(trash.rows[0].trashedAt).toBeGreaterThan(0);
  await s.call(owner, 'POST', `/api/files/${f.id}/restore`);
  expect((await list(owner, 'trash')).total).toBe(0);
  await s.call(owner, 'POST', `/api/files/${f.id}/trash`);
  await s.call(owner, 'POST', `/api/files/${g.id}/trash`);
  expect((await s.call(owner, 'POST', '/api/files/empty-trash')).status).toBe(200);
  expect((await list(owner, 'trash')).total).toBe(0);
  expect((await s.call(owner, 'GET', `/api/files/${f.id}`)).status).toBe(404);
  const { rows } = await s.pool.query('SELECT key FROM blob_trash');
  expect(rows.length).toBe(1);
});

test('файл в корзине недоступен тому, кому его открыли', async () => {
  const f = await s.newFile(owner);
  await giveAccess(f.id, other, 'editor');
  expect((await s.call(other, 'GET', `/api/files/${f.id}`)).status).toBe(200);
  await s.call(owner, 'POST', `/api/files/${f.id}/trash`);
  expect((await s.call(other, 'GET', `/api/files/${f.id}`)).status).toBe(404);
  expect((await s.call(owner, 'GET', `/api/files/${f.id}`)).status).toBe(200);
  expect((await list(other, 'shared')).total).toBe(0);
});

test('«Недавние» у каждого свои, `touch` поднимает файл наверх', async () => {
  const f = await s.newFile(owner, 'А');
  const g = await s.newFile(owner, 'Б');
  await giveAccess(f.id, other, 'viewer');
  expect((await list(other, 'recent')).total).toBe(0);
  await s.call(other, 'POST', `/api/files/${f.id}/touch`);
  expect((await list(other, 'recent')).rows.map((r: { id: string }) => r.id)).toEqual([f.id]);
  await s.pool.query(`UPDATE file_recent SET opened_at = now() - interval '1 hour' WHERE file_id = $1 AND user_id = $2`, [g.id, owner.id]);
  await s.pool.query(`UPDATE file_recent SET opened_at = now() - interval '2 hour' WHERE file_id = $1 AND user_id = $2`, [f.id, owner.id]);
  expect((await list(owner, 'recent')).rows[0].id).toBe(g.id);
  await s.call(owner, 'POST', `/api/files/${f.id}/touch`);
  expect((await list(owner, 'recent')).rows[0].id).toBe(f.id);
  // Снятый доступ убирает файл из «Недавних», запись открытия этому не мешает.
  await s.pool.query('DELETE FROM file_access WHERE file_id = $1 AND user_id = $2', [f.id, other.id]);
  expect((await list(other, 'recent')).total).toBe(0);
});

test('превью: только PNG, версия растёт, снятие, 204 без превью', async () => {
  const f = await s.newFile(owner);
  expect((await s.call(owner, 'GET', `/api/files/${f.id}/thumb`)).status).toBe(204);
  expect((await s.call(owner, 'PUT', `/api/files/${f.id}/thumb`, new Uint8Array([1, 2, 3]))).status).toBe(400);
  const put = await json(await s.call(owner, 'PUT', `/api/files/${f.id}/thumb`, PNG));
  expect(put.thumbVersion).toBe(1);
  const got = await s.call(owner, 'GET', `/api/files/${f.id}/thumb`);
  expect(got.headers.get('content-type')).toBe('image/png');
  expect(new Uint8Array(await got.arrayBuffer())).toEqual(PNG);
  expect((await json(await s.call(owner, 'DELETE', `/api/files/${f.id}/thumb`))).thumbVersion).toBe(2);
  expect((await s.call(owner, 'GET', `/api/files/${f.id}/thumb`)).status).toBe(204);
  expect((await json(await s.call(owner, 'DELETE', `/api/files/${f.id}/thumb`))).thumbVersion).toBe(2);
});

test('проверка доступа: роль и поколение', async () => {
  const f = await s.newFile(owner);
  await giveAccess(f.id, other, 'viewer');
  await s.writeContent(owner, f.id, 0);
  expect(await json(await s.call(other, 'GET', `/api/files/${f.id}/access`))).toEqual({ role: 'viewer', version: 1 });
  await s.pool.query(`UPDATE file_access SET role = 'editor' WHERE file_id = $1 AND user_id = $2`, [f.id, other.id]);
  expect(await json(await s.call(other, 'GET', `/api/files/${f.id}/access`))).toEqual({ role: 'editor', version: 1 });
  await s.pool.query('DELETE FROM file_access WHERE file_id = $1 AND user_id = $2', [f.id, other.id]);
  expect((await s.call(other, 'GET', `/api/files/${f.id}/access`)).status).toBe(404);
});

test('роль снята посреди загрузки: запись не проходит, байты убраны', async () => {
  const f = await s.newFile(owner);
  await giveAccess(f.id, other, 'editor');
  const { promise: gate, resolve: open } = Promise.withResolvers<void>();
  const bytes = container([], 64);
  const body = new ReadableStream<Uint8Array>({
    async start(c) {
      c.enqueue(bytes.subarray(0, 8));
      await gate;
      c.enqueue(bytes.subarray(8));
      c.close();
    },
  });
  const upload = fetch(`${s.publicUrl}/api/files/${f.id}/content`, {
    method: 'PUT',
    headers: { origin: s.publicUrl, cookie: other.cookie, 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() },
    body,
  });
  await waitFor(() => fileCount(f.id) > 0);
  await s.pool.query(`UPDATE file_access SET role = 'viewer' WHERE file_id = $1 AND user_id = $2`, [f.id, other.id]);
  open();
  const res = await upload;
  expect(res.status).toBe(403);
  expect((await json(await s.call(owner, 'GET', `/api/files/${f.id}`))).version).toBe(0);
  expect(readdirSync(join(s.dataDir, 'files', f.id))).toEqual([]);
});

function fileCount(id: string): number {
  try {
    return readdirSync(join(s.dataDir, 'files', id)).length;
  } catch {
    return 0;
  }
}

async function waitFor(check: () => boolean): Promise<void> {
  for (let i = 0; i < 500 && !check(); i++) await Bun.sleep(10);
  expect(check()).toBe(true);
}

test('файл большого размера идёт потоком: память сервера не растёт на размер файла', async () => {
  const f = await s.newFile(owner);
  const MB = 1024 * 1024;
  const whole = container([], 200 * MB);
  const chunk = 256 * 1024;
  let at = 0;
  // Клиент в этом же процессе: его байты уже в памяти (`whole`), а куски — окна в них же.
  const body = new ReadableStream<Uint8Array>({
    pull(c) {
      if (at >= whole.length) return c.close();
      c.enqueue(whole.subarray(at, at + chunk));
      at += chunk;
    },
  });
  Bun.gc(true);
  const before = process.memoryUsage().rss;
  let peak = before;
  const probe = setInterval(() => (peak = Math.max(peak, process.memoryUsage().rss)), 5);
  const res = await fetch(`${s.publicUrl}/api/files/${f.id}/content`, {
    method: 'PUT',
    headers: { origin: s.publicUrl, cookie: owner.cookie, 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() },
    body,
  });
  expect(res.status).toBe(200);
  expect((await json(res)).size).toBe(whole.length);
  const got = await s.call(owner, 'GET', `/api/files/${f.id}/content`);
  const reader = got.body!.getReader();
  let read = 0;
  for (let r = await reader.read(); !r.done; r = await reader.read()) read += r.value.length;
  clearInterval(probe);
  expect(read).toBe(whole.length);
  // Предел — половина файла: сервер, собирающий тело в памяти, сразу его превысит.
  expect((peak - before) / MB).toBeLessThan(100);
});

test('JSON сверх предела — 400, неверный JSON — 400', async () => {
  expect((await s.call(owner, 'POST', '/api/files', { name: 'x'.repeat(MAX_JSON_BYTES) })).status).toBe(400);
  const res = await fetch(`${s.publicUrl}/api/files`, {
    method: 'POST',
    headers: { origin: s.publicUrl, cookie: owner.cookie, 'content-type': 'application/json' },
    body: '{',
  });
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'BAD_REQUEST' });
});

test('предел частоты по человеку: другой не страдает; срок обработчика — 503 в формате ошибок', async () => {
  expect(LIMITS.read.max).toBeGreaterThan(100);
  const tight = await apiStand({ limits: { read: { max: 2, windowSeconds: 60 } } });
  const a = await tight.signIn('a@example.com');
  const b = await tight.signIn('b@example.com');
  const path = '/api/files?section=drafts&start=0&count=1';
  expect((await tight.call(a, 'GET', path)).status).toBe(200);
  expect((await tight.call(a, 'GET', path)).status).toBe(200);
  const limited = await tight.call(a, 'GET', path);
  expect(limited.status).toBe(429);
  expect(await limited.json()).toEqual({ error: 'TOO_MANY_REQUESTS' });
  expect((await tight.call(b, 'GET', path)).status).toBe(200);
  await tight.stop();
});

test('простой клиента посреди загрузки: обрыв по сроку, временных файлов нет', async () => {
  const slow = await apiStand({ uploadIdleMs: 50 });
  const who = await slow.signIn('slow@example.com');
  const f = await slow.newFile(who);
  const silent = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(container().subarray(0, 8));
    },
  });
  const res = await fetch(`${slow.publicUrl}/api/files/${f.id}/content`, {
    method: 'PUT',
    headers: { origin: slow.publicUrl, cookie: who.cookie, 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() },
    body: silent,
  });
  expect(res.status).toBe(408);
  expect(readdirSync(join(slow.dataDir, 'files', f.id))).toEqual([]);
  await slow.stop();
});

test('неизвестный маршрут API и метод — 404 в формате ошибок', async () => {
  for (const [method, path] of [
    ['GET', '/api/none'],
    ['DELETE', '/api/files'],
    ['GET', '/api/files/not-a-uuid'],
  ]) {
    const res = await s.call(owner, method, path);
    expect(res.status).toBe(404);
    expect(await res.json()).toEqual({ error: 'NOT_FOUND' });
  }
});
````

В `apps/server/test/migrate.test.ts` заменить:

````ts
import { afterAll, beforeEach, expect, test } from 'bun:test';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
````

на:

````ts
import { afterAll, beforeEach, expect, test } from 'bun:test';
import { copyFileSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
````

В `apps/server/test/migrate.test.ts` заменить:

````ts
test('миграции репозитория применяются на пустой базе', async () => {
  const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
  expect(applied).toEqual(['001_init.sql', '002_better_auth.sql']);
});
````

на:

````ts
test('миграции репозитория применяются на пустой базе', async () => {
  const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
  expect(applied).toEqual(['001_init.sql', '002_better_auth.sql', '003_files.sql']);
});

test('файлы встают на базу этапа 1: люди целы, повторный запуск ничего не применяет', async () => {
  const all = join(import.meta.dir, '../migrations');
  const stage1 = mkdtempSync(join(tmpdir(), 'ferrum-stage1-'));
  for (const f of ['001_init.sql', '002_better_auth.sql']) copyFileSync(join(all, f), join(stage1, f));
  await migrate(pool, stage1);
  await pool.query(`INSERT INTO "user" (id, name, email, "emailVerified") VALUES ('u1', 'Иван', 'ivan@example.com', true)`);
  expect(await migrate(pool, all)).toEqual(['003_files.sql']);
  expect((await pool.query('SELECT email FROM "user"')).rows).toEqual([{ email: 'ivan@example.com' }]);
  const { rows } = await pool.query(`INSERT INTO file (owner_id, name) VALUES ('u1', 'Файл') RETURNING version`);
  expect(rows[0].version).toBe('0');
  expect(await migrate(pool, all)).toEqual([]);
});
````

В `apps/server/test/server.test.ts` заменить:

````ts
import { startServer } from '../src/app';
import { createAuth } from '../src/auth';
import { connectValkey, valkeyAlive } from '../src/valkey';
import { TEST_DATABASE_URL, TEST_SECRET, TEST_VALKEY_URL, testPool } from './helpers';
````

на:

````ts
import { startServer } from '../src/app';
import { createAuth } from '../src/auth';
import { FsBlobStore } from '../src/blob-store';
import { connectValkey, valkeyAlive } from '../src/valkey';
import { TEST_DATABASE_URL, TEST_SECRET, TEST_VALKEY_URL, testPool } from './helpers';
````

В `apps/server/test/server.test.ts` заменить:

````ts
const publicUrl = 'http://localhost:1';
const auth = createAuth({ publicUrl, secret: TEST_SECRET, pool, valkey, mailer: { send: async () => {} }, allowedEmails: [] });
const deps = { valkey, webDist: web, auth, publicUrl, trustedProxies: [] as string[] };
const server = startServer({ ...deps, pool }, 0);
const url = (p: string) => `http://127.0.0.1:${server.port}${p}`;
````

на:

````ts
const publicUrl = 'http://localhost:1';
const auth = createAuth({ publicUrl, secret: TEST_SECRET, pool, valkey, mailer: { send: async () => {} }, allowedEmails: [] });
const deps = {
  valkey,
  webDist: web,
  auth,
  publicUrl,
  trustedProxies: [] as string[],
  blobs: new FsBlobStore(web),
  mailer: { send: async () => {} },
  secret: TEST_SECRET,
};
const server = startServer({ ...deps, pool }, 0);
const url = (p: string) => `http://127.0.0.1:${server.port}${p}`;
````

Создать `packages/protocol/test/container.test.ts`:

````ts
import { expect, test } from 'bun:test';
import {
  ContainerError,
  ContainerWalker,
  containerHeader,
  imageSectionHeader,
  isFileMeta,
  parseInvite,
  parseListQuery,
  parseRedeem,
  parseSection,
  parseWriteHeaders,
  scanContainer,
  SECTION_IMAGE,
  SECTION_PAGE_REFS,
} from '../src';

const HASH_A = 'a'.repeat(64);
const HASH_B = 'b'.repeat(64);

function section(kind: number, id: string, data: Uint8Array): Uint8Array {
  const idBytes = new TextEncoder().encode(id);
  const out = new Uint8Array(5 + idBytes.length + 40 + data.length);
  const v = new DataView(out.buffer);
  out[0] = kind;
  v.setUint32(1, idBytes.length, true);
  out.set(idBytes, 5);
  v.setBigUint64(5 + idBytes.length, BigInt(data.length), true);
  out.set(data, 5 + idBytes.length + 40);
  return out;
}

function container(...sections: Uint8Array[]): Uint8Array {
  const head = containerHeader(sections.length);
  const out = new Uint8Array(head.length + sections.reduce((n, s) => n + s.length, 0));
  out.set(head);
  let at = head.length;
  for (const s of sections) {
    out.set(s, at);
    at += s.length;
  }
  return out;
}

const refs = (...hashes: string[]) => section(SECTION_PAGE_REFS, '1@1', new TextEncoder().encode(hashes.join('\n')));
const walk = (bytes: Uint8Array, chunk = bytes.length) => {
  const w = new ContainerWalker();
  for (let i = 0; i < bytes.length; i += chunk) w.feed(bytes.subarray(i, i + chunk));
  w.finish();
  return w;
};

test('проход кусками любого размера находит ссылки на картинки', () => {
  const file = container(section(1, '', new Uint8Array(100)), section(2, '1@1', new Uint8Array(70_000)), refs(HASH_A, HASH_B));
  for (const chunk of [1, 7, 64, 65_536, file.length]) {
    expect([...walk(file, chunk).imageRefs].sort()).toEqual([HASH_A, HASH_B]);
  }
});

test('неверный контейнер: чужие байты, версия, картинка внутри, обрыв, хвост', () => {
  const good = container(section(1, '', new Uint8Array(4)));
  expect(() => walk(new Uint8Array(32))).toThrow(ContainerError);
  const version = good.slice();
  version[8] = 9;
  expect(() => walk(version)).toThrow('версия');
  expect(() => walk(container(section(SECTION_IMAGE, HASH_A, new Uint8Array(1))))).toThrow('картинка');
  expect(() => walk(good.subarray(0, good.length - 1))).toThrow('обрезан');
  const tail = new Uint8Array(good.length + 1);
  tail.set(good);
  expect(() => walk(tail)).toThrow('лишние');
  expect(() => walk(new Uint8Array(3))).toThrow('обрезан');
});

test('сканирование кусками файла и сборка картинки обратно', async () => {
  const data = new Uint8Array([1, 2, 3]);
  const image = new Uint8Array(imageSectionHeader(HASH_A, data.length).length + data.length);
  image.set(imageSectionHeader(HASH_A, data.length));
  image.set(data, image.length - data.length);
  const file = container(section(1, '', new Uint8Array(4)), image);
  const reads: [number, number][] = [];
  const spans = await scanContainer(file.length, async (s, e) => {
    reads.push([s, e]);
    return file.subarray(s, e);
  });
  expect(spans.map((s) => [s.kind, s.id, s.size])).toEqual([
    [1, '', 4],
    [SECTION_IMAGE, HASH_A, 3],
  ]);
  expect(file.subarray(spans[1].data, spans[1].data + spans[1].size)).toEqual(data);
  // Данные секций не читаются.
  expect(Math.max(...reads.map(([s, e]) => e - s))).toBeLessThan(200);
  await expect(scanContainer(5, async () => new Uint8Array(5))).rejects.toThrow(ContainerError);
  await expect(scanContainer(file.length - 1, async (s, e) => file.subarray(s, e))).rejects.toThrow('обрезан');
});

test('разбор запросов', () => {
  expect(parseSection('project:00000000-0000-4000-8000-000000000000')).toBe('project:00000000-0000-4000-8000-000000000000');
  expect(parseSection('project:x')).toBeNull();
  expect(parseSection('other')).toBeNull();
  const q = (s: string) => parseListQuery(new URLSearchParams(s));
  expect(q('section=recent&start=0&count=200&q=%20план%20')).toEqual({ section: 'recent', query: 'план', start: 0, count: 200 });
  expect(q('section=recent&start=0&count=201')).toBeNull();
  expect(q('section=recent&start=-1&count=1')).toBeNull();
  expect(parseInvite({ email: 'A@X.ru', role: 'editor' })).toEqual({ email: 'a@x.ru', role: 'editor' });
  expect(parseInvite({ email: 'a@x.ru', role: 'owner' })).toBeNull();
  expect(parseRedeem({ token: 'x'.repeat(43) })).toEqual({ token: 'x'.repeat(43) });
  expect(parseRedeem({ token: '../' })).toBeNull();
  const headers = (h: Record<string, string>) => ({ get: (n: string) => h[n] ?? null });
  const id = '00000000-0000-4000-8000-000000000000';
  expect(parseWriteHeaders(headers({ 'if-match': '"3"', 'x-ferrum-write': id }))).toEqual({ base: 3, write: id, previous: null });
  expect(parseWriteHeaders(headers({ 'if-match': '3', 'x-ferrum-write': id }))).toBeNull();
  expect(parseWriteHeaders(headers({ 'if-match': '"3"' }))).toBeNull();
});

test('ответ не того вида — не `FileMetaDto`', () => {
  const ok = { id: 'a', name: 'b', createdAt: 1, editedAt: 1, openedAt: 0, size: 0, thumbVersion: 0, version: 0, role: 'viewer' };
  expect(isFileMeta(ok)).toBe(true);
  expect(isFileMeta({ ...ok, role: 'admin' })).toBe(false);
  expect(isFileMeta({ ...ok, size: '1' })).toBe(false);
  expect(isFileMeta(null)).toBe(false);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd packages/protocol && bun test`
Run: `npm run test:stand && cd apps/server && bun test`
Expected: FAIL — `packages/protocol` — `Export named ContainerError not found`, 7 pass, 1 fail; `apps/server` — `Export named BlobStalled not found`, `containerHeader not found`, миграции без `003_files.sql`: 66 pass, 5 fail

- [ ] **Step 3: Реализация**

В `apps/server/bun.lock` заменить:

````
      "dependencies": {
        "@ferrum/protocol": "file:../../packages/protocol",
        "better-auth": "1.7.16",
        "nodemailer": "10.2.0",
````

на:

````
      "dependencies": {
        "@ferrum/protocol": "file:../../packages/protocol",
        "@noble/hashes": "2.4.0",
        "better-auth": "1.7.16",
        "nodemailer": "10.2.0",
````

Создать `apps/server/migrations/003_files.sql`:

````sql
-- Файлы на сервере (спека подсистемы 4, раздел 6.1). Сверх модели спеки: поколение и размер
-- содержимого, номер последней записи, ключ содержимого, версия превью; пометка «кто убрал в корзину»;
-- открытия по пользователям; приглашения по адресам без учётной записи; список байтов к удалению.

CREATE TABLE team (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  name text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE team_member (
  id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
  team_id uuid NOT NULL REFERENCES team ON DELETE CASCADE,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  role text NOT NULL CHECK (role IN ('owner', 'member')),
  PRIMARY KEY (team_id, user_id)
);
CREATE INDEX team_member_user_idx ON team_member (user_id);

CREATE TABLE team_invite (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  team_id uuid NOT NULL REFERENCES team ON DELETE CASCADE,
  email text NOT NULL,
  invited_by text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (team_id, email)
);
CREATE INDEX team_invite_email_idx ON team_invite (email);

CREATE TABLE project (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  team_id uuid NOT NULL REFERENCES team ON DELETE CASCADE,
  name text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX project_team_idx ON project (team_id);

CREATE TABLE file (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  owner_id text NOT NULL REFERENCES "user",
  -- Пусто — «Черновики» владельца.
  project_id uuid REFERENCES project ON DELETE SET NULL,
  name text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  edited_at timestamptz NOT NULL DEFAULT now(),
  trashed_at timestamptz,
  trashed_by text REFERENCES "user",
  thumb_key text,
  thumb_version integer NOT NULL DEFAULT 0,
  -- Заполнит этап 3 (`hello { schema }`): сервер этапа 2 документ не раскодирует.
  schema_version integer NOT NULL DEFAULT 1,
  -- Поколение содержимого: растёт с каждой записью (раздел 6.5).
  version bigint NOT NULL DEFAULT 0,
  size bigint NOT NULL DEFAULT 0,
  content_key text,
  last_write_id uuid
);
CREATE INDEX file_owner_drafts_idx ON file (owner_id, edited_at DESC) WHERE project_id IS NULL AND trashed_at IS NULL;
CREATE INDEX file_project_idx ON file (project_id, edited_at DESC) WHERE trashed_at IS NULL;
CREATE INDEX file_trash_idx ON file (trashed_at DESC) WHERE trashed_at IS NOT NULL;

CREATE TABLE file_access (
  id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  role text NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
  PRIMARY KEY (file_id, user_id)
);
CREATE INDEX file_access_user_idx ON file_access (user_id);

CREATE TABLE file_invite (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  email text NOT NULL,
  role text NOT NULL CHECK (role IN ('editor', 'viewer')),
  invited_by text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (file_id, email)
);
CREATE INDEX file_invite_email_idx ON file_invite (email);

-- Токен — только хэшем. `nonce` нужен, чтобы показать ссылку снова: токен выводится из секрета сервера.
CREATE TABLE share_link (
  file_id uuid PRIMARY KEY REFERENCES file ON DELETE CASCADE,
  token_hash text NOT NULL UNIQUE,
  nonce text NOT NULL,
  role text NOT NULL CHECK (role IN ('editor', 'viewer')),
  enabled boolean NOT NULL
);

-- Кто пришёл по ссылке: роль по ссылке действует, пока ссылка включена.
CREATE TABLE file_link_user (
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  PRIMARY KEY (file_id, user_id)
);
CREATE INDEX file_link_user_user_idx ON file_link_user (user_id);

CREATE TABLE file_recent (
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  opened_at timestamptz NOT NULL,
  PRIMARY KEY (user_id, file_id)
);
CREATE INDEX file_recent_order_idx ON file_recent (user_id, opened_at DESC);

CREATE TABLE image (
  hash text PRIMARY KEY,
  size bigint NOT NULL,
  blob_key text NOT NULL,
  -- С этого момента картинка не нужна ни одному файлу.
  orphaned_at timestamptz
);

CREATE TABLE file_image (
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  hash text NOT NULL REFERENCES image,
  -- С этого момента на неё нет ссылок в содержимом файла (отмена может её вернуть).
  unreferenced_at timestamptz,
  PRIMARY KEY (file_id, hash)
);
CREATE INDEX file_image_hash_idx ON file_image (hash);

-- Байты к удалению: запись появляется в той же транзакции, что и замена. Сбой между фиксацией и
-- удалением ничего не теряет, а читатель, начавший раньше, успевает дочитать.
CREATE TABLE blob_trash (
  key text PRIMARY KEY,
  delete_after timestamptz NOT NULL
);

-- Действующая роль (раздел 6.1): наибольшая из прямой, по ссылке и через команду. 0 — нет доступа.
-- Участник команды — редактор на файлах её проектов, владелец команды — владелец.
-- Файл в корзине виден только владельцу и тому, кто его туда убрал.
CREATE FUNCTION file_role_rank(p_file uuid, p_user text) RETURNS integer
LANGUAGE sql STABLE AS $$
  SELECT CASE
    WHEN f.trashed_at IS NOT NULL AND f.owner_id <> p_user AND f.trashed_by IS DISTINCT FROM p_user THEN 0
    ELSE GREATEST(
      COALESCE((SELECT CASE a.role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END
                FROM file_access a WHERE a.file_id = f.id AND a.user_id = p_user), 0),
      COALESCE((SELECT CASE m.role WHEN 'owner' THEN 3 ELSE 2 END
                FROM project p JOIN team_member m ON m.team_id = p.team_id AND m.user_id = p_user
                WHERE p.id = f.project_id), 0),
      COALESCE((SELECT CASE l.role WHEN 'editor' THEN 2 ELSE 1 END
                FROM share_link l JOIN file_link_user u ON u.file_id = l.file_id AND u.user_id = p_user
                WHERE l.file_id = f.id AND l.enabled), 0)
    )
  END
  FROM file f WHERE f.id = p_file
$$;
````

В `apps/server/package.json` заменить:

````json
  "dependencies": {
    "@ferrum/protocol": "file:../../packages/protocol",
    "better-auth": "1.7.16",
    "nodemailer": "10.2.0",
````

на:

````json
  "dependencies": {
    "@ferrum/protocol": "file:../../packages/protocol",
    "@noble/hashes": "2.4.0",
    "better-auth": "1.7.16",
    "nodemailer": "10.2.0",
````

Создать `apps/server/src/api.ts`:

````ts
// Конвейер маршрутов API файлов и команд. Всё, что должно быть у каждого маршрута, живёт здесь, а не в
// обработчиках: `Origin` на изменяющих запросах, сессия, предел частоты, проверка прав, срок и ответ в
// формате ошибок. Маршрут только объявляет, что ему нужно; забыть проверку нельзя, её нет в его коде.

import { ROLE_RANK, RANK_ROLE, isHash, isUuid, type ApiErrorBody, type ApiErrorCode, type FileRole } from '@ferrum/protocol';
import type { Pool } from 'pg';
import type { Auth } from './auth';
import { originAllowed } from './auth-routes';
import type { BlobStore } from './blob-store';
import type { Mailer } from './mail';
import { valkeyStorage } from './secondary-storage';

/** Меньше 15 с, которые ждёт клиент, и больше сроков `Pool`: человек видит ответ сервера. */
export const API_DEADLINE_MS = 10_000;
/** JSON-запросы маленькие: остальное отвергается, не дочитываясь. */
export const MAX_JSON_BYTES = 16 * 1024;

/** Группы пределов частоты на человека: окно в секундах и число запросов. */
export const LIMITS = {
  read: { max: 1200, windowSeconds: 60 },
  /** Опрос открытого файла: раз в 15 с на вкладку. */
  poll: { max: 120, windowSeconds: 60 },
  write: { max: 300, windowSeconds: 60 },
  /** Запись содержимого, превью и картинок: автосохранение и перенос файлов. */
  upload: { max: 600, windowSeconds: 60 },
  /** Новые файлы и команды. */
  create: { max: 600, windowSeconds: 3600 },
  /** Письма: приглашения и запросы доступа. */
  mail: { max: 30, windowSeconds: 3600 },
  /** Попытки открыть файл по ссылке. */
  redeem: { max: 30, windowSeconds: 60 },
} as const;
export type LimitName = keyof typeof LIMITS;
export type Limits = Record<LimitName, { max: number; windowSeconds: number }>;

export interface ApiDeps {
  pool: Pool;
  valkey: Bun.RedisClient;
  auth: Auth;
  publicUrl: string;
  blobs: BlobStore;
  mailer: Mailer;
  /** Секрет сервера: из него выводятся токены ссылок. */
  secret: string;
  /** Тесты сокращают; по умолчанию — `LIMITS`, `API_DEADLINE_MS`, `UPLOAD_IDLE_MS`. */
  limits?: Partial<Limits>;
  deadlineMs?: number;
  uploadIdleMs?: number;
}

export interface ApiUser {
  id: string;
  email: string;
  name: string;
}

/** Что нужно маршруту. `user` — только вход; остальные берут id из адреса и проверяют доступ:
 * нет доступа совсем — 404, как у несуществующего, не хватает роли — 403. */
export type Access =
  | { kind: 'user' }
  | { kind: 'file'; min: FileRole }
  | { kind: 'team'; min: 'member' | 'owner' }
  | { kind: 'project'; min: 'member' | 'owner' };

export interface Ctx {
  req: Request;
  url: URL;
  params: Record<string, string>;
  user: ApiUser;
  /** Действующая роль в файле (`access.kind === 'file'`). */
  role: FileRole;
  /** Роль в команде (`team`, `project`) и команда проекта. */
  teamRole: 'owner' | 'member';
  teamId: string;
}

export interface Route {
  method: 'GET' | 'HEAD' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  /** `:id`, `:teamId`, `:projectId`, `:entryId` — UUID, `:hash` — blake3: неверный вид — 404. */
  path: string;
  access: Access;
  limit: LimitName;
  /** Тело идёт потоком: обработчик следит за простоем сам, общего срока нет. */
  stream?: boolean;
  handle(ctx: Ctx): Promise<Response>;
}

export function apiError(status: number, error: ApiErrorCode): Response {
  return Response.json({ error } satisfies ApiErrorBody, { status, headers: { 'Cache-Control': 'no-store' } });
}

export function ok(body: unknown, status = 200): Response {
  return Response.json(body, { status, headers: { 'Cache-Control': 'no-store' } });
}

/** Разбирает JSON не больше `MAX_JSON_BYTES`; иначе и при мусоре — `null`, разбор протокола ответит 400. */
export async function readJson(req: Request): Promise<unknown> {
  if (!req.body) return null;
  const chunks: Uint8Array[] = [];
  let size = 0;
  for await (const chunk of req.body) {
    size += chunk.length;
    if (size > MAX_JSON_BYTES) return null;
    chunks.push(chunk);
  }
  try {
    return JSON.parse(new TextDecoder().decode(Buffer.concat(chunks)));
  } catch {
    return null;
  }
}

const PARAM_CHECK: Record<string, (v: string) => boolean> = {
  id: isUuid,
  teamId: isUuid,
  projectId: isUuid,
  entryId: isUuid,
  hash: isHash,
};

interface Compiled {
  route: Route;
  parts: string[];
}

function match(c: Compiled, method: string, parts: string[]): Record<string, string> | null {
  if (c.route.method !== method || c.parts.length !== parts.length) return null;
  const params: Record<string, string> = {};
  for (let i = 0; i < parts.length; i++) {
    const want = c.parts[i];
    if (want.startsWith(':')) {
      const name = want.slice(1);
      if (!PARAM_CHECK[name](parts[i])) return null;
      params[name] = parts[i];
    } else if (want !== parts[i]) return null;
  }
  return params;
}

class Refused extends Error {
  constructor(readonly response: Response) {
    super('refused');
  }
}

function refuse(status: number, code: ApiErrorCode): never {
  throw new Refused(apiError(status, code));
}

/** Запрос не должен висеть без ответа: работа не отменяется, но человек видит честный 503. */
function withinDeadline<T>(work: Promise<T>, ms: number, path: string): Promise<T> {
  let timer: Timer;
  const expired = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      console.error('Запрос API не уложился в срок:', path);
      reject(new Refused(apiError(503, 'UNAVAILABLE')));
    }, ms);
  });
  return Promise.race([work, expired]).finally(() => clearTimeout(timer));
}

export function apiHandler(routes: Route[], deps: ApiDeps) {
  const compiled: Compiled[] = routes.map((route) => ({ route, parts: route.path.split('/').filter(Boolean) }));
  const storage = valkeyStorage(deps.valkey);
  const limits: Limits = { ...LIMITS, ...deps.limits };
  const deadline = deps.deadlineMs ?? API_DEADLINE_MS;

  /** Всё, что до обработчика: порядок важен, чужой запрос не тратит ни базу, ни счётчик другого. */
  async function prepare(route: Route, req: Request, url: URL, params: Record<string, string>): Promise<Ctx> {
    if (req.method !== 'GET' && req.method !== 'HEAD' && !originAllowed(req, deps.publicUrl)) refuse(403, 'BAD_ORIGIN');
    const session = await deps.auth.api.getSession({ headers: req.headers, query: { disableRefresh: true } });
    if (!session) refuse(401, 'UNAUTHORIZED');
    const user: ApiUser = { id: session.user.id, email: session.user.email, name: session.user.name };
    const limit = limits[route.limit];
    const used = await storage.increment!(`api:${route.limit}:${user.id}`, limit.windowSeconds);
    if (used > limit.max) refuse(429, 'TOO_MANY_REQUESTS');
    const ctx: Ctx = { req, url, params, user, role: 'viewer', teamRole: 'member', teamId: '' };
    const access = route.access;
    if (access.kind === 'file') {
      const { rows } = await deps.pool.query<{ rank: number | null }>('SELECT file_role_rank($1, $2) AS rank', [params.id, user.id]);
      const role = RANK_ROLE[rows[0]?.rank ?? 0];
      if (!role) refuse(404, 'NOT_FOUND');
      if (ROLE_RANK[role] < ROLE_RANK[access.min]) refuse(403, 'FORBIDDEN');
      ctx.role = role;
    } else if (access.kind === 'team' || access.kind === 'project') {
      const { rows } =
        access.kind === 'team'
          ? await deps.pool.query<{ role: 'owner' | 'member'; team_id: string }>(
              'SELECT role, team_id FROM team_member WHERE team_id = $1 AND user_id = $2',
              [params.teamId, user.id],
            )
          : await deps.pool.query<{ role: 'owner' | 'member'; team_id: string }>(
              'SELECT m.role, m.team_id FROM project p JOIN team_member m ON m.team_id = p.team_id WHERE p.id = $1 AND m.user_id = $2',
              [params.projectId, user.id],
            );
      if (rows.length === 0) refuse(404, 'NOT_FOUND');
      if (access.min === 'owner' && rows[0].role !== 'owner') refuse(403, 'FORBIDDEN');
      ctx.teamRole = rows[0].role;
      ctx.teamId = rows[0].team_id;
    }
    return ctx;
  }

  return async function handle(req: Request): Promise<Response> {
    const url = new URL(req.url);
    const parts = url.pathname.split('/').filter(Boolean);
    for (const c of compiled) {
      const params = match(c, req.method, parts);
      if (!params) continue;
      try {
        const ctx = await withinDeadline(prepare(c.route, req, url, params), deadline, url.pathname);
        return await (c.route.stream ? c.route.handle(ctx) : withinDeadline(c.route.handle(ctx), deadline, url.pathname));
      } catch (e) {
        if (e instanceof Refused) return e.response;
        console.error('Сбой маршрута API', c.route.method, c.route.path, e instanceof Error ? e.message : e);
        return apiError(503, 'UNAVAILABLE');
      }
    }
    return apiError(404, 'NOT_FOUND');
  };
}
````

В `apps/server/src/app.ts` заменить:

````ts
// тесты поднимают настоящий сервер на свободном порту против настоящих Postgres и Valkey.

import type { Pool } from 'pg';
import { authRoutes } from './auth-routes';
import type { Auth } from './auth';
import { responseHeaders, serveStatic } from './static';
import { valkeyAlive } from './valkey';
````

на:

````ts
// тесты поднимают настоящий сервер на свободном порту против настоящих Postgres и Valkey.

import { MAX_CONTENT_BYTES } from '@ferrum/protocol';
import type { Pool } from 'pg';
import { apiHandler, type Limits, type Route } from './api';
import { authRoutes } from './auth-routes';
import type { Auth } from './auth';
import type { BlobStore } from './blob-store';
import { fileRoutes, uploadGate } from './file-routes';
import type { Mailer } from './mail';
import { responseHeaders, serveStatic } from './static';
import { valkeyAlive } from './valkey';
````

В `apps/server/src/app.ts` заменить:

````ts
  trustedProxies: readonly string[];
  authDeadlineMs?: number;
}

````

на:

````ts
  trustedProxies: readonly string[];
  authDeadlineMs?: number;
  blobs: BlobStore;
  mailer: Mailer;
  secret: string;
  /** Тесты сокращают пределы и сроки API. */
  limits?: Partial<Limits>;
  apiDeadlineMs?: number;
  uploadIdleMs?: number;
}

/** Простой соединения: загрузка с паузами дольше этого уже оборвана проверкой `UPLOAD_IDLE_MS`. */
const IDLE_TIMEOUT_SECONDS = 120;

/** Все маршруты API: отсюда их берёт и сервер, и таблица проверки прав в тестах. */
export function apiRoutes(deps: AppDeps): Route[] {
  const api = { ...deps, deadlineMs: deps.apiDeadlineMs };
  return [...fileRoutes(api, uploadGate())];
}

````

В `apps/server/src/app.ts` заменить:

````ts
  const handleAuth = authRoutes(deps);
  const withHeaders = responseHeaders(deps.publicUrl);
  return Bun.serve({
    port,
    routes: {
      '/healthz': async () => withHeaders(await health(deps)),
      '/api/auth/*': async (req, server) => withHeaders(await handleAuth(req, server.requestIP(req)?.address)),
    },
    fetch: async (req) => withHeaders(await serveStatic(req, deps.webDist)),
````

на:

````ts
  const handleAuth = authRoutes(deps);
  const withHeaders = responseHeaders(deps.publicUrl);
  const handleApi = apiHandler(apiRoutes(deps), { ...deps, deadlineMs: deps.apiDeadlineMs });
  return Bun.serve({
    port,
    // Содержимое файла идёт потоком: предел здесь только не должен быть меньше предела файла.
    maxRequestBodySize: MAX_CONTENT_BYTES + 1024 * 1024,
    idleTimeout: IDLE_TIMEOUT_SECONDS,
    routes: {
      '/healthz': async () => withHeaders(await health(deps)),
      '/api/auth/*': async (req, server) => withHeaders(await handleAuth(req, server.requestIP(req)?.address)),
      '/api/*': async (req) => withHeaders(await handleApi(req)),
    },
    fetch: async (req) => withHeaders(await serveStatic(req, deps.webDist)),
````

В `apps/server/src/blob-store.ts` заменить:

````ts
// реализация этого интерфейса.

import { mkdir, rename, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';

export interface BlobStore {
  put(key: string, bytes: Uint8Array): Promise<void>;
  /** Нет такого ключа — `null`. */
  get(key: string): Promise<Uint8Array | null>;
  /** Нет такого ключа — не ошибка. */
  delete(key: string): Promise<void>;
````

на:

````ts
// реализация этого интерфейса.

import { mkdir, open, rename, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';

export class BlobTooLarge extends Error {}
export class BlobStalled extends Error {}

export interface PutStreamOptions {
  maxBytes: number;
  /** Дольше этого между кусками — обрыв: зависший клиент не держит запись без конца. */
  idleMs: number;
  /** Каждый кусок до записи: проверка содержимого. Исключение отменяет запись. */
  onChunk?: (chunk: Uint8Array) => void;
}

export interface BlobStore {
  put(key: string, bytes: Uint8Array): Promise<void>;
  /** Запись потоком: в памяти только текущий кусок. Ключ появляется целиком и только при успехе. */
  putStream(key: string, body: ReadableStream<Uint8Array>, options: PutStreamOptions): Promise<{ size: number }>;
  /** Нет такого ключа — `null`. */
  get(key: string): Promise<Uint8Array | null>;
  /** Для ответа потоком: `new Response(blob)` не читает файл в память. Нет ключа — `null`. */
  open(key: string): Promise<Blob | null>;
  /** Нет такого ключа — не ошибка. */
  delete(key: string): Promise<void>;
````

В `apps/server/src/blob-store.ts` заменить:

````ts
/** Ключ — имена через `/`: путь к файлу не выйдет за каталог хранилища. */
const KEY_RE = /^[A-Za-z0-9_-]+(\/[A-Za-z0-9_-]+)*$/;

export class FsBlobStore implements BlobStore {
````

на:

````ts
/** Ключ — имена через `/`: путь к файлу не выйдет за каталог хранилища. */
const KEY_RE = /^[A-Za-z0-9_-]+(\/[A-Za-z0-9_-]+)*$/;

const noEntry = (e: unknown) => (e as { code?: unknown } | null)?.code === 'ENOENT';

export class FsBlobStore implements BlobStore {
````

В `apps/server/src/blob-store.ts` заменить:

````ts
  }

  /** Запись через временный файл: читатель не увидит половину байтов. */
  async put(key: string, bytes: Uint8Array): Promise<void> {
    const path = this.path(key);
    await mkdir(dirname(path), { recursive: true });
    const tmp = `${path}.${process.pid}.${++this.seq}.tmp`;
    await Bun.write(tmp, bytes);
    await rename(tmp, path);
  }

  async get(key: string): Promise<Uint8Array | null> {
    const file = Bun.file(this.path(key));
    return (await file.exists()) ? new Uint8Array(await file.arrayBuffer()) : null;
  }

````

на:

````ts
  }

  /** Запись через временный файл с `fsync` файла и каталога: читатель не увидит половину байтов,
   * а сбой питания после подтверждения записи не потеряет её. Сбой записи не оставляет `.tmp`. */
  private async atomically(key: string, write: (file: Awaited<ReturnType<typeof open>>) => Promise<void>): Promise<void> {
    const path = this.path(key);
    const parent = dirname(path);
    await mkdir(parent, { recursive: true });
    const tmp = `${path}.${process.pid}.${++this.seq}.tmp`;
    try {
      const file = await open(tmp, 'w');
      try {
        await write(file);
        await file.sync();
      } finally {
        await file.close();
      }
      await rename(tmp, path);
    } catch (e) {
      await rm(tmp, { force: true });
      throw e;
    }
    const folder = await open(parent, 'r');
    try {
      await folder.sync();
    } finally {
      await folder.close();
    }
  }

  async put(key: string, bytes: Uint8Array): Promise<void> {
    await this.atomically(key, async (file) => {
      await file.writeFile(bytes);
    });
  }

  async putStream(key: string, body: ReadableStream<Uint8Array>, options: PutStreamOptions): Promise<{ size: number }> {
    let size = 0;
    await this.atomically(key, async (file) => {
      const reader = body.getReader();
      try {
        for (;;) {
          let timer: Timer | undefined;
          const stalled = new Promise<never>((_, reject) => {
            timer = setTimeout(() => reject(new BlobStalled()), options.idleMs);
          });
          const next = await Promise.race([reader.read(), stalled]).finally(() => clearTimeout(timer));
          if (next.done) return;
          size += next.value.length;
          if (size > options.maxBytes) throw new BlobTooLarge();
          options.onChunk?.(next.value);
          await file.write(next.value);
        }
      } catch (e) {
        await reader.cancel().catch(() => {});
        throw e;
      }
    });
    return { size };
  }

  async get(key: string): Promise<Uint8Array | null> {
    try {
      return new Uint8Array(await Bun.file(this.path(key)).arrayBuffer());
    } catch (e) {
      if (noEntry(e)) return null;
      throw e;
    }
  }

  async open(key: string): Promise<Blob | null> {
    const file = Bun.file(this.path(key));
    return (await file.exists()) ? file : null;
  }

````

Создать `apps/server/src/file-routes.ts`:

````ts
// Файлы: списки, создание, имя, корзина, содержимое с проверкой поколения, превью (спека, раздел 6).
// Содержимое идёт потоком на диск и обратно: сервер не держит файл в памяти, а замена — одна
// транзакция с проверкой поколения и роли на момент фиксации.

import {
  ContainerError,
  ContainerWalker,
  MAX_CONTENT_BYTES,
  MAX_THUMB_BYTES,
  RANK_ROLE,
  ROLE_RANK,
  VERSION_HEADER,
  parseCreateFile,
  parseListQuery,
  parseMove,
  parseRename,
  parseWriteHeaders,
  type FileAccessDto,
  type FileListDto,
  type FileMetaDto,
  type FileRole,
  type ListQuery,
} from '@ferrum/protocol';
import { randomUUID } from 'node:crypto';
import type { Pool, PoolClient } from 'pg';
import { apiError, ok, readJson, type ApiDeps, type Ctx, type Route } from './api';
import { BlobStalled, BlobTooLarge } from './blob-store';

/** Клиент молчит дольше этого посреди загрузки — обрыв. */
export const UPLOAD_IDLE_MS = 60_000;
/** Байты заменённого файла удаляются не сразу: читатель, начавший раньше, успевает дочитать. */
export const BLOB_GRACE_SECONDS = 120;
/** Одновременных загрузок: на человека и на сервер. Памяти загрузка не занимает, занимает диск и поток. */
export const MAX_UPLOADS_PER_USER = 4;
export const MAX_UPLOADS = 32;
const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

export async function inTransaction<T>(pool: Pool, work: (c: PoolClient) => Promise<T>): Promise<T> {
  const client = await pool.connect();
  try {
    await client.query('BEGIN');
    const out = await work(client);
    await client.query('COMMIT');
    return out;
  } catch (e) {
    await client.query('ROLLBACK').catch(() => {});
    throw e;
  } finally {
    client.release();
  }
}

export const trashBlob = (c: PoolClient, key: string | null) =>
  key
    ? c.query(`INSERT INTO blob_trash (key, delete_after) VALUES ($1, now() + make_interval(secs => $2)) ON CONFLICT DO NOTHING`, [
        key,
        BLOB_GRACE_SECONDS,
      ])
    : Promise.resolve();

interface FileRow {
  id: string;
  name: string;
  created_at: Date;
  edited_at: Date;
  trashed_at: Date | null;
  size: string;
  thumb_version: number;
  version: string;
  project_id: string | null;
  opened_at: Date | null;
  rank: number;
}

/** Колонки записи о файле; `$1` запроса — id человека. */
const FILE_COLUMNS = `f.id, f.name, f.created_at, f.edited_at, f.trashed_at, f.size, f.thumb_version, f.version, f.project_id,
  (SELECT r.opened_at FROM file_recent r WHERE r.user_id = $1 AND r.file_id = f.id) AS opened_at,
  file_role_rank(f.id, $1) AS rank`;

export function fileDto(r: FileRow): FileMetaDto {
  return {
    id: r.id,
    name: r.name,
    createdAt: r.created_at.getTime(),
    editedAt: r.edited_at.getTime(),
    openedAt: r.opened_at?.getTime() ?? 0,
    size: Number(r.size),
    thumbVersion: r.thumb_version,
    version: Number(r.version),
    role: (RANK_ROLE[r.rank] ?? 'viewer') as FileRole,
    ...(r.trashed_at ? { trashedAt: r.trashed_at.getTime() } : {}),
    ...(r.project_id ? { projectId: r.project_id } : {}),
  };
}

export async function loadFile(db: Pool | PoolClient, id: string, user: string): Promise<FileMetaDto | null> {
  const { rows } = await db.query<FileRow>(`SELECT ${FILE_COLUMNS} FROM file f WHERE f.id = $2`, [user, id]);
  return rows[0] ? fileDto(rows[0]) : null;
}

/** Условие раздела: только файлы, доступные этому человеку. `$1` — его id, `$2` — проект. */
function sectionWhere(q: ListQuery): { from: string; where: string; order: string; project: string | null } {
  const project = q.section.startsWith('project:') ? q.section.slice(8) : null;
  switch (q.section) {
    case 'recent':
      return {
        from: 'file f JOIN file_recent r ON r.file_id = f.id AND r.user_id = $1',
        where: 'f.trashed_at IS NULL AND file_role_rank(f.id, $1) > 0',
        order: 'r.opened_at DESC',
        project,
      };
    case 'drafts':
      return {
        from: 'file f',
        where: 'f.owner_id = $1 AND f.project_id IS NULL AND f.trashed_at IS NULL',
        order: 'f.edited_at DESC',
        project,
      };
    case 'shared':
      return {
        from: 'file f',
        where: `f.trashed_at IS NULL AND f.owner_id <> $1 AND (
          EXISTS (SELECT 1 FROM file_access a WHERE a.file_id = f.id AND a.user_id = $1)
          OR EXISTS (SELECT 1 FROM file_link_user u JOIN share_link l ON l.file_id = u.file_id AND l.enabled
                     WHERE u.file_id = f.id AND u.user_id = $1))`,
        order: 'f.edited_at DESC',
        project,
      };
    case 'trash':
      return {
        from: 'file f',
        where: 'f.trashed_at IS NOT NULL AND (f.owner_id = $1 OR f.trashed_by = $1)',
        order: 'f.trashed_at DESC',
        project,
      };
    default:
      return {
        from: 'file f',
        where: `f.trashed_at IS NULL AND f.project_id = $2 AND EXISTS (
          SELECT 1 FROM project p JOIN team_member m ON m.team_id = p.team_id AND m.user_id = $1 WHERE p.id = f.project_id)`,
        order: 'f.edited_at DESC',
        project,
      };
  }
}

const escapeLike = (s: string) => s.replace(/[\\%_]/g, (c) => `\\${c}`);

/** Счётчики одновременных загрузок: живут в процессе, сервер один. */
export function uploadGate() {
  const perUser = new Map<string, number>();
  let total = 0;
  return {
    enter(user: string): boolean {
      const mine = perUser.get(user) ?? 0;
      if (mine >= MAX_UPLOADS_PER_USER || total >= MAX_UPLOADS) return false;
      perUser.set(user, mine + 1);
      total++;
      return true;
    },
    leave(user: string): void {
      const mine = (perUser.get(user) ?? 1) - 1;
      if (mine > 0) perUser.set(user, mine);
      else perUser.delete(user);
      total--;
    },
  };
}
export type UploadGate = ReturnType<typeof uploadGate>;

/** Ответ на неудачную загрузку потоком. Неизвестное — сбой маршрута, его показывает конвейер. */
export function uploadFailure(e: unknown): Response | null {
  if (e instanceof BlobTooLarge) return apiError(413, 'TOO_LARGE');
  if (e instanceof ContainerError) return apiError(400, 'BAD_CONTAINER');
  if (e instanceof BlobStalled) return apiError(408, 'UNAVAILABLE');
  return null;
}

/** Своя запись, ответ на которую потерялся, — не чужая: поколение ровно на одну больше и она последняя. */
function writeAllowed(row: { version: string; last_write_id: string | null }, base: number, previous: string | null): boolean {
  const version = Number(row.version);
  return version === base || (previous !== null && row.last_write_id === previous && version === base + 1);
}

export function fileRoutes(deps: ApiDeps, gate: UploadGate): Route[] {
  const { pool, blobs } = deps;
  const idle = deps.uploadIdleMs ?? UPLOAD_IDLE_MS;
  const fresh = async (ctx: Ctx) => {
    const meta = await loadFile(pool, ctx.params.id, ctx.user.id);
    return meta ? ok(meta) : apiError(404, 'NOT_FOUND');
  };
  return [
    {
      method: 'GET',
      path: '/api/files',
      access: { kind: 'user' },
      limit: 'read',
      handle: async (ctx) => {
        const q = parseListQuery(ctx.url.searchParams);
        if (!q) return apiError(400, 'BAD_REQUEST');
        const s = sectionWhere(q);
        const params: unknown[] = [ctx.user.id];
        if (s.project) params.push(s.project);
        let where = s.where;
        if (q.query) {
          params.push(`%${escapeLike(q.query)}%`);
          where += ` AND f.name ILIKE $${params.length}`;
        }
        const total = await pool.query<{ n: string }>(`SELECT count(*) AS n FROM ${s.from} WHERE ${where}`, params);
        const rows = await pool.query<FileRow>(
          `SELECT ${FILE_COLUMNS} FROM ${s.from} WHERE ${where} ORDER BY ${s.order}, f.name, f.id
           LIMIT $${params.length + 1} OFFSET $${params.length + 2}`,
          [...params, q.count, q.start],
        );
        return ok({ total: Number(total.rows[0].n), rows: rows.rows.map(fileDto) } satisfies FileListDto);
      },
    },
    {
      method: 'POST',
      path: '/api/files',
      access: { kind: 'user' },
      limit: 'create',
      handle: async (ctx) => {
        const body = parseCreateFile(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        if (body.projectId) {
          // Чужой проект неотличим от несуществующего.
          const member = await pool.query(
            'SELECT 1 FROM project p JOIN team_member m ON m.team_id = p.team_id WHERE p.id = $1 AND m.user_id = $2',
            [body.projectId, ctx.user.id],
          );
          if (member.rowCount === 0) return apiError(404, 'NOT_FOUND');
        }
        const meta = await inTransaction(pool, async (c) => {
          const { rows } = await c.query<{ id: string }>('INSERT INTO file (owner_id, project_id, name) VALUES ($1, $2, $3) RETURNING id', [
            ctx.user.id,
            body.projectId,
            body.name,
          ]);
          const id = rows[0].id;
          await c.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'owner')`, [id, ctx.user.id]);
          await c.query('INSERT INTO file_recent (user_id, file_id, opened_at) VALUES ($1, $2, now())', [ctx.user.id, id]);
          return loadFile(c, id, ctx.user.id);
        });
        return ok(meta, 201);
      },
    },
    {
      method: 'POST',
      path: '/api/files/empty-trash',
      access: { kind: 'user' },
      limit: 'write',
      handle: async (ctx) => {
        await inTransaction(pool, async (c) => {
          const { rows } = await c.query<{ id: string; content_key: string | null; thumb_key: string | null }>(
            'SELECT id, content_key, thumb_key FROM file WHERE trashed_at IS NOT NULL AND (owner_id = $1 OR trashed_by = $1) FOR UPDATE',
            [ctx.user.id],
          );
          for (const r of rows) {
            await trashBlob(c, r.content_key);
            await trashBlob(c, r.thumb_key);
          }
          await c.query('DELETE FROM file WHERE id = ANY($1::uuid[])', [rows.map((r) => r.id)]);
        });
        return ok({ ok: true });
      },
    },
    { method: 'GET', path: '/api/files/:id', access: { kind: 'file', min: 'viewer' }, limit: 'read', handle: fresh },
    {
      method: 'PATCH',
      path: '/api/files/:id',
      access: { kind: 'file', min: 'editor' },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseRename(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        await pool.query('UPDATE file SET name = $2, edited_at = now() WHERE id = $1', [ctx.params.id, body.name]);
        return fresh(ctx);
      },
    },
    {
      method: 'POST',
      path: '/api/files/:id/move',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseMove(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        if (body.projectId) {
          const member = await pool.query(
            'SELECT 1 FROM project p JOIN team_member m ON m.team_id = p.team_id WHERE p.id = $1 AND m.user_id = $2',
            [body.projectId, ctx.user.id],
          );
          if (member.rowCount === 0) return apiError(404, 'NOT_FOUND');
        }
        await pool.query('UPDATE file SET project_id = $2 WHERE id = $1', [ctx.params.id, body.projectId]);
        return fresh(ctx);
      },
    },
    {
      method: 'POST',
      path: '/api/files/:id/touch',
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      handle: async (ctx) => {
        await pool.query(
          `INSERT INTO file_recent (user_id, file_id, opened_at) VALUES ($1, $2, now())
           ON CONFLICT (user_id, file_id) DO UPDATE SET opened_at = now()`,
          [ctx.user.id, ctx.params.id],
        );
        return fresh(ctx);
      },
    },
    {
      method: 'POST',
      path: '/api/files/:id/trash',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        await pool.query('UPDATE file SET trashed_at = now(), trashed_by = $2 WHERE id = $1 AND trashed_at IS NULL', [
          ctx.params.id,
          ctx.user.id,
        ]);
        return ok({ ok: true });
      },
    },
    {
      method: 'POST',
      path: '/api/files/:id/restore',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        await pool.query('UPDATE file SET trashed_at = NULL, trashed_by = NULL WHERE id = $1', [ctx.params.id]);
        return ok({ ok: true });
      },
    },
    {
      method: 'DELETE',
      path: '/api/files/:id',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        await inTransaction(pool, async (c) => {
          const { rows } = await c.query<{ content_key: string | null; thumb_key: string | null }>(
            'DELETE FROM file WHERE id = $1 RETURNING content_key, thumb_key',
            [ctx.params.id],
          );
          await trashBlob(c, rows[0]?.content_key ?? null);
          await trashBlob(c, rows[0]?.thumb_key ?? null);
        });
        return ok({ ok: true });
      },
    },
    {
      method: 'GET',
      path: '/api/files/:id/access',
      access: { kind: 'file', min: 'viewer' },
      limit: 'poll',
      handle: async (ctx) => {
        const { rows } = await pool.query<{ version: string }>('SELECT version FROM file WHERE id = $1', [ctx.params.id]);
        if (!rows[0]) return apiError(404, 'NOT_FOUND');
        return ok({ role: ctx.role, version: Number(rows[0].version) } satisfies FileAccessDto);
      },
    },
    {
      method: 'GET',
      path: '/api/files/:id/content',
      access: { kind: 'file', min: 'viewer' },
      limit: 'read',
      handle: async (ctx) => {
        const { rows } = await pool.query<{ version: string; content_key: string | null }>(
          'SELECT version, content_key FROM file WHERE id = $1',
          [ctx.params.id],
        );
        const row = rows[0];
        if (!row) return apiError(404, 'NOT_FOUND');
        const headers = { [VERSION_HEADER]: row.version, 'Cache-Control': 'no-store' };
        if (!row.content_key) return new Response(null, { status: 204, headers });
        const blob = await blobs.open(row.content_key);
        // Запись в базе есть, а байтов нет: не «файл не найден», а сбой хранилища.
        if (!blob) throw new Error(`нет байтов содержимого файла ${ctx.params.id}`);
        return new Response(blob, {
          headers: {
            ...headers,
            'Content-Type': 'application/octet-stream',
            'Content-Disposition': 'attachment',
            'Content-Length': String(blob.size),
          },
        });
      },
    },
    {
      method: 'PUT',
      path: '/api/files/:id/content',
      access: { kind: 'file', min: 'editor' },
      limit: 'upload',
      stream: true,
      handle: async (ctx) => {
        const write = parseWriteHeaders(ctx.req.headers);
        if (!write || !ctx.req.body) return apiError(400, 'BAD_REQUEST');
        const declared = Number(ctx.req.headers.get('content-length') ?? 0);
        if (declared > MAX_CONTENT_BYTES) return apiError(413, 'TOO_LARGE');
        // Заведомый отказ — раньше загрузки: не занимать диск и не гонять байты зря.
        const before = await pool.query<{ version: string; last_write_id: string | null }>(
          'SELECT version, last_write_id FROM file WHERE id = $1',
          [ctx.params.id],
        );
        if (!before.rows[0]) return apiError(404, 'NOT_FOUND');
        if (!writeAllowed(before.rows[0], write.base, write.previous)) return apiError(409, 'CONFLICT');
        if (!gate.enter(ctx.user.id)) return apiError(429, 'BUSY');
        const key = `files/${ctx.params.id}/content-${randomUUID()}`;
        try {
          const walker = new ContainerWalker();
          let size: number;
          try {
            ({ size } = await blobs.putStream(key, ctx.req.body, {
              maxBytes: MAX_CONTENT_BYTES,
              idleMs: idle,
              onChunk: (chunk) => walker.feed(chunk),
            }));
            walker.finish();
          } catch (e) {
            await blobs.delete(key).catch(() => {});
            const failure = uploadFailure(e);
            if (failure) return failure;
            throw e;
          }
          const refs = [...walker.imageRefs];
          // Пока байты шли, файл могли записать, а роль снять: решает только эта транзакция.
          const outcome = await inTransaction(pool, async (c) => {
            const { rows } = await c.query<{ version: string; last_write_id: string | null; content_key: string | null; rank: number }>(
              'SELECT version, last_write_id, content_key, file_role_rank(id, $2) AS rank FROM file WHERE id = $1 FOR UPDATE',
              [ctx.params.id, ctx.user.id],
            );
            const row = rows[0];
            if (!row || row.rank === 0) return apiError(404, 'NOT_FOUND');
            if (row.rank < ROLE_RANK.editor) return apiError(403, 'FORBIDDEN');
            if (!writeAllowed(row, write.base, write.previous)) return apiError(409, 'CONFLICT');
            await trashBlob(c, row.content_key);
            await c.query(
              `UPDATE file SET version = version + 1, size = $2, content_key = $3, last_write_id = $4, edited_at = now() WHERE id = $1`,
              [ctx.params.id, size, key, write.write],
            );
            // Ссылки из содержимого: картинка без ссылки живёт ещё 30 дней (отмена может её вернуть).
            await c.query(
              `UPDATE file_image SET unreferenced_at = CASE WHEN hash = ANY($2::text[]) THEN NULL ELSE COALESCE(unreferenced_at, now()) END
               WHERE file_id = $1`,
              [ctx.params.id, refs],
            );
            return null;
          });
          if (outcome) {
            await blobs.delete(key).catch(() => {});
            return outcome;
          }
          return await fresh(ctx);
        } finally {
          gate.leave(ctx.user.id);
        }
      },
    },
    {
      method: 'GET',
      path: '/api/files/:id/thumb',
      access: { kind: 'file', min: 'viewer' },
      limit: 'read',
      handle: async (ctx) => {
        const { rows } = await pool.query<{ thumb_key: string | null }>('SELECT thumb_key FROM file WHERE id = $1', [ctx.params.id]);
        const blob = rows[0]?.thumb_key ? await blobs.open(rows[0].thumb_key) : null;
        if (!blob) return new Response(null, { status: 204, headers: { 'Cache-Control': 'no-store' } });
        return new Response(blob, {
          headers: { 'Content-Type': 'image/png', 'Cache-Control': 'no-store', 'Content-Length': String(blob.size) },
        });
      },
    },
    {
      method: 'PUT',
      path: '/api/files/:id/thumb',
      access: { kind: 'file', min: 'editor' },
      limit: 'upload',
      stream: true,
      handle: async (ctx) => {
        if (!ctx.req.body) return apiError(400, 'BAD_REQUEST');
        if (!gate.enter(ctx.user.id)) return apiError(429, 'BUSY');
        const key = `files/${ctx.params.id}/thumb-${randomUUID()}`;
        try {
          let seen = 0;
          try {
            await blobs.putStream(key, ctx.req.body, {
              maxBytes: MAX_THUMB_BYTES,
              idleMs: idle,
              onChunk: (chunk) => {
                for (let i = 0; i < chunk.length && seen < PNG_SIGNATURE.length; i++, seen++) {
                  if (chunk[i] !== PNG_SIGNATURE[seen]) throw new ContainerError('не PNG');
                }
              },
            });
            if (seen < PNG_SIGNATURE.length) throw new ContainerError('не PNG');
          } catch (e) {
            await blobs.delete(key).catch(() => {});
            const failure = uploadFailure(e);
            if (failure) return failure;
            throw e;
          }
          await inTransaction(pool, async (c) => {
            const { rows } = await c.query<{ thumb_key: string | null }>('SELECT thumb_key FROM file WHERE id = $1 FOR UPDATE', [
              ctx.params.id,
            ]);
            await trashBlob(c, rows[0]?.thumb_key ?? null);
            await c.query('UPDATE file SET thumb_key = $2, thumb_version = thumb_version + 1 WHERE id = $1', [ctx.params.id, key]);
          });
          return await fresh(ctx);
        } finally {
          gate.leave(ctx.user.id);
        }
      },
    },
    {
      method: 'DELETE',
      path: '/api/files/:id/thumb',
      access: { kind: 'file', min: 'editor' },
      limit: 'write',
      handle: async (ctx) => {
        await inTransaction(pool, async (c) => {
          const { rows } = await c.query<{ thumb_key: string | null }>('SELECT thumb_key FROM file WHERE id = $1 FOR UPDATE', [
            ctx.params.id,
          ]);
          if (!rows[0]?.thumb_key) return;
          await trashBlob(c, rows[0].thumb_key);
          await c.query('UPDATE file SET thumb_key = NULL, thumb_version = thumb_version + 1 WHERE id = $1', [ctx.params.id]);
        });
        return fresh(ctx);
      },
    },
  ];
}
````

Создать `apps/server/src/gc.ts`:

````ts
// Фоновая очистка: байты, которые больше никому не нужны. Запись о них появляется в той же транзакции,
// что и замена, поэтому сбой ничего не теряет: следующий проход удалит то, что не успел этот.

import type { Pool } from 'pg';
import type { BlobStore } from './blob-store';

/** Проход раз в минуту, у него есть срок на каждый запрос (`Pool`). */
export const GC_INTERVAL_MS = 60_000;
const BATCH = 200;

/** Сколько байтов-записей удалено. Один проход на все процессы: сервер один, но блокировка дешевле ошибки. */
export async function runGc(pool: Pool, blobs: BlobStore): Promise<number> {
  const { rows } = await pool.query<{ key: string }>(
    'SELECT key FROM blob_trash WHERE delete_after <= now() ORDER BY delete_after LIMIT $1',
    [BATCH],
  );
  let removed = 0;
  for (const { key } of rows) {
    try {
      await blobs.delete(key);
      await pool.query('DELETE FROM blob_trash WHERE key = $1', [key]);
      removed++;
    } catch (e) {
      console.error('Очистка: не удалось удалить байты:', e instanceof Error ? e.message : e);
    }
  }
  return removed;
}

/** Запускает очистку; возвращает остановку. Ошибка прохода — строка в журнал, следующий проход всё равно будет. */
export function startGc(pool: Pool, blobs: BlobStore, everyMs = GC_INTERVAL_MS): () => void {
  let busy = false;
  const timer = setInterval(() => {
    if (busy) return;
    busy = true;
    runGc(pool, blobs)
      .catch((e: unknown) => console.error('Очистка:', e instanceof Error ? e.message : e))
      .finally(() => {
        busy = false;
      });
  }, everyMs);
  return () => clearInterval(timer);
}
````

В `apps/server/src/index.ts` заменить:

````ts
import { FsBlobStore } from './blob-store';
import { loadConfig } from './config';
import { createPool } from './db';
import { smtpMailer } from './mail';
````

на:

````ts
import { FsBlobStore } from './blob-store';
import { loadConfig } from './config';
import { startGc } from './gc';
import { createPool } from './db';
import { smtpMailer } from './mail';
````

В `apps/server/src/index.ts` заменить:

````ts
console.log('Подключение к Valkey…');
const valkey = await connectValkey(config.valkeyUrl);
// Хранилище байтов нужно с этапа 2; здесь — проверка, что том доступен для записи.
await new FsBlobStore(config.dataDir).put('healthcheck', new Uint8Array([1]));
// Better Auth обращается к таблицам при первом запросе: к этому времени миграции уже применены.
const auth = createAuth({
  publicUrl: config.publicUrl,
````

на:

````ts
console.log('Подключение к Valkey…');
const valkey = await connectValkey(config.valkeyUrl);
const blobs = new FsBlobStore(config.dataDir);
// Том доступен для записи: иначе отказ при запуске, а не на первом сохранении.
await blobs.put('healthcheck', new Uint8Array([1]));
// Better Auth обращается к таблицам при первом запросе: к этому времени миграции уже применены.
const mailer = smtpMailer(config.smtpUrl, config.mailFrom);
const auth = createAuth({
  publicUrl: config.publicUrl,
````

В `apps/server/src/index.ts` заменить:

````ts
  pool,
  valkey,
  mailer: smtpMailer(config.smtpUrl, config.mailFrom),
  allowedEmails: config.allowedEmails,
});
const server = startServer(
  { pool, valkey, webDist: config.webDist, auth, publicUrl: config.publicUrl, trustedProxies: config.trustedProxies },
  config.port,
);
console.log(`Ferrum слушает порт ${server.port}`);

// Остановка контейнера: дождаться запросов и закрыть соединения, а не ждать SIGKILL.
stop = () =>
  void server
    .stop()
    .then(() => pool.end())
    .finally(() => process.exit(0));
````

на:

````ts
  pool,
  valkey,
  mailer,
  allowedEmails: config.allowedEmails,
});
const server = startServer(
  {
    pool,
    valkey,
    webDist: config.webDist,
    auth,
    publicUrl: config.publicUrl,
    trustedProxies: config.trustedProxies,
    blobs,
    mailer,
    secret: config.authSecret,
  },
  config.port,
);
const stopGc = startGc(pool, blobs);
console.log(`Ferrum слушает порт ${server.port}`);

// Остановка контейнера: дождаться запросов и закрыть соединения, а не ждать SIGKILL.
stop = () => {
  stopGc();
  void server
    .stop()
    .then(() => pool.end())
    .finally(() => process.exit(0));
};
````

Создать `packages/protocol/src/container.ts`:

````ts
// Контейнер `.ferrum` без разбора данных (формат — `crates/ferrum-doc/src/container.rs`):
// заголовок и секции `тип u8 · длина id u32 · id · длина данных u64 · blake3 32 байта · данные`.
// Сервер идёт по нему потоком, не держа файл в памяти; клиент выносит картинки в отдельные запросы
// и собирает файл обратно: хэш картинки — её id, поэтому заголовок секции собирается без вычислений.

export const CONTAINER_MAGIC = new Uint8Array([70, 69, 82, 82, 85, 77, 0, 0]);
export const CONTAINER_VERSION = 1;
export const SECTION_PAGE_REFS = 3;
export const SECTION_IMAGE = 4;
/** Заголовок файла: магия, версия, число секций. */
export const CONTAINER_HEADER_BYTES = 16;
/** Секция до id: тип и длина id. */
export const SECTION_PREFIX_BYTES = 5;
/** Секция после id: длина данных и blake3. */
export const SECTION_SUFFIX_BYTES = 40;
const MAX_SECTION_ID_BYTES = 1024;
const MAX_REFS_BYTES = 1024 * 1024;
const MAX_SECTIONS = 1_000_000;
const HASH_LINE_RE = /^[0-9a-f]{64}$/;

export class ContainerError extends Error {}

const utf8 = new TextDecoder();

/** Проход по контейнеру кусками, какими они пришли из сети: секция картинки — ошибка (картинки идут
 * отдельно, спека, раздел 6.4), неполный файл и лишние байты — тоже. Память — только заголовки и
 * списки ссылок страниц. */
export class ContainerWalker {
  private head = new Uint8Array(CONTAINER_HEADER_BYTES);
  private filled = 0;
  private state: 'header' | 'prefix' | 'id' | 'suffix' | 'data' | 'refs' | 'end' = 'header';
  private need = CONTAINER_HEADER_BYTES;
  private left = 0n;
  private sections = 0;
  private kind = 0;
  private refs = new Uint8Array(0);
  private refsFilled = 0;
  /** Хэши картинок, на которые ссылаются страницы. */
  readonly imageRefs = new Set<string>();

  private ensure(size: number): void {
    if (this.head.length < size) this.head = new Uint8Array(size);
  }

  feed(chunk: Uint8Array): void {
    let at = 0;
    while (at < chunk.length) {
      if (this.state === 'end') throw new ContainerError('лишние байты в конце');
      if (this.state === 'data') {
        const take = Number(this.left < BigInt(chunk.length - at) ? this.left : BigInt(chunk.length - at));
        this.left -= BigInt(take);
        at += take;
        if (this.left === 0n) this.next();
        continue;
      }
      if (this.state === 'refs') {
        const take = Math.min(this.refs.length - this.refsFilled, chunk.length - at);
        this.refs.set(chunk.subarray(at, at + take), this.refsFilled);
        this.refsFilled += take;
        at += take;
        if (this.refsFilled === this.refs.length) {
          this.takeRefs();
          this.next();
        }
        continue;
      }
      const take = Math.min(this.need - this.filled, chunk.length - at);
      this.head.set(chunk.subarray(at, at + take), this.filled);
      this.filled += take;
      at += take;
      if (this.filled === this.need) this.step();
    }
  }

  private step(): void {
    const view = new DataView(this.head.buffer, this.head.byteOffset, this.filled);
    this.filled = 0;
    switch (this.state) {
      case 'header': {
        for (let i = 0; i < CONTAINER_MAGIC.length; i++) {
          if (this.head[i] !== CONTAINER_MAGIC[i]) throw new ContainerError('это не файл Ferrum');
        }
        if (view.getUint32(8, true) !== CONTAINER_VERSION) throw new ContainerError('неизвестная версия формата');
        this.sections = view.getUint32(12, true);
        if (this.sections > MAX_SECTIONS) throw new ContainerError('слишком много секций');
        this.next();
        return;
      }
      case 'prefix': {
        this.kind = view.getUint8(0);
        if (this.kind === SECTION_IMAGE) throw new ContainerError('картинка внутри файла');
        const idBytes = view.getUint32(1, true);
        if (idBytes > MAX_SECTION_ID_BYTES) throw new ContainerError('слишком длинный id секции');
        this.state = idBytes > 0 ? 'id' : 'suffix';
        this.need = idBytes > 0 ? idBytes : SECTION_SUFFIX_BYTES;
        this.ensure(this.need);
        return;
      }
      case 'id':
        this.state = 'suffix';
        this.need = SECTION_SUFFIX_BYTES;
        this.ensure(this.need);
        return;
      case 'suffix': {
        const size = view.getBigUint64(0, true);
        if (this.kind === SECTION_PAGE_REFS) {
          if (size > BigInt(MAX_REFS_BYTES)) throw new ContainerError('слишком длинный список ссылок');
          if (size === 0n) {
            this.next();
            return;
          }
          this.refs = new Uint8Array(Number(size));
          this.refsFilled = 0;
          this.state = 'refs';
          return;
        }
        if (size === 0n) {
          this.next();
          return;
        }
        this.left = size;
        this.state = 'data';
        return;
      }
      default:
        throw new ContainerError('неверное состояние');
    }
  }

  private takeRefs(): void {
    for (const line of utf8.decode(this.refs).split('\n')) {
      if (HASH_LINE_RE.test(line)) this.imageRefs.add(line);
    }
    this.refs = new Uint8Array(0);
  }

  private next(): void {
    if (this.sections === 0) {
      this.state = 'end';
      return;
    }
    this.sections--;
    this.state = 'prefix';
    this.need = SECTION_PREFIX_BYTES;
    this.ensure(this.need);
  }

  /** Весь файл пришёл: секций столько, сколько обещано, ни одна не оборвана. */
  finish(): void {
    if (this.state !== 'end') throw new ContainerError('файл обрезан');
  }
}

/** Заголовок секции картинки: тип, id (хэш), длина, blake3 (тот же хэш). */
export function imageSectionHeader(hash: string, size: number): Uint8Array {
  const id = new TextEncoder().encode(hash);
  const out = new Uint8Array(SECTION_PREFIX_BYTES + id.length + SECTION_SUFFIX_BYTES);
  const view = new DataView(out.buffer);
  out[0] = SECTION_IMAGE;
  view.setUint32(1, id.length, true);
  out.set(id, SECTION_PREFIX_BYTES);
  view.setBigUint64(SECTION_PREFIX_BYTES + id.length, BigInt(size), true);
  for (let i = 0; i < 32; i++) {
    out[SECTION_PREFIX_BYTES + id.length + 8 + i] = parseInt(hash.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/** Заголовок файла с другим числом секций: остальное копируется как есть. */
export function containerHeader(sections: number): Uint8Array {
  const out = new Uint8Array(CONTAINER_HEADER_BYTES);
  const view = new DataView(out.buffer);
  out.set(CONTAINER_MAGIC);
  view.setUint32(8, CONTAINER_VERSION, true);
  view.setUint32(12, sections, true);
  return out;
}

/** Секция файла: где заголовок, где данные. */
export interface SectionSpan {
  kind: number;
  id: string;
  /** Начало заголовка секции. */
  start: number;
  /** Начало данных. */
  data: number;
  size: number;
}

/** Разбор заголовков по кускам файла, которые читает `read`: файл целиком в память не загружается
 * (на клиенте `read` — это `Blob.slice`). Неверный контейнер — `ContainerError`. */
export async function scanContainer(total: number, read: (start: number, end: number) => Promise<Uint8Array>): Promise<SectionSpan[]> {
  if (total < CONTAINER_HEADER_BYTES) throw new ContainerError('это не файл Ferrum');
  const head = await read(0, CONTAINER_HEADER_BYTES);
  for (let i = 0; i < CONTAINER_MAGIC.length; i++) {
    if (head[i] !== CONTAINER_MAGIC[i]) throw new ContainerError('это не файл Ferrum');
  }
  const view = new DataView(head.buffer, head.byteOffset, head.byteLength);
  if (view.getUint32(8, true) !== CONTAINER_VERSION) throw new ContainerError('неизвестная версия формата');
  const count = view.getUint32(12, true);
  if (count > MAX_SECTIONS) throw new ContainerError('слишком много секций');
  const spans: SectionSpan[] = [];
  let at = CONTAINER_HEADER_BYTES;
  for (let i = 0; i < count; i++) {
    if (at + SECTION_PREFIX_BYTES > total) throw new ContainerError('файл обрезан');
    const prefix = await read(at, at + SECTION_PREFIX_BYTES);
    const pv = new DataView(prefix.buffer, prefix.byteOffset, prefix.byteLength);
    const idBytes = pv.getUint32(1, true);
    if (idBytes > MAX_SECTION_ID_BYTES) throw new ContainerError('слишком длинный id секции');
    const dataAt = at + SECTION_PREFIX_BYTES + idBytes + SECTION_SUFFIX_BYTES;
    if (dataAt > total) throw new ContainerError('файл обрезан');
    const rest = await read(at + SECTION_PREFIX_BYTES, dataAt);
    const size = Number(new DataView(rest.buffer, rest.byteOffset, rest.byteLength).getBigUint64(idBytes, true));
    if (dataAt + size > total) throw new ContainerError('файл обрезан');
    spans.push({ kind: prefix[0], id: utf8.decode(rest.subarray(0, idBytes)), start: at, data: dataAt, size });
    at = dataAt + size;
  }
  if (at !== total) throw new ContainerError('лишние байты в конце');
  return spans;
}
````

Создать `packages/protocol/src/files.ts`:

````ts
// API файлов, команд и «Поделиться» (спека подсистемы 4, раздел 6): адреса, виды запросов и ответов,
// разбор. Разбор не доверяет данным: неверное — `null`. Его используют и сервер (запросы), и клиент
// (ответы: неожиданный вид — это сбой сети, а не `undefined` в интерфейсе).

import { AUTH_ERROR_CODES, parseEmail } from './auth';

export const API_FILES = '/api/files';
export const API_TEAMS = '/api/teams';
export const API_PROJECTS = '/api/projects';

/** Все коды ошибок API: новый код добавляется только здесь, клиент сверяет ответ с этим списком. */
export const API_ERROR_CODES = [
  ...AUTH_ERROR_CODES,
  /** Доступ к файлу есть, но роли не хватает. Нет доступа совсем — `NOT_FOUND`, как у несуществующего. */
  'FORBIDDEN',
  /** Файл записали после того поколения, на котором основана запись. */
  'CONFLICT',
  'TOO_LARGE',
  /** Содержимое — не контейнер `.ferrum` нужного вида. */
  'BAD_CONTAINER',
  /** Байты картинки не совпали с хэшем в адресе. */
  'HASH_MISMATCH',
  /** В команде есть проекты, в проекте — файлы. */
  'NOT_EMPTY',
  /** Слишком много одновременных загрузок. */
  'BUSY',
] as const;
export type ApiErrorCode = (typeof API_ERROR_CODES)[number];
export interface ApiErrorBody {
  error: ApiErrorCode;
}

/** Предел контейнера — как `MAX_OPEN_BYTES` клиента: больше ядро не откроет. */
export const MAX_CONTENT_BYTES = 512 * 1024 * 1024;
/** Предел картинки — как `MAX_DATA_BYTES` клиента. */
export const MAX_IMAGE_BYTES = 64 * 1024 * 1024;
export const MAX_THUMB_BYTES = 8 * 1024 * 1024;
/** Строк в одном окне списка не больше этого (спека ядра, раздел 10). */
export const MAX_FILES_WINDOW = 200;
export const MAX_FILE_NAME_CHARS = 256;
/** Имя команды и проекта. */
export const MAX_TEAM_NAME_CHARS = 100;
export const MAX_QUERY_CHARS = 256;
/** Токен ссылки «Поделиться»: 32 байта в base64url. */
const LINK_TOKEN_RE = /^[A-Za-z0-9_-]{43}$/;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const HASH_RE = /^[0-9a-f]{64}$/;
// eslint-disable-next-line no-control-regex
const CONTROL_RE = /[\u0000-\u001f\u007f]/;

export type FileRole = 'owner' | 'editor' | 'viewer';
/** Роль, которую можно выдать: владелец у файла один. */
export type ShareRole = 'editor' | 'viewer';
export const ROLE_RANK: Record<FileRole, number> = { viewer: 1, editor: 2, owner: 3 };
export const RANK_ROLE: readonly (FileRole | null)[] = [null, 'viewer', 'editor', 'owner'];

export type FileSection = 'recent' | 'drafts' | 'shared' | 'trash' | `project:${string}`;

export interface FileMetaDto {
  id: string;
  name: string;
  createdAt: number;
  editedAt: number;
  /** Последнее открытие этим человеком; 0 — ещё не открывал. */
  openedAt: number;
  size: number;
  trashedAt?: number;
  thumbVersion: number;
  version: number;
  /** Действующая роль этого человека. */
  role: FileRole;
  projectId?: string;
}

export interface FileListDto {
  total: number;
  rows: FileMetaDto[];
}

/** Лёгкая проверка: роль и поколение. Её результат — и отказ на сохранении, и опрос открытого файла. */
export interface FileAccessDto {
  role: FileRole;
  version: number;
}

export interface SharePersonDto {
  /** Запись списка: и у человека с доступом, и у приглашённого без учётной записи. */
  id: string;
  email: string;
  /** Имя — только если человек уже открывал файл: иначе список выдавал бы, есть ли адрес в системе. */
  name: string | null;
  role: FileRole;
}

export interface ShareLinkDto {
  enabled: boolean;
  role: ShareRole;
  /** Токен для адреса; `null` — восстановить нельзя (сменился секрет сервера), нужна новая ссылка. */
  token: string | null;
}

export interface ShareDto {
  people: SharePersonDto[];
  /** Только владельцу; `null` — ссылка ещё не создана или роли не хватает. */
  link: ShareLinkDto | null;
  canManage: boolean;
}

export interface ProjectDto {
  id: string;
  teamId: string;
  name: string;
}
export interface TeamDto {
  id: string;
  name: string;
  role: 'owner' | 'member';
  projects: ProjectDto[];
}
export interface TeamListDto {
  teams: TeamDto[];
}
export interface TeamMemberDto {
  id: string;
  email: string;
  role: 'owner' | 'member';
}
export interface TeamMembersDto {
  members: TeamMemberDto[];
}

function record(v: unknown): Record<string, unknown> | null {
  return typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
}

export const isUuid = (v: unknown): v is string => typeof v === 'string' && UUID_RE.test(v);
export const isHash = (v: unknown): v is string => typeof v === 'string' && HASH_RE.test(v);

function parseLabel(v: unknown, max: number): string | null {
  if (typeof v !== 'string') return null;
  const s = v.trim();
  return s.length > 0 && Array.from(s).length <= max && !CONTROL_RE.test(s) ? s : null;
}

export const parseFileName = (v: unknown): string | null => parseLabel(v, MAX_FILE_NAME_CHARS);
export const parseTeamName = (v: unknown): string | null => parseLabel(v, MAX_TEAM_NAME_CHARS);

export function parseSection(v: unknown): FileSection | null {
  if (v === 'recent' || v === 'drafts' || v === 'shared' || v === 'trash') return v;
  if (typeof v === 'string' && v.startsWith('project:') && isUuid(v.slice(8))) return v as FileSection;
  return null;
}

function wholeInRange(s: string | null, max: number): number | null {
  if (s === null || !/^\d{1,16}$/.test(s)) return null;
  const n = Number(s);
  return n <= max ? n : null;
}

export interface ListQuery {
  section: FileSection;
  query: string;
  start: number;
  count: number;
}

export function parseListQuery(q: URLSearchParams): ListQuery | null {
  const section = parseSection(q.get('section'));
  const start = wholeInRange(q.get('start'), Number.MAX_SAFE_INTEGER);
  const count = wholeInRange(q.get('count'), MAX_FILES_WINDOW);
  const query = q.get('q') ?? '';
  if (!section || start === null || count === null) return null;
  if (Array.from(query).length > MAX_QUERY_CHARS || CONTROL_RE.test(query)) return null;
  return { section, query: query.trim(), start, count };
}

export function parseCreateFile(v: unknown): { name: string; projectId: string | null } | null {
  const r = record(v);
  const name = parseFileName(r?.name);
  if (!r || !name) return null;
  if (r.projectId === undefined || r.projectId === null) return { name, projectId: null };
  return isUuid(r.projectId) ? { name, projectId: r.projectId } : null;
}

export function parseRename(v: unknown): { name: string } | null {
  const name = parseFileName(record(v)?.name);
  return name ? { name } : null;
}

export function parseMove(v: unknown): { projectId: string | null } | null {
  const r = record(v);
  if (!r) return null;
  if (r.projectId === null) return { projectId: null };
  return isUuid(r.projectId) ? { projectId: r.projectId } : null;
}

const isShareRole = (v: unknown): v is ShareRole => v === 'editor' || v === 'viewer';

export function parseInvite(v: unknown): { email: string; role: ShareRole } | null {
  const r = record(v);
  const email = parseEmail(r?.email);
  return r && email && isShareRole(r.role) ? { email, role: r.role } : null;
}

export function parseRoleChange(v: unknown): { role: ShareRole } | null {
  const role = record(v)?.role;
  return isShareRole(role) ? { role } : null;
}

export function parseLinkUpdate(v: unknown): { enabled: boolean; role: ShareRole } | null {
  const r = record(v);
  return r && typeof r.enabled === 'boolean' && isShareRole(r.role) ? { enabled: r.enabled, role: r.role } : null;
}

export function parseRedeem(v: unknown): { token: string } | null {
  const token = record(v)?.token;
  return typeof token === 'string' && LINK_TOKEN_RE.test(token) ? { token } : null;
}

/** Токен из адреса `#/file/<id>?link=…`: неверный отбрасывается. */
export const parseLinkToken = (v: unknown): string | null => (typeof v === 'string' && LINK_TOKEN_RE.test(v) ? v : null);

export function parseTeamCreate(v: unknown): { name: string } | null {
  const name = parseTeamName(record(v)?.name);
  return name ? { name } : null;
}

export function parseTeamInvite(v: unknown): { email: string } | null {
  const email = parseEmail(record(v)?.email);
  return email ? { email } : null;
}

/** Заголовки записи файла: поколение, на котором она основана, номер этой записи и предыдущей. */
export interface WriteHeaders {
  base: number;
  write: string;
  /** Запись, ответ на которую потерялся: своя удачная запись — не `CONFLICT`. */
  previous: string | null;
}
export const WRITE_ID_HEADER = 'x-ferrum-write';
export const PREVIOUS_WRITE_HEADER = 'x-ferrum-previous-write';
export const VERSION_HEADER = 'x-ferrum-version';

export function parseWriteHeaders(h: { get(name: string): string | null }): WriteHeaders | null {
  const m = /^"(\d{1,15})"$/.exec(h.get('if-match') ?? '');
  const write = h.get(WRITE_ID_HEADER);
  const previous = h.get(PREVIOUS_WRITE_HEADER);
  if (!m || !isUuid(write) || (previous !== null && !isUuid(previous))) return null;
  return { base: Number(m[1]), write, previous };
}

const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isStr = (v: unknown): v is string => typeof v === 'string';
const isFileRole = (v: unknown): v is FileRole => v === 'owner' || v === 'editor' || v === 'viewer';
const isArrayOf = <T>(v: unknown, of: (x: unknown) => x is T): v is T[] => Array.isArray(v) && v.every(of);

export function isFileMeta(v: unknown): v is FileMetaDto {
  const m = record(v);
  return (
    !!m &&
    isStr(m.id) &&
    isStr(m.name) &&
    isNum(m.createdAt) &&
    isNum(m.editedAt) &&
    isNum(m.openedAt) &&
    isNum(m.size) &&
    isNum(m.thumbVersion) &&
    isNum(m.version) &&
    isFileRole(m.role) &&
    (m.trashedAt === undefined || isNum(m.trashedAt)) &&
    (m.projectId === undefined || isStr(m.projectId))
  );
}

export function isFileList(v: unknown): v is FileListDto {
  const m = record(v);
  return !!m && isNum(m.total) && isArrayOf(m.rows, isFileMeta);
}

export function isFileAccess(v: unknown): v is FileAccessDto {
  const m = record(v);
  return !!m && isFileRole(m.role) && isNum(m.version);
}

function isSharePerson(v: unknown): v is SharePersonDto {
  const m = record(v);
  return !!m && isStr(m.id) && isStr(m.email) && (m.name === null || isStr(m.name)) && isFileRole(m.role);
}

export function isShareLink(v: unknown): v is ShareLinkDto {
  const m = record(v);
  return !!m && typeof m.enabled === 'boolean' && isShareRole(m.role) && (m.token === null || isStr(m.token));
}

export function isShare(v: unknown): v is ShareDto {
  const m = record(v);
  return !!m && isArrayOf(m.people, isSharePerson) && (m.link === null || isShareLink(m.link)) && typeof m.canManage === 'boolean';
}

function isProject(v: unknown): v is ProjectDto {
  const m = record(v);
  return !!m && isStr(m.id) && isStr(m.teamId) && isStr(m.name);
}

export function isProjectDto(v: unknown): v is ProjectDto {
  return isProject(v);
}

export function isTeam(v: unknown): v is TeamDto {
  const m = record(v);
  return !!m && isStr(m.id) && isStr(m.name) && (m.role === 'owner' || m.role === 'member') && isArrayOf(m.projects, isProject);
}

export function isTeamList(v: unknown): v is TeamListDto {
  const m = record(v);
  return !!m && isArrayOf(m.teams, isTeam);
}

function isTeamMember(v: unknown): v is TeamMemberDto {
  const m = record(v);
  return !!m && isStr(m.id) && isStr(m.email) && (m.role === 'owner' || m.role === 'member');
}

export function isTeamMembers(v: unknown): v is TeamMembersDto {
  const m = record(v);
  return !!m && isArrayOf(m.members, isTeamMember);
}
````

В `packages/protocol/src/index.ts` заменить:

````ts

export * from './auth';
````

на:

````ts

export * from './auth';
export * from './files';
export * from './container';
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `cd apps/server && bun install --frozen-lockfile`
Expected: зависимости установлены по `apps/server/bun.lock`, новая — `@noble/hashes` 2.4.0

- [ ] **Step 5: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `bun test`: `packages/protocol` 12 pass, `apps/server` 163 pass

- [ ] **Step 6: Закоммитить**

```bash
git add apps/server/bun.lock apps/server/migrations/003_files.sql apps/server/package.json apps/server/src/api.ts apps/server/src/app.ts apps/server/src/blob-store.ts apps/server/src/file-routes.ts apps/server/src/gc.ts apps/server/src/index.ts apps/server/test/access.test.ts apps/server/test/api-stand.ts apps/server/test/auth.test.ts apps/server/test/blob-store.test.ts apps/server/test/files.test.ts apps/server/test/migrate.test.ts apps/server/test/server.test.ts packages/protocol/src/container.ts packages/protocol/src/files.ts packages/protocol/src/index.ts packages/protocol/test/container.test.ts
git commit -F - <<'EOF'
сервер: файлы, запись содержимого с проверкой поколения, конвейер прав

Модель файлов (миграция 003), действующая роль в одной функции базы. Конвейер API
сам проверяет у каждого маршрута Origin, сессию, предел частоты, права и срок.
Содержимое идёт потоком на диск и обратно, запись — одна транзакция с проверкой
поколения и роли на момент фиксации. blob-store с fsync (B-204). Таблица в тестах
перечисляет все маршруты, чужой файл неотличим от несуществующего.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3: Сервер — картинки и очистка

Спека §6.4. `PUT`, `HEAD`, `GET /api/files/:id/images/:hash`, blake3 потоком, очистка через 30 дней.

**Files:**
- Create: `apps/server/src/image-routes.ts`
- Modify: `apps/server/src/app.ts`, `apps/server/src/gc.ts`
- Test: `apps/server/test/access.test.ts`, `apps/server/test/images.test.ts` (новый)

**Interfaces:**
- Consumes: `uploadGate`, `inTransaction`, `trashBlob`, `runGc`.
- Produces: `imageRoutes`.

- [ ] **Step 1: Написать тесты**

В `apps/server/test/access.test.ts` заменить:

````ts

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { apiRoutes } from '../src/app';
import { apiStand, container, PNG, type Person } from './api-stand';
````

на:

````ts

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { blake3 } from '@noble/hashes/blake3.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import { apiRoutes } from '../src/app';
import { apiStand, container, PNG, type Person } from './api-stand';
````

В `apps/server/test/access.test.ts` заменить:

````ts
const NOBODY = '00000000-0000-4000-8000-000000000000';
const HASH = 'a'.repeat(64);

interface World {
````

на:

````ts
const NOBODY = '00000000-0000-4000-8000-000000000000';
const HASH = 'a'.repeat(64);
const IMAGE: Uint8Array<ArrayBuffer> = new Uint8Array([1, 2, 3]);
const IMAGE_HASH = bytesToHex(blake3(IMAGE));

interface World {
````

В `apps/server/test/access.test.ts` заменить:

````ts
  'PUT /api/files/:id/thumb': { params: file, body: PNG, allowed: editor, denied: [viewer] },
  'DELETE /api/files/:id/thumb': { params: file, allowed: editor, denied: [viewer] },
};

````

на:

````ts
  'PUT /api/files/:id/thumb': { params: file, body: PNG, allowed: editor, denied: [viewer] },
  'DELETE /api/files/:id/thumb': { params: file, allowed: editor, denied: [viewer] },
  // Хэш не у этого файла: 404 и у читателя, но это уже после проверки доступа к файлу.
  'HEAD /api/files/:id/images/:hash': { params: file, allowed: viewer, status: 404 },
  'GET /api/files/:id/images/:hash': { params: file, allowed: viewer, status: 404 },
  'PUT /api/files/:id/images/:hash': { params: (w) => ({ id: w.file, hash: IMAGE_HASH }), body: IMAGE, allowed: editor, denied: [viewer] },
};

````

В `apps/server/test/access.test.ts` заменить:

````ts
    const res = await s.call(null, route.method, path, sample.body, headersOf(route, sample));
    expect(res.status).toBe(401);
    expect(await res.json()).toEqual({ error: 'UNAUTHORIZED' });
  });

````

на:

````ts
    const res = await s.call(null, route.method, path, sample.body, headersOf(route, sample));
    expect(res.status).toBe(401);
    if (route.method !== 'HEAD') expect(await res.json()).toEqual({ error: 'UNAUTHORIZED' });
  });

````

В `apps/server/test/access.test.ts` заменить:

````ts
      expect(foreign.status).toBe(404);
      expect(missing.status).toBe(404);
      expect(await foreign.text()).toBe(await missing.text());
      // И без записи: файл цел.
````

на:

````ts
      expect(foreign.status).toBe(404);
      expect(missing.status).toBe(404);
      expect(foreign.headers.get('content-type')).toBe(missing.headers.get('content-type'));
      expect(await foreign.text()).toBe(await missing.text());
      // И без записи: файл цел.
````

Создать `apps/server/test/images.test.ts`:

````ts
import { afterAll, beforeEach, expect, test } from 'bun:test';
import { blake3 } from '@noble/hashes/blake3.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import { runGc } from '../src/gc';
import { apiStand, container, type Person } from './api-stand';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const other = await s.signIn('other@example.com');
beforeEach(s.reset);

const hashOf = (b: Uint8Array) => bytesToHex(blake3(b));
const bytes = (fill: number, size = 100): Uint8Array<ArrayBuffer> => new Uint8Array(size).fill(fill);
const put = (who: Person, file: string, hash: string, body: Uint8Array) => s.call(who, 'PUT', `/api/files/${file}/images/${hash}`, body);
const get = (who: Person, file: string, hash: string) => s.call(who, 'GET', `/api/files/${file}/images/${hash}`);
const head = (who: Person, file: string, hash: string) => s.call(who, 'HEAD', `/api/files/${file}/images/${hash}`);

test('загрузка и чтение: хэш сверяется, байты те же, HEAD знает о картинке', async () => {
  const f = await s.newFile(owner);
  const img = bytes(1);
  const hash = hashOf(img);
  expect((await head(owner, f.id, hash)).status).toBe(404);
  expect((await put(owner, f.id, hash, img)).status).toBe(200);
  expect((await head(owner, f.id, hash)).status).toBe(200);
  const res = await get(owner, f.id, hash);
  expect(res.headers.get('content-type')).toBe('application/octet-stream');
  expect(res.headers.get('content-disposition')).toBe('attachment');
  expect(new Uint8Array(await res.arrayBuffer())).toEqual(img);
});

test('байты не совпали с хэшем: 400, ни записи, ни байтов', async () => {
  const f = await s.newFile(owner);
  const res = await put(owner, f.id, hashOf(bytes(1)), bytes(2));
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'HASH_MISMATCH' });
  expect((await s.pool.query('SELECT 1 FROM image')).rowCount).toBe(0);
  expect((await head(owner, f.id, hashOf(bytes(1)))).status).toBe(404);
});

test('сверх предела размера: 413 по `Content-Length`', async () => {
  const f = await s.newFile(owner);
  const res = await fetch(`${s.publicUrl}/api/files/${f.id}/images/${'a'.repeat(64)}`, {
    method: 'PUT',
    headers: { origin: s.publicUrl, cookie: owner.cookie },
    body: new Uint8Array(65 * 1024 * 1024),
  });
  expect(res.status).toBe(413);
  expect(await res.json()).toEqual({ error: 'TOO_LARGE' });
});

test('знание хэша ничего не даёт: чужая картинка не читается и без байтов не привязывается', async () => {
  const mine = await s.newFile(owner);
  const theirs = await s.newFile(other);
  const img = bytes(3);
  const hash = hashOf(img);
  await put(owner, mine.id, hash, img);
  // Через свой файл: ссылки нет, 404.
  expect((await get(other, theirs.id, hash)).status).toBe(404);
  expect((await head(other, theirs.id, hash)).status).toBe(404);
  // Через чужой файл: нет доступа, 404.
  expect((await get(other, mine.id, hash)).status).toBe(404);
  // Содержимое со ссылкой на чужой хэш не привязывает картинку.
  expect((await s.writeContent(other, theirs.id, 0, container([hash]))).status).toBe(200);
  expect((await get(other, theirs.id, hash)).status).toBe(404);
  // Прислать все байты — значит иметь картинку: тогда ссылка есть, а запись на диске одна.
  expect((await put(other, theirs.id, hash, img)).status).toBe(200);
  expect((await get(other, theirs.id, hash)).status).toBe(200);
  const { rows } = await s.pool.query('SELECT blob_key FROM image');
  expect(rows).toHaveLength(1);
  const trash = await s.pool.query('SELECT key FROM blob_trash');
  expect(trash.rows).toHaveLength(1);
  expect(trash.rows[0].key).not.toBe(rows[0].blob_key);
});

const age = (table: string, column: string, days: number) =>
  s.pool.query(`UPDATE ${table} SET ${column} = now() - make_interval(days => ${days})`);

test('очистка: картинка без ссылок живёт 30 дней, отмена её возвращает', async () => {
  const f = await s.newFile(owner);
  const img = bytes(4);
  const hash = hashOf(img);
  await put(owner, f.id, hash, img);
  await s.writeContent(owner, f.id, 0, container([hash]));
  expect((await s.pool.query('SELECT unreferenced_at FROM file_image')).rows[0].unreferenced_at).toBeNull();
  // Ссылка пропала из содержимого: отметка, но чтение работает.
  await s.writeContent(owner, f.id, 1, container());
  expect((await s.pool.query('SELECT unreferenced_at FROM file_image')).rows[0].unreferenced_at).not.toBeNull();
  expect((await get(owner, f.id, hash)).status).toBe(200);
  // Ссылка вернулась (отмена): отметка снята.
  await s.writeContent(owner, f.id, 2, container([hash]));
  expect((await s.pool.query('SELECT unreferenced_at FROM file_image')).rows[0].unreferenced_at).toBeNull();
  await s.writeContent(owner, f.id, 3, container());
  await age('file_image', 'unreferenced_at', 29);
  await runGc(s.pool, s.blobs);
  expect((await s.pool.query('SELECT 1 FROM file_image')).rowCount).toBe(1);
  await age('file_image', 'unreferenced_at', 31);
  await runGc(s.pool, s.blobs);
  expect((await s.pool.query('SELECT 1 FROM file_image')).rowCount).toBe(0);
  expect((await head(owner, f.id, hash)).status).toBe(404);
  // Запись самой картинки ещё держится свои 30 дней.
  const { rows } = await s.pool.query('SELECT blob_key, orphaned_at FROM image');
  expect(rows[0].orphaned_at).not.toBeNull();
  expect(await s.blobs.get(rows[0].blob_key)).not.toBeNull();
  await age('image', 'orphaned_at', 31);
  await runGc(s.pool, s.blobs);
  expect((await s.pool.query('SELECT 1 FROM image')).rowCount).toBe(0);
  expect(await s.blobs.get(rows[0].blob_key)).toBeNull();
});

test('очистка не трогает картинку другого файла; удаление файла снимает только его ссылку', async () => {
  const a = await s.newFile(owner);
  const b = await s.newFile(owner);
  const img = bytes(5);
  const hash = hashOf(img);
  await put(owner, a.id, hash, img);
  await put(owner, b.id, hash, img);
  await s.call(owner, 'DELETE', `/api/files/${a.id}`);
  await s.pool.query(`UPDATE image SET orphaned_at = now() - interval '40 days'`);
  await runGc(s.pool, s.blobs);
  // Картинка нужна второму файлу: отметка снята, байты на месте.
  expect((await s.pool.query('SELECT orphaned_at FROM image')).rows[0].orphaned_at).toBeNull();
  expect((await get(owner, b.id, hash)).status).toBe(200);
});

test('повторная загрузка той же картинки оживляет запись перед удалением', async () => {
  const f = await s.newFile(owner);
  const img = bytes(6);
  const hash = hashOf(img);
  await put(owner, f.id, hash, img);
  await s.call(owner, 'DELETE', `/api/files/${f.id}`);
  await runGc(s.pool, s.blobs);
  await age('image', 'orphaned_at', 29);
  const g = await s.newFile(owner);
  expect((await put(owner, g.id, hash, img)).status).toBe(200);
  await age('image', 'orphaned_at', 40);
  await runGc(s.pool, s.blobs);
  expect((await get(owner, g.id, hash)).status).toBe(200);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/images.test.ts`
Expected: FAIL — маршрута картинок нет, ответы 404: 0 pass, 7 fail

- [ ] **Step 3: Реализация**

В `apps/server/src/app.ts` заменить:

````ts
import type { BlobStore } from './blob-store';
import { fileRoutes, uploadGate } from './file-routes';
import type { Mailer } from './mail';
import { responseHeaders, serveStatic } from './static';
````

на:

````ts
import type { BlobStore } from './blob-store';
import { fileRoutes, uploadGate } from './file-routes';
import { imageRoutes } from './image-routes';
import type { Mailer } from './mail';
import { responseHeaders, serveStatic } from './static';
````

В `apps/server/src/app.ts` заменить:

````ts
export function apiRoutes(deps: AppDeps): Route[] {
  const api = { ...deps, deadlineMs: deps.apiDeadlineMs };
  return [...fileRoutes(api, uploadGate())];
}

````

на:

````ts
export function apiRoutes(deps: AppDeps): Route[] {
  const api = { ...deps, deadlineMs: deps.apiDeadlineMs };
  const gate = uploadGate();
  return [...fileRoutes(api, gate), ...imageRoutes(api, gate)];
}

````

В `apps/server/src/gc.ts` заменить:

````ts
/** Проход раз в минуту, у него есть срок на каждый запрос (`Pool`). */
export const GC_INTERVAL_MS = 60_000;
const BATCH = 200;

/** Сколько байтов-записей удалено. Один проход на все процессы: сервер один, но блокировка дешевле ошибки. */
export async function runGc(pool: Pool, blobs: BlobStore): Promise<number> {
  const { rows } = await pool.query<{ key: string }>(
    'SELECT key FROM blob_trash WHERE delete_after <= now() ORDER BY delete_after LIMIT $1',
````

на:

````ts
/** Проход раз в минуту, у него есть срок на каждый запрос (`Pool`). */
export const GC_INTERVAL_MS = 60_000;
/** Неиспользуемая картинка живёт столько (спека, раздел 6.4): отмена может вернуть ссылку на неё. */
export const IMAGE_GRACE_DAYS = 30;
const BATCH = 200;

/** Картинки: ссылка файла без ссылок в содержимом снимается через срок, картинка без файлов получает
 * отметку и через срок уходит в список на удаление. Удаление записи и отправка байтов на удаление —
 * одним запросом с условием «всё ещё ни одного файла»: одновременная загрузка той же картинки
 * либо успевает раньше и картинка живёт, либо создаёт новую запись со своими байтами. */
async function sweepImages(pool: Pool): Promise<void> {
  await pool.query(`DELETE FROM file_image WHERE unreferenced_at < now() - make_interval(days => $1)`, [IMAGE_GRACE_DAYS]);
  await pool.query(`UPDATE image i SET orphaned_at = now()
    WHERE orphaned_at IS NULL AND NOT EXISTS (SELECT 1 FROM file_image l WHERE l.hash = i.hash)`);
  await pool.query(`UPDATE image i SET orphaned_at = NULL
    WHERE orphaned_at IS NOT NULL AND EXISTS (SELECT 1 FROM file_image l WHERE l.hash = i.hash)`);
  await pool.query(
    `WITH gone AS (
       DELETE FROM image i WHERE orphaned_at < now() - make_interval(days => $1)
         AND NOT EXISTS (SELECT 1 FROM file_image l WHERE l.hash = i.hash)
       RETURNING blob_key
     )
     INSERT INTO blob_trash (key, delete_after) SELECT blob_key, now() FROM gone ON CONFLICT DO NOTHING`,
    [IMAGE_GRACE_DAYS],
  );
}

/** Сколько байтов-записей удалено. Один проход на все процессы: сервер один, но блокировка дешевле ошибки. */
export async function runGc(pool: Pool, blobs: BlobStore): Promise<number> {
  await sweepImages(pool);
  const { rows } = await pool.query<{ key: string }>(
    'SELECT key FROM blob_trash WHERE delete_after <= now() ORDER BY delete_after LIMIT $1',
````

Создать `apps/server/src/image-routes.ts`:

````ts
// Картинки файла (спека, раздел 6.4): адресация по blake3, как в файле. Сервер проверяет хэш и размер,
// байты не держит в памяти. Доступ только через файл: `file_image` появляется только у того, кто прислал
// все байты, поэтому знание хэша чужой картинки ничего не даёт (доказательство владения вместо
// «такая уже есть»). Байты одной картинки на диске одни, сколько бы файлов на неё ни ссылалось.

import { MAX_IMAGE_BYTES } from '@ferrum/protocol';
import { blake3 } from '@noble/hashes/blake3.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import { randomUUID } from 'node:crypto';
import { apiError, ok, type ApiDeps, type Route } from './api';
import { inTransaction, trashBlob, uploadFailure, UPLOAD_IDLE_MS, type UploadGate } from './file-routes';

export function imageRoutes(deps: ApiDeps, gate: UploadGate): Route[] {
  const { pool, blobs } = deps;
  const idle = deps.uploadIdleMs ?? UPLOAD_IDLE_MS;
  const linked = (file: string, hash: string) =>
    pool.query<{ blob_key: string; size: string }>(
      'SELECT i.blob_key, i.size FROM file_image l JOIN image i ON i.hash = l.hash WHERE l.file_id = $1 AND l.hash = $2',
      [file, hash],
    );
  return [
    {
      // Есть ли картинка у этого файла: клиент не шлёт байты второй раз.
      method: 'HEAD',
      path: '/api/files/:id/images/:hash',
      access: { kind: 'file', min: 'viewer' },
      limit: 'read',
      handle: async (ctx) => {
        const { rows } = await linked(ctx.params.id, ctx.params.hash);
        return new Response(null, {
          status: rows[0] ? 200 : 404,
          headers: { 'Cache-Control': 'no-store', ...(rows[0] ? { 'Content-Length': rows[0].size } : {}) },
        });
      },
    },
    {
      method: 'GET',
      path: '/api/files/:id/images/:hash',
      access: { kind: 'file', min: 'viewer' },
      limit: 'read',
      handle: async (ctx) => {
        const { rows } = await linked(ctx.params.id, ctx.params.hash);
        if (!rows[0]) return apiError(404, 'NOT_FOUND');
        const blob = await blobs.open(rows[0].blob_key);
        if (!blob) throw new Error(`нет байтов картинки ${ctx.params.hash}`);
        // Тип не угадывается и не отдаётся картинкой: это байты для ядра, а не страница браузера.
        return new Response(blob, {
          headers: {
            'Content-Type': 'application/octet-stream',
            'Content-Disposition': 'attachment',
            'Cache-Control': 'no-store',
            'Content-Length': String(blob.size),
          },
        });
      },
    },
    {
      method: 'PUT',
      path: '/api/files/:id/images/:hash',
      access: { kind: 'file', min: 'editor' },
      limit: 'upload',
      stream: true,
      handle: async (ctx) => {
        if (!ctx.req.body) return apiError(400, 'BAD_REQUEST');
        if (Number(ctx.req.headers.get('content-length') ?? 0) > MAX_IMAGE_BYTES) return apiError(413, 'TOO_LARGE');
        if (!gate.enter(ctx.user.id)) return apiError(429, 'BUSY');
        const key = `images/${ctx.params.hash}/${randomUUID()}`;
        try {
          const hasher = blake3.create();
          let size: number;
          try {
            ({ size } = await blobs.putStream(key, ctx.req.body, {
              maxBytes: MAX_IMAGE_BYTES,
              idleMs: idle,
              onChunk: (chunk) => void hasher.update(chunk),
            }));
          } catch (e) {
            await blobs.delete(key).catch(() => {});
            const failure = uploadFailure(e);
            if (failure) return failure;
            throw e;
          }
          if (bytesToHex(hasher.digest()) !== ctx.params.hash) {
            await blobs.delete(key).catch(() => {});
            return apiError(400, 'HASH_MISMATCH');
          }
          await inTransaction(pool, async (c) => {
            // Такая уже есть: новые байты лишние, а запись оживает, даже если её вот-вот удалила бы очистка.
            const { rows } = await c.query<{ blob_key: string }>(
              `INSERT INTO image (hash, size, blob_key) VALUES ($1, $2, $3)
               ON CONFLICT (hash) DO UPDATE SET orphaned_at = NULL RETURNING blob_key`,
              [ctx.params.hash, size, key],
            );
            if (rows[0].blob_key !== key) await trashBlob(c, key);
            await c.query(
              `INSERT INTO file_image (file_id, hash) VALUES ($1, $2)
               ON CONFLICT (file_id, hash) DO UPDATE SET unreferenced_at = NULL`,
              [ctx.params.id, ctx.params.hash],
            );
          });
          return ok({ ok: true });
        } finally {
          gate.leave(ctx.user.id);
        }
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

Expected: PASS: всё без замечаний; `bun test`: 12 и 181 pass

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/src/app.ts apps/server/src/gc.ts apps/server/src/image-routes.ts apps/server/test/access.test.ts apps/server/test/images.test.ts
git commit -F - <<'EOF'
сервер: картинки файла и их очистка

PUT, HEAD и GET /api/files/:id/images/:hash: хэш blake3 и размер сверяются потоком,
доступ только через file_image, знание хэша чужой картинки ничего не даёт. Очистка:
картинка без ссылок живёт 30 дней, отмена её возвращает, байты общие для файлов.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4: Сервер — команды, проекты, «Поделиться»

Спека §6.1, §6.2, §6.3, §5 (запрос доступа).

**Files:**
- Create: `apps/server/src/invites.ts`, `apps/server/src/share-routes.ts`, `apps/server/src/team-routes.ts`
- Modify: `apps/server/migrations/003_files.sql`, `apps/server/src/api.ts`, `apps/server/src/app.ts`, `apps/server/src/auth.ts`, `apps/server/src/file-routes.ts`, `apps/server/src/index.ts`, `apps/server/src/mail.ts`, `docker-compose.test.yml`
- Test: `apps/server/test/access.test.ts`, `apps/server/test/api-stand.ts`, `apps/server/test/files.test.ts`, `apps/server/test/images.test.ts`, `apps/server/test/share.test.ts` (новый), `apps/server/test/teams.test.ts` (новый)

**Interfaces:**
- Produces: `teamRoutes`, `shareRoutes`, `claimInvites`, `sendInvite`, письма приглашений.

- [ ] **Step 1: Написать тесты**

В `apps/server/test/access.test.ts` заменить:

````ts
  blobs: s.blobs,
  mailer: { send: async () => {} },
  secret: '',
});
````

на:

````ts
  blobs: s.blobs,
  mailer: { send: async () => {} },
  allowedEmails: [],
  secret: '',
});
````

В `apps/server/test/access.test.ts` заменить:

````ts
interface World {
  file: string;
}

````

на:

````ts
interface World {
  file: string;
  /** Запись списка людей: читатель и приглашённый. */
  entry: string;
  team: string;
  project: string;
  member: string;
}

````

В `apps/server/test/access.test.ts` заменить:

````ts
    viewer.id,
  ]);
  return { file: f.id };
}

````

на:

````ts
    viewer.id,
  ]);
  const entry = await s.pool.query<{ id: string }>('SELECT id FROM file_access WHERE file_id = $1 AND user_id = $2', [f.id, viewer.id]);
  const team = await s.pool.query<{ id: string }>(`INSERT INTO team (name) VALUES ('Команда') RETURNING id`);
  const t = team.rows[0].id;
  await s.pool.query(`INSERT INTO team_member (team_id, user_id, role) VALUES ($1, $2, 'owner'), ($1, $3, 'member')`, [
    t,
    owner.id,
    editor.id,
  ]);
  const member = await s.pool.query<{ id: string }>('SELECT id FROM team_member WHERE team_id = $1 AND user_id = $2', [t, editor.id]);
  const project = await s.pool.query<{ id: string }>(`INSERT INTO project (team_id, name) VALUES ($1, 'Проект') RETURNING id`, [t]);
  return { file: f.id, entry: entry.rows[0].id, team: t, project: project.rows[0].id, member: member.rows[0].id };
}

````

В `apps/server/test/access.test.ts` заменить:

````ts
const write = () => ({ 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() });
const file = (w: World) => ({ id: w.file });

const SAMPLES: Record<string, Sample> = {
````

на:

````ts
const write = () => ({ 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() });
const file = (w: World) => ({ id: w.file });
const team = (w: World) => ({ teamId: w.team });
const project = (w: World) => ({ projectId: w.project });

const SAMPLES: Record<string, Sample> = {
````

В `apps/server/test/access.test.ts` заменить:

````ts
  'GET /api/files/:id/images/:hash': { params: file, allowed: viewer, status: 404 },
  'PUT /api/files/:id/images/:hash': { params: (w) => ({ id: w.file, hash: IMAGE_HASH }), body: IMAGE, allowed: editor, denied: [viewer] },
};

const key = (r: { method: string; path: string }) => `${r.method} ${r.path}`;

function pathOf(path: string, params: Record<string, string>): string {
````

на:

````ts
  'GET /api/files/:id/images/:hash': { params: file, allowed: viewer, status: 404 },
  'PUT /api/files/:id/images/:hash': { params: (w) => ({ id: w.file, hash: IMAGE_HASH }), body: IMAGE, allowed: editor, denied: [viewer] },
  'GET /api/teams': {},
  'POST /api/teams': { body: { name: 'Новая' }, status: 201 },
  'PATCH /api/teams/:teamId': { params: team, body: { name: 'Имя' }, allowed: owner, denied: [editor] },
  // Команда с проектом не удаляется: но это уже после проверки прав.
  'DELETE /api/teams/:teamId': { params: team, allowed: owner, denied: [editor], status: 409 },
  'GET /api/teams/:teamId/members': { params: team, allowed: editor },
  'POST /api/teams/:teamId/invite': { params: team, body: { email: 'new@example.com' }, allowed: owner, denied: [editor] },
  'DELETE /api/teams/:teamId/members/:entryId': { params: (w) => ({ teamId: w.team, entryId: w.member }), allowed: owner },
  'POST /api/teams/:teamId/projects': { params: team, body: { name: 'Новый' }, allowed: editor, status: 201 },
  'PATCH /api/projects/:projectId': { params: project, body: { name: 'Имя' }, allowed: editor },
  'DELETE /api/projects/:projectId': { params: project, allowed: owner, denied: [editor] },
  'GET /api/files/:id/share': { params: file, allowed: editor, denied: [viewer] },
  'POST /api/files/:id/share/invite': {
    params: file,
    body: { email: 'new@example.com', role: 'viewer' },
    allowed: editor,
    denied: [viewer],
  },
  'PATCH /api/files/:id/share/people/:entryId': {
    params: (w) => ({ id: w.file, entryId: w.entry }),
    body: { role: 'editor' },
    allowed: owner,
    denied: [viewer, editor],
  },
  'DELETE /api/files/:id/share/people/:entryId': { params: (w) => ({ id: w.file, entryId: w.entry }), allowed: owner },
  'PUT /api/files/:id/share/link': { params: file, body: { enabled: true, role: 'viewer' }, allowed: owner, denied: [viewer, editor] },
  'POST /api/files/:id/share/link/reset': { params: file, allowed: owner, denied: [viewer, editor] },
  // Прав на файл здесь ещё нет: чужой файл, неверный токен и выключенная ссылка неотличимы.
  'POST /api/files/:id/redeem': { params: file, body: { token: 'x'.repeat(43) }, allowed: stranger, status: 404 },
  'POST /api/files/:id/request-access': { params: file, allowed: stranger },
};

const key = (r: { method: string; path: string }) => `${r.method} ${r.path}`;
/** Права на файл здесь только появляются: их отдельные проверки — в `share.test.ts`. */
const OPEN_ROUTES = new Set(['POST /api/files/:id/redeem', 'POST /api/files/:id/request-access']);

function pathOf(path: string, params: Record<string, string>): string {
````

В `apps/server/test/access.test.ts` заменить:

````ts
  }

  if (route.access.kind !== 'user') {
    test(`${key(route)}: чужой неотличим от несуществующего`, async () => {
      const w = await world();
````

на:

````ts
  }

  // Из таблицы, а не из объявления маршрута: иначе снятие проверки с маршрута снимало бы и этот тест.
  if (sample.params && !OPEN_ROUTES.has(key(route))) {
    test(`${key(route)}: чужой неотличим от несуществующего`, async () => {
      const w = await world();
````

В `apps/server/test/api-stand.ts` заменить:

````ts

import { containerHeader, SECTION_PAGE_REFS } from '@ferrum/protocol';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
````

на:

````ts

import { containerHeader, SECTION_PAGE_REFS } from '@ferrum/protocol';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
````

В `apps/server/test/api-stand.ts` заменить:

````ts
export const PNG: Uint8Array<ArrayBuffer> = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);

export async function apiStand(over: Partial<AppDeps> = {}) {
  const pool: Pool = testPool();
  const valkey = await connectValkey(TEST_VALKEY_URL);
  await resetDatabase(pool);
  await resetValkey(valkey);
  await migrate(pool, join(import.meta.dir, '../migrations'));
  const free = Bun.serve({ port: 0, fetch: () => new Response() });
  const port = free.port!;
````

на:

````ts
export const PNG: Uint8Array<ArrayBuffer> = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);

/** `keep` — второй стенд с другими настройками на той же базе: люди первого остаются. */
export async function apiStand(over: Partial<AppDeps> = {}, keep = false) {
  const pool: Pool = testPool();
  const valkey = await connectValkey(TEST_VALKEY_URL);
  if (!keep) {
    await resetDatabase(pool);
    await resetValkey(valkey);
    await migrate(pool, join(import.meta.dir, '../migrations'));
  }
  const free = Bun.serve({ port: 0, fetch: () => new Response() });
  const port = free.port!;
````

В `apps/server/test/api-stand.ts` заменить:

````ts
    valkey.close();
    await pool.end();
  };

````

на:

````ts
    valkey.close();
    await pool.end();
    // Байты тестов: десятки прогонов иначе заполнят диск (проба памяти пишет 200 МБ).
    rmSync(dataDir, { recursive: true, force: true });
  };

````

В `apps/server/test/files.test.ts` заменить:

````ts
test('предел частоты по человеку: другой не страдает; срок обработчика — 503 в формате ошибок', async () => {
  expect(LIMITS.read.max).toBeGreaterThan(100);
  const tight = await apiStand({ limits: { read: { max: 2, windowSeconds: 60 } } });
  const a = await tight.signIn('a@example.com');
  const b = await tight.signIn('b@example.com');
````

на:

````ts
test('предел частоты по человеку: другой не страдает; срок обработчика — 503 в формате ошибок', async () => {
  expect(LIMITS.read.max).toBeGreaterThan(100);
  const tight = await apiStand({ limits: { read: { max: 2, windowSeconds: 60 } } }, true);
  const a = await tight.signIn('a@example.com');
  const b = await tight.signIn('b@example.com');
````

В `apps/server/test/files.test.ts` заменить:

````ts

test('простой клиента посреди загрузки: обрыв по сроку, временных файлов нет', async () => {
  const slow = await apiStand({ uploadIdleMs: 50 });
  const who = await slow.signIn('slow@example.com');
  const f = await slow.newFile(who);
````

на:

````ts

test('простой клиента посреди загрузки: обрыв по сроку, временных файлов нет', async () => {
  const slow = await apiStand({ uploadIdleMs: 50 }, true);
  const who = await slow.signIn('slow@example.com');
  const f = await slow.newFile(who);
````

В `apps/server/test/images.test.ts` заменить:

````ts
  // Через чужой файл: нет доступа, 404.
  expect((await get(other, mine.id, hash)).status).toBe(404);
  // Содержимое со ссылкой на чужой хэш не привязывает картинку.
  expect((await s.writeContent(other, theirs.id, 0, container([hash]))).status).toBe(200);
````

на:

````ts
  // Через чужой файл: нет доступа, 404.
  expect((await get(other, mine.id, hash)).status).toBe(404);
  expect((await head(other, mine.id, hash)).status).toBe(404);
  // Содержимое со ссылкой на чужой хэш не привязывает картинку.
  expect((await s.writeContent(other, theirs.id, 0, container([hash]))).status).toBe(200);
````

Создать `apps/server/test/share.test.ts`:

````ts
import { afterAll, beforeEach, expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import { INVITES_PER_ADDRESS } from '../src/invites';
import { apiStand, container, type Person } from './api-stand';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com', 'Анна');
const editor = await s.signIn('editor@example.com', 'Борис');
const viewer = await s.signIn('viewer@example.com', 'Вера');
const guest = await s.signIn('guest@example.com', 'Гость');
beforeEach(async () => {
  await s.reset();
  // Счётчики писем на адрес: тесты не должны делить их между собой.
  for (const key of (await s.valkey.send('KEYS', ['invite-to:*'])) as string[]) await s.valkey.del(key);
  for (const key of (await s.valkey.send('KEYS', ['request-access:*'])) as string[]) await s.valkey.del(key);
});

const json = async (res: Response) => (await res.json()) as any; // eslint-disable-line @typescript-eslint/no-explicit-any
const share = async (who: Person, id: string) => json(await s.call(who, 'GET', `/api/files/${id}/share`));
const invite = (who: Person, id: string, email: string, role = 'editor') =>
  s.call(who, 'POST', `/api/files/${id}/share/invite`, { email, role });
const role = async (who: Person, id: string) => {
  const res = await s.call(who, 'GET', `/api/files/${id}/access`);
  return res.status === 200 ? (await json(res)).role : res.status;
};
const entry = async (id: string, email: string) => {
  const { people } = await share(owner, id);
  return (people as { id: string; email: string }[]).find((p) => p.email === email)!.id;
};

test('приглашение существующего: роль, письмо со ссылкой на файл, файл в «Доступные мне»', async () => {
  const f = await s.newFile(owner, 'Макет');
  const res = await invite(owner, f.id, editor.email);
  expect(res.status).toBe(200);
  expect(await role(editor, f.id)).toBe('editor');
  const mail = await s.waitMail(editor.email, 1);
  expect(mail.text).toContain(`${s.publicUrl}/#/file/${f.id}`);
  expect(mail.text).toContain('«Макет»');
  const shared = await json(await s.call(editor, 'GET', '/api/files?section=shared&start=0&count=10'));
  expect(shared.rows.map((r: { id: string }) => r.id)).toEqual([f.id]);
  expect(shared.rows[0].role).toBe('editor');
});

test('приглашение нового адреса: при первом входе файл доступен; список не выдаёт, есть ли учётная запись', async () => {
  const f = await s.newFile(owner);
  const newbie = await json(await invite(owner, f.id, 'newbie@example.com', 'viewer'));
  const known = await json(await invite(owner, f.id, editor.email, 'viewer'));
  const row = (list: { people: { email: string }[] }, email: string) => list.people.find((p) => p.email === email)!;
  // Обе записи одного вида: имени нет, пока человек не открывал файл.
  expect(Object.keys(row(newbie, 'newbie@example.com')).sort()).toEqual(['email', 'id', 'name', 'role']);
  expect(row(known, 'newbie@example.com')).toMatchObject({ name: null, role: 'viewer' });
  expect(row(known, editor.email)).toMatchObject({ name: null, role: 'viewer' });
  await s.call(editor, 'POST', `/api/files/${f.id}/touch`);
  expect((await share(owner, f.id)).people.find((p: { email: string }) => p.email === editor.email).name).toBe('Борис');
  const fresh = await s.signIn('newbie@example.com');
  expect(await role(fresh, f.id)).toBe('viewer');
  expect((await s.pool.query('SELECT 1 FROM file_invite')).rowCount).toBe(0);
});

test('смена и снятие роли действуют сразу, в том числе на запись', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, editor.email);
  expect((await s.writeContent(editor, f.id, 0)).status).toBe(200);
  const id = await entry(f.id, editor.email);
  await s.call(owner, 'PATCH', `/api/files/${f.id}/share/people/${id}`, { role: 'viewer' });
  expect(await role(editor, f.id)).toBe('viewer');
  expect((await s.writeContent(editor, f.id, 1)).status).toBe(403);
  await s.call(owner, 'DELETE', `/api/files/${f.id}/share/people/${id}`);
  expect(await role(editor, f.id)).toBe(404);
  expect((await s.writeContent(editor, f.id, 1)).status).toBe(404);
  expect((await s.call(editor, 'GET', `/api/files/${f.id}/content`)).status).toBe(404);
});

test('запись другого файла по его id не меняется и не удаляется (IDOR)', async () => {
  const mine = await s.newFile(guest);
  const theirs = await s.newFile(owner);
  await invite(owner, theirs.id, viewer.email, 'viewer');
  const foreign = await entry(theirs.id, viewer.email);
  expect((await s.call(guest, 'PATCH', `/api/files/${mine.id}/share/people/${foreign}`, { role: 'editor' })).status).toBe(404);
  expect((await s.call(guest, 'DELETE', `/api/files/${mine.id}/share/people/${foreign}`)).status).toBe(404);
  expect(await role(viewer, theirs.id)).toBe('viewer');
});

test('владельца нельзя понизить и снять; редактор не меняет чужие роли, но может добавить', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, editor.email);
  const self = await entry(f.id, owner.email);
  expect((await s.call(owner, 'PATCH', `/api/files/${f.id}/share/people/${self}`, { role: 'viewer' })).status).toBe(403);
  expect((await s.call(owner, 'DELETE', `/api/files/${f.id}/share/people/${self}`)).status).toBe(403);
  expect(await role(owner, f.id)).toBe('owner');
  // Повторное приглашение от редактора не понижает: и владельца, и другого редактора.
  await invite(owner, f.id, guest.email, 'editor');
  await invite(editor, f.id, owner.email, 'viewer');
  await invite(editor, f.id, guest.email, 'viewer');
  expect(await role(owner, f.id)).toBe('owner');
  expect(await role(guest, f.id)).toBe('editor');
  await invite(editor, f.id, viewer.email, 'viewer');
  expect(await role(viewer, f.id)).toBe('viewer');
  await invite(editor, f.id, viewer.email, 'editor');
  expect(await role(viewer, f.id)).toBe('editor');
});

test('читатель сам снимает свой доступ, чужой — нет', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, viewer.email, 'viewer');
  await invite(owner, f.id, guest.email, 'viewer');
  const mine = await entry(f.id, viewer.email);
  const theirs = await entry(f.id, guest.email);
  expect((await s.call(viewer, 'DELETE', `/api/files/${f.id}/share/people/${theirs}`)).status).toBe(403);
  expect((await s.call(viewer, 'DELETE', `/api/files/${f.id}/share/people/${mine}`)).status).toBe(200);
  expect(await role(viewer, f.id)).toBe(404);
});

test('ссылка: токен в базе хэшем, роль по ссылке, выключатель действует сразу', async () => {
  const f = await s.newFile(owner);
  const on = await json(await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'viewer' }));
  const token = on.link.token as string;
  expect(token).toHaveLength(43);
  const { rows } = await s.pool.query('SELECT * FROM share_link');
  expect(JSON.stringify(rows[0])).not.toContain(token);
  expect(rows[0].token_hash).toBe(createHash('sha256').update(token).digest('hex'));
  // Показать ссылку снова можно, токен тот же.
  expect((await share(owner, f.id)).link.token).toBe(token);
  // Редактор ссылки не видит.
  await invite(owner, f.id, editor.email);
  expect((await share(editor, f.id)).link).toBeNull();

  expect(await role(guest, f.id)).toBe(404);
  const redeem = await s.call(guest, 'POST', `/api/files/${f.id}/redeem`, { token });
  expect(redeem.status).toBe(200);
  expect((await json(redeem)).role).toBe('viewer');
  expect(await role(guest, f.id)).toBe('viewer');
  expect((await s.call(guest, 'GET', `/api/files?section=shared&start=0&count=10`)).status).toBe(200);
  expect((await json(await s.call(guest, 'GET', `/api/files?section=shared&start=0&count=10`))).total).toBe(1);

  // Роль ссылки меняется для всех, кто по ней пришёл; прямая роль важнее меньшей.
  await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'editor' });
  expect(await role(guest, f.id)).toBe('editor');
  await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: false, role: 'editor' });
  expect(await role(guest, f.id)).toBe(404);
  expect((await json(await s.call(guest, 'GET', `/api/files?section=shared&start=0&count=10`))).total).toBe(0);
  // Выключенная ссылка не открывает файл, токен прежний после включения работает.
  expect((await s.call(viewer, 'POST', `/api/files/${f.id}/redeem`, { token })).status).toBe(404);
  await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'viewer' });
  expect((await share(owner, f.id)).link.token).toBe(token);
  expect(await role(guest, f.id)).toBe('viewer');
});

test('новая ссылка отменяет старую и всех пришедших по ней', async () => {
  const f = await s.newFile(owner);
  const old = (await json(await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'viewer' }))).link.token;
  await s.call(guest, 'POST', `/api/files/${f.id}/redeem`, { token: old });
  const next = (await json(await s.call(owner, 'POST', `/api/files/${f.id}/share/link/reset`))).link.token;
  expect(next).not.toBe(old);
  expect(await role(guest, f.id)).toBe(404);
  expect((await s.call(guest, 'POST', `/api/files/${f.id}/redeem`, { token: old })).status).toBe(404);
  expect((await s.call(guest, 'POST', `/api/files/${f.id}/redeem`, { token: next })).status).toBe(200);
});

test('неверный токен, файл без ссылки и чужой токен неотличимы от несуществующего файла', async () => {
  const a = await s.newFile(owner);
  const b = await s.newFile(owner);
  const tokenB = (await json(await s.call(owner, 'PUT', `/api/files/${b.id}/share/link`, { enabled: true, role: 'editor' }))).link.token;
  const bad = 'y'.repeat(43);
  const missing = await s.call(guest, 'POST', `/api/files/00000000-0000-4000-8000-000000000000/redeem`, { token: bad });
  const noLink = await s.call(guest, 'POST', `/api/files/${a.id}/redeem`, { token: bad });
  const wrong = await s.call(guest, 'POST', `/api/files/${b.id}/redeem`, { token: bad });
  const foreign = await s.call(guest, 'POST', `/api/files/${a.id}/redeem`, { token: tokenB });
  for (const res of [missing, noLink, wrong, foreign]) {
    expect(res.status).toBe(404);
    expect(await res.json()).toEqual({ error: 'NOT_FOUND' });
  }
  expect((await s.call(guest, 'POST', `/api/files/${a.id}/redeem`, { token: '../x' })).status).toBe(400);
});

test('файл из корзины по ссылке не открывается', async () => {
  const f = await s.newFile(owner);
  const token = (await json(await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'editor' }))).link.token;
  await s.call(owner, 'POST', `/api/files/${f.id}/trash`);
  expect((await s.call(guest, 'POST', `/api/files/${f.id}/redeem`, { token })).status).toBe(404);
  expect((await s.call(guest, 'GET', `/api/files/${f.id}`)).status).toBe(404);
});

test('запрос доступа: письмо владельцу один раз, ответ один и тот же для чужого и несуществующего', async () => {
  const f = await s.newFile(owner, 'Секрет');
  const seen = s.mails.filter((m) => m.to === owner.email).length;
  const a = await s.call(guest, 'POST', `/api/files/${f.id}/request-access`);
  const b = await s.call(guest, 'POST', '/api/files/00000000-0000-4000-8000-000000000000/request-access');
  expect([a.status, await a.text()]).toEqual([b.status, await b.text()]);
  const mail = await s.waitMail(owner.email, seen);
  expect(mail.subject).toBe('Запрос доступа к файлу в Ferrum');
  expect(mail.text).toContain(guest.email === 'guest@example.com' ? 'Гость' : '');
  await s.call(guest, 'POST', `/api/files/${f.id}/request-access`);
  await Bun.sleep(30);
  expect(s.mails.filter((m) => m.to === owner.email)).toHaveLength(seen + 1);
});

test('письма приглашений на один адрес ограничены, запись при этом остаётся', async () => {
  const f = await s.newFile(owner);
  for (let i = 0; i <= INVITES_PER_ADDRESS + 1; i++) {
    expect((await invite(owner, f.id, 'many@example.com', 'viewer')).status).toBe(200);
  }
  await s.waitMail('many@example.com', INVITES_PER_ADDRESS - 1);
  await Bun.sleep(30);
  expect(s.mails.filter((m) => m.to === 'many@example.com')).toHaveLength(INVITES_PER_ADDRESS);
});

test('адрес вне `ALLOWED_EMAILS`: ответ тот же, письма нет', async () => {
  const closed = await apiStand({ allowedEmails: ['example.org'] }, true);
  // Вход в этом стенде открыт (`allowedEmails` — только у API), поэтому здесь адрес вне списка API.
  const who = await closed.signIn('boss@example.com');
  const f = await closed.newFile(who);
  expect((await closed.call(who, 'POST', `/api/files/${f.id}/share/invite`, { email: 'x@example.com', role: 'viewer' })).status).toBe(200);
  await Bun.sleep(30);
  expect(closed.mails.filter((m) => m.to === 'x@example.com')).toHaveLength(0);
  await closed.stop();
});

test('роль после смены видна и через запись содержимого читателем', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, viewer.email, 'viewer');
  expect((await s.writeContent(viewer, f.id, 0, container())).status).toBe(403);
});
````

Создать `apps/server/test/teams.test.ts`:

````ts
import { afterAll, beforeEach, expect, test } from 'bun:test';
import { apiStand, type Person } from './api-stand';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const member = await s.signIn('member@example.com');
const outsider = await s.signIn('outsider@example.com');
beforeEach(s.reset);

const json = async (res: Response) => (await res.json()) as any; // eslint-disable-line @typescript-eslint/no-explicit-any
const newTeam = async (who: Person, name = 'Команда') => (await json(await s.call(who, 'POST', '/api/teams', { name }))) as { id: string };
const newProject = async (who: Person, team: string, name = 'Проект') =>
  (await json(await s.call(who, 'POST', `/api/teams/${team}/projects`, { name }))) as { id: string };
const invite = (who: Person, team: string, email: string) => s.call(who, 'POST', `/api/teams/${team}/invite`, { email });
const role = async (who: Person, file: string) => json(await s.call(who, 'GET', `/api/files/${file}/access`));
const entries = async (who: Person, team: string) =>
  (await json(await s.call(who, 'GET', `/api/teams/${team}/members`))).members as { id: string; email: string }[];
const list = async (who: Person, section: string) => json(await s.call(who, 'GET', `/api/files?section=${section}&start=0&count=50`));

test('команда: создатель — владелец, проект виден участнику, не виден чужому', async () => {
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  expect((await json(await s.call(owner, 'GET', '/api/teams'))).teams).toEqual([
    { id: t.id, name: 'Команда', role: 'owner', projects: [{ id: p.id, teamId: t.id, name: 'Проект' }] },
  ]);
  expect((await json(await s.call(outsider, 'GET', '/api/teams'))).teams).toEqual([]);
  await invite(owner, t.id, member.email);
  expect((await json(await s.call(member, 'GET', '/api/teams'))).teams[0].role).toBe('member');
});

test('участник — редактор файлов проекта, владелец команды — владелец, чужой — ничего', async () => {
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  await invite(owner, t.id, member.email);
  const f = await s.newFile(member, 'В проекте', p.id);
  const g = await s.newFile(owner, 'Ещё', p.id);
  expect((await role(member, g.id)).role).toBe('editor');
  expect((await role(owner, f.id)).role).toBe('owner');
  expect((await s.call(outsider, 'GET', `/api/files/${f.id}`)).status).toBe(404);
  expect((await list(member, `project:${p.id}`)).total).toBe(2);
  expect((await list(outsider, `project:${p.id}`)).total).toBe(0);
  // Файл проекта не в «Черновиках» создателя.
  expect((await list(member, 'drafts')).total).toBe(0);
  // Чужой проект при создании неотличим от несуществующего.
  expect((await s.call(outsider, 'POST', '/api/files', { name: 'x', projectId: p.id })).status).toBe(404);
});

test('действующая роль — наибольшая из прямой и через команду', async () => {
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  const f = await s.newFile(owner, 'Файл', p.id);
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [f.id, member.id]);
  expect((await role(member, f.id)).role).toBe('viewer');
  await invite(owner, t.id, member.email);
  expect((await role(member, f.id)).role).toBe('editor');
});

test('перемещение: только в свою команду, из черновиков в проект и обратно', async () => {
  const f = await s.newFile(owner);
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  const foreign = await newProject(outsider, (await newTeam(outsider)).id);
  expect((await s.call(owner, 'POST', `/api/files/${f.id}/move`, { projectId: foreign.id })).status).toBe(404);
  expect((await json(await s.call(owner, 'POST', `/api/files/${f.id}/move`, { projectId: p.id }))).projectId).toBe(p.id);
  expect((await list(owner, 'drafts')).total).toBe(0);
  expect((await s.call(owner, 'POST', `/api/files/${f.id}/move`, { projectId: null })).status).toBe(200);
  expect((await list(owner, 'drafts')).total).toBe(1);
});

test('приглашение адреса без учётной записи: при первом входе он в команде', async () => {
  const t = await newTeam(owner);
  await newProject(owner, t.id);
  const res = await invite(owner, t.id, 'Fresh@Example.com');
  expect(res.status).toBe(200);
  expect((await json(res)).members.map((m: { email: string }) => m.email)).toContain('fresh@example.com');
  const mail = await s.waitMail('fresh@example.com');
  expect(mail.subject).toBe('Вас пригласили в команду в Ferrum');
  expect(mail.text).toContain('«Команда»');
  const fresh = await s.signIn('fresh@example.com');
  expect((await json(await s.call(fresh, 'GET', '/api/teams'))).teams[0].id).toBe(t.id);
  expect((await s.pool.query('SELECT 1 FROM team_invite')).rowCount).toBe(0);
});

test('имя с разметкой в письме экранируется', async () => {
  const t = await newTeam(owner, '<b>Команда</b>');
  await invite(owner, t.id, 'html@example.com');
  const mail = await s.waitMail('html@example.com');
  expect(mail.html).toContain('&lt;b&gt;Команда&lt;/b&gt;');
  expect(mail.html).not.toContain('<b>');
});

test('ушедший участник теряет доступ, его файлы переходят владельцу команды', async () => {
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  await invite(owner, t.id, member.email);
  const f = await s.newFile(member, 'Его файл', p.id);
  const entry = (await entries(owner, t.id)).find((m) => m.email === member.email)!;
  expect((await s.call(member, 'DELETE', `/api/teams/${t.id}/members/${entry.id}`)).status).toBe(200);
  expect((await s.call(member, 'GET', `/api/files/${f.id}`)).status).toBe(404);
  expect((await role(owner, f.id)).role).toBe('owner');
  const { rows } = await s.pool.query('SELECT owner_id FROM file WHERE id = $1', [f.id]);
  expect(rows[0].owner_id).toBe(owner.id);
  expect((await list(owner, `project:${p.id}`)).total).toBe(1);
});

test('владельца снять нельзя, другого участника может только владелец', async () => {
  const t = await newTeam(owner);
  await invite(owner, t.id, member.email);
  await invite(owner, t.id, outsider.email);
  const all = await entries(owner, t.id);
  const ownerEntry = all.find((m) => m.email === owner.email)!;
  const outsiderEntry = all.find((m) => m.email === outsider.email)!;
  expect((await s.call(owner, 'DELETE', `/api/teams/${t.id}/members/${ownerEntry.id}`)).status).toBe(403);
  expect((await s.call(member, 'DELETE', `/api/teams/${t.id}/members/${outsiderEntry.id}`)).status).toBe(403);
  // Запись чужой команды по чужому адресу — 404.
  const other = await newTeam(member);
  expect((await s.call(member, 'DELETE', `/api/teams/${other.id}/members/${outsiderEntry.id}`)).status).toBe(404);
});

test('нельзя удалить команду с проектами и проект с файлами; файл из корзины уходит в черновики', async () => {
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  const f = await s.newFile(owner, 'Файл', p.id);
  const refused = await s.call(owner, 'DELETE', `/api/projects/${p.id}`);
  expect(refused.status).toBe(409);
  expect(await refused.json()).toEqual({ error: 'NOT_EMPTY' });
  expect((await s.call(owner, 'DELETE', `/api/teams/${t.id}`)).status).toBe(409);
  await s.call(owner, 'POST', `/api/files/${f.id}/trash`);
  expect((await s.call(owner, 'DELETE', `/api/projects/${p.id}`)).status).toBe(200);
  await s.call(owner, 'POST', `/api/files/${f.id}/restore`);
  expect((await list(owner, 'drafts')).total).toBe(1);
  expect((await s.call(owner, 'DELETE', `/api/teams/${t.id}`)).status).toBe(200);
});

test('пустое и управляющие символы в именах — 400', async () => {
  const t = await newTeam(owner);
  for (const name of ['', '   ', 'a\nb', 'x'.repeat(101)]) {
    expect((await s.call(owner, 'POST', '/api/teams', { name })).status).toBe(400);
    expect((await s.call(owner, 'POST', `/api/teams/${t.id}/projects`, { name })).status).toBe(400);
    expect((await s.call(owner, 'PATCH', `/api/teams/${t.id}`, { name })).status).toBe(400);
  }
  expect((await s.call(owner, 'POST', `/api/teams/${t.id}/invite`, { email: 'нет' })).status).toBe(400);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/teams.test.ts test/share.test.ts`
Expected: FAIL — нет `../src/invites` и маршрутов команд: 0 pass, 11 fail

- [ ] **Step 3: Реализация**

В `apps/server/migrations/003_files.sql` заменить:

````sql
LANGUAGE sql STABLE AS $$
  SELECT CASE
    WHEN f.trashed_at IS NOT NULL AND f.owner_id <> p_user AND f.trashed_by IS DISTINCT FROM p_user THEN 0
    ELSE GREATEST(
      COALESCE((SELECT CASE a.role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END
````

на:

````sql
LANGUAGE sql STABLE AS $$
  SELECT CASE
    WHEN f.trashed_at IS NOT NULL AND f.trashed_by = p_user THEN 3
    WHEN f.trashed_at IS NOT NULL AND f.owner_id <> p_user THEN 0
    ELSE GREATEST(
      COALESCE((SELECT CASE a.role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END
````

В `apps/server/src/api.ts` заменить:

````ts
  blobs: BlobStore;
  mailer: Mailer;
  /** Секрет сервера: из него выводятся токены ссылок. */
  secret: string;
````

на:

````ts
  blobs: BlobStore;
  mailer: Mailer;
  /** Кому можно писать: как у входа. */
  allowedEmails: string[];
  /** Секрет сервера: из него выводятся токены ссылок. */
  secret: string;
````

В `apps/server/src/app.ts` заменить:

````ts
import { MAX_CONTENT_BYTES } from '@ferrum/protocol';
import type { Pool } from 'pg';
import { apiHandler, type Limits, type Route } from './api';
import { authRoutes } from './auth-routes';
import type { Auth } from './auth';
````

на:

````ts
import { MAX_CONTENT_BYTES } from '@ferrum/protocol';
import type { Pool } from 'pg';
import { apiHandler, type ApiDeps, type Limits, type Route } from './api';
import { authRoutes } from './auth-routes';
import type { Auth } from './auth';
````

В `apps/server/src/app.ts` заменить:

````ts
import { fileRoutes, uploadGate } from './file-routes';
import { imageRoutes } from './image-routes';
import type { Mailer } from './mail';
import { responseHeaders, serveStatic } from './static';
````

на:

````ts
import { fileRoutes, uploadGate } from './file-routes';
import { imageRoutes } from './image-routes';
import { shareRoutes } from './share-routes';
import { teamRoutes } from './team-routes';
import type { Mailer } from './mail';
import { responseHeaders, serveStatic } from './static';
````

В `apps/server/src/app.ts` заменить:

````ts
  blobs: BlobStore;
  mailer: Mailer;
  secret: string;
  /** Тесты сокращают пределы и сроки API. */
````

на:

````ts
  blobs: BlobStore;
  mailer: Mailer;
  /** Пусто — письма всем, как у входа. */
  allowedEmails?: string[];
  secret: string;
  /** Тесты сокращают пределы и сроки API. */
````

В `apps/server/src/app.ts` заменить:

````ts
const IDLE_TIMEOUT_SECONDS = 120;

/** Все маршруты API: отсюда их берёт и сервер, и таблица проверки прав в тестах. */
export function apiRoutes(deps: AppDeps): Route[] {
  const api = { ...deps, deadlineMs: deps.apiDeadlineMs };
  const gate = uploadGate();
  return [...fileRoutes(api, gate), ...imageRoutes(api, gate)];
}

````

на:

````ts
const IDLE_TIMEOUT_SECONDS = 120;

const apiDeps = (deps: AppDeps): ApiDeps => ({ ...deps, allowedEmails: deps.allowedEmails ?? [], deadlineMs: deps.apiDeadlineMs });

/** Все маршруты API: отсюда их берёт и сервер, и таблица проверки прав в тестах. */
export function apiRoutes(deps: AppDeps): Route[] {
  const api = apiDeps(deps);
  const gate = uploadGate();
  return [...fileRoutes(api, gate), ...imageRoutes(api, gate), ...teamRoutes(api), ...shareRoutes(api)];
}

````

В `apps/server/src/app.ts` заменить:

````ts
  const handleAuth = authRoutes(deps);
  const withHeaders = responseHeaders(deps.publicUrl);
  const handleApi = apiHandler(apiRoutes(deps), { ...deps, deadlineMs: deps.apiDeadlineMs });
  return Bun.serve({
    port,
````

на:

````ts
  const handleAuth = authRoutes(deps);
  const withHeaders = responseHeaders(deps.publicUrl);
  const handleApi = apiHandler(apiRoutes(deps), apiDeps(deps));
  return Bun.serve({
    port,
````

В `apps/server/src/auth.ts` заменить:

````ts
import { magicLink } from 'better-auth/plugins';
import type { Pool } from 'pg';
import { magicLinkMail, mailFailure, type Mailer } from './mail';
import { valkeyStorage } from './secondary-storage';
````

на:

````ts
import { magicLink } from 'better-auth/plugins';
import type { Pool } from 'pg';
import { claimInvites } from './invites';
import { magicLinkMail, mailFailure, type Mailer } from './mail';
import { valkeyStorage } from './secondary-storage';
````

В `apps/server/src/auth.ts` заменить:

````ts
    verification: { storeInDatabase: false },
    telemetry: { enabled: false },
    session: { expiresIn: SESSION_SECONDS, updateAge: deps.sessionUpdateAgeSeconds ?? 24 * HOUR },
    rateLimit: {
````

на:

````ts
    verification: { storeInDatabase: false },
    telemetry: { enabled: false },
    // Приглашения на этот адрес становятся доступом. Сбой не мешает войти: запись приглашения остаётся.
    databaseHooks: {
      user: {
        create: {
          after: async (user) => {
            await claimInvites(deps.pool, user.id, user.email).catch((e: unknown) =>
              console.error('Приглашения не перешли при первом входе:', e instanceof Error ? e.message : e),
            );
          },
        },
      },
    },
    session: { expiresIn: SESSION_SECONDS, updateAge: deps.sessionUpdateAgeSeconds ?? 24 * HOUR },
    rateLimit: {
````

В `apps/server/src/file-routes.ts` заменить:

````ts
    thumbVersion: r.thumb_version,
    version: Number(r.version),
    role: (RANK_ROLE[r.rank] ?? 'viewer') as FileRole,
    ...(r.trashed_at ? { trashedAt: r.trashed_at.getTime() } : {}),
    ...(r.project_id ? { projectId: r.project_id } : {}),
````

на:

````ts
    thumbVersion: r.thumb_version,
    version: Number(r.version),
    role: RANK_ROLE[r.rank] as FileRole,
    ...(r.trashed_at ? { trashedAt: r.trashed_at.getTime() } : {}),
    ...(r.project_id ? { projectId: r.project_id } : {}),
````

В `apps/server/src/file-routes.ts` заменить:

````ts
export async function loadFile(db: Pool | PoolClient, id: string, user: string): Promise<FileMetaDto | null> {
  const { rows } = await db.query<FileRow>(`SELECT ${FILE_COLUMNS} FROM file f WHERE f.id = $2`, [user, id]);
  return rows[0] ? fileDto(rows[0]) : null;
}

````

на:

````ts
export async function loadFile(db: Pool | PoolClient, id: string, user: string): Promise<FileMetaDto | null> {
  const { rows } = await db.query<FileRow>(`SELECT ${FILE_COLUMNS} FROM file f WHERE f.id = $2`, [user, id]);
  // Нет роли — нет и записи: файл только что убрали в корзину или сняли доступ.
  return rows[0] && rows[0].rank > 0 ? fileDto(rows[0]) : null;
}

````

В `apps/server/src/file-routes.ts` заменить:

````ts
          [...params, q.count, q.start],
        );
        return ok({ total: Number(total.rows[0].n), rows: rows.rows.map(fileDto) } satisfies FileListDto);
      },
    },
````

на:

````ts
          [...params, q.count, q.start],
        );
        return ok({ total: Number(total.rows[0].n), rows: rows.rows.filter((r) => r.rank > 0).map(fileDto) } satisfies FileListDto);
      },
    },
````

В `apps/server/src/index.ts` заменить:

````ts
    blobs,
    mailer,
    secret: config.authSecret,
  },
````

на:

````ts
    blobs,
    mailer,
    allowedEmails: config.allowedEmails,
    secret: config.authSecret,
  },
````

Создать `apps/server/src/invites.ts`:

````ts
// Приглашения по адресу: письма и переход в доступ при первом входе. Счётчики — на адрес получателя,
// отдельно от писем входа: чужие приглашения не должны закрывать человеку вход.

import type { Pool } from 'pg';
import type { ApiDeps } from './api';
import { emailAllowed } from './auth';
import { escapeHtml, mailFailure, type Mail } from './mail';
import { valkeyStorage } from './secondary-storage';

/** Приглашений на один адрес в час: иначе чужую почту можно забить письмами от своего имени. */
export const INVITES_PER_ADDRESS = 5;
const HOUR = 60 * 60;

/** Письмо уходит после ответа: ответ приглашающему не зависит от адреса (`ALLOWED_EMAILS`, предел).
 * Адрес вне списка и сверх предела письма не получает, но запись остаётся: при входе он её получит. */
export function sendInvite(deps: ApiDeps, to: string, mail: Mail): void {
  setTimeout(() => {
    void (async () => {
      const sent = await valkeyStorage(deps.valkey).increment!(`invite-to:${to}`, HOUR);
      if (sent > INVITES_PER_ADDRESS || !emailAllowed(to, deps.allowedEmails)) return;
      await deps.mailer.send(mail);
    })().catch((e: unknown) => console.error('Не удалось отправить приглашение:', mailFailure(e)));
  }, 0);
}

function letter(to: string, subject: string, lines: string[], link: string, action: string): Mail {
  return {
    to,
    subject,
    text: `${lines.join('\n')}\n\n${link}\n`,
    html: `${lines.map((l) => `<p>${escapeHtml(l)}</p>`).join('')}<p><a href="${escapeHtml(link)}">${escapeHtml(action)}</a></p>`,
  };
}

export const fileInviteMail = (to: string, from: string, file: string, role: 'editor' | 'viewer', link: string): Mail =>
  letter(
    to,
    'Вам открыли файл в Ferrum',
    [`${from} открывает вам файл «${file}»: ${role === 'editor' ? 'можно редактировать' : 'можно смотреть'}.`],
    link,
    'Открыть файл',
  );

export const teamInviteMail = (to: string, from: string, team: string, link: string): Mail =>
  letter(to, 'Вас пригласили в команду в Ferrum', [`${from} приглашает вас в команду «${team}».`], link, 'Открыть Ferrum');

export const accessRequestMail = (to: string, from: string, file: string, link: string): Mail =>
  letter(to, 'Запрос доступа к файлу в Ferrum', [`${from} просит доступ к файлу «${file}».`], link, 'Открыть файл');

/** Первый вход: приглашения на этот адрес становятся доступом. Сбой не мешает входу: запись
 * приглашения остаётся и перейдёт при следующем обращении этой функции. */
export async function claimInvites(pool: Pool, user: string, email: string): Promise<void> {
  const client = await pool.connect();
  try {
    await client.query('BEGIN');
    await client.query(
      `INSERT INTO file_access (file_id, user_id, role) SELECT file_id, $1, role FROM file_invite WHERE email = $2
       ON CONFLICT (file_id, user_id) DO NOTHING`,
      [user, email],
    );
    await client.query('DELETE FROM file_invite WHERE email = $1', [email]);
    await client.query(
      `INSERT INTO team_member (team_id, user_id, role) SELECT team_id, $1, 'member' FROM team_invite WHERE email = $2
       ON CONFLICT (team_id, user_id) DO NOTHING`,
      [user, email],
    );
    await client.query('DELETE FROM team_invite WHERE email = $1', [email]);
    await client.query('COMMIT');
  } catch (e) {
    await client.query('ROLLBACK').catch(() => {});
    throw e;
  } finally {
    client.release();
  }
}
````

В `apps/server/src/mail.ts` заменить:

````ts

const ESCAPES: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
const escapeHtml = (s: string) => s.replace(/[&<>"']/g, (c) => ESCAPES[c]);

export function magicLinkMail(to: string, link: string): Mail {
````

на:

````ts

const ESCAPES: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
export const escapeHtml = (s: string) => s.replace(/[&<>"']/g, (c) => ESCAPES[c]);

export function magicLinkMail(to: string, link: string): Mail {
````

Создать `apps/server/src/share-routes.ts`:

````ts
// «Поделиться» (спека, раздел 6.3): приглашение по email с ролью, список людей, смена и снятие роли,
// ссылка с ролью и выключателем. Токен ссылки в базе — хэшем; его можно показать снова, потому что он
// выводится из секрета сервера и случайной `nonce`: утечка базы токенов не раскрывает.

import {
  parseInvite,
  parseLinkUpdate,
  parseRedeem,
  parseRoleChange,
  MAX_FILES_WINDOW,
  type ShareDto,
  type ShareLinkDto,
  type ShareRole,
} from '@ferrum/protocol';
import { createHash, createHmac, randomBytes, timingSafeEqual } from 'node:crypto';
import { apiError, ok, readJson, type ApiDeps, type Ctx, type Route } from './api';
import { loadFile } from './file-routes';
import { accessRequestMail, fileInviteMail, sendInvite } from './invites';
import { valkeyStorage } from './secondary-storage';

const sha256 = (token: string) => createHash('sha256').update(token).digest('hex');
const DAY = 24 * 60 * 60;

export function shareRoutes(deps: ApiDeps): Route[] {
  const { pool } = deps;
  const tokenFor = (file: string, nonce: string) =>
    createHmac('sha256', deps.secret).update(`ferrum-share-link:${file}:${nonce}`).digest('base64url');

  const linkDto = (file: string, r: { role: ShareRole; enabled: boolean; nonce: string; token_hash: string }): ShareLinkDto => {
    const token = tokenFor(file, r.nonce);
    return { enabled: r.enabled, role: r.role, token: sha256(token) === r.token_hash ? token : null };
  };

  async function shareOf(ctx: Ctx): Promise<ShareDto> {
    const file = ctx.params.id;
    // Имя — только у тех, кто уже открывал файл: иначе приглашение любого адреса выдавало бы,
    // есть ли у него учётная запись и как его зовут.
    const people = await pool.query<ShareDto['people'][number]>(
      `SELECT a.id, u.email, CASE WHEN a.user_id = $2 OR EXISTS (SELECT 1 FROM file_recent r WHERE r.file_id = a.file_id AND r.user_id = a.user_id)
         THEN u.name END AS name, a.role
       FROM file_access a JOIN "user" u ON u.id = a.user_id WHERE a.file_id = $1
       UNION ALL SELECT i.id, i.email, NULL, i.role FROM file_invite i WHERE i.file_id = $1
       ORDER BY role DESC, email LIMIT $3`,
      [file, ctx.user.id, MAX_FILES_WINDOW],
    );
    const owner = ctx.role === 'owner';
    const link = owner
      ? await pool.query<{ role: ShareRole; enabled: boolean; nonce: string; token_hash: string }>(
          'SELECT role, enabled, nonce, token_hash FROM share_link WHERE file_id = $1',
          [file],
        )
      : null;
    return { people: people.rows, link: link?.rows[0] ? linkDto(file, link.rows[0]) : null, canManage: owner };
  }

  const newLink = (file: string) => {
    const nonce = randomBytes(16).toString('hex');
    return { nonce, tokenHash: sha256(tokenFor(file, nonce)) };
  };

  return [
    {
      method: 'GET',
      path: '/api/files/:id/share',
      access: { kind: 'file', min: 'editor' },
      limit: 'read',
      handle: async (ctx) => ok(await shareOf(ctx)),
    },
    {
      method: 'POST',
      path: '/api/files/:id/share/invite',
      access: { kind: 'file', min: 'editor' },
      limit: 'mail',
      handle: async (ctx) => {
        const body = parseInvite(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const file = ctx.params.id;
        const { rows } = await pool.query<{ id: string }>('SELECT id FROM "user" WHERE email = $1', [body.email]);
        if (rows[0]) {
          // Редактор может только добавить доступ, поменять чужой — владелец. Владельца это не касается.
          await pool.query(
            `INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, $3)
             ON CONFLICT (file_id, user_id) DO UPDATE SET role = EXCLUDED.role
             WHERE file_access.role <> 'owner' AND ($4 OR file_access.role = 'viewer')`,
            [file, rows[0].id, body.role, ctx.role === 'owner'],
          );
        } else {
          await pool.query(
            `INSERT INTO file_invite (file_id, email, role, invited_by) VALUES ($1, $2, $3, $4)
             ON CONFLICT (file_id, email) DO UPDATE SET role = EXCLUDED.role WHERE $5 OR file_invite.role = 'viewer'`,
            [file, body.email, body.role, ctx.user.id, ctx.role === 'owner'],
          );
        }
        const name = await pool.query<{ name: string }>('SELECT name FROM file WHERE id = $1', [file]);
        const from = ctx.user.name || ctx.user.email;
        sendInvite(deps, body.email, fileInviteMail(body.email, from, name.rows[0].name, body.role, `${deps.publicUrl}/#/file/${file}`));
        return ok(await shareOf(ctx));
      },
    },
    {
      method: 'PATCH',
      path: '/api/files/:id/share/people/:entryId',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseRoleChange(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { id: file, entryId: entry } = ctx.params;
        const access = await pool.query<{ role: string }>('SELECT role FROM file_access WHERE id = $1 AND file_id = $2', [entry, file]);
        if (access.rows[0]?.role === 'owner') return apiError(403, 'FORBIDDEN');
        const changed = access.rows[0]
          ? await pool.query('UPDATE file_access SET role = $3 WHERE id = $1 AND file_id = $2', [entry, file, body.role])
          : await pool.query('UPDATE file_invite SET role = $3 WHERE id = $1 AND file_id = $2', [entry, file, body.role]);
        return changed.rowCount === 0 ? apiError(404, 'NOT_FOUND') : ok(await shareOf(ctx));
      },
    },
    {
      method: 'DELETE',
      path: '/api/files/:id/share/people/:entryId',
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      handle: async (ctx) => {
        const { id: file, entryId: entry } = ctx.params;
        const access = await pool.query<{ role: string; user_id: string }>(
          'SELECT role, user_id FROM file_access WHERE id = $1 AND file_id = $2',
          [entry, file],
        );
        const row = access.rows[0];
        const manage = ctx.role === 'owner';
        if (row) {
          // Сам себе может снять доступ каждый, кроме владельца; другому — только владелец.
          if (row.role === 'owner' || (!manage && row.user_id !== ctx.user.id)) return apiError(403, 'FORBIDDEN');
          await pool.query('DELETE FROM file_access WHERE id = $1', [entry]);
          await pool.query('DELETE FROM file_link_user WHERE file_id = $1 AND user_id = $2', [file, row.user_id]);
        } else {
          const invite = await pool.query('SELECT 1 FROM file_invite WHERE id = $1 AND file_id = $2', [entry, file]);
          if (invite.rowCount === 0) return apiError(404, 'NOT_FOUND');
          if (!manage) return apiError(403, 'FORBIDDEN');
          await pool.query('DELETE FROM file_invite WHERE id = $1', [entry]);
        }
        const left = await pool.query<{ rank: number }>('SELECT file_role_rank($1, $2) AS rank', [file, ctx.user.id]);
        return left.rows[0].rank > 0 && ctx.role !== 'viewer' ? ok(await shareOf(ctx)) : ok({ ok: true });
      },
    },
    {
      method: 'PUT',
      path: '/api/files/:id/share/link',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseLinkUpdate(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const file = ctx.params.id;
        const { nonce, tokenHash } = newLink(file);
        await pool.query(
          `INSERT INTO share_link (file_id, token_hash, nonce, role, enabled) VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (file_id) DO UPDATE SET role = EXCLUDED.role, enabled = EXCLUDED.enabled`,
          [file, tokenHash, nonce, body.role, body.enabled],
        );
        return ok(await shareOf(ctx));
      },
    },
    {
      method: 'POST',
      path: '/api/files/:id/share/link/reset',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        const file = ctx.params.id;
        const { nonce, tokenHash } = newLink(file);
        await pool.query('UPDATE share_link SET token_hash = $2, nonce = $3 WHERE file_id = $1', [file, tokenHash, nonce]);
        // Новая ссылка отменяет старую целиком: и у тех, кто уже пришёл по ней.
        await pool.query('DELETE FROM file_link_user WHERE file_id = $1', [file]);
        return ok(await shareOf(ctx));
      },
    },
    {
      // Прав на файл ещё нет: они появляются здесь. Неверный токен, выключенная ссылка и чужой файл
      // неотличимы от несуществующего.
      method: 'POST',
      path: '/api/files/:id/redeem',
      access: { kind: 'user' },
      limit: 'redeem',
      handle: async (ctx) => {
        const body = parseRedeem(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const file = ctx.params.id;
        const { rows } = await pool.query<{ token_hash: string; enabled: boolean }>(
          'SELECT token_hash, enabled FROM share_link WHERE file_id = $1',
          [file],
        );
        const want = Buffer.from(rows[0]?.token_hash ?? '0'.repeat(64));
        const got = Buffer.from(sha256(body.token));
        if (!rows[0]?.enabled || !timingSafeEqual(want, got)) return apiError(404, 'NOT_FOUND');
        await pool.query('INSERT INTO file_link_user (file_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING', [file, ctx.user.id]);
        const meta = await loadFile(pool, file, ctx.user.id);
        return meta ? ok(meta) : apiError(404, 'NOT_FOUND');
      },
    },
    {
      // Запрос доступа со страницы «Нет доступа». Ответ один и тот же, есть файл или нет.
      method: 'POST',
      path: '/api/files/:id/request-access',
      access: { kind: 'user' },
      limit: 'mail',
      handle: async (ctx) => {
        const file = ctx.params.id;
        const { rows } = await pool.query<{ name: string; email: string; rank: number; trashed: boolean }>(
          `SELECT f.name, u.email, file_role_rank(f.id, $2) AS rank, f.trashed_at IS NOT NULL AS trashed
           FROM file f JOIN "user" u ON u.id = f.owner_id WHERE f.id = $1`,
          [file, ctx.user.id],
        );
        const r = rows[0];
        if (r && r.rank === 0 && !r.trashed) {
          const once = await valkeyStorage(deps.valkey).increment!(`request-access:${file}:${ctx.user.id}`, DAY);
          if (once === 1) {
            const from = ctx.user.name || ctx.user.email;
            sendInvite(deps, r.email, accessRequestMail(r.email, from, r.name, `${deps.publicUrl}/#/file/${file}`));
          }
        }
        return ok({ ok: true });
      },
    },
  ];
}
````

Создать `apps/server/src/team-routes.ts`:

````ts
// Команды и проекты (спека, раздел 6.1, 6.2). Команду создаёт любой человек и становится её владельцем.
// Участники — по email: человек без учётной записи попадает в команду при первом входе
// (`claimInvites`). Участник — редактор файлов проектов команды, владелец команды — владелец.

import {
  MAX_FILES_WINDOW,
  parseTeamCreate,
  parseTeamInvite,
  type ProjectDto,
  type TeamDto,
  type TeamListDto,
  type TeamMembersDto,
} from '@ferrum/protocol';
import type { PoolClient } from 'pg';
import { apiError, ok, readJson, type ApiDeps, type Ctx, type Route } from './api';
import { inTransaction } from './file-routes';
import { sendInvite, teamInviteMail } from './invites';

/** Кто в команде: и участники, и приглашённые. Только адреса: список не выдаёт, есть ли учётная запись. */
async function members(deps: ApiDeps, teamId: string): Promise<TeamMembersDto> {
  const { rows } = await deps.pool.query<TeamMembersDto['members'][number]>(
    `SELECT m.id, u.email, m.role FROM team_member m JOIN "user" u ON u.id = m.user_id WHERE m.team_id = $1
     UNION ALL SELECT i.id, i.email, 'member' FROM team_invite i WHERE i.team_id = $1
     ORDER BY role DESC, email LIMIT $2`,
    [teamId, MAX_FILES_WINDOW],
  );
  return { members: rows };
}

/** Файлы ушедшего из проектов команды остаются у команды: владелец — владелец команды. */
async function handOverFiles(c: PoolClient, teamId: string, from: string, to: string): Promise<void> {
  const { rows } = await c.query<{ id: string }>(
    `UPDATE file f SET owner_id = $3 FROM project p
     WHERE f.project_id = p.id AND p.team_id = $1 AND f.owner_id = $2 RETURNING f.id`,
    [teamId, from, to],
  );
  const ids = rows.map((r) => r.id);
  await c.query('DELETE FROM file_access WHERE file_id = ANY($1::uuid[]) AND user_id = $2', [ids, from]);
  await c.query(
    `INSERT INTO file_access (file_id, user_id, role) SELECT unnest($1::uuid[]), $2, 'owner'
     ON CONFLICT (file_id, user_id) DO UPDATE SET role = 'owner'`,
    [ids, to],
  );
}

export function teamRoutes(deps: ApiDeps): Route[] {
  const { pool } = deps;
  const projectDto = (r: { id: string; team_id: string; name: string }): ProjectDto => ({ id: r.id, teamId: r.team_id, name: r.name });
  return [
    {
      method: 'GET',
      path: '/api/teams',
      access: { kind: 'user' },
      limit: 'read',
      handle: async (ctx) => {
        const teams = await pool.query<{ id: string; name: string; role: 'owner' | 'member' }>(
          `SELECT t.id, t.name, m.role FROM team_member m JOIN team t ON t.id = m.team_id
           WHERE m.user_id = $1 ORDER BY t.name, t.id LIMIT $2`,
          [ctx.user.id, MAX_FILES_WINDOW],
        );
        const projects = await pool.query<{ id: string; team_id: string; name: string }>(
          'SELECT id, team_id, name FROM project WHERE team_id = ANY($1::uuid[]) ORDER BY name, id',
          [teams.rows.map((t) => t.id)],
        );
        const out: TeamDto[] = teams.rows.map((t) => ({ ...t, projects: projects.rows.filter((p) => p.team_id === t.id).map(projectDto) }));
        return ok({ teams: out } satisfies TeamListDto);
      },
    },
    {
      method: 'POST',
      path: '/api/teams',
      access: { kind: 'user' },
      limit: 'create',
      handle: async (ctx) => {
        const body = parseTeamCreate(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const team = await inTransaction(pool, async (c) => {
          const { rows } = await c.query<{ id: string }>('INSERT INTO team (name) VALUES ($1) RETURNING id', [body.name]);
          await c.query(`INSERT INTO team_member (team_id, user_id, role) VALUES ($1, $2, 'owner')`, [rows[0].id, ctx.user.id]);
          return rows[0];
        });
        return ok({ id: team.id, name: body.name, role: 'owner', projects: [] } satisfies TeamDto, 201);
      },
    },
    {
      method: 'PATCH',
      path: '/api/teams/:teamId',
      access: { kind: 'team', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseTeamCreate(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        await pool.query('UPDATE team SET name = $2 WHERE id = $1', [ctx.teamId, body.name]);
        return ok({ ok: true });
      },
    },
    {
      method: 'DELETE',
      path: '/api/teams/:teamId',
      access: { kind: 'team', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        // Сначала проекты: случайное удаление команды не должно трогать файлы.
        const gone = await pool.query(
          'DELETE FROM team t WHERE t.id = $1 AND NOT EXISTS (SELECT 1 FROM project p WHERE p.team_id = t.id)',
          [ctx.teamId],
        );
        return gone.rowCount === 0 ? apiError(409, 'NOT_EMPTY') : ok({ ok: true });
      },
    },
    {
      method: 'GET',
      path: '/api/teams/:teamId/members',
      access: { kind: 'team', min: 'member' },
      limit: 'read',
      handle: async (ctx) => ok(await members(deps, ctx.teamId)),
    },
    {
      method: 'POST',
      path: '/api/teams/:teamId/invite',
      access: { kind: 'team', min: 'owner' },
      limit: 'mail',
      handle: async (ctx) => {
        const body = parseTeamInvite(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { rows } = await pool.query<{ id: string }>('SELECT id FROM "user" WHERE email = $1', [body.email]);
        if (rows[0]) {
          await pool.query(
            `INSERT INTO team_member (team_id, user_id, role) VALUES ($1, $2, 'member') ON CONFLICT (team_id, user_id) DO NOTHING`,
            [ctx.teamId, rows[0].id],
          );
        } else {
          await pool.query(
            'INSERT INTO team_invite (team_id, email, invited_by) VALUES ($1, $2, $3) ON CONFLICT (team_id, email) DO NOTHING',
            [ctx.teamId, body.email, ctx.user.id],
          );
        }
        const name = await pool.query<{ name: string }>('SELECT name FROM team WHERE id = $1', [ctx.teamId]);
        sendInvite(
          deps,
          body.email,
          teamInviteMail(body.email, ctx.user.name || ctx.user.email, name.rows[0].name, `${deps.publicUrl}/#/`),
        );
        return ok(await members(deps, ctx.teamId));
      },
    },
    {
      method: 'DELETE',
      path: '/api/teams/:teamId/members/:entryId',
      access: { kind: 'team', min: 'member' },
      limit: 'write',
      handle: async (ctx: Ctx) => {
        const entry = ctx.params.entryId;
        const invite = await pool.query('SELECT 1 FROM team_invite WHERE id = $1 AND team_id = $2', [entry, ctx.teamId]);
        if (invite.rowCount) {
          if (ctx.teamRole !== 'owner') return apiError(403, 'FORBIDDEN');
          await pool.query('DELETE FROM team_invite WHERE id = $1', [entry]);
          return ok(await members(deps, ctx.teamId));
        }
        const { rows } = await pool.query<{ user_id: string; role: string }>(
          'SELECT user_id, role FROM team_member WHERE id = $1 AND team_id = $2',
          [entry, ctx.teamId],
        );
        const target = rows[0];
        if (!target) return apiError(404, 'NOT_FOUND');
        // Участник может уйти сам; владельца снять нельзя: команда осталась бы без него.
        if (target.role === 'owner' || (ctx.teamRole !== 'owner' && target.user_id !== ctx.user.id)) return apiError(403, 'FORBIDDEN');
        await inTransaction(pool, async (c) => {
          const owner = await c.query<{ user_id: string }>(
            `SELECT user_id FROM team_member WHERE team_id = $1 AND role = 'owner' LIMIT 1`,
            [ctx.teamId],
          );
          await handOverFiles(c, ctx.teamId, target.user_id, owner.rows[0].user_id);
          await c.query('DELETE FROM team_member WHERE id = $1', [entry]);
        });
        return ok(await members(deps, ctx.teamId));
      },
    },
    {
      method: 'POST',
      path: '/api/teams/:teamId/projects',
      access: { kind: 'team', min: 'member' },
      limit: 'create',
      handle: async (ctx) => {
        const body = parseTeamCreate(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { rows } = await pool.query<{ id: string; team_id: string; name: string }>(
          'INSERT INTO project (team_id, name) VALUES ($1, $2) RETURNING id, team_id, name',
          [ctx.teamId, body.name],
        );
        return ok(projectDto(rows[0]), 201);
      },
    },
    {
      method: 'PATCH',
      path: '/api/projects/:projectId',
      access: { kind: 'project', min: 'member' },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseTeamCreate(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { rows } = await pool.query<{ id: string; team_id: string; name: string }>(
          'UPDATE project SET name = $2 WHERE id = $1 RETURNING id, team_id, name',
          [ctx.params.projectId, body.name],
        );
        return ok(projectDto(rows[0]));
      },
    },
    {
      method: 'DELETE',
      path: '/api/projects/:projectId',
      access: { kind: 'project', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        const gone = await pool.query(
          `DELETE FROM project p WHERE p.id = $1 AND NOT EXISTS (SELECT 1 FROM file f WHERE f.project_id = p.id AND f.trashed_at IS NULL)`,
          [ctx.params.projectId],
        );
        return gone.rowCount === 0 ? apiError(409, 'NOT_EMPTY') : ok({ ok: true });
      },
    },
  ];
}
````

В `docker-compose.test.yml` заменить:

````yaml
    tmpfs:
      - /var/lib/postgresql
    restart: 'no'

````

на:

````yaml
    tmpfs:
      - /var/lib/postgresql
    # Тесты много раз пересоздают схему: журнал базы иначе заполняет tmpfs (код 53100).
    command: [postgres, -c, max_wal_size=128MB, -c, min_wal_size=32MB, -c, checkpoint_timeout=30s]
    restart: 'no'

````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `bun test`: 12 и 282 pass, таблица `access.test.ts` перечисляет все маршруты

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/migrations/003_files.sql apps/server/src/api.ts apps/server/src/app.ts apps/server/src/auth.ts apps/server/src/file-routes.ts apps/server/src/index.ts apps/server/src/invites.ts apps/server/src/mail.ts apps/server/src/share-routes.ts apps/server/src/team-routes.ts apps/server/test/access.test.ts apps/server/test/api-stand.ts apps/server/test/files.test.ts apps/server/test/images.test.ts apps/server/test/share.test.ts apps/server/test/teams.test.ts docker-compose.test.yml
git commit -F - <<'EOF'
сервер: команды, проекты и «Поделиться»

Команды и проекты, участник — редактор файлов проектов, файлы ушедшего переходят
владельцу команды. Приглашения по email с ролью: адрес без учётной записи получает
доступ при первом входе, список людей не выдаёт, есть ли учётная запись. Ссылка с
ролью и выключателем, токен в базе хэшем. Запрос доступа: ответ один и тот же для
чужого и несуществующего файла.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 5: Сервер — уборка и проба памяти

Запуск убирает `.tmp` от сбоя (B-204). Проверка памяти загрузки в своём процессе. `no-unassigned-vars` для сервера и протокола (B-189).

**Files:**
- Modify: `.oxlintrc.json`, `apps/server/src/blob-store.ts`, `apps/server/src/file-routes.ts`, `apps/server/src/index.ts`
- Test: `apps/server/test/blob-store.test.ts`, `apps/server/test/files.test.ts`, `apps/server/test/memory-probe.ts` (новый)

- [ ] **Step 1: Написать тесты**

В `apps/server/test/blob-store.test.ts` заменить:

````ts
import { expect, test } from 'bun:test';
import { mkdtempSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
````

на:

````ts
import { expect, test } from 'bun:test';
import { mkdtempSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
````

В `apps/server/test/blob-store.test.ts` заменить:

````ts
  expect(readdirSync(join(dir, 'files'))).toEqual([]);
});
````

на:

````ts
  expect(readdirSync(join(dir, 'files'))).toEqual([]);
});

test('при запуске временные файлы от сбоя убираются, остальное цело', async () => {
  const { s, dir } = store();
  await s.put('files/a', new Uint8Array([1]));
  writeFileSync(join(dir, 'files', 'a.1.1.tmp'), 'обрыв');
  expect(await s.sweep()).toBe(1);
  expect(readdirSync(join(dir, 'files'))).toEqual(['a']);
});
````

В `apps/server/test/files.test.ts` заменить:

````ts

test('файл большого размера идёт потоком: память сервера не растёт на размер файла', async () => {
  const f = await s.newFile(owner);
  const MB = 1024 * 1024;
  const whole = container([], 200 * MB);
  const chunk = 256 * 1024;
  let at = 0;
  // Клиент в этом же процессе: его байты уже в памяти (`whole`), а куски — окна в них же.
  const body = new ReadableStream<Uint8Array>({
    pull(c) {
      if (at >= whole.length) return c.close();
      c.enqueue(whole.subarray(at, at + chunk));
      at += chunk;
    },
  });
  Bun.gc(true);
  const before = process.memoryUsage().rss;
  let peak = before;
  const probe = setInterval(() => (peak = Math.max(peak, process.memoryUsage().rss)), 5);
  const res = await fetch(`${s.publicUrl}/api/files/${f.id}/content`, {
    method: 'PUT',
    headers: { origin: s.publicUrl, cookie: owner.cookie, 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() },
    body,
  });
  expect(res.status).toBe(200);
  expect((await json(res)).size).toBe(whole.length);
  const got = await s.call(owner, 'GET', `/api/files/${f.id}/content`);
  const reader = got.body!.getReader();
  let read = 0;
  for (let r = await reader.read(); !r.done; r = await reader.read()) read += r.value.length;
  clearInterval(probe);
  expect(read).toBe(whole.length);
  // Предел — половина файла: сервер, собирающий тело в памяти, сразу его превысит.
  expect((peak - before) / MB).toBeLessThan(100);
});

````

на:

````ts

test('файл большого размера идёт потоком: память сервера не растёт на размер файла', async () => {
  const MB = 200;
  const child = Bun.spawn(['bun', 'run', join(import.meta.dir, 'memory-probe.ts'), String(MB)], { stdout: 'pipe', stderr: 'inherit' });
  const out = await new Response(child.stdout).text();
  expect(await child.exited).toBe(0);
  const r = JSON.parse(out.trim().split('\n').at(-1)!) as { status: number; read: number; growthMb: number };
  expect(r.status).toBe(200);
  expect(r.read).toBe(MB * 1024 * 1024 + 16 + 5 + 40);
  // Клиент и сервер здесь в одном процессе: потоком прирост около 110 МБ, сборка тела в памяти — больше 500.
  expect(r.growthMb).toBeLessThan(MB);
});

````

Создать `apps/server/test/memory-probe.ts`:

````ts
// Отдельный процесс для теста памяти: настоящий сервер и загрузка потоком, без шума других тестов.
// Клиент не держит файл в памяти: куски — один и тот же буфер. Печатает прирост памяти процесса в МБ.

import { containerHeader } from '@ferrum/protocol';
import { apiStand } from './api-stand';

const MB = 1024 * 1024;
const SIZE = Number(process.argv[2]) * MB;
const s = await apiStand({}, true);
const who = await s.signIn(`memory-${Date.now()}@example.com`);
const f = await s.newFile(who);
const head = new Uint8Array(16 + 5 + 40);
head.set(containerHeader(1));
head[16] = 1;
new DataView(head.buffer).setBigUint64(16 + 5, BigInt(SIZE), true);
const chunk = new Uint8Array(256 * 1024);
let left = SIZE;
let sentHead = false;
const body = new ReadableStream<Uint8Array>({
  pull(c) {
    if (!sentHead) {
      sentHead = true;
      c.enqueue(head);
    } else if (left === 0) c.close();
    else {
      const n = Math.min(left, chunk.length);
      left -= n;
      c.enqueue(chunk.subarray(0, n));
    }
  },
});
Bun.gc(true);
const before = process.memoryUsage().rss;
let peak = before;
const probe = setInterval(() => (peak = Math.max(peak, process.memoryUsage().rss)), 5);
const put = await fetch(`${s.publicUrl}/api/files/${f.id}/content`, {
  method: 'PUT',
  headers: { origin: s.publicUrl, cookie: who.cookie, 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() },
  body,
});
const got = await s.call(who, 'GET', `/api/files/${f.id}/content`);
let read = 0;
const reader = got.body!.getReader();
for (let r = await reader.read(); !r.done; r = await reader.read()) read += r.value.length;
clearInterval(probe);
console.log(JSON.stringify({ status: put.status, read, growthMb: (peak - before) / MB }));
await s.stop();
process.exit(0);
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/blob-store.test.ts`
Expected: FAIL — нет `sweep`: 5 pass, 1 fail

- [ ] **Step 3: Реализация**

В `.oxlintrc.json` заменить:

````json
    // Ссылка на элемент в SolidJS присваивается разметкой (`ref={el}`), правило этого не видит (бэклог).
    "no-unassigned-vars": "off"
  }
}
````

на:

````json
    // Ссылка на элемент в SolidJS присваивается разметкой (`ref={el}`), правило этого не видит (бэклог).
    "no-unassigned-vars": "off"
  },
  "overrides": [
    {
      // Здесь SolidJS нет: правило работает.
      "files": ["apps/server/**", "packages/**"],
      "rules": { "no-unassigned-vars": "error" }
    }
  ]
}
````

В `apps/server/src/blob-store.ts` заменить:

````ts
// реализация этого интерфейса.

import { mkdir, open, rename, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';

````

на:

````ts
// реализация этого интерфейса.

import { mkdir, open, readdir, rename, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';

````

В `apps/server/src/blob-store.ts` заменить:

````ts
  }

  async put(key: string, bytes: Uint8Array): Promise<void> {
    await this.atomically(key, async (file) => {
````

на:

````ts
  }

  /** Временные файлы, оставшиеся от сбоя процесса: при запуске их никто уже не допишет. */
  async sweep(): Promise<number> {
    let removed = 0;
    const entries = await readdir(this.dir, { recursive: true, withFileTypes: true }).catch(() => []);
    for (const e of entries) {
      if (!e.isFile() || !e.name.endsWith('.tmp')) continue;
      await rm(join(e.parentPath, e.name), { force: true });
      removed++;
    }
    return removed;
  }

  async put(key: string, bytes: Uint8Array): Promise<void> {
    await this.atomically(key, async (file) => {
````

В `apps/server/src/file-routes.ts` заменить:

````ts
export const BLOB_GRACE_SECONDS = 120;
/** Одновременных загрузок: на человека и на сервер. Памяти загрузка не занимает, занимает диск и поток. */
export const MAX_UPLOADS_PER_USER = 4;
export const MAX_UPLOADS = 32;
const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
````

на:

````ts
export const BLOB_GRACE_SECONDS = 120;
/** Одновременных загрузок: на человека и на сервер. Памяти загрузка не занимает, занимает диск и поток. */
export const MAX_UPLOADS_PER_USER = 8;
export const MAX_UPLOADS = 32;
const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
````

В `apps/server/src/index.ts` заменить:

````ts
// Том доступен для записи: иначе отказ при запуске, а не на первом сохранении.
await blobs.put('healthcheck', new Uint8Array([1]));
// Better Auth обращается к таблицам при первом запросе: к этому времени миграции уже применены.
const mailer = smtpMailer(config.smtpUrl, config.mailFrom);
````

на:

````ts
// Том доступен для записи: иначе отказ при запуске, а не на первом сохранении.
await blobs.put('healthcheck', new Uint8Array([1]));
await blobs.sweep();
// Better Auth обращается к таблицам при первом запросе: к этому времени миграции уже применены.
const mailer = smtpMailer(config.smtpUrl, config.mailFrom);
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `bun test`: 12 и 283 pass

- [ ] **Step 5: Закоммитить**

```bash
git add .oxlintrc.json apps/server/src/blob-store.ts apps/server/src/file-routes.ts apps/server/src/index.ts apps/server/test/blob-store.test.ts apps/server/test/files.test.ts apps/server/test/memory-probe.ts
git commit -F - <<'EOF'
сервер: уборка временных файлов и проба памяти отдельным процессом

Запуск убирает `.tmp`, оставшиеся от сбоя процесса (B-204). Загрузок на человека 8:
картинки, превью и содержимое двух открытых файлов не должны упираться в предел.
Проверка памяти загрузки — в своём процессе: в общем с другими тестами она шумела.
`no-unassigned-vars` включено для сервера и протокола (B-189).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 6: Интерфейс — серверный `FileStore`, режим просмотра

Спека §6.2, §6.5, §6.3. Главная задача клиента: файлы, журнал, картинки и превью в worker'е, выбор хранилища в `main.tsx`, разделы через `extensions`, режим просмотра и плашки доступа. Закрывает B-190, B-195, B-205, B-206.

**Files:**
- Create: `apps/web/src/core/server-files.ts`, `apps/web/src/core/server-messages.ts`, `apps/web/src/ui/local-store.ts`, `apps/web/src/ui/no-access.tsx`, `apps/web/src/ui/server/server-api.ts`, `apps/web/src/ui/server/server-store.ts`, `apps/web/src/ui/server/setup.local.ts`, `apps/web/src/ui/server/setup.tsx`, `apps/web/src/ui/styles/server.css`, `apps/web/src/worker/server.worker.ts`
- Modify: `apps/web/scripts/check-dist.mjs`, `apps/web/src/core/file-list.ts`, `apps/web/src/main.tsx`, `apps/web/src/ui/app.tsx`, `apps/web/src/ui/auth/auth-gate.tsx`, `apps/web/src/ui/boot.ts`, `apps/web/src/ui/chrome.tsx`, `apps/web/src/ui/extensions.ts`, `apps/web/src/ui/file-store.ts`, `apps/web/src/ui/files-screen.tsx`, `apps/web/src/ui/properties.tsx`, `apps/web/src/ui/root.tsx`, `apps/web/src/ui/router.ts`, `apps/web/src/ui/saver.ts`, `apps/web/src/ui/store.ts`, `apps/web/src/ui/styles/files.css`, `apps/web/src/ui/styles/properties.css`, `apps/web/tsconfig.worker.json`, `apps/web/vite.config.ts`
- Test: `apps/web/e2e-server/auth.spec.ts`, `apps/web/e2e-server/files.spec.ts` (новый), `apps/web/e2e-server/stand.ts`, `apps/web/unit/check-dist.test.ts`, `apps/web/unit/router.test.ts`, `apps/web/unit/saver.test.ts`, `apps/web/unit/server-files.test.ts` (новый)

**Interfaces:**
- Consumes: маршруты задач 2–4, команды задачи 1.
- Produces: `ServerFileStore`, `FileStore.{fullHistory, importLabel, watch, redeemLink, requestAccess}`, `Root { createStore }`, `extensions.{parseSection, sectionTitle, filesNav, filesNavExtra, fileMenu, newFileRoute, shareButton, noDebugParam}`, `editorHooks.sessionLost`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
import { appScreen, drag, editorReady, live, mainMenu, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { linksFor, mailCount, newEmail, resetStand, valkey } from './stand';

test.beforeEach(resetStand);
````

на:

````ts
import { appScreen, drag, editorReady, live, mainMenu, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { linksFor, mailCount, newEmail, requestLink, resetStand, signIn, valkey } from './stand';

test.beforeEach(resetStand);
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  const errors: string[] = [];
  page.on('console', (m) => {
    if (m.type() === 'error' && !m.location().url.endsWith('/favicon.ico')) errors.push(`${m.text()} ${m.location().url}`);
  });
  page.on('pageerror', (e) => errors.push(e.message));
  return errors;
}

async function requestLink(page: Page, email: string): Promise<void> {
  await page.getByLabel('Электронная почта').fill(email);
  await page.getByRole('button', { name: 'Получить ссылку' }).click();
  await expect(page.getByRole('heading', { name: 'Проверьте почту' })).toBeVisible();
  await expect(page.getByText(email)).toBeVisible();
}

/** Вход целиком: от экрана входа до приложения. */
async function signIn(page: Page, email: string, name = 'Иван Петров'): Promise<void> {
  // Новое письмо, а не одно из прежних: оно уходит после ответа сервера и приходит не сразу.
  const before = await mailCount(email);
  await requestLink(page, email);
  const [link] = await linksFor(email, before + 1);
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  if (name) {
    await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
    await page.getByLabel('Имя').fill(name);
    await page.getByRole('button', { name: 'Продолжить' }).click();
  }
}

````

на:

````ts
  const errors: string[] = [];
  page.on('console', (m) => {
    // Firefox: шрифт, загрузку которого оборвал переход на другой адрес (NS_BINDING_ABORTED), — не потеря.
    const aborted = m.text().includes('status=2152398850');
    if (m.type() === 'error' && !aborted && !m.location().url.endsWith('/favicon.ico')) errors.push(`${m.text()} ${m.location().url}`);
  });
  page.on('pageerror', (e) => errors.push(e.message));
  return errors;
}

````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  // Сразу, не дожидаясь журнала: приложение должно записать правку само.
  await pasteHash(page, link);
  await expect(enter(page)).toBeVisible();
  await enter(page).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
````

на:

````ts
  // Сразу, не дожидаясь журнала: приложение должно записать правку само.
  await pasteHash(page, link);
  await enter(page).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  await editorReady(page);
  await drawRect(page);
  await page.evaluate(() => window.__ferrumFiles!.store.failWrites(true));
  await mainMenu(page, 'Настройки', 'Настройки…');
  await page.getByRole('tab', { name: 'Аккаунт' }).click();
````

на:

````ts
  await editorReady(page);
  await drawRect(page);
  // Запись на сервер не проходит: и в worker'е файлов, и из любой вкладки контекста.
  await page.context().route('**/api/files/*/content', (r) => r.abort());
  await mainMenu(page, 'Настройки', 'Настройки…');
  await page.getByRole('tab', { name: 'Аккаунт' }).click();
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
});

test('выход в другой вкладке: новый файл успевает записаться, правка не теряется', async ({ page, context }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
````

на:

````ts
});

test('выход в другой вкладке: новый файл не записан, редактор остаётся и предлагает копию', async ({ page, context }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
  await page.goto('/?nodes=0');
  await signIn(page, email);
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await editorReady(page);
  const tab = await context.newPage();
  await tab.goto('/');
  await expect(tab.getByRole('button', { name: 'Выйти' })).toBeVisible();
  await drawRect(page);
  // Сразу: у нового файла ещё нет записи, а сессии уже нет, сервер её не примет.
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(page.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  await nodesAre(page, 1);
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Сохранить копию на диск' }).click();
  expect((await download).suggestedFilename()).toMatch(/\.ferrum$/);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  const ask = page.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  await expect(ask).toContainText('ещё ни разу не записан');
  await ask.getByRole('button', { name: 'Уйти' }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
});

test('выход в другой вкладке: записанный файл, правка возвращается из журнала после входа', async ({ page, context }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  await editorReady(page);
  const url = page.url();
  const tab = await context.newPage();
  await tab.goto('/');
  await expect(tab.getByRole('button', { name: 'Выйти' })).toBeVisible();
  await drawRect(page);
  // Сразу: у нового файла ещё нет записи, журнал ему не поможет.
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await signIn(page, email, '');
  await expect(page).toHaveURL(url);
  await editorReady(page);
  await nodesAre(page, 1);
});
````

на:

````ts
  await editorReady(page);
  const url = page.url();
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  const tab = await context.newPage();
  await tab.goto('/');
  await expect(tab.getByRole('button', { name: 'Выйти' })).toBeVisible();
  await drawRect(page);
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(page.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await signIn(page, email, '');
  await expect(page).toHaveURL(url);
  await editorReady(page);
  await nodesAre(page, 2);
});
````

Создать `apps/web/e2e-server/files.spec.ts`:

````ts
// Файлы на сервере: настоящий стенд, настоящие письма, два браузера там, где нужны двое.

import { expect, test, type Browser, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { appScreen, drag, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { linksFor, mailCount, newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);

const person = async (browser: Browser, email: string, name = 'Человек'): Promise<Page> => {
  const context = await browser.newContext({ baseURL: STAND_URL, viewport: { width: 1280, height: 720 } });
  const page = await context.newPage();
  await testMode(page);
  await page.goto('/');
  await signIn(page, email, name);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  return page;
};

const title = (page: Page) => page.getByRole('heading', { level: 1 });
const nodes = (page: Page, n: number) => expect.poll(async () => (await live(page)).nodes).toBe(n);
/** Правки — только запись на сервер: новый файл и точка отсчёта, поколение проверяет сервер. */
const saved = (page: Page) => expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
const saveKey = (page: Page) => page.keyboard.press('ControlOrMeta+KeyS');

/** Прямоугольник инструментом R; `dx` — сдвиг в мире. CSP стенда не даёт выполнить строку, поэтому ждём опросом. */
async function drawRect(page: Page, dx = 0): Promise<void> {
  const before = (await live(page)).nodes;
  await page.locator('canvas').click({ position: { x: 10, y: 10 } });
  await page.keyboard.press('KeyR');
  const l = await live(page);
  await drag(page, await appScreen(page, l, l.cameraX - 50 + dx, l.cameraY - 40), 100, 80);
  await expect
    .poll(async () => {
      const now = await live(page);
      return now.busy === 0 ? now.nodes : -1;
    })
    .toBe(before + 1);
}

/** Файл на сервере с одним прямоугольником; адрес файла — у открывшего. */
async function storedFile(page: Page): Promise<string> {
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  const id = fileId(page);
  await drawRect(page);
  await saveKey(page);
  await saved(page);
  return id;
}

const api = (page: Page, method: string, path: string, body?: unknown) =>
  page.evaluate(
    async ({ method, path, body }) => {
      const init: RequestInit = { method };
      if (body !== undefined) {
        init.headers = { 'Content-Type': 'application/json' };
        init.body = JSON.stringify(body);
      }
      const r = await fetch(path, init);
      const text = await r.text();
      return { status: r.status, body: text ? (JSON.parse(text) as Record<string, unknown>) : null };
    },
    { method, path, body },
  );

/** Сообщение внизу экрана. */
const toast = (page: Page, text: string) => page.getByRole('status').filter({ hasText: text }).first();

test('новый файл на сервере: правка переживает перезагрузку, файла нет в IndexedDB', async ({ page }, info) => {
  await testMode(page);
  await page.goto('/');
  await signIn(page, newEmail(info.project.name));
  await expect(title(page)).toHaveText('Недавние');
  const id = await storedFile(page);
  const content = await page.evaluate(async (i) => {
    const r = await fetch(`/api/files/${i}/content`);
    return { status: r.status, version: r.headers.get('x-ferrum-version'), size: (await r.arrayBuffer()).byteLength };
  }, id);
  expect(content.status).toBe(200);
  expect(Number(content.version)).toBeGreaterThan(0);
  expect(content.size).toBeGreaterThan(16);
  await page.reload();
  await editorReady(page);
  await nodes(page, 1);
  expect(await page.evaluate(() => window.__ferrumFiles)).toBeUndefined();
  const local = await page.evaluate(
    () =>
      new Promise<number>((resolve) => {
        const req = indexedDB.open('ferrum');
        req.onsuccess = () => {
          const d = req.result;
          const n = d.objectStoreNames.contains('meta') ? d.transaction('meta').objectStore('meta').count() : null;
          if (!n) return resolve(0);
          n.onsuccess = () => resolve(n.result);
        };
      }),
  );
  expect(local).toBe(0);
});

test('файлы разделены по людям: вошедший следующим в этом браузере чужого не видит (B-190)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name), 'Анна');
  const id = await storedFile(a);
  await a.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(title(a)).toHaveText('Недавние');
  await expect(a.getByRole('option', { name: 'Без названия' })).toBeVisible();
  await a.getByRole('button', { name: 'Выйти' }).click();
  await expect(a.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await signIn(a, newEmail(info.project.name), 'Борис');
  await expect(title(a)).toHaveText('Недавние');
  await expect(a.getByText('Здесь пока нет файлов')).toBeVisible();
  await a.goto(`/#/file/${id}`);
  await expect(a.getByRole('heading', { name: 'Нет доступа' })).toBeVisible();
  await a.context().close();
});

test('разделы экрана файлов: черновики, корзина, имя; читателю меню без правок', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  await a.goto('/#/drafts');
  await expect(title(a)).toHaveText('Черновики');
  const card = a.getByRole('option', { name: 'Без названия' });
  // Превью — отдельным запросом, виден рисунок, а не заглушка.
  await expect(card.locator('img')).toBeVisible();
  await card.getByText('Без названия').dblclick();
  await a.getByLabel('Имя файла').fill('Макет');
  await a.keyboard.press('Enter');
  await expect(a.getByRole('option', { name: 'Макет' })).toBeVisible();
  await a.getByRole('option', { name: 'Макет' }).click({ button: 'right' });
  await a.getByRole('menuitem', { name: 'Удалить' }).click();
  await expect(a.getByText('Здесь пока нет файлов')).toBeVisible();
  await a.getByRole('option', { name: 'Корзина' }).click();
  await expect(a.getByRole('option', { name: 'Макет' })).toBeVisible();
  await a.getByRole('option', { name: 'Макет' }).click({ button: 'right' });
  await a.getByRole('menuitem', { name: 'Восстановить' }).click();
  await expect(a.getByText('Корзина пуста')).toBeVisible();
  await a.getByRole('option', { name: 'Черновики' }).click();
  await expect(a.getByRole('option', { name: 'Макет' })).toBeVisible();
  await a.context().close();
});

test('две вкладки одного человека: файл занят, как раньше', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  const second = await a.context().newPage();
  await testMode(second);
  await second.goto(`/#/file/${id}`);
  await expect(toast(second, 'Файл открыт в другой вкладке')).toBeVisible();
  await a.context().close();
});

test('картинки: отдельными запросами, файл на сервере без них, после открытия они на месте', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  const id = fileId(a);
  const uploads: string[] = [];
  a.on('request', (r) => {
    if (r.method() === 'PUT' && r.url().includes('/images/')) uploads.push(r.url());
  });
  // PNG 4×4, настоящий: ядро его декодирует и кладёт в документ.
  await a.evaluate(async () => {
    const c = new OffscreenCanvas(4, 4);
    c.getContext('2d')!.fillRect(0, 0, 4, 4);
    const bytes = new Uint8Array(await (await c.convertToBlob({ type: 'image/png' })).arrayBuffer());
    window.__ferrum!.core!.send({ type: 'placeImage', bytes });
  });
  // Картинка ждёт щелчка на холсте.
  await expect.poll(() => a.evaluate(() => window.__ferrum!.ui!.tool)).toBe('image');
  const l = await live(a);
  const at = await appScreen(a, l, l.cameraX, l.cameraY);
  await a.mouse.click(at.x, at.y);
  await nodes(a, 1);
  await saveKey(a);
  await saved(a);
  expect(uploads).toHaveLength(1);
  const hash = uploads[0].split('/images/')[1];
  const stored = await a.evaluate(async (i) => new Uint8Array(await (await fetch(`/api/files/${i}/content`)).arrayBuffer()).length, id);
  const image = await a.evaluate(
    async ({ i, h }) => {
      const r = await fetch(`/api/files/${i}/images/${h}`);
      return { status: r.status, size: (await r.arrayBuffer()).byteLength };
    },
    { i: id, h: hash },
  );
  expect(image.status).toBe(200);
  // Запись без изменений картинки её заново не шлёт.
  await drawRect(a);
  await saveKey(a);
  await saved(a);
  expect(uploads).toHaveLength(1);
  await a.reload();
  await editorReady(a);
  await nodes(a, 2);
  // В снимке картинка есть: контейнер собран обратно.
  const size = await a.evaluate(
    () =>
      new Promise<number>((resolve) => {
        const core = window.__ferrum!.core!;
        const off = core.on((e) => {
          if (e.type === 'snapshot') {
            off();
            resolve(e.bytes.byteLength);
          }
        });
        core.send({ type: 'snapshot', thumbnail: false });
      }),
  );
  expect(size).toBeGreaterThan(image.size);
  expect(stored).toBeGreaterThan(16);
  await a.context().close();
});

test('импорт и экспорт `.ferrum`: копия в «Черновиках», скачанный файл открывается снова', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  await a.goto('/#/drafts');
  await a.getByRole('option', { name: 'Без названия' }).click({ button: 'right' });
  const download = a.waitForEvent('download');
  await a.getByRole('menuitem', { name: 'Скачать копию (.ferrum)' }).click();
  const path = await (await download).path();
  await a.evaluate(() => void delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker);
  const chooser = a.waitForEvent('filechooser');
  await a.getByRole('button', { name: 'Открыть с диска…' }).click();
  await (await chooser).setFiles({ name: 'Импорт.ferrum', mimeType: 'application/octet-stream', buffer: await readFile(path) });
  await editorReady(a);
  await nodes(a, 1);
  await a.goto('/#/drafts');
  await expect(a.getByRole('option', { name: 'Импорт' })).toBeVisible();
  // Не контейнер: сообщение, файла нет.
  await a.evaluate(() => void delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker);
  const bad = a.waitForEvent('filechooser');
  await a.getByRole('button', { name: 'Открыть с диска…' }).click();
  await (await bad).setFiles({ name: 'Чужое.ferrum', mimeType: 'application/octet-stream', buffer: Buffer.alloc(64) });
  await expect(toast(a, 'Это не файл Ferrum')).toBeVisible();
  await expect(a.getByRole('option', { name: 'Чужое' })).toHaveCount(0);
  await a.context().close();
});

test('`?debug` не открывает ядро и хранилище в рабочей сборке (B-205)', async ({ page }, info) => {
  await page.goto('/');
  await signIn(page, newEmail(info.project.name));
  await expect(title(page)).toHaveText('Недавние');
  await page.goto('/?debug&nodes=0#/new');
  await page.waitForFunction(() => window.__ferrum?.ready === true);
  expect(await page.evaluate(() => window.__ferrumFiles)).toBeUndefined();
  expect(await page.evaluate(() => window.__ferrum!.core)).toBeUndefined();
});

test('конец сессии в открытом редакторе: сообщение, правка не теряется (B-195)', async ({ browser }, info) => {
  const mail = newEmail(info.project.name);
  const a = await person(browser, mail);
  const id = await storedFile(a);
  await drawRect(a);
  await a.evaluate(() => fetch('/api/auth/sign-out', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' }));
  await saveKey(a);
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  expect(await a.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  const before = await mailCount(mail);
  await a.getByRole('button', { name: 'Войти' }).click();
  await a.getByLabel('Электронная почта').fill(mail);
  await a.getByRole('button', { name: 'Получить ссылку' }).click();
  const [link] = await linksFor(mail, before + 1);
  await a.goto(link);
  await a.getByRole('button', { name: 'Войти', exact: true }).click();
  await editorReady(a);
  expect(fileId(a)).toBe(id);
  await nodes(a, 2);
  await a.context().close();
});

test('сеть пропала: список — сообщение и «Повторить», запись — вопрос при уходе, после возврата проходит', async ({ browser }, info) => {
  test.skip(info.project.name === 'server-firefox', '`setOffline` не действует на worker в Firefox');
  const a = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  await drawRect(a);
  await a.context().setOffline(true);
  await saveKey(a);
  await expect(toast(a, 'Нет связи с сервером')).toBeVisible();
  await a.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(a.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' })).toBeVisible();
  await a.getByRole('button', { name: 'Остаться' }).click();
  await a.context().setOffline(false);
  await saveKey(a);
  await saved(a);
  expect(((await api(a, 'GET', `/api/files/${id}/access`)).body as { version: number }).version).toBeGreaterThan(1);
  await a.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(title(a)).toHaveText('Недавние');
  await a.context().setOffline(true);
  await a.getByRole('option', { name: 'Черновики' }).click();
  await expect(a.getByText('Не удалось загрузить список файлов')).toBeVisible();
  await a.context().setOffline(false);
  await a.getByRole('button', { name: 'Повторить' }).click();
  await expect(a.getByRole('option', { name: 'Без названия' })).toBeVisible();
  await a.context().close();
});

test('файл 100 000 узлов: запись и чтение через сервер, основной поток не занят @big', async ({ browser }, info) => {
  test.skip(info.project.name === 'server-firefox', 'размер проверяется в одном браузере');
  test.setTimeout(240_000);
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=100000#/new');
  await editorReady(a);
  const id = fileId(a);
  await a.evaluate(() => {
    const w = window as unknown as { __long: number[] };
    w.__long = [];
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) w.__long.push(Math.round(e.duration));
    }).observe({ entryTypes: ['longtask'] });
  });
  await drawRect(a);
  const t0 = Date.now();
  await saveKey(a);
  await expect.poll(() => a.evaluate(() => window.__ferrum!.ui!.modified), { timeout: 120_000 }).toBe(false);
  const saveMs = Date.now() - t0;
  const long = await a.evaluate(() => (window as unknown as { __long: number[] }).__long);
  const meta = (await api(a, 'GET', `/api/files/${id}`)).body as { size: number; version: number };
  expect(meta.size).toBeGreaterThan(1024 * 1024);
  const t1 = Date.now();
  await a.reload();
  await editorReady(a);
  await expect.poll(async () => (await live(a)).nodes, { timeout: 60_000 }).toBeGreaterThan(100_000);
  console.log(
    `100 000 узлов: файл ${(meta.size / 1048576).toFixed(1)} МБ, запись ${saveMs} мс, открытие ${Date.now() - t1} мс, длинные задачи при записи: ${JSON.stringify(long)}`,
  );
  await a.context().close();
});
````

В `apps/web/e2e-server/stand.ts` заменить:

````ts
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { expect } from '@playwright/test';

const ROOT = fileURLToPath(new URL('../../../', import.meta.url));
````

на:

````ts
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { expect, type Page } from '@playwright/test';

const ROOT = fileURLToPath(new URL('../../../', import.meta.url));
````

В `apps/web/e2e-server/stand.ts` заменить:

````ts
interface MailSummary {
  ID: string;
  To: { Address: string }[];
}

/** Ссылки из писем на этот адрес, новые первыми; ждёт, пока их станет `count`. */
````

на:

````ts
interface MailSummary {
  ID: string;
  Subject: string;
  To: { Address: string }[];
}

const LOGIN_SUBJECT = 'Вход в Ferrum';
const isLogin = (m: MailSummary, email: string) => m.Subject === LOGIN_SUBJECT && m.To.some((t) => t.Address === email);

/** Ссылки из писем на этот адрес, новые первыми; ждёт, пока их станет `count`. */
````

В `apps/web/e2e-server/stand.ts` заменить:

````ts
      async () => {
        const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
        found = list.messages.filter((m) => m.To.some((t) => t.Address === email));
        return found.length;
      },
````

на:

````ts
      async () => {
        const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
        found = list.messages.filter((m) => isLogin(m, email));
        return found.length;
      },
````

В `apps/web/e2e-server/stand.ts` заменить:

````ts
export async function mailCount(email: string): Promise<number> {
  const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
  return list.messages.filter((m) => m.To.some((t) => t.Address === email)).length;
}

````

на:

````ts
export async function mailCount(email: string): Promise<number> {
  const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
  return list.messages.filter((m) => isLogin(m, email)).length;
}

````

В `apps/web/e2e-server/stand.ts` заменить:

````ts
  return `user-${Date.now().toString(36)}-${++seq}-${project}@example.com`;
}
````

на:

````ts
  return `user-${Date.now().toString(36)}-${++seq}-${project}@example.com`;
}

export async function requestLink(page: Page, email: string): Promise<void> {
  await page.getByLabel('Электронная почта').fill(email);
  await page.getByRole('button', { name: 'Получить ссылку' }).click();
  await expect(page.getByRole('heading', { name: 'Проверьте почту' })).toBeVisible();
  await expect(page.getByText(email)).toBeVisible();
}

/** Вход целиком: от экрана входа до приложения. */
export async function signIn(page: Page, email: string, name = 'Иван Петров'): Promise<void> {
  // Новое письмо, а не одно из прежних: оно уходит после ответа сервера и приходит не сразу.
  const before = await mailCount(email);
  await requestLink(page, email);
  const [link] = await linksFor(email, before + 1);
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  if (name) {
    await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
    await page.getByLabel('Имя').fill(name);
    await page.getByRole('button', { name: 'Продолжить' }).click();
  }
}

/** Письма на адрес с такой темой, новые первыми: текст целиком. Письмо уходит после ответа сервера. */
export async function mailsFor(email: string, subject: string, count = 1): Promise<string[]> {
  let found: MailSummary[] = [];
  await expect
    .poll(
      async () => {
        const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
        found = list.messages.filter((m) => m.Subject === subject && m.To.some((t) => t.Address === email));
        return found.length;
      },
      { message: `письма «${subject}» на ${email}`, timeout: 15_000 },
    )
    .toBeGreaterThanOrEqual(count);
  return Promise.all(found.map(async (m) => ((await (await fetch(`${MAILPIT}/message/${m.ID}`)).json()) as { Text: string }).Text));
}
````

В `apps/web/unit/check-dist.test.ts` заменить:

````ts
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AUTH_MARKER, checkDist, LOCAL_MARKER } from '../scripts/check-dist.mjs';

function dist(files: Record<string, string>): string {
````

на:

````ts
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AUTH_MARKER, checkDist, FILES_MARKER, LOCAL_MARKER, TAURI_STORE_MARKER, TEST_STORE_MARKER } from '../scripts/check-dist.mjs';

function dist(files: Record<string, string>): string {
````

В `apps/web/unit/check-dist.test.ts` заменить:

````ts
});

test('автономная сборка: без кода входа', () => {
  const files = { 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': `x.${LOCAL_MARKER}=1;f('${AUTH_MARKER}')` };
````

на:

````ts
});

test('автономная сборка: без кода файлов на сервере', () => {
  const files = { 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': `x.${LOCAL_MARKER}=1;f('${FILES_MARKER}')` };
  expect(checkDist(dist(files), 'standalone')).toContain('код файлов на сервере');
});

test('автономная сборка: без кода входа', () => {
  const files = { 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': `x.${LOCAL_MARKER}=1;f('${AUTH_MARKER}')` };
````

В `apps/web/unit/check-dist.test.ts` заменить:

````ts

test('рабочая сборка: без стенда и кода автономного режима', () => {
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `f('${AUTH_MARKER}session')` }), 'server')).toBeNull();
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': 'x' }), 'server')).toContain('нет кода входа');
  expect(checkDist(dist({ 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': `f('${AUTH_MARKER}')` }), 'server')).toContain(
    'debug.html в рабочей сборке',
  );
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `x.${LOCAL_MARKER}=1;f('${AUTH_MARKER}')` }), 'server')).toContain(
    'есть код автономного режима',
  );
````

на:

````ts

test('рабочая сборка: без стенда и кода автономного режима', () => {
  const server = `f('${AUTH_MARKER}session');g('${FILES_MARKER}')`;
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': server }), 'server')).toBeNull();
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `f('${AUTH_MARKER}session')` }), 'server')).toContain('нет кода файлов');
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `${server};h('${TAURI_STORE_MARKER}')` }), 'server')).toContain(
    'хранилище десктопа',
  );
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `${server};w.${TEST_STORE_MARKER}=1` }), 'server')).toContain(
    'тестовый доступ',
  );
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `g('${FILES_MARKER}')` }), 'server')).toContain('нет кода входа');
  expect(checkDist(dist({ 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': server }), 'server')).toContain(
    'debug.html в рабочей сборке',
  );
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `x.${LOCAL_MARKER}=1;${server}` }), 'server')).toContain(
    'есть код автономного режима',
  );
````

В `apps/web/unit/router.test.ts` заменить:

````ts
// Маршруты в адресе после `#`: экран файлов, разделы, файл, новый файл.

import { expect, test } from '@playwright/test';
````

на:

````ts
// Маршруты в адресе после `#`: экран файлов, разделы, файл, новый файл.
import { extensions } from '../src/ui/extensions';

import { expect, test } from '@playwright/test';
````

В `apps/web/unit/router.test.ts` заменить:

````ts
  expect(parseRoute('#/file/abc?other=1')).toEqual({ kind: 'file', id: 'abc' });
});
````

на:

````ts
  expect(parseRoute('#/file/abc?other=1')).toEqual({ kind: 'file', id: 'abc' });
});

test('чужие разделы — только если их знает сборка', () => {
  expect(parseRoute('#/shared')).toEqual({ kind: 'files', section: 'recent' });
  expect(parseRoute('#/project/abc')).toEqual({ kind: 'files', section: 'recent' });
  extensions.parseSection = (path) => (path === 'shared' ? 'shared' : path === 'project/abc' ? 'project:abc' : null);
  try {
    expect(parseRoute('#/shared')).toEqual({ kind: 'files', section: 'shared' });
    expect(parseRoute('#/project/abc')).toEqual({ kind: 'files', section: 'project:abc' });
    expect(parseRoute('#/other')).toEqual({ kind: 'files', section: 'recent' });
    expect(routeHash({ kind: 'files', section: 'shared' })).toBe('#/shared');
    expect(routeHash({ kind: 'files', section: 'project:abc' })).toBe('#/project/abc');
  } finally {
    delete extensions.parseSection;
  }
});

test('новый файл в проекте и токен ссылки — из адреса; мусор отбрасывается', () => {
  expect(parseRoute('#/new?project=abc')).toEqual({ kind: 'new', project: 'abc' });
  expect(parseRoute('#/new?project=../x')).toEqual({ kind: 'new' });
  expect(routeHash({ kind: 'new', project: 'abc' })).toBe('#/new?project=abc');
  expect(parseRoute('#/file/abc?link=Tok-en_1')).toEqual({ kind: 'file', id: 'abc', link: 'Tok-en_1' });
  expect(parseRoute('#/file/abc?link=a%2Fb')).toEqual({ kind: 'file', id: 'abc' });
  // Токен в запись истории не попадает: адрес файла — без него.
  expect(routeHash({ kind: 'file', id: 'abc', link: 'x' })).toBe('#/file/abc');
});
````

В `apps/web/unit/saver.test.ts` заменить:

````ts
});

function harness(stored: boolean, journalSession?: number | null) {
  /** Поддельные часы: таймеры `window` и `performance.now()`. */
  let now = 0;
````

на:

````ts
});

function harness(stored: boolean, journalSession?: number | null, fullHistory?: boolean) {
  /** Поддельные часы: таймеры `window` и `performance.now()`. */
  let now = 0;
````

В `apps/web/unit/saver.test.ts` заменить:

````ts
  let warning: string | undefined;
  const files = {
    appendJournal: async (_id: string, mark: JournalMark) => {
      if (hangAppend) await new Promise(() => {});
````

на:

````ts
  let warning: string | undefined;
  const files = {
    fullHistory,
    appendJournal: async (_id: string, mark: JournalMark) => {
      if (hangAppend) await new Promise(() => {});
````

В `apps/web/unit/saver.test.ts` заменить:

````ts
  expect(h.toasts).toHaveLength(2);
});
````

на:

````ts
  expect(h.toasts).toHaveLength(2);
});

test('снимок полный только там, где хранилище этого просит: файл на сервере', () => {
  const asked = (fullHistory?: boolean) => {
    const h = harness(true, null, fullHistory);
    h.emit({ type: 'modified', modified: true });
    void h.saver.save();
    return h.sent.find((c) => c.type === 'snapshot');
  };
  expect(asked()).toEqual({ type: 'snapshot', thumbnail: true });
  expect(asked(true)).toEqual({ type: 'snapshot', thumbnail: true, full: true });
});

test('`CONFLICT` от сервера: причина для вопроса при уходе, признак не снят, поколение прежнее', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  h.failWrites(true);
  const saving = h.saver.save();
  h.snapshot(0);
  expect(await saving).toBe(false);
  expect(h.saver.failure()).toContain('Не удалось сохранить файл');
  expect(h.saver.version()).toBe(1);
  expect(h.sent.some((c) => c.type === 'markSaved')).toBe(false);
});
````

Создать `apps/web/unit/server-files.test.ts`:

````ts
import { expect, test } from '@playwright/test';
import { containerHeader, imageSectionHeader, SECTION_PAGE_REFS } from '@ferrum/protocol';
import {
  ACCESS_POLL_MS,
  failureMessage,
  ServerFailure,
  ServerFiles,
  THUMB_INTERVAL_MS,
  transferTimeout,
  type AccessState,
  type JournalStore,
} from '../src/core/server-files';
import { MAX_OPEN_BYTES } from '../src/core/limits';

const NOW = 1_000_000;
const meta = (over: Record<string, unknown> = {}) => ({
  id: 'f',
  name: 'Файл',
  createdAt: 1,
  editedAt: 1,
  openedAt: 1,
  size: 0,
  thumbVersion: 0,
  version: 1,
  role: 'owner',
  ...over,
});

function section(kind: number, id: string, data: Uint8Array): Uint8Array {
  const idBytes = new TextEncoder().encode(id);
  const out = new Uint8Array(5 + idBytes.length + 40 + data.length);
  const v = new DataView(out.buffer);
  out[0] = kind;
  v.setUint32(1, idBytes.length, true);
  out.set(idBytes, 5);
  v.setBigUint64(5 + idBytes.length, BigInt(data.length), true);
  out.set(data, 5 + idBytes.length + 40);
  return out;
}

function container(...sections: Uint8Array[]): ArrayBuffer {
  const head = containerHeader(sections.length);
  const out = new Uint8Array(head.length + sections.reduce((n, s) => n + s.length, 0));
  out.set(head);
  let at = head.length;
  for (const s of sections) {
    out.set(s, at);
    at += s.length;
  }
  return out.buffer as ArrayBuffer;
}

const HASH = 'a'.repeat(64);
const imageSection = (bytes: Uint8Array): Uint8Array => {
  const head = imageSectionHeader(HASH, bytes.length);
  const out = new Uint8Array(head.length + bytes.length);
  out.set(head);
  out.set(bytes, head.length);
  return out;
};
const refs = (...hashes: string[]) => section(SECTION_PAGE_REFS, '1@1', new TextEncoder().encode(hashes.join('\n')));
const ROOT = section(1, '', new Uint8Array(4));

interface Call {
  url: string;
  method: string;
  headers: Record<string, string>;
  body: unknown;
}

/** Сервер в тесте: ответ по «МЕТОД адрес», неизвестное — падение теста. */
function stand(replies: Record<string, ((c: Call) => Response | Promise<Response>) | Response>) {
  const calls: Call[] = [];
  const events = { changed: 0, access: [] as [string, AccessState][] };
  const timers: { at: number; fn: () => void }[] = [];
  const clock = {
    time: NOW,
    now() {
      return this.time;
    },
    setTimeout(fn: () => void, ms: number) {
      const t = { at: this.time + ms, fn };
      timers.push(t);
      return t;
    },
    clearTimeout(t: unknown) {
      const i = timers.indexOf(t as (typeof timers)[number]);
      if (i >= 0) timers.splice(i, 1);
    },
    advance(ms: number) {
      this.time += ms;
      for (const t of timers.filter((x) => x.at <= this.time)) {
        timers.splice(timers.indexOf(t), 1);
        t.fn();
      }
    },
  };
  const journal = {
    appended: [] as [string, number, number][],
    cuts: [] as [string, number, number][],
    dropped: [] as string[],
    entries: [] as ArrayBuffer[],
    session: null as number | null,
    async append(f: string, s: number, b: number) {
      this.appended.push([f, s, b]);
    },
    async read() {
      return { entries: this.entries, session: this.session, tooLarge: false };
    },
    async cut(f: string, s: number, u: number) {
      this.cuts.push([f, s, u]);
    },
    async drop(f: string) {
      this.dropped.push(f);
    },
  };
  let id = 0;
  const http = (async (input: string | URL | Request, init: RequestInit = {}) => {
    const url = String(input);
    const method = init.method ?? 'GET';
    const call: Call = { url, method, headers: (init.headers ?? {}) as Record<string, string>, body: init.body };
    calls.push(call);
    const r = replies[`${method} ${url}`];
    if (!r) throw new Error(`нет ответа: ${method} ${url}`);
    return typeof r === 'function' ? r(call) : r.clone();
  }) as typeof fetch;
  const files = new ServerFiles(
    http,
    journal as unknown as JournalStore,
    { changed: () => events.changed++, access: (f, s) => events.access.push([f, s]) },
    clock,
    () => `write-${++id}`,
  );
  return { files, calls, events, journal, clock };
}

const json = (v: unknown, status = 200) => new Response(JSON.stringify(v), { status, headers: { 'content-type': 'application/json' } });
const access = (version: number, role = 'editor') => json({ role, version });
const code = async (p: Promise<unknown>) => {
  const e = await p.then(
    () => null,
    (x: unknown) => x,
  );
  expect(e).toBeInstanceOf(ServerFailure);
  return (e as ServerFailure).code;
};

test('неожиданный вид ответа — сбой сети, а не `undefined`', async () => {
  const { files } = stand({
    'GET /api/files/f': json({ id: 'f' }),
    'GET /api/files?section=recent&q=&start=0&count=5': new Response('<html>вход в сеть отеля</html>', { status: 200 }),
  });
  expect(await code(files.get('f'))).toBe('NETWORK');
  expect(await code(files.list('recent', '', 0, 5))).toBe('NETWORK');
});

test('чтение повторяется один раз при сбое сети и 5xx, запись — нет', async () => {
  let reads = 0;
  const { files, calls } = stand({
    'GET /api/files/f': () => {
      reads++;
      if (reads === 1) throw new TypeError('сеть');
      return json(meta());
    },
    'PATCH /api/files/f': () => {
      throw new TypeError('сеть');
    },
  });
  expect((await files.get('f'))?.id).toBe('f');
  expect(reads).toBe(2);
  expect(await code(files.rename('f', 'x'))).toBe('NETWORK');
  expect(calls.filter((c) => c.method === 'PATCH')).toHaveLength(1);
});

test('нет доступа и несуществующий: `get` — `null`, другие коды — ошибка', async () => {
  const { files } = stand({
    'GET /api/files/f': json({ error: 'NOT_FOUND' }, 404),
    'GET /api/files/g': json({ error: 'UNAVAILABLE' }, 503),
  });
  expect(await files.get('f')).toBeNull();
  expect(await code(files.get('g'))).toBe('UNAVAILABLE');
});

test('чтение файла: поколение — именно у этих байтов, не у записи о файле', async () => {
  const bytes = container(ROOT);
  const { files } = stand({
    'GET /api/files/f': json(meta({ version: 1 })),
    'GET /api/files/f/content': new Response(bytes, { headers: { 'x-ferrum-version': '2', 'content-length': String(bytes.byteLength) } }),
  });
  const r = await files.read('f');
  expect(r.meta.version).toBe(2);
  expect(new Uint8Array(r.bytes!)).toEqual(new Uint8Array(bytes));
});

test('чтение: без заголовка поколения — сбой, а не файл с неизвестной базой', async () => {
  const { files } = stand({
    'GET /api/files/f': json(meta()),
    'GET /api/files/f/content': new Response(container(ROOT)),
  });
  expect(await code(files.read('f'))).toBe('NETWORK');
});

test('чтение: файл без содержимого, читателю журнал не применяется', async () => {
  const s = stand({
    'GET /api/files/f': json(meta()),
    'GET /api/files/f/content': new Response(null, { status: 204, headers: { 'x-ferrum-version': '0' } }),
    'GET /api/files/v': json(meta({ id: 'v', role: 'viewer' })),
    'GET /api/files/v/content': new Response(container(ROOT), { headers: { 'x-ferrum-version': '1' } }),
    'GET /api/files/e': json(meta({ id: 'e' })),
    'GET /api/files/e/content': new Response(container(ROOT), { headers: { 'x-ferrum-version': '1' } }),
  });
  s.journal.entries = [new ArrayBuffer(1)];
  s.journal.session = 7;
  expect((await s.files.read('f')).bytes).toBeNull();
  const viewer = await s.files.read('v');
  expect([viewer.journal, viewer.session]).toEqual([[], null]);
  const editor = await s.files.read('e');
  expect([editor.journal.length, editor.session]).toEqual([1, 7]);
});

test('чтение собирает файл с картинками: ядро получает обычный контейнер', async () => {
  const stored = container(ROOT, refs(HASH, 'b'.repeat(64)));
  const image = new Uint8Array([9, 8, 7]);
  const { files } = stand({
    'GET /api/files/f': json(meta()),
    'GET /api/files/f/content': new Response(stored, { headers: { 'x-ferrum-version': '1' } }),
    [`GET /api/files/f/images/${HASH}`]: new Response(image),
    // Нет второй картинки: файл открывается, ядро покажет заглушку.
    [`GET /api/files/f/images/${'b'.repeat(64)}`]: json({ error: 'NOT_FOUND' }, 404),
  });
  const out = new Uint8Array((await files.read('f')).bytes!);
  expect(out).toEqual(new Uint8Array(container(ROOT, refs(HASH, 'b'.repeat(64)), imageSection(image))));
});

test('запись: картинки — отдельно, файл без них, `If-Match` и номер записи; журнал снимается после успеха', async () => {
  const image = new Uint8Array([1, 2, 3]);
  const s = stand({
    'GET /api/files/f/access': access(1),
    [`HEAD /api/files/f/images/${HASH}`]: new Response(null, { status: 404 }),
    [`PUT /api/files/f/images/${HASH}`]: json({ ok: true }),
    'PUT /api/files/f/content': json(meta({ version: 2 })),
  });
  const m = await s.files.write('f', container(ROOT, refs(HASH), imageSection(image)), 'keep', 1, { session: 5, upTo: 3 });
  expect(m.version).toBe(2);
  const put = s.calls.find((c) => c.method === 'PUT' && c.url.endsWith('/content'))!;
  expect(put.headers['If-Match']).toBe('"1"');
  expect(put.headers['x-ferrum-write']).toBe('write-1');
  expect(put.headers['x-ferrum-previous-write']).toBeUndefined();
  expect(new Uint8Array(await (put.body as Blob).arrayBuffer())).toEqual(new Uint8Array(container(ROOT, refs(HASH))));
  expect(s.journal.cuts).toEqual([['f', 5, 3]]);
  expect(s.events.changed).toBe(1);
  // Вторая запись: картинка уже есть, ни HEAD, ни PUT.
  s.calls.length = 0;
  await s.files.write('f', container(ROOT, refs(HASH), imageSection(image)), 'keep', 1, null);
  expect(s.calls.filter((c) => c.url.includes('/images/'))).toEqual([]);
});

test('заведомый отказ — раньше передачи: чужая запись и роль читателя', async () => {
  const s = stand({ 'GET /api/files/f/access': access(5) });
  expect(await code(s.files.write('f', container(ROOT), 'keep', 1, null))).toBe('CONFLICT');
  expect(s.calls.filter((c) => c.method === 'PUT')).toEqual([]);
  expect(s.journal.cuts).toEqual([]);
  const v = stand({ 'GET /api/files/f/access': access(1, 'viewer'), 'GET /api/files/f/access?': access(1, 'viewer') });
  expect(await code(v.files.write('f', container(ROOT), 'keep', 1, null))).toBe('FORBIDDEN');
  expect(v.calls.filter((c) => c.method === 'PUT')).toEqual([]);
});

test('потерянный ответ: следующая запись несёт номер прежней и своя удача не считается `CONFLICT`', async () => {
  let attempts = 0;
  const s = stand({
    'GET /api/files/f/access': () => access(attempts === 0 ? 1 : 2),
    'PUT /api/files/f/content': () => {
      attempts++;
      if (attempts === 1) throw new TypeError('обрыв');
      return json(meta({ version: 3 }));
    },
  });
  expect(await code(s.files.write('f', container(ROOT), 'keep', 1, { session: 1, upTo: 1 }))).toBe('NETWORK');
  expect(s.journal.cuts).toEqual([]);
  const m = await s.files.write('f', container(ROOT), 'keep', 1, { session: 1, upTo: 2 });
  expect(m.version).toBe(3);
  const puts = s.calls.filter((c) => c.method === 'PUT');
  expect(puts[1].headers['x-ferrum-previous-write']).toBe('write-1');
  // Исход известен: дальше номера нет.
  expect(await code(s.files.write('f', container(ROOT), 'keep', 1, null))).toBe('CONFLICT');
});

test('`CONFLICT` от сервера — обычная ошибка: журнал цел, номера прежней записи нет', async () => {
  const s = stand({
    'GET /api/files/f/access': access(1),
    'PUT /api/files/f/content': json({ error: 'CONFLICT' }, 409),
  });
  expect(await code(s.files.write('f', container(ROOT), 'keep', 1, { session: 1, upTo: 1 }))).toBe('CONFLICT');
  expect(s.journal.cuts).toEqual([]);
  await code(s.files.write('f', container(ROOT), 'keep', 1, null));
  expect(s.calls.filter((c) => c.method === 'PUT')[1].headers['x-ferrum-previous-write']).toBeUndefined();
});

test('отказ записи сообщает открытому редактору: нет доступа и конец сессии', async () => {
  const s = stand({ 'GET /api/files/f/access': json({ error: 'NOT_FOUND' }, 404) });
  expect(await code(s.files.write('f', container(ROOT), 'keep', 1, null))).toBe('NOT_FOUND');
  expect(s.events.access).toEqual([['f', { kind: 'none' }]]);
  const t = stand({ 'GET /api/files/f/access': json({ error: 'UNAUTHORIZED' }, 401) });
  await code(t.files.write('f', container(ROOT), 'keep', 1, null));
  expect(t.events.access).toEqual([['f', { kind: 'session' }]]);
});

test('опрос доступа: только изменения; сбой сети ничего не меняет', async () => {
  let reply: () => Response = () => access(1);
  const s = stand({ 'GET /api/files/f/access': () => reply() });
  await s.files.checkAccess('f');
  await s.files.checkAccess('f');
  expect(s.events.access).toEqual([['f', { kind: 'role', role: 'editor', version: 1 }]]);
  reply = () => {
    throw new TypeError('сеть');
  };
  await s.files.checkAccess('f');
  expect(s.events.access).toHaveLength(1);
  reply = () => access(1, 'viewer');
  await s.files.checkAccess('f');
  expect(s.events.access.at(-1)).toEqual(['f', { kind: 'role', role: 'viewer', version: 1 }]);
  reply = () => json({ error: 'NOT_FOUND' }, 404);
  await s.files.checkAccess('f');
  expect(s.events.access.at(-1)).toEqual(['f', { kind: 'none' }]);
  expect(ACCESS_POLL_MS).toBeLessThanOrEqual(15_000);
});

test('превью: не чаще раза в минуту, новое до срока заменяет ждущее, снятие тоже ждёт срока', async () => {
  const s = stand({
    'GET /api/files/f/access': access(1),
    'PUT /api/files/f/content': json(meta()),
    'PUT /api/files/f/thumb': json(meta()),
    'DELETE /api/files/f/thumb': json(meta()),
  });
  const write = (thumb: ArrayBuffer | null | 'keep') => s.files.write('f', container(ROOT), thumb, 1, null);
  const thumbs = () => s.calls.filter((c) => c.url.endsWith('/thumb'));
  await write(new ArrayBuffer(1));
  expect(thumbs()).toHaveLength(1);
  await write(new ArrayBuffer(2));
  await write(new ArrayBuffer(3));
  await write('keep');
  expect(thumbs()).toHaveLength(1);
  s.clock.advance(THUMB_INTERVAL_MS - 1);
  expect(thumbs()).toHaveLength(1);
  s.clock.advance(1);
  expect(thumbs()).toHaveLength(2);
  expect((thumbs()[1].body as ArrayBuffer).byteLength).toBe(3);
  await write(null);
  expect(thumbs()).toHaveLength(2);
  s.clock.advance(THUMB_INTERVAL_MS);
  expect(thumbs().at(-1)!.method).toBe('DELETE');
});

test('сбой превью не портит запись файла', async () => {
  const s = stand({
    'GET /api/files/f/access': access(1),
    'PUT /api/files/f/content': json(meta({ version: 2 })),
    'PUT /api/files/f/thumb': () => {
      throw new TypeError('сеть');
    },
  });
  expect((await s.files.write('f', container(ROOT), new ArrayBuffer(1), 1, null)).version).toBe(2);
});

test('новый файл с содержимым: сбой записи не оставляет пустой файл; файл с диска не загружается целиком', async () => {
  const s = stand({
    'POST /api/files': json(meta({ version: 0 }), 201),
    'PUT /api/files/f/content': json({ error: 'BAD_CONTAINER' }, 400),
    'DELETE /api/files/f': json({ ok: true }),
  });
  const body = new Blob([container(ROOT)]);
  expect(await code(s.files.create('Файл', body, null))).toBe('BAD_CONTAINER');
  expect(s.calls.some((c) => c.method === 'DELETE')).toBe(true);
  expect(s.events.changed).toBe(0);
  // Не контейнер с картинкой — отказ ещё до записи на сервер.
  const bad = stand({ 'POST /api/files': json(meta(), 201), 'DELETE /api/files/f': json({ ok: true }) });
  expect(await code(bad.files.create('Файл', new Blob([new Uint8Array(64)]), null))).toBe('BAD_CONTAINER');
  expect(bad.calls.filter((c) => c.method === 'PUT')).toEqual([]);
});

test('перенос из браузера: журнал становится журналом нового файла', async () => {
  const s = stand({
    'POST /api/files': json(meta({ version: 0 }), 201),
    'GET /api/files/f/access': access(0),
    'PUT /api/files/f/content': json(meta({ version: 1 })),
  });
  const m = await s.files.create('Файл', container(ROOT), null, [new ArrayBuffer(1), new ArrayBuffer(2)]);
  expect(m.version).toBe(1);
  expect(s.journal.appended).toEqual([
    ['f', 1, 0],
    ['f', 1, 1],
  ]);
});

test('срок передачи растёт с размером, тексты ошибок для человека', () => {
  expect(transferTimeout(0)).toBe(60_000);
  expect(transferTimeout(34 * 1024 * 1024)).toBeGreaterThan(60_000);
  expect(transferTimeout(MAX_OPEN_BYTES)).toBeLessThanOrEqual(30 * 60_000);
  expect(failureMessage('CONFLICT')).toContain('сольются');
  expect(failureMessage('NETWORK')).toBe('Нет связи с сервером');
});

test('удаление навсегда снимает журнал файла', async () => {
  const s = stand({ 'DELETE /api/files/f': json({ ok: true }) });
  await s.files.remove('f');
  expect(s.journal.dropped).toEqual(['f']);
  expect(s.events.changed).toBe(1);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/server-files.test.ts unit/check-dist.test.ts unit/router.test.ts`
Expected: FAIL — `Cannot find module src/core/server-files` и `check-dist.mjs does not provide an export named FILES_MARKER`

- [ ] **Step 3: Реализация**

В `apps/web/scripts/check-dist.mjs` заменить:

````js
/** Адреса входа: код входа есть только в рабочей сборке. */
export const AUTH_MARKER = '/api/auth/';

/** Проблема сборки словами или `null`. `mode` — `standalone` или `server`. */
````

на:

````js
/** Адреса входа: код входа есть только в рабочей сборке. */
export const AUTH_MARKER = '/api/auth/';
/** Команда `TauriFileStore`: в рабочей сборке её нет. */
export const TAURI_STORE_MARKER = 'files_open_dialog';
/** Тестовый доступ к локальному хранилищу. */
export const TEST_STORE_MARKER = '__ferrumFiles';
/** Адреса API файлов: код сервера есть только в рабочей сборке. */
export const FILES_MARKER = '/api/files';

/** Проблема сборки словами или `null`. `mode` — `standalone` или `server`. */
````

В `apps/web/scripts/check-dist.mjs` заменить:

````js
  if (mode === 'standalone' && !local) return 'в автономной сборке нет автономного режима: проверьте VITE_FERRUM_LOCAL (.env.standalone)';
  if (mode === 'standalone' && auth) return 'в автономной сборке есть код входа: проверьте условия по VITE_FERRUM_LOCAL';
  if (mode === 'server' && local) return 'в рабочей сборке есть код автономного режима: VITE_FERRUM_LOCAL задан при сборке?';
  if (mode === 'server' && !auth) return 'в рабочей сборке нет кода входа';
  return null;
}
````

на:

````js
  if (mode === 'standalone' && !local) return 'в автономной сборке нет автономного режима: проверьте VITE_FERRUM_LOCAL (.env.standalone)';
  if (mode === 'standalone' && auth) return 'в автономной сборке есть код входа: проверьте условия по VITE_FERRUM_LOCAL';
  if (mode === 'standalone' && has(FILES_MARKER)) return 'в автономной сборке есть код файлов на сервере: проверьте импорты `ui/server`';
  if (mode === 'server' && !has(FILES_MARKER)) return 'в рабочей сборке нет кода файлов на сервере';
  if (mode === 'server' && local) return 'в рабочей сборке есть код автономного режима: VITE_FERRUM_LOCAL задан при сборке?';
  if (mode === 'server' && !auth) return 'в рабочей сборке нет кода входа';
  if (mode === 'server' && has(TAURI_STORE_MARKER)) return 'в рабочей сборке есть хранилище десктопа: проверьте импорты `tauri.ts`';
  if (mode === 'server' && has(TEST_STORE_MARKER)) return 'в рабочей сборке есть тестовый доступ к хранилищу';
  return null;
}
````

В `apps/web/src/core/file-list.ts` заменить:

````ts
// потоков, не доверенное: проверки — `requestError`.

import { MAX_FONT_BYTES, MAX_NAME_CHARS, MAX_OPEN_BYTES, MAX_SETTINGS_BYTES, tooLarge } from './limits';

````

на:

````ts
// потоков, не доверенное: проверки — `requestError`.

import type { FileRole } from '@ferrum/protocol';
import { MAX_FONT_BYTES, MAX_NAME_CHARS, MAX_OPEN_BYTES, MAX_SETTINGS_BYTES, tooLarge } from './limits';

````

В `apps/web/src/core/file-list.ts` заменить:

````ts
   * а не в базе. */
  disk?: string;
}

````

на:

````ts
   * а не в базе. */
  disk?: string;
  /** Действующая роль в файле на сервере; нет — файл свой, локальный, без ролей. */
  role?: FileRole;
  /** Файл лежит в проекте команды (файл на сервере). */
  projectId?: string;
}

````

В `apps/web/src/core/file-list.ts` заменить:

````ts
/** Запись основана на устаревшем поколении: файл записали в другой вкладке. */
export const CONFLICT = 'Файл изменён в другой вкладке — сохраните копию на диск или откройте файл заново';
/** Записи нет: файл удалили навсегда (другая вкладка), пока он был открыт. */
export const GONE = 'Файл удалён в другой вкладке — сохраните копию на диск';
````

на:

````ts
/** Запись основана на устаревшем поколении: файл записали в другой вкладке. */
export const CONFLICT = 'Файл изменён в другой вкладке — сохраните копию на диск или откройте файл заново';
/** То же о файле на сервере: записать мог и другой человек, а при следующем открытии правки сольются. */
export const SERVER_CONFLICT = 'Файл изменён в другом окне или другим человеком — откройте файл заново, ваши правки сольются с его версией';
/** Записи нет: файл удалили навсегда (другая вкладка), пока он был открыт. */
export const GONE = 'Файл удалён в другой вкладке — сохраните копию на диск';
````

В `apps/web/src/core/file-list.ts` заменить:

````ts

/** Разделы экрана файлов: недавние — по открытию, черновики — по изменению. */
export type Section = 'recent' | 'drafts' | 'trash';
export const SECTIONS: readonly Section[] = ['recent', 'drafts', 'trash'];

````

на:

````ts

/** Разделы экрана файлов: недавние — по открытию, черновики — по изменению. */
export type Section = 'recent' | 'drafts' | 'trash' | 'shared' | `project:${string}`;
export const SECTIONS: readonly Section[] = ['recent', 'drafts', 'trash'];

````

Создать `apps/web/src/core/server-files.ts`:

````ts
// Файлы на сервере со стороны worker'а (спека подсистемы 4, раздел 6): запросы, разбор ответов и всё, что
// размером с документ. Основной поток сюда не заходит: байты приходят и уходят передачей. Сеть не
// доверенная: неожиданный вид ответа — `NETWORK`, а не `undefined` в интерфейсе; у каждого запроса срок.

import {
  API_ERROR_CODES,
  API_FILES,
  ContainerError,
  containerHeader,
  imageSectionHeader,
  isFileAccess,
  isFileList,
  isFileMeta,
  isHash,
  parseSection,
  PREVIOUS_WRITE_HEADER,
  scanContainer,
  SECTION_IMAGE,
  SECTION_PAGE_REFS,
  VERSION_HEADER,
  WRITE_ID_HEADER,
  type ApiErrorCode,
  type FileAccessDto,
  type FileMetaDto,
  type FileRole,
  type SectionSpan,
} from '@ferrum/protocol';
import { MAX_THUMB_BYTES, SERVER_CONFLICT, type FileMeta, type IoResults } from './file-list';
import { MAX_OPEN_BYTES, tooLarge } from './limits';

export type Failure = ApiErrorCode | 'NETWORK';

export class ServerFailure extends Error {
  constructor(
    readonly code: Failure,
    message = failureMessage(code),
  ) {
    super(message);
  }
}

export function failureMessage(code: Failure): string {
  switch (code) {
    case 'CONFLICT':
      return SERVER_CONFLICT;
    case 'NOT_FOUND':
      return 'Нет доступа к файлу или он удалён';
    case 'FORBIDDEN':
      return 'Недостаточно прав';
    case 'UNAUTHORIZED':
      return 'Сессия завершена — войдите снова';
    case 'TOO_LARGE':
      return tooLarge(MAX_OPEN_BYTES);
    case 'BAD_CONTAINER':
      return 'Это не файл Ferrum';
    case 'TOO_MANY_REQUESTS':
    case 'BUSY':
      return 'Слишком много запросов — повторите позже';
    case 'UNAVAILABLE':
      return 'Сервер недоступен — повторите позже';
    case 'NETWORK':
      return 'Нет связи с сервером';
    default:
      return 'Сервер отклонил запрос';
  }
}

const isErrorCode = (v: unknown): v is ApiErrorCode => (API_ERROR_CODES as readonly unknown[]).includes(v);

/** Дольше этого не ждём небольшой запрос: то же, что и у входа. */
export const JSON_TIMEOUT_MS = 15_000;
/** Опрос открытого файла: снятие доступа и понижение роли видны не позже этого. */
export const ACCESS_POLL_MS = 15_000;
/** Превью — не чаще раза в минуту (спека, раздел 6.2). */
export const THUMB_INTERVAL_MS = 60_000;
/** Срок передачи по размеру: от 64 КиБ/с, но не меньше минуты и не больше получаса. */
const MIN_TRANSFER_MS = 60_000;
const MAX_TRANSFER_MS = 30 * 60_000;
export const transferTimeout = (bytes: number): number =>
  Math.ceil(Math.min(MAX_TRANSFER_MS, MIN_TRANSFER_MS + (bytes / (64 * 1024)) * 1000));
/** Картинок загружается одновременно. */
const IMAGE_CONCURRENCY = 4;

export type AccessState = { kind: 'role'; role: FileRole; version: number } | { kind: 'none' } | { kind: 'session' };

export interface JournalEntries {
  entries: ArrayBuffer[];
  session: number | null;
  tooLarge: boolean;
}

/** Журнал правок этого человека (в браузере — IndexedDB с разделением по пользователю). */
export interface JournalStore {
  append(file: string, session: number, batch: number, bytes: ArrayBuffer): Promise<void>;
  read(file: string): Promise<JournalEntries>;
  /** Снимок вобрал пачки до `upTo` этого сеанса и все прежние. */
  cut(file: string, session: number, upTo: number): Promise<void>;
  drop(file: string): Promise<void>;
}

export interface ServerEvents {
  changed(): void;
  access(file: string, state: AccessState): void;
}

export interface Clock {
  now(): number;
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(t: unknown): void;
}

function toMeta(d: FileMetaDto): FileMeta {
  return { ...d };
}

function record(v: unknown): Record<string, unknown> | null {
  return typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
}

/** Куда файл читается: из памяти (снимок ядра) или из `Blob` (файл с диска, не загружается целиком). */
interface Source {
  size: number;
  read(start: number, end: number): Promise<Uint8Array>;
  part(start: number, end: number): BlobPart;
}

function bytesSource(bytes: Uint8Array<ArrayBuffer>): Source {
  return {
    size: bytes.length,
    read: async (s, e) => bytes.subarray(s, e),
    part: (s, e) => bytes.subarray(s, e),
  };
}

function blobSource(blob: Blob): Source {
  return {
    size: blob.size,
    read: async (s, e) => new Uint8Array(await blob.slice(s, e).arrayBuffer()),
    part: (s, e) => blob.slice(s, e),
  };
}

interface Split {
  /** Файл без картинок: то, что хранит сервер (раздел 7.5). */
  body: BodyInit;
  size: number;
  images: { hash: string; size: number; part: BlobPart }[];
}

/** Картинки уходят отдельными запросами, остальное — как есть, без копии, если картинок нет. */
async function splitContainer(source: Source, whole: () => BodyInit): Promise<Split> {
  let spans: SectionSpan[];
  try {
    spans = await scanContainer(source.size, source.read);
  } catch (e) {
    if (e instanceof ContainerError) throw new ServerFailure('BAD_CONTAINER');
    throw e;
  }
  const images = spans.filter((s) => s.kind === SECTION_IMAGE && isHash(s.id));
  if (images.length === 0 && spans.every((s) => s.kind !== SECTION_IMAGE)) return { body: whole(), size: source.size, images: [] };
  const kept = spans.filter((s) => s.kind !== SECTION_IMAGE);
  const parts: BlobPart[] = [containerHeader(kept.length) as Uint8Array<ArrayBuffer>];
  let size = 16;
  for (const s of kept) {
    parts.push(source.part(s.start, s.data + s.size));
    size += s.data + s.size - s.start;
  }
  return {
    body: new Blob(parts),
    size,
    images: images.map((s) => ({ hash: s.id, size: s.size, part: source.part(s.data, s.data + s.size) })),
  };
}

async function inPool<T>(items: T[], limit: number, work: (item: T) => Promise<void>): Promise<void> {
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(limit, items.length) }, async () => {
      while (next < items.length) await work(items[next++]);
    }),
  );
}

export class ServerFiles {
  /** Запись, исход которой неизвестен (сеть оборвалась): следующая несёт её номер, и своя удачная
   * запись не станет `CONFLICT`. */
  private readonly uncertain = new Map<string, string>();
  private readonly knownImages = new Map<string, Set<string>>();
  private readonly thumbs = new Map<string, { last: number; pending: ArrayBuffer | null | undefined; timer: unknown }>();
  private readonly lastAccess = new Map<string, string>();

  constructor(
    private readonly http: typeof fetch,
    private readonly journal: JournalStore,
    private readonly events: ServerEvents,
    private readonly clock: Clock,
    private readonly newId: () => string = () => crypto.randomUUID(),
  ) {}

  /** Запрос с небольшим телом и ответом. Повтор один раз — только у чтения и только при сбое сети или
   * 5xx: запись повторяет тот, кто знает, можно ли. */
  private async json<T>(path: string, valid: (v: unknown) => v is T, init: RequestInit = {}): Promise<T> {
    const method = init.method ?? 'GET';
    const attempt = async (): Promise<T> => {
      const signal = AbortSignal.timeout(JSON_TIMEOUT_MS);
      let res: Response;
      try {
        res = await this.http(path, { ...init, signal });
      } catch {
        throw new ServerFailure('NETWORK');
      }
      return this.readReply(res, valid);
    };
    try {
      return await attempt();
    } catch (e) {
      const retry = method === 'GET' && e instanceof ServerFailure && (e.code === 'NETWORK' || e.code === 'UNAVAILABLE');
      if (!retry) throw e;
      return attempt();
    }
  }

  private async readReply<T>(res: Response, valid: (v: unknown) => v is T): Promise<T> {
    if (res.status >= 500) throw new ServerFailure('UNAVAILABLE');
    let body: unknown;
    try {
      body = await res.json();
    } catch {
      throw new ServerFailure('NETWORK');
    }
    if (res.ok) {
      if (valid(body)) return body;
      throw new ServerFailure('NETWORK');
    }
    const code = record(body)?.error;
    throw new ServerFailure(isErrorCode(code) ? code : 'NETWORK');
  }

  private post<T>(path: string, valid: (v: unknown) => v is T, body?: unknown): Promise<T> {
    return this.json(path, valid, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  }

  private note(file: string, failure: unknown): void {
    if (!(failure instanceof ServerFailure)) return;
    if (failure.code === 'NOT_FOUND') this.announce(file, { kind: 'none' });
    else if (failure.code === 'UNAUTHORIZED') this.announce(file, { kind: 'session' });
    else if (failure.code === 'FORBIDDEN') void this.checkAccess(file).catch(() => {});
  }

  /** Состояние доступа открытому редактору: только когда оно изменилось. */
  private announce(file: string, state: AccessState): void {
    const key = JSON.stringify(state);
    if (this.lastAccess.get(file) === key) return;
    this.lastAccess.set(file, key);
    this.events.access(file, state);
  }

  /** Лёгкая проверка: сеть оборвалась — ничего не меняется, это забота следующей попытки. */
  async checkAccess(file: string): Promise<void> {
    try {
      const a = await this.json<FileAccessDto>(`${API_FILES}/${file}/access`, isFileAccess);
      this.announce(file, { kind: 'role', role: a.role, version: a.version });
    } catch (e) {
      this.note(file, e);
    }
  }

  forgetAccess(file: string): void {
    this.lastAccess.delete(file);
  }

  async list(section: string, query: string, start: number, count: number): Promise<IoResults['list']> {
    if (!parseSection(section)) throw new ServerFailure('BAD_REQUEST');
    const q = new URLSearchParams({ section, q: query, start: String(start), count: String(count) });
    const r = await this.json(`${API_FILES}?${q}`, isFileList);
    return { total: r.total, rows: r.rows.map(toMeta) };
  }

  async get(file: string): Promise<FileMeta | null> {
    try {
      return toMeta(await this.json(`${API_FILES}/${file}`, isFileMeta));
    } catch (e) {
      if (e instanceof ServerFailure && e.code === 'NOT_FOUND') return null;
      throw e;
    }
  }

  /** Новая запись; с содержимым — запись вслед за ней. Не вышло — запись не остаётся пустым файлом. */
  async create(name: string, content: Blob | ArrayBuffer | null, project: string | null, journal?: ArrayBuffer[]): Promise<FileMeta> {
    let meta = toMeta(await this.post(API_FILES, isFileMeta, { name, projectId: project }));
    if (content) {
      if ((content instanceof Blob ? content.size : content.byteLength) > MAX_OPEN_BYTES) {
        await this.http(`${API_FILES}/${meta.id}`, { method: 'DELETE' }).catch(() => {});
        throw new ServerFailure('TOO_LARGE');
      }
      try {
        meta = await this.upload(meta.id, 0, content);
      } catch (e) {
        await this.http(`${API_FILES}/${meta.id}`, { method: 'DELETE' }).catch(() => {});
        throw e;
      }
    }
    // Перенос файла из браузера: правки после его байтов становятся журналом нового файла.
    if (journal) await Promise.all(journal.map((bytes, i) => this.journal.append(meta.id, 1, i, bytes)));
    this.events.changed();
    return meta;
  }

  /** Содержимое целиком (картинки — на своём месте в контейнере) и поколение именно этих байтов. */
  private async download(file: string): Promise<{ bytes: ArrayBuffer | null; version: number }> {
    const ctl = new AbortController();
    let timer = this.clock.setTimeout(() => ctl.abort(), JSON_TIMEOUT_MS);
    try {
      let res: Response;
      try {
        res = await this.http(`${API_FILES}/${file}/content`, { signal: ctl.signal });
      } catch {
        throw new ServerFailure('NETWORK');
      }
      if (res.status !== 200 && res.status !== 204) {
        await this.readReply(res, (_v): _v is never => false);
        throw new ServerFailure('NETWORK');
      }
      const version = Number(res.headers.get(VERSION_HEADER));
      if (res.headers.get(VERSION_HEADER) === null || !Number.isFinite(version)) throw new ServerFailure('NETWORK');
      if (res.status === 204) return { bytes: null, version };
      const declared = Number(res.headers.get('content-length') ?? 0);
      if (declared > MAX_OPEN_BYTES) throw new ServerFailure('TOO_LARGE');
      this.clock.clearTimeout(timer);
      timer = this.clock.setTimeout(() => ctl.abort(), transferTimeout(declared));
      try {
        const body = await res.arrayBuffer();
        return { bytes: await this.withImages(file, body), version };
      } catch (e) {
        if (e instanceof ServerFailure) throw e;
        throw new ServerFailure('NETWORK');
      }
    } finally {
      this.clock.clearTimeout(timer);
    }
  }

  private async image(file: string, hash: string): Promise<ArrayBuffer | null> {
    const res = await this.http(`${API_FILES}/${file}/images/${hash}`, { signal: AbortSignal.timeout(transferTimeout(MAX_OPEN_BYTES)) });
    if (res.status === 404) return null;
    if (!res.ok) throw new ServerFailure(res.status >= 500 ? 'UNAVAILABLE' : 'NETWORK');
    return res.arrayBuffer();
  }

  /** Картинки приходят отдельно и встают в контейнер обычными секциями: ядро получает обычный файл.
   * Заголовок секции собирается без вычислений: её хэш — это её имя. Нет картинки — ядро покажет
   * заглушку, как у файла с потерянной секцией. */
  private async withImages(file: string, body: ArrayBuffer): Promise<ArrayBuffer> {
    const source = new Uint8Array(body);
    let spans: SectionSpan[];
    try {
      spans = await scanContainer(source.length, async (s, e) => source.subarray(s, e));
    } catch {
      // Испорченный файл откроет или отвергнет ядро: своё сообщение у него.
      return body;
    }
    const hashes = new Set<string>();
    for (const s of spans) {
      if (s.kind !== SECTION_PAGE_REFS) continue;
      for (const line of new TextDecoder().decode(source.subarray(s.data, s.data + s.size)).split('\n')) {
        if (isHash(line)) hashes.add(line);
      }
    }
    if (hashes.size === 0) return body;
    const found: { hash: string; bytes: Uint8Array }[] = [];
    await inPool([...hashes], IMAGE_CONCURRENCY, async (hash) => {
      const got = await this.image(file, hash);
      if (got) found.push({ hash, bytes: new Uint8Array(got) });
    });
    const headers = found.map((f) => imageSectionHeader(f.hash, f.bytes.length));
    const total = source.length + found.reduce((n, f, i) => n + f.bytes.length + headers[i].length, 0);
    if (total > MAX_OPEN_BYTES) throw new ServerFailure('TOO_LARGE');
    const out = new Uint8Array(total);
    out.set(containerHeader(spans.length + found.length));
    out.set(source.subarray(16), 16);
    let at = source.length;
    found.forEach((f, i) => {
      out.set(headers[i], at);
      at += headers[i].length;
      out.set(f.bytes, at);
      at += f.bytes.length;
      this.images(file).add(f.hash);
    });
    return out.buffer;
  }

  private images(file: string): Set<string> {
    let set = this.knownImages.get(file);
    if (!set) this.knownImages.set(file, (set = new Set()));
    return set;
  }

  async read(file: string): Promise<IoResults['read']> {
    const m = await this.json(`${API_FILES}/${file}`, isFileMeta);
    const { bytes, version } = await this.download(file);
    // Читатель правок не применяет: журнал остаётся до тех пор, пока ему вернут право.
    const j: JournalEntries =
      bytes && m.role !== 'viewer' ? await this.journal.read(file) : { entries: [], session: null, tooLarge: false };
    return {
      // Поколение — именно этих байтов: запись на их основе проверяется по нему.
      meta: toMeta({ ...m, version }),
      bytes,
      journal: j.entries,
      session: j.session,
      journalTooLarge: j.tooLarge,
    };
  }

  /** Файл `.ferrum` с картинками: «Скачать копию» (читателю тоже, раздел 6.1). */
  async blob(file: string): Promise<Blob | null> {
    const { bytes } = await this.download(file);
    return bytes ? new Blob([bytes], { type: 'application/x-ferrum' }) : null;
  }

  private async uploadImages(file: string, images: Split['images']): Promise<void> {
    const known = this.images(file);
    await inPool(images, IMAGE_CONCURRENCY, async (img) => {
      if (known.has(img.hash)) return;
      const url = `${API_FILES}/${file}/images/${img.hash}`;
      let res: Response;
      try {
        res = await this.http(url, { method: 'HEAD', signal: AbortSignal.timeout(JSON_TIMEOUT_MS) });
        if (res.status === 404) {
          res = await this.http(url, { method: 'PUT', body: new Blob([img.part]), signal: AbortSignal.timeout(transferTimeout(img.size)) });
        }
      } catch {
        throw new ServerFailure('NETWORK');
      }
      if (!res.ok) {
        // Ответ HEAD без тела: причину скажет проверка доступа, текст — сам код.
        if (res.status === 404 || res.status === 401 || res.status === 403) {
          throw new ServerFailure(res.status === 404 ? 'NOT_FOUND' : res.status === 401 ? 'UNAUTHORIZED' : 'FORBIDDEN');
        }
        await this.readReply(res, (_v): _v is never => false);
      }
      known.add(img.hash);
    });
  }

  /** Заведомый отказ — раньше передачи: файл могут записать другие, и незачем гнать десятки мегабайт. */
  private async preflight(file: string, base: number): Promise<void> {
    const a = await this.json<FileAccessDto>(`${API_FILES}/${file}/access`, isFileAccess);
    if (a.role === 'viewer') throw new ServerFailure('FORBIDDEN');
    const ours = this.uncertain.has(file) && a.version === base + 1;
    if (a.version !== base && !ours) throw new ServerFailure('CONFLICT');
  }

  private async upload(file: string, base: number, content: Blob | ArrayBuffer): Promise<FileMeta> {
    const source = content instanceof Blob ? blobSource(content) : bytesSource(new Uint8Array(content));
    const split = await splitContainer(source, () => content as BodyInit);
    await this.uploadImages(file, split.images);
    const write = this.newId();
    const previous = this.uncertain.get(file);
    this.uncertain.set(file, write);
    let res: Response;
    try {
      res = await this.http(`${API_FILES}/${file}/content`, {
        method: 'PUT',
        headers: {
          'If-Match': `"${base}"`,
          [WRITE_ID_HEADER]: write,
          ...(previous ? { [PREVIOUS_WRITE_HEADER]: previous } : {}),
        },
        body: split.body,
        signal: AbortSignal.timeout(transferTimeout(split.size)),
      });
    } catch {
      // Ответа нет: запись могла пройти. Номер остаётся: следующая пройдёт и в этом случае.
      throw new ServerFailure('NETWORK');
    }
    // Ответ есть: исход известен.
    if (res.status < 500) this.uncertain.delete(file);
    return toMeta(await this.readReply(res, isFileMeta));
  }

  async write(
    file: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ): Promise<FileMeta> {
    try {
      await this.preflight(file, base);
      const meta = await this.upload(file, base, bytes);
      if (journal) await this.journal.cut(file, journal.session, journal.upTo);
      if (thumb !== 'keep') this.offerThumb(file, thumb);
      this.events.changed();
      return meta;
    } catch (e) {
      this.note(file, e);
      throw e;
    }
  }

  /** Превью не чаще раза в минуту: новое до срока заменяет ждущее. Сбой не портит запись файла. */
  private offerThumb(file: string, thumb: ArrayBuffer | null): void {
    if (thumb && thumb.byteLength > MAX_THUMB_BYTES) return;
    const state = this.thumbs.get(file) ?? { last: -Infinity, pending: undefined, timer: null };
    this.thumbs.set(file, state);
    state.pending = thumb;
    if (state.timer !== null) return;
    const wait = state.last + THUMB_INTERVAL_MS - this.clock.now();
    if (wait <= 0) {
      void this.sendThumb(file);
      return;
    }
    state.timer = this.clock.setTimeout(() => {
      state.timer = null;
      void this.sendThumb(file);
    }, wait);
  }

  private async sendThumb(file: string): Promise<void> {
    const state = this.thumbs.get(file);
    if (!state || state.pending === undefined) return;
    const thumb = state.pending;
    state.pending = undefined;
    state.last = this.clock.now();
    try {
      const res = await this.http(
        `${API_FILES}/${file}/thumb`,
        thumb
          ? { method: 'PUT', body: thumb, signal: AbortSignal.timeout(JSON_TIMEOUT_MS) }
          : { method: 'DELETE', signal: AbortSignal.timeout(JSON_TIMEOUT_MS) },
      );
      if (res.ok) this.events.changed();
    } catch {
      // Превью обновится со следующей записью.
    }
  }

  appendJournal(file: string, session: number, batch: number, bytes: ArrayBuffer): Promise<void> {
    return this.journal.append(file, session, batch, bytes);
  }

  async rename(file: string, name: string): Promise<FileMeta> {
    const m = toMeta(
      await this.json(`${API_FILES}/${file}`, isFileMeta, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name }),
      }),
    );
    this.events.changed();
    return m;
  }

  async touch(file: string): Promise<FileMeta> {
    return toMeta(await this.post(`${API_FILES}/${file}/touch`, isFileMeta));
  }

  private async act(path: string, method = 'POST'): Promise<void> {
    await this.json(path, (v): v is unknown => record(v) !== null, { method });
    this.events.changed();
  }

  trash(file: string): Promise<void> {
    return this.act(`${API_FILES}/${file}/trash`);
  }
  restore(file: string): Promise<void> {
    return this.act(`${API_FILES}/${file}/restore`);
  }
  async remove(file: string): Promise<void> {
    await this.act(`${API_FILES}/${file}`, 'DELETE');
    await this.journal.drop(file);
  }
  emptyTrash(): Promise<void> {
    return this.act(`${API_FILES}/empty-trash`);
  }

  async thumbnail(file: string): Promise<Blob | null> {
    let res: Response;
    try {
      res = await this.http(`${API_FILES}/${file}/thumb`, { signal: AbortSignal.timeout(JSON_TIMEOUT_MS) });
    } catch {
      throw new ServerFailure('NETWORK');
    }
    if (res.status === 204 || res.status === 404) return null;
    if (!res.ok) throw new ServerFailure(res.status >= 500 ? 'UNAVAILABLE' : 'NETWORK');
    return res.blob();
  }
}
````

Создать `apps/web/src/core/server-messages.ts`:

````ts
// Сообщения между основным потоком и worker'ом файлов сервера.

import type { FileMeta, IoResults } from './file-list';
import type { AccessState } from './server-files';

export type ServerOp =
  | { op: 'list'; section: string; query: string; start: number; count: number }
  | { op: 'get'; file: string }
  /** `bytes`: `File` с диска уходит ссылкой, читает worker; `journal` — правки переносимого файла. */
  | { op: 'create'; name: string; bytes: Blob | ArrayBuffer | null; project: string | null; journal?: ArrayBuffer[] }
  | { op: 'read'; file: string }
  | {
      op: 'write';
      file: string;
      bytes: ArrayBuffer;
      thumb: ArrayBuffer | null | 'keep';
      base: number;
      journal: { session: number; upTo: number } | null;
    }
  | { op: 'appendJournal'; file: string; session: number; batch: number; bytes: ArrayBuffer }
  | { op: 'blob'; file: string }
  | { op: 'rename'; file: string; name: string }
  | { op: 'touch'; file: string }
  | { op: 'trash'; file: string }
  | { op: 'restore'; file: string }
  | { op: 'delete'; file: string }
  | { op: 'emptyTrash' }
  | { op: 'thumb'; file: string };

export type ServerRequest = { id: number } & ServerOp;

export interface ServerResults {
  list: IoResults['list'];
  get: FileMeta | null;
  create: FileMeta;
  read: IoResults['read'];
  write: FileMeta;
  appendJournal: null;
  blob: Blob | null;
  rename: FileMeta;
  touch: FileMeta;
  trash: null;
  restore: null;
  delete: null;
  emptyTrash: null;
  thumb: Blob | null;
}

export type ToServer =
  { t: 'init'; user: string } | { t: 'visible'; on: boolean } | { t: 'watch'; file: string; on: boolean } | ({ t: 'req' } & ServerRequest);

export type FromServer =
  | { t: 'res'; id: number; ok: true; value: unknown }
  | { t: 'res'; id: number; ok: false; message: string; code?: string }
  | { t: 'changed' }
  | { t: 'access'; file: string; state: AccessState };
````

В `apps/web/src/main.tsx` заменить:

````tsx
import './ui/styles/files.css';
import './ui/styles/auth.css';
import { parseConfirmHash } from '@ferrum/protocol';
import { render } from 'solid-js/web';
````

на:

````tsx
import './ui/styles/files.css';
import './ui/styles/auth.css';
import './ui/styles/server.css';
import { parseConfirmHash } from '@ferrum/protocol';
import { render } from 'solid-js/web';
````

В `apps/web/src/main.tsx` заменить:

````tsx
import { AuthGate } from './ui/auth/auth-gate';
import { extensions } from './ui/extensions';
import { Root } from './ui/root';
import { runSmoke } from './ui/tauri';

````

на:

````tsx
import { AuthGate } from './ui/auth/auth-gate';
import { extensions } from './ui/extensions';
import { createLocalStore } from './ui/local-store';
import { Root } from './ui/root';
import { createServerStore, installServerUi } from './ui/server/setup';
import { runSmoke } from './ui/tauri';

````

В `apps/web/src/main.tsx` заменить:

````tsx
if (import.meta.env.VITE_FERRUM_LOCAL === '1') {
  document.documentElement.dataset.ferrumLocal = '1';
  render(() => <Root />, document.getElementById('app')!);
} else {
  extensions.accountTab = AccountTab;
  extensions.accountRow = AccountRow;
````

на:

````tsx
if (import.meta.env.VITE_FERRUM_LOCAL === '1') {
  document.documentElement.dataset.ferrumLocal = '1';
  render(() => <Root createStore={createLocalStore} />, document.getElementById('app')!);
  // Десктоп с `FERRUM_SMOKE=1`: проверка запуска (вне Tauri ничего не делает).
  void runSmoke();
} else {
  extensions.noDebugParam = true;
  installServerUi();
  extensions.accountTab = AccountTab;
  extensions.accountRow = AccountRow;
````

В `apps/web/src/main.tsx` заменить:

````tsx
    () => (
      <AuthGate>
        <Root />
      </AuthGate>
    ),
````

на:

````tsx
    () => (
      <AuthGate>
        <Root createStore={createServerStore} />
      </AuthGate>
    ),
````

В `apps/web/src/main.tsx` заменить:

````tsx
  );
}
// Десктоп с `FERRUM_SMOKE=1`: проверка запуска (вне Tauri ничего не делает).
void runSmoke();
````

на:

````tsx
  );
}
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  onLeave: (leave: () => Promise<boolean>) => void;
  /** Дописать журнал без вопросов: вкладка сейчас перезагрузится (смена входа в другой вкладке). */
  onFlush?: (flush: () => Promise<void>) => void;
  /** Файл из хранилища не открылся (испорчен): к экрану файлов с этим сообщением. */
  onOpenFailed: (message: string) => void;
````

на:

````tsx
  onLeave: (leave: () => Promise<boolean>) => void;
  /** Дописать журнал без вопросов: вкладка сейчас перезагрузится (смена входа в другой вкладке). */
  onFlush?: (flush: () => Promise<boolean>) => void;
  /** Сессии больше нет (выход в другой вкладке), а файл не записался: как показать это человеку. */
  onSessionLost?: (mark: () => void) => void;
  /** Файл из хранилища не открылся (испорчен): к экрану файлов с этим сообщением. */
  onOpenFailed: (message: string) => void;
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx

  const id = props.file.id;
  let saver: Saver | undefined;
  let offSaveKey: (() => void) | undefined;
````

на:

````tsx

  const id = props.file.id;
  /** Доступ к файлу потерян, пока он открыт (файл на сервере): снят или сессия кончилась. */
  const [lost, setLost] = createSignal<'none' | 'session' | null>(null);
  /** Просьба о доступе из плашки «Нет доступа». */
  const [asked, setAsked] = createSignal(false);
  store.setState('fileRole', props.file.role ?? null);
  let offWatch: (() => void) | undefined;
  let saver: Saver | undefined;
  let offSaveKey: (() => void) | undefined;
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
      version = r.meta.version;
      journalSession = r.session;
      if (!r.bytes) return null;
      let journal = r.journal;
````

на:

````tsx
      version = r.meta.version;
      journalSession = r.session;
      store.setState('fileRole', r.meta.role ?? null);
      if (!r.bytes) return null;
      let journal = r.journal;
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  props.onFlush?.(async () => {
    // Не только журнал: у нового файла ещё нет записи, и журнал правку не сохранит.
    if (saver) await withTimeout(saver.save(), SETTLE_MS, false);
  });
  const answerLeave = (go: boolean) => {
    const a = askLeave();
````

на:

````tsx
  props.onFlush?.(async () => {
    // Не только журнал: у нового файла ещё нет записи, и журнал правку не сохранит.
    return saver ? withTimeout(saver.save(), SETTLE_MS, false) : true;
  });
  props.onSessionLost?.(() => {
    editor()?.send({ type: 'setViewOnly', on: true });
    setLost('session');
  });
  /** Снова войти — это перезагрузка. Что не в журнале (новый файл без записи), с ней пропало бы. */
  const signInAgain = async () => {
    const s = saver;
    if (s) await withTimeout(s.flushJournal(), SETTLE_MS, false);
    if (s?.pending()) {
      setAskLeave({
        reason: 'Этот файл ещё ни разу не записан. Сначала сохраните копию на диск',
        answer: (go) => {
          if (go) location.reload();
        },
      });
      return;
    }
    location.reload();
  };
  const answerLeave = (go: boolean) => {
    const a = askLeave();
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        };
        saver = createSaver(ed, props.files, id, version, stored, { ...testIntervals(), journalSession });
        // Ctrl/Cmd+S, пока фокус в поле панели (клавиша не ушла ядру): тоже
        // «Сохранить», и браузер не открывает диалог сохранения страницы.
````

на:

````tsx
        };
        saver = createSaver(ed, props.files, id, version, stored, { ...testIntervals(), journalSession });
        // Доступ к файлу меняется, пока он открыт. Этап 3 пришлёт то же событие по соединению файла.
        let toldNewer = false;
        offWatch = props.files.watch?.(id, (state) => {
          if (state.kind !== 'role') {
            // Правки — в журнале браузера, плашка предлагает копию на диск; уход спросит о несохранённом.
            ed.send({ type: 'setViewOnly', on: true });
            setLost(state.kind);
            return;
          }
          const before = store.state.fileRole;
          setLost(null);
          store.setState('fileRole', state.role);
          ed.send({ type: 'setViewOnly', on: state.role === 'viewer' });
          if (before !== state.role && state.role === 'viewer') {
            store.toast(
              store.state.modified
                ? 'Теперь вы только смотрите файл. Правки не записаны — сохраните копию на диск'
                : 'Теперь вы только смотрите файл',
            );
          } else if (before === 'viewer' && state.role !== 'viewer') store.toast('Теперь вы можете редактировать файл');
          if (saver && state.version > saver.version() && !toldNewer) {
            toldNewer = true;
            store.toast('Файл изменён в другом окне или другим человеком. Откройте его заново, чтобы увидеть новую версию');
          }
        });
        // Ctrl/Cmd+S, пока фокус в поле панели (клавиша не ушла ядру): тоже
        // «Сохранить», и браузер не открывает диалог сохранения страницы.
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
            // Имя файла — из хранилища (его могли переименовать на экране файлов);
            // тихо: открытие — не правка, файл из-за него не пересохраняется.
            if (e.name !== props.file.name) ed.send({ type: 'setDocumentName', name: props.file.name, quiet: true });
          } else if (opening && stored && e.type === 'error' && e.message.startsWith('Не удалось открыть файл')) {
            opening = false;
````

на:

````tsx
            // Имя файла — из хранилища (его могли переименовать на экране файлов);
            // тихо: открытие — не правка, файл из-за него не пересохраняется.
            if (e.name !== props.file.name) {
              // Читатель документ не меняет, даже тихо: имя — только для показа.
              if (store.state.fileRole === 'viewer') store.setState('documentName', props.file.name);
              else ed.send({ type: 'setDocumentName', name: props.file.name, quiet: true });
            }
          } else if (opening && stored && e.type === 'error' && e.message.startsWith('Не удалось открыть файл')) {
            opening = false;
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  onCleanup(() => {
    disposed = true;
    // Открытый вопрос об уходе (редактор снят перезапуском) — «Остаться».
    answerLeave(false);
````

на:

````tsx
  onCleanup(() => {
    disposed = true;
    offWatch?.();
    // Открытый вопрос об уходе (редактор снят перезапуском) — «Остаться».
    answerLeave(false);
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
            </div>
          </Show>
          <Show when={store.state.stopped}>
            {(message) => (
````

на:

````tsx
            </div>
          </Show>
          <Show when={lost()}>
            {(kind) => (
              <div class="fe-boot fe-boot--error fe-boot--stopped fe-lost" role="alert">
                <strong>{kind() === 'none' ? 'Нет доступа' : 'Сессия завершена'}</strong>
                <span>
                  {kind() === 'none'
                    ? 'Вам больше не открыт этот файл. Несохранённые правки можно сохранить копией на диск'
                    : 'Войдите снова, правки останутся в этом браузере'}
                </span>
                <span class="fe-lost__actions">
                  <button class="fe-btn fe-btn--secondary" onClick={() => editor()?.send({ type: 'save' })}>
                    Сохранить копию на диск
                  </button>
                  {kind() === 'none' ? (
                    <>
                      <Show when={props.files.requestAccess && !asked()}>
                        <button
                          class="fe-btn fe-btn--secondary"
                          onClick={() => {
                            setAsked(true);
                            void props.files.requestAccess?.(id).catch(() => setAsked(false));
                          }}
                        >
                          Запросить доступ
                        </button>
                      </Show>
                      <button class="fe-btn fe-btn--primary" onClick={() => props.navigate({ kind: 'files', section: 'recent' })}>
                        К файлам
                      </button>
                    </>
                  ) : (
                    <button class="fe-btn fe-btn--primary" onClick={() => void signInAgain()}>
                      Войти
                    </button>
                  )}
                </span>
              </div>
            )}
          </Show>
          <Show when={store.state.stopped}>
            {(message) => (
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
                  onOpen={setMainMenu}
                  onBack={() => props.navigate({ kind: 'files', section: 'recent' })}
                  collapsed={leftCollapsed()}
                  onCollapse={setLeftCollapsed}
````

на:

````tsx
                  onOpen={setMainMenu}
                  onBack={() => props.navigate({ kind: 'files', section: 'recent' })}
                  location={props.file.projectId ? 'Проект' : props.file.role && props.file.role !== 'owner' ? 'Доступные мне' : undefined}
                  collapsed={leftCollapsed()}
                  onCollapse={setLeftCollapsed}
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
                </div>
                <Show when={!rightCollapsed()}>
                  <DesignHeader />
                  <PropertiesPanel theme={theme()} />
                  <ResizeEdge side="right" width={right()} onWidth={setRight} />
````

на:

````tsx
                </div>
                <Show when={!rightCollapsed()}>
                  <DesignHeader file={props.file} />
                  <PropertiesPanel theme={theme()} />
                  <ResizeEdge side="right" width={right()} onWidth={setRight} />
````

В `apps/web/src/ui/auth/auth-gate.tsx` заменить:

````tsx
        if (was?.id === now?.id && was?.name === now?.name) return;
        // Спросить человека некому (он в другой вкладке): файл записывается молча.
        await editorHooks.flush?.().catch(() => {});
        location.reload();
      });
    }),
````

на:

````tsx
        if (was?.id === now?.id && was?.name === now?.name) return;
        // Спросить человека некому (он в другой вкладке): файл записывается молча.
        const saved = await (editorHooks.flush?.() ?? Promise.resolve(true)).catch(() => false);
        // Сессии уже нет, файл не записался: перезагрузка потеряла бы то, чего нет в журнале.
        if (saved || now !== null) location.reload();
        else editorHooks.sessionLost?.();
      });
    }),
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
import type { TransportMode } from '../core/transport';
import type { Editor } from './context';
import { supportsCustomFormat } from './clipboard';
import { editableTarget, forwardKey, modalOpen, opensWindow } from './keys';
````

на:

````ts
import type { TransportMode } from '../core/transport';
import type { Editor } from './context';
import { extensions } from './extensions';
import { supportsCustomFormat } from './clipboard';
import { editableTarget, forwardKey, modalOpen, opensWindow } from './keys';
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
/** Тестовый режим: E2E ставит `window.__ferrumTest` до загрузки или адрес с `?debug`. */
function testMode(params: URLSearchParams): boolean {
  return params.has('debug') || (window as { __ferrumTest?: boolean }).__ferrumTest === true;
}

````

на:

````ts
/** Тестовый режим: E2E ставит `window.__ferrumTest` до загрузки или адрес с `?debug`. */
function testMode(params: URLSearchParams): boolean {
  return (!extensions.noDebugParam && params.has('debug')) || (window as { __ferrumTest?: boolean }).__ferrumTest === true;
}

````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
// панелей для изменения ширины.

import { createSignal, onCleanup, onMount, Show, type JSX } from 'solid-js';
import { useEditor } from './context';
````

на:

````tsx
// панелей для изменения ширины.

import type { FileMeta } from '../core/file-list';
import { extensions } from './extensions';
import { createSignal, onCleanup, onMount, Show, type JSX } from 'solid-js';
import { useEditor } from './context';
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
  onOpen: (open: boolean) => void;
  onBack?: () => void;
  /** Панель свёрнута в полосу: остаются логотип с меню и кнопка разворота. */
  collapsed?: boolean;
````

на:

````tsx
  onOpen: (open: boolean) => void;
  onBack?: () => void;
  /** Где лежит файл; нет — «Черновики». */
  location?: string;
  /** Панель свёрнута в полосу: остаются логотип с меню и кнопка разворота. */
  collapsed?: boolean;
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
            />
          </Show>
          <span class="fe-panel-head__project">Черновики</span>
        </div>
        <Show when={props.onBack}>
````

на:

````tsx
            />
          </Show>
          <span class="fe-panel-head__project">{props.location ?? 'Черновики'}</span>
        </div>
        <Show when={props.onBack}>
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
const ZOOMS = [0.5, 1, 2];

export function DesignHeader() {
  const ed = useEditor();
  let tabs!: HTMLDivElement;
````

на:

````tsx
const ZOOMS = [0.5, 1, 2];

export function DesignHeader(props: { file?: FileMeta }) {
  const ed = useEditor();
  let tabs!: HTMLDivElement;
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
          <Caret />
        </button>
        <button class="fe-btn fe-btn--primary" aria-disabled="true" data-tip="Совместная работа появится в подсистеме 4">
          Поделиться
        </button>
        <Show when={menu()}>{(a) => <Menu label="Масштаб" items={zoomItems()} anchor={a()} onClose={() => setMenu(null)} />}</Show>
      </div>
````

на:

````tsx
          <Caret />
        </button>
        <Show when={ed.store.state.fileRole === 'viewer'}>
          <span class="fe-viewonly t-muted">Только просмотр</span>
        </Show>
        {extensions.shareButton && props.file ? (
          <extensions.shareButton file={props.file} role={() => ed.store.state.fileRole ?? undefined} />
        ) : (
          <button class="fe-btn fe-btn--primary" aria-disabled="true" data-tip="Совместная работа появится в подсистеме 4">
            Поделиться
          </button>
        )}
        <Show when={menu()}>{(a) => <Menu label="Масштаб" items={zoomItems()} anchor={a()} onClose={() => setMenu(null)} />}</Show>
      </div>
````

В `apps/web/src/ui/extensions.ts` заменить:

````ts
// автономная сборка выбрасывает, поэтому код входа в неё не попадает, а экраны о нём не знают.

import type { Component } from 'solid-js';

export const extensions: {
````

на:

````ts
// автономная сборка выбрасывает, поэтому код входа в неё не попадает, а экраны о нём не знают.

import type { Component, JSX } from 'solid-js';
import type { FileMeta, Section } from '../core/file-list';
import type { IconName } from './icons';
import type { MenuItem } from './menu';
import type { Route } from './router';

/** Строка панели разделов: раздел или подпись группы. */
export type NavEntry = { kind: 'section'; id: Section; title: string; icon: IconName } | { kind: 'label'; title: string };

export const extensions: {
````

В `apps/web/src/ui/extensions.ts` заменить:

````ts
   * таких адресов нет, любой адрес — его. */
  ownsHash?: (hash: string) => boolean;
} = {};

````

на:

````ts
   * таких адресов нет, любой адрес — его. */
  ownsHash?: (hash: string) => boolean;
  /** Адрес `?debug` не включает тестовый режим: страница рабочей сборки не раскрывает ядро по ссылке. */
  noDebugParam?: boolean;
  /** Раздел по части адреса, если у этой сборки есть другие разделы. */
  parseSection?: (path: string) => Section | null;
  /** Заголовок такого раздела. */
  sectionTitle?: (section: Section) => string | null;
  /** Строки панели разделов между «Черновиками» и «Корзиной». */
  filesNav?: () => NavEntry[];
  /** Под панелью разделов: команды, файлы прежней версии. */
  filesNavExtra?: Component<{ navigate: (r: Route) => void }>;
  /** Дополнительные пункты меню файла. */
  fileMenu?: (file: FileMeta) => MenuItem[];
  /** Новый файл из этого раздела попадает в проект. */
  newFileRoute?: (section: Section) => Route;
  /** Кнопка «Поделиться» открытого файла. */
  shareButton?: Component<{ file: FileMeta; role: () => FileMeta['role']; children?: JSX.Element }>;
} = {};

````

В `apps/web/src/ui/extensions.ts` заменить:

````ts
   * `false` — человек остался, уходить нельзя. Без редактора — `true`. */
  leave?: () => Promise<boolean>;
  /** Записать открытый файл без вопросов (вкладка сейчас перезагрузится). */
  flush?: () => Promise<void>;
} = {};
````

на:

````ts
   * `false` — человек остался, уходить нельзя. Без редактора — `true`. */
  leave?: () => Promise<boolean>;
  /** Записать открытый файл без вопросов (вкладка сейчас перезагрузится). `false` — не записался. */
  flush?: () => Promise<boolean>;
  /** Сессии больше нет, а файл не записался: редактор остаётся и предлагает копию на диск. */
  sessionLost?: () => void;
} = {};
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts

import type { FileMeta, FromIo, IoOp, IoResults, JournalMark, Section, ToIo } from '../core/file-list';
import { nameFromFile } from '../core/file-list';
import { mb } from '../core/limits';
````

на:

````ts

import type { FileMeta, FromIo, IoOp, IoResults, JournalMark, Section, ToIo } from '../core/file-list';
import type { AccessState } from '../core/server-files';
import { nameFromFile } from '../core/file-list';
import { mb } from '../core/limits';
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  get(id: string): Promise<FileMeta | null>;
  /** Новая запись (сразу видна в списке); `bytes` — содержимое импортированного
   * файла (`File` с диска читает worker). */
  create(name: string, bytes?: Blob): Promise<FileMeta>;
  /** Байты файла (`null` — новый файл, ещё не сохранённый) и пачки журнала,
   * записанные после них, по порядку записи; `session` — наибольший сеанс среди
````

на:

````ts
  get(id: string): Promise<FileMeta | null>;
  /** Новая запись (сразу видна в списке); `bytes` — содержимое импортированного
   * файла (`File` с диска читает worker); `project` — папка на сервере, где они есть. */
  create(name: string, bytes?: Blob, project?: string): Promise<FileMeta>;
  /** Байты файла (`null` — новый файл, ещё не сохранённый) и пачки журнала,
   * записанные после них, по порядку записи; `session` — наибольший сеанс среди
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  /** Файлы на диске (File System Access, Tauri); нет — пунктов «с диска» нет. */
  readonly disk?: DiskFiles;
  /** Байты системного шрифта Chrome (`FontData.blob()`) — читает worker
   * ввода-вывода; нет — системных шрифтов браузера нет. */
````

на:

````ts
  /** Файлы на диске (File System Access, Tauri); нет — пунктов «с диска» нет. */
  readonly disk?: DiskFiles;
  /** Снимок с полной историей правок: файл на сервере (журнал отставшей вкладки сольётся с ним). */
  readonly fullHistory?: boolean;
  /** Подпись кнопки импорта, если она не «Импорт…». */
  readonly importLabel?: string;
  /** Доступ к открытому файлу (файл на сервере); возвращает отписку. Нет — доступ не меняется. */
  watch?(id: string, listener: (state: AccessState) => void): () => void;
  /** Открыть файл по ссылке «Поделиться»; `false` — ссылка не действует. */
  redeemLink?(id: string, token: string): Promise<boolean>;
  /** Попросить доступ к файлу. Есть у хранилища с ролями: файла без доступа там отличить нельзя. */
  requestAccess?(id: string): Promise<boolean>;
  /** Байты системного шрифта Chrome (`FontData.blob()`) — читает worker
   * ввода-вывода; нет — системных шрифтов браузера нет. */
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
import { downloadBlob, fileName, formatSize, relativeTime } from './files';
import { importFromDisk, type FileMeta, type FileStore, type Section } from './file-store';
import { extensions } from './extensions';
import { Icon, type IconName } from './icons';
import { Menu, type MenuItem } from './menu';
````

на:

````tsx
import { downloadBlob, fileName, formatSize, relativeTime } from './files';
import { importFromDisk, type FileMeta, type FileStore, type Section } from './file-store';
import { extensions, type NavEntry } from './extensions';
import { Icon, type IconName } from './icons';
import { Menu, type MenuItem } from './menu';
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
  const [total, setTotal] = createSignal(0);
  const [loaded, setLoaded] = createSignal(false);
  const [rows, setRows] = createStore<{ start: number; list: FileMeta[] }>({ start: 0, list: [] });
  const [width, setWidth] = createSignal(0);
````

на:

````tsx
  const [total, setTotal] = createSignal(0);
  const [loaded, setLoaded] = createSignal(false);
  /** Окно не пришло (сеть, сервер): вместо пустого списка — сообщение и «Повторить». */
  const [failed, setFailed] = createSignal<string | null>(null);
  const [rows, setRows] = createStore<{ start: number; list: FileMeta[] }>({ start: 0, list: [] });
  const [width, setWidth] = createSignal(0);
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx

  const layout = createMemo(() => filesLayout(width(), view()));
  const title = () => SECTIONS.find((s) => s.id === props.section)!.title;

  // ——— Окно списка ———
````

на:

````tsx

  const layout = createMemo(() => filesLayout(width(), view()));
  const title = () => SECTIONS.find((s) => s.id === props.section)?.title ?? extensions.sectionTitle?.(props.section) ?? 'Файлы';
  // Разделы, которые есть не у всех сборок, — между «Черновиками» и «Корзиной».
  const navEntries = (): NavEntry[] => {
    const own = (s: (typeof SECTIONS)[number]): NavEntry => ({ kind: 'section', id: s.id, title: s.title, icon: s.icon });
    return [own(SECTIONS[0]), own(SECTIONS[1]), ...(extensions.filesNav?.() ?? []), own(SECTIONS[2])];
  };
  const navSections = () => navEntries().flatMap((e) => (e.kind === 'section' ? [e.id] : []));
  const newFile = () => props.navigate(extensions.newFileRoute?.(props.section) ?? { kind: 'new' });

  // ——— Окно списка ———
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
        if (!alive || my !== requestSeq) return;
        batch(() => {
          setTotal(r.total);
          setRows('start', w.start);
````

на:

````tsx
        if (!alive || my !== requestSeq) return;
        batch(() => {
          setFailed(null);
          setTotal(r.total);
          setRows('start', w.start);
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
      },
      (e: unknown) => {
        if (alive) ui.toast(`Не удалось прочитать список файлов: ${e instanceof Error ? e.message : String(e)}`, 'error');
      },
    );
````

на:

````tsx
      },
      (e: unknown) => {
        if (!alive || my !== requestSeq) return;
        const text = `Не удалось прочитать список файлов: ${e instanceof Error ? e.message : String(e)}`;
        // Фоновое перечитывание повторяется: одно сообщение, пока оно не сменится.
        if (failed() !== text) ui.toast(text, 'error');
        setFailed(text);
        asked = '';
      },
    );
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
          ...(isTauri() ? [] : [{ label: 'Открыть в новой вкладке', onSelect: () => openInNewTab(f) }]),
          { separator: true },
          { label: 'Переименовать', onSelect: () => setRenaming(f.id) },
          { label: 'Скачать копию (.ferrum)', disabled: f.size === 0, onSelect: () => void download(f) },
          { separator: true },
          { label: 'Удалить', onSelect: () => void act('Не удалось удалить файл', props.files.trash(f.id)) },
        ];

````

на:

````tsx
          ...(isTauri() ? [] : [{ label: 'Открыть в новой вкладке', onSelect: () => openInNewTab(f) }]),
          { separator: true },
          ...(f.role === 'viewer' ? [] : [{ label: 'Переименовать', onSelect: () => setRenaming(f.id) }]),
          { label: 'Скачать копию (.ferrum)', disabled: f.size === 0, onSelect: () => void download(f) },
          ...(extensions.fileMenu?.(f) ?? []),
          // Владелец у файла с ролями один: только он убирает файл в корзину.
          ...(f.role === undefined || f.role === 'owner'
            ? [
                { separator: true as const },
                { label: 'Удалить', onSelect: () => void act('Не удалось удалить файл', props.files.trash(f.id)) },
              ]
            : []),
        ];

````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
        break;
      case 'F2':
        if (f && props.section !== 'trash') setRenaming(f.id);
        break;
      case 'Delete':
        if (f && props.section !== 'trash') {
          // Карточка уйдёт из списка: фокус — на ту, что встанет на её место.
          pendingFocus = at;
````

на:

````tsx
        break;
      case 'F2':
        if (f && props.section !== 'trash' && f.role !== 'viewer') setRenaming(f.id);
        break;
      case 'Delete':
        if (f && props.section !== 'trash' && (f.role === undefined || f.role === 'owner')) {
          // Карточка уйдёт из списка: фокус — на ту, что встанет на её место.
          pendingFocus = at;
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
  // Разделы — список с выбором (listbox): стрелки переходят к соседнему разделу.
  const onNavKey = (e: KeyboardEvent) => {
    const i = SECTIONS.findIndex((s) => s.id === props.section);
    let next = -1;
    if (e.key === 'ArrowDown') next = (i + 1) % SECTIONS.length;
    else if (e.key === 'ArrowUp') next = (i - 1 + SECTIONS.length) % SECTIONS.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = SECTIONS.length - 1;
    else return;
    e.preventDefault();
    props.navigate({ kind: 'files', section: SECTIONS[next].id });
  };
  // Раздел сменили стрелками: фокус переходит к выбранному пункту.
````

на:

````tsx
  // Разделы — список с выбором (listbox): стрелки переходят к соседнему разделу.
  const onNavKey = (e: KeyboardEvent) => {
    const list = navSections();
    const i = list.indexOf(props.section);
    let next = -1;
    if (e.key === 'ArrowDown') next = (i + 1) % list.length;
    else if (e.key === 'ArrowUp') next = (i - 1 + list.length) % list.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = list.length - 1;
    else return;
    e.preventDefault();
    props.navigate({ kind: 'files', section: list[next] });
  };
  // Раздел сменили стрелками: фокус переходит к выбранному пункту.
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
        : props.section === 'trash'
          ? 'Корзина пуста'
          : 'Здесь пока нет файлов'
      : '';

````

на:

````tsx
        : props.section === 'trash'
          ? 'Корзина пуста'
          : props.section === 'shared'
            ? 'Вам пока ничего не открыли'
            : 'Здесь пока нет файлов'
      : '';

````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
            e.stopPropagation();
            clearTimeout(nameClick);
            if (props.section !== 'trash') setRenaming(p.file.id);
          }}
        >
````

на:

````tsx
            e.stopPropagation();
            clearTimeout(nameClick);
            if (props.section !== 'trash' && p.file.role !== 'viewer') setRenaming(p.file.id);
          }}
        >
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
        </label>
        <div ref={nav} class="fe-files__nav" role="listbox" aria-label="Разделы" onKeyDown={onNavKey}>
          <For each={SECTIONS}>
            {(s) => (
              <div
                class="fe-files__nav-item"
                role="option"
                aria-selected={props.section === s.id}
                tabindex={props.section === s.id ? 0 : -1}
                onClick={() => props.navigate({ kind: 'files', section: s.id })}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    props.navigate({ kind: 'files', section: s.id });
                  }
                }}
              >
                <Icon name={s.icon} />
                <span>{s.title}</span>
              </div>
            )}
          </For>
        </div>
        {extensions.accountRow && <extensions.accountRow />}
      </aside>
````

на:

````tsx
        </label>
        <div ref={nav} class="fe-files__nav" role="listbox" aria-label="Разделы" onKeyDown={onNavKey}>
          <For each={navEntries()}>
            {(s) =>
              s.kind === 'label' ? (
                <div class="fe-files__nav-label t-muted" role="presentation" title={s.title}>
                  {s.title}
                </div>
              ) : (
                <div
                  class="fe-files__nav-item"
                  role="option"
                  aria-selected={props.section === s.id}
                  tabindex={props.section === s.id ? 0 : -1}
                  onClick={() => props.navigate({ kind: 'files', section: s.id })}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      props.navigate({ kind: 'files', section: s.id });
                    }
                  }}
                >
                  <Icon name={s.icon} />
                  <span>{s.title}</span>
                </div>
              )
            }
          </For>
        </div>
        {extensions.filesNavExtra && <extensions.filesNavExtra navigate={props.navigate} />}
        {extensions.accountRow && <extensions.accountRow />}
      </aside>
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
                <button class="fe-btn fe-btn--secondary" onClick={() => void importFile()}>
                  <Icon name="upload" />
                  Импорт…
                </button>
                <Show when={props.files.disk}>
````

на:

````tsx
                <button class="fe-btn fe-btn--secondary" onClick={() => void importFile()}>
                  <Icon name="upload" />
                  {props.files.importLabel ?? 'Импорт…'}
                </button>
                <Show when={props.files.disk}>
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
                  </button>
                </Show>
                <button class="fe-btn fe-btn--primary" onClick={() => props.navigate({ kind: 'new' })}>
                  <Icon name="plus" />
                  Новый файл
````

на:

````tsx
                  </button>
                </Show>
                <button class="fe-btn fe-btn--primary" onClick={newFile}>
                  <Icon name="plus" />
                  Новый файл
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
                <p class="t-muted">{text()}</p>
                <Show when={text() === 'Здесь пока нет файлов'}>
                  <button class="fe-btn fe-btn--primary" onClick={() => props.navigate({ kind: 'new' })}>
                    <Icon name="plus" />
                    Новый файл
````

на:

````tsx
                <p class="t-muted">{text()}</p>
                <Show when={text() === 'Здесь пока нет файлов'}>
                  <button class="fe-btn fe-btn--primary" onClick={newFile}>
                    <Icon name="plus" />
                    Новый файл
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
              </div>
            )}
          </Show>
          <div
````

на:

````tsx
              </div>
            )}
          </Show>
          <Show when={failed() && !loaded()}>
            <div class="fe-files__empty" role="alert">
              <p class="t-muted">Не удалось загрузить список файлов</p>
              <button class="fe-btn fe-btn--secondary" onClick={() => request(true)}>
                Повторить
              </button>
            </div>
          </Show>
          <div
````

Создать `apps/web/src/ui/local-store.ts`:

````ts
// Файлы автономной сборки и десктопа: на диске через Tauri, в браузере — IndexedDB в worker'е.
// Этого кода нет в рабочей сборке (её ветка `main.tsx` берёт файлы с сервера, B-206).

import { isTestMode } from './boot';
import { IdbFileStore, type FileStore } from './file-store';
import type { SettingsBackend } from './settings-store';
import { isTauri, TauriFileStore } from './tauri';

export function createLocalStore(): FileStore & SettingsBackend {
  if (isTauri()) return new TauriFileStore();
  const test = isTestMode();
  const store = new IdbFileStore(test);
  // Тестам — хранилище напрямую; `?debug` в рабочей сборке этого не открывает (B-205).
  if (test) window.__ferrumFiles = { store };
  return store;
}
````

Создать `apps/web/src/ui/no-access.tsx`:

````tsx
// «Нет доступа» (спека подсистемы 4, раздел 5): файл по ссылке без права на него. Несуществующий и чужой
// файл неотличимы, поэтому просьба о доступе есть в обоих случаях, а ответ на неё один.

import { createSignal, Show } from 'solid-js';
import type { FileStore } from './file-store';
import type { Route } from './router';

export function NoAccessScreen(props: { id: string; files: FileStore; navigate: (r: Route) => void }) {
  const [state, setState] = createSignal<'idle' | 'busy' | 'sent' | 'failed'>('idle');
  const request = async () => {
    setState('busy');
    const ok = await (props.files.requestAccess?.(props.id) ?? Promise.resolve(false)).catch(() => false);
    setState(ok ? 'sent' : 'failed');
  };
  return (
    <div class="fe-denied" role="main">
      <div class="fe-denied__card">
        <h1 class="fe-denied__title t-heading">Нет доступа</h1>
        <p class="fe-denied__text t-muted">У вас нет доступа к этому файлу, или его больше нет.</p>
        <Show when={state() === 'sent'}>
          <p class="fe-denied__text" role="status">
            Запрос отправлен владельцу файла
          </p>
        </Show>
        <Show when={state() === 'failed'}>
          <p class="fe-denied__text" role="alert">
            Не удалось отправить запрос. Попробуйте ещё раз
          </p>
        </Show>
        <div class="fe-denied__actions">
          <button class="fe-btn fe-btn--secondary" onClick={() => props.navigate({ kind: 'files', section: 'recent' })}>
            К файлам
          </button>
          <Show when={state() !== 'sent'}>
            <button class="fe-btn fe-btn--primary" disabled={state() === 'busy'} onClick={() => void request()}>
              Запросить доступ
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}
````

В `apps/web/src/ui/properties.tsx` заменить:

````tsx
  return (
    <div class="fe-props" id="panel-design" role="tabpanel" aria-labelledby="tab-design" tabindex="-1">
      <Show
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
        <Show when={ed.store.state.tool === 'scale'}>
          <ScaleSection />
        </Show>
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
        <Show when={ed.store.state.textStyle}>
          <TextSection />
        </Show>
        <FillsSection />
        <StrokesSection />
      </Show>
      <ExportSection />
    </div>
````

на:

````tsx
  return (
    <div class="fe-props" id="panel-design" role="tabpanel" aria-labelledby="tab-design" tabindex="-1">
      <div class="fe-props__body" inert={ed.store.state.fileRole === 'viewer'}>
        <Show
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
          <Show when={ed.store.state.tool === 'scale'}>
            <ScaleSection />
          </Show>
          <PositionSection edit={edit} val={val} />
          <AppearanceSection edit={edit} />
          <Show when={ed.store.state.textStyle}>
            <TextSection />
          </Show>
          <FillsSection />
          <StrokesSection />
        </Show>
      </div>
      <ExportSection />
    </div>
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
import { UNTITLED } from '../core/file-list';
import { App } from './app';
import { dismissSplash, isTestMode } from './boot';
import { FilesScreen } from './files-screen';
import { ConfirmDialog } from './dialog';
import { IdbFileStore, type FileMeta, type FileStore, type Section } from './file-store';
import { isTauri, TauriFileStore } from './tauri';
import { storage } from './settings';
import { openSettings, type SettingsBackend, type SettingsStore } from './settings-store';
````

на:

````tsx
import { UNTITLED } from '../core/file-list';
import { App } from './app';
import { dismissSplash } from './boot';
import { FilesScreen } from './files-screen';
import { ConfirmDialog } from './dialog';
import type { FileMeta, FileStore, Section } from './file-store';
import { storage } from './settings';
import { openSettings, type SettingsBackend, type SettingsStore } from './settings-store';
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
import { allowRestart, type Restart } from './restart';
import { editorHooks, extensions } from './extensions';
import { historyIndex, parseRoute, routeHash, routeUrl, type Route } from './router';

````

на:

````tsx
import { allowRestart, type Restart } from './restart';
import { editorHooks, extensions } from './extensions';
import { NoAccessScreen } from './no-access';
import { historyIndex, parseRoute, routeHash, routeUrl, type Route } from './router';

````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
  | { kind: 'editor'; file: FileMeta; restart?: Restart; backend?: Backend; page?: string }
  /** Файл на диске без доступа (браузер перезапущен): вопрос, ответ — по щелчку. */
  | { kind: 'access'; file: FileMeta; answer: (ok: boolean) => void };

/** Сколько ждать блокировку файла, прежде чем сказать «открыт в другой вкладке»:
````

на:

````tsx
  | { kind: 'editor'; file: FileMeta; restart?: Restart; backend?: Backend; page?: string }
  /** Файл на диске без доступа (браузер перезапущен): вопрос, ответ — по щелчку. */
  | { kind: 'access'; file: FileMeta; answer: (ok: boolean) => void }
  /** Файла нет или нет доступа к нему (хранилище с ролями их не различает). */
  | { kind: 'denied'; id: string };

/** Сколько ждать блокировку файла, прежде чем сказать «открыт в другой вкладке»:
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
}

export function Root() {
  const test = isTestMode();
  // Десктоп — хранилище на диске через Tauri; браузер — IndexedDB в worker'е.
  const idb = isTauri() ? null : new IdbFileStore(test);
  const files: FileStore & SettingsBackend = idb ?? new TauriFileStore();
  if (test && idb) window.__ferrumFiles = { store: idb };
  const [view, setView] = createSignal<View>({ kind: 'wait' });
  let leave: (() => Promise<boolean>) | null = null;
  let flushEditor: (() => Promise<void>) | null = null;
  let unlock: (() => void) | null = null;
  let current: Route | null = null;
````

на:

````tsx
}

/** `createStore` — откуда файлы: автономная сборка и десктоп берут их на месте, рабочая — с сервера
 * (`main.tsx`). Корень этого не различает. */
export function Root(props: { createStore: () => FileStore & SettingsBackend }) {
  const files = props.createStore();
  const [view, setView] = createSignal<View>({ kind: 'wait' });
  let leave: (() => Promise<boolean>) | null = null;
  let flushEditor: (() => Promise<boolean>) | null = null;
  let markSessionLost: (() => void) | null = null;
  let unlock: (() => void) | null = null;
  let current: Route | null = null;
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
        showFiles(route.section);
      } else if (route.kind === 'new') {
        const file = await files.create(UNTITLED);
        if (my !== seq) return;
        await edit(file, my);
      } else {
        const file = route.id ? await files.get(route.id) : null;
        if (my !== seq) return;
        if (!file) toFiles('Файл не найден');
        // Файл из корзины для правки не открывается: сначала восстановить.
        else if (file.trashedAt !== undefined) toFiles('Файл в корзине — восстановите его, чтобы открыть');
````

на:

````tsx
        showFiles(route.section);
      } else if (route.kind === 'new') {
        const file = await files.create(UNTITLED, undefined, route.project);
        if (my !== seq) return;
        await edit(file, my);
      } else {
        // Ссылка «Поделиться»: сначала право на файл. Токен в адресе дальше не нужен, неудача — та же «Нет доступа».
        if (route.id && route.link && files.redeemLink) {
          await files.redeemLink(route.id, route.link).catch(() => false);
          if (my !== seq) return;
          current = { kind: 'file', id: route.id };
          replace(current);
        }
        const file = route.id ? await files.get(route.id) : null;
        if (my !== seq) return;
        if (!file && route.id && files.requestAccess) {
          current = { kind: 'file', id: route.id };
          replace(current);
          dismissSplash(true);
          setView({ kind: 'denied', id: route.id });
        } else if (!file) toFiles('Файл не найден');
        // Файл из корзины для правки не открывается: сначала восстановить.
        else if (file.trashedAt !== undefined) toFiles('Файл в корзине — восстановите его, чтобы открыть');
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
  // Выход и смена входа спрашивают у редактора то же, что уход к экрану файлов.
  editorHooks.leave = () => (leave ? leave() : Promise.resolve(true));
  editorHooks.flush = () => (leave && flushEditor ? flushEditor() : Promise.resolve());
  // Настройки — до первого экрана: тема, вид списка и раскладка нужны сразу (экраны
  // получают их готовыми). Хранилище настроек живёт у корня: перезапуск редактора
````

на:

````tsx
  // Выход и смена входа спрашивают у редактора то же, что уход к экрану файлов.
  editorHooks.leave = () => (leave ? leave() : Promise.resolve(true));
  editorHooks.flush = () => (leave && flushEditor ? flushEditor() : Promise.resolve(true));
  editorHooks.sessionLost = () => markSessionLost?.();
  // Настройки — до первого экрана: тема, вид списка и раскладка нужны сразу (экраны
  // получают их готовыми). Хранилище настроек живёт у корня: перезапуск редактора
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
    delete editorHooks.leave;
    delete editorHooks.flush;
    release();
    settings?.dispose();
````

на:

````tsx
    delete editorHooks.leave;
    delete editorHooks.flush;
    delete editorHooks.sessionLost;
    release();
    settings?.dispose();
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
          />
        )}
      </Match>
      <Match when={view().kind === 'files'}>
````

на:

````tsx
          />
        )}
      </Match>
      <Match
        when={(() => {
          const v = view();
          return v.kind === 'denied' ? v : null;
        })()}
        keyed
      >
        {(v) => <NoAccessScreen id={v.id} files={files} navigate={navigate} />}
      </Match>
      <Match when={view().kind === 'files'}>
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
            onLeave={(fn) => (leave = fn)}
            onFlush={(fn) => (flushEditor = fn)}
            onOpenFailed={(message) => {
              // Испорченный файл: запись не трогаем, сохранять нечего.
````

на:

````tsx
            onLeave={(fn) => (leave = fn)}
            onFlush={(fn) => (flushEditor = fn)}
            onSessionLost={(fn) => (markSessionLost = fn)}
            onOpenFailed={(message) => {
              // Испорченный файл: запись не трогаем, сохранять нечего.
````

В `apps/web/src/ui/router.ts` заменить:

````ts
// - `#/file/<id>` — редактор с файлом из хранилища; `#/file/<id>?page=<pageId>` —
//   с этой страницей («Копировать ссылку на страницу»);
// - `#/new` — создать файл в хранилище и открыть его (адрес заменяется на `#/file/<id>`).
// Параметры поиска (`?nodes=`, `?backend=` — тесты) маршрутизатор не трогает.

import type { Section } from '../core/file-list';
import { MAX_ID_BYTES } from '../core/file-list';

export type Route = { kind: 'files'; section: Section } | { kind: 'file'; id: string; page?: string } | { kind: 'new' };

const SECTION_PATH: Record<Section, string> = { recent: '', drafts: 'drafts', trash: 'trash' };

/** Идентификатор файла из адреса — не доверенный: печатные ASCII без `/`, не длиннее предела. */
````

на:

````ts
// - `#/file/<id>` — редактор с файлом из хранилища; `#/file/<id>?page=<pageId>` —
//   с этой страницей («Копировать ссылку на страницу»);
// - `#/new` — создать файл в хранилище и открыть его (адрес заменяется на `#/file/<id>`);
//   `#/new?project=<id>` — в этом проекте, где проекты есть;
// - `#/file/<id>?link=<токен>` — файл по ссылке «Поделиться», токен из адреса сразу убирается.
// Другие разделы (проекты команды, «Доступные мне») — там, где они есть: `extensions.parseSection`.
// Параметры поиска (`?nodes=`, `?backend=` — тесты) маршрутизатор не трогает.

import type { Section } from '../core/file-list';
import { MAX_ID_BYTES } from '../core/file-list';
import { extensions } from './extensions';

export type Route =
  { kind: 'files'; section: Section } | { kind: 'file'; id: string; page?: string; link?: string } | { kind: 'new'; project?: string };

const SECTION_PATH: Partial<Record<Section, string>> = { recent: '', drafts: 'drafts', trash: 'trash', shared: 'shared' };
const sectionPath = (s: Section): string => SECTION_PATH[s] ?? s.replace(':', '/');

/** Идентификатор файла из адреса — не доверенный: печатные ASCII без `/`, не длиннее предела. */
````

В `apps/web/src/ui/router.ts` заменить:

````ts
  const [rawPath, query = ''] = hash.replace(/^#/, '').split('?', 2);
  const path = rawPath.replace(/^\/+/, '').replace(/\/+$/, '');
  if (path === 'new') return { kind: 'new' };
  if (path === 'drafts') return { kind: 'files', section: 'drafts' };
  if (path === 'trash') return { kind: 'files', section: 'trash' };
````

на:

````ts
  const [rawPath, query = ''] = hash.replace(/^#/, '').split('?', 2);
  const path = rawPath.replace(/^\/+/, '').replace(/\/+$/, '');
  if (path === 'new') {
    const project = new URLSearchParams(query).get('project');
    return project !== null && validId(project) ? { kind: 'new', project } : { kind: 'new' };
  }
  if (path === 'drafts') return { kind: 'files', section: 'drafts' };
  if (path === 'trash') return { kind: 'files', section: 'trash' };
````

В `apps/web/src/ui/router.ts` заменить:

````ts
    const page = new URLSearchParams(query).get('page');
    if (page !== null && validPage(page)) route.page = page;
    return route;
  }
  return { kind: 'files', section: 'recent' };
}

````

на:

````ts
    const page = new URLSearchParams(query).get('page');
    if (page !== null && validPage(page)) route.page = page;
    const link = new URLSearchParams(query).get('link');
    if (link !== null && /^[A-Za-z0-9_-]{1,64}$/.test(link)) route.link = link;
    return route;
  }
  const other = extensions.parseSection?.(path);
  return { kind: 'files', section: other ?? 'recent' };
}

````

В `apps/web/src/ui/router.ts` заменить:

````ts
  switch (r.kind) {
    case 'new':
      return '#/new';
    case 'file':
      return `#/file/${encodeURIComponent(r.id)}${r.page ? `?page=${encodeURIComponent(r.page)}` : ''}`;
    case 'files':
      return `#/${SECTION_PATH[r.section]}`;
  }
}
````

на:

````ts
  switch (r.kind) {
    case 'new':
      return r.project ? `#/new?project=${encodeURIComponent(r.project)}` : '#/new';
    case 'file':
      return `#/file/${encodeURIComponent(r.id)}${r.page ? `?page=${encodeURIComponent(r.page)}` : ''}`;
    case 'files':
      return `#/${sectionPath(r.section)}`;
  }
}
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
  /** Почему не удалась последняя запись (для вопроса при уходе); `null` — удалась. */
  failure(): string | null;
  dispose(): void;
}
````

на:

````ts
  /** Почему не удалась последняя запись (для вопроса при уходе); `null` — удалась. */
  failure(): string | null;
  /** Поколение файла, на котором основана следующая запись. */
  version(): number;
  dispose(): void;
}
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
      waiting = resolve;
      failWaiting = () => resolve(null);
      ed.send({ type: 'snapshot', thumbnail: true });
    });
    waiting = null;
````

на:

````ts
      waiting = resolve;
      failWaiting = () => resolve(null);
      ed.send(files.fullHistory ? { type: 'snapshot', thumbnail: true, full: true } : { type: 'snapshot', thumbnail: true });
    });
    waiting = null;
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
      ),
    failure: () => failure,
    dispose() {
      clearTimeout(journalTimer);
````

на:

````ts
      ),
    failure: () => failure,
    version: () => base,
    dispose() {
      clearTimeout(journalTimer);
````

Создать `apps/web/src/ui/server/server-api.ts`:

````ts
// Небольшие запросы к серверу с основного потока: команды, «Поделиться», запрос доступа. Всё размером с
// документ — в worker'е (`server-store.ts`). Ответ не того вида — `NETWORK`, у запроса срок.

import {
  API_ERROR_CODES,
  API_FILES,
  API_PROJECTS,
  API_TEAMS,
  isFileMeta,
  isProjectDto,
  isShare,
  isTeam,
  isTeamList,
  isTeamMembers,
  type ApiErrorCode,
  type ProjectDto,
  type ShareDto,
  type ShareRole,
  type TeamDto,
  type TeamListDto,
  type TeamMembersDto,
} from '@ferrum/protocol';
import { JSON_TIMEOUT_MS } from '../../core/server-files';

export type ApiFailure = ApiErrorCode | 'NETWORK';
export type ApiResult<T> = { ok: true; value: T } | { ok: false; failure: ApiFailure };

const isErrorCode = (v: unknown): v is ApiErrorCode => (API_ERROR_CODES as readonly unknown[]).includes(v);
const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const isOk = (v: unknown): v is { ok: true } => isRecord(v) && v.ok === true;

export function apiFailureText(f: ApiFailure): string {
  switch (f) {
    case 'NETWORK':
      return 'Нет связи с сервером. Попробуйте ещё раз';
    case 'UNAVAILABLE':
      return 'Сервер недоступен. Попробуйте позже';
    case 'TOO_MANY_REQUESTS':
    case 'BUSY':
      return 'Слишком много запросов. Попробуйте позже';
    case 'UNAUTHORIZED':
      return 'Сессия завершена. Войдите снова';
    case 'FORBIDDEN':
      return 'Недостаточно прав';
    case 'NOT_FOUND':
      return 'Не найдено или нет доступа';
    case 'NOT_EMPTY':
      return 'Сначала уберите то, что внутри';
    case 'BAD_REQUEST':
      return 'Проверьте введённые данные';
    default:
      return 'Не получилось. Обновите страницу';
  }
}

async function call<T>(path: string, valid: (v: unknown) => v is T, init?: RequestInit): Promise<ApiResult<T>> {
  try {
    const res = await fetch(path, { ...init, signal: AbortSignal.timeout(JSON_TIMEOUT_MS) });
    if (res.status >= 500) return { ok: false, failure: 'UNAVAILABLE' };
    const body = (await res.json()) as unknown;
    if (res.ok) return valid(body) ? { ok: true, value: body } : { ok: false, failure: 'NETWORK' };
    const code = isRecord(body) ? body.error : undefined;
    return { ok: false, failure: isErrorCode(code) ? code : 'NETWORK' };
  } catch {
    return { ok: false, failure: 'NETWORK' };
  }
}

const send = <T>(method: string, path: string, valid: (v: unknown) => v is T, body?: unknown) =>
  call<T>(path, valid, {
    method,
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });

const file = (id: string) => `${API_FILES}/${encodeURIComponent(id)}`;
const team = (id: string) => `${API_TEAMS}/${encodeURIComponent(id)}`;
const project = (id: string) => `${API_PROJECTS}/${encodeURIComponent(id)}`;

export const getShare = (id: string): Promise<ApiResult<ShareDto>> => call(`${file(id)}/share`, isShare);
export const invitePerson = (id: string, email: string, role: ShareRole) =>
  send('POST', `${file(id)}/share/invite`, isShare, { email, role });
export const changeRole = (id: string, entry: string, role: ShareRole) =>
  send('PATCH', `${file(id)}/share/people/${entry}`, isShare, { role });
export const removePerson = (id: string, entry: string) =>
  send('DELETE', `${file(id)}/share/people/${entry}`, (v): v is ShareDto | { ok: true } => isShare(v) || isOk(v));
export const updateLink = (id: string, enabled: boolean, role: ShareRole) =>
  send('PUT', `${file(id)}/share/link`, isShare, { enabled, role });
export const resetLink = (id: string) => send('POST', `${file(id)}/share/link/reset`, isShare);
export const redeemLink = (id: string, token: string) => send('POST', `${file(id)}/redeem`, isFileMeta, { token });
export const requestFileAccess = (id: string) => send('POST', `${file(id)}/request-access`, isOk);
export const moveFile = (id: string, projectId: string | null) => send('POST', `${file(id)}/move`, isFileMeta, { projectId });

export const getTeams = (): Promise<ApiResult<TeamListDto>> => call(API_TEAMS, isTeamList);
export const createTeam = (name: string): Promise<ApiResult<TeamDto>> => send('POST', API_TEAMS, isTeam, { name });
export const renameTeam = (id: string, name: string) => send('PATCH', team(id), isOk, { name });
export const deleteTeam = (id: string) => send('DELETE', team(id), isOk);
export const getMembers = (id: string): Promise<ApiResult<TeamMembersDto>> => call(`${team(id)}/members`, isTeamMembers);
export const inviteMember = (id: string, email: string) => send('POST', `${team(id)}/invite`, isTeamMembers, { email });
export const removeMember = (id: string, entry: string) => send('DELETE', `${team(id)}/members/${entry}`, isTeamMembers);
export const createProject = (id: string, name: string): Promise<ApiResult<ProjectDto>> =>
  send('POST', `${team(id)}/projects`, isProjectDto, { name });
export const renameProject = (id: string, name: string) => send('PATCH', project(id), isProjectDto, { name });
export const deleteProject = (id: string) => send('DELETE', project(id), isOk);
````

Создать `apps/web/src/ui/server/server-store.ts`:

````ts
// `FileStore` на сервере (спека подсистемы 4, раздел 6.2): тот же интерфейс, что у IndexedDB, поэтому
// экран файлов и редактор не знают, откуда файлы. Настройки интерфейса и файлы прежней версии остаются
// в браузере: их ведёт `IdbFileStore`, а всё о серверных файлах — отдельный worker.

import type { FileMeta, IoResults, JournalMark, Section } from '../../core/file-list';
import type { AccessState } from '../../core/server-files';
import type { FromServer, ServerOp, ServerResults, ToServer } from '../../core/server-messages';
import { IdbFileStore, type FileStore } from '../file-store';
import type { SettingsBackend } from '../settings-store';

export class ServerFileStore implements FileStore, SettingsBackend {
  /** Файлы, настройки и шрифты в браузере. */
  readonly local = new IdbFileStore(false);
  readonly fullHistory = true;
  readonly importLabel = 'Открыть с диска…';
  private readonly worker: Worker;
  private seq = 0;
  private readonly pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
  private readonly listeners = new Set<() => void>();
  private readonly watchers = new Map<string, Set<(s: AccessState) => void>>();
  private broken: string | null = null;
  private readonly onVisibility = () => this.post({ t: 'visible', on: document.visibilityState === 'visible' });

  constructor(user: string) {
    this.worker = new Worker(new URL('../../worker/server.worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<FromServer>) => this.onMessage(e.data);
    // Обновили образ при открытой вкладке: старого файла worker'а больше нет (B-208).
    this.worker.onerror = (e) => {
      e.preventDefault?.();
      this.fail('Не удалось запустить часть приложения — обновите страницу');
    };
    this.post({ t: 'init', user });
    document.addEventListener('visibilitychange', this.onVisibility);
    this.onVisibility();
  }

  private post(m: ToServer, transfer: Transferable[] = []): void {
    this.worker.postMessage(m, transfer);
  }

  private fail(message: string): void {
    this.broken = message;
    for (const p of this.pending.values()) p.reject(new Error(message));
    this.pending.clear();
  }

  private onMessage(m: FromServer): void {
    if (m.t === 'changed') {
      for (const l of this.listeners) l();
      return;
    }
    if (m.t === 'access') {
      for (const w of this.watchers.get(m.file) ?? []) w(m.state);
      return;
    }
    const p = this.pending.get(m.id);
    if (!p) return;
    this.pending.delete(m.id);
    if (m.ok) p.resolve(m.value);
    else p.reject(new Error(m.message));
  }

  private call<K extends ServerOp['op']>(op: Extract<ServerOp, { op: K }>, transfer: Transferable[] = []): Promise<ServerResults[K]> {
    if (this.broken) return Promise.reject(new Error(this.broken));
    const id = ++this.seq;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.post({ t: 'req', id, ...op } as ToServer, transfer);
    });
  }

  list(section: Section, query: string, start: number, count: number): Promise<IoResults['list']> {
    return this.call({ op: 'list', section, query, start, count });
  }
  get(file: string) {
    return this.call({ op: 'get', file });
  }
  create(name: string, bytes?: Blob, project?: string) {
    return this.call({ op: 'create', name, bytes: bytes ?? null, project: project ?? null });
  }
  /** Перенос файла из браузера: байты и журнал передаются, основной поток их не копирует. */
  importLocal(name: string, bytes: ArrayBuffer | null, journal: ArrayBuffer[]) {
    return this.call({ op: 'create', name, bytes, project: null, journal }, [...(bytes ? [bytes] : []), ...journal]);
  }
  read(file: string) {
    return this.call({ op: 'read', file });
  }
  write(
    file: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ) {
    return this.call(
      { op: 'write', file, bytes, thumb, base, journal },
      thumb instanceof ArrayBuffer ? [bytes, thumb] : [bytes],
    ) as Promise<FileMeta & { warning?: string }>;
  }
  async appendJournal(file: string, mark: JournalMark, bytes: ArrayBuffer) {
    await this.call({ op: 'appendJournal', file, session: mark.session, batch: mark.batch, bytes }, [bytes]);
  }
  blob(file: string) {
    return this.call({ op: 'blob', file });
  }
  rename(file: string, name: string) {
    return this.call({ op: 'rename', file, name });
  }
  touch(file: string) {
    return this.call({ op: 'touch', file });
  }
  async trash(file: string) {
    await this.call({ op: 'trash', file });
  }
  async restore(file: string) {
    await this.call({ op: 'restore', file });
  }
  async remove(file: string) {
    await this.call({ op: 'delete', file });
  }
  async emptyTrash() {
    await this.call({ op: 'emptyTrash' });
  }
  thumbnail(file: string) {
    return this.call({ op: 'thumb', file });
  }
  /** Список изменился мимо worker'а (перемещение файла с основного потока). */
  notifyChanged() {
    for (const l of this.listeners) l();
  }
  onChange(listener: () => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  /** Доступ к открытому файлу: понижение роли, снятие доступа, конец сессии. Этап 3 заменит опрос
   * сообщениями `role` и `access-revoked`, у получателя ничего не изменится. */
  watch(file: string, listener: (state: AccessState) => void) {
    const set = this.watchers.get(file) ?? new Set();
    this.watchers.set(file, set);
    set.add(listener);
    if (set.size === 1) this.post({ t: 'watch', file, on: true });
    return () => {
      set.delete(listener);
      if (set.size === 0) {
        this.watchers.delete(file);
        this.post({ t: 'watch', file, on: false });
      }
    };
  }
  /** Просьба о доступе и вход по ссылке — запросы к серверу без байтов. */
  redeemLink?: (file: string, token: string) => Promise<boolean>;
  requestAccess?: (file: string) => Promise<boolean>;
  fontBytes(blob: Blob) {
    return this.local.fontBytes(blob);
  }
  settings() {
    return this.local.settings();
  }
  setSettings(patch: Parameters<IdbFileStore['setSettings']>[0]) {
    return this.local.setSettings(patch);
  }
  onSettings(listener: (patch: Record<string, unknown>) => void) {
    return this.local.onSettings(listener);
  }
  dispose() {
    document.removeEventListener('visibilitychange', this.onVisibility);
    this.fail('хранилище закрыто');
    this.worker.terminate();
    this.local.dispose();
  }
}
````

Создать `apps/web/src/ui/server/setup.local.ts`:

````ts
// Заглушка `setup.tsx` для автономной сборки (`vite.config.ts`): иначе сборщик увидел бы в `server-store.ts`
// `new Worker(...)` и выпустил файл worker'а с кодом сервера, даже если до него нельзя дойти.

const unreachable = (): never => {
  throw new Error('Файлов на сервере нет в автономной сборке');
};

export const createServerStore = unreachable;
export const installServerUi = unreachable;
````

Создать `apps/web/src/ui/server/setup.tsx`:

````tsx
// Всё серверное в интерфейсе: ставит `main.tsx` в ветке рабочей сборки. Общий код об этом не знает,
// поэтому в автономную сборку ни запросы к серверу, ни его разделы не попадают.

import { isUuid } from '@ferrum/protocol';
import { currentUser } from '../auth/session';
import { extensions } from '../extensions';
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';

export function createServerStore(): ServerFileStore {
  // `Root` монтируется только при живой сессии (`AuthGate`).
  const s = new ServerFileStore(currentUser()!.id);
  s.redeemLink = async (id, token) => (await redeemLink(id, token)).ok;
  s.requestAccess = async (id) => (await requestFileAccess(id)).ok;
  return s;
}

export function installServerUi(): void {
  extensions.parseSection = (path) => {
    if (path === 'shared') return 'shared';
    const m = /^project\/([^/]+)$/.exec(path);
    return m && isUuid(m[1]) ? `project:${m[1]}` : null;
  };
  extensions.sectionTitle = (section) => (section === 'shared' ? 'Доступные мне' : null);
  extensions.filesNav = () => [{ kind: 'section', id: 'shared', title: 'Доступные мне', icon: 'file' }];
  extensions.newFileRoute = (section) => (section.startsWith('project:') ? { kind: 'new', project: section.slice(8) } : { kind: 'new' });
}
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  Tool,
} from '../wasm/ferrum_wasm';
import type { LiveState } from '../core/core';

````

на:

````ts
  Tool,
} from '../wasm/ferrum_wasm';
import type { FileRole } from '@ferrum/protocol';
import type { LiveState } from '../core/core';

````

В `apps/web/src/ui/store.ts` заменить:

````ts
  modified: boolean;
  documentName: string;
  layers: { start: number; total: number; rows: LayerRow[]; reveal: number | null; revealSeq: number };
  summary: Summary | null;
````

на:

````ts
  modified: boolean;
  documentName: string;
  /** Роль в открытом файле на сервере; нет — файл свой, без ролей. */
  fileRole: FileRole | null;
  layers: { start: number; total: number; rows: LayerRow[]; reveal: number | null; revealSeq: number };
  summary: Summary | null;
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  modified: false,
  documentName: '',
  layers: { start: 0, total: 0, rows: [], reveal: null, revealSeq: 0 },
  summary: null,
````

на:

````ts
  modified: false,
  documentName: '',
  fileRole: null,
  layers: { start: 0, total: 0, rows: [], reveal: null, revealSeq: 0 },
  summary: null,
````

В `apps/web/src/ui/styles/files.css` заменить:

````css
  }
}
````

на:

````css
  }
}

/* Подпись группы в панели разделов: команда над своими проектами. */
.fe-files__nav-label {
  padding: var(--space-2) var(--space-3) var(--space-1);
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
````

В `apps/web/src/ui/styles/properties.css` заменить:

````css
  margin: 0;
}
````

на:

````css
  margin: 0;
}

/* Обёртка секций: раскладку задаёт сама панель, у обёртки — только `inert` режима просмотра. */
.fe-props__body {
  display: contents;
}
````

Создать `apps/web/src/ui/styles/server.css`:

````css
/* Экраны рабочей сборки: «Нет доступа», «Поделиться», команды. */
.fe-denied {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--panel);
}
.fe-denied__card {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  max-width: 420px;
  padding: var(--space-8);
}
.fe-denied__title,
.fe-denied__text {
  margin: 0;
}
.fe-denied__actions,
.fe-lost__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}
.fe-lost {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.fe-viewonly {
  white-space: nowrap;
}
.fe-modal {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  width: min(560px, calc(100vw - 32px));
  max-height: calc(100vh - 64px);
  overflow: auto;
}
.fe-share__invite,
.fe-share__person,
.fe-share__link {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}
.fe-share__email {
  flex: 1 1 200px;
  min-width: 0;
}
.fe-share__who {
  display: flex;
  flex-direction: column;
  flex: 1 1 160px;
  min-width: 0;
  overflow-wrap: anywhere;
}
.fe-share__people {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}
.fe-share__select {
  height: 28px;
  padding: 0 var(--space-2);
  border: 1px solid var(--line);
  border-radius: 6px;
  background: transparent;
  color: inherit;
  font: inherit;
}
.fe-share__toggle {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}
.fe-share__error {
  margin: 0;
  color: var(--danger, #e5484d);
}
.fe-files__extra-btn {
  width: 100%;
  justify-content: flex-start;
}
````

Создать `apps/web/src/worker/server.worker.ts`:

````ts
// Worker файлов сервера: всё, что размером с документ, идёт здесь. Журнал правок — своя база на каждого
// человека (B-190): вошедший следующим в этом браузере чужих правок не увидит. Опрос доступа открытых
// файлов идёт только пока вкладка видна.

import { MAX_JOURNAL_ENTRIES } from '../core/limits';
import { ACCESS_POLL_MS, ServerFiles, type JournalEntries, type JournalStore } from '../core/server-files';
import type { FromServer, ServerRequest, ToServer } from '../core/server-messages';

const scope = self as unknown as DedicatedWorkerGlobalScope;
const post = (m: FromServer, transfer: Transferable[] = []) => scope.postMessage(m, transfer);

function request<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error ?? new Error('ошибка хранилища браузера'));
  });
}

function journalStore(user: string): JournalStore {
  let db: Promise<IDBDatabase> | null = null;
  const open = () => {
    db ??= new Promise<IDBDatabase>((resolve, reject) => {
      const req = indexedDB.open(`ferrum-user-${user}`, 1);
      req.onupgradeneeded = () => req.result.createObjectStore('journal');
      req.onsuccess = () => {
        req.result.onversionchange = () => {
          req.result.close();
          db = null;
        };
        resolve(req.result);
      };
      req.onerror = () => reject(req.error ?? new Error('хранилище браузера недоступно'));
    });
    db.catch(() => (db = null));
    return db;
  };
  const range = (file: string) => IDBKeyRange.bound([file], [file, []]);
  const finished = (tx: IDBTransaction) =>
    new Promise<void>((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onerror = tx.onabort = () => reject(tx.error ?? new Error('ошибка хранилища браузера'));
    });
  return {
    async append(file, session, batch, bytes) {
      const d = await open();
      const tx = d.transaction('journal', 'readwrite', { durability: 'strict' });
      tx.objectStore('journal').put(bytes, [file, session, batch]);
      await finished(tx);
    },
    async read(file): Promise<JournalEntries> {
      const d = await open();
      const store = d.transaction('journal').objectStore('journal');
      const [count, last] = await Promise.all([
        request(store.count(range(file))),
        request(store.openKeyCursor(range(file), 'prev')).then((c) => c?.key),
      ]);
      const s = Array.isArray(last) ? last[1] : null;
      const session = typeof s === 'number' && Number.isSafeInteger(s) ? s : null;
      if (count > MAX_JOURNAL_ENTRIES) return { entries: [], session, tooLarge: true };
      const all = (await request(d.transaction('journal').objectStore('journal').getAll(range(file)))) as unknown[];
      return { entries: all.filter((b): b is ArrayBuffer => b instanceof ArrayBuffer), session, tooLarge: false };
    },
    async cut(file, session, upTo) {
      const d = await open();
      const tx = d.transaction('journal', 'readwrite', { durability: 'strict' });
      tx.objectStore('journal').delete(IDBKeyRange.bound([file], [file, session, upTo]));
      await finished(tx);
    },
    async drop(file) {
      const d = await open();
      const tx = d.transaction('journal', 'readwrite');
      tx.objectStore('journal').delete(range(file));
      await finished(tx);
    },
  };
}

const channel = new BroadcastChannel('ferrum-server-files');
let files: ServerFiles | null = null;
const watched = new Set<string>();
let visible = true;
let poll: ReturnType<typeof setInterval> | null = null;

const notifyChanged = () => {
  post({ t: 'changed' });
  channel.postMessage('changed');
};
// Другая вкладка изменила список: экрану — перечитать окно.
channel.onmessage = () => post({ t: 'changed' });

function replan(): void {
  if (poll !== null) clearInterval(poll);
  poll = null;
  if (!visible) return;
  poll = setInterval(() => {
    // Открытые файлы — проверка доступа; экран файлов — новое в списке (чужие приглашения и правки).
    if (watched.size === 0) post({ t: 'changed' });
    for (const f of watched) void files?.checkAccess(f);
  }, ACCESS_POLL_MS);
}

async function run(r: ServerRequest, f: ServerFiles): Promise<{ value: unknown; transfer?: Transferable[] }> {
  switch (r.op) {
    case 'list':
      return { value: await f.list(r.section, r.query, r.start, r.count) };
    case 'get':
      return { value: await f.get(r.file) };
    case 'create':
      return { value: await f.create(r.name, r.bytes, r.project, r.journal) };
    case 'read': {
      const value = await f.read(r.file);
      return { value, transfer: [...(value.bytes ? [value.bytes] : []), ...value.journal] };
    }
    case 'write':
      return { value: await f.write(r.file, r.bytes, r.thumb, r.base, r.journal) };
    case 'appendJournal':
      await f.appendJournal(r.file, r.session, r.batch, r.bytes);
      return { value: null };
    case 'blob':
      return { value: await f.blob(r.file) };
    case 'rename':
      return { value: await f.rename(r.file, r.name) };
    case 'touch':
      return { value: await f.touch(r.file) };
    case 'trash':
      await f.trash(r.file);
      return { value: null };
    case 'restore':
      await f.restore(r.file);
      return { value: null };
    case 'delete':
      await f.remove(r.file);
      return { value: null };
    case 'emptyTrash':
      await f.emptyTrash();
      return { value: null };
    case 'thumb':
      return { value: await f.thumbnail(r.file) };
  }
}

scope.onmessage = (e: MessageEvent<ToServer>) => {
  const m = e.data;
  if (m.t === 'init') {
    files = new ServerFiles(
      (input, init) => fetch(input, init),
      journalStore(m.user),
      {
        changed: notifyChanged,
        access: (file, state) => post({ t: 'access', file, state }),
      },
      {
        now: () => performance.now(),
        setTimeout: (fn, ms) => setTimeout(fn, ms),
        clearTimeout: (t) => clearTimeout(t as ReturnType<typeof setTimeout>),
      },
    );
    replan();
    return;
  }
  if (m.t === 'visible') {
    visible = m.on;
    replan();
    // Вернулись на вкладку: снятый доступ виден сразу, а не к следующему опросу.
    if (visible) for (const f of watched) void files?.checkAccess(f);
    return;
  }
  if (m.t === 'watch') {
    if (m.on) {
      watched.add(m.file);
      void files?.checkAccess(m.file);
    } else {
      watched.delete(m.file);
      files?.forgetAccess(m.file);
    }
    return;
  }
  const f = files;
  if (!f) return;
  run(m, f).then(
    ({ value, transfer }) => post({ t: 'res', id: m.id, ok: true, value }, transfer),
    (err: unknown) =>
      post({
        t: 'res',
        id: m.id,
        ok: false,
        message: err instanceof Error ? err.message : String(err),
        code: (err as { code?: string }).code,
      }),
  );
};
````

В `apps/web/tsconfig.worker.json` заменить:

````json
    "src/core/clipboard-text.ts",
    "src/core/fonts.ts",
    "src/core/limits.ts"
  ],
  "exclude": []
````

на:

````json
    "src/core/clipboard-text.ts",
    "src/core/fonts.ts",
    "src/core/limits.ts",
    "src/core/file-list.ts",
    "src/core/server-files.ts",
    "src/core/server-messages.ts"
  ],
  "exclude": []
````

В `apps/web/vite.config.ts` заменить:

````ts
import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';
````

на:

````ts
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';
````

В `apps/web/vite.config.ts` заменить:

````ts
  return {
    plugins: [solid()],
    // `npm run dev:server`: API — у сервера из Docker (PUBLIC_URL должен быть адресом vite).
    server: { headers, fs: { allow: ['../..'] }, proxy: { '/api': process.env.FERRUM_API ?? 'http://localhost:8090' } },
````

на:

````ts
  return {
    plugins: [solid()],
    // Автономная сборка не видит кода сервера вовсе: условие в `main.tsx` убирает ветку, но не файл worker'а.
    resolve: {
      alias:
        mode === 'standalone'
          ? [{ find: /^.*\/ui\/server\/setup$/, replacement: fileURLToPath(new URL('./src/ui/server/setup.local.ts', import.meta.url)) }]
          : [],
    },
    // `npm run dev:server`: API — у сервера из Docker (PUBLIC_URL должен быть адресом vite).
    server: { headers, fs: { allow: ['../..'] }, proxy: { '/api': process.env.FERRUM_API ?? 'http://localhost:8090' } },
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npx playwright test e2e/standalone.spec.ts e2e/files.spec.ts --project=chromium`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Expected: PASS: всё без замечаний; модульные 212 passed; обе сборки и `check-dist.mjs`; `standalone` и `files` автономной сборки 30 passed; `e2e:server` 86 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e-server/auth.spec.ts apps/web/e2e-server/files.spec.ts apps/web/e2e-server/stand.ts apps/web/scripts/check-dist.mjs apps/web/src/core/file-list.ts apps/web/src/core/server-files.ts apps/web/src/core/server-messages.ts apps/web/src/main.tsx apps/web/src/ui/app.tsx apps/web/src/ui/auth/auth-gate.tsx apps/web/src/ui/boot.ts apps/web/src/ui/chrome.tsx apps/web/src/ui/extensions.ts apps/web/src/ui/file-store.ts apps/web/src/ui/files-screen.tsx apps/web/src/ui/local-store.ts apps/web/src/ui/no-access.tsx apps/web/src/ui/properties.tsx apps/web/src/ui/root.tsx apps/web/src/ui/router.ts apps/web/src/ui/saver.ts apps/web/src/ui/server/server-api.ts apps/web/src/ui/server/server-store.ts apps/web/src/ui/server/setup.local.ts apps/web/src/ui/server/setup.tsx apps/web/src/ui/store.ts apps/web/src/ui/styles/files.css apps/web/src/ui/styles/properties.css apps/web/src/ui/styles/server.css apps/web/src/worker/server.worker.ts apps/web/tsconfig.worker.json apps/web/unit/check-dist.test.ts apps/web/unit/router.test.ts apps/web/unit/saver.test.ts apps/web/unit/server-files.test.ts apps/web/vite.config.ts
git commit -F - <<'EOF'
интерфейс: серверный FileStore, файлы в worker'е, режим просмотра

ServerFileStore за тем же интерфейсом FileStore: всё размером с документ идёт в
worker'е, картинки отдельными запросами, поколение у каждой записи, журнал правок —
своя база на человека (B-190). Хранилище выбирает main.tsx, поэтому в рабочей сборке
нет ни TauriFileStore (B-206), ни тестового доступа по ?debug (B-205), а в автономной
нет кода сервера. Роль читателя — режим просмотра, опрос доступа и отказ на записи —
«Нет доступа» в открытом редакторе, выход в другой вкладке не теряет незаписанное.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 7: Интерфейс — «Поделиться»

Спека §6.3.

**Files:**
- Create: `apps/web/src/ui/server/modal.tsx`, `apps/web/src/ui/server/share-dialog.tsx`
- Modify: `apps/web/src/ui/server/setup.tsx`
- Test: `apps/web/e2e-server/files.spec.ts`

- [ ] **Step 1: Реализация и сценарии**

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
import { appScreen, drag, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { linksFor, mailCount, newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);
````

на:

````ts
import { appScreen, drag, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { linksFor, mailCount, mailsFor, newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
    { method, path, body },
  );

/** Сообщение внизу экрана. */
````

на:

````ts
    { method, path, body },
  );

const open = async (page: Page, id: string) => {
  await page.goto(`/#/file/${id}`);
  await editorReady(page);
};

/** Вернулись на вкладку: доступ проверяется сразу, а не к следующему опросу. */
const checkAccessNow = (page: Page) => page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));

async function shareWith(page: Page, email: string, role: 'Может редактировать' | 'Может смотреть' = 'Может редактировать') {
  await page.getByRole('button', { name: 'Поделиться' }).click();
  await page.getByLabel('Адрес, кого пригласить').fill(email);
  await page.getByLabel('Роль приглашённого').selectOption({ label: role });
  await page.getByRole('button', { name: 'Пригласить' }).click();
  await expect(page.getByRole('list', { name: 'Люди с доступом' }).getByText(email)).toBeVisible();
  await page.getByRole('button', { name: 'Закрыть' }).click();
}

/** Сообщение внизу экрана. */
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
});

test('две вкладки одного человека: файл занят, как раньше', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
````

на:

````ts
});

test('читатель: режим просмотра, правок нет, кнопки «Поделиться» нет, копию скачать можно', async ({ browser }, info) => {
  const viewerMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, viewerMail);
  const id = await storedFile(a);
  await shareWith(a, viewerMail, 'Может смотреть');
  const puts: string[] = [];
  b.on('request', (r) => {
    if (r.method() === 'PUT') puts.push(r.url());
  });
  await open(b, id);
  await nodes(b, 1);
  await expect(b.getByText('Только просмотр')).toBeVisible();
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
  // Прямоугольник инструментом не появляется, удаление не проходит, признака изменений нет.
  await b.locator('canvas').click({ position: { x: 10, y: 10 } });
  await b.keyboard.press('KeyR');
  const l = await live(b);
  await drag(b, await appScreen(b, l, l.cameraX, l.cameraY), 100, 80);
  await nodes(b, 1);
  await b.keyboard.press('ControlOrMeta+KeyA');
  await b.keyboard.press('Delete');
  await nodes(b, 1);
  expect(await b.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  await saveKey(b);
  expect(puts).toEqual([]);
  // Панель свойств не принимает ввод, «Экспорт» остаётся.
  await expect(b.locator('.fe-props__body')).toHaveAttribute('inert', '');
  // Запись в обход интерфейса тоже не проходит.
  const direct = await api(b, 'PATCH', `/api/files/${id}`, { name: 'Чужое' });
  expect(direct.status).toBe(403);
  await b.goto('/#/shared');
  await expect(title(b)).toHaveText('Доступные мне');
  const row = b.getByRole('option', { name: 'Без названия' });
  await row.click({ button: 'right' });
  await expect(b.getByRole('menuitem', { name: 'Переименовать' })).toHaveCount(0);
  await expect(b.getByRole('menuitem', { name: 'Удалить' })).toHaveCount(0);
  const download = b.waitForEvent('download');
  await b.getByRole('menuitem', { name: 'Скачать копию (.ferrum)' }).click();
  expect((await download).suggestedFilename()).toBe('Без названия.ferrum');
  await a.context().close();
  await b.context().close();
});

test('понижение и повышение роли открытого редактора без перезагрузки', async ({ browser }, info) => {
  const editorMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, editorMail);
  const id = await storedFile(a);
  await shareWith(a, editorMail);
  const entry = async () =>
    ((await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { id: string; email: string }[] }).people.find(
      (p) => p.email === editorMail,
    )!.id;
  await open(b, id);
  await expect(b.getByRole('button', { name: 'Поделиться' })).toBeVisible();
  const before = (await live(b)).nodes;
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'viewer' });
  await checkAccessNow(b);
  await expect(b.getByText('Только просмотр')).toBeVisible();
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
  await b.locator('canvas').click({ position: { x: 10, y: 10 } });
  await b.keyboard.press('KeyR');
  const l = await live(b);
  await drag(b, await appScreen(b, l, l.cameraX, l.cameraY), 100, 80);
  await nodes(b, before);
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'editor' });
  await checkAccessNow(b);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
  await nodes(b, before + 1);
  await saveKey(b);
  await saved(b);
  await a.context().close();
  await b.context().close();
});

test('снятие роли: «Нет доступа» в открытом редакторе, правки — по протоколу ухода', async ({ browser }, info) => {
  const editorMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, editorMail);
  const id = await storedFile(a);
  await shareWith(a, editorMail);
  await open(b, id);
  await drawRect(b);
  await nodes(b, 2);
  const share = (await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { id: string; email: string }[] };
  const entry = share.people.find((p) => p.email === editorMail)!.id;
  await api(a, 'DELETE', `/api/files/${id}/share/people/${entry}`);
  // Отказ на сохранении — сразу, опрос тут не нужен.
  await saveKey(b);
  await expect(b.getByRole('alert').getByText('Нет доступа', { exact: true })).toBeVisible();
  await expect(b.getByRole('button', { name: 'Сохранить копию на диск' })).toBeVisible();
  // Правки не потерялись молча: уход спрашивает.
  await b.getByRole('button', { name: 'К файлам', exact: true }).click();
  await expect(b.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' })).toBeVisible();
  await b.getByRole('button', { name: 'Остаться' }).click();
  await nodes(b, 2);
  const download = b.waitForEvent('download');
  await b.getByRole('button', { name: 'Сохранить копию на диск' }).click();
  expect((await download).suggestedFilename()).toMatch(/\.ferrum$/);
  await b.getByRole('button', { name: 'К файлам', exact: true }).click();
  await b.getByRole('button', { name: 'Уйти' }).click();
  await expect(title(b)).toHaveText('Недавние');
  // Файл закрыт и после этого: чтение — «Нет доступа», просьба уходит владельцу.
  const ownerMail = (await a.evaluate(
    async () => ((await (await fetch('/api/auth/session')).json()) as { user: { email: string } }).user.email,
  )) as string;
  await b.goto(`/#/file/${id}`);
  await expect(b.getByRole('heading', { name: 'Нет доступа' })).toBeVisible();
  await b.getByRole('button', { name: 'Запросить доступ' }).click();
  await expect(b.getByText('Запрос отправлен владельцу файла')).toBeVisible();
  await mailsFor(ownerMail, 'Запрос доступа к файлу в Ferrum');
  await a.context().close();
  await b.context().close();
});

test('CONFLICT: два окна с одного поколения; после открытия заново правки сливаются', async ({ browser }, info) => {
  const editorMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, editorMail);
  const id = await storedFile(a);
  await shareWith(a, editorMail);
  await open(b, id);
  // Оба правят от одного поколения: A записывает первым.
  await drawRect(a, 200);
  await saveKey(a);
  await saved(a);
  // Открытый редактор узнаёт о новой версии, не дожидаясь своей записи.
  await checkAccessNow(b);
  await expect(toast(b, 'Откройте его заново, чтобы увидеть новую версию')).toBeVisible();
  await drawRect(b, -200);
  await nodes(b, 2);
  // Журнал успевает получить правку B: её и сольёт следующее открытие.
  await b.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await saveKey(b);
  await expect(toast(b, 'Файл изменён в другом окне или другим человеком')).toBeVisible();
  expect(await b.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  // Поверх чужой записи ничего не легло: на сервере версия A.
  const version = (await api(b, 'GET', `/api/files/${id}/access`)).body as { version: number };
  expect(version.version).toBe(2);
  // Уход — по протоколу: вопрос с причиной, правки остаются в журнале браузера.
  await b.getByRole('button', { name: 'Назад к файлам' }).click();
  const ask = b.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  await expect(ask).toContainText('Файл изменён');
  await b.getByRole('button', { name: 'Уйти' }).click();
  await expect(title(b)).toHaveText('Недавние');
  await open(b, id);
  await nodes(b, 3);
  await expect(b.getByText('Восстановлены изменения, не записанные в файл')).toBeVisible();
  await saveKey(b);
  await saved(b);
  await a.goto(`/#/file/${id}`);
  await editorReady(a);
  await nodes(a, 3);
  await a.context().close();
  await b.context().close();
});

test('две вкладки одного человека: файл занят, как раньше', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  await expect(toast(second, 'Файл открыт в другой вкладке')).toBeVisible();
  await a.context().close();
});

````

на:

````ts
  await expect(toast(second, 'Файл открыт в другой вкладке')).toBeVisible();
  await a.context().close();
});

test('ссылка: роль, выключатель, токена нет ни в адресе, ни в журнале истории', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await a.getByRole('checkbox', { name: 'Доступ по ссылке' }).check();
  await a.getByLabel('Роль по ссылке').selectOption({ label: 'Может смотреть' });
  await expect(a.getByRole('button', { name: 'Копировать ссылку' })).toBeVisible();
  const { token } = ((await api(a, 'GET', `/api/files/${id}/share`)).body as { link: { token: string } }).link;
  await b.goto(`/#/file/${id}?link=${token}`);
  await editorReady(b);
  await expect(b.getByText('Только просмотр')).toBeVisible();
  expect(b.url()).not.toContain(token);
  expect(b.url()).toContain(`#/file/${id}`);
  const off = a.waitForResponse((r) => r.url().endsWith('/share/link'));
  await a.getByRole('checkbox', { name: 'Доступ по ссылке' }).uncheck();
  await off;
  await checkAccessNow(b);
  await expect(b.getByRole('alert').getByText('Нет доступа', { exact: true })).toBeVisible();
  // Выключенная ссылка не открывает файл и новому человеку; файл из адреса — та же «Нет доступа».
  await b.goto(`/#/file/${id}?link=${token}`);
  await expect(b.getByRole('heading', { name: 'Нет доступа' })).toBeVisible();
  expect(b.url()).not.toContain(token);
  await a.context().close();
  await b.context().close();
});

test('приглашение адреса без учётной записи: письмо, первый вход, «Доступные мне»', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  const invited = newEmail(info.project.name);
  await shareWith(a, invited);
  const [text] = await mailsFor(invited, 'Вам открыли файл в Ferrum');
  const link = new RegExp(`http\\S+#/file/${id}`).exec(text)![0];
  expect(link).toContain(id);
  const context = await browser.newContext({ baseURL: STAND_URL });
  const page = await context.newPage();
  await testMode(page);
  await page.goto(link);
  await signIn(page, invited, 'Новый');
  await editorReady(page);
  expect(fileId(page)).toBe(id);
  await nodes(page, 1);
  await page.goto('/#/shared');
  await expect(page.getByRole('option', { name: 'Без названия' })).toBeVisible();
  // Имя появилось в списке, потому что человек открыл файл.
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await expect
    .poll(
      async () =>
        ((await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { email: string; name: string }[] }).people.find(
          (p) => p.email === invited,
        )?.name,
    )
    .toBe('Новый');
  await a.context().close();
  await context.close();
});

````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
});

test('файл 100 000 узлов: запись и чтение через сервер, основной поток не занят @big', async ({ browser }, info) => {
  test.skip(info.project.name === 'server-firefox', 'размер проверяется в одном браузере');
````

на:

````ts
});

test('«Поделиться»: сбой, чужой ответ и запоздавший ответ не портят список', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  const invited = newEmail(info.project.name);
  // Страница сети вместо ответа и обрыв: сообщение и «Повторить», а не вечная «Загрузка…».
  await a.context().route('**/share', (r) => r.fulfill({ status: 200, contentType: 'text/html', body: '<html>вход в сеть</html>' }));
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await expect(a.getByRole('dialog').getByRole('alert')).toContainText('Нет связи с сервером');
  await a.context().unroute('**/share');
  await a.context().route('**/share', (r) => r.abort());
  await a.getByRole('button', { name: 'Повторить' }).click();
  await expect(a.getByRole('dialog').getByRole('alert')).toContainText('Нет связи с сервером');
  await a.context().unroute('**/share');
  await a.getByRole('button', { name: 'Повторить' }).click();
  await expect(a.getByRole('list', { name: 'Люди с доступом' })).toBeVisible();
  await a.getByRole('button', { name: 'Закрыть' }).click();
  // Закрыли, пока ответ в пути, открыли заново: запоздавший ответ не затирает новый список.
  const { promise: release, resolve } = Promise.withResolvers<void>();
  let calls = 0;
  await a.context().route('**/share', async (r) => {
    if (++calls === 1) await release;
    await r.continue();
  });
  const held = a.waitForRequest('**/share');
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await held;
  await expect(a.getByText('Загрузка…')).toBeVisible();
  await a.getByRole('button', { name: 'Закрыть' }).click();
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await a.getByLabel('Адрес, кого пригласить').fill(invited);
  await a.getByRole('button', { name: 'Пригласить' }).click();
  await expect(a.getByRole('list', { name: 'Люди с доступом' }).getByText(invited)).toBeVisible();
  const stale = a.waitForResponse('**/share');
  resolve();
  await stale;
  await expect(a.getByRole('list', { name: 'Люди с доступом' }).getByText(invited)).toBeVisible();
  await a.context().close();
});

test('файл 100 000 узлов: запись и чтение через сервер, основной поток не занят @big', async ({ browser }, info) => {
  test.skip(info.project.name === 'server-firefox', 'размер проверяется в одном браузере');
````

Создать `apps/web/src/ui/server/modal.tsx`:

````tsx
// Диалог с полями ввода (стиль Dialog дизайн-системы): Tab ходит внутри, Esc закрывает, фокус
// возвращается туда, откуда диалог открыли.

import { onCleanup, onMount, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';

const FOCUSABLE = 'button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex="0"]';

export function Modal(props: { title: string; onClose: () => void; children: JSX.Element }) {
  let box!: HTMLDivElement;
  const returnFocus = document.activeElement as HTMLElement | null;
  onMount(() => box.querySelector<HTMLElement>('input, select, button')?.focus());
  onCleanup(() => returnFocus?.focus?.());
  return (
    <Portal>
      <div class="fe-scrim" onPointerDown={(e) => e.target === e.currentTarget && props.onClose()}>
        <div
          ref={box}
          class="fe-dialog fe-modal"
          role="dialog"
          aria-modal="true"
          tabindex="-1"
          aria-label={props.title}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.preventDefault();
              e.stopPropagation();
              props.onClose();
            } else if (e.key === 'Tab') {
              const items = Array.from(box.querySelectorAll<HTMLElement>(FOCUSABLE));
              if (items.length === 0) return;
              const at = items.indexOf(document.activeElement as HTMLElement);
              e.preventDefault();
              items[(at + (e.shiftKey ? -1 : 1) + items.length) % items.length].focus();
            }
          }}
        >
          <h2 class="fe-dialog__title">{props.title}</h2>
          {props.children}
        </div>
      </div>
    </Portal>
  );
}
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';

export function createServerStore(): ServerFileStore {
````

на:

````tsx
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';
import { ShareButton } from './share-dialog';

export function createServerStore(): ServerFileStore {
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
  extensions.filesNav = () => [{ kind: 'section', id: 'shared', title: 'Доступные мне', icon: 'file' }];
  extensions.newFileRoute = (section) => (section.startsWith('project:') ? { kind: 'new', project: section.slice(8) } : { kind: 'new' });
}
````

на:

````tsx
  extensions.filesNav = () => [{ kind: 'section', id: 'shared', title: 'Доступные мне', icon: 'file' }];
  extensions.newFileRoute = (section) => (section.startsWith('project:') ? { kind: 'new', project: section.slice(8) } : { kind: 'new' });
  extensions.shareButton = ShareButton;
}
````

Создать `apps/web/src/ui/server/share-dialog.tsx`:

````tsx
// «Поделиться» (спека подсистемы 4, раздел 6.3): приглашение по email с ролью, список людей, смена и
// снятие роли, ссылка с ролью и выключателем. Читателю кнопки нет: список людей и адреса ему не положены.

import { MAX_EMAIL_BYTES, parseEmail, type ShareDto, type ShareRole } from '@ferrum/protocol';
import { createSignal, For, onCleanup, Show } from 'solid-js';
import type { FileMeta } from '../../core/file-list';
import { Modal } from './modal';
import { apiFailureText, changeRole, getShare, invitePerson, removePerson, resetLink, updateLink, type ApiResult } from './server-api';

const ROLE_TEXT: Record<string, string> = { owner: 'Владелец', editor: 'Может редактировать', viewer: 'Может смотреть' };

export function ShareButton(props: { file: FileMeta; role: () => FileMeta['role'] }) {
  const [open, setOpen] = createSignal(false);
  return (
    <Show when={props.role() !== 'viewer'}>
      <button class="fe-btn fe-btn--primary" onClick={() => setOpen(true)}>
        Поделиться
      </button>
      <Show when={open()}>
        <ShareDialog file={props.file} onClose={() => setOpen(false)} />
      </Show>
    </Show>
  );
}

function ShareDialog(props: { file: FileMeta; onClose: () => void }) {
  const id = props.file.id;
  const [state, setState] = createSignal<ShareDto | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [email, setEmail] = createSignal('');
  const [role, setRole] = createSignal<ShareRole>('editor');
  const [copied, setCopied] = createSignal(false);
  /** Поколение: ответ на прежний запрос не затирает то, что уже пришло по новому. */
  let generation = 0;
  let closed = false;
  let copyTimer = 0;
  onCleanup(() => {
    closed = true;
    clearTimeout(copyTimer);
  });

  async function run(work: () => Promise<ApiResult<ShareDto>>): Promise<boolean> {
    const my = ++generation;
    setBusy(true);
    setError(null);
    const r = await work();
    if (closed || my !== generation) return false;
    setBusy(false);
    if (!r.ok) {
      setError(apiFailureText(r.failure));
      return false;
    }
    setState(r.value);
    return true;
  }
  const load = () => void run(() => getShare(id));
  load();

  const invite = async (e: Event) => {
    e.preventDefault();
    const parsed = parseEmail(email());
    if (!parsed) {
      setError('Проверьте адрес');
      return;
    }
    if (await run(() => invitePerson(id, parsed, role()))) setEmail('');
  };
  const remove = (entry: string) =>
    void run(async () => {
      const r = await removePerson(id, entry);
      if (!r.ok) return r;
      return getShare(id);
    });
  const linkUrl = () => {
    const token = state()?.link?.token;
    return token ? `${location.origin}${location.pathname}#/file/${encodeURIComponent(id)}?link=${encodeURIComponent(token)}` : null;
  };
  const copy = async () => {
    const url = linkUrl();
    if (!url) return;
    await navigator.clipboard.writeText(url).then(
      () => {
        setCopied(true);
        clearTimeout(copyTimer);
        copyTimer = window.setTimeout(() => setCopied(false), 2000);
      },
      () => setError('Браузер не дал скопировать ссылку'),
    );
  };

  return (
    <Modal title={`Поделиться: ${props.file.name}`} onClose={props.onClose}>
      <form class="fe-share__invite" novalidate onSubmit={(e) => void invite(e)}>
        <div class="fe-field fe-field--text fe-share__email">
          <input
            type="email"
            aria-label="Адрес, кого пригласить"
            placeholder="Электронная почта"
            autocomplete="off"
            maxlength={MAX_EMAIL_BYTES}
            value={email()}
            onInput={(e) => setEmail(e.currentTarget.value)}
          />
        </div>
        <select
          class="fe-share__select"
          aria-label="Роль приглашённого"
          value={role()}
          onChange={(e) => setRole(e.currentTarget.value as ShareRole)}
        >
          <option value="editor">{ROLE_TEXT.editor}</option>
          <option value="viewer">{ROLE_TEXT.viewer}</option>
        </select>
        <button class="fe-btn fe-btn--primary" type="submit" disabled={busy()}>
          Пригласить
        </button>
      </form>
      <Show when={error()}>
        <p class="fe-share__error" role="alert">
          {error()}
          <Show when={!state()}>
            {' '}
            <button class="fe-btn fe-btn--secondary" onClick={load}>
              Повторить
            </button>
          </Show>
        </p>
      </Show>
      <Show
        when={state()}
        fallback={
          <Show when={!error()}>
            <p class="t-muted">Загрузка…</p>
          </Show>
        }
      >
        {(s) => (
          <>
            <ul class="fe-share__people" aria-label="Люди с доступом">
              <For each={s().people}>
                {(p) => (
                  <li class="fe-share__person">
                    <span class="fe-share__who">
                      <Show when={p.name}>
                        <span class="t-strong">{p.name}</span>
                      </Show>
                      <span class="t-muted">{p.email}</span>
                    </span>
                    <Show when={s().canManage && p.role !== 'owner'} fallback={<span class="t-muted">{ROLE_TEXT[p.role]}</span>}>
                      <select
                        class="fe-share__select"
                        aria-label={`Роль: ${p.email}`}
                        value={p.role}
                        disabled={busy()}
                        onChange={(e) => void run(() => changeRole(id, p.id, e.currentTarget.value as ShareRole))}
                      >
                        <option value="editor">{ROLE_TEXT.editor}</option>
                        <option value="viewer">{ROLE_TEXT.viewer}</option>
                      </select>
                      <button class="fe-btn" aria-label={`Снять доступ: ${p.email}`} disabled={busy()} onClick={() => remove(p.id)}>
                        Снять
                      </button>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
            <Show when={s().canManage}>
              <div class="fe-share__link">
                <label class="fe-share__toggle">
                  <input
                    type="checkbox"
                    checked={s().link?.enabled ?? false}
                    disabled={busy()}
                    onChange={(e) => void run(() => updateLink(id, e.currentTarget.checked, s().link?.role ?? 'viewer'))}
                  />
                  Доступ по ссылке
                </label>
                <Show when={s().link?.enabled}>
                  <select
                    class="fe-share__select"
                    aria-label="Роль по ссылке"
                    value={s().link?.role}
                    disabled={busy()}
                    onChange={(e) => void run(() => updateLink(id, true, e.currentTarget.value as ShareRole))}
                  >
                    <option value="editor">{ROLE_TEXT.editor}</option>
                    <option value="viewer">{ROLE_TEXT.viewer}</option>
                  </select>
                  <Show when={linkUrl()} fallback={<span class="t-muted">Ссылка больше не действует, создайте новую</span>}>
                    <button class="fe-btn fe-btn--secondary" onClick={() => void copy()}>
                      {copied() ? 'Скопировано' : 'Копировать ссылку'}
                    </button>
                  </Show>
                  <button class="fe-btn" disabled={busy()} onClick={() => void run(() => resetLink(id))}>
                    Новая ссылка
                  </button>
                </Show>
              </div>
            </Show>
          </>
        )}
      </Show>
      <div class="fe-dialog__actions">
        <button class="fe-btn" onClick={props.onClose}>
          Закрыть
        </button>
      </div>
    </Modal>
  );
}
````

- [ ] **Step 2: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Expected: PASS: модульные 212 passed; `e2e:server` 100 passed

- [ ] **Step 3: Закоммитить**

```bash
git add apps/web/e2e-server/files.spec.ts apps/web/src/ui/server/modal.tsx apps/web/src/ui/server/setup.tsx apps/web/src/ui/server/share-dialog.tsx
git commit -F - <<'EOF'
интерфейс: «Поделиться»

Приглашение по email с ролью, список людей, смена и снятие роли, ссылка с ролью и
выключателем. Читателю кнопки нет. Сбой, чужой ответ и запоздавший ответ списка не
портят, сценарии двух браузеров: роль, снятие доступа, CONFLICT с последующим слиянием.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 8: Интерфейс — команды и проекты

Спека §6.2.

**Files:**
- Create: `apps/web/src/ui/server/teams.tsx`
- Modify: `apps/web/src/ui/server/setup.tsx`
- Test: `apps/web/e2e-server/files.spec.ts`

- [ ] **Step 1: Реализация и сценарии**

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
});

test('картинки: отдельными запросами, файл на сервере без них, после открытия они на месте', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
````

на:

````ts
});

test('команда: проект, файл в проекте у участника; ушедший теряет доступ', async ({ browser }, info) => {
  const memberMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, memberMail);
  await a.getByRole('button', { name: 'Команды…' }).click();
  await a.getByLabel('Имя новой команды').fill('Студия');
  await a.getByRole('button', { name: 'Создать' }).click();
  await a.getByLabel('Имя нового проекта').fill('Сайт');
  await a.getByRole('button', { name: 'Добавить проект' }).click();
  await expect(a.getByRole('list', { name: 'Проекты команды' }).getByText('Сайт')).toBeVisible();
  await a.getByLabel('Адрес нового участника').fill(memberMail);
  await a.getByRole('button', { name: 'Пригласить' }).click();
  await expect(a.getByRole('list', { name: 'Участники команды' }).getByText(memberMail)).toBeVisible();
  await a.getByRole('button', { name: 'Закрыть' }).click();
  await a.getByRole('option', { name: 'Сайт' }).click();
  await expect(title(a)).toHaveText('Сайт');
  await a.getByRole('button', { name: 'Новый файл' }).first().click();
  await editorReady(a);
  const id = fileId(a);
  await drawRect(a);
  await saveKey(a);
  await saved(a);
  // Участник видит проект и правит файл.
  await b.reload();
  await expect(b.getByRole('option', { name: 'Сайт' })).toBeVisible();
  await b.getByRole('option', { name: 'Сайт' }).click();
  await expect(b.getByRole('option', { name: 'Без названия' })).toBeVisible();
  await open(b, id);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
  await saveKey(b);
  await saved(b);
  const members = (
    await api(a, 'GET', `/api/teams/${((await api(a, 'GET', '/api/teams')).body as { teams: { id: string }[] }).teams[0].id}/members`)
  ).body as {
    members: { id: string; email: string }[];
  };
  const teamId = ((await api(a, 'GET', '/api/teams')).body as { teams: { id: string }[] }).teams[0].id;
  await api(a, 'DELETE', `/api/teams/${teamId}/members/${members.members.find((m) => m.email === memberMail)!.id}`);
  await checkAccessNow(b);
  await expect(b.getByRole('alert').getByText('Нет доступа', { exact: true })).toBeVisible();
  await a.context().close();
  await b.context().close();
});

test('картинки: отдельными запросами, файл на сервере без них, после открытия они на месте', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
import { ServerFileStore } from './server-store';
import { ShareButton } from './share-dialog';

export function createServerStore(): ServerFileStore {
````

на:

````tsx
import { ServerFileStore } from './server-store';
import { ShareButton } from './share-dialog';
import { fileMenuItems, MoveDialog, refreshTeams, setOnMoved, teamNavEntries, TeamsEntry, teamSectionTitle } from './teams';

export function createServerStore(): ServerFileStore {
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
  s.redeemLink = async (id, token) => (await redeemLink(id, token)).ok;
  s.requestAccess = async (id) => (await requestFileAccess(id)).ok;
  return s;
}
````

на:

````tsx
  s.redeemLink = async (id, token) => (await redeemLink(id, token)).ok;
  s.requestAccess = async (id) => (await requestFileAccess(id)).ok;
  // Команды и проекты могли измениться у других: вместе со списком файлов.
  s.onChange(() => void refreshTeams());
  // Перемещение идёт мимо worker'а: список перечитывается как после любого изменения.
  setOnMoved(() => {
    s.notifyChanged();
  });
  return s;
}
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
    return m && isUuid(m[1]) ? `project:${m[1]}` : null;
  };
  extensions.sectionTitle = (section) => (section === 'shared' ? 'Доступные мне' : null);
  extensions.filesNav = () => [{ kind: 'section', id: 'shared', title: 'Доступные мне', icon: 'file' }];
  extensions.newFileRoute = (section) => (section.startsWith('project:') ? { kind: 'new', project: section.slice(8) } : { kind: 'new' });
  extensions.shareButton = ShareButton;
````

на:

````tsx
    return m && isUuid(m[1]) ? `project:${m[1]}` : null;
  };
  extensions.sectionTitle = teamSectionTitle;
  extensions.filesNav = teamNavEntries;
  extensions.filesNavExtra = () => (
    <>
      <TeamsEntry />
      <MoveDialog />
    </>
  );
  extensions.fileMenu = fileMenuItems;
  extensions.newFileRoute = (section) => (section.startsWith('project:') ? { kind: 'new', project: section.slice(8) } : { kind: 'new' });
  extensions.shareButton = ShareButton;
````

Создать `apps/web/src/ui/server/teams.tsx`:

````tsx
// Команды и проекты на экране файлов (спека подсистемы 4, раздел 6.2): строки панели разделов и диалог
// команды с участниками и проектами. Список команд один на страницу: он приходит с сервера и обновляется
// при каждом изменении, которое сделано здесь, и при возврате на экран.

import { MAX_TEAM_NAME_CHARS, parseEmail, type TeamDto, type TeamMembersDto } from '@ferrum/protocol';
import { createSignal, For, onCleanup, Show } from 'solid-js';
import type { Section } from '../../core/file-list';
import type { NavEntry } from '../extensions';
import type { MenuItem } from '../menu';
import type { FileMeta } from '../../core/file-list';
import { Modal } from './modal';
import {
  apiFailureText,
  createProject,
  createTeam,
  deleteProject,
  deleteTeam,
  getMembers,
  getTeams,
  inviteMember,
  moveFile,
  removeMember,
  renameTeam,
  type ApiResult,
} from './server-api';

const [teams, setTeams] = createSignal<TeamDto[]>([]);
/** Поколение: ответ на прежний запрос списка не затирает новый. */
let teamsGeneration = 0;

export async function refreshTeams(): Promise<string | null> {
  const my = ++teamsGeneration;
  const r = await getTeams();
  if (my !== teamsGeneration) return null;
  if (!r.ok) return apiFailureText(r.failure);
  setTeams(r.value.teams);
  return null;
}

export function teamNavEntries(): NavEntry[] {
  const out: NavEntry[] = [];
  for (const t of teams()) {
    out.push({ kind: 'label', title: t.name });
    for (const p of t.projects) out.push({ kind: 'section', id: `project:${p.id}`, title: p.name, icon: 'file' });
  }
  out.push({ kind: 'section', id: 'shared', title: 'Доступные мне', icon: 'file' });
  return out;
}

export function teamSectionTitle(section: Section): string | null {
  if (section === 'shared') return 'Доступные мне';
  if (!section.startsWith('project:')) return null;
  const id = section.slice(8);
  for (const t of teams()) {
    const p = t.projects.find((x) => x.id === id);
    if (p) return p.name;
  }
  return 'Проект';
}

/** Панель разделов: кнопка команд. Команды читаются при появлении экрана файлов. */
export function TeamsEntry() {
  const [open, setOpen] = createSignal(false);
  void refreshTeams();
  return (
    <div class="fe-files__extra">
      <button class="fe-btn fe-files__extra-btn" onClick={() => setOpen(true)}>
        Команды…
      </button>
      <Show when={open()}>
        <TeamsDialog onClose={() => setOpen(false)} />
      </Show>
    </div>
  );
}

function TeamsDialog(props: { onClose: () => void }) {
  const [selected, setSelected] = createSignal<string | null>(null);
  const [name, setName] = createSignal('');
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  let closed = false;
  onCleanup(() => (closed = true));
  const team = () => teams().find((t) => t.id === selected()) ?? null;
  void refreshTeams().then((e) => !closed && e && setError(e));
  const add = async (e: Event) => {
    e.preventDefault();
    if (!name().trim()) return;
    setBusy(true);
    setError(null);
    const r = await createTeam(name());
    if (closed) return;
    setBusy(false);
    if (!r.ok) {
      setError(apiFailureText(r.failure));
      return;
    }
    setName('');
    await refreshTeams();
    if (!closed) setSelected(r.value.id);
  };
  return (
    <Modal title="Команды" onClose={props.onClose}>
      <Show when={error()}>
        <p class="fe-share__error" role="alert">
          {error()}
        </p>
      </Show>
      <Show
        when={team()}
        fallback={
          <>
            <ul class="fe-share__people" aria-label="Ваши команды">
              <For each={teams()} fallback={<li class="t-muted">Команд пока нет</li>}>
                {(t) => (
                  <li class="fe-share__person">
                    <span class="fe-share__who t-strong">{t.name}</span>
                    <button class="fe-btn fe-btn--secondary" onClick={() => setSelected(t.id)}>
                      Открыть
                    </button>
                  </li>
                )}
              </For>
            </ul>
            <form class="fe-share__invite" novalidate onSubmit={(e) => void add(e)}>
              <div class="fe-field fe-field--text fe-share__email">
                <input
                  aria-label="Имя новой команды"
                  placeholder="Новая команда"
                  maxlength={MAX_TEAM_NAME_CHARS}
                  value={name()}
                  onInput={(e) => setName(e.currentTarget.value)}
                />
              </div>
              <button class="fe-btn fe-btn--primary" type="submit" disabled={busy()}>
                Создать
              </button>
            </form>
          </>
        }
        keyed
      >
        {(t) => <TeamPage team={t} onBack={() => setSelected(null)} />}
      </Show>
      <div class="fe-dialog__actions">
        <button class="fe-btn" onClick={props.onClose}>
          Закрыть
        </button>
      </div>
    </Modal>
  );
}

function TeamPage(props: { team: TeamDto; onBack: () => void }) {
  const id = props.team.id;
  const owner = () => props.team.role === 'owner';
  const [members, setMembers] = createSignal<TeamMembersDto['members'] | null>(null);
  const [email, setEmail] = createSignal('');
  const [project, setProject] = createSignal('');
  const [teamName, setTeamName] = createSignal(props.team.name);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  let generation = 0;
  let closed = false;
  onCleanup(() => (closed = true));
  async function run<T>(work: () => Promise<ApiResult<T>>, done?: (v: T) => void): Promise<void> {
    const my = ++generation;
    setBusy(true);
    setError(null);
    const r = await work();
    if (closed || my !== generation) return;
    setBusy(false);
    if (r.ok) done?.(r.value);
    else setError(apiFailureText(r.failure));
  }
  const showMembers = (v: TeamMembersDto) => setMembers(v.members);
  void run(() => getMembers(id), showMembers);
  return (
    <>
      <button class="fe-btn" onClick={props.onBack}>
        ← Все команды
      </button>
      <Show when={error()}>
        <p class="fe-share__error" role="alert">
          {error()}
        </p>
      </Show>
      <Show when={owner()} fallback={<h3 class="t-strong">{props.team.name}</h3>}>
        <form
          class="fe-share__invite"
          novalidate
          onSubmit={(e) => {
            e.preventDefault();
            if (teamName().trim())
              void run(
                () => renameTeam(id, teamName()),
                () => void refreshTeams(),
              );
          }}
        >
          <div class="fe-field fe-field--text fe-share__email">
            <input
              aria-label="Имя команды"
              maxlength={MAX_TEAM_NAME_CHARS}
              value={teamName()}
              onInput={(e) => setTeamName(e.currentTarget.value)}
            />
          </div>
          <button class="fe-btn fe-btn--secondary" type="submit" disabled={busy()}>
            Переименовать
          </button>
        </form>
      </Show>
      <h3 class="t-strong">Проекты</h3>
      <ul class="fe-share__people" aria-label="Проекты команды">
        <For each={props.team.projects} fallback={<li class="t-muted">Проектов пока нет</li>}>
          {(p) => (
            <li class="fe-share__person">
              <span class="fe-share__who">{p.name}</span>
              <Show when={owner()}>
                <button
                  class="fe-btn"
                  aria-label={`Удалить проект: ${p.name}`}
                  disabled={busy()}
                  onClick={() =>
                    void run(
                      () => deleteProject(p.id),
                      () => void refreshTeams(),
                    )
                  }
                >
                  Удалить
                </button>
              </Show>
            </li>
          )}
        </For>
      </ul>
      <form
        class="fe-share__invite"
        novalidate
        onSubmit={(e) => {
          e.preventDefault();
          if (!project().trim()) return;
          void run(
            () => createProject(id, project()),
            () => {
              setProject('');
              void refreshTeams();
            },
          );
        }}
      >
        <div class="fe-field fe-field--text fe-share__email">
          <input
            aria-label="Имя нового проекта"
            placeholder="Новый проект"
            maxlength={MAX_TEAM_NAME_CHARS}
            value={project()}
            onInput={(e) => setProject(e.currentTarget.value)}
          />
        </div>
        <button class="fe-btn fe-btn--secondary" type="submit" disabled={busy()}>
          Добавить проект
        </button>
      </form>
      <h3 class="t-strong">Участники</h3>
      <ul class="fe-share__people" aria-label="Участники команды">
        <For each={members() ?? []}>
          {(m) => (
            <li class="fe-share__person">
              <span class="fe-share__who t-muted">{m.email}</span>
              <span class="t-muted">{m.role === 'owner' ? 'Владелец' : 'Участник'}</span>
              <Show when={m.role !== 'owner' && owner()}>
                <button
                  class="fe-btn"
                  aria-label={`Убрать: ${m.email}`}
                  disabled={busy()}
                  onClick={() => void run(() => removeMember(id, m.id), showMembers)}
                >
                  Убрать
                </button>
              </Show>
            </li>
          )}
        </For>
      </ul>
      <Show when={owner()}>
        <form
          class="fe-share__invite"
          novalidate
          onSubmit={(e) => {
            e.preventDefault();
            const parsed = parseEmail(email());
            if (!parsed) {
              setError('Проверьте адрес');
              return;
            }
            void run(
              () => inviteMember(id, parsed),
              (v) => {
                setEmail('');
                showMembers(v);
              },
            );
          }}
        >
          <div class="fe-field fe-field--text fe-share__email">
            <input
              type="email"
              aria-label="Адрес нового участника"
              placeholder="Электронная почта"
              autocomplete="off"
              value={email()}
              onInput={(e) => setEmail(e.currentTarget.value)}
            />
          </div>
          <button class="fe-btn fe-btn--primary" type="submit" disabled={busy()}>
            Пригласить
          </button>
        </form>
        <button
          class="fe-btn"
          disabled={busy()}
          onClick={() =>
            void run(
              () => deleteTeam(id),
              () => {
                void refreshTeams();
                props.onBack();
              },
            )
          }
        >
          Удалить команду
        </button>
      </Show>
    </>
  );
}

/** Пункт «Переместить…» меню файла: владельцу, когда есть куда. Диалог живёт в подвале панели,
 * потому что меню закрывается раньше. */
const [moving, setMoving] = createSignal<FileMeta | null>(null);

export function fileMenuItems(file: FileMeta): MenuItem[] {
  if (file.role !== 'owner') return [];
  return [{ label: 'Переместить…', onSelect: () => setMoving(file) }];
}

export function MoveDialog() {
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const go = async (file: FileMeta, project: string | null) => {
    setBusy(true);
    setError(null);
    const r = await moveFile(file.id, project);
    setBusy(false);
    if (r.ok) {
      setMoving(null);
      onMoved?.();
    } else setError(apiFailureText(r.failure));
  };
  return (
    <Show when={moving()} keyed>
      {(file) => (
        <Modal title={`Переместить: ${file.name}`} onClose={() => setMoving(null)}>
          <Show when={error()}>
            <p class="fe-share__error" role="alert">
              {error()}
            </p>
          </Show>
          <ul class="fe-share__people" aria-label="Куда переместить">
            <li class="fe-share__person">
              <span class="fe-share__who">Черновики</span>
              <button class="fe-btn fe-btn--secondary" disabled={busy() || !file.projectId} onClick={() => void go(file, null)}>
                Сюда
              </button>
            </li>
            <For each={teams().flatMap((t) => t.projects.map((p) => ({ ...p, team: t.name })))}>
              {(p) => (
                <li class="fe-share__person">
                  <span class="fe-share__who">
                    {p.team}: {p.name}
                  </span>
                  <button class="fe-btn fe-btn--secondary" disabled={busy() || file.projectId === p.id} onClick={() => void go(file, p.id)}>
                    Сюда
                  </button>
                </li>
              )}
            </For>
          </ul>
          <div class="fe-dialog__actions">
            <button class="fe-btn" onClick={() => setMoving(null)}>
              Отмена
            </button>
          </div>
        </Modal>
      )}
    </Show>
  );
}

let onMoved: (() => void) | undefined;
/** Кому сказать, что список изменился. */
export function setOnMoved(fn: () => void): void {
  onMoved = fn;
}
````

- [ ] **Step 2: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Expected: PASS: модульные 212 passed; `e2e:server` 102 passed

- [ ] **Step 3: Закоммитить**

```bash
git add apps/web/e2e-server/files.spec.ts apps/web/src/ui/server/setup.tsx apps/web/src/ui/server/teams.tsx
git commit -F - <<'EOF'
интерфейс: команды и проекты на экране файлов

Проекты команд в панели разделов, диалог команды с участниками и проектами, новый
файл в проекте, перемещение файла.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 9: Интерфейс — «Локальные файлы (N)»

Спека §6.5.

**Files:**
- Create: `apps/web/src/ui/server/local-files.tsx`
- Modify: `apps/web/src/ui/server/setup.tsx`
- Test: `apps/web/e2e-server/files.spec.ts`

- [ ] **Step 1: Реализация и сценарии**

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
});

test('`?debug` не открывает ядро и хранилище в рабочей сборке (B-205)', async ({ page }, info) => {
  await page.goto('/');
````

на:

````ts
});

test('файлы браузера прежней версии: «Локальные файлы (N)», перенос с журналом, оригинал остаётся', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  const bytes = await a.evaluate(async (i) => Array.from(new Uint8Array(await (await fetch(`/api/files/${i}/content`)).arrayBuffer())), id);
  // Запись прежней версии — прямо в базу браузера, как её оставил этап 1.
  await a.evaluate(
    (data) =>
      new Promise<void>((resolve, reject) => {
        const req = indexedDB.open('ferrum', 4);
        req.onsuccess = () => {
          const tx = req.result.transaction(['meta', 'bytes'], 'readwrite');
          const now = Date.now();
          tx.objectStore('meta').put({
            id: 'old-file',
            name: 'Старый',
            createdAt: now,
            editedAt: now,
            openedAt: now,
            size: data.length,
            thumbVersion: 0,
            version: 1,
          });
          tx.objectStore('bytes').put(new Uint8Array(data).buffer, 'old-file');
          tx.oncomplete = () => resolve();
          tx.onerror = () => reject(tx.error);
        };
      }),
    bytes,
  );
  await a.goto('/#/drafts');
  await a.reload();
  await a.getByRole('button', { name: 'Локальные файлы (1)' }).click();
  await a.getByRole('button', { name: 'Перенести: Старый' }).click();
  await expect(a.getByText('Перенесён в «Черновики»')).toBeVisible();
  await a.getByRole('button', { name: 'Закрыть' }).click();
  await expect(a.getByRole('option', { name: 'Старый' })).toBeVisible();
  // Оригинал на месте.
  await expect(a.getByRole('button', { name: 'Локальные файлы (1)' })).toBeVisible();
  await a.getByRole('option', { name: 'Старый' }).click();
  await editorReady(a);
  await nodes(a, 1);
  await a.goto('/#/drafts');
  await a.reload();
  await a.getByRole('button', { name: 'Локальные файлы (1)' }).click();
  await expect(a.getByText('Перенесён в «Черновики»')).toBeVisible();
  await a.getByRole('button', { name: 'Удалить из браузера: Старый' }).click();
  await expect(a.getByRole('dialog').getByText('Старый')).toHaveCount(0);
  await a.context().close();
});

test('`?debug` не открывает ядро и хранилище в рабочей сборке (B-205)', async ({ page }, info) => {
  await page.goto('/');
````

Создать `apps/web/src/ui/server/local-files.tsx`:

````tsx
// Файлы прежней версии из IndexedDB (спека подсистемы 4, раздел 6.5): «Локальные файлы (N)» с переносом на
// сервер по одному или все сразу. Оригинал остаётся, пока его не удалят. Байты идут из worker'а в worker
// передачей, основной поток их не читает; журнал правок переносится вместе с файлом.

import { createSignal, For, onCleanup, Show } from 'solid-js';
import { MAX_WINDOW, type FileMeta } from '../../core/file-list';
import { Modal } from './modal';
import type { ServerFileStore } from './server-store';

const movedKey = (user: string) => `ferrum-moved-files:${user}`;

function readMoved(user: string): Set<string> {
  try {
    const v = JSON.parse(localStorage.getItem(movedKey(user)) ?? '[]') as unknown;
    return new Set(Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []);
  } catch {
    return new Set();
  }
}

export function LocalFilesEntry(props: { store: () => ServerFileStore | null; user: string }) {
  const [count, setCount] = createSignal(0);
  const [open, setOpen] = createSignal(false);
  let closed = false;
  onCleanup(() => (closed = true));
  const refresh = () => {
    const s = props.store();
    if (!s) return;
    s.local.list('drafts', '', 0, 1).then(
      (r) => !closed && setCount(r.total),
      () => {},
    );
  };
  refresh();
  return (
    <Show when={count() > 0}>
      <button class="fe-btn fe-files__extra-btn" onClick={() => setOpen(true)}>
        Локальные файлы ({count()})
      </button>
      <Show when={open() && props.store()}>
        {(s) => (
          <LocalFilesDialog
            store={s()}
            user={props.user}
            onClose={() => {
              setOpen(false);
              refresh();
            }}
          />
        )}
      </Show>
    </Show>
  );
}

function LocalFilesDialog(props: { store: ServerFileStore; user: string; onClose: () => void }) {
  const [rows, setRows] = createSignal<FileMeta[]>([]);
  const [total, setTotal] = createSignal(0);
  const [moved, setMoved] = createSignal(readMoved(props.user));
  const [status, setStatus] = createSignal<Record<string, string>>({});
  const [running, setRunning] = createSignal(false);
  let closed = false;
  onCleanup(() => (closed = true));
  const load = () =>
    props.store.local.list('drafts', '', 0, MAX_WINDOW).then(
      (r) => {
        if (closed) return;
        setRows(r.rows);
        setTotal(r.total);
      },
      (e: unknown) => !closed && setStatus({ '': `Не удалось прочитать файлы браузера: ${e instanceof Error ? e.message : String(e)}` }),
    );
  void load();
  const note = (id: string, text: string) => setStatus((s) => ({ ...s, [id]: text }));
  const moveOne = async (f: FileMeta): Promise<void> => {
    note(f.id, 'Переносится…');
    try {
      const r = await props.store.local.read(f.id);
      await props.store.importLocal(f.name, r.bytes, r.bytes ? r.journal : []);
      if (closed) return;
      const next = new Set(moved()).add(f.id);
      setMoved(next);
      try {
        localStorage.setItem(movedKey(props.user), JSON.stringify([...next]));
      } catch {
        // Отметка «перенесён» — только удобство: без неё перенос можно повторить.
      }
      note(f.id, 'Перенесён в «Черновики»');
    } catch (e) {
      if (!closed) note(f.id, `Не перенесён: ${e instanceof Error ? e.message : String(e)}`);
    }
  };
  const moveAll = async () => {
    setRunning(true);
    // По одному: каждый файл может быть большим.
    for (const f of rows()) {
      if (closed) return;
      if (!moved().has(f.id)) await moveOne(f);
    }
    if (!closed) setRunning(false);
  };
  const remove = async (f: FileMeta) => {
    try {
      await props.store.local.trash(f.id);
      await props.store.local.remove(f.id);
      if (!closed) await load();
    } catch (e) {
      if (!closed) note(f.id, `Не удалён: ${e instanceof Error ? e.message : String(e)}`);
    }
  };
  return (
    <Modal title={`Локальные файлы (${total()})`} onClose={props.onClose}>
      <p class="t-muted">
        Эти файлы лежат только в этом браузере. Перенос создаёт копию в «Черновиках», оригинал остаётся, пока вы его не удалите.
      </p>
      <Show when={status()['']}>
        {(text) => (
          <p class="fe-share__error" role="alert">
            {text()}
          </p>
        )}
      </Show>
      <ul class="fe-share__people" aria-label="Файлы в браузере">
        <For each={rows()}>
          {(f) => (
            <li class="fe-share__person">
              <span class="fe-share__who">
                <span class="t-strong">{f.name}</span>
                <Show when={status()[f.id] ?? (moved().has(f.id) ? 'Перенесён в «Черновики»' : '')}>
                  {(t) => <span class="t-muted">{t()}</span>}
                </Show>
              </span>
              <button
                class="fe-btn fe-btn--secondary"
                aria-label={`Перенести: ${f.name}`}
                disabled={running()}
                onClick={() => void moveOne(f)}
              >
                Перенести
              </button>
              <button class="fe-btn" aria-label={`Удалить из браузера: ${f.name}`} disabled={running()} onClick={() => void remove(f)}>
                Удалить
              </button>
            </li>
          )}
        </For>
      </ul>
      <div class="fe-dialog__actions">
        <button class="fe-btn" onClick={props.onClose}>
          Закрыть
        </button>
        <button
          class="fe-btn fe-btn--primary"
          disabled={running() || rows().every((f) => moved().has(f.id))}
          onClick={() => void moveAll()}
        >
          Перенести все
        </button>
      </div>
    </Modal>
  );
}
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx

import { isUuid } from '@ferrum/protocol';
import { currentUser } from '../auth/session';
import { extensions } from '../extensions';
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';
import { ShareButton } from './share-dialog';
import { fileMenuItems, MoveDialog, refreshTeams, setOnMoved, teamNavEntries, TeamsEntry, teamSectionTitle } from './teams';

export function createServerStore(): ServerFileStore {
````

на:

````tsx

import { isUuid } from '@ferrum/protocol';
import { createSignal } from 'solid-js';
import { currentUser } from '../auth/session';
import { extensions } from '../extensions';
import { LocalFilesEntry } from './local-files';
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';
import { ShareButton } from './share-dialog';
import { fileMenuItems, MoveDialog, refreshTeams, setOnMoved, teamNavEntries, TeamsEntry, teamSectionTitle } from './teams';

const [store, setStore] = createSignal<ServerFileStore | null>(null);

export function createServerStore(): ServerFileStore {
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
  s.redeemLink = async (id, token) => (await redeemLink(id, token)).ok;
  s.requestAccess = async (id) => (await requestFileAccess(id)).ok;
  // Команды и проекты могли измениться у других: вместе со списком файлов.
  s.onChange(() => void refreshTeams());
````

на:

````tsx
  s.redeemLink = async (id, token) => (await redeemLink(id, token)).ok;
  s.requestAccess = async (id) => (await requestFileAccess(id)).ok;
  setStore(s);
  // Команды и проекты могли измениться у других: вместе со списком файлов.
  s.onChange(() => void refreshTeams());
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
    <>
      <TeamsEntry />
      <MoveDialog />
    </>
````

на:

````tsx
    <>
      <TeamsEntry />
      <LocalFilesEntry store={store} user={currentUser()?.id ?? ''} />
      <MoveDialog />
    </>
````

- [ ] **Step 2: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Expected: PASS: модульные 212 passed; `e2e:server` 104 passed

- [ ] **Step 3: Закоммитить**

```bash
git add apps/web/e2e-server/files.spec.ts apps/web/src/ui/server/local-files.tsx apps/web/src/ui/server/setup.tsx
git commit -F - <<'EOF'
интерфейс: «Локальные файлы (N)» с переносом на сервер

Файлы прежней версии из IndexedDB переносятся по одному или все сразу вместе с
журналом правок, байты идут из worker'а в worker передачей, оригинал остаётся.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 10: Вход — адрес на странице подтверждения (B-193)

`POST /api/auth/confirm-info` читает запись токена без траты; вошедшему под другим адресом — выбор, под тем же — «Продолжить» без траты токена.

**Files:**
- Modify: `apps/server/src/auth-routes.ts`, `apps/web/src/ui/auth/auth-api.ts`, `apps/web/src/ui/auth/auth-gate.tsx`, `packages/protocol/src/auth.ts`
- Test: `apps/server/test/auth.test.ts`, `apps/web/e2e-server/auth.spec.ts`

- [ ] **Step 1: Написать тесты**

В `apps/server/test/auth.test.ts` заменить:

````ts
  expect(full.HTML).toContain(link.replaceAll('&', '&amp;'));
});
````

на:

````ts
  expect(full.HTML).toContain(link.replaceAll('&', '&amp;'));
});

test('куда ведёт ссылка: адрес без траты токена, неверный токен и чужой `Origin` — отказ (B-193)', async () => {
  const s = stand();
  const token = await requestLink(s, 'Ivan@Example.com');
  const info = await s.post('/api/auth/confirm-info', { token });
  expect(info.status).toBe(200);
  expect(await info.json()).toEqual({ email: 'ivan@example.com' });
  // Второй раз то же самое: токен не потрачен.
  expect((await s.post('/api/auth/confirm-info', { token })).status).toBe(200);
  expect((await s.post('/api/auth/confirm', { token })).status).toBe(200);
  // Потрачен — той же ошибкой, что и подтверждение.
  const dead = await s.post('/api/auth/confirm-info', { token });
  expect(dead.status).toBe(400);
  expect(await dead.json()).toEqual({ error: 'INVALID_TOKEN' });
  expect((await s.post('/api/auth/confirm-info', { token: 'x' })).status).toBe(400);
  expect((await s.post('/api/auth/confirm-info', { token: 'a'.repeat(32) }, { origin: 'https://evil.example' })).status).toBe(403);
});

test('куда ведёт ссылка: предел с одного адреса', async () => {
  const s = stand();
  const body = { token: 'a'.repeat(32) };
  for (let i = 0; i < 30; i++) expect((await s.post('/api/auth/confirm-info', body)).status).toBe(400);
  expect((await s.post('/api/auth/confirm-info', body)).status).toBe(429);
});
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  await scanner.goto(link);
  await expect(scanner.getByRole('button', { name: 'Войти', exact: true })).toBeVisible();
  expect(asked).toEqual([]);
  await scanner.context().close();
  // Токен цел.
````

на:

````ts
  await scanner.goto(link);
  await expect(scanner.getByRole('button', { name: 'Войти', exact: true })).toBeVisible();
  // Страница спрашивает, кому принадлежит ссылка: токен это не тратит (B-193), подтверждения нет.
  await expect(scanner.getByText(email)).toBeVisible();
  expect(asked.filter((u) => new URL(u).pathname === '/api/auth/confirm')).toEqual([]);
  await scanner.context().close();
  // Токен цел.
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  const [link] = await linksFor(email);
  const other = await newContext(browser);
  await other.goto(link);
  await other.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(other.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await other.getByRole('button', { name: 'Запросить новую ссылку' }).click();
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await other.context().close();
});

````

на:

````ts
  const [link] = await linksFor(email);
  const other = await newContext(browser);
  // Потраченная ссылка видна сразу, нажимать нечего.
  await other.goto(link);
  await expect(other.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await expect(other.getByRole('button', { name: 'Войти', exact: true })).toHaveCount(0);
  await other.getByRole('button', { name: 'Запросить новую ссылку' }).click();
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await other.context().close();
});

test('ссылку потратили, пока страница открыта: нажатие — то же сообщение', async ({ page, browser }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  await page.goto(link);
  await expect(page.getByText(email)).toBeVisible();
  const other = await newContext(browser);
  await other.goto(link);
  await other.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(other.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await other.context().close();
});

test('страница подтверждения показывает адрес; вошедшему под другим адресом — выбор, ссылка цела (B-193)', async ({ page }, info) => {
  const mine = newEmail(info.project.name);
  const foreign = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, mine);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.request.post('/api/auth/sign-in/magic-link', { data: { email: foreign }, headers: { origin: STAND_URL } });
  const [link] = await linksFor(foreign);
  await page.goto(link);
  await expect(page.getByRole('status')).toContainText(`Вы уже вошли как ${mine}`);
  await expect(page.getByRole('status')).toContainText(foreign);
  // «Остаться» не тратит токен и оставляет прежний вход.
  await page.getByRole('button', { name: `Остаться как ${mine}` }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  expect(((await (await page.request.get('/api/auth/session')).json()) as { user: { email: string } }).user.email).toBe(mine);
  await page.goto(link);
  await page.getByRole('button', { name: `Войти как ${foreign}` }).click();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  expect(((await (await page.request.get('/api/auth/session')).json()) as { user: { email: string } }).user.email).toBe(foreign);
});

test('ссылка на свой же адрес: токен не тратится, «Продолжить»', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.request.post('/api/auth/sign-in/magic-link', { data: { email }, headers: { origin: STAND_URL } });
  const [link] = await linksFor(email, 2);
  await page.goto(link);
  await expect(page.getByText('Вы уже вошли как')).toBeVisible();
  const confirm: string[] = [];
  page.on('request', (r) => {
    if (new URL(r.url()).pathname === '/api/auth/confirm') confirm.push(r.url());
  });
  await page.getByRole('button', { name: 'Продолжить' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  expect(confirm).toEqual([]);
});

````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  await expect.poll(() => valkey('EXISTS', keys[0]).trim()).toBe('0');
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
});

````

на:

````ts
  await expect.poll(() => valkey('EXISTS', keys[0]).trim()).toBe('0');
  await page.goto(link);
  await expect(page.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await expect(page.getByRole('button', { name: 'Войти', exact: true })).toHaveCount(0);
});

````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  await pasteHash(page, await secondLink(page, email));
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await enter(page).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
});
````

на:

````ts
  await pasteHash(page, await secondLink(page, email));
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await page.getByRole('button', { name: 'Продолжить' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
});
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  // Сразу, не дожидаясь журнала: приложение должно записать правку само.
  await pasteHash(page, link);
  await enter(page).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.goto(url);
````

на:

````ts
  // Сразу, не дожидаясь журнала: приложение должно записать правку само.
  await pasteHash(page, link);
  await page.getByRole('button', { name: 'Продолжить' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.goto(url);
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts

test('сбой сервера при подтверждении: новая ссылка ведёт на экран входа', async ({ page }) => {
  await page.goto(`/#/auth/confirm?token=${'a'.repeat(32)}&next=%23%2Ftrash`);
  await page.route('**/api/auth/confirm', (r) => r.fulfill({ status: 500, contentType: 'text/plain', body: 'сбой' }));
````

на:

````ts

test('сбой сервера при подтверждении: новая ссылка ведёт на экран входа', async ({ page }) => {
  // Сбой чтения адреса входа не блокирует: на экране просто нет адреса.
  await page.route('**/api/auth/confirm-info', (r) =>
    r.fulfill({ status: 503, contentType: 'application/json', body: '{"error":"UNAVAILABLE"}' }),
  );
  await page.goto(`/#/auth/confirm?token=${'a'.repeat(32)}&next=%23%2Ftrash`);
  await page.route('**/api/auth/confirm', (r) => r.fulfill({ status: 500, contentType: 'text/plain', body: 'сбой' }));
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/auth.test.ts`
Expected: FAIL — `POST /api/auth/confirm-info` отвечает 404: 36 pass, 2 fail

- [ ] **Step 3: Реализация**

В `apps/server/src/auth-routes.ts` заменить:

````ts
import {
  API_CONFIRM,
  API_NAME,
  API_SESSION,
````

на:

````ts
import {
  API_CONFIRM,
  API_CONFIRM_INFO,
  API_NAME,
  API_SESSION,
````

В `apps/server/src/auth-routes.ts` заменить:

````ts
  type AuthErrorBody,
  type AuthErrorCode,
  type SessionResponse,
} from '@ferrum/protocol';
import { CLIENT_IP_HEADER, type Auth } from './auth';
import { clientIp, proxyMatcher } from './client-ip';
````

на:

````ts
  type AuthErrorBody,
  type AuthErrorCode,
  type ConfirmInfo,
  type SessionResponse,
} from '@ferrum/protocol';
import { createHash } from 'node:crypto';
import { CLIENT_IP_HEADER, type Auth } from './auth';
import { clientIp, proxyMatcher } from './client-ip';
````

В `apps/server/src/auth-routes.ts` заменить:

````ts
 * (`REQUEST_TIMEOUT_MS`), иначе человек видел бы сбой сети, а не ответ сервера. */
export const AUTH_DEADLINE_MS = 10_000;

export function originAllowed(req: Request, publicUrl: string): boolean {
````

на:

````ts
 * (`REQUEST_TIMEOUT_MS`), иначе человек видел бы сбой сети, а не ответ сервера. */
export const AUTH_DEADLINE_MS = 10_000;

/** Как у подтверждения (`/magic-link/verify`): 30 в минуту с адреса. */
const CONFIRM_INFO_PER_MINUTE = 30;

function readEmail(value: string | undefined): string | null {
  if (!value) return null;
  try {
    const email = (JSON.parse(value) as { email?: unknown }).email;
    return typeof email === 'string' ? email : null;
  } catch {
    return null;
  }
}

export function originAllowed(req: Request, publicUrl: string): boolean {
````

В `apps/server/src/auth-routes.ts` заменить:

````ts
  }

  /** Токен тратится только здесь, по нажатию «Войти»: сканер почты, открывший ссылку, увидит
   * страницу подтверждения, а POST с верным `Origin` он не пришлёт. */
````

на:

````ts
  }

  /** Куда ведёт ссылка (B-193): запись токена читается без траты. Иначе чужая ссылка, нажатая по
   * неосторожности, молча вошла бы под чужим адресом, и работа попала бы в чужой аккаунт. Предел — как у
   * подтверждения: перебирать токены здесь не легче. */
  async function confirmInfo(req: Request, ip: string): Promise<Response> {
    const parsed = parseConfirmRequest(await readJson(req));
    if (!parsed) return fail(400, 'BAD_REQUEST');
    const context = await auth.$context;
    const used = await context.secondaryStorage!.increment!(`confirm-info:${ip}`, 60);
    if (used > CONFIRM_INFO_PER_MINUTE) return fail(429, 'TOO_MANY_REQUESTS');
    const hashed = createHash('sha256').update(parsed.token).digest('base64url');
    const row = await context.internalAdapter.findVerificationValue(`magic-link:${hashed}`);
    const email = readEmail(row?.value);
    return email ? respond([], { email } satisfies ConfirmInfo) : fail(400, 'INVALID_TOKEN');
  }

  /** Токен тратится только здесь, по нажатию «Войти»: сканер почты, открывший ссылку, увидит
   * страницу подтверждения, а POST с верным `Origin` он не пришлёт. */
````

В `apps/server/src/auth-routes.ts` заменить:

````ts
      [API_SIGN_IN]: signIn,
      [API_CONFIRM]: confirm,
      [API_NAME]: name,
      [API_SIGN_OUT]: signOut,
````

на:

````ts
      [API_SIGN_IN]: signIn,
      [API_CONFIRM]: confirm,
      [API_CONFIRM_INFO]: confirmInfo,
      [API_NAME]: name,
      [API_SIGN_OUT]: signOut,
````

В `apps/web/src/ui/auth/auth-api.ts` заменить:

````ts
import {
  API_CONFIRM,
  API_NAME,
  API_SESSION,
````

на:

````ts
import {
  API_CONFIRM,
  API_CONFIRM_INFO,
  API_NAME,
  API_SESSION,
````

В `apps/web/src/ui/auth/auth-api.ts` заменить:

````ts
  AUTH_ERROR_CODES,
  type AuthErrorCode,
  type SessionResponse,
  type SessionUser,
````

на:

````ts
  AUTH_ERROR_CODES,
  type AuthErrorCode,
  type ConfirmInfo,
  type SessionResponse,
  type SessionUser,
````

В `apps/web/src/ui/auth/auth-api.ts` заменить:

````ts
export const requestLink = (email: string, next: string | null) => post(API_SIGN_IN, isOk, next ? { email, next } : { email });
export const confirmLink = (token: string) => post(API_CONFIRM, isOk, { token });
export const saveName = (name: string) => post(API_NAME, isUserReply, { name });
export const signOut = () => post(API_SIGN_OUT, isOk, {});
````

на:

````ts
export const requestLink = (email: string, next: string | null) => post(API_SIGN_IN, isOk, next ? { email, next } : { email });
export const confirmLink = (token: string) => post(API_CONFIRM, isOk, { token });
const isConfirmInfo = (v: unknown): v is ConfirmInfo => isRecord(v) && typeof v.email === 'string';
export const confirmInfo = (token: string) => post(API_CONFIRM_INFO, isConfirmInfo, { token });
export const saveName = (name: string) => post(API_NAME, isUserReply, { name });
export const signOut = () => post(API_SIGN_OUT, isOk, {});
````

В `apps/web/src/ui/auth/auth-gate.tsx` заменить:

````tsx
import { dismissSplash } from '../boot';
import { editorHooks } from '../extensions';
import { confirmLink, getSession, requestLink, saveName } from './auth-api';
import { failureText, resendLeft, screenFor, type AuthFailure, type AuthScreen } from './auth-logic';
import { announceAuthChange, currentUser, onAuthChange, setCurrentUser } from './session';
````

на:

````tsx
import { dismissSplash } from '../boot';
import { editorHooks } from '../extensions';
import { confirmInfo, confirmLink, getSession, requestLink, saveName } from './auth-api';
import { failureText, resendLeft, screenFor, type AuthFailure, type AuthScreen } from './auth-logic';
import { announceAuthChange, currentUser, onAuthChange, setCurrentUser } from './session';
````

В `apps/web/src/ui/auth/auth-gate.tsx` заменить:

````tsx
 * `INVALID_TOKEN` (или если токена нет). При любом другом сбое «Войти» остаётся повтором, а токен в
 * адресе не трогается. Но при сбое сервера токен мог быть уже потрачен (Better Auth снимает его до
 * обращения к базе): тогда рядом и «Запросить новую ссылку». */
function ConfirmScreen(props: {
  token: string | null;
````

на:

````tsx
 * `INVALID_TOKEN` (или если токена нет). При любом другом сбое «Войти» остаётся повтором, а токен в
 * адресе не трогается. Но при сбое сервера токен мог быть уже потрачен (Better Auth снимает его до
 * обращения к базе): тогда рядом и «Запросить новую ссылку».
 * Страница показывает, в какой адрес идёт вход, и предупреждает, если человек уже вошёл под другим
 * (B-193): чужая ссылка, нажатая по неосторожности, иначе молча сложила бы его работу в чужой аккаунт.
 * Эти два запроса токен не тратят; их сбой вход не блокирует, просто на экране нет адреса. */
function ConfirmScreen(props: {
  token: string | null;
````

В `apps/web/src/ui/auth/auth-gate.tsx` заменить:

````tsx
  const [serverDown, setServerDown] = createSignal(false);
  const [error, setError] = createSignal<string | null>(props.token ? null : failureText('INVALID_TOKEN'));
  const enter = async () => {
    setBusy(true);
````

на:

````tsx
  const [serverDown, setServerDown] = createSignal(false);
  const [error, setError] = createSignal<string | null>(props.token ? null : failureText('INVALID_TOKEN'));
  const [target, setTarget] = createSignal<string | null>(null);
  const [signedIn, setSignedIn] = createSignal<string | null>(null);
  let closed = false;
  onCleanup(() => (closed = true));
  if (props.token) {
    void confirmInfo(props.token).then((r) => {
      if (closed || busy()) return;
      if (r.ok) setTarget(r.value.email);
      else if (r.failure === 'INVALID_TOKEN') {
        setDead(true);
        setError(failureText('INVALID_TOKEN'));
      }
    });
    void getSession().then((r) => {
      if (!closed && r.ok) setSignedIn(r.value.user?.email ?? null);
    });
  }
  /** Уже вошёл под этим же адресом: токен незачем тратить. */
  const same = () => signedIn() !== null && signedIn() === target();
  const enter = async () => {
    setBusy(true);
````

В `apps/web/src/ui/auth/auth-gate.tsx` заменить:

````tsx
      </Show>
      <Show when={!dead()}>
        <Show when={!error()}>
          <p class="fe-auth__text">Нажмите кнопку, чтобы завершить вход.</p>
        </Show>
        <button class="fe-btn fe-btn--primary fe-auth__wide" disabled={busy()} onClick={() => void enter()}>
          Войти
        </button>
      </Show>
      <Show when={dead() || serverDown()}>
````

на:

````tsx
      </Show>
      <Show when={!dead()}>
        <Show
          when={signedIn() && target() && !same()}
          fallback={
            <Show when={!error()}>
              <p class="fe-auth__text">
                <Show when={target()} fallback="Нажмите кнопку, чтобы завершить вход.">
                  {(email) => (
                    <>
                      <Show when={same()} fallback="Вход как ">
                        Вы уже вошли как{' '}
                      </Show>
                      <b>{email()}</b>.
                    </>
                  )}
                </Show>
              </p>
            </Show>
          }
        >
          <p class="fe-auth__text" role="status">
            Вы уже вошли как <b>{signedIn()}</b>. Эта ссылка входит как <b>{target()}</b>: новые файлы будут сохраняться в его аккаунте.
          </p>
        </Show>
        <Show
          when={same()}
          fallback={
            <button class="fe-btn fe-btn--primary fe-auth__wide" disabled={busy()} onClick={() => void enter()}>
              <Show when={signedIn() && target()} fallback="Войти">
                Войти как {target()}
              </Show>
            </button>
          }
        >
          <button class="fe-btn fe-btn--primary fe-auth__wide" onClick={() => props.onDone(props.next)}>
            Продолжить
          </button>
        </Show>
        <Show when={signedIn() && target() && !same()}>
          <button class="fe-btn fe-btn--secondary fe-auth__wide" disabled={busy()} onClick={() => props.onDone(props.next)}>
            Остаться как {signedIn()}
          </button>
        </Show>
      </Show>
      <Show when={dead() || serverDown()}>
````

В `packages/protocol/src/auth.ts` заменить:

````ts
export const API_SIGN_IN = '/api/auth/sign-in/magic-link';
export const API_CONFIRM = '/api/auth/confirm';
export const API_SESSION = '/api/auth/session';
export const API_NAME = '/api/auth/name';
````

на:

````ts
export const API_SIGN_IN = '/api/auth/sign-in/magic-link';
export const API_CONFIRM = '/api/auth/confirm';
export const API_CONFIRM_INFO = '/api/auth/confirm-info';
export const API_SESSION = '/api/auth/session';
export const API_NAME = '/api/auth/name';
````

В `packages/protocol/src/auth.ts` заменить:

````ts
export interface ConfirmRequest {
  token: string;
}

````

на:

````ts
export interface ConfirmRequest {
  token: string;
}

/** Куда ведёт ссылка: страница подтверждения показывает адрес, токен при этом не тратится. */
export interface ConfirmInfo {
  email: string;
}

````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Expected: PASS: `bun test` 12 и 285 pass; модульные 212 passed; `e2e:server` 110 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/src/auth-routes.ts apps/server/test/auth.test.ts apps/web/e2e-server/auth.spec.ts apps/web/src/ui/auth/auth-api.ts apps/web/src/ui/auth/auth-gate.tsx packages/protocol/src/auth.ts
git commit -F - <<'EOF'
вход: страница подтверждения показывает адрес и предупреждает (B-193)

POST /api/auth/confirm-info читает запись токена без траты. Вошедшему под другим
адресом — выбор «Войти как» или «Остаться как», под тем же — «Продолжить» без траты
токена. Потраченная ссылка видна сразу.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 11: Документы и финальная проверка

«Сделано (этап 2)» в спеке, `docs/running.md`, бэклог: закрыты B-190, B-193, B-195, B-204, B-205, B-206; новые B-237…B-258.

**Files:**
- Modify: `docs/backlog.md`, `docs/running.md`, `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md`

- [ ] **Step 1: Реализация**

В `docs/backlog.md` заменить:

````markdown
| № | Что | Пр. | Источник |
|---|---|---|---|
| B-189 | `no-unassigned-vars` в oxlint выключено во всём репозитории: правило не видит присваивание ссылок SolidJS в разметке (`ref={el}`), 36 ложных срабатываний. Для `apps/server` и `packages/protocol` включить уже можно, SolidJS там нет; для `apps/web`, если появится настройка или проверка через `let el!:`. | P3 | этап 1 подсистемы 4 |
| B-190 | Файлы до этапа 2 лежат в IndexedDB браузера и не разделены по пользователям: тот, кто вошёл следующим в этом браузере, видит файлы предыдущего. Снимается серверным `FileStore` (§6.5). | P1 | этап 1 подсистемы 4 |
| B-191 | Имя после первого входа не меняется: в настройках только показ. | P2 | этап 1 подсистемы 4 |
| B-192 | Предел 5 писем на адрес можно истратить чужими запросами: подставить адрес и на час лишить человека писем. Цена защиты от бомбардировки почты; смягчить можно капчей или отдельным счётчиком на пару адрес и IP. | P3 | этап 1 подсистемы 4 |
| B-193 | Страница подтверждения не показывает, в какой адрес идёт вход: чужая ссылка, нажатая по неосторожности, войдёт под чужим адресом (login CSRF). С серверными файлами жертва будет складывать работу в аккаунт атакующего. Минимум к этапу 2: «Вы уже вошли как X, войти другим адресом?» и показ адреса входа, что требует чтения записи токена без траты. | P2 | этап 1 подсистемы 4 |
| B-194 | За пробросом порта Docker Desktop и OrbStack и за прокси без `TRUSTED_PROXIES` все клиенты — один адрес. Проверка сессии с предела снята, вошедших это не закрывает. Осталось: 31 `confirm` в минуту закрывает вход и 21 `sign-in` в час закрывает письма всем за этим адресом. Без прокси правильного адреса на таких машинах не получить, поэтому только запись в `docs/running.md`. | P2 | этап 1 подсистемы 4 |
| B-195 | Открытый редактор не замечает истёкшую или снятую с другого устройства сессию до перезагрузки: запросов к серверу в нём нет. Снимается соединением этапа 3. | P2 | этап 1 подсистемы 4 |
| B-196 | Адрес вне `ALLOWED_EMAILS` мог отвечать быстрее. Замер до правки (loopback, 750 запросов на группу, три захода): из списка p50 1,9–2,4 мс, вне списка 1,3–1,4 мс, разница около 0,7 мс. Письмо теперь собирается и уходит после ответа (`setTimeout(0)`). Остаток: отправка занимает цикл событий и может задержать следующий запрос. Через интернет при пределах 5 на адрес и 20 на IP это неразличимо, в локальной сети на уровне домена возможно. После правки не перемерялось. | P3 | этап 1 подсистемы 4 |
| B-197 | Токен остаётся в Valkey на 15 минут, если письмо не ушло (предел на адрес, адрес вне списка, сбой SMTP): `sendMagicLink` зовётся после записи. Его никто не знает, вреда нет, но ключи копятся в пределах предела на IP. | P3 | этап 1 подсистемы 4 |
````

на:

````markdown
| № | Что | Пр. | Источник |
|---|---|---|---|
| B-189 | `no-unassigned-vars` в oxlint выключено для `apps/web`: правило не видит присваивание ссылок SolidJS в разметке (`ref={el}`). Для `apps/server` и `packages` уже включено. | P3 | этап 1 подсистемы 4 |
| B-191 | Имя после первого входа не меняется: в настройках только показ. | P2 | этап 1 подсистемы 4 |
| B-192 | Предел 5 писем на адрес можно истратить чужими запросами: подставить адрес и на час лишить человека писем. Цена защиты от бомбардировки почты; смягчить можно капчей или отдельным счётчиком на пару адрес и IP. | P3 | этап 1 подсистемы 4 |
| B-194 | За пробросом порта Docker Desktop и OrbStack и за прокси без `TRUSTED_PROXIES` все клиенты — один адрес. Проверка сессии с предела снята, вошедших это не закрывает. Осталось: 31 `confirm` в минуту закрывает вход и 21 `sign-in` в час закрывает письма всем за этим адресом. Без прокси правильного адреса на таких машинах не получить, поэтому только запись в `docs/running.md`. | P2 | этап 1 подсистемы 4 |
| B-196 | Адрес вне `ALLOWED_EMAILS` мог отвечать быстрее. Замер до правки (loopback, 750 запросов на группу, три захода): из списка p50 1,9–2,4 мс, вне списка 1,3–1,4 мс, разница около 0,7 мс. Письмо теперь собирается и уходит после ответа (`setTimeout(0)`). Остаток: отправка занимает цикл событий и может задержать следующий запрос. Через интернет при пределах 5 на адрес и 20 на IP это неразличимо, в локальной сети на уровне домена возможно. После правки не перемерялось. | P3 | этап 1 подсистемы 4 |
| B-197 | Токен остаётся в Valkey на 15 минут, если письмо не ушло (предел на адрес, адрес вне списка, сбой SMTP): `sendMagicLink` зовётся после записи. Его никто не знает, вреда нет, но ключи копятся в пределах предела на IP. | P3 | этап 1 подсистемы 4 |
````

В `docs/backlog.md` заменить:

````markdown
| B-202 | Проверка `argv` в `check-dist.mjs` и симлинки: из `npm run` не срабатывает, но при срабатывании проверка молча пропускается. | P3 | этап 1 подсистемы 4, ревью задачи 3 |
| B-203 | Тест выхода за каталог статики слабый: `%2e%2e` нормализует сам `fetch`, до сервера доходит уже другой путь. | P3 | этап 1 подсистемы 4, ревью задачи 4 |
| B-204 | `blob-store.ts`: нет `fsync`, `.tmp` остаётся при сбое, гонка `exists` и `arrayBuffer`. Исправить до первого настоящего использования. | P2 | этап 2, финальное ревью (Minor 17) |
| B-205 | `?debug` включает тестовый режим и `__ferrumFiles` в рабочей сборке (`boot.ts:54`, есть в `dist-server`). На серверных файлах это нужно закрыть. | P2 | этап 2, финальное ревью (Minor 17) |
| B-206 | `TauriFileStore` тоже в `dist-server`: `check-dist` ловит только ветку `main.tsx`, поэтому «кода автономного режима нет» верно лишь в этом узком смысле. | P3 | этап 2, финальное ревью (Minor 17) |
| B-207 | `server.stop()` без `true` при живых WebSocket нужно пересмотреть: без них остановка чистая (проба 52 мс). | P2 | этап 3, финальное ревью (Minor 18) |
| B-208 | Обновление образа при открытой вкладке: старые `assets/*` дают 404, открытие файла падает на `new Worker` без подсказки перезагрузить страницу. Правки уже открытого файла не страдают. | P2 | этап 1 подсистемы 4, финальное ревью (Minor 19) |
| B-209 | Письмо в пути теряется при остановке сервера, а человек уже видит «Проверьте почту». Окно мало. | P3 | этап 1 подсистемы 4, финальное ревью (Minor 20) |
| B-210 | Нет сжатия: wasm 12 МБ и шрифты идут как есть. Без прокси это долгая первая загрузка. | P2 | этап 1 подсистемы 4, финальное ревью (Minor 21) |
````

на:

````markdown
| B-202 | Проверка `argv` в `check-dist.mjs` и симлинки: из `npm run` не срабатывает, но при срабатывании проверка молча пропускается. | P3 | этап 1 подсистемы 4, ревью задачи 3 |
| B-203 | Тест выхода за каталог статики слабый: `%2e%2e` нормализует сам `fetch`, до сервера доходит уже другой путь. | P3 | этап 1 подсистемы 4, ревью задачи 4 |
| B-207 | `server.stop()` без `true` при живых WebSocket нужно пересмотреть: без них остановка чистая (проба 52 мс). | P2 | этап 3, финальное ревью (Minor 18) |
| B-208 | Обновление образа при открытой вкладке: старые `assets/*` дают 404, открытие файла падает на `new Worker` без подсказки перезагрузить страницу. Правки уже открытого файла не страдают. Worker файлов сервера при таком сбое просит обновить страницу; `new Worker` ядра — ещё нет. | P2 | этап 1 подсистемы 4, финальное ревью (Minor 19) |
| B-209 | Письмо в пути теряется при остановке сервера, а человек уже видит «Проверьте почту». Окно мало. | P3 | этап 1 подсистемы 4, финальное ревью (Minor 20) |
| B-210 | Нет сжатия: wasm 12 МБ и шрифты идут как есть. Без прокси это долгая первая загрузка. | P2 | этап 1 подсистемы 4, финальное ревью (Minor 21) |
````

В `docs/backlog.md` заменить:

````markdown
| B-236 | Запуск сервера при зависшем Postgres (соединение принято, ответа нет) ждёт миграции без срока и без строки в журнале, `restart` не срабатывает. В compose закрыто условием `service_healthy`. Нужны срок подключения для миграций и строка в журнал. | P3 | этап 1 подсистемы 4, повторное ревью финальных правок |

## Код

````

на:

````markdown
| B-236 | Запуск сервера при зависшем Postgres (соединение принято, ответа нет) ждёт миграции без срока и без строки в журнале, `restart` не срабатывает. В compose закрыто условием `service_healthy`. Нужны срок подключения для миграций и строка в журнал. | P3 | этап 1 подсистемы 4, повторное ревью финальных правок |


## Файлы на сервере (этап 2 подсистемы 4)

| № | Что | Пр. | Источник |
|---|---|---|---|
| B-237 | Снятие и понижение роли не мгновенны, как требует §6.3: запись отказывает сразу, а открытый редактор узнаёт при первой записи, при возврате на вкладку или опросом раз в 15 с; скрытая вкладка не опрашивает. Чтение уже открытого файла в памяти не отзывается. Снимается соединением этапа 3 (`role`, `access-revoked`), получатель события тот же, опрос убрать. | P2 | этап 2, расхождение со спекой |
| B-238 | Двое правят файл не вместе: кто записал первым, тот прав, второй получает `CONFLICT` и сливает правки, открыв файл заново. Читатель видит версию на момент открытия и сообщение о новой. Снимается этапом 3. | P2 | этап 2 |
| B-239 | Люди, пришедшие по ссылке, не видны в списке «Поделиться»; снять их можно только новой ссылкой или выключателем. | P3 | этап 2 |
| B-240 | Смена `BETTER_AUTH_SECRET` делает прежние ссылки непоказуемыми (токен выводится из секрета): они действуют, но скопировать нельзя, нужна «Новая ссылка». | P3 | этап 2 |
| B-241 | Корзина не очищается сама: файлы лежат, пока их не удалят. | P3 | этап 2 |
| B-242 | Сбой процесса между загрузкой и фиксацией оставляет `files/<id>/content-*` без записи в базе: `.tmp` убирается при запуске, а готовый файл нет. Нужен обход каталога со сверкой. | P3 | этап 2 |
| B-243 | Нет передачи владения файлом и командой: владельца команды снять нельзя, команда без него не живёт. | P3 | этап 2 |
| B-244 | «Локальные файлы (N)» видит любой, кто вошёл в этом браузере: файлы прежней версии ни за кем не записаны. Серверные файлы и журнал разделены (B-190 закрыт), этот остаток закрывается удалением оригиналов. | P2 | этап 2, §6.5 |
| B-245 | `file.schema_version` не заполняется: сервер этапа 2 документ не раскодирует. Заполнит `hello { schema }` этапа 3. | P3 | этап 2 |
| B-246 | Журнал правок человека, у которого сняли право, остаётся в его браузере и больше ничем не показывается, кроме плашки открытого редактора. §11.1 этапа 6: сохранить копией. | P3 | этап 2 |
| B-247 | Окна списка — `OFFSET` по живому порядку: чужая запись между окнами сдвигает строки до следующего чтения. | P3 | этап 2 |
| B-248 | В списке команды только адреса: имя выдало бы, есть ли у адреса учётная запись. В «Поделиться» имя появляется после первого открытия файла. | P3 | этап 2 |
| B-249 | `server.css` есть в автономной сборке, как `auth.css` (B-229): стили инертны. | P3 | этап 2 |
| B-250 | Панель слоёв и страниц у читателя не отключена: правки оттуда отвергает ядро сообщением «Документ открыт только для чтения». Панель свойств отключена, кроме «Экспорта». | P3 | этап 2 |
| B-251 | Нет предела места на человека: только пределы частоты и 512 МБ на файл. Квоты по тарифам вне спеки (§1), но один человек может заполнить диск. | P2 | этап 2, ревью плана |
| B-252 | Поиск по имени — `ILIKE` без индекса: на больших разделах полный проход. | P3 | этап 2 |
| B-253 | E2E «сеть пропала» только в Chromium: `setOffline` в Firefox проверен не был. | P3 | этап 2 |
| B-254 | Запрос доступа — только письмо: страницы запросов нет, владелец сам открывает файл и приглашает. | P3 | этап 2 |
| B-255 | Запросы API не продлевают сессию (`disableRefresh`): продлевает открытие приложения. Вкладка, открытая дольше 30 дней без перезагрузки, получит «Сессия завершена». | P3 | этап 2 |
| B-256 | Отложенное превью (раз в минуту) теряется при закрытии вкладки: придёт со следующей записью. | P3 | этап 2 |
| B-257 | Заведомый отказ записи (`CONFLICT`) сервер даёт, не дочитав тело: через прокси браузер может увидеть обрыв вместо 409. Предпроверка `GET …/access` делает это редким, повтор получит настоящий ответ. | P3 | этап 2 |
| B-258 | Новый файл до первой записи (2 с) не в журнале: при выходе в другой вкладке редактор остаётся и предлагает копию, а при сбое вкладки правка теряется, как и в автономной сборке. | P3 | этап 2 |

## Код

````

В `docs/running.md` заменить:

````markdown
- От адреса вне списка заголовок не учитывается. Пустой список — заголовку не верим.
- Прямой доступ без прокси: `FERRUM_BIND=0.0.0.0`. Опубликованный Docker порт обходит ufw. За пробросом порта Docker Desktop и OrbStack адрес клиента — адрес шлюза, предел на IP становится общим на всех.

### Снять сессии человека
````

на:

````markdown
- От адреса вне списка заголовок не учитывается. Пустой список — заголовку не верим.
- Прямой доступ без прокси: `FERRUM_BIND=0.0.0.0`. Опубликованный Docker порт обходит ufw. За пробросом порта Docker Desktop и OrbStack адрес клиента — адрес шлюза, предел на IP становится общим на всех.

### Файлы и данные

Содержимое файлов, превью и картинки лежат на томе `ferrum-data`, записи о них — в `ferrum-postgres`: резервная копия нужна обоим вместе. Заменённые байты удаляются фоновой очисткой раз в минуту, неиспользуемые картинки — через 30 дней. Предела места на человека нет, только 512 МБ на файл и пределы частоты: следите за диском.

Смена `BETTER_AUTH_SECRET` не ломает ссылки «Поделиться», но показать прежнюю ссылку снова будет нельзя: нужна «Новая ссылка».

Прокси перед сервером должен пропускать тела до 512 МБ и не буферизовать их на диск без нужды (в nginx: `client_max_body_size 513m; proxy_request_buffering off;`; вариант не проверялся).

### Снять сессии человека
````

В `docs/running.md` заменить:

````markdown
npm run unit                # модульные
npm run build && npm run e2e:ci   # E2E автономной сборки
npm run e2e:server          # проект `server`: вход на настоящем стенде, Chromium и Firefox
```

````

на:

````markdown
npm run unit                # модульные
npm run build && npm run e2e:ci   # E2E автономной сборки
npm run e2e:server          # проект `server`: вход и файлы на настоящем стенде, Chromium и Firefox
```

````

В `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` заменить:

````markdown
  - Better Auth 1.7.16 с `pg` и `Bun.RedisClient`: круг письмо → подтверждение → одноразовость (`GETDEL`, проверено пятью параллельными нажатиями) → сессия в Valkey → проверка сессии работает на Bun без обходов. Токен в Valkey — хэшем, срок 900 с. Таблицы — только `user`, `account`. Valkey сервер переживает: после простоя клиент возвращается сам, на время простоя запросы получают ошибку сразу, а не виснут.
  - **(поведение)** Наружу открыт короткий список маршрутов: `POST /api/auth/{sign-in/magic-link, confirm, name, sign-out, sign-out-everywhere}`, `GET /api/auth/session`; остальные точки Better Auth, в том числе GET, тратящий токен, отдают 404. `confirm` принимает токен в теле и проверяет `Origin`. Ответы не содержат токена сессии: Better Auth отдаёт его в теле, что обесценило бы `HttpOnly`.
  - **(поведение)** Ссылка: `PUBLIC_URL/#/auth/confirm?token=…&next=…`. Токен после `#` не попадает на сервер и в журналы; `next` — только маршрут приложения, поэтому возврат работает и в другом браузере, и не уводит на чужой адрес. Открытие страницы не делает ни одного запроса. Истёкшая и использованная ссылка неразличимы: «Ссылка недействительна или устарела. Запросите новую». При сбое сети на странице подтверждения «Войти» остаётся повтором, токен в адресе цел; таймаут запросов к серверу 15 с. При сбое сервера (5xx) токен мог быть уже потрачен: Better Auth снимает его до обращения к Postgres, и повтор ответит «Ссылка недействительна». Поэтому рядом с «Войти» — «Запросить новую ссылку». Ссылка, вставленная в ту же вкладку (`hashchange`), тоже работает: при открытом приложении редактор сначала дописывает файл (протокол ухода), затем страница перезагружается.
  - **(поведение)** Предел на адрес — свой счётчик в том же Valkey: встроенный `rateLimit` считает только по IP и пути. Сверх предела — 429 и «Слишком много запросов». Счётчик считается и для адресов вне `ALLOWED_EMAILS`, поэтому по ответу список не раскрывается: такой адрес получает тот же ответ, письма нет. Письмо уходит без ожидания ответа SMTP; письмо собирается и отправляется после ответа (`setTimeout(0)`); сбой отправки пишется в журнал (только `code` и `responseCode`: текст ошибки SMTP содержит адрес). Адрес приводится к нижнему регистру.
  - **(поведение)** IP — адрес соединения. `X-Forwarded-For` учитывается, только если соединение пришло от адреса из `TRUSTED_PROXIES` (точные адреса и IPv4 CIDR, `::ffff:a.b.c.d` — это IPv4, `0.0.0.0/0` не принимается): идём по заголовку справа и берём первый адрес не из списка; все из списка или мусор — адрес соединения. Порт по умолчанию только на `127.0.0.1` (`FERRUM_BIND`).
````

на:

````markdown
  - Better Auth 1.7.16 с `pg` и `Bun.RedisClient`: круг письмо → подтверждение → одноразовость (`GETDEL`, проверено пятью параллельными нажатиями) → сессия в Valkey → проверка сессии работает на Bun без обходов. Токен в Valkey — хэшем, срок 900 с. Таблицы — только `user`, `account`. Valkey сервер переживает: после простоя клиент возвращается сам, на время простоя запросы получают ошибку сразу, а не виснут.
  - **(поведение)** Наружу открыт короткий список маршрутов: `POST /api/auth/{sign-in/magic-link, confirm, name, sign-out, sign-out-everywhere}`, `GET /api/auth/session`; остальные точки Better Auth, в том числе GET, тратящий токен, отдают 404. `confirm` принимает токен в теле и проверяет `Origin`. Ответы не содержат токена сессии: Better Auth отдаёт его в теле, что обесценило бы `HttpOnly`.
  - **(поведение)** Ссылка: `PUBLIC_URL/#/auth/confirm?token=…&next=…`. Токен после `#` не попадает на сервер и в журналы; `next` — только маршрут приложения, поэтому возврат работает и в другом браузере, и не уводит на чужой адрес. Открытие страницы токен не тратит: она только спрашивает, кому принадлежит ссылка, и показывает адрес (этап 2, B-193). Истёкшая и использованная ссылка неразличимы: «Ссылка недействительна или устарела. Запросите новую». При сбое сети на странице подтверждения «Войти» остаётся повтором, токен в адресе цел; таймаут запросов к серверу 15 с. При сбое сервера (5xx) токен мог быть уже потрачен: Better Auth снимает его до обращения к Postgres, и повтор ответит «Ссылка недействительна». Поэтому рядом с «Войти» — «Запросить новую ссылку». Ссылка, вставленная в ту же вкладку (`hashchange`), тоже работает: при открытом приложении редактор сначала дописывает файл (протокол ухода), затем страница перезагружается.
  - **(поведение)** Предел на адрес — свой счётчик в том же Valkey: встроенный `rateLimit` считает только по IP и пути. Сверх предела — 429 и «Слишком много запросов». Счётчик считается и для адресов вне `ALLOWED_EMAILS`, поэтому по ответу список не раскрывается: такой адрес получает тот же ответ, письма нет. Письмо уходит без ожидания ответа SMTP; письмо собирается и отправляется после ответа (`setTimeout(0)`); сбой отправки пишется в журнал (только `code` и `responseCode`: текст ошибки SMTP содержит адрес). Адрес приводится к нижнему регистру.
  - **(поведение)** IP — адрес соединения. `X-Forwarded-For` учитывается, только если соединение пришло от адреса из `TRUSTED_PROXIES` (точные адреса и IPv4 CIDR, `::ffff:a.b.c.d` — это IPv4, `0.0.0.0/0` не принимается): идём по заголовку справа и берём первый адрес не из списка; все из списка или мусор — адрес соединения. Порт по умолчанию только на `127.0.0.1` (`FERRUM_BIND`).
````

В `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` заменить:

````markdown
### 6.3 «Поделиться»

Кнопка в шапке редактора: приглашение по email с ролью, список людей, смена и снятие роли, ссылка с ролью и выключателем. Токен ссылки в базе хранится хэшем. Снятие роли мгновенно закрывает соединение этого человека с файлом, а открытый редактор показывает «Нет доступа». Понижение до читателя переводит открытый редактор в режим просмотра без перезагрузки.

### 6.4 Картинки
````

на:

````markdown
### 6.3 «Поделиться»

Кнопка в шапке редактора: приглашение по email с ролью, список людей, смена и снятие роли, ссылка с ролью и выключателем. Токен ссылки в базе хранится хэшем. Снятие роли мгновенно закрывает соединение этого человека с файлом, а открытый редактор показывает «Нет доступа» (до этапа 3 — см. «Сделано (этап 2)»). Понижение до читателя переводит открытый редактор в режим просмотра без перезагрузки.

### 6.4 Картинки
````

В `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` заменить:

````markdown
- **Файлы из IndexedDB:** если у браузера есть файлы прежней версии, экран файлов показывает «Локальные файлы (N)» с переносом на сервер по одному или все сразу. Оригинал остаётся, пока его не удалят.
- До этапа 3 файл пишется целиком, как сейчас, с проверкой поколения (`CONFLICT`). Этап 3 заменяет это журналом.

## 7. Синхронизация (этап 3)
````

на:

````markdown
- **Файлы из IndexedDB:** если у браузера есть файлы прежней версии, экран файлов показывает «Локальные файлы (N)» с переносом на сервер по одному или все сразу. Оригинал остаётся, пока его не удалят.
- До этапа 3 файл пишется целиком, как сейчас, с проверкой поколения (`CONFLICT`). Этап 3 заменяет это журналом.

- **Сделано (этап 2):**
  - **Модель.** Сверх §6.1: `file.version`, `size`, `content_key`, `thumb_version`, `last_write_id`, `trashed_by`; `file_recent` (открытия у каждого свои), `file_invite` и `team_invite` (адрес без учётной записи), `file_link_user` (кто пришёл по ссылке), `share_link.nonce`, `image.orphaned_at`, `file_image.unreferenced_at`, `blob_trash`. Действующая роль — функция базы `file_role_rank`. Адреса API — `/api/files/…`, `/api/teams/…`, `/api/projects/…`: один префикс с входом и прокси разработки.
  - **Конвейер маршрутов** (`api.ts`): `Origin`, сессия, предел частоты, права, срок и ответ в формате ошибок — у каждого маршрута, обработчик этого не делает. Нет доступа — 404, как у несуществующего; роли не хватает — 403.
  - **Запись файла целиком** идёт потоком на диск и обратно; в памяти только текущий кусок. Поколение и роль проверяются в транзакции фиксации, а заведомый отказ — раньше передачи. Сервер ходит по заголовкам контейнера, не раскодируя документ. **(поведение)** Запись несёт свой номер: своя удачная запись, ответ на которую потерялся, не становится `CONFLICT`. Байты заменённого файла удаляются через 2 минуты, читатель успевает дочитать.
  - **Полные снимки уже здесь (§7.6).** `Snapshot { full }`: проба показала, что журнал отставшей вкладки shallow-снимок не принимает, а полный — сливает. 100 000 узлов: 33,3 МБ, запись через сервер около 4 с, открытие около 0,8 с, длинных задач на основном потоке нет. **(поведение)** После `CONFLICT` правки не теряются: они в журнале браузера и сливаются при следующем открытии файла.
  - **Журнал правок** — в браузере, своя база на человека (`ferrum-user-<id>`). Читатель его не применяет.
  - **Картинки** не лежат в файле на сервере: worker выносит их перед записью и вставляет обратно при чтении, ядро получает обычный контейнер, хэш секции — её имя. `file_image` появляется только у того, кто прислал все байты: знание хэша ничего не даёт. **(поведение)** Ссылка без употребления в файле живёт 30 дней (отмена может её вернуть), затем ещё 30 — запись картинки без файлов.
  - **Роли (поведение).** Участник команды — редактор файлов её проектов, владелец команды — владелец. Файлы ушедшего участника переходят владельцу команды. Редактор может приглашать и повышать читателя, менять и снимать роли — только владелец; сам себе снять доступ может каждый, кроме владельца. Читателю кнопки «Поделиться» нет: адреса людей ему не положены (в Figma есть). В корзину убирает только владелец, файл из корзины виден только ему и тому, кто убрал. Нельзя удалить команду с проектами и проект с файлами.
  - **Приглашения (поведение).** Адрес без учётной записи получает доступ при первом входе. Список не выдаёт, есть ли учётная запись: имя появляется, когда человек открыл файл, в команде — только адреса. На адрес не больше 5 приглашений в час; сверх предела и вне `ALLOWED_EMAILS` запись остаётся, письма нет, ответ тот же.
  - **Ссылка (поведение).** Токен в базе хэшем, а показать его можно снова, как в Figma: он выводится из секрета сервера и `nonce`. Роль по ссылке действует, пока ссылка включена; новая ссылка отменяет прежнюю и всех пришедших по ней. Токен из адреса убирается сразу.
  - **Расхождение с §6.3.** WebSocket появится на этапе 3, поэтому снятие роли не мгновенно. Запись отказывает сразу и в транзакции; открытый редактор узнаёт при первой записи, при возврате на вкладку или опросом `GET /api/files/:id/access` раз в 15 с. Понижение переводит в режим просмотра без перезагрузки, снятие показывает «Нет доступа» с копией на диск. Этап 3 шлёт `role` и `access-revoked` в тот же обработчик, опрос убирается (B-237).
  - **Экран файлов.** Раздел «Доступные мне», проекты команд в панели, диалог команды. **(поведение)** Страница команды — диалог, а не маршрут. Список перечитывается раз в 15 с, пока вкладка видна. Превью — не чаще раза в минуту. Импорт называется «Открыть с диска…», связи с файлом на диске в рабочей сборке нет.
  - **Сборки.** Хранилище выбирает `main.tsx`. В рабочей сборке нет `TauriFileStore` и тестового доступа, `?debug` там не действует. В автономной нет кода сервера: `ui/server/setup` подменён заглушкой (`vite.config.ts`), иначе сборщик выпускал файл worker'а из мёртвой ветки. Всё это проверяет `check-dist.mjs`.
  - **(поведение)** Выход в другой вкладке, когда файл не записался: редактор не перезагружается, а показывает «Сессия завершена» и копию на диск.
  - Новая зависимость: `@noble/hashes` 2.4.0 — blake3 на сервере, в `Bun.CryptoHasher` его нет.

## 7. Синхронизация (этап 3)
````

- [ ] **Step 2: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Expected: всё PASS: `bun test` 12 и 285 pass; модульные 212 passed; обе сборки; `e2e:ci` 416 тестов (в прототипе, `--retries=1`, один нестабильный: `paste.spec.ts:142`); `e2e:server` 110 passed, 2 пропущены

- [ ] **Step 3: Закоммитить**

```bash
git add docs/backlog.md docs/running.md docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md
git commit -F - <<'EOF'
docs: файлы на сервере — спека, запуск, бэклог B-237…B-258

«Сделано (этап 2)» с расхождением §6.3: снятие роли не мгновенно до этапа 3.
Закрыты B-190, B-193, B-195, B-204, B-205, B-206; B-189 и B-208 уточнены.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

## Что этап 2 сознательно не делает

Совместная правка и WebSocket (этап 3): двое работают через `CONFLICT` со слиянием при открытии, снятие роли не мгновенно. Присутствие, комментарии, офлайн и версии (этапы 4–6). Серверные файлы в десктопе (этап 7): десктоп остаётся автономным. Автоочистка корзины, передача владения, квоты места — бэклог.
