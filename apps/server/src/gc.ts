// Фоновая очистка: байты, которые больше никому не нужны. Запись о них появляется в той же транзакции,
// что и замена, поэтому сбой ничего не теряет: следующий проход удалит то, что не успел этот.

import type { Pool } from 'pg';
import type { BlobStore } from './blob-store';

/** Проход раз в минуту, у него есть срок на каждый запрос (`Pool`). */
export const GC_INTERVAL_MS = 60_000;
/** Неиспользуемая картинка живёт столько (спека, раздел 6.4): отмена может вернуть ссылку на неё. */
export const IMAGE_GRACE_DAYS = 30;
/** Загрузка без фиксации дольше этого брошена; идущая получит отказ при фиксации, а не потеряет байты. */
export const PENDING_UPLOAD_HOURS = 24;
const BATCH = 200;

/** Картинки: ссылка файла без ссылок в содержимом снимается через срок, картинка без файлов получает
 * отметку и через срок уходит в список на удаление. Удаление записи и отправка байтов на удаление —
 * одним запросом с условием «всё ещё ни одного файла»: одновременная загрузка той же картинки
 * либо успевает раньше и картинка живёт, либо создаёт новую запись со своими байтами. */
async function sweepImages(pool: Pool): Promise<void> {
  await pool.query(`DELETE FROM file_image WHERE unreferenced_at < now() - make_interval(days => $1)`, [IMAGE_GRACE_DAYS]);
  await pool.query(`UPDATE image i SET orphaned_at = now()
    WHERE orphaned_at IS NULL AND NOT EXISTS (SELECT 1 FROM file_image l WHERE l.hash = i.hash)`);
  await pool.query(`UPDATE image i SET orphaned_at = NULL
    WHERE orphaned_at IS NOT NULL AND EXISTS (SELECT 1 FROM file_image l WHERE l.hash = i.hash)`);
  await pool.query(
    `WITH gone AS (
       DELETE FROM image i WHERE orphaned_at < now() - make_interval(days => $1)
         AND NOT EXISTS (SELECT 1 FROM file_image l WHERE l.hash = i.hash)
       RETURNING blob_key
     )
     INSERT INTO blob_trash (key, delete_after) SELECT blob_key, now() FROM gone ON CONFLICT DO NOTHING`,
    [IMAGE_GRACE_DAYS],
  );
}

/** Брошенные загрузки (сбой транзакции или процесса) идут в список на удаление. */
async function sweepPending(pool: Pool): Promise<void> {
  await pool.query(
    `WITH due AS (
       DELETE FROM blob_pending WHERE key IN (
         SELECT key FROM blob_pending WHERE started_at < now() - make_interval(hours => $1) ORDER BY started_at LIMIT $2
       ) RETURNING key
     )
     INSERT INTO blob_trash (key, delete_after) SELECT key, now() FROM due ON CONFLICT DO NOTHING`,
    [PENDING_UPLOAD_HOURS, BATCH],
  );
}

/** Сколько байтов-записей удалено. Один проход на все процессы: сервер один, но блокировка дешевле ошибки. */
export async function runGc(pool: Pool, blobs: BlobStore): Promise<number> {
  await sweepPending(pool);
  await sweepImages(pool);
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
