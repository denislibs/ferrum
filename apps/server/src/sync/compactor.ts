// Когда сжимать (спека подсистемы 4, раздел 7.4): больше 2000 записей или 32 МБ журнала документа, либо через 5 минут после ухода
// последнего участника. Файлы — по одному; worker пересоздаётся каждые 50 задач и после паники wasm. Не вышло — журнал цел, строка
// в журнале сервера, следующая попытка через час.

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

/** Ответ «Скачать копию»: не вышло — через сколько секунд повторить. */
export type ManualAnswer = { ok: true } | { ok: false; retryAfter: number };

export class Compactor {
  private worker: Worker | null = null;
  private tasks = 0;
  private seq = 0;
  private running: { file: string; done: () => void; timer: Timer | null } | null = null;
  /** Задачи «Скачать копию» — вперёд тех, что поставил таймер. */
  private readonly urgent = new Set<string>();
  private readonly queue = new Set<string>();
  private readonly waiters = new Map<string, ((ok: boolean) => void)[]>();
  private readonly leftAt = new Map<string, number>();
  /** Когда файл последний раз переписан по «Скачать копию». */
  private readonly manualAt = new Map<string, number>();
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

  /** После фиксации журнала: документ перерос предел. */
  note(file: string, logCount: number, logBytes: number): void {
    if (logCount > this.limits.maxLogCount || logBytes > this.limits.maxLogBytes) this.enqueue(file);
  }

  lastLeft(file: string): void {
    this.leftAt.set(file, performance.now());
  }

  /** Файлы с журналом без людей дольше срока, в том числе от прошлого запуска. Документы, ждущие срока, не в счёт. */
  private async tick(): Promise<void> {
    if (this.stopped) return;
    const { rows } = await this.pool.query<{ file_id: string }>(
      `SELECT DISTINCT s.file_id FROM doc_state s JOIN file f ON f.id = s.file_id
       WHERE s.log_count > 0 AND (s.compact_after IS NULL OR s.compact_after <= now())
         AND (f.compact_after IS NULL OR f.compact_after <= now()) LIMIT 100`,
    );
    const now = performance.now();
    for (const { file_id: file } of rows) {
      if (this.active(file)) continue;
      const left = this.leftAt.get(file);
      if (left !== undefined && now - left < this.limits.idleMs) continue;
      this.leftAt.delete(file);
      this.enqueue(file);
    }
  }

  /** Сжать сейчас и дождаться: «Скачать копию» отдаёт файл целиком, а не снимок без журнала. Не вышло — `false`, журнал цел. */
  compactNow(file: string): Promise<boolean> {
    return new Promise((resolve) => {
      const list = this.waiters.get(file) ?? [];
      list.push(resolve);
      this.waiters.set(file, list);
      if (this.stopped) {
        this.settle(file, false);
        return;
      }
      this.queue.delete(file);
      if (this.running?.file !== file) this.urgent.add(file);
      this.pump();
    });
  }

  /** «Скачать копию» (`POST …/compact`): не чаще `manualIntervalMs` на файл, ответ не позже `manualDeadlineMs`. */
  async manual(file: string): Promise<ManualAnswer> {
    const last = this.manualAt.get(file);
    const wait = last === undefined ? 0 : this.limits.manualIntervalMs - (performance.now() - last);
    if (wait > 0) {
      // Журнал пуст — переписывать нечего, копия уже полная.
      const { rows } = await this.pool.query<{ any: boolean }>(
        'SELECT EXISTS (SELECT 1 FROM doc_state WHERE file_id = $1 AND log_count > 0) AS any',
        [file],
      );
      if (!rows[0]?.any) return { ok: true };
      return { ok: false, retryAfter: Math.ceil(wait / 1000) };
    }
    let timer: Timer | undefined;
    const late = new Promise<'late'>((r) => {
      timer = setTimeout(() => r('late'), this.limits.manualDeadlineMs);
    });
    const done = await Promise.race([this.compactNow(file), late]).finally(() => clearTimeout(timer));
    if (done === 'late') return { ok: false, retryAfter: 30 };
    if (!done) return { ok: false, retryAfter: 60 };
    this.manualAt.set(file, performance.now());
    return { ok: true };
  }

  private settle(file: string, ok: boolean): void {
    const list = this.waiters.get(file);
    this.waiters.delete(file);
    for (const w of list ?? []) w(ok);
  }

  enqueue(file: string): void {
    if (this.stopped) {
      this.settle(file, false);
      return;
    }
    if (!this.urgent.has(file)) this.queue.add(file);
    this.pump();
  }

  private pump(): void {
    if (this.stopped || this.running) return;
    const from = this.urgent.size > 0 ? this.urgent : this.queue;
    if (from.size === 0) return;
    const file = from.values().next().value as string;
    from.delete(file);
    // Метка «занято» ставится сразу: второй вызов `pump` до начала задачи не запустит вторую.
    this.running = { file, done: () => {}, timer: null };
    void this.run(file);
  }

  private spawn(): Worker {
    this.spawned++;
    const w = new Worker(new URL('./compact.worker.ts', import.meta.url));
    w.unref?.();
    return w;
  }

  private async run(file: string): Promise<void> {
    let ok = false;
    try {
      const due = await this.pool.query<{ waits: boolean }>('SELECT compact_after > now() AS waits FROM file WHERE id = $1', [file]);
      if (due.rows[0]?.waits || this.stopped) return;
      if (this.tasks >= this.limits.tasksPerWorker) this.dropWorker();
      this.worker ??= this.spawn();
      this.tasks++;
      const worker = this.worker;
      const id = ++this.seq;
      ok = await new Promise<boolean>((done) => {
        const timer = setTimeout(() => {
          console.error('Сжатие: файл', file, 'не уложился в срок');
          this.dropWorker();
          void this.failed(file).finally(() => done(false));
        }, this.limits.jobDeadlineMs);
        timer.unref?.();
        this.running = { file, done: () => done(false), timer };
        worker.onmessage = (e: MessageEvent<CompactReply>) => {
          if (e.data.id !== id) return;
          clearTimeout(timer);
          void this.finish(file, e.data).then(done, (err: unknown) => {
            report(err);
            done(false);
          });
        };
        worker.onerror = (e) => {
          e.preventDefault?.();
          clearTimeout(timer);
          console.error('Сжатие: worker упал на файле', file, e.message);
          this.dropWorker();
          void this.finish(file, { id, ok: false, message: e.message }).then(done, () => done(false));
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
      this.settle(file, ok);
      this.pump();
    }
  }

  /** Итог задачи; `true` — файл собран целиком (документы, которые не собрались, названы в журнале сервера). */
  private async finish(file: string, r: CompactReply): Promise<boolean> {
    this.onResult(file, r);
    if (r.ok) {
      const failed = r.result.kind === 'done' || r.result.kind === 'failed' ? r.result.failed : [];
      for (const f of failed) {
        console.error(
          f.seq > 0
            ? `Сжатие: файл ${file}, документ ${f.doc}: не применилось обновление ${f.seq}. Снимок документа не заменён, журнал цел`
            : `Сжатие: файл ${file}, документ ${f.doc} больше предела. Снимок документа не заменён, журнал цел`,
        );
      }
      if (r.result.kind === 'done' || r.result.kind === 'empty') return true;
      return false;
    }
    console.error(`Сжатие: файл ${file}:`, r.message);
    // Паника wasm портит модуль: следующий файл — на новом worker'е. Документ ждёт срока, остальные документы — следующей задачей.
    this.dropWorker();
    if (r.doc !== undefined) {
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
    return false;
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

  stop(): void {
    this.stopped = true;
    this.urgent.clear();
    this.queue.clear();
    clearInterval(this.timer);
    if (this.running) {
      if (this.running.timer) clearTimeout(this.running.timer);
      this.running.done();
    }
    for (const file of this.waiters.keys()) this.settle(file, false);
    this.dropWorker();
  }
}

function report(e: unknown): void {
  console.error('Сжатие:', e instanceof Error ? e.message : e);
}
