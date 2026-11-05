# Ferrum, подсистема 4, этап 3: синхронизация — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Двое и больше правят один файл одновременно: правки идут журналом сервера и видны другим сразу, отмена трогает только своё, обрыв и рестарт сервера ничего не теряют, снятие роли и выход действуют мгновенно.

**Architecture:** Ядро: `Document::apply_remote` обновляет зеркало только у задетых узлов, курсор синхронизации считает, что отправить. Сервер: `SyncHub` держит WebSocket файла, порядок «проверка → журнал → ack и рассылка»; `Validator` применяет пачку к настоящему документу в Bun Worker раньше журнала; `UpdateLog` пишет группами в Postgres; `Compactor` сжимает журнал в файл. Клиент: `sync.worker.ts` с прямым портом к worker'у ядра, основной поток видит только состояние и роль.

**Tech Stack:** Bun 1.4.2, TypeScript 7.0.2, pg 8.23.1, `loro-crdt` 1.16.2 (новая на сервере), Postgres 18, Valkey 9; SolidJS 1.9.17, @playwright/test 1.64.0; Rust с `loro` 1.16.2.

**Спеки:** `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` — разделы 7, 8, а также 2, 6.3, 13, 14.

## Global Constraints

- Ветка `feat/sub4-stage3-sync` от 485279d. Каталог сборки wasm — у каждой копии свой: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0` (в `apps/web` — `"$PWD/../../target"`); команды — из корня копии, если не сказано иное. Нужны Bun 1.4, Node 24, Docker с Compose 2.24+, `wasm-bindgen` 0.2.129.
- Версии точные. Новая зависимость сервера одна: `loro-crdt` 1.16.2, та же версия Loro, что в ядре; совместимость проверяется тестом в обе стороны.
- Docker: только свои проекты compose — `ferrum` и `ferrum-test`. Порты: сервер 8090, Mailpit 8025; стенд тестов — 18090, 18025, 11025, 15432, 16379. Чужие контейнеры (`agent-dev-*`, `relay-*`) не трогать. `down -v` — только `ferrum-test` и `ferrum`. Никаких `docker system prune`.
- Порт 4173 может быть занят посторонним процессом: его не трогать, E2E тогда — через временный конфиг Playwright с другими портами (не коммитить).
- Основной поток тонкий: трафик и всё размером с документ идёт в worker'ах, основной поток видит только состояние и роль.
- Цели §13.1: чужая одиночная правка на 100 000 узлов — 2 мс нативно; своя правка видна другому — 150 мс; от кадра до рассылки на 50 участников — 40 мс. Расхождения с целями записаны в «Решения» и бэклог, а не скрыты.
- Очередей правок вне документа и журнала браузера нет: что отправить, знает только ядро. Обрыв, рестарт сервера, отказ «повторите» и конец паузы — один путь.
- Правка не теряется молча: уход, выход, смена прав и отказ сервера проходят протокол сохранения редактора и говорят человеку правду.
- У каждого маршрута: права и тест на чужого (`test/access.test.ts`), `Origin`, разбор схемой из `packages/protocol`, пределы размера и частоты, срок, ответ в формате ошибок.
- Автономная сборка и десктоп не меняют поведения, кода сервера в них нет (`check-dist.mjs`); общий код без условий по флагу, через `extensions`.
- Тесты падают при откате правки, без ожиданий по времени и зависимости от порядка.
- Поведение — как в Figma; где спека молчит, решение помечено **(поведение)**.
- Тексты интерфейса, ошибок, комментарии и сообщения коммитов — на русском; коммиты кончаются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Проверка плана

План проверен до записи. Первым делом проверен главный риск отдельной пробой (`sync_report`, `hub-load.test.ts`, `sync.spec.ts`). 100 000 узлов, одна чужая правка нативно: свойство p95 37 мкс, новый узел 42 мкс, перенос 27 мкс, удаление 21 мкс, рядом со своей правкой свойства 46 мкс при цели 2 мс. **Цель не выполнена** на пути «правка дерева параллельно своей правке дерева»: 50 мс, первая до 0,7 с, плюс около 0,5 ГБ памяти wasm, это внутри Loro (B-309). Своя правка видна другому браузеру: p50 85–88 мс, p95 по четырём прогонам 102, 112, 153 и 210 мс при цели 150, **то есть цель выдержана не на каждом прогоне**, причина не разобрана (B-327); длинных задач основного потока нет. От кадра до рассылки на 50 участников (сокеты, не браузеры): p95 36 мс при цели 40. Соединение 62 КБ. 20 000 правок: запись 4,8 с, догоняние 0,85 с, применение 0,9 с, сжатие 2 с, рост файла 4,4 байта на правку свойства.

Прототип этапа собран в отдельной копии (`plan-verify-s4s3`, ветка `verify/sub4-s3` от 485279d, по коммиту на задачу). Затем шаги этого документа сгенерированы из коммитов прототипа и применены скриптом, который читает сам этот документ, к чистой копии от 485279d (`plan-apply-s4s3`, ветка `verify/sub4-s3-apply`) в порядке задач с коммитом после каждой. После каждой из 10 задач дерево копии совпало с деревом соответствующего коммита прототипа, `git diff verify/sub4-s3 HEAD` пуст. На этой копии выполнены все шаги «Убедиться, что тесты падают» (причины записаны из этого прогона) и все шаги «Проверить», кроме `npm run e2e:ci` задачи 10.

Эта проверка нашла то, чего прототип не видел: случайный тест сходимости упал на задаче 2, зеркало расходилось с полной сборкой, когда двойник `NodeId` оставался один после своего удаления или отмены. Причина найдена и исправлена в задаче 2 (`PageDoc::settle_duplicates`), добавлены детерминированные тесты; после правки 120 000 случаев в release без падений. Текстовых операций в случайном тесте нет. Остаётся B-328: отмена удаления узла, чьё имя перешло к двойнику, пропускается вместе с поддеревом, живые узлы не теряются. Ещё два замечания были в коммитах задач 4 и 5: форматирование и линтер, исправлены там же.

Значения в шагах «Проверить»: задачи 5–10 — из этого прогона на копии, задачи 1–4 — из прогонов той же копии до перезаписи неизменившихся коммитов. `e2e:ci` на прототипе, на дереве раньше вершины с побайтно той же автономной сборкой, но до правки задачи 2 и двух правок оформления: 411 passed и 5 flaky, все прошли на повторе (`clipboard:398`, `fallback:661`, `files:693`, `journal:21`, `properties:13`; машина засыпала и была под нагрузкой, причины не разобраны). Перед PR его нужно прогнать заново на вершине ветки.

Откат правок: 52 мутации, после усиления тестов пойманы все, кроме двух. «Журнал уже идёт» при записи файла — две линии страхуют друг друга, снятие обеих ловится; мутант «картинка после правки» не собрался, порядок проверен только прямым тестом.

Не запускались: `@perf`, `gpu`, Firefox и WebKit для автономной сборки; 100 000 узлов и `setOffline` в Firefox (9 пропусков); настоящий прокси с WebSocket; настоящий рестарт Postgres и заново открытый файл после рестарта сервера на томе (стенд на tmpfs, вместо этого обрыв соединений и `pause`); `docker compose up` проекта `ferrum` с нуля, образ amd64; десктоп; память worker'а сжатия отдельно; 50 настоящих браузеров; `compat-old-build.sh` на этой копии (на прототипе только текстовый образец, запуск на прототипе одним коммитом документов раньше); выделение в редакторе при смене имени двойника.

---

## Этапы подсистемы 4

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Сервер и вход (сделан) | Docker, миграции, Better Auth, вход, TypeScript 7, oxlint, prettier | 3, 4, 5 |
| 2. Файлы на сервере (сделан) | команды, проекты, черновики, серверный `FileStore`, роли, «Поделиться», картинки | 6 |
| **3. Синхронизация** (этот план) | журнал, снимки, чужие правки, переподключение | 7, 8 |
| 4. Присутствие | курсоры, аватары, следование | 9 |
| 5. Комментарии | метки, обсуждения, письма | 10 |
| 6. Офлайн и версии | очередь правок, история версий | 11 |
| 7. Десктоп | вход через браузер, серверные файлы | 12 |

## Решения

Где спека молчит, план следует Figma; поведение для пользователя помечено **(поведение)**. По справке Figma ничего не сверялось: это решения по памяти о Figma и из требований этапа. Итог и замеры — в «Сделано (этап 3)» спеки (задача 10).

Расхождения со спекой:

1. **§7.3: документы в памяти сервера.** Спека: «сервер не держит документы на горячем пути». Проба: из 20 000 случайных порч с верной контрольной суммой до 2 % пачек, прошедших разбор и пустой документ, роняют Loro на настоящем документе, а журнал отравил бы файл всем. Поэтому пачка до журнала применяется к настоящему документу в Bun Worker; бюджет 512 МБ (`FERRUM_VALIDATOR_BUDGET`), вытеснение по простою. Паника wasm — отказ этой пачке и новый worker.
2. **§13.1: цель 2 мс на пути «дерево параллельно дереву» не выполнена.** 100 000 узлов: около 50 мс на пачку, первая до 0,7 с, плюс около 0,5 ГБ памяти wasm: Loro обходит всё дерево, это внутри него. Работа идёт в worker'ах, входящие складываются в одну пачку, пока ядро занято (B-309). На остальных путях p95 21–46 мкс.
3. **§13.1: цель 150 мс выдержана не на каждом прогоне.** p50 85–88 мс; p95 по четырём прогонам 102, 112, 153, 210 мс; причина не разобрана (B-327).
4. **§7.5:** `seq` в заголовке контейнера не нужен, подписка идёт по вектору версий. Кэш сцены у неизменённых страниц остаётся.
5. **§7.2:** добавлены кадры `subscribed`, `refused`, `pause`, `ping`/`pong`, `please-reload`; `role` несёт `canShare` и `version` (B-296).

Ядро и отмена:

6. **(поведение)** Узел без типа — половина чужой транзакции: файл открывается, узел появится, когда придёт остальное.
7. **(поведение)** Два живых узла с одним `NodeId` (отмена удаления и параллельный перенос): имя у исходного, иначе у меньшего `TreeID`, другой живёт под своим, ничего не теряется.
8. Отменяются только свои шаги. Операция над исчезнувшим узлом и перенос в цикл пропускаются, пустой шаг — тоже. Дельты отмены текста преобразуются через чужие правки.
9. **(поведение)** Чужая вставка ровно в позиции каретки её не двигает. Повтор возвращает значение на момент отмены (B-326).
10. **(поведение)** Страницу удалил другой — переход на первую с уведомлением; удалены все страницы разом — новая. Новая страница другого загружается сразу.

Сервер:

11. Первый писатель пира его занимает (`FOREIGN_PEER`): подделка заняла бы чужие счётчики, и настоящие правки Loro пропускал бы. Роль проверяется в транзакции каждой группы.
12. Семантика пачек сервером не проверяется: редактор может прислать узел неизвестного типа и перевести файл другим в «только чтение» (B-310). Правки редактора считаются правками; защита — от падения, а не от злого редактора.
13. Снятие роли, ссылки, корзина, снятие из команды закрывают соединение сразу: маршрут сообщает из конвейера API (`afterWrite`), забыть нельзя (B-237). Раз в 30 с сверка на изменения мимо маршрутов. Выход закрывает соединения сессии, «на всех» — человека. Сбой Valkey никого не выгоняет.
14. Запись файла целиком при идущем журнале — `CONFLICT`: вкладка прежней версии не затрёт правки.
15. Сжатие: больше 2000 записей или 32 МБ журнала документа либо 5 минут после ухода последнего. Неприменимое обновление — снимок не заменён, журнал цел, номер записи в журнале сервера, следующая попытка через час. Журнал файла не растёт сверх 1 ГБ (`LOG_FULL`).

Клиент:

16. **(поведение)** Файл целиком пишется один раз, при открытии нового файла, без правок. Опередили, своих правок нет — файл открывается заново; правки уже есть — плашка «Файл уже записан» этапа 2.
17. **(поведение)** Вторая вкладка — такой же участник, запрет снят для файлов на сервере. Журнал браузера снимается подтверждением сервера и только своими или прочитанными при открытии пачками.
18. **(поведение)** «Сохранить» и уход ждут сервер до 5 с; не дождались — вопрос с честной причиной: правки сохранены в браузере и уйдут при следующем открытии.
19. **(поведение)** Предел частоты — `pause` и «Сохранение…», а не «Нет сети». Отказ, который повтор не исправит, — плашка «Правки не сохраняются» с копией на диск, чужие правки при этом идут. Понижение с неподтверждёнными правками — «Правки не дошли до сервера».
20. «Скачать копию» сначала сжимает журнал (`POST …/compact`): иначе копия старше того, что видят люди.

## Структура файлов

| Файл | Ответственность |
|---|---|
| `crates/ferrum-doc/src/{sync,text_ot}.rs` (новые), `document.rs`, `page.rs`, `history.rs` | чужие правки, курсор, отмена рядом с чужими |
| `crates/ferrum-editor/src/remote.rs` (новый) | выделение, жесты, каретка при чужих правках |
| `crates/ferrum-wasm/src/core/sync.rs` (новый), `protocol.rs` | команды и события синхронизации |
| `packages/protocol/src/sync.ts` (новый) | двоичные кадры, пределы, разбор |
| `apps/server/migrations/004_sync.sql` (новый) | `doc_state`, `doc_update`, `doc_peer` |
| `apps/server/src/sync/{update-log,validator,validator.worker,container-read}.ts` (новые) | журнал и проверка пачек |
| `apps/server/src/sync/hub.ts` (новый), `api.ts`, `app.ts`, `auth-routes.ts`, `file-routes.ts` | WebSocket, закрытие при смене прав |
| `apps/server/src/sync/{compact,compact.worker,compactor}.ts` (новые) | сжатие журнала |
| `apps/web/src/core/{sync-client,sync-messages}.ts`, `worker/sync.worker.ts` (новые) | соединение файла в своём worker'е |
| `apps/web/src/ui/{saver,app,chrome,store,root,boot,file-store}`, `server/server-store.ts` | индикатор, «Сохранить», плашки, вкладки |
| `apps/web/e2e-server/sync.spec.ts` (новый), `files.spec.ts`, `auth.spec.ts` | E2E на настоящем стенде |
| `docs/` | спека, запуск, бэклог B-309…B-327 |

---

### Task 1: Ядро — чужие правки и курсор синхронизации

Спека §7, §8. Зеркало и `Changes` по событиям Loro только у задетых узлов, правда — текущее состояние дерева; курсор считает версии независимо от журнала. `sync_report` — замер цели 2 мс.

**Files:**
- Create: `crates/ferrum-doc/examples/sync_report.rs`, `crates/ferrum-doc/src/sync.rs`
- Modify: `crates/ferrum-doc/src/codec.rs`, `crates/ferrum-doc/src/document.rs`, `crates/ferrum-doc/src/lib.rs`, `crates/ferrum-doc/src/page.rs`
- Test: `crates/ferrum-doc/tests/remote.rs` (новый)

**Interfaces:**
- Produces: `Document::{apply_remote, sync_start, sync_reset, take_sync_subscribes, sync_subscribed, take_sync_updates, sync_acked, sync_versions, sync_covers, sync_unacked, set_catches_up}`, типы `DocKey`, `SyncVersion`, `SyncBatch`, `TextShift`, `RemoteOutcome`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-doc/tests/remote.rs`:

````rust
//! Чужие правки (спека подсистемы 4, раздел 8): зеркало обновляется по изменённым узлам и совпадает с
//! полной сборкой из Loro при любом порядке и повторах пачек; курсор
//! синхронизации шлёт только своё и считает версии независимо от журнала.

use std::collections::HashMap;

use ferrum_doc::container::{decode, encode_full};
use ferrum_doc::*;
use proptest::prelude::*;

/// Две вкладки одного файла: общий снимок, у каждой свой пир.
fn pair() -> (Document, Document, PageId) {
    let doc = Document::new();
    let bytes = encode_full(&doc).unwrap();
    let mut a = decode(&bytes).unwrap().document;
    let mut b = decode(&bytes).unwrap().document;
    let page = a.pages()[0].0;
    for d in [&mut a, &mut b] {
        d.sync_start();
        d.page(page).unwrap();
        let subs = d.take_sync_subscribes();
        for (k, v) in subs {
            d.sync_subscribed(k, v);
        }
    }
    (a, b, page)
}

fn deliver(from: &mut Document, to: &mut [&mut Document]) -> Vec<RemoteOutcome> {
    let mut out = Vec::new();
    for batch in from.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
        for d in to.iter_mut() {
            out.push(d.apply_remote(batch.doc, &[&batch.bytes]).unwrap());
        }
        from.sync_acked(batch.doc, &batch.end);
    }
    out
}

fn sync(a: &mut Document, b: &mut Document) {
    deliver(a, &mut [&mut *b]);
    deliver(b, &mut [&mut *a]);
}

fn rect(doc: &mut Document, page: PageId) -> NodeId {
    doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
    })
    .unwrap()[0]
}

fn set_x(doc: &mut Document, page: PageId, id: NodeId, x: f64) {
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                x: Some(x),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    })
    .unwrap();
}

/// Узлы страницы: родитель, позиция и свойства — всё, что читает сцена.
fn nodes(doc: &mut Document, page: PageId) -> HashMap<NodeId, (Option<NodeId>, Props)> {
    let p = doc.page(page).unwrap();
    p.walk()
        .into_iter()
        .map(|n| (n, (p.parent(n), p.props(n).unwrap())))
        .collect()
}

/// Зеркало совпадает с тем, что строится из Loro с нуля.
fn assert_mirror(doc: &mut Document, page: PageId) {
    let snapshot = doc.page(page).unwrap().export_full_snapshot().unwrap();
    let fresh = PageDoc::from_snapshot(&snapshot).unwrap();
    let want: HashMap<_, _> = fresh
        .walk()
        .into_iter()
        .map(|n| (n, (fresh.parent(n), fresh.props(n).unwrap())))
        .collect();
    assert_eq!(
        nodes(doc, page),
        want,
        "зеркало расходится с полной сборкой"
    );
}

#[test]
fn foreign_create_edit_move_delete_reach_mirror_and_changes() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    let group = rect(&mut a, page);
    sync(&mut a, &mut b);
    let c = b.take_changes(page);
    assert!(
        c.created.contains(&id) && c.created.contains(&group),
        "{c:?}"
    );
    assert_eq!(
        b.page(page).unwrap().props(id).unwrap().kind,
        NodeKind::Rect
    );

    set_x(&mut a, page, id, 42.0);
    sync(&mut a, &mut b);
    let c = b.take_changes(page);
    assert_eq!(c.props, vec![id], "изменён только этот узел");
    assert_eq!(b.page(page).unwrap().props(id).unwrap().x, 42.0);

    a.transact(page, vec![], |tx| {
        tx.move_node(id, None, 1)?;
        Ok(vec![id])
    })
    .unwrap();
    sync(&mut a, &mut b);
    assert!(b.take_changes(page).moved.contains(&id));
    assert_eq!(b.page(page).unwrap().children(None)[1], id);

    a.transact(page, vec![], |tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
    sync(&mut a, &mut b);
    assert!(b.take_changes(page).deleted.contains(&id));
    assert!(!b.page(page).unwrap().contains(id));
    assert_mirror(&mut b, page);
}

#[test]
fn deleting_a_frame_removes_descendants_from_mirror() {
    let (mut a, mut b, page) = pair();
    let (frame, kid) = {
        let ids = a
            .transact(page, vec![], |tx| {
                let f = tx.create_node(None, None, NodeKind::Frame)?;
                Ok(vec![f, tx.create_node(Some(f), None, NodeKind::Rect)?])
            })
            .unwrap();
        (ids[0], ids[1])
    };
    sync(&mut a, &mut b);
    b.take_changes(page);
    a.transact(page, vec![], |tx| {
        tx.delete_nodes(&[frame])?;
        Ok(vec![])
    })
    .unwrap();
    sync(&mut a, &mut b);
    let c = b.take_changes(page);
    assert!(
        c.deleted.contains(&frame) && c.deleted.contains(&kid),
        "{c:?}"
    );
    assert_mirror(&mut b, page);
}

#[test]
fn text_shift_moves_caret_with_foreign_edits() {
    let (mut a, mut b, page) = pair();
    let id = a
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Text)?])
        })
        .unwrap()[0];
    a.transact(page, vec![id], |tx| {
        tx.insert_text(id, 0, "абвгд", None)?;
        Ok(vec![id])
    })
    .unwrap();
    sync(&mut a, &mut b);
    b.transact(page, vec![id], |tx| {
        tx.insert_text(id, 1, "XY", None)?;
        tx.delete_text(id, 5..6)?;
        Ok(vec![id])
    })
    .unwrap();
    let got = deliver(&mut b, &mut [&mut a]);
    let shifts: Vec<&TextShift> = got
        .iter()
        .flat_map(|o| o.text.iter().map(|(_, s)| s))
        .collect();
    assert_eq!(shifts.len(), 1);
    let s = shifts[0];
    assert_eq!(s.map(0), 0);
    assert_eq!(
        s.map(1),
        1,
        "вставка ровно в позиции курсора его не двигает"
    );
    assert_eq!(s.map(2), 4);
    assert_eq!(s.map(5), 6);
}

#[test]
fn foreign_page_add_and_remove_update_document_and_forget_steps() {
    let (mut a, mut b, _) = pair();
    let added = a.add_page("Вторая").unwrap();
    let got = deliver(&mut a, &mut [&mut b]);
    assert!(got.iter().any(|o| o.added_pages == vec![added]), "{got:?}");
    assert_eq!(b.pages().len(), 2);
    let subs = b.take_sync_subscribes();
    assert!(
        subs.iter().any(|(k, _)| *k == DocKey::Page(added)),
        "подписка на новую страницу"
    );

    let own = rect(&mut b, added);
    assert!(b.can_undo());
    let _ = own;
    a.delete_page(added).unwrap();
    let got = deliver(&mut a, &mut [&mut b]);
    assert!(got.iter().any(|o| o.removed_pages == vec![added]));
    assert_eq!(b.pages().len(), 1);
    assert!(!b.can_undo(), "шаги удалённой страницы забыты");
}

#[test]
fn viewer_applies_foreign_edits_and_sends_nothing() {
    let (mut a, mut b, page) = pair();
    b.set_view_only(true);
    let id = rect(&mut a, page);
    sync(&mut a, &mut b);
    assert!(b.page(page).unwrap().contains(id));
    assert!(b
        .take_sync_updates(MAX_SYNC_BATCH_BYTES)
        .unwrap()
        .is_empty());
    assert!(!b.sync_unacked());
}

#[test]
fn newer_schema_from_other_becomes_read_only_but_keeps_applying() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    a.force_schema_version_for_tests(SCHEMA_VERSION + 1);
    let got = deliver(&mut a, &mut [&mut b]);
    assert!(got.iter().any(|o| o.became_read_only), "{got:?}");
    assert!(b.is_read_only());
    assert!(
        b.page(page).unwrap().contains(id),
        "чужие правки применяются"
    );
}

#[test]
fn unknown_kind_from_other_makes_read_only_with_rebuild() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    sync(&mut a, &mut b);
    b.take_changes(page);
    a.page(page).unwrap().force_kind_for_tests(id, "star");
    let got = deliver(&mut a, &mut [&mut b]);
    let o = got.iter().find(|o| o.page == Some(page)).unwrap();
    assert!(o.rebuilt && o.became_read_only, "{o:?}");
    assert!(
        !b.page(page).unwrap().contains(id),
        "скрыт от модели, в Loro остался"
    );
    let later = PageDoc::from_snapshot(&b.page(page).unwrap().export_full_snapshot().unwrap());
    assert!(later.unwrap().has_unknown_nodes());
}

#[test]
fn broken_bytes_are_reported_without_harm() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    sync(&mut a, &mut b);
    let o = b
        .apply_remote(DocKey::Page(page), &[b"not a loro blob"])
        .unwrap();
    assert!(o.failed);
    assert!(b.page(page).unwrap().contains(id));
    assert_mirror(&mut b, page);
}

#[test]
fn update_without_its_dependencies_waits_and_applies_later() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    let first = a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap();
    set_x(&mut a, page, id, 7.0);
    let second = a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap();
    let doc = DocKey::Page(page);
    let f = first.iter().find(|x| x.doc == doc).unwrap();
    let s = second.iter().find(|x| x.doc == doc).unwrap();
    let o = b.apply_remote(doc, &[&s.bytes]).unwrap();
    assert!(o.pending);
    assert!(!b.page(page).unwrap().contains(id));
    b.apply_remote(doc, &[&f.bytes]).unwrap();
    assert_eq!(b.page(page).unwrap().props(id).unwrap().x, 7.0);
    assert_mirror(&mut b, page);
}

#[test]
fn cursor_sends_only_unsent_and_resends_after_reconnect() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    let batches = a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap();
    assert!(!batches.is_empty());
    assert!(
        a.take_sync_updates(MAX_SYNC_BATCH_BYTES)
            .unwrap()
            .is_empty(),
        "второй раз нечего"
    );
    assert!(a.sync_unacked());
    // Обрыв до ack: подписка заново, сервер сообщает прежнюю версию, правка уходит снова.
    a.sync_reset();
    let old = SyncVersion::default();
    for (k, _) in a.take_sync_subscribes() {
        a.sync_subscribed(
            k,
            if k == DocKey::Root {
                SyncVersion::default()
            } else {
                old.clone()
            },
        );
    }
    let again = a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap();
    assert!(again.iter().any(|x| x.doc == DocKey::Page(page)));
    for x in &again {
        // Повтор безвреден: Loro пропускает известное.
        b.apply_remote(x.doc, &[&x.bytes]).unwrap();
        b.apply_remote(x.doc, &[&x.bytes]).unwrap();
        a.sync_acked(x.doc, &x.end);
    }
    assert!(b.page(page).unwrap().contains(id));
    assert!(!a.sync_unacked());
    assert_mirror(&mut b, page);
}

#[test]
fn foreign_updates_are_not_sent_back() {
    let (mut a, mut b, page) = pair();
    rect(&mut a, page);
    sync(&mut a, &mut b);
    assert!(
        b.take_sync_updates(MAX_SYNC_BATCH_BYTES)
            .unwrap()
            .is_empty(),
        "чужое не шлём"
    );
    assert!(!b.sync_unacked());
}

#[test]
fn journal_marks_are_covered_only_after_ack() {
    let (mut a, _b, page) = pair();
    rect(&mut a, page);
    let batches = a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap();
    let marks = a.sync_versions();
    assert!(!a.sync_covers(&marks));
    for x in &batches {
        a.sync_acked(x.doc, &x.end);
    }
    assert!(a.sync_covers(&marks));
}

#[test]
fn big_edit_is_split_under_the_limit_and_converges() {
    let (mut a, mut b, page) = pair();
    a.transact(page, vec![], |tx| {
        for _ in 0..2000 {
            tx.create_node(None, None, NodeKind::Rect)?;
        }
        Ok(vec![])
    })
    .unwrap();
    let limit = 16 * 1024;
    let batches = a.take_sync_updates(limit).unwrap();
    assert!(batches.len() > 1);
    assert!(batches.iter().all(|x| x.bytes.len() <= limit));
    for x in &batches {
        b.apply_remote(x.doc, &[&x.bytes]).unwrap();
        a.sync_acked(x.doc, &x.end);
        // Снимок между кусками (сжатие на сервере): половина транзакции файл не ломает.
        assert_mirror(&mut b, page);
    }
    assert_eq!(nodes(&mut a, page), nodes(&mut b, page));
    assert_mirror(&mut b, page);
    assert!(!a.sync_unacked());
}

#[test]
fn version_wire_format_round_trips_and_rejects_garbage() {
    let (mut a, _b, page) = pair();
    rect(&mut a, page);
    let v = a.sync_versions();
    let (_, version) = v.iter().find(|(k, _)| *k == DocKey::Page(page)).unwrap();
    let bytes = version.encode();
    assert_eq!(SyncVersion::decode(&bytes).unwrap(), *version);
    assert!(SyncVersion::decode(&bytes[..bytes.len() - 1]).is_none());
    assert!(SyncVersion::decode(&u32::MAX.to_le_bytes()).is_none());
    let k = DocKey::Page(page);
    assert_eq!(DocKey::parse(&k.to_string()), Some(k));
    assert_eq!(DocKey::parse("root"), Some(DocKey::Root));
    assert_eq!(DocKey::parse("../etc"), None);
}

#[test]
fn concurrent_revive_and_restore_converge_with_rebuild() {
    // a удаляет узел и отменяет удаление (новый `TreeID` с прежним `NodeId`), а b параллельно его переносит:
    // перенос может оживить старый узел. У обоих — одно и то же, ничего не потеряно.
    for _ in 0..8 {
        let (mut a, mut b, page) = pair();
        let id = rect(&mut a, page);
        let other = rect(&mut a, page);
        sync(&mut a, &mut b);
        a.transact(page, vec![], |tx| {
            tx.delete_nodes(&[id])?;
            Ok(vec![])
        })
        .unwrap();
        a.undo().unwrap();
        b.transact(page, vec![], |tx| {
            tx.move_node(id, None, 5)?;
            Ok(vec![])
        })
        .unwrap();
        b.transact(page, vec![], |tx| {
            tx.move_node(other, None, 0)?;
            Ok(vec![])
        })
        .unwrap();
        sync(&mut a, &mut b);
        sync(&mut a, &mut b);
        assert_mirror(&mut a, page);
        assert_mirror(&mut b, page);
        assert_eq!(nodes(&mut a, page), nodes(&mut b, page));
    }
}

#[derive(Debug, Clone)]
enum Op {
    Create {
        who: usize,
        parent: usize,
    },
    Move {
        who: usize,
        node: usize,
        parent: usize,
        index: usize,
    },
    Delete {
        who: usize,
        node: usize,
    },
    SetX {
        who: usize,
        node: usize,
        x: i32,
    },
    /// Доставка: пачки в перемешанном порядке и с повторами.
    Sync {
        seed: u64,
    },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (0usize..3, 0usize..40).prop_map(|(who, parent)| Op::Create { who, parent }),
        2 => (0usize..3, 0usize..40, 0usize..40, 0usize..6)
            .prop_map(|(who, node, parent, index)| Op::Move { who, node, parent, index }),
        1 => (0usize..3, 0usize..40).prop_map(|(who, node)| Op::Delete { who, node }),
        3 => (0usize..3, 0usize..40, -500i32..500).prop_map(|(who, node, x)| Op::SetX { who, node, x }),
        3 => any::<u64>().prop_map(|seed| Op::Sync { seed }),
    ]
}

fn shuffle<T>(v: &mut [T], mut seed: u64) {
    for i in (1..v.len()).rev() {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        v.swap(i, (seed >> 33) as usize % (i + 1));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Спека, раздел 13.2: случайные правки нескольких клиентов с перестановкой и повтором пачек; все приходят к одному дереву, зеркало совпадает с полной сборкой.
    #[test]
    fn random_edits_converge_and_mirror_matches_rebuild(ops in proptest::collection::vec(op(), 1..70)) {
        let doc = Document::new();
        let bytes = encode_full(&doc).unwrap();
        let mut docs: Vec<Document> = (0..3).map(|_| decode(&bytes).unwrap().document).collect();
        let page = docs[0].pages()[0].0;
        for d in &mut docs {
            d.sync_start();
            d.page(page).unwrap();
            for (k, v) in d.take_sync_subscribes() {
                d.sync_subscribed(k, v);
            }
        }
        let mut log: Vec<(usize, DocKey, Vec<u8>)> = Vec::new();
        let pick = |d: &mut Document, n: usize| {
            let all = d.page(page).unwrap().walk();
            if all.is_empty() { None } else { Some(all[n % all.len()]) }
        };
        for o in &ops {
            match o {
                Op::Create { who, parent } => {
                    let d = &mut docs[*who];
                    let p = pick(d, *parent).filter(|n| d.page(page).unwrap().kind(*n).unwrap() == NodeKind::Frame);
                    let kind = if parent % 2 == 0 { NodeKind::Frame } else { NodeKind::Rect };
                    let _ = d.transact(page, vec![], |tx| Ok(vec![tx.create_node(p, None, kind)?]));
                }
                Op::Move { who, node, parent, index } => {
                    let d = &mut docs[*who];
                    let (Some(n), p) = (pick(d, *node), pick(d, *parent)) else { continue };
                    let p = p.filter(|x| d.page(page).unwrap().kind(*x).unwrap() == NodeKind::Frame);
                    let _ = d.transact(page, vec![], |tx| { tx.move_node(n, p, *index)?; Ok(vec![n]) });
                }
                Op::Delete { who, node } => {
                    let d = &mut docs[*who];
                    let Some(n) = pick(d, *node) else { continue };
                    let _ = d.transact(page, vec![], |tx| { tx.delete_nodes(&[n])?; Ok(vec![]) });
                }
                Op::SetX { who, node, x } => {
                    let d = &mut docs[*who];
                    let Some(n) = pick(d, *node) else { continue };
                    let _ = d.transact(page, vec![], |tx| {
                        tx.set_props(n, &PropsPatch { x: Some(*x as f64), ..Default::default() })?;
                        Ok(vec![n])
                    });
                }
                Op::Sync { seed } => {
                    for (who, d) in docs.iter_mut().enumerate() {
                        for b in d.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                            log.push((who, b.doc, b.bytes));
                        }
                    }
                    let mut part: Vec<_> = log.clone();
                    shuffle(&mut part, *seed);
                    part.truncate(part.len() / 2 + 1);
                    for (who, doc, bytes) in part {
                        let to = (who + 1 + (*seed as usize % 2)) % 3;
                        docs[to].apply_remote(doc, &[&bytes]).unwrap();
                    }
                }
            }
            for d in &mut docs {
                assert_mirror(d, page);
            }
        }
        // Финал: всё всем в обычном порядке и ещё раз с повторами.
        for (who, d) in docs.iter_mut().enumerate() {
            for b in d.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                log.push((who, b.doc, b.bytes));
            }
        }
        for round in 0..2 {
            let _ = round;
            for (_, doc, bytes) in &log {
                for d in &mut docs {
                    d.apply_remote(*doc, &[bytes]).unwrap();
                }
            }
        }
        let want = nodes(&mut docs[0], page);
        for d in &mut docs {
            assert_mirror(d, page);
            prop_assert_eq!(&nodes(d, page), &want);
        }
    }
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc --test remote`
Expected: FAIL — не собирается: нет `apply_remote` и курсора, 75 ошибок компиляции

- [ ] **Step 3: Реализация**

Создать `crates/ferrum-doc/examples/sync_report.rs`:

````rust
//! Контрольный замер чужих правок (спека подсистемы 4, раздел 13.1):
//! `cargo run --release -p ferrum-doc --example sync_report`. Страница из 100 000 узлов: пачка из одной
//! чужой правки вместе с обновлением зеркала и `take_changes`. Код выхода 1 при провале цели.

use std::time::{Duration, Instant};

use ferrum_doc::container::{decode, encode_full};
use ferrum_doc::fixture::fill;
use ferrum_doc::*;

const NODES: usize = 100_000;
const TARGET: Duration = Duration::from_millis(2);

fn percentile(v: &mut [Duration], p: usize) -> Duration {
    v.sort();
    v[(v.len() * p).div_ceil(100).max(1) - 1]
}

fn open(bytes: &[u8], page: PageId) -> Document {
    let mut d = decode(bytes).unwrap().document;
    d.sync_start();
    d.page(page).unwrap();
    for (k, v) in d.take_sync_subscribes() {
        d.sync_subscribed(k, v);
    }
    d
}

struct Scene {
    a: Document,
    b: Document,
    page: PageId,
    ids: Vec<NodeId>,
}

fn scene() -> Scene {
    let mut seed = Document::new();
    let page = seed.pages()[0].0;
    let ids = seed.transact(page, vec![], |tx| fill(tx, NODES)).unwrap();
    let bytes = encode_full(&seed).unwrap();
    println!("файл: {:.1} МБ", bytes.len() as f64 / 1e6);
    let a = open(&bytes, page);
    let b = open(&bytes, page);
    Scene { a, b, page, ids }
}

fn one(
    s: &mut Scene,
    name: &str,
    target: Option<Duration>,
    runs: usize,
    edit: impl Fn(&mut Document, PageId, &[NodeId], usize),
) -> bool {
    let mut times = Vec::with_capacity(runs);
    let mut first = Duration::ZERO;
    for i in 0..runs {
        edit(&mut s.a, s.page, &s.ids, i);
        let mut applied = Duration::ZERO;
        for batch in s.a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
            let t = Instant::now();
            s.b.apply_remote(batch.doc, &[&batch.bytes]).unwrap();
            s.b.take_changes(s.page);
            applied += t.elapsed();
            s.a.sync_acked(batch.doc, &batch.end);
        }
        if i == 0 {
            first = applied;
        }
        times.push(applied);
    }
    let (p50, p95, max) = (
        percentile(&mut times, 50),
        percentile(&mut times, 95),
        percentile(&mut times, 100),
    );
    let ok = target.is_none_or(|t| p95 <= t);
    println!(
        "{} {name}: первая {first:.2?}, p50 {p50:.2?}, p95 {p95:.2?}, наибольшая {max:.2?}{}",
        if ok { "PASS" } else { "FAIL" },
        target
            .map(|t| format!(" (цель ≤ {t:?})"))
            .unwrap_or_default(),
    );
    ok
}

fn set_x(d: &mut Document, page: PageId, id: NodeId, x: f64) {
    d.transact(page, vec![], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                x: Some(x),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    })
    .unwrap();
}

fn main() {
    let mut s = scene();
    let mut ok = true;
    ok &= one(
        &mut s,
        "свойство чужого узла",
        Some(TARGET),
        500,
        |d, p, ids, i| {
            set_x(d, p, ids[(i * 37) % ids.len()], i as f64 + 0.5);
        },
    );
    ok &= one(
        &mut s,
        "новый узел",
        Some(TARGET),
        200,
        |d, p, ids, _| {
            let frame = ids[50_000];
            d.transact(p, vec![], |tx| {
                Ok(vec![tx.create_node(Some(frame), None, NodeKind::Rect)?])
            })
            .unwrap();
        },
    );
    ok &= one(
        &mut s,
        "перенос к другому родителю",
        Some(TARGET),
        200,
        |d, p, ids, i| {
            let node = ids[1 + (i % 50)];
            let frame = ids[(i % 2 + 1) * 100];
            d.transact(p, vec![], |tx| {
                tx.move_node(node, Some(frame), 0)?;
                Ok(vec![node])
            })
            .unwrap();
        },
    );
    ok &= one(
        &mut s,
        "удаление узла",
        Some(TARGET),
        200,
        |d, p, ids, i| {
            let node = ids[70_001 + i];
            d.transact(p, vec![], |tx| {
                tx.delete_nodes(&[node])?;
                Ok(vec![])
            })
            .unwrap();
        },
    );
    // Своя правка свойства рядом: Loro сливает без отката.
    {
        let mut times = Vec::new();
        let (page, ids) = (s.page, s.ids.clone());
        for i in 0..200 {
            set_x(&mut s.b, page, ids[(i * 13) % ids.len()], i as f64 + 0.25);
            set_x(
                &mut s.a,
                page,
                ids[(i * 17 + 3) % ids.len()],
                i as f64 + 0.75,
            );
            for batch in s.a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                let t = Instant::now();
                s.b.apply_remote(batch.doc, &[&batch.bytes]).unwrap();
                s.b.take_changes(page);
                times.push(t.elapsed());
                s.a.sync_acked(batch.doc, &batch.end);
            }
            for batch in s.b.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                s.a.apply_remote(batch.doc, &[&batch.bytes]).unwrap();
                s.a.take_changes(page);
                s.b.sync_acked(batch.doc, &batch.end);
            }
        }
        let (p50, p95) = (percentile(&mut times, 50), percentile(&mut times, 95));
        let pass = p95 <= TARGET;
        ok &= pass;
        println!("{} свойство параллельно своей правке свойства: p50 {p50:.2?}, p95 {p95:.2?} (цель ≤ {TARGET:?})", if pass { "PASS" } else { "FAIL" });
    }
    // Правки дерева с обеих сторон: Loro откатывает всё дерево. Цель недостижима, цифра — для описания PR.
    {
        let mut times = Vec::new();
        let (page, ids) = (s.page, s.ids.clone());
        for i in 0..20 {
            let frame = ids[20_000];
            s.b.transact(page, vec![], |tx| {
                Ok(vec![tx.create_node(Some(frame), None, NodeKind::Rect)?])
            })
            .unwrap();
            s.a.transact(page, vec![], |tx| {
                Ok(vec![tx.create_node(
                    Some(frame),
                    Some(i),
                    NodeKind::Rect,
                )?])
            })
            .unwrap();
            for batch in s.a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                let t = Instant::now();
                s.b.apply_remote(batch.doc, &[&batch.bytes]).unwrap();
                s.b.take_changes(page);
                times.push(t.elapsed());
                s.a.sync_acked(batch.doc, &batch.end);
            }
            for batch in s.b.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                s.a.apply_remote(batch.doc, &[&batch.bytes]).unwrap();
                s.a.take_changes(page);
                s.b.sync_acked(batch.doc, &batch.end);
            }
        }
        let first = times[0];
        let (p50, max) = (percentile(&mut times, 50), percentile(&mut times, 100));
        println!("ВНЕ ЦЕЛИ правка дерева параллельно своей правке дерева: первая {first:.2?}, p50 {p50:.2?}, наибольшая {max:.2?}");
    }
    // Цена подписки на своей правке: страница без подписки против страницы с ней.
    {
        let mut plain = Document::new();
        let p = plain.pages()[0].0;
        let t = Instant::now();
        plain.transact(p, vec![], |tx| fill(tx, 10_000)).unwrap();
        let without = t.elapsed();
        let mut heard = Document::new();
        heard.sync_start();
        let p = heard.pages()[0].0;
        let t = Instant::now();
        heard.transact(p, vec![], |tx| fill(tx, 10_000)).unwrap();
        println!(
            "своя большая правка, 10 000 узлов: без подписки {without:.1?}, с подпиской {:.1?}",
            t.elapsed()
        );
    }
    if !ok {
        std::process::exit(1);
    }
}
````

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
/// Хэши изображений в заливках узла — без разбора остальных свойств и типа узла:
/// так читаются и узлы неизвестного типа, скрытые от модели.
pub(crate) fn read_image_refs(map: &LoroMap) -> Vec<String> {
    match value(map, key::FILLS) {
````

на:

````rust
/// Хэши изображений в заливках узла — без разбора остальных свойств и типа узла:
/// так читаются и узлы неизвестного типа, скрытые от модели.
/// Записан ли тип узла. Нет — половина чужой транзакции: узел создан, свойства ещё в пути.
pub(crate) fn has_kind(map: &LoroMap) -> bool {
    value(map, key::TYPE).is_some()
}

pub(crate) fn read_image_refs(map: &LoroMap) -> Vec<String> {
    match value(map, key::FILLS) {
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
use crate::page::{Inverse, PageDoc, Tx};
use crate::scene_cache::SceneRow;
use crate::types::{Color, NodeId, PageId, PropsPatch};

````

на:

````rust
use crate::page::{Inverse, PageDoc, Tx};
use crate::scene_cache::SceneRow;
use crate::sync::{
    self, Cursor, DocKey, RemoteOutcome, SyncBatch, SyncCursors, SyncVersion, TextShift,
};
use crate::types::{Color, NodeId, PageId, PropsPatch};

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    /// у фрейма то же самое видно по счётчику сцены.
    guides_revision: u64,
}

````

на:

````rust
    /// у фрейма то же самое видно по счётчику сцены.
    guides_revision: u64,
    /// Файл на сервере: курсоры синхронизации. `None` — локальный файл, чужих правок не бывает.
    sync: Option<SyncCursors>,
    /// Файл на сервере: журналу не хватает правок, которые ещё придут с сервера, это не повреждение.
    catches_up: bool,
}

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            parked: HashMap::new(),
            guides_revision: 0,
        }
    }
````

на:

````rust
            parked: HashMap::new(),
            guides_revision: 0,
            sync: None,
            catches_up: false,
        }
    }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            _ => SCHEMA_VERSION,
        }
    }

````

на:

````rust
            _ => SCHEMA_VERSION,
        }
    }

    /// Только для тестов: версия схемы, как будто корень правит более новая сборка.
    #[doc(hidden)]
    pub fn force_schema_version_for_tests(&mut self, version: u32) {
        self.force_schema_version(version);
    }

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            // Загруженное из файла уже в нём: журнал пишет правки после загрузки.
            self.journaled_pages.insert(id, page.oplog_vv());
            self.loaded.insert(id, page);
        }
````

на:

````rust
            // Загруженное из файла уже в нём: журнал пишет правки после загрузки.
            self.journaled_pages.insert(id, page.oplog_vv());
            if self.sync.is_some() {
                page.listen();
            }
            self.loaded.insert(id, page);
        }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        for data in &root {
            match self.root.import(data) {
                Ok(status) => lost |= status.pending.is_some(),
                Err(_) => lost = true,
            }
````

на:

````rust
        for data in &root {
            match self.root.import(data) {
                Ok(status) => lost |= status.pending.is_some() && !self.catches_up,
                Err(_) => lost = true,
            }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            let refs: Vec<&[u8]> = list.iter().map(Vec::as_slice).collect();
            match page.import_updates(&refs) {
                Ok(complete) => lost |= !complete,
                Err(_) => lost = true,
            }
````

на:

````rust
            let refs: Vec<&[u8]> = list.iter().map(Vec::as_slice).collect();
            match page.import_updates(&refs) {
                // Файл на сервере: Loro держит такие правки у себя и применит, когда придут предыдущие.
                Ok(complete) => lost |= !complete && !self.catches_up,
                Err(_) => lost = true,
            }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        self.guides_revision += 1;
        out
    }

````

на:

````rust
        self.guides_revision += 1;
        out
    }

    /// Файл на сервере, ещё до применения журнала: неполный журнал — не повреждение.
    pub fn set_catches_up(&mut self) {
        self.catches_up = true;
    }

    /// Включает синхронизацию: зеркала страниц начинают слушать чужие правки, а на следующем
    /// `take_sync_subscribes` — подписка на корень и все загруженные страницы.
    pub fn sync_start(&mut self) {
        self.catches_up = true;
        if self.sync.is_none() {
            self.sync = Some(SyncCursors::default());
        }
        for page in self.loaded.values() {
            page.listen();
        }
    }

    /// Соединение оборвалось: на новом всё начинается с подписки, а неподтверждённое уйдёт снова.
    pub fn sync_reset(&mut self) {
        if let Some(s) = &mut self.sync {
            s.subscribed.clear();
            for c in s.docs.values_mut() {
                c.ready = false;
            }
        }
    }

    /// Документы, на которые ещё нет подписки, с их версиями. Незагруженной страницы здесь нет:
    /// подписка — когда её откроют, её снимок потом догонит журнал.
    pub fn take_sync_subscribes(&mut self) -> Vec<(DocKey, SyncVersion)> {
        let Some(s) = &mut self.sync else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if s.subscribed.insert(DocKey::Root) {
            out.push((DocKey::Root, SyncVersion(self.root.oplog_vv())));
        }
        for (id, page) in &self.loaded {
            if s.subscribed.insert(DocKey::Page(*id)) {
                out.push((DocKey::Page(*id), SyncVersion(page.oplog_vv())));
            }
        }
        out
    }

    /// Сервер ответил на подписку: всё сверх его версии надо отправить.
    pub fn sync_subscribed(&mut self, doc: DocKey, server: SyncVersion) {
        if let Some(s) = &mut self.sync {
            s.docs.insert(
                doc,
                Cursor {
                    ready: true,
                    sent: server.0.clone(),
                    acked: server.0,
                },
            );
        }
    }

    /// Свои правки, которых сервер ещё не получал, кусками до `limit` байт. Просмотр и файл только для
    /// чтения правок не шлют, что бы ни было в журнале документа.
    pub fn take_sync_updates(&mut self, limit: usize) -> DocResult<Vec<SyncBatch>> {
        if self.is_read_only() {
            return Ok(Vec::new());
        }
        let Some(s) = &mut self.sync else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for (key, c) in s.docs.iter_mut().filter(|(_, c)| c.ready) {
            let doc = match key {
                DocKey::Root => Some(self.root.clone()),
                DocKey::Page(id) => self.loaded.get(id).map(|p| p.loro().clone()),
            };
            let Some(doc) = doc else { continue };
            let vv = doc.oplog_vv();
            if c.sent.includes_vv(&vv) {
                continue;
            }
            for (bytes, end) in sync::split_updates(&doc, &c.sent, limit)? {
                out.push(SyncBatch {
                    doc: *key,
                    bytes,
                    end: SyncVersion(end),
                });
            }
            sync::merge(&mut c.sent, &vv);
        }
        Ok(out)
    }

    /// Сервер записал пачку на диск базы.
    pub fn sync_acked(&mut self, doc: DocKey, end: &SyncVersion) {
        if let Some(c) = self.sync.as_mut().and_then(|s| s.docs.get_mut(&doc)) {
            sync::merge(&mut c.acked, &end.0);
        }
    }

    /// Текущие версии документов с подпиской: отметка пачки журнала, которую потом можно снять.
    pub fn sync_versions(&self) -> Vec<(DocKey, SyncVersion)> {
        let Some(s) = &self.sync else {
            return Vec::new();
        };
        s.docs
            .keys()
            .filter_map(|k| match k {
                DocKey::Root => Some((*k, SyncVersion(self.root.oplog_vv()))),
                DocKey::Page(id) => self.loaded.get(id).map(|p| (*k, SyncVersion(p.oplog_vv()))),
            })
            .collect()
    }

    /// Сервер подтвердил всё, что было в документе на момент отметки.
    pub fn sync_covers(&self, marks: &[(DocKey, SyncVersion)]) -> bool {
        let Some(s) = &self.sync else {
            return false;
        };
        marks.iter().all(|(k, v)| {
            // Документа больше нет (страницу удалили): его правки ждать незачем.
            let gone = matches!(k, DocKey::Page(id) if !self.loaded.contains_key(id));
            gone || s.docs.get(k).is_some_and(|c| c.acked.includes_vv(&v.0))
        })
    }

    /// Есть правки, которых сервер не подтвердил (или ещё не подключались).
    pub fn sync_unacked(&self) -> bool {
        let Some(s) = &self.sync else {
            return false;
        };
        let behind = |key: DocKey, vv: &VersionVector| match s.docs.get(&key) {
            Some(c) => !c.acked.includes_vv(vv),
            None => false,
        };
        behind(DocKey::Root, &self.root.oplog_vv())
            || self
                .loaded
                .iter()
                .any(|(id, p)| behind(DocKey::Page(*id), &p.oplog_vv()))
    }

    /// Применяет пачку чужих правок документа `doc` (спека подсистемы 4, раздел 8). Шаги отмены
    /// остаются, зеркало обновляется по изменённым узлам, изменения — обычным путём `take_changes`.
    /// Работает и в режиме просмотра, и в файле только для чтения: правки других применяются всегда.
    pub fn apply_remote(&mut self, doc: DocKey, batches: &[&[u8]]) -> DocResult<RemoteOutcome> {
        let mut out = match doc {
            DocKey::Root => self.apply_remote_root(batches),
            DocKey::Page(id) => self.apply_remote_page(id, batches)?,
        };
        // Принесённое сервером отправлять незачем, и оно уже подтверждено.
        if let Some(c) = self.sync.as_mut().and_then(|s| s.docs.get_mut(&doc)) {
            for bytes in batches {
                let end = sync::blob_end(bytes);
                sync::merge(&mut c.sent, &end);
                sync::merge(&mut c.acked, &end);
            }
        }
        out.missing_images.sort();
        out.missing_images.dedup();
        Ok(out)
    }

    fn apply_remote_root(&mut self, batches: &[&[u8]]) -> RemoteOutcome {
        let mut out = RemoteOutcome {
            root_changed: true,
            ..RemoteOutcome::default()
        };
        let was_read_only = self.is_read_only();
        for data in batches {
            match self.root.import(data) {
                Ok(status) => out.pending |= status.pending.is_some(),
                Err(_) => out.failed = true,
            }
        }
        let listed: Vec<PageId> = self.pages().into_iter().map(|(id, _)| id).collect();
        for id in &listed {
            if !self.loaded.contains_key(id) && !self.unloaded.contains_key(id) {
                // Страница другого участника: содержимое придёт по подписке, загружать нечего.
                let page = PageDoc::new();
                if self.sync.is_some() {
                    page.listen();
                }
                self.loaded.insert(*id, page);
                self.journaled_pages.insert(*id, VersionVector::default());
                out.added_pages.push(*id);
            }
        }
        let known: Vec<PageId> = self
            .loaded
            .keys()
            .chain(self.unloaded.keys())
            .copied()
            .collect();
        for id in known {
            if !listed.contains(&id) {
                self.loaded.remove(&id);
                self.unloaded.remove(&id);
                self.changes.remove(&id);
                self.journaled_pages.remove(&id);
                for orphan in self.history.forget_page(id) {
                    self.parked.remove(&orphan);
                }
                if let Some(s) = &mut self.sync {
                    s.docs.remove(&DocKey::Page(id));
                    s.subscribed.remove(&DocKey::Page(id));
                }
                out.removed_pages.push(id);
            }
        }
        self.forget_parked();
        // Чужая правка могла сменить направляющие страницы и версию схемы.
        self.guides_revision += 1;
        if self.schema_version() > SCHEMA_VERSION {
            self.read_only = true;
        }
        out.became_read_only = self.is_read_only() && !was_read_only;
        out
    }

    fn apply_remote_page(&mut self, id: PageId, batches: &[&[u8]]) -> DocResult<RemoteOutcome> {
        // Страница, которую уже удалили, или ещё не загруженная: подписки на неё нет,
        // а опоздавший кадр догонит журнал при открытии.
        let Some(page) = self.loaded.get(&id) else {
            return Ok(RemoteOutcome::default());
        };
        let was_read_only = self.is_read_only();
        let applied = page.apply_remote(batches)?;
        let mut out = RemoteOutcome {
            page: Some(id),
            failed: applied.failed,
            pending: applied.pending,
            rebuilt: applied.rebuilt,
            vectors: applied.vectors,
            text: applied
                .text
                .iter()
                .map(|(n, d)| (*n, TextShift(d.clone())))
                .collect(),
            ..RemoteOutcome::default()
        };
        if page.has_unknown_nodes() || page.has_oversized_text() {
            self.read_only = true;
        }
        out.became_read_only = self.is_read_only() && !was_read_only;
        if applied.rebuilt {
            // Сцена строится заново, частичные изменения ей не нужны; сверить все хэши дешевле, чем помнить их.
            self.changes.remove(&id);
            let refs = page.image_refs()?;
            out.missing_images = refs
                .into_iter()
                .filter(|h| self.images.get(h).is_none())
                .collect();
        } else {
            out.missing_images = applied
                .images
                .into_iter()
                .filter(|h| self.images.get(h).is_none())
                .collect();
            self.changes.entry(id).or_default().extend(applied.changes);
        }
        Ok(out)
    }

````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
mod page;
mod scene_cache;
mod text;
mod text_codec;
````

на:

````rust
mod page;
mod scene_cache;
mod sync;
mod text;
mod text_codec;
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
pub use page::{PageDoc, Tx};
pub use scene_cache::SceneRow;
pub use text::*;
pub use types::*;
````

на:

````rust
pub use page::{PageDoc, Tx};
pub use scene_cache::SceneRow;
pub use sync::{
    DocKey, RemoteOutcome, SyncBatch, SyncVersion, TextShift, MAX_SYNC_BATCH_BYTES,
    MAX_VERSION_PEERS,
};
pub use text::*;
pub use types::*;
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
use std::sync::Arc;

use loro::{
    ExportMode, LoroDoc, LoroText, LoroTree, LoroValue, TextDelta, TreeID, TreeParentId,
    ValueOrContainer, VersionVector,
};

use crate::codec;
use crate::error::{DocError, DocResult};
````

на:

````rust
use std::sync::Arc;

use loro::event::{Diff, DiffEvent};
use loro::{
    EventTriggerKind, ExportMode, Index, LoroDoc, LoroText, LoroTree, LoroValue, Subscription,
    TextDelta, TreeExternalDiff, TreeID, TreeParentId, ValueOrContainer, VersionVector,
};

use crate::changes::Changes;
use crate::codec;
use crate::error::{DocError, DocResult};
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
}

/// Индекс стабильных ID и зеркало свойств живых узлов.
#[derive(Default)]
````

на:

````rust
}

/// Больше стольких изменённых узлов за пачку зеркало строится заново: по одному узлу это дольше
/// полной сборки, а сцене дешевле собраться целиком.
const REBUILD_ABOVE: usize = 20_000;

/// Что сообщили события Loro о пачке чужих правок: сырые `TreeID`, зеркало их разберёт, когда пачка
/// применена целиком (события приходят по контейнерам, и порядок между ними не важен).
#[derive(Default)]
struct RemoteDirty {
    tree: Vec<TreeID>,
    props: Vec<TreeID>,
    text: Vec<(TreeID, Vec<TextDelta>)>,
}

impl RemoteDirty {
    fn collect(&mut self, e: &DiffEvent) {
        // Своя транзакция зеркало уже обновила; чужие приходят только импортом.
        if e.triggered_by != EventTriggerKind::Import {
            return;
        }
        for d in &e.events {
            match &d.diff {
                Diff::Tree(t) => self.tree.extend(t.diff.iter().map(|i| {
                    debug_assert!(matches!(
                        i.action,
                        TreeExternalDiff::Create { .. }
                            | TreeExternalDiff::Move { .. }
                            | TreeExternalDiff::Delete { .. }
                    ));
                    i.target
                })),
                diff => {
                    // Карта узла, текст в карте узла: узел — последний в пути.
                    let node = d.path.iter().rev().find_map(|(_, i)| match i {
                        Index::Node(t) => Some(*t),
                        _ => None,
                    });
                    if let Some(tid) = node {
                        self.props.push(tid);
                        if let Diff::Text(delta) = diff {
                            self.text.push((tid, delta.clone()));
                        }
                    }
                }
            }
        }
    }
}

/// Итог пачки чужих правок для страницы.
#[derive(Default)]
pub(crate) struct RemoteApplied {
    pub changes: Changes,
    pub failed: bool,
    pub pending: bool,
    pub rebuilt: bool,
    pub text: Vec<(NodeId, Vec<TextDelta>)>,
    pub vectors: Vec<NodeId>,
    pub images: Vec<String>,
}

/// Индекс стабильных ID и зеркало свойств живых узлов.
#[derive(Default)]
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    /// Индекс и зеркало взяты из кэша сцены, а не прочитаны из Loro.
    from_cache: Cell<bool>,
}

````

на:

````rust
    /// Индекс и зеркало взяты из кэша сцены, а не прочитаны из Loro.
    from_cache: Cell<bool>,
    /// Один `NodeId` у двух живых узлов (параллельные отмена удаления и перенос): см. `rebuild_from_loro`.
    duplicates: Cell<bool>,
    /// События чужих правок: подписка только у файлов на сервере, остальным она ни к чему.
    remote: RefCell<Option<(std::sync::Arc<std::sync::Mutex<RemoteDirty>>, Subscription)>>,
}

````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
            hidden_refs: RefCell::default(),
            from_cache: Cell::default(),
        }
    }
````

на:

````rust
            hidden_refs: RefCell::default(),
            from_cache: Cell::default(),
            duplicates: Cell::default(),
            remote: RefCell::default(),
        }
    }
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        let mut oversized = false;
        let mut hidden_refs = Vec::new();
        let mut stack = self.tree.children(TreeParentId::Root).unwrap_or_default();
        while let Some(tid) = stack.pop() {
            let id = self.stable_id(tid);
            let props = match codec::read_props(&self.tree.get_meta(tid)?, &id) {
                Ok(props) => props,
                // Неверная сеть вектора и неизвестная булева операция (чужая
````

на:

````rust
        let mut oversized = false;
        let mut hidden_refs = Vec::new();
        let mut duplicates = false;
        let mut stack = self.tree.children(TreeParentId::Root).unwrap_or_default();
        while let Some(tid) = stack.pop() {
            let mut id = self.stable_id(tid);
            // Два живых узла с одним `NodeId`: отмена удаления создала новый узел, а параллельный
            // перенос оживил старый. Правило не зависит от порядка обхода, поэтому все участники
            // видят одно и то же: имя остаётся у исходного узла, иначе у меньшего `TreeID`;
            // другой узел живёт под своим `TreeID`, ничего не пропадает.
            if let Some(&other) = state.by_id.get(&id) {
                duplicates = true;
                let order = |t: TreeID| (t.peer, t.counter);
                let keeps = id.0 == other || (id.0 != tid && order(other) < order(tid));
                if keeps {
                    id = NodeId(tid);
                } else {
                    let props = state.props.remove(&id).expect("запись индекса");
                    state.by_id.remove(&id);
                    state.by_tree.remove(&other);
                    state.insert(NodeId(other), other, props);
                }
            }
            let meta = self.tree.get_meta(tid)?;
            // Узел без типа — половина чужой транзакции (большая правка идёт кусками): остальное придёт,
            // а пока ни его, ни его детей для модели нет.
            if !codec::has_kind(&meta) {
                continue;
            }
            let props = match codec::read_props(&meta, &id) {
                Ok(props) => props,
                // Неверная сеть вектора и неизвестная булева операция (чужая
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        self.unknown_nodes.set(unknown);
        self.oversized_text.set(oversized);
        hidden_refs.sort();
        hidden_refs.dedup();
````

на:

````rust
        self.unknown_nodes.set(unknown);
        self.oversized_text.set(oversized);
        self.duplicates.set(duplicates);
        hidden_refs.sort();
        hidden_refs.dedup();
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
            );
        }
        Ok(())
    }

    /// Загружена ли страница по кэшу сцены — чтобы тесты проверяли именно быстрый путь.
````

на:

````rust
            );
        }
        Ok(())
    }

    /// Подписка на события Loro: дальше чужие правки обновляют зеркало по изменённым узлам. У
    /// подписки цена и на своих правках, поэтому только у файлов на сервере.
    pub(crate) fn listen(&self) {
        let mut remote = self.remote.borrow_mut();
        if remote.is_some() {
            return;
        }
        let dirty = std::sync::Arc::new(std::sync::Mutex::new(RemoteDirty::default()));
        let sink = dirty.clone();
        let sub = self.doc.subscribe_root(std::sync::Arc::new(move |e| {
            if let Ok(mut d) = sink.lock() {
                d.collect(&e);
            }
        }));
        *remote = Some((dirty, sub));
    }

    /// Применяет чужие правки и обновляет зеркало только там, где они что-то изменили (спека
    /// подсистемы 4, раздел 8). Сбой одной пачки не мешает следующим и не роняет документ.
    pub(crate) fn apply_remote(&self, batches: &[&[u8]]) -> DocResult<RemoteApplied> {
        self.listen();
        let mut out = RemoteApplied::default();
        for data in batches {
            match self.doc.import(data) {
                Ok(status) => out.pending |= status.pending.is_some(),
                Err(_) => out.failed = true,
            }
        }
        let dirty = match self.remote.borrow().as_ref() {
            Some((d, _)) => match d.lock() {
                Ok(mut d) => std::mem::take(&mut *d),
                Err(_) => RemoteDirty::default(),
            },
            None => RemoteDirty::default(),
        };
        self.from_cache.set(false);
        let tree: HashSet<TreeID> = dirty.tree.into_iter().collect();
        let props: HashSet<TreeID> = dirty.props.into_iter().collect();
        // Скрытые узлы и двойники зеркало по частям не ведёт, это редкость: сборка заново.
        let whole = self.unknown_nodes.get()
            || self.duplicates.get()
            || tree.len() + props.len() > REBUILD_ABOVE
            || !self.reconcile(&tree, &props, &mut out)?;
        if whole {
            out.changes = Changes::default();
            out.images.clear();
            out.vectors.clear();
            out.rebuilt = true;
            self.rebuild_from_loro()?;
        }
        let state = self.state.borrow();
        out.text = dirty
            .text
            .into_iter()
            .filter_map(|(tid, d)| Some((*state.by_tree.get(&tid)?, d)))
            .collect();
        Ok(out)
    }

    /// Сверяет с Loro узлы, которых коснулись события. События при параллельных правках бывают
    /// шумными (Loro откатывает и применяет заново): правда — текущее состояние дерева, а не их порядок.
    /// `false` — зеркало по частям не обновить: нужна сборка заново.
    fn reconcile(
        &self,
        tree: &HashSet<TreeID>,
        props: &HashSet<TreeID>,
        out: &mut RemoteApplied,
    ) -> DocResult<bool> {
        let dead = |t: &TreeID| self.tree.is_node_deleted(t).unwrap_or(true);
        // Сначала удаления: отмена удаления создаёт новый узел с прежним `NodeId`.
        for tid in tree.iter().filter(|t| dead(t)) {
            self.forget_subtree(*tid, &mut out.changes);
        }
        let mut born = HashSet::new();
        for tid in tree.iter().filter(|t| !dead(t)) {
            if born.contains(tid) {
                continue;
            }
            let known = self.state.borrow().by_tree.get(tid).copied();
            match known {
                Some(id) => out.changes.moved.push(id),
                None => {
                    if !self.adopt_subtree(*tid, &mut born, out)? {
                        return Ok(false);
                    }
                }
            }
        }
        for tid in props {
            if born.contains(tid) || dead(tid) {
                continue;
            }
            let Some(id) = self.state.borrow().by_tree.get(tid).copied() else {
                // Живой, но не в зеркале: половина чужой транзакции, теперь, возможно, узел целый.
                if !self.adopt_subtree(*tid, &mut born, out)? {
                    return Ok(false);
                }
                continue;
            };
            let new = match codec::read_props(&self.tree.get_meta(*tid)?, &id) {
                Ok(p) => p,
                Err(
                    DocError::UnknownKind(..)
                    | DocError::InvalidVector(..)
                    | DocError::InvalidBooleanOp(..),
                ) => return Ok(false),
                Err(e) => return Err(e),
            };
            let mut state = self.state.borrow_mut();
            let Some(old) = state.props.get_mut(&id) else {
                continue;
            };
            if *old == new {
                continue;
            }
            if old.vector != new.vector {
                out.vectors.push(id);
            }
            if old.fills != new.fills {
                out.images.extend(image_hashes(&new));
            }
            if new
                .text
                .as_ref()
                .is_some_and(|t| t.content.check_limits().is_err())
            {
                self.oversized_text.set(true);
            }
            *old = new;
            out.changes.props.push(id);
        }
        Ok(true)
    }

    /// Убирает из зеркала узел и потомков. У удалённого узла Loro по-прежнему отдаёт детей.
    fn forget_subtree(&self, root: TreeID, changes: &mut Changes) {
        let mut stack = vec![root];
        let mut state = self.state.borrow_mut();
        while let Some(tid) = stack.pop() {
            let Some(id) = state.by_tree.get(&tid).copied() else {
                continue;
            };
            state.remove(id);
            changes.deleted.push(id);
            stack.extend(
                self.tree
                    .children(TreeParentId::Node(tid))
                    .unwrap_or_default(),
            );
        }
    }

    /// Добавляет в зеркало новый узел с потомками. `false` — неизвестный вид или занятый `NodeId`.
    fn adopt_subtree(
        &self,
        root: TreeID,
        born: &mut HashSet<TreeID>,
        out: &mut RemoteApplied,
    ) -> DocResult<bool> {
        // Родитель ещё неполон: узел придёт вместе с ним, как и при полной сборке.
        let parent_ready = match self.tree.parent(root) {
            Some(TreeParentId::Node(p)) => self.state.borrow().by_tree.contains_key(&p),
            _ => true,
        };
        if !parent_ready {
            return Ok(true);
        }
        let mut stack = vec![root];
        while let Some(tid) = stack.pop() {
            let meta = self.tree.get_meta(tid)?;
            if !codec::has_kind(&meta) {
                continue;
            }
            let id = self.stable_id(tid);
            if self.state.borrow().by_id.contains_key(&id) {
                return Ok(false);
            }
            let props = match codec::read_props(&meta, &id) {
                Ok(p) => p,
                Err(
                    DocError::UnknownKind(..)
                    | DocError::InvalidVector(..)
                    | DocError::InvalidBooleanOp(..),
                ) => return Ok(false),
                Err(e) => return Err(e),
            };
            if props
                .text
                .as_ref()
                .is_some_and(|t| t.content.check_limits().is_err())
            {
                self.oversized_text.set(true);
            }
            out.images.extend(image_hashes(&props));
            self.state.borrow_mut().insert(id, tid, props);
            out.changes.created.push(id);
            born.insert(tid);
            stack.extend(
                self.tree
                    .children(TreeParentId::Node(tid))
                    .unwrap_or_default(),
            );
        }
        Ok(true)
    }

    /// Загружена ли страница по кэшу сцены — чтобы тесты проверяли именно быстрый путь.
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

    /// Только для тестов: тип узла в Loro как есть (чужая правка из более новой сборки).
    #[doc(hidden)]
    pub fn force_kind_for_tests(&self, id: NodeId, kind: &str) {
        let tid = self.tid(id).expect("узел есть");
        let meta = self.tree.get_meta(tid).expect("карта узла");
        meta.insert(codec::key::TYPE, kind).expect("запись в Loro");
        self.doc.commit();
    }
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    pub(crate) fn commit(&self) {
        self.doc.commit();
    }

````

на:

````rust
    pub(crate) fn commit(&self) {
        self.doc.commit();
    }

    /// Документ Loro страницы: курсор синхронизации сам собирает пачки (`sync::split_updates`).
    pub(crate) fn loro(&self) -> &LoroDoc {
        &self.doc
    }

````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
}

impl std::ops::Deref for Tx<'_> {
    type Target = PageDoc;
````

на:

````rust
}

fn image_hashes(p: &Props) -> impl Iterator<Item = String> + '_ {
    p.fills.iter().filter_map(|f| f.image_hash.clone())
}

impl std::ops::Deref for Tx<'_> {
    type Target = PageDoc;
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    }

    #[test]
    fn missing_kind_is_corruption() {
        let page = PageDoc::new();
        let a = page.create_node(None, None, NodeKind::Rect).unwrap();
        page.force_raw_kind_for_tests(a, None);
        assert!(matches!(
            PageDoc::from_snapshot(&page.export_snapshot().unwrap()),
            Err(DocError::Corrupted(_))
        ));
    }

````

на:

````rust
    }

    /// Узел без типа — половина чужой транзакции: файл открывается, узла для модели пока нет.
    #[test]
    fn missing_kind_hides_node_until_it_arrives() {
        let page = PageDoc::new();
        let a = page.create_node(None, None, NodeKind::Rect).unwrap();
        let b = page.create_node(None, None, NodeKind::Rect).unwrap();
        page.force_raw_kind_for_tests(a, None);
        let loaded = PageDoc::from_snapshot(&page.export_snapshot().unwrap()).unwrap();
        assert!(!loaded.contains(a));
        assert_eq!(loaded.walk(), vec![b]);
        assert!(!loaded.has_unknown_nodes(), "не повод для «только чтения»");
    }

````

Создать `crates/ferrum-doc/src/sync.rs`:

````rust
//! Синхронизация с сервером (спека подсистемы 4, разделы 7 и 8): какой документ файла,
//! вектор версий на проводе и курсоры: что сервер уже знает и что ему отправлено.
//! Курсор считает версии независимо от журнала правок (`take_journal`).

use std::collections::HashMap;
use std::fmt;

use loro::{ExportMode, LoroDoc, TextDelta, VersionVector, ID};

use crate::error::DocResult;
use crate::types::{NodeId, PageId};

/// Предел одной пачки: кадр сервера 16 МБ (спека, раздел 7.3), запас — на заголовок кадра.
pub const MAX_SYNC_BATCH_BYTES: usize = 16 * 1024 * 1024 - 1024;
/// Больше стольких записей в векторе версий файл не принимает: столько сеансов правки на один
/// документ не набирается, а чужой кадр не должен задавать размер разбора.
pub const MAX_VERSION_PEERS: usize = 65_536;

/// Единица синхронизации: корневой документ или страница.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocKey {
    Root,
    Page(PageId),
}

impl DocKey {
    pub const ROOT: &'static str = "root";

    pub fn parse(s: &str) -> Option<DocKey> {
        if s == DocKey::ROOT {
            return Some(DocKey::Root);
        }
        s.parse().ok().map(DocKey::Page)
    }
}

impl fmt::Display for DocKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocKey::Root => f.write_str(DocKey::ROOT),
            DocKey::Page(p) => p.fmt(f),
        }
    }
}

/// Вектор версий документа: остальные крейты Loro не видят.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncVersion(pub(crate) VersionVector);

impl SyncVersion {
    /// `n: u32 · (peer: u64 · counter: u32) × n`, всё LE: формат `subscribe` и `subscribed`.
    pub fn encode(&self) -> Vec<u8> {
        let mut peers: Vec<_> = self.0.iter().map(|(p, c)| (*p, *c)).collect();
        peers.sort_unstable();
        let mut out = Vec::with_capacity(4 + peers.len() * 12);
        out.extend_from_slice(&(peers.len() as u32).to_le_bytes());
        for (peer, counter) in peers {
            out.extend_from_slice(&peer.to_le_bytes());
            out.extend_from_slice(&(counter as u32).to_le_bytes());
        }
        out
    }

    /// Вектор с провода: сверх предела, обрезанный и с хвостом — `None`.
    pub fn decode(bytes: &[u8]) -> Option<SyncVersion> {
        let n = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
        if n > MAX_VERSION_PEERS || bytes.len() != 4 + n * 12 {
            return None;
        }
        let mut vv = VersionVector::default();
        for i in 0..n {
            let at = 4 + i * 12;
            let peer = u64::from_le_bytes(bytes[at..at + 8].try_into().ok()?);
            let counter = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().ok()?);
            let counter = i32::try_from(counter).ok()?;
            if counter > 0 {
                vv.insert(peer, counter);
            }
        }
        Some(SyncVersion(vv))
    }

    /// Содержит ли этот вектор все правки `other`.
    pub fn includes(&self, other: &SyncVersion) -> bool {
        self.0.includes_vv(&other.0)
    }
}

/// Пачка своих правок для сервера. `end` — версия, которую сервер узнает, приняв пачку.
#[derive(Debug, Clone)]
pub struct SyncBatch {
    pub doc: DocKey,
    pub bytes: Vec<u8>,
    pub end: SyncVersion,
}

/// Чужая вставка или удаление в тексте: куда переехала позиция каретки.
#[derive(Debug, Clone, PartialEq)]
pub struct TextShift(pub(crate) Vec<TextDelta>);

impl TextShift {
    /// Новая позиция символа `at` (символы Unicode). Чужая вставка ровно в позиции `at` курсор не
    /// сдвигает: человек продолжает печатать там, где печатал **(поведение)**.
    pub fn map(&self, at: usize) -> usize {
        let (mut old, mut new) = (0usize, 0usize);
        for op in &self.0 {
            match op {
                TextDelta::Retain { retain, .. } => {
                    if at < old + retain {
                        return new + (at - old);
                    }
                    old += retain;
                    new += retain;
                }
                TextDelta::Insert { insert, .. } => {
                    if old == at {
                        return new;
                    }
                    new += insert.chars().count();
                }
                TextDelta::Delete { delete } => {
                    if at < old + delete {
                        return new;
                    }
                    old += delete;
                }
            }
        }
        new + at.saturating_sub(old)
    }
}

/// Что изменилось после применения пачки чужих правок. Изменения узлов — обычным путём
/// (`Document::take_changes`), кроме `rebuilt`: зеркало страницы построено заново, сцену — тоже.
#[derive(Debug, Clone, Default)]
pub struct RemoteOutcome {
    /// Страница пачки; `None` — корневой документ или страница, которой нет.
    pub page: Option<PageId>,
    /// Часть байтов не применена: повреждена (её отверг бы и сервер) — интерфейс скажет об этом.
    pub failed: bool,
    /// Пачке не хватает предыдущих правок: Loro применит её, когда они придут.
    pub pending: bool,
    pub rebuilt: bool,
    /// Чужие правки текста по узлам, по порядку.
    pub text: Vec<(NodeId, TextShift)>,
    /// Узлы с изменённой сетью вектора.
    pub vectors: Vec<NodeId>,
    /// Корень: список страниц, имя или направляющие могли измениться.
    pub root_changed: bool,
    pub removed_pages: Vec<PageId>,
    /// Новые страницы другого участника.
    pub added_pages: Vec<PageId>,
    /// Изображения, на которые теперь ссылаются узлы, а байтов нет.
    pub missing_images: Vec<String>,
    /// Файл стал «только для чтения»: чужая правка из более новой версии Ferrum.
    pub became_read_only: bool,
}

/// Что сервер знает о документе и что ему отправлено.
#[derive(Debug, Default, Clone)]
pub(crate) struct Cursor {
    /// До ответа `subscribed` версия сервера неизвестна, и отправлять нечего.
    pub ready: bool,
    pub sent: VersionVector,
    pub acked: VersionVector,
}

#[derive(Default)]
pub(crate) struct SyncCursors {
    pub docs: HashMap<DocKey, Cursor>,
    /// Новых пачек не собираем, пока их не примет сторона соединения.
    pub subscribed: std::collections::HashSet<DocKey>,
}

/// Обновления `from` … текущая версия кусками не больше `limit` байт. Куски идут в порядке
/// счётчиков внутри пира, старые сеансы (пиры прошлых открытий) раньше нынешнего: правки нынешнего
/// могут зависеть от них.
pub(crate) fn split_updates(
    doc: &LoroDoc,
    from: &VersionVector,
    limit: usize,
) -> DocResult<Vec<(Vec<u8>, VersionVector)>> {
    let vv = doc.oplog_vv();
    let whole = doc.export(ExportMode::updates(from))?;
    if whole.len() <= limit {
        return Ok(vec![(whole, vv)]);
    }
    let own = doc.peer_id();
    let mut spans: Vec<(u64, i32, i32)> = vv
        .iter()
        .filter_map(|(p, c)| {
            let start = from.get(p).copied().unwrap_or(0);
            (*c > start).then_some((*p, start, *c))
        })
        .collect();
    spans.sort_by_key(|(p, ..)| (*p == own, *p));
    let mut out = Vec::new();
    let mut end = from.clone();
    for (peer, start, stop) in spans {
        let mut at = start;
        let mut step = (stop - start).max(1);
        while at < stop {
            let upto = (at + step).min(stop);
            let span = loro::IdSpan::new(peer, at, upto);
            let bytes = doc.export(ExportMode::updates_in_range(vec![span]))?;
            // Одна операция сверх предела делиться не может: её откажет сервер, причину увидит человек.
            if bytes.len() > limit && upto - at > 1 {
                step = ((upto - at) / 2).max(1);
                continue;
            }
            end.extend_to_include_last_id(ID::new(peer, upto - 1));
            out.push((bytes, end.clone()));
            at = upto;
        }
    }
    Ok(out)
}

/// Объединяет: всё, что есть в `other`, считается и в `into`.
pub(crate) fn merge(into: &mut VersionVector, other: &VersionVector) {
    into.extend_to_include_vv(other.iter());
}

/// Что принесла пачка: конец её версий по пирам. Снимок или повреждённые байты — пусто.
pub(crate) fn blob_end(bytes: &[u8]) -> VersionVector {
    LoroDoc::decode_import_blob_meta(bytes, false)
        .map(|m| m.partial_end_vv)
        .unwrap_or_default()
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc`

Expected: PASS: fmt и clippy без замечаний; `ferrum-doc`: все тесты проходят

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-doc/examples/sync_report.rs crates/ferrum-doc/src/codec.rs crates/ferrum-doc/src/document.rs crates/ferrum-doc/src/lib.rs crates/ferrum-doc/src/page.rs crates/ferrum-doc/src/sync.rs crates/ferrum-doc/tests/remote.rs
git commit -F - <<'EOF'
ядро: чужие правки — зеркало по изменённым узлам и курсор синхронизации

`Document::apply_remote` применяет пачку Loro корня или страницы; зеркало, индекс и `Changes` обновляются
по событиям Loro только у задетых узлов (спека подсистемы 4, раздел 8), дальше обычный путь сцены. Узел без
типа — половина чужой транзакции: файл открывается, узел появится, когда придёт остальное. Двойник `NodeId`
(отмена удаления и параллельное оживление) решается правилом, не зависящим от порядка.

Курсор синхронизации считает версии независимо от журнала: `take_sync_updates` шлёт только своё и кусками
не больше кадра, `sync_acked`, `sync_covers`, `sync_unacked`. Формат вектора версий для провода — `SyncVersion`.
Журнал файла на сервере не считает нехватку предыдущих правок повреждением: они придут при догонянии.

`sync_report`: 100 000 узлов, одна чужая правка — p95 37 мкс при цели 2 мс; правка дерева параллельно своей
правке дерева — 50 мс (первая 0,7 с): это откат всего дерева внутри Loro, цель там недостижима.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 2: Ядро — отмена рядом с чужими правками

Спека §8, риск 3 раздела 14. Дельты текста преобразуются через чужие правки, исчезнувшее и циклы пропускаются, proptest перемешивает отмену, повтор и импорт.

**Files:**
- Create: `crates/ferrum-doc/src/text_ot.rs`
- Modify: `crates/ferrum-doc/src/document.rs`, `crates/ferrum-doc/src/history.rs`, `crates/ferrum-doc/src/lib.rs`, `crates/ferrum-doc/src/page.rs`
- Test: `crates/ferrum-doc/tests/remote.proptest-regressions` (новый), `crates/ferrum-doc/tests/remote.rs`

**Interfaces:**
- Consumes: `Document::apply_remote`.
- Produces: `text_ot`, `History::transform_text`, `PageDoc::apply_step`, пропускающий невыполнимое.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-doc/tests/remote.proptest-regressions`:

````
# Seeds for failure cases proptest has generated in the past. It is
# automatically read and these particular cases re-run before any
# novel cases are generated.
#
# It is recommended to check this file in to source control so that
# everyone who runs the test benefits from these saved cases.
cc e705987698203e05afbbaf69dcded9328c20a50cf8e19c43187a68750b54b48d # shrinks to ops = [Create { who: 2, parent: 15 }, Create { who: 0, parent: 1 }, Sync { seed: 18138697921306519780 }, SetX { who: 2, node: 18, x: 314 }, SetX { who: 1, node: 2, x: -181 }, Redo { who: 0 }, Sync { seed: 16879731939370906077 }, Create { who: 1, parent: 38 }, Create { who: 2, parent: 27 }, Create { who: 0, parent: 38 }, Delete { who: 2, node: 34 }, Delete { who: 0, node: 24 }, Undo { who: 1 }, Sync { seed: 13053099428560909583 }, SetX { who: 2, node: 32, x: 295 }, Sync { seed: 4619095229409144641 }, Undo { who: 0 }, Move { who: 1, node: 30, parent: 33, index: 5 }, Undo { who: 0 }, Sync { seed: 9936209035233501474 }, Sync { seed: 12485988878668468793 }, Sync { seed: 15337764591852827387 }, Create { who: 0, parent: 0 }, SetX { who: 2, node: 26, x: 22 }, Delete { who: 1, node: 37 }, Sync { seed: 10635741446499739060 }, SetX { who: 1, node: 33, x: 389 }, Create { who: 2, parent: 39 }, Undo { who: 1 }, Redo { who: 2 }, Move { who: 1, node: 6, parent: 21, index: 3 }, Delete { who: 2, node: 1 }, Sync { seed: 4166416152049987310 }, Create { who: 1, parent: 35 }, Delete { who: 0, node: 27 }, Move { who: 2, node: 20, parent: 13, index: 0 }, Sync { seed: 2511527128132625671 }, Undo { who: 2 }, Delete { who: 0, node: 27 }, SetX { who: 0, node: 1, x: 495 }, Redo { who: 1 }, Sync { seed: 574656060471813281 }, Create { who: 0, parent: 11 }, SetX { who: 2, node: 39, x: 364 }, Sync { seed: 11675136553321240531 }, SetX { who: 1, node: 7, x: 300 }, Delete { who: 1, node: 36 }, SetX { who: 0, node: 20, x: -483 }, Undo { who: 1 }, SetX { who: 2, node: 21, x: 333 }, SetX { who: 0, node: 29, x: 115 }, Sync { seed: 15578993590394070746 }, Undo { who: 0 }]
````

В `crates/ferrum-doc/tests/remote.rs` заменить:

````rust
//! Чужие правки (спека подсистемы 4, раздел 8): зеркало обновляется по изменённым узлам и совпадает с
//! полной сборкой из Loro при любом порядке и повторах пачек; курсор
//! синхронизации шлёт только своё и считает версии независимо от журнала.

````

на:

````rust
//! Чужие правки (спека подсистемы 4, раздел 8): зеркало обновляется по изменённым узлам и совпадает с
//! полной сборкой из Loro при любом порядке и повторах пачек; отменяются только свои шаги; курсор
//! синхронизации шлёт только своё и считает версии независимо от журнала.

````

В `crates/ferrum-doc/tests/remote.rs` заменить:

````rust

#[test]
fn text_shift_moves_caret_with_foreign_edits() {
    let (mut a, mut b, page) = pair();
````

на:

````rust

#[test]
fn own_edits_are_not_counted_as_foreign_and_undo_history_survives() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    sync(&mut a, &mut b);
    set_x(&mut a, page, id, 5.0);
    let mine = rect(&mut b, page);
    b.take_changes(page);
    sync(&mut a, &mut b);
    assert!(b.can_undo());
    assert_eq!(b.undo().unwrap().unwrap().page, page);
    assert!(!b.page(page).unwrap().contains(mine), "отменён свой узел");
    assert_eq!(
        b.page(page).unwrap().props(id).unwrap().x,
        5.0,
        "чужая правка на месте"
    );
}

#[test]
fn undo_returns_own_value_even_if_other_changed_it() {
    let (mut a, mut b, page) = pair();
    let id = rect(&mut a, page);
    sync(&mut a, &mut b);
    set_x(&mut a, page, id, 10.0);
    sync(&mut a, &mut b);
    set_x(&mut b, page, id, 20.0);
    sync(&mut a, &mut b);
    assert_eq!(a.page(page).unwrap().props(id).unwrap().x, 20.0);
    a.undo().unwrap();
    assert_eq!(
        a.page(page).unwrap().props(id).unwrap().x,
        0.0,
        "своё прежнее значение"
    );
    sync(&mut a, &mut b);
    assert_eq!(b.page(page).unwrap().props(id).unwrap().x, 0.0);
}

#[test]
fn step_over_node_deleted_by_other_is_skipped_and_undo_goes_on() {
    let (mut a, mut b, page) = pair();
    let first = rect(&mut a, page);
    let second = rect(&mut a, page);
    sync(&mut a, &mut b);
    set_x(&mut a, page, first, 1.0);
    set_x(&mut a, page, second, 2.0);
    sync(&mut a, &mut b);
    b.transact(page, vec![], |tx| {
        tx.delete_nodes(&[second])?;
        Ok(vec![])
    })
    .unwrap();
    sync(&mut a, &mut b);
    // Верхний шаг — про удалённый узел: пропущен, отменился шаг про `first`.
    let restore = a.undo().unwrap();
    assert!(restore.is_some());
    assert_eq!(a.page(page).unwrap().props(first).unwrap().x, 0.0);
    assert!(
        !a.page(page).unwrap().contains(second),
        "чужое удаление отмена не трогает"
    );
}

#[test]
fn restoring_deleted_node_under_parent_removed_by_other_is_skipped() {
    let (mut a, mut b, page) = pair();
    let ids = a
        .transact(page, vec![], |tx| {
            let f = tx.create_node(None, None, NodeKind::Frame)?;
            Ok(vec![f, tx.create_node(Some(f), None, NodeKind::Rect)?])
        })
        .unwrap();
    let (frame, kid) = (ids[0], ids[1]);
    sync(&mut a, &mut b);
    a.transact(page, vec![], |tx| {
        tx.delete_nodes(&[kid])?;
        Ok(vec![])
    })
    .unwrap();
    sync(&mut a, &mut b);
    b.transact(page, vec![], |tx| {
        tx.delete_nodes(&[frame])?;
        Ok(vec![])
    })
    .unwrap();
    sync(&mut a, &mut b);
    // Ни удаление ребёнка, ни создание: от создания фрейма осталось только его создание.
    a.undo().unwrap();
    assert!(!a.page(page).unwrap().contains(kid));
    assert_mirror(&mut a, page);
}

#[test]
fn undo_of_move_into_descendant_of_foreign_move_is_skipped() {
    let (mut a, mut b, page) = pair();
    let ids = a
        .transact(page, vec![], |tx| {
            Ok(vec![
                tx.create_node(None, None, NodeKind::Frame)?,
                tx.create_node(None, None, NodeKind::Frame)?,
            ])
        })
        .unwrap();
    let (x, y) = (ids[0], ids[1]);
    a.transact(page, vec![], |tx| {
        tx.move_node(x, Some(y), 0)?;
        Ok(vec![])
    })
    .unwrap();
    sync(&mut a, &mut b);
    // x внутри y. a вернул бы x на корень — можно. Другой участник вынес y в x нельзя: цикл.
    b.transact(page, vec![], |tx| {
        tx.move_node(x, None, 0)?;
        tx.move_node(y, Some(x), 0)?;
        Ok(vec![])
    })
    .unwrap();
    sync(&mut a, &mut b);
    // Отмена переноса x в y: прежний родитель x — корень, цикла нет, шаг применяется без ошибки.
    assert!(a.undo().is_ok());
    assert_mirror(&mut a, page);
}

#[test]
fn text_undo_keeps_foreign_insert_before_and_inside() {
    let (mut a, mut b, page) = pair();
    let id = a
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Text)?])
        })
        .unwrap()[0];
    sync(&mut a, &mut b);
    a.transact(page, vec![id], |tx| {
        tx.insert_text(id, 0, "абв", None)?;
        Ok(vec![id])
    })
    .unwrap();
    sync(&mut a, &mut b);
    b.transact(page, vec![id], |tx| {
        tx.insert_text(id, 0, "!", None)?;
        tx.insert_text(id, 2, "?", None)?;
        Ok(vec![id])
    })
    .unwrap();
    sync(&mut a, &mut b);
    let t = |d: &mut Document| -> String {
        let c = d
            .page(page)
            .unwrap()
            .props(id)
            .unwrap()
            .text
            .unwrap()
            .content
            .clone();
        c.runs.iter().map(|r| r.text.as_str()).collect()
    };
    assert_eq!(t(&mut a), "!а?бв\n");
    a.undo().unwrap();
    assert_eq!(t(&mut a), "!?\n", "убраны только свои символы");
    a.redo().unwrap();
    assert_eq!(t(&mut a).chars().filter(|c| "абв".contains(*c)).count(), 3);
    sync(&mut a, &mut b);
    assert_eq!(t(&mut a), t(&mut b));
}

#[test]
fn text_shift_moves_caret_with_foreign_edits() {
    let (mut a, mut b, page) = pair();
````

В `crates/ferrum-doc/tests/remote.rs` заменить:

````rust
        .is_empty());
    assert!(!b.sync_unacked());
}

````

на:

````rust
        .is_empty());
    assert!(!b.sync_unacked());
}

#[test]
fn downgraded_editor_keeps_unacked_visible_and_sends_nothing() {
    let (mut a, _b, page) = pair();
    rect(&mut a, page);
    a.set_view_only(true);
    assert!(a
        .take_sync_updates(MAX_SYNC_BATCH_BYTES)
        .unwrap()
        .is_empty());
    assert!(a.sync_unacked(), "правка не дошла, и это видно");
}

````

В `crates/ferrum-doc/tests/remote.rs` заменить:

````rust
}

#[derive(Debug, Clone)]
enum Op {
````

на:

````rust
}

/// Два живых узла с одним `NodeId` у обоих участников: a удалил и отменил (новый `TreeID`), а перенос
/// b позже удаления по часам Lamport оживил старый узел. Возвращает `(исходный, новый под своим TreeID)`.
fn twins(kind: NodeKind) -> (Document, Document, PageId, NodeId, NodeId) {
    let (mut a, mut b, page) = pair();
    let id = a
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, kind)?])
        })
        .unwrap()[0];
    sync(&mut a, &mut b);
    a.transact(page, vec![], |tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
    a.undo().unwrap();
    // Часы b уходят вперёд: его перенос побеждает удаление независимо от случайных пиров.
    let spare = rect(&mut b, page);
    for x in 0..8 {
        set_x(&mut b, page, spare, x as f64);
    }
    b.transact(page, vec![], |tx| {
        tx.move_node(id, None, 5)?;
        Ok(vec![])
    })
    .unwrap();
    sync(&mut a, &mut b);
    sync(&mut a, &mut b);
    assert_eq!(a.page(page).unwrap().walk().len(), 3, "двойник жив");
    assert_mirror(&mut a, page);
    assert_mirror(&mut b, page);
    let alias = a
        .page(page)
        .unwrap()
        .walk()
        .into_iter()
        .find(|n| *n != id && *n != spare)
        .unwrap();
    (a, b, page, id, alias)
}

#[test]
fn deleting_named_twin_gives_its_name_to_the_other() {
    let (mut a, mut b, page, id, alias) = twins(NodeKind::Rect);
    a.take_changes(page);
    a.transact(page, vec![], |tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
    assert_mirror(&mut a, page);
    let doc = a.page(page).unwrap();
    assert!(doc.contains(id), "оставшийся двойник получил имя");
    assert!(!doc.contains(alias));
    let c = a.take_changes(page);
    assert!(
        c.deleted.contains(&alias) && c.created.contains(&id),
        "{c:?}"
    );

    sync(&mut a, &mut b);
    assert_mirror(&mut b, page);
    assert_eq!(nodes(&mut a, page), nodes(&mut b, page));

    // Узел с таким именем жив: отменять нечего, и зеркало не расходится.
    a.undo().unwrap();
    assert_mirror(&mut a, page);
    sync(&mut a, &mut b);
    assert_eq!(nodes(&mut a, page), nodes(&mut b, page));
}

#[test]
fn deleting_second_twin_keeps_names_and_undo_returns_it() {
    let (mut a, mut b, page, id, alias) = twins(NodeKind::Rect);
    a.transact(page, vec![], |tx| {
        tx.delete_nodes(&[alias])?;
        Ok(vec![])
    })
    .unwrap();
    assert_mirror(&mut a, page);
    assert!(a.page(page).unwrap().contains(id));
    a.undo().unwrap();
    assert_mirror(&mut a, page);
    assert!(a.page(page).unwrap().contains(alias));
    sync(&mut a, &mut b);
    assert_mirror(&mut b, page);
    assert_eq!(nodes(&mut a, page), nodes(&mut b, page));
}

#[test]
fn renamed_twin_reports_its_children_to_the_scene() {
    // Имя меняется у фрейма с ребёнком: сцена должна перецепить ребёнка, а не удалить вместе со старым.
    let (mut a, mut b, page, id, alias) = twins(NodeKind::Frame);
    let kid = a
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(Some(alias), None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    assert_mirror(&mut a, page);
    a.take_changes(page);
    a.transact(page, vec![], |tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
    assert_mirror(&mut a, page);
    assert_eq!(a.page(page).unwrap().parent(kid), Some(id));
    let c = a.take_changes(page);
    assert!(c.moved.contains(&kid), "{c:?}");
    assert!(
        c.deleted.contains(&alias) && c.created.contains(&id),
        "{c:?}"
    );
    sync(&mut a, &mut b);
    assert_eq!(nodes(&mut a, page), nodes(&mut b, page));
}

#[derive(Debug, Clone)]
enum Op {
````

В `crates/ferrum-doc/tests/remote.rs` заменить:

````rust
        x: i32,
    },
    /// Доставка: пачки в перемешанном порядке и с повторами.
    Sync {
````

на:

````rust
        x: i32,
    },
    Undo {
        who: usize,
    },
    Redo {
        who: usize,
    },
    /// Доставка: пачки в перемешанном порядке и с повторами.
    Sync {
````

В `crates/ferrum-doc/tests/remote.rs` заменить:

````rust
        1 => (0usize..3, 0usize..40).prop_map(|(who, node)| Op::Delete { who, node }),
        3 => (0usize..3, 0usize..40, -500i32..500).prop_map(|(who, node, x)| Op::SetX { who, node, x }),
        3 => any::<u64>().prop_map(|seed| Op::Sync { seed }),
    ]
````

на:

````rust
        1 => (0usize..3, 0usize..40).prop_map(|(who, node)| Op::Delete { who, node }),
        3 => (0usize..3, 0usize..40, -500i32..500).prop_map(|(who, node, x)| Op::SetX { who, node, x }),
        2 => (0usize..3).prop_map(|who| Op::Undo { who }),
        1 => (0usize..3).prop_map(|who| Op::Redo { who }),
        3 => any::<u64>().prop_map(|seed| Op::Sync { seed }),
    ]
````

В `crates/ferrum-doc/tests/remote.rs` заменить:

````rust
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Спека, раздел 13.2: случайные правки нескольких клиентов с перестановкой и повтором пачек; все приходят к одному дереву, зеркало совпадает с полной сборкой.
    #[test]
    fn random_edits_converge_and_mirror_matches_rebuild(ops in proptest::collection::vec(op(), 1..70)) {
````

на:

````rust
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Спека, раздел 13.2: случайные правки нескольких клиентов с перестановкой и повтором пачек, отмена
    /// вперемешку с импортом; все приходят к одному дереву, зеркало совпадает с полной сборкой.
    #[test]
    fn random_edits_converge_and_mirror_matches_rebuild(ops in proptest::collection::vec(op(), 1..70)) {
````

В `crates/ferrum-doc/tests/remote.rs` заменить:

````rust
                    });
                }
                Op::Sync { seed } => {
                    for (who, d) in docs.iter_mut().enumerate() {
````

на:

````rust
                    });
                }
                Op::Undo { who } => { docs[*who].undo().unwrap(); }
                Op::Redo { who } => { docs[*who].redo().unwrap(); }
                Op::Sync { seed } => {
                    for (who, d) in docs.iter_mut().enumerate() {
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc --test remote`
Expected: FAIL — собирается, 20 passed, 6 failed: шаги отмены над исчезнувшим узлом и дельты текста через чужие правки ещё не обработаны

- [ ] **Step 3: Реализация**

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        // сцене тем более нужно сверить их со страницей.
        self.changes.entry(page).or_default().record(&ops);
        match result {
            Ok(selection_after) => {
                if !ops.is_empty() {
````

на:

````rust
        // сцене тем более нужно сверить их со страницей.
        self.changes.entry(page).or_default().record(&ops);
        let out = match result {
            Ok(selection_after) => {
                if !ops.is_empty() {
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            // Откат: применяем обратные операции, в историю ничего не попадает.
            Err(e) => Err(rollback(p, &ops, e)),
        }
    }

````

на:

````rust
            // Откат: применяем обратные операции, в историю ничего не попадает.
            Err(e) => Err(rollback(p, &ops, e)),
        };
        self.settle_duplicates(page, out)
    }

    /// Зеркало после своей правки, если на странице есть двойники (`PageDoc::settle_duplicates`).
    /// Ошибка самой правки важнее сбоя сборки и не заслоняется им.
    fn settle_duplicates<T>(&mut self, page: PageId, out: DocResult<T>) -> DocResult<T> {
        let settled = match self.loaded.get(&page) {
            Some(p) => p.settle_duplicates(self.changes.entry(page).or_default()),
            None => Ok(()),
        };
        let out = out?;
        settled.map(|()| out)
    }

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            self.changes.entry(id).or_default().extend(applied.changes);
        }
        Ok(out)
    }
````

на:

````rust
            self.changes.entry(id).or_default().extend(applied.changes);
        }
        for (node, delta) in &applied.text {
            self.history.transform_text(id, *node, delta);
        }
        Ok(out)
    }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        self.writable()?;
        let p = self.loaded.get(&entry.page).expect("загружена");
        // При сбое apply_inverse сам откатывает уже применённую часть шага.
        let result = p.apply_inverse(&entry.ops);
        // Применённые операции записывают обратные к себе — это и есть изменения.
        // При сбое сверяем все узлы шага: откат мог и не удаться.
````

на:

````rust
        self.writable()?;
        let p = self.loaded.get(&entry.page).expect("загружена");
        // При сбое apply_inverse сам откатывает уже применённую часть шага. Узлы, которые успел
        // убрать другой участник, шаг пропускает.
        let result = p.apply_step(&entry.ops);
        // Применённые операции записывают обратные к себе — это и есть изменения.
        // При сбое сверяем все узлы шага: откат мог и не удаться.
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            Err(_) => changes.record(&entry.ops),
        }
        let ops = result?;
        match self.apply_doc_ops(&entry.doc_ops) {
            Ok((doc_ops, remap)) => Ok(Replay {
````

на:

````rust
            Err(_) => changes.record(&entry.ops),
        }
        let ops = self.settle_duplicates(entry.page, result)?;
        match self.apply_doc_ops(&entry.doc_ops) {
            Ok((doc_ops, remap)) => Ok(Replay {
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
                let changes = self.changes.entry(entry.page).or_default();
                changes.record(&ops);
                Err(match back {
                    Ok(_) => e,
                    Err(rollback) => DocError::RollbackFailed {
````

на:

````rust
                let changes = self.changes.entry(entry.page).or_default();
                changes.record(&ops);
                let failed = Err(match back {
                    Ok(_) => e,
                    Err(rollback) => DocError::RollbackFailed {
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
                        rollback: Box::new(rollback),
                    },
                })
            }
        }
````

на:

````rust
                        rollback: Box::new(rollback),
                    },
                });
                self.settle_duplicates(entry.page, failed)
            }
        }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

    pub fn undo(&mut self) -> DocResult<Option<Restore>> {
        let Some(entry) = self.history.pop_undo() else {
            return Ok(None);
        };
        match self.replay(&entry) {
            Ok(r) => {
                let restore = Restore {
````

на:

````rust

    pub fn undo(&mut self) -> DocResult<Option<Restore>> {
        // Шаг, от которого чужие правки не оставили ничего, пропускается: отмена идёт к следующему.
        while let Some(entry) = self.history.pop_undo() {
            match self.undo_entry(entry)? {
                Some(restore) => return Ok(Some(restore)),
                None => continue,
            }
        }
        Ok(None)
    }

    fn undo_entry(&mut self, entry: HistoryEntry) -> DocResult<Option<Restore>> {
        let empty = entry.ops.is_empty() && entry.doc_ops.is_empty();
        match self.replay(&entry) {
            Ok(r) if !empty && r.ops.is_empty() && r.doc_ops.is_empty() => Ok(None),
            Ok(r) => {
                let restore = Restore {
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

    pub fn redo(&mut self) -> DocResult<Option<Restore>> {
        let Some(entry) = self.history.pop_redo() else {
            return Ok(None);
        };
        match self.replay(&entry) {
            Ok(r) => {
                let mut next = HistoryEntry {
````

на:

````rust

    pub fn redo(&mut self) -> DocResult<Option<Restore>> {
        while let Some(entry) = self.history.pop_redo() {
            match self.redo_entry(entry)? {
                Some(restore) => return Ok(Some(restore)),
                None => continue,
            }
        }
        Ok(None)
    }

    fn redo_entry(&mut self, entry: HistoryEntry) -> DocResult<Option<Restore>> {
        let empty = entry.ops.is_empty() && entry.doc_ops.is_empty();
        match self.replay(&entry) {
            Ok(r) if !empty && r.ops.is_empty() && r.doc_ops.is_empty() => Ok(None),
            Ok(r) => {
                let mut next = HistoryEntry {
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    }

    #[test]
    fn failed_undo_keeps_history_entry() {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
````

на:

````rust
    }

    /// Шаг про узел, которого больше нет (убрал другой участник), пропускается, а не блокирует стек.
    #[test]
    fn undo_step_over_missing_node_is_skipped() {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            selection_after: vec![],
        });
        assert!(doc.undo().is_err());
        assert!(doc.can_undo());
        assert!(!doc.can_redo());
    }

    #[test]
    fn failed_redo_keeps_history_entry() {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
````

на:

````rust
            selection_after: vec![],
        });
        assert_eq!(doc.undo().unwrap(), None);
        assert!(!doc.can_undo());
        assert!(!doc.can_redo());
    }

    #[test]
    fn redo_step_over_missing_node_is_skipped() {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            selection_after: vec![],
        });
        assert!(doc.redo().is_err());
        assert!(doc.can_redo());
    }

````

на:

````rust
            selection_after: vec![],
        });
        assert_eq!(doc.redo().unwrap(), None);
        assert!(!doc.can_redo());
    }

````

В `crates/ferrum-doc/src/history.rs` заменить:

````rust
use crate::guides::Guide;
use crate::page::Inverse;
use crate::types::{Color, NodeId, PageId};

````

на:

````rust
use crate::guides::Guide;
use crate::page::Inverse;
use crate::text_ot;
use crate::types::{Color, NodeId, PageId};

````

В `crates/ferrum-doc/src/history.rs` заменить:

````rust
    }

    /// Шаги повтора ещё есть: припаркованная копия страницы может понадобиться.
    pub fn has_redo(&self) -> bool {
````

на:

````rust
    }

    /// Другой участник правил текст `node`: дельты отмены и повтора этого текста сдвигаются. Верхний
    /// шаг написан к нынешнему тексту, следующий — к тексту после его применения, поэтому чужая правка
    /// проходит стек сверху вниз и сама преобразуется каждым шагом. Склейка ввода рвётся: позиции
    /// курсора прошлой правки больше не те.
    pub fn transform_text(&mut self, page: PageId, node: NodeId, delta: &[loro::TextDelta]) {
        for stack in [&mut self.undo, &mut self.redo] {
            let mut incoming = delta.to_vec();
            for e in stack.iter_mut().rev().filter(|e| e.page == page) {
                for op in e.ops.iter_mut().rev() {
                    if let Inverse::Text(n, d) = op {
                        if *n == node {
                            let moved = text_ot::transform(d, &incoming);
                            incoming = text_ot::transform(&incoming, d);
                            *d = moved;
                        }
                    }
                }
            }
        }
        self.last_merge = None;
    }

    /// Шаги повтора ещё есть: припаркованная копия страницы может понадобиться.
    pub fn has_redo(&self) -> bool {
````

В `crates/ferrum-doc/src/lib.rs` заменить:

````rust
mod text;
mod text_codec;
mod types;
mod vector;
````

на:

````rust
mod text;
mod text_codec;
mod text_ot;
mod types;
mod vector;
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        while let Some(tid) = stack.pop() {
            let mut id = self.stable_id(tid);
            // Два живых узла с одним `NodeId`: отмена удаления создала новый узел, а параллельный
            // перенос оживил старый. Правило не зависит от порядка обхода, поэтому все участники
            // видят одно и то же: имя остаётся у исходного узла, иначе у меньшего `TreeID`;
            // другой узел живёт под своим `TreeID`, ничего не пропадает.
            if let Some(&other) = state.by_id.get(&id) {
                duplicates = true;
                let order = |t: TreeID| (t.peer, t.counter);
                let keeps = id.0 == other || (id.0 != tid && order(other) < order(tid));
                if keeps {
                    id = NodeId(tid);
                } else {
                    let props = state.props.remove(&id).expect("запись индекса");
                    state.by_id.remove(&id);
                    state.by_tree.remove(&other);
                    state.insert(NodeId(other), other, props);
                }
            }
            let meta = self.tree.get_meta(tid)?;
            // Узел без типа — половина чужой транзакции (большая правка идёт кусками): остальное придёт,
````

на:

````rust
        while let Some(tid) = stack.pop() {
            let mut id = self.stable_id(tid);
            let meta = self.tree.get_meta(tid)?;
            // Узел без типа — половина чужой транзакции (большая правка идёт кусками): остальное придёт,
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
                Err(e) => return Err(e),
            };
            oversized |= props
                .text
````

на:

````rust
                Err(e) => return Err(e),
            };
            // Два живых узла с одним `NodeId`: отмена удаления создала новый узел, а параллельный
            // перенос оживил старый. Правило не зависит от порядка обхода, поэтому все участники
            // видят одно и то же: имя остаётся у исходного узла, иначе у меньшего `TreeID`;
            // другой узел живёт под своим `TreeID`, ничего не пропадает. Скрытые узлы выше в этом
            // не участвуют: для модели их нет, и имя у живого они не отбирают.
            if let Some(&other) = state.by_id.get(&id) {
                duplicates = true;
                let order = |t: TreeID| (t.peer, t.counter);
                let keeps = id.0 == other || (id.0 != tid && order(other) < order(tid));
                if keeps {
                    id = NodeId(tid);
                } else {
                    let props = state.props.remove(&id).expect("запись индекса");
                    state.by_id.remove(&id);
                    state.by_tree.remove(&other);
                    state.insert(NodeId(other), other, props);
                }
            }
            oversized |= props
                .text
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
                    .unwrap_or_default(),
            );
        }
        Ok(())
````

на:

````rust
                    .unwrap_or_default(),
            );
        }
        Ok(())
    }

    /// Имя двойника зависит от того, жив ли второй, поэтому после своей правки зеркало со двойниками
    /// собирается заново, как и после чужой: иначе оставшийся узел так и жил бы под временным именем,
    /// а полная сборка вернула бы ему `NodeId`. Без двойников это чтение одного флага.
    ///
    /// Сцене сообщается смена имён: старое исчезло, новое появилось со своими свойствами, а дети
    /// перецепляются, иначе ушли бы вместе со старым именем.
    pub(crate) fn settle_duplicates(&self, changes: &mut Changes) -> DocResult<()> {
        if !self.duplicates.get() {
            return Ok(());
        }
        let before = self.state.borrow().by_tree.clone();
        self.rebuild_from_loro()?;
        let state = self.state.borrow();
        for (tid, old) in &before {
            let Some(new) = state.by_tree.get(tid).filter(|new| *new != old) else {
                continue;
            };
            changes.deleted.push(*old);
            changes.created.push(*new);
            changes.props.push(*new);
            let kids = self
                .tree
                .children(TreeParentId::Node(*tid))
                .unwrap_or_default();
            changes
                .moved
                .extend(kids.iter().filter_map(|k| state.by_tree.get(k)));
        }
        Ok(())
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        self.commit();
        result
    }

````

на:

````rust
        self.commit();
        result
    }

    /// Шаг отмены или повтора рядом с чужими правками (спека подсистемы 4, раздел 8): операция над
    /// узлом, которого больше нет, пропускается. Проверка — перед самой операцией: восстановление
    /// узла раньше в обратном проходе оживляет следующие за ним правки того же шага. Возвращает
    /// обратные к применённым; пусто — от шага ничего не осталось. Значения свойств возвращаются к
    /// своим прежним, даже если их менял другой.
    pub(crate) fn apply_step(&self, ops: &[Inverse]) -> DocResult<Vec<Inverse>> {
        self.begin_journal();
        let result = ops.iter().rev().try_for_each(|op| match op {
            _ if !self.can_replay(op) => Ok(()),
            Inverse::Restore(nodes) => nodes.iter().try_for_each(|n| {
                // Потомка могли вынести из удалённого поддерева, а его родителя — убрать насовсем.
                if self.contains(n.id) || n.parent.is_some_and(|p| !self.contains(p)) {
                    return Ok(());
                }
                self.create_with(n.id, n.parent, Some(n.index), n.props.clone())
                    .map(|_| ())
            }),
            // Дельта перенесена через чужие правки (`History::transform_text`). Не легла всё равно —
            // шаг не блокирует отмену навсегда из-за одного текста.
            Inverse::Text(id, delta) => {
                let _ = self.apply_text_delta(*id, delta);
                Ok(())
            }
            op => self.replay(std::slice::from_ref(op)),
        });
        let redo = self.take_journal();
        let result = match result {
            Ok(()) => Ok(redo),
            Err(original) => Err(match self.replay(&redo) {
                Ok(()) => original,
                Err(rollback) => DocError::RollbackFailed {
                    original: Box::new(original),
                    rollback: Box::new(rollback),
                },
            }),
        };
        self.commit();
        result
    }

    /// Осталось ли, к чему применить операцию.
    fn can_replay(&self, op: &Inverse) -> bool {
        match op {
            Inverse::Delete(id)
            | Inverse::Props(id, _)
            | Inverse::Shape(id, _)
            | Inverse::Text(id, _) => self.contains(*id),
            Inverse::Restore(nodes) => match nodes.first() {
                Some(root) => {
                    !self.contains(root.id) && root.parent.is_none_or(|p| self.contains(p))
                }
                None => false,
            },
            Inverse::Move { id, parent, .. } => {
                let Some(target) = parent else {
                    return self.contains(*id);
                };
                // Другой участник убрал родителя или перенёс его внутрь этого узла: на прежнее место нельзя.
                self.contains(*id)
                    && self.contains(*target)
                    && !std::iter::successors(Some(*target), |n| self.parent(*n)).any(|n| n == *id)
            }
        }
    }

````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    }

    /// Узел без типа — половина чужой транзакции: файл открывается, узла для модели пока нет.
    #[test]
````

на:

````rust
    }

    /// Половина чужой транзакции с чужим именем не отбирает его у живого узла, в каком бы порядке
    /// их ни обошла сборка: для модели такого узла нет.
    #[test]
    fn node_without_kind_does_not_take_name_from_live_twin() {
        for half_first in [false, true] {
            let page = PageDoc::new();
            let first = page.create_node(None, None, NodeKind::Rect).unwrap();
            let second = page.create_node(None, None, NodeKind::Rect).unwrap();
            let (half, live) = if half_first {
                (first, second)
            } else {
                (second, first)
            };
            let meta = page.tree.get_meta(page.tid(live).unwrap()).unwrap();
            meta.insert(KEY_ID, half.to_string()).unwrap();
            page.force_raw_kind_for_tests(half, None);
            let loaded = PageDoc::from_snapshot(&page.export_snapshot().unwrap()).unwrap();
            assert_eq!(loaded.walk(), vec![half], "живой узел с именем половины");
            assert!(!loaded.contains(live));
        }
    }

    /// Узел без типа — половина чужой транзакции: файл открывается, узла для модели пока нет.
    #[test]
````

Создать `crates/ferrum-doc/src/text_ot.rs`:

````rust
//! Перенос дельты текста через чужую правку: шаг отмены записан относительно текста, каким он был,
//! а другой человек с тех пор вставил или удалил символы (спека подсистемы 4, раздел 8).
//! Обычное преобразование операций: `transform(a, b)` — как применить `a` после `b`, если обе написаны
//! к одному и тому же тексту. При вставке в одно место своя (`a`) встаёт после чужой (`b`).

use loro::TextDelta;

/// Длина операции в символах исходного текста; вставка исходного текста не занимает.
struct Cursor<'a> {
    ops: &'a [TextDelta],
    i: usize,
    /// Сколько символов текущей операции (retain или delete) уже пройдено.
    used: usize,
}

impl<'a> Cursor<'a> {
    fn new(ops: &'a [TextDelta]) -> Cursor<'a> {
        Cursor { ops, i: 0, used: 0 }
    }

    fn peek(&self) -> Option<&'a TextDelta> {
        self.ops.get(self.i)
    }

    /// Остаток текущей операции по исходному тексту.
    fn left(&self) -> usize {
        match self.peek() {
            Some(TextDelta::Retain { retain, .. }) => retain - self.used,
            Some(TextDelta::Delete { delete }) => delete - self.used,
            Some(TextDelta::Insert { .. }) => 0,
            None => usize::MAX,
        }
    }

    fn advance(&mut self, n: usize) {
        if self.peek().is_none() {
            return;
        }
        self.used += n;
        if self.left() == 0 {
            self.i += 1;
            self.used = 0;
        }
    }
}

fn push(out: &mut Vec<TextDelta>, op: TextDelta) {
    match (out.last_mut(), &op) {
        (
            Some(TextDelta::Retain {
                retain,
                attributes: None,
            }),
            TextDelta::Retain {
                retain: n,
                attributes: None,
            },
        ) => {
            *retain += n;
        }
        (Some(TextDelta::Delete { delete }), TextDelta::Delete { delete: n }) => *delete += n,
        _ => out.push(op),
    }
}

/// `a` после `b`. Обе — к одному тексту, хвост после последней операции — как `retain`.
pub(crate) fn transform(a: &[TextDelta], b: &[TextDelta]) -> Vec<TextDelta> {
    let (mut x, mut y) = (Cursor::new(a), Cursor::new(b));
    let mut out = Vec::new();
    loop {
        // Чужая вставка: своя операция её только перешагивает.
        if let Some(TextDelta::Insert { insert, .. }) = y.peek() {
            push(
                &mut out,
                TextDelta::Retain {
                    retain: insert.chars().count(),
                    attributes: None,
                },
            );
            y.i += 1;
            continue;
        }
        match x.peek() {
            None => break,
            Some(op @ TextDelta::Insert { .. }) => {
                push(&mut out, op.clone());
                x.i += 1;
            }
            Some(op) => {
                let n = x.left().min(y.left());
                let gone = matches!(y.peek(), Some(TextDelta::Delete { .. }));
                // Символы, которые чужая правка удалила, своей операции уже не нужны.
                if !gone {
                    match op {
                        TextDelta::Retain { attributes, .. } => push(
                            &mut out,
                            TextDelta::Retain {
                                retain: n,
                                attributes: attributes.clone(),
                            },
                        ),
                        TextDelta::Delete { .. } => push(&mut out, TextDelta::Delete { delete: n }),
                        TextDelta::Insert { .. } => unreachable!("вставка разобрана выше"),
                    }
                }
                x.advance(n);
                y.advance(n);
            }
        }
    }
    // Хвостовой `retain` без разметки ничего не делает.
    if matches!(
        out.last(),
        Some(TextDelta::Retain {
            attributes: None,
            ..
        })
    ) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use loro::LoroDoc;

    fn ret(n: usize) -> TextDelta {
        TextDelta::Retain {
            retain: n,
            attributes: None,
        }
    }
    fn ins(s: &str) -> TextDelta {
        TextDelta::Insert {
            insert: s.into(),
            attributes: None,
        }
    }
    fn del(n: usize) -> TextDelta {
        TextDelta::Delete { delete: n }
    }

    fn apply(start: &str, ops: &[&[TextDelta]]) -> String {
        let doc = LoroDoc::new();
        let t = doc.get_text("t");
        t.insert(0, start).unwrap();
        for d in ops {
            t.apply_delta(d).unwrap();
        }
        t.to_string()
    }

    #[test]
    fn deleting_own_insert_survives_foreign_insert_before_it() {
        // Текст «аб»: своя вставка «XY» в 1 дала «аXYб», чужая дописала «!» в начало.
        let undo = [ret(1), del(2)];
        let foreign = [ins("!")];
        let moved = transform(&undo, &foreign);
        assert_eq!(apply("аXYб", &[&foreign, &moved]), "!аб");
    }

    #[test]
    fn foreign_insert_inside_deleted_range_stays() {
        let undo = [ret(1), del(2)];
        let foreign = [ret(2), ins("!")];
        let moved = transform(&undo, &foreign);
        assert_eq!(apply("аXYб", &[&foreign, &moved]), "а!б");
    }

    #[test]
    fn foreign_delete_shrinks_own_delete() {
        let undo = [ret(1), del(3)];
        let foreign = [ret(2), del(5)];
        let moved = transform(&undo, &foreign);
        assert_eq!(apply("0123456789", &[&foreign, &moved]), "0789");
    }

    #[test]
    fn insert_at_the_same_place_goes_after_foreign() {
        let own = [ret(1), ins("X")];
        let foreign = [ret(1), ins("Y")];
        assert_eq!(apply("аб", &[&foreign, &transform(&own, &foreign)]), "аYXб");
    }

    #[test]
    fn empty_foreign_keeps_delta_meaning() {
        let own = [ret(2), del(1)];
        assert_eq!(apply("абв", &[&transform(&own, &[])]), "аб");
    }
}
````

- [ ] **Step 4: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc`

Expected: PASS: fmt и clippy без замечаний; `ferrum-doc`: все тесты проходят, `tests/remote.rs` 26 passed

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-doc/src/document.rs crates/ferrum-doc/src/history.rs crates/ferrum-doc/src/lib.rs crates/ferrum-doc/src/page.rs crates/ferrum-doc/src/text_ot.rs crates/ferrum-doc/tests/remote.proptest-regressions crates/ferrum-doc/tests/remote.rs
git commit -F - <<'EOF'
ядро: отмена рядом с чужими правками

Отменяются только свои шаги (спека подсистемы 4, раздел 8). Операция над узлом, которого больше нет, и
перенос на место, которое стало циклом, пропускаются; шаг, от которого ничего не осталось, пропускается
целиком, и отмена идёт к следующему. Значение свойства возвращается к своему прежнему, даже если его менял
другой. Обратные дельты текста переносятся через чужие правки преобразованием операций: отмена убирает
только свои символы и не задевает чужие вставки перед ними и внутри.

Тест сходимости теперь перемешивает отмену и повтор с импортом (риск 3 спеки): зеркало совпадает с полной
сборкой после каждого шага.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 3: Редактор и ядро worker'а

Спека §8. Чужая правка не делает документ изменённым; выделение, жесты, каретка, сеанс вектора и страницы реагируют на неё. Команды и события для клиента.

**Files:**
- Create: `crates/ferrum-editor/src/remote.rs`, `crates/ferrum-wasm/src/core/sync.rs`
- Modify: `crates/ferrum-doc/src/document.rs`, `crates/ferrum-doc/src/images.rs`, `crates/ferrum-doc/src/sync.rs`, `crates/ferrum-editor/src/editor.rs`, `crates/ferrum-editor/src/lib.rs`, `crates/ferrum-wasm/src/core.rs`, `crates/ferrum-wasm/src/core/panels.rs`, `crates/ferrum-wasm/src/images.rs`, `crates/ferrum-wasm/src/protocol.rs`
- Test: `crates/ferrum-editor/tests/remote.rs` (новый), `crates/ferrum-wasm/tests/journal.rs`, `crates/ferrum-wasm/tests/journal_perf.rs`, `crates/ferrum-wasm/tests/protocol.rs`, `crates/ferrum-wasm/tests/sync.rs` (новый)

**Interfaces:**
- Consumes: курсор и `apply_remote` задачи 1.
- Produces: команды `SyncStart`, `SyncReset`, `SyncSubscribed`, `SyncAcked`, `SyncFlush`, `ApplyRemote`, `AddRemoteImage`, `Thumbnail`; события `SyncSubscribe`, `LocalImage`, `LocalUpdates`, `SyncState`, `JournalCovered`, `ImagesNeeded`, `Notice`, `Thumbnail`; `RemoteChanged`.

- [ ] **Step 1: Написать тесты**

Создать `crates/ferrum-editor/tests/remote.rs`:

````rust
//! Чужие правки в редакторе (спека подсистемы 4, раздел 8): сцена совпадает с документом, выделение
//! теряет удалённые узлы, жест над удалённым узлом отменяется, чужие свойства жест не прерывают,
//! каретка едет за чужими вставками, отмена не трогает чужого, и всё это не делает документ изменённым.

mod common;

use std::sync::Arc;

use common::text::loaded_fonts;
use common::*;
use ferrum_doc::container::{decode, encode_full};
use ferrum_doc::*;
use ferrum_editor::*;
use ferrum_render::Camera;
use kurbo::Point;

fn open(bytes: &[u8]) -> H {
    let mut ed = Editor::new(
        decode(bytes).unwrap().document,
        None,
        VIEW,
        1.0,
        Platform::Windows,
    )
    .unwrap();
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_center(Point::new(VIEW.width / 2.0, VIEW.height / 2.0));
    ed.set_camera(cam);
    ed.set_fonts(loaded_fonts());
    ed.sync_start();
    for (k, v) in ed.take_sync_subscribes() {
        ed.sync_subscribed(k, v);
    }
    ed.take_events();
    H { ed, t: 1000.0 }
}

fn pair() -> (H, H) {
    let bytes = encode_full(&Document::new()).unwrap();
    (open(&bytes), open(&bytes))
}

fn deliver(from: &mut H, to: &mut H) -> Vec<RemoteEffects> {
    for (k, v) in to.ed.take_sync_subscribes() {
        to.ed.sync_subscribed(k, v);
    }
    let mut out = Vec::new();
    for b in from.ed.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
        out.push(to.ed.apply_remote(b.doc, &[&b.bytes]).unwrap());
        from.ed.sync_acked(b.doc, &b.end);
    }
    out
}

#[test]
fn foreign_edits_update_scene_and_are_not_own_edits() {
    let (mut a, mut b) = pair();
    let id = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    let events = b.ed.take_events();
    assert!(events.contains(&EditorEvent::RemoteChanged), "{events:?}");
    assert!(
        !events.contains(&EditorEvent::DocumentChanged),
        "«изменён» чужая правка не ставит"
    );
    assert!(b.ed.scene().node(id).is_some());
    b.assert_scene_clean();
    assert!(!b.ed.document().can_undo(), "чужая правка не шаг отмены");
    a.set(id, patch(100.0, 10.0, 50.0, 50.0));
    deliver(&mut a, &mut b);
    assert_eq!(b.props(id).x, 100.0);
    b.assert_scene_clean();
}

#[test]
fn deleted_by_other_leaves_selection_silently() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    let y = a.rect(None, 100.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.ed.select(&[x, y]);
    a.ed.transact(|tx| {
        tx.delete_nodes(&[x])?;
        Ok(vec![])
    })
    .unwrap();
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert_eq!(b.sel(), vec![y]);
    let events = b.ed.take_events();
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, EditorEvent::Error(_) | EditorEvent::Notice(_))),
        "без сообщения: {events:?}"
    );
    b.assert_scene_clean();
}

#[test]
fn move_to_other_parent_keeps_selection() {
    let (mut a, mut b) = pair();
    let f = a.frame(None, 300.0, 300.0, 200.0, 200.0);
    let x = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.ed.select(&[x]);
    a.ed.transact(|tx| {
        tx.move_node(x, Some(f), 0)?;
        Ok(vec![])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    assert_eq!(b.sel(), vec![x]);
    assert_eq!(b.parent(x), Some(f));
    b.assert_scene_clean();
}

#[test]
fn drag_cancelled_when_dragged_node_deleted_by_other() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 100.0, 100.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    let steps = b.ed.step_count();
    b.drag_hold((120.0, 120.0), (220.0, 120.0), none());
    assert!(b.ed.is_busy());
    assert!(b.ed.scene().has_preview());
    a.ed.transact(|tx| {
        tx.delete_nodes(&[x])?;
        Ok(vec![])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    assert!(!b.ed.is_busy(), "жест отменён");
    b.up(220.0, 120.0, none());
    assert_eq!(b.ed.step_count(), steps, "шага нет");
    assert!(b.sel().is_empty());
    b.assert_scene_clean();
}

#[test]
fn foreign_props_do_not_interrupt_drag_and_last_write_wins_on_release() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 100.0, 100.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.drag_hold((120.0, 120.0), (220.0, 120.0), none());
    a.set(
        x,
        PropsPatch {
            y: Some(400.0),
            name: Some("чужое".into()),
            ..Default::default()
        },
    );
    deliver(&mut a, &mut b);
    assert!(b.ed.is_busy(), "жест продолжается");
    b.mv(240.0, 120.0, none());
    b.up(240.0, 120.0, none());
    assert_eq!(b.props(x).name, "чужое");
    assert_eq!(b.props(x).x, 220.0);
    deliver(&mut b, &mut a);
    assert_eq!(a.props(x), b.props(x), "сошлись");
    a.assert_scene_clean();
    b.assert_scene_clean();
}

#[test]
fn drag_in_progress_survives_foreign_create_and_delete_elsewhere() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 100.0, 100.0, 50.0, 50.0);
    let other = a.rect(None, 500.0, 500.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.drag_hold((120.0, 120.0), (220.0, 120.0), none());
    a.rect(None, 600.0, 600.0, 20.0, 20.0);
    a.ed.transact(|tx| {
        tx.delete_nodes(&[other])?;
        Ok(vec![])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    assert!(b.ed.is_busy());
    b.up(220.0, 120.0, none());
    assert_eq!(b.props(x).x, 200.0);
    b.assert_scene_clean();
}

#[test]
fn caret_follows_foreign_insert_and_session_ends_when_text_deleted() {
    let (mut a, mut b) = pair();
    let id = a.add(None, NodeKind::Text, patch(100.0, 100.0, 300.0, 100.0));
    a.ed.transact(|tx| {
        tx.insert_text(id, 0, "абвгд", None)?;
        Ok(vec![id])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    b.ed.select(&[id]);
    b.key("Enter", none());
    b.key("ArrowLeft", none());
    for _ in 0..3 {
        b.key("ArrowRight", none());
    }
    assert_eq!(b.ed.text_edit_info().unwrap().focus, 3);
    a.ed.transact(|tx| {
        tx.insert_text(id, 1, "XY", None)?;
        Ok(vec![id])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    let info = b.ed.text_edit_info().unwrap();
    assert_eq!(
        (info.anchor, info.focus),
        (5, 5),
        "курсор — за теми же символами"
    );
    a.ed.transact(|tx| {
        tx.insert_text(id, 7, "Z", None)?;
        Ok(vec![id])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    assert_eq!(
        b.ed.text_edit_info().unwrap().focus,
        5,
        "правка после курсора его не двигает"
    );
    a.ed.transact(|tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    assert!(b.ed.text_edit_info().is_none(), "сеанс правки закрыт");
    b.assert_scene_clean();
}

#[test]
fn vector_session_closes_with_notice_when_other_changes_network() {
    let (mut a, mut b) = pair();
    let square = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(100.0, 100.0),
        ],
        segments: (0..2).map(|i| VectorSegment::line(i, i + 1)).collect(),
        regions: vec![],
    };
    let id = a.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(square.clone())),
            ..patch(100.0, 100.0, 100.0, 100.0)
        },
    );
    deliver(&mut a, &mut b);
    assert!(b.ed.start_vector_edit(id));
    assert!(b.ed.is_editing_vector());
    // Чужая правка другого свойства сеанс не трогает.
    a.set(
        id,
        PropsPatch {
            name: Some("имя".into()),
            ..Default::default()
        },
    );
    deliver(&mut a, &mut b);
    assert!(b.ed.is_editing_vector());
    let mut bigger = square;
    bigger.points.push(VectorPoint::new(0.0, 100.0));
    a.set(
        id,
        PropsPatch {
            vector: Some(Arc::new(bigger.clone())),
            ..Default::default()
        },
    );
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert!(!b.ed.is_editing_vector(), "сеанс закрыт");
    let events = b.ed.take_events();
    assert!(
        events.contains(&EditorEvent::Notice(VECTOR_CHANGED_BY_OTHER)),
        "{events:?}"
    );
    assert_eq!(b.props(id).vector.as_deref(), Some(&bigger));
    b.assert_scene_clean();
}

#[test]
fn page_removed_by_other_moves_to_first_and_new_page_is_listed() {
    let (mut a, mut b) = pair();
    let added = a.ed.add_page("Вторая").unwrap();
    deliver(&mut a, &mut b);
    assert_eq!(b.ed.document().pages().len(), 2);
    b.ed.take_events();
    b.ed.set_page(added).unwrap();
    let kept = b.ed.document().pages()[0].0;
    a.ed.document_mut_for_tests().delete_page(added).unwrap();
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert_eq!(b.ed.page(), kept);
    let events = b.ed.take_events();
    assert!(events.contains(&EditorEvent::PageChanged(kept)));
    assert!(
        events.contains(&EditorEvent::Notice(PAGE_REMOVED_BY_OTHER)),
        "{events:?}"
    );
}

#[test]
fn undo_affects_only_own_edits_and_skips_deleted() {
    let (mut a, mut b) = pair();
    let own = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    let shared = a.rect(None, 100.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    let foreign = b.rect(None, 200.0, 10.0, 50.0, 50.0);
    deliver(&mut b, &mut a);
    a.set(shared, patch(101.0, 10.0, 50.0, 50.0));
    deliver(&mut a, &mut b);
    b.ed.transact(|tx| {
        tx.delete_nodes(&[shared])?;
        Ok(vec![])
    })
    .unwrap();
    deliver(&mut b, &mut a);
    // Верхний шаг a — про `shared`, его удалил b: пропуск, следующий шаг — создание `shared`, его тоже нет,
    // дальше — создание `own`.
    a.ed.execute(CommandId::Undo);
    assert!(a.ed.scene().node(own).is_none(), "свой узел отменён");
    assert!(a.ed.scene().node(foreign).is_some(), "чужой на месте");
    a.assert_scene_clean();
    deliver(&mut a, &mut b);
    assert!(b.ed.scene().node(own).is_none());
    assert_eq!(a.state(), b.state());
}

#[test]
fn scene_rebuilt_after_foreign_unknown_node_and_goes_read_only() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.ed.select(&[x]);
    let page = a.ed.page();
    a.ed.document_mut_for_tests()
        .page(page)
        .unwrap()
        .force_kind_for_tests(x, "star");
    let fx = deliver(&mut a, &mut b);
    assert!(fx.iter().any(|f| f.rebuilt && f.became_read_only), "{fx:?}");
    assert!(b.sel().is_empty());
    b.assert_scene_clean();
}

#[test]
fn missing_image_of_foreign_fill_is_requested() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    let hash = a.ed.add_image(b"png".to_vec());
    assert_eq!(a.ed.take_sync_images().len(), 1, "байты для сервера");
    assert!(a.ed.take_sync_images().is_empty());
    a.set(
        x,
        PropsPatch {
            fills: Some(vec![Fill::image(hash.clone())]),
            ..Default::default()
        },
    );
    let fx = deliver(&mut a, &mut b);
    assert!(
        fx.iter().any(|f| f.missing_images == vec![hash.clone()]),
        "{fx:?}"
    );
}
````

В `crates/ferrum-wasm/tests/journal.rs` заменить:

````rust
        bytes: ByteBuf::from(file.to_vec()),
        journal: batches.iter().cloned().map(ByteBuf::from).collect(),
    });
    let ev = c.take_events();
````

на:

````rust
        bytes: ByteBuf::from(file.to_vec()),
        journal: batches.iter().cloned().map(ByteBuf::from).collect(),
        remote: None,
    });
    let ev = c.take_events();
````

В `crates/ferrum-wasm/tests/journal.rs` заменить:

````rust
        bytes: ByteBuf::from(file),
        journal: vec![ByteBuf::new(); MAX_JOURNAL_BATCHES + 1],
    });
    let ev = c.take_events();
````

на:

````rust
        bytes: ByteBuf::from(file),
        journal: vec![ByteBuf::new(); MAX_JOURNAL_BATCHES + 1],
        remote: None,
    });
    let ev = c.take_events();
````

В `crates/ferrum-wasm/tests/journal_perf.rs` заменить:

````rust
        bytes: ByteBuf::from(file),
        journal: vec![batch.clone()],
    });
    let open = t.elapsed();
````

на:

````rust
        bytes: ByteBuf::from(file),
        journal: vec![batch.clone()],
        remote: None,
    });
    let open = t.elapsed();
````

В `crates/ferrum-wasm/tests/protocol.rs` заменить:

````rust
            bytes: bytes(b"FERRUM"),
            journal: vec![bytes(b"FERRUMJ"), bytes(b"")],
        },
        Command::FlushJournal,
````

на:

````rust
            bytes: bytes(b"FERRUM"),
            journal: vec![bytes(b"FERRUMJ"), bytes(b"")],
            remote: None,
        },
        Command::FlushJournal,
````

Создать `crates/ferrum-wasm/tests/sync.rs`:

````rust
//! Синхронизация в ядре worker'а (спека подсистемы 4, разделы 7 и 8): подписка, своя правка уходит
//! событием, чужая применяется, «изменён» снимает только подтверждение сервера, журнал снимается
//! только после него.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_doc::SyncVersion;
use ferrum_editor::{Input, Modifiers, Platform, PointerEvent};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

fn file() -> Vec<u8> {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::Snapshot {
        thumbnail: false,
        full: Some(true),
    });
    c.take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Snapshot { bytes, .. } => Some(bytes.into_vec()),
            _ => None,
        })
        .unwrap()
}

/// Ядро открыло файл на сервере; сервер знает то же, что и файл.
fn opened(bytes: &[u8]) -> Core {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::OpenWithJournal {
        bytes: ByteBuf::from(bytes.to_vec()),
        journal: vec![],
        remote: Some(true),
    });
    c.command(Command::SyncStart);
    subscribe(&mut c);
    c.take_events();
    c
}

/// Подписки, на которые сервер отвечает версией клиента.
fn subscribe(c: &mut Core) {
    c.command(Command::SyncFlush);
    for e in c.take_events() {
        if let CoreEvent::SyncSubscribe { doc, version } = e {
            c.command(Command::SyncSubscribed { doc, version });
        }
    }
}

/// Рисует прямоугольник инструментом: своя правка, как у человека.
fn rect(c: &mut Core) {
    let at = |x: f64, y: f64, t: f64| PointerEvent::new((x, y), Modifiers::NONE, t);
    c.command(Command::SetTool { tool: Tool::Rect });
    c.handle(Input::PointerDown(at(100.0, 100.0, 1000.0)));
    c.handle(Input::PointerMove(at(200.0, 200.0, 1100.0)));
    c.handle(Input::PointerUp(at(200.0, 200.0, 1200.0)));
}

fn updates(c: &mut Core) -> Vec<(String, u32, Vec<u8>)> {
    c.command(Command::SyncFlush);
    c.take_events()
        .into_iter()
        .filter_map(|e| match e {
            CoreEvent::LocalUpdates {
                doc,
                client_seq,
                bytes,
            } => Some((doc, client_seq, bytes.into_vec())),
            _ => None,
        })
        .collect()
}

/// PNG 1×1: хэш — как у `ImageStore`.
fn tiny_png() -> Vec<u8> {
    let mut pm = ferrum_render::Pixmap::new(1, 1);
    pm.data_as_u8_slice_mut()
        .copy_from_slice(&[10, 20, 30, 255]);
    pm.into_png().unwrap()
}

#[test]
fn own_edit_goes_out_foreign_comes_in_and_modified_follows_ack() {
    let bytes = file();
    let mut a = opened(&bytes);
    let mut b = opened(&bytes);
    rect(&mut a);
    let ev = a.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::Modified { modified: true })),
        "{ev:?}"
    );

    let ups = updates(&mut a);
    assert!(!ups.is_empty());
    for (doc, _, bytes) in &ups {
        b.command(Command::ApplyRemote {
            doc: doc.clone(),
            batches: vec![ByteBuf::from(bytes.clone())],
        });
    }
    let ev = b.take_events();
    assert!(
        ev.iter()
            .all(|e| !matches!(e, CoreEvent::Modified { modified: true })),
        "чужая правка не «изменён»: {ev:?}"
    );
    assert!(
        ev.iter().any(|e| matches!(e, CoreEvent::Document { .. })),
        "панели знают: {ev:?}"
    );
    assert_eq!(b.editor().scene().len(), 1);
    assert!(!b.editor().document().can_undo());

    for (doc, seq, _) in &ups {
        a.command(Command::SyncAcked {
            doc: doc.clone(),
            client_seq: *seq,
        });
    }
    let ev = a.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::Modified { modified: false })),
        "{ev:?}"
    );
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::SyncState { unacked: false })),
        "{ev:?}"
    );
}

#[test]
fn journal_is_covered_only_after_ack_and_empty_flush_writes_nothing() {
    let bytes = file();
    let mut a = opened(&bytes);
    a.command(Command::FlushJournal);
    let ev = a.take_events();
    assert!(
        ev.iter().any(|e| matches!(
            e,
            CoreEvent::Journal {
                bytes: None,
                failed: false,
                ..
            }
        )),
        "своего нет — пачки нет: {ev:?}"
    );
    rect(&mut a);
    a.command(Command::FlushJournal);
    let ev = a.take_events();
    let batch = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::Journal {
                batch,
                bytes: Some(_),
                ..
            } => Some(*batch),
            _ => None,
        })
        .unwrap();
    let ups = updates(&mut a);
    assert!(a
        .take_events()
        .iter()
        .all(|e| !matches!(e, CoreEvent::JournalCovered { batch: b } if *b >= batch)));
    for (doc, seq, _) in &ups {
        a.command(Command::SyncAcked {
            doc: doc.clone(),
            client_seq: *seq,
        });
    }
    let ev = a.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::JournalCovered { batch: b } if *b == batch)),
        "{ev:?}"
    );
}

#[test]
fn reconnect_resends_what_was_not_acked() {
    let bytes = file();
    let mut a = opened(&bytes);
    let server: Vec<(String, Vec<u8>)> = {
        let mut x = opened(&bytes);
        x.command(Command::SyncReset);
        x.command(Command::SyncFlush);
        x.take_events()
            .into_iter()
            .filter_map(|e| match e {
                CoreEvent::SyncSubscribe { doc, version } => Some((doc, version.into_vec())),
                _ => None,
            })
            .collect()
    };
    rect(&mut a);
    let first = updates(&mut a);
    assert!(!first.is_empty());
    assert!(updates(&mut a).is_empty(), "дважды не шлём");
    a.command(Command::SyncReset);
    a.command(Command::SyncFlush);
    a.take_events();
    for (doc, version) in &server {
        assert!(SyncVersion::decode(version).is_some());
        a.command(Command::SyncSubscribed {
            doc: doc.clone(),
            version: ByteBuf::from(version.clone()),
        });
    }
    let again = updates(&mut a);
    assert_eq!(first.len(), again.len());
    assert_eq!(first[0].2, again[0].2, "те же байты: повтор безвреден");
    assert_ne!(
        first[0].1, again[0].1,
        "новый номер: ответ на старый игнорируется"
    );
}

#[test]
fn view_only_receives_and_sends_nothing() {
    let bytes = file();
    let mut a = opened(&bytes);
    let mut b = opened(&bytes);
    b.command(Command::SetViewOnly { on: true });
    b.take_events();
    rect(&mut a);
    for (doc, _, bytes) in updates(&mut a) {
        b.command(Command::ApplyRemote {
            doc,
            batches: vec![ByteBuf::from(bytes)],
        });
    }
    assert_eq!(b.editor().scene().len(), 1);
    assert!(updates(&mut b).is_empty());
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    let bytes = file();
    let mut a = opened(&bytes);
    a.command(Command::ApplyRemote {
        doc: "../x".into(),
        batches: vec![],
    });
    a.command(Command::SyncSubscribed {
        doc: "root".into(),
        version: ByteBuf::from(vec![1, 2, 3]),
    });
    let page = a.editor().page().to_string();
    a.command(Command::ApplyRemote {
        doc: page,
        batches: vec![ByteBuf::from(b"garbage".to_vec())],
    });
    a.command(Command::SyncAcked {
        doc: "root".into(),
        client_seq: 777,
    });
    let errors = a
        .take_events()
        .into_iter()
        .filter(|e| matches!(e, CoreEvent::Error { .. }))
        .count();
    assert_eq!(errors, 3);
}

#[test]
fn foreign_image_reference_is_requested_and_bytes_are_checked() {
    let bytes = file();
    let mut a = opened(&bytes);
    let mut b = opened(&bytes);
    rect(&mut a);
    for (doc, _, bytes) in updates(&mut a) {
        b.command(Command::ApplyRemote {
            doc,
            batches: vec![ByteBuf::from(bytes)],
        });
    }
    a.take_events();
    let png = tiny_png();
    a.command(Command::PlaceImage {
        bytes: ByteBuf::from(png.clone()),
    });
    a.take_events();
    let at = PointerEvent::new((300.0, 300.0), Modifiers::NONE, 5000.0);
    a.handle(Input::PointerDown(at));
    a.handle(Input::PointerUp(PointerEvent::new(
        (300.0, 300.0),
        Modifiers::NONE,
        5100.0,
    )));
    a.command(Command::SyncFlush);
    let ev = a.take_events();
    let image = ev
        .iter()
        .position(|e| matches!(e, CoreEvent::LocalImage { .. }));
    let update = ev
        .iter()
        .position(|e| matches!(e, CoreEvent::LocalUpdates { .. }));
    assert!(
        image.unwrap() < update.unwrap(),
        "картинка раньше правки: {ev:?}"
    );
    for e in ev {
        if let CoreEvent::LocalUpdates { doc, bytes, .. } = e {
            b.command(Command::ApplyRemote {
                doc,
                batches: vec![bytes],
            });
        }
    }
    let ev = b.take_events();
    let hash = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::ImagesNeeded { hashes } => hashes.first().cloned(),
            _ => None,
        })
        .expect("картинку попросили");
    b.command(Command::AddRemoteImage {
        hash: hash.clone(),
        bytes: ByteBuf::from(b"chto-to drugoe".to_vec()),
    });
    assert!(
        b.editor().document().images.get(&hash).is_none(),
        "чужой хэш не проходит"
    );
    b.command(Command::AddRemoteImage {
        hash: hash.clone(),
        bytes: ByteBuf::from(png),
    });
    assert!(b.editor().document().images.get(&hash).is_some());
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-editor --test remote`
Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-wasm --test sync`
Expected: FAIL — `ferrum-editor`: нет `take_sync_images`, 17 ошибок компиляции; `ferrum-wasm`: нет `Command::AddRemoteImage`, 33 ошибки компиляции

- [ ] **Step 3: Реализация**

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    }

    /// Забывает историю отмены и повтора: документ, собранный программно (пример,
    /// шаблон), открывается как только что прочитанный файл — без шага «создать всё».
````

на:

````rust
    }

    /// Изменения страницы, ещё не забранные `take_changes`.
    pub fn pending_changes(&self, page: PageId) -> Option<&Changes> {
        self.changes.get(&page)
    }

    /// Забывает историю отмены и повтора: документ, собранный программно (пример,
    /// шаблон), открывается как только что прочитанный файл — без шага «создать всё».
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            };
            let Some(doc) = doc else { continue };
            let vv = doc.oplog_vv();
            if c.sent.includes_vv(&vv) {
                continue;
            }
            for (bytes, end) in sync::split_updates(&doc, &c.sent, limit)? {
                out.push(SyncBatch {
                    doc: *key,
````

на:

````rust
            };
            let Some(doc) = doc else { continue };
            if c.sent.includes_vv(&doc.oplog_vv()) {
                continue;
            }
            for (bytes, end) in
                sync::split_updates(&doc, &c.sent, limit, sync::MAX_SYNC_FLUSH_BYTES)?
            {
                // Отправлено ровно то, что собрано: остаток заберёт следующий вызов.
                sync::merge(&mut c.sent, &end);
                out.push(SyncBatch {
                    doc: *key,
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
                });
            }
            sync::merge(&mut c.sent, &vv);
        }
        Ok(out)
````

на:

````rust
                });
            }
        }
        Ok(out)
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    }

    /// Текущие версии документов с подпиской: отметка пачки журнала, которую потом можно снять.
    pub fn sync_versions(&self) -> Vec<(DocKey, SyncVersion)> {
        let Some(s) = &self.sync else {
            return Vec::new();
        };
        s.docs
            .keys()
            .filter_map(|k| match k {
                DocKey::Root => Some((*k, SyncVersion(self.root.oplog_vv()))),
                DocKey::Page(id) => self.loaded.get(id).map(|p| (*k, SyncVersion(p.oplog_vv()))),
            })
            .collect()
    }

````

на:

````rust
    }

    /// Текущие версии документов: отметка пачки журнала, которую потом можно снять.
    pub fn sync_versions(&self) -> Vec<(DocKey, SyncVersion)> {
        if self.sync.is_none() {
            return Vec::new();
        }
        let mut out = vec![(DocKey::Root, SyncVersion(self.root.oplog_vv()))];
        out.extend(
            self.loaded
                .iter()
                .map(|(id, p)| (DocKey::Page(*id), SyncVersion(p.oplog_vv()))),
        );
        out
    }

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            return false;
        };
        let behind = |key: DocKey, vv: &VersionVector| match s.docs.get(&key) {
            Some(c) => !c.acked.includes_vv(vv),
            None => false,
        };
        behind(DocKey::Root, &self.root.oplog_vv())
````

на:

````rust
            return false;
        };
        // Сервера ещё не спрашивали: своё ли там, неизвестно, значит, не подтверждено.
        let behind = |key: DocKey, vv: &VersionVector| match s.docs.get(&key) {
            Some(c) => !c.acked.includes_vv(vv),
            None => true,
        };
        behind(DocKey::Root, &self.root.oplog_vv())
````

В `crates/ferrum-doc/src/images.rs` заменить:

````rust
    /// нет в файле.
    fresh: Mutex<Vec<String>>,
}

````

на:

````rust
    /// нет в файле.
    fresh: Mutex<Vec<String>>,
    /// Добавленные, чьи байты сервер ещё не видел (`take_sync_fresh`): журнал считает их отдельно.
    sync_fresh: Vec<String>,
}

````

В `crates/ferrum-doc/src/images.rs` заменить:

````rust
            e.insert(bytes);
            self.fresh_list().push(hash.clone());
        }
        hash
````

на:

````rust
            e.insert(bytes);
            self.fresh_list().push(hash.clone());
            self.sync_fresh.push(hash.clone());
        }
        hash
````

В `crates/ferrum-doc/src/images.rs` заменить:

````rust
    }

    /// Изображения, добавленные с прошлого вызова, — для пачки журнала.
    pub(crate) fn take_fresh(&mut self) -> Vec<(String, Vec<u8>)> {
````

на:

````rust
    }

    /// Новые изображения для сервера: они уходят раньше правки, которая на них ссылается.
    pub fn take_sync_fresh(&mut self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.sync_fresh)
            .into_iter()
            .filter_map(|h| {
                let bytes = self.by_hash.get(&h)?.clone();
                Some((h, bytes))
            })
            .collect()
    }

    /// Байты, скачанные по чужой ссылке: хэш проверяется, чужое имя не пройдёт. Сервер их уже знает.
    pub fn add_remote(&mut self, hash: &str, bytes: Vec<u8>) -> bool {
        if blake3::hash(&bytes).to_hex().as_str() != hash {
            return false;
        }
        self.by_hash.insert(hash.to_string(), bytes);
        true
    }

    /// Изображения, добавленные с прошлого вызова, — для пачки журнала.
    pub(crate) fn take_fresh(&mut self) -> Vec<(String, Vec<u8>)> {
````

В `crates/ferrum-doc/src/sync.rs` заменить:

````rust
/// документ не набирается, а чужой кадр не должен задавать размер разбора.
pub const MAX_VERSION_PEERS: usize = 65_536;

/// Единица синхронизации: корневой документ или страница.
````

на:

````rust
/// документ не набирается, а чужой кадр не должен задавать размер разбора.
pub const MAX_VERSION_PEERS: usize = 65_536;
/// За один вызов `take_sync_updates`: вставка на сотни мегабайт идёт порциями, по мере места в сокете.
pub const MAX_SYNC_FLUSH_BYTES: usize = 32 * 1024 * 1024;

/// Единица синхронизации: корневой документ или страница.
````

В `crates/ferrum-doc/src/sync.rs` заменить:

````rust
}

/// Обновления `from` … текущая версия кусками не больше `limit` байт. Куски идут в порядке
/// счётчиков внутри пира, старые сеансы (пиры прошлых открытий) раньше нынешнего: правки нынешнего
/// могут зависеть от них.
pub(crate) fn split_updates(
    doc: &LoroDoc,
    from: &VersionVector,
    limit: usize,
) -> DocResult<Vec<(Vec<u8>, VersionVector)>> {
    let vv = doc.oplog_vv();
    let whole = doc.export(ExportMode::updates(from))?;
    if whole.len() <= limit {
        return Ok(vec![(whole, vv)]);
    }
    let own = doc.peer_id();
    let mut spans: Vec<(u64, i32, i32)> = vv
````

на:

````rust
}

/// Операций за раз, больше которых незачем пробовать собирать одним куском: на большой вставке целый
/// экспорт стоил бы сотни мегабайт на каждый вызов.
const FIRST_STEP_OPS: i32 = 200_000;

/// Обновления `from` … текущая версия кусками не больше `limit` байт, всего — примерно до `budget` байт
/// за вызов: остальное заберёт следующий. Куски идут в порядке счётчиков внутри пира, старые сеансы
/// (пиры прошлых открытий) раньше нынешнего: правки нынешнего могут зависеть от них. Второе значение
/// куска — версия, которую сервер узнает, приняв его.
pub(crate) fn split_updates(
    doc: &LoroDoc,
    from: &VersionVector,
    limit: usize,
    budget: usize,
) -> DocResult<Vec<(Vec<u8>, VersionVector)>> {
    let vv = doc.oplog_vv();
    let own = doc.peer_id();
    let mut spans: Vec<(u64, i32, i32)> = vv
````

В `crates/ferrum-doc/src/sync.rs` заменить:

````rust
        })
        .collect();
    spans.sort_by_key(|(p, ..)| (*p == own, *p));
    let mut out = Vec::new();
    let mut end = from.clone();
    for (peer, start, stop) in spans {
        let mut at = start;
        let mut step = (stop - start).max(1);
        while at < stop {
            let upto = (at + step).min(stop);
            let span = loro::IdSpan::new(peer, at, upto);
````

на:

````rust
        })
        .collect();
    let total: i64 = spans.iter().map(|(_, a, b)| (*b - *a) as i64).sum();
    if total <= FIRST_STEP_OPS as i64 {
        let whole = doc.export(ExportMode::updates(from))?;
        if whole.len() <= limit {
            return Ok(vec![(whole, vv)]);
        }
    }
    spans.sort_by_key(|(p, ..)| (*p == own, *p));
    let mut out = Vec::new();
    let mut end = from.clone();
    let mut size = 0usize;
    for (peer, start, stop) in spans {
        let mut at = start;
        let mut step = (stop - start).clamp(1, FIRST_STEP_OPS);
        while at < stop {
            if size >= budget {
                return Ok(out);
            }
            let upto = (at + step).min(stop);
            let span = loro::IdSpan::new(peer, at, upto);
````

В `crates/ferrum-doc/src/sync.rs` заменить:

````rust
            }
            end.extend_to_include_last_id(ID::new(peer, upto - 1));
            out.push((bytes, end.clone()));
            at = upto;
````

на:

````rust
            }
            end.extend_to_include_last_id(ID::new(peer, upto - 1));
            size += bytes.len();
            out.push((bytes, end.clone()));
            at = upto;
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
    /// Документ изменён правкой, отменой или повтором.
    DocumentChanged,
    PageChanged(PageId),
    /// Изменился список страниц или их порядок (отмена и повтор их шагов).
````

на:

````rust
    /// Документ изменён правкой, отменой или повтором.
    DocumentChanged,
    /// Документ изменил другой участник: сцена и панели обновляются, «изменён» — нет.
    RemoteChanged,
    /// Чужая правка перестроила сцену целиком: кэши панелей нужны заново.
    RemoteRebuilt,
    /// Сообщение для человека, не ошибка.
    Notice(&'static str),
    PageChanged(PageId),
    /// Изменился список страниц или их порядок (отмена и повтор их шагов).
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
pub mod presets;
pub mod props_clip;
pub mod scale;
pub mod selection;
````

на:

````rust
pub mod presets;
pub mod props_clip;
mod remote;
pub mod scale;
pub mod selection;
````

В `crates/ferrum-editor/src/lib.rs` заменить:

````rust
};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use text::{fit_size, MissingFont, TextError, TextPending};
pub use text_edit::{EnterAt, TextEditInfo, CARET_BLINK_MS};
````

на:

````rust
};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use remote::{RemoteEffects, PAGE_REMOVED_BY_OTHER, VECTOR_CHANGED_BY_OTHER};
pub use text::{fit_size, MissingFont, TextError, TextPending};
pub use text_edit::{EnterAt, TextEditInfo, CARET_BLINK_MS};
````

Создать `crates/ferrum-editor/src/remote.rs`:

````rust
//! Чужие правки в редакторе (спека подсистемы 4, раздел 8): сцена обновляется обычным путём, выделение
//! теряет удалённые узлы, жест над удалённым узлом отменяется, каретка следует за чужими вставками,
//! сеанс правки вектора закрывается, если другой изменил его сеть. Чужая правка не шаг отмены и не
//! делает документ изменённым.

use ferrum_doc::{DocKey, DocResult, RemoteOutcome, SyncBatch, SyncVersion};
use ferrum_scene::Scene;

use crate::editor::{install_text_outlines, Editor, EditorEvent};
use crate::tools::sync;

/// Текст уведомления: сеть вектора изменил другой участник (`VectorEdit` закрыт).
pub const VECTOR_CHANGED_BY_OTHER: &str =
    "Другой участник изменил этот контур: правка контура закрыта";
/// Страницу, на которой вы были, удалил другой участник.
pub const PAGE_REMOVED_BY_OTHER: &str = "Другой участник удалил эту страницу";

/// Что осталось сделать вызывающему.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemoteEffects {
    /// Часть байтов не применена.
    pub failed: bool,
    /// Правкам не хватает предыдущих, подождут.
    pub pending: bool,
    /// Узлы ссылаются на эти картинки, а байтов нет: их надо скачать.
    pub missing_images: Vec<String>,
    /// Файл только что стал только для чтения: другой правит более новой версией.
    pub became_read_only: bool,
    /// Корень: мог смениться список страниц или имя документа.
    pub root_changed: bool,
    /// Правимая сцена построена заново.
    pub rebuilt: bool,
}

impl Editor {
    /// Файл на сервере: журналу не хватает правок, которые придут при догонянии.
    pub fn set_catches_up(&mut self) {
        self.doc.set_catches_up();
    }

    pub fn sync_start(&mut self) {
        self.doc.sync_start();
    }

    pub fn sync_reset(&mut self) {
        self.doc.sync_reset();
    }

    pub fn take_sync_subscribes(&mut self) -> Vec<(DocKey, SyncVersion)> {
        self.doc.take_sync_subscribes()
    }

    pub fn sync_subscribed(&mut self, doc: DocKey, server: SyncVersion) {
        self.doc.sync_subscribed(doc, server);
    }

    pub fn take_sync_updates(&mut self, limit: usize) -> DocResult<Vec<SyncBatch>> {
        self.doc.take_sync_updates(limit)
    }

    pub fn sync_acked(&mut self, doc: DocKey, end: &SyncVersion) {
        self.doc.sync_acked(doc, end);
    }

    pub fn sync_versions(&self) -> Vec<(DocKey, SyncVersion)> {
        self.doc.sync_versions()
    }

    pub fn sync_covers(&self, marks: &[(DocKey, SyncVersion)]) -> bool {
        self.doc.sync_covers(marks)
    }

    pub fn sync_unacked(&self) -> bool {
        self.doc.sync_unacked()
    }

    /// Новые картинки, байты которых сервер ещё не видел: `take_sync_updates` ссылается на них.
    pub fn take_sync_images(&mut self) -> Vec<(String, Vec<u8>)> {
        self.doc.images.take_sync_fresh()
    }

    /// Картинка по чужой ссылке: хэш сверяется, чужое имя не пройдёт. Сцена увидит её на следующем кадре.
    pub fn add_remote_image(&mut self, hash: &str, bytes: Vec<u8>) -> bool {
        self.doc.images.add_remote(hash, bytes)
    }

    /// Применяет чужие правки. Сбой разбора байтов не страшен (`failed`), редактор и документ целы.
    pub fn apply_remote(&mut self, doc: DocKey, batches: &[&[u8]]) -> DocResult<RemoteEffects> {
        let pages = self.doc.pages();
        let guides = self.doc.guides_revision();
        let outcome = self.doc.apply_remote(doc, batches)?;
        self.follow_text(&outcome);
        let on_this_page = outcome.page == Some(self.page);
        if on_this_page && outcome.rebuilt {
            self.rebuild_scene_after_remote();
        } else if on_this_page {
            self.leave_what_was_deleted();
        }
        if on_this_page {
            self.close_vector_edit_if_changed(&outcome);
        }
        let before = self.out.events.len();
        sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        // Правка другого — не своя: документ не становится изменённым.
        for e in &mut self.out.events[before..] {
            if matches!(e, EditorEvent::DocumentChanged) {
                *e = EditorEvent::RemoteChanged;
            }
        }
        let list_changed = self.doc.pages() != pages;
        if list_changed {
            self.out.events.push(EditorEvent::PagesChanged);
        }
        if outcome.root_changed {
            self.follow_pages(&outcome);
            if list_changed || self.doc.guides_revision() != guides {
                self.out.events.push(EditorEvent::RemoteChanged);
                self.out.events.push(EditorEvent::NeedsRedraw);
            }
        }
        self.emit();
        Ok(RemoteEffects {
            failed: outcome.failed,
            pending: outcome.pending,
            missing_images: outcome.missing_images,
            became_read_only: outcome.became_read_only,
            root_changed: outcome.root_changed,
            rebuilt: outcome.rebuilt && on_this_page,
        })
    }

    /// Каретка и выделение текста едут за чужими вставками; склейка ввода рвётся (её рвёт и история).
    fn follow_text(&mut self, outcome: &RemoteOutcome) {
        let Some(t) = &mut self.text_edit else {
            return;
        };
        for (node, shift) in &outcome.text {
            if *node == t.id && t.draft.is_none() {
                t.anchor = shift.map(t.anchor);
                t.focus.index = shift.map(t.focus.index);
                t.merge = None;
                t.goal_x = None;
            }
        }
    }

    /// Удалённые узлы тихо уходят из выделения; жест над ними отменяется, остальным правкам
    /// жест не мешает (при отпускании побеждает последняя запись по правилам Loro).
    fn leave_what_was_deleted(&mut self) {
        let Some(changes) = self.doc.pending_changes(self.page) else {
            return;
        };
        if changes.deleted.is_empty() {
            return;
        }
        // Узел мог вернуться той же пачкой (отмена удаления у другого): сверка с документом.
        let page = self.page;
        let deleted: Vec<_> = changes.deleted.clone();
        let Ok(doc) = self.doc.page(page) else {
            return;
        };
        let gone: std::collections::HashSet<_> =
            deleted.into_iter().filter(|n| !doc.contains(*n)).collect();
        if gone.is_empty() {
            return;
        }
        if self.is_busy() && self.selection.iter().any(|n| gone.contains(n)) {
            self.cancel_gesture();
        }
        self.selection.retain(|n| !gone.contains(n));
    }

    /// Зеркало страницы построено заново: сцена — тоже, как при смене страницы, на месте.
    fn rebuild_scene_after_remote(&mut self) {
        self.cancel_gesture();
        let Ok(page) = self.doc.page(self.page) else {
            return;
        };
        let scene = Scene::from_page(page);
        let alive: std::collections::HashSet<_> = page.walk().into_iter().collect();
        self.scene = scene;
        install_text_outlines(&mut self.scene, &self.fonts, &self.outlines_missed);
        self.scene_epoch += 1;
        self.selection.retain(|n| alive.contains(n));
        self.out.events.push(EditorEvent::RemoteRebuilt);
        self.out.events.push(EditorEvent::RemoteChanged);
        self.out.events.push(EditorEvent::NeedsRedraw);
    }

    /// Сеть — неделимое значение: последняя запись побеждает. Незаписанные точки сеанса пропадают,
    /// и человек об этом узнаёт.
    fn close_vector_edit_if_changed(&mut self, outcome: &RemoteOutcome) {
        let edited = self.vector_edit.as_ref().and_then(|v| v.id);
        if edited.is_some_and(|id| outcome.vectors.contains(&id)) {
            self.vector_edit = None;
            self.scene.clear_preview();
            self.out
                .events
                .push(EditorEvent::Notice(VECTOR_CHANGED_BY_OTHER));
        }
    }

    /// Страницу, на которой человек, удалил другой: уходим на первую. Все страницы удалены одновременно
    /// разными людьми: файл без страниц не открыть, появляется новая **(поведение)**.
    fn follow_pages(&mut self, outcome: &RemoteOutcome) {
        if self.doc.pages().is_empty() {
            let _ = self.doc.add_page("Страница 1");
        }
        if outcome.removed_pages.contains(&self.page) {
            if let Some((first, _)) = self.doc.pages().first().cloned() {
                self.text_edit = None;
                self.vector_edit = None;
                self.scene.clear_preview();
                let _ = self.set_page(first);
                self.out
                    .events
                    .push(EditorEvent::Notice(PAGE_REMOVED_BY_OTHER));
            }
        }
    }
}
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
mod fonts;
mod panels;
mod text;

````

на:

````rust
mod fonts;
mod panels;
mod sync;
mod text;

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    modified: bool,
    modified_sent: Option<bool>,
    /// Ревизия правок: растёт с каждой правкой и заменой документа. Снимок несёт её,
    /// а `MarkSaved` снимает `modified`, только если с тех пор она не сменилась.
````

на:

````rust
    modified: bool,
    modified_sent: Option<bool>,
    /// Синхронизация с сервером: не включена у локальных файлов.
    sync: sync::SyncCore,
    /// Ревизия правок: растёт с каждой правкой и заменой документа. Снимок несёт её,
    /// а `MarkSaved` снимает `modified`, только если с тех пор она не сменилась.
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            | Command::Open { .. }
            | Command::OpenWithJournal { .. }
            | Command::Demo { .. }
            | Command::DemoText { .. }
````

на:

````rust
            | Command::Open { .. }
            | Command::OpenWithJournal { .. }
            | Command::SyncStart
            | Command::SyncReset
            | Command::SyncSubscribed { .. }
            | Command::SyncAcked { .. }
            | Command::SyncFlush
            | Command::ApplyRemote { .. }
            | Command::AddRemoteImage { .. }
            | Command::Thumbnail
            | Command::Demo { .. }
            | Command::DemoText { .. }
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            revision: 0,
            journal_batch: 0,
            fonts,
            font_state: fonts::FontState::default(),
````

на:

````rust
            revision: 0,
            journal_batch: 0,
            sync: sync::SyncCore::default(),
            fonts,
            font_state: fonts::FontState::default(),
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        }
        self.images = DocImages::default();
        self.selection = None;
        self.layers.reset();
````

на:

````rust
        }
        self.images = DocImages::default();
        self.sync = sync::SyncCore::default();
        self.selection = None;
        self.layers.reset();
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    /// снимком — только если есть что записать.
    fn flush_journal(&mut self, reply: bool) {
        match self.editor.take_journal() {
            Ok(Some(bytes)) => {
                self.journal_batch += 1;
                self.events.push(CoreEvent::Journal {
                    batch: self.journal_batch as f64,
````

на:

````rust
    /// снимком — только если есть что записать.
    fn flush_journal(&mut self, reply: bool) {
        // Файл на сервере: своего неподтверждённого нет — журнал незачем писать, сервер всё знает
        // (чужие правки, идущие потоком, в браузер не складываются).
        if self.sync.on && !self.editor.sync_unacked() {
            self.editor.mark_journal();
            if reply {
                self.events.push(CoreEvent::Journal {
                    batch: self.journal_batch as f64,
                    bytes: None,
                    failed: false,
                });
            }
            return;
        }
        match self.editor.take_journal() {
            Ok(Some(bytes)) => {
                self.journal_batch += 1;
                if self.sync.on {
                    self.sync
                        .marks
                        .push_back((self.journal_batch, self.editor.sync_versions()));
                }
                self.events.push(CoreEvent::Journal {
                    batch: self.journal_batch as f64,
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
    /// `Opened`; восстановленное ещё не в файле — документ изменён.
    pub fn open_with_journal(&mut self, bytes: &[u8], journal: &[ByteBuf]) {
        if journal.len() > MAX_JOURNAL_BATCHES {
            self.error(format!(
````

на:

````rust
    /// `Opened`; восстановленное ещё не в файле — документ изменён.
    pub fn open_with_journal(&mut self, bytes: &[u8], journal: &[ByteBuf]) {
        self.open_journaled(bytes, journal, false);
    }

    fn open_journaled(&mut self, bytes: &[u8], journal: &[ByteBuf], remote: bool) {
        if journal.len() > MAX_JOURNAL_BATCHES {
            self.error(format!(
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        };
        let batches: Vec<&[u8]> = journal.iter().map(|b| b.as_slice()).collect();
        let replayed = opened.document.apply_journal(&batches);
        opened.warnings.extend(replayed.warnings);
````

на:

````rust
        };
        let batches: Vec<&[u8]> = journal.iter().map(|b| b.as_slice()).collect();
        if remote {
            opened.document.set_catches_up();
        }
        let replayed = opened.document.apply_journal(&batches);
        opened.warnings.extend(replayed.warnings);
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            | Command::ReplaceMissingFont { .. }
            | Command::TestCreateText { .. }) => self.font_command(cmd),
            Command::OpenWithJournal { bytes, journal } => self.open_with_journal(&bytes, &journal),
            Command::FlushJournal => {
                self.settle_waiting_fonts();
                self.flush_journal(true);
            }
            // Правка текста; ждущая шрифтов — в очередь.
````

на:

````rust
            | Command::ReplaceMissingFont { .. }
            | Command::TestCreateText { .. }) => self.font_command(cmd),
            Command::OpenWithJournal {
                bytes,
                journal,
                remote,
            } => self.open_journaled(&bytes, &journal, remote.unwrap_or(false)),
            Command::SyncStart => self.sync_start(),
            Command::SyncReset => self.sync_reset(),
            Command::SyncSubscribed { doc, version } => self.sync_subscribed(&doc, &version),
            Command::SyncAcked { doc, client_seq } => self.sync_acked(&doc, client_seq),
            Command::SyncFlush => self.sync_flush(),
            Command::ApplyRemote { doc, batches } => self.apply_remote(&doc, &batches),
            Command::AddRemoteImage { hash, bytes } => {
                self.add_remote_image(&hash, bytes.into_vec())
            }
            Command::Thumbnail => self.push_thumbnail(),
            Command::FlushJournal => {
                self.settle_waiting_fonts();
                self.flush_journal(true);
                self.sync_note();
            }
            // Правка текста; ждущая шрифтов — в очередь.
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                    self.events.push(CoreEvent::Cursor { cursor, angle });
                }
                EditorEvent::PageChanged(p) => {
                    self.events.push(CoreEvent::Page { id: p.to_string() });
````

на:

````rust
                    self.events.push(CoreEvent::Cursor { cursor, angle });
                }
                EditorEvent::RemoteChanged => {
                    document = true;
                    self.ui.layers = true;
                    self.ui.properties = true;
                }
                EditorEvent::RemoteRebuilt => {
                    self.layers.reset();
                    self.summary = None;
                    self.ui.layers = true;
                    self.ui.properties = true;
                }
                EditorEvent::Notice(message) => self.events.push(CoreEvent::Notice {
                    message: message.to_string(),
                }),
                EditorEvent::PageChanged(p) => {
                    self.events.push(CoreEvent::Page { id: p.to_string() });
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                .retain(|e| !matches!(e, CoreEvent::Document { .. }));
            self.push_document();
        }
        let properties = self.ui.properties;
````

на:

````rust
                .retain(|e| !matches!(e, CoreEvent::Document { .. }));
            self.push_document();
            // Своя правка: индикатор и «Сохранить» знают сразу, что сервер её ещё не подтвердил.
            self.sync_note();
        }
        let properties = self.ui.properties;
````

В `crates/ferrum-wasm/src/core/panels.rs` заменить:

````rust
            | Command::ReplaceMissingFont { .. }
            | Command::TestCreateText { .. }
            | Command::TextEditStart { .. }
            | Command::TextEditEnd
````

на:

````rust
            | Command::ReplaceMissingFont { .. }
            | Command::TestCreateText { .. }
            | Command::SyncStart
            | Command::SyncReset
            | Command::SyncSubscribed { .. }
            | Command::SyncAcked { .. }
            | Command::SyncFlush
            | Command::ApplyRemote { .. }
            | Command::AddRemoteImage { .. }
            | Command::Thumbnail
            | Command::TextEditStart { .. }
            | Command::TextEditEnd
````

Создать `crates/ferrum-wasm/src/core/sync.rs`:

````rust
//! Синхронизация в ядре (спека подсистемы 4, разделы 7 и 8): подписки и свои правки уходят событиями в
//! `sync.worker`, чужие приходят командами. Журнал правок снимается только после подтверждения сервера.

use std::collections::{HashMap, VecDeque};

use ferrum_doc::{DocKey, SyncVersion, MAX_SYNC_BATCH_BYTES};
use serde_bytes::ByteBuf;

use super::{panels, Core};
use crate::protocol::CoreEvent;

/// Сообщение: другой участник правит более новой версией, и файл стал только для чтения.
pub const NEWER_VERSION_EDITS: &str =
    "Другой участник правит файл более новой версией Ferrum. Файл открыт только для чтения — обновите страницу";

#[derive(Default)]
pub(crate) struct SyncCore {
    pub on: bool,
    next_seq: u32,
    /// Пачки без ответа сервера: номер — документ и версия, которую он узнает.
    inflight: HashMap<u32, (DocKey, SyncVersion)>,
    /// Пачка журнала и версии документов на момент её записи. 0 — журнал прошлых сеансов.
    pub marks: VecDeque<(u64, Vec<(DocKey, SyncVersion)>)>,
    unacked_sent: Option<bool>,
}

impl Core {
    pub(super) fn sync_start(&mut self) {
        self.editor.sync_start();
        self.sync.on = true;
        // Всё, что было открыто, вместе с журналом прошлых сеансов — отметка 0.
        self.sync.marks.push_back((0, self.editor.sync_versions()));
    }

    pub(super) fn sync_reset(&mut self) {
        self.sync.inflight.clear();
        self.editor.sync_reset();
    }

    pub(super) fn sync_subscribed(&mut self, doc: &str, version: &[u8]) {
        let (Some(key), Some(v)) = (DocKey::parse(doc), SyncVersion::decode(version)) else {
            self.error("Сервер прислал неверную версию документа");
            return;
        };
        self.editor.sync_subscribed(key, v);
        self.sync_note();
    }

    pub(super) fn sync_acked(&mut self, doc: &str, client_seq: u32) {
        let Some((key, end)) = self.sync.inflight.remove(&client_seq) else {
            return;
        };
        // Кадр не доверенный: чужой документ не подтверждает эту пачку.
        if key.to_string() != doc {
            return;
        }
        self.editor.sync_acked(key, &end);
        self.sync_note();
    }

    /// Новые подписки, картинки и правки. Картинки — раньше правок, которые на них ссылаются.
    pub(super) fn sync_flush(&mut self) {
        if !self.sync.on {
            return;
        }
        for (doc, version) in self.editor.take_sync_subscribes() {
            self.events.push(CoreEvent::SyncSubscribe {
                doc: doc.to_string(),
                version: ByteBuf::from(version.encode()),
            });
        }
        let batches = match self.editor.take_sync_updates(MAX_SYNC_BATCH_BYTES) {
            Ok(b) => b,
            Err(e) => {
                self.error(format!("Не удалось собрать правки для сервера: {e}"));
                return;
            }
        };
        if !batches.is_empty() {
            for (hash, bytes) in self.editor.take_sync_images() {
                self.events.push(CoreEvent::LocalImage {
                    hash,
                    bytes: ByteBuf::from(bytes),
                });
            }
        }
        for b in batches {
            self.sync.next_seq = self.sync.next_seq.wrapping_add(1);
            let client_seq = self.sync.next_seq;
            self.sync.inflight.insert(client_seq, (b.doc, b.end));
            self.events.push(CoreEvent::LocalUpdates {
                doc: b.doc.to_string(),
                client_seq,
                bytes: ByteBuf::from(b.bytes),
            });
        }
        self.sync_note();
    }

    /// Что изменилось от ответа сервера: журнал, который можно снять, и «всё подтверждено».
    pub(super) fn sync_note(&mut self) {
        if !self.sync.on {
            return;
        }
        let mut covered = None;
        while let Some((batch, marks)) = self.sync.marks.front() {
            if !self.editor.sync_covers(marks) {
                break;
            }
            covered = Some(*batch);
            self.sync.marks.pop_front();
        }
        if let Some(batch) = covered {
            self.events.push(CoreEvent::JournalCovered {
                batch: batch as f64,
            });
        }
        let unacked = self.editor.sync_unacked();
        if self.sync.unacked_sent != Some(unacked) {
            self.sync.unacked_sent = Some(unacked);
            self.events.push(CoreEvent::SyncState { unacked });
        }
        // «Изменён» значит «не на сервере».
        if !unacked && self.modified {
            self.modified = false;
        }
    }

    pub(super) fn apply_remote(&mut self, doc: &str, batches: &[ByteBuf]) {
        let Some(key) = DocKey::parse(doc) else {
            self.error("Сервер прислал правки неизвестного документа");
            return;
        };
        let refs: Vec<&[u8]> = batches.iter().map(|b| b.as_slice()).collect();
        let name = self.editor.document().name();
        match self.editor.apply_remote(key, &refs) {
            Ok(fx) => {
                if fx.failed {
                    self.error("Часть правок других участников не применилась — обновите страницу");
                }
                if fx.became_read_only {
                    self.error(NEWER_VERSION_EDITS);
                }
                if !fx.missing_images.is_empty() {
                    self.events.push(CoreEvent::ImagesNeeded {
                        hashes: fx.missing_images,
                    });
                }
                if fx.root_changed {
                    self.ui.pages = true;
                    let now = self.editor.document().name();
                    if now != name {
                        self.events.push(CoreEvent::DocumentName {
                            name: panels::event_name(&now),
                        });
                    }
                }
                self.dirty = true;
            }
            Err(e) => self.error(format!(
                "Не удалось применить правки других участников: {e}"
            )),
        }
        self.sync_note();
    }

    pub(super) fn add_remote_image(&mut self, hash: &str, bytes: Vec<u8>) {
        if self.editor.add_remote_image(hash, bytes) {
            self.images.forget(hash);
            self.dirty = true;
        }
    }

    pub(super) fn push_thumbnail(&mut self) {
        let (bytes, keep) = match self.thumbnail() {
            Ok(png) => (png.map(ByteBuf::from), false),
            Err(()) => (None, true),
        };
        self.events.push(CoreEvent::Thumbnail { bytes, keep });
    }
}
````

В `crates/ferrum-wasm/src/images.rs` заменить:

````rust
pub struct DocImages {
    inner: RefCell<Inner>,
}

````

на:

````rust
pub struct DocImages {
    inner: RefCell<Inner>,
}

impl DocImages {
    /// Байты картинки появились позже заглушки (чужая ссылка, скачанная по запросу): декодировать заново.
    pub fn forget(&self, hash: &str) {
        let mut inner = self.inner.borrow_mut();
        if let Some(e) = inner.map.remove(hash) {
            inner.bytes = inner.bytes.saturating_sub(e.bytes());
        }
    }
}

````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        #[tsify(type = "Uint8Array[]")]
        journal: Vec<ByteBuf>,
    },
    /// Отдать журналу правки с прошлой пачки: ответ — `Journal` (и без правок).
    FlushJournal,
````

на:

````rust
        #[tsify(type = "Uint8Array[]")]
        journal: Vec<ByteBuf>,
        /// Файл на сервере: журналу может не хватать правок, которые придут при догонянии.
        #[serde(default)]
        #[tsify(optional)]
        remote: Option<bool>,
    },
    /// Синхронизация с сервером (спека подсистемы 4, разделы 7 и 8). Команды этой группы идут из
    /// `sync.worker`, основной поток трафика не видит.
    SyncStart,
    /// Соединение оборвалось: на следующем всё с подписки, неподтверждённое уйдёт снова.
    SyncReset,
    /// Сервер ответил на подписку своей версией документа.
    SyncSubscribed {
        doc: String,
        #[tsify(type = "Uint8Array")]
        version: ByteBuf,
    },
    /// Сервер записал пачку `clientSeq` на диск базы.
    SyncAcked {
        doc: String,
        client_seq: u32,
    },
    /// Отдать новые подписки и свои правки: не реже 30 раз в секунду, пока есть правки и есть куда.
    SyncFlush,
    /// Чужие правки документа, по порядку.
    ApplyRemote {
        doc: String,
        #[tsify(type = "Uint8Array[]")]
        batches: Vec<ByteBuf>,
    },
    /// Картинка по чужой ссылке, скачанная с сервера.
    AddRemoteImage {
        hash: String,
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Только превью первой страницы для экрана файлов (ответ — `Thumbnail`): файл пишет журнал сервера.
    Thumbnail,
    /// Отдать журналу правки с прошлой пачки: ответ — `Journal` (и без правок).
    FlushJournal,
````

В `crates/ferrum-wasm/src/protocol.rs` заменить:

````rust
        failed: bool,
    },
    /// Перед `Opened`: поверх файла применены пачки журнала — правки, которые не
    /// успели попасть в файл (документ изменён).
````

на:

````rust
        failed: bool,
    },
    /// На какие документы подписаться и с какой версией (`SyncVersion::encode`).
    SyncSubscribe {
        doc: String,
        #[tsify(type = "Uint8Array")]
        version: ByteBuf,
    },
    /// Новая картинка: уходит на сервер раньше правок, идущих за ней.
    LocalImage {
        hash: String,
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Своя правка для сервера.
    LocalUpdates {
        doc: String,
        client_seq: u32,
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Есть ли правки, которых сервер не подтвердил. Только при смене.
    SyncState {
        unacked: bool,
    },
    /// Пачки журнала до этой включительно сервер подтвердил: их можно снять.
    JournalCovered {
        batch: f64,
    },
    /// Чужие правки ссылаются на картинки, которых нет в памяти: скачать и прислать `AddRemoteImage`.
    ImagesNeeded {
        hashes: Vec<String>,
    },
    /// Сообщение для человека, не ошибка.
    Notice {
        message: String,
    },
    /// Ответ на команду `Thumbnail`: `keep` — показать нечего или не вышло, прежнее оставить.
    Thumbnail {
        #[tsify(type = "Uint8Array | undefined")]
        bytes: Option<ByteBuf>,
        keep: bool,
    },
    /// Перед `Opened`: поверх файла применены пачки журнала — правки, которые не
    /// успели попасть в файл (документ изменён).
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run wasm`
Expected: ядро собрано: `src/wasm`, в типах появились новые команды и события

- [ ] **Step 5: Проверить**

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc -p ferrum-editor -p ferrum-wasm`

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Expected: PASS: всё без замечаний; тесты трёх крейтов проходят

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-doc/src/document.rs crates/ferrum-doc/src/images.rs crates/ferrum-doc/src/sync.rs crates/ferrum-editor/src/editor.rs crates/ferrum-editor/src/lib.rs crates/ferrum-editor/src/remote.rs crates/ferrum-editor/tests/remote.rs crates/ferrum-wasm/src/core.rs crates/ferrum-wasm/src/core/panels.rs crates/ferrum-wasm/src/core/sync.rs crates/ferrum-wasm/src/images.rs crates/ferrum-wasm/src/protocol.rs crates/ferrum-wasm/tests/journal.rs crates/ferrum-wasm/tests/journal_perf.rs crates/ferrum-wasm/tests/protocol.rs crates/ferrum-wasm/tests/sync.rs
git commit -F - <<'EOF'
редактор и ядро worker'а: чужие правки, подписки и подтверждения

Редактор: чужая правка обновляет сцену обычным путём и не делает документ изменённым (`RemoteChanged`);
удалённые узлы тихо уходят из выделения, жест над удалённым узлом отменяется, правки других свойств
жест не прерывают; каретка едет за чужими вставками, сеанс текста кончается с узлом; сеанс правки вектора
закрывается с уведомлением, если другой изменил сеть; страницу, удалённую другим, сменяет первая.

Ядро worker'а: команды `SyncStart`, `SyncReset`, `SyncSubscribed`, `SyncAcked`, `SyncFlush`, `ApplyRemote`,
`AddRemoteImage`, `Thumbnail`; события `SyncSubscribe`, `LocalImage` (раньше правки со ссылкой), `LocalUpdates`,
`SyncState`, `JournalCovered`, `ImagesNeeded`, `Notice`. «Изменён» снимает подтверждение сервера. Пачка журнала
снимается только после подтверждения всего, что в неё вошло; без своего неподтверждённого журнал не пишется.

Большая правка собирается порциями: до 32 МБ за вызов, каждый кусок меньше кадра сервера.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 4: Протокол — кадры синхронизации

Спека §7.2. Двоичные кадры с пределами размера и строгим разбором, вектор версий на проводе, коды закрытия и отказов.

**Files:**
- Create: `packages/protocol/src/sync.ts`
- Modify: `packages/protocol/src/index.ts`
- Test: `packages/protocol/test/sync.test.ts` (новый)

**Interfaces:**
- Produces: `packages/protocol/src/sync.ts`, экспорт из `@ferrum/protocol`; кодеки кадров `hello`, `subscribe`, `subscribed`, `update`, `ack`, `refused`, `pause`, `role`, `ping`, `pong`, `snapshot-needed`, `please-reload`.

- [ ] **Step 1: Написать тесты**

Создать `packages/protocol/test/sync.test.ts`:

````ts
import { expect, test } from 'bun:test';
import {
  encodeClientFrame,
  encodeServerFrame,
  encodeVersion,
  MAX_SYNC_FRAME_BYTES,
  MAX_SYNC_UPDATE_BYTES,
  parseClientFrame,
  parseServerFrame,
  parseVersion,
  versionIncludes,
  type ClientFrame,
  type ServerFrame,
} from '../src/sync';

const version = encodeVersion(
  new Map([
    ['18446744073709551615', 7],
    ['12', 1],
  ]),
);
const doc = '2808@7910247476534078275';

test('кадры клиента туда и обратно', () => {
  const frames: ClientFrame[] = [
    { t: 'hello', protocol: 1, schema: 3 },
    { t: 'subscribe', doc: 'root', version },
    { t: 'update', doc, clientSeq: 4_000_000_000, bytes: new Uint8Array([1, 2, 3]) },
    { t: 'ephemeral', bytes: new Uint8Array([9]) },
    { t: 'ping' },
  ];
  for (const f of frames) expect(parseClientFrame(encodeClientFrame(f))).toEqual(f);
});

test('кадры сервера туда и обратно', () => {
  const frames: ServerFrame[] = [
    { t: 'role', role: 'editor', canShare: true, version: 12 },
    { t: 'subscribed', doc: 'root', seq: 2 ** 40, version },
    { t: 'updates', doc, seq: 5, bytes: new Uint8Array([1]) },
    { t: 'snapshot-needed', doc, url: `/api/files/x/docs/${doc}/snapshot` },
    { t: 'ack', doc, clientSeq: 3, seq: 4 },
    { t: 'refused', doc, clientSeq: 3, code: 'FOREIGN_PEER' },
    { t: 'pause', ms: 1500 },
    { t: 'access-revoked', reason: 'session' },
    { t: 'please-reload' },
    { t: 'pong' },
  ];
  for (const f of frames) expect(parseServerFrame(encodeServerFrame(f))).toEqual(f);
});

test('вектор версий: пир u64 без потери, мусор — null', () => {
  const m = parseVersion(version)!;
  expect(m.get('18446744073709551615')).toBe(7);
  expect(encodeVersion(m)).toEqual(version);
  expect(parseVersion(version.subarray(0, version.length - 1))).toBeNull();
  expect(parseVersion(new Uint8Array(3))).toBeNull();
  // Объявил миллиард записей: разбор не идёт дальше заголовка.
  const huge = new Uint8Array(4);
  new DataView(huge.buffer).setUint32(0, 1_000_000_000, true);
  expect(parseVersion(huge)).toBeNull();
  // Счётчик вне i32 в Rust не живёт.
  const bad = encodeVersion(new Map([['1', 5]]));
  new DataView(bad.buffer).setUint32(12, 0x80000000, true);
  expect(parseVersion(bad)).toBeNull();
});

test('versionIncludes', () => {
  const a = new Map([
    ['1', 5],
    ['2', 1],
  ]);
  expect(versionIncludes(a, new Map([['1', 5]]))).toBe(true);
  expect(versionIncludes(a, new Map([['1', 6]]))).toBe(false);
  expect(versionIncludes(a, new Map([['3', 1]]))).toBe(false);
  expect(versionIncludes(a, new Map())).toBe(true);
});

test('разбор не доверяет кадру', () => {
  const update = encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 1, bytes: new Uint8Array([1]) });
  // Обрезка в любом месте заголовка, неизвестный вид, пустой кадр.
  for (let n = 0; n < 9; n++) expect(parseClientFrame(update.subarray(0, n))).toBeNull();
  expect(parseClientFrame(new Uint8Array([200]))).toBeNull();
  expect(parseClientFrame(new Uint8Array(0))).toBeNull();
  // Документ не того вида: путь, длинное имя, не UTF-8.
  for (const bad of ['../../etc', 'x'.repeat(30), '1@', '@1', 'root ']) {
    const f = encodeClientFrame({ t: 'subscribe', doc: bad, version });
    expect(parseClientFrame(f)).toBeNull();
  }
  const notUtf = new Uint8Array([2, 2, 0xff, 0xfe, ...version]);
  expect(parseClientFrame(notUtf)).toBeNull();
  // Хвост у кадров без него.
  expect(parseClientFrame(new Uint8Array([5, 1]))).toBeNull();
  expect(parseClientFrame(new Uint8Array([1, 1, 0, 1, 0, 0, 0, 9]))).toBeNull();
});

test('предел кадра и пачки', () => {
  const big = new Uint8Array(MAX_SYNC_UPDATE_BYTES + 1);
  expect(parseClientFrame(encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 1, bytes: big }))).toBeNull();
  const ok = new Uint8Array(MAX_SYNC_UPDATE_BYTES);
  expect(parseClientFrame(encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 1, bytes: ok }))?.t).toBe('update');
  expect(parseClientFrame(new Uint8Array(MAX_SYNC_FRAME_BYTES + 1))).toBeNull();
  // Пустая пачка — не пачка.
  expect(parseClientFrame(encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 1, bytes: new Uint8Array(0) }))).toBeNull();
});

test('сервер не может прислать неизвестную роль и код', () => {
  expect(parseServerFrame(new Uint8Array([16, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0]))).toBeNull();
  expect(parseServerFrame(new Uint8Array([21, 1, 0x31, 0, 0, 0, 0, 99]))).toBeNull();
  expect(parseServerFrame(new Uint8Array([99]))).toBeNull();
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd packages/protocol && bun test test/sync.test.ts`
Expected: FAIL — `Cannot find module '../src/sync'`: 0 pass, 1 fail

- [ ] **Step 3: Реализация**

В `packages/protocol/src/index.ts` заменить:

````ts
export * from './files';
export * from './container';
````

на:

````ts
export * from './files';
export * from './container';
export * from './sync';
````

Создать `packages/protocol/src/sync.ts`:

````ts
// Кадры WebSocket файла (спека подсистемы 4, раздел 7.2): двоичные, разбор не доверяет данным, неверное — `null`.
// Первый байт — вид кадра, числа — LE, строка — длина u8 и UTF-8. Документ — `root` или `TreeID` страницы.

import type { FileRole } from './files';

/** Растёт при несовместимой смене кадров: старый клиент получает `please-reload`. */
export const SYNC_PROTOCOL = 1;
/** Предел кадра по спеке, раздел 7.3. Вся полезная нагрузка — байты Loro, заголовок сверху не считается. */
export const MAX_SYNC_UPDATE_BYTES = 16 * 1024 * 1024;
/** Сервер закрывает кадр больше этого, не читая его. */
export const MAX_SYNC_FRAME_BYTES = MAX_SYNC_UPDATE_BYTES + 1024;
/** Вектор версий: столько сеансов правки на один документ не набирается, а чужой кадр не задаёт размер разбора. */
export const MAX_VERSION_PEERS = 65_536;
const MAX_DOC_BYTES = 40;
const DOC_RE = /^(root|\d{1,10}@\d{1,20})$/;

/** Адрес соединения файла: `?user=` — кто в этой вкладке, как заголовок `USER_HEADER` у запросов. */
export const syncPath = (file: string, user: string): string => `/api/files/${file}/sync?user=${encodeURIComponent(user)}`;
export const snapshotPath = (file: string, doc: string): string => `/api/files/${file}/docs/${doc}/snapshot`;

/** Коды закрытия соединения: клиент по ним решает, пробовать ли снова. */
export const SYNC_CLOSE = {
  /** Сервер останавливается: переподключиться. */
  GOING_AWAY: 1001,
  TOO_BIG: 1009,
  /** Клиент нарушил протокол: повтор не поможет. */
  BAD_FRAME: 4400,
  /** Сессии больше нет или вкладка работает за другого человека. */
  SESSION: 4401,
  /** Доступа к файлу больше нет. */
  REVOKED: 4403,
  /** Нужна новая версия приложения. */
  RELOAD: 4426,
  /** Слишком много соединений. */
  TOO_MANY: 4429,
  /** Не успевает читать: догонит заново. */
  SLOW: 4408,
} as const;

/** Почему отказано в пачке. */
export const SYNC_REFUSALS = [
  /** Байты разобрать не удалось, или они не применяются. Повтор бессмыслен. */
  'BAD_UPDATE',
  /** Пачка вне вектора документа: клиент подписывается заново и шлёт от версии сервера. */
  'GAP',
  /** Роли не хватает. */
  'FORBIDDEN',
  /** Пачка пишет от чужого пира. */
  'FOREIGN_PEER',
  /** Журнал файла переполнен: сжатие отстало или не удаётся. Правки ждут у клиента. */
  'LOG_FULL',
  /** База недоступна: правка не записана, клиент подписывается заново и шлёт снова. */
  'UNAVAILABLE',
] as const;
export type SyncRefusal = (typeof SYNC_REFUSALS)[number];

/** Почему доступ снят. */
export type RevokeReason = 'access' | 'session';

export type ClientFrame =
  | { t: 'hello'; protocol: number; schema: number }
  | { t: 'subscribe'; doc: string; version: Uint8Array }
  | { t: 'update'; doc: string; clientSeq: number; bytes: Uint8Array }
  | { t: 'ephemeral'; bytes: Uint8Array }
  | { t: 'ping' };

export type ServerFrame =
  | { t: 'role'; role: FileRole; canShare: boolean; version: number }
  | { t: 'subscribed'; doc: string; seq: number; version: Uint8Array }
  | { t: 'updates'; doc: string; seq: number; bytes: Uint8Array }
  | { t: 'snapshot-needed'; doc: string; url: string }
  | { t: 'ack'; doc: string; clientSeq: number; seq: number }
  | { t: 'refused'; doc: string; clientSeq: number; code: SyncRefusal }
  /** Предел частоты: пауза, а не сбой. Кадр не принят и после неё будет отправлен снова. */
  | { t: 'pause'; ms: number }
  | { t: 'access-revoked'; reason: RevokeReason }
  | { t: 'please-reload' }
  | { t: 'pong' };

const KIND = {
  hello: 1,
  subscribe: 2,
  update: 3,
  ephemeral: 4,
  ping: 5,
  role: 16,
  subscribed: 17,
  updates: 18,
  'snapshot-needed': 19,
  ack: 20,
  refused: 21,
  pause: 22,
  'access-revoked': 23,
  'please-reload': 24,
  pong: 25,
} as const;

const ROLES: readonly FileRole[] = ['viewer', 'editor', 'owner'];
const REASONS: readonly RevokeReason[] = ['access', 'session'];
const utf8 = new TextEncoder();
const fromUtf8 = new TextDecoder('utf-8', { fatal: true });

class Writer {
  private readonly parts: Uint8Array[] = [];
  private size = 0;
  private push(p: Uint8Array): this {
    this.parts.push(p);
    this.size += p.length;
    return this;
  }
  u8(v: number): this {
    return this.push(Uint8Array.of(v));
  }
  u16(v: number): this {
    const b = new Uint8Array(2);
    new DataView(b.buffer).setUint16(0, v, true);
    return this.push(b);
  }
  u32(v: number): this {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, v, true);
    return this.push(b);
  }
  /** Номер записи журнала: до 2^53, столько не набрать. */
  u64(v: number): this {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, BigInt(v), true);
    return this.push(b);
  }
  str(s: string): this {
    const b = utf8.encode(s);
    return this.u16(b.length).push(b);
  }
  doc(s: string): this {
    const b = utf8.encode(s);
    return this.u8(b.length).push(b);
  }
  bytes(b: Uint8Array): this {
    return this.push(b);
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

class Reader {
  private at = 1;
  private readonly view: DataView;
  constructor(private readonly b: Uint8Array) {
    this.view = new DataView(b.buffer, b.byteOffset, b.byteLength);
  }
  private has(n: number): boolean {
    return this.at + n <= this.b.length;
  }
  u8(): number | null {
    if (!this.has(1)) return null;
    return this.b[this.at++];
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
  u64(): number | null {
    if (!this.has(8)) return null;
    const v = this.view.getBigUint64(this.at, true);
    this.at += 8;
    return v <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(v) : null;
  }
  str(max: number): string | null {
    const n = this.u16();
    if (n === null || n > max || !this.has(n)) return null;
    return this.text(n);
  }
  doc(): string | null {
    const n = this.u8();
    if (n === null || n > MAX_DOC_BYTES || !this.has(n)) return null;
    const s = this.text(n);
    return s !== null && DOC_RE.test(s) ? s : null;
  }
  private text(n: number): string | null {
    try {
      const s = fromUtf8.decode(this.b.subarray(this.at, this.at + n));
      this.at += n;
      return s;
    } catch {
      return null;
    }
  }
  rest(): Uint8Array {
    const r = this.b.subarray(this.at);
    this.at = this.b.length;
    return r;
  }
  done(): boolean {
    return this.at === this.b.length;
  }
}

export const isSyncDoc = (s: string): boolean => DOC_RE.test(s);

export function encodeClientFrame(f: ClientFrame): Uint8Array<ArrayBuffer> {
  const w = new Writer().u8(KIND[f.t]);
  switch (f.t) {
    case 'hello':
      return w.u16(f.protocol).u32(f.schema).done();
    case 'subscribe':
      return w.doc(f.doc).bytes(f.version).done();
    case 'update':
      return w.doc(f.doc).u32(f.clientSeq).bytes(f.bytes).done();
    case 'ephemeral':
      return w.bytes(f.bytes).done();
    case 'ping':
      return w.done();
  }
}

export function encodeServerFrame(f: ServerFrame): Uint8Array<ArrayBuffer> {
  const w = new Writer().u8(KIND[f.t]);
  switch (f.t) {
    case 'role':
      return w
        .u8(ROLES.indexOf(f.role))
        .u8(f.canShare ? 1 : 0)
        .u64(f.version)
        .done();
    case 'subscribed':
      return w.doc(f.doc).u64(f.seq).bytes(f.version).done();
    case 'updates':
      return w.doc(f.doc).u64(f.seq).bytes(f.bytes).done();
    case 'snapshot-needed':
      return w.doc(f.doc).str(f.url).done();
    case 'ack':
      return w.doc(f.doc).u32(f.clientSeq).u64(f.seq).done();
    case 'refused':
      return w.doc(f.doc).u32(f.clientSeq).u8(SYNC_REFUSALS.indexOf(f.code)).done();
    case 'pause':
      return w.u32(f.ms).done();
    case 'access-revoked':
      return w.u8(REASONS.indexOf(f.reason)).done();
    case 'please-reload':
    case 'pong':
      return w.done();
  }
}

/** Разбор кадра клиента на сервере. Байты пачки — вид внутрь кадра, без копии. */
export function parseClientFrame(b: Uint8Array): ClientFrame | null {
  if (b.length === 0 || b.length > MAX_SYNC_FRAME_BYTES) return null;
  const r = new Reader(b);
  switch (b[0]) {
    case KIND.hello: {
      const protocol = r.u16();
      const schema = r.u32();
      return protocol !== null && schema !== null && r.done() ? { t: 'hello', protocol, schema } : null;
    }
    case KIND.subscribe: {
      const doc = r.doc();
      if (doc === null) return null;
      const version = r.rest();
      return parseVersion(version) ? { t: 'subscribe', doc, version } : null;
    }
    case KIND.update: {
      const doc = r.doc();
      const clientSeq = r.u32();
      if (doc === null || clientSeq === null) return null;
      const bytes = r.rest();
      return bytes.length > 0 && bytes.length <= MAX_SYNC_UPDATE_BYTES ? { t: 'update', doc, clientSeq, bytes } : null;
    }
    case KIND.ephemeral:
      return { t: 'ephemeral', bytes: r.rest() };
    case KIND.ping:
      return r.done() ? { t: 'ping' } : null;
    default:
      return null;
  }
}

/** Разбор кадра сервера у клиента: сеть не доверенная, неверное — как обрыв. */
export function parseServerFrame(b: Uint8Array): ServerFrame | null {
  if (b.length === 0) return null;
  const r = new Reader(b);
  switch (b[0]) {
    case KIND.role: {
      const role = ROLES[r.u8() ?? -1];
      const canShare = r.u8();
      const version = r.u64();
      return role && canShare !== null && version !== null && r.done() ? { t: 'role', role, canShare: canShare === 1, version } : null;
    }
    case KIND.subscribed: {
      const doc = r.doc();
      const seq = r.u64();
      if (doc === null || seq === null) return null;
      const version = r.rest();
      return parseVersion(version) ? { t: 'subscribed', doc, seq, version } : null;
    }
    case KIND.updates: {
      const doc = r.doc();
      const seq = r.u64();
      return doc !== null && seq !== null ? { t: 'updates', doc, seq, bytes: r.rest() } : null;
    }
    case KIND['snapshot-needed']: {
      const doc = r.doc();
      const url = r.str(1024);
      return doc !== null && url !== null && r.done() ? { t: 'snapshot-needed', doc, url } : null;
    }
    case KIND.ack: {
      const doc = r.doc();
      const clientSeq = r.u32();
      const seq = r.u64();
      return doc !== null && clientSeq !== null && seq !== null && r.done() ? { t: 'ack', doc, clientSeq, seq } : null;
    }
    case KIND.refused: {
      const doc = r.doc();
      const clientSeq = r.u32();
      const code = SYNC_REFUSALS[r.u8() ?? -1];
      return doc !== null && clientSeq !== null && code && r.done() ? { t: 'refused', doc, clientSeq, code } : null;
    }
    case KIND.pause: {
      const ms = r.u32();
      return ms !== null && r.done() ? { t: 'pause', ms } : null;
    }
    case KIND['access-revoked']: {
      const reason = REASONS[r.u8() ?? -1];
      return reason && r.done() ? { t: 'access-revoked', reason } : null;
    }
    case KIND['please-reload']:
      return r.done() ? { t: 'please-reload' } : null;
    case KIND.pong:
      return r.done() ? { t: 'pong' } : null;
    default:
      return null;
  }
}

/** Вектор версий на проводе: `n: u32 · (peer: u64 · counter: u32) × n`, как `SyncVersion` в ядре. Пир — десятичной
 * строкой: u64 не влезает в число JavaScript, а в JSON базы — так же. Сверх предела и с хвостом — `null`. */
export type VersionMap = Map<string, number>;

export function parseVersion(b: Uint8Array): VersionMap | null {
  if (b.length < 4) return null;
  const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const n = v.getUint32(0, true);
  if (n > MAX_VERSION_PEERS || b.length !== 4 + n * 12) return null;
  const out: VersionMap = new Map();
  for (let i = 0; i < n; i++) {
    const at = 4 + i * 12;
    const counter = v.getUint32(at + 8, true);
    if (counter > 0x7fffffff) return null;
    if (counter > 0) out.set(v.getBigUint64(at, true).toString(), counter);
  }
  return out;
}

export function encodeVersion(m: VersionMap): Uint8Array<ArrayBuffer> {
  const peers = [...m.entries()].sort(([a], [b]) => {
    const [x, y] = [BigInt(a), BigInt(b)];
    return x < y ? -1 : x > y ? 1 : 0;
  });
  const out = new Uint8Array(4 + peers.length * 12);
  const v = new DataView(out.buffer);
  v.setUint32(0, peers.length, true);
  peers.forEach(([peer, counter], i) => {
    v.setBigUint64(4 + i * 12, BigInt(peer), true);
    v.setUint32(12 + i * 12, counter, true);
  });
  return out;
}

/** Содержит ли `a` всё из `b`. */
export function versionIncludes(a: VersionMap, b: VersionMap): boolean {
  for (const [peer, counter] of b) {
    if ((a.get(peer) ?? 0) < counter) return false;
  }
  return true;
}
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd packages/protocol && bun test`

Expected: PASS: всё без замечаний; `packages/protocol` 23 pass

- [ ] **Step 5: Закоммитить**

```bash
git add packages/protocol/src/index.ts packages/protocol/src/sync.ts packages/protocol/test/sync.test.ts
git commit -F - <<'EOF'
протокол: кадры синхронизации

`packages/protocol/src/sync.ts`: двоичные кадры WebSocket файла по спеке подсистемы 4, раздел 7.2, с пределами
размера и строгим разбором, вектор версий на проводе, коды закрытия и отказов. Сверх таблицы спеки: `subscribed`
(версия сервера, от которой клиент шлёт своё), `refused`, `pause` (предел частоты — пауза, а не сбой), `ping`,
`role` несёт `canShare` и `version`, которые читает редактор (B-296).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 5: Сервер — журнал обновлений и проверка пачек

Спека §7.3, расхождение в «Решениях» (1). Миграция 004, группы каждые 20 мс, порядок и непрерывность держит строка документа под блокировкой, `Validator` в Bun Worker, совместимость с Rust в обе стороны.

**Files:**
- Create: `apps/server/migrations/004_sync.sql`, `apps/server/src/sync/container-read.ts`, `apps/server/src/sync/update-log.ts`, `apps/server/src/sync/validator.ts`, `apps/server/src/sync/validator.worker.ts`
- Modify: `apps/server/bun.lock`, `apps/server/package.json`
- Test: `apps/server/test/data/loro-base.snap` (новый), `apps/server/test/data/loro-good.update` (новый), `apps/server/test/data/loro-panic-any.update` (новый), `apps/server/test/data/loro-panic-real.update` (новый), `apps/server/test/loro-compat.test.ts` (новый), `apps/server/test/migrate.test.ts`, `apps/server/test/update-log.test.ts` (новый), `apps/server/test/validator.test.ts` (новый), `crates/ferrum-doc/tests/data/loro-compat/js-page.snapshot` (новый), `crates/ferrum-doc/tests/data/loro-compat/js.snapshot` (новый), `crates/ferrum-doc/tests/data/loro-compat/rust-page.snapshot` (новый), `crates/ferrum-doc/tests/data/loro-compat/rust.snapshot` (новый), `crates/ferrum-doc/tests/data/loro-compat/rust.update` (новый), `crates/ferrum-doc/tests/loro_compat.rs` (новый)

**Interfaces:**
- Consumes: кадры задачи 4, `file_role_rank`.
- Produces: `UpdateLog`, `Validator`, `readContainerSection`, таблицы `doc_state`, `doc_update`, `doc_peer`.

- [ ] **Step 1: Написать тесты**

Создать `apps/server/test/loro-compat.test.ts`:

````ts
// Совместимость Loro (спека подсистемы 4, разделы 3 и 13.2): обновления и снимки ядра на Rust читает `loro-crdt` на Bun, и
// наоборот; версии в `Cargo.toml` и `package.json` одни и те же. Обратная сторона — `crates/ferrum-doc/tests/loro_compat.rs`.

import { expect, test } from 'bun:test';
import { decodeImportBlobMeta, LoroDoc } from 'loro-crdt';

const dir = new URL('../../../crates/ferrum-doc/tests/data/loro-compat/', import.meta.url);
const read = (name: string) => Bun.file(new URL(name, dir)).bytes();

test('версии Loro на сервере и в ядре совпадают', async () => {
  const cargo = await Bun.file(new URL('../../../Cargo.toml', import.meta.url)).text();
  const rust = /^loro = "=([0-9.]+)"/m.exec(cargo)?.[1];
  const pkg = (await Bun.file(new URL('../package.json', import.meta.url)).json()) as { dependencies: Record<string, string> };
  expect(pkg.dependencies['loro-crdt']).toBe(rust!);
});

test('снимок и обновление ядра читаются на сервере', async () => {
  const snapshot = await read('rust.snapshot');
  const update = await read('rust.update');
  expect(decodeImportBlobMeta(snapshot, true).mode).toBe('snapshot');
  expect(decodeImportBlobMeta(update, false).mode).toBe('update');
  const d = LoroDoc.fromSnapshot(snapshot);
  expect(d.import(update).pending).toBeNull();
  const roots = d.getTree('nodes').roots();
  expect(roots.length).toBe(1);
  expect(roots[0].data.get('x')).toBe(42);
});

test('файл для обратной проверки — от этой же версии сервера', async () => {
  // `js.update` проверяет Rust. Его можно собрать заново этим сервером и получить те же данные.
  const d = new LoroDoc();
  d.getMap('m').set('hello', 'привет');
  d.commit();
  const fresh = LoroDoc.fromSnapshot(d.export({ mode: 'snapshot' }));
  expect(fresh.getMap('m').get('hello')).toBe('привет');
  const stored = LoroDoc.fromSnapshot(await read('js.snapshot'));
  expect(stored.getMap('m').get('hello')).toBe('привет');
});

test('страницу ядра сервер пересобирает: снимок для обратной проверки ядром', async () => {
  const page = LoroDoc.fromSnapshot(await read('rust-page.snapshot'));
  const again = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  expect(again.oplogVersion().toJSON()).toEqual(page.oplogVersion().toJSON());
  if (process.env.LORO_COMPAT_WRITE) await Bun.write(new URL('js-page.snapshot', dir), page.export({ mode: 'snapshot' }));
  // Сохранённый снимок той же версии Loro читается и здесь.
  expect(
    LoroDoc.fromSnapshot(await read('js-page.snapshot'))
      .getTree('nodes')
      .getNodes().length,
  ).toBe(2);
});
````

В `apps/server/test/migrate.test.ts` заменить:

````ts
test('миграции репозитория применяются на пустой базе', async () => {
  const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
  expect(applied).toEqual(['001_init.sql', '002_better_auth.sql', '003_files.sql']);
});

````

на:

````ts
test('миграции репозитория применяются на пустой базе', async () => {
  const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
  expect(applied).toEqual(['001_init.sql', '002_better_auth.sql', '003_files.sql', '004_sync.sql']);
});

````

В `apps/server/test/migrate.test.ts` заменить:

````ts
  await migrate(pool, stage1);
  await pool.query(`INSERT INTO "user" (id, name, email, "emailVerified") VALUES ('u1', 'Иван', 'ivan@example.com', true)`);
  expect(await migrate(pool, all)).toEqual(['003_files.sql']);
  expect((await pool.query('SELECT email FROM "user"')).rows).toEqual([{ email: 'ivan@example.com' }]);
  const { rows } = await pool.query(`INSERT INTO file (owner_id, name) VALUES ('u1', 'Файл') RETURNING version`);
````

на:

````ts
  await migrate(pool, stage1);
  await pool.query(`INSERT INTO "user" (id, name, email, "emailVerified") VALUES ('u1', 'Иван', 'ivan@example.com', true)`);
  expect(await migrate(pool, all)).toEqual(['003_files.sql', '004_sync.sql']);
  expect((await pool.query('SELECT email FROM "user"')).rows).toEqual([{ email: 'ivan@example.com' }]);
  const { rows } = await pool.query(`INSERT INTO file (owner_id, name) VALUES ('u1', 'Файл') RETURNING version`);
````

Создать `apps/server/test/update-log.test.ts`:

````ts
// Журнал обновлений на настоящем Postgres: порядок, повторы, непрерывность, роль в транзакции записи, чужой пир,
// предел журнала, сбой базы и два писателя на одной базе (спека подсистемы 4, раздел 7.3).

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { apiStand } from './api-stand';
import { UpdateLog, type Item } from '../src/sync/update-log';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const other = await s.signIn('other@example.com');
const stranger = await s.signIn('stranger@example.com');
let file = '';

beforeEach(async () => {
  await s.reset();
  file = (await s.newFile(owner)).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor')`, [file, other.id]);
});

const item = (over: Partial<Item> = {}): Item => ({
  file,
  doc: 'root',
  user: owner.id,
  bytes: new Uint8Array([1, 2, 3]),
  start: { '1': 0 },
  end: { '1': 1 },
  ...over,
});

const rows = async (doc = 'root') =>
  (
    await s.pool.query<{ seq: string; end_vv: Record<string, number> }>(
      'SELECT seq, end_vv FROM doc_update WHERE file_id = $1 AND doc = $2 ORDER BY seq',
      [file, doc],
    )
  ).rows.map((r) => [Number(r.seq), r.end_vv]);

test('порядок журнала — порядок приёма, ack после фиксации', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  const committed: number[] = [];
  log.onCommitted = (items) => committed.push(...items.map((i) => i.seq));
  const out = await Promise.all([0, 1, 2, 3, 4].map((i) => log.submit(item({ start: { '1': i }, end: { '1': i + 1 } }))));
  expect(out.map((o) => (o.kind === 'written' ? o.seq : o.kind))).toEqual([1, 2, 3, 4, 5]);
  expect(committed).toEqual([1, 2, 3, 4, 5]);
  expect((await rows()).map((r) => r[0])).toEqual([1, 2, 3, 4, 5]);
  await log.stop();
});

test('повтор пачки — тот же результат без новой записи', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  expect(await log.submit(item())).toEqual({ kind: 'written', seq: 1 });
  expect(await log.submit(item())).toEqual({ kind: 'known', seq: 1 });
  // Перекрытие с новым хвостом: Loro примет, вектор сдвинется.
  expect(await log.submit(item({ start: { '1': 0 }, end: { '1': 3 } }))).toEqual({ kind: 'written', seq: 2 });
  expect(await rows()).toHaveLength(2);
  await log.stop();
});

test('разрыв по пиру отвергается и журнала не трогает', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  await log.submit(item());
  expect(await log.submit(item({ start: { '1': 5 }, end: { '1': 6 } }))).toEqual({ kind: 'refused', code: 'GAP' });
  expect(await rows()).toHaveLength(1);
  const { rows: st } = await s.pool.query<{ vv: Record<string, number> }>('SELECT vv FROM doc_state WHERE file_id = $1', [file]);
  expect(st[0].vv).toEqual({ '1': 1 });
  await log.stop();
});

test('роль проверяется в транзакции записи: снятая и читатель не пишут', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  expect(await log.submit(item({ user: other.id }))).toMatchObject({ kind: 'written' });
  await s.pool.query(`UPDATE file_access SET role = 'viewer' WHERE file_id = $1 AND user_id = $2`, [file, other.id]);
  expect(await log.submit(item({ user: other.id, start: { '1': 1 }, end: { '1': 2 } }))).toEqual({ kind: 'refused', code: 'FORBIDDEN' });
  expect(await log.submit(item({ user: stranger.id, start: { '9': 0 }, end: { '9': 1 } }))).toEqual({ kind: 'refused', code: 'FORBIDDEN' });
  expect(await rows()).toHaveLength(1);
  await log.stop();
});

test('чужой пир: первый писатель пира его занимает, подделка не проходит', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  await log.submit(item({ start: { '77': 0 }, end: { '77': 1 } }));
  expect(await log.submit(item({ user: other.id, start: { '77': 1 }, end: { '77': 2 } }))).toEqual({
    kind: 'refused',
    code: 'FOREIGN_PEER',
  });
  expect(await log.submit(item({ start: { '77': 1 }, end: { '77': 2 } }))).toMatchObject({ kind: 'written' });
  // Пир u64 не теряет младших разрядов.
  const big = '18446744073709551615';
  await log.submit(item({ start: { [big]: 0 }, end: { [big]: 1 } }));
  const { rows: p } = await s.pool.query<{ peer: string }>('SELECT peer::text AS peer FROM doc_peer d WHERE file_id = $1 ORDER BY d.peer', [
    file,
  ]);
  expect(p.map((x) => x.peer)).toEqual(['77', big]);
  await log.stop();
});

test('журнал не растёт сверх предела файла: отказ, правка цела у клиента', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1, maxLogBytesPerFile: 5 });
  expect(await log.submit(item())).toMatchObject({ kind: 'written' });
  expect(await log.submit(item({ start: { '1': 1 }, end: { '1': 2 } }))).toEqual({ kind: 'refused', code: 'LOG_FULL' });
  expect(await rows()).toHaveLength(1);
  await log.stop();
});

test('сбой базы: группа получает отказ «повторите», процесс живёт, следующая группа пишется', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  const failed = await s.failing('doc_update', 'INSERT', () => log.submit(item()));
  expect(failed).toEqual({ kind: 'refused', code: 'UNAVAILABLE' });
  const { rows: st } = await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file]);
  expect(st).toHaveLength(0);
  expect(await log.submit(item())).toEqual({ kind: 'written', seq: 1 });
  await log.stop();
});

test('два писателя на одной базе: номера подряд, порядок и повторы держит база', async () => {
  // Спека: процесс один, но решение этого не закрывает. Настоящие два клиента базы, как два процесса.
  const { Pool } = await import('pg');
  const second = new Pool({ connectionString: process.env.TEST_DATABASE_URL ?? 'postgres://ferrum:ferrum@127.0.0.1:15432/ferrum' });
  const a = new UpdateLog(s.pool, { groupMs: 1 });
  const b = new UpdateLog(second, { groupMs: 1 });
  const out = await Promise.all(
    Array.from({ length: 40 }, (_, i) => {
      const peer = i % 2 ? '100' : '200';
      const n = Math.floor(i / 2);
      return (i % 2 ? a : b).submit(item({ start: { [peer]: n }, end: { [peer]: n + 1 } }));
    }),
  );
  expect(out.filter((o) => o.kind === 'refused').length).toBeLessThanOrEqual(40);
  const seqs = (await rows()).map((r) => r[0] as number);
  expect(seqs).toEqual(seqs.map((_, i) => i + 1));
  const again = await a.submit(item({ start: { '100': 0 }, end: { '100': 1 } }));
  expect(again.kind === 'known' || again.kind === 'refused').toBe(true);
  await a.stop();
  await b.stop();
  await second.end();
});

test('остановка дописывает принятое', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 10_000 });
  const pending = log.submit(item());
  await log.stop();
  expect(await pending).toEqual({ kind: 'written', seq: 1 });
  expect(await log.submit(item())).toEqual({ kind: 'refused', code: 'UNAVAILABLE' });
});
````

Создать `apps/server/test/validator.test.ts`:

````ts
// Проверка пачек на настоящем Loro (спека подсистемы 4, раздел 7.3): порченые байты, снимок вместо обновления, паника
// wasm на настоящем документе при успехе на пустом, замена worker'а без потери чужих пачек. `loro-*` — найденные
// случайной порчей: контрольная сумма верна, разбор проходит, Loro падает при применении.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { containerHeader } from '@ferrum/protocol';
import { blake3 } from '@noble/hashes/blake3.js';
import { LoroDoc } from 'loro-crdt';
import { apiStand, type Person } from './api-stand';
import { Validator } from '../src/sync/validator';

const data = (name: string) => Bun.file(new URL(`./data/${name}`, import.meta.url)).bytes();
const base = await data('loro-base.snap');
const good = await data('loro-good.update');
const panicAny = await data('loro-panic-any.update');
const panicReal = await data('loro-panic-real.update');

const s = await apiStand();
afterAll(s.stop);
const owner: Person = await s.signIn('owner@example.com');
let file = '';
let v: Validator;

function section(kind: number, id: string, bytes: Uint8Array): Uint8Array {
  const idb = new TextEncoder().encode(id);
  const out = new Uint8Array(5 + idb.length + 40 + bytes.length);
  const dv = new DataView(out.buffer);
  out[0] = kind;
  dv.setUint32(1, idb.length, true);
  out.set(idb, 5);
  dv.setBigUint64(5 + idb.length, BigInt(bytes.length), true);
  out.set(blake3(bytes), 13 + idb.length);
  out.set(bytes, 45 + idb.length);
  return out;
}

const fileWith = (root: Uint8Array): Uint8Array => Buffer.concat([containerHeader(1), section(1, '', root)]);

beforeEach(async () => {
  v?.stop();
  await s.reset();
  v = new Validator(s.pool, s.blobs);
  file = (await s.newFile(owner)).id;
  const res = await s.writeContent(owner, file, 0, fileWith(base));
  expect(res.status).toBe(200);
});

test('верная пачка: версии начала и конца, документ принял', async () => {
  const r = await v.validate(file, 'root', good);
  expect(r.kind).toBe('ok');
  if (r.kind !== 'ok') return;
  expect(r.pending).toBe(false);
  expect(Object.keys(r.end)).toEqual(['1']);
  expect(r.end['1']).toBeGreaterThan(r.start['1']);
});

test('мусор и обрезка отвергаются', async () => {
  expect((await v.validate(file, 'root', new Uint8Array([1, 2, 3]))).kind).toBe('bad');
  expect((await v.validate(file, 'root', good.subarray(0, 40))).kind).toBe('bad');
  const flipped = good.slice();
  flipped[60] ^= 0xff;
  expect((await v.validate(file, 'root', flipped)).kind).toBe('bad');
});

test('снимок вместо обновления не принимается: он раздул бы журнал и обошёл бы проверку', async () => {
  expect((await v.validate(file, 'root', base)).kind).toBe('bad');
  const d = new LoroDoc();
  d.getMap('m').set('a', 1);
  d.commit();
  expect((await v.validate(file, 'root', d.export({ mode: 'snapshot' }))).kind).toBe('bad');
});

test('пачка без предыдущих правок: ждёт, а не проходит в журнал', async () => {
  const a = LoroDoc.fromSnapshot(base);
  const from = a.oplogVersion();
  a.getMap('m').set('a', 1);
  a.commit();
  const middle = a.oplogVersion();
  a.getMap('m').set('a', 2);
  a.commit();
  const r = await v.validate(file, 'root', a.export({ mode: 'update', from: middle }));
  expect(r.kind === 'ok' && r.pending).toBe(true);
  const r2 = await v.validate(file, 'root', a.export({ mode: 'update', from }));
  expect(r2.kind === 'ok' && r2.pending).toBe(false);
});

test('паника Loro на настоящем документе при успехе на пустом: отказ, worker заменён, остальные пачки целы', async () => {
  // Сам образец: на пустом документе проходит, на настоящем падает. Без настоящего документа проверка бы его пропустила.
  const empty = new LoroDoc();
  const saved = console.error;
  console.error = () => {};
  try {
    expect(() => empty.import(panicReal)).not.toThrow();
  } finally {
    console.error = saved;
  }
  expect((await v.validate(file, 'root', panicReal)).kind).toBe('bad');
  expect(v.restarts).toBe(1);
  // Новый worker загрузил документ заново и работает.
  const r = await v.validate(file, 'root', good);
  expect(r.kind).toBe('ok');
});

test('снимок не читается: проверка идёт на пустом документе, паника и там отвергается', async () => {
  const f = (await s.newFile(owner)).id;
  expect((await s.writeContent(owner, f, 0, fileWith(new Uint8Array([7, 7, 7, 7])))).status).toBe(200);
  expect((await v.validate(f, 'root', panicAny)).kind).toBe('bad');
  expect(v.restarts).toBe(1);
});

test('очередь проверки ограничена: сверх предела — «повторите», байты не копятся', async () => {
  const big = new Uint8Array(300 * 1024 * 1024);
  expect((await v.validate(file, 'root', big)).kind).toBe('retry');
});

test('снимок не читается (записан не Ferrum): файл живёт, разбор всё равно проверяется', async () => {
  const f = (await s.newFile(owner)).id;
  const res = await s.writeContent(owner, f, 0, fileWith(new Uint8Array([7, 7, 7, 7])));
  expect(res.status).toBe(200);
  expect((await v.validate(f, 'root', good)).kind).toBe('ok');
  expect((await v.validate(f, 'root', new Uint8Array([1]))).kind).toBe('bad');
});

test('версия снимка — без загрузки документа, чужие байты — пустая', async () => {
  const b = await v.baseVersion(base);
  expect(Object.keys(b)).toEqual(['1']);
  expect(await v.baseVersion(new Uint8Array([1, 2, 3]))).toEqual({});
  expect(await v.baseVersion(null)).toEqual({});
});
````

Создать `crates/ferrum-doc/tests/loro_compat.rs`:

````rust
//! Совместимость Loro с сервером (спека подсистемы 4, разделы 3 и 13.2): что написал `loro-crdt` на Bun, читает `loro` ядра,
//! и наоборот (`apps/server/test/loro-compat.test.ts`). Файлы данных пишут `LORO_COMPAT_WRITE=1 cargo test`
//! и однострочник в начале теста сервера; при смене версии Loro они пересобираются вместе.

use ferrum_doc::container::{decode, encode_full};
use ferrum_doc::fixture::page_with_nodes;
use ferrum_doc::*;
use loro::{ExportMode, LoroDoc, LoroValue, TreeParentId, ValueOrContainer};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/loro-compat");

#[test]
fn server_blobs_read_and_core_blobs_written() {
    let doc = LoroDoc::new();
    let tree = doc.get_tree("nodes");
    tree.enable_fractional_index(0);
    let node = tree.create(TreeParentId::Root).unwrap();
    doc.commit();
    let snapshot = doc.export(ExportMode::Snapshot).unwrap();
    let from = doc.oplog_vv();
    tree.get_meta(node).unwrap().insert("x", 42.0).unwrap();
    doc.commit();
    let update = doc.export(ExportMode::updates(&from)).unwrap();
    if std::env::var_os("LORO_COMPAT_WRITE").is_some() {
        std::fs::create_dir_all(DIR).unwrap();
        std::fs::write(format!("{DIR}/rust.snapshot"), &snapshot).unwrap();
        std::fs::write(format!("{DIR}/rust.update"), &update).unwrap();
    }

    let js = std::fs::read(format!("{DIR}/js.snapshot")).unwrap();
    let read = LoroDoc::from_snapshot(&js).unwrap();
    match read.get_map("m").get("hello") {
        Some(ValueOrContainer::Value(LoroValue::String(s))) => assert_eq!(&*s, "привет"),
        other => panic!("{other:?}"),
    }
    // Снимок страницы настоящего размера тоже читается: его заберёт сжатие на сервере.
    let (page, _) = page_with_nodes(10).unwrap();
    let meta =
        LoroDoc::decode_import_blob_meta(&page.export_full_snapshot().unwrap(), false).unwrap();
    assert!(meta.partial_end_vv.iter().count() >= 1);
}

/// Страница настоящей схемы: снимок и журнал пишет ядро, сервер (`compact`) собирает новый снимок, читает его снова ядро.
/// Текст с разметкой — главный риск: у `loro-crdt` на сервере нет настройки расширения разметки.
#[test]
fn page_compacted_by_server_reads_back_with_text_and_fills() {
    let doc = Document::new();
    let page = doc.pages()[0].0;
    let first = encode_full(&doc).unwrap();
    let mut a = decode(&first).unwrap().document;
    a.sync_start();
    let ids = a
        .transact(page, vec![], |tx| {
            let t = tx.create_node(None, None, NodeKind::Text)?;
            tx.insert_text(t, 0, "Привет, мир", None)?;
            tx.format_text(
                t,
                0..6,
                &TextAttrs {
                    size: Some(32.0),
                    ..Default::default()
                },
                &[],
            )?;
            let r = tx.create_node(None, None, NodeKind::Rect)?;
            tx.set_props(
                r,
                &PropsPatch {
                    fills: Some(vec![Fill::image("a".repeat(64))]),
                    ..Default::default()
                },
            )?;
            Ok(vec![t, r])
        })
        .unwrap();
    let snapshot = a.page(page).unwrap().export_full_snapshot().unwrap();
    if std::env::var_os("LORO_COMPAT_WRITE").is_some() {
        std::fs::write(format!("{DIR}/rust-page.snapshot"), &snapshot).unwrap();
        return;
    }
    let bytes = std::fs::read(format!("{DIR}/js-page.snapshot")).unwrap();
    let read = PageDoc::from_snapshot(&bytes).unwrap();
    let mut texts = read
        .walk()
        .into_iter()
        .filter_map(|n| read.props(n).unwrap().text.map(|t| t.content.clone()));
    let text = texts.next().expect("текст на месте");
    assert_eq!(text.text(), "Привет, мир\n");
    assert!(text.runs.len() >= 2, "разметка на месте: {:?}", text.runs);
    assert_eq!(text.runs[0].attrs.size, Some(32.0));
    assert!(read.image_refs().unwrap().contains(&"a".repeat(64)));
    assert_eq!(ids.len(), 2);
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/update-log.test.ts test/validator.test.ts test/loro-compat.test.ts`
Expected: FAIL — `Cannot find package 'loro-crdt'`, `Cannot find module '../src/sync/update-log'`: 0 pass, 3 fail

- [ ] **Step 3: Реализация**

В `apps/server/bun.lock` заменить:

````
        "@noble/hashes": "2.4.0",
        "better-auth": "1.7.16",
        "nodemailer": "10.2.0",
        "pg": "8.23.1",
````

на:

````
        "@noble/hashes": "2.4.0",
        "better-auth": "1.7.16",
        "loro-crdt": "1.16.2",
        "nodemailer": "10.2.0",
        "pg": "8.23.1",
````

В `apps/server/bun.lock` заменить:

````
    "kysely": ["kysely@0.29.8", "", {}, "sha512-ixHU1H4R+LeC5Y0nAhYgiGRFpFRHm2DXKEmJSs/vbes9sKXyssyzxzE/NYiOJX5fY0BL28biCre3GqlCqUwyXQ=="],

    "nanostores": ["nanostores@1.5.5", "", {}, "sha512-FixtE239Gl6Gz0ZGCK0WzvNwObNCIwVT+L7zQcf1QgcA6AzBxblzqe9OT88qiucNkhsjSOqiogSaF5jVbJiqSA=="],

````

на:

````
    "kysely": ["kysely@0.29.8", "", {}, "sha512-ixHU1H4R+LeC5Y0nAhYgiGRFpFRHm2DXKEmJSs/vbes9sKXyssyzxzE/NYiOJX5fY0BL28biCre3GqlCqUwyXQ=="],

    "loro-crdt": ["loro-crdt@1.16.2", "", {}, "sha512-Jbfy/P6ggYWa3joyz/P+Vqtg66+pUdF+a6PrC69bTnNQIaElJd/Tx5xBPZBa9rgMjoU3YW67v9CSMLqRi8TeQA=="],

    "nanostores": ["nanostores@1.5.5", "", {}, "sha512-FixtE239Gl6Gz0ZGCK0WzvNwObNCIwVT+L7zQcf1QgcA6AzBxblzqe9OT88qiucNkhsjSOqiogSaF5jVbJiqSA=="],

````

Создать `apps/server/migrations/004_sync.sql`:

````sql
-- Синхронизация (спека подсистемы 4, разделы 7.3 и 7.4): журнал обновлений по документам и их состояние.

CREATE TABLE doc_state (
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  -- `root` или `TreeID` страницы.
  doc text NOT NULL,
  -- Следующий номер записи журнала этого документа.
  next_seq bigint NOT NULL DEFAULT 1,
  -- Вектор версий сервера: снимок и журнал вместе.
  vv jsonb NOT NULL DEFAULT '{}',
  -- Что вошло в снимок: подписка не покрывает его — клиенту нужен снимок.
  base_vv jsonb NOT NULL DEFAULT '{}',
  log_count integer NOT NULL DEFAULT 0,
  log_bytes bigint NOT NULL DEFAULT 0,
  PRIMARY KEY (file_id, doc)
);

CREATE TABLE doc_update (
  file_id uuid NOT NULL,
  doc text NOT NULL,
  seq bigint NOT NULL,
  start_vv jsonb NOT NULL,
  end_vv jsonb NOT NULL,
  -- Байты Loro уже плотные, сжимать их базе незачем.
  bytes bytea NOT NULL,
  PRIMARY KEY (file_id, doc, seq),
  FOREIGN KEY (file_id, doc) REFERENCES doc_state ON DELETE CASCADE
);
ALTER TABLE doc_update ALTER COLUMN bytes SET STORAGE EXTERNAL;

-- Чей пир: пачка от чужого пира подделала бы чужие правки и заняла бы их счётчики.
CREATE TABLE doc_peer (
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  peer numeric(20) NOT NULL,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  PRIMARY KEY (file_id, peer)
);

-- Сжатие не удалось: следующая попытка не раньше этого времени (раздел 7.4).
ALTER TABLE file ADD COLUMN compact_after timestamptz;
````

В `apps/server/package.json` заменить:

````json
    "@noble/hashes": "2.4.0",
    "better-auth": "1.7.16",
    "nodemailer": "10.2.0",
    "pg": "8.23.1"
````

на:

````json
    "@noble/hashes": "2.4.0",
    "better-auth": "1.7.16",
    "loro-crdt": "1.16.2",
    "nodemailer": "10.2.0",
    "pg": "8.23.1"
````

Создать `apps/server/src/sync/container-read.ts`:

````ts
// Снимок одного документа из контейнера файла: сервер не держит файл в памяти, читает только нужные куски.

import { ContainerError, scanContainer } from '@ferrum/protocol';
import type { BlobStore } from '../blob-store';

const SECTION_ROOT = 1;
const SECTION_PAGE = 2;

/** Данные секции документа; файла нет, контейнер чужой или секции нет — `null`. */
export async function readDocSnapshot(blobs: BlobStore, contentKey: string | null, doc: string): Promise<Uint8Array | null> {
  if (!contentKey) return null;
  const blob = await blobs.open(contentKey);
  if (!blob) return null;
  const read = async (s: number, e: number) => new Uint8Array(await blob.slice(s, e).arrayBuffer());
  try {
    const spans = await scanContainer(blob.size, read);
    const span = spans.find((x) => (doc === 'root' ? x.kind === SECTION_ROOT : x.kind === SECTION_PAGE && x.id === doc));
    return span ? await read(span.data, span.data + span.size) : null;
  } catch (e) {
    if (e instanceof ContainerError) return null;
    throw e;
  }
}
````

Создать `apps/server/src/sync/update-log.ts`:

````ts
// Журнал обновлений (спека подсистемы 4, раздел 7.3): пачки собираются группами каждые 20 мс одной транзакцией;
// `ack` и рассылка — только после фиксации. Источник правды о порядке — строка `doc_state` под блокировкой: порядок
// журнала, повторы и непрерывность по пирам держатся и у двух процессов на одной базе.

import type { SyncRefusal } from '@ferrum/protocol';
import { ROLE_RANK } from '@ferrum/protocol';
import type { Pool, PoolClient } from 'pg';
import { inTransaction } from '../db';

export const GROUP_MS = 20;
/** Группа не больше этого: большая вставка не держит транзакцию дольше срока запроса. */
const MAX_GROUP_BYTES = 32 * 1024 * 1024;
const MAX_GROUP_ITEMS = 2000;
/** Журнал одного файла. Сжатие держит его намного меньше; если оно не удаётся, правки ждут у клиента, а диск цел. */
export const MAX_LOG_BYTES_PER_FILE = 1024 * 1024 * 1024;
/** Документов в файле: страниц столько не бывает, чужой клиент не заводит строки без конца. */
export const MAX_DOCS_PER_FILE = 10_000;

export interface Item {
  file: string;
  doc: string;
  user: string;
  bytes: Uint8Array;
  start: Record<string, number>;
  end: Record<string, number>;
}

export type Outcome =
  | { kind: 'written'; seq: number }
  /** Сервер всё это уже знал: повтор. */
  | { kind: 'known'; seq: number }
  | { kind: 'refused'; code: SyncRefusal };

interface Queued {
  item: Item;
  done: (o: Outcome) => void;
}

export interface LogLimits {
  maxLogBytesPerFile: number;
  groupMs: number;
}

interface DocRow {
  file_id: string;
  doc: string;
  next_seq: string;
  vv: Record<string, number>;
  log_bytes: string;
  log_count: number;
}

const covers = (vv: Record<string, number>, end: Record<string, number>): boolean =>
  Object.entries(end).every(([p, c]) => (vv[p] ?? 0) >= c);

/** Пачка продолжает вектор: каждый её пир начинается не дальше, чем знает сервер. */
const continues = (vv: Record<string, number>, start: Record<string, number>): boolean =>
  Object.entries(start).every(([p, c]) => (vv[p] ?? 0) >= c);

export class UpdateLog {
  private queue: Queued[] = [];
  private timer: Timer | null = null;
  private flushing = false;
  private stopped = false;
  /** После фиксации группы: рассылка и сжатие. */
  onCommitted: (items: { item: Item; seq: number }[], states: { file: string; doc: string; logCount: number; logBytes: number }[]) => void =
    () => {};
  private readonly limits: LogLimits;

  constructor(
    private readonly pool: Pool,
    limits: Partial<LogLimits> = {},
  ) {
    this.limits = { maxLogBytesPerFile: MAX_LOG_BYTES_PER_FILE, groupMs: GROUP_MS, ...limits };
  }

  submit(item: Item): Promise<Outcome> {
    if (this.stopped) return Promise.resolve({ kind: 'refused', code: 'UNAVAILABLE' });
    return new Promise((done) => {
      this.queue.push({ item, done });
      this.arm();
    });
  }

  private arm(): void {
    if (this.timer || this.flushing || this.queue.length === 0) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.flush();
    }, this.limits.groupMs);
  }

  private take(): Queued[] {
    let size = 0;
    let n = 0;
    while (n < this.queue.length && n < MAX_GROUP_ITEMS && (n === 0 || size + this.queue[n].item.bytes.length <= MAX_GROUP_BYTES)) {
      size += this.queue[n].item.bytes.length;
      n++;
    }
    return this.queue.splice(0, n);
  }

  /** По группе за раз: порядок `seq` — порядок фиксации. Сбой базы отказывает группе «повторите», процесс живёт. */
  private async flush(): Promise<void> {
    this.flushing = true;
    const group = this.take();
    let outcomes: Outcome[];
    let states: DocRow[] = [];
    try {
      const r = await inTransaction(this.pool, (c) => this.write(c, group));
      outcomes = r.outcomes;
      states = r.states;
    } catch (e) {
      console.error('Журнал обновлений: группа не записана:', e instanceof Error ? e.message : e);
      outcomes = group.map(() => ({ kind: 'refused', code: 'UNAVAILABLE' }));
    }
    this.flushing = false;
    const written: { item: Item; seq: number }[] = [];
    group.forEach((q, i) => {
      const o = outcomes[i];
      if (o.kind === 'written') written.push({ item: q.item, seq: o.seq });
    });
    try {
      if (written.length > 0) {
        this.onCommitted(
          written,
          states.map((s) => ({ file: s.file_id, doc: s.doc, logCount: s.log_count, logBytes: Number(s.log_bytes) })),
        );
      }
    } catch (e) {
      console.error('Журнал обновлений: рассылка:', e instanceof Error ? e.message : e);
    }
    group.forEach((q, i) => q.done(outcomes[i]));
    this.arm();
  }

  private async write(c: PoolClient, group: Queued[]): Promise<{ outcomes: Outcome[]; states: DocRow[] }> {
    const pairs = [...new Map(group.map((q) => [`${q.item.file}/${q.item.doc}`, q.item])).values()];
    // Первая запись в документ, снимка которого нет (новая страница). Счётчик документов — ниже, под блокировкой строк.
    await c.query(
      `INSERT INTO doc_state (file_id, doc) SELECT f, d FROM unnest($1::uuid[], $2::text[]) AS u(f, d)
       ON CONFLICT DO NOTHING`,
      [pairs.map((p) => p.file), pairs.map((p) => p.doc)],
    );
    // Порядок блокировок один на всех: два процесса не сцепятся.
    const { rows } = await c.query<DocRow>(
      `SELECT s.file_id, s.doc, s.next_seq, s.vv, s.log_bytes, s.log_count FROM doc_state s
       JOIN unnest($1::uuid[], $2::text[]) AS u(f, d) ON u.f = s.file_id AND u.d = s.doc
       ORDER BY s.file_id, s.doc FOR UPDATE OF s`,
      [pairs.map((p) => p.file), pairs.map((p) => p.doc)],
    );
    const state = new Map(rows.map((r) => [`${r.file_id}/${r.doc}`, { ...r, next: Number(r.next_seq), dirty: false }]));
    const ranks = await this.ranks(c, group);
    const logBytes = await this.fileLogBytes(c, group);
    const docCount = await this.docCounts(c, group);
    const owners = await this.peers(c, group);
    const claims: { file: string; peer: string; user: string }[] = [];
    const outcomes: Outcome[] = [];
    const inserts: { item: Item; seq: number }[] = [];
    for (const { item } of group) {
      const s = state.get(`${item.file}/${item.doc}`)!;
      const rank = ranks.get(`${item.file}/${item.user}`) ?? 0;
      if (rank < ROLE_RANK.editor) {
        outcomes.push({ kind: 'refused', code: 'FORBIDDEN' });
        continue;
      }
      if ((docCount.get(item.file) ?? 0) > MAX_DOCS_PER_FILE) {
        outcomes.push({ kind: 'refused', code: 'BAD_UPDATE' });
        continue;
      }
      // Чужой пир: иначе подделка заняла бы чужие счётчики, и настоящие правки того человека Loro бы пропускал.
      const peers = Object.keys(item.end);
      if (peers.some((p) => (owners.get(`${item.file}/${p}`) ?? item.user) !== item.user)) {
        outcomes.push({ kind: 'refused', code: 'FOREIGN_PEER' });
        continue;
      }
      if (covers(s.vv, item.end)) {
        outcomes.push({ kind: 'known', seq: s.next - 1 });
        continue;
      }
      if (!continues(s.vv, item.start)) {
        outcomes.push({ kind: 'refused', code: 'GAP' });
        continue;
      }
      const used = logBytes.get(item.file) ?? 0;
      if (used + item.bytes.length > this.limits.maxLogBytesPerFile) {
        outcomes.push({ kind: 'refused', code: 'LOG_FULL' });
        continue;
      }
      logBytes.set(item.file, used + item.bytes.length);
      for (const p of peers) {
        if (!owners.has(`${item.file}/${p}`)) {
          owners.set(`${item.file}/${p}`, item.user);
          claims.push({ file: item.file, peer: p, user: item.user });
        }
        s.vv[p] = Math.max(s.vv[p] ?? 0, item.end[p]);
      }
      const seq = s.next++;
      s.dirty = true;
      s.log_count += 1;
      s.log_bytes = String(Number(s.log_bytes) + item.bytes.length);
      inserts.push({ item, seq });
      outcomes.push({ kind: 'written', seq });
    }
    if (claims.length > 0) {
      await c.query(
        `INSERT INTO doc_peer (file_id, peer, user_id)
         SELECT f, p::numeric, u FROM unnest($1::uuid[], $2::text[], $3::text[]) AS x(f, p, u) ON CONFLICT DO NOTHING`,
        [claims.map((x) => x.file), claims.map((x) => x.peer), claims.map((x) => x.user)],
      );
    }
    if (inserts.length > 0) {
      const values = inserts.map(
        (_, i) =>
          `($${i * 6 + 1}::uuid, $${i * 6 + 2}, $${i * 6 + 3}::bigint, $${i * 6 + 4}::jsonb, $${i * 6 + 5}::jsonb, $${i * 6 + 6}::bytea)`,
      );
      await c.query(
        `INSERT INTO doc_update (file_id, doc, seq, start_vv, end_vv, bytes) VALUES ${values.join(',')}`,
        inserts.flatMap((i) => [
          i.item.file,
          i.item.doc,
          String(i.seq),
          JSON.stringify(i.item.start),
          JSON.stringify(i.item.end),
          Buffer.from(i.item.bytes.buffer, i.item.bytes.byteOffset, i.item.bytes.byteLength),
        ]),
      );
      const dirty = [...state.values()].filter((s) => s.dirty);
      await c.query(
        `UPDATE doc_state s SET next_seq = u.n::bigint, vv = u.v::jsonb, log_count = u.c::integer, log_bytes = u.b::bigint
         FROM unnest($1::uuid[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[]) AS u(f, d, n, v, c, b)
         WHERE s.file_id = u.f AND s.doc = u.d`,
        [
          dirty.map((s) => s.file_id),
          dirty.map((s) => s.doc),
          dirty.map((s) => String(s.next)),
          dirty.map((s) => JSON.stringify(s.vv)),
          dirty.map((s) => String(s.log_count)),
          dirty.map((s) => s.log_bytes),
        ],
      );
      await c.query('UPDATE file SET edited_at = now() WHERE id = ANY($1::uuid[])', [[...new Set(inserts.map((i) => i.item.file))]]);
    }
    return { outcomes, states: rows };
  }

  /** Роль проверяется в транзакции записи на каждой группе: снятая только что не запишет ни байта. */
  private async ranks(c: PoolClient, group: Queued[]): Promise<Map<string, number>> {
    const pairs = [...new Map(group.map((q) => [`${q.item.file}/${q.item.user}`, q.item])).values()];
    const { rows } = await c.query<{ f: string; u: string; rank: number | null }>(
      `SELECT f, u, file_role_rank(f, u) AS rank FROM unnest($1::uuid[], $2::text[]) AS x(f, u)`,
      [pairs.map((p) => p.file), pairs.map((p) => p.user)],
    );
    return new Map(rows.map((r) => [`${r.f}/${r.u}`, r.rank ?? 0]));
  }

  private async fileLogBytes(c: PoolClient, group: Queued[]): Promise<Map<string, number>> {
    const files = [...new Set(group.map((q) => q.item.file))];
    const { rows } = await c.query<{ file_id: string; total: string }>(
      'SELECT file_id, sum(log_bytes) AS total FROM doc_state WHERE file_id = ANY($1::uuid[]) GROUP BY file_id',
      [files],
    );
    return new Map(rows.map((r) => [r.file_id, Number(r.total)]));
  }

  private async docCounts(c: PoolClient, group: Queued[]): Promise<Map<string, number>> {
    const files = [...new Set(group.map((q) => q.item.file))];
    const { rows } = await c.query<{ file_id: string; n: string }>(
      'SELECT file_id, count(*) AS n FROM doc_state WHERE file_id = ANY($1::uuid[]) GROUP BY file_id',
      [files],
    );
    return new Map(rows.map((r) => [r.file_id, Number(r.n)]));
  }

  private async peers(c: PoolClient, group: Queued[]): Promise<Map<string, string>> {
    const asked = [...new Set(group.flatMap((q) => Object.keys(q.item.end).map((p) => `${q.item.file}/${p}`)))];
    if (asked.length === 0) return new Map();
    const { rows } = await c.query<{ file_id: string; peer: string; user_id: string }>(
      `SELECT p.file_id, p.peer::text AS peer, p.user_id FROM doc_peer p
       JOIN unnest($1::uuid[], $2::numeric[]) AS x(f, n) ON x.f = p.file_id AND x.n = p.peer`,
      [asked.map((a) => a.split('/')[0]), asked.map((a) => a.split('/')[1])],
    );
    return new Map(rows.map((r) => [`${r.file_id}/${r.peer}`, r.user_id]));
  }

  /** Остановка: всё принятое дописывается. */
  async stop(): Promise<void> {
    this.stopped = true;
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    while (this.queue.length > 0 || this.flushing) {
      if (!this.flushing) await this.flush();
      else await Bun.sleep(5);
    }
  }
}
````

Создать `apps/server/src/sync/validator.ts`:

````ts
// Сторона сервера у worker'а проверки: очередь с пределом, замена worker'а после паники и срока, загрузка документа
// по первому требованию. Основной поток Loro не загружает: паника wasm не задевает приём запросов.

import type { Pool } from 'pg';
import type { BlobStore } from '../blob-store';
import { readDocSnapshot } from './container-read';
import type { FromValidator, ToValidator } from './validator.worker';

/** Больше этого в очереди проверки: клиент получает паузу, сервер не копит байты без конца. */
export const MAX_VALIDATOR_QUEUE_BYTES = 256 * 1024 * 1024;
/** Дольше этого на одну работу — worker завис: замена, работы очереди получают отказ «повторите». */
const WORK_DEADLINE_MS = 60_000;
/** Столько загрузок на один worker: память wasm не возвращается системе (тот же приём, что в разделе 7.4). */
const LOADS_PER_WORKER = 50;

export type Verdict =
  | { kind: 'ok'; start: Record<string, number>; end: Record<string, number>; pending: boolean }
  | { kind: 'bad' }
  /** Worker заменён не по вине этой пачки или очередь полна: повторить. */
  | { kind: 'retry' };

interface Waiting {
  done: (m: FromValidator | null) => void;
  size: number;
  at: number;
}

export class Validator {
  private worker: Worker;
  private seq = 0;
  private loads = 0;
  private queued = 0;
  private readonly waiting = new Map<number, Waiting>();
  private readonly loading = new Map<string, Promise<void>>();
  private readonly watchdog: Timer;
  private stopped = false;
  /** Тесты: сколько раз worker заменили. */
  restarts = 0;

  constructor(
    private readonly pool: Pool,
    private readonly blobs: BlobStore,
  ) {
    this.worker = this.spawn();
    this.watchdog = setInterval(() => this.checkHung(), 1000);
    this.watchdog.unref?.();
  }

  private spawn(): Worker {
    const w = new Worker(new URL('./validator.worker.ts', import.meta.url));
    w.onmessage = (e: MessageEvent<FromValidator>) => this.answer(e.data);
    w.onerror = (e) => {
      console.error("Проверка пачек: сбой worker'а:", e.message);
      e.preventDefault?.();
      this.restart();
    };
    w.unref?.();
    return w;
  }

  private answer(m: FromValidator): void {
    const w = this.waiting.get(m.id);
    if (!w) return;
    this.waiting.delete(m.id);
    this.queued -= w.size;
    w.done(m);
    // Паника: модуль wasm испорчен, остальная очередь пойдёт новому worker'у.
    if (!m.ok && m.reason === 'bad' && m.fatal) this.restart();
  }

  /** Новый worker без документов. Ждавшие получают «повторите»: их вина не доказана. */
  private restart(): void {
    if (this.stopped) return;
    this.restarts++;
    this.worker.terminate();
    this.worker = this.spawn();
    this.loads = 0;
    this.loading.clear();
    for (const w of this.waiting.values()) w.done(null);
    this.waiting.clear();
    this.queued = 0;
  }

  private checkHung(): void {
    const now = performance.now();
    for (const w of this.waiting.values()) {
      if (now - w.at > WORK_DEADLINE_MS) {
        console.error('Проверка пачек: worker не отвечает, замена');
        this.restart();
        return;
      }
    }
  }

  private send(work: Omit<ToValidator, 'id'>, size = 0): Promise<FromValidator | null> {
    if (this.stopped) return Promise.resolve(null);
    const id = ++this.seq;
    this.queued += size;
    return new Promise((done) => {
      this.waiting.set(id, { done, size, at: performance.now() });
      this.worker.postMessage({ ...work, id } as ToValidator);
    });
  }

  private ensureLoaded(file: string, doc: string): Promise<void> {
    const key = `${file}/${doc}`;
    const going = this.loading.get(key);
    if (going) return going;
    const p = this.load(file, doc, key).finally(() => this.loading.delete(key));
    this.loading.set(key, p);
    return p;
  }

  private async load(file: string, doc: string, key: string): Promise<void> {
    if (++this.loads > LOADS_PER_WORKER && this.waiting.size === 0) this.restart();
    const client = await this.pool.connect();
    let snapshot: Uint8Array | null;
    let rows: { bytes: Uint8Array }[];
    try {
      // Содержимое и журнал — одним снимком базы: сжатие между ними не потеряет и не удвоит записи.
      await client.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');
      const f = await client.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [file]);
      snapshot = await readDocSnapshot(this.blobs, f.rows[0]?.content_key ?? null, doc);
      rows = (
        await client.query<{ bytes: Uint8Array }>('SELECT bytes FROM doc_update WHERE file_id = $1 AND doc = $2 ORDER BY seq', [file, doc])
      ).rows;
      await client.query('COMMIT');
    } catch (e) {
      await client.query('ROLLBACK').catch(() => {});
      throw e;
    } finally {
      client.release();
    }
    await this.send({ op: 'load', key, snapshot, log: rows.map((r) => r.bytes) } as Omit<ToValidator, 'id'>);
  }

  /** Вердикт по пачке. Вызовы одного документа отвечают в порядке вызова. Байты должны жить до ответа. */
  async validate(file: string, doc: string, bytes: Uint8Array): Promise<Verdict> {
    if (this.queued + bytes.length > MAX_VALIDATOR_QUEUE_BYTES) return { kind: 'retry' };
    const key = `${file}/${doc}`;
    for (let attempt = 0; attempt < 3; attempt++) {
      if (this.loading.has(key)) await this.loading.get(key)!.catch(() => {});
      const r = await this.send({ op: 'validate', key, bytes } as Omit<ToValidator, 'id'>, bytes.length);
      if (!r) return { kind: 'retry' };
      if (r.ok && 'end' in r) return { kind: 'ok', start: r.start, end: r.end, pending: r.pending };
      if (!r.ok && r.reason === 'bad') return { kind: 'bad' };
      try {
        await this.ensureLoaded(file, doc);
      } catch (e) {
        console.error('Проверка пачек: не удалось загрузить документ:', e instanceof Error ? e.message : e);
        return { kind: 'retry' };
      }
    }
    return { kind: 'retry' };
  }

  /** Вектор версий снимка, без загрузки документа. */
  async baseVersion(snapshot: Uint8Array | null): Promise<Record<string, number>> {
    if (!snapshot || snapshot.length === 0) return {};
    const r = await this.send({ op: 'base', snapshot } as Omit<ToValidator, 'id'>);
    return r?.ok && 'base' in r ? r.base : {};
  }

  /** Файл заменён целиком: его документы в памяти больше не верны. */
  forget(file: string): void {
    void this.send({ op: 'forget', prefix: `${file}/` } as Omit<ToValidator, 'id'>);
  }

  stop(): void {
    this.stopped = true;
    clearInterval(this.watchdog);
    this.worker.terminate();
    for (const w of this.waiting.values()) w.done(null);
    this.waiting.clear();
  }
}
````

Создать `apps/server/src/sync/validator.worker.ts`:

````ts
// Проверка пачек перед журналом (спека подсистемы 4, раздел 7.3): байты разбираются и применяются к настоящему
// документу файла. Пачка, которая разбирается на пустом документе, может уронить Loro на настоящем (проба: 1 из
// 3619 случайных порч), и журнал отравил бы файл всем. Паника wasm портит весь модуль, поэтому worker отдельный:
// сервер его заменяет и пачку отвергает.

import { decodeImportBlobMeta, LoroDoc } from 'loro-crdt';

/** Документов в памяти на столько байт снимков и журнала: дальше вытесняются давние. */
const BUDGET_BYTES = Number(process.env.FERRUM_VALIDATOR_BUDGET ?? 512 * 1024 * 1024);
/** Без пачек дольше этого документ уходит из памяти, следующая пачка загрузит его заново. */
const IDLE_MS = 120_000;
/** Дольше этого идёт только параллельная правка дерева: Loro откатывает всё дерево. */
const HEAVY_IMPORT_MS = 30;
/** Документ с кэшем истории уходит раньше: в `loro-crdt` кэш отдельно не освободить. */
const HEAVY_IDLE_MS = 5000;

export type ToValidator =
  | { id: number; op: 'validate'; key: string; bytes: Uint8Array }
  | { id: number; op: 'load'; key: string; snapshot: Uint8Array | null; log: Uint8Array[] }
  | { id: number; op: 'forget'; prefix: string }
  | { id: number; op: 'base'; snapshot: Uint8Array };

export type FromValidator =
  | { id: number; ok: true; start: Record<string, number>; end: Record<string, number>; pending: boolean; loaded: boolean }
  /** Документа нет в памяти: сервер пришлёт его и повторит. */
  | { id: number; ok: false; reason: 'need-load' }
  /** Плохие байты. `fatal` — Loro упал: модуль испорчен, worker надо заменить. */
  | { id: number; ok: false; reason: 'bad'; fatal: boolean }
  | { id: number; ok: true; base: Record<string, number> }
  | { id: number; ok: true; done: true };

interface Held {
  /** `null` — снимок не читается (его записал не Ferrum): проверяем только разбор, файл живёт как жил. */
  doc: LoroDoc | null;
  bytes: number;
  used: number;
  /** Loro построил кэш истории (параллельная правка дерева): на большом документе это сотни мегабайт. */
  heavy: boolean;
}

const scope = globalThis as unknown as Worker;
const docs = new Map<string, Held>();
let held = 0;

const plain = (m: { toJSON(): Map<string, number> }): Record<string, number> => Object.fromEntries(m.toJSON());

function drop(key: string): void {
  const h = docs.get(key);
  if (!h) return;
  docs.delete(key);
  held -= h.bytes;
  h.doc?.free();
}

function makeRoom(need: number): void {
  const oldest = [...docs.entries()].sort(([, a], [, b]) => a.used - b.used);
  for (const [key] of oldest) {
    if (held + need <= BUDGET_BYTES) return;
    drop(key);
  }
}

function load(key: string, snapshot: Uint8Array | null, log: Uint8Array[]): void {
  drop(key);
  const bytes = (snapshot?.length ?? 0) + log.reduce((n, b) => n + b.length, 0);
  makeRoom(bytes);
  let doc: LoroDoc | null;
  try {
    doc = snapshot && snapshot.length > 0 ? LoroDoc.fromSnapshot(snapshot) : new LoroDoc();
    for (const b of log) doc.import(b);
  } catch {
    doc = null;
  }
  docs.set(key, { doc, bytes, used: performance.now(), heavy: false });
  held += bytes;
}

function validate(id: number, key: string, bytes: Uint8Array): FromValidator {
  const h = docs.get(key);
  if (!h) return { id, ok: false, reason: 'need-load' };
  const meta = decodeImportBlobMeta(bytes, false);
  if (meta.mode !== 'update') throw new BadUpdate();
  const start = plain(meta.partialStartVersionVector);
  const end = plain(meta.partialEndVersionVector);
  let pending = false;
  if (h.doc) {
    const at = performance.now();
    const status = h.doc.import(bytes);
    pending = status.pending != null;
    if (performance.now() - at > HEAVY_IMPORT_MS) h.heavy = true;
  } else {
    const scratch = new LoroDoc();
    scratch.import(bytes);
    scratch.free();
  }
  h.used = performance.now();
  h.bytes += bytes.length;
  held += bytes.length;
  return { id, ok: true, start, end, pending, loaded: h.doc !== null };
}

class BadUpdate extends Error {}

/** Конец версий снимка: документ не загружается. Чужие байты — пустой вектор. */
function base(snapshot: Uint8Array): Record<string, number> {
  if (snapshot.length === 0) return {};
  try {
    return plain(decodeImportBlobMeta(snapshot, true).partialEndVersionVector);
  } catch {
    return {};
  }
}

scope.onmessage = (e: MessageEvent<ToValidator>) => {
  const m = e.data;
  try {
    switch (m.op) {
      case 'validate':
        scope.postMessage(validate(m.id, m.key, m.bytes));
        return;
      case 'load':
        load(m.key, m.snapshot, m.log);
        scope.postMessage({ id: m.id, ok: true, done: true });
        return;
      case 'forget':
        for (const key of docs.keys()) if (key.startsWith(m.prefix)) drop(key);
        scope.postMessage({ id: m.id, ok: true, done: true });
        return;
      case 'base':
        scope.postMessage({ id: m.id, ok: true, base: base(m.snapshot) });
        return;
    }
  } catch (err) {
    // Обычная ошибка разбора — плохие байты. Паника wasm (`unreachable`) ещё и портит модуль.
    const fatal = err instanceof WebAssembly.RuntimeError || (err instanceof Error && /unreachable/i.test(err.message));
    scope.postMessage({ id: m.id, ok: false, reason: 'bad', fatal } satisfies FromValidator);
  }
};

setInterval(() => {
  const now = performance.now();
  for (const [key, h] of docs) {
    if (now - h.used > (h.heavy ? HEAVY_IDLE_MS : IDLE_MS)) drop(key);
  }
}, 1000);
````

- [ ] **Step 4: Сгенерировать файлы**

Run: `cd apps/server && bun install --frozen-lockfile`
Run: `mkdir -p apps/server/test/data && printf %s 'bG9ybwAAAAAAAAAAAAAAAEA4fLAAA78FAABMT1JPAAQiTRhgQIKEBQAAsgD+AwD+AwEQAQEAAQADCQDwAAUBAAABANoDUQQBAwAGBBwAMgQAAgoAEBgKABAeFAAQMAoAEDYKABBICgAQTgoAEGAKABBmCgAQeAoAEH4KACCQAQsAIJYBDAARqAwAEa4MABHADAARxgwAEdgMABHeDAAR8AwAEfYMACCKAjwAIJACPAARpAwAEaoMABG+DAARxAwAEdgMABHeDAAR8gwAEfgMACCMAzwAIJIDPAARpgwAEawMABHADAARxgwAEdoMABHgDAAR9AwAEfoMACCOBDwAIJQEPAARqAwAEa4MABHCDAARyAwAEdwMABHiDAAR9gwAEfwMACCQBTwAIJYFPAARqgwAEbAMABHEDAARygwAEd4MABHkDAAR+AwAEf4MACCSBjwAEZh0ARGsDAARsgwAEcYMABHMDAAR4AwAEeYMABH6DAAggAdsACCUBzwAEZoMABGuDAARtAwAEcgMABHODAAR4gwA8P9u6AcQAmlkAXgEdGV4dAVub2Rlc34BAgJQAHgoAYACgYACgoACg4AChIAChYAChoACh4ACiIACiYACioACi4ACjIACjYACjoACj4ACkIACkYACkoACk4AClIAClYACloACl4ACmIACmYACmoACm4ACnIACnYACnoACn4ACoIACoYACooACo4ACpIACpYACpoACp4DjBgEEgAIDAAIEAAUCAwYEAAUCBwoEAAUCCw4EAAUCDxIEAAUCExYEAAUCFxoEAAUCGx4EAAUCHyIEAAUCIyYEAAUCJyoEAAUCKy4EAAUCLzIEAAUCMzYEAAUCNzoEAAUCOz4EAAUCP0IEAAUCQ0YEAAUCR0oEAAUCS04EAAUCT1IEAAUCU1YEAAUCV1oEAAUCW14EAAUCX2IEAAUCY2YEAAUCZ2oEAAUCa24EAAUCb3IEAAUCc3YEAAUCd3oEAAUCe34EAAUCf4IBBAAFAoMBhgEEAAUChwGKAQQABQKLAY4BBAAFAo8BkgEtACCTAQ4DUQUClwGaKACAmwGeAQQAAQL6Ak8EAgEDBgDXj8kBARAGCwMFBQCujwEFoAEIAQEIBAAUHwkEAGH8FwDpCAAAAAEFATADAAkCDtC/0YDQuNCy0LXRgiAwAAwBAQUBMQMBGgCsMQAYAgEFATIDAhoArDIAJAMBBQEzAwMaAKwzADAEAQUBNAMEGgCsNAA8BQEFATUDBRoArDUASAYBBQE2AwYaAKw2AFQHAQUBNwMHGgCsNwBgCAEFATgDCBoArDgAbAkBBQE5AwkaAOk5AHgKAQUCMTADCgkCDwUBYDEwAIUBCx0APTEDCx0AUDEAkgEMHQA9MgMMHQBQMgCfAQ0dAD0zAw0dAFAzAKwBDh0APTQDDh0AUDQAuQEPHQA9NQMPHQBQNQDGARAdAD02AxAdAFA2ANMBER0APTcDER0AUDcA4AESHQA9OAMSHQBQOADtARMdAD05AxMdAMw5APoBFAEFAjIwAxQdAGAyMACHAhUdAD0xAxUdAFAxAJQCFh0APTIDFh0AUDIAoQIXHQA9MwMXHQBQMwCuAhgdAD00AxgdAFA0ALsCGR0APTUDGR0AUDUAyAIaHQA9NgMaHQBQNgDVAhsdAD03AxsdAFA3AOICHB0APTgDHB0AUDgA7wIdHQA9OQMdHQDMOQD8Ah4BBQIzMAMeHQBgMzAAiQMfHQA9MQMfHQBQMQCWAyAdAD0yAyAdAFAyAKMDIR0APTMDIR0AUDMAsAMiHQA9NAMiHQBQNAC9AyMdAD01AyMdAFA1AMoDJB0APTYDJB0AUDYA1wMlHQA9NwMlHQBQNwDkAyYdAD04AyYdAFA4APEDJx0APTkDJx0A8Aw5AAIAZnIBAfoHAAIAdnYBAfwHAABbCmQKAwAAAAAAXkqthAEAAAAFAAAADAAAAAAAAAAAAQAAAAABAgB2dkbrZ9ycBQAAHwkAAExPUk8ABCJNGGBAgocGAADyFgACAQAFbm9kZXMEAwJpZAQBMAR0ZXh0BwEBBgABeAMAAAEBAAACAMwBAAMAAgkEAAcBAAA3ADQCMjE4ACCUBDkAFyo5AHCIAgCKAgCJPAAQDEsADHMABTsAEB46ABcCOgCfDQAPAA4JBAAUcwACFDJzABGucwAXLDkAcJUCAJcCAJZzAB8YcwABBTsAEDZzABcEOgCfGQAbABoJBAAhcwACFDNzABHIcwAXLjkAcKICAKQCAKNzAB8kcwABBTsAEE5zABcGOgCfJQAnACYJBAAucwACFDRzABHicwAXMDkAcK8CALECALBzAB8wcwABBTsAEGZzABcIOgCfMQAzADIJBAA7cwACFDVzABH8cwAXMjkAcLwCAL4CAL1zAB88cwABBTsAEH5zABcKOgCfPQA/AD4JBABINwABFDZyACCWATgAFww4AFBJAEsASjgAD6sAAgY5ABAFOQAXNDkAcMkCAMsCAMqrAB9UdAABFDd0ABGudAAXDjsAn1UAVwBWCQQAVXQAAgU5ABGwdAAXNjkAcNYCANgCANd0AB9gdAABFDh0ABHGdAAXEDsAn2EAYwBiCQQAYnQAAgU5ABHKdAAXODkAcOMCAOUCAOR0AB9sdAABFDl0ABHedAAXEjsAn20AbwBuCQQAb3QAAgU5ABHkdAAXOjkAcPACAPICAPF0AB94dAAANAIxMHUAEfZ1ABcUPACfeQB7AHoJBAB8dQABFTM5ABH+dQAXPDkAcP0CAP8CAP51AB+FdQACFDF1ACCQAg0CFxY8AM+GAQCIAQCHAQkEAIl4AAIFPAARmPkEFz48AM+KAwCMAwCLAwkEAJJ4AAIFTgQRqngAFxg8AHCTAQCVAQCUeAAflngAAgU8ABGyeAAXQDwAcJcDAJkDAJh4AB+feAACBVMEEcR4ABcaPABwoAEAogEAoXgAH6N4AAIFPAARzHgAF0I8AHCkAwCmAwCleAAfrHgAAgVYBBHeeAAXHDwAcK0BAK8BAK54AB+weAACBTwAEeZ4ABdEPABwsQMAswMAsngAH7l4AAIFXQQR+HgAFx48AHC6AQC8AQC7eAAfvXgAAgU8ACCABxwCF0Y8AHC+AwDAAwC/eAAfxngAAgVlBCCSAzwAFyA8AHDHAQDJAQDIeAAfyngAAgU8ABGaeAAXSDwAcMsDAM0DAMx4AB/TeAACBWkEEax4ABciPABw1AEA1gEA1XgAH9d4AAIFPAARtHgAF0o8AHDYAwDaAwDZeAAf4HgAAgVtBBHGeAAXJDwAcOEBAOMBAOJ4AB/keAACBTwAEc54ABdMPABw5QMA5wMA5ngAH+14AAIFcQQR4HgAFyY8AHDuAQDwAQDveAAf8XgAAgU8ABHoeAAXTjwAcPIDAPQDAPN4AB/6eAABFTJ0BBH6eAAXKDwAoPsBAP0BAPwBAA3zCAA+ADAAAAMACfUHAwEBAQABDtC/0YDQuNCy0LXRgiAwASMAcQQCAQACAQgGABEQyggUAhwAIwoBPwBJjgQBD0AAKjIxQQAyAwGWSAAaEkIAFA+BABsYgQALQAA/AgEggQABFBeBAB2ogQAcMoEAH7CBAAIUG4EAGzCBAAtAAD8CATiBAAEUJIEAHcKBABwzgQAfyoEAAhQngQAbSIEAC0AAPwIBUIEAARQxgQAd3IEAHDSBAB/kgQACFDOBABtggQALQAA/AgFogQABFD6BAB32gQAcNYEAH/6BAAIUP4EAG3iBAA1AAC+AAYIAARRLQAArkAFBABw2gQAfmEEAAwTDACyQBccCDkIAEQWDAAvHAhRXgwAcqIMAHDeDAB+wgwACFFiDAB2qgwANQgAfsoMAAhRjgwAcwIMAHDiDAB/IgwACFGWDAB3EgwANQgAfzIMAAhRvgwAc2IMAHDmDAB/ggwACFHKDAB3egwANQgAf5oMAAhR7gwAr8AHLARsx0wQyAwH4hAALywEUf4QAHPiEAB0zQgAvgAYNAgEUiIQALIoChAAL1gRPAwGSAkIAARSMhAArkgZCAB0zQgAfmoQAAhSVhAAdpIQAC9kENgMBrIQA8HIAMABsAKMA3wAWAVIBiQHFAfwBOAJvAqcC4wIbA1cDjwPLAwMEPwR4BLQE8AQsBWgFpAXgBRwGWAaUBtAGDAdIB4QHwAf8BzgIdAiwCOwIKAlnCakJ6AkqCmkKqwrqCiwLawutC+0LLgxwDLEM8ww0DXYNtw35DTsOfQ6/DgEPQAAAAAAAIrZ93QQiTRhgQIIhAgAA8gwCAwEBAawGAQ/Qv9GA0LjQstC10YIgMzIBAQABAPEJAwQCAQADAbQGAgEAAgESAAAJBACiAAAAOQArvgI5ACwxMzkAJ8YCOQAjpgE5AB3GcgANOQAYznIAFK9yAB3YcgAcNHIAGOByABSzcgAd4HIADTkAGOhyABS8cgAd8nIAHDVyABj6cgAUwHIAHfpyAA05ACeCBx0BFMlyACyMA1YBHDZyACeUAzkAFM1yACuUBzkAHTM5ABiccgAU1nIAHaZyABw3cgAYrnIAFNpyAB2ucgANOQAYtnIAFONyAB3AcgAcOHIAGMhyABTncgAdyHIADTkAGNByABTwcgAd2nIAHDlyABjicgAU9HIAHeJyAA05ABjqcgAU/XIAHPRyACwyMHIAFfxyANUABwCDBW5vZGVzAwEAJwD2AQQCAlAABgEAFBg6GgUCUAAOAPDCAlAAKSgAAQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyAhIiMkJSYnfgECAlAAeCgBgAKBgAKCgAKDgAKEgAKFgAKGgAKHgAKIgAKJgAKKgAKLgAKMgAKNgAKOgAKPgAKQgAKRgAKSgAKTgAKUgAKVgAKWgAKXgAKYgAKZgAKagAKbgAKcgAKdgAKegAKfgAKggAKhgAKigAKjgAKkgAKlgAKmgAKngAAAADIAawCkAN0AFgFPAYgBwQH6ATMCbAKlAt4CFwNQA4kDEQAAAAAA7zYFBgIAAAAFAAAADQAAAQAAAAAAAAAAAAAAAQ0AAgEAAAAAAAAAlQAAAJ8GAAANAAIBAAAAAAAAAJkBAAABBwCDBW5vZGVzvo1VjdMIAAAAAAAA' | base64 -d > apps/server/test/data/loro-base.snap`
Run: `mkdir -p apps/server/test/data && printf %s 'bG9ybwAAAAAAAAAAAAAAACH7hbsABKEB/gMI/gMIARkCAQAAAAAAAAD//////////wABAQAAAAAABQEAAAEAFgQEAAAASAQAAgBmBAACAJYBBAEDAAIIAXgFbm9kZXMJAQICAQADAQGAHgEEBgEABgIEAAYFAAIBBgAGBQsFCQYQBgEBBAIGAQwBAwIBAAMBmAECAQQjBD/gAAAAAAAAAmFiAIMEAAAAPABUAAAAYABsAAAB/////wc=' | base64 -d > apps/server/test/data/loro-good.update`
Run: `mkdir -p apps/server/test/data && printf %s 'bG9ybwAAAAAAAAAAAAAAAP8EwvgABKEB/gMI/gMIARkCAQAAAAAAAAD//////////9sBAQAAAAAABQEAAAEAFgQEAAAASAQAAgBmBAACAJYBBAEDAAIIAXgFbm9kZXMJAQICAQADAQGAHgEEBgEABgIEAAYFAAIBBgAGBQsFCQYQBgEBBAIGAQwBAwIBAAMBmAECAQQjBD/gAAAAAAAAAmFiAIMEAAAAPABUAAAAYABsAAAB/////wc=' | base64 -d > apps/server/test/data/loro-panic-any.update`
Run: `mkdir -p apps/server/test/data && printf %s 'bG9ybwAAAAAAAAAAAAAAALULq5wABKEB/gMI/gMIARkCAQAAAAAAAAD//////////wABAQAAAAAABQEAAAEAFgQEAAAASAQAAgBrBAACAJYBBAEDAAIIAXgFbm9kZXMJAQICAQADAQGAHgEEBgEABgIEAAYFAAJVBgAGBQsFCQYQBgEBBAIGAQwBAwIBAAMBmAECAQQjBD/gAAAAAAAAAmFiAIMEAAAAPABUAAAAYABsAAAB/////wc=' | base64 -d > apps/server/test/data/loro-panic-real.update`
Run: `mkdir -p crates/ferrum-doc/tests/data/loro-compat && printf %s 'bG9ybwAAAAAAAAAAAAAAADq8TmcAAxADAABMT1JPAAQiTRhgQILVAgAA8AMAMQAxARABHBGrh6TIjgsBAQABAOAFAQAAAQAVBAQBAwA+BBIAUAQAAgAuCgDyUk7xAQJpZAR0eXBlBG5hbWUBdwFoC2NsaXBDb250ZW50BWZpbGxzB3Zpc2libGUHb3BhY2l0eQlzY2FsZU1vZGUEa2luZAVjb2xvcgdzdHJva2VzBnNpemluZwZhbGlnbkgHAMBWEHBhcmFncmFwaFNDADVuZw8RAPMrSW5kZW50CHRydW5jYXRlCG1heExpbmVzBGZvbnQFc3R5bGUEc2l6ZQpsaW5lSGVpZ2h0DWxldHRlcksA8KIEdW5pdAV2YWx1ZQpkZWNvcmF0aW9uBGNhc2UEdGV4dAlpbWFnZUhhc2gFbm9kZXMMAQICBAAGAgGAAoGARAEEDwMAAiwADQIAAQIAAwYQABgEAAwCAQwYAgEGBAIHOQAEAwYADAIDDAsRARAcCwEIEAsEBQcLDAAQEgsGMgEBCxoBAPwDAAAAAQUUMEA4MzI4MjM1OTAwODY0NDczODgFBHRleHQFCtCi0LXQutGB0YJ5AQECAAUJALECBwEIBQcBCAQ/8BkA8QYACQUEZmlsbAoFBXNvbGlkCwUJIzABAPULZmYHAAUJYXV0b1dpZHRoBQRsZWZ0BQN0b3BNAAUJAPIFAgUFSW50ZXIFB1JlZ3VsYXIEQChfACAFBEIA1QgCGQUHcGVyY2VudBo2AGIFBG5vbmUGAP8LCQIBChTQn9GA0LjQstC10YIsINC80LjRgAUWAAJihAYWBEBAYwCPACcBAQUVMzkSAQJgcmVjdAUaWQDyCtGP0LzQvtGD0LPQvtC70YzQvdC40LoEQFlGAAUJAA0iAQQbAQMqAQAiASBkOQIAACIBrwcBCAYHAR4FQGEBACwDXgAGmQExCgUFTwIAcAATZgEA9gYAAgBmcgGcoqy9yJSyxwtgAAIAdnYQAJBiAAB2A4YDAwAAAAAAll6OjQEAAAAFAAAADAALjsikh6sRHAAAAAABAgB2du68ECztAgAAKAMAAExPUk8ABCJNGGBAgucCAADyMAACAQAFbm9kZXMEFgpkZWNvcmF0aW9uBARub25lC2NsaXBDb250ZW50AQAQcGFyYWdyYXBoU3BhY2luZwIAAAIAYgRzaXplAgwA8gMoQAR0eXBlBAR0ZXh0BGNhc2VMAPMRBmFsaWduSAQEbGVmdApsaW5lSGVpZ2h0BARhdXRvAXc/ACIAACkA01YEA3RvcA1sZXR0ZXJwAPQGBgIEdW5pdAQHcGVyY2VudAV2YWx1eADxJAAABG5hbWUEFNCf0YDQuNCy0LXRgiwg0LzQuNGAB3N0cm9rZXMFAARmb250BAVJbnRlcqMA8CUHAZyirL3IlLLHCy4AAmlkBBQwQDgzMjgyMzU5MDA4NjQ0NzM4OAh0cnVuY2F0ZQEAAWgCbgAAAgDzDQVmaWxscwUBBgUHdmlzaWJsZQEBB29wYWNpdHnQAODwPwlzY2FsZU1vZGUEBDAA8QcEa2luZAQFc29saWQFY29sb3IECSMwAQA1ZmYPZAFjSW5kZW50RgCwAAAGc2l6aW5nBAkuAfdNV2lkdGgFc3R5bGUEB1JlZ3VsYXIBCG1heExpbmVzARwRq4ekyI4LAAoACwAWAAYAFQAHABAABQABABQAEwAPACQADQAMABIACQAIABEAFwAOAAIABAkEACcAAAARAhoIAAID2AFFcmVjdK8BIFlAMwE/FTM5NAEABSkBJVlAKQEWBikB3wlpbWFnZUhhc2gEQGEBACwMZAENhQEDdQEBcwAFdQETZgEABx4CAUMCEBpDAvYI0Y/QvNC+0YPQs9C+0LvRjNC90LjQugBdAfQELQAwACwAKAAqAC8AKQArAA0AAnkBEBdHASYDAVoCMAABFVQADJcCFQpOAMADBAIKAAYFShcaBA0KAHEJAAwNDAcBWQMzAQMABwJyQECEAAcAg7ADJgMBjQDTBAICBAADAwBOBQIEAAsAAAcAUAIAAQwBHADwAwYCAYACgYAAAAAKAkkDsAMEAAAAAACRNWphAQAAAAUAAAANAAAcEauHpMiOCwAAAAABBwCDBW5vZGVzc/760f8CAAAAAAAA' | base64 -d > crates/ferrum-doc/tests/data/loro-compat/js-page.snapshot`
Run: `mkdir -p crates/ferrum-doc/tests/data/loro-compat && printf %s 'bG9ybwAAAAAAAAAAAAAAAFVwCYUAA6wAAABMT1JPAAGx2uyG47HxnuQBAAACAHZ2AbHa7IbjsfGe5AECAAwA5D3FjjDbLTEAAAAAAAEAAQEQATEt2zCOxT3kAQEAAAAAAAUBAAABAAYBBAEAAAIIBWhlbGxvAW0ADgEEAgEAAgEAAgELAgEBAA4FDNC/0YDQuNCy0LXRggAADAAdAAMAPwbpGAEAAAAFAAAAAgBmcgAMAOQ9xY4w2y0xAAAAAOsb8W6JAAAATAAAAExPUk8AAAEAAQVoZWxsbwQM0L/RgNC40LLQtdGCAAExLdswjsU95AAAAAABAFsgM+MBAAAABQAAAAMAgAFtAAMAgAFtT54qlTEAAAAAAAAA' | base64 -d > crates/ferrum-doc/tests/data/loro-compat/js.snapshot`
Run: `mkdir -p crates/ferrum-doc/tests/data/loro-compat && printf %s 'bG9ybwAAAAAAAAAAAAAAAFe5CWcAAy8DAABMT1JPAAQiTRhgQIL0AgAA8f+cADEAMQEQARwRq4ekyI4LAQEAAAAAAAUBAAABABUEBAEDAD4EAAAAAAQAAgAuBAAAAE7xAQJpZAR0eXBlBG5hbWUBdwFoC2NsaXBDb250ZW50BWZpbGxzB3Zpc2libGUHb3BhY2l0eQlzY2FsZU1vZGUEa2luZAVjb2xvcgdzdHJva2VzBnNpemluZwZhbGlnbkgGYWxpZ25WEHBhcmFncmFwaFNwYWNpbmcPcGFyYWdyYXBoSW5kZW50CHRydW5jYXRlCG1heExpbmVzBGZvbnQFc3R5bGUEc2l6ZQpsaW5lSGVpZ2h0DWxldHRlclNwYWNpbmcEdW5pdAV2YWx1ZQpkZWNvcmF0aW9uBGNhc2UEdGV4dAlpbWFnZUhhc2gFbm9kZXMMAQICBAAGAgGAAoGARAEEDwMAAiwADQIAAQIAAwYQABgEAAwCAQwYAgEGBAIHOQAEAwYADAIDDAsRARAcCwEIEAsEBQcLDAAQEgsGMgEBCxoBAPwDAAAAAQUUMEA4MzI4MjM1OTAwODY0NDczODgFBHRleHQFCtCi0LXQutGB0YIEAAAAmQEBjAHyAAAAAAACBwEIBQcBCAQ/8BEA8QUJBQRmaWxsCgUFc29saWQLBQkjMAEA9QtmZgcABQlhdXRvV2lkdGgFBGxlZnQFA3RvcE0ABQkA8gUCBQVJbnRlcgUHUmVndWxhcgRAKF8A9QQFBGF1dG8IAhkFB3BlcmNlbnQaNgBiBQRub25lBgD/CwkCAQoU0J/RgNC40LLQtdGCLCDQvNC40YAFFgACYoQGFgRAQGMAjwAnAQEFFTM5EgEC8RVyZWN0BRrQn9GA0Y/QvNC+0YPQs9C+0LvRjNC90LjQugRAWQBHAAUJAA0iAQQbAQMqAf8JCwUJI2Q5ZDlkOWZmBwAHAQgGBwEeBUBhAQAsA14ABpkB0woFBWltYWdlCwUJI2YBAPYGAAIAZnIBnKKsvciUsscLYAACAHZ2EACQYgAAdgOGAwMAAAAAAD+a0kcBAAAABQAAAAwAC47IpIerERwAAAAAAQIAdnbuvBAsDAMAADQDAABMT1JPAAQiTRhgQILzAgAA8jAAAgEABW5vZGVzBBYKZGVjb3JhdGlvbgQEbm9uZQtjbGlwQ29udGVudAEAEHBhcmFncmFwaFNwYWNpbmcCAAACAGIEc2l6ZQIMAPIDKEAEdHlwZQQEdGV4dARjYXNlTADzEQZhbGlnbkgEBGxlZnQKbGluZUhlaWdodAQEYXV0bwF3PwAiAAApANNWBAN0b3ANbGV0dGVycAD0BgYCBHVuaXQEB3BlcmNlbnQFdmFsdXgA81wAAARuYW1lBBTQn9GA0LjQstC10YIsINC80LjRgAdzdHJva2VzBQAEZm9udAQFSW50ZXIEdGV4dAcBnKKsvciUsscLLgACaWQEFDBAODMyODIzNTkwMDg2NDQ3Mzg4CHRydW5jYXRlAQABaOoA8w8AAAVmaWxscwUBBgUHdmlzaWJsZQEBB29wYWNpdHnQAPUw8D8Jc2NhbGVNb2RlBARmaWxsBGtpbmQEBXNvbGlkBWNvbG9yBAkjMDAwMDAwZmYPcGFyYWdyYXBoSW5kZW50awD3WgZzaXppbmcECWF1dG9XaWR0aAVzdHlsZQQHUmVndWxhcgEIbWF4TGluZXMBHBGrh6TIjgsACgALABYABgAVAAcAEAAFAAEAFAATAA8AJAANAAwAEgAJAAgAEQAXAA4AAgAECQQAJwAAABECGggAAgPYAUVyZWN0rwGfWUACaWQEFTM5NAEABSkBJVlAKQEWBikB3wlpbWFnZUhhc2gEQGEBACwMZAENhQEDdQEBcwAFdQETZgEABx4CAUMC9g0a0J/RgNGP0LzQvtGD0LPQvtC70YzQvdC40LoAXQH0BC0AMAAsACgAKgAvACkAKwANAAIcAHYXAAAAAgMBWgI/AAEVlwIBFQpOAPEIAwQCCgAGBUoXGgQNAgoABgkADA0MBwFZAzMBAwBNAnJAQIQABwCDsAMmAwGNANMEAgIEAAMDAE4FAgQACwDwEAIEAAMCAAEMAQICBAAGAgGAAoGAAAAACgJJA7ADBAAAAAAAq4UQMwEAAAAFAAAADQAAHBGrh6TIjgsAAAAAAQcAgwVub2Rlc3P++tELAwAAAAAAAA==' | base64 -d > crates/ferrum-doc/tests/data/loro-compat/rust-page.snapshot`
Run: `mkdir -p crates/ferrum-doc/tests/data/loro-compat && printf %s 'bG9ybwAAAAAAAAAAAAAAAKDHxrgAA6kAAABMT1JPAAGJhbmLm8btr8MBAAACAHZ2AYmFuYubxu2vwwECAAwAw1+2MbFuQokAAAAAAAEAAQEQAYlCbrExtl/DAQEAAAAAAAUBAAABAAYBBAEDAAAGBW5vZGVzCQECAgEAAwEBgA4BBAIBAAIBAAIBEAIBAQAEAAAAAQAADAAdAAMAVyiBSwEAAAAFAAAAAgBmcgAMAMNftjGxbkKJAAAAAIIuhEKGAAAAfwAAAExPUk8AAAIBAAVub2RlcwQAAAAABwCDBW5vZGVzAwEAAYlCbrExtl/DBAICAQACAQAFAgEAAgEAAgEAAgEAAgEACQECAgEAAwEBgAAAAA4AAgBnLDhYAQAAAAUAAAANAACJQm6xMbZfwwAAAAAABwCDBW5vZGVzr7g90lYAAAAAAAAA' | base64 -d > crates/ferrum-doc/tests/data/loro-compat/rust.snapshot`
Run: `mkdir -p crates/ferrum-doc/tests/data/loro-compat && printf %s 'bG9ybwAAAAAAAAAAAAAAAKLeSZMABEIBAQEBAREBiUJusTG2X8MAAQEAAAAAAAUBAAABAAYBBAAAAAACAXgADgEEAgEAAgEAAgELAgEBAAkEQEUAAAAAAAA=' | base64 -d > crates/ferrum-doc/tests/data/loro-compat/rust.update`
Expected: зависимости установлены по `apps/server/bun.lock`, новая — `loro-crdt` 1.16.2; двоичные данные тестов совместимости записаны

- [ ] **Step 5: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test -p ferrum-doc --test loro_compat`

Expected: PASS: всё без замечаний; `bun test` 23 и 365 pass (паники Loro в выводе теста проверки пачек ожидаемы); `loro_compat` проходит

- [ ] **Step 6: Закоммитить**

```bash
git add apps/server/bun.lock apps/server/migrations/004_sync.sql apps/server/package.json apps/server/src/sync/container-read.ts apps/server/src/sync/update-log.ts apps/server/src/sync/validator.ts apps/server/src/sync/validator.worker.ts apps/server/test/loro-compat.test.ts apps/server/test/migrate.test.ts apps/server/test/update-log.test.ts apps/server/test/validator.test.ts crates/ferrum-doc/tests/loro_compat.rs apps/server/test/data/loro-base.snap apps/server/test/data/loro-good.update apps/server/test/data/loro-panic-any.update apps/server/test/data/loro-panic-real.update crates/ferrum-doc/tests/data/loro-compat/js-page.snapshot crates/ferrum-doc/tests/data/loro-compat/js.snapshot crates/ferrum-doc/tests/data/loro-compat/rust-page.snapshot crates/ferrum-doc/tests/data/loro-compat/rust.snapshot crates/ferrum-doc/tests/data/loro-compat/rust.update
git commit -F - <<'EOF'
сервер: журнал обновлений и проверка пачек

`doc_state`, `doc_update`, `doc_peer` (миграция 004). `UpdateLog`: группы каждые 20 мс одной транзакцией, порядок и
непрерывность держит строка документа под блокировкой, поэтому повторы и порядок верны и у двух писателей на одной
базе. Роль проверяется в транзакции записи. Первый писатель пира его занимает: подделка чужого пира заняла бы чужие
счётчики. Журнал файла не растёт сверх 1 ГБ (`LOG_FULL`). Сбой базы — отказ «повторите», а не потеря.

`Validator`: пачка применяется к настоящему документу в отдельном Bun Worker раньше журнала. Расхождение со спекой
§7.3 («сервер не держит документы на горячем пути»): проба показала, что 2 % порченых пачек, прошедших разбор и
пустой документ, роняют Loro на настоящем, и журнал отравил бы файл всем. Документы загружаются по первой пачке,
вытесняются по простою и по бюджету 512 МБ. Паника wasm — отказ этой пачке и новый worker. Снимок вместо обновления не
принимается. `loro-crdt` 1.16.2 добавлен, тест совместимости с ядром на Rust в обе стороны.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 6: Сервер — WebSocket файла

Спека §7.2, §6.3 (B-237, B-296, B-245, B-207). Открытие с проверкой `Origin`, сессии и роли, догоняние из базы, снимок отставшему, пределы, медленный читатель, закрытие при смене прав из конвейера API.

**Files:**
- Create: `apps/server/src/sync/hub.ts`
- Modify: `apps/server/src/api.ts`, `apps/server/src/app.ts`, `apps/server/src/auth-routes.ts`, `apps/server/src/file-routes.ts`, `apps/server/src/index.ts`, `apps/server/src/sync/validator.ts`
- Test: `apps/server/test/access.test.ts`, `apps/server/test/api-stand.ts`, `apps/server/test/connection-drop-probe.ts`, `apps/server/test/db.test.ts`, `apps/server/test/hub-load.test.ts` (новый), `apps/server/test/hub.test.ts` (новый), `apps/server/test/sync-client.ts` (новый), `apps/server/test/sync-load-probe.ts` (новый)

**Interfaces:**
- Consumes: `UpdateLog`, `Validator`, `apiRoutes`, `authorizeUpgrade`.
- Produces: `SyncHub`, `WS /api/files/:id/sync`, `GET /api/files/:id/docs/:doc/snapshot`, `afterWrite` в конвейере API.

- [ ] **Step 1: Написать тесты**

В `apps/server/test/access.test.ts` заменить:

````ts
  'GET /api/files/:id/content': { params: file, allowed: viewer, status: 204 },
  'PUT /api/files/:id/content': { params: file, body: container(), headers: {}, allowed: editor, denied: [viewer] },
  'GET /api/files/:id/thumb': { params: file, allowed: viewer, status: 204 },
  'PUT /api/files/:id/thumb': { params: file, body: PNG, allowed: editor, denied: [viewer] },
````

на:

````ts
  'GET /api/files/:id/content': { params: file, allowed: viewer, status: 204 },
  'PUT /api/files/:id/content': { params: file, body: container(), headers: {}, allowed: editor, denied: [viewer] },
  // Документа нет в файле: 404 и у читателя, но это уже после проверки доступа.
  'GET /api/files/:id/docs/:doc/snapshot': { params: (w) => ({ id: w.file, doc: 'root' }), allowed: viewer, status: 404 },
  'GET /api/files/:id/thumb': { params: file, allowed: viewer, status: 204 },
  'PUT /api/files/:id/thumb': { params: file, body: PNG, allowed: editor, denied: [viewer] },
````

В `apps/server/test/access.test.ts` заменить:

````ts

function pathOf(path: string, params: Record<string, string>): string {
  return path.replace(/:(\w+)/g, (_, name: string) => params[name] ?? (name === 'hash' ? HASH : NOBODY));
}

````

на:

````ts

function pathOf(path: string, params: Record<string, string>): string {
  return path.replace(/:(\w+)/g, (_, name: string) => params[name] ?? (name === 'hash' ? HASH : name === 'doc' ? 'root' : NOBODY));
}

````

В `apps/server/test/api-stand.ts` заменить:

````ts
  const reset = () =>
    pool.query(
      'TRUNCATE file, file_access, file_invite, share_link, file_link_user, file_recent, file_image, image, blob_trash, blob_pending, team, team_member, team_invite, project CASCADE',
    );

  const stop = async () => {
    server.stop(true);
    valkey.close();
````

на:

````ts
  const reset = () =>
    pool.query(
      'TRUNCATE doc_update, doc_state, doc_peer, file, file_access, file_invite, share_link, file_link_user, file_recent, file_image, image, blob_trash, blob_pending, team, team_member, team_invite, project CASCADE',
    );

  const stop = async () => {
    await server.hub.stop();
    server.stop(true);
    valkey.close();
````

В `apps/server/test/api-stand.ts` заменить:

````ts
    dataDir,
    server,
    call,
    signIn,
````

на:

````ts
    dataDir,
    server,
    hub: server.hub,
    call,
    signIn,
````

В `apps/server/test/connection-drop-probe.ts` заменить:

````ts
import { createPool } from '../src/db';
import { claimInvites } from '../src/invites';
import { apiStand, container } from './api-stand';
import { TEST_DATABASE_URL, TEST_VALKEY_URL } from './helpers';

````

на:

````ts
import { createPool } from '../src/db';
import { claimInvites } from '../src/invites';
import { LoroDoc } from 'loro-crdt';
import { apiStand, container } from './api-stand';
import { Client } from './sync-client';
import { TEST_DATABASE_URL, TEST_VALKEY_URL } from './helpers';

````

В `apps/server/test/connection-drop-probe.ts` заменить:

````ts
  }

  console.log(JSON.stringify(out));
  await admin.end();
````

на:

````ts
  }

  // Запись журнала: соединение группы обрывается посреди транзакции. Пачка получает отказ «повторите», процесс живёт.
  const synced = await s.newFile(who);
  const client = await Client.open(s, who, synced.id);
  if (!client) throw new Error('соединение не открылось');
  client.hello();
  client.subscribe('root');
  await client.wait('subscribed');
  const doc = new LoroDoc();
  doc.getMap('m').set('a', 1);
  doc.commit();
  const bytes = doc.export({ mode: 'update' });
  const docLock = await admin.connect();
  await docLock.query('BEGIN');
  await docLock.query('SELECT 1 FROM file WHERE id = $1 FOR UPDATE', [synced.id]);
  client.update('root', 1, bytes);
  await dropWaiter();
  await docLock.query('ROLLBACK');
  docLock.release();
  const refused = await client.wait('refused');
  out.journal = refused.code;
  client.frames.length = 0;
  client.update('root', 2, bytes);
  out.journalNext = (await client.wait('ack')).seq;
  client.close();
  out.afterJournal = await healthy();

  console.log(JSON.stringify(out));
  await admin.end();
````

В `apps/server/test/db.test.ts` заменить:

````ts
    next: 200,
    afterValkey: 200,
  });
}, 60_000);
````

на:

````ts
    next: 200,
    afterValkey: 200,
    journal: 'UNAVAILABLE',
    journalNext: 1,
    afterJournal: 200,
  });
}, 60_000);
````

Создать `apps/server/test/hub-load.test.ts`:

````ts
// Нагрузка отдельным процессом: память на соединение, задержка на 50 участниках, медленный читатель.

import { expect, test } from 'bun:test';

async function run(...args: string[]): Promise<Record<string, number | boolean>> {
  const p = Bun.spawn(['bun', new URL('./sync-load-probe.ts', import.meta.url).pathname, ...args], { stdout: 'pipe', stderr: 'pipe' });
  const [text, err] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  await p.exited;
  const line = text.trim().split('\n').at(-1) ?? '';
  try {
    return JSON.parse(line) as Record<string, number | boolean>;
  } catch {
    throw new Error(`проба упала: ${text}\n${err.slice(-2000)}`);
  }
}

test('память: 1000 соединений с подпиской — меньше 100 КБ на соединение', async () => {
  const r = await run('memory', '1000');
  console.log('память', r);
  expect(r.connections).toBe(1000);
  expect(r.perConnectionKb as number).toBeLessThan(100);
}, 120_000);

test('от кадра до рассылки, 50 участников (цель 40 мс, p95)', async () => {
  const r = await run('latency', '50');
  console.log('задержка', r);
  // Клиенты в том же процессе, что и сервер, и опрашивают кадры: порог с запасом, цифра в выводе.
  expect(r.p95 as number).toBeLessThan(60);
}, 120_000);

test('медленный читатель: буфер сокета ограничен, потом догоняет всё по порядку', async () => {
  const r = await run('slow', '150');
  console.log('медленный читатель', r);
  expect(r.frames).toBe(150);
  expect(r.ordered).toBe(true);
  // В буфере сокета — предел и один кадр, остальное ждёт в базе: 150 МБ без предела легли бы в память.
  expect(r.peakBufferedMb as number).toBeLessThan(16);
}, 180_000);
````

Создать `apps/server/test/hub.test.ts`:

````ts
// WebSocket файла против настоящих Postgres и Valkey (спека подсистемы 4, разделы 7 и 13.2): открытие, кадры, правки двух
// людей, чужие и испорченные пачки, снятие роли и выход закрывают соединение сразу, пределы не трогают настоящую работу,
// а отказ по пределу — пауза, сбой базы не теряет правок и не роняет процесс.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { encodeClientFrame, encodeVersion, SYNC_CLOSE, type VersionMap } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import { SyncHub } from '../src/sync/hub';
import { apiStand, type Person } from './api-stand';
import { Client, peers, realContainer } from './sync-client';

const s = await apiStand({
  syncLimits: { recheckMs: 3_600_000, sessionCheckMs: 3_600_000, helloMs: 300, framesPerSecond: 120, framesBurst: 2000 },
});
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const editor = await s.signIn('editor@example.com');
const viewer = await s.signIn('viewer@example.com');
const stranger = await s.signIn('stranger@example.com');

const data = (name: string) => Bun.file(new URL(`./data/${name}`, import.meta.url)).bytes();

let file = '';
let root: LoroDoc;
const open: Client[] = [];

async function connect(who: Person, f = file): Promise<Client> {
  const c = await Client.open(s, who, f);
  if (!c) throw new Error('соединение не открылось');
  open.push(c);
  c.hello();
  await c.wait('role');
  return c;
}

async function joined(who: Person, doc = 'root', version: VersionMap = peers(root)): Promise<Client> {
  const c = await connect(who);
  c.subscribe(doc, version);
  await c.wait('subscribed', (f) => f.doc === doc);
  return c;
}

/** Своя вкладка: у каждого человека — свой пир. */
function tab(): LoroDoc {
  const d = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  return d;
}

function edit(d: LoroDoc, value: number): Uint8Array {
  const from = d.oplogVersion();
  d.getMap('meta').set('x', value);
  d.commit();
  return d.export({ mode: 'update', from });
}

beforeEach(async () => {
  for (const c of open.splice(0)) c.close();
  await s.reset();
  file = (await s.newFile(owner)).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
    file,
    editor.id,
    viewer.id,
  ]);
  root = new LoroDoc();
  root.getMap('meta').set('name', 'файл');
  root.commit();
  const res = await s.writeContent(owner, file, 0, realContainer(root.export({ mode: 'snapshot' })));
  expect(res.status).toBe(200);
});

test('открытие: чужой Origin, без сессии, чужой файл, вкладка за другого человека', async () => {
  expect(await Client.open(s, owner, file, { origin: 'https://evil.example' })).toBeNull();
  expect(await Client.open(s, owner, file, { cookie: '' })).toBeNull();
  expect(await Client.open(s, stranger, file)).toBeNull();
  expect(await Client.open(s, owner, file, { user: editor.id })).toBeNull();
  expect(await Client.open(s, owner, '00000000-0000-4000-8000-000000000000')).toBeNull();
  const ok = await Client.open(s, owner, file);
  expect(ok).not.toBeNull();
  ok?.close();
  // Ответ на открытие без WebSocket — обычный отказ.
  const plain = await fetch(`${s.publicUrl}/api/files/${file}/sync?user=${owner.id}`, {
    headers: { cookie: owner.cookie, origin: s.publicUrl },
  });
  expect(plain.status).toBe(400);
});

test('кадры: первым hello, мусор и текст закрывают, старый протокол просит обновиться', async () => {
  const silent = (await Client.open(s, owner, file))!;
  expect((await silent.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);

  const first = (await Client.open(s, owner, file))!;
  first.subscribe('root');
  expect((await first.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);

  const garbage = await connect(owner);
  garbage.send(new Uint8Array([200, 1, 2]));
  expect((await garbage.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);

  const text = await connect(owner);
  text.ws.send('привет');
  expect((await text.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);

  const old = (await Client.open(s, owner, file))!;
  old.hello(999);
  await old.wait('please-reload');
  expect((await old.waitClosed()).code).toBe(SYNC_CLOSE.RELOAD);

  const huge = await connect(owner);
  huge.send(new Uint8Array(17 * 1024 * 1024));
  // Сверх `maxPayloadLength` сокет рвётся без чтения кадра: 1009 или обрыв, клиенту всё равно.
  expect([SYNC_CLOSE.TOO_BIG, 1006]).toContain((await huge.waitClosed()).code as 1009);
  // Процесс жив.
  expect((await s.call(owner, 'GET', `/api/files/${file}`)).status).toBe(200);
});

test('правка доходит до другого, отправителю только ack, журнал записан раньше рассылки', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const v = await joined(viewer);
  const mine = tab();
  a.update('root', 7, edit(mine, 1));
  const ack = await a.wait('ack', (f) => f.clientSeq === 7);
  const got = await b.wait('updates');
  expect(got.seq).toBe(ack.seq);
  await v.wait('updates');
  expect(a.frames.filter((f) => f.t === 'updates')).toEqual([]);
  const { rows } = await s.pool.query('SELECT seq FROM doc_update WHERE file_id = $1', [file]);
  expect(rows).toHaveLength(1);
  const theirs = tab();
  theirs.import(got.bytes);
  expect(theirs.getMap('meta').get('x')).toBe(1);
});

test('порядок: подписчик получает записи по порядку seq при правках двух людей', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const c = await joined(viewer);
  const [da, db] = [tab(), tab()];
  for (let i = 0; i < 40; i++) {
    a.update('root', i, edit(da, i));
    b.update('root', i, edit(db, -i));
  }
  await a.wait('ack', (f) => f.clientSeq === 39);
  await b.wait('ack', (f) => f.clientSeq === 39);
  await c.wait('updates', (f) => f.seq === 80);
  const seqs = c.frames.flatMap((f) => (f.t === 'updates' ? [f.seq] : []));
  expect(seqs).toEqual(Array.from({ length: 80 }, (_, i) => i + 1));
});

test('читатель не пишет: отказ, журнал пуст', async () => {
  const v = await joined(viewer);
  v.update('root', 1, edit(tab(), 1));
  const r = await v.wait('refused');
  expect(r.code).toBe('FORBIDDEN');
  expect((await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file])).rows).toHaveLength(0);
});

test('испорченная пачка не попадает в журнал и другим, другие работают', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const bad = edit(tab(), 1).slice();
  bad[bad.length - 2] ^= 0xff;
  a.update('root', 1, bad);
  expect((await a.wait('refused')).code).toBe('BAD_UPDATE');
  // Снимок вместо обновления — тоже.
  a.update('root', 2, root.export({ mode: 'snapshot' }));
  await a.wait('refused', (f) => f.clientSeq === 2 && f.code === 'BAD_UPDATE');
  const logged = await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file]);
  expect(logged.rows).toHaveLength(0);
  expect(b.frames.filter((f) => f.t === 'updates')).toEqual([]);
  a.update('root', 3, edit(tab(), 2));
  await a.wait('ack', (f) => f.clientSeq === 3);
  await b.wait('updates');
});

test('паника Loro на пачке: отказ, файл цел, остальные пишут дальше', async () => {
  const base = await data('loro-base.snap');
  const panic = await data('loro-panic-real.update');
  const f = (await s.newFile(owner)).id;
  expect((await s.writeContent(owner, f, 0, realContainer(base))).status).toBe(200);
  const version = new Map([...LoroDoc.fromSnapshot(base).oplogVersion().toJSON()].map(([p, c]) => [String(p), c]));
  const c = await connect(owner, f);
  c.subscribe('root', version);
  await c.wait('subscribed');
  const saved = console.error;
  console.error = () => {};
  c.update('root', 1, panic);
  expect((await c.wait('refused', (x) => x.clientSeq === 1)).code).toBe('BAD_UPDATE');
  console.error = saved;
  const good = await data('loro-good.update');
  c.update('root', 2, good);
  await c.wait('ack', (x) => x.clientSeq === 2);
  expect((await s.pool.query('SELECT seq FROM doc_update WHERE file_id = $1', [f])).rows).toHaveLength(1);
});

test('чужой пир не проходит: подделка чужих правок', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const d = tab();
  a.update('root', 1, edit(d, 1));
  await a.wait('ack');
  // Тот же документ — тот же пир: второй человек шлёт следующую правку от него.
  b.update('root', 1, edit(d, 2));
  expect((await b.wait('refused')).code).toBe('FOREIGN_PEER');
});

test('разрыв версий: GAP, клиент подписывается заново и шлёт от версии сервера', async () => {
  const a = await joined(owner);
  const d = tab();
  const first = edit(d, 1);
  const second = edit(d, 2);
  a.update('root', 2, second);
  expect((await a.wait('refused')).code).toBe('GAP');
  a.frames.length = 0;
  a.subscribe('root', peers(d));
  const sub = await a.wait('subscribed');
  expect(sub.seq).toBe(0);
  a.update('root', 3, first);
  a.update('root', 4, second);
  await a.wait('ack', (f) => f.clientSeq === 4);
});

test('пачка, зависящая от неизвестного серверу чужого пира: вектор по её пирам в порядке, а применить нельзя — GAP', async () => {
  const a = await joined(owner);
  const other = tab();
  edit(other, 1);
  const mine = tab();
  mine.import(other.export({ mode: 'update', from: root.oplogVersion() }));
  const bytes = edit(mine, 2);
  a.update('root', 1, bytes);
  expect((await a.wait('refused')).code).toBe('GAP');
  expect((await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file])).rows).toHaveLength(0);
});

test('повтор пачки после обрыва безвреден: ack без новой записи', async () => {
  const a = await joined(owner);
  const bytes = edit(tab(), 1);
  a.update('root', 1, bytes);
  await a.wait('ack', (f) => f.clientSeq === 1);
  a.update('root', 2, bytes);
  const ack = await a.wait('ack', (f) => f.clientSeq === 2);
  expect(ack.seq).toBe(1);
  expect((await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file])).rows).toHaveLength(1);
});

test('догоняние: только то, чего нет у клиента, и версия сервера', async () => {
  const a = await joined(owner);
  const d = tab();
  for (let i = 0; i < 5; i++) a.update('root', i, edit(d, i));
  await a.wait('ack', (f) => f.clientSeq === 4);
  const behind = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  behind.import(d.export({ mode: 'update', from: root.oplogVersion() }));
  const c = await connect(editor);
  c.subscribe('root', peers(root));
  const sub = await c.wait('subscribed');
  expect(sub.seq).toBe(5);
  expect(c.frames.filter((f) => f.t === 'updates')).toHaveLength(5);
  // Уже всё знает — ни одной записи.
  const c2 = await connect(editor);
  c2.subscribe('root', peers(behind));
  await c2.wait('subscribed');
  expect(c2.frames.filter((f) => f.t === 'updates')).toEqual([]);
});

test('отстал сильнее журнала: snapshot-needed и снимок документа', async () => {
  const c = await connect(editor);
  await s.pool.query(`UPDATE doc_state SET base_vv = '{"123":5}' WHERE file_id = $1`, [file]);
  await s.pool.query(
    `INSERT INTO doc_state (file_id, doc, base_vv, vv) VALUES ($1, 'root', '{"123":5}', '{"123":5}')
     ON CONFLICT (file_id, doc) DO UPDATE SET base_vv = excluded.base_vv`,
    [file],
  );
  c.subscribe('root', peers(root));
  const need = await c.wait('snapshot-needed');
  const res = await s.call(editor, 'GET', need.url);
  expect(res.status).toBe(200);
  const snapshot = LoroDoc.fromSnapshot(new Uint8Array(await res.arrayBuffer()));
  expect(snapshot.getMap('meta').get('name')).toBe('файл');
  // Чужой — нет, как у любого маршрута файла.
  expect((await s.call(stranger, 'GET', need.url)).status).toBe(404);
});

test('снятие роли закрывает соединение сразу', async () => {
  const c = await joined(editor);
  const { rows } = await s.pool.query<{ id: string }>('SELECT id FROM file_access WHERE file_id = $1 AND user_id = $2', [file, editor.id]);
  const res = await s.call(owner, 'DELETE', `/api/files/${file}/share/people/${rows[0].id}`);
  expect(res.status).toBe(200);
  expect(await c.wait('access-revoked')).toEqual({ t: 'access-revoked', reason: 'access' });
  expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.REVOKED);
});

test('понижение до читателя: сообщение сразу, запись отказывает', async () => {
  const c = await joined(editor);
  const { rows } = await s.pool.query<{ id: string }>('SELECT id FROM file_access WHERE file_id = $1 AND user_id = $2', [file, editor.id]);
  const res = await s.call(owner, 'PATCH', `/api/files/${file}/share/people/${rows[0].id}`, { role: 'viewer' });
  expect(res.status).toBe(200);
  const r = await c.wait('role', (f) => f.role === 'viewer');
  expect(r.canShare).toBe(false);
  c.update('root', 1, edit(tab(), 1));
  expect((await c.wait('refused')).code).toBe('FORBIDDEN');
  expect(c.closed).toBeNull();
});

test('запись проверяет роль в транзакции: снятая мимо сервера не пишет, даже пока соединение думает иначе', async () => {
  const c = await joined(editor);
  await s.pool.query(`UPDATE file_access SET role = 'viewer' WHERE file_id = $1 AND user_id = $2`, [file, editor.id]);
  c.update('root', 1, edit(tab(), 1));
  expect((await c.wait('refused')).code).toBe('FORBIDDEN');
  expect((await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file])).rows).toHaveLength(0);
});

test('ссылка: выключение и новая ссылка закрывают пришедших по ней', async () => {
  const link = await s.call(owner, 'PUT', `/api/files/${file}/share/link`, { role: 'editor', enabled: true });
  expect(link.status).toBe(200);
  await s.pool.query('INSERT INTO file_link_user (file_id, user_id) VALUES ($1, $2)', [file, stranger.id]);
  const c = await joined(stranger);
  const off = await s.call(owner, 'PUT', `/api/files/${file}/share/link`, { role: 'editor', enabled: false });
  expect(off.status).toBe(200);
  await c.wait('access-revoked');
  expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.REVOKED);
});

test('корзина закрывает всех, кроме убравшего', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  expect((await s.call(owner, 'POST', `/api/files/${file}/trash`)).status).toBe(200);
  await b.wait('access-revoked');
  expect((await b.waitClosed()).code).toBe(SYNC_CLOSE.REVOKED);
  expect(a.closed).toBeNull();
});

test('выход закрывает соединения этой сессии, выход на всех — все соединения человека', async () => {
  const other = await s.signIn('multi@example.com');
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor')`, [file, other.id]);
  const c = await joined(other);
  const res = await s.call(other, 'POST', '/api/auth/sign-out', {});
  expect(res.status).toBe(200);
  expect(await c.wait('access-revoked')).toEqual({ t: 'access-revoked', reason: 'session' });
  expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.SESSION);

  const again = await s.signIn('multi@example.com');
  const c1 = await joined(again);
  const c2 = await joined(again);
  expect((await s.call(again, 'POST', '/api/auth/sign-out-everywhere', {})).status).toBe(200);
  expect((await c1.waitClosed()).code).toBe(SYNC_CLOSE.SESSION);
  expect((await c2.waitClosed()).code).toBe(SYNC_CLOSE.SESSION);
});

test('снятие команды закрывает соединения участника', async () => {
  const team = (await (await s.call(owner, 'POST', '/api/teams', { name: 'Т' })).json()) as { id: string };
  const project = (await (await s.call(owner, 'POST', `/api/teams/${team.id}/projects`, { name: 'П' })).json()) as { id: string };
  const f = await s.newFile(owner, 'В проекте', project.id);
  const member = await s.signIn('member@example.com');
  await s.pool.query(`INSERT INTO team_member (team_id, user_id, role) VALUES ($1, $2, 'member')`, [team.id, member.id]);
  const c = await Client.open(s, member, f.id);
  expect(c).not.toBeNull();
  open.push(c!);
  c!.hello();
  await c!.wait('role');
  const { rows } = await s.pool.query<{ id: string }>('SELECT id FROM team_member WHERE team_id = $1 AND user_id = $2', [
    team.id,
    member.id,
  ]);
  expect((await s.call(owner, 'DELETE', `/api/teams/${team.id}/members/${rows[0].id}`)).status).toBe(200);
  await c!.wait('access-revoked');
});

test('настоящая работа не упирается в пределы: перетаскивание, набор и большая вставка', async () => {
  const a = await joined(owner);
  const d = tab();
  // Перетаскивание: 30 пачек в секунду на документ две секунды, разом корень и страница по очереди.
  const start = performance.now();
  for (let i = 0; i < 60; i++) {
    a.update('root', i, edit(d, i));
    while (performance.now() - start < (i + 1) * (1000 / 30)) await Bun.sleep(2);
  }
  await a.wait('ack', (f) => f.clientSeq === 59);
  // Набор и вставка: 300 пачек разом, 8 МБ в сумме.
  const blob = 'ж'.repeat(16 * 1024);
  for (let i = 0; i < 300; i++) {
    const from = d.oplogVersion();
    d.getMap('meta').set('t', `${i}${blob}`);
    d.commit();
    a.update('root', 1000 + i, d.export({ mode: 'update', from }));
  }
  await a.wait('ack', (f) => f.clientSeq === 1299);
  expect(a.frames.filter((f) => f.t === 'pause')).toEqual([]);
  expect(a.frames.filter((f) => f.t === 'refused')).toEqual([]);
  // И большая вставка кадрами почти по пределу.
  const big = 'ё'.repeat(5 * 1024 * 1024);
  for (let i = 0; i < 4; i++) {
    const from = d.oplogVersion();
    d.getMap('meta').set('big', `${i}${big}`);
    d.commit();
    a.update('root', 2000 + i, d.export({ mode: 'update', from }));
  }
  await a.wait('ack', (f) => f.clientSeq === 2003, 25_000);
  expect(a.frames.filter((f) => f.t === 'pause')).toEqual([]);
});

test('предел частоты — пауза с понятным сроком: кадр не принят, соединение живо, повтор проходит', async () => {
  const limited = await apiStand(
    { syncLimits: { framesPerSecond: 5, framesBurst: 8, recheckMs: 3_600_000, sessionCheckMs: 3_600_000 } },
    true,
  );
  try {
    const who = await limited.signIn('limited@example.com');
    const f = (await limited.newFile(who)).id;
    const c = (await Client.open(limited, who, f))!;
    c.hello();
    c.subscribe('root');
    await c.wait('subscribed');
    const d = new LoroDoc();
    const bytes: Uint8Array[] = [];
    for (let i = 0; i < 40; i++) bytes.push(edit(d, i));
    bytes.forEach((b, i) => c.update('root', i, b));
    const pause = await c.wait('pause');
    expect(pause.ms).toBeGreaterThan(0);
    expect(c.closed).toBeNull();
    // Клиент ждёт паузу и шлёт снова с версии сервера.
    c.frames.length = 0;
    await Bun.sleep(1500);
    c.subscribe('root', new Map());
    const sub = await c.wait('subscribed');
    const known = sub.seq;
    await Bun.sleep(1000);
    c.update('root', 100, d.export({ mode: 'update', from: new LoroDoc().oplogVersion() }));
    await c.wait('ack', (x) => x.clientSeq === 100);
    expect(known).toBeLessThan(40);
    c.close();
  } finally {
    await limited.stop();
  }
});

test('сбой базы при записи: отказ «повторите», рассылки нет, процесс жив, повтор записывается', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const bytes = edit(tab(), 1);
  await s.failing('doc_update', 'INSERT', async () => {
    a.update('root', 1, bytes);
    expect((await a.wait('refused')).code).toBe('UNAVAILABLE');
  });
  expect(b.frames.filter((f) => f.t === 'updates')).toEqual([]);
  a.frames.length = 0;
  a.update('root', 2, bytes);
  await a.wait('ack', (f) => f.clientSeq === 2);
  await b.wait('updates');
});

test('Valkey не отвечает при открытии: 503, а не падение; проверка сессий людей не выгоняет', async () => {
  const hub = new SyncHub(
    s.pool,
    s.blobs,
    {
      api: {
        getSession: () => Promise.reject(new Error('Valkey недоступен')),
      },
    } as never,
    s.publicUrl,
    { sessionCheckMs: 10 },
  );
  const refused = await hub.upgrade(new Request(s.publicUrl, { headers: { origin: s.publicUrl } }), s.server, file);
  expect(refused?.status).toBe(503);
  await hub.stop();
  // Живой сервер: соединение держится, пока Valkey недоступен (`checkSessions` под `catch`).
  expect((await s.call(owner, 'GET', `/api/files/${file}`)).status).toBe(200);
});

test('сессия снята мимо сервера: периодическая проверка закрывает соединение', async () => {
  const tight = await apiStand({ syncLimits: { sessionCheckMs: 50 } }, true);
  try {
    const who = await tight.signIn('expired@example.com');
    const f = (await tight.newFile(who)).id;
    const c = (await Client.open(tight, who, f))!;
    c.hello();
    await c.wait('role');
    const { rows } = await tight.pool.query<{ id: string }>('SELECT id FROM "user" WHERE email = $1', ['expired@example.com']);
    expect(rows[0].id).toBe(who.id);
    const list = await tight.valkey.get(`active-sessions-${who.id}`);
    for (const x of JSON.parse(list!) as { token: string }[]) await tight.valkey.del(x.token);
    await tight.valkey.del(`active-sessions-${who.id}`);
    expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.SESSION);
  } finally {
    await tight.stop();
  }
});

test('предел соединений и документов на соединение', async () => {
  const tight = await apiStand({ syncLimits: { connectionsPerUser: 2, subscribesPerConnection: 2 } }, true);
  try {
    const who = await tight.signIn('tight@example.com');
    const f = (await tight.newFile(who)).id;
    const [c1, c2] = [(await Client.open(tight, who, f))!, (await Client.open(tight, who, f))!];
    expect(await Client.open(tight, who, f)).toBeNull();
    c1.hello();
    c1.send(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) }));
    c1.subscribe('1@1');
    c1.subscribe('2@1');
    expect((await c1.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);
    c2.close();
    await c2.waitClosed();
    // Место освободилось.
    const c3 = await Client.open(tight, who, f);
    expect(c3).not.toBeNull();
    c3?.close();
  } finally {
    await tight.stop();
  }
});

test('документы без снимка: читатель не заводит строки, первая запись заводит', async () => {
  const v = await connect(viewer);
  v.subscribe('5@5');
  const sub = await v.wait('subscribed');
  expect(sub.seq).toBe(0);
  expect((await s.pool.query(`SELECT 1 FROM doc_state WHERE file_id = $1 AND doc = '5@5'`, [file])).rows).toHaveLength(0);
  const a = await connect(owner);
  a.subscribe('5@5');
  await a.wait('subscribed');
  a.update('5@5', 1, edit(new LoroDoc(), 1));
  await a.wait('ack');
  expect((await s.pool.query(`SELECT 1 FROM doc_state WHERE file_id = $1 AND doc = '5@5'`, [file])).rows).toHaveLength(1);
  await v.wait('updates');
});

test('пока идёт журнал, запись файла целиком отказывает: она затёрла бы правки', async () => {
  const a = await joined(owner);
  a.update('root', 1, edit(tab(), 1));
  await a.wait('ack');
  const res = await s.writeContent(owner, file, 1, realContainer(root.export({ mode: 'snapshot' })));
  expect(res.status).toBe(409);
  expect((await s.pool.query<{ version: string }>('SELECT version FROM file WHERE id = $1', [file])).rows[0].version).toBe('1');
});

test('schema из hello записывается в файл', async () => {
  const c = (await Client.open(s, owner, file))!;
  open.push(c);
  c.send(encodeClientFrame({ t: 'hello', protocol: 1, schema: 7 }));
  await c.wait('role');
  const { rows } = await s.pool.query<{ schema_version: number }>('SELECT schema_version FROM file WHERE id = $1', [file]);
  expect(rows[0].schema_version).toBe(7);
});
````

Создать `apps/server/test/sync-client.ts`:

````ts
// Клиент WebSocket для тестов: настоящий сокет и настоящие кадры, настоящие документы Loro.

import {
  encodeClientFrame,
  encodeVersion,
  parseServerFrame,
  SYNC_PROTOCOL,
  syncPath,
  type ServerFrame,
  type VersionMap,
} from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import type { ApiStand, Person } from './api-stand';

export function peers(doc: LoroDoc): VersionMap {
  return new Map([...doc.oplogVersion().toJSON()].map(([p, c]) => [String(p), c]));
}

export class Client {
  readonly frames: ServerFrame[] = [];
  closed: { code: number; reason: string } | null = null;
  private waiters: (() => void)[] = [];
  private constructor(readonly ws: WebSocket) {
    ws.binaryType = 'arraybuffer';
    ws.onmessage = (e) => {
      const f = parseServerFrame(new Uint8Array(e.data as ArrayBuffer));
      if (!f) throw new Error('неверный кадр сервера');
      this.frames.push(f);
      this.wake();
    };
    ws.onclose = (e) => {
      this.closed = { code: e.code, reason: e.reason };
      this.wake();
    };
  }

  private wake(): void {
    for (const w of this.waiters.splice(0)) w();
  }

  /** Подключение; отказ при открытии — `null`. */
  static async open(
    s: ApiStand,
    who: Person,
    file: string,
    over: { origin?: string; user?: string; cookie?: string } = {},
  ): Promise<Client | null> {
    const url = s.publicUrl.replace('http', 'ws') + syncPath(file, over.user ?? who.id);
    const ws = new WebSocket(url, { headers: { origin: over.origin ?? s.publicUrl, cookie: over.cookie ?? who.cookie } } as never);
    return new Promise((resolve) => {
      ws.onopen = () => resolve(new Client(ws));
      ws.onerror = () => resolve(null);
    });
  }

  send(bytes: Uint8Array): void {
    this.ws.send(bytes);
  }

  hello(protocol = SYNC_PROTOCOL): void {
    this.send(encodeClientFrame({ t: 'hello', protocol, schema: 1 }));
  }

  subscribe(doc: string, version: VersionMap = new Map()): void {
    this.send(encodeClientFrame({ t: 'subscribe', doc, version: encodeVersion(version) }));
  }

  update(doc: string, clientSeq: number, bytes: Uint8Array): void {
    this.send(encodeClientFrame({ t: 'update', doc, clientSeq, bytes }));
  }

  /** Кадр, подходящий условию; ждать — до срока теста. */
  async wait<T extends ServerFrame['t']>(
    t: T,
    match: (f: Extract<ServerFrame, { t: T }>) => boolean = () => true,
    ms = 10_000,
  ): Promise<Extract<ServerFrame, { t: T }>> {
    const deadline = performance.now() + ms;
    for (;;) {
      const f = this.frames.find((x) => x.t === t && match(x as Extract<ServerFrame, { t: T }>));
      if (f) return f as Extract<ServerFrame, { t: T }>;
      if (performance.now() > deadline)
        throw new Error(`нет кадра ${t}: ${JSON.stringify(this.frames.map((x) => x.t))}, закрыт: ${JSON.stringify(this.closed)}`);
      await this.tick(deadline);
    }
  }

  async waitClosed(ms = 10_000): Promise<{ code: number; reason: string }> {
    const deadline = performance.now() + ms;
    while (!this.closed) {
      if (performance.now() > deadline) throw new Error('соединение не закрыто');
      await this.tick(deadline);
    }
    return this.closed;
  }

  private tick(deadline: number): Promise<void> {
    return new Promise((resolve) => {
      const t = setTimeout(resolve, Math.max(1, Math.min(200, deadline - performance.now())));
      this.waiters.push(() => {
        clearTimeout(t);
        resolve();
      });
    });
  }

  close(): void {
    this.ws.close();
  }
}

/** Файл с настоящим снимком: корень — снимок документа `base`. */
export function realContainer(root: Uint8Array, pages: [string, Uint8Array][] = []): Uint8Array<ArrayBuffer> {
  return buildContainer([[1, '', root], ...pages.map(([id, b]): [number, string, Uint8Array] => [2, id, b])]);
}

export function buildContainer(sections: [number, string, Uint8Array][]): Uint8Array<ArrayBuffer> {
  const parts: Uint8Array[] = [];
  const head = new Uint8Array(16);
  head.set([70, 69, 82, 82, 85, 77, 0, 0]);
  new DataView(head.buffer).setUint32(8, 1, true);
  new DataView(head.buffer).setUint32(12, sections.length, true);
  parts.push(head);
  for (const [kind, id, bytes] of sections) {
    const idb = new TextEncoder().encode(id);
    const out = new Uint8Array(5 + idb.length + 40 + bytes.length);
    const v = new DataView(out.buffer);
    out[0] = kind;
    v.setUint32(1, idb.length, true);
    out.set(idb, 5);
    v.setBigUint64(5 + idb.length, BigInt(bytes.length), true);
    out.set(bytes, 45 + idb.length);
    parts.push(out);
  }
  const total = parts.reduce((n, p) => n + p.length, 0);
  const all = new Uint8Array(total);
  let at = 0;
  for (const p of parts) {
    all.set(p, at);
    at += p.length;
  }
  return all;
}

export const newDoc = (seedPeer?: bigint): LoroDoc => {
  const d = new LoroDoc();
  if (seedPeer !== undefined) d.setPeerId(seedPeer as never);
  return d;
};
````

Создать `apps/server/test/sync-load-probe.ts`:

````ts
// Отдельный процесс нагрузки на настоящий сервер (спека подсистемы 4, раздел 13.1): клиенты — дочерний процесс, поэтому
// память процесса — только память сервера. Печатает итоги в JSON.
//   memory N     — память на соединение: N подключено и подписано.
//   latency N    — от кадра до рассылки на N участников, p50 и p95.
//   slow MB      — медленный читатель: не читает, пока другой пишет MB мегабайт, затем читает и получает всё по порядку.

import { encodeClientFrame, encodeVersion, parseServerFrame, syncPath } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import { connect } from 'node:net';
import { apiStand } from './api-stand';
import { Client } from './sync-client';

const [mode, arg] = process.argv.slice(2);
const n = Number(arg);

if (mode === 'clients') {
  // Дочерний процесс: N соединений, каждое подписано и молчит; итог — на stdout.
  const [url, cookie, origin] = process.argv.slice(4);
  let ready = 0;
  const sockets: WebSocket[] = [];
  for (let i = 0; i < n; i++) {
    const ws = new WebSocket(url, { headers: { cookie, origin } } as never);
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => {
      ws.send(encodeClientFrame({ t: 'hello', protocol: 1, schema: 1 }));
      ws.send(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) }));
    };
    ws.onmessage = (e) => {
      if (parseServerFrame(new Uint8Array(e.data as ArrayBuffer))?.t === 'subscribed') {
        if (++ready === n) console.log('ready');
      }
    };
    sockets.push(ws);
    if (i % 50 === 49) await Bun.sleep(5);
  }
  await new Promise(() => {});
}

const s = await apiStand({ syncLimits: { connectionsPerUser: 100_000, framesBurst: 1_000_000, userFramesBurst: 1_000_000 } }, true);
const who = await s.signIn(`load-${Date.now()}@example.com`);
const file = (await s.newFile(who)).id;
const url = s.publicUrl.replace('http', 'ws') + syncPath(file, who.id);

async function subscribed(count: number): Promise<Bun.Subprocess> {
  const child = Bun.spawn(['bun', import.meta.path, 'clients', String(count), url, who.cookie, s.publicUrl], {
    stdout: 'pipe',
    stderr: 'ignore',
  });
  const reader = (child.stdout as ReadableStream<Uint8Array>).getReader();
  for (let text = ''; !text.includes('ready');) {
    const r = await reader.read();
    if (r.done) throw new Error('клиенты упали');
    text += new TextDecoder().decode(r.value);
  }
  return child;
}

function percentile(v: number[], p: number): number {
  const sorted = [...v].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil((sorted.length * p) / 100) - 1)];
}

let out: Record<string, unknown> = {};
if (mode === 'memory') {
  Bun.gc(true);
  const before = process.memoryUsage().rss;
  const child = await subscribed(n);
  Bun.gc(true);
  const after = process.memoryUsage().rss;
  out = { connections: s.hub.connections(), growthMb: (after - before) / 1e6, perConnectionKb: (after - before) / n / 1000 };
  child.kill();
} else if (mode === 'latency') {
  const child = await subscribed(n);
  const writer = (await Client.open(s, who, file))!;
  writer.hello();
  writer.subscribe('root');
  await writer.wait('subscribed');
  const listener = (await Client.open(s, who, file))!;
  listener.hello();
  listener.subscribe('root');
  await listener.wait('subscribed');
  const d = new LoroDoc();
  const times: number[] = [];
  for (let i = 0; i < 200; i++) {
    const from = d.oplogVersion();
    d.getMap('m').set('x', i);
    d.commit();
    const bytes = d.export({ mode: 'update', from });
    const seen = listener.frames.length;
    const t = performance.now();
    writer.update('root', i, bytes);
    while (listener.frames.length === seen) await Bun.sleep(0);
    times.push(performance.now() - t);
    await Bun.sleep(5);
  }
  out = { participants: n, p50: percentile(times, 50), p95: percentile(times, 95), max: percentile(times, 100) };
  child.kill();
} else if (mode === 'slow') {
  const writer = (await Client.open(s, who, file))!;
  writer.hello();
  writer.subscribe('root');
  await writer.wait('subscribed');
  // Настоящий сокет, который не читает: ядро и буфер сервера заполняются, как у зависшего клиента.
  const raw = connect(new URL(s.publicUrl).port as unknown as number, '127.0.0.1');
  let chunks: Buffer[] = [];
  let handshaken: () => void;
  const up = new Promise<void>((r) => (handshaken = r));
  raw.on('data', (b: Buffer) => {
    chunks.push(b);
    if (Buffer.concat(chunks).includes('\r\n\r\n')) handshaken();
  });
  raw.write(
    `GET ${syncPath(file, who.id)} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
      `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: ${s.publicUrl}\r\nCookie: ${who.cookie}\r\n\r\n`,
  );
  await up;
  chunks = [Buffer.concat(chunks)];
  const mask = (p: Uint8Array): Buffer => {
    const head =
      p.length < 126
        ? Buffer.from([0x82, 0x80 | p.length])
        : Buffer.concat([Buffer.from([0x82, 0x80 | 126]), Buffer.from([p.length >> 8, p.length & 255])]);
    return Buffer.concat([head, Buffer.from([0, 0, 0, 0]), p]);
  };
  raw.write(mask(encodeClientFrame({ t: 'hello', protocol: 1, schema: 1 })));
  raw.write(mask(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) })));
  await Bun.sleep(300);
  raw.pause();
  let peak = 0;
  const probe = setInterval(() => (peak = Math.max(peak, s.hub.bufferedBytes())), 5);
  const d = new LoroDoc();
  const blob = 'x'.repeat(1024 * 1024);
  for (let i = 0; i < n; i++) {
    const from = d.oplogVersion();
    d.getMap('m').set('b', `${i}${blob}`);
    d.commit();
    writer.update('root', i, d.export({ mode: 'update', from }));
    await writer.wait('ack', (f) => f.clientSeq === i, 30_000);
    writer.frames.length = 0;
    Bun.gc(false);
  }
  clearInterval(probe);
  // Читает всё: сервер догоняет из базы, по порядку, без пропусков.
  raw.resume();
  const seqs: number[] = [];
  let buf = Buffer.concat(chunks);
  const skip = buf.indexOf('\r\n\r\n') + 4;
  buf = buf.subarray(skip);
  chunks = [];
  const parse = () => {
    for (;;) {
      if (buf.length < 2) return;
      const code = buf[1] & 127;
      const headLen = code === 126 ? 4 : code === 127 ? 10 : 2;
      if (buf.length < headLen) return;
      const len = code === 126 ? buf.readUInt16BE(2) : code === 127 ? Number(buf.readBigUInt64BE(2)) : code;
      if (buf.length < headLen + len) return;
      const payload = buf.subarray(headLen, headLen + len);
      if ((buf[0] & 15) === 2) {
        const f = parseServerFrame(new Uint8Array(payload));
        if (f?.t === 'updates') seqs.push(f.seq);
      }
      buf = buf.subarray(headLen + len);
    }
  };
  raw.removeAllListeners('data');
  raw.on('data', (b: Buffer) => {
    buf = Buffer.concat([buf, b]);
    parse();
  });
  parse();
  const until = performance.now() + 60_000;
  while (seqs.at(-1) !== n && performance.now() < until) await Bun.sleep(20);
  out = { frames: seqs.length, ordered: seqs.every((q, i) => q === i + 1), peakBufferedMb: peak / 1e6, sentMb: n };
  raw.destroy();
}
console.log(JSON.stringify(out));
await s.stop();
process.exit(0);
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/hub.test.ts`
Expected: FAIL — `Cannot find module '../src/sync/hub'`: 0 pass, 1 fail

- [ ] **Step 3: Реализация**

В `apps/server/src/api.ts` заменить:

````ts
// формате ошибок. Маршрут только объявляет, что ему нужно; забыть проверку нельзя, её нет в его коде.

import { ROLE_RANK, RANK_ROLE, USER_HEADER, isHash, isUuid, type ApiErrorBody, type ApiErrorCode, type FileRole } from '@ferrum/protocol';
import type { Pool } from 'pg';
import type { Auth } from './auth';
````

на:

````ts
// формате ошибок. Маршрут только объявляет, что ему нужно; забыть проверку нельзя, её нет в его коде.

import {
  ROLE_RANK,
  RANK_ROLE,
  USER_HEADER,
  isHash,
  isSyncDoc,
  isUuid,
  type ApiErrorBody,
  type ApiErrorCode,
  type FileRole,
} from '@ferrum/protocol';
import type { Pool } from 'pg';
import type { Auth } from './auth';
````

В `apps/server/src/api.ts` заменить:

````ts
  uploadPace?: UploadPace;
  maxContentBytes?: number;
}

````

на:

````ts
  uploadPace?: UploadPace;
  maxContentBytes?: number;
  /** Изменяющий запрос удался: открытые соединения сверяют роль сразу. */
  afterWrite?: (route: Route, ctx: Ctx) => void;
  /** Содержимое файла заменено целиком: проверка пачек забывает его документы. */
  contentReplaced?: (file: string) => void;
}

````

В `apps/server/src/api.ts` заменить:

````ts
  entryId: isUuid,
  hash: isHash,
};

````

на:

````ts
  entryId: isUuid,
  hash: isHash,
  doc: isSyncDoc,
};

````

В `apps/server/src/api.ts` заменить:

````ts
      try {
        const ctx = await withinDeadline(prepare(c.route, req, url, params), deadline, url.pathname);
        return await (c.route.stream ? c.route.handle(ctx) : withinDeadline(c.route.handle(ctx), deadline, url.pathname));
      } catch (e) {
        if (e instanceof Refused) return e.response;
````

на:

````ts
      try {
        const ctx = await withinDeadline(prepare(c.route, req, url, params), deadline, url.pathname);
        const res = await (c.route.stream ? c.route.handle(ctx) : withinDeadline(c.route.handle(ctx), deadline, url.pathname));
        if (req.method !== 'GET' && req.method !== 'HEAD' && res.ok) deps.afterWrite?.(c.route, ctx);
        return res;
      } catch (e) {
        if (e instanceof Refused) return e.response;
````

В `apps/server/src/app.ts` заменить:

````ts
// тесты поднимают настоящий сервер на свободном порту против настоящих Postgres и Valkey.

import { MAX_CONTENT_BYTES } from '@ferrum/protocol';
import type { Pool } from 'pg';
import { apiHandler, type ApiDeps, type Limits, type Route } from './api';
import { authRoutes } from './auth-routes';
import type { Auth } from './auth';
````

на:

````ts
// тесты поднимают настоящий сервер на свободном порту против настоящих Postgres и Valkey.

import { isUuid, MAX_CONTENT_BYTES } from '@ferrum/protocol';
import type { Pool } from 'pg';
import { apiHandler, type ApiDeps, type Ctx, type Limits, type Route } from './api';
import { authRoutes } from './auth-routes';
import type { Auth } from './auth';
````

В `apps/server/src/app.ts` заменить:

````ts
import { imageRoutes } from './image-routes';
import { shareRoutes } from './share-routes';
import { teamRoutes } from './team-routes';
import type { Mailer } from './mail';
````

на:

````ts
import { imageRoutes } from './image-routes';
import { shareRoutes } from './share-routes';
import { SyncHub, type ConnData, type SyncLimits } from './sync/hub';
import { teamRoutes } from './team-routes';
import type { Mailer } from './mail';
````

В `apps/server/src/app.ts` заменить:

````ts
  uploadPace?: UploadPace;
  maxContentBytes?: number;
}

````

на:

````ts
  uploadPace?: UploadPace;
  maxContentBytes?: number;
  /** Тесты сокращают пределы и сроки WebSocket. */
  syncLimits?: Partial<SyncLimits>;
}

/** Запись, после которой доступ к открытым файлам не изменился: содержимое, превью, картинки, отметка открытия. */
const NO_ACCESS_CHANGE = /\/(content|thumb|touch|images\/[^/]+)$/;

/** Маршрут изменил файлы или права: открытые соединения сверяют роль сразу, а не по таймеру. */
function accessChanged(hub: SyncHub, route: Route, ctx: Ctx): void {
  if (NO_ACCESS_CHANGE.test(route.path) || route.path === '/api/files') return;
  if (ctx.params.id) void hub.recheckFile(ctx.params.id).catch((e: unknown) => console.error('Проверка ролей:', e));
  else hub.recheckAll();
}

````

В `apps/server/src/app.ts` заменить:

````ts

/** Все маршруты API: отсюда их берёт и сервер, и таблица проверки прав в тестах. */
export function apiRoutes(deps: AppDeps): Route[] {
  const api = apiDeps(deps);
  const gate = uploadGate();
  return [...fileRoutes(api, gate), ...imageRoutes(api, gate), ...teamRoutes(api), ...shareRoutes(api)];
````

на:

````ts

/** Все маршруты API: отсюда их берёт и сервер, и таблица проверки прав в тестах. */
export function apiRoutes(deps: AppDeps | ApiDeps): Route[] {
  const api = apiDeps(deps as AppDeps);
  const gate = uploadGate();
  return [...fileRoutes(api, gate), ...imageRoutes(api, gate), ...teamRoutes(api), ...shareRoutes(api)];
````

В `apps/server/src/app.ts` заменить:

````ts
}

export function startServer(deps: AppDeps, port: number): Bun.Server<undefined> {
  const handleAuth = authRoutes(deps);
  const withHeaders = responseHeaders(deps.publicUrl);
  const handleApi = apiHandler(apiRoutes(deps), apiDeps(deps));
  return Bun.serve({
    port,
    // Содержимое файла идёт потоком: предел здесь только не должен быть меньше предела файла.
````

на:

````ts
}

/** Сервер и его WebSocket: остановка контейнера и тесты обращаются к обоим. */
export type Running = Bun.Server<ConnData> & { hub: SyncHub };

export function startServer(deps: AppDeps, port: number): Running {
  const hub = new SyncHub(deps.pool, deps.blobs, deps.auth, deps.publicUrl, deps.syncLimits);
  const handleAuth = authRoutes({ ...deps, hub });
  const withHeaders = responseHeaders(deps.publicUrl);
  const api: ApiDeps = {
    ...apiDeps(deps),
    afterWrite: (route, ctx) => accessChanged(hub, route, ctx),
    contentReplaced: (file) => hub.forgetFile(file),
  };
  const handleApi = apiHandler(apiRoutes(api), api);
  const server = Bun.serve({
    port,
    // Содержимое файла идёт потоком: предел здесь только не должен быть меньше предела файла.
````

В `apps/server/src/app.ts` заменить:

````ts
    routes: {
      '/healthz': async () => withHeaders(await health(deps)),
      '/api/auth/*': async (req, server) => withHeaders(await handleAuth(req, server.requestIP(req)?.address)),
      '/api/*': async (req) => withHeaders(await handleApi(req)),
    },
    fetch: async (req) => withHeaders(await serveStatic(req, deps.webDist)),
    error: (e) => {
      console.error(e);
      return withHeaders(new Response('Внутренняя ошибка сервера', { status: 500 }));
    },
  });
}
````

на:

````ts
    routes: {
      '/healthz': async () => withHeaders(await health(deps)),
      '/api/auth/*': async (req: Request, server: Bun.Server<ConnData>) =>
        withHeaders(await handleAuth(req, server.requestIP(req)?.address)),
      '/api/files/:id/sync': async (req: Bun.BunRequest<'/api/files/:id/sync'>, server: Bun.Server<ConnData>) => {
        if (!isUuid(req.params.id)) return withHeaders(new Response(null, { status: 404 }));
        const refused = await hub.upgrade(req, server, req.params.id);
        return refused && withHeaders(refused);
      },
      '/api/*': async (req: Request) => withHeaders(await handleApi(req)),
    },
    fetch: async (req: Request) => withHeaders(await serveStatic(req, deps.webDist)),
    error: (e: Error) => {
      console.error(e);
      return withHeaders(new Response('Внутренняя ошибка сервера', { status: 500 }));
    },
    websocket: hub.websocket,
  });
  return Object.assign(server, { hub });
}
````

В `apps/server/src/auth-routes.ts` заменить:

````ts
  /** Тесты сокращают срок; по умолчанию — `AUTH_DEADLINE_MS`. */
  authDeadlineMs?: number;
}

````

на:

````ts
  /** Тесты сокращают срок; по умолчанию — `AUTH_DEADLINE_MS`. */
  authDeadlineMs?: number;
  /** Выход закрывает WebSocket этой сессии сразу. */
  hub?: { sessionEnded(session: string): void; userSignedOut(user: string): void };
}

````

В `apps/server/src/auth-routes.ts` заменить:

````ts
  async function dropPreviousSession(req: Request, ip: string): Promise<void> {
    if (!req.headers.get('cookie')) return;
    const out = await inner(req, ip, '/sign-out', { method: 'POST', body: {} }).catch(() => null);
    if (!out?.ok) console.error('Не удалось снять прежнюю сессию при входе:', out?.status ?? 'сбой запроса');
  }

````

на:

````ts
  async function dropPreviousSession(req: Request, ip: string): Promise<void> {
    if (!req.headers.get('cookie')) return;
    const s = await sessionOf(req);
    const out = await inner(req, ip, '/sign-out', { method: 'POST', body: {} }).catch(() => null);
    if (!out?.ok) console.error('Не удалось снять прежнюю сессию при входе:', out?.status ?? 'сбой запроса');
    else if (s) deps.hub?.sessionEnded(s.session.id);
  }

````

В `apps/server/src/auth-routes.ts` заменить:

````ts
  }

  async function signOut(req: Request, ip: string): Promise<Response> {
    const r = await inner(req, ip, '/sign-out', { method: 'POST', body: {} });
    if (r.status === 429) return limited(r);
    if (!r.ok) throw new Error(`Better Auth: выход вернул ${r.status}`);
    return respond([r], { ok: true });
  }
````

на:

````ts
  }

  /** Сессия запроса: её соединения закрываются вместе с ней. */
  const sessionOf = (req: Request) => auth.api.getSession({ headers: req.headers, query: { disableRefresh: true } }).catch(() => null);

  async function signOut(req: Request, ip: string): Promise<Response> {
    const s = await sessionOf(req);
    const r = await inner(req, ip, '/sign-out', { method: 'POST', body: {} });
    if (r.status === 429) return limited(r);
    if (!r.ok) throw new Error(`Better Auth: выход вернул ${r.status}`);
    if (s) deps.hub?.sessionEnded(s.session.id);
    return respond([r], { ok: true });
  }
````

В `apps/server/src/auth-routes.ts` заменить:

````ts
  /** Все устройства: сессии снимает `/revoke-sessions`, cookie этого браузера убирает `/sign-out`. */
  async function signOutEverywhere(req: Request, ip: string): Promise<Response> {
    const revoked = await inner(req, ip, '/revoke-sessions', { method: 'POST', body: {} });
    if (revoked.status === 429) return limited(revoked);
````

на:

````ts
  /** Все устройства: сессии снимает `/revoke-sessions`, cookie этого браузера убирает `/sign-out`. */
  async function signOutEverywhere(req: Request, ip: string): Promise<Response> {
    const s = await sessionOf(req);
    const revoked = await inner(req, ip, '/revoke-sessions', { method: 'POST', body: {} });
    if (revoked.status === 429) return limited(revoked);
````

В `apps/server/src/auth-routes.ts` заменить:

````ts
    const out = await inner(req, ip, '/sign-out', { method: 'POST', body: {} });
    if (!out.ok) throw new Error(`Better Auth: выход вернул ${out.status}`);
    return respond([revoked, out], { ok: true });
  }
````

на:

````ts
    const out = await inner(req, ip, '/sign-out', { method: 'POST', body: {} });
    if (!out.ok) throw new Error(`Better Auth: выход вернул ${out.status}`);
    if (s) deps.hub?.userSignedOut(s.user.id);
    return respond([revoked, out], { ok: true });
  }
````

В `apps/server/src/auth-routes.ts` заменить:

````ts
}

/** Проверка перед открытием WebSocket (спека, разделы 5 и 7.1): верный `Origin` и живая сессия
 * в Valkey. Сам WebSocket синхронизации — этап 3. */
export async function authorizeUpgrade(req: Request, auth: Auth, publicUrl: string): Promise<UpgradeSession | null> {
  if (!originAllowed(req, publicUrl)) return null;
````

на:

````ts
}

/** Проверка перед открытием WebSocket (спека, разделы 5 и 7.1): верный `Origin` и живая сессия в Valkey. */
export async function authorizeUpgrade(req: Request, auth: Auth, publicUrl: string): Promise<UpgradeSession | null> {
  if (!originAllowed(req, publicUrl)) return null;
````

В `apps/server/src/file-routes.ts` заменить:

````ts
import { apiError, busy, ok, readJson, type ApiDeps, type Ctx, type Route } from './api';
import { BlobStalled, BlobTooLarge, type UploadPace } from './blob-store';
import { inTransaction } from './db';

````

на:

````ts
import { apiError, busy, ok, readJson, type ApiDeps, type Ctx, type Route } from './api';
import { BlobStalled, BlobTooLarge, type UploadPace } from './blob-store';
import { readDocSnapshot } from './sync/container-read';
import { inTransaction } from './db';

````

В `apps/server/src/file-routes.ts` заменить:

````ts
  const version = Number(row.version);
  return version === base || (previous !== null && row.last_write_id === previous && version === base + 1);
}

````

на:

````ts
  const version = Number(row.version);
  return version === base || (previous !== null && row.last_write_id === previous && version === base + 1);
}

/** Правки уже идут журналом: запись файла целиком (вкладка прежней версии приложения) затёрла бы их. */
async function journalStarted(db: Pool | PoolClient, file: string): Promise<boolean> {
  const { rowCount } = await db.query('SELECT 1 FROM doc_state WHERE file_id = $1 AND next_seq > 1 LIMIT 1', [file]);
  return (rowCount ?? 0) > 0;
}

````

В `apps/server/src/file-routes.ts` заменить:

````ts
        if (!before.rows[0]) return apiError(404, 'NOT_FOUND');
        if (!writeAllowed(before.rows[0], write.base, write.previous)) return apiError(409, 'CONFLICT');
        if (!gate.enter(ctx.user.id)) return busy();
        const key = `files/${ctx.params.id}/content-${randomUUID()}`;
````

на:

````ts
        if (!before.rows[0]) return apiError(404, 'NOT_FOUND');
        if (!writeAllowed(before.rows[0], write.base, write.previous)) return apiError(409, 'CONFLICT');
        if (await journalStarted(pool, ctx.params.id)) return apiError(409, 'CONFLICT');
        if (!gate.enter(ctx.user.id)) return busy();
        const key = `files/${ctx.params.id}/content-${randomUUID()}`;
````

В `apps/server/src/file-routes.ts` заменить:

````ts
              commitRefusal(row) ??
              (writeAllowed(row, write.base, write.previous) ? null : apiError(409, 'CONFLICT')) ??
              (await missingImages(c, ctx.params.id, refs, write.lost));
            const settled = await settleUpload(c, key, refused);
            if (settled) return settled;
            await trashBlob(c, row.content_key);
            await c.query(
              `UPDATE file SET version = version + 1, size = $2, content_key = $3, last_write_id = $4, edited_at = now() WHERE id = $1`,
````

на:

````ts
              commitRefusal(row) ??
              (writeAllowed(row, write.base, write.previous) ? null : apiError(409, 'CONFLICT')) ??
              ((await journalStarted(c, ctx.params.id)) ? apiError(409, 'CONFLICT') : null) ??
              (await missingImages(c, ctx.params.id, refs, write.lost));
            const settled = await settleUpload(c, key, refused);
            if (settled) return settled;
            await trashBlob(c, row.content_key);
            // Журнала нет: строки документов описывали прежние байты, следующая подписка заведёт новые.
            await c.query('DELETE FROM doc_state WHERE file_id = $1', [ctx.params.id]);
            await c.query(
              `UPDATE file SET version = version + 1, size = $2, content_key = $3, last_write_id = $4, edited_at = now() WHERE id = $1`,
````

В `apps/server/src/file-routes.ts` заменить:

````ts
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
````

на:

````ts
          if (outcome) {
            await blobs.delete(key).catch(() => {});
            return outcome;
          }
          deps.contentReplaced?.(ctx.params.id);
          return await fresh(ctx);
        } finally {
          gate.leave(ctx.user.id);
        }
      },
    },
    {
      // Клиент отстал сильнее журнала (спека, раздел 7.3): снимок одного документа, поверх него — журнал.
      method: 'GET',
      path: '/api/files/:id/docs/:doc/snapshot',
      access: { kind: 'file', min: 'viewer' },
      limit: 'read',
      handle: async (ctx) => {
        const { rows } = await pool.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [ctx.params.id]);
        const bytes = await readDocSnapshot(blobs, rows[0]?.content_key ?? null, ctx.params.doc);
        if (!bytes) return apiError(404, 'NOT_FOUND');
        return new Response(bytes as Uint8Array<ArrayBuffer>, {
          headers: {
            'Content-Type': 'application/octet-stream',
            'Content-Disposition': 'attachment',
            'Cache-Control': 'no-store',
            'Cross-Origin-Resource-Policy': 'same-origin',
          },
        });
      },
    },
    {
      method: 'GET',
````

В `apps/server/src/index.ts` заменить:

````ts
stop = () => {
  stopGc();
  void server
    .stop()
    .then(() => pool.end())
    .finally(() => process.exit(0));
````

на:

````ts
stop = () => {
  stopGc();
  // Принятое дописывается, соединения закрываются (клиенты подключатся заново): `server.stop()` без `true`
  // ждал бы их, и контейнер дождался бы SIGKILL (B-207).
  void server.hub
    .stop()
    .then(() => server.stop(true))
    .then(() => pool.end())
    .finally(() => process.exit(0));
````

Создать `apps/server/src/sync/hub.ts`:

````ts
// WebSocket файлов (спека подсистемы 4, разделы 7.1–7.3): открытие с проверкой `Origin`, сессии и роли, кадры с
// пределами, порядок «проверка → журнал → ack и рассылка», догоняние с учётом медленного сокета. Сервер не падает ни
// от сбоя базы, ни от чужого клиента: всякий обработчик под `try`, очереди и память на соединение ограничены.

import {
  encodeServerFrame,
  encodeVersion,
  parseClientFrame,
  parseVersion,
  RANK_ROLE,
  ROLE_RANK,
  snapshotPath,
  SYNC_CLOSE,
  SYNC_PROTOCOL,
  versionIncludes,
  type FileRole,
  type RevokeReason,
  type ServerFrame,
  type SyncRefusal,
  type VersionMap,
} from '@ferrum/protocol';
import type { Pool } from 'pg';
import type { Auth } from '../auth';
import { authorizeUpgrade, type UpgradeSession } from '../auth-routes';
import type { BlobStore } from '../blob-store';
import { readDocSnapshot } from './container-read';
import { UpdateLog, type Item, type LogLimits } from './update-log';
import { Validator } from './validator';

export interface ConnData {
  id: number;
  file: string;
  user: string;
  /** Сессия при открытии: выход на этом устройстве закрывает её соединения сразу. */
  session: string;
  /** Cookie открытия: периодическая проверка, что сессия жива (срок, снятие администратором). */
  cookie: string;
  role: FileRole;
  hello: boolean;
  docs: Map<string, DocSub>;
  frames: Bucket;
  ws: Bun.ServerWebSocket<ConnData> | null;
  closed: boolean;
  /** Кто ждёт освобождения сокета. */
  drained: (() => void)[];
  /** Сколько раз подряд прислал негодное. */
  strikes: number;
  helloTimer: Timer | null;
  lagSince: number;
}

interface DocSub {
  version: VersionMap;
  /** Последняя запись, которую соединение уже получило. */
  sent: number;
  /** `true` — рассылка идёт напрямую; иначе соединение догоняет из базы. */
  live: boolean;
  pumping: boolean;
  /** Подписку повторили: идущий проход заканчивается. */
  generation: number;
}

export interface SyncLimits {
  framesPerSecond: number;
  framesBurst: number;
  userFramesPerSecond: number;
  userFramesBurst: number;
  userBytesPerSecond: number;
  userBytesBurst: number;
  connectionsPerUser: number;
  connectionsTotal: number;
  subscribesPerConnection: number;
  /** Буфер сокета: выше — рассылка этому соединению идёт с паузой и из базы. */
  highWaterBytes: number;
  /** Не вычитывает дольше — закрывается. */
  stalledMs: number;
  /** Плохих кадров подряд, дальше соединение закрывается. */
  strikes: number;
  helloMs: number;
  recheckMs: number;
  sessionCheckMs: number;
  log: Partial<LogLimits>;
}

/** Настоящая нагрузка и запас (`hub.test.ts` прогоняет перетаскивание, набор и большую вставку через эти пределы):
 * ядро шлёт пачку на документ не чаще 30 раз в секунду, поэтому 120 кадров в секунду с запасом хватает и на правки
 * двух документов разом, и на подписки при открытии файла со многими страницами. */
export const SYNC_LIMITS: SyncLimits = {
  framesPerSecond: 120,
  framesBurst: 2000,
  userFramesPerSecond: 480,
  userFramesBurst: 4000,
  userBytesPerSecond: 64 * 1024 * 1024,
  userBytesBurst: 256 * 1024 * 1024,
  connectionsPerUser: 64,
  connectionsTotal: 20_000,
  subscribesPerConnection: 4096,
  highWaterBytes: 1024 * 1024,
  stalledMs: 120_000,
  strikes: 16,
  helloMs: 15_000,
  recheckMs: 30_000,
  sessionCheckMs: 60_000,
  log: {},
};

/** Столько записей журнала за один запрос догоняния и столько их байт. */
const PUMP_ROWS = 512;
const PUMP_BYTES = 8 * 1024 * 1024;

class Bucket {
  private tokens: number;
  private last = performance.now();
  constructor(
    private readonly rate: number,
    private readonly burst: number,
  ) {
    this.tokens = burst;
  }
  /** Взять `n`; не хватает — сколько миллисекунд ждать. */
  take(n: number): number {
    const now = performance.now();
    this.tokens = Math.min(this.burst, this.tokens + ((now - this.last) / 1000) * this.rate);
    this.last = now;
    if (this.tokens >= n) {
      this.tokens -= n;
      return 0;
    }
    return Math.ceil(((n - this.tokens) / this.rate) * 1000);
  }
}

interface UserState {
  frames: Bucket;
  bytes: Bucket;
  conns: number;
}

const toMap = (o: Record<string, number>): VersionMap => new Map(Object.entries(o));

export class SyncHub {
  readonly validator: Validator;
  readonly log: UpdateLog;
  private readonly limits: SyncLimits;
  private readonly conns = new Map<string, Set<ConnData>>();
  private readonly users = new Map<string, UserState>();
  private total = 0;
  private seq = 0;
  private readonly timers: Timer[] = [];
  private stopped = false;
  /** Вызывается после фиксации: сжатие считает записи журнала; при уходе последнего — тоже. */
  onLog: (file: string, doc: string, logCount: number, logBytes: number) => void = () => {};
  onLastLeft: (file: string) => void = () => {};

  constructor(
    private readonly pool: Pool,
    private readonly blobs: BlobStore,
    private readonly auth: Auth,
    private readonly publicUrl: string,
    limits: Partial<SyncLimits> = {},
  ) {
    this.limits = { ...SYNC_LIMITS, ...limits };
    this.validator = new Validator(pool, blobs);
    this.log = new UpdateLog(pool, this.limits.log);
    this.log.onCommitted = (items, states) => {
      for (const { item, seq } of items) this.broadcast(item, seq);
      for (const s of states) this.onLog(s.file, s.doc, s.logCount, s.logBytes);
    };
    this.timers.push(setInterval(() => void this.recheckEverything().catch(report), this.limits.recheckMs));
    this.timers.push(setInterval(() => void this.checkSessions().catch(report), this.limits.sessionCheckMs));
    this.timers.push(setInterval(() => this.closeStalled(), Math.min(5000, this.limits.stalledMs)));
    for (const t of this.timers) t.unref?.();
  }

  /** Проверка перед открытием: `Origin` и живая сессия (`authorizeUpgrade`), вкладка за того же человека, роль, предел соединений. */
  async upgrade(req: Request, server: Bun.Server<ConnData>, file: string): Promise<Response | undefined> {
    if (this.stopped) return new Response(null, { status: 503 });
    let session: UpgradeSession | null;
    try {
      session = await authorizeUpgrade(req, this.auth, this.publicUrl);
    } catch {
      return new Response(null, { status: 503 });
    }
    if (!session) return new Response(null, { status: 401 });
    // Вкладка работает за другого человека (вход сменили в другой вкладке): правки X не пишутся под Y.
    const expected = new URL(req.url).searchParams.get('user');
    if (expected !== session.userId) return new Response(null, { status: 401 });
    let role: FileRole | null;
    try {
      role = await this.role(file, session.userId);
    } catch {
      return new Response(null, { status: 503 });
    }
    if (!role) return new Response(null, { status: 404 });
    if (this.total >= this.limits.connectionsTotal || (this.users.get(session.userId)?.conns ?? 0) >= this.limits.connectionsPerUser) {
      return new Response(null, { status: 429, headers: { 'Retry-After': '5' } });
    }
    const data: ConnData = {
      id: ++this.seq,
      file,
      user: session.userId,
      session: session.sessionId,
      cookie: req.headers.get('cookie') ?? '',
      role,
      hello: false,
      docs: new Map(),
      frames: new Bucket(this.limits.framesPerSecond, this.limits.framesBurst),
      ws: null,
      closed: false,
      drained: [],
      strikes: 0,
      helloTimer: null,
      lagSince: 0,
    };
    if (!server.upgrade(req, { data })) return new Response(null, { status: 400 });
    return undefined;
  }

  private async role(file: string, user: string): Promise<FileRole | null> {
    const { rows } = await this.pool.query<{ rank: number | null }>('SELECT file_role_rank($1, $2) AS rank', [file, user]);
    return RANK_ROLE[rows[0]?.rank ?? 0];
  }

  /** Настройка `websocket` для `Bun.serve`. */
  readonly websocket: Bun.WebSocketHandler<ConnData> = {
    maxPayloadLength: 16 * 1024 * 1024 + 1024,
    idleTimeout: 120,
    sendPings: true,
    // Память на медленного читателя: дальше сокет не копит, соединение догонит из базы.
    backpressureLimit: 8 * 1024 * 1024,
    closeOnBackpressureLimit: false,
    open: (ws) => {
      guard(() => this.opened(ws));
    },
    message: (ws, message) => {
      guard(() => this.received(ws, message));
    },
    drain: (ws) => {
      guard(() => {
        ws.data.lagSince = 0;
        for (const w of ws.data.drained.splice(0)) w();
      });
    },
    close: (ws) => {
      guard(() => this.closed(ws));
    },
  };

  private opened(ws: Bun.ServerWebSocket<ConnData>): void {
    const d = ws.data;
    d.ws = ws;
    this.total++;
    let set = this.conns.get(d.file);
    if (!set) this.conns.set(d.file, (set = new Set()));
    set.add(d);
    let u = this.users.get(d.user);
    if (!u) {
      u = {
        frames: new Bucket(this.limits.userFramesPerSecond, this.limits.userFramesBurst),
        bytes: new Bucket(this.limits.userBytesPerSecond, this.limits.userBytesBurst),
        conns: 0,
      };
      this.users.set(d.user, u);
    }
    u.conns++;
    d.helloTimer = setTimeout(() => {
      if (!d.hello) ws.close(SYNC_CLOSE.BAD_FRAME, 'hello');
    }, this.limits.helloMs);
  }

  private closed(ws: Bun.ServerWebSocket<ConnData>): void {
    const d = ws.data;
    if (d.closed) return;
    d.closed = true;
    d.ws = null;
    if (d.helloTimer) clearTimeout(d.helloTimer);
    for (const w of d.drained.splice(0)) w();
    this.total--;
    const u = this.users.get(d.user);
    if (u && --u.conns === 0) this.users.delete(d.user);
    const set = this.conns.get(d.file);
    set?.delete(d);
    if (set?.size === 0) {
      this.conns.delete(d.file);
      this.onLastLeft(d.file);
    }
  }

  private send(d: ConnData, frame: ServerFrame): void {
    this.sendBytes(d, encodeServerFrame(frame));
  }

  private sendBytes(d: ConnData, bytes: Uint8Array): number {
    if (d.closed || !d.ws) return 0;
    return d.ws.sendBinary(bytes);
  }

  private strike(d: ConnData): void {
    if (++d.strikes >= this.limits.strikes) d.ws?.close(SYNC_CLOSE.BAD_FRAME, 'bad frames');
  }

  private received(ws: Bun.ServerWebSocket<ConnData>, message: string | Buffer): void {
    const d = ws.data;
    if (typeof message === 'string') {
      ws.close(SYNC_CLOSE.BAD_FRAME, 'text');
      return;
    }
    const frame = parseClientFrame(message);
    if (!frame) {
      ws.close(SYNC_CLOSE.BAD_FRAME, 'frame');
      return;
    }
    if (frame.t === 'ping') {
      this.send(d, { t: 'pong' });
      return;
    }
    if (!d.hello && frame.t !== 'hello') {
      ws.close(SYNC_CLOSE.BAD_FRAME, 'hello first');
      return;
    }
    const u = this.users.get(d.user)!;
    const wait = Math.max(d.frames.take(1), u.frames.take(1), frame.t === 'update' ? u.bytes.take(frame.bytes.length) : 0);
    if (wait > 0) {
      // Предел частоты — пауза: кадр не принят, клиент пришлёт его снова. Подписка и правка равны перед ним.
      this.send(d, { t: 'pause', ms: Math.min(wait, 30_000) });
      return;
    }
    switch (frame.t) {
      case 'hello':
        this.hello(d, frame.protocol, frame.schema);
        break;
      case 'subscribe':
        void this.subscribe(d, frame.doc, frame.version).catch((e: unknown) => {
          console.error('Подписка:', e instanceof Error ? e.message : e);
          // База недоступна: соединение закрывается, клиент переподключится и подпишется заново.
          d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'unavailable');
        });
        break;
      case 'update':
        void this.update(d, frame.doc, frame.clientSeq, frame.bytes).catch((e: unknown) => {
          console.error('Приём пачки:', e instanceof Error ? e.message : e);
          this.refuse(d, frame.doc, frame.clientSeq, 'UNAVAILABLE');
        });
        break;
      case 'ephemeral':
        // Присутствие — этап 4: кадр известен, сервер его пока не рассылает.
        break;
    }
  }

  private hello(d: ConnData, protocol: number, schema: number): void {
    if (d.hello) return;
    if (protocol !== SYNC_PROTOCOL) {
      this.send(d, { t: 'please-reload' });
      d.ws?.close(SYNC_CLOSE.RELOAD, 'protocol');
      return;
    }
    d.hello = true;
    if (d.helloTimer) clearTimeout(d.helloTimer);
    d.helloTimer = null;
    void this.pool
      .query<{ version: string; member: number; schema_version: number }>(
        `UPDATE file SET schema_version = GREATEST(schema_version, $3) WHERE id = $1
         RETURNING version, file_member_rank(id, $2) AS member, schema_version`,
        [d.file, d.user, Math.min(schema, 1_000_000)],
      )
      .then(({ rows }) => {
        if (rows[0])
          this.send(d, { t: 'role', role: d.role, canShare: rows[0].member >= ROLE_RANK.editor, version: Number(rows[0].version) });
      })
      .catch(report);
  }

  private refuse(d: ConnData, doc: string, clientSeq: number, code: SyncRefusal): void {
    this.send(d, { t: 'refused', doc, clientSeq, code });
  }

  private async update(d: ConnData, doc: string, clientSeq: number, frame: Uint8Array): Promise<void> {
    // Буфер кадра живёт только на время обработчика сокета: дальше — своя копия.
    const bytes = frame.slice();
    // Читатель не пишет: и по последней роли соединения, и в транзакции записи.
    if (ROLE_RANK[d.role] < ROLE_RANK.editor) {
      this.strike(d);
      this.refuse(d, doc, clientSeq, 'FORBIDDEN');
      return;
    }
    if (!d.docs.has(doc)) {
      this.strike(d);
      this.refuse(d, doc, clientSeq, 'GAP');
      return;
    }
    const verdict = await this.validator.validate(d.file, doc, bytes);
    if (verdict.kind === 'bad') {
      this.strike(d);
      this.refuse(d, doc, clientSeq, 'BAD_UPDATE');
      return;
    }
    if (verdict.kind === 'retry') {
      this.refuse(d, doc, clientSeq, 'UNAVAILABLE');
      return;
    }
    if (verdict.pending) {
      this.refuse(d, doc, clientSeq, 'GAP');
      return;
    }
    d.strikes = 0;
    const item: Item = { file: d.file, doc, user: d.user, bytes, start: verdict.start, end: verdict.end };
    (item as ItemFrom).from = d;
    const outcome = await this.log.submit(item);
    if (outcome.kind === 'refused') {
      this.refuse(d, doc, clientSeq, outcome.code);
      return;
    }
    this.send(d, { t: 'ack', doc, clientSeq, seq: outcome.seq });
  }

  /** Правка уходит подписчикам этого документа в порядке `seq`; отправителю только `ack`. */
  private broadcast(item: Item, seq: number): void {
    const set = this.conns.get(item.file);
    if (!set) return;
    const from = (item as ItemFrom).from;
    let frame: Uint8Array | null = null;
    for (const d of set) {
      const sub = d.docs.get(item.doc);
      if (!sub) continue;
      if (d === from) {
        // Своя правка: соединению она не нужна, а номер двигается, иначе догоняние пошлёт её обратно.
        if (sub.live && sub.sent === seq - 1) sub.sent = seq;
        for (const [peer, counter] of Object.entries(item.end)) sub.version.set(peer, Math.max(sub.version.get(peer) ?? 0, counter));
        continue;
      }
      if (!sub.live) continue;
      // Пропуск в рассылке (соединение отстало) — только через догоняние из базы.
      if (sub.sent !== seq - 1 || this.slow(d)) {
        this.goLag(d, item.doc, sub);
        continue;
      }
      frame ??= encodeServerFrame({ t: 'updates', doc: item.doc, seq, bytes: item.bytes });
      const sent = this.sendBytes(d, frame);
      if (sent === 0) {
        // Сокет отказал: кадр не ушёл, догонит из базы.
        this.goLag(d, item.doc, sub);
        continue;
      }
      sub.sent = seq;
    }
  }

  private slow(d: ConnData): boolean {
    return !!d.ws && d.ws.getBufferedAmount() > this.limits.highWaterBytes;
  }

  private goLag(d: ConnData, doc: string, sub: DocSub): void {
    sub.live = false;
    if (!d.lagSince) d.lagSince = performance.now();
    if (!sub.pumping) void this.pump(d, doc, sub, sub.generation, false).catch((e: unknown) => this.pumpFailed(d, e));
  }

  private pumpFailed(d: ConnData, e: unknown): void {
    console.error('Догоняние:', e instanceof Error ? e.message : e);
    d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'unavailable');
  }

  /** Сокет освободился; не занят — сразу. */
  private whenWritable(d: ConnData): Promise<void> {
    if (d.closed || !this.slow(d)) return Promise.resolve();
    if (!d.lagSince) d.lagSince = performance.now();
    return new Promise((resolve) => d.drained.push(resolve));
  }

  private async subscribe(d: ConnData, doc: string, raw: Uint8Array): Promise<void> {
    const version = parseVersion(raw);
    if (!version) return;
    const old = d.docs.get(doc);
    if (!old && d.docs.size >= this.limits.subscribesPerConnection) {
      d.ws?.close(SYNC_CLOSE.BAD_FRAME, 'too many docs');
      return;
    }
    const sub: DocSub = { version, sent: 0, live: false, pumping: false, generation: (old?.generation ?? 0) + 1 };
    d.docs.set(doc, sub);
    const state = await this.docState(d.file, doc);
    if (d.closed || d.docs.get(doc) !== sub) return;
    // Клиент отстал сильнее журнала: снимок вобрал правки, которых у него нет.
    if (state && !versionIncludes(version, toMap(state.base_vv))) {
      d.docs.delete(doc);
      this.send(d, { t: 'snapshot-needed', doc, url: snapshotPath(d.file, doc) });
      return;
    }
    await this.pump(d, doc, sub, sub.generation, true);
  }

  /** Строка документа; первая подписка заводит её с версией снимка файла. Читатель новых строк не заводит: у документа
   * без снимка (страница, в которую ещё не писали) журнала нет, и версия у него пустая. */
  private async docState(file: string, doc: string): Promise<{ base_vv: Record<string, number> } | null> {
    const found = await this.pool.query<{ base_vv: Record<string, number> }>(
      'SELECT base_vv FROM doc_state WHERE file_id = $1 AND doc = $2',
      [file, doc],
    );
    if (found.rows[0]) return found.rows[0];
    const { rows } = await this.pool.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [file]);
    const snapshot = await readDocSnapshot(this.blobs, rows[0]?.content_key ?? null, doc);
    if (!snapshot) return null;
    const base = await this.validator.baseVersion(snapshot);
    await this.pool.query(
      `INSERT INTO doc_state (file_id, doc, vv, base_vv) VALUES ($1, $2, $3::jsonb, $3::jsonb) ON CONFLICT DO NOTHING`,
      [file, doc, JSON.stringify(base)],
    );
    return { base_vv: base };
  }

  /** Догоняние: всё, что есть в журнале сверх `sub.sent` и не покрыто версией клиента, кусками и с учётом сокета. Когда
   * догнал — соединение снова получает рассылку напрямую. `announce` — конец подписки: клиенту сообщают версию сервера. */
  private async pump(d: ConnData, doc: string, sub: DocSub, generation: number, announce: boolean): Promise<void> {
    if (sub.pumping) return;
    sub.pumping = true;
    try {
      for (;;) {
        if (d.closed || sub.generation !== generation || d.docs.get(doc) !== sub) return;
        const { rows } = await this.pool.query<{ seq: string; end_vv: Record<string, number>; size: number }>(
          `SELECT seq, end_vv, octet_length(bytes) AS size FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq > $3 ORDER BY seq LIMIT $4`,
          [d.file, doc, sub.sent, PUMP_ROWS],
        );
        if (rows.length === 0) {
          const head = await this.pool.query<{ next_seq: string; vv: Record<string, number> }>(
            'SELECT next_seq, vv FROM doc_state WHERE file_id = $1 AND doc = $2',
            [d.file, doc],
          );
          if (d.closed || sub.generation !== generation) return;
          const seq = head.rows[0] ? Number(head.rows[0].next_seq) - 1 : 0;
          // Журнал пуст после сжатия, а версия клиента покрывает снимок: писать нечего.
          if (seq > sub.sent) sub.sent = seq;
          sub.live = true;
          d.lagSince = 0;
          if (announce) {
            this.send(d, { t: 'subscribed', doc, seq, version: encodeVersion(toMap(head.rows[0]?.vv ?? {})) });
          }
          return;
        }
        const need: number[] = [];
        let bytes = 0;
        for (const r of rows) {
          // Клиент уже знает: не шлём, но и не останавливаемся.
          if (versionIncludes(sub.version, toMap(r.end_vv))) {
            if (need.length === 0) sub.sent = Number(r.seq);
            continue;
          }
          if (need.length > 0 && bytes + r.size > PUMP_BYTES) break;
          need.push(Number(r.seq));
          bytes += r.size;
        }
        if (need.length === 0) continue;
        const got = await this.pool.query<{ seq: string; bytes: Uint8Array }>(
          'SELECT seq, bytes FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq = ANY($3::bigint[]) ORDER BY seq',
          [d.file, doc, need],
        );
        for (const r of got.rows) {
          await this.whenWritable(d);
          if (d.closed || sub.generation !== generation) return;
          this.send(d, { t: 'updates', doc, seq: Number(r.seq), bytes: r.bytes });
          sub.sent = Number(r.seq);
        }
        // Сжатие убрало записи посреди прохода: всё, что до них, вошло в снимок. Клиент подпишется заново и получит его.
        if (got.rows.length < need.length) {
          d.docs.delete(doc);
          this.send(d, { t: 'snapshot-needed', doc, url: snapshotPath(d.file, doc) });
          return;
        }
        // Рядом с `sent` могли лежать записи, которые клиент знает: следующий запрос их пропустит.
      }
    } finally {
      sub.pumping = false;
    }
  }

  /** Роль соединений файла заново: снята — закрыть сразу, понижена — сообщить. Зовут маршруты после правки доступа;
   * раз в `recheckMs` — на случай изменений мимо маршрутов. */
  async recheckFile(file: string): Promise<void> {
    const set = this.conns.get(file);
    if (!set || set.size === 0) return;
    const users = [...new Set([...set].map((d) => d.user))];
    const { rows } = await this.pool.query<{ u: string; rank: number | null; member: number | null; version: string | null }>(
      `SELECT u, file_role_rank($1, u) AS rank, file_member_rank($1, u) AS member,
              (SELECT version FROM file WHERE id = $1) AS version
       FROM unnest($2::text[]) AS x(u)`,
      [file, users],
    );
    const by = new Map(rows.map((r) => [r.u, r]));
    for (const d of set) {
      const r = by.get(d.user);
      const role = RANK_ROLE[r?.rank ?? 0];
      if (!role) {
        this.revoke(d, 'access');
        continue;
      }
      if (role !== d.role) {
        d.role = role;
        this.send(d, { t: 'role', role, canShare: (r?.member ?? 0) >= ROLE_RANK.editor, version: Number(r?.version ?? 0) });
      }
    }
  }

  private async recheckEverything(): Promise<void> {
    for (const file of this.conns.keys()) await this.recheckFile(file);
  }

  /** Команды и проекты: меняется доступ ко многим файлам сразу. */
  recheckAll(): void {
    void this.recheckEverything().catch(report);
  }

  private revoke(d: ConnData, reason: RevokeReason): void {
    this.send(d, { t: 'access-revoked', reason });
    d.ws?.close(reason === 'session' ? SYNC_CLOSE.SESSION : SYNC_CLOSE.REVOKED, reason);
  }

  /** Выход на этом устройстве: соединения этой сессии закрываются сразу. */
  sessionEnded(session: string): void {
    for (const set of this.conns.values()) for (const d of set) if (d.session === session) this.revoke(d, 'session');
  }

  /** Выход на всех устройствах. */
  userSignedOut(user: string): void {
    for (const set of this.conns.values()) for (const d of set) if (d.user === user) this.revoke(d, 'session');
  }

  /** Сессия истекла или снята мимо сервера. Сбой Valkey — не повод выгонять людей: проверим в следующий раз. */
  private async checkSessions(): Promise<void> {
    const seen = new Map<string, boolean>();
    for (const set of this.conns.values()) {
      for (const d of set) {
        let alive = seen.get(d.cookie);
        if (alive === undefined) {
          const s = await this.auth.api.getSession({ headers: new Headers({ cookie: d.cookie }), query: { disableRefresh: true } });
          alive = !!s && s.session.id === d.session;
          seen.set(d.cookie, alive);
        }
        if (!alive) this.revoke(d, 'session');
      }
    }
  }

  private closeStalled(): void {
    const now = performance.now();
    for (const set of this.conns.values()) {
      for (const d of set) {
        if (d.lagSince && now - d.lagSince > this.limits.stalledMs) d.ws?.close(SYNC_CLOSE.SLOW, 'slow');
      }
    }
  }

  /** Файл заменён целиком (`PUT content` до начала журнала). */
  forgetFile(file: string): void {
    this.validator.forget(file);
  }

  /** Сколько байт ждут в буферах сокетов: тесты и проба медленного читателя. */
  bufferedBytes(): number {
    let n = 0;
    for (const set of this.conns.values()) for (const d of set) n += d.ws?.getBufferedAmount() ?? 0;
    return n;
  }

  /** Сколько соединений: тесты и проба памяти. */
  connections(): number {
    return this.total;
  }

  /** Остановка контейнера: принятое дописывается, соединения закрываются, клиенты переподключатся. */
  async stop(): Promise<void> {
    this.stopped = true;
    for (const t of this.timers) clearInterval(t);
    await this.log.stop();
    for (const set of this.conns.values()) for (const d of set) d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'restart');
    this.validator.stop();
  }
}

type ItemFrom = Item & { from?: ConnData };

function report(e: unknown): void {
  console.error('Синхронизация:', e instanceof Error ? e.message : e);
}

/** Обработчик сокета не должен ронять процесс. */
function guard(work: () => void): void {
  try {
    work();
  } catch (e) {
    report(e);
  }
}
````

В `apps/server/src/sync/validator.ts` заменить:

````ts

export class Validator {
  private worker: Worker;
  private seq = 0;
  private loads = 0;
````

на:

````ts

export class Validator {
  /** Заводится по первой работе: сервер без открытых файлов память wasm не держит. */
  private worker: Worker | null = null;
  private seq = 0;
  private loads = 0;
````

В `apps/server/src/sync/validator.ts` заменить:

````ts
    private readonly blobs: BlobStore,
  ) {
    this.worker = this.spawn();
    this.watchdog = setInterval(() => this.checkHung(), 1000);
    this.watchdog.unref?.();
````

на:

````ts
    private readonly blobs: BlobStore,
  ) {
    this.watchdog = setInterval(() => this.checkHung(), 1000);
    this.watchdog.unref?.();
````

В `apps/server/src/sync/validator.ts` заменить:

````ts
    if (this.stopped) return;
    this.restarts++;
    this.worker.terminate();
    this.worker = this.spawn();
    this.loads = 0;
````

на:

````ts
    if (this.stopped) return;
    this.restarts++;
    this.worker?.terminate();
    this.worker = this.spawn();
    this.loads = 0;
````

В `apps/server/src/sync/validator.ts` заменить:

````ts
    return new Promise((done) => {
      this.waiting.set(id, { done, size, at: performance.now() });
      this.worker.postMessage({ ...work, id } as ToValidator);
    });
````

на:

````ts
    return new Promise((done) => {
      this.waiting.set(id, { done, size, at: performance.now() });
      this.worker ??= this.spawn();
      this.worker.postMessage({ ...work, id } as ToValidator);
    });
````

В `apps/server/src/sync/validator.ts` заменить:

````ts
    this.stopped = true;
    clearInterval(this.watchdog);
    this.worker.terminate();
    for (const w of this.waiting.values()) w.done(null);
    this.waiting.clear();
````

на:

````ts
    this.stopped = true;
    clearInterval(this.watchdog);
    this.worker?.terminate();
    for (const w of this.waiting.values()) w.done(null);
    this.waiting.clear();
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `bun test` 23 и 400 pass

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/src/api.ts apps/server/src/app.ts apps/server/src/auth-routes.ts apps/server/src/file-routes.ts apps/server/src/index.ts apps/server/src/sync/hub.ts apps/server/src/sync/validator.ts apps/server/test/access.test.ts apps/server/test/api-stand.ts apps/server/test/connection-drop-probe.ts apps/server/test/db.test.ts apps/server/test/hub-load.test.ts apps/server/test/hub.test.ts apps/server/test/sync-client.ts apps/server/test/sync-load-probe.ts
git commit -F - <<'EOF'
сервер: WebSocket файла, роли и выход закрывают соединение сразу

`SyncHub`: открытие с проверкой `Origin`, сессии, «вкладка за того же человека» и роли; `hello`, `subscribe`, `update`;
порядок «проверка → журнал → ack и рассылка»; догоняние из базы по версии клиента, кусками и с учётом сокета.
Отставший сильнее журнала получает `snapshot-needed` и снимок документа (`GET …/docs/:doc/snapshot`).

Снятие роли, отключение ссылки, новая ссылка, корзина, снятие из команды закрывают соединение сразу, понижение —
сообщение `role` (B-237, B-296): маршрут сообщает об изменении из конвейера, забыть нельзя; раз в 30 с — сверка
на случай изменений мимо маршрутов. Выход закрывает соединения сессии, «на всех» — человека; раз в минуту проверка, что
сессия жива, сбой Valkey никого не выгоняет. Запись файла целиком при идущем журнале — `CONFLICT`: вкладка прежней
версии не затрёт правки. `hello { schema }` заполняет `file.schema_version` (B-245). `server.stop` не ждёт живых WebSocket (B-207).

Пределы проверены настоящей работой: перетаскивание 30 раз в секунду, набор, вставка кадрами по 5 МБ. Сверх предела —
`pause`, соединение живо. Медленный читатель: в буфере сокета предел и один кадр, остальное догоняет из базы по порядку
(проба на настоящем сокете, который не читает). 1000 соединений — 62 КБ на соединение. От кадра до рассылки на 50
участников — 36 мс, p95, при цели 40. Обрыв соединения с базой посреди группы и зависший Valkey процесс не роняют.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 7: Сервер — сжатие журнала

Спека §7.4, риск 2 раздела 14. Снимок и журнал становятся контейнером, запись и удаление вошедших — одной транзакцией.

**Files:**
- Create: `apps/server/src/sync/compact.ts`, `apps/server/src/sync/compact.worker.ts`, `apps/server/src/sync/compactor.ts`
- Modify: `apps/server/src/api.ts`, `apps/server/src/app.ts`, `apps/server/src/file-routes.ts`, `apps/server/src/index.ts`, `apps/server/src/sync/hub.ts`
- Test: `apps/server/test/access.test.ts`, `apps/server/test/api-stand.ts`, `apps/server/test/compact.test.ts` (новый)

**Interfaces:**
- Consumes: `UpdateLog`, `FsBlobStore`, `ContainerWalker`.
- Produces: `compactFile`, `Compactor`, `POST /api/files/:id/compact`.

- [ ] **Step 1: Написать тесты**

В `apps/server/test/access.test.ts` заменить:

````ts
  'GET /api/files/:id/content': { params: file, allowed: viewer, status: 204 },
  'PUT /api/files/:id/content': { params: file, body: container(), headers: {}, allowed: editor, denied: [viewer] },
  // Документа нет в файле: 404 и у читателя, но это уже после проверки доступа.
  'GET /api/files/:id/docs/:doc/snapshot': { params: (w) => ({ id: w.file, doc: 'root' }), allowed: viewer, status: 404 },
````

на:

````ts
  'GET /api/files/:id/content': { params: file, allowed: viewer, status: 204 },
  'PUT /api/files/:id/content': { params: file, body: container(), headers: {}, allowed: editor, denied: [viewer] },
  'POST /api/files/:id/compact': { params: file, allowed: viewer },
  // Документа нет в файле: 404 и у читателя, но это уже после проверки доступа.
  'GET /api/files/:id/docs/:doc/snapshot': { params: (w) => ({ id: w.file, doc: 'root' }), allowed: viewer, status: 404 },
````

В `apps/server/test/api-stand.ts` заменить:

````ts
  const blobs = new FsBlobStore(dataDir);
  const server = startServer(
    { pool, valkey, webDist: '/nonexistent', auth, publicUrl, trustedProxies: [], blobs, mailer, secret: TEST_SECRET, ...over },
    port,
  );
````

на:

````ts
  const blobs = new FsBlobStore(dataDir);
  const server = startServer(
    {
      pool,
      valkey,
      webDist: '/nonexistent',
      auth,
      publicUrl,
      trustedProxies: [],
      blobs,
      mailer,
      secret: TEST_SECRET,
      ...over,
      ...(over.compact ? { compact: { ...over.compact, init: { ...over.compact.init, dataDir } } } : {}),
    },
    port,
  );
````

В `apps/server/test/api-stand.ts` заменить:

````ts

  const stop = async () => {
    await server.hub.stop();
    server.stop(true);
````

на:

````ts

  const stop = async () => {
    server.compactor?.stop();
    await server.hub.stop();
    server.stop(true);
````

В `apps/server/test/api-stand.ts` заменить:

````ts
    server,
    hub: server.hub,
    call,
    signIn,
````

на:

````ts
    server,
    hub: server.hub,
    compactor: server.compactor,
    call,
    signIn,
````

Создать `apps/server/test/compact.test.ts`:

````ts
// Сжатие (спека подсистемы 4, раздел 7.4): снимок заменяется и вошедшие записи удаляются одной транзакцией; новые записи остаются;
// сбой посередине ничего не теряет; обновление, которое не применилось, не заменяет снимок; клиент догоняет, отставший получает снимок.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { scanContainer } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import { apiStand } from './api-stand';
import type { BlobStore } from '../src/blob-store';
import { compactFile } from '../src/sync/compact';
import { Client, peers, realContainer } from './sync-client';
import { TEST_DATABASE_URL } from './helpers';

const s = await apiStand({
  compact: {
    init: { databaseUrl: TEST_DATABASE_URL, dataDir: '' },
    limits: { maxLogCount: 5, tickMs: 50, idleMs: 100, retryAfterSeconds: 1 },
  },
});
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const editor = await s.signIn('editor@example.com');
let file = '';
let root: LoroDoc;
let pageId = '';
let page: LoroDoc;
const open: Client[] = [];

beforeEach(async () => {
  for (const c of open.splice(0)) c.close();
  await s.reset();
  file = (await s.newFile(owner)).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor')`, [file, editor.id]);
  root = new LoroDoc();
  root.getMap('meta').set('name', 'файл');
  const tree = root.getTree('pages');
  tree.enableFractionalIndex(0);
  const node = tree.createNode();
  node.data.set('name', 'Страница');
  root.commit();
  pageId = String(node.id);
  page = new LoroDoc();
  page.getTree('nodes').enableFractionalIndex(0);
  page.commit();
  const res = await s.writeContent(
    owner,
    file,
    0,
    realContainer(root.export({ mode: 'snapshot' }), [[pageId, page.export({ mode: 'snapshot' })]]),
  );
  expect(res.status).toBe(200);
});

async function joined(doc: string, version: Map<string, number>, who = owner, f = file): Promise<Client> {
  const c = (await Client.open(s, who, f))!;
  open.push(c);
  c.hello();
  c.subscribe(doc, version);
  await c.wait('subscribed');
  return c;
}

/** Пишет `n` правок в журнал страницы, ack каждой. */
async function writes(n: number, d: LoroDoc, c: Client, doc = pageId): Promise<void> {
  const tree = d.getTree('nodes');
  tree.enableFractionalIndex(0);
  for (let i = 0; i < n; i++) {
    const from = d.oplogVersion();
    const node = tree.createNode();
    node.data.set('type', 'rect');
    node.data.set('x', i);
    d.commit();
    c.update(doc, i, d.export({ mode: 'update', from }));
    await c.wait('ack', (f) => f.clientSeq === i);
    c.frames.length = 0;
  }
}

const logRows = async (doc = pageId) =>
  (await s.pool.query<{ seq: string }>('SELECT seq FROM doc_update WHERE file_id = $1 AND doc = $2 ORDER BY seq', [file, doc])).rows.map(
    (r) => Number(r.seq),
  );

const contentSections = async () => {
  const res = await s.call(owner, 'GET', `/api/files/${file}/content`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  return {
    bytes,
    spans: await scanContainer(bytes.length, async (a, b) => bytes.subarray(a, b)),
    version: Number(res.headers.get('x-ferrum-version')),
  };
};

test('снимок заменяется, журнал пустеет, содержимое то же, что у клиента', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(3, d, c);
  const before = await contentSections();
  const r = await compactFile(s.pool, s.blobs, file);
  expect(r).toMatchObject({ kind: 'done', rows: 3 });
  expect(await logRows()).toEqual([]);
  const after = await contentSections();
  expect(after.version).toBe(before.version + 1);
  const span = after.spans.find((x) => x.kind === 2 && x.id === pageId)!;
  const read = LoroDoc.fromSnapshot(after.bytes.subarray(span.data, span.data + span.size));
  expect(read.getTree('nodes').getNodes().length).toBe(3);
  expect(read.oplogVersion().toJSON()).toEqual(d.oplogVersion().toJSON());
  const { rows } = await s.pool.query<{ base_vv: object; log_count: number; log_bytes: string }>(
    'SELECT base_vv, log_count, log_bytes FROM doc_state WHERE file_id = $1 AND doc = $2',
    [file, pageId],
  );
  expect(rows[0].log_count).toBe(0);
  expect(rows[0].log_bytes).toBe('0');
  expect(rows[0].base_vv).toEqual(Object.fromEntries(d.oplogVersion().toJSON()));
});

test('ядро Rust откроет контейнер: секции и контрольные суммы на месте', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(1, d, c);
  await compactFile(s.pool, s.blobs, file);
  const { bytes, spans } = await contentSections();
  const { blake3 } = await import('@noble/hashes/blake3.js');
  for (const x of spans) {
    const sum = bytes.subarray(x.data - 32, x.data);
    expect(sum).toEqual(blake3(bytes.subarray(x.data, x.data + x.size)));
  }
  expect(spans.map((x) => x.kind).sort()).toEqual([1, 2, 3]);
});

test('запись во время сжатия: остаётся в журнале и не теряется', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(2, d, c);
  // Сжатие уже прочитало границу журнала и пишет файл: ровно здесь приходит новая правка.
  const late: BlobStore = {
    ...s.blobs,
    open: (k) => s.blobs.open(k),
    put: (k, b) => s.blobs.put(k, b),
    get: (k) => s.blobs.get(k),
    delete: (k) => s.blobs.delete(k),
    putStream: async (k, body, o) => {
      const from = d.oplogVersion();
      d.getMap('late').set('x', 1);
      d.commit();
      c.update(pageId, 99, d.export({ mode: 'update', from }));
      await c.wait('ack', (f) => f.clientSeq === 99);
      return s.blobs.putStream(k, body, o);
    },
  };
  expect((await compactFile(s.pool, late, file)).kind).toBe('done');
  expect(await logRows()).toEqual([3]);
  const snap = await s.call(owner, 'GET', `/api/files/${file}/docs/${pageId}/snapshot`);
  const merged = LoroDoc.fromSnapshot(new Uint8Array(await snap.arrayBuffer()));
  expect(merged.getMap('late').get('x')).toBeUndefined();
  const again = await joined(pageId, peers(merged));
  for (const f of again.frames) if (f.t === 'updates') merged.import(f.bytes);
  expect(merged.getMap('late').get('x')).toBe(1);
});

test('сбой посреди транзакции: содержимое и журнал целы, байты нового файла уйдут в очистку', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(2, d, c);
  const before = await contentSections();
  const failed = s.failing('doc_update', 'DELETE', () =>
    compactFile(s.pool, s.blobs, file).then(
      () => 'прошло',
      (e: Error) => e.message,
    ),
  );
  expect(await failed).toContain('сбой теста');
  const after = await contentSections();
  expect(after.version).toBe(before.version);
  expect(after.bytes).toEqual(before.bytes);
  expect(await logRows()).toEqual([1, 2]);
  const pending = await s.pool.query('SELECT key FROM blob_pending');
  expect(pending.rows).toHaveLength(1);
  // Следующая попытка удаётся.
  expect((await compactFile(s.pool, s.blobs, file)).kind).toBe('done');
});

test('обновление не применилось: снимок не заменён, журнал цел, seq назван, приём правок идёт', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(2, d, c);
  await s.pool.query('UPDATE doc_update SET bytes = $3 WHERE file_id = $1 AND seq = $2', [file, 2, Buffer.from([1, 2, 3])]);
  const before = await contentSections();
  const r = await compactFile(s.pool, s.blobs, file);
  expect(r).toEqual({ kind: 'failed', doc: pageId, seq: 2 });
  expect((await contentSections()).version).toBe(before.version);
  expect(await logRows()).toEqual([1, 2]);
});

test('Compactor сам сжимает по числу записей и после ухода людей; не вышло — повтор через срок', async () => {
  const results: string[] = [];
  s.compactor!.onResult = (_f, r) => results.push(r.ok ? r.result.kind : 'error');
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(8, d, c);
  for (let i = 0; i < 400 && results.length === 0; i++) await Bun.sleep(25);
  expect(results[0]).toBe('done');
  expect((await logRows()).length).toBeLessThan(8);
  // Ушли все: остаток журнала уходит сам.
  c.close();
  await c.waitClosed();
  for (let i = 0; i < 400 && (await logRows()).length > 0; i++) await Bun.sleep(25);
  expect(await logRows()).toEqual([]);
  s.compactor!.onResult = () => {};
});

test('отставший клиент после сжатия: snapshot-needed, снимок и догоняние дают тот же документ', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(3, d, c);
  const r = await compactFile(s.pool, s.blobs, file);
  expect(r.kind).toBe('done');
  await writes(2, d, c);
  const late = (await Client.open(s, editor, file))!;
  open.push(late);
  late.hello();
  late.subscribe(pageId, peers(page));
  const need = await late.wait('snapshot-needed');
  const res = await s.call(editor, 'GET', need.url);
  const mine = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  mine.import(new Uint8Array(await res.arrayBuffer()));
  late.frames.length = 0;
  late.subscribe(pageId, peers(mine));
  await late.wait('subscribed');
  for (const f of late.frames) if (f.t === 'updates') mine.import(f.bytes);
  expect(mine.oplogVersion().toJSON()).toEqual(d.oplogVersion().toJSON());
  expect(mine.getTree('nodes').getNodes().length).toBe(5);
});

test('удалённая страница уходит из контейнера и журнала, картинки считаются по новым ссылкам', async () => {
  const hash = 'a'.repeat(64);
  await s.pool.query(`INSERT INTO image (hash, size, blob_key) VALUES ($1, 1, 'x') ON CONFLICT DO NOTHING`, [hash]);
  await s.pool.query('INSERT INTO file_image (file_id, hash) VALUES ($1, $2)', [file, hash]);
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  // Страница со ссылкой на картинку.
  const tree = d.getTree('nodes');
  tree.enableFractionalIndex(0);
  const from = d.oplogVersion();
  const node = tree.createNode();
  node.data.set('type', 'rect');
  node.data.set('fills', [{ kind: 'image', imageHash: hash }]);
  d.commit();
  c.update(pageId, 1, d.export({ mode: 'update', from }));
  await c.wait('ack');
  await compactFile(s.pool, s.blobs, file);
  let row = await s.pool.query<{ unreferenced_at: Date | null }>('SELECT unreferenced_at FROM file_image WHERE file_id = $1', [file]);
  expect(row.rows[0].unreferenced_at).toBeNull();
  const after = await contentSections();
  const refs = after.spans.find((x) => x.kind === 3 && x.id === pageId)!;
  expect(new TextDecoder().decode(after.bytes.subarray(refs.data, refs.data + refs.size))).toBe(hash);

  // Удаление страницы другим.
  const r = (await Client.open(s, owner, file))!;
  open.push(r);
  r.hello();
  r.subscribe('root', peers(root));
  await r.wait('subscribed');
  const rootTab = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  const treeR = rootTab.getTree('pages');
  const rf = rootTab.oplogVersion();
  const extra = treeR.createNode();
  extra.data.set('name', 'Вторая');
  treeR.delete(treeR.getNodes()[0].id);
  rootTab.commit();
  r.update('root', 1, rootTab.export({ mode: 'update', from: rf }));
  await r.wait('ack');
  expect((await compactFile(s.pool, s.blobs, file)).kind).toBe('done');
  const gone = await contentSections();
  expect(gone.spans.filter((x) => x.kind === 2).map((x) => x.id)).toEqual([String(extra.id)]);
  expect((await s.pool.query(`SELECT 1 FROM doc_state WHERE file_id = $1 AND doc = $2`, [file, pageId])).rows).toHaveLength(0);
  row = await s.pool.query<{ unreferenced_at: Date | null }>('SELECT unreferenced_at FROM file_image WHERE file_id = $1', [file]);
  expect(row.rows[0].unreferenced_at).not.toBeNull();
});

test('новая страница без правок получает пустую секцию: файл не выглядит повреждённым', async () => {
  const c = await joined('root', peers(root));
  const d = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  const from = d.oplogVersion();
  const added = d.getTree('pages').createNode();
  added.data.set('name', 'Вторая');
  d.commit();
  c.update('root', 1, d.export({ mode: 'update', from }));
  await c.wait('ack');
  expect((await compactFile(s.pool, s.blobs, file)).kind).toBe('done');
  const { bytes, spans } = await contentSections();
  const span = spans.find((x) => x.kind === 2 && x.id === String(added.id))!;
  expect(span).toBeTruthy();
  expect(
    LoroDoc.fromSnapshot(bytes.subarray(span.data, span.data + span.size))
      .oplogVersion()
      .toJSON().size,
  ).toBe(0);
});

test('файл сверх предела контейнера не заменяет прежний', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(2, d, c);
  await expect(compactFile(s.pool, s.blobs, file, 16)).rejects.toThrow();
  expect(await logRows()).toEqual([1, 2]);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `npm run test:stand && cd apps/server && bun test test/compact.test.ts`
Expected: FAIL — `Cannot find module '../src/sync/compact'`: 0 pass, 1 fail

- [ ] **Step 3: Реализация**

В `apps/server/src/api.ts` заменить:

````ts
  /** Содержимое файла заменено целиком: проверка пачек забывает его документы. */
  contentReplaced?: (file: string) => void;
}

````

на:

````ts
  /** Содержимое файла заменено целиком: проверка пачек забывает его документы. */
  contentReplaced?: (file: string) => void;
  /** Сжать журнал файла сейчас: копия для скачивания должна быть полной. `false` — не вышло. */
  compactNow?: (file: string) => Promise<boolean>;
}

````

В `apps/server/src/app.ts` заменить:

````ts
import { imageRoutes } from './image-routes';
import { shareRoutes } from './share-routes';
import { SyncHub, type ConnData, type SyncLimits } from './sync/hub';
import { teamRoutes } from './team-routes';
````

на:

````ts
import { imageRoutes } from './image-routes';
import { shareRoutes } from './share-routes';
import type { CompactInit } from './sync/compact.worker';
import { Compactor, type CompactLimits } from './sync/compactor';
import { SyncHub, type ConnData, type SyncLimits } from './sync/hub';
import { teamRoutes } from './team-routes';
````

В `apps/server/src/app.ts` заменить:

````ts
  /** Тесты сокращают пределы и сроки WebSocket. */
  syncLimits?: Partial<SyncLimits>;
}

````

на:

````ts
  /** Тесты сокращают пределы и сроки WebSocket. */
  syncLimits?: Partial<SyncLimits>;
  /** Откуда worker сжатия берёт базу и байты; нет — сжатия нет (тесты другого). */
  compact?: { init: CompactInit; limits?: Partial<CompactLimits> };
}

````

В `apps/server/src/app.ts` заменить:

````ts

/** Сервер и его WebSocket: остановка контейнера и тесты обращаются к обоим. */
export type Running = Bun.Server<ConnData> & { hub: SyncHub };

export function startServer(deps: AppDeps, port: number): Running {
  const hub = new SyncHub(deps.pool, deps.blobs, deps.auth, deps.publicUrl, deps.syncLimits);
  const handleAuth = authRoutes({ ...deps, hub });
  const withHeaders = responseHeaders(deps.publicUrl);
````

на:

````ts

/** Сервер и его WebSocket: остановка контейнера и тесты обращаются к обоим. */
export type Running = Bun.Server<ConnData> & { hub: SyncHub; compactor: Compactor | null };

export function startServer(deps: AppDeps, port: number): Running {
  const hub = new SyncHub(deps.pool, deps.blobs, deps.auth, deps.publicUrl, deps.syncLimits);
  const compactor = deps.compact ? new Compactor(deps.pool, deps.compact.init, deps.compact.limits) : null;
  if (compactor) {
    hub.onLog = (file, _doc, count, bytes) => compactor.note(file, count, bytes);
    hub.onLastLeft = (file) => compactor.lastLeft(file);
    compactor.active = (file) => hub.isOpen(file);
  }
  const handleAuth = authRoutes({ ...deps, hub });
  const withHeaders = responseHeaders(deps.publicUrl);
````

В `apps/server/src/app.ts` заменить:

````ts
    afterWrite: (route, ctx) => accessChanged(hub, route, ctx),
    contentReplaced: (file) => hub.forgetFile(file),
  };
  const handleApi = apiHandler(apiRoutes(api), api);
````

на:

````ts
    afterWrite: (route, ctx) => accessChanged(hub, route, ctx),
    contentReplaced: (file) => hub.forgetFile(file),
    compactNow: (file) => (compactor ? compactor.compactNow(file) : Promise.resolve(true)),
  };
  const handleApi = apiHandler(apiRoutes(api), api);
````

В `apps/server/src/app.ts` заменить:

````ts
    websocket: hub.websocket,
  });
  return Object.assign(server, { hub });
}
````

на:

````ts
    websocket: hub.websocket,
  });
  return Object.assign(server, { hub, compactor });
}
````

В `apps/server/src/file-routes.ts` заменить:

````ts
    },
    {
      // Клиент отстал сильнее журнала (спека, раздел 7.3): снимок одного документа, поверх него — журнал.
      method: 'GET',
````

на:

````ts
    },
    {
      // «Скачать копию» с экрана файлов: содержимое без журнала было бы старше того, что видят люди. Читателю тоже: экспорт ему
      // разрешён. Сжатие занимает секунды на большом файле, поэтому общего срока нет.
      method: 'POST',
      path: '/api/files/:id/compact',
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      stream: true,
      handle: async (ctx) => {
        const done = deps.compactNow ? await deps.compactNow(ctx.params.id) : true;
        return done ? ok({ ok: true }) : apiError(503, 'UNAVAILABLE');
      },
    },
    {
      // Клиент отстал сильнее журнала (спека, раздел 7.3): снимок одного документа, поверх него — журнал.
      method: 'GET',
````

В `apps/server/src/index.ts` заменить:

````ts
    allowedEmails: config.allowedEmails,
    secret: config.authSecret,
  },
  config.port,
````

на:

````ts
    allowedEmails: config.allowedEmails,
    secret: config.authSecret,
    compact: { init: { databaseUrl: config.databaseUrl, dataDir: config.dataDir } },
  },
  config.port,
````

В `apps/server/src/index.ts` заменить:

````ts
stop = () => {
  stopGc();
  // Принятое дописывается, соединения закрываются (клиенты подключатся заново): `server.stop()` без `true`
  // ждал бы их, и контейнер дождался бы SIGKILL (B-207).
  void server.hub
    .stop()
````

на:

````ts
stop = () => {
  stopGc();
  server.compactor?.stop();
  // Принятое дописывается, соединения закрываются (клиенты подключатся заново): `server.stop()` без `true`
  // ждал бы их, и контейнер дождался бы SIGKILL.
  void server.hub
    .stop()
````

Создать `apps/server/src/sync/compact.ts`:

````ts
// Сжатие файла (спека подсистемы 4, раздел 7.4): снимок и журнал документов становятся новым контейнером одной транзакцией
// с удалением вошедших записей. Сбой посередине ничего не теряет: прежнее содержимое и журнал остаются. Обновление, которое не
// применилось, не удаляется и файл не заменяет; следующая попытка через час, приём правок при этом идёт дальше.
//
// Функция без глобального состояния: её вызывает `compact.worker.ts` (Bun Worker, память wasm не возвращается системе) и тесты.

import { MAX_CONTENT_BYTES, ContainerError, containerHeader, scanContainer, SECTION_IMAGE, SECTION_PAGE_REFS } from '@ferrum/protocol';
import { blake3 } from '@noble/hashes/blake3.js';
import { LoroDoc, type LoroMap } from 'loro-crdt';
import { randomUUID } from 'node:crypto';
import type { Pool } from 'pg';
import type { BlobStore } from '../blob-store';
import { inTransaction } from '../db';

const SECTION_ROOT = 1;
const SECTION_PAGE = 2;
const SECTION_SCENE = 5;
/** Записей журнала за один запрос: байты идут в wasm и сразу освобождаются. */
const ROWS_AT_ONCE = 256;

export type CompactResult =
  | { kind: 'done'; docs: number; size: number; rows: number }
  /** Журнал пуст: сжимать нечего. */
  | { kind: 'empty' }
  /** Обновление не применилось; файл не тронут. */
  | { kind: 'failed'; doc: string; seq: number }
  /** Файл заменили, пока шла работа. Журнал цел. */
  | { kind: 'raced' };

interface Section {
  kind: number;
  id: string;
  /** Откуда копировать без чтения в память; `null` — новые данные. */
  from: { start: number; end: number } | null;
  data: Uint8Array | null;
}

interface DocRow {
  doc: string;
  next_seq: string;
  log_count: number;
}

function imageRefs(doc: LoroDoc): string[] {
  const refs = new Set<string>();
  const tree = doc.getTree('nodes');
  for (const node of tree.getNodes({ withDeleted: false })) {
    const fills = (node.data as LoroMap).get('fills');
    if (!Array.isArray(fills)) continue;
    for (const f of fills as { imageHash?: unknown }[]) {
      if (typeof f?.imageHash === 'string') refs.add(f.imageHash);
    }
  }
  return [...refs].sort();
}

function pageIds(root: LoroDoc): { listed: Set<string>; gone: Set<string> } {
  const tree = root.getTree('pages');
  const listed = new Set<string>(tree.roots().map((n) => String(n.id)));
  const gone = new Set<string>();
  for (const n of tree.getNodes({ withDeleted: true })) {
    if (tree.isNodeDeleted(n.id)) gone.add(String(n.id));
  }
  return { listed, gone };
}

const plain = (d: LoroDoc): Record<string, number> => Object.fromEntries(d.oplogVersion().toJSON()) as Record<string, number>;

/** Сжать файл. Журнал читается до `next_seq` на момент начала: новые записи остаются на следующий раз. */
export async function compactFile(pool: Pool, blobs: BlobStore, file: string, maxContent = MAX_CONTENT_BYTES): Promise<CompactResult> {
  const fileRow = await pool.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [file]);
  const contentKey = fileRow.rows[0]?.content_key ?? null;
  if (!fileRow.rows[0]) return { kind: 'empty' };
  const { rows: states } = await pool.query<DocRow>('SELECT doc, next_seq, log_count FROM doc_state WHERE file_id = $1', [file]);
  const logged = states.filter((s) => s.log_count > 0);
  if (logged.length === 0) return { kind: 'empty' };

  const blob = contentKey ? await blobs.open(contentKey) : null;
  if (contentKey && !blob) throw new Error(`нет байтов содержимого файла ${file}`);
  const read = async (s: number, e: number) => new Uint8Array(await blob!.slice(s, e).arrayBuffer());
  let spans: Awaited<ReturnType<typeof scanContainer>> = [];
  if (blob) {
    try {
      spans = await scanContainer(blob.size, read);
    } catch (e) {
      if (!(e instanceof ContainerError)) throw e;
      throw new Error(`содержимое файла ${file} не контейнер: ${e.message}`);
    }
  }
  const dataOf = async (kind: number, id: string): Promise<Uint8Array | null> => {
    const s = spans.find((x) => x.kind === kind && x.id === id);
    return s ? read(s.data, s.data + s.size) : null;
  };

  /** Документ со снимком и журналом до `cut`. Не применилось — `failed`, файл не трогаем. */
  const build = async (
    doc: string,
    kind: number,
    id: string,
    cut: number,
  ): Promise<{ loro: LoroDoc; rows: number } | { doc: string; seq: number }> => {
    const snapshot = await dataOf(kind, id);
    const loro = snapshot && snapshot.length > 0 ? LoroDoc.fromSnapshot(snapshot) : new LoroDoc();
    let rows = 0;
    let after = 0;
    for (;;) {
      const page = await pool.query<{ seq: string; bytes: Uint8Array }>(
        'SELECT seq, bytes FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq > $3 AND seq <= $4 ORDER BY seq LIMIT $5',
        [file, doc, after, cut, ROWS_AT_ONCE],
      );
      if (page.rows.length === 0) break;
      for (const r of page.rows) {
        after = Number(r.seq);
        try {
          const status = loro.import(r.bytes);
          if (status.pending != null) return { doc, seq: after };
        } catch {
          return { doc, seq: after };
        }
        rows++;
      }
    }
    return { loro, rows };
  };

  const stateOf = new Map(states.map((s) => [s.doc, s]));
  const cutOf = (doc: string): number => Number(stateOf.get(doc)?.next_seq ?? 1) - 1;

  const rootBuilt = await build('root', SECTION_ROOT, '', cutOf('root'));
  if (!('loro' in rootBuilt)) return { kind: 'failed', ...rootBuilt };
  const { listed, gone } = pageIds(rootBuilt.loro);
  let totalRows = rootBuilt.rows;

  const replaced = new Map<string, { snapshot: Uint8Array; refs: string[]; version: Record<string, number> }>();
  for (const s of logged) {
    if (s.doc === 'root' || !listed.has(s.doc)) continue;
    const built = await build(s.doc, SECTION_PAGE, s.doc, cutOf(s.doc));
    if (!('loro' in built)) return { kind: 'failed', ...built };
    totalRows += built.rows;
    replaced.set(s.doc, { snapshot: built.loro.export({ mode: 'snapshot' }), refs: imageRefs(built.loro), version: plain(built.loro) });
    built.loro.free();
  }
  const rootVersion = plain(rootBuilt.loro);
  const rootSnapshot = rootBuilt.loro.export({ mode: 'snapshot' });
  rootBuilt.loro.free();

  // Секции нового контейнера: неизменённые — копией куска прежнего файла, в памяти только новые снимки.
  const sections: Section[] = [{ kind: SECTION_ROOT, id: '', from: null, data: rootSnapshot }];
  for (const s of spans) {
    const copy = { start: s.start, end: s.data + s.size };
    if (s.kind === SECTION_ROOT || s.kind === SECTION_IMAGE) continue;
    // Удалённая страница уходит; страница, которой корень ещё не знает, остаётся: корень мог не дойти.
    if ((s.kind === SECTION_PAGE || s.kind === SECTION_PAGE_REFS || s.kind === SECTION_SCENE) && (gone.has(s.id) || replaced.has(s.id)))
      continue;
    sections.push({ kind: s.kind, id: s.id, from: copy, data: null });
  }
  // Страница без единой правки (только что добавлена): ни журнала, ни секции. Ядро приняло бы отсутствие секции за повреждение файла.
  for (const id of listed) {
    if (replaced.has(id) || spans.some((x) => x.kind === SECTION_PAGE && x.id === id)) continue;
    sections.push({ kind: SECTION_PAGE, id, from: null, data: new LoroDoc().export({ mode: 'snapshot' }) });
    sections.push({ kind: SECTION_PAGE_REFS, id, from: null, data: new Uint8Array(0) });
  }
  for (const [id, r] of replaced) {
    sections.push({ kind: SECTION_PAGE, id, from: null, data: r.snapshot });
    sections.push({ kind: SECTION_PAGE_REFS, id, from: null, data: new TextEncoder().encode(r.refs.join('\n')) });
  }
  const refs = new Set<string>();
  for (const r of replaced.values()) for (const h of r.refs) refs.add(h);
  for (const s of spans) {
    if (s.kind !== SECTION_PAGE_REFS || replaced.has(s.id) || gone.has(s.id)) continue;
    for (const line of new TextDecoder().decode(await read(s.data, s.data + s.size)).split('\n')) if (line) refs.add(line);
  }

  const key = `files/${file}/content-${randomUUID()}`;
  await pool.query('INSERT INTO blob_pending (key) VALUES ($1) ON CONFLICT DO NOTHING', [key]);
  const head = containerHeader(sections.length);
  const body = new ReadableStream<Uint8Array>({
    async start(c) {
      c.enqueue(head);
      for (const s of sections) {
        if (s.from) c.enqueue(await read(s.from.start, s.from.end));
        else c.enqueue(sectionBytes(s.kind, s.id, s.data!));
      }
      c.close();
    },
  });
  const { size } = await blobs.putStream(key, body, { maxBytes: maxContent, idleMs: 60_000 });

  const outcome = await inTransaction(pool, async (c) => {
    const locked = await c.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1 FOR UPDATE', [file]);
    if (!locked.rows[0] || locked.rows[0].content_key !== contentKey) return 'raced' as const;
    const pending = await c.query('DELETE FROM blob_pending WHERE key = $1', [key]);
    if (pending.rowCount === 0) return 'raced' as const;
    if (contentKey) {
      await c.query(`INSERT INTO blob_trash (key, delete_after) VALUES ($1, now() + interval '2 minutes') ON CONFLICT DO NOTHING`, [
        contentKey,
      ]);
    }
    await c.query('UPDATE file SET version = version + 1, size = $2, content_key = $3, compact_after = NULL WHERE id = $1', [
      file,
      size,
      key,
    ]);
    // Вошедшие записи — той же транзакцией: записи новее `cut` остаются, журнал и снимок не расходятся.
    const done = [{ doc: 'root', version: rootVersion }, ...[...replaced].map(([doc, r]) => ({ doc, version: r.version }))];
    for (const d of done) {
      if (!stateOf.has(d.doc)) continue;
      const removed = await c.query<{ size: number }>(
        'DELETE FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq <= $3 RETURNING octet_length(bytes) AS size',
        [file, d.doc, cutOf(d.doc)],
      );
      await c.query(
        `UPDATE doc_state SET base_vv = $3::jsonb, log_count = log_count - $4, log_bytes = log_bytes - $5 WHERE file_id = $1 AND doc = $2`,
        [file, d.doc, JSON.stringify(d.version), removed.rowCount ?? 0, removed.rows.reduce((n, r) => n + r.size, 0)],
      );
    }
    for (const doc of gone) {
      await c.query('DELETE FROM doc_state WHERE file_id = $1 AND doc = $2', [file, doc]);
    }
    await c.query(
      `UPDATE file_image SET unreferenced_at = CASE WHEN hash = ANY($2::text[]) THEN NULL ELSE COALESCE(unreferenced_at, now()) END WHERE file_id = $1`,
      [file, [...refs]],
    );
    return 'done' as const;
  });
  if (outcome === 'raced') {
    await blobs.delete(key).catch(() => {});
    await pool.query('DELETE FROM blob_pending WHERE key = $1', [key]).catch(() => {});
    return { kind: 'raced' };
  }
  return { kind: 'done', docs: replaced.size + 1, size, rows: totalRows };
}

function sectionBytes(kind: number, id: string, data: Uint8Array): Uint8Array {
  const idb = new TextEncoder().encode(id);
  const out = new Uint8Array(5 + idb.length + 40 + data.length);
  const v = new DataView(out.buffer);
  out[0] = kind;
  v.setUint32(1, idb.length, true);
  out.set(idb, 5);
  v.setBigUint64(5 + idb.length, BigInt(data.length), true);
  out.set(blake3(data), 13 + idb.length);
  out.set(data, 45 + idb.length);
  return out;
}
````

Создать `apps/server/src/sync/compact.worker.ts`:

````ts
// Bun Worker сжатия (спека подсистемы 4, раздел 7.4): по одному файлу за раз. Своё соединение с базой и свой wasm: основной поток
// от паники и памяти Loro отделён, а `Compactor` пересоздаёт worker каждые 50 задач.

import { FsBlobStore } from '../blob-store';
import { createPool } from '../db';
import { compactFile, type CompactResult } from './compact';

export interface CompactJob {
  id: number;
  file: string;
}

export type CompactReply = { id: number; ok: true; result: CompactResult } | { id: number; ok: false; message: string };

export interface CompactInit {
  databaseUrl: string;
  dataDir: string;
  maxContent?: number;
}

const scope = globalThis as unknown as Worker;
let deps: { pool: ReturnType<typeof createPool>; blobs: FsBlobStore; maxContent?: number } | null = null;

function ready(c: CompactInit) {
  deps ??= {
    pool: createPool(c.databaseUrl, { max: 2, statement_timeout: 120_000, query_timeout: 125_000 }),
    blobs: new FsBlobStore(c.dataDir),
    maxContent: c.maxContent,
  };
  return deps;
}

scope.onmessage = (e: MessageEvent<CompactJob & { init: CompactInit }>) => {
  const { id, file, init: given } = e.data;
  const d = ready(given);
  compactFile(d.pool, d.blobs, file, d.maxContent).then(
    (result) => scope.postMessage({ id, ok: true, result } satisfies CompactReply),
    (err: unknown) =>
      scope.postMessage({ id, ok: false, message: err instanceof Error ? err.message : String(err) } satisfies CompactReply),
  );
};
````

Создать `apps/server/src/sync/compactor.ts`:

````ts
// Когда сжимать (спека подсистемы 4, раздел 7.4): больше 2000 записей или 32 МБ журнала документа, либо через 5 минут после ухода
// последнего участника. Файлы — по одному; worker пересоздаётся каждые 50 задач. Не вышло — журнал цел, строка в журнале сервера,
// следующая попытка через час.

import type { Pool } from 'pg';
import type { CompactInit, CompactJob, CompactReply } from './compact.worker';

export interface CompactLimits {
  maxLogCount: number;
  maxLogBytes: number;
  idleMs: number;
  tickMs: number;
  retryAfterSeconds: number;
  tasksPerWorker: number;
  jobDeadlineMs: number;
}

export const COMPACT_LIMITS: CompactLimits = {
  maxLogCount: 2000,
  maxLogBytes: 32 * 1024 * 1024,
  idleMs: 5 * 60_000,
  tickMs: 30_000,
  retryAfterSeconds: 3600,
  tasksPerWorker: 50,
  jobDeadlineMs: 10 * 60_000,
};

export class Compactor {
  private worker: Worker | null = null;
  private tasks = 0;
  private seq = 0;
  private running: { id: number; file: string; done: () => void; timer: Timer } | null = null;
  private readonly queue = new Set<string>();
  private readonly waiters = new Map<string, ((ok: boolean) => void)[]>();
  private readonly leftAt = new Map<string, number>();
  private readonly timer: Timer;
  private stopped = false;
  private readonly limits: CompactLimits;
  /** Занят ли файл людьми: сжатие простаивающих не трогает открытые. */
  active: (file: string) => boolean = () => false;
  /** Тесты: итог каждой задачи. */
  onResult: (file: string, r: CompactReply) => void = () => {};

  constructor(
    private readonly pool: Pool,
    private readonly init: CompactInit,
    limits: Partial<CompactLimits> = {},
  ) {
    this.limits = { ...COMPACT_LIMITS, ...limits };
    this.timer = setInterval(() => void this.tick().catch(report), this.limits.tickMs);
    this.timer.unref?.();
  }

  /** После фиксации журнала: документ перерос предел. */
  note(file: string, logCount: number, logBytes: number): void {
    if (logCount > this.limits.maxLogCount || logBytes > this.limits.maxLogBytes) this.enqueue(file);
  }

  lastLeft(file: string): void {
    this.leftAt.set(file, performance.now());
  }

  /** Файлы с журналом без людей дольше срока, в том числе от прошлого запуска. */
  private async tick(): Promise<void> {
    if (this.stopped) return;
    const { rows } = await this.pool.query<{ file_id: string }>(
      `SELECT DISTINCT s.file_id FROM doc_state s JOIN file f ON f.id = s.file_id
       WHERE s.log_count > 0 AND (f.compact_after IS NULL OR f.compact_after <= now()) LIMIT 100`,
    );
    const now = performance.now();
    for (const { file_id: file } of rows) {
      if (this.active(file)) continue;
      const left = this.leftAt.get(file);
      if (left !== undefined && now - left < this.limits.idleMs) continue;
      this.leftAt.delete(file);
      this.enqueue(file);
    }
  }

  /** Сжать сейчас и дождаться: «Скачать копию» отдаёт файл целиком, а не снимок без журнала. Не вышло — `false`, журнал цел. */
  compactNow(file: string): Promise<boolean> {
    return new Promise((resolve) => {
      const list = this.waiters.get(file) ?? [];
      list.push(resolve);
      this.waiters.set(file, list);
      this.enqueue(file);
    });
  }

  private settle(file: string, ok: boolean): void {
    const list = this.waiters.get(file);
    this.waiters.delete(file);
    for (const w of list ?? []) w(ok);
  }

  enqueue(file: string): void {
    if (this.stopped) {
      this.settle(file, false);
      return;
    }
    this.queue.add(file);
    this.pump();
  }

  private pump(): void {
    if (this.running || this.queue.size === 0) return;
    const file = this.queue.values().next().value as string;
    this.queue.delete(file);
    void this.run(file).catch(report);
  }

  private spawn(): Worker {
    const w = new Worker(new URL('./compact.worker.ts', import.meta.url));
    w.unref?.();
    return w;
  }

  private async run(file: string): Promise<void> {
    const due = await this.pool.query<{ waits: boolean }>('SELECT compact_after > now() AS waits FROM file WHERE id = $1', [file]);
    if (due.rows[0]?.waits) {
      this.settle(file, false);
      this.pump();
      return;
    }
    if (this.tasks >= this.limits.tasksPerWorker) this.dropWorker();
    this.worker ??= this.spawn();
    this.tasks++;
    const worker = this.worker;
    const id = ++this.seq;
    await new Promise<void>((done) => {
      const timer = setTimeout(() => {
        console.error('Сжатие: файл', file, 'не уложился в срок');
        this.dropWorker();
        this.settle(file, false);
        void this.failed(file).finally(done);
      }, this.limits.jobDeadlineMs);
      timer.unref?.();
      this.running = { id, file, done, timer };
      worker.onmessage = (e: MessageEvent<CompactReply>) => {
        if (e.data.id !== id) return;
        clearTimeout(timer);
        this.finish(file, e.data).finally(done);
      };
      worker.onerror = (e) => {
        e.preventDefault?.();
        clearTimeout(timer);
        console.error('Сжатие: worker упал на файле', file, e.message);
        this.dropWorker();
        const reply: CompactReply = { id, ok: false, message: e.message };
        this.finish(file, reply).finally(done);
      };
      worker.postMessage({ id, file, init: this.init } satisfies CompactJob & { init: CompactInit });
    });
    this.running = null;
    this.pump();
  }

  private async finish(file: string, r: CompactReply): Promise<void> {
    this.onResult(file, r);
    this.settle(file, r.ok && (r.result.kind === 'done' || r.result.kind === 'empty'));
    if (r.ok && (r.result.kind === 'done' || r.result.kind === 'empty' || r.result.kind === 'raced')) return;
    if (r.ok && r.result.kind === 'failed') {
      console.error(
        `Сжатие: файл ${file}, документ ${r.result.doc}: не применилось обновление ${r.result.seq}. Снимок не заменён, журнал цел`,
      );
    } else if (!r.ok) {
      console.error(`Сжатие: файл ${file}:`, r.message);
    }
    await this.failed(file);
  }

  private async failed(file: string): Promise<void> {
    await this.pool
      .query(`UPDATE file SET compact_after = now() + make_interval(secs => $2) WHERE id = $1`, [file, this.limits.retryAfterSeconds])
      .catch(report);
  }

  private dropWorker(): void {
    this.worker?.terminate();
    this.worker = null;
    this.tasks = 0;
  }

  stop(): void {
    this.stopped = true;
    for (const file of this.waiters.keys()) this.settle(file, false);
    clearInterval(this.timer);
    if (this.running) {
      clearTimeout(this.running.timer);
      this.running.done();
    }
    this.dropWorker();
  }
}

function report(e: unknown): void {
  console.error('Сжатие:', e instanceof Error ? e.message : e);
}
````

В `apps/server/src/sync/hub.ts` заменить:

````ts
  }

  /** Сколько байт ждут в буферах сокетов: тесты и проба медленного читателя. */
  bufferedBytes(): number {
````

на:

````ts
  }

  /** Открыт ли файл у кого-нибудь. */
  isOpen(file: string): boolean {
    return (this.conns.get(file)?.size ?? 0) > 0;
  }

  /** Сколько байт ждут в буферах сокетов: тесты и проба медленного читателя. */
  bufferedBytes(): number {
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `npm run test:server`

Expected: PASS: всё без замечаний; `bun test` 23 и 414 pass

- [ ] **Step 5: Закоммитить**

```bash
git add apps/server/src/api.ts apps/server/src/app.ts apps/server/src/file-routes.ts apps/server/src/index.ts apps/server/src/sync/compact.ts apps/server/src/sync/compact.worker.ts apps/server/src/sync/compactor.ts apps/server/src/sync/hub.ts apps/server/test/access.test.ts apps/server/test/api-stand.ts apps/server/test/compact.test.ts
git commit -F - <<'EOF'
сервер: сжатие журнала

`compactFile`: снимок и журнал документов становятся новым контейнером, запись о нём и удаление вошедших записей — одной
транзакцией. Новые записи (после `next_seq` на начало) остаются. Неизменённые страницы копируются куском файла, кэш сцены у
них остаётся верным. Удалённые страницы уходят, ссылки на картинки считаются заново. Неприменимое обновление (спека §7.4): снимок
не заменён, журнал цел, в журнале сервера `seq`, следующая попытка через час. `Compactor`: больше 2000 записей или 32 МБ журнала
документа либо 5 минут после ухода последнего; в том числе остаток от прошлого запуска; файл по одному, Bun Worker
пересоздаётся каждые 50 задач.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 8: Клиент — соединение файла в своём worker'е

Спека §7.1. `sync-client.ts` без DOM, прямой порт с worker'ом ядра, переподключение от 0,5 до 30 с с разбросом, пауза не считается простоем, сокет копит до 1 МБ, отправка до 32 МБ.

**Files:**
- Create: `apps/web/src/core/sync-client.ts`, `apps/web/src/core/sync-messages.ts`, `apps/web/src/worker/sync.worker.ts`
- Modify: `apps/web/src/core/core.ts`, `apps/web/src/core/messages.ts`, `apps/web/src/core/server-files.ts`, `apps/web/src/core/server-messages.ts`, `apps/web/src/worker/core.worker.ts`, `apps/web/src/worker/server.worker.ts`
- Test: `apps/web/e2e-server/image-limits.spec.ts`, `apps/web/unit/server-files.test.ts`, `apps/web/unit/sync-client.test.ts` (новый)

**Interfaces:**
- Consumes: команды и события задачи 3, кадры задачи 4, `WS /api/files/:id/sync`.
- Produces: `SyncClient`, `sync.worker.ts`, `ServerFiles.{uploadOneImage, downloadOneImage, fetchSnapshot, accessOnce, cutJournal, putThumb}`, `JournalStore.cutExact`.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e-server/image-limits.spec.ts` заменить:

````ts
  read: async () => ({ entries: [], session: null, tooLarge: false }),
  cut: async () => {},
  drop: async () => {},
};
````

на:

````ts
  read: async () => ({ entries: [], session: null, tooLarge: false }),
  cut: async () => {},
  cutExact: async () => {},
  drop: async () => {},
};
````

В `apps/web/unit/server-files.test.ts` заменить:

````ts
    },
    async read() {
      return { entries: this.entries, session: this.session, tooLarge: false };
    },
    async cut(f: string, s: number, u: number) {
      this.cuts.push([f, s, u]);
````

на:

````ts
    },
    async read() {
      return { entries: this.entries, session: this.session, tooLarge: false, keys: this.keys };
    },
    exact: [] as unknown[][],
    async cutExact(keys: readonly unknown[]) {
      this.exact.push([...keys]);
    },
    keys: undefined as [string, number, number][] | undefined,
    async cut(f: string, s: number, u: number) {
      this.cuts.push([f, s, u]);
````

В `apps/web/unit/server-files.test.ts` заменить:

````ts
  expect(failure).toMatchObject({ code: 'TOO_MANY_REQUESTS' });
});
````

на:

````ts
  expect(failure).toMatchObject({ code: 'TOO_MANY_REQUESTS' });
});

test('журнал сервера: подтверждение снимает прочитанное при открытии и своё, а пачки другой вкладки остаются', async () => {
  const s = stand({
    'GET /api/files/f': json(meta({ id: 'f' })),
    'GET /api/files/f/content': new Response(container(ROOT), { headers: { 'x-ferrum-version': '1' } }),
  });
  s.journal.entries = [new ArrayBuffer(1), new ArrayBuffer(1)];
  s.journal.session = 7;
  s.journal.keys = [
    ['f', 5, 1],
    ['f', 7, 3],
  ];
  await s.files.read('f');
  await s.files.cutJournal('f', 100, 0);
  expect(s.journal.exact[0]).toEqual([
    ['f', 5, 1],
    ['f', 7, 3],
  ]);
  // Прочитанное снято один раз; новые пачки другой вкладки в этот список не попадают.
  await s.files.cutJournal('f', 100, 0);
  expect(s.journal.exact).toHaveLength(1);
  await s.files.cutJournal('f', 100, 2);
  expect(s.journal.exact[1]).toEqual([
    ['f', 100, 1],
    ['f', 100, 2],
  ]);
});

test('журнал читателя подтверждение не снимает: он не применён и ждёт, пока вернут право', async () => {
  const s = stand({
    'GET /api/files/v': json(meta({ id: 'v', role: 'viewer' })),
    'GET /api/files/v/content': new Response(container(ROOT), { headers: { 'x-ferrum-version': '1' } }),
  });
  s.journal.entries = [new ArrayBuffer(1)];
  s.journal.session = 7;
  s.journal.keys = [['v', 7, 1]];
  await s.files.read('v');
  await s.files.cutJournal('v', 100, 0);
  expect(s.journal.exact).toEqual([]);
});
````

Создать `apps/web/unit/sync-client.test.ts`:

````ts
// Соединение файла: обрыв, пауза, отказы и порядок, без настоящего сервера. Сокет — настоящий по договору браузера: `send` копит в
// `bufferedAmount`, пока «сеть» не вычитает; часы — ручные: ничего не ждёт настоящего времени, и видно, какой таймер идёт во время паузы.

import { expect, test } from '@playwright/test';
import { encodeServerFrame, encodeVersion, parseClientFrame, SYNC_CLOSE, type ClientFrame, type ServerFrame } from '@ferrum/protocol';
import {
  PING_EVERY_MS,
  PONG_WAIT_MS,
  RECONNECT_MAX_MS,
  SyncClient,
  type CoreCommand,
  type SocketLike,
  type SyncEnv,
} from '../src/core/sync-client';
import type { AccessState } from '../src/core/server-files';

class FakeSocket implements SocketLike {
  binaryType = '';
  onopen: ((e: unknown) => void) | null = null;
  onmessage: ((e: { data: unknown }) => void) | null = null;
  onclose: ((e: { code: number }) => void) | null = null;
  onerror: ((e: unknown) => void) | null = null;
  /** Ушедшие кадры по порядку. */
  sent: ClientFrame[] = [];
  queued: Uint8Array[] = [];
  closedCode: number | null = null;
  get bufferedAmount(): number {
    return this.queued.reduce((n, b) => n + b.length, 0);
  }
  send(bytes: Uint8Array): void {
    if (this.closedCode !== null) throw new Error('сокет закрыт');
    this.queued.push(bytes);
  }
  /** Сеть вычитала всё. */
  drain(): void {
    for (const b of this.queued) {
      const f = parseClientFrame(b);
      if (!f) throw new Error('клиент послал неверный кадр');
      this.sent.push(f);
    }
    this.queued = [];
  }
  close(code = 1000): void {
    this.closedCode = code;
  }
  open(): void {
    this.onopen?.({});
  }
  receive(f: ServerFrame): void {
    this.onmessage?.({ data: encodeServerFrame(f).buffer });
  }
  dropped(code = 1006): void {
    this.closedCode = code;
    this.onclose?.({ code });
  }
}

class Clock {
  t = 0;
  private seq = 0;
  timers = new Map<number, { at: number; fn: () => void; ms: number }>();
  now = () => this.t;
  setTimeout = (fn: () => void, ms: number) => {
    const id = ++this.seq;
    this.timers.set(id, { at: this.t + ms, fn, ms });
    return id;
  };
  clearTimeout = (id: unknown) => void this.timers.delete(id as number);
  advance(ms: number): void {
    const end = this.t + ms;
    for (;;) {
      const due = [...this.timers.entries()].filter(([, x]) => x.at <= end).sort(([, a], [, b]) => a.at - b.at)[0];
      if (!due) break;
      this.timers.delete(due[0]);
      this.t = Math.max(this.t, due[1].at);
      due[1].fn();
    }
    this.t = end;
  }
}

function stand(over: Partial<SyncEnv['files']> = {}) {
  const clock = new Clock();
  const sockets: FakeSocket[] = [];
  const commands: CoreCommand[] = [];
  const flushes: boolean[] = [];
  const statuses: { status: string; detail?: string }[] = [];
  const access: AccessState[] = [];
  let reloads = 0;
  const env: SyncEnv = {
    file: 'f',
    user: 'u',
    schema: 1,
    origin: 'https://ferrum.example',
    socket: (url) => {
      expect(url).toBe('wss://ferrum.example/api/files/f/sync?user=u');
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    core: {
      command: (c) => commands.push(c),
      flush: (on) => flushes.push(on),
    },
    files: {
      uploadImage: () => Promise.resolve(),
      downloadImage: () => Promise.resolve(null),
      snapshot: () => Promise.resolve(new ArrayBuffer(1)),
      access: () => Promise.resolve({ kind: 'role', role: 'editor', version: 1, canShare: true, own: true }),
      ...over,
    },
    clock,
    random: () => 1,
    status: (status, detail) => statuses.push({ status, detail }),
    access: (s) => access.push(s),
    reload: () => reloads++,
  };
  const client = new SyncClient(env);
  return {
    client,
    clock,
    sockets,
    commands,
    flushes,
    statuses,
    access,
    reloads: () => reloads,
    last: () => sockets.at(-1)!,
    status: () => statuses.at(-1)?.status,
    flushing: () => flushes.at(-1),
    live(): FakeSocket {
      client.start();
      const s = sockets.at(-1)!;
      s.open();
      s.receive({ t: 'subscribed', doc: 'root', seq: 0, version: encodeVersion(new Map()) });
      return s;
    },
  };
}

const update = (clientSeq: number, size = 3) => ({
  type: 'localUpdates' as const,
  doc: 'root',
  clientSeq,
  bytes: new Uint8Array(size).fill(1),
});
const tick = () => new Promise((r) => setTimeout(r, 0));

test('открытие: hello, сброс курсора, ядро отдаёт подписки; подтверждение — ядру', () => {
  const s = stand();
  s.client.start();
  expect(s.commands[0]).toEqual({ type: 'syncStart' });
  expect(s.status()).toBe('connecting');
  const ws = s.last();
  ws.open();
  ws.drain();
  expect(ws.sent[0]).toEqual({ t: 'hello', protocol: 1, schema: 1 });
  expect(s.commands).toContainEqual({ type: 'syncReset' });
  expect(s.flushing()).toBe(true);
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
  ws.drain();
  expect(ws.sent.at(-1)?.t).toBe('subscribe');
  ws.receive({ t: 'subscribed', doc: 'root', seq: 4, version: encodeVersion(new Map([['1', 3]])) });
  expect(s.commands.at(-1)).toMatchObject({ type: 'syncSubscribed', doc: 'root' });
  expect(s.status()).toBe('synced');
  ws.receive({ t: 'ack', doc: 'root', clientSeq: 9, seq: 5 });
  expect(s.commands.at(-1)).toEqual({ type: 'syncAcked', doc: 'root', clientSeq: 9 });
});

test('индикатор: «синхронизировано» — только когда ответили все документы и догоняющие правки в ядре', () => {
  const s = stand();
  s.client.start();
  const ws = s.last();
  ws.open();
  const version = encodeVersion(new Map());
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version });
  s.client.onCore({ type: 'syncSubscribe', doc: '1@1', version });
  ws.receive({ t: 'subscribed', doc: 'root', seq: 0, version });
  expect(s.status()).toBe('connecting');
  ws.receive({ t: 'updates', doc: '1@1', seq: 1, bytes: new Uint8Array([1]) });
  ws.receive({ t: 'subscribed', doc: '1@1', seq: 1, version });
  expect(s.status()).toBe('connecting');
  s.client.onApplied();
  expect(s.status()).toBe('synced');
});

test('индикатор: правки без подтверждения — «сохранение», подтвердили — снова «синхронизировано»', () => {
  const s = stand();
  s.live();
  expect(s.status()).toBe('synced');
  s.client.onCore({ type: 'syncState', unacked: true });
  expect(s.status()).toBe('saving');
  s.client.onCore({ type: 'syncState', unacked: false });
  expect(s.status()).toBe('synced');
});

test('обрыв: состояние «нет сети», правки не отправляются, переподключение с растущей паузой до 30 с и сразу при online', () => {
  const s = stand();
  const ws = s.live();
  ws.dropped();
  expect(s.status()).toBe('offline');
  expect(s.flushing()).toBe(false);
  // Первая пауза — от 0,5 с.
  s.clock.advance(499);
  expect(s.sockets).toHaveLength(1);
  s.clock.advance(1);
  expect(s.sockets).toHaveLength(2);
  // Подряд не открывается: пауза растёт и упирается в предел.
  let longest = 0;
  for (let i = 0; i < 12; i++) {
    const before = s.sockets.length;
    s.last().dropped();
    let waited = 0;
    while (s.sockets.length === before) {
      s.clock.advance(100);
      waited += 100;
      expect(waited).toBeLessThanOrEqual(RECONNECT_MAX_MS);
    }
    longest = Math.max(longest, waited);
  }
  expect(longest).toBe(RECONNECT_MAX_MS);
  // `online`: не ждать таймера.
  s.last().dropped();
  const n = s.sockets.length;
  s.client.online();
  expect(s.sockets).toHaveLength(n + 1);
  // Удачное подключение сбрасывает паузу.
  s.last().open();
  s.last().receive({ t: 'subscribed', doc: 'root', seq: 0, version: encodeVersion(new Map()) });
  s.last().dropped();
  const again = s.sockets.length;
  s.clock.advance(500);
  expect(s.sockets).toHaveLength(again + 1);
});

test('неподтверждённая правка переживает обрыв: новое соединение — снова подписка и сброс курсора в ядре', () => {
  const s = stand();
  const ws = s.live();
  s.client.onCore(update(1));
  ws.drain();
  expect(ws.sent.some((f) => f.t === 'update')).toBe(true);
  ws.dropped();
  const resets = s.commands.filter((c) => c.type === 'syncReset').length;
  s.clock.advance(500);
  s.last().open();
  expect(s.commands.filter((c) => c.type === 'syncReset').length).toBe(resets + 1);
  // Правка, собранная ядром до обрыва, в очереди не осталась: ядро соберёт её заново от версии сервера.
  s.last().drain();
  expect(s.last().sent.filter((f) => f.t === 'update')).toEqual([]);
});

test('пауза по пределу — не «нет связи»: правки ждут, сокет открыт, таймер переподключения не идёт', () => {
  const s = stand();
  const ws = s.live();
  ws.receive({ t: 'pause', ms: 4000 });
  expect(s.status()).toBe('paused');
  expect(s.flushing()).toBe(false);
  s.clock.advance(3999);
  expect(s.sockets).toHaveLength(1);
  expect(ws.closedCode).toBeNull();
  expect(s.flushing()).toBe(false);
  const resets = s.commands.filter((c) => c.type === 'syncReset').length;
  s.clock.advance(1);
  // Конец паузы: снова подписка, потому что кадр в паузе не принят.
  expect(s.commands.filter((c) => c.type === 'syncReset').length).toBe(resets + 1);
  expect(s.flushing()).toBe(true);
  expect(s.status()).toBe('connecting');
});

test('пауза не считается простоем: проверка соединения ждёт pong, любой кадр продлевает', () => {
  const s = stand();
  const ws = s.live();
  ws.receive({ t: 'pause', ms: 60_000 });
  s.clock.advance(PING_EVERY_MS);
  ws.drain();
  expect(ws.sent.some((f) => f.t === 'ping')).toBe(true);
  ws.receive({ t: 'pong' });
  s.clock.advance(PONG_WAIT_MS);
  expect(ws.closedCode).toBeNull();
  expect(s.status()).toBe('paused');
});

test('мёртвый сокет: нет ответа на ping — закрыть и подключиться заново', () => {
  const s = stand();
  const ws = s.live();
  s.clock.advance(PING_EVERY_MS + PONG_WAIT_MS);
  expect(ws.closedCode).not.toBeNull();
  expect(s.status()).toBe('offline');
  s.clock.advance(500);
  expect(s.sockets).toHaveLength(2);
});

test('медленный канал: сокет не копит без конца, пока буфер полон — следующая правка ждёт', async () => {
  const s = stand();
  const ws = s.live();
  ws.drain();
  const big = 700 * 1024;
  s.client.onCore(update(1, big));
  s.client.onCore(update(2, big));
  s.client.onCore(update(3, big));
  await tick();
  // Два кадра в буфере — уже выше предела; третий ждёт, а не лежит в сокете.
  expect(ws.bufferedAmount).toBeLessThan(2 * big + 1024);
  expect(ws.queued).toHaveLength(2);
  ws.drain();
  s.clock.advance(25);
  await tick();
  ws.drain();
  expect(ws.sent.filter((f) => f.t === 'update').map((f) => (f as { clientSeq: number }).clientSeq)).toEqual([1, 2, 3]);
});

test('очередь на отправку полна: ядро перестаёт отдавать правки, они копятся в документе', async () => {
  const s = stand();
  const ws = s.live();
  // Буфер сокета занят: отправка стоит.
  ws.send(new Uint8Array(2 * 1024 * 1024));
  const mb = 1024 * 1024;
  for (let i = 0; i < 40; i++) s.client.onCore(update(i, mb));
  await tick();
  expect(s.flushing()).toBe(false);
  // Освободилось: снова отдаёт.
  for (let i = 0; i < 60; i++) {
    ws.queued = [];
    s.clock.advance(25);
    await tick();
  }
  expect(s.flushing()).toBe(true);
});

test('картинка уходит раньше правки со ссылкой; не дошла по сети — правка ждёт, не теряется', async () => {
  const uploads: string[] = [];
  let fail = true;
  const s = stand({
    uploadImage: (hash) => {
      uploads.push(hash);
      return fail ? Promise.reject(Object.assign(new Error('сеть'), { code: 'NETWORK' })) : Promise.resolve();
    },
  });
  const ws = s.live();
  s.client.onCore({ type: 'localImage', hash: 'h', bytes: new Uint8Array(1) });
  s.client.onCore(update(1));
  await tick();
  ws.drain();
  expect(ws.sent.filter((f) => f.t === 'update')).toEqual([]);
  fail = false;
  s.clock.advance(5000);
  await tick();
  await tick();
  ws.drain();
  expect(uploads).toEqual(['h', 'h']);
  expect(ws.sent.filter((f) => f.t === 'update')).toHaveLength(1);
});

test('картинка отвергнута сервером: правки дальше не идут, человек об этом узнаёт', async () => {
  const s = stand({ uploadImage: () => Promise.reject(Object.assign(new Error('роль'), { code: 'FORBIDDEN' })) });
  const ws = s.live();
  s.client.onCore({ type: 'localImage', hash: 'h', bytes: new Uint8Array(1) });
  s.client.onCore(update(1));
  await tick();
  ws.drain();
  expect(ws.sent.filter((f) => f.t === 'update')).toEqual([]);
  expect(s.status()).toBe('rejected');
  expect(s.statuses.at(-1)?.detail).toContain('сохраните копию');
});

test('чужие правки складываются, пока ядро занято: следующая пачка — после ответа ядра', () => {
  const s = stand();
  const ws = s.live();
  ws.receive({ t: 'updates', doc: 'root', seq: 1, bytes: new Uint8Array([1]) });
  ws.receive({ t: 'updates', doc: 'root', seq: 2, bytes: new Uint8Array([2]) });
  ws.receive({ t: 'updates', doc: 'root', seq: 3, bytes: new Uint8Array([3]) });
  const applies = () => s.commands.filter((c) => c.type === 'applyRemote') as Extract<CoreCommand, { type: 'applyRemote' }>[];
  expect(applies()).toHaveLength(1);
  expect(applies()[0].batches).toHaveLength(1);
  s.client.onApplied();
  expect(applies()).toHaveLength(2);
  expect(applies()[1].batches).toHaveLength(2);
});

test('порядок ядру: `subscribed` — после правок, пришедших до него', () => {
  const s = stand();
  s.client.start();
  const ws = s.last();
  ws.open();
  ws.receive({ t: 'updates', doc: 'root', seq: 1, bytes: new Uint8Array([1]) });
  ws.receive({ t: 'updates', doc: 'root', seq: 2, bytes: new Uint8Array([2]) });
  ws.receive({ t: 'subscribed', doc: 'root', seq: 2, version: encodeVersion(new Map()) });
  const kinds = () => s.commands.map((c) => c.type);
  expect(kinds().includes('syncSubscribed')).toBe(false);
  s.client.onApplied();
  s.client.onApplied();
  expect(kinds().at(-1)).toBe('syncSubscribed');
});

test('`GAP` и `UNAVAILABLE`: подписка заново через паузу; бесконечно не крутится', () => {
  const s = stand();
  const ws = s.live();
  const resets = () => s.commands.filter((c) => c.type === 'syncReset').length;
  const start = resets();
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
  expect(s.flushing()).toBe(false);
  s.clock.advance(1000);
  expect(resets()).toBe(start + 1);
  expect(s.status()).not.toBe('rejected');
  for (let i = 0; i < 6; i++) {
    ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'GAP' });
    s.clock.advance(1000);
  }
  expect(s.status()).toBe('rejected');
  expect(s.flushing()).toBe(false);
});

test('подтверждение сбрасывает счёт разрывов', () => {
  const s = stand();
  const ws = s.live();
  for (let i = 0; i < 20; i++) {
    ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'GAP' });
    s.clock.advance(1000);
    ws.receive({ t: 'ack', doc: 'root', clientSeq: 2, seq: 1 });
  }
  expect(s.status()).not.toBe('rejected');
});

test('`BAD_UPDATE` и чужой пир: правки больше не уходят, чужие приходят, причина видна', () => {
  for (const code of ['BAD_UPDATE', 'FOREIGN_PEER'] as const) {
    const s = stand();
    const ws = s.live();
    ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code });
    expect(s.status()).toBe('rejected');
    expect(s.flushing()).toBe(false);
    ws.receive({ t: 'updates', doc: 'root', seq: 1, bytes: new Uint8Array([1]) });
    expect(s.commands.at(-1)?.type).toBe('applyRemote');
  }
});

test('`LOG_FULL` — пауза с объяснением, а не сбой', () => {
  const s = stand();
  const ws = s.live();
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'LOG_FULL' });
  expect(s.statuses.at(-1)).toMatchObject({ status: 'paused' });
  expect(s.statuses.at(-1)?.detail).toContain('не потеряются');
});

test('`FORBIDDEN`: спрашивает сервер о роли, редактор узнаёт', async () => {
  const s = stand({ access: () => Promise.resolve({ kind: 'role', role: 'viewer', version: 1, canShare: false, own: true }) });
  const ws = s.live();
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'FORBIDDEN' });
  await tick();
  expect(s.access.at(-1)).toMatchObject({ kind: 'role', role: 'viewer' });
});

test('роль и снятие доступа — тем же получателям; снятый не переподключается', () => {
  const s = stand();
  const ws = s.live();
  ws.receive({ t: 'role', role: 'viewer', canShare: false, version: 3 });
  expect(s.access.at(-1)).toEqual({ kind: 'role', role: 'viewer', version: 3, canShare: false, own: true });
  ws.receive({ t: 'access-revoked', reason: 'access' });
  expect(s.access.at(-1)).toEqual({ kind: 'none' });
  ws.dropped(SYNC_CLOSE.REVOKED);
  s.clock.advance(RECONNECT_MAX_MS * 2);
  expect(s.sockets).toHaveLength(1);
});

test('закрытие сервера кодом: сессия, доступ, новая версия; не повторять', () => {
  for (const [code, state] of [
    [SYNC_CLOSE.SESSION, { kind: 'session' }],
    [SYNC_CLOSE.REVOKED, { kind: 'none' }],
  ] as const) {
    const s = stand();
    s.live().dropped(code);
    expect(s.access.at(-1)).toEqual(state);
    s.clock.advance(RECONNECT_MAX_MS);
    s.client.online();
    expect(s.sockets).toHaveLength(1);
  }
  const s = stand();
  s.live().dropped(SYNC_CLOSE.RELOAD);
  expect(s.reloads()).toBe(1);
  const f = stand();
  f.live().receive({ t: 'please-reload' });
  expect(f.reloads()).toBe(1);
});

test('соединение не открывается: браузер причину не скажет, спрашиваем сервер; сессия кончилась — не вечное «нет сети»', async () => {
  const s = stand({ access: () => Promise.resolve({ kind: 'session' }) });
  s.client.start();
  s.last().dropped();
  await tick();
  expect(s.access.at(-1)).toEqual({ kind: 'session' });
  s.clock.advance(RECONNECT_MAX_MS);
  expect(s.sockets).toHaveLength(1);
});

test('сеть не отвечает и на проверку: продолжает пробовать', async () => {
  const s = stand({ access: () => Promise.resolve(null) });
  s.client.start();
  s.last().dropped();
  await tick();
  expect(s.access).toEqual([]);
  s.clock.advance(500);
  expect(s.sockets).toHaveLength(2);
});

test('сервер отправил мусор: как обрыв, а не сбой', () => {
  const s = stand();
  const ws = s.live();
  ws.onmessage?.({ data: new Uint8Array([250, 1]).buffer });
  expect(ws.closedCode).not.toBeNull();
  expect(s.status()).toBe('offline');
});

test('снимок при `snapshot-needed`: применить, подписаться заново; 429 и сбой сети — ждать и повторить', async () => {
  let calls = 0;
  const s = stand({
    snapshot: () => {
      calls++;
      return calls === 1 ? Promise.reject(Object.assign(new Error('сеть'), { code: 'NETWORK' })) : Promise.resolve(new ArrayBuffer(4));
    },
  });
  const ws = s.live();
  const resets = s.commands.filter((c) => c.type === 'syncReset').length;
  ws.receive({ t: 'snapshot-needed', doc: 'root', url: '/x' });
  expect(s.flushing()).toBe(false);
  await tick();
  s.clock.advance(1000);
  await tick();
  await tick();
  expect(calls).toBe(2);
  expect(s.commands.some((c) => c.type === 'applyRemote')).toBe(true);
  s.client.onApplied();
  expect(s.commands.filter((c) => c.type === 'syncReset').length).toBe(resets + 1);
});

test('сервер снова и снова просит снимок: не по кругу, человек узнаёт', async () => {
  const s = stand();
  const ws = s.live();
  for (let i = 0; i < 4; i++) {
    ws.receive({ t: 'snapshot-needed', doc: 'root', url: '/x' });
    await tick();
    await tick();
    s.client.onApplied();
  }
  expect(s.status()).toBe('rejected');
});

test('чужая картинка: скачать и отдать ядру, не больше четырёх разом', async () => {
  let active = 0;
  let peak = 0;
  const release: (() => void)[] = [];
  const s = stand({
    downloadImage: () =>
      new Promise((resolve) => {
        active++;
        peak = Math.max(peak, active);
        release.push(() => {
          active--;
          resolve(new ArrayBuffer(1));
        });
      }),
  });
  s.live();
  s.client.onCore({ type: 'imagesNeeded', hashes: ['a', 'b', 'c', 'd', 'e', 'f'] });
  await tick();
  expect(peak).toBe(4);
  while (release.length) {
    release.shift()!();
    await tick();
  }
  await tick();
  expect(s.commands.filter((c) => c.type === 'addRemoteImage')).toHaveLength(6);
});

test('остановка: таймеров нет, сокет закрыт, переподключения нет', () => {
  const s = stand();
  const ws = s.live();
  s.client.stop();
  expect(ws.closedCode).not.toBeNull();
  expect(s.flushing()).toBe(false);
  expect(s.clock.timers.size).toBe(0);
  s.clock.advance(RECONNECT_MAX_MS);
  expect(s.sockets).toHaveLength(1);
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/sync-client.test.ts`
Expected: FAIL — `Cannot find module` для `src/core/sync-client`

- [ ] **Step 3: Реализация**

В `apps/web/src/core/core.ts` заменить:

````ts
    core.loop();
    return core;
  }

````

на:

````ts
    core.loop();
    return core;
  }

  /** Прямой порт к `sync.worker`: дальше worker ядра говорит с ним сам, основной поток трафика не видит. */
  connectSync(port: MessagePort): void {
    this.transport.post({ t: 'syncPort', port }, [port]);
  }

````

В `apps/web/src/core/messages.ts` заменить:

````ts

export type ToWorker =
  | {
      t: 'init';
````

на:

````ts

export type ToWorker =
  /** Прямой порт к `sync.worker`: файл на сервере. */
  | { t: 'syncPort'; port: MessagePort }
  | {
      t: 'init';
````

В `apps/web/src/core/messages.ts` заменить:

````ts

/** Виды сообщений worker'у: остальное — неверный конверт. */
export const TO_WORKER_KINDS: ReadonlySet<string> = new Set(['init', 'cmd', 'pasteText', 'encodeClipboard', 'tick', 'testPanic']);

/** Конверт сообщения worker'у: объект известного вида. Содержимое команды
````

на:

````ts

/** Виды сообщений worker'у: остальное — неверный конверт. */
export const TO_WORKER_KINDS: ReadonlySet<string> = new Set([
  'init',
  'cmd',
  'pasteText',
  'encodeClipboard',
  'tick',
  'testPanic',
  'syncPort',
]);

/** Конверт сообщения worker'у: объект известного вида. Содержимое команды
````

В `apps/web/src/core/server-files.ts` заменить:

````ts
  session: number | null;
  tooLarge: boolean;
}

/** Журнал правок этого человека (в браузере — IndexedDB с разделением по пользователю). */
````

на:

````ts
  session: number | null;
  tooLarge: boolean;
  /** Ключи `entries`: снять именно их, когда сервер подтвердил. */
  keys?: JournalKey[];
}

export type JournalKey = [file: string, session: number, batch: number];

/** Журнал правок этого человека (в браузере — IndexedDB с разделением по пользователю). */
````

В `apps/web/src/core/server-files.ts` заменить:

````ts
  /** Снимок вобрал пачки до `upTo` этого сеанса и все прежние. */
  cut(file: string, session: number, upTo: number): Promise<void>;
  drop(file: string): Promise<void>;
}
````

на:

````ts
  /** Снимок вобрал пачки до `upTo` этого сеанса и все прежние. */
  cut(file: string, session: number, upTo: number): Promise<void>;
  /** Только эти пачки: у журнала сервера файл открыт в нескольких вкладках, пачки других вкладок ещё нужны. */
  cutExact(keys: readonly JournalKey[]): Promise<void>;
  drop(file: string): Promise<void>;
}
````

В `apps/web/src/core/server-files.ts` заменить:

````ts
  private readonly thumbs = new Map<string, { last: number; pending: ArrayBuffer | null | undefined; timer: unknown }>();
  private readonly lastAccess = new Map<string, string>();

  constructor(
````

на:

````ts
  private readonly thumbs = new Map<string, { last: number; pending: ArrayBuffer | null | undefined; timer: unknown }>();
  private readonly lastAccess = new Map<string, string>();
  /** Пачки журнала, прочитанные при открытии файла: они вошли в документ, их снимает подтверждение сервера. */
  private readonly readKeys = new Map<string, readonly JournalKey[]>();

  constructor(
````

В `apps/web/src/core/server-files.ts` заменить:

````ts
    // новые пачки идут после прежних.
    const held = m.role === 'viewer' && (j.tooLarge || j.entries.length > 0);
    if (held) this.heldJournal.add(file);
    else this.heldJournal.delete(file);
````

на:

````ts
    // новые пачки идут после прежних.
    const held = m.role === 'viewer' && (j.tooLarge || j.entries.length > 0);
    // Читатель журнал не применил: его подтверждение ничего о нём не значит, пачки ждут, пока вернут право.
    this.readKeys.set(file, m.role === 'viewer' ? [] : (j.keys ?? []));
    if (held) this.heldJournal.add(file);
    else this.heldJournal.delete(file);
````

В `apps/web/src/core/server-files.ts` заменить:

````ts
  /** Файл `.ferrum` с картинками: «Скачать копию» (читателю тоже, раздел 6.1). */
  async blob(file: string): Promise<Blob | null> {
    const { bytes } = await this.download(file);
    return bytes ? new Blob([bytes], { type: 'application/x-ferrum' }) : null;
````

на:

````ts
  /** Файл `.ferrum` с картинками: «Скачать копию» (читателю тоже, раздел 6.1). */
  async blob(file: string): Promise<Blob | null> {
    // Правки могут быть ещё в журнале сервера: сначала он собирает файл целиком. Срок — как у передачи всего файла.
    let compacted: Response;
    try {
      compacted = await this.http(`${API_FILES}/${file}/compact`, {
        method: 'POST',
        signal: AbortSignal.timeout(transferTimeout(MAX_OPEN_BYTES)),
      });
    } catch {
      throw new ServerFailure('NETWORK');
    }
    if (!compacted.ok) await this.readReply(compacted, (_v): _v is never => false);
    const { bytes } = await this.download(file);
    return bytes ? new Blob([bytes], { type: 'application/x-ferrum' }) : null;
````

В `apps/web/src/core/server-files.ts` заменить:

````ts
    const thumb = state.pending;
    state.pending = undefined;
    state.last = this.clock.now();
    try {
      const res = await this.http(
````

на:

````ts
    const thumb = state.pending;
    state.pending = undefined;
    // Убрать превью (страница пуста) — не повод откладывать первое настоящее.
    if (thumb) state.last = this.clock.now();
    try {
      const res = await this.http(
````

В `apps/web/src/core/server-files.ts` заменить:

````ts
      // Превью обновится со следующей записью.
    }
  }

````

на:

````ts
      // Превью обновится со следующей записью.
    }
  }

  /** Картинка новой правки (`sync.worker`): раньше самой правки. Есть у файла — байты не идут. */
  async uploadOneImage(file: string, hash: string, bytes: Uint8Array): Promise<void> {
    await this.uploadImages(file, [{ hash, size: bytes.length, part: bytes as Uint8Array<ArrayBuffer> }]);
  }

  /** Картинка по чужой ссылке. Нет у файла — `null`. */
  downloadOneImage(file: string, hash: string): Promise<ArrayBuffer | null> {
    return this.image(file, hash);
  }

  /** Снимок одного документа: клиент отстал сильнее журнала. Сверх предела частоты ждёт, сколько просит сервер. */
  async fetchSnapshot(path: string): Promise<ArrayBuffer> {
    let res: Response;
    try {
      res = await this.paced(() => this.http(path));
    } catch {
      throw new ServerFailure('NETWORK');
    }
    if (!res.ok) {
      await this.readReply(res, (_v): _v is never => false);
      throw new ServerFailure('NETWORK');
    }
    try {
      return await res.arrayBuffer();
    } catch {
      throw new ServerFailure('NETWORK');
    }
  }

  /** Доступ сейчас, без памяти наблюдений. Сеть — `null`: это не сообщение о доступе. */
  async accessOnce(file: string): Promise<AccessState | null> {
    try {
      const a = await this.json<FileAccessDto>(`${API_FILES}/${file}/access`, isFileAccess);
      return { kind: 'role', role: a.role, version: a.version, canShare: a.canShare === true, own: true };
    } catch (e) {
      if (e instanceof ServerFailure && e.code === 'NOT_FOUND') return { kind: 'none' };
      if (e instanceof ServerFailure && e.code === 'UNAUTHORIZED') return { kind: 'session' };
      return null;
    }
  }

  /** Сервер подтвердил правки журнала. `upTo` 0 — то, что было прочитано при открытии; иначе — пачки этого сеанса до `upTo`. Чужие
   * сеансы не трогает: это другая вкладка, а её правки могут быть ещё только здесь. */
  async cutJournal(file: string, session: number, upTo: number): Promise<void> {
    if (upTo === 0) {
      const keys = this.readKeys.get(file) ?? [];
      this.readKeys.delete(file);
      if (keys.length > 0) await this.journal.cutExact(keys);
      return;
    }
    const own: JournalKey[] = Array.from({ length: upTo }, (_, i): JournalKey => [file, session, i + 1]);
    await this.journal.cutExact(own);
  }

  /** Превью при правках журналом: файл целиком больше не пишется. */
  putThumb(file: string, thumb: ArrayBuffer | null): void {
    this.offerThumb(file, thumb);
  }

````

В `apps/web/src/core/server-messages.ts` заменить:

````ts
    }
  | { op: 'appendJournal'; file: string; session: number; batch: number; bytes: ArrayBuffer }
  | { op: 'blob'; file: string }
  | { op: 'rename'; file: string; name: string }
````

на:

````ts
    }
  | { op: 'appendJournal'; file: string; session: number; batch: number; bytes: ArrayBuffer }
  | { op: 'cutJournal'; file: string; session: number; upTo: number }
  | { op: 'putThumb'; file: string; thumb: ArrayBuffer | null }
  | { op: 'blob'; file: string }
  | { op: 'rename'; file: string; name: string }
````

В `apps/web/src/core/server-messages.ts` заменить:

````ts
  write: FileMeta;
  appendJournal: null;
  blob: Blob | null;
  rename: FileMeta;
````

на:

````ts
  write: FileMeta;
  appendJournal: null;
  cutJournal: null;
  putThumb: null;
  blob: Blob | null;
  rename: FileMeta;
````

В `apps/web/src/core/server-messages.ts` заменить:

````ts

export type ToServer =
  { t: 'init'; user: string } | { t: 'visible'; on: boolean } | { t: 'watch'; file: string; on: boolean } | ({ t: 'req' } & ServerRequest);

export type FromServer =
````

на:

````ts

export type ToServer =
  | { t: 'init'; user: string }
  | { t: 'visible'; on: boolean }
  /** `poll: false` — состояние доступа приходит по соединению файла, опрашивать не нужно. */
  | { t: 'watch'; file: string; on: boolean; poll?: boolean }
  | ({ t: 'req' } & ServerRequest);

export type FromServer =
````

Создать `apps/web/src/core/sync-client.ts`:

````ts
// Соединение файла со стороны клиента (спека подсистемы 4, разделы 7 и 8.1). Логика без DOM и без настоящего сокета: её запускает
// `worker/sync.worker.ts`, модульные тесты дают ей свой сокет с настоящим `bufferedAmount`. Основной поток трафика не видит:
// правки идут из worker'а ядра прямым `MessagePort`.
//
// Ничего не хранится здесь надолго. Что отправить, знает курсор ядра: на каждом новом соединении ядро подписывается своей версией,
// сервер отвечает своей, и ядро шлёт всё сверх неё. Поэтому обрыв, рестарт сервера, отказ «повторите» и пауза — один и тот же
// путь (`resync`), а неподтверждённое не теряется: оно в документе и в журнале браузера.

import {
  encodeClientFrame,
  parseServerFrame,
  SYNC_CLOSE,
  SYNC_PROTOCOL,
  syncPath,
  type ServerFrame,
  type SyncRefusal,
} from '@ferrum/protocol';
import type { AccessState, Clock } from './server-files';

/** `offline` и `saving` — тексты индикатора §8.1. `rejected` — правки больше не уходят, человек должен знать. */
export type SyncStatus = 'connecting' | 'synced' | 'saving' | 'offline' | 'paused' | 'rejected';

export interface SocketLike {
  binaryType: string;
  readonly bufferedAmount: number;
  onopen: ((e: unknown) => void) | null;
  onmessage: ((e: { data: unknown }) => void) | null;
  onclose: ((e: { code: number }) => void) | null;
  onerror: ((e: unknown) => void) | null;
  send(bytes: Uint8Array): void;
  close(code?: number): void;
}

/** Команды ядру: `transfer` — буферы уходят без копии. `applied` — ядро закончило `applyRemote`. */
export type CoreCommand =
  | { type: 'syncStart' }
  | { type: 'syncReset' }
  | { type: 'syncSubscribed'; doc: string; version: Uint8Array }
  | { type: 'syncAcked'; doc: string; clientSeq: number }
  | { type: 'applyRemote'; doc: string; batches: Uint8Array[] }
  | { type: 'addRemoteImage'; hash: string; bytes: Uint8Array };

/** События ядра, которые идут этому классу. */
export type CoreSyncEvent =
  | { type: 'syncSubscribe'; doc: string; version: Uint8Array }
  | { type: 'localImage'; hash: string; bytes: Uint8Array }
  | { type: 'localUpdates'; doc: string; clientSeq: number; bytes: Uint8Array }
  | { type: 'syncState'; unacked: boolean }
  | { type: 'imagesNeeded'; hashes: string[] };

export interface SyncEnv {
  file: string;
  user: string;
  schema: number;
  /** Адрес сайта: `https://…` → `wss://…`. */
  origin: string;
  socket(url: string): SocketLike;
  core: {
    command(c: CoreCommand): void;
    /** Отдавать ли правки: ядро шлёт, только пока есть куда. */
    flush(on: boolean): void;
  };
  files: {
    uploadImage(hash: string, bytes: Uint8Array): Promise<void>;
    downloadImage(hash: string): Promise<ArrayBuffer | null>;
    snapshot(path: string): Promise<ArrayBuffer>;
    /** Лёгкая проверка: почему соединение не открывается. Сеть — `null`. */
    access(): Promise<AccessState | null>;
  };
  clock: Clock;
  random(): number;
  status(s: SyncStatus, detail?: string): void;
  access(state: AccessState): void;
  reload(): void;
}

/** Переподключение: от 0,5 до 30 с с разбросом, сразу при `online` (спека, раздел 8.1). */
export const RECONNECT_MIN_MS = 500;
export const RECONNECT_MAX_MS = 30_000;
/** Сокет молчит столько после `ping` — мёртв: браузер об этом не сообщает. */
export const PING_EVERY_MS = 20_000;
export const PONG_WAIT_MS = 15_000;
/** В буфере сокета больше этого — следующая пачка ждёт: сокет не копит без конца. */
const SEND_HIGH_WATER = 1024 * 1024;
const SEND_RETRY_MS = 25;
/** Столько байт правок может ждать отправки здесь. */
const MAX_OUT_BYTES = 32 * 1024 * 1024;
/** Загрузка картинки не удалась по сети: снова через столько, правки за ней ждут. */
const IMAGE_RETRY_MS = 5000;
/** Отказ «повторите» (`UNAVAILABLE`, `GAP`): от этого и с тем же ростом, что переподключение. */
const RESYNC_MIN_MS = 1000;
/** Столько раз подряд `GAP` без единого `ack`: клиент не может послать правки в верном порядке. */
const MAX_GAPS = 6;
/** Столько снимков подряд на документ без ответа `subscribed`. */
const MAX_SNAPSHOT_ROUNDS = 3;
/** Сколько картинок качается разом. */
const IMAGE_CONCURRENCY = 4;
/** Журнал файла полон: ждём, пока сервер его сожмёт. */
const LOG_FULL_WAIT_MS = 30_000;

type Out = { kind: 'image'; hash: string; bytes: Uint8Array } | { kind: 'update'; doc: string; clientSeq: number; bytes: Uint8Array };
type In = { kind: 'apply'; doc: string; batches: Uint8Array[] } | { kind: 'cmd'; cmd: CoreCommand };

export class SyncClient {
  private ws: SocketLike | null = null;
  private attempt = 0;
  private reconnectTimer: unknown = null;
  private pingTimer: unknown = null;
  private pongTimer: unknown = null;
  private pauseTimer: unknown = null;
  private retryTimer: unknown = null;
  private sendTimer: unknown = null;
  private opened = false;
  private ended = false;
  private rejected: string | null = null;
  private paused = false;
  private unacked = false;
  private readyDocs = 0;
  /** Подписки без ответа: пока они есть, файл ещё догоняет. */
  private pendingSubs = 0;
  private gaps = 0;
  private resyncs = 0;
  private readonly out: Out[] = [];
  private outBytes = 0;
  private sending = false;
  private readonly inbound: In[] = [];
  private applying = false;
  private snapshots = 0;
  private readonly snapshotRounds = new Map<string, number>();
  private needImages = new Set<string>();
  private imagesActive = 0;
  private lastStatus: string | null = null;
  private generation = 0;
  /** Соединение было и пропало или не открылось: с этого момента — «нет сети», а не «подключается». */
  private lostOnce = false;
  /** С начала подписки до первого «всё на месте». */
  private catchingUp = true;

  constructor(private readonly env: SyncEnv) {}

  start(): void {
    this.env.core.command({ type: 'syncStart' });
    this.emitStatus();
    this.connect();
  }

  /** Вкладка ушла с файла. */
  stop(): void {
    this.ended = true;
    this.clearTimers();
    this.env.core.flush(false);
    const ws = this.ws;
    this.ws = null;
    if (ws) {
      ws.onclose = null;
      ws.close();
    }
  }

  /** Браузер сообщил, что сеть вернулась: не ждать таймера. */
  online(): void {
    if (this.ended || this.ws) return;
    this.attempt = 0;
    this.clock(this.reconnectTimer);
    this.reconnectTimer = null;
    this.connect();
  }

  /** Сколько раз заново подписывались, для тестов. */
  get resubscribes(): number {
    return this.resyncs;
  }

  private clock(t: unknown): void {
    if (t !== null) this.env.clock.clearTimeout(t);
  }

  private clearTimers(): void {
    for (const t of [this.reconnectTimer, this.pingTimer, this.pongTimer, this.pauseTimer, this.retryTimer, this.sendTimer]) this.clock(t);
    this.reconnectTimer = this.pingTimer = this.pongTimer = this.pauseTimer = this.retryTimer = this.sendTimer = null;
  }

  private connect(): void {
    if (this.ended || this.ws) return;
    const generation = ++this.generation;
    const url = this.env.origin.replace(/^http/, 'ws') + syncPath(this.env.file, this.env.user);
    let ws: SocketLike;
    try {
      ws = this.env.socket(url);
    } catch {
      this.scheduleReconnect();
      return;
    }
    this.ws = ws;
    this.opened = false;
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => {
      if (generation === this.generation) this.onOpen();
    };
    ws.onmessage = (e) => {
      if (generation === this.generation) this.onMessage(e.data);
    };
    ws.onclose = (e) => {
      if (generation === this.generation) this.onClose(e.code);
    };
    ws.onerror = () => {};
  }

  private onOpen(): void {
    this.opened = true;
    this.send(encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: this.env.schema }));
    this.resync();
    this.armPing();
  }

  private send(bytes: Uint8Array): void {
    try {
      this.ws?.send(bytes);
    } catch {
      // Сокет закрывается: `onclose` уже в пути.
    }
  }

  /** Всё заново от версии сервера: новое соединение, отказ «повторите», конец паузы. Очередь правок уже не нужна: их заново соберёт
   * ядро. Картинки остаются: их байты больше нигде не собрать. */
  private resync(): void {
    this.resyncs++;
    this.readyDocs = 0;
    this.pendingSubs = 0;
    this.catchingUp = true;
    this.drop('update');
    this.enqueueCmd({ type: 'syncReset' });
    this.updateFlush();
    this.emitStatus();
  }

  private drop(kind: Out['kind']): void {
    for (let i = this.out.length - 1; i >= 0; i--) {
      if (this.out[i].kind === kind) this.outBytes -= this.out.splice(i, 1)[0].bytes.length;
    }
  }

  private push(item: Out): void {
    this.out.push(item);
    this.outBytes += item.bytes.length;
    if (this.outBytes >= MAX_OUT_BYTES) this.updateFlush();
    this.pumpOut();
  }

  private shiftOut(): void {
    const was = this.outBytes;
    this.outBytes -= this.out.shift()!.bytes.length;
    if (was >= MAX_OUT_BYTES && this.outBytes < MAX_OUT_BYTES) this.updateFlush();
  }

  private armPing(): void {
    this.clock(this.pingTimer);
    this.pingTimer = this.env.clock.setTimeout(() => {
      this.pingTimer = null;
      this.send(encodeClientFrame({ t: 'ping' }));
      this.clock(this.pongTimer);
      this.pongTimer = this.env.clock.setTimeout(() => {
        this.pongTimer = null;
        // Сокет мёртв: закрыть и не ждать `onclose` браузера, он может идти минуты.
        const ws = this.ws;
        if (ws) {
          ws.onclose = null;
          ws.close();
          this.onClose(1006);
        }
      }, PONG_WAIT_MS);
    }, PING_EVERY_MS);
  }

  private onMessage(data: unknown): void {
    if (!(data instanceof ArrayBuffer)) return;
    // Любой кадр — соединение живо.
    this.clock(this.pongTimer);
    this.pongTimer = null;
    if (!this.pingTimer) this.armPing();
    const frame = parseServerFrame(new Uint8Array(data));
    if (!frame) {
      // Сеть не доверенная: как обрыв.
      this.dropSocket();
      return;
    }
    this.handle(frame);
  }

  private dropSocket(): void {
    const ws = this.ws;
    if (ws) {
      ws.onclose = null;
      ws.close();
      this.onClose(1006);
    }
  }

  private handle(f: ServerFrame): void {
    switch (f.t) {
      case 'role':
        this.env.access({ kind: 'role', role: f.role, version: f.version, canShare: f.canShare, own: true });
        break;
      case 'subscribed':
        this.attempt = 0;
        this.lostOnce = false;
        this.snapshotRounds.delete(f.doc);
        this.readyDocs++;
        this.pendingSubs = Math.max(0, this.pendingSubs - 1);
        this.enqueueCmd({ type: 'syncSubscribed', doc: f.doc, version: f.version.slice() });
        this.emitStatus();
        break;
      case 'updates':
        this.enqueueApply(f.doc, f.bytes.slice());
        break;
      case 'ack':
        this.gaps = 0;
        this.enqueueCmd({ type: 'syncAcked', doc: f.doc, clientSeq: f.clientSeq });
        break;
      case 'refused':
        this.refused(f.code);
        break;
      case 'pause':
        this.pause(f.ms);
        break;
      case 'snapshot-needed':
        this.pendingSubs = Math.max(0, this.pendingSubs - 1);
        void this.fetchSnapshot(f.doc, f.url);
        break;
      case 'access-revoked':
        this.end();
        this.env.access({ kind: f.reason === 'session' ? 'session' : 'none' });
        break;
      case 'please-reload':
        this.end();
        this.env.reload();
        break;
      case 'pong':
        break;
    }
  }

  private end(): void {
    this.ended = true;
    this.clearTimers();
    this.env.core.flush(false);
  }

  private refused(code: SyncRefusal): void {
    switch (code) {
      case 'GAP':
        if (++this.gaps >= MAX_GAPS) {
          this.reject(
            'Сервер не принимает правки в этом порядке. Они сохранены только в этом браузере — сохраните копию на диск и обновите страницу',
          );
          return;
        }
        this.resyncLater();
        break;
      case 'UNAVAILABLE':
        this.resyncLater();
        break;
      case 'LOG_FULL':
        this.pause(LOG_FULL_WAIT_MS, 'Журнал файла на сервере переполнен. Правки ждут здесь, они не потеряются');
        break;
      case 'FORBIDDEN':
        // Роль сменилась: настоящее состояние скажет сервер, редактор перейдёт в просмотр и покажет, что правки не дошли.
        void this.env.files.access().then((a) => {
          if (a) this.env.access(a);
        });
        break;
      case 'BAD_UPDATE':
      case 'FOREIGN_PEER':
        this.reject('Сервер не принял правки. Они сохранены только в этом браузере — сохраните копию на диск и обновите страницу');
        break;
    }
  }

  /** Отказ, который повтор не исправит: правки дальше не уходят, чужие продолжают приходить, человек об этом знает. */
  private reject(message: string): void {
    this.rejected = message;
    this.env.core.flush(false);
    this.emitStatus();
  }

  private resyncLater(): void {
    if (this.retryTimer !== null || this.paused) return;
    const wait = RESYNC_MIN_MS * (0.5 + this.env.random() / 2);
    this.env.core.flush(false);
    this.retryTimer = this.env.clock.setTimeout(() => {
      this.retryTimer = null;
      if (this.ws && this.opened && !this.paused) this.resync();
    }, wait);
  }

  /** Предел частоты: пауза, а не «Нет связи». Ни один срок не считает её простоем. */
  private pause(ms: number, detail?: string): void {
    this.paused = true;
    this.env.core.flush(false);
    this.clock(this.retryTimer);
    this.retryTimer = null;
    this.clock(this.pauseTimer);
    this.pauseTimer = this.env.clock.setTimeout(() => {
      this.pauseTimer = null;
      this.paused = false;
      this.emitStatus();
      if (this.ws && this.opened) this.resync();
    }, ms);
    this.emitStatus(detail);
  }

  private onClose(code: number): void {
    this.lostOnce = true;
    this.ws = null;
    this.opened = false;
    this.readyDocs = 0;
    this.pendingSubs = 0;
    this.clock(this.pingTimer);
    this.clock(this.pongTimer);
    this.pingTimer = this.pongTimer = null;
    this.env.core.flush(false);
    this.drop('update');
    if (code === SYNC_CLOSE.SESSION) {
      this.end();
      this.env.access({ kind: 'session' });
      return;
    }
    if (code === SYNC_CLOSE.REVOKED) {
      this.end();
      this.env.access({ kind: 'none' });
      return;
    }
    if (code === SYNC_CLOSE.RELOAD) {
      this.end();
      this.env.reload();
      return;
    }
    if (code === SYNC_CLOSE.BAD_FRAME) {
      this.reject('Соединение с сервером нарушено — обновите страницу');
    }
    this.emitStatus();
    this.scheduleReconnect();
    // Соединение не открылось: браузер не говорит почему. Сессия кончилась и доступ снят — не вечное «Нет сети».
    if (!this.ended) {
      void this.env.files.access().then((a) => {
        if (!this.ended && a && a.kind !== 'role') {
          this.end();
          this.env.access(a);
        }
      });
    }
  }

  private scheduleReconnect(): void {
    if (this.ended || this.reconnectTimer !== null) return;
    const base = Math.min(RECONNECT_MAX_MS, RECONNECT_MIN_MS * 2 ** this.attempt++);
    this.reconnectTimer = this.env.clock.setTimeout(
      () => {
        this.reconnectTimer = null;
        this.connect();
      },
      base * (0.5 + this.env.random() / 2),
    );
  }

  // ——— События ядра ———

  onCore(e: CoreSyncEvent): void {
    switch (e.type) {
      case 'syncSubscribe':
        this.pendingSubs++;
        this.emitStatus();
        this.send(encodeClientFrame({ t: 'subscribe', doc: e.doc, version: e.version }));
        break;
      case 'localImage':
        this.push({ kind: 'image', hash: e.hash, bytes: e.bytes });
        break;
      case 'localUpdates':
        // Соединение успело упасть: ядро соберёт их снова от версии сервера.
        if (this.ws && this.opened) this.push({ kind: 'update', doc: e.doc, clientSeq: e.clientSeq, bytes: e.bytes });
        break;
      case 'syncState':
        this.unacked = e.unacked;
        this.emitStatus();
        break;
      case 'imagesNeeded':
        for (const h of e.hashes) this.needImages.add(h);
        this.pumpImages();
        break;
    }
  }

  /** Ядро закончило `applyRemote`: следующая пачка. Пока оно занято, входящее складывается в одну. */
  onApplied(): void {
    this.applying = false;
    this.pumpIn();
  }

  private enqueueCmd(cmd: CoreCommand): void {
    this.inbound.push({ kind: 'cmd', cmd });
    this.pumpIn();
  }

  private enqueueApply(doc: string, bytes: Uint8Array): void {
    const last = this.inbound.at(-1);
    if (last?.kind === 'apply' && last.doc === doc) last.batches.push(bytes);
    else this.inbound.push({ kind: 'apply', doc, batches: [bytes] });
    this.pumpIn();
  }

  private pumpIn(): void {
    while (!this.applying && this.inbound.length > 0) {
      const item = this.inbound.shift()!;
      if (item.kind === 'cmd') {
        this.env.core.command(item.cmd);
      } else {
        this.applying = true;
        this.env.core.command({ type: 'applyRemote', doc: item.doc, batches: item.batches });
      }
    }
    this.emitStatus();
  }

  // ——— Исходящее: по порядку, картинка раньше правки со ссылкой на неё ———

  /** Подписки и правки отдаёт одна и та же команда ядра; правки ядро шлёт только после ответа на подписку. Очередь на отправку полна —
   * ядро ждёт, правки копятся в документе, а не в памяти этого worker'а. */
  private updateFlush(): void {
    const room = this.outBytes < MAX_OUT_BYTES;
    this.env.core.flush(!!this.ws && this.opened && !this.paused && !this.rejected && room);
  }

  private pumpOut(): void {
    if (this.sending) return;
    this.sending = true;
    void this.drainOut().finally(() => {
      this.sending = false;
      // Пока флаг ещё стоял, в очередь могло лечь новое: его никто бы не забрал.
      if (this.out.length > 0 && this.ws && this.opened && !this.rejected) this.pumpOut();
    });
  }

  private async drainOut(): Promise<void> {
    while (this.out.length > 0 && !this.ended) {
      const next = this.out[0];
      if (next.kind === 'image') {
        try {
          await this.env.files.uploadImage(next.hash, next.bytes);
          this.shiftOut();
        } catch (e) {
          const code = (e as { code?: string }).code;
          if (code === 'NETWORK' || code === 'UNAVAILABLE' || code === 'TOO_MANY_REQUESTS' || code === 'BUSY') {
            await this.sleep(IMAGE_RETRY_MS);
            continue;
          }
          this.reject('Картинка не загрузилась на сервер. Правки сохранены только в этом браузере — сохраните копию на диск');
          return;
        }
        continue;
      }
      if (!this.ws || !this.opened) return;
      // Медленный канал: `send` копил бы в браузере без предела.
      if (this.ws.bufferedAmount > SEND_HIGH_WATER) {
        await this.sleep(SEND_RETRY_MS);
        continue;
      }
      this.shiftOut();
      this.send(encodeClientFrame({ t: 'update', doc: next.doc, clientSeq: next.clientSeq, bytes: next.bytes }));
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => {
      this.sendTimer = this.env.clock.setTimeout(() => {
        this.sendTimer = null;
        resolve();
      }, ms);
    });
  }

  // ——— Что запрашивается отдельно ———

  private async fetchSnapshot(doc: string, url: string): Promise<void> {
    // Снимок применён, а сервер снова просит снимок: дальше по кругу только трафик.
    const rounds = (this.snapshotRounds.get(doc) ?? 0) + 1;
    this.snapshotRounds.set(doc, rounds);
    if (rounds > MAX_SNAPSHOT_ROUNDS) {
      this.reject('Файл не удаётся догнать — обновите страницу. Правки сохранены в этом браузере');
      return;
    }
    this.snapshots++;
    this.env.core.flush(false);
    for (let wait = RESYNC_MIN_MS; !this.ended; wait = Math.min(RECONNECT_MAX_MS, wait * 2)) {
      try {
        const bytes = await this.env.files.snapshot(url);
        this.enqueueApply(doc, new Uint8Array(bytes));
        break;
      } catch (e) {
        const code = (e as { code?: string }).code;
        if (code === 'NOT_FOUND' || code === 'UNAUTHORIZED') {
          const a = await this.env.files.access();
          if (a && a.kind !== 'role') {
            this.end();
            this.env.access(a);
            return;
          }
        }
        await this.sleep(wait);
      }
    }
    if (--this.snapshots === 0 && this.ws && this.opened) this.resync();
  }

  private pumpImages(): void {
    for (const hash of this.needImages) {
      if (this.imagesActive >= IMAGE_CONCURRENCY) return;
      this.needImages.delete(hash);
      this.imagesActive++;
      void this.env.files
        .downloadImage(hash)
        .then((bytes) => {
          if (bytes) this.enqueueCmd({ type: 'addRemoteImage', hash, bytes: new Uint8Array(bytes) });
        })
        .catch(() => {
          // Сеть: ядро рисует заглушку, файл при следующем открытии получит картинку обычным путём.
        })
        .finally(() => {
          this.imagesActive--;
          this.pumpImages();
        });
    }
  }

  private emitStatus(detail?: string): void {
    const s = this.status();
    const key = `${s}|${detail ?? this.rejected ?? ''}`;
    if (key === this.lastStatus) return;
    this.lastStatus = key;
    this.env.status(s, detail ?? this.rejected ?? undefined);
  }

  private status(): SyncStatus {
    if (this.rejected) return 'rejected';
    if (this.paused) return 'paused';
    if (!this.ws || !this.opened) return this.lostOnce ? 'offline' : 'connecting';
    // «Синхронизировано» — когда все документы ответили и догоняющие правки уже в ядре. Обычная чужая правка индикатор не трогает:
    // она приходит, когда подписок в пути нет.
    if (this.readyDocs === 0 || this.pendingSubs > 0 || (this.catchingUp && (this.applying || this.inbound.length > 0)))
      return 'connecting';
    this.catchingUp = false;
    return this.unacked ? 'saving' : 'synced';
  }
}
````

Создать `apps/web/src/core/sync-messages.ts`:

````ts
// Сообщения `sync.worker`: с основным потоком только состояние и роль, трафик идёт прямым портом с worker'ом ядра.

import type { CoreCommand, CoreSyncEvent, SyncStatus } from './sync-client';
import type { AccessState } from './server-files';

export type ToSync = { t: 'start'; file: string; user: string; schema: number; port: MessagePort } | { t: 'stop' };

export type FromSync = { t: 'status'; status: SyncStatus; detail?: string } | { t: 'access'; state: AccessState } | { t: 'reload' };

/** Порт между worker'ом ядра и `sync.worker`. */
export type CoreToSync = { t: 'event'; event: CoreSyncEvent } | { t: 'applied' };
export type SyncToCore = { t: 'cmd'; cmd: CoreCommand } | { t: 'flush'; on: boolean };
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  type ToWorker,
} from '../core/messages';
import { imageSize } from '../core/image-size';
import { isFontPath, manifestFamilies } from '../core/fonts';
````

на:

````ts
  type ToWorker,
} from '../core/messages';
import type { SyncToCore } from '../core/sync-messages';
import { imageSize } from '../core/image-size';
import { isFontPath, manifestFamilies } from '../core/fonts';
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
}

/** Байты событий уходят без копии: их буферы передаются основному потоку. */
function sendEvents(): void {
````

на:

````ts
}

/** Прямой порт к `sync.worker` (файл на сервере): трафик синхронизации идёт мимо основного потока. */
let syncPort: MessagePort | null = null;
let syncFlushTimer: ReturnType<typeof setInterval> | null = null;
/** Пачки правок — не реже 30 раз в секунду, пока есть куда (спека подсистемы 4, раздел 8). */
const SYNC_FLUSH_MS = 33;

const SYNC_EVENTS: ReadonlySet<string> = new Set(['syncSubscribe', 'localImage', 'localUpdates', 'syncState', 'imagesNeeded']);

function sendToSync(events: CoreEvent[]): void {
  if (!syncPort) return;
  for (const e of events) {
    const transfer: Transferable[] = [];
    if (e.type === 'localUpdates' || e.type === 'localImage') transfer.push(e.bytes.buffer as ArrayBuffer);
    if (e.type === 'syncSubscribe') transfer.push(e.version.buffer as ArrayBuffer);
    syncPort.postMessage({ t: 'event', event: e }, transfer);
  }
}

function runSync(cmd: Parameters<WasmCore['command']>[0]): void {
  if (!core) return;
  try {
    core.command(cmd);
    sendEvents();
  } catch (e) {
    fatal(e);
  }
}

function connectSync(port: MessagePort): void {
  syncPort = port;
  port.onmessage = (e: MessageEvent<SyncToCore>) => {
    const m = e.data;
    if (m.t === 'flush') {
      if (syncFlushTimer !== null) clearInterval(syncFlushTimer);
      syncFlushTimer = null;
      if (m.on) {
        runSync({ type: 'syncFlush' });
        syncFlushTimer = setInterval(() => runSync({ type: 'syncFlush' }), SYNC_FLUSH_MS);
      }
      return;
    }
    runSync(m.cmd as Parameters<WasmCore['command']>[0]);
    if (m.cmd.type === 'applyRemote') port.postMessage({ t: 'applied' });
  };
}

/** Байты событий уходят без копии: их буферы передаются основному потоку. */
function sendEvents(): void {
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
  let events: CoreEvent[] = core.takeEvents();
  if (events.length === 0) return;
  // Вшитые шрифты качает этот worker; основному потоку — только системные.
  for (const e of events) if (e.type === 'fontRequest' && e.source === 'bundled') void loadBundled(e.family, e.files);
````

на:

````ts
  let events: CoreEvent[] = core.takeEvents();
  if (events.length === 0) return;
  // Трафик синхронизации основному потоку не нужен. `syncState` нужен обоим: индикатор считает `sync.worker`, а «Сохранить» ждёт сервер.
  if (syncPort && events.some((e) => SYNC_EVENTS.has(e.type))) {
    sendToSync(events.filter((e) => SYNC_EVENTS.has(e.type)));
    events = events.filter((e) => !SYNC_EVENTS.has(e.type) || e.type === 'syncState');
    if (events.length === 0) return;
  }
  // Вшитые шрифты качает этот worker; основному потоку — только системные.
  for (const e of events) if (e.type === 'fontRequest' && e.source === 'bundled') void loadBundled(e.family, e.files);
````

В `apps/web/src/worker/core.worker.ts` заменить:

````ts
        else postError('Неверное сообщение для ядра');
        break;
      case 'init':
        break;
````

на:

````ts
        else postError('Неверное сообщение для ядра');
        break;
      case 'syncPort':
        connectSync(msg.port);
        break;
      case 'init':
        break;
````

В `apps/web/src/worker/server.worker.ts` заменить:

````ts

import { MAX_JOURNAL_ENTRIES } from '../core/limits';
import { ACCESS_POLL_MS, asUser, ServerFiles, type JournalEntries, type JournalStore } from '../core/server-files';
import type { FromServer, ServerRequest, ToServer } from '../core/server-messages';

````

на:

````ts

import { MAX_JOURNAL_ENTRIES } from '../core/limits';
import { ACCESS_POLL_MS, asUser, ServerFiles, type JournalEntries, type JournalKey, type JournalStore } from '../core/server-files';
import type { FromServer, ServerRequest, ToServer } from '../core/server-messages';

````

В `apps/web/src/worker/server.worker.ts` заменить:

````ts
      const session = typeof s === 'number' && Number.isSafeInteger(s) ? s : null;
      if (count > MAX_JOURNAL_ENTRIES) return { entries: [], session, tooLarge: true };
      const all = (await request(d.transaction('journal').objectStore('journal').getAll(range(file)))) as unknown[];
      return { entries: all.filter((b): b is ArrayBuffer => b instanceof ArrayBuffer), session, tooLarge: false };
    },
    async cut(file, session, upTo) {
````

на:

````ts
      const session = typeof s === 'number' && Number.isSafeInteger(s) ? s : null;
      if (count > MAX_JOURNAL_ENTRIES) return { entries: [], session, tooLarge: true };
      const tx = d.transaction('journal');
      const [all, keys] = await Promise.all([
        request(tx.objectStore('journal').getAll(range(file))) as Promise<unknown[]>,
        request(tx.objectStore('journal').getAllKeys(range(file))) as Promise<unknown[]>,
      ]);
      return {
        entries: all.filter((b): b is ArrayBuffer => b instanceof ArrayBuffer),
        session,
        tooLarge: false,
        keys: keys as JournalKey[],
      };
    },
    async cut(file, session, upTo) {
````

В `apps/web/src/worker/server.worker.ts` заменить:

````ts
      const tx = d.transaction('journal', 'readwrite', { durability: 'strict' });
      tx.objectStore('journal').delete(IDBKeyRange.bound([file], [file, session, upTo]));
      await finished(tx);
    },
````

на:

````ts
      const tx = d.transaction('journal', 'readwrite', { durability: 'strict' });
      tx.objectStore('journal').delete(IDBKeyRange.bound([file], [file, session, upTo]));
      await finished(tx);
    },
    async cutExact(keys) {
      const d = await open();
      const tx = d.transaction('journal', 'readwrite', { durability: 'strict' });
      const store = tx.objectStore('journal');
      for (const key of keys) store.delete(key);
      await finished(tx);
    },
````

В `apps/web/src/worker/server.worker.ts` заменить:

````ts
let files: ServerFiles | null = null;
const watched = new Set<string>();
let visible = true;
let poll: ReturnType<typeof setInterval> | null = null;
````

на:

````ts
let files: ServerFiles | null = null;
const watched = new Set<string>();
const synced = new Set<string>();
let visible = true;
let poll: ReturnType<typeof setInterval> | null = null;
````

В `apps/web/src/worker/server.worker.ts` заменить:

````ts
  poll = setInterval(() => {
    // Открытые файлы — проверка доступа; экран файлов — новое в списке (чужие приглашения и правки).
    if (watched.size === 0) post({ t: 'changed' });
    for (const f of watched) void files?.checkAccess(f);
  }, ACCESS_POLL_MS);
````

на:

````ts
  poll = setInterval(() => {
    // Открытые файлы — проверка доступа; экран файлов — новое в списке (чужие приглашения и правки).
    if (watched.size === 0 && synced.size === 0) post({ t: 'changed' });
    for (const f of watched) void files?.checkAccess(f);
  }, ACCESS_POLL_MS);
````

В `apps/web/src/worker/server.worker.ts` заменить:

````ts
    case 'appendJournal':
      await f.appendJournal(r.file, r.session, r.batch, r.bytes);
      return { value: null };
    case 'blob':
````

на:

````ts
    case 'appendJournal':
      await f.appendJournal(r.file, r.session, r.batch, r.bytes);
      return { value: null };
    case 'cutJournal':
      await f.cutJournal(r.file, r.session, r.upTo);
      return { value: null };
    case 'putThumb':
      f.putThumb(r.file, r.thumb);
      return { value: null };
    case 'blob':
````

В `apps/web/src/worker/server.worker.ts` заменить:

````ts
  }
  if (m.t === 'watch') {
    if (m.on) {
      watched.add(m.file);
      void files?.watchAccess(m.file);
    } else {
      watched.delete(m.file);
      files?.forgetAccess(m.file);
````

на:

````ts
  }
  if (m.t === 'watch') {
    if (m.on && m.poll === false) {
      // Доступ сообщает соединение файла: опрос снят, список файлов на экране файлов и без него.
      watched.delete(m.file);
      synced.add(m.file);
      return;
    }
    if (m.on) {
      watched.add(m.file);
      void files?.watchAccess(m.file);
    } else {
      synced.delete(m.file);
      watched.delete(m.file);
      files?.forgetAccess(m.file);
````

Создать `apps/web/src/worker/sync.worker.ts`:

````ts
// WebSocket файла (спека подсистемы 4, раздел 7.1): всё здесь и в worker'е ядра, с ним — прямой порт, буферы передаются. Основной поток
// получает только состояние соединения и роль.

import { asUser, ServerFiles, type JournalStore } from '../core/server-files';
import { SyncClient, type CoreCommand, type SocketLike } from '../core/sync-client';
import type { CoreToSync, FromSync, SyncToCore, ToSync } from '../core/sync-messages';

const scope = self as unknown as DedicatedWorkerGlobalScope;
const post = (m: FromSync) => scope.postMessage(m);

/** Журнала здесь нет: им занят worker файлов, здесь только запросы картинок и снимков. */
const noJournal: JournalStore = {
  append: () => Promise.reject(new Error('нет журнала')),
  read: () => Promise.resolve({ entries: [], session: null, tooLarge: false }),
  cut: () => Promise.resolve(),
  cutExact: () => Promise.resolve(),
  drop: () => Promise.resolve(),
};

let client: SyncClient | null = null;

function transferOf(c: CoreCommand): Transferable[] {
  switch (c.type) {
    case 'syncSubscribed':
      return [c.version.buffer as ArrayBuffer];
    case 'applyRemote':
      return c.batches.map((b) => b.buffer as ArrayBuffer);
    case 'addRemoteImage':
      return [c.bytes.buffer as ArrayBuffer];
    default:
      return [];
  }
}

scope.onmessage = (e: MessageEvent<ToSync>) => {
  const m = e.data;
  if (m.t === 'stop') {
    client?.stop();
    client = null;
    return;
  }
  const port = m.port;
  const toCore = (s: SyncToCore, transfer: Transferable[] = []) => port.postMessage(s, transfer);
  const files = new ServerFiles(
    asUser(m.user, (input, init) => fetch(input, init)),
    noJournal,
    { changed: () => {}, access: () => {} },
    {
      now: () => performance.now(),
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (t) => clearTimeout(t as ReturnType<typeof setTimeout>),
    },
  );
  client = new SyncClient({
    file: m.file,
    user: m.user,
    schema: m.schema,
    origin: self.location.origin,
    socket: (url) => new WebSocket(url) as unknown as SocketLike,
    core: {
      command: (cmd) => toCore({ t: 'cmd', cmd }, transferOf(cmd)),
      flush: (on) => toCore({ t: 'flush', on }),
    },
    files: {
      uploadImage: (hash, bytes) => files.uploadOneImage(m.file, hash, bytes),
      downloadImage: (hash) => files.downloadOneImage(m.file, hash),
      snapshot: (path) => files.fetchSnapshot(path),
      access: () => files.accessOnce(m.file),
    },
    clock: {
      now: () => performance.now(),
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (t) => clearTimeout(t as ReturnType<typeof setTimeout>),
    },
    random: Math.random,
    status: (status, detail) => post({ t: 'status', status, detail }),
    access: (state) => post({ t: 'access', state }),
    reload: () => post({ t: 'reload' }),
  });
  port.onmessage = (ev: MessageEvent<CoreToSync>) => {
    if (ev.data.t === 'applied') client?.onApplied();
    else client?.onCore(ev.data.event);
  };
  client.start();
};

scope.addEventListener('online', () => client?.online());
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Expected: PASS: всё без замечаний; модульные 271 passed; обе сборки и `check-dist.mjs`

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e-server/image-limits.spec.ts apps/web/src/core/core.ts apps/web/src/core/messages.ts apps/web/src/core/server-files.ts apps/web/src/core/server-messages.ts apps/web/src/core/sync-client.ts apps/web/src/core/sync-messages.ts apps/web/src/worker/core.worker.ts apps/web/src/worker/server.worker.ts apps/web/src/worker/sync.worker.ts apps/web/unit/server-files.test.ts apps/web/unit/sync-client.test.ts
git commit -F - <<'EOF'
клиент: соединение файла в своём worker'е

`core/sync-client.ts` без DOM, `worker/sync.worker.ts`, прямой порт с worker'ом ядра: основной поток трафика не видит (спека подсистемы
4, раздел 7.1). Обрыв, рестарт сервера, отказ «повторите» и конец паузы — один путь: подписка заново, ядро шлёт всё сверх версии сервера.
Очередей правок вне документа нет: сокет копит до 1 МБ, отправка — до 32 МБ, дальше ядро ждёт. Переподключение от 0,5 до 30 с с разбросом,
сразу при `online`; мёртвый сокет ловит `ping`. Предел частоты — `paused`, а не «нет сети», и никакой срок пауза не считает простоем. Отказ,
который повтор не исправит, — `rejected` с причиной, чужие правки идут дальше. Соединение не открывается — спрашиваем `GET access`: сессия
и снятый доступ не прячутся за вечным «нет сети». Чужие правки складываются, пока ядро занято.

Журнал браузера снимается подтверждением сервера и только своими и прочитанными при открытии пачками: другая вкладка могла ещё ничего не
отправить. «Скачать копию» сначала просит сервер сжать журнал.

Тесты — на сокете с настоящим `bufferedAmount` и ручных часах: нашли потерю следующей правки, лёгшей в очередь, пока флаг отправки ещё стоял.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 9: Интерфейс — журнал сервера, индикатор, ничего не теряется молча

Спека §8.1 (B-238). Первая запись при открытии, индикатор в шапке, «Сохранить» и уход ждут сервер, плашки отказа, вкладка как участник, E2E двумя браузерами на настоящем стенде.

**Files:**
- Modify: `apps/web/src/ui/app.tsx`, `apps/web/src/ui/boot.ts`, `apps/web/src/ui/chrome.tsx`, `apps/web/src/ui/file-store.ts`, `apps/web/src/ui/root.tsx`, `apps/web/src/ui/saver.ts`, `apps/web/src/ui/server/server-store.ts`, `apps/web/src/ui/store.ts`, `apps/web/src/ui/styles/server.css`
- Test: `apps/web/e2e-server/auth.spec.ts`, `apps/web/e2e-server/files.spec.ts`, `apps/web/e2e-server/sync.spec.ts` (новый), `apps/web/unit/saver.test.ts`

**Interfaces:**
- Consumes: `sync.worker.ts`, события `SyncState`, `Notice`.
- Produces: E2E `e2e-server/sync.spec.ts`; индикатор и плашки для этапа 6.

- [ ] **Step 1: Написать тесты**

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  const l = await live(page);
  await drag(page, await appScreen(page, l, l.cameraX - 50, l.cameraY - 40), 100, 80);
  await nodesAre(page, l.nodes + 1);
  // Живое число узлов идёт через общую память и обгоняет событие `modified`, а запись видит правку
  // только по нему: человек не успеет уйти быстрее одного кадра (B-200).
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
}

````

на:

````ts
  const l = await live(page);
  await drag(page, await appScreen(page, l, l.cameraX - 50, l.cameraY - 40), 100, 80);
  // Число узлов считает и черновик рисования: жест закончен, только когда редактор свободен.
  await expect
    .poll(async () => {
      const now = await live(page);
      return now.busy === 0 ? now.nodes : -1;
    })
    .toBe(l.nodes + 1);
}

````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts

test('выход, когда запись не удалась: вопрос, «Остаться» отменяет выход', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
````

на:

````ts

test('выход, когда запись не удалась: вопрос, «Остаться» отменяет выход', async ({ page }, info) => {
  test.skip(info.project.name === 'server-firefox', '`setOffline` не действует на worker в Firefox');
  const email = newEmail(info.project.name);
  await testMode(page);
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await editorReady(page);
  await drawRect(page);
  // Запись на сервер не проходит: и в worker'е файлов, и из любой вкладки контекста.
  await page.context().route('**/api/files/*/content', (r) => r.abort());
  await mainMenu(page, 'Настройки', 'Настройки…');
  await page.getByRole('tab', { name: 'Аккаунт' }).click();
  await page.getByRole('tabpanel', { name: 'Аккаунт' }).getByRole('button', { name: 'Выйти', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  await expect(dialog).toContainText('Изменения не сохранены');
  await dialog.getByRole('button', { name: 'Остаться' }).click();
  await expect(dialog).toHaveCount(0);
  // Сессия жива, редактор на месте.
  const me = (await (await page.request.get('/api/auth/session')).json()) as { user: { email: string } | null };
````

на:

````ts
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await editorReady(page);
  // Сервера нет, правка в браузере: сервер её не подтвердит.
  await expect(page.locator('.fe-sync')).toHaveText('Синхронизировано');
  await page.context().setOffline(true);
  await expect(page.locator('.fe-sync')).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(page);
  await mainMenu(page, 'Настройки', 'Настройки…');
  await page.getByRole('tab', { name: 'Аккаунт' }).click();
  await page.getByRole('tabpanel', { name: 'Аккаунт' }).getByRole('button', { name: 'Выйти', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  await expect(dialog).toContainText('Изменения не сохранены', { timeout: 15_000 });
  await expect(dialog).toContainText('сохранены в этом браузере');
  await dialog.getByRole('button', { name: 'Остаться' }).click();
  await expect(dialog).toHaveCount(0);
  await page.context().setOffline(false);
  // Сессия жива, редактор на месте.
  const me = (await (await page.request.get('/api/auth/session')).json()) as { user: { email: string } | null };
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  await testMode(page);
  await page.goto('/?nodes=0');
  await signIn(page, email);
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await editorReady(page);
  const tab = await context.newPage();
````

на:

````ts
  await testMode(page);
  await page.goto('/?nodes=0');
  await signIn(page, email);
  // Первая запись нового файла не проходит: у него нет ничего, кроме памяти вкладки.
  await context.route('**/api/files/*/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await editorReady(page);
  const tab = await context.newPage();
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
});

test('выход в другой вкладке: записанный файл, правка возвращается из журнала после входа', async ({ page, context }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
````

на:

````ts
});

test('выход в другой вкладке: записанный файл, правка на месте после входа', async ({ page, context }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
````

В `apps/web/e2e-server/auth.spec.ts` заменить:

````ts
  await expect(tab.getByRole('button', { name: 'Выйти' })).toBeVisible();
  await drawRect(page);
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(page.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await signIn(page, email, '');
````

на:

````ts
  await expect(tab.getByRole('button', { name: 'Выйти' })).toBeVisible();
  await drawRect(page);
  await expect(page.locator('.fe-sync')).toHaveText('Синхронизировано');
  await tab.getByRole('button', { name: 'Выйти' }).click();
  // Правка на сервере: вопросов нет, вкладка просит войти.
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await signIn(page, email, '');
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
import { appScreen, drag, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { compose, linksFor, mailCount, mailsFor, newEmail, resetStand, signIn } from './stand';

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
}

/** Файл на сервере с одним прямоугольником; адрес файла — у открывшего. */
async function storedFile(page: Page): Promise<string> {
````

на:

````ts
}

const inSync = (page: Page) => expect(page.locator('.fe-sync')).toHaveText('Синхронизировано', { timeout: 30_000 });

/** Размер файла на сервере, с журналом. */
const contentSize = (page: Page, id: string) =>
  page.evaluate(async (i) => {
    await fetch(`/api/files/${i}/compact`, { method: 'POST' });
    return Number((await (await fetch(`/api/files/${i}`)).json()).size);
  }, id);

/** Файл на сервере с одним прямоугольником; адрес файла — у открывшего. */
async function storedFile(page: Page): Promise<string> {
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  await editorReady(page);
  const id = fileId(page);
  await drawRect(page);
  await saveKey(page);
  await saved(page);
  return id;
}
````

на:

````ts
  await editorReady(page);
  const id = fileId(page);
  await inSync(page);
  const before = await contentSize(page, id);
  await drawRect(page);
  // Правка на сервере и в его файле: журнал сжат, `content` содержит всё.
  await expect.poll(() => contentSize(page, id), { timeout: 30_000 }).toBeGreaterThan(before);
  await inSync(page);
  return id;
}
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  await page.goto(`/#/file/${id}`);
  await editorReady(page);
};

````

на:

````ts
  await page.goto(`/#/file/${id}`);
  await editorReady(page);
  await inSync(page);
};

````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  await page.getByRole('button', { name: 'Пригласить' }).click();
  await expect(page.getByRole('list', { name: 'Люди с доступом' }).getByText(email)).toBeVisible();
  await page.getByRole('button', { name: 'Закрыть' }).click();
}

````

на:

````ts
  await page.getByRole('button', { name: 'Пригласить' }).click();
  await expect(page.getByRole('list', { name: 'Люди с доступом' }).getByText(email)).toBeVisible();
  await page.getByRole('button', { name: 'Закрыть', exact: true }).click();
}

````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  await a.goto('/#/drafts');
  await expect(title(a)).toHaveText('Черновики');
````

на:

````ts
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  // Превью — по «Сохранить» и при уходе: отдельным запросом.
  const thumb = a.waitForResponse((r) => r.url().endsWith('/thumb') && r.request().method() === 'PUT');
  await saveKey(a);
  await thumb;
  await a.goto('/#/drafts');
  await expect(title(a)).toHaveText('Черновики');
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  const before = (await live(b)).nodes;
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'viewer' });
  await checkAccessNow(b);
  await expect(b.getByText('Только просмотр')).toBeVisible();
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
````

на:

````ts
  const before = (await live(b)).nodes;
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'viewer' });
  await expect(b.getByText('Только просмотр')).toBeVisible();
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
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
````

на:

````ts
  await nodes(b, before);
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'editor' });
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
  await nodes(b, before + 1);
  await inSync(b);
  await a.context().close();
  await b.context().close();
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
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
````

на:

````ts
  await drawRect(b);
  await nodes(b, 2);
  await inSync(b);
  const share = (await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { id: string; email: string }[] };
  const entry = share.people.find((p) => p.email === editorMail)!.id;
  await api(a, 'DELETE', `/api/files/${id}/share/people/${entry}`);
  // Сразу, по соединению файла.
  await expect(b.getByRole('alert').getByText('Нет доступа', { exact: true })).toBeVisible();
  await expect(b.getByRole('button', { name: 'Сохранить копию на диск' })).toBeVisible();
  await nodes(b, 2);
  const download = b.waitForEvent('download');
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  expect((await download).suggestedFilename()).toMatch(/\.ferrum$/);
  await b.getByRole('button', { name: 'К файлам', exact: true }).click();
  await b.getByRole('button', { name: 'Уйти' }).click();
  await expect(title(b)).toHaveText('Недавние');
  // Файл закрыт и после этого: чтение — «Нет доступа», просьба уходит владельцу.
````

на:

````ts
  expect((await download).suggestedFilename()).toMatch(/\.ferrum$/);
  await b.getByRole('button', { name: 'К файлам', exact: true }).click();
  await expect(title(b)).toHaveText('Недавние');
  // Файл закрыт и после этого: чтение — «Нет доступа», просьба уходит владельцу.
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
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

test('файл без записи записал другой человек: честный вопрос, правки уходят новым файлом (C-1)', async ({ browser }, info) => {
  const editorMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, editorMail);
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  const id = fileId(a);
  await shareWith(a, editorMail);
  await open(b, id);
  await drawRect(a, 200);
  await saveKey(a);
  await saved(a);
  await drawRect(b, -200);
  await saveKey(b);
  const plaque = b.getByRole('alert').filter({ hasText: 'Файл уже записан' });
  await expect(plaque).toBeVisible();
  await expect(plaque).toContainText('пропадут');
  await expect(toast(b, 'пропадут')).toBeVisible();
  expect(await b.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  // Запись X не тронута, версия прежняя.
  const version = (await api(b, 'GET', `/api/files/${id}/access`)).body as { version: number };
  expect(version.version).toBe(1);
  // Вопрос при уходе не обещает слияния.
  await b.getByRole('button', { name: 'Назад к файлам' }).click();
  const ask = b.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  await expect(ask).toContainText('пропадут');
  await expect(ask).not.toContainText('правки сольются');
  await b.getByRole('button', { name: 'Остаться' }).click();
  // Опрос не снимает плашку и режим просмотра, даже когда поколение снова новое.
  await drawRect(a, 400);
  await saveKey(a);
  await saved(a);
  await checkAccessNow(b);
  await expect(plaque).toBeVisible();
  // Правки — новым файлом в «Черновиках»: Y в нём, правка на месте, в исходном файле только правки X.
  await b.getByRole('button', { name: 'Сохранить новым файлом' }).click();
  await expect.poll(() => fileId(b)).not.toBe(id);
  await editorReady(b);
  await nodes(b, 1);
  const copy = fileId(b);
  const list = (await api(b, 'GET', '/api/files?section=drafts&start=0&count=10')).body as { rows: { id: string; name: string }[] };
  expect(list.rows.find((r) => r.id === copy)?.name).toContain('(копия)');
  await a.reload();
  await editorReady(a);
  await nodes(a, 2);
  await a.context().close();
  await b.context().close();
});

test('смена входа в другой вкладке: правки X не записываются под Y и не теряются (F-I4)', async ({ browser }, info) => {
  const xMail = newEmail(info.project.name);
  const yMail = newEmail(info.project.name);
  const a = await person(browser, xMail);
  const id = await storedFile(a);
  await shareWith(a, yMail);
  const before = (await api(a, 'GET', `/api/files/${id}/access`)).body as { version: number };
  await drawRect(a, 200);
  await nodes(a, 2);
  const tab = await a.context().newPage();
  await testMode(tab);
  await tab.goto('/');
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(tab.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  // Правки только в журнале X: вкладка перезагружается как Y, у которого право на файл есть.
  const reloaded = a.waitForEvent('load', { timeout: 90_000 });
  await signIn(tab, yMail, 'Другой');
````

на:

````ts
});

test('смена входа в другой вкладке: соединение X закрыто, вкладка становится вкладкой Y (F-I4)', async ({ browser }, info) => {
  const xMail = newEmail(info.project.name);
  const yMail = newEmail(info.project.name);
  const a = await person(browser, xMail);
  await storedFile(a);
  await shareWith(a, yMail);
  await drawRect(a, 200);
  await nodes(a, 2);
  await inSync(a);
  const tab = await a.context().newPage();
  await testMode(tab);
  await tab.goto('/');
  // Правки на сервере: вкладка X без вопросов перезагружается и просит войти.
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(tab.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await expect(a.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  const reloaded = a.waitForEvent('load', { timeout: 90_000 });
  await signIn(tab, yMail, 'Другой');
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  await reloaded;
  await editorReady(a);
  await nodes(a, 1);
  const after = (await api(tab, 'GET', `/api/files/${id}/access`)).body as { version: number; role: string };
  expect(after.version).toBe(before.version);
  await a.close();
  // X возвращается: его правка цела и сливается.
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(tab.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await signIn(tab, xMail, '');
  await expect(tab.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await open(tab, id);
  await nodes(tab, 2);
  await tab.context().close();
});

````

на:

````ts
  await reloaded;
  await editorReady(a);
  await nodes(a, 2);
  await a.context().close();
});

````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  const yMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  const id = fileId(a);
  await shareWith(a, yMail);
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await drawRect(a);
  await saveKey(a);
````

на:

````ts
  const yMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  const id = fileId(a);
  await shareWith(a, yMail);
  await drawRect(a);
  await saveKey(a);
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  await a.evaluate(() => new BroadcastChannel('ferrum-server-files').postMessage('changed'));
  await expect(a.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
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
````

на:

````ts
  await a.evaluate(() => new BroadcastChannel('ferrum-server-files').postMessage('changed'));
  await expect(a.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await a.context().close();
});
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  await expect(b.getByRole('option', { name: 'Без названия' })).toBeVisible();
  await open(b, id);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
````

на:

````ts
  await expect(b.getByRole('option', { name: 'Без названия' })).toBeVisible();
  await open(b, id);
  await nodes(b, 1);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  await drawRect(a);
  await a.evaluate(() => fetch('/api/auth/sign-out', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' }));
  await saveKey(a);
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  expect(await a.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  const before = await mailCount(mail);
  await a.getByRole('button', { name: 'Войти' }).click();
````

на:

````ts
  await drawRect(a);
  await a.evaluate(() => fetch('/api/auth/sign-out', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' }));
  // Соединение закрыто сразу, плашка не ждёт сохранения.
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  const before = await mailCount(mail);
  await a.getByRole('button', { name: 'Войти' }).click();
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
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
````

на:

````ts
  test.skip(info.project.name === 'server-firefox', '`setOffline` не действует на worker в Firefox');
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  await a.context().setOffline(true);
  await expect(a.locator('.fe-sync')).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(a);
  await a.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(a.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' })).toContainText('сохранены в этом браузере', {
    timeout: 15_000,
  });
  await a.getByRole('button', { name: 'Остаться' }).click();
  await a.context().setOffline(false);
  await inSync(a);
  await a.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(title(a)).toHaveText('Недавние');
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
});

test('редактор по ссылке: кнопки нет; 403 в диалоге — объяснение без повтора и формы (T7-I1, T7-I2)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, newEmail(info.project.name));
````

на:

````ts
});

test('редактор по ссылке: кнопки нет, сервер отказывает (T7-I1, T7-I2)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, newEmail(info.project.name));
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  await editorReady(b);
  await answered;
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
  // Сервер отказывает и когда кнопка всё-таки на экране (старый ответ опроса).
  await b.context().route('**/access', async (r) => {
    const res = await r.fetch();
    await r.fulfill({ response: res, json: { ...((await res.json()) as object), canShare: true } });
  });
  await checkAccessNow(b);
  await b.getByRole('button', { name: 'Поделиться' }).click();
  const dialog = b.getByRole('dialog');
  await expect(dialog.getByText('Делиться файлом может тот, кому его открыли напрямую')).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Повторить' })).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Пригласить' })).toHaveCount(0);
  await a.context().close();
  await b.context().close();
````

на:

````ts
  await editorReady(b);
  await answered;
  await inSync(b);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
  // Сервер отказывает и без кнопки.
  expect((await api(b, 'GET', `/api/files/${id}/share`)).status).toBe(403);
  await a.context().close();
  await b.context().close();
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts

test('право вернули читателю: журнал не стирается, файл открывается заново с правками (T6-I2)', async ({ browser }, info) => {
  const mail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
````

на:

````ts

test('право вернули читателю: журнал не стирается, файл открывается заново с правками (T6-I2)', async ({ browser }, info) => {
  test.skip(info.project.name === 'server-firefox', '`setOffline` не действует на worker в Firefox');
  const mail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  // Редактор правит, запись не проходит: правка только в журнале. Уходит.
  await open(b, id);
  await b.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await drawRect(b);
  await saveKey(b);
  await expect(toast(b, 'Нет связи с сервером')).toBeVisible();
  await b.getByRole('button', { name: 'Назад к файлам' }).click();
  await b.getByRole('button', { name: 'Уйти' }).click();
  await expect(title(b)).toHaveText('Недавние');
  await b.context().unroute('**/content');
  // Роль понизили: журнал не применён.
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'viewer' });
````

на:

````ts
  // Редактор правит, запись не проходит: правка только в журнале. Уходит.
  await open(b, id);
  await b.context().setOffline(true);
  await expect(b.locator('.fe-sync')).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(b);
  await b.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(b.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' })).toContainText('сохранены в этом браузере', {
    timeout: 15_000,
  });
  await b.getByRole('button', { name: 'Уйти' }).click();
  await expect(title(b)).toHaveText('Недавние');
  await b.context().setOffline(false);
  // Роль понизили: журнал не применён.
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'viewer' });
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
  // Роль вернули в той же вкладке: прежняя правка возвращается сразу.
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'editor' });
  await checkAccessNow(b);
  await expect(toast(b, 'Ваши несохранённые правки восстановлены')).toBeVisible();
  await nodes(b, 2);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b, 200);
  await saveKey(b);
  await saved(b);
  await b.reload();
  await editorReady(b);
````

на:

````ts
  // Роль вернули в той же вкладке: прежняя правка возвращается сразу.
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'editor' });
  await expect(toast(b, 'Ваши несохранённые правки восстановлены')).toBeVisible();
  await nodes(b, 2);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b, 200);
  await inSync(b);
  await b.reload();
  await editorReady(b);
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
});

test('своя запись: ответ ещё в пути, опрос видит новое поколение, «изменён другим» не появляется (T6-Minor5)', async ({
  browser,
}, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  await drawRect(a);
  const { promise: letGo, resolve } = Promise.withResolvers<void>();
  const { promise: committed, resolve: done } = Promise.withResolvers<void>();
  await a.context().route('**/content', async (r) => {
    if (r.request().method() !== 'PUT') return r.continue();
    const res = await r.fetch();
    done();
    await letGo;
    await r.fulfill({ response: res });
  });
  await saveKey(a);
  await committed;
  const polled = a.waitForResponse((r) => r.url().endsWith('/access'));
  await checkAccessNow(a);
  await polled;
  resolve();
  await saved(a);
  // Следующий опрос уже после ответа: всё тихо, чужой записи не было.
  const again = a.waitForResponse((r) => r.url().endsWith('/access'));
  await checkAccessNow(a);
  await again;
  // Без повторов: уведомление исчезает само, и ожидание «его нет» дождалось бы этого.
  expect(await a.locator('body').innerText()).not.toContain('Файл изменён в другом окне');
  await a.context().close();
});

test('неудачная запись повторяется сама; новый файл не закрывается молча (T6-I4)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=0&fullSave=200#/new');
  await editorReady(a);
  const id = fileId(a);
  let refused = 0;
  await a.context().route('**/content', (r) => {
````

на:

````ts
});

test('неудачная запись повторяется сама; новый файл не закрывается молча (T6-I4)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  let refused = 0;
  await a.context().route('**/content', (r) => {
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
    return r.abort();
  });
  await drawRect(a);
  // Без единой правки и нажатия: первая запись оборвалась, вторая прошла сама.
  await saved(a);
  expect(refused).toBeGreaterThan(1);
  expect(((await api(a, 'GET', `/api/files/${id}/access`)).body as { version: number }).version).toBeGreaterThan(0);
  await a.context().unroute('**/content');
  // Новый файл, запись не проходит: закрытие вкладки спрашивает.
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await drawRect(a);
  await saveKey(a);
````

на:

````ts
    return r.abort();
  });
  await a.goto('/?nodes=0&fullSave=200#/new');
  await editorReady(a);
  const id = fileId(a);
  // Без единой правки и нажатия: первая запись оборвалась, вторая прошла сама.
  await inSync(a);
  expect(refused).toBeGreaterThan(1);
  expect(((await api(a, 'GET', `/api/files/${id}/access`)).body as { version: number }).version).toBeGreaterThan(0);
  await a.context().unroute('**/content');
  // Новый файл, запись не проходит: закрытие вкладки спрашивает.
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  await drawRect(a);
  await saveKey(a);
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
test('смена входа в другой вкладке: новый файл без записи спрашивает, а не перезагружается (T6-I4)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await drawRect(a);
  await saveKey(a);
````

на:

````ts
test('смена входа в другой вкладке: новый файл без записи спрашивает, а не перезагружается (T6-I4)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  await drawRect(a);
  await saveKey(a);
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
});

test('картинку убрали с сервера, вкладка считает её известной: запись догружает её (T6-I3)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  const id = fileId(a);
  const uploads: string[] = [];
  a.on('request', (r) => {
    if (r.method() === 'PUT' && r.url().includes('/images/')) uploads.push(r.url());
  });
  await a.evaluate(async () => {
    const c = new OffscreenCanvas(4, 4);
    c.getContext('2d')!.fillRect(0, 0, 4, 4);
    const bytes = new Uint8Array(await (await c.convertToBlob({ type: 'image/png' })).arrayBuffer());
    window.__ferrum!.core!.send({ type: 'placeImage', bytes });
  });
  await expect.poll(() => a.evaluate(() => window.__ferrum!.ui!.tool)).toBe('image');
  const l = await live(a);
  const at = await appScreen(a, l, l.cameraX, l.cameraY);
  await a.mouse.click(at.x, at.y);
  await nodes(a, 1);
  await saveKey(a);
  await saved(a);
  expect(uploads).toHaveLength(1);
  const hash = uploads[0].split('/images/')[1];
  // Как очистка через 30 дней: строки картинки файла больше нет.
  compose('exec', '-T', 'postgres', 'psql', '-U', 'ferrum', '-d', 'ferrum', '-c', `DELETE FROM file_image WHERE file_id = '${id}'`);
  await drawRect(a, 300);
  await saveKey(a);
  await saved(a);
  expect(uploads).toHaveLength(2);
  const image = await a.evaluate(async (u) => (await fetch(u)).status, uploads[1]);
  expect(image).toBe(200);
  expect(hash).toHaveLength(64);
  await a.context().close();
});

const closeDialog = (p: Page) => p.getByRole('button', { name: 'Закрыть' }).click();

/** Команда с проектом у владельца; диалог остаётся на странице команды. */
````

на:

````ts
});

const closeDialog = (p: Page) => p.getByRole('button', { name: 'Закрыть', exact: true }).click();

/** Команда с проектом у владельца; диалог остаётся на странице команды. */
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
/** Содержимое файла с сервера и пачка журнала, которую ядро отдало поверх него. */
async function fileAndJournal(a: Page): Promise<{ id: string; bytes: number[]; batch: number[] }> {
  const id = await storedFile(a);
  const bytes = await a.evaluate(async (i) => Array.from(new Uint8Array(await (await fetch(`/api/files/${i}/content`)).arrayBuffer())), id);
  // Запись не проходит: правка только в журнале браузера, файл на сервере прежний.
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await drawRect(a, 200);
  await saveKey(a);
  await expect(toast(a, 'Нет связи с сервером')).toBeVisible();
  await a.context().unroute('**/content');
  const batch = await a.evaluate(async () => {
    const { user } = (await (await fetch('/api/auth/session')).json()) as { user: { id: string } };
````

на:

````ts
/** Содержимое файла с сервера и пачка журнала, которую ядро отдало поверх него. */
async function fileAndJournal(a: Page): Promise<{ id: string; bytes: number[]; batch: number[] }> {
  test.skip(test.info().project.name === 'server-firefox', '`setOffline` не действует на worker в Firefox');
  const id = await storedFile(a);
  const bytes = await a.evaluate(async (i) => Array.from(new Uint8Array(await (await fetch(`/api/files/${i}/content`)).arrayBuffer())), id);
  // Сервера нет: правка только в журнале браузера, файл на сервере прежний.
  await a.context().setOffline(true);
  await expect(a.locator('.fe-sync')).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(a, 200);
  await expect
    .poll(
      () =>
        a.evaluate(async () => {
          return new Promise<number>((resolve) => {
            const dbs = indexedDB.databases();
            void dbs.then((list) => {
              const name = list.find((d) => d.name?.startsWith('ferrum-user-'))?.name;
              if (!name) return resolve(0);
              const req = indexedDB.open(name);
              req.onsuccess = () => {
                const n = req.result.transaction('journal').objectStore('journal').count();
                n.onsuccess = () => resolve(n.result);
              };
            });
          });
        }),
      { timeout: 15_000 },
    )
    .toBeGreaterThan(0);
  await a.context().setOffline(false);
  const batch = await a.evaluate(async () => {
    const { user } = (await (await fetch('/api/auth/session')).json()) as { user: { id: string } };
````

В `apps/web/e2e-server/files.spec.ts` заменить:

````ts
    }).observe({ entryTypes: ['longtask'] });
  });
  await drawRect(a);
  const t0 = Date.now();
  await saveKey(a);
  await expect.poll(() => a.evaluate(() => window.__ferrum!.ui!.modified), { timeout: 120_000 }).toBe(false);
  const saveMs = Date.now() - t0;
  const long = await a.evaluate(() => (window as unknown as { __long: number[] }).__long);
````

на:

````ts
    }).observe({ entryTypes: ['longtask'] });
  });
  await expect(a.locator('.fe-sync')).toHaveText('Синхронизировано', { timeout: 120_000 });
  await drawRect(a);
  const t0 = Date.now();
  await inSync(a);
  const saveMs = Date.now() - t0;
  const long = await a.evaluate(() => (window as unknown as { __long: number[] }).__long);
````

Создать `apps/web/e2e-server/sync.spec.ts`:

````ts
// Совместная работа на настоящем стенде (спека подсистемы 4, разделы 7, 8, 13.2): два браузера правят один файл живьём, отмена
// трогает только своё, обрыв и рестарт сервера ничего не теряют, снятие роли действует сразу.

import { expect, test, type Browser, type Page } from '@playwright/test';
import { appScreen, drag, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { compose, newEmail, resetStand, signIn } from './stand';

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

const nodes = (page: Page, n: number, timeout = 15_000) => expect.poll(async () => (await live(page)).nodes, { timeout }).toBe(n);
const indicator = (page: Page) => page.locator('.fe-sync');
const synced = (page: Page, timeout = 15_000) => expect(indicator(page)).toHaveText('Синхронизировано', { timeout });

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

/** Новый файл: страница уже в нём, можно звать людей. */
async function newFile(page: Page): Promise<string> {
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  await synced(page);
  return fileId(page);
}

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

/** Человек получает доступ по ссылке «может редактировать» или «может смотреть» и открывает файл. */
async function join(owner: Page, guest: Page, id: string, role: 'editor' | 'viewer' = 'editor'): Promise<void> {
  const email = await guest.evaluate(async () => {
    const r = (await (await fetch('/api/auth/session')).json()) as { user: { email: string } };
    return r.user.email;
  });
  const res = await api(owner, 'POST', `/api/files/${id}/share/invite`, { email, role });
  expect(res.status).toBe(200);
  await guest.goto(`/#/file/${id}`);
  await guest.reload();
  await editorReady(guest);
}

async function peopleEntry(owner: Page, id: string, email: string): Promise<string> {
  const res = await api(owner, 'GET', `/api/files/${id}/share`);
  const people = (res.body as { people: { id: string; email: string }[] }).people;
  return people.find((p) => p.email === email)!.id;
}

test('двое правят один файл: каждый видит чужое, отмена трогает только своё', async ({ browser }) => {
  const [a, b] = [await person(browser, newEmail('sa'), 'Анна'), await person(browser, newEmail('sb'), 'Борис')];
  const id = await newFile(a);
  await join(a, b, id);
  await synced(b);

  await drawRect(a);
  await nodes(b, 1);
  await drawRect(b, 200);
  await nodes(a, 2);
  await synced(a);
  await synced(b);

  // Отмена у `a` убирает его прямоугольник, чужой остаётся у обоих.
  await a.locator('canvas').click({ position: { x: 10, y: 10 } });
  await a.keyboard.press('ControlOrMeta+KeyZ');
  await nodes(a, 1);
  await nodes(b, 1);
  await synced(a);
  // Отмена у `b` — его прямоугольник; у него шаг чужой правки не появился.
  await b.locator('canvas').click({ position: { x: 10, y: 10 } });
  await b.keyboard.press('ControlOrMeta+KeyZ');
  await nodes(b, 0);
  await nodes(a, 0);
  await b.keyboard.press('ControlOrMeta+KeyZ');
  await nodes(a, 0);

  // Заново открытый файл — то же самое: всё дошло до сервера.
  await a.keyboard.press('ControlOrMeta+Shift+KeyZ');
  await nodes(a, 1);
  await synced(a);
  await a.reload();
  await editorReady(a);
  await nodes(a, 1);
  await synced(a);
});

test('читатель видит правки живьём и не пишет', async ({ browser }) => {
  const [a, v] = [await person(browser, newEmail('sc'), 'Анна'), await person(browser, newEmail('sd'), 'Вера')];
  const id = await newFile(a);
  await join(a, v, id, 'viewer');
  await expect(v.getByText('Только просмотр')).toBeVisible();
  await drawRect(a);
  await nodes(v, 1);
  await v.locator('canvas').click({ position: { x: 10, y: 10 } });
  await v.keyboard.press('KeyR');
  await expect.poll(async () => (await live(a)).nodes).toBe(1);
  const before = (await live(v)).nodes;
  expect(before).toBe(1);
});

test('вторая вкладка того же человека — такой же участник', async ({ browser }) => {
  const a = await person(browser, newEmail('se'), 'Анна');
  const id = await newFile(a);
  const tab = await a.context().newPage();
  await testMode(tab);
  await tab.goto(`/#/file/${id}`);
  await editorReady(tab);
  await synced(tab);
  await drawRect(a);
  await nodes(tab, 1);
  await drawRect(tab, 200);
  await nodes(a, 2);
});

test('снятие роли действует сразу, понижение переводит в просмотр без перезагрузки', async ({ browser }) => {
  const [a, b] = [await person(browser, newEmail('sf'), 'Анна'), await person(browser, newEmail('sg'), 'Борис')];
  const email = await b.evaluate(async () => ((await (await fetch('/api/auth/session')).json()) as { user: { email: string } }).user.email);
  const id = await newFile(a);
  await join(a, b, id);
  await synced(b);
  const entry = await peopleEntry(a, id, email);

  const started = Date.now();
  expect((await api(a, 'PATCH', `/api/files/${id}/share/people/${entry}`, { role: 'viewer' })).status).toBe(200);
  await expect(b.getByText('Только просмотр')).toBeVisible({ timeout: 3000 });
  expect(Date.now() - started).toBeLessThan(3000);
  // Правки идут и читателю.
  await drawRect(a);
  await nodes(b, 1);

  expect((await api(a, 'PATCH', `/api/files/${id}/share/people/${entry}`, { role: 'editor' })).status).toBe(200);
  await expect(b.getByText('Только просмотр')).toBeHidden({ timeout: 3000 });
  await drawRect(b, 200);
  await nodes(a, 2);

  const removed = Date.now();
  expect((await api(a, 'DELETE', `/api/files/${id}/share/people/${entry}`)).status).toBe(200);
  await expect(b.getByRole('alert').getByText('Нет доступа')).toBeVisible({ timeout: 3000 });
  expect(Date.now() - removed).toBeLessThan(3000);
});

test('обрыв сети: правки копятся, индикатор говорит правду, при возврате всё сходится', async ({ browser }) => {
  test.skip(test.info().project.name === 'server-firefox', 'setOffline в Firefox не закрывает WebSocket');
  const [a, b] = [await person(browser, newEmail('sh'), 'Анна'), await person(browser, newEmail('si'), 'Борис')];
  const id = await newFile(a);
  await join(a, b, id);
  await synced(b);

  await a.context().setOffline(true);
  await expect(indicator(a)).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(a);
  await drawRect(a, 150);
  await drawRect(b, 300);
  await nodes(b, 1);
  await a.context().setOffline(false);
  await synced(a, 45_000);
  await nodes(a, 3);
  await nodes(b, 3);
});

test('рестарт сервера: переподключение, ни одна правка не потеряна', async ({ browser }) => {
  const [a, b] = [await person(browser, newEmail('sj'), 'Анна'), await person(browser, newEmail('sk'), 'Борис')];
  const id = await newFile(a);
  await join(a, b, id);
  await synced(b);
  await drawRect(a);
  await nodes(b, 1);

  compose('restart', 'server');
  await synced(a, 60_000);
  await synced(b, 60_000);
  await drawRect(b, 200);
  await nodes(a, 2);
  await drawRect(a, 400);
  await nodes(b, 3);
  // Правка, сделанная прямо в рестарт: неподтверждённая уходит заново.
  const restart = (async () => compose('restart', 'server'))();
  await drawRect(a, 600);
  await restart;
  await synced(a, 60_000);
  await nodes(b, 4, 60_000);
});

test('база отвалилась и зависла: сервер живёт, правки доходят, когда она вернулась', async ({ browser }) => {
  const a = await person(browser, newEmail('sl'), 'Анна');
  await newFile(a);
  await drawRect(a);
  await synced(a);
  // У стенда база на tmpfs: рестарт стёр бы её, поэтому обрыв всех соединений и заморозка.
  compose(
    'exec',
    '-T',
    'postgres',
    'psql',
    '-U',
    'ferrum',
    '-d',
    'ferrum',
    '-c',
    `SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE pid <> pg_backend_pid() AND datname = 'ferrum'`,
  );
  compose('pause', 'postgres');
  await drawRect(a, 100);
  await a.waitForTimeout(8000);
  await expect(indicator(a)).not.toHaveText('Синхронизировано');
  compose('unpause', 'postgres');
  await drawRect(a, 200);
  await synced(a, 60_000);
  await a.reload();
  await editorReady(a);
  await nodes(a, 3);
});

test('выход в другой вкладке закрывает соединение и не теряет правки молча', async ({ browser }) => {
  const a = await person(browser, newEmail('sm'), 'Анна');
  await newFile(a);
  await drawRect(a);
  await synced(a);
  expect((await api(a, 'POST', '/api/auth/sign-out', {})).status).toBe(200);
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible({ timeout: 5000 });
});

test('журнал браузера снимается подтверждением сервера и держит неподтверждённое', async ({ browser }) => {
  const a = await person(browser, newEmail('sn'), 'Анна');
  await newFile(a);
  await drawRect(a);
  await synced(a);
  test.skip(test.info().project.name === 'server-firefox', 'setOffline в Firefox не закрывает WebSocket');
  await a.context().setOffline(true);
  await expect(indicator(a)).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(a, 200);
  // Журнал браузера записан: перезагрузка без сети не теряет правку, сеть вернулась — она на сервере.
  await a.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(async () => (await live(a)).nodes).toBe(2);
  await a.waitForTimeout(2500);
  await a.context().setOffline(false);
  await a.reload();
  await editorReady(a);
  await nodes(a, 2);
  await synced(a, 45_000);
  await a.reload();
  await editorReady(a);
  await nodes(a, 2);
});

test('файл 100 000 узлов: чужие правки во время своего жеста, основной поток свободен, задержка до другого @big', async ({
  browser,
}, info) => {
  test.skip(info.project.name === 'server-firefox', 'замер в одном браузере');
  test.setTimeout(300_000);
  const [a, b] = [await person(browser, newEmail('so'), 'Анна'), await person(browser, newEmail('sp'), 'Борис')];
  await a.goto('/?nodes=100000#/new');
  await editorReady(a);
  await synced(a, 120_000);
  const id = fileId(a);
  await join(a, b, id);
  await synced(b, 120_000);
  await nodes(b, 100_000, 60_000);

  // Задержка: дубль у `b`, момент появления у `a`; часы одной машины.
  await drawRect(b, 300);
  await nodes(a, 100_001, 60_000);
  await a.evaluate(() => {
    const w = window as unknown as { __longTasks: number[] };
    w.__longTasks = [];
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) w.__longTasks.push(e.duration);
    }).observe({ entryTypes: ['longtask'] });
  });
  // Жест у `a` идёт, пока `b` правит.
  const box = (await a.locator('canvas').boundingBox())!;
  await a.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await a.mouse.down();
  const delays: number[] = [];
  let expected = (await live(a)).nodes;
  for (let i = 0; i < 20; i++) {
    await a.mouse.move(box.x + box.width / 2 + i * 3, box.y + box.height / 2 + i);
    const started = Date.now();
    await b.evaluate(() =>
      (window as unknown as { __ferrum: { core: { send(c: unknown): void } } }).__ferrum.core.send({
        type: 'execute',
        command: 'edit.duplicate',
      }),
    );
    expected++;
    await expect.poll(async () => (await live(a)).nodes, { intervals: [5], timeout: 30_000 }).toBeGreaterThanOrEqual(expected);
    delays.push(Date.now() - started);
  }
  await a.mouse.up();
  const long = await a.evaluate(() => (window as unknown as { __longTasks: number[] }).__longTasks);
  delays.sort((x, y) => x - y);
  console.log(
    '100 000 узлов, задержка до другого, мс: p50',
    delays[10],
    'p95',
    delays[18],
    'наибольшая',
    delays[19],
    'длинные задачи основного потока',
    long,
  );
  expect(delays[18]).toBeLessThan(1000);
  // Синхронизация — в worker'ах: на основном потоке её нет.
  expect(long.filter((d) => d > 200)).toEqual([]);
});
````

В `apps/web/unit/saver.test.ts` заменить:

````ts
  JOURNAL_MS,
  newSession,
  sessionAfter,
  SETTLE_MS,
  withTimeout,
  type SaverOptions,
````

на:

````ts
  JOURNAL_MS,
  newSession,
  NOT_ON_SERVER,
  sessionAfter,
  SETTLE_MS,
  SYNC_WAIT_MS,
  withTimeout,
  type SaverOptions,
````

В `apps/web/unit/saver.test.ts` заменить:

````ts
  expect(ask(stored)).toBe(false);
});
````

на:

````ts
  expect(ask(stored)).toBe(false);
});

// --- Журнал сервера: файл целиком пишется только первый раз ---

const synced = (stored: boolean, opts: SaverOptions = {}) => harness(stored, null, true, true, { sync: true, ...opts });

test('журнал сервера: правки идут журналом, полный файл не пишется ни в простое, ни со временем', async () => {
  const h = synced(true);
  h.emit({ type: 'modified', modified: true });
  h.advance(FULL_MAX_MS * 3);
  await h.tick();
  expect(h.sent.some((c) => c.type === 'flushJournal')).toBe(true);
  expect(h.sent.some((c) => c.type === 'snapshot')).toBe(false);
  expect(h.writes).toHaveLength(0);
});

test('журнал сервера: новый файл записывается сразу, без правок; потом можно подключаться', async () => {
  let stored = 0;
  const h = harness(false, null, true, true, { sync: true, onStored: () => stored++ });
  await h.tick();
  expect(h.sent.find((c) => c.type === 'snapshot')).toBeTruthy();
  h.snapshot(0);
  await h.tick();
  await h.tick();
  expect(h.writes).toHaveLength(1);
  expect(stored).toBe(1);
  expect(h.saver.stored()).toBe(true);
});

test('журнал сервера: читатель первую запись не делает', async () => {
  const h = harness(false, null, true, true, { sync: true, viewer: true });
  await h.tick();
  expect(h.sent.some((c) => c.type === 'snapshot')).toBe(false);
});

test('журнал сервера: первую запись опередили, своих правок нет — файл открывается заново, а не теряет чужое', async () => {
  let taken = 0;
  const h = harness(false, null, true, true, { sync: true, onTaken: () => taken++ });
  h.failWrites(true, 'CONFLICT');
  await h.tick();
  h.snapshot(0);
  await h.tick();
  await h.tick();
  expect(taken).toBe(1);
});

test('журнал сервера: первую запись опередили, а правки уже есть — честный вопрос, правки не пропадают молча', async () => {
  let taken = 0;
  let diverged = 0;
  const h = harness(false, null, true, true, { sync: true, onTaken: () => taken++, onDiverged: () => diverged++ });
  h.failWrites(true, 'CONFLICT');
  h.emit({ type: 'modified', modified: true });
  await h.tick();
  h.snapshot(0);
  await h.tick();
  await h.tick();
  expect(taken).toBe(0);
  expect(diverged).toBe(1);
});

test('журнал сервера: «Сохранить» ждёт сервера; подтвердил — записано', async () => {
  const h = synced(true);
  h.emit({ type: 'modified', modified: true });
  h.emit({ type: 'syncState', unacked: true });
  const saved = h.saver.save();
  await h.tick();
  h.emit({ type: 'journal', batch: 1, bytes: new Uint8Array([1]), failed: false });
  await h.tick();
  await h.tick();
  h.emit({ type: 'syncState', unacked: false });
  expect(await saved).toBe(true);
  expect(h.saver.failure()).toBeNull();
});

test('журнал сервера: сервер не подтвердил за срок — уход спросит, причина не пугает: правки в браузере', async () => {
  const h = synced(true);
  h.emit({ type: 'modified', modified: true });
  h.emit({ type: 'syncState', unacked: true });
  const saved = h.saver.save();
  await h.tick();
  h.emit({ type: 'journal', batch: 1, bytes: new Uint8Array([1]), failed: false });
  await h.tick();
  await h.tick();
  h.advance(SYNC_WAIT_MS);
  expect(await saved).toBe(false);
  expect(h.saver.failure()).toBe(NOT_ON_SERVER);
  // Позже подтвердил: причина снята.
  h.emit({ type: 'syncState', unacked: false });
  expect(h.saver.failure()).toBeNull();
});
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit -- unit/saver.test.ts`
Expected: FAIL — `The requested module '../src/ui/saver' does not provide an export named 'NOT_ON_SERVER'`

- [ ] **Step 3: Реализация**

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  const id = props.file.id;
  /** Доступ к файлу потерян, пока он открыт (файл на сервере): снят или сессия кончилась. */
  const [lost, setLost] = createSignal<'none' | 'session' | 'diverged' | null>(null);
  /** Просьба о доступе из плашки «Нет доступа». */
  const [asked, setAsked] = createSignal(false);
````

на:

````tsx
  const id = props.file.id;
  /** Доступ к файлу потерян, пока он открыт (файл на сервере): снят или сессия кончилась. */
  const [lost, setLost] = createSignal<'none' | 'session' | 'diverged' | 'rejected' | null>(null);
  /** Сервер не принял правки: причина для плашки. */
  const [rejected, setRejected] = createSignal('');
  let syncHandle: { stop(): void } | undefined;
  /** Просьба о доступе из плашки «Нет доступа». */
  const [asked, setAsked] = createSignal(false);
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
    boot(canvas, store, {
      document: bytes,
      backend: props.backend,
      systemFonts: systemFonts((blob) => props.files.fontBytes?.(blob) ?? Promise.reject(new Error('нет хранилища'))),
````

на:

````tsx
    boot(canvas, store, {
      document: bytes,
      remote: !!props.files.sync,
      backend: props.backend,
      systemFonts: systemFonts((blob) => props.files.fontBytes?.(blob) ?? Promise.reject(new Error('нет хранилища'))),
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
        };
        let diverged = false;
        saver = createSaver(ed, props.files, id, version, stored, {
          ...testIntervals(),
          journalSession,
          onDiverged: () => {
            diverged = true;
````

на:

````tsx
        };
        let diverged = false;
        /** Соединение файла: как только файл есть на сервере. Само не падает, сбои — состоянием. */
        const startSync = () => {
          if (syncHandle || disposed || !props.files.sync) return;
          store.setState('syncStatus', 'connecting');
          syncHandle = props.files.sync(id, ed.core, {
            status: (status, detail) => {
              store.setState('syncStatus', status);
              if (status === 'rejected' && detail) {
                setRejected(detail);
                setLost('rejected');
              } else if (status === 'paused' && detail) store.toast(detail);
            },
            reload: () => {
              ed.send({ type: 'setViewOnly', on: true });
              store.toast('Вышла новая версия Ferrum — обновите страницу, правки сохранятся');
            },
          });
        };
        saver = createSaver(ed, props.files, id, version, stored, {
          ...testIntervals(),
          journalSession,
          sync: !!props.files.sync,
          viewer: store.state.fileRole === 'viewer',
          onStored: startSync,
          onTaken: () => void props.onRestart({ reason: 'role', lost: false, message: 'файл уже записан другим' }),
          onDiverged: () => {
            diverged = true;
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
          } else if (before === 'viewer' && state.role !== 'viewer') store.toast('Теперь вы можете редактировать файл');
          // Своя запись, ответ на которую ещё в пути или потерян, — не «другой человек».
          if (saver && !state.own && state.version > saver.version() && !toldNewer) {
            toldNewer = true;
            store.toast('Файл изменён в другом окне или другим человеком. Откройте его заново, чтобы увидеть новую версию');
````

на:

````tsx
          } else if (before === 'viewer' && state.role !== 'viewer') store.toast('Теперь вы можете редактировать файл');
          // Своя запись, ответ на которую ещё в пути или потерян, — не «другой человек».
          // Файл без записи записали, пока он был открыт, а своих правок нет: его содержимое и есть правда.
          if (
            saver &&
            !saver.stored() &&
            !store.state.modified &&
            state.version > 0 &&
            props.onRestart({ reason: 'role', lost: false, message: 'файл записан другим' })
          )
            return;
          if (saver && !props.files.sync && !state.own && state.version > saver.version() && !toldNewer) {
            toldNewer = true;
            store.toast('Файл изменён в другом окне или другим человеком. Откройте его заново, чтобы увидеть новую версию');
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
          }
          if (e.type === 'recovered') recovered = true;
          if (opening && e.type === 'opened') {
            opening = false;
````

на:

````tsx
          }
          if (e.type === 'recovered') recovered = true;
          if (opening && e.type === 'opened' && saver?.stored()) startSync();
          if (opening && e.type === 'opened') {
            opening = false;
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
  onCleanup(() => {
    disposed = true;
    offWatch?.();
    // Открытый вопрос об уходе (редактор снят перезапуском) — «Остаться».
````

на:

````tsx
  onCleanup(() => {
    disposed = true;
    syncHandle?.stop();
    store.setState('syncStatus', null);
    offWatch?.();
    // Открытый вопрос об уходе (редактор снят перезапуском) — «Остаться».
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
            {(kind) => (
              <div class="fe-boot fe-boot--error fe-boot--stopped fe-lost" role="alert">
                <strong>{kind() === 'none' ? 'Нет доступа' : kind() === 'diverged' ? 'Файл уже записан' : 'Сессия завершена'}</strong>
                <span>
                  {kind() === 'none'
````

на:

````tsx
            {(kind) => (
              <div class="fe-boot fe-boot--error fe-boot--stopped fe-lost" role="alert">
                <strong>
                  {kind() === 'none'
                    ? 'Нет доступа'
                    : kind() === 'diverged'
                      ? 'Файл уже записан'
                      : kind() === 'rejected'
                        ? 'Правки не сохраняются'
                        : 'Сессия завершена'}
                </strong>
                <span>
                  {kind() === 'none'
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
                    : kind() === 'diverged'
                      ? NEW_FILE_TAKEN
                      : 'Войдите снова, правки останутся в этом браузере'}
                </span>
                <span class="fe-lost__actions">
````

на:

````tsx
                    : kind() === 'diverged'
                      ? NEW_FILE_TAKEN
                      : kind() === 'rejected'
                        ? rejected()
                        : 'Войдите снова, правки останутся в этом браузере'}
                </span>
                <span class="fe-lost__actions">
````

В `apps/web/src/ui/app.tsx` заменить:

````tsx
                    >
                      Сохранить новым файлом
                    </button>
                  ) : kind() === 'none' ? (
````

на:

````tsx
                    >
                      Сохранить новым файлом
                    </button>
                  ) : kind() === 'rejected' ? (
                    <button class="fe-btn fe-btn--primary" onClick={() => location.reload()}>
                      Обновить страницу
                    </button>
                  ) : kind() === 'none' ? (
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
   * `null` — новый файл. */
  document?: Promise<{ bytes: ArrayBuffer; journal: ArrayBuffer[] } | null>;
  /** Системные шрифты платформы (подсистема 3); нет — только вшитые. */
  systemFonts?: SystemFonts;
````

на:

````ts
   * `null` — новый файл. */
  document?: Promise<{ bytes: ArrayBuffer; journal: ArrayBuffer[] } | null>;
  /** Файл на сервере: журналу может не хватать правок, которые придут при догонянии. */
  remote?: boolean;
  /** Системные шрифты платформы (подсистема 3); нет — только вшитые. */
  systemFonts?: SystemFonts;
````

В `apps/web/src/ui/boot.ts` заменить:

````ts
  if (doc && doc.journal.length > 0) {
    // Правки после файла (сбой, закрытая вкладка): ядро применит их поверх него.
    core.send({ type: 'openWithJournal', bytes: new Uint8Array(doc.bytes), journal: doc.journal.map((b) => new Uint8Array(b)) });
  } else if (doc) core.send({ type: 'open', bytes: new Uint8Array(doc.bytes) });
  else if (params.has('texts')) core.send({ type: 'demoText', texts: Number(params.get('texts')) });
````

на:

````ts
  if (doc && doc.journal.length > 0) {
    // Правки после файла (сбой, закрытая вкладка): ядро применит их поверх него.
    core.send({
      type: 'openWithJournal',
      bytes: new Uint8Array(doc.bytes),
      journal: doc.journal.map((b) => new Uint8Array(b)),
      remote: opts.remote === true,
    });
  } else if (doc) core.send({ type: 'open', bytes: new Uint8Array(doc.bytes) });
  else if (params.has('texts')) core.send({ type: 'demoText', texts: Number(params.get('texts')) });
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
// панелей для изменения ширины.

import type { FileMeta } from '../core/file-list';
import { extensions } from './extensions';
import { createSignal, onCleanup, onMount, Show, type JSX } from 'solid-js';
````

на:

````tsx
// панелей для изменения ширины.

import type { FileRole } from '@ferrum/protocol';
import type { FileMeta } from '../core/file-list';
import type { SyncStatus } from '../core/sync-client';
import { extensions } from './extensions';
import { createSignal, onCleanup, onMount, Show, type JSX } from 'solid-js';
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx

export { clampPanel, PANEL_MAX, PANEL_MIN };

/** Шапка левой панели: кнопка с логотипом открывает главное меню (как в Figma
````

на:

````tsx

export { clampPanel, PANEL_MAX, PANEL_MIN };

/** Подпись состояния соединения файла (спека подсистемы 4, раздел 8.1). */
function syncText(status: SyncStatus | null, role: FileRole | null): string | null {
  switch (status) {
    case 'synced':
      return 'Синхронизировано';
    case 'saving':
      // Роль понизили, пока правки шли: они уже не дойдут.
      return role === 'viewer' ? 'Правки не дошли до сервера' : 'Сохранение…';
    case 'connecting':
    case 'paused':
      return 'Сохранение…';
    case 'offline':
      return 'Нет сети — правки сохранятся при подключении';
    case 'rejected':
      return 'Правки не сохраняются';
    case null:
      return null;
  }
}

/** Шапка левой панели: кнопка с логотипом открывает главное меню (как в Figma
````

В `apps/web/src/ui/chrome.tsx` заменить:

````tsx
          <span class="fe-viewonly t-muted">Только просмотр</span>
        </Show>
        {extensions.shareButton && props.file ? (
          <extensions.shareButton
````

на:

````tsx
          <span class="fe-viewonly t-muted">Только просмотр</span>
        </Show>
        <Show when={syncText(ed.store.state.syncStatus, ed.store.state.fileRole)}>
          {(text) => (
            <span class="fe-sync t-muted" role="status" data-status={ed.store.state.syncStatus}>
              {text()}
            </span>
          )}
        </Show>
        {extensions.shareButton && props.file ? (
          <extensions.shareButton
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
import type { FileMeta, FromIo, IoOp, IoResults, JournalMark, Section, ToIo } from '../core/file-list';
import type { AccessState } from '../core/server-files';
import { nameFromFile } from '../core/file-list';
import { mb } from '../core/limits';
````

на:

````ts
import type { FileMeta, FromIo, IoOp, IoResults, JournalMark, Section, ToIo } from '../core/file-list';
import type { AccessState } from '../core/server-files';
import type { SyncStatus } from '../core/sync-client';
import { nameFromFile } from '../core/file-list';
import { mb } from '../core/limits';
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts

export type { FileMeta, JournalMark, Section } from '../core/file-list';

export interface FileStore {
````

на:

````ts

export type { FileMeta, JournalMark, Section } from '../core/file-list';

export interface SyncEvents {
  status(status: SyncStatus, detail?: string): void;
  reload(): void;
}

export interface FileStore {
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  /** Запись зависит от сети: неудачная повторяется сама, закрытие вкладки спрашивает о незаписанном. */
  readonly remote?: boolean;
  /** Новый файл из снимка ядра (буфер передаётся); нет — спасать правки в новый файл негде. */
  createFrom?(name: string, bytes: ArrayBuffer): Promise<FileMeta>;
````

на:

````ts
  /** Запись зависит от сети: неудачная повторяется сама, закрытие вкладки спрашивает о незаписанном. */
  readonly remote?: boolean;
  /** Соединение файла (спека подсистемы 4, раздел 7): правки идут журналом сервера, файл целиком пишется только первый раз. */
  sync?(id: string, core: { connectSync(port: MessagePort): void }, events: SyncEvents): { stop(): void };
  /** Сервер подтвердил правки: снять пачки журнала браузера. */
  cutJournal?(id: string, mark: JournalMark): Promise<void>;
  /** Превью при правках журналом. `null` — убрать. */
  putThumb?(id: string, thumb: ArrayBuffer | null): void;
  /** Новый файл из снимка ядра (буфер передаётся); нет — спасать правки в новый файл негде. */
  createFrom?(name: string, bytes: ArrayBuffer): Promise<FileMeta>;
````

В `apps/web/src/ui/file-store.ts` заменить:

````ts
  /** Доступ к открытому файлу (файл на сервере); возвращает отписку. Нет — доступ не меняется. */
  watch?(id: string, listener: (state: AccessState) => void): () => void;
  /** Открыть файл по ссылке «Поделиться»; `false` — ссылка не действует. */
  redeemLink?(id: string, token: string): Promise<boolean>;
````

на:

````ts
  /** Доступ к открытому файлу (файл на сервере); возвращает отписку. Нет — доступ не меняется. */
  watch?(id: string, listener: (state: AccessState) => void): () => void;
  /** Доступ теперь сообщает соединение файла: опрос не нужен. */
  stopPolling?(id: string): void;
  /** Открыть файл по ссылке «Поделиться»; `false` — ссылка не действует. */
  redeemLink?(id: string, token: string): Promise<boolean>;
````

В `apps/web/src/ui/root.tsx` заменить:

````tsx
      }
    }
    const lock = await lockFile(file.id);
    if (my !== seq) {
      if ('release' in lock) lock.release();
````

на:

````tsx
      }
    }
    // Файл с журналом сервера: вторая вкладка — такой же участник (спека подсистемы 4, раздел 8).
    const lock: Lock = files.sync ? { release: () => {} } : await lockFile(file.id);
    if (my !== seq) {
      if ('release' in lock) lock.release();
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
// не отодвигают полный файл дальше `FULL_MAX_MS` от прошлого. После неудачной
// полной записи следующая сама по себе — не раньше чем через `FULL_IDLE_MS`.
// Запись через сеть (`files.remote`): после сбоя она повторяется сама с растущей паузой, от `FULL_IDLE_MS` до
// десяти минут (удача или правка начинают заново), кроме отказов, которые повтор не исправит. Без сети и за
````

на:

````ts
// не отодвигают полный файл дальше `FULL_MAX_MS` от прошлого. После неудачной
// полной записи следующая сама по себе — не раньше чем через `FULL_IDLE_MS`.
// Файл с журналом сервера (`opts.sync`): правки идут по соединению файла, а полный файл пишется только первый раз, чтобы журналу
// было к чему лечь. Журнал браузера держит неподтверждённое и снимается по подтверждению сервера (`journalCovered`). «Сохранить» —
// дождаться сервера; не дождались (нет сети) — уход спрашивает, правки при этом в журнале браузера и уйдут при следующем открытии.
// Запись через сеть (`files.remote`): после сбоя она повторяется сама с растущей паузой, от `FULL_IDLE_MS` до
// десяти минут (удача или правка начинают заново), кроме отказов, которые повтор не исправит. Без сети и за
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
  /** Файл без записи уже записал другой человек: правки больше не лягут в него. */
  onDiverged?: () => void;
}

export interface Saver {
````

на:

````ts
  /** Файл без записи уже записал другой человек: правки больше не лягут в него. */
  onDiverged?: () => void;
  /** Файл на сервере с журналом: правки идут соединением файла, а не записью целиком. */
  sync?: boolean;
  /** Читатель: первую запись делает не он. */
  viewer?: boolean;
  /** Файл теперь есть на сервере: можно подключаться. */
  onStored?: () => void;
  /** Первую запись опередил другой, своих правок ещё не было: открыть файл заново, его содержимое и есть правда. */
  onTaken?: () => void;
}

/** Сколько «Сохранить» и уход ждут подтверждения сервера. */
export const SYNC_WAIT_MS = 5000;
/** Превью при правках журналом — не чаще этого (раздел 6.2). */
export const THUMB_MS = 60_000;
/** Текст вопроса при уходе: правки не потеряются, но и на сервере их нет. */
export const NOT_ON_SERVER =
  'Правки ещё не дошли до сервера. Они сохранены в этом браузере и уйдут, когда вы откроете файл снова при подключении';

export interface Saver {
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
  /** Поколение файла, на котором основана следующая запись. */
  version(): number;
  /** Человек согласился уйти: закрытие вкладки больше не спрашивает. */
  release(): void;
````

на:

````ts
  /** Поколение файла, на котором основана следующая запись. */
  version(): number;
  /** Есть ли у файла запись на сервере. */
  stored(): boolean;
  /** Человек согласился уйти: закрытие вкладки больше не спрашивает. */
  release(): void;
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
  const journalWaiters: ((done: Promise<boolean>) => void)[] = [];

  const edited = () => {
    dirty = true;
````

на:

````ts
  const journalWaiters: ((done: Promise<boolean>) => void)[] = [];

  /** Подтверждение сервера: ждущие «Сохранить» и уход. */
  let unacked = false;
  let ackWaiters: ((ok: boolean) => void)[] = [];
  let thumbTimer = 0;
  /** Первая запись нужна и без правок: журналу надо к чему лечь. */
  let needBase = opts.sync === true && !stored && opts.viewer !== true;
  const synced = () => opts.sync === true && stored;

  const edited = () => {
    dirty = true;
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
    if (!stored) {
      fullTimer = window.setTimeout(() => void save(), AUTOSAVE_MS);
      return;
    }
````

на:

````ts
    if (!stored) {
      fullTimer = window.setTimeout(() => void save(), AUTOSAVE_MS);
      return;
    }
    if (synced()) {
      if (!journalTimer) journalTimer = window.setTimeout(() => void flushJournal(), journalMs);
      if (!thumbTimer && files.putThumb) {
        thumbTimer = window.setTimeout(() => {
          thumbTimer = 0;
          ed.send({ type: 'thumbnail' });
        }, opts.fullIdleMs ?? THUMB_MS);
      }
      return;
    }
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
      case 'journal':
        onJournal(e);
        break;
      case 'saveRequested':
````

на:

````ts
      case 'journal':
        onJournal(e);
        break;
      case 'journalCovered':
        void files.cutJournal?.(id, { session, batch: e.batch }).catch(() => {
          // Не снялось — останется до следующего раза, при открытии его пропустит Loro.
        });
        break;
      case 'syncState':
        unacked = e.unacked;
        if (!unacked) {
          for (const w of ackWaiters.splice(0)) w(true);
          failure = null;
        }
        break;
      case 'thumbnail':
        if (!e.keep) files.putThumb?.(id, e.bytes ? ownBuffer(e.bytes) : null);
        break;
      case 'saveRequested':
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
  }

  async function write(): Promise<boolean> {
    clearTimeout(fullTimer);
    if (!modified || rescued) return true;
    // Отказ не изменится, а причина уже показана: вопрос при уходе назовёт её.
    if (diverged) return false;
````

на:

````ts
  }

  /** Файл ведёт журнал сервера: «записано» — это подтвердил сервер. */
  async function waitServer(): Promise<boolean> {
    // Человек сохраняет или уходит: превью не ждёт своего срока.
    if (thumbTimer) {
      clearTimeout(thumbTimer);
      thumbTimer = 0;
      ed.send({ type: 'thumbnail' });
    }
    const flushed = await flushJournal();
    if (!unacked) return flushed;
    const ok = await new Promise<boolean>((resolve) => {
      const timer = window.setTimeout(() => {
        ackWaiters = ackWaiters.filter((w) => w !== done);
        resolve(false);
      }, SYNC_WAIT_MS);
      const done = (v: boolean) => {
        clearTimeout(timer);
        resolve(v);
      };
      ackWaiters.push(done);
    });
    if (!ok) failure = NOT_ON_SERVER;
    return ok;
  }

  async function write(): Promise<boolean> {
    clearTimeout(fullTimer);
    if (synced()) return rescued ? true : waitServer();
    if ((!modified && !needBase) || rescued) return true;
    // Отказ не изменится, а причина уже показана: вопрос при уходе назовёт её.
    if (diverged) return false;
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
      base = meta.version;
      stored = true;
      lastFull = performance.now();
      covered = Math.max(covered, seen);
````

на:

````ts
      base = meta.version;
      stored = true;
      needBase = false;
      if (opts.sync) opts.onStored?.();
      lastFull = performance.now();
      covered = Math.max(covered, seen);
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
      failedAt = performance.now();
      const code = (err as { code?: unknown }).code;
      if (files.remote && !wasStored && code === 'CONFLICT') {
        diverged = true;
````

на:

````ts
      failedAt = performance.now();
      const code = (err as { code?: unknown }).code;
      if (files.remote && !wasStored && code === 'CONFLICT' && !modified && opts.onTaken) {
        opts.onTaken();
        return false;
      }
      if (files.remote && !wasStored && code === 'CONFLICT') {
        diverged = true;
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
  const release = () => window.removeEventListener('beforeunload', onBeforeUnload);

  return {
    save,
    flushJournal,
    pending,
    settled: () =>
      withTimeout(
````

на:

````ts
  const release = () => window.removeEventListener('beforeunload', onBeforeUnload);

  if (needBase) queueMicrotask(() => void save());

  return {
    save,
    flushJournal,
    pending,
    stored: () => stored,
    settled: () =>
      withTimeout(
````

В `apps/web/src/ui/saver.ts` заменить:

````ts
      clearTimeout(journalTimer);
      clearTimeout(fullTimer);
      off();
      window.removeEventListener('pagehide', onHide);
````

на:

````ts
      clearTimeout(journalTimer);
      clearTimeout(fullTimer);
      clearTimeout(thumbTimer);
      for (const w of ackWaiters.splice(0)) w(false);
      off();
      window.removeEventListener('pagehide', onHide);
````

В `apps/web/src/ui/server/server-store.ts` заменить:

````ts
import type { AccessState } from '../../core/server-files';
import type { FromServer, ServerOp, ServerResults, ToServer } from '../../core/server-messages';
import { IdbFileStore, type FileStore } from '../file-store';
import type { SettingsBackend } from '../settings-store';

export class ServerFileStore implements FileStore, SettingsBackend {
````

на:

````ts
import type { AccessState } from '../../core/server-files';
import type { FromServer, ServerOp, ServerResults, ToServer } from '../../core/server-messages';
import type { FromSync, ToSync } from '../../core/sync-messages';
import { IdbFileStore, type FileStore, type SyncEvents } from '../file-store';
import type { SettingsBackend } from '../settings-store';

/** Версия схемы документа этой сборки: то же, что `SCHEMA_VERSION` ядра. Растёт вместе с ней. */
const SCHEMA_VERSION = 1;

export class ServerFileStore implements FileStore, SettingsBackend {
````

В `apps/web/src/ui/server/server-store.ts` заменить:

````ts
  private readonly onVisibility = () => this.post({ t: 'visible', on: document.visibilityState === 'visible' });

  constructor(user: string) {
    this.worker = new Worker(new URL('../../worker/server.worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<FromServer>) => this.onMessage(e.data);
````

на:

````ts
  private readonly onVisibility = () => this.post({ t: 'visible', on: document.visibilityState === 'visible' });

  constructor(private readonly user: string) {
    this.worker = new Worker(new URL('../../worker/server.worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<FromServer>) => this.onMessage(e.data);
````

В `apps/web/src/ui/server/server-store.ts` заменить:

````ts
    return () => this.listeners.delete(listener);
  }
  /** Доступ к открытому файлу: понижение роли, снятие доступа, конец сессии. Этап 3 заменит опрос
   * сообщениями `role` и `access-revoked`, у получателя ничего не изменится. */
  watch(file: string, listener: (state: AccessState) => void) {
    const set = this.watchers.get(file) ?? new Set();
````

на:

````ts
    return () => this.listeners.delete(listener);
  }
  /** Соединение файла: свой worker, с worker'ом ядра — прямой порт. Доступ теперь сообщает оно, а получатели `watch` те же. */
  sync(file: string, core: { connectSync(port: MessagePort): void }, events: SyncEvents) {
    const worker = new Worker(new URL('../../worker/sync.worker.ts', import.meta.url), { type: 'module' });
    const channel = new MessageChannel();
    core.connectSync(channel.port1);
    worker.onmessage = (e: MessageEvent<FromSync>) => {
      const m = e.data;
      if (m.t === 'status') events.status(m.status, m.detail);
      else if (m.t === 'reload') events.reload();
      else for (const w of this.watchers.get(file) ?? []) w(m.state);
    };
    worker.onerror = (e) => {
      e.preventDefault?.();
      events.status('rejected', 'Не удалось запустить часть приложения — обновите страницу');
    };
    worker.postMessage({ t: 'start', file, user: this.user, schema: SCHEMA_VERSION, port: channel.port2 } satisfies ToSync, [
      channel.port2,
    ]);
    this.stopPolling(file);
    return {
      stop: () => {
        worker.postMessage({ t: 'stop' } satisfies ToSync);
        worker.terminate();
      },
    };
  }
  stopPolling(file: string) {
    if (this.watchers.has(file)) this.post({ t: 'watch', file, on: true, poll: false });
  }
  async cutJournal(file: string, mark: JournalMark) {
    await this.call({ op: 'cutJournal', file, session: mark.session, upTo: mark.batch });
  }
  putThumb(file: string, thumb: ArrayBuffer | null) {
    void this.call({ op: 'putThumb', file, thumb }, thumb ? [thumb] : []).catch(() => {});
  }
  /** Доступ к открытому файлу: понижение роли, снятие доступа, конец сессии. До соединения файла — опрос, с ним — его сообщения. */
  watch(file: string, listener: (state: AccessState) => void) {
    const set = this.watchers.get(file) ?? new Set();
````

В `apps/web/src/ui/store.ts` заменить:

````ts
// общей памяти в фазе measure. Здесь ничего не вычисляется о документе.

import { createStore, reconcile } from 'solid-js/store';
import type {
````

на:

````ts
// общей памяти в фазе measure. Здесь ничего не вычисляется о документе.

import type { SyncStatus } from '../core/sync-client';
import { createStore, reconcile } from 'solid-js/store';
import type {
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  /** Роль в открытом файле на сервере; нет — файл свой, без ролей. */
  fileRole: FileRole | null;
  /** Сервер разрешает звать людей в открытый файл (пришедшему по ссылке — нет). */
  fileCanShare: boolean;
````

на:

````ts
  /** Роль в открытом файле на сервере; нет — файл свой, без ролей. */
  fileRole: FileRole | null;
  /** Соединение файла на сервере (спека подсистемы 4, раздел 8.1); `null` — файл не на сервере. */
  syncStatus: SyncStatus | null;
  /** Сервер разрешает звать людей в открытый файл (пришедшему по ссылке — нет). */
  fileCanShare: boolean;
````

В `apps/web/src/ui/store.ts` заменить:

````ts
  documentName: '',
  fileRole: null,
  fileCanShare: false,
  layers: { start: 0, total: 0, rows: [], reveal: null, revealSeq: 0 },
````

на:

````ts
  documentName: '',
  fileRole: null,
  syncStatus: null,
  fileCanShare: false,
  layers: { start: 0, total: 0, rows: [], reveal: null, revealSeq: 0 },
````

В `apps/web/src/ui/styles/server.css` заменить:

````css
.fe-viewonly {
  white-space: nowrap;
}
.fe-modal {
````

на:

````css
.fe-viewonly {
  white-space: nowrap;
}
.fe-sync {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}
.fe-modal {
````

- [ ] **Step 4: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Expected: PASS: всё без замечаний; модульные 278 passed; обе сборки; `e2e:server` 163 passed, 9 пропущены (100 000 узлов и `setOffline` только в Chromium)

- [ ] **Step 5: Закоммитить**

```bash
git add apps/web/e2e-server/auth.spec.ts apps/web/e2e-server/files.spec.ts apps/web/e2e-server/sync.spec.ts apps/web/src/ui/app.tsx apps/web/src/ui/boot.ts apps/web/src/ui/chrome.tsx apps/web/src/ui/file-store.ts apps/web/src/ui/root.tsx apps/web/src/ui/saver.ts apps/web/src/ui/server/server-store.ts apps/web/src/ui/store.ts apps/web/src/ui/styles/server.css apps/web/unit/saver.test.ts
git commit -F - <<'EOF'
интерфейс: правки журналом сервера, индикатор, ничего не теряется молча

Файл целиком пишется один раз, при открытии нового файла; дальше соединение файла. Роль и снятие доступа приходят по нему (B-237, B-238,
B-296), опрос снят. Индикатор §8.1 в шапке. «Сохранить» и уход ждут сервер до 5 с; не дождались — вопрос с честной причиной: правки в
браузере и уйдут при следующем открытии. Плашка «Правки не сохраняются» с копией на диск при отказе сервера, «Правки не дошли до сервера»
при понижении. Запрет второй вкладки снят для файлов на сервере: вкладка — такой же участник. Превью — по «Сохранить», при уходе и раз в минуту.

E2E `sync.spec.ts`: два браузера на настоящем стенде видят правки друг друга, отмена трогает только своё, обрыв сети, рестарт сервера,
обрыв и заморозка базы, снятие и понижение роли живьём, выход, 100 000 узлов с жестом. Сценарии этапа 2 про `CONFLICT` двух окон заменены:
теперь это одно живое редактирование.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

### Task 10: Документы и финальная проверка

«Сделано (этап 3)» в спеке, `docs/running.md`, бэклог B-309…B-327, снятие закрытых B-207, B-237, B-238, B-245, B-296.

**Files:**
- Modify: `docs/backlog.md`, `docs/running.md`, `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md`

- [ ] **Step 1: Реализация**

В `docs/backlog.md` заменить:

````markdown
| B-202 | Проверка `argv` в `check-dist.mjs` и симлинки: из `npm run` не срабатывает, но при срабатывании проверка молча пропускается. | P3 | этап 1 подсистемы 4, ревью задачи 3 |
| B-203 | Тест выхода за каталог статики слабый: `%2e%2e` нормализует сам `fetch`, до сервера доходит уже другой путь. | P3 | этап 1 подсистемы 4, ревью задачи 4 |
| B-207 | `server.stop()` без `true` при живых WebSocket нужно пересмотреть: без них остановка чистая (проба 52 мс). | P2 | этап 3, финальное ревью (Minor 18) |
| B-208 | Обновление образа при открытой вкладке: старые `assets/*` дают 404, открытие файла падает на `new Worker` без подсказки перезагрузить страницу. Правки уже открытого файла не страдают. Worker файлов сервера при таком сбое просит обновить страницу; `new Worker` ядра — ещё нет. | P2 | этап 1 подсистемы 4, финальное ревью (Minor 19) |
| B-209 | Письмо в пути теряется при остановке сервера, а человек уже видит «Проверьте почту». Окно мало. | P3 | этап 1 подсистемы 4, финальное ревью (Minor 20) |
````

на:

````markdown
| B-202 | Проверка `argv` в `check-dist.mjs` и симлинки: из `npm run` не срабатывает, но при срабатывании проверка молча пропускается. | P3 | этап 1 подсистемы 4, ревью задачи 3 |
| B-203 | Тест выхода за каталог статики слабый: `%2e%2e` нормализует сам `fetch`, до сервера доходит уже другой путь. | P3 | этап 1 подсистемы 4, ревью задачи 4 |
| B-208 | Обновление образа при открытой вкладке: старые `assets/*` дают 404, открытие файла падает на `new Worker` без подсказки перезагрузить страницу. Правки уже открытого файла не страдают. Worker файлов сервера при таком сбое просит обновить страницу; `new Worker` ядра — ещё нет. | P2 | этап 1 подсистемы 4, финальное ревью (Minor 19) |
| B-209 | Письмо в пути теряется при остановке сервера, а человек уже видит «Проверьте почту». Окно мало. | P3 | этап 1 подсистемы 4, финальное ревью (Minor 20) |
````

В `docs/backlog.md` заменить:

````markdown
| № | Что | Пр. | Источник |
|---|---|---|---|
| B-237 | Снятие и понижение роли не мгновенны, как требует §6.3: запись отказывает сразу, а открытый редактор узнаёт при первой записи, при возврате на вкладку или опросом раз в 15 с; скрытая вкладка не опрашивает. Чтение уже открытого файла в памяти не отзывается. Снимается соединением этапа 3 (`role`, `access-revoked`), получатель события тот же, опрос убрать. | P2 | этап 2, расхождение со спекой |
| B-238 | Двое правят файл не вместе: кто записал первым, тот прав, второй получает `CONFLICT` и сливает правки, открыв файл заново; у файла без записи слить нечего, правки сохраняются новым файлом или копией (спека, «Сделано (этап 2)»). Читатель видит версию на момент открытия и сообщение о новой. Снимается этапом 3. | P2 | этап 2 |
| B-239 | Люди, пришедшие по ссылке, не видны в списке «Поделиться»; снять их можно только новой ссылкой или выключателем. | P3 | этап 2 |
| B-240 | Смена `BETTER_AUTH_SECRET` делает прежние ссылки непоказуемыми (токен выводится из секрета): они действуют, но скопировать нельзя, нужна «Новая ссылка». | P3 | этап 2 |
````

на:

````markdown
| № | Что | Пр. | Источник |
|---|---|---|---|
| B-239 | Люди, пришедшие по ссылке, не видны в списке «Поделиться»; снять их можно только новой ссылкой или выключателем. | P3 | этап 2 |
| B-240 | Смена `BETTER_AUTH_SECRET` делает прежние ссылки непоказуемыми (токен выводится из секрета): они действуют, но скопировать нельзя, нужна «Новая ссылка». | P3 | этап 2 |
````

В `docs/backlog.md` заменить:

````markdown
| B-243 | Нет передачи владения файлом и командой: владельца команды снять нельзя, команда без него не живёт. | P3 | этап 2 |
| B-244 | «Локальные файлы (N)» видит любой, кто вошёл в этом браузере: файлы прежней версии ни за кем не записаны. Серверные файлы и журнал разделены (B-190 закрыт), этот остаток закрывается удалением оригиналов. Подтверждено на стенде рабочей сборкой: второй вошедший видит число и имя файла первого. | P2 | этап 2, §6.5, ревью безопасности (M-1) |
| B-245 | `file.schema_version` не заполняется: сервер этапа 2 документ не раскодирует. Заполнит `hello { schema }` этапа 3. | P3 | этап 2 |
| B-246 | Журнал правок человека, у которого сняли право, остаётся в его браузере и больше ничем не показывается, кроме плашки открытого редактора. §11.1 этапа 6: сохранить копией. | P3 | этап 2 |
| B-247 | Окна списка — `OFFSET` по живому порядку: чужая запись между окнами сдвигает строки до следующего чтения. | P3 | этап 2 |
````

на:

````markdown
| B-243 | Нет передачи владения файлом и командой: владельца команды снять нельзя, команда без него не живёт. | P3 | этап 2 |
| B-244 | «Локальные файлы (N)» видит любой, кто вошёл в этом браузере: файлы прежней версии ни за кем не записаны. Серверные файлы и журнал разделены (B-190 закрыт), этот остаток закрывается удалением оригиналов. Подтверждено на стенде рабочей сборкой: второй вошедший видит число и имя файла первого. | P2 | этап 2, §6.5, ревью безопасности (M-1) |
| B-246 | Журнал правок человека, у которого сняли право, остаётся в его браузере и больше ничем не показывается, кроме плашки открытого редактора. §11.1 этапа 6: сохранить копией. | P3 | этап 2 |
| B-247 | Окна списка — `OFFSET` по живому порядку: чужая запись между окнами сдвигает строки до следующего чтения. | P3 | этап 2 |
````

В `docs/backlog.md` заменить:

````markdown
| B-294 | `x-ferrum-lost-images` считает количество, а не хэши: потерянная прикрывает настоящую потерю, если объявленная есть на сервере. Отключает проверку только своему файлу, доступа не даёт. | P3 | этап 2, повторное ревью клиента (Minor 2), ревью безопасности (M-2) |
| B-295 | «Скачать копию» отдаёт полную историю и кэш сцены, «Сохранить копию» — shallow только у открытых страниц: удалённое остаётся в копии, которую передадут дальше. Решить вместе с обрезкой истории. | P2 | этап 2, финальное ревью |
| B-296 | `role { role }` §7.2 не несёт `canShare` и `version`, которые читает обработчик редактора: на этапе 3 расширить сообщение или оставить `GET access`. | P2 | этап 2, финальное ревью (Minor 2) |
| B-297 | Правило адреса этапа 2 действует и на вход: адреса с `%`, `!`, `..`, точкой в конце локальной части и `_` в домене, которые этап 1 принимал, больше не войдут. Сервера с такими людьми ещё нет. Перед первым развёртыванием решить, смягчить ли правило для входа. | P3 | этап 2, повторное ревью (M-1) |
| B-298 | Запрос доступа к файлу владельца со старым адресом (не проходит новое правило) теряется молча: разовый счётчик на сутки уже потрачен, письма нет. | P3 | этап 2, повторное ревью (M-2) |
````

на:

````markdown
| B-294 | `x-ferrum-lost-images` считает количество, а не хэши: потерянная прикрывает настоящую потерю, если объявленная есть на сервере. Отключает проверку только своему файлу, доступа не даёт. | P3 | этап 2, повторное ревью клиента (Minor 2), ревью безопасности (M-2) |
| B-295 | «Скачать копию» отдаёт полную историю и кэш сцены, «Сохранить копию» — shallow только у открытых страниц: удалённое остаётся в копии, которую передадут дальше. Решить вместе с обрезкой истории. | P2 | этап 2, финальное ревью |
| B-297 | Правило адреса этапа 2 действует и на вход: адреса с `%`, `!`, `..`, точкой в конце локальной части и `_` в домене, которые этап 1 принимал, больше не войдут. Сервера с такими людьми ещё нет. Перед первым развёртыванием решить, смягчить ли правило для входа. | P3 | этап 2, повторное ревью (M-1) |
| B-298 | Запрос доступа к файлу владельца со старым адресом (не проходит новое правило) теряется молча: разовый счётчик на сутки уже потрачен, письма нет. | P3 | этап 2, повторное ревью (M-2) |
````

В `docs/backlog.md` заменить:

````markdown
| B-308 | Владелец не всегда первый в списке «Поделиться»: как только у приглашённого появилось имя, он встал выше. | P3 | этап 2, проверка в браузере |

## Код

````

на:

````markdown
| B-308 | Владелец не всегда первый в списке «Поделиться»: как только у приглашённого появилось имя, он встал выше. | P3 | этап 2, проверка в браузере |

## Синхронизация (этап 3 подсистемы 4)

| № | Что | Пр. | Источник |
|---|---|---|---|
| B-309 | Правка дерева параллельно своей правке дерева на большой странице: Loro откатывает всё дерево. 100 000 узлов: около 50 мс на пачку (нативно и в wasm), первая 0,5–0,7 с, память wasm +0,5 ГБ на документ, у клиента и в проверке сервера. Цель 2 мс здесь недостижима без изменений в Loro. В `loro-crdt` кэш отдельно не освободить: сервер вытесняет такой документ через 5 с простоя, клиент его держит. Нужна обрезка истории дерева или свой обход в Loro. | P1 | этап 3, проба |
| B-310 | Семантика не проверяется: редактор с правом записи может прислать верную для Loro пачку с узлом неизвестного типа, неверной сетью или текстом сверх пределов, и у всех файл станет «только для чтения», пока это не отменят. Содержимое цело. Сервер мог бы сверять значения на документе проверки, но схема живёт в Rust. | P2 | этап 3, безопасность |
| B-311 | Проверка пачки на настоящем документе защищает от того, что роняет Loro в состоянии сервера. У клиента с параллельными правками состояние другое: подобранная пачка теоретически роняет ядро только у него, перезапуск ядра снова применит её. Случайной порчей такое не найдено, поток защиты от падения в цикле у клиента не сделан. | P2 | этап 3, безопасность |
| B-312 | Подписка на документ без строки читает секцию из контейнера: читатель, подписываясь на несуществующие документы, тратит чтение диска (предел: 120 кадров в секунду, 4096 документов на соединение). Строк он не заводит. Стоит кэш ответа «секции нет». | P3 | этап 3 |
| B-313 | Роль той же величины, но другой `canShare` (передача через команду): кадр `role` не уходит, кнопка «Поделиться» обновится при следующем открытии. | P3 | этап 3 |
| B-314 | Кадр `role` с понижением приходит сразу, но своя неподтверждённая правка в этот момент остаётся только в браузере: индикатор «Правки не дошли до сервера», журнал держит её до возврата права. Слить её в новый файл можно только копией на диск. | P2 | этап 3 |
| B-315 | Рассылка только внутри одного процесса: второй процесс на той же базе пишет журнал верно (порядок, повторы и непрерывность держит база, тест есть), но подписчики другого процесса узнают о правке только при переподключении. Вне спеки (§1). Для второго процесса нужен `LISTEN/NOTIFY` или Valkey. | P3 | этап 3 |
| B-316 | Подписка на события Loro у страницы стоит и на своих правках: большая вставка 10 000 узлов 120 мс → 220 мс нативно. Выключить запись событий в Loro нельзя. | P3 | этап 3, `sync_report` |
| B-317 | `SyncFlush` идёт 30 раз в секунду, пока открыто соединение, и без правок: сравнение векторов документов, в скрытой вкладке тоже. Дёшево, но батарея. Нужен признак «есть своё». | P3 | этап 3 |
| B-318 | Первая чужая правка после открытия на 100 000 узлах — около 30 мс: Loro один раз поднимает структуры истории. Дальше p95 37 мкс. | P3 | этап 3, `sync_report` |
| B-319 | При обрыве посреди загрузки картинки по чужой ссылке (`ImagesNeeded`) повтора нет: заглушка до следующего открытия файла. | P3 | этап 3 |
| B-320 | Плашка «Правки не сохраняются» после отказа сервера: новые страницы других участников не подписываются, пока страница не обновлена (подписки идут той же командой, что и правки). | P3 | этап 3 |
| B-321 | Сервер не отзывает уже идущий проход догоняния, когда роль снята: соединение закрыто раньше, поэтому кадры не уходят, но запрос к базе дорабатывает. Безвредно. | P3 | этап 3 |
| B-322 | Настоящий прокси с WebSocket не проверялся, строки nginx в `running.md` только написаны. | P3 | этап 3 |
| B-323 | Читатель на новом файле без записи видит пустой документ, пока владелец его не записал: перезапуск редактора приходит с опросом раз в 15 с. | P3 | этап 3 |
| B-324 | «Скачать копию» сжимает журнал файла, у которого сжатие застряло (обновление не применилось): ответ 503 «повторите», копии нет до исправления. Журнал цел. Нужен путь спасти файл без плохой записи. | P2 | этап 3 |
| B-325 | Правки в журнале сервера не считаются в `file.size` до сжатия; экран файлов показывает размер прошлого снимка. | P3 | этап 3 |
| B-327 | Цель 150 мс «своя правка видна другому» на 100 000 узлах выдержана не на каждом прогоне: p95 102, 112, 153, 210 мс при p50 85–88 (20 замеров, занятая машина). Порог теста 1000 мс. Причина выбросов не разобрана, нужен замер с большей выборкой на свободной машине. | P2 | этап 3, E2E |
| B-326 | Журнал отмены и повтора: значение, которое возвращает повтор, — то, что было перед отменой, даже если его поставил другой. В Figma проверить не на чем. | P3 | этап 3 |
| B-328 | Два живых узла с одним `NodeId` (§8.1, редкая гонка отмены удаления и переноса): после удаления одного из них шаги отмены, записанные про временное имя второго, пропускаются, а отмена удаления узла с именем, которое уже перешло к двойнику, пропускается целиком, с поддеревом. Ничего из живого не теряется и все сходятся, но вернуть именно удалённый узел отменой нельзя. Нужно, чтобы отмена умела восстановить узел под новым именем. | P3 | этап 3, задача 2, проверка плана |

## Код

````

В `docs/running.md` заменить:

````markdown
- Прямой доступ без прокси: `FERRUM_BIND=0.0.0.0`. Опубликованный Docker порт обходит ufw. За пробросом порта Docker Desktop и OrbStack адрес клиента — адрес шлюза, предел на IP становится общим на всех.

### Файлы и данные

Содержимое файлов, превью и картинки лежат на томе `ferrum-data`, записи о них — в `ferrum-postgres`: резервная копия нужна обоим вместе. Заменённые байты удаляются фоновой очисткой раз в минуту, неиспользуемые картинки — через 30–60 дней, брошенные загрузки — через 24 часа. Предела места на человека нет: только 512 МБ на файл, 10 000 картинок в файле, пределы частоты и не больше 8 одновременных загрузок на человека и 32 на сервер (сверх — 429 `BUSY`): следите за диском. Загрузка, которая в среднем медленнее 32 КиБ/с после минуты форы или молчит дольше минуты, обрывается с 408.

Смена `BETTER_AUTH_SECRET` не ломает ссылки «Поделиться», но показать прежнюю ссылку снова будет нельзя: нужна «Новая ссылка».
````

на:

````markdown
- Прямой доступ без прокси: `FERRUM_BIND=0.0.0.0`. Опубликованный Docker порт обходит ufw. За пробросом порта Docker Desktop и OrbStack адрес клиента — адрес шлюза, предел на IP становится общим на всех.

WebSocket файла идёт по тому же адресу, `/api/files/:id/sync`. Прокси должен пропускать `Upgrade` и не обрывать простаивающее соединение раньше 2 минут (клиент проверяет связь раз в 20 с). В nginx: `proxy_http_version 1.1; proxy_set_header Upgrade $http_upgrade; proxy_set_header Connection "upgrade"; proxy_read_timeout 180s;`, вариант не проверялся (B-322).

### Файлы и данные

Содержимое файлов, превью и картинки лежат на томе `ferrum-data`, записи о них — в `ferrum-postgres`: резервная копия нужна обоим вместе. Заменённые байты удаляются фоновой очисткой раз в минуту, неиспользуемые картинки — через 30–60 дней, брошенные загрузки — через 24 часа. Предела места на человека нет: только 512 МБ на файл, 10 000 картинок в файле, пределы частоты и не больше 8 одновременных загрузок на человека и 32 на сервер (сверх — 429 `BUSY`): следите за диском. Загрузка, которая в среднем медленнее 32 КиБ/с после минуты форы или молчит дольше минуты, обрывается с 408.

Правки идут журналом в Postgres (`doc_update`). Журнал документа сжимается в файл на томе `ferrum-data`, когда в нём больше 2000 записей или 32 МБ и через 5 минут после ухода последнего участника. Журнал файла не растёт сверх 1 ГБ: дальше сервер просит подождать, правки ждут в браузерах. Сжатие, которому не удаётся применить запись, оставляет файл и журнал как есть, пишет в журнал сервера номер записи и пробует снова через час: строку `Сжатие: файл … не применилось обновление` стоит искать. Сервер держит в памяти документы файлов, в которые сейчас пишут: до 512 МБ (`FERRUM_VALIDATOR_BUDGET`), параллельные правки дерева страницы на 100 000 узлов прибавляют к этому до 0,5 ГБ на документ.

Смена `BETTER_AUTH_SECRET` не ломает ссылки «Поделиться», но показать прежнюю ссылку снова будет нельзя: нужна «Новая ссылка».
````

В `docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md` заменить:

````markdown
Индикатор в шапке: «Синхронизировано», «Сохранение…», «Нет сети — правки сохранятся при подключении». Переподключение: от 0,5 до 30 с с разбросом, сразу при событии `online`. Правки без `ack` хранятся в журнале IndexedDB и отправляются заново; Loro пропускает уже известные, поэтому повтор безвреден.

## 9. Присутствие (этап 4)

````

на:

````markdown
Индикатор в шапке: «Синхронизировано», «Сохранение…», «Нет сети — правки сохранятся при подключении». Переподключение: от 0,5 до 30 с с разбросом, сразу при событии `online`. Правки без `ack` хранятся в журнале IndexedDB и отправляются заново; Loro пропускает уже известные, поэтому повтор безвреден.

- **Сделано (этап 3):**
  - **Ядро.** `Document::apply_remote` обновляет зеркало и `Changes` по событиям Loro только у задетых узлов; события при параллельных правках шумные, поэтому правда — текущее состояние дерева. Больше 20 000 узлов за пачку, скрытые узлы и двойники `NodeId` — сборка заново, сцена тоже. Подписка на события только у файлов на сервере. **(поведение)** Узел без типа — половина чужой транзакции: файл открывается, узла для модели пока нет. **(поведение)** Два живых узла с одним `NodeId` (отмена удаления и параллельный перенос): имя у исходного, иначе у меньшего `TreeID`, другой живёт под своим, ничего не теряется. Имя зависит от того, жив ли второй, поэтому и после своей правки или отмены на такой странице зеркало собирается заново: когда двойник остался один, имя возвращается к нему, а сцена получает смену имён и перецепляет детей. Скрытый узел в этом не участвует и имя у живого не отбирает (B-328). Дельты отмены текста преобразуются через чужие правки; операция над исчезнувшим узлом и перенос в цикл пропускаются, пустой шаг — тоже. **(поведение)** Чужая вставка ровно в позиции каретки её не двигает. **(поведение)** Страницу удалил другой — переход на первую с уведомлением; все страницы удалены разом — появляется новая.
  - **Курсор.** Что отправить, знает только ядро: `subscribe` со своей версией, `subscribed` с версией сервера, всё сверх неё уходит кусками меньше кадра и порциями до 32 МБ. Обрыв, рестарт сервера, отказ «повторите» и конец паузы — один путь. Очередей правок вне документа и журнала браузера нет.
  - **Сервер.** Сверх §7.2: `subscribed`, `refused`, `pause`, `ping`/`pong`, `please-reload`; `role` несёт `canShare` и `version` (B-296). `doc_state`, `doc_update`, `doc_peer`. Первый писатель пира его занимает: подделка чужого пира (`FOREIGN_PEER`) заняла бы чужие счётчики, и настоящие правки Loro пропускал бы. Роль проверяется в транзакции каждой группы.
  - **Расхождение с §7.3: документы в памяти сервера.** Спека: «сервер не держит документы на горячем пути». Проба: случайная порча с верной контрольной суммой — 2 % пачек, прошедших разбор и пустой документ, роняют Loro на настоящем документе, паника wasm у всех участников. Поэтому пачка до журнала применяется к настоящему документу в отдельном Bun Worker: загрузка по первой пачке, вытеснение через 2 минуты простоя и по бюджету 512 МБ (`FERRUM_VALIDATOR_BUDGET`), паника — отказ этой пачке и новый worker. Снимок вместо обновления не принимается.
  - **Расхождение с §7.5.** `seq` в заголовке контейнера не нужен: подписка идёт по вектору версий, а его ядро знает после открытия. Кэш сцены у неизменённых страниц в контейнере остаётся: он верен.
  - **Сжатие** (§7.4): `compact.ts` в Bun Worker, новый контейнер и удаление вошедших записей одной транзакцией. Записи после начала прохода остаются. **(поведение)** Новая страница без правок получает пустую секцию, иначе ядро сочло бы файл повреждённым. «Скачать копию» сначала сжимает (`POST …/compact`), иначе копия была бы старше того, что видят люди. Запись файла целиком при идущем журнале — `CONFLICT`: вкладка прежней версии не затрёт правки.
  - **Первая запись.** Файл целиком пишется один раз, сразу при открытии нового файла, без правок: журналу надо к чему лечь, а второй открывший получает то же самое, а не свой отдельный документ. Опередили, своих правок нет — файл открывается заново; правки уже есть — плашка «Файл уже записан» этапа 2.
  - **Клиент.** `core/sync-client.ts` без DOM, `worker/sync.worker.ts`, прямой порт с worker'ом ядра. Основной поток видит только состояние и роль. Запрет второй вкладки снят. Журнал браузера снимается подтверждением сервера и только своими или прочитанными при открытии пачками: другая вкладка могла ещё ничего не отправить. Картинка уходит раньше правки со ссылкой, чужая скачивается по `ImagesNeeded`. Превью — по «Сохранить», при уходе и раз в минуту правок, отдельно от файла.
  - **Что видит человек.** «Сохранить» ждёт сервер до 5 с; не дождались — уход спрашивает, причина: правки сохранены в браузере и уйдут при следующем открытии. Предел частоты — `pause` и «Сохранение…», а не «Нет сети». Отказ, который повтор не исправит (`BAD_UPDATE`, `FOREIGN_PEER`, картинка не принята, 6 `GAP` подряд) — плашка «Правки не сохраняются» с копией на диск, чужие правки при этом идут. Понижение с неподтверждёнными правками — «Правки не дошли до сервера». Соединение не открывается — спрашиваем `GET access`: сессия и доступ не прячутся за вечным «Нет сети».
  - **Закрытие сразу (B-237).** Снятие роли, выключение и замена ссылки, корзина, снятие из команды: маршрут сообщает из конвейера API, забыть нельзя; раз в 30 с сверка на изменения мимо маршрутов. Выход — соединения сессии, «на всех» — человека, вход поверх входа — прежней. Раз в минуту проверка сессии; сбой Valkey никого не выгоняет. Опрос `access` снят, пока есть соединение.
  - **Пределы, проверенные настоящей работой.** Кадр 16 МБ; на соединение 120 кадров в секунду (запас 2000), на человека 480 и 64 МБ/с (запас 256 МБ), 64 соединения на человека, 20 000 на сервер, 4096 документов на соединение, журнал файла 1 ГБ. Тест: перетаскивание 30 раз в секунду, набор, вставка кадрами по 5 МБ — ни одной паузы. Медленный читатель: буфер сокета ограничен 8 МБ, дальше догоняет из базы по порядку; не вычитывает 2 минуты — закрытие.
  - **Замеры** (Apple Silicon, 100 000 узлов, `sync_report`, `hub-load.test.ts`, `sync.spec.ts`):
    - Чужая одиночная правка нативно, цель 2 мс: свойство p95 37 мкс, новый узел 42 мкс, перенос 27 мкс, удаление 21 мкс, рядом со своей правкой свойства 46 мкс. Самая первая после открытия — около 30 мс, один раз (B-318).
    - **Цель не выполнена:** правка дерева параллельно своей правке дерева — 50 мс, первая 0,5–0,7 с, плюс около 0,5 ГБ памяти wasm на документ. Loro обходит всё дерево, это внутри него. Идёт в worker'ах, основной поток не задет, входящие складываются в одну пачку, пока ядро занято (B-309).
    - Своя правка видна другому, два браузера, у получателя идёт жест, цель 150 мс: p50 85–88 мс; p95 по 20 замеров на четырёх прогонах 102, 112, 153 и 210 мс, то есть цель выдержана не на каждом прогоне; причина выбросов не разобрана, машина под нагрузкой (B-327). Длинных задач основного потока нет.
    - От кадра до рассылки, 50 участников, цель 40 мс: p95 36 мс. Запас небольшой: 20 мс из них — окно группы.
    - Соединение: 62 КБ. 20 000 правок: запись 4,8 с, догоняние 0,85 с, применение 0,9 с. Сжатие 2 с. Рост файла: 4,4 байта на правку свойства (риск 2 раздела 14). Память worker'а сжатия отдельно не мерилась (риск 4).
    - Кадр с курсорами и открытие из кэша — этапы 4 и 6.

## 9. Присутствие (этап 4)

````

- [ ] **Step 2: Проверить**

Run: `npm run format:check`

Run: `npm run lint`

Run: `npm run typecheck`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo fmt --all --check`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo clippy --workspace --all-targets -- -D warnings`

Run: `npm run test:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run unit`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run build:server`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run build && npm run e2e:ci`

Run: `cd apps/web && export CARGO_TARGET_DIR="$PWD/../../target" CARGO_INCREMENTAL=0 && npm run e2e:server`

Run: `export CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 && cargo test --workspace`

Expected: всё PASS: `bun test` 23 и 414 pass; модульные 278 passed; обе сборки; `e2e:ci` 416 тестов (`--retries=1`); `e2e:server` 163 passed, 9 пропущены; `cargo test --workspace` без падений

- [ ] **Step 3: Закоммитить**

```bash
git add docs/backlog.md docs/running.md docs/superpowers/specs/2026-11-04-ferrum-backend-collab-design.md
git commit -F - <<'EOF'
docs: синхронизация — итоговое поведение этапа 3, бэклог B-309…B-327

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

## Что этап 3 сознательно не делает

Курсоры, аватары и следование (этап 4). Комментарии (этап 5). Очередь правок на долгий офлайн, открытие из кэша без сети и история версий (этап 6): при обрыве правки ждут в журнале браузера, открытый редактор продолжает работать. Серверные файлы в десктопе (этап 7). Рассылка между несколькими процессами сервера (B-315). Проверка семантики пачек (B-310).
