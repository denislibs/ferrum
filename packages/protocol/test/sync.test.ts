import { expect, test } from 'bun:test';
import {
  encodeClientFrame,
  encodeServerFrame,
  encodeVersion,
  MAX_SYNC_FRAME_BYTES,
  MAX_SYNC_UPDATE_BYTES,
  MAX_VERSION_PEERS,
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
    { t: 'comment', event: { kind: 'gap', seq: 12 } },
    { t: 'comment', event: { kind: 'removed', seq: 13, by: 'u1', thread: '0b5c1d3e-7f40-4a8b-9c2d-1e2f3a4b5c6d' } },
  ];
  for (const f of frames) expect(parseServerFrame(encodeServerFrame(f))).toEqual(f);
});

test('кадр комментария: JSON строго, иначе — как обрыв', () => {
  const frame = (json: string) => new Uint8Array([27, ...new TextEncoder().encode(json)]);
  expect(parseServerFrame(frame('{"kind":"gap","seq":1}'))).toEqual({ t: 'comment', event: { kind: 'gap', seq: 1 } });
  for (const bad of ['', '{', '{"kind":"gap","seq":1,"x":1}', '{"kind":"gap","seq":"1"}', 'null', '[1]'])
    expect(parseServerFrame(frame(bad)), bad).toBeNull();
  // Кодирование не пишет того, что не прочтётся: событие больше предела — ошибка отправителя.
  const huge = { kind: 'removed' as const, seq: 1, by: 'x'.repeat(70_000), thread: '0b5c1d3e-7f40-4a8b-9c2d-1e2f3a4b5c6d' };
  expect(() => encodeServerFrame({ t: 'comment', event: huge })).toThrow(RangeError);
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
    const d = new TextEncoder().encode(bad);
    expect(parseClientFrame(new Uint8Array([2, d.length, ...d, ...version]))).toBeNull();
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
  expect(parseServerFrame(new Uint8Array([16, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]))).toEqual({
    t: 'role',
    role: 'editor',
    canShare: false,
    version: 0,
  });
  // Документ `root` верный: отвергает именно код.
  const refused = (code: number) => new Uint8Array([21, 4, 0x72, 0x6f, 0x6f, 0x74, 0, 0, 0, 0, code]);
  expect(parseServerFrame(refused(0))).toEqual({ t: 'refused', doc: 'root', clientSeq: 0, code: 'BAD_UPDATE' });
  expect(parseServerFrame(refused(6))).toEqual({ t: 'refused', doc: 'root', clientSeq: 0, code: 'DOC_LIMIT' });
  expect(parseServerFrame(refused(99))).toBeNull();
  expect(parseServerFrame(new Uint8Array([99]))).toBeNull();
});

test('документ — только в записи Loro: без ведущих нулей и в диапазоне счётчика i32 и пира u64', () => {
  const ok = ['root', '0@0', '2147483647@18446744073709551615', '7@1'];
  const bad = ['007@1', '1@0001', '00@1', '2147483648@1', '1@18446744073709551616', '9999999999@99999999999999999999', '-1@1'];
  for (const d of ok)
    expect(parseClientFrame(encodeClientFrame({ t: 'subscribe', doc: d, version }))).toEqual({ t: 'subscribe', doc: d, version });
  for (const d of bad) {
    const b = new TextEncoder().encode(d);
    expect(parseClientFrame(new Uint8Array([2, b.length, ...b, ...version]))).toBeNull();
    expect(parseServerFrame(new Uint8Array([20, b.length, ...b, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]))).toBeNull();
    // Кодирование не пишет того, что не прочтётся.
    expect(() => encodeClientFrame({ t: 'subscribe', doc: d, version })).toThrow(RangeError);
  }
});

test('BOM не срезается: разбор и кодирование сходятся', () => {
  const bom = new Uint8Array([2, 7, 0xef, 0xbb, 0xbf, 0x72, 0x6f, 0x6f, 0x74, ...version]);
  expect(parseClientFrame(bom)).toBeNull();
  const url = new TextEncoder().encode('\ufeff/api/files/x/docs/root/snapshot');
  expect(parseServerFrame(new Uint8Array([19, 4, 0x72, 0x6f, 0x6f, 0x74, url.length, 0, ...url]))).toBeNull();
});

test('роль: canShare строго 0 или 1', () => {
  const role = (canShare: number) => new Uint8Array([16, 1, canShare, 5, 0, 0, 0, 0, 0, 0, 0]);
  expect(parseServerFrame(role(1))).toMatchObject({ canShare: true });
  expect(parseServerFrame(role(0))).toMatchObject({ canShare: false });
  expect(parseServerFrame(role(2))).toBeNull();
  expect(parseServerFrame(role(255))).toBeNull();
});

test('вектор версий: одна запись — пиры по возрастанию без повторов, счётчик больше нуля', () => {
  const raw = (entries: [bigint, number][]) => {
    const b = new Uint8Array(4 + entries.length * 12);
    const v = new DataView(b.buffer);
    v.setUint32(0, entries.length, true);
    entries.forEach(([p, c], i) => {
      v.setBigUint64(4 + i * 12, p, true);
      v.setUint32(12 + i * 12, c, true);
    });
    return b;
  };
  expect(
    parseVersion(
      raw([
        [0n, 3],
        [1n, 9],
      ]),
    ),
  ).toEqual(
    new Map([
      ['0', 3],
      ['1', 9],
    ]),
  );
  expect(
    parseVersion(
      raw([
        [0n, 3],
        [0n, 9],
      ]),
    ),
  ).toBeNull();
  expect(
    parseVersion(
      raw([
        [2n, 3],
        [1n, 9],
      ]),
    ),
  ).toBeNull();
  expect(parseVersion(raw([[1n, 0]]))).toBeNull();
  // Ни подписка, ни ответ на неё не несут неканонический вектор.
  expect(
    parseClientFrame(
      new Uint8Array([
        2,
        4,
        0x72,
        0x6f,
        0x6f,
        0x74,
        ...raw([
          [2n, 3],
          [1n, 9],
        ]),
      ]),
    ),
  ).toBeNull();
  expect(parseServerFrame(new Uint8Array([17, 4, 0x72, 0x6f, 0x6f, 0x74, 0, 0, 0, 0, 0, 0, 0, 0, ...raw([[1n, 0]])]))).toBeNull();
  // Кодирование: пустые записи не пишутся, неверный пир и счётчик — ошибка.
  expect(
    encodeVersion(
      new Map([
        ['1', 0],
        ['2', 4],
      ]),
    ),
  ).toEqual(raw([[2n, 4]]));
  expect(() => encodeVersion(new Map([['x', 1]]))).toThrow(RangeError);
  expect(() => encodeVersion(new Map([['18446744073709551616', 1]]))).toThrow(RangeError);
  expect(() => encodeVersion(new Map([['1', 2 ** 31]]))).toThrow(RangeError);
  // Предел числа пиров — на границе.
  const at = (n: number) => {
    const b = new Uint8Array(4 + n * 12);
    const v = new DataView(b.buffer);
    v.setUint32(0, n, true);
    for (let i = 0; i < n; i++) {
      v.setBigUint64(4 + i * 12, BigInt(i), true);
      v.setUint32(12 + i * 12, 1, true);
    }
    return b;
  };
  expect(parseVersion(at(MAX_VERSION_PEERS))?.size).toBe(MAX_VERSION_PEERS);
  expect(parseVersion(at(MAX_VERSION_PEERS + 1))).toBeNull();
});

test('кадры сервера: предел кадра, пустая пачка, хвост', () => {
  const big = new Uint8Array(MAX_SYNC_FRAME_BYTES + 1);
  big[0] = 18;
  expect(parseServerFrame(big)).toBeNull();
  expect(parseServerFrame(encodeServerFrame({ t: 'updates', doc: 'root', seq: 1, bytes: new Uint8Array(0) }))).toBeNull();
  for (const f of [
    { t: 'ack', doc: 'root', clientSeq: 1, seq: 2 },
    { t: 'pause', ms: 5 },
    { t: 'access-revoked', reason: 'access' },
    { t: 'snapshot-needed', doc: 'root', url: '/api/files/f/docs/root/snapshot' },
    { t: 'please-reload' },
  ] satisfies ServerFrame[]) {
    const b = encodeServerFrame(f);
    expect(parseServerFrame(b)).toEqual(f);
    expect(parseServerFrame(new Uint8Array([...b, 0]))).toBeNull();
  }
});

test('snapshot-needed: адрес — только снимок того же документа на своём сервере', () => {
  const frame = (doc: string, url: string) => {
    const d = new TextEncoder().encode(doc);
    const u = new TextEncoder().encode(url);
    return new Uint8Array([19, d.length, ...d, u.length & 0xff, u.length >> 8, ...u]);
  };
  expect(parseServerFrame(frame('root', '/api/files/f-1/docs/root/snapshot'))).toEqual({
    t: 'snapshot-needed',
    doc: 'root',
    url: '/api/files/f-1/docs/root/snapshot',
  });
  for (const url of [
    'https://evil.example/steal',
    '//evil.example/api/files/f/docs/root/snapshot',
    '/api/files/f/docs/1@1/snapshot',
    '/api/files/../docs/root/snapshot',
    '/api/files/f/docs/root/snapshot?x=1',
    '/other/f/docs/root/snapshot',
  ])
    expect(parseServerFrame(frame('root', url))).toBeNull();
});

test('кодирование не пишет молча чужое число', () => {
  expect(() => encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 2 ** 32 + 5, bytes: new Uint8Array([1]) })).toThrow(RangeError);
  expect(() => encodeServerFrame({ t: 'ack', doc: 'root', clientSeq: 1, seq: -1 })).toThrow(RangeError);
  expect(() => encodeServerFrame({ t: 'ack', doc: 'root', clientSeq: 1, seq: 1.5 })).toThrow(RangeError);
  expect(() => encodeServerFrame({ t: 'role', role: 'boss' as never, canShare: true, version: 1 })).toThrow(RangeError);
  expect(() => encodeServerFrame({ t: 'refused', doc: 'root', clientSeq: 1, code: 'NOPE' as never })).toThrow(RangeError);
  expect(() => encodeClientFrame({ t: 'hello', protocol: 70_000, schema: 1 })).toThrow(RangeError);
});
