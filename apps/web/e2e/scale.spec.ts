// Инструмент «Масштабирование» (K, подсистема 3, этап 5): угловая ручка
// масштабирует прямоугольник вместе с обводкой и радиусом одним шагом, поле
// «Масштаб» панели — вокруг выбранной точки рамки; кнопка тулбара и меню
// «Инструменты перемещения». Документ тест читает сводкой панели свойств.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, live, liveWhen, openApp } from './helpers';

type LiveState = Awaited<ReturnType<typeof live>>;

test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

const summary = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)));

/** Пустой файл при зуме 100 %, прямоугольник 100 × 100 от (cameraX − 100, cameraY − 50)
 * с обводкой 2 и радиусом 10, выделен. */
async function styledRect(page: Page): Promise<LiveState> {
  await openApp(page, 0);
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+Digit0');
  const l = await liveWhen(page, 'l.zoom === 1');
  await page.keyboard.press('KeyR');
  await drag(page, await appScreen(page, l, l.cameraX - 100, l.cameraY - 50), 100, 100);
  await liveWhen(page, 'l.busy === 0');
  const send = (edit: object) =>
    page.evaluate((e) => window.__ferrum!.core!.send({ type: 'editSelection', edit: e as never, phase: 'commit' } as never), edit);
  await send({ type: 'number', field: 'cornerRadius', value: 10, relative: false });
  await send({ type: 'strokes', strokes: [{ color: '#000000ff', width: 2, align: 'inside' }] });
  await expect.poll(async () => (await summary(page))?.strokes?.list?.[0]?.width).toBe(2);
  return l;
}

test('K: угловая ручка — масштаб с обводкой и радиусом одним шагом @firefox', async ({ page }) => {
  const l = await styledRect(page);
  await page.keyboard.press('KeyK');
  const toolbar = page.getByRole('toolbar', { name: 'Инструменты' });
  await expect(toolbar.getByRole('button', { name: 'Масштабирование' })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('radiogroup', { name: 'Точка масштаба' })).toBeVisible();
  const before = await live(page);
  // Правый нижний угол рамки — на (100, 100) дальше: ×2.
  await drag(page, await appScreen(page, l, l.cameraX, l.cameraY + 50), 100, 100);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  const s = await summary(page);
  expect([s.w.value, s.h.value, s.cornerRadius.value, s.strokes.list[0].width]).toEqual([200, 200, 20, 4]);
  // Инструмент остаётся (как в Figma), отмена — одним шагом.
  expect(await page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('scale');
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(async () => (await summary(page)).w.value).toBe(100);
  expect((await summary(page)).strokes.list[0].width).toBe(2);
});

test('поле «Масштаб»: 50 % вокруг центра, точка рамки стрелками', async ({ page }) => {
  await styledRect(page);
  const toolbar = page.getByRole('toolbar', { name: 'Инструменты' });
  await toolbar.getByRole('button', { name: 'Инструменты перемещения' }).click();
  await page.getByRole('menu', { name: 'Инструменты перемещения' }).getByRole('menuitemcheckbox', { name: /Масштабирование/ }).click();
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('scale');
  const s0 = await summary(page);
  // Точка — центр: из левого верхнего угла стрелками вправо и вниз.
  const grid = page.getByRole('radiogroup', { name: 'Точка масштаба' });
  await grid.getByRole('radio', { name: 'Слева сверху' }).focus();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowDown');
  await expect(grid.getByRole('radio', { name: 'По центру', exact: true })).toHaveAttribute('aria-checked', 'true');
  const field = page.getByRole('tabpanel').getByRole('textbox', { name: 'Масштаб', exact: true });
  await expect(field).toHaveValue('100%');
  const before = await live(page);
  await field.fill('50');
  await field.press('Enter');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  // Сводка приходит событием после шага документа: ждём значения, а не читаем один раз.
  await expect
    .poll(async () => {
      const s = await summary(page);
      return [s.w.value, s.x.value - s0.x.value, s.y.value - s0.y.value, s.cornerRadius.value, s.strokes.list[0].width];
    })
    .toEqual([50, 25, 25, 5, 1]);
  // Поле снова показывает 100 %: масштаб — относительно текущего размера.
  await expect(field).toHaveValue('100%');
});
