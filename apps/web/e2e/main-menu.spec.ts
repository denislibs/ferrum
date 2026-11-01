// Главное меню (спека подсистемы 2, раздел 4): кнопка с логотипом и Alt+/,
// подменю клавиатурой и наведением, серые пункты и галочки — из состояния
// команд ядра, сочетания из раскладки, поиск по командам в любой раскладке,
// подменю у края окна. Открытие меню и ввод в поиск не читают раскладку вне
// фазы measure (`__ferrumWarnings`).

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

const root = (page: Page) => page.getByRole('menu', { name: 'Главное меню' });
const submenu = (page: Page, name: string) => page.getByRole('menu', { name, exact: true });
const warnings = (page: Page) => page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

/** Выделить «Слой 1» первого фрейма щелчком на холсте (мир (200, 40)). */
async function selectNode(page: Page): Promise<void> {
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
}

test('логотип открывает меню с подменю Figma; клавиатура ходит внутрь и обратно', async ({ page }) => {
  await openApp(page);
  const logo = page.getByRole('button', { name: 'Главное меню' });
  await logo.click();
  await expect(logo).toHaveAttribute('aria-expanded', 'true');
  const menu = root(page);
  await expect(menu.getByRole('searchbox', { name: 'Поиск по командам' })).toBeFocused();
  await expect(menu.getByRole('menuitem')).toHaveText(['Файл', 'Правка', 'Вид', 'Объект', 'Текст', 'Расставить', 'Вектор', 'Настройки', 'Справка']);
  // Вниз — к первому пункту, вправо — в подменю на первый пункт.
  await page.keyboard.press('ArrowDown');
  const file = menu.getByRole('menuitem', { name: 'Файл' });
  await expect(file).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(file).toHaveAttribute('aria-expanded', 'true');
  await expect(submenu(page, 'Файл').getByRole('menuitem', { name: 'Новый файл' })).toBeFocused();
  // Влево — обратно к «Файлу», подменю закрыто.
  await page.keyboard.press('ArrowLeft');
  await expect(submenu(page, 'Файл')).toHaveCount(0);
  await expect(file).toBeFocused();
  // Вниз и Enter: «Правка», затем Esc — к «Правке», ещё Esc — меню закрыто, фокус — на логотипе.
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await expect(submenu(page, 'Правка').getByRole('menuitem').first()).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu.getByRole('menuitem', { name: 'Правка', exact: true })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
  await expect(logo).toBeFocused();
  expect(await warnings(page)).toEqual([]);
});

test('Alt+/ на холсте открывает главное меню (команда ui.main_menu через ядро)', async ({ page }) => {
  await openApp(page);
  await page.locator('canvas').click({ position: { x: 600, y: 40 } });
  await page.keyboard.press('Alt+Slash');
  await expect(root(page)).toBeVisible();
  await expect(root(page).getByRole('searchbox')).toBeFocused();
  // Повторный щелчок по логотипу закрывает меню.
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await expect(root(page)).toHaveCount(0);
});

test('подменю открывается наведением с задержкой и закрывается у соседнего пункта', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await root(page).getByRole('menuitem', { name: 'Объект' }).hover();
  const object = submenu(page, 'Объект');
  await expect(object).toBeVisible();
  // Подменю в подменю: «Булевы операции».
  await object.getByRole('menuitem', { name: 'Булевы операции' }).hover();
  await expect(submenu(page, 'Булевы операции').getByRole('menuitemcheckbox')).toHaveCount(4);
  // Соседний пункт верхнего уровня без подменю нет — «Вид» открывает своё.
  await root(page).getByRole('menuitem', { name: 'Вид' }).hover();
  await expect(submenu(page, 'Вид')).toBeVisible();
  await expect(object).toHaveCount(0);
  expect(await warnings(page)).toEqual([]);
});

test('серые пункты и галочки — из состояния команд ядра, сочетания — из раскладки', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Правка');
  const edit = submenu(page, 'Правка');
  const dup = edit.getByRole('menuitem', { name: /Дублировать/ });
  await expect(dup).toHaveAttribute('aria-disabled', 'true');
  await expect(dup).toContainText(/D$/);
  // Серый пункт не выполняется и меню не закрывает.
  await dup.click({ force: true });
  await expect(edit).toBeVisible();
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  // С выделением — доступен; выполнение — один шаг.
  await selectNode(page);
  const before = await live(page);
  await mainMenu(page, 'Правка', /Дублировать/);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  // Переключатель: «Пиксельная сетка» отмечена, щелчок снимает отметку (ядро).
  await mainMenu(page, 'Вид');
  const grid = submenu(page, 'Вид').getByRole('menuitemcheckbox', { name: /Пиксельная сетка/ });
  await expect(grid).toHaveAttribute('aria-checked', 'true');
  await grid.click();
  await mainMenu(page, 'Вид');
  await expect(submenu(page, 'Вид').getByRole('menuitemcheckbox', { name: /Пиксельная сетка/ })).toHaveAttribute('aria-checked', 'false');
  await page.keyboard.press('Escape');
});

test('тема из «Настройки › Тема»: отметка у выбранной', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Настройки', 'Тема', 'Светлая тема');
  await expect.poll(() => page.evaluate(() => document.documentElement.dataset.theme)).toBe('light');
  await mainMenu(page, 'Настройки', 'Тема');
  const theme = submenu(page, 'Тема');
  await expect(theme.getByRole('menuitemcheckbox', { name: 'Светлая тема' })).toHaveAttribute('aria-checked', 'true');
  await expect(theme.getByRole('menuitemcheckbox', { name: 'Тёмная тема' })).toHaveAttribute('aria-checked', 'false');
  await theme.getByRole('menuitemcheckbox', { name: 'Тёмная тема' }).click();
  await expect.poll(() => page.evaluate(() => document.documentElement.dataset.theme)).toBe('dark');
});

test('поиск по командам: подстрока, синонимы, другая раскладка; Enter выполняет', async ({ page }) => {
  await openApp(page);
  await selectNode(page);
  const before = await live(page);
  await page.getByRole('button', { name: 'Главное меню' }).click();
  const search = root(page).getByRole('searchbox');
  // «le,km» — «дубль» в английской раскладке (синоним «Дублировать»).
  await search.fill('le,km');
  await expect(root(page).getByRole('menuitem')).toHaveText([/Дублировать/]);
  // «щквук» — «order» в русской раскладке.
  await search.fill('щквук');
  await expect(root(page).getByRole('menuitem').first()).toContainText('На передний план');
  await search.fill('нет такой команды');
  await expect(root(page).getByText('Ничего не найдено')).toBeVisible();
  await search.fill('дубл');
  await page.keyboard.press('Enter');
  await expect(root(page)).toHaveCount(0);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  expect(await warnings(page)).toEqual([]);
});

test('переход по первой букве в подменю', async ({ page }) => {
  await openApp(page);
  await mainMenu(page, 'Правка');
  const edit = submenu(page, 'Правка');
  await edit.getByRole('menuitem').first().focus();
  await page.evaluate(() => document.activeElement!.dispatchEvent(new KeyboardEvent('keydown', { key: 'д', bubbles: true })));
  await expect(edit.getByRole('menuitem', { name: /Дублировать/ })).toBeFocused();
});

test('подменю у нижнего края окна поднимается и остаётся на экране', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 420 });
  await openApp(page);
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await root(page).getByRole('menuitem', { name: 'Объект' }).click();
  const object = submenu(page, 'Объект');
  await expect(object).toBeVisible();
  // Появление меню анимировано масштабом: размеры — после анимации.
  const settled = (m: ReturnType<typeof submenu>) => m.evaluate((el) => Promise.all(el.getAnimations().map((x) => x.finished)).then(() => true));
  await settled(object);
  await settled(root(page));
  const box = (await object.boundingBox())!;
  expect(box.y + box.height).toBeLessThanOrEqual(420 - 8 + 0.5);
  expect(box.y).toBeGreaterThanOrEqual(8);
  // Справа от пункта «Объект».
  const item = (await root(page).getByRole('menuitem', { name: 'Объект' }).boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(item.x + item.width - 1);
});

test('команды интерфейса: «Экспорт…» сочетанием, «О программе» и «Настройки…» из меню', async ({ page }) => {
  await openApp(page);
  await page.locator('canvas').click({ position: { x: 600, y: 40 } });
  await page.keyboard.press('ControlOrMeta+Shift+KeyE');
  await expect(page.getByRole('button', { name: /Экспортировать/ })).toBeFocused();
  await mainMenu(page, 'Справка', 'О программе');
  await expect(page.getByRole('status').filter({ hasText: 'Ferrum' })).toBeVisible();
  await mainMenu(page, 'Настройки', 'Настройки…');
  await expect(page.getByRole('dialog', { name: 'Настройки' })).toBeVisible();
});
