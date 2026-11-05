import { expect, test, type Browser, type Page } from '@playwright/test';
import { appScreen, drag, editorReady, live, mainMenu, testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { linksFor, mailCount, newEmail, requestLink, resetStand, signIn, valkey } from './stand';

test.beforeEach(resetStand);

/** Ошибки загрузки и нарушения CSP видны как ошибки консоли: на этих экранах их нет. Потерянный
 * wasm или шрифт тест заметит; значка сайта у приложения нет, браузер просит его сам. */
function watchConsole(page: Page): string[] {
  const errors: string[] = [];
  page.on('console', (m) => {
    // Firefox: шрифт, загрузку которого оборвал переход на другой адрес (NS_BINDING_ABORTED), — не потеря.
    const aborted = m.text().includes('status=2152398850');
    if (m.type() === 'error' && !aborted && !m.location().url.endsWith('/favicon.ico')) errors.push(`${m.text()} ${m.location().url}`);
  });
  page.on('pageerror', (e) => errors.push(e.message));
  return errors;
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
  // Страница спрашивает, кому принадлежит ссылка: токен это не тратит (B-193), подтверждения нет.
  await expect(scanner.getByText(email)).toBeVisible();
  expect(asked.filter((u) => new URL(u).pathname === '/api/auth/confirm')).toEqual([]);
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
  // Потраченная ссылка видна сразу, нажимать нечего.
  await other.goto(link);
  await expect(other.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await expect(other.getByRole('button', { name: 'Войти', exact: true })).toHaveCount(0);
  await other.getByRole('button', { name: 'Запросить новую ссылку' }).click();
  await expect(other.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await other.context().close();
});

test('ссылку потратили, пока страница открыта: нажатие — то же сообщение', async ({ page, browser }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  await page.goto(link);
  await expect(page.getByText(email)).toBeVisible();
  const other = await newContext(browser);
  await other.goto(link);
  await other.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(other.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await other.context().close();
});

test('страница подтверждения показывает адрес; вошедшему под другим адресом — выбор, ссылка цела (B-193)', async ({ page }, info) => {
  const mine = newEmail(info.project.name);
  const foreign = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, mine);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.request.post('/api/auth/sign-in/magic-link', { data: { email: foreign }, headers: { origin: STAND_URL } });
  const [link] = await linksFor(foreign);
  await page.goto(link);
  await expect(page.getByRole('status')).toContainText(`Вы уже вошли как ${mine}`);
  await expect(page.getByRole('status')).toContainText(foreign);
  // «Остаться» не тратит токен и оставляет прежний вход.
  await page.getByRole('button', { name: `Остаться как ${mine}` }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  expect(((await (await page.request.get('/api/auth/session')).json()) as { user: { email: string } }).user.email).toBe(mine);
  await page.goto(link);
  await page.getByRole('button', { name: `Войти как ${foreign}` }).click();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  expect(((await (await page.request.get('/api/auth/session')).json()) as { user: { email: string } }).user.email).toBe(foreign);
});

test('вошедшему: пока адрес неизвестен, «Войти» недоступна; сбой чтения адреса — предупреждение и выбор (T10-I1)', async ({
  page,
}, info) => {
  const mine = newEmail(info.project.name);
  const foreign = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, mine);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.request.post('/api/auth/sign-in/magic-link', { data: { email: foreign }, headers: { origin: STAND_URL } });
  const [link] = await linksFor(foreign);
  // Ответ ещё в пути: нажать нельзя, сессия не заменится вслепую.
  const { promise: letGo, resolve } = Promise.withResolvers<void>();
  await page.route('**/api/auth/confirm-info', async (r) => {
    await letGo;
    await r.fulfill({ status: 503, contentType: 'application/json', body: '{"error":"UNAVAILABLE"}' });
  });
  await page.goto(link);
  const enterButton = page.getByRole('button', { name: 'Войти', exact: true });
  await expect(enterButton).toBeDisabled();
  resolve();
  // Сбой (503 или 429): адрес неизвестен, предупреждение всё равно есть, токен цел.
  await expect(page.getByRole('status')).toContainText(`Вы уже вошли как ${mine}`);
  await expect(page.getByRole('status')).toContainText('вход может заменить текущий');
  await expect(page.getByRole('button', { name: `Остаться как ${mine}` })).toBeVisible();
  await expect(enterButton).toBeEnabled();
  await page.unroute('**/api/auth/confirm-info');
  await page.getByRole('button', { name: `Остаться как ${mine}` }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  // Сессия неизвестна: нейтральное предупреждение.
  await page.route('**/api/auth/session', (r) =>
    r.fulfill({ status: 503, contentType: 'application/json', body: '{"error":"UNAVAILABLE"}' }),
  );
  await page.goto(link);
  await expect(page.getByRole('status')).toContainText('Не удалось проверить, вошли ли вы');
  await page.unroute('**/api/auth/session');
  // Токен не потрачен: тот же адрес входит.
  await page.goto('/#/');
  await page.goto(link);
  await page.reload();
  await page.getByRole('button', { name: `Войти как ${foreign}` }).click();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
});

test('«Остаться» отвергает ссылку: `next` не исполняется, другие вкладки не тревожатся (T10-I2)', async ({ page, context }, info) => {
  const mine = newEmail(info.project.name);
  const foreign = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, mine);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.request.post('/api/auth/sign-in/magic-link', { data: { email: foreign }, headers: { origin: STAND_URL } });
  const [link] = await linksFor(foreign);
  const tab = await context.newPage();
  await tab.goto('/');
  await expect(tab.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await tab.evaluate(() => {
    const w = window as unknown as { __auth: string[] };
    w.__auth = [];
    new BroadcastChannel('ferrum-auth').onmessage = (e) => w.__auth.push(String(e.data));
  });
  await page.goto(`${link}&next=${encodeURIComponent('#/new')}`);
  await page.getByRole('button', { name: `Остаться как ${mine}` }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await expect(page).toHaveURL(/#\/$/);
  const list = (await (await page.request.get('/api/files?section=recent&q=&start=0&count=10')).json()) as { total: number };
  expect(list.total).toBe(0);
  // Сообщения одного канала идут по порядку: метка пришла, смены входа перед ней не было.
  await page.evaluate(() => {
    const c = new BroadcastChannel('ferrum-auth');
    c.postMessage('метка');
    c.close();
  });
  await expect.poll(() => tab.evaluate(() => (window as unknown as { __auth: string[] }).__auth)).toEqual(['метка']);
});

test('ссылка на свой же адрес: токен не тратится, «Продолжить»', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await signIn(page, email);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await page.request.post('/api/auth/sign-in/magic-link', { data: { email }, headers: { origin: STAND_URL } });
  const [link] = await linksFor(email, 2);
  await page.goto(link);
  await expect(page.getByText('Вы уже вошли как')).toBeVisible();
  const confirm: string[] = [];
  page.on('request', (r) => {
    if (new URL(r.url()).pathname === '/api/auth/confirm') confirm.push(r.url());
  });
  await page.getByRole('button', { name: 'Продолжить' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  expect(confirm).toEqual([]);
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
  await expect(page.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await expect(page.getByRole('button', { name: 'Войти', exact: true })).toHaveCount(0);
});

test('испорченная ссылка: сообщение сразу, запросов нет', async ({ page }) => {
  await page.goto('/#/auth/confirm?token=short');
  await expect(page.getByRole('alert')).toHaveText('Ссылка недействительна или устарела. Запросите новую');
  await expect(page.getByRole('button', { name: 'Войти', exact: true })).toHaveCount(0);
  // Фокус скриптом на открытии страницы: кольца на заголовке нет.
  const title = page.getByRole('heading', { name: 'Вход в Ferrum' });
  await expect(title).toBeFocused();
  await expect(title).toHaveCSS('outline-style', 'none');
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
  await expect(page.getByLabel('Электронная почта')).toBeVisible();
  // Часы стоят с этого момента: задержка машины не меняет показанные секунды.
  await page.clock.pauseAt(Date.now() + 1000);
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
  // Живое число узлов идёт через общую память и обгоняет событие `modified`, а запись видит правку
  // только по нему: человек не успеет уйти быстрее одного кадра (B-200).
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
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
  await page.getByRole('button', { name: 'Продолжить' }).click();
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
  // Ссылка заранее: письмо ждётся дольше, чем идёт запись журнала, и правка успела бы записаться сама.
  const link = await secondLink(page, email);
  await drawRect(page);
  // Сразу, не дожидаясь журнала: приложение должно записать правку само.
  await pasteHash(page, link);
  await page.getByRole('button', { name: 'Продолжить' }).click();
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
  // Запись на сервер не проходит: и в worker'е файлов, и из любой вкладки контекста.
  await page.context().route('**/api/files/*/content', (r) => r.abort());
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
  // Заголовок получает фокус скриптом, а не с клавиатуры: кольца на нём нет.
  await expect(page.getByRole('heading', { name: 'Проверьте почту' })).toHaveCSS('outline-style', 'none');
  await page.getByRole('button', { name: 'Другой адрес' }).click();
  await expect(page.getByLabel('Электронная почта')).toBeFocused();
});

test('смена адреса на экране входа не сбрасывает «Проверьте почту»', async ({ page }, info) => {
  await page.goto('/');
  await requestLink(page, newEmail(info.project.name));
  await sessionRefresh(page, () => page.evaluate(() => void (location.hash = '#/trash')));
  await expect(page.getByRole('heading', { name: 'Проверьте почту' })).toBeVisible();
  await expect(page.getByRole('button', { name: /Отправить ещё раз через/ })).toBeDisabled();
});

/** Действие меняет адрес, и страница заново спрашивает сессию: дожидаемся, пока она применит ответ. */
async function sessionRefresh(page: Page, act: () => Promise<unknown>): Promise<void> {
  const reply = page.waitForResponse((r) => r.url().endsWith('/api/auth/session'));
  await act();
  await (await reply).finished();
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve)));
}

test('сбой обновления при смене адреса не снимает показанный экран входа', async ({ page }, info) => {
  await page.goto('/');
  await requestLink(page, newEmail(info.project.name));
  await page.route('**/api/auth/session', (r) =>
    r.fulfill({ status: 503, contentType: 'application/json', body: '{"error":"UNAVAILABLE"}' }),
  );
  await sessionRefresh(page, () => page.evaluate(() => void (location.hash = '#/trash')));
  await expect(page.getByRole('heading', { name: 'Проверьте почту' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Не удалось открыть Ferrum' })).toHaveCount(0);
});

test('экран имени: смена адреса и сбой обновления не стирают набранное', async ({ page }, info) => {
  await page.goto('/');
  await signIn(page, newEmail(info.project.name), '');
  await page.getByLabel('Имя').fill('Анна');
  await sessionRefresh(page, () => page.evaluate(() => void (location.hash = '#/trash')));
  await expect(page.getByLabel('Имя')).toHaveValue('Анна');
  await page.route('**/api/auth/session', (r) => r.abort());
  await page.evaluate(() => void (location.hash = '#/drafts'));
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 50)));
  await expect(page.getByLabel('Имя')).toHaveValue('Анна');
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
});

test('запоздавший ответ сессии не снимает страницу подтверждения', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  let release!: () => void;
  const hold = new Promise<void>((resolve) => (release = resolve));
  await page.route('**/api/auth/session', async (r) => {
    await hold;
    await r.continue();
  });
  const reply = page.waitForResponse((r) => r.url().endsWith('/api/auth/session'));
  await page.reload();
  await pasteHash(page, link);
  await expect(enter(page)).toBeVisible();
  release();
  await (await reply).finished();
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve)));
  await expect(enter(page)).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await expect(page.getByLabel('Электронная почта')).toHaveCount(0);
});

test('в письмо уходит адрес на момент отправки, а не на момент загрузки', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/#/drafts');
  await expect(page.getByLabel('Электронная почта')).toBeVisible();
  // Обновление по новому адресу ещё не ответило, а человек уже отправляет форму.
  let release!: () => void;
  const hold = new Promise<void>((resolve) => (release = resolve));
  await page.route('**/api/auth/session', async (r) => {
    await hold;
    await r.continue();
  });
  await page.evaluate(() => void (location.hash = '#/trash'));
  await requestLink(page, email);
  release();
  const [link] = await linksFor(email);
  expect(new URLSearchParams(new URL(link).hash.split('?')[1]).get('next')).toBe('#/trash');
});

test('другая ссылка в той же вкладке: страница подтверждения начинает заново', async ({ page }, info) => {
  await page.goto('/#/auth/confirm?token=short');
  await expect(page.getByRole('alert')).toContainText('Ссылка недействительна');
  const email = newEmail(info.project.name);
  const res = await page.request.post('/api/auth/sign-in/magic-link', { data: { email }, headers: { origin: STAND_URL } });
  expect(res.status()).toBe(200);
  const [link] = await linksFor(email);
  await pasteHash(page, link);
  await enter(page).click();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
});

test('сбой сервера при подтверждении: и «Войти» как повтор, и новая ссылка', async ({ page }, info) => {
  const email = newEmail(info.project.name);
  await page.goto('/');
  await requestLink(page, email);
  const [link] = await linksFor(email);
  await page.goto(link);
  const newLink = page.getByRole('button', { name: 'Запросить новую ссылку' });
  // Срок сервера вышел (503 с кодом) или обработчик упал (500 текстом): для человека одно и то же.
  const replies = [
    { status: 503, contentType: 'application/json', body: '{"error":"UNAVAILABLE"}' },
    { status: 500, contentType: 'text/plain', body: 'Внутренняя ошибка сервера' },
  ];
  for (const reply of replies) {
    await page.route('**/api/auth/confirm', (r) => r.fulfill(reply));
    await enter(page).click();
    await expect(page.getByRole('alert')).toContainText('Сервер недоступен');
    await expect(enter(page)).toBeEnabled();
    await expect(newLink).toBeVisible();
    await page.unroute('**/api/auth/confirm');
  }
  // Повтор: токен здесь цел, вход удаётся.
  await enter(page).click();
  await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
  await expect(newLink).toHaveCount(0);
});

test('сбой сервера при подтверждении: новая ссылка ведёт на экран входа', async ({ page }) => {
  // Сбой чтения адреса входа не блокирует: на экране просто нет адреса.
  await page.route('**/api/auth/confirm-info', (r) =>
    r.fulfill({ status: 503, contentType: 'application/json', body: '{"error":"UNAVAILABLE"}' }),
  );
  await page.goto(`/#/auth/confirm?token=${'a'.repeat(32)}&next=%23%2Ftrash`);
  await page.route('**/api/auth/confirm', (r) => r.fulfill({ status: 500, contentType: 'text/plain', body: 'сбой' }));
  await enter(page).click();
  await page.getByRole('button', { name: 'Запросить новую ссылку' }).click();
  await expect(page.getByLabel('Электронная почта')).toBeVisible();
  await expect(page).toHaveURL(/#\/trash$/);
});

test('выход в другой вкладке: новый файл не записан, редактор остаётся и предлагает копию', async ({ page, context }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
  await page.goto('/?nodes=0');
  await signIn(page, email);
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await editorReady(page);
  const tab = await context.newPage();
  await tab.goto('/');
  await expect(tab.getByRole('button', { name: 'Выйти' })).toBeVisible();
  await drawRect(page);
  // Сразу: у нового файла ещё нет записи, а сессии уже нет, сервер её не примет.
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(page.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  await nodesAre(page, 1);
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Сохранить копию на диск' }).click();
  expect((await download).suggestedFilename()).toMatch(/\.ferrum$/);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  const ask = page.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  await expect(ask).toContainText('ещё ни разу не записан');
  await ask.getByRole('button', { name: 'Уйти' }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
});

test('выход в другой вкладке: записанный файл, правка возвращается из журнала после входа', async ({ page, context }, info) => {
  const email = newEmail(info.project.name);
  await testMode(page);
  await page.goto('/?nodes=0');
  await signIn(page, email);
  await page.locator('header').getByRole('button', { name: 'Новый файл' }).click();
  await editorReady(page);
  const url = page.url();
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  const tab = await context.newPage();
  await tab.goto('/');
  await expect(tab.getByRole('button', { name: 'Выйти' })).toBeVisible();
  await drawRect(page);
  await tab.getByRole('button', { name: 'Выйти' }).click();
  await expect(page.getByRole('alert').getByText('Сессия завершена')).toBeVisible();
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Вход в Ferrum' })).toBeVisible();
  await signIn(page, email, '');
  await expect(page).toHaveURL(url);
  await editorReady(page);
  await nodesAre(page, 2);
});
