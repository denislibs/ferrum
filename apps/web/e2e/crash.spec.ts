// Сбой ядра (этап 6, спека 11): паника в Rust — редактор перезапущен сам, правки из
// журнала на месте, одно уведомление и никакой плашки остановки; паника раньше пачки
// журнала — уведомление о потерянных секундах; третий сбой за минуту — плашка.

import { expect, test, type Page } from '@playwright/test';
import { drawRect, editorReady, fileId, journalCount, liveWhen, storedFile } from './helpers';

const toast = (page: Page, text: string) => page.locator('.fe-toast', { hasText: text });

/** Настоящая паника в ядре (тестовый режим) и ожидание нового ядра. */
async function panicAndRestart(page: Page, text: string): Promise<void> {
  await page.evaluate(() => ((window.__ferrum!.core as unknown as { old?: boolean }).old = true));
  await page.evaluate(() => window.__ferrum!.core!.testPanic());
  await expect(toast(page, text)).toHaveCount(1);
  await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
  await editorReady(page);
}

test('паника ядра: редактор перезапущен, правки из журнала на месте', async ({ page }) => {
  const logs: string[] = [];
  page.on('console', (m) => logs.push(m.text()));
  const id = await storedFile(page, '&fullSave=600000');
  await drawRect(page, 150);
  await expect.poll(() => journalCount(page, id), { timeout: 10_000 }).toBeGreaterThan(0);
  await panicAndRestart(page, 'Редактор перезапущен, изменения сохранены');
  await liveWhen(page, 'l.nodes === 2');
  expect(fileId(page)).toBe(id);
  await expect(page.locator('.fe-boot--stopped')).toHaveCount(0);
  await expect(toast(page, 'Ядро остановлено')).toHaveCount(0);
  // Причина — в консоли: текст паники дошёл до worker'а раньше остановки wasm.
  expect(logs.some((t) => t.includes('Ядро остановлено: паника в ядре: тестовая паника ядра'))).toBe(true);
  // Новый редактор работает и сохраняет.
  await drawRect(page, 300);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  expect(await journalCount(page, id)).toBe(0);
});

test('паника раньше пачки журнала: перезапуск и уведомление о потерянных секундах', async ({ page }) => {
  const id = await storedFile(page, '&fullSave=600000&journal=600000');
  await drawRect(page, 150);
  await panicAndRestart(page, 'Редактор перезапущен. Правки последних секунд не сохранились');
  await liveWhen(page, 'l.nodes === 1');
  expect(await journalCount(page, id)).toBe(0);
});

test('третий сбой за минуту: плашка остановки, без перезапуска', async ({ page }) => {
  await storedFile(page);
  for (let i = 0; i < 2; i++) await panicAndRestart(page, 'Редактор перезапущен');
  await page.evaluate(() => window.__ferrum!.core!.testPanic());
  await expect(page.getByRole('alert')).toContainText('Ядро остановлено: паника в ядре: тестовая паника ядра');
});

test('вне тестового режима паника по сообщению не вызывается', async ({ page }) => {
  // Обычный режим: ядро не раскрыто, сообщение шлём в worker напрямую.
  await page.addInitScript(() => {
    const Base = window.Worker;
    window.Worker = class extends Base {
      constructor(url: string | URL, opts?: WorkerOptions) {
        super(url, opts);
        if (String(url).includes('core.worker')) (window as unknown as { __coreWorker: Worker }).__coreWorker = this;
      }
    };
  });
  await page.goto('/?nodes=10#/new');
  await page.waitForFunction(() => window.__ferrum?.ready === true);
  await page.evaluate(() => (window as unknown as { __coreWorker: Worker }).__coreWorker.postMessage({ t: 'testPanic' }));
  await expect(toast(page, 'Неверное сообщение для ядра')).toHaveCount(1);
  await expect(page.locator('.fe-boot--stopped')).toHaveCount(0);
});
