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

test('автономная сборка: адреса `#/auth/…` ей не принадлежат, они ведут на экран файлов', async ({ page }) => {
  await page.goto('/#/auth/foo');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.evaluate(() => void (location.hash = '#/trash'));
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Корзина');
  // Адрес, набранный при открытом приложении, тот же: экран и адрес не расходятся.
  await page.evaluate(() => void (location.hash = '#/auth/foo'));
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.evaluate(() => void (location.hash = '#/trash'));
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Корзина');
  await page.evaluate(() => void (location.hash = '#/auth/confirm?token=abc'));
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await expect(page.getByRole('button', { name: 'Войти', exact: true })).toHaveCount(0);
});
