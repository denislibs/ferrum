// Отказы бэкендов рендера и самого worker'а: что видит пользователь, когда WebGPU,
// WebGL2, показ кадра или ядро не работают. Условия — настоящие, где браузер их даёт (программный
// SwiftShader, холст больше предела устройства, смена размера окна), и внедрённые
// там, где иначе не получить.
//
// Внедрение. Скрипты страницы (`addInitScript`) в worker не попадают, поэтому
// Playwright подменяет сам файл worker'а в ответе сервера: перед его кодом
// ставится «пролог» с заглушками (`prelude`). Для продукта это ничего не меняет:
// в собранном бандле нет ни хуков, ни параметров адреса для тестов — заглушки
// живут только в этом файле и только в тестовом браузере. Заглушки включаются
// условиями пролога (`Opt`) или сообщением `{ t: '__fault', name }` в worker уже
// после старта (его слушатель — в прологе, раньше обработчика worker'а: служебные
// сообщения дальше пролога не идут, иначе worker ответил бы на них ошибкой
// «неверное сообщение»).

import { expect, test, type Page } from '@playwright/test';
import { drag, drawRect, editorReady, expectPixel, fileId, gotoDebug, journalCount, live, liveWhen, open, screen, storedFile } from './helpers';

interface Opt {
  /** `getContext('webgl2')` на пробном холсте 1×1 даёт `null`. */
  noProbeContext?: boolean;
  /** Сборка шейдеров на пробном холсте 1×1 «не удаётся». */
  probeShaderFail?: boolean;
  /** `navigator.gpu.requestAdapter()` даёт `null`. */
  noAdapter?: boolean;
  /** Адаптер WebGPU называет себя SwiftShader. */
  softAdapter?: boolean;
  /** Предел стороны текстуры у WebGPU и WebGL2 (на видеокарте он 16384 — как и
   * потолок размера холста ядра, так что настоящим размером его не превысить). */
  maxTexture?: number;
  /** Сообщение `init` до worker'а не доходит: запуск ядра висит. */
  stallInit?: boolean;
}

/** Код пролога: заглушки в worker'е. Выполняется до кода worker'а. */
function prelude(opt: Opt): string {
  return `(() => {
  const OPT = ${JSON.stringify(opt)};
  const F = {};
  let gpuDevice = null;
  let realGl = null;
  // Расширение потери контекста: у потерянного контекста getExtension даёт null,
  // поэтому для возврата оно запоминается при потере (пролог — строка в обратных
  // кавычках: без них в комментариях).
  let loseExt = null;
  const fail = (what) => { throw new Error('внедрённый сбой: ' + what); };

  const getContext = OffscreenCanvas.prototype.getContext;
  OffscreenCanvas.prototype.getContext = function (type, ...rest) {
    const probe = this.width === 1 && this.height === 1;
    if (type === 'webgl2' && probe && OPT.noProbeContext) return null;
    const ctx = getContext.call(this, type, ...rest);
    if (type === 'webgl2' && !probe) realGl = ctx;
    return ctx;
  };

  if (OPT.probeShaderFail) {
    const gl = WebGL2RenderingContext.prototype;
    const shaderParam = gl.getShaderParameter;
    gl.getShaderParameter = function (shader, pname) {
      if (this.canvas.width === 1 && pname === this.COMPILE_STATUS) return false;
      return shaderParam.call(this, shader, pname);
    };
    const programParam = gl.getProgramParameter;
    gl.getProgramParameter = function (program, pname) {
      if (this.canvas.width === 1 && pname === this.LINK_STATUS) return false;
      return programParam.call(this, program, pname);
    };
  }

  if (OPT.maxTexture) {
    const getParameter = WebGL2RenderingContext.prototype.getParameter;
    WebGL2RenderingContext.prototype.getParameter = function (pname) {
      if (pname === this.MAX_TEXTURE_SIZE) return OPT.maxTexture;
      return getParameter.call(this, pname);
    };
  }

  const putImageData = OffscreenCanvasRenderingContext2D.prototype.putImageData;
  OffscreenCanvasRenderingContext2D.prototype.putImageData = function (...a) {
    if (F.putImageData) fail('putImageData');
    return putImageData.apply(this, a);
  };
  const readPixels = WebGL2RenderingContext.prototype.readPixels;
  WebGL2RenderingContext.prototype.readPixels = function (...a) {
    if (F.readPixels) fail('readPixels');
    return readPixels.apply(this, a);
  };

  if (navigator.gpu && (OPT.noAdapter || OPT.softAdapter || OPT.maxTexture)) {
    const requestAdapter = navigator.gpu.requestAdapter.bind(navigator.gpu);
    navigator.gpu.requestAdapter = async (...a) => {
      if (OPT.noAdapter) return null;
      const adapter = await requestAdapter(...a);
      if (adapter && OPT.maxTexture) {
        const requestDevice = adapter.requestDevice.bind(adapter);
        adapter.requestDevice = (desc = {}) =>
          requestDevice({ ...desc, requiredLimits: { ...desc.requiredLimits, maxTextureDimension2D: OPT.maxTexture } });
      }
      if (adapter && OPT.softAdapter) {
        const info = { vendor: 'google', architecture: 'swiftshader', device: 'SwiftShader', description: 'SwiftShader Device', subgroupMinSize: 4, subgroupMaxSize: 4, isFallbackAdapter: true };
        Object.defineProperty(adapter, 'info', { value: info });
      }
      return adapter;
    };
  } else if (navigator.gpu) {
    // Запоминаем устройство, чтобы потом сломать его снаружи.
    const requestAdapter = navigator.gpu.requestAdapter.bind(navigator.gpu);
    navigator.gpu.requestAdapter = async (...a) => {
      // После loseDeviceForGood адаптера нет: устройство заново не создать.
      if (F.loseDeviceForGood) return null;
      const adapter = await requestAdapter(...a);
      if (adapter) {
        const requestDevice = adapter.requestDevice.bind(adapter);
        adapter.requestDevice = async (...b) => (gpuDevice = await requestDevice(...b));
      }
      return adapter;
    };
  }

  self.addEventListener('message', (e) => {
    if (OPT.stallInit && e.data && e.data.t === 'init') {
      e.stopImmediatePropagation();
      return;
    }
    const name = e.data && e.data.t === '__fault' ? e.data.name : null;
    if (!name) return;
    e.stopImmediatePropagation();
    F[name] = true;
    // Исключение вне обработчиков worker'а — как настоящий сбой в коде worker'а.
    if (name === 'throw') setTimeout(() => fail('throw'));
    // Ошибка проверки WebGPU вне вызовов wgpu: событие uncapturederror устройства.
    // Команда доходит до GPU вместе с отправкой очереди: без отправки очереди событие не приходит.
    if (name === 'uncaptured') {
      gpuDevice.createTexture({ size: [1, 1], format: 'rgba8unorm', usage: 0 });
      gpuDevice.queue.submit([]);
    }
    // Потеря устройства WebGPU и контекста WebGL2.
    if (name === 'loseDevice' || name === 'loseDeviceForGood') gpuDevice.destroy();
    if (name === 'loseGl') {
      loseExt = realGl.getExtension('WEBGL_lose_context');
      loseExt.loseContext();
    }
    // Браузер вернул контекст (worker «принял» потерю — preventDefault, иначе возврата нет).
    if (name === 'restoreGl') loseExt.restoreContext();
  });
})();
`;
}

/** Подменяет файл worker'а: заглушки идут перед его кодом. */
async function stubWorker(page: Page, opt: Opt): Promise<void> {
  const code = prelude(opt);
  await page.context().route('**/assets/core.worker-*.js', async (route) => {
    const response = await route.fetch();
    await route.fulfill({ response, body: code + (await response.text()) });
  });
}

async function fault(page: Page, name: string): Promise<void> {
  await page.evaluate((name) => {
    (window.__ferrum!.core! as unknown as { worker: Worker }).worker.postMessage({ t: '__fault', name });
  }, name);
}

/** Тексты событий `error` с начала журнала. */
function errors(page: Page): Promise<string[]> {
  return page.evaluate(() =>
    window.__ferrum!.events.flatMap((e) => (e.type === 'error' ? [e.message] : [])),
  );
}

async function info(page: Page): Promise<{ backend: string | null; fallbacks: string[] }> {
  return page.evaluate(() => ({ backend: window.__ferrum!.core!.backend, fallbacks: window.__ferrum!.core!.fallbacks }));
}

/** Кадр на экране непустой: «Слой 1» первого фрейма синий, фон тёмный. */
async function expectDrawn(page: Page): Promise<void> {
  const l = await live(page);
  const at = screen(l, 200, 40);
  await expectPixel(page, at.x, at.y, [1, 120, 200]);
  // Пустой холст — токен `canvas` дизайн-системы (#1a1918).
  await expectPixel(page, 600, l.viewportH - 10, [26, 25, 24]);
}

/** Просит ещё кадр: ввод, после которого редактор перерисовывается. */
async function poke(page: Page): Promise<void> {
  const l = await live(page);
  await drag(page, screen(l, 200, 40), 6, 0, 3);
}

/** Пиксель первого события `pixels` в точке холста: бэкенд и RGB. */
function pixelsAt(page: Page, at: { x: number; y: number }): Promise<(string | number)[]> {
  return page.evaluate(({ x, y }) => {
    const p = window.__ferrum!.events.find((e) => e.type === 'pixels');
    if (!p || p.type !== 'pixels') return [];
    const i = (Math.round(y) * p.width + Math.round(x)) * 4;
    return [p.backend, ...p.rgba.slice(i, i + 3)];
  }, at);
}

/** Размер, на 1 больший предела WebGL2 (`MAX_TEXTURE_SIZE`) этого браузера. */
function pastWebglLimit(page: Page): Promise<number> {
  return page.evaluate(() => {
    const gl = new OffscreenCanvas(1, 1).getContext('webgl2')!;
    return (gl.getParameter(gl.MAX_TEXTURE_SIZE) as number) + 1;
  });
}

// --- 1. Программный WebGL2 при автовыборе ------------------------------------

test('автовыбор: программный WebGL2 отклонён, рисует CPU', async ({ page }) => {
  await open(page);
  const { backend, fallbacks } = await info(page);
  expect(backend).toBe('cpu');
  expect(fallbacks).toHaveLength(2);
  expect(fallbacks[0]).toMatch(/^webgpu: нет адаптера/);
  expect(fallbacks[1]).toMatch(/^webgl: программный рендер \(.*SwiftShader/);
  // Холст получил 2D и кадры не пустые.
  expect((await live(page)).frames).toBeGreaterThan(0);
  await expectDrawn(page);
  expect(await errors(page)).toEqual([]);
});

// --- 2. Отказ пробы WebGL2: настоящий холст остаётся свободным ----------------

test('проба WebGL2 не дала контекст: холст свободен, рисует CPU', async ({ page }) => {
  await stubWorker(page, { noProbeContext: true });
  await open(page, '&backend=webgl');
  const { backend, fallbacks } = await info(page);
  expect(backend).toBe('cpu');
  expect(fallbacks).toHaveLength(1);
  expect(fallbacks[0]).toMatch(/^webgl: WebGL2 недоступен/);
  await expectDrawn(page);
  expect(await errors(page)).toEqual([]);
});

test('проба WebGL2 не собрала шейдеры: холст свободен, рисует CPU', async ({ page }) => {
  await stubWorker(page, { probeShaderFail: true });
  await open(page, '&backend=webgl');
  const { backend, fallbacks } = await info(page);
  expect(backend).toBe('cpu');
  expect(fallbacks).toHaveLength(1);
  expect(fallbacks[0]).toMatch(/^webgl: WebGL2 недоступен/);
  await expectDrawn(page);
});

test('много запусков и остановок ядра: контексты WebGL2 не копятся', async ({ page }) => {
  const warnings: string[] = [];
  page.on('console', (m) => warnings.push(m.text()));
  page.on('worker', (w) => (w as unknown as { on(e: 'console', f: (m: { text(): string }) => void): void }).on?.('console', (m) => warnings.push(m.text())));
  await open(page, '&backend=webgl');
  // Каждое ядро берёт пробный контекст и настоящий; после остановки оба свободны.
  const results = await page.evaluate(async () => {
    const Core = window.__ferrum!.core!.constructor as typeof import('../src/core/core').Core;
    const out: { backend: string | null; fallbacks: string[] }[] = [];
    for (let i = 0; i < 24; i++) {
      const canvas = document.createElement('canvas');
      canvas.width = 200;
      canvas.height = 100;
      document.body.append(canvas);
      const core = await Core.start(canvas, { backend: 'webgl' });
      out.push({ backend: core.backend, fallbacks: core.fallbacks });
      core.dispose();
      canvas.remove();
    }
    return out;
  });
  expect(results.filter((r) => r.backend !== 'webgl' || r.fallbacks.length > 0)).toEqual([]);
  expect(warnings.filter((w) => /too many|слишком много/i.test(w))).toEqual([]);
});

// --- 3. Холст больше предела устройства ------------------------------------

test('холст больше MAX_TEXTURE_SIZE при создании: WebGL2 отклонён, рисует CPU', async ({ page }) => {
  await gotoDebug(page, '/debug.html?nodes=10');
  const width = await pastWebglLimit(page);
  await page.setViewportSize({ width, height: 200 });
  await open(page, '&backend=webgl');
  const { backend, fallbacks } = await info(page);
  expect(backend).toBe('cpu');
  expect(fallbacks).toEqual([`webgl: холст ${width} × 200 больше предела устройства (${width - 1})`]);
  expect((await live(page)).frames).toBeGreaterThan(0);
  expect(await errors(page)).toEqual([]);
});

// --- 4. Отказ WebGPU до поверхности ------------------------------------------

test('WebGPU без адаптера: холст свободен, рисует CPU', async ({ page }) => {
  await open(page, '&backend=webgpu');
  const { backend, fallbacks } = await info(page);
  expect(backend).toBe('cpu');
  expect(fallbacks).toHaveLength(1);
  expect(fallbacks[0]).toMatch(/^webgpu: нет адаптера/);
  await expectDrawn(page);
});

test('WebGPU без адаптера на GPU: холст свободен, рисует WebGL2 @gpu', async ({ page }) => {
  await stubWorker(page, { noAdapter: true });
  await open(page);
  const { backend, fallbacks } = await info(page);
  expect(backend).toBe('webgl');
  expect(fallbacks).toHaveLength(1);
  expect(fallbacks[0]).toMatch(/^webgpu: нет адаптера/);
  await expectDrawn(page);
});

test('программный адаптер WebGPU при автовыборе отклонён, рисует WebGL2 @gpu', async ({ page }) => {
  await stubWorker(page, { softAdapter: true });
  await open(page);
  const { backend, fallbacks } = await info(page);
  expect(backend).toBe('webgl');
  expect(fallbacks).toHaveLength(1);
  expect(fallbacks[0]).toMatch(/^webgpu: только программный адаптер \(SwiftShader/);
  await expectDrawn(page);
});

test('холст больше предела WebGPU и WebGL2: оба отклонены, рисует CPU @gpu', async ({ page }) => {
  // На видеокарте оба предела — 16384, а ядро не делает холст больше 16384, так
  // что предел занижен заглушкой до 8192 (минимум WebGPU).
  await stubWorker(page, { maxTexture: 8192 });
  await page.setViewportSize({ width: 8193, height: 200 });
  await open(page);
  const { backend, fallbacks } = await info(page);
  expect(backend).toBe('cpu');
  expect(fallbacks).toEqual([
    'webgpu: холст 8193 × 200 больше предела устройства (8192)',
    'webgl: холст 8193 × 200 больше предела устройства (8192)',
  ]);
  expect((await live(page)).frames).toBeGreaterThan(0);
  expect(await errors(page)).toEqual([]);
});

// --- 5. Ошибка показа кадра ---------------------------------------------------

/** После ошибки показа: ровно одно событие, кадры больше не рисуются, чтение
 * пикселей отвечает ошибкой. */
async function expectDead(page: Page, text: RegExp): Promise<void> {
  await expect.poll(async () => (await errors(page)).length).toBe(1);
  expect((await errors(page))[0]).toMatch(text);
  const frames = (await live(page)).frames;
  // Ввод, который раньше вызвал бы перерисовку, кадров не добавляет и ошибок не плодит.
  await poke(page);
  await page.waitForTimeout(500);
  expect((await live(page)).frames).toBe(frames);
  expect(await errors(page)).toHaveLength(1);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'readPixels', reference: false }));
  await expect.poll(async () => (await errors(page)).length).toBe(2);
  expect((await errors(page))[1]).toBe('Пиксели не прочитать: поверхность не рисует');
  expect(await page.evaluate(() => window.__ferrum!.events.filter((e) => e.type === 'pixels').length)).toBe(0);
  expect((await live(page)).frames).toBe(frames);
}

test('ошибка показа кадра на CPU: одно сообщение, поверхность не рисует', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=cpu');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'putImageData');
  await poke(page);
  await expectDead(page, /^Не удалось показать кадр: .*внедрённый сбой: putImageData/);
});

test('ошибка показа кадра на WebGL2: одно сообщение, поверхность не рисует', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=webgl');
  expect((await info(page)).backend).toBe('webgl');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'readPixels');
  // Чтение пикселей — единственная часть показа WebGL2, которая может вернуть ошибку.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'readPixels', reference: false }));
  // Запрос пикселей не обслужен: он возвращается в очередь и получает отказ —
  // это второе (и последнее) событие.
  await expect.poll(async () => (await errors(page)).length).toBe(2);
  const [shown, pixels] = await errors(page);
  expect(shown).toMatch(/^Не удалось показать кадр: .*внедрённый сбой: readPixels/);
  expect(pixels).toBe('Пиксели не прочитать: поверхность не рисует');
  const frames = (await live(page)).frames;
  await poke(page);
  await page.waitForTimeout(500);
  expect((await live(page)).frames).toBe(frames);
  expect(await errors(page)).toHaveLength(2);
});

// --- 6. Размер больше предела после старта ------------------------------------

test('окно выросло больше предела WebGL2: одно сообщение, пауза, после уменьшения рисует снова', async ({ page }) => {
  await open(page, '&backend=webgl');
  expect((await info(page)).backend).toBe('webgl');
  const width = await pastWebglLimit(page);
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await page.setViewportSize({ width, height: 720 });
  await expect.poll(async () => (await errors(page)).length).toBe(1);
  expect((await errors(page))[0]).toBe(
    `Кадр не показан: ${width} × 720 больше предела устройства (${width - 1}); уменьшите окно`,
  );
  // Перерисовка не зациклена: кадров нет, сообщений больше нет.
  const frames = (await live(page)).frames;
  await page.waitForTimeout(500);
  expect((await live(page)).frames).toBe(frames);
  // Чтение пикселей, пока кадр не показать, отвечает ошибкой, а не перерисовкой
  // каждый кадр (запрос не возвращается в очередь).
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'readPixels', reference: false }));
  await expect.poll(async () => (await errors(page)).length).toBe(2);
  expect((await errors(page))[1]).toBe('Пиксели не прочитать: кадр не показан');
  const paused = (await live(page)).frames;
  await page.waitForTimeout(500);
  expect((await live(page)).frames).toBe(paused);
  // Окно уменьшили — кадры снова идут, и пиксели читаются.
  await page.setViewportSize({ width: 1280, height: 720 });
  await liveWhen(page, `l.viewportW === 1280 && l.frames > ${paused}`);
  const l = await live(page);
  const at = screen(l, 200, 40);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'readPixels', reference: false }));
  await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'pixels'));
  expect(await pixelsAt(page, at)).toEqual(['webgl', 1, 120, 200]);
  expect(await errors(page)).toHaveLength(2);
});

// --- 7. Ошибки WebGPU вне вызовов --------------------------------------------

test('ошибка проверки WebGPU вне вызовов: сообщение, рисование продолжается @gpu', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=webgpu');
  expect((await info(page)).backend).toBe('webgpu');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'uncaptured');
  await expect.poll(async () => (await errors(page)).length).toBe(1);
  expect((await errors(page))[0]).toMatch(/^Ошибка WebGPU: /);
  // Та же ошибка на следующих кадрах — без новых сообщений. Ошибка проверки не
  // смертельна: устройство цело, кадры идут.
  for (let i = 0; i < 2; i++) {
    const frames = (await live(page)).frames;
    await fault(page, 'uncaptured');
    await poke(page);
    await liveWhen(page, `l.frames > ${frames}`);
  }
  await page.waitForTimeout(300);
  expect(await errors(page)).toHaveLength(1);
  // Поверхность жива: пиксели читаются и «Слой 1» на месте (холст WebGPU снимок
  // страницы в headless может не показать, поэтому читаем из GPU).
  const l = await live(page);
  const at = screen(l, 200, 40);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'readPixels', reference: false }));
  await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'pixels'));
  expect(await pixelsAt(page, at)).toEqual(['webgpu', 1, 120, 200]);
  expect(await errors(page)).toHaveLength(1);
});

/** Первое событие вида `type` в журнале стенда (ждёт его). */
async function firstEvent(page: Page, type: string): Promise<Record<string, unknown>> {
  await page.waitForFunction((t) => window.__ferrum!.events.some((e) => e.type === t), type, { timeout: 15_000 });
  return page.evaluate((t) => window.__ferrum!.events.find((e) => e.type === t) as unknown as Record<string, unknown>, type);
}

test('потеря устройства WebGPU: устройство создано заново, рисует снова @gpu', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=webgpu');
  expect((await info(page)).backend).toBe('webgpu');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'loseDevice');
  const lost = await firstEvent(page, 'surfaceLost');
  expect(lost.backend).toBe('webgpu');
  expect(lost.message).toMatch(/^Устройство WebGPU потеряно \(.*\): /);
  expect(await firstEvent(page, 'surfaceRestored')).toEqual({ type: 'surfaceRestored', backend: 'webgpu' });
  await poke(page);
  // Новое устройство рисует: пиксели — из GPU (снимок страницы в headless холст
  // WebGPU может не показать, а последний кадр старого устройства на нём остаётся).
  const at = screen(await live(page), 200, 40);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'readPixels', reference: false }));
  await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'pixels'));
  expect(await pixelsAt(page, at)).toEqual(['webgpu', 1, 120, 200]);
  expect(await errors(page)).toEqual([]);
});

test('потеря контекста WebGL2 и его возврат: поверхность создана заново, рисует снова', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=webgl');
  expect((await info(page)).backend).toBe('webgl');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'loseGl');
  expect((await firstEvent(page, 'surfaceLost')).message).toMatch(/^Контекст WebGL2 потерян/);
  await fault(page, 'restoreGl');
  expect(await firstEvent(page, 'surfaceRestored')).toEqual({ type: 'surfaceRestored', backend: 'webgl' });
  await poke(page);
  await expectDrawn(page);
  expect(await errors(page)).toEqual([]);
  expect(await page.evaluate(() => window.__ferrum!.events.some((e) => e.type === 'surfaceFailed'))).toBe(false);
});

test('контекст WebGL2 не вернулся: запасной бэкенд CPU', async ({ page }) => {
  await stubWorker(page, {});
  await open(page, '&backend=webgl');
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'loseGl');
  const failed = await firstEvent(page, 'surfaceFailed');
  expect(failed.next).toBe('cpu');
  expect(failed.message).toMatch(/^Контекст WebGL2 потерян; пересоздать не удалось: контекст WebGL2 не восстановлен$/);
  expect(await errors(page)).toEqual([]);
});

/** Пометить ядро редактора: после перезапуска `newCore` ждёт другое. */
async function markOld(page: Page): Promise<void> {
  await page.evaluate(() => ((window.__ferrum!.core as unknown as { old?: boolean }).old = true));
}

/** Новое ядро редактора готово (старое помечено `markOld`). */
async function newCore(page: Page): Promise<void> {
  await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
  await editorReady(page);
}

test('редактор: WebGL2 потерян навсегда — перезапуск на процессоре, документ на месте', async ({ page }) => {
  await stubWorker(page, {});
  // Пачки журнала сами не уходят (journal — 10 мин): правка попадает в журнал только
  // записью перед перезапуском.
  const id = await storedFile(page, '&backend=webgl&fullSave=600000&journal=600000');
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('webgl');
  await drawRect(page, 150);
  expect(await journalCount(page, id)).toBe(0);
  await markOld(page);
  await fault(page, 'loseGl');
  await expect(
    page.locator('.fe-toast', { hasText: 'Видеокарта недоступна — редактор перезапущен с рендером на процессоре' }),
  ).toHaveCount(1, { timeout: 15_000 });
  await newCore(page);
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('cpu');
  // Правка до потери ушла в журнал перед перезапуском.
  await liveWhen(page, 'l.nodes === 2');
  expect(fileId(page)).toBe(id);
});

test('редактор: после перезапуска на процессоре сбой ядра оставляет CPU; предел перезапусков общий с потерей GPU', async ({ page }) => {
  await stubWorker(page, {});
  await storedFile(page, '&backend=webgl');
  // Первый перезапуск — потеря GPU.
  await markOld(page);
  await fault(page, 'loseGl');
  await expect(page.locator('.fe-toast', { hasText: 'Видеокарта недоступна' })).toHaveCount(1, { timeout: 15_000 });
  await newCore(page);
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('cpu');
  // Второй — сбой ядра: бэкенд остаётся запасным.
  await markOld(page);
  await page.evaluate(() => window.__ferrum!.core!.testPanic());
  await expect(page.locator('.fe-toast', { hasText: 'Редактор перезапущен' })).toHaveCount(1);
  await newCore(page);
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('cpu');
  // Третий за минуту — уже нет: плашка остановки.
  await page.evaluate(() => window.__ferrum!.core!.testPanic());
  await expect(page.getByRole('alert')).toContainText('Ядро остановлено: паника в ядре: тестовая паника ядра');
});

test('редактор: перезапуски исчерпаны, WebGL2 потерян навсегда — плашка «Рендер остановлен», правка записана, уход работает', async ({ page }) => {
  await stubWorker(page, {});
  const id = await storedFile(page, '&backend=webgl&fullSave=600000&journal=600000');
  // Два перезапуска за минуту (сбои ядра) — предел исчерпан.
  for (let i = 0; i < 2; i++) {
    await markOld(page);
    await page.evaluate(() => window.__ferrum!.core!.testPanic());
    await newCore(page);
  }
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('webgl');
  await drawRect(page, 150);
  await markOld(page);
  await fault(page, 'loseGl');
  await expect(page.getByRole('alert')).toContainText(
    'Рендер остановлен: Контекст WebGL2 потерян; пересоздать не удалось: контекст WebGL2 не восстановлен. Правки больше не принимаются',
    { timeout: 15_000 },
  );
  // Перезапуска нет; ядро остановлено — ввод в него не идёт.
  expect(await page.evaluate(() => (window.__ferrum!.core as unknown as { old?: boolean; stopped: boolean }).stopped)).toBe(true);
  // Правка до остановки записана: несохранённого нет.
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  // Уход — без вопроса; файл открывается снова с правкой.
  await page.evaluate(() => (location.hash = '#/'));
  await expect(page.locator('.fe-files')).toHaveCount(1);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.evaluate((i) => (location.hash = `#/file/${encodeURIComponent(i)}`), id);
  await newCore(page);
  await liveWhen(page, 'l.nodes === 2');
});

test('редактор: WebGPU потерян навсегда, WebGL2 не запустился — уведомление называет рендер нового ядра @gpu', async ({ page }) => {
  // В новом worker'е проба WebGL2 не даёт контекста: принудительный WebGL2 уступает CPU.
  await stubWorker(page, { noProbeContext: true });
  const warnings: string[] = [];
  page.on('console', (m) => m.type() === 'warning' && warnings.push(m.text()));
  await storedFile(page, '&backend=webgpu');
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('webgpu');
  await markOld(page);
  await fault(page, 'loseDeviceForGood');
  await expect(
    page.locator('.fe-toast', { hasText: 'Видеокарта недоступна — редактор перезапущен с рендером на процессоре' }),
  ).toHaveCount(1, { timeout: 15_000 });
  await newCore(page);
  expect(await page.evaluate(() => window.__ferrum!.core!.backend)).toBe('cpu');
  await liveWhen(page, 'l.nodes === 1');
  // Причина потери — в консоли (предупреждение о потере, до попыток пересоздать).
  expect(warnings.some((t) => /^Устройство WebGPU потеряно/.test(t))).toBe(true);
});

// --- 8. Сбои самого worker'а ----------------------------------------------------

test('исключение в worker\'е после запуска: одно сообщение с причиной, команды отвечают ошибкой', async ({ page }) => {
  await stubWorker(page, {});
  await open(page);
  await page.evaluate(() => (window.__ferrum!.events.length = 0));
  await fault(page, 'throw');
  await expect.poll(async () => (await errors(page)).length).toBe(1);
  // Слушатель `error` worker'а и `onerror` в основном потоке видят одно исключение:
  // сообщение должно быть одно.
  await page.waitForTimeout(500);
  expect(await errors(page)).toEqual(['Ядро остановлено: внедрённый сбой: throw']);
  await expect(page.locator('#hud')).toContainText('внедрённый сбой: throw');
  // Команда остановленному ядру не теряется молча.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'save' }));
  await expect.poll(async () => (await errors(page)).length).toBe(2);
  expect((await errors(page))[1]).toBe('Ядро остановлено, команда не выполнена');
  expect(await page.evaluate(() => window.__ferrum!.events.some((e) => e.type === 'saved'))).toBe(false);
});

test('worker не ответил на запуск: start отклоняется по таймауту, worker остановлен', async ({ page }) => {
  await stubWorker(page, { stallInit: true });
  let closed = false;
  page.on('worker', (w) => w.on('close', () => (closed = true)));
  await gotoDebug(page, '/debug.html?nodes=10');
  await expect(page.locator('#hud')).toHaveText('Ядро не запустилось: истекло время ожидания', { timeout: 40_000 });
  await expect.poll(() => closed).toBe(true);
  expect(await page.evaluate(() => window.__ferrum === undefined)).toBe(true);
});
