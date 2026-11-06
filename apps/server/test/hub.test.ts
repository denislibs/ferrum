// WebSocket файла против настоящих Postgres и Valkey (спека подсистемы 4, разделы 7 и 13.2): открытие, кадры, правки двух
// людей, чужие и испорченные пачки, снятие роли и выход закрывают соединение сразу, пределы не трогают настоящую работу,
// а отказ по пределу — пауза, сбой базы не теряет правок и не роняет процесс.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { encodeClientFrame, encodeVersion, SYNC_CLOSE, type VersionMap } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import type { Pool } from 'pg';
import { SERVER_SCHEMA, SyncHub } from '../src/sync/hub';
import { apiStand, type Person } from './api-stand';
import { Client, peers, realContainer } from './sync-client';

const s = await apiStand({
  syncLimits: { recheckMs: 3_600_000, sessionCheckMs: 3_600_000, helloMs: 300, framesPerSecond: 120, framesBurst: 2000, knownSchema: 9 },
});
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const editor = await s.signIn('editor@example.com');
const viewer = await s.signIn('viewer@example.com');
const stranger = await s.signIn('stranger@example.com');

const data = (name: string) => Bun.file(new URL(`./data/${name}`, import.meta.url)).bytes();

let file = '';
let root: LoroDoc;
const open: Client[] = [];

async function connect(who: Person, f = file): Promise<Client> {
  const c = await Client.open(s, who, f);
  if (!c) throw new Error('соединение не открылось');
  open.push(c);
  c.hello();
  await c.wait('role');
  return c;
}

async function joined(who: Person, doc = 'root', version: VersionMap = peers(root)): Promise<Client> {
  const c = await connect(who);
  c.subscribe(doc, version);
  await c.wait('subscribed', (f) => f.doc === doc);
  return c;
}

/** Своя вкладка: у каждого человека — свой пир. */
function tab(): LoroDoc {
  const d = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  return d;
}

function edit(d: LoroDoc, value: number | string): Uint8Array {
  const from = d.oplogVersion();
  d.getMap('meta').set('x', value);
  d.commit();
  return d.export({ mode: 'update', from });
}

/** Запросы к базе, текст которых подходит: `hold` решает, когда запрос уйдёт и когда вернётся ответ. Только для
 * подсмотра момента: сам запрос настоящий. */
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

beforeEach(async () => {
  for (const c of open.splice(0)) c.close();
  await s.reset();
  file = (await s.newFile(owner)).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
    file,
    editor.id,
    viewer.id,
  ]);
  root = new LoroDoc();
  root.getMap('meta').set('name', 'файл');
  root.commit();
  const res = await s.writeContent(owner, file, 0, realContainer(root.export({ mode: 'snapshot' })));
  expect(res.status).toBe(200);
});

test('открытие: чужой Origin, без сессии, чужой файл, вкладка за другого человека', async () => {
  expect(await Client.open(s, owner, file, { origin: 'https://evil.example' })).toBeNull();
  expect(await Client.open(s, owner, file, { cookie: '' })).toBeNull();
  expect(await Client.open(s, stranger, file)).toBeNull();
  expect(await Client.open(s, owner, file, { user: editor.id })).toBeNull();
  expect(await Client.open(s, owner, '00000000-0000-4000-8000-000000000000')).toBeNull();
  const ok = await Client.open(s, owner, file);
  expect(ok).not.toBeNull();
  ok?.close();
  // Ответ на открытие без WebSocket — обычный отказ.
  const plain = await fetch(`${s.publicUrl}/api/files/${file}/sync?user=${owner.id}`, {
    headers: { cookie: owner.cookie, origin: s.publicUrl },
  });
  expect(plain.status).toBe(400);
});

test('кадры: первым hello, мусор и текст закрывают, старый протокол просит обновиться', async () => {
  const silent = (await Client.open(s, owner, file))!;
  expect((await silent.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);

  const first = (await Client.open(s, owner, file))!;
  first.subscribe('root');
  expect((await first.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);

  const garbage = await connect(owner);
  garbage.send(new Uint8Array([200, 1, 2]));
  expect((await garbage.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);

  const text = await connect(owner);
  text.ws.send('привет');
  expect((await text.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);

  const old = (await Client.open(s, owner, file))!;
  old.hello(999);
  await old.wait('please-reload');
  expect((await old.waitClosed()).code).toBe(SYNC_CLOSE.RELOAD);

  const huge = await connect(owner);
  huge.send(new Uint8Array(17 * 1024 * 1024));
  // Сверх `maxPayloadLength` сокет рвётся без чтения кадра: 1009 или обрыв, клиенту всё равно.
  expect([SYNC_CLOSE.TOO_BIG, 1006]).toContain((await huge.waitClosed()).code as 1009);
  // Процесс жив.
  expect((await s.call(owner, 'GET', `/api/files/${file}`)).status).toBe(200);
});

test('правка доходит до другого, отправителю только ack, журнал записан раньше рассылки', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const v = await joined(viewer);
  const mine = tab();
  a.update('root', 7, edit(mine, 1));
  const ack = await a.wait('ack', (f) => f.clientSeq === 7);
  const got = await b.wait('updates');
  expect(got.seq).toBe(ack.seq);
  await v.wait('updates');
  expect(a.frames.filter((f) => f.t === 'updates')).toEqual([]);
  const { rows } = await s.pool.query('SELECT seq FROM doc_update WHERE file_id = $1', [file]);
  expect(rows).toHaveLength(1);
  const theirs = tab();
  theirs.import(got.bytes);
  expect(theirs.getMap('meta').get('x')).toBe(1);
});

test('порядок: подписчик получает записи по порядку seq при правках двух людей', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const c = await joined(viewer);
  const [da, db] = [tab(), tab()];
  for (let i = 0; i < 40; i++) {
    a.update('root', i, edit(da, i));
    b.update('root', i, edit(db, -i));
  }
  await a.wait('ack', (f) => f.clientSeq === 39);
  await b.wait('ack', (f) => f.clientSeq === 39);
  await c.wait('updates', (f) => f.seq === 80);
  const seqs = c.frames.flatMap((f) => (f.t === 'updates' ? [f.seq] : []));
  expect(seqs).toEqual(Array.from({ length: 80 }, (_, i) => i + 1));
});

test('конец догоняния не перепрыгивает запись, зафиксированную во время последнего запроса', async () => {
  const a = await joined(owner);
  const d = tab();
  // Новый подписчик: его запрос журнала прочитан пустым, а ответ ещё не обработан — тут фиксируется правка, и рассылка
  // её пропускает (подписчик ещё не на рассылке).
  let held = false;
  const restore = intercept(
    (text, params) => !held && text.includes('FROM doc_update') && params[0] === file && params[2] === 0,
    async (run) => {
      held = true;
      const out = await run();
      a.update('root', 1, edit(d, 1));
      await a.wait('ack', (f) => f.clientSeq === 1);
      return out;
    },
  );
  try {
    const c = await connect(editor);
    c.subscribe('root', peers(root));
    await c.wait('subscribed');
    await c.wait('updates', (f) => f.seq === 1);
    // И следующая правка идёт рассылкой, без дыры.
    a.update('root', 2, edit(d, 2));
    await c.wait('updates', (f) => f.seq === 2);
    const mine = tab();
    for (const f of c.frames) if (f.t === 'updates') expect(mine.import(f.bytes).pending ?? null).toBeNull();
    expect(mine.getMap('meta').get('x')).toBe(2);
  } finally {
    restore();
  }
});

test('подписка под непрерывной записью: у каждого подписчика все записи по порядку', async () => {
  const w = await joined(owner);
  const d = tab();
  let n = 0;
  let stop = false;
  const writing = (async () => {
    while (!stop) {
      w.update('root', n, edit(d, n));
      n++;
      await Bun.sleep(4);
    }
  })();
  const subs: Client[] = [];
  for (let i = 0; i < 30; i++) {
    subs.push(await joined(editor));
    await Bun.sleep(30);
  }
  stop = true;
  await writing;
  await w.wait('ack', (f) => f.clientSeq === n - 1, 20_000);
  for (const c of subs) {
    await c.wait('updates', (f) => f.seq === n, 20_000);
    const got = c.frames.flatMap((f) => (f.t === 'updates' ? [f.seq] : []));
    // Подписчик знает начало по версии подписки; дальше — подряд, без пропусков и повторов.
    expect(got).toEqual(Array.from({ length: got.length }, (_, i) => n - got.length + 1 + i));
    const sub = c.frames.find((f) => f.t === 'subscribed')!;
    expect(sub.t === 'subscribed' && sub.seq).toBeGreaterThanOrEqual(n - got.length);
  }
}, 60_000);

test('правка, отправленная, пока строка документа заводится, не заводит её с пустой версией', async () => {
  const c = await connect(editor);
  const d = tab();
  let held = false;
  const restore = intercept(
    (text) => !held && text.startsWith('SELECT vv, base_vv FROM doc_state'),
    async (run) => {
      held = true;
      // Подписка ждёт базу, а пачка уже пришла.
      c.update('root', 1, edit(d, 1));
      await c.wait('refused', (f) => f.clientSeq === 1);
      return run();
    },
  );
  try {
    c.subscribe('root', peers(root));
    await c.wait('subscribed');
  } finally {
    restore();
  }
  expect(c.frames.find((f) => f.t === 'refused')).toMatchObject({ code: 'UNAVAILABLE' });
  const { rows } = await s.pool.query<{ base_vv: Record<string, number> }>('SELECT base_vv FROM doc_state WHERE file_id = $1', [file]);
  expect(rows[0].base_vv).toEqual(Object.fromEntries(peers(root)));
  // Следующая правка того же клиента пишется.
  c.update('root', 2, d.export({ mode: 'update', from: root.oplogVersion() }));
  await c.wait('ack', (f) => f.clientSeq === 2);
});

test('сбой проверки при заведении строки: строки с пустой версией нет, соединение переподключится', async () => {
  const v = s.hub.validator;
  const real = v.baseVersion.bind(v);
  v.baseVersion = async () => null;
  const saved = console.error;
  console.error = () => {};
  try {
    const c = await connect(editor);
    c.subscribe('root', peers(root));
    expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.GOING_AWAY);
  } finally {
    v.baseVersion = real;
    console.error = saved;
  }
  expect((await s.pool.query('SELECT 1 FROM doc_state WHERE file_id = $1', [file])).rows).toHaveLength(0);
  const again = await joined(editor);
  again.update('root', 1, edit(tab(), 1));
  await again.wait('ack');
});

test('журнал отверг подделку: зависящая от неё пачка не проходит, ни следом, ни вперемешку', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const da = tab();
  a.update('root', 1, edit(da, 1));
  await a.wait('ack');
  // Второй человек подделывает следующую правку пира первого и шлёт свою, зависящую от подделки, не дожидаясь ответа.
  const forged = edit(da, 'подделка');
  const mine = tab();
  mine.import(da.export({ mode: 'update', from: root.oplogVersion() }));
  const dependent = edit(mine, 'зависит');
  b.update('root', 1, forged);
  b.update('root', 2, dependent);
  expect((await b.wait('refused', (f) => f.clientSeq === 1)).code).toBe('FOREIGN_PEER');
  // Настоящий клиент чужой пир не шлёт: соединение закрыто сразу.
  expect((await b.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);
  // И следом, с нового соединения.
  const again = await joined(editor);
  again.update('root', 3, dependent);
  expect((await again.wait('refused', (f) => f.clientSeq === 3)).code).toBe('GAP');
  // Документ проверки загружен заново: настоящие правки снова пишутся.
  a.update('root', 2, edit(tab(), 2));
  await a.wait('ack', (f) => f.clientSeq === 2);
  // Журнал воспроизводится с нуля без ждущих правок.
  const replay = tab();
  for (const r of (await s.pool.query<{ bytes: Uint8Array }>('SELECT bytes FROM doc_update WHERE file_id = $1 ORDER BY seq', [file])).rows)
    expect(replay.import(r.bytes).pending ?? null).toBeNull();
});

test('читатель не пишет: отказ, журнал пуст', async () => {
  const v = await joined(viewer);
  v.update('root', 1, edit(tab(), 1));
  const r = await v.wait('refused');
  expect(r.code).toBe('FORBIDDEN');
  expect((await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file])).rows).toHaveLength(0);
});

test('испорченная пачка не попадает в журнал и другим, другие работают', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const bad = edit(tab(), 1).slice();
  bad[bad.length - 2] ^= 0xff;
  a.update('root', 1, bad);
  expect((await a.wait('refused')).code).toBe('BAD_UPDATE');
  // Снимок вместо обновления — тоже.
  a.update('root', 2, root.export({ mode: 'snapshot' }));
  await a.wait('refused', (f) => f.clientSeq === 2 && f.code === 'BAD_UPDATE');
  const logged = await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file]);
  expect(logged.rows).toHaveLength(0);
  expect(b.frames.filter((f) => f.t === 'updates')).toEqual([]);
  a.update('root', 3, edit(tab(), 2));
  await a.wait('ack', (f) => f.clientSeq === 3);
  await b.wait('updates');
});

test('паника Loro на пачке: отказ, файл цел, остальные пишут дальше', async () => {
  const base = await data('loro-base.snap');
  const panic = await data('loro-panic-real.update');
  const f = (await s.newFile(owner)).id;
  expect((await s.writeContent(owner, f, 0, realContainer(base))).status).toBe(200);
  const version = new Map([...LoroDoc.fromSnapshot(base).oplogVersion().toJSON()].map(([p, c]) => [String(p), c]));
  const c = await connect(owner, f);
  c.subscribe('root', version);
  await c.wait('subscribed');
  const saved = console.error;
  console.error = () => {};
  c.update('root', 1, panic);
  expect((await c.wait('refused', (x) => x.clientSeq === 1)).code).toBe('BAD_UPDATE');
  // Пачка уронила Loro и на свежем worker'е: соединение закрыто, клиент переподключится.
  expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);
  console.error = saved;
  const good = await data('loro-good.update');
  const again = await connect(owner, f);
  again.subscribe('root', version);
  await again.wait('subscribed');
  again.update('root', 2, good);
  await again.wait('ack', (x) => x.clientSeq === 2);
  expect((await s.pool.query('SELECT seq FROM doc_update WHERE file_id = $1', [f])).rows).toHaveLength(1);
});

test('чужой пир не проходит: подделка чужих правок', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const d = tab();
  a.update('root', 1, edit(d, 1));
  await a.wait('ack');
  // Тот же документ — тот же пир: второй человек шлёт следующую правку от него.
  b.update('root', 1, edit(d, 2));
  expect((await b.wait('refused')).code).toBe('FOREIGN_PEER');
  // Подделка — не ошибка настоящего клиента: соединение закрыто, годная пачка этого не исправит.
  expect((await b.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);
  // Соавтор пишет дальше.
  a.update('root', 2, edit(tab(), 3));
  await a.wait('ack', (f) => f.clientSeq === 2);
});

test('пачка, роняющая Loro, закрывает соединение и копится на человеке: годная пачка удар не прощает', async () => {
  const tight = await apiStand(
    { syncLimits: { userFaultsBurst: 2, userFaultsPerSecond: 0.0001, recheckMs: 3_600_000, sessionCheckMs: 3_600_000 } },
    true,
  );
  const saved = console.error;
  console.error = () => {};
  try {
    const base = await data('loro-base.snap');
    const panic = await data('loro-panic-real.update');
    const who = await tight.signIn(`panic-${crypto.randomUUID()}@example.com`);
    const f = (await tight.newFile(who)).id;
    expect((await tight.writeContent(who, f, 0, realContainer(base))).status).toBe(200);
    const version = new Map([...LoroDoc.fromSnapshot(base).oplogVersion().toJSON()].map(([p, c]) => [String(p), c]));
    // Свой пир: паника-образец пишет счётчики пира 1, и годная пачка на них же сделала бы её известной.
    const mine = LoroDoc.fromSnapshot(base);
    for (let round = 0; round < 2; round++) {
      const c = (await Client.open(tight, who, f))!;
      c.hello();
      c.subscribe('root', version);
      await c.wait('subscribed');
      // Годная пачка перед паникой: удар за панику она не сбрасывает.
      c.update('root', 1, edit(mine, round));
      await c.wait('ack', (x) => x.clientSeq === 1);
      c.update('root', 2, panic);
      expect((await c.wait('refused', (x) => x.clientSeq === 2)).code).toBe('BAD_UPDATE');
      expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);
    }
    // Удары копятся на человеке, а не на соединении: новое соединение не откроется, пока бакет не наполнится.
    expect(await Client.open(tight, who, f)).toBeNull();
    expect((await tight.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [f])).rows).toHaveLength(2);
  } finally {
    console.error = saved;
    await tight.stop();
  }
});

test('hello сверяет роль: снятая или пониженная, пока соединение открывалось, видна сразу', async () => {
  // Роль прочитана при открытии, а до hello её понизили мимо сверки.
  const lowered = (await Client.open(s, editor, file))!;
  open.push(lowered);
  await s.pool.query(`UPDATE file_access SET role = 'viewer' WHERE file_id = $1 AND user_id = $2`, [file, editor.id]);
  lowered.hello();
  expect((await lowered.wait('role')).role).toBe('viewer');
  lowered.subscribe('root', peers(root));
  await lowered.wait('subscribed');
  lowered.update('root', 1, edit(tab(), 1));
  expect((await lowered.wait('refused')).code).toBe('FORBIDDEN');
  // Сняли совсем.
  const gone = (await Client.open(s, editor, file))!;
  open.push(gone);
  await s.pool.query(`DELETE FROM file_access WHERE file_id = $1 AND user_id = $2`, [file, editor.id]);
  gone.hello();
  expect((await gone.waitClosed()).code).toBe(SYNC_CLOSE.REVOKED);
});

test('подписка с огромным вектором платит своим маленьким бакетом: сотни разом не держат память процесса', async () => {
  const big = new Map<string, number>();
  for (let i = 1; i <= 65_536; i++) big.set(String(i * 1000003), 1);
  const bytes = encodeVersion(big);
  const v = await connect(viewer);
  // 40 кадров по 786 КБ — 31 МБ: больше бакета подписок (16 МБ), но меньше общего бакета байт (256 МБ).
  for (let i = 0; i < 40; i++) v.send(encodeClientFrame({ t: 'subscribe', doc: `${i + 1}@2`, version: bytes }));
  await v.wait('pause');
  expect(v.closed).toBeNull();
});

test('разрыв версий: GAP, клиент подписывается заново и шлёт от версии сервера', async () => {
  const a = await joined(owner);
  const d = tab();
  const first = edit(d, 1);
  const second = edit(d, 2);
  a.update('root', 2, second);
  expect((await a.wait('refused')).code).toBe('GAP');
  a.frames.length = 0;
  a.subscribe('root', peers(d));
  const sub = await a.wait('subscribed');
  expect(sub.seq).toBe(0);
  a.update('root', 3, first);
  a.update('root', 4, second);
  await a.wait('ack', (f) => f.clientSeq === 4);
});

test('пачка, зависящая от неизвестного серверу чужого пира: вектор по её пирам в порядке, а применить нельзя — GAP', async () => {
  const a = await joined(owner);
  const other = tab();
  edit(other, 1);
  const mine = tab();
  mine.import(other.export({ mode: 'update', from: root.oplogVersion() }));
  const bytes = edit(mine, 2);
  a.update('root', 1, bytes);
  expect((await a.wait('refused')).code).toBe('GAP');
  expect((await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file])).rows).toHaveLength(0);
});

test('повтор пачки после обрыва безвреден: ack без новой записи', async () => {
  const a = await joined(owner);
  const bytes = edit(tab(), 1);
  a.update('root', 1, bytes);
  await a.wait('ack', (f) => f.clientSeq === 1);
  a.update('root', 2, bytes);
  const ack = await a.wait('ack', (f) => f.clientSeq === 2);
  expect(ack.seq).toBe(1);
  expect((await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file])).rows).toHaveLength(1);
});

test('догоняние: только то, чего нет у клиента, и версия сервера', async () => {
  const a = await joined(owner);
  const d = tab();
  for (let i = 0; i < 5; i++) a.update('root', i, edit(d, i));
  await a.wait('ack', (f) => f.clientSeq === 4);
  const behind = LoroDoc.fromSnapshot(root.export({ mode: 'snapshot' }));
  behind.import(d.export({ mode: 'update', from: root.oplogVersion() }));
  const c = await connect(editor);
  c.subscribe('root', peers(root));
  const sub = await c.wait('subscribed');
  expect(sub.seq).toBe(5);
  expect(c.frames.filter((f) => f.t === 'updates')).toHaveLength(5);
  // Уже всё знает — ни одной записи.
  const c2 = await connect(editor);
  c2.subscribe('root', peers(behind));
  await c2.wait('subscribed');
  expect(c2.frames.filter((f) => f.t === 'updates')).toEqual([]);
});

test('отстал сильнее журнала: snapshot-needed и снимок документа', async () => {
  const c = await connect(editor);
  await s.pool.query(`UPDATE doc_state SET base_vv = '{"123":5}' WHERE file_id = $1`, [file]);
  await s.pool.query(
    `INSERT INTO doc_state (file_id, doc, base_vv, vv) VALUES ($1, 'root', '{"123":5}', '{"123":5}')
     ON CONFLICT (file_id, doc) DO UPDATE SET base_vv = excluded.base_vv`,
    [file],
  );
  c.subscribe('root', peers(root));
  const need = await c.wait('snapshot-needed');
  const res = await s.call(editor, 'GET', need.url);
  expect(res.status).toBe(200);
  const snapshot = LoroDoc.fromSnapshot(new Uint8Array(await res.arrayBuffer()));
  expect(snapshot.getMap('meta').get('name')).toBe('файл');
  // Чужой — нет, как у любого маршрута файла.
  expect((await s.call(stranger, 'GET', need.url)).status).toBe(404);
});

test('снятие роли закрывает соединение сразу', async () => {
  const c = await joined(editor);
  const { rows } = await s.pool.query<{ id: string }>('SELECT id FROM file_access WHERE file_id = $1 AND user_id = $2', [file, editor.id]);
  const res = await s.call(owner, 'DELETE', `/api/files/${file}/share/people/${rows[0].id}`);
  expect(res.status).toBe(200);
  expect(await c.wait('access-revoked')).toEqual({ t: 'access-revoked', reason: 'access' });
  expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.REVOKED);
});

test('понижение до читателя: сообщение сразу, запись отказывает', async () => {
  const c = await joined(editor);
  const { rows } = await s.pool.query<{ id: string }>('SELECT id FROM file_access WHERE file_id = $1 AND user_id = $2', [file, editor.id]);
  const res = await s.call(owner, 'PATCH', `/api/files/${file}/share/people/${rows[0].id}`, { role: 'viewer' });
  expect(res.status).toBe(200);
  const r = await c.wait('role', (f) => f.role === 'viewer');
  expect(r.canShare).toBe(false);
  c.update('root', 1, edit(tab(), 1));
  expect((await c.wait('refused')).code).toBe('FORBIDDEN');
  expect(c.closed).toBeNull();
});

test('понижение с пачками в пути: отказы не считаются нарушением, соединение живо', async () => {
  const c = await joined(editor);
  const { rows } = await s.pool.query<{ id: string }>('SELECT id FROM file_access WHERE file_id = $1 AND user_id = $2', [file, editor.id]);
  expect((await s.call(owner, 'PATCH', `/api/files/${file}/share/people/${rows[0].id}`, { role: 'viewer' })).status).toBe(200);
  await c.wait('role', (f) => f.role === 'viewer');
  const d = tab();
  for (let i = 0; i < 20; i++) c.update('root', i, edit(d, i));
  // И пачки в документ, подписку на который сняло `snapshot-needed`.
  for (let i = 0; i < 20; i++) c.update('9@9', 100 + i, edit(d, i));
  await c.wait('refused', (f) => f.clientSeq === 119);
  expect(c.frames.filter((f) => f.t === 'refused')).toHaveLength(40);
  expect(c.closed).toBeNull();
});

test('сверка ролей не выгоняет того, кто открыл соединение, пока она шла', async () => {
  const a = await connect(owner);
  let opened: Client | null = null;
  const restore = intercept(
    (text) => text.includes('unnest($2::text[]) AS x(u)'),
    async (run) => {
      // Запрос сверки ушёл; человек открывает файл.
      opened = await connect(editor);
      return run();
    },
  );
  try {
    await s.hub.recheckFile(file);
  } finally {
    restore();
  }
  const b = opened as Client | null;
  expect(b).not.toBeNull();
  // Всё, что сервер послал до ответа на `ping`, уже дошло.
  b!.send(encodeClientFrame({ t: 'ping' }));
  await b!.wait('pong');
  expect(b!.frames.filter((f) => f.t === 'access-revoked')).toEqual([]);
  expect(b!.closed).toBeNull();
  expect(a.closed).toBeNull();
});

test('запись проверяет роль в транзакции: снятая мимо сервера не пишет, даже пока соединение думает иначе', async () => {
  const c = await joined(editor);
  await s.pool.query(`UPDATE file_access SET role = 'viewer' WHERE file_id = $1 AND user_id = $2`, [file, editor.id]);
  c.update('root', 1, edit(tab(), 1));
  expect((await c.wait('refused')).code).toBe('FORBIDDEN');
  expect((await s.pool.query('SELECT 1 FROM doc_update WHERE file_id = $1', [file])).rows).toHaveLength(0);
});

test('ссылка: выключение и новая ссылка закрывают пришедших по ней', async () => {
  const link = await s.call(owner, 'PUT', `/api/files/${file}/share/link`, { role: 'editor', enabled: true });
  expect(link.status).toBe(200);
  await s.pool.query('INSERT INTO file_link_user (file_id, user_id) VALUES ($1, $2)', [file, stranger.id]);
  const c = await joined(stranger);
  const off = await s.call(owner, 'PUT', `/api/files/${file}/share/link`, { role: 'editor', enabled: false });
  expect(off.status).toBe(200);
  await c.wait('access-revoked');
  expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.REVOKED);
});

test('корзина закрывает всех, кроме убравшего', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  expect((await s.call(owner, 'POST', `/api/files/${file}/trash`)).status).toBe(200);
  await b.wait('access-revoked');
  expect((await b.waitClosed()).code).toBe(SYNC_CLOSE.REVOKED);
  expect(a.closed).toBeNull();
});

test('выход закрывает соединения этой сессии, выход на всех — все соединения человека', async () => {
  const other = await s.signIn('multi@example.com');
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor')`, [file, other.id]);
  const c = await joined(other);
  const res = await s.call(other, 'POST', '/api/auth/sign-out', {});
  expect(res.status).toBe(200);
  expect(await c.wait('access-revoked')).toEqual({ t: 'access-revoked', reason: 'session' });
  expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.SESSION);

  const again = await s.signIn('multi@example.com');
  const c1 = await joined(again);
  const c2 = await joined(again);
  expect((await s.call(again, 'POST', '/api/auth/sign-out-everywhere', {})).status).toBe(200);
  expect((await c1.waitClosed()).code).toBe(SYNC_CLOSE.SESSION);
  expect((await c2.waitClosed()).code).toBe(SYNC_CLOSE.SESSION);
});

test('снятие команды закрывает соединения участника', async () => {
  const team = (await (await s.call(owner, 'POST', '/api/teams', { name: 'Т' })).json()) as { id: string };
  const project = (await (await s.call(owner, 'POST', `/api/teams/${team.id}/projects`, { name: 'П' })).json()) as { id: string };
  const f = await s.newFile(owner, 'В проекте', project.id);
  const member = await s.signIn('member@example.com');
  await s.pool.query(`INSERT INTO team_member (team_id, user_id, role) VALUES ($1, $2, 'member')`, [team.id, member.id]);
  const c = await Client.open(s, member, f.id);
  expect(c).not.toBeNull();
  open.push(c!);
  c!.hello();
  await c!.wait('role');
  const { rows } = await s.pool.query<{ id: string }>('SELECT id FROM team_member WHERE team_id = $1 AND user_id = $2', [
    team.id,
    member.id,
  ]);
  expect((await s.call(owner, 'DELETE', `/api/teams/${team.id}/members/${rows[0].id}`)).status).toBe(200);
  await c!.wait('access-revoked');
});

test('настоящая работа не упирается в пределы: перетаскивание, набор и большая вставка', async () => {
  const a = await joined(owner);
  const d = tab();
  // Перетаскивание: 30 пачек в секунду на документ две секунды, разом корень и страница по очереди.
  const start = performance.now();
  for (let i = 0; i < 60; i++) {
    a.update('root', i, edit(d, i));
    while (performance.now() - start < (i + 1) * (1000 / 30)) await Bun.sleep(2);
  }
  await a.wait('ack', (f) => f.clientSeq === 59);
  // Набор и вставка: 300 пачек разом, 8 МБ в сумме.
  const blob = 'ж'.repeat(16 * 1024);
  for (let i = 0; i < 300; i++) {
    const from = d.oplogVersion();
    d.getMap('meta').set('t', `${i}${blob}`);
    d.commit();
    a.update('root', 1000 + i, d.export({ mode: 'update', from }));
  }
  await a.wait('ack', (f) => f.clientSeq === 1299);
  expect(a.frames.filter((f) => f.t === 'pause')).toEqual([]);
  expect(a.frames.filter((f) => f.t === 'refused')).toEqual([]);
  // И большая вставка кадрами почти по пределу.
  const big = 'ё'.repeat(5 * 1024 * 1024);
  for (let i = 0; i < 4; i++) {
    const from = d.oplogVersion();
    d.getMap('meta').set('big', `${i}${big}`);
    d.commit();
    a.update('root', 2000 + i, d.export({ mode: 'update', from }));
  }
  await a.wait('ack', (f) => f.clientSeq === 2003, 25_000);
  expect(a.frames.filter((f) => f.t === 'pause')).toEqual([]);
});

test('предел частоты — пауза с понятным сроком: кадр не принят, соединение живо, повтор проходит', async () => {
  const limited = await apiStand(
    { syncLimits: { framesPerSecond: 5, framesBurst: 8, recheckMs: 3_600_000, sessionCheckMs: 3_600_000 } },
    true,
  );
  try {
    const who = await limited.signIn('limited@example.com');
    const f = (await limited.newFile(who)).id;
    const c = (await Client.open(limited, who, f))!;
    c.hello();
    c.subscribe('root');
    await c.wait('subscribed');
    const d = new LoroDoc();
    const bytes: Uint8Array[] = [];
    for (let i = 0; i < 40; i++) bytes.push(edit(d, i));
    bytes.forEach((b, i) => c.update('root', i, b));
    const pause = await c.wait('pause');
    expect(pause.ms).toBeGreaterThan(0);
    expect(c.closed).toBeNull();
    // Клиент ждёт паузу и шлёт снова с версии сервера.
    c.frames.length = 0;
    await Bun.sleep(1500);
    c.subscribe('root', new Map());
    const sub = await c.wait('subscribed');
    const known = sub.seq;
    await Bun.sleep(1000);
    c.update('root', 100, d.export({ mode: 'update', from: new LoroDoc().oplogVersion() }));
    await c.wait('ack', (x) => x.clientSeq === 100);
    expect(known).toBeLessThan(40);
    c.close();
  } finally {
    await limited.stop();
  }
  // Сам тест ждёт паузу 2,5 с: на нагруженной машине общий срок теста в 5 с впритык.
}, 15_000);

test('сбой базы при записи: отказ «повторите», рассылки нет, процесс жив, повтор записывается', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const bytes = edit(tab(), 1);
  await s.failing('doc_update', 'INSERT', async () => {
    a.update('root', 1, bytes);
    expect((await a.wait('refused')).code).toBe('UNAVAILABLE');
  });
  expect(b.frames.filter((f) => f.t === 'updates')).toEqual([]);
  a.frames.length = 0;
  a.update('root', 2, bytes);
  await a.wait('ack', (f) => f.clientSeq === 2);
  await b.wait('updates');
});

test('Valkey не отвечает при открытии: 503, а не падение', async () => {
  const hub = new SyncHub(
    s.pool,
    s.blobs,
    {
      api: {
        getSession: () => Promise.reject(new Error('Valkey недоступен')),
      },
    } as never,
    s.publicUrl,
    { sessionCheckMs: 3_600_000 },
  );
  const refused = await hub.upgrade(new Request(s.publicUrl, { headers: { origin: s.publicUrl } }), s.server, file);
  expect(refused?.status).toBe(503);
  await hub.stop();
});

test('Valkey не отвечает при проверке сессий: люди не выгоняются; сессия снята — соединение закрывается', async () => {
  const tight = await apiStand({ syncLimits: { sessionCheckMs: 20 } }, true);
  try {
    const who = await tight.signIn('valkey@example.com');
    const f = (await tight.newFile(who)).id;
    const c = (await Client.open(tight, who, f))!;
    c.hello();
    await c.wait('role');
    const api = tight.auth.api as { getSession: (...a: unknown[]) => Promise<unknown> };
    const real = api.getSession;
    let failed = 0;
    api.getSession = () => {
      failed++;
      return Promise.reject(new Error('Valkey недоступен'));
    };
    const saved = console.error;
    console.error = () => {};
    try {
      while (failed < 5) await Bun.sleep(10);
    } finally {
      api.getSession = real;
      console.error = saved;
    }
    expect(c.closed).toBeNull();
    // Хранилище вернулось, а сессии больше нет: следующая проверка закрывает.
    const list = await tight.valkey.get(`active-sessions-${who.id}`);
    for (const x of JSON.parse(list!) as { token: string }[]) await tight.valkey.del(x.token);
    await tight.valkey.del(`active-sessions-${who.id}`);
    expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.SESSION);
  } finally {
    await tight.stop();
  }
});

test('сессия снята мимо сервера: периодическая проверка закрывает соединение', async () => {
  const tight = await apiStand({ syncLimits: { sessionCheckMs: 50 } }, true);
  try {
    const who = await tight.signIn('expired@example.com');
    const f = (await tight.newFile(who)).id;
    const c = (await Client.open(tight, who, f))!;
    c.hello();
    await c.wait('role');
    const { rows } = await tight.pool.query<{ id: string }>('SELECT id FROM "user" WHERE email = $1', ['expired@example.com']);
    expect(rows[0].id).toBe(who.id);
    const list = await tight.valkey.get(`active-sessions-${who.id}`);
    for (const x of JSON.parse(list!) as { token: string }[]) await tight.valkey.del(x.token);
    await tight.valkey.del(`active-sessions-${who.id}`);
    expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.SESSION);
  } finally {
    await tight.stop();
  }
});

test('предел соединений и документов на соединение', async () => {
  const tight = await apiStand({ syncLimits: { connectionsPerUser: 2, subscribesPerConnection: 2 } }, true);
  try {
    const who = await tight.signIn('tight@example.com');
    const f = (await tight.newFile(who)).id;
    const [c1, c2] = [(await Client.open(tight, who, f))!, (await Client.open(tight, who, f))!];
    expect(await Client.open(tight, who, f)).toBeNull();
    c1.hello();
    c1.send(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) }));
    c1.subscribe('1@1');
    c1.subscribe('2@1');
    expect((await c1.waitClosed()).code).toBe(SYNC_CLOSE.BAD_FRAME);
    c2.close();
    await c2.waitClosed();
    // Место освободилось.
    const c3 = await Client.open(tight, who, f);
    expect(c3).not.toBeNull();
    c3?.close();
  } finally {
    await tight.stop();
  }
});

test('пределы человека не обнуляются переподключением, открытия ограничены', async () => {
  const tight = await apiStand(
    {
      syncLimits: {
        userFramesPerSecond: 0.001,
        userFramesBurst: 8,
        userOpensPerSecond: 0.001,
        userOpensBurst: 11,
        recheckMs: 3_600_000,
        sessionCheckMs: 3_600_000,
      },
    },
    true,
  );
  try {
    const who = await tight.signIn('reconnect@example.com');
    const f = (await tight.newFile(who)).id;
    let subscribed = 0;
    for (let round = 0; round < 10; round++) {
      const c = (await Client.open(tight, who, f))!;
      c.hello();
      await c.wait('role');
      for (const doc of ['root', '1@1', '2@1']) c.subscribe(doc);
      const answered = () => c.frames.filter((x) => x.t === 'subscribed' || x.t === 'pause').length;
      while (answered() < 3) await Bun.sleep(5);
      subscribed += c.frames.filter((x) => x.t === 'subscribed').length;
      c.close();
      await c.waitClosed();
    }
    // Бакет человека один на все его соединения: 8 кадров на десять переподключений.
    expect(subscribed).toBe(8);
    // Открытия: 11 за всё время, дальше — отказ, пока бакет не наполнится.
    expect(await Client.open(tight, who, f)).not.toBeNull();
    expect(await Client.open(tight, who, f)).toBeNull();
  } finally {
    await tight.stop();
  }
});

test('подписка платит байтами и хранит только пиры, которые знает сервер', async () => {
  const big = new Map<string, number>();
  for (let i = 1; i <= 20_000; i++) big.set(String(i * 1000003), 1);
  for (const [p, c] of peers(root)) big.set(p, c);
  const v = await connect(viewer);
  for (let i = 0; i < 5; i++) v.subscribe(i === 0 ? 'root' : `${i}@1`, big);
  await v.wait('subscribed', (f) => f.doc === '4@1');
  // Корень знает одного пира, страницы без снимка — ни одного.
  expect(s.hub.versionEntries()).toBe(1);
  const tight = await apiStand(
    { syncLimits: { userSubscribeBytesBurst: 300_000, userSubscribeBytesPerSecond: 1, recheckMs: 3_600_000 } },
    true,
  );
  try {
    const who = await tight.signIn('bytes@example.com');
    const f = (await tight.newFile(who)).id;
    const c = (await Client.open(tight, who, f))!;
    c.hello();
    await c.wait('role');
    for (let i = 0; i < 3; i++) c.subscribe(`${i + 1}@1`, big);
    await c.wait('pause');
    expect(c.frames.filter((x) => x.t === 'subscribed').length).toBeLessThan(3);
    c.close();
  } finally {
    await tight.stop();
  }
});

test('документы без снимка: читатель не заводит строки, первая запись заводит', async () => {
  const v = await connect(viewer);
  v.subscribe('5@5');
  const sub = await v.wait('subscribed');
  expect(sub.seq).toBe(0);
  expect((await s.pool.query(`SELECT 1 FROM doc_state WHERE file_id = $1 AND doc = '5@5'`, [file])).rows).toHaveLength(0);
  const a = await connect(owner);
  a.subscribe('5@5');
  await a.wait('subscribed');
  a.update('5@5', 1, edit(new LoroDoc(), 1));
  await a.wait('ack');
  expect((await s.pool.query(`SELECT 1 FROM doc_state WHERE file_id = $1 AND doc = '5@5'`, [file])).rows).toHaveLength(1);
  await v.wait('updates');
});

test('пока идёт журнал, запись файла целиком отказывает: она затёрла бы правки', async () => {
  const a = await joined(owner);
  a.update('root', 1, edit(tab(), 1));
  await a.wait('ack');
  const res = await s.writeContent(owner, file, 1, realContainer(root.export({ mode: 'snapshot' })));
  expect(res.status).toBe(409);
  expect((await s.pool.query<{ version: string }>('SELECT version FROM file WHERE id = $1', [file])).rows[0].version).toBe('1');
});

test('файл заменён целиком при открытых соединениях: они закрываются и после переподключения видят новые байты', async () => {
  const a = await joined(owner);
  const b = await joined(editor);
  const next = tab();
  next.getMap('meta').set('name', 'новое');
  next.commit();
  expect((await s.writeContent(owner, file, 1, realContainer(next.export({ mode: 'snapshot' })))).status).toBe(200);
  // Строки документов удалены: без переподключения их завела бы следующая пачка, с пустой версией.
  expect((await a.waitClosed()).code).toBe(SYNC_CLOSE.GOING_AWAY);
  expect((await b.waitClosed()).code).toBe(SYNC_CLOSE.GOING_AWAY);
  // Подписка со старой версией: новый снимок её не покрывает — снимок.
  const c = await connect(editor);
  c.subscribe('root', peers(root));
  await c.wait('snapshot-needed');
});

test('schema из hello: поднимает только редактор, не выше известной сборке и не вниз', async () => {
  const schema = async () =>
    (await s.pool.query<{ schema_version: number }>('SELECT schema_version FROM file WHERE id = $1', [file])).rows[0].schema_version;
  const hello = async (who: Person, value: number) => {
    const c = (await Client.open(s, who, file))!;
    open.push(c);
    c.send(encodeClientFrame({ t: 'hello', protocol: 1, schema: value }));
    await c.wait('role');
  };
  await hello(viewer, 4_000_000_000);
  expect(await schema()).toBe(1);
  await hello(owner, 7);
  expect(await schema()).toBe(7);
  await hello(editor, 3);
  expect(await schema()).toBe(7);
  await hello(editor, 4_000_000_000);
  // `knownSchema` стенда — 9.
  expect(await schema()).toBe(9);
});

test('сбой базы на hello: соединение закрывается, клиент переподключится, а не ждёт роли вечно', async () => {
  const restore = intercept(
    (text) => text.includes('schema_version'),
    async () => {
      throw new Error('сбой теста');
    },
  );
  const saved = console.error;
  console.error = () => {};
  try {
    const c = (await Client.open(s, owner, file))!;
    open.push(c);
    c.hello();
    expect((await c.waitClosed()).code).toBe(SYNC_CLOSE.GOING_AWAY);
  } finally {
    restore();
    console.error = saved;
  }
});

test('схема сборки сервера — та же, что у ядра', async () => {
  const rust = await Bun.file(new URL('../../../crates/ferrum-doc/src/document.rs', import.meta.url)).text();
  expect(rust).toContain(`pub const SCHEMA_VERSION: u32 = ${SERVER_SCHEMA};`);
});
