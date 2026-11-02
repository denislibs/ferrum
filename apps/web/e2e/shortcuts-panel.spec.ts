// Панель «Сочетания клавиш» (спека подсистемы 2, раздел 8): Ctrl+Shift+? и пункт
// «Справка → Сочетания клавиш», вкладки по группам, клавиши из действующей раскладки,
// отметка использованных недавно, ссылка в настройки. Открытие не читает раскладку
// вне фазы measure.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

const panel = (page: Page) => page.getByRole('dialog', { name: 'Сочетания клавиш' });
const row = (page: Page, key: string) => panel(page).locator(`[data-command="${key}"]`);
const tab = (page: Page, name: string) => panel(page).getByRole('tab', { name, exact: true });
const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

async function openByKey(page: Page): Promise<void> {
  await page.locator('canvas').focus();
  await page.keyboard.press('Control+Shift+Slash');
  await expect(panel(page)).toBeVisible();
}

test('Ctrl+Shift+? поднимает панель снизу окна; то же сочетание и Esc закрывают', async ({ page }) => {
  await openApp(page);
  await openByKey(page);
  await panel(page).evaluate((el) => Promise.all(el.getAnimations().map((a) => a.finished)));
  const box = (await panel(page).boundingBox())!;
  const size = page.viewportSize()!;
  expect(box.x).toBe(0);
  expect(box.width).toBe(size.width);
  expect(box.y + box.height).toBe(size.height);
  expect(box.height).toBeLessThanOrEqual(size.height / 2);
  // Фокус — на выбранной вкладке, клавиши холсту не уходят.
  await expect(tab(page, 'Инструменты')).toBeFocused();
  await page.keyboard.press('KeyR');
  expect(await page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('move');
  await page.keyboard.press('Control+Shift+Slash');
  await expect(panel(page)).toHaveCount(0);
  await expect(page.locator('canvas')).toBeFocused();
  await openByKey(page);
  await page.keyboard.press('Escape');
  await expect(panel(page)).toHaveCount(0);
  await expect(page.locator('canvas')).toBeFocused();
  expect(await warnings(page)).toEqual([]);
});

test('панель не модальная: холст работает, сочетание с холста её закрывает', async ({ page }) => {
  await openApp(page);
  await openByKey(page);
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await expect(panel(page)).toBeVisible();
  await page.keyboard.press('Control+Shift+Slash');
  await expect(panel(page)).toHaveCount(0);
});

test('пункт «Справка → Сочетания клавиш» и вкладки: клавиши из действующей раскладки', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Справка', 'Сочетания клавиш');
  await expect(panel(page)).toBeVisible();
  await expect(panel(page).getByRole('tab')).toHaveText(['Инструменты', 'Правка', 'Объект', 'Текст', 'Вектор', 'Порядок', 'Сдвиг', 'Вид', 'Файл', 'Настройки', 'Справка']);
  await expect(row(page, 'tool.rect')).toContainText('Прямоугольник');
  await expect(row(page, 'tool.rect').locator('kbd')).toHaveText(['R']);
  await expect(row(page, 'tool.frame').locator('kbd')).toHaveText(['F', 'A']);
  // Стрелки ходят по вкладкам, в «Правке» — её команды.
  await tab(page, 'Инструменты').focus();
  await page.keyboard.press('ArrowRight');
  await expect(tab(page, 'Правка')).toBeFocused();
  await expect(tab(page, 'Правка')).toHaveAttribute('aria-selected', 'true');
  await expect(row(page, 'edit.duplicate')).toBeVisible();
  await expect(row(page, 'tool.rect')).toHaveCount(0);
  // Команда без сочетания — прочерк.
  await tab(page, 'Файл').click();
  await expect(row(page, 'ui.new_file')).toContainText('—');
  await expect(row(page, 'ui.new_file').locator('kbd')).toHaveCount(0);
  // Новое сочетание в раскладке — панель показывает и его.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'bind', command: 'tool.rect', shortcut: 'KeyQ', replace: false }));
  await tab(page, 'Инструменты').click();
  await expect(row(page, 'tool.rect').locator('kbd')).toHaveText(['R', 'Q']);
});

test('использованные недавно отмечены: команда из меню и сочетание на холсте', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Вид', 'Масштаб 100%');
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyR');
  await page.keyboard.press('Escape');
  await openByKey(page);
  await expect(row(page, 'tool.rect')).toHaveClass(/fe-keys__row--used/);
  await expect(row(page, 'tool.rect').getByRole('img', { name: 'использовалась недавно' })).toBeVisible();
  await expect(row(page, 'tool.ellipse')).not.toHaveClass(/fe-keys__row--used/);
  await tab(page, 'Вид').click();
  await expect(row(page, 'view.zoom_100')).toHaveClass(/fe-keys__row--used/);
  await expect(row(page, 'view.fit_all')).not.toHaveClass(/fe-keys__row--used/);
});

test('«Изменить в настройках» открывает настройки на вкладке сочетаний', async ({ page }) => {
  await openApp(page);
  await openByKey(page);
  await panel(page).getByRole('button', { name: 'Изменить в настройках' }).click();
  await expect(panel(page)).toHaveCount(0);
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('tab', { name: 'Горячие клавиши' })).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  expect(await warnings(page)).toEqual([]);
});
