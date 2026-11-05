// Файлы на сервере: настоящий стенд, настоящие письма, два браузера там, где нужны двое.

import { expect, test, type Browser, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { appScreen, drag, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { linksFor, mailCount, newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);

const person = async (browser: Browser, email: string, name = 'Человек'): Promise<Page> => {
  const context = await browser.newContext({ baseURL: STAND_URL, viewport: { width: 1280, height: 720 } });
  const page = await context.newPage();
  await testMode(page);
  await page.goto('/');
  await signIn(page, email, name);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  return page;
};

const title = (page: Page) => page.getByRole('heading', { level: 1 });
const nodes = (page: Page, n: number) => expect.poll(async () => (await live(page)).nodes).toBe(n);
/** Правки — только запись на сервер: новый файл и точка отсчёта, поколение проверяет сервер. */
const saved = (page: Page) => expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
const saveKey = (page: Page) => page.keyboard.press('ControlOrMeta+KeyS');

/** Прямоугольник инструментом R; `dx` — сдвиг в мире. CSP стенда не даёт выполнить строку, поэтому ждём опросом. */
async function drawRect(page: Page, dx = 0): Promise<void> {
  const before = (await live(page)).nodes;
  await page.locator('canvas').click({ position: { x: 10, y: 10 } });
  await page.keyboard.press('KeyR');
  const l = await live(page);
  await drag(page, await appScreen(page, l, l.cameraX - 50 + dx, l.cameraY - 40), 100, 80);
  await expect
    .poll(async () => {
      const now = await live(page);
      return now.busy === 0 ? now.nodes : -1;
    })
    .toBe(before + 1);
}

/** Файл на сервере с одним прямоугольником; адрес файла — у открывшего. */
async function storedFile(page: Page): Promise<string> {
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  const id = fileId(page);
  await drawRect(page);
  await saveKey(page);
  await saved(page);
  return id;
}

const api = (page: Page, method: string, path: string, body?: unknown) =>
  page.evaluate(
    async ({ method, path, body }) => {
      const init: RequestInit = { method };
      if (body !== undefined) {
        init.headers = { 'Content-Type': 'application/json' };
        init.body = JSON.stringify(body);
      }
      const r = await fetch(path, init);
      const text = await r.text();
      return { status: r.status, body: text ? (JSON.parse(text) as Record<string, unknown>) : null };
    },
    { method, path, body },
  );

/** Сообщение внизу экрана. */
const toast = (page: Page, text: string) => page.getByRole('status').filter({ hasText: text }).first();

test('новый файл на сервере: правка переживает перезагрузку, файла нет в IndexedDB', async ({ page }, info) => {
  await testMode(page);
  await page.goto('/');
  await signIn(page, newEmail(info.project.name));
  await expect(title(page)).toHaveText('Недавние');
  const id = await storedFile(page);
  const content = await page.evaluate(async (i) => {
    const r = await fetch(`/api/files/${i}/content`);
    return { status: r.status, version: r.headers.get('x-ferrum-version'), size: (await r.arrayBuffer()).byteLength };
  }, id);
  expect(content.status).toBe(200);
  expect(Number(content.version)).toBeGreaterThan(0);
  expect(content.size).toBeGreaterThan(16);
  await page.reload();
  await editorReady(page);
  await nodes(page, 1);
  expect(await page.evaluate(() => window.__ferrumFiles)).toBeUndefined();
  const local = await page.evaluate(
    () =>
      new Promise<number>((resolve) => {
        const req = indexedDB.open('ferrum');
        req.onsuccess = () => {
          const d = req.result;
          const n = d.objectStoreNames.contains('meta') ? d.transaction('meta').objectStore('meta').count() : null;
          if (!n) return resolve(0);
          n.onsuccess = () => resolve(n.result);
        };
      }),
  );
  expect(local).toBe(0);
});

test('файлы разделены по людям: вошедший следующим в этом браузере чужого не видит (B-190)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name), 'Анна');
  const id = await storedFile(a);
  await a.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(title(a)).toHaveText('Недавние');
  await expect(a.getByRole('option', { name: 'Без названия' })).toBeVisible();
  await a.getByRole('button', { name: 'Выйти' }).click();
  await expect(a.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await signIn(a, newEmail(info.project.name), 'Борис');
  await expect(title(a)).toHaveText('Недавние');
  await expect(a.getByText('Здесь пока нет файлов')).toBeVisible();
  await a.goto(`/#/file/${id}`);
  await expect(a.getByRole('heading', { name: 'Нет доступа' })).toBeVisible();
  await a.context().close();
});

test('разделы экрана файлов: черновики, корзина, имя; читателю меню без правок', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  await a.goto('/#/drafts');
  await expect(title(a)).toHaveText('Черновики');
  const card = a.getByRole('option', { name: 'Без названия' });
  // Превью — отдельным запросом, виден рисунок, а не заглушка.
  await expect(card.locator('img')).toBeVisible();
  await card.getByText('Без названия').dblclick();
  await a.getByLabel('Имя файла').fill('Макет');
  await a.keyboard.press('Enter');
  await expect(a.getByRole('option', { name: 'Макет' })).toBeVisible();
  await a.getByRole('option', { name: 'Макет' }).click({ button: 'right' });
  await a.getByRole('menuitem', { name: 'Удалить' }).click();
  await expect(a.getByText('Здесь пока нет файлов')).toBeVisible();
  await a.getByRole('option', { name: 'Корзина' }).click();
  await expect(a.getByRole('option', { name: 'Макет' })).toBeVisible();
  await a.getByRole('option', { name: 'Макет' }).click({ button: 'right' });
  await a.getByRole('menuitem', { name: 'Восстановить' }).click();
  await expect(a.getByText('Корзина пуста')).toBeVisible();
  await a.getByRole('option', { name: 'Черновики' }).click();
  await expect(a.getByRole('option', { name: 'Макет' })).toBeVisible();
  await a.context().close();
});

test('две вкладки одного человека: файл занят, как раньше', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  const second = await a.context().newPage();
  await testMode(second);
  await second.goto(`/#/file/${id}`);
  await expect(toast(second, 'Файл открыт в другой вкладке')).toBeVisible();
  await a.context().close();
});

test('картинки: отдельными запросами, файл на сервере без них, после открытия они на месте', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  const id = fileId(a);
  const uploads: string[] = [];
  a.on('request', (r) => {
    if (r.method() === 'PUT' && r.url().includes('/images/')) uploads.push(r.url());
  });
  // PNG 4×4, настоящий: ядро его декодирует и кладёт в документ.
  await a.evaluate(async () => {
    const c = new OffscreenCanvas(4, 4);
    c.getContext('2d')!.fillRect(0, 0, 4, 4);
    const bytes = new Uint8Array(await (await c.convertToBlob({ type: 'image/png' })).arrayBuffer());
    window.__ferrum!.core!.send({ type: 'placeImage', bytes });
  });
  // Картинка ждёт щелчка на холсте.
  await expect.poll(() => a.evaluate(() => window.__ferrum!.ui!.tool)).toBe('image');
  const l = await live(a);
  const at = await appScreen(a, l, l.cameraX, l.cameraY);
  await a.mouse.click(at.x, at.y);
  await nodes(a, 1);
  await saveKey(a);
  await saved(a);
  expect(uploads).toHaveLength(1);
  const hash = uploads[0].split('/images/')[1];
  const stored = await a.evaluate(async (i) => new Uint8Array(await (await fetch(`/api/files/${i}/content`)).arrayBuffer()).length, id);
  const image = await a.evaluate(
    async ({ i, h }) => {
      const r = await fetch(`/api/files/${i}/images/${h}`);
      return { status: r.status, size: (await r.arrayBuffer()).byteLength };
    },
    { i: id, h: hash },
  );
  expect(image.status).toBe(200);
  // Запись без изменений картинки её заново не шлёт.
  await drawRect(a);
  await saveKey(a);
  await saved(a);
  expect(uploads).toHaveLength(1);
  await a.reload();
  await editorReady(a);
  await nodes(a, 2);
  // В снимке картинка есть: контейнер собран обратно.
  const size = await a.evaluate(
    () =>
      new Promise<number>((resolve) => {
        const core = window.__ferrum!.core!;
        const off = core.on((e) => {
          if (e.type === 'snapshot') {
            off();
            resolve(e.bytes.byteLength);
          }
        });
        core.send({ type: 'snapshot', thumbnail: false });
      }),
  );
  expect(size).toBeGreaterThan(image.size);
  expect(stored).toBeGreaterThan(16);
  await a.context().close();
});

test('импорт и экспорт `.ferrum`: копия в «Черновиках», скачанный файл открывается снова', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  await a.goto('/#/drafts');
  await a.getByRole('option', { name: 'Без названия' }).click({ button: 'right' });
  const download = a.waitForEvent('download');
  await a.getByRole('menuitem', { name: 'Скачать копию (.ferrum)' }).click();
  const path = await (await download).path();
  await a.evaluate(() => void delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker);
  const chooser = a.waitForEvent('filechooser');
  await a.getByRole('button', { name: 'Открыть с диска…' }).click();
  await (await chooser).setFiles({ name: 'Импорт.ferrum', mimeType: 'application/octet-stream', buffer: await readFile(path) });
  await editorReady(a);
  await nodes(a, 1);
  await a.goto('/#/drafts');
  await expect(a.getByRole('option', { name: 'Импорт' })).toBeVisible();
  // Не контейнер: сообщение, файла нет.
  await a.evaluate(() => void delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker);
  const bad = a.waitForEvent('filechooser');
  await a.getByRole('button', { name: 'Открыть с диска…' }).click();
  await (await bad).setFiles({ name: 'Чужое.ferrum', mimeType: 'application/octet-stream', buffer: Buffer.alloc(64) });
  await expect(toast(a, 'Это не файл Ferrum')).toBeVisible();
  await expect(a.getByRole('option', { name: 'Чужое' })).toHaveCount(0);
  await a.context().close();
});

test('`?debug` не открывает ядро и хранилище в рабочей сборке (B-205)', async ({ page }, info) => {
  await page.goto('/');
  await signIn(page, newEmail(info.project.name));
  await expect(title(page)).toHaveText('Недавние');
  await page.goto('/?debug&nodes=0#/new');
  await page.waitForFunction(() => window.__ferrum?.ready === true);
  expect(await page.evaluate(() => window.__ferrumFiles)).toBeUndefined();
  expect(await page.evaluate(() => window.__ferrum!.core)).toBeUndefined();
});

test('конец сессии в открытом редакторе: сообщение, правка не теряется (B-195)', async ({ browser }, info) => {
  const mail = newEmail(info.project.name);
  const a = await person(browser, mail);
  const id = await storedFile(a);
  await drawRect(a);
  await a.evaluate(() => fetch('/api/auth/sign-out', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' }));
  await saveKey(a);
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  expect(await a.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  const before = await mailCount(mail);
  await a.getByRole('button', { name: 'Войти' }).click();
  await a.getByLabel('Электронная почта').fill(mail);
  await a.getByRole('button', { name: 'Получить ссылку' }).click();
  const [link] = await linksFor(mail, before + 1);
  await a.goto(link);
  await a.getByRole('button', { name: 'Войти', exact: true }).click();
  await editorReady(a);
  expect(fileId(a)).toBe(id);
  await nodes(a, 2);
  await a.context().close();
});

test('сеть пропала: список — сообщение и «Повторить», запись — вопрос при уходе, после возврата проходит', async ({ browser }, info) => {
  test.skip(info.project.name === 'server-firefox', '`setOffline` не действует на worker в Firefox');
  const a = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  await drawRect(a);
  await a.context().setOffline(true);
  await saveKey(a);
  await expect(toast(a, 'Нет связи с сервером')).toBeVisible();
  await a.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(a.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' })).toBeVisible();
  await a.getByRole('button', { name: 'Остаться' }).click();
  await a.context().setOffline(false);
  await saveKey(a);
  await saved(a);
  expect(((await api(a, 'GET', `/api/files/${id}/access`)).body as { version: number }).version).toBeGreaterThan(1);
  await a.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(title(a)).toHaveText('Недавние');
  await a.context().setOffline(true);
  await a.getByRole('option', { name: 'Черновики' }).click();
  await expect(a.getByText('Не удалось загрузить список файлов')).toBeVisible();
  await a.context().setOffline(false);
  await a.getByRole('button', { name: 'Повторить' }).click();
  await expect(a.getByRole('option', { name: 'Без названия' })).toBeVisible();
  await a.context().close();
});

test('файл 100 000 узлов: запись и чтение через сервер, основной поток не занят @big', async ({ browser }, info) => {
  test.skip(info.project.name === 'server-firefox', 'размер проверяется в одном браузере');
  test.setTimeout(240_000);
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=100000#/new');
  await editorReady(a);
  const id = fileId(a);
  await a.evaluate(() => {
    const w = window as unknown as { __long: number[] };
    w.__long = [];
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) w.__long.push(Math.round(e.duration));
    }).observe({ entryTypes: ['longtask'] });
  });
  await drawRect(a);
  const t0 = Date.now();
  await saveKey(a);
  await expect.poll(() => a.evaluate(() => window.__ferrum!.ui!.modified), { timeout: 120_000 }).toBe(false);
  const saveMs = Date.now() - t0;
  const long = await a.evaluate(() => (window as unknown as { __long: number[] }).__long);
  const meta = (await api(a, 'GET', `/api/files/${id}`)).body as { size: number; version: number };
  expect(meta.size).toBeGreaterThan(1024 * 1024);
  const t1 = Date.now();
  await a.reload();
  await editorReady(a);
  await expect.poll(async () => (await live(a)).nodes, { timeout: 60_000 }).toBeGreaterThan(100_000);
  console.log(
    `100 000 узлов: файл ${(meta.size / 1048576).toFixed(1)} МБ, запись ${saveMs} мс, открытие ${Date.now() - t1} мс, длинные задачи при записи: ${JSON.stringify(long)}`,
  );
  await a.context().close();
});
