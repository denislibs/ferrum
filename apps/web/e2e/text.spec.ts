// Текст (подсистема 3, этап 1): текстовый узел рисуется вшитым Inter (файл
// качает worker ядра по манифесту), отсутствующий шрифт — в списке
// «Отсутствующие шрифты (N)» с заменой одним шагом отмены, тестовая команда
// создания текста — только в тестовом режиме. Firefox (@firefox): текст рисуется,
// «Показать системные шрифты» нет.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

/** Текст в точке мира; ждёт узел в сцене (шрифты грузятся асинхронно). */
async function createText(page: Page, x: number, y: number, text: string, font?: { family: string; style: string }): Promise<void> {
  const before = (await live(page)).nodes;
  await page.evaluate(
    ({ x, y, text, font }) => window.__ferrum!.core!.send({ type: 'testCreateText', x, y, text, size: 64, font }),
    { x, y, text, font },
  );
  await liveWhen(page, `l.nodes === ${before + 1} && l.busy === 0`);
}

/** Тёмных пикселей в квадрате экрана (текст чёрный на белом фрейме примера). */
async function darkPixels(page: Page, at: { x: number; y: number }, size: number): Promise<number> {
  const png = (await page.screenshot({ clip: { x: at.x, y: at.y, width: size, height: size } })).toString('base64');
  return page.evaluate(async (png) => {
    const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    const ctx = c.getContext('2d')!;
    ctx.drawImage(bmp, 0, 0);
    const d = ctx.getImageData(0, 0, bmp.width, bmp.height).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] < 100 && d[i + 1] < 100 && d[i + 2] < 100) n++;
    return n;
  }, png);
}

test('текст рисуется вшитым Inter, загруженным worker\'ом ядра @firefox', async ({ page }) => {
  const fonts: string[] = [];
  page.on('request', (r) => {
    if (r.url().includes('/fonts/')) fonts.push(new URL(r.url()).pathname);
  });
  await openApp(page, 0);
  // Белый фон страницы: до текста в этом месте нет тёмных пикселей.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setPageBackground', color: '#ffffff', phase: 'commit' }));
  const l = await live(page);
  const at = await appScreen(page, l, 100, 300);
  await expect.poll(() => darkPixels(page, at, 120)).toBe(0);
  await createText(page, 100, 300, 'Ferrum');
  await expect.poll(() => darkPixels(page, at, 120), { timeout: 10_000 }).toBeGreaterThan(200);
  expect(fonts).toContain('/fonts/manifest.json');
  expect(fonts).toContain('/fonts/inter/Inter-Variable.ttf');
  // Строка слоя — «Текст» с иконкой текста.
  await expect(page.getByRole('treeitem', { name: 'Текст', exact: true })).toBeVisible();
  // Системные шрифты в Firefox не предлагаются.
  if (test.info().project.name === 'firefox') {
    await page.evaluate(() => window.__ferrum!.core!.send({ type: 'select', ids: [], mode: 'replace' }));
    await expect(page.getByRole('button', { name: 'Показать системные шрифты' })).toHaveCount(0);
  }
});
