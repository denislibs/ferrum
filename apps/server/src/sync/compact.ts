// Сжатие файла (спека подсистемы 4, раздел 7.4): снимок и журнал документов становятся новым контейнером одной транзакцией
// с удалением вошедших записей. Сбой посередине ничего не теряет: прежнее содержимое и журнал остаются. Обновление, которое не
// применилось, откладывает только свой документ: его снимок и журнал остаются как есть, следующая попытка через час, остальные
// документы файла сжимаются. Приём правок при этом идёт дальше. Автоматически ничего не удаляется: удалённая страница остаётся
// в контейнере, параллельный перенос в Loro может её вернуть.
//
// Функция без глобального состояния: её вызывает `compact.worker.ts` (Bun Worker, память wasm не возвращается системе) и тесты.
// В памяти — один собираемый документ: его секции сразу уходят во временный файл.

import { MAX_CONTENT_BYTES, ContainerError, containerHeader, scanContainer, SECTION_IMAGE, SECTION_PAGE_REFS } from '@ferrum/protocol';
import { blake3 } from '@noble/hashes/blake3.js';
import { LoroDoc, type LoroMap } from 'loro-crdt';
import { randomUUID } from 'node:crypto';
import { rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { Pool } from 'pg';
import type { BlobStore } from '../blob-store';
import { inTransaction } from '../db';
import { SECTION_PAGE, SECTION_ROOT, SECTION_SCENE } from './container-read';

/** Записей журнала за один запрос: байты идут в wasm и сразу освобождаются. */
const ROWS_AT_ONCE = 256;
/** Снимок и журнал одного документа больше этого не собираются (риск 4 спеки: память wasm worker'а). */
export const MAX_DOC_BYTES = 256 * 1024 * 1024;
/** Байты прежнего файла уходят в корзину на этот срок, как при записи файла целиком (`BLOB_GRACE_SECONDS`). */
const BLOB_GRACE_SECONDS = 120;

export interface CompactOptions {
  maxContent: number;
  maxDocBytes: number;
  /** Документ, который не собрался, ждёт столько секунд. */
  retryAfterSeconds: number;
}

/** Документ не собран: `seq` — запись, которая не применилась (0 — документ больше предела). */
export interface CompactFailure {
  doc: string;
  seq: number;
}

export type CompactResult =
  /** `failed` — документы, отложенные до срока; остальные сжаты. */
  | { kind: 'done'; docs: number; size: number; rows: number; failed: CompactFailure[] }
  /** Сжимать нечего: журнала нет или все документы ждут срока. */
  | { kind: 'empty' }
  /** Ни один документ не собрался; файл не тронут. */
  | { kind: 'failed'; failed: CompactFailure[] }
  /** Файл заменили, пока шла работа. Журнал цел. */
  | { kind: 'raced' };

/** Паника wasm: модуль испорчен, worker нужно заменить, документ — отложить. */
export class CompactPanic extends Error {
  constructor(
    readonly doc: string,
    readonly seq: number,
    cause: unknown,
  ) {
    super(`паника Loro: документ ${doc}, запись ${seq}: ${cause instanceof Error ? cause.message : String(cause)}`);
  }
}

const isPanic = (e: unknown): boolean => e instanceof WebAssembly.RuntimeError || (e instanceof Error && /unreachable/i.test(e.message));

interface Section {
  kind: number;
  id: string;
  /** Откуда копировать без чтения в память: прежний файл или временный файл с новыми секциями. */
  from: { src: 'old' | 'new'; start: number; end: number } | null;
  data: Uint8Array | null;
}

interface DocRow {
  doc: string;
  next_seq: string;
  log_count: number;
  log_bytes: string;
  waits: boolean;
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

/** Страницы корня: живые и удалённые. Удалённая остаётся — параллельный перенос её вернёт. */
function pageIds(root: LoroDoc): { listed: Set<string>; known: Set<string> } {
  const tree = root.getTree('pages');
  const listed = new Set<string>(tree.roots().map((n) => String(n.id)));
  const known = new Set<string>(listed);
  for (const n of tree.getNodes({ withDeleted: true })) known.add(String(n.id));
  return { listed, known };
}

const plain = (d: LoroDoc): Record<string, number> => Object.fromEntries(d.oplogVersion().toJSON()) as Record<string, number>;

/** Сжать файл. Журнал читается до `next_seq` на момент начала: новые записи остаются на следующий раз. */
export async function compactFile(
  pool: Pool,
  blobs: BlobStore,
  file: string,
  options: Partial<CompactOptions> = {},
): Promise<CompactResult> {
  const opts: CompactOptions = { maxContent: MAX_CONTENT_BYTES, maxDocBytes: MAX_DOC_BYTES, retryAfterSeconds: 3600, ...options };
  const fileRow = await pool.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [file]);
  const contentKey = fileRow.rows[0]?.content_key ?? null;
  if (!fileRow.rows[0]) return { kind: 'empty' };
  const { rows: states } = await pool.query<DocRow>(
    `SELECT doc, next_seq, log_count, log_bytes, coalesce(compact_after > now(), false) AS waits FROM doc_state WHERE file_id = $1`,
    [file],
  );
  const due = states.filter((s) => s.log_count > 0 && !s.waits);
  if (due.length === 0) return { kind: 'empty' };

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
  const spanOf = (kind: number, id: string) => spans.find((x) => x.kind === kind && x.id === id);
  const stateOf = new Map(states.map((s) => [s.doc, s]));
  const cutOf = (doc: string): number => Number(stateOf.get(doc)?.next_seq ?? 1) - 1;

  /** Документ со снимком и журналом до `cut`. Не применилось — `failed` и то, что успело собраться. */
  const build = async (
    doc: string,
    kind: number,
    id: string,
    cut: number,
  ): Promise<{ loro: LoroDoc; rows: number; failed: CompactFailure | null }> => {
    const span = spanOf(kind, id);
    const snapshot = span ? await read(span.data, span.data + span.size) : null;
    let loro: LoroDoc;
    try {
      loro = snapshot && snapshot.length > 0 ? LoroDoc.fromSnapshot(snapshot) : new LoroDoc();
    } catch (e) {
      if (isPanic(e)) throw new CompactPanic(doc, 0, e);
      throw e;
    }
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
          if (status.pending != null) return { loro, rows, failed: { doc, seq: after } };
        } catch (e) {
          if (isPanic(e)) throw new CompactPanic(doc, after, e);
          return { loro, rows, failed: { doc, seq: after } };
        }
        rows++;
      }
    }
    return { loro, rows, failed: null };
  };

  const failed: CompactFailure[] = [];
  /** Отложенные до срока: не собрались или корень их не знает. */
  const deferred = new Set<string>();
  const replaced = new Map<string, { version: Record<string, number>; refs: string[] }>();
  const fresh: Section[] = [];
  let totalRows = 0;
  const tmp = join(tmpdir(), `ferrum-compact-${randomUUID()}`);
  const sink = Bun.file(tmp).writer();
  let written = 0;
  /** Секция сразу во временный файл: в памяти её больше нет. */
  const keep = async (kind: number, id: string, data: Uint8Array) => {
    const bytes = sectionBytes(kind, id, data);
    fresh.push({ kind, id, from: { src: 'new', start: written, end: written + bytes.length }, data: null });
    sink.write(bytes);
    written += bytes.length;
    await sink.flush();
  };
  const tooLarge = (doc: string, kind: number, id: string) =>
    (spanOf(kind, id)?.size ?? 0) + Number(stateOf.get(doc)?.log_bytes ?? 0) > opts.maxDocBytes;

  try {
    // Корень нужен всегда: по нему видно, какие страницы есть. Его журнал — только если он не ждёт срока.
    const rootDue = due.some((s) => s.doc === 'root');
    let rootBuilt: Awaited<ReturnType<typeof build>> | null = null;
    if (rootDue && tooLarge('root', SECTION_ROOT, '')) {
      failed.push({ doc: 'root', seq: 0 });
      deferred.add('root');
    } else {
      rootBuilt = await build('root', SECTION_ROOT, '', rootDue ? cutOf('root') : 0);
      if (rootBuilt.failed) {
        failed.push(rootBuilt.failed);
        deferred.add('root');
      }
    }
    const pages = rootBuilt ? pageIds(rootBuilt.loro) : { listed: new Set<string>(), known: new Set<string>() };
    if (rootBuilt && rootDue && !rootBuilt.failed) {
      totalRows += rootBuilt.rows;
      replaced.set('root', { version: plain(rootBuilt.loro), refs: [] });
      await keep(SECTION_ROOT, '', rootBuilt.loro.export({ mode: 'snapshot' }));
    }
    rootBuilt?.loro.free();

    for (const s of due) {
      if (s.doc === 'root') continue;
      // Страница, которой корень ещё не знает: корень мог не дойти. Ждёт срока, файл ради неё не переписывается.
      if (!pages.known.has(s.doc)) {
        deferred.add(s.doc);
        continue;
      }
      if (tooLarge(s.doc, SECTION_PAGE, s.doc)) {
        failed.push({ doc: s.doc, seq: 0 });
        deferred.add(s.doc);
        continue;
      }
      const built = await build(s.doc, SECTION_PAGE, s.doc, cutOf(s.doc));
      if (built.failed) {
        built.loro.free();
        failed.push(built.failed);
        deferred.add(s.doc);
        continue;
      }
      totalRows += built.rows;
      const refs = imageRefs(built.loro);
      replaced.set(s.doc, { version: plain(built.loro), refs });
      await keep(SECTION_PAGE, s.doc, built.loro.export({ mode: 'snapshot' }));
      await keep(SECTION_PAGE_REFS, s.doc, new TextEncoder().encode(refs.join('\n')));
      built.loro.free();
    }
    await sink.end();

    const defer = (c: Pick<Pool, 'query'>) =>
      deferred.size === 0
        ? Promise.resolve()
        : c.query(`UPDATE doc_state SET compact_after = now() + make_interval(secs => $3) WHERE file_id = $1 AND doc = ANY($2::text[])`, [
            file,
            [...deferred],
            opts.retryAfterSeconds,
          ]);
    if (replaced.size === 0) {
      await defer(pool);
      return failed.length > 0 ? { kind: 'failed', failed } : { kind: 'empty' };
    }

    // Секции нового контейнера: неизменённые — копией куска прежнего файла, новые — из временного файла.
    const sections: Section[] = [];
    const rootNew = fresh.find((x) => x.kind === SECTION_ROOT);
    const rootOld = spanOf(SECTION_ROOT, '');
    if (rootNew) sections.push(rootNew);
    else if (rootOld)
      sections.push({
        kind: SECTION_ROOT,
        id: '',
        from: { src: 'old', start: rootOld.start, end: rootOld.data + rootOld.size },
        data: null,
      });
    for (const s of spans) {
      if (s.kind === SECTION_ROOT || s.kind === SECTION_IMAGE) continue;
      if ((s.kind === SECTION_PAGE || s.kind === SECTION_PAGE_REFS || s.kind === SECTION_SCENE) && replaced.has(s.id)) continue;
      sections.push({ kind: s.kind, id: s.id, from: { src: 'old', start: s.start, end: s.data + s.size }, data: null });
    }
    // Страница без единой правки (только что добавлена): ни журнала, ни секции. Ядро приняло бы отсутствие секции за повреждение файла.
    for (const id of pages.listed) {
      if (replaced.has(id) || spanOf(SECTION_PAGE, id)) continue;
      sections.push({ kind: SECTION_PAGE, id, from: null, data: new LoroDoc().export({ mode: 'snapshot' }) });
      sections.push({ kind: SECTION_PAGE_REFS, id, from: null, data: new Uint8Array(0) });
    }
    sections.push(...fresh.filter((x) => x.kind !== SECTION_ROOT));
    const refs = new Set<string>();
    for (const r of replaced.values()) for (const h of r.refs) refs.add(h);
    for (const s of spans) {
      if (s.kind !== SECTION_PAGE_REFS || replaced.has(s.id)) continue;
      for (const line of new TextDecoder().decode(await read(s.data, s.data + s.size)).split('\n')) if (line) refs.add(line);
    }

    const key = `files/${file}/content-${randomUUID()}`;
    await pool.query('INSERT INTO blob_pending (key) VALUES ($1) ON CONFLICT DO NOTHING', [key]);
    const head = containerHeader(sections.length);
    const made = Bun.file(tmp);
    const body = new ReadableStream<Uint8Array>({
      async start(c) {
        c.enqueue(head);
        for (const s of sections) {
          if (!s.from) c.enqueue(sectionBytes(s.kind, s.id, s.data!));
          else if (s.from.src === 'old') c.enqueue(await read(s.from.start, s.from.end));
          else c.enqueue(new Uint8Array(await made.slice(s.from.start, s.from.end).arrayBuffer()));
        }
        c.close();
      },
    });
    const { size } = await blobs.putStream(key, body, { maxBytes: opts.maxContent, idleMs: 60_000 });

    const outcome = await inTransaction(pool, async (c) => {
      // Порядок блокировок как у журнала: строка файла, потом строки документов.
      const locked = await c.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1 FOR UPDATE', [file]);
      if (!locked.rows[0] || locked.rows[0].content_key !== contentKey) return 'raced' as const;
      const pending = await c.query('DELETE FROM blob_pending WHERE key = $1', [key]);
      if (pending.rowCount === 0) return 'raced' as const;
      if (contentKey) {
        await c.query(`INSERT INTO blob_trash (key, delete_after) VALUES ($1, now() + make_interval(secs => $2)) ON CONFLICT DO NOTHING`, [
          contentKey,
          BLOB_GRACE_SECONDS,
        ]);
      }
      await c.query('UPDATE file SET version = version + 1, size = $2, content_key = $3, compact_after = NULL WHERE id = $1', [
        file,
        size,
        key,
      ]);
      // Вошедшие записи — той же транзакцией: записи новее `cut` остаются, журнал и снимок не расходятся.
      for (const [doc, r] of replaced) {
        if (!stateOf.has(doc)) continue;
        const removed = await c.query<{ size: number }>(
          'DELETE FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq <= $3 RETURNING octet_length(bytes) AS size',
          [file, doc, cutOf(doc)],
        );
        await c.query(
          `UPDATE doc_state SET base_vv = $3::jsonb, log_count = log_count - $4, log_bytes = log_bytes - $5, compact_after = NULL
           WHERE file_id = $1 AND doc = $2`,
          [file, doc, JSON.stringify(r.version), removed.rowCount ?? 0, removed.rows.reduce((n, x) => n + x.size, 0)],
        );
      }
      await defer(c);
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
    return { kind: 'done', docs: replaced.size, size, rows: totalRows, failed };
  } finally {
    await Promise.resolve(sink.end()).catch(() => {});
    rmSync(tmp, { force: true });
  }
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
