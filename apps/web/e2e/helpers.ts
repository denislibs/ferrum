// Общие шаги E2E: открыть стенд, прочитать живое состояние, перевести мировые
// координаты в экранные, прочитать пиксель снимка.

import { expect, type Page } from '@playwright/test';
import type { LiveState } from '../src/core/core';

/** Заголовок страницы стенда: по нему видно, что открыт именно debug.html, а не
 * index.html, поданный запасным маршрутом сервера на отсутствующий адрес. */
export const DEBUG_TITLE = 'Ferrum — стенд ядра';

/** Открывает стенд и проверяет, что это он: заголовок и плашка `#hud` есть только у него. */
export async function gotoDebug(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page).toHaveTitle(DEBUG_TITLE);
  await expect(page.locator('#hud')).toHaveCount(1);
}

/** Ядро раскрыто в `window.__ferrum` (тестовый режим, стенд или `__ferrumPerf`):
 * дальше тесты пишут `window.__ferrum!.core!` — здесь это проверено, а не принято
 * на веру. */
export async function expectCore(page: Page): Promise<void> {
  const kind = await page.evaluate(() => typeof window.__ferrum?.core?.send);
  expect(kind, 'window.__ferrum.core нет: страница не в тестовом режиме').toBe('function');
}

export async function open(page: Page, query = '', origin = ''): Promise<void> {
  await gotoDebug(page, `${origin}/debug.html?nodes=1000${query}`);
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
  await expectCore(page);
  await page.waitForFunction(() => (window.__ferrum!.core!.live?.frames ?? 0) > 0);
}

export function live(page: Page): Promise<LiveState> {
  return page.evaluate(() => window.__ferrum!.core!.live!);
}

/** Ждёт, пока живое состояние не удовлетворит условию (возвращает его). */
export async function liveWhen(page: Page, check: string): Promise<LiveState> {
  await page.waitForFunction(`(() => { const l = window.__ferrum.core.live; return !!l && (${check}); })()`);
  return live(page);
}

/** Точка мира на экране (CSS-пиксели страницы; холст — на всё окно). */
export function screen(l: LiveState, x: number, y: number): { x: number; y: number } {
  return { x: (x - l.cameraX) * l.zoom + l.viewportW / 2, y: (y - l.cameraY) * l.zoom + l.viewportH / 2 };
}

export async function events(page: Page, type: string): Promise<number> {
  return page.evaluate((t) => window.__ferrum!.events.filter((e) => e.type === t).length, type);
}

/** Пиксель снимка страницы в CSS-точке (снимок декодирует сама страница). */
export async function pixel(page: Page, x: number, y: number): Promise<number[]> {
  const png = (await page.screenshot()).toString('base64');
  return page.evaluate(
    async ({ png, x, y }) => {
      const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
      const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
      const k = bmp.width / innerWidth;
      const c = new OffscreenCanvas(bmp.width, bmp.height);
      const ctx = c.getContext('2d')!;
      ctx.drawImage(bmp, 0, 0);
      return Array.from(ctx.getImageData(Math.round(x * k), Math.round(y * k), 1, 1).data);
    },
    { png, x, y },
  );
}

function near(actual: number[], rgb: number[], tol: number): boolean {
  return [0, 1, 2].every((i) => Math.abs(actual[i] - rgb[i]) <= tol);
}

/** Пиксель на экране становится цвета `rgb`: кадр из worker'а доходит до экрана
 * не в тот же момент, когда обновилось живое состояние, поэтому — с повтором. */
export async function expectPixel(page: Page, x: number, y: number, rgb: number[], tol = 6): Promise<void> {
  let last: number[] = [];
  await expect
    .poll(
      async () => {
        last = await pixel(page, x, y);
        return near(last, rgb, tol);
      },
      { message: `пиксель (${x}, ${y}) не стал ${rgb}`, timeout: 10_000 },
    )
    .toBe(true);
  expect(last.length).toBe(4);
}

/** Перетаскивание мышью с промежуточными шагами (pointermove на каждом). */
export async function drag(page: Page, from: { x: number; y: number }, dx: number, dy: number, steps = 10): Promise<void> {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + dx, from.y + dy, { steps });
  await page.mouse.up();
}

/** Адрес нового файла в редакторе: `#/new` создаёт запись в хранилище и открывает
 * её; у нового файла документ — страница-пример из `nodes` узлов. */
export function newFileUrl(nodes = 1000, query = ''): string {
  return `/?nodes=${nodes}${query}#/new`;
}

/** Редактор (index.html) с новым файлом — страницей-примером из `nodes` узлов. */
export async function openApp(page: Page, nodes = 1000, query = ''): Promise<void> {
  // Тестовый режим интерфейса: журнал событий (без байтов) и защита чтений layout.
  await page.addInitScript(() => ((window as { __ferrumTest?: boolean }).__ferrumTest = true));
  await page.goto(newFileUrl(nodes, query));
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true, null, {
    timeout: 90_000,
  });
  await expectCore(page);
  await page.waitForFunction(() => (window.__ferrum!.core!.live?.frames ?? 0) > 0);
}

/** Точка мира на экране редактора: холст между панелями, его угол — со страницы. */
export async function appScreen(page: Page, l: LiveState, x: number, y: number): Promise<{ x: number; y: number }> {
  const box = (await page.locator('canvas').boundingBox())!;
  const p = screen(l, x, y);
  return { x: box.x + p.x, y: box.y + p.y };
}

/** Тестовый режим интерфейса до загрузки страницы: хуки хранилища, ядро в `__ferrum`. */
export async function testMode(page: Page): Promise<void> {
  await page.addInitScript(() => ((window as { __ferrumTest?: boolean }).__ferrumTest = true));
}

/** Редактор открыт и ядро готово (после перезапуска — новое ядро). */
export async function editorReady(page: Page): Promise<void> {
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true, null, {
    timeout: 90_000,
  });
  await expectCore(page);
  await page.waitForFunction(() => (window.__ferrum!.core!.live?.frames ?? 0) > 0);
}

/** id файла из адреса редактора (`#/file/<id>`). */
export function fileId(page: Page): string {
  const m = /#\/file\/([^/?#]+)/.exec(page.url());
  if (!m) throw new Error(`не адрес файла: ${page.url()}`);
  return decodeURIComponent(m[1]);
}

/** Прямоугольник инструментом R около центра холста (`dx` — сдвиг по x в мире);
 * ждёт записанный узел: в сцене на один больше и жест закончен (черновик рисования
 * тоже в сцене, поэтому одного числа узлов мало). */
export async function drawRect(page: Page, dx = 0): Promise<void> {
  const before = (await live(page)).nodes;
  await page.locator('canvas').click({ position: { x: 10, y: 10 } });
  await page.keyboard.press('KeyR');
  const l = await live(page);
  await drag(page, await appScreen(page, l, l.cameraX - 50 + dx, l.cameraY - 40), 100, 80);
  await liveWhen(page, `l.nodes === ${before + 1} && l.busy === 0`);
}

/** Пачек журнала файла в хранилище (тестовый хук worker'а ввода-вывода). */
export function journalCount(page: Page, id: string): Promise<number> {
  return page.evaluate((i) => window.__ferrumFiles!.store.journalCount(i), id);
}

/** Новый файл в хранилище, уже записанный полным файлом (в нём один прямоугольник):
 * дальше правки идут в журнал. `query` — параметры адреса (`&fullSave=…`). */
export async function storedFile(page: Page, query = ''): Promise<string> {
  await testMode(page);
  await page.goto(`/?nodes=0${query}#/new`);
  await editorReady(page);
  const id = fileId(page);
  await drawRect(page);
  // Новый файл без байтов: первая запись — полным файлом через 2 с после правки.
  await expect
    .poll(() => page.evaluate((i) => window.__ferrumFiles!.store.get(i).then((m) => m?.size ?? 0), id), { timeout: 10_000 })
    .toBeGreaterThan(0);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  return id;
}
