import { expect, test } from 'bun:test';
import {
  ContainerError,
  ContainerWalker,
  MAX_IMAGE_REFS,
  TooManyImages,
  containerHeader,
  imageSectionHeader,
  isFileMeta,
  parseInvite,
  parseListQuery,
  parseRedeem,
  parseSection,
  parseWriteHeaders,
  scanContainer,
  SECTION_IMAGE,
  SECTION_PAGE_REFS,
} from '../src';

const HASH_A = 'a'.repeat(64);
const HASH_B = 'b'.repeat(64);

function section(kind: number, id: string, data: Uint8Array): Uint8Array {
  const idBytes = new TextEncoder().encode(id);
  const out = new Uint8Array(5 + idBytes.length + 40 + data.length);
  const v = new DataView(out.buffer);
  out[0] = kind;
  v.setUint32(1, idBytes.length, true);
  out.set(idBytes, 5);
  v.setBigUint64(5 + idBytes.length, BigInt(data.length), true);
  out.set(data, 5 + idBytes.length + 40);
  return out;
}

function container(...sections: Uint8Array[]): Uint8Array {
  const head = containerHeader(sections.length);
  const out = new Uint8Array(head.length + sections.reduce((n, s) => n + s.length, 0));
  out.set(head);
  let at = head.length;
  for (const s of sections) {
    out.set(s, at);
    at += s.length;
  }
  return out;
}

const refs = (...hashes: string[]) => section(SECTION_PAGE_REFS, '1@1', new TextEncoder().encode(hashes.join('\n')));
const walk = (bytes: Uint8Array, chunk = bytes.length) => {
  const w = new ContainerWalker();
  for (let i = 0; i < bytes.length; i += chunk) w.feed(bytes.subarray(i, i + chunk));
  w.finish();
  return w;
};

test('проход кусками любого размера находит ссылки на картинки', () => {
  const file = container(section(1, '', new Uint8Array(100)), section(2, '1@1', new Uint8Array(70_000)), refs(HASH_A, HASH_B));
  for (const chunk of [1, 7, 64, 65_536, file.length]) {
    expect([...walk(file, chunk).imageRefs].sort()).toEqual([HASH_A, HASH_B]);
  }
});

test('разных картинок больше предела: отказ, повторы и мусор не считаются', () => {
  const hash = (i: number) => i.toString(16).padStart(64, '0');
  const hashes = (from: number, to: number) => Array.from({ length: to - from }, (_, i) => hash(from + i));
  const parts = (list: string[]) => {
    const out: Uint8Array[] = [];
    for (let i = 0; i < list.length; i += 8000) out.push(refs(...list.slice(i, i + 8000)));
    return out;
  };
  const full = hashes(0, MAX_IMAGE_REFS);
  const atLimit = container(...parts(full));
  expect(walk(atLimit).imageRefs.size).toBe(MAX_IMAGE_REFS);
  expect(walk(container(...parts(full), refs(hash(0), 'мусор', hash(1)))).imageRefs.size).toBe(MAX_IMAGE_REFS);
  expect(() => walk(container(...parts(hashes(0, MAX_IMAGE_REFS + 1))))).toThrow(TooManyImages);
  expect(new TooManyImages('x')).toBeInstanceOf(ContainerError);
});

test('неверный контейнер: чужие байты, версия, картинка внутри, обрыв, хвост', () => {
  const good = container(section(1, '', new Uint8Array(4)));
  expect(() => walk(new Uint8Array(32))).toThrow(ContainerError);
  const version = good.slice();
  version[8] = 9;
  expect(() => walk(version)).toThrow('версия');
  expect(() => walk(container(section(SECTION_IMAGE, HASH_A, new Uint8Array(1))))).toThrow('картинка');
  expect(() => walk(good.subarray(0, good.length - 1))).toThrow('обрезан');
  const tail = new Uint8Array(good.length + 1);
  tail.set(good);
  expect(() => walk(tail)).toThrow('лишние');
  expect(() => walk(new Uint8Array(3))).toThrow('обрезан');
});

test('сканирование кусками файла и сборка картинки обратно', async () => {
  const data = new Uint8Array([1, 2, 3]);
  const image = new Uint8Array(imageSectionHeader(HASH_A, data.length).length + data.length);
  image.set(imageSectionHeader(HASH_A, data.length));
  image.set(data, image.length - data.length);
  const file = container(section(1, '', new Uint8Array(4)), image);
  const reads: [number, number][] = [];
  const spans = await scanContainer(file.length, async (s, e) => {
    reads.push([s, e]);
    return file.subarray(s, e);
  });
  expect(spans.map((s) => [s.kind, s.id, s.size])).toEqual([
    [1, '', 4],
    [SECTION_IMAGE, HASH_A, 3],
  ]);
  expect(file.subarray(spans[1].data, spans[1].data + spans[1].size)).toEqual(data);
  // Данные секций не читаются.
  expect(Math.max(...reads.map(([s, e]) => e - s))).toBeLessThan(200);
  await expect(scanContainer(5, async () => new Uint8Array(5))).rejects.toThrow(ContainerError);
  await expect(scanContainer(file.length - 1, async (s, e) => file.subarray(s, e))).rejects.toThrow('обрезан');
});

test('разбор запросов', () => {
  expect(parseSection('project:00000000-0000-4000-8000-000000000000')).toBe('project:00000000-0000-4000-8000-000000000000');
  expect(parseSection('project:x')).toBeNull();
  expect(parseSection('other')).toBeNull();
  const q = (s: string) => parseListQuery(new URLSearchParams(s));
  expect(q('section=recent&start=0&count=200&q=%20план%20')).toEqual({ section: 'recent', query: 'план', start: 0, count: 200 });
  expect(q('section=recent&start=0&count=201')).toBeNull();
  expect(q('section=recent&start=-1&count=1')).toBeNull();
  expect(parseInvite({ email: 'A@X.ru', role: 'editor' })).toEqual({ email: 'a@x.ru', role: 'editor' });
  expect(parseInvite({ email: 'a@x.ru', role: 'owner' })).toBeNull();
  expect(parseRedeem({ token: 'x'.repeat(43) })).toEqual({ token: 'x'.repeat(43) });
  expect(parseRedeem({ token: '../' })).toBeNull();
  const headers = (h: Record<string, string>) => ({ get: (n: string) => h[n] ?? null });
  const id = '00000000-0000-4000-8000-000000000000';
  expect(parseWriteHeaders(headers({ 'if-match': '"3"', 'x-ferrum-write': id }))).toEqual({ base: 3, write: id, previous: null, lost: 0 });
  expect(parseWriteHeaders(headers({ 'if-match': '"3"', 'x-ferrum-write': id, 'x-ferrum-lost-images': '2' }))?.lost).toBe(2);
  expect(parseWriteHeaders(headers({ 'if-match': '"3"', 'x-ferrum-write': id, 'x-ferrum-lost-images': 'много' }))).toBeNull();
  expect(parseWriteHeaders(headers({ 'if-match': '3', 'x-ferrum-write': id }))).toBeNull();
  expect(parseWriteHeaders(headers({ 'if-match': '"3"' }))).toBeNull();
});

test('ответ не того вида — не `FileMetaDto`', () => {
  const ok = { id: 'a', name: 'b', createdAt: 1, editedAt: 1, openedAt: 0, size: 0, thumbVersion: 0, version: 0, role: 'viewer' };
  expect(isFileMeta(ok)).toBe(true);
  expect(isFileMeta({ ...ok, role: 'admin' })).toBe(false);
  expect(isFileMeta({ ...ok, size: '1' })).toBe(false);
  expect(isFileMeta(null)).toBe(false);
});
