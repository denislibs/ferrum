// Файлы на сервере: настоящий стенд, настоящие письма, два браузера там, где нужны двое.

import { expect, test, type Browser, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { appScreen, drag, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { compose, linksFor, mailCount, mailsFor, newEmail, resetStand, signIn } from './stand';

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

const open = async (page: Page, id: string) => {
  await page.goto(`/#/file/${id}`);
  await editorReady(page);
};

/** Вернулись на вкладку: доступ проверяется сразу, а не к следующему опросу. */
const checkAccessNow = (page: Page) => page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));

async function shareWith(page: Page, email: string, role: 'Может редактировать' | 'Может смотреть' = 'Может редактировать') {
  await page.getByRole('button', { name: 'Поделиться' }).click();
  await page.getByLabel('Адрес, кого пригласить').fill(email);
  await page.getByLabel('Роль приглашённого').selectOption({ label: role });
  await page.getByRole('button', { name: 'Пригласить' }).click();
  await expect(page.getByRole('list', { name: 'Люди с доступом' }).getByText(email)).toBeVisible();
  await page.getByRole('button', { name: 'Закрыть' }).click();
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
  await checkAccessNow(b);
  await expect(b.getByText('Только просмотр')).toBeVisible();
  await expect(b.getByRole('button', { name: 'Поделиться' })).toHaveCount(0);
  await b.locator('canvas').click({ position: { x: 10, y: 10 } });
  await b.keyboard.press('KeyR');
  const l = await live(b);
  await drag(b, await appScreen(b, l, l.cameraX, l.cameraY), 100, 80);
  await nodes(b, before);
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'editor' });
  await checkAccessNow(b);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b);
  await nodes(b, before + 1);
  await saveKey(b);
  await saved(b);
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
  const share = (await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { id: string; email: string }[] };
  const entry = share.people.find((p) => p.email === editorMail)!.id;
  await api(a, 'DELETE', `/api/files/${id}/share/people/${entry}`);
  // Отказ на сохранении — сразу, опрос тут не нужен.
  await saveKey(b);
  await expect(b.getByRole('alert').getByText('Нет доступа', { exact: true })).toBeVisible();
  await expect(b.getByRole('button', { name: 'Сохранить копию на диск' })).toBeVisible();
  // Правки не потерялись молча: уход спрашивает.
  await b.getByRole('button', { name: 'К файлам', exact: true }).click();
  await expect(b.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' })).toBeVisible();
  await b.getByRole('button', { name: 'Остаться' }).click();
  await nodes(b, 2);
  const download = b.waitForEvent('download');
  await b.getByRole('button', { name: 'Сохранить копию на диск' }).click();
  expect((await download).suggestedFilename()).toMatch(/\.ferrum$/);
  await b.getByRole('button', { name: 'К файлам', exact: true }).click();
  await b.getByRole('button', { name: 'Уйти' }).click();
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

test('CONFLICT: два окна с одного поколения; после открытия заново правки сливаются', async ({ browser }, info) => {
  const editorMail = newEmail(info.project.name);
  const a = await person(browser, newEmail(info.project.name));
  const b = await person(browser, editorMail);
  const id = await storedFile(a);
  await shareWith(a, editorMail);
  await open(b, id);
  // Оба правят от одного поколения: A записывает первым.
  await drawRect(a, 200);
  await saveKey(a);
  await saved(a);
  // Открытый редактор узнаёт о новой версии, не дожидаясь своей записи.
  await checkAccessNow(b);
  await expect(toast(b, 'Откройте его заново, чтобы увидеть новую версию')).toBeVisible();
  await drawRect(b, -200);
  await nodes(b, 2);
  // Журнал успевает получить правку B: её и сольёт следующее открытие.
  await b.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await saveKey(b);
  await expect(toast(b, 'Файл изменён в другом окне или другим человеком')).toBeVisible();
  expect(await b.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  // Поверх чужой записи ничего не легло: на сервере версия A.
  const version = (await api(b, 'GET', `/api/files/${id}/access`)).body as { version: number };
  expect(version.version).toBe(2);
  // Уход — по протоколу: вопрос с причиной, правки остаются в журнале браузера.
  await b.getByRole('button', { name: 'Назад к файлам' }).click();
  const ask = b.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  await expect(ask).toContainText('Файл изменён');
  await b.getByRole('button', { name: 'Уйти' }).click();
  await expect(title(b)).toHaveText('Недавние');
  await open(b, id);
  await nodes(b, 3);
  await expect(b.getByText('Восстановлены изменения, не записанные в файл')).toBeVisible();
  await saveKey(b);
  await saved(b);
  await a.goto(`/#/file/${id}`);
  await editorReady(a);
  await nodes(a, 3);
  await a.context().close();
  await b.context().close();
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
  await a.getByRole('button', { name: 'Удалить из браузера: Старый' }).click();
  await expect(a.getByRole('dialog').getByText('Старый')).toHaveCount(0);
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
  // Закрыли, пока ответ в пути, открыли заново: запоздавший ответ не затирает новый список.
  const { promise: release, resolve } = Promise.withResolvers<void>();
  let calls = 0;
  await a.context().route('**/share', async (r) => {
    if (++calls === 1) await release;
    await r.continue();
  });
  const held = a.waitForRequest('**/share');
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await held;
  await expect(a.getByText('Загрузка…')).toBeVisible();
  await a.getByRole('button', { name: 'Закрыть' }).click();
  await a.getByRole('button', { name: 'Поделиться' }).click();
  await a.getByLabel('Адрес, кого пригласить').fill(invited);
  await a.getByRole('button', { name: 'Пригласить' }).click();
  await expect(a.getByRole('list', { name: 'Люди с доступом' }).getByText(invited)).toBeVisible();
  const stale = a.waitForResponse('**/share');
  resolve();
  await stale;
  await expect(a.getByRole('list', { name: 'Люди с доступом' }).getByText(invited)).toBeVisible();
  await a.context().close();
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
  await b.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
  await drawRect(b);
  await saveKey(b);
  await expect(toast(b, 'Нет связи с сервером')).toBeVisible();
  await b.getByRole('button', { name: 'Назад к файлам' }).click();
  await b.getByRole('button', { name: 'Уйти' }).click();
  await expect(title(b)).toHaveText('Недавние');
  await b.context().unroute('**/content');
  // Роль понизили: журнал не применён.
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'viewer' });
  await open(b, id);
  await nodes(b, 1);
  await expect(b.getByText('Только просмотр')).toBeVisible();
  // Роль вернули в той же вкладке: прежняя правка возвращается сразу.
  await api(a, 'PATCH', `/api/files/${id}/share/people/${await entry()}`, { role: 'editor' });
  await checkAccessNow(b);
  await expect(toast(b, 'Ваши несохранённые правки восстановлены')).toBeVisible();
  await nodes(b, 2);
  await expect(b.getByText('Только просмотр')).toHaveCount(0);
  await drawRect(b, 200);
  await saveKey(b);
  await saved(b);
  await b.reload();
  await editorReady(b);
  await nodes(b, 3);
  await a.context().close();
  await b.context().close();
});

test('своя запись: ответ ещё в пути, опрос видит новое поколение, «изменён другим» не появляется (T6-Minor5)', async ({
  browser,
}, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await storedFile(a);
  await drawRect(a);
  const { promise: letGo, resolve } = Promise.withResolvers<void>();
  const { promise: committed, resolve: done } = Promise.withResolvers<void>();
  await a.context().route('**/content', async (r) => {
    if (r.request().method() !== 'PUT') return r.continue();
    const res = await r.fetch();
    done();
    await letGo;
    await r.fulfill({ response: res });
  });
  await saveKey(a);
  await committed;
  const polled = a.waitForResponse((r) => r.url().endsWith('/access'));
  await checkAccessNow(a);
  await polled;
  resolve();
  await saved(a);
  // Следующий опрос уже после ответа: всё тихо, чужой записи не было.
  const again = a.waitForResponse((r) => r.url().endsWith('/access'));
  await checkAccessNow(a);
  await again;
  // Без повторов: уведомление исчезает само, и ожидание «его нет» дождалось бы этого.
  expect(await a.locator('body').innerText()).not.toContain('Файл изменён в другом окне');
  await a.context().close();
});

test('неудачная запись повторяется сама; новый файл не закрывается молча (T6-I4)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=0&fullSave=200#/new');
  await editorReady(a);
  const id = fileId(a);
  let refused = 0;
  await a.context().route('**/content', (r) => {
    if (r.request().method() !== 'PUT' || ++refused > 1) return r.continue();
    return r.abort();
  });
  await drawRect(a);
  // Без единой правки и нажатия: первая запись оборвалась, вторая прошла сама.
  await saved(a);
  expect(refused).toBeGreaterThan(1);
  expect(((await api(a, 'GET', `/api/files/${id}/access`)).body as { version: number }).version).toBeGreaterThan(0);
  await a.context().unroute('**/content');
  // Новый файл, запись не проходит: закрытие вкладки спрашивает.
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
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
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  await a.context().route('**/content', (r) => (r.request().method() === 'PUT' ? r.abort() : r.continue()));
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

test('картинку убрали с сервера, вкладка считает её известной: запись догружает её (T6-I3)', async ({ browser }, info) => {
  const a = await person(browser, newEmail(info.project.name));
  await a.goto('/?nodes=0#/new');
  await editorReady(a);
  const id = fileId(a);
  const uploads: string[] = [];
  a.on('request', (r) => {
    if (r.method() === 'PUT' && r.url().includes('/images/')) uploads.push(r.url());
  });
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
  await saveKey(a);
  await saved(a);
  expect(uploads).toHaveLength(1);
  const hash = uploads[0].split('/images/')[1];
  // Как очистка через 30 дней: строки картинки файла больше нет.
  compose('exec', '-T', 'postgres', 'psql', '-U', 'ferrum', '-d', 'ferrum', '-c', `DELETE FROM file_image WHERE file_id = '${id}'`);
  await drawRect(a, 300);
  await saveKey(a);
  await saved(a);
  expect(uploads).toHaveLength(2);
  const image = await a.evaluate(async (u) => (await fetch(u)).status, uploads[1]);
  expect(image).toBe(200);
  expect(hash).toHaveLength(64);
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
