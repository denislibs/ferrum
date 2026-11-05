// Нагрузка отдельным процессом: память на соединение, задержка на 50 участниках, медленный читатель.

import { fileURLToPath } from 'node:url';
import { expect, test } from 'bun:test';

async function run(...args: string[]): Promise<Record<string, number | boolean>> {
  const p = Bun.spawn(['bun', fileURLToPath(new URL('./sync-load-probe.ts', import.meta.url)), ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [text, err] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  await p.exited;
  const line = text.trim().split('\n').at(-1) ?? '';
  try {
    return JSON.parse(line) as Record<string, number | boolean>;
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

test('от кадра до рассылки, 50 участников (цель 40 мс, p95)', async () => {
  const r = await run('latency', '50');
  console.log('задержка', r);
  // Клиенты в том же процессе, что и сервер, и опрашивают кадры: порог с запасом, цифра в выводе.
  expect(r.p95 as number).toBeLessThan(60);
}, 120_000);

test('медленный читатель: буфер сокета ограничен, потом догоняет всё по порядку', async () => {
  const r = await run('slow', '150');
  console.log('медленный читатель', r);
  expect(r.frames).toBe(150);
  expect(r.ordered).toBe(true);
  // В буфере сокета — предел и один кадр, остальное ждёт в базе: 150 МБ без предела легли бы в память.
  expect(r.peakBufferedMb as number).toBeLessThan(16);
}, 180_000);
