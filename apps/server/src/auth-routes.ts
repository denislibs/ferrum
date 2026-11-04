// Маршруты входа. Наружу открыт только этот список: остальные точки Better Auth (их десятки,
// в том числе GET, тратящий токен ссылки) недоступны снаружи. Всё, что меняет состояние,
// проходит проверку `Origin`; входные данные — разбор из `packages/protocol`.

import {
  API_CONFIRM,
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
  type SessionResponse,
} from '@ferrum/protocol';
import { CLIENT_IP_HEADER, type Auth } from './auth';

/** Запросы входа маленькие: всё, что больше, отвергается, не дочитываясь. */
const MAX_BODY_BYTES = 4096;

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

/** Адрес клиента для счётчиков частоты. Чужому заголовку верим только за своим прокси:
 * последний адрес в `X-Forwarded-For` дописал он, остальные мог прислать кто угодно. */
export function clientIp(req: Request, peer: string | undefined, trustProxy: boolean): string {
  if (trustProxy) {
    const last = req.headers.get('x-forwarded-for')?.split(',').at(-1)?.trim();
    if (last) return last;
  }
  return peer ?? 'unknown';
}

export interface AuthRoutesDeps {
  auth: Auth;
  publicUrl: string;
  trustProxy: boolean;
}

export function authRoutes(deps: AuthRoutesDeps) {
  const { auth, publicUrl } = deps;

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
    if (!r.ok) throw new Error(`Better Auth: отправка ссылки вернула ${r.status}: ${await r.text()}`);
    return respond([r], { ok: true });
  }

  /** Токен тратится только здесь, по нажатию «Войти»: сканер почты, открывший ссылку, увидит
   * страницу подтверждения, а POST с верным `Origin` он не пришлёт. */
  async function confirm(req: Request, ip: string): Promise<Response> {
    const parsed = parseConfirmRequest(await readJson(req));
    if (!parsed) return fail(400, 'BAD_REQUEST');
    const r = await inner(req, ip, `/magic-link/verify?token=${encodeURIComponent(parsed.token)}`, { method: 'GET' });
    if (r.status === 429) return limited(r);
    // Better Auth сообщает о неверном токене перенаправлением с `?error=`.
    if (!r.ok) return fail(400, 'INVALID_TOKEN');
    return respond([r], { ok: true });
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
    return respond([r], { ok: true });
  }

  /** Все устройства: сессии снимает `/revoke-sessions`, cookie этого браузера убирает `/sign-out`. */
  async function signOutEverywhere(req: Request, ip: string): Promise<Response> {
    const revoked = await inner(req, ip, '/revoke-sessions', { method: 'POST', body: {} });
    if (revoked.status === 429) return limited(revoked);
    if (revoked.status === 401) return fail(401, 'UNAUTHORIZED');
    if (!revoked.ok) throw new Error(`Better Auth: снятие сессий вернуло ${revoked.status}: ${await revoked.text()}`);
    const out = await inner(req, ip, '/sign-out', { method: 'POST', body: {} });
    return respond([revoked, out], { ok: true });
  }

  return async function handle(req: Request, peer: string | undefined): Promise<Response> {
    const ip = clientIp(req, peer, deps.trustProxy);
    const path = new URL(req.url).pathname;
    if (req.method === 'GET' && path === API_SESSION) return session(req, ip);
    const post: Record<string, (req: Request, ip: string) => Promise<Response>> = {
      [API_SIGN_IN]: signIn,
      [API_CONFIRM]: confirm,
      [API_NAME]: name,
      [API_SIGN_OUT]: signOut,
      [API_SIGN_OUT_EVERYWHERE]: signOutEverywhere,
    };
    const route = req.method === 'POST' && Object.hasOwn(post, path) ? post[path] : null;
    if (!route) return fail(404, 'NOT_FOUND');
    if (!originAllowed(req, publicUrl)) return fail(403, 'BAD_ORIGIN');
    return route(req, ip);
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
