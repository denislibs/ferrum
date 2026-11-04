// HTTP-сервер: `Bun.serve` с маршрутами, без веб-фреймворка. Все зависимости — параметрами:
// тесты поднимают настоящий сервер на свободном порту против настоящих Postgres и Valkey.

import type { Pool } from 'pg';
import { authRoutes } from './auth-routes';
import type { Auth } from './auth';
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
  return Bun.serve({
    port,
    routes: {
      '/healthz': async () => withHeaders(await health(deps)),
      '/api/auth/*': async (req, server) => withHeaders(await handleAuth(req, server.requestIP(req)?.address)),
    },
    fetch: async (req) => withHeaders(await serveStatic(req, deps.webDist)),
    error: (e) => {
      console.error(e);
      return withHeaders(new Response('Внутренняя ошибка сервера', { status: 500 }));
    },
  });
}
