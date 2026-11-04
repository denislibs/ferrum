// Только для генерации таблиц Better Auth (`apps/server/migrations/002_better_auth.sql`):
// bun --bun x auth@1.7.16 generate --config ./auth-schema.ts --output schema.sql -y
// Базы и Valkey этому файлу не нужны: схема зависит только от настроек. Тест «миграция совпадает
// со схемой» в `test/auth.test.ts` сверяет миграцию с той же схемой.

import { betterAuth } from 'better-auth';
import { Pool } from 'pg';
import { authOptions } from './src/auth';

export const auth = betterAuth(
  authOptions({
    publicUrl: 'http://localhost:8090',
    secret: 'schema-only-schema-only-schema-only-0000',
    pool: new Pool({ connectionString: process.env.DATABASE_URL ?? 'postgres://ferrum:ferrum@127.0.0.1:15432/ferrum' }),
    valkey: new Bun.RedisClient(process.env.VALKEY_URL ?? 'redis://127.0.0.1:16379'),
    mailer: { send: async () => {} },
    allowedEmails: [],
  }),
);
