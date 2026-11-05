// Маршруты входа. Наружу открыт только этот список: остальные точки Better Auth (их десятки,
// в том числе GET, тратящий токен ссылки) недоступны снаружи. Всё, что меняет состояние,
// проходит проверку `Origin`; входные данные — разбор из `packages/protocol`.

import {
  API_CONFIRM,
  API_CONFIRM_INFO,
  API_NAME,
  API_SESSION,
  API_SIGN_IN,
  API_SIGN_OUT,
  API_SIGN_OUT_EVERYWHERE,
  parseConfirmRequest,
  parseNameRequest,
  parseSignInRequest,
  type AuthErrorBody,
  type AuthErrorCode,
  type ConfirmInfo,
  type SessionResponse,
} from '@ferrum/protocol';
import { createHash } from 'node:crypto';
import { CLIENT_IP_HEADER, type Auth } from './auth';
import { clientIp, proxyMatcher } from './client-ip';

/** Запросы входа маленькие: всё, что больше, отвергается, не дочитываясь. */
const MAX_BODY_BYTES = 4096;
/** Срок на обработчик: зависшая база или Valkey дают честный 503. Меньше 15 с, которые ждёт клиент
 * (`REQUEST_TIMEOUT_MS`), иначе человек видел бы сбой сети, а не ответ сервера. */
export const AUTH_DEADLINE_MS = 10_000;

/** Как у подтверждения (`/magic-link/verify`): 30 в минуту с адреса. */
const CONFIRM_INFO_PER_MINUTE = 30;

function readEmail(value: string | undefined): string | null {
  if (!value) return null;
  try {
    const email = (JSON.parse(value) as { email?: unknown }).email;
    return typeof email === 'string' ? email : null;
  } catch {
    return null;
  }
}

export function originAllowed(req: Request, publicUrl: string): boolean {
  return req.headers.get('origin') === publicUrl;
}

function fail(status: number, error: AuthErrorCode): Response {
  return Response.json({ error } satisfies AuthErrorBody, { status });
}

async function readJson(req: Request): Promise<unknown> {
  if (!req.body) return null;
  const chunks: Uint8Array[] = [];
  let size = 0;
  for await (const chunk of req.body) {
    size += chunk.length;
    if (size > MAX_BODY_BYTES) return null;
    chunks.push(chunk);
  }
  try {
    return JSON.parse(new TextDecoder().decode(Buffer.concat(chunks)));
  } catch {
    return null;
  }
}

/** Работа не отменяется, но ответ уходит вовремя. Что с ней стало дальше, уже никому не сообщить. */
function withinDeadline(work: Promise<Response>, ms: number, path: string): Promise<Response> {
  let timer: Timer;
  const expired = new Promise<Response>((resolve) => {
    timer = setTimeout(() => {
      console.error('Запрос входа не уложился в срок:', path);
      const res = fail(503, 'UNAVAILABLE');
      res.headers.set('Cache-Control', 'no-store');
      resolve(res);
    }, ms);
  });
  return Promise.race([work, expired]).finally(() => clearTimeout(timer));
}

export interface AuthRoutesDeps {
  auth: Auth;
  publicUrl: string;
  /** Адреса обратных прокси, см. `client-ip.ts`. */
  trustedProxies: readonly string[];
  /** Тесты сокращают срок; по умолчанию — `AUTH_DEADLINE_MS`. */
  authDeadlineMs?: number;
}

export function authRoutes(deps: AuthRoutesDeps) {
  const { auth, publicUrl } = deps;
  const trusted = proxyMatcher(deps.trustedProxies);

  /** Запрос к Better Auth от имени клиента: только нужные заголовки и адрес, поставленный сервером. */
  function inner(req: Request, ip: string, path: string, init: { method: string; body?: unknown }): Promise<Response> {
    const headers = new Headers({ [CLIENT_IP_HEADER]: ip, origin: publicUrl });
    for (const name of ['cookie', 'user-agent']) {
      const v = req.headers.get(name);
      if (v) headers.set(name, v);
    }
    if (init.body !== undefined) headers.set('content-type', 'application/json');
    return auth.handler(
      new Request(`${publicUrl}/api/auth${path}`, {
        method: init.method,
        headers,
        body: init.body === undefined ? undefined : JSON.stringify(init.body),
        redirect: 'manual',
      }),
    );
  }

  /** Ответ для браузера: своё тело и cookie из ответа Better Auth. Тело Better Auth наружу
   * не идёт: в нём токен сессии, который должен оставаться только в cookie `HttpOnly`. */
  function respond(from: Response[], body: unknown, status = 200): Response {
    const res = Response.json(body, { status, headers: { 'Cache-Control': 'no-store' } });
    for (const r of from) for (const c of r.headers.getSetCookie()) res.headers.append('Set-Cookie', c);
    return res;
  }

  function limited(r: Response): Response {
    const res = fail(429, 'TOO_MANY_REQUESTS');
    const retry = r.headers.get('x-retry-after');
    if (retry) res.headers.set('Retry-After', retry);
    return res;
  }

  async function session(req: Request, ip: string): Promise<Response> {
    const r = await inner(req, ip, '/get-session', { method: 'GET' });
    if (r.status === 429) return limited(r);
    // Сбой Valkey — не «сессии нет»: иначе человека молча выбросило бы на экран входа.
    if (!r.ok) throw new Error(`Better Auth: чтение сессии вернуло ${r.status}`);
    const data = (await r.json()) as { user?: { id: string; email: string; name: string } } | null;
    const user = data?.user ? { id: data.user.id, email: data.user.email, name: data.user.name } : null;
    return respond([r], { user } satisfies SessionResponse);
  }

  async function signIn(req: Request, ip: string): Promise<Response> {
    const parsed = parseSignInRequest(await readJson(req));
    if (!parsed) return fail(400, 'BAD_REQUEST');
    const r = await inner(req, ip, '/sign-in/magic-link', {
      method: 'POST',
      body: { email: parsed.email, metadata: parsed.next ? { next: parsed.next } : undefined },
    });
    if (r.status === 429) return limited(r);
    // Адрес прошёл разбор протокола, но не схему Better Auth (`иван@почта.рф`): это 400 клиенту, не сбой.
    if (r.status === 400) return fail(400, 'BAD_REQUEST');
    if (!r.ok) throw new Error(`Better Auth: отправка ссылки вернула ${r.status}: ${await r.text()}`);
    return respond([r], { ok: true });
  }

  /** Куда ведёт ссылка (B-193): запись токена читается без траты. Иначе чужая ссылка, нажатая по
   * неосторожности, молча вошла бы под чужим адресом, и работа попала бы в чужой аккаунт. Предел — как у
   * подтверждения: перебирать токены здесь не легче. */
  async function confirmInfo(req: Request, ip: string): Promise<Response> {
    const parsed = parseConfirmRequest(await readJson(req));
    if (!parsed) return fail(400, 'BAD_REQUEST');
    const context = await auth.$context;
    const used = await context.secondaryStorage!.increment!(`confirm-info:${ip}`, 60);
    if (used > CONFIRM_INFO_PER_MINUTE) return fail(429, 'TOO_MANY_REQUESTS');
    const hashed = createHash('sha256').update(parsed.token).digest('base64url');
    const row = await context.internalAdapter.findVerificationValue(`magic-link:${hashed}`);
    const email = readEmail(row?.value);
    return email ? respond([], { email } satisfies ConfirmInfo) : fail(400, 'INVALID_TOKEN');
  }

  /** Токен тратится только здесь, по нажатию «Войти»: сканер почты, открывший ссылку, увидит
   * страницу подтверждения, а POST с верным `Origin` он не пришлёт. */
  async function confirm(req: Request, ip: string): Promise<Response> {
    const parsed = parseConfirmRequest(await readJson(req));
    if (!parsed) return fail(400, 'BAD_REQUEST');
    const r = await inner(req, ip, `/magic-link/verify?token=${encodeURIComponent(parsed.token)}`, { method: 'GET' });
    if (r.status === 429) return limited(r);
    // Сбой сервера (Postgres) — не «ссылка недействительна»: о неверном токене Better Auth сообщает
    // не ошибкой сервера. Токен при этом уже мог быть потрачен: Better Auth снимает его до обращения
    // к базе, поэтому клиент при 5xx предлагает и повтор, и новую ссылку.
    if (r.status >= 500) throw new Error(`Better Auth: подтверждение вернуло ${r.status}`);
    if (!r.ok) return fail(400, 'INVALID_TOKEN');
    await dropPreviousSession(req, ip);
    return respond([r], { ok: true });
  }

  /** Вход поверх прежнего входа в этом браузере: cookie заменится, и снять прежнюю сессию из
   * интерфейса будет уже нельзя. Её cookie из ответа в браузер не идёт. Сбой входу не мешает. */
  async function dropPreviousSession(req: Request, ip: string): Promise<void> {
    if (!req.headers.get('cookie')) return;
    const out = await inner(req, ip, '/sign-out', { method: 'POST', body: {} }).catch(() => null);
    if (!out?.ok) console.error('Не удалось снять прежнюю сессию при входе:', out?.status ?? 'сбой запроса');
  }

  async function name(req: Request, ip: string): Promise<Response> {
    const parsed = parseNameRequest(await readJson(req));
    if (!parsed) return fail(400, 'BAD_REQUEST');
    const r = await inner(req, ip, '/update-user', { method: 'POST', body: { name: parsed.name } });
    if (r.status === 429) return limited(r);
    if (r.status === 401) return fail(401, 'UNAUTHORIZED');
    if (!r.ok) throw new Error(`Better Auth: смена имени вернула ${r.status}: ${await r.text()}`);
    return session(req, ip);
  }

  async function signOut(req: Request, ip: string): Promise<Response> {
    const r = await inner(req, ip, '/sign-out', { method: 'POST', body: {} });
    if (r.status === 429) return limited(r);
    if (!r.ok) throw new Error(`Better Auth: выход вернул ${r.status}`);
    return respond([r], { ok: true });
  }

  /** Все устройства: сессии снимает `/revoke-sessions`, cookie этого браузера убирает `/sign-out`. */
  async function signOutEverywhere(req: Request, ip: string): Promise<Response> {
    const revoked = await inner(req, ip, '/revoke-sessions', { method: 'POST', body: {} });
    if (revoked.status === 429) return limited(revoked);
    if (revoked.status === 401) return fail(401, 'UNAUTHORIZED');
    if (!revoked.ok) throw new Error(`Better Auth: снятие сессий вернуло ${revoked.status}: ${await revoked.text()}`);
    const out = await inner(req, ip, '/sign-out', { method: 'POST', body: {} });
    if (!out.ok) throw new Error(`Better Auth: выход вернул ${out.status}`);
    return respond([revoked, out], { ok: true });
  }

  async function route(req: Request, path: string, peer: string | undefined): Promise<Response> {
    const ip = clientIp(req, peer, trusted);
    if (req.method === 'GET' && path === API_SESSION) return session(req, ip);
    const post: Record<string, (req: Request, ip: string) => Promise<Response>> = {
      [API_SIGN_IN]: signIn,
      [API_CONFIRM]: confirm,
      [API_CONFIRM_INFO]: confirmInfo,
      [API_NAME]: name,
      [API_SIGN_OUT]: signOut,
      [API_SIGN_OUT_EVERYWHERE]: signOutEverywhere,
    };
    const handler = req.method === 'POST' && Object.hasOwn(post, path) ? post[path] : null;
    if (!handler) return fail(404, 'NOT_FOUND');
    if (!originAllowed(req, publicUrl)) return fail(403, 'BAD_ORIGIN');
    return handler(req, ip);
  }

  return function handle(req: Request, peer: string | undefined): Promise<Response> {
    const path = new URL(req.url).pathname;
    return withinDeadline(route(req, path, peer), deps.authDeadlineMs ?? AUTH_DEADLINE_MS, path);
  };
}

export interface UpgradeSession {
  userId: string;
  sessionId: string;
}

/** Проверка перед открытием WebSocket (спека, разделы 5 и 7.1): верный `Origin` и живая сессия
 * в Valkey. Сам WebSocket синхронизации — этап 3. */
export async function authorizeUpgrade(req: Request, auth: Auth, publicUrl: string): Promise<UpgradeSession | null> {
  if (!originAllowed(req, publicUrl)) return null;
  const s = await auth.api.getSession({ headers: req.headers, query: { disableRefresh: true } });
  return s ? { userId: s.user.id, sessionId: s.session.id } : null;
}
