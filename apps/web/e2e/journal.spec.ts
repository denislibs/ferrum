// Автосохранение журналом (этап 6): правка — в журнал за 2 с, полный файл — позже;
// перезагрузка вкладки до полного файла восстанавливает правки с уведомлением; отказ
// записи файла правку не теряет (журнал пишется раньше); полный файл в простое и уход
// к экрану файлов стирают журнал.

import { expect, test, type Page } from '@playwright/test';
import { drawRect, editorReady, journalCount, liveWhen, storedFile } from './helpers';

const meta = (page: Page, id: string) => page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
const toast = (page: Page, text: string) => page.locator('.fe-toast', { hasText: text });
const modified = (page: Page) => page.evaluate(() => window.__ferrum!.ui!.modified);

/** Перезагрузка как сбой вкладки: полный файл при уходе не запишется (отказ записи
 * в этой вкладке; в новой worker ввода-вывода — новый, отказов нет). */
async function crashReload(page: Page): Promise<void> {
  await page.evaluate(() => window.__ferrumFiles!.store.failWrites(true));
  await page.reload();
  await editorReady(page);
}

test('правка — в журнал за 2 с, файл не переписан; перезагрузка восстанавливает @firefox', async ({ page }) => {
  const id = await storedFile(page, '&fullSave=600000');
  const version = (await meta(page, id))!.version;
  await drawRect(page, 150);
  await expect.poll(() => journalCount(page, id), { timeout: 10_000 }).toBeGreaterThan(0);
  expect((await meta(page, id))!.version, 'полный файл не писался').toBe(version);
  expect(await modified(page)).toBe(true);
  await crashReload(page);
  await liveWhen(page, 'l.nodes === 2');
  await expect(toast(page, 'Восстановлены изменения, не записанные в файл')).toHaveCount(1);
  expect(await modified(page), 'восстановленного нет в файле').toBe(true);
  // Ctrl+S: полный файл, журнал стёрт, признак снят; следующее открытие — без восстановления.
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => journalCount(page, id)).toBe(0);
  await expect.poll(() => modified(page)).toBe(false);
  await page.reload();
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 2');
  await expect(toast(page, 'Восстановлены')).toHaveCount(0);
});

test('журнал пишется раньше файла: отказ записи файла правку не теряет', async ({ page }) => {
  const id = await storedFile(page, '&fullSave=600000&journal=600000');
  await page.evaluate(() => window.__ferrumFiles!.store.failWrites(true));
  await drawRect(page, 150);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect(toast(page, 'Не удалось сохранить файл: недостаточно места в хранилище браузера')).toHaveCount(1);
  // Пачка с правкой ушла в журнал до попытки записать файл (таймер журнала — 10 минут).
  expect(await journalCount(page, id)).toBe(1);
  await crashReload(page);
  await liveWhen(page, 'l.nodes === 2');
});

test('полный файл в простое стирает журнал и снимает «изменён»', async ({ page }) => {
  const id = await storedFile(page, '&journal=500&fullSave=4000');
  const version = (await meta(page, id))!.version;
  await drawRect(page, 150);
  await expect.poll(() => journalCount(page, id), { timeout: 5_000 }).toBeGreaterThan(0);
  await expect.poll(async () => (await meta(page, id))!.version, { timeout: 15_000 }).toBeGreaterThan(version);
  expect(await journalCount(page, id)).toBe(0);
  await expect.poll(() => modified(page)).toBe(false);
});

test('уход к экрану файлов пишет полный файл, журнал пуст', async ({ page }) => {
  const id = await storedFile(page, '&fullSave=600000');
  await drawRect(page, 150);
  await expect.poll(() => journalCount(page, id), { timeout: 10_000 }).toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: 'Все файлы' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  expect(await journalCount(page, id)).toBe(0);
});

test('повреждённая пачка: файл открыт, одно уведомление о потерянном', async ({ page }) => {
  const id = await storedFile(page, '&fullSave=600000');
  await page.evaluate(
    (i) => window.__ferrumFiles!.store.appendJournal(i, { session: 1, batch: 1 }, new TextEncoder().encode('мусор').buffer as ArrayBuffer),
    id,
  );
  await page.reload();
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 1');
  await expect(
    toast(page, 'Не все несохранённые изменения удалось восстановить: журнал изменений повреждён'),
  ).toHaveCount(1);
});
