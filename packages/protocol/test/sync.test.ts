import { expect, test } from 'bun:test';
import {
  encodeClientFrame,
  encodeServerFrame,
  encodeVersion,
  MAX_SYNC_FRAME_BYTES,
  MAX_SYNC_UPDATE_BYTES,
  parseClientFrame,
  parseServerFrame,
  parseVersion,
  versionIncludes,
  type ClientFrame,
  type ServerFrame,
} from '../src/sync';

const version = encodeVersion(
  new Map([
    ['18446744073709551615', 7],
    ['12', 1],
  ]),
);
const doc = '2808@7910247476534078275';

test('кадры клиента туда и обратно', () => {
  const frames: ClientFrame[] = [
    { t: 'hello', protocol: 1, schema: 3 },
    { t: 'subscribe', doc: 'root', version },
    { t: 'update', doc, clientSeq: 4_000_000_000, bytes: new Uint8Array([1, 2, 3]) },
    { t: 'ephemeral', bytes: new Uint8Array([9]) },
    { t: 'ping' },
  ];
  for (const f of frames) expect(parseClientFrame(encodeClientFrame(f))).toEqual(f);
});

test('кадры сервера туда и обратно', () => {
  const frames: ServerFrame[] = [
    { t: 'role', role: 'editor', canShare: true, version: 12 },
    { t: 'subscribed', doc: 'root', seq: 2 ** 40, version },
    { t: 'updates', doc, seq: 5, bytes: new Uint8Array([1]) },
    { t: 'snapshot-needed', doc, url: `/api/files/x/docs/${doc}/snapshot` },
    { t: 'ack', doc, clientSeq: 3, seq: 4 },
    { t: 'refused', doc, clientSeq: 3, code: 'FOREIGN_PEER' },
    { t: 'pause', ms: 1500 },
    { t: 'access-revoked', reason: 'session' },
    { t: 'please-reload' },
    { t: 'pong' },
  ];
  for (const f of frames) expect(parseServerFrame(encodeServerFrame(f))).toEqual(f);
});

test('вектор версий: пир u64 без потери, мусор — null', () => {
  const m = parseVersion(version)!;
  expect(m.get('18446744073709551615')).toBe(7);
  expect(encodeVersion(m)).toEqual(version);
  expect(parseVersion(version.subarray(0, version.length - 1))).toBeNull();
  expect(parseVersion(new Uint8Array(3))).toBeNull();
  // Объявил миллиард записей: разбор не идёт дальше заголовка.
  const huge = new Uint8Array(4);
  new DataView(huge.buffer).setUint32(0, 1_000_000_000, true);
  expect(parseVersion(huge)).toBeNull();
  // Счётчик вне i32 в Rust не живёт.
  const bad = encodeVersion(new Map([['1', 5]]));
  new DataView(bad.buffer).setUint32(12, 0x80000000, true);
  expect(parseVersion(bad)).toBeNull();
});

test('versionIncludes', () => {
  const a = new Map([
    ['1', 5],
    ['2', 1],
  ]);
  expect(versionIncludes(a, new Map([['1', 5]]))).toBe(true);
  expect(versionIncludes(a, new Map([['1', 6]]))).toBe(false);
  expect(versionIncludes(a, new Map([['3', 1]]))).toBe(false);
  expect(versionIncludes(a, new Map())).toBe(true);
});

test('разбор не доверяет кадру', () => {
  const update = encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 1, bytes: new Uint8Array([1]) });
  // Обрезка в любом месте заголовка, неизвестный вид, пустой кадр.
  for (let n = 0; n < 9; n++) expect(parseClientFrame(update.subarray(0, n))).toBeNull();
  expect(parseClientFrame(new Uint8Array([200]))).toBeNull();
  expect(parseClientFrame(new Uint8Array(0))).toBeNull();
  // Документ не того вида: путь, длинное имя, не UTF-8.
  for (const bad of ['../../etc', 'x'.repeat(30), '1@', '@1', 'root ']) {
    const f = encodeClientFrame({ t: 'subscribe', doc: bad, version });
    expect(parseClientFrame(f)).toBeNull();
  }
  const notUtf = new Uint8Array([2, 2, 0xff, 0xfe, ...version]);
  expect(parseClientFrame(notUtf)).toBeNull();
  // Хвост у кадров без него.
  expect(parseClientFrame(new Uint8Array([5, 1]))).toBeNull();
  expect(parseClientFrame(new Uint8Array([1, 1, 0, 1, 0, 0, 0, 9]))).toBeNull();
});

test('предел кадра и пачки', () => {
  const big = new Uint8Array(MAX_SYNC_UPDATE_BYTES + 1);
  expect(parseClientFrame(encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 1, bytes: big }))).toBeNull();
  const ok = new Uint8Array(MAX_SYNC_UPDATE_BYTES);
  expect(parseClientFrame(encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 1, bytes: ok }))?.t).toBe('update');
  expect(parseClientFrame(new Uint8Array(MAX_SYNC_FRAME_BYTES + 1))).toBeNull();
  // Пустая пачка — не пачка.
  expect(parseClientFrame(encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 1, bytes: new Uint8Array(0) }))).toBeNull();
});

test('сервер не может прислать неизвестную роль и код', () => {
  expect(parseServerFrame(new Uint8Array([16, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0]))).toBeNull();
  expect(parseServerFrame(new Uint8Array([21, 1, 0x31, 0, 0, 0, 0, 99]))).toBeNull();
  expect(parseServerFrame(new Uint8Array([99]))).toBeNull();
});
