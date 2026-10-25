// Стенд этапа 4 в headless Chromium: worker стартует, транспорт выбирается по
// изоляции страницы, холст показывает документ, ввод доходит до редактора,
// отмена работает, бэкенд выбирается автоматически, ресайз и dpr применяются.

import { expect, test, type Page } from '@playwright/test';
import { drag, events, expectPixel, gotoDebug, live, liveWhen, open, screen } from './helpers';

// Токен `canvas` дизайн-системы в тёмной теме (#1a1918).
const CANVAS_DARK = [26, 25, 24];

async function dragMovesAndUndoReturns(page: Page): Promise<void> {
  const l = await live(page);
  // «Слой 1» первого фрейма: (140, 0) – (260, 80). Сдвиг на (50, 30) CSS-пикселей
  // при зуме 0.5 — (100, 60) в мире.
  await drag(page, screen(l, 200, 40), 50, 30);
  const moved = await liveWhen(page, 'l.steps === 1 && l.busy === 0');
  expect([moved.selectionCount, moved.selectionX, moved.selectionY]).toEqual([1, 240, 60]);
  expect([moved.selectionW, moved.selectionH, moved.canUndo]).toEqual([120, 80, 1]);
  expect(await events(page, 'document')).toBeGreaterThanOrEqual(2);
  await page.keyboard.press('ControlOrMeta+z');
  const back = await liveWhen(page, 'l.canRedo === 1');
  expect([back.selectionX, back.selectionY, back.canUndo]).toEqual([140, 0, 0]);
}

test('стенд запускает worker и транспорт на общей памяти', async ({ page }) => {
  await open(page);
  const info = await page.evaluate(() => ({
    isolated: crossOriginIsolated,
    mode: window.__ferrum!.core!.mode,
    backend: window.__ferrum!.core!.backend,
    errors: window.__ferrum!.events.filter((e) => e.type === 'error'),
  }));
  expect(info).toMatchObject({ isolated: true, mode: 'sab', errors: [] });
  expect(['webgpu', 'webgl', 'cpu']).toContain(info.backend);
  const l = await live(page);
  expect([l.nodes, l.viewportW, l.viewportH, l.zoom]).toEqual([1000, 1280, 720, 0.5]);
});

test('холст показывает документ', async ({ page }) => {
  await open(page);
  const l = await live(page);
  const rect = screen(l, 200, 40);
  await expectPixel(page, rect.x, rect.y, [1, 120, 200]);
  await expectPixel(page, 600, l.viewportH - 10, CANVAS_DARK);
});

test('перетаскивание двигает узел, отмена возвращает', async ({ page }) => {
  await open(page);
  await dragMovesAndUndoReturns(page);
  // На новом месте после повтора — тот же цвет узла.
  await page.keyboard.press('ControlOrMeta+Shift+z');
  const l = await liveWhen(page, 'l.canUndo === 1 && l.selectionX === 240');
  const p = screen(l, 300, 100);
  await expectPixel(page, p.x, p.y, [1, 120, 200]);
});

test('без заголовков изоляции работает запасной транспорт postMessage', async ({ page }) => {
  await open(page, '', 'http://localhost:4174');
  const info = await page.evaluate(() => ({ isolated: crossOriginIsolated, mode: window.__ferrum!.core!.mode }));
  expect(info).toEqual({ isolated: false, mode: 'message' });
  await dragMovesAndUndoReturns(page);
});

test('postMessage можно выбрать и на изолированной странице', async ({ page }) => {
  await open(page, '&transport=message');
  expect(await page.evaluate(() => window.__ferrum!.core!.mode)).toBe('message');
  await dragMovesAndUndoReturns(page);
});

test('в режиме postMessage ресайз и ввод доходят до worker\'а', async ({ page }) => {
  await open(page, '', 'http://localhost:4174');
  expect(await page.evaluate(() => window.__ferrum!.core!.mode)).toBe('message');
  const before = await live(page);
  await page.setViewportSize({ width: 900, height: 500 });
  const l = await liveWhen(page, 'l.viewportW === 900 && l.viewportH === 500');
  expect([l.deviceW, l.deviceH]).toEqual([900, 500]);
  // Колесо с Ctrl — зум: запись ввода дошла и разобралась.
  await page.mouse.move(450, 250);
  await page.keyboard.down('Control');
  await page.mouse.wheel(0, -200);
  await page.keyboard.up('Control');
  const zoomed = await liveWhen(page, `l.zoom > ${before.zoom}`);
  expect(zoomed.inputs).toBeGreaterThan(before.inputs);
  expect(await page.evaluate(() => window.__ferrum!.core!.droppedInput())).toBe(0);
});

test('без WebGPU выбирается запасной бэкенд', async ({ page }) => {
  await open(page);
  const { backend, fallbacks } = await page.evaluate(() => ({
    backend: window.__ferrum!.core!.backend,
    fallbacks: window.__ferrum!.core!.fallbacks,
  }));
  // В headless Chromium без флагов адаптера WebGPU нет, а WebGL2 — программный
  // (SwiftShader): автовыбор доходит до vello_cpu.
  expect(backend).toBe('cpu');
  expect(fallbacks).toHaveLength(2);
  expect(fallbacks[0]).toMatch(/^webgpu: /);
  expect(fallbacks[1]).toMatch(/^webgl: программный рендер \(.*SwiftShader/);
  expect((await live(page)).backend).toBe(3);
});

test('явно запрошенный WebGL2 берётся и программным', async ({ page }) => {
  await open(page, '&backend=webgl');
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('webgl');
  await dragMovesAndUndoReturns(page);
});

test('WebGPU выбирается, если доступен @gpu', async ({ page }) => {
  await open(page);
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('webgpu');
  await dragMovesAndUndoReturns(page);
});

test('ресайз окна меняет размер холста', async ({ page }) => {
  await open(page);
  await page.setViewportSize({ width: 900, height: 500 });
  const l = await liveWhen(page, 'l.viewportW === 900 && l.viewportH === 500');
  expect([l.deviceW, l.deviceH, l.dpr]).toEqual([900, 500, 1]);
  // Камера сохранила центр: «Слой 25» первого фрейма ((700, 200) – (820, 280),
  // заливка (25, 120, 200)) — там, где его ждёт пересчёт по новому размеру.
  const p = screen(l, 760, 240);
  await expectPixel(page, p.x, p.y, [25, 120, 200]);
});

test.describe('плотность экрана 2', () => {
  test.use({ deviceScaleFactor: 2 });

  test('холст в пикселях устройства', async ({ page }) => {
    await open(page);
    const l = await live(page);
    expect([l.dpr, l.deviceW, l.deviceH]).toEqual([2, 2560, 1440]);
    await page.evaluate(() => window.__ferrum!.core!.send({ type: 'readPixels', reference: false }));
    await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'pixels'));
    const size = await page.evaluate(() => {
      const e = window.__ferrum!.events.find((e) => e.type === 'pixels')!;
      return e.type === 'pixels' ? [e.width, e.height, e.rgba.length] : [];
    });
    expect(size).toEqual([2560, 1440, 2560 * 1440 * 4]);
  });
});

test('копирование и вставка идут через события буфера обмена', async ({ page }) => {
  await open(page);
  const l = await live(page);
  const p = screen(l, 200, 40);
  await page.mouse.click(p.x, p.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.keyboard.press('ControlOrMeta+c');
  await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'clipboard' && e.bytes.length > 8));
  const magic = await page.evaluate(() => {
    const e = window.__ferrum!.events.find((e) => e.type === 'clipboard');
    return e && e.type === 'clipboard' ? new TextDecoder().decode(e.bytes.subarray(0, 8)) : '';
  });
  expect(magic).toBe('FERRUMCB');
  await page.keyboard.press('ControlOrMeta+v');
  await liveWhen(page, 'l.nodes === 1001 && l.steps === 1');
  // «Скопировать как PNG» — PNG выделения из worker'а.
  await page.keyboard.press('ControlOrMeta+Shift+c');
  await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'clipboardPng'));
  const png = await page.evaluate(() => {
    const e = window.__ferrum!.events.find((e) => e.type === 'clipboardPng');
    return e && e.type === 'clipboardPng' ? Array.from(e.bytes.subarray(1, 4)) : [];
  });
  expect(String.fromCharCode(...png)).toBe('PNG');
});

test('неверная команда не роняет worker', async ({ page }) => {
  await open(page);
  // Команды из основного потока не доверенные: форма проверяется в worker'е.
  await page.evaluate(() => window.__ferrum!.core!.send(JSON.parse('{"type":"formatDisk"}')));
  await page.waitForFunction(() =>
    window.__ferrum!.events.some((e) => e.type === 'error' && e.message.startsWith('Неверная команда')),
  );
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'execute', command: 'view.zoom_in' }));
  const l = await liveWhen(page, 'l.zoom > 0.5');
  expect(l.nodes).toBe(1000);
});

test('неверный конверт сообщения не роняет worker', async ({ page }) => {
  await open(page);
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  // Сообщения worker'у не доверенные: не объект, неизвестный вид, команда не объект.
  await page.evaluate(() => {
    const worker = (window.__ferrum!.core! as unknown as { worker: Worker }).worker;
    for (const m of [null, { t: 'cmd', cmd: null }, { t: 'cmd', cmd: 42 }, {}, { t: 'init' }]) worker.postMessage(m);
  });
  const errors = () =>
    page.evaluate(() => window.__ferrum!.events.flatMap((e) => (e.type === 'error' ? [e.message] : [])));
  await expect.poll(async () => (await errors()).length).toBe(5);
  const [nul, cmdNull, cmd42, empty, init] = await errors();
  expect(nul).toBe('Неверное сообщение для ядра');
  expect(cmdNull).toMatch(/^Неверная команда/);
  expect(cmd42).toMatch(/^Неверная команда/);
  expect(empty).toBe('Неверное сообщение для ядра');
  expect(init).toBe('Ядро уже запущено: повторный init пропущен');
  // Ядро работает дальше.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'execute', command: 'view.zoom_in' }));
  const l = await liveWhen(page, 'l.zoom > 0.5');
  expect(l.nodes).toBe(1000);
  expect((await errors()).filter((m) => m.startsWith('Ядро остановлено'))).toEqual([]);
});

// Сбои worker'а не должны вешать запуск и не должны быть тихими.
test.describe('сбои worker\'а', () => {
  test('ядро не загрузилось: плашка показывает причину, start не висит', async ({ page }) => {
    await page.route('**/*.wasm', (route) => route.fulfill({ status: 200, contentType: 'application/wasm', body: 'не wasm' }));
    await gotoDebug(page, '/debug.html?nodes=10');
    // Причина — ошибка компиляции wasm, а не только «не запустилось».
    await expect(page.locator('#hud')).toHaveText(/^Ядро не запустилось: WebAssembly\.\w+\(\): expected magic word/, {
      timeout: 20_000,
    });
    expect(await page.evaluate(() => window.__ferrum === undefined)).toBe(true);
  });

  test('скрипт worker\'а не загрузился: onerror отклоняет start', async ({ page }) => {
    await page.route('**/assets/core.worker-*.js', (route) =>
      route.fulfill({ status: 200, contentType: 'text/javascript', body: 'это не javascript (' }),
    );
    await gotoDebug(page, '/debug.html?nodes=10');
    await expect(page.locator('#hud')).toHaveText('Ядро не запустилось: скрипт worker\'а не загрузился', {
      timeout: 20_000,
    });
  });

  test('сбой после запуска приходит событием error и попадает на плашку', async ({ page }) => {
    await open(page);
    await page.evaluate(() => {
      const worker = (window.__ferrum!.core! as unknown as { worker: Worker }).worker;
      worker.dispatchEvent(new ErrorEvent('error', { message: 'проверочный сбой' }));
    });
    await page.waitForFunction(() =>
      window.__ferrum!.events.some((e) => e.type === 'error' && e.message === 'Ядро остановлено: проверочный сбой'),
    );
    await expect(page.locator('#hud')).toContainText('проверочный сбой');
  });

  test('неверные параметры адреса игнорируются', async ({ page }) => {
    await open(page, '&backend=вулкан&transport=пигеон&atlas=abc');
    expect(await page.evaluate(() => window.__ferrum!.core!.mode)).toBe('sab');
  });
});
