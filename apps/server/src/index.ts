// Запуск сервера: настройки, миграции под блокировкой, затем приём запросов.

import { join } from 'node:path';
import { Pool } from 'pg';
import { startServer } from './app';
import { createAuth } from './auth';
import { FsBlobStore } from './blob-store';
import { loadConfig } from './config';
import { smtpMailer } from './mail';
import { migrate } from './migrate';
import { connectValkey } from './valkey';

const config = loadConfig();
const pool = new Pool({ connectionString: config.databaseUrl });
// Обрыв простаивающего соединения: пул создаст новое, но без обработчика процесс бы упал.
pool.on('error', (e) => console.error('Postgres:', e.message));
const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
if (applied.length > 0) console.log(`Миграции применены: ${applied.join(', ')}`);
const valkey = await connectValkey(config.valkeyUrl);
// Хранилище байтов нужно с этапа 2; здесь — проверка, что том доступен для записи.
await new FsBlobStore(config.dataDir).put('healthcheck', new Uint8Array([1]));
// После миграций: Better Auth при создании сверяет схему базы.
const auth = createAuth({
  publicUrl: config.publicUrl,
  secret: config.authSecret,
  pool,
  valkey,
  mailer: smtpMailer(config.smtpUrl, config.mailFrom),
  allowedEmails: config.allowedEmails,
});
const server = startServer(
  { pool, valkey, webDist: config.webDist, auth, publicUrl: config.publicUrl, trustedProxies: config.trustedProxies },
  config.port,
);
console.log(`Ferrum слушает порт ${server.port}`);

// Остановка контейнера: дождаться запросов и закрыть соединения, а не ждать SIGKILL.
for (const signal of ['SIGTERM', 'SIGINT'] as const) {
  process.on(signal, () => {
    void server
      .stop()
      .then(() => pool.end())
      .finally(() => process.exit(0));
  });
}
