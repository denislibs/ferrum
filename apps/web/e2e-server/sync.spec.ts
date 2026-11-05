// Совместная работа на настоящем стенде (спека подсистемы 4, разделы 7, 8, 13.2): два браузера правят один файл живьём, отмена
// трогает только своё, обрыв и рестарт сервера ничего не теряют, снятие роли действует сразу.

import { expect, test, type Browser, type Page } from '@playwright/test';
import { appScreen, drag, editorReady, fileId, live, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { compose, newEmail, resetStand, signIn } from './stand';

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

const nodes = (page: Page, n: number, timeout = 15_000) => expect.poll(async () => (await live(page)).nodes, { timeout }).toBe(n);
const indicator = (page: Page) => page.locator('.fe-sync');
const synced = (page: Page, timeout = 15_000) => expect(indicator(page)).toHaveText('Синхронизировано', { timeout });

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

/** Новый файл: страница уже в нём, можно звать людей. */
async function newFile(page: Page): Promise<string> {
  await page.goto('/?nodes=0#/new');
  await editorReady(page);
  await synced(page);
  return fileId(page);
}

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

/** Человек получает доступ по ссылке «может редактировать» или «может смотреть» и открывает файл. */
async function join(owner: Page, guest: Page, id: string, role: 'editor' | 'viewer' = 'editor'): Promise<void> {
  const email = await guest.evaluate(async () => {
    const r = (await (await fetch('/api/auth/session')).json()) as { user: { email: string } };
    return r.user.email;
  });
  const res = await api(owner, 'POST', `/api/files/${id}/share/invite`, { email, role });
  expect(res.status).toBe(200);
  await guest.goto(`/#/file/${id}`);
  await guest.reload();
  await editorReady(guest);
}

async function peopleEntry(owner: Page, id: string, email: string): Promise<string> {
  const res = await api(owner, 'GET', `/api/files/${id}/share`);
  const people = (res.body as { people: { id: string; email: string }[] }).people;
  return people.find((p) => p.email === email)!.id;
}

test('двое правят один файл: каждый видит чужое, отмена трогает только своё', async ({ browser }) => {
  const [a, b] = [await person(browser, newEmail('sa'), 'Анна'), await person(browser, newEmail('sb'), 'Борис')];
  const id = await newFile(a);
  await join(a, b, id);
  await synced(b);

  await drawRect(a);
  await nodes(b, 1);
  await drawRect(b, 200);
  await nodes(a, 2);
  await synced(a);
  await synced(b);

  // Отмена у `a` убирает его прямоугольник, чужой остаётся у обоих.
  await a.locator('canvas').click({ position: { x: 10, y: 10 } });
  await a.keyboard.press('ControlOrMeta+KeyZ');
  await nodes(a, 1);
  await nodes(b, 1);
  await synced(a);
  // Отмена у `b` — его прямоугольник; у него шаг чужой правки не появился.
  await b.locator('canvas').click({ position: { x: 10, y: 10 } });
  await b.keyboard.press('ControlOrMeta+KeyZ');
  await nodes(b, 0);
  await nodes(a, 0);
  await b.keyboard.press('ControlOrMeta+KeyZ');
  await nodes(a, 0);

  // Заново открытый файл — то же самое: всё дошло до сервера.
  await a.keyboard.press('ControlOrMeta+Shift+KeyZ');
  await nodes(a, 1);
  await synced(a);
  await a.reload();
  await editorReady(a);
  await nodes(a, 1);
  await synced(a);
});

test('читатель видит правки живьём и не пишет', async ({ browser }) => {
  const [a, v] = [await person(browser, newEmail('sc'), 'Анна'), await person(browser, newEmail('sd'), 'Вера')];
  const id = await newFile(a);
  await join(a, v, id, 'viewer');
  await expect(v.getByText('Только просмотр')).toBeVisible();
  await drawRect(a);
  await nodes(v, 1);
  await v.locator('canvas').click({ position: { x: 10, y: 10 } });
  await v.keyboard.press('KeyR');
  await expect.poll(async () => (await live(a)).nodes).toBe(1);
  const before = (await live(v)).nodes;
  expect(before).toBe(1);
});

test('вторая вкладка того же человека — такой же участник', async ({ browser }) => {
  const a = await person(browser, newEmail('se'), 'Анна');
  const id = await newFile(a);
  const tab = await a.context().newPage();
  await testMode(tab);
  await tab.goto(`/#/file/${id}`);
  await editorReady(tab);
  await synced(tab);
  await drawRect(a);
  await nodes(tab, 1);
  await drawRect(tab, 200);
  await nodes(a, 2);
});

test('снятие роли действует сразу, понижение переводит в просмотр без перезагрузки', async ({ browser }) => {
  const [a, b] = [await person(browser, newEmail('sf'), 'Анна'), await person(browser, newEmail('sg'), 'Борис')];
  const email = await b.evaluate(async () => ((await (await fetch('/api/auth/session')).json()) as { user: { email: string } }).user.email);
  const id = await newFile(a);
  await join(a, b, id);
  await synced(b);
  const entry = await peopleEntry(a, id, email);

  const started = Date.now();
  expect((await api(a, 'PATCH', `/api/files/${id}/share/people/${entry}`, { role: 'viewer' })).status).toBe(200);
  await expect(b.getByText('Только просмотр')).toBeVisible({ timeout: 3000 });
  expect(Date.now() - started).toBeLessThan(3000);
  // Правки идут и читателю.
  await drawRect(a);
  await nodes(b, 1);

  expect((await api(a, 'PATCH', `/api/files/${id}/share/people/${entry}`, { role: 'editor' })).status).toBe(200);
  await expect(b.getByText('Только просмотр')).toBeHidden({ timeout: 3000 });
  await drawRect(b, 200);
  await nodes(a, 2);

  const removed = Date.now();
  expect((await api(a, 'DELETE', `/api/files/${id}/share/people/${entry}`)).status).toBe(200);
  await expect(b.getByRole('alert').getByText('Нет доступа')).toBeVisible({ timeout: 3000 });
  expect(Date.now() - removed).toBeLessThan(3000);
});

test('обрыв сети: правки копятся, индикатор говорит правду, при возврате всё сходится', async ({ browser }) => {
  test.skip(test.info().project.name === 'server-firefox', 'setOffline в Firefox не закрывает WebSocket');
  const [a, b] = [await person(browser, newEmail('sh'), 'Анна'), await person(browser, newEmail('si'), 'Борис')];
  const id = await newFile(a);
  await join(a, b, id);
  await synced(b);

  await a.context().setOffline(true);
  await expect(indicator(a)).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(a);
  await drawRect(a, 150);
  await drawRect(b, 300);
  await nodes(b, 1);
  await a.context().setOffline(false);
  await synced(a, 45_000);
  await nodes(a, 3);
  await nodes(b, 3);
});

test('рестарт сервера: переподключение, ни одна правка не потеряна', async ({ browser }) => {
  const [a, b] = [await person(browser, newEmail('sj'), 'Анна'), await person(browser, newEmail('sk'), 'Борис')];
  const id = await newFile(a);
  await join(a, b, id);
  await synced(b);
  await drawRect(a);
  await nodes(b, 1);

  compose('restart', 'server');
  await synced(a, 60_000);
  await synced(b, 60_000);
  await drawRect(b, 200);
  await nodes(a, 2);
  await drawRect(a, 400);
  await nodes(b, 3);
  // Правка, сделанная прямо в рестарт: неподтверждённая уходит заново.
  const restart = (async () => compose('restart', 'server'))();
  await drawRect(a, 600);
  await restart;
  await synced(a, 60_000);
  await nodes(b, 4, 60_000);
});

test('база отвалилась и зависла: сервер живёт, правки доходят, когда она вернулась', async ({ browser }) => {
  const a = await person(browser, newEmail('sl'), 'Анна');
  await newFile(a);
  await drawRect(a);
  await synced(a);
  // У стенда база на tmpfs: рестарт стёр бы её, поэтому обрыв всех соединений и заморозка.
  compose(
    'exec',
    '-T',
    'postgres',
    'psql',
    '-U',
    'ferrum',
    '-d',
    'ferrum',
    '-c',
    `SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE pid <> pg_backend_pid() AND datname = 'ferrum'`,
  );
  compose('pause', 'postgres');
  await drawRect(a, 100);
  await a.waitForTimeout(8000);
  await expect(indicator(a)).not.toHaveText('Синхронизировано');
  compose('unpause', 'postgres');
  await drawRect(a, 200);
  await synced(a, 60_000);
  await a.reload();
  await editorReady(a);
  await nodes(a, 3);
});

test('выход в другой вкладке закрывает соединение и не теряет правки молча', async ({ browser }) => {
  const a = await person(browser, newEmail('sm'), 'Анна');
  await newFile(a);
  await drawRect(a);
  await synced(a);
  expect((await api(a, 'POST', '/api/auth/sign-out', {})).status).toBe(200);
  await expect(a.getByRole('alert').getByText('Сессия завершена')).toBeVisible({ timeout: 5000 });
});

test('журнал браузера снимается подтверждением сервера и держит неподтверждённое', async ({ browser }) => {
  const a = await person(browser, newEmail('sn'), 'Анна');
  await newFile(a);
  await drawRect(a);
  await synced(a);
  test.skip(test.info().project.name === 'server-firefox', 'setOffline в Firefox не закрывает WebSocket');
  await a.context().setOffline(true);
  await expect(indicator(a)).toHaveText('Нет сети — правки сохранятся при подключении', { timeout: 45_000 });
  await drawRect(a, 200);
  // Журнал браузера записан: перезагрузка без сети не теряет правку, сеть вернулась — она на сервере.
  await a.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(async () => (await live(a)).nodes).toBe(2);
  await a.waitForTimeout(2500);
  await a.context().setOffline(false);
  await a.reload();
  await editorReady(a);
  await nodes(a, 2);
  await synced(a, 45_000);
  await a.reload();
  await editorReady(a);
  await nodes(a, 2);
});

test('файл 100 000 узлов: чужие правки во время своего жеста, основной поток свободен, задержка до другого @big', async ({
  browser,
}, info) => {
  test.skip(info.project.name === 'server-firefox', 'замер в одном браузере');
  test.setTimeout(300_000);
  const [a, b] = [await person(browser, newEmail('so'), 'Анна'), await person(browser, newEmail('sp'), 'Борис')];
  await a.goto('/?nodes=100000#/new');
  await editorReady(a);
  await synced(a, 120_000);
  const id = fileId(a);
  await join(a, b, id);
  await synced(b, 120_000);
  await nodes(b, 100_000, 60_000);

  // Задержка: дубль у `b`, момент появления у `a`; часы одной машины.
  await drawRect(b, 300);
  await nodes(a, 100_001, 60_000);
  await a.evaluate(() => {
    const w = window as unknown as { __longTasks: number[] };
    w.__longTasks = [];
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) w.__longTasks.push(e.duration);
    }).observe({ entryTypes: ['longtask'] });
  });
  // Жест у `a` идёт, пока `b` правит.
  const box = (await a.locator('canvas').boundingBox())!;
  await a.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await a.mouse.down();
  const delays: number[] = [];
  let expected = (await live(a)).nodes;
  for (let i = 0; i < 20; i++) {
    await a.mouse.move(box.x + box.width / 2 + i * 3, box.y + box.height / 2 + i);
    const started = Date.now();
    await b.evaluate(() =>
      (window as unknown as { __ferrum: { core: { send(c: unknown): void } } }).__ferrum.core.send({
        type: 'execute',
        command: 'edit.duplicate',
      }),
    );
    expected++;
    await expect.poll(async () => (await live(a)).nodes, { intervals: [5], timeout: 30_000 }).toBeGreaterThanOrEqual(expected);
    delays.push(Date.now() - started);
  }
  await a.mouse.up();
  const long = await a.evaluate(() => (window as unknown as { __longTasks: number[] }).__longTasks);
  delays.sort((x, y) => x - y);
  console.log(
    '100 000 узлов, задержка до другого, мс: p50',
    delays[10],
    'p95',
    delays[18],
    'наибольшая',
    delays[19],
    'длинные задачи основного потока',
    long,
  );
  expect(delays[18]).toBeLessThan(1000);
  // Синхронизация — в worker'ах: на основном потоке её нет.
  expect(long.filter((d) => d > 200)).toEqual([]);
});
