# Ferrum, подсистема 4, этап 4: присутствие — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Участники открытого файла видят друг друга: курсоры с именем, рамки чужих выделений, живое превью чужого перетаскивания, ресайза и поворота, аватары в шапке и следование за участником; чужое превью никогда не попадает в документ.

**Architecture:** Протокол: свои строгие двоичные кадры `presence` и `presence-part` поверх WebSocket файла этапа 3, от сервера — пачка записей. Сервер: `PresenceHub` подписывает кадр участником соединения (кто, имя, цвет), держит последнее состояние в памяти и раз в 33 мс шлёт каждому получателю одну пачку, медленному — пропускает тик. Ядро: редактор ведёт участников, плавное движение курсоров и следование; сцена — слой чужого превью под своим; рендер рисует курсоры готовыми картинками. Клиент: `sync.worker` кодирует свои кадры и передаёт чужие ядру напрямую; основной поток рисует только полосу аватаров.

**Tech Stack:** Bun 1.4.2, TypeScript 7.0.2, SolidJS 1.9.17, @playwright/test 1.64.0; Rust: ferrum-scene, ferrum-render (vello), ferrum-editor, ferrum-wasm. Новых зависимостей нет.

**Спеки:** `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` — раздел 9, а также 7.2, 13, 14; спека ядра `2026-10-23-ferrum-core-design.md` §8.6.

## Global Constraints

- Ветка `feat/sub4-stage4-presence` от 69fb29c. Каталог сборки wasm — у каждой копии свой: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0` (в `apps/web` — `"$PWD/../../target"`); команды — из корня копии, если не сказано иное. Нужны Bun 1.4, Node 24, Docker с Compose 2.24+, `wasm-bindgen` 0.2.129.
- Новых зависимостей нет.
- Docker: только свои проекты compose — `ferrum` и `ferrum-test`. Порты: сервер 8090, Mailpit 8025; стенд тестов — 18090, 18025, 11025, 15432, 16379. Чужие контейнеры (`agent-dev-*`, `relay-*`) не трогать. `down -v` — только `ferrum-test` и `ferrum`. Никаких `docker system prune`. `bun test` сервера и `e2e:server` делят базу стенда: не запускать их одновременно.
- Порт 4173 может быть занят посторонним процессом: его не трогать, E2E тогда — через временный конфиг Playwright с другими портами (не коммитить).
- Основной поток тонкий: кадры присутствия, плавное движение и превью — в worker'ах; основной поток получает только список участников не чаще 4 раз в секунду.
- Цели §13.1: кадр с 50 курсорами — прибавка не больше 1 мс на 1920×1080; от кадра до рассылки на 50 участников — 40 мс p95.
- Кто, имя и цвет участника задаёт сервер по соединению: в кадре клиента этих полей нет, подделать чужой курсор нельзя. Присутствие файла получают только соединения, чью роль подтвердил `hello`; закрытие соединения — уход у всех сразу.
- Пределы: 2 КБ на кадр, 30 кадров в секунду на соединение, 120 на человека; лишнее и неверное отбрасывается молча, соединение не рвётся. Обычная работа (быстрая мышь, перетаскивание 1000 узлов) в пределы не упирается.
- Очередей присутствия нет: у медленного получателя тик пропускается, отметки сливаются, буфер сокета не растёт.
- Чужое живое превью — слой сцены, в документ не пишется ни при каком своём действии.
- Автономная сборка и десктоп не меняют поведения, кода сервера и полосы аватаров в них нет (`check-dist.mjs`); общий код без условий по флагу, через `extensions`.
- Тесты падают при откате правки, без ожиданий по времени и зависимости от порядка.
- Поведение — как в Figma; где спека молчит, решение помечено **(поведение)**.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Сначала отдельными пробами (`render_report`, `editor_report`, `presence-load.test.ts`) проверены главные риски.
- **Кадр.** 50 курсоров с подписями и 50 чужих выделений на 1920×1080 добавляют к кадру 0,13 мс на 1× и 0,18 мс на 2× при цели 1 мс. Первый вариант рисовал всё заново каждый кадр и стоил 0,96 / 1,35 мс; на 2× цель не держалась. Поэтому курсор с подписью рисуется готовой картинкой.
- **Сервер.** 50 участников по 20 кадров в секунду (сокеты в дочернем процессе, не браузеры): от отправки до приёма p50 19 мс, p95 35 мс при цели 40 мс. Сервер занят на 5 % ядра, живая куча растёт на 1,9 МБ. Враждебный участник (около 4000 кадров в секунду, половина больше 2 КБ) вместе с сокетом, который не читает, не поднимает буфер выше 74 КБ, а у остальных p95 остаётся 34 мс.
- **Чужое превью.** Приём кадра с перетаскиванием 1000 узлов занимает 2,9 мс (медиана) против 1,7 мс у своего жеста; документ при этом не меняется.
- **Почему не `EphemeralStore`.** Он стоит 54 мкс на кадр против 0,3 мкс у строгого разбора своих кадров.

**Как проверялся план.**
1. Прототип этапа собран в отдельной копии: `plan-verify-s4s4`, ветка `verify/sub4-s4` от 69fb29c, по коммиту на задачу.
2. Шаги этого документа сгенерированы из коммитов прототипа.
3. Скрипт, читающий сам этот документ, применил их к чистой копии от 69fb29c (`plan-apply-s4s4`, ветка `verify/sub4-s4-apply`) по порядку задач, с коммитом после каждой.
4. После каждой из 9 задач дерево копии совпало с деревом соответствующего коммита прототипа; `git diff verify/sub4-s4 HEAD` пуст.

На этой копии выполнены все шаги «Убедиться, что тесты падают» и все шаги «Проверить», кроме `npm run e2e:ci` задачи 9. Причины падений и значения в шагах «Проверить» записаны из этого прогона. Красный шаг задачи 8 в первом прогоне выбирал несуществующий проект `chromium` (у `e2e:server` проекты `server` и `server-firefox`); он исправлен и перезапущен на дереве задачи 7. `e2e:ci` выполнен на прототипе на вершине 91d63e1, с той же автономной сборкой: 416 passed, без повторов. Перед PR его нужно прогнать заново на вершине ветки.

**Откат правок.** Проверено 38 мутаций: протокол 4, сервер 9, клиент 6, сцена 3, редактор 8, ядро worker'а 5, рендер 3. В первом прогоне 4 не были пойманы; после усиления тестов пойманы все:
- присутствие тратило бакет правок: тест теперь шлёт 2500 кадров вместо 500;
- две проверки «не слать до роли» страховали друг друга: лишняя убрана;
- курсор за краем экрана: тест проверяет, что картинка для него не строится.

Прототип нашёл ещё одну дыру: клавиатурный сдвиг читал свойства из сцены мимо `begin_doc_edit` и записал бы в документ сдвиг из чужого превью. Это закрыто снятием слоя превью в `Editor::run`, и там же стоит `debug_assert`.

**Не запускались:**
- `@perf`, проект `gpu`, Firefox и WebKit автономной сборки; 11 прежних пропусков Firefox на стенде;
- 50 настоящих браузеров и замер «движение мыши — курсор на экране другого» в двух браузерах (B-383); длинные задачи основного потока при 50 участниках;
- отдельный замер кадра рисования живого превью 1000 узлов; `render_report` в wasm и на GPU;
- несколько процессов сервера (B-373), настоящий прокси, образ amd64, `docker compose up` проекта `ferrum`, десктоп.

---

## Этапы подсистемы 4

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Сервер и вход (сделан) | Docker, миграции, Better Auth, вход | 3, 4, 5 |
| 2. Файлы на сервере (сделан) | команды, проекты, черновики, роли, «Поделиться» | 6 |
| 3. Синхронизация (сделан) | журнал, снимки, чужие правки, переподключение | 7, 8 |
| **4. Присутствие** (этот план) | курсоры, выделения, живое превью, аватары, следование | 9 |
| 5. Комментарии | метки, обсуждения, письма | 10 |
| 6. Офлайн и версии | очередь правок, история версий | 11 |
| 7. Десктоп | вход через браузер, серверные файлы | 12 |

## Решения

Где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**. По справке Figma ничего не сверялось. Итог и замеры — в «Сделано (этап 4)» спеки (задача 9).

Расхождения со спекой:

1. **§9: транспорт — свои кадры, а не `EphemeralStore`.** Проба: проверка и подпись кадра `EphemeralStore` на сервере — 54 мкс, своего строгого кадра — 0,3 мкс при том же размере; при 2500 кадрах в секунду это 13,5 % ядра против 0,07 %.
2. **§9: «без хранения».** Сервер держит последнее состояние участника в памяти, чтобы пришедший позже увидел всех сразу: не больше 2 КБ состояния и 16 частей по 2 КБ на соединение, уходит с соединением. На диск ничего не пишется.
3. **§9: живое превью — одна матрица жеста и рамка, узлы — выделение участника частями.** Идентификаторы и преобразования 1000 узлов — около 36 КБ, в 2 КБ не входят. Выделение больше 16 частей у других — только общей рамкой (B-377).
4. **§7.2:** вместо `ephemeral { bytes }` — кадры `presence`, `presence-part` и пачка `presence` от сервера; `SYNC_PROTOCOL = 2`, вкладка прежней версии получает `please-reload`.

Поведение:

5. Сервер подписывает участника: `userId` и имя из базы при `hello` (пустое имя — часть почты до `@`), цвет — FNV-1a от `userId` в палитре протокола. Участник — соединение (вкладка).
6. Пределы: 2 КБ на кадр; 30 кадров в секунду на соединение (запас 60); 120 в секунду на человека по всем вкладкам (запас 240); бакеты правок присутствие не тратит. Клиент шлёт не больше 25 кадров в секунду, курсор — не чаще 20 раз (ядро, 50 мс).
7. Рассылка — пачка каждому получателю раз в 33 мс; при буфере сокета выше 64 КБ тик пропускается, отметки сливаются.
8. **(поведение)** Читатель виден и видит других. Указатель ушёл с холста — курсора нет. Метка курсора не гаснет. Рамка чужого выделения без подписи: рамки узлов (до 64) и общая. Участники других страниц — в списке (аватар бледнее), курсоров нет.
9. **(поведение)** Свои другие вкладки — курсор со своим именем, своего аватара в полосе нет; у другого человека один аватар на все его вкладки, щелчок следует за новейшей. Больше пяти — счётчик со списком.
10. **(поведение)** Следование: камера вписывает вид участника в свой холст (центр и зум), страница — за ним. Выход — нажатие, колесо, щипок, клавиша (не модификатор), команда, своя смена страницы; уход участника и обрыв — конец. Рамка окна синяя, как в спеке; в Figma — цветом участника (B-382).
11. **(поведение)** Пока идёт своё действие (жест, правка текста или вектора, жест панели, команда, нажатие), живого превью других не видно: так чужое превью гарантированно не попадает в документ. В Figma оно видно и во время своего действия (B-375).
12. **(поведение)** Копии с Alt и конец линии — без живого превью (B-378). Ресайз текста у других переносит строки своим реестром шрифтов.
13. Повтор своего состояния раз в 10 с из `sync.worker`: скрытая вкладка остаётся видна, как в Figma; 30 с без кадров — участник пропадает, закрытие соединения — сразу.
14. Курсор — готовая картинка (стрелка с белой обводкой и плашка имени), одна на имя, цвет и плотность, не больше 8 новых за кадр; стороны рамок по осям — заливкой прямоугольников. Так кадр с 50 курсорами — 0,13 мс вместо 0,96 мс.

## Структура файлов

| Файл | Ответственность |
|---|---|
| `packages/protocol/src/presence.ts` (новый), `sync.ts` | кадры присутствия, пределы, строгий разбор, палитра |
| `apps/server/src/sync/presence.ts` (новый), `hub.ts` | подпись участником, пачки по тику, пределы, уход |
| `crates/ferrum-scene/src/scene.rs` | слой чужого превью под своим |
| `crates/ferrum-render/src/overlay.rs`, `lib.rs` | курсоры картинками, рамки чужих выделений |
| `crates/ferrum-editor/src/presence.rs` (новый), `editor.rs`, `tools/select.rs` | участники, плавное движение, превью, следование, защита от записи чужого |
| `crates/ferrum-wasm/src/core/presence.rs` (новый), `protocol.rs`, `core.rs` | команды и события присутствия, своё состояние 20 раз в секунду |
| `apps/web/src/core/sync-client.ts`, `worker/{sync,core}.worker.ts`, `ui/{store,file-store}.ts` | свои кадры, чужие ядру, список участников |
| `apps/web/src/ui/server/presence-bar.tsx` (новый), `chrome.tsx`, `app.tsx`, `styles/server.css` | полоса аватаров, рамка следования |
| `apps/web/e2e-server/presence.spec.ts` (новый) | E2E двумя браузерами на стенде |
| `docs/` | спека, запуск, бэклог B-373…B-383 |

---

### Task 1: Протокол — кадры присутствия

Спека §9, §7.2; «Решения» 1–4. Состояние участника целиком, части выделения, пачка сервера; неверное или больше 2 КБ — `null`, а не отказ кадра. Сервер на этом шаге кадры знает, но не рассылает.

**Files:**
- Create: `packages/protocol/src/presence.ts`
- Modify: `apps/server/src/sync/hub.ts`, `packages/protocol/src/index.ts`, `packages/protocol/src/sync.ts`
- Test: `apps/server/test/hub.test.ts`, `apps/server/test/sync-load-probe.ts`, `apps/web/unit/sync-client.test.ts`, `packages/protocol/test/presence.test.ts` (новый), `packages/protocol/test/sync.test.ts`

**Interfaces:**
- Produces: `SYNC_PROTOCOL = 2`; `ClientFrame` `{t:'presence', body, state?}`, `{t:'presence-part', body, part?}`; `ServerFrame` `{t:'presence', items}`; `encodePresenceBody`, `parsePresenceBody`, `encodePartBody`, `parsePartBody`, `encodeSelectionIds`, `decodeSelectionIds`, `splitSelection`, `encodePresenceRecord`, `encodePresenceBatch`, `parsePresenceBatch`, `presenceColor`, `PRESENCE_COLORS`, пределы `PRESENCE_*`.

- [ ] **Step 1: Написать тесты**

В `apps/server/test/hub.test.ts` заменить:

````ts

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { encodeClientFrame, encodeVersion, SYNC_CLOSE, type VersionMap } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import type { Pool } from 'pg';
````

на:

````ts

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { encodeClientFrame, encodeVersion, SYNC_CLOSE, SYNC_PROTOCOL, type VersionMap } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import type { Pool } from 'pg';
````

В `apps/server/test/hub.test.ts` заменить:

````ts
    const c = (await Client.open(s, who, file))!;
    open.push(c);
    c.send(encodeClientFrame({ t: 'hello', protocol: 1, schema: value }));
    await c.wait('role');
  };
````

на:

````ts
    const c = (await Client.open(s, who, file))!;
    open.push(c);
    c.send(encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: value }));
    await c.wait('role');
  };
````

В `apps/server/test/sync-load-probe.ts` заменить:

````ts
//   slowbig MB COUNT       — то же кадрами по MB мегабайт: ни один кадр, свой ack и чужие правки, не теряется.

import { encodeClientFrame, encodeVersion, parseServerFrame, syncPath } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import { connect } from 'node:net';
````

на:

````ts
//   slowbig MB COUNT       — то же кадрами по MB мегабайт: ни один кадр, свой ack и чужие правки, не теряется.

import { encodeClientFrame, encodeVersion, parseServerFrame, SYNC_PROTOCOL, syncPath } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import { connect } from 'node:net';
````

В `apps/server/test/sync-load-probe.ts` заменить:

````ts
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => {
      ws.send(encodeClientFrame({ t: 'hello', protocol: 1, schema: 1 }));
      ws.send(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) }));
    };
````

на:

````ts
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => {
      ws.send(encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 }));
      ws.send(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) }));
    };
````

В `apps/server/test/sync-load-probe.ts` заменить:

````ts
    return Buffer.concat([head, Buffer.from([0, 0, 0, 0]), p]);
  };
  raw.write(mask(encodeClientFrame({ t: 'hello', protocol: 1, schema: 1 })));
  raw.write(mask(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) })));
  await Bun.sleep(300);
````

на:

````ts
    return Buffer.concat([head, Buffer.from([0, 0, 0, 0]), p]);
  };
  raw.write(mask(encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 })));
  raw.write(mask(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) })));
  await Bun.sleep(300);
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts

import { expect, test } from '@playwright/test';
import { encodeServerFrame, encodeVersion, parseClientFrame, SYNC_CLOSE, type ClientFrame, type ServerFrame } from '@ferrum/protocol';
import {
  PING_EVERY_MS,
````

на:

````ts

import { expect, test } from '@playwright/test';
import {
  encodeServerFrame,
  encodeVersion,
  parseClientFrame,
  SYNC_CLOSE,
  SYNC_PROTOCOL,
  type ClientFrame,
  type ServerFrame,
} from '@ferrum/protocol';
import {
  PING_EVERY_MS,
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
  ws.open();
  ws.drain();
  expect(ws.sent[0]).toEqual({ t: 'hello', protocol: 1, schema: 1 });
  expect(s.commands).toContainEqual({ type: 'syncReset' });
  expect(s.flushing()).toBe(true);
````

на:

````ts
  ws.open();
  ws.drain();
  expect(ws.sent[0]).toEqual({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 });
  expect(s.commands).toContainEqual({ type: 'syncReset' });
  expect(s.flushing()).toBe(true);
````

Создать `packages/protocol/test/presence.test.ts`:

````ts
import { expect, test } from 'bun:test';
import {
  decodeSelectionIds,
  encodePartBody,
  encodePresenceBody,
  encodePresenceRecord,
  encodeSelectionIds,
  parsePartBody,
  parsePresenceBody,
  presenceColor,
  PRESENCE_BATCH_MAX,
  PRESENCE_COLORS,
  PRESENCE_FRAME_MAX,
  PRESENCE_PART_BYTES,
  PRESENCE_PARTS_MAX,
  splitSelection,
  type PresenceState,
} from '../src/presence';
import { encodeClientFrame, encodePresenceBatch, parseClientFrame, parseServerFrame } from '../src/sync';

const page = '12@7910247476534078275';
const full: PresenceState = {
  page,
  view: [-100.5, -50, 1820, 1030],
  cursor: [12.25, -7.5],
  selection: { rev: 7, count: 1000, parts: 2, bounds: [0, 0, 500, 300] },
  gesture: { kind: 'resize', transform: [2, 0, 0, 1.5, -10, 20], frame: [1, 0, 0, 1, 5, 5], k: 1 },
};

test('состояние туда и обратно со всеми сочетаниями полей', () => {
  for (const cursor of [null, full.cursor]) {
    for (const selection of [null, full.selection]) {
      for (const gesture of [null, full.gesture]) {
        const s = { ...full, cursor, selection, gesture };
        const body = encodePresenceBody(s);
        expect(parsePresenceBody(body)).toEqual(s);
        expect(parseClientFrame(encodeClientFrame({ t: 'presence', body }))).toEqual({ t: 'presence', body, state: s });
      }
    }
  }
});

test('кадр с курсором и всеми полями меньше 2 КБ, а обычный — меньше 100 байт', () => {
  expect(encodePresenceBody(full).length).toBeLessThan(300);
  expect(encodePresenceBody({ ...full, selection: null, gesture: null }).length).toBeLessThan(100);
});

test('разбор строгий: нечисловые и огромные числа, лишние флаги, хвост, корень вместо страницы', () => {
  const body = encodePresenceBody({ ...full, selection: null, gesture: null });
  const view = new DataView(body.buffer);
  const pageLen = body[1];
  const at = 2 + pageLen;
  for (const bad of [NaN, Infinity, -Infinity, 1e10]) {
    const b = body.slice();
    new DataView(b.buffer).setFloat64(at + 32, bad, true);
    expect(parsePresenceBody(b)).toBeNull();
  }
  const flags = body.slice();
  flags[0] |= 8;
  expect(parsePresenceBody(flags)).toBeNull();
  expect(parsePresenceBody(new Uint8Array([...body, 0]))).toBeNull();
  expect(parsePresenceBody(body.subarray(0, body.length - 1))).toBeNull();
  // Вид без площади — не вид.
  const flat = body.slice();
  new DataView(flat.buffer).setFloat64(at + 16, view.getFloat64(at, true), true);
  expect(parsePresenceBody(flat)).toBeNull();
  expect(() => encodePresenceBody({ ...full, page: 'root' })).toThrow();
  const root = new Uint8Array([0, 4, ...new TextEncoder().encode('root'), ...body.subarray(at)]);
  expect(parsePresenceBody(root)).toBeNull();
});

test('разбор строгий: выделение и жест', () => {
  const at = (s: PresenceState) => encodePresenceBody(s);
  const sel = at({ ...full, cursor: null, gesture: null });
  const off = 2 + sel[1] + 32;
  const patch = (b: Uint8Array, i: number, f: (v: DataView) => void) => {
    const c = b.slice();
    f(new DataView(c.buffer, i));
    return c;
  };
  expect(parsePresenceBody(patch(sel, off + 4, (v) => v.setUint32(0, 0, true)))).toBeNull();
  expect(parsePresenceBody(patch(sel, off + 4, (v) => v.setUint32(0, 100_001, true)))).toBeNull();
  expect(parsePresenceBody(patch(sel, off + 8, (v) => v.setUint8(0, 0)))).toBeNull();
  expect(parsePresenceBody(patch(sel, off + 8, (v) => v.setUint8(0, PRESENCE_PARTS_MAX + 1)))).toBeNull();
  const g = at({ ...full, cursor: null, selection: null });
  const goff = 2 + g[1] + 32;
  expect(parsePresenceBody(patch(g, goff, (v) => v.setUint8(0, 4)))).toBeNull();
  expect(parsePresenceBody(patch(g, goff + 1, (v) => v.setFloat64(0, NaN, true)))).toBeNull();
});

test('кадр присутствия больше 2 КБ или неверный разбирается в `null`-состояние, а не в отказ кадра', () => {
  const big = new Uint8Array(PRESENCE_FRAME_MAX + 10);
  big[0] = 4;
  expect(parseClientFrame(big)).toEqual({ t: 'presence', body: big.subarray(1), state: null });
  const bad = new Uint8Array([6, 1, 2]);
  expect(parseClientFrame(bad)).toEqual({ t: 'presence-part', body: bad.subarray(1), part: null });
});

test('узлы выделения: запись одна, туда и обратно', () => {
  const ids = ['5@2', '1@2', '1@18446744073709551615', '2147483647@3', '7@2'];
  const bytes = encodeSelectionIds(ids);
  expect(decodeSelectionIds(bytes)).toEqual(['1@2', '5@2', '7@2', '2147483647@3', '1@18446744073709551615']);
  // Повтор узла не удваивается.
  expect(decodeSelectionIds(encodeSelectionIds(['1@2', '1@2']))).toEqual(['1@2']);
});

test('узлы выделения: неверная запись отвергается', () => {
  const peer = (p: bigint) => {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, p, true);
    return [...b];
  };
  expect(decodeSelectionIds(new Uint8Array())).toBeNull();
  // Лишний байт в числе (0x80 0x00).
  expect(decodeSelectionIds(new Uint8Array([...peer(2n), 1, 0x81, 0x00]))).toBeNull();
  // Разность 0: узел дважды.
  expect(decodeSelectionIds(new Uint8Array([...peer(2n), 2, 1, 0]))).toBeNull();
  // Пиры не по возрастанию.
  expect(decodeSelectionIds(new Uint8Array([...peer(3n), 1, 1, ...peer(2n), 1, 1]))).toBeNull();
  // Счётчик за пределом TreeID.
  expect(decodeSelectionIds(new Uint8Array([...peer(2n), 1, 0xff, 0xff, 0xff, 0xff, 0x0f]))).toBeNull();
  // Заголовок обещает больше узлов, чем байтов: разбор не идёт дальше пришедшего.
  expect(decodeSelectionIds(new Uint8Array([...peer(2n), 0xff, 0xff, 0x03, 1]))).toBeNull();
  // Хвост без числа.
  expect(decodeSelectionIds(new Uint8Array([...peer(2n), 1]))).toBeNull();
});

test('1000 перетаскиваемых узлов одного автора — одна часть кадра', () => {
  const ids = Array.from({ length: 1000 }, (_, i) => `${100 + i * 15}@7910247476534078275`);
  const parts = splitSelection(ids);
  expect(parts.length).toBe(1);
  expect(parts[0].length).toBeLessThan(1100);
  const body = encodePartBody({ rev: 3, index: 0, parts: 1, bytes: parts[0] });
  expect(body.length + 1).toBeLessThanOrEqual(PRESENCE_FRAME_MAX);
  expect(parsePartBody(body)?.ids.length).toBe(1000);
});

test('большое выделение многих авторов: части не больше предела и не больше 16', () => {
  const rnd = (() => {
    let x = 1;
    return () => (x = (x * 48271) % 0x7fffffff);
  })();
  const ids = Array.from({ length: 40_000 }, () => `${rnd() % 2_000_000_000}@${rnd() % 20}`);
  const parts = splitSelection(ids);
  expect(parts.length).toBe(PRESENCE_PARTS_MAX);
  for (const p of parts) expect(p.length).toBeLessThanOrEqual(PRESENCE_PART_BYTES);
  const got = parts.flatMap((p) => decodeSelectionIds(p)!);
  expect(new Set(got).size).toBe(got.length);
  expect(got.every((id) => ids.includes(id))).toBe(true);
});

test('часть выделения: номер не больше числа частей, байты в пределе', () => {
  const bytes = encodeSelectionIds(['1@2']);
  expect(() => encodePartBody({ rev: 1, index: 1, parts: 1, bytes })).toThrow();
  expect(() => encodePartBody({ rev: 1, index: 0, parts: 17, bytes })).toThrow();
  const body = encodePartBody({ rev: 1, index: 0, parts: 1, bytes });
  const bad = body.slice();
  bad[4] = 1;
  expect(parsePartBody(bad)).toBeNull();
});

test('цвет участника — из userId, один и тот же, в пределах палитры', () => {
  expect(presenceColor('u-1')).toBe(presenceColor('u-1'));
  const seen = new Set<number>();
  for (let i = 0; i < 500; i++) {
    const c = presenceColor(`user-${i}`);
    expect(c).toBeGreaterThanOrEqual(0);
    expect(c).toBeLessThan(PRESENCE_COLORS.length);
    seen.add(c);
  }
  expect(seen.size).toBe(PRESENCE_COLORS.length);
});

test('пачка сервера: записи туда и обратно, неверная запись — весь кадр неверен', () => {
  const body = encodePresenceBody(full);
  const part = encodePartBody({ rev: 7, index: 1, parts: 2, bytes: encodeSelectionIds(['3@4']) });
  const frame = encodePresenceBatch([
    encodePresenceRecord({ k: 'join', participant: 9, user: 'u-1', name: 'Анна', color: 3 }),
    encodePresenceRecord({ k: 'state', participant: 9, body }),
    encodePresenceRecord({ k: 'part', participant: 9, body: part }),
    encodePresenceRecord({ k: 'left', participant: 4 }),
  ]);
  expect(parseServerFrame(frame)).toEqual({
    t: 'presence',
    items: [
      { k: 'join', participant: 9, user: 'u-1', name: 'Анна', color: 3 },
      { k: 'state', participant: 9, state: full },
      { k: 'part', participant: 9, part: { rev: 7, index: 1, parts: 2, ids: ['3@4'] } },
      { k: 'left', participant: 4 },
    ],
  });
  expect(parseServerFrame(new Uint8Array([...frame, 0]))).toBeNull();
  expect(parseServerFrame(encodePresenceBatch([]))).toBeNull();
  const join = encodePresenceRecord({ k: 'join', participant: 1, user: 'u', name: 'x', color: 0 });
  join[join.length - 1] = PRESENCE_COLORS.length;
  expect(parseServerFrame(encodePresenceBatch([join]))).toBeNull();
  const unknown = encodePresenceRecord({ k: 'left', participant: 1 });
  unknown[0] = 9;
  expect(parseServerFrame(encodePresenceBatch([unknown]))).toBeNull();
  const badState = encodePresenceRecord({ k: 'state', participant: 1, body: new Uint8Array([0, 1, 49]) });
  expect(parseServerFrame(encodePresenceBatch([badState]))).toBeNull();
  expect(() => encodePresenceBatch([new Uint8Array(PRESENCE_BATCH_MAX + 1)])).toThrow();
  expect(() => encodePresenceRecord({ k: 'join', participant: 1, user: 'u', name: 'я'.repeat(200), color: 0 })).toThrow();
});
````

В `packages/protocol/test/sync.test.ts` заменить:

````ts
  encodeServerFrame,
  encodeVersion,
  MAX_EPHEMERAL_BYTES,
  MAX_SYNC_FRAME_BYTES,
  MAX_SYNC_UPDATE_BYTES,
````

на:

````ts
  encodeServerFrame,
  encodeVersion,
  MAX_SYNC_FRAME_BYTES,
  MAX_SYNC_UPDATE_BYTES,
````

В `packages/protocol/test/sync.test.ts` заменить:

````ts
    { t: 'subscribe', doc: 'root', version },
    { t: 'update', doc, clientSeq: 4_000_000_000, bytes: new Uint8Array([1, 2, 3]) },
    { t: 'ephemeral', bytes: new Uint8Array([9]) },
    { t: 'ping' },
  ];
````

на:

````ts
    { t: 'subscribe', doc: 'root', version },
    { t: 'update', doc, clientSeq: 4_000_000_000, bytes: new Uint8Array([1, 2, 3]) },
    { t: 'ping' },
  ];
````

В `packages/protocol/test/sync.test.ts` заменить:

````ts
});

test('присутствие — свой малый предел', () => {
  expect(parseClientFrame(encodeClientFrame({ t: 'ephemeral', bytes: new Uint8Array(MAX_EPHEMERAL_BYTES) }))?.t).toBe('ephemeral');
  expect(parseClientFrame(encodeClientFrame({ t: 'ephemeral', bytes: new Uint8Array(MAX_EPHEMERAL_BYTES + 1) }))).toBeNull();
});

test('snapshot-needed: адрес — только снимок того же документа на своём сервере', () => {
  const frame = (doc: string, url: string) => {
````

на:

````ts
});

test('snapshot-needed: адрес — только снимок того же документа на своём сервере', () => {
  const frame = (doc: string, url: string) => {
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd packages/protocol && bun test test/presence.test.ts`
Expected: FAIL — нет модуля `../src/presence`: 1 fail, 1 error

- [ ] **Step 3: Реализация**

В `apps/server/src/sync/hub.ts` заменить:

````ts
        );
        break;
      case 'ephemeral':
        // Присутствие — этап 4: кадр известен, сервер его пока не рассылает.
        break;
    }
````

на:

````ts
        );
        break;
      case 'presence':
      case 'presence-part':
        // Присутствие рассылается следующей задачей; кадр известен и проверен разбором.
        break;
    }
````

В `packages/protocol/src/index.ts` заменить:

````ts
export * from './container';
export * from './sync';
````

на:

````ts
export * from './container';
export * from './sync';
export * from './presence';
````

Создать `packages/protocol/src/presence.ts`:

````ts
// Присутствие (спека подсистемы 4, раздел 9): курсор, вид, выделение и живое превью жеста участника. Кадры — строгие двоичные,
// без `EphemeralStore` Loro: сервер должен прочесть каждое поле, чтобы подписать кадр своим участником и отбросить чужие
// поля, а разбор хранилища Loro на сервере стоит около 54 мкс на кадр против 0,3 мкс у своего формата (проба этапа 4).
//
// Числа — LE, координаты — f64 мира. Состояние кадра — целиком (идемпотентно): потерянный или выброшенный кадр
// заменяется следующим. Выделение длиннее одного кадра идёт частями своими кадрами с номером версии.

/** Предел кадра присутствия от клиента (спека, раздел 9): 2 КБ. */
export const PRESENCE_FRAME_MAX = 2048;
/** Координаты мира по модулю: дальше — не холст, а мусор. */
export const PRESENCE_WORLD_MAX = 1e9;
/** Выделение: столько узлов называет заголовок, частей — не больше `PRESENCE_PARTS_MAX`. Больше — только рамкой. */
export const PRESENCE_SELECTION_MAX = 100_000;
export const PRESENCE_PARTS_MAX = 16;
/** Байты узлов в одной части: остальное в кадре — заголовок части. */
export const PRESENCE_PART_BYTES = PRESENCE_FRAME_MAX - 8;
/** Пачка присутствия от сервера одному получателю: остальное уйдёт следующей. */
export const PRESENCE_BATCH_MAX = 64 * 1024;
/** Имя участника в кадре, байты UTF-8, и `userId`. */
export const PRESENCE_NAME_MAX = 256;
export const PRESENCE_USER_MAX = 128;

/** Цвета участников (как в Figma — насыщенные, белый текст на них читается). Индекс — из хэша `userId`. */
export const PRESENCE_COLORS = [
  '#e5484d',
  '#f76b15',
  '#2d8a3e',
  '#0d74ce',
  '#8e4ec6',
  '#d6409f',
  '#12a594',
  '#5b5bd6',
  '#ab6400',
  '#3e63dd',
  '#ca244d',
  '#00749e',
] as const;

/** Цвет участника: FNV-1a от `userId`, один и тот же у всех получателей и во всех вкладках человека. */
export function presenceColor(user: string): number {
  let h = 0x811c9dc5;
  for (const b of new TextEncoder().encode(user)) {
    h ^= b;
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h % PRESENCE_COLORS.length;
}

/** Жест, который участник сейчас тянет: превью видно другим, в документ не идёт. */
export const GESTURE_KINDS = ['move', 'resize', 'rotate', 'scale'] as const;
export type GestureKind = (typeof GESTURE_KINDS)[number];

/** Матрица `[a, b, c, d, e, f]`, как `kurbo::Affine`. */
export type Affine6 = [number, number, number, number, number, number];
export type Rect4 = [number, number, number, number];

export interface PresenceSelection {
  /** Версия выделения: части с той же версией складываются вместе. */
  rev: number;
  count: number;
  parts: number;
  /** Общая рамка в мире: её видно, пока части не пришли, и у выделения больше частей. */
  bounds: Rect4;
}

export interface PresenceGesture {
  kind: GestureKind;
  /** Мировая матрица жеста (перенос, ресайз, поворот) и рамка выделения на начало жеста. */
  transform: Affine6;
  frame: Affine6;
  /** Равномерный коэффициент «Масштаба» (у остальных 1). */
  k: number;
}

export interface PresenceState {
  /** Страница — `TreeID` документа страницы. */
  page: string;
  /** Видимая часть мира. */
  view: Rect4;
  /** Курсор в мире; нет — указатель вне холста. */
  cursor: [number, number] | null;
  selection: PresenceSelection | null;
  gesture: PresenceGesture | null;
}

export interface PresencePart {
  rev: number;
  index: number;
  parts: number;
  /** Узлы, `counter@peer`, как `TreeID` Loro. */
  ids: string[];
}

/** Записи пачки сервера: кто пришёл, его состояние, часть выделения, кто ушёл. Участник — номер соединения. */
export type PresenceItem =
  | { k: 'join'; participant: number; user: string; name: string; color: number }
  | { k: 'state'; participant: number; state: PresenceState }
  | { k: 'part'; participant: number; part: PresencePart }
  | { k: 'left'; participant: number };

const utf8 = new TextEncoder();
const fromUtf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
const PAGE_RE = /^(0|[1-9]\d{0,9})@(0|[1-9]\d{0,19})$/;
const MAX_PEER = 2n ** 64n - 1n;
const MAX_COUNTER = 0x7fffffff;

export function isPageDoc(s: string): boolean {
  if (!PAGE_RE.test(s)) return false;
  const [counter, peer] = s.split('@');
  return Number(counter) <= MAX_COUNTER && BigInt(peer) <= MAX_PEER;
}

const coord = (v: number) => Number.isFinite(v) && Math.abs(v) <= PRESENCE_WORLD_MAX;

/** Писатель с проверкой: неверное значение — ошибка здесь, а не неверный кадр на проводе. */
class Out {
  private readonly parts: Uint8Array[] = [];
  size = 0;
  push(p: Uint8Array): this {
    this.parts.push(p);
    this.size += p.length;
    return this;
  }
  u8(v: number): this {
    if (!Number.isInteger(v) || v < 0 || v > 0xff) throw new RangeError(`u8: ${v}`);
    return this.push(Uint8Array.of(v));
  }
  u16(v: number): this {
    if (!Number.isInteger(v) || v < 0 || v > 0xffff) throw new RangeError(`u16: ${v}`);
    const b = new Uint8Array(2);
    new DataView(b.buffer).setUint16(0, v, true);
    return this.push(b);
  }
  u32(v: number): this {
    if (!Number.isInteger(v) || v < 0 || v > 0xffffffff) throw new RangeError(`u32: ${v}`);
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, v, true);
    return this.push(b);
  }
  f64(v: number): this {
    if (!coord(v)) throw new RangeError(`число: ${v}`);
    const b = new Uint8Array(8);
    new DataView(b.buffer).setFloat64(0, v, true);
    return this.push(b);
  }
  str(s: string, max: number): this {
    const b = utf8.encode(s);
    if (b.length > max) throw new RangeError(`строка длиннее ${max} байт`);
    return this.u16(b.length).push(b);
  }
  done(): Uint8Array<ArrayBuffer> {
    const out = new Uint8Array(this.size);
    let at = 0;
    for (const p of this.parts) {
      out.set(p, at);
      at += p.length;
    }
    return out;
  }
}

class In {
  at: number;
  private readonly view: DataView;
  constructor(
    private readonly b: Uint8Array,
    at = 0,
    readonly end = b.length,
  ) {
    this.at = at;
    this.view = new DataView(b.buffer, b.byteOffset, b.byteLength);
  }
  has(n: number): boolean {
    return this.at + n <= this.end;
  }
  u8(): number | null {
    return this.has(1) ? this.b[this.at++] : null;
  }
  u16(): number | null {
    if (!this.has(2)) return null;
    const v = this.view.getUint16(this.at, true);
    this.at += 2;
    return v;
  }
  u32(): number | null {
    if (!this.has(4)) return null;
    const v = this.view.getUint32(this.at, true);
    this.at += 4;
    return v;
  }
  u64(): bigint | null {
    if (!this.has(8)) return null;
    const v = this.view.getBigUint64(this.at, true);
    this.at += 8;
    return v;
  }
  /** Конечное число в пределах мира; иначе `null`. */
  f64(): number | null {
    if (!this.has(8)) return null;
    const v = this.view.getFloat64(this.at, true);
    this.at += 8;
    return coord(v) ? v : null;
  }
  bytes(n: number): Uint8Array | null {
    if (!this.has(n)) return null;
    const v = this.b.subarray(this.at, this.at + n);
    this.at += n;
    return v;
  }
  str(max: number): string | null {
    const n = this.u16();
    if (n === null || n > max) return null;
    const raw = this.bytes(n);
    if (!raw) return null;
    try {
      return fromUtf8.decode(raw);
    } catch {
      return null;
    }
  }
  page(): string | null {
    const n = this.u8();
    if (n === null || n > 40) return null;
    const raw = this.bytes(n);
    if (!raw) return null;
    try {
      const s = fromUtf8.decode(raw);
      return isPageDoc(s) ? s : null;
    } catch {
      return null;
    }
  }
  /** LEB128 до u32, без лишних байтов: у числа одна запись. */
  varint(): number | null {
    let v = 0;
    for (let i = 0; i < 5; i++) {
      const b = this.u8();
      if (b === null) return null;
      v += (b & 0x7f) * 2 ** (7 * i);
      if ((b & 0x80) === 0) {
        if (i > 0 && b === 0) return null;
        return v <= 0xffffffff ? v : null;
      }
    }
    return null;
  }
  done(): boolean {
    return this.at === this.end;
  }
}

function varint(out: Out, v: number): void {
  if (!Number.isInteger(v) || v < 0 || v > 0xffffffff) throw new RangeError(`varint: ${v}`);
  const bytes: number[] = [];
  do {
    let b = v % 128;
    v = Math.floor(v / 128);
    if (v > 0) b |= 0x80;
    bytes.push(b);
  } while (v > 0);
  out.push(Uint8Array.from(bytes));
}

const FLAG_CURSOR = 1;
const FLAG_SELECTION = 2;
const FLAG_GESTURE = 4;

function writeRect(out: Out, r: Rect4): void {
  for (const v of r) out.f64(v);
}

function readRect(r: In, strict: boolean): Rect4 | null {
  const x0 = r.f64();
  const y0 = r.f64();
  const x1 = r.f64();
  const y1 = r.f64();
  if (x0 === null || y0 === null || x1 === null || y1 === null) return null;
  if (strict ? x1 <= x0 || y1 <= y0 : x1 < x0 || y1 < y0) return null;
  return [x0, y0, x1, y1];
}

function readAffine(r: In): Affine6 | null {
  const out: number[] = [];
  for (let i = 0; i < 6; i++) {
    const v = r.f64();
    if (v === null) return null;
    out.push(v);
  }
  return out as Affine6;
}

/** Тело кадра состояния (без вида кадра). */
function writePresenceState(out: Out, s: PresenceState): void {
  if (!isPageDoc(s.page)) throw new RangeError(`страница: ${s.page}`);
  const flags = (s.cursor ? FLAG_CURSOR : 0) | (s.selection ? FLAG_SELECTION : 0) | (s.gesture ? FLAG_GESTURE : 0);
  out.u8(flags);
  const page = utf8.encode(s.page);
  out.u8(page.length).push(page);
  writeRect(out, s.view);
  if (s.cursor) out.f64(s.cursor[0]).f64(s.cursor[1]);
  if (s.selection) {
    const sel = s.selection;
    if (sel.count < 1 || sel.count > PRESENCE_SELECTION_MAX || sel.parts < 1 || sel.parts > PRESENCE_PARTS_MAX)
      throw new RangeError('выделение');
    out.u32(sel.rev).u32(sel.count).u8(sel.parts);
    writeRect(out, sel.bounds);
  }
  if (s.gesture) {
    out.u8(GESTURE_KINDS.indexOf(s.gesture.kind));
    for (const v of s.gesture.transform) out.f64(v);
    for (const v of s.gesture.frame) out.f64(v);
    out.f64(s.gesture.k);
  }
}

function readPresenceState(r: In): PresenceState | null {
  const flags = r.u8();
  if (flags === null || flags & ~(FLAG_CURSOR | FLAG_SELECTION | FLAG_GESTURE)) return null;
  const page = r.page();
  const view = readRect(r, true);
  if (page === null || view === null) return null;
  let cursor: [number, number] | null = null;
  if (flags & FLAG_CURSOR) {
    const x = r.f64();
    const y = r.f64();
    if (x === null || y === null) return null;
    cursor = [x, y];
  }
  let selection: PresenceSelection | null = null;
  if (flags & FLAG_SELECTION) {
    const rev = r.u32();
    const count = r.u32();
    const parts = r.u8();
    const bounds = readRect(r, false);
    if (rev === null || count === null || parts === null || bounds === null) return null;
    if (count < 1 || count > PRESENCE_SELECTION_MAX || parts < 1 || parts > PRESENCE_PARTS_MAX) return null;
    selection = { rev, count, parts, bounds };
  }
  let gesture: PresenceGesture | null = null;
  if (flags & FLAG_GESTURE) {
    const kind = GESTURE_KINDS[r.u8() ?? -1];
    const transform = readAffine(r);
    const frame = readAffine(r);
    const k = r.f64();
    if (!kind || transform === null || frame === null || k === null) return null;
    gesture = { kind, transform, frame, k };
  }
  return { page, view, cursor, selection, gesture };
}

/** Узлы части: группы по пиру (пиры по возрастанию), в группе — счётчики по возрастанию разностями. */
export function encodeSelectionIds(ids: string[]): Uint8Array<ArrayBuffer> {
  const byPeer = new Map<bigint, number[]>();
  for (const id of ids) {
    if (!isPageDoc(id)) throw new RangeError(`узел: ${id}`);
    const [c, p] = id.split('@');
    const peer = BigInt(p);
    let list = byPeer.get(peer);
    if (!list) byPeer.set(peer, (list = []));
    list.push(Number(c));
  }
  const out = new Out();
  for (const peer of [...byPeer.keys()].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))) {
    const counters = [...new Set(byPeer.get(peer)!)].sort((a, b) => a - b);
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, peer, true);
    out.push(b);
    varint(out, counters.length);
    varint(out, counters[0]);
    for (let i = 1; i < counters.length; i++) varint(out, counters[i] - counters[i - 1]);
  }
  return out.done();
}

/** Разбор узлов части. Запись одна: пиры строго по возрастанию, разности больше нуля, счётчик в пределах `TreeID`. */
export function decodeSelectionIds(b: Uint8Array): string[] | null {
  const r = new In(b);
  const out: string[] = [];
  let prev = -1n;
  while (!r.done()) {
    const peer = r.u64();
    const n = r.varint();
    if (peer === null || n === null || peer <= prev || n < 1) return null;
    prev = peer;
    // Каждый узел — хотя бы байт: заголовок не заставит разбирать больше, чем пришло.
    if (n > r.end - r.at) return null;
    let counter = r.varint();
    if (counter === null || counter > MAX_COUNTER) return null;
    out.push(`${counter}@${peer}`);
    for (let i = 1; i < n; i++) {
      const d = r.varint();
      if (d === null || d < 1) return null;
      counter += d;
      if (counter > MAX_COUNTER) return null;
      out.push(`${counter}@${peer}`);
    }
  }
  return out.length > 0 ? out : null;
}

/** Разбивает выделение на части, каждая — не больше `PRESENCE_PART_BYTES`. Сверх `PRESENCE_PARTS_MAX` — не уходит. */
export function splitSelection(ids: string[]): Uint8Array<ArrayBuffer>[] {
  const parts: Uint8Array<ArrayBuffer>[] = [];
  let at = 0;
  while (at < ids.length && parts.length < PRESENCE_PARTS_MAX) {
    // Кусок, который точно влезает (по 14 байт на узел в худшем случае), затем добор, пока влезает.
    let n = Math.min(ids.length - at, Math.floor(PRESENCE_PART_BYTES / 14));
    let bytes = encodeSelectionIds(ids.slice(at, at + n));
    for (let step = 64; step >= 1; step = Math.floor(step / 2)) {
      while (at + n + step <= ids.length) {
        const next = encodeSelectionIds(ids.slice(at, at + n + step));
        if (next.length > PRESENCE_PART_BYTES) break;
        n += step;
        bytes = next;
      }
    }
    parts.push(bytes);
    at += n;
  }
  return parts;
}

/** Кадр клиента: состояние (вид `kind`) — тело без вида кадра. */
export function encodePresenceBody(s: PresenceState): Uint8Array<ArrayBuffer> {
  const out = new Out();
  writePresenceState(out, s);
  return out.done();
}

export function encodePartBody(p: { rev: number; index: number; parts: number; bytes: Uint8Array }): Uint8Array<ArrayBuffer> {
  if (p.parts < 1 || p.parts > PRESENCE_PARTS_MAX || p.index >= p.parts) throw new RangeError('часть выделения');
  if (p.bytes.length < 1 || p.bytes.length > PRESENCE_PART_BYTES) throw new RangeError('байты части');
  return new Out().u32(p.rev).u8(p.index).u8(p.parts).push(p.bytes).done();
}

/** Тело состояния от клиента: разобрано строго, иначе `null`. Сервер пересылает эти же байты: лишнего в них нет. */
export function parsePresenceBody(b: Uint8Array): PresenceState | null {
  if (b.length > PRESENCE_FRAME_MAX) return null;
  const r = new In(b);
  const s = readPresenceState(r);
  return s && r.done() ? s : null;
}

export function parsePartBody(b: Uint8Array): PresencePart | null {
  if (b.length > PRESENCE_FRAME_MAX) return null;
  const r = new In(b);
  const rev = r.u32();
  const index = r.u8();
  const parts = r.u8();
  if (rev === null || index === null || parts === null || parts < 1 || parts > PRESENCE_PARTS_MAX || index >= parts) return null;
  const rest = r.bytes(r.end - r.at);
  if (!rest || rest.length > PRESENCE_PART_BYTES) return null;
  const ids = decodeSelectionIds(rest);
  return ids ? { rev, index, parts, ids } : null;
}

const ITEM = { join: 1, state: 2, part: 3, left: 4 } as const;

/** Запись пачки сервера. Состояние и часть — байты, уже проверенные разбором при приёме. */
export type PresenceRecord =
  | { k: 'join'; participant: number; user: string; name: string; color: number }
  | { k: 'state'; participant: number; body: Uint8Array }
  | { k: 'part'; participant: number; body: Uint8Array }
  | { k: 'left'; participant: number };

export function encodePresenceRecord(rec: PresenceRecord): Uint8Array<ArrayBuffer> {
  const out = new Out().u8(ITEM[rec.k]).u32(rec.participant);
  switch (rec.k) {
    case 'join':
      if (rec.color < 0 || rec.color >= PRESENCE_COLORS.length) throw new RangeError('цвет');
      return out.str(rec.user, PRESENCE_USER_MAX).str(rec.name, PRESENCE_NAME_MAX).u8(rec.color).done();
    case 'state':
    case 'part':
      return out.u16(rec.body.length).push(rec.body).done();
    case 'left':
      return out.done();
  }
}

/** Пачка сервера: записи подряд. Разбор у клиента строгий: сеть не доверенная, неверное — `null`. */
export function parsePresenceBatch(b: Uint8Array, at: number): PresenceItem[] | null {
  if (b.length - at > PRESENCE_BATCH_MAX) return null;
  const r = new In(b, at);
  const items: PresenceItem[] = [];
  while (!r.done()) {
    const kind = r.u8();
    const participant = r.u32();
    if (kind === null || participant === null) return null;
    switch (kind) {
      case ITEM.join: {
        const user = r.str(PRESENCE_USER_MAX);
        const name = r.str(PRESENCE_NAME_MAX);
        const color = r.u8();
        if (user === null || user.length === 0 || name === null || color === null || color >= PRESENCE_COLORS.length) return null;
        items.push({ k: 'join', participant, user, name, color });
        break;
      }
      case ITEM.state:
      case ITEM.part: {
        const n = r.u16();
        if (n === null) return null;
        const body = r.bytes(n);
        if (!body) return null;
        if (kind === ITEM.state) {
          const state = parsePresenceBody(body);
          if (!state) return null;
          items.push({ k: 'state', participant, state });
        } else {
          const part = parsePartBody(body);
          if (!part) return null;
          items.push({ k: 'part', participant, part });
        }
        break;
      }
      case ITEM.left:
        items.push({ k: 'left', participant });
        break;
      default:
        return null;
    }
  }
  return items.length > 0 ? items : null;
}
````

В `packages/protocol/src/sync.ts` заменить:

````ts

import type { FileRole } from './files';

/** Растёт при несовместимой смене кадров: старый клиент получает `please-reload`. */
export const SYNC_PROTOCOL = 1;
/** Предел кадра по спеке, раздел 7.3. Вся полезная нагрузка — байты Loro, заголовок сверху не считается. */
export const MAX_SYNC_UPDATE_BYTES = 16 * 1024 * 1024;
````

на:

````ts

import type { FileRole } from './files';
import {
  parsePartBody,
  parsePresenceBatch,
  parsePresenceBody,
  PRESENCE_BATCH_MAX,
  PRESENCE_FRAME_MAX,
  type PresenceItem,
  type PresencePart,
  type PresenceState,
} from './presence';

/** Растёт при несовместимой смене кадров: старый клиент получает `please-reload`. 2 — присутствие (этап 4): кадров
 * `presence` прежний клиент не знает и счёл бы соединение испорченным. */
export const SYNC_PROTOCOL = 2;
/** Предел кадра по спеке, раздел 7.3. Вся полезная нагрузка — байты Loro, заголовок сверху не считается. */
export const MAX_SYNC_UPDATE_BYTES = 16 * 1024 * 1024;
````

В `packages/protocol/src/sync.ts` заменить:

````ts
/** Вектор версий: столько сеансов правки на один документ не набирается, а чужой кадр не задаёт размер разбора. */
export const MAX_VERSION_PEERS = 65_536;
/** Присутствие (этап 4) — курсор и выделение, килобайты: кадр в мегабайты — не присутствие. */
export const MAX_EPHEMERAL_BYTES = 64 * 1024;
const MAX_DOC_BYTES = 40;
/** Документ — `root` или `TreeID` страницы `counter@peer` в той же записи, что у Loro: без ведущих нулей, иначе один документ жил бы
````

на:

````ts
/** Вектор версий: столько сеансов правки на один документ не набирается, а чужой кадр не задаёт размер разбора. */
export const MAX_VERSION_PEERS = 65_536;
const MAX_DOC_BYTES = 40;
/** Документ — `root` или `TreeID` страницы `counter@peer` в той же записи, что у Loro: без ведущих нулей, иначе один документ жил бы
````

В `packages/protocol/src/sync.ts` заменить:

````ts
  | { t: 'subscribe'; doc: string; version: Uint8Array }
  | { t: 'update'; doc: string; clientSeq: number; bytes: Uint8Array }
  | { t: 'ephemeral'; bytes: Uint8Array }
  | { t: 'ping' };

````

на:

````ts
  | { t: 'subscribe'; doc: string; version: Uint8Array }
  | { t: 'update'; doc: string; clientSeq: number; bytes: Uint8Array }
  /** Присутствие (спека, раздел 9): `body` — тело `encodePresenceBody`. При разборе `state` — разобранное тело, `null` —
   * тело неверно или больше 2 КБ: сервер такой кадр молча отбрасывает, а не рвёт соединение. */
  | { t: 'presence'; body: Uint8Array; state?: PresenceState | null }
  /** Часть выделения: тело `encodePartBody`, при разборе `part` — как `state` выше. */
  | { t: 'presence-part'; body: Uint8Array; part?: PresencePart | null }
  | { t: 'ping' };

````

В `packages/protocol/src/sync.ts` заменить:

````ts
  | { t: 'access-revoked'; reason: RevokeReason }
  | { t: 'please-reload' }
  | { t: 'pong' };

const KIND = {
````

на:

````ts
  | { t: 'access-revoked'; reason: RevokeReason }
  | { t: 'please-reload' }
  | { t: 'pong' }
  /** Пачка присутствия других участников файла (`encodePresenceBatch`). */
  | { t: 'presence'; items: PresenceItem[] };

const KIND = {
````

В `packages/protocol/src/sync.ts` заменить:

````ts
  subscribe: 2,
  update: 3,
  ephemeral: 4,
  ping: 5,
  role: 16,
  subscribed: 17,
````

на:

````ts
  subscribe: 2,
  update: 3,
  presence: 4,
  ping: 5,
  'presence-part': 6,
  role: 16,
  subscribed: 17,
````

В `packages/protocol/src/sync.ts` заменить:

````ts
  'please-reload': 24,
  pong: 25,
} as const;

````

на:

````ts
  'please-reload': 24,
  pong: 25,
  'presence-batch': 26,
} as const;

````

В `packages/protocol/src/sync.ts` заменить:

````ts
    case 'update':
      return w.doc(f.doc).u32(f.clientSeq).bytes(f.bytes).done();
    case 'ephemeral':
      return w.bytes(f.bytes).done();
    case 'ping':
      return w.done();
  }
}

````

на:

````ts
    case 'update':
      return w.doc(f.doc).u32(f.clientSeq).bytes(f.bytes).done();
    case 'presence':
    case 'presence-part':
      return w.bytes(f.body).done();
    case 'ping':
      return w.done();
  }
}

/** Пачка присутствия одному получателю: записи подряд, не больше `PRESENCE_BATCH_MAX`. */
export function encodePresenceBatch(records: Uint8Array[]): Uint8Array<ArrayBuffer> {
  const w = new Writer().u8(KIND['presence-batch']);
  for (const r of records) w.bytes(r);
  const out = w.done();
  if (out.length - 1 > PRESENCE_BATCH_MAX) throw new RangeError('пачка присутствия больше предела');
  return out;
}

````

В `packages/protocol/src/sync.ts` заменить:

````ts
    case 'pong':
      return w.done();
  }
}
````

на:

````ts
    case 'pong':
      return w.done();
    case 'presence':
      throw new RangeError('пачку присутствия собирает encodePresenceBatch');
  }
}
````

В `packages/protocol/src/sync.ts` заменить:

````ts
      return bytes.length > 0 && bytes.length <= MAX_SYNC_UPDATE_BYTES ? { t: 'update', doc, clientSeq, bytes } : null;
    }
    case KIND.ephemeral: {
      const bytes = r.rest();
      return bytes.length <= MAX_EPHEMERAL_BYTES ? { t: 'ephemeral', bytes } : null;
    }
    case KIND.ping:
````

на:

````ts
      return bytes.length > 0 && bytes.length <= MAX_SYNC_UPDATE_BYTES ? { t: 'update', doc, clientSeq, bytes } : null;
    }
    case KIND.presence: {
      const body = r.rest();
      return { t: 'presence', body, state: b.length <= PRESENCE_FRAME_MAX ? parsePresenceBody(body) : null };
    }
    case KIND['presence-part']: {
      const body = r.rest();
      return { t: 'presence-part', body, part: b.length <= PRESENCE_FRAME_MAX ? parsePartBody(body) : null };
    }
    case KIND.ping:
````

В `packages/protocol/src/sync.ts` заменить:

````ts
    case KIND.pong:
      return r.done() ? { t: 'pong' } : null;
    default:
      return null;
````

на:

````ts
    case KIND.pong:
      return r.done() ? { t: 'pong' } : null;
    case KIND['presence-batch']: {
      const items = parsePresenceBatch(b, 1);
      return items ? { t: 'presence', items } : null;
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

Expected: PASS: всё без замечаний; `test:server` — протокол 42 pass, сервер 488 pass; `sync-client.test.ts` 47 passed

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/src/sync/hub.ts apps/server/test/hub.test.ts apps/server/test/sync-load-probe.ts apps/web/unit/sync-client.test.ts packages/protocol/src/index.ts packages/protocol/src/presence.ts packages/protocol/src/sync.ts packages/protocol/test/presence.test.ts packages/protocol/test/sync.test.ts
git commit -F - <<'EOF'
протокол: кадры присутствия — состояние участника, части выделения, пачка сервера

Строгие двоичные кадры вместо EphemeralStore: сервер читает каждое поле, чтобы подписать кадр своим участником (проба: 0,3 мкс
против 54 мкс у EphemeralStore). Выделение — частями до 2 КБ (пиры по возрастанию, счётчики разностями), жест — одна матрица.
Протокол 2: прежняя вкладка получает please-reload.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 2: Сервер — присутствие

Спека §9; «Решения» 2, 5–7. `PresenceHub`: подпись участником, последнее состояние в памяти, пачки по тику без очередей, пределы на соединение и человека, уход сразу при закрытии. Нагрузочная проба на 50 участников и враждебного.

**Files:**
- Create: `apps/server/src/sync/presence.ts`
- Modify: `apps/server/src/sync/hub.ts`
- Test: `apps/server/test/presence-load-probe.ts` (новый), `apps/server/test/presence-load.test.ts` (новый), `apps/server/test/presence.test.ts` (новый)

**Interfaces:**
- Consumes: кадры задачи 1, `SyncHub` этапа 3.
- Produces: `PresenceHub`, `PRESENCE_LIMITS`, `hub.presence`, `SyncLimits.presence`, `ConnData.name`.

- [ ] **Step 1: Написать тесты**

Создать `apps/server/test/presence-load-probe.ts`:

````ts
// Нагрузка присутствия отдельным процессом (спека подсистемы 4, разделы 9 и 13.1): клиенты — дочерний процесс, поэтому
// процессор и память процесса — только сервера. Печатает итоги в JSON.
//   load N HZ SECS [hostile] — N участников, каждый шлёт курсор HZ раз в секунду SECS секунд. Задержка — от отправки до
//                              прихода к другому участнику (часы одного процесса клиентов). `hostile` — ещё один
//                              участник шлёт кадры по 2 КБ без остановки, а один сокет не читает вовсе.

import {
  encodeClientFrame,
  encodePartBody,
  encodePresenceBody,
  parseServerFrame,
  splitSelection,
  SYNC_PROTOCOL,
  syncPath,
  type PresenceState,
} from '@ferrum/protocol';
import { connect } from 'node:net';
import { apiStand } from './api-stand';

const [mode] = process.argv.slice(2);
const page = '3@7910247476534078275';
// Часы процесса клиентов: и отправка, и приём — в нём.
const clock = () => performance.now();

if (mode === 'clients') {
  const [n, hz, secs, url, cookie, origin, hostile] = process.argv.slice(3);
  const count = Number(n);
  const sockets: WebSocket[] = [];
  const lat: number[] = [];
  let ready = 0;
  let received = 0;
  for (let i = 0; i < count; i++) {
    const ws = new WebSocket(url, { headers: { cookie, origin } } as never);
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => ws.send(encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 }));
    ws.onmessage = (e) => {
      const f = parseServerFrame(new Uint8Array(e.data as ArrayBuffer));
      if (f?.t === 'role' && ++ready === count) console.log('ready');
      if (f?.t !== 'presence') return;
      const now = clock();
      for (const it of f.items) {
        if (it.k !== 'state' || !it.state.cursor) continue;
        received++;
        // Курсор x — момент отправки.
        if (it.state.cursor[1] === 1) lat.push(now - it.state.cursor[0]);
      }
    };
    sockets.push(ws);
    if (i % 25 === 24) await Bun.sleep(5);
  }
  while (ready < count) await Bun.sleep(5);
  // У каждого — выделение из 1000 узлов, как у перетаскивающего.
  const ids = Array.from({ length: 1000 }, (_, i) => `${10 + i * 12}@7910247476534078275`);
  const parts = splitSelection(ids);
  const selection = { rev: 1, count: ids.length, parts: parts.length, bounds: [0, 0, 10, 10] as [number, number, number, number] };
  let flood: ReturnType<typeof setInterval> | null = null;
  if (hostile === 'hostile') {
    const big = encodePresenceBody({
      page,
      view: [0, 0, 1, 1],
      cursor: [0, 2],
      selection: { rev: 9, count: 1, parts: 16, bounds: [0, 0, 1, 1] },
      gesture: null,
    });
    const huge = new Uint8Array(2040);
    huge.set(big);
    const evil = sockets[0];
    // Без остановки: ~4000 кадров в секунду, верных и неверных вперемешку.
    flood = setInterval(() => {
      for (let k = 0; k < 40; k++) {
        evil.send(encodeClientFrame({ t: 'presence', body: big }));
        evil.send(encodeClientFrame({ t: 'presence', body: huge }));
      }
    }, 10);
  }
  const step = 1000 / Number(hz);
  const start = performance.now();
  let sent = 0;
  for (let k = 0; performance.now() - start < Number(secs) * 1000; k++) {
    const t0 = performance.now();
    for (const [i, ws] of sockets.entries()) {
      if (hostile === 'hostile' && i === 0) continue;
      const st: PresenceState = {
        page,
        view: [0, 0, 1920, 1080],
        cursor: [clock(), 1],
        selection,
        gesture: k % 4 === 0 ? { kind: 'move', transform: [1, 0, 0, 1, k, k], frame: [1, 0, 0, 1, 0, 0], k: 1 } : null,
      };
      ws.send(encodeClientFrame({ t: 'presence', body: encodePresenceBody(st) }));
      sent++;
      // Части выделения — за первым состоянием, как у клиента.
      if (k === 0)
        parts.forEach((bytes, index) =>
          ws.send(encodeClientFrame({ t: 'presence-part', body: encodePartBody({ rev: 1, index, parts: parts.length, bytes }) })),
        );
    }
    const wait = step - (performance.now() - t0);
    if (wait > 0) await Bun.sleep(wait);
  }
  if (flood) clearInterval(flood);
  await Bun.sleep(500);
  const sorted = lat.sort((a, b) => a - b);
  const at = (p: number) => sorted[Math.min(sorted.length - 1, Math.ceil((sorted.length * p) / 100) - 1)] ?? -1;
  console.log(JSON.stringify({ sent, received, samples: sorted.length, p50: at(50), p95: at(95), p99: at(99), max: at(100) }));
  // Родитель убивает процесс клиентов; если он упал раньше — выход сам.
  setTimeout(() => process.exit(0), 10_000);
  await new Promise(() => {});
}

// Нечего ждать: дочерний процесс клиентов без родителя не живёт.
const s = await apiStand({
  syncLimits: {
    connectionsPerUser: 100_000,
    userOpensBurst: 100_000,
    presence: { userFramesPerSecond: 100_000, userFramesBurst: 100_000 },
  },
});
const who = await s.signIn(`presence-load-${Date.now()}@example.com`);
const file = (await s.newFile(who)).id;
const url = s.publicUrl.replace('http', 'ws') + syncPath(file, who.id);
const [, n, hz, secs, hostile = ''] = process.argv.slice(2);

// Сокет, который не читает: буфер сервера для него не должен расти.
let raw: ReturnType<typeof connect> | null = null;
if (hostile === 'hostile') {
  raw = connect(Number(new URL(s.publicUrl).port), '127.0.0.1');
  raw.write(
    `GET ${syncPath(file, who.id)} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
      `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: ${s.publicUrl}\r\nCookie: ${who.cookie}\r\n\r\n`,
  );
  await Bun.sleep(200);
  const hello = encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 });
  raw.write(Buffer.concat([Buffer.from([0x82, 0x80 | hello.length, 0, 0, 0, 0]), hello]));
  await Bun.sleep(200);
  raw.pause();
}

Bun.gc(true);
const rss0 = process.memoryUsage().rss;
const heap0 = process.memoryUsage().heapUsed;
const child = Bun.spawn(['bun', import.meta.path, 'clients', n, hz, secs, url, who.cookie, s.publicUrl, hostile], {
  stdout: 'pipe',
  stderr: 'inherit',
});
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
const cpu0 = process.cpuUsage();
const t0 = performance.now();
let peakBuffered = 0;
let peakHeld = 0;
let peakRss = 0;
const probe = setInterval(() => {
  peakBuffered = Math.max(peakBuffered, s.hub.bufferedBytes());
  peakHeld = Math.max(peakHeld, s.hub.presence.heldBytes());
  peakRss = Math.max(peakRss, process.memoryUsage().rss);
}, 20);
const report = JSON.parse(await next()) as Record<string, number>;
clearInterval(probe);
const cpu = process.cpuUsage(cpu0);
const wall = performance.now() - t0;
Bun.gc(true);
const rssAfter = process.memoryUsage().rss;
// Живая куча после сборки мусора, пока участники на месте: RSS не возвращает страницы системе и показывает пик.
const heapAfter = process.memoryUsage().heapUsed;
console.log(
  JSON.stringify({
    participants: Number(n),
    hz: Number(hz),
    ...report,
    serverCpuPercent: ((cpu.user + cpu.system) / 1000 / wall) * 100,
    batches: s.hub.presence.sent,
    deferred: s.hub.presence.deferred,
    dropped: s.hub.presence.dropped,
    rssPeakGrowthMb: (peakRss - rss0) / 1e6,
    rssAfterGcGrowthMb: (rssAfter - rss0) / 1e6,
    heapGrowthMb: (heapAfter - heap0) / 1e6,
    peakBufferedKb: peakBuffered / 1024,
    peakHeldKb: peakHeld / 1024,
  }),
);
child.kill();
raw?.destroy();
await s.stop();
process.exit(0);
````

Создать `apps/server/test/presence-load.test.ts`:

````ts
// Нагрузка присутствия отдельным процессом (спека подсистемы 4, раздел 9): 50 участников по 20 кадров в секунду, и то же с
// враждебным участником (поток кадров по 2 КБ) и сокетом, который не читает.

import { fileURLToPath } from 'node:url';
import { expect, test } from 'bun:test';

async function run(...args: string[]): Promise<Record<string, number>> {
  const p = Bun.spawn(['bun', fileURLToPath(new URL('./presence-load-probe.ts', import.meta.url)), ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [text, err] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  await p.exited;
  const line = text.trim().split('\n').at(-1) ?? '';
  try {
    return JSON.parse(line) as Record<string, number>;
  } catch {
    throw new Error(`проба упала: ${text}\n${err.slice(-2000)}`);
  }
}

test('50 участников по 20 кадров в секунду: всё доходит, память — по участникам, а не по кадрам', async () => {
  const r = await run('load', '50', '20', '5');
  console.log('присутствие, 50 × 20 Гц', r);
  // Каждый получает кадры остальных 49; слияние в тике (33 мс) склеивает соседние кадры одного участника.
  expect(r.received).toBeGreaterThan(r.sent * 49 * 0.5);
  expect(r.dropped).toBe(0);
  expect(r.heapGrowthMb).toBeLessThan(20);
  expect(r.peakHeldKb).toBeLessThan(50 * 40);
}, 120_000);

test('враждебный участник и сокет, который не читает: остальные получают своё, сервер не копит', async () => {
  const r = await run('load', '50', '20', '5', 'hostile');
  console.log('присутствие, враждебный участник', r);
  expect(r.received).toBeGreaterThan(r.sent * 48 * 0.5);
  expect(r.dropped).toBeGreaterThan(1000);
  // Пропуск тиков у нечитающего сокета виден, когда заполнится буфер ядра ОС (за 5 с — не всегда): его проверяет
  // `presence.test.ts`. Здесь — что буфер сервера не растёт.
  expect(r.peakBufferedKb).toBeLessThan(512);
  expect(r.heapGrowthMb).toBeLessThan(20);
}, 120_000);
````

Создать `apps/server/test/presence.test.ts`:

````ts
// Присутствие на настоящем сервере (спека подсистемы 4, раздел 9): настоящие сокеты, база и сессии. Кто участник — знает
// только сервер; уход, снятие роли и выход убирают человека у других сразу; пределы молча отбрасывают, не рвут.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import {
  encodeClientFrame,
  encodePartBody,
  encodePresenceBody,
  encodeSelectionIds,
  presenceColor,
  splitSelection,
  syncPath,
  type PresenceItem,
  type PresenceState,
} from '@ferrum/protocol';
import { connect as tcp } from 'node:net';
import { apiStand, type Person } from './api-stand';
import { Client } from './sync-client';

const STALE_MS = 1500;
const s = await apiStand({
  syncLimits: { recheckMs: 3_600_000, sessionCheckMs: 3_600_000, presence: { staleMs: STALE_MS } },
});
afterAll(s.stop);
const owner = await s.signIn('p-owner@example.com', 'Анна');
const editor = await s.signIn('p-editor@example.com', 'Борис');
const viewer = await s.signIn('p-viewer@example.com', 'Вера');
const other = await s.signIn('p-other@example.com', 'Глеб');
const leaver = await s.signIn('p-leaver@example.com', 'Дина');

let file = '';
const open: Client[] = [];

const page = '3@7910247476534078275';
const state = (x: number, extra: Partial<PresenceState> = {}): PresenceState => ({
  page,
  view: [0, 0, 1920, 1080],
  cursor: [x, 10],
  selection: null,
  gesture: null,
  ...extra,
});

async function join(who: Person, f = file): Promise<Client> {
  const c = await Client.open(s, who, f);
  if (!c) throw new Error('соединение не открылось');
  open.push(c);
  c.hello();
  await c.wait('role');
  return c;
}

const send = (c: Client, st: PresenceState) => c.send(encodeClientFrame({ t: 'presence', body: encodePresenceBody(st) }));

/** Все записи присутствия, пришедшие клиенту. */
const items = (c: Client): PresenceItem[] => c.frames.flatMap((f) => (f.t === 'presence' ? f.items : []));

async function waitItem<K extends PresenceItem['k']>(
  c: Client,
  k: K,
  match: (i: Extract<PresenceItem, { k: K }>) => boolean = () => true,
  ms = 5000,
): Promise<Extract<PresenceItem, { k: K }>> {
  const deadline = performance.now() + ms;
  for (;;) {
    const found = items(c).find((i) => i.k === k && match(i as Extract<PresenceItem, { k: K }>));
    if (found) return found as Extract<PresenceItem, { k: K }>;
    if (performance.now() > deadline) throw new Error(`нет записи ${k}: ${JSON.stringify(items(c))}`);
    await Bun.sleep(10);
  }
}

/** Несколько тиков рассылки: всё, что могло прийти, пришло. */
const ticks = () => Bun.sleep(150);

beforeEach(async () => {
  for (const c of open.splice(0)) c.close();
  await s.reset();
  file = (await s.newFile(owner)).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
    file,
    editor.id,
    viewer.id,
  ]);
});

test('участник виден другим: кто, имя и цвет — с сервера, своё состояние себе не приходит', async () => {
  const a = await join(owner);
  const b = await join(editor);
  send(a, state(42));
  const j = await waitItem(b, 'join');
  expect(j).toMatchObject({ user: owner.id, name: 'Анна', color: presenceColor(owner.id) });
  const st = await waitItem(b, 'state', (i) => i.participant === j.participant);
  expect(st.state).toEqual(state(42));
  await ticks();
  expect(items(a)).toEqual([]);
});

test('кадр нельзя выдать за другого: имени и userId в нём нет, лишние байты — молча мимо, соединение живо', async () => {
  const a = await join(owner);
  const b = await join(editor);
  const body = encodePresenceBody(state(1));
  // Хвост с «чужим» именем: разбор строгий, кадр отбрасывается целиком.
  a.send(encodeClientFrame({ t: 'presence', body: new Uint8Array([...body, ...new TextEncoder().encode(editor.id)]) }));
  // Нечисловой курсор.
  const nan = body.slice();
  new DataView(nan.buffer).setFloat64(nan.length - 8, NaN, true);
  a.send(encodeClientFrame({ t: 'presence', body: nan }));
  // Кадр больше 2 КБ.
  a.send(encodeClientFrame({ t: 'presence', body: new Uint8Array(4096) }));
  await ticks();
  expect(items(b)).toEqual([]);
  expect(a.closed).toBeNull();
  send(a, state(2));
  expect((await waitItem(b, 'join')).user).toBe(owner.id);
  expect(
    items(b)
      .filter((i) => i.k === 'join')
      .every((i) => i.k === 'join' && i.user === owner.id),
  ).toBe(true);
});

test('присутствие не уходит в другой файл и тому, у кого нет доступа', async () => {
  const otherFile = (await s.newFile(other)).id;
  const a = await join(owner);
  const elsewhere = await join(other, otherFile);
  expect(await Client.open(s, other, file)).toBeNull();
  send(a, state(5));
  send(elsewhere, state(6));
  const b = await join(editor);
  await waitItem(b, 'state', (i) => i.state.cursor?.[0] === 5);
  await ticks();
  expect(items(elsewhere)).toEqual([]);
  expect(items(b).some((i) => i.k === 'state' && i.state.cursor?.[0] === 6)).toBe(false);
});

test('читатель виден и видит других (как в Figma)', async () => {
  const a = await join(owner);
  const v = await join(viewer);
  send(v, state(7));
  send(a, state(8));
  expect((await waitItem(a, 'join')).user).toBe(viewer.id);
  expect((await waitItem(v, 'join')).user).toBe(owner.id);
});

test('пришедший позже получает всех сразу: состояние и выделение частями', async () => {
  const a = await join(owner);
  const ids = Array.from({ length: 1000 }, (_, i) => `${10 + i * 12}@7910247476534078275`);
  const parts = splitSelection(ids);
  send(a, state(1, { selection: { rev: 4, count: ids.length, parts: parts.length, bounds: [0, 0, 10, 10] } }));
  parts.forEach((bytes, index) =>
    a.send(encodeClientFrame({ t: 'presence-part', body: encodePartBody({ rev: 4, index, parts: parts.length, bytes }) })),
  );
  await ticks();
  const late = await join(editor);
  const j = await waitItem(late, 'join');
  await waitItem(late, 'state', (i) => i.participant === j.participant);
  const got = await waitItem(late, 'part', (i) => i.participant === j.participant);
  expect(got.part.ids).toEqual(ids);
});

test('новая версия выделения заменяет части прежней', async () => {
  const a = await join(owner);
  const b = await join(editor);
  const part = (rev: number, id: string) =>
    a.send(encodeClientFrame({ t: 'presence-part', body: encodePartBody({ rev, index: 0, parts: 1, bytes: encodeSelectionIds([id]) }) }));
  send(a, state(1, { selection: { rev: 1, count: 1, parts: 1, bounds: [0, 0, 1, 1] } }));
  part(1, '1@2');
  await waitItem(b, 'part', (i) => i.part.rev === 1);
  send(a, state(1, { selection: { rev: 2, count: 1, parts: 1, bounds: [0, 0, 1, 1] } }));
  part(2, '9@2');
  await waitItem(b, 'part', (i) => i.part.rev === 2);
  const late = await join(viewer);
  const got = await waitItem(late, 'part');
  expect(got.part).toEqual({ rev: 2, index: 0, parts: 1, ids: ['9@2'] });
  await ticks();
  expect(items(late).filter((i) => i.k === 'part')).toHaveLength(1);
});

test('закрыл соединение — пропал у других сразу; переподключение не двоит участника', async () => {
  const a = await join(owner);
  const b = await join(editor);
  send(a, state(1));
  const first = await waitItem(b, 'join');
  const at = performance.now();
  a.close();
  await waitItem(b, 'left', (i) => i.participant === first.participant);
  expect(performance.now() - at).toBeLessThan(1000);
  expect(s.hub.presence.shown(file)).toBe(0);
  const again = await join(owner);
  send(again, state(2));
  const second = await waitItem(b, 'join', (i) => i.participant !== first.participant);
  expect(second.user).toBe(owner.id);
  expect(s.hub.presence.shown(file)).toBe(1);
});

test('снятие роли убирает человека у других сразу', async () => {
  const a = await join(owner);
  const e = await join(editor);
  send(e, state(3));
  const j = await waitItem(a, 'join', (i) => i.user === editor.id);
  const entry = await s.pool.query<{ id: string }>('SELECT id FROM file_access WHERE file_id = $1 AND user_id = $2', [file, editor.id]);
  const res = await s.call(owner, 'DELETE', `/api/files/${file}/share/people/${entry.rows[0].id}`);
  expect(res.status).toBeLessThan(300);
  await e.waitClosed();
  await waitItem(a, 'left', (i) => i.participant === j.participant);
});

test('выход убирает человека у других сразу', async () => {
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, leaver.id]);
  const a = await join(owner);
  const v = await join(leaver);
  send(v, state(3));
  const j = await waitItem(a, 'join', (i) => i.user === leaver.id);
  const res = await s.call(leaver, 'POST', '/api/auth/sign-out', {});
  expect(res.status).toBe(200);
  await v.waitClosed();
  await waitItem(a, 'left', (i) => i.participant === j.participant);
});

test('файл в корзине: у остальных соединения закрыты, и убравший их больше не видит', async () => {
  const a = await join(owner);
  const e = await join(editor);
  send(e, state(3));
  const j = await waitItem(a, 'join');
  const res = await s.call(owner, 'POST', `/api/files/${file}/trash`, {});
  expect(res.status).toBeLessThan(300);
  await e.waitClosed();
  await waitItem(a, 'left', (i) => i.participant === j.participant);
  expect(s.hub.presence.shown(file)).toBe(0);
});

test('до подтверждения роли кадры присутствия не принимаются', async () => {
  const b = await join(editor);
  const c = (await Client.open(s, owner, file))!;
  open.push(c);
  c.hello();
  // Кадр сразу за `hello`: роль ещё проверяется запросом.
  send(c, state(99));
  await c.wait('role');
  await ticks();
  expect(items(b)).toEqual([]);
  send(c, state(100));
  await waitItem(b, 'state', (i) => i.state.cursor?.[0] === 100);
});

test('молчит дольше срока — пропадает; следующий кадр возвращает его', async () => {
  const a = await join(owner);
  const b = await join(editor);
  send(a, state(1));
  const j = await waitItem(b, 'join');
  await waitItem(b, 'left', (i) => i.participant === j.participant, STALE_MS * 3);
  send(a, state(2));
  await waitItem(b, 'state', (i) => i.state.cursor?.[0] === 2);
  expect(items(b).filter((i) => i.k === 'join')).toHaveLength(2);
});

test('сверх предела частоты кадры молча отбрасываются: ни паузы, ни закрытия; получатель видит последнее', async () => {
  const a = await join(owner);
  const b = await join(editor);
  const dropped = s.hub.presence.dropped;
  for (let i = 0; i < 2500; i++) send(a, state(i));
  await ticks();
  // Больше запаса бакета правок (2000): присутствие его не тратит, иначе правки получили бы паузу.
  expect(s.hub.presence.dropped - dropped).toBeGreaterThan(2000);
  expect(a.closed).toBeNull();
  expect(a.frames.some((f) => f.t === 'pause')).toBe(false);
  // Правки после потока не ждут: присутствие не тратит бакеты правок.
  a.subscribe('root');
  await a.wait('subscribed');
  // Рассылка сливает кадры: пачек не больше, чем тиков.
  const states = items(b).filter((i) => i.k === 'state');
  expect(states.length).toBeLessThan(60);
});

test('настоящая работа пределы не задевает: курсор 20 раз в секунду, выделение 1000 узлов, перетаскивание', async () => {
  const a = await join(owner);
  const tab2 = await join(owner);
  const b = await join(editor);
  const ids = Array.from({ length: 1000 }, (_, i) => `${10 + i * 12}@7910247476534078275`);
  const parts = splitSelection(ids);
  const dropped = s.hub.presence.dropped;
  const selection = { rev: 1, count: ids.length, parts: parts.length, bounds: [0, 0, 10, 10] as [number, number, number, number] };
  for (let i = 0; i < 60; i++) {
    const gesture = i > 10 ? ({ kind: 'move', transform: [1, 0, 0, 1, i, i], frame: [1, 0, 0, 1, 0, 0], k: 1 } as const) : null;
    for (const c of [a, tab2]) {
      send(c, state(i, { selection, gesture: gesture && { ...gesture, transform: [...gesture.transform], frame: [...gesture.frame] } }));
      if (i === 0)
        parts.forEach((bytes, index) =>
          c.send(encodeClientFrame({ t: 'presence-part', body: encodePartBody({ rev: 1, index, parts: parts.length, bytes }) })),
        );
    }
    await Bun.sleep(50);
  }
  await ticks();
  expect(s.hub.presence.dropped - dropped).toBe(0);
  const last = items(b).filter((i) => i.k === 'state');
  expect(last.at(-1)?.k === 'state' && last.at(-1)?.state.gesture?.transform[4]).toBe(59);
});

test('медленный читатель: сервер не копит для него кадры, а после чтения он получает последнее', async () => {
  const a = await join(owner);
  // Настоящий сокет, который не читает.
  const raw = tcp(Number(new URL(s.publicUrl).port), '127.0.0.1');
  const got: Buffer[] = [];
  raw.on('data', (b: Buffer) => got.push(b));
  raw.write(
    `GET ${syncPath(file, editor.id)} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
      `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: ${s.publicUrl}\r\nCookie: ${editor.cookie}\r\n\r\n`,
  );
  const mask = (p: Uint8Array): Buffer => Buffer.concat([Buffer.from([0x82, 0x80 | p.length]), Buffer.from([0, 0, 0, 0]), p]);
  await Bun.sleep(200);
  raw.write(mask(encodeClientFrame({ t: 'hello', protocol: 2, schema: 1 })));
  await Bun.sleep(300);
  raw.pause();
  // Много участников с большим выделением (16 частей по 2 КБ) шлют без остановки: в сумме десятки мегабайт.
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, other.id]);
  const writers = [a];
  for (const who of [owner, owner, viewer, viewer, viewer, other, other, other]) writers.push(await join(who));
  const rnd = (() => {
    let x = 7;
    return () => (x = (x * 48271) % 0x7fffffff);
  })();
  const ids = Array.from({ length: 40_000 }, () => `${rnd() % 2_000_000_000}@${rnd() % 20}`);
  const parts = splitSelection(ids);
  expect(parts.length).toBe(16);
  const deferred = s.hub.presence.deferred;
  let peakHeld = 0;
  let peakBuffered = 0;
  for (let round = 0; round < 150; round++) {
    for (const w of writers) {
      send(w, state(round, { selection: { rev: round, count: ids.length, parts: parts.length, bounds: [0, 0, 1, 1] } }));
      // Части — по одной на кадр, в пределе частоты соединения.
      const index = round % parts.length;
      w.send(
        encodeClientFrame({ t: 'presence-part', body: encodePartBody({ rev: round, index, parts: parts.length, bytes: parts[index] }) }),
      );
    }
    peakHeld = Math.max(peakHeld, s.hub.presence.heldBytes());
    peakBuffered = Math.max(peakBuffered, s.hub.bufferedBytes());
    await Bun.sleep(40);
  }
  console.log('медленный читатель присутствия', {
    deferred: s.hub.presence.deferred - deferred,
    peakHeldKb: peakHeld / 1024,
    peakBufferedKb: peakBuffered / 1024,
  });
  expect(s.hub.presence.deferred - deferred).toBeGreaterThan(0);
  // Память присутствия — последнее состояние участников (до 17 записей по 2 КБ), а не число кадров.
  expect(peakHeld).toBeLessThan(writers.length * 40 * 1024);
  // Буфер сокетов сервера — до порога и одной пачки, сколько бы ни прислали.
  expect(peakBuffered).toBeLessThan(512 * 1024);
  raw.resume();
  await Bun.sleep(500);
  const bytes = Buffer.concat(got);
  expect(bytes.length).toBeGreaterThan(0);
  raw.destroy();
}, 60_000);
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/presence.test.ts test/presence-load.test.ts`
Expected: FAIL — 17 fail, 0 pass: у `Hub` нет `presence`, кадры присутствия сервер не принимает и не рассылает; проба падает на `s.hub.presence`

- [ ] **Step 3: Реализация**

В `apps/server/src/sync/hub.ts` заменить:

````ts
import { inTransaction } from '../db';
import { findDocSection } from './container-read';
import { UpdateLog, type Item, type LogLimits } from './update-log';
import { Validator } from './validator';
````

на:

````ts
import { inTransaction } from '../db';
import { findDocSection } from './container-read';
import { PresenceHub, type PresenceLimits } from './presence';
import { UpdateLog, type Item, type LogLimits } from './update-log';
import { Validator } from './validator';
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  helloTimer: Timer | null;
  lagSince: number;
}

````

на:

````ts
  helloTimer: Timer | null;
  lagSince: number;
  /** Имя человека для присутствия: из базы при `hello`, не из кадров клиента. */
  name: string;
}

````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  pumpRows: number;
  log: Partial<LogLimits>;
}

````

на:

````ts
  pumpRows: number;
  log: Partial<LogLimits>;
  presence: Partial<PresenceLimits>;
}

````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  pumpRows: 512,
  log: {},
};

````

на:

````ts
  pumpRows: 512,
  log: {},
  presence: {},
};

````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  readonly validator: Validator;
  readonly log: UpdateLog;
  private readonly limits: SyncLimits;
  private readonly conns = new Map<string, Set<ConnData>>();
````

на:

````ts
  readonly validator: Validator;
  readonly log: UpdateLog;
  /** Присутствие участников (спека, раздел 9): своё у каждого файла, по соединениям с подтверждённой ролью. */
  readonly presence: PresenceHub;
  private readonly limits: SyncLimits;
  private readonly conns = new Map<string, Set<ConnData>>();
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
    this.validator = new Validator(pool, blobs);
    this.log = new UpdateLog(pool, this.limits.log);
    this.log.onCommitted = (items, states) => {
      for (const { item, seq } of items) this.broadcast(item, seq);
````

на:

````ts
    this.validator = new Validator(pool, blobs);
    this.log = new UpdateLog(pool, this.limits.log);
    this.presence = new PresenceHub(this.limits.presence);
    this.log.onCommitted = (items, states) => {
      for (const { item, seq } of items) this.broadcast(item, seq);
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
      helloTimer: null,
      lagSince: 0,
    };
    if (!server.upgrade(req, { data })) return new Response(null, { status: 400 });
````

на:

````ts
      helloTimer: null,
      lagSince: 0,
      name: '',
    };
    if (!server.upgrade(req, { data })) return new Response(null, { status: 400 });
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
    const u = this.users.get(d.user);
    if (u) u.conns--;
    const set = this.conns.get(d.file);
    set?.delete(d);
````

на:

````ts
    const u = this.users.get(d.user);
    if (u) u.conns--;
    // Ушёл — пропадает у других сразу, а не через 30 с (спека, раздел 9).
    this.presence.leave(d);
    const set = this.conns.get(d.file);
    set?.delete(d);
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
      return;
    }
    const u = this.user(d.user);
    // Байты правки — в общий бакет; вектор подписки — в свой, маленький: он живёт в памяти, пока подписка ждёт базу.
````

на:

````ts
      return;
    }
    // Присутствие — мимо бакетов правок: его пределы свои, лишнее молча отбрасывается, а пауза правок от курсора была бы
    // ложной. Кадр подписывает соединение: кто, имя и цвет — с сервера.
    if (frame.t === 'presence') {
      this.presence.state(d, d.name, frame.body, frame.state);
      return;
    }
    if (frame.t === 'presence-part') {
      this.presence.part(d, frame.body, frame.part);
      return;
    }
    const u = this.user(d.user);
    // Байты правки — в общий бакет; вектор подписки — в свой, маленький: он живёт в памяти, пока подписка ждёт базу.
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
        );
        break;
      case 'presence':
      case 'presence-part':
        // Присутствие рассылается следующей задачей; кадр известен и проверен разбором.
        break;
    }
  }
````

на:

````ts
        );
        break;
    }
  }
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
    this.track(
      this.pool
        .query<{ version: string; member: number; rank: number | null }>(
          `WITH f AS (SELECT id, version, file_role_rank(id, $2) AS rank, file_member_rank(id, $2) AS member FROM file WHERE id = $1),
                up AS (UPDATE file SET schema_version = $3 FROM f
                       WHERE file.id = f.id AND f.rank >= $4 AND file.schema_version < $3 RETURNING 1)
           SELECT version, member, rank FROM f`,
          [d.file, d.user, Math.min(schema, this.limits.knownSchema), ROLE_RANK.editor],
        )
````

на:

````ts
    this.track(
      this.pool
        .query<{ version: string; member: number; rank: number | null; name: string | null }>(
          `WITH f AS (SELECT id, version, file_role_rank(id, $2) AS rank, file_member_rank(id, $2) AS member FROM file WHERE id = $1),
                up AS (UPDATE file SET schema_version = $3 FROM f
                       WHERE file.id = f.id AND f.rank >= $4 AND file.schema_version < $3 RETURNING 1)
           SELECT version, member, rank,
                  (SELECT coalesce(nullif(btrim(name), ''), split_part(email, '@', 1)) FROM "user" WHERE id = $2) AS name
           FROM f`,
          [d.file, d.user, Math.min(schema, this.limits.knownSchema), ROLE_RANK.editor],
        )
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
          }
          d.role = role;
          this.send(d, { t: 'role', role, canShare: rows[0].member >= ROLE_RANK.editor, version: Number(rows[0].version) });
        })
        .catch((e: unknown) => {
````

на:

````ts
          }
          d.role = role;
          d.name = rows[0].name ?? '';
          this.send(d, { t: 'role', role, canShare: rows[0].member >= ROLE_RANK.editor, version: Number(rows[0].version) });
          // Присутствие файла — только после роли, подтверждённой этим запросом: снятый доступ его не увидит.
          this.presence.admit(d);
        })
        .catch((e: unknown) => {
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
    this.stopped = true;
    for (const t of this.timers) clearInterval(t);
    await this.log.stop();
    for (const set of this.conns.values()) for (const d of set) d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'restart');
````

на:

````ts
    this.stopped = true;
    for (const t of this.timers) clearInterval(t);
    this.presence.stop();
    await this.log.stop();
    for (const set of this.conns.values()) for (const d of set) d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'restart');
````

Создать `apps/server/src/sync/presence.ts`:

````ts
// Присутствие участников файла (спека подсистемы 4, раздел 9): сервер подписывает кадр участником своего соединения, держит
// последнее состояние каждого (для пришедших позже) и рассылает пачками по таймеру.
//
// Чего здесь нет намеренно: очереди кадров. У каждого получателя — только отметки «что у кого изменилось»; на таймере в пачку
// идёт последнее состояние, а не каждый кадр. Медленный читатель (буфер сокета выше `lowWaterBytes`) пропускает тики, его
// отметки сливаются, а память не растёт: работа сервера на получателя — не больше пачки за тик, сколько бы кадров ни
// присылали другие. Лишнее сверх пределов частоты молча отбрасывается: присутствие не рвёт соединение.

import {
  encodePresenceBatch,
  encodePresenceRecord,
  presenceColor,
  PRESENCE_BATCH_MAX,
  PRESENCE_NAME_MAX,
  type PresencePart,
  type PresenceState,
} from '@ferrum/protocol';

export interface PresenceLimits {
  /** Кадров присутствия на соединение: клиент шлёт не больше 20 в секунду, части выделения — сверх того. */
  framesPerSecond: number;
  framesBurst: number;
  /** На человека по всем вкладкам и соединениям. */
  userFramesPerSecond: number;
  userFramesBurst: number;
  /** Пачка каждому получателю не чаще. */
  tickMs: number;
  /** Без кадров дольше — участник пропадает у других (спека, раздел 9). */
  staleMs: number;
  /** Буфер сокета получателя выше этого — пачка ждёт следующего тика; правки документа идут своим путём до `highWaterBytes`. */
  lowWaterBytes: number;
}

export const PRESENCE_LIMITS: PresenceLimits = {
  framesPerSecond: 30,
  framesBurst: 60,
  userFramesPerSecond: 120,
  userFramesBurst: 240,
  tickMs: 33,
  staleMs: 30_000,
  lowWaterBytes: 64 * 1024,
};

/** Соединение глазами присутствия: что нужно знать о нём хабу. */
export interface PresenceConn {
  id: number;
  file: string;
  user: string;
  ws: { getBufferedAmount(): number; sendBinary(b: Uint8Array): number } | null;
  closed: boolean;
}

class Bucket {
  private tokens: number;
  private last: number;
  constructor(
    private readonly rate: number,
    private readonly burst: number,
    now: number,
  ) {
    this.tokens = burst;
    this.last = now;
  }
  take(now: number): boolean {
    this.tokens = Math.min(this.burst, this.tokens + ((now - this.last) / 1000) * this.rate);
    this.last = now;
    if (this.tokens < 1) return false;
    this.tokens -= 1;
    return true;
  }
  full(now: number): boolean {
    return Math.min(this.burst, this.tokens + ((now - this.last) / 1000) * this.rate) >= this.burst;
  }
}

/** Участник: последнее состояние и части выделения — уже закодированные записи пачки. */
interface Participant {
  conn: PresenceConn;
  join: Uint8Array;
  state: Uint8Array | null;
  rev: number;
  parts: (Uint8Array | null)[];
  last: number;
  /** Виден другим: прислал состояние и не молчал дольше `staleMs`. */
  shown: boolean;
  frames: Bucket;
}

/** Что получатель ещё не получил о каждом участнике. */
interface Pending {
  join: boolean;
  state: boolean;
  parts: Set<number>;
  left: boolean;
}

interface Recipient {
  conn: PresenceConn;
  pending: Map<number, Pending>;
}

interface Room {
  participants: Map<number, Participant>;
  recipients: Map<number, Recipient>;
}

export class PresenceHub {
  private readonly rooms = new Map<string, Room>();
  private readonly users = new Map<string, Bucket>();
  private readonly timer: Timer;
  private readonly limits: PresenceLimits;
  /** Сколько пачек ушло и сколько тиков получатель пропустил из-за буфера — для тестов и пробы. */
  sent = 0;
  deferred = 0;
  dropped = 0;

  constructor(
    limits: Partial<PresenceLimits> = {},
    private readonly now: () => number = () => performance.now(),
  ) {
    this.limits = { ...PRESENCE_LIMITS, ...limits };
    this.timer = setInterval(() => {
      try {
        this.tick();
      } catch (e) {
        console.error('Присутствие:', e instanceof Error ? e.message : e);
      }
    }, this.limits.tickMs);
    this.timer.unref?.();
  }

  private room(file: string): Room {
    let r = this.rooms.get(file);
    if (!r) this.rooms.set(file, (r = { participants: new Map(), recipients: new Map() }));
    return r;
  }

  /** Роль соединения подтверждена: оно получает присутствие файла, начиная со всех, кто уже виден. */
  admit(conn: PresenceConn): void {
    if (conn.closed) return;
    const room = this.room(conn.file);
    if (room.recipients.has(conn.id)) return;
    const r: Recipient = { conn, pending: new Map() };
    room.recipients.set(conn.id, r);
    for (const p of room.participants.values()) {
      if (p.shown && p.conn !== conn) r.pending.set(p.conn.id, everything(p));
    }
  }

  /** Состояние участника. `name` — имя человека с сервера, не из кадра. Неверное и сверх пределов — молча мимо. */
  state(conn: PresenceConn, name: string, body: Uint8Array, state: PresenceState | null | undefined): void {
    const room = this.rooms.get(conn.file);
    if (!state || !room?.recipients.has(conn.id) || !this.allow(conn)) {
      this.dropped++;
      return;
    }
    const now = this.now();
    let p = room.participants.get(conn.id);
    if (!p) {
      p = {
        conn,
        join: encodePresenceRecord({
          k: 'join',
          participant: conn.id,
          user: conn.user,
          name: clip(name),
          color: presenceColor(conn.user),
        }),
        state: null,
        rev: -1,
        parts: [],
        last: now,
        shown: false,
        frames: new Bucket(this.limits.framesPerSecond, this.limits.framesBurst, now),
      };
      room.participants.set(conn.id, p);
    }
    p.last = now;
    p.state = encodePresenceRecord({ k: 'state', participant: conn.id, body: body.slice() });
    const sel = state.selection;
    if (!sel) {
      p.rev = -1;
      p.parts = [];
    } else if (sel.rev !== p.rev || sel.parts !== p.parts.length) {
      p.rev = sel.rev;
      p.parts = Array.from({ length: sel.parts }, () => null);
    }
    const appearing = !p.shown;
    p.shown = true;
    for (const r of room.recipients.values()) {
      if (r.conn === conn) continue;
      const q = pendingOf(r, conn.id);
      q.left = false;
      q.state = true;
      if (appearing) Object.assign(q, everything(p));
    }
  }

  /** Часть выделения. Часть другой версии начинает её заново: кадры одного сокета идут по порядку, старых частей после новой
   * версии не бывает. */
  part(conn: PresenceConn, body: Uint8Array, part: PresencePart | null | undefined): void {
    const room = this.rooms.get(conn.file);
    const p = room?.participants.get(conn.id);
    if (!part || !room || !p || !this.allow(conn)) {
      this.dropped++;
      return;
    }
    if (part.rev !== p.rev || part.parts !== p.parts.length) {
      p.rev = part.rev;
      p.parts = Array.from({ length: part.parts }, () => null);
    }
    p.parts[part.index] = encodePresenceRecord({ k: 'part', participant: conn.id, body: body.slice() });
    p.last = this.now();
    if (!p.shown) return;
    for (const r of room.recipients.values()) {
      if (r.conn !== conn) pendingOf(r, conn.id).parts.add(part.index);
    }
  }

  /** Пределы частоты: соединения и человека по всем соединениям. Сначала проверяются оба, токены берутся у обоих. */
  private allow(conn: PresenceConn): boolean {
    const now = this.now();
    const p = this.rooms.get(conn.file)?.participants.get(conn.id);
    let u = this.users.get(conn.user);
    if (!u) this.users.set(conn.user, (u = new Bucket(this.limits.userFramesPerSecond, this.limits.userFramesBurst, now)));
    if (p && !p.frames.take(now)) return false;
    return u.take(now);
  }

  /** Соединение закрыто: участник пропадает у всех сразу, получатель уходит. */
  leave(conn: PresenceConn): void {
    const room = this.rooms.get(conn.file);
    if (!room) return;
    room.recipients.delete(conn.id);
    const p = room.participants.get(conn.id);
    room.participants.delete(conn.id);
    if (p?.shown) this.hide(room, conn.id);
    if (room.recipients.size === 0 && room.participants.size === 0) this.rooms.delete(conn.file);
  }

  private hide(room: Room, id: number): void {
    for (const r of room.recipients.values()) this.markLeft(r, id);
  }

  private markLeft(r: Recipient, id: number): void {
    r.pending.set(id, { join: false, state: false, parts: new Set(), left: true });
  }

  /** Пачки получателям, у которых сокет не занят; молчащие дольше `staleMs` участники пропадают. */
  tick(): void {
    const now = this.now();
    for (const [file, room] of this.rooms) {
      for (const p of room.participants.values()) {
        if (p.shown && now - p.last > this.limits.staleMs) {
          p.shown = false;
          this.hide(room, p.conn.id);
        }
      }
      for (const r of room.recipients.values()) {
        if (r.pending.size === 0 || r.conn.closed || !r.conn.ws) continue;
        if (r.conn.ws.getBufferedAmount() > this.limits.lowWaterBytes) {
          this.deferred++;
          continue;
        }
        const records = this.collect(room, r);
        if (records.length === 0) continue;
        r.conn.ws.sendBinary(encodePresenceBatch(records));
        this.sent++;
      }
      if (room.recipients.size === 0 && room.participants.size === 0) this.rooms.delete(file);
    }
    for (const [user, b] of this.users) if (b.full(now)) this.users.delete(user);
  }

  /** Записи для получателя до предела пачки; что не влезло, остаётся отмеченным. */
  private collect(room: Room, r: Recipient): Uint8Array[] {
    const out: Uint8Array[] = [];
    let size = 0;
    const fits = (rec: Uint8Array) => {
      if (size + rec.length > PRESENCE_BATCH_MAX) return false;
      out.push(rec);
      size += rec.length;
      return true;
    };
    for (const [id, q] of r.pending) {
      if (q.left) {
        if (!fits(encodePresenceRecord({ k: 'left', participant: id }))) return out;
        r.pending.delete(id);
        continue;
      }
      const p = room.participants.get(id);
      if (!p?.shown) {
        r.pending.delete(id);
        continue;
      }
      if (q.join) {
        if (!fits(p.join)) return out;
        q.join = false;
      }
      if (q.state && p.state) {
        if (!fits(p.state)) return out;
        q.state = false;
      }
      for (const i of q.parts) {
        const rec = p.parts[i];
        if (rec && !fits(rec)) return out;
        q.parts.delete(i);
      }
      r.pending.delete(id);
    }
    return out;
  }

  /** Сколько байт держит присутствие: состояния, части и отметки. Для тестов памяти. */
  heldBytes(): number {
    let n = 0;
    for (const room of this.rooms.values()) {
      for (const p of room.participants.values()) {
        n += p.join.length + (p.state?.length ?? 0);
        for (const part of p.parts) n += part?.length ?? 0;
      }
      for (const r of room.recipients.values()) for (const q of r.pending.values()) n += 32 + q.parts.size * 8;
    }
    return n;
  }

  /** Сколько участников видно в файле. */
  shown(file: string): number {
    let n = 0;
    for (const p of this.rooms.get(file)?.participants.values() ?? []) if (p.shown) n++;
    return n;
  }

  stop(): void {
    clearInterval(this.timer);
  }
}

function pendingOf(r: Recipient, id: number): Pending {
  let q = r.pending.get(id);
  if (!q) r.pending.set(id, (q = { join: false, state: false, parts: new Set(), left: false }));
  return q;
}

function everything(p: Participant): Pending {
  return {
    join: true,
    state: p.state !== null,
    parts: new Set(p.parts.flatMap((part, i) => (part ? [i] : []))),
    left: false,
  };
}

/** Имя в кадре: без управляющих символов, не длиннее предела в байтах (по границе символа). */
export function clip(name: string): string {
  let s = [...name]
    .map((ch) => {
      const c = ch.codePointAt(0)!;
      return c < 0x20 || (c >= 0x7f && c <= 0x9f) || c === 0x2028 || c === 0x2029 ? ' ' : ch;
    })
    .join('')
    .trim();
  const enc = new TextEncoder();
  while (enc.encode(s).length > PRESENCE_NAME_MAX) s = [...s].slice(0, -1).join('');
  return s;
}
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `test:server` — протокол 42 pass, сервер 505 pass, 0 fail

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/src/sync/hub.ts apps/server/src/sync/presence.ts apps/server/test/presence-load-probe.ts apps/server/test/presence-load.test.ts apps/server/test/presence.test.ts
git commit -F - <<'EOF'
сервер: присутствие — подпись участником, пачки по тику без очередей, уход сразу

Кто, имя и цвет — с сервера; присутствие файла — только после роли из hello; закрытие соединения — left у всех. Пределы:
2 КБ, 30 кадров в секунду на соединение и 120 на человека, лишнее молча отбрасывается. Каждому получателю — пачка раз в
33 мс из последних состояний, при полном буфере сокета тик пропускается. 30 с тишины — участник пропадает.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3: Сцена — слой живого превью других участников

Спека §9. Слой чужого превью лежит под своим превью и в документ не идёт.

**Files:**
- Modify: `crates/ferrum-scene/src/scene.rs`
- Test: `crates/ferrum-scene/tests/scene.rs`

**Interfaces:**
- Produces: `Scene::{set_remote_preview, has_remote_preview, remote_patches}`, `doc_props`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-scene/tests/scene.rs` заменить:

````rust
    assert!(scene.text_revision() > r2, "удаление текста меняет ревизию");
}
````

на:

````rust
    assert!(scene.text_revision() > r2, "удаление текста меняет ревизию");
}

fn x(x: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        ..Default::default()
    }
}

#[test]
fn remote_preview_moves_scene_and_never_the_document() {
    let (mut doc, page, [f, r, ..]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let before = snapshot(&scene);
    scene.set_remote_preview(&[(r, x(300.0))]);
    assert!(scene.has_remote_preview());
    assert!(
        !scene.has_preview(),
        "свой жест чужое превью не видит своим"
    );
    assert_eq!(scene.node(r).unwrap().props.x, 300.0);
    assert_eq!(doc.page(page).unwrap().props(r).unwrap().x, 10.0);
    // Правка документа под чужим превью: превью остаётся поверх нового документа.
    doc.transact(page, vec![], |p| {
        p.set_props(
            r,
            &PropsPatch {
                y: Some(50.0),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    let n = scene.node(r).unwrap();
    assert_eq!((n.props.x, n.props.y), (300.0, 50.0));
    // Новый список заменяет прежний; пустой возвращает документ.
    scene.set_remote_preview(&[(f, x(0.0))]);
    assert_eq!(scene.node(r).unwrap().props.x, 10.0);
    scene.set_remote_preview(&[]);
    assert!(!scene.has_remote_preview());
    assert_matches_rebuild(&scene, &mut doc, page);
    assert_ne!(snapshot(&scene), before);
}

#[test]
fn own_preview_wins_over_remote_and_each_returns_to_the_other() {
    let (mut doc, page, [_, r, ..]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    scene.set_remote_preview(&[(r, x(300.0))]);
    // Свой жест над тем же узлом: видно своё.
    scene.set_preview(&[(r, x(50.0))]);
    assert_eq!(scene.node(r).unwrap().props.x, 50.0);
    // Свой снят — снова чужое, а не документ и не смесь.
    scene.clear_preview();
    assert_eq!(scene.node(r).unwrap().props.x, 300.0);
    // Чужое снято под своим — своё остаётся, документ — под ним.
    scene.set_preview(&[(r, x(70.0))]);
    scene.set_remote_preview(&[]);
    assert_eq!(scene.node(r).unwrap().props.x, 70.0);
    scene.clear_preview();
    assert_eq!(scene.node(r).unwrap().props.x, 10.0);
    assert_matches_rebuild(&scene, &mut doc, page);
    // Своё превью поверх чужого берёт за основу документ, а не чужое превью.
    scene.set_remote_preview(&[(r, x(300.0))]);
    scene.set_preview(&[(
        r,
        PropsPatch {
            y: Some(5.0),
            ..Default::default()
        },
    )]);
    assert_eq!(scene.node(r).unwrap().props.x, 10.0);
    scene.clear_preview();
    scene.set_remote_preview(&[]);
    assert_matches_rebuild(&scene, &mut doc, page);
}

#[test]
fn remote_preview_survives_rebuild_and_drops_with_deleted_node() {
    let (mut doc, page, [_, r, g, _, _, l]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    scene.set_remote_preview(&[(r, x(300.0)), (l, x(5.0))]);
    testing::forget(&mut scene, g);
    doc.transact(page, vec![], |p| p.move_node(l, Some(g), 0).map(|_| vec![]))
        .unwrap();
    apply(&mut scene, &mut doc, page);
    assert_eq!(
        scene.node(r).unwrap().props.x,
        300.0,
        "превью пережило перестройку"
    );
    doc.transact(page, vec![], |p| p.delete_nodes(&[r]).map(|_| vec![]))
        .unwrap();
    apply(&mut scene, &mut doc, page);
    assert!(scene.node(r).is_none());
    assert_eq!(
        scene.remote_patches().len(),
        1,
        "превью удалённого узла ушло"
    );
    scene.set_remote_preview(&[]);
    assert_matches_rebuild(&scene, &mut doc, page);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-scene --test scene`
Expected: FAIL — не собирается: у `Scene` нет `set_remote_preview` и `has_remote_preview`, 12 ошибок компиляции

- [ ] **Step 3: Реализация**

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
    /// Узлы под превью жеста: свойства из документа и временная правка поверх них.
    preview: HashMap<Slot, (Props, PropsPatch)>,
    /// Изменения R-tree, накопленные за один пересчёт (см. `flush_tree`).
    tree_removals: Vec<(Slot, Rect)>,
````

на:

````rust
    /// Узлы под превью жеста: свойства из документа и временная правка поверх них.
    preview: HashMap<Slot, (Props, PropsPatch)>,
    /// Живое превью жестов других участников (спека подсистемы 4, раздел 9): так же
    /// свойства из документа и правка поверх них. Своё превью у узла главнее: узел
    /// под своим жестом чужую правку не показывает. В документ не попадает никогда.
    remote: HashMap<Slot, (Props, PropsPatch)>,
    /// Изменения R-tree, накопленные за один пересчёт (см. `flush_tree`).
    tree_removals: Vec<(Slot, Rect)>,
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
                if p.inputs.len() == inputs.len()
                    && inputs.len() > 1
                    && (!self.preview.is_empty() || p.rest.as_ref().is_some_and(|r| r.0 == *i)) =>
            {
                // Пустые формы в операции не участвуют — тогда без частей.
````

на:

````rust
                if p.inputs.len() == inputs.len()
                    && inputs.len() > 1
                    && (!self.preview.is_empty()
                        || !self.remote.is_empty()
                        || p.rest.as_ref().is_some_and(|r| r.0 == *i)) =>
            {
                // Пустые формы в операции не участвуют — тогда без частей.
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
                .map(|(&s, (_, patch))| (self.at(s).id, patch.clone()))
                .collect();
            let outlines = self.text_outlines.take();
            *self = Scene::from_page(page);
````

на:

````rust
                .map(|(&s, (_, patch))| (self.at(s).id, patch.clone()))
                .collect();
            let remote = self.remote_patches();
            let outlines = self.text_outlines.take();
            *self = Scene::from_page(page);
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
                self.set_text_outlines(outlines);
            }
            self.set_preview(&preview);
        }
````

на:

````rust
                self.set_text_outlines(outlines);
            }
            self.set_remote_preview(&remote);
            self.set_preview(&preview);
        }
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
    }

    /// Свойства из документа; превью жеста, если оно есть, накладывается поверх.
    fn set_doc_props(&mut self, slot: Slot, mut props: Props) {
        if let Some((doc, patch)) = self.preview.get_mut(&slot) {
            *doc = props.clone();
            patch.apply_to(&mut props);
        }
        shape::sanitize(&mut props);
````

на:

````rust
    }

    /// Свойства из документа; превью жеста, если оно есть, накладывается поверх: своё,
    /// иначе чужое.
    fn set_doc_props(&mut self, slot: Slot, mut props: Props) {
        let own = match self.preview.get_mut(&slot) {
            Some((doc, patch)) => {
                *doc = props.clone();
                Some(patch.clone())
            }
            None => None,
        };
        let remote = match self.remote.get_mut(&slot) {
            Some((doc, patch)) => {
                *doc = props.clone();
                Some(patch)
            }
            None => None,
        };
        match (own, remote) {
            (Some(patch), _) => patch.apply_to(&mut props),
            (None, Some(patch)) => patch.apply_to(&mut props),
            (None, None) => {}
        }
        shape::sanitize(&mut props);
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
            self.by_id.remove(&n.id);
            self.preview.remove(&s);
            self.free.push(s);
        }
````

на:

````rust
            self.by_id.remove(&n.id);
            self.preview.remove(&s);
            self.remote.remove(&s);
            self.free.push(s);
        }
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
            let doc = match self.preview.remove(&slot) {
                Some((doc, _)) => doc,
                None => self.at(slot).props.clone(),
            };
            self.preview.insert(slot, (doc.clone(), patch.clone()));
````

на:

````rust
            let doc = match self.preview.remove(&slot) {
                Some((doc, _)) => doc,
                None => self.doc_props(slot),
            };
            self.preview.insert(slot, (doc.clone(), patch.clone()));
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
        }
        self.recompute(dirty, Vec::new());
        self.flush_tree();
    }

    /// Убирает превью и черновики: все узлы возвращаются к свойствам документа.
````

на:

````rust
        }
        self.recompute(dirty, Vec::new());
        self.flush_tree();
    }

    /// Свойства узла из документа, без превью: своё и чужое лежат поверх них.
    fn doc_props(&self, slot: Slot) -> Props {
        match (self.preview.get(&slot), self.remote.get(&slot)) {
            (Some((doc, _)), _) | (None, Some((doc, _))) => doc.clone(),
            (None, None) => self.at(slot).props.clone(),
        }
    }

    /// Живое превью жестов других участников целиком (узлы, которых нет в списке,
    /// возвращаются к документу). Как `set_preview`, но слоем под своим превью: узел
    /// под своим жестом остаётся своим. Меняет только сцену, не документ. Правки имени
    /// и текста сюда не приходят: чужой жест двигает, тянет и поворачивает.
    pub fn set_remote_preview(&mut self, patches: &[(NodeId, PropsPatch)]) {
        let mut dirty = Vec::new();
        let new: HashSet<Slot> = patches
            .iter()
            .filter_map(|(id, _)| self.slot(*id))
            .collect();
        let stale: Vec<Slot> = self
            .remote
            .keys()
            .copied()
            .filter(|s| !new.contains(s))
            .collect();
        for s in stale {
            let (doc, _) = self.remote.remove(&s).expect("есть в чужом превью");
            self.set_doc_props(s, doc);
            dirty.push(s);
        }
        for (id, patch) in patches {
            let Some(slot) = self.slot(*id) else { continue };
            let doc = match self.remote.remove(&slot) {
                Some((doc, _)) => doc,
                None => self.doc_props(slot),
            };
            self.remote.insert(slot, (doc.clone(), patch.clone()));
            self.set_doc_props(slot, doc);
            dirty.push(slot);
        }
        if !dirty.is_empty() {
            self.changes += 1;
        }
        self.recompute(dirty, Vec::new());
        self.flush_tree();
    }

    /// Есть ли живое превью других участников.
    pub fn has_remote_preview(&self) -> bool {
        !self.remote.is_empty()
    }

    /// Чужое превью как список правок: пересборка сцены его сохраняет.
    pub fn remote_patches(&self) -> Vec<(NodeId, PropsPatch)> {
        self.remote
            .iter()
            .map(|(&s, (_, patch))| (self.at(s).id, patch.clone()))
            .collect()
    }

    /// Убирает превью и черновики: все узлы возвращаются к свойствам документа.
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-scene`

Expected: PASS: fmt и clippy без замечаний; `ferrum-scene` 78 passed

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-scene/src/scene.rs crates/ferrum-scene/tests/scene.rs
git commit -F - <<'EOF'
сцена: слой живого превью других участников под своим превью

Scene::set_remote_preview: свойства документа и правка поверх, своё превью у узла главнее, перестройка сцены слой сохраняет,
документ не меняется.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4: Рендер — курсоры и рамки других участников

Спека ядра §8.6; «Решения» 14. Курсор с подписью — готовая картинка, рамки выделения цветом участника; секция `render_report` с целью 1 мс.

**Files:**
- Modify: `crates/ferrum-editor/src/editor.rs`, `crates/ferrum-render/examples/render_report.rs`, `crates/ferrum-render/src/lib.rs`, `crates/ferrum-render/src/overlay.rs`
- Test: `crates/ferrum-render/tests/overlay.rs`

**Interfaces:**
- Produces: `RemoteMark`, `Overlay.remote: Arc<[RemoteMark]>`, `REMOTE_BOXES_MAX = 64`, `REMOTE_LABEL_SIZE`, `REMOTE_LABEL_MAX_WIDTH`, `CursorSprites`, `Renderer::cursor_sprites`.

- [ ] **Step 1: Написать тесты**

В `crates/ferrum-render/tests/overlay.rs` заменить:

````rust
    assert!(img.data().iter().all(|c| c.r == 0x1e));
}
````

на:

````rust
    assert!(img.data().iter().all(|c| c.r == 0x1e));
}

mod remote {
    use super::common::*;
    use ferrum_render::{
        Camera, CpuBackend, ImagePaint, NoImages, Overlay, RemoteMark, RenderBackend, Renderer,
        Rgba, REMOTE_BOXES_MAX,
    };
    use ferrum_scene::Scene;
    use kurbo::{Affine, BezPath, Point, Rect, Stroke};
    use std::sync::Arc;
    use vello_cpu::peniko::Fill;

    const BLUE: [u8; 3] = [0x0d, 0x74, 0xce];

    fn mark(cursor: Option<Point>, name: &str) -> RemoteMark {
        RemoteMark {
            color: Rgba::from_rgba8(BLUE[0], BLUE[1], BLUE[2], 255),
            name: name.into(),
            cursor,
            boxes: Vec::new(),
            frame: None,
        }
    }

    fn overlay(marks: Vec<RemoteMark>) -> Overlay {
        Overlay {
            frame_labels: false,
            remote: Arc::from(marks),
            ..Default::default()
        }
    }

    fn frame(r: &mut Renderer, b: &mut impl RenderBackend, cam: &Camera, o: &Overlay) {
        r.render(b, &Scene::new(), cam, o, &NoImages, Some(WHITE));
    }

    /// Бэкенд без картинок: курсор рисуется глифами и обводкой напрямую.
    struct NoImagesBackend(CpuBackend);

    impl RenderBackend for NoImagesBackend {
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

    #[test]
    fn cursor_and_label_are_drawn_at_world_point_in_peer_colour() {
        let mut cam = camera(200.0, 100.0);
        // Мир сдвинут: курсор в мире (1000, 500) — на экране (40, 30).
        cam.set_center(Point::new(1060.0, 520.0));
        let img = render_overlay(
            &Scene::new(),
            &cam,
            &overlay(vec![mark(Some(Point::new(1000.0, 500.0)), "Анна")]),
            WHITE,
        );
        // Стрелка у острия — цвет участника; левее острия — фон.
        assert_px(&img, 42, 38, BLUE, 40);
        assert_px(&img, 35, 38, [255, 255, 255], 0);
        // Плашка имени справа снизу от острия: цвет участника вокруг букв.
        assert_px(&img, 52, 51, BLUE, 40);
    }

    #[test]
    fn hidden_and_off_screen_cursors_are_not_drawn() {
        let cam = camera(100.0, 100.0);
        for m in [
            mark(None, "Анна"),
            mark(Some(Point::new(5000.0, 50.0)), "Анна"),
            mark(Some(Point::new(50.0, 5000.0)), "Анна"),
            mark(Some(Point::new(-5000.0, 50.0)), "Анна"),
        ] {
            let mut r = Renderer::new();
            let mut cpu = CpuBackend::new(100, 100);
            frame(&mut r, &mut cpu, &cam, &overlay(vec![m]));
            let img = cpu.render();
            for (x, y) in [(5, 5), (50, 50), (95, 95)] {
                assert_px(&img, x, y, [255, 255, 255], 0);
            }
            // Курсор за краем экрана и картинки не строит.
            assert_eq!(r.cursor_sprites(), 0);
        }
    }

    #[test]
    fn sprite_looks_like_direct_drawing_and_is_built_once() {
        let cam = camera(160.0, 80.0);
        let o = overlay(vec![mark(Some(Point::new(20.0, 10.0)), "Вероника")]);
        let mut r = Renderer::new();
        let mut cpu = CpuBackend::new(160, 80);
        frame(&mut r, &mut cpu, &cam, &o);
        let sprite = cpu.render();
        assert_eq!(r.cursor_sprites(), 1);
        frame(&mut r, &mut cpu, &cam, &o);
        assert_eq!(r.cursor_sprites(), 1, "тот же курсор — та же картинка");
        let mut direct = NoImagesBackend(CpuBackend::new(160, 80));
        let mut r2 = Renderer::new();
        frame(&mut r2, &mut direct, &cam, &o);
        assert_eq!(r2.cursor_sprites(), 0);
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
            "картинка курсора отличается от прямого рисования на {worst}"
        );
    }

    #[test]
    fn sprites_are_built_a_few_per_frame() {
        let cam = camera(400.0, 300.0);
        let marks: Vec<_> = (0..30)
            .map(|i| {
                mark(
                    Some(Point::new(10.0 + i as f64 * 12.0, 20.0)),
                    &format!("Участник {i}"),
                )
            })
            .collect();
        let o = overlay(marks);
        let mut r = Renderer::new();
        let mut cpu = CpuBackend::new(400, 300);
        frame(&mut r, &mut cpu, &cam, &o);
        assert_eq!(r.cursor_sprites(), 8, "не больше восьми картинок за кадр");
        for _ in 0..3 {
            frame(&mut r, &mut cpu, &cam, &o);
        }
        assert_eq!(r.cursor_sprites(), 30);
    }

    #[test]
    fn selection_boxes_and_frame_in_peer_colour() {
        let cam = camera(200.0, 120.0);
        let mut m = mark(None, "Анна");
        m.boxes = vec![
            (
                Affine::translate((20.0, 20.0)),
                Rect::new(0.0, 0.0, 40.0, 30.0),
            ),
            (
                Affine::translate((150.0, 60.0)) * Affine::rotate(0.5),
                Rect::new(0.0, 0.0, 30.0, 20.0),
            ),
        ];
        m.frame = Some(Rect::new(10.0, 10.0, 190.0, 110.0));
        let img = render_overlay(&Scene::new(), &cam, &overlay(vec![m]), WHITE);
        // Рамка узла: левая сторона — x = 20, внутри — фон.
        assert_px(&img, 20, 35, BLUE, 30);
        assert_px(&img, 35, 35, [255, 255, 255], 0);
        // Общая рамка.
        assert_px(&img, 100, 10, BLUE, 30);
        assert_px(&img, 190, 60, BLUE, 30);
    }

    #[test]
    fn boxes_beyond_the_limit_are_not_drawn() {
        let cam = camera(400.0, 40.0);
        let mut m = mark(None, "Анна");
        m.boxes = (0..REMOTE_BOXES_MAX + 1)
            .map(|i| {
                (
                    Affine::translate((i as f64 * 5.0, 10.0)),
                    Rect::new(0.0, 0.0, 3.0, 10.0),
                )
            })
            .collect();
        let img = render_overlay(&Scene::new(), &cam, &overlay(vec![m]), WHITE);
        assert_px(&img, ((REMOTE_BOXES_MAX - 1) * 5) as u16, 15, BLUE, 30);
        assert_px(&img, (REMOTE_BOXES_MAX * 5) as u16, 15, [255, 255, 255], 0);
    }
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-render --test overlay`
Expected: FAIL — не собирается: нет `RemoteMark`, `REMOTE_BOXES_MAX`, поля `Overlay::remote` и `Renderer::cursor_sprites`, 8 ошибок компиляции

- [ ] **Step 3: Реализация**

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            guides: self.guide_marks(),
            active_guide: self.active_guide(),
        }
    }
````

на:

````rust
            guides: self.guide_marks(),
            active_guide: self.active_guide(),
            remote: std::sync::Arc::from(Vec::new()),
        }
    }
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{
    Camera, CpuBackend, GuideMark, NoImages, Overlay, Renderer, RulersOverlay, TextEngine,
    LABEL_SIZE,
};
use ferrum_scene::Scene;
````

на:

````rust
use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{
    Camera, CpuBackend, GuideMark, NoImages, Overlay, RemoteMark, Renderer, Rgba, RulersOverlay,
    TextEngine, LABEL_SIZE,
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

/// Присутствие (спека подсистемы 4, разделы 9 и 13.1): 50 чужих курсоров с подписями и
/// 50 рамок чужого выделения на экране 1920×1080, у каждого участника — по 4 узла
/// (рамка каждого и общая), один повёрнут. Цель — прибавка к кадру не больше 1 мс. Курсор
/// движется каждый кадр, как при интерполяции в ядре.
fn presence_section(report: &mut Report, scene: &Scene, base: &Camera) {
    const RUNS: usize = 61;
    let names = [
        "Анна Смирнова",
        "Борис",
        "Вероника Кузнецова-Петрова",
        "Глеб",
        "Дарья",
    ];
    let palette: [u32; 10] = [
        0xe5484d, 0xf76b15, 0x2d8a3e, 0x0d74ce, 0x8e4ec6, 0xd6409f, 0x12a594, 0x5b5bd6, 0xab6400,
        0x3e63dd,
    ];
    let marks = |t: f64| -> Vec<RemoteMark> {
        let view = base.visible_world_rect();
        (0..50)
            .map(|i| {
                let fx = (i % 10) as f64 / 10.0 + 0.03;
                let fy = (i / 10) as f64 / 5.0 + 0.05;
                let at = kurbo::Point::new(
                    view.x0 + view.width() * fx + t * 3.0,
                    view.y0 + view.height() * fy + t,
                );
                let boxes = (0..4)
                    .map(|k| {
                        let o = kurbo::Affine::translate((at.x + 30.0 * k as f64, at.y - 60.0));
                        let o = if k == 3 {
                            o * kurbo::Affine::rotate(0.3)
                        } else {
                            o
                        };
                        (o, kurbo::Rect::new(0.0, 0.0, 24.0, 18.0))
                    })
                    .collect::<Vec<_>>();
                let c = palette[i % palette.len()];
                RemoteMark {
                    color: Rgba::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, 255),
                    name: format!("{} {i}", names[i % names.len()]).into(),
                    cursor: Some(at),
                    frame: Some(kurbo::Rect::new(
                        at.x,
                        at.y - 60.0,
                        at.x + 120.0,
                        at.y - 30.0,
                    )),
                    boxes,
                }
            })
            .collect()
    };
    for dpr in [1.0, 2.0] {
        let mut camera = Camera::new(VIEWPORT, dpr);
        camera.set_zoom(base.zoom());
        camera.set_center(base.center());
        let (w, h) = camera.device_size();
        let mut backend = CpuBackend::new(w, h);
        let mut renderer = Renderer::new();
        let plain = Overlay::default();
        let mut with = Overlay {
            remote: marks(0.0).into(),
            ..Default::default()
        };
        renderer.render(&mut backend, scene, &camera, &plain, &NoImages, None);
        renderer.render(&mut backend, scene, &camera, &with, &NoImages, None);
        let (mut off, mut on) = (Vec::new(), Vec::new());
        for i in 0..RUNS {
            with.remote = marks(i as f64 * 0.37).into();
            for (overlay, runs) in [(&plain, &mut off), (&with, &mut on)] {
                backend.reset(w, h);
                let t = Instant::now();
                renderer.render(&mut backend, scene, &camera, overlay, &NoImages, None);
                runs.push(t.elapsed());
            }
        }
        report.gate(
            if dpr == 1.0 {
                "Кадр с 50 курсорами и 50 чужими выделениями: прибавка, 1920×1080"
            } else {
                "Кадр с 50 курсорами и 50 чужими выделениями: прибавка, 3840×2160 (2×)"
            },
            median(on).saturating_sub(median(off)),
            1.0,
            "≤ 1 мс",
        );
        // С растеризацией CPU — справочно: на GPU растеризация не на этом потоке.
        let empty = Scene::new();
        let (mut off, mut on) = (Vec::new(), Vec::new());
        for i in 0..RUNS {
            with.remote = marks(i as f64 * 0.37).into();
            for (overlay, runs) in [(&plain, &mut off), (&with, &mut on)] {
                backend.reset(w, h);
                let t = Instant::now();
                renderer.render(&mut backend, &empty, &camera, overlay, &NoImages, None);
                std::hint::black_box(backend.render());
                runs.push(t.elapsed());
            }
        }
        report.info(
            &format!("Кадр с 50 курсорами и 50 чужими выделениями: прибавка с растеризацией CPU, dpr {dpr}"),
            ms(median(on).saturating_sub(median(off))),
        );
    }
}

/// Экспорт SVG и PDF (подсистема 3, этап 5): SVG страницы из 10 000 узлов
/// (фреймы по 100: фрейм + 99 фигур) и PDF страницы с текстом и вектором в
````

В `crates/ferrum-render/examples/render_report.rs` заменить:

````rust
    );
    rulers_section(&mut report, &scene, &camera);

    // Вся страница при сильном отдалении: LOD и растровый кэш фреймов. Кэш
````

на:

````rust
    );
    rulers_section(&mut report, &scene, &camera);
    presence_section(&mut report, &scene, &camera);

    // Вся страница при сильном отдалении: LOD и растровый кэш фреймов. Кэш
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
pub use overlay::{
    format_size, ruler_steps, ActiveGuide, GuideMark, Measure, MeasureKind, Overlay, OverlayTheme,
    RulersOverlay, TextEditOverlay, VectorEditOverlay, HANDLE_SIZE, LABEL_SIZE, MEASURE_DASH,
    MEASURE_GAP, PIXEL_GRID_MIN_ZOOM, RULER_SIZE, TEXT_SELECTION_ALPHA, VECTOR_HANDLE_RADIUS,
    VECTOR_MAX_POINTS_DRAWN, VECTOR_POINT_RADIUS,
};
````

на:

````rust
pub use overlay::{
    format_size, ruler_steps, ActiveGuide, GuideMark, Measure, MeasureKind, Overlay, OverlayTheme,
    RemoteMark, RulersOverlay, TextEditOverlay, VectorEditOverlay, HANDLE_SIZE, LABEL_SIZE,
    MEASURE_DASH, MEASURE_GAP, PIXEL_GRID_MIN_ZOOM, REMOTE_BOXES_MAX, REMOTE_LABEL_MAX_WIDTH,
    REMOTE_LABEL_SIZE, RULER_SIZE, TEXT_SELECTION_ALPHA, VECTOR_HANDLE_RADIUS,
    VECTOR_MAX_POINTS_DRAWN, VECTOR_POINT_RADIUS,
};
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    painter: Painter,
    text: TextEngine,
    /// Реестр шрифтов и раскладка текстовых узлов — общие с редактором
    /// (`set_fonts`); свой рендерер без реестра знает только вшитый Onest.
````

на:

````rust
    painter: Painter,
    text: TextEngine,
    /// Картинки чужих курсоров (`overlay::CursorSprites`).
    sprites: overlay::CursorSprites,
    /// Реестр шрифтов и раскладка текстовых узлов — общие с редактором
    /// (`set_fonts`); свой рендерер без реестра знает только вшитый Onest.
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
            painter: Painter::with_raster_limit(bytes),
            text: TextEngine::new(),
            fonts: Fonts::shared(),
        }
````

на:

````rust
            painter: Painter::with_raster_limit(bytes),
            text: TextEngine::new(),
            sprites: Default::default(),
            fonts: Fonts::shared(),
        }
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
        }
        let stats = self.draw_scene(backend, scene, camera, images);
        overlay::draw(backend, &mut self.text, scene, camera, overlay);
        stats
    }

````

на:

````rust
        }
        let stats = self.draw_scene(backend, scene, camera, images);
        overlay::draw(
            backend,
            &mut self.text,
            &mut self.sprites,
            scene,
            camera,
            overlay,
        );
        stats
    }

    /// Сколько картинок чужих курсоров построено и хранится (для проверки кэша).
    pub fn cursor_sprites(&self) -> usize {
        self.sprites.len()
    }

````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
//! Оверлеи поверх содержимого в экранных пикселях: пиксельная сетка, названия
//! фреймов, направляющие, подсветка наведения, рамка выделения мышью, линии привязки,
//! рамка выделения с ручками, бейдж «W × H», измерения расстояний с Alt и линейки.
//! Толщины и размеры задаются в CSS-пикселях и умножаются на `devicePixelRatio`.

use ferrum_doc::{NodeId, NodeKind};
````

на:

````rust
//! Оверлеи поверх содержимого в экранных пикселях: пиксельная сетка, названия
//! фреймов, направляющие, подсветка наведения, рамка выделения мышью, линии привязки,
//! рамка выделения с ручками, бейдж «W × H», измерения расстояний с Alt, линейки и
//! участники совместной работы: их курсоры с подписью и рамки выделения.
//! Толщины и размеры задаются в CSS-пикселях и умножаются на `devicePixelRatio`.

use std::sync::Arc;

use ferrum_doc::{NodeId, NodeKind};
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
use vello_cpu::peniko::Fill;

use crate::backend::{RenderBackend, Rgba};
use crate::camera::Camera;
use crate::text::{Label, TextEngine};
````

на:

````rust
use vello_cpu::peniko::Fill;

use std::collections::HashMap;

use vello_cpu::peniko::Extend;
use vello_cpu::Pixmap;

use crate::backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::camera::Camera;
use crate::text::{Label, TextEngine};
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
    /// Направляющая, которую сейчас тянут.
    pub active_guide: Option<ActiveGuide>,
}

impl Default for Overlay {
````

на:

````rust
    /// Направляющая, которую сейчас тянут.
    pub active_guide: Option<ActiveGuide>,
    /// Другие участники файла на этой странице (спека подсистемы 4, раздел 9).
    pub remote: Arc<[RemoteMark]>,
}

/// Другой участник на холсте: его курсор с подписью и рамка его выделения, цветом
/// участника. Всё — в мировых координатах; кэширует и пересчитывает редактор.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteMark {
    pub color: Rgba,
    pub name: Arc<str>,
    /// Курсор; `None` — указатель вне холста.
    pub cursor: Option<Point>,
    /// Рамки выделенных узлов: матрица узла в мир и его границы. Не больше
    /// `REMOTE_BOXES_MAX`: остальные видны общей рамкой.
    pub boxes: Vec<(Affine, Rect)>,
    /// Общая рамка выделения в мире: у нескольких узлов и у выделения, узлы
    /// которого не пришли или которых здесь нет.
    pub frame: Option<Rect>,
}

/// Столько рамок узлов чужого выделения рисуется; дальше — только общая рамка.
pub const REMOTE_BOXES_MAX: usize = 64;
/// Подпись чужого курсора: кегль, наибольшая ширина имени — CSS-пиксели.
pub const REMOTE_LABEL_SIZE: f64 = 11.0;
pub const REMOTE_LABEL_MAX_WIDTH: f64 = 160.0;

impl Default for Overlay {
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
            guides: Vec::new(),
            active_guide: None,
        }
    }
````

на:

````rust
            guides: Vec::new(),
            active_guide: None,
            remote: Arc::from(Vec::new()),
        }
    }
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
pub(crate) fn draw(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    scene: &Scene,
    camera: &Camera,
    overlay: &Overlay,
````

на:

````rust
pub(crate) fn draw(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    sprites: &mut CursorSprites,
    scene: &Scene,
    camera: &Camera,
    overlay: &Overlay,
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
        vector_edit(backend, view, dpr, theme, v);
    }
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
    if let Some(r) = &overlay.rulers {
        rulers(backend, text, camera, theme, r);
````

на:

````rust
        vector_edit(backend, view, dpr, theme, v);
    }
    remote_frames(backend, view, dpr, &overlay.remote);
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
    remote_cursors(backend, text, sprites, view, dpr, &overlay.remote);
    if let Some(r) = &overlay.rulers {
        rulers(backend, text, camera, theme, r);
````

В `crates/ferrum-render/src/overlay.rs` заменить:

````rust
}

/// Высота плашки с подписью `label` (поля 2 px сверху и снизу).
fn badge_height(label: &Label, dpr: f64) -> f64 {
````

на:

````rust
}

/// Рамки чужих выделений: у каждого узла — его повёрнутый прямоугольник, у
/// нескольких — ещё общая рамка, как у своего выделения, но без ручек и бейджа.
/// Всё, что целиком за краем экрана, пропускается. Стороны рамок по осям —
/// заливкой прямоугольников (обводка у 50 участников стоила треть цели §13.1),
/// повёрнутые — обводкой.
fn remote_frames(backend: &mut impl RenderBackend, view: Affine, dpr: f64, marks: &[RemoteMark]) {
    if marks.is_empty() {
        return;
    }
    let (w, h) = backend.size();
    let screen = Rect::new(0.0, 0.0, w as f64, h as f64).inflate(2.0 * dpr, 2.0 * dpr);
    let line = Stroke::new(dpr);
    backend.set_transform(Affine::IDENTITY);
    for m in marks {
        let mut edges: Vec<Rect> = Vec::new();
        let mut slanted = BezPath::new();
        let mut ring = |r: Rect| {
            let r = Rect::from_points(
                crisp_point(r.origin(), dpr),
                crisp_point(Point::new(r.x1, r.y1), dpr),
            );
            if !overlaps(r, screen) {
                return;
            }
            let half = dpr / 2.0;
            // Четыре стороны: верх и низ во всю ширину, бока между ними.
            edges.push(Rect::new(
                r.x0 - half,
                r.y0 - half,
                r.x1 + half,
                r.y0 + half,
            ));
            edges.push(Rect::new(
                r.x0 - half,
                r.y1 - half,
                r.x1 + half,
                r.y1 + half,
            ));
            if r.y1 - r.y0 > dpr {
                edges.push(Rect::new(
                    r.x0 - half,
                    r.y0 + half,
                    r.x0 + half,
                    r.y1 - half,
                ));
                edges.push(Rect::new(
                    r.x1 - half,
                    r.y0 + half,
                    r.x1 + half,
                    r.y1 - half,
                ));
            }
        };
        for (to_world, b) in m.boxes.iter().take(REMOTE_BOXES_MAX) {
            let to_device = view * *to_world;
            let c = to_device.as_coeffs();
            if c[1] == 0.0 && c[2] == 0.0 {
                ring(to_device.transform_rect_bbox(*b));
                continue;
            }
            let quad = [
                Point::new(b.x0, b.y0),
                Point::new(b.x1, b.y0),
                Point::new(b.x1, b.y1),
                Point::new(b.x0, b.y1),
            ]
            .map(|p| to_device * p);
            let bbox = quad
                .iter()
                .fold(Rect::from_points(quad[0], quad[0]), |r, p| r.union_pt(*p));
            if !overlaps(bbox, screen) {
                continue;
            }
            slanted.move_to(quad[0]);
            for p in &quad[1..] {
                slanted.line_to(*p);
            }
            slanted.close_path();
        }
        if let Some(f) = m.frame {
            ring(view.transform_rect_bbox(f));
        }
        if edges.is_empty() && slanted.elements().is_empty() {
            continue;
        }
        backend.set_color(m.color);
        for e in &edges {
            backend.fill_rect(e);
        }
        if !slanted.elements().is_empty() {
            backend.stroke_path(&slanted, &line);
        }
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && a.x1 >= b.x0 && a.y0 <= b.y1 && a.y1 >= b.y0
}

/// Стрелка курсора в CSS-пикселях: остриё в начале координат.
fn cursor_arrow() -> BezPath {
    let mut p = BezPath::new();
    p.move_to((0.0, 0.0));
    p.line_to((0.0, 16.0));
    p.line_to((4.2, 12.2));
    p.line_to((7.0, 18.4));
    p.line_to((9.6, 17.2));
    p.line_to((6.9, 11.2));
    p.line_to((12.4, 11.2));
    p.close_path();
    p
}

/// Текст на плашке цвета участника: тёмный на светлом, иначе белый.
fn ink_on(c: Rgba) -> Rgba {
    let [r, g, b, _] = c.components;
    let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if l > 0.6 {
        Rgba::from_rgba8(0x1a, 0x19, 0x18, 255)
    } else {
        Rgba::from_rgba8(255, 255, 255, 255)
    }
}

/// Готовая картинка чужого курсора: стрелка с белой обводкой и плашка с именем. Курсор
/// рисуется одной заливкой прямоугольника этой картинкой: глифы подписи и обводка
/// стрелки каждый кадр у 50 участников стоили бы больше всей цели §13.1.
#[derive(Clone)]
struct Sprite {
    key: ImageKey,
    pixmap: Arc<Pixmap>,
    /// Где в картинке остриё стрелки, пиксели устройства.
    tip: Point,
    used: u64,
}

/// Картинки курсоров по имени, цвету и плотности пикселей. Строится не больше
/// `SPRITE_BUILDS_PER_FRAME` за кадр: остальные курсоры этого кадра рисуются
/// напрямую, а следующие кадры — картинкой.
#[derive(Default)]
pub(crate) struct CursorSprites {
    map: HashMap<(Arc<str>, [u8; 4], u32), Sprite>,
    frame: u64,
}

/// Столько картинок курсоров строится за кадр (около 0,1 мс каждая).
const SPRITE_BUILDS_PER_FRAME: usize = 8;
/// Столько картинок курсоров хранится: участников с курсором — не больше 50 на
/// страницу, запас на смену имён и тем.
const SPRITE_LIMIT: usize = 256;

impl CursorSprites {
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    fn get(&mut self, key: &(Arc<str>, [u8; 4], u32)) -> Option<Sprite> {
        let frame = self.frame;
        self.map.get_mut(key).map(|s| {
            s.used = frame;
            s.clone()
        })
    }

    fn insert(&mut self, key: (Arc<str>, [u8; 4], u32), sprite: Sprite) {
        if self.map.len() >= SPRITE_LIMIT {
            // Давно не нужные уходят первыми; в кадре их не больше 50.
            let frame = self.frame;
            self.map.retain(|_, s| s.used + 1 >= frame);
        }
        self.map.insert(key, sprite);
    }
}

/// Стрелка курсора и плашка с именем в своей системе координат: остриё в `tip`.
fn draw_cursor(
    backend: &mut impl RenderBackend,
    label: Option<&Label>,
    tip: Point,
    dpr: f64,
    color: Rgba,
) {
    let arrow = Affine::scale(dpr) * cursor_arrow();
    backend.set_transform(Affine::translate(tip.to_vec2()));
    backend.set_color(Rgba::from_rgba8(255, 255, 255, 255));
    backend.stroke_path(&arrow, &Stroke::new(1.5 * dpr));
    backend.set_color(color);
    backend.fill_path(&arrow, Fill::NonZero);
    let Some(label) = label else { return };
    let pill = label_pill(label, tip, dpr);
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(color);
    backend.fill_path(
        &RoundedRect::from_rect(pill, 4.0 * dpr).to_path(0.1),
        Fill::NonZero,
    );
    label.draw(
        backend,
        Point::new(pill.x0 + 6.0 * dpr, pill.y0 + 3.0 * dpr + label.ascent),
        ink_on(color),
    );
}

/// Плашка имени справа снизу от острия.
fn label_pill(label: &Label, tip: Point, dpr: f64) -> Rect {
    let (pad_x, pad_y) = (6.0 * dpr, 3.0 * dpr);
    let x0 = tip.x + 10.0 * dpr;
    let y0 = tip.y + 18.0 * dpr;
    Rect::new(
        x0,
        y0,
        x0 + label.width + 2.0 * pad_x,
        y0 + label.ascent + label.descent + 2.0 * pad_y,
    )
}

/// Картинка курсора: стрелка и плашка на прозрачном фоне с полем в 2 px под обводку.
fn build_sprite(label: Option<&Label>, dpr: f64, color: Rgba) -> Sprite {
    let margin = 2.0 * dpr;
    let tip = Point::new(margin.ceil(), margin.ceil());
    let mut bounds = Rect::new(0.0, 0.0, tip.x + 14.0 * dpr, tip.y + 20.0 * dpr);
    if let Some(l) = label {
        bounds = bounds.union(label_pill(l, tip, dpr));
    }
    let (w, h) = (
        (bounds.x1 + margin).ceil() as u16,
        (bounds.y1 + margin).ceil() as u16,
    );
    let mut cpu = CpuBackend::new(w, h);
    draw_cursor(&mut cpu, label, tip, dpr, color);
    Sprite {
        key: ImageKey::next_raster(),
        pixmap: Arc::new(cpu.render()),
        tip,
        used: 0,
    }
}

/// Чужие курсоры: стрелка цвета участника с белой обводкой и плашка с именем справа
/// снизу от острия. Курсор за краем экрана (вместе с подписью) не рисуется.
fn remote_cursors(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    sprites: &mut CursorSprites,
    view: Affine,
    dpr: f64,
    marks: &[RemoteMark],
) {
    if marks.iter().all(|m| m.cursor.is_none()) {
        return;
    }
    sprites.frame += 1;
    let mut builds = 0;
    let images = backend.supports_images();
    let (w, h) = backend.size();
    let screen = Rect::new(0.0, 0.0, w as f64, h as f64);
    let reach = (REMOTE_LABEL_MAX_WIDTH + 40.0) * dpr;
    for m in marks {
        let Some(at) = m.cursor else { continue };
        let tip = view * at;
        if !tip.x.is_finite()
            || !tip.y.is_finite()
            || tip.x < screen.x0 - reach
            || tip.y < screen.y0 - reach
            || tip.x > screen.x1
            || tip.y > screen.y1
        {
            continue;
        }
        // Картинка ложится пиксель в пиксель: остриё — на целом пикселе.
        let tip = Point::new(tip.x.round(), tip.y.round());
        let key = (
            m.name.clone(),
            m.color.to_rgba8().to_u8_array(),
            (dpr * 64.0).round() as u32,
        );
        let mut sprite = if images { sprites.get(&key) } else { None };
        if sprite.is_none() && images && builds < SPRITE_BUILDS_PER_FRAME {
            builds += 1;
            let label = text.label(
                &m.name,
                REMOTE_LABEL_SIZE * dpr,
                500,
                REMOTE_LABEL_MAX_WIDTH * dpr,
            );
            let s = build_sprite(label.as_deref(), dpr, m.color);
            sprites.insert(key, s.clone());
            sprite = Some(s);
        }
        match sprite {
            Some(s) => {
                let origin = tip - s.tip.to_vec2();
                let (sw, sh) = (s.pixmap.width() as f64, s.pixmap.height() as f64);
                backend.set_transform(Affine::IDENTITY);
                backend.set_image(&ImagePaint {
                    key: s.key,
                    pixmap: s.pixmap.clone(),
                    transform: Affine::translate(origin.to_vec2()),
                    extend: Extend::Pad,
                    alpha: 1.0,
                });
                backend.fill_rect(&Rect::new(origin.x, origin.y, origin.x + sw, origin.y + sh));
            }
            None => {
                let label = text.label(
                    &m.name,
                    REMOTE_LABEL_SIZE * dpr,
                    500,
                    REMOTE_LABEL_MAX_WIDTH * dpr,
                );
                draw_cursor(backend, label.as_deref(), tip, dpr, m.color);
            }
        }
    }
}

/// Высота плашки с подписью `label` (поля 2 px сверху и снизу).
fn badge_height(label: &Label, dpr: f64) -> f64 {
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-render -p ferrum-editor`

Expected: PASS: fmt и clippy без замечаний; `ferrum-render` и `ferrum-editor` 738 passed

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor/src/editor.rs crates/ferrum-render/examples/render_report.rs crates/ferrum-render/src/lib.rs crates/ferrum-render/src/overlay.rs crates/ferrum-render/tests/overlay.rs
git commit -F - <<'EOF'
рендер: курсоры с подписью и рамки выделения других участников

Курсор — готовая картинка (стрелка и плашка имени), не больше 8 новых за кадр; стороны рамок по осям — заливкой. Кадр с 50
курсорами и 50 чужими выделениями: прибавка 0,13 мс при цели 1 мс (render_report).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 5: Редактор — участники, живое превью, следование

Спека §9; «Решения» 8–13. Участники и плавное движение курсора, уход через 30 с, рамки выделения, превью чужого жеста теми же функциями, что у своего, следование; слой снимается перед любой своей правкой документа (`run`, `begin_doc_edit`, `debug_assert` в `Ctx::commit`).

**Files:**
- Create: `crates/ferrum-editor/src/presence.rs`
- Modify: `apps/web/src/core/layout.gen.ts`, `crates/ferrum-editor/examples/editor_report.rs`, `crates/ferrum-editor/src/actions.rs`, `crates/ferrum-editor/src/editor.rs`, `crates/ferrum-editor/src/input.rs`, `crates/ferrum-editor/src/lib.rs`, `crates/ferrum-editor/src/text_edit.rs`, `crates/ferrum-editor/src/tools/mod.rs`, `crates/ferrum-editor/src/tools/select.rs`, `crates/ferrum-wasm/src/core.rs`, `crates/ferrum-wasm/src/input.rs`, `crates/ferrum-wasm/src/layout.rs`
- Test: `crates/ferrum-editor/tests/presence.rs` (новый)

**Interfaces:**
- Consumes: слой сцены задачи 3, `RemoteMark` задачи 4.
- Produces: `Editor::{presence_join, presence_state, presence_part, presence_left, presence_reset, peers, follow, following, presence_hold, presence_release, own_presence}`, `PeerState`, `PeerSelection`, `Gesture`, `GestureKind`, `OwnPresence`, `PeerInfo`, `PEERS_DRAWN = 50`, `PEER_STALE_MS = 30 000`; `Input::PointerLeave` (`POINTER_LEAVE = 11`); `EditorEvent::Following`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/presence.rs`:

````rust
//! Другие участники (спека подсистемы 4, раздел 9): курсоры с плавным движением, рамки
//! выделения, живое превью жестов (только сцена, никогда документ), уход, следование.

mod common;

use common::*;
use ferrum_doc::NodeId;
use ferrum_editor::*;
use kurbo::{Affine, Point, Rect};

const BLUE: u32 = 0x0d74ce;

fn state(ed: &Editor, cursor: Option<(f64, f64)>) -> PeerState {
    PeerState {
        page: Some(ed.page()),
        view: Rect::new(0.0, 0.0, 1000.0, 800.0),
        cursor: cursor.map(Point::from),
        selection: None,
        gesture: None,
    }
}

fn select(ed: &Editor, ids: &[NodeId], rev: u32) -> PeerState {
    PeerState {
        selection: Some(PeerSelection {
            rev,
            count: ids.len() as u32,
            parts: 1,
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
        }),
        ..state(ed, None)
    }
}

fn moving(ed: &Editor, ids: &[NodeId], dx: f64) -> PeerState {
    PeerState {
        gesture: Some(Gesture {
            kind: GestureKind::Move,
            transform: Affine::translate((dx, 0.0)),
            frame: Affine::IDENTITY,
            k: 1.0,
        }),
        ..select(ed, ids, 1)
    }
}

fn join(h: &mut H, id: u32) {
    h.ed.presence_join(id, &format!("u-{id}"), "Анна", peer_color(BLUE));
}

/// Участник выделил `ids` (одной частью) и тянет их на `dx`.
fn drag_by(h: &mut H, id: u32, ids: &[NodeId], dx: f64) {
    h.ed.presence_part(id, 1, 0, 1, ids.to_vec());
    let s = moving(&h.ed, ids, dx);
    h.ed.presence_state(id, s);
}

fn cursor_of(h: &H, i: usize) -> Option<Point> {
    h.ed.overlay().remote.get(i).and_then(|m| m.cursor)
}

#[test]
fn peer_cursor_is_drawn_with_name_and_colour_only_on_its_page() {
    let mut h = H::new();
    join(&mut h, 3);
    // Без состояния участника на холсте нет.
    assert!(h.ed.overlay().remote.is_empty());
    let s = state(&h.ed, Some((120.0, 40.0)));
    h.ed.presence_state(3, s);
    let o = h.ed.overlay();
    assert_eq!(o.remote.len(), 1);
    assert_eq!(&*o.remote[0].name, "Анна");
    assert_eq!(o.remote[0].color, peer_color(BLUE));
    assert_eq!(o.remote[0].cursor, Some(Point::new(120.0, 40.0)));
    // Другая страница или неизвестная — курсора не видно, участник в списке есть.
    let other = h.ed.add_page("Вторая").unwrap();
    let mut s = state(&h.ed, Some((1.0, 1.0)));
    s.page = Some(other);
    h.ed.presence_state(3, s.clone());
    assert!(h.ed.overlay().remote.is_empty());
    s.page = None;
    h.ed.presence_state(3, s);
    assert!(h.ed.overlay().remote.is_empty());
    assert_eq!(h.ed.peers().len(), 1);
    // Без `join` состояние не принимается: имя и цвет знает только сервер.
    let s = state(&h.ed, Some((5.0, 5.0)));
    h.ed.presence_state(9, s);
    assert_eq!(h.ed.peers().len(), 1);
}

#[test]
fn cursor_moves_smoothly_between_peer_frames() {
    let mut h = H::new();
    h.ed.set_time(0.0);
    join(&mut h, 1);
    let s = state(&h.ed, Some((0.0, 0.0)));
    h.ed.presence_state(1, s);
    h.ed.set_time(50.0);
    h.ed.take_events();
    let s = state(&h.ed, Some((100.0, 0.0)));
    h.ed.presence_state(1, s);
    assert_eq!(
        cursor_of(&h, 0),
        Some(Point::new(0.0, 0.0)),
        "движение начинается с места"
    );
    h.ed.set_time(75.0);
    let mid = cursor_of(&h, 0).unwrap();
    assert!((mid.x - 50.0).abs() < 1e-9, "середина пути: {mid:?}");
    assert!(
        h.ed.take_events().contains(&EditorEvent::NeedsRedraw),
        "движение просит кадры"
    );
    h.ed.set_time(100.0);
    assert_eq!(cursor_of(&h, 0), Some(Point::new(100.0, 0.0)));
    h.ed.set_time(120.0);
    h.ed.take_events();
    h.ed.set_time(140.0);
    assert!(
        !h.ed.take_events().contains(&EditorEvent::NeedsRedraw),
        "стоящий курсор кадров не просит"
    );
    // Указатель ушёл с холста — курсора нет.
    let s = state(&h.ed, None);
    h.ed.presence_state(1, s);
    assert_eq!(h.ed.overlay().remote[0].cursor, None);
}

#[test]
fn silent_peer_vanishes_after_30_seconds_and_left_at_once() {
    let mut h = H::new();
    h.ed.set_time(1000.0);
    join(&mut h, 1);
    join(&mut h, 2);
    let s = state(&h.ed, Some((1.0, 1.0)));
    h.ed.presence_state(1, s.clone());
    h.ed.presence_state(2, s);
    h.ed.set_time(1000.0 + PEER_STALE_MS - 1.0);
    assert_eq!(h.ed.peers().len(), 2);
    let s = state(&h.ed, Some((2.0, 2.0)));
    h.ed.presence_state(2, s);
    h.ed.set_time(1000.0 + PEER_STALE_MS + 1.0);
    assert_eq!(
        h.ed.peers().iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![2]
    );
    h.ed.presence_left(2);
    assert!(h.ed.peers().is_empty());
    assert!(h.ed.overlay().remote.is_empty());
}

#[test]
fn reconnect_reset_drops_everyone() {
    let mut h = H::new();
    join(&mut h, 1);
    let s = state(&h.ed, Some((1.0, 1.0)));
    h.ed.presence_state(1, s);
    h.ed.presence_reset();
    assert!(h.ed.peers().is_empty());
}

#[test]
fn selection_frame_follows_nodes_and_waits_for_all_parts() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    let b = h.rect(None, 200.0, 100.0, 30.0, 30.0);
    join(&mut h, 1);
    let mut s = select(&h.ed, &[a, b], 4);
    s.selection.as_mut().unwrap().parts = 2;
    s.selection.as_mut().unwrap().bounds = Rect::new(1.0, 2.0, 3.0, 4.0);
    h.ed.presence_state(1, s);
    // Части не пришли: рамка из кадра.
    let m = h.ed.overlay().remote[0].clone();
    assert!(m.boxes.is_empty());
    assert_eq!(m.frame, Some(Rect::new(1.0, 2.0, 3.0, 4.0)));
    h.ed.presence_part(1, 4, 0, 2, vec![a]);
    assert!(h.ed.overlay().remote[0].boxes.is_empty());
    h.ed.presence_part(1, 4, 1, 2, vec![b]);
    let m = h.ed.overlay().remote[0].clone();
    assert_eq!(m.boxes.len(), 2);
    assert_eq!(m.frame, Some(Rect::new(10.0, 10.0, 230.0, 130.0)));
    // Узел сдвинулся — рамка за ним.
    h.set(b, patch(300.0, 100.0, 30.0, 30.0));
    assert_eq!(
        h.ed.overlay().remote[0].frame,
        Some(Rect::new(10.0, 10.0, 330.0, 130.0))
    );
    // Новая версия: прежние части не годятся.
    let s = select(&h.ed, &[a], 5);
    h.ed.presence_state(1, s);
    assert!(h.ed.overlay().remote[0].boxes.is_empty());
    h.ed.presence_part(1, 5, 0, 1, vec![a]);
    let m = h.ed.overlay().remote[0].clone();
    assert_eq!(m.boxes.len(), 1);
    assert_eq!(m.frame, None, "у одного узла рамка — его собственная");
}

#[test]
fn live_preview_moves_scene_never_document_and_ends_with_gesture_or_peer() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    let b = h.rect(None, 200.0, 100.0, 30.0, 30.0);
    let before = h.state();
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a, b], 100.0);
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 110.0);
    assert_eq!(h.ed.scene().node(b).unwrap().props.x, 300.0);
    assert_eq!(h.state(), before, "документ не тронут");
    // Рамка участника — по сдвинутым узлам.
    assert_eq!(h.ed.overlay().remote[0].frame.unwrap().x0, 110.0);
    // Жест кончился (или отменён): превью снято.
    let s = select(&h.ed, &[a, b], 1);
    h.ed.presence_state(1, s);
    assert!(!h.ed.scene().has_remote_preview());
    h.assert_scene_clean();
    // Ушёл посреди жеста — превью снято сразу.
    drag_by(&mut h, 1, &[a, b], 50.0);
    assert!(h.ed.scene().has_remote_preview());
    h.ed.presence_left(1);
    assert!(!h.ed.scene().has_remote_preview());
    h.assert_scene_clean();
    assert_eq!(h.state(), before);
}

#[test]
fn own_drag_of_a_node_under_peer_preview_writes_only_own_move() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a], 100.0);
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 110.0);
    // Нажатие там, где узел виден (под чужим превью), и перетаскивание на 20: во время своего
    // жеста чужого превью нет, записывается только свой сдвиг от документа.
    h.down(130.0, 30.0, none());
    assert!(
        !h.ed.scene().has_remote_preview(),
        "свой жест — чужое превью снято"
    );
    assert_eq!(h.sel(), vec![a]);
    for x in [135.0, 140.0, 150.0] {
        h.mv(x, 30.0, none());
    }
    assert_eq!(
        h.ed.scene().node(a).unwrap().props.x,
        30.0,
        "превью жеста — от документа"
    );
    h.up(150.0, 30.0, none());
    assert_eq!(h.props(a).x, 30.0, "в документе — только свой сдвиг");
    // После своего жеста чужое превью возвращается — сдвиг участника от нового документа.
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 130.0);
    // Стрелка клавиатуры и поле панели — тоже без чужого сдвига.
    h.ed.select(&[a]);
    h.key("ArrowRight", none());
    assert_eq!(h.props(a).x, 31.0);
    h.ed.transact(|tx| {
        tx.set_props(
            a,
            &ferrum_doc::PropsPatch {
                w: Some(70.0),
                ..Default::default()
            },
        )?;
        Ok(vec![a])
    })
    .unwrap();
    assert_eq!(h.props(a).x, 31.0);
    assert_eq!(h.props(a).w, 70.0);
}

#[test]
fn hold_removes_peer_preview_until_release() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a], 100.0);
    h.ed.presence_hold();
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 10.0);
    // Кадр участника во время своего действия слой не возвращает.
    drag_by(&mut h, 1, &[a], 120.0);
    assert!(!h.ed.scene().has_remote_preview());
    h.ed.presence_release();
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 130.0);
}

#[test]
fn rotate_and_resize_previews_use_own_gesture_maths() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 100.0, 50.0);
    join(&mut h, 1);
    h.ed.presence_part(1, 1, 0, 1, vec![a]);
    let mut s = select(&h.ed, &[a], 1);
    s.gesture = Some(Gesture {
        kind: GestureKind::Resize,
        transform: Affine::scale_non_uniform(2.0, 1.0),
        frame: Affine::IDENTITY,
        k: 1.0,
    });
    h.ed.presence_state(1, s.clone());
    assert_eq!(h.ed.scene().node(a).unwrap().props.w, 200.0);
    s.gesture = Some(Gesture {
        kind: GestureKind::Rotate,
        transform: Affine::rotate_about(-std::f64::consts::FRAC_PI_2, Point::new(50.0, 25.0)),
        frame: Affine::IDENTITY,
        k: 1.0,
    });
    h.ed.presence_state(1, s);
    assert!((h.ed.scene().node(a).unwrap().props.rotation - 90.0).abs() < 1e-6);
    assert_eq!(h.props(a).rotation, 0.0);
}

#[test]
fn follow_copies_page_and_view_and_ends_on_own_action_or_departure() {
    let mut h = H::new();
    let second = h.ed.add_page("Вторая").unwrap();
    let first = h.ed.page();
    join(&mut h, 1);
    let mut s = state(&h.ed, Some((0.0, 0.0)));
    s.view = Rect::new(1000.0, 1000.0, 1500.0, 1400.0);
    s.page = Some(second);
    h.ed.presence_state(1, s.clone());
    h.ed.take_events();
    h.ed.follow(Some(1));
    assert_eq!(h.ed.following(), Some(1));
    assert!(h
        .ed
        .take_events()
        .contains(&EditorEvent::Following(Some(1))));
    assert_eq!(h.ed.page(), second, "страница участника");
    let v = h.ed.camera().visible_world_rect();
    assert!(v.contains(Point::new(1001.0, 1001.0)) && v.contains(Point::new(1499.0, 1399.0)));
    assert!((v.center() - Point::new(1250.0, 1200.0)).hypot() < 1e-6);
    // Участник ушёл на другую страницу — за ним.
    s.page = Some(first);
    h.ed.presence_state(1, s.clone());
    assert_eq!(h.ed.page(), first);
    assert_eq!(
        h.ed.following(),
        Some(1),
        "смена страницы следованием — не своё действие"
    );
    // Движение указателя — не действие; нажатие — выход.
    h.hover(10.0, 10.0);
    assert_eq!(h.ed.following(), Some(1));
    h.down(10.0, 10.0, none());
    assert_eq!(h.ed.following(), None);
    assert!(h.ed.take_events().contains(&EditorEvent::Following(None)));
    h.up(10.0, 10.0, none());
    // Колесо и своя смена страницы — тоже выход; уход участника — конец.
    h.ed.follow(Some(1));
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(1.0, 1.0),
        delta: kurbo::Vec2::new(0.0, 10.0),
        modifiers: none(),
    }));
    assert_eq!(h.ed.following(), None);
    h.ed.follow(Some(1));
    h.ed.set_page(second).unwrap();
    assert_eq!(h.ed.following(), None);
    h.ed.follow(Some(1));
    h.ed.take_events();
    h.ed.presence_left(1);
    assert_eq!(h.ed.following(), None);
    assert!(h.ed.take_events().contains(&EditorEvent::Following(None)));
    // За неизвестным не следуют.
    h.ed.follow(Some(42));
    assert_eq!(h.ed.following(), None);
}

#[test]
fn own_presence_reports_cursor_selection_and_gesture() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    assert_eq!(h.ed.own_presence().cursor, None, "указателя ещё не было");
    h.hover(300.0, 200.0);
    let own = h.ed.own_presence();
    assert_eq!(own.cursor, Some(Point::new(300.0, 200.0)));
    assert_eq!(own.page, h.ed.page());
    assert_eq!(own.view, h.ed.camera().visible_world_rect());
    h.ev(Input::PointerLeave);
    assert_eq!(h.ed.own_presence().cursor, None);
    let v = h.ed.own_presence().selection_version;
    h.drag_hold((30.0, 30.0), (70.0, 30.0), none());
    let own = h.ed.own_presence();
    assert_ne!(own.selection_version, v);
    let g = own.gesture.expect("перенос виден другим");
    assert_eq!(g.kind, GestureKind::Move);
    assert_eq!(g.transform.translation().x, 40.0);
    h.up(70.0, 30.0, none());
    assert_eq!(h.ed.own_presence().gesture, None);
    assert_eq!(h.props(a).x, 50.0);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test presence`
Expected: FAIL — не собирается: нет `PeerState`, `Input::PointerLeave` и методов присутствия редактора, 78 ошибок компиляции

- [ ] **Step 3: Реализация**

В `apps/web/src/core/layout.gen.ts` заменить:

````ts
  KEY_UP: 8,
  BLUR: 9,
  RESIZE: 10,
} as const;
````

на:

````ts
  KEY_UP: 8,
  BLUR: 9,
  POINTER_LEAVE: 11,
  RESIZE: 10,
} as const;
````

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
}

fn main() {
    let mut report = Report(Vec::new());
````

на:

````rust
}

/// Чужое перетаскивание: участник выделил `ids` (одной частью кадра) и тянет их; каждый его
/// кадр — сдвиг. Замер — приём кадра целиком (`presence_state`: слой превью и сцена).
fn presence_preview(report: &mut Report, ed: &mut Editor, ids: &[NodeId]) {
    use ferrum_editor::{peer_color, Gesture, GestureKind, PeerSelection, PeerState};
    use kurbo::{Affine, Rect};
    let page = ed.page();
    let before = ed.page_doc().props(ids[500]).unwrap().x;
    ed.presence_join(7, "u-7", "Участник", peer_color(0x0d74ce));
    let head = PeerSelection {
        rev: 1,
        count: ids.len() as u32,
        parts: 1,
        bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
    };
    ed.presence_part(7, 1, 0, 1, ids.to_vec());
    let state = |k: f64, moving: bool| PeerState {
        page: Some(page),
        view: Rect::new(0.0, 0.0, 1920.0, 1080.0),
        cursor: Some(Point::new(k, k)),
        selection: Some(head),
        gesture: moving.then_some(Gesture {
            kind: GestureKind::Move,
            transform: Affine::translate((k, k * 0.5)),
            frame: Affine::IDENTITY,
            k: 1.0,
        }),
    };
    ed.presence_state(7, state(0.0, false));
    let runs: Vec<Duration> = (1..=MOVES)
        .map(|i| {
            let t = Instant::now();
            ed.presence_state(7, state(i as f64 * 3.0, true));
            t.elapsed()
        })
        .collect();
    let shown = ed.scene().node(ids[500]).unwrap().props.x;
    let doc = ed.page_doc().props(ids[500]).unwrap().x;
    report.gate_median(
        "Живое превью чужого перетаскивания 1000 узлов: кадр участника",
        &runs,
        4.0,
    );
    report.check(
        "Чужое превью: сцена сдвинута, документ — нет",
        shown == before + MOVES as f64 * 3.0 && doc == before,
        format!("сцена {shown}, документ {doc}"),
        "сцена сдвинута, документ прежний",
    );
    let t = Instant::now();
    ed.presence_state(7, state(0.0, false));
    report.info("Конец чужого жеста: превью снято", ms(t.elapsed()));
    ed.presence_left(7);
    ed.take_events();
}

fn main() {
    let mut report = Report(Vec::new());
````

В `crates/ferrum-editor/examples/editor_report.rs` заменить:

````rust
    ed.execute(CommandId::Undo);
    ed.take_events();

    // 2. Запись перетаскивания 1000 узлов одной транзакцией и её отмена.
````

на:

````rust
    ed.execute(CommandId::Undo);
    ed.take_events();

    // 1а. Живое превью чужого перетаскивания тех же 1000 узлов (спека подсистемы 4, раздел 9):
    // кадр участника 20 раз в секунду — слой превью сцены по выделению и матрице жеста, без
    // правки документа. Цель — как у своего движения.
    presence_preview(&mut report, &mut ed, &children);

    // 2. Запись перетаскивания 1000 узлов одной транзакцией и её отмена.
````

В `crates/ferrum-editor/src/actions.rs` заменить:

````rust
    /// жестам (удалить точку, символ): уведомление — только про слои.
    pub(crate) fn run(&mut self, cmd: CommandId) {
        let notice =
            undo_notice(cmd).filter(|_| self.text_edit.is_none() && self.vector_edit.is_none());
````

на:

````rust
    /// жестам (удалить точку, символ): уведомление — только про слои.
    pub(crate) fn run(&mut self, cmd: CommandId) {
        // Команда может править по свойствам из сцены: чужого превью в ней на это время нет,
        // `emit` в конце вернёт его поверх нового документа.
        self.drop_remote_preview();
        let notice =
            undo_notice(cmd).filter(|_| self.text_edit.is_none() && self.vector_edit.is_none());
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Сообщение об ошибке для пользователя.
    Error(String),
}

````

на:

````rust
    /// Сообщение об ошибке для пользователя.
    Error(String),
    /// Следование за участником началось (номер) или кончилось (`None`): действием на
    /// холсте, уходом участника, сменой страницы.
    Following(Option<u32>),
}

/// Клавиша-модификатор: её нажатие — не действие на холсте.
fn is_modifier(code: &str) -> bool {
    matches!(
        code,
        "ShiftLeft"
            | "ShiftRight"
            | "ControlLeft"
            | "ControlRight"
            | "AltLeft"
            | "AltRight"
            | "MetaLeft"
            | "MetaRight"
    )
}

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Тянущаяся направляющая: положение, длина и значение на плашке.
    active_guide: Option<ferrum_render::ActiveGuide>,
    /// Подсказки привязки тянущейся линии: меняются без сдвига самой линии.
    guide_snap: Vec<kurbo::Line>,
````

на:

````rust
    /// Тянущаяся направляющая: положение, длина и значение на плашке.
    active_guide: Option<ferrum_render::ActiveGuide>,
    /// Другие участники: их курсоры, рамки, плавное движение.
    presence: u64,
    /// Подсказки привязки тянущейся линии: меняются без сдвига самой линии.
    guide_snap: Vec<kurbo::Line>,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    theme: OverlayTheme,
    pub(crate) out: Outbox,
    last_pointer: Option<PointerEvent>,
    seen: Seen,
    /// Жест панели свойств (перетаскивание поля, палитра): свойства на начало.
````

на:

````rust
    theme: OverlayTheme,
    pub(crate) out: Outbox,
    pub(crate) last_pointer: Option<PointerEvent>,
    seen: Seen,
    /// Жест панели свойств (перетаскивание поля, палитра): свойства на начало.
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Протяжка направляющей из линейки или за неё саму.
    pub(crate) guide_drag: Option<crate::guides::GuideDrag>,
    /// Превью `SetGuides`: список владельца вместо его направляющих, пока жест идёт.
    pub(crate) guides_preview: Option<(ferrum_doc::GuideOwner, Vec<ferrum_doc::Guide>)>,
````

на:

````rust
    /// Протяжка направляющей из линейки или за неё саму.
    pub(crate) guide_drag: Option<crate::guides::GuideDrag>,
    /// Другие участники файла (спека подсистемы 4, раздел 9).
    pub(crate) presence: crate::presence::Presence,
    /// Указатель над холстом: курсор виден другим.
    pub(crate) pointer_inside: bool,
    /// Превью `SetGuides`: список владельца вместо его направляющих, пока жест идёт.
    pub(crate) guides_preview: Option<(ferrum_doc::GuideOwner, Vec<ferrum_doc::Guide>)>,
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                guides_revision: 0,
                active_guide: None,
                guide_snap: Vec::new(),
                measures: Vec::new(),
````

на:

````rust
                guides_revision: 0,
                active_guide: None,
                presence: 0,
                guide_snap: Vec::new(),
                measures: Vec::new(),
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            guides_preview: None,
            guide_hover: None,
        })
    }
````

на:

````rust
            guides_preview: None,
            guide_hover: None,
            presence: Default::default(),
            pointer_inside: false,
        })
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            guides: self.guide_marks(),
            active_guide: self.active_guide(),
            remote: std::sync::Arc::from(Vec::new()),
        }
    }
````

на:

````rust
            guides: self.guide_marks(),
            active_guide: self.active_guide(),
            remote: self.remote_marks(),
        }
    }
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
            self.cancel_panel();
        }
        match &input {
            Input::PointerDown(e) | Input::PointerMove(e) | Input::PointerUp(e) => {
````

на:

````rust
            self.cancel_panel();
        }
        // Действие на холсте кончает следование за участником (поведение, как в Figma):
        // нажатие, колесо, щипок, клавиша. Движение указателя — не действие.
        match &input {
            Input::PointerDown(_) | Input::Wheel(_) | Input::Pinch { .. } => self.stop_following(),
            Input::KeyDown(e) if !is_modifier(&e.code) => self.stop_following(),
            _ => {}
        }
        match &input {
            Input::PointerDown(_) | Input::PointerMove(_) | Input::PointerUp(_) => {
                self.pointer_inside = true
            }
            Input::PointerLeave => self.pointer_inside = false,
            _ => {}
        }
        match &input {
            Input::PointerDown(e) | Input::PointerMove(e) | Input::PointerUp(e) => {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                self.release_hold();
            }
        }
        self.emit();
````

на:

````rust
                self.release_hold();
            }
            Input::PointerLeave => {}
        }
        self.emit();
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.finish_pen_path();
        self.cancel_gesture();
    }

````

на:

````rust
        self.finish_pen_path();
        self.cancel_gesture();
        // Правка читает свойства из сцены: чужого превью в ней быть не должно.
        self.drop_remote_preview();
    }

````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        // их удалить или сдвинуть, поэтому он отменяется.
        self.cancel_panel();
        if self.is_busy() {
            if cmd == CommandId::Escape {
````

на:

````rust
        // их удалить или сдвинуть, поэтому он отменяется.
        self.cancel_panel();
        self.stop_following();
        if self.is_busy() {
            if cmd == CommandId::Escape {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.begin_doc_edit();
        let before = self.selection.to_vec();
        let result = self.doc.transact(self.page, before, f);
        let changed = sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
````

на:

````rust
        self.begin_doc_edit();
        let before = self.selection.to_vec();
        debug_assert!(
            !self.scene.has_remote_preview(),
            "чужое превью попало бы в документ"
        );
        let result = self.doc.transact(self.page, before, f);
        let changed = sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Переключает страницу: сцена строится заново, выделение снимается.
    pub fn set_page(&mut self, page: PageId) -> DocResult<()> {
        self.cancel_gesture();
        self.finish_text_edit();
````

на:

````rust
    /// Переключает страницу: сцена строится заново, выделение снимается.
    pub fn set_page(&mut self, page: PageId) -> DocResult<()> {
        // Своя смена страницы кончает следование; смена следованием — нет.
        self.stop_following();
        self.cancel_gesture();
        self.finish_text_edit();
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        self.resync_vector_edit();
        self.sync_text_outlines();
        self.update_measures();
        self.refresh_guides();
````

на:

````rust
        self.resync_vector_edit();
        self.sync_text_outlines();
        self.sync_remote_preview();
        self.update_measures();
        self.refresh_guides();
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
                guides_revision: self.guides.revision,
                active_guide: self.active_guide(),
                guide_snap: self
                    .guide_drag
````

на:

````rust
                guides_revision: self.guides.revision,
                active_guide: self.active_guide(),
                presence: self.presence_revision(),
                guide_snap: self
                    .guide_drag
````

В `crates/ferrum-editor/src/input.rs` заменить:

````rust
    /// Окно потеряло фокус: отпущенных клавиш и кнопок мы уже не увидим.
    Blur,
}
````

на:

````rust
    /// Окно потеряло фокус: отпущенных клавиш и кнопок мы уже не увидим.
    Blur,
    /// Указатель ушёл с холста: другие участники больше не видят его курсор.
    PointerLeave,
}
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod pages;
pub mod panel;
pub mod presets;
pub mod props_clip;
````

на:

````rust
pub mod pages;
pub mod panel;
pub mod presence;
pub mod presets;
pub mod props_clip;
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
    display_rotation, field_applies, panel_pose, DropTarget, NumField, PanelEdit, PanelPose, Phase,
};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use remote::{
````

на:

````rust
    display_rotation, field_applies, panel_pose, DropTarget, NumField, PanelEdit, PanelPose, Phase,
};
pub use presence::{
    fit_view, peer_color, Gesture, GestureKind, OwnPresence, PeerInfo, PeerSelection, PeerState,
    PEERS_DRAWN, PEER_STALE_MS,
};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use remote::{
````

Создать `crates/ferrum-editor/src/presence.rs`:

````rust
//! Другие участники файла (спека подсистемы 4, раздел 9): их курсоры с плавным движением,
//! рамки выделения, живое превью их жестов и следование за участником; своё присутствие для
//! них — `Editor::own_presence`.
//!
//! Данные из сети не доверены: числа проверил разбор кадра, здесь — смысл (страница, узлы,
//! сколько участников рисуется). Живое превью меняет только сцену — слой
//! `Scene::set_remote_preview` под своим превью — и никогда документ. Пока идёт свой жест,
//! правка текста или вектора и любая команда (`Editor::presence_hold`), слоя нет: иначе
//! базы своего жеста прочли бы чужое превью и записали бы его в документ.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use ferrum_doc::{NodeId, PageId, PropsPatch};
use ferrum_render::{Camera, RemoteMark, Rgba, REMOTE_BOXES_MAX};
use ferrum_scene::Scene;
use kurbo::{Affine, Point, Rect, Size};

use crate::editor::{Editor, EditorEvent};

/// Курсоры и рамки рисуются у стольких участников страницы, остальные — только в счётчике.
pub const PEERS_DRAWN: usize = 50;
/// Участник без кадров дольше этого пропадает (спека, раздел 9).
pub const PEER_STALE_MS: f64 = 30_000.0;
/// Плавное движение курсора: от прежнего положения к новому за промежуток между кадрами
/// участника (20 в секунду — 50 мс), не короче и не длиннее этих пределов.
pub const CURSOR_LERP_MIN_MS: f64 = 16.0;
pub const CURSOR_LERP_MAX_MS: f64 = 150.0;

/// Вид чужого жеста.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureKind {
    Move,
    Resize,
    Rotate,
    Scale,
}

/// Жест: матрица в мире, рамка выделения на начало (у ресайза и «Масштаба») и равномерный
/// коэффициент «Масштаба». Узлы жеста — выделение участника.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gesture {
    pub kind: GestureKind,
    pub transform: Affine,
    pub frame: Affine,
    pub k: f64,
}

/// Заголовок чужого выделения: узлы приходят частями, рамка — сразу.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeerSelection {
    pub rev: u32,
    pub count: u32,
    pub parts: u8,
    pub bounds: Rect,
}

/// Состояние участника из его последнего кадра. `page` — `None`, если такой страницы здесь
/// нет (удалена, ещё не пришла).
#[derive(Debug, Clone, PartialEq)]
pub struct PeerState {
    pub page: Option<PageId>,
    pub view: Rect,
    pub cursor: Option<Point>,
    pub selection: Option<PeerSelection>,
    pub gesture: Option<Gesture>,
}

#[derive(Debug, Clone, Copy)]
struct Lerp {
    from: Point,
    to: Point,
    start: f64,
    dur: f64,
}

impl Lerp {
    fn at(&self, now: f64) -> Point {
        let t = ((now - self.start) / self.dur).clamp(0.0, 1.0);
        self.from.lerp(self.to, t)
    }

    fn done(&self, now: f64) -> bool {
        now - self.start >= self.dur
    }
}

struct Peer {
    user: Arc<str>,
    name: Arc<str>,
    color: Rgba,
    state: Option<PeerState>,
    cursor: Option<Lerp>,
    /// Когда пришло прошлое положение курсора: длина плавного движения.
    cursor_at: f64,
    rev: u32,
    parts: Vec<Option<Vec<NodeId>>>,
    /// Выделение целиком, когда пришли все части его версии.
    ids: Option<Arc<[NodeId]>>,
    seen: f64,
}

impl Peer {
    fn on_page(&self, page: PageId) -> bool {
        self.state.as_ref().is_some_and(|s| s.page == Some(page))
    }

    /// Выделение целиком, если пришли все части нынешней версии.
    fn selection(&self) -> Option<&Arc<[NodeId]>> {
        let head = self.state.as_ref()?.selection?;
        (head.rev == self.rev).then_some(self.ids.as_ref()?)
    }
}

/// Участник для шапки и тестов.
#[derive(Debug, Clone, PartialEq)]
pub struct PeerInfo {
    pub id: u32,
    pub user: Arc<str>,
    pub name: Arc<str>,
    pub page: Option<PageId>,
}

type MarksCache = ((u64, u64, u64, PageId), Arc<[RemoteMark]>);

#[derive(Default)]
pub(crate) struct Presence {
    peers: BTreeMap<u32, Peer>,
    /// Растёт при всём, что видно на холсте: кадр нужен заново.
    pub(crate) revision: u64,
    /// Растёт, когда меняется живое превью: жест участника, его выделение, уход.
    preview_rev: u64,
    /// С чем посчитан слой чужого превью в сцене: версия превью, сцена, страница.
    applied: Option<(u64, u64, u64, PageId)>,
    /// Своё действие идёт (`Editor::presence_hold`): слоя нет.
    holds: u32,
    pub(crate) follow: Option<u32>,
    /// Страница, на которую следование переключает само: это не действие человека.
    following_switch: bool,
    marks: RefCell<Option<MarksCache>>,
    now: f64,
}

impl Presence {
    fn bump(&mut self, preview: bool) {
        self.revision += 1;
        if preview {
            self.preview_rev += 1;
        }
    }

    fn animating(&self) -> bool {
        let now = self.now;
        self.peers
            .values()
            .any(|p| p.cursor.is_some_and(|c| !c.done(now)))
    }
}

/// Цвет участника из `#rrggbb` палитры протокола.
pub fn peer_color(rgb: u32) -> Rgba {
    Rgba::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

impl Editor {
    /// Участник пришёл: его человек, имя и цвет — с сервера. Повторный приход того же
    /// номера (имя сменилось) состояние не теряет.
    pub fn presence_join(&mut self, id: u32, user: &str, name: &str, color: Rgba) {
        let now = self.presence.now;
        let p = self.presence.peers.entry(id).or_insert_with(|| Peer {
            user: Arc::from(user),
            name: Arc::from(name),
            color,
            state: None,
            cursor: None,
            cursor_at: now,
            rev: 0,
            parts: Vec::new(),
            ids: None,
            seen: now,
        });
        p.user = Arc::from(user);
        p.name = Arc::from(name);
        p.color = color;
        p.seen = now;
        self.presence.bump(false);
        self.emit();
    }

    /// Состояние участника. Участник без `join` пропускается: имя и цвет знает только сервер.
    pub fn presence_state(&mut self, id: u32, state: PeerState) {
        let now = self.presence.now;
        let Some(p) = self.presence.peers.get_mut(&id) else {
            return;
        };
        p.seen = now;
        // Курсор движется плавно от того места, где он виден сейчас.
        let same_page = p.state.as_ref().map(|s| s.page) == Some(state.page);
        p.cursor = match (state.cursor, p.cursor, same_page) {
            (Some(to), Some(c), true) => Some(Lerp {
                from: c.at(now),
                to,
                start: now,
                dur: (now - p.cursor_at).clamp(CURSOR_LERP_MIN_MS, CURSOR_LERP_MAX_MS),
            }),
            (Some(to), _, _) => Some(Lerp {
                from: to,
                to,
                start: now,
                dur: CURSOR_LERP_MIN_MS,
            }),
            (None, _, _) => None,
        };
        if state.cursor.is_some() {
            p.cursor_at = now;
        }
        // Новая версия выделения: прежние части не годятся.
        let head = state.selection;
        match head {
            Some(h) if h.rev != p.rev || h.parts as usize != p.parts.len() => {
                p.rev = h.rev;
                p.parts = vec![None; h.parts as usize];
                p.ids = None;
            }
            None => {
                p.parts.clear();
                p.ids = None;
            }
            _ => {}
        }
        let old = p.state.replace(state);
        let preview = {
            let new = p.state.as_ref().expect("только что поставлено");
            old.as_ref().map(|s| (s.gesture, s.selection, s.page))
                != Some((new.gesture, new.selection, new.page))
        };
        self.presence.bump(preview);
        if self.presence.follow == Some(id) {
            self.follow_peer();
        }
        self.emit();
    }

    /// Часть чужого выделения. Часть другой версии начинает её заново.
    pub fn presence_part(&mut self, id: u32, rev: u32, index: u8, parts: u8, ids: Vec<NodeId>) {
        let now = self.presence.now;
        let Some(p) = self.presence.peers.get_mut(&id) else {
            return;
        };
        if index >= parts {
            return;
        }
        p.seen = now;
        if rev != p.rev || parts as usize != p.parts.len() {
            p.rev = rev;
            p.parts = vec![None; parts as usize];
            p.ids = None;
        }
        p.parts[index as usize] = Some(ids);
        if p.parts.iter().all(Option::is_some) {
            let all: Vec<NodeId> = p.parts.iter().flatten().flatten().copied().collect();
            p.ids = Some(Arc::from(all));
        }
        self.presence.bump(true);
        self.emit();
    }

    /// Участник ушёл: его курсор, рамка и живое превью пропадают сразу; следование за ним
    /// кончается.
    pub fn presence_left(&mut self, id: u32) {
        if self.presence.peers.remove(&id).is_none() {
            return;
        }
        if self.presence.follow == Some(id) {
            self.set_follow(None);
        }
        self.presence.bump(true);
        self.emit();
    }

    /// Соединение заново: участники придут снова, прежние не остаются двойниками.
    pub fn presence_reset(&mut self) {
        if self.presence.peers.is_empty() {
            return;
        }
        self.presence.peers.clear();
        if self.presence.follow.is_some() {
            self.set_follow(None);
        }
        self.presence.bump(true);
        self.emit();
    }

    /// Время ядра: плавное движение курсоров и уход молчащих участников.
    pub(crate) fn presence_time(&mut self, now: f64) -> bool {
        let was = self.presence.animating();
        self.presence.now = now;
        let stale: Vec<u32> = self
            .presence
            .peers
            .iter()
            .filter(|(_, p)| now - p.seen > PEER_STALE_MS)
            .map(|(id, _)| *id)
            .collect();
        for id in &stale {
            self.presence.peers.remove(id);
            if self.presence.follow == Some(*id) {
                self.set_follow(None);
            }
        }
        if !stale.is_empty() {
            self.presence.bump(true);
            return true;
        }
        if was {
            self.presence.revision += 1;
            return true;
        }
        false
    }

    /// Участники файла (шапка, тесты), по номеру.
    pub fn peers(&self) -> Vec<PeerInfo> {
        self.presence
            .peers
            .iter()
            .map(|(id, p)| PeerInfo {
                id: *id,
                user: p.user.clone(),
                name: p.name.clone(),
                page: p.state.as_ref().and_then(|s| s.page),
            })
            .collect()
    }

    /// За кем следуем.
    pub fn following(&self) -> Option<u32> {
        self.presence.follow
    }

    /// Следовать за участником (`None` — перестать): камера и страница повторяют его вид.
    pub fn follow(&mut self, id: Option<u32>) {
        let id = id.filter(|id| self.presence.peers.contains_key(id));
        self.set_follow(id);
        if id.is_some() {
            self.follow_peer();
        }
        self.emit();
    }

    pub(crate) fn set_follow(&mut self, id: Option<u32>) {
        if self.presence.follow != id {
            self.presence.follow = id;
            self.out.events.push(EditorEvent::Following(id));
        }
    }

    /// Действие на холсте кончает следование (поведение, как в Figma).
    pub(crate) fn stop_following(&mut self) {
        if self.presence.follow.is_some() && !self.presence.following_switch {
            self.set_follow(None);
        }
    }

    /// Вид участника, за которым следуем: его страница и камера, вписанная в свой холст.
    fn follow_peer(&mut self) {
        let Some(id) = self.presence.follow else {
            return;
        };
        let Some(state) = self.presence.peers.get(&id).and_then(|p| p.state.clone()) else {
            return;
        };
        if let Some(page) = state.page {
            if page != self.page && self.doc.page(page).is_ok() {
                self.presence.following_switch = true;
                let _ = self.set_page(page);
                self.presence.following_switch = false;
            }
        }
        if state.page == Some(self.page) {
            let mut camera = self.camera.clone();
            fit_view(&mut camera, state.view);
            if camera != self.camera {
                self.camera = camera;
                self.presence.revision += 1;
            }
        }
    }

    /// Своё действие начинается: слой чужого превью уходит из сцены до него, иначе правка
    /// прочла бы чужое превью как документ. Пары `presence_hold`/`presence_release`
    /// вкладываются.
    pub fn presence_hold(&mut self) {
        self.presence.holds += 1;
        self.drop_remote_preview();
    }

    pub fn presence_release(&mut self) {
        self.presence.holds = self.presence.holds.saturating_sub(1);
        self.emit();
    }

    pub(crate) fn drop_remote_preview(&mut self) {
        if self.scene.has_remote_preview() {
            self.scene.set_remote_preview(&[]);
        }
        self.presence.applied = None;
    }

    /// Слой чужого превью по нынешним жестам участников этой страницы. Нет его, пока идёт
    /// своё: жест, правка текста или вектора, удержание.
    pub(crate) fn sync_remote_preview(&mut self) {
        let own = self.presence.holds > 0
            || self.is_busy()
            || self.text_edit.is_some()
            || self.vector_edit.is_some()
            || self.panel.is_some();
        if own {
            self.drop_remote_preview();
            return;
        }
        let key = (
            self.presence.preview_rev,
            self.scene.generation(),
            self.scene_epoch,
            self.page,
        );
        if self.presence.applied == Some(key) {
            return;
        }
        // Базы — из документа: прежний слой сначала снимается.
        if self.scene.has_remote_preview() {
            self.scene.set_remote_preview(&[]);
        }
        let mut patches: Vec<(NodeId, PropsPatch)> = Vec::new();
        for p in self.presence.peers.values() {
            if !p.on_page(self.page) {
                continue;
            }
            let Some(g) = p.state.as_ref().and_then(|s| s.gesture) else {
                continue;
            };
            let Some(ids) = p.selection() else { continue };
            patches.extend(crate::tools::select::gesture_patches(
                &self.scene,
                ids,
                &g,
                &self.fonts,
            ));
        }
        if !patches.is_empty() {
            self.scene.set_remote_preview(&patches);
        }
        self.presence.applied = Some((
            self.presence.preview_rev,
            self.scene.generation(),
            self.scene_epoch,
            self.page,
        ));
    }

    /// Курсоры и рамки участников этой страницы для оверлея. Рамки считаются по сцене при её
    /// смене, курсоры — каждый кадр (плавное движение).
    pub(crate) fn remote_marks(&self) -> Arc<[RemoteMark]> {
        let key = (
            self.presence.revision,
            self.scene.change_counter(),
            self.scene.generation(),
            self.page,
        );
        if let Some((k, marks)) = &*self.presence.marks.borrow() {
            if *k == key {
                return marks.clone();
            }
        }
        let now = self.presence.now;
        let page = self.page;
        let scene = &self.scene;
        let marks: Vec<RemoteMark> = self
            .presence
            .peers
            .values()
            .filter(|p| p.on_page(page))
            .take(PEERS_DRAWN)
            .map(|p| {
                let state = p.state.as_ref().expect("на странице — есть состояние");
                let (boxes, frame) = selection_marks(scene, p, state);
                RemoteMark {
                    color: p.color,
                    name: p.name.clone(),
                    cursor: p.cursor.map(|c| c.at(now)),
                    boxes,
                    frame,
                }
            })
            .collect();
        let marks: Arc<[RemoteMark]> = Arc::from(marks);
        *self.presence.marks.borrow_mut() = Some((key, marks.clone()));
        marks
    }

    /// Номер того, что видно от участников: входит в ключ кадра.
    pub(crate) fn presence_revision(&self) -> u64 {
        self.presence.revision
    }

    /// Своё присутствие для других: страница, вид, курсор над холстом, выделение и жест.
    pub fn own_presence(&self) -> OwnPresence {
        let cursor = self
            .last_pointer
            .filter(|_| self.pointer_inside)
            .map(|e| self.camera.screen_to_world(e.point))
            .filter(|p| p.x.is_finite() && p.y.is_finite());
        OwnPresence {
            page: self.page,
            view: self.camera.visible_world_rect(),
            cursor,
            selection_version: self.selection.version(),
            gesture: if self.tool.is_busy() {
                self.tool.gesture()
            } else {
                None
            },
        }
    }
}

/// Своё присутствие: выделение — номером версии, узлы и рамку ядро берёт у редактора сам.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnPresence {
    pub page: PageId,
    pub view: Rect,
    pub cursor: Option<Point>,
    pub selection_version: u64,
    pub gesture: Option<Gesture>,
}

/// Рамки чужого выделения: узлы, которые есть в сцене (не больше `REMOTE_BOXES_MAX`), и общая
/// рамка. Пока части не пришли или их больше предела — рамка из кадра, сдвинутая жестом.
fn selection_marks(
    scene: &Scene,
    p: &Peer,
    state: &PeerState,
) -> (Vec<(Affine, Rect)>, Option<Rect>) {
    let Some(head) = state.selection else {
        return (Vec::new(), None);
    };
    let fallback = || {
        let r = match state.gesture {
            Some(g) => g.transform.transform_rect_bbox(head.bounds),
            None => head.bounds,
        };
        (Vec::new(), Some(r))
    };
    let Some(ids) = p.selection() else {
        return fallback();
    };
    if ids.len() < head.count as usize {
        return fallback();
    }
    let mut boxes = Vec::new();
    let mut union: Option<Rect> = None;
    let mut found = 0;
    for id in ids.iter() {
        let Some(n) = scene.node(*id) else { continue };
        let Some(b) = n.local_bounds else { continue };
        if !n.props.visible {
            continue;
        }
        found += 1;
        if boxes.len() < REMOTE_BOXES_MAX {
            boxes.push((n.world, b));
        }
        let w = n.world.transform_rect_bbox(b);
        union = Some(union.map_or(w, |u| u.union(w)));
    }
    if found == 0 {
        return (Vec::new(), None);
    }
    // У одного узла его рамка и есть рамка выделения.
    let frame = if found > 1 { union } else { None };
    (boxes, frame)
}

/// Камера так, чтобы вид участника целиком был виден в своём холсте (по центру).
pub fn fit_view(camera: &mut Camera, view: Rect) {
    let Size { width, height } = camera.viewport();
    if view.width() <= 0.0 || view.height() <= 0.0 || width <= 0.0 || height <= 0.0 {
        return;
    }
    let zoom = (width / view.width()).min(height / view.height());
    camera.set_zoom(zoom);
    camera.set_center(view.center());
}
````

В `crates/ferrum-editor/src/text_edit.rs` заменить:

````rust
        if now_ms.is_finite() {
            self.now_ms = now_ms;
            if self.text_edit.is_some() {
                self.emit();
            }
````

на:

````rust
        if now_ms.is_finite() {
            self.now_ms = now_ms;
            // Плавное движение чужих курсоров и уход молчащих участников — тоже по времени.
            let presence = self.presence_time(now_ms);
            if self.text_edit.is_some() || presence {
                self.emit();
            }
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
        );
        self.scene.clear_preview();
        let scene: &Scene = self.scene;
        let result = self
````

на:

````rust
        );
        self.scene.clear_preview();
        debug_assert!(
            !self.scene.has_remote_preview(),
            "чужое превью попало бы в документ"
        );
        let scene: &Scene = self.scene;
        let result = self
````

В `crates/ferrum-editor/src/tools/mod.rs` заменить:

````rust
    fn cursor(&self) -> Cursor;
    fn overlay(&self) -> ToolOverlay;
}
````

на:

````rust
    fn cursor(&self) -> Cursor;
    fn overlay(&self) -> ToolOverlay;
    /// Идущий жест для других участников (живое превью): перенос, ресайз, поворот выделения.
    fn gesture(&self) -> Option<crate::presence::Gesture> {
        None
    }
}
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
use crate::input::{Modifiers, PointerEvent};
use crate::ops::{duplicate, local_delta, reposition};
use crate::scale::{scale_patches, uniform_factor};
use crate::selection::{
````

на:

````rust
use crate::input::{Modifiers, PointerEvent};
use crate::ops::{duplicate, local_delta, reposition};
use crate::presence::{Gesture, GestureKind};
use crate::scale::{scale_patches, uniform_factor};
use crate::selection::{
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
}

/// Узлы, которые жест может менять: есть в сцене, не черновики, не заблокированы.
fn editable(cx: &Ctx, ids: &[NodeId]) -> Vec<NodeId> {
````

на:

````rust
}

/// Правки живого превью чужого жеста (спека подсистемы 4, раздел 9): те же функции, что у
/// своего жеста этого вида, по выделению участника и матрице его жеста. Привязки и пиксели
/// участник уже учёл в матрице. Сцена — без чужого превью: базы берутся из документа.
pub(crate) fn gesture_patches(
    scene: &ferrum_scene::Scene,
    selection: &[NodeId],
    g: &Gesture,
    fonts: &ferrum_text::SharedFonts,
) -> Vec<(NodeId, PropsPatch)> {
    let ids: Vec<NodeId> = top_level(scene, selection)
        .into_iter()
        .filter(|id| {
            scene
                .slot(*id)
                .is_some_and(|s| !scene.is_draft(*id) && !locked(scene, s))
        })
        .collect();
    if ids.is_empty() {
        return Vec::new();
    }
    match g.kind {
        GestureKind::Move => {
            let delta = g.transform.translation();
            ids.iter()
                .filter_map(|id| {
                    let n = scene.node(*id)?;
                    let parent_world = n.parent.map_or(Affine::IDENTITY, |p| scene.at(p).world);
                    let dl = local_delta(parent_world, delta);
                    Some((
                        *id,
                        PropsPatch {
                            x: Some(n.props.x + dl.x),
                            y: Some(n.props.y + dl.y),
                            ..Default::default()
                        },
                    ))
                })
                .collect()
        }
        GestureKind::Rotate => rigid_patches(&bases(scene, &ids, false), g.transform, false),
        GestureKind::Resize => {
            let bases = bases(scene, &ids, true);
            let mut patches = scale_patches_in(&bases, g.transform, g.frame, false.into());
            // Реестр шрифтов занят (раскладка посреди кадра) — текст без переноса строк до
            // следующего кадра участника.
            if let Ok(mut f) = fonts.try_borrow_mut() {
                resize_texts(&mut f, bases.iter().map(|b| (b.id, &b.props)), &mut patches);
            }
            patches
        }
        GestureKind::Scale => {
            let bases = bases_deep(scene, &ids);
            match fonts.try_borrow_mut() {
                Ok(mut f) => scale_patches(&bases, g.transform, g.frame, g.k, Some(&mut f)),
                Err(_) => scale_patches(&bases, g.transform, g.frame, g.k, None),
            }
        }
    }
}

/// Узлы, которые жест может менять: есть в сцене, не черновики, не заблокированы.
fn editable(cx: &Ctx, ids: &[NodeId]) -> Vec<NodeId> {
````

В `crates/ferrum-editor/src/tools/select.rs` заменить:

````rust
    }

    fn overlay(&self) -> ToolOverlay {
        let mut out = ToolOverlay {
````

на:

````rust
    }

    /// Перенос (без копий с Alt: черновиков у других нет), ресайз, «Масштаб» и поворот выделения —
    /// той же матрицей, что и превью здесь. Конец линии и рамка выделения другим не видны.
    fn gesture(&self) -> Option<Gesture> {
        match &self.state {
            State::Dragging(d) if d.copies.is_none() => Some(Gesture {
                kind: GestureKind::Move,
                transform: Affine::translate(d.delta),
                frame: Affine::IDENTITY,
                k: 1.0,
            }),
            State::Resizing(r) => Some(Gesture {
                kind: if self.scale {
                    GestureKind::Scale
                } else {
                    GestureKind::Resize
                },
                transform: r.transform,
                frame: r.sbox.to_world,
                k: r.k,
            }),
            State::Rotating(r) => Some(Gesture {
                kind: GestureKind::Rotate,
                transform: r.transform,
                frame: Affine::IDENTITY,
                k: 1.0,
            }),
            _ => None,
        }
    }

    fn overlay(&self) -> ToolOverlay {
        let mut out = ToolOverlay {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                EditorEvent::CommandStates(changed) => self.push_command_states(changed),
                EditorEvent::Error(message) => self.events.push(CoreEvent::Error { message }),
            }
        }
````

на:

````rust
                EditorEvent::CommandStates(changed) => self.push_command_states(changed),
                EditorEvent::Error(message) => self.events.push(CoreEvent::Error { message }),
                // Интерфейсу о следовании скажет следующая задача.
                EditorEvent::Following(_) => {}
            }
        }
````

В `crates/ferrum-wasm/src/input.rs` заменить:

````rust
        Record::Input(Input::KeyUp(e)) => key(&mut out, kind::KEY_UP, e),
        Record::Input(Input::Blur) => out[field::KIND] = kind::BLUR,
        Record::Resize { width, height, dpr } => {
            out[field::KIND] = kind::RESIZE;
````

на:

````rust
        Record::Input(Input::KeyUp(e)) => key(&mut out, kind::KEY_UP, e),
        Record::Input(Input::Blur) => out[field::KIND] = kind::BLUR,
        Record::Input(Input::PointerLeave) => out[field::KIND] = kind::POINTER_LEAVE,
        Record::Resize { width, height, dpr } => {
            out[field::KIND] = kind::RESIZE;
````

В `crates/ferrum-wasm/src/input.rs` заменить:

````rust
        kind::KEY_UP => Input::KeyUp(key()?),
        kind::BLUR => Input::Blur,
        kind::RESIZE => {
            let (width, height, dpr) = (
````

на:

````rust
        kind::KEY_UP => Input::KeyUp(key()?),
        kind::BLUR => Input::Blur,
        kind::POINTER_LEAVE => Input::PointerLeave,
        kind::RESIZE => {
            let (width, height, dpr) = (
````

В `crates/ferrum-wasm/src/layout.rs` заменить:

````rust
    /// Размер холста: CSS-пиксели и `devicePixelRatio`.
    pub const RESIZE: u8 = 10;
}

````

на:

````rust
    /// Размер холста: CSS-пиксели и `devicePixelRatio`.
    pub const RESIZE: u8 = 10;
    /// Указатель ушёл с холста (`pointerleave`): курсор не виден другим участникам.
    pub const POINTER_LEAVE: u8 = 11;
}

````

В `crates/ferrum-wasm/src/layout.rs` заменить:

````rust
            ("KEY_UP", kind::KEY_UP as usize),
            ("BLUR", kind::BLUR as usize),
            ("RESIZE", kind::RESIZE as usize),
        ],
````

на:

````rust
            ("KEY_UP", kind::KEY_UP as usize),
            ("BLUR", kind::BLUR as usize),
            ("POINTER_LEAVE", kind::POINTER_LEAVE as usize),
            ("RESIZE", kind::RESIZE as usize),
        ],
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm`
Expected: ядро собрано: `src/wasm`, в типах ввода появился `POINTER_LEAVE`

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor -p ferrum-wasm`

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Expected: PASS: всё без замечаний; `ferrum-editor` и `ferrum-wasm` 773 passed

- [ ] **Step 6: Закоммитить**

```bash
git add apps/web/src/core/layout.gen.ts crates/ferrum-editor/examples/editor_report.rs crates/ferrum-editor/src/actions.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/input.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/presence.rs crates/ferrum-editor/src/text_edit.rs crates/ferrum-editor/src/tools/mod.rs crates/ferrum-editor/src/tools/select.rs crates/ferrum-editor/tests/presence.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/input.rs crates/ferrum-wasm/src/layout.rs
git commit -F - <<'EOF'
редактор: другие участники — курсоры, выделения, живое превью, следование

Плавное движение курсора, уход через 30 с, рамки по сцене. Живое превью — слоем сцены по матрице жеста теми же функциями,
что свой жест; пока идёт своё действие, слоя нет (снятие в run и begin_doc_edit, проверка в commit и transact). Следование:
камера и страница за участником, выход — действие на холсте. Своё присутствие — own_presence; уход указателя с холста.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 6: Ядро worker'а — команды и события присутствия

Спека §9. Записи сервера с проверкой пределов, своё состояние не чаще 20 раз в секунду, `presence_hold` вокруг команд и нажатий.

**Files:**
- Create: `crates/ferrum-wasm/src/core/presence.rs`
- Modify: `crates/ferrum-wasm/src/core.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `crates/ferrum-wasm/src/core/text.rs`, `crates/ferrum-wasm/src/protocol.rs`
- Test: `crates/ferrum-wasm/tests/presence.rs` (новый)

**Interfaces:**
- Consumes: `Editor::presence_*` задачи 5.
- Produces: команды `Presence { items }`, `PresenceReset`, `Follow { participant }`, тестовая `ReadPresence`; события `Presence { page, view, cursor, selection, parts, gesture }`, `Following`, `PresenceMarks`; `PRESENCE_ITEMS_MAX = 4096`, `PRESENCE_PEERS_MAX = 1024`, `PRESENCE_PART_IDS_MAX = 2048`, `PRESENCE_SEND_MS = 50`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-wasm/tests/presence.rs`:

````rust
//! Присутствие в ядре worker'а (спека подсистемы 4, раздел 9): записи сервера — редактору с
//! проверкой, своё состояние — не чаще 20 раз в секунду и только при изменении, чужое превью
//! не попадает в документ ни через команду, ни через ввод.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerEvent};
use ferrum_wasm::core::presence::{encode_selection, PRESENCE_PART_BYTES};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;

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

fn world(c: &Core, x: f64, y: f64) -> kurbo::Point {
    c.editor().camera().screen_to_world(kurbo::Point::new(x, y))
}

fn page(c: &Core) -> String {
    c.editor().page().to_string()
}

/// Прямоугольник инструментом; возвращает его идентификатор.
fn rect(c: &mut Core) -> String {
    let at = |x: f64, y: f64, t: f64| PointerEvent::new((x, y), Modifiers::NONE, t);
    c.command(Command::SetTool { tool: Tool::Rect });
    c.handle(Input::PointerDown(at(100.0, 100.0, 1000.0)));
    c.handle(Input::PointerMove(at(200.0, 200.0, 1100.0)));
    c.handle(Input::PointerUp(at(200.0, 200.0, 1200.0)));
    c.take_events();
    c.editor().selection()[0].to_string()
}

fn join(id: u32) -> PresenceItemDto {
    PresenceItemDto::Join {
        participant: id,
        user: format!("u-{id}"),
        name: "Анна".into(),
        color: 0x0d74ce,
    }
}

fn st(c: &Core, id: u32, cursor: [f64; 2]) -> PresenceItemDto {
    PresenceItemDto::State {
        participant: id,
        page: page(c),
        view: [0.0, 0.0, 800.0, 600.0],
        cursor: Some(cursor),
        selection: None,
        gesture: None,
    }
}

fn drag(c: &Core, id: u32, node: &str, dx: f64) -> Vec<PresenceItemDto> {
    vec![
        PresenceItemDto::Part {
            participant: id,
            rev: 1,
            index: 0,
            parts: 1,
            ids: vec![node.into()],
        },
        PresenceItemDto::State {
            participant: id,
            page: page(c),
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
                transform: [1.0, 0.0, 0.0, 1.0, dx, 0.0],
                frame: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                k: 1.0,
            }),
        },
    ]
}

fn x_of(c: &mut Core, id: &str) -> (f64, f64) {
    let node = id.parse().unwrap();
    let shown = c.editor().scene().node(node).unwrap().props.x;
    (shown, x_in_doc(c, id))
}

fn x_in_doc(c: &mut Core, id: &str) -> f64 {
    let node = id.parse().unwrap();
    let page = c.editor().page();
    c.editor()
        .document()
        .loaded_page(page)
        .unwrap()
        .props(node)
        .unwrap()
        .x
}

#[test]
fn server_items_reach_the_editor_and_bad_ones_are_skipped_alone() {
    let mut c = core();
    let p = page(&c);
    let mut bad_view = st(&c, 1, [1.0, 1.0]);
    if let PresenceItemDto::State { view, .. } = &mut bad_view {
        *view = [0.0, 0.0, f64::NAN, 1.0];
    }
    c.command(Command::Presence {
        items: vec![
            join(1),
            bad_view,
            // Без `join` участника нет.
            st(&c, 2, [3.0, 3.0]),
            PresenceItemDto::State {
                participant: 1,
                page: p.clone(),
                view: [0.0, 0.0, 800.0, 600.0],
                cursor: Some([1e12, 0.0]),
                selection: None,
                gesture: None,
            },
            PresenceItemDto::Part {
                participant: 1,
                rev: 1,
                index: 1,
                parts: 1,
                ids: vec!["1@2".into()],
            },
            PresenceItemDto::Part {
                participant: 1,
                rev: 1,
                index: 0,
                parts: 17,
                ids: vec!["1@2".into()],
            },
            PresenceItemDto::Part {
                participant: 1,
                rev: 1,
                index: 0,
                parts: 1,
                ids: vec!["мусор".into()],
            },
        ],
    });
    assert_eq!(c.editor().peers().len(), 1);
    assert!(
        c.editor().overlay().remote.is_empty(),
        "ни одно неверное состояние не принято"
    );
    c.command(Command::Presence {
        items: vec![st(&c, 1, [40.0, 50.0])],
    });
    let o = c.editor().overlay();
    assert_eq!(o.remote.len(), 1);
    assert_eq!(o.remote[0].cursor, Some(kurbo::Point::new(40.0, 50.0)));
    c.command(Command::Presence {
        items: vec![PresenceItemDto::Left { participant: 1 }],
    });
    assert!(c.editor().peers().is_empty());
}

#[test]
fn participants_are_capped() {
    let mut c = core();
    let items = (0..PRESENCE_PEERS_MAX as u32 + 10).map(join).collect();
    c.command(Command::Presence { items });
    assert_eq!(c.editor().peers().len(), PRESENCE_PEERS_MAX);
}

/// Своё состояние: только после начала синхронизации, не чаще 50 мс, только при изменении.
#[test]
fn own_presence_is_sent_on_change_at_most_20_times_a_second() {
    let mut c = core();
    let presence = |c: &mut Core| {
        c.take_events()
            .into_iter()
            .filter(|e| matches!(e, CoreEvent::Presence { .. }))
            .collect::<Vec<_>>()
    };
    c.set_time(0.0);
    assert!(presence(&mut c).is_empty(), "без синхронизации — молчит");
    c.command(Command::SyncStart);
    c.set_time(10.0);
    assert_eq!(presence(&mut c).len(), 1);
    c.set_time(100.0);
    assert!(presence(&mut c).is_empty(), "без изменений — молчит");
    let at = |x: f64| PointerEvent::new((x, 10.0), Modifiers::NONE, 0.0);
    c.handle(Input::PointerMove(at(10.0)));
    c.set_time(120.0);
    let sent = presence(&mut c);
    let x10 = world(&c, 10.0, 10.0).x;
    assert!(matches!(&sent[..], [CoreEvent::Presence { cursor: Some([x, _]), .. }] if *x == x10));
    c.handle(Input::PointerMove(at(20.0)));
    c.set_time(140.0);
    assert!(presence(&mut c).is_empty(), "раньше 50 мс — ждёт");
    c.set_time(175.0);
    let x20 = world(&c, 20.0, 10.0).x;
    assert!(
        matches!(&presence(&mut c)[..], [CoreEvent::Presence { cursor: Some([x, _]), .. }] if *x == x20)
    );
    c.handle(Input::PointerLeave);
    c.set_time(240.0);
    assert!(matches!(
        &presence(&mut c)[..],
        [CoreEvent::Presence { cursor: None, .. }]
    ));
}

#[test]
fn own_selection_goes_with_parts_once_per_version() {
    let mut c = core();
    c.command(Command::SyncStart);
    let id = rect(&mut c);
    c.set_time(1000.0);
    let events = c.take_events();
    let (sel, parts) = events
        .iter()
        .find_map(|e| match e {
            CoreEvent::Presence {
                selection, parts, ..
            } => Some((*selection, parts.clone())),
            _ => None,
        })
        .unwrap();
    let sel = sel.expect("выделение");
    assert_eq!((sel.count, sel.parts), (1, 1));
    let (a, b) = (world(&c, 100.0, 100.0), world(&c, 200.0, 200.0));
    assert_eq!(sel.bounds, [a.x, a.y, b.x, b.y]);
    let parts = parts.expect("части с новой версией");
    assert_eq!(
        parts[0].to_vec(),
        encode_selection(&[id.parse().unwrap()])[0]
    );
    // Рамка сдвинулась, версия та же: частей нет.
    let at = |x: f64, y: f64, t: f64| PointerEvent::new((x, y), Modifiers::NONE, t);
    c.handle(Input::PointerDown(at(150.0, 150.0, 2000.0)));
    c.handle(Input::PointerMove(at(170.0, 150.0, 2100.0)));
    c.set_time(2000.0);
    let again = c.take_events();
    let found = again.iter().find_map(|e| match e {
        CoreEvent::Presence {
            selection,
            parts,
            gesture,
            ..
        } => Some((*selection, parts.is_some(), *gesture)),
        _ => None,
    });
    let (sel2, has_parts, gesture) = found.unwrap();
    assert_eq!(sel2.unwrap().rev, sel.rev);
    assert!(!has_parts);
    assert_eq!(gesture.unwrap().kind, GestureKindDto::Move);
}

/// Совпадает с `packages/protocol/test/presence.test.ts` («узлы выделения: запись одна»).
#[test]
fn selection_encoding_matches_protocol() {
    let ids: Vec<_> = [
        "5@2",
        "1@2",
        "1@18446744073709551615",
        "2147483647@3",
        "7@2",
    ]
    .iter()
    .map(|s| s.parse().unwrap())
    .collect();
    let parts = encode_selection(&ids);
    assert_eq!(parts.len(), 1);
    let mut want = Vec::new();
    want.extend_from_slice(&2u64.to_le_bytes());
    want.extend_from_slice(&[3, 1, 4, 2]);
    want.extend_from_slice(&3u64.to_le_bytes());
    want.extend_from_slice(&[1, 0xff, 0xff, 0xff, 0xff, 0x07]);
    want.extend_from_slice(&u64::MAX.to_le_bytes());
    want.extend_from_slice(&[1, 1]);
    assert_eq!(parts[0], want);
}

#[test]
fn big_selection_is_split_into_bounded_parts() {
    let ids: Vec<_> = (0..40_000u64)
        .map(|i| {
            format!("{}@{}", (i * 7919) % 2_000_000_000, i % 20)
                .parse()
                .unwrap()
        })
        .collect();
    let parts = encode_selection(&ids);
    assert_eq!(parts.len(), 16);
    assert!(parts
        .iter()
        .all(|p| p.len() <= PRESENCE_PART_BYTES && !p.is_empty()));
}

#[test]
fn peer_preview_never_reaches_document_through_commands_or_input() {
    let mut c = core();
    let id = rect(&mut c);
    c.command(Command::Presence {
        items: [vec![join(1)], drag(&c, 1, &id, 300.0)].concat(),
    });
    let x0 = world(&c, 100.0, 100.0).x;
    assert_eq!(x_of(&mut c, &id), (x0 + 300.0, x0));
    // Поле панели «X»: +10 к документу, а не к превью.
    c.command(Command::EditSelection {
        edit: Edit::Number {
            field: Field::X,
            value: 10.0,
            relative: true,
        },
        phase: EditPhase::Commit,
        ids: None,
        selection_version: None,
    });
    assert_eq!(x_in_doc(&mut c, &id), x0 + 10.0);
    assert_eq!(
        x_of(&mut c, &id).0,
        x0 + 310.0,
        "превью вернулось поверх нового документа"
    );
    // Клавиши: стрелка сдвигает документ на 1.
    c.handle(Input::KeyDown(ferrum_editor::KeyEvent::new(
        "ArrowRight",
        Modifiers::NONE,
    )));
    c.handle(Input::KeyUp(ferrum_editor::KeyEvent::new(
        "ArrowRight",
        Modifiers::NONE,
    )));
    assert_eq!(x_in_doc(&mut c, &id), x0 + 11.0);
    // Отмена и повтор — свои шаги, без чужого сдвига.
    c.command(Command::Execute {
        command: "edit.undo".into(),
    });
    assert_eq!(x_in_doc(&mut c, &id), x0 + 10.0);
    // Участник ушёл — сцена совпадает с документом.
    c.command(Command::Presence {
        items: vec![PresenceItemDto::Left { participant: 1 }],
    });
    assert_eq!(x_of(&mut c, &id), (x0 + 10.0, x0 + 10.0));
}

#[test]
fn following_is_reported_to_the_interface() {
    let mut c = core();
    c.command(Command::Presence {
        items: vec![join(4), st(&c, 4, [1.0, 1.0])],
    });
    c.take_events();
    c.command(Command::Follow {
        participant: Some(4),
    });
    let ev = c.take_events();
    assert!(ev.contains(&CoreEvent::Following {
        participant: Some(4)
    }));
    c.handle(Input::PointerDown(PointerEvent::new(
        (5.0, 5.0),
        Modifiers::NONE,
        10.0,
    )));
    assert!(c
        .take_events()
        .contains(&CoreEvent::Following { participant: None }));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test presence`
Expected: FAIL — не собирается: нет `ferrum_wasm::core::presence` и `PresenceItemDto`, 37 ошибок компиляции

- [ ] **Step 3: Реализация**

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
mod fonts;
mod panels;
mod sync;
mod text;
````

на:

````rust
mod fonts;
mod panels;
pub mod presence;
mod sync;
mod text;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    /// Синхронизация с сервером: не включена у локальных файлов.
    sync: sync::SyncCore,
    /// Ревизия правок: растёт с каждой правкой и заменой документа. Снимок несёт её,
    /// а `MarkSaved` снимает `modified`, только если с тех пор она не сменилась.
````

на:

````rust
    /// Синхронизация с сервером: не включена у локальных файлов.
    sync: sync::SyncCore,
    /// Своё присутствие для других участников: что и когда сказано.
    presence: presence::PresenceOut,
    /// Ревизия правок: растёт с каждой правкой и заменой документа. Снимок несёт её,
    /// а `MarkSaved` снимает `modified`, только если с тех пор она не сменилась.
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            | Command::AddRemoteImage { .. }
            | Command::Thumbnail
            | Command::Demo { .. }
            | Command::DemoText { .. }
````

на:

````rust
            | Command::AddRemoteImage { .. }
            | Command::Thumbnail
            | Command::Presence { .. }
            | Command::PresenceReset
            | Command::Demo { .. }
            | Command::DemoText { .. }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            | Command::ReadPixels { .. }
            | Command::ReadMeasurements
            | Command::SetTheme { .. }
            | Command::LayersWindow { .. }
````

на:

````rust
            | Command::ReadPixels { .. }
            | Command::ReadMeasurements
            | Command::ReadPresence
            | Command::SetTheme { .. }
            | Command::LayersWindow { .. }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            journal_batch: 0,
            sync: sync::SyncCore::default(),
            fonts,
            font_state: fonts::FontState::default(),
````

на:

````rust
            journal_batch: 0,
            sync: sync::SyncCore::default(),
            presence: Default::default(),
            fonts,
            font_state: fonts::FontState::default(),
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        self.editor.take_text_pending();
        let retry = input.clone();
        self.editor.handle(input);
        if self.editor.take_text_pending() {
            self.wait(fonts::Waiting::Input(retry));
````

на:

````rust
        self.editor.take_text_pending();
        let retry = input.clone();
        // Нажатие, отпускание и клавиши могут править: чужого превью в сцене на это время нет.
        // Движение, колесо и щипок не правят (жест уже держит превью снятым сам).
        let hold = !matches!(
            input,
            Input::PointerMove(_) | Input::Wheel(_) | Input::Pinch { .. } | Input::PointerLeave
        );
        if hold {
            self.editor.presence_hold();
        }
        self.editor.handle(input);
        if hold {
            self.editor.presence_release();
        }
        if self.editor.take_text_pending() {
            self.wait(fonts::Waiting::Input(retry));
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    /// (`waits_behind_fonts`): порядок действий пользователя сохраняется.
    pub fn command(&mut self, cmd: Command) {
        if self.fonts_awaited() && waits_behind_fonts(&cmd) {
            self.wait(fonts::Waiting::Command(cmd));
        } else {
            self.run_command(cmd);
        }
        self.drain_editor();
````

на:

````rust
    /// (`waits_behind_fonts`): порядок действий пользователя сохраняется.
    pub fn command(&mut self, cmd: Command) {
        // Команда может править документ по свойствам из сцены: чужого превью в ней нет, пока
        // она идёт (спека подсистемы 4, раздел 9).
        let hold = presence::holds(&cmd);
        if hold {
            self.editor.presence_hold();
        }
        if self.fonts_awaited() && waits_behind_fonts(&cmd) {
            self.wait(fonts::Waiting::Command(cmd));
        } else {
            self.run_command(cmd);
        }
        if hold {
            self.editor.presence_release();
        }
        self.drain_editor();
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            }
            Command::Thumbnail => self.push_thumbnail(),
            Command::FlushJournal => {
                self.settle_waiting_fonts();
````

на:

````rust
            }
            Command::Thumbnail => self.push_thumbnail(),
            Command::Presence { items } => self.presence_items(items),
            Command::PresenceReset => self.presence_reset(),
            Command::Follow { participant } => self.editor.follow(participant),
            Command::FlushJournal => {
                self.settle_waiting_fonts();
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                self.dirty = true;
            }
            Command::ReadMeasurements => {
                use ferrum_render::MeasureKind;
````

на:

````rust
                self.dirty = true;
            }
            Command::ReadPresence => self.read_presence(),
            Command::ReadMeasurements => {
                use ferrum_render::MeasureKind;
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                EditorEvent::CommandStates(changed) => self.push_command_states(changed),
                EditorEvent::Error(message) => self.events.push(CoreEvent::Error { message }),
                // Интерфейсу о следовании скажет следующая задача.
                EditorEvent::Following(_) => {}
            }
        }
````

на:

````rust
                EditorEvent::CommandStates(changed) => self.push_command_states(changed),
                EditorEvent::Error(message) => self.events.push(CoreEvent::Error { message }),
                EditorEvent::Following(participant) => {
                    self.events.push(CoreEvent::Following { participant })
                }
            }
        }
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            | Command::ReadPixels { .. }
            | Command::ReadMeasurements
            | Command::DemoText { .. }
            | Command::FontsAvailable { .. }
````

на:

````rust
            | Command::ReadPixels { .. }
            | Command::ReadMeasurements
            | Command::ReadPresence
            | Command::DemoText { .. }
            | Command::FontsAvailable { .. }
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            | Command::AddRemoteImage { .. }
            | Command::Thumbnail
            | Command::TextEditStart { .. }
            | Command::TextEditEnd
````

на:

````rust
            | Command::AddRemoteImage { .. }
            | Command::Thumbnail
            | Command::Presence { .. }
            | Command::PresenceReset
            | Command::Follow { .. }
            | Command::TextEditStart { .. }
            | Command::TextEditEnd
````

Создать `crates/ferrum-wasm/src/core/presence.rs`:

````rust
//! Присутствие в ядре (спека подсистемы 4, раздел 9): записи пачек сервера от `sync.worker` —
//! редактору, своё состояние — событием `Presence` не чаще 20 раз в секунду и только при
//! изменении. Повтор того же состояния раз в 10 с, пока соединение живо, шлёт `sync.worker`:
//! в скрытой вкладке кадров нет, а участник не должен пропадать у других.
//!
//! Записи приходят из сети через `sync.worker`: разбор кадра там строгий, но ядро проверяет и само
//! — числа конечны и в пределах мира, частей и узлов не больше пределов, участников не больше
//! `PRESENCE_PEERS_MAX`.

use std::str::FromStr;

use ferrum_doc::{NodeId, PageId};
use ferrum_editor::{peer_color, Gesture, GestureKind, PeerSelection, PeerState};
use kurbo::{Affine, Point, Rect};
use serde_bytes::ByteBuf;

use super::Core;
use crate::protocol::{
    Command, CoreEvent, GestureKindDto, PresenceGestureDto, PresenceItemDto, PresenceSelectionDto,
    PRESENCE_ITEMS_MAX, PRESENCE_PART_IDS_MAX, PRESENCE_PEERS_MAX,
};

/// Своё состояние уходит не чаще (20 раз в секунду, спека раздел 9).
pub const PRESENCE_SEND_MS: f64 = 50.0;
/// Части выделения: байты узлов в кадре 2 КБ и их число — как в `packages/protocol`.
pub const PRESENCE_PART_BYTES: usize = 2040;
pub const PRESENCE_PARTS_MAX: usize = 16;
pub const PRESENCE_SELECTION_MAX: usize = 100_000;
const WORLD_MAX: f64 = 1e9;

#[derive(Default)]
pub(crate) struct PresenceOut {
    last_ms: Option<f64>,
    last: Option<CoreEvent>,
    rev: u32,
    /// Выделение, о котором сказано: версия, ключ сцены для рамки, заголовок.
    sel_version: Option<u64>,
    bounds_key: Option<(u64, u64)>,
    selection: Option<PresenceSelectionDto>,
    parts: Option<Vec<ByteBuf>>,
}

/// Команды, которые сцену не правят и чужое превью снимать не должны: присутствие, синхронизация,
/// чтение пикселей. Остальные идут с `presence_hold`: правка не прочтёт чужое превью.
pub(crate) fn holds(cmd: &Command) -> bool {
    !matches!(
        cmd,
        Command::Presence { .. }
            | Command::PresenceReset
            | Command::Follow { .. }
            | Command::SyncStart
            | Command::SyncReset
            | Command::SyncSubscribed { .. }
            | Command::SyncAcked { .. }
            | Command::SyncFlush
            | Command::ApplyRemote { .. }
            | Command::AddRemoteImage { .. }
            | Command::ReadPixels { .. }
            | Command::ReadMeasurements
            | Command::ReadPresence
    )
}

fn coord(v: f64) -> bool {
    v.is_finite() && v.abs() <= WORLD_MAX
}

fn rect(r: [f64; 4], strict: bool) -> Option<Rect> {
    if !r.iter().all(|v| coord(*v)) {
        return None;
    }
    let ok = if strict {
        r[2] > r[0] && r[3] > r[1]
    } else {
        r[2] >= r[0] && r[3] >= r[1]
    };
    ok.then(|| Rect::new(r[0], r[1], r[2], r[3]))
}

fn affine(a: [f64; 6]) -> Option<Affine> {
    a.iter().all(|v| coord(*v)).then(|| Affine::new(a))
}

fn gesture(g: PresenceGestureDto) -> Option<Gesture> {
    Some(Gesture {
        kind: match g.kind {
            GestureKindDto::Move => GestureKind::Move,
            GestureKindDto::Resize => GestureKind::Resize,
            GestureKindDto::Rotate => GestureKind::Rotate,
            GestureKindDto::Scale => GestureKind::Scale,
        },
        transform: affine(g.transform)?,
        frame: affine(g.frame)?,
        k: coord(g.k).then_some(g.k)?,
    })
}

fn gesture_dto(g: Gesture) -> Option<PresenceGestureDto> {
    let out = PresenceGestureDto {
        kind: match g.kind {
            GestureKind::Move => GestureKindDto::Move,
            GestureKind::Resize => GestureKindDto::Resize,
            GestureKind::Rotate => GestureKindDto::Rotate,
            GestureKind::Scale => GestureKindDto::Scale,
        },
        transform: g.transform.as_coeffs(),
        frame: g.frame.as_coeffs(),
        k: g.k,
    };
    // Нечисловая матрица (вырожденная рамка) не уходит: превью у других было бы мусором.
    (out.transform.iter().chain(&out.frame).all(|v| coord(*v)) && coord(out.k)).then_some(out)
}

/// Узел как пара (пир, счётчик) `TreeID`.
fn id_parts(id: NodeId) -> Option<(u64, u32)> {
    let s = id.to_string();
    let (counter, peer) = s.split_once('@')?;
    Some((peer.parse().ok()?, counter.parse().ok()?))
}

fn varint_len(v: u32) -> usize {
    match v {
        0..=0x7f => 1,
        0x80..=0x3fff => 2,
        0x4000..=0x1f_ffff => 3,
        0x20_0000..=0xfff_ffff => 4,
        _ => 5,
    }
}

fn put_varint(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// Узлы частями по `PRESENCE_PART_BYTES`, не больше `PRESENCE_PARTS_MAX` частей; запись — как
/// `encodeSelectionIds` протокола: группы по пиру (по возрастанию), счётчики разностями.
pub fn encode_selection(ids: &[NodeId]) -> Vec<Vec<u8>> {
    let mut all: Vec<(u64, u32)> = ids.iter().filter_map(|id| id_parts(*id)).collect();
    all.sort_unstable();
    all.dedup();
    let mut parts: Vec<Vec<(u64, Vec<u32>)>> = Vec::new();
    let mut groups: Vec<(u64, Vec<u32>)> = Vec::new();
    let mut size = 0;
    for (peer, counter) in all {
        let grow = match groups.last() {
            Some((p, list)) if *p == peer => {
                let n = list.len() as u32;
                varint_len(n + 1) - varint_len(n)
                    + varint_len(counter - list.last().expect("группа не пуста"))
            }
            _ => 8 + 1 + varint_len(counter),
        };
        if size + grow > PRESENCE_PART_BYTES {
            parts.push(std::mem::take(&mut groups));
            size = 0;
            if parts.len() == PRESENCE_PARTS_MAX {
                break;
            }
        }
        match groups.last_mut() {
            Some((p, list)) if *p == peer => {
                size += varint_len(list.len() as u32 + 1) - varint_len(list.len() as u32)
                    + varint_len(counter - list.last().expect("группа не пуста"));
                list.push(counter);
            }
            _ => {
                size += 8 + 1 + varint_len(counter);
                groups.push((peer, vec![counter]));
            }
        }
    }
    if !groups.is_empty() && parts.len() < PRESENCE_PARTS_MAX {
        parts.push(groups);
    }
    parts
        .into_iter()
        .map(|groups| {
            let mut out = Vec::new();
            for (peer, list) in groups {
                out.extend_from_slice(&peer.to_le_bytes());
                put_varint(&mut out, list.len() as u32);
                put_varint(&mut out, list[0]);
                for w in list.windows(2) {
                    put_varint(&mut out, w[1] - w[0]);
                }
            }
            out
        })
        .collect()
}

impl Core {
    /// Записи пачки сервера — редактору. Неверная запись пропускается одна, остальные идут.
    pub(super) fn presence_items(&mut self, items: Vec<PresenceItemDto>) {
        if items.len() > PRESENCE_ITEMS_MAX {
            self.error("Слишком много записей присутствия за раз");
            return;
        }
        for item in items {
            match item {
                PresenceItemDto::Join {
                    participant,
                    user,
                    name,
                    color,
                } => {
                    let known = self.editor.peers().iter().any(|p| p.id == participant);
                    if !known && self.editor.peers().len() >= PRESENCE_PEERS_MAX {
                        continue;
                    }
                    if user.is_empty() || user.len() > 128 || name.len() > 256 || color > 0xff_ffff
                    {
                        continue;
                    }
                    self.editor
                        .presence_join(participant, &user, &name, peer_color(color));
                }
                PresenceItemDto::State {
                    participant,
                    page,
                    view,
                    cursor,
                    selection,
                    gesture: g,
                } => {
                    let Some(view) = rect(view, true) else {
                        continue;
                    };
                    let cursor = match cursor {
                        Some([x, y]) if coord(x) && coord(y) => Some(Point::new(x, y)),
                        Some(_) => continue,
                        None => None,
                    };
                    let selection = match selection {
                        Some(s) => {
                            let ok = s.count >= 1
                                && s.count as usize <= PRESENCE_SELECTION_MAX
                                && s.parts >= 1
                                && s.parts as usize <= PRESENCE_PARTS_MAX;
                            match rect(s.bounds, false).filter(|_| ok) {
                                Some(bounds) => Some(PeerSelection {
                                    rev: s.rev,
                                    count: s.count,
                                    parts: s.parts,
                                    bounds,
                                }),
                                None => continue,
                            }
                        }
                        None => None,
                    };
                    let gesture = match g {
                        Some(g) => match gesture(g) {
                            Some(g) => Some(g),
                            None => continue,
                        },
                        None => None,
                    };
                    // Страница, которой здесь нет, — участник не на нашей странице.
                    let page = PageId::from_str(&page)
                        .ok()
                        .filter(|p| self.editor.document().pages().iter().any(|(id, _)| id == p));
                    self.editor.presence_state(
                        participant,
                        PeerState {
                            page,
                            view,
                            cursor,
                            selection,
                            gesture,
                        },
                    );
                }
                PresenceItemDto::Part {
                    participant,
                    rev,
                    index,
                    parts,
                    ids,
                } => {
                    if ids.is_empty()
                        || ids.len() > PRESENCE_PART_IDS_MAX
                        || parts == 0
                        || parts as usize > PRESENCE_PARTS_MAX
                        || index >= parts
                    {
                        continue;
                    }
                    let Ok(ids) = ids.iter().map(|s| NodeId::from_str(s)).collect() else {
                        continue;
                    };
                    self.editor
                        .presence_part(participant, rev, index, parts, ids);
                }
                PresenceItemDto::Left { participant } => self.editor.presence_left(participant),
            }
        }
    }

    /// Своё состояние для других: не чаще `PRESENCE_SEND_MS` и только при изменении.
    pub(super) fn presence_flush(&mut self) {
        if !self.sync.on {
            return;
        }
        let now = self.now_ms;
        if self
            .presence
            .last_ms
            .is_some_and(|t| now - t < PRESENCE_SEND_MS && now >= t)
        {
            return;
        }
        let own = self.editor.own_presence();
        let version = own.selection_version;
        let scene = self.editor.scene();
        let key = (scene.generation(), scene.change_counter());
        // Новое выделение — новая версия и части; та же — только рамка, если сцена менялась.
        let mut parts = None;
        if self.presence.sel_version != Some(version) || self.presence.bounds_key != Some(key) {
            let ids = self.editor.selection();
            let fresh = self.presence.sel_version != Some(version);
            self.presence.sel_version = Some(version);
            self.presence.bounds_key = Some(key);
            let bounds = ids
                .iter()
                .filter_map(|id| {
                    let n = scene.node(*id)?;
                    Some(n.world.transform_rect_bbox(n.local_bounds?))
                })
                .reduce(|a, b| a.union(b));
            match bounds {
                Some(b) if !ids.is_empty() => {
                    if fresh {
                        self.presence.rev = self.presence.rev.wrapping_add(1);
                        let encoded =
                            encode_selection(&ids[..ids.len().min(PRESENCE_SELECTION_MAX)]);
                        self.presence.parts =
                            Some(encoded.into_iter().map(ByteBuf::from).collect());
                        parts = self.presence.parts.clone();
                    }
                    let n = self.presence.parts.as_ref().map_or(0, Vec::len);
                    self.presence.selection = (n > 0
                        && [b.x0, b.y0, b.x1, b.y1].iter().all(|v| coord(*v)))
                    .then(|| PresenceSelectionDto {
                        rev: self.presence.rev,
                        count: ids.len().min(PRESENCE_SELECTION_MAX) as u32,
                        parts: n as u8,
                        bounds: [b.x0, b.y0, b.x1, b.y1],
                    });
                }
                _ => {
                    self.presence.selection = None;
                    self.presence.parts = None;
                }
            }
        }
        let view = own.view;
        if ![view.x0, view.y0, view.x1, view.y1]
            .iter()
            .all(|v| coord(*v))
            || view.x1 <= view.x0
            || view.y1 <= view.y0
        {
            return;
        }
        let event = CoreEvent::Presence {
            page: own.page.to_string(),
            view: [view.x0, view.y0, view.x1, view.y1],
            cursor: own
                .cursor
                .filter(|p| coord(p.x) && coord(p.y))
                .map(|p| [p.x, p.y]),
            selection: self.presence.selection,
            parts: None,
            gesture: own.gesture.and_then(gesture_dto),
        };
        if parts.is_none() && self.presence.last.as_ref() == Some(&event) {
            return;
        }
        self.presence.last = Some(event.clone());
        self.presence.last_ms = Some(now);
        let event = match event {
            CoreEvent::Presence {
                page,
                view,
                cursor,
                selection,
                gesture,
                ..
            } => CoreEvent::Presence {
                page,
                view,
                cursor,
                selection,
                parts,
                gesture,
            },
            e => e,
        };
        self.events.push(event);
    }

    /// Тестовая `ReadPresence`: что видно от других участников.
    pub(super) fn read_presence(&mut self) {
        let hex = |c: ferrum_render::Rgba| {
            let [r, g, b, _] = c.to_rgba8().to_u8_array();
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        let marks = self
            .editor
            .overlay()
            .remote
            .iter()
            .map(|m| crate::protocol::PresenceMarkDto {
                name: m.name.to_string(),
                color: hex(m.color),
                cursor: m.cursor.map(|p| [p.x, p.y]),
                frame: m.frame.map(|r| [r.x0, r.y0, r.x1, r.y1]),
                boxes: m.boxes.len() as u32,
            })
            .collect();
        let scene = self.editor.scene();
        let preview = scene
            .remote_patches()
            .into_iter()
            .filter_map(|(id, _)| {
                let n = scene.node(id)?;
                Some(crate::protocol::PreviewedNodeDto {
                    id: id.to_string(),
                    x: n.props.x,
                    y: n.props.y,
                })
            })
            .collect();
        self.events.push(CoreEvent::PresenceMarks {
            marks,
            following: self.editor.following(),
            preview,
        });
    }

    /// Соединение заново: о себе сказать всё снова (сервер не помнит прежнее соединение).
    pub(super) fn presence_reset(&mut self) {
        self.editor.presence_reset();
        self.presence = PresenceOut::default();
    }
}
````

В `crates/ferrum-wasm/src/core/text.rs` заменить:

````rust
        self.now_ms = now_ms;
        self.editor.set_time(now_ms);
        let overdue = self
            .font_state
````

на:

````rust
        self.now_ms = now_ms;
        self.editor.set_time(now_ms);
        // Своё присутствие — по времени кадра: не чаще 20 раз в секунду и только при изменении.
        self.presence_flush();
        let overdue = self
            .font_state
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
}

/// Правка выделения из панели свойств.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

на:

````rust
}

/// Наибольшее число записей присутствия в одной команде и участников, которых держит ядро.
pub const PRESENCE_ITEMS_MAX: usize = 4096;
pub const PRESENCE_PEERS_MAX: usize = 1024;
/// Узлов в одной части выделения: 2 КБ кадра — не больше байта на узел и заголовка.
pub const PRESENCE_PART_IDS_MAX: usize = 2048;

/// Выделение участника: версия, сколько узлов, частей и общая рамка.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresenceSelectionDto {
    pub rev: u32,
    pub count: u32,
    pub parts: u8,
    pub bounds: [f64; 4],
}

/// Вид жеста участника.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum GestureKindDto {
    Move,
    Resize,
    Rotate,
    Scale,
}

/// Жест участника: матрица `[a, b, c, d, e, f]`, рамка на начало, коэффициент «Масштаба».
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresenceGestureDto {
    pub kind: GestureKindDto,
    pub transform: [f64; 6],
    pub frame: [f64; 6],
    pub k: f64,
}

/// Запись присутствия от сервера: пришёл, состояние, часть выделения, ушёл. `color` —
/// `0xRRGGBB` из палитры протокола.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "k",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PresenceItemDto {
    Join {
        participant: u32,
        user: String,
        name: String,
        color: u32,
    },
    State {
        participant: u32,
        page: String,
        view: [f64; 4],
        #[serde(default)]
        #[tsify(type = "[number, number] | undefined")]
        cursor: Option<[f64; 2]>,
        #[serde(default)]
        #[tsify(type = "PresenceSelectionDto | undefined")]
        selection: Option<PresenceSelectionDto>,
        #[serde(default)]
        #[tsify(type = "PresenceGestureDto | undefined")]
        gesture: Option<PresenceGestureDto>,
    },
    Part {
        participant: u32,
        rev: u32,
        index: u8,
        parts: u8,
        ids: Vec<String>,
    },
    Left {
        participant: u32,
    },
}

/// Участник на холсте (тестовая `ReadPresence`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct PresenceMarkDto {
    pub name: String,
    pub color: String,
    #[tsify(type = "[number, number] | undefined")]
    pub cursor: Option<[f64; 2]>,
    #[tsify(type = "[number, number, number, number] | undefined")]
    pub frame: Option<[f64; 4]>,
    pub boxes: u32,
}

/// Узел под чужим превью: где он в сцене (тестовая `ReadPresence`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct PreviewedNodeDto {
    pub id: String,
    pub x: f64,
    pub y: f64,
}

/// Правка выделения из панели свойств.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    /// Только превью первой страницы для экрана файлов (ответ — `Thumbnail`): файл пишет журнал сервера.
    Thumbnail,
    /// Отдать журналу правки с прошлой пачки: ответ — `Journal` (и без правок).
    FlushJournal,
````

на:

````rust
    /// Только превью первой страницы для экрана файлов (ответ — `Thumbnail`): файл пишет журнал сервера.
    Thumbnail,
    /// Присутствие других участников (спека подсистемы 4, раздел 9): записи пачки сервера, уже
    /// разобранные `sync.worker`. Не больше `PRESENCE_ITEMS_MAX` за команду.
    Presence {
        items: Vec<PresenceItemDto>,
    },
    /// Соединение заново: прежние участники уходят, новые придут с сервера.
    PresenceReset,
    /// Следовать за участником (`None` — перестать).
    Follow {
        #[serde(default)]
        #[tsify(type = "number | undefined")]
        participant: Option<u32>,
    },
    /// Отдать журналу правки с прошлой пачки: ответ — `Journal` (и без правок).
    FlushJournal,
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
    },
    /// Измерения с Alt в текущем оверлее (для тестов): ответ — `Measurements`.
    ReadMeasurements,
    // ——— Этап 5: панели ———
````

на:

````rust
    },
    /// Измерения с Alt в текущем оверлее (для тестов): ответ — `Measurements`.
    /// Тестовая: курсоры и рамки других участников в оверлее, за кем следуем, узлы под их живым
    /// превью (ответ — `PresenceMarks`).
    ReadPresence,
    ReadMeasurements,
    // ——— Этап 5: панели ———
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        keep: bool,
    },
    /// Перед `Opened`: поверх файла применены пачки журнала — правки, которые не
    /// успели попасть в файл (документ изменён).
````

на:

````rust
        keep: bool,
    },
    /// Своё присутствие для других (`sync.worker`): не чаще 20 раз в секунду и только при
    /// изменении. `parts` — узлы выделения частями, когда сменилась его версия.
    Presence {
        page: String,
        view: [f64; 4],
        #[tsify(type = "[number, number] | undefined")]
        cursor: Option<[f64; 2]>,
        #[tsify(type = "PresenceSelectionDto | undefined")]
        selection: Option<PresenceSelectionDto>,
        #[tsify(type = "Uint8Array[] | undefined")]
        parts: Option<Vec<ByteBuf>>,
        #[tsify(type = "PresenceGestureDto | undefined")]
        gesture: Option<PresenceGestureDto>,
    },
    /// Следование за участником началось или кончилось (действие на холсте, уход участника).
    Following {
        #[tsify(type = "number | undefined")]
        participant: Option<u32>,
    },
    /// Перед `Opened`: поверх файла применены пачки журнала — правки, которые не
    /// успели попасть в файл (документ изменён).
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        values: Vec<f64>,
        guides: u32,
    },
    /// Пиксели кадра RGBA, строки сверху вниз.
````

на:

````rust
        values: Vec<f64>,
        guides: u32,
    },
    /// Ответ на `ReadPresence`: участники на холсте (имя, цвет `#rrggbb`, курсор и общая рамка в
    /// мире, рамок узлов), за кем следуем, узлы под чужим превью с их `x` и `y` в сцене.
    PresenceMarks {
        marks: Vec<PresenceMarkDto>,
        #[tsify(type = "number | undefined")]
        following: Option<u32>,
        preview: Vec<PreviewedNodeDto>,
    },
    /// Пиксели кадра RGBA, строки сверху вниз.
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm`
Expected: ядро собрано: `src/wasm`, в типах появились команды и события присутствия

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm`

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Expected: PASS: всё без замечаний; `ferrum-wasm` 217 passed

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/core/presence.rs crates/ferrum-wasm/src/core/text.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/presence.rs
git commit -F - <<'EOF'
ядро worker'а: присутствие — записи сервера с проверкой, своё состояние 20 раз в секунду

Команды Presence, PresenceReset, Follow, тестовая ReadPresence; события Presence и Following. Записи из сети проверяются
(числа, части, участников не больше 1024). Команды и нажатия идут с presence_hold: чужое превью не попадает в документ.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 7: Клиент — присутствие в `sync.worker`

Спека §9, §7.1. Свои кадры — не больше 25 в секунду, лишнее сливается в последнее; чужие — ядру напрямую; шапке — список участников не чаще 4 раз в секунду; повтор состояния раз в 10 с.

**Files:**
- Modify: `apps/web/src/core/sync-client.ts`, `apps/web/src/core/sync-link.ts`, `apps/web/src/core/sync-messages.ts`, `apps/web/src/ui/app.tsx`, `apps/web/src/ui/file-store.ts`, `apps/web/src/ui/server/server-store.ts`, `apps/web/src/ui/store.ts`, `apps/web/src/worker/core.worker.ts`, `apps/web/src/worker/sync.worker.ts`
- Test: `apps/web/unit/sync-client.test.ts`

**Interfaces:**
- Consumes: кадры задачи 1, команды и события задачи 6.
- Produces: `PresenceItemForCore`, `Participant`, `PRESENCE_RATE = 25`, `PRESENCE_BURST = 40`, `PRESENCE_HEARTBEAT_MS = 10 000`, `PARTICIPANTS_MS = 250`, `SyncEvents.participants`, `FileStore.user`, `participants` и `following` в хранилище.

- [ ] **Step 1: Написать тесты**

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
import { expect, test } from '@playwright/test';
import {
  encodeServerFrame,
  encodeVersion,
````

на:

````ts
import { expect, test } from '@playwright/test';
import {
  encodePartBody,
  encodePresenceBatch,
  encodePresenceBody,
  encodePresenceRecord,
  encodeSelectionIds,
  encodeServerFrame,
  encodeVersion,
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
  SYNC_PROTOCOL,
  type ClientFrame,
  type ServerFrame,
} from '@ferrum/protocol';
import {
  PING_EVERY_MS,
  PONG_WAIT_MS,
  RECONNECT_MAX_MS,
  SyncClient,
  type CoreCommand,
  type SocketLike,
  type SyncEnv,
````

на:

````ts
  SYNC_PROTOCOL,
  type ClientFrame,
  type PresenceState,
  type ServerFrame,
} from '@ferrum/protocol';
import {
  PARTICIPANTS_MS,
  PING_EVERY_MS,
  PONG_WAIT_MS,
  PRESENCE_BURST,
  PRESENCE_HEARTBEAT_MS,
  RECONNECT_MAX_MS,
  SyncClient,
  type CoreCommand,
  type Participant,
  type SocketLike,
  type SyncEnv,
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
    this.onmessage?.({ data: encodeServerFrame(f).buffer });
  }
  dropped(code = 1006): void {
    this.closedCode = code;
````

на:

````ts
    this.onmessage?.({ data: encodeServerFrame(f).buffer });
  }
  receiveRaw(b: Uint8Array): void {
    this.onmessage?.({ data: b.slice().buffer });
  }
  dropped(code = 1006): void {
    this.closedCode = code;
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
  const statuses: { status: string; detail?: string }[] = [];
  const access: AccessState[] = [];
  let reloads = 0;
  const env: SyncEnv = {
````

на:

````ts
  const statuses: { status: string; detail?: string }[] = [];
  const access: AccessState[] = [];
  const participants: Participant[][] = [];
  let reloads = 0;
  const env: SyncEnv = {
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
    access: (s) => access.push(s),
    reload: () => reloads++,
  };
  const client = new SyncClient(env);
````

на:

````ts
    access: (s) => access.push(s),
    reload: () => reloads++,
    participants: (list) => participants.push(list),
  };
  const client = new SyncClient(env);
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
    sockets,
    commands,
    flushes,
    statuses,
````

на:

````ts
    sockets,
    commands,
    participants,
    flushes,
    statuses,
````

В `apps/web/unit/sync-client.test.ts` заменить:

````ts
  expect(s.last().closedCode).toBeNull();
});
````

на:

````ts
  expect(s.last().closedCode).toBeNull();
});

// ——— Присутствие (спека подсистемы 4, раздел 9) ———

const page = '3@7910247476534078275';
const own = (x: number, extra: Partial<Extract<Parameters<SyncClient['onCore']>[0], { type: 'presence' }>> = {}) => ({
  type: 'presence' as const,
  page,
  view: [0, 0, 100, 100] as [number, number, number, number],
  cursor: [x, 1] as [number, number],
  ...extra,
});
const presenceFrames = (ws: FakeSocket) => ws.sent.filter((f) => f.t === 'presence' || f.t === 'presence-part');
const role = (ws: FakeSocket) => ws.receive({ t: 'role', role: 'editor', canShare: true, version: 1 });

test('присутствие: до роли не уходит, после — состояние, затем части выделения', () => {
  const s = stand();
  const ws = s.live();
  ws.drain();
  const ids = encodeSelectionIds(['1@2', '5@2']);
  s.client.onCore(own(10, { selection: { rev: 3, count: 2, parts: 1, bounds: [0, 0, 1, 1] }, parts: [ids] }));
  ws.drain();
  expect(presenceFrames(ws)).toEqual([]);
  role(ws);
  ws.drain();
  const sent = presenceFrames(ws);
  expect(sent.map((f) => f.t)).toEqual(['presence', 'presence-part']);
  expect(sent[0].t === 'presence' && sent[0].state?.cursor).toEqual([10, 1]);
  expect(sent[1].t === 'presence-part' && sent[1].part).toEqual({ rev: 3, index: 0, parts: 1, ids: ['1@2', '5@2'] });
});

test('присутствие: не чаще предела, лишнее сливается в последнее; повтор раз в 10 с', () => {
  const s = stand();
  const ws = s.live();
  role(ws);
  for (let i = 0; i < 200; i++) s.client.onCore(own(i));
  ws.drain();
  expect(presenceFrames(ws).length).toBeLessThanOrEqual(PRESENCE_BURST);
  s.clock.advance(100);
  ws.drain();
  const last = presenceFrames(ws).at(-1)!;
  expect(last.t === 'presence' && last.state?.cursor?.[0]).toBe(199);
  const n = presenceFrames(ws).length;
  s.clock.advance(PRESENCE_HEARTBEAT_MS + 10);
  ws.drain();
  expect(presenceFrames(ws).length).toBe(n + 1);
});

test('присутствие других: ядру — записи с цветом, шапке — список не чаще 4 раз в секунду; обрыв — сброс', () => {
  const s = stand();
  const ws = s.live();
  role(ws);
  const st: PresenceState = { page, view: [0, 0, 10, 10], cursor: [1, 2], selection: null, gesture: null };
  ws.receiveRaw(
    encodePresenceBatch([
      encodePresenceRecord({ k: 'join', participant: 7, user: 'u-7', name: 'Анна', color: 3 }),
      encodePresenceRecord({ k: 'state', participant: 7, body: encodePresenceBody(st) }),
      encodePresenceRecord({
        k: 'part',
        participant: 7,
        body: encodePartBody({ rev: 1, index: 0, parts: 1, bytes: encodeSelectionIds(['4@2']) }),
      }),
    ]),
  );
  const cmd = s.commands.findLast((c) => c.type === 'presence');
  expect(cmd).toEqual({
    type: 'presence',
    items: [
      { k: 'join', participant: 7, user: 'u-7', name: 'Анна', color: 0x0d74ce },
      { k: 'state', participant: 7, page, view: [0, 0, 10, 10], cursor: [1, 2] },
      { k: 'part', participant: 7, rev: 1, index: 0, parts: 1, ids: ['4@2'] },
    ],
  });
  expect(s.participants.at(-1)).toEqual([{ id: 7, user: 'u-7', name: 'Анна', color: '#0d74ce', page }]);
  // Второй сразу за первым — список придёт одним сообщением после паузы.
  ws.receiveRaw(encodePresenceBatch([encodePresenceRecord({ k: 'join', participant: 8, user: 'u-8', name: 'Борис', color: 1 })]));
  expect(s.participants).toHaveLength(1);
  s.clock.advance(PARTICIPANTS_MS);
  expect(s.participants.at(-1)?.map((p) => p.id)).toEqual([7, 8]);
  ws.dropped();
  expect(s.commands.at(-1)).toEqual({ type: 'presenceReset' });
  s.clock.advance(PARTICIPANTS_MS);
  expect(s.participants.at(-1)).toEqual([]);
});

test('присутствие: новое соединение получает последнее своё состояние без нового кадра ядра', () => {
  const s = stand();
  const ws = s.live();
  role(ws);
  s.client.onCore(own(42));
  ws.drain();
  ws.dropped();
  const next = nextSocket(s);
  next.open();
  next.drain();
  expect(presenceFrames(next)).toEqual([]);
  role(next);
  next.drain();
  const f = presenceFrames(next)[0];
  expect(f.t === 'presence' && f.state?.cursor?.[0]).toBe(42);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/sync-client.test.ts`
Expected: FAIL — `sync-client` не экспортирует `PARTICIPANTS_MS`: модуль тестов не загружается, «No tests found»

- [ ] **Step 3: Реализация**

В `apps/web/src/core/sync-client.ts` заменить:

````ts
import {
  encodeClientFrame,
  parseServerFrame,
  snapshotPath,
  SYNC_CLOSE,
  SYNC_PROTOCOL,
  syncPath,
  type ServerFrame,
  type SyncRefusal,
````

на:

````ts
import {
  encodeClientFrame,
  encodePartBody,
  encodePresenceBody,
  parseServerFrame,
  PRESENCE_COLORS,
  snapshotPath,
  SYNC_CLOSE,
  SYNC_PROTOCOL,
  syncPath,
  type Affine6,
  type GestureKind,
  type PresenceItem,
  type Rect4,
  type ServerFrame,
  type SyncRefusal,
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
  | { type: 'syncAcked'; doc: string; clientSeq: number }
  | { type: 'applyRemote'; doc: string; batches: Uint8Array[] }
  | { type: 'addRemoteImage'; hash: string; bytes: Uint8Array };

/** События ядра, которые идут этому классу. */
````

на:

````ts
  | { type: 'syncAcked'; doc: string; clientSeq: number }
  | { type: 'applyRemote'; doc: string; batches: Uint8Array[] }
  | { type: 'addRemoteImage'; hash: string; bytes: Uint8Array }
  /** Присутствие других участников (спека подсистемы 4, раздел 9): записи пачки сервера, уже разобранные. */
  | { type: 'presence'; items: PresenceItemForCore[] }
  /** Соединение заново: прежние участники уходят у ядра, своё состояние оно скажет снова. */
  | { type: 'presenceReset' };

/** Выделение и жест в кадре присутствия — те же поля, что у `PresenceSelection` и `PresenceGesture` протокола. */
export interface PresenceSelectionForCore {
  rev: number;
  count: number;
  parts: number;
  bounds: Rect4;
}
export interface PresenceGestureForCore {
  kind: GestureKind;
  transform: Affine6;
  frame: Affine6;
  k: number;
}

/** Запись присутствия для ядра (`PresenceItemDto`): цвет — `0xRRGGBB`, пустые поля — без значения. */
export type PresenceItemForCore =
  | { k: 'join'; participant: number; user: string; name: string; color: number }
  | {
      k: 'state';
      participant: number;
      page: string;
      view: Rect4;
      cursor?: [number, number];
      selection?: PresenceSelectionForCore;
      gesture?: PresenceGestureForCore;
    }
  | { k: 'part'; participant: number; rev: number; index: number; parts: number; ids: string[] }
  | { k: 'left'; participant: number };

/** Участник для шапки: номер соединения, человек, имя, цвет и страница. */
export interface Participant {
  id: number;
  user: string;
  name: string;
  color: string;
  page: string | null;
}

/** События ядра, которые идут этому классу. */
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
  | { type: 'localUpdates'; doc: string; clientSeq: number; bytes: Uint8Array }
  | { type: 'syncState'; unacked: boolean }
  | { type: 'imagesNeeded'; hashes: string[] };

export interface SyncEnv {
````

на:

````ts
  | { type: 'localUpdates'; doc: string; clientSeq: number; bytes: Uint8Array }
  | { type: 'syncState'; unacked: boolean }
  | { type: 'imagesNeeded'; hashes: string[] }
  /** Своё присутствие: не чаще 20 раз в секунду и при изменении; `parts` — узлы выделения новой версии. */
  | {
      type: 'presence';
      page: string;
      view: Rect4;
      cursor?: [number, number];
      selection?: PresenceSelectionForCore;
      parts?: Uint8Array[];
      gesture?: PresenceGestureForCore;
    };

export interface SyncEnv {
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
  access(state: AccessState): void;
  reload(): void;
}

````

на:

````ts
  access(state: AccessState): void;
  reload(): void;
  /** Участники файла для шапки: не чаще `PARTICIPANTS_MS` и только при изменении. */
  participants(list: Participant[]): void;
}

````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
/** Журнал файла полон: ждём, пока сервер его сожмёт. */
const LOG_FULL_WAIT_MS = 30_000;

type Out = { kind: 'image'; hash: string; bytes: Uint8Array } | { kind: 'update'; doc: string; clientSeq: number; bytes: Uint8Array };
````

на:

````ts
/** Журнал файла полон: ждём, пока сервер его сожмёт. */
const LOG_FULL_WAIT_MS = 30_000;
/** Кадры присутствия: не больше 25 в секунду (сервер принимает 30), лишнее сливается в последнее состояние. */
export const PRESENCE_RATE = 25;
export const PRESENCE_BURST = 40;
/** Повтор своего состояния, пока соединение живо: в скрытой вкладке кадров нет, а через 30 с тишины сервер скроет участника. */
export const PRESENCE_HEARTBEAT_MS = 10_000;
/** Список участников основному потоку не чаще. */
export const PARTICIPANTS_MS = 250;

type Out = { kind: 'image'; hash: string; bytes: Uint8Array } | { kind: 'update'; doc: string; clientSeq: number; bytes: Uint8Array };
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
  /** С начала подписки до первого «всё на месте». */
  private catchingUp = true;

  constructor(private readonly env: SyncEnv) {}
````

на:

````ts
  /** С начала подписки до первого «всё на месте». */
  private catchingUp = true;
  /** Присутствие: роль подтверждена этим соединением (до этого сервер кадры отбрасывает), последнее своё состояние и части
   * выделения, ожидающие отправки кадры и бакет частоты. */
  private present = false;
  private ownState: Uint8Array | null = null;
  private ownParts: Uint8Array[] = [];
  private outState: Uint8Array | null = null;
  private outParts: Uint8Array[] = [];
  private tokens = PRESENCE_BURST;
  private tokensAt = 0;
  private presenceTimer: unknown = null;
  private heartbeat: unknown = null;
  private readonly participants = new Map<number, Participant>();
  private participantsTimer: unknown = null;
  private participantsDirty = false;

  constructor(private readonly env: SyncEnv) {}
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts

  private clearTimers(): void {
    for (const t of [this.reconnectTimer, this.pingTimer, this.pongTimer, this.pauseTimer, this.retryTimer, this.openTimer, ...this.sleeps])
      this.clock(t);
    this.reconnectTimer = this.pingTimer = this.pongTimer = this.pauseTimer = this.retryTimer = this.openTimer = null;
    this.sleeps.clear();
  }
````

на:

````ts

  private clearTimers(): void {
    for (const t of [
      this.reconnectTimer,
      this.pingTimer,
      this.pongTimer,
      this.pauseTimer,
      this.retryTimer,
      this.openTimer,
      this.presenceTimer,
      this.heartbeat,
      this.participantsTimer,
      ...this.sleeps,
    ])
      this.clock(t);
    this.reconnectTimer = this.pingTimer = this.pongTimer = this.pauseTimer = this.retryTimer = this.openTimer = null;
    this.presenceTimer = this.heartbeat = this.participantsTimer = null;
    this.sleeps.clear();
  }
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
      case 'role':
        this.env.access({ kind: 'role', role: f.role, version: f.version, canShare: f.canShare, own: true });
        break;
      case 'subscribed':
````

на:

````ts
      case 'role':
        this.env.access({ kind: 'role', role: f.role, version: f.version, canShare: f.canShare, own: true });
        // Роль подтверждена: сервер принимает присутствие. Своё — сразу, новое соединение о нём не знает.
        if (!this.present) {
          this.present = true;
          if (this.ownState) this.queuePresence(this.ownState, this.ownParts);
          this.armHeartbeat();
        }
        break;
      case 'presence':
        this.onPresence(f.items);
        break;
      case 'subscribed':
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
    this.openTimer = null;
    this.lostOnce = true;
    this.ws = null;
    this.opened = false;
````

на:

````ts
    this.openTimer = null;
    this.lostOnce = true;
    this.dropPresence();
    this.ws = null;
    this.opened = false;
````

В `apps/web/src/core/sync-client.ts` заменить:

````ts
        this.pumpImages();
        break;
    }
  }

````

на:

````ts
        this.pumpImages();
        break;
      case 'presence':
        this.ownPresence(e);
        break;
    }
  }

  // ——— Присутствие (спека подсистемы 4, раздел 9) ———

  /** Своё состояние от ядра: кадр и, у новой версии выделения, её части. Без соединения — только запомнить. */
  private ownPresence(e: Extract<CoreSyncEvent, { type: 'presence' }>): void {
    let body: Uint8Array;
    try {
      body = encodePresenceBody({
        page: e.page,
        view: e.view,
        cursor: e.cursor ?? null,
        selection: e.selection ?? null,
        gesture: e.gesture ?? null,
      });
    } catch {
      // Ядро проверяет числа само; неверное сюда не доходит, а дошедшее не уходит в сеть.
      return;
    }
    this.ownState = body;
    const sel = e.selection;
    let parts: Uint8Array[] = [];
    if (e.parts && sel) {
      parts = e.parts.map((bytes, index) => encodePartBody({ rev: sel.rev, index, parts: e.parts!.length, bytes }));
      this.ownParts = parts;
    } else if (!sel) this.ownParts = [];
    // До роли очередь только копит последнее: уйдёт с ответом о роли.
    this.queuePresence(body, parts);
  }

  /** В очередь: состояние заменяет прежнее неотправленное, части новой версии — прежние. */
  private queuePresence(state: Uint8Array, parts: Uint8Array[]): void {
    this.outState = state;
    if (parts.length) this.outParts = [...parts];
    this.pumpPresence();
  }

  private pumpPresence(): void {
    if (!this.ws || !this.opened || !this.present) return;
    const now = this.env.clock.now();
    this.tokens = Math.min(PRESENCE_BURST, this.tokens + ((now - this.tokensAt) / 1000) * PRESENCE_RATE);
    this.tokensAt = now;
    while (this.tokens >= 1 && (this.outState || this.outParts.length)) {
      const body = this.outState ?? this.outParts.shift()!;
      const t = this.outState ? 'presence' : 'presence-part';
      this.outState = null;
      this.tokens -= 1;
      this.send(encodeClientFrame({ t, body }));
    }
    if ((this.outState || this.outParts.length) && this.presenceTimer === null) {
      this.presenceTimer = this.env.clock.setTimeout(() => {
        this.presenceTimer = null;
        this.pumpPresence();
      }, 1000 / PRESENCE_RATE);
    }
    this.armHeartbeat();
  }

  /** Повтор последнего состояния через `PRESENCE_HEARTBEAT_MS` после последнего кадра. */
  private armHeartbeat(): void {
    this.clock(this.heartbeat);
    this.heartbeat = this.env.clock.setTimeout(() => {
      this.heartbeat = null;
      if (this.present && this.ownState && !this.outState) this.queuePresence(this.ownState, []);
      else this.armHeartbeat();
    }, PRESENCE_HEARTBEAT_MS);
  }

  /** Соединения нет: участники пропадают, ядро забывает их; своё состояние уйдёт на следующем. */
  private dropPresence(): void {
    this.present = false;
    this.outState = null;
    this.outParts = [];
    this.clock(this.presenceTimer);
    this.clock(this.heartbeat);
    this.presenceTimer = this.heartbeat = null;
    this.env.core.command({ type: 'presenceReset' });
    if (this.participants.size > 0) {
      this.participants.clear();
      this.participantsChanged();
    }
  }

  /** Пачка сервера: ядру — записи, шапке — список участников. Порядок с правками не важен: ядро их не смешивает. */
  private onPresence(items: PresenceItem[]): void {
    const out: PresenceItemForCore[] = [];
    let changed = false;
    for (const it of items) {
      switch (it.k) {
        case 'join': {
          const color = PRESENCE_COLORS[it.color];
          const was = this.participants.get(it.participant);
          this.participants.set(it.participant, { id: it.participant, user: it.user, name: it.name, color, page: was?.page ?? null });
          changed = true;
          out.push({ k: 'join', participant: it.participant, user: it.user, name: it.name, color: parseInt(color.slice(1), 16) });
          break;
        }
        case 'state': {
          const s = it.state;
          const p = this.participants.get(it.participant);
          if (p && p.page !== s.page) {
            p.page = s.page;
            changed = true;
          }
          out.push({
            k: 'state',
            participant: it.participant,
            page: s.page,
            view: s.view,
            ...(s.cursor ? { cursor: s.cursor } : {}),
            ...(s.selection ? { selection: s.selection } : {}),
            ...(s.gesture ? { gesture: s.gesture } : {}),
          });
          break;
        }
        case 'part':
          out.push({ k: 'part', participant: it.participant, ...it.part });
          break;
        case 'left':
          changed = this.participants.delete(it.participant) || changed;
          out.push({ k: 'left', participant: it.participant });
          break;
      }
    }
    this.env.core.command({ type: 'presence', items: out });
    if (changed) this.participantsChanged();
  }

  /** Шапке — не чаще `PARTICIPANTS_MS`: присутствие приходит 30 раз в секунду, а список меняется редко. */
  private participantsChanged(): void {
    this.participantsDirty = true;
    if (this.participantsTimer !== null) return;
    this.env.participants([...this.participants.values()].map((p) => ({ ...p })));
    this.participantsDirty = false;
    this.participantsTimer = this.env.clock.setTimeout(() => {
      this.participantsTimer = null;
      if (this.participantsDirty) this.participantsChanged();
    }, PARTICIPANTS_MS);
  }

````

В `apps/web/src/core/sync-link.ts` заменить:

````ts
      return;
    }
    this.wanted = true;
    this.env.run(m.cmd);
    // Порядок порта: всё, что ядро пришлёт после этих ответов, — после команды.
````

на:

````ts
      return;
    }
    // Присутствие правок не рождает: сборка пачек на него не просыпается (оно приходит 30 раз в секунду).
    if (m.cmd.type !== 'presence' && m.cmd.type !== 'presenceReset') this.wanted = true;
    this.env.run(m.cmd);
    // Порядок порта: всё, что ядро пришлёт после этих ответов, — после команды.
````

В `apps/web/src/core/sync-messages.ts` заменить:

````ts
// Сообщения `sync.worker`: с основным потоком только состояние и роль, трафик идёт прямым портом с worker'ом ядра.

import type { CoreCommand, CoreSyncEvent, SyncStatus } from './sync-client';
import type { AccessState } from './server-files';

export type ToSync = { t: 'start'; file: string; user: string; schema: number; port: MessagePort } | { t: 'stop' };

export type FromSync = { t: 'status'; status: SyncStatus; detail?: string } | { t: 'access'; state: AccessState } | { t: 'reload' };

/** Порт между worker'ом ядра и `sync.worker`. `applied` и `reset` — ядро выполнило `applyRemote` и `syncReset`. */
````

на:

````ts
// Сообщения `sync.worker`: с основным потоком только состояние и роль, трафик идёт прямым портом с worker'ом ядра.

import type { CoreCommand, CoreSyncEvent, Participant, SyncStatus } from './sync-client';
import type { AccessState } from './server-files';

export type ToSync = { t: 'start'; file: string; user: string; schema: number; port: MessagePort } | { t: 'stop' };

export type FromSync =
  | { t: 'status'; status: SyncStatus; detail?: string }
  | { t: 'access'; state: AccessState }
  | { t: 'reload' }
  /** Участники файла для шапки (спека подсистемы 4, раздел 9): не чаще четырёх раз в секунду. */
  | { t: 'participants'; list: Participant[] };

/** Порт между worker'ом ядра и `sync.worker`. `applied` и `reset` — ядро выполнило `applyRemote` и `syncReset`. */
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx

import { createEffect, createMemo, createSignal, ErrorBoundary, onCleanup, onMount, Show, untrack } from 'solid-js';
import { detectPlatform } from '../core/core';
import { NEW_FILE_TAKEN } from '../core/file-list';
````

на:

````tsx

import { createEffect, createMemo, createSignal, ErrorBoundary, onCleanup, onMount, Show, untrack } from 'solid-js';
import { reconcile } from 'solid-js/store';
import { detectPlatform } from '../core/core';
import { NEW_FILE_TAKEN } from '../core/file-list';
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
              store.toast('Вышла новая версия Ferrum — обновите страницу, правки сохранятся');
            },
          });
        };
````

на:

````tsx
              store.toast('Вышла новая версия Ferrum — обновите страницу, правки сохранятся');
            },
            participants: (list) => store.setState('participants', reconcile(list, { key: 'id' })),
          });
        };
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
import type { FileMeta, FromIo, IoOp, IoResults, JournalMark, Section, ToIo } from '../core/file-list';
import type { AccessState } from '../core/server-files';
import type { SyncStatus } from '../core/sync-client';
import { nameFromFile } from '../core/file-list';
import { mb } from '../core/limits';
````

на:

````ts
import type { FileMeta, FromIo, IoOp, IoResults, JournalMark, Section, ToIo } from '../core/file-list';
import type { AccessState } from '../core/server-files';
import type { Participant, SyncStatus } from '../core/sync-client';
import { nameFromFile } from '../core/file-list';
import { mb } from '../core/limits';
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  status(status: SyncStatus, detail?: string): void;
  reload(): void;
}

````

на:

````ts
  status(status: SyncStatus, detail?: string): void;
  reload(): void;
  /** Участники файла для шапки (спека подсистемы 4, раздел 9). */
  participants(list: Participant[]): void;
}

````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  /** Запись зависит от сети: неудачная повторяется сама, закрытие вкладки спрашивает о незаписанном. */
  readonly remote?: boolean;
  /** Соединение файла (спека подсистемы 4, раздел 7): правки идут журналом сервера, файл целиком пишется только первый раз. */
  sync?(id: string, core: SyncCore, events: SyncEvents): { stop(): void };
````

на:

````ts
  /** Запись зависит от сети: неудачная повторяется сама, закрытие вкладки спрашивает о незаписанном. */
  readonly remote?: boolean;
  /** Кто вошёл (файлы на сервере): себя в полосе участников нет. */
  readonly user?: string;
  /** Соединение файла (спека подсистемы 4, раздел 7): правки идут журналом сервера, файл целиком пишется только первый раз. */
  sync?(id: string, core: SyncCore, events: SyncEvents): { stop(): void };
````

В `apps/web/src/ui/server/server-store.ts` заменить:

````ts
  private readonly onVisibility = () => this.post({ t: 'visible', on: document.visibilityState === 'visible' });

  constructor(private readonly user: string) {
    this.worker = new Worker(new URL('../../worker/server.worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<FromServer>) => this.onMessage(e.data);
````

на:

````ts
  private readonly onVisibility = () => this.post({ t: 'visible', on: document.visibilityState === 'visible' });

  constructor(readonly user: string) {
    this.worker = new Worker(new URL('../../worker/server.worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<FromServer>) => this.onMessage(e.data);
````

В `apps/web/src/ui/server/server-store.ts` заменить:

````ts
      if (m.t === 'status') events.status(m.status, m.detail);
      else if (m.t === 'reload') events.reload();
      else for (const w of this.watchers.get(file) ?? []) w(m.state);
    };
````

на:

````ts
      if (m.t === 'status') events.status(m.status, m.detail);
      else if (m.t === 'reload') events.reload();
      else if (m.t === 'participants') events.participants(m.list);
      else for (const w of this.watchers.get(file) ?? []) w(m.state);
    };
````

В `apps/web/src/ui/store.ts` заменить:

````ts
// общей памяти в фазе measure. Здесь ничего не вычисляется о документе.

import type { SyncStatus } from '../core/sync-client';
import { createStore, reconcile } from 'solid-js/store';
import type {
````

на:

````ts
// общей памяти в фазе measure. Здесь ничего не вычисляется о документе.

import type { Participant, SyncStatus } from '../core/sync-client';
import { createStore, reconcile } from 'solid-js/store';
import type {
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  /** Сервер разрешает звать людей в открытый файл (пришедшему по ссылке — нет). */
  fileCanShare: boolean;
  layers: { start: number; total: number; rows: LayerRow[]; reveal: number | null; revealSeq: number };
  summary: Summary | null;
````

на:

````ts
  /** Сервер разрешает звать людей в открытый файл (пришедшему по ссылке — нет). */
  fileCanShare: boolean;
  /** Участники открытого файла (спека подсистемы 4, раздел 9) и тот, за кем следуем. */
  participants: Participant[];
  following: number | null;
  layers: { start: number; total: number; rows: LayerRow[]; reveal: number | null; revealSeq: number };
  summary: Summary | null;
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  syncStatus: null,
  fileCanShare: false,
  layers: { start: 0, total: 0, rows: [], reveal: null, revealSeq: 0 },
  summary: null,
````

на:

````ts
  syncStatus: null,
  fileCanShare: false,
  participants: [],
  following: null,
  layers: { start: 0, total: 0, rows: [], reveal: null, revealSeq: 0 },
  summary: null,
````

В `apps/web/src/ui/store.ts` заменить:

````ts
        undoToast(e.text, e.step);
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
````

на:

````ts
        undoToast(e.text, e.step);
        break;
      case 'following':
        setState('following', e.participant ?? null);
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
}

const SYNC_EVENTS: ReadonlySet<string> = new Set(['syncSubscribe', 'localImage', 'localUpdates', 'syncState', 'imagesNeeded']);

function runSync(cmd: Parameters<WasmCore['command']>[0]): void {
````

на:

````ts
}

const SYNC_EVENTS: ReadonlySet<string> = new Set(['syncSubscribe', 'localImage', 'localUpdates', 'syncState', 'imagesNeeded', 'presence']);

function runSync(cmd: Parameters<WasmCore['command']>[0]): void {
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
    if (e.type === 'localUpdates' || e.type === 'localImage') transfer.push(e.bytes.buffer as ArrayBuffer);
    if (e.type === 'syncSubscribe') transfer.push(e.version.buffer as ArrayBuffer);
    syncLink.post({ t: 'event', event: e as CoreSyncEvent }, transfer);
  }
````

на:

````ts
    if (e.type === 'localUpdates' || e.type === 'localImage') transfer.push(e.bytes.buffer as ArrayBuffer);
    if (e.type === 'syncSubscribe') transfer.push(e.version.buffer as ArrayBuffer);
    if (e.type === 'presence') for (const p of e.parts ?? []) transfer.push(p.buffer as ArrayBuffer);
    syncLink.post({ t: 'event', event: e as CoreSyncEvent }, transfer);
  }
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  if (events.length === 0) return;
  // Ядро что-то сделало: следующий шаг таймера пачек соберёт правки. В простое событий нет — и сборки нет.
  syncLink.touch();
  // Трафик синхронизации основному потоку не нужен. `syncState` нужен обоим: индикатор считает `sync.worker`, а «Сохранить» ждёт сервер.
  if (syncLink.connected && events.some((e) => SYNC_EVENTS.has(e.type))) {
````

на:

````ts
  if (events.length === 0) return;
  // Ядро что-то сделало: следующий шаг таймера пачек соберёт правки. В простое событий нет — и сборки нет.
  if (events.some((e) => e.type !== 'presence')) syncLink.touch();
  // Трафик синхронизации основному потоку не нужен. `syncState` нужен обоим: индикатор считает `sync.worker`, а «Сохранить» ждёт сервер.
  if (syncLink.connected && events.some((e) => SYNC_EVENTS.has(e.type))) {
````

В `apps/web/src/worker/sync.worker.ts` заменить:

````ts
    access: (state) => post({ t: 'access', state }),
    reload: () => post({ t: 'reload' }),
  });
  port.onmessage = (ev: MessageEvent<CoreToSync>) => {
````

на:

````ts
    access: (state) => post({ t: 'access', state }),
    reload: () => post({ t: 'reload' }),
    participants: (list) => post({ t: 'participants', list }),
  });
  port.onmessage = (ev: MessageEvent<CoreToSync>) => {
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Expected: PASS: всё без замечаний; `unit` 318 passed; обе сборки собираются

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/src/core/sync-client.ts apps/web/src/core/sync-link.ts apps/web/src/core/sync-messages.ts apps/web/src/ui/app.tsx apps/web/src/ui/file-store.ts apps/web/src/ui/server/server-store.ts apps/web/src/ui/store.ts apps/web/src/worker/core.worker.ts apps/web/src/worker/sync.worker.ts apps/web/unit/sync-client.test.ts
git commit -F - <<'EOF'
клиент: присутствие в sync.worker — свои кадры, чужие ядру, участники шапке

Свои кадры только после роли, не чаще 25 в секунду (лишнее — в последнее состояние), повтор раз в 10 с для скрытой вкладки.
Пачки сервера — ядру прямым портом, список участников основному потоку не чаще 4 раз в секунду; обрыв — сброс у ядра.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 8: Интерфейс — аватары и следование

Спека §9. Полоса аватаров (пять и счётчик со списком), щелчок — следование с синей рамкой окна, курсор уходит вместе с указателем; E2E двумя браузерами на настоящем стенде.

**Files:**
- Create: `apps/web/src/ui/server/presence-bar.tsx`
- Modify: `apps/web/src/core/core.ts`, `apps/web/src/core/input.ts`, `apps/web/src/ui/app.tsx`, `apps/web/src/ui/chrome.tsx`, `apps/web/src/ui/extensions.ts`, `apps/web/src/ui/server/setup.tsx`, `apps/web/src/ui/styles/server.css`
- Test: `apps/web/e2e-server/presence.spec.ts` (новый)

**Interfaces:**
- Consumes: `participants`, `following`, команда `Follow`.
- Produces: `PresenceBar`, `AVATARS_SHOWN = 5`, `extensions.presenceBar`; E2E `e2e-server/presence.spec.ts`.

- [ ] **Step 1: Написать тесты**

Создать `apps/web/e2e-server/presence.spec.ts`:

````ts
// Присутствие на настоящем стенде (спека подсистемы 4, раздел 9): два браузера видят курсоры друг друга с подписью в мировой
// точке, рамку выделения цветом участника, живое превью перетаскивания (только в сцене), аватары и следование; ушедший — по
// закрытию вкладки и по снятию роли — пропадает сразу.

import { expect, test, type Browser, type Page } from '@playwright/test';
import { PRESENCE_COLORS, presenceColor } from '@ferrum/protocol';
import { appScreen, drag, editorReady, expectPixel, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);

interface Marks {
  marks: { name: string; color: string; cursor?: [number, number]; frame?: [number, number, number, number]; boxes: number }[];
  following?: number;
  preview: { id: string; x: number; y: number }[];
}

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

const userId = (page: Page) =>
  page.evaluate(async () => ((await (await fetch('/api/auth/session')).json()) as { user: { id: string } }).user.id);

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

/** Что видно от других: тестовая команда ядра `readPresence`. */
const marks = (page: Page): Promise<Marks> =>
  page.evaluate(
    () =>
      new Promise<Marks>((resolve) => {
        const core = window.__ferrum!.core!;
        const off = core.on((e) => {
          if (e.type !== 'presenceMarks') return;
          off();
          resolve({ marks: e.marks, following: e.following, preview: e.preview } as Marks);
        });
        core.send({ type: 'readPresence' });
      }),
  );

async function newFile(page: Page): Promise<string> {
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  await synced(page);
  return fileId(page);
}

async function join(owner: Page, guest: Page, id: string, role: 'editor' | 'viewer' = 'editor'): Promise<void> {
  const email = await guest.evaluate(
    async () => ((await (await fetch('/api/auth/session')).json()) as { user: { email: string } }).user.email,
  );
  const res = await api(owner, 'POST', `/api/files/${id}/share/invite`, { email, role });
  expect(res.status).toBe(200);
  await guest.goto(`/#/file/${id}`);
  await guest.reload();
  await editorReady(guest);
  await synced(guest);
}

/** Прямоугольник 100×80 в мире с левым верхним углом (x, y). */
async function drawRect(page: Page, x: number, y: number): Promise<void> {
  const before = (await live(page)).nodes;
  await page.keyboard.press('KeyR');
  const l = await live(page);
  await drag(page, await appScreen(page, l, x, y), 100 * l.zoom, 80 * l.zoom);
  await expect.poll(async () => (await live(page)).nodes).toBe(before + 1);
}

/** Указатель над точкой мира. */
async function hoverWorld(page: Page, x: number, y: number): Promise<void> {
  const l = await live(page);
  const p = await appScreen(page, l, x, y);
  await page.mouse.move(p.x, p.y, { steps: 3 });
}

const rgb = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));

async function pair(browser: Browser): Promise<{ a: Page; b: Page; id: string; colorA: string }> {
  const [a, b] = [await person(browser, newEmail('pa'), 'Анна Смирнова'), await person(browser, newEmail('pb'), 'Борис')];
  const id = await newFile(a);
  await join(a, b, id);
  const colorA = PRESENCE_COLORS[presenceColor(await userId(a))];
  return { a, b, id, colorA };
}

test('курсор с подписью в мировой точке, рамка выделения цветом участника; ушёл с холста — курсора нет', async ({ browser }) => {
  const { a, b, colorA } = await pair(browser);
  await drawRect(a, 0, 0);
  await hoverWorld(a, 40, 30);
  await expect
    .poll(async () => (await marks(b)).marks.map((m) => [m.name, m.color, m.cursor?.map(Math.round)]))
    .toEqual([['Анна Смирнова', colorA, [40, 30]]]);
  // Стрелка цвета участника на холсте другого — там, где эта точка мира у него на экране.
  const lb = await live(b);
  const tip = await appScreen(b, lb, 40, 30);
  await expectPixel(b, tip.x + 2, tip.y + 7, rgb(colorA), 40);
  // Выделение участника: рамка его узла.
  await a.mouse.click((await appScreen(a, await live(a), 50, 40)).x, (await appScreen(a, await live(a), 50, 40)).y);
  await expect.poll(async () => (await marks(b)).marks[0]?.boxes).toBe(1);
  // Указатель ушёл с холста — курсора у другого нет, участник есть.
  await a.mouse.move(5, 5);
  await expect.poll(async () => (await marks(b)).marks[0]?.cursor).toBeUndefined();
  expect((await marks(b)).marks).toHaveLength(1);
});

test('живое превью перетаскивания: у другого узел едет в сцене, документ — после отпускания; Esc и уход посреди жеста — превью нет', async ({
  browser,
}) => {
  const { a, b } = await pair(browser);
  await drawRect(a, 0, 0);
  await expect.poll(async () => (await live(b)).nodes).toBe(1);
  await a.keyboard.press('KeyV');
  const la = await live(a);
  const from = await appScreen(a, la, 50, 40);
  await a.mouse.move(from.x, from.y);
  await a.mouse.down();
  await a.mouse.move(from.x + 60 * la.zoom, from.y, { steps: 6 });
  // Жест идёт: у другого узел сдвинут на 60 в сцене.
  await expect.poll(async () => (await marks(b)).preview.map((p) => Math.round(p.x))).toEqual([60]);
  await a.mouse.up();
  // Отпустил: превью снято, сдвиг пришёл правкой.
  await expect.poll(async () => (await marks(b)).preview).toEqual([]);
  await expect.poll(async () => (await live(b)).selectionCount >= 0 && (await live(b)).nodes).toBe(1);
  // Esc посреди жеста: превью снято, правки нет.
  await a.mouse.move(from.x + 60 * la.zoom, from.y);
  await a.mouse.down();
  await a.mouse.move(from.x + 120 * la.zoom, from.y, { steps: 6 });
  await expect.poll(async () => (await marks(b)).preview.length).toBe(1);
  await a.keyboard.press('Escape');
  await expect.poll(async () => (await marks(b)).preview).toEqual([]);
  await a.mouse.up();
  // Ушёл посреди жеста (вкладка закрыта): превью и курсор пропали сразу.
  await a.mouse.move(from.x + 60 * la.zoom, from.y);
  await a.mouse.down();
  await a.mouse.move(from.x + 160 * la.zoom, from.y, { steps: 6 });
  await expect.poll(async () => (await marks(b)).preview.length).toBe(1);
  await a.context().close();
  await expect.poll(async () => marks(b), { timeout: 5000 }).toEqual({ marks: [], preview: [] });
});

test('аватары и следование: камера и страница за участником, действие на холсте — выход', async ({ browser }) => {
  const { a, b } = await pair(browser);
  const avatar = b.getByRole('button', { name: /^Анна Смирнова/ });
  await hoverWorld(a, 10, 10);
  await expect(avatar).toBeVisible();
  await expect(avatar).toHaveText('АС');
  await avatar.click();
  await expect(avatar).toHaveAttribute('aria-pressed', 'true');
  await expect(b.locator('.fe-stage')).toHaveClass(/fe-following/);
  // Участник уходит далеко: камера другого — за ним.
  await a.locator('canvas').hover();
  await a.mouse.wheel(3000, 2000);
  const target = await live(a);
  await expect.poll(async () => Math.round((await live(b)).cameraX - target.cameraX)).toBe(0);
  await expect.poll(async () => Math.round((await live(b)).cameraY - target.cameraY)).toBe(0);
  // Участник на новой странице — и другой там же.
  await a.getByRole('button', { name: 'Добавить страницу' }).click();
  await expect
    .poll(() => b.evaluate(() => window.__ferrum!.ui!.pages.current))
    .toBe(await a.evaluate(() => window.__ferrum!.ui!.pages.current));
  await expect(avatar).toHaveAttribute('aria-pressed', 'true');
  // Своё нажатие на холсте — выход.
  await b.locator('canvas').click({ position: { x: 300, y: 300 } });
  await expect(avatar).toHaveAttribute('aria-pressed', 'false');
  await expect(b.locator('.fe-stage')).not.toHaveClass(/fe-following/);
  expect((await marks(b)).following).toBeUndefined();
});

test('ушедший пропадает сразу: закрыл вкладку, снята роль; читатель виден', async ({ browser }) => {
  const { a, b, id } = await pair(browser);
  const emailV = newEmail('pv');
  const v = await person(browser, emailV, 'Вера');
  await join(a, v, id, 'viewer');
  await hoverWorld(v, 20, 20);
  await hoverWorld(b, 30, 30);
  await expect.poll(async () => (await marks(a)).marks.map((m) => m.name).sort()).toEqual(['Борис', 'Вера']);
  await expect(a.getByRole('button', { name: /^Вера/ })).toBeVisible();
  // Снятие роли у читателя: у остальных пропал сразу (опроса нет, соединение закрыто сервером).
  const share = (await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { id: string; email: string }[] };
  const entry = share.people.find((p) => p.email === emailV)!.id;
  const at = Date.now();
  expect((await api(a, 'DELETE', `/api/files/${id}/share/people/${entry}`)).status).toBeLessThan(300);
  await expect.poll(async () => (await marks(a)).marks.map((m) => m.name), { timeout: 3000 }).toEqual(['Борис']);
  expect(Date.now() - at).toBeLessThan(3000);
  await expect(a.getByRole('button', { name: /^Вера/ })).toHaveCount(0);
  // Закрыл вкладку — пропал сразу.
  await b.context().close();
  await expect.poll(async () => (await marks(a)).marks, { timeout: 3000 }).toEqual([]);
  await expect(a.locator('.fe-presence')).toHaveCount(0);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build:server && npm run e2e:server -- e2e-server/presence.spec.ts --project=server`
Expected: FAIL — 3 failed, 1 passed: нет полосы аватаров, следования и курсора в мировой точке; тест живого превью перетаскивания проходит уже здесь — его путь собран задачами 5–7, этот тест закрепляет его сквозь интерфейс

- [ ] **Step 3: Реализация**

В `apps/web/src/core/core.ts` заменить:

````ts
    });
    this.add(c, 'pointercancel', () => enc.simple(sink, Kind.POINTER_CANCEL));
    this.add(
      c,
````

на:

````ts
    });
    this.add(c, 'pointercancel', () => enc.simple(sink, Kind.POINTER_CANCEL));
    // Указатель ушёл с холста: другие участники больше не видят его курсор (спека подсистемы 4, раздел 9).
    this.add(c, 'pointerleave', () => enc.simple(sink, Kind.POINTER_LEAVE));
    this.add(
      c,
````

В `apps/web/src/core/input.ts` заменить:

````ts
}

export function simple(sink: Sink, kind: typeof Kind.POINTER_CANCEL | typeof Kind.BLUR): void {
  sink((v, at) => {
    clear(v, at);
````

на:

````ts
}

export function simple(sink: Sink, kind: typeof Kind.POINTER_CANCEL | typeof Kind.BLUR | typeof Kind.POINTER_LEAVE): void {
  sink((v, at) => {
    clear(v, at);
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        }}
      >
        <main class="fe-stage">
          <canvas
            ref={canvas}
````

на:

````tsx
        }}
      >
        <main class="fe-stage" classList={{ 'fe-following': store.state.following !== null }}>
          <canvas
            ref={canvas}
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
                </div>
                <Show when={!rightCollapsed()}>
                  <DesignHeader file={props.file} />
                  <PropertiesPanel theme={theme()} />
                  <ResizeEdge side="right" width={right()} onWidth={setRight} />
````

на:

````tsx
                </div>
                <Show when={!rightCollapsed()}>
                  <DesignHeader file={props.file} self={props.files.user} />
                  <PropertiesPanel theme={theme()} />
                  <ResizeEdge side="right" width={right()} onWidth={setRight} />
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
const ZOOMS = [0.5, 1, 2];

export function DesignHeader(props: { file?: FileMeta }) {
  const ed = useEditor();
  let tabs!: HTMLDivElement;
````

на:

````tsx
const ZOOMS = [0.5, 1, 2];

export function DesignHeader(props: { file?: FileMeta; self?: string }) {
  const ed = useEditor();
  let tabs!: HTMLDivElement;
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
          )}
        </Show>
        {extensions.shareButton && props.file ? (
          <extensions.shareButton
````

на:

````tsx
          )}
        </Show>
        {extensions.presenceBar && props.self ? (
          <extensions.presenceBar
            participants={() => ed.store.state.participants}
            self={props.self}
            following={() => ed.store.state.following}
            currentPage={() => ed.store.state.pages.current}
            follow={(participant) => ed.send({ type: 'follow', participant: participant ?? undefined })}
          />
        ) : null}
        {extensions.shareButton && props.file ? (
          <extensions.shareButton
````

В `apps/web/src/ui/extensions.ts` заменить:

````ts
import type { Component, JSX } from 'solid-js';
import type { FileMeta, Section } from '../core/file-list';
import type { IconName } from './icons';
import type { MenuItem } from './menu';
import type { Route } from './router';

/** Строка панели разделов: раздел или подпись группы. */
````

на:

````ts
import type { Component, JSX } from 'solid-js';
import type { FileMeta, Section } from '../core/file-list';
import type { Participant } from '../core/sync-client';
import type { IconName } from './icons';
import type { MenuItem } from './menu';
import type { Route } from './router';

/** Полоса аватаров: участники, свой человек, за кем следуем, текущая страница; `follow(null)` — перестать. */
export interface PresenceBarProps {
  participants: () => Participant[];
  self: string;
  following: () => number | null;
  currentPage: () => string;
  follow: (participant: number | null) => void;
}

/** Строка панели разделов: раздел или подпись группы. */
````

В `apps/web/src/ui/extensions.ts` заменить:

````ts
  /** Кнопка «Поделиться» открытого файла. */
  shareButton?: Component<{ file: FileMeta; role: () => FileMeta['role']; canShare: () => boolean; children?: JSX.Element }>;
} = {};

````

на:

````ts
  /** Кнопка «Поделиться» открытого файла. */
  shareButton?: Component<{ file: FileMeta; role: () => FileMeta['role']; canShare: () => boolean; children?: JSX.Element }>;
  /** Аватары участников открытого файла (спека подсистемы 4, раздел 9). */
  presenceBar?: Component<PresenceBarProps>;
} = {};

````

Создать `apps/web/src/ui/server/presence-bar.tsx`:

````tsx
// Аватары участников в шапке (спека подсистемы 4, раздел 9): по человеку один аватар, сколько бы вкладок у него ни было; больше
// пяти — счётчик со списком. Щелчок — следовать за участником, ещё раз — перестать. Себя в полосе нет: свой аватар — в меню
// аккаунта, свои другие вкладки видны курсором (поведение). Список приходит из `sync.worker` не чаще четырёх раз в секунду.

import { createMemo, createSignal, For, Show } from 'solid-js';
import type { PresenceBarProps } from '../extensions';
import { Menu, type MenuItem } from '../menu';

/** Аватаров в полосе, остальные — в счётчике. */
export const AVATARS_SHOWN = 5;

interface Person {
  user: string;
  name: string;
  color: string;
  /** Участник человека, за которым следовать: новейшее соединение. */
  participant: number;
  /** Все участники человека: за кем из них следуем. */
  ids: number[];
  elsewhere: boolean;
}

function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  const letters = parts.length > 1 ? parts[0][0] + parts[1][0] : (parts[0] ?? '?').slice(0, 1);
  return letters.toUpperCase();
}

export function PresenceBar(props: PresenceBarProps) {
  const [menu, setMenu] = createSignal<HTMLElement | null>(null);
  const people = createMemo<Person[]>(() => {
    const by = new Map<string, Person>();
    for (const p of props.participants()) {
      if (p.user === props.self) continue;
      const was = by.get(p.user);
      const elsewhere = p.page !== null && p.page !== props.currentPage();
      if (!was) {
        by.set(p.user, { user: p.user, name: p.name, color: p.color, participant: p.id, ids: [p.id], elsewhere });
      } else {
        was.ids.push(p.id);
        if (p.id > was.participant) was.participant = p.id;
        was.elsewhere &&= elsewhere;
      }
    }
    return [...by.values()];
  });
  const isFollowed = (p: Person) => {
    const f = props.following();
    return f !== null && p.ids.includes(f);
  };
  const toggle = (p: Person) => props.follow(isFollowed(p) ? null : p.participant);
  const title = (p: Person) =>
    `${p.name}${p.elsewhere ? ' — на другой странице' : ''}. ${isFollowed(p) ? 'Перестать следовать' : 'Следовать'}`;
  const items = (): MenuItem[] =>
    people().map((p) => ({
      label: `${p.name}${p.elsewhere ? ' (другая страница)' : ''}`,
      checked: isFollowed(p),
      onSelect: () => toggle(p),
    }));
  return (
    <Show when={people().length > 0}>
      <div class="fe-presence" role="group" aria-label="Участники">
        <For each={people().slice(0, AVATARS_SHOWN)}>
          {(p) => (
            <button
              class="fe-avatar"
              classList={{ 'fe-avatar--followed': isFollowed(p), 'fe-avatar--away': p.elsewhere }}
              style={{ '--avatar': p.color }}
              aria-pressed={isFollowed(p)}
              aria-label={title(p)}
              data-tip={title(p)}
              data-user={p.user}
              onClick={() => toggle(p)}
            >
              {initials(p.name)}
            </button>
          )}
        </For>
        <Show when={people().length > AVATARS_SHOWN}>
          <button
            class="fe-avatar fe-avatar--more"
            aria-haspopup="menu"
            aria-expanded={!!menu()}
            aria-label={`Все участники: ${people().length}`}
            data-tip="Все участники"
            onClick={(e) => setMenu(e.currentTarget)}
          >
            +{people().length - AVATARS_SHOWN}
          </button>
        </Show>
        <Show when={menu()}>{(a) => <Menu label="Участники" items={items()} anchor={a()} onClose={() => setMenu(null)} />}</Show>
      </div>
    </Show>
  );
}
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';
import { ShareButton } from './share-dialog';
import { fileMenuItems, MoveDialog, refreshTeams, setOnMoved, teamNavEntries, TeamsEntry, teamSectionTitle } from './teams';
````

на:

````tsx
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';
import { PresenceBar } from './presence-bar';
import { ShareButton } from './share-dialog';
import { fileMenuItems, MoveDialog, refreshTeams, setOnMoved, teamNavEntries, TeamsEntry, teamSectionTitle } from './teams';
````

В `apps/web/src/ui/server/setup.tsx` заменить:

````tsx
  extensions.newFileRoute = (section) => (section.startsWith('project:') ? { kind: 'new', project: section.slice(8) } : { kind: 'new' });
  extensions.shareButton = ShareButton;
}
````

на:

````tsx
  extensions.newFileRoute = (section) => (section.startsWith('project:') ? { kind: 'new', project: section.slice(8) } : { kind: 'new' });
  extensions.shareButton = ShareButton;
  extensions.presenceBar = PresenceBar;
}
````

В `apps/web/src/ui/styles/server.css` заменить:

````css
  justify-content: flex-start;
}
````

на:

````css
  justify-content: flex-start;
}

/* Аватары участников в шапке (спека подсистемы 4, раздел 9). */
.fe-presence {
  display: flex;
  align-items: center;
  flex: none;
}
.fe-avatar {
  width: 24px;
  height: 24px;
  margin-left: -4px;
  border-radius: 50%;
  border: 2px solid var(--surface);
  background: var(--avatar, var(--ink-muted));
  color: #fff;
  font: 600 10px/20px var(--font-sans);
  text-align: center;
  padding: 0;
  cursor: pointer;
}
.fe-avatar:first-child {
  margin-left: 0;
}
.fe-avatar--away {
  opacity: 0.55;
}
.fe-avatar--followed {
  box-shadow: 0 0 0 2px var(--avatar);
}
.fe-avatar--more {
  background: var(--surface-raised);
  color: var(--ink);
}
/* Следование за участником: синяя рамка окна (спека, раздел 9). */
.fe-stage.fe-following::after {
  content: '';
  position: absolute;
  inset: 0;
  border: 2px solid var(--select);
  pointer-events: none;
}
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Expected: PASS: всё без замечаний; `unit` 318 passed; `e2e:server` 175 passed, 11 skipped (прежние пропуски Firefox), Chromium и Firefox

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e-server/presence.spec.ts apps/web/src/core/core.ts apps/web/src/core/input.ts apps/web/src/ui/app.tsx apps/web/src/ui/chrome.tsx apps/web/src/ui/extensions.ts apps/web/src/ui/server/presence-bar.tsx apps/web/src/ui/server/setup.tsx apps/web/src/ui/styles/server.css
git commit -F - <<'EOF'
интерфейс: аватары участников, следование с рамкой окна, курсор уходит с холстом

Полоса аватаров в шапке (по человеку один, больше пяти — счётчик со списком), щелчок — следовать, ещё раз — перестать; синяя
рамка окна, пока следуем. Уход указателя с холста — запись ввода. E2E на стенде: курсор и подпись, рамка, превью, следование,
уход по закрытию и по снятию роли.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 9: Документы и финальная проверка

«Сделано (этап 4)» в спеке, §8.6 спеки ядра, `docs/running.md`, бэклог B-373…B-383.

**Files:**
- Modify: `docs/backlog.md`, `docs/running.md`, `docs/superpowers/specs/2026-10-23-ferrum-core-design.md`, `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md`

- [ ] **Step 1: Реализация**

В `docs/backlog.md` заменить:

````markdown
| B-372 | При удалении человека его `doc_peer` удаляется каскадом, а `file.content_by` становится NULL — разная семантика; индекса на `content_by` нет. | P3 | этап 3, повторное ревью финальной волны сервера |

## Код

````

на:

````markdown
| B-372 | При удалении человека его `doc_peer` удаляется каскадом, а `file.content_by` становится NULL — разная семантика; индекса на `content_by` нет. | P3 | этап 3, повторное ревью финальной волны сервера |

## Присутствие (этап 4 подсистемы 4)

| № | Что | Пр. | Источник |
|---|---|---|---|
| B-373 | Присутствие живёт в одном процессе, как рассылка правок (B-315): участники второго процесса друг друга не видят. Комната файла и пределы человека — в памяти процесса. | P3 | этап 4 |
| B-374 | Имя участника сервер берёт при `hello`: смена имени дойдёт до других только после переподключения. Аватар — инициалы, фото (`user.image`) не показывается, в Figma — фото. | P3 | этап 4 |
| B-375 | Пока идёт своё действие (жест, правка текста или вектора, жест панели, любая команда), живого превью других не видно: слой снимается, чтобы свои базы не прочли чужое превью и не записали его. В Figma чужое движение видно и во время своего. Лечится базами жеста из документа, а не из сцены. | P2 | этап 4, решение по безопасности записи |
| B-376 | Приём кадра с чужим перетаскиванием 1000 узлов — 2,9 мс (своё движение — 1,7 мс): слой снимается целиком и строится заново, чтобы базы были из документа. Можно брать свойства документа из сцены без снятия. | P3 | этап 4, `editor_report` |
| B-377 | Выделение больше 16 частей по 2 КБ (около 15–30 тысяч узлов) у других — только общей рамкой из кадра, а его живое превью — рамкой, сдвинутой жестом, без узлов. | P3 | этап 4 |
| B-378 | Перетаскивание копий с Alt и конец линии другим не видны живым превью: только курсор, а затем правка. Ресайз текста у других переносит строки по своему реестру шрифтов: если он занят посреди кадра, перенос — на следующем кадре участника. | P3 | этап 4 |
| B-379 | Узел под чужим превью, который в это время правит третий, держит базу превью до следующего кадра участника (до 50 мс): положение может на кадр отстать. | P3 | этап 4 |
| B-380 | Кадры присутствия не сжимаются: 50 участников по 20 кадров в секунду — около 9 МБ/с исходящего у сервера. Пачки по тику (33 мс) уже сливают кадры одного участника. | P3 | этап 4, проба сервера |
| B-381 | Повтор своего состояния раз в 10 с идёт из `sync.worker`: скрытая вкладка видна другим, пока соединение живо, без признака «отошёл». | P3 | этап 4 |
| B-382 | Рамка следования — синяя по спеке; в Figma — цветом участника. Метка курсора не гаснет у неподвижного курсора (поведение выбрано без сверки с Figma). | P3 | этап 4 |
| B-383 | Задержка курсора у другого — до тика рассылки (33 мс) сверх сети и плавного движения (около 50 мс): p95 35 мс на сервере при 50 участниках. Отдельного замера «движение мыши — курсор на экране другого» в двух браузерах нет. | P3 | этап 4 |

## Код

````

В `docs/running.md` заменить:

````markdown
- Прямой доступ без прокси: `FERRUM_BIND=0.0.0.0`. Опубликованный Docker порт обходит ufw. За пробросом порта Docker Desktop и OrbStack адрес клиента — адрес шлюза, предел на IP становится общим на всех.

WebSocket файла идёт по тому же адресу, `/api/files/:id/sync`. Прокси должен пропускать `Upgrade` и не обрывать простаивающее соединение раньше 2 минут (клиент проверяет связь раз в 20 с). В nginx: `proxy_http_version 1.1; proxy_set_header Upgrade $http_upgrade; proxy_set_header Connection "upgrade"; proxy_read_timeout 180s;`, вариант не проверялся (B-322).

### Файлы и данные
````

на:

````markdown
- Прямой доступ без прокси: `FERRUM_BIND=0.0.0.0`. Опубликованный Docker порт обходит ufw. За пробросом порта Docker Desktop и OrbStack адрес клиента — адрес шлюза, предел на IP становится общим на всех.

WebSocket файла идёт по тому же адресу, `/api/files/:id/sync`. По нему же идёт присутствие (курсоры, выделения, живое превью): пачки каждому участнику раз в 33 мс, их пределы — в памяти процесса, как у правок. Прокси должен пропускать `Upgrade` и не обрывать простаивающее соединение раньше 2 минут (клиент проверяет связь раз в 20 с). В nginx: `proxy_http_version 1.1; proxy_set_header Upgrade $http_upgrade; proxy_set_header Connection "upgrade"; proxy_read_timeout 180s;`, вариант не проверялся (B-322).

### Файлы и данные
````

В `docs/superpowers/specs/2026-10-23-ferrum-core-design.md` заменить:

````markdown
### 8.6 Оверлеи

В той же сцене vello поверх содержимого, в экранных пикселях: рамки выделения, ручки (8px), подсветка наведения, рамка выделения мышью, линии привязки, пиксельная сетка (зум ≥ 800%), названия фреймов, бейдж «W × H», измерения с Alt (раздел 7.3): отрезки в мире, которые редактор отдаёт в `Overlay.measurements` (расстояние со значением или направляющая), рисуются сплошной линией в 1 px (только видимая часть) с плашкой значения по середине видимой части, целиком на экране, и пунктиром в 1 px (штрих 4 px, промежуток 3 px; строятся только штрихи у экрана) — размеры экранные, от зума не зависят. Тонкие линии выравниваются по пикселям. Подписи — однострочный текст: раскладка `parley` со встроенным шрифтом Onest (SIL OFL), глифы — заливка контуров из `skrifa` через тот же трейт бэкенда (`glyph_run` в `vello_gpu` требует ресурсов рендерера с устройством). Бейдж пока набран Onest с цифрами одинаковой ширины. Цвета — из дизайн-системы (`select`, `on-select`, `component`, `ink-muted`; тёмная и светлая темы). Измерения — цвет `measure` = `danger` (#ff7a7a в тёмной, #c22c2c в светлой), текст их плашек — `on-select`; плашка — та же, что у бейджа «W × H». Совпадение цветов оверлеев с `theme.css` проверяет модульный тест интерфейса.

### 8.7 Экспорт
````

на:

````markdown
### 8.6 Оверлеи

В той же сцене vello поверх содержимого, в экранных пикселях: рамки выделения, ручки (8px), подсветка наведения, рамка выделения мышью, линии привязки, пиксельная сетка (зум ≥ 800%), названия фреймов, бейдж «W × H», измерения с Alt (раздел 7.3): отрезки в мире, которые редактор отдаёт в `Overlay.measurements` (расстояние со значением или направляющая), рисуются сплошной линией в 1 px (только видимая часть) с плашкой значения по середине видимой части, целиком на экране, и пунктиром в 1 px (штрих 4 px, промежуток 3 px; строятся только штрихи у экрана) — размеры экранные, от зума не зависят. Тонкие линии выравниваются по пикселям. Подписи — однострочный текст: раскладка `parley` со встроенным шрифтом Onest (SIL OFL), глифы — заливка контуров из `skrifa` через тот же трейт бэкенда (`glyph_run` в `vello_gpu` требует ресурсов рендерера с устройством). Бейдж пока набран Onest с цифрами одинаковой ширины. Цвета — из дизайн-системы (`select`, `on-select`, `component`, `ink-muted`; тёмная и светлая темы). Измерения — цвет `measure` = `danger` (#ff7a7a в тёмной, #c22c2c в светлой), текст их плашек — `on-select`; плашка — та же, что у бейджа «W × H». Совпадение цветов оверлеев с `theme.css` проверяет модульный тест интерфейса. Другие участники файла (спека подсистемы 4, раздел 9): курсор — готовая картинка стрелки с плашкой имени цветом участника (одна на имя, цвет и плотность, не больше 8 новых за кадр), рамки их выделения — цветом участника без ручек; рисуются под своими рамками, курсоры — поверх.

### 8.7 Экспорт
````

В `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` заменить:

````markdown
- **Предел:** курсоры рисуются для 50 участников, остальные только в счётчике.

## 10. Комментарии (этап 5)

````

на:

````markdown
- **Предел:** курсоры рисуются для 50 участников, остальные только в счётчике.

- **Сделано (этап 4):**
  - **Расхождение с §9: транспорт.** Не `EphemeralStore` Loro, а свои строгие двоичные кадры (`packages/protocol/src/presence.ts`): `presence` — состояние целиком (страница, вид, курсор, заголовок выделения, жест), `presence-part` — часть выделения, от сервера — пачка записей `join`/`state`/`part`/`left`. Проба: проверка и подпись кадра `EphemeralStore` на сервере — 54 мкс (временное хранилище, разбор, новое значение с участником), свой разбор — 0,3 мкс; значения Loro — произвольные карты, строгий разбор каждого поля всё равно нужен. Протокол 2: прежняя вкладка получает `please-reload`.
  - **Расхождение с §9: «без хранения».** Сервер держит последнее состояние участника в памяти (не на диске): пришедший позже получает всех сразу. Не больше 2 КБ состояния и 16 частей по 2 КБ на соединение; уходит с соединением.
  - **Расхождение с §9: «идентификаторы и преобразования живого превью».** 1000 перетаскиваемых узлов по идентификатору и матрице — около 36 КБ, в 2 КБ не входят. Превью — одна матрица жеста (перенос, ресайз, «Масштаб», поворот) и рамка на начало; узлы — выделение участника, его части приходят своими кадрами (1000 узлов одного автора — одна часть около 1 КБ: пиры по возрастанию, счётчики разностями LEB128). Получатель считает правки теми же функциями, что свой жест (`gesture_patches`). Выделение больше 16 частей — только рамкой (B-377).
  - **Сервер** (`sync/presence.ts`): кто, имя, цвет — с сервера (имя из базы при `hello`, цвет — FNV-1a от `userId` в палитре протокола); в кадре их нет, лишний байт — кадр отброшен. Присутствие файла — только соединениям, чью роль подтвердил `hello`; закрытие соединения (уход, снятие роли, выход, корзина) — `left` у всех сразу. Пределы: 2 КБ на кадр, 30 кадров в секунду на соединение (запас 60), 120 на человека по всем вкладкам; лишнее и неверное молча отбрасывается, без паузы и закрытия; бакеты правок присутствие не тратит. Рассылка — пачкой каждому получателю раз в 33 мс: у получателя только отметки «что изменилось», в пачку идёт последнее состояние; буфер сокета выше 64 КБ — тик пропускается, отметки сливаются, очереди нет. Молчит 30 с — пропадает у других, следующий кадр возвращает его.
  - **Ядро.** Сцена: слой чужого превью `Scene::set_remote_preview` под своим превью, в документ не идёт. Редактор (`presence.rs`): участники, плавное движение курсора (от видимого места к новому за промежуток между кадрами участника, 16–150 мс), уход через 30 с, рамки выделения (до 64 рамок узлов и общая), следование. Своё состояние — событием `Presence` не чаще 20 раз в секунду и только при изменении, части — при новой версии выделения. **(поведение)** Пока идёт своё действие (жест, правка текста или вектора, жест панели, любая команда ядра и нажатия), слоя чужого превью нет: иначе базы своего жеста прочли бы чужое превью и записали бы его (B-375). Защита в три слоя: `presence_hold` ядра, снятие в `begin_doc_edit` и `run`, `debug_assert` в `Ctx::commit` и `transact`.
  - **Рендер** (§8.6 спеки ядра): курсор — готовая картинка (стрелка с белой обводкой и плашка имени), строится один раз на имя, цвет и плотность, не больше 8 за кадр; стороны рамок по осям — заливкой прямоугольников, повёрнутые — обводкой.
  - **Клиент и интерфейс.** `sync.worker` кодирует свои кадры (не больше 25 в секунду, лишнее сливается в последнее), повторяет состояние раз в 10 с (скрытая вкладка видна, B-381), шапке — список участников не чаще 4 раз в секунду. Основной поток рисует только полосу аватаров. **(поведение)** Указатель ушёл с холста — курсора у других нет. **(поведение)** Читатель виден и видит других, как в Figma. **(поведение)** Своего аватара в полосе нет, свои другие вкладки видны курсором с своим именем. **(поведение)** Следование: камера вписывает вид участника в свой холст, страница — за ним; выход — нажатие, колесо, щипок, клавиша (кроме модификаторов), команда, своя смена страницы; уход участника кончает следование. **(поведение)** Метка курсора не гаснет; рамка выделения без подписи. **(поведение)** Перетаскивание копий с Alt и конец линии — без превью (B-378).
  - **Замеры** (Apple Silicon, машина под чужой нагрузкой 5–16):
    - Кадр с 50 курсорами с подписями и 50 чужими выделениями (по 4 узла, один повёрнут), прибавка к спискам кадра, цель 1 мс: 1920×1080 — 0,13 мс, 3840×2160 (2×) — 0,18 мс (`render_report`; отдельной пробой 0,18–0,26 мс); с растеризацией на CPU — 0,5 и 1,6 мс, справочно: на GPU растеризация не в этом потоке. Без картинок курсоров и с обводкой рамок было 0,96 и 1,35 мс: цель не держалась.
    - Сервер, 50 участников по 20 кадров в секунду, у каждого выделение 1000 узлов, жест каждый четвёртый кадр: от отправки до прихода к другому p50 19 мс, p95 35 мс, p99 37 мс (до тика 33 мс); процессор сервера 5 % ядра; живая куча +1,9 МБ (RSS +60–75 МБ — пик аллокатора); состояние участников 159 КБ. С враждебным участником (около 4000 кадров в секунду по 2 КБ) и сокетом, который не читает: 70 000 кадров отброшено, у остальных p95 34 мс, процессор 9–10 %, буфер сокетов не выше 74 КБ (`presence-load.test.ts`).
    - Живое превью чужого перетаскивания 1000 узлов (100 000 на странице), приём кадра участника: 2,9 мс (p95 3,3 мс), цель — как у своего движения, 4 мс; своё движение — 1,7 мс (B-376). Документ не меняется (`editor_report`).

## 10. Комментарии (этап 5)

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

Expected: PASS: всё без замечаний; протокол 42 pass, сервер 505 pass; `unit` 318 passed; `e2e:ci` 416 passed; `e2e:server` 175 passed, 11 skipped; `cargo test --workspace` 1314 passed; совместимость с `origin/main` подтверждена

- [ ] **Step 3: Закоммитить**

```bash
git add docs/backlog.md docs/running.md docs/superpowers/specs/2026-10-23-ferrum-core-design.md docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md
git commit -F - <<'EOF'
docs: присутствие — итоговое поведение этапа 4, замеры, бэклог B-373…B-383

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

## Что этап 4 сознательно не делает

Комментарии (этап 5). Spotlight, «Аудио», реакции и метки курсора с чатом (вне подсистемы, §1). Несколько процессов сервера: присутствие, как и рассылка правок, живёт в одном процессе (B-373). Фото в аватаре (B-374).
