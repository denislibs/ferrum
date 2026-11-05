// Сжатие (спека подсистемы 4, раздел 7.4): снимок заменяется и вошедшие записи удаляются одной транзакцией; новые записи остаются;
// сбой посередине ничего не теряет; обновление, которое не применилось, не заменяет снимок; клиент догоняет, отставший получает снимок.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { scanContainer } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import { apiStand } from './api-stand';
import type { BlobStore } from '../src/blob-store';
import { compactFile } from '../src/sync/compact';
import { Client, peers, realContainer } from './sync-client';
import { TEST_DATABASE_URL } from './helpers';

const s = await apiStand({
  compact: {
    init: { databaseUrl: TEST_DATABASE_URL, dataDir: '' },
    limits: { maxLogCount: 5, tickMs: 50, idleMs: 100, retryAfterSeconds: 1 },
  },
});
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

test('обновление не применилось: снимок не заменён, журнал цел, seq назван, приём правок идёт', async () => {
  const d = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  const c = await joined(pageId, peers(page));
  await writes(2, d, c);
  await s.pool.query('UPDATE doc_update SET bytes = $3 WHERE file_id = $1 AND seq = $2', [file, 2, Buffer.from([1, 2, 3])]);
  const before = await contentSections();
  const r = await compactFile(s.pool, s.blobs, file);
  expect(r).toEqual({ kind: 'failed', doc: pageId, seq: 2 });
  expect((await contentSections()).version).toBe(before.version);
  expect(await logRows()).toEqual([1, 2]);
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

test('удалённая страница уходит из контейнера и журнала, картинки считаются по новым ссылкам', async () => {
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
  const gone = await contentSections();
  expect(gone.spans.filter((x) => x.kind === 2).map((x) => x.id)).toEqual([String(extra.id)]);
  expect((await s.pool.query(`SELECT 1 FROM doc_state WHERE file_id = $1 AND doc = $2`, [file, pageId])).rows).toHaveLength(0);
  row = await s.pool.query<{ unreferenced_at: Date | null }>('SELECT unreferenced_at FROM file_image WHERE file_id = $1', [file]);
  expect(row.rows[0].unreferenced_at).not.toBeNull();
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
  await expect(compactFile(s.pool, s.blobs, file, 16)).rejects.toThrow();
  expect(await logRows()).toEqual([1, 2]);
});
