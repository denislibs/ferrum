// Присутствие на настоящем стенде (спека подсистемы 4, раздел 9): два браузера видят курсоры друг друга с подписью в мировой
// точке, рамку выделения цветом участника, живое превью перетаскивания (только в сцене), аватары и следование; ушедший — по
// закрытию вкладки и по снятию роли — пропадает сразу. Без ожиданий по времени: «сразу» — порядком событий (уход виден раньше, чем
// пришедшее после него движение другого участника), а не сроком.

import { expect, test, type Browser, type Page } from '@playwright/test';
import { encodeClientFrame, encodePresenceBody, PRESENCE_COLORS, presenceColor, SYNC_PROTOCOL } from '@ferrum/protocol';
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

/** Свидетель `witness` (по имени `name` у наблюдателя) двигает курсор в точку мира `to`; первая выборка у наблюдателя, где его
 * курсор уже там, — кто в ней виден. Ушедший до этого движения в ней виден быть не должен. */
async function goneBefore(observer: Page, witness: Page, name: string, to: [number, number]): Promise<string[]> {
  await hoverWorld(witness, to[0], to[1]);
  let names: string[] = [];
  await expect
    .poll(async () => {
      const m = (await marks(observer)).marks;
      names = m.map((x) => x.name).sort();
      return m.find((x) => x.name === name && x.cursor?.map(Math.round).join() === to.join()) !== undefined;
    })
    .toBe(true);
  return names;
}

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

/** Участник без редактора: только соединение файла и кадр присутствия на странице `page` — для полосы аватаров, где нужно больше
 * пяти человек. Повтор кадра — как у настоящего клиента, чтобы участник не устарел, пока идёт тест. */
async function lightJoin(browser: Browser, owner: Page, id: string, name: string, page: string) {
  const email = newEmail('pl');
  const p = await person(browser, email, name);
  expect((await api(owner, 'POST', `/api/files/${id}/share/invite`, { email, role: 'viewer' })).status).toBe(200);
  const user = await userId(p);
  const hello = [...encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 })];
  const body = encodePresenceBody({ page, view: [0, 0, 100, 100], cursor: null, selection: null, gesture: null });
  const state = [...encodeClientFrame({ t: 'presence', body })];
  type Light = { ws?: WebSocket; repeat?: ReturnType<typeof setInterval> };
  const connect = () =>
    p.evaluate(
      ({ id, user, hello, state }) =>
        new Promise<void>((resolve, reject) => {
          const w = window as unknown as Light;
          const ws = new WebSocket(`${location.origin.replace(/^http/, 'ws')}/api/files/${id}/sync?user=${user}`);
          ws.binaryType = 'arraybuffer';
          ws.onopen = () => ws.send(new Uint8Array(hello));
          // Первый ответ — роль: присутствие сервер принимает только после неё.
          ws.onmessage = () => {
            if (w.repeat !== undefined) return;
            ws.send(new Uint8Array(state));
            w.repeat = setInterval(() => ws.send(new Uint8Array(state)), 5000);
            resolve();
          };
          ws.onclose = (e) => reject(new Error(`соединение закрыто: ${e.code}`));
          w.ws = ws;
        }),
      { id, user, hello, state },
    );
  await connect();
  return {
    leave: () =>
      p.evaluate(() => {
        const w = window as unknown as Light;
        clearInterval(w.repeat);
        w.repeat = undefined;
        w.ws!.onclose = null;
        w.ws!.close();
      }),
    rejoin: connect,
  };
}

/** Своё выделение в документе: X узла по панели свойств. */
const selectedX = (page: Page) => page.evaluate(() => (window.__ferrum!.ui!.summary as { x?: { value: number } } | null)?.x?.value);

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
  // Пока жест идёт, в документе другого узел на месте.
  await b.keyboard.press('ControlOrMeta+a');
  await expect.poll(() => selectedX(b)).toBe(0);
  await a.mouse.up();
  // Отпустил: превью снято, сдвиг пришёл правкой — в документе другого узел на 60 правее.
  await expect.poll(async () => (await marks(b)).preview).toEqual([]);
  await expect.poll(() => selectedX(b)).toBe(60);
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
  await expect.poll(async () => marks(b)).toEqual({ marks: [], preview: [] });
  // Жест не дошёл до документа: узел там, где его оставило первое отпускание.
  expect(await selectedX(b)).toBe(60);
});

test('аватары и следование: другая страница, клавиатура (фокус остаётся, Esc — выход), камера и страница за участником; действие на холсте и уход — выход', async ({
  browser,
}) => {
  const { a, b } = await pair(browser);
  const avatar = b.getByRole('button', { name: /^Анна Смирнова/ });
  const stage = b.locator('.fe-stage');
  await hoverWorld(a, 10, 10);
  await expect(avatar).toBeVisible();
  await expect(avatar).toHaveText('АС');
  await expect(avatar).toHaveAccessibleName('Анна Смирнова');
  // Участник на другой странице: аватар приглушён, в подписи — где он.
  await a.getByRole('button', { name: 'Добавить страницу' }).click();
  await expect(avatar).toHaveClass(/fe-avatar--away/);
  await expect(avatar).toHaveAccessibleName('Анна Смирнова — на другой странице');
  // Страница дошла правкой, и после неё — кадр участника. Присутствие идёт мимо очереди правок и может её обогнать, а страницу,
  // которой ещё нет, ядро у участника не запоминает: перейдёт за ним на следующем его кадре.
  const newPage = await a.evaluate(() => window.__ferrum!.ui!.pages.current);
  await expect.poll(() => b.evaluate(() => window.__ferrum!.ui!.pages.list.map((p) => p.id))).toContain(newPage);
  await hoverWorld(a, 20, 20);
  // С клавиатуры: Enter — следовать. Своя страница сменилась, список пришёл заново — фокус на той же кнопке, подпись не о
  // следовании, состояние — в `aria-pressed`.
  await avatar.focus();
  await b.keyboard.press('Enter');
  await expect(avatar).toHaveAttribute('aria-pressed', 'true');
  await expect(stage).toHaveClass(/fe-following/);
  await expect.poll(() => b.evaluate(() => window.__ferrum!.ui!.pages.current)).toBe(newPage);
  await expect(avatar).not.toHaveClass(/fe-avatar--away/);
  await expect(avatar).toHaveAccessibleName('Анна Смирнова');
  await expect(avatar).toBeFocused();
  // Участник уходит далеко: камера другого — за ним.
  await a.locator('canvas').hover();
  await a.mouse.wheel(3000, 2000);
  const target = await live(a);
  await expect.poll(async () => Math.round((await live(b)).cameraX - target.cameraX)).toBe(0);
  await expect.poll(async () => Math.round((await live(b)).cameraY - target.cameraY)).toBe(0);
  // Esc, пока фокус на аватаре, — выход (как в Figma: любая своя клавиша).
  await expect(avatar).toBeFocused();
  await b.keyboard.press('Escape');
  await expect(avatar).toHaveAttribute('aria-pressed', 'false');
  await expect(stage).not.toHaveClass(/fe-following/);
  // Щелчок — снова; своё нажатие на холсте — выход.
  await avatar.click();
  await expect(avatar).toHaveAttribute('aria-pressed', 'true');
  await b.locator('canvas').click({ position: { x: 300, y: 300 } });
  await expect(avatar).toHaveAttribute('aria-pressed', 'false');
  await expect(stage).not.toHaveClass(/fe-following/);
  expect((await marks(b)).following).toBeUndefined();
  // Снова; участник ушёл — выход, аватара нет.
  await avatar.click();
  await expect(stage).toHaveClass(/fe-following/);
  await a.context().close();
  await expect(stage).not.toHaveClass(/fe-following/);
  await expect(avatar).toHaveCount(0);
  expect((await marks(b)).following).toBeUndefined();
});

test('больше пяти человек: пять аватаров и счётчик со списком; людей стало меньше — меню закрыто и само не открывается', async ({
  browser,
}) => {
  const a = await person(browser, newEmail('pn'), 'Анна');
  const id = await newFile(a);
  const here = await a.evaluate(() => window.__ferrum!.ui!.pages.current);
  const others = [];
  for (let i = 1; i <= 6; i++) others.push(await lightJoin(browser, a, id, `Участник ${i}`, i === 6 ? '1@1' : here));
  const bar = a.getByRole('group', { name: 'Участники' });
  const more = a.getByRole('button', { name: 'Все участники: 6' });
  await expect(more).toHaveText('+1');
  await expect(bar.getByRole('button')).toHaveCount(6);
  await more.click();
  const menu = a.getByRole('menu', { name: 'Участники' });
  await expect(menu.getByRole('menuitemcheckbox')).toHaveText([
    'Участник 1',
    'Участник 2',
    'Участник 3',
    'Участник 4',
    'Участник 5',
    'Участник 6 (другая страница)',
  ]);
  // Пункт списка — следование, как щелчок по аватару.
  await menu.getByRole('menuitemcheckbox', { name: 'Участник 6 (другая страница)' }).click();
  await expect(a.locator('.fe-stage')).toHaveClass(/fe-following/);
  await a.keyboard.press('Escape');
  await expect(a.locator('.fe-stage')).not.toHaveClass(/fe-following/);
  // Меню открыто, один ушёл: счётчика нет — нет и меню.
  await more.click();
  await expect(menu).toBeVisible();
  await others[0].leave();
  await expect(more).toHaveCount(0);
  await expect(bar.getByRole('button')).toHaveCount(5);
  await expect(menu).toHaveCount(0);
  // Вернулся: счётчик снова есть, меню — только по щелчку.
  await others[0].rejoin();
  await expect(a.getByRole('button', { name: 'Все участники: 6' })).toBeVisible();
  await expect(menu).toHaveCount(0);
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
  // Снятие роли у читателя: у остальных пропал сразу (опроса нет, соединение закрыто сервером). «Сразу» — не сроком: доступ
  // снят и соединение закрыто, после этого Борис сдвинул курсор, и в той же выборке, где его курсор уже новый, Веры нет. Через
  // устаревание (30 с без кадров у сервера и у ядра) она была бы ещё видна.
  const share = (await api(a, 'GET', `/api/files/${id}/share`)).body as { people: { id: string; email: string }[] };
  const entry = share.people.find((p) => p.email === emailV)!.id;
  expect((await api(a, 'DELETE', `/api/files/${id}/share/people/${entry}`)).status).toBeLessThan(300);
  await expect(v.getByRole('alert').getByText('Нет доступа', { exact: true })).toBeVisible();
  expect(await goneBefore(a, b, 'Борис', [70, 70])).toEqual(['Борис']);
  await expect(a.getByRole('button', { name: /^Вера/ })).toHaveCount(0);
  // Закрыл вкладку — пропал сразу; свидетель — вторая вкладка Анны (свои другие вкладки видны курсором).
  const a2 = await a.context().newPage();
  await testMode(a2);
  await a2.goto(`/#/file/${id}`);
  await editorReady(a2);
  await synced(a2);
  await hoverWorld(a2, 20, 20);
  await expect.poll(async () => (await marks(a)).marks.map((m) => m.name).sort()).toEqual(['Анна Смирнова', 'Борис']);
  await b.context().close();
  expect(await goneBefore(a, a2, 'Анна Смирнова', [90, 90])).toEqual(['Анна Смирнова']);
  await expect(a.locator('.fe-presence')).toHaveCount(0);
});
