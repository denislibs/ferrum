// Комментарии на настоящем стенде (спека подсистемы 4, раздел 10): два браузера — метка и обсуждение видны другому сразу, ответ и
// закрытие тоже; упоминание приходит письмом (Mailpit), ссылка из письма открывает обсуждение; читатель комментирует, но чужое
// не удаляет; человек без доступа по ссылке ничего не получает. Окно группировки писем на стенде — 2 с (`COMMENT_MAIL_DELAY_MS`).

import { expect, test, type Browser, type Page } from '@playwright/test';
import { appScreen, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { mailsMatching, newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);

const COMMENT_MAIL = /комментари/;

const person = async (browser: Browser, email: string, name: string): Promise<Page> => {
  const context = await browser.newContext({ baseURL: STAND_URL, viewport: { width: 1280, height: 720 } });
  const page = await context.newPage();
  await testMode(page);
  await page.goto('/');
  await signIn(page, email, name);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  return page;
};

const synced = (page: Page) => expect(page.locator('.fe-sync')).toHaveText('Синхронизировано', { timeout: 15_000 });

const api = (page: Page, method: string, path: string, body?: unknown) =>
  page.evaluate(
    async ({ method, path, body }) => {
      const init: RequestInit = { method };
      if (body !== undefined) {
        init.headers = { 'content-type': 'application/json' };
        init.body = JSON.stringify(body);
      }
      const res = await fetch(path, init);
      return { status: res.status, body: (await res.json().catch(() => null)) as never };
    },
    { method, path, body },
  );

async function newFile(page: Page): Promise<string> {
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  await synced(page);
  return fileId(page);
}

const invite = async (owner: Page, id: string, email: string, role: 'editor' | 'viewer') =>
  expect((await api(owner, 'POST', `/api/files/${id}/share/invite`, { email, role })).status).toBe(200);

async function openFile(page: Page, id: string, query = ''): Promise<void> {
  await page.goto(`/#/file/${id}${query}`);
  await page.reload();
  await editorReady(page);
  await synced(page);
}

/** Режим комментариев: инструмент доступен, когда роль пришла (ядро получило `commentsAvailable`). */
async function commentMode(page: Page): Promise<void> {
  await expect(page.getByRole('button', { name: 'Комментарий', exact: true })).toBeVisible();
  await page.locator('canvas').focus();
  await page.keyboard.press('KeyC');
  await expect(page.getByRole('region', { name: 'Комментарии' })).toBeVisible();
}

/** Метка в точке мира (x, y): её капля — квадрат над остриём, щелчок — в его середину. */
async function pinAt(page: Page, x: number, y: number): Promise<{ x: number; y: number }> {
  const p = await appScreen(page, await live(page), x, y);
  return { x: p.x + 14, y: p.y - 14 };
}

test('два браузера: комментарий с упоминанием, метка и обсуждение у другого сразу, ответ и закрытие', async ({ browser }) => {
  const [a, b] = [await person(browser, newEmail('ca'), 'Анна Смирнова'), await person(browser, newEmail('cb'), 'Борис')];
  const id = await newFile(a);
  const emailB = await b.evaluate(
    async () => ((await (await fetch('/api/auth/session')).json()) as { user: { email: string } }).user.email,
  );
  await invite(a, id, emailB, 'editor');
  await openFile(b, id);
  // Борис открыл файл: имя знают только у тех, кто открывал — подсказка упоминаний его находит.
  await commentMode(a);
  const at = await appScreen(a, await live(a), 40, 30);
  await a.mouse.click(at.x, at.y);
  const draft = a.getByRole('dialog', { name: 'Новый комментарий' });
  await expect(draft).toBeVisible();
  const field = draft.getByLabel('Комментарий');
  await field.pressSequentially('@Бор');
  await expect(a.getByRole('option', { name: /Борис/ })).toBeVisible();
  await field.press('Enter');
  await expect(field).toHaveValue('@Борис ');
  await field.pressSequentially('посмотри <b>это</b>');
  await field.press('Enter');
  // Окно нового обсуждения: текст как текст, упоминание — именем.
  const threadA = a.getByRole('dialog', { name: 'Обсуждение' });
  await expect(threadA.locator('.fe-comment__body')).toHaveText('@Борис посмотри <b>это</b>');
  await expect(threadA.locator('.fe-comment__body b')).toHaveCount(0);

  // У Бориса метка уже есть: щелчок по ней (вне режима комментариев) открывает обсуждение.
  const pin = await pinAt(b, 40, 30);
  await b.mouse.click(pin.x, pin.y);
  const threadB = b.getByRole('dialog', { name: 'Обсуждение' });
  await expect(threadB.locator('.fe-comment__body').first()).toHaveText('@Борис посмотри <b>это</b>');
  // Ответ — у Анны в открытом окне сразу.
  const reply = threadB.getByLabel('Ответ');
  await reply.fill('Готово');
  await reply.press('Enter');
  await expect(threadA.locator('.fe-comment__body')).toHaveText(['@Борис посмотри <b>это</b>', 'Готово']);
  // Закрытие обсуждения — у Анны сразу; в панели закрытые скрыты, «Показать закрытые» возвращает.
  await threadB.getByRole('button', { name: 'Закрыть обсуждение' }).click();
  await expect(threadA.getByRole('button', { name: 'Открыть снова' })).toBeVisible();
  await expect(threadA.getByText('Обсуждение закрыто: Борис')).toBeVisible();
  await a.keyboard.press('Escape');
  await expect(threadA).toHaveCount(0);
  const panel = a.getByRole('region', { name: 'Комментарии' });
  await expect(panel.getByText('Обсуждений нет')).toBeVisible();
  await panel.getByLabel('Показать закрытые').check();
  await expect(panel.locator('.fe-comments__row')).toHaveCount(1);
});

test('упоминание — письмо, ссылка из письма открывает обсуждение; читатель отвечает, но не удаляет чужое; без доступа — ничего', async ({
  browser,
}) => {
  const a = await person(browser, newEmail('ma'), 'Анна Смирнова');
  const emailV = newEmail('mv');
  const v = await person(browser, emailV, 'Вера Читатель');
  const id = await newFile(a);
  await invite(a, id, emailV, 'viewer');
  // Вера открыла файл раньше (её имя в подсказке) и ушла: упоминание — письмом.
  await openFile(v, id);
  await v.goto('/');
  await expect(v.getByRole('heading', { level: 1 })).toHaveText('Недавние');

  await commentMode(a);
  const at = await appScreen(a, await live(a), 80, 60);
  await a.mouse.click(at.x, at.y);
  const field = a.getByRole('dialog', { name: 'Новый комментарий' }).getByLabel('Комментарий');
  await field.pressSequentially('@Вер');
  await expect(a.getByRole('option', { name: /Вера Читатель/ })).toBeVisible();
  await field.press('Enter');
  await field.pressSequentially('глянь');
  await field.press('Enter');
  const threadA = a.getByRole('dialog', { name: 'Обсуждение' });
  await expect(threadA.locator('.fe-comment__body')).toHaveText('@Вера Читатель глянь');
  const thread = (await api(a, 'GET', `/api/files/${id}/comments?after=0&limit=10`)).body as { threads: { id: string }[] };
  const threadId = thread.threads[0].id;

  const [mail] = await mailsMatching(emailV, COMMENT_MAIL);
  expect(mail.subject).toContain('Анна Смирнова');
  expect(mail.text).toContain('@Вера Читатель глянь');
  const link = /http\S+#\/file\/\S+\?comment=\S+/.exec(mail.text)![0];
  expect(link).toBe(`${STAND_URL}/#/file/${id}?comment=${threadId}`);

  // Ссылка из письма: файл открывается на обсуждении (вкладка открывается по ссылке заново, как из почты).
  const fresh = await v.context().newPage();
  await testMode(fresh);
  await fresh.goto(link);
  await editorReady(fresh);
  await v.close();
  const vv = fresh;
  const threadV = vv.getByRole('dialog', { name: 'Обсуждение' });
  await expect(threadV.locator('.fe-comment__body')).toHaveText('@Вера Читатель глянь');
  // Читатель: отвечает, своё может удалить, чужое — нет.
  await expect(threadV.getByRole('button', { name: 'Удалить' })).toHaveCount(0);
  const reply = threadV.getByLabel('Ответ');
  await reply.fill('Вижу');
  await reply.press('Enter');
  await expect(threadV.locator('.fe-comment__body')).toHaveText(['@Вера Читатель глянь', 'Вижу']);
  await expect(threadV.getByRole('button', { name: 'Удалить' })).toHaveCount(1);
  await expect(threadA.locator('.fe-comment__body')).toHaveText(['@Вера Читатель глянь', 'Вижу']);
  // Сервер тоже не даёт: удалить чужой комментарий — 403.
  const first = ((await api(vv, 'GET', `/api/files/${id}/comments/${threadId}`)).body as { comments: { id: string }[] }).comments[0].id;
  expect((await api(vv, 'DELETE', `/api/files/${id}/comments/${threadId}/comments/${first}`)).status).toBe(403);

  // Без доступа: ссылка ведёт на «Нет доступа», а запрос обсуждения неотличим от несуществующего.
  const g = await person(browser, newEmail('mg'), 'Галина');
  const gg = await g.context().newPage();
  await gg.goto(link);
  await g.close();
  await expect(gg.getByRole('heading', { name: 'Нет доступа' })).toBeVisible();
  await expect(gg.getByText('глянь')).toHaveCount(0);
  expect((await api(gg, 'GET', `/api/files/${id}/comments/${threadId}`)).status).toBe(404);
  expect((await api(gg, 'GET', `/api/files/${id}/comments?after=0&limit=10`)).status).toBe(404);
});
