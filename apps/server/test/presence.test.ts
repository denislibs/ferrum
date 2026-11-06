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
import { PresenceHub } from '../src/sync/presence';
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

test('предел на человека: шесть вкладок в пределах своих 30 в секунду, а в сумме больше 120 — лишнее отбрасывается, вкладки живы', async () => {
  const tabs = [];
  for (let i = 0; i < 6; i++) tabs.push(await join(owner));
  const b = await join(editor);
  const dropped = s.hub.presence.dropped;
  // По 50 кадров на вкладку — меньше запаса соединения (60), всего 300 против запаса человека 240 (+ пополнение за миллисекунды).
  for (let i = 0; i < 50; i++) for (const [k, t] of tabs.entries()) send(t, state(i, { cursor: [i, k] }));
  await ticks();
  expect(s.hub.presence.dropped - dropped).toBeGreaterThan(30);
  for (const t of tabs) {
    expect(t.closed).toBeNull();
    expect(t.frames.some((f) => f.t === 'pause')).toBe(false);
  }
  await waitItem(b, 'state');
});

test('получатель, который не читает, не копит отметки об уходе тех, о ком не знает: ожидающее ограничено живыми участниками', () => {
  let t = 0;
  const hub = new PresenceHub({}, () => t);
  const conn = (id: number, user: string, buffered: number) => ({
    id,
    file: 'f',
    user,
    closed: false,
    ws: { getBufferedAmount: () => buffered, sendBinary: () => 0 },
  });
  hub.admit(conn(1, 'slow', 1_000_000));
  const body = encodePresenceBody(state(1));
  for (let i = 0; i < 500; i++) {
    const c = conn(1000 + i, `u${i}`, 0);
    hub.admit(c);
    hub.state(c, `u${i}`, body, state(1));
    t += 40;
    hub.tick();
    hub.leave(c);
    t += 40;
    hub.tick();
  }
  expect(hub.shown('f')).toBe(0);
  expect(hub.pendingMax('f')).toBe(0);
  // Живые участники — граница: по записи на каждого.
  for (let i = 0; i < 5; i++) {
    const c = conn(5000 + i, `live${i}`, 0);
    hub.admit(c);
    hub.state(c, `live${i}`, body, state(1));
  }
  expect(hub.pendingMax('f')).toBeLessThanOrEqual(5);
  hub.stop();
});
