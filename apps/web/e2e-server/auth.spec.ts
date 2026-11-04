import { expect, test, type Browser, type Page } from '@playwright/test';
import { appScreen, drag, editorReady, live, mainMenu, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { linksFor, mailCount, newEmail, resetStand, valkey } from './stand';

test.beforeEach(resetStand);

/** Ошибки загрузки и нарушения CSP видны как ошибки консоли: на всех экранах их нет. */
function watchConsole(page: Page): string[] {
  const errors: string[] = [];
  page.on('console', (m) => {
    // Ответ 401/404 браузер пишет сам, это не ошибка страницы.
    if (m.type() === 'error' && !m.text().includes('Failed to load resource')) errors.push(m.text());
  });
  page.on('pageerror', (e) => errors.push(e.message));
  return errors;
}

async function requestLink(page: Page, email: string): Promise<void> {
  await page.getByLabel('Электронная почта').fill(email);
  await page.getByRole('button', { name: 'Получить ссылку' }).click();
  await expect(page.getByRole('heading', { name: 'Проверьте почту' })).toBeVisible();
  await expect(page.getByText(email)).toBeVisible();
}

/** Вход целиком: от экрана входа до приложения. */
async function signIn(page: Page, email: string, name = 'Иван Петров'): Promise<void> {
  await requestLink(page, email);
  const [link] = await linksFor(email);
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  if (name) {
    await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
    await page.getByLabel('Имя').fill(name);
    await page.getByRole('button', { name: 'Продолжить' }).click();
  }
}

const newContext = async (browser: Browser) => (await browser.newContext({ baseURL: STAND_URL })).newPage();

test('без сессии виден только экран входа, кнопки Google и GitHub неактивны', async ({ page, request }) => {
  const errors = watchConsole(page);
  for (const path of ['/', '/#/file/abc', '/#/new', '/#/drafts']) {
    await page.goto(path);
    await page.reload();
    await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
    await expect(page.locator('.fe-files, .fe-app')).toHaveCount(0);
    await expect(page.locator('#fe-splash')).toHaveCount(0);
  }
  // Ядро без входа не запускается, стенд ядра в рабочей сборке отсутствует.
  expect(await page.evaluate(() => window.__ferrum)).toBeUndefined();
  expect((await request.get('/debug.html')).status()).toBe(404);

  for (const name of ['Войти через Google', 'Войти через GitHub']) {
    const b = page.getByRole('button', { name });
    await expect(b).toHaveAttribute('aria-disabled', 'true');
    const tip = b.getByRole('tooltip');
    await expect(tip).toBeHidden();
    await b.hover();
    await expect(tip).toBeVisible();
    await expect(tip).toHaveText('Скоро');
    // Playwright ждёт активности у `aria-disabled`: нажимаем как пользователь — по координатам.
    await page.mouse.down();
    await page.mouse.up();
    await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  }
  expect(errors).toEqual([]);
});

test('вход целиком: письмо, подтверждение, имя, сессия, редактор', async ({ page, context }, info) => {
  const errors = watchConsole(page);
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await expect(page.locator('.fe-account-row')).toContainText('Иван Петров');
  await expect(page.locator('.fe-avatar')).toHaveText('ИП');

  // Токен сессии недоступен скриптам.
  const session = (await context.cookies()).find((c) => c.name === 'ferrum.session_token')!;
  expect(session.httpOnly).toBe(true);
  expect(session.sameSite).toBe('Lax');
  expect(session.secure).toBe(false);
  expect(await page.evaluate(() => document.cookie)).toBe('');
  const me = (await (await page.request.get('/api/auth/session')).json()) as { user: { email: string; name: string } };
  expect(me.user).toMatchObject({ email, name: 'Иван Петров' });

  // Перезагрузка: сессия жива, экран имени второй раз не показывается.
  await page.reload();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');

  // Редактор работает в рабочей сборке: изоляция, CSP, wasm, файлы — в IndexedDB, как раньше.
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(true);
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
  await expect(page).toHaveURL(/#\/file\//);
  expect(errors).toEqual([]);
});

test('второй вход тем же адресом: экрана имени нет', async ({ page, browser }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  const other = await newContext(browser);
  await other.goto('/');
  await signIn(other, email, '');
  await expect(other.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await expect(other.locator('.fe-account-row')).toContainText('Иван Петров');
  await other.context().close();
});

test('открытие ссылки токен не тратит: сканер и страница без нажатия', async ({ page, browser, request }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  // Почтовый сканер: GET по ссылке и открытие страницы со скриптами в другом браузере.
  expect((await request.get(link)).status()).toBe(200);
  const scanner = await newContext(browser);
  const asked: string[] = [];
  scanner.on('request', (r) => {
    if (r.url().includes('/api/')) asked.push(r.url());
  });
  await scanner.goto(link);
  await expect(scanner.getByRole('button', { name: 'Войти', exact: true })).toBeVisible();
  expect(asked).toEqual([]);
  await scanner.context().close();
  // Токен цел.
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
});

test('ссылка одноразовая: второе нажатие — сообщение и путь к новой ссылке', async ({ page, browser }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  const [link] = await linksFor(email);
  const other = await newContext(browser);
  await other.goto(link);
  await other.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(other.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await other.getByRole('button', { name: 'Запросить новую ссылку' }).click();
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await other.context().close();
});

test('истёкшая ссылка показывает то же сообщение', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  // Токен истекает по сроку ключа в Valkey: срок сокращается, ключ исчезает сам.
  const keys = valkey('--scan', '--pattern', 'verification:*').trim().split('\n');
  expect(keys).toHaveLength(1);
  const ttl = Number(valkey('TTL', keys[0]).trim());
  expect(ttl).toBeGreaterThan(890);
  expect(ttl).toBeLessThanOrEqual(900);
  valkey('PEXPIRE', keys[0], '1');
  await expect.poll(() => valkey('EXISTS', keys[0]).trim()).toBe('0');
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
});

test('испорченная ссылка: сообщение сразу, запросов нет', async ({ page }) => {
  await page.goto('/#/auth/confirm?token=short');
  await expect(page.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await expect(page.getByRole('button', { name: 'Войти', exact: true })).toHaveCount(0);
});

test('возврат на запрошенный адрес: раздел из другого браузера и файл в этом', async ({ page, browser }, info) => {
  const email = newEmail(info.project.name);
  // Письмо открыто в другом браузере: адрес едет в ссылке, а не в хранилище страницы.
  await page.goto('/#/trash');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  expect(link).toContain('next=');
  const other = await newContext(browser);
  await other.goto(link);
  await other.getByRole('button', { name: 'Войти', exact: true }).click();
  await other.getByLabel('Имя').fill('Анна');
  await other.getByRole('button', { name: 'Продолжить' }).click();
  await expect(other.getByRole('heading', { level: 1 })).toHaveText('Корзина');
  await expect(other).toHaveURL(/#\/trash$/);

  // Файл: создан, сессия снята, открыт адрес файла → вход → снова этот файл.
  await other.goto('/#/');
  await other.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await expect(other).toHaveURL(/#\/file\/[^/?]+$/);
  await other.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
  const fileUrl = other.url();
  await other.goto('/#/');
  await other.getByRole('button', { name: 'Выйти' }).click();
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await resetStand();
  await other.goto(fileUrl);
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await requestLink(other, email);
  await other.goto((await linksFor(email))[0]);
  await other.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(other).toHaveURL(fileUrl);
  await other.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
  await other.context().close();
});

test('повторная отправка — через 60 с', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.clock.install();
  await page.goto('/');
  await requestLink(page, email);
  const again = page.getByRole('button', { name: /Отправить ещё раз/ });
  await expect(again).toBeDisabled();
  await expect(again).toHaveText('Отправить ещё раз через 60 с');
  await page.clock.fastForward(30_000);
  await expect(again).toHaveText('Отправить ещё раз через 30 с');
  await expect(again).toBeDisabled();
  await page.clock.fastForward(30_000);
  await expect(again).toHaveText('Отправить ещё раз');
  await expect(again).toBeEnabled();
  await again.click();
  await expect(again).toHaveText('Отправить ещё раз через 60 с');
  await linksFor(email, 2);
  expect(await mailCount(email)).toBe(2);
  await page.getByRole('button', { name: 'Другой адрес' }).click();
  await expect(page.getByLabel('Электронная почта')).toBeVisible();
});

test('ошибки формы: неверный адрес, предел писем, сервер недоступен', async ({ page }, info) => {
  await page.goto('/');
  await page.getByLabel('Электронная почта').fill('не адрес');
  await page.getByRole('button', { name: 'Получить ссылку' }).click();
  await expect(page.getByRole('alert')).toContainText('Проверьте адрес');
  const email = newEmail(info.project.name);
  // Настоящий предел сервера: 5 писем на адрес, шестое — отказ.
  for (let i = 0; i < 5; i++)
    expect((await page.request.post('/api/auth/sign-in/magic-link', { data: { email }, headers: { origin: STAND_URL } })).status()).toBe(
      200,
    );
  await page.getByLabel('Электронная почта').fill(email);
  await page.getByRole('button', { name: 'Получить ссылку' }).click();
  await expect(page.getByRole('alert')).toHaveText('Слишком много запросов. Попробуйте позже');
  await page.route('**/api/auth/**', (r) => r.abort());
  await page.getByRole('button', { name: 'Получить ссылку' }).click();
  await expect(page.getByRole('alert')).toContainText('Сервер недоступен');
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Не удалось открыть Ferrum' })).toBeVisible();
  await page.unroute('**/api/auth/**');
  await page.getByRole('button', { name: 'Повторить' }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
});

test('имя обязательно: пустое не принимается', async ({ page }, info) => {
  await page.goto('/');
  await signIn(page, newEmail(info.project.name), '');
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  await page.getByLabel('Имя').fill('   ');
  await page.getByRole('button', { name: 'Продолжить' }).click();
  await expect(page.getByRole('alert')).toHaveText('Введите имя');
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
});

test('выход с экрана файлов: сессия снята, после перезагрузки — экран входа', async ({ page, context }, info) => {
  await page.goto('/');
  await signIn(page, newEmail(info.project.name));
  await page.getByRole('button', { name: 'Выйти' }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  expect((await context.cookies()).find((c) => c.name === 'ferrum.session_token')).toBeUndefined();
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
});

test('настройки: вкладка «Аккаунт», выход на всех устройствах', async ({ page, browser }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  const other = await newContext(browser);
  await other.goto('/');
  await signIn(other, email, '');
  await expect(other.getByRole('heading', { level: 1 })).toHaveText('Недавние');

  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
  await mainMenu(page, 'Настройки', 'Настройки…');
  await page.getByRole('tab', { name: 'Аккаунт' }).click();
  const panel = page.getByRole('tabpanel', { name: 'Аккаунт' });
  await expect(panel).toContainText('Иван Петров');
  await expect(panel).toContainText(email);
  await panel.getByRole('button', { name: 'Выйти на всех устройствах' }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();

  // Другое устройство: при следующей загрузке — экран входа.
  await other.reload();
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await other.context().close();
});

test('вход и выход видны в других вкладках', async ({ page, context }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/#/drafts');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  // Письмо открыто в новой вкладке, а первая ждала на «Проверьте почту».
  const tab = await context.newPage();
  await tab.goto(link);
  await tab.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(tab.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  await tab.getByLabel('Имя').fill('Иван');
  await tab.getByRole('button', { name: 'Продолжить' }).click();
  await expect(tab.getByRole('heading', { level: 1 })).toBeVisible();
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(tab.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
});

test('сервер: /healthz и заголовки изоляции на настоящем стенде', async ({ request }) => {
  const health = await request.get('/healthz');
  expect(health.status()).toBe(200);
  expect(await health.json()).toEqual({ ok: true, postgres: true, valkey: true });
  const index = await request.get('/');
  expect(index.headers()['cross-origin-opener-policy']).toBe('same-origin');
  expect(index.headers()['cross-origin-embedder-policy']).toBe('credentialless');
  expect(index.headers()['content-security-policy']).toContain("frame-ancestors 'none'");
});

/** Рабочая сборка запрещает `eval` (CSP): ждём число узлов без строковых условий. */
const nodesAre = (page: Page, n: number) => expect.poll(async () => (await live(page)).nodes).toBe(n);

async function drawRect(page: Page): Promise<void> {
  await page.locator('canvas').click({ position: { x: 10, y: 10 } });
  await page.keyboard.press('KeyR');
  const l = await live(page);
  await drag(page, await appScreen(page, l, l.cameraX - 50, l.cameraY - 40), 100, 80);
  await nodesAre(page, l.nodes + 1);
}

/** Вторая ссылка для уже вошедшего человека: письмо запрашивается в обход интерфейса. */
async function secondLink(page: Page, email: string): Promise<string> {
  const res = await page.request.post('/api/auth/sign-in/magic-link', { data: { email }, headers: { origin: STAND_URL } });
  expect(res.status()).toBe(200);
  return (await linksFor(email, 2))[0];
}

/** Ссылка набрана в адресной строке этой вкладки: меняется только часть после `#`. */
const pasteHash = (page: Page, link: string) => page.evaluate((h) => void (location.hash = h), new URL(link).hash);

const enter = (page: Page) => page.getByRole('button', { name: 'Войти', exact: true });

test('ссылка из письма в той же вкладке на экране файлов: страница подтверждения', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await pasteHash(page, await secondLink(page, email));
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await enter(page).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
});

test('ссылка из письма в той же вкладке в редакторе: правка не теряется', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
  await page.goto('/?nodes=0');
  await signIn(page, email);
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await editorReady(page);
  const url = page.url();
  await drawRect(page);
  // Сразу, не дожидаясь журнала: приложение должно записать правку само.
  await pasteHash(page, await secondLink(page, email));
  await expect(enter(page)).toBeVisible();
  await enter(page).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.goto(url);
  await page.reload();
  await editorReady(page);
  await nodesAre(page, 1);
});

test('сбой сети на странице подтверждения: «Войти» остаётся, ссылка цела', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  await page.goto(link);
  let down = true;
  await page.route('**/api/auth/confirm', (r) => (down ? r.abort() : r.continue()));
  await enter(page).click();
  await expect(page.getByRole('alert')).toContainText('Сервер недоступен');
  await expect(enter(page)).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Запросить новую ссылку' })).toHaveCount(0);
  expect(page.url()).toContain('token=');
  down = false;
  await enter(page).click();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
});

test('странный ответ на подтверждение не считается входом', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  await page.goto(link);
  await page.route('**/api/auth/confirm', (r) =>
    r.fulfill({ status: 200, contentType: 'text/html', body: '<html>вход через сеть</html>' }),
  );
  await enter(page).click();
  await expect(page.getByRole('alert')).toContainText('Сервер недоступен');
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toHaveCount(0);
  await expect(enter(page)).toBeEnabled();
});

test('чужой ответ на запрос сессии: ошибка и «Повторить», а не вечная заставка', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  const replies = [
    { status: 200, contentType: 'text/html', body: '<html>страница сети</html>' },
    { status: 502, contentType: 'application/json', body: '{"error":"Bad Gateway"}' },
    { status: 200, contentType: 'application/json', body: '{"user":{"id":1}}' },
  ];
  for (const reply of replies) {
    await page.route('**/api/auth/session', (r) => r.fulfill(reply));
    await page.reload();
    await expect(page.getByRole('heading', { name: 'Не удалось открыть Ferrum' })).toBeVisible();
    await expect(page.getByRole('alert')).toContainText('Сервер недоступен');
    await expect(page.locator('#fe-splash')).toHaveCount(0);
    await page.unroute('**/api/auth/session');
    await page.getByRole('button', { name: 'Повторить' }).click();
    await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  }
});

test('«Повторить» показывает занятую кнопку, а не пустую страницу', async ({ page }) => {
  await page.route('**/api/auth/session', (r) => r.abort());
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Не удалось открыть Ferrum' })).toBeVisible();
  await page.unroute('**/api/auth/session');
  let release!: () => void;
  const hold = new Promise<void>((resolve) => (release = resolve));
  await page.route('**/api/auth/session', async (r) => {
    await hold;
    await r.continue();
  });
  await page.getByRole('button', { name: 'Повторить' }).click();
  await expect(page.getByRole('heading', { name: 'Не удалось открыть Ferrum' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Повторить' })).toBeDisabled();
  release();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
});

test('выход из редактора: правка не пропадает', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
  await page.goto('/?nodes=0');
  await signIn(page, email);
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await editorReady(page);
  const url = page.url();
  await drawRect(page);
  await mainMenu(page, 'Настройки', 'Настройки…');
  await page.getByRole('tab', { name: 'Аккаунт' }).click();
  await page.getByRole('tabpanel', { name: 'Аккаунт' }).getByRole('button', { name: 'Выйти', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await signIn(page, email, '');
  // Запрошенный до выхода адрес — тот же файл.
  await expect(page).toHaveURL(url);
  await editorReady(page);
  await nodesAre(page, 1);
});

test('выход, когда запись не удалась: вопрос, «Остаться» отменяет выход', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
  await page.goto('/?nodes=0');
  await signIn(page, email);
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await editorReady(page);
  await drawRect(page);
  await page.evaluate(() => window.__ferrumFiles!.store.failWrites(true));
  await mainMenu(page, 'Настройки', 'Настройки…');
  await page.getByRole('tab', { name: 'Аккаунт' }).click();
  await page.getByRole('tabpanel', { name: 'Аккаунт' }).getByRole('button', { name: 'Выйти', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  await expect(dialog).toContainText('Изменения не сохранены');
  await dialog.getByRole('button', { name: 'Остаться' }).click();
  await expect(dialog).toHaveCount(0);
  // Сессия жива, редактор на месте.
  const me = (await (await page.request.get('/api/auth/session')).json()) as { user: { email: string } | null };
  expect(me.user?.email).toBe(email);
  await nodesAre(page, 1);
});

test('смена экрана: фокус на заголовке, а с полем — в поле', async ({ page }, info) => {
  await page.goto('/');
  await requestLink(page, newEmail(info.project.name));
  await expect(page.getByRole('heading', { name: 'Проверьте почту' })).toBeFocused();
  await page.getByRole('button', { name: 'Другой адрес' }).click();
  await expect(page.getByLabel('Электронная почта')).toBeFocused();
});

test('смена адреса на экране входа не сбрасывает «Проверьте почту»', async ({ page }, info) => {
  await page.goto('/');
  await requestLink(page, newEmail(info.project.name));
  await page.evaluate(() => void (location.hash = '#/trash'));
  // Загрузка по новому адресу идёт к серверу: ждём её окончания.
  await page.waitForResponse((r) => r.url().endsWith('/api/auth/session'));
  await page.waitForTimeout(300);
  await expect(page.getByRole('heading', { name: 'Проверьте почту' })).toBeVisible();
});
