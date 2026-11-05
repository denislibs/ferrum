// Байты снимков, журналов-архивов и картинок (спека, раздел 4). Переход на S3 — новая
// реализация этого интерфейса.

import { mkdir, open, readdir, rename, rm } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';

export class BlobTooLarge extends Error {}
export class BlobStalled extends Error {}

/** Наименьшая скорость загрузки: каждый принятый байт продлевает срок. Обрыв связи на минуту переживает и
 * медленный клиент, а капля байтов не держит запись часами. */
export interface UploadPace {
  graceMs: number;
  bytesPerSecond: number;
}

export interface PutStreamOptions {
  maxBytes: number;
  /** Дольше этого между кусками — обрыв: зависший клиент не держит запись без конца. */
  idleMs: number;
  /** Отстал от наименьшей скорости — тот же обрыв. */
  pace?: UploadPace;
  /** Тесты: часы для `pace`. */
  now?: () => number;
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
  private readonly dir: string;

  /** `openFile` подменяют тесты: короткая запись и `fsync` каталогов иначе не наблюдаемы. */
  constructor(
    dir: string,
    private readonly openFile: typeof open = open,
  ) {
    this.dir = resolve(dir);
  }

  private path(key: string): string {
    if (!KEY_RE.test(key)) throw new Error(`Неверный ключ хранилища: «${key}»`);
    return join(this.dir, key);
  }

  /** Запись через временный файл с `fsync` файла и каталога: читатель не увидит половину байтов,
   * а сбой питания после подтверждения записи не потеряет её: синхронизируются все каталоги до корня
   * хранилища, запись о новой папке лежит в её родителе. Сбой записи не оставляет `.tmp`. */
  private async atomically(key: string, write: (file: Awaited<ReturnType<typeof open>>) => Promise<void>): Promise<void> {
    const path = this.path(key);
    const parent = dirname(path);
    await mkdir(parent, { recursive: true });
    const tmp = `${path}.${process.pid}.${++this.seq}.tmp`;
    try {
      const file = await this.openFile(tmp, 'w');
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
    for (let folder = parent; ; folder = dirname(folder)) {
      const handle = await this.openFile(folder, 'r');
      try {
        await handle.sync();
      } finally {
        await handle.close();
      }
      if (folder === this.dir) break;
    }
  }

  /** Временные файлы, оставшиеся от сбоя процесса: при запуске их никто уже не допишет. */
  async sweep(): Promise<number> {
    let removed = 0;
    // Каталога нет — убирать нечего. Остальное (EACCES, EIO) — настоящий сбой: запуск должен его показать.
    const entries = await readdir(this.dir, { recursive: true, withFileTypes: true }).catch((e: unknown) => {
      if (noEntry(e)) return [];
      throw e;
    });
    for (const e of entries) {
      if (!e.isFile() || !e.name.endsWith('.tmp')) continue;
      await rm(join(e.parentPath, e.name), { force: true });
      removed++;
    }
    return removed;
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
      const now = options.now ?? Date.now;
      const started = now();
      /** Сколько ещё можно ждать: простой или срок по уже принятым байтам, что раньше. */
      const patience = () => {
        const pace = options.pace;
        if (!pace) return options.idleMs;
        return Math.min(options.idleMs, started + pace.graceMs + (size / pace.bytesPerSecond) * 1000 - now());
      };
      try {
        for (;;) {
          const wait = patience();
          if (wait <= 0) throw new BlobStalled();
          let timer: Timer | undefined;
          const stalled = new Promise<never>((_, reject) => {
            timer = setTimeout(() => reject(new BlobStalled()), wait);
          });
          const next = await Promise.race([reader.read(), stalled]).finally(() => clearTimeout(timer));
          if (next.done) {
            // Подтверждённая запись обязана быть полной: иначе старые байты уйдут в очистку.
            const { size: written } = await file.stat();
            if (written !== size) throw new Error(`Записано ${written} байт из ${size}`);
            return;
          }
          size += next.value.length;
          if (size > options.maxBytes) throw new BlobTooLarge();
          options.onChunk?.(next.value);
          // `write` — один системный вызов и может записать только часть куска, например на почти полном диске.
          for (let at = 0; at < next.value.length;) {
            const { bytesWritten } = await file.write(next.value, at, next.value.length - at);
            if (bytesWritten === 0) throw new Error('Запись на диск не продвигается');
            at += bytesWritten;
          }
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
