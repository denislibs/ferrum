# Ferrum, подсистема 4, этап 1: сервер и вход — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `docker compose up` поднимает сервер на Bun, Postgres, Valkey и Mailpit. Без входа редактор недоступен: только экран входа с «Получить ссылку», письмо со ссылкой, страница подтверждения с кнопкой «Войти», экран имени при первом входе, возврат на запрошенный адрес, выход на этом устройстве и на всех. Весь репозиторий переходит на TypeScript 7, oxlint и prettier. Автономная сборка под `VITE_FERRUM_LOCAL=1` сохраняет прежний режим для существующих E2E, unit и десктопа. Файлы на этом этапе остаются в IndexedDB и в рабочей сборке (серверные файлы — этап 2).

**Architecture:** `apps/server` — `Bun.serve` с маршрутами, без фреймворка: `config.ts`, `migrate.ts` (нумерованный SQL под `pg_advisory_lock`), `blob-store.ts`, `static.ts` (раздача сборки с заголовками изоляции и CSP), `valkey.ts`, `secondary-storage.ts` (Better Auth на `Bun.RedisClient`, `GETDEL`, `INCR` с `EXPIRE` скриптом), `auth.ts` (Better Auth, `magicLink` хэшем, срок 15 минут, предел на адрес), `auth-routes.ts` (короткий список открытых маршрутов, `Origin`, разбор из протокола, `authorizeUpgrade`), `mail.ts`, `app.ts`, `index.ts`. `packages/protocol` — типы и разбор без зависимостей, импортируется и сервером, и `apps/web` (`file:`). `apps/web`: `src/ui/auth/` (`auth-logic.ts`, `auth-api.ts`, `session.ts`, `auth-gate.tsx`, `account.tsx`), `styles/auth.css`, `extensions.ts`; ветка в `main.tsx` по `import.meta.env.VITE_FERRUM_LOCAL` подключает код входа только в рабочую сборку, `scripts/check-dist.mjs` проверяет обе. Образ — три стадии: Rust и wasm, `apps/web`, Bun. E2E: `playwright.server.config.ts`, проекты `server` и `server-firefox` против стенда compose `ferrum-test`.

**Tech Stack:** Bun 1.4.2, TypeScript 7.0.2, better-auth 1.7.16, pg 8.23.1, nodemailer 10.2.0, oxlint 1.89.0, prettier 3.9.10, Postgres 18, Valkey 9, Mailpit 1.33.0; SolidJS 1.9.17, @playwright/test 1.64.0 (`chromium`, `firefox`). Rust не меняется.

**Спеки:** `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` — разделы 2, 3, 4, 5, 13, 14 (этап 1).

## Global Constraints

- Ветка `feat/sub4-stage1-server` от c1e237f. Каталог сборки wasm — у каждой копии свой: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0` (в `apps/web` — `"$PWD/../../target"`); команды — из корня копии, если не сказано иное. Нужны Bun 1.4, Node 24, Docker с Compose 2.24+ (`!override`), `wasm-bindgen` 0.2.129.
- Версии точные: `save-exact`, `package-lock.json`, `apps/server/bun.lock`. Корневой `.npmrc` — `before=2026-10-21`, как в `apps/web` (пакет старше 14 дней). `apps/server` ставится Bun, предела возраста там нет; better-auth 1.7.16 вышел 2026-11-03, версию задал автор этапа (B-199).
- Docker: только свои проекты compose — `ferrum` и `ferrum-test`. Порты: сервер 8090 (`FERRUM_PORT`), Mailpit 8025; стенд тестов — 18090, 18025, 11025, 15432, 16379. На машине проверки заняты чужие контейнеры `agent-dev-*` (55432, 56379, 8080): не трогать. `down -v` — только `ferrum-test` и `ferrum`.
- Порт 4173 может быть занят посторонним процессом: его не трогать, E2E тогда — через временный конфиг Playwright с другими портами (не коммитить).
- Основной поток тонкий: экраны входа не читают layout, подсказка «Скоро» — чистым CSS. Код входа не должен попадать в автономную сборку, а код автономного режима — в рабочую. Модули входа не имеют побочных действий при импорте (канал `BroadcastChannel` создаётся лениво), иначе сборщик оставит их в автономной сборке.
- Поведение — как в Figma; где спека молчит, решение помечено **(поведение)** (раздел «Решения»).
- Тесты не проходят вхолостую: сервер — `bun test` против настоящих Postgres, Valkey и Mailpit; E2E — настоящий стенд и письмо через API Mailpit. Мутациями проверено, что тесты ловят `get` вместо `getdel`, `plain` вместо `hashed`, снятие предела на адрес, снятие проверок `Origin`, доверие чужому `X-Forwarded-For`, снятие `ALLOWED_EMAILS` и `sign-out` в выходе со всех устройств.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Первым делом доказан главный риск §14.1, отдельной пробой на Bun 1.4.2, better-auth 1.7.16, Valkey 9 и Postgres 18: письмо → токен хэшем в Valkey со сроком 900 с → подтверждение → `GETDEL` не даёт войти второй раз, из четырёх одновременных подтверждений прошло одно → сессия в Valkey, таблицы только `user` и `account` → проверка сессии → снятие всех сессий. Работает без обходов; `Bun.RedisClient.getdel` есть, `INCR` с `EXPIRE` одним скриптом через `send('EVAL', …)`. Единственная ловушка — Better Auth сверяет схему при первом обращении и запоминает ответ, поэтому `createAuth` создаётся после миграций.

Прототип всего этапа собран в отдельной копии (`plan-verify-s4s1`, ветка `verify/sub4-s1` от c1e237f, по коммиту на задачу; вершина — be00e6e). На его коде прошло: `npm run format:check`, `npm run lint`, `npm run typecheck` (три tsconfig `apps/web`, `apps/server`, `packages/protocol`); `bun test` — 6 pass и 35 pass против настоящих Postgres, Valkey и Mailpit; модульные — 186 passed; `build` и `build:server`, `check-dist.mjs` обеих; E2E автономной сборки, Chromium целиком, `--grep-invert @perf --retries=1` — 415 passed, 0 flaky, 0 failed, 9,1 мин; `e2e:server` — 30 passed (15 сценариев в Chromium и Firefox); `docker compose build --no-cache` — 4 мин 21 с, образ 130 МБ; `docker compose up` с чистых томов — 13 с до готовности всех контейнеров; вход вручную через API Mailpit: письмо, подтверждение, второе подтверждение 400, имя, сессия, она же после `docker compose restart valkey postgres`; `docker compose stop server` — 0,3 с; десктоп: `npx tauri build --no-bundle` запускает `beforeBuildCommand`, `dist` автономный, `FERRUM_SMOKE=1` — `ready: true`, `csp: []`, `io: true`, `screen: true`. Прогон 415 был на предыдущей ревизии `src/ui/auth`: автономная сборка на вершине байт в байт та же (`main-DkaDRxYs.js`, тот же хэш `index.html`), поэтому повторять не нужно.

Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом, который читает сам этот документ, к чистой копии от c1e237f (`plan-apply-s4s1`, ветка `verify/sub4-s1-apply`) в порядке задач с коммитом после каждой: каждый фрагмент «заменить» найден ровно один раз (операций по задачам: 18, 0, 11, 29, 28, 27, 7, 3). Задача 2 выполнена своей командой `npx prettier --write .`. После каждой задачи дерево копии совпало с деревом соответствующего коммита прототипа (тот же хэш у всех восьми), `git diff verify/sub4-s1 HEAD` — пустой.

На этой копии выполнены все шаги «Убедиться, что тесты падают» задач 3–6, причины в шагах записаны из этого прогона, и все шаги «Проверить» всех задач, кроме `npm run e2e:ci` задачи 8 (он на том же дереве прошёл в прототипе): `format:check`, `lint`, `typecheck`, модульные (176, 176, 179, 186, 186), `bun test`, образ и `/healthz` задачи 4, `e2e:server` задач 7 и 8 (по 30 passed), `cargo check` десктопа. Сценарии задачи 7 проверены и наоборот: образ с автономной сборкой — 14 из 15 падают.

Мутации сервера, каждая роняет свой тест: `get` вместо `getdel`, `storeToken: plain`, снятие предела на адрес, снятие проверки `Origin` у маршрутов и у `authorizeUpgrade`, доверие `X-Forwarded-For` без `TRUST_PROXY`, снятие `ALLOWED_EMAILS`, выход со всех устройств без `sign-out`. Не роняет снятие `verification.storeInDatabase: false`: при `secondaryStorage` это умолчание, а значение `true` ловит сверка схемы.

Найденное прототипом и вошедшее в шаги: ссылка, вставленная в ту же вкладку, не открывала страницу подтверждения, адрес после `#` страницу не перезагружает (задача 6, `hashchange`; поймал E2E); условие по флагу в разметке SolidJS не выбрасывает импорт, код входа попадал в автономную сборку (задача 6, `extensions.ts`; поймал `check-dist.mjs`); `new BroadcastChannel` при импорте оставлял модуль в автономной сборке, найдено сравнением хэша бандла (задача 6, ленивый канал); канал, созданный заново на каждое сообщение, доставлял его самой вкладке (задача 6); сбой Valkey выглядел как «сессии нет» и выбросил бы на экран входа (задача 5, теперь 500); без `SIGTERM` Bun как PID 1 ждал бы SIGKILL 10 с, без `pool.on('error')` падал бы при перезапуске Postgres (задача 5); встроенный `rateLimit` считает только по IP и пути, предел на адрес — свой счётчик (задача 5); Better Auth отдаёт токен сессии в теле `verify` и `get-session`, поэтому наши ответы своё тело (задача 5); `getByRole(...).click()` Playwright ждёт активности у `aria-disabled`, тест жмёт мышью (задача 7).

Не запускались: E2E автономной сборки в Firefox (код вне входа не менялся, кроме форматирования), проекты `gpu`, `@perf`, WebKit и Safari, `compat-old-build.sh` (Rust и формат файла не менялись), `cargo fmt`, `clippy`, `cargo test` (`crates/` не менялись, `git diff` пуст), сборки десктопа на Windows и Linux, настоящий SMTP (только Mailpit), HTTPS и `Secure` у cookie на живом стенде (проверена только настройка `useSecureCookies`), `TRUST_PROXY=1` за настоящим прокси (проверен заголовок в `bun test`), образ на amd64 (сборка шла на arm64). Порт 4173 был свободен, чужие контейнеры `agent-dev-*` не трогали.

---

## Этапы подсистемы 4

| Этап | Результат | Разделы спеки |
|---|---|---|
| **1. Сервер и вход** (этот план) | Docker, миграции, Better Auth, magic link, сессии в Valkey, экран входа, обязательный вход; TypeScript 7, oxlint, prettier | 3, 4, 5 |
| 2. Файлы на сервере | команды, проекты, черновики, серверный `FileStore`, роли, «Поделиться», картинки | 6 |
| 3. Синхронизация | журнал, снимки, чужие правки, переподключение | 7, 8 |
| 4. Присутствие | курсоры, аватары, следование | 9 |
| 5. Комментарии | метки, обсуждения, письма | 10 |
| 6. Офлайн и версии | очередь правок, история версий | 11 |
| 7. Десктоп | вход через браузер, серверные файлы | 12 |

## Решения

Где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**. По справке Figma при сборке прототипа ничего не сверялось: это решения по памяти о Figma и из требований этапа.

Вход:

1. **(поведение)** Наружу открыт короткий список: `POST /api/auth/{sign-in/magic-link, confirm, name, sign-out, sign-out-everywhere}`, `GET /api/auth/session`. Остальные точки Better Auth, в том числе `GET /magic-link/verify`, тратящий токен, снаружи отдают 404. Сервер зовёт их внутри, `auth.handler` с собственным запросом.
2. **(поведение)** Ссылка в письме: `PUBLIC_URL/#/auth/confirm?token=…&next=…`. Токен после `#` не попадает на сервер, в журналы и в `Referer`; открытие страницы не делает запросов, тратит токен только кнопка `POST /api/auth/confirm`.
3. **(поведение)** `next` — только маршрут приложения (`#/…`, не `#/auth/…`, до 512 символов, без управляющих): едет в ссылке, поэтому возврат работает и при открытии письма в другом браузере, и не уводит на чужой адрес.
4. **(поведение)** Истёкшая и использованная ссылка неразличимы: ключа в Valkey нет, текст один — «Ссылка недействительна или устарела. Запросите новую» и кнопка «Запросить новую ссылку».
5. **(поведение)** Ссылка, вставленная в ту же вкладку, работает: меняется только адрес после `#`, `AuthGate` слушает `hashchange`. Нашёл прототип: без этого страница подтверждения не открывалась.
6. **(поведение)** Ответы не содержат токена сессии. Better Auth кладёт его в тело `verify` и `get-session`, что обесценило бы `HttpOnly`; наши маршруты отдают `{ ok: true }` и `{ user: { id, email, name } }`.
7. **(поведение)** Адрес приводится к нижнему регистру. Адрес вне `ALLOWED_EMAILS` получает обычный ответ, письма нет. Записи `ALLOWED_EMAILS` — адрес целиком или домен.
8. **(поведение)** Предел на адрес — свой счётчик в том же Valkey (`mail-to:<адрес>`), считается в `sendMagicLink`, после проверки IP: **встроенный `rateLimit` считает только по IP и пути**, спека §5 этого не учитывает. Сверх предела — 429, «Слишком много запросов. Попробуйте позже».
9. **(поведение)** IP — адрес соединения (`server.requestIP`), заголовок для Better Auth ставит сервер сам. `TRUST_PROXY=1` — последняя запись `X-Forwarded-For`. За пробросом порта Docker Desktop и OrbStack адрес — шлюза (B-194).
10. **(поведение)** Вкладка «Проверьте почту» открывается сама, когда вход завершён в другой вкладке; выход тоже виден во всех вкладках (`BroadcastChannel`, перезагрузка). На странице подтверждения чужие события не перебивают нажатие.
11. **(поведение)** Экран имени обязателен, пустое не принимается, до 100 символов; повторный вход его не показывает. Смены имени в настройках нет (B-191).
12. **(поведение)** Выход есть и на экране файлов: настройки открываются только из редактора. «Выйти на всех устройствах» — только в настройках. Правки открытого файла к выходу в журнале IndexedDB, страница перезагружается и дописывает остальное.
13. **(поведение)** Кнопки Google и GitHub — `aria-disabled`, а не `disabled`: подсказка «Скоро» открывается и с клавиатуры.
14. **(поведение)** Кружок с инициалами: первые буквы двух слов имени; оттенок — из хэша `userId`, текст белый.
15. **(поведение)** CSP сервера — как в окне десктопа, плюс `form-action 'self'`. Статика: `assets/` кэшируется навсегда, остальное `no-cache`; запасного `index.html` нет, чужой адрес — 404: маршруты приложения живут после `#`.
16. «Черновики» отдельной записи не требуют: по §6.1 это файлы с пустым `project_id`, создавать при регистрации нечего.
17. Сервер не держит `BETTER_AUTH_SECRET` короче 32 символов и не запускается при неполных настройках: отказ сразу со всем списком.
18. Файлы до этапа 2 в IndexedDB не разделены по пользователям (B-190).

Инструменты и сборки:

19. Prettier: `singleQuote`, `trailingComma: all`, `printWidth: 140`. Замер правок старого кода: 120 — 108 файлов, +2510 −756; 140 — 90 файлов, +1124 −607. Вне форматирования: `docs/`, сгенерированное, lock-файлы.
20. oxlint: `--deny-warnings`, категория по умолчанию. `no-unassigned-vars` выключено: ссылки SolidJS присваиваются разметкой (`ref={el}`), 36 ложных срабатываний (B-189). Остальные 9 замечаний исправлены.
21. `npm run build` и `npm run dev` — автономный режим (`--mode standalone`, `.env.standalone`), чтобы все существующие команды работали как раньше; рабочая — `npm run build:server` в `dist-server`, без `debug.html`. Десктоп получает автономную через `beforeBuildCommand`, что проверяет модульный тест.
22. Части интерфейса только для рабочей сборки ставит `main.tsx` через `extensions.ts`, а не условия по флагу в экранах: прототип показал, что условие в разметке SolidJS не выбрасывает импорт, и `check-dist.mjs` поймал код входа в автономной сборке.
23. `packages/protocol` — исходники `.ts`, подключаются `file:` в обоих пакетах; `apps/server` проверяется корневым `tsc`.

## Структура файлов

| Файл | Ответственность |
|---|---|
| `package.json`, `.npmrc`, `.prettierrc`, `.prettierignore`, `.oxlintrc.json` (новые) | инструменты в корне |
| `Dockerfile`, `.dockerignore`, `docker-compose.yml`, `docker-compose.test.yml`, `docker-compose.test.env`, `.env.example` (новые) | развёртывание и стенд тестов |
| `packages/protocol/` (новый) | разбор запросов входа, адреса, константы |
| `apps/server/src/{config,migrate,blob-store,static,valkey,app,index}.ts` (новые) | скелет сервера |
| `apps/server/src/{mail,secondary-storage,auth,auth-routes}.ts`, `auth-schema.ts`, `migrations/` (новые) | вход |
| `apps/server/test/` (новый) | `bun test` против настоящего стенда |
| `apps/web/src/ui/auth/`, `extensions.ts`, `styles/auth.css` (новые) | экраны входа, сессия, аккаунт |
| `apps/web/src/main.tsx`, `src/ui/{settings-dialog,files-screen}.tsx`, `vite.config.ts`, `package.json`, `scripts/check-dist.mjs`, `.env.standalone` | сборки, вкладка и строка аккаунта |
| `apps/desktop/src-tauri/tauri.conf.json` | автономная сборка перед сборкой десктопа |
| `apps/web/e2e/standalone.spec.ts`, `unit/{check-dist,auth-logic}.test.ts` (новые) | сборки, логика входа |
| `apps/web/e2e-server/`, `playwright.server.config.ts` (новые) | проект `server` |
| `docs/running.md` (новый), спека, `docs/backlog.md` | запуск, «Сделано (этап 1)», B-189…B-199 |

---

### Task 1: Инструменты — TypeScript 7, oxlint, prettier

Спека §3, §14.5. Корневой `package.json`, настройки oxlint и prettier, `apps/web` на TypeScript 7.0.2, все три tsconfig проверяются им. Замечания oxlint на старом коде исправлены, одно правило выключено. Форматирование — следующая задача отдельным коммитом.

**Files:**
- Create: `.npmrc`, `.oxlintrc.json`, `.prettierignore`, `.prettierrc`, `package-lock.json`, `package.json`
- Modify: `.gitignore`, `apps/web/package-lock.json`, `apps/web/package.json`
- Test: `apps/web/e2e/a11y.spec.ts`, `apps/web/e2e/clipboard.spec.ts`, `apps/web/e2e/fallback.spec.ts`, `apps/web/e2e/shared.spec.ts`, `apps/web/unit/image-size.test.ts`, `apps/web/unit/messages.test.ts`, `apps/web/unit/toasts.test.ts`

**Interfaces:**
- Consumes: `apps/web/tsconfig{,.worker,.e2e}.json`.
- Produces: `npm run typecheck`, `npm run lint`, `npm run format`, `npm run format:check` в корне.

- [ ] **Step 1: Реализация**

В `apps/web/e2e/a11y.spec.ts` заменить:

````ts
function contrast(a: string, b: string): number {
  const lum = (text: string) => {
    const hex = text.length === 4 ? `#${[...text.slice(1)].map((c) => c + c).join('')}` : text;
    const [r, g, bl] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    const f = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
````

на:

````ts
function contrast(a: string, b: string): number {
  const lum = (text: string) => {
    const hex = text.length === 4 ? `#${Array.from(text.slice(1)).map((c) => c + c).join('')}` : text;
    const [r, g, bl] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    const f = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
````

В `apps/web/e2e/clipboard.spec.ts` заменить:

````ts
  // JPEG в несколько байт объявляет 60 000 × 60 000: декодировать его нельзя.
  await page.evaluate(() => {
    const app0 = [0xff, 0xe0, 0, 16, ...new Array(14).fill(0)];
    const sof = [0xff, 0xc0, 0, 17, 8, 0xea, 0x60, 0xea, 0x60, 3, ...new Array(9).fill(0)];
    const bytes = new Uint8Array([0xff, 0xd8, ...app0, ...sof, 0xff, 0xd9]);
    window.__ferrum!.core!.send({ type: 'pasteImage', bytes });
````

на:

````ts
  // JPEG в несколько байт объявляет 60 000 × 60 000: декодировать его нельзя.
  await page.evaluate(() => {
    const app0 = [0xff, 0xe0, 0, 16, ...Array.from({ length: 14 }, () => 0)];
    const sof = [0xff, 0xc0, 0, 17, 8, 0xea, 0x60, 0xea, 0x60, 3, ...Array.from({ length: 9 }, () => 0)];
    const bytes = new Uint8Array([0xff, 0xd8, ...app0, ...sof, 0xff, 0xd9]);
    window.__ferrum!.core!.send({ type: 'pasteImage', bytes });
````

В `apps/web/e2e/fallback.spec.ts` заменить:

````ts
  await liveWhen(page, 'l.nodes === 1');
  // Причина потери — в консоли (предупреждение о потере, до попыток пересоздать).
  expect(warnings.some((t) => /^Устройство WebGPU потеряно/.test(t))).toBe(true);
});

````

на:

````ts
  await liveWhen(page, 'l.nodes === 1');
  // Причина потери — в консоли (предупреждение о потере, до попыток пересоздать).
  expect(warnings.some((t) => t.startsWith('Устройство WebGPU потеряно'))).toBe(true);
});

````

В `apps/web/e2e/shared.spec.ts` заменить:

````ts
import { MessageTransport, SabTransport } from '../src/core/transport';

const MiB = 1024 * 1024;

function sharedParts() {
````

на:

````ts
import { MessageTransport, SabTransport } from '../src/core/transport';


function sharedParts() {
````

В `apps/web/unit/image-size.test.ts` заменить:

````ts

test('JPEG: маркер SOF после APP0, огромный размер читается без декодирования', () => {
  const app0 = [0xff, 0xe0, 0, 16, ...new Array(14).fill(0)];
  const sof = [0xff, 0xc2, 0, 17, 8, 0xea, 0x60, 0xea, 0x60, 3, ...new Array(9).fill(0)];
  expect(imageSize(bytes([0xff, 0xd8], app0, sof))).toEqual({ width: 60000, height: 60000, type: 'jpeg' });
  expect(imageSize(bytes([0xff, 0xd8], app0))).toBeNull();
````

на:

````ts

test('JPEG: маркер SOF после APP0, огромный размер читается без декодирования', () => {
  const app0 = [0xff, 0xe0, 0, 16, ...Array.from({ length: 14 }, () => 0)];
  const sof = [0xff, 0xc2, 0, 17, 8, 0xea, 0x60, 0xea, 0x60, 3, ...Array.from({ length: 9 }, () => 0)];
  expect(imageSize(bytes([0xff, 0xd8], app0, sof))).toEqual({ width: 60000, height: 60000, type: 'jpeg' });
  expect(imageSize(bytes([0xff, 0xd8], app0))).toBeNull();
````

В `apps/web/unit/messages.test.ts` заменить:

````ts
    'Неверная команда: данные не в виде байтов',
  );
  const many = new Array<Uint8Array>(MAX_JOURNAL_ENTRIES + 1).fill(new Uint8Array(1));
  expect(commandSizeError({ type: 'openWithJournal', bytes: file, journal: many })).toBe(
    'Неверная команда: в журнале изменений больше 100000 записей',
````

на:

````ts
    'Неверная команда: данные не в виде байтов',
  );
  const many = Array.from({ length: MAX_JOURNAL_ENTRIES + 1 }, () => new Uint8Array(1));
  expect(commandSizeError({ type: 'openWithJournal', bytes: file, journal: many })).toBe(
    'Неверная команда: в журнале изменений больше 100000 записей',
````

В `apps/web/unit/toasts.test.ts` заменить:

````ts
    advance(ms: number) {
      now += ms;
      for (const [id, t] of [...queue]) {
        if (t.at <= now) {
          queue.delete(id);
````

на:

````ts
    advance(ms: number) {
      now += ms;
      for (const [id, t] of Array.from(queue)) {
        if (t.at <= now) {
          queue.delete(id);
````

В `.gitignore` заменить:

````
target/
# Стенд apps/web: зависимости, сборка, ядро из wasm-bindgen, отчёты Playwright.
/apps/web/node_modules/
/apps/web/dist/
/apps/web/src/wasm/
/apps/web/test-results/
/apps/web/playwright-report/
````

на:

````
target/
# Стенд apps/web: зависимости, сборка, ядро из wasm-bindgen, отчёты Playwright.
/apps/web/dist/
/apps/web/src/wasm/
/apps/web/test-results/
/apps/web/playwright-report/

# Инструменты в корне и сервер: зависимости; настройки развёртывания с секретами.
node_modules/
/.env
````

Создать `.npmrc`:

````
save-exact=true
# Пакеты не новее этой даты (≥ 14 дней до начала этапа 1 подсистемы 4), как в apps/web.
before=2026-10-21
````

Создать `.oxlintrc.json`:

````json
{
  "ignorePatterns": [
    "target/**",
    "docs/**",
    "**/dist/**",
    "apps/web/src/wasm/**",
    "apps/web/src/core/layout.gen.ts",
    "apps/web/public/**",
    "apps/desktop/src-tauri/gen/**"
  ],
  "rules": {
    // Ссылка на элемент в SolidJS присваивается разметкой (`ref={el}`), правило этого не видит (бэклог).
    "no-unassigned-vars": "off"
  }
}
````

Создать `.prettierignore`:

````
target/
docs/
**/node_modules/
**/dist/
**/package-lock.json
apps/web/src/wasm/
apps/web/src/core/layout.gen.ts
apps/web/test-results/
apps/web/playwright-report/
apps/desktop/src-tauri/gen/
apps/desktop/src-tauri/icons/
.superpowers/
````

Создать `.prettierrc`:

````
{
  "singleQuote": true,
  "trailingComma": "all",
  "printWidth": 140
}
````

В `apps/web/package-lock.json` заменить:

````json
        "@playwright/test": "1.64.0",
        "@types/node": "24.19.1",
        "typescript": "6.0.3",
        "vite": "8.3.4",
        "vite-plugin-solid": "2.11.14"
````

на:

````json
        "@playwright/test": "1.64.0",
        "@types/node": "24.19.1",
        "typescript": "7.0.2",
        "vite": "8.3.4",
        "vite-plugin-solid": "2.11.14"
````

В `apps/web/package-lock.json` заменить:

````json
      "dependencies": {
        "undici-types": ">=7.24.0 <7.24.7"
      }
    },
````

на:

````json
      "dependencies": {
        "undici-types": ">=7.24.0 <7.24.7"
      }
    },
    "node_modules/@typescript/typescript-aix-ppc64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-aix-ppc64/-/typescript-aix-ppc64-7.0.2.tgz",
      "integrity": "sha512-MTKKkWB7p/0E9xi1d1tHtZ5PiLkGEMIq88pK2CubZjOsLtYTLqhgIgi6zepFa+9GHZ6h05NMCkQxGKiPXMxXtQ==",
      "cpu": [
        "ppc64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "aix"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-darwin-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-darwin-arm64/-/typescript-darwin-arm64-7.0.2.tgz",
      "integrity": "sha512-gowzar9MwS/aRWp6f3a4KUqzRjAZjOsmGNCM6LcTgXum+dBfgsBVMN+AgvOCCbguXyick6LJhpBszxMebJ8syA==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-darwin-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-darwin-x64/-/typescript-darwin-x64-7.0.2.tgz",
      "integrity": "sha512-SZ9xZInqApNlNGc9s0W1VSsktYSOe9cFqNOIqmN1Gs8SmkjKZYFt017G4VwPxASInODuAdbTW7sXiFUf893RgA==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-freebsd-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-freebsd-arm64/-/typescript-freebsd-arm64-7.0.2.tgz",
      "integrity": "sha512-W5NH4y/J0plIIS5b2xvTEkU7JFxyqdMAOgf+Ilhl0vHQXKO5dZoxd+C/jEtq56c4F3wk71RB4BMRQ2XdI+bwYQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "freebsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-freebsd-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-freebsd-x64/-/typescript-freebsd-x64-7.0.2.tgz",
      "integrity": "sha512-UMGDx5sTpzNw3WiPebH7l90IWfJggEd+egHt/q6p7/Cm3zqoV7VxkGXt+3DxPIw8CcmvAB0j3sVVfbhX+M4Tpw==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "freebsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-arm": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-arm/-/typescript-linux-arm-7.0.2.tgz",
      "integrity": "sha512-gffT3xPz9sR7j/YJExkyPntrI0P2EP9XbOyWzth2/Gs0RstK+90RBcO0ncXoXy/beYll1SXw846Nf2zdnEz0QQ==",
      "cpu": [
        "arm"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-arm64/-/typescript-linux-arm64-7.0.2.tgz",
      "integrity": "sha512-Qh4eU4/y3yDjnfjjyPYihMj5/ODIlmt+Bzu17OI+fiSRDW57QmU5SiN63exPRNJPKUzcc1INa1NXdrJ+MqHjUQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-loong64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-loong64/-/typescript-linux-loong64-7.0.2.tgz",
      "integrity": "sha512-uEHck9i8hoAzXPiYRib1O7miOnz23SxIeVl6F4LXox+qov1K35jHcEW6VHKvZI+pyvl7fZEP4MCU5LYvIq1GuQ==",
      "cpu": [
        "loong64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-mips64el": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-mips64el/-/typescript-linux-mips64el-7.0.2.tgz",
      "integrity": "sha512-R4KvAMnE43W5Qeqb0Ly56O3mWMWIAgsMyz36DCaycd5nbg/9kzm0liw3JocfRqyJY0KPmzFjbswozXyW0DnIYA==",
      "cpu": [
        "mips64el"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-ppc64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-ppc64/-/typescript-linux-ppc64-7.0.2.tgz",
      "integrity": "sha512-DORx5b3sd/4S7eayxm4FQv+A7CrkUIGRaHiwI8oiHTAI1fAPWhF4J0vAlkC8biAlHSVVwxMQ3tjZ2/DVbnQiiA==",
      "cpu": [
        "ppc64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-riscv64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-riscv64/-/typescript-linux-riscv64-7.0.2.tgz",
      "integrity": "sha512-wf0jqEDOjrPRnKwYRyyJDRo11KMbvMFrU+q4zqKyChODBzvlkbhNQfKvLxQCcwTpdDaXSHZTVuh0JoCrKCUMHQ==",
      "cpu": [
        "riscv64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-s390x": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-s390x/-/typescript-linux-s390x-7.0.2.tgz",
      "integrity": "sha512-IkwJc3L7yhytWd/ewjyxNDfOmswCm9GWMJT/ue/dU4aZNbwZeYAetq42VyLmsmSjvoX7z74X6ZaYCtzAr0EuGw==",
      "cpu": [
        "s390x"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-x64/-/typescript-linux-x64-7.0.2.tgz",
      "integrity": "sha512-EYdf2cNg7rgCWJnxCdJ+F3V39O8ihb37eHAu1LK8oAFizgTQbPOK7zHHXbPt8rX24COqODXeI3sIf0fCXG7H/A==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-netbsd-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-netbsd-arm64/-/typescript-netbsd-arm64-7.0.2.tgz",
      "integrity": "sha512-+polYF4MF04aPpO5FTkHran9yUQDSXqy5GiSDKpsll5jy3l3+g9QLhpf39T+ePtefhXLOGrLl0QIjkQP6VnelA==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "netbsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-netbsd-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-netbsd-x64/-/typescript-netbsd-x64-7.0.2.tgz",
      "integrity": "sha512-8YIT0EHM/3dq10ZOVF/A7pc/YSMtbcecct4rWtexrnSCHOPcpC2KTLXfTCR6vDpnSiY12heNb1GiN/wu+T/FyA==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "netbsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-openbsd-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-openbsd-arm64/-/typescript-openbsd-arm64-7.0.2.tgz",
      "integrity": "sha512-APT8+ClYnuYm1u9+kgGXoMj2VzWzcymwh2gNSQVySHfkRDGOTVkoWLjCmOQSaO+PoqQ57B0flRp9SA+7GnnkzQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "openbsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-openbsd-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-openbsd-x64/-/typescript-openbsd-x64-7.0.2.tgz",
      "integrity": "sha512-yX7s+Q0Dln0Dt9tEzZsAjXXR/+ytBM7AlglaqyeMPxQszJ1JhlJdZ6jLA+IzldHtflX81em7lDao1xXu+aRRkg==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "openbsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-sunos-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-sunos-x64/-/typescript-sunos-x64-7.0.2.tgz",
      "integrity": "sha512-dLJDGaLZ1D4HPQn62u1n8mBDkJREwMsAkCdkwd4Ieqw+x3TUyTsqY0YiBCtE6H6OzzgGk3iuZ3vFWRS+E8/d1g==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "sunos"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-win32-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-win32-arm64/-/typescript-win32-arm64-7.0.2.tgz",
      "integrity": "sha512-Gyl1Vy6OsWesLzmq+EP0Fb7b4Nid5232AvcA2SFcdYreldpNtYFFofPjnt62y9hQy7VTaZp65ICJjuAQRaVcIQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-win32-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-win32-x64/-/typescript-win32-x64-7.0.2.tgz",
      "integrity": "sha512-0BQ3HkAHHlKLSp1qRvf3SUhGpGsDuhB/jgFw75guyqbxJqEaS0Cw/VFO8i2nHglJUzQCRtMMR/IBAKE3ETMC4g==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
````

В `apps/web/package-lock.json` заменить:

````json
    },
    "node_modules/typescript": {
      "version": "6.0.3",
      "resolved": "https://registry.npmjs.org/typescript/-/typescript-6.0.3.tgz",
      "integrity": "sha512-y2TvuxSZPDyQakkFRPZHKFm+KKVqIisdg9/CZwm9ftvKXLP8NRWj38/ODjNbr43SsoXqNuAisEf1GdCxqWcdBw==",
      "dev": true,
      "license": "Apache-2.0",
      "bin": {
        "tsc": "bin/tsc",
        "tsserver": "bin/tsserver"
      },
      "engines": {
        "node": ">=14.17"
      }
    },
````

на:

````json
    },
    "node_modules/typescript": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/typescript/-/typescript-7.0.2.tgz",
      "integrity": "sha512-8FYau96o3NKOhbjKi/qNvG/W5jhzxkbdm5sj9AbZ/5T5sWqn3hJgLfGx27sRKZWTvyzCP8dLRBTf5tBTSRVUNA==",
      "dev": true,
      "license": "Apache-2.0",
      "bin": {
        "tsc": "bin/tsc"
      },
      "engines": {
        "node": ">=16.20.0"
      },
      "optionalDependencies": {
        "@typescript/typescript-aix-ppc64": "7.0.2",
        "@typescript/typescript-darwin-arm64": "7.0.2",
        "@typescript/typescript-darwin-x64": "7.0.2",
        "@typescript/typescript-freebsd-arm64": "7.0.2",
        "@typescript/typescript-freebsd-x64": "7.0.2",
        "@typescript/typescript-linux-arm": "7.0.2",
        "@typescript/typescript-linux-arm64": "7.0.2",
        "@typescript/typescript-linux-loong64": "7.0.2",
        "@typescript/typescript-linux-mips64el": "7.0.2",
        "@typescript/typescript-linux-ppc64": "7.0.2",
        "@typescript/typescript-linux-riscv64": "7.0.2",
        "@typescript/typescript-linux-s390x": "7.0.2",
        "@typescript/typescript-linux-x64": "7.0.2",
        "@typescript/typescript-netbsd-arm64": "7.0.2",
        "@typescript/typescript-netbsd-x64": "7.0.2",
        "@typescript/typescript-openbsd-arm64": "7.0.2",
        "@typescript/typescript-openbsd-x64": "7.0.2",
        "@typescript/typescript-sunos-x64": "7.0.2",
        "@typescript/typescript-win32-arm64": "7.0.2",
        "@typescript/typescript-win32-x64": "7.0.2"
      }
    },
````

В `apps/web/package.json` заменить:

````json
    "@playwright/test": "1.64.0",
    "@types/node": "24.19.1",
    "typescript": "6.0.3",
    "vite": "8.3.4",
    "vite-plugin-solid": "2.11.14"
````

на:

````json
    "@playwright/test": "1.64.0",
    "@types/node": "24.19.1",
    "typescript": "7.0.2",
    "vite": "8.3.4",
    "vite-plugin-solid": "2.11.14"
````

Создать `package-lock.json`:

````json
{
  "name": "ferrum",
  "version": "0.0.0",
  "lockfileVersion": 3,
  "requires": true,
  "packages": {
    "": {
      "name": "ferrum",
      "version": "0.0.0",
      "devDependencies": {
        "oxlint": "1.89.0",
        "prettier": "3.9.10",
        "typescript": "7.0.2"
      }
    },
    "node_modules/@oxlint/binding-android-arm-eabi": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-android-arm-eabi/-/binding-android-arm-eabi-1.89.0.tgz",
      "integrity": "sha512-Pflpba5F3gRAQ12sWglRxvWTHEdAusNDvmi4Y5N1T5SY1AGkM7AixOnET8qkrnLJ/F32qAlNokO6eCknT8tmnw==",
      "cpu": [
        "arm"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "android"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-android-arm64": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-android-arm64/-/binding-android-arm64-1.89.0.tgz",
      "integrity": "sha512-dYmyD1x7LaPlb79vZWUcpTShk8dI9z1I+IubtqrBxWNJaDhQdPHwcsvg7q5StTU4xWyd9AwPfLaTYFZY8NZgtQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "android"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-darwin-arm64": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-darwin-arm64/-/binding-darwin-arm64-1.89.0.tgz",
      "integrity": "sha512-Z/oxOPiIqiS7fItRdxaaOEC+6GD3OpGi0IxoR0dMefzQHolsKLzScw2DNcu/5gUPok3tv+G9mslvYB5bo3i0qg==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-darwin-x64": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-darwin-x64/-/binding-darwin-x64-1.89.0.tgz",
      "integrity": "sha512-eAjTkHSjAxrdCPJxO6y5sPQL5pYLKb2kvxZz9UU7tXnOZk9I/HLtkvx9S7GHY883k+AmL/eC7mqFt+w7WCwk0w==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-freebsd-x64": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-freebsd-x64/-/binding-freebsd-x64-1.89.0.tgz",
      "integrity": "sha512-0lNCA9Tl1n1FKpqy2/9ckp0kIuSrtJ/l9bMRgaEhOtM7TsOHshFuwOcvW8oA9teXLMOJT61LjEKWiaHDFGxF/w==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "freebsd"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-linux-arm-gnueabihf": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-linux-arm-gnueabihf/-/binding-linux-arm-gnueabihf-1.89.0.tgz",
      "integrity": "sha512-OTFEvi9W+JczaZD1Caj17SGCoW8H7TE+fvx0D7ATEWa1Lohn91UMxFQ6r145SiGy/yCxeIwptMys/RAM7gSVSQ==",
      "cpu": [
        "arm"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-linux-arm-musleabihf": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-linux-arm-musleabihf/-/binding-linux-arm-musleabihf-1.89.0.tgz",
      "integrity": "sha512-fRxgNHvBLswj6MrhW2uBq5UDhPRQlCbHeq/Nf8yuG9zOxhDsXqX/mi2PX1axgsQxMm88el7HLiJAzglAnflYxw==",
      "cpu": [
        "arm"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-linux-arm64-gnu": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-linux-arm64-gnu/-/binding-linux-arm64-gnu-1.89.0.tgz",
      "integrity": "sha512-v9ENzHvwrQoYSWTtviH1pOYc7R9UovDxVofnck50cNmPihRRTcrg5X0UlRbLOZ7IVtxPOlh2hb7kDO+FpE8SfQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-linux-arm64-musl": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-linux-arm64-musl/-/binding-linux-arm64-musl-1.89.0.tgz",
      "integrity": "sha512-vfffe8mQ484AzVadlUOFpgL+JDwhvs9hrzxTgQP1FVfi4ZEUUYeffXlzC9dFV7Dgnu5s/KrNKvfmG6HxBd/ISQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-linux-ppc64-gnu": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-linux-ppc64-gnu/-/binding-linux-ppc64-gnu-1.89.0.tgz",
      "integrity": "sha512-Buonlp4hVS7RGNP5M7bPpGNQrenyzA2C64JcO/oz9C1c6dGOXJ7VEAnaiAMY7voou0JYyCJdvDVlFQc5DZX3DQ==",
      "cpu": [
        "ppc64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-linux-riscv64-gnu": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-linux-riscv64-gnu/-/binding-linux-riscv64-gnu-1.89.0.tgz",
      "integrity": "sha512-1eVwJJrJ3lYIPT+cm7BcBBZZXY6IQZP/oyonaruTor/MrVyWOpvnwt1oD5CLdsHg5aVBIxXodQSvt+1gK6ck5w==",
      "cpu": [
        "riscv64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-linux-riscv64-musl": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-linux-riscv64-musl/-/binding-linux-riscv64-musl-1.89.0.tgz",
      "integrity": "sha512-fRQQsKUGShdEL5Fl9o9uhj3grkZbeXKLv0icQ5tspUCNYSDy8wzzEcP1WC2bKdTQ0rSszbKlO3iiHORxV22DLg==",
      "cpu": [
        "riscv64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-linux-s390x-gnu": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-linux-s390x-gnu/-/binding-linux-s390x-gnu-1.89.0.tgz",
      "integrity": "sha512-dEtYjIy6+fHcjKklsmtmMkZRhwHr2UbzjmpvuPtQYSeN9awQ2hx5JLms5Wvri+8P/NZtD8v5hfgHALUql+/Ycg==",
      "cpu": [
        "s390x"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-linux-x64-gnu": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-linux-x64-gnu/-/binding-linux-x64-gnu-1.89.0.tgz",
      "integrity": "sha512-hqjwIj5KOYM8Wjef/p7IruNi1iByLhSzbMBk62y22P/E8q7Jj0Pp+DeUTKTRYyZYCDtU1wEGlnWOnttHbyNIww==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-linux-x64-musl": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-linux-x64-musl/-/binding-linux-x64-musl-1.89.0.tgz",
      "integrity": "sha512-iAQN3e6o9UgUCcdGwqRx/jYcKbG3HK7ur3rH6lTqVcQeNGZ85AeWIg6cgn/edwFQ0ofEZZItMwkzyCVTahnvEA==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-openharmony-arm64": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-openharmony-arm64/-/binding-openharmony-arm64-1.89.0.tgz",
      "integrity": "sha512-MNxjEByz3N/7Q4qFQuPWvc14TpN9LoD3pECEDsJTZbP/wyB7U+hduUXEl7QAbbgVezFwsb9sWtNUO6XVBndOhg==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "openharmony"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-win32-arm64-msvc": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-win32-arm64-msvc/-/binding-win32-arm64-msvc-1.89.0.tgz",
      "integrity": "sha512-xxe+i9wfuFso/VnoL4mfj0UlzE+cfCFiUB06vkXCMyQdgVVmItF5FfAF4tATAktSU6uHruWR+FjqvAT9cHZZzg==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-win32-ia32-msvc": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-win32-ia32-msvc/-/binding-win32-ia32-msvc-1.89.0.tgz",
      "integrity": "sha512-saFy07cO94E664glwgLmKLI40HgTL1PY86y4T6+ud4niX4H0Kdqjcmqmjps7mQNdBytV5guGbXDdyQtRfeLbLA==",
      "cpu": [
        "ia32"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@oxlint/binding-win32-x64-msvc": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/@oxlint/binding-win32-x64-msvc/-/binding-win32-x64-msvc-1.89.0.tgz",
      "integrity": "sha512-hLFJ/o0f/dTJh/EW0vmcbREYSQO9tYgfiAAlsOkiLFiJuRRWvYtFELUXN5o2ijpSo/tlO1n+YlWN58tZwPG18A==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@typescript/typescript-aix-ppc64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-aix-ppc64/-/typescript-aix-ppc64-7.0.2.tgz",
      "integrity": "sha512-MTKKkWB7p/0E9xi1d1tHtZ5PiLkGEMIq88pK2CubZjOsLtYTLqhgIgi6zepFa+9GHZ6h05NMCkQxGKiPXMxXtQ==",
      "cpu": [
        "ppc64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "aix"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-darwin-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-darwin-arm64/-/typescript-darwin-arm64-7.0.2.tgz",
      "integrity": "sha512-gowzar9MwS/aRWp6f3a4KUqzRjAZjOsmGNCM6LcTgXum+dBfgsBVMN+AgvOCCbguXyick6LJhpBszxMebJ8syA==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-darwin-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-darwin-x64/-/typescript-darwin-x64-7.0.2.tgz",
      "integrity": "sha512-SZ9xZInqApNlNGc9s0W1VSsktYSOe9cFqNOIqmN1Gs8SmkjKZYFt017G4VwPxASInODuAdbTW7sXiFUf893RgA==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-freebsd-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-freebsd-arm64/-/typescript-freebsd-arm64-7.0.2.tgz",
      "integrity": "sha512-W5NH4y/J0plIIS5b2xvTEkU7JFxyqdMAOgf+Ilhl0vHQXKO5dZoxd+C/jEtq56c4F3wk71RB4BMRQ2XdI+bwYQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "freebsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-freebsd-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-freebsd-x64/-/typescript-freebsd-x64-7.0.2.tgz",
      "integrity": "sha512-UMGDx5sTpzNw3WiPebH7l90IWfJggEd+egHt/q6p7/Cm3zqoV7VxkGXt+3DxPIw8CcmvAB0j3sVVfbhX+M4Tpw==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "freebsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-arm": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-arm/-/typescript-linux-arm-7.0.2.tgz",
      "integrity": "sha512-gffT3xPz9sR7j/YJExkyPntrI0P2EP9XbOyWzth2/Gs0RstK+90RBcO0ncXoXy/beYll1SXw846Nf2zdnEz0QQ==",
      "cpu": [
        "arm"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-arm64/-/typescript-linux-arm64-7.0.2.tgz",
      "integrity": "sha512-Qh4eU4/y3yDjnfjjyPYihMj5/ODIlmt+Bzu17OI+fiSRDW57QmU5SiN63exPRNJPKUzcc1INa1NXdrJ+MqHjUQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-loong64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-loong64/-/typescript-linux-loong64-7.0.2.tgz",
      "integrity": "sha512-uEHck9i8hoAzXPiYRib1O7miOnz23SxIeVl6F4LXox+qov1K35jHcEW6VHKvZI+pyvl7fZEP4MCU5LYvIq1GuQ==",
      "cpu": [
        "loong64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-mips64el": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-mips64el/-/typescript-linux-mips64el-7.0.2.tgz",
      "integrity": "sha512-R4KvAMnE43W5Qeqb0Ly56O3mWMWIAgsMyz36DCaycd5nbg/9kzm0liw3JocfRqyJY0KPmzFjbswozXyW0DnIYA==",
      "cpu": [
        "mips64el"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-ppc64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-ppc64/-/typescript-linux-ppc64-7.0.2.tgz",
      "integrity": "sha512-DORx5b3sd/4S7eayxm4FQv+A7CrkUIGRaHiwI8oiHTAI1fAPWhF4J0vAlkC8biAlHSVVwxMQ3tjZ2/DVbnQiiA==",
      "cpu": [
        "ppc64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-riscv64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-riscv64/-/typescript-linux-riscv64-7.0.2.tgz",
      "integrity": "sha512-wf0jqEDOjrPRnKwYRyyJDRo11KMbvMFrU+q4zqKyChODBzvlkbhNQfKvLxQCcwTpdDaXSHZTVuh0JoCrKCUMHQ==",
      "cpu": [
        "riscv64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-s390x": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-s390x/-/typescript-linux-s390x-7.0.2.tgz",
      "integrity": "sha512-IkwJc3L7yhytWd/ewjyxNDfOmswCm9GWMJT/ue/dU4aZNbwZeYAetq42VyLmsmSjvoX7z74X6ZaYCtzAr0EuGw==",
      "cpu": [
        "s390x"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-linux-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-linux-x64/-/typescript-linux-x64-7.0.2.tgz",
      "integrity": "sha512-EYdf2cNg7rgCWJnxCdJ+F3V39O8ihb37eHAu1LK8oAFizgTQbPOK7zHHXbPt8rX24COqODXeI3sIf0fCXG7H/A==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-netbsd-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-netbsd-arm64/-/typescript-netbsd-arm64-7.0.2.tgz",
      "integrity": "sha512-+polYF4MF04aPpO5FTkHran9yUQDSXqy5GiSDKpsll5jy3l3+g9QLhpf39T+ePtefhXLOGrLl0QIjkQP6VnelA==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "netbsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-netbsd-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-netbsd-x64/-/typescript-netbsd-x64-7.0.2.tgz",
      "integrity": "sha512-8YIT0EHM/3dq10ZOVF/A7pc/YSMtbcecct4rWtexrnSCHOPcpC2KTLXfTCR6vDpnSiY12heNb1GiN/wu+T/FyA==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "netbsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-openbsd-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-openbsd-arm64/-/typescript-openbsd-arm64-7.0.2.tgz",
      "integrity": "sha512-APT8+ClYnuYm1u9+kgGXoMj2VzWzcymwh2gNSQVySHfkRDGOTVkoWLjCmOQSaO+PoqQ57B0flRp9SA+7GnnkzQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "openbsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-openbsd-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-openbsd-x64/-/typescript-openbsd-x64-7.0.2.tgz",
      "integrity": "sha512-yX7s+Q0Dln0Dt9tEzZsAjXXR/+ytBM7AlglaqyeMPxQszJ1JhlJdZ6jLA+IzldHtflX81em7lDao1xXu+aRRkg==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "openbsd"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-sunos-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-sunos-x64/-/typescript-sunos-x64-7.0.2.tgz",
      "integrity": "sha512-dLJDGaLZ1D4HPQn62u1n8mBDkJREwMsAkCdkwd4Ieqw+x3TUyTsqY0YiBCtE6H6OzzgGk3iuZ3vFWRS+E8/d1g==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "sunos"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-win32-arm64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-win32-arm64/-/typescript-win32-arm64-7.0.2.tgz",
      "integrity": "sha512-Gyl1Vy6OsWesLzmq+EP0Fb7b4Nid5232AvcA2SFcdYreldpNtYFFofPjnt62y9hQy7VTaZp65ICJjuAQRaVcIQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/@typescript/typescript-win32-x64": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/@typescript/typescript-win32-x64/-/typescript-win32-x64-7.0.2.tgz",
      "integrity": "sha512-0BQ3HkAHHlKLSp1qRvf3SUhGpGsDuhB/jgFw75guyqbxJqEaS0Cw/VFO8i2nHglJUzQCRtMMR/IBAKE3ETMC4g==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": ">=16.20.0"
      }
    },
    "node_modules/oxlint": {
      "version": "1.89.0",
      "resolved": "https://registry.npmjs.org/oxlint/-/oxlint-1.89.0.tgz",
      "integrity": "sha512-a25hkfo61okGPvJz432fRIPoxlKtNnOVy9w4rzrmGgNm4iVTPwpSQlmdqNWnp6p9QyyPVfAZRgnf5XvSBZFk/A==",
      "dev": true,
      "license": "MIT",
      "bin": {
        "oxlint": "bin/oxlint"
      },
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      },
      "funding": {
        "url": "https://github.com/sponsors/oxc-project"
      },
      "optionalDependencies": {
        "@oxlint/binding-android-arm-eabi": "1.89.0",
        "@oxlint/binding-android-arm64": "1.89.0",
        "@oxlint/binding-darwin-arm64": "1.89.0",
        "@oxlint/binding-darwin-x64": "1.89.0",
        "@oxlint/binding-freebsd-x64": "1.89.0",
        "@oxlint/binding-linux-arm-gnueabihf": "1.89.0",
        "@oxlint/binding-linux-arm-musleabihf": "1.89.0",
        "@oxlint/binding-linux-arm64-gnu": "1.89.0",
        "@oxlint/binding-linux-arm64-musl": "1.89.0",
        "@oxlint/binding-linux-ppc64-gnu": "1.89.0",
        "@oxlint/binding-linux-riscv64-gnu": "1.89.0",
        "@oxlint/binding-linux-riscv64-musl": "1.89.0",
        "@oxlint/binding-linux-s390x-gnu": "1.89.0",
        "@oxlint/binding-linux-x64-gnu": "1.89.0",
        "@oxlint/binding-linux-x64-musl": "1.89.0",
        "@oxlint/binding-openharmony-arm64": "1.89.0",
        "@oxlint/binding-win32-arm64-msvc": "1.89.0",
        "@oxlint/binding-win32-ia32-msvc": "1.89.0",
        "@oxlint/binding-win32-x64-msvc": "1.89.0"
      },
      "peerDependencies": {
        "oxlint-tsgolint": ">=7.0.2003",
        "vite-plus": "*"
      },
      "peerDependenciesMeta": {
        "oxlint-tsgolint": {
          "optional": true
        },
        "vite-plus": {
          "optional": true
        }
      }
    },
    "node_modules/prettier": {
      "version": "3.9.10",
      "resolved": "https://registry.npmjs.org/prettier/-/prettier-3.9.10.tgz",
      "integrity": "sha512-skGHTH1Yb9QvaTC4hWnRTzEYtCpApF0UMBPz+t/0ZPzP8ihefo/btKd+QA7pUE0fEwA1SbS/NmhyEcstdXOIdA==",
      "dev": true,
      "license": "MIT",
      "bin": {
        "prettier": "bin/prettier.cjs"
      },
      "engines": {
        "node": ">=14"
      },
      "funding": {
        "url": "https://github.com/prettier/prettier?sponsor=1"
      }
    },
    "node_modules/typescript": {
      "version": "7.0.2",
      "resolved": "https://registry.npmjs.org/typescript/-/typescript-7.0.2.tgz",
      "integrity": "sha512-8FYau96o3NKOhbjKi/qNvG/W5jhzxkbdm5sj9AbZ/5T5sWqn3hJgLfGx27sRKZWTvyzCP8dLRBTf5tBTSRVUNA==",
      "dev": true,
      "license": "Apache-2.0",
      "bin": {
        "tsc": "bin/tsc"
      },
      "engines": {
        "node": ">=16.20.0"
      },
      "optionalDependencies": {
        "@typescript/typescript-aix-ppc64": "7.0.2",
        "@typescript/typescript-darwin-arm64": "7.0.2",
        "@typescript/typescript-darwin-x64": "7.0.2",
        "@typescript/typescript-freebsd-arm64": "7.0.2",
        "@typescript/typescript-freebsd-x64": "7.0.2",
        "@typescript/typescript-linux-arm": "7.0.2",
        "@typescript/typescript-linux-arm64": "7.0.2",
        "@typescript/typescript-linux-loong64": "7.0.2",
        "@typescript/typescript-linux-mips64el": "7.0.2",
        "@typescript/typescript-linux-ppc64": "7.0.2",
        "@typescript/typescript-linux-riscv64": "7.0.2",
        "@typescript/typescript-linux-s390x": "7.0.2",
        "@typescript/typescript-linux-x64": "7.0.2",
        "@typescript/typescript-netbsd-arm64": "7.0.2",
        "@typescript/typescript-netbsd-x64": "7.0.2",
        "@typescript/typescript-openbsd-arm64": "7.0.2",
        "@typescript/typescript-openbsd-x64": "7.0.2",
        "@typescript/typescript-sunos-x64": "7.0.2",
        "@typescript/typescript-win32-arm64": "7.0.2",
        "@typescript/typescript-win32-x64": "7.0.2"
      }
    }
  }
}
````

Создать `package.json`:

````json
{
  "name": "ferrum",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "typecheck": "npm --prefix apps/web run typecheck",
    "lint": "oxlint --deny-warnings",
    "format": "prettier --write .",
    "format:check": "prettier --check ."
  },
  "devDependencies": {
    "oxlint": "1.89.0",
    "prettier": "3.9.10",
    "typescript": "7.0.2"
  }
}
````

- [ ] **Step 2: Сгенерировать файлы**

Run: `npm ci`
Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm ci`
Expected: зависимости установлены точно по новым `package-lock.json`

- [ ] **Step 3: Проверить**

Run: `npm run lint`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm && npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Expected: PASS: oxlint без замечаний (`--deny-warnings`); wasm собран; `tsc` 7.0.2 по трём tsconfig без ошибок; модульные — 176 passed. `format:check` на этой задаче ещё падает: форматирование — следующая задача

- [ ] **Step 4: Закоммитить**

```bash
git add .gitignore .npmrc .oxlintrc.json .prettierignore .prettierrc apps/web/e2e/a11y.spec.ts apps/web/e2e/clipboard.spec.ts apps/web/e2e/fallback.spec.ts apps/web/e2e/shared.spec.ts apps/web/package-lock.json apps/web/package.json apps/web/unit/image-size.test.ts apps/web/unit/messages.test.ts apps/web/unit/toasts.test.ts package-lock.json package.json
git commit -F - <<'EOF'
chore: инструменты в корне — TypeScript 7, oxlint, prettier

Корневой package.json с точными версиями; `npm run typecheck`, `lint`,
`format:check`. apps/web переходит на TypeScript 7.0.2, все три tsconfig
проверяются им же. Замечания oxlint на старом коде исправлены, кроме
`no-unassigned-vars`: ссылки SolidJS присваиваются разметкой (`ref={el}`), правило
их не видит.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 2: Форматирование всего репозитория

Спека §3: отдельный коммит только с `prettier --write`. Шаг — команда, содержимое в план не копируется; версия prettier точная, поэтому дерево получается одно и то же (проверено: совпало с прототипом).

- [ ] **Step 1: Выполнить команду**

Run: `npx prettier --write .`
Expected: изменено 90 файлов, только форматирование; дерево совпадает с прототипом

- [ ] **Step 2: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build`

Expected: PASS: `All matched files use Prettier code style!`; lint и typecheck без замечаний; модульные — 176 passed; сборка собрана

- [ ] **Step 3: Закоммитить**

```bash
git add -A
git commit -F - <<'EOF'
style: prettier --write по всему репозиторию

Только форматирование, команда: `npx prettier --write .`

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3: Автономная сборка

Спека §5: флаг времени сборки `VITE_FERRUM_LOCAL=1`, рабочая сборка кода автономного режима не содержит. Все существующие команды остаются автономными, рабочая — новая команда. Десктоп использует флаг.

**Files:**
- Create: `apps/web/.env.standalone`
- Modify: `.gitignore`, `apps/desktop/src-tauri/tauri.conf.json`, `apps/web/package.json`, `apps/web/scripts/check-dist.mjs`, `apps/web/src/main.tsx`, `apps/web/src/vite-env.d.ts`, `apps/web/tsconfig.e2e.json`, `apps/web/vite.config.ts`
- Test: `apps/web/e2e/standalone.spec.ts` (новый), `apps/web/unit/check-dist.test.ts` (новый)

**Interfaces:**
- Consumes: `index.html`, `debug.html`, `scripts/check-dist.mjs`.
- Produces: `npm run build` (`dist/`, автономная), `npm run build:server` (`dist-server/`, рабочая), `npm run dev`, `npm run dev:server`; `checkDist(dist, mode)`, `LOCAL_MARKER`; `document.documentElement.dataset.ferrumLocal`; `build.beforeBuildCommand` у десктопа.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/e2e/standalone.spec.ts`:

````ts
import { expect, test } from '@playwright/test';

// Автономная сборка: сразу экран файлов, без входа и без запросов к серверу.
test('автономная сборка открывается без входа и не ходит на сервер', async ({ page }) => {
  const api: string[] = [];
  page.on('request', (r) => {
    if (new URL(r.url()).pathname.startsWith('/api/')) api.push(r.url());
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await expect(page.getByRole('button', { name: 'Получить ссылку' })).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.dataset.ferrumLocal)).toBe('1');
  expect(api).toEqual([]);
});
````

Создать `apps/web/unit/check-dist.test.ts`:

````ts
import { expect, test } from '@playwright/test';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { checkDist, LOCAL_MARKER } from '../scripts/check-dist.mjs';

function dist(files: Record<string, string>): string {
  const dir = mkdtempSync(join(tmpdir(), 'ferrum-dist-'));
  mkdirSync(join(dir, 'assets'));
  for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, name), text);
  return dir;
}
const INDEX = '<title>Ferrum</title>';
const DEBUG = '<title>Ferrum — стенд ядра</title>';

test('автономная сборка: обе страницы и признак режима', () => {
  expect(checkDist(dist({ 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': `x.${LOCAL_MARKER}=1` }), 'standalone')).toBeNull();
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `x.${LOCAL_MARKER}=1` }), 'standalone')).toContain('debug.html не собран');
  expect(checkDist(dist({ 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': 'x' }), 'standalone')).toContain(
    'нет автономного режима',
  );
});

test('рабочая сборка: без стенда и кода автономного режима', () => {
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': 'x' }), 'server')).toBeNull();
  expect(checkDist(dist({ 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': 'x' }), 'server')).toContain(
    'debug.html в рабочей сборке',
  );
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `x.${LOCAL_MARKER}=1` }), 'server')).toContain(
    'есть код автономного режима',
  );
});

test('десктоп перед сборкой собирает автономный режим', () => {
  const tauri = JSON.parse(readFileSync('../desktop/src-tauri/tauri.conf.json', 'utf8'));
  expect(tauri.build.beforeBuildCommand).toBe('npm --prefix ../web run build');
  expect(tauri.build.frontendDist).toBe('../../web/dist');
  const web = JSON.parse(readFileSync('package.json', 'utf8'));
  expect(web.scripts.build).toContain('--mode standalone');
  expect(web.scripts['build:server']).not.toContain('standalone');
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/check-dist.test.ts`
Expected: FAIL — `SyntaxError: … check-dist.mjs does not provide an export named LOCAL_MARKER`, «No tests found». `e2e/standalone.spec.ts` проверяется в шаге «Проверить»

- [ ] **Step 3: Реализация**

В `.gitignore` заменить:

````
# Стенд apps/web: зависимости, сборка, ядро из wasm-bindgen, отчёты Playwright.
/apps/web/dist/
/apps/web/src/wasm/
/apps/web/test-results/
````

на:

````
# Стенд apps/web: зависимости, сборка, ядро из wasm-bindgen, отчёты Playwright.
/apps/web/dist/
/apps/web/dist-server/
/apps/web/src/wasm/
/apps/web/test-results/
````

В `apps/desktop/src-tauri/tauri.conf.json` заменить:

````json
  "identifier": "app.ferrum.desktop",
  "build": {
    "frontendDist": "../../web/dist"
  },
````

на:

````json
  "identifier": "app.ferrum.desktop",
  "build": {
    "beforeBuildCommand": "npm --prefix ../web run build",
    "frontendDist": "../../web/dist"
  },
````

Создать `apps/web/.env.standalone`:

````
# Автономная сборка (`vite --mode standalone`): редактор без сервера и входа, файлы
# в локальном хранилище. На ней идут E2E и unit, её же собирает десктоп до этапа 7.
VITE_FERRUM_LOCAL=1
````

В `apps/web/package.json` заменить:

````json
  "scripts": {
    "wasm": "node scripts/build-wasm.mjs",
    "dev": "vite",
    "build": "vite build && node scripts/check-dist.mjs",
    "preview": "vite preview",
    "typecheck": "tsc -p tsconfig.json && tsc -p tsconfig.worker.json && tsc -p tsconfig.e2e.json",
````

на:

````json
  "scripts": {
    "wasm": "node scripts/build-wasm.mjs",
    "dev": "vite --mode standalone",
    "dev:server": "vite",
    "build": "vite build --mode standalone && node scripts/check-dist.mjs standalone",
    "build:server": "vite build --outDir dist-server && node scripts/check-dist.mjs server",
    "preview": "vite preview",
    "typecheck": "tsc -p tsconfig.json && tsc -p tsconfig.worker.json && tsc -p tsconfig.e2e.json",
````

В `apps/web/scripts/check-dist.mjs` заменить:

````js
// После `vite build`: обе страницы на месте. Без debug.html в сборке сервер отдаёт
// на его адрес index.html (запасной маршрут), и тесты ядра шли бы не на стенде.
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const dist = fileURLToPath(new URL('../dist/', import.meta.url));
for (const [file, marker] of [
  ['index.html', '<title>Ferrum</title>'],
  ['debug.html', '<title>Ferrum — стенд ядра</title>'],
]) {
  if (!existsSync(dist + file)) {
    console.error(`dist/${file} не собран: проверьте build.rolldownOptions.input в vite.config.ts`);
    process.exit(1);
  }
  if (!readFileSync(dist + file, 'utf8').includes(marker)) {
    console.error(`dist/${file} не та страница (нет ${marker})`);
    process.exit(1);
  }
````

на:

````js
// После `vite build`: сборка того режима, что заказан. Автономная — обе страницы (без
// debug.html сервер отдаёт на его адрес index.html запасным маршрутом, и тесты ядра шли бы
// не на стенде) и признак режима в коде. Рабочая — только редактор и никакого кода
// автономного режима: иначе редактор открывался бы без входа.
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Метка, которую оставляет только ветка автономного режима в `main.tsx`. */
export const LOCAL_MARKER = 'ferrumLocal';

/** Проблема сборки словами или `null`. `mode` — `standalone` или `server`. */
export function checkDist(dist, mode) {
  const pages = [['index.html', '<title>Ferrum</title>']];
  if (mode === 'standalone') pages.push(['debug.html', '<title>Ferrum — стенд ядра</title>']);
  for (const [file, marker] of pages) {
    if (!existsSync(join(dist, file))) return `${file} не собран: проверьте build.rolldownOptions.input в vite.config.ts`;
    if (!readFileSync(join(dist, file), 'utf8').includes(marker)) return `${file} не та страница (нет ${marker})`;
  }
  if (mode === 'server' && existsSync(join(dist, 'debug.html'))) return 'debug.html в рабочей сборке: стенд ядра открывался бы без входа';
  const assets = join(dist, 'assets');
  const code = existsSync(assets) ? readdirSync(assets).filter((f) => f.endsWith('.js')) : [];
  const local = code.some((f) => readFileSync(join(assets, f), 'utf8').includes(LOCAL_MARKER));
  if (mode === 'standalone' && !local) return 'в автономной сборке нет автономного режима: проверьте VITE_FERRUM_LOCAL (.env.standalone)';
  if (mode === 'server' && local) return 'в рабочей сборке есть код автономного режима: VITE_FERRUM_LOCAL задан при сборке?';
  return null;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const mode = process.argv[2];
  if (mode !== 'standalone' && mode !== 'server') {
    console.error('Использование: check-dist.mjs standalone|server');
    process.exit(1);
  }
  const problem = checkDist(fileURLToPath(new URL(mode === 'server' ? '../dist-server/' : '../dist/', import.meta.url)), mode);
  if (problem) {
    console.error(`dist: ${problem}`);
    process.exit(1);
  }
````

В `apps/web/src/main.tsx` заменить:

````tsx
import { runSmoke } from './ui/tauri';

render(() => <Root />, document.getElementById('app')!);
// Десктоп с `FERRUM_SMOKE=1`: проверка запуска (вне Tauri ничего не делает).
````

на:

````tsx
import { runSmoke } from './ui/tauri';

// Условие — прямо по `import.meta.env`, без константы: сборщик выбрасывает ветку целиком,
// `scripts/check-dist.mjs` проверяет, что в рабочей сборке её нет.
if (import.meta.env.VITE_FERRUM_LOCAL === '1') document.documentElement.dataset.ferrumLocal = '1';
render(() => <Root />, document.getElementById('app')!);
// Десктоп с `FERRUM_SMOKE=1`: проверка запуска (вне Tauri ничего не делает).
````

В `apps/web/src/vite-env.d.ts` заменить:

````ts
/// <reference types="vite/client" />
````

на:

````ts
/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** `1` — автономная сборка: редактор без сервера и входа (спека подсистемы 4, раздел 5). */
  readonly VITE_FERRUM_LOCAL?: string;
}
````

В `apps/web/tsconfig.e2e.json` заменить:

````json
  "compilerOptions": {
    "lib": ["ES2024", "DOM"],
    "types": ["node"]
  },
````

на:

````json
  "compilerOptions": {
    "lib": ["ES2024", "DOM"],
    "allowJs": true,
    "types": ["node"]
  },
````

В `apps/web/vite.config.ts` заменить:

````ts
  : {};

// Две страницы: редактор (index.html, интерфейс на SolidJS) и стенд этапа 4
// (debug.html: холст на всё окно и отладочная плашка) — для тестов ядра.
export default defineConfig({
  plugins: [solid()],
  server: { headers },
  preview: { headers },
  worker: { format: 'es' },
  build: {
    target: 'es2022',
    rolldownOptions: { input: { main: 'index.html', debug: 'debug.html' } },
  },
});
````

на:

````ts
  : {};

// Автономная сборка (`--mode standalone`) — две страницы: редактор (index.html) и стенд
// этапа 4 (debug.html: холст на всё окно и отладочная плашка) для тестов ядра. Рабочая
// сборка — только редактор: стенд открывался бы без входа.
export default defineConfig(({ mode }) => {
  const input: Record<string, string> = { main: 'index.html' };
  if (mode === 'standalone') input.debug = 'debug.html';
  return {
    plugins: [solid()],
    server: { headers },
    preview: { headers },
    worker: { format: 'es' as const },
    build: {
      target: 'es2022',
      rolldownOptions: { input },
    },
  };
});
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npx playwright test e2e/standalone.spec.ts --project=chromium`

Expected: PASS: всё без замечаний; модульные — 179 passed; обе сборки собраны, `check-dist.mjs` молчит; E2E `standalone.spec.ts` — 1 passed

- [ ] **Step 5: Закоммитить**

```bash
git add .gitignore apps/desktop/src-tauri/tauri.conf.json apps/web/.env.standalone apps/web/e2e/standalone.spec.ts apps/web/package.json apps/web/scripts/check-dist.mjs apps/web/src/main.tsx apps/web/src/vite-env.d.ts apps/web/tsconfig.e2e.json apps/web/unit/check-dist.test.ts apps/web/vite.config.ts
git commit -F - <<'EOF'
feat(web): автономная сборка под флагом VITE_FERRUM_LOCAL

`npm run build` и `npm run dev` — автономный режим (`--mode standalone`): на нём идут
E2E и unit, его собирает десктоп (`beforeBuildCommand`). `npm run build:server` —
рабочая сборка в dist-server: без стенда ядра и без кода автономного режима, это
проверяет `check-dist.mjs`.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4: Скелет сервера

Спека §3, §4: Bun без фреймворка, общий протокол, миграции, `BlobStore`, `/healthz`, раздача сборки, образ в три стадии, compose, стенд тестов.

**Files:**
- Create: `.dockerignore`, `.env.example`, `Dockerfile`, `apps/server/bun.lock`, `apps/server/bunfig.toml`, `apps/server/migrations/001_init.sql`, `apps/server/package.json`, `apps/server/src/app.ts`, `apps/server/src/blob-store.ts`, `apps/server/src/config.ts`, `apps/server/src/index.ts`, `apps/server/src/migrate.ts`, `apps/server/src/static.ts`, `apps/server/src/valkey.ts`, `apps/server/tsconfig.json`, `docker-compose.test.env`, `docker-compose.test.yml`, `docker-compose.yml`, `packages/protocol/package.json`, `packages/protocol/src/auth.ts`, `packages/protocol/src/index.ts`, `packages/protocol/tsconfig.json`
- Modify: `package.json`
- Test: `apps/server/test/blob-store.test.ts` (новый), `apps/server/test/config.test.ts` (новый), `apps/server/test/helpers.ts` (новый), `apps/server/test/migrate.test.ts` (новый), `apps/server/test/server.test.ts` (новый), `packages/protocol/test/auth.test.ts` (новый)

**Interfaces:**
- Consumes: `npm run build:server` (задача 3), `apps/web/scripts/build-wasm.mjs`.
- Produces: `loadConfig(env): Config`; `migrate(pool, dir): Promise<string[]>`; `BlobStore { put, get, delete }`, `FsBlobStore`; `SECURITY_HEADERS`, `withHeaders`, `serveStatic(req, dist)`; `connectValkey`, `valkeyAlive`; `startServer(deps, port)`; `npm run test:stand`, `test:server`, `test:stand:down`; из `@ferrum/protocol`: `parseEmail`, `parseNext`, `parseSignInRequest`, `parseConfirmRequest`, `parseNameRequest`, `confirmHash`, `parseConfirmHash`, константы `API_*`, `MAGIC_LINK_TTL_SECONDS`, `RESEND_SECONDS`.

- [ ] **Step 1: Написать тесты**

Создать `apps/server/test/blob-store.test.ts`:

````ts
import { expect, test } from 'bun:test';
import { mkdtempSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FsBlobStore } from '../src/blob-store';

const store = () => {
  const dir = mkdtempSync(join(tmpdir(), 'ferrum-blob-'));
  return { dir, s: new FsBlobStore(dir) };
};

test('запись, чтение, замена, удаление', async () => {
  const { s, dir } = store();
  expect(await s.get('files/a')).toBeNull();
  await s.put('files/a', new Uint8Array([1, 2]));
  expect(await s.get('files/a')).toEqual(new Uint8Array([1, 2]));
  await s.put('files/a', new Uint8Array([3]));
  expect(await s.get('files/a')).toEqual(new Uint8Array([3]));
  expect(readdirSync(join(dir, 'files'))).toEqual(['a']);
  await s.delete('files/a');
  expect(await s.get('files/a')).toBeNull();
  await s.delete('files/a');
});

test('ключ не выходит за каталог', async () => {
  const { s } = store();
  for (const key of ['../x', 'a/../../x', '/etc/passwd', '', 'a//b', 'a/', 'a.b']) {
    await expect(s.put(key, new Uint8Array(1))).rejects.toThrow('Неверный ключ');
    await expect(s.get(key)).rejects.toThrow('Неверный ключ');
    await expect(s.delete(key)).rejects.toThrow('Неверный ключ');
  }
});
````

Создать `apps/server/test/config.test.ts`:

````ts
import { expect, test } from 'bun:test';
import { loadConfig } from '../src/config';

const ENV = {
  BETTER_AUTH_SECRET: 's'.repeat(32),
  PUBLIC_URL: 'http://localhost:8090/',
  DATABASE_URL: 'postgres://a/b',
  VALKEY_URL: 'redis://a',
  SMTP_URL: 'smtp://a',
  MAIL_FROM: 'Ferrum <a@b.ru>',
};

test('разбирает окружение: умолчания, список адресов, адрес без косой черты', () => {
  const c = loadConfig({ ...ENV, ALLOWED_EMAILS: ' Ivan@X.ru, y.ru ,,' });
  expect(c.port).toBe(8090);
  expect(c.publicUrl).toBe('http://localhost:8090');
  expect(c.allowedEmails).toEqual(['ivan@x.ru', 'y.ru']);
  expect(c.trustProxy).toBe(false);
  expect(loadConfig({ ...ENV, TRUST_PROXY: '1', FERRUM_PORT: '0' })).toMatchObject({ trustProxy: true, port: 0 });
  expect(loadConfig(ENV).allowedEmails).toEqual([]);
});

test('неверные настройки: отказ со всем списком сразу', () => {
  expect(() => loadConfig({})).toThrow(
    /BETTER_AUTH_SECRET не задан.*PUBLIC_URL не задан.*DATABASE_URL не задан.*VALKEY_URL не задан.*SMTP_URL не задан.*MAIL_FROM не задан/,
  );
  expect(() => loadConfig({ ...ENV, BETTER_AUTH_SECRET: 'short' })).toThrow('короче 32');
  expect(() => loadConfig({ ...ENV, PUBLIC_URL: 'http://x/path' })).toThrow('без пути');
  expect(() => loadConfig({ ...ENV, FERRUM_PORT: 'abc' })).toThrow('неверный порт');
});
````

Создать `apps/server/test/helpers.ts`:

````ts
// Общее для тестов сервера: настоящие Postgres и Valkey из `docker-compose.test.yml`
// (`npm run test:server` поднимает их сам). База и Valkey — только тестовые: перед каждым
// тестом очищаются целиком.

import { Pool } from 'pg';

export const TEST_DATABASE_URL = process.env.TEST_DATABASE_URL ?? 'postgres://ferrum:ferrum@127.0.0.1:15432/ferrum';
export const TEST_VALKEY_URL = process.env.TEST_VALKEY_URL ?? 'redis://127.0.0.1:16379';

export function testPool(): Pool {
  return new Pool({ connectionString: TEST_DATABASE_URL });
}

export async function resetDatabase(pool: Pool): Promise<void> {
  await pool.query('DROP SCHEMA public CASCADE; CREATE SCHEMA public;');
}

export async function resetValkey(client: Bun.RedisClient): Promise<void> {
  await client.send('FLUSHDB', []);
}
````

Создать `apps/server/test/migrate.test.ts`:

````ts
import { afterAll, beforeEach, expect, test } from 'bun:test';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { migrate } from '../src/migrate';
import { resetDatabase, testPool } from './helpers';

const pool = testPool();
beforeEach(() => resetDatabase(pool));
afterAll(() => pool.end());

function dir(files: Record<string, string>): string {
  const d = mkdtempSync(join(tmpdir(), 'ferrum-migrate-'));
  for (const [name, sql] of Object.entries(files)) writeFileSync(join(d, name), sql);
  return d;
}

test('применяет по порядку и только один раз', async () => {
  const d = dir({ '002_b.sql': 'INSERT INTO a VALUES (2);', '001_a.sql': 'CREATE TABLE a (v int); INSERT INTO a VALUES (1);' });
  expect(await migrate(pool, d)).toEqual(['001_a.sql', '002_b.sql']);
  expect(await migrate(pool, d)).toEqual([]);
  expect((await pool.query('SELECT v FROM a ORDER BY v')).rows).toEqual([{ v: 1 }, { v: 2 }]);
});

test('упавшая миграция откатывается целиком, следующие не идут', async () => {
  const d = dir({
    '001_a.sql': 'CREATE TABLE a (v int);',
    '002_b.sql': 'CREATE TABLE b (v int); SELECT nonsense;',
    '003_c.sql': 'CREATE TABLE c (v int);',
  });
  await expect(migrate(pool, d)).rejects.toThrow('«002_b.sql» не применилась');
  const tables = (await pool.query("SELECT table_name AS t FROM information_schema.tables WHERE table_schema = 'public'")).rows.map(
    (r) => r.t,
  );
  expect(tables).toContain('a');
  expect(tables).not.toContain('b');
  expect(tables).not.toContain('c');
  expect((await pool.query('SELECT name FROM schema_migration')).rows).toEqual([{ name: '001_a.sql' }]);
});

test('два процесса одновременно: блокировка не даёт применить дважды', async () => {
  const d = dir({ '001_a.sql': 'CREATE TABLE a (v int); SELECT pg_sleep(0.2);', '002_b.sql': 'INSERT INTO a VALUES (1);' });
  const [x, y] = await Promise.all([migrate(pool, d), migrate(pool, d)]);
  expect([...x, ...y].sort()).toEqual(['001_a.sql', '002_b.sql']);
  expect((await pool.query('SELECT count(*)::int AS n FROM a')).rows[0].n).toBe(1);
});

test('имя без номера — отказ до всякой записи', async () => {
  await expect(migrate(pool, dir({ 'init.sql': 'SELECT 1;' }))).rejects.toThrow('нужно имя вида');
});

test('миграции репозитория применяются на пустой базе', async () => {
  const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
  expect(applied[0]).toBe('001_init.sql');
});
````

Создать `apps/server/test/server.test.ts`:

````ts
import { afterAll, expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Pool } from 'pg';
import { startServer } from '../src/app';
import { connectValkey } from '../src/valkey';
import { TEST_VALKEY_URL, testPool } from './helpers';

const web = mkdtempSync(join(tmpdir(), 'ferrum-web-'));
mkdirSync(join(web, 'assets'));
writeFileSync(join(web, 'index.html'), '<title>Ferrum</title>');
writeFileSync(join(web, 'assets/main-abc.js'), 'export {}');
writeFileSync(join(web, 'assets/core.wasm'), new Uint8Array([0, 97, 115, 109]));

const pool = testPool();
const valkey = connectValkey(TEST_VALKEY_URL);
const server = startServer({ pool, valkey, webDist: web }, 0);
const url = (p: string) => `http://127.0.0.1:${server.port}${p}`;
afterAll(async () => {
  server.stop(true);
  valkey.close();
  await pool.end();
});

function expectIsolated(res: Response) {
  expect(res.headers.get('cross-origin-opener-policy')).toBe('same-origin');
  expect(res.headers.get('cross-origin-embedder-policy')).toBe('credentialless');
  expect(res.headers.get('x-content-type-options')).toBe('nosniff');
}

test('/healthz: база и Valkey живы', async () => {
  const res = await fetch(url('/healthz'));
  expect(res.status).toBe(200);
  expect(await res.json()).toEqual({ ok: true, postgres: true, valkey: true });
  expectIsolated(res);
});

test('/healthz: база недоступна — 503 и что именно упало', async () => {
  const dead = new Pool({ connectionString: 'postgres://ferrum:ferrum@127.0.0.1:1/ferrum', connectionTimeoutMillis: 200 });
  dead.on('error', () => {});
  const s = startServer({ pool: dead, valkey, webDist: web }, 0);
  const res = await fetch(`http://127.0.0.1:${s.port}/healthz`);
  expect(res.status).toBe(503);
  expect(await res.json()).toEqual({ ok: false, postgres: false, valkey: true });
  s.stop(true);
  await dead.end();
});

test('статика: корень — index.html, изоляция, CSP, кэш по виду файла', async () => {
  const index = await fetch(url('/'));
  expect(index.status).toBe(200);
  expect(await index.text()).toContain('<title>Ferrum</title>');
  expectIsolated(index);
  expect(index.headers.get('cache-control')).toBe('no-cache');
  expect(index.headers.get('content-security-policy')).toContain("script-src 'self' 'wasm-unsafe-eval'");
  const asset = await fetch(url('/assets/main-abc.js'));
  expect(asset.headers.get('cache-control')).toContain('immutable');
  expect(asset.headers.get('content-type')).toContain('javascript');
  const wasm = await fetch(url('/assets/core.wasm'));
  expect(wasm.headers.get('content-type')).toBe('application/wasm');
  expectIsolated(wasm);
});

test('статика: чужой адрес — 404, за каталог не выйти, запись запрещена', async () => {
  expect((await fetch(url('/nope'))).status).toBe(404);
  expect((await fetch(url('/%2e%2e/%2e%2e/etc/passwd'))).status).toBe(404);
  expect((await fetch(url('/..%2f..%2fetc%2fpasswd'))).status).toBe(404);
  const post = await fetch(url('/index.html'), { method: 'POST' });
  expect(post.status).toBe(405);
  const head = await fetch(url('/'), { method: 'HEAD' });
  expect(head.status).toBe(200);
  expect(await head.text()).toBe('');
});
````

Создать `packages/protocol/test/auth.test.ts`:

````ts
import { expect, test } from 'bun:test';
import {
  confirmHash,
  MAX_NAME_CHARS,
  parseConfirmHash,
  parseConfirmRequest,
  parseEmail,
  parseNameRequest,
  parseNext,
  parseSignInRequest,
} from '../src/index';

const TOKEN = 'a'.repeat(32);

test('адрес приводится к нижнему регистру, мусор отвергается', () => {
  expect(parseEmail('  Ivan@Example.COM ')).toBe('ivan@example.com');
  for (const bad of ['', 'ivan', 'ivan@', '@x.ru', 'a b@x.ru', 'a@x', 5, null, `${'a'.repeat(250)}@x.ru`, 'a\n@x.ru']) {
    expect(parseEmail(bad)).toBeNull();
  }
});

test('возврат после входа: только маршрут приложения', () => {
  expect(parseNext('#/file/abc?page=1@2')).toBe('#/file/abc?page=1@2');
  for (const bad of [
    'https://evil.example',
    '//evil.example',
    '/file/x',
    '#file',
    '#/auth/confirm?token=x',
    `#/${'a'.repeat(600)}`,
    7,
    '#/a\nb',
  ]) {
    expect(parseNext(bad)).toBeNull();
  }
});

test('запрос ссылки: лишние поля не проходят дальше, неверный возврат — отказ', () => {
  expect(parseSignInRequest({ email: 'A@x.ru', callbackURL: 'https://evil.example', name: 'x' })).toEqual({ email: 'a@x.ru' });
  expect(parseSignInRequest({ email: 'a@x.ru', next: '#/file/1' })).toEqual({ email: 'a@x.ru', next: '#/file/1' });
  expect(parseSignInRequest({ email: 'a@x.ru', next: 'https://evil.example' })).toBeNull();
  expect(parseSignInRequest([])).toBeNull();
  expect(parseSignInRequest(null)).toBeNull();
});

test('подтверждение: токен из 32 букв', () => {
  expect(parseConfirmRequest({ token: TOKEN })).toEqual({ token: TOKEN });
  for (const bad of [{ token: 'a' }, { token: `${TOKEN}!` }, { token: 5 }, {}, 'x']) expect(parseConfirmRequest(bad)).toBeNull();
});

test('имя: пробелы убираются, пустое и длинное не годятся', () => {
  expect(parseNameRequest({ name: '  Иван  ' })).toEqual({ name: 'Иван' });
  expect(parseNameRequest({ name: 'я'.repeat(MAX_NAME_CHARS) })).not.toBeNull();
  for (const bad of [{ name: '   ' }, { name: 'я'.repeat(MAX_NAME_CHARS + 1) }, { name: 'a\u0007' }, { name: 1 }, {}]) {
    expect(parseNameRequest(bad)).toBeNull();
  }
});

test('адрес подтверждения: туда и обратно, чужое — не он', () => {
  const hash = confirmHash(TOKEN, '#/file/abc?page=1@2');
  expect(parseConfirmHash(hash)).toEqual({ token: TOKEN, next: '#/file/abc?page=1@2' });
  expect(parseConfirmHash(confirmHash(TOKEN))).toEqual({ token: TOKEN, next: null });
  expect(parseConfirmHash('#/auth/confirm?token=short')).toEqual({ token: null, next: null });
  expect(parseConfirmHash('#/file/1')).toBeNull();
  expect(parseConfirmHash('')).toBeNull();
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd packages/protocol && bun test`
Run: `cd apps/server && bun test`
Expected: FAIL — `packages/protocol` — `Cannot find module ../src/index`, 0 pass, 1 fail; `apps/server` — `Cannot find module` для `../src/config`, `../src/migrate`, `../src/blob-store` и `Cannot find package pg`, 0 pass, 4 fail

- [ ] **Step 3: Реализация**

Создать `.dockerignore`:

````
**/node_modules
**/dist
**/dist-server
target
docs
.git
.env
.superpowers
apps/web/src/wasm
apps/web/test-results
apps/web/playwright-report
apps/desktop
````

Создать `.env.example`:

````
# Образец настроек: скопируйте в .env и задайте секрет.
# Секрет подписи сессий, не короче 32 символов: `openssl rand -base64 32`
BETTER_AUTH_SECRET=
# Адрес, с которого пользователи открывают Ferrum: из него строятся ссылки в письмах и trustedOrigins.
PUBLIC_URL=http://localhost:8090
# Порт сервера снаружи контейнера.
FERRUM_PORT=8090
# Пароль Postgres; DATABASE_URL внутри сети compose собирается из него.
POSTGRES_PASSWORD=ferrum
# Почта. По умолчанию письма перехватывает Mailpit (http://localhost:8025, профиль dev).
SMTP_URL=smtp://mailpit:1025
MAIL_FROM=Ferrum <ferrum@localhost>
# Профили compose: `dev` включает Mailpit. На рабочем сервере уберите и задайте настоящий SMTP_URL.
COMPOSE_PROFILES=dev
# Кто может войти: адреса и домены через запятую (ivan@example.com,example.org). Пусто — любой.
ALLOWED_EMAILS=
# 1 — сервер за обратным прокси: адрес клиента берётся из X-Forwarded-For.
TRUST_PROXY=0
````

Создать `Dockerfile`:

````
# Образ сервера, три стадии: ядро (Rust и wasm), интерфейс (apps/web), сервер (Bun).

FROM rust:1.96-slim AS wasm
RUN apt-get update && apt-get install -y --no-install-recommends nodejs && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY scripts scripts
COPY crates crates
COPY apps/web/scripts/build-wasm.mjs apps/web/scripts/build-wasm.mjs
RUN cargo install wasm-bindgen-cli --version 0.2.129 --locked
RUN node apps/web/scripts/build-wasm.mjs

FROM node:24-slim AS web
WORKDIR /src
COPY packages/protocol packages/protocol
COPY apps/web apps/web
COPY --from=wasm /src/apps/web/src/wasm apps/web/src/wasm
WORKDIR /src/apps/web
RUN npm ci && npm run build:server

FROM oven/bun:1.4.2-alpine
WORKDIR /app
COPY packages/protocol packages/protocol
COPY apps/server/package.json apps/server/bun.lock apps/server/bunfig.toml apps/server/
COPY apps/server/src apps/server/src
COPY apps/server/migrations apps/server/migrations
WORKDIR /app/apps/server
RUN bun install --frozen-lockfile --production
COPY --from=web /src/apps/web/dist-server /app/web
RUN mkdir -p /data && chown bun:bun /data
USER bun
ENV FERRUM_PORT=8090 FERRUM_DATA_DIR=/data FERRUM_WEB_DIST=/app/web
EXPOSE 8090
VOLUME /data
CMD ["bun", "run", "src/index.ts"]
````

Создать `apps/server/bun.lock`:

````
{
  "lockfileVersion": 2,
  "configVersion": 1,
  "workspaces": {
    "": {
      "name": "ferrum-server",
      "dependencies": {
        "@ferrum/protocol": "file:../../packages/protocol",
        "pg": "8.23.1",
      },
      "devDependencies": {
        "@types/bun": "1.4.3",
        "@types/pg": "8.23.1",
      },
    },
  },
  "packages": {
    "@ferrum/protocol": ["@ferrum/protocol@file:../../packages/protocol", {}],

    "@types/bun": ["@types/bun@1.4.3", "", { "dependencies": { "bun-types": "1.4.3" } }, "sha512-N67Pqy8JXbAY/j0tNraS3V5mubGvc6rrrNDmDCtZLlFzccbUJPDb0tvgUBC5q4m8ybkP93Jcn1QB+RYQ6PvBhw=="],

    "@types/node": ["@types/node@26.6.6", "", { "dependencies": { "undici-types": "~8.9.0" } }, "sha512-MzQj+8Kx3XI40OmUAE6Qoh58ZMNstNNMwn2nfd3qWsfe7nDNSqJNzes4njhz6AHwrSHOffSLwXU6yb474Q4aog=="],

    "@types/pg": ["@types/pg@8.23.1", "", { "dependencies": { "@types/node": "*", "pg-protocol": "*", "pg-types": "^2.2.0" } }, "sha512-fKVHpikPdg4GKks3JuLEhvwSyvwzF23hnabPy6DD8ljVbC7+6J5dQzdv4arV6jqq57djnMgs1HKBxX4P8aBI3A=="],

    "bun-types": ["bun-types@1.4.3", "", { "dependencies": { "@types/node": "*", "undici-types": "*" } }, "sha512-tinRJNGOmUN+tEAsn2y2e942n9JHRHR42iUYzDH75NmhJqVBokGSQkkfSFdNWwsANIdrK6ACH9t01M452YyTwg=="],

    "pg": ["pg@8.23.1", "", { "dependencies": { "pg-connection-string": "^2.14.1", "pg-pool": "^3.14.0", "pg-protocol": "^1.16.1", "pg-types": "2.2.0", "pgpass": "1.0.5" }, "optionalDependencies": { "pg-cloudflare": "^1.4.1" }, "peerDependencies": { "pg-native": ">=3.0.1" }, "optionalPeers": ["pg-native"] }, "sha512-aL96AHANtWjPLDOLqnhx+ngp9+UK7ETEU8VJrDCGvsSSi/mGLcWYsS6Herg7lmaBJe4uwrfqsa7gTEFaSizDoQ=="],

    "pg-cloudflare": ["pg-cloudflare@1.4.1", "", {}, "sha512-6PQbsFWZcp9EmJEwy5cGQ2La+AMWpP46lgbb8X+U/XsHIUweYDNCpeuKck5RxL2MdVFi7krbbEi5nX4Zh7JhrQ=="],

    "pg-connection-string": ["pg-connection-string@2.14.1", "", {}, "sha512-qR3kGNPBLpCNtz0evbKA0Y/MRFXwSSdT+pTJvYp/bXTcReZbvX1kzF0IyTc1QnxqF7AZbOeBhNL8R5mYQZV/MA=="],

    "pg-int8": ["pg-int8@1.0.1", "", {}, "sha512-WCtabS6t3c8SkpDBUlb1kjOs7l66xsGdKpIPZsg4wR+B3+u9UAum2odSsF9tnvxg80h4ZxLWMy4pRjOsFIqQpw=="],

    "pg-pool": ["pg-pool@3.14.0", "", { "peerDependencies": { "pg": ">=8.0" } }, "sha512-gKtPkFdQPU3DksooVLi9LsjZxrsBUZIpa+7aVx+LV5pNh0KzP4Zleud2po+ConrxbuXGBJ6Hfer6hdgpIBpBaw=="],

    "pg-protocol": ["pg-protocol@1.16.1", "", {}, "sha512-p9VOFMiHB/ZbJATetbg+99PxssTVSQRnyuPSQ67mN1+1KBOjZaZ83ZQzltnxPhJwSsC3nwVjJ10DVJlerbFzLg=="],

    "pg-types": ["pg-types@2.2.0", "", { "dependencies": { "pg-int8": "1.0.1", "postgres-array": "~2.0.0", "postgres-bytea": "~1.0.0", "postgres-date": "~1.0.4", "postgres-interval": "^1.1.0" } }, "sha512-qTAAlrEsl8s4OiEQY69wDvcMIdQN6wdz5ojQiOy6YRMuynxenON0O5oCpJI6lshc6scgAY8qvJ2On/p+CXY0GA=="],

    "pgpass": ["pgpass@1.0.5", "", { "dependencies": { "split2": "^4.1.0" } }, "sha512-FdW9r/jQZhSeohs1Z3sI1yxFQNFvMcnmfuj4WBMUTxOrAyLMaTcE1aAMBiTlbMNaXvBCQuVi0R7hd8udDSP7ug=="],

    "postgres-array": ["postgres-array@2.0.0", "", {}, "sha512-VpZrUqU5A69eQyW2c5CA1jtLecCsN2U/bD6VilrFDWq5+5UIEVO7nazS3TEcHf1zuPYO/sqGvUvW62g86RXZuA=="],

    "postgres-bytea": ["postgres-bytea@1.0.1", "", {}, "sha512-5+5HqXnsZPE65IJZSMkZtURARZelel2oXUEO8rH83VS/hxH5vv1uHquPg5wZs8yMAfdv971IU+kcPUczi7NVBQ=="],

    "postgres-date": ["postgres-date@1.0.7", "", {}, "sha512-suDmjLVQg78nMK2UZ454hAG+OAW+HQPZ6n++TNDUX+L0+uUlLywnoxJKDou51Zm+zTCjrCl0Nq6J9C5hP9vK/Q=="],

    "postgres-interval": ["postgres-interval@1.2.0", "", { "dependencies": { "xtend": "^4.0.0" } }, "sha512-9ZhXKM/rw350N1ovuWHbGxnGh/SNJ4cnxHiM0rxE4VN41wsg8P8zWn9hv/buK00RP4WvlOyr/RBDiptyxVbkZQ=="],

    "split2": ["split2@4.2.0", "", {}, "sha512-UcjcJOWknrNkF6PLX83qcHM6KHgVKNkV62Y8a5uYDVv9ydGQVwAHMKqHdJje1VTWpljG0WYpCDhrCdAOYH4TWg=="],

    "undici-types": ["undici-types@8.12.3", "", {}, "sha512-EXE4ydL3S76dFb27UFjNVZHTyv/TLdATj/KUe29ptdcg6qXL48J2tZ3126E5kICj5jn781OR+XlVH1N6Mz8jbA=="],

    "xtend": ["xtend@4.0.2", "", {}, "sha512-LKYU1iAXJXUgAXn9URjiu+MWhyUXHsvfp7mcuYm9dSUKK0/CjtrUwFAxD82/mCWbtLsGjFIad0wIsod4zrTAEQ=="],

    "@types/node/undici-types": ["undici-types@8.9.0", "", {}, "sha512-KTDyRTYX8sWmKXAikPHHSyc63CRPETMctyjKFupcC6OBLXT3xsN0e9aF7m+mIXutFWpUXuedtowG7iLOzp0kQg=="],
  }
}
````

Создать `apps/server/bunfig.toml`:

````toml
[test]
# Тесты делят одну базу и один Valkey: файлы идут по очереди, тесты в файле — тоже.
timeout = 30000
````

Создать `apps/server/migrations/001_init.sql`:

````sql
-- Начало журнала миграций. Таблицы входа — в следующей миграции, файлы — в этапе 2.
SELECT 1;
````

Создать `apps/server/package.json`:

````json
{
  "name": "ferrum-server",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "dependencies": {
    "@ferrum/protocol": "file:../../packages/protocol",
    "pg": "8.23.1"
  },
  "devDependencies": {
    "@types/bun": "1.4.3",
    "@types/pg": "8.23.1"
  }
}
````

Создать `apps/server/src/app.ts`:

````ts
// HTTP-сервер: `Bun.serve` с маршрутами, без веб-фреймворка. Все зависимости — параметрами:
// тесты поднимают настоящий сервер на свободном порту против настоящих Postgres и Valkey.

import type { Pool } from 'pg';
import { serveStatic, withHeaders } from './static';
import { valkeyAlive } from './valkey';

export interface AppDeps {
  pool: Pool;
  valkey: Bun.RedisClient;
  webDist: string;
}

/** Проверка не должна висеть дольше проверки контейнера. */
const HEALTH_TIMEOUT_MS = 2000;

function within(p: Promise<boolean>): Promise<boolean> {
  return Promise.race([p.catch(() => false), new Promise<boolean>((r) => setTimeout(r, HEALTH_TIMEOUT_MS, false))]);
}

async function health(deps: AppDeps): Promise<Response> {
  const [postgres, valkey] = await Promise.all([within(deps.pool.query('SELECT 1').then(() => true)), within(valkeyAlive(deps.valkey))]);
  return Response.json(
    { ok: postgres && valkey, postgres, valkey },
    { status: postgres && valkey ? 200 : 503, headers: { 'Cache-Control': 'no-store' } },
  );
}

export function startServer(deps: AppDeps, port: number): Bun.Server<undefined> {
  return Bun.serve({
    port,
    routes: {
      '/healthz': async () => withHeaders(await health(deps)),
    },
    fetch: async (req) => withHeaders(await serveStatic(req, deps.webDist)),
    error: (e) => {
      console.error(e);
      return withHeaders(new Response('Внутренняя ошибка сервера', { status: 500 }));
    },
  });
}
````

Создать `apps/server/src/blob-store.ts`:

````ts
// Байты снимков, журналов-архивов и картинок (спека, раздел 4). Переход на S3 — новая
// реализация этого интерфейса.

import { mkdir, rename, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';

export interface BlobStore {
  put(key: string, bytes: Uint8Array): Promise<void>;
  /** Нет такого ключа — `null`. */
  get(key: string): Promise<Uint8Array | null>;
  /** Нет такого ключа — не ошибка. */
  delete(key: string): Promise<void>;
}

/** Ключ — имена через `/`: путь к файлу не выйдет за каталог хранилища. */
const KEY_RE = /^[A-Za-z0-9_-]+(\/[A-Za-z0-9_-]+)*$/;

export class FsBlobStore implements BlobStore {
  private seq = 0;

  constructor(private readonly dir: string) {}

  private path(key: string): string {
    if (!KEY_RE.test(key)) throw new Error(`Неверный ключ хранилища: «${key}»`);
    return join(this.dir, key);
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

  async delete(key: string): Promise<void> {
    await rm(this.path(key), { force: true });
  }
}
````

Создать `apps/server/src/config.ts`:

````ts
// Настройки сервера из окружения (`.env`, образец — `.env.example`). Неверное — отказ при
// запуске со всем списком сразу, а не ошибка на первом запросе.

export interface Config {
  port: number;
  publicUrl: string;
  authSecret: string;
  databaseUrl: string;
  valkeyUrl: string;
  smtpUrl: string;
  mailFrom: string;
  /** Адреса (`ivan@x.ru`) и домены (`x.ru`) в нижнем регистре; пусто — войти может любой. */
  allowedEmails: string[];
  /** Том байтов: снимки, журналы-архивы, картинки. */
  dataDir: string;
  /** Собранный `apps/web`. */
  webDist: string;
  /** За обратным прокси: адрес клиента из `X-Forwarded-For`, иначе — адрес соединения. */
  trustProxy: boolean;
}

const MIN_SECRET_CHARS = 32;

export function loadConfig(env: Record<string, string | undefined> = process.env): Config {
  const problems: string[] = [];
  const need = (name: string): string => {
    const v = env[name]?.trim();
    if (!v) problems.push(`${name} не задан`);
    return v ?? '';
  };
  const authSecret = need('BETTER_AUTH_SECRET');
  if (authSecret && authSecret.length < MIN_SECRET_CHARS) problems.push(`BETTER_AUTH_SECRET короче ${MIN_SECRET_CHARS} символов`);
  const publicUrl = need('PUBLIC_URL').replace(/\/+$/, '');
  if (publicUrl && !/^https?:\/\/[^/]+$/.test(publicUrl)) problems.push('PUBLIC_URL должен быть вида https://адрес, без пути');
  const port = Number(env.FERRUM_PORT ?? 8090);
  if (!Number.isInteger(port) || port < 0 || port > 65535) problems.push('FERRUM_PORT — неверный порт');
  const config: Config = {
    port,
    publicUrl,
    authSecret,
    databaseUrl: need('DATABASE_URL'),
    valkeyUrl: need('VALKEY_URL'),
    smtpUrl: need('SMTP_URL'),
    mailFrom: need('MAIL_FROM'),
    allowedEmails: (env.ALLOWED_EMAILS ?? '')
      .split(',')
      .map((s) => s.trim().toLowerCase())
      .filter(Boolean),
    dataDir: env.FERRUM_DATA_DIR ?? '/data',
    webDist: env.FERRUM_WEB_DIST ?? '/app/web',
    trustProxy: env.TRUST_PROXY === '1',
  };
  if (problems.length > 0) throw new Error(`Настройки сервера: ${problems.join('; ')}`);
  return config;
}
````

Создать `apps/server/src/index.ts`:

````ts
// Запуск сервера: настройки, миграции под блокировкой, затем приём запросов.

import { join } from 'node:path';
import { Pool } from 'pg';
import { startServer } from './app';
import { FsBlobStore } from './blob-store';
import { loadConfig } from './config';
import { migrate } from './migrate';
import { connectValkey } from './valkey';

const config = loadConfig();
const pool = new Pool({ connectionString: config.databaseUrl });
const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
if (applied.length > 0) console.log(`Миграции применены: ${applied.join(', ')}`);
const valkey = connectValkey(config.valkeyUrl);
// Хранилище байтов нужно с этапа 2; здесь — проверка, что том доступен для записи.
await new FsBlobStore(config.dataDir).put('healthcheck', new Uint8Array([1]));
const server = startServer({ pool, valkey, webDist: config.webDist }, config.port);
console.log(`Ferrum слушает порт ${server.port}`);
````

Создать `apps/server/src/migrate.ts`:

````ts
// Миграции: нумерованные SQL-файлы (`001_имя.sql`) применяются по порядку при запуске, каждый
// в своей транзакции, под блокировкой Postgres: два процесса не применят один файл дважды.

import { readdir } from 'node:fs/promises';
import { join } from 'node:path';
import type { Pool } from 'pg';

/** Номер блокировки `pg_advisory_lock`: любое постоянное число, своё у миграций. */
const LOCK = 7_204_911;
const NAME_RE = /^\d{3}_[a-z0-9_]+\.sql$/;

export async function migrate(pool: Pool, dir: string): Promise<string[]> {
  const files = (await readdir(dir)).filter((f) => f.endsWith('.sql')).sort();
  for (const f of files) if (!NAME_RE.test(f)) throw new Error(`Миграция «${f}»: нужно имя вида 001_имя.sql`);
  const client = await pool.connect();
  try {
    await client.query('SELECT pg_advisory_lock($1)', [LOCK]);
    await client.query(
      'CREATE TABLE IF NOT EXISTS schema_migration (name text PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now())',
    );
    const done = new Set((await client.query<{ name: string }>('SELECT name FROM schema_migration')).rows.map((r) => r.name));
    const applied: string[] = [];
    for (const f of files) {
      if (done.has(f)) continue;
      await client.query('BEGIN');
      try {
        await client.query(await Bun.file(join(dir, f)).text());
        await client.query('INSERT INTO schema_migration (name) VALUES ($1)', [f]);
        await client.query('COMMIT');
      } catch (e) {
        await client.query('ROLLBACK');
        throw new Error(`Миграция «${f}» не применилась: ${e instanceof Error ? e.message : String(e)}`);
      }
      applied.push(f);
    }
    return applied;
  } finally {
    await client.query('SELECT pg_advisory_unlock($1)', [LOCK]).catch(() => {});
    client.release();
  }
}
````

Создать `apps/server/src/static.ts`:

````ts
// Раздача собранного `apps/web` и заголовки, общие для всех ответов сервера. Заголовки
// изоляции нужны для `SharedArrayBuffer`, как у `vite preview`; файлы с хэшем в имени
// (`assets/`) кэшируются навсегда, остальное — с проверкой.

import { resolve, sep } from 'node:path';

export const SECURITY_HEADERS: Record<string, string> = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'credentialless',
  'X-Content-Type-Options': 'nosniff',
  'Referrer-Policy': 'same-origin',
};

/** Как в окне десктопа (`tauri.conf.json`): только свои скрипты, wasm и worker'ы. */
const CSP = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "worker-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' blob: data:",
  "font-src 'self' data:",
  "connect-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'self'",
  "frame-ancestors 'none'",
].join('; ');

export function withHeaders(res: Response): Response {
  for (const [k, v] of Object.entries(SECURITY_HEADERS)) res.headers.set(k, v);
  return res;
}

export async function serveStatic(req: Request, dist: string): Promise<Response> {
  if (req.method !== 'GET' && req.method !== 'HEAD')
    return new Response('Метод не разрешён', { status: 405, headers: { Allow: 'GET, HEAD' } });
  let path: string;
  try {
    path = decodeURIComponent(new URL(req.url).pathname);
  } catch {
    return new Response('Неверный адрес', { status: 400 });
  }
  const root = resolve(dist);
  const full = resolve(root, `.${path === '/' ? '/index.html' : path}`);
  if (!full.startsWith(root + sep)) return new Response('Не найдено', { status: 404 });
  const file = Bun.file(full);
  // Маршруты приложения — после `#`: запасной index.html не нужен, чужой адрес — честный 404.
  if (!(await file.exists())) return new Response('Не найдено', { status: 404 });
  const headers: Record<string, string> = {
    'Cache-Control': path.startsWith('/assets/') ? 'public, max-age=31536000, immutable' : 'no-cache',
    'Content-Security-Policy': CSP,
  };
  return new Response(req.method === 'HEAD' ? null : file, {
    headers: { 'Content-Type': file.type, 'Content-Length': String(file.size), ...headers },
  });
}
````

Создать `apps/server/src/valkey.ts`:

````ts
// Клиент Valkey: один на сервер, встроенный `Bun.RedisClient`.

export function connectValkey(url: string): Bun.RedisClient {
  return new Bun.RedisClient(url);
}

/** Valkey отвечает на `PING`. */
export async function valkeyAlive(client: Bun.RedisClient): Promise<boolean> {
  return (await client.send('PING', [])) === 'PONG';
}
````

Создать `apps/server/tsconfig.json`:

````json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2024"],
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "types": ["bun"]
  },
  "include": ["src/**/*.ts", "test/**/*.ts", "../../packages/protocol/test/**/*.ts"]
}
````

Создать `docker-compose.test.env`:

````
# Настройки стенда тестов: не секрет и не годятся для настоящего развёртывания.
BETTER_AUTH_SECRET=ferrum-test-secret-ferrum-test-secret-0123456789
PUBLIC_URL=http://localhost:18090
COMPOSE_PROFILES=dev
````

Создать `docker-compose.test.yml`:

````yaml
# Стенд тестов поверх docker-compose.yml, свой проект и порты, данные не сохраняются:
# docker compose -p ferrum-test --env-file docker-compose.test.env -f docker-compose.yml -f docker-compose.test.yml up -d --wait
services:
  server:
    ports: !override
      - '127.0.0.1:18090:8090'
    volumes: !override []
    tmpfs:
      - /data:uid=1000,gid=1000
    restart: 'no'

  postgres:
    ports:
      - '127.0.0.1:15432:5432'
    volumes: !override []
    tmpfs:
      - /var/lib/postgresql
    restart: 'no'

  valkey:
    ports:
      - '127.0.0.1:16379:6379'
    command: [valkey-server, --save, '']
    volumes: !override []
    restart: 'no'

  mailpit:
    ports: !override
      - '127.0.0.1:18025:8025'
    restart: 'no'
````

Создать `docker-compose.yml`:

````yaml
# Ferrum одной командой: `cp .env.example .env && docker compose up --build`.
name: ferrum

services:
  server:
    build: .
    restart: unless-stopped
    ports:
      - '${FERRUM_PORT:-8090}:8090'
    environment:
      BETTER_AUTH_SECRET: ${BETTER_AUTH_SECRET:?задайте BETTER_AUTH_SECRET в .env, образец — .env.example}
      PUBLIC_URL: ${PUBLIC_URL:-http://localhost:8090}
      DATABASE_URL: postgres://ferrum:${POSTGRES_PASSWORD:-ferrum}@postgres:5432/ferrum
      VALKEY_URL: redis://valkey:6379
      SMTP_URL: ${SMTP_URL:-smtp://mailpit:1025}
      MAIL_FROM: ${MAIL_FROM:-Ferrum <ferrum@localhost>}
      ALLOWED_EMAILS: ${ALLOWED_EMAILS:-}
      TRUST_PROXY: ${TRUST_PROXY:-0}
    volumes:
      - ferrum-data:/data
    depends_on:
      postgres:
        condition: service_healthy
      valkey:
        condition: service_healthy
    healthcheck:
      test: [CMD, bun, -e, "const r = await fetch('http://127.0.0.1:8090/healthz'); process.exit(r.ok ? 0 : 1)"]
      interval: 5s
      timeout: 5s
      retries: 12

  postgres:
    image: postgres:18-alpine
    restart: unless-stopped
    environment:
      POSTGRES_USER: ferrum
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:-ferrum}
      POSTGRES_DB: ferrum
    volumes:
      - ferrum-postgres:/var/lib/postgresql
    healthcheck:
      test: [CMD-SHELL, pg_isready -U ferrum -d ferrum]
      interval: 2s
      timeout: 3s
      retries: 30

  valkey:
    image: valkey/valkey:9-alpine
    restart: unless-stopped
    # Сессии лежат здесь: перезапуск не должен выходить из всех устройств.
    command: [valkey-server, --appendonly, 'yes']
    volumes:
      - ferrum-valkey:/data
    healthcheck:
      test: [CMD, valkey-cli, ping]
      interval: 2s
      timeout: 3s
      retries: 30

  # Перехватывает письма в разработке: http://localhost:8025. В рабочем развёртывании
  # профиля `dev` нет, письма уходят на SMTP_URL.
  mailpit:
    image: axllent/mailpit:v1.33.0
    restart: unless-stopped
    profiles: [dev]
    ports:
      - '${MAILPIT_PORT:-8025}:8025'

volumes:
  ferrum-data:
  ferrum-postgres:
  ferrum-valkey:
````

В `package.json` заменить:

````json
  "type": "module",
  "scripts": {
    "typecheck": "npm --prefix apps/web run typecheck",
    "lint": "oxlint --deny-warnings",
    "format": "prettier --write .",
    "format:check": "prettier --check ."
  },
  "devDependencies": {
````

на:

````json
  "type": "module",
  "scripts": {
    "typecheck": "npm --prefix apps/web run typecheck && tsc -p apps/server/tsconfig.json && tsc -p packages/protocol/tsconfig.json",
    "lint": "oxlint --deny-warnings",
    "format": "prettier --write .",
    "format:check": "prettier --check .",
    "test:stand": "docker compose -p ferrum-test --env-file docker-compose.test.env -f docker-compose.yml -f docker-compose.test.yml up -d --wait postgres valkey mailpit",
    "test:server": "npm run test:stand && cd packages/protocol && bun test && cd ../../apps/server && bun test",
    "test:stand:down": "docker compose -p ferrum-test --env-file docker-compose.test.env -f docker-compose.yml -f docker-compose.test.yml down -v"
  },
  "devDependencies": {
````

Создать `packages/protocol/package.json`:

````json
{
  "name": "@ferrum/protocol",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "exports": "./src/index.ts"
}
````

Создать `packages/protocol/src/auth.ts`:

````ts
// Запросы и ответы входа. Разбор не доверяет данным: неверное — `null`, сервер отвечает
// 400, ничего не передавая дальше.

export const API_SIGN_IN = '/api/auth/sign-in/magic-link';
export const API_CONFIRM = '/api/auth/confirm';
export const API_SESSION = '/api/auth/get-session';
export const API_UPDATE_USER = '/api/auth/update-user';
export const API_SIGN_OUT = '/api/auth/sign-out';
export const API_SIGN_OUT_EVERYWHERE = '/api/auth/revoke-sessions';

/** Адрес не длиннее, чем позволяет RFC 5321. */
export const MAX_EMAIL_BYTES = 254;
export const MAX_NAME_CHARS = 100;
/** Запрошенный адрес приложения, куда вернуться после входа. */
export const MAX_NEXT_CHARS = 512;
/** Срок ссылки из письма, спека, раздел 5. */
export const MAGIC_LINK_TTL_SECONDS = 15 * 60;
/** Повторная отправка со страницы входа не раньше этого срока. */
export const RESEND_SECONDS = 60;
/** Токен ссылки: столько букв выдаёт плагин `magicLink`. */
const TOKEN_RE = /^[A-Za-z]{32}$/;
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
// eslint-disable-next-line no-control-regex
const CONTROL_RE = /[\u0000-\u001f\u007f]/;

export type AuthErrorCode = 'BAD_REQUEST' | 'BAD_ORIGIN' | 'INVALID_TOKEN' | 'TOO_MANY_REQUESTS' | 'UNAUTHORIZED';

export interface AuthErrorBody {
  error: AuthErrorCode;
}

export interface SignInRequest {
  email: string;
  /** Маршрут приложения после `#`, куда открыть после входа. */
  next?: string;
}

export interface ConfirmRequest {
  token: string;
}

export interface NameRequest {
  name: string;
}

export interface SessionUser {
  id: string;
  email: string;
  name: string;
}

function record(v: unknown): Record<string, unknown> | null {
  return typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
}

/** Адрес приводится к нижнему регистру: `Ivan@x.ru` и `ivan@x.ru` — один человек. */
export function parseEmail(v: unknown): string | null {
  if (typeof v !== 'string') return null;
  const email = v.trim().toLowerCase();
  if (new TextEncoder().encode(email).length > MAX_EMAIL_BYTES) return null;
  return EMAIL_RE.test(email) && !CONTROL_RE.test(email) ? email : null;
}

/** Только маршрут самого приложения (`#/…`): ссылка из письма не уведёт на чужой адрес. */
export function parseNext(v: unknown): string | null {
  if (typeof v !== 'string' || v.length > MAX_NEXT_CHARS || CONTROL_RE.test(v)) return null;
  if (!v.startsWith('#/') || v.startsWith('#/auth/')) return null;
  return v;
}

export function parseSignInRequest(v: unknown): SignInRequest | null {
  const r = record(v);
  const email = r && parseEmail(r.email);
  if (!r || !email) return null;
  if (r.next === undefined) return { email };
  const next = parseNext(r.next);
  return next ? { email, next } : null;
}

export function parseConfirmRequest(v: unknown): ConfirmRequest | null {
  const r = record(v);
  return r && typeof r.token === 'string' && TOKEN_RE.test(r.token) ? { token: r.token } : null;
}

/** Имя с первого входа: пробелы по краям убираются, пустое не годится. */
export function parseNameRequest(v: unknown): NameRequest | null {
  const r = record(v);
  if (!r || typeof r.name !== 'string') return null;
  const name = r.name.trim();
  if (name.length === 0 || Array.from(name).length > MAX_NAME_CHARS || CONTROL_RE.test(name)) return null;
  return { name };
}

/** Ссылка из письма: токен лежит после `#`, поэтому не попадает на сервер и в журналы. */
export function confirmHash(token: string, next?: string): string {
  const q = new URLSearchParams({ token });
  if (next) q.set('next', next);
  return `#/auth/confirm?${q.toString()}`;
}

/** Обратно: что пришло в адресе страницы подтверждения; не она — `null`. */
export function parseConfirmHash(hash: string): { token: string | null; next: string | null } | null {
  const [path, query = ''] = hash.split('?', 2);
  if (path !== '#/auth/confirm') return null;
  const q = new URLSearchParams(query);
  const parsed = parseConfirmRequest({ token: q.get('token') });
  return { token: parsed ? parsed.token : null, next: parseNext(q.get('next')) };
}
````

Создать `packages/protocol/src/index.ts`:

````ts
// Общий протокол клиента и сервера: только типы и разбор сообщений, без зависимостей,
// потому что его импортируют и `apps/web`, и `apps/server` (спека подсистемы 4, раздел 3).

export * from './auth';
````

Создать `packages/protocol/tsconfig.json`:

````json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2024", "DOM"],
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "types": []
  },
  "include": ["src/**/*.ts"]
}
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `cd apps/server && bun install --frozen-lockfile`
Expected: зависимости установлены по `apps/server/bun.lock`

- [ ] **Step 5: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Run: `docker compose -p ferrum-test --env-file docker-compose.test.env -f docker-compose.yml -f docker-compose.test.yml up -d --wait --build server && curl -fsS http://localhost:18090/healthz && docker compose -p ferrum-test --env-file docker-compose.test.env -f docker-compose.yml -f docker-compose.test.yml stop server`

Expected: PASS: всё без замечаний; `bun test`: `packages/protocol` — 6 pass, `apps/server` — 13 pass; образ собран (первый раз несколько минут), `/healthz` отвечает `{"ok":true,"postgres":true,"valkey":true}`

- [ ] **Step 6: Закоммитить**

```bash
git add .dockerignore .env.example Dockerfile apps/server/bun.lock apps/server/bunfig.toml apps/server/migrations/001_init.sql apps/server/package.json apps/server/src/app.ts apps/server/src/blob-store.ts apps/server/src/config.ts apps/server/src/index.ts apps/server/src/migrate.ts apps/server/src/static.ts apps/server/src/valkey.ts apps/server/test/blob-store.test.ts apps/server/test/config.test.ts apps/server/test/helpers.ts apps/server/test/migrate.test.ts apps/server/test/server.test.ts apps/server/tsconfig.json docker-compose.test.env docker-compose.test.yml docker-compose.yml package.json packages/protocol/package.json packages/protocol/src/auth.ts packages/protocol/src/index.ts packages/protocol/test/auth.test.ts packages/protocol/tsconfig.json
git commit -F - <<'EOF'
feat(server): скелет сервера — Bun, миграции, BlobStore, /healthz, Docker

apps/server на `Bun.serve` без фреймворка: настройки из окружения, нумерованные
SQL-миграции под блокировкой Postgres, `FsBlobStore`, `/healthz` (база и Valkey),
раздача сборки apps/web с заголовками изоляции. packages/protocol — общие типы и разбор
запросов входа. Образ в три стадии, docker-compose.yml с postgres, valkey и mailpit
(профиль dev), docker-compose.test.yml — свой стенд тестов.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 5: Вход на сервере

Спека §5, главный риск §14.1: Better Auth на Bun проходит весь круг без обходов. Токен хэшем и `GETDEL`, сессии в Valkey, таблицы только `user` и `account`, пределы частоты, `ALLOWED_EMAILS`, выход, проверка для WebSocket.

**Files:**
- Create: `apps/server/auth-schema.ts`, `apps/server/migrations/002_better_auth.sql`, `apps/server/src/auth-routes.ts`, `apps/server/src/auth.ts`, `apps/server/src/mail.ts`, `apps/server/src/secondary-storage.ts`
- Modify: `apps/server/bun.lock`, `apps/server/package.json`, `apps/server/src/app.ts`, `apps/server/src/index.ts`, `docker-compose.test.yml`, `packages/protocol/src/auth.ts`
- Test: `apps/server/test/auth.test.ts` (новый), `apps/server/test/helpers.ts`, `apps/server/test/migrate.test.ts`, `apps/server/test/server.test.ts`

**Interfaces:**
- Consumes: `@ferrum/protocol`, `startServer`, `migrate`, `connectValkey`.
- Produces: `createAuth(deps): Auth`, `authOptions(deps)`, `emailAllowed`, `CLIENT_IP_HEADER`, `MAILS_PER_ADDRESS`, `MAILS_PER_IP`; `authRoutes(deps)`, `originAllowed`, `clientIp`, `authorizeUpgrade(req, auth, publicUrl): Promise<UpgradeSession | null>`; `valkeyStorage(client)`; `Mailer`, `smtpMailer`, `magicLinkMail`; маршруты `/api/auth/*`.

- [ ] **Step 1: Написать тесты**

Создать `apps/server/test/auth.test.ts`:

````ts
import { afterAll, beforeEach, expect, test } from 'bun:test';
import { getMigrations } from 'better-auth/db/migration';
import { join } from 'node:path';
import { startServer } from '../src/app';
import { authorizeUpgrade } from '../src/auth-routes';
import { authOptions, createAuth, emailAllowed, MAILS_PER_ADDRESS, MAILS_PER_IP, type AuthDeps } from '../src/auth';
import { magicLinkMail, smtpMailer, type Mail } from '../src/mail';
import { migrate } from '../src/migrate';
import { connectValkey } from '../src/valkey';
import { resetDatabase, resetValkey, TEST_SECRET, TEST_VALKEY_URL, testPool } from './helpers';

const pool = testPool();
const valkey = connectValkey(TEST_VALKEY_URL);
const stops: (() => void)[] = [];
let mails: Mail[] = [];

beforeEach(async () => {
  await resetDatabase(pool);
  await resetValkey(valkey);
  await migrate(pool, join(import.meta.dir, '../migrations'));
  mails = [];
});
afterAll(async () => {
  for (const s of stops) s();
  valkey.close();
  await pool.end();
});

/** Настоящий сервер на свободном порту; `PUBLIC_URL` совпадает с его адресом, как на стенде. */
function stand(over: Partial<AuthDeps> = {}, trustProxy = false) {
  const server = Bun.serve({ port: 0, fetch: () => new Response() });
  const port = server.port!;
  server.stop(true);
  const publicUrl = `http://localhost:${port}`;
  const deps: AuthDeps = {
    publicUrl,
    secret: TEST_SECRET,
    pool,
    valkey,
    allowedEmails: [],
    mailer: { send: async (m) => void mails.push(m) },
    ...over,
  };
  const auth = createAuth(deps);
  const app = startServer({ pool, valkey, webDist: '/nonexistent', auth, publicUrl, trustProxy }, port);
  stops.push(() => app.stop(true));
  const post = (path: string, body: unknown, headers: Record<string, string> = {}) =>
    fetch(`${publicUrl}${path}`, {
      method: 'POST',
      headers: { origin: publicUrl, 'content-type': 'application/json', ...headers },
      body: typeof body === 'string' ? body : JSON.stringify(body),
      redirect: 'manual',
    });
  return {
    publicUrl,
    auth,
    post,
    get: (path: string, headers: Record<string, string> = {}) => fetch(`${publicUrl}${path}`, { headers, redirect: 'manual' }),
  };
}
type Stand = ReturnType<typeof stand>;

const cookieOf = (res: Response) =>
  res.headers
    .getSetCookie()
    .map((c) => c.split(';')[0])
    .join('; ');
const tokenOf = (mail: Mail) => new URLSearchParams(mail.text.split('#/auth/confirm?')[1].split('\n')[0]).get('token')!;

async function requestLink(s: Stand, email = 'ivan@example.com', next?: string): Promise<string> {
  const res = await s.post('/api/auth/sign-in/magic-link', { email, next });
  expect(res.status).toBe(200);
  return tokenOf(mails.at(-1)!);
}
async function signIn(s: Stand, email?: string): Promise<string> {
  const res = await s.post('/api/auth/confirm', { token: await requestLink(s, email) });
  expect(res.status).toBe(200);
  return cookieOf(res);
}
const sessionOf = async (s: Stand, cookie: string) =>
  (await (await s.get('/api/auth/session', { cookie })).json()) as { user: { id: string; email: string; name: string } | null };

test('круг: письмо → подтверждение → сессия в Valkey → проверка сессии', async () => {
  const s = stand();
  const res = await s.post('/api/auth/sign-in/magic-link', { email: 'Ivan@Example.com', next: '#/file/abc' });
  expect(res.status).toBe(200);
  expect(await res.json()).toEqual({ ok: true });
  expect(mails).toHaveLength(1);
  expect(mails[0].to).toBe('ivan@example.com');
  expect(mails[0].subject).toBe('Вход в Ferrum');
  // Ссылка ведёт на страницу подтверждения, а не на точку, тратящую токен.
  expect(mails[0].text).toContain(`${s.publicUrl}/#/auth/confirm?token=`);
  expect(mails[0].text).toContain('next=%23%2Ffile%2Fabc');
  expect(mails[0].text).not.toContain('magic-link/verify');

  // Открытие ссылки токен не тратит: страница сама ничего не отправляет на сервер.
  const token = tokenOf(mails[0]);
  // Токен в Valkey — только хэшем.
  const keys = (await valkey.send('KEYS', ['verification:*'])) as string[];
  expect(keys).toHaveLength(1);
  expect(keys[0]).not.toContain(token);
  expect(await valkey.get(keys[0])).not.toContain(token);
  expect(await valkey.ttl(keys[0])).toBeGreaterThan(890);
  expect(await valkey.ttl(keys[0])).toBeLessThanOrEqual(900);

  const ok = await s.post('/api/auth/confirm', { token });
  expect(ok.status).toBe(200);
  expect(await ok.json()).toEqual({ ok: true });
  const cookie = ok.headers.getSetCookie().find((c) => c.startsWith('ferrum.session_token='))!;
  expect(cookie).toContain('HttpOnly');
  expect(cookie).toContain('SameSite=Lax');
  expect(cookie).toContain('Max-Age=2592000');
  expect(cookie).not.toContain('Secure');

  const me = await sessionOf(s, cookieOf(ok));
  expect(me.user?.email).toBe('ivan@example.com');
  expect(me.user?.name).toBe('');
  // Пользователь — в Postgres, сессия — в Valkey, таблицы сессий нет.
  expect((await pool.query('SELECT email FROM "user"')).rows).toEqual([{ email: 'ivan@example.com' }]);
  const tables = (await pool.query("SELECT table_name AS t FROM information_schema.tables WHERE table_schema = 'public'")).rows.map(
    (r) => r.t,
  );
  expect(tables.sort()).toEqual(['account', 'schema_migration', 'user']);
  const sessionToken = decodeURIComponent(cookieOf(ok).split('=')[1]).split('.')[0];
  expect(await valkey.get(sessionToken)).toContain('ivan@example.com');
});

test('ссылка одноразовая, в том числе при одновременных нажатиях', async () => {
  const s = stand();
  const token = await requestLink(s);
  const results = await Promise.all(Array.from({ length: 5 }, () => s.post('/api/auth/confirm', { token })));
  expect(results.map((r) => r.status).sort()).toEqual([200, 400, 400, 400, 400]);
  const again = await s.post('/api/auth/confirm', { token });
  expect(again.status).toBe(400);
  expect(await again.json()).toEqual({ error: 'INVALID_TOKEN' });
  expect(again.headers.getSetCookie()).toEqual([]);
});

test('истёкшая ссылка не входит, срок задаётся в Valkey', async () => {
  const s = stand({ magicLinkSeconds: 1 });
  const token = await requestLink(s);
  await Bun.sleep(1200);
  const res = await s.post('/api/auth/confirm', { token });
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'INVALID_TOKEN' });
});

test('точка Better Auth, тратящая токен по GET, снаружи закрыта', async () => {
  const s = stand();
  const token = await requestLink(s);
  const res = await s.get(`/api/auth/magic-link/verify?token=${token}&callbackURL=/`);
  expect(res.status).toBe(404);
  expect(res.headers.getSetCookie()).toEqual([]);
  // Токен цел.
  expect((await s.post('/api/auth/confirm', { token })).status).toBe(200);
});

test('наружу открыт только список маршрутов', async () => {
  const s = stand();
  for (const [method, path] of [
    ['GET', '/api/auth/get-session'],
    ['GET', '/api/auth/ok'],
    ['GET', '/api/auth/list-sessions'],
    ['POST', '/api/auth/update-user'],
    ['POST', '/api/auth/revoke-sessions'],
    ['POST', '/api/auth/delete-user'],
    ['POST', '/api/auth/sign-up/email'],
    ['GET', '/api/auth/sign-in/magic-link'],
    ['GET', '/api/auth/constructor'],
    ['POST', '/api/auth/constructor'],
  ]) {
    const res = await fetch(`${s.publicUrl}${path}`, { method, headers: { origin: s.publicUrl } });
    expect([path, res.status]).toEqual([path, 404]);
  }
});

test('изменяющие запросы: чужой Origin и его отсутствие — 403, без последствий', async () => {
  const s = stand();
  for (const headers of [{ origin: 'https://evil.example' }, { origin: 'null' }]) {
    expect((await s.post('/api/auth/sign-in/magic-link', { email: 'a@b.ru' }, headers)).status).toBe(403);
  }
  const bare = await fetch(`${s.publicUrl}/api/auth/sign-in/magic-link`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: '{"email":"a@b.ru"}',
  });
  expect(bare.status).toBe(403);
  expect(await bare.json()).toEqual({ error: 'BAD_ORIGIN' });
  expect(mails).toEqual([]);
  const token = await requestLink(s);
  expect((await s.post('/api/auth/confirm', { token }, { origin: 'https://evil.example' })).status).toBe(403);
  expect((await s.post('/api/auth/confirm', { token })).status).toBe(200);
});

test('неверные данные — 400: схема из protocol, лишние поля не доходят до Better Auth', async () => {
  const s = stand();
  for (const body of [
    'не json',
    '{}',
    '{"email":"нет"}',
    '{"email":"a@b.ru","next":"https://evil.example"}',
    `{"email":"a@b.ru","pad":"${'x'.repeat(5000)}"}`,
  ]) {
    const res = await s.post('/api/auth/sign-in/magic-link', body);
    expect([body.slice(0, 40), res.status]).toEqual([body.slice(0, 40), 400]);
    expect(await res.json()).toEqual({ error: 'BAD_REQUEST' });
  }
  expect((await s.post('/api/auth/confirm', { token: 'short' })).status).toBe(400);
  expect(mails).toEqual([]);
  await s.post('/api/auth/sign-in/magic-link', { email: 'a@b.ru', callbackURL: 'https://evil.example', name: 'Чужое имя' });
  const cookie = cookieOf(await s.post('/api/auth/confirm', { token: tokenOf(mails[0]) }));
  expect((await sessionOf(s, cookie)).user?.name).toBe('');
});

test('ответы не раскрывают токен сессии и то, есть ли такой адрес', async () => {
  const s = stand();
  const cookie = await signIn(s, 'known@example.com');
  const session = await (await s.get('/api/auth/session', { cookie })).text();
  expect(session).not.toContain('token');
  const known = await s.post('/api/auth/sign-in/magic-link', { email: 'known@example.com' });
  const unknown = await s.post('/api/auth/sign-in/magic-link', { email: 'unknown@example.com' });
  expect(known.status).toBe(unknown.status);
  expect(await known.text()).toBe(await unknown.text());
});

test('ALLOWED_EMAILS: адрес и домен; остальным ответ тот же, письма нет', async () => {
  expect(emailAllowed('a@b.ru', [])).toBe(true);
  expect(emailAllowed('a@b.ru', ['a@b.ru'])).toBe(true);
  expect(emailAllowed('c@b.ru', ['b.ru'])).toBe(true);
  expect(emailAllowed('c@notb.ru', ['b.ru'])).toBe(false);
  const s = stand({ allowedEmails: ['ivan@example.com', 'team.org'] });
  const denied = await s.post('/api/auth/sign-in/magic-link', { email: 'petr@example.com' });
  expect(denied.status).toBe(200);
  expect(await denied.json()).toEqual({ ok: true });
  expect(mails).toEqual([]);
  await s.post('/api/auth/sign-in/magic-link', { email: 'ivan@example.com' });
  await s.post('/api/auth/sign-in/magic-link', { email: 'any@team.org' });
  expect(mails.map((m) => m.to)).toEqual(['ivan@example.com', 'any@team.org']);
});

test('ограничение: не больше 5 писем на адрес в час, другому адресу можно, счётчик истекает', async () => {
  const s = stand();
  const send = (email: string) => s.post('/api/auth/sign-in/magic-link', { email });
  for (let i = 0; i < MAILS_PER_ADDRESS; i++) expect((await send('ivan@example.com')).status).toBe(200);
  const over = await send('IVAN@example.com');
  expect(over.status).toBe(429);
  expect(await over.json()).toEqual({ error: 'TOO_MANY_REQUESTS' });
  expect(mails).toHaveLength(MAILS_PER_ADDRESS);
  expect((await send('petr@example.com')).status).toBe(200);
  const ttl = await valkey.ttl('mail-to:ivan@example.com');
  expect(ttl).toBeGreaterThan(3500);
  expect(ttl).toBeLessThanOrEqual(3600);
});

test('ограничение: не больше 20 писем на IP в час, чужой X-Forwarded-For не помогает', async () => {
  const s = stand();
  for (let i = 0; i < MAILS_PER_IP; i++) {
    const res = await s.post(
      '/api/auth/sign-in/magic-link',
      { email: `u${i}@example.com` },
      { 'x-forwarded-for': `10.0.0.${i}`, 'x-ferrum-client-ip': `10.0.1.${i}` },
    );
    expect(res.status).toBe(200);
  }
  const over = await s.post('/api/auth/sign-in/magic-link', { email: 'last@example.com' }, { 'x-forwarded-for': '10.9.9.9' });
  expect(over.status).toBe(429);
  expect(over.headers.get('retry-after')).toBe('3600');
  expect(mails).toHaveLength(MAILS_PER_IP);
});

test('за своим прокси адрес клиента — последний в X-Forwarded-For', async () => {
  const s = stand({}, true);
  for (let i = 0; i < MAILS_PER_IP; i++) {
    expect(
      (await s.post('/api/auth/sign-in/magic-link', { email: `u${i}@example.com` }, { 'x-forwarded-for': `1.1.1.${i}, 10.0.0.1` })).status,
    ).toBe(200);
  }
  expect((await s.post('/api/auth/sign-in/magic-link', { email: 'x@example.com' }, { 'x-forwarded-for': '10.0.0.1' })).status).toBe(429);
  expect((await s.post('/api/auth/sign-in/magic-link', { email: 'y@example.com' }, { 'x-forwarded-for': '10.0.0.2' })).status).toBe(200);
});

test('имя с первого входа сохраняется и видно в сессии; без сессии — 401', async () => {
  const s = stand();
  expect((await s.post('/api/auth/name', { name: 'Иван' })).status).toBe(401);
  const cookie = await signIn(s);
  expect((await s.post('/api/auth/name', { name: '   ' }, { cookie })).status).toBe(400);
  const res = await s.post('/api/auth/name', { name: '  Иван  ' }, { cookie });
  expect(res.status).toBe(200);
  expect(((await res.json()) as { user: { name: string } }).user.name).toBe('Иван');
  expect((await sessionOf(s, cookie)).user?.name).toBe('Иван');
  expect((await pool.query('SELECT name FROM "user"')).rows).toEqual([{ name: 'Иван' }]);
});

test('повторный вход тем же адресом — тот же пользователь', async () => {
  const s = stand();
  const a = await signIn(s, 'ivan@example.com');
  const b = await signIn(s, 'IVAN@example.com');
  expect((await sessionOf(s, a)).user?.id).toBe((await sessionOf(s, b)).user!.id);
  expect((await pool.query('SELECT count(*)::int AS n FROM "user"')).rows[0].n).toBe(1);
});

test('выход на этом устройстве: другая сессия жива, cookie снята', async () => {
  const s = stand();
  const a = await signIn(s);
  const b = await signIn(s);
  const out = await s.post('/api/auth/sign-out', {}, { cookie: a });
  expect(out.status).toBe(200);
  expect(out.headers.getSetCookie().join()).toContain('Max-Age=0');
  expect((await sessionOf(s, a)).user).toBeNull();
  expect((await sessionOf(s, b)).user).not.toBeNull();
});

test('выход на всех устройствах: все сессии человека сняты, чужие целы', async () => {
  const s = stand();
  const a = await signIn(s);
  const b = await signIn(s);
  const other = await signIn(s, 'petr@example.com');
  const out = await s.post('/api/auth/sign-out-everywhere', {}, { cookie: a });
  expect(out.status).toBe(200);
  expect(out.headers.getSetCookie().join()).toContain('Max-Age=0');
  expect((await sessionOf(s, a)).user).toBeNull();
  expect((await sessionOf(s, b)).user).toBeNull();
  expect((await sessionOf(s, other)).user?.email).toBe('petr@example.com');
  expect((await s.post('/api/auth/sign-out-everywhere', {})).status).toBe(401);
});

test('сессия продлевается: после updateAge новый срок и в cookie, и в Valkey', async () => {
  const s = stand({ sessionUpdateAgeSeconds: 1 });
  const cookie = await signIn(s);
  const token = decodeURIComponent(cookie.split('=')[1]).split('.')[0];
  await valkey.send('EXPIRE', [token, '1000']);
  await Bun.sleep(1100);
  const res = await s.get('/api/auth/session', { cookie });
  expect(res.headers.getSetCookie().find((c) => c.startsWith('ferrum.session_token='))).toContain('Max-Age=2592000');
  expect(await valkey.ttl(token)).toBeGreaterThan(2_590_000);
});

test('Valkey недоступен: сбой, а не «сессии нет», человека не выбрасывает на экран входа', async () => {
  const dead = connectValkey(TEST_VALKEY_URL);
  await dead.connect();
  const s = stand({ valkey: dead });
  const cookie = await signIn(s);
  dead.close();
  const res = await s.get('/api/auth/session', { cookie });
  expect(res.status).toBe(500);
});

test('Secure у cookie — при HTTPS', () => {
  const o = authOptions({
    publicUrl: 'https://ferrum.example',
    secret: TEST_SECRET,
    pool,
    valkey,
    mailer: { send: async () => {} },
    allowedEmails: [],
  });
  expect(o.advanced.useSecureCookies).toBe(true);
  const h = authOptions({
    publicUrl: 'http://localhost:8090',
    secret: TEST_SECRET,
    pool,
    valkey,
    mailer: { send: async () => {} },
    allowedEmails: [],
  });
  expect(h.advanced.useSecureCookies).toBe(false);
});

test('проверка перед WebSocket: настоящий upgrade с сессией, без неё, с чужим Origin, после выхода', async () => {
  const s = stand();
  const cookie = await signIn(s);
  const opened: string[] = [];
  const ws = Bun.serve<{ userId: string }>({
    port: 0,
    fetch: async (req, server) => {
      const who = await authorizeUpgrade(req, s.auth, s.publicUrl);
      if (!who) return new Response('Нет доступа', { status: 401 });
      if (server.upgrade(req, { data: { userId: who.userId } })) return undefined;
      return new Response('Не WebSocket', { status: 400 });
    },
    websocket: {
      open: (socket) => void opened.push(socket.data.userId),
      message: () => {},
    },
  });
  stops.push(() => ws.stop(true));
  const open = (headers: Record<string, string>) =>
    new Promise<string>((resolve) => {
      const c = new WebSocket(`ws://127.0.0.1:${ws.port}/`, { headers } as never);
      c.onopen = () => {
        c.close();
        resolve('open');
      };
      c.onerror = () => resolve('refused');
    });
  const me = (await sessionOf(s, cookie)).user!;
  expect(await open({ cookie, origin: s.publicUrl })).toBe('open');
  expect(opened).toEqual([me.id]);
  expect(await open({ origin: s.publicUrl })).toBe('refused');
  expect(await open({ cookie, origin: 'https://evil.example' })).toBe('refused');
  expect(await open({ cookie: 'ferrum.session_token=forged.signature', origin: s.publicUrl })).toBe('refused');
  await s.post('/api/auth/sign-out', {}, { cookie });
  expect(await open({ cookie, origin: s.publicUrl })).toBe('refused');
  expect(opened).toEqual([me.id]);
});

test('миграция совпадает со схемой, которую ждёт Better Auth этой версии', async () => {
  const { toBeCreated, toBeAdded } = await getMigrations(
    authOptions({
      publicUrl: 'http://localhost:1',
      secret: TEST_SECRET,
      pool,
      valkey,
      mailer: { send: async () => {} },
      allowedEmails: [],
    }),
  );
  expect(toBeCreated).toEqual([]);
  expect(toBeAdded).toEqual([]);
});

test('настоящее письмо через SMTP в Mailpit', async () => {
  const api = 'http://127.0.0.1:18025/api/v1';
  await fetch(`${api}/messages`, { method: 'DELETE' });
  const mailer = smtpMailer('smtp://127.0.0.1:11025', 'Ferrum <ferrum@localhost>');
  const link = 'http://localhost:8090/#/auth/confirm?token=abc&next=%23%2Ffile%2F1';
  await mailer.send(magicLinkMail('ivan@example.com', link));
  const list = (await (await fetch(`${api}/messages`)).json()) as {
    messages: { ID: string; Subject: string; To: { Address: string }[]; From: { Address: string; Name: string } }[];
  };
  expect(list.messages).toHaveLength(1);
  expect(list.messages[0].Subject).toBe('Вход в Ferrum');
  expect(list.messages[0].To[0].Address).toBe('ivan@example.com');
  expect(list.messages[0].From).toMatchObject({ Address: 'ferrum@localhost', Name: 'Ferrum' });
  const full = (await (await fetch(`${api}/message/${list.messages[0].ID}`)).json()) as { Text: string; HTML: string };
  expect(full.Text).toContain(link);
  expect(full.HTML).toContain(link.replaceAll('&', '&amp;'));
});
````

В `apps/server/test/helpers.ts` заменить:

````ts
export const TEST_DATABASE_URL = process.env.TEST_DATABASE_URL ?? 'postgres://ferrum:ferrum@127.0.0.1:15432/ferrum';
export const TEST_VALKEY_URL = process.env.TEST_VALKEY_URL ?? 'redis://127.0.0.1:16379';

export function testPool(): Pool {
````

на:

````ts
export const TEST_DATABASE_URL = process.env.TEST_DATABASE_URL ?? 'postgres://ferrum:ferrum@127.0.0.1:15432/ferrum';
export const TEST_VALKEY_URL = process.env.TEST_VALKEY_URL ?? 'redis://127.0.0.1:16379';

export const TEST_SECRET = 'ferrum-test-secret-ZkP3nV8qR2xW7mB4tY9c';

export function testPool(): Pool {
````

В `apps/server/test/migrate.test.ts` заменить:

````ts
test('миграции репозитория применяются на пустой базе', async () => {
  const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
  expect(applied[0]).toBe('001_init.sql');
});
````

на:

````ts
test('миграции репозитория применяются на пустой базе', async () => {
  const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
  expect(applied).toEqual(['001_init.sql', '002_better_auth.sql']);
});
````

В `apps/server/test/server.test.ts` заменить:

````ts
import { Pool } from 'pg';
import { startServer } from '../src/app';
import { connectValkey } from '../src/valkey';
import { TEST_VALKEY_URL, testPool } from './helpers';

const web = mkdtempSync(join(tmpdir(), 'ferrum-web-'));
````

на:

````ts
import { Pool } from 'pg';
import { startServer } from '../src/app';
import { createAuth } from '../src/auth';
import { connectValkey } from '../src/valkey';
import { TEST_SECRET, TEST_VALKEY_URL, testPool } from './helpers';

const web = mkdtempSync(join(tmpdir(), 'ferrum-web-'));
````

В `apps/server/test/server.test.ts` заменить:

````ts
const pool = testPool();
const valkey = connectValkey(TEST_VALKEY_URL);
const server = startServer({ pool, valkey, webDist: web }, 0);
const url = (p: string) => `http://127.0.0.1:${server.port}${p}`;
afterAll(async () => {
````

на:

````ts
const pool = testPool();
const valkey = connectValkey(TEST_VALKEY_URL);
const publicUrl = 'http://localhost:1';
const auth = createAuth({ publicUrl, secret: TEST_SECRET, pool, valkey, mailer: { send: async () => {} }, allowedEmails: [] });
const deps = { valkey, webDist: web, auth, publicUrl, trustProxy: false };
const server = startServer({ ...deps, pool }, 0);
const url = (p: string) => `http://127.0.0.1:${server.port}${p}`;
afterAll(async () => {
````

В `apps/server/test/server.test.ts` заменить:

````ts
  const dead = new Pool({ connectionString: 'postgres://ferrum:ferrum@127.0.0.1:1/ferrum', connectionTimeoutMillis: 200 });
  dead.on('error', () => {});
  const s = startServer({ pool: dead, valkey, webDist: web }, 0);
  const res = await fetch(`http://127.0.0.1:${s.port}/healthz`);
  expect(res.status).toBe(503);
````

на:

````ts
  const dead = new Pool({ connectionString: 'postgres://ferrum:ferrum@127.0.0.1:1/ferrum', connectionTimeoutMillis: 200 });
  dead.on('error', () => {});
  const s = startServer({ ...deps, pool: dead }, 0);
  const res = await fetch(`http://127.0.0.1:${s.port}/healthz`);
  expect(res.status).toBe(503);
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/server && bun test`
Expected: FAIL — `Cannot find module ../src/auth` и `better-auth/db/migration`, а `миграции репозитория применяются на пустой базе` падает на `002_better_auth.sql`: 8 pass, 3 fail. Стенд поднят задачей 4 (`npm run test:stand`), иначе падает больше

- [ ] **Step 3: Реализация**

Создать `apps/server/auth-schema.ts`:

````ts
// Только для генерации таблиц Better Auth (`apps/server/migrations/002_better_auth.sql`):
// bun --bun x auth@1.7.16 generate --config ./auth-schema.ts --output schema.sql -y
// Базы и Valkey этому файлу не нужны: схема зависит только от настроек. Запись `migrate.test.ts`
// сверяет миграцию с той же схемой.

import { betterAuth } from 'better-auth';
import { Pool } from 'pg';
import { authOptions } from './src/auth';

export const auth = betterAuth(
  authOptions({
    publicUrl: 'http://localhost:8090',
    secret: 'schema-only-schema-only-schema-only-0000',
    pool: new Pool({ connectionString: process.env.DATABASE_URL ?? 'postgres://ferrum:ferrum@127.0.0.1:15432/ferrum' }),
    valkey: new Bun.RedisClient(process.env.VALKEY_URL ?? 'redis://127.0.0.1:16379'),
    mailer: { send: async () => {} },
    allowedEmails: [],
  }),
);
````

В `apps/server/bun.lock` заменить:

````
      "dependencies": {
        "@ferrum/protocol": "file:../../packages/protocol",
        "pg": "8.23.1",
      },
      "devDependencies": {
        "@types/bun": "1.4.3",
        "@types/pg": "8.23.1",
      },
````

на:

````
      "dependencies": {
        "@ferrum/protocol": "file:../../packages/protocol",
        "better-auth": "1.7.16",
        "nodemailer": "10.2.0",
        "pg": "8.23.1",
      },
      "devDependencies": {
        "@types/bun": "1.4.3",
        "@types/nodemailer": "8.0.2",
        "@types/pg": "8.23.1",
      },
````

В `apps/server/bun.lock` заменить:

````
  },
  "packages": {
    "@ferrum/protocol": ["@ferrum/protocol@file:../../packages/protocol", {}],

    "@types/bun": ["@types/bun@1.4.3", "", { "dependencies": { "bun-types": "1.4.3" } }, "sha512-N67Pqy8JXbAY/j0tNraS3V5mubGvc6rrrNDmDCtZLlFzccbUJPDb0tvgUBC5q4m8ybkP93Jcn1QB+RYQ6PvBhw=="],
````

на:

````
  },
  "packages": {
    "@better-auth/core": ["@better-auth/core@1.7.16", "", { "dependencies": { "@standard-schema/spec": "^1.1.0", "zod": "^4.5.4" }, "peerDependencies": { "@better-auth/utils": "^0.4.2 || ^0.5.0", "@better-fetch/fetch": "1.3.3", "@opentelemetry/api": "^1.9.0", "better-call": "1.4.5", "jose": "^6.1.0", "kysely": "^0.28.5 || ^0.29.0", "nanostores": "^1.0.1" }, "optionalPeers": ["@opentelemetry/api"] }, "sha512-dwVAtEAEpjPsCV/TqOKRHreWyuiqvja+TnTTAuVbi3P0afM0+b1+ga+RTXNjVE86Q4HRwRg45Ek3txFrY8/1DA=="],

    "@better-auth/drizzle-adapter": ["@better-auth/drizzle-adapter@1.7.16", "", { "peerDependencies": { "@better-auth/core": "^1.7.16", "@better-auth/utils": "^0.4.2 || ^0.5.0", "drizzle-orm": "^0.45.2 || >=1.0.0-rc.1 <2.0.0" }, "optionalPeers": ["drizzle-orm"] }, "sha512-8cgtcWkCmJal+ecrl681Y+SKvMDy5Vg2ydCOPE7X8Mcx1Ud7Mqve3pASG0I2JWMJtrge1m8YbpNmCxhVGh+gpQ=="],

    "@better-auth/kysely-adapter": ["@better-auth/kysely-adapter@1.7.16", "", { "peerDependencies": { "@better-auth/core": "^1.7.16", "@better-auth/utils": "^0.4.2 || ^0.5.0", "kysely": "^0.28.17 || ^0.29.0" }, "optionalPeers": ["kysely"] }, "sha512-MkLZwHxhYbVr1QGGDJDZXMT1g54Q5yucDMdLGaz3Y5P8ERRfaqVEe+3MCVCb8XxJ7Rh+VNJKUix4VOBq7vXimg=="],

    "@better-auth/memory-adapter": ["@better-auth/memory-adapter@1.7.16", "", { "peerDependencies": { "@better-auth/core": "^1.7.16", "@better-auth/utils": "^0.4.2 || ^0.5.0" } }, "sha512-b0uUE3mhQJDu2QqLDN3XVHhXUhpr6zK+UhjL7IdNteTJJaAMhCcXfpLc7+kpOcNVEEBdRiFXBwsEtxRjcB3rLQ=="],

    "@better-auth/mongo-adapter": ["@better-auth/mongo-adapter@1.7.16", "", { "peerDependencies": { "@better-auth/core": "^1.7.16", "@better-auth/utils": "^0.4.2 || ^0.5.0", "mongodb": "^6.0.0 || ^7.0.0" }, "optionalPeers": ["mongodb"] }, "sha512-uZG9DhCHv7QcF8HtEIgYEL7Lawu0/1Pcp6ABe4ImYQdLV3HNRaAuQU/PxVxFQFAMSpnCVi4Rwg8+Qk8qVn2YMg=="],

    "@better-auth/prisma-adapter": ["@better-auth/prisma-adapter@1.7.16", "", { "peerDependencies": { "@better-auth/core": "^1.7.16", "@better-auth/utils": "^0.4.2 || ^0.5.0", "@prisma/client": "^5.0.0 || ^6.0.0 || ^7.0.0", "prisma": "^5.0.0 || ^6.0.0 || ^7.0.0" }, "optionalPeers": ["@prisma/client", "prisma"] }, "sha512-rSVMa4sWxvEURqVR+o4SZvq6dosCvD58nLw2LOw7woz791/CKOMxhrqrjiHLLzx+OfzGO0Ak9x7lrFTUwYO/KQ=="],

    "@better-auth/telemetry": ["@better-auth/telemetry@1.7.16", "", { "peerDependencies": { "@better-auth/core": "^1.7.16", "@better-auth/utils": "^0.4.2 || ^0.5.0", "@better-fetch/fetch": "1.3.3" } }, "sha512-mwQC0xB8KO636CjBxMZCNgglaIwh18DOemBJyNCy7DBOngkjPdNggPWYB8BpJ8wMqcn3SaKchdWZrNNqLRZV4Q=="],

    "@better-auth/utils": ["@better-auth/utils@0.5.1", "", { "dependencies": { "@noble/hashes": "^2.0.1" } }, "sha512-WETqw5vQ52uQ/xwWR4E8rTgT6fDEr/wnS+otDNkBCa/69281FCF32OKDdQUcahMCnjsvqXBNiEvAQHFB0EVRRg=="],

    "@better-fetch/fetch": ["@better-fetch/fetch@1.3.3", "", {}, "sha512-U6SlDt/GUWtK2JAxS/NhdoYTEim+EL1G5byz2TUj83KjLZBUpYzb5SmX0aXaCnbGkAjFKnV/o/0+cyXMbEcqqg=="],

    "@ferrum/protocol": ["@ferrum/protocol@file:../../packages/protocol", {}],

    "@noble/ciphers": ["@noble/ciphers@2.4.0", "", {}, "sha512-AnjFn0Jv92laAkvMrghlFZq4qQCIN/4DxFV/eooqtC2YTjB7kBeLMS2T9KJX4Dn+ZVXLOwK0lSgqDtx9gvxtiw=="],

    "@noble/hashes": ["@noble/hashes@2.4.0", "", {}, "sha512-X5XaVWZIBCT7HHZGm5I7ZQXDwLG+bGXuSrMQAW+7Zvl87h1kmc1ZB1VSRJcpUfoUrGQp4Fkoxm5kZ+Ms+aW+eA=="],

    "@standard-schema/spec": ["@standard-schema/spec@1.1.0", "", {}, "sha512-l2aFy5jALhniG5HgqrD6jXLi/rUWrKvqN/qJx6yoJsgKhblVd+iqqU4RCXavm/jPityDo5TCvKMnpjKnOriy0w=="],

    "@types/bun": ["@types/bun@1.4.3", "", { "dependencies": { "bun-types": "1.4.3" } }, "sha512-N67Pqy8JXbAY/j0tNraS3V5mubGvc6rrrNDmDCtZLlFzccbUJPDb0tvgUBC5q4m8ybkP93Jcn1QB+RYQ6PvBhw=="],
````

В `apps/server/bun.lock` заменить:

````
    "@types/node": ["@types/node@26.6.6", "", { "dependencies": { "undici-types": "~8.9.0" } }, "sha512-MzQj+8Kx3XI40OmUAE6Qoh58ZMNstNNMwn2nfd3qWsfe7nDNSqJNzes4njhz6AHwrSHOffSLwXU6yb474Q4aog=="],

    "@types/pg": ["@types/pg@8.23.1", "", { "dependencies": { "@types/node": "*", "pg-protocol": "*", "pg-types": "^2.2.0" } }, "sha512-fKVHpikPdg4GKks3JuLEhvwSyvwzF23hnabPy6DD8ljVbC7+6J5dQzdv4arV6jqq57djnMgs1HKBxX4P8aBI3A=="],

    "bun-types": ["bun-types@1.4.3", "", { "dependencies": { "@types/node": "*", "undici-types": "*" } }, "sha512-tinRJNGOmUN+tEAsn2y2e942n9JHRHR42iUYzDH75NmhJqVBokGSQkkfSFdNWwsANIdrK6ACH9t01M452YyTwg=="],

    "pg": ["pg@8.23.1", "", { "dependencies": { "pg-connection-string": "^2.14.1", "pg-pool": "^3.14.0", "pg-protocol": "^1.16.1", "pg-types": "2.2.0", "pgpass": "1.0.5" }, "optionalDependencies": { "pg-cloudflare": "^1.4.1" }, "peerDependencies": { "pg-native": ">=3.0.1" }, "optionalPeers": ["pg-native"] }, "sha512-aL96AHANtWjPLDOLqnhx+ngp9+UK7ETEU8VJrDCGvsSSi/mGLcWYsS6Herg7lmaBJe4uwrfqsa7gTEFaSizDoQ=="],
````

на:

````
    "@types/node": ["@types/node@26.6.6", "", { "dependencies": { "undici-types": "~8.9.0" } }, "sha512-MzQj+8Kx3XI40OmUAE6Qoh58ZMNstNNMwn2nfd3qWsfe7nDNSqJNzes4njhz6AHwrSHOffSLwXU6yb474Q4aog=="],

    "@types/nodemailer": ["@types/nodemailer@8.0.2", "", { "dependencies": { "@types/node": "*" } }, "sha512-c7M5ox8p0nEOfbJ2E9Qcmt8/QCu/VzsiAhzBiZbvky2PhKZDHpKB3sQHBM5MEZNkCfBOaS6S9//AGPRTNB7IaA=="],

    "@types/pg": ["@types/pg@8.23.1", "", { "dependencies": { "@types/node": "*", "pg-protocol": "*", "pg-types": "^2.2.0" } }, "sha512-fKVHpikPdg4GKks3JuLEhvwSyvwzF23hnabPy6DD8ljVbC7+6J5dQzdv4arV6jqq57djnMgs1HKBxX4P8aBI3A=="],

    "better-auth": ["better-auth@1.7.16", "", { "dependencies": { "@better-auth/core": "1.7.16", "@better-auth/drizzle-adapter": "1.7.16", "@better-auth/kysely-adapter": "1.7.16", "@better-auth/memory-adapter": "1.7.16", "@better-auth/mongo-adapter": "1.7.16", "@better-auth/prisma-adapter": "1.7.16", "@better-auth/telemetry": "1.7.16", "@better-auth/utils": "0.5.1", "@better-fetch/fetch": "1.3.3", "@noble/ciphers": "^2.2.0", "@noble/hashes": "^2.2.0", "better-call": "1.4.5", "defu": "^6.1.4", "jose": "^6.2.3", "kysely": "^0.28.17 || ^0.29.0", "nanostores": "^1.3.0", "zod": "^4.5.4" }, "peerDependencies": { "@lynx-js/react": "*", "@prisma/client": "^5.0.0 || ^6.0.0 || ^7.0.0", "@sveltejs/kit": "^2.0.0 || ^3.0.0", "@tanstack/react-start": "^1.0.0", "@tanstack/solid-start": "^1.0.0", "drizzle-kit": ">=0.31.4 || >=1.0.0-beta.1", "drizzle-orm": "^0.45.2 || >=1.0.0-rc.1 <2.0.0", "mongodb": "^6.0.0 || ^7.0.0", "mysql2": "^3.0.0", "next": "^14.0.0 || ^15.0.0 || ^16.0.0", "pg": "^8.0.0", "prisma": "^5.0.0 || ^6.0.0 || ^7.0.0", "react": "^18.0.0 || ^19.0.0", "react-dom": "^18.0.0 || ^19.0.0", "solid-js": "^1.0.0", "svelte": "^4.0.0 || ^5.0.0", "vitest": "^2.0.0 || ^3.0.0 || ^4.0.0 || ^5.0.0", "vue": "^3.0.0" }, "optionalPeers": ["@lynx-js/react", "@prisma/client", "@sveltejs/kit", "@tanstack/react-start", "@tanstack/solid-start", "drizzle-kit", "drizzle-orm", "mongodb", "mysql2", "next", "pg", "prisma", "react", "react-dom", "solid-js", "svelte", "vitest", "vue"] }, "sha512-ur6xj8jeUC9lcH8ni9LrwB4FiFMPDTWFJmb89yZ2SHsFSUobYgGMFsMq6aOorlVtqRr9ckTemRunCR6vzsO65Q=="],

    "better-call": ["better-call@1.4.5", "", { "dependencies": { "@better-auth/utils": "^0.5.0", "@better-fetch/fetch": "^1.3.1", "rou3": "^0.9.1", "set-cookie-parser": "^3.1.2" }, "peerDependencies": { "zod": "^4.0.0" }, "optionalPeers": ["zod"] }, "sha512-GVOWfn3pjEfUXn6GrfGf2h7tPAqj1oMLFBWBEswJrSkDKcLk6TIcjV0zexibcZZ0c5N1mkLMsPFopXWzWj/gDQ=="],

    "bun-types": ["bun-types@1.4.3", "", { "dependencies": { "@types/node": "*", "undici-types": "*" } }, "sha512-tinRJNGOmUN+tEAsn2y2e942n9JHRHR42iUYzDH75NmhJqVBokGSQkkfSFdNWwsANIdrK6ACH9t01M452YyTwg=="],

    "defu": ["defu@6.1.7", "", {}, "sha512-7z22QmUWiQ/2d0KkdYmANbRUVABpZ9SNYyH5vx6PZ+nE5bcC0l7uFvEfHlyld/HcGBFTL536ClDt3DEcSlEJAQ=="],

    "jose": ["jose@6.2.15", "", {}, "sha512-qdF6ewcZARtef63fJ9AKKZPgxw+t7USH6HLXYcD4CrdmSvDp79rpJSPFEKc90QBeGS63u1KQtN2MZQHC0yDIzA=="],

    "kysely": ["kysely@0.29.8", "", {}, "sha512-ixHU1H4R+LeC5Y0nAhYgiGRFpFRHm2DXKEmJSs/vbes9sKXyssyzxzE/NYiOJX5fY0BL28biCre3GqlCqUwyXQ=="],

    "nanostores": ["nanostores@1.5.5", "", {}, "sha512-FixtE239Gl6Gz0ZGCK0WzvNwObNCIwVT+L7zQcf1QgcA6AzBxblzqe9OT88qiucNkhsjSOqiogSaF5jVbJiqSA=="],

    "nodemailer": ["nodemailer@10.2.0", "", {}, "sha512-/XyGw6kX2stMdKGUJdAQmYAeY1FscWfdRydfd2NtK3A3KDfUUXCqZzExSNAt2xhh90X2qLCR+o2Gd48YPJcJwA=="],

    "pg": ["pg@8.23.1", "", { "dependencies": { "pg-connection-string": "^2.14.1", "pg-pool": "^3.14.0", "pg-protocol": "^1.16.1", "pg-types": "2.2.0", "pgpass": "1.0.5" }, "optionalDependencies": { "pg-cloudflare": "^1.4.1" }, "peerDependencies": { "pg-native": ">=3.0.1" }, "optionalPeers": ["pg-native"] }, "sha512-aL96AHANtWjPLDOLqnhx+ngp9+UK7ETEU8VJrDCGvsSSi/mGLcWYsS6Herg7lmaBJe4uwrfqsa7gTEFaSizDoQ=="],
````

В `apps/server/bun.lock` заменить:

````
    "postgres-interval": ["postgres-interval@1.2.0", "", { "dependencies": { "xtend": "^4.0.0" } }, "sha512-9ZhXKM/rw350N1ovuWHbGxnGh/SNJ4cnxHiM0rxE4VN41wsg8P8zWn9hv/buK00RP4WvlOyr/RBDiptyxVbkZQ=="],

    "split2": ["split2@4.2.0", "", {}, "sha512-UcjcJOWknrNkF6PLX83qcHM6KHgVKNkV62Y8a5uYDVv9ydGQVwAHMKqHdJje1VTWpljG0WYpCDhrCdAOYH4TWg=="],

````

на:

````
    "postgres-interval": ["postgres-interval@1.2.0", "", { "dependencies": { "xtend": "^4.0.0" } }, "sha512-9ZhXKM/rw350N1ovuWHbGxnGh/SNJ4cnxHiM0rxE4VN41wsg8P8zWn9hv/buK00RP4WvlOyr/RBDiptyxVbkZQ=="],

    "rou3": ["rou3@0.9.2", "", {}, "sha512-3SOzvaAg8rkHrXtRjpCvCvbyO5to9oOO27Z/XqHEYXfMRVSw/qMIVdmaOk9W2lcRLtR6dlqTjo9hDeJk70QBYQ=="],

    "set-cookie-parser": ["set-cookie-parser@3.2.2", "", {}, "sha512-df017qMeYuXo4emL09nlyMpi2i2MtEIKS/6jtXH3vmjFgIRsOi852/5RmhJ4EkvN3ZF8jXadM/EkTPXgc8yPIw=="],

    "split2": ["split2@4.2.0", "", {}, "sha512-UcjcJOWknrNkF6PLX83qcHM6KHgVKNkV62Y8a5uYDVv9ydGQVwAHMKqHdJje1VTWpljG0WYpCDhrCdAOYH4TWg=="],

````

В `apps/server/bun.lock` заменить:

````
    "xtend": ["xtend@4.0.2", "", {}, "sha512-LKYU1iAXJXUgAXn9URjiu+MWhyUXHsvfp7mcuYm9dSUKK0/CjtrUwFAxD82/mCWbtLsGjFIad0wIsod4zrTAEQ=="],

    "@types/node/undici-types": ["undici-types@8.9.0", "", {}, "sha512-KTDyRTYX8sWmKXAikPHHSyc63CRPETMctyjKFupcC6OBLXT3xsN0e9aF7m+mIXutFWpUXuedtowG7iLOzp0kQg=="],
  }
````

на:

````
    "xtend": ["xtend@4.0.2", "", {}, "sha512-LKYU1iAXJXUgAXn9URjiu+MWhyUXHsvfp7mcuYm9dSUKK0/CjtrUwFAxD82/mCWbtLsGjFIad0wIsod4zrTAEQ=="],

    "zod": ["zod@4.6.7", "", {}, "sha512-g8W2D2kU6a8untc053fX1uVBKzM7yqAxW/3zwk83+6RyF2+SKOuV+c0wBDhUwzV1PaGapmZH+rmcUzPf+3CGwg=="],

    "@types/node/undici-types": ["undici-types@8.9.0", "", {}, "sha512-KTDyRTYX8sWmKXAikPHHSyc63CRPETMctyjKFupcC6OBLXT3xsN0e9aF7m+mIXutFWpUXuedtowG7iLOzp0kQg=="],
  }
````

Создать `apps/server/migrations/002_better_auth.sql`:

````sql
-- Таблицы Better Auth 1.7.16: сгенерированы командой из `apps/server/auth-schema.ts`, без правок.
-- Сессии и токены входа — в Valkey, поэтому таблиц `session` и `verification` нет.
create table "user" ("id" text not null primary key, "name" text not null, "email" text not null unique, "emailVerified" boolean not null, "image" text, "createdAt" timestamptz default CURRENT_TIMESTAMP not null, "updatedAt" timestamptz default CURRENT_TIMESTAMP not null);

create table "account" ("id" text not null primary key, "accountId" text not null, "providerId" text not null, "userId" text not null references "user" ("id") on delete cascade, "accessToken" text, "refreshToken" text, "idToken" text, "accessTokenExpiresAt" timestamptz, "refreshTokenExpiresAt" timestamptz, "scope" text, "password" text, "createdAt" timestamptz default CURRENT_TIMESTAMP not null, "updatedAt" timestamptz not null);

create index "account_userId_idx" on "account" ("userId");
````

В `apps/server/package.json` заменить:

````json
  "dependencies": {
    "@ferrum/protocol": "file:../../packages/protocol",
    "pg": "8.23.1"
  },
  "devDependencies": {
    "@types/bun": "1.4.3",
    "@types/pg": "8.23.1"
  }
````

на:

````json
  "dependencies": {
    "@ferrum/protocol": "file:../../packages/protocol",
    "better-auth": "1.7.16",
    "nodemailer": "10.2.0",
    "pg": "8.23.1"
  },
  "devDependencies": {
    "@types/bun": "1.4.3",
    "@types/nodemailer": "8.0.2",
    "@types/pg": "8.23.1"
  }
````

В `apps/server/src/app.ts` заменить:

````ts

import type { Pool } from 'pg';
import { serveStatic, withHeaders } from './static';
import { valkeyAlive } from './valkey';
````

на:

````ts

import type { Pool } from 'pg';
import { authRoutes } from './auth-routes';
import type { Auth } from './auth';
import { serveStatic, withHeaders } from './static';
import { valkeyAlive } from './valkey';
````

В `apps/server/src/app.ts` заменить:

````ts
  valkey: Bun.RedisClient;
  webDist: string;
}

````

на:

````ts
  valkey: Bun.RedisClient;
  webDist: string;
  auth: Auth;
  publicUrl: string;
  trustProxy: boolean;
}

````

В `apps/server/src/app.ts` заменить:

````ts

export function startServer(deps: AppDeps, port: number): Bun.Server<undefined> {
  return Bun.serve({
    port,
    routes: {
      '/healthz': async () => withHeaders(await health(deps)),
    },
    fetch: async (req) => withHeaders(await serveStatic(req, deps.webDist)),
````

на:

````ts

export function startServer(deps: AppDeps, port: number): Bun.Server<undefined> {
  const handleAuth = authRoutes(deps);
  return Bun.serve({
    port,
    routes: {
      '/healthz': async () => withHeaders(await health(deps)),
      '/api/auth/*': async (req, server) => withHeaders(await handleAuth(req, server.requestIP(req)?.address)),
    },
    fetch: async (req) => withHeaders(await serveStatic(req, deps.webDist)),
````

Создать `apps/server/src/auth-routes.ts`:

````ts
// Маршруты входа. Наружу открыт только этот список: остальные точки Better Auth (их десятки,
// в том числе GET, тратящий токен ссылки) недоступны снаружи. Всё, что меняет состояние,
// проходит проверку `Origin`; входные данные — разбор из `packages/protocol`.

import {
  API_CONFIRM,
  API_NAME,
  API_SESSION,
  API_SIGN_IN,
  API_SIGN_OUT,
  API_SIGN_OUT_EVERYWHERE,
  parseConfirmRequest,
  parseNameRequest,
  parseSignInRequest,
  type AuthErrorBody,
  type AuthErrorCode,
  type SessionResponse,
} from '@ferrum/protocol';
import { CLIENT_IP_HEADER, type Auth } from './auth';

/** Запросы входа маленькие: всё, что больше, отвергается, не дочитываясь. */
const MAX_BODY_BYTES = 4096;

export function originAllowed(req: Request, publicUrl: string): boolean {
  return req.headers.get('origin') === publicUrl;
}

function fail(status: number, error: AuthErrorCode): Response {
  return Response.json({ error } satisfies AuthErrorBody, { status });
}

async function readJson(req: Request): Promise<unknown> {
  if (!req.body) return null;
  const chunks: Uint8Array[] = [];
  let size = 0;
  for await (const chunk of req.body) {
    size += chunk.length;
    if (size > MAX_BODY_BYTES) return null;
    chunks.push(chunk);
  }
  try {
    return JSON.parse(new TextDecoder().decode(Buffer.concat(chunks)));
  } catch {
    return null;
  }
}

/** Адрес клиента для счётчиков частоты. Чужому заголовку верим только за своим прокси:
 * последний адрес в `X-Forwarded-For` дописал он, остальные мог прислать кто угодно. */
export function clientIp(req: Request, peer: string | undefined, trustProxy: boolean): string {
  if (trustProxy) {
    const last = req.headers.get('x-forwarded-for')?.split(',').at(-1)?.trim();
    if (last) return last;
  }
  return peer ?? 'unknown';
}

export interface AuthRoutesDeps {
  auth: Auth;
  publicUrl: string;
  trustProxy: boolean;
}

export function authRoutes(deps: AuthRoutesDeps) {
  const { auth, publicUrl } = deps;

  /** Запрос к Better Auth от имени клиента: только нужные заголовки и адрес, поставленный сервером. */
  function inner(req: Request, ip: string, path: string, init: { method: string; body?: unknown }): Promise<Response> {
    const headers = new Headers({ [CLIENT_IP_HEADER]: ip, origin: publicUrl });
    for (const name of ['cookie', 'user-agent']) {
      const v = req.headers.get(name);
      if (v) headers.set(name, v);
    }
    if (init.body !== undefined) headers.set('content-type', 'application/json');
    return auth.handler(
      new Request(`${publicUrl}/api/auth${path}`, {
        method: init.method,
        headers,
        body: init.body === undefined ? undefined : JSON.stringify(init.body),
        redirect: 'manual',
      }),
    );
  }

  /** Ответ для браузера: своё тело и cookie из ответа Better Auth. Тело Better Auth наружу
   * не идёт: в нём токен сессии, который должен оставаться только в cookie `HttpOnly`. */
  function respond(from: Response[], body: unknown, status = 200): Response {
    const res = Response.json(body, { status, headers: { 'Cache-Control': 'no-store' } });
    for (const r of from) for (const c of r.headers.getSetCookie()) res.headers.append('Set-Cookie', c);
    return res;
  }

  function limited(r: Response): Response {
    const res = fail(429, 'TOO_MANY_REQUESTS');
    const retry = r.headers.get('x-retry-after');
    if (retry) res.headers.set('Retry-After', retry);
    return res;
  }

  async function session(req: Request, ip: string): Promise<Response> {
    const r = await inner(req, ip, '/get-session', { method: 'GET' });
    if (r.status === 429) return limited(r);
    // Сбой Valkey — не «сессии нет»: иначе человека молча выбросило бы на экран входа.
    if (!r.ok) throw new Error(`Better Auth: чтение сессии вернуло ${r.status}`);
    const data = (await r.json()) as { user?: { id: string; email: string; name: string } } | null;
    const user = data?.user ? { id: data.user.id, email: data.user.email, name: data.user.name } : null;
    return respond([r], { user } satisfies SessionResponse);
  }

  async function signIn(req: Request, ip: string): Promise<Response> {
    const parsed = parseSignInRequest(await readJson(req));
    if (!parsed) return fail(400, 'BAD_REQUEST');
    const r = await inner(req, ip, '/sign-in/magic-link', {
      method: 'POST',
      body: { email: parsed.email, metadata: parsed.next ? { next: parsed.next } : undefined },
    });
    if (r.status === 429) return limited(r);
    if (!r.ok) throw new Error(`Better Auth: отправка ссылки вернула ${r.status}: ${await r.text()}`);
    return respond([r], { ok: true });
  }

  /** Токен тратится только здесь, по нажатию «Войти»: сканер почты, открывший ссылку, увидит
   * страницу подтверждения, а POST с верным `Origin` он не пришлёт. */
  async function confirm(req: Request, ip: string): Promise<Response> {
    const parsed = parseConfirmRequest(await readJson(req));
    if (!parsed) return fail(400, 'BAD_REQUEST');
    const r = await inner(req, ip, `/magic-link/verify?token=${encodeURIComponent(parsed.token)}`, { method: 'GET' });
    if (r.status === 429) return limited(r);
    // Better Auth сообщает о неверном токене перенаправлением с `?error=`.
    if (!r.ok) return fail(400, 'INVALID_TOKEN');
    return respond([r], { ok: true });
  }

  async function name(req: Request, ip: string): Promise<Response> {
    const parsed = parseNameRequest(await readJson(req));
    if (!parsed) return fail(400, 'BAD_REQUEST');
    const r = await inner(req, ip, '/update-user', { method: 'POST', body: { name: parsed.name } });
    if (r.status === 429) return limited(r);
    if (r.status === 401) return fail(401, 'UNAUTHORIZED');
    if (!r.ok) throw new Error(`Better Auth: смена имени вернула ${r.status}: ${await r.text()}`);
    return session(req, ip);
  }

  async function signOut(req: Request, ip: string): Promise<Response> {
    const r = await inner(req, ip, '/sign-out', { method: 'POST', body: {} });
    if (r.status === 429) return limited(r);
    return respond([r], { ok: true });
  }

  /** Все устройства: сессии снимает `/revoke-sessions`, cookie этого браузера убирает `/sign-out`. */
  async function signOutEverywhere(req: Request, ip: string): Promise<Response> {
    const revoked = await inner(req, ip, '/revoke-sessions', { method: 'POST', body: {} });
    if (revoked.status === 429) return limited(revoked);
    if (revoked.status === 401) return fail(401, 'UNAUTHORIZED');
    if (!revoked.ok) throw new Error(`Better Auth: снятие сессий вернуло ${revoked.status}: ${await revoked.text()}`);
    const out = await inner(req, ip, '/sign-out', { method: 'POST', body: {} });
    return respond([revoked, out], { ok: true });
  }

  return async function handle(req: Request, peer: string | undefined): Promise<Response> {
    const ip = clientIp(req, peer, deps.trustProxy);
    const path = new URL(req.url).pathname;
    if (req.method === 'GET' && path === API_SESSION) return session(req, ip);
    const post: Record<string, (req: Request, ip: string) => Promise<Response>> = {
      [API_SIGN_IN]: signIn,
      [API_CONFIRM]: confirm,
      [API_NAME]: name,
      [API_SIGN_OUT]: signOut,
      [API_SIGN_OUT_EVERYWHERE]: signOutEverywhere,
    };
    const route = req.method === 'POST' && Object.hasOwn(post, path) ? post[path] : null;
    if (!route) return fail(404, 'NOT_FOUND');
    if (!originAllowed(req, publicUrl)) return fail(403, 'BAD_ORIGIN');
    return route(req, ip);
  };
}

export interface UpgradeSession {
  userId: string;
  sessionId: string;
}

/** Проверка перед открытием WebSocket (спека, разделы 5 и 7.1): верный `Origin` и живая сессия
 * в Valkey. Сам WebSocket синхронизации — этап 3. */
export async function authorizeUpgrade(req: Request, auth: Auth, publicUrl: string): Promise<UpgradeSession | null> {
  if (!originAllowed(req, publicUrl)) return null;
  const s = await auth.api.getSession({ headers: req.headers, query: { disableRefresh: true } });
  return s ? { userId: s.user.id, sessionId: s.session.id } : null;
}
````

Создать `apps/server/src/auth.ts`:

````ts
// Better Auth: magic link, пользователи в Postgres, сессии и токены — только в Valkey
// (спека подсистемы 4, раздел 5).

import { confirmHash, MAGIC_LINK_TTL_SECONDS } from '@ferrum/protocol';
import { APIError, betterAuth, type BetterAuthOptions } from 'better-auth';
import { magicLink } from 'better-auth/plugins';
import type { Pool } from 'pg';
import { magicLinkMail, type Mailer } from './mail';
import { valkeyStorage } from './secondary-storage';

/** Заголовок с адресом клиента: сервер ставит его сам (`clientIp`), чужой затирает. */
export const CLIENT_IP_HEADER = 'x-ferrum-client-ip';
const SESSION_SECONDS = 30 * 24 * 60 * 60;
const HOUR = 60 * 60;
/** Писем на адрес и на IP в час, спека, раздел 5. */
export const MAILS_PER_ADDRESS = 5;
export const MAILS_PER_IP = 20;

export interface AuthDeps {
  publicUrl: string;
  secret: string;
  pool: Pool;
  valkey: Bun.RedisClient;
  mailer: Mailer;
  allowedEmails: string[];
  /** Тесты сокращают сроки; по умолчанию — из спеки. */
  magicLinkSeconds?: number;
  sessionUpdateAgeSeconds?: number;
}

/** Пустой список — войти может любой; иначе адрес целиком или его домен. */
export function emailAllowed(email: string, allowed: string[]): boolean {
  if (allowed.length === 0) return true;
  return allowed.includes(email) || allowed.includes(email.slice(email.lastIndexOf('@') + 1));
}

export function authOptions(deps: AuthDeps) {
  const storage = valkeyStorage(deps.valkey);
  return {
    baseURL: deps.publicUrl,
    basePath: '/api/auth',
    secret: deps.secret,
    trustedOrigins: [deps.publicUrl],
    database: deps.pool,
    secondaryStorage: storage,
    verification: { storeInDatabase: false },
    telemetry: { enabled: false },
    session: { expiresIn: SESSION_SECONDS, updateAge: deps.sessionUpdateAgeSeconds ?? 24 * HOUR },
    rateLimit: {
      enabled: true,
      storage: 'secondary-storage',
      window: 60,
      max: 100,
      customRules: {
        '/sign-in/magic-link': { window: HOUR, max: MAILS_PER_IP },
        '/magic-link/verify': { window: 60, max: 30 },
      },
    },
    advanced: {
      cookiePrefix: 'ferrum',
      useSecureCookies: deps.publicUrl.startsWith('https://'),
      ipAddress: { ipAddressHeaders: [CLIENT_IP_HEADER] },
    },
    plugins: [
      magicLink({
        expiresIn: deps.magicLinkSeconds ?? MAGIC_LINK_TTL_SECONDS,
        storeToken: 'hashed',
        sendMagicLink: async ({ email, token, metadata }) => {
          // Адрес не из списка: ответ тот же, письма нет, ничего не раскрываем.
          if (!emailAllowed(email, deps.allowedEmails)) return;
          // Встроенный `rateLimit` считает только по IP и пути; счётчик на адрес — рядом,
          // в том же хранилище и после проверки IP.
          const sent = await storage.increment(`mail-to:${email}`, HOUR);
          if (sent > MAILS_PER_ADDRESS) throw new APIError('TOO_MANY_REQUESTS');
          const next = typeof metadata?.next === 'string' ? metadata.next : undefined;
          await deps.mailer.send(magicLinkMail(email, `${deps.publicUrl}/${confirmHash(token, next)}`));
        },
      }),
    ],
  } satisfies BetterAuthOptions;
}

export function createAuth(deps: AuthDeps) {
  return betterAuth(authOptions(deps));
}

export type Auth = ReturnType<typeof createAuth>;
````

В `apps/server/src/index.ts` заменить:

````ts
import { Pool } from 'pg';
import { startServer } from './app';
import { FsBlobStore } from './blob-store';
import { loadConfig } from './config';
import { migrate } from './migrate';
import { connectValkey } from './valkey';
````

на:

````ts
import { Pool } from 'pg';
import { startServer } from './app';
import { createAuth } from './auth';
import { FsBlobStore } from './blob-store';
import { loadConfig } from './config';
import { smtpMailer } from './mail';
import { migrate } from './migrate';
import { connectValkey } from './valkey';
````

В `apps/server/src/index.ts` заменить:

````ts
const config = loadConfig();
const pool = new Pool({ connectionString: config.databaseUrl });
const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
if (applied.length > 0) console.log(`Миграции применены: ${applied.join(', ')}`);
````

на:

````ts
const config = loadConfig();
const pool = new Pool({ connectionString: config.databaseUrl });
// Обрыв простаивающего соединения: пул создаст новое, но без обработчика процесс бы упал.
pool.on('error', (e) => console.error('Postgres:', e.message));
const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
if (applied.length > 0) console.log(`Миграции применены: ${applied.join(', ')}`);
````

В `apps/server/src/index.ts` заменить:

````ts
// Хранилище байтов нужно с этапа 2; здесь — проверка, что том доступен для записи.
await new FsBlobStore(config.dataDir).put('healthcheck', new Uint8Array([1]));
const server = startServer({ pool, valkey, webDist: config.webDist }, config.port);
console.log(`Ferrum слушает порт ${server.port}`);
````

на:

````ts
// Хранилище байтов нужно с этапа 2; здесь — проверка, что том доступен для записи.
await new FsBlobStore(config.dataDir).put('healthcheck', new Uint8Array([1]));
// После миграций: Better Auth при создании сверяет схему базы.
const auth = createAuth({
  publicUrl: config.publicUrl,
  secret: config.authSecret,
  pool,
  valkey,
  mailer: smtpMailer(config.smtpUrl, config.mailFrom),
  allowedEmails: config.allowedEmails,
});
const server = startServer(
  { pool, valkey, webDist: config.webDist, auth, publicUrl: config.publicUrl, trustProxy: config.trustProxy },
  config.port,
);
console.log(`Ferrum слушает порт ${server.port}`);

// Остановка контейнера: дождаться запросов и закрыть соединения, а не ждать SIGKILL.
for (const signal of ['SIGTERM', 'SIGINT'] as const) {
  process.on(signal, () => {
    void server
      .stop()
      .then(() => pool.end())
      .finally(() => process.exit(0));
  });
}
````

Создать `apps/server/src/mail.ts`:

````ts
// Письма: `nodemailer` по SMTP_URL. В разработке их перехватывает Mailpit.

import nodemailer from 'nodemailer';

export interface Mail {
  to: string;
  subject: string;
  text: string;
  html: string;
}

export interface Mailer {
  send(mail: Mail): Promise<void>;
}

export function smtpMailer(url: string, from: string): Mailer {
  const transport = nodemailer.createTransport(url);
  return {
    send: async (mail) => {
      await transport.sendMail({ from, ...mail });
    },
  };
}

const ESCAPES: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
const escapeHtml = (s: string) => s.replace(/[&<>"']/g, (c) => ESCAPES[c]);

export function magicLinkMail(to: string, link: string): Mail {
  const a = escapeHtml(link);
  return {
    to,
    subject: 'Вход в Ferrum',
    text: `Чтобы войти в Ferrum, откройте ссылку:\n\n${link}\n\nОна действует 15 минут и только один раз. Если вы не просили вход, просто удалите это письмо.\n`,
    html: `<p>Чтобы войти в Ferrum, откройте ссылку:</p><p><a href="${a}">Войти в Ferrum</a></p><p>Она действует 15 минут и только один раз. Если вы не просили вход, просто удалите это письмо.</p>`,
  };
}
````

Создать `apps/server/src/secondary-storage.ts`:

````ts
// Хранилище Better Auth в Valkey на встроенном `Bun.RedisClient`: сессии, токены входа,
// счётчики частоты. `getAndDelete` (`GETDEL`) делает ссылку из письма одноразовой, а
// `increment` одним скриптом не даёт счётчику остаться без срока.

import type { SecondaryStorage } from 'better-auth';

const INCREMENT = "local v = redis.call('INCR', KEYS[1]); if v == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end; return v";

export function valkeyStorage(client: Bun.RedisClient): SecondaryStorage {
  return {
    get: (key) => client.get(key),
    getAndDelete: (key) => client.getdel(key),
    increment: async (key, ttl) => {
      if (!Number.isInteger(ttl) || ttl <= 0) throw new TypeError('Срок счётчика — целое число секунд больше нуля');
      return Number(await client.send('EVAL', [INCREMENT, '1', key, String(ttl)]));
    },
    set: async (key, value, ttl) => {
      if (ttl) await client.send('SET', [key, value, 'EX', String(ttl)]);
      else await client.set(key, value);
    },
    delete: async (key) => {
      await client.del(key);
    },
  };
}
````

В `docker-compose.test.yml` заменить:

````yaml
    ports: !override
      - '127.0.0.1:18025:8025'
    restart: 'no'
````

на:

````yaml
    ports: !override
      - '127.0.0.1:18025:8025'
      - '127.0.0.1:11025:1025'
    restart: 'no'
````

В `packages/protocol/src/auth.ts` заменить:

````ts
export const API_SIGN_IN = '/api/auth/sign-in/magic-link';
export const API_CONFIRM = '/api/auth/confirm';
export const API_SESSION = '/api/auth/get-session';
export const API_UPDATE_USER = '/api/auth/update-user';
export const API_SIGN_OUT = '/api/auth/sign-out';
export const API_SIGN_OUT_EVERYWHERE = '/api/auth/revoke-sessions';

/** Адрес не длиннее, чем позволяет RFC 5321. */
````

на:

````ts
export const API_SIGN_IN = '/api/auth/sign-in/magic-link';
export const API_CONFIRM = '/api/auth/confirm';
export const API_SESSION = '/api/auth/session';
export const API_NAME = '/api/auth/name';
export const API_SIGN_OUT = '/api/auth/sign-out';
export const API_SIGN_OUT_EVERYWHERE = '/api/auth/sign-out-everywhere';

/** Адрес не длиннее, чем позволяет RFC 5321. */
````

В `packages/protocol/src/auth.ts` заменить:

````ts
const CONTROL_RE = /[\u0000-\u001f\u007f]/;

export type AuthErrorCode = 'BAD_REQUEST' | 'BAD_ORIGIN' | 'INVALID_TOKEN' | 'TOO_MANY_REQUESTS' | 'UNAUTHORIZED';

export interface AuthErrorBody {
````

на:

````ts
const CONTROL_RE = /[\u0000-\u001f\u007f]/;

export type AuthErrorCode = 'BAD_REQUEST' | 'BAD_ORIGIN' | 'INVALID_TOKEN' | 'TOO_MANY_REQUESTS' | 'UNAUTHORIZED' | 'NOT_FOUND';

export interface AuthErrorBody {
````

В `packages/protocol/src/auth.ts` заменить:

````ts
  id: string;
  email: string;
  name: string;
}

````

на:

````ts
  id: string;
  email: string;
  /** Пусто, пока человек не прошёл экран первого входа. */
  name: string;
}

/** Ответ `GET /api/auth/session`: без токена сессии, он живёт только в cookie `HttpOnly`. */
export interface SessionResponse {
  user: SessionUser | null;
}

````

- [ ] **Step 4: Сгенерировать файлы**

Run: `cd apps/server && bun install --frozen-lockfile`
Expected: зависимости установлены по `apps/server/bun.lock`

- [ ] **Step 5: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `bun test`: `packages/protocol` — 6 pass, `apps/server` — 35 pass, в том числе настоящее письмо в Mailpit и настоящий WebSocket

- [ ] **Step 6: Закоммитить**

```bash
git add apps/server/auth-schema.ts apps/server/bun.lock apps/server/migrations/002_better_auth.sql apps/server/package.json apps/server/src/app.ts apps/server/src/auth-routes.ts apps/server/src/auth.ts apps/server/src/index.ts apps/server/src/mail.ts apps/server/src/secondary-storage.ts apps/server/test/auth.test.ts apps/server/test/helpers.ts apps/server/test/migrate.test.ts apps/server/test/server.test.ts docker-compose.test.yml packages/protocol/src/auth.ts
git commit -F - <<'EOF'
feat(server): вход — Better Auth, magic link, сессии в Valkey

Better Auth 1.7.16 на `pg` и `Bun.RedisClient`: токены хэшем и `GETDEL`, срок 15 минут,
таблицы только `user` и `account`. Письмо ведёт на страницу подтверждения; токен тратит
`POST /api/auth/confirm`, точка Better Auth с GET снаружи закрыта, как и остальные: наружу
открыт короткий список маршрутов с проверкой `Origin` и разбором из packages/protocol.
Частота: 20 писем на IP (`rateLimit`) и 5 на адрес (свой счётчик в том же Valkey),
`ALLOWED_EMAILS`, выход на устройстве и на всех, `authorizeUpgrade` для будущего WebSocket.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 6: Интерфейс входа

Спека §5: экран входа, «Проверьте почту», повтор через 60 с, неактивные Google и GitHub, страница подтверждения, экран имени, обязательный вход с возвратом, выход в настройках. Токены и классы дизайн-системы.

**Files:**
- Create: `apps/web/src/ui/auth/account.tsx`, `apps/web/src/ui/auth/auth-api.ts`, `apps/web/src/ui/auth/auth-gate.tsx`, `apps/web/src/ui/auth/auth-logic.ts`, `apps/web/src/ui/auth/session.ts`, `apps/web/src/ui/extensions.ts`, `apps/web/src/ui/styles/auth.css`
- Modify: `apps/web/package-lock.json`, `apps/web/package.json`, `apps/web/scripts/check-dist.mjs`, `apps/web/src/main.tsx`, `apps/web/src/ui/files-screen.tsx`, `apps/web/src/ui/settings-dialog.tsx`, `apps/web/vite.config.ts`
- Test: `apps/web/unit/auth-logic.test.ts` (новый), `apps/web/unit/check-dist.test.ts`

**Interfaces:**
- Consumes: маршруты задачи 5, `@ferrum/protocol`, `dismissSplash`, `.fe-btn`, `.fe-field`, `.fe-tooltip`.
- Produces: `AuthGate`, `screenFor(hash, user)`, `resendLeft(sentAt, now)`, `failureText`, `initials`, `avatarHue`; `currentUser`, `leave(everywhere)`, `onAuthChange`, `announceAuthChange`; `Avatar`, `AccountTab`, `AccountRow`; `extensions.{accountTab, accountRow}`; `AUTH_MARKER`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/unit/auth-logic.test.ts`:

````ts
import { expect, test } from '@playwright/test';
import { confirmHash, RESEND_SECONDS } from '@ferrum/protocol';
import { avatarHue, failureText, initials, resendLeft, screenFor } from '../src/ui/auth/auth-logic';

const USER = { id: 'u1', email: 'ivan@example.com', name: 'Иван' };
const TOKEN = 'a'.repeat(32);

test('без сессии — вход, запрошенный адрес запоминается; служебные адреса — нет', () => {
  expect(screenFor('#/file/abc', null)).toEqual({ kind: 'login', next: '#/file/abc' });
  expect(screenFor('', null)).toEqual({ kind: 'login', next: null });
  expect(screenFor('#/', null)).toEqual({ kind: 'login', next: '#/' });
});

test('с сессией: приложение, а без имени — сначала экран имени', () => {
  expect(screenFor('#/file/abc', USER)).toEqual({ kind: 'app', user: USER });
  expect(screenFor('#/file/abc', { ...USER, name: '' })).toEqual({ kind: 'name', user: { ...USER, name: '' } });
});

test('страница подтверждения видна и с сессией, и без неё', () => {
  const hash = confirmHash(TOKEN, '#/file/abc');
  expect(screenFor(hash, null)).toEqual({ kind: 'confirm', token: TOKEN, next: '#/file/abc' });
  expect(screenFor(hash, USER)).toEqual({ kind: 'confirm', token: TOKEN, next: '#/file/abc' });
  expect(screenFor('#/auth/confirm?token=short', null)).toEqual({ kind: 'confirm', token: null, next: null });
});

test('повторная отправка — через 60 с, секунды округляются вверх', () => {
  expect(RESEND_SECONDS).toBe(60);
  expect(resendLeft(1000, 1000)).toBe(60);
  expect(resendLeft(1000, 1001)).toBe(60);
  expect(resendLeft(1000, 59_500)).toBe(2);
  expect(resendLeft(1000, 61_000)).toBe(0);
  expect(resendLeft(1000, 99_000)).toBe(0);
});

test('у каждой ошибки — понятный текст', () => {
  expect(failureText('INVALID_TOKEN')).toContain('недействительна или устарела');
  expect(failureText('TOO_MANY_REQUESTS')).toContain('Слишком много');
  expect(failureText('NETWORK')).toContain('Сервер недоступен');
  for (const f of ['BAD_REQUEST', 'BAD_ORIGIN', 'UNAUTHORIZED', 'NOT_FOUND'] as const) expect(failureText(f).length).toBeGreaterThan(5);
});

test('инициалы и оттенок кружка', () => {
  expect(initials({ name: 'Иван Петров', email: 'x@y.ru' })).toBe('ИП');
  expect(initials({ name: 'иван', email: 'x@y.ru' })).toBe('И');
  expect(initials({ name: 'Анна Мария Шмидт', email: 'x@y.ru' })).toBe('АМ');
  expect(initials({ name: '', email: 'ivan@y.ru' })).toBe('I');
  expect(avatarHue('u1')).toBe(avatarHue('u1'));
  expect(avatarHue('u1')).not.toBe(avatarHue('u2'));
  expect(avatarHue('длинный-идентификатор')).toBeLessThan(360);
});
````

В `apps/web/unit/check-dist.test.ts` заменить:

````ts
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { checkDist, LOCAL_MARKER } from '../scripts/check-dist.mjs';

function dist(files: Record<string, string>): string {
````

на:

````ts
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AUTH_MARKER, checkDist, LOCAL_MARKER } from '../scripts/check-dist.mjs';

function dist(files: Record<string, string>): string {
````

В `apps/web/unit/check-dist.test.ts` заменить:

````ts
});

test('рабочая сборка: без стенда и кода автономного режима', () => {
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': 'x' }), 'server')).toBeNull();
  expect(checkDist(dist({ 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': 'x' }), 'server')).toContain(
    'debug.html в рабочей сборке',
  );
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `x.${LOCAL_MARKER}=1` }), 'server')).toContain(
    'есть код автономного режима',
  );
````

на:

````ts
});

test('автономная сборка: без кода входа', () => {
  const files = { 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': `x.${LOCAL_MARKER}=1;f('${AUTH_MARKER}')` };
  expect(checkDist(dist(files), 'standalone')).toContain('есть код входа');
});

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

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/auth-logic.test.ts unit/check-dist.test.ts`
Expected: FAIL — `Cannot find package @ferrum/protocol` и `check-dist.mjs does not provide an export named AUTH_MARKER`, «No tests found»

- [ ] **Step 3: Реализация**

В `apps/web/package-lock.json` заменить:

````json
      "version": "0.0.0",
      "dependencies": {
        "@fontsource-variable/onest": "5.3.1",
        "@fontsource/jetbrains-mono": "5.3.0",
````

на:

````json
      "version": "0.0.0",
      "dependencies": {
        "@ferrum/protocol": "file:../../packages/protocol",
        "@fontsource-variable/onest": "5.3.1",
        "@fontsource/jetbrains-mono": "5.3.0",
````

В `apps/web/package-lock.json` заменить:

````json
        "vite-plugin-solid": "2.11.14"
      }
    },
    "node_modules/@babel/code-frame": {
````

на:

````json
        "vite-plugin-solid": "2.11.14"
      }
    },
    "../../packages/protocol": {
      "name": "@ferrum/protocol",
      "version": "0.0.0"
    },
    "node_modules/@babel/code-frame": {
````

В `apps/web/package-lock.json` заменить:

````json
        "node": ">=6.9.0"
      }
    },
    "node_modules/@fontsource-variable/onest": {
````

на:

````json
        "node": ">=6.9.0"
      }
    },
    "node_modules/@ferrum/protocol": {
      "resolved": "../../packages/protocol",
      "link": true
    },
    "node_modules/@fontsource-variable/onest": {
````

В `apps/web/package.json` заменить:

````json
  },
  "dependencies": {
    "@fontsource-variable/onest": "5.3.1",
    "@fontsource/jetbrains-mono": "5.3.0",
````

на:

````json
  },
  "dependencies": {
    "@ferrum/protocol": "file:../../packages/protocol",
    "@fontsource-variable/onest": "5.3.1",
    "@fontsource/jetbrains-mono": "5.3.0",
````

В `apps/web/scripts/check-dist.mjs` заменить:

````js
/** Метка, которую оставляет только ветка автономного режима в `main.tsx`. */
export const LOCAL_MARKER = 'ferrumLocal';

/** Проблема сборки словами или `null`. `mode` — `standalone` или `server`. */
````

на:

````js
/** Метка, которую оставляет только ветка автономного режима в `main.tsx`. */
export const LOCAL_MARKER = 'ferrumLocal';
/** Адреса входа: код входа есть только в рабочей сборке. */
export const AUTH_MARKER = '/api/auth/';

/** Проблема сборки словами или `null`. `mode` — `standalone` или `server`. */
````

В `apps/web/scripts/check-dist.mjs` заменить:

````js
  const assets = join(dist, 'assets');
  const code = existsSync(assets) ? readdirSync(assets).filter((f) => f.endsWith('.js')) : [];
  const local = code.some((f) => readFileSync(join(assets, f), 'utf8').includes(LOCAL_MARKER));
  if (mode === 'standalone' && !local) return 'в автономной сборке нет автономного режима: проверьте VITE_FERRUM_LOCAL (.env.standalone)';
  if (mode === 'server' && local) return 'в рабочей сборке есть код автономного режима: VITE_FERRUM_LOCAL задан при сборке?';
  return null;
}
````

на:

````js
  const assets = join(dist, 'assets');
  const code = existsSync(assets) ? readdirSync(assets).filter((f) => f.endsWith('.js')) : [];
  const has = (marker) => code.some((f) => readFileSync(join(assets, f), 'utf8').includes(marker));
  const local = has(LOCAL_MARKER);
  const auth = has(AUTH_MARKER);
  if (mode === 'standalone' && !local) return 'в автономной сборке нет автономного режима: проверьте VITE_FERRUM_LOCAL (.env.standalone)';
  if (mode === 'standalone' && auth) return 'в автономной сборке есть код входа: проверьте условия по VITE_FERRUM_LOCAL';
  if (mode === 'server' && local) return 'в рабочей сборке есть код автономного режима: VITE_FERRUM_LOCAL задан при сборке?';
  if (mode === 'server' && !auth) return 'в рабочей сборке нет кода входа';
  return null;
}
````

В `apps/web/src/main.tsx` заменить:

````tsx
import './ui/styles/properties.css';
import './ui/styles/files.css';
import { render } from 'solid-js/web';
import { Root } from './ui/root';
import { runSmoke } from './ui/tauri';
````

на:

````tsx
import './ui/styles/properties.css';
import './ui/styles/files.css';
import './ui/styles/auth.css';
import { render } from 'solid-js/web';
import { AccountRow, AccountTab } from './ui/auth/account';
import { AuthGate } from './ui/auth/auth-gate';
import { extensions } from './ui/extensions';
import { Root } from './ui/root';
import { runSmoke } from './ui/tauri';
````

В `apps/web/src/main.tsx` заменить:

````tsx
// Условие — прямо по `import.meta.env`, без константы: сборщик выбрасывает ветку целиком,
// `scripts/check-dist.mjs` проверяет, что в рабочей сборке её нет.
if (import.meta.env.VITE_FERRUM_LOCAL === '1') document.documentElement.dataset.ferrumLocal = '1';
render(() => <Root />, document.getElementById('app')!);
// Десктоп с `FERRUM_SMOKE=1`: проверка запуска (вне Tauri ничего не делает).
void runSmoke();
````

на:

````tsx
// Условие — прямо по `import.meta.env`, без константы: сборщик выбрасывает ветку целиком,
// `scripts/check-dist.mjs` проверяет, что в рабочей сборке её нет.
if (import.meta.env.VITE_FERRUM_LOCAL === '1') {
  document.documentElement.dataset.ferrumLocal = '1';
  render(() => <Root />, document.getElementById('app')!);
} else {
  extensions.accountTab = AccountTab;
  extensions.accountRow = AccountRow;
  render(
    () => (
      <AuthGate>
        <Root />
      </AuthGate>
    ),
    document.getElementById('app')!,
  );
}
// Десктоп с `FERRUM_SMOKE=1`: проверка запуска (вне Tauri ничего не делает).
void runSmoke();
````

Создать `apps/web/src/ui/auth/account.tsx`:

````tsx
// Человек в интерфейсе: кружок с инициалами, вкладка настроек и строка на экране файлов.

import { createSignal, Show } from 'solid-js';
import { avatarHue, initials } from './auth-logic';
import { currentUser, leave } from './session';
import type { SessionUser } from '@ferrum/protocol';

export function Avatar(props: { user: SessionUser }) {
  return (
    <span class="fe-avatar" style={{ '--avatar-hue': avatarHue(props.user.id) }} aria-hidden="true">
      {initials(props.user)}
    </span>
  );
}

/** Вкладка «Аккаунт» настроек: кто вошёл, выход на этом устройстве и на всех. */
export function AccountTab() {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const out = async (everywhere: boolean) => {
    setBusy(true);
    setError(await leave(everywhere));
    setBusy(false);
  };
  return (
    <div class="fe-account">
      <Show when={currentUser()}>
        {(u) => (
          <div class="fe-account__who">
            <Avatar user={u()} />
            <span class="fe-account__text">
              <span class="t-strong">{u().name}</span>
              <span class="t-muted">{u().email}</span>
            </span>
          </div>
        )}
      </Show>
      <div class="fe-account__actions">
        <button class="fe-btn fe-btn--secondary" disabled={busy()} onClick={() => void out(false)}>
          Выйти
        </button>
        <button class="fe-btn fe-btn--secondary" disabled={busy()} onClick={() => void out(true)}>
          Выйти на всех устройствах
        </button>
      </div>
      <Show when={error()}>
        <p class="fe-auth__error" role="alert">
          {error()}
        </p>
      </Show>
    </div>
  );
}

/** Строка внизу панели экрана файлов (поведение: настройки открываются только из редактора,
 * а выйти нужно и отсюда). */
export function AccountRow() {
  const [error, setError] = createSignal<string | null>(null);
  return (
    <Show when={currentUser()}>
      {(u) => (
        <div class="fe-account-row">
          <Avatar user={u()} />
          <span class="fe-account-row__name" title={u().email}>
            {u().name}
          </span>
          <button class="fe-btn" onClick={() => void leave(false).then(setError)}>
            Выйти
          </button>
          <Show when={error()}>
            <p class="fe-auth__error" role="alert">
              {error()}
            </p>
          </Show>
        </div>
      )}
    </Show>
  );
}
````

Создать `apps/web/src/ui/auth/auth-api.ts`:

````ts
// Запросы входа к серверу. Токен сессии в JS не попадает: он в cookie `HttpOnly`.

import {
  API_CONFIRM,
  API_NAME,
  API_SESSION,
  API_SIGN_IN,
  API_SIGN_OUT,
  API_SIGN_OUT_EVERYWHERE,
  type AuthErrorBody,
  type SessionResponse,
  type SessionUser,
} from '@ferrum/protocol';
import type { AuthFailure } from './auth-logic';

export type ApiResult<T> = { ok: true; value: T } | { ok: false; failure: AuthFailure };

async function call<T>(path: string, init?: RequestInit): Promise<ApiResult<T>> {
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch {
    return { ok: false, failure: 'NETWORK' };
  }
  const body = (await res.json().catch(() => null)) as unknown;
  if (res.ok) return { ok: true, value: body as T };
  return { ok: false, failure: (body as AuthErrorBody | null)?.error ?? 'NETWORK' };
}

const post = <T>(path: string, body: unknown) =>
  call<T>(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

export const getSession = () => call<SessionResponse>(API_SESSION);
export const requestLink = (email: string, next: string | null) => post<{ ok: true }>(API_SIGN_IN, next ? { email, next } : { email });
export const confirmLink = (token: string) => post<{ ok: true }>(API_CONFIRM, { token });
export const saveName = (name: string) => post<{ user: SessionUser }>(API_NAME, { name });
export const signOut = () => post<{ ok: true }>(API_SIGN_OUT, {});
export const signOutEverywhere = () => post<{ ok: true }>(API_SIGN_OUT_EVERYWHERE, {});
````

Создать `apps/web/src/ui/auth/auth-gate.tsx`:

````tsx
// Обязательный вход, как в Figma: без сессии открывается только экран входа, запрошенный адрес
// остаётся в адресной строке и уходит в письмо. Приложение монтируется только при живой сессии.
// Весь этот код есть только в рабочей сборке (см. `main.tsx`, `scripts/check-dist.mjs`).

import { createSignal, For, Match, onCleanup, onMount, Show, Switch, type JSX } from 'solid-js';
import { MAX_NAME_CHARS, parseConfirmHash, parseEmail, parseNext, type SessionUser } from '@ferrum/protocol';
import { dismissSplash } from '../boot';
import { confirmLink, getSession, requestLink, saveName } from './auth-api';
import { failureText, resendLeft, screenFor, type AuthFailure, type AuthScreen } from './auth-logic';
import { announceAuthChange, currentUser, onAuthChange, setCurrentUser } from './session';

export function AuthGate(props: { children: JSX.Element }) {
  const [screen, setScreen] = createSignal<AuthScreen | null>(null);
  const [failure, setFailure] = createSignal<AuthFailure | null>(null);

  const load = async () => {
    setFailure(null);
    const hash = location.hash;
    // Страница подтверждения не спрашивает сессию: открытие ссылки ни к чему не обращается.
    const first = screenFor(hash, null);
    if (first.kind === 'confirm') {
      setScreen(first);
      return;
    }
    const r = await getSession();
    if (!r.ok) {
      setFailure(r.failure);
      return;
    }
    const s = screenFor(hash, r.value.user);
    setCurrentUser(r.value.user);
    setScreen(s);
  };

  /** Новый адрес — без перезагрузки: приложение прочтёт его при монтировании. */
  const go = (hash: string) => {
    history.replaceState(null, '', `${location.pathname}${location.search}${hash}`);
    void load();
  };
  const signedIn = (next: string | null) => {
    announceAuthChange();
    go(next ?? '#/');
  };
  const named = (user: SessionUser) => {
    announceAuthChange();
    setCurrentUser(user);
    setScreen({ kind: 'app', user });
  };

  void load();
  // Ссылка из письма вставлена в эту же вкладку: сменился только адрес после `#`, страница не
  // перезагружается. Внутри приложения адресом ведает оно само, кроме страницы подтверждения.
  const onHash = () => {
    if (screen()?.kind !== 'app' || parseConfirmHash(location.hash)) void load();
  };
  window.addEventListener('hashchange', onHash);
  onCleanup(() => window.removeEventListener('hashchange', onHash));
  // Вход или выход в другой вкладке: здесь то же самое. Страница заново — только если человек
  // или его имя правда другие: редактор не закрывается от повторного входа тем же адресом.
  onCleanup(
    onAuthChange(() => {
      if (screen()?.kind === 'confirm') return;
      void getSession().then((r) => {
        if (!r.ok) return;
        const was = currentUser();
        const now = r.value.user;
        if (was?.id !== now?.id || was?.name !== now?.name) location.reload();
      });
    }),
  );

  return (
    <Switch>
      <Match when={failure()}>
        {(f) => (
          <AuthCard title="Не удалось открыть Ferrum">
            <p class="fe-auth__error" role="alert">
              {failureText(f())}
            </p>
            <button class="fe-btn fe-btn--primary fe-auth__wide" onClick={() => void load()}>
              Повторить
            </button>
          </AuthCard>
        )}
      </Match>
      <Match when={screen()?.kind === 'app'}>{props.children}</Match>
      <Match when={screen()} keyed>
        {(s) => (
          <>
            {s.kind === 'login' && <LoginScreen next={s.next} />}
            {s.kind === 'confirm' && (
              <ConfirmScreen token={s.token} next={s.next} onDone={signedIn} onNewLink={(next) => go(next ?? '#/')} />
            )}
            {s.kind === 'name' && <NameScreen user={s.user} onDone={named} />}
          </>
        )}
      </Match>
    </Switch>
  );
}

function AuthCard(props: { title: string; children: JSX.Element }) {
  onMount(() => dismissSplash(true));
  return (
    <div class="fe-auth">
      <main class="fe-auth__card">
        <div class="fe-files__brand">
          <span class="fe-panel-head__mark" aria-hidden="true" />
          <span class="t-strong">Ferrum</span>
        </div>
        <h1 class="fe-auth__title">{props.title}</h1>
        {props.children}
      </main>
    </div>
  );
}

const PROVIDERS = ['Войти через Google', 'Войти через GitHub'];

function LoginScreen(props: { next: string | null }) {
  const [email, setEmail] = createSignal('');
  const [sentTo, setSentTo] = createSignal<string | null>(null);
  const [sentAt, setSentAt] = createSignal(0);
  const [now, setNow] = createSignal(Date.now());
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const timer = setInterval(() => setNow(Date.now()), 1000);
  onCleanup(() => clearInterval(timer));

  const send = async (address: string) => {
    setBusy(true);
    setError(null);
    const r = await requestLink(address, props.next);
    setBusy(false);
    if (!r.ok) {
      setError(failureText(r.failure));
      return;
    }
    setSentTo(address);
    setSentAt(Date.now());
    setNow(Date.now());
  };
  const left = () => resendLeft(sentAt(), now());

  return (
    <Show
      when={sentTo()}
      fallback={
        <AuthCard title="Вход в Ferrum">
          <form
            class="fe-auth__form"
            novalidate
            onSubmit={(e) => {
              e.preventDefault();
              const address = parseEmail(email());
              if (address) void send(address);
              else setError('Проверьте адрес: например, ivan@example.com');
            }}
          >
            <label class="fe-auth__label" for="fe-auth-email">
              Электронная почта
            </label>
            <div class="fe-field fe-field--text fe-auth__field">
              <input
                id="fe-auth-email"
                type="email"
                autocomplete="email"
                autofocus
                required
                value={email()}
                aria-invalid={error() !== null}
                onInput={(e) => setEmail(e.currentTarget.value)}
              />
            </div>
            <Show when={error()}>
              <p class="fe-auth__error" role="alert">
                {error()}
              </p>
            </Show>
            <button class="fe-btn fe-btn--primary fe-auth__wide" type="submit" disabled={busy()}>
              Получить ссылку
            </button>
          </form>
          <div class="fe-auth__or" role="separator">
            или
          </div>
          <For each={PROVIDERS}>
            {(name) => (
              // `aria-disabled`, а не `disabled`: подсказка должна открываться и с клавиатуры.
              <button
                class="fe-btn fe-btn--secondary fe-auth__wide fe-auth__soon"
                type="button"
                aria-disabled="true"
                onClick={(e) => e.preventDefault()}
              >
                {name}
                <span class="fe-tooltip fe-auth__tip" role="tooltip">
                  Скоро
                </span>
              </button>
            )}
          </For>
        </AuthCard>
      }
    >
      {(address) => (
        <AuthCard title="Проверьте почту">
          <p class="fe-auth__text">
            Мы отправили ссылку для входа на <b>{address()}</b>. Она действует 15 минут.
          </p>
          <Show when={error()}>
            <p class="fe-auth__error" role="alert">
              {error()}
            </p>
          </Show>
          <button class="fe-btn fe-btn--secondary fe-auth__wide" disabled={left() > 0 || busy()} onClick={() => void send(address())}>
            {left() > 0 ? `Отправить ещё раз через ${left()} с` : 'Отправить ещё раз'}
          </button>
          <button
            class="fe-btn fe-auth__wide"
            onClick={() => {
              setError(null);
              setSentTo(null);
            }}
          >
            Другой адрес
          </button>
        </AuthCard>
      )}
    </Show>
  );
}

/** Ссылка из письма: кнопка вместо входа по GET, почтовые сканеры открывают ссылки и сожгли бы
 * одноразовый токен. Токен тратится только по нажатию. */
function ConfirmScreen(props: {
  token: string | null;
  next: string | null;
  onDone: (next: string | null) => void;
  onNewLink: (next: string | null) => void;
}) {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(props.token ? null : failureText('INVALID_TOKEN'));
  const enter = async () => {
    setBusy(true);
    const r = await confirmLink(props.token!);
    if (r.ok) {
      props.onDone(props.next);
      return;
    }
    setBusy(false);
    setError(failureText(r.failure));
  };
  return (
    <AuthCard title="Вход в Ferrum">
      <Show
        when={!error()}
        fallback={
          <>
            <p class="fe-auth__error" role="alert">
              {error()}
            </p>
            <button class="fe-btn fe-btn--primary fe-auth__wide" onClick={() => props.onNewLink(parseNext(props.next))}>
              Запросить новую ссылку
            </button>
          </>
        }
      >
        <p class="fe-auth__text">Нажмите кнопку, чтобы завершить вход.</p>
        <button class="fe-btn fe-btn--primary fe-auth__wide" disabled={busy()} onClick={() => void enter()}>
          Войти
        </button>
      </Show>
    </AuthCard>
  );
}

/** Экран первого входа: имя видят другие участники. */
function NameScreen(props: { user: SessionUser; onDone: (user: SessionUser) => void }) {
  const [name, setName] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  return (
    <AuthCard title="Как вас зовут?">
      <p class="fe-auth__text">Это имя увидят те, с кем вы работаете. Вы вошли как {props.user.email}.</p>
      <form
        class="fe-auth__form"
        novalidate
        onSubmit={(e) => {
          e.preventDefault();
          if (!name().trim()) {
            setError('Введите имя');
            return;
          }
          setBusy(true);
          setError(null);
          void saveName(name()).then((r) => {
            setBusy(false);
            if (r.ok) props.onDone(r.value.user);
            else setError(failureText(r.failure));
          });
        }}
      >
        <label class="fe-auth__label" for="fe-auth-name">
          Имя
        </label>
        <div class="fe-field fe-field--text fe-auth__field">
          <input
            id="fe-auth-name"
            autocomplete="name"
            autofocus
            maxlength={MAX_NAME_CHARS}
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
          />
        </div>
        <Show when={error()}>
          <p class="fe-auth__error" role="alert">
            {error()}
          </p>
        </Show>
        <button class="fe-btn fe-btn--primary fe-auth__wide" type="submit" disabled={busy()}>
          Продолжить
        </button>
      </form>
    </AuthCard>
  );
}
````

Создать `apps/web/src/ui/auth/auth-logic.ts`:

````ts
// Чистая логика входа: что показать, сколько ждать до повторной отправки, тексты ошибок,
// кружок с инициалами. Без DOM и сети: это проверяют модульные тесты.

import { parseConfirmHash, parseNext, RESEND_SECONDS, type AuthErrorCode, type SessionUser } from '@ferrum/protocol';

export type AuthScreen =
  | { kind: 'confirm'; token: string | null; next: string | null }
  | { kind: 'login'; next: string | null }
  | { kind: 'name'; user: SessionUser }
  | { kind: 'app'; user: SessionUser };

/** Адрес страницы подтверждения показывается всегда, остальное решает сессия. */
export function screenFor(hash: string, user: SessionUser | null): AuthScreen {
  const confirm = parseConfirmHash(hash);
  if (confirm) return { kind: 'confirm', ...confirm };
  if (!user) return { kind: 'login', next: parseNext(hash) };
  return user.name ? { kind: 'app', user } : { kind: 'name', user };
}

/** Сколько секунд осталось до повторной отправки; `0` — можно. */
export function resendLeft(sentAt: number, now: number): number {
  return Math.max(0, Math.ceil((sentAt + RESEND_SECONDS * 1000 - now) / 1000));
}

export type AuthFailure = AuthErrorCode | 'NETWORK';

export function failureText(f: AuthFailure): string {
  switch (f) {
    case 'BAD_REQUEST':
      return 'Проверьте, правильно ли всё заполнено';
    case 'INVALID_TOKEN':
      return 'Ссылка недействительна или устарела. Запросите новую';
    case 'TOO_MANY_REQUESTS':
      return 'Слишком много запросов. Попробуйте позже';
    case 'NETWORK':
      return 'Сервер недоступен. Проверьте соединение и попробуйте ещё раз';
    case 'BAD_ORIGIN':
    case 'UNAUTHORIZED':
    case 'NOT_FOUND':
      return 'Не получилось. Обновите страницу и попробуйте ещё раз';
  }
}

/** Инициалы для кружка: первые буквы двух слов имени, без имени — буква адреса. */
export function initials(user: Pick<SessionUser, 'name' | 'email'>): string {
  const words = user.name.trim().split(/\s+/).filter(Boolean);
  const letters = words.length > 0 ? words.slice(0, 2).map((w) => Array.from(w)[0]) : [Array.from(user.email)[0] ?? '?'];
  return letters.join('').toUpperCase();
}

/** Оттенок кружка из идентификатора: у человека он один и тот же на всех устройствах. */
export function avatarHue(id: string): number {
  let h = 0;
  for (const c of id) h = (h * 31 + (c.codePointAt(0) ?? 0)) % 360;
  return h;
}
````

Создать `apps/web/src/ui/auth/session.ts`:

````ts
// Текущий пользователь для настроек и экрана файлов, смена входа в других вкладках.

import { createSignal } from 'solid-js';
import type { SessionUser } from '@ferrum/protocol';
import { signOut, signOutEverywhere } from './auth-api';
import { failureText } from './auth-logic';

const [user, setUser] = createSignal<SessionUser | null>(null);
export const currentUser = user;
export const setCurrentUser = setUser;

/** Другие вкладки узнают о входе и выходе (поведение, как в Figma). Канал один на страницу:
 * своих сообщений он не получает. */
let channel: BroadcastChannel | undefined;
const authChannel = () => (channel ??= new BroadcastChannel('ferrum-auth'));

export function announceAuthChange(): void {
  authChannel().postMessage('changed');
}

export function onAuthChange(listener: () => void): () => void {
  authChannel().onmessage = listener;
  return () => (authChannel().onmessage = null);
}

/** Выход: сессия снята — страница заново, она покажет экран входа. Правки открытого файла
 * к этому моменту в журнале, при уходе со страницы редактор допишет остальное. Ошибка — текст. */
export async function leave(everywhere: boolean): Promise<string | null> {
  const r = await (everywhere ? signOutEverywhere() : signOut());
  if (!r.ok) return failureText(r.failure);
  announceAuthChange();
  location.reload();
  return null;
}
````

Создать `apps/web/src/ui/extensions.ts`:

````ts
// Части интерфейса, которые есть только в рабочей сборке. Их ставит `main.tsx` в ветке, которую
// автономная сборка выбрасывает, поэтому код входа в неё не попадает, а экраны о нём не знают.

import type { Component } from 'solid-js';

export const extensions: {
  /** Вкладка «Аккаунт» настроек. */
  accountTab?: Component;
  /** Строка с человеком внизу панели экрана файлов. */
  accountRow?: Component;
} = {};
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
import { downloadBlob, fileName, formatSize, relativeTime } from './files';
import { importFromDisk, type FileMeta, type FileStore, type Section } from './file-store';
import { Icon, type IconName } from './icons';
import { Menu, type MenuItem } from './menu';
````

на:

````tsx
import { downloadBlob, fileName, formatSize, relativeTime } from './files';
import { importFromDisk, type FileMeta, type FileStore, type Section } from './file-store';
import { extensions } from './extensions';
import { Icon, type IconName } from './icons';
import { Menu, type MenuItem } from './menu';
````

В `apps/web/src/ui/files-screen.tsx` заменить:

````tsx
          </For>
        </div>
      </aside>
      <main class="fe-files__main">
````

на:

````tsx
          </For>
        </div>
        {extensions.accountRow && <extensions.accountRow />}
      </aside>
      <main class="fe-files__main">
````

В `apps/web/src/ui/settings-dialog.tsx` заменить:

````tsx
import { useEditor } from './context';
import { downloadBytes } from './files';
import { Icon, type IconName } from './icons';
import { conflictsOf, filterGroups, groupCommands, MAX_KEYMAP_FILE, recordKey, reservedOf } from './keymap-ui';
````

на:

````tsx
import { useEditor } from './context';
import { downloadBytes } from './files';
import { extensions } from './extensions';
import { Icon, type IconName } from './icons';
import { conflictsOf, filterGroups, groupCommands, MAX_KEYMAP_FILE, recordKey, reservedOf } from './keymap-ui';
````

В `apps/web/src/ui/settings-dialog.tsx` заменить:

````tsx
import { shortcutLabel } from './shortcuts';

type Tab = 'keys' | 'theme' | 'canvas';

const TABS: { id: Tab; title: string }[] = [
````

на:

````tsx
import { shortcutLabel } from './shortcuts';

type Tab = 'keys' | 'theme' | 'canvas' | 'account';

const TABS: { id: Tab; title: string }[] = [
````

В `apps/web/src/ui/settings-dialog.tsx` заменить:

````tsx
  const ed = useEditor();
  const [tab, setTab] = createSignal<Tab>('keys');
  let el!: HTMLDivElement;
  const returnFocus = document.activeElement as HTMLElement | null;
````

на:

````tsx
  const ed = useEditor();
  const [tab, setTab] = createSignal<Tab>('keys');
  const Account = extensions.accountTab;
  const tabs: { id: Tab; title: string }[] = Account ? [...TABS, { id: 'account', title: 'Аккаунт' }] : TABS;
  let el!: HTMLDivElement;
  const returnFocus = document.activeElement as HTMLElement | null;
````

В `apps/web/src/ui/settings-dialog.tsx` заменить:

````tsx
          </div>
          <div class="fe-tabs" role="tablist" aria-label="Разделы настроек">
            <For each={TABS}>
              {(t) => (
                <button
````

на:

````tsx
          </div>
          <div class="fe-tabs" role="tablist" aria-label="Разделы настроек">
            <For each={tabs}>
              {(t) => (
                <button
````

В `apps/web/src/ui/settings-dialog.tsx` заменить:

````tsx
                  onClick={() => setTab(t.id)}
                  onKeyDown={(e) => {
                    const i = TABS.findIndex((x) => x.id === tab());
                    const next =
                      e.key === 'ArrowRight'
                        ? (i + 1) % TABS.length
                        : e.key === 'ArrowLeft'
                          ? (i - 1 + TABS.length) % TABS.length
                          : e.key === 'Home'
                            ? 0
                            : e.key === 'End'
                              ? TABS.length - 1
                              : -1;
                    if (next < 0) return;
                    e.preventDefault();
                    setTab(TABS[next].id);
                    el.querySelector<HTMLElement>(`#settings-tab-${TABS[next].id}`)?.focus();
                  }}
                >
````

на:

````tsx
                  onClick={() => setTab(t.id)}
                  onKeyDown={(e) => {
                    const i = tabs.findIndex((x) => x.id === tab());
                    const next =
                      e.key === 'ArrowRight'
                        ? (i + 1) % tabs.length
                        : e.key === 'ArrowLeft'
                          ? (i - 1 + tabs.length) % tabs.length
                          : e.key === 'Home'
                            ? 0
                            : e.key === 'End'
                              ? tabs.length - 1
                              : -1;
                    if (next < 0) return;
                    e.preventDefault();
                    setTab(tabs[next].id);
                    el.querySelector<HTMLElement>(`#settings-tab-${tabs[next].id}`)?.focus();
                  }}
                >
````

В `apps/web/src/ui/settings-dialog.tsx` заменить:

````tsx
              </label>
            </Show>
          </div>
        </div>
````

на:

````tsx
              </label>
            </Show>
            <Show when={Account && tab() === 'account'}>{Account && <Account />}</Show>
          </div>
        </div>
````

Создать `apps/web/src/ui/styles/auth.css`:

````css
/* Экраны входа и аккаунт: карточка на холсте темы, токены и компоненты дизайн-системы.
   Экран входа — единственный, который открывается без сессии. */

.fe-auth {
  display: grid;
  place-items: center;
  height: 100%;
  overflow: auto;
  padding: var(--space-6);
  background: var(--canvas);
}
.fe-auth__card {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  width: 100%;
  max-width: 340px;
  padding: var(--space-6);
  border-radius: var(--radius-lg);
  background: var(--surface);
  box-shadow: var(--shadow-popover);
}
.fe-auth__title {
  margin: 0;
  font-size: 15px;
  line-height: 20px;
  font-weight: 600;
}
.fe-auth__text {
  margin: 0;
  font-size: 13px;
  line-height: 20px;
  color: var(--ink-muted);
  overflow-wrap: anywhere;
}
.fe-auth__text b {
  color: var(--ink);
  font-weight: 600;
}
.fe-auth__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.fe-auth__label {
  font-size: 11px;
  line-height: 16px;
  font-weight: 500;
  color: var(--ink-muted);
}
.fe-auth__field {
  height: 32px;
}
.fe-auth__error {
  margin: 0;
  color: var(--danger);
  overflow-wrap: anywhere;
}
.fe-auth__wide {
  justify-content: center;
  width: 100%;
  height: 32px;
}
.fe-auth__or {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--ink-faint);
}
.fe-auth__or::before,
.fe-auth__or::after {
  content: '';
  flex: 1;
  height: 1px;
  background: var(--line);
}
/* Неактивная кнопка входа через другой сервис; подсказка «Скоро» — чистым CSS: на этом экране
   нет редактора, а значит и слоя подсказок. */
.fe-auth__soon {
  position: relative;
  color: var(--ink-faint);
  cursor: default;
}
.fe-auth__soon:hover,
.fe-auth__soon:active {
  background: transparent;
}
.fe-auth__tip {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 50%;
  translate: -50% 0;
  display: none;
}
.fe-auth__soon:hover .fe-auth__tip,
.fe-auth__soon:focus-visible .fe-auth__tip {
  display: flex;
}

.fe-avatar {
  display: inline-grid;
  place-items: center;
  flex: none;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: hsl(var(--avatar-hue) 55% 42%);
  color: #ffffff;
  font-size: 10px;
  line-height: 1;
  font-weight: 600;
}

.fe-account {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}
.fe-account__who {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
.fe-account__text {
  display: flex;
  flex-direction: column;
  min-width: 0;
  overflow-wrap: anywhere;
}
.fe-account__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.fe-account-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
  margin-top: auto;
}
.fe-account-row__name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
````

В `apps/web/vite.config.ts` заменить:

````ts
  return {
    plugins: [solid()],
    server: { headers },
    preview: { headers },
    worker: { format: 'es' as const },
````

на:

````ts
  return {
    plugins: [solid()],
    // `npm run dev:server`: API — у сервера из Docker (PUBLIC_URL должен быть адресом vite).
    server: { headers, fs: { allow: ['../..'] }, proxy: { '/api': process.env.FERRUM_API ?? 'http://localhost:8090' } },
    preview: { headers },
    worker: { format: 'es' as const },
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm ci`
Expected: `node_modules/@ferrum/protocol` — ссылка на `packages/protocol`

- [ ] **Step 5: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npx playwright test e2e/standalone.spec.ts e2e/settings.spec.ts --project=chromium`

Expected: PASS: всё без замечаний; модульные — 186 passed; обе сборки собраны, в автономной нет кода входа, в рабочей — кода автономного режима; E2E `standalone`, `settings` — без падений

- [ ] **Step 6: Закоммитить**

```bash
git add apps/web/package-lock.json apps/web/package.json apps/web/scripts/check-dist.mjs apps/web/src/main.tsx apps/web/src/ui/auth/account.tsx apps/web/src/ui/auth/auth-api.ts apps/web/src/ui/auth/auth-gate.tsx apps/web/src/ui/auth/auth-logic.ts apps/web/src/ui/auth/session.ts apps/web/src/ui/extensions.ts apps/web/src/ui/files-screen.tsx apps/web/src/ui/settings-dialog.tsx apps/web/src/ui/styles/auth.css apps/web/unit/auth-logic.test.ts apps/web/unit/check-dist.test.ts apps/web/vite.config.ts
git commit -F - <<'EOF'
feat(web): экраны входа, обязательный вход и выход

Экран входа с «Получить ссылку», «Проверьте почту» и повтором через 60 с, неактивные
кнопки Google и GitHub с подсказкой «Скоро», страница подтверждения с кнопкой «Войти»,
экран имени при первом входе. Без сессии приложение не монтируется; запрошенный адрес уходит
в письмо и открывается после входа. Вкладка «Аккаунт» в настройках и строка на экране
файлов: выход на этом устройстве и на всех, другие вкладки узнают через BroadcastChannel.
Всё это есть только в рабочей сборке: `main.tsx` ставит части через `extensions.ts`.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 7: E2E против стенда

Спека §13.2: проект Playwright `server`, письмо через API Mailpit. Сценарии проверяют код задач 3–6 на настоящем стенде, поэтому шага «падают» здесь нет: реализация уже есть. Что они падают без неё, проверено отдельно: образ с автономной сборкой вместо рабочей дал 14 падений из 15, проходит только сценарий `/healthz`, у которого реализация в сервере.

**Files:**
- Create: `apps/web/playwright.server.config.ts`
- Modify: `apps/web/package.json`, `apps/web/tsconfig.e2e.json`
- Test: `apps/web/e2e-server/auth.spec.ts` (новый), `apps/web/e2e-server/global-setup.ts` (новый), `apps/web/e2e-server/global-teardown.ts` (новый), `apps/web/e2e-server/stand.ts` (новый)

**Interfaces:**
- Consumes: `docker-compose.test.yml`, рабочий образ, `mainMenu` из `e2e/helpers.ts`.
- Produces: `npm run e2e:server`; `compose`, `valkey`, `resetStand`, `linksFor`, `mailCount`, `newEmail`.

- [ ] **Step 1: Написать сценарии**

Создать `apps/web/e2e-server/auth.spec.ts`:

````ts
import { expect, test, type Browser, type Page } from '@playwright/test';
import { mainMenu } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { linksFor, mailCount, newEmail, resetStand, valkey } from './stand';

test.beforeEach(resetStand);

/** Ошибки загрузки и нарушения CSP видны как ошибки консоли: на всех экранах их нет. */
function watchConsole(page: Page): string[] {
  const errors: string[] = [];
  page.on('console', (m) => {
    // Ответ 401/404 браузер пишет сам, это не ошибка страницы.
    if (m.type() === 'error' && !m.text().includes('Failed to load resource')) errors.push(m.text());
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
  await requestLink(page, email);
  const [link] = await linksFor(email);
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  if (name) {
    await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
    await page.getByLabel('Имя').fill(name);
    await page.getByRole('button', { name: 'Продолжить' }).click();
  }
}

const newContext = async (browser: Browser) => (await browser.newContext({ baseURL: STAND_URL })).newPage();

test('без сессии виден только экран входа, кнопки Google и GitHub неактивны', async ({ page, request }) => {
  const errors = watchConsole(page);
  for (const path of ['/', '/#/file/abc', '/#/new', '/#/drafts']) {
    await page.goto(path);
    await page.reload();
    await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
    await expect(page.locator('.fe-files, .fe-app')).toHaveCount(0);
    await expect(page.locator('#fe-splash')).toHaveCount(0);
  }
  // Ядро без входа не запускается, стенд ядра в рабочей сборке отсутствует.
  expect(await page.evaluate(() => window.__ferrum)).toBeUndefined();
  expect((await request.get('/debug.html')).status()).toBe(404);

  for (const name of ['Войти через Google', 'Войти через GitHub']) {
    const b = page.getByRole('button', { name });
    await expect(b).toHaveAttribute('aria-disabled', 'true');
    const tip = b.getByRole('tooltip');
    await expect(tip).toBeHidden();
    await b.hover();
    await expect(tip).toBeVisible();
    await expect(tip).toHaveText('Скоро');
    // Playwright ждёт активности у `aria-disabled`: нажимаем как пользователь — по координатам.
    await page.mouse.down();
    await page.mouse.up();
    await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  }
  expect(errors).toEqual([]);
});

test('вход целиком: письмо, подтверждение, имя, сессия, редактор', async ({ page, context }, info) => {
  const errors = watchConsole(page);
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await expect(page.locator('.fe-account-row')).toContainText('Иван Петров');
  await expect(page.locator('.fe-avatar')).toHaveText('ИП');

  // Токен сессии недоступен скриптам.
  const session = (await context.cookies()).find((c) => c.name === 'ferrum.session_token')!;
  expect(session.httpOnly).toBe(true);
  expect(session.sameSite).toBe('Lax');
  expect(session.secure).toBe(false);
  expect(await page.evaluate(() => document.cookie)).toBe('');
  const me = (await (await page.request.get('/api/auth/session')).json()) as { user: { email: string; name: string } };
  expect(me.user).toMatchObject({ email, name: 'Иван Петров' });

  // Перезагрузка: сессия жива, экран имени второй раз не показывается.
  await page.reload();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');

  // Редактор работает в рабочей сборке: изоляция, CSP, wasm, файлы — в IndexedDB, как раньше.
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(true);
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
  await expect(page).toHaveURL(/#\/file\//);
  expect(errors).toEqual([]);
});

test('второй вход тем же адресом: экрана имени нет', async ({ page, browser }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  const other = await newContext(browser);
  await other.goto('/');
  await signIn(other, email, '');
  await expect(other.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await expect(other.locator('.fe-account-row')).toContainText('Иван Петров');
  await other.context().close();
});

test('открытие ссылки токен не тратит: сканер и страница без нажатия', async ({ page, browser, request }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  // Почтовый сканер: GET по ссылке и открытие страницы со скриптами в другом браузере.
  expect((await request.get(link)).status()).toBe(200);
  const scanner = await newContext(browser);
  const asked: string[] = [];
  scanner.on('request', (r) => {
    if (r.url().includes('/api/')) asked.push(r.url());
  });
  await scanner.goto(link);
  await expect(scanner.getByRole('button', { name: 'Войти', exact: true })).toBeVisible();
  expect(asked).toEqual([]);
  await scanner.context().close();
  // Токен цел.
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
});

test('ссылка одноразовая: второе нажатие — сообщение и путь к новой ссылке', async ({ page, browser }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  const [link] = await linksFor(email);
  const other = await newContext(browser);
  await other.goto(link);
  await other.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(other.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await other.getByRole('button', { name: 'Запросить новую ссылку' }).click();
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await other.context().close();
});

test('истёкшая ссылка показывает то же сообщение', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  // Токен истекает по сроку ключа в Valkey: срок сокращается, ключ исчезает сам.
  const keys = valkey('--scan', '--pattern', 'verification:*').trim().split('\n');
  expect(keys).toHaveLength(1);
  const ttl = Number(valkey('TTL', keys[0]).trim());
  expect(ttl).toBeGreaterThan(890);
  expect(ttl).toBeLessThanOrEqual(900);
  valkey('PEXPIRE', keys[0], '1');
  await expect.poll(() => valkey('EXISTS', keys[0]).trim()).toBe('0');
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
});

test('испорченная ссылка: сообщение сразу, запросов нет', async ({ page }) => {
  await page.goto('/#/auth/confirm?token=short');
  await expect(page.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await expect(page.getByRole('button', { name: 'Войти', exact: true })).toHaveCount(0);
});

test('возврат на запрошенный адрес: раздел из другого браузера и файл в этом', async ({ page, browser }, info) => {
  const email = newEmail(info.project.name);
  // Письмо открыто в другом браузере: адрес едет в ссылке, а не в хранилище страницы.
  await page.goto('/#/trash');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  expect(link).toContain('next=');
  const other = await newContext(browser);
  await other.goto(link);
  await other.getByRole('button', { name: 'Войти', exact: true }).click();
  await other.getByLabel('Имя').fill('Анна');
  await other.getByRole('button', { name: 'Продолжить' }).click();
  await expect(other.getByRole('heading', { level: 1 })).toHaveText('Корзина');
  await expect(other).toHaveURL(/#\/trash$/);

  // Файл: создан, сессия снята, открыт адрес файла → вход → снова этот файл.
  await other.goto('/#/');
  await other.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await expect(other).toHaveURL(/#\/file\/[^/?]+$/);
  await other.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
  const fileUrl = other.url();
  await other.goto('/#/');
  await other.getByRole('button', { name: 'Выйти' }).click();
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await resetStand();
  await other.goto(fileUrl);
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await requestLink(other, email);
  await other.goto((await linksFor(email))[0]);
  await other.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(other).toHaveURL(fileUrl);
  await other.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
  await other.context().close();
});

test('повторная отправка — через 60 с', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.clock.install();
  await page.goto('/');
  await requestLink(page, email);
  const again = page.getByRole('button', { name: /Отправить ещё раз/ });
  await expect(again).toBeDisabled();
  await expect(again).toHaveText('Отправить ещё раз через 60 с');
  await page.clock.fastForward(30_000);
  await expect(again).toHaveText('Отправить ещё раз через 30 с');
  await expect(again).toBeDisabled();
  await page.clock.fastForward(30_000);
  await expect(again).toHaveText('Отправить ещё раз');
  await expect(again).toBeEnabled();
  await again.click();
  await expect(again).toHaveText('Отправить ещё раз через 60 с');
  await linksFor(email, 2);
  expect(await mailCount(email)).toBe(2);
  await page.getByRole('button', { name: 'Другой адрес' }).click();
  await expect(page.getByLabel('Электронная почта')).toBeVisible();
});

test('ошибки формы: неверный адрес, предел писем, сервер недоступен', async ({ page }, info) => {
  await page.goto('/');
  await page.getByLabel('Электронная почта').fill('не адрес');
  await page.getByRole('button', { name: 'Получить ссылку' }).click();
  await expect(page.getByRole('alert')).toContainText('Проверьте адрес');
  const email = newEmail(info.project.name);
  // Настоящий предел сервера: 5 писем на адрес, шестое — отказ.
  for (let i = 0; i < 5; i++)
    expect((await page.request.post('/api/auth/sign-in/magic-link', { data: { email }, headers: { origin: STAND_URL } })).status()).toBe(
      200,
    );
  await page.getByLabel('Электронная почта').fill(email);
  await page.getByRole('button', { name: 'Получить ссылку' }).click();
  await expect(page.getByRole('alert')).toHaveText('Слишком много запросов. Попробуйте позже');
  await page.route('**/api/auth/**', (r) => r.abort());
  await page.getByRole('button', { name: 'Получить ссылку' }).click();
  await expect(page.getByRole('alert')).toContainText('Сервер недоступен');
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Не удалось открыть Ferrum' })).toBeVisible();
  await page.unroute('**/api/auth/**');
  await page.getByRole('button', { name: 'Повторить' }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
});

test('имя обязательно: пустое не принимается', async ({ page }, info) => {
  await page.goto('/');
  await signIn(page, newEmail(info.project.name), '');
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  await page.getByLabel('Имя').fill('   ');
  await page.getByRole('button', { name: 'Продолжить' }).click();
  await expect(page.getByRole('alert')).toHaveText('Введите имя');
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
});

test('выход с экрана файлов: сессия снята, после перезагрузки — экран входа', async ({ page, context }, info) => {
  await page.goto('/');
  await signIn(page, newEmail(info.project.name));
  await page.getByRole('button', { name: 'Выйти' }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  expect((await context.cookies()).find((c) => c.name === 'ferrum.session_token')).toBeUndefined();
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
});

test('настройки: вкладка «Аккаунт», выход на всех устройствах', async ({ page, browser }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  const other = await newContext(browser);
  await other.goto('/');
  await signIn(other, email, '');
  await expect(other.getByRole('heading', { level: 1 })).toHaveText('Недавние');

  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
  await mainMenu(page, 'Настройки', 'Настройки…');
  await page.getByRole('tab', { name: 'Аккаунт' }).click();
  const panel = page.getByRole('tabpanel', { name: 'Аккаунт' });
  await expect(panel).toContainText('Иван Петров');
  await expect(panel).toContainText(email);
  await panel.getByRole('button', { name: 'Выйти на всех устройствах' }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();

  // Другое устройство: при следующей загрузке — экран входа.
  await other.reload();
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await other.context().close();
});

test('вход и выход видны в других вкладках', async ({ page, context }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/#/drafts');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  // Письмо открыто в новой вкладке, а первая ждала на «Проверьте почту».
  const tab = await context.newPage();
  await tab.goto(link);
  await tab.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(tab.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  await tab.getByLabel('Имя').fill('Иван');
  await tab.getByRole('button', { name: 'Продолжить' }).click();
  await expect(tab.getByRole('heading', { level: 1 })).toBeVisible();
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(tab.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
});

test('сервер: /healthz и заголовки изоляции на настоящем стенде', async ({ request }) => {
  const health = await request.get('/healthz');
  expect(health.status()).toBe(200);
  expect(await health.json()).toEqual({ ok: true, postgres: true, valkey: true });
  const index = await request.get('/');
  expect(index.headers()['cross-origin-opener-policy']).toBe('same-origin');
  expect(index.headers()['cross-origin-embedder-policy']).toBe('credentialless');
  expect(index.headers()['content-security-policy']).toContain("frame-ancestors 'none'");
});
````

Создать `apps/web/e2e-server/global-setup.ts`:

````ts
import { compose } from './stand';

export default function setup(): void {
  compose('up', '-d', '--wait', '--build');
}
````

Создать `apps/web/e2e-server/global-teardown.ts`:

````ts
import { compose } from './stand';

export default function teardown(): void {
  if (process.env.FERRUM_KEEP_STAND !== '1') compose('down', '-v');
}
````

Создать `apps/web/e2e-server/stand.ts`:

````ts
// Стенд E2E: команды compose, письма через API Mailpit, Valkey напрямую.

import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { expect } from '@playwright/test';

const ROOT = fileURLToPath(new URL('../../../', import.meta.url));
const MAILPIT = 'http://localhost:18025/api/v1';

export function compose(...args: string[]): string {
  return execFileSync(
    'docker',
    [
      'compose',
      '-p',
      'ferrum-test',
      '--env-file',
      'docker-compose.test.env',
      '-f',
      'docker-compose.yml',
      '-f',
      'docker-compose.test.yml',
      ...args,
    ],
    { cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] },
  );
}

export function valkey(...args: string[]): string {
  return compose('exec', '-T', 'valkey', 'valkey-cli', ...args);
}

/** Чистый стенд между тестами: счётчики частоты и письма. */
export async function resetStand(): Promise<void> {
  valkey('FLUSHALL');
  await fetch(`${MAILPIT}/messages`, { method: 'DELETE' });
}

interface MailSummary {
  ID: string;
  To: { Address: string }[];
}

/** Ссылки из писем на этот адрес, новые первыми; ждёт, пока их станет `count`. */
export async function linksFor(email: string, count = 1): Promise<string[]> {
  let found: MailSummary[] = [];
  await expect
    .poll(
      async () => {
        const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
        found = list.messages.filter((m) => m.To.some((t) => t.Address === email));
        return found.length;
      },
      { message: `письма на ${email}` },
    )
    .toBeGreaterThanOrEqual(count);
  return Promise.all(
    found.map(async (m) => {
      const full = (await (await fetch(`${MAILPIT}/message/${m.ID}`)).json()) as { Text: string };
      return /http\S+#\/auth\/confirm\S+/.exec(full.Text)![0];
    }),
  );
}

export async function mailCount(email: string): Promise<number> {
  const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
  return list.messages.filter((m) => m.To.some((t) => t.Address === email)).length;
}

/** Адреса тестов не повторяются: пользователи живут в базе стенда весь прогон. */
let seq = 0;
export function newEmail(project: string): string {
  return `user-${Date.now().toString(36)}-${++seq}-${project}@example.com`;
}
````

В `apps/web/package.json` заменить:

````json
    "unit": "playwright test -c playwright.unit.config.ts",
    "e2e": "playwright test",
    "e2e:ci": "playwright test --project=chromium --grep-invert @perf"
  },
  "dependencies": {
````

на:

````json
    "unit": "playwright test -c playwright.unit.config.ts",
    "e2e": "playwright test",
    "e2e:ci": "playwright test --project=chromium --grep-invert @perf",
    "e2e:server": "playwright test -c playwright.server.config.ts"
  },
  "dependencies": {
````

Создать `apps/web/playwright.server.config.ts`:

````ts
import { defineConfig } from '@playwright/test';

// Проект `server`: рабочая сборка против настоящего стенда в Docker (свой проект compose и порты,
// письма — Mailpit). Стенд поднимает `e2e-server/global-setup.ts` и снимает `global-teardown.ts`;
// `FERRUM_KEEP_STAND=1` оставляет его для повторных прогонов. Отдельный конфиг: обычному
// `playwright test` Docker не нужен.
export const STAND_URL = 'http://localhost:18090';

export default defineConfig({
  testDir: 'e2e-server',
  timeout: 60_000,
  // Тесты делят один стенд и очищают его счётчики.
  workers: 1,
  reporter: [['list']],
  globalSetup: './e2e-server/global-setup.ts',
  globalTeardown: './e2e-server/global-teardown.ts',
  use: { baseURL: STAND_URL, viewport: { width: 1280, height: 720 } },
  projects: [
    { name: 'server', use: { browserName: 'chromium' } },
    { name: 'server-firefox', use: { browserName: 'firefox' } },
  ],
});
````

В `apps/web/tsconfig.e2e.json` заменить:

````json
  "include": [
    "e2e/**/*.ts",
    "unit/**/*.ts",
    "src/window.d.ts",
    "playwright.config.ts",
    "playwright.unit.config.ts",
    "vite.config.ts",
    "scripts/**/*.mjs"
````

на:

````json
  "include": [
    "e2e/**/*.ts",
    "e2e-server/**/*.ts",
    "unit/**/*.ts",
    "src/window.d.ts",
    "playwright.config.ts",
    "playwright.unit.config.ts",
    "playwright.server.config.ts",
    "vite.config.ts",
    "scripts/**/*.mjs"
````

- [ ] **Step 2: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Expected: PASS: всё без замечаний; `e2e:server` — 30 passed: 15 сценариев в Chromium и Firefox, около минуты при собранном образе

- [ ] **Step 3: Закоммитить**

```bash
git add apps/web/e2e-server/auth.spec.ts apps/web/e2e-server/global-setup.ts apps/web/e2e-server/global-teardown.ts apps/web/e2e-server/stand.ts apps/web/package.json apps/web/playwright.server.config.ts apps/web/tsconfig.e2e.json
git commit -F - <<'EOF'
test(web): проект Playwright `server` — вход на настоящем стенде

Рабочая сборка в Docker, письма читаются через API Mailpit, Chromium и Firefox: вход целиком,
одноразовая и истёкшая ссылка, сканер почты не тратит токен, без сессии — только экран входа,
возврат на запрошенный адрес, выход здесь и на всех устройствах, другие вкладки, повтор через
60 с, ошибки формы. Стенд поднимает `global-setup.ts`: свой проект compose `ferrum-test` и порты.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 8: Документы и финальная проверка

`docs/running.md`, «Сделано (этап 1)» в спеке, B-189…B-199.

**Files:**
- Create: `docs/running.md`
- Modify: `docs/backlog.md`, `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md`

- [ ] **Step 1: Реализация**

В `docs/backlog.md` заменить:

````markdown
| B-188 | Имя текста: обрезка до 40 считает символы Unicode, а не графемы (эмодзи с модификаторами режется посередине); U+2028/2029 не конец строки; ведущие пробелы входят в 40; нет тестов с эмодзи, `find_layers` и слитыми шагами. | P3 | этап 4, ревью задачи 8 |

## Код

````

на:

````markdown
| B-188 | Имя текста: обрезка до 40 считает символы Unicode, а не графемы (эмодзи с модификаторами режется посередине); U+2028/2029 не конец строки; ведущие пробелы входят в 40; нет тестов с эмодзи, `find_layers` и слитыми шагами. | P3 | этап 4, ревью задачи 8 |

## Сервер и вход (этап 1 подсистемы 4)

| № | Что | Пр. | Источник |
|---|---|---|---|
| B-189 | `no-unassigned-vars` в oxlint выключено: правило не видит присваивание ссылок SolidJS в разметке (`ref={el}`), 36 ложных срабатываний. Включить, если появится настройка или проверка через `let el!:`. | P3 | этап 1 подсистемы 4 |
| B-190 | Файлы до этапа 2 лежат в IndexedDB браузера и не разделены по пользователям: тот, кто вошёл следующим в этом браузере, видит файлы предыдущего. Снимается серверным `FileStore` (§6.5). | P1 | этап 1 подсистемы 4 |
| B-191 | Имя после первого входа не меняется: в настройках только показ. | P2 | этап 1 подсистемы 4 |
| B-192 | Предел 5 писем на адрес можно истратить чужими запросами: подставить адрес и на час лишить человека писем. Цена защиты от бомбардировки почты; смягчить можно капчей или отдельным счётчиком на пару адрес и IP. | P3 | этап 1 подсистемы 4 |
| B-193 | Страница подтверждения не показывает, в какой адрес идёт вход: чужая ссылка, нажатая по неосторожности, войдёт под чужим адресом (login CSRF). Показ требует чтения записи токена без траты. | P3 | этап 1 подсистемы 4 |
| B-194 | За пробросом порта Docker Desktop и OrbStack адрес клиента — адрес шлюза, предел 20 писем в час общий на всех. За прокси спасает `TRUST_PROXY=1`, без прокси — только Linux с iptables. | P2 | этап 1 подсистемы 4 |
| B-195 | Открытый редактор не замечает истёкшую или снятую с другого устройства сессию до перезагрузки: запросов к серверу в нём нет. Снимается соединением этапа 3. | P2 | этап 1 подсистемы 4 |
| B-196 | Адрес вне `ALLOWED_EMAILS` отвечает быстрее: письмо не отправляется. По времени ответа можно подобрать список. | P3 | этап 1 подсистемы 4 |
| B-197 | Токен остаётся в Valkey на 15 минут, если письмо не ушло (предел на адрес, адрес вне списка, сбой SMTP): `sendMagicLink` зовётся после записи. Его никто не знает, вреда нет, но ключи копятся в пределах предела на IP. | P3 | этап 1 подсистемы 4 |
| B-198 | Проверка `Origin` отвергает запросы без него. Десктоп на этапе 7 (`bearer`) потребует отдельного пути. | P2 | этап 1 подсистемы 4 |
| B-199 | better-auth 1.7.16 вышел за день до этапа, вне правила «пакет старше 14 дней» из `.npmrc`; `bun.lock` такого предела не применяет. Обновиться до патча, когда версия отлежится. | P3 | этап 1 подсистемы 4 |

## Код

````

Создать `docs/running.md`:

````markdown
# Как запустить Ferrum

## Сервер, одной командой

Нужен Docker с Compose.

```bash
cp .env.example .env
# впишите в .env секрет: BETTER_AUTH_SECRET=$(openssl rand -base64 32)
docker compose up --build
```

Откройте http://localhost:8090. Письма со ссылками входа перехватывает Mailpit: http://localhost:8025 (профиль `dev` из `.env.example`).

Первая сборка образа идёт несколько минут: ядро Rust в wasm, затем интерфейс, затем сервер. Данные живут в томах `ferrum-postgres`, `ferrum-valkey` и `ferrum-data`; `docker compose down` их не трогает.

На рабочем сервере: уберите `COMPOSE_PROFILES=dev`, задайте настоящий `SMTP_URL` и `MAIL_FROM`, `PUBLIC_URL` с `https://`. За обратным прокси поставьте `TRUST_PROXY=1`: иначе все клиенты считаются одним IP (предел — 20 писем в час), а при проброшенном порте Docker Desktop и OrbStack адрес клиента и так подменяют адресом шлюза. `ALLOWED_EMAILS` ограничивает, кто может войти.

## Редактор без сервера

Автономная сборка (`VITE_FERRUM_LOCAL=1`, режим `standalone`): без входа, файлы в IndexedDB.

```bash
cd apps/web
npm ci
npm run wasm    # ядро; нужен Rust и wasm-bindgen-cli 0.2.129
npm run dev     # или npm run build && npm run preview
```

Десктоп собирает её же: `beforeBuildCommand` в `tauri.conf.json`. Рабочая сборка, та что в образе, — `npm run build:server`: `dist-server`, без стенда ядра и без автономного режима, это проверяет `scripts/check-dist.mjs`.

Интерфейс в разработке против сервера: `npm run dev:server` (API проксируется на `FERRUM_API`, по умолчанию http://localhost:8090), а `PUBLIC_URL` сервера — адрес vite.

## Проверки

Из корня репозитория, зависимости — `npm ci`, `apps/server`: `bun install`.

```bash
npm run typecheck           # apps/web (три tsconfig), apps/server, packages/protocol
npm run lint                # oxlint
npm run format:check        # prettier
npm run test:server         # bun test против Postgres, Valkey и Mailpit, стенд поднимается сам
npm run test:stand:down     # снять стенд тестов
```

В `apps/web`:

```bash
npm run unit                # модульные
npm run build && npm run e2e:ci   # E2E автономной сборки
npm run e2e:server          # проект `server`: вход на настоящем стенде, Chromium и Firefox
```

Стенд тестов — свой проект compose `ferrum-test` с портами 18090 (сервер), 18025 и 11025 (Mailpit), 15432 (Postgres), 16379 (Valkey). `FERRUM_KEEP_STAND=1` оставляет его после E2E.
````

В `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` заменить:

````markdown
- **Ограничения:** не больше 5 писем на адрес и 20 на IP в час (встроенный `rateLimit`, счётчики в Valkey).
- **Защита:** проверка `Origin` на всех изменяющих запросах и при открытии WebSocket, `trustedOrigins` из `PUBLIC_URL`, проверка входных данных по схемам из `packages/protocol`.

**Автономная сборка.** Флаг времени сборки `VITE_FERRUM_LOCAL=1` оставляет прежний режим без сервера, с локальным `FileStore`. Рабочая сборка кода автономного режима не содержит. Он нужен для 414 существующих E2E и для работы над редактором без Docker. Новые сценарии проверяются отдельным проектом Playwright против настоящего стенда.
````

на:

````markdown
- **Ограничения:** не больше 5 писем на адрес и 20 на IP в час (встроенный `rateLimit`, счётчики в Valkey).
- **Защита:** проверка `Origin` на всех изменяющих запросах и при открытии WebSocket, `trustedOrigins` из `PUBLIC_URL`, проверка входных данных по схемам из `packages/protocol`.

- **Сделано (этап 1):**
  - Инструменты: корневой `package.json`, TypeScript 7.0.2 (и в `apps/web`), oxlint 1.89.0, prettier 3.9.10, `.prettierrc` под нынешний стиль: одинарные кавычки, ширина 140 (самая маленькая правка старого кода из 100–140). Форматирование — отдельный коммит. `no-unassigned-vars` выключено: ссылки SolidJS присваиваются разметкой (B-189).
  - Сервер: `apps/server` на `Bun.serve`, нумерованные миграции под `pg_advisory_lock`, `FsBlobStore`, `/healthz`, заголовки изоляции и CSP как в окне десктопа. `packages/protocol` — разбор запросов входа без зависимостей. Образ в три стадии, `docker-compose.yml` и свой стенд тестов `docker-compose.test.yml`.
  - Better Auth 1.7.16 с `pg` и `Bun.RedisClient`: круг письмо → подтверждение → одноразовость (`GETDEL`, проверено пятью параллельными нажатиями) → сессия в Valkey → проверка сессии работает на Bun без обходов. Токен в Valkey — хэшем, срок 900 с. Таблицы — только `user`, `account`.
  - **(поведение)** Наружу открыт короткий список маршрутов: `POST /api/auth/{sign-in/magic-link, confirm, name, sign-out, sign-out-everywhere}`, `GET /api/auth/session`; остальные точки Better Auth, в том числе GET, тратящий токен, отдают 404. `confirm` принимает токен в теле и проверяет `Origin`. Ответы не содержат токена сессии: Better Auth отдаёт его в теле, что обесценило бы `HttpOnly`.
  - **(поведение)** Ссылка: `PUBLIC_URL/#/auth/confirm?token=…&next=…`. Токен после `#` не попадает на сервер и в журналы; `next` — только маршрут приложения, поэтому возврат работает и в другом браузере, и не уводит на чужой адрес. Открытие страницы не делает ни одного запроса. Истёкшая и использованная ссылка неразличимы: «Ссылка недействительна или устарела. Запросите новую». Ссылка, вставленная в ту же вкладку, тоже работает (`hashchange`).
  - **(поведение)** Предел на адрес — свой счётчик в том же Valkey: встроенный `rateLimit` считает только по IP и пути. Сверх предела — 429 и «Слишком много запросов». Адрес вне `ALLOWED_EMAILS` получает тот же ответ, письма нет. Адрес приводится к нижнему регистру. IP — адрес соединения; чужому `X-Forwarded-For` верим только при `TRUST_PROXY=1`, последней записи.
  - **(поведение)** Вход и выход видны в других вкладках (`BroadcastChannel`), вкладка «Проверьте почту» открывается сама. Выход есть и на экране файлов: настройки открываются только из редактора.
  - «Черновики» отдельной записи не требуют: по §6.1 это файлы с пустым `project_id`, создавать при регистрации нечего.
  - Рабочая сборка: `npm run build:server` в `dist-server`, без `debug.html`, код входа подключает ветка `main.tsx` через `extensions.ts`; `check-dist.mjs` проверяет обе сборки. `npm run build` и `npm run dev` — автономный режим (`.env.standalone`), десктоп собирает его через `beforeBuildCommand`. Файлы в обеих сборках пока в IndexedDB, общие для всех, кто входит в этом браузере (B-190).
  - `authorizeUpgrade(req, auth, publicUrl)` — проверка `Origin` и сессии для WebSocket этапа 3, тест — настоящий upgrade на Bun.
  - Тесты: `bun test` против Postgres, Valkey и Mailpit в Docker; Playwright `playwright.server.config.ts`, проекты `server` и `server-firefox`. Отдельный конфиг, чтобы обычному `playwright test` Docker не требовался.

**Автономная сборка.** Флаг времени сборки `VITE_FERRUM_LOCAL=1` оставляет прежний режим без сервера, с локальным `FileStore`. Рабочая сборка кода автономного режима не содержит. Он нужен для 414 существующих E2E и для работы над редактором без Docker. Новые сценарии проверяются отдельным проектом Playwright против настоящего стенда.
````

- [ ] **Step 2: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/desktop/src-tauri && export CARGO_TARGET_DIR="$PWD/../../../target" CARGO_INCREMENTAL=0 && cargo check`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Expected: всё PASS: format:check, lint, typecheck без замечаний; `bun test`: 6 и 35 pass; модульные — 186 passed; обе сборки; `cargo check` десктопа без ошибок; `e2e:ci` — 415 passed (в прототипе, 9,1 мин; `--retries=1`, flaky нет); `e2e:server` — 30 passed

- [ ] **Step 3: Закоммитить**

```bash
git add docs/backlog.md docs/running.md docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md
git commit -F - <<'EOF'
docs: подсистема 4, этап 1 — «Сделано» в спеке, запуск, бэклог

docs/running.md — запуск и проверки; B-189…B-199.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

## Что этап 1 сознательно не делает

Серверные файлы, команды и роли (этап 2), WebSocket синхронизации (этап 3; здесь только `authorizeUpgrade` и его тест), «Нет доступа» к чужому файлу (этап 2), вход на десктопе (этап 7).
