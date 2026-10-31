// Секция «Текст» панели свойств (подсистема 3, этап 2): сводка выделенных
// текстов со «Смешанными», размер полем — один шаг, выбор шрифта с поиском и
// группами, начертание, выравнивание и режим размера, правка выделенного
// диапазона в правке текста.

import { expect, test, type Page } from '@playwright/test';
import { live, liveWhen, openApp } from './helpers';

const ui = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!)));

/** Текст тестовой командой; ждёт узел (шрифт грузится асинхронно). */
async function createText(page: Page, x: number, y: number, text: string, size: number): Promise<void> {
  const before = (await live(page)).nodes;
  await page.evaluate(
    ({ x, y, text, size }) => window.__ferrum!.core!.send({ type: 'testCreateText', x, y, text, size }),
    { x, y, text, size },
  );
  await liveWhen(page, `l.nodes === ${before + 1} && l.busy === 0`);
}

const section = (page: Page) => page.getByRole('region', { name: 'Текст' });

test('сводка, размер полем одним шагом, «Смешанные» у разных', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Первый', 24);
  await expect(section(page)).toBeVisible();
  await expect(section(page).getByRole('button', { name: 'Шрифт', exact: true })).toHaveText('Inter');
  const size = section(page).getByRole('textbox', { name: 'Размер шрифта' });
  await expect(size).toHaveValue('24');
  const steps = (await live(page)).steps;
  await size.fill('40');
  await size.press('Enter');
  await liveWhen(page, `l.steps === ${steps + 1}`);
  await expect.poll(async () => (await ui(page)).textStyle.size.value).toBe(40);
  // Второй текст другого размера: вместе — «Смешанные» (поле без фокуса).
  await page.locator('canvas').focus();
  await createText(page, 0, 200, 'Второй', 12);
  const ids = (await ui(page)).layers.rows.map((r: { id: string }) => r.id);
  await page.evaluate((ids) => window.__ferrum!.core!.send({ type: 'select', ids, mode: 'replace' }), ids);
  await expect(size).toHaveValue('Смешанные');
  await page.keyboard.press('ControlOrMeta+z');
  await liveWhen(page, `l.steps === ${steps + 2}`);
});

test('выбор шрифта: поиск, группы, начертание того же имени', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Шрифт', 24);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setTextStyle', patch: { family: 'Inter', style: 'Bold' }, phase: 'commit' }));
  await expect.poll(async () => (await ui(page)).textStyle.style.value).toBe('Bold');
  await section(page).getByRole('button', { name: 'Шрифт', exact: true }).click();
  const list = section(page).getByRole('listbox', { name: 'Шрифты' });
  // Inter — и «В документе», и во «Вшитых»: обе строки отмечены текущими.
  await expect(list.getByRole('option', { name: 'Inter' })).toHaveCount(2);
  await expect(list.getByRole('option', { name: 'Inter' }).first()).toHaveAttribute('aria-selected', 'true');
  await section(page).getByRole('searchbox', { name: 'Поиск шрифта' }).fill('robo');
  await expect(list.getByRole('option')).toHaveCount(1);
  await list.getByRole('option', { name: 'Roboto' }).click();
  // Roboto качает worker ядра; начертание — то же имя (Bold).
  await expect.poll(async () => (await ui(page)).textStyle.family.value, { timeout: 10_000 }).toBe('Roboto');
  expect((await ui(page)).textStyle.style.value).toBe('Bold');
  await expect(section(page).getByRole('combobox', { name: 'Начертание' })).toHaveValue('Bold');
  // Группы списка — вшитые семейства, без поиска их десять.
  await section(page).getByRole('button', { name: 'Шрифт', exact: true }).click();
  await expect(list.getByRole('option')).not.toHaveCount(0);
});

test('выравнивание, режим размера и подчёркивание — кнопками', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Кнопки', 24);
  await section(page).getByRole('radio', { name: 'По центру' }).click();
  await expect.poll(async () => (await ui(page)).textStyle.alignH.value).toBe('center');
  await section(page).getByRole('radio', { name: 'Фиксированный размер' }).click();
  await expect.poll(async () => (await ui(page)).textStyle.sizing.value).toBe('fixed');
  await section(page).getByRole('button', { name: 'Подчёркнутый' }).click();
  await expect(section(page).getByRole('button', { name: 'Подчёркнутый' })).toHaveAttribute('aria-pressed', 'true');
  await section(page).getByRole('combobox', { name: 'Регистр' }).selectOption('upper');
  await expect.poll(async () => (await ui(page)).textStyle.case.value).toBe('upper');
});

test('в правке текста панель правит выделенный диапазон', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Диапазон', 24);
  await page.locator('canvas').focus();
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await ui(page)).text.editing).toBe(true);
  await page.keyboard.press('Home');
  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.press('Shift+ArrowRight');
  await expect.poll(async () => (await ui(page)).textStyle.range).toBe(true);
  const size = section(page).getByRole('textbox', { name: 'Размер шрифта' });
  await size.fill('48');
  await size.press('Enter');
  await expect.poll(async () => (await ui(page)).textStyle.size.value).toBe(48);
  // Правка из панели не выходит из правки текста; весь текст — «Смешанные».
  expect((await ui(page)).text.editing).toBe(true);
  await page.locator('.fe-text-input').focus();
  await page.keyboard.press('ControlOrMeta+a');
  await expect.poll(async () => (await ui(page)).textStyle.size.mixed).toBe(true);
});

/** Тянет подпись поля «Aa» влево на `dx` пикселей: жест, превью и отпускание. */
async function scrubSizeLeft(page: Page, dx: number): Promise<void> {
  const label = section(page).locator('.fe-field__label', { hasText: 'Aa' });
  const box = (await label.boundingBox())!;
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x - dx / 2, y, { steps: 5 });
  await page.mouse.move(x - dx, y, { steps: 5 });
  await page.mouse.up();
}

test('размер, протянутый ниже 1, — 1, без ошибки ядра (ядро не зажимает, секция зажимает)', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Мелко', 24);
  const steps = (await live(page)).steps;
  await scrubSizeLeft(page, 200);
  await expect.poll(async () => (await ui(page)).textStyle.size.value).toBe(1);
  await liveWhen(page, `l.steps === ${steps + 1}`);
  expect((await ui(page)).toasts).toEqual([]);
  await expect(section(page).getByRole('textbox', { name: 'Размер шрифта' })).toHaveValue('1');
});

test('размер «Смешанных», протянутый ниже 1, — 1 у всех, без ошибки ядра', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Первый', 24);
  await page.locator('canvas').focus();
  await createText(page, 0, 200, 'Второй', 12);
  const ids = (await ui(page)).layers.rows.map((r: { id: string }) => r.id);
  await page.evaluate((ids) => window.__ferrum!.core!.send({ type: 'select', ids, mode: 'replace' }), ids);
  await expect(section(page).getByRole('textbox', { name: 'Размер шрифта' })).toHaveValue('Смешанные');
  await scrubSizeLeft(page, 200);
  await expect.poll(async () => (await ui(page)).textStyle.size).toMatchObject({ value: 1, mixed: false });
  expect((await ui(page)).toasts).toEqual([]);
});
