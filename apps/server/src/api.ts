// Конвейер маршрутов API файлов и команд. Всё, что должно быть у каждого маршрута, живёт здесь, а не в
// обработчиках: `Origin` на изменяющих запросах, сессия, предел частоты, проверка прав, срок и ответ в
// формате ошибок. Маршрут только объявляет, что ему нужно; забыть проверку нельзя, её нет в его коде.

import {
  ROLE_RANK,
  RANK_ROLE,
  USER_HEADER,
  isHash,
  isSyncDoc,
  isUuid,
  type ApiErrorBody,
  type ApiErrorCode,
  type FileRole,
} from '@ferrum/protocol';
import type { Pool } from 'pg';
import type { Auth } from './auth';
import { originAllowed } from './auth-routes';
import type { BlobStore, UploadPace } from './blob-store';
import type { Mailer } from './mail';
import { valkeyStorage } from './secondary-storage';

/** Меньше 15 с, которые ждёт клиент, и больше сроков `Pool`: человек видит ответ сервера. */
export const API_DEADLINE_MS = 10_000;
/** JSON-запросы маленькие: остальное отвергается, не дочитываясь. */
export const MAX_JSON_BYTES = 16 * 1024;

/** Группы пределов частоты на человека: окно в секундах и число запросов. */
export const LIMITS = {
  read: { max: 1200, windowSeconds: 60 },
  /** Картинки файла, каждая отдельным запросом: файл на `MAX_IMAGE_REFS` картинок должен открываться. */
  images: { max: 6000, windowSeconds: 60 },
  /** Опрос открытого файла: раз в 15 с на вкладку. */
  poll: { max: 120, windowSeconds: 60 },
  write: { max: 300, windowSeconds: 60 },
  /** Запись содержимого, превью и картинок: автосохранение и перенос файлов. */
  upload: { max: 600, windowSeconds: 60 },
  /** Новые файлы и команды. */
  create: { max: 600, windowSeconds: 3600 },
  /** Письма: приглашения и запросы доступа. */
  mail: { max: 30, windowSeconds: 3600 },
  /** Попытки открыть файл по ссылке. */
  redeem: { max: 30, windowSeconds: 60 },
} as const;
export type LimitName = keyof typeof LIMITS;
export type Limits = Record<LimitName, { max: number; windowSeconds: number }>;

export interface ApiDeps {
  pool: Pool;
  valkey: Bun.RedisClient;
  auth: Auth;
  publicUrl: string;
  blobs: BlobStore;
  mailer: Mailer;
  /** Кому можно писать: как у входа. */
  allowedEmails: string[];
  /** Секрет сервера: из него выводятся токены ссылок. */
  secret: string;
  /** Тесты сокращают; по умолчанию — `LIMITS`, `API_DEADLINE_MS`, `UPLOAD_IDLE_MS`. */
  limits?: Partial<Limits>;
  deadlineMs?: number;
  uploadIdleMs?: number;
  uploadPace?: UploadPace;
  maxContentBytes?: number;
  /** Изменяющий запрос удался: открытые соединения сверяют роль сразу. */
  afterWrite?: (route: Route, ctx: Ctx) => void;
  /** Содержимое файла заменено целиком: проверка пачек забывает его документы. */
  contentReplaced?: (file: string) => void;
}

export interface ApiUser {
  id: string;
  email: string;
  name: string;
}

/** Что нужно маршруту. `user` — только вход; остальные берут id из адреса и проверяют доступ:
 * нет доступа совсем — 404, как у несуществующего, не хватает роли — 403.
 * `member` — нужной роли мало получить по ссылке или за убранный в корзину файл: нужна прямая или через команду. */
export type Access =
  | { kind: 'user' }
  | { kind: 'file'; min: FileRole; member?: boolean }
  | { kind: 'team'; min: 'member' | 'owner' }
  | { kind: 'project'; min: 'member' | 'owner' };

export interface Ctx {
  req: Request;
  url: URL;
  params: Record<string, string>;
  user: ApiUser;
  /** Действующая роль в файле (`access.kind === 'file'`). */
  role: FileRole;
  /** Роль в команде (`team`, `project`) и команда проекта. */
  teamRole: 'owner' | 'member';
  teamId: string;
}

export interface Route {
  method: 'GET' | 'HEAD' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  /** `:id`, `:teamId`, `:projectId`, `:entryId` — UUID, `:hash` — blake3: неверный вид — 404. */
  path: string;
  access: Access;
  limit: LimitName;
  /** Тело идёт потоком: обработчик следит за простоем сам, общего срока нет. */
  stream?: boolean;
  handle(ctx: Ctx): Promise<Response>;
}

export function apiError(status: number, error: ApiErrorCode, headers: Record<string, string> = {}): Response {
  return Response.json({ error } satisfies ApiErrorBody, { status, headers: { 'Cache-Control': 'no-store', ...headers } });
}

/** Слоты загрузок заняты: они освобождаются быстро, клиент повторит сам. */
export const busy = (): Response => apiError(429, 'BUSY', { 'Retry-After': '5' });

export function ok(body: unknown, status = 200): Response {
  return Response.json(body, { status, headers: { 'Cache-Control': 'no-store' } });
}

/** Разбирает JSON не больше `MAX_JSON_BYTES`; иначе и при мусоре — `null`, разбор протокола ответит 400. */
export async function readJson(req: Request): Promise<unknown> {
  if (!req.body) return null;
  const chunks: Uint8Array[] = [];
  let size = 0;
  for await (const chunk of req.body) {
    size += chunk.length;
    if (size > MAX_JSON_BYTES) return null;
    chunks.push(chunk);
  }
  try {
    return JSON.parse(new TextDecoder().decode(Buffer.concat(chunks)));
  } catch {
    return null;
  }
}

const PARAM_CHECK: Record<string, (v: string) => boolean> = {
  id: isUuid,
  teamId: isUuid,
  projectId: isUuid,
  entryId: isUuid,
  hash: isHash,
  doc: isSyncDoc,
};

interface Compiled {
  route: Route;
  parts: string[];
}

function match(c: Compiled, method: string, parts: string[]): Record<string, string> | null {
  if (c.route.method !== method || c.parts.length !== parts.length) return null;
  const params: Record<string, string> = {};
  for (let i = 0; i < parts.length; i++) {
    const want = c.parts[i];
    if (want.startsWith(':')) {
      const name = want.slice(1);
      if (!PARAM_CHECK[name](parts[i])) return null;
      params[name] = parts[i];
    } else if (want !== parts[i]) return null;
  }
  return params;
}

class Refused extends Error {
  constructor(readonly response: Response) {
    super('refused');
  }
}

function refuse(status: number, code: ApiErrorCode): never {
  throw new Refused(apiError(status, code));
}

/** Запрос не должен висеть без ответа: работа не отменяется, но человек видит честный 503. */
function withinDeadline<T>(work: Promise<T>, ms: number, path: string): Promise<T> {
  let timer: Timer;
  const expired = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      console.error('Запрос API не уложился в срок:', path);
      reject(new Refused(apiError(503, 'UNAVAILABLE')));
    }, ms);
  });
  return Promise.race([work, expired]).finally(() => clearTimeout(timer));
}

export function apiHandler(routes: Route[], deps: ApiDeps) {
  const compiled: Compiled[] = routes.map((route) => ({ route, parts: route.path.split('/').filter(Boolean) }));
  const storage = valkeyStorage(deps.valkey);
  const limits: Limits = { ...LIMITS, ...deps.limits };
  const deadline = deps.deadlineMs ?? API_DEADLINE_MS;

  /** Всё, что до обработчика: порядок важен, чужой запрос не тратит ни базу, ни счётчик другого. */
  async function prepare(route: Route, req: Request, url: URL, params: Record<string, string>): Promise<Ctx> {
    if (req.method !== 'GET' && req.method !== 'HEAD' && !originAllowed(req, deps.publicUrl)) refuse(403, 'BAD_ORIGIN');
    const session = await deps.auth.api.getSession({ headers: req.headers, query: { disableRefresh: true } });
    if (!session) refuse(401, 'UNAUTHORIZED');
    // Вкладка работает за другого человека (вход сменили в другой вкладке): для неё сессии нет.
    const expected = req.headers.get(USER_HEADER);
    if (expected !== null && expected !== session.user.id) refuse(401, 'UNAUTHORIZED');
    const user: ApiUser = { id: session.user.id, email: session.user.email, name: session.user.name };
    const limit = limits[route.limit];
    const counter = `api:${route.limit}:${user.id}`;
    const used = await storage.increment!(counter, limit.windowSeconds);
    if (used > limit.max) {
      // Сколько ждать, знает только счётчик: клиент не гадает и не долбит сервер.
      const left = await deps.valkey.ttl(counter);
      throw new Refused(apiError(429, 'TOO_MANY_REQUESTS', { 'Retry-After': String(left > 0 ? left : limit.windowSeconds) }));
    }
    const ctx: Ctx = { req, url, params, user, role: 'viewer', teamRole: 'member', teamId: '' };
    const access = route.access;
    if (access.kind === 'file') {
      const { rows } = await deps.pool.query<{ rank: number | null }>('SELECT file_role_rank($1, $2) AS rank', [params.id, user.id]);
      const role = RANK_ROLE[rows[0]?.rank ?? 0];
      if (!role) refuse(404, 'NOT_FOUND');
      if (ROLE_RANK[role] < ROLE_RANK[access.min]) refuse(403, 'FORBIDDEN');
      if (access.member) {
        const member = await deps.pool.query<{ rank: number }>('SELECT file_member_rank($1, $2) AS rank', [params.id, user.id]);
        if (member.rows[0].rank < ROLE_RANK[access.min]) refuse(403, 'FORBIDDEN');
      }
      ctx.role = role;
    } else if (access.kind === 'team' || access.kind === 'project') {
      const { rows } =
        access.kind === 'team'
          ? await deps.pool.query<{ role: 'owner' | 'member'; team_id: string }>(
              'SELECT role, team_id FROM team_member WHERE team_id = $1 AND user_id = $2',
              [params.teamId, user.id],
            )
          : await deps.pool.query<{ role: 'owner' | 'member'; team_id: string }>(
              'SELECT m.role, m.team_id FROM project p JOIN team_member m ON m.team_id = p.team_id WHERE p.id = $1 AND m.user_id = $2',
              [params.projectId, user.id],
            );
      if (rows.length === 0) refuse(404, 'NOT_FOUND');
      if (access.min === 'owner' && rows[0].role !== 'owner') refuse(403, 'FORBIDDEN');
      ctx.teamRole = rows[0].role;
      ctx.teamId = rows[0].team_id;
    }
    return ctx;
  }

  return async function handle(req: Request): Promise<Response> {
    const url = new URL(req.url);
    const parts = url.pathname.split('/').filter(Boolean);
    for (const c of compiled) {
      const params = match(c, req.method, parts);
      if (!params) continue;
      try {
        const ctx = await withinDeadline(prepare(c.route, req, url, params), deadline, url.pathname);
        const res = await (c.route.stream ? c.route.handle(ctx) : withinDeadline(c.route.handle(ctx), deadline, url.pathname));
        if (req.method !== 'GET' && req.method !== 'HEAD' && res.ok) deps.afterWrite?.(c.route, ctx);
        return res;
      } catch (e) {
        if (e instanceof Refused) return e.response;
        console.error('Сбой маршрута API', c.route.method, c.route.path, e instanceof Error ? e.message : e);
        return apiError(503, 'UNAVAILABLE');
      }
    }
    return apiError(404, 'NOT_FOUND');
  };
}
