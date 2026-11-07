// Нагрузка комментариев отдельным процессом (`comments-load-probe.ts`): большой файл, рассылка событий 50 соединениям, то же
// под потоком враждебного человека. Пределы — с запасом на чужую нагрузку машины; цифры печатаются.

import { fileURLToPath } from 'node:url';
import { expect, test } from 'bun:test';

type Stat = { p50: number; p95: number; max: number };

async function run(...args: string[]): Promise<Record<string, unknown>> {
  const p = Bun.spawn(['bun', fileURLToPath(new URL('./comments-load-probe.ts', import.meta.url)), ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [text, err] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  await p.exited;
  const line = text.trim().split('\n').at(-1) ?? '';
  try {
    return JSON.parse(line) as Record<string, unknown>;
  } catch {
    throw new Error(`проба упала: ${text}\n${err.slice(-2000)}`);
  }
}

test('1000 обсуждений по 20 комментариев: список, обсуждение, создание и ответ — по индексам и быстро', async () => {
  const r = await run('big');
  console.log('комментарии, большой файл (мс)', r);
  expect((r.listAllPages as Stat).p95).toBeLessThan(1000);
  for (const k of ['listAfter', 'detail', 'create', 'reply', 'mentions']) expect((r[k] as Stat).p95, k).toBeLessThan(150);
  expect(r.indexes).toEqual({
    list: ['comment_thread_live_idx'],
    listAfter: ['comment_thread_seq_idx'],
    threadComments: ['comment_thread_idx'],
    unread: ['comment_thread_idx', 'comment_read_pkey'],
    mentions: ['comment_mention_pkey', 'user_pkey'],
  });
  expect(Object.values(r.seqScans as Record<string, boolean>).some(Boolean)).toBe(false);
}, 180_000);

test('событие 50 соединениям одного файла: доходит до всех, от ответа REST до прихода — в пределах цели', async () => {
  const r = await run('fanout', '8');
  console.log('комментарии, рассылка 50 соединениям', r);
  expect(r.deliveries).toBe(r.expected);
  expect(r.fromResponseP95 as number).toBeLessThan(150);
  expect(r.heapGrowthMb as number).toBeLessThan(20);
}, 180_000);

test('враждебный человек (5 сеансов, 40 потоков) и сокет, который не читает: остальные получают всё, предел общий, память не растёт', async () => {
  const r = await run('fanout', '8', 'hostile');
  console.log('комментарии, враждебный поток', r);
  expect(r.deliveries).toBe(r.expected);
  // 60 комментариев в минуту на человека — на все его сеансы вместе.
  expect(r.hostileAcceptedComments as number).toBeLessThanOrEqual(60);
  expect(r.hostile429 as number).toBeGreaterThan(1000);
  expect(r.fromResponseP95 as number).toBeLessThan(500);
  expect(r.loopLagP95 as number).toBeLessThan(50);
  expect(r.heapGrowthMb as number).toBeLessThan(20);
}, 180_000);
