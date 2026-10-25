// Измерение расстояний с Alt (разделы 7.3 и 8.6 спеки): выделить узел, держать Alt,
// навести на соседа — на холсте линии и плашки цвета `measure` (`danger` темы);
// отпустить Alt — их нет. Одиночный Alt на холсте не уводит фокус в меню браузера
// (на Windows — `preventDefault` отпускания), а в поле ввода остаётся браузеру.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, openApp } from './helpers';

/** `danger` дизайн-системы — цвет измерений в текущей теме страницы. */
async function measureColour(page: Page): Promise<number[]> {
  const dark = await page.evaluate(() => getComputedStyle(document.documentElement).colorScheme.includes('dark'));
  return dark ? [0xff, 0x7a, 0x7a] : [0xc2, 0x2c, 0x2c];
}

/** Пиксели цвета `rgb` (±8) на холсте по снимку страницы и их общая рамка. */
async function colourPixels(page: Page, rgb: number[]): Promise<{ count: number; box: number[] }> {
  const box = (await page.locator('canvas').boundingBox())!;
  const png = (await page.screenshot({ clip: box })).toString('base64');
  const r = await page.evaluate(
    async ({ png, rgb, cssWidth }) => {
      const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
      const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
      const c = new OffscreenCanvas(bmp.width, bmp.height);
      const ctx = c.getContext('2d')!;
      ctx.drawImage(bmp, 0, 0);
      const d = ctx.getImageData(0, 0, bmp.width, bmp.height).data;
      let count = 0;
      const b = [Infinity, Infinity, -Infinity, -Infinity];
      for (let i = 0; i < d.length; i += 4) {
        if (Math.abs(d[i] - rgb[0]) <= 8 && Math.abs(d[i + 1] - rgb[1]) <= 8 && Math.abs(d[i + 2] - rgb[2]) <= 8) {
          count++;
          const x = (i / 4) % bmp.width;
          const y = Math.floor(i / 4 / bmp.width);
          b[0] = Math.min(b[0], x);
          b[1] = Math.min(b[1], y);
          b[2] = Math.max(b[2], x);
          b[3] = Math.max(b[3], y);
        }
      }
      return { count, box: b, k: bmp.width / cssWidth };
    },
    { png, rgb, cssWidth: box.width },
  );
  // В CSS-пиксели холста.
  return { count: r.count, box: r.box.map((v) => v / r.k) };
}

/** Отпускания клавиш, которые до тестового слушателя уже отменило ядро. */
async function watchKeyups(page: Page): Promise<void> {
  await page.evaluate(() => {
    const w = window as unknown as { __keyups: { code: string; prevented: boolean }[] };
    w.__keyups = [];
    window.addEventListener('keyup', (e) => w.__keyups.push({ code: e.code, prevented: e.defaultPrevented }));
  });
}

/** Измерения в оверлее ядра (тестовая команда `readMeasurements`): значения
 * расстояний и число направляющих. */
async function readMeasurements(page: Page): Promise<{ values: number[]; guides: number }> {
  return page.evaluate(
    () =>
      new Promise<{ values: number[]; guides: number }>((resolve) => {
        const core = window.__ferrum!.core!;
        const off = core.on((e) => {
          if (e.type !== 'measurements') return;
          off();
          resolve({ values: e.values, guides: e.guides });
        });
        core.send({ type: 'readMeasurements' });
      }),
  );
}

const keyups = (page: Page) =>
  page.evaluate(() => (window as unknown as { __keyups: { code: string; prevented: boolean }[] }).__keyups.splice(0));

test('Alt над соседом выделенного — измерения на холсте; отпускание Alt убирает их, фокус на холсте', async ({ page }) => {
  await openApp(page);
  await watchKeyups(page);
  const colour = await measureColour(page);
  const l = await live(page);
  // Фикстура: в «Экран 0» дети 120 × 80 сеткой с шагом 140 × 100. «Слой 0» — (0, 0),
  // «Слой 1» — (140, 0): зазор 20 по x на высоте y = 40.
  const a = await appScreen(page, l, 60, 40);
  const b = await appScreen(page, l, 200, 40);
  await page.mouse.click(a.x, a.y);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.selectionCount)).toBe(1);
  expect(await page.evaluate(() => document.activeElement?.tagName)).toBe('CANVAS');
  await page.mouse.move(b.x, b.y, { steps: 4 });
  // Без Alt на холсте нет цвета измерений (фикстура синяя, наведение и выделение — `select`).
  const before = await colourPixels(page, colour);
  expect(before.count, 'цвет измерений до Alt').toBeLessThan(5);
  expect(await readMeasurements(page)).toEqual({ values: [], guides: 0 });

  await page.keyboard.down('Alt');
  await page.mouse.move(b.x, b.y - 2, { steps: 2 });
  // Линия от правого края «Слоя 0» до левого края «Слоя 1» и плашка «20» посередине.
  let on = before;
  await expect
    .poll(async () => (on = await colourPixels(page, colour)).count, { message: 'измерения не появились', timeout: 10_000 })
    .toBeGreaterThan(80);
  const canvas = (await page.locator('canvas').boundingBox())!;
  const mid = await appScreen(page, l, 130, 40);
  const [x0, y0, x1, y1] = on.box;
  const cx = (x0 + x1) / 2 + canvas.x;
  const cy = (y0 + y1) / 2 + canvas.y;
  expect(Math.abs(cx - mid.x), `центр измерения по x: ${on.box}`).toBeLessThan(4);
  expect(Math.abs(cy - mid.y), `центр измерения по y: ${on.box}`).toBeLessThan(4);
  // Плашка с текстом — фиксированного экранного размера, не тоньше 10 px.
  expect(y1 - y0).toBeGreaterThanOrEqual(10);
  // Ровно один отрезок со значением 20 и без направляющих (соседи на одной высоте).
  expect(await readMeasurements(page)).toEqual({ values: [20], guides: 0 });

  await page.keyboard.up('Alt');
  await expect
    .poll(async () => (await colourPixels(page, colour)).count, { message: 'измерения не ушли', timeout: 10_000 })
    .toBeLessThan(5);
  expect(await readMeasurements(page)).toEqual({ values: [], guides: 0 });
  // Отпускание одиночного Alt на холсте отменено (меню браузера не получает фокус),
  // фокус остался на холсте, окно в фокусе.
  expect(await keyups(page)).toEqual([{ code: 'AltLeft', prevented: true }]);
  expect(await page.evaluate(() => [document.activeElement?.tagName, document.hasFocus()])).toEqual(['CANVAS', true]);
  // Выделение и документ Alt не тронул.
  expect(await page.evaluate(() => window.__ferrum!.ui!.selectionCount)).toBe(1);
  expect((await live(page)).steps).toBe(l.steps);
});

test('Alt в поле ввода и в сочетании — у браузера и у прежних сочетаний', async ({ page }) => {
  await openApp(page);
  await watchKeyups(page);
  const l = await live(page);
  const a = await appScreen(page, l, 60, 40);
  await page.mouse.click(a.x, a.y);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.selectionCount)).toBe(1);
  // Сочетание с Alt на холсте — не одиночный Alt: его отпускание не отменяется.
  await page.keyboard.down('Alt');
  await page.keyboard.press('KeyX');
  await page.keyboard.up('Alt');
  expect(await keyups(page)).toEqual([
    { code: 'KeyX', prevented: false },
    { code: 'AltLeft', prevented: false },
  ]);
  // В поле свойств одиночный Alt остаётся браузеру.
  const x = page.getByRole('tabpanel').getByRole('textbox', { name: 'X', exact: true });
  await x.focus();
  await page.keyboard.press('Alt');
  expect(await keyups(page)).toEqual([{ code: 'AltLeft', prevented: false }]);
  await expect(x).toBeFocused();
});
