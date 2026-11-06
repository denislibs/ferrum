// Нагрузка отдельным процессом: память на соединение, задержка на 50 участниках, медленный читатель.

import { fileURLToPath } from 'node:url';
import { expect, test } from 'bun:test';

async function run(...args: string[]): Promise<Record<string, unknown>> {
  const p = Bun.spawn(['bun', fileURLToPath(new URL('./sync-load-probe.ts', import.meta.url)), ...args], {
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

test('память: 1000 соединений с подпиской — меньше 100 КБ на соединение', async () => {
  const r = await run('memory', '1000');
  console.log('память', r);
  expect(r.connections).toBe(1000);
  expect(r.perConnectionKb as number).toBeLessThan(100);
}, 120_000);

test('от кадра до рассылки, 50 участников, 5 пишут по 30 правок в секунду: всё доходит, замер §13.1 в выводе', async () => {
  const r = await run('latency', '50', '5', '30', '5');
  console.log('задержка (мс, от кадра до конца рассылки, мерит сервер)', r);
  expect(r.delivered).toBe(true);
  expect(r.measured).toBe(r.sent);
  // Цель §13.1 — 40 мс p95. Порог по времени здесь не проверяется: на нагруженной машине он плавает, а поднятый до
  // «не плавает» прятал бы промах. Сверка с целью — `FERRUM_LATENCY_GATE=1` на спокойной машине; цифры — в отчёте.
  if (process.env.FERRUM_LATENCY_GATE === '1') expect(r.p95 as number).toBeLessThanOrEqual(40);
}, 120_000);

test('медленный читатель: буфер сокета ограничен, потом догоняет всё по порядку', async () => {
  const r = await run('slow', '150');
  console.log('медленный читатель', r);
  expect(r.frames).toBe(150);
  expect(r.missing).toEqual([]);
  expect(r.ordered).toBe(true);
  // В буфере сокета — предел и один кадр, остальное ждёт в базе: 150 МБ без предела легли бы в память.
  expect(r.peakBufferedMb as number).toBeLessThan(16);
}, 180_000);

test('медленный читатель, кадры по 15 МБ: ни правки, ни своего ack не теряет', async () => {
  const r = await run('slowbig', '15', '8');
  console.log('медленный читатель, большие кадры', r);
  expect(r.missing).toEqual([]);
  expect(r.frames).toBe(8);
  expect(r.ordered).toBe(true);
  expect(r.acks).toBe(1);
}, 240_000);
