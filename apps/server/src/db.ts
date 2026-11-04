// Пул Postgres для запросов сервера. У него есть сроки: зависшая база или потерянные пакеты дают
// ошибку, а запросы не копятся без конца. Сроки меньше 10 с на весь обработчик (`AUTH_DEADLINE_MS`).

import { Pool, type PoolConfig } from 'pg';

const TIMEOUTS = {
  /** Получить соединение: новое или свободное из пула. */
  connectionTimeoutMillis: 3000,
  /** Срок запроса на стороне базы. */
  statement_timeout: 5000,
  /** Срок запроса на стороне клиента: сработает и тогда, когда база молчит. */
  query_timeout: 6000,
};

export function createPool(connectionString: string, over: Partial<PoolConfig> = {}): Pool {
  const pool = new Pool({ connectionString, ...TIMEOUTS, ...over });
  // Обрыв простаивающего соединения: пул создаст новое, но без обработчика процесс бы упал.
  pool.on('error', (e) => console.error('Postgres:', e.message));
  return pool;
}
