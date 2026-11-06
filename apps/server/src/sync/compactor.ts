// Когда сжимать (спека подсистемы 4, раздел 7.4): больше 2000 записей или 32 МБ журнала документа, либо через 5 минут после ухода
// последнего участника. Файлы — по одному; worker пересоздаётся каждые 50 задач и после паники wasm. Не вышло — журнал цел, строка
// в журнале сервера, следующая попытка через час. Документ, который ждёт срока, порог не считает: иначе каждая правка в нём
// переписывала бы весь файл ради остальных документов.

import type { Pool } from 'pg';
import type { CompactInit, CompactJob, CompactReply } from './compact.worker';

export interface CompactLimits {
  maxLogCount: number;
  maxLogBytes: number;
  idleMs: number;
  tickMs: number;
  retryAfterSeconds: number;
  tasksPerWorker: number;
  jobDeadlineMs: number;
  /** «Скачать копию» чаще этого по одному файлу файл не переписывает: ответ — «через столько-то секунд». */
  manualIntervalMs: number;
  /** Срок ответа «Скачать копию»: сжатие идёт дальше, а человек получает понятный отказ, а не обрыв соединения. */
  manualDeadlineMs: number;
}

export const COMPACT_LIMITS: CompactLimits = {
  maxLogCount: 2000,
  maxLogBytes: 32 * 1024 * 1024,
  idleMs: 5 * 60_000,
  tickMs: 30_000,
  retryAfterSeconds: 3600,
  tasksPerWorker: 50,
  jobDeadlineMs: 10 * 60_000,
  manualIntervalMs: 30_000,
  manualDeadlineMs: 60_000,
};

/** Ответ «Скачать копию»: копия полная; не вышло — через сколько секунд повторить; или копия была бы неполной: журнал документа,
 * который видят люди, в файл не входит (больше предела, паника при сжатии). */
export type ManualAnswer = { ok: true } | { ok: false; retryAfter: number } | { ok: false; incomplete: true };

/** Итог задачи: файл равен тому, что видят люди; переписан, но не весь журнал вошёл; не вышло. */
type Outcome = 'complete' | 'incomplete' | 'failed';

/** Зачем файл в очереди: по порогу задача идёт, только если порог перешёл документ, который не ждёт срока. */
type Reason = 'threshold' | 'idle';

export class Compactor {
  private worker: Worker | null = null;
  private tasks = 0;
  private seq = 0;
  private running: { file: string; done: () => void; timer: Timer | null } | null = null;
  /** Задачи «Скачать копию»: идут через одну с задачами таймера и порога, а не вперёд всех. */
  private readonly urgent = new Set<string>();
  private readonly queue = new Map<string, Reason>();
  private lastUrgent = false;
  private readonly waiters = new Map<string, ((o: Outcome) => void)[]>();
  private readonly leftAt = new Map<string, number>();
  /** Когда файл последний раз собран по «Скачать копию» и был ли он полным. Записи старше окна убирает таймер. */
  private readonly manualAt = new Map<string, { at: number; complete: boolean }>();
  /** Чья задача «Скачать копию» идёт: у человека одна разом. */
  private readonly manualBy = new Set<string>();
  /** `файл/документ` → до какого времени документ ждёт срока (`doc_state.compact_after`): порог его не считает. */
  private readonly waiting = new Map<string, number>();
  private readonly timer: Timer;
  private stopped = false;
  private readonly limits: CompactLimits;
  /** Занят ли файл людьми: сжатие простаивающих не трогает открытые. */
  active: (file: string) => boolean = () => false;
  /** Тесты: итог каждой задачи. */
  onResult: (file: string, r: CompactReply) => void = () => {};
  /** Тесты: сколько worker'ов заведено. */
  spawned = 0;

  constructor(
    private readonly pool: Pool,
    private readonly init: CompactInit,
    limits: Partial<CompactLimits> = {},
  ) {
    this.limits = { ...COMPACT_LIMITS, ...limits };
    this.timer = setInterval(() => void this.tick().catch(report), this.limits.tickMs);
    this.timer.unref?.();
  }

  /** После фиксации журнала: документ перерос предел. `logCount` — без пропущенных записей (`log_kept`). */
  note(file: string, doc: string, logCount: number, logBytes: number): void {
    if (logCount <= this.limits.maxLogCount && logBytes <= this.limits.maxLogBytes) return;
    const until = this.waiting.get(`${file}/${doc}`);
    if (until !== undefined && until > performance.now()) return;
    this.enqueue(file, 'threshold');
  }

  lastLeft(file: string): void {
    this.leftAt.set(file, performance.now());
  }

  /** Файлы с журналом без людей дольше срока, в том числе от прошлого запуска. Документы, ждущие срока, не в счёт. */
  private async tick(): Promise<void> {
    if (this.stopped) return;
    const now = performance.now();
    for (const [key, until] of this.waiting) if (until <= now) this.waiting.delete(key);
    for (const [file, m] of this.manualAt) if (now - m.at >= this.limits.manualIntervalMs) this.manualAt.delete(file);
    const { rows } = await this.pool.query<{ file_id: string }>(
      `SELECT DISTINCT s.file_id FROM doc_state s JOIN file f ON f.id = s.file_id
       WHERE s.log_count > s.log_kept AND (s.compact_after IS NULL OR s.compact_after <= now())
         AND (f.compact_after IS NULL OR f.compact_after <= now()) LIMIT 100`,
    );
    for (const { file_id: file } of rows) {
      if (this.active(file)) continue;
      const left = this.leftAt.get(file);
      if (left !== undefined && now - left < this.limits.idleMs) continue;
      this.leftAt.delete(file);
      this.enqueue(file, 'idle');
    }
  }

  /** Сжать сейчас и дождаться. `true` — файл равен тому, что видят люди; не вышло или вошло не всё — `false`, журнал цел. */
  async compactNow(file: string): Promise<boolean> {
    return (await this.request(file)) === 'complete';
  }

  private request(file: string): Promise<Outcome> {
    return new Promise((resolve) => {
      const list = this.waiters.get(file) ?? [];
      list.push(resolve);
      this.waiters.set(file, list);
      if (this.stopped) {
        this.settle(file, 'failed');
        return;
      }
      this.queue.delete(file);
      if (this.running?.file !== file) this.urgent.add(file);
      this.pump();
    });
  }

  /** «Скачать копию» (`POST …/compact`): не чаще `manualIntervalMs` на файл, у человека одна разом, ответ не позже
   * `manualDeadlineMs`. Копия, в которую вошёл не весь журнал, — не успех. */
  async manual(file: string, user: string): Promise<ManualAnswer> {
    if (this.manualBy.has(user)) return { ok: false, retryAfter: Math.ceil(this.limits.manualIntervalMs / 1000) };
    const last = this.manualAt.get(file);
    const wait = last === undefined ? 0 : this.limits.manualIntervalMs - (performance.now() - last.at);
    if (last && wait > 0) {
      // Новых записей нет — прошлая сборка в силе; есть — копия старее того, что видят люди.
      const { rows } = await this.pool.query<{ any: boolean }>(
        `SELECT EXISTS (SELECT 1 FROM doc_state WHERE file_id = $1 AND log_count > log_kept
                        AND (compact_after IS NULL OR compact_after <= now())) AS any`,
        [file],
      );
      if (rows[0]?.any) return { ok: false, retryAfter: Math.ceil(wait / 1000) };
      return last.complete ? { ok: true } : { ok: false, incomplete: true };
    }
    this.manualBy.add(user);
    const work = this.request(file).finally(() => this.manualBy.delete(user));
    let timer: Timer | undefined;
    const late = new Promise<'late'>((r) => {
      timer = setTimeout(() => r('late'), this.limits.manualDeadlineMs);
    });
    const done = await Promise.race([work, late]).finally(() => clearTimeout(timer));
    if (done === 'late') return { ok: false, retryAfter: 30 };
    if (done === 'failed') return { ok: false, retryAfter: 60 };
    this.manualAt.set(file, { at: performance.now(), complete: done === 'complete' });
    return done === 'complete' ? { ok: true } : { ok: false, incomplete: true };
  }

  private settle(file: string, o: Outcome): void {
    const list = this.waiters.get(file);
    this.waiters.delete(file);
    for (const w of list ?? []) w(o);
  }

  enqueue(file: string, reason: Reason = 'idle'): void {
    if (this.stopped) {
      this.settle(file, 'failed');
      return;
    }
    // Задача по простою проверяет всё, по порогу — только порог: из двух остаётся та, что сожмёт больше.
    if (!this.urgent.has(file) && this.queue.get(file) !== 'idle') this.queue.set(file, reason);
    this.pump();
  }

  /** «Скачать копию» и задачи таймера — через одну: свои файлы одного человека не занимают сжатие сервера. */
  private pump(): void {
    if (this.stopped || this.running) return;
    let file: string;
    let reason: Reason | 'manual';
    const takeUrgent = this.urgent.size > 0 && (!this.lastUrgent || this.queue.size === 0);
    if (takeUrgent) {
      file = this.urgent.values().next().value as string;
      this.urgent.delete(file);
      reason = 'manual';
    } else {
      const next = this.queue.entries().next();
      if (next.done) return;
      [file, reason] = next.value;
      this.queue.delete(file);
    }
    this.lastUrgent = takeUrgent;
    // Метка «занято» ставится сразу: второй вызов `pump` до начала задачи не запустит вторую.
    this.running = { file, done: () => {}, timer: null };
    void this.run(file, reason);
  }

  private spawn(): Worker {
    this.spawned++;
    const w = new Worker(new URL('./compact.worker.ts', import.meta.url));
    w.unref?.();
    return w;
  }

  /** Порог перешёл документ, который не ждёт срока: иначе сжимать по порогу нечего. */
  private async overThreshold(file: string): Promise<boolean> {
    const { rows } = await this.pool.query<{ any: boolean }>(
      `SELECT EXISTS (SELECT 1 FROM doc_state WHERE file_id = $1 AND (log_count - log_kept > $2 OR log_bytes > $3)
                      AND (compact_after IS NULL OR compact_after <= now())) AS any`,
      [file, this.limits.maxLogCount, this.limits.maxLogBytes],
    );
    return rows[0]?.any === true;
  }

  private async run(file: string, reason: Reason | 'manual'): Promise<void> {
    let outcome: Outcome = 'failed';
    try {
      const due = await this.pool.query<{ waits: boolean }>('SELECT compact_after > now() AS waits FROM file WHERE id = $1', [file]);
      if (due.rows[0]?.waits || this.stopped) return;
      // По порогу сжимать нечего — если только «Скачать копию» не ждёт этот же файл.
      if (reason === 'threshold' && !(await this.overThreshold(file)) && !this.waiters.has(file)) {
        outcome = 'complete';
        return;
      }
      if (this.tasks >= this.limits.tasksPerWorker) this.dropWorker();
      this.worker ??= this.spawn();
      this.tasks++;
      const worker = this.worker;
      const id = ++this.seq;
      outcome = await new Promise<Outcome>((done) => {
        const timer = setTimeout(() => {
          console.error('Сжатие: файл', file, 'не уложился в срок');
          this.dropWorker();
          void this.failed(file).finally(() => done('failed'));
        }, this.limits.jobDeadlineMs);
        timer.unref?.();
        this.running = { file, done: () => done('failed'), timer };
        worker.onmessage = (e: MessageEvent<CompactReply>) => {
          if (e.data.id !== id) return;
          clearTimeout(timer);
          void this.finish(file, e.data).then(done, (err: unknown) => {
            report(err);
            done('failed');
          });
        };
        worker.onerror = (e) => {
          e.preventDefault?.();
          clearTimeout(timer);
          console.error('Сжатие: worker упал на файле', file, e.message);
          this.dropWorker();
          void this.finish(file, { id, ok: false, message: e.message }).then(done, () => done('failed'));
        };
        worker.postMessage({
          id,
          file,
          init: { ...this.init, retryAfterSeconds: this.limits.retryAfterSeconds },
        } satisfies CompactJob & { init: CompactInit });
      });
    } catch (e) {
      // База недоступна ещё до задачи: ждущие получают отказ, очередь идёт дальше.
      report(e);
    } finally {
      if (this.running?.timer) clearTimeout(this.running.timer);
      this.running = null;
      this.settle(file, outcome);
      this.pump();
    }
  }

  private wait(file: string, doc: string): void {
    this.waiting.set(`${file}/${doc}`, performance.now() + this.limits.retryAfterSeconds * 1000);
  }

  /** Итог задачи. Пропущенные записи и отложенные документы названы в журнале сервера. */
  private async finish(file: string, r: CompactReply): Promise<Outcome> {
    this.onResult(file, r);
    if (r.ok) {
      if (r.result.kind === 'raced') return 'failed';
      for (const f of r.result.skipped) {
        console.error(
          `Сжатие: файл ${file}, документ ${f.doc}: не применилось обновление ${f.seq}. Оно оставлено в журнале, остальное сжато`,
        );
      }
      for (const f of r.result.failed) {
        console.error(`Сжатие: файл ${file}, документ ${f.doc} больше предела. Снимок документа не заменён, журнал цел`);
      }
      for (const doc of r.result.deferred) this.wait(file, doc);
      return r.result.incomplete.length > 0 ? 'incomplete' : 'complete';
    }
    console.error(`Сжатие: файл ${file}:`, r.message);
    // Паника wasm портит модуль: следующий файл — на новом worker'е. Документ ждёт срока, остальные документы — следующей задачей.
    this.dropWorker();
    if (r.doc !== undefined) {
      this.wait(file, r.doc);
      await this.pool
        .query(`UPDATE doc_state SET compact_after = now() + make_interval(secs => $3) WHERE file_id = $1 AND doc = $2`, [
          file,
          r.doc,
          this.limits.retryAfterSeconds,
        ])
        .catch(report);
    } else {
      await this.failed(file);
    }
    return 'failed';
  }

  private async failed(file: string): Promise<void> {
    await this.pool
      .query(`UPDATE file SET compact_after = now() + make_interval(secs => $2) WHERE id = $1`, [file, this.limits.retryAfterSeconds])
      .catch(report);
  }

  private dropWorker(): void {
    this.worker?.terminate();
    this.worker = null;
    this.tasks = 0;
  }

  /** Тесты: держит ли сжатие worker. */
  hasWorker(): boolean {
    return this.worker !== null;
  }

  /** Тесты: сколько файлов помнит «Скачать копию». */
  manualEntries(): number {
    return this.manualAt.size;
  }

  stop(): void {
    this.stopped = true;
    this.urgent.clear();
    this.queue.clear();
    clearInterval(this.timer);
    if (this.running) {
      if (this.running.timer) clearTimeout(this.running.timer);
      this.running.done();
    }
    for (const file of this.waiters.keys()) this.settle(file, 'failed');
    this.dropWorker();
  }
}

function report(e: unknown): void {
  console.error('Сжатие:', e instanceof Error ? e.message : e);
}
