// Журнал обновлений (спека подсистемы 4, раздел 7.3): пачки собираются группами одной транзакцией; `ack` и рассылка — только
// после фиксации. Окна в 20 мс нет: оно одно съедало половину цели §13.1 (40 мс от кадра до рассылки). Группа — пачки одного
// тика (окно `GROUP_MS`) и всё, что пришло, пока шла прошлая транзакция: под нагрузкой транзакций столько же, сколько с окном. Источник правды о порядке — строка `doc_state` под блокировкой: порядок
// журнала, повторы и непрерывность по пирам держатся и у двух процессов на одной базе. Блокировки — в одном порядке со
// сжатием и записью файла целиком: строка файла, потом строки документов.

import type { SyncRefusal } from '@ferrum/protocol';
import { ROLE_RANK } from '@ferrum/protocol';
import type { Pool, PoolClient } from 'pg';
import { inTransaction } from '../db';

/** Окно сбора группы после первой пачки: соседи по тику идут одной транзакцией, а не ждут чужую. */
export const GROUP_MS = 1;
/** `file.edited_at` не чаще: строка файла горячая, а время правки на экране файлов точнее не нужно. */
const EDITED_EVERY = '1 second';
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
  /** Документ проверки, на котором пачка проверена (`Validator`): отказ журнала одной пачке отвергает и следующие этой же
   * цепочки — их проверяли на документе, где уже была отвергнутая. */
  chain?: string;
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
  maxDocsPerFile: number;
  groupMs: number;
}

/** Столько отвергнутых цепочек помнит журнал: их пачки в очереди отказывают, новые цепочки идут от загрузки заново. */
const MAX_POISONED = 4096;

interface DocRow {
  file_id: string;
  doc: string;
  next_seq: string;
  vv: Record<string, number>;
  log_bytes: string;
  log_count: number;
  /** Записи, которые сжатие оставило в журнале неприменимыми: порог сжатия их не считает. */
  log_kept: number;
}

const covers = (vv: Record<string, number>, end: Record<string, number>): boolean =>
  Object.entries(end).every(([p, c]) => (vv[p] ?? 0) >= c);

/** Пачка продолжает вектор: каждый её пир начинается не дальше, чем знает сервер. */
const continues = (vv: Record<string, number>, start: Record<string, number>): boolean =>
  Object.entries(start).every(([p, c]) => (vv[p] ?? 0) >= c);

export class UpdateLog {
  private queue: Queued[] = [];
  private readonly poisoned = new Set<string>();
  private timer: Timer | null = null;
  private flushing = false;
  private stopped = false;
  /** После фиксации группы: рассылка и сжатие. Размеры журнала — вместе с этой группой; число записей — без тех, что сжатие
   * оставило неприменимыми. */
  onCommitted: (items: { item: Item; seq: number }[], states: { file: string; doc: string; logCount: number; logBytes: number }[]) => void =
    () => {};
  private readonly limits: LogLimits;

  constructor(
    private readonly pool: Pool,
    limits: Partial<LogLimits> = {},
  ) {
    this.limits = { maxLogBytesPerFile: MAX_LOG_BYTES_PER_FILE, maxDocsPerFile: MAX_DOCS_PER_FILE, groupMs: GROUP_MS, ...limits };
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
    group.forEach((q, i) => {
      if (outcomes[i].kind === 'refused') this.poison(q.item.chain);
    });
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
          states.map((s) => ({ file: s.file_id, doc: s.doc, logCount: s.log_count - s.log_kept, logBytes: Number(s.log_bytes) })),
        );
      }
    } catch (e) {
      console.error('Журнал обновлений: рассылка:', e instanceof Error ? e.message : e);
    }
    group.forEach((q, i) => q.done(outcomes[i]));
    this.arm();
  }

  /** Цепочка проверки отвергнута: её пачки, ещё ждущие в очереди, отказывают. */
  private poison(chain: string | undefined): void {
    if (!chain) return;
    this.poisoned.add(chain);
    if (this.poisoned.size > MAX_POISONED) this.poisoned.delete(this.poisoned.values().next().value as string);
  }

  private async write(c: PoolClient, group: Queued[]): Promise<{ outcomes: Outcome[]; states: DocRow[] }> {
    // Порядок блокировок один на всех (сжатие, запись файла целиком, второй процесс): строки файлов по порядку, потом документов.
    const files = [...new Set(group.map((q) => q.item.file))].sort();
    const live = await c.query<{ id: string }>('SELECT id FROM file WHERE id = ANY($1::uuid[]) ORDER BY id FOR NO KEY UPDATE', [files]);
    const existing = new Set(live.rows.map((r) => r.id));
    const ranks = await this.ranks(c, group);
    const mayWrite = (i: Item) => existing.has(i.file) && (ranks.get(`${i.file}/${i.user}`) ?? 0) >= ROLE_RANK.editor;
    const pairs = [...new Map(group.filter((q) => mayWrite(q.item)).map((q) => [`${q.item.file}/${q.item.doc}`, q.item])).values()].sort(
      (a, b) => (a.file === b.file ? (a.doc < b.doc ? -1 : 1) : a.file < b.file ? -1 : 1),
    );
    // Первая запись в документ без строки (новая страница) — только тому, кто может писать, и в пределе документов файла.
    const docCount = await this.docCounts(c, files);
    const known = await c.query<{ file_id: string; doc: string }>(
      `SELECT s.file_id, s.doc FROM doc_state s JOIN unnest($1::uuid[], $2::text[]) AS u(f, d) ON u.f = s.file_id AND u.d = s.doc`,
      [pairs.map((p) => p.file), pairs.map((p) => p.doc)],
    );
    const have = new Set(known.rows.map((r) => `${r.file_id}/${r.doc}`));
    const fresh: Item[] = [];
    for (const p of pairs) {
      if (have.has(`${p.file}/${p.doc}`)) continue;
      const n = docCount.get(p.file) ?? 0;
      if (n >= this.limits.maxDocsPerFile) continue;
      docCount.set(p.file, n + 1);
      fresh.push(p);
    }
    if (fresh.length > 0) {
      await c.query(
        `INSERT INTO doc_state (file_id, doc) SELECT f, d FROM unnest($1::uuid[], $2::text[]) AS u(f, d) ON CONFLICT DO NOTHING`,
        [fresh.map((p) => p.file), fresh.map((p) => p.doc)],
      );
    }
    const { rows } = await c.query<DocRow>(
      `SELECT s.file_id, s.doc, s.next_seq, s.vv, s.log_bytes, s.log_count, s.log_kept FROM doc_state s
       JOIN unnest($1::uuid[], $2::text[]) AS u(f, d) ON u.f = s.file_id AND u.d = s.doc
       ORDER BY s.file_id, s.doc FOR UPDATE OF s`,
      [pairs.map((p) => p.file), pairs.map((p) => p.doc)],
    );
    const state = new Map(rows.map((r) => [`${r.file_id}/${r.doc}`, { ...r, next: Number(r.next_seq), dirty: false }]));
    const logBytes = await this.fileLogBytes(c, files);
    const owners = await this.peers(c, group);
    const claims: { file: string; peer: string; user: string }[] = [];
    const outcomes: Outcome[] = [];
    const inserts: { item: Item; seq: number }[] = [];
    const refuse = (item: Item, code: SyncRefusal) => {
      outcomes.push({ kind: 'refused', code });
      this.poison(item.chain);
    };
    for (const { item } of group) {
      if (!mayWrite(item)) {
        refuse(item, 'FORBIDDEN');
        continue;
      }
      if (item.chain && this.poisoned.has(item.chain)) {
        refuse(item, 'GAP');
        continue;
      }
      const s = state.get(`${item.file}/${item.doc}`);
      // Документов в файле больше предела: новые не заводятся. Свой код: журнал не полон, и ожидание сжатия не поможет.
      if (!s) {
        refuse(item, 'DOC_LIMIT');
        continue;
      }
      // Чужой пир: иначе подделка заняла бы чужие счётчики, и настоящие правки того человека Loro бы пропускал.
      const peers = Object.keys(item.end);
      if (peers.some((p) => (owners.get(`${item.file}/${p}`) ?? item.user) !== item.user)) {
        refuse(item, 'FOREIGN_PEER');
        continue;
      }
      if (covers(s.vv, item.end)) {
        outcomes.push({ kind: 'known', seq: s.next - 1 });
        continue;
      }
      if (!continues(s.vv, item.start)) {
        refuse(item, 'GAP');
        continue;
      }
      const used = logBytes.get(item.file) ?? 0;
      if (used + item.bytes.length > this.limits.maxLogBytesPerFile) {
        refuse(item, 'LOG_FULL');
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
      await c.query(`UPDATE file SET edited_at = now() WHERE id = ANY($1::uuid[]) AND edited_at < now() - interval '${EDITED_EVERY}'`, [
        [...new Set(inserts.map((i) => i.item.file))],
      ]);
    }
    // Размеры после этой группы: сжатие замечает порог той же записью, что его перешла.
    return {
      outcomes,
      states: [...state.values()].map((s) => ({ ...s, next_seq: String(s.next), log_count: s.log_count, log_bytes: s.log_bytes })),
    };
  }

  /** Роль проверяется в транзакции записи на каждой группе. */
  private async ranks(c: PoolClient, group: Queued[]): Promise<Map<string, number>> {
    const pairs = [...new Map(group.map((q) => [`${q.item.file}/${q.item.user}`, q.item])).values()];
    const { rows } = await c.query<{ f: string; u: string; rank: number | null }>(
      `SELECT f, u, file_role_rank(f, u) AS rank FROM unnest($1::uuid[], $2::text[]) AS x(f, u)`,
      [pairs.map((p) => p.file), pairs.map((p) => p.user)],
    );
    return new Map(rows.map((r) => [`${r.f}/${r.u}`, r.rank ?? 0]));
  }

  private async fileLogBytes(c: PoolClient, files: string[]): Promise<Map<string, number>> {
    const { rows } = await c.query<{ file_id: string; total: string }>(
      'SELECT file_id, sum(log_bytes) AS total FROM doc_state WHERE file_id = ANY($1::uuid[]) GROUP BY file_id',
      [files],
    );
    return new Map(rows.map((r) => [r.file_id, Number(r.total)]));
  }

  private async docCounts(c: PoolClient, files: string[]): Promise<Map<string, number>> {
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
