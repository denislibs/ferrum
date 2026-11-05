// Проверка пачек на настоящем Loro (спека подсистемы 4, раздел 7.3): порченые байты, снимок вместо обновления, паника
// wasm на настоящем документе при успехе на пустом, замена worker'а без потери чужих пачек. `loro-*` — найденные
// случайной порчей: контрольная сумма верна, разбор проходит, Loro падает при применении.

import { afterAll, beforeEach, expect, test } from 'bun:test';
import { containerHeader } from '@ferrum/protocol';
import { blake3 } from '@noble/hashes/blake3.js';
import { LoroDoc } from 'loro-crdt';
import { apiStand, type Person } from './api-stand';
import { Validator } from '../src/sync/validator';

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
  expect(v.restarts).toBe(1);
  // Новый worker загрузил документ заново и работает.
  const r = await v.validate(file, 'root', good);
  expect(r.kind).toBe('ok');
});

test('снимок не читается: проверка идёт на пустом документе, паника и там отвергается', async () => {
  const f = (await s.newFile(owner)).id;
  expect((await s.writeContent(owner, f, 0, fileWith(new Uint8Array([7, 7, 7, 7])))).status).toBe(200);
  expect((await v.validate(f, 'root', panicAny)).kind).toBe('bad');
  expect(v.restarts).toBe(1);
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
  expect(Object.keys(b)).toEqual(['1']);
  expect(await v.baseVersion(new Uint8Array([1, 2, 3]))).toEqual({});
  expect(await v.baseVersion(null)).toEqual({});
});
