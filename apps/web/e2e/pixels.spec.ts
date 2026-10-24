// Каждый бэкенд рисует то же, что эталонный vello_cpu: пиксели читаются из GPU
// (`readPixels`) и сравниваются с тем же кадром, нарисованным на CPU в том же
// worker'е. В кадре — узлы, подписи, выделение и картинка из атласа GPU.
// Отдельно — вытеснение из атласа: на место старой картинки встаёт новая.

import { expect, test, type Page } from '@playwright/test';
import { drag, live, liveWhen, open, screen } from './helpers';

const QUADRANTS = ['#ff0000', '#00ff00', '#0000ff', '#ffff00'];

/** PNG 64 × 48 из четырёх цветных четвертей — видно и отражение, и сдвиг —
 * ставится кликом в точку мира (`x`, `y`); `steps` — шагов истории после этого. */
async function placeQuadrants(page: Page, colors = QUADRANTS, x = 720, y = 1100, steps = 1): Promise<void> {
  await page.evaluate(async (colors) => {
    window.__ferrum!.events.length = 0;
    const c = new OffscreenCanvas(64, 48);
    const ctx = c.getContext('2d')!;
    colors.forEach((col, i) => {
      ctx.fillStyle = col;
      ctx.fillRect((i % 2) * 32, Math.floor(i / 2) * 24, 32, 24);
    });
    const blob = await c.convertToBlob({ type: 'image/png' });
    const bytes = new Uint8Array(await blob.arrayBuffer());
    window.__ferrum!.core.send({ type: 'placeImage', bytes });
  }, colors);
  await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'tool' && e.tool === 'image'));
  const l = await live(page);
  // Пустое место под первым фреймом: картинка встаёт в исходном размере.
  const at = screen(l, x, y);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, `l.steps === ${steps} && l.selectionW === 64 && l.selectionH === 48`);
}

async function checkAgainstCpu(page: Page, backend: 'auto' | 'cpu' | 'webgl' | 'webgpu'): Promise<void> {
  await placeQuadrants(page);
  // Перетаскивание «Слоя 1»: в кадре — его рамка выделения с ручками.
  const l = await live(page);
  await drag(page, screen(l, 200, 40), 4, 0, 2);
  const diff = await compareWithCpu(page);
  console.log(JSON.stringify(diff));
  const active = await page.evaluate(() => window.__ferrum!.core.backend);
  expect(diff.backend).toBe(backend === 'auto' ? active : backend);
  expect([diff.width, diff.height]).toEqual([1280, 720]);
  expect(diff.mean).toBeLessThan(0.1);
  expect(diff.over16).toBeLessThan(0.0005);
  // У GPU картинка пришла из атласа.
  if (diff.backend !== 'cpu') expect((await live(page)).atlasImages).toBeGreaterThanOrEqual(1);
}

interface Diff {
  backend: string;
  width: number;
  height: number;
  mean: number;
  over16: number;
}

async function compareWithCpu(page: Page): Promise<Diff> {
  await page.evaluate(() => {
    window.__ferrum!.events.length = 0;
    window.__ferrum!.core.send({ type: 'readPixels', reference: true });
  });
  await page.waitForFunction(() => window.__ferrum!.events.filter((e) => e.type === 'pixels').length === 2, null, {
    timeout: 30_000,
  });
  return page.evaluate(() => {
    const px = window.__ferrum!.events.filter((e) => e.type === 'pixels');
    const shown = px.find((e) => e.type === 'pixels' && e.backend !== 'cpu') ?? px[0];
    const cpu = px.find((e) => e !== shown)!;
    if (shown.type !== 'pixels' || cpu.type !== 'pixels') throw new Error('нет пикселей');
    let sum = 0;
    let over = 0;
    for (let i = 0; i < shown.rgba.length; i += 4) {
      let d = 0;
      for (let k = 0; k < 3; k++) d = Math.max(d, Math.abs(shown.rgba[i + k] - cpu.rgba[i + k]));
      sum += d;
      if (d > 16) over++;
    }
    const n = shown.rgba.length / 4;
    return { backend: shown.backend, width: shown.width, height: shown.height, mean: sum / n, over16: over / n };
  });
}

// Проект `chromium`: автовыбор (vello_cpu), CPU и программный WebGL2.
for (const backend of ['auto', 'cpu', 'webgl'] as const) {
  test(`бэкенд ${backend} рисует то же, что vello_cpu`, async ({ page }) => {
    await open(page, backend === 'auto' ? '' : `&backend=${backend}`);
    await checkAgainstCpu(page, backend);
  });
}

// Проект `gpu`: WebGPU и WebGL2 на настоящем GPU.
for (const backend of ['webgpu', 'webgl'] as const) {
  test(`${backend} на GPU рисует то же, что vello_cpu @gpu`, async ({ page }) => {
    await open(page, `&backend=${backend}`);
    await checkAgainstCpu(page, backend);
  });
}

// Атлас из одной страницы 64 × 64: две картинки 64 × 48 вместе не помещаются.
// Кадр с обеими — вторая заглушкой; после «показать выделение» видна только
// вторая: первая вытесняется, вторая загружается на её место. Очистка места не
// должна стереть только что загруженную картинку (у WebGPU запись картинки идёт
// раньше команд кадра).
for (const backend of ['webgpu', 'webgl'] as const) {
  test(`вытеснение из атласа ${backend}: новая картинка на месте старой @gpu`, async ({ page }) => {
    await open(page, `&backend=${backend}&atlas=64`);
    await placeQuadrants(page);
    await placeQuadrants(page, ['#ff00ff', '#00ffff', '#ffffff', '#000000'], 1900, 1100, 2);
    await liveWhen(page, 'l.atlasMisses >= 1');
    await page.evaluate(() => window.__ferrum!.core.send({ type: 'execute', command: 'view.fit_selection' }));
    await liveWhen(page, 'l.zoom > 2');
    const diff = await compareWithCpu(page);
    console.log(JSON.stringify(diff));
    expect(diff.backend).toBe(backend);
    expect(diff.mean).toBeLessThan(0.1);
    expect(diff.over16).toBeLessThan(0.0005);
    expect((await live(page)).atlasImages).toBe(1);
  });
}
