// Контекстные меню (спека подсистемы 2, раздел 5): дерево и выбор на правой
// кнопке — из ядра, меню открывается у точки щелчка; основной поток не читает
// раскладку вне фазы measure (`__ferrumWarnings` пуст).

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

const labels = (page: Page, name: string) =>
  page
    .getByRole('menu', { name, exact: true })
    .locator(':scope > [role^="menuitem"]')
    .evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.label));

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
  const box = (await menu.boundingBox())!;
  expect(Math.abs(box.x - node.x)).toBeLessThanOrEqual(1);
  expect(box.y).toBeGreaterThanOrEqual(8);
  expect(box.y + box.height).toBeLessThanOrEqual(720 - 8 + 1);
  expect(await labels(page, 'Действия с выделением')).toEqual([
    'Копировать',
    'Вставить',
    'Копировать как',
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

test('пустое место холста: выделение снимается, меню — вставить, выделить всё, показать всё', async ({ page }) => {
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
  expect(await labels(page, 'Действия с выделением')).toEqual(['Вставить', 'Выделить всё', 'Показать всё']);
  await menu.getByRole('menuitem', { name: /^Выделить всё/ }).click();
  await liveWhen(page, 'l.selectionCount === 10');
  expect(await warnings(page)).toEqual([]);
});
