// Нагрузка комментариев отдельным процессом (`comments-load-probe.ts`): большой файл, рассылка событий 50 соединениям, то же
// под потоком враждебного человека. Цифры времени печатаются; пределы времени — с большим запасом (~1,5 с): ловят обвал
// (секундные простои), а не чужую нагрузку машины. Проверяется то, что от нагрузки не зависит: доставка всем, ни одного
// отказа законному автору, общий предел враждебного, пропуск медленному читателю, плато памяти.

import { fileURLToPath } from 'node:url';
import { expect, test } from 'bun:test';

type Stat = { p50: number; p95: number; max: number };

/** Обвал, а не медленная машина: цели §13.1 (150 мс) печатаются, утверждается порядок секунд. */
const COLLAPSE_MS = 1500;

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

test('1000 обсуждений по 20 комментариев: список, обсуждение, создание и ответ — по индексам', async () => {
  const r = await run('big');
  console.log('комментарии, большой файл (мс)', r);
  expect((r.listAllPages as Stat).p95).toBeLessThan(2 * COLLAPSE_MS);
  for (const k of ['listAfter', 'detail', 'create', 'reply', 'mentions']) expect((r[k] as Stat).p95, k).toBeLessThan(COLLAPSE_MS);
  expect(r.indexes).toEqual({
    list: ['comment_thread_live_idx'],
    listAfter: ['comment_thread_seq_idx'],
    threadComments: ['comment_thread_idx'],
    unread: ['comment_thread_idx', 'comment_read_pkey'],
    mentions: ['comment_mention_pkey', 'user_pkey'],
  });
  expect(Object.values(r.seqScans as Record<string, boolean>).some(Boolean)).toBe(false);
}, 180_000);

test('событие 50 соединениям одного файла: доходит до всех, автор не получает отказов', async () => {
  const r = await run('fanout', '8');
  console.log('комментарии, рассылка 50 соединениям', r);
  expect(r.failed).toBe(0);
  expect(r.deliveries).toBe(r.expected);
  expect(r.comments as number).toBeGreaterThanOrEqual(30);
  expect(r.fromResponseP95 as number).toBeLessThan(COLLAPSE_MS);
  expect(r.heapGrowthMb as number).toBeLessThan(20);
}, 180_000);

test('враждебный человек (5 сеансов, 40 потоков) и сокет, который не читает: остальные получают всё, предел общий, память — плато', async () => {
  const r = await run('fanout', '8', 'hostile');
  console.log('комментарии, враждебный поток', r);
  // Законный автор: ни одного отказа и не задушен (без потока — ~80 за 8 с).
  expect(r.failed).toBe(0);
  expect(r.comments as number).toBeGreaterThanOrEqual(30);
  expect(r.deliveries).toBe(r.expected);
  expect(r.restP95 as number).toBeLessThan(COLLAPSE_MS);
  expect(r.fromResponseP95 as number).toBeLessThan(COLLAPSE_MS);
  // 60 комментариев в минуту на человека — на все его сеансы вместе; отказы есть.
  expect(r.hostileAcceptedComments as number).toBeLessThanOrEqual(60);
  expect(r.hostile429 as number).toBeGreaterThan(0);
  // Сокет, который не читает: открыт (101), роль подтверждена, события ему пропускаются, буфер — порог (64 КБ) и одно
  // событие (до 64 КБ).
  expect([r.rawUpgraded, r.rawConfirmed]).toEqual([true, true]);
  expect(r.commentSkipped as number).toBeGreaterThan(0);
  expect(r.peakConnBufferedKb as number).toBeLessThan(64 + 64 + 4);
  expect(r.loopLagP95 as number).toBeLessThan(50);
  // Память: после сборки — как в начале (ничего не удержано); пик ограничен; последняя треть замера не выше средней
  // больше, чем на шум распределителя (±50 МБ между прогонами; 30 с потока — плато ~110 МБ).
  expect(r.heapGrowthMb as number).toBeLessThan(20);
  expect(r.rssPeakGrowthMb as number).toBeLessThan(300);
  const [, middle, end] = r.rssThirdsPeakMb as number[];
  expect(end - middle).toBeLessThan(100);
}, 180_000);
