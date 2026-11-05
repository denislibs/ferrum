// Проверка прав на каждом маршруте (спека, раздел 13.2): таблица перечисляет все маршруты, новый без
// записи здесь роняет тест. У каждого: без сессии — 401; изменяющий с чужим `Origin` — 403; чужой
// неотличим от несуществующего; роли ниже нужной — 403; нужной — работает.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { blake3 } from '@noble/hashes/blake3.js';
import { bytesToHex } from '@noble/hashes/utils.js';
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
const IMAGE: Uint8Array<ArrayBuffer> = new Uint8Array([1, 2, 3]);
const IMAGE_HASH = bytesToHex(blake3(IMAGE));

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
  // Хэш не у этого файла: 404 и у читателя, но это уже после проверки доступа к файлу.
  'HEAD /api/files/:id/images/:hash': { params: file, allowed: viewer, status: 404 },
  'GET /api/files/:id/images/:hash': { params: file, allowed: viewer, status: 404 },
  'PUT /api/files/:id/images/:hash': { params: (w) => ({ id: w.file, hash: IMAGE_HASH }), body: IMAGE, allowed: editor, denied: [viewer] },
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
    if (route.method !== 'HEAD') expect(await res.json()).toEqual({ error: 'UNAUTHORIZED' });
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
      expect(foreign.headers.get('content-type')).toBe(missing.headers.get('content-type'));
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
