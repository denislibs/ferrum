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

export interface ValidatorOptions {
  /** Документов в памяти worker'а на столько байт снимков и журнала. */
  budgetBytes: number;
  /** Память wasm worker'а больше этого — новые работы идут новому worker'у, старый дорабатывает своё и уходит. Память
   * wasm системе не возвращается (тот же приём, что в разделе 7.4), а предел модуля wasm32 — 4 ГБ. */
  memoryLimitBytes: number;
  /** Загрузок документов разом: каждая держит соединение пула и снимок в памяти основного потока. */
  loadsAtOnce: number;
  /** Журнал читается кусками не больше этого: одна запись бывает до 16 МБ, кусок — не меньше одной записи. */
  logChunkBytes: number;
  /** Тесты подменяют worker. */
  workerUrl: URL;
}

const DEFAULTS: ValidatorOptions = {
  budgetBytes: 256 * 1024 * 1024,
  memoryLimitBytes: 2 * 1024 * 1024 * 1024,
  loadsAtOnce: 2,
  logChunkBytes: 16 * 1024 * 1024,
  workerUrl: new URL('./validator.worker.ts', import.meta.url),
};

export type Verdict =
  /** `chain` — загрузка документа, на которой пачка проверена: отказ журнала отзывает её (`reject`). */
  | { kind: 'ok'; start: Record<string, number>; end: Record<string, number>; pending: boolean; chain: string }
  | { kind: 'bad' }
  /** Worker заменён не по вине этой пачки или очередь полна: повторить. */
  | { kind: 'retry' };

interface Waiting {
  done: (m: FromValidator | null) => void;
  size: number;
  at: number;
}

/** Один worker: свои ждущие работы. Уходящий доделывает своё, новых не получает. */
interface Slot {
  w: Worker;
  waiting: Map<number, Waiting>;
}

/** Загрузка шла в worker, который сменили по памяти. */
class Moved extends Error {}

export class Validator {
  /** Заводится по первой работе: сервер без открытых файлов память wasm не держит. */
  private current: Slot | null = null;
  private readonly retiring = new Set<Slot>();
  private seq = 0;
  private epoch = 0;
  private queued = 0;
  private readonly loading = new Map<string, { p: Promise<void>; cancelled: boolean }>();
  private loadsRunning = 0;
  private readonly loadQueue: (() => void)[] = [];
  private readonly watchdog: Timer;
  private stopped = false;
  private readonly options: ValidatorOptions;
  /** Тесты: сколько раз worker заменили после сбоя, сколько раз — по памяти, сколько загрузок. */
  restarts = 0;
  recycles = 0;
  loads = 0;

  constructor(
    private readonly pool: Pick<Pool, 'connect'>,
    private readonly blobs: BlobStore,
    options: Partial<ValidatorOptions> = {},
  ) {
    this.options = { ...DEFAULTS, ...options };
    this.watchdog = setInterval(() => this.checkHung(), 1000);
    this.watchdog.unref?.();
  }

  private spawn(): Slot {
    const w = new Worker(this.options.workerUrl);
    const slot: Slot = { w, waiting: new Map() };
    w.onmessage = (e: MessageEvent<FromValidator>) => this.answer(slot, e.data);
    w.onerror = (e) => {
      console.error("Проверка пачек: сбой worker'а:", e.message);
      e.preventDefault?.();
      this.kill(slot);
    };
    w.unref?.();
    w.postMessage({ id: 0, op: 'config', budget: this.options.budgetBytes } satisfies ToValidator);
    return slot;
  }

  private answer(slot: Slot, m: FromValidator): void {
    const w = slot.waiting.get(m.id);
    if (!w) return;
    slot.waiting.delete(m.id);
    this.queued -= w.size;
    // Паника: модуль wasm испорчен, остальная очередь пойдёт новому worker'у.
    if (!m.ok && (m.reason === 'fatal' || (m.reason === 'bad' && m.fatal))) {
      if (m.reason === 'fatal') console.error('Проверка пачек: паника на документе файла:', m.message);
      w.done(m);
      this.kill(slot);
      return;
    }
    w.done(m);
    // Память смотрим по ответу на пачку: загрузка, начатая на этом worker'е, доходит до конца на нём же.
    if (slot === this.current && m.ok && 'end' in m && (m.memory ?? 0) > this.options.memoryLimitBytes) this.retire(slot);
    if (this.retiring.has(slot) && slot.waiting.size === 0) this.terminate(slot);
  }

  /** Worker испорчен или завис: заменить. Ждавшие получают «повторите»: их вина не доказана. */
  private kill(slot: Slot): void {
    if (slot === this.current) {
      this.restarts++;
      this.current = null;
      this.cancelLoads();
    }
    this.terminate(slot);
  }

  /** Память worker'а выросла: новые работы — новому, этот доделает начатое. Начатые загрузки дойдут до старого, а пачка
   * после них загрузит документ в новый. */
  private retire(slot: Slot): void {
    this.recycles++;
    this.current = null;
    this.retiring.add(slot);
  }

  private terminate(slot: Slot): void {
    this.retiring.delete(slot);
    slot.w.terminate();
    for (const w of slot.waiting.values()) {
      this.queued -= w.size;
      w.done(null);
    }
    slot.waiting.clear();
  }

  /** Документы прежнего worker'а у нового не загружены: идущие загрузки шли туда. */
  private cancelLoads(): void {
    for (const l of this.loading.values()) l.cancelled = true;
    this.loading.clear();
  }

  private checkHung(): void {
    const now = performance.now();
    for (const slot of [...this.retiring, ...(this.current ? [this.current] : [])]) {
      for (const w of slot.waiting.values()) {
        if (now - w.at > WORK_DEADLINE_MS) {
          console.error('Проверка пачек: worker не отвечает, замена');
          this.kill(slot);
          break;
        }
      }
    }
  }

  private slot(): Slot {
    this.current ??= this.spawn();
    return this.current;
  }

  private send(slot: Slot, work: Omit<ToValidator, 'id'>, size = 0, transfer: ArrayBuffer[] = []): Promise<FromValidator | null> {
    if (this.stopped || (slot !== this.current && !this.retiring.has(slot))) return Promise.resolve(null);
    const id = ++this.seq;
    this.queued += size;
    return new Promise((done) => {
      slot.waiting.set(id, { done, size, at: performance.now() });
      slot.w.postMessage({ ...work, id } as ToValidator, transfer);
    });
  }

  private ensureLoaded(file: string, doc: string): Promise<void> {
    const key = `${file}/${doc}`;
    const going = this.loading.get(key);
    if (going) return going.p;
    const entry = { p: Promise.resolve(), cancelled: false };
    entry.p = this.load(file, doc, key, entry).finally(() => {
      if (this.loading.get(key) === entry) this.loading.delete(key);
    });
    this.loading.set(key, entry);
    return entry.p;
  }

  /** Очередь загрузок: не больше `loadsAtOnce` разом. */
  private async gate<T>(work: () => Promise<T>): Promise<T> {
    if (this.loadsRunning >= this.options.loadsAtOnce) await new Promise<void>((r) => this.loadQueue.push(r));
    this.loadsRunning++;
    try {
      return await work();
    } finally {
      this.loadsRunning--;
      this.loadQueue.shift()?.();
    }
  }

  private load(file: string, doc: string, key: string, entry: { cancelled: boolean }): Promise<void> {
    return this.gate(async () => {
      if (entry.cancelled) throw new Error('загрузка отменена');
      this.loads++;
      const slot = this.slot();
      const epoch = ++this.epoch;
      const fail = (r: FromValidator | null) => {
        if (r?.ok) return;
        if (!r && slot !== this.current && !this.stopped) throw new Moved();
        throw new Error(r && !r.ok && r.reason === 'fatal' ? `паника: ${r.message}` : 'worker заменён');
      };
      const client = await this.pool.connect();
      try {
        // Содержимое и журнал — одним снимком базы: сжатие между ними не потеряет и не удвоит записи. Журнал — кусками,
        // каждый кусок уходит в worker раньше чтения следующего: память основного потока — один кусок.
        await client.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');
        const f = await client.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [file]);
        const snapshot = await readDocSnapshot(this.blobs, f.rows[0]?.content_key ?? null, doc);
        if (entry.cancelled) throw new Error('загрузка отменена');
        fail(
          await this.send(
            slot,
            { op: 'load', key, epoch, snapshot } as Omit<ToValidator, 'id'>,
            snapshot?.length ?? 0,
            snapshot ? [snapshot.buffer as ArrayBuffer] : [],
          ),
        );
        let after = 0;
        for (;;) {
          const sizes = await client.query<{ seq: string; size: number }>(
            'SELECT seq, octet_length(bytes) AS size FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq > $3 ORDER BY seq LIMIT 4096',
            [file, doc, after],
          );
          if (sizes.rows.length === 0) break;
          let last = 0;
          let sum = 0;
          for (const r of sizes.rows) {
            if (last > 0 && sum + r.size > this.options.logChunkBytes) break;
            sum += r.size;
            last = Number(r.seq);
          }
          const { rows } = await client.query<{ seq: string; bytes: Uint8Array }>(
            'SELECT seq, bytes FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq > $3 AND seq <= $4 ORDER BY seq',
            [file, doc, after, last],
          );
          if (entry.cancelled) throw new Error('загрузка отменена');
          const total = rows.reduce((n, r) => n + r.bytes.length, 0);
          const log = new Uint8Array(total);
          let at = 0;
          for (const r of rows) {
            log.set(r.bytes, at);
            at += r.bytes.length;
          }
          after = Number(rows[rows.length - 1].seq);
          fail(
            await this.send(
              slot,
              { op: 'append', key, epoch, log, sizes: rows.map((r) => r.bytes.length) } as Omit<ToValidator, 'id'>,
              total,
              [log.buffer as ArrayBuffer],
            ),
          );
        }
        await client.query('COMMIT');
      } catch (e) {
        await client.query('ROLLBACK').catch(() => {});
        throw e;
      } finally {
        client.release();
      }
    });
  }

  /** Вердикт по пачке. Вызовы одного документа отвечают в порядке вызова. Байты должны жить до ответа. */
  async validate(file: string, doc: string, bytes: Uint8Array): Promise<Verdict> {
    if (this.queued + bytes.length > MAX_VALIDATOR_QUEUE_BYTES) return { kind: 'retry' };
    const key = `${file}/${doc}`;
    let panics = 0;
    for (let attempt = 0; attempt < 6; attempt++) {
      if (this.loading.has(key)) await this.loading.get(key)!.p.catch(() => {});
      const r = await this.send(this.slot(), { op: 'validate', key, bytes } as Omit<ToValidator, 'id'>, bytes.length);
      if (!r) return { kind: 'retry' };
      if (r.ok && 'end' in r) return { kind: 'ok', start: r.start, end: r.end, pending: r.pending, chain: `${key}#${r.epoch}` };
      if (!r.ok && r.reason === 'bad') {
        // Паника бывает и не от пачки (память worker'а кончилась, модуль испорчен раньше): винить пачку можно, только
        // если она уронила и новый worker с заново загруженным документом.
        if (!r.fatal || ++panics >= 2) return { kind: 'bad' };
      }
      try {
        await this.ensureLoaded(file, doc);
      } catch (e) {
        // Worker сменился по памяти посреди загрузки: загрузить в новый.
        if (e instanceof Moved) continue;
        console.error('Проверка пачек: не удалось загрузить документ:', e instanceof Error ? e.message : e);
        return { kind: 'retry' };
      }
    }
    return { kind: 'retry' };
  }

  /** Журнал отверг пачку: документ проверки опередил журнал, следующая пачка загрузит его заново. */
  reject(chain: string): void {
    const at = chain.lastIndexOf('#');
    const key = chain.slice(0, at);
    const epoch = Number(chain.slice(at + 1));
    if (!this.current) return;
    void this.send(this.current, { op: 'drop', key, epoch } as Omit<ToValidator, 'id'>);
  }

  /** Вектор версий снимка, без загрузки документа. Чужие байты — пустой вектор; сбой worker'а — `null`, повторить позже. */
  async baseVersion(snapshot: Uint8Array | null): Promise<Record<string, number> | null> {
    if (!snapshot || snapshot.length === 0) return {};
    const r = await this.send(this.slot(), { op: 'base', snapshot } as Omit<ToValidator, 'id'>);
    if (!r || !r.ok || !('base' in r)) return null;
    return r.base ?? {};
  }

  /** Файл заменён целиком: его документы в памяти больше не верны, идущие загрузки читают прежнее. */
  forget(file: string): void {
    for (const [key, l] of this.loading) {
      if (!key.startsWith(`${file}/`)) continue;
      l.cancelled = true;
      this.loading.delete(key);
    }
    if (!this.current) return;
    void this.send(this.current, { op: 'forget', prefix: `${file}/` } as Omit<ToValidator, 'id'>);
  }

  stop(): void {
    this.stopped = true;
    clearInterval(this.watchdog);
    for (const slot of [...this.retiring, ...(this.current ? [this.current] : [])]) this.terminate(slot);
    this.current = null;
    for (const r of this.loadQueue.splice(0)) r();
  }
}
