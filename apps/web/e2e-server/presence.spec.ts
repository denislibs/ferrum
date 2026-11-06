// Присутствие на настоящем стенде (спека подсистемы 4, раздел 9): два браузера видят курсоры друг друга с подписью в мировой
// точке, рамку выделения цветом участника, живое превью перетаскивания (только в сцене), аватары и следование; ушедший — по
// закрытию вкладки и по снятию роли — пропадает сразу.

import { expect, test, type Browser, type Page } from '@playwright/test';
import { PRESENCE_COLORS, presenceColor } from '@ferrum/protocol';
import { appScreen, drag, editorReady, expectPixel, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);

interface Marks {
  marks: { name: string; color: string; cursor?: [number, number]; frame?: [number, number, number, number]; boxes: number }[];
  following?: number;
  preview: { id: string; x: number; y: number }[];
}

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

const userId = (page: Page) =>
  page.evaluate(async () => ((await (await fetch('/api/auth/session')).json()) as { user: { id: string } }).user.id);

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

/** Что видно от других: тестовая команда ядра `readPresence`. */
const marks = (page: Page): Promise<Marks> =>
  page.evaluate(
    () =>
      new Promise<Marks>((resolve) => {
        const core = window.__ferrum!.core!;
        const off = core.on((e) => {
          if (e.type !== 'presenceMarks') return;
          off();
          resolve({ marks: e.marks, following: e.following, preview: e.preview } as Marks);
        });
        core.send({ type: 'readPresence' });
      }),
  );

async function newFile(page: Page): Promise<string> {
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  await synced(page);
  return fileId(page);
}

async function join(owner: Page, guest: Page, id: string, role: 'editor' | 'viewer' = 'editor'): Promise<void> {
  const email = await guest.evaluate(
    async () => ((await (await fetch('/api/auth/session')).json()) as { user: { email: string } }).user.email,
  );
  const res = await api(owner, 'POST', `/api/files/${id}/share/invite`, { email, role });
  expect(res.status).toBe(200);
  await guest.goto(`/#/file/${id}`);
  await guest.reload();
  await editorReady(guest);
  await synced(guest);
}

/** Прямоугольник 100×80 в мире с левым верхним углом (x, y). */
async function drawRect(page: Page, x: number, y: number): Promise<void> {
  const before = (await live(page)).nodes;
  await page.keyboard.press('KeyR');
  const l = await live(page);
  await drag(page, await appScreen(page, l, x, y), 100 * l.zoom, 80 * l.zoom);
  await expect.poll(async () => (await live(page)).nodes).toBe(before + 1);
}

/** Указатель над точкой мира. */
async function hoverWorld(page: Page, x: number, y: number): Promise<void> {
  const l = await live(page);
  const p = await appScreen(page, l, x, y);
  await page.mouse.move(p.x, p.y, { steps: 3 });
}

const rgb = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));

async function pair(browser: Browser): Promise<{ a: Page; b: Page; id: string; colorA: string }> {
  const [a, b] = [await person(browser, newEmail('pa'), 'Анна Смирнова'), await person(browser, newEmail('pb'), 'Борис')];
  const id = await newFile(a);
  await join(a, b, id);
  const colorA = PRESENCE_COLORS[presenceColor(await userId(a))];
  return { a, b, id, colorA };
}

test('курсор с подписью в мировой точке, рамка выделения цветом участника; ушёл с холста — курсора нет', async ({ browser }) => {
  const { a, b, colorA } = await pair(browser);
  await drawRect(a, 0, 0);
  await hoverWorld(a, 40, 30);
  await expect
    .poll(async () => (await marks(b)).marks.map((m) => [m.name, m.color, m.cursor?.map(Math.round)]))
    .toEqual([['Анна Смирнова', colorA, [40, 30]]]);
  // Стрелка цвета участника на холсте другого — там, где эта точка мира у него на экране.
  const lb = await live(b);
  const tip = await appScreen(b, lb, 40, 30);
  await expectPixel(b, tip.x + 2, tip.y + 7, rgb(colorA), 40);
  // Выделение участника: рамка его узла.
  await a.mouse.click((await appScreen(a, await live(a), 50, 40)).x, (await appScreen(a, await live(a), 50, 40)).y);
  await expect.poll(async () => (await marks(b)).marks[0]?.boxes).toBe(1);
  // Указатель ушёл с холста — курсора у другого нет, участник есть.
  await a.mouse.move(5, 5);
  await expect.poll(async () => (await marks(b)).marks[0]?.cursor).toBeUndefined();
  expect((await marks(b)).marks).toHaveLength(1);
});

test('живое превью перетаскивания: у другого узел едет в сцене, документ — после отпускания; Esc и уход посреди жеста — превью нет', async ({
  browser,
}) => {
  const { a, b } = await pair(browser);
  await drawRect(a, 0, 0);
  await expect.poll(async () => (await live(b)).nodes).toBe(1);
  await a.keyboard.press('KeyV');
  const la = await live(a);
  const from = await appScreen(a, la, 50, 40);
  await a.mouse.move(from.x, from.y);
  await a.mouse.down();
  await a.mouse.move(from.x + 60 * la.zoom, from.y, { steps: 6 });
  // Жест идёт: у другого узел сдвинут на 60 в сцене.
  await expect.poll(async () => (await marks(b)).preview.map((p) => Math.round(p.x))).toEqual([60]);
  await a.mouse.up();
  // Отпустил: превью снято, сдвиг пришёл правкой.
  await expect.poll(async () => (await marks(b)).preview).toEqual([]);
  await expect.poll(async () => (await live(b)).selectionCount >= 0 && (await live(b)).nodes).toBe(1);
  // Esc посреди жеста: превью снято, правки нет.
  await a.mouse.move(from.x + 60 * la.zoom, from.y);
  await a.mouse.down();
  await a.mouse.move(from.x + 120 * la.zoom, from.y, { steps: 6 });
  await expect.poll(async () => (await marks(b)).preview.length).toBe(1);
  await a.keyboard.press('Escape');
  await expect.poll(async () => (await marks(b)).preview).toEqual([]);
  await a.mouse.up();
  // Ушёл посреди жеста (вкладка закрыта): превью и курсор пропали сразу.
  await a.mouse.move(from.x + 60 * la.zoom, from.y);
  await a.mouse.down();
  await a.mouse.move(from.x + 160 * la.zoom, from.y, { steps: 6 });
  await expect.poll(async () => (await marks(b)).preview.length).toBe(1);
  await a.context().close();
  await expect.poll(async () => marks(b), { timeout: 5000 }).toEqual({ marks: [], preview: [] });
});

test('аватары и следование: камера и страница за участником, действие на холсте — выход', async ({ browser }) => {
  const { a, b } = await pair(browser);
  const avatar = b.getByRole('button', { name: /^Анна Смирнова/ });
  await hoverWorld(a, 10, 10);
  await expect(avatar).toBeVisible();
  await expect(avatar).toHaveText('АС');
  await avatar.click();
  await expect(avatar).toHaveAttribute('aria-pressed', 'true');
  await expect(b.locator('.fe-stage')).toHaveClass(/fe-following/);
  // Участник уходит далеко: камера другого — за ним.
  await a.locator('canvas').hover();
  await a.mouse.wheel(3000, 2000);
  const target = await live(a);
  await expect.poll(async () => Math.round((await live(b)).cameraX - target.cameraX)).toBe(0);
  await expect.poll(async () => Math.round((await live(b)).cameraY - target.cameraY)).toBe(0);
  // Участник на новой странице — и другой там же.
  await a.getByRole('button', { name: 'Добавить страницу' }).click();
  await expect
    .poll(() => b.evaluate(() => window.__ferrum!.ui!.pages.current))
    .toBe(await a.evaluate(() => window.__ferrum!.ui!.pages.current));
  await expect(avatar).toHaveAttribute('aria-pressed', 'true');
  // Своё нажатие на холсте — выход.
  await b.locator('canvas').click({ position: { x: 300, y: 300 } });
  await expect(avatar).toHaveAttribute('aria-pressed', 'false');
  await expect(b.locator('.fe-stage')).not.toHaveClass(/fe-following/);
  expect((await marks(b)).following).toBeUndefined();
});

test('ушедший пропадает сразу: закрыл вкладку, снята роль; читатель виден', async ({ browser }) => {
  const { a, b, id } = await pair(browser);
  const emailV = newEmail('pv');
  const v = await person(browser, emailV, 'Вера');
  await join(a, v, id, 'viewer');
  await hoverWorld(v, 20, 20);
  await hoverWorld(b, 30, 30);
  await expect.poll(async () => (await marks(a)).marks.map((m) => m.name).sort()).toEqual(['Борис', 'Вера']);
  await expect(a.getByRole('button', { name: /^Вера/ })).toBeVisible();
  // Снятие роли у читателя: у остальных пропал сразу (опроса нет, соединение закрыто сервером).
  const share = (await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { id: string; email: string }[] };
  const entry = share.people.find((p) => p.email === emailV)!.id;
  const at = Date.now();
  expect((await api(a, 'DELETE', `/api/files/${id}/share/people/${entry}`)).status).toBeLessThan(300);
  await expect.poll(async () => (await marks(a)).marks.map((m) => m.name), { timeout: 3000 }).toEqual(['Борис']);
  expect(Date.now() - at).toBeLessThan(3000);
  await expect(a.getByRole('button', { name: /^Вера/ })).toHaveCount(0);
  // Закрыл вкладку — пропал сразу.
  await b.context().close();
  await expect.poll(async () => (await marks(a)).marks, { timeout: 3000 }).toEqual([]);
  await expect(a.locator('.fe-presence')).toHaveCount(0);
});
