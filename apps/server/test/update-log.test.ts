// Журнал обновлений на настоящем Postgres: порядок, повторы, непрерывность, роль в транзакции записи, чужой пир,
// предел журнала, сбой базы и два писателя на одной базе (спека подсистемы 4, раздел 7.3).

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { apiStand } from './api-stand';
import { UpdateLog, type Item } from '../src/sync/update-log';
import { TEST_DATABASE_URL } from './helpers';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const other = await s.signIn('other@example.com');
const stranger = await s.signIn('stranger@example.com');
let file = '';

beforeEach(async () => {
  await s.reset();
  file = (await s.newFile(owner)).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor')`, [file, other.id]);
});

const item = (over: Partial<Item> = {}): Item => ({
  file,
  doc: 'root',
  user: owner.id,
  bytes: new Uint8Array([1, 2, 3]),
  start: { '1': 0 },
  end: { '1': 1 },
  ...over,
});

const rows = async (doc = 'root') =>
  (
    await s.pool.query<{ seq: string; end_vv: Record<string, number> }>(
      'SELECT seq, end_vv FROM doc_update WHERE file_id = $1 AND doc = $2 ORDER BY seq',
      [file, doc],
    )
  ).rows.map((r) => [Number(r.seq), r.end_vv]);

test('порядок журнала — порядок приёма, ack после фиксации', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  const committed: number[] = [];
  log.onCommitted = (items) => committed.push(...items.map((i) => i.seq));
  const out = await Promise.all([0, 1, 2, 3, 4].map((i) => log.submit(item({ start: { '1': i }, end: { '1': i + 1 } }))));
  expect(out.map((o) => (o.kind === 'written' ? o.seq : o.kind))).toEqual([1, 2, 3, 4, 5]);
  expect(committed).toEqual([1, 2, 3, 4, 5]);
  expect((await rows()).map((r) => r[0])).toEqual([1, 2, 3, 4, 5]);
  await log.stop();
});

test('повтор пачки — тот же результат без новой записи', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  expect(await log.submit(item())).toEqual({ kind: 'written', seq: 1 });
  expect(await log.submit(item())).toEqual({ kind: 'known', seq: 1 });
  // Перекрытие с новым хвостом: Loro примет, вектор сдвинется.
  expect(await log.submit(item({ start: { '1': 0 }, end: { '1': 3 } }))).toEqual({ kind: 'written', seq: 2 });
  expect(await rows()).toHaveLength(2);
  await log.stop();
});

test('разрыв по пиру отвергается и журнала не трогает', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  await log.submit(item());
  expect(await log.submit(item({ start: { '1': 5 }, end: { '1': 6 } }))).toEqual({ kind: 'refused', code: 'GAP' });
  expect(await rows()).toHaveLength(1);
  const { rows: st } = await s.pool.query<{ vv: Record<string, number> }>('SELECT vv FROM doc_state WHERE file_id = $1', [file]);
  expect(st[0].vv).toEqual({ '1': 1 });
  await log.stop();
});

test('роль проверяется в транзакции записи: снятая и читатель не пишут', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  expect(await log.submit(item({ user: other.id }))).toMatchObject({ kind: 'written' });
  await s.pool.query(`UPDATE file_access SET role = 'viewer' WHERE file_id = $1 AND user_id = $2`, [file, other.id]);
  expect(await log.submit(item({ user: other.id, start: { '1': 1 }, end: { '1': 2 } }))).toEqual({ kind: 'refused', code: 'FORBIDDEN' });
  expect(await log.submit(item({ user: stranger.id, start: { '9': 0 }, end: { '9': 1 } }))).toEqual({ kind: 'refused', code: 'FORBIDDEN' });
  expect(await rows()).toHaveLength(1);
  await log.stop();
});

test('чужой пир: первый писатель пира его занимает, подделка не проходит', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  await log.submit(item({ start: { '77': 0 }, end: { '77': 1 } }));
  expect(await log.submit(item({ user: other.id, start: { '77': 1 }, end: { '77': 2 } }))).toEqual({
    kind: 'refused',
    code: 'FOREIGN_PEER',
  });
  expect(await log.submit(item({ start: { '77': 1 }, end: { '77': 2 } }))).toMatchObject({ kind: 'written' });
  // Пир u64 не теряет младших разрядов.
  const big = '18446744073709551615';
  await log.submit(item({ start: { [big]: 0 }, end: { [big]: 1 } }));
  const { rows: p } = await s.pool.query<{ peer: string }>('SELECT peer::text AS peer FROM doc_peer d WHERE file_id = $1 ORDER BY d.peer', [
    file,
  ]);
  expect(p.map((x) => x.peer)).toEqual(['77', big]);
  await log.stop();
});

test('журнал не растёт сверх предела файла: отказ, правка цела у клиента', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1, maxLogBytesPerFile: 5 });
  expect(await log.submit(item())).toMatchObject({ kind: 'written' });
  expect(await log.submit(item({ start: { '1': 1 }, end: { '1': 2 } }))).toEqual({ kind: 'refused', code: 'LOG_FULL' });
  expect(await rows()).toHaveLength(1);
  await log.stop();
});

test('сбой базы: группа получает отказ «повторите», процесс живёт, следующая группа пишется', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  const failed = await s.failing('doc_update', 'INSERT', () => log.submit(item()));
  expect(failed).toEqual({ kind: 'refused', code: 'UNAVAILABLE' });
  const { rows: st } = await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file]);
  expect(st).toHaveLength(0);
  expect(await log.submit(item())).toEqual({ kind: 'written', seq: 1 });
  await log.stop();
});

test('два писателя на одной базе: все записаны, номера подряд, у каждого пира концы идут подряд', async () => {
  // Спека: процесс один, но решение этого не закрывает. Настоящие два клиента базы, как два процесса.
  const { Pool } = await import('pg');
  const second = new Pool({ connectionString: TEST_DATABASE_URL });
  const a = new UpdateLog(s.pool, { groupMs: 1 });
  const b = new UpdateLog(second, { groupMs: 1 });
  // У каждого писателя свой пир и счётчики по порядку: отказов быть не должно. Каждая пачка — своя группа, транзакции
  // двух писателей идут вперемешку.
  const writer = async (log: UpdateLog, peer: string) => {
    const out: string[] = [];
    for (let n = 0; n < 20; n++) out.push((await log.submit(item({ start: { [peer]: n }, end: { [peer]: n + 1 } }))).kind);
    return out;
  };
  const out = await Promise.all([writer(a, '100'), writer(b, '200')]);
  expect(out.flat()).toEqual(Array.from({ length: 40 }, () => 'written'));
  const logged = await rows();
  expect(logged.map((r) => r[0])).toEqual(Array.from({ length: 40 }, (_, i) => i + 1));
  for (const peer of ['100', '200']) {
    const ends = logged.flatMap(([, end]) => ((end as Record<string, number>)[peer] ? [(end as Record<string, number>)[peer]] : []));
    expect(ends).toEqual(Array.from({ length: 20 }, (_, i) => i + 1));
  }
  // Повтор, пришедший ко второму процессу, — уже известная запись.
  expect(await a.submit(item({ start: { '200': 0 }, end: { '200': 1 } }))).toMatchObject({ kind: 'known' });
  await a.stop();
  await b.stop();
  await second.end();
});

test('после фиксации сжатие видит размеры журнала вместе с этой группой', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  const seen: { doc: string; logCount: number; logBytes: number }[] = [];
  log.onCommitted = (_items, states) => seen.push(...states.map((x) => ({ doc: x.doc, logCount: x.logCount, logBytes: x.logBytes })));
  await log.submit(item({ bytes: new Uint8Array(1000) }));
  expect(seen).toEqual([{ doc: 'root', logCount: 1, logBytes: 1000 }]);
  await log.stop();
});

test('предел документов: новый документ сверх него отказывает и строки не заводит, прежние пишутся', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1, maxDocsPerFile: 2 });
  expect(await log.submit(item())).toMatchObject({ kind: 'written' });
  expect(await log.submit(item({ doc: '1@1', start: { '2': 0 }, end: { '2': 1 } }))).toMatchObject({ kind: 'written' });
  expect(await log.submit(item({ doc: '2@1', start: { '3': 0 }, end: { '3': 1 } }))).toEqual({ kind: 'refused', code: 'DOC_LIMIT' });
  const docs = async () =>
    (await s.pool.query<{ doc: string }>('SELECT doc FROM doc_state WHERE file_id = $1 ORDER BY doc', [file])).rows.map((r) => r.doc);
  expect(await docs()).toEqual(['1@1', 'root']);
  // Уже заведённым документам предел не мешает.
  expect(await log.submit(item({ start: { '1': 1 }, end: { '1': 2 } }))).toMatchObject({ kind: 'written' });
  // Отказ по роли строк не заводит.
  await s.pool.query('DELETE FROM doc_state WHERE file_id = $1 AND doc = $2', [file, '1@1']);
  expect(await log.submit(item({ user: stranger.id, doc: '3@1', start: { '9': 0 }, end: { '9': 1 } }))).toEqual({
    kind: 'refused',
    code: 'FORBIDDEN',
  });
  expect(await docs()).toEqual(['root']);
  await log.stop();
});

test('пачка удалённого файла в группе не роняет пачки других файлов', async () => {
  const gone = (await s.newFile(owner)).id;
  await s.pool.query('DELETE FROM file WHERE id = $1', [gone]);
  const log = new UpdateLog(s.pool, { groupMs: 50 });
  const [a, b] = await Promise.all([log.submit(item({ file: gone })), log.submit(item())]);
  expect(a).toEqual({ kind: 'refused', code: 'FORBIDDEN' });
  expect(b).toEqual({ kind: 'written', seq: 1 });
  await log.stop();
});

test('порядок блокировок как у сжатия и записи файла: файл, потом документы — журнал и они не сцепляются', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 1 });
  expect(await log.submit(item())).toMatchObject({ kind: 'written' });
  // Как сжатие (`compact.ts`): строка файла, затем строки документов.
  const other = await s.pool.connect();
  try {
    await other.query('BEGIN');
    await other.query('SELECT 1 FROM file WHERE id = $1 FOR UPDATE', [file]);
    const pending = log.submit(item({ start: { '1': 1 }, end: { '1': 2 } }));
    // Журнал дошёл до ожидания блокировки.
    for (let i = 0; i < 500; i++) {
      const { rows: w } = await s.pool.query(
        `SELECT 1 FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND pid <> pg_backend_pid()`,
      );
      if (w.length > 0) break;
      await Bun.sleep(5);
    }
    await other.query('SELECT 1 FROM doc_state WHERE file_id = $1 FOR UPDATE', [file]);
    await other.query('COMMIT');
    expect(await pending).toMatchObject({ kind: 'written', seq: 2 });
  } finally {
    await other.query('ROLLBACK').catch(() => {});
    other.release();
  }
  await log.stop();
});

test('отказ журнала отвергает и пачки той же проверки после него: они могли опираться на отвергнутую', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 50 });
  await log.submit(item({ start: { '77': 0 }, end: { '77': 1 } }));
  // Чужой пир отвергнут; следующая пачка той же цепочки проверки — тоже, даже в той же группе.
  const [forged, dependent] = await Promise.all([
    log.submit(item({ user: other.id, chain: 'k#1', start: { '77': 1 }, end: { '77': 2 } })),
    log.submit(item({ user: other.id, chain: 'k#1', start: { '5': 0 }, end: { '5': 1 } })),
  ]);
  expect(forged).toEqual({ kind: 'refused', code: 'FOREIGN_PEER' });
  expect(dependent).toEqual({ kind: 'refused', code: 'GAP' });
  // И в следующей группе.
  expect(await log.submit(item({ user: other.id, chain: 'k#1', start: { '6': 0 }, end: { '6': 1 } }))).toEqual({
    kind: 'refused',
    code: 'GAP',
  });
  // Новая цепочка (документ загружен заново) пишется.
  expect(await log.submit(item({ user: other.id, chain: 'k#2', start: { '5': 0 }, end: { '5': 1 } }))).toMatchObject({
    kind: 'written',
  });
  await log.stop();
});

test('остановка дописывает принятое', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 10_000 });
  const pending = log.submit(item());
  await log.stop();
  expect(await pending).toEqual({ kind: 'written', seq: 1 });
  expect(await log.submit(item())).toEqual({ kind: 'refused', code: 'UNAVAILABLE' });
});
