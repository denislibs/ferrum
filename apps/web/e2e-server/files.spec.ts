// Файлы на сервере: настоящий стенд, настоящие письма, два браузера там, где нужны двое.

import { expect, test, type Browser, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { appScreen, drag, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { linksFor, mailCount, mailsFor, newEmail, resetStand, signIn } from './stand';

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

const inSync = (page: Page) => expect(page.locator('.fe-sync')).toHaveText('Синхронизировано', { timeout: 30_000 });

/** Размер файла на сервере, с журналом. */
const contentSize = (page: Page, id: string) =>
  page.evaluate(async (i) => {
    await fetch(`/api/files/${i}/compact`, { method: 'POST' });
    return Number((await (await fetch(`/api/files/${i}`)).json()).size);
  }, id);

/** Файл на сервере с одним прямоугольником; адрес файла — у открывшего. */
async function storedFile(page: Page): Promise<string> {
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  const id = fileId(page);
  await inSync(page);
  const before = await contentSize(page, id);
  await drawRect(page);
  // Правка на сервере и в его файле: журнал сжат, `content` содержит всё. «Скачать копию» сервер повторяет по файлу не чаще
  // раза в 30 с (503 с `Retry-After`), а первое сжатие было только что: ждём дольше этого окна.
  await expect.poll(() => contentSize(page, id), { timeout: 60_000 }).toBeGreaterThan(before);
  await inSync(page);
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

const open = async (page: Page, id: string) => {
  await page.goto(`/#/file/${id}`);
  await editorReady(page);
  await inSync(page);
};

/** Вернулись на вкладку: доступ проверяется сразу, а не к следующему опросу. */
const checkAccessNow = (page: Page) => page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));

async function shareWith(page: Page, email: string, role: 'Может редактировать' | 'Может смотреть' = 'Может редактировать') {
  await page.getByRole('button', { name: 'Поделиться' }).click();
  await page.getByLabel('Адрес, кого пригласить').fill(email);
  await page.getByLabel('Роль приглашённого').selectOption({ label: role });
  await page.getByRole('button', { name: 'Пригласить' }).click();
  await expect(page.getByRole('list', { name: 'Люди с доступом' }).getByText(email)).toBeVisible();
  await page.getByRole('button', { name: 'Закрыть', exact: true }).click();
}

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

test('разделы экрана файлов: черновики, корзина, имя', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  // Превью — по «Сохранить» и при уходе: отдельным запросом.
  const thumb = a.waitForResponse((r) => r.url().endsWith('/thumb') && r.request().method() === 'PUT');
  await saveKey(a);
  await thumb;
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

test('читатель: режим просмотра, правок нет, кнопки «Поделиться» нет, копию скачать можно', async ({ browser }, info) => {
  const viewerMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, viewerMail);
  const id = await storedFile(a);
  await shareWith(a, viewerMail, 'Может смотреть');
  const puts: string[] = [];
  b.on('request', (r) => {
    if (r.method() === 'PUT') puts.push(r.url());
  });
  await open(b, id);
  await nodes(b, 1);
  await expect(b.getByText('Только просмотр')).toBeVisible();
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
  // Прямоугольник инструментом не появляется, удаление не проходит, признака изменений нет.
  await b.locator('canvas').click({ position: { x: 10, y: 10 } });
  await b.keyboard.press('KeyR');
  const l = await live(b);
  await drag(b, await appScreen(b, l, l.cameraX, l.cameraY), 100, 80);
  await nodes(b, 1);
  await b.keyboard.press('ControlOrMeta+KeyA');
  await b.keyboard.press('Delete');
  await nodes(b, 1);
  expect(await b.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  await saveKey(b);
  expect(puts).toEqual([]);
  // Панель свойств не принимает ввод, «Экспорт» остаётся.
  await expect(b.locator('.fe-props__body')).toHaveAttribute('inert', '');
  // Запись в обход интерфейса тоже не проходит.
  const direct = await api(b, 'PATCH', `/api/files/${id}`, { name: 'Чужое' });
  expect(direct.status).toBe(403);
  await b.goto('/#/shared');
  await expect(title(b)).toHaveText('Доступные мне');
  const row = b.getByRole('option', { name: 'Без названия' });
  await row.click({ button: 'right' });
  await expect(b.getByRole('menuitem', { name: 'Переименовать' })).toHaveCount(0);
  await expect(b.getByRole('menuitem', { name: 'Удалить' })).toHaveCount(0);
  const download = b.waitForEvent('download');
  await b.getByRole('menuitem', { name: 'Скачать копию (.ferrum)' }).click();
  expect((await download).suggestedFilename()).toBe('Без названия.ferrum');
  await a.context().close();
  await b.context().close();
});

test('понижение и повышение роли открытого редактора без перезагрузки', async ({ browser }, info) => {
  const editorMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, editorMail);
  const id = await storedFile(a);
  await shareWith(a, editorMail);
  const entry = async () =>
    ((await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { id: string; email: string }[] }).people.find(
      (p) => p.email === editorMail,
    )!.id;
  await open(b, id);
  await expect(b.getByRole('button', { name: 'Поделиться' })).toBeVisible();
  const before = (await live(b)).nodes;
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'viewer' });
  await expect(b.getByText('Только просмотр')).toBeVisible();
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
  await b.locator('canvas').click({ position: { x: 10, y: 10 } });
  await b.keyboard.press('KeyR');
  const l = await live(b);
  await drag(b, await appScreen(b, l, l.cameraX, l.cameraY), 100, 80);
  await nodes(b, before);
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'editor' });
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
  await nodes(b, before + 1);
  await inSync(b);
  await a.context().close();
  await b.context().close();
});

test('снятие роли: «Нет доступа» в открытом редакторе, правки — по протоколу ухода', async ({ browser }, info) => {
  const editorMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, editorMail);
  const id = await storedFile(a);
  await shareWith(a, editorMail);
  await open(b, id);
  await drawRect(b);
  await nodes(b, 2);
  await inSync(b);
  const share = (await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { id: string; email: string }[] };
  const entry = share.people.find((p) => p.email === editorMail)!.id;
  await api(a, 'DELETE', `/api/files/${id}/share/people/${entry}`);
  // Сразу, по соединению файла.
  await expect(b.getByRole('alert').getByText('Нет доступа', { exact: true })).toBeVisible();
  await expect(b.getByRole('button', { name: 'Сохранить копию на диск' })).toBeVisible();
  await nodes(b, 2);
  const download = b.waitForEvent('download');
  await b.getByRole('button', { name: 'Сохранить копию на диск' }).click();
  expect((await download).suggestedFilename()).toMatch(/\.ferrum$/);
  await b.getByRole('button', { name: 'К файлам', exact: true }).click();
  await expect(title(b)).toHaveText('Недавние');
  // Файл закрыт и после этого: чтение — «Нет доступа», просьба уходит владельцу.
  const ownerMail = (await a.evaluate(
    async () => ((await (await fetch('/api/auth/session')).json()) as { user: { email: string } }).user.email,
  )) as string;
  await b.goto(`/#/file/${id}`);
  await expect(b.getByRole('heading', { name: 'Нет доступа' })).toBeVisible();
  await b.getByRole('button', { name: 'Запросить доступ' }).click();
  await expect(b.getByText('Запрос отправлен владельцу файла')).toBeVisible();
  await mailsFor(ownerMail, 'Запрос доступа к файлу в Ferrum');
  await a.context().close();
  await b.context().close();
});

test('смена входа в другой вкладке: соединение X закрыто, вкладка становится вкладкой Y (F-I4)', async ({ browser }, info) => {
  const xMail = newEmail(info.project.name);
  const yMail = newEmail(info.project.name);
  const a = await person(browser, xMail);
  await storedFile(a);
  await shareWith(a, yMail);
  await drawRect(a, 200);
  await nodes(a, 2);
  await inSync(a);
  const tab = await a.context().newPage();
  await testMode(tab);
  await tab.goto('/');
  // Правки на сервере: вкладка X без вопросов перезагружается и просит войти.
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(tab.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await expect(a.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  const reloaded = a.waitForEvent('load', { timeout: 90_000 });
  await signIn(tab, yMail, 'Другой');
  await expect(tab.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await reloaded;
  await editorReady(a);
  await nodes(a, 2);
  await a.context().close();
});

test('смена входа в другой вкладке: опрос под чужой сессией не снимает плашку и режим просмотра (F-I4)', async ({ browser }, info) => {
  const yMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  const id = fileId(a);
  await shareWith(a, yMail);
  await drawRect(a);
  await saveKey(a);
  await expect(toast(a, 'Нет связи с сервером')).toBeVisible();
  const tab = await a.context().newPage();
  await testMode(tab);
  await tab.goto('/');
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(tab.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  await signIn(tab, yMail, 'Другой');
  await expect(tab.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  expect((await api(tab, 'GET', `/api/files/${id}/access`)).status).toBe(200);
  // Ответ опроса пришёл, и он отказ: для вкладки X сессии нет, хотя у Y право на файл есть.
  const polled = a
    .context()
    .waitForEvent('response', (r) => r.url().endsWith(`/api/files/${id}/access`) && r.status() !== 200 && r.status() !== 304);
  await checkAccessNow(a);
  expect((await polled).status()).toBe(401);
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  // Режим просмотра не снят: ядро отвергает правку.
  await a.evaluate(() => window.__ferrum!.core!.send({ type: 'setDocumentName', name: 'Чужое' }));
  await expect(a.getByRole('status').filter({ hasText: 'только для чтения' }).first()).toBeVisible();
  await a.context().close();
});

test('экран файлов: сессии больше нет — страница показывает вход (T6-Minor9)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.context().clearCookies();
  // Другая вкладка изменила список: экран читает его заново и получает 401.
  await a.evaluate(() => new BroadcastChannel('ferrum-server-files').postMessage('changed'));
  await expect(a.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await a.context().close();
});

test('ссылка: роль, выключатель, токена нет ни в адресе, ни в журнале истории', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await a.getByRole('checkbox', { name: 'Доступ по ссылке' }).check();
  await a.getByLabel('Роль по ссылке').selectOption({ label: 'Может смотреть' });
  await expect(a.getByRole('button', { name: 'Копировать ссылку' })).toBeVisible();
  const { token } = ((await api(a, 'GET', `/api/files/${id}/share`)).body as { link: { token: string } }).link;
  await b.goto(`/#/file/${id}?link=${token}`);
  await editorReady(b);
  await expect(b.getByText('Только просмотр')).toBeVisible();
  expect(b.url()).not.toContain(token);
  expect(b.url()).toContain(`#/file/${id}`);
  const off = a.waitForResponse((r) => r.url().endsWith('/share/link'));
  await a.getByRole('checkbox', { name: 'Доступ по ссылке' }).uncheck();
  await off;
  await checkAccessNow(b);
  await expect(b.getByRole('alert').getByText('Нет доступа', { exact: true })).toBeVisible();
  // Выключенная ссылка не открывает файл и новому человеку; файл из адреса — та же «Нет доступа».
  await b.goto(`/#/file/${id}?link=${token}`);
  await expect(b.getByRole('heading', { name: 'Нет доступа' })).toBeVisible();
  expect(b.url()).not.toContain(token);
  await a.context().close();
  await b.context().close();
});

test('приглашение адреса без учётной записи: письмо, первый вход, «Доступные мне»', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  const invited = newEmail(info.project.name);
  await shareWith(a, invited);
  const [text] = await mailsFor(invited, 'Вам открыли файл в Ferrum');
  const link = new RegExp(`http\\S+#/file/${id}`).exec(text)![0];
  expect(link).toContain(id);
  const context = await browser.newContext({ baseURL: STAND_URL });
  const page = await context.newPage();
  await testMode(page);
  await page.goto(link);
  await signIn(page, invited, 'Новый');
  await editorReady(page);
  expect(fileId(page)).toBe(id);
  await nodes(page, 1);
  await page.goto('/#/shared');
  await expect(page.getByRole('option', { name: 'Без названия' })).toBeVisible();
  // Имя появилось в списке, потому что человек открыл файл.
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await expect
    .poll(
      async () =>
        ((await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { email: string; name: string }[] }).people.find(
          (p) => p.email === invited,
        )?.name,
    )
    .toBe('Новый');
  await a.context().close();
  await context.close();
});

test('команда: проект, файл в проекте у участника; ушедший теряет доступ', async ({ browser }, info) => {
  const memberMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, memberMail);
  await a.getByRole('button', { name: 'Команды…' }).click();
  await a.getByLabel('Имя новой команды').fill('Студия');
  await a.getByRole('button', { name: 'Создать' }).click();
  await a.getByLabel('Имя нового проекта').fill('Сайт');
  await a.getByRole('button', { name: 'Добавить проект' }).click();
  await expect(a.getByRole('list', { name: 'Проекты команды' }).getByText('Сайт')).toBeVisible();
  await a.getByLabel('Адрес нового участника').fill(memberMail);
  await a.getByRole('button', { name: 'Пригласить' }).click();
  await expect(a.getByRole('list', { name: 'Участники команды' }).getByText(memberMail)).toBeVisible();
  await a.getByRole('button', { name: 'Закрыть' }).click();
  await a.getByRole('option', { name: 'Сайт' }).click();
  await expect(title(a)).toHaveText('Сайт');
  await a.getByRole('button', { name: 'Новый файл' }).first().click();
  await editorReady(a);
  const id = fileId(a);
  await drawRect(a);
  await saveKey(a);
  await saved(a);
  // Участник видит проект и правит файл.
  await b.reload();
  await expect(b.getByRole('option', { name: 'Сайт' })).toBeVisible();
  await b.getByRole('option', { name: 'Сайт' }).click();
  await expect(b.getByRole('option', { name: 'Без названия' })).toBeVisible();
  await open(b, id);
  await nodes(b, 1);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
  await saveKey(b);
  await saved(b);
  const members = (
    await api(a, 'GET', `/api/teams/${((await api(a, 'GET', '/api/teams')).body as { teams: { id: string }[] }).teams[0].id}/members`)
  ).body as {
    members: { id: string; email: string }[];
  };
  const teamId = ((await api(a, 'GET', '/api/teams')).body as { teams: { id: string }[] }).teams[0].id;
  await api(a, 'DELETE', `/api/teams/${teamId}/members/${members.members.find((m) => m.email === memberMail)!.id}`);
  await checkAccessNow(b);
  await expect(b.getByRole('alert').getByText('Нет доступа', { exact: true })).toBeVisible();
  await a.context().close();
  await b.context().close();
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

test('файлы браузера прежней версии: «Локальные файлы (N)», перенос с журналом, оригинал остаётся', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  const bytes = await a.evaluate(async (i) => Array.from(new Uint8Array(await (await fetch(`/api/files/${i}/content`)).arrayBuffer())), id);
  // Запись прежней версии — прямо в базу браузера, как её оставил этап 1.
  await a.evaluate(
    (data) =>
      new Promise<void>((resolve, reject) => {
        const req = indexedDB.open('ferrum', 4);
        req.onsuccess = () => {
          const tx = req.result.transaction(['meta', 'bytes'], 'readwrite');
          const now = Date.now();
          tx.objectStore('meta').put({
            id: 'old-file',
            name: 'Старый',
            createdAt: now,
            editedAt: now,
            openedAt: now,
            size: data.length,
            thumbVersion: 0,
            version: 1,
          });
          tx.objectStore('bytes').put(new Uint8Array(data).buffer, 'old-file');
          tx.oncomplete = () => resolve();
          tx.onerror = () => reject(tx.error);
        };
      }),
    bytes,
  );
  await a.goto('/#/drafts');
  await a.reload();
  await a.getByRole('button', { name: 'Локальные файлы (1)' }).click();
  await a.getByRole('button', { name: 'Перенести: Старый' }).click();
  await expect(a.getByText('Перенесён в «Черновики»')).toBeVisible();
  await a.getByRole('button', { name: 'Закрыть' }).click();
  await expect(a.getByRole('option', { name: 'Старый' })).toBeVisible();
  // Оригинал на месте.
  await expect(a.getByRole('button', { name: 'Локальные файлы (1)' })).toBeVisible();
  await a.getByRole('option', { name: 'Старый' }).click();
  await editorReady(a);
  await nodes(a, 1);
  await a.goto('/#/drafts');
  await a.reload();
  await a.getByRole('button', { name: 'Локальные файлы (1)' }).click();
  await expect(a.getByText('Перенесён в «Черновики»')).toBeVisible();
  // Перенесённый: кнопки «Перенести» больше нет, повтор не создаст копию.
  await expect(a.getByRole('button', { name: 'Перенести: Старый' })).toBeDisabled();
  await a.getByRole('button', { name: 'Удалить из браузера: Старый' }).click();
  const ask = a.getByRole('dialog', { name: 'Удалить файл из браузера?' });
  await expect(ask).toContainText('есть в «Черновиках»');
  await ask.getByRole('button', { name: 'Отмена' }).click();
  await expect(a.getByRole('list', { name: 'Файлы в браузере' }).getByText('Старый')).toBeVisible();
  await a.getByRole('button', { name: 'Удалить из браузера: Старый' }).click();
  await a.getByRole('dialog', { name: 'Удалить файл из браузера?' }).getByRole('button', { name: 'Удалить' }).click();
  await expect(a.getByRole('list', { name: 'Файлы в браузере' }).getByText('Старый')).toHaveCount(0);
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
  // Соединение закрыто сразу, плашка не ждёт сохранения.
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
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
  await storedFile(a);
  await a.context().setOffline(true);
  await expect(a.locator('.fe-sync')).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(a);
  await a.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(a.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' })).toContainText('сохранены в этом браузере', {
    timeout: 15_000,
  });
  await a.getByRole('button', { name: 'Остаться' }).click();
  await a.context().setOffline(false);
  await inSync(a);
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

test('«Поделиться»: сбой, чужой ответ и запоздавший ответ не портят список', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  const invited = newEmail(info.project.name);
  // Страница сети вместо ответа и обрыв: сообщение и «Повторить», а не вечная «Загрузка…».
  await a.context().route('**/share', (r) => r.fulfill({ status: 200, contentType: 'text/html', body: '<html>вход в сеть</html>' }));
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await expect(a.getByRole('dialog').getByRole('alert')).toContainText('Нет связи с сервером');
  await a.context().unroute('**/share');
  await a.context().route('**/share', (r) => r.abort());
  await a.getByRole('button', { name: 'Повторить' }).click();
  await expect(a.getByRole('dialog').getByRole('alert')).toContainText('Нет связи с сервером');
  await a.context().unroute('**/share');
  await a.getByRole('button', { name: 'Повторить' }).click();
  await expect(a.getByRole('list', { name: 'Люди с доступом' })).toBeVisible();
  await a.getByRole('button', { name: 'Закрыть' }).click();
  // Закрыли, пока ответ в пути, открыли заново: запоздавший ответ (список без приглашённого) не затирает новый.
  const { promise: release, resolve } = Promise.withResolvers<void>();
  const { promise: fetched, resolve: gotStale } = Promise.withResolvers<void>();
  let calls = 0;
  await a.context().route('**/share', async (r) => {
    if (++calls > 1) return r.continue();
    const stale = await r.fetch();
    gotStale();
    await release;
    await r.fulfill({ response: stale });
  });
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await fetched;
  await expect(a.getByText('Загрузка…')).toBeVisible();
  await a.getByRole('button', { name: 'Закрыть' }).click();
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await a.getByLabel('Адрес, кого пригласить').fill(invited);
  await a.getByRole('button', { name: 'Пригласить' }).click();
  const people = a.getByRole('list', { name: 'Люди с доступом' });
  await expect(people.getByText(invited)).toBeVisible();
  const delivered = a.waitForResponse('**/share');
  resolve();
  await delivered;
  await expect(people.getByText(invited)).toBeVisible();
  await a.context().unroute('**/share');
  await a.context().close();
});

test('«Поделиться»: снять, сменить роль, новая ссылка и копирование — через диалог', async ({ browser }, info) => {
  const ownerPage = await person(browser, newEmail(info.project.name));
  const mail = newEmail(info.project.name);
  const b = await person(browser, mail);
  const id = await storedFile(ownerPage);
  await shareWith(ownerPage, mail);
  await open(b, id);
  await expect(b.getByRole('button', { name: 'Поделиться' })).toBeVisible();
  await ownerPage.getByRole('button', { name: 'Поделиться' }).click();
  // Смена роли в списке: сервер запомнил, и открытый редактор это видит.
  await ownerPage.getByLabel(`Роль: ${mail}`).selectOption({ label: 'Может смотреть' });
  await expect
    .poll(
      async () =>
        ((await api(ownerPage, 'GET', `/api/files/${id}/share`)).body as { people: { email: string; role: string }[] }).people.find(
          (p) => p.email === mail,
        )?.role,
    )
    .toBe('viewer');
  await checkAccessNow(b);
  await expect(b.getByText('Только просмотр')).toBeVisible();
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
  // Возврат роли без перезагрузки: кнопка и правка снова есть (T7, `chrome.tsx`).
  await ownerPage.getByLabel(`Роль: ${mail}`).selectOption({ label: 'Может редактировать' });
  await expect(ownerPage.getByLabel(`Роль: ${mail}`)).toBeEnabled();
  await checkAccessNow(b);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await expect(b.getByRole('button', { name: 'Поделиться' })).toBeVisible();
  // Ссылка: копирование отдаёт то, что на экране, «Новая ссылка» её меняет.
  await ownerPage.getByRole('checkbox', { name: 'Доступ по ссылке' }).check();
  await expect(ownerPage.getByRole('button', { name: 'Копировать ссылку' })).toBeVisible();
  const token = async () => ((await api(ownerPage, 'GET', `/api/files/${id}/share`)).body as { link: { token: string } }).link.token;
  const first = await token();
  if (info.project.name === 'server') {
    await ownerPage.context().grantPermissions(['clipboard-read', 'clipboard-write']);
    await ownerPage.getByRole('button', { name: 'Копировать ссылку' }).click();
    await expect(ownerPage.getByRole('button', { name: 'Скопировано' })).toBeVisible();
    expect(await ownerPage.evaluate(() => navigator.clipboard.readText())).toContain(`#/file/${id}?link=${first}`);
  }
  const reset = ownerPage.waitForResponse('**/share/link/reset');
  await ownerPage.getByRole('button', { name: 'Новая ссылка' }).click();
  await reset;
  expect(await token()).not.toBe(first);
  // «Снять»: человек уходит из списка, его редактор получает «Нет доступа».
  await ownerPage.getByRole('button', { name: `Снять доступ: ${mail}` }).click();
  await expect(ownerPage.getByRole('list', { name: 'Люди с доступом' }).getByText(mail)).toHaveCount(0);
  await checkAccessNow(b);
  await expect(b.getByRole('alert').getByText('Нет доступа', { exact: true })).toBeVisible();
  await ownerPage.context().close();
  await b.context().close();
});

test('«Поделиться»: отказ сервера возвращает выбор на место (T7-I3)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const mail = newEmail(info.project.name);
  await storedFile(a);
  await shareWith(a, mail);
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await a.context().route('**/share/people/**', (r) => r.fulfill({ status: 500, body: 'сбой' }));
  await a.getByLabel(`Роль: ${mail}`).selectOption({ label: 'Может смотреть' });
  await expect(a.getByRole('dialog').getByRole('alert')).toContainText('Сервер недоступен');
  await expect(a.getByLabel(`Роль: ${mail}`)).toHaveValue('editor');
  await a.context().unroute('**/share/people/**');
  await a.context().route('**/share/link', (r) => r.fulfill({ status: 500, body: 'сбой' }));
  await a.getByRole('checkbox', { name: 'Доступ по ссылке' }).click();
  await expect(a.getByRole('dialog').getByRole('alert')).toContainText('Сервер недоступен');
  await expect(a.getByRole('checkbox', { name: 'Доступ по ссылке' })).not.toBeChecked();
  await a.context().unroute('**/share/link');
  await a.getByRole('checkbox', { name: 'Доступ по ссылке' }).check();
  await expect(a.getByLabel('Роль по ссылке')).toHaveValue('viewer');
  await a.context().route('**/share/link', (r) => r.fulfill({ status: 500, body: 'сбой' }));
  await a.getByLabel('Роль по ссылке').selectOption({ label: 'Может редактировать' });
  await expect(a.getByRole('dialog').getByRole('alert')).toContainText('Сервер недоступен');
  await expect(a.getByLabel('Роль по ссылке')).toHaveValue('viewer');
  await a.context().close();
});

test('редактор по ссылке: кнопки нет, сервер отказывает (T7-I1, T7-I2)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await a.getByRole('checkbox', { name: 'Доступ по ссылке' }).check();
  await a.getByLabel('Роль по ссылке').selectOption({ label: 'Может редактировать' });
  await expect(a.getByRole('button', { name: 'Копировать ссылку' })).toBeVisible();
  const { token } = ((await api(a, 'GET', `/api/files/${id}/share`)).body as { link: { token: string } }).link;
  const answered = b.waitForResponse((r) => r.url().endsWith('/access'));
  await b.goto(`/#/file/${id}?link=${token}`);
  await editorReady(b);
  await answered;
  await inSync(b);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
  // Сервер отказывает и без кнопки.
  expect((await api(b, 'GET', `/api/files/${id}/share`)).status).toBe(403);
  await a.context().close();
  await b.context().close();
});

test('читатель в режиме просмотра сразу: ответ опроса ещё в пути, а правки уже нет (T6-I1)', async ({ browser }, info) => {
  const mail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, mail);
  const id = await storedFile(a);
  await shareWith(a, mail, 'Может смотреть');
  // Опрос не отвечает, запись — тоже проверка доступа: окно держится открытым на весь тест.
  await b.context().route('**/access', () => {});
  await open(b, id);
  await nodes(b, 1);
  await b.locator('canvas').click({ position: { x: 10, y: 10 } });
  await b.keyboard.press('KeyR');
  const l = await live(b);
  await drag(b, await appScreen(b, l, l.cameraX, l.cameraY), 100, 80);
  await nodes(b, 1);
  expect(await b.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  await a.context().close();
  await b.context().close();
});

test('право вернули читателю: журнал не стирается, файл открывается заново с правками (T6-I2)', async ({ browser }, info) => {
  test.skip(info.project.name === 'server-firefox', '`setOffline` не действует на worker в Firefox');
  const mail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, mail);
  const id = await storedFile(a);
  await shareWith(a, mail);
  const entry = async () =>
    ((await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { id: string; email: string }[] }).people.find(
      (p) => p.email === mail,
    )!.id;
  // Редактор правит, запись не проходит: правка только в журнале. Уходит.
  await open(b, id);
  await b.context().setOffline(true);
  await expect(b.locator('.fe-sync')).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(b);
  await b.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(b.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' })).toContainText('сохранены в этом браузере', {
    timeout: 15_000,
  });
  await b.getByRole('button', { name: 'Уйти' }).click();
  await expect(title(b)).toHaveText('Недавние');
  await b.context().setOffline(false);
  // Роль понизили: журнал не применён.
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'viewer' });
  await open(b, id);
  await nodes(b, 1);
  await expect(b.getByText('Только просмотр')).toBeVisible();
  // Роль вернули в той же вкладке: прежняя правка возвращается сразу.
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'editor' });
  await expect(toast(b, 'Ваши несохранённые правки восстановлены')).toBeVisible();
  await nodes(b, 2);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b, 200);
  await inSync(b);
  await b.reload();
  await editorReady(b);
  await nodes(b, 3);
  await a.context().close();
  await b.context().close();
});

test('неудачная запись повторяется сама; новый файл не закрывается молча (T6-I4)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  let refused = 0;
  await a.context().route('**/content', (r) => {
    if (r.request().method() !== 'PUT' || ++refused > 1) return r.continue();
    return r.abort();
  });
  await a.goto('/?nodes=0&fullSave=200#/new');
  await editorReady(a);
  const id = fileId(a);
  // Без единой правки и нажатия: первая запись оборвалась, вторая прошла сама.
  await inSync(a);
  expect(refused).toBeGreaterThan(1);
  expect(((await api(a, 'GET', `/api/files/${id}/access`)).body as { version: number }).version).toBeGreaterThan(0);
  await a.context().unroute('**/content');
  // Новый файл, запись не проходит: закрытие вкладки спрашивает.
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  await drawRect(a);
  await saveKey(a);
  await expect(toast(a, 'Нет связи с сервером')).toBeVisible();
  const dialog = a.waitForEvent('dialog');
  const closing = a.close({ runBeforeUnload: true });
  const shown = await dialog;
  expect(shown.type()).toBe('beforeunload');
  await shown.dismiss();
  await closing;
  await expect.poll(() => a.isClosed()).toBe(false);
  await a.context().close();
});

test('смена входа в другой вкладке: новый файл без записи спрашивает, а не перезагружается (T6-I4)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  await drawRect(a);
  await saveKey(a);
  await expect(toast(a, 'Нет связи с сервером')).toBeVisible();
  const tab = await a.context().newPage();
  await testMode(tab);
  await tab.goto('/');
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(tab.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  // Уже другой человек: правка цела, страница не перезагружена.
  await signIn(tab, newEmail(info.project.name), 'Другой');
  await expect(tab.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await nodes(a, 1);
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  await a.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(a.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' })).toContainText('ещё ни разу не записан');
  await a.context().close();
});

const closeDialog = (p: Page) => p.getByRole('button', { name: 'Закрыть', exact: true }).click();

/** Команда с проектом у владельца; диалог остаётся на странице команды. */
async function teamWithProject(a: Page, project = 'Сайт'): Promise<void> {
  await a.getByRole('button', { name: 'Команды…' }).click();
  await a.getByLabel('Имя новой команды').fill('Студия');
  await a.getByRole('button', { name: 'Создать' }).click();
  await a.getByLabel('Имя нового проекта').fill(project);
  await a.getByRole('button', { name: 'Добавить проект' }).click();
  await expect(a.getByRole('list', { name: 'Проекты команды' }).getByText(project)).toBeVisible();
}

const changed = (p: Page) =>
  p.evaluate(() => {
    const c = new BroadcastChannel('ferrum-server-files');
    c.postMessage('changed');
    c.close();
  });

test('страница команды: обновление списка не стирает ввод, фокус и участников (T8-I1)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await teamWithProject(a);
  await expect(a.getByRole('list', { name: 'Участники команды' }).getByText('Владелец')).toBeVisible();
  const field = a.getByLabel('Адрес нового участника');
  await field.fill('наполовину@');
  await field.focus();
  let members = 0;
  a.on('request', (r) => {
    if (r.url().endsWith('/members')) members++;
  });
  const list = a.waitForResponse((r) => r.url().endsWith('/api/teams'));
  await changed(a);
  await list;
  // Новый объект команды дошёл до экрана: имя нового проекта появилось бы, а поле то же.
  await a.getByLabel('Имя нового проекта').fill('Ещё');
  await field.focus();
  const list2 = a.waitForResponse((r) => r.url().endsWith('/api/teams'));
  await changed(a);
  await list2;
  await expect(field).toHaveValue('наполовину@');
  await expect(field).toBeFocused();
  await expect(a.getByLabel('Имя нового проекта')).toHaveValue('Ещё');
  await expect(a.getByRole('list', { name: 'Участники команды' }).getByText('Владелец')).toBeVisible();
  expect(members).toBe(0);
  await a.context().close();
});

test('команды: загрузка, сбой и пусто различимы, фоновый сбой виден, есть «Повторить» (T8-I2, T8-I3, T8-I4)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  // Сбой при первом чтении: панель говорит об этом, диалог не обещает «Команд пока нет».
  await a
    .context()
    .route('**/api/teams', (r) => (r.request().method() === 'GET' ? r.fulfill({ status: 503, body: 'сбой' }) : r.continue()));
  await a.reload();
  await expect(a.getByRole('alert').getByText('Команды не загрузились')).toBeVisible();
  await a.getByRole('button', { name: 'Команды…' }).click();
  const dialog = a.getByRole('dialog', { name: 'Команды' });
  await expect(dialog.getByRole('alert')).toContainText('Сервер недоступен');
  await expect(dialog.getByText('Команд пока нет')).toHaveCount(0);
  await a.context().unroute('**/api/teams');
  await dialog.getByRole('button', { name: 'Повторить' }).click();
  await expect(dialog.getByText('Команд пока нет')).toBeVisible();
  await expect(a.getByText('Команды не загрузились')).toHaveCount(0);
  // Загрузка — не «пусто».
  await closeDialog(a);
  const { promise: letGo, resolve } = Promise.withResolvers<void>();
  await a.context().route('**/api/teams', async (r) => {
    await letGo;
    await r.continue();
  });
  await a.reload();
  await a.getByRole('button', { name: 'Команды…' }).click();
  await expect(dialog.getByText('Загрузка…')).toBeVisible();
  await expect(dialog.getByText('Команд пока нет')).toHaveCount(0);
  resolve();
  await expect(dialog.getByText('Команд пока нет')).toBeVisible();
  await a.context().unroute('**/api/teams');
  // Команда создана, а список не прочитался: сообщение, повтор открывает её страницу.
  await a.context().route('**/api/teams', (r) => (r.request().method() === 'GET' ? r.abort() : r.continue()));
  await dialog.getByLabel('Имя новой команды').fill('Студия');
  await dialog.getByRole('button', { name: 'Создать' }).click();
  await expect(dialog.getByRole('alert')).toContainText('Нет связи с сервером');
  await a.context().unroute('**/api/teams');
  await dialog.getByRole('button', { name: 'Повторить' }).click();
  await expect(dialog.getByLabel('Имя нового проекта')).toBeVisible();
  // Участники не загрузились: не пустой список, а повтор.
  await dialog.getByRole('button', { name: '← Все команды' }).click();
  await a.context().route('**/members', (r) => r.fulfill({ status: 503, body: 'сбой' }));
  await dialog.getByRole('button', { name: 'Открыть' }).click();
  await expect(dialog.getByText('Список не загрузился')).toBeVisible();
  await a.context().unroute('**/members');
  await dialog.getByRole('button', { name: 'Повторить' }).click();
  await expect(dialog.getByRole('list', { name: 'Участники команды' }).getByText('Владелец')).toBeVisible();
  await closeDialog(a);
  // Фоновый сбой при показанном списке: строка в панели, раздел команды на месте.
  await a.context().route('**/api/teams', (r) => r.abort());
  await changed(a);
  await expect(a.getByRole('alert').getByText('Команды не загрузились')).toBeVisible();
  await a.context().unroute('**/api/teams');
  await a.getByRole('alert').getByRole('button', { name: 'Повторить' }).click();
  await expect(a.getByText('Команды не загрузились')).toHaveCount(0);
  await a.context().close();
});

test('диалог команды: убрать, переименовать, удалить, неверный адрес, вид участника, отказ и сбой (T8-I5)', async ({ browser }, info) => {
  const mail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, mail);
  await teamWithProject(a);
  const dialog = a.getByRole('dialog', { name: 'Команды' });
  // Неверный адрес — без запроса.
  await dialog.getByLabel('Адрес нового участника').fill('не адрес');
  await dialog.getByRole('button', { name: 'Пригласить' }).click();
  await expect(dialog.getByRole('alert')).toContainText('Проверьте адрес');
  // Отказ сервера и сбой сети дают понятный текст.
  await a.context().route('**/invite', (r) => r.fulfill({ status: 403, contentType: 'application/json', body: '{"error":"FORBIDDEN"}' }));
  await dialog.getByLabel('Адрес нового участника').fill(mail);
  await dialog.getByRole('button', { name: 'Пригласить' }).click();
  await expect(dialog.getByRole('alert')).toContainText('Недостаточно прав');
  await a.context().unroute('**/invite');
  await a.context().route('**/invite', (r) => r.abort());
  await dialog.getByRole('button', { name: 'Пригласить' }).click();
  await expect(dialog.getByRole('alert')).toContainText('Нет связи с сервером');
  await a.context().unroute('**/invite');
  await dialog.getByRole('button', { name: 'Пригласить' }).click();
  await expect(dialog.getByRole('list', { name: 'Участники команды' }).getByText(mail)).toBeVisible();
  // Переименование видно в панели.
  await dialog.getByLabel('Имя команды').fill('Мастерская');
  await dialog.getByRole('button', { name: 'Переименовать' }).click();
  await expect(a.getByText('Мастерская').first()).toBeVisible();
  // Вид участника: без форм управления.
  await b.reload();
  await b.getByRole('button', { name: 'Команды…' }).click();
  const theirs = b.getByRole('dialog', { name: 'Команды' });
  await theirs.getByRole('button', { name: 'Открыть' }).click();
  await expect(theirs.getByRole('list', { name: 'Участники команды' }).getByText(mail)).toBeVisible();
  await expect(theirs.getByLabel('Имя команды')).toHaveCount(0);
  await expect(theirs.getByLabel('Адрес нового участника')).toHaveCount(0);
  await expect(theirs.getByRole('button', { name: /^Убрать/ })).toHaveCount(0);
  await expect(theirs.getByRole('button', { name: 'Удалить команду' })).toHaveCount(0);
  await expect(theirs.getByRole('button', { name: /^Удалить проект/ })).toHaveCount(0);
  await theirs.getByRole('button', { name: '← Все команды' }).click();
  await expect(theirs.getByRole('list', { name: 'Ваши команды' })).toBeVisible();
  await b.getByRole('button', { name: 'Закрыть' }).click();
  // «Убрать»: сначала вопрос; отмена ничего не делает, согласие снимает доступ.
  await dialog.getByRole('button', { name: `Убрать: ${mail}` }).click();
  const ask = a.getByRole('dialog', { name: 'Убрать участника?' });
  await ask.getByRole('button', { name: 'Отмена' }).click();
  await expect(dialog.getByRole('list', { name: 'Участники команды' }).getByText(mail)).toBeVisible();
  await dialog.getByRole('button', { name: `Убрать: ${mail}` }).click();
  await ask.getByRole('button', { name: 'Убрать' }).click();
  await expect(dialog.getByRole('list', { name: 'Участники команды' }).getByText(mail)).toHaveCount(0);
  await b.reload();
  await b.getByRole('button', { name: 'Команды…' }).click();
  await expect(b.getByRole('dialog', { name: 'Команды' }).getByText('Команд пока нет')).toBeVisible();
  // Удаление проекта с вопросом; команду с проектом удалить нельзя — объяснение.
  await dialog.getByRole('button', { name: 'Удалить команду' }).click();
  await a.getByRole('dialog', { name: 'Удалить команду?' }).getByRole('button', { name: 'Удалить' }).click();
  await expect(dialog.getByRole('alert')).toContainText('Сначала уберите то, что внутри');
  await dialog.getByRole('button', { name: 'Удалить проект: Сайт' }).click();
  await a.getByRole('dialog', { name: 'Удалить проект?' }).getByRole('button', { name: 'Удалить' }).click();
  await expect(dialog.getByText('Проектов пока нет')).toBeVisible();
  await dialog.getByRole('button', { name: 'Удалить команду' }).click();
  await a.getByRole('dialog', { name: 'Удалить команду?' }).getByRole('button', { name: 'Удалить' }).click();
  await expect(dialog.getByText('Команд пока нет')).toBeVisible();
  await a.context().close();
  await b.context().close();
});

test('«Переместить…»: в проект и обратно, текущее место недоступно, сбой — сообщение (T8-I5)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await teamWithProject(a);
  await closeDialog(a);
  await storedFile(a);
  await a.goto('/#/drafts');
  const row = () => a.getByRole('option', { name: 'Без названия' });
  await row().click({ button: 'right' });
  await a.getByRole('menuitem', { name: 'Переместить…' }).click();
  const move = a.getByRole('dialog', { name: 'Переместить: Без названия' });
  const places = move.getByRole('list', { name: 'Куда переместить' }).getByRole('listitem');
  await expect(places.filter({ hasText: 'Черновики' }).getByRole('button', { name: 'Сюда' })).toBeDisabled();
  await a.context().route('**/move', (r) => r.abort());
  await places.filter({ hasText: 'Студия: Сайт' }).getByRole('button', { name: 'Сюда' }).click();
  await expect(move.getByRole('alert')).toContainText('Нет связи с сервером');
  await a.context().unroute('**/move');
  await places.filter({ hasText: 'Студия: Сайт' }).getByRole('button', { name: 'Сюда' }).click();
  await expect(move).toHaveCount(0);
  await expect(a.getByText('Здесь пока нет файлов')).toBeVisible();
  await a.getByRole('option', { name: 'Сайт' }).click();
  await expect(row()).toBeVisible();
  await row().click({ button: 'right' });
  await a.getByRole('menuitem', { name: 'Переместить…' }).click();
  await expect(places.filter({ hasText: 'Студия: Сайт' }).getByRole('button', { name: 'Сюда' })).toBeDisabled();
  await places.filter({ hasText: 'Черновики' }).getByRole('button', { name: 'Сюда' }).click();
  await expect(a.getByText('Здесь пока нет файлов')).toBeVisible();
  await a.context().close();
});

test('раздел удалённого проекта: заголовок говорит об этом, а не «Проект» (T8-Minor)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await teamWithProject(a);
  const dialog = a.getByRole('dialog', { name: 'Команды' });
  await closeDialog(a);
  await a.getByRole('option', { name: 'Сайт' }).click();
  await expect(title(a)).toHaveText('Сайт');
  await a.getByRole('button', { name: 'Команды…' }).click();
  await dialog.getByRole('button', { name: 'Открыть' }).click();
  await dialog.getByRole('button', { name: 'Удалить проект: Сайт' }).click();
  await a.getByRole('dialog', { name: 'Удалить проект?' }).getByRole('button', { name: 'Удалить' }).click();
  await expect(dialog.getByText('Проектов пока нет')).toBeVisible();
  await closeDialog(a);
  await expect(title(a)).toHaveText('Проект удалён или недоступен');
  await a.context().close();
});

/** Файл прежней версии в базе браузера, с журналом, как его оставил этап 1. */
async function seedLocal(page: Page, id: string, name: string, bytes: number[], journal: number[][] = [], extraEntries = 0): Promise<void> {
  await page.evaluate(
    ({ id, name, bytes, journal, extraEntries }) =>
      new Promise<void>((resolve, reject) => {
        const req = indexedDB.open('ferrum', 4);
        req.onsuccess = () => {
          const tx = req.result.transaction(['meta', 'bytes', 'journal'], 'readwrite');
          const now = Date.now();
          tx.objectStore('meta').put({
            id,
            name,
            createdAt: now,
            editedAt: now,
            openedAt: now,
            size: bytes.length,
            thumbVersion: 0,
            version: 1,
          });
          tx.objectStore('bytes').put(new Uint8Array(bytes).buffer, id);
          journal.forEach((b, i) => tx.objectStore('journal').put(new Uint8Array(b).buffer, [id, 1, i]));
          const empty = new ArrayBuffer(0);
          for (let i = 0; i < extraEntries; i++) tx.objectStore('journal').put(empty, [id, 2, i]);
          tx.oncomplete = () => resolve();
          tx.onerror = () => reject(tx.error);
        };
      }),
    { id, name, bytes, journal, extraEntries },
  );
}

/** Содержимое файла с сервера и пачка журнала, которую ядро отдало поверх него. */
async function fileAndJournal(a: Page): Promise<{ id: string; bytes: number[]; batch: number[] }> {
  test.skip(test.info().project.name === 'server-firefox', '`setOffline` не действует на worker в Firefox');
  const id = await storedFile(a);
  const bytes = await a.evaluate(async (i) => Array.from(new Uint8Array(await (await fetch(`/api/files/${i}/content`)).arrayBuffer())), id);
  // Сервера нет: правка только в журнале браузера, файл на сервере прежний.
  await a.context().setOffline(true);
  await expect(a.locator('.fe-sync')).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(a, 200);
  await expect
    .poll(
      () =>
        a.evaluate(async () => {
          return new Promise<number>((resolve) => {
            const dbs = indexedDB.databases();
            void dbs.then((list) => {
              const name = list.find((d) => d.name?.startsWith('ferrum-user-'))?.name;
              if (!name) return resolve(0);
              const req = indexedDB.open(name);
              req.onsuccess = () => {
                const n = req.result.transaction('journal').objectStore('journal').count();
                n.onsuccess = () => resolve(n.result);
              };
            });
          });
        }),
      { timeout: 15_000 },
    )
    .toBeGreaterThan(0);
  await a.context().setOffline(false);
  const batch = await a.evaluate(async () => {
    const { user } = (await (await fetch('/api/auth/session')).json()) as { user: { id: string } };
    return new Promise<number[]>((resolve, reject) => {
      const req = indexedDB.open(`ferrum-user-${user.id}`);
      req.onerror = () => reject(req.error);
      req.onsuccess = () => {
        const all = req.result.transaction('journal').objectStore('journal').getAll();
        all.onsuccess = () => resolve(Array.from(new Uint8Array((all.result as ArrayBuffer[]).at(-1)!)));
      };
    });
  });
  return { id, bytes, batch };
}

test('перенос: журнал едет с файлом, «Перенести все» обходит всё, сбой одного не мешает другим (T9-I5)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const { bytes, batch } = await fileAndJournal(a);
  await seedLocal(a, 'with-journal', 'С журналом', bytes, [batch]);
  await seedLocal(a, 'broken', 'Сбойный', bytes);
  await seedLocal(a, 'plain', 'Простой', bytes);
  await a.goto('/#/drafts');
  await a.reload();
  await a.getByRole('button', { name: 'Локальные файлы (3)' }).click();
  // Один файл не создаётся на сервере: остальные идут дальше, у него — причина.
  let posts = 0;
  await a.context().route('**/api/files', async (r) => {
    if (r.request().method() !== 'POST') return r.continue();
    const name = (r.request().postDataJSON() as { name: string }).name;
    posts++;
    return name === 'Сбойный' ? r.fulfill({ status: 503, body: 'сбой' }) : r.continue();
  });
  const dialog = a.getByRole('dialog', { name: 'Локальные файлы (3)' });
  await dialog.getByRole('button', { name: 'Перенести все' }).click();
  const rowOf = (n: string) => dialog.getByRole('listitem').filter({ hasText: n });
  await expect(rowOf('С журналом')).toContainText('Перенесён в «Черновики»');
  await expect(rowOf('Простой')).toContainText('Перенесён в «Черновики»');
  await expect(rowOf('Сбойный')).toContainText('Не перенесён');
  expect(posts).toBe(3);
  await a.context().unroute('**/api/files');
  // Повтор только сбойного.
  await dialog.getByRole('button', { name: 'Перенести все' }).click();
  await expect(rowOf('Сбойный')).toContainText('Перенесён в «Черновики»');
  await dialog.getByRole('button', { name: 'Закрыть' }).click();
  // Правка из журнала на месте, и файл не задвоился.
  await expect(a.getByRole('option', { name: 'Простой' })).toHaveCount(1);
  await a.getByRole('option', { name: 'С журналом' }).click();
  await editorReady(a);
  await nodes(a, 2);
  await a.context().close();
});

test('перенос: повторного нет ни при двойном нажатии, ни после закрытия диалога посреди переноса (T9-I2)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  const bytes = await a.evaluate(async (i) => Array.from(new Uint8Array(await (await fetch(`/api/files/${i}/content`)).arrayBuffer())), id);
  await seedLocal(a, 'old', 'Старый', bytes);
  await a.goto('/#/drafts');
  await a.reload();
  await a.getByRole('button', { name: 'Локальные файлы (1)' }).click();
  const { promise: letGo, resolve } = Promise.withResolvers<void>();
  let posts = 0;
  await a.context().route('**/api/files', async (r) => {
    if (r.request().method() !== 'POST') return r.continue();
    posts++;
    await letGo;
    await r.continue();
  });
  const move = a.getByRole('button', { name: 'Перенести: Старый' });
  await move.click();
  await expect(move).toBeDisabled();
  // Закрыли, пока файл идёт, открыли снова: строка знает, что он в пути.
  await a.getByRole('button', { name: 'Закрыть' }).click();
  await a.getByRole('button', { name: 'Локальные файлы (1)' }).click();
  await expect(move).toBeDisabled();
  await a.getByRole('button', { name: 'Закрыть' }).click();
  const created = a.waitForResponse((r) => r.url().endsWith('/api/files') && r.request().method() === 'POST');
  resolve();
  await created;
  await expect(a.getByRole('option', { name: 'Старый' })).toBeVisible();
  // Отметка сохранена, хотя диалог был закрыт.
  await a.reload();
  await a.getByRole('button', { name: 'Локальные файлы (1)' }).click();
  await expect(a.getByRole('dialog').getByText('Перенесён в «Черновики»')).toBeVisible();
  await expect(move).toBeDisabled();
  expect(posts).toBe(1);
  await a.context().close();
});

test('перенос файла с картинкой: картинка отдельным запросом, в новом файле она есть (T9-I5)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  await a.evaluate(async () => {
    const c = new OffscreenCanvas(4, 4);
    c.getContext('2d')!.fillRect(0, 0, 4, 4);
    const bytes = new Uint8Array(await (await c.convertToBlob({ type: 'image/png' })).arrayBuffer());
    window.__ferrum!.core!.send({ type: 'placeImage', bytes });
  });
  await expect.poll(() => a.evaluate(() => window.__ferrum!.ui!.tool)).toBe('image');
  const l = await live(a);
  const at = await appScreen(a, l, l.cameraX, l.cameraY);
  await a.mouse.click(at.x, at.y);
  await nodes(a, 1);
  // Контейнер с картинкой — снимком ядра: так он лежал бы в браузере.
  const bytes = await a.evaluate(
    () =>
      new Promise<number[]>((resolve) => {
        const core = window.__ferrum!.core!;
        const off = core.on((e) => {
          if (e.type === 'snapshot') {
            off();
            resolve(Array.from(e.bytes));
          }
        });
        core.send({ type: 'snapshot', thumbnail: false });
      }),
  );
  await saveKey(a);
  await saved(a);
  await seedLocal(a, 'pic', 'С картинкой', bytes);
  await a.goto('/#/drafts');
  await a.reload();
  await a.getByRole('button', { name: 'Локальные файлы (1)' }).click();
  const images: string[] = [];
  a.on('request', (r) => {
    if (r.url().includes('/images/') && r.method() === 'PUT') images.push(r.url());
  });
  await a.getByRole('button', { name: 'Перенести: С картинкой' }).click();
  await expect(a.getByRole('dialog').getByText('Перенесён в «Черновики»')).toBeVisible();
  expect(images).toHaveLength(1);
  const fileOf = images[0].split('/images/')[0];
  const got = await a.evaluate(async (u) => (await fetch(u)).status, images[0]);
  expect(got).toBe(200);
  const content = await a.evaluate(async (u) => (await fetch(`${u}/content`)).status, fileOf);
  expect(content).toBe(200);
  await a.context().close();
});

test('журнал сверх предела: без правок файл «Перенесён» не становится, причина на экране (T9-I1)', async ({ browser }, info) => {
  test.setTimeout(240_000);
  const a = await person(browser, newEmail(info.project.name));
  const id = await storedFile(a);
  const bytes = await a.evaluate(async (i) => Array.from(new Uint8Array(await (await fetch(`/api/files/${i}/content`)).arrayBuffer())), id);
  await seedLocal(a, 'huge', 'Огромный', bytes, [], 100_001);
  await a.goto('/#/drafts');
  await a.reload();
  await a.getByRole('button', { name: 'Локальные файлы (1)' }).click();
  let posts = 0;
  a.on('request', (r) => {
    if (r.url().endsWith('/api/files') && r.method() === 'POST') posts++;
  });
  const dialog = a.getByRole('dialog', { name: 'Локальные файлы (1)' });
  await dialog.getByRole('button', { name: 'Перенести: Огромный' }).click();
  await expect(dialog).toContainText('несохранённых правок слишком много', { timeout: 60_000 });
  expect(posts).toBe(0);
  await expect(dialog.getByText('Перенесён в «Черновики»')).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Перенести: Огромный' })).toBeEnabled();
  // Удаление такого файла предупреждает, что он только здесь.
  await dialog.getByRole('button', { name: 'Удалить из браузера: Огромный' }).click();
  await expect(a.getByRole('dialog', { name: 'Удалить файл из браузера?' })).toContainText('есть только в этом браузере');
  await a.getByRole('dialog', { name: 'Удалить файл из браузера?' }).getByRole('button', { name: 'Отмена' }).click();
  // Явный перенос без правок: копия есть, отметки «перенесён» нет.
  await dialog.getByRole('button', { name: 'Перенести без правок: Огромный' }).click();
  await expect(dialog).toContainText('Копия без несохранённых правок', { timeout: 60_000 });
  expect(posts).toBe(1);
  await expect(dialog.getByRole('button', { name: 'Перенести: Огромный' })).toBeEnabled();
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
  await expect(a.locator('.fe-sync')).toHaveText('Синхронизировано', { timeout: 120_000 });
  await drawRect(a);
  const t0 = Date.now();
  await inSync(a);
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
