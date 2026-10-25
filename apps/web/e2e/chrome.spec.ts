// Раскладка UI3: тулбар рисует фигуры, меню и подсказки берут сочетания из
// раскладки, Ctrl+\ скрывает интерфейс, зум из шапки, ширина панелей,
// контекстное меню холста, роли для клавиатуры и экранного диктора.

import { expect, test, type Page } from '@playwright/test';
import { shortcutLabel } from '../src/ui/shortcuts';
import type { Platform } from '../src/core/core';
import { appScreen, drag, live, liveWhen, openApp } from './helpers';

const ui = <T>(page: Page, f: (s: NonNullable<Window['__ferrum']>['ui'] & object) => T) =>
  page.evaluate((src) => new Function('s', `return (${src})(s)`)(window.__ferrum!.ui), f.toString()) as Promise<T>;

test('прямоугольник рисуется инструментом из тулбара — один шаг', async ({ page }) => {
  await openApp(page);
  const toolbar = page.getByRole('toolbar', { name: 'Инструменты' });
  await toolbar.getByRole('button', { name: 'Фигуры' }).click();
  const menu = page.getByRole('menu', { name: 'Фигуры' });
  await expect(menu.getByRole('menuitemcheckbox')).toHaveCount(4);
  await menu.getByRole('menuitemcheckbox', { name: /Эллипс/ }).click();
  await expect.poll(() => ui(page, (s) => s.tool)).toBe('ellipse');
  await expect(toolbar.getByRole('button', { name: 'Эллипс' })).toHaveAttribute('aria-pressed', 'true');
  const before = await live(page);
  // Пустое место холста под первым фреймом: мир (100, 1080).
  const at = await appScreen(page, before, 100, 1080);
  await drag(page, at, 80, 40);
  const after = await liveWhen(page, `l.steps === ${before.steps + 1} && l.busy === 0`);
  expect([after.selectionCount, after.selectionW, after.selectionH]).toEqual([1, 160, 80]);
  await expect.poll(() => ui(page, (s) => s.tool)).toBe('move');
  await expect(toolbar.getByRole('button', { name: 'Эллипс' })).toHaveAttribute('aria-pressed', 'false');
});

test('Ctrl+\\ скрывает и показывает интерфейс', async ({ page }) => {
  await openApp(page);
  const left = page.getByRole('complementary', { name: 'Страницы и слои' });
  await expect(left).toBeVisible();
  await page.locator('canvas').click({ position: { x: 600, y: 40 } });
  await page.keyboard.press('ControlOrMeta+Backslash');
  await expect(left).toBeHidden();
  await expect(page.getByRole('toolbar')).toBeHidden();
  await liveWhen(page, 'l.viewportW === 1280');
  await page.keyboard.press('ControlOrMeta+Backslash');
  await expect(left).toBeVisible();
  await liveWhen(page, 'l.viewportW === 1280 - 2 * 248');
});

test('подсказка «Название  Клавиша» — из действующей раскладки', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Перемещение' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  V');
  await page.getByRole('button', { name: 'Перо — подсистема 3' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перо  P — подсистема 3');
  // Переназначили — подсказка другая.
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'bind', command: 'tool.move', shortcut: 'KeyQ', replace: true }));
  await page.mouse.move(5, 300);
  await expect(page.getByRole('tooltip')).toHaveCount(0);
  await page.getByRole('button', { name: 'Перемещение' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  V');
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'unbind', command: 'tool.move', shortcut: 'KeyV' }));
  await page.mouse.move(5, 300);
  await page.getByRole('button', { name: 'Перемещение' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  Q');
});

test('подписи сочетаний в интерфейсе совпадают с подписями ядра', async ({ page }) => {
  await openApp(page);
  const { platform, commands } = await page.evaluate(() => ({
    platform: navigator.platform.toLowerCase().startsWith('mac') ? 'mac' : 'linux',
    commands: window.__ferrum!.ui!.keymap.commands.map((c) => ({ bindings: [...c.bindings], labels: [...c.labels] })),
  }));
  let checked = 0;
  for (const c of commands) {
    c.bindings.forEach((b, i) => {
      expect(shortcutLabel(b, platform as Platform)).toBe(c.labels[i]);
      checked++;
    });
  }
  expect(checked).toBeGreaterThanOrEqual(commands.length);
});

test('зум из шапки и меню масштабов', async ({ page }) => {
  await openApp(page);
  const zoom = page.getByRole('button', { name: 'Масштаб', exact: true });
  await expect(zoom).toHaveText('50%');
  await zoom.click();
  await page.getByRole('menu', { name: 'Масштаб' }).getByRole('menuitemcheckbox', { name: /Масштаб 100%/ }).click();
  await liveWhen(page, 'l.zoom === 1');
  await expect(zoom).toHaveText('100%');
  // Клавиатура: меню открывается Enter, стрелки ходят по пунктам, Esc закрывает.
  await zoom.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('menuitem', { name: /Приблизить/ })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('menuitem', { name: /Отдалить/ })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('menu')).toHaveCount(0);
  await expect(zoom).toBeFocused();
});

test('ширина панели меняется перетаскиванием края', async ({ page }) => {
  await openApp(page);
  const edge = page.getByRole('separator', { name: 'Ширина левой панели' });
  const box = (await edge.boundingBox())!;
  await drag(page, { x: box.x + box.width / 2, y: 400 }, 100, 0);
  const l = await liveWhen(page, 'l.viewportW === 1280 - 248 - 348');
  expect(l.viewportW).toBe(684);
  await expect(edge).toHaveAttribute('aria-valuenow', '348');
  await edge.focus();
  await page.keyboard.press('ArrowLeft');
  await expect(edge).toHaveAttribute('aria-valuenow', '340');
});

test('контекстное меню холста выполняет команды редактора', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.mouse.click(node.x, node.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu.getByRole('menuitem', { name: /Дублировать/ })).toContainText(/D$/);
  await menu.getByRole('menuitem', { name: /Дублировать/ }).click();
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expect(menu).toHaveCount(0);
});

test('роли для клавиатуры: тулбар, вкладки, отключённые инструменты', async ({ page }) => {
  await openApp(page);
  await expect(page.getByRole('tablist')).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Дизайн' })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('tab', { name: 'Прототип' })).toHaveAttribute('aria-disabled', 'true');
  await expect(page.getByRole('button', { name: 'Поделиться' })).toHaveAttribute('aria-disabled', 'true');
  // Фокус с клавиатуры: кольцо select 2px.
  await page.getByRole('button', { name: 'Перемещение' }).focus();
  await page.keyboard.press('Tab');
  const outline = await page.evaluate(() => getComputedStyle(document.activeElement!).outlineWidth);
  expect(outline).toBe('2px');
});

test('меню: Home/End, Enter, Tab; фокус возвращается к якорю', async ({ page }) => {
  await openApp(page);
  const zoom = page.getByRole('button', { name: 'Масштаб', exact: true });
  await zoom.focus();
  await page.keyboard.press('Enter');
  const menu = page.getByRole('menu', { name: 'Масштаб' });
  await expect(menu.getByRole('menuitem', { name: /Приблизить/ })).toBeFocused();
  await page.keyboard.press('End');
  await expect(menu.getByRole('menuitemcheckbox', { name: /Масштаб 200%/ })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(menu.getByRole('menuitem', { name: /Приблизить/ })).toBeFocused();
  await page.keyboard.press('Home');
  await expect(menu.getByRole('menuitem', { name: /Приблизить/ })).toBeFocused();
  // Enter выполняет пункт, закрывает меню и возвращает фокус.
  await page.keyboard.press('End');
  await page.keyboard.press('Enter');
  await liveWhen(page, 'l.zoom === 2');
  await expect(menu).toHaveCount(0);
  await expect(zoom).toBeFocused();
  // Tab закрывает меню, не возвращая фокус.
  await page.keyboard.press('Enter');
  await expect(menu).toBeVisible();
  await page.keyboard.press('Tab');
  await expect(menu).toHaveCount(0);
});

test('стрелки в открытом меню не доходят до холста', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.mouse.click(node.x, node.y, { button: 'right' });
  const menu = page.getByRole('menu', { name: 'Действия с выделением' });
  await expect(menu.getByRole('menuitem').first()).toBeFocused();
  const before = await live(page);
  for (const key of ['ArrowRight', 'ArrowDown', 'ArrowLeft', 'ArrowUp']) await page.keyboard.press(key);
  await page.waitForTimeout(200);
  const after = await live(page);
  expect([after.steps, after.cameraX, after.cameraY, after.selectionW]).toEqual([before.steps, before.cameraX, before.cameraY, before.selectionW]);
  expect(await page.evaluate(() => window.__ferrum!.uiEvents!.filter((e) => e.type === 'error').length)).toBe(0);
  // Esc закрывает и возвращает фокус на холст.
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
  await expect(page.locator('canvas')).toBeFocused();
});

test('ширина панели зажата в пределах 200–480 px', async ({ page }) => {
  await openApp(page);
  const edge = page.getByRole('separator', { name: 'Ширина левой панели' });
  const box = (await edge.boundingBox())!;
  await drag(page, { x: box.x + box.width / 2, y: 400 }, -600, 0);
  await expect(edge).toHaveAttribute('aria-valuenow', '200');
  await liveWhen(page, 'l.viewportW === 1280 - 200 - 248');
  const box2 = (await edge.boundingBox())!;
  await drag(page, { x: box2.x + box2.width / 2, y: 400 }, 800, 0);
  await expect(edge).toHaveAttribute('aria-valuenow', '480');
  await liveWhen(page, 'l.viewportW === 1280 - 480 - 248');
  await edge.focus();
  await page.keyboard.press('ArrowRight');
  await expect(edge).toHaveAttribute('aria-valuenow', '480');
  // Чтений layout вне фазы measure не было.
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});
