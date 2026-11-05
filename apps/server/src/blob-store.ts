// Байты снимков, журналов-архивов и картинок (спека, раздел 4). Переход на S3 — новая
// реализация этого интерфейса.

import { mkdir, open, rename, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';

export class BlobTooLarge extends Error {}
export class BlobStalled extends Error {}

export interface PutStreamOptions {
  maxBytes: number;
  /** Дольше этого между кусками — обрыв: зависший клиент не держит запись без конца. */
  idleMs: number;
  /** Каждый кусок до записи: проверка содержимого. Исключение отменяет запись. */
  onChunk?: (chunk: Uint8Array) => void;
}

export interface BlobStore {
  put(key: string, bytes: Uint8Array): Promise<void>;
  /** Запись потоком: в памяти только текущий кусок. Ключ появляется целиком и только при успехе. */
  putStream(key: string, body: ReadableStream<Uint8Array>, options: PutStreamOptions): Promise<{ size: number }>;
  /** Нет такого ключа — `null`. */
  get(key: string): Promise<Uint8Array | null>;
  /** Для ответа потоком: `new Response(blob)` не читает файл в память. Нет ключа — `null`. */
  open(key: string): Promise<Blob | null>;
  /** Нет такого ключа — не ошибка. */
  delete(key: string): Promise<void>;
}

/** Ключ — имена через `/`: путь к файлу не выйдет за каталог хранилища. */
const KEY_RE = /^[A-Za-z0-9_-]+(\/[A-Za-z0-9_-]+)*$/;

const noEntry = (e: unknown) => (e as { code?: unknown } | null)?.code === 'ENOENT';

export class FsBlobStore implements BlobStore {
  private seq = 0;

  constructor(private readonly dir: string) {}

  private path(key: string): string {
    if (!KEY_RE.test(key)) throw new Error(`Неверный ключ хранилища: «${key}»`);
    return join(this.dir, key);
  }

  /** Запись через временный файл с `fsync` файла и каталога: читатель не увидит половину байтов,
   * а сбой питания после подтверждения записи не потеряет её. Сбой записи не оставляет `.tmp`. */
  private async atomically(key: string, write: (file: Awaited<ReturnType<typeof open>>) => Promise<void>): Promise<void> {
    const path = this.path(key);
    const parent = dirname(path);
    await mkdir(parent, { recursive: true });
    const tmp = `${path}.${process.pid}.${++this.seq}.tmp`;
    try {
      const file = await open(tmp, 'w');
      try {
        await write(file);
        await file.sync();
      } finally {
        await file.close();
      }
      await rename(tmp, path);
    } catch (e) {
      await rm(tmp, { force: true });
      throw e;
    }
    const folder = await open(parent, 'r');
    try {
      await folder.sync();
    } finally {
      await folder.close();
    }
  }

  async put(key: string, bytes: Uint8Array): Promise<void> {
    await this.atomically(key, async (file) => {
      await file.writeFile(bytes);
    });
  }

  async putStream(key: string, body: ReadableStream<Uint8Array>, options: PutStreamOptions): Promise<{ size: number }> {
    let size = 0;
    await this.atomically(key, async (file) => {
      const reader = body.getReader();
      try {
        for (;;) {
          let timer: Timer | undefined;
          const stalled = new Promise<never>((_, reject) => {
            timer = setTimeout(() => reject(new BlobStalled()), options.idleMs);
          });
          const next = await Promise.race([reader.read(), stalled]).finally(() => clearTimeout(timer));
          if (next.done) return;
          size += next.value.length;
          if (size > options.maxBytes) throw new BlobTooLarge();
          options.onChunk?.(next.value);
          await file.write(next.value);
        }
      } catch (e) {
        await reader.cancel().catch(() => {});
        throw e;
      }
    });
    return { size };
  }

  async get(key: string): Promise<Uint8Array | null> {
    try {
      return new Uint8Array(await Bun.file(this.path(key)).arrayBuffer());
    } catch (e) {
      if (noEntry(e)) return null;
      throw e;
    }
  }

  async open(key: string): Promise<Blob | null> {
    const file = Bun.file(this.path(key));
    return (await file.exists()) ? file : null;
  }

  async delete(key: string): Promise<void> {
    await rm(this.path(key), { force: true });
  }
}
