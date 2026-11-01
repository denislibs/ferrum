// Панель свойств: правка X и отмена, выражения, перетаскивание подписи — превью
// и один шаг, Esc, «Смешанные», живые значения во время жеста на холсте,
// пресет фрейма, экспорт PNG.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

const field = (page: Page, name: string) => page.getByRole('tabpanel').getByRole('textbox', { name, exact: true });
const summary = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)));
const selectRow = (page: Page, name: string, modifiers: ('Shift' | 'ControlOrMeta')[] = []) =>
  page.getByRole('tree', { name: 'Слои' }).getByRole('treeitem', { name, exact: true }).click({ modifiers });

test('X из панели — один шаг, отмена возвращает', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const x = field(page, 'X');
  await expect(x).toHaveValue('14400');
  const before = await live(page);
  await x.fill('100');
  await x.press('Enter');
  await expect.poll(async () => (await summary(page)).x.value).toBe(100);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(x).toHaveValue('14400');
  // Выражение и проценты.
  const w = field(page, 'Ширина');
  await w.fill('1440/2+16');
  await w.press('Enter');
  await expect(w).toHaveValue('736');
  await w.fill('50%');
  await w.press('Enter');
  await expect(w).toHaveValue('368');
  // Esc откатывает набранное.
  await w.fill('9999');
  await w.press('Escape');
  await expect(w).toHaveValue('368');
});

test('перетаскивание подписи: превью, затем один шаг; Shift ×10', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const label = page.getByRole('tabpanel').locator('.fe-field__label', { hasText: /^X$/ });
  const box = (await label.boundingBox())!;
  const before = await live(page);
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 25, box.y + 5, { steps: 5 });
  // Во время жеста документ не меняется: шагов истории нет.
  expect((await live(page)).steps).toBe(before.steps);
  await page.mouse.up();
  await expect.poll(async () => (await summary(page)).x.value).toBe(14420);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await page.keyboard.down('Shift');
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 8, box.y + 5, { steps: 3 });
  await page.mouse.up();
  await page.keyboard.up('Shift');
  await expect.poll(async () => (await summary(page)).x.value).toBe(14450);
  // Esc во время жеста — без следа.
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 40, box.y + 5, { steps: 3 });
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await page.waitForTimeout(200);
  expect((await summary(page)).x.value).toBe(14450);
  expect((await live(page)).steps).toBe(before.steps + 2);
});

test('«Смешанные» и прибавка к каждому', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  await selectRow(page, 'Экран 8', ['Shift']);
  const x = field(page, 'X');
  await expect(x).toHaveValue('Смешанные');
  await expect(field(page, 'Y')).toHaveValue('0');
  await x.fill('+10');
  await x.press('Enter');
  await selectRow(page, 'Экран 8');
  await expect(x).toHaveValue('12810');
  await selectRow(page, 'Экран 9');
  await expect(x).toHaveValue('14410');
});

test('во время перетаскивания на холсте поля показывают живые значения', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  const x = field(page, 'X');
  await expect(x).toHaveValue('140');
  // Второе нажатие позже порога двойного щелчка (500 мс): двойной щелчок по
  // прямоугольнику — его правка как вектора.
  await page.waitForTimeout(600);
  await page.mouse.down();
  await page.mouse.move(at.x + 30, at.y, { steps: 6 });
  // Документ ещё не изменён, а поле уже показывает новое положение.
  await expect(x).toHaveValue('200');
  expect((await live(page)).steps).toBe(l.steps);
  await page.mouse.up();
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expect(x).toHaveValue('200');
});

test('пресет фрейма — размер одним шагом', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  await page.getByRole('button', { name: 'Пресет размера' }).click();
  await page.getByRole('menu', { name: 'Пресеты размера' }).getByRole('menuitemcheckbox', { name: /iPhone 16 Pro Max/ }).click();
  await expect(field(page, 'Ширина')).toHaveValue('440');
  await expect(field(page, 'Высота')).toHaveValue('956');
  expect((await live(page)).steps).toBe(1);
});

test('экспорт PNG выделения и страницы', async ({ page }) => {
  await openApp(page, 100);
  const button = page.getByRole('button', { name: /Экспортировать/ });
  await expect(button).toHaveText('Экспортировать страницу');
  let download = page.waitForEvent('download');
  await button.click();
  expect((await download).suggestedFilename()).toBe('Страница 1.png');
  await selectRow(page, 'Экран 0');
  await page.getByRole('radio', { name: '2×' }).click();
  download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Экспортировать «Экран 0»' }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('Экран 0@2x.png');
  const bytes = await (await file.createReadStream()).toArray();
  const png = Buffer.concat(bytes);
  // IHDR: 1440 × 1024 при 2× — 2880 × 2048.
  expect([png.readUInt32BE(16), png.readUInt32BE(20)]).toEqual([2880, 2048]);
  await expect(page.getByRole('status').filter({ hasText: 'Экспортировано: Экран 0@2x.png' })).toBeVisible();
});

test('набранное в поле не затирается событиями ядра; Esc откатывает', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const x = field(page, 'X');
  const send = (f: string, value: number) =>
    page.evaluate(
      ([field, value]) =>
        window.__ferrum!.core!.send({ type: 'editSelection', edit: { type: 'number', field, value, relative: false } as never, phase: 'commit', ids: undefined, selectionVersion: undefined }),
      [f, value] as const,
    );
  await expect(x).toHaveValue('14400');
  await x.click();
  await x.press('ControlOrMeta+a');
  await x.pressSequentially('12');
  // Ядро присылает новую сводку именно с другим X, пока X набирают.
  await send('x', 500);
  await expect.poll(async () => (await summary(page)).x.value).toBe(500);
  await expect(x).toHaveValue('12');
  await x.pressSequentially('3');
  await expect(x).toHaveValue('123');
  await send('x', 600);
  await expect.poll(async () => (await summary(page)).x.value).toBe(600);
  await expect(x).toHaveValue('123');
  // Esc отбрасывает набранное, поле показывает значение сводки.
  await x.press('Escape');
  await expect(x).toHaveValue('600');
  // Без набора поле, наоборот, следует за сводкой.
  await send('x', 700);
  await expect(x).toHaveValue('700');
});

test('стрелки в поле: шаг 1, Shift — 10, каждый — свой шаг; холст не слышит', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const x = field(page, 'X');
  const before = await live(page);
  await x.focus();
  await x.press('ArrowUp');
  await expect.poll(async () => (await summary(page)).x.value).toBe(14401);
  await x.press('Shift+ArrowUp');
  await expect.poll(async () => (await summary(page)).x.value).toBe(14411);
  await x.press('ArrowDown');
  await expect.poll(async () => (await summary(page)).x.value).toBe(14410);
  await liveWhen(page, `l.steps === ${before.steps + 3}`);
  // Остальные клавиши и сочетания не уходят на холст: выделение и документ целы.
  await x.press('Delete');
  await x.press('ControlOrMeta+a');
  await x.press('Backspace');
  await page.waitForTimeout(200);
  const after = await live(page);
  expect(after.steps).toBe(before.steps + 3);
  expect(after.selectionCount).toBe(1);
  await expect(selectRowCount(page)).resolves.toBeGreaterThan(0);
});
const selectRowCount = (page: Page) => page.getByRole('tree', { name: 'Слои' }).getByRole('treeitem').count();

test('без изменений шага нет; неверное выражение — прежнее значение без ошибок', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const x = field(page, 'X');
  const before = await live(page);
  await x.fill('14400');
  await x.press('Enter');
  await x.fill('1+');
  await x.press('Enter');
  await expect(x).toHaveValue('14400');
  await x.fill('abc');
  await x.press('Tab');
  await expect(x).toHaveValue('14400');
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before.steps);
  expect(await page.locator('[role="alert"], .fe-toast').count()).toBe(0);
  // Жест, вернувшийся к исходному значению, тоже без шага.
  const box = (await page.getByRole('tabpanel').locator('.fe-field__label', { hasText: /^X$/ }).boundingBox())!;
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 25, box.y + 5, { steps: 4 });
  await page.mouse.move(box.x + 5, box.y + 5, { steps: 4 });
  await page.mouse.up();
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before.steps);
  expect((await summary(page)).x.value).toBe(14400);
});

test('подпись захватывает указатель: жест идёт за краем окна', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const label = page.getByRole('tabpanel').locator('.fe-field__label', { hasText: /^X$/ });
  const box = (await label.boundingBox())!;
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.move(box.x + 30, box.y + 5, { steps: 3 });
  expect(await label.evaluate((el) => el.hasPointerCapture(1))).toBe(true);
  // Указатель ушёл на холст и за край окна — подпись всё ещё получает движение.
  await page.mouse.move(1279, 300, { steps: 4 });
  await page.mouse.move(box.x + 45, box.y + 5, { steps: 4 });
  await page.mouse.up();
  await expect.poll(async () => (await summary(page)).x.value).toBe(14440);
});

// ---- Раунд 1: адресат правки, «Смешанные», мелочи поля ----

const coreSend = (page: Page, cmd: unknown) => page.evaluate((c) => window.__ferrum!.core!.send(c as never), cmd);
const rowsTop = (page: Page) => page.getByRole('tree', { name: 'Слои' }).evaluate((el) => (el.scrollTop = 0));
const xOf = async (page: Page, name: string) => {
  await rowsTop(page);
  await selectRow(page, name);
  await expect.poll(async () => (await summary(page)).name).toBe(name);
  return (await summary(page)).x.value;
};

test('набранное применяется к тем, кого правили: щелчок по холсту на другой объект', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const x = field(page, 'X');
  const before = await live(page);
  await x.click();
  await x.press('ControlOrMeta+a');
  await x.pressSequentially('777');
  await coreSend(page, { type: 'execute', command: 'view.fit_all' });
  const l = await liveWhen(page, 'l.frames > 0');
  await page.waitForTimeout(300);
  const at = await appScreen(page, await live(page), 13500, 512);
  void l;
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await page.waitForTimeout(300);
  expect((await live(page)).steps).toBe(before.steps + 1);
  expect(await xOf(page, 'Экран 9')).toBe(777);
  expect(await xOf(page, 'Экран 8')).toBe(12800);
});

test('набранное применяется к тем, кого правили: смена выделения через слои и без потери фокуса', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const x = field(page, 'X');
  const before = await live(page);
  await x.click();
  await x.press('ControlOrMeta+a');
  await x.pressSequentially('555');
  // Строка слоёв: выделение сменилось, и поле уходит из фокуса.
  await selectRow(page, 'Экран 8');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  expect(await xOf(page, 'Экран 9')).toBe(555);
  expect(await xOf(page, 'Экран 8')).toBe(12800);
  // Выделение сменилось, а поле осталось в фокусе: набранное применяется сразу.
  await selectRow(page, 'Экран 9');
  await x.click();
  await x.press('ControlOrMeta+a');
  await x.pressSequentially('600');
  const ids = await page.evaluate(() => window.__ferrum!.ui!.selectionIds.slice());
  expect(ids.length).toBe(1);
  await coreSend(page, { type: 'layersWindow', start: 0, count: 40 });
  const other = await page.evaluate(
    (id) => (window.__ferrum!.ui!.layers.rows.find((r) => r.id !== id) ?? { id: '' }).id,
    ids[0],
  );
  await coreSend(page, { type: 'select', ids: [other], mode: 'replace' });
  await liveWhen(page, `l.steps === ${before.steps + 2}`);
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before.steps + 2);
  expect(await xOf(page, 'Экран 9')).toBe(600);
});

test('«Смешанные»: *2, /2 и % не ставят всем одно значение', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  await selectRow(page, 'Экран 8', ['Shift']);
  const x = field(page, 'X');
  const before = await live(page);
  await expect(x).toHaveAttribute('placeholder', 'Смешанные');
  for (const bad of ['*2', '/2', '50%']) {
    await x.fill(bad);
    await x.press('Enter');
    await expect(x).toHaveValue('');
    await expect(x).toHaveAttribute('placeholder', 'Смешанные');
  }
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before.steps);
  // Прозрачность: проценты от 100 от узла не зависят и работают.
  const o = field(page, 'Прозрачность');
  await o.fill('50%');
  await o.press('Enter');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect(o).toHaveValue('50%');
});

test('Enter при вводе через IME не применяет; другое поле с набранным применяется до жеста', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const x = field(page, 'X');
  const y = field(page, 'Y');
  const before = await live(page);
  await x.click();
  await x.press('ControlOrMeta+a');
  await x.pressSequentially('321');
  await x.evaluate((el) => el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', isComposing: true, bubbles: true, cancelable: true })));
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before.steps);
  await expect(x).toHaveValue('321');
  await x.press('Enter');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  // Набрано в Y, затем потянули подпись X: Y применяется первым.
  await y.click();
  await y.press('ControlOrMeta+a');
  await y.pressSequentially('55');
  const box = (await page.getByRole('tabpanel').locator('.fe-field__label', { hasText: /^X$/ }).boundingBox())!;
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await liveWhen(page, `l.steps === ${before.steps + 2}`);
  await page.mouse.move(box.x + 15, box.y + 5, { steps: 3 });
  await page.mouse.up();
  await liveWhen(page, `l.steps === ${before.steps + 3}`);
  expect((await summary(page)).y.value).toBe(55);
  expect((await summary(page)).x.value).toBe(331);
});

test('превью подписи — не чаще кадра и без повторов; размонтирование отменяет жест', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  await page.evaluate(() => {
    const core = window.__ferrum!.core! as unknown as { send(c: { type: string; phase?: string; edit?: { value: number } }): void };
    const send = core.send.bind(core);
    const w = window as unknown as { __prev: number[]; __cancels: number };
    w.__prev = [];
    w.__cancels = 0;
    core.send = (c) => {
      if (c.type === 'editSelection' && c.phase === 'preview') w.__prev.push(c.edit!.value);
      if (c.type === 'editSelection' && c.phase === 'cancel') w.__cancels++;
      send(c);
    };
  });
  const before = await live(page);
  const box = (await page.getByRole('tabpanel').locator('.fe-field__label', { hasText: /^X$/ }).boundingBox())!;
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  // 40 движений в одной задаче: до кадра превью уходит не больше одного.
  await page.getByRole('tabpanel').locator('.fe-field__label', { hasText: /^X$/ }).evaluate((el, x0) => {
    for (let i = 1; i <= 40; i++) {
      el.dispatchEvent(new PointerEvent('pointermove', { clientX: x0 + i, clientY: 10, bubbles: true, pointerId: 1, buttons: 1 }));
    }
  }, box.x + 5);
  await page.waitForTimeout(100);
  const prev = await page.evaluate(() => (window as unknown as { __prev: number[] }).__prev);
  expect(prev.length).toBeGreaterThan(0);
  expect(prev.length).toBeLessThanOrEqual(2);
  expect(prev.every((v, i) => i === 0 || v !== prev[i - 1])).toBe(true);
  expect(prev.at(-1)).toBe(14440);
  // Панель размонтирована (выделения нет) посреди жеста: превью снято.
  await coreSend(page, { type: 'select', ids: [], mode: 'replace' });
  await expect(field(page, 'X')).toHaveCount(0);
  await page.mouse.up();
  await page.waitForTimeout(200);
  expect(await page.evaluate(() => (window as unknown as { __cancels: number }).__cancels)).toBeGreaterThan(0);
  expect((await live(page)).steps).toBe(before.steps);
  expect(await xOf(page, 'Экран 9')).toBe(14400);
});

test('поворот: ввод, равный показанному, — не правка', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const r = field(page, 'Поворот');
  const before = await live(page);
  await coreSend(page, { type: 'editSelection', edit: { type: 'number', field: 'rotation', value: 30.004, relative: false }, phase: 'commit', ids: undefined, selectionVersion: undefined });
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect(r).toHaveValue('30°');
  await r.fill('30');
  await r.press('Enter');
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before.steps + 1);
});

// ---- Раунд 2: выделение больше предела id ----

test('выделение больше 1000: правка идёт ко всем; смена выделения откатывает набранное', async ({ page }) => {
  test.setTimeout(240_000);
  await openApp(page, 110_000);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+a');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.selectionCount)).toBeGreaterThan(1000);
  const cap = await page.evaluate(() => [window.__ferrum!.ui!.selectionCount, window.__ferrum!.ui!.selectionIds.length]);
  expect(cap[1]).toBeLessThan(cap[0]);
  const x = field(page, 'X');
  await expect(x).toHaveAttribute('placeholder', 'Смешанные');
  const before = await live(page);
  await x.click();
  await x.pressSequentially('77');
  await x.press('Enter');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  // Сводка — по всему выделению: не «Смешанные», а 77 у всех (в том числе после 1000-го).
  await expect.poll(async () => (await summary(page)).x.mixed).toBe(false);
  const s = await summary(page);
  expect([s.x.value, s.count]).toEqual([77, cap[0]]);
  // Выделение сменилось, пока набирали: набранное откатывается, шага нет.
  await x.click();
  await x.press('ControlOrMeta+a');
  await x.pressSequentially('5');
  const one = await page.evaluate(() => window.__ferrum!.ui!.selectionIds[0]);
  await coreSend(page, { type: 'select', ids: [one], mode: 'replace' });
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.selectionCount)).toBe(1);
  await page.waitForTimeout(300);
  expect((await live(page)).steps).toBe(before.steps + 1);
  await expect(x).toHaveValue('77');
  expect((await summary(page)).x.value).toBe(77);
});

// ——— Итоговая волна исправлений ———

const xLabel = (page: Page) => page.getByRole('tabpanel').locator('.fe-field__label', { hasText: /^X$/ });

test('перетаскивание подписи: потеря захвата указателя отменяет жест', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const box = (await xLabel(page).boundingBox())!;
  const before = await live(page);
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 45, box.y + 5, { steps: 4 });
  // Превью — в сцене (рамка выделения живого состояния), документ ещё прежний.
  await liveWhen(page, 'l.selectionX !== 14400');
  // Захват отняли (система, другое окно): жеста больше нет, превью снято.
  await xLabel(page).evaluate((el) => el.dispatchEvent(new PointerEvent('lostpointercapture', { pointerId: 1, bubbles: true })));
  await page.mouse.move(box.x + 90, box.y + 5, { steps: 2 });
  await page.mouse.up();
  await expect.poll(async () => (await live(page)).selectionX, { timeout: 5000 }).toBe(14400);
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before.steps);
  expect((await summary(page)).x.value).toBe(14400);
});

test('перетаскивание подписи: движение без нажатой кнопки заканчивает жест одним шагом', async ({ page }) => {
  await openApp(page);
  await selectRow(page, 'Экран 9');
  const box = (await xLabel(page).boundingBox())!;
  const before = await live(page);
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 25, box.y + 5, { steps: 4 });
  // Кнопку отпустили за окном: pointerup не пришёл, следующее движение — без кнопок.
  await xLabel(page).evaluate((el, x) => {
    el.dispatchEvent(new PointerEvent('pointermove', { pointerId: 1, isPrimary: true, buttons: 0, clientX: x, bubbles: true }));
  }, box.x + 25);
  await expect.poll(async () => (await live(page)).steps, { timeout: 5000 }).toBe(before.steps + 1);
  expect((await summary(page)).x.value).toBe(14420);
  await page.mouse.up();
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before.steps + 1);
});
