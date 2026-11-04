// Запросы и ответы входа. Разбор не доверяет данным: неверное — `null`, сервер отвечает
// 400, ничего не передавая дальше.

export const API_SIGN_IN = '/api/auth/sign-in/magic-link';
export const API_CONFIRM = '/api/auth/confirm';
export const API_SESSION = '/api/auth/get-session';
export const API_UPDATE_USER = '/api/auth/update-user';
export const API_SIGN_OUT = '/api/auth/sign-out';
export const API_SIGN_OUT_EVERYWHERE = '/api/auth/revoke-sessions';

/** Адрес не длиннее, чем позволяет RFC 5321. */
export const MAX_EMAIL_BYTES = 254;
export const MAX_NAME_CHARS = 100;
/** Запрошенный адрес приложения, куда вернуться после входа. */
export const MAX_NEXT_CHARS = 512;
/** Срок ссылки из письма, спека, раздел 5. */
export const MAGIC_LINK_TTL_SECONDS = 15 * 60;
/** Повторная отправка со страницы входа не раньше этого срока. */
export const RESEND_SECONDS = 60;
/** Токен ссылки: столько букв выдаёт плагин `magicLink`. */
const TOKEN_RE = /^[A-Za-z]{32}$/;
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
// eslint-disable-next-line no-control-regex
const CONTROL_RE = /[\u0000-\u001f\u007f]/;

export type AuthErrorCode = 'BAD_REQUEST' | 'BAD_ORIGIN' | 'INVALID_TOKEN' | 'TOO_MANY_REQUESTS' | 'UNAUTHORIZED';

export interface AuthErrorBody {
  error: AuthErrorCode;
}

export interface SignInRequest {
  email: string;
  /** Маршрут приложения после `#`, куда открыть после входа. */
  next?: string;
}

export interface ConfirmRequest {
  token: string;
}

export interface NameRequest {
  name: string;
}

export interface SessionUser {
  id: string;
  email: string;
  name: string;
}

function record(v: unknown): Record<string, unknown> | null {
  return typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
}

/** Адрес приводится к нижнему регистру: `Ivan@x.ru` и `ivan@x.ru` — один человек. */
export function parseEmail(v: unknown): string | null {
  if (typeof v !== 'string') return null;
  const email = v.trim().toLowerCase();
  if (new TextEncoder().encode(email).length > MAX_EMAIL_BYTES) return null;
  return EMAIL_RE.test(email) && !CONTROL_RE.test(email) ? email : null;
}

/** Только маршрут самого приложения (`#/…`): ссылка из письма не уведёт на чужой адрес. */
export function parseNext(v: unknown): string | null {
  if (typeof v !== 'string' || v.length > MAX_NEXT_CHARS || CONTROL_RE.test(v)) return null;
  if (!v.startsWith('#/') || v.startsWith('#/auth/')) return null;
  return v;
}

export function parseSignInRequest(v: unknown): SignInRequest | null {
  const r = record(v);
  const email = r && parseEmail(r.email);
  if (!r || !email) return null;
  if (r.next === undefined) return { email };
  const next = parseNext(r.next);
  return next ? { email, next } : null;
}

export function parseConfirmRequest(v: unknown): ConfirmRequest | null {
  const r = record(v);
  return r && typeof r.token === 'string' && TOKEN_RE.test(r.token) ? { token: r.token } : null;
}

/** Имя с первого входа: пробелы по краям убираются, пустое не годится. */
export function parseNameRequest(v: unknown): NameRequest | null {
  const r = record(v);
  if (!r || typeof r.name !== 'string') return null;
  const name = r.name.trim();
  if (name.length === 0 || Array.from(name).length > MAX_NAME_CHARS || CONTROL_RE.test(name)) return null;
  return { name };
}

/** Ссылка из письма: токен лежит после `#`, поэтому не попадает на сервер и в журналы. */
export function confirmHash(token: string, next?: string): string {
  const q = new URLSearchParams({ token });
  if (next) q.set('next', next);
  return `#/auth/confirm?${q.toString()}`;
}

/** Обратно: что пришло в адресе страницы подтверждения; не она — `null`. */
export function parseConfirmHash(hash: string): { token: string | null; next: string | null } | null {
  const [path, query = ''] = hash.split('?', 2);
  if (path !== '#/auth/confirm') return null;
  const q = new URLSearchParams(query);
  const parsed = parseConfirmRequest({ token: q.get('token') });
  return { token: parsed ? parsed.token : null, next: parseNext(q.get('next')) };
}
