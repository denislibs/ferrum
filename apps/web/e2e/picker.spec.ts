// Палитра и заливки: жест в палитре — превью и один шаг, HEX и прозрачность,
// добавить и убрать заливку, обводка, фон страницы, пипетка — только где есть
// EyeDropper.

import { expect, test, type Page } from '@playwright/test';
import { hsbToRgb, rgbToHsb, toHex } from '../src/ui/colour';
import { appScreen, expectPixel, live, liveWhen, openApp, pixel } from './helpers';

const fills = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary?.fills ?? null)));

/** Выделить «Слой 1» первого фрейма щелчком на холсте (мир (200, 40)). */
async function selectShape(page: Page): Promise<{ x: number; y: number }> {
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  return at;
}

test('жест в палитре — превью, затем один шаг; отмена возвращает цвет', async ({ page }) => {
  await openApp(page);
  const at = await selectShape(page);
  const before = await live(page);
  const start = (await fills(page)).list[0].color;
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  const picker = page.getByRole('dialog', { name: 'Цвет заливки' });
  const area = picker.getByRole('slider', { name: 'Насыщенность и яркость' });
  const box = (await area.boundingBox())!;
  // За правый верхний угол (указатель захвачен, значение упирается в край) —
  // чистый тон: насыщенность и яркость 100 %.
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width + 20, box.y - 20, { steps: 8 });
  expect((await live(page)).steps).toBe(before.steps);
  // Превью уже на холсте, документ — нет.
  await page.mouse.up();
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  const after = (await fills(page)).list[0].color;
  expect(after).not.toBe(start);
  // Тон — прежний (у «Слой 1» цвет (1, 120, 200)), насыщенность и яркость — 100 %.
  const hue = rgbToHsb({ r: 1, g: 120, b: 200 }).h;
  expect(after).toBe(toHex({ ...hsbToRgb({ h: hue, s: 1, b: 1 }), a: 255 }));
  await page.keyboard.press('Escape');
  await expect(picker).toHaveCount(0);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(async () => (await fills(page)).list[0].color).toBe(start);
  await expectPixel(page, at.x, at.y, [1, 120, 200]);
});

test('HEX и прозрачность заливки; заливку добавить и убрать', async ({ page }) => {
  await openApp(page);
  const at = await selectShape(page);
  const hex = page.getByRole('textbox', { name: 'HEX заливки' });
  await hex.fill('ff0000');
  await hex.press('Enter');
  await expect.poll(async () => (await fills(page)).list[0].color).toBe('#ff0000ff');
  await expectPixel(page, at.x, at.y, [255, 0, 0]);
  const opacity = page.getByRole('textbox', { name: 'Прозрачность заливки' });
  await opacity.fill('50');
  await opacity.press('Enter');
  await expect.poll(async () => (await fills(page)).list[0].opacity).toBe(0.5);
  // Пройти по полю Tab-ом без правки — не шаг отмены.
  const before = (await live(page)).steps;
  await opacity.focus();
  await page.keyboard.press('Tab');
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before);
  await page.getByRole('button', { name: 'Добавить заливку' }).click();
  await expect.poll(async () => (await fills(page)).list.length).toBe(2);
  await page.getByRole('button', { name: 'Удалить заливку' }).first().click();
  await expect.poll(async () => (await fills(page)).list.length).toBe(1);
  // Неверный HEX — поле возвращает прежнее значение, правки нет.
  const steps = (await live(page)).steps;
  await hex.fill('красный');
  await hex.press('Enter');
  await expect(hex).toHaveValue('FF0000');
  expect((await live(page)).steps).toBe(steps);
});

test('обводка: цвет, толщина, выравнивание', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  const strokes = () => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary!.strokes)));
  const width = page.getByRole('textbox', { name: 'Толщина обводки' });
  await width.fill('4');
  await width.press('Enter');
  await expect.poll(async () => (await strokes()).list[0].width).toBe(4);
  await page.getByRole('combobox', { name: 'Выравнивание обводки' }).selectOption('outside');
  await expect.poll(async () => (await strokes()).list[0].align).toBe('outside');
  await page.getByRole('textbox', { name: 'HEX обводки' }).fill('00ff00');
  await page.getByRole('textbox', { name: 'HEX обводки' }).press('Enter');
  await expect.poll(async () => (await strokes()).list[0].color).toBe('#00ff00ff');
});

test('фон страницы без выделения', async ({ page }) => {
  await openApp(page, 100);
  const hex = page.getByRole('textbox', { name: 'HEX фона страницы' });
  await expect(hex).toHaveValue('1A1918');
  await hex.fill('336699');
  await hex.press('Enter');
  const l = await live(page);
  const box = (await page.locator('canvas').boundingBox())!;
  await expectPixel(page, box.x + 20, box.y + l.viewportH - 20, [51, 102, 153]);
  await page.getByRole('button', { name: 'Фон как в теме' }).click();
  await expectPixel(page, box.x + 20, box.y + l.viewportH - 20, [26, 25, 24]);
  expect((await live(page)).canUndo).toBe(0);
  // Жест палитры — превью в кадре, документ не меняется до отпускания; Esc во время
  // жеста возвращает фон документа.
  const background = () => page.evaluate(() => window.__ferrum!.ui!.pages.background);
  await page.getByRole('button', { name: 'Фон страницы' }).click();
  const area = page.getByRole('dialog', { name: 'Фон страницы' }).getByRole('slider', { name: 'Насыщенность и яркость' });
  const a = (await area.boundingBox())!;
  await page.mouse.move(a.x + 5, a.y + 5);
  await page.mouse.down();
  await page.mouse.move(a.x + 10, a.y + 6, { steps: 4 });
  // Почти белый (левый верхний угол области — малая насыщенность, полная яркость).
  await expect.poll(async () => (await pixel(page, box.x + 20, box.y + l.viewportH - 20))[1]).toBeGreaterThan(220);
  expect(await background()).toBeNull();
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expectPixel(page, box.x + 20, box.y + l.viewportH - 20, [26, 25, 24]);
  expect(await background()).toBeNull();
});

test('пипетка — только если есть EyeDropper', async ({ page }) => {
  await page.addInitScript(() => {
    delete (window as unknown as { EyeDropper?: unknown }).EyeDropper;
  });
  await openApp(page);
  await selectShape(page);
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  await expect(page.getByRole('dialog', { name: 'Цвет заливки' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Пипетка' })).toHaveCount(0);
});

// ---- Уроки раундов 1–2: адресат правки, кадр, грязное поле, клавиатура ----

interface Sent {
  type: string;
  phase?: string;
  edit?: string;
  ids?: string[];
}

/** Журнал команд, ушедших ядру (`editSelection`, `setPageBackground`). */
async function recordSent(page: Page): Promise<void> {
  await page.evaluate(() => {
    const core = window.__ferrum!.core as unknown as { send(c: Record<string, unknown>): void };
    const send = core.send.bind(core);
    const w = window as unknown as { __sent: Sent[] };
    w.__sent = [];
    core.send = (c) => {
      if (c.type === 'editSelection' || c.type === 'setPageBackground') {
        w.__sent.push({
          type: c.type as string,
          phase: c.phase as string,
          edit: (c.edit as { type: string } | undefined)?.type,
          ids: c.ids as string[] | undefined,
        });
      }
      send(c);
    };
  });
}
const sent = (page: Page) => page.evaluate(() => (window as unknown as { __sent: Sent[] }).__sent);
const phases = async (page: Page, type = 'editSelection') =>
  (await sent(page)).filter((c) => c.type === type).map((c) => c.phase);
const count = (list: (string | undefined)[], phase: string) => list.filter((p) => p === phase).length;
const coreSend = (page: Page, cmd: Record<string, unknown>) =>
  page.evaluate((c) => window.__ferrum!.core.send(c as never), cmd);
const steps = async (page: Page) => (await live(page)).steps;
/** События ядра, дошедшие до интерфейса (журнал тестового режима). */
const events = (page: Page, type: string) =>
  page.evaluate((t) => window.__ferrum!.uiEvents!.filter((e) => e.type === t).length, type);

async function selectedId(page: Page): Promise<string> {
  return page.evaluate(() => window.__ferrum!.ui!.selectionIds[0]);
}

test('жест палитры: превью не чаще кадра, одна запись при отпускании', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  await recordSent(page);
  const before = await steps(page);
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  const area = page.getByRole('dialog', { name: 'Цвет заливки' }).getByRole('slider', { name: 'Насыщенность и яркость' });
  const box = (await area.boundingBox())!;
  await page.mouse.move(box.x + 10, box.y + 10);
  await page.mouse.down();
  await page.waitForTimeout(100);
  // 40 движений в одной задаче: до кадра превью уходит не больше одного.
  await area.evaluate((el, [x0, y0]) => {
    for (let i = 1; i <= 40; i++) {
      el.dispatchEvent(new PointerEvent('pointermove', { clientX: x0 + i * 2, clientY: y0 + i, bubbles: true, pointerId: 1 }));
    }
  }, [box.x + 10, box.y + 10]);
  await page.waitForTimeout(150);
  let p = await phases(page);
  expect(count(p, 'preview')).toBeGreaterThan(0);
  expect(count(p, 'preview')).toBeLessThanOrEqual(3);
  expect(count(p, 'commit')).toBe(0);
  expect(await steps(page)).toBe(before);
  await page.mouse.up();
  await liveWhen(page, `l.steps === ${before + 1}`);
  await page.waitForTimeout(150);
  p = await phases(page);
  expect(count(p, 'commit')).toBe(1);
  expect(count(p, 'cancel')).toBe(0);
  expect(await steps(page)).toBe(before + 1);
});

test('Esc во время жеста палитры — цвет на начало жеста, шага нет', async ({ page }) => {
  await openApp(page);
  const at = await selectShape(page);
  await recordSent(page);
  const before = await steps(page);
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  const picker = page.getByRole('dialog', { name: 'Цвет заливки' });
  const box = (await picker.getByRole('slider', { name: 'Тон' }).boundingBox())!;
  await page.mouse.move(box.x + 4, box.y + 6);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - 4, box.y + 6, { steps: 6 });
  await expectPixel(page, at.x, at.y, [255, 0, 0], 40).catch(() => undefined);
  await page.keyboard.press('Escape');
  // Палитра осталась открытой: Esc отменил только жест.
  await expect(picker).toBeVisible();
  await page.mouse.up();
  await expectPixel(page, at.x, at.y, [1, 120, 200]);
  const p = await phases(page);
  expect(count(p, 'commit')).toBe(0);
  expect(count(p, 'cancel')).toBe(1);
  expect(await steps(page)).toBe(before);
});

test('выделение сменилось посреди жеста: правка не попадает в новое, прежнее возвращается', async ({ page }) => {
  await openApp(page);
  const at = await selectShape(page);
  const shapeId = await selectedId(page);
  const frame = await page.evaluate(() => window.__ferrum!.ui!.layers.rows.find((r) => r.name === 'Экран 9')!.id);
  await recordSent(page);
  const before = await steps(page);
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  const area = page.getByRole('dialog', { name: 'Цвет заливки' }).getByRole('slider', { name: 'Насыщенность и яркость' });
  const box = (await area.boundingBox())!;
  await page.mouse.move(box.x + 10, box.y + 10);
  await page.mouse.down();
  await page.mouse.move(box.x + 60, box.y + 40, { steps: 5 });
  await page.waitForTimeout(100);
  expect(count(await phases(page), 'preview')).toBeGreaterThan(0);
  await coreSend(page, { type: 'select', ids: [frame], mode: 'replace' });
  await expect(page.getByRole('dialog', { name: 'Цвет заливки' })).toHaveCount(0);
  await page.mouse.move(box.x + 90, box.y + 60, { steps: 3 });
  await page.mouse.up();
  await page.waitForTimeout(200);
  const all = (await sent(page)).filter((c) => c.edit === 'fills');
  // Все правки жеста — к фигуре, с которой он начался; записи нет, отмена есть.
  expect(all.every((c) => c.ids?.length === 1 && c.ids[0] === shapeId)).toBe(true);
  expect(all.some((c) => c.phase === 'commit')).toBe(false);
  expect(all.some((c) => c.phase === 'cancel')).toBe(true);
  expect(await steps(page)).toBe(before);
  // Рамка не перекрашена, фигура — прежняя.
  expect((await fills(page)).list[0].color).toBe('#ffffffff');
  await expectPixel(page, at.x, at.y, [1, 120, 200]);
  await coreSend(page, { type: 'select', ids: [shapeId], mode: 'replace' });
  await expect.poll(async () => (await fills(page)).list[0].color).toBe('#0178c8ff');
});

test('HEX и прозрачность: то же значение — не правка; набранное не затирается; адресат — прежний', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  const shapeId = await selectedId(page);
  const frame = await page.evaluate(() => window.__ferrum!.ui!.layers.rows.find((r) => r.name === 'Экран 9')!.id);
  await recordSent(page);
  const before = await steps(page);
  const hex = page.getByRole('textbox', { name: 'HEX заливки' });
  const opacity = page.getByRole('textbox', { name: 'Прозрачность заливки' });
  await expect(hex).toHaveValue('0178C8');
  // То же значение (в другой записи) и Enter, затем уход фокуса — ни одной команды.
  await hex.fill('#0178c8');
  await hex.press('Enter');
  await hex.fill('0178C8');
  await page.keyboard.press('Tab');
  await opacity.fill('100');
  await opacity.press('Enter');
  await opacity.fill('100%');
  await page.keyboard.press('Tab');
  await page.waitForTimeout(150);
  expect(await sent(page)).toEqual([]);
  expect(await steps(page)).toBe(before);
  // Событие ядра не переписывает набранное.
  await hex.click();
  await hex.press('ControlOrMeta+a');
  await hex.pressSequentially('ab');
  await coreSend(page, {
    type: 'editSelection',
    edit: { type: 'number', field: 'x', value: 5, relative: true },
    phase: 'commit',
    ids: undefined,
    selectionVersion: undefined,
  });
  await expect.poll(() => steps(page)).toBe(before + 1);
  await expect(hex).toHaveValue('ab');
  await hex.press('Escape');
  await expect(hex).toHaveValue('0178C8');
  // Выделение сменилось, пока набирали: набранное идёт к прежней фигуре.
  await hex.click();
  await hex.press('ControlOrMeta+a');
  await hex.pressSequentially('ff0000');
  await coreSend(page, { type: 'select', ids: [frame], mode: 'replace' });
  await expect.poll(() => steps(page)).toBe(before + 2);
  const fillEdits = (await sent(page)).filter((c) => c.edit === 'fills');
  expect(fillEdits.map((c) => [c.phase, c.ids])).toEqual([['commit', [shapeId]]]);
  expect((await fills(page)).list[0].color).toBe('#ffffffff');
  await coreSend(page, { type: 'select', ids: [shapeId], mode: 'replace' });
  await expect.poll(async () => (await fills(page)).list[0].color).toBe('#ff0000ff');
});

test('поле палитры: HEX и прозрачность; то же значение — не правка', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  await recordSent(page);
  const before = await steps(page);
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  const picker = page.getByRole('dialog', { name: 'Цвет заливки' });
  const hex = picker.getByRole('textbox', { name: 'HEX', exact: true });
  await expect(hex).toHaveValue('0178C8');
  await hex.fill('0178c8');
  await hex.press('Enter');
  const op = picker.getByRole('textbox', { name: 'Прозрачность цвета' });
  await op.fill('100');
  await op.press('Enter');
  await page.waitForTimeout(150);
  expect(await sent(page)).toEqual([]);
  await hex.fill('00ff00');
  await hex.press('Enter');
  await expect.poll(async () => (await fills(page)).list[0].color).toBe('#00ff00ff');
  await op.fill('40');
  await op.press('Enter');
  await expect.poll(async () => (await fills(page)).list[0].opacity).toBe(0.4);
  expect(await steps(page)).toBe(before + 2);
});

test('фон страницы: превью только в кадре, одна запись при отпускании, то же значение — не правка', async ({ page }) => {
  await openApp(page, 100);
  await recordSent(page);
  const pagesBefore = await events(page, 'pages');
  await page.getByRole('button', { name: 'Фон страницы' }).click();
  const area = page.getByRole('dialog', { name: 'Фон страницы' }).getByRole('slider', { name: 'Насыщенность и яркость' });
  const box = (await area.boundingBox())!;
  await page.mouse.move(box.x + 20, box.y + 20);
  await page.mouse.down();
  await page.mouse.move(box.x + 120, box.y + 80, { steps: 12 });
  await page.waitForTimeout(150);
  let p = await phases(page, 'setPageBackground');
  expect(count(p, 'preview')).toBeGreaterThan(0);
  expect(count(p, 'commit')).toBe(0);
  // В документ ничего не писали: событий страниц нет, фон прежний.
  expect(await events(page, 'pages')).toBe(pagesBefore);
  expect(await page.evaluate(() => window.__ferrum!.ui!.pages.background)).toBeNull();
  await page.mouse.up();
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.pages.background)).not.toBeNull();
  p = await phases(page, 'setPageBackground');
  expect(count(p, 'commit')).toBe(1);
  expect(count(p, 'cancel')).toBe(0);
  expect(await events(page, 'pages')).toBe(pagesBefore + 1);
  expect((await live(page)).canUndo).toBe(0);
  // Закрыли и снова поставили то же самое в поле — записи нет.
  await page.keyboard.press('Escape');
  const hex = page.getByRole('textbox', { name: 'HEX фона страницы' });
  const now = await hex.inputValue();
  await hex.fill(now.toLowerCase());
  await hex.press('Enter');
  await page.waitForTimeout(150);
  expect(count(await phases(page, 'setPageBackground'), 'commit')).toBe(1);
});

test('пипетка: цвет применяется; отмена пользователем ничего не меняет', async ({ page }) => {
  await page.addInitScript(() => {
    (window as unknown as { __eye: string }).__eye = '#00ff00';
    (window as unknown as { EyeDropper: unknown }).EyeDropper = class {
      open() {
        const eye = (window as unknown as { __eye: string }).__eye;
        return eye === 'cancel'
          ? Promise.reject(new DOMException('The user canceled the selection.', 'AbortError'))
          : Promise.resolve({ sRGBHex: eye });
      }
    };
  });
  await openApp(page);
  await selectShape(page);
  const before = await steps(page);
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  const picker = page.getByRole('dialog', { name: 'Цвет заливки' });
  await page.evaluate(() => ((window as unknown as { __eye: string }).__eye = 'cancel'));
  await picker.getByRole('button', { name: 'Пипетка' }).click();
  await page.waitForTimeout(200);
  expect((await fills(page)).list[0].color).toBe('#0178c8ff');
  expect(await steps(page)).toBe(before);
  await expect(picker).toBeVisible();
  await page.evaluate(() => ((window as unknown as { __eye: string }).__eye = '#00ff00'));
  await picker.getByRole('button', { name: 'Пипетка' }).click();
  await expect.poll(async () => (await fills(page)).list[0].color).toBe('#00ff00ff');
  expect(await steps(page)).toBe(before + 1);
});

test('несколько объектов: «Смешанные» заливки и обводки; «+» заменяет одним шагом', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  const shapeId = await selectedId(page);
  const frame = await page.evaluate(() => window.__ferrum!.ui!.layers.rows.find((r) => r.name === 'Экран 9')!.id);
  await coreSend(page, { type: 'select', ids: [frame, shapeId], mode: 'replace' });
  await liveWhen(page, 'l.selectionCount === 2');
  await expect(page.getByText('Смешанные заливки')).toBeVisible();
  await expect(page.getByText('Смешанные обводки')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Цвет заливки' })).toHaveCount(0);
  await expect(page.getByRole('textbox', { name: 'HEX заливки' })).toHaveCount(0);
  const before = await steps(page);
  await page.getByRole('button', { name: 'Добавить заливку' }).click();
  await expect(page.getByText('Смешанные заливки')).toHaveCount(0);
  const s = await fills(page);
  expect(s.mixed).toBe(false);
  expect(s.list).toHaveLength(1);
  expect(await steps(page)).toBe(before + 1);
  // Отмена возвращает прежние разные заливки одним шагом.
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(page.getByText('Смешанные заливки')).toBeVisible();
});

test('заливка изображением показывается и не правится; скрыть и удалить — по шагу', async ({ page }) => {
  await openApp(page);
  await page.evaluate(async () => {
    const c = new OffscreenCanvas(4, 4);
    c.getContext('2d')!.fillRect(0, 0, 4, 4);
    const blob = await c.convertToBlob({ type: 'image/png' });
    window.__ferrum!.core.send({ type: 'pasteImage', bytes: new Uint8Array(await blob.arrayBuffer()) });
  });
  await liveWhen(page, 'l.selectionCount === 1');
  await expect(page.getByText('Изображение · Заполнить')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Цвет заливки' })).toHaveCount(0);
  await expect(page.getByRole('textbox', { name: 'HEX заливки' })).toHaveCount(0);
  const before = await steps(page);
  await page.getByRole('button', { name: 'Скрыть заливку' }).click();
  await expect.poll(async () => (await fills(page)).list[0].visible).toBe(false);
  expect(await steps(page)).toBe(before + 1);
  // Скрытая заливка изображением по-прежнему изображение.
  await expect(page.getByText('Изображение · Заполнить')).toBeVisible();
  await page.getByRole('button', { name: 'Удалить заливку' }).click();
  await expect.poll(async () => (await fills(page)).list.length).toBe(0);
  expect(await steps(page)).toBe(before + 2);
});

test('добавить, скрыть, показать, удалить заливку — по одному шагу', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  const before = await steps(page);
  await page.getByRole('button', { name: 'Добавить заливку' }).click();
  await expect.poll(async () => (await fills(page)).list.length).toBe(2);
  expect(await steps(page)).toBe(before + 1);
  await page.getByRole('button', { name: 'Скрыть заливку' }).first().click();
  await expect.poll(async () => (await fills(page)).list[1].visible).toBe(false);
  expect(await steps(page)).toBe(before + 2);
  await page.getByRole('button', { name: 'Показать заливку' }).click();
  await expect.poll(async () => (await fills(page)).list[1].visible).toBe(true);
  expect(await steps(page)).toBe(before + 3);
  await page.getByRole('button', { name: 'Удалить заливку' }).first().click();
  await expect.poll(async () => (await fills(page)).list.length).toBe(1);
  expect(await steps(page)).toBe(before + 4);
});

test('обводка: ширина жестом идёт к тем, кого правили, а не к новому выделению', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  const shapeId = await selectedId(page);
  const frame = await page.evaluate(() => window.__ferrum!.ui!.layers.rows.find((r) => r.name === 'Экран 9')!.id);
  await recordSent(page);
  const before = await steps(page);
  const label = page.getByRole('tabpanel').locator('.fe-stroke .fe-field__label', { hasText: /^W$/ });
  const box = (await label.boundingBox())!;
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 25, box.y + 5, { steps: 5 });
  await page.waitForTimeout(100);
  // Выделение сменилось посреди жеста: список обводок рамки (пустой) не должен
  // уйти к фигуре, а правка — к рамке.
  await coreSend(page, { type: 'select', ids: [frame], mode: 'replace' });
  await page.mouse.up();
  await page.waitForTimeout(200);
  const strokeEdits = (await sent(page)).filter((c) => c.edit === 'strokes');
  expect(strokeEdits.length).toBeGreaterThan(0);
  expect(strokeEdits.every((c) => c.ids?.length === 1 && c.ids[0] === shapeId)).toBe(true);
  expect(await steps(page)).toBe(before + 1);
  // Рамка без обводки осталась без неё; у фигуры ширина выросла.
  expect((await page.evaluate(() => window.__ferrum!.ui!.summary!.strokes!.list)).length).toBe(0);
  await coreSend(page, { type: 'select', ids: [shapeId], mode: 'replace' });
  await expect(page.getByRole('textbox', { name: 'Толщина обводки' })).toHaveValue('21');
});

test('палитра с клавиатуры: фокус внутрь, стрелки меняют цвет и не двигают холст, Esc возвращает фокус', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  await openApp(page);
  await selectShape(page);
  const x0 = (await page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary!.x.value)))) as number;
  const before = await steps(page);
  const swatch = page.getByRole('button', { name: 'Цвет заливки' });
  await swatch.focus();
  await page.keyboard.press('Enter');
  const picker = page.getByRole('dialog', { name: 'Цвет заливки' });
  await expect(picker).toBeVisible();
  const area = picker.getByRole('slider', { name: 'Насыщенность и яркость' });
  await expect(area).toBeFocused();
  await expect(picker.getByRole('slider')).toHaveCount(3);
  // Стрелка — шаг 1 %, Shift — 10 %, каждое нажатие — свой шаг.
  await page.keyboard.press('ArrowLeft');
  await expect.poll(() => steps(page)).toBe(before + 1);
  await page.keyboard.press('Shift+ArrowLeft');
  await expect.poll(() => steps(page)).toBe(before + 2);
  await expect(area).toHaveAttribute('aria-valuenow', /^(?!100$)\d+$/);
  const hue = picker.getByRole('slider', { name: 'Тон' });
  await hue.focus();
  const h0 = Number(await hue.getAttribute('aria-valuenow'));
  await page.keyboard.press('ArrowRight');
  expect(Number(await hue.getAttribute('aria-valuenow'))).toBe(h0 + 1);
  await expect.poll(() => steps(page)).toBe(before + 3);
  const alphaSlider = picker.getByRole('slider', { name: 'Прозрачность' });
  await alphaSlider.focus();
  await page.keyboard.press('ArrowLeft');
  await expect(alphaSlider).toHaveAttribute('aria-valuenow', '99');
  // Стрелки не дошли до холста: объект стоит на месте.
  await page.waitForTimeout(150);
  expect((await page.evaluate(() => window.__ferrum!.ui!.summary!.x.value))).toBe(x0);
  await page.keyboard.press('Escape');
  await expect(picker).toHaveCount(0);
  await expect(swatch).toBeFocused();
  // Повторное открытие и уход фокуса: палитра закрывается, фокус не отбирается.
  await page.keyboard.press('Enter');
  await expect(picker).toBeVisible();
  const xField = page.getByRole('textbox', { name: 'X', exact: true });
  await xField.focus();
  await expect(picker).toHaveCount(0);
  await expect(xField).toBeFocused();
  // Чтений layout вне фазы measure и ошибок страницы не было.
  expect(await page.evaluate(() => (window as unknown as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
  expect(errors).toEqual([]);
});
