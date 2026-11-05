import { expect, test } from 'bun:test';
import { chmodSync, mkdtempSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { open } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
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

test('уборка: каталога нет — не сбой, настоящий сбой чтения не проглатывается', async () => {
  const { dir } = store();
  expect(await new FsBlobStore(join(dir, 'нет')).sweep()).toBe(0);
  writeFileSync(join(dir, 'файл'), '');
  await expect(new FsBlobStore(join(dir, 'файл')).sweep()).rejects.toMatchObject({ code: 'ENOTDIR' });
  if (process.getuid?.() === 0) return;
  chmodSync(dir, 0);
  try {
    await expect(new FsBlobStore(dir).sweep()).rejects.toMatchObject({ code: 'EACCES' });
  } finally {
    chmodSync(dir, 0o755);
  }
});

type Handle = Awaited<ReturnType<typeof open>>;
/** Настоящий файл с подменёнными методами. */
function patched(file: Handle, over: Partial<Record<keyof Handle, unknown>>): Handle {
  return new Proxy(file, {
    get(f, k) {
      if (k in over) return over[k as keyof Handle];
      const v: unknown = Reflect.get(f, k, f);
      return typeof v === 'function' ? v.bind(f) : v;
    },
  });
}
/** Настоящий файл, но `write` за раз пишет не больше `limit` байт, как на почти полном диске. */
const shortWrites =
  (limit: number): typeof open =>
  async (path, flags) => {
    const file = await open(path, flags);
    const write = (bytes: Uint8Array, at = 0, length = bytes.length - at) => file.write(bytes, at, Math.min(length, limit));
    return patched(file, { write });
  };

test('короткая запись: файл полный, куски дописываются до конца', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'ferrum-blob-'));
  const s = new FsBlobStore(dir, shortWrites(2));
  const bytes = new Uint8Array(101).map((_, i) => i);
  const { size } = await s.putStream('files/a', streamOf(bytes.subarray(0, 50), bytes.subarray(50)), { maxBytes: 200, idleMs: 1000 });
  expect(size).toBe(101);
  expect(statSync(join(dir, 'files/a')).size).toBe(101);
  expect(await s.get('files/a')).toEqual(bytes);
});

test('запись не продвигается или размер не сошёлся: ключа нет, старое цело', async () => {
  const stuck = mkdtempSync(join(tmpdir(), 'ferrum-blob-'));
  const zero = new FsBlobStore(stuck, shortWrites(0));
  await new FsBlobStore(stuck).put('files/a', new Uint8Array([9]));
  await expect(zero.putStream('files/a', streamOf(new Uint8Array(3)), OPTIONS)).rejects.toThrow('не продвигается');
  expect(await zero.get('files/a')).toEqual(new Uint8Array([9]));

  // `write` докладывает обо всём, а записал меньше: это ловит только сверка с диском.
  const lying: typeof open = async (path, flags) => {
    const file = await open(path, flags);
    const write = async (bytes: Uint8Array, at = 0, length = bytes.length - at) => {
      await file.write(bytes, at, Math.max(length - 1, 0));
      return { bytesWritten: length, buffer: bytes };
    };
    return patched(file, { write });
  };
  const liar = new FsBlobStore(stuck, lying);
  await expect(liar.putStream('files/a', streamOf(new Uint8Array(3)), OPTIONS)).rejects.toThrow('Записано 2 байт из 3');
  expect(await liar.get('files/a')).toEqual(new Uint8Array([9]));
  expect(readdirSync(join(stuck, 'files'))).toEqual(['a']);
});

test('новая папка: каталоги синхронизируются до корня хранилища, и потоком, и обычной записью', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'ferrum-blob-'));
  const synced: string[] = [];
  const watching: typeof open = async (path, flags) => {
    const file = await open(path, flags);
    const sync = async () => {
      if (flags === 'r') synced.push(relative(dir, String(path)));
      await file.sync();
    };
    return patched(file, { sync });
  };
  const s = new FsBlobStore(dir, watching);
  await s.putStream('files/id/content', streamOf(new Uint8Array(1)), OPTIONS);
  expect(synced).toEqual(['files/id', 'files', '']);
  synced.length = 0;
  await s.put('images/hash/key', new Uint8Array(1));
  expect(synced).toEqual(['images/hash', 'images', '']);
});

test('наименьшая скорость: срок растёт с байтами, отставший обрывается, успевающий нет', async () => {
  const { s } = store();
  const pace = { graceMs: 1000, bytesPerSecond: 100 };
  let time = 0;
  // Каждый кусок приходит на поддельных часах: 100 байт за `step` мс.
  const dripping = (step: number, chunks: number) =>
    new ReadableStream<Uint8Array>({
      pull(c) {
        if (chunks-- === 0) {
          c.close();
          return;
        }
        time += step;
        c.enqueue(new Uint8Array(100));
      },
    });
  // Ровно по пределу: каждые 100 байт дают секунду, а первая секунда — про запас.
  await expect(s.putStream('files/a', dripping(990, 20), { maxBytes: 1e6, idleMs: 1e6, pace, now: () => time })).resolves.toEqual({
    size: 2000,
  });
  time = 0;
  // Вдвое медленнее: срок кончается посреди загрузки, простой тут ни при чём.
  await expect(s.putStream('files/b', dripping(2000, 20), { maxBytes: 1e6, idleMs: 1e6, pace, now: () => time })).rejects.toBeInstanceOf(
    BlobStalled,
  );
  expect(await s.get('files/b')).toBeNull();
});

test('наименьшая скорость: капля за первым куском не держит запись до простоя', async () => {
  const { s } = store();
  const silent = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(new Uint8Array(1));
    },
  });
  await expect(
    s.putStream('files/a', silent, { maxBytes: 8, idleMs: 30_000, pace: { graceMs: 20, bytesPerSecond: 1000 } }),
  ).rejects.toBeInstanceOf(BlobStalled);
});
