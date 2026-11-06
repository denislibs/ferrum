import { expect, test } from 'bun:test';
import {
  checkPartBody,
  decodeSelectionIds,
  encodePartBody,
  encodePresenceBody,
  encodePresenceRecord,
  encodeSelectionIds,
  parsePartBody,
  parsePresenceBody,
  presenceColor,
  PRESENCE_BATCH_MAX,
  PRESENCE_COLORS,
  PRESENCE_FRAME_HOSTILE,
  PRESENCE_FRAME_MAX,
  PRESENCE_PART_BYTES,
  PRESENCE_PARTS_MAX,
  splitSelection,
  withoutGesture,
  type PresenceState,
} from '../src/presence';
import { encodeClientFrame, encodePresenceBatch, parseClientFrame, parseServerFrame, readClientFrame } from '../src/sync';

const page = '12@7910247476534078275';
const full: PresenceState = {
  page,
  view: [-100.5, -50, 1820, 1030],
  cursor: [12.25, -7.5],
  selection: { rev: 7, count: 1000, parts: 2, bounds: [0, 0, 500, 300] },
  gesture: { kind: 'resize', transform: [2, 0, 0, 1.5, -10, 20], frame: [1, 0, 0, 1, 5, 5], k: 1 },
};

test('состояние туда и обратно со всеми сочетаниями полей', () => {
  for (const cursor of [null, full.cursor]) {
    for (const selection of [null, full.selection]) {
      for (const gesture of [null, full.gesture]) {
        const s = { ...full, cursor, selection, gesture };
        const body = encodePresenceBody(s);
        expect(parsePresenceBody(body)).toEqual(s);
        expect(parseClientFrame(encodeClientFrame({ t: 'presence', body }))).toEqual({ t: 'presence', body, state: s });
      }
    }
  }
});

test('кадр с курсором и всеми полями меньше 2 КБ, а обычный — меньше 100 байт', () => {
  expect(encodePresenceBody(full).length).toBeLessThan(300);
  expect(encodePresenceBody({ ...full, selection: null, gesture: null }).length).toBeLessThan(100);
});

test('разбор строгий: нечисловые и огромные числа, лишние флаги, хвост, корень вместо страницы', () => {
  const body = encodePresenceBody({ ...full, selection: null, gesture: null });
  const view = new DataView(body.buffer);
  const pageLen = body[1];
  const at = 2 + pageLen;
  for (const bad of [NaN, Infinity, -Infinity, 1e10]) {
    const b = body.slice();
    new DataView(b.buffer).setFloat64(at + 32, bad, true);
    expect(parsePresenceBody(b)).toBeNull();
  }
  const flags = body.slice();
  flags[0] |= 8;
  expect(parsePresenceBody(flags)).toBeNull();
  expect(parsePresenceBody(new Uint8Array([...body, 0]))).toBeNull();
  expect(parsePresenceBody(body.subarray(0, body.length - 1))).toBeNull();
  // Вид без площади — не вид.
  const flat = body.slice();
  new DataView(flat.buffer).setFloat64(at + 16, view.getFloat64(at, true), true);
  expect(parsePresenceBody(flat)).toBeNull();
  expect(() => encodePresenceBody({ ...full, page: 'root' })).toThrow();
  const root = new Uint8Array([0, 4, ...new TextEncoder().encode('root'), ...body.subarray(at)]);
  expect(parsePresenceBody(root)).toBeNull();
});

test('разбор строгий: выделение и жест', () => {
  const at = (s: PresenceState) => encodePresenceBody(s);
  const sel = at({ ...full, cursor: null, gesture: null });
  const off = 2 + sel[1] + 32;
  const patch = (b: Uint8Array, i: number, f: (v: DataView) => void) => {
    const c = b.slice();
    f(new DataView(c.buffer, i));
    return c;
  };
  expect(parsePresenceBody(patch(sel, off + 4, (v) => v.setUint32(0, 0, true)))).toBeNull();
  expect(parsePresenceBody(patch(sel, off + 4, (v) => v.setUint32(0, 100_001, true)))).toBeNull();
  expect(parsePresenceBody(patch(sel, off + 8, (v) => v.setUint8(0, 0)))).toBeNull();
  expect(parsePresenceBody(patch(sel, off + 8, (v) => v.setUint8(0, PRESENCE_PARTS_MAX + 1)))).toBeNull();
  const g = at({ ...full, cursor: null, selection: null });
  const goff = 2 + g[1] + 32;
  expect(parsePresenceBody(patch(g, goff, (v) => v.setUint8(0, 4)))).toBeNull();
  expect(parsePresenceBody(patch(g, goff + 1, (v) => v.setFloat64(0, NaN, true)))).toBeNull();
  // Коэффициент «Масштаба» — после вида и двух матриц: только больше нуля.
  const scale = at({ ...full, cursor: null, selection: null, gesture: { ...full.gesture!, kind: 'scale', k: 2 } });
  expect(parsePresenceBody(scale)?.gesture?.k).toBe(2);
  for (const k of [0, -0, -1, NaN, Infinity]) {
    expect(parsePresenceBody(patch(scale, goff + 97, (v) => v.setFloat64(0, k, true)))).toBeNull();
    expect(() => encodePresenceBody({ ...full, gesture: { ...full.gesture!, k } })).toThrow();
  }
});

test('кадр присутствия больше 2 КБ или неверный разбирается в `null`-состояние, а не в отказ кадра', () => {
  const big = new Uint8Array(PRESENCE_FRAME_MAX + 10);
  big[0] = 4;
  expect(parseClientFrame(big)).toEqual({ t: 'presence', body: big.subarray(1), state: null });
  const bad = new Uint8Array([6, 1, 2]);
  expect(parseClientFrame(bad)).toEqual({ t: 'presence-part', body: bad.subarray(1), part: null });
});

test('сервер читает кадр присутствия без разбора тела; больше 64 КБ — неверный кадр', () => {
  const body = encodePresenceBody(full);
  const f = encodeClientFrame({ t: 'presence', body });
  expect(readClientFrame(f)).toStrictEqual({ t: 'presence', body });
  const part = encodePartBody({ rev: 1, index: 0, parts: 1, bytes: encodeSelectionIds(['1@2']) });
  expect(readClientFrame(encodeClientFrame({ t: 'presence-part', body: part }))).toStrictEqual({ t: 'presence-part', body: part });
  for (const kind of [4, 6]) {
    const edge = new Uint8Array(PRESENCE_FRAME_HOSTILE);
    edge[0] = kind;
    expect(readClientFrame(edge)).not.toBeNull();
    const over = new Uint8Array(PRESENCE_FRAME_HOSTILE + 1);
    over[0] = kind;
    expect(readClientFrame(over)).toBeNull();
    expect(parseClientFrame(over)).toBeNull();
  }
});

test('жест убирается из тела, остальное — как было', () => {
  for (const cursor of [null, full.cursor]) {
    for (const selection of [null, full.selection]) {
      for (const gesture of [null, full.gesture]) {
        const body = encodePresenceBody({ ...full, cursor, selection, gesture });
        expect(parsePresenceBody(withoutGesture(body))).toEqual({ ...full, cursor, selection, gesture: null });
      }
    }
  }
});

test('проверка части без строк согласна с разбором', () => {
  const peer = (p: bigint) => {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, p, true);
    return [...b];
  };
  const head = [1, 0, 0, 0, 0, 1];
  const cases = [
    encodeSelectionIds(['5@2', '1@2', '1@18446744073709551615', '2147483647@3', '7@2', '1@4294967296', '1@4294967295']),
    new Uint8Array(),
    new Uint8Array([...peer(2n), 1, 0x81, 0x00]),
    new Uint8Array([...peer(2n), 2, 1, 0]),
    new Uint8Array([...peer(3n), 1, 1, ...peer(2n), 1, 1]),
    new Uint8Array([...peer(2n), 1, 1, ...peer(2n), 1, 2]),
    // Старшее слово пира меньше при большем младшем: пиры не по возрастанию.
    new Uint8Array([...peer(2n ** 32n), 1, 1, ...peer(0xffffffffn), 1, 1]),
    new Uint8Array([...peer(0xffffffffn), 1, 1, ...peer(2n ** 32n), 1, 1]),
    new Uint8Array([...peer(2n), 1, 0xff, 0xff, 0xff, 0xff, 0x0f]),
    new Uint8Array([...peer(2n), 0xff, 0xff, 0x03, 1]),
    new Uint8Array([...peer(2n), 1]),
  ];
  for (const ids of cases) {
    const body = new Uint8Array([...head, ...ids]);
    const parsed = parsePartBody(body);
    const checked = checkPartBody(body);
    expect(checked === null).toBe(parsed === null);
    if (parsed) expect(checked).toEqual({ rev: parsed.rev, index: parsed.index, parts: parsed.parts });
  }
  expect(decodeSelectionIds(cases[0])).toEqual([
    '1@2',
    '5@2',
    '7@2',
    '2147483647@3',
    '1@4294967295',
    '1@4294967296',
    '1@18446744073709551615',
  ]);
  expect(decodeSelectionIds(cases[6])).toBeNull();
  expect(decodeSelectionIds(cases[7])).toEqual(['1@4294967295', '1@4294967296']);
});

test('предел 2 КБ: верная часть выделения на границе принимается, на байт длиннее — нет', () => {
  // Узлы одного пира: 8 байт пира, число узлов (2 байта), первый счётчик и разности по байту — всего n + 10 байт.
  const nodes = (n: number) => {
    const b = new Uint8Array(n + 10);
    new DataView(b.buffer).setBigUint64(0, 2n, true);
    b[8] = (n & 0x7f) | 0x80;
    b[9] = n >> 7;
    b.fill(1, 10);
    return b;
  };
  const frame = (bytes: Uint8Array) => {
    const f = new Uint8Array(1 + 6 + bytes.length);
    f[0] = 6;
    new DataView(f.buffer).setUint32(1, 5, true);
    f[5] = 0;
    f[6] = 1;
    f.set(bytes, 7);
    return f;
  };
  const edge = frame(nodes(PRESENCE_PART_BYTES - 10));
  expect(edge.length).toBe(PRESENCE_FRAME_MAX - 1);
  expect(parseClientFrame(edge)).toMatchObject({ t: 'presence-part', part: { rev: 5, index: 0, parts: 1 } });
  expect(parseClientFrame(edge)).not.toMatchObject({ part: null });
  for (const extra of [1, 2]) {
    const over = frame(nodes(PRESENCE_PART_BYTES - 10 + extra));
    expect(over.length).toBe(PRESENCE_FRAME_MAX - 1 + extra);
    // Без предела кадр был бы верным: узлы разбираются.
    expect(decodeSelectionIds(nodes(PRESENCE_PART_BYTES - 10 + extra))).not.toBeNull();
    expect(parseClientFrame(over)).toEqual({ t: 'presence-part', body: over.subarray(1), part: null });
    expect(checkPartBody(over.subarray(1))).toBeNull();
  }
  expect(checkPartBody(edge.subarray(1))).toEqual({ rev: 5, index: 0, parts: 1 });
});

test('узлы выделения: запись одна, туда и обратно', () => {
  const ids = ['5@2', '1@2', '1@18446744073709551615', '2147483647@3', '7@2'];
  const bytes = encodeSelectionIds(ids);
  expect(decodeSelectionIds(bytes)).toEqual(['1@2', '5@2', '7@2', '2147483647@3', '1@18446744073709551615']);
  // Повтор узла не удваивается.
  expect(decodeSelectionIds(encodeSelectionIds(['1@2', '1@2']))).toEqual(['1@2']);
});

test('узлы выделения: неверная запись отвергается', () => {
  const peer = (p: bigint) => {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, p, true);
    return [...b];
  };
  expect(decodeSelectionIds(new Uint8Array())).toBeNull();
  // Лишний байт в числе (0x80 0x00).
  expect(decodeSelectionIds(new Uint8Array([...peer(2n), 1, 0x81, 0x00]))).toBeNull();
  // Разность 0: узел дважды.
  expect(decodeSelectionIds(new Uint8Array([...peer(2n), 2, 1, 0]))).toBeNull();
  // Пиры не по возрастанию.
  expect(decodeSelectionIds(new Uint8Array([...peer(3n), 1, 1, ...peer(2n), 1, 1]))).toBeNull();
  // Счётчик за пределом TreeID.
  expect(decodeSelectionIds(new Uint8Array([...peer(2n), 1, 0xff, 0xff, 0xff, 0xff, 0x0f]))).toBeNull();
  // Заголовок обещает больше узлов, чем байтов: разбор не идёт дальше пришедшего.
  expect(decodeSelectionIds(new Uint8Array([...peer(2n), 0xff, 0xff, 0x03, 1]))).toBeNull();
  // Хвост без числа.
  expect(decodeSelectionIds(new Uint8Array([...peer(2n), 1]))).toBeNull();
});

test('1000 перетаскиваемых узлов одного автора — одна часть кадра', () => {
  const ids = Array.from({ length: 1000 }, (_, i) => `${100 + i * 15}@7910247476534078275`);
  const parts = splitSelection(ids);
  expect(parts.length).toBe(1);
  expect(parts[0].length).toBeLessThan(1100);
  const body = encodePartBody({ rev: 3, index: 0, parts: 1, bytes: parts[0] });
  expect(body.length + 1).toBeLessThanOrEqual(PRESENCE_FRAME_MAX);
  expect(parsePartBody(body)?.ids.length).toBe(1000);
});

test('большое выделение многих авторов: части не больше предела и не больше 16', () => {
  const rnd = (() => {
    let x = 1;
    return () => (x = (x * 48271) % 0x7fffffff);
  })();
  const ids = Array.from({ length: 40_000 }, () => `${rnd() % 2_000_000_000}@${rnd() % 20}`);
  const parts = splitSelection(ids);
  expect(parts.length).toBe(PRESENCE_PARTS_MAX);
  for (const p of parts) expect(p.length).toBeLessThanOrEqual(PRESENCE_PART_BYTES);
  const got = parts.flatMap((p) => decodeSelectionIds(p)!);
  expect(new Set(got).size).toBe(got.length);
  expect(got.every((id) => ids.includes(id))).toBe(true);
});

test('часть выделения: номер не больше числа частей, байты в пределе', () => {
  const bytes = encodeSelectionIds(['1@2']);
  expect(() => encodePartBody({ rev: 1, index: 1, parts: 1, bytes })).toThrow();
  expect(() => encodePartBody({ rev: 1, index: 0, parts: 17, bytes })).toThrow();
  const body = encodePartBody({ rev: 1, index: 0, parts: 1, bytes });
  const bad = body.slice();
  bad[4] = 1;
  expect(parsePartBody(bad)).toBeNull();
});

test('цвет участника — из userId, один и тот же, в пределах палитры', () => {
  expect(presenceColor('u-1')).toBe(presenceColor('u-1'));
  const seen = new Set<number>();
  for (let i = 0; i < 500; i++) {
    const c = presenceColor(`user-${i}`);
    expect(c).toBeGreaterThanOrEqual(0);
    expect(c).toBeLessThan(PRESENCE_COLORS.length);
    seen.add(c);
  }
  expect(seen.size).toBe(PRESENCE_COLORS.length);
});

test('пачка сервера: записи туда и обратно, неверная запись — весь кадр неверен', () => {
  const body = encodePresenceBody(full);
  const part = encodePartBody({ rev: 7, index: 1, parts: 2, bytes: encodeSelectionIds(['3@4']) });
  const frame = encodePresenceBatch([
    encodePresenceRecord({ k: 'join', participant: 9, user: 'u-1', name: 'Анна', color: 3 }),
    encodePresenceRecord({ k: 'state', participant: 9, body }),
    encodePresenceRecord({ k: 'part', participant: 9, body: part }),
    encodePresenceRecord({ k: 'left', participant: 4 }),
  ]);
  expect(parseServerFrame(frame)).toEqual({
    t: 'presence',
    items: [
      { k: 'join', participant: 9, user: 'u-1', name: 'Анна', color: 3 },
      { k: 'state', participant: 9, state: full },
      { k: 'part', participant: 9, part: { rev: 7, index: 1, parts: 2, ids: ['3@4'] } },
      { k: 'left', participant: 4 },
    ],
  });
  expect(parseServerFrame(new Uint8Array([...frame, 0]))).toBeNull();
  expect(parseServerFrame(encodePresenceBatch([]))).toBeNull();
  const join = encodePresenceRecord({ k: 'join', participant: 1, user: 'u', name: 'x', color: 0 });
  join[join.length - 1] = PRESENCE_COLORS.length;
  expect(parseServerFrame(encodePresenceBatch([join]))).toBeNull();
  const unknown = encodePresenceRecord({ k: 'left', participant: 1 });
  unknown[0] = 9;
  expect(parseServerFrame(encodePresenceBatch([unknown]))).toBeNull();
  const badState = encodePresenceRecord({ k: 'state', participant: 1, body: new Uint8Array([0, 1, 49]) });
  expect(parseServerFrame(encodePresenceBatch([badState]))).toBeNull();
  expect(() => encodePresenceBatch([new Uint8Array(PRESENCE_BATCH_MAX + 1)])).toThrow();
  expect(() => encodePresenceRecord({ k: 'join', participant: 1, user: 'u', name: 'я'.repeat(200), color: 0 })).toThrow();
});
