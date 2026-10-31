// Векторы (подсистема 3, этап 3): перо на холсте одним шагом, правка вектора
// двойным щелчком (точка тянется, Esc — выход), «Сплющить» двух прямоугольников,
// окончания и соединение в секции «Обводка», инструмент «Стрелка». Документ тест
// читает сводкой панели свойств и строками слоёв, вид — пикселями экрана.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, live, liveWhen, openApp, pixel } from './helpers';

// Чтений layout вне фазы measure планировщика не было (тестовый режим их копит).
test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

const summary = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)));
const tool = (page: Page) => page.evaluate(() => window.__ferrum!.ui!.tool);
const rows = (page: Page) => page.getByRole('tree', { name: 'Слои' }).getByRole('treeitem');

/** Пустой файл на белом фоне при зуме 100 % (пиксель экрана — единица мира), фокус на холсте. */
async function blank(page: Page): Promise<void> {
  await openApp(page, 0);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setPageBackground', color: '#ffffff', phase: 'commit' }));
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+Digit0');
  await liveWhen(page, 'l.zoom === 1');
}

/** Тёмных пикселей в прямоугольнике экрана (обводки чёрные на белом фоне). */
async function darkIn(page: Page, at: { x: number; y: number }, w: number, h: number): Promise<number> {
  const png = (await page.screenshot({ clip: { x: at.x, y: at.y, width: w, height: h } })).toString('base64');
  return page.evaluate(async (png) => {
    const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    const ctx = c.getContext('2d')!;
    ctx.drawImage(bmp, 0, 0);
    const d = ctx.getImageData(0, 0, bmp.width, bmp.height).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] < 160) n++;
    return n;
  }, png);
}

/** Щелчки пером по точкам мира и Enter; ждёт записанный вектор. */
async function penPath(page: Page, points: [number, number][]): Promise<void> {
  const before = await live(page);
  await page.keyboard.press('KeyP');
  await expect.poll(() => tool(page)).toBe('pen');
  for (const [x, y] of points) {
    const at = await appScreen(page, before, x, y);
    await page.mouse.click(at.x, at.y);
  }
  await page.keyboard.press('Enter');
  await liveWhen(page, `l.nodes === ${before.nodes + 1} && l.steps === ${before.steps + 1}`);
}

test('перо: щелчки и Enter — вектор одним шагом, отмена убирает @firefox', async ({ page }) => {
  await blank(page);
  const l = await live(page);
  await penPath(page, [
    [l.cameraX - 100, l.cameraY - 50],
    [l.cameraX + 100, l.cameraY - 50],
    [l.cameraX + 100, l.cameraY + 50],
  ]);
  await expect.poll(() => tool(page)).toBe('move');
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toHaveAccessibleName('Вектор');
  const s = await summary(page);
  expect([s.kind, s.w.value, s.h.value]).toEqual(['vector', 200, 100]);
  // Отрезок нарисован (под рамкой выделения тоже): у верхней стороны вдали от ручек
  // есть тёмные пиксели обводки и рамки.
  const mid = await appScreen(page, l, l.cameraX - 53, l.cameraY - 53);
  await expect.poll(() => darkIn(page, mid, 6, 6)).toBeGreaterThan(0);
  await page.keyboard.press('ControlOrMeta+z');
  await liveWhen(page, `l.nodes === ${l.nodes}`);
});

test('правка вектора: двойной щелчок, точка тянется одним шагом, Esc — выход', async ({ page }) => {
  await blank(page);
  const l = await live(page);
  const [x0, y0] = [l.cameraX - 100, l.cameraY - 50];
  await penPath(page, [
    [x0, y0],
    [x0 + 200, y0],
  ]);
  const mid = await appScreen(page, l, x0 + 100, y0);
  await page.mouse.dblclick(mid.x, mid.y);
  // Правый конец (x0 + 200, y0) — вниз на 80.
  const before = await live(page);
  const end = await appScreen(page, l, x0 + 200, y0);
  await drag(page, end, 0, 80);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect.poll(async () => (await summary(page)).h.value).toBe(80);
  await page.keyboard.press('Escape');
  // Вне правки Delete удаляет узел целиком (в правке — точки).
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes}`);
});

test('«Сплющить»: два прямоугольника — один вектор', async ({ page }) => {
  await blank(page);
  const l = await live(page);
  for (const dx of [-150, 50]) {
    await page.keyboard.press('KeyR');
    await drag(page, await appScreen(page, l, l.cameraX + dx, l.cameraY - 40), 100, 80);
    await liveWhen(page, `l.busy === 0`);
  }
  await expect(rows(page)).toHaveCount(2);
  await page.keyboard.press('ControlOrMeta+a');
  const before = await live(page);
  await page.keyboard.press('ControlOrMeta+e');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect(rows(page)).toHaveCount(1);
  const s = await summary(page);
  expect([s.kind, s.w.value, s.h.value]).toEqual(['vector', 300, 80]);
  // Обе заливки на месте: серый внутри каждого прямоугольника.
  for (const dx of [-100, 100]) {
    const at = await appScreen(page, l, l.cameraX + dx, l.cameraY);
    await expect.poll(async () => (await pixel(page, at.x, at.y))[0]).toBe(217);
  }
});

test('«Обводка»: окончание «Стрелка» у линии, соединение; инструмент «Стрелка»', async ({ page }) => {
  await blank(page);
  const l = await live(page);
  await page.keyboard.press('KeyL');
  const from = await appScreen(page, l, l.cameraX - 100, l.cameraY);
  await drag(page, from, 200, 0);
  await liveWhen(page, `l.busy === 0 && l.nodes === ${l.nodes + 1}`);
  const end = page.getByRole('combobox', { name: 'Окончание в конце' });
  await expect(end).toHaveValue('none');
  // Под концом линии пусто (ручка конца — правее): плечо стрелки появится там.
  const box = await appScreen(page, l, l.cameraX + 100 - 10, l.cameraY + 2);
  expect(await darkIn(page, box, 4, 5)).toBe(0);
  // Соединения у линии нет (углов нет).
  await expect(page.getByRole('combobox', { name: 'Соединение' })).toHaveCount(0);
  const before = await live(page);
  await end.selectOption('arrowLines');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect.poll(async () => (await summary(page)).strokeStyle.capEnd).toBe('arrowLines');
  // Плечо стрелки длиной 9 под 30° назад: (−7,8; +4,5) от конца линии.
  await expect.poll(() => darkIn(page, box, 4, 5)).toBeGreaterThan(0);
  // «Стрелка» (Shift+L) рисует линию со стрелкой сразу.
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+KeyL');
  await expect.poll(() => tool(page)).toBe('arrow');
  await drag(page, await appScreen(page, l, l.cameraX - 100, l.cameraY + 100), 200, 0);
  await liveWhen(page, `l.busy === 0 && l.nodes === ${l.nodes + 2}`);
  await expect.poll(async () => (await summary(page)).strokeStyle.capEnd).toBe('arrowLines');
  // Прямоугольник: соединение есть, окончаний нет; «Круглое» прячет предел угла.
  await page.keyboard.press('KeyR');
  await drag(page, await appScreen(page, l, l.cameraX - 50, l.cameraY + 150), 100, 60);
  await liveWhen(page, `l.busy === 0 && l.nodes === ${l.nodes + 3}`);
  await expect(page.getByRole('combobox', { name: 'Окончание в конце' })).toHaveCount(0);
  await expect(page.getByRole('combobox', { name: 'Соединение' })).toHaveCount(0);
  // У прямоугольника без обводки строк стиля нет: добавить обводку.
  await page.getByRole('button', { name: 'Добавить обводку' }).click();
  const join = page.getByRole('combobox', { name: 'Соединение' });
  await expect(join).toHaveValue('miter');
  await expect(page.getByRole('textbox', { name: 'Предел острого угла' })).toBeVisible();
  await join.selectOption('round');
  await expect.poll(async () => (await summary(page)).strokeStyle.join).toBe('round');
  await expect(page.getByRole('textbox', { name: 'Предел острого угла' })).toHaveCount(0);
});

test('перо в тулбаре и подсказка', async ({ page }) => {
  await blank(page);
  const pen = page.getByRole('button', { name: 'Перо', exact: true });
  await pen.click();
  await expect(pen).toHaveAttribute('aria-pressed', 'true');
  await page.locator('canvas').focus();
  await page.keyboard.press('Escape');
  await expect(pen).toHaveAttribute('aria-pressed', 'false');
});
