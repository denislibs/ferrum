// Соединение файла: обрыв, пауза, отказы и порядок, без настоящего сервера. Сокет — настоящий по договору браузера: `send` копит в
// `bufferedAmount`, пока «сеть» не вычитает; часы — ручные: ничего не ждёт настоящего времени, и видно, какой таймер идёт во время паузы.

import { expect, test } from '@playwright/test';
import {
  encodeClientFrame,
  encodePartBody,
  encodePresenceBatch,
  encodePresenceBody,
  encodePresenceRecord,
  encodeSelectionIds,
  encodeServerFrame,
  encodeVersion,
  parseClientFrame,
  SYNC_CLOSE,
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
  PRESENCE_HIGH_WATER,
  PRESENCE_RATE,
  RECONNECT_MAX_MS,
  SyncClient,
  type CoreCommand,
  type Participant,
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
  receiveRaw(b: Uint8Array): void {
    this.onmessage?.({ data: b.slice().buffer });
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
  const participants: Participant[][] = [];
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
    participants: (list) => participants.push(list),
  };
  const client = new SyncClient(env);
  return {
    client,
    clock,
    sockets,
    commands,
    participants,
    flushes,
    statuses,
    access,
    reloads: () => reloads,
    last: () => sockets.at(-1)!,
    status: () => statuses.at(-1)?.status,
    flushing: () => flushes.at(-1),
    /** Ядро выполнило `syncReset`: всё, что оно пришлёт дальше, — новое поколение. */
    reset(): void {
      (client as unknown as { onReset?(): void }).onReset?.();
    },
    /** Ядро подписалось на документы, сервер ответил. */
    subscribe(ws: FakeSocket, docs: string[] = ['root']): void {
      for (const doc of docs) client.onCore({ type: 'syncSubscribe', doc, version: encodeVersion(new Map()) });
      for (const doc of docs) ws.receive({ t: 'subscribed', doc, seq: 0, version: encodeVersion(new Map()) });
    },
    live(): FakeSocket {
      client.start();
      const s = sockets.at(-1)!;
      s.open();
      this.reset();
      this.subscribe(s);
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
/** Время до следующего сокета, и не дальше: у нового сокета свой срок на открытие. */
const nextSocket = (s: ReturnType<typeof stand>): FakeSocket => {
  const n = s.sockets.length;
  for (let waited = 0; s.sockets.length === n; waited += 50) {
    if (waited > RECONNECT_MAX_MS) throw new Error('переподключения нет');
    s.clock.advance(50);
  }
  return s.last();
};

test('открытие: hello, сброс курсора, ядро отдаёт подписки; подтверждение — ядру', () => {
  const s = stand();
  s.client.start();
  expect(s.commands[0]).toEqual({ type: 'syncStart' });
  expect(s.status()).toBe('connecting');
  const ws = s.last();
  ws.open();
  ws.drain();
  expect(ws.sent[0]).toEqual({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 });
  expect(s.commands).toContainEqual({ type: 'syncReset' });
  expect(s.flushing()).toBe(true);
  s.reset();
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
  s.reset();
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
  s.reset();
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
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
  // Шаг ядра идёт (он отдаёт подписки), правки не уходят.
  s.client.onCore(update(9));
  expect(updatesSent(ws)).toEqual([]);
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
    s.client.onCore(update(2));
    expect(updatesSent(ws)).toEqual([]);
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
  ws.receive({ t: 'snapshot-needed', doc: 'root', url: '/api/files/f/docs/root/snapshot' });
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
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
  s.clock.advance(1000);
  for (let i = 0; i < 4; i++) {
    // Снимок применён, подписка заново — и снова просьба о снимке.
    expect(s.status()).not.toBe('rejected');
    s.reset();
    s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
    ws.receive({ t: 'snapshot-needed', doc: 'root', url: '/api/files/f/docs/root/snapshot' });
    await tick();
    await tick();
    s.client.onApplied();
  }
  expect(s.status()).toBe('rejected');
  expect(s.commands.filter((c) => c.type === 'applyRemote')).toHaveLength(3);
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

const updatesSent = (ws: FakeSocket) => {
  ws.drain();
  return ws.sent.filter((f) => f.t === 'update').map((f) => (f as { clientSeq: number }).clientSeq);
};
const resets = (s: ReturnType<typeof stand>) => s.commands.filter((c) => c.type === 'syncReset').length;

test('«повторите» и отказы пачкам в пути — одно поколение: не вечный отказ (T8-I1)', async () => {
  const s = stand();
  const ws = s.live();
  for (let seq = 1; seq <= 7; seq++) s.client.onCore(update(seq));
  await tick();
  expect(updatesSent(ws)).toEqual([1, 2, 3, 4, 5, 6, 7]);
  // Сервер отказал первой «повторите», следующим — разрыв: они в пути и опирались на неё.
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
  for (let seq = 2; seq <= 7; seq++) ws.receive({ t: 'refused', doc: 'root', clientSeq: seq, code: 'GAP' });
  s.clock.advance(1000);
  expect(s.status()).not.toBe('rejected');
  expect(resets(s)).toBe(2);
  // Новое поколение: отказы прежним пачкам, пришедшие после подписки заново, не считаются.
  s.reset();
  s.subscribe(ws);
  for (let seq = 2; seq <= 7; seq++) ws.receive({ t: 'refused', doc: 'root', clientSeq: seq, code: 'GAP' });
  s.clock.advance(1000);
  expect(resets(s)).toBe(2);
  expect(s.status()).toBe('synced');
  expect(s.flushing()).toBe(true);
});

test('правки, собранные ядром до сброса, не уходят раньше новой подписки (T8-I1)', async () => {
  const s = stand();
  const ws = s.live();
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
  s.clock.advance(1000);
  // Сброс в пути к ядру, а пачка старого поколения уже идёт по порту.
  s.client.onCore(update(5));
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
  await tick();
  ws.drain();
  expect(ws.sent.filter((f) => f.t === 'update' || f.t === 'subscribe').length).toBe(1);
  s.reset();
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
  ws.receive({ t: 'subscribed', doc: 'root', seq: 0, version: encodeVersion(new Map()) });
  s.client.onCore(update(6));
  await tick();
  ws.drain();
  const kinds = ws.sent.map((f) => (f.t === 'update' ? `update ${f.clientSeq}` : f.t));
  expect(kinds.slice(-2)).toEqual(['subscribe', 'update 6']);
});

test('разрыв — один на поколение; шесть поколений подряд без подтверждения — отказ', async () => {
  const s = stand();
  const ws = s.live();
  let seq = 0;
  for (let gen = 0; gen < 6; gen++) {
    expect(s.status()).not.toBe('rejected');
    s.client.onCore(update(++seq));
    s.client.onCore(update(++seq));
    await tick();
    ws.receive({ t: 'refused', doc: 'root', clientSeq: seq - 1, code: 'GAP' });
    ws.receive({ t: 'refused', doc: 'root', clientSeq: seq, code: 'GAP' });
    s.clock.advance(1000);
    s.reset();
    s.subscribe(ws);
  }
  expect(s.status()).toBe('rejected');
});

test('`UNAVAILABLE` до ответа на подписку и при чужом отказе — снова подписка, а не отказ (сервер, п. 2–3)', () => {
  const s = stand();
  s.client.start();
  const ws = s.last();
  ws.open();
  s.reset();
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
  for (let i = 0; i < 20; i++) {
    ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
    s.clock.advance(1000);
  }
  expect(s.status()).not.toBe('rejected');
});

test('`DOC_LIMIT` — правки не уходят, причина своя', () => {
  const s = stand();
  const ws = s.live();
  ws.receive({ t: 'refused', doc: '1@1', clientSeq: 1, code: 'DOC_LIMIT' });
  expect(s.status()).toBe('rejected');
  s.client.onCore(update(2));
  expect(updatesSent(ws)).toEqual([]);
  expect(s.statuses.at(-1)?.detail).toContain('страниц');
});

test('закрытие «не успевает читать» (4408) и остановка сервера (1001): переподключение, не отказ (сервер, п. 6–7)', () => {
  for (const code of [SYNC_CLOSE.SLOW, SYNC_CLOSE.GOING_AWAY]) {
    const s = stand();
    s.live().dropped(code);
    expect(s.status()).toBe('offline');
    s.clock.advance(500);
    expect(s.sockets).toHaveLength(2);
  }
});

test('устаревший ответ на подписку прежнего поколения не доходит до ядра (T8-M3)', () => {
  const s = stand();
  const ws = s.live();
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
  s.clock.advance(1000);
  s.reset();
  const before = s.commands.length;
  // Ответ на прежнюю подписку: новой ещё нет.
  ws.receive({ t: 'subscribed', doc: 'root', seq: 1, version: encodeVersion(new Map([['1', 1]])) });
  expect(s.commands.slice(before).some((c) => c.type === 'syncSubscribed')).toBe(false);
  expect(s.status()).toBe('connecting');
});

test('снимок: адрес строится из своего файла, а не из кадра; документ без подписки — не повод (T8-I3)', async () => {
  const paths: string[] = [];
  const s = stand({
    snapshot: (path) => {
      paths.push(path);
      return Promise.resolve(new ArrayBuffer(1));
    },
  });
  const ws = s.live();
  // Документ, на который подписки нет.
  ws.receive({ t: 'snapshot-needed', doc: '1@1', url: '/api/files/f/docs/1@1/snapshot' });
  await tick();
  expect(paths).toEqual([]);
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
  s.clock.advance(1000);
  s.reset();
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
  ws.receive({ t: 'snapshot-needed', doc: 'root', url: '/api/files/other/docs/root/snapshot' });
  await tick();
  expect(paths).toEqual(['/api/files/f/docs/root/snapshot']);
});

test('снимок: 404 при живой роли — отказ с причиной, а не вечный повтор (T8-I2)', async () => {
  let calls = 0;
  const s = stand({
    snapshot: () => {
      calls++;
      return Promise.reject(Object.assign(new Error('нет'), { code: 'NOT_FOUND' }));
    },
  });
  const ws = s.live();
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
  s.clock.advance(1000);
  s.reset();
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
  ws.receive({ t: 'snapshot-needed', doc: 'root', url: '/api/files/f/docs/root/snapshot' });
  for (let i = 0; i < 20; i++) {
    await tick();
    await tick();
    s.clock.advance(30_000);
  }
  expect(s.status()).toBe('rejected');
  expect(s.statuses.at(-1)?.detail).toContain('Снимок');
  expect(calls).toBeLessThan(5);
});

test('снимок: сеть не отдаёт его — после нескольких попыток обычный путь переподключения (T8-I2)', async () => {
  let calls = 0;
  const s = stand({
    snapshot: () => {
      calls++;
      return Promise.reject(Object.assign(new Error('сеть'), { code: 'NETWORK' }));
    },
  });
  const ws = s.live();
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
  s.clock.advance(1000);
  s.reset();
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
  ws.receive({ t: 'snapshot-needed', doc: 'root', url: '/api/files/f/docs/root/snapshot' });
  for (let i = 0; i < 10 && ws.closedCode === null; i++) {
    await tick();
    await tick();
    // Сокет жив и отвечает: рвёт его не проверка связи.
    ws.receive({ t: 'pong' });
    s.clock.advance(10_000);
  }
  expect(ws.closedCode).not.toBeNull();
  expect(calls).toBeLessThanOrEqual(4);
  expect(s.status()).not.toBe('rejected');
});

test('снимок качается долго, соединение рвётся: одна загрузка, не отказ (T8-I6)', async () => {
  let started = 0;
  let finish!: () => void;
  const s = stand({
    snapshot: () => {
      started++;
      return new Promise((resolve) => (finish = () => resolve(new ArrayBuffer(4))));
    },
  });
  let ws = s.live();
  for (let i = 0; i < 4; i++) {
    // Каждое новое соединение подписывается прежней версией, сервер снова просит снимок.
    ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
    s.clock.advance(1000);
    s.reset();
    s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
    ws.receive({ t: 'snapshot-needed', doc: 'root', url: '/api/files/f/docs/root/snapshot' });
    await tick();
    expect(s.status()).toBe('connecting');
    // Пока снимка нет, правки ждут: документ без подписки дал бы `GAP` на каждую.
    expect(s.flushing()).toBe(false);
    ws.dropped();
    ws = nextSocket(s);
    ws.open();
    s.reset();
  }
  expect(started).toBe(1);
  expect(s.status()).not.toBe('rejected');
  finish();
  await tick();
  await tick();
  expect(s.commands.filter((c) => c.type === 'applyRemote')).toHaveLength(1);
});

test('`snapshot-needed` посреди догоняния: документ не считается дважды (сервер, п. 5)', async () => {
  let finish!: () => void;
  const s = stand({ snapshot: () => new Promise((resolve) => (finish = () => resolve(new ArrayBuffer(4)))) });
  s.client.start();
  const ws = s.last();
  ws.open();
  s.reset();
  const v = encodeVersion(new Map());
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: v });
  s.client.onCore({ type: 'syncSubscribe', doc: '1@1', version: v });
  ws.receive({ t: 'subscribed', doc: 'root', seq: 0, version: v });
  ws.receive({ t: 'updates', doc: '1@1', seq: 1, bytes: new Uint8Array([1]) });
  ws.receive({ t: 'snapshot-needed', doc: '1@1', url: '/api/files/f/docs/1@1/snapshot' });
  s.client.onApplied();
  await tick();
  // Снимок ещё качается: не «синхронизировано».
  expect(s.status()).toBe('connecting');
  // Устаревший ответ на ту же подписку ничего не меняет.
  ws.receive({ t: 'subscribed', doc: '1@1', seq: 1, version: v });
  expect(s.status()).toBe('connecting');
  finish();
  await tick();
  await tick();
  s.client.onApplied();
  expect(resets(s)).toBe(2);
});

test('`FORBIDDEN` пачкам в пути: один вопрос о роли, отправка выключена до ответа (T8-M4)', async () => {
  let asked = 0;
  let answer!: (a: AccessState) => void;
  const s = stand({
    access: () => {
      asked++;
      return new Promise((resolve) => (answer = resolve));
    },
  });
  const ws = s.live();
  for (let seq = 1; seq <= 5; seq++) ws.receive({ t: 'refused', doc: 'root', clientSeq: seq, code: 'FORBIDDEN' });
  expect(asked).toBe(1);
  expect(s.flushing()).toBe(false);
  answer({ kind: 'role', role: 'viewer', version: 2, canShare: false, own: true });
  await tick();
  expect(s.access.at(-1)).toMatchObject({ role: 'viewer' });
  expect(s.flushing()).toBe(true);
});

test('остановка снимает все ожидания: повтор картинки и пауза снимка разом (T8-M2)', async () => {
  const s = stand({
    uploadImage: () => Promise.reject(Object.assign(new Error('сеть'), { code: 'NETWORK' })),
    snapshot: () => Promise.reject(Object.assign(new Error('сеть'), { code: 'NETWORK' })),
  });
  const ws = s.live();
  s.client.onCore({ type: 'localImage', hash: 'h', bytes: new Uint8Array(1) });
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'UNAVAILABLE' });
  s.clock.advance(1000);
  s.reset();
  s.client.onCore({ type: 'syncSubscribe', doc: 'root', version: encodeVersion(new Map()) });
  ws.receive({ t: 'snapshot-needed', doc: 'root', url: '/api/files/f/docs/root/snapshot' });
  await tick();
  await tick();
  s.client.stop();
  expect(s.clock.timers.size).toBe(0);
});

const subscribesSent = (ws: FakeSocket) => {
  ws.drain();
  return ws.sent.filter((f) => f.t === 'subscribe').map((f) => (f as { doc: string }).doc);
};

test('отказ: новые чужие страницы подписываются, свои правки не уходят (финальное ревью, I1)', async () => {
  const s = stand();
  const ws = s.live();
  s.client.onCore(update(1));
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'BAD_UPDATE' });
  expect(s.status()).toBe('rejected');
  // Подписки отдаёт тот же шаг ядра, что и правки: он не выключается.
  expect(s.flushing()).toBe(true);
  s.client.onCore({ type: 'syncSubscribe', doc: '2@1', version: encodeVersion(new Map()) });
  expect(subscribesSent(ws)).toContain('2@1');
  s.client.onCore(update(2));
  await tick();
  expect(updatesSent(ws)).toEqual([1]);
  ws.receive({ t: 'updates', doc: '2@1', seq: 1, bytes: new Uint8Array([1]) });
  expect(s.commands.at(-1)).toMatchObject({ type: 'applyRemote', doc: '2@1' });
  expect(s.status()).toBe('rejected');
});

test('отказ и переподключение: подписки есть, чужие правки идут, свои — нет (финальное ревью, I1)', async () => {
  const s = stand();
  const ws = s.live();
  s.client.onCore(update(1));
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'BAD_UPDATE' });
  ws.dropped(1006);
  const ws2 = nextSocket(s);
  ws2.open();
  s.reset();
  expect(s.flushing()).toBe(true);
  s.subscribe(ws2, ['root', '1@1']);
  expect(subscribesSent(ws2)).toEqual(['root', '1@1']);
  // Ядро шлёт всё сверх версии сервера — отказ окончателен до перезагрузки, пачки не уходят.
  s.client.onCore(update(2));
  await tick();
  expect(updatesSent(ws2)).toEqual([]);
  expect(s.status()).toBe('rejected');
});

test('`GAP` после своего `UNAVAILABLE` в том же поколении — не разрыв (повторное ревью клиента, N1)', async () => {
  const s = stand();
  const ws = s.live();
  let seq = 0;
  // Запись базы падает, чтение живо: подписка проходит, первая пачка — «повторите», следующие в пути — разрыв.
  for (let gen = 0; gen < 10; gen++) {
    s.client.onCore(update(++seq));
    s.client.onCore(update(++seq));
    s.client.onCore(update(++seq));
    await tick();
    ws.receive({ t: 'refused', doc: 'root', clientSeq: seq - 2, code: 'UNAVAILABLE' });
    ws.receive({ t: 'refused', doc: 'root', clientSeq: seq - 1, code: 'GAP' });
    ws.receive({ t: 'refused', doc: 'root', clientSeq: seq, code: 'GAP' });
    s.clock.advance(RECONNECT_MAX_MS);
    s.reset();
    s.subscribe(ws);
  }
  expect(s.status()).not.toBe('rejected');
});

test('`FORBIDDEN`, а роль та же: неподтверждённое уходит заново без новой правки (финальное ревью, M1)', async () => {
  const s = stand();
  const ws = s.live();
  const before = resets(s);
  ws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'FORBIDDEN' });
  await tick();
  await tick();
  s.clock.advance(RECONNECT_MAX_MS);
  expect(resets(s)).toBe(before + 1);
  s.reset();
  s.subscribe(ws);
  expect(s.flushing()).toBe(true);
  // Роль сменилась на просмотр: подписываться заново незачем, правки не уйдут и так.
  const v = stand({ access: () => Promise.resolve({ kind: 'role', role: 'viewer', version: 2, canShare: false, own: true }) });
  const vws = v.live();
  const vbefore = resets(v);
  vws.receive({ t: 'refused', doc: 'root', clientSeq: 1, code: 'FORBIDDEN' });
  await tick();
  await tick();
  v.clock.advance(RECONNECT_MAX_MS);
  expect(resets(v)).toBe(vbefore);
});

test('сокет не открывается: срок на рукопожатие, потом обычное переподключение (T8-M5)', () => {
  const s = stand();
  s.client.start();
  const first = s.last();
  s.clock.advance(PONG_WAIT_MS - 1);
  expect(first.closedCode).toBeNull();
  s.clock.advance(1);
  expect(first.closedCode).not.toBeNull();
  expect(s.status()).toBe('offline');
  nextSocket(s).open();
  // Открылся вовремя — срок снят.
  s.clock.advance(PONG_WAIT_MS);
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

test('присутствие: устойчиво не чаще 25 кадров в секунду при непрерывном движении, повтора между ними нет', () => {
  const s = stand();
  const ws = s.live();
  role(ws);
  const perSecond: number[] = [];
  let x = 0;
  let total = 0;
  for (let sec = 0; sec < 4; sec++) {
    // Кадр ядра каждые 10 мс — 100 в секунду.
    for (let t = 0; t < 100; t++) {
      s.client.onCore(own(x++));
      s.clock.advance(10);
    }
    ws.drain();
    const n = presenceFrames(ws).length;
    perSecond.push(n - total);
    total = n;
  }
  // Первая секунда — запас и частота, дальше — только частота.
  expect(perSecond[0]).toBeLessThanOrEqual(PRESENCE_BURST + PRESENCE_RATE + 1);
  for (const n of perSecond.slice(1)) {
    expect(n).toBeGreaterThanOrEqual(PRESENCE_RATE - 1);
    expect(n).toBeLessThanOrEqual(PRESENCE_RATE + 1);
  }
});

test('присутствие: снятое выделение снимает и неотправленные части прежнего', () => {
  const s = stand();
  const ws = s.live();
  role(ws);
  // Запас кончился: следующее ждёт в очереди.
  for (let i = 0; i < PRESENCE_BURST; i++) s.client.onCore(own(i));
  ws.drain();
  const before = presenceFrames(ws).length;
  const parts = Array.from({ length: 5 }, (_, i) => encodeSelectionIds([`${i + 1}@2`]));
  s.client.onCore(own(50, { selection: { rev: 4, count: 5, parts: 5, bounds: [0, 0, 1, 1] }, parts }));
  s.client.onCore(own(51));
  s.clock.advance(1000);
  ws.drain();
  const after = presenceFrames(ws).slice(before);
  expect(after.map((f) => f.t)).toEqual(['presence']);
  expect(after[0].t === 'presence' && after[0].state?.cursor?.[0]).toBe(51);
});

test('присутствие: буфер сокета занят — кадр в него не ложится, а сливается в последнее и уходит, когда буфер освободится', () => {
  const s = stand();
  const ws = s.live();
  role(ws);
  ws.drain();
  ws.queued.push(encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 1, bytes: new Uint8Array(PRESENCE_HIGH_WATER + 1) }));
  for (let i = 0; i < 5; i++) s.client.onCore(own(i));
  s.clock.advance(1000);
  expect(ws.queued).toHaveLength(1);
  ws.drain();
  s.clock.advance(100);
  ws.drain();
  const sent = presenceFrames(ws);
  expect(sent).toHaveLength(1);
  expect(sent[0].t === 'presence' && sent[0].state?.cursor?.[0]).toBe(4);
});

test('присутствие: после `access-revoked` при открытом сокете ничего не уходит, участники и таймеры сброшены', () => {
  const s = stand();
  const ws = s.live();
  role(ws);
  ws.receiveRaw(encodePresenceBatch([encodePresenceRecord({ k: 'join', participant: 7, user: 'u-7', name: 'Анна', color: 3 })]));
  s.client.onCore(own(1));
  ws.drain();
  const sent = presenceFrames(ws).length;
  ws.receive({ t: 'access-revoked', reason: 'access' });
  expect(s.commands.at(-1)).toEqual({ type: 'presenceReset' });
  expect(s.participants.at(-1)).toEqual([]);
  // Сервер не закрыл сокет сразу: ни кадра от ядра, ни повтора, ни ответа на кадры сервера.
  s.client.onCore(own(2));
  role(ws);
  ws.receiveRaw(encodePresenceBatch([encodePresenceRecord({ k: 'join', participant: 8, user: 'u-8', name: 'Борис', color: 1 })]));
  s.clock.advance(PRESENCE_HEARTBEAT_MS * 3);
  ws.drain();
  expect(presenceFrames(ws)).toHaveLength(sent);
  expect(ws.sent.filter((f) => f.t === 'ping')).toEqual([]);
  expect(s.participants.at(-1)).toEqual([]);
  expect(s.clock.timers.size).toBe(0);
});
