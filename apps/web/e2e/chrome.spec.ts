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
  const move = page.getByRole('button', { name: 'Перемещение' });
  await move.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  V');
  await page.getByRole('button', { name: 'Перо — подсистема 3' }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перо  P — подсистема 3');
  // Открытая подсказка следует за раскладкой: V снят, Q назначен — текст меняется на месте.
  await move.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  V');
  await page.evaluate(() => {
    window.__ferrum!.core.send({ type: 'unbind', command: 'tool.move', shortcut: 'KeyV' });
    window.__ferrum!.core.send({ type: 'bind', command: 'tool.move', shortcut: 'KeyQ', replace: false });
  });
  await ui(page, (s) => s.keymap.commands.length);
  await page.waitForFunction(() => window.__ferrum!.ui!.keymap.commands.find((c) => c.key === 'tool.move')?.bindings[0] === 'KeyQ');
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  Q');
  // Закрытая — тоже.
  await page.mouse.move(5, 300);
  await expect(page.getByRole('tooltip')).toHaveCount(0);
  await move.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Перемещение  Q');
  // Клавиша переходит от одной команды к другой: у прямоугольника R уходит.
  const rect = page.getByRole('button', { name: 'Прямоугольник', exact: true });
  await rect.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Прямоугольник  R');
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'bind', command: 'tool.move', shortcut: 'KeyR', replace: true }));
  await page.waitForFunction(() => !window.__ferrum!.ui!.keymap.commands.find((c) => c.key === 'tool.rect')?.bindings.includes('KeyR'));
  // Открытая подсказка прямоугольника потеряла «R» (клавиша ушла к «Перемещению»).
  await expect(page.getByRole('tooltip')).toHaveText('Прямоугольник');
  const moveKeys = await page.evaluate(() => [...window.__ferrum!.ui!.keymap.commands.find((c) => c.key === 'tool.move')!.bindings]);
  expect(moveKeys).toEqual(['KeyQ', 'KeyR']);
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

test('стрелки на крае панели и на кнопке тулбара не двигают выделение', async ({ page }) => {
  await openApp(page);
  const l = await live(page);
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y);
  const sel = await liveWhen(page, 'l.selectionCount === 1');
  const edge = page.getByRole('separator', { name: 'Ширина левой панели' });
  await edge.focus();
  await page.keyboard.press('ArrowRight');
  await expect(edge).toHaveAttribute('aria-valuenow', '256');
  await page.waitForTimeout(200);
  let now = await live(page);
  expect([now.selectionX, now.selectionY, now.steps]).toEqual([sel.selectionX, sel.selectionY, sel.steps]);
  await page.getByRole('button', { name: 'Перемещение' }).focus();
  for (const k of ['ArrowRight', 'ArrowLeft', 'ArrowDown', 'ArrowUp']) await page.keyboard.press(k);
  await page.waitForTimeout(200);
  now = await live(page);
  expect([now.selectionX, now.selectionY, now.steps]).toEqual([sel.selectionX, sel.selectionY, sel.steps]);
});

test('тулбар и вкладки: одна точка Tab, стрелки и Home/End', async ({ page }) => {
  await openApp(page);
  const tools = page.getByRole('toolbar', { name: 'Инструменты' }).getByRole('button');
  expect(await tools.evaluateAll((els) => els.filter((e) => e.getAttribute('tabindex') === '0').length)).toBe(1);
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+Tab');
  // Фокус вошёл в тулбар (тулбар в DOM раньше панелей, холст — перед ним).
  await page.getByRole('button', { name: 'Перемещение' }).focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('button', { name: 'Фрейм', exact: true })).toBeFocused();
  await page.keyboard.press('End');
  await expect(page.getByRole('button', { name: /Комментарии/ })).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('button', { name: 'Перемещение' })).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expect(page.getByRole('button', { name: /Комментарии/ })).toBeFocused();
  await page.keyboard.press('Home');
  await expect(page.getByRole('button', { name: 'Перемещение' })).toBeFocused();
  // Tab уходит из тулбара одним нажатием (внутри остальные кнопки вне порядка Tab).
  await page.keyboard.press('Tab');
  expect(await page.evaluate(() => !!document.activeElement!.closest('[role="toolbar"]'))).toBe(false);
  // Вкладки.
  const tabs = page.getByRole('tablist').getByRole('tab');
  expect(await tabs.evaluateAll((els) => els.filter((e) => e.getAttribute('tabindex') === '0').length)).toBe(1);
  await page.getByRole('tab', { name: 'Дизайн' }).focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('tab', { name: 'Прототип' })).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('tab', { name: 'Дизайн' })).toBeFocused();
  await page.keyboard.press('End');
  await expect(page.getByRole('tab', { name: 'Прототип' })).toBeFocused();
  await page.keyboard.press('Home');
  await expect(page.getByRole('tab', { name: 'Дизайн' })).toBeFocused();
  // Нет висячего aria-controls.
  const ctl = await page.getByRole('tab', { name: 'Дизайн' }).getAttribute('aria-controls');
  if (ctl) expect(await page.locator(`#${ctl}`).count()).toBe(1);
});

test('«Создать» просит подтверждения, пока в документе есть правки', async ({ page }) => {
  await openApp(page);
  const create = async () => {
    await page.getByRole('button', { name: 'Меню файла' }).click();
    await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: 'Создать' }).click();
  };
  // Правка: дублирование выделенного.
  const l = await live(page);
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.keyboard.press('ControlOrMeta+KeyD');
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  const total = (await live(page)).nodes;
  await create();
  const dialog = page.getByRole('dialog', { name: 'Создать новый файл?' });
  await expect(dialog).toContainText('Несохранённые изменения будут потеряны');
  await expect(dialog.getByRole('button', { name: 'Отмена' })).toBeFocused();
  await dialog.getByRole('button', { name: 'Отмена' }).click();
  await expect(dialog).toHaveCount(0);
  await page.waitForTimeout(300);
  expect((await live(page)).nodes).toBe(total);
  await create();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  expect((await live(page)).nodes).toBe(total);
  await create();
  await dialog.getByRole('button', { name: 'Создать' }).click();
  await liveWhen(page, 'l.nodes < 5');
  // Пустой документ без правок — без вопросов.
  await create();
  await expect(dialog).toHaveCount(0);
});

test('фокус пункта меню не теряется, когда пункты перестраиваются', async ({ page }) => {
  await openApp(page);
  const zoom = page.getByRole('button', { name: 'Масштаб', exact: true });
  await zoom.focus();
  await page.keyboard.press('Enter');
  const menu = page.getByRole('menu', { name: 'Масштаб' });
  await expect(menu.getByRole('menuitem', { name: /Приблизить/ })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  const fit = menu.getByRole('menuitem', { name: /Показать всё/ });
  await expect(fit).toBeFocused();
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'setZoom', zoom: 2 }));
  await liveWhen(page, 'l.zoom === 2');
  await expect(menu.getByRole('menuitemcheckbox', { name: /Масштаб 200%/ })).toHaveAttribute('aria-checked', 'true');
  await expect(fit).toBeFocused();
});

test('край панели: правая кнопка мыши не начинает перетаскивание', async ({ page }) => {
  await openApp(page);
  const edge = page.getByRole('separator', { name: 'Ширина левой панели' });
  const box = (await edge.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, 400);
  await page.mouse.down({ button: 'right' });
  await page.mouse.move(box.x + 100, 400, { steps: 5 });
  await page.mouse.up({ button: 'right' });
  await page.keyboard.press('Escape');
  await expect(edge).toHaveAttribute('aria-valuenow', '248');
});

// ——— Итоговая волна исправлений ———

/** Выделение и число шагов: ни одна клавиша на виджете панели не должна их менять. */
const docState = (page: Page) =>
  page.evaluate(() => ({
    steps: window.__ferrum!.core.live!.steps,
    nodes: window.__ferrum!.core.live!.nodes,
    x: window.__ferrum!.core.live!.selectionX,
    ids: [...window.__ferrum!.ui!.selectionIds],
    background: window.__ferrum!.ui!.pages.background,
  }));

test('стрелки, Delete и Backspace на кнопках панелей не доходят до холста', async ({ page }) => {
  await openApp(page);
  const tree = page.getByRole('tree', { name: 'Слои' });
  await tree.getByRole('treeitem', { name: 'Экран 9', exact: true }).click();
  await liveWhen(page, 'l.selectionCount === 1');
  const panel = page.locator('#panel-design');
  // Обводка — чтобы была её кнопка «Удалить обводку».
  const s0 = (await live(page)).steps;
  await panel.getByRole('button', { name: 'Добавить обводку' }).click();
  await liveWhen(page, `l.steps === ${s0 + 1}`);
  await expect(panel.getByRole('button', { name: 'Удалить обводку' })).toHaveCount(1);
  const controls = [
    panel.getByRole('button', { name: 'Отразить по горизонтали' }),
    panel.getByRole('button', { name: 'Пресет размера' }),
    panel.getByRole('radio', { name: '2×' }),
    panel.getByRole('button', { name: 'Добавить заливку' }),
    panel.getByRole('button', { name: 'Удалить заливку' }).first(),
    panel.getByRole('button', { name: 'Скрыть заливку' }).first(),
    panel.getByRole('button', { name: 'Цвет заливки' }).first(),
    panel.getByRole('button', { name: 'Удалить обводку' }),
    page.getByRole('button', { name: 'Меню файла' }),
    page.getByRole('button', { name: 'Масштаб', exact: true }),
    page.getByRole('button', { name: 'Страницы', exact: true }),
    page.getByRole('button', { name: 'Добавить страницу' }),
  ];
  const before = await docState(page);
  expect(before.ids).toHaveLength(1);
  for (const c of controls) {
    const name = (await c.getAttribute('aria-label')) ?? (await c.textContent());
    await c.focus();
    for (const k of ['ArrowLeft', 'Delete', 'Backspace', 'ArrowDown', 'Home']) await page.keyboard.press(k);
    await page.waitForTimeout(150);
    expect(await docState(page), `клавиши на «${name}»`).toEqual(before);
  }
  // Фон страницы: без выделения, свой фон задан — кнопка «Фон как в теме».
  await page.locator('canvas').focus();
  await page.keyboard.press('Escape');
  await liveWhen(page, 'l.selectionCount === 0');
  const hex = panel.getByRole('textbox', { name: 'HEX фона страницы' });
  await hex.fill('123456');
  await hex.press('Enter');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.pages.background)).toBe('#123456ff');
  const reset = panel.getByRole('button', { name: 'Фон как в теме' });
  const bare = await docState(page);
  await reset.focus();
  for (const k of ['ArrowLeft', 'Delete', 'Backspace']) await page.keyboard.press(k);
  await page.waitForTimeout(150);
  expect(await docState(page)).toEqual(bare);
});

test('масштаб экспорта — группа переключателей: стрелки выбирают, одна точка Tab', async ({ page }) => {
  await openApp(page);
  const group = page.getByRole('radiogroup', { name: 'Масштаб экспорта' });
  const radio = (n: string) => group.getByRole('radio', { name: n });
  await expect(radio('1×')).toHaveAttribute('tabindex', '0');
  await expect(radio('2×')).toHaveAttribute('tabindex', '-1');
  await radio('1×').focus();
  await page.keyboard.press('ArrowRight');
  await expect(radio('2×')).toBeFocused();
  await expect(radio('2×')).toHaveAttribute('aria-checked', 'true');
  await expect(radio('1×')).toHaveAttribute('aria-checked', 'false');
  await expect(radio('2×')).toHaveAttribute('tabindex', '0');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('ArrowLeft');
  await expect(radio('3×')).toBeFocused();
  await expect(radio('3×')).toHaveAttribute('aria-checked', 'true');
  await page.keyboard.press('Home');
  await expect(radio('1×')).toHaveAttribute('aria-checked', 'true');
});

const fileMenu = async (page: Page, item: string) => {
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: item }).click();
};

test('«Создать» спрашивает и после правок вне истории; после «Сохранить как…» — нет', async ({ page }) => {
  await openApp(page);
  const total = (await live(page)).nodes;
  // Переименование страницы в историю отмены не попадает, но документ изменился.
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  await pages.getByRole('option', { name: /Страница 1/ }).dblclick();
  await pages.getByRole('textbox', { name: 'Имя страницы' }).fill('Черновик');
  await pages.getByRole('textbox', { name: 'Имя страницы' }).press('Enter');
  await expect(pages.getByRole('option', { name: /Черновик/ })).toBeVisible();
  expect((await live(page)).canUndo).toBe(0);
  const dialog = page.getByRole('dialog', { name: 'Создать новый файл?' });
  await fileMenu(page, 'Создать');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Отмена' }).click();
  await expect(dialog).toHaveCount(0);
  await page.waitForTimeout(300);
  expect((await live(page)).nodes).toBe(total);
  await expect(pages.getByRole('option', { name: /Черновик/ })).toBeVisible();
  // Сохранено — вопроса нет, документ заменяется сразу.
  const download = page.waitForEvent('download');
  await fileMenu(page, 'Сохранить как…');
  await download;
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  await fileMenu(page, 'Создать');
  await liveWhen(page, 'l.nodes < 5');
  await expect(dialog).toHaveCount(0);
  // Имя документа — тоже правка вне истории.
  await page.locator('.fe-panel-head__name').dblclick();
  await page.getByRole('textbox', { name: 'Имя файла' }).fill('Новое имя');
  await page.getByRole('textbox', { name: 'Имя файла' }).press('Enter');
  await expect(page.locator('.fe-panel-head__name')).toHaveText('Новое имя');
  await fileMenu(page, 'Создать');
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
});

test('имя файла: Enter и F2 открывают поле, предел длины, фокус возвращается', async ({ page }) => {
  await openApp(page);
  const name = page.locator('.fe-panel-head__name');
  const input = page.getByRole('textbox', { name: 'Имя файла' });
  await name.focus();
  await page.keyboard.press('F2');
  await expect(input).toBeFocused();
  await expect(input).toHaveAttribute('maxlength', '256');
  await input.fill('План');
  await page.keyboard.press('Enter');
  await expect(name).toHaveText('План');
  await expect(name).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(input).toBeFocused();
  await input.fill('Не то');
  await page.keyboard.press('Escape');
  await expect(name).toHaveText('План');
  await expect(name).toBeFocused();
  // F2 на имени не дошёл до холста: шагов нет.
  expect((await live(page)).steps).toBe(0);
});

test('край панели: потеря захвата и движение без кнопки заканчивают перетаскивание', async ({ page }) => {
  await openApp(page);
  const edge = page.getByRole('separator', { name: 'Ширина левой панели' });
  const box = (await edge.boundingBox())!;
  const x = box.x + box.width / 2;
  const move = (clientX: number, buttons: number) =>
    edge.evaluate(
      (el, a) => el.dispatchEvent(new PointerEvent('pointermove', { pointerId: 1, isPrimary: true, bubbles: true, ...a })),
      { clientX, buttons },
    );
  for (const stop of ['lost', 'buttons'] as const) {
    await page.mouse.move(x, 400);
    await page.mouse.down();
    await page.mouse.move(x + 20, 400, { steps: 3 });
    const w = Number(await edge.getAttribute('aria-valuenow'));
    expect(w, stop).toBeGreaterThan(248);
    if (stop === 'lost') {
      await edge.evaluate((el) => el.dispatchEvent(new PointerEvent('lostpointercapture', { pointerId: 1, bubbles: true })));
      await move(x + 120, 1);
    } else {
      await move(x + 120, 0);
      await move(x + 140, 1);
    }
    await page.waitForTimeout(150);
    await expect(edge, stop).toHaveAttribute('aria-valuenow', String(w));
    await page.mouse.up();
  }
});

test('у кнопок-иконок панелей — подсказки «Название» или «Название  Клавиша»', async ({ page }) => {
  await openApp(page);
  const tree = page.getByRole('tree', { name: 'Слои' });
  const r = tree.getByRole('treeitem', { name: 'Экран 9', exact: true });
  await r.click();
  await liveWhen(page, 'l.selectionCount === 1');
  const panel = page.locator('#panel-design');
  await panel.getByRole('button', { name: 'Добавить обводку' }).click();
  await expect(panel.getByRole('button', { name: 'Удалить обводку' })).toHaveCount(1);
  const tip = page.getByRole('tooltip');
  const cases: [ReturnType<Page['locator']>, string][] = [
    [panel.getByRole('button', { name: 'Скрыть заливку' }).first(), 'Скрыть заливку'],
    [panel.getByRole('button', { name: 'Удалить заливку' }).first(), 'Удалить заливку'],
    [panel.getByRole('button', { name: 'Удалить обводку' }), 'Удалить обводку'],
  ];
  for (const [el, text] of cases) {
    await el.hover();
    await expect(tip, text).toHaveText(text);
    await page.mouse.move(700, 400);
    await expect(tip).toHaveCount(0);
  }
  await r.hover();
  await r.getByRole('button', { name: 'Заблокировать' }).hover();
  await expect(tip).toHaveText('Заблокировать');
  await r.getByRole('button', { name: 'Скрыть' }).hover();
  await expect(tip).toHaveText('Скрыть');
  await panel.getByRole('button', { name: 'Цвет заливки' }).first().click();
  await page.getByRole('dialog', { name: 'Цвет заливки' }).getByRole('button', { name: 'Закрыть палитру' }).hover();
  await expect(tip).toHaveText('Закрыть палитру  Esc');
});
