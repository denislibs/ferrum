// Небольшие запросы к серверу с основного потока: команды, «Поделиться», запрос доступа. Всё размером с
// документ — в worker'е (`server-store.ts`). Ответ не того вида — `NETWORK`, у запроса срок.

import {
  API_ERROR_CODES,
  API_FILES,
  API_PROJECTS,
  API_TEAMS,
  isFileMeta,
  isProjectDto,
  isShare,
  isTeam,
  isTeamList,
  isTeamMembers,
  parsePrefs,
  type PrefsDto,
  type ApiErrorCode,
  type ProjectDto,
  type ShareDto,
  type ShareRole,
  type TeamDto,
  type TeamListDto,
  type TeamMembersDto,
} from '@ferrum/protocol';
import { JSON_TIMEOUT_MS } from '../../core/server-files';

export type ApiFailure = ApiErrorCode | 'NETWORK';
export type ApiResult<T> = { ok: true; value: T } | { ok: false; failure: ApiFailure };

const isErrorCode = (v: unknown): v is ApiErrorCode => (API_ERROR_CODES as readonly unknown[]).includes(v);
const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const isOk = (v: unknown): v is { ok: true } => isRecord(v) && v.ok === true;

export function apiFailureText(f: ApiFailure): string {
  switch (f) {
    case 'NETWORK':
      return 'Нет связи с сервером. Попробуйте ещё раз';
    case 'UNAVAILABLE':
      return 'Сервер недоступен. Попробуйте позже';
    case 'TOO_MANY_REQUESTS':
    case 'BUSY':
      return 'Слишком много запросов. Попробуйте позже';
    case 'UNAUTHORIZED':
      return 'Сессия завершена. Войдите снова';
    case 'FORBIDDEN':
      return 'Недостаточно прав';
    case 'NOT_FOUND':
      return 'Не найдено или нет доступа';
    case 'NOT_EMPTY':
      return 'Сначала уберите то, что внутри';
    case 'BAD_REQUEST':
      return 'Проверьте введённые данные';
    default:
      return 'Не получилось. Обновите страницу';
  }
}

async function call<T>(path: string, valid: (v: unknown) => v is T, init?: RequestInit): Promise<ApiResult<T>> {
  try {
    const res = await fetch(path, { ...init, signal: AbortSignal.timeout(JSON_TIMEOUT_MS) });
    if (res.status >= 500) return { ok: false, failure: 'UNAVAILABLE' };
    const body = (await res.json()) as unknown;
    if (res.ok) return valid(body) ? { ok: true, value: body } : { ok: false, failure: 'NETWORK' };
    const code = isRecord(body) ? body.error : undefined;
    return { ok: false, failure: isErrorCode(code) ? code : 'NETWORK' };
  } catch {
    return { ok: false, failure: 'NETWORK' };
  }
}

const send = <T>(method: string, path: string, valid: (v: unknown) => v is T, body?: unknown) =>
  call<T>(path, valid, {
    method,
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });

const file = (id: string) => `${API_FILES}/${encodeURIComponent(id)}`;
const team = (id: string) => `${API_TEAMS}/${encodeURIComponent(id)}`;
const project = (id: string) => `${API_PROJECTS}/${encodeURIComponent(id)}`;

export const getShare = (id: string): Promise<ApiResult<ShareDto>> => call(`${file(id)}/share`, isShare);
export const invitePerson = (id: string, email: string, role: ShareRole) =>
  send('POST', `${file(id)}/share/invite`, isShare, { email, role });
export const changeRole = (id: string, entry: string, role: ShareRole) =>
  send('PATCH', `${file(id)}/share/people/${entry}`, isShare, { role });
export const removePerson = (id: string, entry: string) =>
  send('DELETE', `${file(id)}/share/people/${entry}`, (v): v is ShareDto | { ok: true } => isShare(v) || isOk(v));
export const updateLink = (id: string, enabled: boolean, role: ShareRole) =>
  send('PUT', `${file(id)}/share/link`, isShare, { enabled, role });
export const resetLink = (id: string) => send('POST', `${file(id)}/share/link/reset`, isShare);
export const redeemLink = (id: string, token: string) => send('POST', `${file(id)}/redeem`, isFileMeta, { token });
export const requestFileAccess = (id: string) => send('POST', `${file(id)}/request-access`, isOk);
export const moveFile = (id: string, projectId: string | null) => send('POST', `${file(id)}/move`, isFileMeta, { projectId });

export const getTeams = (): Promise<ApiResult<TeamListDto>> => call(API_TEAMS, isTeamList);
export const createTeam = (name: string): Promise<ApiResult<TeamDto>> => send('POST', API_TEAMS, isTeam, { name });
export const renameTeam = (id: string, name: string) => send('PATCH', team(id), isOk, { name });
export const deleteTeam = (id: string) => send('DELETE', team(id), isOk);
export const getMembers = (id: string): Promise<ApiResult<TeamMembersDto>> => call(`${team(id)}/members`, isTeamMembers);
export const inviteMember = (id: string, email: string) => send('POST', `${team(id)}/invite`, isTeamMembers, { email });
export const removeMember = (id: string, entry: string) => send('DELETE', `${team(id)}/members/${entry}`, isTeamMembers);
export const createProject = (id: string, name: string): Promise<ApiResult<ProjectDto>> =>
  send('POST', `${team(id)}/projects`, isProjectDto, { name });
export const renameProject = (id: string, name: string) => send('PATCH', project(id), isProjectDto, { name });
export const deleteProject = (id: string) => send('DELETE', project(id), isOk);

const isPrefs = (v: unknown): v is PrefsDto => parsePrefs(v) !== null;
/** Настройка «Письма о комментариях» (спека подсистемы 4, раздел 10.4): на сервере, письма отправляет он. */
export const getPrefs = (): Promise<ApiResult<PrefsDto>> => call('/api/me/prefs', isPrefs);
export const setPrefs = (prefs: PrefsDto) => send('PUT', '/api/me/prefs', (v): v is unknown => isPrefs(v) || isOk(v), prefs);
