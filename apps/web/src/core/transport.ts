// Транспорт между основным потоком и worker'ом. Две реализации, выбор
// автоматический: SharedArrayBuffer + postMessage, если страница изолирована
// (`crossOriginIsolated`), иначе только postMessage. Команды и события всегда идут
// сообщениями; ввод и живое состояние — через общую память или в сообщениях кадра.

import { Ctrl, CTRL_BYTES, LIVE_OFFSET, LIVE_SLOTS, RECORD_SIZE, RING_CAPACITY, RING_OFFSET, SAB_BYTES } from './layout.gen';
import type { Sink } from './input';
import { commandSizeError, type FromWorker, type ToWorker } from './messages';
import { LiveReader } from './shared';

export type TransportMode = 'sab' | 'message';

export interface Transport {
  readonly mode: TransportMode;
  /** Кладёт одну запись ввода. */
  readonly sink: Sink;
  /** Кадр основного потока (`requestAnimationFrame`): разбудить worker. */
  tick(): void;
  /** Последнее живое состояние или `null`, если его ещё не было. */
  live(): Float64Array | null;
  /** Сколько событий ввода пришлось выбросить (очередь переполнилась). */
  dropped(): number;
  /** Шлёт сообщение worker'у. Команда с байтами больше предела не уходит —
   * возвращается текст ошибки для пользователя; иначе `null`. */
  post(msg: ToWorker, transfer?: Transferable[]): string | null;
  /** Сообщения worker'а, кроме служебных сообщений самого транспорта. */
  onMessage(cb: (msg: FromWorker) => void): void;
  /** Общая память для worker'а (только режим SAB). */
  readonly sab: SharedArrayBuffer | null;
}

/** Сколько записей ждать в очереди основного потока, пока кольцо полно. */
const PENDING_MAX = 4 * RING_CAPACITY;

export class SabTransport implements Transport {
  readonly mode = 'sab' as const;
  readonly sab = new SharedArrayBuffer(SAB_BYTES);
  private readonly ctrl = new Int32Array(this.sab, 0, CTRL_BYTES / 4);
  private readonly liveView = new Float64Array(this.sab, LIVE_OFFSET, LIVE_SLOTS);
  private readonly ring = new DataView(this.sab, RING_OFFSET, RING_CAPACITY * RECORD_SIZE);
  private readonly pending = new DataView(new ArrayBuffer(PENDING_MAX * RECORD_SIZE));
  private pendingCount = 0;
  private readonly reader = new LiveReader(this.ctrl, this.liveView);
  /** Worker без `Atomics.waitAsync` (Firefox) будится сообщением. */
  needsTickMessages = false;
  /** Версия живого состояния при последнем тике-сообщении: пока worker не
   * записал новое состояние, прошлый тик не обработан и новый не шлём. */
  private tickSeq = -1;
  private listener: ((msg: FromWorker) => void) | null = null;

  constructor(private readonly worker: Worker) {
    worker.onmessage = (e: MessageEvent<FromWorker>) => {
      if (e.data.t === 'started') this.needsTickMessages = !e.data.waitAsync;
      this.listener?.(e.data);
    };
  }

  readonly sink: Sink = (fill) => {
    if (this.pendingCount > 0 || !this.tryWrite(fill)) {
      // Кольцо полно (worker занят, например открывает документ): запись ждёт в
      // очереди и уходит следующим кадром; порядок событий сохраняется.
      if (this.pendingCount === PENDING_MAX) {
        Atomics.add(this.ctrl, Ctrl.INPUT_DROPPED, 1);
        return;
      }
      fill(this.pending, this.pendingCount * RECORD_SIZE);
      this.pendingCount++;
    }
  };

  private tryWrite(fill: (view: DataView, at: number) => void): boolean {
    const w = Atomics.load(this.ctrl, Ctrl.INPUT_WRITE);
    const r = Atomics.load(this.ctrl, Ctrl.INPUT_READ);
    // Беззнаково: счётчики — int32 по модулю 2^32, разность не должна уходить в минус.
    if (((w - r) >>> 0) >= RING_CAPACITY) return false;
    fill(this.ring, ((w >>> 0) % RING_CAPACITY) * RECORD_SIZE);
    // Запись видна worker'у не раньше нового значения счётчика.
    Atomics.store(this.ctrl, Ctrl.INPUT_WRITE, (w + 1) | 0);
    return true;
  }

  private flushPending(): void {
    let sent = 0;
    while (sent < this.pendingCount) {
      const from = sent * RECORD_SIZE;
      const ok = this.tryWrite((view, at) => {
        for (let i = 0; i < RECORD_SIZE; i++) view.setUint8(at + i, this.pending.getUint8(from + i));
      });
      if (!ok) break;
      sent++;
    }
    if (sent > 0) {
      const left = this.pendingCount - sent;
      const bytes = new Uint8Array(this.pending.buffer);
      bytes.copyWithin(0, sent * RECORD_SIZE, this.pendingCount * RECORD_SIZE);
      this.pendingCount = left;
    }
  }

  tick(): void {
    if (this.pendingCount > 0) this.flushPending();
    Atomics.add(this.ctrl, Ctrl.FRAME, 1);
    Atomics.notify(this.ctrl, Ctrl.FRAME);
    if (this.needsTickMessages) {
      const seq = Atomics.load(this.ctrl, Ctrl.LIVE_SEQ);
      if (seq !== this.tickSeq) {
        this.tickSeq = seq;
        this.worker.postMessage({ t: 'tick', input: null } satisfies ToWorker);
      }
    }
  }

  live(): Float64Array | null {
    return this.reader.read();
  }

  dropped(): number {
    return Atomics.load(this.ctrl, Ctrl.INPUT_DROPPED);
  }

  post(msg: ToWorker, transfer: Transferable[] = []): string | null {
    const error = msg.t === 'cmd' ? commandSizeError(msg.cmd) : null;
    if (error) return error;
    this.worker.postMessage(msg, transfer);
    return null;
  }

  onMessage(cb: (msg: FromWorker) => void): void {
    this.listener = cb;
  }
}

export class MessageTransport implements Transport {
  readonly mode = 'message' as const;
  readonly sab = null;
  private buf = new ArrayBuffer(256 * RECORD_SIZE);
  private view = new DataView(this.buf);
  private count = 0;
  private lastLive: Float64Array | null = null;
  /** Кадр отправлен, а живое состояние в ответ ещё не пришло: следующий тик ждёт —
   * иначе медленный worker копил бы очередь кадров. */
  private inFlight = false;
  private dropCount = 0;
  private listener: ((msg: FromWorker) => void) | null = null;

  constructor(private readonly worker: Worker) {
    worker.onmessage = (e: MessageEvent<FromWorker>) => {
      if (e.data.t === 'live') {
        this.lastLive = e.data.live;
        this.inFlight = false;
        return;
      }
      this.listener?.(e.data);
    };
  }

  readonly sink: Sink = (fill) => {
    if (this.count * RECORD_SIZE === this.buf.byteLength) {
      if (this.count >= PENDING_MAX) {
        this.dropCount++;
        return;
      }
      const bigger = new ArrayBuffer(this.buf.byteLength * 2);
      new Uint8Array(bigger).set(new Uint8Array(this.buf));
      this.buf = bigger;
      this.view = new DataView(bigger);
    }
    fill(this.view, this.count * RECORD_SIZE);
    this.count++;
  };

  tick(): void {
    if (this.inFlight) return;
    const input = this.buf.slice(0, this.count * RECORD_SIZE);
    this.count = 0;
    this.inFlight = true;
    this.worker.postMessage({ t: 'tick', input } satisfies ToWorker, [input]);
  }

  live(): Float64Array | null {
    return this.lastLive;
  }

  dropped(): number {
    return this.dropCount;
  }

  post(msg: ToWorker, transfer: Transferable[] = []): string | null {
    const error = msg.t === 'cmd' ? commandSizeError(msg.cmd) : null;
    if (error) return error;
    this.worker.postMessage(msg, transfer);
    return null;
  }

  onMessage(cb: (msg: FromWorker) => void): void {
    this.listener = cb;
  }
}

/** Транспорт для worker'а: SAB, если страница изолирована и это не запрещено. */
export function createTransport(worker: Worker, force?: TransportMode): Transport {
  const sab = force !== 'message' && globalThis.crossOriginIsolated === true && typeof SharedArrayBuffer === 'function';
  return sab ? new SabTransport(worker) : new MessageTransport(worker);
}
