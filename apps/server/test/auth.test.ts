import { afterAll, beforeEach, expect, test } from 'bun:test';
import { getMigrations } from 'better-auth/db/migration';
import { join } from 'node:path';
import { Pool } from 'pg';
import { startServer } from '../src/app';
import { AUTH_DEADLINE_MS, authorizeUpgrade } from '../src/auth-routes';
import { authOptions, createAuth, emailAllowed, MAILS_PER_ADDRESS, MAILS_PER_IP, type AuthDeps } from '../src/auth';
import { magicLinkMail, smtpMailer, type Mail } from '../src/mail';
import { migrate } from '../src/migrate';
import { connectValkey } from '../src/valkey';
import { resetDatabase, resetValkey, tcpProxy, TEST_DATABASE_URL, TEST_SECRET, TEST_VALKEY_URL, testPool } from './helpers';

const pool = testPool();
const valkey = await connectValkey(TEST_VALKEY_URL);
const stops: (() => void)[] = [];
let mails: Mail[] = [];

beforeEach(async () => {
  await resetDatabase(pool);
  await resetValkey(valkey);
  await migrate(pool, join(import.meta.dir, '../migrations'));
  mails = [];
});
afterAll(async () => {
  for (const s of stops) s();
  valkey.close();
  await pool.end();
});

/** Настоящий сервер на свободном порту; `PUBLIC_URL` совпадает с его адресом, как на стенде. */
function stand(over: Partial<AuthDeps> = {}, trustedProxies: string[] = []) {
  const server = Bun.serve({ port: 0, fetch: () => new Response() });
  const port = server.port!;
  server.stop(true);
  const publicUrl = `http://localhost:${port}`;
  const deps: AuthDeps = {
    publicUrl,
    secret: TEST_SECRET,
    pool,
    valkey,
    allowedEmails: [],
    mailer: { send: async (m) => void mails.push(m) },
    ...over,
  };
  const auth = createAuth(deps);
  const app = startServer({ pool, valkey, webDist: '/nonexistent', auth, publicUrl, trustedProxies }, port);
  stops.push(() => app.stop(true));
  const post = (path: string, body: unknown, headers: Record<string, string> = {}) =>
    fetch(`${publicUrl}${path}`, {
      method: 'POST',
      headers: { origin: publicUrl, 'content-type': 'application/json', ...headers },
      body: typeof body === 'string' ? body : JSON.stringify(body),
      redirect: 'manual',
    });
  return {
    publicUrl,
    auth,
    post,
    get: (path: string, headers: Record<string, string> = {}) => fetch(`${publicUrl}${path}`, { headers, redirect: 'manual' }),
  };
}
type Stand = ReturnType<typeof stand>;

const cookieOf = (res: Response) =>
  res.headers
    .getSetCookie()
    .map((c) => c.split(';')[0])
    .join('; ');
const tokenOf = (mail: Mail) => new URLSearchParams(mail.text.split('#/auth/confirm?')[1].split('\n')[0]).get('token')!;

/** Письмо уходит без ожидания ответа, поэтому его ждут опросом, а не полагаются на порядок. */
async function waitMails(count: number): Promise<void> {
  for (let i = 0; i < 100 && mails.length < count; i++) await Bun.sleep(20);
  expect(mails.length).toBeGreaterThanOrEqual(count);
}

async function requestLink(s: Stand, email = 'ivan@example.com', next?: string): Promise<string> {
  const before = mails.length;
  const res = await s.post('/api/auth/sign-in/magic-link', { email, next });
  expect(res.status).toBe(200);
  await waitMails(before + 1);
  return tokenOf(mails.at(-1)!);
}
async function signIn(s: Stand, email?: string): Promise<string> {
  const res = await s.post('/api/auth/confirm', { token: await requestLink(s, email) });
  expect(res.status).toBe(200);
  return cookieOf(res);
}
const sessionOf = async (s: Stand, cookie: string) =>
  (await (await s.get('/api/auth/session', { cookie })).json()) as { user: { id: string; email: string; name: string } | null };

test('круг: письмо → подтверждение → сессия в Valkey → проверка сессии', async () => {
  const s = stand();
  const res = await s.post('/api/auth/sign-in/magic-link', { email: 'Ivan@Example.com', next: '#/file/abc' });
  expect(res.status).toBe(200);
  expect(await res.json()).toEqual({ ok: true });
  await waitMails(1);
  expect(mails).toHaveLength(1);
  expect(mails[0].to).toBe('ivan@example.com');
  expect(mails[0].subject).toBe('Вход в Ferrum');
  // Ссылка ведёт на страницу подтверждения, а не на точку, тратящую токен.
  expect(mails[0].text).toContain(`${s.publicUrl}/#/auth/confirm?token=`);
  expect(mails[0].text).toContain('next=%23%2Ffile%2Fabc');
  expect(mails[0].text).not.toContain('magic-link/verify');

  // Открытие ссылки токен не тратит: страница сама ничего не отправляет на сервер.
  const token = tokenOf(mails[0]);
  // Токен в Valkey — только хэшем.
  const keys = (await valkey.send('KEYS', ['verification:*'])) as string[];
  expect(keys).toHaveLength(1);
  expect(keys[0]).not.toContain(token);
  expect(await valkey.get(keys[0])).not.toContain(token);
  expect(await valkey.ttl(keys[0])).toBeGreaterThan(890);
  expect(await valkey.ttl(keys[0])).toBeLessThanOrEqual(900);

  const ok = await s.post('/api/auth/confirm', { token });
  expect(ok.status).toBe(200);
  expect(await ok.json()).toEqual({ ok: true });
  const cookie = ok.headers.getSetCookie().find((c) => c.startsWith('ferrum.session_token='))!;
  expect(cookie).toContain('HttpOnly');
  expect(cookie).toContain('SameSite=Lax');
  expect(cookie).toContain('Max-Age=2592000');
  expect(cookie).not.toContain('Secure');

  const me = await sessionOf(s, cookieOf(ok));
  expect(me.user?.email).toBe('ivan@example.com');
  expect(me.user?.name).toBe('');
  // Пользователь — в Postgres, сессия — в Valkey, таблицы сессий нет.
  expect((await pool.query('SELECT email FROM "user"')).rows).toEqual([{ email: 'ivan@example.com' }]);
  const tables = (await pool.query("SELECT table_name AS t FROM information_schema.tables WHERE table_schema = 'public'")).rows.map(
    (r) => r.t,
  );
  expect(tables.sort()).toEqual(['account', 'schema_migration', 'user']);
  const sessionToken = decodeURIComponent(cookieOf(ok).split('=')[1]).split('.')[0];
  expect(await valkey.get(sessionToken)).toContain('ivan@example.com');
});

test('ссылка одноразовая, в том числе при одновременных нажатиях', async () => {
  const s = stand();
  const token = await requestLink(s);
  const results = await Promise.all(Array.from({ length: 5 }, () => s.post('/api/auth/confirm', { token })));
  expect(results.map((r) => r.status).sort()).toEqual([200, 400, 400, 400, 400]);
  const again = await s.post('/api/auth/confirm', { token });
  expect(again.status).toBe(400);
  expect(await again.json()).toEqual({ error: 'INVALID_TOKEN' });
  expect(again.headers.getSetCookie()).toEqual([]);
});

test('сбой Postgres при подтверждении — ошибка сервера, а не «ссылка недействительна»', async () => {
  // Пул закрывается после выдачи ссылки: к нажатию «Войти» база уже недоступна.
  const dead = new Pool({ connectionString: TEST_DATABASE_URL });
  const s = stand({ pool: dead });
  const token = await requestLink(s);
  await dead.end();
  const original = console.error;
  console.error = () => {};
  try {
    expect((await s.post('/api/auth/confirm', { token })).status).toBe(500);
  } finally {
    console.error = original;
  }
});

test('зависший Valkey: 503 в формате ошибок входа по истечении срока, а не вечное ожидание', async () => {
  const proxy = tcpProxy(Number(new URL(TEST_VALKEY_URL).port));
  const hung = await connectValkey(`redis://127.0.0.1:${proxy.port}`);
  const logged: unknown[][] = [];
  const original = console.error;
  console.error = (...args: unknown[]) => void logged.push(args);
  const publicUrl = 'http://localhost:1';
  const auth = createAuth({ publicUrl, secret: TEST_SECRET, pool, valkey: hung, allowedEmails: [], mailer: { send: async () => {} } });
  const app = startServer({ pool, valkey: hung, webDist: '/nonexistent', auth, publicUrl, trustedProxies: [], authDeadlineMs: 150 }, 0);
  try {
    proxy.freeze();
    const started = Date.now();
    const res = await fetch(`http://127.0.0.1:${app.port}/api/auth/sign-in/magic-link`, {
      method: 'POST',
      headers: { origin: publicUrl, 'content-type': 'application/json' },
      body: JSON.stringify({ email: 'ivan@example.com' }),
    });
    expect(res.status).toBe(503);
    expect(await res.json()).toEqual({ error: 'UNAVAILABLE' });
    expect(res.headers.get('cache-control')).toBe('no-store');
    expect(res.headers.get('x-content-type-options')).toBe('nosniff');
    expect(Date.now() - started).toBeLessThan(1500);
    expect(logged).toEqual([['Запрос входа не уложился в срок:', '/api/auth/sign-in/magic-link']]);
  } finally {
    console.error = original;
    app.stop(true);
    hung.close();
    proxy.stop();
  }
});

test('срок обработчика меньше 15 с, которые ждёт клиент', () => {
  expect(AUTH_DEADLINE_MS).toBe(10_000);
});

test('истёкшая ссылка не входит, срок задаётся в Valkey', async () => {
  const s = stand({ magicLinkSeconds: 1 });
  const token = await requestLink(s);
  await Bun.sleep(1200);
  const res = await s.post('/api/auth/confirm', { token });
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'INVALID_TOKEN' });
});

test('точка Better Auth, тратящая токен по GET, снаружи закрыта', async () => {
  const s = stand();
  const token = await requestLink(s);
  const res = await s.get(`/api/auth/magic-link/verify?token=${token}&callbackURL=/`);
  expect(res.status).toBe(404);
  expect(res.headers.getSetCookie()).toEqual([]);
  // Токен цел.
  expect((await s.post('/api/auth/confirm', { token })).status).toBe(200);
});

test('наружу открыт только список маршрутов', async () => {
  const s = stand();
  for (const [method, path] of [
    ['GET', '/api/auth/get-session'],
    ['GET', '/api/auth/ok'],
    ['GET', '/api/auth/list-sessions'],
    ['POST', '/api/auth/update-user'],
    ['POST', '/api/auth/revoke-sessions'],
    ['POST', '/api/auth/delete-user'],
    ['POST', '/api/auth/sign-up/email'],
    ['GET', '/api/auth/sign-in/magic-link'],
    ['GET', '/api/auth/constructor'],
    ['POST', '/api/auth/constructor'],
  ]) {
    const res = await fetch(`${s.publicUrl}${path}`, { method, headers: { origin: s.publicUrl } });
    expect([path, res.status]).toEqual([path, 404]);
  }
});

test('изменяющие запросы: чужой Origin и его отсутствие — 403, без последствий', async () => {
  const s = stand();
  for (const headers of [{ origin: 'https://evil.example' }, { origin: 'null' }]) {
    expect((await s.post('/api/auth/sign-in/magic-link', { email: 'a@b.ru' }, headers)).status).toBe(403);
  }
  const bare = await fetch(`${s.publicUrl}/api/auth/sign-in/magic-link`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: '{"email":"a@b.ru"}',
  });
  expect(bare.status).toBe(403);
  expect(await bare.json()).toEqual({ error: 'BAD_ORIGIN' });
  expect(mails).toEqual([]);
  const token = await requestLink(s);
  expect((await s.post('/api/auth/confirm', { token }, { origin: 'https://evil.example' })).status).toBe(403);
  expect((await s.post('/api/auth/confirm', { token })).status).toBe(200);
});

test('неверные данные — 400: схема из protocol, лишние поля не доходят до Better Auth', async () => {
  const s = stand();
  for (const body of [
    'не json',
    '{}',
    '{"email":"нет"}',
    '{"email":"a@b.ru","next":"https://evil.example"}',
    // Проходит разбор протокола, но не `z.email()` Better Auth: 400, а не сбой сервера.
    '{"email":"иван@почта.рф"}',
    `{"email":"a@b.ru","pad":"${'x'.repeat(5000)}"}`,
  ]) {
    const res = await s.post('/api/auth/sign-in/magic-link', body);
    expect([body.slice(0, 40), res.status]).toEqual([body.slice(0, 40), 400]);
    expect(await res.json()).toEqual({ error: 'BAD_REQUEST' });
  }
  expect((await s.post('/api/auth/confirm', { token: 'short' })).status).toBe(400);
  expect(mails).toEqual([]);
  await s.post('/api/auth/sign-in/magic-link', { email: 'a@b.ru', callbackURL: 'https://evil.example', name: 'Чужое имя' });
  await waitMails(1);
  const cookie = cookieOf(await s.post('/api/auth/confirm', { token: tokenOf(mails[0]) }));
  expect((await sessionOf(s, cookie)).user?.name).toBe('');
});

test('ответы не раскрывают токен сессии и то, есть ли такой адрес', async () => {
  const s = stand();
  const cookie = await signIn(s, 'known@example.com');
  const session = await (await s.get('/api/auth/session', { cookie })).text();
  expect(session).not.toContain('token');
  const known = await s.post('/api/auth/sign-in/magic-link', { email: 'known@example.com' });
  const unknown = await s.post('/api/auth/sign-in/magic-link', { email: 'unknown@example.com' });
  expect(known.status).toBe(unknown.status);
  expect(await known.text()).toBe(await unknown.text());
});

test('ALLOWED_EMAILS: адрес и домен; остальным ответ тот же, письма нет', async () => {
  expect(emailAllowed('a@b.ru', [])).toBe(true);
  expect(emailAllowed('a@b.ru', ['a@b.ru'])).toBe(true);
  expect(emailAllowed('c@b.ru', ['b.ru'])).toBe(true);
  expect(emailAllowed('c@notb.ru', ['b.ru'])).toBe(false);
  const s = stand({ allowedEmails: ['ivan@example.com', 'team.org'] });
  const denied = await s.post('/api/auth/sign-in/magic-link', { email: 'petr@example.com' });
  expect(denied.status).toBe(200);
  expect(await denied.json()).toEqual({ ok: true });
  expect(mails).toEqual([]);
  await s.post('/api/auth/sign-in/magic-link', { email: 'ivan@example.com' });
  await s.post('/api/auth/sign-in/magic-link', { email: 'any@team.org' });
  await waitMails(2);
  expect(mails.map((m) => m.to)).toEqual(['ivan@example.com', 'any@team.org']);
});

test('ALLOWED_EMAILS: шесть запросов дают 429 на шестом и для адреса вне списка, и для адреса из списка', async () => {
  const s = stand({ allowedEmails: ['ivan@example.com'] });
  for (const email of ['petr@example.com', 'ivan@example.com']) {
    const codes: number[] = [];
    for (let i = 0; i <= MAILS_PER_ADDRESS; i++) codes.push((await s.post('/api/auth/sign-in/magic-link', { email })).status);
    expect([email, codes]).toEqual([email, [200, 200, 200, 200, 200, 429]]);
  }
  await waitMails(MAILS_PER_ADDRESS);
  expect(mails.every((m) => m.to === 'ivan@example.com')).toBe(true);
});

test('сбой отправки письма не раскрывается ответом и не роняет сервер, в журнал — без ссылки', async () => {
  const logged: unknown[][] = [];
  const original = console.error;
  console.error = (...args: unknown[]) => void logged.push(args);
  try {
    const s = stand({
      mailer: {
        send: async () => {
          throw new Error('SMTP недоступен');
        },
      },
    });
    const res = await s.post('/api/auth/sign-in/magic-link', { email: 'ivan@example.com' });
    expect(res.status).toBe(200);
    for (let i = 0; i < 100 && logged.length === 0; i++) await Bun.sleep(20);
  } finally {
    console.error = original;
  }
  expect(logged).toEqual([['Не удалось отправить письмо входа:', 'SMTP недоступен']]);
});

test('ограничение: не больше 5 писем на адрес в час, другому адресу можно, счётчик истекает', async () => {
  const s = stand();
  const send = (email: string) => s.post('/api/auth/sign-in/magic-link', { email });
  for (let i = 0; i < MAILS_PER_ADDRESS; i++) expect((await send('ivan@example.com')).status).toBe(200);
  const over = await send('IVAN@example.com');
  expect(over.status).toBe(429);
  expect(await over.json()).toEqual({ error: 'TOO_MANY_REQUESTS' });
  expect(mails).toHaveLength(MAILS_PER_ADDRESS);
  expect((await send('petr@example.com')).status).toBe(200);
  const ttl = await valkey.ttl('mail-to:ivan@example.com');
  expect(ttl).toBeGreaterThan(3500);
  expect(ttl).toBeLessThanOrEqual(3600);
});

test('ограничение: не больше 20 писем на IP в час, чужой X-Forwarded-For не помогает', async () => {
  const s = stand();
  for (let i = 0; i < MAILS_PER_IP; i++) {
    const res = await s.post(
      '/api/auth/sign-in/magic-link',
      { email: `u${i}@example.com` },
      { 'x-forwarded-for': `10.0.0.${i}`, 'x-ferrum-client-ip': `10.0.1.${i}` },
    );
    expect(res.status).toBe(200);
  }
  const over = await s.post('/api/auth/sign-in/magic-link', { email: 'last@example.com' }, { 'x-forwarded-for': '10.9.9.9' });
  expect(over.status).toBe(429);
  expect(over.headers.get('retry-after')).toBe('3600');
  expect(mails).toHaveLength(MAILS_PER_IP);
});

/** Этот тест ходит на `localhost`: соединение придёт с одного из этих адресов. */
const LOOPBACK = ['127.0.0.1', '::1'];

test('адрес не из TRUSTED_PROXIES: X-Forwarded-For не влияет на счётчик', async () => {
  const s = stand({}, ['192.0.2.1']);
  for (let i = 0; i < MAILS_PER_IP; i++) {
    const res = await s.post('/api/auth/sign-in/magic-link', { email: `u${i}@example.com` }, { 'x-forwarded-for': `1.1.1.${i}` });
    expect(res.status).toBe(200);
  }
  const over = await s.post('/api/auth/sign-in/magic-link', { email: 'x@example.com' }, { 'x-forwarded-for': '1.1.1.200' });
  expect(over.status).toBe(429);
});

test('цепочка из двух прокси: у каждого клиента свой счётчик, подмена слева не помогает', async () => {
  const s = stand({}, [...LOOPBACK, '10.0.0.0/8']);
  const send = (i: number, xff: string) =>
    s.post('/api/auth/sign-in/magic-link', { email: `u${i}@example.com` }, { 'x-forwarded-for': xff });
  // Клиент каждый раз подставляет новый адрес слева: настоящий у него один.
  for (let i = 0; i < MAILS_PER_IP; i++) expect((await send(i, `6.6.6.${i}, 1.1.1.1, 10.0.0.1`)).status).toBe(200);
  expect((await send(100, '6.6.6.200, 1.1.1.1, 10.0.0.2')).status).toBe(429);
  expect((await send(101, '1.1.1.2, 10.0.0.1')).status).toBe(200);
});

test('за своим прокси адрес клиента — первый справа не из списка', async () => {
  const s = stand({}, LOOPBACK);
  for (let i = 0; i < MAILS_PER_IP; i++) {
    expect(
      (await s.post('/api/auth/sign-in/magic-link', { email: `u${i}@example.com` }, { 'x-forwarded-for': `1.1.1.${i}, 10.0.0.1` })).status,
    ).toBe(200);
  }
  expect((await s.post('/api/auth/sign-in/magic-link', { email: 'x@example.com' }, { 'x-forwarded-for': '10.0.0.1' })).status).toBe(429);
  expect((await s.post('/api/auth/sign-in/magic-link', { email: 'y@example.com' }, { 'x-forwarded-for': '10.0.0.2' })).status).toBe(200);
});

test('проверка сессии без предела: общий IP не закрывает приложение вошедшим', async () => {
  const s = stand();
  const cookie = await signIn(s);
  const replies = await Promise.all(Array.from({ length: 150 }, () => s.get('/api/auth/session', { cookie })));
  expect(new Set(replies.map((r) => r.status))).toEqual(new Set([200]));
});

test('подтверждение: предел остаётся, 30 запросов в минуту', async () => {
  const s = stand();
  for (let i = 0; i < 30; i++) expect((await s.post('/api/auth/confirm', { token: 'a'.repeat(32) })).status).toBe(400);
  expect((await s.post('/api/auth/confirm', { token: 'a'.repeat(32) })).status).toBe(429);
});

test('имя с первого входа сохраняется и видно в сессии; без сессии — 401', async () => {
  const s = stand();
  expect((await s.post('/api/auth/name', { name: 'Иван' })).status).toBe(401);
  const cookie = await signIn(s);
  expect((await s.post('/api/auth/name', { name: '   ' }, { cookie })).status).toBe(400);
  const res = await s.post('/api/auth/name', { name: '  Иван  ' }, { cookie });
  expect(res.status).toBe(200);
  expect(((await res.json()) as { user: { name: string } }).user.name).toBe('Иван');
  expect((await sessionOf(s, cookie)).user?.name).toBe('Иван');
  expect((await pool.query('SELECT name FROM "user"')).rows).toEqual([{ name: 'Иван' }]);
});

test('повторный вход тем же адресом — тот же пользователь', async () => {
  const s = stand();
  const a = await signIn(s, 'ivan@example.com');
  const b = await signIn(s, 'IVAN@example.com');
  expect((await sessionOf(s, a)).user?.id).toBe((await sessionOf(s, b)).user!.id);
  expect((await pool.query('SELECT count(*)::int AS n FROM "user"')).rows[0].n).toBe(1);
});

test('выход на этом устройстве: другая сессия жива, cookie снята', async () => {
  const s = stand();
  const a = await signIn(s);
  const b = await signIn(s);
  const out = await s.post('/api/auth/sign-out', {}, { cookie: a });
  expect(out.status).toBe(200);
  expect(out.headers.getSetCookie().join()).toContain('Max-Age=0');
  expect((await sessionOf(s, a)).user).toBeNull();
  expect((await sessionOf(s, b)).user).not.toBeNull();
});

test('выход без сессии не падает: cookie снимается, ответ тот же', async () => {
  const s = stand();
  const out = await s.post('/api/auth/sign-out', {});
  expect(out.status).toBe(200);
  expect(await out.json()).toEqual({ ok: true });
});

test('выход на всех устройствах: все сессии человека сняты, чужие целы', async () => {
  const s = stand();
  const a = await signIn(s);
  const b = await signIn(s);
  const other = await signIn(s, 'petr@example.com');
  const out = await s.post('/api/auth/sign-out-everywhere', {}, { cookie: a });
  expect(out.status).toBe(200);
  expect(out.headers.getSetCookie().join()).toContain('Max-Age=0');
  expect((await sessionOf(s, a)).user).toBeNull();
  expect((await sessionOf(s, b)).user).toBeNull();
  expect((await sessionOf(s, other)).user?.email).toBe('petr@example.com');
  expect((await s.post('/api/auth/sign-out-everywhere', {})).status).toBe(401);
});

test('сессия продлевается: после updateAge новый срок и в cookie, и в Valkey', async () => {
  const s = stand({ sessionUpdateAgeSeconds: 1 });
  const cookie = await signIn(s);
  const token = decodeURIComponent(cookie.split('=')[1]).split('.')[0];
  await valkey.send('EXPIRE', [token, '1000']);
  await Bun.sleep(1100);
  const res = await s.get('/api/auth/session', { cookie });
  expect(res.headers.getSetCookie().find((c) => c.startsWith('ferrum.session_token='))).toContain('Max-Age=2592000');
  expect(await valkey.ttl(token)).toBeGreaterThan(2_590_000);
});

test('Valkey недоступен: сбой, а не «сессии нет», человека не выбрасывает на экран входа', async () => {
  const dead = await connectValkey(TEST_VALKEY_URL);
  const s = stand({ valkey: dead });
  const cookie = await signIn(s);
  dead.close();
  const res = await s.get('/api/auth/session', { cookie });
  expect(res.status).toBe(500);
});

test('Secure у cookie — при HTTPS', () => {
  const o = authOptions({
    publicUrl: 'https://ferrum.example',
    secret: TEST_SECRET,
    pool,
    valkey,
    mailer: { send: async () => {} },
    allowedEmails: [],
  });
  expect(o.advanced.useSecureCookies).toBe(true);
  const h = authOptions({
    publicUrl: 'http://localhost:8090',
    secret: TEST_SECRET,
    pool,
    valkey,
    mailer: { send: async () => {} },
    allowedEmails: [],
  });
  expect(h.advanced.useSecureCookies).toBe(false);
});

test('проверка перед WebSocket: настоящий upgrade с сессией, без неё, с чужим Origin, после выхода', async () => {
  const s = stand();
  const cookie = await signIn(s);
  const opened: string[] = [];
  const ws = Bun.serve<{ userId: string }>({
    port: 0,
    fetch: async (req, server) => {
      const who = await authorizeUpgrade(req, s.auth, s.publicUrl);
      if (!who) return new Response('Нет доступа', { status: 401 });
      if (server.upgrade(req, { data: { userId: who.userId } })) return undefined;
      return new Response('Не WebSocket', { status: 400 });
    },
    websocket: {
      open: (socket) => void opened.push(socket.data.userId),
      message: () => {},
    },
  });
  stops.push(() => ws.stop(true));
  const open = (headers: Record<string, string>) =>
    new Promise<string>((resolve) => {
      const c = new WebSocket(`ws://127.0.0.1:${ws.port}/`, { headers } as never);
      c.onopen = () => {
        c.close();
        resolve('open');
      };
      c.onerror = () => resolve('refused');
    });
  const me = (await sessionOf(s, cookie)).user!;
  expect(await open({ cookie, origin: s.publicUrl })).toBe('open');
  expect(opened).toEqual([me.id]);
  expect(await open({ origin: s.publicUrl })).toBe('refused');
  expect(await open({ cookie, origin: 'https://evil.example' })).toBe('refused');
  expect(await open({ cookie: 'ferrum.session_token=forged.signature', origin: s.publicUrl })).toBe('refused');
  await s.post('/api/auth/sign-out', {}, { cookie });
  expect(await open({ cookie, origin: s.publicUrl })).toBe('refused');
  expect(opened).toEqual([me.id]);
});

test('миграция совпадает со схемой, которую ждёт Better Auth этой версии', async () => {
  const { toBeCreated, toBeAdded } = await getMigrations(
    authOptions({
      publicUrl: 'http://localhost:1',
      secret: TEST_SECRET,
      pool,
      valkey,
      mailer: { send: async () => {} },
      allowedEmails: [],
    }),
  );
  expect(toBeCreated).toEqual([]);
  expect(toBeAdded).toEqual([]);
});

test('настоящее письмо через SMTP в Mailpit', async () => {
  const api = 'http://127.0.0.1:18025/api/v1';
  await fetch(`${api}/messages`, { method: 'DELETE' });
  const mailer = smtpMailer('smtp://127.0.0.1:11025', 'Ferrum <ferrum@localhost>');
  const link = 'http://localhost:8090/#/auth/confirm?token=abc&next=%23%2Ffile%2F1';
  await mailer.send(magicLinkMail('ivan@example.com', link));
  const list = (await (await fetch(`${api}/messages`)).json()) as {
    messages: { ID: string; Subject: string; To: { Address: string }[]; From: { Address: string; Name: string } }[];
  };
  expect(list.messages).toHaveLength(1);
  expect(list.messages[0].Subject).toBe('Вход в Ferrum');
  expect(list.messages[0].To[0].Address).toBe('ivan@example.com');
  expect(list.messages[0].From).toMatchObject({ Address: 'ferrum@localhost', Name: 'Ferrum' });
  const full = (await (await fetch(`${api}/message/${list.messages[0].ID}`)).json()) as { Text: string; HTML: string };
  expect(full.Text).toContain(link);
  expect(full.HTML).toContain(link.replaceAll('&', '&amp;'));
});
