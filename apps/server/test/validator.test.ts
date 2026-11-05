// Проверка пачек на настоящем Loro (спека подсистемы 4, раздел 7.3): порченые байты, снимок вместо обновления, паника
// wasm на настоящем документе при успехе на пустом, замена worker'а без потери чужих пачек. `loro-*` — найденные
// случайной порчей: контрольная сумма верна, разбор проходит, Loro падает при применении.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { containerHeader } from '@ferrum/protocol';
import { blake3 } from '@noble/hashes/blake3.js';
import { LoroDoc } from 'loro-crdt';
import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { Pool } from 'pg';
import { apiStand, type Person } from './api-stand';
import { Validator, type ValidatorOptions } from '../src/sync/validator';

const data = (name: string) => Bun.file(new URL(`./data/${name}`, import.meta.url)).bytes();
const base = await data('loro-base.snap');
const good = await data('loro-good.update');
const panicAny = await data('loro-panic-any.update');
const panicReal = await data('loro-panic-real.update');

const s = await apiStand();
afterAll(s.stop);
const owner: Person = await s.signIn('owner@example.com');
let file = '';
let v: Validator;

function section(kind: number, id: string, bytes: Uint8Array): Uint8Array {
  const idb = new TextEncoder().encode(id);
  const out = new Uint8Array(5 + idb.length + 40 + bytes.length);
  const dv = new DataView(out.buffer);
  out[0] = kind;
  dv.setUint32(1, idb.length, true);
  out.set(idb, 5);
  dv.setBigUint64(5 + idb.length, BigInt(bytes.length), true);
  out.set(blake3(bytes), 13 + idb.length);
  out.set(bytes, 45 + idb.length);
  return out;
}

const fileWith = (root: Uint8Array): Uint8Array => Buffer.concat([containerHeader(1), section(1, '', root)]);

/** Файл с настоящим снимком `base`. */
async function baseFile(): Promise<string> {
  const f = (await s.newFile(owner)).id;
  expect((await s.writeContent(owner, f, 0, fileWith(base))).status).toBe(200);
  return f;
}

/** Записи журнала мимо журнала: проверка читает то, что лежит в базе. */
async function logged(f: string, rows: Uint8Array[]): Promise<void> {
  await s.pool.query(`INSERT INTO doc_state (file_id, doc, next_seq) VALUES ($1, 'root', $2)`, [f, rows.length + 1]);
  for (const [i, b] of rows.entries()) {
    await s.pool.query(`INSERT INTO doc_update (file_id, doc, seq, start_vv, end_vv, bytes) VALUES ($1, 'root', $2, '{}', '{}', $3)`, [
      f,
      i + 1,
      Buffer.from(b),
    ]);
  }
}

/** Правки нового пира поверх снимка `base`: каждая — своя пачка. */
function chain(n: number): Uint8Array[] {
  const d = LoroDoc.fromSnapshot(base);
  d.setPeerId(77n as never);
  const out: Uint8Array[] = [];
  for (let i = 0; i < n; i++) {
    const from = d.oplogVersion();
    d.getMap('m').set('k', i);
    d.commit();
    out.push(d.export({ mode: 'update', from }));
  }
  return out;
}

const faultMark = join(tmpdir(), `ferrum-validator-fault-${process.pid}`);
const faulty = (over: Partial<ValidatorOptions> = {}) =>
  new Validator(s.pool, s.blobs, { workerUrl: new URL('./validator-faulty.worker.ts', import.meta.url), ...over });

beforeEach(async () => {
  v?.stop();
  await s.reset();
  v = new Validator(s.pool, s.blobs);
  file = (await s.newFile(owner)).id;
  const res = await s.writeContent(owner, file, 0, fileWith(base));
  expect(res.status).toBe(200);
});

test('верная пачка: версии начала и конца, документ принял', async () => {
  const r = await v.validate(file, 'root', good);
  expect(r.kind).toBe('ok');
  if (r.kind !== 'ok') return;
  expect(r.pending).toBe(false);
  expect(Object.keys(r.end)).toEqual(['1']);
  expect(r.end['1']).toBeGreaterThan(r.start['1']);
});

test('мусор и обрезка отвергаются', async () => {
  expect((await v.validate(file, 'root', new Uint8Array([1, 2, 3]))).kind).toBe('bad');
  expect((await v.validate(file, 'root', good.subarray(0, 40))).kind).toBe('bad');
  const flipped = good.slice();
  flipped[60] ^= 0xff;
  expect((await v.validate(file, 'root', flipped)).kind).toBe('bad');
});

test('снимок вместо обновления не принимается: он раздул бы журнал и обошёл бы проверку', async () => {
  expect((await v.validate(file, 'root', base)).kind).toBe('bad');
  const d = new LoroDoc();
  d.getMap('m').set('a', 1);
  d.commit();
  expect((await v.validate(file, 'root', d.export({ mode: 'snapshot' }))).kind).toBe('bad');
});

test('пачка без предыдущих правок: ждёт, а не проходит в журнал', async () => {
  const a = LoroDoc.fromSnapshot(base);
  const from = a.oplogVersion();
  a.getMap('m').set('a', 1);
  a.commit();
  const middle = a.oplogVersion();
  a.getMap('m').set('a', 2);
  a.commit();
  const r = await v.validate(file, 'root', a.export({ mode: 'update', from: middle }));
  expect(r.kind === 'ok' && r.pending).toBe(true);
  const r2 = await v.validate(file, 'root', a.export({ mode: 'update', from }));
  expect(r2.kind === 'ok' && r2.pending).toBe(false);
});

test('паника Loro на настоящем документе при успехе на пустом: отказ, worker заменён, остальные пачки целы', async () => {
  // Сам образец: на пустом документе проходит, на настоящем падает. Без настоящего документа проверка бы его пропустила.
  const empty = new LoroDoc();
  const saved = console.error;
  console.error = () => {};
  try {
    expect(() => empty.import(panicReal)).not.toThrow();
  } finally {
    console.error = saved;
  }
  expect((await v.validate(file, 'root', panicReal)).kind).toBe('bad');
  // Первая паника — повтор на новом worker'е с заново загруженным документом, вторая — вина пачки.
  expect(v.restarts).toBe(2);
  // Новый worker загрузил документ заново и работает.
  const r = await v.validate(file, 'root', good);
  expect(r.kind).toBe('ok');
});

test('снимок не читается: проверка идёт на пустом документе, паника и там отвергается', async () => {
  const f = (await s.newFile(owner)).id;
  expect((await s.writeContent(owner, f, 0, fileWith(new Uint8Array([7, 7, 7, 7])))).status).toBe(200);
  expect((await v.validate(f, 'root', panicAny)).kind).toBe('bad');
  expect(v.restarts).toBe(2);
});

test('очередь проверки ограничена: сверх предела — «повторите», байты не копятся', async () => {
  const big = new Uint8Array(300 * 1024 * 1024);
  expect((await v.validate(file, 'root', big)).kind).toBe('retry');
});

test('снимок не читается (записан не Ferrum): файл живёт, разбор всё равно проверяется', async () => {
  const f = (await s.newFile(owner)).id;
  const res = await s.writeContent(owner, f, 0, fileWith(new Uint8Array([7, 7, 7, 7])));
  expect(res.status).toBe(200);
  expect((await v.validate(f, 'root', good)).kind).toBe('ok');
  expect((await v.validate(f, 'root', new Uint8Array([1]))).kind).toBe('bad');
});

test('версия снимка — без загрузки документа, чужие байты — пустая', async () => {
  const b = await v.baseVersion(base);
  expect(Object.keys(b ?? {})).toEqual(['1']);
  expect(await v.baseVersion(new Uint8Array([1, 2, 3]))).toEqual({});
  expect(await v.baseVersion(null)).toEqual({});
});

test("сбой worker'а при версии снимка — «повторите», а не пустая версия", async () => {
  const f = faulty();
  try {
    writeFileSync(`${faultMark}.base`, '');
    expect(await f.baseVersion(base)).toBeNull();
    expect(f.restarts).toBe(1);
    expect(Object.keys((await f.baseVersion(base)) ?? {})).toEqual(['1']);
  } finally {
    f.stop();
  }
});

test('журнал отверг пачку: документ проверки её забывает, и пачка на тех же счётчиках проверяется заново', async () => {
  // `good` и `panicReal` пишут те же счётчики пира 1. Loro пропускает известные счётчики: если бы `good` осталась в
  // документе, отравленная прошла бы проверку и попала в журнал.
  const ok = await v.validate(file, 'root', good);
  expect(ok.kind).toBe('ok');
  if (ok.kind !== 'ok') return;
  v.reject(ok.chain);
  const saved = console.error;
  console.error = () => {};
  try {
    expect((await v.validate(file, 'root', panicReal)).kind).toBe('bad');
  } finally {
    console.error = saved;
  }
});

test('ждущая пачка не остаётся в документе проверки: зависящая от неё тоже ждёт', async () => {
  const [u0, u1, u2] = chain(3);
  const first = await v.validate(file, 'root', u1);
  expect(first.kind === 'ok' && first.pending).toBe(true);
  const second = await v.validate(file, 'root', u0);
  expect(second.kind === 'ok' && second.pending).toBe(false);
  // `u1` журнал не получил: `u2` зависит от неё.
  const third = await v.validate(file, 'root', u2);
  expect(third.kind === 'ok' && third.pending).toBe(true);
});

test("паника не по вине пачки (нехватка памяти, испорченный модуль): повтор на новом worker'е, пачка принята", async () => {
  const f = faulty();
  try {
    writeFileSync(`${faultMark}.validate`, '');
    expect((await f.validate(file, 'root', good)).kind).toBe('ok');
    expect(f.restarts).toBe(1);
  } finally {
    f.stop();
  }
});

test('паника на журнале файла при загрузке: worker заменён, пачка не винится и не проходит', async () => {
  const f = await baseFile();
  await logged(f, [panicReal]);
  const saved = console.error;
  console.error = () => {};
  try {
    expect((await v.validate(f, 'root', good)).kind).toBe('retry');
  } finally {
    console.error = saved;
  }
  expect(v.restarts).toBe(1);
});

test('неприменимая запись журнала при загрузке: документ остаётся, проверка не слабеет', async () => {
  const f = await baseFile();
  await logged(f, [new Uint8Array([1, 2, 3])]);
  const [, u1] = chain(2);
  const saved = console.error;
  console.error = () => {};
  try {
    // Без документа (`doc = null`) ждущую пачку не отличить: прошла бы как готовая.
    const r = await v.validate(f, 'root', u1);
    expect(r.kind === 'ok' && r.pending).toBe(true);
  } finally {
    console.error = saved;
  }
});

test('журнал читается кусками: всё на месте', async () => {
  const f = await baseFile();
  const rows = chain(30);
  await logged(f, rows.slice(0, 29));
  const small = new Validator(s.pool, s.blobs, { logChunkBytes: 200 });
  try {
    const r = await small.validate(f, 'root', rows[29]);
    expect(r.kind === 'ok' && r.pending).toBe(false);
  } finally {
    small.stop();
  }
});

test('загрузок разом не больше предела: соединения пула и память основного потока ограничены', async () => {
  const files = await Promise.all(Array.from({ length: 6 }, () => baseFile()));
  let active = 0;
  let peak = 0;
  const pool = {
    connect: async () => {
      const c = await s.pool.connect();
      active++;
      peak = Math.max(peak, active);
      const release = c.release.bind(c);
      c.release = (e?: Error | boolean) => {
        active--;
        release(e);
      };
      return c;
    },
  } as unknown as Pool;
  const limited = new Validator(pool, s.blobs, { loadsAtOnce: 2 });
  try {
    const out = await Promise.all(files.map((f) => limited.validate(f, 'root', good)));
    expect(out.map((r) => r.kind)).toEqual(files.map(() => 'ok'));
    expect(peak).toBeLessThanOrEqual(2);
  } finally {
    limited.stop();
  }
});

test("память worker'а выше предела: замена без паузы при непрерывной работе, ни одной пачке не отказано", async () => {
  const f = await baseFile();
  const recycled = new Validator(s.pool, s.blobs, { memoryLimitBytes: 1 });
  try {
    const rows = chain(20);
    // Работа не прекращается: ждущих всегда больше нуля.
    const out = await Promise.all([
      ...rows.map((r) => recycled.validate(f, 'root', r)),
      ...rows.map(() => recycled.validate(file, 'root', good)),
    ]);
    expect(out.map((r) => r.kind).filter((k) => k !== 'ok')).toEqual([]);
    expect(recycled.recycles).toBeGreaterThan(0);
    expect(recycled.restarts).toBe(0);
  } finally {
    recycled.stop();
  }
});

test('бюджет памяти документов: пачка сверх него вытесняет давний документ, следующая пачка загружает его заново', async () => {
  const other = await baseFile();
  const run = async (budgetBytes: number) => {
    const b = new Validator(s.pool, s.blobs, { budgetBytes });
    try {
      for (const f of [file, other, file]) expect((await b.validate(f, 'root', good)).kind).toBe('ok');
      return b.loads;
    } finally {
      b.stop();
    }
  };
  expect(await run(64 * 1024 * 1024)).toBe(2);
  // Оба снимка влезают, а пачка второго документа — уже нет: вытесняется первый, ещё до следующей загрузки.
  expect(await run(2 * base.length + good.length)).toBe(3);
});

test('файл заменён во время загрузки: старые байты не загружаются после этого', async () => {
  const f = await baseFile();
  const { promise: gate, resolve: release } = Promise.withResolvers<void>();
  const blobs = { ...s.blobs, open: async (k: string) => (await gate, s.blobs.open(k)) } as typeof s.blobs;
  const slow = new Validator(s.pool, blobs);
  try {
    const verdict = slow.validate(f, 'root', good);
    for (let i = 0; i < 200 && slow.loads === 0; i++) await Bun.sleep(5);
    slow.forget(f);
    release();
    expect((await verdict).kind).toBe('retry');
  } finally {
    slow.stop();
  }
});
