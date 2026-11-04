// Запуск сервера: настройки, миграции под блокировкой, затем приём запросов.

import { join } from 'node:path';
import { Pool } from 'pg';
import { startServer } from './app';
import { FsBlobStore } from './blob-store';
import { loadConfig } from './config';
import { migrate } from './migrate';
import { connectValkey } from './valkey';

const config = loadConfig();
const pool = new Pool({ connectionString: config.databaseUrl });
const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
if (applied.length > 0) console.log(`Миграции применены: ${applied.join(', ')}`);
const valkey = connectValkey(config.valkeyUrl);
// Хранилище байтов нужно с этапа 2; здесь — проверка, что том доступен для записи.
await new FsBlobStore(config.dataDir).put('healthcheck', new Uint8Array([1]));
const server = startServer({ pool, valkey, webDist: config.webDist }, config.port);
console.log(`Ferrum слушает порт ${server.port}`);
