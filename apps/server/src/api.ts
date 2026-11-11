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
  type CommentEvent,
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
  /** Новые комментарии и ответы (спека, раздел 10.4): на человека, общий для всех его вкладок и сеансов. */
  comment: { max: 60, windowSeconds: 60 },
  /** Список обсуждений файла: первая загрузка — 1000 обсуждений за две страницы по ~50 мс базы, догоняние — дёшево. Общий
   * `read` (1200 в минуту) дал бы одному человеку занять ядро базы. Окно короткое: минутный запас не тратится одним
   * всплеском в первые секунды. */
  commentList: { max: 20, windowSeconds: 10 },
  /** Обсуждение целиком: до 500 комментариев и `THREAD_BYTES_MAX` текста — свой предел, как у списка, а не общий `read`. */
  commentThread: { max: 30, windowSeconds: 10 },
  /** Правка комментария, перенос метки, закрытие и открытие: каждое — событие всем соединениям файла с обсуждением внутри,
   * поэтому как у новых комментариев, а не общий `write` (300 в минуту правок 16-КБ текста — ×50 рассылки). */
  commentEdit: { max: 60, windowSeconds: 60 },
} as const;
export type LimitName = keyof typeof LIMITS;
export type Limits = Record<LimitName, { max: number; windowSeconds: number }>;

/** Запросы одного человека в работе (на процесс): пул Postgres общий на всех (10 соединений), и пределы частоты его не
 * защищают — минутный запас тратится разом, двадцатью запросами одновременно. Сверх `running` запрос ждёт своей очереди,
 * не держа соединения базы (экран файлов просит превью окна разом, отказ оставил бы их пустыми); сверх `waiting` —
 * 429 `BUSY`. Потоковые маршруты сюда не входят: у загрузок свои места (`uploadGate`). */
export const IN_FLIGHT = { running: 4, waiting: 200 };
export type InFlight = typeof IN_FLIGHT;
/** Тяжёлые чтения (`Route.heavy`: список обсуждений, обсуждение целиком) всех людей процесса разом: места на человека
 * (`IN_FLIGHT`) не спасают пул от десятка людей, каждый из которых в своём пределе. Сверх `running` запрос ждёт, не держа
 * соединения базы; сверх `waiting` — 429 `BUSY`. Остальные маршруты всегда находят свободные соединения пула. */
export const HEAVY_IN_FLIGHT: InFlight = { running: 3, waiting: 500 };
/** Тяжёлые чтения одного человека: из общих мест (`HEAVY_IN_FLIGHT`) у него в работе не больше `running`, остальные ждут
 * своей очереди, не стоя в общей; сверх `waiting` — 429 `BUSY`. Несколько вкладок одного человека не забирают все общие места. */
export const HEAVY_PER_USER: InFlight = { running: 2, waiting: 20 };

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
  /** Сжать журнал файла сейчас: копия для скачивания должна быть полной. Не вышло — когда повторить; `incomplete` — журнал
   * документа, который видят люди, в файл не входит. */
  compactNow?: (file: string, user: string) => Promise<{ ok: true } | { ok: false; retryAfter: number } | { ok: false; incomplete: true }>;
  /** Комментарии и соединения файла: событие после фиксации изменения и кто держит файл открытым (им письма не нужны).
   * Нет — событий нет, письма всем (тесты маршрутов без концентратора). */
  comments?: { event(file: string, ev: CommentEvent): void; openUsers(file: string): string[] };
  /** Группировка писем о комментариях; по умолчанию — `COMMENT_MAIL_DELAY_MS`. */
  commentMailDelayMs?: number;
  /** Места запросов в работе; по умолчанию — свои, по `IN_FLIGHT`. Тесты дают свои и смотрят в них. */
  slots?: UserSlots;
  /** Места тяжёлых чтений на процесс; по умолчанию — свои, по `HEAVY_IN_FLIGHT`. */
  heavySlots?: UserSlots;
  /** Места тяжёлых чтений на человека; по умолчанию — свои, по `HEAVY_PER_USER`. */
  heavyUserSlots?: UserSlots;
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
  /** Маршрут без `:id` назвал файлы, доступ к которым он изменил: сверяются только они. */
  touched?: string[];
}

export interface Route {
  method: 'GET' | 'HEAD' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  /** `:id`, `:teamId`, `:projectId`, `:entryId`, `:threadId`, `:commentId` — UUID, `:hash` — blake3: неверный вид — 404. */
  path: string;
  access: Access;
  limit: LimitName;
  /** Тело идёт потоком: обработчик следит за простоем сам, общего срока нет. */
  stream?: boolean;
  /** Удачная запись не меняет доступа к файлам (комментарии, настройки): открытые соединения роль не сверяют. */
  keepsAccess?: boolean;
  /** Тяжёлое чтение: обработчик ждёт места в общем на процесс семафоре (`HEAVY_IN_FLIGHT`). */
  heavy?: boolean;
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

/** Ответ — готовый текст JSON (собран базой): без разбора и сборки заново. */
export function okJson(text: string, status = 200): Response {
  return new Response(text, { status, headers: { 'Content-Type': 'application/json;charset=utf-8', 'Cache-Control': 'no-store' } });
}

/** Разбирает JSON не больше `max` байт; иначе и при мусоре — `null`, разбор протокола ответит 400. */
export async function readJson(req: Request, max = MAX_JSON_BYTES): Promise<unknown> {
  if (!req.body) return null;
  const chunks: Uint8Array[] = [];
  let size = 0;
  for await (const chunk of req.body) {
    size += chunk.length;
    if (size > max) return null;
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
  threadId: isUuid,
  commentId: isUuid,
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

/** Места запросов людей в работе (`IN_FLIGHT`): очередь на человека, освобождённое место переходит следующему в ней. */
export class UserSlots {
  private readonly users = new Map<string, { running: number; waiting: (() => void)[] }>();
  constructor(private readonly max: InFlight) {}

  /** Место человеку: `ready` — можно работать, `release` — освободить (и уйти из очереди, если ещё ждёт). Очередь полна —
   * `null`. */
  take(user: string): { ready: Promise<void>; release: () => void } | null {
    let u = this.users.get(user);
    if (!u) this.users.set(user, (u = { running: 0, waiting: [] }));
    const slot = u;
    let state: 'waiting' | 'running' | 'done' = 'waiting';
    const release = () => {
      if (state === 'waiting') {
        const i = slot.waiting.indexOf(start);
        if (i >= 0) slot.waiting.splice(i, 1);
      } else if (state === 'running') {
        const next = slot.waiting.shift();
        if (next) next();
        else slot.running--;
      }
      state = 'done';
      if (slot.running === 0 && slot.waiting.length === 0 && this.users.get(user) === slot) this.users.delete(user);
    };
    const { promise: ready, resolve } = Promise.withResolvers<void>();
    // Место переходит от освободившего: счётчик `running` не меняется.
    const start = () => {
      state = 'running';
      resolve();
    };
    if (slot.running < this.max.running) {
      slot.running++;
      start();
    } else if (slot.waiting.length < this.max.waiting) slot.waiting.push(start);
    else {
      release();
      return null;
    }
    return { ready, release };
  }

  /** Сколько запросов человека в работе и в очереди: тесты. */
  count(user: string): { running: number; waiting: number } {
    const u = this.users.get(user);
    return { running: u?.running ?? 0, waiting: u?.waiting.length ?? 0 };
  }
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

/** Место запроса в работе: `handle` освобождает его в конце, как бы запрос ни кончился. `heavy` — место тяжёлого чтения
 * в общей очереди, `mine` — среди тяжёлых мест человека. */
interface Job {
  done: boolean;
  release: () => void;
  heavy: () => void;
  mine: () => void;
}

/** Ключ общей очереди тяжёлых чтений в `UserSlots`. */
const HEAVY_KEY = '';

export function apiHandler(routes: Route[], deps: ApiDeps) {
  const compiled: Compiled[] = routes.map((route) => ({ route, parts: route.path.split('/').filter(Boolean) }));
  const storage = valkeyStorage(deps.valkey);
  const limits: Limits = { ...LIMITS, ...deps.limits };
  const deadline = deps.deadlineMs ?? API_DEADLINE_MS;
  const slots = deps.slots ?? new UserSlots(IN_FLIGHT);
  const heavySlots = deps.heavySlots ?? new UserSlots(HEAVY_IN_FLIGHT);
  const heavyUserSlots = deps.heavyUserSlots ?? new UserSlots(HEAVY_PER_USER);

  /** Всё, что до обработчика: порядок важен, чужой запрос не тратит ни базу, ни счётчик другого. Место в работе (`job`)
   * берётся после предела частоты и до первого запроса к базе; освобождает его `handle`. */
  async function prepare(route: Route, req: Request, url: URL, params: Record<string, string>, job: Job): Promise<Ctx> {
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
    if (!route.stream) {
      // Срок запроса вышел раньше: место не берётся — освободить его было бы уже некому.
      if (job.done) refuse(503, 'UNAVAILABLE');
      const slot = slots.take(user.id);
      if (!slot) throw new Refused(busy());
      job.release = slot.release;
      await slot.ready;
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
    if (route.heavy) {
      // После проверки прав: чужой запрос не занимает общих мест. Место человека (`IN_FLIGHT`) отдаётся: ожидая общую
      // очередь, тяжёлое чтение не держит его лёгкие запросы; базу тяжёлых бережёт свой предел — `HEAVY_PER_USER` на
      // человека и `HEAVY_IN_FLIGHT` на всех.
      job.release();
      job.release = () => {};
      if (job.done) refuse(503, 'UNAVAILABLE');
      const mine = heavyUserSlots.take(user.id);
      if (!mine) throw new Refused(busy());
      job.mine = mine.release;
      await mine.ready;
      // Срок мог выйти, пока ждали своей очереди: место уже отдано, общее не берётся.
      if (job.done) refuse(503, 'UNAVAILABLE');
      const slot = heavySlots.take(HEAVY_KEY);
      if (!slot) throw new Refused(busy());
      job.heavy = slot.release;
      await slot.ready;
    }
    return ctx;
  }

  return async function handle(req: Request): Promise<Response> {
    const url = new URL(req.url);
    const parts = url.pathname.split('/').filter(Boolean);
    for (const c of compiled) {
      const params = match(c, req.method, parts);
      if (!params) continue;
      const job: Job = { done: false, release: () => {}, heavy: () => {}, mine: () => {} };
      try {
        const ctx = await withinDeadline(prepare(c.route, req, url, params, job), deadline, url.pathname);
        const res = await (c.route.stream ? c.route.handle(ctx) : withinDeadline(c.route.handle(ctx), deadline, url.pathname));
        if (req.method !== 'GET' && req.method !== 'HEAD' && res.ok) deps.afterWrite?.(c.route, ctx);
        return res;
      } catch (e) {
        if (e instanceof Refused) return e.response;
        console.error('Сбой маршрута API', c.route.method, c.route.path, e instanceof Error ? e.message : e);
        return apiError(503, 'UNAVAILABLE');
      } finally {
        // И по сроку: запрос к базе ограничен своими сроками пула (меньше срока API), место дольше не держится.
        job.done = true;
        job.heavy();
        job.mine();
        job.release();
      }
    }
    return apiError(404, 'NOT_FOUND');
  };
}
