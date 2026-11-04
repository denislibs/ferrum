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
  // Заголовок остаётся заголовком (h2), кнопка — внутри него.
  await expect(exportSection.getByRole('heading', { name: 'Экспорт' }).getByRole('button')).toHaveCount(1);
  await exportSection.getByRole('heading', { name: 'Экспорт' }).getByRole('button').click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
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

test('@firefox фокус остаётся на кнопке правой панели и на заголовке секции', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Свернуть правую панель' }).focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('button', { name: 'Развернуть правую панель' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('button', { name: 'Свернуть правую панель' })).toBeFocused();
  const toggle = page.getByRole('region', { name: 'Экспорт' }).getByRole('button', { name: 'Экспорт', exact: true });
  await toggle.focus();
  await page.keyboard.press('Enter');
  await expect(toggle).toBeFocused();
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
});

test('@firefox Ctrl+\\ при свёрнутых панелях прячет и возвращает интерфейс', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Свернуть левую панель' }).click();
  await page.getByRole('button', { name: 'Свернуть правую панель' }).click();
  await liveWhen(page, 'l.viewportW === 1280 - 2 * 48');
  await page.locator('canvas').click({ position: { x: 600, y: 40 } });
  await page.keyboard.press('ControlOrMeta+Backslash');
  await expect(page.getByRole('complementary', { name: 'Страницы и слои' })).toBeHidden();
  await liveWhen(page, 'l.viewportW === 1280');
  await page.keyboard.press('ControlOrMeta+Backslash');
  await expect(page.getByRole('button', { name: 'Развернуть левую панель' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Развернуть правую панель' })).toBeVisible();
  await liveWhen(page, 'l.viewportW === 1280 - 2 * 48');
});

test('@firefox вид не прыгает: точка мира остаётся на месте экрана при сворачивании и разворачивании', async ({ page }) => {
  await openApp(page);
  const where = async () => appScreen(page, await live(page), 100, 100);
  const same = async (a: { x: number; y: number }, w: number) => {
    await liveWhen(page, `l.viewportW === ${w}`);
    await expect.poll(async () => Math.abs((await where()).x - a.x) + Math.abs((await where()).y - a.y)).toBeLessThan(1);
  };
  const a = await where();
  const full = (await live(page)).viewportW;
  await page.getByRole('button', { name: 'Свернуть левую панель' }).click();
  await same(a, full + 200);
  await page.getByRole('button', { name: 'Свернуть правую панель' }).click();
  await same(a, full + 400);
  await page.getByRole('button', { name: 'Развернуть левую панель' }).click();
  await same(a, full + 200);
  await page.getByRole('button', { name: 'Развернуть правую панель' }).click();
  await same(a, full);
});

test('@firefox сворачивание панели не оставляет ложной обводки наведения', async ({ page }) => {
  await openApp(page);
  const box = (await page.locator('canvas').boundingBox())!;
  // Прямоугольник у верха холста; затем вид уезжает влево и вверх так, что он
  // накрывает точку за краем холста (минус отступ холста от угла окна) — туда раньше
  // падала точка синтетической прокрутки.
  await page.keyboard.press('KeyR');
  await page.mouse.move(box.x + 100, box.y + 50);
  await page.mouse.down();
  await page.mouse.move(box.x + 700, box.y + 300, { steps: 6 });
  await page.mouse.up();
  await page.keyboard.press('Escape');
  await liveWhen(page, 'l.selectionCount === 0');
  const before = await live(page);
  await page.mouse.move(box.x + box.width * 0.7, box.y + box.height / 2);
  await page.mouse.wheel(600, 80);
  await liveWhen(page, `l.cameraX !== ${before.cameraX}`);
  // Указатель уходит с холста: наведения нет, пока он не вернётся.
  const collapse = page.getByRole('button', { name: 'Свернуть левую панель' });
  await collapse.hover();
  // Правая кромка прямоугольника (x = 100 от левого края холста): обводка наведения легла бы на неё.
  const clip = { x: box.x + 86, y: box.y, width: 14, height: 200 };
  await expect.poll(async () => (await page.screenshot({ clip })).length).toBeGreaterThan(0);
  const quiet = await page.screenshot({ clip });
  const full = (await live(page)).viewportW;
  await collapse.click();
  await liveWhen(page, `l.viewportW === ${full + 200}`);
  await expect
    .poll(async () => (await page.screenshot({ clip })).equals(quiet), { timeout: 5000 })
    .toBe(true);
});

test('@firefox кнопка правой панели не налезает на «Масштаб» и «Поделиться» при минимальной ширине', async ({ page }) => {
  await openApp(page);
  const edge = page.getByRole('separator', { name: 'Ширина правой панели' });
  await edge.focus();
  const aside = page.getByRole('complementary', { name: 'Свойства' });
  const width = async () => (await aside.boundingBox())!.width;
  // У правой панели стрелка вправо сужает: до упора (200).
  for (let w = 240; w >= 200; w -= 8) {
    await page.keyboard.press('ArrowRight');
    await expect.poll(width).toBe(w);
  }
  const toggle = (await page.getByRole('button', { name: 'Свернуть правую панель' }).boundingBox())!;
  for (const name of ['Масштаб', 'Поделиться']) {
    const b = (await page.getByRole('complementary', { name: 'Свойства' }).getByRole('button', { name, exact: true }).boundingBox())!;
    const apart = b.x >= toggle.x + toggle.width || b.x + b.width <= toggle.x;
    expect(apart, `${name} пересекает кнопку панели`).toBe(true);
    const panel = (await aside.boundingBox())!;
    expect(b.x + b.width, `${name} вылезает за панель`).toBeLessThanOrEqual(panel.x + panel.width);
  }
});
