import { expect, test } from 'bun:test';
import { mkdtempSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FsBlobStore } from '../src/blob-store';

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
