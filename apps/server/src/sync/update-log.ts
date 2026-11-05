// Журнал обновлений (спека подсистемы 4, раздел 7.3): пачки собираются группами каждые 20 мс одной транзакцией;
// `ack` и рассылка — только после фиксации. Источник правды о порядке — строка `doc_state` под блокировкой: порядок
// журнала, повторы и непрерывность по пирам держатся и у двух процессов на одной базе.

import type { SyncRefusal } from '@ferrum/protocol';
import { ROLE_RANK } from '@ferrum/protocol';
import type { Pool, PoolClient } from 'pg';
import { inTransaction } from '../db';

export const GROUP_MS = 20;
/** Группа не больше этого: большая вставка не держит транзакцию дольше срока запроса. */
const MAX_GROUP_BYTES = 32 * 1024 * 1024;
const MAX_GROUP_ITEMS = 2000;
/** Журнал одного файла. Сжатие держит его намного меньше; если оно не удаётся, правки ждут у клиента, а диск цел. */
export const MAX_LOG_BYTES_PER_FILE = 1024 * 1024 * 1024;
/** Документов в файле: страниц столько не бывает, чужой клиент не заводит строки без конца. */
export const MAX_DOCS_PER_FILE = 10_000;

export interface Item {
  file: string;
  doc: string;
  user: string;
  bytes: Uint8Array;
  start: Record<string, number>;
  end: Record<string, number>;
}

export type Outcome =
  | { kind: 'written'; seq: number }
  /** Сервер всё это уже знал: повтор. */
  | { kind: 'known'; seq: number }
  | { kind: 'refused'; code: SyncRefusal };

interface Queued {
  item: Item;
  done: (o: Outcome) => void;
}

export interface LogLimits {
  maxLogBytesPerFile: number;
  groupMs: number;
}

interface DocRow {
  file_id: string;
  doc: string;
  next_seq: string;
  vv: Record<string, number>;
  log_bytes: string;
  log_count: number;
}

const covers = (vv: Record<string, number>, end: Record<string, number>): boolean =>
  Object.entries(end).every(([p, c]) => (vv[p] ?? 0) >= c);

/** Пачка продолжает вектор: каждый её пир начинается не дальше, чем знает сервер. */
const continues = (vv: Record<string, number>, start: Record<string, number>): boolean =>
  Object.entries(start).every(([p, c]) => (vv[p] ?? 0) >= c);

export class UpdateLog {
  private queue: Queued[] = [];
  private timer: Timer | null = null;
  private flushing = false;
  private stopped = false;
  /** После фиксации группы: рассылка и сжатие. */
  onCommitted: (items: { item: Item; seq: number }[], states: { file: string; doc: string; logCount: number; logBytes: number }[]) => void =
    () => {};
  private readonly limits: LogLimits;

  constructor(
    private readonly pool: Pool,
    limits: Partial<LogLimits> = {},
  ) {
    this.limits = { maxLogBytesPerFile: MAX_LOG_BYTES_PER_FILE, groupMs: GROUP_MS, ...limits };
  }

  submit(item: Item): Promise<Outcome> {
    if (this.stopped) return Promise.resolve({ kind: 'refused', code: 'UNAVAILABLE' });
    return new Promise((done) => {
      this.queue.push({ item, done });
      this.arm();
    });
  }

  private arm(): void {
    if (this.timer || this.flushing || this.queue.length === 0) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.flush();
    }, this.limits.groupMs);
  }

  private take(): Queued[] {
    let size = 0;
    let n = 0;
    while (n < this.queue.length && n < MAX_GROUP_ITEMS && (n === 0 || size + this.queue[n].item.bytes.length <= MAX_GROUP_BYTES)) {
      size += this.queue[n].item.bytes.length;
      n++;
    }
    return this.queue.splice(0, n);
  }

  /** По группе за раз: порядок `seq` — порядок фиксации. Сбой базы отказывает группе «повторите», процесс живёт. */
  private async flush(): Promise<void> {
    this.flushing = true;
    const group = this.take();
    let outcomes: Outcome[];
    let states: DocRow[] = [];
    try {
      const r = await inTransaction(this.pool, (c) => this.write(c, group));
      outcomes = r.outcomes;
      states = r.states;
    } catch (e) {
      console.error('Журнал обновлений: группа не записана:', e instanceof Error ? e.message : e);
      outcomes = group.map(() => ({ kind: 'refused', code: 'UNAVAILABLE' }));
    }
    this.flushing = false;
    const written: { item: Item; seq: number }[] = [];
    group.forEach((q, i) => {
      const o = outcomes[i];
      if (o.kind === 'written') written.push({ item: q.item, seq: o.seq });
    });
    try {
      if (written.length > 0) {
        this.onCommitted(
          written,
          states.map((s) => ({ file: s.file_id, doc: s.doc, logCount: s.log_count, logBytes: Number(s.log_bytes) })),
        );
      }
    } catch (e) {
      console.error('Журнал обновлений: рассылка:', e instanceof Error ? e.message : e);
    }
    group.forEach((q, i) => q.done(outcomes[i]));
    this.arm();
  }

  private async write(c: PoolClient, group: Queued[]): Promise<{ outcomes: Outcome[]; states: DocRow[] }> {
    const pairs = [...new Map(group.map((q) => [`${q.item.file}/${q.item.doc}`, q.item])).values()];
    // Первая запись в документ, снимка которого нет (новая страница). Счётчик документов — ниже, под блокировкой строк.
    await c.query(
      `INSERT INTO doc_state (file_id, doc) SELECT f, d FROM unnest($1::uuid[], $2::text[]) AS u(f, d)
       ON CONFLICT DO NOTHING`,
      [pairs.map((p) => p.file), pairs.map((p) => p.doc)],
    );
    // Порядок блокировок один на всех: два процесса не сцепятся.
    const { rows } = await c.query<DocRow>(
      `SELECT s.file_id, s.doc, s.next_seq, s.vv, s.log_bytes, s.log_count FROM doc_state s
       JOIN unnest($1::uuid[], $2::text[]) AS u(f, d) ON u.f = s.file_id AND u.d = s.doc
       ORDER BY s.file_id, s.doc FOR UPDATE OF s`,
      [pairs.map((p) => p.file), pairs.map((p) => p.doc)],
    );
    const state = new Map(rows.map((r) => [`${r.file_id}/${r.doc}`, { ...r, next: Number(r.next_seq), dirty: false }]));
    const ranks = await this.ranks(c, group);
    const logBytes = await this.fileLogBytes(c, group);
    const docCount = await this.docCounts(c, group);
    const owners = await this.peers(c, group);
    const claims: { file: string; peer: string; user: string }[] = [];
    const outcomes: Outcome[] = [];
    const inserts: { item: Item; seq: number }[] = [];
    for (const { item } of group) {
      const s = state.get(`${item.file}/${item.doc}`)!;
      const rank = ranks.get(`${item.file}/${item.user}`) ?? 0;
      if (rank < ROLE_RANK.editor) {
        outcomes.push({ kind: 'refused', code: 'FORBIDDEN' });
        continue;
      }
      if ((docCount.get(item.file) ?? 0) > MAX_DOCS_PER_FILE) {
        outcomes.push({ kind: 'refused', code: 'BAD_UPDATE' });
        continue;
      }
      // Чужой пир: иначе подделка заняла бы чужие счётчики, и настоящие правки того человека Loro бы пропускал.
      const peers = Object.keys(item.end);
      if (peers.some((p) => (owners.get(`${item.file}/${p}`) ?? item.user) !== item.user)) {
        outcomes.push({ kind: 'refused', code: 'FOREIGN_PEER' });
        continue;
      }
      if (covers(s.vv, item.end)) {
        outcomes.push({ kind: 'known', seq: s.next - 1 });
        continue;
      }
      if (!continues(s.vv, item.start)) {
        outcomes.push({ kind: 'refused', code: 'GAP' });
        continue;
      }
      const used = logBytes.get(item.file) ?? 0;
      if (used + item.bytes.length > this.limits.maxLogBytesPerFile) {
        outcomes.push({ kind: 'refused', code: 'LOG_FULL' });
        continue;
      }
      logBytes.set(item.file, used + item.bytes.length);
      for (const p of peers) {
        if (!owners.has(`${item.file}/${p}`)) {
          owners.set(`${item.file}/${p}`, item.user);
          claims.push({ file: item.file, peer: p, user: item.user });
        }
        s.vv[p] = Math.max(s.vv[p] ?? 0, item.end[p]);
      }
      const seq = s.next++;
      s.dirty = true;
      s.log_count += 1;
      s.log_bytes = String(Number(s.log_bytes) + item.bytes.length);
      inserts.push({ item, seq });
      outcomes.push({ kind: 'written', seq });
    }
    if (claims.length > 0) {
      await c.query(
        `INSERT INTO doc_peer (file_id, peer, user_id)
         SELECT f, p::numeric, u FROM unnest($1::uuid[], $2::text[], $3::text[]) AS x(f, p, u) ON CONFLICT DO NOTHING`,
        [claims.map((x) => x.file), claims.map((x) => x.peer), claims.map((x) => x.user)],
      );
    }
    if (inserts.length > 0) {
      const values = inserts.map(
        (_, i) =>
          `($${i * 6 + 1}::uuid, $${i * 6 + 2}, $${i * 6 + 3}::bigint, $${i * 6 + 4}::jsonb, $${i * 6 + 5}::jsonb, $${i * 6 + 6}::bytea)`,
      );
      await c.query(
        `INSERT INTO doc_update (file_id, doc, seq, start_vv, end_vv, bytes) VALUES ${values.join(',')}`,
        inserts.flatMap((i) => [
          i.item.file,
          i.item.doc,
          String(i.seq),
          JSON.stringify(i.item.start),
          JSON.stringify(i.item.end),
          Buffer.from(i.item.bytes.buffer, i.item.bytes.byteOffset, i.item.bytes.byteLength),
        ]),
      );
      const dirty = [...state.values()].filter((s) => s.dirty);
      await c.query(
        `UPDATE doc_state s SET next_seq = u.n::bigint, vv = u.v::jsonb, log_count = u.c::integer, log_bytes = u.b::bigint
         FROM unnest($1::uuid[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[]) AS u(f, d, n, v, c, b)
         WHERE s.file_id = u.f AND s.doc = u.d`,
        [
          dirty.map((s) => s.file_id),
          dirty.map((s) => s.doc),
          dirty.map((s) => String(s.next)),
          dirty.map((s) => JSON.stringify(s.vv)),
          dirty.map((s) => String(s.log_count)),
          dirty.map((s) => s.log_bytes),
        ],
      );
      await c.query('UPDATE file SET edited_at = now() WHERE id = ANY($1::uuid[])', [[...new Set(inserts.map((i) => i.item.file))]]);
    }
    return { outcomes, states: rows };
  }

  /** Роль проверяется в транзакции записи на каждой группе: снятая только что не запишет ни байта. */
  private async ranks(c: PoolClient, group: Queued[]): Promise<Map<string, number>> {
    const pairs = [...new Map(group.map((q) => [`${q.item.file}/${q.item.user}`, q.item])).values()];
    const { rows } = await c.query<{ f: string; u: string; rank: number | null }>(
      `SELECT f, u, file_role_rank(f, u) AS rank FROM unnest($1::uuid[], $2::text[]) AS x(f, u)`,
      [pairs.map((p) => p.file), pairs.map((p) => p.user)],
    );
    return new Map(rows.map((r) => [`${r.f}/${r.u}`, r.rank ?? 0]));
  }

  private async fileLogBytes(c: PoolClient, group: Queued[]): Promise<Map<string, number>> {
    const files = [...new Set(group.map((q) => q.item.file))];
    const { rows } = await c.query<{ file_id: string; total: string }>(
      'SELECT file_id, sum(log_bytes) AS total FROM doc_state WHERE file_id = ANY($1::uuid[]) GROUP BY file_id',
      [files],
    );
    return new Map(rows.map((r) => [r.file_id, Number(r.total)]));
  }

  private async docCounts(c: PoolClient, group: Queued[]): Promise<Map<string, number>> {
    const files = [...new Set(group.map((q) => q.item.file))];
    const { rows } = await c.query<{ file_id: string; n: string }>(
      'SELECT file_id, count(*) AS n FROM doc_state WHERE file_id = ANY($1::uuid[]) GROUP BY file_id',
      [files],
    );
    return new Map(rows.map((r) => [r.file_id, Number(r.n)]));
  }

  private async peers(c: PoolClient, group: Queued[]): Promise<Map<string, string>> {
    const asked = [...new Set(group.flatMap((q) => Object.keys(q.item.end).map((p) => `${q.item.file}/${p}`)))];
    if (asked.length === 0) return new Map();
    const { rows } = await c.query<{ file_id: string; peer: string; user_id: string }>(
      `SELECT p.file_id, p.peer::text AS peer, p.user_id FROM doc_peer p
       JOIN unnest($1::uuid[], $2::numeric[]) AS x(f, n) ON x.f = p.file_id AND x.n = p.peer`,
      [asked.map((a) => a.split('/')[0]), asked.map((a) => a.split('/')[1])],
    );
    return new Map(rows.map((r) => [`${r.file_id}/${r.peer}`, r.user_id]));
  }

  /** Остановка: всё принятое дописывается. */
  async stop(): Promise<void> {
    this.stopped = true;
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    while (this.queue.length > 0 || this.flushing) {
      if (!this.flushing) await this.flush();
      else await Bun.sleep(5);
    }
  }
}
