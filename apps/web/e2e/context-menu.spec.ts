// Контекстные меню (спека подсистемы 2, раздел 5): дерево и выбор на правой
// кнопке — из ядра, меню открывается у точки щелчка; основной поток не читает
// раскладку вне фазы measure (`__ferrumWarnings` пуст).

import { expect, test, type Page } from '@playwright/test';
import { appScreen, fileId, live, liveWhen, openApp } from './helpers';

const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

/** Названия пунктов открытого меню (сначала ждём, пока оно встанет на место). */
async function labels(page: Page, name: string): Promise<(string | undefined)[]> {
  const items = page.getByRole('menu', { name, exact: true }).locator(':scope > [role^="menuitem"]');
  await expect(items.first()).toBeVisible();
  return items.evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.label));
}

test('холст: правый щелчок выделяет узел, меню у точки, «Выделить слой» — узлы под точкой', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  expect(l.selectionCount).toBe(0);
  // Мир (200, 40) — «Слой 1» в «Экран 0».
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu).toBeVisible();
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 120');
  // Меню — у точки щелчка; длинное поднимается, чтобы целиком влезть в окно.
  // Рамку меряем после анимации появления: она сдвигает меню на 2 px.
  await menu.evaluate((el) => Promise.all(el.getAnimations().map((a) => a.finished)));
  const box = (await menu.boundingBox())!;
  expect(Math.abs(box.x - node.x)).toBeLessThanOrEqual(1);
  expect(box.y).toBeGreaterThanOrEqual(8);
  expect(box.y + box.height).toBeLessThanOrEqual(720 - 8);
  expect(await labels(page, 'Действия с выделением')).toEqual([
    'Копировать',
    'Вставить',
    'Вставить поверх',
    'Вставить с заменой',
    'Копировать как',
    'Копировать свойства',
    'Вставить свойства',
    'Выделить слой',
    'Переместить вперёд',
    'Переместить назад',
    'На передний план',
    'На задний план',
    'Сгруппировать',
    'Разгруппировать',
    'Обернуть во фрейм',
    'Булевы операции',
    'Сплющить',
    'Контур обводки',
    'Отразить по горизонтали',
    'Отразить по вертикали',
    'Скрыть или показать',
    'Заблокировать или разблокировать',
    'Войти в правку',
    'Удалить',
  ]);
  await menu.getByRole('menuitem', { name: 'Выделить слой' }).click();
  const under = page.getByRole('menu', { name: 'Выделить слой' });
  await expect(under.getByRole('menuitem', { name: 'Экран 0' })).toBeVisible();
  expect(await labels(page, 'Выделить слой')).toEqual(['Слой 1', 'Экран 0']);
  await under.getByRole('menuitem', { name: 'Экран 0' }).click();
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 1440');
  await expect(menu).toHaveCount(0);
  expect(await warnings(page)).toEqual([]);
});

test('холст: правый щелчок по выделенному сохраняет выделение, команда из меню — шаг', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const a = await appScreen(page, l, 200, 40);
  const b = await appScreen(page, l, 340, 40);
  await page.mouse.click(a.x, a.y);
  await page.keyboard.down('Shift');
  await page.mouse.click(b.x, b.y);
  await page.keyboard.up('Shift');
  await liveWhen(page, 'l.selectionCount === 2');
  await page.mouse.click(b.x, b.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await menu.getByRole('menuitem', { name: /^Сгруппировать/ }).click();
  const after = await liveWhen(page, `l.steps === ${l.steps + 1}`);
  expect(after.selectionCount).toBe(1);
  expect(after.selectionW).toBe(260);
});

test('пустое место холста: выделение снимается, меню — вставить, вставить поверх, выделить всё, показать всё', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y);
  await liveWhen(page, 'l.selectionCount === 1');
  // Мир (100, 1080) — между фреймами.
  const empty = await appScreen(page, l, 100, 1080);
  await page.mouse.click(empty.x, empty.y, { button: 'right' });
  await liveWhen(page, 'l.selectionCount === 0');
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu).toBeVisible();
  // Короткое меню — у точки щелчка: под ней или, у низа окна, над ней (отступ 4 px).
  const box = (await menu.boundingBox())!;
  expect(Math.abs(box.x - empty.x)).toBeLessThanOrEqual(1);
  expect(Math.min(Math.abs(box.y - (empty.y + 4)), Math.abs(box.y + box.height - (empty.y - 4)))).toBeLessThanOrEqual(2);
  expect(await labels(page, 'Действия с выделением')).toEqual(['Вставить', 'Вставить поверх', 'Выделить всё', 'Показать всё', 'Линейки']);
  await menu.getByRole('menuitem', { name: /^Выделить всё/ }).click();
  await liveWhen(page, 'l.selectionCount === 10');
  expect(await warnings(page)).toEqual([]);
});

test('слои: правый щелчок выделяет строку; «Развернуть всё», «Свернуть всё», «Переименовать»', async ({ page }) => {
  await openApp(page);
  const tree = page.getByRole('tree', { name: 'Слои' });
  const row = tree.getByRole('treeitem', { name: 'Экран 3', exact: true });
  await row.click({ button: 'right' });
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 1440');
  await expect(row).toHaveAttribute('aria-selected', 'true');
  const menu = page.getByRole('menu', { name: 'Действия со слоем' });
  await expect(menu).toBeVisible();
  const items = await labels(page, 'Действия со слоем');
  expect(items.slice(-4)).toEqual(['Переименовать', 'Развернуть всё', 'Свернуть всё', 'Удалить']);
  expect(items).not.toContain('Выделить слой');
  await menu.getByRole('menuitem', { name: 'Развернуть всё' }).click();
  // Все 10 фреймов раскрыты: 10 + 990 строк.
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(1000);
  await tree.getByRole('treeitem').first().click({ button: 'right' });
  await menu.getByRole('menuitem', { name: 'Свернуть всё' }).click();
  await expect(row).toHaveAttribute('aria-expanded', 'false');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(10);
  await row.click({ button: 'right' });
  await menu.getByRole('menuitem', { name: 'Переименовать' }).click();
  const input = page.getByRole('textbox', { name: 'Имя слоя' });
  await expect(input).toBeFocused();
  await input.fill('Главный');
  await input.press('Enter');
  await expect(tree.getByRole('treeitem', { name: 'Главный', exact: true })).toBeVisible();
  expect(await warnings(page)).toEqual([]);
});

test('страницы: удалить единственную нельзя, «Дублировать», ссылка на страницу открывает её', async ({ page, browserName }) => {
  if (browserName === 'chromium') await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await openApp(page);
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  const first = pages.getByRole('option').first();
  await first.click({ button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия со страницей' });
  expect(await labels(page, 'Действия со страницей')).toEqual([
    'Переименовать',
    'Дублировать страницу',
    'Удалить страницу',
    'Копировать ссылку на страницу',
  ]);
  await expect(menu.getByRole('menuitem', { name: 'Удалить страницу' })).toHaveAttribute('aria-disabled', 'true');
  await menu.getByRole('menuitem', { name: 'Дублировать страницу' }).click();
  const copy = pages.getByRole('option', { name: /Страница 1 \(копия\)/ });
  await expect(copy).toHaveAttribute('aria-selected', 'true');
  await expect(pages.getByRole('option')).toHaveCount(2);
  await liveWhen(page, 'l.nodes === 1000');
  // Ссылка на вторую страницу (копию); перед переходом открыта первая — иначе
  // тест прошёл бы и без обработки `?page=` (файл всегда открывается на первой).
  const copyId = await page.evaluate(() => window.__ferrum!.ui!.pages.list[1].id);
  await copy.click({ button: 'right' });
  await menu.getByRole('menuitem', { name: 'Копировать ссылку на страницу' }).click();
  await expect(page.getByText('Ссылка на страницу скопирована')).toBeVisible();
  const firstId = await page.evaluate(() => window.__ferrum!.ui!.pages.list[0].id);
  expect(copyId).not.toBe(firstId);
  const id = fileId(page);
  const link = await page.evaluate(
    ([f, p]) => `${location.origin}${location.pathname}${location.search}#/file/${f}?page=${encodeURIComponent(p)}`,
    [id, copyId],
  );
  if (browserName === 'chromium') expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(link);
  await first.click();
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.pages.current)).toBe(firstId);
  // Переход по ссылке: файл открывается заново (сохранившись) — на странице из ссылки.
  await page.evaluate((l) => (location.href = l), link);
  await page.waitForFunction(
    (p) => window.__ferrum?.ui?.ready === true && window.__ferrum.ui.pages.list.length === 2 && window.__ferrum.ui.pages.current === p,
    copyId,
    { timeout: 30_000 },
  );
  // В адресе остаётся только файл.
  await expect.poll(() => page.url()).toMatch(new RegExp(`#/file/${id}$`));
  expect(await warnings(page)).toEqual([]);
});

test('правый щелчок в поле переименования слоя и страницы не открывает меню ядра', async ({ page }) => {
  await openApp(page);
  const tree = page.getByRole('tree', { name: 'Слои' });
  const row = tree.getByRole('treeitem', { name: 'Экран 3', exact: true });
  await row.click({ button: 'right' });
  await page.getByRole('menu', { name: 'Действия со слоем' }).getByRole('menuitem', { name: 'Переименовать' }).click();
  const input = page.getByRole('textbox', { name: 'Имя слоя' });
  await expect(input).toBeFocused();
  const sel = await live(page);
  await input.click({ button: 'right' });
  await expect(page.getByRole('menu')).toHaveCount(0);
  await expect(input).toBeVisible();
  expect((await live(page)).selectionCount).toBe(sel.selectionCount);
  await input.press('Escape');
  // То же — для страницы.
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  await pages.getByRole('option').first().click({ button: 'right' });
  await page.getByRole('menu', { name: 'Действия со страницей' }).getByRole('menuitem', { name: 'Переименовать' }).click();
  const field = pages.getByRole('textbox', { name: 'Имя страницы' });
  await expect(field).toBeFocused();
  await field.click({ button: 'right' });
  await expect(page.getByRole('menu')).toHaveCount(0);
  await expect(field).toBeVisible();
});

test('«Копировать ссылку на страницу» без буфера обмена — уведомление об ошибке, без исключения', async ({ page }) => {
  await openApp(page);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.evaluate(() => Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true }));
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  await pages.getByRole('option').first().click({ button: 'right' });
  await page.getByRole('menu', { name: 'Действия со страницей' }).getByRole('menuitem', { name: 'Копировать ссылку на страницу' }).click();
  await expect(page.getByText('Не удалось записать ссылку в буфер обмена')).toBeVisible();
  expect(errors).toEqual([]);
});
