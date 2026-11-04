import { afterAll, expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Pool } from 'pg';
import { startServer } from '../src/app';
import { createAuth } from '../src/auth';
import { connectValkey, valkeyAlive } from '../src/valkey';
import { TEST_DATABASE_URL, TEST_SECRET, TEST_VALKEY_URL, testPool } from './helpers';

const web = mkdtempSync(join(tmpdir(), 'ferrum-web-'));
mkdirSync(join(web, 'assets'));
writeFileSync(join(web, 'index.html'), '<title>Ferrum</title>');
writeFileSync(join(web, 'assets/main-abc.js'), 'export {}');
writeFileSync(join(web, 'assets/core.wasm'), new Uint8Array([0, 97, 115, 109]));
mkdirSync(join(web, 'fonts'));
writeFileSync(join(web, 'fonts/inter.woff2'), new Uint8Array(1024));

const pool = testPool();
const valkey = await connectValkey(TEST_VALKEY_URL);
const publicUrl = 'http://localhost:1';
const auth = createAuth({ publicUrl, secret: TEST_SECRET, pool, valkey, mailer: { send: async () => {} }, allowedEmails: [] });
const deps = { valkey, webDist: web, auth, publicUrl, trustedProxies: [] as string[] };
const server = startServer({ ...deps, pool }, 0);
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
  const s = startServer({ ...deps, pool: dead }, 0);
  const res = await fetch(`http://127.0.0.1:${s.port}/healthz`);
  expect(res.status).toBe(503);
  expect(await res.json()).toEqual({ ok: false, postgres: false, valkey: true });
  s.stop(true);
  await dead.end();
});

test('/healthz: Valkey недоступен — 503 сразу, а не зависание', async () => {
  const gone = await connectValkey(TEST_VALKEY_URL);
  gone.close();
  const s = startServer({ ...deps, valkey: gone, pool }, 0);
  const started = Date.now();
  const res = await fetch(`http://127.0.0.1:${s.port}/healthz`);
  expect(res.status).toBe(503);
  expect(await res.json()).toEqual({ ok: false, postgres: true, valkey: false });
  expect(Date.now() - started).toBeLessThan(1000);
  s.stop(true);
});

test('Valkey: во время обрыва команда сразу получает ошибку, после возврата клиент работает сам', async () => {
  // Прокси перед настоящим Valkey: его можно оборвать и вернуть на том же порту.
  type Pipe = { peer?: Bun.Socket<Pipe>; early: Uint8Array[] };
  const listen = (port: number) =>
    Bun.listen<Pipe>({
      hostname: '127.0.0.1',
      port,
      socket: {
        open: async (client) => {
          // Клиент пишет сразу после соединения, раньше, чем откроется соединение с Valkey.
          client.data = { early: [] };
          const peer = await Bun.connect<Pipe>({
            hostname: '127.0.0.1',
            port: Number(new URL(TEST_VALKEY_URL).port),
            socket: { data: (_up, d) => void client.write(d), close: () => void client.end(), open: () => {}, error: () => {} },
          });
          for (const chunk of client.data.early) peer.write(chunk);
          client.data.peer = peer;
        },
        data: (client, d) => (client.data.peer ? void client.data.peer.write(d) : void client.data.early.push(new Uint8Array(d))),
        close: (client) => void client.data.peer?.end(),
        error: () => {},
      },
    });
  let proxy = listen(0);
  const port = proxy.port;
  const client = await connectValkey(`redis://127.0.0.1:${port}`);
  try {
    expect(await valkeyAlive(client)).toBe(true);
    proxy.stop(true);
    await Bun.sleep(300);
    const started = Date.now();
    await expect(valkeyAlive(client)).rejects.toThrow();
    expect(Date.now() - started).toBeLessThan(1000);
    proxy = listen(port);
    let alive = false;
    for (let i = 0; i < 100 && !alive; i++) {
      await Bun.sleep(100);
      alive = await valkeyAlive(client).catch(() => false);
    }
    expect(alive).toBe(true);
  } finally {
    client.close();
    proxy.stop(true);
  }
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

test('статика: `\\0` в адресе — 404, а не сбой сервера', async () => {
  for (const path of ['/%00', '/index.html%00', '/index.html%00.png', '/assets/%00']) {
    const res = await fetch(url(path));
    expect([path, res.status]).toEqual([path, 404]);
    expectIsolated(res);
  }
});

test('статика: «навсегда» только файлы каталога `assets`, какой бы путь ни прислали', async () => {
  const res = await fetch(url('/assets/..%2findex.html'));
  expect(res.status).toBe(200);
  expect(await res.text()).toContain('<title>Ferrum</title>');
  expect(res.headers.get('cache-control')).toBe('no-cache');
  expect((await fetch(url('/assets%2fmain-abc.js'))).headers.get('cache-control')).toContain('immutable');
});

test('статика: файл не изменился — 304 без тела, шрифт не качается заново', async () => {
  const first = await fetch(url('/fonts/inter.woff2'));
  expect(first.status).toBe(200);
  expect(first.headers.get('cache-control')).toBe('no-cache');
  const etag = first.headers.get('etag')!;
  const modified = first.headers.get('last-modified')!;
  expect(etag).toMatch(/^"[0-9a-z-]+"$/);
  expect(new Date(modified).getTime()).not.toBeNaN();
  expect((await first.arrayBuffer()).byteLength).toBe(1024);

  const asked: Record<string, string>[] = [
    { 'if-none-match': etag },
    { 'if-none-match': `"other", W/${etag}` },
    { 'if-none-match': '*' },
    { 'if-modified-since': modified },
  ];
  for (const headers of asked) {
    const same = await fetch(url('/fonts/inter.woff2'), { headers });
    expect([headers, same.status]).toEqual([headers, 304]);
    expect(await same.text()).toBe('');
    expect(same.headers.get('etag')).toBe(etag);
    expect(same.headers.get('cache-control')).toBe('no-cache');
    expectIsolated(same);
  }
  const head = await fetch(url('/fonts/inter.woff2'), { method: 'HEAD', headers: { 'if-none-match': etag } });
  expect(head.status).toBe(304);

  // Другая версия у браузера: файл целиком. `If-None-Match` главнее даты.
  const stale: Record<string, string>[] = [
    { 'if-none-match': '"other"' },
    { 'if-none-match': '"other"', 'if-modified-since': modified },
    { 'if-modified-since': 'Mon, 01 Jan 2001 00:00:00 GMT' },
    { 'if-modified-since': 'garbage' },
  ];
  for (const headers of stale) {
    const changed = await fetch(url('/fonts/inter.woff2'), { headers });
    expect([headers, changed.status]).toEqual([headers, 200]);
    expect((await changed.arrayBuffer()).byteLength).toBe(1024);
  }
});

test('HSTS: только при https:// в PUBLIC_URL, на всех ответах', async () => {
  const secureUrl = 'https://ferrum.example';
  const secure = startServer(
    {
      ...deps,
      pool,
      publicUrl: secureUrl,
      auth: createAuth({ publicUrl: secureUrl, secret: TEST_SECRET, pool, valkey, mailer: { send: async () => {} }, allowedEmails: [] }),
    },
    0,
  );
  try {
    for (const path of ['/', '/nope', '/healthz', '/api/auth/nope']) {
      const res = await fetch(`http://127.0.0.1:${secure.port}${path}`);
      expect([path, res.headers.get('strict-transport-security')]).toEqual([path, 'max-age=31536000']);
    }
  } finally {
    secure.stop(true);
  }
  for (const path of ['/', '/healthz']) expect((await fetch(url(path))).headers.get('strict-transport-security')).toBeNull();
});

test('Valkey при старте: недоступен — отказ в срок, а не вечное ожидание', async () => {
  const started = Date.now();
  await expect(connectValkey('redis://127.0.0.1:1', 200)).rejects.toThrow('Valkey не ответил');
  expect(Date.now() - started).toBeLessThan(1500);
});

test('запуск: пока ждём Valkey, строка в журнале, а сигнал останавливает сразу', async () => {
  const proc = Bun.spawn(['bun', 'run', join(import.meta.dir, '../src/index.ts')], {
    env: {
      ...process.env,
      BETTER_AUTH_SECRET: TEST_SECRET,
      PUBLIC_URL: 'http://localhost:1',
      DATABASE_URL: TEST_DATABASE_URL,
      VALKEY_URL: 'redis://127.0.0.1:1',
      SMTP_URL: 'smtp://127.0.0.1:1',
      MAIL_FROM: 'Ferrum <ferrum@localhost>',
      FERRUM_PORT: '0',
    },
    stdout: 'pipe',
    stderr: 'inherit',
  });
  let output = '';
  const decoder = new TextDecoder();
  for await (const chunk of proc.stdout) {
    output += decoder.decode(chunk);
    if (output.includes('Valkey')) break;
  }
  expect(output).toContain('Подключение к Valkey');
  const signalled = Date.now();
  proc.kill('SIGTERM');
  expect(await proc.exited).toBe(0);
  expect(Date.now() - signalled).toBeLessThan(1500);
});
