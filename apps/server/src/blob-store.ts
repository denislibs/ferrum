// Байты снимков, журналов-архивов и картинок (спека, раздел 4). Переход на S3 — новая
// реализация этого интерфейса.

import { mkdir, rename, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';

export interface BlobStore {
  put(key: string, bytes: Uint8Array): Promise<void>;
  /** Нет такого ключа — `null`. */
  get(key: string): Promise<Uint8Array | null>;
  /** Нет такого ключа — не ошибка. */
  delete(key: string): Promise<void>;
}

/** Ключ — имена через `/`: путь к файлу не выйдет за каталог хранилища. */
const KEY_RE = /^[A-Za-z0-9_-]+(\/[A-Za-z0-9_-]+)*$/;

export class FsBlobStore implements BlobStore {
  private seq = 0;

  constructor(private readonly dir: string) {}

  private path(key: string): string {
    if (!KEY_RE.test(key)) throw new Error(`Неверный ключ хранилища: «${key}»`);
    return join(this.dir, key);
  }

  /** Запись через временный файл: читатель не увидит половину байтов. */
  async put(key: string, bytes: Uint8Array): Promise<void> {
    const path = this.path(key);
    await mkdir(dirname(path), { recursive: true });
    const tmp = `${path}.${process.pid}.${++this.seq}.tmp`;
    await Bun.write(tmp, bytes);
    await rename(tmp, path);
  }

  async get(key: string): Promise<Uint8Array | null> {
    const file = Bun.file(this.path(key));
    return (await file.exists()) ? new Uint8Array(await file.arrayBuffer()) : null;
  }

  async delete(key: string): Promise<void> {
    await rm(this.path(key), { force: true });
  }
}
