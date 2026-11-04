// Общее для тестов сервера: настоящие Postgres и Valkey из `docker-compose.test.yml`
// (`npm run test:server` поднимает их сам). База и Valkey — только тестовые: перед каждым
// тестом очищаются целиком.

import { Pool } from 'pg';

export const TEST_DATABASE_URL = process.env.TEST_DATABASE_URL ?? 'postgres://ferrum:ferrum@127.0.0.1:15432/ferrum';
export const TEST_VALKEY_URL = process.env.TEST_VALKEY_URL ?? 'redis://127.0.0.1:16379';

export function testPool(): Pool {
  return new Pool({ connectionString: TEST_DATABASE_URL });
}

export async function resetDatabase(pool: Pool): Promise<void> {
  await pool.query('DROP SCHEMA public CASCADE; CREATE SCHEMA public;');
}

export async function resetValkey(client: Bun.RedisClient): Promise<void> {
  await client.send('FLUSHDB', []);
}
