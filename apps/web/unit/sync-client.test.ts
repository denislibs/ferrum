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
