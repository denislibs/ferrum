// Нагрузка присутствия отдельным процессом (спека подсистемы 4, раздел 9): 50 участников по 20 кадров в секунду, и то же с
// враждебным участником (поток кадров по 2 КБ) и сокетом, который не читает.

import { fileURLToPath } from 'node:url';
import { expect, test } from 'bun:test';

async function run(...args: string[]): Promise<Record<string, number>> {
  const p = Bun.spawn(['bun', fileURLToPath(new URL('./presence-load-probe.ts', import.meta.url)), ...args], {
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [text, err] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  await p.exited;
  const line = text.trim().split('\n').at(-1) ?? '';
  try {
    return JSON.parse(line) as Record<string, number>;
  } catch {
    throw new Error(`проба упала: ${text}\n${err.slice(-2000)}`);
  }
}

test('50 участников по 20 кадров в секунду: всё доходит, память — по участникам, а не по кадрам', async () => {
  const r = await run('load', '50', '20', '5');
  console.log('присутствие, 50 × 20 Гц', r);
  // Каждый получает кадры остальных 49; слияние в тике (33 мс) склеивает соседние кадры одного участника.
  expect(r.received).toBeGreaterThan(r.sent * 49 * 0.5);
  expect(r.dropped).toBe(0);
  expect(r.heapGrowthMb).toBeLessThan(20);
  expect(r.peakHeldKb).toBeLessThan(50 * 40);
}, 120_000);

test('враждебный участник и сокет, который не читает: остальные получают своё, сервер не копит', async () => {
  const r = await run('load', '50', '20', '5', 'hostile');
  console.log('присутствие, враждебный участник', r);
  expect(r.received).toBeGreaterThan(r.sent * 48 * 0.5);
  expect(r.dropped).toBeGreaterThan(1000);
  // Пропуск тиков у нечитающего сокета виден, когда заполнится буфер ядра ОС (за 5 с — не всегда): его проверяет
  // `presence.test.ts`. Здесь — что буфер сервера не растёт.
  expect(r.peakBufferedKb).toBeLessThan(512);
  expect(r.heapGrowthMb).toBeLessThan(20);
}, 120_000);
