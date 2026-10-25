// Системный буфер обмена и файлы: копировать и вставить на другую страницу (свой
// формат в асинхронном API и запасной путь — текст с меткой в событии), вырезать,
// вставить картинку из другого приложения, копировать как PNG, обычное
// копирование в полях ввода и при открытом диалоге, не доверенный текст из
// буфера, отказ в доступе на чтение, «Сохранить как…» и «Открыть…», изображение
// из файла.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

test.use({ permissions: ['clipboard-read', 'clipboard-write'] });

const ui = (page: Page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__ferrum!.ui!)));
/** События ядра в редакторе — из журнала тестового режима (`uiEvents`). */
const events = (page: Page, type: string) =>
  page.evaluate((t) => window.__ferrum!.uiEvents!.filter((e) => e.type === t).length, type);
const errors = (page: Page) =>
  page.evaluate(() => window.__ferrum!.uiEvents!.filter((e) => e.type === 'error').map((e) => e.message));

/** Выделить «Слой 1» первого фрейма щелчком на холсте. */
async function selectShape(page: Page) {
  const at = await appScreen(page, await live(page), 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
}

async function copyToNewPage(page: Page) {
  await selectShape(page);
  await page.keyboard.press('ControlOrMeta+c');
  await expect.poll(() => events(page, 'clipboard')).toBe(1);
  await page.getByRole('button', { name: 'Добавить страницу' }).click();
  await expect.poll(async () => (await ui(page)).pages.list.length).toBe(2);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+v');
  await expect.poll(async () => (await ui(page)).layers.total).toBe(1);
  const s = (await ui(page)).summary;
  expect([s.count, s.kind, s.w.value, s.h.value]).toEqual([1, 'rect', 120, 80]);
}

/** Запасной путь записи: асинхронный API «не умеет» свой формат. */
const noCustomFormat = () => {
  (ClipboardItem as unknown as { supports?: unknown }).supports = undefined;
};

/** Событие `paste` на холсте с текстом (как его кладёт в буфер другое приложение). */
async function pasteText(page: Page, text: string) {
  await page.evaluate((text) => {
    const dt = new DataTransfer();
    dt.setData('text/plain', text);
    document.querySelector('canvas')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
  }, text);
}

test('копировать и вставить на другую страницу — свой формат', async ({ page }) => {
  await openApp(page);
  expect(await page.evaluate(() => ClipboardItem.supports('web application/x-ferrum'))).toBe(true);
  await copyToNewPage(page);
  const types = await page.evaluate(async () => (await navigator.clipboard.read())[0].types);
  expect(types).toContain('web application/x-ferrum');
});

test('запасной путь: текст с меткой в событии copy', async ({ page }) => {
  await page.addInitScript(noCustomFormat);
  await openApp(page);
  await copyToNewPage(page);
  const text = await page.evaluate(() => navigator.clipboard.readText());
  expect(text.startsWith('ferrum-clipboard:v1:')).toBe(true);
});

test('вырезать удаляет и кладёт в буфер одним шагом отмены', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  const before = await live(page);
  await page.keyboard.press('ControlOrMeta+x');
  await liveWhen(page, `l.nodes === ${before.nodes - 1} && l.selectionCount === 0`);
  await expect.poll(() => events(page, 'clipboard')).toBe(1);
  // Копирование и удаление — один шаг истории (`steps` — счётчик созданных шагов).
  expect(before.canUndo).toBe(0);
  expect((await live(page)).steps).toBe(before.steps + 1);
  // Одна отмена возвращает вырезанное, и отменять больше нечего.
  await page.keyboard.press('ControlOrMeta+z');
  await liveWhen(page, `l.nodes === ${before.nodes} && l.canUndo === 0 && l.canRedo === 1`);
  await page.keyboard.press('ControlOrMeta+v');
  await liveWhen(page, `l.nodes === ${before.nodes + 1}`);
});

test('картинка из другого приложения вставляется узлом-изображением', async ({ page }) => {
  await openApp(page, 100);
  // PNG 3 × 2 и JPEG 5 × 4 — как их кладёт в буфер другое приложение.
  for (const [type, w, h] of [['image/png', 3, 2], ['image/jpeg', 5, 4]] as const) {
    await page.evaluate(
      async ({ type, w, h }) => {
        const c = new OffscreenCanvas(w, h);
        const ctx = c.getContext('2d')!;
        ctx.fillStyle = '#ff0000';
        ctx.fillRect(0, 0, w, h);
        const blob = await c.convertToBlob({ type });
        // Буфер Chromium принимает картинки только как PNG: JPEG кладём файлом в событие.
        if (type === 'image/png') {
          await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
          return;
        }
        const dt = new DataTransfer();
        dt.items.add(new File([blob], 'photo.jpg', { type }));
        document.querySelector('canvas')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
      },
      { type, w, h },
    );
    if (type === 'image/png') {
      await page.locator('canvas').focus();
      await page.keyboard.press('ControlOrMeta+v');
    }
    await expect.poll(async () => (await ui(page)).summary?.kind).toBe('image');
    const s = (await ui(page)).summary;
    expect([s.w.value, s.h.value]).toEqual([w, h]);
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await ui(page)).summary?.count).toBe(0);
  }
  expect((await ui(page)).layers.total).toBe(1 + 2);
});

test('копировать как PNG кладёт картинку в буфер', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  await page.keyboard.press('ControlOrMeta+Shift+c');
  await expect(page.getByRole('status').filter({ hasText: 'Скопировано как PNG' })).toBeVisible();
  const size = await page.evaluate(async () => {
    const item = (await navigator.clipboard.read())[0];
    const bmp = await createImageBitmap(await item.getType('image/png'));
    return [bmp.width, bmp.height];
  });
  // «Слой 1» 120 × 80, обводка внутри — размер не растёт.
  expect(size).toEqual([120, 80]);
});

test('в поле ввода копирование, вырезание и вставка — обычный текст', async ({ page }) => {
  await openApp(page);
  await selectShape(page);
  const before = await live(page);
  const search = page.getByRole('searchbox', { name: 'Поиск слоёв' });
  await search.fill('Экран');
  await search.selectText();
  await page.keyboard.press('ControlOrMeta+c');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Экран');
  await page.keyboard.press('ControlOrMeta+x');
  await expect(search).toHaveValue('');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Экран');
  // Даже текст с меткой Ferrum вставляется в поле текстом, а не в документ.
  await page.evaluate(() => navigator.clipboard.writeText('ferrum-clipboard:v1:AQI='));
  await page.keyboard.press('ControlOrMeta+v');
  await expect(search).toHaveValue('ferrum-clipboard:v1:AQI=');
  expect(await events(page, 'clipboard')).toBe(0);
  const after = await live(page);
  expect([after.nodes, after.steps, after.selectionCount]).toEqual([before.nodes, before.steps, 1]);
  expect(await errors(page)).toEqual([]);
});

test('открытый модальный диалог: буфер обмена не трогает документ, поля диалога — обычные', async ({ page }) => {
  await page.addInitScript(noCustomFormat);
  await openApp(page);
  await selectShape(page);
  const before = await live(page);
  await page.evaluate(() => navigator.clipboard.writeText('исходный текст'));
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
  // Сочетания на вкладке диалога: ни копирования, ни вставки в документ.
  await page.keyboard.press('ControlOrMeta+c');
  await page.keyboard.press('ControlOrMeta+x');
  await page.keyboard.press('ControlOrMeta+v');
  // События на холсте (фокус потерялся) — тоже мимо документа.
  await page.evaluate(() => {
    const c = document.querySelector('canvas')!;
    for (const type of ['copy', 'cut']) c.dispatchEvent(new ClipboardEvent(type, { bubbles: true, clipboardData: new DataTransfer() }));
  });
  await pasteText(page, 'ferrum-clipboard:v1:AQI=');
  // Поле поиска диалога — обычное копирование и вставка.
  await dialog.getByRole('tab', { name: 'Горячие клавиши' }).click();
  const search = dialog.getByRole('searchbox', { name: 'Поиск команд' });
  await search.fill('Копировать');
  await search.selectText();
  await page.keyboard.press('ControlOrMeta+c');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Копировать');
  await search.fill('');
  await page.keyboard.press('ControlOrMeta+v');
  await expect(search).toHaveValue('Копировать');
  await page.waitForTimeout(300);
  expect(await events(page, 'clipboard')).toBe(0);
  const after = await live(page);
  expect([after.nodes, after.steps]).toEqual([before.nodes, before.steps]);
  expect(await errors(page)).toEqual([]);
});

test('не доверенный текст: испорченный base64 за меткой — одна ошибка без правки', async ({ page }) => {
  await openApp(page);
  const before = await live(page);
  await page.evaluate(() => navigator.clipboard.writeText('ferrum-clipboard:v1:***не base64***'));
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+v');
  await expect(page.getByRole('status').filter({ hasText: 'данные Ferrum в буфере обмена повреждены' })).toBeVisible();
  // Верный base64, но не формат Ferrum — тоже одна ошибка (её даёт разбор в ядре).
  await pasteText(page, 'ferrum-clipboard:v1:AAECAwQFBgc=');
  await expect.poll(async () => (await errors(page)).length).toBe(2);
  await page.waitForTimeout(300);
  const e = await errors(page);
  expect(e[0]).toBe('Не удалось вставить: данные Ferrum в буфере обмена повреждены');
  expect(e.length).toBe(2);
  const after = await live(page);
  expect([after.nodes, after.steps]).toEqual([before.nodes, before.steps]);
});

test('не доверенный текст: огромный текст с меткой отклоняется до декодирования', async ({ page }) => {
  await openApp(page, 100);
  const before = await live(page);
  // 90 млн символов base64 — больше предела в 64 МБ байтов.
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.setData('text/plain', 'ferrum-clipboard:v1:' + 'A'.repeat(90_000_000));
    document.querySelector('canvas')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
  });
  await expect(page.getByRole('status').filter({ hasText: 'Слишком большой объём данных в буфере обмена' })).toBeVisible();
  // До worker'а текст не дошёл: ни ошибки ядра, ни правки.
  expect(await errors(page)).toEqual([]);
  const after = await live(page);
  expect([after.nodes, after.steps]).toEqual([before.nodes, before.steps]);
});

test('огромная картинка отклоняется по заголовку без правки', async ({ page }) => {
  await openApp(page, 100);
  const before = await live(page);
  // JPEG в несколько байт объявляет 60 000 × 60 000: декодировать его нельзя.
  await page.evaluate(() => {
    const app0 = [0xff, 0xe0, 0, 16, ...new Array(14).fill(0)];
    const sof = [0xff, 0xc0, 0, 17, 8, 0xea, 0x60, 0xea, 0x60, 3, ...new Array(9).fill(0)];
    const bytes = new Uint8Array([0xff, 0xd8, ...app0, ...sof, 0xff, 0xd9]);
    window.__ferrum!.core.send({ type: 'pasteImage', bytes });
  });
  await expect(page.getByRole('status').filter({ hasText: 'изображение больше 8192 px' })).toBeVisible();
  const after = await live(page);
  expect([after.nodes, after.steps]).toEqual([before.nodes, before.steps]);
});

/** Настоящий JPEG `w × h` из шума (перекодировка в PNG заметно долгая) — в `window.__jpeg`. */
async function makeJpeg(page: Page, w: number, h: number): Promise<number> {
  return page.evaluate(
    async ({ w, h }) => {
      const c = new OffscreenCanvas(w, h);
      const ctx = c.getContext('2d')!;
      const img = ctx.createImageData(w, h);
      let x = 12345;
      for (let i = 0; i < img.data.length; i++) {
        x = (x * 1103515245 + 12345) >>> 0;
        img.data[i] = i % 4 === 3 ? 255 : x >>> 24;
      }
      ctx.putImageData(img, 0, 0);
      const bytes = new Uint8Array(await (await c.convertToBlob({ type: 'image/jpeg', quality: 0.9 })).arrayBuffer());
      (window as unknown as { __jpeg: Uint8Array }).__jpeg = bytes;
      return bytes.length;
    },
    { w, h },
  );
}

/** Правка A: дублировать «Слой 1» (один шаг истории). */
async function editA(page: Page) {
  await selectShape(page);
  const l = await live(page);
  await page.keyboard.press('ControlOrMeta+KeyD');
  return liveWhen(page, `l.steps === ${l.steps + 1} && l.nodes === ${l.nodes + 1}`);
}

test('команда после перекодируемой картинки ждёт её: отмена отменяет вставку', async ({ page }) => {
  await openApp(page, 100);
  await makeJpeg(page, 1200, 800);
  const a = await editA(page);
  await page.evaluate(() => {
    window.__ferrum!.core.send({ type: 'pasteImage', bytes: (window as unknown as { __jpeg: Uint8Array }).__jpeg });
    // Следом — команда отмены: она не должна обогнать вставку.
    window.__ferrum!.core.send({ type: 'execute', command: 'edit.undo' });
  });
  // Вставка, затем её отмена: правка A на месте, повтор вернёт картинку.
  await liveWhen(page, `l.canRedo === 1 && l.steps === ${a.steps + 1}`);
  const after = await live(page);
  expect([after.nodes, after.canUndo]).toEqual([a.nodes, 1]);
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'execute', command: 'edit.redo' }));
  await liveWhen(page, `l.nodes === ${a.nodes + 1}`);
  expect((await ui(page)).summary?.kind).toBe('image');
});

for (const transport of ['sab', 'message'] as const) {
  test(`ввод во время перекодировки ждёт её: Ctrl+Z отменяет вставку, а не прошлую правку (${transport})`, async ({ page }) => {
    await openApp(page, 100, `&transport=${transport}`);
    const size = await makeJpeg(page, 3000, 2000);
    expect(size).toBeGreaterThan(1024 * 1024);
    const a = await editA(page);
    // Вставка и сразу — клавиша отмены (в том же задании: точно во время перекодировки).
    const t = await page.evaluate(() => {
      window.__ferrum!.core.send({ type: 'pasteImage', bytes: (window as unknown as { __jpeg: Uint8Array }).__jpeg });
      const mac = navigator.platform.toLowerCase().startsWith('mac');
      const key = { code: 'KeyZ', key: 'z', ctrlKey: !mac, metaKey: mac, bubbles: true };
      window.dispatchEvent(new KeyboardEvent('keydown', key));
      window.dispatchEvent(new KeyboardEvent('keyup', key));
      return performance.now();
    });
    await liveWhen(page, `l.canRedo === 1 && l.steps === ${a.steps + 1}`);
    const after = await live(page);
    console.log(`перекодировка 3000 × 2000: ${(size / 1024 / 1024).toFixed(1)} МБ, до отмены ${Math.round((await page.evaluate(() => performance.now())) - t)} мс`);
    // Отменена вставка: узлов — как после правки A, A ещё можно отменить, повтор вернёт картинку.
    expect([after.nodes, after.canUndo, after.canRedo]).toEqual([a.nodes, 1, 1]);
    await page.evaluate(() => window.__ferrum!.core.send({ type: 'execute', command: 'edit.redo' }));
    await liveWhen(page, `l.nodes === ${a.nodes + 1}`);
    expect((await ui(page)).summary?.kind).toBe('image');
    expect(await page.evaluate(() => window.__ferrum!.core.mode)).toBe(transport);
  });
}

/** Длинные задачи основного потока с этого момента — в `window.__long`. */
async function watchLongTasks(page: Page): Promise<number> {
  return page.evaluate(() => {
    const w = window as unknown as { __long: { start: number; duration: number }[] };
    w.__long = [];
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) w.__long.push({ start: e.startTime, duration: e.duration });
    }).observe({ type: 'longtask', buffered: false });
    return performance.now();
  });
}
const longTasks = (page: Page, t0: number) =>
  page.evaluate((t0) => (window as unknown as { __long: { start: number; duration: number }[] }).__long.filter((e) => e.start >= t0), t0);

test('запасной путь: большое выделение кодирует worker — без длинных задач, вставка возвращает всё', async ({ page }) => {
  await page.addInitScript(noCustomFormat);
  await openApp(page, 20_000);
  const before = await live(page);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+a');
  await liveWhen(page, 'l.selectionCount > 100');
  const t0 = await watchLongTasks(page);
  await page.keyboard.press('ControlOrMeta+c');
  await expect.poll(() => events(page, 'clipboard')).toBe(1);
  await expect
    .poll(() => page.evaluate(() => navigator.clipboard.readText().then((t) => t.startsWith('ferrum-clipboard:v1:'))))
    .toBe(true);
  await page.waitForTimeout(300);
  const long = await longTasks(page, t0);
  const bytes = await page.evaluate(() => window.__ferrum!.uiEvents!.find((e) => e.type === 'clipboard')!.size!);
  console.log(`копирование ${before.nodes} узлов: ${(bytes / 1024 / 1024).toFixed(1)} МБ, длинных задач: ${long.length}`, long);
  expect(long).toEqual([]);
  expect(await errors(page)).toEqual([]);
  await page.getByRole('button', { name: 'Добавить страницу' }).click();
  await expect.poll(async () => (await ui(page)).pages.list.length).toBe(2);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+v');
  await liveWhen(page, `l.nodes === ${before.nodes}`);
  expect(await errors(page)).toEqual([]);
});

test('картинка больше 64 МБ отклоняется до копии в worker', async ({ page }) => {
  await openApp(page, 100);
  const before = await live(page);
  // Файл-картинка 65 МБ в событии вставки: отказ по размеру Blob, без чтения.
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array(65 * 1024 * 1024)], 'big.png', { type: 'image/png' }));
    document.querySelector('canvas')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
  });
  await expect(page.getByRole('status').filter({ hasText: 'Изображение слишком большое для вставки' })).toBeVisible();
  // Команда в обход интерфейса — отказ предела команды (до передачи worker'у).
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'pasteImage', bytes: new Uint8Array(65 * 1024 * 1024) }));
  await expect.poll(() => errors(page)).toEqual(['Слишком большой объём данных: 65 МБ, предел для этой команды — 64 МБ.']);
  expect((await live(page)).nodes).toBe(before.nodes);
});

test('«Сохранить как…» и «Открыть…»', async ({ page }) => {
  await page.addInitScript(() => {
    delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker;
  });
  await openApp(page, 300);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const download = page.waitForEvent('download');
  await page.getByRole('menuitem', { name: 'Сохранить как…' }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('Без названия.ferrum');
  const bytes = Buffer.concat(await (await file.createReadStream()).toArray());
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: 'Создать' }).click();
  await expect.poll(async () => (await ui(page)).layers.total).toBe(0);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('menuitem', { name: 'Открыть…' }).click();
  await (await chooser).setFiles({ name: 'проект.ferrum', mimeType: 'application/octet-stream', buffer: bytes });
  await expect.poll(async () => (await ui(page)).layers.total).toBe(3);
  await liveWhen(page, 'l.nodes === 300');
});

test('«Сохранить как…»: имя файла из имени документа очищается', async ({ page }) => {
  await openApp(page, 10);
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'setDocumentName', name: 'План: этаж 1/2' }));
  await expect.poll(async () => (await ui(page)).documentName).toBe('План: этаж 1/2');
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const download = page.waitForEvent('download');
  await page.getByRole('menuitem', { name: 'Сохранить как…' }).click();
  expect((await download).suggestedFilename()).toBe('План_ этаж 1_2.ferrum');
});

test('«Открыть…» при правках спрашивает; отмена и испорченный файл оставляют документ', async ({ page }) => {
  await page.addInitScript(() => {
    delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker;
  });
  await openApp(page);
  await selectShape(page);
  await page.keyboard.press('ControlOrMeta+KeyD');
  const edited = await liveWhen(page, 'l.steps === 1');
  const open = async () => {
    await page.getByRole('button', { name: 'Меню файла' }).click();
    await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: 'Открыть…' }).click();
  };
  let chooserShown = false;
  page.on('filechooser', () => (chooserShown = true));
  await open();
  const dialog = page.getByRole('dialog', { name: 'Открыть другой файл?' });
  await expect(dialog).toContainText('Несохранённые изменения будут потеряны');
  await expect(dialog.getByRole('button', { name: 'Отмена' })).toBeFocused();
  await dialog.getByRole('button', { name: 'Отмена' }).click();
  await expect(dialog).toHaveCount(0);
  await page.waitForTimeout(300);
  expect(chooserShown).toBe(false);
  expect((await live(page)).nodes).toBe(edited.nodes);
  // Подтверждение → выбор файла → испорченный файл: одна ошибка, документ прежний.
  await open();
  const chooser = page.waitForEvent('filechooser');
  await dialog.getByRole('button', { name: 'Открыть' }).click();
  await (await chooser).setFiles({ name: 'битый.ferrum', mimeType: 'application/octet-stream', buffer: Buffer.from('не ferrum') });
  await expect.poll(async () => (await errors(page)).length).toBe(1);
  await page.waitForTimeout(300);
  const e = await errors(page);
  expect(e.length).toBe(1);
  expect(e[0]).toMatch(/^Не удалось открыть файл: /);
  const after = await live(page);
  expect([after.nodes, after.steps]).toEqual([edited.nodes, edited.steps]);
});

test('«Сохранить как…» на 100 000 узлах — без длинных задач основного потока', async ({ page }) => {
  await openApp(page, 100_000);
  await page.evaluate(() => {
    const w = window as unknown as { __long: { start: number; duration: number }[] };
    w.__long = [];
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) w.__long.push({ start: e.startTime, duration: e.duration });
    }).observe({ type: 'longtask', buffered: false });
  });
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const download = page.waitForEvent('download');
  const t0 = await page.evaluate(() => performance.now());
  await page.getByRole('menuitem', { name: 'Сохранить как…' }).click();
  const file = await download;
  const size = (await (await file.createReadStream()).toArray()).reduce((n, b: Buffer) => n + b.length, 0);
  await page.waitForTimeout(500);
  const long = await page.evaluate(
    (t0) => (window as unknown as { __long: { start: number; duration: number }[] }).__long.filter((e) => e.start >= t0),
    t0,
  );
  console.log(`сохранение 100 000 узлов: ${(size / 1024 / 1024).toFixed(1)} МБ, длинных задач: ${long.length}`, long);
  expect(size).toBeGreaterThan(1024 * 1024);
  expect(long).toEqual([]);
});

test('изображение из файла: Ctrl+Shift+K, выбор, щелчок на холсте', async ({ page }) => {
  await page.addInitScript(() => {
    delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker;
  });
  await openApp(page, 0);
  const png = await page.evaluate(async () => {
    const c = new OffscreenCanvas(40, 30);
    c.getContext('2d')!.fillRect(0, 0, 40, 30);
    return Array.from(new Uint8Array(await (await c.convertToBlob({ type: 'image/png' })).arrayBuffer()));
  });
  await page.locator('canvas').focus();
  const chooser = page.waitForEvent('filechooser');
  await page.keyboard.press('ControlOrMeta+Shift+k');
  await (await chooser).setFiles({ name: 'logo.png', mimeType: 'image/png', buffer: Buffer.from(png) });
  const box = (await page.locator('canvas').boundingBox())!;
  await page.waitForTimeout(200);
  await page.mouse.click(box.x + 200, box.y + 200);
  await expect.poll(async () => (await ui(page)).summary?.kind).toBe('image');
  expect((await ui(page)).summary.w.value).toBe(40);
});

test('свой формат без доступа на чтение: одна подсказка за сеанс', async ({ page }) => {
  // Свой формат записан, а чтение отказано (как после «Запретить» в запросе
  // Chromium). Headless без права чтения не пишет и свой формат — отказ имитируем.
  await page.addInitScript(() => {
    navigator.clipboard.read = () => Promise.reject(new DOMException('Read permission denied.', 'NotAllowedError'));
  });
  await openApp(page);
  const hint = page.getByRole('status').filter({ hasText: 'Разрешите доступ к буферу обмена, чтобы вставлять объекты Ferrum' });
  await selectShape(page);
  await page.keyboard.press('ControlOrMeta+c');
  await expect.poll(() => events(page, 'clipboard')).toBe(1);
  // Записан именно свой формат: в тексте буфера метки нет.
  await page.waitForTimeout(300);
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('');
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+v');
  await expect(hint).toHaveCount(1);
  await page.keyboard.press('ControlOrMeta+v');
  // Уведомление ушло по времени — следующая вставка его не повторяет.
  await expect(hint).toHaveCount(0, { timeout: 10_000 });
  await page.keyboard.press('ControlOrMeta+v');
  await page.waitForTimeout(500);
  await expect(hint).toHaveCount(0);
  expect(await errors(page)).toEqual([]);
});

test.describe('без доступа на чтение буфера', () => {
  test.use({ permissions: ['clipboard-write'] });

  test('вставка берёт данные события, отказ в чтении — без лишних ошибок', async ({ page }) => {
    await page.addInitScript(noCustomFormat);
    await openApp(page);
    // Чтение действительно запрещено.
    expect(await page.evaluate(() => navigator.clipboard.read().then(() => 'ok', (e: Error) => e.name))).not.toBe('ok');
    await copyToNewPage(page);
    // Пустое событие вставки: асинхронное чтение отказано — молча.
    await page.evaluate(() =>
      document.querySelector('canvas')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: new DataTransfer(), bubbles: true })),
    );
    // Чужой текст — не повод спрашивать доступ.
    await page.evaluate(() => {
      const dt = new DataTransfer();
      dt.setData('text/plain', 'просто текст');
      document.querySelector('canvas')!.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
    });
    await page.waitForTimeout(300);
    expect(await errors(page)).toEqual([]);
    expect(await page.getByRole('status').filter({ hasText: 'Нет доступа к буферу обмена' }).count()).toBe(0);
    // «Вставить» из меню читает только асинхронно: без доступа — одно уведомление.
    const at = await appScreen(page, await live(page), 0, 0);
    await page.mouse.click(at.x, at.y, { button: 'right' });
    await page.getByRole('menuitem', { name: /Вставить/ }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Нет доступа к буферу обмена' })).toHaveCount(1);
  });
});

test('запись отклонена браузером: вырезание подсказывает отмену, копирование — повтор', async ({ page }) => {
  await page.addInitScript(() => {
    navigator.clipboard.write = () => Promise.reject(new DOMException('нет доступа', 'NotAllowedError'));
    document.execCommand = () => false;
  });
  await openApp(page);
  await selectShape(page);
  await page.keyboard.press('ControlOrMeta+c');
  await expect(page.getByRole('status').filter({ hasText: 'Повторите копирование' })).toBeVisible();
  const before = await live(page);
  await page.keyboard.press('ControlOrMeta+x');
  await expect(page.getByRole('status').filter({ hasText: 'Отмена вернёт вырезанные объекты' })).toBeVisible();
  await liveWhen(page, `l.nodes === ${before.nodes - 1}`);
  await page.keyboard.press('ControlOrMeta+z');
  await liveWhen(page, `l.nodes === ${before.nodes}`);
});

test('«Открыть…»: файл больше предела отклоняется до чтения', async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as unknown as { __read: boolean; showOpenFilePicker: unknown };
    w.__read = false;
    const file = {
      size: 600 * 1024 * 1024,
      arrayBuffer: () => {
        w.__read = true;
        return Promise.reject(new Error('прочитан'));
      },
    };
    w.showOpenFilePicker = async () => [{ getFile: async () => file }];
  });
  await openApp(page, 10);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: 'Открыть…' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Не удалось открыть файл: больше 512 МБ' })).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { __read: boolean }).__read)).toBe(false);
});

test('свой формат отклонён браузером: текст с меткой кодирует worker по запросу', async ({ page }) => {
  await page.addInitScript(() => {
    navigator.clipboard.write = () => Promise.reject(new DOMException('нет доступа', 'NotAllowedError'));
  });
  await openApp(page);
  // Свой формат «поддерживается»: событие `clipboard` текста не несёт, его просят у worker'а.
  await copyToNewPage(page);
  expect((await page.evaluate(() => navigator.clipboard.readText())).startsWith('ferrum-clipboard:v1:')).toBe(true);
  expect(await errors(page)).toEqual([]);
});
