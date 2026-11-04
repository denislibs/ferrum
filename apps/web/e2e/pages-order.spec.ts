// Порядок страниц и дубль — шаги отмены (спека подсистемы 2, раздел 10).

import { expect, test, type Page } from '@playwright/test';
import { live, liveWhen, openApp } from './helpers';

const rows = (page: Page) => page.getByRole('listbox', { name: 'Страницы' }).getByRole('option');
const selected = (page: Page) => page.getByRole('listbox', { name: 'Страницы' }).getByRole('option', { selected: true });
const names = (page: Page) => rows(page).allInnerTexts().then((list) => list.map((s) => s.trim()));

async function threePages(page: Page) {
  await openApp(page);
  const add = page.getByRole('button', { name: 'Добавить страницу' });
  await add.click();
  await add.click();
  await expect(rows(page)).toHaveCount(3);
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 2', 'Страница 3']);
}

async function dragRow(page: Page, from: number, to: number, finish = true) {
  const a = (await rows(page).nth(from).boundingBox())!;
  const b = (await rows(page).nth(to).boundingBox())!;
  await page.mouse.move(a.x + 60, a.y + a.height / 2);
  await page.mouse.down();
  // Край строки: перед строкой `to`, если тянем вверх, за ней — если вниз.
  const y = to > from ? b.y + b.height - 1 : b.y + 1;
  await page.mouse.move(a.x + 60, y, { steps: 6 });
  if (finish) await page.mouse.up();
}

test('@firefox протяжка меняет порядок одним шагом, текущая страница та же; отмена и повтор', async ({ page }) => {
  await threePages(page);
  const before = await live(page);
  const current = selected(page);
  await expect(current).toContainText('Страница 3');
  await dragRow(page, 0, 2);
  await expect.poll(() => names(page)).toEqual(['Страница 2', 'Страница 3', 'Страница 1']);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect(current).toContainText('Страница 3');
  // Щелчок, которым закончилась протяжка, не переключил страницу.
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 2', 'Страница 3']);
  await page.keyboard.press('ControlOrMeta+Shift+KeyZ');
  await expect.poll(() => names(page)).toEqual(['Страница 2', 'Страница 3', 'Страница 1']);
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

test('@firefox вверх, Esc и обратно на место — без шага; короткое нажатие переключает страницу', async ({ page }) => {
  await threePages(page);
  const before = await live(page);
  // Esc посреди жеста.
  await dragRow(page, 2, 0, false);
  await expect(page.locator('.fe-page--drop-before')).toHaveCount(1);
  await page.keyboard.press('Escape');
  await expect(page.locator('.fe-page--drop-before')).toHaveCount(0);
  await page.mouse.up();
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 2', 'Страница 3']);
  // На своё же место.
  await dragRow(page, 1, 1);
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 2', 'Страница 3']);
  expect((await live(page)).steps).toBe(before.steps);
  // Вверх.
  await dragRow(page, 2, 0);
  await expect.poll(() => names(page)).toEqual(['Страница 3', 'Страница 1', 'Страница 2']);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await rows(page).nth(1).click();
  await expect(selected(page)).toContainText('Страница 1');
});

test('@firefox порядок переживает перезагрузку; Alt+стрелка с клавиатуры', async ({ page }) => {
  await threePages(page);
  const row = selected(page);
  await row.focus();
  await page.keyboard.press('Alt+ArrowUp');
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 3', 'Страница 2']);
  // Фокус — за строкой: ещё раз вверх двигает ту же страницу.
  await page.keyboard.press('Alt+ArrowUp');
  await expect.poll(() => names(page)).toEqual(['Страница 3', 'Страница 1', 'Страница 2']);
  await page.keyboard.press('Alt+ArrowDown');
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 3', 'Страница 2']);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true);
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 3', 'Страница 2']);
});

test('@firefox «Дублировать страницу» — шаг: отмена убирает копию и возвращает назад', async ({ page }) => {
  await threePages(page);
  await rows(page).nth(0).click();
  await expect(selected(page)).toContainText('Страница 1');
  // Живое состояние обновляется кадром, позже списка: страница примера — с узлами.
  const before = await liveWhen(page, 'l.nodes > 0');
  await rows(page).nth(0).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Дублировать страницу' }).click();
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 1 (копия)', 'Страница 2', 'Страница 3']);
  await expect(selected(page)).toContainText('Страница 1 (копия)');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 2', 'Страница 3']);
  await expect(selected(page)).toContainText('Страница 1');
  await page.keyboard.press('ControlOrMeta+Shift+KeyZ');
  await expect.poll(() => names(page)).toEqual(['Страница 1', 'Страница 1 (копия)', 'Страница 2', 'Страница 3']);
  await expect(selected(page)).toContainText('Страница 1 (копия)');
  // Узлы копии на месте после повтора.
  await liveWhen(page, `l.nodes === ${before.nodes}`);
});

test('@firefox прокрутка списка посреди протяжки: место считается по текущему положению', async ({ page }) => {
  await openApp(page);
  const add = page.getByRole('button', { name: 'Добавить страницу' });
  for (let i = 0; i < 11; i++) await add.click();
  await expect(rows(page)).toHaveCount(12);
  const list = page.getByRole('listbox', { name: 'Страницы' });
  await expect.poll(() => list.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
  await list.evaluate((el) => (el.scrollTop = 0));
  const first = (await rows(page).nth(0).boundingBox())!;
  const rowH = first.height;
  await page.mouse.move(first.x + 60, first.y + rowH / 2);
  await page.mouse.down();
  await page.mouse.move(first.x + 60, first.y + rowH + 6, { steps: 4 });
  // Список прокрутили под указателем: строки сдвинулись на четыре высоты.
  await list.evaluate((el, dy) => (el.scrollTop = dy), rowH * 4);
  await expect.poll(() => list.evaluate((el) => el.scrollTop)).toBeGreaterThan(rowH * 3.5);
  // Строка, стоящая теперь под указателем (по её видимому положению), — за её нижним краем.
  const target = 6;
  const b = (await rows(page).nth(target).boundingBox())!;
  await page.mouse.move(first.x + 60, b.y + b.height - 1, { steps: 4 });
  await page.mouse.up();
  await expect.poll(() => names(page).then((n) => n.indexOf('Страница 1'))).toBe(target);
});

test('@firefox протяжка страницы не выделяет текст имён', async ({ page }) => {
  await threePages(page);
  await dragRow(page, 0, 2, false);
  expect(await page.evaluate(() => getSelection()?.toString() ?? '')).toBe('');
  await page.mouse.up();
  expect(await page.evaluate(() => getSelection()?.toString() ?? '')).toBe('');
});
