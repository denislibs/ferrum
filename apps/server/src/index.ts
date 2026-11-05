// Запуск сервера: настройки, миграции под блокировкой, затем приём запросов.

import { join } from 'node:path';
import { Pool } from 'pg';
import { startServer } from './app';
import { createAuth } from './auth';
import { FsBlobStore } from './blob-store';
import { loadConfig } from './config';
import { startGc } from './gc';
import { createPool, guardPool } from './db';
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
const migrations = guardPool(new Pool({ connectionString: config.databaseUrl, max: 1 }));
const applied = await migrate(migrations, join(import.meta.dir, '../migrations')).finally(() => migrations.end());
if (applied.length > 0) console.log(`Миграции применены: ${applied.join(', ')}`);
console.log('Подключение к Valkey…');
const valkey = await connectValkey(config.valkeyUrl);
const blobs = new FsBlobStore(config.dataDir);
// Том доступен для записи: иначе отказ при запуске, а не на первом сохранении.
await blobs.put('healthcheck', new Uint8Array([1]));
const swept = await blobs.sweep();
if (swept > 0) console.log(`Убрано временных файлов от прошлого запуска: ${swept}`);
// Better Auth обращается к таблицам при первом запросе: к этому времени миграции уже применены.
const mailer = smtpMailer(config.smtpUrl, config.mailFrom);
const auth = createAuth({
  publicUrl: config.publicUrl,
  secret: config.authSecret,
  pool,
  valkey,
  mailer,
  allowedEmails: config.allowedEmails,
});
const server = startServer(
  {
    pool,
    valkey,
    webDist: config.webDist,
    auth,
    publicUrl: config.publicUrl,
    trustedProxies: config.trustedProxies,
    blobs,
    mailer,
    allowedEmails: config.allowedEmails,
    secret: config.authSecret,
  },
  config.port,
);
const stopGc = startGc(pool, blobs);
console.log(`Ferrum слушает порт ${server.port}`);

// Остановка контейнера: дождаться запросов и закрыть соединения, а не ждать SIGKILL.
stop = () => {
  stopGc();
  void server
    .stop()
    .then(() => pool.end())
    .finally(() => process.exit(0));
};
