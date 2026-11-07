// HTTP-сервер: `Bun.serve` с маршрутами, без веб-фреймворка. Все зависимости — параметрами:
// тесты поднимают настоящий сервер на свободном порту против настоящих Postgres и Valkey.

import { isUuid, MAX_CONTENT_BYTES } from '@ferrum/protocol';
import type { Pool } from 'pg';
import { apiHandler, type ApiDeps, type Ctx, type Limits, type Route, type UserSlots } from './api';
import { authRoutes } from './auth-routes';
import type { Auth } from './auth';
import type { BlobStore, UploadPace } from './blob-store';
import { clientIp, proxyMatcher } from './client-ip';
import { CommentMailWorker, type CommentMailOptions } from './comment-mail';
import { commentRoutes } from './comment-routes';
import { fileRoutes, uploadGate } from './file-routes';
import { imageRoutes } from './image-routes';
import { shareRoutes } from './share-routes';
import type { CompactInit } from './sync/compact.worker';
import { Compactor, type CompactLimits } from './sync/compactor';
import { SyncHub, type ConnData, type SyncLimits } from './sync/hub';
import { teamRoutes } from './team-routes';
import type { Mailer } from './mail';
import { responseHeaders, serveStatic } from './static';
import { valkeyAlive } from './valkey';

export interface AppDeps {
  pool: Pool;
  valkey: Bun.RedisClient;
  webDist: string;
  auth: Auth;
  publicUrl: string;
  trustedProxies: readonly string[];
  authDeadlineMs?: number;
  blobs: BlobStore;
  mailer: Mailer;
  /** Пусто — письма всем, как у входа. */
  allowedEmails?: string[];
  secret: string;
  /** Тесты сокращают пределы и сроки API. */
  limits?: Partial<Limits>;
  /** Места запросов людей в работе: тесты дают свои. */
  slots?: UserSlots;
  /** Места тяжёлых чтений на процесс: тесты дают свои. */
  heavySlots?: UserSlots;
  /** Места тяжёлых чтений на человека: тесты дают свои. */
  heavyUserSlots?: UserSlots;
  /** Группировка писем о комментариях: тесты сокращают. */
  commentMailDelayMs?: number;
  /** Отправка писем о комментариях; `false` — без неё (тесты гоняют свою с поддельной почтой). */
  commentMail?: Partial<CommentMailOptions> | false;
  apiDeadlineMs?: number;
  uploadIdleMs?: number;
  uploadPace?: UploadPace;
  maxContentBytes?: number;
  /** Тесты сокращают пределы и сроки WebSocket. */
  syncLimits?: Partial<SyncLimits>;
  /** Откуда worker сжатия берёт базу и байты; нет — сжатия нет (тесты другого). */
  compact?: { init: CompactInit; limits?: Partial<CompactLimits> };
}

/** Запись, после которой доступ к открытым файлам не изменился: содержимое, превью, картинки, отметка открытия. */
const NO_ACCESS_CHANGE = /\/(content|thumb|touch|images\/[^/]+)$/;

/** Маршрут изменил файлы или права: открытые соединения сверяют роль сразу, а не по таймеру. Новая команда пуста: доступа
 * к файлам она не меняет. Маршрут, назвавший затронутые файлы, сверяет только их; без `:id` и без названных — все файлы,
 * запуски сливаются. */
function accessChanged(hub: SyncHub, route: Route, ctx: Ctx): void {
  if (route.keepsAccess) return;
  if (NO_ACCESS_CHANGE.test(route.path) || route.path === '/api/files' || (route.method === 'POST' && route.path === '/api/teams')) return;
  const files = ctx.touched ?? (ctx.params.id ? [ctx.params.id] : null);
  if (files) for (const id of files) void hub.recheckFile(id).catch((e: unknown) => console.error('Проверка ролей:', e));
  else void hub.recheckAll();
}

/** Простой соединения: загрузка с паузами дольше этого уже оборвана проверкой `UPLOAD_IDLE_MS`. */
const IDLE_TIMEOUT_SECONDS = 120;

const apiDeps = (deps: AppDeps): ApiDeps => ({ ...deps, allowedEmails: deps.allowedEmails ?? [], deadlineMs: deps.apiDeadlineMs });

/** Все маршруты API: отсюда их берёт и сервер, и таблица проверки прав в тестах. */
export function apiRoutes(deps: AppDeps | ApiDeps): Route[] {
  const api = apiDeps(deps as AppDeps);
  const gate = uploadGate();
  return [...fileRoutes(api, gate), ...imageRoutes(api, gate), ...teamRoutes(api), ...shareRoutes(api), ...commentRoutes(api)];
}

/** Проверка не должна висеть дольше проверки контейнера. */
const HEALTH_TIMEOUT_MS = 2000;

function within(p: Promise<boolean>): Promise<boolean> {
  return Promise.race([p.catch(() => false), new Promise<boolean>((r) => setTimeout(r, HEALTH_TIMEOUT_MS, false))]);
}

async function health(deps: AppDeps): Promise<Response> {
  const [postgres, valkey] = await Promise.all([within(deps.pool.query('SELECT 1').then(() => true)), within(valkeyAlive(deps.valkey))]);
  return Response.json(
    { ok: postgres && valkey, postgres, valkey },
    { status: postgres && valkey ? 200 : 503, headers: { 'Cache-Control': 'no-store' } },
  );
}

/** Сервер и его WebSocket: остановка контейнера и тесты обращаются к обоим. */
export type Running = Bun.Server<ConnData> & { hub: SyncHub; compactor: Compactor | null; commentMail: CommentMailWorker | null };

export function startServer(deps: AppDeps, port: number): Running {
  const hub = new SyncHub(deps.pool, deps.blobs, deps.auth, deps.publicUrl, deps.syncLimits);
  const compactor = deps.compact ? new Compactor(deps.pool, deps.compact.init, deps.compact.limits) : null;
  if (compactor) {
    hub.onLog = (file, doc, count, bytes) => compactor.note(file, doc, count, bytes);
    hub.onLastLeft = (file) => compactor.lastLeft(file);
    compactor.active = (file) => hub.isOpen(file);
  }
  const handleAuth = authRoutes({ ...deps, hub });
  const commentMail =
    deps.commentMail === false
      ? null
      : new CommentMailWorker(deps.pool, deps.mailer, deps.publicUrl, (file) => hub.openUsers(file), deps.allowedEmails, deps.commentMail);
  void commentMail?.start().catch((e: unknown) => console.error('Письма о комментариях:', e instanceof Error ? e.message : e));
  const trusted = proxyMatcher(deps.trustedProxies);
  const withHeaders = responseHeaders(deps.publicUrl);
  const api: ApiDeps = {
    ...apiDeps(deps),
    afterWrite: (route, ctx) => accessChanged(hub, route, ctx),
    contentReplaced: (file) => hub.forgetFile(file),
    compactNow: (file, user) => (compactor ? compactor.manual(file, user) : Promise.resolve({ ok: true as const })),
    comments: { event: (file, ev) => hub.commentEvent(file, ev), openUsers: (file) => hub.openUsers(file) },
  };
  const handleApi = apiHandler(apiRoutes(api), api);
  const server = Bun.serve({
    port,
    // Содержимое файла идёт потоком: предел здесь только не должен быть меньше предела файла.
    maxRequestBodySize: MAX_CONTENT_BYTES + 1024 * 1024,
    idleTimeout: IDLE_TIMEOUT_SECONDS,
    routes: {
      '/healthz': async () => withHeaders(await health(deps)),
      '/api/auth/*': async (req: Request, server: Bun.Server<ConnData>) =>
        withHeaders(await handleAuth(req, server.requestIP(req)?.address)),
      '/api/files/:id/sync': async (req: Bun.BunRequest<'/api/files/:id/sync'>, server: Bun.Server<ConnData>) => {
        if (!isUuid(req.params.id)) return withHeaders(new Response(null, { status: 404 }));
        const refused = await hub.upgrade(req, server, req.params.id, clientIp(req, server.requestIP(req)?.address, trusted));
        return refused && withHeaders(refused);
      },
      '/api/*': async (req: Request) => withHeaders(await handleApi(req)),
    },
    fetch: async (req: Request) => withHeaders(await serveStatic(req, deps.webDist)),
    error: (e: Error) => {
      console.error(e);
      return withHeaders(new Response('Внутренняя ошибка сервера', { status: 500 }));
    },
    websocket: hub.websocket,
  });
  return Object.assign(server, { hub, compactor, commentMail });
}
