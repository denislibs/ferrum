// Панель комментариев с 1000 обсуждений (проба плана, спека подсистемы 4, раздел 10.3): список виртуальный — строк в DOM столько,
// сколько видно; живое изменение одного обсуждения не пересобирает остальные строки (тот же элемент DOM). Длинные задачи
// основного потока и время открытия панели печатаются для заметок, без проверок по времени.

import { expect, test, type Page } from '@playwright/test';
import { editorReady, fileId, testMode } from '../e2e/helpers';
import { compose, newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);

const THREADS = 1000;

const psql = (sql: string) => compose('exec', '-T', 'postgres', 'psql', '-U', 'ferrum', '-d', 'ferrum', '-v', 'ON_ERROR_STOP=1', '-c', sql);

const longTasks = (page: Page) =>
  page.evaluate(() => {
    const w = window as unknown as { __long?: number[] };
    w.__long = [];
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) w.__long!.push(Math.round(e.duration));
    }).observe({ type: 'longtask', buffered: false });
  });
const takeLong = (page: Page) => page.evaluate(() => (window as unknown as { __long: number[] }).__long.splice(0));

test('панель с 1000 обсуждений: строк в DOM — видимые, живое изменение не пересобирает остальные строки', async ({ page }) => {
  await testMode(page);
  await page.goto('/');
  await signIn(page, newEmail('pp'), 'Пётр Панель');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  await expect(page.locator('.fe-sync')).toHaveText('Синхронизировано', { timeout: 15_000 });
  const id = fileId(page);
  const user = await page.evaluate(async () => ((await (await fetch('/api/auth/session')).json()) as { user: { id: string } }).user.id);
  const pageId = await page.evaluate(() => (window.__ferrum!.ui as unknown as { pages: { current: string } }).pages.current);
  // 1000 обсуждений прямо в базе: через API их не набрать — 60 комментариев в минуту на человека.
  psql(
    `WITH t AS (
       INSERT INTO comment_thread (id, file_id, page_id, node_id, x, y, wx, wy, first_id, created_by, created_at, seq, replies, last_at, participants)
       SELECT gen_random_uuid(), '${id}', '${pageId}', NULL, (i % 40) * 60, (i / 40) * 60, (i % 40) * 60, (i / 40) * 60, gen_random_uuid(),
              '${user}', now() - make_interval(mins => i), i, 0, now() - make_interval(mins => i), ARRAY['${user}']
       FROM generate_series(1, ${THREADS}) i RETURNING id, first_id, created_at, seq)
     INSERT INTO comment (id, thread_id, author_id, body, created_at)
     SELECT first_id, id, '${user}', 'Обсуждение ' || seq, created_at FROM t;
     INSERT INTO comment_file (file_id, seq) VALUES ('${id}', ${THREADS}) ON CONFLICT (file_id) DO UPDATE SET seq = ${THREADS};`,
  );
  await page.reload();
  await editorReady(page);
  await expect(page.locator('.fe-sync')).toHaveText('Синхронизировано', { timeout: 15_000 });
  await longTasks(page);
  await expect(page.getByRole('button', { name: 'Комментарий', exact: true })).toBeVisible();
  await page.locator('canvas').focus();
  const t0 = Date.now();
  await page.keyboard.press('KeyC');
  const panel = page.getByRole('region', { name: 'Комментарии' });
  await expect(panel.locator('.fe-comments__row').first()).toContainText('Обсуждение 1');
  const openMs = Date.now() - t0;
  const rows = await panel.locator('.fe-comments__row').count();
  expect(rows).toBeLessThan(40);
  // Прокрутка в конец: те же десятки строк, последние обсуждения.
  const scroller = panel.locator('.fe-comments__scroll');
  await scroller.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(panel.locator('.fe-comments__row').last()).toContainText(`Обсуждение ${THREADS}`);
  expect(await panel.locator('.fe-comments__row').count()).toBeLessThan(40);
  await scroller.evaluate((el) => (el.scrollTop = 0));
  await expect(panel.locator('.fe-comments__row').first()).toContainText('Обсуждение 1');
  // Метка на строке, которая не изменится: после живого изменения другого обсуждения элемент тот же.
  await panel
    .locator('.fe-comments__row')
    .nth(1)
    .evaluate((el) => ((el as unknown as { __mark: number }).__mark = 1));
  const scrollLong = await takeLong(page);
  const list = (await page.evaluate(async (id) => (await fetch(`/api/files/${id}/comments?after=0&limit=3`)).json(), id)) as {
    threads: { id: string; first: { body: string } }[];
  };
  const target = list.threads.find((t) => t.first.body === 'Обсуждение 1')!;
  const res = await page.evaluate(
    async ({ id, thread }) =>
      (
        await fetch(`/api/files/${id}/comments/${thread}/comments`, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ body: 'живой ответ' }),
        })
      ).status,
    { id, thread: target.id },
  );
  expect(res).toBe(201);
  await expect(panel.locator('.fe-comments__row').first()).toContainText('Ответов: 1');
  expect(
    await panel
      .locator('.fe-comments__row')
      .nth(1)
      .evaluate((el) => (el as unknown as { __mark?: number }).__mark),
  ).toBe(1);
  const updateLong = await takeLong(page);
  console.log(
    `ПРОБА панели: ${THREADS} обсуждений, открытие ${openMs} мс, строк в DOM ${rows}, длинные задачи: прокрутка ${JSON.stringify(scrollLong)}, живое изменение ${JSON.stringify(updateLong)}`,
  );
});
