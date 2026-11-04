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
