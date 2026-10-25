
// Настройки: переназначить сочетание и пользоваться им (по физической клавише),
// конфликт с заменой, сочетания браузера, сброс, импорт и экспорт, тема,
// привязка к пикселям, сохранение между запусками.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, expectPixel, live, liveWhen, openApp } from './helpers';

async function openSettings(page: Page) {
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  return page.getByRole('dialog', { name: 'Настройки' });
}

const tool = (page: Page) => page.evaluate(() => window.__ferrum!.ui!.tool);

test('переназначить сочетание и пользоваться им; конфликт — с заменой', async ({ page }) => {
  await openApp(page);
  const dialog = await openSettings(page);
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('Прямоугольник');
  await dialog.getByRole('button', { name: 'Добавить сочетание для «Прямоугольник»' }).click();
  await expect(dialog.getByRole('status')).toHaveText('Нажмите сочетание…');
  // Физическая клавиша Q — на русской раскладке это «й», сочетание то же.
  await page.keyboard.press('KeyQ');
  await expect(dialog.getByRole('row').filter({ hasText: 'Прямоугольник' })).toContainText('Q');
  await dialog.getByRole('button', { name: 'Сбросить', exact: true }).waitFor();
  // Конфликт: R уже у прямоугольника.
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('Эллипс');
  await dialog.getByRole('button', { name: 'Добавить сочетание для «Эллипс»' }).click();
  await page.keyboard.press('KeyR');
  const confirm = dialog.getByRole('alertdialog', { name: 'Сочетание занято' });
  await expect(confirm).toContainText('R уже у команды «Прямоугольник»');
  await confirm.getByRole('button', { name: 'Заменить' }).click();
  // Первый Esc очищает поиск, второй закрывает диалог.
  await page.keyboard.press('Escape');
  await expect(dialog.getByRole('searchbox', { name: 'Поиск команд' })).toHaveValue('');
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyQ');
  await expect.poll(() => tool(page)).toBe('rect');
  await page.keyboard.press('KeyR');
  await expect.poll(() => tool(page)).toBe('ellipse');
  // Сохранено: после перезагрузки раскладка та же.
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true);
  // Сохранённая раскладка при запуске — без уведомления.
  await expect(page.getByRole('status').filter({ hasText: 'Раскладка импортирована' })).toHaveCount(0);
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyQ');
  await expect.poll(() => tool(page)).toBe('rect');
  // Сбросить все.
  const again = await openSettings(page);
  await again.getByRole('button', { name: 'Сбросить все' }).click();
  await page.keyboard.press('Escape');
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyR');
  await expect.poll(() => tool(page)).toBe('rect');
});

test('импорт, экспорт и предупреждение о сочетаниях браузера', async ({ page }) => {
  await openApp(page);
  const mod = await page.evaluate(() => (navigator.platform.toLowerCase().startsWith('mac') ? 'Meta' : 'Ctrl'));
  const dialog = await openSettings(page);
  const json = JSON.stringify({ format: 'ferrum-keymap', version: 1, overrides: { 'edit.duplicate': [`${mod}+KeyW`], 'tool.pen': ['KeyP'] } });
  await dialog.locator('input[type=file]').setInputFiles({ name: 'keys.json', mimeType: 'application/json', buffer: Buffer.from(json) });
  await expect(page.getByRole('status').filter({ hasText: /Раскладка импортирована, пропущено: 1/ })).toBeVisible();
  const row = dialog.getByRole('row').filter({ hasText: 'Дублировать' });
  await expect(row).toContainText('Перехватывает браузер');
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Экспорт' }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('ferrum-keymap.json');
  const text = Buffer.concat(await (await file.createReadStream()).toArray()).toString();
  expect(JSON.parse(text)).toMatchObject({ format: 'ferrum-keymap', overrides: { 'edit.duplicate': [`${mod}+KeyW`] } });
  // Неверный файл — ошибка, раскладка не меняется.
  await dialog.locator('input[type=file]').setInputFiles({ name: 'bad.json', mimeType: 'application/json', buffer: Buffer.from('{') });
  await expect(page.getByRole('status').filter({ hasText: 'Не удалось импортировать раскладку' })).toBeVisible();
  await expect(row).toContainText('Перехватывает браузер');
});

const cssVar = (page: Page, name: string) =>
  page.evaluate((n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(), name);
const color = (page: Page, selector: string, prop: 'backgroundColor' | 'color') =>
  page.evaluate(([sel, p]) => getComputedStyle(document.querySelector(sel)!)[p as 'color'], [selector, prop]);

async function pickTheme(page: Page, name: string) {
  const dialog = await openSettings(page);
  await dialog.getByRole('tab', { name: 'Тема' }).click();
  await dialog.getByRole('radio', { name }).check();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
}

/** Пиксель холста у нижнего левого угла: на пустом месте страницы виден только фон. */
async function cornerPixel(page: Page) {
  const box = (await page.locator('canvas').boundingBox())!;
  const l = await live(page);
  return { x: box.x + 20, y: box.y + l.viewportH - 20 };
}

test('тема: светлая — токены, панели, холст и фон страницы; тёмная; как в системе', async ({ page }) => {
  await openApp(page, 100);
  const html = page.locator('html');
  await expect(html).toHaveAttribute('data-theme', 'dark');
  const dark = {
    canvas: await cssVar(page, '--canvas'),
    surface: await color(page, '.fe-side--right', 'backgroundColor'),
    ink: await color(page, '.fe-side--right', 'color'),
  };
  expect(dark.canvas).toBe('#1a1918');
  const at = await cornerPixel(page);
  await expectPixel(page, at.x, at.y, [26, 25, 24]);
  await expect(page.getByRole('textbox', { name: 'HEX фона страницы' })).toHaveValue('1A1918');

  await pickTheme(page, 'Светлая');
  await expect(html).toHaveAttribute('data-theme', 'light');
  // Токены светлой темы применены к интерфейсу…
  expect(await cssVar(page, '--canvas')).toBe('#e9e7e4');
  expect(await cssVar(page, '--ink')).toBe('#1c1b1a');
  expect(await color(page, 'body', 'backgroundColor')).toBe('rgb(233, 231, 228)');
  expect(await color(page, '.fe-side--right', 'backgroundColor')).not.toBe(dark.surface);
  expect(await color(page, '.fe-side--right', 'color')).not.toBe(dark.ink);
  // …и worker рисует холст в цвете светлой темы.
  await expectPixel(page, at.x, at.y, [233, 231, 228]);
  // Панель получила настоящую тему: фон страницы без своего цвета — холст светлой темы.
  const hex = page.getByRole('textbox', { name: 'HEX фона страницы' });
  await expect(hex).toHaveValue('E9E7E4');
  // «Тот же цвет» сверяется с цветом темы: ввод цвета темы правки не делает.
  const steps = (await live(page)).steps;
  await hex.fill('E9E7E4');
  await hex.press('Enter');
  expect(await page.evaluate(() => window.__ferrum!.ui!.pages.background)).toBeNull();
  expect((await live(page)).steps).toBe(steps);
  // А цвет тёмной темы в светлой — уже правка.
  await hex.fill('1A1918');
  await hex.press('Enter');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.pages.background)).not.toBeNull();
  await page.getByRole('button', { name: 'Фон как в теме' }).click();
  await expectPixel(page, at.x, at.y, [233, 231, 228]);

  // Тема переживает перезагрузку, и первый кадр — уже в её цвете.
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true);
  await expect(html).toHaveAttribute('data-theme', 'light');
  await expectPixel(page, at.x, at.y, [233, 231, 228]);

  // Как в системе — по prefers-color-scheme, на лету.
  await page.emulateMedia({ colorScheme: 'dark' });
  await pickTheme(page, 'Как в системе');
  await expect(html).toHaveAttribute('data-theme', 'dark');
  await expectPixel(page, at.x, at.y, [26, 25, 24]);
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(html).toHaveAttribute('data-theme', 'light');
  await expectPixel(page, at.x, at.y, [233, 231, 228]);
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(html).toHaveAttribute('data-theme', 'dark');
  await expectPixel(page, at.x, at.y, [26, 25, 24]);
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true);
  await expect(html).toHaveAttribute('data-theme', 'dark');
});

test('светлая тема: интерфейс и холст при запуске, без тёмной вспышки', async ({ page }) => {
  await page.addInitScript(() =>
    localStorage.setItem('ferrum.settings.v1', JSON.stringify({ theme: 'light', pixelSnap: true })),
  );
  await openApp(page, 100);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  const at = await cornerPixel(page);
  await expectPixel(page, at.x, at.y, [233, 231, 228]);
  // Панели и диалог — в светлых токенах.
  expect(await color(page, '.fe-side--left', 'backgroundColor')).toBe('rgb(255, 255, 255)');
  const dialog = await openSettings(page);
  expect(await dialog.evaluate((d) => getComputedStyle(d).backgroundColor)).toBe('rgb(255, 255, 255)');
  await expect(dialog.getByRole('tab', { name: 'Горячие клавиши' })).toBeFocused();
});

test('привязка к пикселям выключается в настройках', async ({ page }) => {
  await openApp(page, 0);
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'setZoom', zoom: 3 }));
  await liveWhen(page, 'l.zoom === 3');
  // Каждый прямоугольник — на своём месте: иначе второй притянулся бы к краю первого.
  let offset = 0;
  const draw = async () => {
    const l = await live(page);
    await page.keyboard.press('KeyR');
    const at = await appScreen(page, l, l.cameraX + offset, l.cameraY);
    offset += 40;
    await drag(page, at, 10, 10);
    return liveWhen(page, `l.steps === ${l.steps + 1} && l.busy === 0`);
  };
  await page.locator('canvas').focus();
  const snapped = await draw();
  expect(snapped.selectionW).toBe(Math.round(snapped.selectionW));
  const dialog = await openSettings(page);
  await dialog.getByRole('tab', { name: 'Холст' }).click();
  await dialog.getByRole('checkbox', { name: /Привязка к пикселям/ }).uncheck();
  await page.keyboard.press('Escape');
  await page.locator('canvas').focus();
  await page.keyboard.press('Escape');
  const free = await draw();
  expect(free.selectionW).not.toBe(Math.round(free.selectionW));
});

test('диалог: роли, фокус внутри, Esc закрывает', async ({ page }) => {
  await openApp(page);
  const dialog = await openSettings(page);
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
  await expect(dialog.getByRole('tab', { name: 'Горячие клавиши' })).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(dialog.getByRole('tab', { name: 'Тема' })).toBeFocused();
  for (let i = 0; i < 12; i++) await page.keyboard.press('Tab');
  expect(await dialog.evaluate((d) => d.contains(document.activeElement))).toBe(true);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Меню файла' })).toBeFocused();
});

/** Выделить «Слой 1» первого фрейма щелчком на холсте (мир (200, 40)). */
async function selectShape(page: Page) {
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  return liveWhen(page, 'l.selectionCount === 1');
}

const toasts = (page: Page) => page.locator('.fe-toast, [role="status"]').filter({ hasText: /./ });

test('диалог модальный: щелчок по пустому месту не отдаёт клавиши холсту', async ({ page }) => {
  await openApp(page);
  const before = await selectShape(page);
  const dialog = await openSettings(page);
  // Заголовок и поля диалога — не интерактивные: щелчок оставляет фокус на самом диалоге.
  await dialog.getByRole('heading', { name: 'Настройки' }).click();
  await expect(dialog).toBeFocused();
  await dialog.locator('.fe-dialog__body').click({ position: { x: 2, y: 2 } });
  expect(await dialog.evaluate((d) => d.contains(document.activeElement))).toBe(true);
  await page.keyboard.press('Delete');
  await page.keyboard.press('KeyR');
  await page.keyboard.press('ControlOrMeta+KeyD');
  // Ядру ничего не ушло: выделение, число шагов и инструмент прежние.
  const after = await live(page);
  expect([after.selectionCount, after.steps]).toEqual([1, before.steps]);
  expect(await tool(page)).toBe('move');
  // Даже если фокус всё же оказался на body, пока диалог открыт, холст клавиш не получает.
  await page.evaluate(() => (document.activeElement as HTMLElement).blur());
  await page.keyboard.press('Delete');
  expect((await live(page)).steps).toBe(before.steps);
  expect((await live(page)).selectionCount).toBe(1);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  // Диалог закрыт — Delete снова работает.
  await page.locator('canvas').focus();
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.steps === ${before.steps + 1} && l.selectionCount === 0`);
});

test('диалог подтверждения тоже модальный: щелчок по тексту и Delete', async ({ page }) => {
  await openApp(page);
  const before = await selectShape(page);
  // Правка есть — «Создать» спросит.
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: 'Создать' }).click();
  const confirm = page.getByRole('dialog', { name: 'Создать новый файл?' });
  await expect(confirm).toBeVisible();
  await confirm.getByText('Несохранённые изменения будут потеряны').click();
  await expect(confirm).toBeFocused();
  const steps = (await live(page)).steps;
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await page.keyboard.press('Delete');
  expect((await live(page)).steps).toBe(steps);
});

test('запись сочетания: все клавиши — записи, холсту ничего; Esc отменяет и диалог остаётся', async ({ page }) => {
  await openApp(page);
  const before = await selectShape(page);
  const dialog = await openSettings(page);
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('Эллипс');
  const add = dialog.getByRole('button', { name: 'Добавить сочетание для «Эллипс»' });
  await add.click();
  const status = dialog.getByRole('status');
  await expect(status).toHaveText('Нажмите сочетание…');
  // Одиночный модификатор — ждать дальше.
  await page.keyboard.down('Shift');
  await expect(status).toHaveText('Нажмите сочетание…');
  await page.keyboard.up('Shift');
  // Esc отменяет запись, диалог остаётся.
  await page.keyboard.press('Escape');
  await expect(status).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await expect(add).toBeFocused();
  // Delete во время записи — сочетание «Delete» у «Эллипса» (это конфликт с удалением),
  // но сам Delete холсту не уходит: выделение цело.
  await add.click();
  await page.keyboard.press('Delete');
  await expect(dialog.getByRole('alertdialog', { name: 'Сочетание занято' })).toContainText('Delete');
  expect((await live(page)).steps).toBe(before.steps);
  expect((await live(page)).selectionCount).toBe(1);
  // Вопрос о замене закрывается первым Esc, диалог — вторым.
  await page.keyboard.press('Escape');
  await expect(dialog.getByRole('alertdialog')).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog.getByRole('searchbox', { name: 'Поиск команд' })).toHaveValue('');
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  // Запись прервана щелчком в сторону: не висит вечно.
  const again = await openSettings(page);
  await again.getByRole('searchbox', { name: 'Поиск команд' }).fill('Эллипс');
  await again.getByRole('button', { name: 'Добавить сочетание для «Эллипс»' }).click();
  await expect(again.getByRole('status')).toBeVisible();
  await again.getByRole('heading', { name: 'Настройки' }).click();
  await expect(again.getByRole('status')).toHaveCount(0);
});

test('AltGr: на Windows сочетание с ним отклоняется, на macOS игнорируется', async ({ page }) => {
  for (const [platform, accepted] of [['Win32', false], ['MacIntel', true]] as const) {
    await page.addInitScript((p) => Object.defineProperty(navigator, 'platform', { get: () => p, configurable: true }), platform);
    await openApp(page);
    const dialog = await openSettings(page);
    await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('Эллипс');
    await dialog.getByRole('button', { name: 'Добавить сочетание для «Эллипс»' }).click();
    await page.evaluate(() =>
      window.dispatchEvent(
        new KeyboardEvent('keydown', { code: 'KeyQ', key: '@', ctrlKey: true, altKey: true, modifierAltGraph: true, bubbles: true, cancelable: true }),
      ),
    );
    if (accepted) {
      await expect(dialog.getByRole('row').filter({ hasText: 'Эллипс' })).toContainText('Q');
    } else {
      await expect(dialog.getByRole('status')).toContainText('AltGr');
      // Запись продолжается: обычное сочетание принимается.
      await page.keyboard.press('KeyQ');
      await expect(dialog.getByRole('row').filter({ hasText: 'Эллипс' })).toContainText('Q');
    }
    await page.keyboard.press('Escape');
  }
});

test('русская раскладка: запись по физической клавише, подписи — по раскладке, в том числе в подсказках', async ({ page }) => {
  await page.addInitScript(() => {
    const ru = new Map([
      ['KeyQ', 'й'],
      ['KeyR', 'к'],
      ['KeyV', 'м'],
    ]);
    Object.defineProperty(navigator, 'keyboard', { value: { getLayoutMap: async () => ru }, configurable: true });
  });
  await openApp(page);
  // Подсказка кнопки читает раскладку пользователя: «К», а не «R».
  await page.getByRole('button', { name: 'Прямоугольник', exact: true }).hover();
  await expect(page.getByRole('tooltip')).toHaveText('Прямоугольник  К');
  const dialog = await openSettings(page);
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('Прямоугольник');
  await expect(dialog.getByRole('row').filter({ hasText: 'Прямоугольник' })).toContainText('К');
  await dialog.getByRole('button', { name: 'Добавить сочетание для «Прямоугольник»' }).click();
  // На русской раскладке эта клавиша печатает «й»; сочетание записывается как KeyQ.
  await page.keyboard.press('KeyQ');
  await expect(dialog.getByRole('row').filter({ hasText: 'Прямоугольник' })).toContainText('Й');
  expect(await page.evaluate(() => window.__ferrum!.ui!.keymap.commands.find((c) => c.key === 'tool.rect')!.bindings)).toContain('KeyQ');
  // Поиск по подписи сочетания — тоже по раскладке.
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('й');
  await expect(dialog.getByRole('row').filter({ hasText: 'Прямоугольник' })).toHaveCount(1);
});

test('хранилище: мусор и отказ доступа — умолчания, интерфейс не падает', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('ferrum.settings.v1', '{не json'));
  await openApp(page);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(page.getByRole('alert')).toHaveCount(0);

  const blocked = await page.context().newPage();
  await blocked.addInitScript(() =>
    Object.defineProperty(window, 'localStorage', {
      get() {
        throw new DOMException('denied', 'SecurityError');
      },
    }),
  );
  await openApp(blocked);
  const dialog = await openSettings(blocked);
  await dialog.getByRole('tab', { name: 'Тема' }).click();
  await dialog.getByRole('radio', { name: 'Светлая' }).check();
  await expect(blocked.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(blocked.getByRole('alert')).toHaveCount(0);
});

test('сохранённая раскладка применяется при запуске молча; испорченная — одна ошибка и сброс', async ({ page, browser }) => {
  await page.addInitScript(() =>
    localStorage.setItem(
      'ferrum.settings.v1',
      JSON.stringify({ keymap: JSON.stringify({ format: 'ferrum-keymap', version: 1, overrides: { 'tool.rect': ['KeyQ'] } }) }),
    ),
  );
  await openApp(page);
  expect(await page.evaluate(() => window.__ferrum!.ui!.keymap.commands.find((c) => c.key === 'tool.rect')!.bindings)).toEqual(['KeyQ']);
  await expect(toasts(page).filter({ hasText: 'Раскладка импортирована' })).toHaveCount(0);
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyQ');
  await expect.poll(() => tool(page)).toBe('rect');

  // Свой контекст: хранилище первой страницы (её отложенная запись) не должно подменить значение.
  const bad = await (await browser.newContext({ baseURL: 'http://localhost:4173', viewport: { width: 1280, height: 720 } })).newPage();
  await bad.addInitScript(() => localStorage.setItem('ferrum.settings.v1', JSON.stringify({ keymap: '{"format":"чужой"}', theme: 'light' })));
  await openApp(bad);
  await expect(bad.getByRole('status').filter({ hasText: 'Не удалось импортировать раскладку' })).toHaveCount(1);
  // Тема из того же значения не пострадала, а испорченная раскладка из хранилища ушла.
  await expect(bad.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect
    .poll(() => bad.evaluate(() => JSON.parse(localStorage.getItem('ferrum.settings.v1')!).keymap as string))
    .not.toContain('чужой');
});

test('импорт: слишком большой файл, не JSON и чтение файла — понятные сообщения, интерфейс жив', async ({ page }) => {
  await openApp(page);
  const dialog = await openSettings(page);
  const input = dialog.locator('input[type=file]');
  await input.setInputFiles({ name: 'big.json', mimeType: 'application/json', buffer: Buffer.alloc(70 * 1024, 32) });
  await expect(page.getByRole('status').filter({ hasText: 'Файл раскладки слишком большой' })).toBeVisible();
  await input.setInputFiles({ name: 'photo.json', mimeType: 'application/json', buffer: Buffer.from([0xff, 0xd8, 0xff, 0x00, 0x81]) });
  await expect(page.getByRole('status').filter({ hasText: 'Не удалось импортировать раскладку' })).toBeVisible();
  const errors = () => page.evaluate(() => window.__ferrum!.uiEvents!.filter((e) => e.type === 'error').length);
  const e0 = await errors();
  await input.setInputFiles({ name: 'empty.json', mimeType: 'application/json', buffer: Buffer.alloc(0) });
  // Пустой файл — ровно одна ошибка ядра, раскладка не тронута.
  await expect.poll(errors).toBe(e0 + 1);
  await input.setInputFiles({ name: 'array.json', mimeType: 'application/json', buffer: Buffer.from('[1,2,3]') });
  await expect.poll(errors).toBe(e0 + 2);
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('row').filter({ hasText: 'Прямоугольник' })).toContainText('R');
  // Экспорт без отличий — всё равно верный файл раскладки.
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Экспорт' }).click();
  const text = Buffer.concat(await (await (await download).createReadStream()).toArray()).toString();
  expect(JSON.parse(text)).toMatchObject({ format: 'ferrum-keymap', version: 1 });
});

test('экспорт и импорт обратно — раскладка та же', async ({ page }) => {
  await openApp(page);
  const dialog = await openSettings(page);
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('Прямоугольник');
  await dialog.getByRole('button', { name: 'Добавить сочетание для «Прямоугольник»' }).click();
  await page.keyboard.press('KeyQ');
  const keys = () => page.evaluate(() => window.__ferrum!.ui!.keymap.commands.find((c) => c.key === 'tool.rect')!.bindings);
  await expect.poll(keys).toEqual(['KeyR', 'KeyQ']);
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Экспорт' }).click();
  const bytes = Buffer.concat(await (await (await download).createReadStream()).toArray());
  await dialog.getByRole('button', { name: 'Сбросить все' }).click();
  await expect.poll(keys).toEqual(['KeyR']);
  await dialog.locator('input[type=file]').setInputFiles({ name: 'ferrum-keymap.json', mimeType: 'application/json', buffer: bytes });
  await expect.poll(keys).toEqual(['KeyR', 'KeyQ']);
  await expect(page.getByRole('status').filter({ hasText: 'Раскладка импортирована' })).toBeVisible();
});

const active = (page: Page) =>
  page.evaluate(() => {
    const a = document.activeElement as HTMLElement | null;
    return {
      inDialog: !!a?.closest('[role="dialog"]'),
      label: a?.getAttribute('aria-label') ?? a?.tagName ?? '',
    };
  });

test('фокус остаётся на строке после записи, удаления сочетания и сброса', async ({ page }) => {
  await openApp(page);
  const dialog = await openSettings(page);
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('Прямоугольник');
  const add = dialog.getByRole('button', { name: 'Добавить сочетание для «Прямоугольник»' });
  await add.click();
  await page.keyboard.press('KeyQ');
  await expect(dialog.getByRole('row').filter({ hasText: 'Прямоугольник' })).toContainText('Q');
  await dialog.getByRole('button', { name: 'Сбросить', exact: true }).waitFor();
  // Событие раскладки пришло — «+» не пересоздан и в фокусе.
  await expect(add).toBeFocused();
  // × у Q: фокус на соседнем сочетании (R), затем «+».
  await dialog.getByRole('button', { name: /Убрать Q у команды «Прямоугольник»/ }).click();
  await expect(dialog.getByRole('button', { name: /Убрать Q /})).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: /Убрать R у команды «Прямоугольник»/ })).toBeFocused();
  // Сбросить: «Сбросить» исчезает, фокус — на «+».
  await add.click();
  await page.keyboard.press('KeyQ');
  await dialog.getByRole('button', { name: 'Сбросить', exact: true }).click();
  await expect(dialog.getByRole('button', { name: 'Сбросить', exact: true })).toHaveCount(0);
  await expect(add).toBeFocused();
  // Убрать единственное сочетание — фокус на «+», а не на body.
  await dialog.getByRole('button', { name: /Убрать R у команды «Прямоугольник»/ }).click();
  await expect(add).toBeFocused();
  expect((await active(page)).inDialog).toBe(true);
});

test('«Отмена» (Esc): сочетание Escape убрать нельзя', async ({ page }) => {
  await openApp(page);
  const dialog = await openSettings(page);
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('Отмена');
  const row = dialog.getByRole('row').filter({ hasText: 'Отмена' }).first();
  await expect(row).toContainText('Esc');
  await expect(row.getByRole('button', { name: /Убрать/ })).toHaveCount(0);
});

test('запись: Tab и буфер обмена отклоняются, подсказка о браузере, Esc очищает поиск', async ({ page }) => {
  await openApp(page);
  const dialog = await openSettings(page);
  const search = dialog.getByRole('searchbox', { name: 'Поиск команд' });
  await search.fill('Эллипс');
  await dialog.getByRole('button', { name: 'Добавить сочетание для «Эллипс»' }).click();
  await expect(dialog.getByText('Некоторые сочетания перехватывает браузер')).toBeVisible();
  const status = dialog.getByRole('status');
  for (const combo of ['Tab', 'Shift+Tab', 'ControlOrMeta+KeyC', 'ControlOrMeta+KeyV']) {
    await page.keyboard.press(combo);
    await expect(status).toContainText('не сработает');
  }
  // Запись всё ещё идёт; нормальное сочетание принимается.
  await page.keyboard.press('KeyO');
  await expect(dialog.getByRole('row').filter({ hasText: 'Эллипс' })).toContainText('O');
  // Esc в непустом поиске очищает его и оставляет диалог и фокус.
  await search.focus();
  await page.keyboard.press('Escape');
  await expect(search).toHaveValue('');
  await expect(search).toBeFocused();
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
});

test('тема из хранилища ставится до загрузки приложения (без вспышки)', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('ferrum.settings.v1', JSON.stringify({ theme: 'light' })));
  // Приложение не грузится вовсе: тему мог поставить только встроенный скрипт.
  await page.route('**/assets/main-*.js', (r) => r.abort());
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  // «Как в системе» — по prefers-color-scheme.
  await page.addInitScript(() => localStorage.setItem('ferrum.settings.v1', JSON.stringify({ theme: 'system' })));
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('частично верная сохранённая раскладка: предупреждение один раз, в хранилище — очищенная', async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem('ferrum.settings.v1'))
      localStorage.setItem(
        'ferrum.settings.v1',
        JSON.stringify({
          keymap: JSON.stringify({ format: 'ferrum-keymap', version: 1, overrides: { 'tool.rect': ['KeyQ'], 'нет.такой': ['KeyZ'] } }),
        }),
      );
  });
  await openApp(page);
  await expect(page.getByRole('status').filter({ hasText: 'Сохранённая раскладка: пропущено' })).toHaveCount(1);
  const stored = () => page.evaluate(() => JSON.parse(localStorage.getItem('ferrum.settings.v1')!).keymap as string);
  await expect.poll(stored).not.toContain('нет.такой');
  expect(await stored()).toContain('KeyQ');
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true);
  await expect(page.getByRole('status').filter({ hasText: 'Сохранённая раскладка' })).toHaveCount(0);
  expect(await page.evaluate(() => window.__ferrum!.ui!.keymap.commands.find((c) => c.key === 'tool.rect')!.bindings)).toEqual(['KeyQ']);
});
