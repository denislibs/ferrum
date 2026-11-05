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
  signal?: AbortSignal;
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
    const call: Call = {
      url,
      method,
      headers: (init.headers ?? {}) as Record<string, string>,
      body: init.body,
      signal: init.signal ?? undefined,
    };
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
  // Журнал не применён, но сеанс известен: новые пачки читателя идут после прежних.
  expect([viewer.journal, viewer.session, viewer.journalHeld]).toEqual([[], 7, true]);
  const editor = await s.files.read('e');
  expect([editor.journal.length, editor.session, editor.journalHeld]).toEqual([1, 7, false]);
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
  expect(s.events.access).toEqual([['f', { kind: 'role', role: 'editor', version: 1, canShare: false, own: false }]]);
  reply = () => {
    throw new TypeError('сеть');
  };
  await s.files.checkAccess('f');
  expect(s.events.access).toHaveLength(1);
  reply = () => access(1, 'viewer');
  await s.files.checkAccess('f');
  expect(s.events.access.at(-1)).toEqual(['f', { kind: 'role', role: 'viewer', version: 1, canShare: false, own: false }]);
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

/** Ответ, которого тест ждёт сам: `open` — отпустить; сигнал запроса его обрывает. */
function held() {
  let open!: (r: Response) => void;
  const reply = new Promise<Response>((resolve) => (open = resolve));
  return { reply, open };
}
const untilAborted = (c: Call) =>
  new Promise<Response>((_, reject) => {
    c.signal?.addEventListener('abort', () => reject(new DOMException('срок', 'AbortError')));
  });
const flush = async () => {
  for (let i = 0; i < 20; i++) await Promise.resolve();
};

test('опоздавший ответ опроса: старый не отменяет новый, снятое наблюдение не оживает (T6-I1, T6-Minor5)', async () => {
  const first = held();
  const second = held();
  const replies = [first, second];
  const s = stand({ 'GET /api/files/f/access': () => replies.shift()!.reply });
  const a = s.files.checkAccess('f');
  const b = s.files.checkAccess('f');
  second.open(access(1, 'viewer'));
  await b;
  first.open(access(1, 'editor'));
  await a;
  // Старый ответ вернул бы прежнюю роль на целый период опроса.
  expect(s.events.access.map(([, e]) => (e.kind === 'role' ? e.role : e.kind))).toEqual(['viewer']);

  const late = held();
  replies.push(late);
  const c = s.files.checkAccess('f');
  s.files.forgetAccess('f');
  late.open(access(1, 'viewer'));
  await c;
  // Опоздавший ответ снятого наблюдения не запоминается, и новое наблюдение получит событие.
  expect(s.events.access).toHaveLength(1);
  replies.push({ reply: Promise.resolve(access(1, 'viewer')), open: () => {} });
  await s.files.checkAccess('f');
  expect(s.events.access).toHaveLength(2);
});

test('новое наблюдение получает состояние, даже если отказ записи уже запомнил такое же (T6-I1)', async () => {
  const s = stand({ 'GET /api/files/f/access': () => access(1, 'viewer') });
  await s.files.checkAccess('f');
  s.files.forgetAccess('f');
  // Наблюдение снято, а запись, которая шла, ещё сообщила о доступе: это не должно подавить следующее.
  await s.files.checkAccess('f');
  expect(s.events.access).toHaveLength(2);
  await s.files.watchAccess('f');
  expect(s.events.access).toHaveLength(3);
});

test('`canShare` из ответа доступа; нет поля — нет права (T7-I1)', async () => {
  const s = stand({ 'GET /api/files/f/access': json({ role: 'editor', version: 1, canShare: true }) });
  await s.files.checkAccess('f');
  expect(s.events.access[0][1]).toMatchObject({ canShare: true });
  const t = stand({ 'GET /api/files/f/access': json({ role: 'editor', version: 1 }) });
  await t.files.checkAccess('f');
  expect(t.events.access[0][1]).toMatchObject({ canShare: false });
});

test('поколение, которое может быть своим: идущая запись и потерянный ответ (T6-Minor5)', async () => {
  const put = held();
  let version = 1;
  const s = stand({
    'GET /api/files/f/access': () => access(version),
    'PUT /api/files/f/content': () => put.reply,
  });
  const write = s.files.write('f', container(ROOT), 'keep', 1, null);
  await flush();
  await expect.poll(() => s.calls.some((c) => c.method === 'PUT')).toBe(true);
  version = 2;
  await s.files.checkAccess('f');
  expect(s.events.access.at(-1)![1]).toMatchObject({ version: 2, own: true });
  put.open(json(meta({ version: 2 })));
  await write;
  await s.files.checkAccess('f');
  expect(s.events.access.at(-1)![1]).toMatchObject({ version: 2, own: false });

  const lost = stand({
    'GET /api/files/f/access': () => access(version),
    'PUT /api/files/f/content': () => {
      throw new TypeError('обрыв');
    },
  });
  version = 1;
  expect(await code(lost.files.write('f', container(ROOT), 'keep', 1, null))).toBe('NETWORK');
  version = 2;
  await lost.files.checkAccess('f');
  expect(lost.events.access.at(-1)![1]).toMatchObject({ version: 2, own: true });
});

test('два сбоя подряд: дошедшая запись не становится ложным `CONFLICT` (T6-Minor1)', async () => {
  const puts: Call[] = [];
  let version = 1;
  const s = stand({
    'GET /api/files/f/access': () => access(version),
    'PUT /api/files/f/content': (c) => {
      puts.push(c);
      // A дошла, ответ потерян. B не дошла. C — как решил бы сервер: номер должен быть у записи A.
      if (puts.length === 1) version = 2;
      if (puts.length < 3) throw new TypeError('обрыв');
      return json(meta({ version: 3 }));
    },
  });
  await code(s.files.write('f', container(ROOT), 'keep', 1, null));
  await code(s.files.write('f', container(ROOT), 'keep', 1, null));
  await s.files.write('f', container(ROOT), 'keep', 1, null);
  expect(puts[2].headers['x-ferrum-previous-write']).toBe('write-1');
  expect(puts[2].headers['If-Match']).toBe('"1"');

  // Дошли обе: поколение ушло на две, следующая идёт поверх второй.
  const both: Call[] = [];
  version = 1;
  const t = stand({
    'GET /api/files/f/access': () => access(version),
    'PUT /api/files/f/content': (c) => {
      both.push(c);
      if (both.length < 3) {
        version++;
        throw new TypeError('обрыв');
      }
      return json(meta({ version: 4 }));
    },
  });
  await code(t.files.write('f', container(ROOT), 'keep', 1, null));
  await code(t.files.write('f', container(ROOT), 'keep', 1, null));
  expect((await t.files.write('f', container(ROOT), 'keep', 1, null)).version).toBe(4);
  expect(both[2].headers['x-ferrum-previous-write']).toBe('write-2');
  expect(both[2].headers['If-Match']).toBe('"2"');
});

test('сбой снятия журнала после удачной записи — предупреждение, а не отказ (T6-Minor4)', async () => {
  const s = stand({
    'GET /api/files/f/access': access(1),
    'PUT /api/files/f/content': json(meta({ version: 2 })),
  });
  s.journal.cut = async () => {
    throw new Error('нет места');
  };
  const m = await s.files.write('f', container(ROOT), 'keep', 1, { session: 1, upTo: 1 });
  expect(m.version).toBe(2);
  expect(m.warning).toBe('Файл записан, но журнал изменений не очищен');
});

test('журнал, не применённый читателем, запись не стирает; открытие редактором снимает запрет (T6-I2)', async () => {
  let role = 'viewer';
  const s = stand({
    'GET /api/files/f': () => json(meta({ role })),
    'GET /api/files/f/content': () => new Response(container(ROOT), { headers: { 'x-ferrum-version': '1' } }),
    'GET /api/files/f/access': () => access(1),
    'PUT /api/files/f/content': () => json(meta({ version: 1 })),
  });
  s.journal.entries = [new ArrayBuffer(1)];
  s.journal.session = 7;
  await s.files.read('f');
  role = 'editor';
  await s.files.write('f', container(ROOT), 'keep', 1, { session: 8, upTo: 1 });
  expect(s.journal.cuts).toEqual([]);
  await s.files.read('f');
  await s.files.write('f', container(ROOT), 'keep', 1, { session: 9, upTo: 1 });
  expect(s.journal.cuts).toEqual([['f', 9, 1]]);
});

test('сервер не нашёл картинку: кэш сброшен, байты загружены заново, запись повторена один раз (T6-I3)', async () => {
  const bytes = new Uint8Array([1, 2, 3]);
  let onServer = true;
  const contentPuts: Call[] = [];
  const imagePuts: Call[] = [];
  const url = `/api/files/f/images/${HASH}`;
  const s = stand({
    'GET /api/files/f/access': access(1),
    [`HEAD ${url}`]: () => new Response(null, { status: onServer ? 200 : 404 }),
    [`PUT ${url}`]: (c) => {
      imagePuts.push(c);
      onServer = true;
      return json({ ok: true });
    },
    'PUT /api/files/f/content': (c) => {
      contentPuts.push(c);
      return onServer ? json(meta({ version: 2 })) : json({ error: 'MISSING_IMAGES' }, 409);
    },
  });
  const file = () => container(ROOT, refs(HASH), imageSection(bytes));
  // Первая запись: картинка есть, хэш попадает в кэш.
  await s.files.write('f', file(), 'keep', 1, { session: 1, upTo: 1 });
  expect(imagePuts).toHaveLength(0);
  // Очистка сняла картинку, кэш об этом не знает: HEAD не спрашивается, запись отказывает.
  onServer = false;
  const m = await s.files.write('f', file(), 'keep', 1, { session: 1, upTo: 2 });
  expect(m.version).toBe(2);
  expect(imagePuts).toHaveLength(1);
  expect(contentPuts).toHaveLength(3);
  expect(s.journal.cuts).toHaveLength(2);
  // Сервер снова отказал: отказ, а не цикл; журнал цел.
  const stuck = stand({
    'GET /api/files/f/access': access(1),
    [`HEAD ${url}`]: new Response(null, { status: 200 }),
    'PUT /api/files/f/content': () => json({ error: 'MISSING_IMAGES' }, 409),
  });
  expect(await code(stuck.files.write('f', file(), 'keep', 1, { session: 1, upTo: 1 }))).toBe('MISSING_IMAGES');
  expect(stuck.calls.filter((c) => c.method === 'PUT')).toHaveLength(2);
  expect(stuck.journal.cuts).toEqual([]);
});

test('ссылки без байтов в файле считаются потерянными и объявляются серверу (T6-I3)', async () => {
  const s = stand({
    'GET /api/files/f/access': access(1),
    'PUT /api/files/f/content': json(meta({ version: 2 })),
  });
  await s.files.write('f', container(ROOT, refs(HASH)), 'keep', 1, null);
  await s.files.write('f', container(ROOT), 'keep', 1, null);
  const puts = s.calls.filter((c) => c.method === 'PUT');
  expect(puts[0].headers['x-ferrum-lost-images']).toBe('1');
  expect(puts[1].headers['x-ferrum-lost-images']).toBeUndefined();
});

test('картинка при открытии: застывшие заголовки и тело обрываются по сроку (T6-Minor2)', async () => {
  const file = () => new Response(container(ROOT, refs(HASH)), { headers: { 'x-ferrum-version': '1' } });
  const silent = stand({
    'GET /api/files/f': json(meta()),
    'GET /api/files/f/content': file,
    [`GET /api/files/f/images/${HASH}`]: untilAborted,
  });
  const a = code(silent.files.read('f'));
  await expect.poll(() => silent.calls.some((c) => c.url.includes('/images/'))).toBe(true);
  silent.clock.advance(15_000);
  expect(await a).toBe('NETWORK');

  const ctl = { signal: null as AbortSignal | null };
  const frozen = stand({
    'GET /api/files/f': json(meta()),
    'GET /api/files/f/content': file,
    [`GET /api/files/f/images/${HASH}`]: (c) => {
      ctl.signal = c.signal ?? null;
      const body = new ReadableStream({
        start(out) {
          ctl.signal?.addEventListener('abort', () => out.error(new DOMException('срок', 'AbortError')));
        },
      });
      return new Response(body, { headers: { 'content-length': '1000' } });
    },
  });
  const b = code(frozen.files.read('f'));
  await expect.poll(() => ctl.signal !== null).toBe(true);
  await flush();
  // Срок заголовков снят: на нём тело ещё живёт.
  frozen.clock.advance(15_000);
  expect(ctl.signal!.aborted).toBe(false);
  frozen.clock.advance(transferTimeout(1000));
  expect(await b).toBe('NETWORK');
});

test('откат импорта: `DELETE` не ответил — операция всё равно завершается (T6-Minor3)', async () => {
  const s = stand({
    'POST /api/files': json(meta({ id: 'n', version: 0 })),
    'PUT /api/files/n/content': json({ error: 'BAD_CONTAINER' }, 400),
    'DELETE /api/files/n': untilAborted,
  });
  const r = code(s.files.create('Файл', container(ROOT), null));
  await expect.poll(() => s.calls.some((c) => c.method === 'DELETE')).toBe(true);
  s.clock.advance(15_000);
  expect(await r).toBe('BAD_CONTAINER');
});

test('перенос: сбой журнала откатывает файл целиком (T9-I4)', async () => {
  const s = stand({
    'POST /api/files': json(meta({ id: 'n', version: 0 })),
    'PUT /api/files/n/content': json(meta({ id: 'n', version: 1 })),
    'DELETE /api/files/n': json({ ok: true }),
  });
  s.journal.append = async () => {
    throw new Error('нет места');
  };
  await expect(s.files.create('Файл', container(ROOT), null, [new ArrayBuffer(1)])).rejects.toThrow('нет места');
  expect(s.calls.some((c) => c.method === 'DELETE' && c.url === '/api/files/n')).toBe(true);
  expect(s.journal.dropped).toEqual(['n']);
  expect(s.events.changed).toBe(0);
});
