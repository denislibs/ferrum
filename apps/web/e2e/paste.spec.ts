// Вставка (спека подсистемы 2, раздел 6): повтор вставки того же содержимого —
// +10, +10; Ctrl/Cmd+Shift+V — «Вставить поверх» на исходные координаты;
// «Вставить с заменой» из меню; предупреждение о сочетании, занятом браузером.
// Первый тест не читает системный буфер (события `paste` — как от браузера) и
// идёт и в Firefox; чтение буфера (`clipboard-read`) — только в Chromium.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

/** Щелчок по узлу в точке мира. */
async function pick(page: Page, x: number, y: number) {
  const at = await appScreen(page, await live(page), x, y);
  await page.mouse.click(at.x, at.y);
}

/** Копия выделения текстом с меткой (как запасной путь кладёт её в буфер). Байты
 * события `clipboard` интерфейс может сразу передать worker'у (запасной путь
 * записи — Firefox): тогда текст берётся из его же `encodeClipboard`. */
async function copiedText(page: Page): Promise<string> {
  await page.evaluate(() => {
    const w = window as { __clip?: Uint8Array; __text?: string };
    const core = window.__ferrum!.core!;
    w.__clip = undefined;
    w.__text = undefined;
    const encode = core.encodeClipboard.bind(core);
    core.encodeClipboard = async (b) => {
      const t = await encode(b);
      if (t) w.__text = t;
      return t;
    };
    core.on((e) => {
      if (e.type === 'clipboard' && e.bytes.byteLength > 0) w.__clip = e.bytes.slice();
    });
  });
  // Команда ядра, а не Ctrl+C: Firefox без фокуса окна событие `copy` не шлёт.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'execute', command: 'edit.copy' }));
  await page.waitForFunction(() => {
    const w = window as { __clip?: Uint8Array; __text?: string };
    return w.__clip !== undefined || w.__text !== undefined;
  });
  return page.evaluate(async () => {
    const w = window as { __clip?: Uint8Array; __text?: string };
    return w.__text ?? (await window.__ferrum!.core!.encodeClipboard(w.__clip!))!;
  });
}

/** Событие `paste` на холсте с текстом; `over` — перед ним нажато Ctrl/Cmd+Shift+V. */
async function paste(page: Page, text: string, over = false) {
  await page.evaluate(
    ({ text, over }) => {
      const canvas = document.querySelector('canvas')!;
      const mac = navigator.platform.toLowerCase().startsWith('mac');
      canvas.dispatchEvent(
        new KeyboardEvent('keydown', { code: 'KeyV', key: 'V', shiftKey: over, ctrlKey: !mac, metaKey: mac, bubbles: true }),
      );
      const dt = new DataTransfer();
      dt.setData('text/plain', text);
      canvas.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
    },
    { text, over },
  );
}

test('повтор вставки — +10, +10; Ctrl/Cmd+Shift+V — поверх, на исходное место', async ({ page }) => {
  await openApp(page);
  // «Слой 1» в «Экран 0»: мир (140, 0) — (260, 80).
  await pick(page, 200, 40);
  const l = await liveWhen(page, 'l.selectionCount === 1');
  const text = await copiedText(page);
  for (const [i, x] of [140, 150, 160].entries()) {
    await paste(page, text);
    await liveWhen(page, `l.nodes === ${l.nodes + i + 1} && l.selectionX === ${x} && l.selectionY === ${x - 140}`);
  }
  await paste(page, text, true);
  const after = await liveWhen(page, `l.nodes === ${l.nodes + 4} && l.selectionX === 140 && l.selectionY === 0`);
  expect(after.steps).toBe(l.steps + 4);
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

test('«Вставить с заменой» и «Вставить поверх» из меню читают буфер', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'чтение буфера из меню — разрешение clipboard-read есть только в Chromium');
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await openApp(page);
  // «Слой 0» — эллипс в мире (0, 0) — (120, 80).
  await pick(page, 60, 40);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.keyboard.press('ControlOrMeta+c');
  await expect.poll(() => page.evaluate(async () => (await navigator.clipboard.read())[0]?.types ?? [])).toContain('web application/x-ferrum');
  // Заменить «Слой 1» (прямоугольник в (140, 0)): эллипс на его месте, одним шагом.
  await pick(page, 200, 40);
  const l = await liveWhen(page, 'l.selectionCount === 1 && l.selectionX === 140');
  await mainMenu(page, 'Правка', 'Вставить с заменой');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.summary?.kind)).toBe('ellipse');
  const r = await liveWhen(page, `l.steps === ${l.steps + 1}`);
  expect([r.nodes, r.selectionX, r.selectionY, r.selectionCount]).toEqual([l.nodes, 140, 0, 1]);
  // Поверх из контекстного меню: на исходное место эллипса.
  const at = await appScreen(page, r, 200, 40);
  await page.mouse.click(at.x, at.y, { button: 'right' });
  await page.getByRole('menu', { name: 'Действия с выделением' }).getByRole('menuitem', { name: /^Вставить поверх/ }).click();
  await liveWhen(page, `l.nodes === ${l.nodes + 1} && l.selectionX === 0 && l.selectionY === 0`);
});

const summary = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)));

test('Ctrl/Cmd+Shift+R на холсте — «Вставить с заменой» одним шагом, страница не перезагружается', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'чтение буфера — разрешение clipboard-read есть только в Chromium');
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await openApp(page);
  await page.evaluate(() => ((window as { __marker?: number }).__marker = 7));
  await pick(page, 60, 40);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.keyboard.press('ControlOrMeta+c');
  await expect.poll(() => page.evaluate(async () => (await navigator.clipboard.read())[0]?.types ?? [])).toContain('web application/x-ferrum');
  await pick(page, 200, 40);
  const l = await liveWhen(page, 'l.selectionCount === 1 && l.selectionX === 140');
  await page.keyboard.press('ControlOrMeta+Shift+KeyR');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.summary?.kind)).toBe('ellipse');
  const r = await liveWhen(page, `l.steps === ${l.steps + 1}`);
  expect([r.nodes, r.selectionX, r.selectionY]).toEqual([l.nodes, 140, 0]);
  expect(await page.evaluate(() => (window as { __marker?: number }).__marker)).toBe(7);
});

test('свойства: Ctrl/Cmd+Alt+C и Ctrl/Cmd+Alt+V — к фрейму одним шагом, что подходит его типу', async ({ page }) => {
  await openApp(page);
  // «Слой 1» — прямоугольник: скругление 8, обводка 1 внутри.
  await pick(page, 200, 40);
  await liveWhen(page, 'l.selectionCount === 1');
  const rect = await summary(page);
  await page.keyboard.press('ControlOrMeta+Alt+KeyC');
  await expect(page.getByRole('status').filter({ hasText: 'Свойства скопированы' })).toBeVisible();
  // Фрейм «Экран 0» — щелчком по его названию над левым верхним углом.
  const corner = await appScreen(page, await live(page), 0, 0);
  await page.mouse.click(corner.x + 10, corner.y - 6);
  const l = await liveWhen(page, 'l.selectionCount === 1 && l.selectionW === 1440');
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+Alt+KeyV');
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expect.poll(async () => (await summary(page)).cornerRadius?.value).toBe(8);
  const frame = await summary(page);
  expect(frame.fills).toEqual(rect.fills);
  expect(frame.strokes).toEqual(rect.strokes);
  expect([frame.w.value, frame.h.value]).toEqual([1440, 1024]);
});

test('копия свойств — текстом с меткой в системном буфере; Ctrl+V её не вставляет', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'чтение буфера — разрешение clipboard-read есть только в Chromium');
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await openApp(page);
  await pick(page, 200, 40);
  const l = await liveWhen(page, 'l.selectionCount === 1');
  await page.keyboard.press('ControlOrMeta+Alt+KeyC');
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toMatch(/^ferrum-props:v1:/);
  await page.keyboard.press('ControlOrMeta+v');
  await page.waitForTimeout(300);
  expect((await live(page)).nodes).toBe(l.nodes);
  // Из меню — свойства из системного буфера (к эллипсу «Слой 0»: скругления у него нет).
  await pick(page, 60, 40);
  const e = await liveWhen(page, 'l.selectionCount === 1 && l.selectionX === 0');
  await mainMenu(page, 'Правка', 'Вставить свойства');
  await liveWhen(page, `l.steps === ${e.steps + 1}`);
  await expect.poll(async () => (await summary(page)).kind).toBe('ellipse');
});
