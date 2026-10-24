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
import { live, screen } from './helpers';

const NODES = 100_000;
/** Бюджет основного потока на кадр (спека 4.2). */
const MAIN_BUDGET_MS = 4;
/** 60 fps: 95-й процентиль времени кадра worker'а (спека 3). */
const WORKER_P95_MS = 16.7;

interface Sample {
  backend: string;
  longTasks: number[];
  mainMaxMs: number;
  mainP95Ms: number;
  mainFrames: number;
  workerFrames: number;
  workerP50: number;
  workerP95: number;
  workerMax: number;
  openMs: number;
  dropped: number;
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
    f.core.send({ type: 'save' });
    await until(() => f.events.some((e) => e.type === 'saved'));
    const saveMs = performance.now() - t0;
    const saved = f.events.find((e) => e.type === 'saved');
    if (!saved || saved.type !== 'saved') throw new Error('нет сохранения');
    const fileMb = saved.bytes.length / 1e6;
    f.events.length = 0;
    const frames = f.core.live!.frames;
    const t1 = performance.now();
    f.core.send({ type: 'open', bytes: saved.bytes });
    await until(() => f.events.some((e) => e.type === 'opened'));
    const fileOpenMs = performance.now() - t1;
    await until(() => f.core.live!.frames > frames);
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
  await page.goto(`/?nodes=${NODES}${query}`);
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 120_000 });
  await page.waitForFunction(() => (window.__ferrum!.core.live?.frames ?? 0) > 0, null, { timeout: 60_000 });
  const openMs = Date.now() - t0;
  // Время кадров worker'а — из живого состояния, на каждом новом кадре.
  const start = await page.evaluate(() => {
    const w = window as unknown as { __frameMs: number[] };
    w.__frameMs = [];
    let seen = window.__ferrum!.core.live!.frames;
    const z = window as unknown as { __minZoom: number };
    z.__minZoom = Infinity;
    const tick = () => {
      const l = window.__ferrum!.core.live!;
      z.__minZoom = Math.min(z.__minZoom, l.zoom);
      if (l.frames !== seen) {
        seen = l.frames;
        w.__frameMs.push(l.frameMs);
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    window.__ferrum!.core.resetMainThreadStats();
    return performance.now();
  });
  await interact(page);
  const file = await saveAndOpen(page);
  await page.waitForTimeout(300);
  const sample = await page.evaluate((start) => {
    const w = window as unknown as { __longTasks: { start: number; duration: number }[]; __frameMs: number[] };
    const core = window.__ferrum!.core;
    const main = core.mainThreadStats();
    const frames = [...w.__frameMs].sort((a, b) => a - b);
    const pick = (q: number) => (frames.length ? frames[Math.min(frames.length - 1, Math.round((frames.length - 1) * q))] : 0);
    return {
      backend: core.backend ?? 'none',
      longTasks: w.__longTasks.filter((t) => t.start >= start && t.duration > 50).map((t) => Math.round(t.duration)),
      mainMaxMs: main.maxMs,
      mainP95Ms: main.p95Ms,
      mainFrames: main.frames,
      workerFrames: frames.length,
      workerP50: pick(0.5),
      workerP95: pick(0.95),
      workerMax: frames.length ? frames[frames.length - 1] : 0,
      openMs: 0,
      dropped: core.droppedInput(),
      atlasImages: core.live!.atlasImages,
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
    if (s.backend === 'cpu' || s.backend === 'webgl') {
      expect(s.workerP95, 'p95 кадра worker\'а').toBeLessThanOrEqual(WORKER_P95_MS);
    }
  });
}
