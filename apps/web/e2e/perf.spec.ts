// Основной поток тонкий: на странице из 100 000 узлов 5 секунд панорамирования,
// зума (до всей страницы — растры фреймов в атласе GPU) и перетаскивания не дают
// длинных задач (> 50 мс) в основном потоке, а обработчики ввода и кадра
// укладываются в 4 мс на кадр. Время кадров worker'а (медиана и 95-й процентиль)
// печатается по бэкендам и пишется в test-results/<проект>/perf-<проект>-<бэкенд>.json.
//
// Время кадра worker'а — подготовка кадра на CPU и отправка в GPU; исполнение на
// GPU в него не входит (оно асинхронное). Поэтому цель 60 fps (p95 ≤ 16,7 мс)
// проверяется для vello_cpu и для WebGL2 на настоящем GPU, а WebGPU — справочно.

import { expect, test, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { expectCore, gotoDebug, live, liveWhen, screen } from './helpers';

const NODES = 100_000;
/** Бюджет основного потока на кадр (спека 4.2). */
const MAIN_BUDGET_MS = 4;
/** 60 fps: 95-й процентиль времени кадра worker'а (спека 3). */
const WORKER_P95_MS = 16.7;

/** Длинная задача основного потока: начало от старта замера и длительность, мс. */
interface LongTask {
  startMs: number;
  durationMs: number;
}

interface Sample {
  backend: string;
  longTasks: LongTask[];
  mainMaxMs: number;
  mainP95Ms: number;
  mainFrames: number;
  workerFrames: number;
  workerP50: number;
  workerP95: number;
  workerMax: number;
  openMs: number;
  dropped: number;
  /** Наибольшее число картинок в атласе GPU за замер (растры фреймов при зуме). */
  atlasImages: number;
  atlasMisses: number;
  minZoom: number;
  /** Сохранение и открытие файла страницы в worker'е: размер, время до события
   * `opened` и до первого кадра. */
  fileMb: number;
  saveMs: number;
  fileOpenMs: number;
  fileFrameMs: number;
}

/** Сохранить документ и открыть его снова — всё в worker'е; основной поток только
 * передаёт байты (transferable). */
async function saveAndOpen(page: Page): Promise<{ fileMb: number; saveMs: number; fileOpenMs: number; fileFrameMs: number }> {
  return page.evaluate(async () => {
    const f = window.__ferrum!;
    const until = (ok: () => boolean) =>
      new Promise<void>((resolve) => {
        const check = () => (ok() ? resolve() : requestAnimationFrame(check));
        check();
      });
    f.events.length = 0;
    const t0 = performance.now();
    f.core!.send({ type: 'save' });
    await until(() => f.events.some((e) => e.type === 'saved'));
    const saveMs = performance.now() - t0;
    const saved = f.events.find((e) => e.type === 'saved');
    if (!saved || saved.type !== 'saved') throw new Error('нет сохранения');
    const fileMb = saved.bytes.length / 1e6;
    f.events.length = 0;
    const frames = f.core!.live!.frames;
    const t1 = performance.now();
    f.core!.send({ type: 'open', bytes: saved.bytes });
    await until(() => f.events.some((e) => e.type === 'opened'));
    const fileOpenMs = performance.now() - t1;
    await until(() => f.core!.live!.frames > frames);
    return { fileMb, saveMs, fileOpenMs, fileFrameMs: performance.now() - t1 };
  });
}

/** Повторяет шаг ~60 раз в секунду в течение `ms`. */
async function during(page: Page, ms: number, step: (i: number) => Promise<void>): Promise<void> {
  const end = Date.now() + ms;
  for (let i = 0; Date.now() < end; i++) {
    await step(i);
    await page.waitForTimeout(16);
  }
}

/** 5 секунд: панорамирование, зум к курсору, перетаскивание узла. */
async function interact(page: Page): Promise<void> {
  const l = await live(page);
  await page.mouse.move(l.viewportW / 2, l.viewportH / 2);
  await during(page, 1500, (i) => page.mouse.wheel(i % 60 < 30 ? 12 : -12, i % 30 < 15 ? 8 : -8));
  // Ctrl + колесо — зум, как щипок тачпада: отдаление и обратно.
  await page.keyboard.down('Control');
  await during(page, 1500, (i) => page.mouse.wheel(0, i % 40 < 20 ? 20 : -20));
  await page.keyboard.up('Control');
  const from = screen(await live(page), 760, 240);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await during(page, 2000, (i) => page.mouse.move(from.x + (i % 100) * 2, from.y + Math.sin(i / 10) * 40));
  await page.mouse.up();
}

async function measure(page: Page, query: string): Promise<Sample> {
  await page.addInitScript(() => {
    const w = window as unknown as { __longTasks: { start: number; duration: number }[] };
    w.__longTasks = [];
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) w.__longTasks.push({ start: e.startTime, duration: e.duration });
    }).observe({ type: 'longtask', buffered: true });
  });
  const t0 = Date.now();
  await gotoDebug(page, `/debug.html?nodes=${NODES}${query}`);
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 120_000 });
  await page.waitForFunction(() => (window.__ferrum!.core!.live?.frames ?? 0) > 0, null, { timeout: 60_000 });
  const openMs = Date.now() - t0;
  // Время кадров worker'а — из живого состояния, на каждом новом кадре.
  const start = await page.evaluate(() => {
    const w = window as unknown as { __frameMs: number[] };
    w.__frameMs = [];
    let seen = window.__ferrum!.core!.live!.frames;
    const z = window as unknown as { __minZoom: number; __atlasMax: number };
    z.__minZoom = Infinity;
    z.__atlasMax = 0;
    const tick = () => {
      const l = window.__ferrum!.core!.live!;
      z.__minZoom = Math.min(z.__minZoom, l.zoom);
      z.__atlasMax = Math.max(z.__atlasMax, l.atlasImages);
      if (l.frames !== seen) {
        seen = l.frames;
        w.__frameMs.push(l.frameMs);
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    window.__ferrum!.core!.resetMainThreadStats();
    return performance.now();
  });
  await interact(page);
  const file = await saveAndOpen(page);
  await page.waitForTimeout(300);
  const sample = await page.evaluate((start) => {
    const w = window as unknown as { __longTasks: { start: number; duration: number }[]; __frameMs: number[] };
    const core = window.__ferrum!.core!;
    const main = core.mainThreadStats();
    const frames = [...w.__frameMs].sort((a, b) => a - b);
    const pick = (q: number) => (frames.length ? frames[Math.min(frames.length - 1, Math.round((frames.length - 1) * q))] : 0);
    return {
      backend: core.backend ?? 'none',
      longTasks: w.__longTasks
        .filter((t) => t.start >= start && t.duration > 50)
        .map((t) => ({ startMs: Math.round(t.start - start), durationMs: Math.round(t.duration) })),
      mainMaxMs: main.maxMs,
      mainP95Ms: main.p95Ms,
      mainFrames: main.frames,
      workerFrames: frames.length,
      workerP50: pick(0.5),
      workerP95: pick(0.95),
      workerMax: frames.length ? frames[frames.length - 1] : 0,
      openMs: 0,
      dropped: core.droppedInput(),
      atlasImages: (window as unknown as { __atlasMax: number }).__atlasMax,
      atlasMisses: core.live!.atlasMisses,
      minZoom: (window as unknown as { __minZoom: number }).__minZoom,
    };
  }, start);
  return { ...sample, ...file, openMs };
}

// Проект `chromium`: автовыбор (без GPU это vello_cpu). Проект `gpu`: каждый
// бэкенд на настоящем GPU.
for (const backend of ['auto', 'webgpu', 'webgl', 'cpu'] as const) {
  test(`основной поток тонкий на 100 000 узлах (${backend}) @perf`, async ({ page }, info) => {
    test.skip((info.project.name === 'gpu') === (backend === 'auto'), 'другой проект');
    const s = await measure(page, backend === 'auto' ? '' : `&backend=${backend}`);
    if (backend !== 'auto') expect(s.backend).toBe(backend);
    const out = path.join(info.project.outputDir, `perf-${info.project.name}-${s.backend}.json`);
    fs.mkdirSync(path.dirname(out), { recursive: true });
    fs.writeFileSync(out, JSON.stringify(s, null, 2));
    console.log(`perf ${s.backend}: ${JSON.stringify(s)}`);
    expect(s.workerFrames, 'worker не рисовал кадры').toBeGreaterThan(30);
    expect(s.mainFrames).toBeGreaterThan(200);
    expect(s.longTasks, 'длинные задачи в основном потоке').toEqual([]);
    expect(s.mainMaxMs, 'обработчики основного потока за кадр').toBeLessThanOrEqual(MAIN_BUDGET_MS);
    expect(s.dropped).toBe(0);
    // Зум дошёл до всей страницы: растровый кэш фреймов был в работе.
    expect(s.minZoom).toBeLessThan(0.05);
    expect(s.atlasMisses, 'картинкам не хватило места в атласе').toBe(0);
    // На GPU растры фреймов рисуются из атласа: без картинок в нём замер не тот.
    if (s.backend === 'webgpu' || s.backend === 'webgl') {
      expect(s.atlasImages, 'атлас GPU пуст: растры фреймов не дошли до GPU').toBeGreaterThan(0);
    }
    if (s.backend === 'cpu' || s.backend === 'webgl') {
      expect(s.workerP95, 'p95 кадра worker\'а').toBeLessThanOrEqual(WORKER_P95_MS);
    }
  });
}

// --- Интерфейс (этап 5) -------------------------------------------------------
//
// Редактор с панелями на 100 000 узлах: дерево слоёв раскрыто на десяти фреймах,
// выделено 1000 строк (10 фреймов и 990 их детей), панель свойств показывает
// сводку выделения. 3 секунды выделение перетаскивается на холсте, а дерево
// слоёв прокручивается. Ворота: длинных задач нет, основной поток (обработчики,
// планировщик, обновления Solid) — не дольше 4 мс за кадр, в DOM дерева — только
// окно строк, панель получает живые значения жеста.

/** Наибольшее окно строк: экран 900 px / 28 px + запас по 10 строк сверху и снизу. */
const MAX_TREE_ROWS = Math.ceil(900 / 28) + 20 + 1;

test('интерфейс на 100 000 узлах: перетаскивание 1000 выделенных и прокрутка слоёв @perf', async ({ page }, info) => {
  test.skip(info.project.name !== 'chromium', 'замер интерфейса — в проекте chromium');
  await page.setViewportSize({ width: 1440, height: 900 });
  // Ядро — тесту, но без журнала и защиты layout тестового режима: они сами стоят времени.
  await page.addInitScript(() => ((window as { __ferrumPerf?: boolean }).__ferrumPerf = true));
  await page.addInitScript(() => {
    const w = window as unknown as { __longTasks: { start: number; duration: number }[]; __longObs: PerformanceObserver };
    w.__longTasks = [];
    w.__longObs = new PerformanceObserver((list) => {
      for (const e of list.getEntries()) w.__longTasks.push({ start: e.startTime, duration: e.duration });
    });
    w.__longObs.observe({ type: 'longtask', buffered: true });
  });
  await page.goto(`/?nodes=${NODES}`);
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true, null, { timeout: 120_000 });
  await expectCore(page);
  const tree = page.getByRole('tree', { name: 'Слои' });
  // Раскрыть десять верхних фреймов («Экран 999» … «Экран 990»): первый — кнопкой
  // строки, остальные — той же командой (их строки уходят за окно списка).
  await tree.getByRole('treeitem', { name: 'Экран 999', exact: true }).getByRole('button', { name: 'Развернуть' }).click();
  await page.evaluate(() => {
    const rows = window.__ferrum!.ui!.layers.rows.filter((r) => r.depth === 0).slice(0, 10);
    for (const r of rows) window.__ferrum!.core!.send({ type: 'expandLayer', id: r.id, expanded: true });
  });
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(1000 + 990);
  // Выделить строки 0 … 999 (10 фреймов и 990 их детей): первая — щелчком,
  // последняя («Слой 0» фрейма «Экран 990») — с Shift после прокрутки.
  await tree.getByRole('treeitem', { name: 'Экран 999', exact: true }).click();
  await tree.evaluate((el) => (el.scrollTop = 999 * 28 - 400));
  const last = tree.locator(`[role="treeitem"][style*="top: ${999 * 28}px"]`);
  await expect(last).toHaveAccessibleName('Слой 0');
  await last.click({ modifiers: ['Shift'] });
  await liveWhen(page, 'l.selectionCount === 1000');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.summary?.count)).toBe(1000);
  // Панель свойств на этом окне видна и показывает выделение из 1000 слоёв.
  const panel = page.locator('#panel-design');
  await expect(panel).toBeInViewport();
  const xField = panel.locator('input[aria-label="X"]');
  await expect(xField).toBeVisible();
  await expect(xField).toHaveValue('Смешанные');
  // Выделение — в кадр, затем жест из центра холста (внутри общей рамки).
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+Digit2');
  await page.waitForTimeout(500);
  const box = (await page.locator('canvas').boundingBox())!;
  const from = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const start = await page.evaluate(() => {
    // Запросы окна строк, ушедшие ядру во время жеста.
    const w = window as unknown as { __windowReqs: number };
    w.__windowReqs = 0;
    const core = window.__ferrum!.core!;
    const send = core.send.bind(core);
    core.send = (cmd) => {
      if (cmd.type === 'layersWindow') w.__windowReqs++;
      send(cmd);
    };
    window.__ferrum!.core!.resetMainThreadStats();
    return performance.now();
  });
  const steps0 = (await live(page)).steps;
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  let maxRows = 0;
  let minRows = Infinity;
  /** Верх окна строк, пришедший от worker'а: прокрутка дерева дошла до ядра и обратно. */
  const starts = new Set<number>();
  /** Что показывает поле X панели во время жеста (из DOM). */
  const shownX = new Set<string>();
  // Значения живого состояния, которые дошли до панели (общая рамка выделения).
  const liveX = new Set<number>();
  const end = Date.now() + 3000;
  for (let i = 0; Date.now() < end; i++) {
    await page.mouse.move(from.x + (i % 60) * 3, from.y + Math.sin(i / 8) * 30);
    // Прокрутка дерева слоёв во время жеста; строки читаются после отрисовки.
    await tree.evaluate((el, i) => (el.scrollTop = ((i * 37) % 900) * 28), i);
    await page.waitForTimeout(16);
    const seen = await page.evaluate(() => ({
      rows: document.querySelectorAll('[role="tree"] [role="treeitem"]').length,
      start: window.__ferrum!.ui!.layers.start,
      live: window.__ferrum!.ui!.live.x,
      x: document.querySelector<HTMLInputElement>('#panel-design input[aria-label="X"]')?.value ?? '',
    }));
    maxRows = Math.max(maxRows, seen.rows);
    minRows = Math.min(minRows, seen.rows);
    starts.add(seen.start);
    liveX.add(seen.live);
    shownX.add(seen.x);
  }
  await page.mouse.up();
  const after = await liveWhen(page, `l.steps === ${steps0 + 1} && l.busy === 0`);
  const sample = await page.evaluate((start) => {
    const w = window as unknown as { __longTasks: { start: number; duration: number }[] };
    const main = window.__ferrum!.core!.mainThreadStats();
    const w2 = w as unknown as { __longObs: PerformanceObserver };
    // Записи, ещё не доставленные наблюдателю, тоже в счёт.
    for (const e of w2.__longObs.takeRecords()) w.__longTasks.push({ start: e.startTime, duration: e.duration });
    return {
      windowRequests: (window as unknown as { __windowReqs: number }).__windowReqs,
      selectionCount: window.__ferrum!.ui!.selectionCount,
      summaryCount: window.__ferrum!.ui!.summary?.count,
      longTasks: w.__longTasks
        .filter((t) => t.start >= start && t.duration > 50)
        .map((t) => ({ startMs: Math.round(t.start - start), durationMs: Math.round(t.duration) })),
      mainMaxMs: main.maxMs,
      mainP95Ms: main.p95Ms,
      mainFrames: main.frames,
      dropped: window.__ferrum!.core!.droppedInput(),
    };
  }, start);
  const out = path.join(info.project.outputDir, 'perf-ui.json');
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, JSON.stringify({ ...sample, maxRows, minRows, layerStarts: starts.size, liveX: liveX.size, shownX: shownX.size }, null, 2));
  console.log(`perf ui: ${JSON.stringify({ ...sample, maxRows, minRows, layerStarts: starts.size, liveX: liveX.size, shownX: [...shownX].slice(0, 5) })}`);
  expect(after.nodes).toBe(NODES);
  expect(sample.mainFrames).toBeGreaterThan(100);
  expect(sample.longTasks, 'длинные задачи в основном потоке').toEqual([]);
  expect(sample.mainMaxMs, 'обработчики, планировщик и Solid за кадр').toBeLessThanOrEqual(MAIN_BUDGET_MS);
  expect(sample.dropped).toBe(0);
  expect(maxRows, 'строк дерева в DOM').toBeLessThanOrEqual(MAX_TREE_ROWS);
  // Прокрутка не вхолостую: строки были, окно сменилось много раз, запросы ушли ядру.
  expect(minRows, 'дерево во время прокрутки не пустело').toBeGreaterThan(0);
  expect(starts.size, 'разных окон строк от worker\'а').toBeGreaterThanOrEqual(10);
  expect(sample.windowRequests, 'запросов окна строк за жест').toBeGreaterThan(10);
  // Панель свойств всё время жеста — в состоянии выделения тысячи: поле X «Смешанные».
  // Живые значения жеста панель показывает только для одного узла (`liveOne`), поэтому
  // число разных значений в поле X здесь не проверить; они доходят до store (liveX выше).
  expect([...shownX], 'поле X панели во время жеста').toEqual(['Смешанные']);
  // Жест двигал именно выделенную тысячу.
  expect(sample.selectionCount).toBe(1000);
  expect(sample.summaryCount).toBe(1000);
  expect(after.selectionCount).toBe(1000);
  // Рамка выделения во время жеста доходила до интерфейса из общей памяти.
  expect(liveX.size, 'живые значения жеста в панели').toBeGreaterThan(10);
});
