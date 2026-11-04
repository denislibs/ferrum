import { expect, test } from '@playwright/test';

// Автономная сборка: сразу экран файлов, без входа и без запросов к серверу.
test('автономная сборка открывается без входа и не ходит на сервер', async ({ page }) => {
  const api: string[] = [];
  page.on('request', (r) => {
    if (new URL(r.url()).pathname.startsWith('/api/')) api.push(r.url());
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await expect(page.getByRole('button', { name: 'Получить ссылку' })).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.dataset.ferrumLocal)).toBe('1');
  expect(api).toEqual([]);
});
