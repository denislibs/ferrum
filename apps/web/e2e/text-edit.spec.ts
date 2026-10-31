// Правка текста на холсте (подсистема 3, этап 2): инструмент T, набор в скрытое
// поле, ход курсора и удаление, IME синтетическими событиями составления, отмена
// ввода подряд одним шагом, клавиши в правке не уходят командам раскладки, буфер
// обмена (фрагмент со стилями, обычный текст вне правки). Текст документа тест
// читает копией фрагмента (журнал событий тестового режима хранит её обычный текст).

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

const ui = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!)));
const clipTexts = (page: Page) =>
  page.evaluate(() => window.__ferrum!.uiEvents!.filter((e) => e.type === 'clipboard' && e.text !== undefined).map((e) => e.text!));

/** Новый текст инструментом T в точке мира; ждёт начала правки. */
async function newText(page: Page, x: number, y: number): Promise<void> {
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyT');
  await expect.poll(async () => (await ui(page)).tool).toBe('text');
  const at = await appScreen(page, await live(page), x, y);
  await page.mouse.click(at.x, at.y);
  await expect.poll(async () => (await ui(page)).text.editing).toBe(true);
  await expect(page.locator('.fe-text-input')).toBeFocused();
}

/** Текст правимого узла: выделить всё и скопировать фрагмент. */
async function copyAll(page: Page): Promise<string> {
  const before = (await clipTexts(page)).length;
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.press('ControlOrMeta+c');
  await expect.poll(async () => (await clipTexts(page)).length).toBe(before + 1);
  return (await clipTexts(page))[before];
}

/** Тёмных пикселей в квадрате экрана (текст чёрный на белом фоне страницы). */
async function darkPixels(page: Page, at: { x: number; y: number }, size: number): Promise<number> {
  const png = (await page.screenshot({ clip: { x: at.x, y: at.y, width: size, height: size } })).toString('base64');
  return page.evaluate(async (png) => {
    const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    const ctx = c.getContext('2d')!;
    ctx.drawImage(bmp, 0, 0);
    const d = ctx.getImageData(0, 0, bmp.width, bmp.height).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] < 100 && d[i + 1] < 100 && d[i + 2] < 100) n++;
    return n;
  }, png);
}

test('инструмент T: щелчок, набор, Esc — текст на холсте одним шагом @firefox', async ({ page }) => {
  await openApp(page, 0);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setPageBackground', color: '#ffffff', phase: 'commit' }));
  const l = await live(page);
  await newText(page, 100, 300);
  // Черновик: в документе (и в слоях) узла ещё нет, шага тоже.
  expect((await ui(page)).text.draft).toBe(true);
  expect((await ui(page)).layers.total).toBe(0);
  expect((await live(page)).steps).toBe(l.steps);
  await page.keyboard.type('Привет, мир');
  await liveWhen(page, `l.nodes === ${l.nodes + 1} && l.steps === ${l.steps + 1}`);
  expect(await copyAll(page)).toBe('Привет, мир');
  // Поле ввода не держит текста документа.
  expect(await page.locator('.fe-text-input').inputValue()).toBe('');
  await page.keyboard.press('Escape');
  await expect.poll(async () => (await ui(page)).text.editing).toBe(false);
  expect((await ui(page)).selectionCount).toBe(1);
  expect((await live(page)).steps).toBe(l.steps + 1);
  await expect(page.getByRole('treeitem', { name: 'Текст', exact: true })).toBeVisible();
  const at = await appScreen(page, await live(page), 100, 300);
  await expect.poll(() => darkPixels(page, at, 60), { timeout: 10_000 }).toBeGreaterThan(20);
});

test('стрелки, Backspace и выделение Shift — в позиции курсора @firefox', async ({ page }) => {
  await openApp(page, 0);
  await newText(page, 100, 300);
  await page.keyboard.type('abc');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.type('X');
  await page.keyboard.press('End');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Home');
  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.type('Y');
  expect(await copyAll(page)).toBe('YXb');
});

test('IME: составление — превью без шага, конец составления — текст', async ({ page }) => {
  await openApp(page, 0);
  const steps = (await live(page)).steps + 1;
  await newText(page, 100, 300);
  await page.keyboard.type('a');
  await liveWhen(page, `l.steps === ${steps}`);
  const compose = (type: string, data: string) =>
    page.evaluate(
      ({ type, data }) => document.querySelector('.fe-text-input')!.dispatchEvent(new CompositionEvent(type, { data, bubbles: true })),
      { type, data },
    );
  await compose('compositionstart', '');
  await compose('compositionupdate', 'к');
  await compose('compositionupdate', 'ко');
  await expect.poll(async () => (await ui(page)).text.editing).toBe(true);
  // Превью — не правка: шагов не прибавилось.
  await page.waitForTimeout(100);
  expect((await live(page)).steps).toBe(steps);
  await compose('compositionend', 'кот');
  await liveWhen(page, `l.steps >= ${steps}`);
  expect(await copyAll(page)).toBe('aкот');
});

test('ввод подряд — один шаг: Ctrl/Cmd+Z в правке убирает новый текст целиком', async ({ page }) => {
  await openApp(page, 0);
  const l = await live(page);
  await newText(page, 100, 300);
  await page.keyboard.type('abc');
  await liveWhen(page, `l.nodes === ${l.nodes + 1}`);
  await page.keyboard.press('ControlOrMeta+z');
  await liveWhen(page, `l.nodes === ${l.nodes} && l.canUndo === 0`);
  await expect.poll(async () => (await ui(page)).text.editing).toBe(false);
});

test('клавиши в правке не уходят командам раскладки', async ({ page }) => {
  await openApp(page, 0);
  const l = await live(page);
  await newText(page, 100, 300);
  // R — буква, а не «Прямоугольник»; Delete — не удаление узла.
  await page.keyboard.type('rr');
  await page.keyboard.press('Delete');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('Delete');
  expect((await ui(page)).tool).toBe('move');
  expect(await copyAll(page)).toBe('r');
  await page.keyboard.press('Escape');
  await expect.poll(async () => (await ui(page)).text.editing).toBe(false);
  // Вне правки Delete удаляет выделенный текст.
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes}`);
});

test('двойной щелчок по тексту — правка; щелчок мимо — выход', async ({ page }) => {
  await openApp(page, 0);
  await newText(page, 100, 300);
  await page.keyboard.type('Привет');
  await page.keyboard.press('Escape');
  await expect.poll(async () => (await ui(page)).text.editing).toBe(false);
  const empty = await appScreen(page, await live(page), 400, 600);
  await page.mouse.click(empty.x, empty.y);
  await expect.poll(async () => (await ui(page)).selectionCount).toBe(0);
  const at = await appScreen(page, await live(page), 105, 305);
  await page.mouse.dblclick(at.x, at.y);
  await expect.poll(async () => (await ui(page)).text.editing).toBe(true);
  await expect(page.locator('.fe-text-input')).toBeFocused();
  // Щелчок по тексту в правке оставляет фокус у поля ввода.
  await page.mouse.click(at.x, at.y);
  await expect(page.locator('.fe-text-input')).toBeFocused();
  await page.keyboard.type('!');
  await page.mouse.click(empty.x, empty.y);
  await expect.poll(async () => (await ui(page)).text.editing).toBe(false);
});

test.describe('буфер обмена', () => {
  test.use({ permissions: ['clipboard-read', 'clipboard-write'] });

  test('фрагмент со стилями вставляется в другой текст', async ({ page }) => {
    await openApp(page, 0);
    await newText(page, 100, 300);
    await page.keyboard.type('Жирный');
    await page.keyboard.press('ControlOrMeta+a');
    await page.keyboard.press('ControlOrMeta+b');
    await expect.poll(async () => (await ui(page)).textStyle?.style.value).toBe('Bold');
    await page.keyboard.press('ControlOrMeta+c');
    // В буфере — и обычный текст, и наш формат со стилями.
    await expect.poll(async () => page.evaluate(async () => (await navigator.clipboard.read())[0]?.types ?? [])).toContain('text/plain');
    expect(await page.evaluate(async () => (await navigator.clipboard.read())[0].types)).toContain('web application/x-ferrum');
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Жирный');
    await page.keyboard.press('Escape');
    await newText(page, 100, 500);
    await page.keyboard.press('ControlOrMeta+v');
    // Свой формат читается асинхронно: новый текст появится, когда вставка дойдёт.
    await expect.poll(async () => (await ui(page)).text.draft).toBe(false);
    expect(await copyAll(page)).toBe('Жирный');
    expect((await ui(page)).textStyle?.style.value).toBe('Bold');
  });

  test('обычный текст вне правки — новый текстовый узел', async ({ page }) => {
    await openApp(page, 0);
    const l = await live(page);
    await page.locator('canvas').focus();
    await page.evaluate(() => {
      const dt = new DataTransfer();
      dt.setData('text/plain', 'Из буфера');
      document.querySelector('canvas')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
    });
    await liveWhen(page, `l.nodes === ${l.nodes + 1} && l.selectionCount === 1`);
    expect((await ui(page)).summary.kind).toBe('text');
  });
});
