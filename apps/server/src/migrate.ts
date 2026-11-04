// Миграции: нумерованные SQL-файлы (`001_имя.sql`) применяются по порядку при запуске, каждый
// в своей транзакции, под блокировкой Postgres: два процесса не применят один файл дважды.

import { readdir } from 'node:fs/promises';
import { join } from 'node:path';
import type { Pool } from 'pg';

/** Номер блокировки `pg_advisory_lock`: любое постоянное число, своё у миграций. */
const LOCK = 7_204_911;
const NAME_RE = /^\d{3}_[a-z0-9_]+\.sql$/;

export async function migrate(pool: Pool, dir: string): Promise<string[]> {
  const files = (await readdir(dir)).filter((f) => f.endsWith('.sql')).sort();
  for (const f of files) if (!NAME_RE.test(f)) throw new Error(`Миграция «${f}»: нужно имя вида 001_имя.sql`);
  const client = await pool.connect();
  try {
    await client.query('SELECT pg_advisory_lock($1)', [LOCK]);
    await client.query(
      'CREATE TABLE IF NOT EXISTS schema_migration (name text PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now())',
    );
    const done = new Set((await client.query<{ name: string }>('SELECT name FROM schema_migration')).rows.map((r) => r.name));
    const applied: string[] = [];
    for (const f of files) {
      if (done.has(f)) continue;
      await client.query('BEGIN');
      try {
        await client.query(await Bun.file(join(dir, f)).text());
        await client.query('INSERT INTO schema_migration (name) VALUES ($1)', [f]);
        await client.query('COMMIT');
      } catch (e) {
        await client.query('ROLLBACK');
        throw new Error(`Миграция «${f}» не применилась: ${e instanceof Error ? e.message : String(e)}`);
      }
      applied.push(f);
    }
    return applied;
  } finally {
    await client.query('SELECT pg_advisory_unlock($1)', [LOCK]).catch(() => {});
    client.release();
  }
}
