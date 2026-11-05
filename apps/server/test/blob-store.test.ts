import { expect, test } from 'bun:test';
import { mkdtempSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BlobStalled, BlobTooLarge, FsBlobStore } from '../src/blob-store';

const store = () => {
  const dir = mkdtempSync(join(tmpdir(), 'ferrum-blob-'));
  return { dir, s: new FsBlobStore(dir) };
};

test('запись, чтение, замена, удаление', async () => {
  const { s, dir } = store();
  expect(await s.get('files/a')).toBeNull();
  await s.put('files/a', new Uint8Array([1, 2]));
  expect(await s.get('files/a')).toEqual(new Uint8Array([1, 2]));
  await s.put('files/a', new Uint8Array([3]));
  expect(await s.get('files/a')).toEqual(new Uint8Array([3]));
  expect(readdirSync(join(dir, 'files'))).toEqual(['a']);
  await s.delete('files/a');
  expect(await s.get('files/a')).toBeNull();
  await s.delete('files/a');
});

test('ключ не выходит за каталог', async () => {
  const { s } = store();
  for (const key of ['../x', 'a/../../x', '/etc/passwd', '', 'a//b', 'a/', 'a.b']) {
    await expect(s.put(key, new Uint8Array(1))).rejects.toThrow('Неверный ключ');
    await expect(s.get(key)).rejects.toThrow('Неверный ключ');
    await expect(s.delete(key)).rejects.toThrow('Неверный ключ');
  }
});

const streamOf = (...chunks: Uint8Array[]) =>
  new ReadableStream<Uint8Array>({
    start(c) {
      for (const chunk of chunks) c.enqueue(chunk);
      c.close();
    },
  });
const OPTIONS = { maxBytes: 8, idleMs: 1000 };

test('запись потоком: ключ появляется целиком, временных файлов нет', async () => {
  const { s, dir } = store();
  expect(await s.putStream('files/a', streamOf(new Uint8Array([1, 2]), new Uint8Array([3])), OPTIONS)).toEqual({ size: 3 });
  expect(await s.get('files/a')).toEqual(new Uint8Array([1, 2, 3]));
  expect(readdirSync(join(dir, 'files'))).toEqual(['a']);
  const blob = await s.open('files/a');
  expect(blob?.size).toBe(3);
  expect(await s.open('files/none')).toBeNull();
});

test('сверх предела и отказ проверки: ключа нет, `.tmp` убран, старое значение цело', async () => {
  const { s, dir } = store();
  await s.put('files/a', new Uint8Array([9]));
  await expect(s.putStream('files/a', streamOf(new Uint8Array(9)), OPTIONS)).rejects.toBeInstanceOf(BlobTooLarge);
  const bad = {
    ...OPTIONS,
    onChunk: () => {
      throw new Error('плохой кусок');
    },
  };
  await expect(s.putStream('files/a', streamOf(new Uint8Array(1)), bad)).rejects.toThrow('плохой кусок');
  expect(await s.get('files/a')).toEqual(new Uint8Array([9]));
  expect(readdirSync(join(dir, 'files'))).toEqual(['a']);
});

test('клиент молчит дольше срока: запись отменена, `.tmp` убран', async () => {
  const { s, dir } = store();
  let cancelled = false;
  const silent = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(new Uint8Array(1));
    },
    cancel() {
      cancelled = true;
    },
  });
  await expect(s.putStream('files/a', silent, { maxBytes: 8, idleMs: 20 })).rejects.toBeInstanceOf(BlobStalled);
  expect(cancelled).toBe(true);
  expect(readdirSync(join(dir, 'files'))).toEqual([]);
});

test('при запуске временные файлы от сбоя убираются, остальное цело', async () => {
  const { s, dir } = store();
  await s.put('files/a', new Uint8Array([1]));
  writeFileSync(join(dir, 'files', 'a.1.1.tmp'), 'обрыв');
  expect(await s.sweep()).toBe(1);
  expect(readdirSync(join(dir, 'files'))).toEqual(['a']);
});
