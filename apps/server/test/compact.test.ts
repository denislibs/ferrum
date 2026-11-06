// Сжатие (спека подсистемы 4, раздел 7.4): снимок заменяется и вошедшие записи удаляются одной транзакцией; новые записи остаются;
// сбой посередине ничего не теряет; обновление, которое не применилось, не заменяет снимок; клиент догоняет, отставший получает снимок.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { scanContainer } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import { apiStand } from './api-stand';
import type { BlobStore } from '../src/blob-store';
import type { Pool } from 'pg';
import { compactFile } from '../src/sync/compact';
import { Compactor } from '../src/sync/compactor';
import { Client, peers, realContainer } from './sync-client';
import { TEST_DATABASE_URL } from './helpers';

const s = await apiStand({
  compact: {
    init: { databaseUrl: TEST_DATABASE_URL, dataDir: '' },
    limits: { maxLogCount: 5, tickMs: 50, idleMs: 100, retryAfterSeconds: 1 },
  },
  // Догоняние кусками по 4 записи: сжатие между проходами проверяется на десятке правок.
  syncLimits: { pumpRows: 4 },
});
const data = (name: string) => Bun.file(new URL(`./data/${name}`, import.meta.url)).bytes();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const editor = await s.signIn('editor@example.com');
let file = '';
let root: LoroDoc;
let pageId = '';
let page: LoroDoc;
const open: Client[] = [];

beforeEach(async () => {
  for (const c of open.splice(0)) c.close();
  await s.reset();
  file = (await s.newFile(owner)).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor')`, [file, editor.id]);
  root = new LoroDoc();
  root.getMap('meta').set('name', 'файл');
  const tree = root.getTree('pages');
  tree.enableFractionalIndex(0);
  const node = tree.createNode();
  node.data.set('name', 'Страница');
  root.commit();
  pageId = String(node.id);
  page = new LoroDoc();
  page.getTree('nodes').enableFractionalIndex(0);
  page.commit();
  const res = await s.writeContent(
    owner,
    file,
    0,
    realContainer(root.export({ mode: 'snapshot' }), [[pageId, page.export({ mode: 'snapshot' })]]),
  );
  expect(res.status).toBe(200);
});

async function joined(doc: string, version: Map<string, number>, who = owner, f = file): Promise<Client> {
  const c = (await Client.open(s, who, f))!;
  open.push(c);
  c.hello();
  c.subscribe(doc, version);
  await c.wait('subscribed');
  return c;
}

/** Пишет `n` правок в журнал страницы, ack каждой. */
async function writes(n: number, d: LoroDoc, c: Client, doc = pageId): Promise<void> {
  const tree = d.getTree('nodes');
  tree.enableFractionalIndex(0);
  for (let i = 0; i < n; i++) {
    const from = d.oplogVersion();
    const node = tree.createNode();
    node.data.set('type', 'rect');
    node.data.set('x', i);
    d.commit();
    c.update(doc, i, d.export({ mode: 'update', from }));
    await c.wait('ack', (f) => f.clientSeq === i);
    c.frames.length = 0;
  }
}

/** Запросы к базе, текст которых подходит: `hold` решает, когда запрос уйдёт. Только момент: сам запрос настоящий. */
function intercept(
  match: (text: string, params: unknown[]) => boolean,
  hold: (run: () => Promise<unknown>) => Promise<unknown>,
): () => void {
  const pool = s.pool as Pool & { query: (...a: unknown[]) => Promise<unknown> };
  const real = pool.query.bind(pool) as (...a: unknown[]) => Promise<unknown>;
  pool.query = ((...a: unknown[]) => {
    const text = typeof a[0] === 'string' ? a[0] : '';
    if (!match(text, (a[1] as unknown[]) ?? [])) return real(...a);
    return hold(() => real(...a));
  }) as never;
  return () => {
    pool.query = real as never;
  };
}

/** Первый проход догоняния документа `doc` после `sent`; дальше — мимо. */
function pumpPass(doc: string, sent: number): (text: string, params: unknown[]) => boolean {
  let seen = false;
  return (text, params) => {
    if (seen || !text.includes('LEFT JOIN LATERAL') || params[1] !== doc || params[2] !== sent) return false;
    seen = true;
    return true;
  };
}

/** Отставший получает снимок и догоняет: документ тот же, что у пишущего. */
async function catchUp(c: Client, writer: LoroDoc): Promise<LoroDoc> {
  const need = await c.wait('snapshot-needed');
  const res = await s.call(editor, 'GET', need.url);
  const mine = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  mine.import(new Uint8Array(await res.arrayBuffer()));
  c.frames.length = 0;
  c.subscribe(pageId, peers(mine));
  await c.wait('subscribed');
  for (const f of c.frames) if (f.t === 'updates') mine.import(f.bytes);
  expect(mine.oplogVersion().toJSON()).toEqual(writer.oplogVersion().toJSON());
  return mine;
}

const fileVersion = async () =>
  Number((await s.pool.query<{ version: string }>('SELECT version FROM file WHERE id = $1', [file])).rows[0].version);

const logRows = async (doc = pageId) =>
  (await s.pool.query<{ seq: string }>('SELECT seq FROM doc_update WHERE file_id = $1 AND doc = $2 ORDER BY seq', [file, doc])).rows.map(
    (r) => Number(r.seq),
  );

const contentSections = async () => {
  const res = await s.call(owner, 'GET', `/api/files/${file}/content`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  return {
    bytes,
    spans: await scanContainer(bytes.length, async (a, b) => bytes.subarray(a, b)),
    version: Number(res.headers.get('x-ferrum-version')),
  };
};

test('снимок заменяется, журнал пустеет, содержимое то же, что у клиента', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(3, d, c);
  const before = await contentSections();
  const r = await compactFile(s.pool, s.blobs, file);
  expect(r).toMatchObject({ kind: 'done', rows: 3 });
  expect(await logRows()).toEqual([]);
  const after = await contentSections();
  expect(after.version).toBe(before.version + 1);
  const span = after.spans.find((x) => x.kind === 2 && x.id === pageId)!;
  const read = LoroDoc.fromSnapshot(after.bytes.subarray(span.data, span.data + span.size));
  expect(read.getTree('nodes').getNodes().length).toBe(3);
  expect(read.oplogVersion().toJSON()).toEqual(d.oplogVersion().toJSON());
  const { rows } = await s.pool.query<{ base_vv: object; log_count: number; log_bytes: string }>(
    'SELECT base_vv, log_count, log_bytes FROM doc_state WHERE file_id = $1 AND doc = $2',
    [file, pageId],
  );
  expect(rows[0].log_count).toBe(0);
  expect(rows[0].log_bytes).toBe('0');
  expect(rows[0].base_vv).toEqual(Object.fromEntries(d.oplogVersion().toJSON()));
});

test('ядро Rust откроет контейнер: секции и контрольные суммы на месте', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(1, d, c);
  await compactFile(s.pool, s.blobs, file);
  const { bytes, spans } = await contentSections();
  const { blake3 } = await import('@noble/hashes/blake3.js');
  for (const x of spans) {
    const sum = bytes.subarray(x.data - 32, x.data);
    expect(sum).toEqual(blake3(bytes.subarray(x.data, x.data + x.size)));
  }
  expect(spans.map((x) => x.kind).sort()).toEqual([1, 2, 3]);
});

test('запись во время сжатия: остаётся в журнале и не теряется', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(2, d, c);
  // Сжатие уже прочитало границу журнала и пишет файл: ровно здесь приходит новая правка.
  const late: BlobStore = {
    ...s.blobs,
    open: (k) => s.blobs.open(k),
    put: (k, b) => s.blobs.put(k, b),
    get: (k) => s.blobs.get(k),
    delete: (k) => s.blobs.delete(k),
    putStream: async (k, body, o) => {
      const from = d.oplogVersion();
      d.getMap('late').set('x', 1);
      d.commit();
      c.update(pageId, 99, d.export({ mode: 'update', from }));
      await c.wait('ack', (f) => f.clientSeq === 99);
      return s.blobs.putStream(k, body, o);
    },
  };
  expect((await compactFile(s.pool, late, file)).kind).toBe('done');
  expect(await logRows()).toEqual([3]);
  const snap = await s.call(owner, 'GET', `/api/files/${file}/docs/${pageId}/snapshot`);
  const merged = LoroDoc.fromSnapshot(new Uint8Array(await snap.arrayBuffer()));
  expect(merged.getMap('late').get('x')).toBeUndefined();
  const again = await joined(pageId, peers(merged));
  for (const f of again.frames) if (f.t === 'updates') merged.import(f.bytes);
  expect(merged.getMap('late').get('x')).toBe(1);
});

test('сбой посреди транзакции: содержимое и журнал целы, байты нового файла уйдут в очистку', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(2, d, c);
  const before = await contentSections();
  const failed = s.failing('doc_update', 'DELETE', () =>
    compactFile(s.pool, s.blobs, file).then(
      () => 'прошло',
      (e: Error) => e.message,
    ),
  );
  expect(await failed).toContain('сбой теста');
  const after = await contentSections();
  expect(after.version).toBe(before.version);
  expect(after.bytes).toEqual(before.bytes);
  expect(await logRows()).toEqual([1, 2]);
  const pending = await s.pool.query('SELECT key FROM blob_pending');
  expect(pending.rows).toHaveLength(1);
  // Следующая попытка удаётся.
  expect((await compactFile(s.pool, s.blobs, file)).kind).toBe('done');
});

/** Запись, которую не применить: вторая правка пира 77 без первой. Вставляется мимо журнала — журнал её бы не пропустил. */
async function ghost(doc = pageId): Promise<number> {
  const g = new LoroDoc();
  g.setPeerId(77n as never);
  g.getMap('g').set('a', 1);
  g.commit();
  const first = g.oplogVersion();
  g.getMap('g').set('b', 2);
  g.commit();
  const bytes = g.export({ mode: 'update', from: first });
  const { rows } = await s.pool.query<{ seq: string }>(
    `UPDATE doc_state SET next_seq = next_seq + 1, log_count = log_count + 1, log_bytes = log_bytes + $3
     WHERE file_id = $1 AND doc = $2 RETURNING next_seq - 1 AS seq`,
    [file, doc, bytes.length],
  );
  const seq = Number(rows[0].seq);
  await s.pool.query(
    `INSERT INTO doc_update (file_id, doc, seq, start_vv, end_vv, bytes) VALUES ($1, $2, $3, '{"77":1}', '{"77":2}', $4)`,
    [file, doc, seq, Buffer.from(bytes)],
  );
  return seq;
}

/** Документ страницы в текущем содержимом файла. */
async function pageInContent(): Promise<LoroDoc> {
  const { bytes, spans } = await contentSections();
  const span = spans.find((x) => x.kind === 2 && x.id === pageId)!;
  return LoroDoc.fromSnapshot(bytes.subarray(span.data, span.data + span.size));
}

test('неприменимая запись пропускается: остальные записи документа входят в снимок, она остаётся в журнале', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(1, d, c);
  const bad = await ghost();
  // Правки после неё — у людей они есть.
  await writes(2, d, c);
  const before = await fileVersion();
  const r = await compactFile(s.pool, s.blobs, file);
  expect(r).toMatchObject({ kind: 'done', rows: 3, failed: [], skipped: [{ doc: pageId, seq: bad }], incomplete: [] });
  expect(await fileVersion()).toBe(before + 1);
  expect(await logRows()).toEqual([bad]);
  // Копия — то, что видят люди: все правки, кроме той, которую не применит никто.
  const read = await pageInContent();
  expect(read.oplogVersion().toJSON()).toEqual(d.oplogVersion().toJSON());
  expect(read.getTree('nodes').getNodes().length).toBe(3);
  const { rows } = await s.pool.query<{ log_count: number; log_kept: number }>(
    'SELECT log_count, log_kept FROM doc_state WHERE file_id = $1 AND doc = $2',
    [file, pageId],
  );
  expect(rows[0]).toEqual({ log_count: 1, log_kept: 1 });
  // Ничего нового: файл не переписывается ради пропущенной записи.
  expect(await compactFile(s.pool, s.blobs, file)).toMatchObject({ kind: 'empty', incomplete: [] });
  expect(await fileVersion()).toBe(before + 1);
  // Новая правка сжимается, пропущенная остаётся.
  await writes(1, d, c);
  expect(await compactFile(s.pool, s.blobs, file)).toMatchObject({ kind: 'done', rows: 1, skipped: [{ doc: pageId, seq: bad }] });
  expect(await logRows()).toEqual([bad]);
  // Новый подписчик со снимком получает её догонянием, как и прежде: у него то же, что у людей.
  const late = LoroDoc.fromSnapshot((await pageInContent()).export({ mode: 'snapshot' }));
  const l = await joined(pageId, peers(late), editor);
  for (const f of l.frames) if (f.t === 'updates') late.import(f.bytes);
  expect(late.oplogVersion().toJSON()).toEqual(d.oplogVersion().toJSON());
});

test('одна неприменимая запись не останавливает сжатие файла: остальные документы и записи сжимаются, она остаётся в журнале', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(2, d, c);
  await s.pool.query('UPDATE doc_update SET bytes = $3 WHERE file_id = $1 AND seq = $2', [file, 2, Buffer.from([1, 2, 3])]);
  // Правка корня — другой документ.
  const r = await joined('root', peers(root));
  const rootTab = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  const rf = rootTab.oplogVersion();
  rootTab.getMap('meta').set('name', 'другое');
  rootTab.commit();
  r.update('root', 1, rootTab.export({ mode: 'update', from: rf }));
  await r.wait('ack');
  const before = await fileVersion();
  const out = await compactFile(s.pool, s.blobs, file);
  expect(out).toMatchObject({ kind: 'done', failed: [], skipped: [{ doc: pageId, seq: 2 }], incomplete: [] });
  expect(await fileVersion()).toBe(before + 1);
  expect(await logRows('root')).toEqual([]);
  expect(await logRows()).toEqual([2]);
  expect((await pageInContent()).getTree('nodes').getNodes().length).toBe(1);
  // Пропущенная запись — не повод переписывать файл на каждом проходе.
  expect(await compactFile(s.pool, s.blobs, file)).toMatchObject({ kind: 'empty' });
  expect(await fileVersion()).toBe(before + 1);
});

test('документ, которого нет в корне, не переписывает файл ни разу', async () => {
  const c = await joined('5@99', new Map());
  await writes(2, LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' })), c, '5@99');
  const before = await fileVersion();
  // Людям её не видно: копия без неё полная.
  expect(await compactFile(s.pool, s.blobs, file)).toMatchObject({ kind: 'empty', incomplete: [] });
  expect(await compactFile(s.pool, s.blobs, file)).toMatchObject({ kind: 'empty', incomplete: [] });
  expect(await fileVersion()).toBe(before);
  expect(await logRows('5@99')).toEqual([1, 2]);
  // Таймер его не берёт до срока: файл не встаёт в очередь на каждом тике.
  const { rows } = await s.pool.query<{ waits: boolean }>(
    `SELECT compact_after > now() AS waits FROM doc_state WHERE file_id = $1 AND doc = '5@99'`,
    [file],
  );
  expect(rows[0].waits).toBe(true);
});

test('сжатие между проходами догоняния: отставший получает snapshot-needed, а не дыру', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const w = await joined(pageId, peers(page));
  // Пять записей: два прохода по четыре и ещё не порог сжатия стенда.
  await writes(5, d, w);
  // Первый проход отдал четыре записи; перед вторым журнал сжат.
  const restore = intercept(pumpPass(pageId, 4), async (run) => {
    expect((await compactFile(s.pool, s.blobs, file)).kind).toBe('done');
    return run();
  });
  const late = (await Client.open(s, editor, file))!;
  open.push(late);
  try {
    late.hello();
    late.subscribe(pageId, peers(page));
    await late.wait('snapshot-needed');
    // Первый проход дошёл: дыра появилась между проходами, а не до подписки.
    expect(late.frames.flatMap((f) => (f.t === 'updates' ? [f.seq] : []))).toEqual([1, 2, 3, 4]);
    const mine = await catchUp(late, d);
    expect(mine.getTree('nodes').getNodes().length).toBe(5);
  } finally {
    restore();
  }
});

test('сжатие между заведением строки и первым проходом: отставший получает snapshot-needed', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const w = await joined(pageId, peers(page));
  await writes(3, d, w);
  const restore = intercept(pumpPass(pageId, 0), async (run) => {
    expect((await compactFile(s.pool, s.blobs, file)).kind).toBe('done');
    return run();
  });
  try {
    const late = (await Client.open(s, editor, file))!;
    open.push(late);
    late.hello();
    late.subscribe(pageId, peers(page));
    await catchUp(late, d);
  } finally {
    restore();
  }
});

test('после сжатия клиент, у которого всё есть, подписывается без снимка и без записей', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const w = await joined(pageId, peers(page));
  await writes(3, d, w);
  expect((await compactFile(s.pool, s.blobs, file)).kind).toBe('done');
  const c = await joined(pageId, peers(d), editor);
  expect(c.frames.filter((f) => f.t === 'updates' || f.t === 'snapshot-needed')).toEqual([]);
  // И следующая правка идёт рассылкой.
  await writes(1, d, w);
  await c.wait('updates', (f) => f.seq === 4);
});

test('удалённая страница, ожившая параллельным переносом, сохраняет содержимое', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(3, d, c);
  // У второго человека корень до удаления; он на связи и получит удаление рассылкой, но его перенос — параллельный.
  const theirs = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  const b = await joined('root', peers(root), editor);
  const a = await joined('root', peers(root));
  const mineRoot = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  const af = mineRoot.oplogVersion();
  mineRoot.getTree('pages').delete(mineRoot.getTree('pages').roots()[0].id);
  mineRoot.commit();
  a.update('root', 1, mineRoot.export({ mode: 'update', from: af }));
  await a.wait('ack');
  expect((await compactFile(s.pool, s.blobs, file)).kind).toBe('done');
  // Параллельный перенос с бо́льшим lamport оживляет страницу.
  const bf = theirs.oplogVersion();
  for (let i = 0; i < 5; i++) theirs.getMap('meta').set('n', i);
  const pages = theirs.getTree('pages');
  const moved = pages.roots()[0].id;
  pages.createNode().data.set('name', 'Вторая');
  pages.move(moved, undefined, 1);
  theirs.commit();
  b.update('root', 1, theirs.export({ mode: 'update', from: bf }));
  await b.wait('ack');
  const merged = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  merged.import(mineRoot.export({ mode: 'update', from: root.oplogVersion() }));
  merged.import(theirs.export({ mode: 'update', from: root.oplogVersion() }));
  expect(merged.getTree('pages').isNodeDeleted(moved)).toBe(false);
  // Содержимое страницы на месте, строка документа тоже: правки в неё идут.
  const res = await s.call(editor, 'GET', `/api/files/${file}/docs/${pageId}/snapshot`);
  expect(res.status).toBe(200);
  expect(
    LoroDoc.fromSnapshot(new Uint8Array(await res.arrayBuffer()))
      .getTree('nodes')
      .getNodes().length,
  ).toBe(3);
  await writes(1, d, c);
});

test('Compactor: задача POST — вперёд очереди', async () => {
  const files = [file, (await s.newFile(owner)).id, (await s.newFile(owner)).id];
  for (const f of files) {
    await s.pool.query(`INSERT INTO doc_state (file_id, doc, log_count) VALUES ($1, 'root', 1) ON CONFLICT DO NOTHING`, [f]);
  }
  const order: string[] = [];
  const c = new Compactor(s.pool, { databaseUrl: TEST_DATABASE_URL, dataDir: s.dataDir }, { tickMs: 3_600_000 });
  c.onResult = (f) => order.push(f);
  try {
    c.enqueue(files[0]);
    c.enqueue(files[1]);
    const urgent = c.compactNow(files[2]);
    await urgent;
    for (let i = 0; i < 400 && order.length < 3; i++) await Bun.sleep(10);
    expect(order).toEqual([files[0], files[2], files[1]]);
  } finally {
    c.stop();
  }
});

test('Compactor: «Скачать копию» и задачи таймера — через одну, у человека одна ручная задача разом', async () => {
  const files = [file, ...(await Promise.all([0, 1, 2].map(async () => (await s.newFile(owner)).id)))];
  for (const f of files) {
    await s.pool.query(`INSERT INTO doc_state (file_id, doc, log_count) VALUES ($1, 'root', 1) ON CONFLICT DO NOTHING`, [f]);
  }
  const order: string[] = [];
  const c = new Compactor(s.pool, { databaseUrl: TEST_DATABASE_URL, dataDir: s.dataDir }, { tickMs: 3_600_000 });
  c.onResult = (f) => order.push(f);
  try {
    c.enqueue(files[0]);
    c.enqueue(files[1]);
    // Два файла одного человека «Скачать копию» не ставят вперёд всех: задача таймера идёт между ними.
    await Promise.all([c.compactNow(files[2]), c.compactNow(files[3])]);
    for (let i = 0; i < 400 && order.length < 4; i++) await Bun.sleep(10);
    expect(order).toEqual([files[0], files[2], files[1], files[3]]);
    // Вторая ручная задача того же человека, пока идёт первая, — «повторите».
    const first = c.manual(files[0], 'кто-то');
    const second = await c.manual(files[1], 'кто-то');
    expect(second).toMatchObject({ ok: false });
    expect((second as { retryAfter: number }).retryAfter).toBeGreaterThan(0);
    expect(await first).toEqual({ ok: true });
  } finally {
    c.stop();
  }
});

test('Compactor: память «Скачать копию» по файлам не растёт без конца', async () => {
  const c = new Compactor(s.pool, { databaseUrl: TEST_DATABASE_URL, dataDir: s.dataDir }, { tickMs: 20, manualIntervalMs: 50 });
  try {
    expect(await c.manual(file, owner.id)).toEqual({ ok: true });
    expect(c.manualEntries()).toBe(1);
    for (let i = 0; i < 200 && c.manualEntries() > 0; i++) await Bun.sleep(10);
    expect(c.manualEntries()).toBe(0);
  } finally {
    c.stop();
  }
});

test('документ, который ждёт срока, порог не считает: правки в нём не переписывают файл', async () => {
  // Порог стенда — 5 записей. Файл открыт: таймер простоя его не трогает, сжимает только порог.
  const c = await joined(pageId, peers(page));
  c.subscribe('5@99', new Map());
  await c.wait('subscribed', (x) => x.doc === '5@99');
  const results: string[] = [];
  s.compactor!.onResult = (f) => results.push(f);
  try {
    const p = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
    const z = new LoroDoc();
    let seq = 0;
    const put = async (doc: string, d: LoroDoc) => {
      const from = d.oplogVersion();
      d.getMap('m').set('x', seq);
      d.commit();
      const k = ++seq;
      c.update(doc, k, d.export({ mode: 'update', from }));
      await c.wait('ack', (x) => x.clientSeq === k);
    };
    // Страница, которой нет в корне, перешла порог: сжатие её откладывает.
    for (let i = 0; i < 7; i++) await put('5@99', z);
    for (let i = 0; i < 400 && results.length === 0; i++) await Bun.sleep(10);
    expect(results.length).toBeGreaterThan(0);
    const { rows } = await s.pool.query<{ waits: boolean }>(
      `SELECT compact_after > now() AS waits FROM doc_state WHERE file_id = $1 AND doc = '5@99'`,
      [file],
    );
    expect(rows[0].waits).toBe(true);
    const before = await fileVersion();
    for (let i = 0; i < 4; i++) {
      await put(pageId, p);
      await put('5@99', z);
    }
    // Очередь сжатия — по порядку: когда дошла задача после правок, всё, что они поставили, уже прошло.
    const sentinel = (await s.newFile(owner)).id;
    s.compactor!.enqueue(sentinel);
    for (let i = 0; i < 400 && !results.includes(sentinel); i++) await Bun.sleep(10);
    expect(results).toContain(sentinel);
    expect(await fileVersion()).toBe(before);
    expect(await logRows()).toEqual([1, 2, 3, 4]);
  } finally {
    s.compactor!.onResult = () => {};
  }
});

test('таймер сжатия берёт таблицы в том же порядке, что журнал и очистка стенда: файл, потом документы', async () => {
  const c = new Compactor(s.pool, { databaseUrl: TEST_DATABASE_URL, dataDir: s.dataDir }, { tickMs: 20 });
  const other = await s.pool.connect();
  try {
    // Как `TRUNCATE file, doc_state, …`: файл, потом документы.
    await other.query('BEGIN');
    await other.query('LOCK TABLE file IN ACCESS EXCLUSIVE MODE');
    let waiting = false;
    for (let i = 0; i < 400 && !waiting; i++) {
      const { rows } = await s.pool.query(
        `SELECT 1 FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND query LIKE '%log_kept%'`,
      );
      waiting = rows.length > 0;
      if (!waiting) await Bun.sleep(5);
    }
    expect(waiting).toBe(true);
    // Таймер ждёт файл, не держа документы: иначе здесь `deadlock detected`.
    await other.query('LOCK TABLE doc_state IN ACCESS EXCLUSIVE MODE');
    await other.query('COMMIT');
  } finally {
    await other.query('ROLLBACK').catch(() => {});
    other.release();
    c.stop();
  }
});

test('Compactor: паника wasm меняет worker, документ ждёт срока, остальные файлы сжимаются', async () => {
  const base = await data('loro-base.snap');
  const bad = (await s.newFile(owner)).id;
  expect((await s.writeContent(owner, bad, 0, realContainer(base))).status).toBe(200);
  // Файл открыт раньше, чем в журнале что-то есть: таймер сжатия стенда его не тронет.
  await joined('root', peers(LoroDoc.fromSnapshot(base)), owner, bad);
  await s.pool.query(`UPDATE doc_state SET next_seq = 2, log_count = 1 WHERE file_id = $1 AND doc = 'root'`, [bad]);
  await s.pool.query(`INSERT INTO doc_update (file_id, doc, seq, start_vv, end_vv, bytes) VALUES ($1, 'root', 1, '{}', '{}', $2)`, [
    bad,
    Buffer.from(await data('loro-panic-real.update')),
  ]);
  const c = new Compactor(s.pool, { databaseUrl: TEST_DATABASE_URL, dataDir: s.dataDir }, { tickMs: 3_600_000 });
  const saved = console.error;
  console.error = () => {};
  try {
    expect(await c.compactNow(bad)).toBe(false);
    expect(c.spawned).toBe(1);
    const { rows } = await s.pool.query<{ waits: boolean }>(`SELECT compact_after > now() AS waits FROM doc_state WHERE file_id = $1`, [
      bad,
    ]);
    expect(rows[0].waits).toBe(true);
    // Следующий файл — уже на новом worker'е.
    const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
    await writes(1, d, await joined(pageId, peers(page)));
    expect(await c.compactNow(file)).toBe(true);
    expect(c.spawned).toBe(2);
  } finally {
    console.error = saved;
    c.stop();
  }
});

test('Compactor: worker пересоздаётся через заданное число задач и после stop не заводится', async () => {
  const c = new Compactor(s.pool, { databaseUrl: TEST_DATABASE_URL, dataDir: s.dataDir }, { tickMs: 3_600_000, tasksPerWorker: 1 });
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const w = await joined(pageId, peers(page));
  await writes(1, d, w);
  expect(await c.compactNow(file)).toBe(true);
  await writes(1, d, w);
  expect(await c.compactNow(file)).toBe(true);
  expect(c.spawned).toBe(2);
  const other = (await s.newFile(owner)).id;
  await s.pool.query(`INSERT INTO doc_state (file_id, doc, log_count) VALUES ($1, 'root', 1)`, [other]);
  const pending = [c.compactNow(file), c.compactNow(other)];
  c.stop();
  expect(await Promise.all(pending)).toEqual([false, false]);
  await Bun.sleep(200);
  expect(c.hasWorker()).toBe(false);
  expect(c.spawned).toBeLessThanOrEqual(3);
});

test('Compactor: сбой базы до начала задачи не вешает ждущих и не останавливает очередь', async () => {
  const c = new Compactor(s.pool, { databaseUrl: TEST_DATABASE_URL, dataDir: s.dataDir }, { tickMs: 3_600_000 });
  let failed = false;
  const restore = intercept(
    (text) => !failed && text.includes('compact_after > now()'),
    async () => {
      failed = true;
      throw new Error('сбой теста');
    },
  );
  const saved = console.error;
  console.error = () => {};
  try {
    const other = (await s.newFile(owner)).id;
    const first = c.compactNow(file);
    const second = c.compactNow(other);
    expect(await first).toBe(false);
    expect(await second).toBe(true);
  } finally {
    restore();
    console.error = saved;
    c.stop();
  }
});

test('Compactor: не вышло — пауза до срока, после срока выходит', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  await writes(2, d, await joined(pageId, peers(page)));
  const c = new Compactor(s.pool, { databaseUrl: TEST_DATABASE_URL, dataDir: s.dataDir }, { tickMs: 3_600_000, retryAfterSeconds: 3600 });
  const saved = console.error;
  console.error = () => {};
  try {
    const results: string[] = [];
    c.onResult = (_f, r) => results.push(r.ok ? r.result.kind : 'error');
    expect(await s.failing('doc_update', 'DELETE', () => c.compactNow(file))).toBe(false);
    expect(results).toEqual(['error']);
    // Пауза: попытки нет вовсе.
    expect(await c.compactNow(file)).toBe(false);
    expect(results).toEqual(['error']);
    // Срок прошёл.
    await s.pool.query(`UPDATE file SET compact_after = now() - interval '1 second' WHERE id = $1`, [file]);
    expect(await c.compactNow(file)).toBe(true);
    expect(results).toEqual(['error', 'done']);
    expect(await logRows()).toEqual([]);
  } finally {
    console.error = saved;
    c.stop();
  }
});

test('POST compact: сжимает и отвечает 200; на паузе — 503; повтор сразу после сжатия не переписывает файл', async () => {
  const viewer = await s.signIn(`viewer-${crypto.randomUUID()}@example.com`);
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, viewer.id]);
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const w = await joined(pageId, peers(page));
  await writes(1, d, w);
  const before = await fileVersion();
  expect((await s.call(viewer, 'POST', `/api/files/${file}/compact`)).status).toBe(200);
  expect(await logRows()).toEqual([]);
  expect(await fileVersion()).toBe(before + 1);
  // Сразу снова, с новой правкой: файл не переписывается, ответ говорит, когда можно.
  await writes(1, d, w);
  const again = await s.call(viewer, 'POST', `/api/files/${file}/compact`);
  expect(again.status).toBe(503);
  expect(Number(again.headers.get('retry-after'))).toBeGreaterThan(0);
  expect(await fileVersion()).toBe(before + 1);
  // Пауза после сбоя — 503.
  await s.pool.query(`UPDATE file SET compact_after = now() + interval '1 hour' WHERE id = $1`, [file]);
  const paused = await s.call(owner, 'POST', `/api/files/${file}/compact`);
  expect(paused.status).toBe(503);
});

test('«Скачать копию» не отвечает 200 с неполной копией: документ, который ждёт срока, — 503 INCOMPLETE', async () => {
  const viewer = await s.signIn(`viewer-${crypto.randomUUID()}@example.com`);
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, viewer.id]);
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  await writes(2, d, await joined(pageId, peers(page)));
  // Документ отложен (паника wasm при прошлом сжатии): его правок в копии не будет.
  await s.pool.query(`UPDATE doc_state SET compact_after = now() + interval '1 hour' WHERE file_id = $1 AND doc = $2`, [file, pageId]);
  const before = await fileVersion();
  const res = await s.call(viewer, 'POST', `/api/files/${file}/compact`);
  expect(res.status).toBe(503);
  expect(await res.json()).toEqual({ error: 'INCOMPLETE' });
  expect(await fileVersion()).toBe(before);
  expect(await logRows()).toEqual([1, 2]);
  // Повтор в окне без новых правок — тот же ответ, файл не переписывается.
  expect((await s.call(viewer, 'POST', `/api/files/${file}/compact`)).status).toBe(503);
  expect(await fileVersion()).toBe(before);
});

test('«Скачать копию»: страница, которой нет в корне, людям не видна — копия без неё полная, 200', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  await writes(2, d, await joined(pageId, peers(page)));
  await writes(1, new LoroDoc(), await joined('5@99', new Map()), '5@99');
  expect((await s.call(owner, 'POST', `/api/files/${file}/compact`)).status).toBe(200);
  expect(await logRows()).toEqual([]);
  expect(await logRows('5@99')).toEqual([1]);
  // Повтор в окне без новых правок: копия та же, полная.
  expect((await s.call(owner, 'POST', `/api/files/${file}/compact`)).status).toBe(200);
});

test('«Скачать копию»: документ больше предела в копию не входит — 503 INCOMPLETE', async () => {
  const tight = await apiStand(
    {
      compact: {
        init: { databaseUrl: TEST_DATABASE_URL, dataDir: '', maxDocBytes: 1 },
        limits: { tickMs: 3_600_000 },
      },
    },
    true,
  );
  try {
    const who = await tight.signIn(`large-${crypto.randomUUID()}@example.com`);
    const f = (await tight.newFile(who)).id;
    expect((await tight.writeContent(who, f, 0, realContainer(root.export({ mode: 'snapshot' })))).status).toBe(200);
    const c = (await Client.open(tight, who, f))!;
    c.hello();
    c.subscribe('root', peers(root));
    await c.wait('subscribed');
    const tabRoot = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
    const from = tabRoot.oplogVersion();
    tabRoot.getMap('meta').set('name', 'другое');
    tabRoot.commit();
    c.update('root', 1, tabRoot.export({ mode: 'update', from }));
    await c.wait('ack');
    const res = await tight.call(who, 'POST', `/api/files/${f}/compact`);
    expect(res.status).toBe(503);
    expect(await res.json()).toEqual({ error: 'INCOMPLETE' });
    c.close();
  } finally {
    await tight.stop();
  }
});

test('POST compact со сроком: долгое сжатие — 503 с Retry-After, а не обрыв соединения', async () => {
  const tight = await apiStand(
    {
      compact: {
        init: { databaseUrl: TEST_DATABASE_URL, dataDir: '' },
        limits: { tickMs: 3_600_000, manualDeadlineMs: 1 },
      },
    },
    true,
  );
  try {
    const who = await tight.signIn(`deadline-${crypto.randomUUID()}@example.com`);
    const f = (await tight.newFile(who)).id;
    await tight.pool.query(`INSERT INTO doc_state (file_id, doc, log_count) VALUES ($1, 'root', 1)`, [f]);
    const res = await tight.call(who, 'POST', `/api/files/${f}/compact`);
    expect(res.status).toBe(503);
    expect(Number(res.headers.get('retry-after'))).toBeGreaterThan(0);
  } finally {
    await tight.stop();
  }
});

test('документ больше предела не собирается: отказ этому документу, остальные сжимаются', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  // Пять записей — ещё не порог сжатия стенда.
  await writes(5, d, await joined(pageId, peers(page)));
  const r = await joined('root', peers(root));
  const rootTab = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  const rf = rootTab.oplogVersion();
  rootTab.getMap('meta').set('name', 'другое');
  rootTab.commit();
  r.update('root', 1, rootTab.export({ mode: 'update', from: rf }));
  await r.wait('ack');
  // Размер документа — его секция и журнал.
  const { spans } = await contentSections();
  const sizeOf = async (doc: string, kind: number, id: string) =>
    spans.find((x) => x.kind === kind && x.id === id)!.size +
    Number(
      (await s.pool.query<{ b: string }>('SELECT log_bytes AS b FROM doc_state WHERE file_id = $1 AND doc = $2', [file, doc])).rows[0].b,
    );
  const pageSize = await sizeOf(pageId, 2, pageId);
  expect(await sizeOf('root', 1, '')).toBeLessThan(pageSize);
  const out = await compactFile(s.pool, s.blobs, file, { maxDocBytes: pageSize - 1 });
  expect(out).toMatchObject({ kind: 'done', failed: [{ doc: pageId, seq: 0 }], incomplete: [pageId] });
  expect(await logRows('root')).toEqual([]);
  expect(await logRows()).toEqual([1, 2, 3, 4, 5]);
});

test('Compactor сам сжимает по числу записей и после ухода людей; не вышло — повтор через срок', async () => {
  const results: string[] = [];
  s.compactor!.onResult = (_f, r) => results.push(r.ok ? r.result.kind : 'error');
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(8, d, c);
  for (let i = 0; i < 400 && results.length === 0; i++) await Bun.sleep(25);
  expect(results[0]).toBe('done');
  expect((await logRows()).length).toBeLessThan(8);
  // Ушли все: остаток журнала уходит сам.
  c.close();
  await c.waitClosed();
  for (let i = 0; i < 400 && (await logRows()).length > 0; i++) await Bun.sleep(25);
  expect(await logRows()).toEqual([]);
  s.compactor!.onResult = () => {};
});

test('отставший клиент после сжатия: snapshot-needed, снимок и догоняние дают тот же документ', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(3, d, c);
  const r = await compactFile(s.pool, s.blobs, file);
  expect(r.kind).toBe('done');
  await writes(2, d, c);
  const late = (await Client.open(s, editor, file))!;
  open.push(late);
  late.hello();
  late.subscribe(pageId, peers(page));
  const need = await late.wait('snapshot-needed');
  const res = await s.call(editor, 'GET', need.url);
  const mine = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  mine.import(new Uint8Array(await res.arrayBuffer()));
  late.frames.length = 0;
  late.subscribe(pageId, peers(mine));
  await late.wait('subscribed');
  for (const f of late.frames) if (f.t === 'updates') mine.import(f.bytes);
  expect(mine.oplogVersion().toJSON()).toEqual(d.oplogVersion().toJSON());
  expect(mine.getTree('nodes').getNodes().length).toBe(5);
});

test('удалённая страница остаётся в контейнере, картинки считаются по ссылкам', async () => {
  const hash = 'a'.repeat(64);
  await s.pool.query(`INSERT INTO image (hash, size, blob_key) VALUES ($1, 1, 'x') ON CONFLICT DO NOTHING`, [hash]);
  await s.pool.query('INSERT INTO file_image (file_id, hash) VALUES ($1, $2)', [file, hash]);
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  // Страница со ссылкой на картинку.
  const tree = d.getTree('nodes');
  tree.enableFractionalIndex(0);
  const from = d.oplogVersion();
  const node = tree.createNode();
  node.data.set('type', 'rect');
  node.data.set('fills', [{ kind: 'image', imageHash: hash }]);
  d.commit();
  c.update(pageId, 1, d.export({ mode: 'update', from }));
  await c.wait('ack');
  await compactFile(s.pool, s.blobs, file);
  let row = await s.pool.query<{ unreferenced_at: Date | null }>('SELECT unreferenced_at FROM file_image WHERE file_id = $1', [file]);
  expect(row.rows[0].unreferenced_at).toBeNull();
  const after = await contentSections();
  const refs = after.spans.find((x) => x.kind === 3 && x.id === pageId)!;
  expect(new TextDecoder().decode(after.bytes.subarray(refs.data, refs.data + refs.size))).toBe(hash);

  // Удаление страницы другим.
  const r = (await Client.open(s, owner, file))!;
  open.push(r);
  r.hello();
  r.subscribe('root', peers(root));
  await r.wait('subscribed');
  const rootTab = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  const treeR = rootTab.getTree('pages');
  const rf = rootTab.oplogVersion();
  const extra = treeR.createNode();
  extra.data.set('name', 'Вторая');
  treeR.delete(treeR.getNodes()[0].id);
  rootTab.commit();
  r.update('root', 1, rootTab.export({ mode: 'update', from: rf }));
  await r.wait('ack');
  expect((await compactFile(s.pool, s.blobs, file)).kind).toBe('done');
  // Удалённая страница остаётся: параллельный перенос может её вернуть (§7.4: автоматически ничего не удаляется).
  const gone = await contentSections();
  expect(
    gone.spans
      .filter((x) => x.kind === 2)
      .map((x) => x.id)
      .sort(),
  ).toEqual([pageId, String(extra.id)].sort());
  expect((await s.pool.query(`SELECT 1 FROM doc_state WHERE file_id = $1 AND doc = $2`, [file, pageId])).rows).toHaveLength(1);
  // Её картинки по-прежнему учтены.
  row = await s.pool.query<{ unreferenced_at: Date | null }>('SELECT unreferenced_at FROM file_image WHERE file_id = $1', [file]);
  expect(row.rows[0].unreferenced_at).toBeNull();
});

test('новая страница без правок получает пустую секцию: файл не выглядит повреждённым', async () => {
  const c = await joined('root', peers(root));
  const d = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  const from = d.oplogVersion();
  const added = d.getTree('pages').createNode();
  added.data.set('name', 'Вторая');
  d.commit();
  c.update('root', 1, d.export({ mode: 'update', from }));
  await c.wait('ack');
  expect((await compactFile(s.pool, s.blobs, file)).kind).toBe('done');
  const { bytes, spans } = await contentSections();
  const span = spans.find((x) => x.kind === 2 && x.id === String(added.id))!;
  expect(span).toBeTruthy();
  expect(
    LoroDoc.fromSnapshot(bytes.subarray(span.data, span.data + span.size))
      .oplogVersion()
      .toJSON().size,
  ).toBe(0);
});

test('файл сверх предела контейнера не заменяет прежний', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(2, d, c);
  await expect(compactFile(s.pool, s.blobs, file, { maxContent: 16 })).rejects.toThrow();
  expect(await logRows()).toEqual([1, 2]);
});
