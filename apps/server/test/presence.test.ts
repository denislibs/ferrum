// Присутствие на настоящем сервере (спека подсистемы 4, раздел 9): настоящие сокеты, база и сессии. Кто участник — знает
// только сервер; уход, снятие роли и выход убирают человека у других сразу; пределы молча отбрасывают, не рвут.

import { afterAll, beforeEach, expect, spyOn, test } from 'bun:test';
import * as protocol from '@ferrum/protocol';
import {
  encodeClientFrame,
  encodePartBody,
  encodePresenceBody,
  encodeSelectionIds,
  parseServerFrame,
  presenceColor,
  PRESENCE_FRAME_HOSTILE,
  splitSelection,
  SYNC_CLOSE,
  syncPath,
  type PresenceItem,
  type PresenceState,
} from '@ferrum/protocol';
import { connect as tcp } from 'node:net';
import { apiStand, type Person } from './api-stand';
import { clip, PresenceHub, PRESENCE_ANON_NAME, type PresenceConn, type PresenceLimits } from '../src/sync/presence';
import { Client } from './sync-client';

// Срок устаревания — обычный (30 с), дольше любого ожидания в тестах: «пропал сразу» не спутать с «пропал по сроку».
// Устаревание проверяется на `PresenceHub` с часами теста.
const s = await apiStand({ syncLimits: { recheckMs: 3_600_000, sessionCheckMs: 3_600_000 } });
afterAll(s.stop);
const owner = await s.signIn('p-owner@example.com', 'Анна');
const editor = await s.signIn('p-editor@example.com', 'Борис');
const viewer = await s.signIn('p-viewer@example.com', 'Вера');
const other = await s.signIn('p-other@example.com', 'Глеб');
const leaver = await s.signIn('p-leaver@example.com', 'Дина');
const nameless = await s.signIn('p-nameless@example.com', 'Ева');

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

/** Всё, что клиент послал до этого, сервер обработал: кадры одного сокета идут по порядку, `pong` — за ними. */
async function settled(c: Client): Promise<void> {
  const pongs = () => c.frames.filter((f) => f.t === 'pong').length;
  const n = pongs();
  c.send(encodeClientFrame({ t: 'ping' }));
  await c.wait('pong', () => pongs() > n);
}

beforeEach(async () => {
  for (const c of open.splice(0)) c.close();
  await s.reset();
  file = (await s.newFile(owner)).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer'), ($1, $4, 'viewer')`, [
    file,
    editor.id,
    viewer.id,
    nameless.id,
  ]);
});

test('участник виден другим: кто, имя и цвет — с сервера, своё состояние себе не приходит', async () => {
  // Соединение с другим файлом раньше: номер соединения сервера у следующих — не 1.
  await join(other, (await s.newFile(other)).id);
  const a = await join(owner);
  const b = await join(editor);
  send(a, state(42));
  const j = await waitItem(b, 'join');
  expect(j).toMatchObject({ user: owner.id, name: 'Анна', color: presenceColor(owner.id) });
  // Номер участника — в файле (первый вошедший — 1), а не номер соединения сервера: тот растёт с каждым тестом.
  expect(j.participant).toBe(1);
  const st = await waitItem(b, 'state', (i) => i.participant === j.participant);
  expect(st.state).toEqual(state(42));
  // Своё состояние отмечено раньше чужого: пришло бы не позже него.
  send(b, state(43));
  const jb = await waitItem(a, 'join');
  await waitItem(a, 'state', (i) => i.state.cursor?.[0] === 43);
  expect(items(a).every((i) => i.participant === jb.participant)).toBe(true);
});

test('кадр нельзя выдать за другого: имени и userId в нём нет, лишние байты — молча мимо, соединение живо', async () => {
  const a = await join(owner);
  const b = await join(editor);
  const dropped = s.hub.presence.dropped;
  const body = encodePresenceBody(state(1));
  // Хвост с «чужим» именем: разбор строгий, кадр отбрасывается целиком.
  a.send(encodeClientFrame({ t: 'presence', body: new Uint8Array([...body, ...new TextEncoder().encode(editor.id)]) }));
  // Нечисловой курсор.
  const nan = body.slice();
  new DataView(nan.buffer).setFloat64(nan.length - 8, NaN, true);
  a.send(encodeClientFrame({ t: 'presence', body: nan }));
  // Кадр больше 2 КБ.
  a.send(encodeClientFrame({ t: 'presence', body: new Uint8Array(4096) }));
  send(a, state(2));
  await settled(a);
  // Все три отброшены при приёме, а не слиты в тике с верным.
  expect(s.hub.presence.dropped - dropped).toBe(3);
  expect(a.closed).toBeNull();
  expect((await waitItem(b, 'join')).user).toBe(owner.id);
  await waitItem(b, 'state', (i) => i.state.cursor?.[0] === 2);
  expect(items(b).every((i) => i.k === 'join' || (i.k === 'state' && i.state.cursor?.[0] === 2))).toBe(true);
});

test('кадр присутствия больше 64 КБ — враждебный: соединение закрывается', async () => {
  const a = await join(owner);
  a.send(encodeClientFrame({ t: 'presence', body: new Uint8Array(PRESENCE_FRAME_HOSTILE) }));
  expect((await a.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);
  const c = await join(owner);
  c.send(encodeClientFrame({ t: 'presence-part', body: new Uint8Array(PRESENCE_FRAME_HOSTILE) }));
  expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);
});

test('сверх предела частоты тело кадра не разбирается: поток частей и кадров больше 2 КБ стоит проверки длины и бакета', async () => {
  const a = await join(owner);
  const b = await join(editor);
  send(a, state(1, { selection: { rev: 1, count: 2028, parts: 16, bounds: [0, 0, 1, 1] } }));
  await waitItem(b, 'state');
  // Часть ~2 КБ: один пир, 2028 узлов разностью 1 — самая дорогая для разбора.
  const nodes = new Uint8Array(2038);
  new DataView(nodes.buffer).setBigUint64(0, 2n ** 64n - 1n, true);
  nodes[8] = (2028 & 0x7f) | 0x80;
  nodes[9] = 2028 >> 7;
  nodes.fill(1, 11);
  const part = encodeClientFrame({ t: 'presence-part', body: encodePartBody({ rev: 1, index: 0, parts: 16, bytes: nodes }) });
  const big = encodeClientFrame({ t: 'presence', body: new Uint8Array(4096) });
  const spies = [
    spyOn(protocol, 'checkPartBody'),
    spyOn(protocol, 'parsePartBody'),
    spyOn(protocol, 'decodeSelectionIds'),
    spyOn(protocol, 'parsePresenceBody'),
  ];
  try {
    const dropped = s.hub.presence.dropped;
    for (let i = 0; i < 100; i++) a.send(big);
    for (let i = 0; i < 2500; i++) a.send(part);
    await settled(a);
    expect(s.hub.presence.dropped - dropped).toBeGreaterThan(2400);
    // Разобрано не больше запаса бакета соединения (60) и пополнения за время потока; кадры больше 2 КБ — ни одного.
    const calls = spies.reduce((n, x) => n + x.mock.calls.length, 0);
    expect(calls).toBeLessThan(120);
    expect(spies[3].mock.calls.length).toBe(0);
    expect(a.closed).toBeNull();
  } finally {
    for (const x of spies) x.mockRestore();
  }
});

test('присутствие не уходит в другой файл и тому, у кого нет доступа', async () => {
  const otherFile = (await s.newFile(other)).id;
  const a = await join(owner);
  const elsewhere = await join(other, otherFile);
  expect(await Client.open(s, other, file)).toBeNull();
  send(a, state(5));
  send(elsewhere, state(6));
  const b = await join(editor);
  const beside = await join(other, otherFile);
  await waitItem(b, 'state', (i) => i.state.cursor?.[0] === 5);
  // В другом файле приходит своё — а состояние из этого файла пришло бы не позже.
  send(elsewhere, state(16));
  await waitItem(beside, 'state', (i) => i.state.cursor?.[0] === 16);
  expect(items(beside).some((i) => i.k === 'state' && i.state.cursor?.[0] === 5)).toBe(false);
  expect(items(elsewhere)).toEqual([]);
  expect(items(b).some((i) => i.k === 'state' && i.state.cursor?.[0] !== 5)).toBe(false);
});

test('читатель виден и видит других (как в Figma)', async () => {
  const a = await join(owner);
  const v = await join(viewer);
  send(v, state(7));
  send(a, state(8));
  expect((await waitItem(a, 'join')).user).toBe(viewer.id);
  expect((await waitItem(v, 'join')).user).toBe(owner.id);
});

test('жест — только с правом правки: у читателя другим уходят курсор и выделение без живого превью', async () => {
  const a = await join(owner);
  const e = await join(editor);
  const v = await join(viewer);
  const gesture = { kind: 'move', transform: [1, 0, 0, 1, 30, 40], frame: [1, 0, 0, 1, 0, 0], k: 1 } as const;
  const selection = { rev: 3, count: 2, parts: 1, bounds: [0, 0, 10, 10] as [number, number, number, number] };
  const g = () => ({ ...gesture, transform: [...gesture.transform] as typeof gesture.transform, frame: [...gesture.frame] });
  send(v, state(7, { selection, gesture: g() as PresenceState['gesture'] }));
  send(e, state(8, { selection, gesture: g() as PresenceState['gesture'] }));
  const jv = await waitItem(a, 'join', (i) => i.user === viewer.id);
  const je = await waitItem(a, 'join', (i) => i.user === editor.id);
  const fromViewer = await waitItem(a, 'state', (i) => i.participant === jv.participant);
  expect(fromViewer.state).toEqual(state(7, { selection, gesture: null }));
  const fromEditor = await waitItem(a, 'state', (i) => i.participant === je.participant);
  expect(fromEditor.state.gesture?.transform).toEqual([1, 0, 0, 1, 30, 40]);
});

test('роль понижена посреди жеста: превью у других снимается сразу, без нового кадра', async () => {
  const a = await join(owner);
  const e = await join(editor);
  const gesture = { kind: 'move', transform: [1, 0, 0, 1, 30, 40], frame: [1, 0, 0, 1, 0, 0], k: 1 } as PresenceState['gesture'];
  send(e, state(8, { gesture }));
  const je = await waitItem(a, 'join', (i) => i.user === editor.id);
  await waitItem(a, 'state', (i) => i.participant === je.participant && i.state.gesture !== null);
  await s.pool.query(`UPDATE file_access SET role = 'viewer' WHERE file_id = $1 AND user_id = $2`, [file, editor.id]);
  await s.hub.recheckAll();
  await e.wait('role', (f) => f.role === 'viewer');
  const st = await waitItem(a, 'state', (i) => i.participant === je.participant && i.state.gesture === null);
  expect(st.state).toEqual(state(8));
  // Пришедший позже получает уже без жеста.
  const late = await join(owner);
  const got = await waitItem(late, 'state', (i) => i.participant === je.participant);
  expect(got.state.gesture).toBeNull();
});

test('без имени у присутствия — «Участник», не часть почты', async () => {
  await s.pool.query(`UPDATE "user" SET name = ' ' WHERE id = $1`, [nameless.id]);
  const a = await join(owner);
  const n = await join(nameless);
  send(n, state(1));
  const j = await waitItem(a, 'join');
  expect(j.name).toBe(PRESENCE_ANON_NAME);
  expect(j.name).not.toContain('p-nameless');
});

test('пришедший позже получает всех сразу: состояние и выделение частями', async () => {
  const a = await join(owner);
  const ids = Array.from({ length: 1000 }, (_, i) => `${10 + i * 12}@7910247476534078275`);
  const parts = splitSelection(ids);
  send(a, state(1, { selection: { rev: 4, count: ids.length, parts: parts.length, bounds: [0, 0, 10, 10] } }));
  parts.forEach((bytes, index) =>
    a.send(encodeClientFrame({ t: 'presence-part', body: encodePartBody({ rev: 4, index, parts: parts.length, bytes }) })),
  );
  await settled(a);
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
  // Прежняя часть пришла бы вместе с новой, не позже следующего состояния.
  send(a, state(2, { selection: { rev: 2, count: 1, parts: 1, bounds: [0, 0, 1, 1] } }));
  await waitItem(late, 'state', (i) => i.state.cursor?.[0] === 2);
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
  const dropped = s.hub.presence.dropped;
  // Кадр сразу за `hello`: роль ещё проверяется запросом.
  send(c, state(99));
  await c.wait('role');
  send(c, state(100));
  await waitItem(b, 'state', (i) => i.state.cursor?.[0] === 100);
  // Кадры сокета — по порядку: раз дошёл второй, первый уже обработан — и отброшен, а не слит со вторым в тике.
  expect(s.hub.presence.dropped - dropped).toBe(1);
  expect(items(b).some((i) => i.k === 'state' && i.state.cursor?.[0] === 99)).toBe(false);
});

test('молчит дольше срока — пропадает; следующий кадр возвращает его', () => {
  const t = new Clock();
  const hub = new PresenceHub({ staleMs: 1000 }, t.now);
  const a = fake(1, 'a');
  const b = fake(2, 'b');
  hub.admit(a);
  hub.admit(b);
  hub.state(a, 'А', encodePresenceBody(state(1)));
  hub.tick();
  expect(b.got().map((i) => i.k)).toEqual(['join', 'state']);
  t.at += 999;
  hub.tick();
  expect(b.got()).toEqual([]);
  t.at += 2;
  hub.tick();
  expect(b.got().map((i) => i.k)).toEqual(['left']);
  hub.state(a, 'А', encodePresenceBody(state(2)));
  hub.tick();
  expect(b.got().map((i) => i.k)).toEqual(['join', 'state']);
  hub.stop();
});

test('сверх предела частоты кадры молча отбрасываются: ни паузы, ни закрытия; получатель видит последнее', async () => {
  const a = await join(owner);
  const b = await join(editor);
  const dropped = s.hub.presence.dropped;
  for (let i = 0; i < 2500; i++) send(a, state(i));
  await settled(a);
  // Больше запаса бакета правок (2000): присутствие его не тратит, иначе правки получили бы паузу.
  expect(s.hub.presence.dropped - dropped).toBeGreaterThan(2000);
  expect(a.closed).toBeNull();
  expect(a.frames.some((f) => f.t === 'pause')).toBe(false);
  // Правки после потока не ждут: присутствие не тратит бакеты правок.
  a.subscribe('root');
  await a.wait('subscribed');
  // Рассылка сливает кадры: принятых — запас бакета (60), а пришедших состояний меньше.
  await waitItem(b, 'state');
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
    // Темп настоящего клиента: 20 кадров в секунду.
    await Bun.sleep(50);
  }
  await settled(a);
  await settled(tab2);
  expect(s.hub.presence.dropped - dropped).toBe(0);
  await waitItem(b, 'state', (i) => i.state.gesture?.transform[4] === 59);
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

test('предел на человека: шесть вкладок в пределах своих 30 в секунду, а в сумме больше 120 — лишнее отбрасывается, вкладки живы', async () => {
  const tabs = [];
  for (let i = 0; i < 6; i++) tabs.push(await join(owner));
  const b = await join(editor);
  const dropped = s.hub.presence.dropped;
  // По 50 кадров на вкладку — меньше запаса соединения (60), всего 300 против запаса человека 240 (+ пополнение за миллисекунды).
  for (let i = 0; i < 50; i++) for (const [k, t] of tabs.entries()) send(t, state(i, { cursor: [i, k] }));
  for (const t of tabs) await settled(t);
  expect(s.hub.presence.dropped - dropped).toBeGreaterThan(30);
  for (const t of tabs) {
    expect(t.closed).toBeNull();
    expect(t.frames.some((f) => f.t === 'pause')).toBe(false);
  }
  await waitItem(b, 'state');
});

/** Часы теста. */
class Clock {
  at = 0;
  now = () => this.at;
}

/** Соединение без сокета: пачки складываются, `got()` отдаёт пришедшее с прошлого вызова. */
function fake(id: number, user: string, role: PresenceConn['role'] = 'editor', buffered = 0) {
  const batches: Uint8Array[] = [];
  let seen = 0;
  const conn = {
    id,
    file: 'f',
    user,
    role,
    closed: false,
    ws: {
      getBufferedAmount: () => buffered,
      sendBinary: (b: Uint8Array) => {
        batches.push(b.slice());
        return b.length;
      },
    },
    got(): PresenceItem[] {
      const out = batches.slice(seen).flatMap((b) => {
        const f = parseServerFrame(b);
        return f?.t === 'presence' ? f.items : [];
      });
      seen = batches.length;
      return out;
    },
  };
  return conn;
}

test('получатель, который не читает, не копит отметки об уходе тех, о ком не знает: ожидающее ограничено живыми участниками', () => {
  const t = new Clock();
  const hub = new PresenceHub({}, t.now);
  hub.admit(fake(1, 'slow', 'editor', 1_000_000));
  const body = encodePresenceBody(state(1));
  for (let i = 0; i < 500; i++) {
    const c = fake(1000 + i, `u${i}`);
    hub.admit(c);
    hub.state(c, `u${i}`, body);
    t.at += 40;
    hub.tick();
    hub.leave(c);
    t.at += 40;
    hub.tick();
  }
  expect(hub.shown('f')).toBe(0);
  expect(hub.pendingMax('f')).toBe(0);
  // Живые участники — граница: по записи на каждого.
  for (let i = 0; i < 5; i++) {
    const c = fake(5000 + i, `live${i}`);
    hub.admit(c);
    hub.state(c, `live${i}`, body);
  }
  expect(hub.pendingMax('f')).toBeLessThanOrEqual(5);
  hub.stop();
});

test('пределы соединений в файле и у человека: сверх — присутствие не принимается и не рассылается; место освободилось — ждущий входит', () => {
  const limits: Partial<PresenceLimits> = { roomConnections: 3, userConnections: 2 };
  const t = new Clock();
  const hub = new PresenceHub(limits, t.now);
  const a1 = fake(1, 'a');
  const a2 = fake(2, 'a');
  const a3 = fake(3, 'a');
  const b = fake(4, 'b');
  const c = fake(5, 'c');
  for (const x of [a1, a2, a3, b, c]) hub.admit(x);
  const body = (x: number) => encodePresenceBody(state(x));
  // a3 — третье соединение человека, c — четвёртое в файле на три места.
  const dropped = hub.dropped;
  hub.state(a3, 'А', body(3));
  hub.state(c, 'В', body(5));
  expect(hub.dropped - dropped).toBe(2);
  hub.state(b, 'Б', body(4));
  hub.tick();
  expect(a1.got().map((i) => i.k)).toEqual(['join', 'state']);
  expect(a2.got().map((i) => i.k)).toEqual(['join', 'state']);
  expect(a3.got()).toEqual([]);
  expect(c.got()).toEqual([]);
  // Ушло соединение a1: первым по приходу ждал a3, у человека снова место — он входит и получает всех; c ждёт дальше.
  // Вошедший виден другим сразу — с последним состоянием, присланным, пока ждал, а не со следующим кадром.
  hub.leave(a1);
  hub.tick();
  expect(a3.got().map((i) => i.k)).toEqual(['join', 'state']);
  expect(c.got()).toEqual([]);
  const seen = a2.got();
  expect(seen).toMatchObject([
    { k: 'join', participant: 4, user: 'a' },
    { k: 'state', participant: 4 },
  ]);
  expect(seen[1].k === 'state' && seen[1].state.cursor?.[0]).toBe(3);
  // Ушёл b — входит c (номер 5: a1, a2, b, затем a3 и c) с последним из присланных, пока ждал.
  hub.state(c, 'В', body(6));
  hub.leave(b);
  hub.tick();
  expect(c.got().map((i) => i.k)).toEqual(['join', 'state']);
  const after = a2.got();
  expect(after).toMatchObject([
    { k: 'left', participant: 3 },
    { k: 'join', participant: 5, user: 'c' },
    { k: 'state', participant: 5 },
  ]);
  expect(after[2].k === 'state' && after[2].state.cursor?.[0]).toBe(6);
  hub.stop();
});

test('все соединения файла ушли, в том числе ждущие, — комнаты файла нет', () => {
  const hub = new PresenceHub({ roomConnections: 1 }, new Clock().now);
  const rooms = () => (hub as unknown as { rooms: Map<string, unknown> }).rooms.size;
  const x = fake(1, 'x');
  const y = fake(2, 'y');
  const z = fake(3, 'z');
  for (const c of [x, y, z]) hub.admit(c);
  hub.state(x, 'Икс', encodePresenceBody(state(1)));
  hub.state(y, 'Игрек', encodePresenceBody(state(2)));
  // Ждущий закрылся раньше, чем освободилось место.
  y.closed = true;
  hub.leave(y);
  hub.leave(x);
  hub.leave(z);
  hub.tick();
  expect(rooms()).toBe(0);
  expect(hub.heldBytes()).toBe(0);
  hub.stop();
});

test('номер участника — счётчик файла: у вошедшего позже номер больше', () => {
  const t = new Clock();
  const hub = new PresenceHub({}, t.now);
  const watcher = fake(900, 'w');
  hub.admit(watcher);
  const x = fake(77, 'x');
  const y = fake(12, 'x');
  hub.admit(x);
  hub.admit(y);
  hub.state(x, 'Икс', encodePresenceBody(state(1)));
  hub.state(y, 'Икс', encodePresenceBody(state(2)));
  hub.tick();
  const joins = watcher.got().flatMap((i) => (i.k === 'join' ? [i.participant] : []));
  expect(joins).toEqual([2, 3]);
  hub.stop();
});

test('имя для подписи: без символов направления и нулевой ширины', () => {
  expect(clip('\u202eАнна\u200b\u2066\ufeff')).toBe('Анна');
  expect(clip('Ан\u200dна\u2060\u2064')).toBe('Анна');
  expect(clip('\u202e\u200f')).toBe('');
});
