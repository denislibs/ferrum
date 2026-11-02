// «Быстрые действия» (спека подсистемы 2, раздел 7): Ctrl/Cmd+K и Ctrl/Cmd+/,
// поле сверху по центру холста, команды с сочетаниями и серыми пунктами, поиск в
// другой раскладке, слои из worker'а с переходом к слою, страницы, недавние в
// настройках. Открытие и ввод не читают раскладку вне фазы measure.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, editorReady, live, liveWhen, mainMenu, openApp } from './helpers';

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

test('Ctrl/Cmd+K и Ctrl/Cmd+/ открывают поле сверху по центру холста; Esc закрывает и возвращает фокус', async ({ page }) => {
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
  // Щелчок мимо закрывает, выделение и шаги не меняются от самой палитры.
  await openPalette(page);
  await page.locator('canvas').click({ position: { x: 10, y: 300 } });
  await expect(palette(page)).toHaveCount(0);
  expect(await warnings(page)).toEqual([]);
});

test('команды: название, группа и сочетание; серая не выполняется; Enter выполняет и закрывает', async ({ page }) => {
  await openApp(page);
  await openPalette(page);
  await field(page).fill('дубл');
  const dup = list(page).getByRole('option', { name: /Дублировать/ }).first();
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

test('стрелки ходят по строкам, в том числе по серым и по кругу; команда интерфейса выполняется', async ({ page }) => {
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

test('раскладка не важна: «щквук» находит команды порядка, «ё» и регистр — тоже', async ({ page }) => {
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

test('слои: поиск в worker\'е, переход выделяет слой, показывает его и раскрывает предков', async ({ page }) => {
  await openApp(page);
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
  expect(after.zoom).toBeGreaterThan(0);
  // Вложенный слой: предок раскрыт, строка выделена.
  await openPalette(page);
  await field(page).fill('слой 5');
  const child = list(page).getByRole('option', { name: 'Слой 5', exact: true }).first();
  await expect(child).toBeVisible();
  await child.click();
  await expect(tree.getByRole('treeitem', { name: 'Слой 5', exact: true })).toHaveAttribute('aria-selected', 'true');
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 120');
  expect(await warnings(page)).toEqual([]);
});

test('ответ worker\'а на прежний запрос не показывается', async ({ page }) => {
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

test('страницы: поиск по имени и переход', async ({ page }) => {
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

test('недавние: последние команды первыми, переживают перезагрузку; пункт меню тоже попадает', async ({ page }) => {
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

test('в поле — обычный ввод: буквы не вызывают инструменты, стрелки не двигают выделение', async ({ page }) => {
  await openApp(page);
  const before = await live(page);
  await openPalette(page);
  await page.keyboard.type('rvl');
  await expect(field(page)).toHaveValue('rvl');
  await page.keyboard.press('ArrowLeft');
  const l = await live(page);
  expect(l.steps).toBe(before.steps);
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
