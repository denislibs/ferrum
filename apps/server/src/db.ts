// Пул Postgres для запросов сервера. У него есть сроки: зависшая база или потерянные пакеты дают
// ошибку, а запросы не копятся без конца. Сроки меньше 10 с на весь обработчик (`AUTH_DEADLINE_MS`).

import { Pool, type PoolClient, type PoolConfig } from 'pg';

const TIMEOUTS = {
  /** Получить соединение: новое или свободное из пула. */
  connectionTimeoutMillis: 3000,
  /** Срок запроса на стороне базы. */
  statement_timeout: 5000,
  /** Срок запроса на стороне клиента: сработает и тогда, когда база молчит. */
  query_timeout: 6000,
};

/** Обрыв соединения не должен ронять процесс. `node-pg` шлёт `error` туда, где соединение сейчас: простаивающее —
 * пулу, а выданное (транзакция, запрос через пул) — самому клиенту, и без слушателя это необработанное исключение,
 * которого не увидит ни один `catch`. Поэтому слушатель стоит на каждом клиенте с рождения: так он есть и у
 * соединений Better Auth. Запрос на таком соединении получает отказ, маршрут отвечает 503, а пул соединение
 * обратно не берёт и заводит новое. */
export function guardPool(pool: Pool): Pool {
  const report = (e: Error) => console.error('Postgres:', e.message);
  pool.on('error', report);
  pool.on('connect', (client) => void client.on('error', report));
  return pool;
}

export function createPool(connectionString: string, over: Partial<PoolConfig> = {}): Pool {
  return guardPool(new Pool({ connectionString, ...TIMEOUTS, ...over }));
}

/** Транзакция на своём соединении. `ROLLBACK` не прошёл (соединение оборвалось, зависло) — соединение в
 * неизвестном состоянии: пул его закрывает, а не выдаёт следующему запросу посреди чужой транзакции. */
export async function inTransaction<T>(pool: Pool, work: (c: PoolClient) => Promise<T>): Promise<T> {
  const client = await pool.connect();
  let broken: Error | undefined;
  try {
    await client.query('BEGIN');
    const out = await work(client);
    await client.query('COMMIT');
    return out;
  } catch (e) {
    await client.query('ROLLBACK').catch((failed: Error) => {
      broken = failed;
    });
    throw e;
  } finally {
    client.release(broken);
  }
}
