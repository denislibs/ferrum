// Сторона сервера у worker'а проверки: очередь с пределом, замена worker'а после паники и срока, загрузка документа
// по первому требованию. Основной поток Loro не загружает: паника wasm не задевает приём запросов.

import type { Pool } from 'pg';
import type { BlobStore } from '../blob-store';
import { readDocSnapshot } from './container-read';
import type { FromValidator, ToValidator } from './validator.worker';

/** Больше этого в очереди проверки: клиент получает паузу, сервер не копит байты без конца. */
export const MAX_VALIDATOR_QUEUE_BYTES = 256 * 1024 * 1024;
/** Дольше этого на одну работу — worker завис: замена, работы очереди получают отказ «повторите». */
const WORK_DEADLINE_MS = 60_000;
/** Столько загрузок на один worker: память wasm не возвращается системе (тот же приём, что в разделе 7.4). */
const LOADS_PER_WORKER = 50;

export type Verdict =
  | { kind: 'ok'; start: Record<string, number>; end: Record<string, number>; pending: boolean }
  | { kind: 'bad' }
  /** Worker заменён не по вине этой пачки или очередь полна: повторить. */
  | { kind: 'retry' };

interface Waiting {
  done: (m: FromValidator | null) => void;
  size: number;
  at: number;
}

export class Validator {
  /** Заводится по первой работе: сервер без открытых файлов память wasm не держит. */
  private worker: Worker | null = null;
  private seq = 0;
  private loads = 0;
  private queued = 0;
  private readonly waiting = new Map<number, Waiting>();
  private readonly loading = new Map<string, Promise<void>>();
  private readonly watchdog: Timer;
  private stopped = false;
  /** Тесты: сколько раз worker заменили. */
  restarts = 0;

  constructor(
    private readonly pool: Pool,
    private readonly blobs: BlobStore,
  ) {
    this.watchdog = setInterval(() => this.checkHung(), 1000);
    this.watchdog.unref?.();
  }

  private spawn(): Worker {
    const w = new Worker(new URL('./validator.worker.ts', import.meta.url));
    w.onmessage = (e: MessageEvent<FromValidator>) => this.answer(e.data);
    w.onerror = (e) => {
      console.error("Проверка пачек: сбой worker'а:", e.message);
      e.preventDefault?.();
      this.restart();
    };
    w.unref?.();
    return w;
  }

  private answer(m: FromValidator): void {
    const w = this.waiting.get(m.id);
    if (!w) return;
    this.waiting.delete(m.id);
    this.queued -= w.size;
    w.done(m);
    // Паника: модуль wasm испорчен, остальная очередь пойдёт новому worker'у.
    if (!m.ok && m.reason === 'bad' && m.fatal) this.restart();
  }

  /** Новый worker без документов. Ждавшие получают «повторите»: их вина не доказана. */
  private restart(): void {
    if (this.stopped) return;
    this.restarts++;
    this.worker?.terminate();
    this.worker = this.spawn();
    this.loads = 0;
    this.loading.clear();
    for (const w of this.waiting.values()) w.done(null);
    this.waiting.clear();
    this.queued = 0;
  }

  private checkHung(): void {
    const now = performance.now();
    for (const w of this.waiting.values()) {
      if (now - w.at > WORK_DEADLINE_MS) {
        console.error('Проверка пачек: worker не отвечает, замена');
        this.restart();
        return;
      }
    }
  }

  private send(work: Omit<ToValidator, 'id'>, size = 0): Promise<FromValidator | null> {
    if (this.stopped) return Promise.resolve(null);
    const id = ++this.seq;
    this.queued += size;
    return new Promise((done) => {
      this.waiting.set(id, { done, size, at: performance.now() });
      this.worker ??= this.spawn();
      this.worker.postMessage({ ...work, id } as ToValidator);
    });
  }

  private ensureLoaded(file: string, doc: string): Promise<void> {
    const key = `${file}/${doc}`;
    const going = this.loading.get(key);
    if (going) return going;
    const p = this.load(file, doc, key).finally(() => this.loading.delete(key));
    this.loading.set(key, p);
    return p;
  }

  private async load(file: string, doc: string, key: string): Promise<void> {
    if (++this.loads > LOADS_PER_WORKER && this.waiting.size === 0) this.restart();
    const client = await this.pool.connect();
    let snapshot: Uint8Array | null;
    let rows: { bytes: Uint8Array }[];
    try {
      // Содержимое и журнал — одним снимком базы: сжатие между ними не потеряет и не удвоит записи.
      await client.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');
      const f = await client.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [file]);
      snapshot = await readDocSnapshot(this.blobs, f.rows[0]?.content_key ?? null, doc);
      rows = (
        await client.query<{ bytes: Uint8Array }>('SELECT bytes FROM doc_update WHERE file_id = $1 AND doc = $2 ORDER BY seq', [file, doc])
      ).rows;
      await client.query('COMMIT');
    } catch (e) {
      await client.query('ROLLBACK').catch(() => {});
      throw e;
    } finally {
      client.release();
    }
    await this.send({ op: 'load', key, snapshot, log: rows.map((r) => r.bytes) } as Omit<ToValidator, 'id'>);
  }

  /** Вердикт по пачке. Вызовы одного документа отвечают в порядке вызова. Байты должны жить до ответа. */
  async validate(file: string, doc: string, bytes: Uint8Array): Promise<Verdict> {
    if (this.queued + bytes.length > MAX_VALIDATOR_QUEUE_BYTES) return { kind: 'retry' };
    const key = `${file}/${doc}`;
    for (let attempt = 0; attempt < 3; attempt++) {
      if (this.loading.has(key)) await this.loading.get(key)!.catch(() => {});
      const r = await this.send({ op: 'validate', key, bytes } as Omit<ToValidator, 'id'>, bytes.length);
      if (!r) return { kind: 'retry' };
      if (r.ok && 'end' in r) return { kind: 'ok', start: r.start, end: r.end, pending: r.pending };
      if (!r.ok && r.reason === 'bad') return { kind: 'bad' };
      try {
        await this.ensureLoaded(file, doc);
      } catch (e) {
        console.error('Проверка пачек: не удалось загрузить документ:', e instanceof Error ? e.message : e);
        return { kind: 'retry' };
      }
    }
    return { kind: 'retry' };
  }

  /** Вектор версий снимка, без загрузки документа. Чужие байты — пустой вектор. */
  async baseVersion(snapshot: Uint8Array | null): Promise<Record<string, number>> {
    if (!snapshot || snapshot.length === 0) return {};
    const r = await this.send({ op: 'base', snapshot } as Omit<ToValidator, 'id'>);
    return r?.ok && 'base' in r ? r.base : {};
  }

  /** Файл заменён целиком: его документы в памяти больше не верны. */
  forget(file: string): void {
    void this.send({ op: 'forget', prefix: `${file}/` } as Omit<ToValidator, 'id'>);
  }

  stop(): void {
    this.stopped = true;
    clearInterval(this.watchdog);
    this.worker?.terminate();
    for (const w of this.waiting.values()) w.done(null);
    this.waiting.clear();
  }
}
