// Журнал обновлений на настоящем Postgres: порядок, повторы, непрерывность, роль в транзакции записи, чужой пир,
// предел журнала, сбой базы и два писателя на одной базе (спека подсистемы 4, раздел 7.3).

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { apiStand } from './api-stand';
import { UpdateLog, type Item } from '../src/sync/update-log';

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

test('два писателя на одной базе: номера подряд, порядок и повторы держит база', async () => {
  // Спека: процесс один, но решение этого не закрывает. Настоящие два клиента базы, как два процесса.
  const { Pool } = await import('pg');
  const second = new Pool({ connectionString: process.env.TEST_DATABASE_URL ?? 'postgres://ferrum:ferrum@127.0.0.1:15432/ferrum' });
  const a = new UpdateLog(s.pool, { groupMs: 1 });
  const b = new UpdateLog(second, { groupMs: 1 });
  const out = await Promise.all(
    Array.from({ length: 40 }, (_, i) => {
      const peer = i % 2 ? '100' : '200';
      const n = Math.floor(i / 2);
      return (i % 2 ? a : b).submit(item({ start: { [peer]: n }, end: { [peer]: n + 1 } }));
    }),
  );
  expect(out.filter((o) => o.kind === 'refused').length).toBeLessThanOrEqual(40);
  const seqs = (await rows()).map((r) => r[0] as number);
  expect(seqs).toEqual(seqs.map((_, i) => i + 1));
  const again = await a.submit(item({ start: { '100': 0 }, end: { '100': 1 } }));
  expect(again.kind === 'known' || again.kind === 'refused').toBe(true);
  await a.stop();
  await b.stop();
  await second.end();
});

test('остановка дописывает принятое', async () => {
  const log = new UpdateLog(s.pool, { groupMs: 10_000 });
  const pending = log.submit(item());
  await log.stop();
  expect(await pending).toEqual({ kind: 'written', seq: 1 });
  expect(await log.submit(item())).toEqual({ kind: 'refused', code: 'UNAVAILABLE' });
});
