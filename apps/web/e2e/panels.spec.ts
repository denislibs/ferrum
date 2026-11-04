// Полировка панелей (спека подсистемы 2, раздел 10): секции сворачиваются и помнят
// это по имени, боковые панели сворачиваются кнопкой в шапке, ширина и свёрнутость
// переживают перезагрузку.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

/** Настройки в базе: запись идёт раз в четверть секунды — ждём её, а не время. */
const stored = (page: Page) => page.evaluate(() => window.__ferrumFiles!.store.settings());

const reload = async (page: Page) => {
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true);
};

const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

test('@firefox заголовок секции сворачивает её, свёрнутость хранится по имени', async ({ page }) => {
  await openApp(page);
  const exportSection = page.getByRole('region', { name: 'Экспорт' });
  const toggle = exportSection.getByRole('button', { name: 'Экспорт', exact: true });
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await expect(exportSection.locator('[data-export-run]')).toBeVisible();
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await expect(exportSection.locator('[data-export-run]')).toHaveCount(0);
  // Другие секции не тронуты.
  await expect(page.getByRole('region', { name: 'Страница' }).getByRole('button', { name: 'Страница' })).toHaveAttribute('aria-expanded', 'true');
  // С клавиатуры: Enter по заголовку.
  await toggle.focus();
  await page.keyboard.press('Enter');
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('Enter');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');

  // Та же секция при другом выделении — свёрнута; имя «Положение» и у фрейма.
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await expect(exportSection.getByRole('button', { name: 'Экспорт', exact: true })).toHaveAttribute('aria-expanded', 'false');
  const position = page.getByRole('region', { name: 'Положение' });
  await position.getByRole('button', { name: 'Положение' }).click();
  await expect(position.getByRole('button', { name: 'Положение' })).toHaveAttribute('aria-expanded', 'false');
  // Полей больше не видно.
  await expect(position.getByRole('textbox')).toHaveCount(0);

  await expect.poll(() => stored(page)).toMatchObject({ collapsedSections: 'Экспорт\nПоложение' });
  await reload(page);
  await expect(page.getByRole('region', { name: 'Экспорт' }).getByRole('button', { name: 'Экспорт', exact: true })).toHaveAttribute('aria-expanded', 'false');
  expect(await warnings(page)).toEqual([]);
});

test('@firefox «Экспорт…» из меню разворачивает свёрнутую секцию и панель', async ({ page }) => {
  await openApp(page);
  await page.getByRole('region', { name: 'Экспорт' }).getByRole('button', { name: 'Экспорт', exact: true }).click();
  await page.getByRole('button', { name: 'Свернуть правую панель' }).click();
  await expect(page.locator('[data-export-run]')).toHaveCount(0);
  await mainMenu(page, 'Файл', /Экспорт/);
  await expect(page.locator('[data-export-run]')).toBeFocused();
});

test('@firefox левая панель сворачивается кнопкой в шапке, меню остаётся, ширина и свёрнутость запоминаются', async ({ page }) => {
  await openApp(page);
  const canvas = page.locator('canvas');
  const before = (await canvas.boundingBox())!;
  const collapse = page.getByRole('button', { name: 'Свернуть левую панель' });
  await expect(collapse).toHaveAttribute('aria-expanded', 'true');
  await collapse.focus();
  await page.keyboard.press('Enter');
  const expand = page.getByRole('button', { name: 'Развернуть левую панель' });
  await expect(expand).toBeFocused();
  await expect(page.getByRole('region', { name: 'Слои' })).toHaveCount(0);
  await expect(page.getByRole('region', { name: 'Страницы' })).toHaveCount(0);
  await expect.poll(async () => (await canvas.boundingBox())!.width).toBe(before.width + 248 - 48);
  await expect.poll(async () => (await live(page)).viewportW).toBe(before.width + 248 - 48);
  // Главное меню — на месте.
  await mainMenu(page, 'Вид', 'Линейки');
  await liveWhen(page, 'true');

  await expect.poll(() => stored(page)).toMatchObject({ leftCollapsed: true });
  await reload(page);
  await expect(page.getByRole('button', { name: 'Развернуть левую панель' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Слои' })).toHaveCount(0);

  // Развернуть — с прежней шириной, затем сменить её и снова перезагрузить.
  await page.getByRole('button', { name: 'Развернуть левую панель' }).click();
  await expect(page.getByRole('region', { name: 'Слои' })).toBeVisible();
  await expect.poll(async () => (await canvas.boundingBox())!.x).toBe(248);
  const edge = page.getByRole('separator', { name: 'Ширина левой панели' });
  await edge.focus();
  for (const x of [256, 264, 272]) {
    await page.keyboard.press('ArrowRight');
    await expect.poll(async () => (await canvas.boundingBox())!.x).toBe(x);
  }
  await expect.poll(() => stored(page)).toMatchObject({ leftCollapsed: false, leftWidth: 272 });
  await reload(page);
  await expect.poll(async () => (await page.locator('canvas').boundingBox())!.x).toBe(272);
  expect(await warnings(page)).toEqual([]);
});

test('@firefox правая панель сворачивается и помнит это', async ({ page }) => {
  await openApp(page);
  const canvas = page.locator('canvas');
  const before = (await canvas.boundingBox())!;
  await page.getByRole('button', { name: 'Свернуть правую панель' }).click();
  await expect(page.getByRole('tabpanel')).toHaveCount(0);
  await expect.poll(async () => (await canvas.boundingBox())!.width).toBe(before.width + 248 - 48);
  await expect.poll(() => stored(page)).toMatchObject({ rightCollapsed: true });
  await reload(page);
  await expect(page.getByRole('button', { name: 'Развернуть правую панель' })).toBeVisible();
  await page.getByRole('button', { name: 'Развернуть правую панель' }).click();
  await expect(page.getByRole('tabpanel')).toBeVisible();
  await expect.poll(async () => (await page.locator('canvas').boundingBox())!.width).toBe(before.width);
});
