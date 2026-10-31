// Текст (подсистема 3, этап 1): текстовый узел рисуется вшитым Inter (файл
// качает worker ядра по манифесту), отсутствующий шрифт — в списке
// «Отсутствующие шрифты (N)» с заменой одним шагом отмены, тестовая команда
// создания текста — только в тестовом режиме. Firefox (@firefox): текст рисуется,
// «Показать системные шрифты» нет.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

// Чтений layout вне фазы measure планировщика не было (тестовый режим их копит).
test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

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

test('отсутствующий шрифт: список и замена одним шагом отмены', async ({ page }) => {
  await openApp(page, 0);
  await createText(page, 0, 0, 'Comic', { family: 'Comic Sans', style: 'Bold' });
  // Без выделения — панель «Дизайн» показывает отсутствующие шрифты.
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'select', ids: [], mode: 'replace' }));
  const section = page.getByRole('region', { name: 'Отсутствующие шрифты (1)' });
  await expect(section).toBeVisible();
  const group = section.getByRole('group', { name: 'Comic Sans Bold' });
  await group.getByRole('combobox', { name: 'Замена для Comic Sans Bold: семейство' }).selectOption('Roboto');
  await expect(group.getByRole('combobox', { name: 'Замена для Comic Sans Bold: начертание' })).toHaveValue('Bold');
  const steps = (await live(page)).steps;
  await group.getByRole('button', { name: 'Заменить' }).click();
  // Замена ждёт загрузки Roboto, затем — один шаг, и список пуст.
  await liveWhen(page, `l.steps === ${steps + 1}`);
  await expect(section).toHaveCount(0);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await expect(page.getByRole('region', { name: 'Отсутствующие шрифты (1)' })).toBeVisible();
});

test('тестовая команда создания текста вне тестового режима отвергается', async ({ page }) => {
  await page.addInitScript(() => ((window as { __ferrumPerf?: boolean }).__ferrumPerf = true));
  await page.goto('/?nodes=0#/new');
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true, null, { timeout: 90_000 });
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'testCreateText', x: 0, y: 0, text: 'x' }));
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.toasts.map((t) => t.text))).toContain('Неверное сообщение для ядра');
  expect((await live(page)).nodes).toBe(0);
});

test('системные шрифты Chrome: «Показать системные шрифты», байты через worker ввода-вывода', async ({ page, context }, info) => {
  test.skip(info.project.name !== 'chromium', 'queryLocalFonts — только Chrome и Edge');
  await openApp(page, 0);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setPageBackground', color: '#ffffff', phase: 'commit' }));
  // Разрешение после запуска: список при запуске не строился, нужен щелчок.
  await context.grantPermissions(['local-fonts']);
  const family = await page.evaluate(async () => {
    const all = await (window as unknown as { queryLocalFonts: () => Promise<{ family: string; style: string }[]> }).queryLocalFonts();
    const bundled = ['Inter', 'Onest', 'Roboto', 'Open Sans', 'Montserrat', 'PT Sans', 'PT Serif', 'Noto Sans', 'JetBrains Mono', 'Playfair Display'];
    return all.find((f) => f.style === 'Regular' && !bundled.includes(f.family))?.family ?? null;
  });
  test.skip(family === null, 'в системе нет шрифтов, кроме вшитых');
  const l = await live(page);
  const at = await appScreen(page, l, 100, 300);
  await createText(page, 100, 300, 'Ferrum', { family: family!, style: 'Regular' });
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'select', ids: [], mode: 'replace' }));
  await expect(page.getByRole('region', { name: 'Отсутствующие шрифты (1)' })).toBeVisible();
  // «Показать системные шрифты» — в выборе шрифта секции «Текст» (этап 2).
  const id = await page.evaluate(() => window.__ferrum!.ui!.layers.rows[0].id);
  await page.evaluate((id) => window.__ferrum!.core!.send({ type: 'select', ids: [id], mode: 'replace' }), id);
  await page.getByRole('button', { name: 'Шрифт', exact: true }).click();
  await page.getByRole('button', { name: 'Показать системные шрифты' }).click();
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'select', ids: [], mode: 'replace' }));
  // Шрифт нашёлся: список пуст, текст нарисован системным шрифтом (байты пришли).
  await expect(page.getByRole('region', { name: /Отсутствующие шрифты/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Показать системные шрифты' })).toHaveCount(0);
  await expect.poll(() => darkPixels(page, at, 120), { timeout: 10_000 }).toBeGreaterThan(200);
});
