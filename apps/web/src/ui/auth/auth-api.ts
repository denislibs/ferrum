// Запросы входа к серверу. Токен сессии в JS не попадает: он в cookie `HttpOnly`.

import {
  API_CONFIRM,
  API_NAME,
  API_SESSION,
  API_SIGN_IN,
  API_SIGN_OUT,
  API_SIGN_OUT_EVERYWHERE,
  type AuthErrorBody,
  type SessionResponse,
  type SessionUser,
} from '@ferrum/protocol';
import type { AuthFailure } from './auth-logic';

export type ApiResult<T> = { ok: true; value: T } | { ok: false; failure: AuthFailure };

async function call<T>(path: string, init?: RequestInit): Promise<ApiResult<T>> {
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch {
    return { ok: false, failure: 'NETWORK' };
  }
  const body = (await res.json().catch(() => null)) as unknown;
  if (res.ok) return { ok: true, value: body as T };
  return { ok: false, failure: (body as AuthErrorBody | null)?.error ?? 'NETWORK' };
}

const post = <T>(path: string, body: unknown) =>
  call<T>(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

export const getSession = () => call<SessionResponse>(API_SESSION);
export const requestLink = (email: string, next: string | null) => post<{ ok: true }>(API_SIGN_IN, next ? { email, next } : { email });
export const confirmLink = (token: string) => post<{ ok: true }>(API_CONFIRM, { token });
export const saveName = (name: string) => post<{ user: SessionUser }>(API_NAME, { name });
export const signOut = () => post<{ ok: true }>(API_SIGN_OUT, {});
export const signOutEverywhere = () => post<{ ok: true }>(API_SIGN_OUT_EVERYWHERE, {});
