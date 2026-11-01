// Булевы группы (подсистема 3, этап 4): Alt+Shift+U (как в Figma) одним шагом, список
// «Булевы операции» в тулбаре (новая группа и смена операции выделенной), строка
// слоя с иконкой операции и детьми, правка ребёнка с живым результатом,
// «Сплющить» группы. Документ тест читает сводкой панели свойств и строками
// слоёв, вид — пикселями экрана.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, live, liveWhen, openApp, pixel } from './helpers';

type LiveState = Awaited<ReturnType<typeof live>>;

// Чтений layout вне фазы measure планировщика не было (тестовый режим их копит).
test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

const summary = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)));
const rows = (page: Page) => page.getByRole('tree', { name: 'Слои' }).getByRole('treeitem');

/** Пустой файл на белом фоне при зуме 100 % (пиксель экрана — единица мира), фокус на холсте. */
async function blank(page: Page): Promise<LiveState> {
  await openApp(page, 0);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setPageBackground', color: '#ffffff', phase: 'commit' }));
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+Digit0');
  return liveWhen(page, 'l.zoom === 1');
}

/** Два серых квадрата 100 × 100 с перекрытием 50 по x (левый — от cameraX − 100), оба выделены. */
async function twoRects(page: Page, l: LiveState): Promise<void> {
  for (const dx of [-100, -50]) {
    await page.keyboard.press('KeyR');
    await drag(page, await appScreen(page, l, l.cameraX + dx, l.cameraY - 50), 100, 100);
    await liveWhen(page, 'l.busy === 0');
  }
  await page.keyboard.press('ControlOrMeta+a');
  await expect.poll(async () => (await summary(page))?.count).toBe(2);
}

/** Красный канал пикселя мира: 217 — серая заливка, 255 — белый фон. */
async function red(page: Page, l: LiveState, x: number, y: number): Promise<number> {
  const at = await appScreen(page, l, x, y);
  return (await pixel(page, at.x, at.y))[0];
}

test('Alt+Shift+U — объединение одним шагом, слой с детьми, отмена @firefox', async ({ page }) => {
  const l = await blank(page);
  await twoRects(page, l);
  const before = await live(page);
  await page.keyboard.press('Alt+Shift+KeyU');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  const s = await summary(page);
  expect([s.kind, s.booleanOp, s.name, s.w.value, s.h.value]).toEqual(['boolean', 'union', 'Объединение', 150, 100]);
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toHaveAccessibleName('Объединение');
  // Заливка группы — серая (стиль верхнего квадрата), перекрытие тоже серое.
  await expect.poll(() => red(page, l, l.cameraX - 25, l.cameraY)).toBe(217);
  await page.keyboard.press('ControlOrMeta+z');
  await liveWhen(page, `l.nodes === ${before.nodes}`);
  await expect(rows(page)).toHaveCount(2);
});

test('список «Булевы операции»: вычитание, затем смена операции выделенной группы', async ({ page }) => {
  const l = await blank(page);
  await twoRects(page, l);
  const toolbar = page.getByRole('toolbar', { name: 'Инструменты' });
  // Подсказка кнопки — с сочетанием из раскладки: Alt+Shift+U (⌥⇧U на маке).
  await toolbar.getByRole('button', { name: 'Объединение' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText(/^Объединение {2}.*U$/);
  await expect(page.getByRole('tooltip')).not.toHaveText(/Ctrl|⌘/);
  await toolbar.getByRole('button', { name: 'Булевы операции' }).click();
  const menu = page.getByRole('menu', { name: 'Булевы операции' });
  await expect(menu.getByRole('menuitemcheckbox')).toHaveCount(4);
  const before = await live(page);
  await menu.getByRole('menuitemcheckbox', { name: /Вычитание/ }).click();
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect.poll(async () => (await summary(page)).booleanOp).toBe('subtract');
  // Кнопка показывает операцию выделенной группы.
  await expect(toolbar.getByRole('button', { name: 'Вычитание' })).toHaveAttribute('aria-pressed', 'true');
  // Левая половина осталась, правый квадрат вычтен (белый фон).
  await expect.poll(() => red(page, l, l.cameraX - 75, l.cameraY)).toBe(217);
  await expect.poll(() => red(page, l, l.cameraX - 25, l.cameraY)).toBe(255);
  await expect.poll(() => red(page, l, l.cameraX + 25, l.cameraY)).toBe(255);
  // Та же группа, другая операция: слоёв не прибавилось.
  await toolbar.getByRole('button', { name: 'Булевы операции' }).click();
  await menu.getByRole('menuitemcheckbox', { name: /Пересечение/ }).click();
  await liveWhen(page, `l.steps === ${before.steps + 2}`);
  await expect.poll(async () => (await summary(page)).booleanOp).toBe('intersect');
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toHaveAccessibleName('Пересечение');
  await expect.poll(() => red(page, l, l.cameraX - 25, l.cameraY)).toBe(217);
  await expect.poll(() => red(page, l, l.cameraX - 75, l.cameraY)).toBe(255);
});

test('двойной щелчок по ребёнку и перетаскивание — результат меняется одним шагом @firefox', async ({ page }) => {
  const l = await blank(page);
  await twoRects(page, l);
  await page.keyboard.press('Alt+Shift+KeyI');
  await expect.poll(async () => (await summary(page))?.booleanOp).toBe('intersect');
  // Пересечение — полоса −50…0; правее нуля пусто.
  await expect.poll(() => red(page, l, l.cameraX + 25, l.cameraY)).toBe(255);
  const inside = await appScreen(page, l, l.cameraX - 25, l.cameraY);
  await page.mouse.dblclick(inside.x, inside.y);
  await expect.poll(async () => (await summary(page))?.kind).toBe('rect');
  // Пауза дольше двойного щелчка: нажатие перетаскивания — не третий щелчок.
  await page.waitForTimeout(600);
  // Верхний квадрат (−50…50) — влево на 30: пересечение −80…0.
  const before = await live(page);
  await drag(page, inside, -30, 0);
  await liveWhen(page, `l.steps === ${before.steps + 1} && l.busy === 0`);
  await expect.poll(async () => (await summary(page)).x.value).toBe(before.selectionX - 30);
  await expect.poll(() => red(page, l, l.cameraX - 65, l.cameraY)).toBe(217);
  // Отмена — одним шагом: ребёнок на месте, пересечение прежнее.
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(async () => (await summary(page)).x.value).toBe(before.selectionX);
  await expect.poll(() => red(page, l, l.cameraX - 65, l.cameraY)).toBe(255);
});

test('«Сплющить» булевой группы — один вектор с результатом', async ({ page }) => {
  const l = await blank(page);
  await twoRects(page, l);
  await page.keyboard.press('Alt+Shift+KeyE');
  await expect.poll(async () => (await summary(page))?.booleanOp).toBe('exclude');
  const before = await live(page);
  await page.keyboard.press('ControlOrMeta+e');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect.poll(async () => (await summary(page)).kind).toBe('vector');
  const s = await summary(page);
  expect([s.name, s.w.value, s.h.value]).toEqual(['Исключение', 150, 100]);
  await expect(rows(page)).toHaveCount(1);
  // Середина исключения пуста, края — серые.
  await expect.poll(() => red(page, l, l.cameraX - 25, l.cameraY)).toBe(255);
  await expect.poll(() => red(page, l, l.cameraX - 75, l.cameraY)).toBe(217);
  await expect.poll(() => red(page, l, l.cameraX + 25, l.cameraY)).toBe(217);
});
