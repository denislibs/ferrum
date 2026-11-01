// Настройки в IndexedDB (этап 6): перенос из localStorage при первом запуске, две
// вкладки меняют разные поля и не затирают друг друга, тема сменяется в обеих на
// лету, тема до загрузки приложения — из зеркала `ferrum.theme`.

import { expect, test, type Page } from '@playwright/test';
import { editorReady, mainMenu, newFileUrl, testMode } from './helpers';

const stored = (page: Page) => page.evaluate(() => window.__ferrumFiles!.store.settings());

test('перенос из localStorage: один раз, прежняя запись удаляется, тема — в зеркале', async ({ page }) => {
  await testMode(page);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('seeded')) return;
    sessionStorage.setItem('seeded', '1');
    localStorage.setItem('ferrum.settings.v1', JSON.stringify({ theme: 'light', filesView: 'list', pixelSnap: false }));
  });
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByRole('radiogroup', { name: 'Вид' }).getByRole('radio', { name: 'Список' })).toHaveAttribute('aria-checked', 'true');
  expect(await stored(page)).toMatchObject({ theme: 'light', filesView: 'list', pixelSnap: false });
  expect(await page.evaluate(() => [localStorage.getItem('ferrum.settings.v1'), localStorage.getItem('ferrum.theme')])).toEqual([
    null,
    'light',
  ]);
  // Приложение не грузится — тему ставит только theme-init.js по зеркалу.
  await page.route('**/assets/main-*.js', (r) => r.abort());
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('две вкладки меняют разные поля — оба сохраняются; тема — на лету в обеих', async ({ context }) => {
  const a = await context.newPage();
  const b = await context.newPage();
  await testMode(a);
  await testMode(b);
  await a.goto(newFileUrl(10));
  await editorReady(a);
  await b.goto('/');
  await expect(b.getByRole('heading', { level: 1 })).toBeVisible();
  // A: тема «Светлая» в настройках редактора.
  await mainMenu(a, 'Настройки', 'Настройки…');
  const dialog = a.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
  await dialog.getByRole('radio', { name: 'Светлая' }).check();
  await expect(b.locator('html')).toHaveAttribute('data-theme', 'light');
  // B: вид списка. A снова меняет тему — вид из B не затёрт.
  await b.getByRole('radiogroup', { name: 'Вид' }).getByRole('radio', { name: 'Список' }).click();
  await expect.poll(() => stored(a)).toMatchObject({ theme: 'light', filesView: 'list' });
  await dialog.getByRole('radio', { name: 'Тёмная' }).check();
  await expect.poll(() => stored(a)).toMatchObject({ theme: 'dark', filesView: 'list' });
  await expect(b.locator('html')).toHaveAttribute('data-theme', 'dark');
  await b.reload();
  await expect(b.getByRole('radiogroup', { name: 'Вид' }).getByRole('radio', { name: 'Список' })).toHaveAttribute('aria-checked', 'true');
});

test('база версии 2 обновляется до 3: файлы, байты, превью и журнал на месте; старая вкладка уступает', async ({ context }) => {
  // Старая вкладка (приложение до этапа 6, часть 2): база версии 2 с файлом, превью и
  // пачкой журнала; соединение открыто и закрывается по `versionchange`, как в
  // прежнем worker'е ввода-вывода.
  const old = await context.newPage();
  await old.goto('/theme-init.js');
  const id = await old.evaluate(
    () =>
      new Promise<string>((resolve, reject) => {
        const req = indexedDB.open('ferrum', 2);
        req.onupgradeneeded = () => {
          const d = req.result;
          d.createObjectStore('meta', { keyPath: 'id' });
          d.createObjectStore('bytes');
          d.createObjectStore('thumbs');
          d.createObjectStore('journal');
        };
        req.onerror = () => reject(req.error);
        req.onsuccess = () => {
          const d = req.result;
          const w = window as unknown as { yielded?: boolean };
          d.onversionchange = () => {
            d.close();
            w.yielded = true;
          };
          const id = crypto.randomUUID();
          const now = Date.now();
          const tx = d.transaction(['meta', 'bytes', 'thumbs', 'journal'], 'readwrite');
          tx.objectStore('meta').put({ id, name: 'Старый файл', createdAt: now, editedAt: now, openedAt: now, size: 8, thumbVersion: 1, version: 1 });
          tx.objectStore('bytes').put(new Uint8Array(8).fill(7).buffer, id);
          tx.objectStore('thumbs').put(new Blob([new Uint8Array(5)], { type: 'image/png' }), id);
          tx.objectStore('journal').put(new Uint8Array(3).buffer, [id, 1, 0]);
          tx.oncomplete = () => resolve(id);
          tx.onerror = () => reject(tx.error);
        };
      }),
  );
  const page = await context.newPage();
  await testMode(page);
  await page.goto('/');
  await expect(page.getByText('Старый файл')).toBeVisible();
  expect(await old.evaluate(() => (window as unknown as { yielded?: boolean }).yielded)).toBe(true);
  expect(await page.evaluate((i) => window.__ferrumFiles!.store.journalCount(i), id)).toBe(1);
  await page.evaluate(() => window.__ferrumFiles!.store.setSettings({ theme: 'light' }));
  expect(await stored(page)).toMatchObject({ theme: 'light' });
  // Сама база: нынешняя версия (4), прежние хранилища с данными, новое — настройки.
  const db = await old.evaluate(
    (i) =>
      new Promise<unknown>((resolve, reject) => {
        const req = indexedDB.open('ferrum');
        req.onerror = () => reject(req.error);
        req.onsuccess = () => {
          const d = req.result;
          const tx = d.transaction(['bytes', 'thumbs', 'journal', 'settings']);
          const get = <T>(r: IDBRequest<T>) => new Promise<T>((ok) => (r.onsuccess = () => ok(r.result)));
          void Promise.all([
            get(tx.objectStore('bytes').get(i)),
            get(tx.objectStore('thumbs').get(i)),
            get(tx.objectStore('journal').count()),
            get(tx.objectStore('settings').get('theme')),
          ]).then(([bytes, thumb, journal, theme]) => {
            resolve({
              version: d.version,
              bytes: (bytes as ArrayBuffer).byteLength,
              thumb: (thumb as Blob).size,
              journal,
              theme,
            });
            d.close();
          });
        };
      }),
    id,
  );
  expect(db).toEqual({ version: 4, bytes: 8, thumb: 5, journal: 1, theme: 'light' });
});

test('настройки не теряются при перезапуске ядра', async ({ page }) => {
  await testMode(page);
  await page.goto(newFileUrl(10));
  await editorReady(page);
  await mainMenu(page, 'Настройки', 'Настройки…');
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
  await dialog.getByRole('radio', { name: 'Светлая' }).check();
  // Сразу сбой — раньше отложенной записи настроек (четверть секунды).
  await page.evaluate(() => ((window.__ferrum!.core as unknown as { old?: boolean }).old = true));
  await page.evaluate(() => window.__ferrum!.core!.testPanic());
  await expect(page.locator('.fe-toast', { hasText: 'Редактор перезапущен' })).toHaveCount(1);
  await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
  await editorReady(page);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect.poll(() => stored(page)).toMatchObject({ theme: 'light' });
  await page.reload();
  await editorReady(page);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('чужое изменение темы не возвращает в базу раскладку панелей этой вкладки', async ({ context }) => {
  const a = await context.newPage();
  const b = await context.newPage();
  await testMode(a);
  await testMode(b);
  await a.goto(newFileUrl(10));
  await editorReady(a);
  await b.goto(newFileUrl(10));
  await editorReady(b);
  // A сворачивает страницы — запись в базе.
  await a.getByRole('button', { name: 'Страницы', exact: true }).click();
  await expect(a.getByRole('listbox', { name: 'Страницы' })).toHaveCount(0);
  await expect.poll(() => stored(a)).toMatchObject({ pagesOpen: false });
  // A уходит к списку файлов: её редактор не перепишет поле, если B его затрёт.
  await a.goto('/');
  await expect(a.getByRole('heading', { level: 1 })).toBeVisible();
  // B (страницы открыты) меняет тему: пишет только тему.
  await mainMenu(b, 'Настройки', 'Настройки…');
  const dialog = b.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
  await dialog.getByRole('radio', { name: 'Светлая' }).check();
  await expect.poll(() => stored(b)).toMatchObject({ theme: 'light' });
  await expect(a.locator('html')).toHaveAttribute('data-theme', 'light');
  // Отложенная запись B (четверть секунды) прошла — выбор A на месте.
  await b.waitForTimeout(600);
  expect(await stored(a)).toMatchObject({ theme: 'light', pagesOpen: false });
  await a.goto(newFileUrl(10));
  await editorReady(a);
  await expect(a.getByRole('button', { name: 'Страницы', exact: true })).toBeVisible();
  await expect(a.getByRole('listbox', { name: 'Страницы' })).toHaveCount(0);
});

test('хранилище не отвечает — экран с умолчаниями и одним уведомлением, заставка не висит', async ({ context }) => {
  // Другая страница держит обновление схемы бесконечно (транзакция обновления не
  // кончается): открытие базы в приложении не получает ни успеха, ни ошибки.
  const holder = await context.newPage();
  await holder.goto('/theme-init.js');
  await holder.evaluate(
    () =>
      new Promise<void>((resolve) => {
        const req = indexedDB.open('ferrum', 3);
        req.onupgradeneeded = () => {
          const st = req.result.createObjectStore('settings');
          const spin = () => (st.get('theme').onsuccess = spin);
          spin();
          resolve();
        };
      }),
  );
  const page = await context.newPage();
  await testMode(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 10_000 });
  await expect(page.locator('.fe-toast', { hasText: 'Не удалось прочитать настройки: хранилище не отвечает' })).toHaveCount(1);
  await page.waitForTimeout(500);
  await expect(page.locator('.fe-toast', { hasText: 'Не удалось прочитать настройки' })).toHaveCount(1);
});

test('настройка, изменённая прямо перед перезагрузкой, не теряется: worker не успел записать', async ({ page }) => {
  await testMode(page);
  await page.goto(newFileUrl(10));
  await editorReady(page);
  // Как при закрытии вкладки: worker уничтожается раньше, чем запись дойдёт до базы.
  await page.evaluate(() => window.__ferrumFiles!.store.holdSettings(true));
  await mainMenu(page, 'Настройки', 'Настройки…');
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await dialog.getByRole('tab', { name: 'Тема' }).click();
  await dialog.getByRole('radio', { name: 'Светлая' }).check();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.reload();
  await editorReady(page);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  // Запасная запись перенесена в базу и убрана.
  await expect.poll(() => stored(page)).toMatchObject({ theme: 'light' });
  await expect.poll(() => page.evaluate(() => localStorage.getItem('ferrum.settings.pending'))).toBeNull();
});
