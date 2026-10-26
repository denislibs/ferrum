// Журнал изменений в хранилище IndexedDB (этап 6): пачки читаются по порядку сеансов
// и номеров вместе с файлом (и только если у файла есть байты), чтение после
// отправленной пачки её видит, запись файла стирает пачки до номера снимка, отказ
// записи их оставляет, удаление файла и корзины стирает журнал.

import { expect, test } from '@playwright/test';
import { testMode } from './helpers';

test.beforeEach(async ({ page }) => {
  await testMode(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
});

test('пачки — по порядку сеансов и номеров; без байтов файла журнала нет', async ({ page }) => {
  const r = await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const buf = (n: number) => new Uint8Array([n, n, n]).buffer;
    const m = await s.create('Журнал');
    await s.appendJournal(m.id, { session: 20, batch: 1 }, buf(3));
    const empty = await s.read(m.id);
    await s.write(m.id, new Uint8Array([1, 2, 3, 4]).buffer, null, m.version, null);
    await s.appendJournal(m.id, { session: 20, batch: 2 }, buf(4));
    // Без ожидания: чтение встаёт в ту же очередь и видит пачку.
    void s.appendJournal(m.id, { session: 10, batch: 7 }, buf(1));
    const full = await s.read(m.id);
    return {
      emptyJournal: empty.journal.length,
      order: full.journal.map((b) => new Uint8Array(b)[0]),
      count: await s.journalCount(m.id),
    };
  });
  expect(r).toEqual({ emptyJournal: 0, order: [1, 3, 4], count: 3 });
});

test('запись файла стирает пачки до номера снимка; отказ записи их оставляет', async ({ page }) => {
  const r = await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const m = await s.create('Снимок', new Blob([new Uint8Array([9])]));
    for (const [session, batch] of [[5, 1], [7, 1], [7, 2], [7, 3]]) {
      await s.appendJournal(m.id, { session, batch }, new Uint8Array([session * 10 + batch]).buffer);
    }
    await s.failWrites(true);
    let failed = '';
    try {
      await s.write(m.id, new Uint8Array([1]).buffer, null, m.version, { session: 7, upTo: 2 });
    } catch (e) {
      failed = (e as Error).message;
    }
    const kept = await s.journalCount(m.id);
    await s.failWrites(false);
    await s.write(m.id, new Uint8Array([1]).buffer, null, m.version, { session: 7, upTo: 2 });
    const left = (await s.read(m.id)).journal.map((b) => new Uint8Array(b)[0]);
    return { failed, kept, left };
  });
  expect(r).toEqual({ failed: 'недостаточно места в хранилище браузера', kept: 4, left: [73] });
});

test('удаление навсегда и очистка корзины стирают журнал', async ({ page }) => {
  const r = await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const one = await s.create('Один', new Blob([new Uint8Array([1])]));
    const two = await s.create('Два', new Blob([new Uint8Array([1])]));
    for (const m of [one, two]) await s.appendJournal(m.id, { session: 1, batch: 1 }, new ArrayBuffer(8));
    await s.remove(one.id);
    await s.trash(two.id);
    const inTrash = await s.journalCount(two.id);
    await s.emptyTrash();
    return [await s.journalCount(one.id), inTrash, await s.journalCount(two.id)];
  });
  expect(r).toEqual([0, 1, 0]);
});

test('пачка удалённого файла — отказ «Файл не найден»', async ({ page }) => {
  const message = await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const m = await s.create('Временный');
    await s.remove(m.id);
    return s.appendJournal(m.id, { session: 1, batch: 1 }, new ArrayBuffer(1)).then(
      () => 'записано',
      (e: Error) => e.message,
    );
  });
  expect(message).toBe('Файл не найден');
});

test('пачка файла, удалённого в другой вкладке, — отказ по базе, сиротских пачек нет', async ({ page }) => {
  const r = await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const m = await s.create('Чужое удаление', new Blob([new Uint8Array([1])]));
    // Другая вкладка удалила запись: в памяти этого worker'а она ещё есть.
    await new Promise<void>((resolve, reject) => {
      const req = indexedDB.open('ferrum');
      req.onerror = () => reject(req.error);
      req.onsuccess = () => {
        const d = req.result;
        const tx = d.transaction('meta', 'readwrite');
        tx.objectStore('meta').delete(m.id);
        tx.oncomplete = () => {
          d.close();
          resolve();
        };
        tx.onerror = () => reject(tx.error);
      };
    });
    const message = await s.appendJournal(m.id, { session: 1, batch: 1 }, new ArrayBuffer(1)).then(
      () => 'записано',
      (e: Error) => e.message,
    );
    return { message, count: await s.journalCount(m.id) };
  });
  expect(r).toEqual({ message: 'Файл не найден', count: 0 });
});

test('база версии 1 с файлами открывается как нынешняя (3): файлы целы, журнал работает', async ({ browser }) => {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await testMode(page);
  // Пустая страница того же происхождения: приложение не открывает базу.
  await page.route('**/v1-seed', (route) =>
    route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>seed</title>' }),
  );
  await page.goto('/v1-seed');
  await page.evaluate(
    () =>
      new Promise<void>((resolve, reject) => {
        const req = indexedDB.open('ferrum', 1);
        req.onupgradeneeded = () => {
          const d = req.result;
          d.createObjectStore('meta', { keyPath: 'id' });
          d.createObjectStore('bytes');
          d.createObjectStore('thumbs');
        };
        req.onerror = () => reject(req.error);
        req.onsuccess = () => {
          const d = req.result;
          const tx = d.transaction(['meta', 'bytes'], 'readwrite');
          const meta = {
            id: 'old-file-1',
            name: 'Старый файл',
            createdAt: 1,
            editedAt: 2,
            openedAt: 3,
            size: 4,
            thumbVersion: 0,
            version: 1,
          };
          tx.objectStore('meta').put(meta);
          tx.objectStore('bytes').put(new Uint8Array([7, 8, 9, 10]).buffer, 'old-file-1');
          tx.oncomplete = () => {
            d.close();
            resolve();
          };
          tx.onerror = () => reject(tx.error);
        };
      }),
  );
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  const r = await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const before = await s.read('old-file-1');
    await s.appendJournal('old-file-1', { session: 1, batch: 1 }, new Uint8Array([5]).buffer);
    const after = await s.read('old-file-1');
    const version = await new Promise<number>((resolve, reject) => {
      const req = indexedDB.open('ferrum');
      req.onsuccess = () => {
        const v = req.result.version;
        req.result.close();
        resolve(v);
      };
      req.onerror = () => reject(req.error);
    });
    return {
      name: before.meta.name,
      bytes: Array.from(new Uint8Array(before.bytes!)),
      journalBefore: before.journal.length,
      journalAfter: after.journal.map((b) => new Uint8Array(b)[0]),
      version,
    };
  });
  expect(r).toEqual({ name: 'Старый файл', bytes: [7, 8, 9, 10], journalBefore: 0, journalAfter: [5], version: 4 });
  await ctx.close();
});
