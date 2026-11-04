// «Быстрые действия» (спека подсистемы 2, раздел 7): Ctrl/Cmd+K и Ctrl/Cmd+/,
// поле сверху по центру холста, команды с сочетаниями и серыми пунктами, поиск в
// другой раскладке, слои из worker'а с переходом к слою, страницы, недавние в
// настройках. Открытие и ввод не читают раскладку вне фазы measure.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, editorReady, live, liveWhen, mainMenu, openApp, screen } from './helpers';

const palette = (page: Page) => page.getByRole('dialog', { name: 'Быстрые действия' });
const field = (page: Page) => page.getByRole('combobox', { name: 'Быстрые действия' });
const list = (page: Page) => palette(page).getByRole('listbox');
const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);
const stored = (page: Page) => page.evaluate(() => window.__ferrumFiles!.store.settings());

async function openPalette(page: Page, key = 'ControlOrMeta+KeyK'): Promise<void> {
  await page.locator('canvas').focus();
  await page.keyboard.press(key);
  await expect(field(page)).toBeFocused();
}

/** Выбранная строка списка (`aria-activedescendant`). */
const active = (page: Page) => list(page).locator('[role="option"][aria-selected="true"]');

test('Ctrl/Cmd+K и Ctrl/Cmd+/ открывают поле сверху по центру холста; Esc закрывает и возвращает фокус @firefox', async ({ page }) => {
  await openApp(page);
  for (const key of ['ControlOrMeta+KeyK', 'ControlOrMeta+Slash']) {
    await openPalette(page, key);
    const box = (await palette(page).boundingBox())!;
    const canvas = (await page.locator('canvas').boundingBox())!;
    expect(Math.abs(box.x + box.width / 2 - (canvas.x + canvas.width / 2))).toBeLessThanOrEqual(1);
    expect(box.y - canvas.y).toBeLessThanOrEqual(24);
    await page.keyboard.press('Escape');
    await expect(palette(page)).toHaveCount(0);
    await expect(page.locator('canvas')).toBeFocused();
  }
  // Сочетание в самом поле закрывает палитру.
  await openPalette(page);
  await page.keyboard.press('ControlOrMeta+KeyK');
  await expect(palette(page)).toHaveCount(0);
  // Щелчок мимо закрывает; ни шаги, ни выделение от самой палитры не меняются.
  const before = await live(page);
  await openPalette(page);
  await page.locator('canvas').click({ position: { x: 10, y: 300 } });
  await expect(palette(page)).toHaveCount(0);
  const after = await live(page);
  expect(after.steps).toBe(before.steps);
  expect(after.selectionCount).toBe(before.selectionCount);
  expect(await warnings(page)).toEqual([]);
});

test('Tab закрывает палитру без возврата фокуса на холст @firefox', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await page.keyboard.press('Tab');
  await expect(palette(page)).toHaveCount(0);
  await expect(page.locator('canvas')).not.toBeFocused();
});

test('щелчок мимо закрывает палитру, фокус остаётся там, куда щёлкнули @firefox', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  const box = (await page.getByRole('tree', { name: 'Слои' }).boundingBox())!;
  await page.mouse.click(box.x + 4, box.y + box.height - 4);
  await expect(palette(page)).toHaveCount(0);
  await expect(page.locator('canvas')).not.toBeFocused();
});

test('IME: Enter и Esc составления текста не выполняют и не закрывают палитру @firefox', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('настройки');
  await field(page).dispatchEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 229, isComposing: true });
  await field(page).dispatchEvent('keydown', { key: 'Escape', code: 'Escape', keyCode: 229, isComposing: true });
  await expect(palette(page)).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Настройки' })).toHaveCount(0);
});

test('щелчок внутри окна палитры оставляет фокус в поле; Delete не удаляет выделение @firefox', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  const sel = await liveWhen(page, 'l.selectionCount === 1');
  await openPalette(page);
  for (const where of ['.fe-palette__field svg', '.fe-palette__field', '.fe-palette__heading']) {
    await palette(page).locator(where).first().click();
    await expect(field(page)).toBeFocused();
  }
  await palette(page).click({ position: { x: 2, y: 2 } });
  await expect(field(page)).toBeFocused();
  await page.keyboard.press('Delete');
  await page.keyboard.press('KeyR');
  const now = await live(page);
  expect(now.steps).toBe(sel.steps);
  expect(now.selectionCount).toBe(1);
  expect(await page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('move');
  await expect(palette(page)).toBeVisible();
});

test('фокус ушёл из поля: Esc и сочетание палитры всё равно закрывают, клавиши холст не получает @firefox', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  const sel = await liveWhen(page, 'l.selectionCount === 1');
  for (const key of ['Escape', 'ControlOrMeta+KeyK']) {
    await openPalette(page);
    await page.evaluate(() => (document.activeElement as HTMLElement).blur());
    await page.keyboard.press('Delete');
    expect((await live(page)).steps).toBe(sel.steps);
    await page.keyboard.press(key);
    await expect(palette(page)).toHaveCount(0);
    expect((await live(page)).steps).toBe(sel.steps);
  }
});

test('удержание Enter выполняет команду один раз; удержание сочетания не мигает окном @firefox', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  const sel = await liveWhen(page, 'l.selectionCount === 1');
  await openPalette(page);
  await field(page).fill('дубл');
  await field(page).dispatchEvent('keydown', { key: 'Enter', code: 'Enter', repeat: true });
  await expect(palette(page)).toBeVisible();
  expect((await live(page)).steps).toBe(sel.steps);
  await page.keyboard.press('Escape');
  // Удержание Ctrl/Cmd+K: открыто и остаётся открытым.
  await page.locator('canvas').focus();
  await page.keyboard.down('ControlOrMeta');
  await page.keyboard.down('KeyK');
  await expect(field(page)).toBeFocused();
  await page.keyboard.down('KeyK');
  await page.keyboard.down('KeyK');
  await page.keyboard.up('KeyK');
  await page.keyboard.up('ControlOrMeta');
  await expect(palette(page)).toBeVisible();
  await page.keyboard.press('Escape');
  // То же для панели «Сочетания клавиш».
  await page.locator('canvas').focus();
  await page.keyboard.down('Control');
  await page.keyboard.down('Shift');
  await page.keyboard.down('Slash');
  const keys = page.getByRole('dialog', { name: 'Сочетания клавиш' });
  await expect(keys).toBeVisible();
  await page.keyboard.down('Slash');
  await page.keyboard.down('Slash');
  await page.keyboard.up('Slash');
  await page.keyboard.up('Shift');
  await page.keyboard.up('Control');
  await expect(keys).toBeVisible();
});

test('Ctrl/Cmd+K из поля панели открывает палитру; в модальном диалоге — нет @firefox', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  const x = page.getByRole('textbox', { name: 'X', exact: true });
  await x.focus();
  // Слушатель позже ядра: видит, отменил ли кто-то действие браузера (адресная строка).
  await page.evaluate(() => {
    const w = window as unknown as { __kPrevented: boolean[] };
    w.__kPrevented = [];
    window.addEventListener('keydown', (e) => e.code === 'KeyK' && w.__kPrevented.push(e.defaultPrevented));
  });
  await page.keyboard.press('ControlOrMeta+KeyK');
  await expect(field(page)).toBeFocused();
  expect(await page.evaluate(() => (window as unknown as { __kPrevented: boolean[] }).__kPrevented)).toEqual([true]);
  await page.keyboard.press('Escape');
  await expect(x).toBeFocused();
  // Модальный диалог: окно не открывается.
  await openPalette(page);
  await field(page).fill('настройки');
  await page.keyboard.press('Enter');
  const settings = page.getByRole('dialog', { name: 'Настройки' });
  await expect(settings).toBeVisible();
  await page.keyboard.press('ControlOrMeta+KeyK');
  await expect(palette(page)).toHaveCount(0);
});

test('привязка окна к одной букве: в полях буква набирается, окно не открывается; на холсте открывает @firefox', async ({ page }) => {
  await openApp(page);
  await page.evaluate(() => {
    window.__ferrum!.core!.send({ type: 'bind', command: 'ui.quick_actions', shortcut: 'KeyJ', replace: false });
    window.__ferrum!.core!.send({ type: 'bind', command: 'ui.shortcuts', shortcut: 'KeyU', replace: false });
  });
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  // Поле панели: буква не отменяется и ничего не открывает.
  const x = page.getByRole('textbox', { name: 'X', exact: true });
  await x.focus();
  await page.evaluate(() => {
    const w = window as unknown as { __prevented: boolean[] };
    w.__prevented = [];
    window.addEventListener('keydown', (e) => (e.code === 'KeyJ' || e.code === 'KeyU') && w.__prevented.push(e.defaultPrevented));
  });
  await page.keyboard.press('KeyJ');
  await page.keyboard.press('KeyU');
  await expect(palette(page)).toHaveCount(0);
  await expect(page.getByRole('dialog', { name: 'Сочетания клавиш' })).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as { __prevented: boolean[] }).__prevented)).toEqual([false, false]);
  // Поле самой палитры: буквы набираются, окно не закрывается и панель не открывается.
  await openPalette(page);
  await page.keyboard.type('juj');
  await expect(field(page)).toHaveValue('juj');
  await expect(palette(page)).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Сочетания клавиш' })).toHaveCount(0);
  await page.keyboard.press('Escape');
  // На холсте та же буква открывает окно.
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyJ');
  await expect(field(page)).toBeFocused();
  await page.keyboard.press('Escape');
  await page.keyboard.press('KeyU');
  await expect(page.getByRole('dialog', { name: 'Сочетания клавиш' })).toBeVisible();
});

test('команды: название, группа и сочетание; серая не выполняется; Enter выполняет и закрывает @firefox', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('дубл');
  const dup = list(page)
    .getByRole('option', { name: /Дублировать/ })
    .first();
  await expect(dup).toHaveAttribute('data-command', 'edit.duplicate');
  await expect(dup).toContainText('Правка');
  await expect(dup).toContainText(/D/);
  // Ничего не выделено — серая: Enter и щелчок не выполняют, палитра открыта.
  await expect(dup).toHaveAttribute('aria-disabled', 'true');
  const before = await live(page);
  await page.keyboard.press('Enter');
  await dup.click({ force: true });
  await expect(palette(page)).toBeVisible();
  expect((await live(page)).steps).toBe(before.steps);
  await page.keyboard.press('Escape');

  // Выделяем слой — та же команда доступна и даёт один шаг.
  const at = await appScreen(page, before, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await openPalette(page);
  await field(page).fill('дубл');
  await expect(active(page)).toHaveAttribute('data-command', 'edit.duplicate');
  await expect(active(page)).not.toHaveAttribute('aria-disabled', 'true');
  await page.keyboard.press('Enter');
  await expect(palette(page)).toHaveCount(0);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
});

test('стрелки ходят по строкам, в том числе по серым и по кругу; команда интерфейса выполняется @firefox', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('настройки');
  const opts = list(page).getByRole('option');
  await expect(opts.first()).toHaveAttribute('aria-selected', 'true');
  await expect(active(page)).toHaveAttribute('data-command', 'ui.settings');
  await page.keyboard.press('ArrowUp');
  await expect(opts.last()).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('ArrowDown');
  await expect(opts.first()).toHaveAttribute('aria-selected', 'true');
  const activeId = await field(page).getAttribute('aria-activedescendant');
  expect(activeId).toBe(await opts.first().getAttribute('id'));
  await page.keyboard.press('Enter');
  await expect(page.getByRole('dialog', { name: 'Настройки' })).toBeVisible();
  await expect(palette(page)).toHaveCount(0);
  await page.keyboard.press('Escape');
  // Длинный список: выбранная строка прокручивается в видимую часть.
  await openPalette(page);
  for (let i = 0; i < 25; i++) await page.keyboard.press('ArrowDown');
  await expect(active(page)).toBeInViewport({ ratio: 1 });
  expect(await warnings(page)).toEqual([]);
});

test('раскладка не важна: «щквук» находит команды порядка, «ё» и регистр — тоже @firefox', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('щквук');
  await expect(list(page).getByRole('option', { name: /На передний план/ })).toBeVisible();
  await field(page).fill('ПЕРЕДНИЙ');
  await expect(list(page).getByRole('option', { name: /На передний план/ })).toBeVisible();
  await field(page).fill('темная');
  await expect(list(page).getByRole('option', { name: /Тёмная тема/ })).toBeVisible();
  await field(page).fill('нет такой команды');
  await expect(palette(page).getByText('Ничего не найдено')).toBeVisible();
  expect(await warnings(page)).toEqual([]);
});

test("слои: поиск в worker'е, переход выделяет слой, показывает его и раскрывает предков @firefox", async ({ page }) => {
  await openApp(page);
  const start = await live(page);
  await openPalette(page);
  // Фрейм «Экран 9» на странице-примере; ищем в другой раскладке.
  await field(page).fill("'rhfy 9");
  const row = list(page).getByRole('option', { name: 'Экран 9' });
  await expect(row).toBeVisible();
  await expect(palette(page).getByText('Слои', { exact: true })).toBeVisible();
  await row.click();
  await expect(palette(page)).toHaveCount(0);
  const after = await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 1440');
  // Фрейм в виде целиком (1440×1024 — вписан, если не влезал).
  const tree = page.getByRole('tree', { name: 'Слои' });
  await expect(tree.getByRole('treeitem', { name: 'Экран 9', exact: true })).toHaveAttribute('aria-selected', 'true');
  // Вид сменился, и фрейм виден целиком в окне холста.
  expect([after.cameraX, after.cameraY, after.zoom]).not.toEqual([start.cameraX, start.cameraY, start.zoom]);
  const inView = (l: typeof after) => {
    const a = screen(l, l.selectionX, l.selectionY);
    const b = screen(l, l.selectionX + l.selectionW, l.selectionY + l.selectionH);
    return a.x >= 0 && a.y >= 0 && b.x <= l.viewportW && b.y <= l.viewportH;
  };
  expect(inView(after)).toBe(true);
  // Вложенный слой: предок раскрыт, строка выделена.
  await openPalette(page);
  await field(page).fill('слой 5');
  const child = list(page).getByRole('option', { name: 'Слой 5', exact: true }).first();
  await expect(child).toBeVisible();
  await child.click();
  await expect(tree.getByRole('treeitem', { name: 'Слой 5', exact: true })).toHaveAttribute('aria-selected', 'true');
  const nested = await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 120');
  // Координаты вложенного слоя — в системе предка (фрейма): к миру — сдвигом фрейма.
  expect(inView({ ...nested, selectionX: after.selectionX + nested.selectionX, selectionY: after.selectionY + nested.selectionY })).toBe(
    true,
  );
  expect(await warnings(page)).toEqual([]);
});

test("ответ worker'а на прежний запрос не показывается @firefox", async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('слой 42');
  await expect(list(page).getByRole('option', { name: 'Слой 42', exact: true }).first()).toBeVisible();
  // Новый запрос, а ядро ещё раз отвечает на старый (ответ пришёл после нового).
  await field(page).fill('экран 1');
  await expect(list(page).getByRole('option', { name: 'Экран 1', exact: true })).toBeVisible();
  const answers = () => page.evaluate(() => window.__ferrum!.uiEvents!.filter((e) => e.type === 'layersFound').length);
  const was = await answers();
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'findLayers', query: 'слой 42', limit: 50 }));
  await expect.poll(answers).toBe(was + 1);
  await page.waitForTimeout(100);
  await expect(list(page).getByRole('option', { name: 'Слой 42', exact: true })).toHaveCount(0);
  await expect(list(page).getByRole('option', { name: 'Экран 1', exact: true })).toBeVisible();
});

test('страницы: поиск по имени и переход @firefox', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Добавить страницу' }).click();
  await page.waitForFunction(() => window.__ferrum!.ui!.pages.list.length === 2);
  const first = await page.evaluate(() => window.__ferrum!.ui!.pages.list[0]);
  const second = await page.evaluate(() => window.__ferrum!.ui!.pages.current);
  expect(second).not.toBe(first.id);
  await openPalette(page);
  await field(page).fill(first.name);
  await expect(palette(page).getByText('Страницы', { exact: true })).toBeVisible();
  await list(page).getByRole('option', { name: first.name, exact: true }).last().click();
  await expect(palette(page)).toHaveCount(0);
  await page.waitForFunction((id) => window.__ferrum!.ui!.pages.current === id, first.id);
});

test('недавние: последние команды первыми, переживают перезагрузку; пункт меню тоже попадает @firefox', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await expect(palette(page).getByText('Недавние', { exact: true })).toHaveCount(0);
  await field(page).fill('показать все');
  await expect(active(page)).toHaveAttribute('data-command', 'view.fit_all');
  await page.keyboard.press('Enter');
  await openPalette(page);
  await field(page).fill('масштаб 100');
  await expect(active(page)).toHaveAttribute('data-command', 'view.zoom_100');
  await page.keyboard.press('Enter');
  await openPalette(page);
  await expect(palette(page).getByText('Недавние', { exact: true })).toBeVisible();
  const opts = list(page).getByRole('option');
  await expect(opts.nth(0)).toHaveAttribute('data-command', 'view.zoom_100');
  await expect(opts.nth(1)).toHaveAttribute('data-command', 'view.fit_all');
  await page.keyboard.press('Escape');
  await expect.poll(() => stored(page)).toMatchObject({ recentCommands: 'view.zoom_100 view.fit_all' });
  await page.reload();
  await editorReady(page);
  await openPalette(page);
  await expect(list(page).getByRole('option').nth(0)).toHaveAttribute('data-command', 'view.zoom_100');
  // Недавняя команда выше при поиске: «показать» — и «Показать всё» первой.
  await field(page).fill('показать');
  await expect(active(page)).toHaveAttribute('data-command', 'view.fit_all');
  await page.keyboard.press('Escape');
  // Команда из главного меню — тоже в недавних.
  await mainMenu(page, 'Правка', 'Выделить всё');
  await openPalette(page);
  await expect(list(page).getByRole('option').nth(0)).toHaveAttribute('data-command', 'edit.select_all');
});

test('кнопки панелей (масштаб, тулбар) недавних не пополняют: только палитра и меню @firefox', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Масштаб', exact: true }).click();
  await page
    .getByRole('menu', { name: 'Масштаб' })
    .getByRole('menuitem', { name: /Показать всё/ })
    .click();
  await page.getByRole('button', { name: 'Инструменты перемещения' }).click();
  await page.getByRole('menu', { name: 'Инструменты перемещения' }).getByRole('menuitemcheckbox').last().click();
  await openPalette(page);
  await expect(palette(page).getByText('Недавние', { exact: true })).toHaveCount(0);
  expect((await stored(page)).recentCommands ?? '').toBe('');
  await page.keyboard.press('Escape');
  // Главное меню пополняет.
  await mainMenu(page, 'Вид', 'Масштаб 100%');
  await openPalette(page);
  await expect(list(page).getByRole('option').nth(0)).toHaveAttribute('data-command', 'view.zoom_100');
});

test('новый запрос сразу убирает слои прежнего: Enter не переходит к ним @firefox', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('слой 42');
  await expect(list(page).getByRole('option', { name: 'Слой 42', exact: true }).first()).toBeVisible();
  // Следующий запрос, пока worker не ответил (ответ — отдельной задачей позже).
  const state = await page.evaluate(() => {
    const input = document.querySelector<HTMLInputElement>('[role="combobox"]')!;
    input.value = 'экран 1';
    input.dispatchEvent(new InputEvent('input', { bubbles: true }));
    return {
      names: [...document.querySelectorAll('[role="option"]')].map((o) => o.textContent),
      empty: document.body.textContent!.includes('Ничего не найдено'),
    };
  });
  expect(state.names.join('|')).not.toContain('Слой 42');
  expect(state.empty).toBe(false);
  await expect(list(page).getByRole('option', { name: 'Экран 1', exact: true })).toBeVisible();
});

test('поздний ответ на текущий запрос не сбрасывает выбранную стрелками строку @firefox', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('слой');
  await expect(
    list(page)
      .getByRole('option', { name: /^Слой \d+$/ })
      .first(),
  ).toBeVisible();
  for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowDown');
  const picked = async () => ({ id: await active(page).getAttribute('id'), text: await active(page).textContent() });
  const before = await picked();
  expect(before.id).not.toBe('fe-palette-option-0');
  const answers = () => page.evaluate(() => window.__ferrum!.uiEvents!.filter((e) => e.type === 'layersFound').length);
  const was = await answers();
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'findLayers', query: 'слой', limit: 50 }));
  await expect.poll(answers).toBe(was + 1);
  await page.waitForTimeout(100);
  expect(await picked()).toEqual(before);
  // Новый текст запроса — снова первая строка.
  await field(page).fill('слои');
  await expect(list(page).getByRole('option').first()).toBeVisible();
  await expect
    .poll(async () => (await picked()).id)
    .toBe(await list(page).locator('[role="option"]:not([aria-disabled="true"])').first().getAttribute('id'));
});

test('в поле — обычный ввод: буквы не вызывают инструменты, стрелки не двигают выделение @firefox', async ({ page }) => {
  await openApp(page);
  const at = await appScreen(page, await live(page), 200, 40);
  await page.mouse.click(at.x, at.y);
  const before = await liveWhen(page, 'l.selectionCount === 1');
  await openPalette(page);
  await page.keyboard.type('rvl');
  await expect(field(page)).toHaveValue('rvl');
  for (const k of ['ArrowLeft', 'ArrowUp', 'ArrowDown', 'ArrowRight']) await page.keyboard.press(k);
  const l = await live(page);
  expect(l.steps).toBe(before.steps);
  expect([l.selectionX, l.selectionY]).toEqual([before.selectionX, before.selectionY]);
  expect(await page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('move');
});

test('ввод в палитру на 20 000 узлов: без длинных задач и чтений раскладки вне measure @perf', async ({ page }, info) => {
  test.skip(info.project.name !== 'chromium', 'длинные задачи — только в Chromium');
  await openApp(page, 20_000);
  await page.evaluate(() => {
    const w = window as unknown as { __long: number[] };
    w.__long = [];
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) w.__long.push(e.duration);
    }).observe({ type: 'longtask', buffered: false });
  });
  await openPalette(page);
  for (const c of 'слой 42') await page.keyboard.type(c);
  await expect(list(page).getByRole('option', { name: 'Слой 42', exact: true }).first()).toBeVisible();
  for (let i = 0; i < 30; i++) await page.keyboard.press('ArrowDown');
  await page.waitForTimeout(100);
  expect(await page.evaluate(() => (window as unknown as { __long: number[] }).__long)).toEqual([]);
  expect(await warnings(page)).toEqual([]);
});
