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

test('«Вставить с заменой» и «Вставить поверх» из меню читают буфер; Ctrl+Shift+R — с предупреждением', async ({ page, browserName }) => {
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
  // Сочетание «Вставить с заменой» занято браузером: настройки предупреждают.
  await mainMenu(page, 'Настройки', 'Настройки…');
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('searchbox', { name: 'Поиск команд' }).fill('с заменой');
  await expect(dialog.getByRole('row').filter({ hasText: 'Вставить с заменой' })).toContainText('Перехватывает браузер');
});
