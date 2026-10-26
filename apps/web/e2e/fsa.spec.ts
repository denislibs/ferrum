// File System Access (этап 6): файл на диске — запись хранилища, связанная с ним.
// Системные диалоги заглушены файлами OPFS (`navigator.storage.getDirectory()`): у них
// тот же FileSystemFileHandle и запись `createWritable`, но без окна выбора.
// Проверяется: «Открыть с диска…» и правки в файл, «Сохранить на диск…», файл,
// изменённый другой программой, запрос доступа после перезапуска, нет API — нет пунктов.

import { expect, test, type Page } from '@playwright/test';
import { drawRect, editorReady, fileId, liveWhen, newFileUrl, storedFile, testMode } from './helpers';

/** Диалоги выбора — файлы OPFS: открыть — `window.__pick` (или «проект.ferrum»),
 * сохранить — предложенное имя. */
async function stubPickers(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    const root = () => navigator.storage.getDirectory();
    w.showOpenFilePicker = async () => [await (await root()).getFileHandle((w.__pick as string) ?? 'проект.ferrum')];
    w.showSaveFilePicker = async (o?: { suggestedName?: string }) =>
      (await root()).getFileHandle(o?.suggestedName ?? 'копия.ferrum', { create: true });
  });
}

const diskSize = (page: Page, name: string) =>
  page.evaluate(async (n) => (await (await (await navigator.storage.getDirectory()).getFileHandle(n)).getFile()).size, name);
const meta = (page: Page, id: string) => page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
const toast = (page: Page, text: string) => page.locator('.fe-toast', { hasText: text });
const fileMenu = async (page: Page, item: string) => {
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: item }).click();
};

/** Байты открытого документа — файлом `name` в OPFS («на диске»). */
async function writeDocToDisk(page: Page, name: string): Promise<void> {
  await page.evaluate(
    (n) =>
      new Promise<void>((resolve) => {
        const core = window.__ferrum!.core!;
        const off = core.on(async (e) => {
          if (e.type !== 'saved') return;
          off();
          const h = await (await navigator.storage.getDirectory()).getFileHandle(n, { create: true });
          const w = await h.createWritable();
          await w.write(e.bytes as BlobPart);
          await w.close();
          resolve();
        });
        core.send({ type: 'save' });
      }),
    name,
  );
}

/** Новое ядро после перехода к другому файлу. */
async function nextEditor(page: Page, go: () => Promise<void>): Promise<void> {
  await page.evaluate(() => ((window.__ferrum!.core as unknown as { old?: boolean }).old = true));
  await go();
  await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
  await editorReady(page);
}

test('«Открыть с диска…»: запись связана с файлом, правки пишутся в него', async ({ page }) => {
  await testMode(page);
  await stubPickers(page);
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await writeDocToDisk(page, 'проект.ferrum');
  await nextEditor(page, () => fileMenu(page, 'Открыть с диска…'));
  await liveWhen(page, 'l.nodes === 5');
  const id = fileId(page);
  expect((await meta(page, id))!.disk).toBe('проект.ferrum');
  await expect(page.locator('.fe-panel-head__name')).toHaveText('проект');
  const before = await diskSize(page, 'проект.ferrum');
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => diskSize(page, 'проект.ferrum')).not.toBe(before);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  // Экран файлов: «На диске»; тот же файл ещё раз — та же запись.
  await fileMenu(page, 'Все файлы');
  const card = page.getByRole('main').getByRole('listbox').getByRole('option', { name: 'проект', exact: true });
  await expect(card).toContainText('На диске · Изменён');
  await nextEditor(page, () => page.getByRole('button', { name: 'Открыть с диска…' }).click());
  expect(fileId(page)).toBe(id);
  await liveWhen(page, 'l.nodes === 6');
});

test('«Сохранить на диск…»: файл из хранилища переезжает на диск', async ({ page }) => {
  await stubPickers(page);
  const id = await storedFile(page, '&fullSave=600000');
  await fileMenu(page, 'Сохранить на диск…');
  await expect(toast(page, 'Файл сохранён на диск: Без названия.ferrum')).toHaveCount(1);
  expect((await meta(page, id))!.disk).toBe('Без названия.ferrum');
  const before = await diskSize(page, 'Без названия.ferrum');
  expect(before).toBeGreaterThan(0);
  await drawRect(page, 150);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => diskSize(page, 'Без названия.ferrum')).not.toBe(before);
  await page.reload();
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 2');
});

test('файл на диске изменён другой программой: запись отказывает, правка не теряется', async ({ page }) => {
  await testMode(page);
  await stubPickers(page);
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await writeDocToDisk(page, 'проект.ferrum');
  await nextEditor(page, () => fileMenu(page, 'Открыть с диска…'));
  await page.waitForTimeout(20);
  await page.evaluate(async () => {
    const h = await (await navigator.storage.getDirectory()).getFileHandle('проект.ferrum');
    const w = await h.createWritable();
    await w.write(new Uint8Array([1, 2, 3]));
    await w.close();
  });
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect(
    toast(page, 'Не удалось сохранить файл: Файл на диске изменён другой программой — сохраните копию на диск или откройте файл заново'),
  ).toHaveCount(1);
  expect(await page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  expect(await diskSize(page, 'проект.ferrum')).toBe(3);
});

test('доступ к файлу на диске — вопрос; «Разрешить» открывает, «Отмена» — экран файлов', async ({ page }) => {
  await testMode(page);
  await stubPickers(page);
  // Как после перезапуска браузера: доступа нет, пока его не разрешат.
  await page.addInitScript(() => {
    let granted = false;
    const proto = FileSystemHandle.prototype as unknown as Record<string, unknown>;
    proto.queryPermission = async () => (granted ? 'granted' : 'prompt');
    proto.requestPermission = async () => ((granted = true), 'granted');
  });
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await writeDocToDisk(page, 'проект.ferrum');
  await fileMenu(page, 'Открыть с диска…');
  const ask = page.getByRole('dialog', { name: 'Разрешить доступ к файлу на диске?' });
  await expect(ask).toContainText('Ferrum откроет «проект.ferrum» и будет сохранять изменения в него');
  await ask.getByRole('button', { name: 'Разрешить' }).click();
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 5');
  await page.reload();
  await page.getByRole('dialog', { name: 'Разрешить доступ к файлу на диске?' }).getByRole('button', { name: 'Отмена' }).click();
  await expect(toast(page, 'Нет доступа к файлу на диске — разрешите его или откройте файл заново')).toHaveCount(1);
  await expect(page).toHaveURL(/#\/$/);
});

test('без File System Access пунктов «с диска» и «на диск» нет @firefox', async ({ page }) => {
  await page.addInitScript(() => {
    delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker;
  });
  await testMode(page);
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Импорт…' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Открыть с диска…' })).toHaveCount(0);
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const menu = page.getByRole('menu', { name: 'Меню файла' });
  await expect(menu.getByRole('menuitem', { name: 'Открыть…' })).toBeVisible();
  await expect(menu.getByRole('menuitem', { name: 'Открыть с диска…' })).toHaveCount(0);
  await expect(menu.getByRole('menuitem', { name: 'Сохранить на диск…' })).toHaveCount(0);
});

test('«Сохранить на диск…» у записи, уже связанной с диском: байты — из прежнего файла', async ({ page }) => {
  await stubPickers(page);
  const id = await storedFile(page, '&fullSave=600000');
  await fileMenu(page, 'Сохранить на диск…');
  await expect(toast(page, 'Файл сохранён на диск: Без названия.ferrum')).toHaveCount(1);
  // Второй файл — другое имя: в базе байтов уже нет, они только в первом файле.
  await page.evaluate(() => {
    (window as unknown as Record<string, unknown>).showSaveFilePicker = async () =>
      (await navigator.storage.getDirectory()).getFileHandle('второй.ferrum', { create: true });
  });
  await fileMenu(page, 'Сохранить на диск…');
  await expect(toast(page, 'Файл сохранён на диск: второй.ferrum')).toHaveCount(1);
  expect((await meta(page, id))!.disk).toBe('второй.ferrum');
  expect(await diskSize(page, 'второй.ferrum')).toBe(await diskSize(page, 'Без названия.ferrum'));
  await page.reload();
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 1');
});

test('«Сохранить на диск…» в файл другой записи — отказ, тот файл цел', async ({ page }) => {
  await stubPickers(page);
  const first = await storedFile(page, '&fullSave=600000');
  await fileMenu(page, 'Сохранить на диск…');
  await expect(toast(page, 'Файл сохранён на диск: Без названия.ferrum')).toHaveCount(1);
  const size = await diskSize(page, 'Без названия.ferrum');
  // Другая запись с двумя прямоугольниками — «Сохранить на диск…» в тот же файл.
  await nextEditor(page, async () => {
    await page.evaluate(() => (location.hash = '#/new'));
  });
  const second = fileId(page);
  expect(second).not.toBe(first);
  await drawRect(page);
  await drawRect(page, 150);
  await fileMenu(page, 'Сохранить на диск…');
  await expect(
    toast(page, 'Не удалось сохранить на диск: Этот файл принадлежит другой записи Ferrum — выберите другое имя'),
  ).toHaveCount(1);
  expect(await diskSize(page, 'Без названия.ferrum')).toBe(size);
  expect((await meta(page, second))!.disk).toBeUndefined();
  expect((await meta(page, first))!.disk).toBe('Без названия.ferrum');
});

test('файл на диске изменён другой программой: журнал в базе, уход спрашивает', async ({ page }) => {
  await testMode(page);
  await stubPickers(page);
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await writeDocToDisk(page, 'проект.ferrum');
  await nextEditor(page, () => fileMenu(page, 'Открыть с диска…'));
  const id = fileId(page);
  await page.waitForTimeout(20);
  await page.evaluate(async () => {
    const w = await (await (await navigator.storage.getDirectory()).getFileHandle('проект.ferrum')).createWritable();
    await w.write(new Uint8Array([1, 2, 3]));
    await w.close();
  });
  await drawRect(page);
  // Правка — в журнале базы (запись в файл отказала бы).
  await expect.poll(() => page.evaluate((i) => window.__ferrumFiles!.store.journalCount(i), id), { timeout: 10_000 }).toBeGreaterThan(0);
  await page.keyboard.press('ControlOrMeta+KeyS');
  const failed = 'Не удалось сохранить файл: Файл на диске изменён другой программой — сохраните копию на диск или откройте файл заново';
  await expect(toast(page, failed)).toHaveCount(1);
  // Повторная попытка — то же уведомление не повторяется.
  await page.keyboard.press('ControlOrMeta+KeyS');
  await page.waitForTimeout(300);
  await expect(toast(page, failed)).toHaveCount(1);
  await fileMenu(page, 'Все файлы');
  const ask = page.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  await expect(ask).toContainText('Файл на диске изменён другой программой');
  await ask.getByRole('button', { name: 'Остаться' }).click();
  expect(fileId(page)).toBe(id);
  expect(await page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  expect(await diskSize(page, 'проект.ferrum')).toBe(3);
  // Неудачная запись на диск журнал не стёрла.
  expect(await page.evaluate((i) => window.__ferrumFiles!.store.journalCount(i), id)).toBeGreaterThan(0);
});

test('запись на диске: из корзины — снова «Открыть с диска…»; «Удалить навсегда» файл не трогает', async ({ page }) => {
  await testMode(page);
  await stubPickers(page);
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await writeDocToDisk(page, 'проект.ferrum');
  await nextEditor(page, () => fileMenu(page, 'Открыть с диска…'));
  const id = fileId(page);
  await fileMenu(page, 'Все файлы');
  const card = page.getByRole('main').getByRole('listbox').getByRole('option', { name: 'проект', exact: true });
  await card.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Удалить' }).click();
  await expect(card).toHaveCount(0);
  // Тот же файл с диска — та же запись, уже не в корзине.
  await nextEditor(page, () => page.getByRole('button', { name: 'Открыть с диска…' }).click());
  expect(fileId(page)).toBe(id);
  expect((await meta(page, id))!.trashedAt).toBeUndefined();
  await fileMenu(page, 'Все файлы');
  await card.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Удалить' }).click();
  await page.goto('/#/trash');
  await card.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Удалить навсегда' }).click();
  const dialog = page.getByRole('dialog', { name: 'Удалить файл навсегда?' });
  await expect(dialog).toContainText('Запись «проект» будет удалена из списка; файл «проект.ferrum» на диске останется');
  await dialog.getByRole('button', { name: 'Удалить' }).click();
  await expect(card).toHaveCount(0);
  expect(await meta(page, id)).toBeNull();
  expect(await diskSize(page, 'проект.ferrum')).toBeGreaterThan(0);
});

test('база версии 3 обновляется до 4: файлы, байты, превью, журнал и настройки на месте', async ({ context }) => {
  // Старая вкладка (до File System Access): база версии 3 с данными во всех хранилищах;
  // соединение открыто и закрывается по `versionchange`, как в прежнем worker'е.
  const old = await context.newPage();
  await old.goto('/theme-init.js');
  const id = await old.evaluate(
    () =>
      new Promise<string>((resolve, reject) => {
        const req = indexedDB.open('ferrum', 3);
        req.onupgradeneeded = () => {
          const d = req.result;
          d.createObjectStore('meta', { keyPath: 'id' });
          for (const st of ['bytes', 'thumbs', 'journal', 'settings']) d.createObjectStore(st);
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
          const tx = d.transaction(['meta', 'bytes', 'thumbs', 'journal', 'settings'], 'readwrite');
          tx.objectStore('meta').put({ id, name: 'Файл третьей версии', createdAt: now, editedAt: now, openedAt: now, size: 8, thumbVersion: 1, version: 1 });
          tx.objectStore('bytes').put(new Uint8Array(8).fill(7).buffer, id);
          tx.objectStore('thumbs').put(new Blob([new Uint8Array(5)], { type: 'image/png' }), id);
          tx.objectStore('journal').put(new Uint8Array(3).buffer, [id, 1, 0]);
          tx.objectStore('settings').put('list', 'filesView');
          tx.oncomplete = () => resolve(id);
          tx.onerror = () => reject(tx.error);
        };
      }),
  );
  const page = await context.newPage();
  await testMode(page);
  await page.goto('/');
  await expect(page.getByText('Файл третьей версии')).toBeVisible();
  // Настройка из базы версии 3 — на экране: вид «Список».
  await expect(page.getByRole('radiogroup', { name: 'Вид' }).getByRole('radio', { name: 'Список' })).toHaveAttribute('aria-checked', 'true');
  expect(await old.evaluate(() => (window as unknown as { yielded?: boolean }).yielded)).toBe(true);
  expect(await page.evaluate((i) => window.__ferrumFiles!.store.journalCount(i), id)).toBe(1);
  expect(await meta(page, id)).toMatchObject({ name: 'Файл третьей версии', size: 8, version: 1 });
  expect((await meta(page, id))!.disk).toBeUndefined();
  const db = await old.evaluate(
    (i) =>
      new Promise<unknown>((resolve, reject) => {
        const req = indexedDB.open('ferrum');
        req.onerror = () => reject(req.error);
        req.onsuccess = () => {
          const d = req.result;
          const tx = d.transaction(['bytes', 'thumbs', 'journal', 'settings', 'handles']);
          const get = <T>(r: IDBRequest<T>) => new Promise<T>((ok) => (r.onsuccess = () => ok(r.result)));
          void Promise.all([
            get(tx.objectStore('bytes').get(i)),
            get(tx.objectStore('thumbs').get(i)),
            get(tx.objectStore('journal').count()),
            get(tx.objectStore('settings').get('filesView')),
            get(tx.objectStore('handles').count()),
          ]).then(([bytes, thumb, journal, view, handles]) => {
            resolve({
              version: d.version,
              bytes: (bytes as ArrayBuffer).byteLength,
              thumb: (thumb as Blob).size,
              journal,
              view,
              handles,
            });
            d.close();
          });
        };
      }),
    id,
  );
  expect(db).toEqual({ version: 4, bytes: 8, thumb: 5, journal: 1, view: 'list', handles: 0 });
});

/** Открытый с диска «проект.ferrum» (5 узлов); id записи. */
async function openLinked(page: Page): Promise<string> {
  await testMode(page);
  await stubPickers(page);
  await page.goto(newFileUrl(5));
  await editorReady(page);
  await writeDocToDisk(page, 'проект.ferrum');
  await nextEditor(page, () => fileMenu(page, 'Открыть с диска…'));
  return fileId(page);
}

/** Правка записи прямо в базе — как другая вкладка, о которой этот worker ещё не знает. */
const editDb = (page: Page, id: string, how: 'bump' | 'delete') =>
  page.evaluate(
    ([i, h]) =>
      new Promise<void>((resolve, reject) => {
        const req = indexedDB.open('ferrum');
        req.onerror = () => reject(req.error);
        req.onsuccess = () => {
          const d = req.result;
          const tx = d.transaction(['meta', 'handles'], 'readwrite');
          const meta = tx.objectStore('meta');
          const get = meta.get(i);
          get.onsuccess = () => {
            if (h === 'bump') meta.put({ ...get.result, version: get.result.version + 1 });
            // Удаление дошло до записи, а до строки дескриптора — ещё нет (гонка).
            else meta.delete(i);
          };
          tx.oncomplete = () => (d.close(), resolve());
          tx.onerror = () => reject(tx.error);
        };
      }),
    [id, how] as const,
  );

const handleRows = (page: Page, id: string) =>
  page.evaluate(
    (i) =>
      new Promise<number>((resolve, reject) => {
        const req = indexedDB.open('ferrum');
        req.onerror = () => reject(req.error);
        req.onsuccess = () => {
          const d = req.result;
          const r = d.transaction('handles').objectStore('handles').count(i);
          r.onsuccess = () => (d.close(), resolve(r.result));
        };
      }),
    id,
  );

test('файл на диске: поколение сверяется с базой до записи файла — чужая запись в нём не затирается', async ({ page }) => {
  const id = await openLinked(page);
  const before = await diskSize(page, 'проект.ferrum');
  await editDb(page, id, 'bump');
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect(
    toast(page, 'Не удалось сохранить файл: Файл изменён в другой вкладке — сохраните копию на диск или откройте файл заново'),
  ).toHaveCount(1);
  expect(await diskSize(page, 'проект.ferrum')).toBe(before);
  expect(await page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
});

test('файл на диске: запись удалена в другой вкладке — файл не тронут, осиротевшей строки дескриптора нет', async ({ page }) => {
  const id = await openLinked(page);
  const before = await diskSize(page, 'проект.ferrum');
  await editDb(page, id, 'delete');
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect(toast(page, 'Не удалось сохранить файл: Файл удалён в другой вкладке — сохраните копию на диск')).toHaveCount(1);
  expect(await diskSize(page, 'проект.ferrum')).toBe(before);
  expect(await handleRows(page, id)).toBe(0);
});

test('файл на диске удалён: «не найден», документ изменён, журнал на месте', async ({ page }) => {
  const id = await openLinked(page);
  await page.evaluate(async () => (await navigator.storage.getDirectory()).removeEntry('проект.ferrum'));
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect(toast(page, 'Не удалось сохранить файл: Файл на диске не найден — он перемещён или удалён')).toHaveCount(1);
  expect(await page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  expect(await page.evaluate((i) => window.__ferrumFiles!.store.journalCount(i), id)).toBeGreaterThan(0);
});

test('файл записан, а чтение после записи не удалось: запись удачна, следующая — без ложного «изменён другой программой»', async ({
  page,
}) => {
  // Пролог worker'а ввода-вывода: после удачного `close()` следующее `getFile()` отказывает.
  const prelude = `(() => {
  const P = FileSystemFileHandle.prototype;
  const get = P.getFile;
  const create = P.createWritable;
  let failNext = false;
  P.createWritable = async function (...a) {
    const w = await create.apply(this, a);
    const close = w.close.bind(w);
    w.close = async () => {
      await close();
      failNext = true;
    };
    return w;
  };
  P.getFile = function (...a) {
    if (failNext) {
      failNext = false;
      return Promise.reject(new DOMException('сбой чтения', 'NotReadableError'));
    }
    return get.apply(this, a);
  };
})();
`;
  await page.context().route('**/assets/io.worker-*.js', async (route) => {
    const response = await route.fetch();
    await route.fulfill({ response, body: prelude + (await response.text()) });
  });
  await openLinked(page);
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  const after = await diskSize(page, 'проект.ferrum');
  await drawRect(page, 150);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => diskSize(page, 'проект.ferrum')).not.toBe(after);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  await expect(page.locator('.fe-toast', { hasText: 'Не удалось сохранить файл' })).toHaveCount(0);
});
