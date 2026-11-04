// Запуск сервера: настройки, миграции под блокировкой, затем приём запросов.

import { join } from 'node:path';
import { Pool } from 'pg';
import { startServer } from './app';
import { createAuth } from './auth';
import { FsBlobStore } from './blob-store';
import { loadConfig } from './config';
import { createPool } from './db';
import { smtpMailer } from './mail';
import { migrate } from './migrate';
import { connectValkey } from './valkey';

// Сигналы — раньше всего: запуск может ждать базу и Valkey, а `docker stop` не должен ждать SIGKILL.
// Пока запросы не принимаются, дожидаться нечего.
let stop: () => void = () => process.exit(0);
for (const signal of ['SIGTERM', 'SIGINT'] as const) process.on(signal, () => stop());

const config = loadConfig();
const pool = createPool(config.databaseUrl);
// Миграции — своим соединением, без сроков запросов: ожидание блокировки и долгий файл не сбой.
const migrations = new Pool({ connectionString: config.databaseUrl, max: 1 });
const applied = await migrate(migrations, join(import.meta.dir, '../migrations')).finally(() => migrations.end());
if (applied.length > 0) console.log(`Миграции применены: ${applied.join(', ')}`);
console.log('Подключение к Valkey…');
const valkey = await connectValkey(config.valkeyUrl);
// Хранилище байтов нужно с этапа 2; здесь — проверка, что том доступен для записи.
await new FsBlobStore(config.dataDir).put('healthcheck', new Uint8Array([1]));
// Better Auth обращается к таблицам при первом запросе: к этому времени миграции уже применены.
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
stop = () =>
  void server
    .stop()
    .then(() => pool.end())
    .finally(() => process.exit(0));
