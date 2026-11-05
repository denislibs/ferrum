// HTTP-сервер: `Bun.serve` с маршрутами, без веб-фреймворка. Все зависимости — параметрами:
// тесты поднимают настоящий сервер на свободном порту против настоящих Postgres и Valkey.

import { MAX_CONTENT_BYTES } from '@ferrum/protocol';
import type { Pool } from 'pg';
import { apiHandler, type ApiDeps, type Limits, type Route } from './api';
import { authRoutes } from './auth-routes';
import type { Auth } from './auth';
import type { BlobStore } from './blob-store';
import { fileRoutes, uploadGate } from './file-routes';
import { imageRoutes } from './image-routes';
import { shareRoutes } from './share-routes';
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
  apiDeadlineMs?: number;
  uploadIdleMs?: number;
  maxContentBytes?: number;
}

/** Простой соединения: загрузка с паузами дольше этого уже оборвана проверкой `UPLOAD_IDLE_MS`. */
const IDLE_TIMEOUT_SECONDS = 120;

const apiDeps = (deps: AppDeps): ApiDeps => ({ ...deps, allowedEmails: deps.allowedEmails ?? [], deadlineMs: deps.apiDeadlineMs });

/** Все маршруты API: отсюда их берёт и сервер, и таблица проверки прав в тестах. */
export function apiRoutes(deps: AppDeps): Route[] {
  const api = apiDeps(deps);
  const gate = uploadGate();
  return [...fileRoutes(api, gate), ...imageRoutes(api, gate), ...teamRoutes(api), ...shareRoutes(api)];
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

export function startServer(deps: AppDeps, port: number): Bun.Server<undefined> {
  const handleAuth = authRoutes(deps);
  const withHeaders = responseHeaders(deps.publicUrl);
  const handleApi = apiHandler(apiRoutes(deps), apiDeps(deps));
  return Bun.serve({
    port,
    // Содержимое файла идёт потоком: предел здесь только не должен быть меньше предела файла.
    maxRequestBodySize: MAX_CONTENT_BYTES + 1024 * 1024,
    idleTimeout: IDLE_TIMEOUT_SECONDS,
    routes: {
      '/healthz': async () => withHeaders(await health(deps)),
      '/api/auth/*': async (req, server) => withHeaders(await handleAuth(req, server.requestIP(req)?.address)),
      '/api/*': async (req) => withHeaders(await handleApi(req)),
    },
    fetch: async (req) => withHeaders(await serveStatic(req, deps.webDist)),
    error: (e) => {
      console.error(e);
      return withHeaders(new Response('Внутренняя ошибка сервера', { status: 500 }));
    },
  });
}
