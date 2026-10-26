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

// --- Перезапуск и переходы корня: блокировка файла (Web Locks) не течёт и не теряется ---

/** Держит ли вкладка блокировку файла `id`. */
const locked = (page: Page, id: string) =>
  page.evaluate(async (i) => ((await navigator.locks.query()).held ?? []).some((l) => l.name === `ferrum-file:${i}`), id);

/** Остановка ядра после открытия (тот же путь, что `fatal` от worker'а); внутри других
 * функций страницы — тем же выражением (замыкания в страницу не переносятся). */
const failCore = () => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка');

test('переход сразу после сбоя (перезапуск ещё не открыл редактор): блокировка снята, файл открывается снова', async ({ page }) => {
  const id = await storedFile(page);
  // Сбой и сразу «назад к файлам»: переход приходит, пока перезапуск перечитывает запись.
  await page.evaluate(() => {
    (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка');
    location.hash = '#/';
  });
  await expect(page.locator('.fe-files')).toHaveCount(1);
  await expect.poll(() => locked(page, id)).toBe(false);
  // Тот же файл в той же вкладке — редактор, а не «открыт в другой вкладке».
  await page.evaluate((i) => (location.hash = `#/file/${i}`), id);
  await editorReady(page);
  expect(fileId(page)).toBe(id);
  await expect(toast(page, 'Файл открыт в другой вкладке')).toHaveCount(0);
  expect(await locked(page, id)).toBe(true);
});

test('сбой после удачного ухода (файл ещё не сменился): без перезапуска, новый файл открыт с блокировкой', async ({ page }) => {
  const id = await storedFile(page);
  // Уход уже удался и блокировка снята; ядро падает, пока создаётся новый файл.
  await page.evaluate(() => {
    const store = window.__ferrumFiles!.store as unknown as { create: (...a: unknown[]) => Promise<unknown> };
    const create = store.create.bind(store);
    store.create = (...a) => {
      (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка');
      return create(...a);
    };
  });
  await page.evaluate(() => (location.hash = '#/new'));
  await expect.poll(() => /#\/file\//.test(page.url()) && fileId(page) !== id).toBe(true);
  await editorReady(page);
  const created = fileId(page);
  expect(await locked(page, created)).toBe(true);
  expect(await locked(page, id)).toBe(false);
  await expect(page.locator('.fe-boot--stopped')).toHaveCount(0);
  await expect(toast(page, 'Редактор перезапущен')).toHaveCount(0);
});

test('файл ушёл в корзину до перезапуска: экран файлов с сообщением, блокировка снята', async ({ page }) => {
  const id = await storedFile(page);
  await page.evaluate((i) => window.__ferrumFiles!.store.trash(i), id);
  await page.evaluate(failCore);
  await expect(toast(page, 'Файл в корзине — восстановите его, чтобы открыть')).toHaveCount(1);
  await expect(page.locator('.fe-files')).toHaveCount(1);
  await expect.poll(() => locked(page, id)).toBe(false);
});

test('сбой во время ухода с вопросом: «Остаться» без зависания, новый редактор, дальше уход работает', async ({ page }) => {
  const id = await storedFile(page);
  await drawRect(page, 150);
  // Ядро падает, едва уход попросил снимок: сохранить не удалось — вопрос об уходе.
  await page.evaluate(() => {
    const core = window.__ferrum!.core! as unknown as { send(c: { type: string }): void; old?: boolean };
    core.old = true;
    const send = core.send.bind(core);
    core.send = (c) => {
      send(c);
      if (c.type === 'snapshot') queueMicrotask(() => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка'));
    };
  });
  await page.evaluate(() => (location.hash = '#/'));
  await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
  await editorReady(page);
  // Перезапуск ответил на вопрос «Остаться»: тот же файл, вопроса нет.
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(fileId(page)).toBe(id);
  expect(await locked(page, id)).toBe(true);
  // Корень не завис в уходе: следующий переход проходит.
  await page.evaluate(() => (location.hash = '#/'));
  await expect(page.locator('.fe-files')).toHaveCount(1);
  await expect.poll(() => locked(page, id)).toBe(false);
});

test('хранилище не отвечает после сбоя: перезапуск не ждёт вечно; ушли к другому файлу — старый не перезапускается поверх', async ({ page }) => {
  const id = await storedFile(page, '&journal=100&fullSave=600000');
  // Запись пачек журнала повисает: перезапуск ждёт её не дольше предела.
  await page.evaluate(() => {
    const store = window.__ferrumFiles!.store as unknown as { appendJournal: () => Promise<void>; hung?: boolean };
    store.appendJournal = () => ((store.hung = true), new Promise<void>(() => {}));
  });
  await drawRect(page, 150);
  await page.waitForFunction(() => (window.__ferrumFiles!.store as unknown as { hung?: boolean }).hung === true);
  await page.evaluate(() => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка'));
  // Пока перезапуск ждёт, пользователь уходит («Уйти» — правки не сохранены) к новому файлу.
  await page.evaluate(() => (location.hash = '#/new'));
  await page.getByRole('dialog').getByRole('button', { name: 'Уйти' }).click();
  await expect.poll(() => /#\/file\//.test(page.url()) && fileId(page) !== id).toBe(true);
  await editorReady(page);
  const created = fileId(page);
  // Уведомление о перезапуске исчезает само: ловим его появление.
  await page.evaluate(() => {
    const w = window as unknown as { restartShown?: boolean };
    new MutationObserver(() => {
      if (document.body.textContent?.includes('Редактор перезапущен')) w.restartShown = true;
    }).observe(document.body, { childList: true, subtree: true, characterData: true });
  });
  // Предел ожидания прошёл: старый редактор снят — открыт по-прежнему новый (пустой) файл.
  await page.waitForTimeout(3500);
  expect(fileId(page)).toBe(created);
  expect(await page.evaluate(() => (window as unknown as { restartShown?: boolean }).restartShown ?? false)).toBe(false);
  expect(await page.evaluate(() => window.__ferrum!.core!.live!.nodes)).toBe(0);
  expect(await locked(page, created)).toBe(true);
  expect(await locked(page, id)).toBe(false);
});

test('хранилище не отвечает после сбоя: перезапуск через предел ожидания, правки — потерянными', async ({ page }) => {
  await storedFile(page, '&journal=100&fullSave=600000');
  await page.evaluate(() => {
    const store = window.__ferrumFiles!.store as unknown as { appendJournal: () => Promise<void>; hung?: boolean };
    store.appendJournal = () => ((store.hung = true), new Promise<void>(() => {}));
  });
  await drawRect(page, 150);
  await page.waitForFunction(() => (window.__ferrumFiles!.store as unknown as { hung?: boolean }).hung === true);
  await page.evaluate(() => ((window.__ferrum!.core as unknown as { old?: boolean }).old = true));
  await page.evaluate(() => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка'));
  await expect(toast(page, 'Редактор перезапущен. Правки последних секунд не сохранились')).toHaveCount(1, { timeout: 10_000 });
  await page.waitForFunction(() => window.__ferrum?.ready === true && !(window.__ferrum.core as unknown as { old?: boolean }).old);
  await editorReady(page);
});
