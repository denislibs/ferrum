// Сжатие файла (спека подсистемы 4, раздел 7.4): снимок и журнал документов становятся новым контейнером одной транзакцией
// с удалением вошедших записей. Сбой посередине ничего не теряет: прежнее содержимое и журнал остаются. Обновление, которое не
// применилось, не удаляется и файл не заменяет; следующая попытка через час, приём правок при этом идёт дальше.
//
// Функция без глобального состояния: её вызывает `compact.worker.ts` (Bun Worker, память wasm не возвращается системе) и тесты.

import { MAX_CONTENT_BYTES, ContainerError, containerHeader, scanContainer, SECTION_IMAGE, SECTION_PAGE_REFS } from '@ferrum/protocol';
import { blake3 } from '@noble/hashes/blake3.js';
import { LoroDoc, type LoroMap } from 'loro-crdt';
import { randomUUID } from 'node:crypto';
import type { Pool } from 'pg';
import type { BlobStore } from '../blob-store';
import { inTransaction } from '../db';

const SECTION_ROOT = 1;
const SECTION_PAGE = 2;
const SECTION_SCENE = 5;
/** Записей журнала за один запрос: байты идут в wasm и сразу освобождаются. */
const ROWS_AT_ONCE = 256;

export type CompactResult =
  | { kind: 'done'; docs: number; size: number; rows: number }
  /** Журнал пуст: сжимать нечего. */
  | { kind: 'empty' }
  /** Обновление не применилось; файл не тронут. */
  | { kind: 'failed'; doc: string; seq: number }
  /** Файл заменили, пока шла работа. Журнал цел. */
  | { kind: 'raced' };

interface Section {
  kind: number;
  id: string;
  /** Откуда копировать без чтения в память; `null` — новые данные. */
  from: { start: number; end: number } | null;
  data: Uint8Array | null;
}

interface DocRow {
  doc: string;
  next_seq: string;
  log_count: number;
}

function imageRefs(doc: LoroDoc): string[] {
  const refs = new Set<string>();
  const tree = doc.getTree('nodes');
  for (const node of tree.getNodes({ withDeleted: false })) {
    const fills = (node.data as LoroMap).get('fills');
    if (!Array.isArray(fills)) continue;
    for (const f of fills as { imageHash?: unknown }[]) {
      if (typeof f?.imageHash === 'string') refs.add(f.imageHash);
    }
  }
  return [...refs].sort();
}

function pageIds(root: LoroDoc): { listed: Set<string>; gone: Set<string> } {
  const tree = root.getTree('pages');
  const listed = new Set<string>(tree.roots().map((n) => String(n.id)));
  const gone = new Set<string>();
  for (const n of tree.getNodes({ withDeleted: true })) {
    if (tree.isNodeDeleted(n.id)) gone.add(String(n.id));
  }
  return { listed, gone };
}

const plain = (d: LoroDoc): Record<string, number> => Object.fromEntries(d.oplogVersion().toJSON()) as Record<string, number>;

/** Сжать файл. Журнал читается до `next_seq` на момент начала: новые записи остаются на следующий раз. */
export async function compactFile(pool: Pool, blobs: BlobStore, file: string, maxContent = MAX_CONTENT_BYTES): Promise<CompactResult> {
  const fileRow = await pool.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [file]);
  const contentKey = fileRow.rows[0]?.content_key ?? null;
  if (!fileRow.rows[0]) return { kind: 'empty' };
  const { rows: states } = await pool.query<DocRow>('SELECT doc, next_seq, log_count FROM doc_state WHERE file_id = $1', [file]);
  const logged = states.filter((s) => s.log_count > 0);
  if (logged.length === 0) return { kind: 'empty' };

  const blob = contentKey ? await blobs.open(contentKey) : null;
  if (contentKey && !blob) throw new Error(`нет байтов содержимого файла ${file}`);
  const read = async (s: number, e: number) => new Uint8Array(await blob!.slice(s, e).arrayBuffer());
  let spans: Awaited<ReturnType<typeof scanContainer>> = [];
  if (blob) {
    try {
      spans = await scanContainer(blob.size, read);
    } catch (e) {
      if (!(e instanceof ContainerError)) throw e;
      throw new Error(`содержимое файла ${file} не контейнер: ${e.message}`);
    }
  }
  const dataOf = async (kind: number, id: string): Promise<Uint8Array | null> => {
    const s = spans.find((x) => x.kind === kind && x.id === id);
    return s ? read(s.data, s.data + s.size) : null;
  };

  /** Документ со снимком и журналом до `cut`. Не применилось — `failed`, файл не трогаем. */
  const build = async (
    doc: string,
    kind: number,
    id: string,
    cut: number,
  ): Promise<{ loro: LoroDoc; rows: number } | { doc: string; seq: number }> => {
    const snapshot = await dataOf(kind, id);
    const loro = snapshot && snapshot.length > 0 ? LoroDoc.fromSnapshot(snapshot) : new LoroDoc();
    let rows = 0;
    let after = 0;
    for (;;) {
      const page = await pool.query<{ seq: string; bytes: Uint8Array }>(
        'SELECT seq, bytes FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq > $3 AND seq <= $4 ORDER BY seq LIMIT $5',
        [file, doc, after, cut, ROWS_AT_ONCE],
      );
      if (page.rows.length === 0) break;
      for (const r of page.rows) {
        after = Number(r.seq);
        try {
          const status = loro.import(r.bytes);
          if (status.pending != null) return { doc, seq: after };
        } catch {
          return { doc, seq: after };
        }
        rows++;
      }
    }
    return { loro, rows };
  };

  const stateOf = new Map(states.map((s) => [s.doc, s]));
  const cutOf = (doc: string): number => Number(stateOf.get(doc)?.next_seq ?? 1) - 1;

  const rootBuilt = await build('root', SECTION_ROOT, '', cutOf('root'));
  if (!('loro' in rootBuilt)) return { kind: 'failed', ...rootBuilt };
  const { listed, gone } = pageIds(rootBuilt.loro);
  let totalRows = rootBuilt.rows;

  const replaced = new Map<string, { snapshot: Uint8Array; refs: string[]; version: Record<string, number> }>();
  for (const s of logged) {
    if (s.doc === 'root' || !listed.has(s.doc)) continue;
    const built = await build(s.doc, SECTION_PAGE, s.doc, cutOf(s.doc));
    if (!('loro' in built)) return { kind: 'failed', ...built };
    totalRows += built.rows;
    replaced.set(s.doc, { snapshot: built.loro.export({ mode: 'snapshot' }), refs: imageRefs(built.loro), version: plain(built.loro) });
    built.loro.free();
  }
  const rootVersion = plain(rootBuilt.loro);
  const rootSnapshot = rootBuilt.loro.export({ mode: 'snapshot' });
  rootBuilt.loro.free();

  // Секции нового контейнера: неизменённые — копией куска прежнего файла, в памяти только новые снимки.
  const sections: Section[] = [{ kind: SECTION_ROOT, id: '', from: null, data: rootSnapshot }];
  for (const s of spans) {
    const copy = { start: s.start, end: s.data + s.size };
    if (s.kind === SECTION_ROOT || s.kind === SECTION_IMAGE) continue;
    // Удалённая страница уходит; страница, которой корень ещё не знает, остаётся: корень мог не дойти.
    if ((s.kind === SECTION_PAGE || s.kind === SECTION_PAGE_REFS || s.kind === SECTION_SCENE) && (gone.has(s.id) || replaced.has(s.id)))
      continue;
    sections.push({ kind: s.kind, id: s.id, from: copy, data: null });
  }
  // Страница без единой правки (только что добавлена): ни журнала, ни секции. Ядро приняло бы отсутствие секции за повреждение файла.
  for (const id of listed) {
    if (replaced.has(id) || spans.some((x) => x.kind === SECTION_PAGE && x.id === id)) continue;
    sections.push({ kind: SECTION_PAGE, id, from: null, data: new LoroDoc().export({ mode: 'snapshot' }) });
    sections.push({ kind: SECTION_PAGE_REFS, id, from: null, data: new Uint8Array(0) });
  }
  for (const [id, r] of replaced) {
    sections.push({ kind: SECTION_PAGE, id, from: null, data: r.snapshot });
    sections.push({ kind: SECTION_PAGE_REFS, id, from: null, data: new TextEncoder().encode(r.refs.join('\n')) });
  }
  const refs = new Set<string>();
  for (const r of replaced.values()) for (const h of r.refs) refs.add(h);
  for (const s of spans) {
    if (s.kind !== SECTION_PAGE_REFS || replaced.has(s.id) || gone.has(s.id)) continue;
    for (const line of new TextDecoder().decode(await read(s.data, s.data + s.size)).split('\n')) if (line) refs.add(line);
  }

  const key = `files/${file}/content-${randomUUID()}`;
  await pool.query('INSERT INTO blob_pending (key) VALUES ($1) ON CONFLICT DO NOTHING', [key]);
  const head = containerHeader(sections.length);
  const body = new ReadableStream<Uint8Array>({
    async start(c) {
      c.enqueue(head);
      for (const s of sections) {
        if (s.from) c.enqueue(await read(s.from.start, s.from.end));
        else c.enqueue(sectionBytes(s.kind, s.id, s.data!));
      }
      c.close();
    },
  });
  const { size } = await blobs.putStream(key, body, { maxBytes: maxContent, idleMs: 60_000 });

  const outcome = await inTransaction(pool, async (c) => {
    const locked = await c.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1 FOR UPDATE', [file]);
    if (!locked.rows[0] || locked.rows[0].content_key !== contentKey) return 'raced' as const;
    const pending = await c.query('DELETE FROM blob_pending WHERE key = $1', [key]);
    if (pending.rowCount === 0) return 'raced' as const;
    if (contentKey) {
      await c.query(`INSERT INTO blob_trash (key, delete_after) VALUES ($1, now() + interval '2 minutes') ON CONFLICT DO NOTHING`, [
        contentKey,
      ]);
    }
    await c.query('UPDATE file SET version = version + 1, size = $2, content_key = $3, compact_after = NULL WHERE id = $1', [
      file,
      size,
      key,
    ]);
    // Вошедшие записи — той же транзакцией: записи новее `cut` остаются, журнал и снимок не расходятся.
    const done = [{ doc: 'root', version: rootVersion }, ...[...replaced].map(([doc, r]) => ({ doc, version: r.version }))];
    for (const d of done) {
      if (!stateOf.has(d.doc)) continue;
      const removed = await c.query<{ size: number }>(
        'DELETE FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq <= $3 RETURNING octet_length(bytes) AS size',
        [file, d.doc, cutOf(d.doc)],
      );
      await c.query(
        `UPDATE doc_state SET base_vv = $3::jsonb, log_count = log_count - $4, log_bytes = log_bytes - $5 WHERE file_id = $1 AND doc = $2`,
        [file, d.doc, JSON.stringify(d.version), removed.rowCount ?? 0, removed.rows.reduce((n, r) => n + r.size, 0)],
      );
    }
    for (const doc of gone) {
      await c.query('DELETE FROM doc_state WHERE file_id = $1 AND doc = $2', [file, doc]);
    }
    await c.query(
      `UPDATE file_image SET unreferenced_at = CASE WHEN hash = ANY($2::text[]) THEN NULL ELSE COALESCE(unreferenced_at, now()) END WHERE file_id = $1`,
      [file, [...refs]],
    );
    return 'done' as const;
  });
  if (outcome === 'raced') {
    await blobs.delete(key).catch(() => {});
    await pool.query('DELETE FROM blob_pending WHERE key = $1', [key]).catch(() => {});
    return { kind: 'raced' };
  }
  return { kind: 'done', docs: replaced.size + 1, size, rows: totalRows };
}

function sectionBytes(kind: number, id: string, data: Uint8Array): Uint8Array {
  const idb = new TextEncoder().encode(id);
  const out = new Uint8Array(5 + idb.length + 40 + data.length);
  const v = new DataView(out.buffer);
  out[0] = kind;
  v.setUint32(1, idb.length, true);
  out.set(idb, 5);
  v.setBigUint64(5 + idb.length, BigInt(data.length), true);
  out.set(blake3(data), 13 + idb.length);
  out.set(data, 45 + idb.length);
  return out;
}
