import { afterAll, beforeEach, expect, test } from 'bun:test';
import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import { MAX_CONTENT_BYTES, MAX_IMAGE_REFS } from '@ferrum/protocol';
import { LIMITS, MAX_JSON_BYTES } from '../src/api';
import { MAX_UPLOADS, MAX_UPLOADS_PER_USER, uploadGate } from '../src/file-routes';
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

test('заголовки записи обязательны', async () => {
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
  expect(await json(await s.call(other, 'GET', `/api/files/${f.id}/access`))).toEqual({ role: 'viewer', version: 1, canShare: false });
  await s.pool.query(`UPDATE file_access SET role = 'editor' WHERE file_id = $1 AND user_id = $2`, [f.id, other.id]);
  expect(await json(await s.call(other, 'GET', `/api/files/${f.id}/access`))).toEqual({ role: 'editor', version: 1, canShare: true });
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
  const MB = 200;
  const child = Bun.spawn([process.execPath, 'run', join(import.meta.dir, 'memory-probe.ts'), String(MB)], {
    stdout: 'pipe',
    stderr: 'inherit',
  });
  const out = await new Response(child.stdout).text();
  expect(await child.exited).toBe(0);
  const r = JSON.parse(out.trim().split('\n').at(-1)!) as { status: number; read: number; growthMb: number };
  expect(r.status).toBe(200);
  expect(r.read).toBe(MB * 1024 * 1024 + 16 + 5 + 40);
  // Клиент и сервер здесь в одном процессе: потоком прирост около 110 МБ, сборка тела в памяти — больше 500.
  expect(r.growthMb).toBeLessThan(MB);
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

test('предел частоты по человеку: другой не страдает', async () => {
  expect(LIMITS.read.max).toBeGreaterThan(100);
  const tight = await apiStand({ limits: { read: { max: 2, windowSeconds: 60 } } }, true);
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
  const slow = await apiStand({ uploadIdleMs: 50 }, true);
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

test('загрузка медленнее наименьшей скорости освобождает слот, хотя простоя нет', async () => {
  const slow = await apiStand({ uploadPace: { graceMs: 30, bytesPerSecond: 1_000_000 } }, true);
  const who = await slow.signIn('drip@example.com');
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

test('картинок в файле больше предела: 400 в формате ошибок, файл не записан', async () => {
  const f = await s.newFile(owner);
  const hashes = Array.from({ length: MAX_IMAGE_REFS + 1 }, (_, i) => i.toString(16).padStart(64, '0'));
  const res = await s.writeContent(owner, f.id, 0, container(hashes));
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'TOO_MANY_IMAGES' });
  expect(readdirSync(join(s.dataDir, 'files', f.id))).toEqual([]);
  expect(await json(await s.call(owner, 'GET', `/api/files/${f.id}/access`))).toMatchObject({ version: 0 });
});

test('содержимое и превью не читаются с чужого сайта: `Cross-Origin-Resource-Policy`', async () => {
  const f = await s.newFile(owner);
  await s.writeContent(owner, f.id, 0);
  await s.call(owner, 'PUT', `/api/files/${f.id}/thumb`, PNG);
  for (const path of ['content', 'thumb']) {
    const res = await s.call(owner, 'GET', `/api/files/${f.id}/${path}`);
    expect(res.status).toBe(200);
    expect(res.headers.get('cross-origin-resource-policy')).toBe('same-origin');
  }
});

test('вкладка работает за другого человека: 401 и ни запись, ни чтение не проходят', async () => {
  const f = await s.newFile(owner);
  const as = (who: Person) => ({ 'x-ferrum-user': who.id });
  expect((await s.call(owner, 'GET', `/api/files/${f.id}/access`, undefined, as(owner))).status).toBe(200);
  const read = await s.call(owner, 'GET', `/api/files/${f.id}/access`, undefined, as(other));
  expect(read.status).toBe(401);
  expect(await read.json()).toEqual({ error: 'UNAUTHORIZED' });
  expect((await s.writeContent(owner, f.id, 0, container(), as(other))).status).toBe(401);
  expect(await json(await s.call(owner, 'GET', `/api/files/${f.id}/access`))).toMatchObject({ version: 0 });
  // Без заголовка — как раньше: запросы основного потока идут от того, чья cookie.
  expect((await s.writeContent(owner, f.id, 0)).status).toBe(200);
});

test('сверх предела частоты: ответ говорит, сколько ждать', async () => {
  const tight = await apiStand({ limits: { read: { max: 1, windowSeconds: 60 } } }, true);
  const a = await tight.signIn('wait@example.com');
  const path = '/api/files?section=drafts&start=0&count=1';
  expect((await tight.call(a, 'GET', path)).status).toBe(200);
  const limited = await tight.call(a, 'GET', path);
  expect(limited.status).toBe(429);
  const wait = Number(limited.headers.get('retry-after'));
  expect(wait).toBeGreaterThan(0);
  expect(wait).toBeLessThanOrEqual(60);
  await tight.stop();
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

const THUMB = (id: string) => `/api/files/${id}/thumb`;
const pendingKeys = async () => (await s.pool.query<{ key: string }>('SELECT key FROM blob_pending')).rows.map((r) => r.key);

/** Запрос без тела: ответ пришёл, значит сервер отказал, не дожидаясь байтов. */
function declaredOnly(stand: typeof s, who: Person, path: string, bytes: number, headers: string[]): Promise<string> {
  const { promise, resolve, reject } = Promise.withResolvers<string>();
  let text = '';
  const head = [
    `PUT ${path} HTTP/1.1`,
    `Host: localhost:${stand.server.port}`,
    `Origin: ${stand.publicUrl}`,
    `Cookie: ${who.cookie}`,
    `Content-Length: ${bytes}`,
    ...headers,
  ];
  void Bun.connect({
    hostname: 'localhost',
    port: stand.server.port!,
    socket: {
      open: (sock) => void sock.write(`${head.join('\r\n')}\r\n\r\n`),
      data: (sock, d) => {
        text += new TextDecoder().decode(d);
        if (/\{"error":"\w+"\}/.test(text)) {
          resolve(text);
          sock.end();
        }
      },
      close: () => resolve(text),
      error: (_, e) => reject(e),
    },
  });
  return promise;
}

test('сверх настоящего предела по `Content-Length` — 413, тело не читается', async () => {
  // Срок простоя короткий: если сервер станет ждать тело, ответ будет другим и сразу.
  const stand = await apiStand({ uploadIdleMs: 200 }, true);
  const f = await s.newFile(owner);
  const write = ['If-Match: "0"', `X-Ferrum-Write: ${crypto.randomUUID()}`];
  const reply = await declaredOnly(stand, owner, `/api/files/${f.id}/content`, MAX_CONTENT_BYTES + 1, write);
  expect(reply.split(' ')[1]).toBe('413');
  expect(reply).toContain('{"error":"TOO_LARGE"}');
  await stand.stop();
});

test('сверх предела потоком и по `Content-Length`: 413, байтов и намерения нет, старое цело', async () => {
  const stand = await apiStand({ maxContentBytes: 64 }, true);
  const f = await s.newFile(owner);
  const small = container([], 1);
  expect(
    (await stand.call(owner, 'PUT', `/api/files/${f.id}/content`, small, { 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() }))
      .status,
  ).toBe(200);
  const big = container([], 1000);
  const write = () => ({ 'if-match': '"1"', 'x-ferrum-write': crypto.randomUUID() });
  const declared = await stand.call(owner, 'PUT', `/api/files/${f.id}/content`, big, write());
  expect(declared.status).toBe(413);
  const { done, release } = stand.gatedUpload(owner, `/api/files/${f.id}/content`, big, write());
  release();
  const streamed = await done;
  expect(streamed.status).toBe(413);
  expect(await streamed.json()).toEqual({ error: 'TOO_LARGE' });
  expect(await pendingKeys()).toEqual([]);
  expect(new Uint8Array(await (await stand.call(owner, 'GET', `/api/files/${f.id}/content`)).arrayBuffer())).toEqual(small);
  await stand.stop();
});

test('срок обработчика: зависший запрос к базе — 503 в формате ошибок', async () => {
  const f = await s.newFile(owner);
  const tight = await apiStand({ apiDeadlineMs: 100 }, true);
  const lock = await s.pool.connect();
  await lock.query('BEGIN');
  await lock.query('SELECT 1 FROM file WHERE id = $1 FOR UPDATE', [f.id]);
  const res = await tight.call(owner, 'PATCH', `/api/files/${f.id}`, { name: 'Новое' });
  expect(res.status).toBe(503);
  expect(await res.json()).toEqual({ error: 'UNAVAILABLE' });
  await lock.query('ROLLBACK');
  lock.release();
  await tight.stop();
});

test('срок проверки прав: зависшая база до обработчика — 503 в формате ошибок', async () => {
  const f = await s.newFile(owner);
  const tight = await apiStand({ apiDeadlineMs: 100 }, true);
  const lock = await s.pool.connect();
  await lock.query('BEGIN');
  await lock.query('LOCK TABLE file IN ACCESS EXCLUSIVE MODE');
  const res = await tight.call(owner, 'GET', `/api/files/${f.id}`);
  expect(res.status).toBe(503);
  expect(await res.json()).toEqual({ error: 'UNAVAILABLE' });
  await lock.query('ROLLBACK');
  lock.release();
  await tight.stop();
});

test('сбой транзакции после загрузки: 503, намерение остаётся, очистка убирает байты', async () => {
  const f = await s.newFile(owner);
  const res = await s.failing('file', 'UPDATE', () => s.writeContent(owner, f.id, 0));
  expect(res.status).toBe(503);
  expect(readdirSync(join(s.dataDir, 'files', f.id))).toHaveLength(1);
  expect(await pendingKeys()).toHaveLength(1);
  // Загрузка ещё может быть жива: до срока очистка её не трогает.
  await runGc(s.pool, s.blobs);
  expect(readdirSync(join(s.dataDir, 'files', f.id))).toHaveLength(1);
  await s.expireUploads();
  await runGc(s.pool, s.blobs);
  expect(readdirSync(join(s.dataDir, 'files', f.id))).toEqual([]);
  expect(await pendingKeys()).toEqual([]);
  expect((await s.pool.query('SELECT 1 FROM blob_trash')).rowCount).toBe(0);
});

test('сбой транзакции превью: байты тоже убирает очистка', async () => {
  const f = await s.newFile(owner);
  const res = await s.failing('file', 'UPDATE', () => s.call(owner, 'PUT', THUMB(f.id), PNG));
  expect(res.status).toBe(503);
  expect(fileCount(f.id)).toBe(1);
  await s.expireUploads();
  await runGc(s.pool, s.blobs);
  expect(fileCount(f.id)).toBe(0);
});

test('зафиксированная запись не считается брошенной: очистка по сроку не трогает живые байты', async () => {
  const f = await s.newFile(owner);
  const bytes = container([], 8);
  await s.writeContent(owner, f.id, 0, bytes);
  await s.call(owner, 'PUT', THUMB(f.id), PNG);
  expect(await pendingKeys()).toEqual([]);
  await s.expireUploads();
  await runGc(s.pool, s.blobs);
  expect(new Uint8Array(await (await s.call(owner, 'GET', `/api/files/${f.id}/content`)).arrayBuffer())).toEqual(bytes);
  expect(new Uint8Array(await (await s.call(owner, 'GET', THUMB(f.id))).arrayBuffer())).toEqual(PNG);
});

test('загрузка дольше срока намерения: отказ при фиксации, в базе нет ключа без байтов', async () => {
  const f = await s.newFile(owner);
  const write = { 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() };
  const { done, release } = s.gatedUpload(owner, `/api/files/${f.id}/content`, container([], 64), write);
  await s.waitTmp(1);
  await s.expireUploads();
  await runGc(s.pool, s.blobs);
  release();
  const res = await done;
  expect(res.status).toBe(503);
  expect(await res.json()).toEqual({ error: 'UNAVAILABLE' });
  const meta = await s.pool.query('SELECT version, content_key FROM file WHERE id = $1', [f.id]);
  expect(meta.rows[0]).toEqual({ version: '0', content_key: null });
  expect(fileCount(f.id)).toBe(0);
});

test('превью: роль снята посреди загрузки — 403, превью прежнее, байты убраны', async () => {
  const f = await s.newFile(owner);
  await giveAccess(f.id, other, 'editor');
  const { done, release } = s.gatedUpload(other, THUMB(f.id), PNG);
  await s.waitTmp(1);
  await s.pool.query(`UPDATE file_access SET role = 'viewer' WHERE file_id = $1 AND user_id = $2`, [f.id, other.id]);
  release();
  const res = await done;
  expect(res.status).toBe(403);
  expect(await res.json()).toEqual({ error: 'FORBIDDEN' });
  expect((await json(await s.call(owner, 'GET', `/api/files/${f.id}`))).thumbVersion).toBe(0);
  expect(fileCount(f.id)).toBe(0);
  expect(await pendingKeys()).toEqual([]);
});

test('превью: файл удалён посреди загрузки — 404, байтов нет; убран в корзину — редактору 404', async () => {
  const gone = await s.newFile(owner);
  const a = s.gatedUpload(owner, THUMB(gone.id), PNG);
  await s.waitTmp(1);
  expect((await s.call(owner, 'DELETE', `/api/files/${gone.id}`)).status).toBe(200);
  a.release();
  const res = await a.done;
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({ error: 'NOT_FOUND' });
  expect(fileCount(gone.id)).toBe(0);
  expect(await pendingKeys()).toEqual([]);

  const f = await s.newFile(owner);
  await giveAccess(f.id, other, 'editor');
  const b = s.gatedUpload(other, THUMB(f.id), PNG);
  await s.waitTmp(1);
  await s.call(owner, 'POST', `/api/files/${f.id}/trash`);
  b.release();
  expect((await b.done).status).toBe(404);
  expect((await s.pool.query('SELECT thumb_key FROM file WHERE id = $1', [f.id])).rows[0].thumb_key).toBeNull();
});

test('файл в корзине без роли владельца: не в списке, счётчик честный, очистка корзины его не удаляет', async () => {
  const f = await s.newFile(other, 'Чужое');
  await s.call(other, 'POST', `/api/files/${f.id}/trash`);
  // Владелец по записи файла без действующей роли: ранг 0 не показывается и не даёт удалить.
  await s.pool.query('DELETE FROM file_access WHERE file_id = $1', [f.id]);
  await s.pool.query('UPDATE file SET trashed_by = $2 WHERE id = $1', [f.id, owner.id]);
  const trash = await list(other, 'trash');
  expect([trash.total, trash.rows]).toEqual([0, []]);
  expect((await s.call(other, 'POST', '/api/files/empty-trash')).status).toBe(200);
  expect((await s.pool.query('SELECT 1 FROM file WHERE id = $1', [f.id])).rowCount).toBe(1);
});

test('предел одновременных загрузок: девятая — 429 `BUSY`, у другого своё место, освободившееся снова доступно', async () => {
  const f = await s.newFile(owner);
  const running = Array.from({ length: 8 }, () => s.gatedUpload(owner, THUMB(f.id), PNG));
  await s.waitTmp(8);
  const write = { 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() };
  for (const refused of [
    await s.call(owner, 'PUT', THUMB(f.id), PNG),
    await s.call(owner, 'PUT', `/api/files/${f.id}/content`, container(), write),
    await s.call(owner, 'PUT', `/api/files/${f.id}/images/${'a'.repeat(64)}`, new Uint8Array(1)),
  ]) {
    expect(refused.status).toBe(429);
    expect(await refused.json()).toEqual({ error: 'BUSY' });
  }
  const mine = await s.newFile(other);
  expect((await s.call(other, 'PUT', THUMB(mine.id), PNG)).status).toBe(200);
  running[0].release();
  expect((await running[0].done).status).toBe(200);
  expect((await s.call(owner, 'PUT', THUMB(f.id), PNG)).status).toBe(200);
  for (const u of running.slice(1)) u.release();
  for (const u of running.slice(1)) expect((await u.done).status).toBe(200);
});

test('предел загрузок на сервер: общий счётчик', () => {
  const gate = uploadGate();
  let taken = 0;
  // Людей больше, чем нужно общему пределу при их собственном.
  const people = Math.ceil(MAX_UPLOADS / MAX_UPLOADS_PER_USER) + 1;
  for (let p = 0; p < people; p++) for (let i = 0; i < MAX_UPLOADS_PER_USER; i++) if (gate.enter(`u${p}`)) taken++;
  expect(taken).toBe(MAX_UPLOADS);
  expect(gate.enter('новый')).toBe(false);
  gate.leave('u0');
  expect(gate.enter('новый')).toBe(true);
});
