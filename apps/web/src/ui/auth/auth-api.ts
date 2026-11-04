// Запросы входа к серверу. Токен сессии в JS не попадает: он в cookie `HttpOnly`.

import {
  API_CONFIRM,
  API_NAME,
  API_SESSION,
  API_SIGN_IN,
  API_SIGN_OUT,
  API_SIGN_OUT_EVERYWHERE,
  type AuthErrorCode,
  type SessionResponse,
  type SessionUser,
} from '@ferrum/protocol';
import type { AuthFailure } from './auth-logic';

export type ApiResult<T> = { ok: true; value: T } | { ok: false; failure: AuthFailure };

/** Дольше этого запрос не ждём (поведение): зависший сервер — та же ошибка, что и недоступный. */
export const REQUEST_TIMEOUT_MS = 15_000;

const ERROR_CODES: readonly string[] = ['BAD_REQUEST', 'BAD_ORIGIN', 'INVALID_TOKEN', 'TOO_MANY_REQUESTS', 'UNAUTHORIZED', 'NOT_FOUND'];

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const isUser = (v: unknown): v is SessionUser =>
  isRecord(v) && typeof v.id === 'string' && typeof v.email === 'string' && typeof v.name === 'string';
const isOk = (v: unknown): v is { ok: true } => isRecord(v) && v.ok === true;
const isUserReply = (v: unknown): v is { user: SessionUser } => isRecord(v) && isUser(v.user);
const isSession = (v: unknown): v is SessionResponse => isRecord(v) && (v.user === null || isUser(v.user));

/** Тело разбирается здесь: ответ не того вида (страница сети, чужой JSON, неизвестный код) —
 * это `NETWORK`, а не успех и не `undefined` в тексте ошибки. */
async function call<T>(path: string, valid: (body: unknown) => body is T, init?: RequestInit): Promise<ApiResult<T>> {
  try {
    const res = await fetch(path, { ...init, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
    const body = (await res.json()) as unknown;
    if (res.ok) return valid(body) ? { ok: true, value: body } : { ok: false, failure: 'NETWORK' };
    const code = isRecord(body) ? body.error : undefined;
    return { ok: false, failure: typeof code === 'string' && ERROR_CODES.includes(code) ? (code as AuthErrorCode) : 'NETWORK' };
  } catch {
    return { ok: false, failure: 'NETWORK' };
  }
}

const post = <T>(path: string, valid: (body: unknown) => body is T, body: unknown) =>
  call<T>(path, valid, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

export const getSession = () => call<SessionResponse>(API_SESSION, isSession);
export const requestLink = (email: string, next: string | null) => post(API_SIGN_IN, isOk, next ? { email, next } : { email });
export const confirmLink = (token: string) => post(API_CONFIRM, isOk, { token });
export const saveName = (name: string) => post(API_NAME, isUserReply, { name });
export const signOut = () => post(API_SIGN_OUT, isOk, {});
export const signOutEverywhere = () => post(API_SIGN_OUT_EVERYWHERE, isOk, {});
