import { afterAll, expect, test } from 'bun:test';
import { createPool } from '../src/db';
import { tcpProxy, TEST_DATABASE_URL } from './helpers';

const stops: (() => unknown)[] = [];
afterAll(async () => {
  for (const s of stops) await s();
});

test('умолчания: у всех запросов есть срок', () => {
  const pool = createPool(TEST_DATABASE_URL);
  stops.push(() => pool.end());
  expect(pool.options).toMatchObject({ connectionTimeoutMillis: 3000, statement_timeout: 5000, query_timeout: 6000 });
});

test('сервер принял соединение и молчит: подключение отказывает в срок', async () => {
  const silent = Bun.listen({ hostname: '127.0.0.1', port: 0, socket: { data: () => {} } });
  stops.push(() => silent.stop(true));
  const pool = createPool(`postgres://ferrum:ferrum@127.0.0.1:${silent.port}/ferrum`, { connectionTimeoutMillis: 150 });
  stops.push(() => pool.end());
  const started = Date.now();
  await expect(pool.query('SELECT 1')).rejects.toThrow();
  expect(Date.now() - started).toBeLessThan(1500);
});

test('долгий запрос снимает сама база: statement_timeout', async () => {
  const pool = createPool(TEST_DATABASE_URL, { statement_timeout: 100 });
  stops.push(() => pool.end());
  await expect(pool.query('SELECT pg_sleep(5)')).rejects.toThrow('statement timeout');
});

test('база зависла после подключения: запрос отказывает в срок клиента, пул не падает', async () => {
  const url = new URL(TEST_DATABASE_URL);
  const proxy = tcpProxy(Number(url.port));
  stops.push(() => proxy.stop());
  url.hostname = '127.0.0.1';
  url.port = String(proxy.port);
  const pool = createPool(url.href, { query_timeout: 150 });
  stops.push(() => pool.end().catch(() => {}));
  expect((await pool.query('SELECT 1 AS n')).rows).toEqual([{ n: 1 }]);
  proxy.freeze();
  const started = Date.now();
  await expect(pool.query('SELECT 2')).rejects.toThrow('timeout');
  expect(Date.now() - started).toBeLessThan(1500);
});
