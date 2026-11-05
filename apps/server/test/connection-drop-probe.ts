// Отдельный процесс для теста обрыва соединений: необработанное `error` снимает весь процесс, а внутри
// `bun test` это выглядело бы как чужая ошибка. Настоящий сервер на настоящем пуле. Печатает итоги в JSON,
// до конца доживает, только если процесс пережил все обрывы. Всё идёт в `main` без верхнего `await`, как у
// настоящего сервера: пока модуль не дозагружен, Bun откладывает необработанную ошибку и процесс не падает.

import { Pool } from 'pg';
import { createPool } from '../src/db';
import { claimInvites } from '../src/invites';
import { apiStand, container } from './api-stand';
import { TEST_DATABASE_URL, TEST_VALKEY_URL } from './helpers';

async function main(): Promise<void> {
  const pool = createPool(TEST_DATABASE_URL);
  const s = await apiStand({ pool });
  const who = await s.signIn('drop@example.com');
  const f = await s.newFile(who);
  // Отдельный пул: блокировку держит посторонняя сессия, она же снимает занятое соединение.
  const admin = new Pool({ connectionString: TEST_DATABASE_URL });
  admin.on('error', () => {});

  /** Запрос занял соединение и ждёт блокировку строки. Снимаем именно его: так выглядит сбой базы. */
  async function dropWaiter(): Promise<void> {
    for (;;) {
      const { rows } = await admin.query<{ pid: number }>(
        `SELECT a.pid FROM pg_stat_activity a WHERE a.wait_event_type = 'Lock' AND a.pid <> pg_backend_pid()`,
      );
      if (rows[0]) {
        await admin.query('SELECT pg_terminate_backend($1)', [rows[0].pid]);
        return;
      }
      await Bun.sleep(10);
    }
  }

  /** Пока `lock` держит строку файла, `work` упирается в неё; соединение `work` обрывается. */
  async function whileLocked<T>(work: () => Promise<T>): Promise<T> {
    const lock = await admin.connect();
    try {
      await lock.query('BEGIN');
      await lock.query('SELECT 1 FROM file WHERE id = $1 FOR UPDATE', [f.id]);
      const running = work();
      await dropWaiter();
      return await running;
    } finally {
      await lock.query('ROLLBACK').catch(() => {});
      lock.release();
    }
  }

  const healthy = async () => (await fetch(`${s.publicUrl}/healthz`)).status;
  const out: Record<string, unknown> = {};

  // Транзакция фиксации содержимого: соединение выдано через `pool.connect()`.
  const written = await whileLocked(() => s.writeContent(who, f.id, 0, container()));
  out.transaction = written.status;
  out.transactionError = await written.json();
  out.afterTransaction = await healthy();

  // Одиночный запрос через пул.
  const renamed = await whileLocked(() => s.call(who, 'PATCH', `/api/files/${f.id}`, { name: 'Новое' }));
  out.query = renamed.status;
  out.afterQuery = await healthy();

  // Приглашения при входе: своя транзакция, обрыв — сбой, а не падение.
  await pool.query(
    `INSERT INTO file_invite (file_id, email, role, invited_by) VALUES ($1, 'drop@example.com', 'editor', $2) ON CONFLICT DO NOTHING`,
    [f.id, who.id],
  );
  await pool.query(`DELETE FROM file_access WHERE user_id = $1`, [who.id]);
  const lock = await admin.connect();
  await lock.query('BEGIN');
  await lock.query(`SELECT 1 FROM file_invite WHERE file_id = $1 FOR UPDATE`, [f.id]);
  const claiming = claimInvites(pool, who.id).then(
    () => 'записано',
    () => 'отказ',
  );
  await dropWaiter();
  out.invites = await claiming;
  await lock.query('ROLLBACK');
  lock.release();
  out.afterInvites = await healthy();

  // Пул живой: следующая запись проходит на новом соединении.
  await pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'owner') ON CONFLICT DO NOTHING`, [f.id, who.id]);
  out.next = (await s.writeContent(who, f.id, 0, container())).status;

  // Valkey: сервер рвёт соединение, идущие команды получают ошибку, клиент подключается заново.
  const killer = new Bun.RedisClient(TEST_VALKEY_URL);
  await killer.connect();
  await killer.send('CLIENT', ['KILL', 'TYPE', 'normal', 'SKIPME', 'yes']);
  killer.close();
  for (;;) {
    const res = await s.call(who, 'GET', `/api/files/${f.id}/access`);
    if (res.status === 200) {
      out.afterValkey = 200;
      break;
    }
    out.duringValkey = res.status;
    await Bun.sleep(20);
  }

  console.log(JSON.stringify(out));
  await admin.end();
  await pool.end();
  await s.stop();
}

void main().then(
  () => process.exit(0),
  (e: unknown) => {
    console.error(e);
    process.exit(2);
  },
);
