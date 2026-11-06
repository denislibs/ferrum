// Сторона сервера у worker'а проверки: очередь с пределом, замена worker'а после паники и срока, загрузка документа
// по первому требованию. Основной поток Loro не загружает: паника wasm не задевает приём запросов.
//
// Worker один на сервер, поэтому чужие документы не должны его занимать: загрузок у человека — одна разом, очередь
// загрузок — по людям и со сроком, документ перезагружается не чаще раза в `reloadMs`, а документ, чей журнал роняет
// Loro, ждёт на карантине и worker больше не роняет. Вина документа, как и вина пачки, доказывается второй паникой — на
// свежем worker'е, где в это время не загружается ничего другого.

import type { Pool } from 'pg';
import type { BlobStore } from '../blob-store';
import { readDocSnapshot, readSection, type DocSection } from './container-read';
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
  /** Загрузок разом у одного человека: свои документы не занимают всю очередь. */
  loadsPerUser: number;
  /** Дольше этого загрузка не ждёт очереди: пачка получает «повторите». */
  loadWaitMs: number;
  /** Документ загружается заново не чаще: пачка с разрывом сбрасывает документ, и иначе каждая стоила бы полной загрузки. */
  reloadMs: number;
  /** Карантин документа, чья загрузка роняет Loro и на свежем worker'е; каждый следующий — вдвое дольше, до часа. */
  quarantineMs: number;
  /** Журнал читается кусками не больше этого: одна запись бывает до 16 МБ, кусок — не меньше одной записи. */
  logChunkBytes: number;
  /** Байт в очереди проверки: пачки и снимки для версии снимка. */
  maxQueueBytes: number;
  /** Тесты подменяют worker. */
  workerUrl: URL;
}

const DEFAULTS: ValidatorOptions = {
  budgetBytes: 256 * 1024 * 1024,
  memoryLimitBytes: 2 * 1024 * 1024 * 1024,
  loadsAtOnce: 2,
  loadsPerUser: 1,
  loadWaitMs: 10_000,
  reloadMs: 1000,
  quarantineMs: 60_000,
  logChunkBytes: 16 * 1024 * 1024,
  maxQueueBytes: MAX_VALIDATOR_QUEUE_BYTES,
  workerUrl: new URL('./validator.worker.ts', import.meta.url),
};

export type Verdict =
  /** `chain` — загрузка документа, на которой пачка проверена: отказ журнала отзывает её (`reject`). */
  | { kind: 'ok'; start: Record<string, number>; end: Record<string, number>; pending: boolean; chain: string }
  /** `fatal` — пачка роняет Loro и на новом worker'е с заново загруженным документом: её вина доказана. */
  | { kind: 'bad'; fatal: boolean }
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
  /** Номер по порядку заведения: worker новее паники в ней не участвовал. */
  gen: number;
}

/** Загрузка шла в worker, который сменили по памяти. */
class Moved extends Error {}
/** Загрузка уронила Loro. `proven` — загрузка шла одна на свежем worker'е: виноват документ, ему карантин. Иначе паника
 * могла быть и не от него (память worker'а заняли чужие документы): повтор. */
class LoadPanic extends Error {
  constructor(
    message: string,
    readonly proven: boolean,
    /** Worker, в котором случилась паника. */
    readonly gen: number,
  ) {
    super(message);
  }
}
/** Очередь загрузок не дошла до этой в срок. */
class LoadLate extends Error {}

/** Наибольший карантин документа. */
const MAX_QUARANTINE_MS = 3_600_000;

interface LoadTurn {
  user: string;
  /** Одна на сервере: не начинается, пока идут другие, и другие не начинаются, пока идёт она. */
  alone: boolean;
  go: () => void;
  late: (e: Error) => void;
}

export class Validator {
  /** Заводится по первой работе: сервер без открытых файлов память wasm не держит. */
  private current: Slot | null = null;
  private readonly retiring = new Set<Slot>();
  private seq = 0;
  private epoch = 0;
  private queued = 0;
  private readonly loading = new Map<string, { p: Promise<void>; cancelled: boolean }>();
  private loadsRunning = 0;
  /** Загрузок у каждого человека сейчас. */
  private readonly loadsBy = new Map<string, number>();
  private readonly loadQueue: LoadTurn[] = [];
  /** Когда документ начал загружаться в последний раз. */
  private readonly loadedAt = new Map<string, number>();
  /** Документ ушёл из worker'а из-за пачки (ждала неизвестного или её отверг журнал): следующая загрузка — не раньше срока.
   * Замена worker'а и вытеснение по памяти сюда не входят: в них человек не виноват. */
  private readonly dropped = new Set<string>();
  /** Документы, чей снимок или журнал роняет Loro: до `until` пачки получают «повторите», worker не трогается. Запись
   * остаётся и после срока (до часа): одна попытка, и снова карантин, если документ всё ещё роняет. */
  private readonly quarantine = new Map<string, { until: number; times: number }>();
  /** Загрузка документа уронила Loro, но вина не доказана: следующая его загрузка идёт одна. `gen` — worker паники. */
  private readonly suspect = new Map<string, { at: number; gen: number }>();
  private gens = 0;
  /** Идёт загрузка «одна на сервере». */
  private alone = false;
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
    const slot: Slot = { w, waiting: new Map(), gen: ++this.gens };
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
    for (const [key, at] of this.loadedAt) {
      if (now - at <= this.options.reloadMs) continue;
      this.loadedAt.delete(key);
      this.dropped.delete(key);
    }
    for (const [key, q] of this.quarantine) if (now - q.until > MAX_QUARANTINE_MS) this.quarantine.delete(key);
    for (const [key, x] of this.suspect) if (now - x.at > MAX_QUARANTINE_MS) this.suspect.delete(key);
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

  private ensureLoaded(file: string, doc: string, user: string): Promise<void> {
    const key = `${file}/${doc}`;
    const going = this.loading.get(key);
    if (going) return going.p;
    const entry = { p: Promise.resolve(), cancelled: false };
    entry.p = this.load(file, doc, key, entry, user).finally(() => {
      if (this.loading.get(key) === entry) this.loading.delete(key);
    });
    this.loading.set(key, entry);
    return entry.p;
  }

  private mayStart(user: string, alone: boolean): boolean {
    if (this.alone) return false;
    if (alone) return this.loadsRunning === 0;
    return this.loadsRunning < this.options.loadsAtOnce && (this.loadsBy.get(user) ?? 0) < this.options.loadsPerUser;
  }

  private admit(user: string, alone: boolean): void {
    this.loadsRunning++;
    this.loadsBy.set(user, (this.loadsBy.get(user) ?? 0) + 1);
    if (alone) this.alone = true;
  }

  /** Свободное место — тому из ждущих, у кого меньше всего загрузок идёт; среди равных — кто раньше. Загрузка «одна» ждёт
   * только идущих: пока она в очереди, новые не начинаются. */
  private next(): void {
    for (;;) {
      const first = this.loadQueue.findIndex((t) => t.alone);
      let best = -1;
      if (first >= 0) {
        if (this.mayStart(this.loadQueue[first].user, true)) best = first;
      } else {
        for (const [i, t] of this.loadQueue.entries()) {
          if (!this.mayStart(t.user, false)) continue;
          if (best < 0 || (this.loadsBy.get(t.user) ?? 0) < (this.loadsBy.get(this.loadQueue[best].user) ?? 0)) best = i;
        }
      }
      if (best < 0) return;
      const [turn] = this.loadQueue.splice(best, 1);
      this.admit(turn.user, turn.alone);
      turn.go();
    }
  }

  /** Очередь загрузок: не больше `loadsAtOnce` разом и `loadsPerUser` у человека, ожидание — не дольше `loadWaitMs`. `alone` —
   * загрузка одна на сервере. */
  private async gate<T>(user: string, alone: boolean, work: () => Promise<T>): Promise<T> {
    if (this.mayStart(user, alone) && this.loadQueue.length === 0) this.admit(user, alone);
    else {
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => {
          const at = this.loadQueue.indexOf(turn);
          if (at >= 0) this.loadQueue.splice(at, 1);
          // Ушла загрузка «одна»: остальные, ждавшие её, могут начинаться.
          this.next();
          reject(new LoadLate('загрузка не дождалась очереди'));
        }, this.options.loadWaitMs);
        const turn: LoadTurn = {
          user,
          alone,
          go: () => {
            clearTimeout(timer);
            resolve();
          },
          late: (e) => {
            clearTimeout(timer);
            reject(e);
          },
        };
        this.loadQueue.push(turn);
        this.next();
      });
    }
    try {
      return await work();
    } finally {
      if (alone) this.alone = false;
      this.loadsRunning--;
      const n = (this.loadsBy.get(user) ?? 1) - 1;
      if (n > 0) this.loadsBy.set(user, n);
      else this.loadsBy.delete(user);
      this.next();
    }
  }

  private async load(file: string, doc: string, key: string, entry: { cancelled: boolean }, user: string): Promise<void> {
    // Пачка с разрывом сбрасывает документ: подряд такие стоили бы полной загрузки каждая.
    const last = this.dropped.delete(key) ? this.loadedAt.get(key) : undefined;
    const pause = last === undefined ? 0 : last + this.options.reloadMs - performance.now();
    if (pause > 0) await Bun.sleep(pause);
    // Прошлая загрузка уронила Loro (или документ уже бывал на карантине): эта идёт одна, на свежем worker'е, и её паника —
    // вина документа.
    const alone = this.suspect.has(key) || this.quarantine.has(key);
    return this.gate(user, alone, async () => {
      if (entry.cancelled) throw new Error('загрузка отменена');
      this.loadedAt.set(key, performance.now());
      this.loads++;
      const slot = this.slot();
      // Свежий — заведён после паники: она его не испортила, а загрузка идёт одна.
      const fresh = slot.gen > (this.suspect.get(key)?.gen ?? 0);
      const epoch = ++this.epoch;
      const fail = (r: FromValidator | null) => {
        if (r?.ok) return;
        if (!r && slot !== this.current && !this.stopped) throw new Moved();
        if (r && !r.ok && r.reason === 'fatal') throw new LoadPanic(`паника: ${r.message}`, alone && fresh, slot.gen);
        throw new Error('worker заменён');
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

  /** Вердикт по пачке. Вызовы одного документа отвечают в порядке вызова. Байты должны жить до ответа. `user` — чья пачка:
   * загрузка документа, если нужна, идёт в его очередь. */
  async validate(file: string, doc: string, bytes: Uint8Array, user = ''): Promise<Verdict> {
    if (this.queued + bytes.length > this.options.maxQueueBytes) return { kind: 'retry' };
    const key = `${file}/${doc}`;
    const held = this.quarantine.get(key);
    if (held && held.until > performance.now()) return { kind: 'retry' };
    let panics = 0;
    for (let attempt = 0; attempt < 6; attempt++) {
      if (this.loading.has(key)) await this.loading.get(key)!.p.catch(() => {});
      const r = await this.send(this.slot(), { op: 'validate', key, bytes } as Omit<ToValidator, 'id'>, bytes.length);
      if (!r) return { kind: 'retry' };
      if (r.ok && 'end' in r) {
        // Ждущая пачка: worker выбросил документ.
        if (r.pending) this.dropped.add(key);
        return { kind: 'ok', start: r.start, end: r.end, pending: r.pending, chain: `${key}#${r.epoch}` };
      }
      if (!r.ok && r.reason === 'bad') {
        // Паника бывает и не от пачки (память worker'а кончилась, модуль испорчен раньше): винить пачку можно, только
        // если она уронила и новый worker с заново загруженным документом.
        if (!r.fatal) return { kind: 'bad', fatal: false };
        if (++panics >= 2) return { kind: 'bad', fatal: true };
      }
      try {
        await this.ensureLoaded(file, doc, user);
        this.quarantine.delete(key);
        this.suspect.delete(key);
      } catch (e) {
        // Worker сменился по памяти посреди загрузки: загрузить в новый.
        if (e instanceof Moved) continue;
        if (e instanceof LoadPanic) {
          // Вина не доказана: worker уже заменён, загрузка повторится одна на свежем.
          if (!e.proven) {
            this.suspect.set(key, { at: performance.now(), gen: e.gen });
            continue;
          }
          this.suspect.delete(key);
          this.hold(key);
        }
        if (!(e instanceof LoadLate)) console.error('Проверка пачек: не удалось загрузить документ:', e instanceof Error ? e.message : e);
        return { kind: 'retry' };
      }
    }
    return { kind: 'retry' };
  }

  /** Документ роняет Loro при загрузке: пачки к нему ждут, а не меняют общий worker одна за другой. */
  private hold(key: string): void {
    const times = (this.quarantine.get(key)?.times ?? 0) + 1;
    const ms = Math.min(MAX_QUARANTINE_MS, this.options.quarantineMs * 2 ** (times - 1));
    this.quarantine.set(key, { until: performance.now() + ms, times });
  }

  /** Журнал отверг пачку: документ проверки опередил журнал, следующая пачка загрузит его заново. */
  reject(chain: string): void {
    const at = chain.lastIndexOf('#');
    const key = chain.slice(0, at);
    const epoch = Number(chain.slice(at + 1));
    this.dropped.add(key);
    if (!this.current) return;
    void this.send(this.current, { op: 'drop', key, epoch } as Omit<ToValidator, 'id'>);
  }

  /** Вектор версий снимка, без загрузки документа. Чужие байты — пустой вектор; сбой worker'а — `null`, повторить позже. */
  async baseVersion(snapshot: Uint8Array | null): Promise<Record<string, number> | null> {
    if (!snapshot || snapshot.length === 0) return {};
    return this.base(snapshot, snapshot.length, []);
  }

  /** Вектор версий секции документа. Её байты — в очереди проверки с начала чтения: очередь полна — `busy`, байты не
   * читаются. В worker они уходят передачей, без копии. */
  async baseOf(section: DocSection): Promise<Record<string, number> | null | 'busy'> {
    if (section.size === 0) return {};
    if (this.stopped || this.queued + section.size > this.options.maxQueueBytes) return 'busy';
    this.queued += section.size;
    let bytes: Uint8Array;
    try {
      bytes = await readSection(section);
    } finally {
      this.queued -= section.size;
    }
    return this.base(bytes, bytes.length, [bytes.buffer as ArrayBuffer]);
  }

  private async base(snapshot: Uint8Array, size: number, transfer: ArrayBuffer[]): Promise<Record<string, number> | null> {
    const r = await this.send(this.slot(), { op: 'base', snapshot } as Omit<ToValidator, 'id'>, size, transfer);
    if (!r || !r.ok || !('base' in r)) return null;
    return r.base ?? {};
  }

  /** Файл заменён целиком: его документы в памяти больше не верны, идущие загрузки читают прежнее. */
  forget(file: string): void {
    // Новое содержимое — новый документ: прежняя паника о нём ничего не говорит.
    for (const key of [...this.quarantine.keys(), ...this.suspect.keys()]) {
      if (!key.startsWith(`${file}/`)) continue;
      this.quarantine.delete(key);
      this.suspect.delete(key);
    }
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
    for (const t of this.loadQueue.splice(0)) t.late(new LoadLate('проверка остановлена'));
  }
}
