// Фоновая очистка: байты, которые больше никому не нужны. Запись о них появляется в той же транзакции,
// что и замена, поэтому сбой ничего не теряет: следующий проход удалит то, что не успел этот.

import type { Pool } from 'pg';
import type { BlobStore } from './blob-store';

/** Проход раз в минуту, у него есть срок на каждый запрос (`Pool`). */
export const GC_INTERVAL_MS = 60_000;
const BATCH = 200;

/** Сколько байтов-записей удалено. Один проход на все процессы: сервер один, но блокировка дешевле ошибки. */
export async function runGc(pool: Pool, blobs: BlobStore): Promise<number> {
  const { rows } = await pool.query<{ key: string }>(
    'SELECT key FROM blob_trash WHERE delete_after <= now() ORDER BY delete_after LIMIT $1',
    [BATCH],
  );
  let removed = 0;
  for (const { key } of rows) {
    try {
      await blobs.delete(key);
      await pool.query('DELETE FROM blob_trash WHERE key = $1', [key]);
      removed++;
    } catch (e) {
      console.error('Очистка: не удалось удалить байты:', e instanceof Error ? e.message : e);
    }
  }
  return removed;
}

/** Запускает очистку; возвращает остановку. Ошибка прохода — строка в журнал, следующий проход всё равно будет. */
export function startGc(pool: Pool, blobs: BlobStore, everyMs = GC_INTERVAL_MS): () => void {
  let busy = false;
  const timer = setInterval(() => {
    if (busy) return;
    busy = true;
    runGc(pool, blobs)
      .catch((e: unknown) => console.error('Очистка:', e instanceof Error ? e.message : e))
      .finally(() => {
        busy = false;
      });
  }, everyMs);
  return () => clearInterval(timer);
}
