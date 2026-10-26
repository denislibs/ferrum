// Круг сохранения (спека 12, этап 6): нарисовать фрейм → сохранить → перезагрузить →
// открыть → сравнить. Сравниваются строки дерева слоёв (id, имена, типы, вложенность),
// сводка свойств выделенного фрейма и картинка документа без оверлеев — превью первой
// страницы, которое ядро рисует заново после открытия (PNG байт в байт). Идёт и в Firefox.
// Здесь файл записан целиком; восстановление правок из журнала после перезагрузки (тоже
// с Firefox) — в `journal.spec.ts`.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, drawRect, editorReady, fileId, live, liveWhen, testMode } from './helpers';

/** Всё видимое о документе: строки слоёв (фрейм раскрыт), сводка фрейма, превью. */
async function look(page: Page) {
  const frame = await page.evaluate(() => window.__ferrum!.ui!.layers.rows.find((r) => r.kind === 'frame')!.id);
  await page.evaluate((id) => {
    window.__ferrum!.core!.send({ type: 'expandLayer', id, expanded: true });
    window.__ferrum!.core!.send({ type: 'select', ids: [id], mode: 'replace' });
  }, frame);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(2);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.summary?.count)).toBe(1);
  const thumb = await page.evaluate(
    () =>
      new Promise<number[]>((resolve) => {
        const core = window.__ferrum!.core!;
        const off = core.on((e) => {
          if (e.type !== 'snapshot') return;
          off();
          resolve(Array.from(e.thumbnail ?? []));
        });
        core.send({ type: 'snapshot', thumbnail: true });
      }),
  );
  return page.evaluate(
    (thumb) => ({
      rows: window.__ferrum!.ui!.layers.rows.map((r) => ({ id: r.id, name: r.name, kind: r.kind, depth: r.depth })),
      summary: JSON.parse(JSON.stringify(window.__ferrum!.ui!.summary)) as unknown,
      thumb,
    }),
    thumb,
  );
}

test('нарисовать фрейм → сохранить → перезагрузить → открыть → сравнить @firefox', async ({ page }) => {
  await testMode(page);
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  const id = fileId(page);
  // Фрейм инструментом F, в нём — прямоугольник (ребёнок фрейма).
  await page.locator('canvas').click({ position: { x: 10, y: 10 } });
  await page.keyboard.press('KeyF');
  const l = await live(page);
  await drag(page, await appScreen(page, l, l.cameraX - 150, l.cameraY - 100), 300, 200);
  await liveWhen(page, 'l.nodes === 1');
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  const before = await look(page);
  expect(before.rows.map((r) => [r.kind, r.depth])).toEqual([
    ['frame', 0],
    ['rect', 1],
  ]);
  expect(before.thumb.length).toBeGreaterThan(100);
  await page.reload();
  await editorReady(page);
  expect(fileId(page)).toBe(id);
  expect(await look(page)).toEqual(before);
});
