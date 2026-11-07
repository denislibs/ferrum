import { afterAll, beforeEach, expect, test } from 'bun:test';
import { copyFileSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { migrate } from '../src/migrate';
import { resetDatabase, testPool } from './helpers';

const pool = testPool();
beforeEach(() => resetDatabase(pool));
afterAll(() => pool.end());

function dir(files: Record<string, string>): string {
  const d = mkdtempSync(join(tmpdir(), 'ferrum-migrate-'));
  for (const [name, sql] of Object.entries(files)) writeFileSync(join(d, name), sql);
  return d;
}

test('применяет по порядку и только один раз', async () => {
  const d = dir({ '002_b.sql': 'INSERT INTO a VALUES (2);', '001_a.sql': 'CREATE TABLE a (v int); INSERT INTO a VALUES (1);' });
  expect(await migrate(pool, d)).toEqual(['001_a.sql', '002_b.sql']);
  expect(await migrate(pool, d)).toEqual([]);
  expect((await pool.query('SELECT v FROM a ORDER BY v')).rows).toEqual([{ v: 1 }, { v: 2 }]);
});

test('упавшая миграция откатывается целиком, следующие не идут', async () => {
  const d = dir({
    '001_a.sql': 'CREATE TABLE a (v int);',
    '002_b.sql': 'CREATE TABLE b (v int); SELECT nonsense;',
    '003_c.sql': 'CREATE TABLE c (v int);',
  });
  await expect(migrate(pool, d)).rejects.toThrow('«002_b.sql» не применилась');
  const tables = (await pool.query("SELECT table_name AS t FROM information_schema.tables WHERE table_schema = 'public'")).rows.map(
    (r) => r.t,
  );
  expect(tables).toContain('a');
  expect(tables).not.toContain('b');
  expect(tables).not.toContain('c');
  expect((await pool.query('SELECT name FROM schema_migration')).rows).toEqual([{ name: '001_a.sql' }]);
});

test('два процесса одновременно: блокировка не даёт применить дважды', async () => {
  const d = dir({ '001_a.sql': 'CREATE TABLE a (v int); SELECT pg_sleep(0.2);', '002_b.sql': 'INSERT INTO a VALUES (1);' });
  const [x, y] = await Promise.all([migrate(pool, d), migrate(pool, d)]);
  expect([...x, ...y].sort()).toEqual(['001_a.sql', '002_b.sql']);
  expect((await pool.query('SELECT count(*)::int AS n FROM a')).rows[0].n).toBe(1);
});

test('имя без номера — отказ до всякой записи', async () => {
  await expect(migrate(pool, dir({ 'init.sql': 'SELECT 1;' }))).rejects.toThrow('нужно имя вида');
});

test('миграции репозитория применяются на пустой базе', async () => {
  const applied = await migrate(pool, join(import.meta.dir, '../migrations'));
  expect(applied).toEqual([
    '001_init.sql',
    '002_better_auth.sql',
    '003_files.sql',
    '004_sync.sql',
    '005_compact_doc.sql',
    '006_log_kept.sql',
    '007_snapshot_peers.sql',
    '008_log_kept_bytes.sql',
    '009_content_by.sql',
    '010_comments.sql',
  ]);
});

test('файлы встают на базу этапа 1: люди целы, повторный запуск ничего не применяет', async () => {
  const all = join(import.meta.dir, '../migrations');
  const stage1 = mkdtempSync(join(tmpdir(), 'ferrum-stage1-'));
  for (const f of ['001_init.sql', '002_better_auth.sql']) copyFileSync(join(all, f), join(stage1, f));
  await migrate(pool, stage1);
  await pool.query(`INSERT INTO "user" (id, name, email, "emailVerified") VALUES ('u1', 'Иван', 'ivan@example.com', true)`);
  expect(await migrate(pool, all)).toEqual([
    '003_files.sql',
    '004_sync.sql',
    '005_compact_doc.sql',
    '006_log_kept.sql',
    '007_snapshot_peers.sql',
    '008_log_kept_bytes.sql',
    '009_content_by.sql',
    '010_comments.sql',
  ]);
  expect((await pool.query('SELECT email FROM "user"')).rows).toEqual([{ email: 'ivan@example.com' }]);
  const { rows } = await pool.query(`INSERT INTO file (owner_id, name) VALUES ('u1', 'Файл') RETURNING version`);
  expect(rows[0].version).toBe('0');
  expect(await migrate(pool, all)).toEqual([]);
});
