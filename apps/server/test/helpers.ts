// Общее для тестов сервера: настоящие Postgres и Valkey из `docker-compose.test.yml`
// (`npm run test:server` поднимает их сам). База и Valkey — только тестовые: перед каждым
// тестом очищаются целиком.

import { Pool } from 'pg';

export const TEST_DATABASE_URL = process.env.TEST_DATABASE_URL ?? 'postgres://ferrum:ferrum@127.0.0.1:15432/ferrum';
export const TEST_VALKEY_URL = process.env.TEST_VALKEY_URL ?? 'redis://127.0.0.1:16379';

export const TEST_SECRET = 'ferrum-test-secret-ZkP3nV8qR2xW7mB4tY9c';

export function testPool(): Pool {
  return new Pool({ connectionString: TEST_DATABASE_URL });
}

export async function resetDatabase(pool: Pool): Promise<void> {
  await pool.query('DROP SCHEMA public CASCADE; CREATE SCHEMA public;');
}

export async function resetValkey(client: Bun.RedisClient): Promise<void> {
  await client.send('FLUSHDB', []);
}

/** Прокси перед настоящим сервисом: `freeze()` оставляет соединения открытыми, но ответов больше
 * нет — так выглядит зависший сервис или потерянные пакеты. */
export function tcpProxy(target: number) {
  type Pipe = { peer?: Bun.Socket<Pipe>; early: Uint8Array[] };
  let frozen = false;
  const listener = Bun.listen<Pipe>({
    hostname: '127.0.0.1',
    port: 0,
    socket: {
      open: async (client) => {
        client.data = { early: [] };
        const peer = await Bun.connect<Pipe>({
          hostname: '127.0.0.1',
          port: target,
          socket: {
            data: (_up, d) => {
              if (!frozen) client.write(d);
            },
            close: () => void client.end(),
            open: () => {},
            error: () => {},
          },
        });
        for (const chunk of client.data.early) peer.write(chunk);
        client.data.peer = peer;
      },
      data: (client, d) => {
        if (frozen) return;
        if (client.data.peer) client.data.peer.write(d);
        else client.data.early.push(new Uint8Array(d));
      },
      close: (client) => void client.data.peer?.end(),
      error: () => {},
    },
  });
  return {
    port: listener.port,
    freeze: () => {
      frozen = true;
    },
    stop: () => listener.stop(true),
  };
}
