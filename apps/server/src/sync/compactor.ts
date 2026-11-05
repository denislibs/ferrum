// Когда сжимать (спека подсистемы 4, раздел 7.4): больше 2000 записей или 32 МБ журнала документа, либо через 5 минут после ухода
// последнего участника. Файлы — по одному; worker пересоздаётся каждые 50 задач. Не вышло — журнал цел, строка в журнале сервера,
// следующая попытка через час.

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
}

export const COMPACT_LIMITS: CompactLimits = {
  maxLogCount: 2000,
  maxLogBytes: 32 * 1024 * 1024,
  idleMs: 5 * 60_000,
  tickMs: 30_000,
  retryAfterSeconds: 3600,
  tasksPerWorker: 50,
  jobDeadlineMs: 10 * 60_000,
};

export class Compactor {
  private worker: Worker | null = null;
  private tasks = 0;
  private seq = 0;
  private running: { id: number; file: string; done: () => void; timer: Timer } | null = null;
  private readonly queue = new Set<string>();
  private readonly waiters = new Map<string, ((ok: boolean) => void)[]>();
  private readonly leftAt = new Map<string, number>();
  private readonly timer: Timer;
  private stopped = false;
  private readonly limits: CompactLimits;
  /** Занят ли файл людьми: сжатие простаивающих не трогает открытые. */
  active: (file: string) => boolean = () => false;
  /** Тесты: итог каждой задачи. */
  onResult: (file: string, r: CompactReply) => void = () => {};

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

  /** Файлы с журналом без людей дольше срока, в том числе от прошлого запуска. */
  private async tick(): Promise<void> {
    if (this.stopped) return;
    const { rows } = await this.pool.query<{ file_id: string }>(
      `SELECT DISTINCT s.file_id FROM doc_state s JOIN file f ON f.id = s.file_id
       WHERE s.log_count > 0 AND (f.compact_after IS NULL OR f.compact_after <= now()) LIMIT 100`,
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
      this.enqueue(file);
    });
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
    this.queue.add(file);
    this.pump();
  }

  private pump(): void {
    if (this.running || this.queue.size === 0) return;
    const file = this.queue.values().next().value as string;
    this.queue.delete(file);
    void this.run(file).catch(report);
  }

  private spawn(): Worker {
    const w = new Worker(new URL('./compact.worker.ts', import.meta.url));
    w.unref?.();
    return w;
  }

  private async run(file: string): Promise<void> {
    const due = await this.pool.query<{ waits: boolean }>('SELECT compact_after > now() AS waits FROM file WHERE id = $1', [file]);
    if (due.rows[0]?.waits) {
      this.settle(file, false);
      this.pump();
      return;
    }
    if (this.tasks >= this.limits.tasksPerWorker) this.dropWorker();
    this.worker ??= this.spawn();
    this.tasks++;
    const worker = this.worker;
    const id = ++this.seq;
    await new Promise<void>((done) => {
      const timer = setTimeout(() => {
        console.error('Сжатие: файл', file, 'не уложился в срок');
        this.dropWorker();
        this.settle(file, false);
        void this.failed(file).finally(done);
      }, this.limits.jobDeadlineMs);
      timer.unref?.();
      this.running = { id, file, done, timer };
      worker.onmessage = (e: MessageEvent<CompactReply>) => {
        if (e.data.id !== id) return;
        clearTimeout(timer);
        this.finish(file, e.data).finally(done);
      };
      worker.onerror = (e) => {
        e.preventDefault?.();
        clearTimeout(timer);
        console.error('Сжатие: worker упал на файле', file, e.message);
        this.dropWorker();
        const reply: CompactReply = { id, ok: false, message: e.message };
        this.finish(file, reply).finally(done);
      };
      worker.postMessage({ id, file, init: this.init } satisfies CompactJob & { init: CompactInit });
    });
    this.running = null;
    this.pump();
  }

  private async finish(file: string, r: CompactReply): Promise<void> {
    this.onResult(file, r);
    this.settle(file, r.ok && (r.result.kind === 'done' || r.result.kind === 'empty'));
    if (r.ok && (r.result.kind === 'done' || r.result.kind === 'empty' || r.result.kind === 'raced')) return;
    if (r.ok && r.result.kind === 'failed') {
      console.error(
        `Сжатие: файл ${file}, документ ${r.result.doc}: не применилось обновление ${r.result.seq}. Снимок не заменён, журнал цел`,
      );
    } else if (!r.ok) {
      console.error(`Сжатие: файл ${file}:`, r.message);
    }
    await this.failed(file);
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

  stop(): void {
    this.stopped = true;
    for (const file of this.waiters.keys()) this.settle(file, false);
    clearInterval(this.timer);
    if (this.running) {
      clearTimeout(this.running.timer);
      this.running.done();
    }
    this.dropWorker();
  }
}

function report(e: unknown): void {
  console.error('Сжатие:', e instanceof Error ? e.message : e);
}
